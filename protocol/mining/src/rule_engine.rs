use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use kaspa_consensus_core::{
    api::counters::ProcessingCounters,
    config::Config,
    daa_score_timestamp::DaaScoreTimestamp,
    mining_rules::MiningRules,
    network::NetworkType::{Mainnet, Testnet},
};
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::{
    info,
    task::{
        service::{AsyncService, AsyncServiceFuture},
        tick::{TickReason, TickService},
    },
    time::unix_now,
    trace,
};
use kaspa_p2p_lib::Hub;

use crate::rules::{ExtraData, mining_rule::MiningRule, sync_rate_rule::SyncRateRule};

const RULE_ENGINE: &str = "mining-rule-engine";
pub const SNAPSHOT_INTERVAL: u64 = 10;

/// How long a node withholds participation after an IBD completes.
///
/// IBD picks its syncer by arrival order — whichever peer relays first takes the latch — and
/// nothing downstream re-examines that choice. A node that starts mining and attesting the
/// instant IBD returns therefore converts an arbitrary pick into support: it mines onto that
/// branch, its validator attests that branch's anchor, and once a branch-local DNS anchor forms
/// the reorg gate refuses the alternative. That is the step where a transient fork stops being
/// able to heal, and it is the step this window interrupts.
///
/// The window is sized for the machinery that can still catch the mistake. Once the latch is
/// released, the relay guard no longer discards competing offers: a heavier peer's block is
/// requested, lands as an orphan, and orphan resolution starts a fresh IBD. Three minutes covers
/// a relay round-trip, orphan resolution, and the start of a re-sync, without stalling a node
/// that simply restarted on the majority branch.
///
/// This is a delay, not a decision. It does not choose a chain and does not detect a wrong one —
/// it only stops the node from committing infrastructure to a chain nobody compared.
pub const POST_IBD_PROBATION: Duration = Duration::from_secs(180);

/// Bounded refusal to mine or report ourselves synced after an IBD, per [`POST_IBD_PROBATION`].
///
/// Deliberately dependency-free (`now` is a parameter, not a call) so the expiry arithmetic is
/// testable without a consensus manager, a hub, or a clock.
#[derive(Debug, Default)]
struct PostIbdProbation {
    /// Unix-ms deadline; `0` means no probation has ever been armed.
    until_ms: AtomicU64,
    /// Whether probation applies on this network at all — see [`MiningRuleEngine::new`].
    enabled: bool,
}

impl PostIbdProbation {
    fn new(enabled: bool) -> Self {
        Self { until_ms: AtomicU64::new(0), enabled }
    }

    /// Arm (or extend) the window, returning how long the node will now hold back.
    ///
    /// Uses `fetch_max` so a second IBD finishing inside an existing window can only push the
    /// deadline out. Two IBDs back to back is more reason to wait, not less, and IBD flows run
    /// one per peer — a shorter later window must not release the node early.
    fn begin(&self, now_ms: u64, duration: Duration) -> Option<Duration> {
        if !self.enabled {
            return None;
        }
        let deadline = now_ms.saturating_add(duration.as_millis() as u64);
        let previous = self.until_ms.fetch_max(deadline, Ordering::Relaxed);
        Some(Duration::from_millis(deadline.max(previous).saturating_sub(now_ms)))
    }

    fn remaining(&self, now_ms: u64) -> Option<Duration> {
        let until = self.until_ms.load(Ordering::Relaxed);
        (until > now_ms).then(|| Duration::from_millis(until - now_ms))
    }

    fn is_active(&self, now_ms: u64) -> bool {
        self.remaining(now_ms).is_some()
    }
}

#[derive(Clone)]
pub struct MiningRuleEngine {
    config: Arc<Config>,
    processing_counters: Arc<ProcessingCounters>,
    tick_service: Arc<TickService>,
    // Sync Rate Rule: Allow mining if sync rate is below threshold AND finality point is "recent" (defined below)
    use_sync_rate_rule: Arc<AtomicBool>,
    consensus_manager: Arc<ConsensusManager>,
    hub: Hub,
    mining_rules: Arc<MiningRules>,
    rules: Vec<Arc<dyn MiningRule>>,
    post_ibd_probation: Arc<PostIbdProbation>,
}

