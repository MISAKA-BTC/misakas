//! **The heartbeat miner** — ADR-0060 Decision 1's operational half.
//!
//! A bondless, one-thread hash miner for the heartbeat lane: it asks the mining manager for an
//! ordinary template (so the mempool's transactions — bond registrations included — ride the
//! block), lets consensus re-shape it into the lane
//! (`ConsensusApi::heartbeat_adapt_block_template`: algo-8, the global bits with the lane's own
//! fixed target and its one-block-deep slot rule (ADR-0066 Decisions 1-2), an
//! empty carriage, a zero-subsidy coinbase), waits for the slot, grinds BLAKE2b-512 ∥ SHA3-512
//! nonces, and submits.
//!
//! It earns fees only, by design. In calm weather this service wakes for a few seconds an hour;
//! in a crisis — every bonded lane silent — the slot ladder tightens to the full cadence and
//! this one thread is what keeps the chain's clock (and every PALW timeout sweep) running.
//! Anyone may run it; the more that do, the harder the lane's own retarget and the same 24/day.
//!
//! ## It stands aside for a bonded block that is waiting to be merged (ADR-0102 Decision 2)
//!
//! Measured on testnet-11, 2026-09-10: once a heartbeat was the selected parent, this service kept
//! the chain on heartbeats for as long as it ran. A Qwen3.6 draw takes ~17 minutes, the recovery
//! cadence is 120 s, so every bonded block landed eight heartbeats behind the tip and was never
//! selected again — the operator had to stop every heartbeat miner for one draw to get the chain
//! back. So when the chain runs on heartbeats and a bonded block is waiting in the virtual's
//! mergeset, the miner now waits (until that block's timestamp plus the nominal hour), which gives
//! the NEXT bonded draw a tip nobody is stacking ε on. It is advice, not a rule: a heartbeat mined
//! anyway is exactly as valid as before.
//!
//! **Bounded, because a clock that waits on producers is ADR-0060 §1's hostage.** The waiting is
//! capped at one nominal hour per heartbeat-led episode, and an episode ends only when a bonded
//! block becomes the selected parent. A producer whose blocks can never take the chain — a wedged
//! bond at its exposure ceiling, a key that paid the lottery and has no bond — costs the clock at
//! most that hour, once, and then the lane ticks at cadence until the bonded lane really returns.

use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::network::NetworkId;
use kaspa_consensus_core::palw_heartbeat_v1::{HEARTBEAT_NOMINAL_INTERVAL_MS, HeartbeatYieldHintV1};
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::task::service::{AsyncService, AsyncServiceFuture};
use kaspa_core::{info, trace, warn};
use kaspa_mining::manager::MiningManagerProxy;
use kaspa_p2p_flows::flow_context::FlowContext;
use std::sync::Arc;

pub const PALW_HEARTBEAT: &str = "palw-heartbeat-miner";

/// Nonces per adapted template before rebuilding. The lane's floor is 2²⁴ expected hashes; two
/// floors' worth makes a give-up rare while a long grind against a moving median time stays
/// bounded. Bounded LOUDLY, like the producer's: a silent give-up looks like an unreachable
/// difficulty.
const NONCES_PER_TEMPLATE: u64 = 1 << 25;

/// The longest single wait before the miner looks at the chain again — the slot wait's own cap,
/// so a bonded block landing mid-wait (which ends the episode) is seen within a minute.
const MAX_WAIT_MS: u64 = 60_000;

/// **ADR-0102 Decision 2's budget: how long this miner may still stand aside in the current
/// heartbeat-led episode.** Node-local state, deliberately — it is this miner's policy and no peer
/// needs to agree with it.
///
/// One nominal hour per episode: the same hour the slot rule grants a bonded selected parent, so a
/// yield never holds the lane longer than the rule already would have for a block that took the
/// chain. It is spent only while the slot is open and a bonded block is waiting, and refilled only
/// when a bonded block IS the selected parent — never by a quiet heartbeat, which would let a
/// stream of bonded blocks that never take the chain hold the clock indefinitely.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HeartbeatYieldBudget {
    remaining_ms: u64,
    /// Whether the last decision was a wait — so the operator log says when a yield STARTS and
    /// when it ends, not once a minute in between.
    yielding: bool,
}

