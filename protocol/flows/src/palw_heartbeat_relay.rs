//! **The heartbeat lane on the wire: one beat a slot, and a budget a peer** (the 2026-09-24 heartbeat
//! audit, H2).
//!
//! Past ADR-0142's cursor a slot needs two beats and no more: the one stamped into the open slot,
//! and the one that merges it and steps the clock. Every further beat for the same slot is a valid
//! block that ticks nothing — it weighs ε, adds a blue score and costs every peer a download and a
//! validation. Consensus admits them (a beat is priced by its hash, and the width rule bounds a
//! mergeset, not the DAG), so the transport is where their spread is bounded, the way ADR-0125 §7.3
//! bounds a permit's round blocks: a node announces the FIRST beat it validates for a
//! `(DAA score, selected parent's DAA score)` and not a second. The key names the slot without
//! reading a window: a beat whose score equals its parent's is the one holding the slot that parent
//! score's reference opened, and one whose score is its parent's plus one is the step. A beat that
//! is not announced is still stored and still mergeable; a descendant that needs it brings it back
//! as an orphan root, so liveness never waits on a relay decision.
//!
//! And a per-peer budget ([`PalwHeartbeatInvBudgetV1`]): a peer may hand this node at most
//! [`PALW_HEARTBEAT_INV_BURST`] heartbeats at once and one per [`PALW_HEARTBEAT_INV_REFILL_MS`]
//! after that. The honest rate is two or three a slot (120 s) from the whole network; the budget is
//! four to six times that, and past it a peer's heartbeats are not downloaded into consensus at all
//! (orphan roots — beats a descendant needs — are exempt).
//!
//! Bounded: at most [`PALW_HEARTBEAT_RELAY_MAX_KEYS`] slots remembered, pruned below
//! [`PALW_HEARTBEAT_RELAY_KEEP_DAA`] of the newest score first.

use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::pow_layer0::POW_ALGO_ID_HEARTBEAT_V1;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// The most slots the relay remembers.
pub const PALW_HEARTBEAT_RELAY_MAX_KEYS: usize = 4_096;
/// How far below the newest DAA score a slot is remembered once the map is full.
pub const PALW_HEARTBEAT_RELAY_KEEP_DAA: u64 = 1_000;
/// A peer's heartbeat allowance at once: four slots' worth of the honest two-to-three.
pub const PALW_HEARTBEAT_INV_BURST: u32 = 12;
/// One more heartbeat a peer may hand over per this many milliseconds: twelve per 120 s slot.
pub const PALW_HEARTBEAT_INV_REFILL_MS: u64 = 10_000;

/// What the relay policy says about a validated block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwHeartbeatRelayVerdictV1 {
    /// Not a heartbeat: no opinion — relay as any block.
    NotHeartbeat,
    /// The first beat this node validated for its slot: announce it.
    First,
    /// Another beat for a slot already announced (or the same block seen again): keep it, do not
    /// announce it.
    Repeat,
}

/// `(the beat's own DAA score, its selected parent's DAA score)`.
type SlotKey = (u64, u64);

#[derive(Default)]
struct Inner {
    seen: BTreeMap<SlotKey, BlockHash>,
    newest_daa: u64,
}

/// The relay's memory of heartbeats by slot.
#[derive(Default)]
pub struct PalwHeartbeatRelayV1 {
    inner: Mutex<Inner>,
}

impl PalwHeartbeatRelayV1 {
    /// Observe a validated block. `selected_parent_daa` is its selected parent's DAA score.
    pub fn observe(&self, block: BlockHash, header: &Header, selected_parent_daa: u64) -> PalwHeartbeatRelayVerdictV1 {
        if header.pow_algo_id != POW_ALGO_ID_HEARTBEAT_V1 {
            return PalwHeartbeatRelayVerdictV1::NotHeartbeat;
        }
        let key: SlotKey = (header.daa_score, selected_parent_daa);
        let mut inner = self.inner.lock().expect("the heartbeat relay lock is never poisoned by a panic while held");
        inner.newest_daa = inner.newest_daa.max(header.daa_score);
        if inner.seen.contains_key(&key) {
            return PalwHeartbeatRelayVerdictV1::Repeat;
        }
        inner.seen.insert(key, block);
        if inner.seen.len() > PALW_HEARTBEAT_RELAY_MAX_KEYS {
            let floor = inner.newest_daa.saturating_sub(PALW_HEARTBEAT_RELAY_KEEP_DAA);
            // Oldest scores first, and down to half the bound even when every score is recent, so
            // the bound holds whatever the scores are.
            while inner.seen.len() > PALW_HEARTBEAT_RELAY_MAX_KEYS / 2 || inner.seen.keys().next().is_some_and(|(daa, _)| *daa < floor)
            {
                if inner.seen.pop_first().is_none() {
                    break;
                }
            }
        }
        PalwHeartbeatRelayVerdictV1::First
    }

    /// How many slots are remembered.
    pub fn len(&self) -> usize {
        self.inner.lock().expect("the heartbeat relay lock is never poisoned by a panic while held").seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// **A peer's heartbeat allowance** — a token bucket, per relay flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwHeartbeatInvBudgetV1 {
    tokens: u32,
    last_refill_ms: u64,
}

impl PalwHeartbeatInvBudgetV1 {
    /// A full allowance at `now_ms`.
    pub fn new(now_ms: u64) -> Self {
        Self { tokens: PALW_HEARTBEAT_INV_BURST, last_refill_ms: now_ms }
    }

