//! **ADR-0125 §7.3: one block a permit on the wire, and the evidence when there are two.**
//!
//! A permit is one round block. The chain accepts one per permit and burns a permit proven signed
//! twice; this is the transport's half. A node relays the first round block it validates for a
//! `(round, permit index, bond)` and not a second one — the block is still stored and can still be
//! merged, it is just not announced onward, so one equivocating holder cannot fill every peer's DAG
//! with its copies. When the second block is a different signed block (not the same header signed
//! again), the two are equivocation evidence, queued once per permit for a panel with a carrier to
//! file (`PalwConsensusObjectV2::RoundPermitEquivocated`).
//!
//! Bounded: at most [`PALW_ROUND_RELAY_MAX_PERMITS`] permits remembered, pruned oldest round first
//! beyond [`PALW_ROUND_RELAY_KEEP_ROUNDS`] of the newest, and at most
//! [`PALW_ROUND_RELAY_MAX_EVIDENCE`] evidence waiting.

use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::palw_execution_lane_v1::{
    PALW_EXEC_EQUIVOCATION_VERSION_V1, PalwExecEnvelopeV1, PalwExecEquivocationV1, PalwExecSignedRoundV1,
};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_ROUND_V1;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Mutex;

/// The most permits the relay remembers.
pub const PALW_ROUND_RELAY_MAX_PERMITS: usize = 65_536;
/// How many rounds behind the newest one a permit is remembered for once the map is full.
pub const PALW_ROUND_RELAY_KEEP_ROUNDS: u64 = 7_200;
/// The most evidence waiting for a carrier.
pub const PALW_ROUND_RELAY_MAX_EVIDENCE: usize = 64;

/// What the relay policy says about a validated block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwRoundRelayVerdictV1 {
    /// Not a round block (or one whose envelope does not decode): no opinion — relay as any block.
    NotRound,
    /// The first block seen for its permit: relay it.
    First,
    /// The same signed block seen again, or the same header signed again: do not relay.
    Repeat,
    /// A different signed block for a permit already seen: do not relay, and the pair is evidence.
    Equivocation,
}

type PermitKey = (u64, u16, PalwBondKeyV2);

struct Seen {
    block: BlockHash,
    span: u64,
    side: PalwExecSignedRoundV1,
}

#[derive(Default)]
struct Inner {
    seen: BTreeMap<PermitKey, Seen>,
    by_round: BTreeMap<u64, Vec<PermitKey>>,
    newest_round: u64,
    reported: BTreeSet<PermitKey>,
    evidence: VecDeque<PalwExecEquivocationV1>,
}

/// The relay's memory of round blocks by permit.
#[derive(Default)]
pub struct PalwRoundRelayV1 {
    inner: Mutex<Inner>,
}

impl PalwRoundRelayV1 {
    /// Observe a validated block. `anchor_span` is the span of the block's selected parent — the
    /// schedule its permit is judged by, which the evidence names.
    pub fn observe(&self, block: BlockHash, header: &Header, anchor_span: u64) -> PalwRoundRelayVerdictV1 {
        if header.pow_algo_id != POW_ALGO_ID_PALW_ROUND_V1 {
            return PalwRoundRelayVerdictV1::NotRound;
        }
        let Ok(envelope) = PalwExecEnvelopeV1::decode(&header.palw_commitment) else {
            return PalwRoundRelayVerdictV1::NotRound;
        };
        let mut bare = header.clone();
        bare.palw_commitment = Vec::new();
        let side =
            envelope.signed_round_v1(kaspa_consensus_core::hashing::header::pre_pow_hash_64(&bare), header.timestamp, header.nonce);
        let key: PermitKey = (envelope.round, envelope.permit_index, envelope.bond);
        let mut inner = self.inner.lock().expect("the round relay lock is never poisoned by a panic while held");
        inner.newest_round = inner.newest_round.max(envelope.round);
        let facts = |s: &PalwExecSignedRoundV1| (s.pre_pow_hash, s.timestamp_ms, s.nonce);
        if let Some(seen) = inner.seen.get(&key) {
            if seen.block == block || facts(&seen.side) == facts(&side) {
                return PalwRoundRelayVerdictV1::Repeat;
            }
            let evidence = PalwExecEquivocationV1 {
                version: PALW_EXEC_EQUIVOCATION_VERSION_V1,
                span: seen.span,
                round: envelope.round,
                permit_index: envelope.permit_index,
                bond: envelope.bond,
                first: seen.side.clone(),
                second: side,
            };
            if !inner.reported.contains(&key) && inner.evidence.len() < PALW_ROUND_RELAY_MAX_EVIDENCE {
                inner.reported.insert(key);
                inner.evidence.push_back(evidence);
            }
            return PalwRoundRelayVerdictV1::Equivocation;
        }
        inner.seen.insert(key, Seen { block, span: anchor_span, side });
        inner.by_round.entry(envelope.round).or_default().push(key);
        if inner.seen.len() > PALW_ROUND_RELAY_MAX_PERMITS {
            let floor = inner.newest_round.saturating_sub(PALW_ROUND_RELAY_KEEP_ROUNDS);
            // Oldest rounds first, and at least one round even when every round is recent, so the
            // bound holds whatever the rounds are.
            while inner.seen.len() > PALW_ROUND_RELAY_MAX_PERMITS / 2 || inner.by_round.keys().next().is_some_and(|r| *r < floor) {
                let Some((_, keys)) = inner.by_round.pop_first() else { break };
                for key in keys {
                    inner.seen.remove(&key);
                    inner.reported.remove(&key);
                }
            }
        }
        PalwRoundRelayVerdictV1::First
    }

