//! **The PALW lane watch: is this chain doing PALW work, or only ticking its clock?**
//!
//! The 2026-09-23 route-matrix audit read testnet-12 from genesis: 1,230 blocks, every one of them a
//! heartbeat, not one attempt — while every liveness signal an operator looked at was green. "Blocks
//! are arriving" and "the DAA is advancing" are both true of a chain that carries no PALW work at
//! all, because the heartbeat lane exists precisely to keep the clock moving when nothing else does.
//! That is what hid a dead attempt lane for a whole deployment.
//!
//! So this service counts LANES, not blocks: once a minute it walks the selected chain back from the
//! sink over [`PALW_LANE_WATCH_WINDOW_BLOCKS`] blocks, classifies each by its header's `pow_algo_id`,
//! counts the ADR-0125 round blocks each of them merged, publishes the mix through
//! `getPalwNodeStatus`, and logs an ERROR while the newest [`PALW_LANE_WATCH_MIN_BLOCKS_TO_ALARM`]
//! selected-chain blocks carry no PALW work. It runs on every ConsensusV2 node, producing or not — a
//! seeder host or an explorer backend is exactly where an operator reads the chain's health — and
//! judges only once the node is nearly synced: a node catching up is reading history, not the chain.
//!
//! **Round blocks are counted from mergesets, and they are not work** (the route-matrix re-audit's
//! #9). An algo-10 round block is never a selected parent, never blue and never the sink, so a walk
//! of the selected chain alone never meets one; each visited chain block's red mergeset is where they
//! are. They stay out of the alarm: permits from earlier Finals keep minting round blocks for up to
//! the ~1,200-DAA maturity after the attempt lane stops, so counting them would mask a dead attempt
//! lane. Algo 9 is not the execution lane either — it is ADR-0072's execution-priced ATTEMPT lane,
//! armed on no shipped preset; it is a chain block carrying an attempt, so it is work when it exists.

use std::sync::Arc;
use std::time::Duration;

use kaspa_consensus_core::pow_layer0::{
    POW_ALGO_ID_HEARTBEAT_V1, POW_ALGO_ID_PALW_COMMITTED_V2, POW_ALGO_ID_PALW_EXEC_V3, POW_ALGO_ID_PALW_RECEIPT_V3,
    POW_ALGO_ID_PALW_ROUND_V1,
};
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::{
    error, info,
    task::service::{AsyncService, AsyncServiceFuture},
    trace,
};
use kaspa_p2p_flows::flow_context::FlowContext;

const PALW_LANE_WATCH: &str = "palw-lane-watch";
/// How often the mix is taken.
const PALW_LANE_WATCH_PERIOD: Duration = Duration::from_secs(60);
/// How many selected-chain blocks the mix is taken over — twenty hours of blocks at the frozen 120 s
/// cadence of heartbeats (600 × 120 s), far fewer hours where the attempt lane is producing. The
/// window is the MIX's; the alarm reads recency ([`PALW_LANE_WATCH_MIN_BLOCKS_TO_ALARM`]).
pub(crate) const PALW_LANE_WATCH_WINDOW_BLOCKS: usize = 600;
/// **The alarm's horizon: this many of the newest selected-chain blocks without a PALW work block**
/// — an hour at the 120 s heartbeat cadence a chain falls back to when its attempt lane stops
/// (the route-matrix re-audit's #10). The alarm used to need the whole 600-block window empty, so a
/// lane that died on a running chain was reported ~20 hours later (600 × 120 s), not the hour and a
/// quarter the window's doc promised. It is also the young-chain guard: a window shorter than this
/// is a chain too young to judge (a fresh genesis's first hour), and gets the mix without an alarm.
pub(crate) const PALW_LANE_WATCH_MIN_BLOCKS_TO_ALARM: u64 = 30;
/// While the alarm stands it is repeated this often, so an operator scrolling a log still meets it.
const PALW_LANE_WATCH_REPEAT: Duration = Duration::from_secs(600);