    /// Spend one heartbeat if the allowance has one. A clock that stepped backwards restarts the
    /// refill from now rather than stalling it.
    pub fn take(&mut self, now_ms: u64) -> bool {
        if now_ms < self.last_refill_ms {
            self.last_refill_ms = now_ms;
        }
        let earned = (now_ms - self.last_refill_ms) / PALW_HEARTBEAT_INV_REFILL_MS;
        if earned > 0 {
            self.tokens = (self.tokens as u64 + earned).min(PALW_HEARTBEAT_INV_BURST as u64) as u32;
            self.last_refill_ms += earned * PALW_HEARTBEAT_INV_REFILL_MS;
        }
        if self.tokens == 0 {
            return false;
        }
        self.tokens -= 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2;

    fn beat(daa: u64, nonce: u64) -> (BlockHash, Header) {
        let mut header = Header::from_precomputed_hash(BlockHash::from_u64_word(nonce), vec![]);
        header.pow_algo_id = POW_ALGO_ID_HEARTBEAT_V1;
        header.daa_score = daa;
        header.nonce = nonce;
        (BlockHash::from_u64_word(nonce), header)
    }

    /// **H2: a flood of beats for one slot is announced once.** Before this every valid beat was
    /// announced to every peer — the relay half of 89% of beats ticking nothing.
    #[test]
    fn a_hundred_beats_for_one_slot_are_announced_once() {
        let relay = PalwHeartbeatRelayV1::default();
        let verdicts: Vec<_> = (0..100u64)
            .map(|nonce| {
                let (hash, header) = beat(500, 1_000 + nonce);
                relay.observe(hash, &header, 500)
            })
            .collect();
        assert_eq!(verdicts[0], PalwHeartbeatRelayVerdictV1::First);
        assert!(verdicts[1..].iter().all(|v| *v == PalwHeartbeatRelayVerdictV1::Repeat), "one announcement per slot");

        // The step over that slot is another key — announced — and so is the next slot's beat.
        let (hash, header) = beat(501, 9_000);
        assert_eq!(relay.observe(hash, &header, 500), PalwHeartbeatRelayVerdictV1::First, "the step is announced");
        let (hash, header) = beat(501, 9_001);
        assert_eq!(relay.observe(hash, &header, 501), PalwHeartbeatRelayVerdictV1::First, "the next slot's beat is announced");
        // The same block seen twice is not announced twice.
        assert_eq!(relay.observe(hash, &header, 501), PalwHeartbeatRelayVerdictV1::Repeat);

        // Another lane is never this relay's business.
        let (hash, mut header) = beat(500, 7);
        header.pow_algo_id = POW_ALGO_ID_PALW_COMMITTED_V2;
        assert_eq!(relay.observe(hash, &header, 500), PalwHeartbeatRelayVerdictV1::NotHeartbeat);
    }

    /// The memory is bounded whatever the scores are.
    #[test]
    fn the_relay_memory_is_bounded() {
        let relay = PalwHeartbeatRelayV1::default();
        for daa in 0..(3 * PALW_HEARTBEAT_RELAY_MAX_KEYS as u64) {
            let (hash, header) = beat(daa, daa);
            assert_eq!(relay.observe(hash, &header, daa), PalwHeartbeatRelayVerdictV1::First);
            assert!(relay.len() <= PALW_HEARTBEAT_RELAY_MAX_KEYS);
        }
        // A recent slot is still remembered after the pruning.
        let newest = 3 * PALW_HEARTBEAT_RELAY_MAX_KEYS as u64 - 1;
        let (hash, header) = beat(newest, u64::MAX);
        assert_eq!(relay.observe(hash, &header, newest), PalwHeartbeatRelayVerdictV1::Repeat);
    }

    /// **H2: a peer's heartbeats are budgeted** — a burst, then one per refill interval.
    #[test]
    fn a_peer_flooding_heartbeats_is_held_to_its_budget() {
        let t0 = 1_790_000_000_000u64;
        let mut budget = PalwHeartbeatInvBudgetV1::new(t0);
        let taken = (0..100).filter(|_| budget.take(t0)).count();
        assert_eq!(taken, PALW_HEARTBEAT_INV_BURST as usize, "a burst of at most the allowance");
        assert!(!budget.take(t0 + PALW_HEARTBEAT_INV_REFILL_MS - 1), "nothing earned before the interval");
        assert!(budget.take(t0 + PALW_HEARTBEAT_INV_REFILL_MS), "one earned per interval");
        assert!(!budget.take(t0 + PALW_HEARTBEAT_INV_REFILL_MS));
        // Over one 120 s slot a flooding peer gets at most the allowance plus twelve.
        let mut budget = PalwHeartbeatInvBudgetV1::new(t0);
        let over_a_slot = (0..120_000u64).step_by(100).filter(|dt| budget.take(t0 + dt)).count();
        assert!(over_a_slot <= PALW_HEARTBEAT_INV_BURST as usize + 12, "{over_a_slot}");
        // The honest rate — three a slot — is never refused.
        let mut budget = PalwHeartbeatInvBudgetV1::new(t0);
        for slot in 0..1_000u64 {
            for k in 0..3u64 {
                assert!(budget.take(t0 + slot * 120_000 + k * 1_000), "slot {slot} beat {k}");
            }
        }
        // A clock that steps backwards does not stall the refill.
        let mut budget = PalwHeartbeatInvBudgetV1::new(t0);
        for _ in 0..PALW_HEARTBEAT_INV_BURST {
            assert!(budget.take(t0));
        }
        assert!(!budget.take(t0 - 5_000));
        assert!(budget.take(t0 - 5_000 + PALW_HEARTBEAT_INV_REFILL_MS));
    }
}
