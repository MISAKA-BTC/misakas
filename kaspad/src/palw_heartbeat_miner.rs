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
//! ## It stands aside for a bonded block that is waiting to be merged (ADR-0105 Decision 2)
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
//!
//! ## It carries convictions first (ADR-0152 v3.1 H-1, P2-9)
//!
//! During a licence halt this lane may be the only one minting, and V-8 needs the conviction, DA
//! and reporter objects filed during the halt to land during it. Where R-core+ is armed the mining
//! manager's templates take those carriers before any other transaction (the carrier lane,
//! `TransactionsPool::build_palw_carrier_lane`, bounded to half a block), a full mempool keeps a
//! reserve for them, and only carriers the tip's fold would take get either (the virtual processor's
//! H-1 gate) — so this service needs no selector of its own: the template it asks for already leads
//! with them. Each minted beat says how many it carries, and the relay spares a carrier-bearing beat
//! its H2 limits (`FlowContext::palw_heartbeat_h1_exempt`).

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

/// **How often the grind looks up (the 2026-09-24 heartbeat audit, H1).** Every 2¹⁸ nonces — a
/// sixty-fourth of the lane's 2²⁴ expectation, a fraction of a second on one core — the search asks
/// whether it is still worth finishing: whether the node is shutting down, and whether the virtual
/// has moved (a new sink, or a new tip: another beat that took the slot this one was ground for,
/// or the step that opened a new one). The grind used to run all 2²⁵ nonces blind, so a beat that
/// lost the race was still found, submitted and relayed, and the node's own next beat started late.
/// A nonce search is memoryless, so abandoning one and starting over on fresh parents costs
/// nothing in expectation.
const NONCES_PER_CHECK: u64 = 1 << 18;

/// The shortest wait the miner takes before it looks at the chain again, so a slot that is
/// already open by the time the wait is computed cannot turn the loop into a spin.
const MIN_WAIT_MS: u64 = 100;

/// The longest single wait before the miner looks at the chain again — the slot wait's own cap,
/// so a bonded block landing mid-wait (which ends the episode) is seen within a minute.
const MAX_WAIT_MS: u64 = 60_000;

/// **ADR-0105 Decision 2's budget: how long this miner may still stand aside in the current
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
            // A taken slot is waited out by the worker before the budget is asked (H1); if one does
            // reach here it is not a bonded block to stand aside for, and it spends no budget.
            HeartbeatYieldHintV1::NothingToYieldTo | HeartbeatYieldHintV1::SlotTaken(_) => (0, 0),
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

/// **What a minted beat did for the clock** — the operator line's subject (H1).
///
/// The miner used to print "the clock ticked" for every block it minted, and on testnet-12 nine
/// beats in ten ticked nothing: a line that is true one time in ten is how a lane can waste most of
/// its work while its log says it is healthy. The role is read off the adapted template itself —
/// its own DAA score against its selected parent's, and the slot the adapter stamped it for — so it
/// is the chain's arithmetic, not the miner's opinion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HeartbeatRoleV1 {
    /// **Granted.** Its own DAA score is above its selected parent's: it merged the beat that held
    /// the slot and advanced the clock to `daa`.
    Stepped { daa: u64 },
    /// **Not granted yet**: stamped at or past an open slot that no beat held, so the block that
    /// merges it ticks the clock to `daa`. Whether that block merges THIS beat or a rival's that
    /// reached it first is the race the next line settles.
    HoldsSlot { daa: u64 },
    /// **Not granted**: stamped before the slot it would claim, so no block that merges it ticks.
    /// With the hint and the adapter's `earliest` the miner never grinds one; the line exists so
    /// that if something upstream answers wrongly, the log says so instead of "the clock ticked".
    NotGranted { slot_ms: u64 },
    /// The clock cursor does not govern this block: the pre-ADR-0142 lane, where every beat that
    /// passes the slot rule is the clock.
    Ungoverned,
}