/// **The selected chain's last blocks, by lane**, and the round blocks they merged.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PalwLaneMixV1 {
    /// Selected-chain blocks in the window.
    pub window_blocks: u64,
    /// `pow_algo_id` 6: a PALW attempt — a model or floor inference won a class ticket.
    pub attempt_blocks: u64,
    /// 7: a certified free-prompt quantum, spent.
    pub receipt_blocks: u64,
    /// 9: an execution-PRICED ATTEMPT (ADR-0072's second attempt lane) — armed on no shipped preset,
    /// so zero on every network today. It is not the ADR-0125 execution lane; that is `round_blocks`.
    pub exec_priced_attempt_blocks: u64,
    /// 10: ADR-0125 round blocks (the execution lane) MERGED by the window's chain blocks — found in
    /// their red mergesets, since a round block is never on the selected chain. Not work: see the
    /// module doc.
    pub round_blocks: u64,
    /// 8: the clock, and nothing else.
    pub heartbeat_blocks: u64,
    /// Anything else (the genesis, a legacy lane).
    pub other_blocks: u64,
    /// The DAA of the newest block in the window that carried PALW work, if any did.
    pub last_work_daa: Option<u64>,
    /// How many selected-chain blocks are newer than the newest work block — the whole window when
    /// none carried work. What the alarm reads.
    pub blocks_since_work: u64,
    /// The DAA span the window covers, oldest first.
    pub from_daa: u64,
    pub to_daa: u64,
}

impl PalwLaneMixV1 {
    /// Selected-chain blocks that carried PALW work — an attempt of either lane or a receipt spend.
    /// Not the clock, not the genesis, and not the round blocks merged beside them.
    pub fn work_blocks(&self) -> u64 {
        self.attempt_blocks + self.receipt_blocks + self.exec_priced_attempt_blocks
    }

    /// Count one selected-chain block — called newest first, as the walk visits them.
    fn count(&mut self, pow_algo_id: u8, daa_score: u64) {
        let lane = match pow_algo_id {
            POW_ALGO_ID_PALW_COMMITTED_V2 => &mut self.attempt_blocks,
            POW_ALGO_ID_PALW_RECEIPT_V3 => &mut self.receipt_blocks,
            POW_ALGO_ID_PALW_EXEC_V3 => &mut self.exec_priced_attempt_blocks,
            POW_ALGO_ID_HEARTBEAT_V1 => &mut self.heartbeat_blocks,
            _ => &mut self.other_blocks,
        };
        *lane += 1;
        let works = matches!(pow_algo_id, POW_ALGO_ID_PALW_COMMITTED_V2 | POW_ALGO_ID_PALW_RECEIPT_V3 | POW_ALGO_ID_PALW_EXEC_V3);
        if works && self.last_work_daa.is_none() {
            // The first work block the newest-first walk meets is the newest one.
            self.last_work_daa = Some(daa_score);
            self.blocks_since_work = self.window_blocks;
        }
        self.window_blocks += 1;
        if self.last_work_daa.is_none() {
            self.blocks_since_work = self.window_blocks;
        }
    }

    /// Count the round blocks one selected-chain block merged.
    fn count_merged_rounds(&mut self, merged_round_blocks: u64) {
        self.round_blocks += merged_round_blocks;
    }

    /// One line an operator can read:
    /// `attempt=… receipt=… exec-priced-attempt=… heartbeat=… other=… | merged round blocks=…`.
    pub fn summary(&self) -> String {
        format!(
            "{} blocks (DAA {}..{}): attempt={} receipt={} exec-priced-attempt={} heartbeat={} other={} | merged round blocks={}",
            self.window_blocks,
            self.from_daa,
            self.to_daa,
            self.attempt_blocks,
            self.receipt_blocks,
            self.exec_priced_attempt_blocks,
            self.heartbeat_blocks,
            self.other_blocks,
            self.round_blocks
        )
    }

    /// **The alarm**: a window old enough to judge whose newest [`PALW_LANE_WATCH_MIN_BLOCKS_TO_ALARM`]
    /// selected-chain blocks carry no PALW work. `None` otherwise.
    pub fn alarm(&self) -> Option<String> {
        (self.window_blocks >= PALW_LANE_WATCH_MIN_BLOCKS_TO_ALARM && self.blocks_since_work >= PALW_LANE_WATCH_MIN_BLOCKS_TO_ALARM)
            .then(|| {
                let last = match self.last_work_daa {
                    Some(daa) => format!("the newest work block is at DAA {daa}, {} selected-chain blocks back", self.blocks_since_work),
                    None => format!("none in the whole window of {} (DAA {}..{})", self.window_blocks, self.from_daa, self.to_daa),
                };
                format!(
                    "no PALW work block in the newest {} selected-chain blocks ({last}) — {} of the window's blocks are \
                     heartbeats: the chain is running on its clock alone. Check that producers are running for a class that \
                     can produce (the floor always can), that panel seats are up, and each producer's `holding:` line",
                    self.blocks_since_work, self.heartbeat_blocks
                )
            })
    }
}