    /// The oldest evidence waiting for a carrier.
    pub fn take_evidence(&self) -> Option<PalwExecEquivocationV1> {
        self.inner.lock().expect("the round relay lock is never poisoned by a panic while held").evidence.pop_front()
    }

    /// Put evidence back at the front — a carrier that could not take it this tick.
    pub fn return_evidence(&self, evidence: PalwExecEquivocationV1) {
        let mut inner = self.inner.lock().expect("the round relay lock is never poisoned by a panic while held");
        if inner.evidence.len() < PALW_ROUND_RELAY_MAX_EVIDENCE {
            inner.evidence.push_front(evidence);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_execution_lane_v1::{PALW_EXEC_ENVELOPE_VERSION_V1, PALW_EXEC_MLDSA87_SIGNATURE_LEN};
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
    use kaspa_hashes::Hash64;

    fn round_header(round: u64, index: u16, bond: u64, nonce: u64, signature_byte: u8) -> Header {
        let mut header = Header::from_precomputed_hash(Hash64::from_u64_word(round * 1_000 + nonce), Vec::new());
        header.pow_algo_id = POW_ALGO_ID_PALW_ROUND_V1;
        header.timestamp = round * 1_000;
        header.nonce = nonce;
        header.palw_commitment = PalwExecEnvelopeV1 {
            version: PALW_EXEC_ENVELOPE_VERSION_V1,
            network_domain: Hash64::from_u64_word(1),
            round,
            permit_index: index,
            bond: PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(bond), 0)),
            pubkey: vec![0; 8],
            signature: vec![signature_byte; PALW_EXEC_MLDSA87_SIGNATURE_LEN],
        }
        .encode();
        header
    }

    #[test]
    fn one_block_a_permit_is_relayed_and_a_second_signed_block_is_evidence_once() {
        let relay = PalwRoundRelayV1::default();
        let a = round_header(10, 0, 7, 1, 1);
        assert_eq!(relay.observe(Hash64::from_u64_word(1), &a, 3), PalwRoundRelayVerdictV1::First);
        assert_eq!(relay.observe(Hash64::from_u64_word(1), &a, 3), PalwRoundRelayVerdictV1::Repeat, "the same block again");
        let resigned = round_header(10, 0, 7, 1, 2);
        assert_eq!(
            relay.observe(Hash64::from_u64_word(2), &resigned, 3),
            PalwRoundRelayVerdictV1::Repeat,
            "the same header signed again is not a second block"
        );
        assert!(relay.take_evidence().is_none());
        let b = round_header(10, 0, 7, 2, 1);
        assert_eq!(relay.observe(Hash64::from_u64_word(3), &b, 3), PalwRoundRelayVerdictV1::Equivocation);
        let c = round_header(10, 0, 7, 3, 1);
        assert_eq!(relay.observe(Hash64::from_u64_word(4), &c, 3), PalwRoundRelayVerdictV1::Equivocation, "not relayed either");
        let evidence = relay.take_evidence().expect("the pair is evidence");
        assert_eq!(
            (evidence.span, evidence.round, evidence.permit_index, evidence.first.nonce, evidence.second.nonce),
            (3, 10, 0, 1, 2)
        );
        assert!(relay.take_evidence().is_none(), "once per permit");
        assert_eq!(
            relay.observe(Hash64::from_u64_word(5), &round_header(10, 1, 7, 1, 1), 3),
            PalwRoundRelayVerdictV1::First,
            "another index"
        );
        assert_eq!(
            relay.observe(Hash64::from_u64_word(6), &round_header(10, 0, 8, 1, 1), 3),
            PalwRoundRelayVerdictV1::First,
            "another bond"
        );
        let mut chain_block = round_header(10, 0, 7, 9, 1);
        chain_block.pow_algo_id = 9;
        assert_eq!(relay.observe(Hash64::from_u64_word(7), &chain_block, 3), PalwRoundRelayVerdictV1::NotRound);
    }

    #[test]
    fn the_memory_is_bounded_oldest_round_first() {
        let relay = PalwRoundRelayV1::default();
        for i in 0..(PALW_ROUND_RELAY_MAX_PERMITS as u64 + 1) {
            let header = round_header(i / 8, (i % 8) as u16, i, 1, 1);
            assert_eq!(relay.observe(Hash64::from_u64_word(i + 1), &header, 0), PalwRoundRelayVerdictV1::First);
        }
        let inner = relay.inner.lock().unwrap();
        assert!(inner.seen.len() <= PALW_ROUND_RELAY_MAX_PERMITS / 2 + 8, "pruned to half: {}", inner.seen.len());
        assert!(inner.by_round.keys().next().copied().unwrap_or(0) > 0, "the oldest rounds left first");
    }
}