impl HeartbeatRoleV1 {
    /// Classify a beat from its own header facts and the slot the adapter stamped it for (`None`
    /// where the cursor does not govern).
    pub(crate) fn of(daa_score: u64, selected_parent_daa_score: u64, timestamp: u64, slot_ms: Option<u64>) -> Self {
        if daa_score > selected_parent_daa_score {
            return Self::Stepped { daa: daa_score };
        }
        match slot_ms {
            None => Self::Ungoverned,
            Some(slot_ms) if timestamp >= slot_ms => Self::HoldsSlot { daa: daa_score + 1 },
            Some(slot_ms) => Self::NotGranted { slot_ms },
        }
    }
}

/// How a grind ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GrindOutcomeV1 {
    Found(u64),
    /// All the template's nonces were tried.
    Exhausted,
    /// The virtual moved under the template (H1): abandon it and build on the new one.
    Superseded,
    /// The node is shutting down.
    Shutdown,
}

/// **The grind, cancellable** (H1). `solves(nonce)` is the proof-of-work check; every `check_every`
/// nonces `shutting_down()` and `superseded()` are asked, in that order, and either ends the search.
/// A pure function of its closures so the cancellation is testable without a chain or a hash.
pub(crate) fn grind_nonces_v1(
    max_nonces: u64,
    check_every: u64,
    solves: impl Fn(u64) -> bool,
    shutting_down: impl Fn() -> bool,
    mut superseded: impl FnMut() -> bool,
) -> GrindOutcomeV1 {
    let check_every = check_every.max(1);
    let mut start = 0u64;
    while start < max_nonces {
        let end = start.saturating_add(check_every).min(max_nonces);
        if let Some(nonce) = (start..end).find(|&nonce| solves(nonce)) {
            return GrindOutcomeV1::Found(nonce);
        }
        start = end;
        if shutting_down() {
            return GrindOutcomeV1::Shutdown;
        }
        if start < max_nonces && superseded() {
            return GrindOutcomeV1::Superseded;
        }
    }
    GrindOutcomeV1::Exhausted
}

