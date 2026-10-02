//! **RFC-0007 Part I, the node's side of the verification vertex** (node policy; a child of `palw_panel`). Nothing here is a rule: the
//! fold judges whatever arrives, and below `Params::palw_verification_vertex_v1` none of this runs — a seat files receipts exactly as it
//! always did.
//!
//! * **A seat builds one vertex a round from the verdicts it reached** ([`PalwVertexBookV1`]): each verdict the duty loop would have
//!   signed as a receipt becomes a leaf; once a round has passed, the pending leaves are sealed into one vertex — sorted, rooted,
//!   signed once with the seat's ML-DSA-87 key — and queued to be carried. **A seat never signs two vertices for one round**: the last
//!   sealed round is persisted before the vertex leaves the node, and a restart refuses to seal a round at or below it (two vertices of
//!   a round are an equivocation, slashed 100 ‰, the locks forfeited and the bond ejected).
//! * **A vertex rides once, as an ordinary lifecycle carrier**: it goes out through the panel's own carrier slots (the `OwnReceipts`
//!   site), every node relays it as any transaction, and any block template includes it. There is no collector and no pool of receipts
//!   to assemble, and nothing for a collector to duplicate.
//! * **A vertex is re-sent until the chain has it.** The seat reads its own `(round, root)` rows at the tip; a vertex whose row is there
//!   has landed, one that has waited longer than a vertex may wait ([`PALW_VERTEX_MAX_CARRY_DAA_V1`]) is dropped and its leaves go back
//!   into the next round's vertex (a verdict that has not counted yet still stands to be said).
//! * **Which claims a seat answers this way** ([`palw_vertex_claim_by_leaf_v1`]): those whose panel bound at or after the fence — the
//!   chain's path rule (`palw_vertex_claim_licenses_by_tally_v1`); a claim bound before it keeps the receipt path until it is licensed
//!   or void. References are compact (a bound DAA and a 16-byte prefix) unless the operator asks for whole ids.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use kaspa_consensus_core::palw_panel_v2::PalwReceiptVerdictV2;
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
use kaspa_consensus_core::palw_vertex_v1::{
    PALW_VERTEX_MAX_CARRY_DAA_V1, PALW_VERTEX_MAX_LEAF_BYTES_V1, PALW_VERTEX_MAX_LEAVES_V1, PALW_VERTEX_MLDSA87_CONTEXT_V1,
    PALW_VERTEX_ROUND_DAA_V1, PalwClaimRefV1, PalwVerificationVertexV1, PalwVertexLeafV1,
};
use kaspa_hashes::Hash64;

/// How long a sent vertex waits for its row before the carrier is built again: a few DAA, the lane's replan interval.
pub(crate) const PALW_VERTEX_RESEND_DAA_V1: u64 = 20;

/// **The leaf a seat's verdict on a claim becomes**, with the claim named as the operator chose: compact (the bound DAA of its panel and
/// a 16-byte prefix) where the bound DAA fits, whole otherwise.
pub(crate) fn palw_vertex_claim_by_leaf_v1(claim: &Hash64, bound_daa: u64, full_refs: bool) -> PalwClaimRefV1 {
    if full_refs {
        return PalwClaimRefV1::Full(*claim);
    }
    PalwClaimRefV1::compact_of(claim, bound_daa).unwrap_or(PalwClaimRefV1::Full(*claim))
}

/// The verdict leaf for `claim`.
pub(crate) fn palw_vertex_verdict_leaf_v1(
    claim: &Hash64,
    bound_daa: u64,
    verdict: PalwReceiptVerdictV2,
    full_refs: bool,
) -> PalwVertexLeafV1 {
    PalwVertexLeafV1::Verdict { claim: palw_vertex_claim_by_leaf_v1(claim, bound_daa, full_refs), verdict }
}

