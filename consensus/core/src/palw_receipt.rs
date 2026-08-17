//! ADR-0038 Decision B/C: DAG-native verification receipts — `verify(B, sample #i)` as
//! ordinary carriage riding *successor* blocks.
//!
//! A receipt is a bonded panel member's signed claim that it recomputed specific sampled
//! positions of an earlier block's PALW work and observed specific roots. Receipts are what
//! license the weight ramp's `ρ_r` stage (ADR-0038 W4): an attacker's private fork can
//! fabricate its own blocks but not the receipts of bonded validators it does not control,
//! so fabricated pwu never matures past the spam-hash backbone.
//!
//! Three rules, enforced here:
//!
//! * **A receipt is a claim, not truth** (Decision C). It carries positions and observed
//!   roots — never a bare "OK" — so a rubber-stamping attester is convictable later on its
//!   own signed roots by the court. Shape admission REFUSES a receipt that cannot name its
//!   samples.
//! * **Ramp counting is by distinct verifier bond** ([`count_distinct_receipt_verifiers_v1`]):
//!   one bonded identity filing the same receipt through ten carrier blocks is one voice.
//! * **Stateless admission only.** Like [`crate::palw_carriage`], this module checks shape
//!   (sizes, arity, ordering, signature length) and builds the signing digest; membership in
//!   the assigned panel, bond status and signature validity are stateful questions answered
//!   at the consumer entry ([`crate::palw_job_identity`]'s verified-entry idiom).
//!
//! Consensus-inert until the ADR-0038 change set wires and activates together.

use crate::tx::TransactionOutpoint;
use kaspa_hashes::{Hash, Hash64};
use std::collections::BTreeSet;
use thiserror::Error;

// ---------------------------------------------------------------------------------------------
// Domains and constants
// ---------------------------------------------------------------------------------------------

/// Keyed-BLAKE2b domain of the receipt signing digest.
pub const PALW_RECEIPT_DOMAIN_MESSAGE: &[u8] = b"misaka-palw/verification-receipt-message/v1";

/// ML-DSA-87 signing context for a verification receipt.
pub const PALW_RECEIPT_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/verification-receipt/mldsa87/v1";

/// Every domain this module introduces (uniqueness-tested against every other PALW family).
pub const PALW_RECEIPT_ALL_DOMAINS: &[&[u8]] = &[PALW_RECEIPT_DOMAIN_MESSAGE];

/// A receipt names at most this many sampled coordinates — a panel duty is a handful of
/// positions, and an unbounded list is a data-availability attack riding a signature.
pub const PALW_RECEIPT_MAX_SAMPLES: usize = 64;

pub const PALW_RECEIPT_VERSION_V1: u16 = 1;

// ---------------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------------

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwReceiptError {
    #[error("unsupported receipt version {got} (expected {expected})")]
    UnsupportedVersion { got: u16, expected: u16 },
    #[error("receipt carries {coordinates} coordinates but {roots} observed roots")]
    SampleArityMismatch { coordinates: usize, roots: usize },
    #[error("receipt carries {got} samples, above the cap {cap} (or zero)")]
    SampleCountOutOfRange { got: usize, cap: usize },
    #[error("receipt sample coordinates are not strictly ascending")]
    SampleCoordinatesNotSorted,
    #[error("signature is {got} bytes, not the ML-DSA-87 {expected}")]
    SignatureLength { got: usize, expected: usize },
}

// ---------------------------------------------------------------------------------------------
// The receipt
// ---------------------------------------------------------------------------------------------

/// One sampled position inside a block's committed execution: the bisection court's address
/// space, flattened to the four coordinates every backend shares. Ordering is lexicographic
/// (token, layer, node slot, unit), which is also the strict-ascending admission order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwSampleCoordinateV1 {
    pub token_index: u32,
    pub layer_index: u32,
    pub node_slot: u32,
    pub unit_index: u32,
}

/// A receipt's verdict over its samples. `Mismatch` is an alarm that routes into the
/// refutation/dispute machinery — it is never itself a ruling, and it still earns the
/// filer nothing unless the court later agrees (ADR-0038 Decision C).
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwReceiptVerdictV1 {
    Match = 0,
    Mismatch = 1,
}