/// Walk the selected chain back from the sink over at most `window` blocks, counting the round
/// blocks each one merged.
fn lane_mix_v1(consensus: &dyn kaspa_consensus_core::api::ConsensusApi, window: usize) -> PalwLaneMixV1 {
    let mut mix = PalwLaneMixV1::default();
    let mut cursor = consensus.get_sink();
    for _ in 0..window {
        let Ok(header) = consensus.get_header(cursor) else { break };
        if mix.window_blocks == 0 {
            mix.to_daa = header.daa_score;
        }
        mix.from_daa = header.daa_score;
        mix.count(header.pow_algo_id, header.daa_score);
        let Ok(ghostdag) = consensus.get_ghostdag_data(cursor) else { break };
        // A round block is red wherever it is merged; the blues are read too, so a rule change that
        // coloured one blue would not make the lane vanish from this count.
        let merged_rounds = ghostdag
            .mergeset_reds
            .iter()
            .chain(ghostdag.mergeset_blues.iter())
            .filter(|merged| consensus.get_header(**merged).is_ok_and(|h| h.pow_algo_id == POW_ALGO_ID_PALW_ROUND_V1))
            .count() as u64;
        mix.count_merged_rounds(merged_rounds);
        if ghostdag.selected_parent == cursor {
            break;
        }
        cursor = ghostdag.selected_parent;
    }
    mix
}

pub struct PalwLaneWatch {
    consensus_manager: Arc<ConsensusManager>,
    flow_context: Arc<FlowContext>,
    shutdown: kaspa_utils::triggers::SingleTrigger,
}

impl PalwLaneWatch {
    pub fn new(consensus_manager: Arc<ConsensusManager>, flow_context: Arc<FlowContext>) -> Self {
        Self { consensus_manager, flow_context, shutdown: kaspa_utils::triggers::SingleTrigger::default() }
    }

    async fn worker(self: &Arc<Self>) {
        info!(
            "[{PALW_LANE_WATCH}] counting the selected chain's last {PALW_LANE_WATCH_WINDOW_BLOCKS} blocks by lane every {} s; \
             an ERROR while the newest {PALW_LANE_WATCH_MIN_BLOCKS_TO_ALARM} carry no PALW work",
            PALW_LANE_WATCH_PERIOD.as_secs()
        );
        let mut alarmed_at: Option<std::time::Instant> = None;
        loop {
            let session = self.consensus_manager.consensus().unguarded_session();
            if !session.async_is_consensus_in_transitional_ibd_state().await {
                // **Judged only once the node is nearly synced** (the route-matrix re-audit's #12). During
                // an IBD or a catch-up the sink is an old block and the mix describes history: a
                // heartbeat-only stretch of it (on testnet-12, the chain's first hours) raised the ERROR
                // and `getPalwNodeStatus` served that stale alarm to the explorer. The mix is still
                // published; the alarm waits for the chain it would describe.
                let synced = self.flow_context.is_nearly_synced(&session).await;
                let mix = session.spawn_blocking(|c| lane_mix_v1(c, PALW_LANE_WATCH_WINDOW_BLOCKS)).await;
                let alarm = if synced { mix.alarm() } else { None };
                self.flow_context.update_palw_runtime(|r| {
                    r.lane_window_blocks = mix.window_blocks;
                    r.lane_work_blocks = mix.work_blocks();
                    r.lane_heartbeat_blocks = mix.heartbeat_blocks;
                    r.lane_last_work_daa = mix.last_work_daa.unwrap_or(0);
                    r.lane_mix = mix.summary();
                    r.lane_alarm = alarm.clone().unwrap_or_default();
                });
                match (&alarm, alarmed_at) {
                    // A node that fell behind says nothing either way until it has caught up.
                    _ if !synced => {}
                    (Some(why), None) => {
                        error!("[{PALW_LANE_WATCH}] {why}");
                        alarmed_at = Some(std::time::Instant::now());
                    }
                    (Some(why), Some(at)) if at.elapsed() >= PALW_LANE_WATCH_REPEAT => {
                        error!("[{PALW_LANE_WATCH}] still: {why}");
                        alarmed_at = Some(std::time::Instant::now());
                    }
                    (None, Some(_)) => {
                        info!("[{PALW_LANE_WATCH}] PALW work is back on the chain: {}", mix.summary());
                        alarmed_at = None;
                    }
                    _ => {}
                }
            }
            tokio::select! {
                _ = tokio::time::sleep(PALW_LANE_WATCH_PERIOD) => {}
                _ = self.shutdown.listener.clone() => break,
            }
        }
    }
}