/// The chunk size a `Held` attestation of a capture counts in: the capture is attested as chunks `0..=⌈len / chunk⌉ − 1`.
pub(crate) const PALW_VERTEX_HELD_CHUNK_BYTES_V1: usize = 65_536;

/// The domain of a `Held` capture digest (node policy: the chain compares attesters' digests, it does not recompute one).
const PALW_VERTEX_HELD_DIGEST_DOMAIN_V1: &[u8] = b"misaka-node/vertex-held-capture/v1";

/// **The `Held` leaf of a capture this seat holds** (RFC-0007 §I.7): "I hold chunks `0..=last` of this claim's capture, whose digest is
/// `digest`, and I will serve them until its challenge window closes." Voluntary — a seat that verified a claim's material and kept it
/// (`persist_foreign_material`) says so; three equal leaves are a DA certificate. `None` for an empty capture.
pub(crate) fn palw_vertex_held_capture_leaf_v1(
    claim: &Hash64,
    bound_daa: u64,
    bytes: &[u8],
    full_refs: bool,
) -> Option<PalwVertexLeafV1> {
    if bytes.is_empty() {
        return None;
    }
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_VERTEX_HELD_DIGEST_DOMAIN_V1).to_state();
    state.update(bytes);
    let mut digest = [0u8; 64];
    digest.copy_from_slice(state.finalize().as_bytes());
    let last = u32::try_from(bytes.len().div_ceil(PALW_VERTEX_HELD_CHUNK_BYTES_V1) - 1).ok()?;
    Some(PalwVertexLeafV1::Held {
        claim: palw_vertex_claim_by_leaf_v1(claim, bound_daa, full_refs),
        object: kaspa_consensus_core::palw_vertex_v1::PALW_VERTEX_HELD_OBJECT_CAPTURE_V1,
        first: 0,
        last,
        digest: Hash64::from_bytes(digest),
    })
}

/// A vertex this seat sealed and has not seen land.
#[derive(Clone, Debug)]
pub(crate) struct PalwSealedVertexV1 {
    pub vertex: PalwVerificationVertexV1,
    /// The DAA the carrier last went to the mempool at, if it has.
    pub sent_daa: Option<u64>,
}

/// **The seat's vertex book**: the leaves waiting for a round, the sealed vertices waiting to land, and the counters the status reads.
#[derive(Debug, Default)]
pub(crate) struct PalwVertexBookV1 {
    pending: BTreeMap<Vec<u8>, PalwVertexLeafV1>,
    sealed: BTreeMap<u64, PalwSealedVertexV1>,
    /// The highest round this seat has signed — persisted, and the only guard against signing one twice.
    last_sealed_round: Option<u64>,
    pub sealed_total: u64,
    pub sent_total: u64,
    pub landed_total: u64,
    pub expired_total: u64,
    pub leaves_total: u64,
    /// DRILL ONLY: a second vertex of a round, signed on purpose (`--palw-drill-vertex-equivocate-at`), waiting to be carried once.
    drill_extra: Option<PalwSealedVertexV1>,
}

impl PalwVertexBookV1 {
    /// A book that continues from the last round a previous run signed (`None` for a fresh state dir).
    pub(crate) fn resume(last_sealed_round: Option<u64>) -> Self {
        Self { last_sealed_round, ..Default::default() }
    }

    pub(crate) fn last_sealed_round(&self) -> Option<u64> {
        self.last_sealed_round
    }

    pub(crate) fn pending_leaves(&self) -> usize {
        self.pending.len()
    }

    pub(crate) fn sealed_waiting(&self) -> usize {
        self.sealed.len()
    }

    /// Record a leaf. **The first verdict a seat reaches on a claim is its verdict** (PALW-VC-4), so a leaf for a claim already pending
    /// or already sealed is not replaced.
    pub(crate) fn record(&mut self, leaf: PalwVertexLeafV1) {
        let key = leaf.sort_key();
        if self.sealed.values().any(|sealed| sealed.vertex.leaves.iter().any(|held| held.sort_key() == key)) {
            return;
        }
        self.pending.entry(key).or_insert(leaf);
    }