/// The carried receipt: `verify(target_block, samples…) = roots…`, signed by a bonded
/// verifier. Rides any successor block as ordinary carriage; the carrier earns the
/// inclusion fee, the verifier earns the receipt award at credit time (both through exact
/// bond-outpoint payees, ADR-0037 I4).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwVerificationReceiptV1 {
    /// = [`PALW_RECEIPT_VERSION_V1`].
    pub version: u16,
    /// The PALW block whose work this receipt covers.
    pub target_block_hash: Hash64,
    /// That block's committed root (binds the receipt to the exact claim, not just the block).
    pub target_commitment_root: Hash64,
    /// The execution class the verifier replayed under — must equal the target's class at the
    /// consumer entry (cross-class results are telemetry, never evidence: ADR-0037 I11).
    pub execution_class_id: Hash64,
    /// Strictly ascending sampled coordinates, pairwise with `observed_roots`.
    pub sample_coordinates: Vec<PalwSampleCoordinateV1>,
    pub observed_roots: Vec<Hash64>,
    pub verdict: PalwReceiptVerdictV1,
    /// The verifier's bond — the receipt's accountable identity and its payee (I4).
    pub verifier_bond_outpoint: TransactionOutpoint,
    /// ML-DSA-87 over [`palw_receipt_message_v1`] under [`PALW_RECEIPT_MLDSA87_CONTEXT`].
    pub signature: Vec<u8>,
}

impl PalwVerificationReceiptV1 {
    /// Stateless shape admission: version, arity, count bounds, strict coordinate order,
    /// signature length. Everything stateful (panel membership, bond status, signature
    /// validity, class equality with the target) is the consumer entry's.
    pub fn validate_shape(&self) -> Result<(), PalwReceiptError> {
        if self.version != PALW_RECEIPT_VERSION_V1 {
            return Err(PalwReceiptError::UnsupportedVersion { got: self.version, expected: PALW_RECEIPT_VERSION_V1 });
        }
        if self.sample_coordinates.len() != self.observed_roots.len() {
            return Err(PalwReceiptError::SampleArityMismatch {
                coordinates: self.sample_coordinates.len(),
                roots: self.observed_roots.len(),
            });
        }
        if self.sample_coordinates.is_empty() || self.sample_coordinates.len() > PALW_RECEIPT_MAX_SAMPLES {
            return Err(PalwReceiptError::SampleCountOutOfRange { got: self.sample_coordinates.len(), cap: PALW_RECEIPT_MAX_SAMPLES });
        }
        if !self.sample_coordinates.windows(2).all(|w| w[0] < w[1]) {
            return Err(PalwReceiptError::SampleCoordinatesNotSorted);
        }
        let expected = crate::dns_finality::STAKE_ATTESTATION_SIG_LEN;
        if self.signature.len() != expected {
            return Err(PalwReceiptError::SignatureLength { got: self.signature.len(), expected });
        }
        Ok(())
    }

    /// The digest this receipt's signature must cover.
    pub fn message(&self, network_id: &[u8]) -> Hash {
        palw_receipt_message_v1(
            network_id,
            self.target_block_hash,
            self.target_commitment_root,
            self.execution_class_id,
            &self.verifier_bond_outpoint,
            &self.sample_coordinates,
            &self.observed_roots,
            self.verdict,
        )
    }
}

/// Keyed-BLAKE2b-256 signing digest of a verification receipt. Layout mirrors the V3 job
/// digests ([`crate::palw_job_identity`]): length-prefixed network id, fixed-width fields in
/// struct order, count-prefixed variable sections.
#[allow(clippy::too_many_arguments)]
pub fn palw_receipt_message_v1(
    network_id: &[u8],
    target_block_hash: Hash64,
    target_commitment_root: Hash64,
    execution_class_id: Hash64,
    verifier_bond_outpoint: &TransactionOutpoint,
    sample_coordinates: &[PalwSampleCoordinateV1],
    observed_roots: &[Hash64],
    verdict: PalwReceiptVerdictV1,
) -> Hash {
    let mut state = blake2b_simd::Params::new().hash_length(32).key(PALW_RECEIPT_DOMAIN_MESSAGE).to_state();
    state.update(&(network_id.len() as u32).to_le_bytes());
    state.update(network_id);
    state.update(target_block_hash.as_byte_slice());
    state.update(target_commitment_root.as_byte_slice());
    state.update(execution_class_id.as_byte_slice());
    state.update(verifier_bond_outpoint.transaction_id.as_byte_slice());
    state.update(&verifier_bond_outpoint.index.to_le_bytes());
    state.update(&(sample_coordinates.len() as u32).to_le_bytes());
    for c in sample_coordinates {
        state.update(&c.token_index.to_le_bytes());
        state.update(&c.layer_index.to_le_bytes());
        state.update(&c.node_slot.to_le_bytes());
        state.update(&c.unit_index.to_le_bytes());
    }
    state.update(&(observed_roots.len() as u32).to_le_bytes());
    for root in observed_roots {
        state.update(root.as_byte_slice());
    }
    state.update(&[verdict as u8]);
    let mut out = [0u8; 32];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash::from_bytes(out)
}