impl MiningRuleEngine {
    pub async fn worker(self: &Arc<MiningRuleEngine>) {
        let mut last_snapshot = self.processing_counters.snapshot();
        let mut last_log_time = Instant::now();
        loop {
            // START: Sync monitor
            if let TickReason::Shutdown = self.tick_service.tick(Duration::from_secs(SNAPSHOT_INTERVAL)).await {
                // Let the system print final logs before exiting
                tokio::time::sleep(Duration::from_millis(500)).await;
                break;
            }

            let now = Instant::now();
            let elapsed_time = now - last_log_time;
            if elapsed_time.as_secs() == 0 {
                continue;
            }

            let snapshot = self.processing_counters.snapshot();

            // Subtract the snapshots
            let delta = &snapshot - &last_snapshot;

            if elapsed_time.as_secs() > 0 {
                let session = self.consensus_manager.consensus().unguarded_session();

                let finality_point = session.async_finality_point().await;
                let finality_point_timestamp = session.async_get_header(finality_point).await.unwrap().timestamp;

                let extra_data = ExtraData {
                    finality_point_timestamp,
                    target_time_per_block: self.config.target_time_per_block(),
                    has_sufficient_peer_connectivity: self.has_sufficient_peer_connectivity(),
                    finality_duration: self.config.finality_duration_in_milliseconds(),
                    elapsed_time,
                };

                trace!("Current Mining Rule: {:?}", self.mining_rules);

                // Check for all the rules
                for rule in &self.rules {
                    rule.check_rule(&delta, &extra_data);
                }
            }

            // A node at tip that reports itself unsynced looks broken. Say why, on every tick of
            // the window, so an operator seeing an idle miner or a `NodeNotSynced` validator has
            // the reason in front of them rather than having to infer it.
            if let Some(remaining) = self.post_ibd_probation_remaining() {
                info!("Post-IBD probation: {}s before this node will mine or report itself synced", remaining.as_secs());
            }

            last_snapshot = snapshot;
            last_log_time = now;
        }
    }

    pub fn new(
        consensus_manager: Arc<ConsensusManager>,
        config: Arc<Config>,
        processing_counters: Arc<ProcessingCounters>,
        tick_service: Arc<TickService>,
        hub: Hub,
        mining_rules: Arc<MiningRules>,
    ) -> Self {
        let use_sync_rate_rule = Arc::new(AtomicBool::new(false));
        let rules: Vec<Arc<dyn MiningRule + 'static>> = vec![Arc::new(SyncRateRule::new(use_sync_rate_rule.clone()))];

        // Scoped to the networks that have peers to be wrong about, mirroring
        // `has_sufficient_peer_connectivity`. A peerless devnet/simnet node has no competing branch
        // to overlook and no way to shorten the wait, so probation there would only stall tests.
        let post_ibd_probation = Arc::new(PostIbdProbation::new(matches!(config.net.network_type, Mainnet | Testnet)));

        Self {
            consensus_manager,
            config,
            processing_counters,
            tick_service,
            hub,
            use_sync_rate_rule,
            mining_rules,
            rules,
            post_ibd_probation,
        }
    }

    /// Withhold mining and the synced flag for [`POST_IBD_PROBATION`], returning how long the node
    /// will hold back (`None` where probation does not apply). Called when an IBD completes.
    ///
    /// See [`POST_IBD_PROBATION`] for why the window exists.
    pub fn begin_post_ibd_probation(&self) -> Option<Duration> {
        self.post_ibd_probation.begin(unix_now(), POST_IBD_PROBATION)
    }

    /// Time left in the post-IBD window, if any. For operator-facing reporting.
    pub fn post_ibd_probation_remaining(&self) -> Option<Duration> {
        self.post_ibd_probation.remaining(unix_now())
    }

    pub fn should_mine(&self, sink_daa_score_timestamp: DaaScoreTimestamp) -> bool {
        // Checked ahead of the sync-rate rule, which it deliberately overrides: that rule exists to
        // keep a node mining when the network is slow, and answers "is my chain moving?". Probation
        // answers "is my chain the one anybody else is on?" — an unresolved doubt there is not
        // something a healthy block rate can settle.
        if self.post_ibd_probation.is_active(unix_now()) {
            return false;
        }

        if !self.has_sufficient_peer_connectivity() {
            return false;
        }

        let is_nearly_synced = self.is_nearly_synced(sink_daa_score_timestamp);

        is_nearly_synced || self.use_sync_rate_rule.load(Ordering::Relaxed)
    }