    /// **Seal the pending leaves into this round's vertex**, if there are any and the round has not been signed (this process or a
    /// previous one). At most one vertex a round, at most [`PALW_VERTEX_MAX_LEAVES_V1`] leaves and [`PALW_VERTEX_MAX_LEAF_BYTES_V1`] bytes
    /// of them: what does not fit stays pending for the next round. `persist` is called with the round **before** the vertex is handed
    /// out, and a failure to persist seals nothing (a seat that cannot remember a round must not risk signing it twice).
    pub(crate) fn seal(
        &mut self,
        network_domain: Hash64,
        seat: PalwBondKeyV2,
        now_daa: u64,
        sign: &dyn Fn(&[u8], &[u8]) -> Option<Vec<u8>>,
        persist: &dyn Fn(u64) -> bool,
    ) -> Option<u64> {
        let round = now_daa / PALW_VERTEX_ROUND_DAA_V1;
        if self.pending.is_empty() || self.last_sealed_round.is_some_and(|last| last >= round) {
            return None;
        }
        let mut take: Vec<Vec<u8>> = Vec::new();
        let mut bytes = 0usize;
        for (key, leaf) in &self.pending {
            let len = leaf.encoded_len();
            if take.len() >= PALW_VERTEX_MAX_LEAVES_V1 || bytes + len > PALW_VERTEX_MAX_LEAF_BYTES_V1 {
                break;
            }
            bytes += len;
            take.push(key.clone());
        }
        let leaves: Vec<PalwVertexLeafV1> = take.iter().filter_map(|key| self.pending.get(key).copied()).collect();
        let vertex =
            PalwVerificationVertexV1::sign_v1(network_domain, seat, now_daa, leaves, |message, context| sign(message, context))?;
        if !persist(round) {
            return None;
        }
        for key in &take {
            self.pending.remove(key);
        }
        self.last_sealed_round = Some(round);
        self.sealed_total += 1;
        self.leaves_total += vertex.leaves.len() as u64;
        self.sealed.insert(round, PalwSealedVertexV1 { vertex, sent_daa: None });
        Some(round)
    }

    /// **The sealed vertex to carry next**: the oldest one that has not been sent, or whose last carrier has waited
    /// [`PALW_VERTEX_RESEND_DAA_V1`] without its row appearing.
    pub(crate) fn next_to_send(&self, now_daa: u64) -> Option<&PalwVerificationVertexV1> {
        self.sealed
            .values()
            .find(|sealed| sealed.sent_daa.is_none_or(|at| now_daa >= at.saturating_add(PALW_VERTEX_RESEND_DAA_V1)))
            .map(|sealed| &sealed.vertex)
    }

    pub(crate) fn mark_sent(&mut self, round: u64, now_daa: u64) {
        if let Some(sealed) = self.sealed.get_mut(&round) {
            if sealed.sent_daa.is_none() {
                self.sent_total += 1;
            }
            sealed.sent_daa = Some(now_daa);
        }
    }

    /// **Reconcile with the chain**: `rows` are this seat's `(round, leaves_root)` rows at the tip. A sealed vertex whose row is there
    /// has landed and is forgotten. One that has waited past [`PALW_VERTEX_MAX_CARRY_DAA_V1`] without landing is expired — the chain
    /// would refuse it now — and its leaves return to pending, to be said again in a later round. Returns the rounds that landed.
    pub(crate) fn reconcile(&mut self, rows: &[(u64, Hash64)], now_daa: u64) -> Vec<u64> {
        let mut landed = Vec::new();
        let mut expired = Vec::new();
        for (round, sealed) in &self.sealed {
            if rows.iter().any(|(at, root)| at == round && *root == sealed.vertex.leaves_root) {
                landed.push(*round);
            } else if now_daa.saturating_sub(sealed.vertex.signed_daa) > PALW_VERTEX_MAX_CARRY_DAA_V1 {
                expired.push(*round);
            }
        }
        for round in &landed {
            self.sealed.remove(round);
            self.landed_total += 1;
        }
        for round in expired {
            if let Some(sealed) = self.sealed.remove(&round) {
                self.expired_total += 1;
                for leaf in sealed.vertex.leaves {
                    self.pending.entry(leaf.sort_key()).or_insert(leaf);
                }
            }
        }
        landed
    }

