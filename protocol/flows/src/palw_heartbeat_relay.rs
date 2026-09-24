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
//!
//! ## A heartbeat that carries H-1's objects is spared both rules (ADR-0152 v3.1 H-1, P2-9)
//!
//! During a licence halt heartbeats may be the only blocks, and a conviction, a DA move or a
//! reporter's reveal has to ride one to land in time (V-8). H-1: "the H2 relay allowance MUST NOT
//! drop a heartbeat for carrying them". So a beat that carries an H-1 carrier
//! (`palw_h1_carrier_ids_v1`, the same list the miner's lane reads) is downloaded past an empty
//! allowance and announced although another beat holds its slot — but only on the strength of a
//! carrier that is **pending**: in this node's mempool (fee paid, scripts and inputs checked, not
//! yet seen in a block) and not already spent on another beat's exemption
//! ([`PalwHeartbeatRelayV1::h1_exemption`]). An unconditional exemption would reopen the
//! flood H2 closed, since a 0x4b payload costs nothing to put in a block; a pending carrier costs a
//! relay fee and buys one beat, once — and an honest carrier-bearing beat almost always qualifies,
//! because a carrier travels as a transaction before any miner includes it.

use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::pow_layer0::POW_ALGO_ID_HEARTBEAT_V1;
use kaspa_consensus_core::tx::TransactionId;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Mutex;