/// What the budget decided for one pass of the miner loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HeartbeatYieldDecision {
    /// Mine now. `ended_yield` is set on the first pass after a wait, for the log.
    Mine { ended_yield: bool },
    /// Wait this long, then look again. `started` is set on the first pass of a wait, for the log.
    Wait { wait_ms: u64, until: u64, remaining_ms: u64, started: bool },
}

impl HeartbeatYieldBudget {
    pub(crate) fn new() -> Self {
        Self { remaining_ms: HEARTBEAT_NOMINAL_INTERVAL_MS, yielding: false }
    }

    /// Note what the chain looks like on a pass where the slot rule is still holding the lane. A
    /// bonded selected parent ends the episode HERE too: a miner whose slot never opens during a
    /// bonded stretch (another miner took each first slot) must still start the next episode with
    /// a full budget, or a spent budget would carry over into an episode it has nothing to do with.
    pub(crate) fn observe(&mut self, hint: HeartbeatYieldHintV1) {
        if hint == HeartbeatYieldHintV1::BondedSelectedParent {
            self.remaining_ms = HEARTBEAT_NOMINAL_INTERVAL_MS;
        }
    }

    /// One pass: given the hint and the wall clock, mine or wait — and charge the wait to the
    /// budget before it is taken, so an interrupted wait is never refunded.
    pub(crate) fn decide(&mut self, hint: HeartbeatYieldHintV1, now_ms: u64) -> HeartbeatYieldDecision {
        let (wait_ms, until) = match hint {
            HeartbeatYieldHintV1::BondedSelectedParent => {
                // The bonded lane holds the chain: the episode is over and the next one starts full.
                self.remaining_ms = HEARTBEAT_NOMINAL_INTERVAL_MS;
                (0, 0)
            }
            HeartbeatYieldHintV1::NothingToYieldTo => (0, 0),
            HeartbeatYieldHintV1::YieldUntil(until) => (until.saturating_sub(now_ms).min(self.remaining_ms).min(MAX_WAIT_MS), until),
        };
        if wait_ms == 0 {
            let ended_yield = std::mem::replace(&mut self.yielding, false);
            return HeartbeatYieldDecision::Mine { ended_yield };
        }
        self.remaining_ms -= wait_ms;
        let started = !std::mem::replace(&mut self.yielding, true);
        HeartbeatYieldDecision::Wait { wait_ms, until, remaining_ms: self.remaining_ms, started }
    }
}

#[derive(Clone, Debug)]
pub struct PalwHeartbeatMinerConfig {
    /// Where the lane's FEES are paid (the subsidy is zero by rule). ML-DSA-87 P2PKH.
    pub pay_address: String,
    pub address_prefix: kaspa_addresses::Prefix,
    pub network_id: NetworkId,
    // `enable_unsynced_mining` used to live here, "honoured exactly as the producer honours it" —
    // which was the defect: the sink-age clause it waived should never have applied to the clock
    // at all (see the worker's gate comment, ADR-0068 launch audit). The lane needs no waiver
    // because it holds on nothing a stalled chain cannot supply.
}

pub struct PalwHeartbeatMinerService {
    config: PalwHeartbeatMinerConfig,
    consensus_manager: Arc<ConsensusManager>,
    mining_manager: MiningManagerProxy,
    flow_context: Arc<FlowContext>,
    miner_data: Option<MinerData>,
    /// Fired by `signal_exit` so `start` can finish — the panel's fix, copied here for the same
    /// reason it went into the producer: the ADR-0068 drill's H node hung in `pthread_join` on
    /// SIGTERM with every server already stopped (finding F1).
    shutdown: kaspa_utils::triggers::SingleTrigger,
    /// ADR-0102 Decision 2: how long this miner may still stand aside in the current episode.
    yield_budget: std::sync::Mutex<HeartbeatYieldBudget>,
}

