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
//! drop a heartbeat for carrying them". So a beat that carries an H-1 carrier is downloaded past an
//! empty allowance and announced although another beat holds its slot — but only on the strength
//! of a carrier that is **pending**: in this node's mempool (fee paid, scripts and inputs checked,
//! and — the node's H-1 gate — a carrier the tip's fold would take) and not already spent on
//! another beat's exemption. [`palw_heartbeat_h1_exemption_v1`] is the whole decision, as a pure
//! function the flows call and the tests drive:
//!
//! 1. **The gate:** a heartbeat, where R-core+ is in force; nothing else is ever spared.
//! 2. **The beat's own proof of work** ([`palw_heartbeat_pow_passes_v1`], two hashes) — before any
//!    body is read, so a junk beat costs its sender the lane's 2^24 (P2-9 review, finding 4).
//! 3. **At most [`PALW_H1_CARRIERS_EXAMINED_PER_BEAT_V1`] lifecycle payloads decoded**
//!    (`palw_h1_carrier_candidates_v1`: a lane-built beat carries its carriers first).
//! 4. **One mempool query**, under one lock, for those candidates.
//! 5. **The claim ledger** ([`PalwHeartbeatRelayV1::h1_exemption`]): a carrier buys ONE validated
//!    beat. A beat asks before it is validated and claims once it is — the one the allowance spared,
//!    whatever its slot verdict (review finding 3: a `First` beat used to leave its carrier unspent,
//!    so one pending carrier exempted every beat that copied it), and a `Repeat` at its
//!    announcement. A beat that fails validation never claims, so a forged block cannot burn an
//!    honest carrier's exemption.
//!
//! An unconditional exemption would reopen the flood H2 closed, since a 0x4b payload costs nothing
//! to put in a block; a pending carrier costs a relay fee, the gate's fold check and buys one beat
//! — and an honest carrier-bearing beat almost always qualifies, because a carrier travels as a
//! transaction before any miner includes it. One that does not (this node never saw the carrier)
//! falls back to H2's own escape, the orphan-root fetch.

use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::palw_heartbeat_carriers_v1::{PALW_H1_CARRIERS_EXAMINED_PER_BEAT_V1, palw_h1_carrier_candidates_v1};
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
    /// `claim` spends every open carrier on `block`; without it the answer comes at the first open
    /// one and nothing is written. Only a VALIDATED block may claim (see the module doc, step 5).
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
                None => return true,
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

/// **Does this heartbeat header meet the lane's proof of work?** The constant-target check every
/// node runs in header validation (`StateLayer0` prices algo-8 at `PALW_HEARTBEAT_WORK_LOG2`), asked
/// by the H-1 exemption before it reads a body a peer sent: two hashes, so a beat the exemption
/// looks into cost its sender the lane's work (P2-9 review, finding 4). The network id bytes are
/// consensus's own (`params.net`'s string form).
pub fn palw_heartbeat_pow_passes_v1(params: &Params, header: &Header) -> bool {
    matches!(kaspa_pow::StateLayer0::new(header, params.net.to_string().as_bytes()).check_pow_layer0(header.nonce), Ok((true, _)))
}

