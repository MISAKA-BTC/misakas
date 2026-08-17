//! ADR-0038 assumption A4 as a type — the Track-D M2 catalog-coverage gate, pure.
//!
//! A4 says the court's kernel catalog is 100% of reachable kernels per Active class: 90%
//! coverage is an invitation for the remaining 10%, because a miner who can steer execution
//! into an uncatalogued kernel makes every dispute over it `Unadjudicable` (ADR-0037
//! Decision 5) — rejected but unslashed, the exact hole a forger farms. This module turns
//! that sentence into a construction rule instead of a review item:
//!
//! * a class may only activate with a [`PalwCatalogCoverageCertificateV1`], and
//! * a certificate is only constructible through [`verify_catalog_coverage_v1`] — the private
//!   `_sealed` field means no other code can assemble one, so "we checked coverage" and
//!   "a certificate exists" are the same fact.
//!
//! The certificate carries a digest of the EXACT reachable set that was compared, so a later
//! shape-profile change (new kernels reachable) cannot ride an old certificate: the caller
//! re-derives the reachable set, re-verifies, and gets a different digest or a
//! [`PalwCoverageError::CoverageGap`]. The gap error lists EVERY missing id, sorted — a
//! truncated list would read as "almost covered" when A4's whole point is that almost is
//! nothing.
//!
//! Kernel identity is [`crate::palw_step::kernel_semantics_id_v1`] — one frozen
//! reduction-order program per id (ADR-0030 premise "order is code named by id"); coverage
//! is set inclusion over those ids, nothing semantic. Assembling the reachable set from the
//! registered shape profile is the CALLER's job (registration wiring, not this module's);
//! per I7's spirit an EMPTY reachable set is an error, not a vacuous pass — unknown
//! reachability is not coverage.
//!
//! Everything here is arithmetic over the caller's sets — no store handle, no registry, no
//! clock. Consensus-inert: nothing constructs these types on any shipped network; the
//! Track-D change set (ADR-0038 §Implementation order) wires the gate at class activation.

use kaspa_hashes::Hash64;
use std::collections::BTreeSet;

// ---------------------------------------------------------------------------------------------
// Domains and constants
// ---------------------------------------------------------------------------------------------

pub const PALW_CATALOG_COVERAGE_VERSION_V1: u16 = 1;

/// Keyed-BLAKE2b domain of the coverage digest: `H(domain ‖ class_id ‖ count ‖ sorted ids)`.
pub const PALW_CATALOG_COVERAGE_DOMAIN_DIGEST: &[u8] = b"misaka-palw/catalog-coverage-digest/v1";

/// Every domain this module introduces (uniqueness-tested against every other PALW family).
pub const PALW_COVERAGE_ALL_DOMAINS: &[&[u8]] = &[PALW_CATALOG_COVERAGE_DOMAIN_DIGEST];

// ---------------------------------------------------------------------------------------------
// The two claims
// ---------------------------------------------------------------------------------------------

/// The reachability claim: every `kernel_semantics_id` an execution class's shape profile can
/// reach at adjudication time. Assembled by the CALLER from the registered shape profile
/// (that assembly is registration wiring, not this module's); the `BTreeSet` makes the claim
/// order-free and duplicate-free by type, so two assemblers of the same profile produce the
/// same claim bytes.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwReachableKernelSetV1 {
    pub execution_class_id: Hash64,
    pub kernel_ids: BTreeSet<Hash64>,
}

/// The catalog claim: every `kernel_semantics_id` the court can actually adjudicate — the ids
/// `resolve_kernel` answers for in [`crate::palw_step_refute`]'s catalog. Extra entries are
/// harmless (a court that can adjudicate more than a class reaches is a superset, not a gap).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwCataloguedKernelSetV1 {
    pub execution_class_id: Hash64,
    pub kernel_ids: BTreeSet<Hash64>,
}

// ---------------------------------------------------------------------------------------------
// The certificate
// ---------------------------------------------------------------------------------------------