impl PalwHeartbeatMinerService {
    pub fn new(
        config: PalwHeartbeatMinerConfig,
        consensus_manager: Arc<ConsensusManager>,
        mining_manager: MiningManagerProxy,
        flow_context: Arc<FlowContext>,
    ) -> Self {
        // The same pay-address gate the producer applies, for the same reason: a non-PQ script
        // in the coinbase payload poisons every descendant's reward fan-out.
        let miner_data = match kaspa_addresses::Address::try_from(config.pay_address.as_str()) {
            Ok(addr) if addr.version != kaspa_addresses::Version::PubKeyHashMlDsa87 => {
                warn!("[{PALW_HEARTBEAT}] pay address is not ML-DSA-87 P2PKH — heartbeat mining disabled");
                None
            }
            Ok(addr) if addr.prefix != config.address_prefix => {
                warn!(
                    "[{PALW_HEARTBEAT}] pay address is for {} and this node is {} — heartbeat mining disabled",
                    addr.prefix, config.address_prefix
                );
                None
            }
            Ok(addr) => Some(MinerData::new(kaspa_txscript::pay_to_address_script(&addr), Vec::new())),
            Err(err) => {
                warn!("[{PALW_HEARTBEAT}] pay address is unusable: {err} — heartbeat mining disabled");
                None
            }
        };
        Self {
            config,
            consensus_manager,
            mining_manager,
            flow_context,
            miner_data,
            shutdown: kaspa_utils::triggers::SingleTrigger::default(),
            yield_budget: std::sync::Mutex::new(HeartbeatYieldBudget::new()),
        }
    }

    /// Sleep `period`, or return `false` the moment `signal_exit` fires (the panel's `tick`,
    /// ADR-0068 drill finding F1). Every wait in the worker goes through this.
    async fn tick(&self, period: std::time::Duration) -> bool {
        tokio::select! {
            _ = tokio::time::sleep(period) => true,
            _ = self.shutdown.listener.clone() => false,
        }
    }

    pub async fn worker(self: &Arc<Self>) {
        let Some(miner_data) = self.miner_data.clone() else {
            info!("[{PALW_HEARTBEAT}] not mining (see the startup warning above)");
            return;
        };
        info!("[{PALW_HEARTBEAT}] starting — bondless heartbeat lane (ADR-0060), fee-only, one thread");
        let mut mined = 0u64;
        loop {
            if !self.tick(std::time::Duration::from_secs(2)).await {
                break;
            }
            let session = self.consensus_manager.consensus().unguarded_session();
            if session.async_is_consensus_in_transitional_ibd_state().await {
                continue;
            }
            // **Deliberately NOT `should_mine`** (ADR-0068 launch audit). `should_mine` folds in
            // `is_nearly_synced` — the sink's timestamp must be recent — with the sync-rate rule
            // as its only escape, and that escape itself expires once the finality point is more
            // than three finality durations old. Which is to say: the longer a chain has been
            // stopped, the more firmly it refuses to be restarted — the exact self-referential
            // hostage ADR-0060 §1 catalogues, wearing a mining-heuristic costume. This lane IS
            // the clock; a stale sink is not a reason for the clock to hold, it is the one
            // condition the clock exists to end (its own block is what makes the sink recent
            // again, which is also what lets every PRODUCER's unmodified `should_mine` pass
            // flag-free after an outage). The drill never saw this because its miner carried
            // `--enable-unsynced-mining`; a fresh re-genesis (whose genesis timestamp is months
            // old) or any stall past the sync-rate rule's horizon would have seen it immediately.
            //
            // What legitimately still holds the lane: the chain-participation gate (a quarantined
            // or unresolved chain must not be extended — by anyone, clock included), peer
            // connectivity (a tick nobody hears sweeps nobody's timeouts; total isolation would
            // only mint a weightless solo branch), and the transitional-IBD check above.
            if !(self.flow_context.hub().has_peers() && self.flow_context.is_consensus_participation_allowed()) {
                trace!("[{PALW_HEARTBEAT}] holding: no peers, or chain participation is not allowed");
                if !self.tick(std::time::Duration::from_secs(5)).await {
                    break;
                }
                continue;
            }
            match self.mine_one(&session, miner_data.clone()).await {
                Ok(Some(hash)) => {
                    mined += 1;
                    info!("[{PALW_HEARTBEAT}] heartbeat #{mined} {hash} — the clock ticked");
                }
                Ok(None) => {}
                Err(err) => {
                    warn!("[{PALW_HEARTBEAT}] {err}");
                    if !self.tick(std::time::Duration::from_secs(5)).await {
                        break;
                    }
                }
            }
        }
        info!("[{PALW_HEARTBEAT}] stopping ({mined} heartbeats this run)");
    }