/// **ADR-0152 v3.1 H-1's relay half: is `block` spared H2's two rules?** The whole decision (the
/// module doc's steps 1–5), pure over its inputs so a test can drive every path:
///
/// * `pow_passes` — [`palw_heartbeat_pow_passes_v1`] in the node;
/// * `pooled` — for the candidate ids, whether each sits in this node's transaction pool, asked
///   ONCE (`MiningManager::has_pooled_transactions`);
/// * `claim` — spend the open carriers on this beat (a validated beat) or only ask (a beat about to
///   be downloaded).
///
/// `false` for anything that is not a heartbeat where R-core+ is in force, for a header that fails
/// the lane's work, for a beat with no candidate, and when every pending carrier already bought
/// another beat.
pub fn palw_heartbeat_h1_exemption_v1(
    params: &Params,
    block: &Block,
    relay: &PalwHeartbeatRelayV1,
    pow_passes: impl FnOnce(&Params, &Header) -> bool,
    pooled: impl FnOnce(&[TransactionId]) -> Vec<bool>,
    claim: bool,
) -> bool {
    if block.header.pow_algo_id != POW_ALGO_ID_HEARTBEAT_V1 || !params.palw_rcore_plus_active_at(block.header.daa_score) {
        return false;
    }
    if !pow_passes(params, &block.header) {
        return false;
    }
    let candidates = palw_h1_carrier_candidates_v1(&block.transactions);
    debug_assert!(candidates.len() <= PALW_H1_CARRIERS_EXAMINED_PER_BEAT_V1);
    if candidates.is_empty() {
        return false;
    }
    let in_pool = pooled(&candidates);
    let pending: Vec<TransactionId> = candidates.into_iter().zip(in_pool).filter_map(|(id, pending)| pending.then_some(id)).collect();
    relay.h1_exemption(block.hash(), &pending, claim)
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
    /// The ledger alone: the flow composes it as `budget.take(now) || exempt(ask)` at download, a
    /// claim once a spared beat validates, and `observe → Repeat → exempt(claim)` at announcement,
    /// where `exempt` is the block's pending H-1 carriers put to [`PalwHeartbeatRelayV1::h1_exemption`]
    /// (the whole decision is [`palw_heartbeat_h1_exemption_v1`], tested below).
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

    fn t12() -> Params {
        use kaspa_consensus_core::network::{NetworkId, NetworkType};
        let params = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
        assert!(params.palw_rcore_plus_active_at(0), "testnet-12 arms R-core+ from genesis");
        params
    }

    /// A heartbeat at `daa` (its hash is `nonce`) carrying `txs`.
    fn beat_block(daa: u64, nonce: u64, txs: Vec<kaspa_consensus_core::tx::Transaction>) -> Block {
        Block::new(beat(daa, nonce).1, txs)
    }

    fn accused(claim: u64) -> kaspa_consensus_core::tx::Transaction {
        use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
        use kaspa_hashes::Hash64;
        lifecycle(
            PalwConsensusObjectV2::DefaultAccused {
                claim: Hash64::from_u64_word(claim),
                missing_event_index: 0,
                accuser: PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(Hash64::from_u64_word(2), 0)),
                signature: vec![1; 8],
            },
            claim,
        )
    }

    fn pow_ok(_: &Params, _: &Header) -> bool {
        true
    }

    fn all_pooled(ids: &[TransactionId]) -> Vec<bool> {
        vec![true; ids.len()]
    }

    /// **T38's relay half, the decision itself (P2-9 review, finding 6)**: the gate, the beat's own
    /// work, the pending check and the claim, each on its own — `palw_heartbeat_h1_exemption_v1` is
    /// what `FlowContext::palw_heartbeat_h1_exempt` runs, so this drives the node's decision, not a
    /// stand-in for it.
    #[test]
    fn the_h1_exemption_decides_on_the_gate_the_work_the_pool_and_the_ledger() {
        let params = t12();
        let relay = PalwHeartbeatRelayV1::default();
        let bearing = beat_block(500, 1, vec![accused(1)]);
        let decide =
            |params: &Params, block: &Block, pow: fn(&Params, &Header) -> bool, pooled: fn(&[TransactionId]) -> Vec<bool>, claim| {
                palw_heartbeat_h1_exemption_v1(params, block, &relay, pow, pooled, claim)
            };
        fn never_asked(_: &[TransactionId]) -> Vec<bool> {
            panic!("the mempool is not asked")
        }

        // The gate: below R-core+ (testnet-11) and for a block that is not a heartbeat, nothing is
        // spared and nothing is read.
        use kaspa_consensus_core::network::{NetworkId, NetworkType};
        let t11 = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 11));
        assert!(!decide(&t11, &bearing, pow_ok, never_asked, false), "no R-core+, no exemption");
        let mut committed = beat(500, 9).1;
        committed.pow_algo_id = kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2;
        assert!(!decide(&params, &Block::new(committed, vec![accused(1)]), pow_ok, never_asked, false), "only heartbeats");

        // The work comes before the body: a header without the lane's work is refused unread.
        assert!(!decide(&params, &bearing, |_, _| false, never_asked, false), "no work, no look");

        // Pending: a carrier this node does not hold spares nothing (the orphan-root route remains).
        assert!(!decide(&params, &bearing, pow_ok, |ids| vec![false; ids.len()], false), "not in the pool");
        // A body with no H-1 carrier asks nothing either.
        assert!(!decide(&params, &beat_block(500, 2, vec![]), pow_ok, never_asked, false), "no carrier, no query");

        // Asking never spends; claiming spends the carrier on this beat, and only this beat.
        assert!(decide(&params, &bearing, pow_ok, all_pooled, false), "asked");
        assert!(decide(&params, &bearing, pow_ok, all_pooled, false), "asked again: nothing was spent");
        assert!(decide(&params, &bearing, pow_ok, all_pooled, true), "claimed by the validated beat");
        assert!(decide(&params, &bearing, pow_ok, all_pooled, true), "one beat, one answer");
        let copy = beat_block(500, 3, vec![accused(1)]);
        assert!(!decide(&params, &copy, pow_ok, all_pooled, false), "a copy in another beat buys nothing");
    }

    /// **The review's probe D, closed (finding 3)**: one peer sends five hundred beats, each the
    /// FIRST for its own slot, all carrying one pending carrier, after its allowance is spent. The
    /// flow asks at download and claims once the beat validates — whatever its slot verdict — so the
    /// carrier spares one beat and the other 499 are dropped at the allowance, as H2 intends.
    #[test]
    fn a_pending_carrier_spares_one_beat_not_every_beat_that_copies_it() {
        let params = t12();
        let relay = PalwHeartbeatRelayV1::default();
        let t0 = 1_790_000_000_000u64;
        let mut budget = PalwHeartbeatInvBudgetV1::new(t0);
        while budget.take(t0) {}
        let carrier = accused(1);
        let mut spared = 0;
        for n in 0..500u64 {
            let block = beat_block(1_000 + 2 * n, 10_000 + n, vec![carrier.clone()]);
            // Download: `budget.take(now) || exempt(ask)` (`HandleRelayInvsFlow`).
            if budget.take(t0) || !palw_heartbeat_h1_exemption_v1(&params, &block, &relay, pow_ok, all_pooled, false) {
                continue;
            }
            spared += 1;
            // Validated: the spared beat spends its carrier, although it is First for its slot.
            assert!(palw_heartbeat_h1_exemption_v1(&params, &block, &relay, pow_ok, all_pooled, true));
            assert_eq!(relay.observe(block.hash(), &block.header, 999 + 2 * n), PalwHeartbeatRelayVerdictV1::First);
        }
        assert_eq!(spared, 1, "one pending carrier, one spared beat");
    }

    /// **The announcement composition**: a spared beat that turns out a `Repeat` for its slot is
    /// still announced on the carrier it already claimed, and a later `Repeat` carrying the same
    /// carrier is not.
    #[test]
    fn a_spared_repeat_is_announced_on_its_own_claim_and_no_other() {
        let params = t12();
        let relay = PalwHeartbeatRelayV1::default();
        let (first, first_header) = beat(500, 1);
        assert_eq!(relay.observe(first, &first_header, 500), PalwHeartbeatRelayVerdictV1::First);
        let spared = beat_block(500, 2, vec![accused(1)]);
        assert!(palw_heartbeat_h1_exemption_v1(&params, &spared, &relay, pow_ok, all_pooled, true), "claimed once validated");
        // `palw_heartbeat_relay_admits`: observe → Repeat → exempt(claim).
        assert_eq!(relay.observe(spared.hash(), &spared.header, 500), PalwHeartbeatRelayVerdictV1::Repeat);
        assert!(palw_heartbeat_h1_exemption_v1(&params, &spared, &relay, pow_ok, all_pooled, true), "announced");
        let late = beat_block(500, 3, vec![accused(1)]);
        assert_eq!(relay.observe(late.hash(), &late.header, 500), PalwHeartbeatRelayVerdictV1::Repeat);
        assert!(!palw_heartbeat_h1_exemption_v1(&params, &late, &relay, pow_ok, all_pooled, true), "kept, not announced");
    }

    /// **Bounded work on a body nobody has validated (finding 4)**: fifty carriers in a beat cost at
    /// most eight decodes and ONE mempool query.
    #[test]
    fn the_mempool_is_asked_once_about_a_bounded_head_of_the_beat() {
        let params = t12();
        let relay = PalwHeartbeatRelayV1::default();
        let flood = beat_block(500, 1, (1..=50).map(accused).collect());
        let calls = std::cell::Cell::new(0);
        let exempt = palw_heartbeat_h1_exemption_v1(
            &params,
            &flood,
            &relay,
            pow_ok,
            |ids| {
                calls.set(calls.get() + 1);
                assert!(ids.len() <= PALW_H1_CARRIERS_EXAMINED_PER_BEAT_V1, "{} ids", ids.len());
                vec![false; ids.len()]
            },
            false,
        );
        assert!(!exempt && calls.get() == 1, "one query, and a head none of which is pending spares nothing");
    }

    /// The node's work check is the lane's own: a heartbeat header nobody mined does not pass it.
    #[test]
    fn an_unmined_heartbeat_header_fails_the_lanes_work() {
        assert!(!palw_heartbeat_pow_passes_v1(&t12(), &beat(500, 1).1));
    }

    /// **The flows run the decision where H2 would drop a beat, in the right order** — pinned on the
    /// source, since no unit test drives a live relay flow: the allowance gate asks before the
    /// block is validated, the spared beat claims after it, a `Repeat` claims at its announcement,
    /// and the node's wrapper runs this module's decision with the real work check and ONE batched
    /// mempool query (finding 6: deleting any of these used to leave every test green).
    #[test]
    fn the_flows_ask_before_validation_claim_after_it_and_run_this_decision() {
        let flow = include_str!("v7/blockrelay/flow.rs");
        let ask = flow.find("self.ctx.palw_heartbeat_h1_exempt(&block, false).await").expect("the allowance gate asks");
        let validated = flow.find("block_task.await").expect("the flow validates the block");
        let claim = flow.find("self.ctx.palw_heartbeat_h1_exempt(&block, true).await").expect("a spared beat claims");
        assert!(ask < validated && validated < claim, "ask, validate, then claim");

        let ctx = include_str!("flow_context.rs");
        let body_of = |name: &str| -> &'static str {
            let start = ctx.find(&format!("pub async fn {name}(")).unwrap_or_else(|| panic!("{name} exists"));
            let rest = &ctx[start..];
            &rest[..rest[1..].find("\n    pub ").map(|i| i + 1).unwrap_or(rest.len())]
        };
        let admits = body_of("palw_heartbeat_relay_admits");
        let repeat = admits.find("V::Repeat").expect("the Repeat arm");
        assert!(admits[repeat..].contains("self.palw_heartbeat_h1_exempt(block, true).await"), "a Repeat claims at its announcement");
        let exempt = body_of("palw_heartbeat_h1_exempt");
        for piece in [
            "palw_heartbeat_h1_exemption_v1(",
            "crate::palw_heartbeat_relay::palw_heartbeat_pow_passes_v1",
            "has_pooled_transactions_blocking(",
            "spawn_blocking(",
        ] {
            assert!(exempt.contains(piece), "the wrapper runs {piece}");
        }
    }
}