    /// The vertex sealed for `round`, if it is still waiting to land.
    pub(crate) fn sealed_vertex(&self, round: u64) -> Option<&PalwVerificationVertexV1> {
        self.sealed.get(&round).map(|sealed| &sealed.vertex)
    }

    /// DRILL ONLY: queue a deliberate second vertex of a round to be carried once, after the first.
    pub(crate) fn drill_extra(&mut self, vertex: PalwVerificationVertexV1) {
        self.drill_extra = Some(PalwSealedVertexV1 { vertex, sent_daa: None });
    }

    /// DRILL ONLY: the deliberate second vertex, once, when the first has gone out.
    pub(crate) fn take_drill_extra(&mut self) -> Option<PalwVerificationVertexV1> {
        self.drill_extra.take().map(|sealed| sealed.vertex)
    }

    /// The `key=value` pairs the node status carries (`getPalwNodeStatus`'s `verification` line).
    pub(crate) fn status_v1(&self, fence_from_daa: Option<u64>, tip_rounds: u64, tip_tallies: u64, tip_held_claims: u64) -> String {
        format!(
            "vertex_fence={} vertex_pending={} vertex_sealed_waiting={} vertex_sealed={} vertex_sent={} vertex_landed={} vertex_expired={} \
             vertex_leaves={} vertex_last_round={} vertex_tip_rounds={tip_rounds} vertex_tip_tallies={tip_tallies} vertex_tip_held={tip_held_claims}",
            fence_from_daa.map(|at| at.to_string()).unwrap_or_else(|| "off".to_string()),
            self.pending.len(),
            self.sealed.len(),
            self.sealed_total,
            self.sent_total,
            self.landed_total,
            self.expired_total,
            self.leaves_total,
            self.last_sealed_round.map(|round| round.to_string()).unwrap_or_else(|| "none".to_string()),
        )
    }
}

/// **The persisted last-sealed round**: a plain decimal in the state dir. `None` for a fresh dir or an unreadable file — and an
/// unreadable file is *not* a licence to sign from round zero: the caller seals nothing until it has written the file once.
pub(crate) fn palw_vertex_state_read_v1(path: &Path) -> Option<u64> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// Persist the round a vertex is about to be signed for, durably (written beside, then renamed over).
pub(crate) fn palw_vertex_state_write_v1(path: &Path, round: u64) -> bool {
    let tmp: PathBuf = path.with_extension("tmp");
    std::fs::write(&tmp, round.to_string()).and_then(|()| std::fs::rename(&tmp, path)).is_ok()
}

/// The lifecycle object a sealed vertex is carried as.
pub(crate) fn palw_vertex_object_v1(vertex: &PalwVerificationVertexV1) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::VerificationVertexV1 { vertex: Box::new(vertex.clone()) }
}