    /// In non-mining contexts, we consider the node synced if the sink is recent and it is connected
    /// to a peer
    ///
    /// Backs the `is_synced` flag on `getInfo` / `getServerInfo` / `getSyncStatus`, which is what
    /// the validator service polls before it will attest (`kaspa-pq-validator`: `ValidatorStatus::
    /// NodeNotSynced`). Reporting `false` during post-IBD probation is what keeps a freshly synced
    /// node from signing an anchor on a branch it adopted by arrival order.
    pub fn is_sink_recent_and_connected(&self, sink_daa_score_timestamp: DaaScoreTimestamp) -> bool {
        !self.post_ibd_probation.is_active(unix_now())
            && self.has_sufficient_peer_connectivity()
            && self.is_nearly_synced(sink_daa_score_timestamp)
    }

    /// Returns whether the sink timestamp is recent enough and the node is considered synced or nearly synced.
    ///
    /// This info is used to determine if it's ok to use a block template from this node for mining purposes.
    pub fn is_nearly_synced(&self, sink_daa_score_timestamp: DaaScoreTimestamp) -> bool {
        let sink_timestamp = sink_daa_score_timestamp.timestamp;

        // We consider the node close to being synced if the sink (virtual selected parent) block
        // timestamp is within a quarter of the DAA window duration far in the past. Blocks mined over such DAG state would
        // enter the DAA window of fully-synced nodes and thus contribute to overall network difficulty
        let synced_threshold = self.config.expected_difficulty_window_duration_in_milliseconds() / 4;

        // Roughly 10mins in all networks
        unix_now() < sink_timestamp + synced_threshold
    }

    fn has_sufficient_peer_connectivity(&self) -> bool {
        // Other network types can be used in an isolated environment without peers
        !matches!(self.config.net.network_type, Mainnet | Testnet) || self.hub.has_peers()
    }
}

impl AsyncService for MiningRuleEngine {
    fn ident(self: Arc<Self>) -> &'static str {
        RULE_ENGINE
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            self.worker().await;
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", RULE_ENGINE);
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            trace!("{} stopped", RULE_ENGINE);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_754_600_000_000;

    #[test]
    fn probation_is_inactive_until_an_ibd_arms_it() {
        // A node that restarts without ever running IBD never adopted anyone's chain, so it has
        // nothing to hold back for.
        let probation = PostIbdProbation::new(true);
        assert!(!probation.is_active(NOW));
        assert_eq!(probation.remaining(NOW), None);
    }

    #[test]
    fn probation_holds_for_the_full_window_then_releases() {
        let probation = PostIbdProbation::new(true);
        assert_eq!(probation.begin(NOW, POST_IBD_PROBATION), Some(POST_IBD_PROBATION));

        assert!(probation.is_active(NOW));
        assert!(probation.is_active(NOW + POST_IBD_PROBATION.as_millis() as u64 - 1));
        // Expiry is a release, not a decision: the node resumes because the window elapsed, which
        // is exactly why the window must be short enough not to matter and long enough to notice.
        assert!(!probation.is_active(NOW + POST_IBD_PROBATION.as_millis() as u64));
    }

    #[test]
    fn a_second_ibd_extends_the_window_and_never_shortens_it() {
        let probation = PostIbdProbation::new(true);
        probation.begin(NOW, POST_IBD_PROBATION);

        // A shorter window landing mid-flight must not release the node early — back-to-back IBDs
        // mean the chain was re-picked, which is more doubt, not less.
        let stale = probation.begin(NOW + 1_000, Duration::from_secs(1));
        assert_eq!(stale, Some(POST_IBD_PROBATION - Duration::from_secs(1)));
        assert!(probation.is_active(NOW + POST_IBD_PROBATION.as_millis() as u64 - 1));

        // A later IBD pushes the deadline out past where the first one would have expired.
        probation.begin(NOW + 60_000, POST_IBD_PROBATION);
        assert!(probation.is_active(NOW + POST_IBD_PROBATION.as_millis() as u64 + 30_000));
    }

    #[test]
    fn probation_is_a_no_op_where_it_does_not_apply() {
        // Peerless networks (devnet/simnet) have no competing branch to overlook.
        let probation = PostIbdProbation::new(false);
        assert_eq!(probation.begin(NOW, POST_IBD_PROBATION), None);
        assert!(!probation.is_active(NOW));
    }
}