impl AsyncService for PalwLaneWatch {
    fn ident(self: Arc<Self>) -> &'static str {
        PALW_LANE_WATCH
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            self.worker().await;
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", PALW_LANE_WATCH);
        self.shutdown.trigger.trigger();
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            trace!("{} stopped", PALW_LANE_WATCH);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mix_of(lanes: &[(u8, u64)]) -> PalwLaneMixV1 {
        let mut mix = PalwLaneMixV1::default();
        for &(algo, daa) in lanes {
            if mix.window_blocks == 0 {
                mix.to_daa = daa;
            }
            mix.from_daa = daa;
            mix.count(algo, daa);
        }
        mix
    }

    /// **The live testnet-12 of 2026-09-23, in miniature**: a chain of heartbeats alarms once it is old
    /// enough to judge, whatever its block count — the signal that "blocks are arriving" missed.
    #[test]
    fn a_heartbeat_only_window_alarms_and_a_young_one_does_not() {
        let heartbeats: Vec<(u8, u64)> = (0..600u64).rev().map(|d| (POW_ALGO_ID_HEARTBEAT_V1, d)).collect();
        let mix = mix_of(&heartbeats);
        assert_eq!((mix.window_blocks, mix.work_blocks(), mix.heartbeat_blocks), (600, 0, 600));
        assert_eq!(mix.blocks_since_work, 600, "no work block: the whole window");
        let alarm = mix.alarm().expect("600 heartbeats and no work is the alarm");
        assert!(alarm.contains("no PALW work block") && alarm.contains("600"), "{alarm}");
        let young = mix_of(&heartbeats[..10]);
        assert_eq!(young.alarm(), None, "ten blocks are too few to judge a chain by");
    }

    /// **The route-matrix re-audit's #10: the alarm reads recency, not window emptiness.** A lane that
    /// dies on a running chain leaves its last work block deep in the window; the alarm must fire
    /// once the newest 30 blocks (an hour of 120 s heartbeats) are clock-only, not 600 blocks (twenty
    /// hours) later — and must not fire while the newest work is younger than that.
    #[test]
    fn the_alarm_fires_an_hour_after_the_attempt_lane_dies_not_twenty() {
        // Newest first: `since` heartbeats, then an attempt, then 500 more attempts (a lane that ran).
        let chain = |since: u64| -> Vec<(u8, u64)> {
            let mut blocks: Vec<(u8, u64)> = Vec::new();
            let mut daa = 10_000u64;
            for _ in 0..since {
                blocks.push((POW_ALGO_ID_HEARTBEAT_V1, daa));
                daa -= 1;
            }
            for _ in 0..500 {
                blocks.push((POW_ALGO_ID_PALW_COMMITTED_V2, daa));
            }
            blocks
        };
        let died = mix_of(&chain(PALW_LANE_WATCH_MIN_BLOCKS_TO_ALARM));
        assert!(died.work_blocks() >= 500, "the window is still full of the lane's old work");
        assert_eq!(died.blocks_since_work, PALW_LANE_WATCH_MIN_BLOCKS_TO_ALARM);
        let alarm = died.alarm().expect("thirty clock-only blocks after the last attempt is the alarm");
        assert!(alarm.contains("the newest work block is at DAA"), "{alarm}");
        let recent = mix_of(&chain(PALW_LANE_WATCH_MIN_BLOCKS_TO_ALARM - 1));
        assert_eq!(recent.alarm(), None, "twenty-nine blocks since the last attempt is not yet an hour");
    }