    /// The budget, whatever state its lock is in. A poisoned lock means a pass panicked while
    /// holding it; the budget is taken as it stands rather than holding the clock on a panic.
    fn with_yield_budget<R>(&self, f: impl FnOnce(&mut HeartbeatYieldBudget) -> R) -> R {
        match self.yield_budget.lock() {
            Ok(mut budget) => f(&mut budget),
            Err(poisoned) => f(&mut poisoned.into_inner()),
        }
    }

    /// One template, one adapt, at most one slot wait, one yield wait, one bounded nonce search.
    async fn mine_one(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        miner_data: MinerData,
    ) -> Result<Option<kaspa_consensus_core::BlockHash>, String> {
        // ADR-0102 Decision 2: read the chain's state for the yield before the template, and let a
        // bonded selected parent refill the budget even on a pass the slot rule is about to hold.
        let hint = session.heartbeat_yield_hint();
        self.with_yield_budget(|budget| budget.observe(hint));
        let template = self
            .mining_manager
            .clone()
            .get_block_template(session, miner_data)
            .await
            .map_err(|e| format!("no block template: {e}"))?;
        let (mut template, earliest) =
            session.heartbeat_adapt_block_template(template).map_err(|e| format!("the lane refused the template: {e}"))?;
        let now = kaspa_core::time::unix_now();
        if earliest > now {
            // Inside the slot. Sleep up to the boundary (capped so a ladder change mid-wait is
            // picked up by a fresh template) and try again with fresh facts.
            let wait = (earliest - now).min(60_000u64);
            trace!("[{PALW_HEARTBEAT}] slot in {} s", wait / 1000);
            // A slot wait can be a full minute — long enough to be the wait a SIGTERM lands in
            // (finding F1), so it is a tick like every other: on shutdown, hand back to the
            // worker loop, whose own tick exits.
            self.tick(std::time::Duration::from_millis(wait)).await;
            return Ok(None);
        }
        // **ADR-0102 Decision 2: the slot is open — stand aside if a bonded block is waiting.**
        //
        // Decided only once the slot is open, so time the slot rule was already holding the lane is
        // never charged to the episode's budget.
        let decision = self.with_yield_budget(|budget| budget.decide(hint, now));
        match decision {
            HeartbeatYieldDecision::Wait { wait_ms, until, remaining_ms, started } => {
                if started {
                    info!(
                        "[{PALW_HEARTBEAT}] standing aside: the chain runs on heartbeats and a bonded block is waiting to be \
                         merged — waiting until {until} (its timestamp + the nominal hour) or until a bonded block is the \
                         selected parent, with {} s of this episode's yield budget left (ADR-0102)",
                        remaining_ms / 1000
                    );
                } else {
                    trace!("[{PALW_HEARTBEAT}] still standing aside ({} s of budget left)", remaining_ms / 1000);
                }
                self.tick(std::time::Duration::from_millis(wait_ms)).await;
                return Ok(None);
            }
            HeartbeatYieldDecision::Mine { ended_yield } => {
                if ended_yield {
                    info!("[{PALW_HEARTBEAT}] done standing aside ({hint:?}); mining at the slot rule's cadence again (ADR-0102)");
                }
            }
        }
        // Grind. The lane's floor is ~2²⁴ hashes — seconds of one core — and the retarget can
        // raise it when many nodes run this service; the search stays bounded and loud.
        let header0 = template.block.header.clone();
        let network_id = self.config.network_id;
        let found = tokio::task::spawn_blocking(move || {
            let state = kaspa_pow::StateLayer0::new(&header0, network_id.to_string().as_bytes());
            (0..NONCES_PER_TEMPLATE).find(|&nonce| state.check_pow_layer0(nonce).map(|(ok, _)| ok).unwrap_or(false))
        })
        .await
        .map_err(|e| format!("the nonce search task did not finish: {e}"))?;
        let Some(nonce) = found else {
            trace!("[{PALW_HEARTBEAT}] no nonce in {NONCES_PER_TEMPLATE} tries against this template");
            return Ok(None);
        };
        template.block.header.nonce = nonce;
        template.block.header.finalize();
        let block: kaspa_consensus_core::block::Block = template.block.to_immutable();
        let hash = block.hash();
        self.flow_context
            .submit_rpc_block(session, block)
            .await
            .map_err(|e| format!("the chain refused a heartbeat this node mined: {e}"))?;
        Ok(Some(hash))
    }
}