/// The most slots the relay remembers.
pub const PALW_HEARTBEAT_RELAY_MAX_KEYS: usize = 4_096;
/// How far below the newest DAA score a slot is remembered once the map is full.
pub const PALW_HEARTBEAT_RELAY_KEEP_DAA: u64 = 1_000;
/// A peer's heartbeat allowance at once: four slots' worth of the honest two-to-three.
pub const PALW_HEARTBEAT_INV_BURST: u32 = 12;
/// One more heartbeat a peer may hand over per this many milliseconds: twelve per 120 s slot.
pub const PALW_HEARTBEAT_INV_REFILL_MS: u64 = 10_000;
/// The most carriers the H-1 exemption remembers as spent, oldest forgotten first. A forgotten
/// carrier has long since left the mempool (a block took it), so it cannot buy a second beat.
pub const PALW_HEARTBEAT_H1_MAX_CLAIMS: usize = 4_096;

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
    /// H-1: each carrier that bought an exemption, and the beat it bought it for.
    h1_claims: HashMap<TransactionId, BlockHash>,
    /// `h1_claims`' keys in claim order, for the bound.
    h1_claim_order: VecDeque<TransactionId>,
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

    /// **H-1: may `block` be spared H2's rules on the strength of `pending` carriers?** `pending`
    /// is the caller's list of the block's H-1 carriers that sit in this node's mempool. `true` when
    /// one of them has bought no other beat's exemption yet, or already bought `block`'s — so a
    /// beat asked twice gets one answer. A carrier buys one beat, however many beats a miner copies
    /// it into.
    ///
    /// `claim` spends the open carriers on `block`. Only a VALIDATED block may claim (the
    /// announcement); the allowance gate, which runs before validation, only asks — or a peer could
    /// burn an honest carrier's exemption with a block that fails its proof of work.
    pub fn h1_exemption(&self, block: BlockHash, pending: &[TransactionId], claim: bool) -> bool {
        let mut inner = self.inner.lock().expect("the heartbeat relay lock is never poisoned by a panic while held");
        let mut exempt = false;
        for carrier in pending {
            match inner.h1_claims.get(carrier) {
                Some(owner) => exempt |= *owner == block,
                None if claim => {
                    inner.h1_claims.insert(*carrier, block);
                    inner.h1_claim_order.push_back(*carrier);
                    exempt = true;
                }
                None => exempt = true,
            }
        }
        while inner.h1_claim_order.len() > PALW_HEARTBEAT_H1_MAX_CLAIMS {
            if let Some(oldest) = inner.h1_claim_order.pop_front() {
                inner.h1_claims.remove(&oldest);
            }
        }
        exempt
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

    /// A lifecycle transaction carrying `object`.
    fn lifecycle(
        object: kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
        salt: u64,
    ) -> kaspa_consensus_core::tx::Transaction {
        use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
        let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).unwrap();
        let spk = kaspa_consensus_core::tx::ScriptPublicKey::from_vec(0, vec![0x51]);
        kaspa_consensus_core::tx::Transaction::new(
            0,
            vec![],
            vec![kaspa_consensus_core::tx::TransactionOutput::new(salt, spk)],
            0,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            payload,
        )
    }

    /// **ADR-0152 H-1, T38's relay half: a carrier-bearing beat is kept after the allowance is spent,
    /// and announced although another beat holds its slot — once per pending carrier.**
    ///
    /// The flow composes these pieces: `budget.take(now) || exempt(ask)` at download, and
    /// `observe → Repeat → exempt(claim)` at announcement, where `exempt` is the block's H-1 carriers
    /// (`palw_h1_carrier_ids_v1`) that sit in the mempool, put to [`PalwHeartbeatRelayV1::h1_exemption`].
    #[test]
    fn a_carrier_bearing_beat_is_kept_after_the_allowance_and_a_carrier_buys_one_beat() {
        use kaspa_consensus_core::palw_heartbeat_carriers_v1::palw_h1_carrier_ids_v1;
        use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
        use kaspa_consensus_core::tx::TransactionOutpoint;
        use kaspa_hashes::Hash64;

        let accusation = lifecycle(
            PalwConsensusObjectV2::DefaultAccused {
                claim: Hash64::from_u64_word(1),
                missing_event_index: 0,
                accuser: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(2), 0)),
                signature: vec![1; 8],
            },
            1,
        );
        let licence = lifecycle(PalwConsensusObjectV2::ReceiptLicensed { claim: Hash64::from_u64_word(3), receipts: vec![] }, 2);
        let mempool: std::collections::HashSet<TransactionId> = [accusation.id(), licence.id()].into_iter().collect();
        let pending = |txs: &[kaspa_consensus_core::tx::Transaction]| -> Vec<TransactionId> {
            palw_h1_carrier_ids_v1(txs).into_iter().filter(|id| mempool.contains(id)).collect()
        };

        // A peer has spent its allowance.
        let t0 = 1_790_000_000_000u64;
        let mut budget = PalwHeartbeatInvBudgetV1::new(t0);
        while budget.take(t0) {}
        let relay = PalwHeartbeatRelayV1::default();

        // Slot 500's first beat carries nothing; the peer's next beat for it carries the accusation.
        let (first, first_header) = beat(500, 1);
        assert_eq!(relay.observe(first, &first_header, 500), PalwHeartbeatRelayVerdictV1::First);
        let (bearing, bearing_header) = beat(500, 2);
        let body = vec![accusation.clone()];
        // Download: the allowance is spent, the exemption is asked (not claimed — not yet validated).
        assert!(!budget.take(t0) && relay.h1_exemption(bearing, &pending(&body), false), "kept past the allowance");
        // Announcement: it does not hold the slot, but its pending carrier buys it the announcement,
        // and asking again for the same beat gives the same answer.
        assert_eq!(relay.observe(bearing, &bearing_header, 500), PalwHeartbeatRelayVerdictV1::Repeat);
        assert!(relay.h1_exemption(bearing, &pending(&body), true), "announced");
        assert!(relay.h1_exemption(bearing, &pending(&body), true), "one beat, one answer");

        // A copy of the same carrier in another beat buys nothing: the carrier is spent.
        let (copy, _) = beat(500, 3);
        assert!(!relay.h1_exemption(copy, &pending(&body), false), "a carrier buys one beat");

        // A beat with no H-1 carrier — only a licence, which rides the fee market — is not spared,
        // and neither is one whose carrier this node does not hold (it takes the orphan-root route).
        let (plain, _) = beat(501, 4);
        assert!(!relay.h1_exemption(plain, &pending(&[licence]), false), "a licence is not an H-1 carrier");
        let unseen = lifecycle(
            PalwConsensusObjectV2::ReporterRevealed {
                offence_key: Hash64::from_u64_word(5),
                reporter: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(6), 0)),
                salt: [7; 32],
            },
            3,
        );
        assert_eq!(palw_h1_carrier_ids_v1(std::slice::from_ref(&unseen)), vec![unseen.id()], "an H-1 carrier…");
        assert!(!relay.h1_exemption(plain, &pending(&[unseen]), false), "…this node never saw is not pending");

        // Asking never spends: a block that failed validation after being asked leaves the carrier
        // free for the honest beat that carries it.
        let fresh = lifecycle(
            PalwConsensusObjectV2::ReporterRevealed {
                offence_key: Hash64::from_u64_word(8),
                reporter: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(9), 0)),
                salt: [1; 32],
            },
            4,
        );
        let (forged, _) = beat(502, 5);
        let (honest, _) = beat(502, 6);
        assert!(relay.h1_exemption(forged, &[fresh.id()], false));
        assert!(relay.h1_exemption(honest, &[fresh.id()], true), "the honest beat still buys it");
    }

    /// The exemption memory is bounded, oldest carrier forgotten first.
    #[test]
    fn the_exemption_memory_is_bounded() {
        let relay = PalwHeartbeatRelayV1::default();
        for n in 0..(2 * PALW_HEARTBEAT_H1_MAX_CLAIMS as u64) {
            assert!(relay.h1_exemption(BlockHash::from_u64_word(n), &[TransactionId::from_u64_word(n)], true));
        }
        let inner = relay.inner.lock().unwrap();
        assert_eq!(inner.h1_claims.len(), PALW_HEARTBEAT_H1_MAX_CLAIMS);
        assert_eq!(inner.h1_claim_order.len(), PALW_HEARTBEAT_H1_MAX_CLAIMS);
        assert!(!inner.h1_claims.contains_key(&TransactionId::from_u64_word(0)), "the oldest is forgotten");
    }
}