/// Proof that coverage was verified complete for one class. Only constructible through
/// [`verify_catalog_coverage_v1`] — the private `_sealed` field cannot be named outside this
/// module, so external construction is impossible. Carries the digest of the exact reachable
/// set compared, so a later shape-profile change cannot ride an old certificate.
// Not `#[non_exhaustive]`: that only blocks construction outside the CRATE, while the sealed
// field blocks it outside this MODULE — in-crate consumers (the class activation gate) must
// also be unable to mint a certificate without running the verification.
#[allow(clippy::manual_non_exhaustive)]
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwCatalogCoverageCertificateV1 {
    pub execution_class_id: Hash64,
    /// How many reachable kernels were checked — never 0 (an empty claim refuses upstream).
    pub reachable_count: u32,
    /// `H(domain ‖ class_id ‖ reachable_count ‖ sorted reachable ids)` — the reachable set's
    /// identity. Depends ONLY on the reachable side: a catalog may grow without invalidating
    /// certificates, but a reachable set may not change by one id without changing this.
    pub coverage_digest: Hash64,
    _sealed: (),
}

// ---------------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------------

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwCoverageError {
    #[error("reachable set names class {reachable}, catalog names class {catalogued} — the comparison is meaningless")]
    ClassMismatch { reachable: Hash64, catalogued: Hash64 },
    #[error("the reachable set is empty — unknown reachability is not coverage (I7), nothing activates on a vacuous pass")]
    EmptyReachableSet,
    #[error("{} reachable kernel(s) are not in the court's catalog — A4 fails, the class must not activate", missing.len())]
    CoverageGap {
        /// EVERY missing id, ascending — a truncated list would read as "covered" when it isn't.
        missing: Vec<Hash64>,
    },
}

// ---------------------------------------------------------------------------------------------
// The gate
// ---------------------------------------------------------------------------------------------

/// The A4 gate: `Ok(certificate)` iff the reachable set is non-empty, both claims name the
/// same class, and EVERY reachable kernel id is catalogued. Extra catalogued kernels are fine
/// (superset allowed) and do not enter the digest — the certificate identifies the reachable
/// set it vouched for, not the catalog snapshot that happened to cover it.
///
/// Deterministic in its inputs and invariant under how the sets were built (the `BTreeSet`
/// already erased insertion order); two verifiers of the same claims emit byte-identical
/// certificates.
pub fn verify_catalog_coverage_v1(
    reachable: &PalwReachableKernelSetV1,
    catalogued: &PalwCataloguedKernelSetV1,
) -> Result<PalwCatalogCoverageCertificateV1, PalwCoverageError> {
    if reachable.execution_class_id != catalogued.execution_class_id {
        return Err(PalwCoverageError::ClassMismatch {
            reachable: reachable.execution_class_id,
            catalogued: catalogued.execution_class_id,
        });
    }
    if reachable.kernel_ids.is_empty() {
        return Err(PalwCoverageError::EmptyReachableSet);
    }
    // BTreeSet difference iterates ascending, so `missing` is sorted and complete by
    // construction — no cap, no early exit.
    let missing: Vec<Hash64> = reachable.kernel_ids.difference(&catalogued.kernel_ids).copied().collect();
    if !missing.is_empty() {
        return Err(PalwCoverageError::CoverageGap { missing });
    }
    Ok(PalwCatalogCoverageCertificateV1 {
        execution_class_id: reachable.execution_class_id,
        reachable_count: reachable.kernel_ids.len() as u32,
        coverage_digest: coverage_digest_v1(reachable),
        _sealed: (),
    })
}