impl AsyncService for PalwHeartbeatMinerService {
    fn ident(self: Arc<Self>) -> &'static str {
        PALW_HEARTBEAT
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            self.worker().await;
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", PALW_HEARTBEAT);
        // ADR-0068 drill finding F1: the missing half — see the producer's twin comment.
        self.shutdown.trigger.trigger();
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            trace!("{} stopped", PALW_HEARTBEAT);
            Ok(())
        })
    }
}

#[cfg(test)]
mod yield_budget_tests {
    use super::*;

    const T0: u64 = 1_788_000_000_000;

    /// **The trap's shape: a bonded block lands while the chain runs on heartbeats, and the miner
    /// waits for it until the block's hour is up — in minute-sized steps, logged once.**
    #[test]
    fn a_waiting_bonded_block_holds_the_miner_until_its_hour_in_minute_steps() {
        let mut budget = HeartbeatYieldBudget::new();
        // The bonded block's template is 17 minutes old when it lands, so 43 minutes are left.
        let until = T0 - 17 * 60_000 + HEARTBEAT_NOMINAL_INTERVAL_MS;
        let first = budget.decide(HeartbeatYieldHintV1::YieldUntil(until), T0);
        assert_eq!(
            first,
            HeartbeatYieldDecision::Wait {
                wait_ms: MAX_WAIT_MS,
                until,
                remaining_ms: HEARTBEAT_NOMINAL_INTERVAL_MS - MAX_WAIT_MS,
                started: true
            }
        );
        // Later passes of the same wait are not a new start — the log says it once.
        let second = budget.decide(HeartbeatYieldHintV1::YieldUntil(until), T0 + MAX_WAIT_MS);
        assert!(matches!(second, HeartbeatYieldDecision::Wait { started: false, .. }));
        // At the block's hour the wait is over and the miner mines — and says the yield ended.
        assert_eq!(budget.decide(HeartbeatYieldHintV1::YieldUntil(until), until), HeartbeatYieldDecision::Mine { ended_yield: true });
        assert_eq!(
            budget.decide(HeartbeatYieldHintV1::NothingToYieldTo, until + 1),
            HeartbeatYieldDecision::Mine { ended_yield: false }
        );
    }