/// What one pass of the worker did, so the loop knows whether it has already rested.
enum PassOutcomeV1 {
    Minted {
        hash: kaspa_consensus_core::BlockHash,
        role: HeartbeatRoleV1,
        /// How many of H-1's lifecycle carriers the beat carries (ADR-0152).
        carriers: usize,
    },
    /// The pass waited (a taken slot, a yield, a slot opening shortly): the wait was the rest.
    Waited,
    /// The virtual moved during the grind: rebuild at once on the new one.
    Superseded,
    /// Nothing minted and nothing waited: rest before the next pass.
    Idle,
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
    /// ADR-0105 Decision 2: how long this miner may still stand aside in the current episode.
    yield_budget: std::sync::Mutex<HeartbeatYieldBudget>,
    /// **Why this lane is not minting, said out loud.** The reason last printed and when.
    ///
    /// Every wait in `mine_one` was a `trace!`, so a miner that could not mint said nothing at the
    /// level anyone runs. Both of 2026-09-18's liveness bugs left this lane running and silent —
    /// one because it yielded to blocks that no longer paced the clock, one because the template
    /// stamped a slot an hour out — and on both a single line here would have named the cause in
    /// seconds instead of a drill. Same discipline as the producer's holds: print on a CHANGE of
    /// reason, then at most every five minutes, so an unchanging cause cannot bury the line that
    /// explains it.
    last_hold: std::sync::Mutex<Option<(String, std::time::Instant)>>,
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
            last_hold: std::sync::Mutex::new(None),
        }
    }

    /// Print `reason` if it differs from the last one, or if five minutes have passed since it was
    /// last printed. `None` clears the memory, so the next hold prints at once rather than being
    /// suppressed as a repeat of one the lane has since left.
    fn say_hold(&self, reason: Option<String>) {
        const REPEAT_AFTER: std::time::Duration = std::time::Duration::from_secs(300);
        let Ok(mut last) = self.last_hold.lock() else { return };
        match reason {
            None => *last = None,
            Some(reason) => {
                let due = match last.as_ref() {
                    Some((prev, at)) => prev != &reason || at.elapsed() >= REPEAT_AFTER,
                    None => true,
                };
                if due {
                    info!("[{PALW_HEARTBEAT}] {reason}");
                    *last = Some((reason, std::time::Instant::now()));
                }
            }
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
        let (mut stepped, mut holding, mut not_granted) = (0u64, 0u64, 0u64);
        // A pass that waited has already rested, and one whose template went stale or that just
        // minted should look again at once: the two-second rest between passes is the `a` of the
        // audit's `tick = 120 s + a + b + c`, paid on the one pass where it cost a tick.
        let mut rest = true;
        loop {
            if rest && !self.tick(std::time::Duration::from_secs(2)).await {
                break;
            }
            if self.shutdown.listener.is_triggered() {
                break;
            }
            rest = true;
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
                Ok(PassOutcomeV1::Minted { hash, role, carriers }) => {
                    mined += 1;
                    self.say_hold(None);
                    if carriers > 0 {
                        info!(
                            "[{PALW_HEARTBEAT}] heartbeat #{mined} {hash} carries {carriers} lifecycle carrier(s) — conviction, DA or \
                             reporter objects the lane is obliged to carry (ADR-0152 H-1)"
                        );
                    }
                    match role {
                        HeartbeatRoleV1::Stepped { daa } => {
                            stepped += 1;
                            info!(
                                "[{PALW_HEARTBEAT}] heartbeat #{mined} {hash} — granted: it merges the beat that held the slot \
                                 and advanced the clock to DAA {daa}"
                            );
                        }
                        HeartbeatRoleV1::HoldsSlot { daa } => {
                            holding += 1;
                            info!(
                                "[{PALW_HEARTBEAT}] heartbeat #{mined} {hash} — holds the open slot, not granted yet: the block \
                                 that merges it ticks the clock to DAA {daa}"
                            );
                        }
                        HeartbeatRoleV1::NotGranted { slot_ms } => {
                            not_granted += 1;
                            warn!(
                                "[{PALW_HEARTBEAT}] heartbeat #{mined} {hash} — NOT granted: stamped before its slot ({slot_ms}); \
                                 it adds a blue score and ticks nothing"
                            );
                        }
                        HeartbeatRoleV1::Ungoverned => info!("[{PALW_HEARTBEAT}] heartbeat #{mined} {hash} — the clock ticked"),
                    }
                    rest = false;
                }
                Ok(PassOutcomeV1::Waited | PassOutcomeV1::Superseded) => rest = false,
                Ok(PassOutcomeV1::Idle) => {}
                Err(err) => {
                    warn!("[{PALW_HEARTBEAT}] {err}");
                    if !self.tick(std::time::Duration::from_secs(5)).await {
                        break;
                    }
                }
            }
        }
        info!(
            "[{PALW_HEARTBEAT}] stopping ({mined} heartbeats this run: {stepped} advanced the clock, {holding} held an open \
             slot, {not_granted} not granted)"
        );
    }

    /// The budget, whatever state its lock is in. A poisoned lock means a pass panicked while
    /// holding it; the budget is taken as it stands rather than holding the clock on a panic.
    fn with_yield_budget<R>(&self, f: impl FnOnce(&mut HeartbeatYieldBudget) -> R) -> R {
        match self.yield_budget.lock() {
            Ok(mut budget) => f(&mut budget),
            Err(poisoned) => f(&mut poisoned.into_inner()),
        }
    }

    /// One template, one adapt, at most one slot wait, one yield wait, one bounded nonce search —
    /// and, past the clock cursor, no template at all while the slot is taken (H1).
    async fn mine_one(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        miner_data: MinerData,
    ) -> Result<PassOutcomeV1, String> {
        // ADR-0105 Decision 2: read the chain's state for the yield before the template, and let a
        // bonded selected parent refill the budget even on a pass the slot rule is about to hold.
        let hint = session.heartbeat_yield_hint();
        self.with_yield_budget(|budget| budget.observe(hint));
        // **H1: a taken slot is waited out, not ground against.** The clock already advanced into
        // the slot a beat minted now would claim; the beat would be merged, weigh ε, add a blue
        // score and tick nothing. Wait for the next slot, then ask again with fresh facts.
        if let HeartbeatYieldHintV1::SlotTaken(opens) = hint {
            let now = kaspa_core::time::unix_now();
            let away = opens.saturating_sub(now);
            // Two intervals is more than any honest reference can put between now and the next
            // slot — an honest beat is stamped at its miner's clock or at the slot; say so, because
            // a clock held that far out is a reference stamped in the future and an operator should
            // see it. The future-drift bound decides how far out one can be: 132 s on the hash
            // lineage's presets, 1,620 s on testnet-12 since 2026-09-25 — about 13 intervals, which
            // a producer running the clock ahead can take in one burst.
            if away > 2 * kaspa_consensus_core::palw_heartbeat_v1::HEARTBEAT_RECOVERY_INTERVAL_MS {
                self.say_hold(Some(format!(
                    "the next slot opens {} s from now — more than two intervals, so the reference it is measured from \
                     carries a timestamp ahead of this node's clock (check this host's clock and the peers' — or a \
                     producer is running the clock ahead of wall time)",
                    away / 1000
                )));
            } else {
                trace!("[{PALW_HEARTBEAT}] the slot is taken; the next opens in {} ms", away);
            }
            self.tick(std::time::Duration::from_millis(away.clamp(MIN_WAIT_MS, MAX_WAIT_MS))).await;
            return Ok(PassOutcomeV1::Waited);
        }
        let template = self
            .mining_manager
            .clone()
            .get_block_template(session, miner_data)
            .await
            .map_err(|e| format!("no block template: {e}"))?;
        let (mut template, earliest) =
            session.heartbeat_adapt_block_template(template).map_err(|e| format!("the lane refused the template: {e}"))?;
        let cursor_governs =
            self.flow_context.config.params.palw_clock_cursor.is_some_and(|fence| fence.is_active(template.block.header.daa_score));
        let now = kaspa_core::time::unix_now();
        if earliest > now {
            let wait = (earliest - now).clamp(MIN_WAIT_MS, MAX_WAIT_MS);
            if cursor_governs {
                // Past the cursor this is the slot a beat waiting in the virtual was granted, a few
                // seconds ahead of this node's clock (its miner's clock runs ahead of ours). The
                // step has to be stamped at or past it, so wait the difference.
                //
                // **The lead cap is kept here by construction** (`palw_clock_lead_cap`): no beat is
                // minted while its stamp is past this node's clock, so none is past the 132 s its
                // peers admit. A wait longer than that means the chain's median or slot stands that
                // far ahead of this clock — say so, since a peer's beat stamped there is refused too.
                if earliest - now > kaspa_consensus_core::palw_clock_cursor_v1::PALW_CLOCK_LEAD_CAP_MS
                    && self.flow_context.config.params.palw_clock_lead_cap.is_some()
                {
                    self.say_hold(Some(format!(
                        "the next beat would be stamped {} s past this node's clock — more than the 132 s a clock-moving \
                         header may lead it (palw_clock_lead_cap), so the chain's past-median time or its slot stands ahead \
                         of this clock: waiting for wall time (check this host's clock, and the peers')",
                        (earliest - now) / 1000
                    )));
                }
                trace!("[{PALW_HEARTBEAT}] the step's slot opens in {} ms", earliest - now);
            } else {
                // Inside the slot. Sleep up to the boundary (capped so a ladder change mid-wait is
                // picked up by a fresh template) and try again with fresh facts.
                self.say_hold(Some(format!(
                    "the slot rule holds this beat for {} s more (earliest {earliest}, now {now}): the lane's interval \
                     from the selected parent has not elapsed. If this line repeats while the chain keeps producing, the \
                     parent is being refreshed faster than the interval and the lane cannot mint at all — the state \
                     ADR-0138 §3c and its template half exist to prevent",
                    (earliest - now) / 1000
                )));
                trace!("[{PALW_HEARTBEAT}] slot in {} s", wait / 1000);
            }
            // A slot wait can be a full minute — long enough to be the wait a SIGTERM lands in
            // (finding F1), so it is a tick like every other: on shutdown, hand back to the
            // worker loop, which checks the trigger.
            self.tick(std::time::Duration::from_millis(wait)).await;
            return Ok(PassOutcomeV1::Waited);
        }
        // **ADR-0105 Decision 2: the slot is open — stand aside if a bonded block is waiting.**
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
                         selected parent, with {} s of this episode's yield budget left (ADR-0105)",
                        remaining_ms / 1000
                    );
                } else {
                    trace!("[{PALW_HEARTBEAT}] still standing aside ({} s of budget left)", remaining_ms / 1000);
                }
                self.tick(std::time::Duration::from_millis(wait_ms)).await;
                return Ok(PassOutcomeV1::Waited);
            }
            HeartbeatYieldDecision::Mine { ended_yield } => {
                if ended_yield {
                    info!("[{PALW_HEARTBEAT}] done standing aside ({hint:?}); mining at the slot rule's cadence again (ADR-0105)");
                }
            }
        }
        // Grind — cancellably (H1). The lane's floor is ~2²⁴ hashes, seconds of one core; every
        // `NONCES_PER_CHECK` the search asks whether the node is stopping and whether the virtual
        // still has the parents this template was built on. A rival beat for the same slot, or the
        // step that opens the next one, moves the virtual — and a beat finished against the old
        // parents would be one that loses the race, gets relayed and ticks nothing.
        let header0 = template.block.header.clone();
        let network_id = self.config.network_id;
        let parents: kaspa_consensus_core::BlockHashSet = header0.direct_parents().iter().copied().collect();
        let watcher = session.clone();
        let shutdown = self.shutdown.listener.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            let state = kaspa_pow::StateLayer0::new(&header0, network_id.to_string().as_bytes());
            grind_nonces_v1(
                NONCES_PER_TEMPLATE,
                NONCES_PER_CHECK,
                |nonce| state.check_pow_layer0(nonce).map(|(ok, _)| ok).unwrap_or(false),
                || shutdown.is_triggered(),
                || watcher.get_virtual_parents() != parents,
            )
        })
        .await
        .map_err(|e| format!("the nonce search task did not finish: {e}"))?;
        let nonce = match outcome {
            GrindOutcomeV1::Found(nonce) => nonce,
            GrindOutcomeV1::Exhausted => {
                trace!("[{PALW_HEARTBEAT}] no nonce in {NONCES_PER_TEMPLATE} tries against this template");
                return Ok(PassOutcomeV1::Idle);
            }
            GrindOutcomeV1::Superseded => {
                trace!("[{PALW_HEARTBEAT}] the virtual moved during the grind — rebuilding on it");
                return Ok(PassOutcomeV1::Superseded);
            }
            GrindOutcomeV1::Shutdown => return Ok(PassOutcomeV1::Idle),
        };
        template.block.header.nonce = nonce;
        template.block.header.finalize();
        let role = HeartbeatRoleV1::of(
            template.block.header.daa_score,
            template.selected_parent_daa_score,
            template.block.header.timestamp,
            cursor_governs.then_some(earliest),
        );
        let block: kaspa_consensus_core::block::Block = template.block.to_immutable();
        let hash = block.hash();
        // Counted only where H-1 binds (R-core+, genesis-armed), so the line never claims an
        // obligation a network does not have.
        let carriers = if self.flow_context.config.params.palw_rcore_plus_fence().is_some() {
            kaspa_consensus_core::palw_heartbeat_carriers_v1::palw_h1_carrier_ids_v1(&block.transactions).len()
        } else {
            0
        };
        self.flow_context
            .submit_rpc_block(session, block)
            .await
            .map_err(|e| format!("the chain refused a heartbeat this node mined: {e}"))?;
        Ok(PassOutcomeV1::Minted { hash, role, carriers })
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