/// `H(domain ‖ class_id ‖ count ‖ sorted reachable ids)` — keyed-BLAKE2b-512, the same
/// construction as [`crate::palw_schedule::select_replay_panel_v1`]'s ticket. Count-prefixed
/// so two sets can never concatenate to the same byte stream.
fn coverage_digest_v1(reachable: &PalwReachableKernelSetV1) -> Hash64 {
    let mut hasher = blake2b_simd::Params::new().hash_length(64).key(PALW_CATALOG_COVERAGE_DOMAIN_DIGEST).to_state();
    hasher.update(reachable.execution_class_id.as_byte_slice());
    hasher.update(&(reachable.kernel_ids.len() as u32).to_le_bytes());
    for id in &reachable.kernel_ids {
        hasher.update(id.as_byte_slice());
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash64::from_bytes(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_step::kernel_semantics_id_v1;

    fn h64(word: u64) -> Hash64 {
        Hash64::from_u64_word(word)
    }

    fn reachable(class: u64, ids: &[u64]) -> PalwReachableKernelSetV1 {
        PalwReachableKernelSetV1 { execution_class_id: h64(class), kernel_ids: ids.iter().map(|w| h64(*w)).collect() }
    }

    fn catalogued(class: u64, ids: &[u64]) -> PalwCataloguedKernelSetV1 {
        PalwCataloguedKernelSetV1 { execution_class_id: h64(class), kernel_ids: ids.iter().map(|w| h64(*w)).collect() }
    }

    /// Full coverage certifies; the certificate names the class and the count, and the digest
    /// is the reachable set's identity — one added kernel changes it.
    #[test]
    fn full_coverage_certifies_and_digest_tracks_the_reachable_set() {
        let cert = verify_catalog_coverage_v1(&reachable(0xC1, &[1, 2, 3]), &catalogued(0xC1, &[1, 2, 3])).unwrap();
        assert_eq!(cert.execution_class_id, h64(0xC1));
        assert_eq!(cert.reachable_count, 3);

        let grown = verify_catalog_coverage_v1(&reachable(0xC1, &[1, 2, 3, 4]), &catalogued(0xC1, &[1, 2, 3, 4])).unwrap();
        assert_eq!(grown.reachable_count, 4);
        assert_ne!(cert.coverage_digest, grown.coverage_digest, "a changed reachable set must change the digest");
    }

    /// One uncatalogued kernel is a gap, and the gap error names exactly that id.
    #[test]
    fn one_missing_kernel_is_a_gap_naming_exactly_it() {
        let err = verify_catalog_coverage_v1(&reachable(0xC1, &[1, 2, 3]), &catalogued(0xC1, &[1, 3])).unwrap_err();
        assert_eq!(err, PalwCoverageError::CoverageGap { missing: vec![h64(2)] });
    }

    /// Many gaps list COMPLETELY, ascending — 5 gaps are 5 entries, never a truncated sample
    /// (a silent cap would read as "covered" when it isn't).
    #[test]
    fn gap_list_is_complete_and_sorted_never_truncated() {
        let err = verify_catalog_coverage_v1(
            &reachable(0xC1, &[1, 2, 3, 4, 5, 6, 7, 8]),
            &catalogued(0xC1, &[2, 4, 6]),
        )
        .unwrap_err();
        let PalwCoverageError::CoverageGap { missing } = err else { panic!("expected CoverageGap, got {err:?}") };
        assert_eq!(missing, vec![h64(1), h64(3), h64(5), h64(7), h64(8)]);
        let mut sorted = missing.clone();
        sorted.sort();
        assert_eq!(missing, sorted);
    }

    /// An empty reachable set never certifies — even against an empty catalog. Unknown
    /// reachability is not coverage (I7): vacuous truth activates nothing.
    #[test]
    fn empty_reachable_set_refuses_even_when_catalog_is_also_empty() {
        let err = verify_catalog_coverage_v1(&reachable(0xC1, &[]), &catalogued(0xC1, &[])).unwrap_err();
        assert_eq!(err, PalwCoverageError::EmptyReachableSet);
        let err = verify_catalog_coverage_v1(&reachable(0xC1, &[]), &catalogued(0xC1, &[1, 2])).unwrap_err();
        assert_eq!(err, PalwCoverageError::EmptyReachableSet);
    }

    /// The two claims must name the same class — identical kernel sets under different class
    /// ids are two different classes' facts, not a coverage proof.
    #[test]
    fn class_mismatch_refuses_even_with_identical_kernel_sets() {
        let err = verify_catalog_coverage_v1(&reachable(0xC1, &[1, 2, 3]), &catalogued(0xC2, &[1, 2, 3])).unwrap_err();
        assert_eq!(err, PalwCoverageError::ClassMismatch { reachable: h64(0xC1), catalogued: h64(0xC2) });
    }

    /// A superset catalog certifies, and the digest depends ONLY on the reachable set — the
    /// exact-match and superset certificates are byte-identical.
    #[test]
    fn superset_catalog_certifies_with_the_reachable_only_digest() {
        let exact = verify_catalog_coverage_v1(&reachable(0xC1, &[1, 2, 3]), &catalogued(0xC1, &[1, 2, 3])).unwrap();
        let superset = verify_catalog_coverage_v1(&reachable(0xC1, &[1, 2, 3]), &catalogued(0xC1, &[1, 2, 3, 9, 10])).unwrap();
        assert_eq!(exact, superset, "catalog extras must not enter the certificate");
    }

    /// Insertion order is erased by the set types: any build order of the same claims yields
    /// the identical certificate.
    #[test]
    fn certificates_are_invariant_under_insertion_order() {
        let forward = verify_catalog_coverage_v1(&reachable(0xC1, &[1, 2, 3, 4]), &catalogued(0xC1, &[1, 2, 3, 4])).unwrap();
        let backward = verify_catalog_coverage_v1(&reachable(0xC1, &[4, 3, 2, 1]), &catalogued(0xC1, &[3, 1, 4, 2])).unwrap();
        assert_eq!(forward, backward);
    }

    /// Borsh roundtrips: both claims and the certificate survive serialize→deserialize
    /// byte-exactly (the deserialized certificate compares equal, sealed field and all).
    #[test]
    fn borsh_roundtrips_certificate_and_both_set_types() {
        let r = reachable(0xC1, &[1, 2, 3]);
        let c = catalogued(0xC1, &[1, 2, 3, 4]);
        let cert = verify_catalog_coverage_v1(&r, &c).unwrap();

        let r2: PalwReachableKernelSetV1 = borsh::from_slice(&borsh::to_vec(&r).unwrap()).unwrap();
        assert_eq!(r, r2);
        let c2: PalwCataloguedKernelSetV1 = borsh::from_slice(&borsh::to_vec(&c).unwrap()).unwrap();
        assert_eq!(c, c2);
        let cert2: PalwCatalogCoverageCertificateV1 = borsh::from_slice(&borsh::to_vec(&cert).unwrap()).unwrap();
        assert_eq!(cert, cert2);
    }

    /// The gate speaks the court's identity scheme: real `kernel_semantics_id_v1` ids flow
    /// through unchanged, and the gap it reports IS the id of the uncatalogued descriptor.
    #[test]
    fn real_kernel_semantics_ids_flow_through_the_gate() {
        let class = h64(0xC1);
        let covered = ["l2-norm/whole-row/double-sum-ascending/llama-030ebb558/v1", "glu/swiglu/v-silu-per-lane/llama-030ebb558/v1"];
        let uncovered = "matmul/q8_0-q8_0/tile-pending-transcription/llama-030ebb558/v1";

        let mut reach: BTreeSet<Hash64> = covered.iter().map(|d| kernel_semantics_id_v1(d)).collect();
        let cat = PalwCataloguedKernelSetV1 { execution_class_id: class, kernel_ids: reach.clone() };
        let r = PalwReachableKernelSetV1 { execution_class_id: class, kernel_ids: reach.clone() };
        let cert = verify_catalog_coverage_v1(&r, &cat).unwrap();
        assert_eq!(cert.reachable_count, 2);

        // Reach one program the court never transcribed: the gap names its exact id.
        reach.insert(kernel_semantics_id_v1(uncovered));
        let r = PalwReachableKernelSetV1 { execution_class_id: class, kernel_ids: reach };
        let err = verify_catalog_coverage_v1(&r, &cat).unwrap_err();
        assert_eq!(err, PalwCoverageError::CoverageGap { missing: vec![kernel_semantics_id_v1(uncovered)] });
    }
}