    /// **The bound that keeps this from being ADR-0060 §1's hostage: one nominal hour per episode,
    /// however many bonded blocks keep arriving without ever taking the chain.**
    ///
    /// A wedged bond at its exposure ceiling, or a key that pays the lottery and has no bond, can
    /// keep putting blocks into the virtual's mergeset that never become the selected parent. Each
    /// asks for its own hour; the episode's budget pays for one hour in total and then the miner
    /// ticks at cadence, no matter what the hint says.
    #[test]
    fn a_stream_of_blocks_that_never_take_the_chain_buys_one_hour_and_no_more() {
        let mut budget = HeartbeatYieldBudget::new();
        let mut now = T0;
        let mut waited = 0u64;
        for _ in 0..500 {
            // A fresh bonded block every pass, always an hour of yield away.
            match budget.decide(HeartbeatYieldHintV1::YieldUntil(now + HEARTBEAT_NOMINAL_INTERVAL_MS), now) {
                HeartbeatYieldDecision::Wait { wait_ms, .. } => {
                    waited += wait_ms;
                    now += wait_ms;
                }
                HeartbeatYieldDecision::Mine { .. } => now += 120_000,
            }
            // …and the chain keeps running on heartbeats: nothing refills the budget.
            budget.observe(HeartbeatYieldHintV1::NothingToYieldTo);
        }
        assert_eq!(waited, HEARTBEAT_NOMINAL_INTERVAL_MS, "the whole episode yields exactly one nominal hour");
        assert_eq!(
            budget.decide(HeartbeatYieldHintV1::YieldUntil(now + HEARTBEAT_NOMINAL_INTERVAL_MS), now),
            HeartbeatYieldDecision::Mine { ended_yield: false },
            "a spent budget mines whatever the hint asks"
        );
    }

    /// **The episode ends when a bonded block IS the selected parent — observed on a pass the slot
    /// rule holds as well as on one it opens — and never when the clock is merely quiet.**
    #[test]
    fn only_a_bonded_selected_parent_refills_the_budget() {
        let mut budget = HeartbeatYieldBudget::new();
        let mut now = T0;
        // Drain the episode's budget against a deadline it can never reach.
        while let HeartbeatYieldDecision::Wait { wait_ms, .. } = budget.decide(HeartbeatYieldHintV1::YieldUntil(u64::MAX), now) {
            now += wait_ms;
        }
        assert_eq!(now - T0, HEARTBEAT_NOMINAL_INTERVAL_MS);
        // A quiet heartbeat chain does not refill it…
        budget.observe(HeartbeatYieldHintV1::NothingToYieldTo);
        assert!(matches!(budget.decide(HeartbeatYieldHintV1::YieldUntil(now + 1_000), now), HeartbeatYieldDecision::Mine { .. }));
        // …a bonded selected parent does, seen while the slot rule was holding the lane…
        budget.observe(HeartbeatYieldHintV1::BondedSelectedParent);
        assert!(matches!(budget.decide(HeartbeatYieldHintV1::YieldUntil(now + 1_000), now), HeartbeatYieldDecision::Wait { .. }));
        // …or on an open-slot pass, where the bonded selected parent is itself the answer: mine.
        let mut fresh = HeartbeatYieldBudget::new();
        assert_eq!(fresh.decide(HeartbeatYieldHintV1::BondedSelectedParent, T0), HeartbeatYieldDecision::Mine { ended_yield: false });
    }

    /// A deadline already in the past, or saturated at the top of the range, is never a wait longer
    /// than one step or than the budget.
    #[test]
    fn a_stale_or_saturated_deadline_is_bounded_by_the_step_and_the_budget() {
        let mut budget = HeartbeatYieldBudget::new();
        assert_eq!(budget.decide(HeartbeatYieldHintV1::YieldUntil(T0 - 1), T0), HeartbeatYieldDecision::Mine { ended_yield: false });
        assert!(matches!(
            budget.decide(HeartbeatYieldHintV1::YieldUntil(u64::MAX), T0),
            HeartbeatYieldDecision::Wait { wait_ms: MAX_WAIT_MS, .. }
        ));
    }
}