/// The ramp's counting rule (ADR-0038 Decision B): how many DISTINCT bonded verifiers filed a
/// `Match` receipt for `target_block_hash` over `target_commitment_root`. One identity through
/// many carrier blocks is one voice; a `Mismatch` receipt licenses nothing (it routes into
/// dispute instead). The caller passes receipts that already passed shape + consumer entry.
pub fn count_distinct_receipt_verifiers_v1(
    receipts: &[PalwVerificationReceiptV1],
    target_block_hash: &Hash64,
    target_commitment_root: &Hash64,
) -> usize {
    receipts
        .iter()
        .filter(|r| {
            r.target_block_hash == *target_block_hash
                && r.target_commitment_root == *target_commitment_root
                && r.verdict == PalwReceiptVerdictV1::Match
        })
        .map(|r| (r.verifier_bond_outpoint.transaction_id, r.verifier_bond_outpoint.index))
        .collect::<BTreeSet<_>>()
        .len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dns_finality::STAKE_ATTESTATION_SIG_LEN;

    const NET: &[u8] = b"misaka-testnet-11";

    fn coord(t: u32, l: u32, n: u32, u: u32) -> PalwSampleCoordinateV1 {
        PalwSampleCoordinateV1 { token_index: t, layer_index: l, node_slot: n, unit_index: u }
    }

    fn outpoint(seed: u64) -> TransactionOutpoint {
        TransactionOutpoint::new(kaspa_hashes::Hash64::from_u64_word(seed), (seed % 5) as u32)
    }

    fn receipt(verifier_seed: u64) -> PalwVerificationReceiptV1 {
        PalwVerificationReceiptV1 {
            version: PALW_RECEIPT_VERSION_V1,
            target_block_hash: Hash64::from_u64_word(1),
            target_commitment_root: Hash64::from_u64_word(2),
            execution_class_id: Hash64::from_u64_word(3),
            sample_coordinates: vec![coord(0, 4, 0, 0), coord(0, 27, 1, 8), coord(17, 51, 0, 2)],
            observed_roots: vec![Hash64::from_u64_word(5), Hash64::from_u64_word(6), Hash64::from_u64_word(7)],
            verdict: PalwReceiptVerdictV1::Match,
            verifier_bond_outpoint: outpoint(verifier_seed),
            signature: vec![0x5A; STAKE_ATTESTATION_SIG_LEN],
        }
    }

    /// Receipt domains are unique against every other PALW family (incl. the V3 job family).
    #[test]
    fn domains_are_unique_across_all_palw_families() {
        let mut all: Vec<&[u8]> = Vec::new();
        all.extend(PALW_RECEIPT_ALL_DOMAINS);
        all.push(PALW_RECEIPT_MLDSA87_CONTEXT);
        all.extend(crate::palw_job_identity::PALW_JOB_ALL_DOMAINS);
        all.extend(crate::palw_schedule::PALW_SCHEDULE_ALL_DOMAINS);
        all.extend(crate::palw_slash::PALW_S_ALL_DOMAINS);
        all.extend(crate::palw_routing::PALW_ROUTING_ALL_DOMAINS);
        all.extend(crate::palw_registry::PALW_REGISTRY_ALL_DOMAINS);
        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                assert_ne!(a, b, "domain collision: {:?}", String::from_utf8_lossy(a));
            }
        }
    }

    /// Shape admission refuses: version drift, arity mismatch, empty, over-cap, unsorted or
    /// duplicate coordinates, wrong signature length — and admits the well-formed receipt.
    #[test]
    fn shape_admission_is_closed() {
        assert!(receipt(9).validate_shape().is_ok());
        let mut r = receipt(9);
        r.version = 2;
        assert_eq!(r.validate_shape(), Err(PalwReceiptError::UnsupportedVersion { got: 2, expected: 1 }));
        let mut r = receipt(9);
        r.observed_roots.pop();
        assert_eq!(r.validate_shape(), Err(PalwReceiptError::SampleArityMismatch { coordinates: 3, roots: 2 }));
        let mut r = receipt(9);
        r.sample_coordinates.clear();
        r.observed_roots.clear();
        assert!(matches!(r.validate_shape(), Err(PalwReceiptError::SampleCountOutOfRange { got: 0, .. })));
        let mut r = receipt(9);
        r.sample_coordinates = (0..(PALW_RECEIPT_MAX_SAMPLES as u32 + 1)).map(|i| coord(i, 0, 0, 0)).collect();
        r.observed_roots = vec![Hash64::from_u64_word(1); PALW_RECEIPT_MAX_SAMPLES + 1];
        assert!(matches!(r.validate_shape(), Err(PalwReceiptError::SampleCountOutOfRange { .. })));
        let mut r = receipt(9);
        r.sample_coordinates[2] = r.sample_coordinates[1]; // duplicate ⇒ not strictly ascending
        assert_eq!(r.validate_shape(), Err(PalwReceiptError::SampleCoordinatesNotSorted));
        let mut r = receipt(9);
        r.signature = vec![0x5A; 64];
        assert_eq!(r.validate_shape(), Err(PalwReceiptError::SignatureLength { got: 64, expected: STAKE_ATTESTATION_SIG_LEN }));
    }

    /// The signing digest binds target block, committed root, class, bond, every coordinate,
    /// every observed root, and the verdict — a receipt can never be replayed against a
    /// different block, claim, class or sample set.
    #[test]
    fn message_binds_every_field() {
        let base = receipt(9).message(NET);
        assert_ne!(base, receipt(9).message(b"other-net"));
        let mut r = receipt(9);
        r.target_block_hash = Hash64::from_u64_word(99);
        assert_ne!(base, r.message(NET));
        let mut r = receipt(9);
        r.target_commitment_root = Hash64::from_u64_word(99);
        assert_ne!(base, r.message(NET));
        let mut r = receipt(9);
        r.execution_class_id = Hash64::from_u64_word(99);
        assert_ne!(base, r.message(NET));
        let mut r = receipt(9);
        r.sample_coordinates[1].unit_index += 1;
        assert_ne!(base, r.message(NET));
        let mut r = receipt(9);
        r.observed_roots[0] = Hash64::from_u64_word(99);
        assert_ne!(base, r.message(NET));
        let mut r = receipt(9);
        r.verdict = PalwReceiptVerdictV1::Mismatch;
        assert_ne!(base, r.message(NET));
        let mut r = receipt(9);
        r.verifier_bond_outpoint = outpoint(99);
        assert_ne!(base, r.message(NET));
        // The signature is NOT part of its own message.
        let mut r = receipt(9);
        r.signature = vec![0x77; STAKE_ATTESTATION_SIG_LEN];
        assert_eq!(base, r.message(NET));
    }

    /// Ramp counting: distinct verifier bonds only; duplicates through many carrier blocks are
    /// one voice; Mismatch receipts and receipts for other targets/roots license nothing.
    #[test]
    fn ramp_counts_distinct_matching_verifiers_only() {
        let target = Hash64::from_u64_word(1);
        let root = Hash64::from_u64_word(2);
        let mut mismatch = receipt(30);
        mismatch.verdict = PalwReceiptVerdictV1::Mismatch;
        let mut other_target = receipt(40);
        other_target.target_block_hash = Hash64::from_u64_word(99);
        let mut other_root = receipt(50);
        other_root.target_commitment_root = Hash64::from_u64_word(99);
        let receipts = vec![
            receipt(10),
            receipt(10), // same verifier again (different carrier block) — one voice
            receipt(20),
            mismatch,     // alarm, not license
            other_target, // different block
            other_root,   // different claim on the same block
        ];
        assert_eq!(count_distinct_receipt_verifiers_v1(&receipts, &target, &root), 2);
        // The stray receipts count toward THEIR target, not toward nothing…
        assert_eq!(count_distinct_receipt_verifiers_v1(&receipts, &Hash64::from_u64_word(99), &root), 1);
        // …and a target nobody covered counts zero.
        assert_eq!(count_distinct_receipt_verifiers_v1(&receipts, &Hash64::from_u64_word(77), &root), 0);
    }

    /// Borsh roundtrip of the carried receipt.
    #[test]
    fn receipt_roundtrips_borsh() {
        let r = receipt(9);
        let bytes = borsh::to_vec(&r).unwrap();
        assert_eq!(r, borsh::from_slice::<PalwVerificationReceiptV1>(&bytes).unwrap());
    }

    /// Coordinate ordering is lexicographic across the four fields — the admission order is
    /// total, so two receipts over the same set normalize identically on every node.
    #[test]
    fn coordinate_order_is_total_lexicographic() {
        assert!(coord(0, 1, 0, 0) < coord(0, 1, 0, 1));
        assert!(coord(0, 1, 0, 9) < coord(0, 2, 0, 0));
        assert!(coord(0, 9, 9, 9) < coord(1, 0, 0, 0));
    }
}