#[cfg(test)]
mod grind_and_role_tests {
    use super::*;
    use std::cell::Cell;

    /// **H1: the grind stops when the virtual moves, and when the node stops — at the next check,
    /// not after 2²⁵ nonces.** Before this the search ran every nonce blind, so a beat that had
    /// already lost its slot was still found, submitted and relayed.
    #[test]
    fn the_grind_is_abandoned_at_the_first_check_after_the_virtual_moves() {
        let tried = Cell::new(0u64);
        let checks = Cell::new(0u64);
        // No nonce solves; the virtual moves after the second look.
        let outcome = grind_nonces_v1(
            NONCES_PER_TEMPLATE,
            NONCES_PER_CHECK,
            |_| {
                tried.set(tried.get() + 1);
                false
            },
            || false,
            || {
                checks.set(checks.get() + 1);
                checks.get() >= 2
            },
        );
        assert_eq!(outcome, GrindOutcomeV1::Superseded);
        assert_eq!(tried.get(), 2 * NONCES_PER_CHECK, "abandoned at the second check, not after the whole template");
        assert!(NONCES_PER_CHECK >= 1 << 16 && NONCES_PER_CHECK <= 1 << 20, "the audit's window: every 2^16..2^20 nonces");

        // Shutdown is asked first and wins over a moved virtual.
        let outcome = grind_nonces_v1(NONCES_PER_TEMPLATE, NONCES_PER_CHECK, |_| false, || true, || true);
        assert_eq!(outcome, GrindOutcomeV1::Shutdown);

        // A solution inside the first window is returned before anything is asked.
        let asked = Cell::new(false);
        let outcome = grind_nonces_v1(
            NONCES_PER_TEMPLATE,
            NONCES_PER_CHECK,
            |nonce| nonce == 12_345,
            || {
                asked.set(true);
                false
            },
            || {
                asked.set(true);
                false
            },
        );
        assert_eq!(outcome, GrindOutcomeV1::Found(12_345));
        assert!(!asked.get());

        // And a template nobody moves runs to exhaustion exactly as before.
        let outcome = grind_nonces_v1(1 << 20, NONCES_PER_CHECK, |_| false, || false, || false);
        assert_eq!(outcome, GrindOutcomeV1::Exhausted);
        // A zero check interval is not a spin on the closures.
        assert_eq!(grind_nonces_v1(3, 0, |nonce| nonce == 2, || false, || false), GrindOutcomeV1::Found(2));
    }