    /// One selected-chain work block of any attempt or receipt lane clears it, and the newest one's
    /// DAA is the one reported. Round blocks never clear it (#9): they are merged, not chained, and a
    /// dead attempt lane's matured permits keep minting them.
    #[test]
    fn any_palw_work_block_clears_the_alarm_and_merged_round_blocks_do_not() {
        for lane in [POW_ALGO_ID_PALW_COMMITTED_V2, POW_ALGO_ID_PALW_RECEIPT_V3, POW_ALGO_ID_PALW_EXEC_V3] {
            let mut blocks: Vec<(u8, u64)> = (0..100u64).rev().map(|d| (POW_ALGO_ID_HEARTBEAT_V1, d)).collect();
            blocks[10] = (lane, 89);
            blocks[70] = (lane, 29);
            let mix = mix_of(&blocks);
            assert_eq!(mix.work_blocks(), 2, "lane {lane}");
            assert_eq!(mix.last_work_daa, Some(89), "the newest work block, lane {lane}");
            assert_eq!(mix.blocks_since_work, 10, "lane {lane}");
            assert_eq!(mix.alarm(), None, "lane {lane} clears the alarm");
        }
        let mut rounds_only = mix_of(&(0..100u64).rev().map(|d| (POW_ALGO_ID_HEARTBEAT_V1, d)).collect::<Vec<_>>());
        rounds_only.count_merged_rounds(120);
        assert_eq!((rounds_only.round_blocks, rounds_only.work_blocks()), (120, 0));
        assert!(rounds_only.alarm().is_some(), "merged round blocks do not stand in for a dead attempt lane");
        assert!(rounds_only.summary().contains("merged round blocks=120"), "{}", rounds_only.summary());
        let genesis_only = mix_of(&[(1, 0)]);
        assert_eq!((genesis_only.other_blocks, genesis_only.work_blocks()), (1, 0), "the genesis is not work");
    }

    /// **#9 through the walk itself**: the round blocks a chain block merged are counted from its
    /// mergeset, where they are — never from the selected chain, where they cannot be.
    #[test]
    fn the_walk_counts_the_round_blocks_each_chain_block_merged() {
        use kaspa_consensus_core::api::ConsensusApi;
        use kaspa_consensus_core::errors::consensus::{ConsensusError, ConsensusResult};
        use kaspa_consensus_core::header::Header;
        use kaspa_consensus_core::trusted::ExternalGhostdagData;
        use kaspa_consensus_core::{BlockHash, BlockHashMap};
        use std::collections::HashMap;
        struct Chain {
            sink: BlockHash,
            headers: HashMap<BlockHash, Arc<Header>>,
            ghostdag: HashMap<BlockHash, ExternalGhostdagData>,
        }
        impl ConsensusApi for Chain {
            fn get_sink(&self) -> BlockHash {
                self.sink
            }
            fn get_header(&self, hash: BlockHash) -> ConsensusResult<Arc<Header>> {
                self.headers.get(&hash).cloned().ok_or(ConsensusError::HeaderNotFound(hash))
            }
            fn get_ghostdag_data(&self, hash: BlockHash) -> ConsensusResult<ExternalGhostdagData> {
                self.ghostdag.get(&hash).cloned().ok_or(ConsensusError::HeaderNotFound(hash))
            }
        }
        let hash = |n: u64| BlockHash::from_u64_word(n);
        let header = |algo: u8, daa: u64| {
            let mut h = Header::from_precomputed_hash(hash(0), vec![]);
            h.pow_algo_id = algo;
            h.daa_score = daa;
            Arc::new(h)
        };
        let ghostdag = |selected_parent: BlockHash, reds: Vec<BlockHash>| ExternalGhostdagData {
            blue_score: 0,
            blue_work: Default::default(),
            selected_parent,
            mergeset_blues: vec![selected_parent],
            mergeset_reds: reds,
            blues_anticone_sizes: BlockHashMap::default(),
        };
        // genesis(1) <- attempt(2) <- heartbeat(3), which merged round blocks 10, 11 and 12 as reds.
        let mut chain = Chain { sink: hash(3), headers: HashMap::new(), ghostdag: HashMap::new() };
        chain.headers.insert(hash(1), header(1, 0));
        chain.headers.insert(hash(2), header(POW_ALGO_ID_PALW_COMMITTED_V2, 1));
        chain.headers.insert(hash(3), header(POW_ALGO_ID_HEARTBEAT_V1, 2));
        for r in 10..13 {
            chain.headers.insert(hash(r), header(POW_ALGO_ID_PALW_ROUND_V1, 1));
        }
        chain.ghostdag.insert(hash(1), ghostdag(hash(1), vec![]));
        chain.ghostdag.insert(hash(2), ghostdag(hash(1), vec![]));
        chain.ghostdag.insert(hash(3), ghostdag(hash(2), vec![hash(10), hash(11), hash(12)]));
        let mix = lane_mix_v1(&chain, 600);
        assert_eq!((mix.window_blocks, mix.attempt_blocks, mix.heartbeat_blocks, mix.other_blocks), (3, 1, 1, 1));
        assert_eq!(mix.round_blocks, 3, "the three round blocks the heartbeat merged");
        assert_eq!((mix.last_work_daa, mix.blocks_since_work), (Some(1), 1));
    }
}