/// The context a vertex is signed under (the one constant the node reads from the rule).
pub(crate) const PALW_VERTEX_SIGN_CONTEXT_V1: &[u8] = PALW_VERTEX_MLDSA87_CONTEXT_V1;

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

    fn seat() -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_bytes([7; 64]), 3))
    }

    fn leaf(i: u8) -> PalwVertexLeafV1 {
        palw_vertex_verdict_leaf_v1(&Hash64::from_bytes([i; 64]), 100 + u64::from(i), PalwReceiptVerdictV2::Valid, false)
    }

    fn sign(message: &[u8], context: &[u8]) -> Option<Vec<u8>> {
        assert_eq!(context, PALW_VERTEX_SIGN_CONTEXT_V1);
        let mut signature = message.to_vec();
        signature.resize(kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN, 0);
        Some(signature)
    }

    /// **One vertex a round, and never two for a round**, across a restart.
    #[test]
    fn a_seat_seals_one_vertex_a_round_and_never_signs_a_round_twice() {
        let domain = Hash64::from_bytes([9; 64]);
        let persisted = std::cell::Cell::new(None);
        let persist = |round: u64| {
            persisted.set(Some(round));
            true
        };
        let mut book = PalwVertexBookV1::default();
        assert_eq!(book.seal(domain, seat(), 50, &sign, &persist), None, "nothing to say: no vertex");
        book.record(leaf(1));
        book.record(leaf(2));
        book.record(leaf(1));
        assert_eq!(book.pending_leaves(), 2, "a verdict recorded twice is one leaf");
        assert_eq!(book.seal(domain, seat(), 50, &sign, &persist), Some(50));
        assert_eq!(persisted.get(), Some(50), "the round is persisted before the vertex goes out");
        // The same round again — leaves that arrive later wait for the next round.
        book.record(leaf(3));
        assert_eq!(book.seal(domain, seat(), 50, &sign, &persist), None, "one vertex a round");
        assert_eq!(book.seal(domain, seat(), 49, &sign, &persist), None, "and never an older one");
        // A restart: the persisted round is the floor.
        let mut resumed = PalwVertexBookV1::resume(persisted.get());
        resumed.record(leaf(4));
        assert_eq!(resumed.seal(domain, seat(), 50, &sign, &persist), None, "a restarted seat does not sign round 50 again");
        assert_eq!(resumed.seal(domain, seat(), 51, &sign, &persist), Some(51));
        assert_eq!(book.seal(domain, seat(), 51, &sign, &persist), Some(51));
        // A seat that cannot persist a round seals nothing.
        let mut stuck = PalwVertexBookV1::default();
        stuck.record(leaf(5));
        assert_eq!(stuck.seal(domain, seat(), 60, &sign, &|_| false), None);
        assert_eq!(stuck.pending_leaves(), 1, "and keeps its leaves");
    }

    /// A vertex is carried until its row appears; one that never lands expires and its leaves are said again.
    #[test]
    fn a_vertex_is_carried_until_it_lands_and_an_expired_one_is_said_again() {
        let domain = Hash64::from_bytes([9; 64]);
        let mut book = PalwVertexBookV1::default();
        book.record(leaf(1));
        assert_eq!(book.seal(domain, seat(), 100, &sign, &|_| true), Some(100));
        let root = book.next_to_send(100).expect("a sealed vertex is to be sent").leaves_root;
        book.mark_sent(100, 100);
        assert!(book.next_to_send(101).is_none(), "not re-sent at once");
        assert!(book.next_to_send(100 + PALW_VERTEX_RESEND_DAA_V1).is_some(), "re-sent after the replan interval");
        // Landed: the row at the tip is the vertex's.
        let mut landed = PalwVertexBookV1::default();
        landed.record(leaf(1));
        landed.seal(domain, seat(), 100, &sign, &|_| true);
        assert_eq!(landed.reconcile(&[(100, root)], 105), vec![100]);
        assert_eq!((landed.sealed_waiting(), landed.landed_total), (0, 1));
        // Not landed, and too old: the leaves return to pending.
        assert!(book.reconcile(&[], 100 + PALW_VERTEX_MAX_CARRY_DAA_V1).is_empty());
        assert_eq!(book.sealed_waiting(), 1, "still within the carry window");
        book.reconcile(&[], 100 + PALW_VERTEX_MAX_CARRY_DAA_V1 + 1);
        assert_eq!((book.sealed_waiting(), book.pending_leaves(), book.expired_total), (0, 1, 1));
        // …and a verdict already sealed is not replaced by a later one for the same claim.
        let mut stands = PalwVertexBookV1::default();
        stands.record(leaf(1));
        stands.seal(domain, seat(), 10, &sign, &|_| true);
        stands.record(leaf(1));
        assert_eq!(stands.pending_leaves(), 0, "the first verdict stands");
    }

    /// The leaf cap: a round's overflow waits for the next.
    #[test]
    fn a_round_past_the_leaf_cap_carries_the_rest_to_the_next() {
        let domain = Hash64::from_bytes([9; 64]);
        let mut book = PalwVertexBookV1::default();
        for i in 0..(PALW_VERTEX_MAX_LEAVES_V1 as u32 + 10) {
            book.record(PalwVertexLeafV1::Verdict {
                claim: PalwClaimRefV1::Compact { bound_daa: i, id_prefix: [1; 16] },
                verdict: PalwReceiptVerdictV2::Valid,
            });
        }
        assert_eq!(book.seal(domain, seat(), 10, &sign, &|_| true), Some(10));
        assert_eq!(book.pending_leaves(), 10, "the overflow stays");
        assert_eq!(book.seal(domain, seat(), 11, &sign, &|_| true), Some(11));
        assert_eq!(book.pending_leaves(), 0);
    }

    #[test]
    fn references_are_compact_unless_the_operator_asks_for_whole_ids() {
        let claim = Hash64::from_bytes([5; 64]);
        assert!(matches!(palw_vertex_claim_by_leaf_v1(&claim, 123, false), PalwClaimRefV1::Compact { bound_daa: 123, .. }));
        assert_eq!(palw_vertex_claim_by_leaf_v1(&claim, 123, true), PalwClaimRefV1::Full(claim));
        assert_eq!(
            palw_vertex_claim_by_leaf_v1(&claim, u64::from(u32::MAX) + 1, false),
            PalwClaimRefV1::Full(claim),
            "a DAA past 32 bits is whole"
        );
    }

    /// A held capture is attested as the chunks it spans, under a digest of its bytes; the same bytes give the same leaf.
    #[test]
    fn a_held_capture_is_attested_as_its_chunks_under_one_digest() {
        let claim = Hash64::from_bytes([3; 64]);
        assert!(palw_vertex_held_capture_leaf_v1(&claim, 9, &[], false).is_none());
        let bytes = vec![7u8; PALW_VERTEX_HELD_CHUNK_BYTES_V1 * 2 + 1];
        let leaf = palw_vertex_held_capture_leaf_v1(&claim, 9, &bytes, false).expect("a leaf");
        let PalwVertexLeafV1::Held { object, first, last, digest, .. } = leaf else { panic!("a Held leaf") };
        assert_eq!((object, first, last), (0, 0, 2), "two whole chunks and one byte are three chunks");
        assert_eq!(leaf, palw_vertex_held_capture_leaf_v1(&claim, 9, &bytes, false).unwrap(), "the same bytes, the same leaf");
        let mut other = bytes.clone();
        other[0] ^= 1;
        let PalwVertexLeafV1::Held { digest: other_digest, .. } = palw_vertex_held_capture_leaf_v1(&claim, 9, &other, false).unwrap()
        else {
            panic!("a Held leaf")
        };
        assert_ne!(digest, other_digest, "another capture, another digest");
    }

    #[test]
    fn the_state_file_round_trips_and_a_missing_one_reads_none() {
        let dir = std::env::temp_dir().join(format!("palw-vertex-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("palw-vertex-round");
        assert_eq!(palw_vertex_state_read_v1(&path), None);
        assert!(palw_vertex_state_write_v1(&path, 4_242));
        assert_eq!(palw_vertex_state_read_v1(&path), Some(4_242));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