    /// **H1: the line says whether the beat was granted.** "The clock ticked" was printed for every
    /// block; the role is now read off the block's own score and the slot it was stamped for.
    #[test]
    fn a_beat_is_logged_as_granted_holding_or_not_granted_from_its_own_facts() {
        // It merged the slot's beat: its score is above its parent's.
        assert_eq!(HeartbeatRoleV1::of(11, 10, 5_000, Some(4_000)), HeartbeatRoleV1::Stepped { daa: 11 });
        // At or past the open slot, not stepping: it holds the slot, and the block that merges it
        // ticks the next score.
        assert_eq!(HeartbeatRoleV1::of(10, 10, 4_000, Some(4_000)), HeartbeatRoleV1::HoldsSlot { daa: 11 });
        // Before the slot: not granted — and no longer called a tick.
        assert_eq!(HeartbeatRoleV1::of(10, 10, 3_999, Some(4_000)), HeartbeatRoleV1::NotGranted { slot_ms: 4_000 });
        // No cursor: the old lane and the old line.
        assert_eq!(HeartbeatRoleV1::of(10, 10, 3_999, None), HeartbeatRoleV1::Ungoverned);
    }

    /// A taken slot spends no yield budget: it is not a bonded block to stand aside for.
    #[test]
    fn a_taken_slot_spends_no_yield_budget() {
        let mut budget = HeartbeatYieldBudget::new();
        assert_eq!(budget.decide(HeartbeatYieldHintV1::SlotTaken(u64::MAX), 1), HeartbeatYieldDecision::Mine { ended_yield: false });
        assert_eq!(budget, HeartbeatYieldBudget::new());
    }
}
