//! ADR-0037 Decision 3 / ADR-0038 Decision A: PALW V3 identity and signature binding —
//! and the verified-entry types that make "another layer will verify it" untypable.
//!
//! Every audited fail-open in the consumer layer shared one shape: a fact (a signature, an
//! identity, a binding) was checked at one door and *assumed* at another. This module ends
//! the assumption two ways:
//!
//! * **Full binding (I5).** [`palw_job_id_v3`] makes a job's identity a function of the
//!   funding request itself, and every signing digest below binds network, job, context,
//!   class and bond — so a signature can never be replayed across networks, jobs, contexts,
//!   classes or bonds. `committed_root` alone is NOT an identity (it cannot distinguish
//!   same-root distinct jobs, duplicated carriage, or replay); the ADR-0037 rule is that
//!   `job_id`, `job_context_hash`, `commitment_root` and the bond outpoint bind together.
//! * **Verified-entry wrappers.** [`VerifiedPalwCommitmentV3`] / [`VerifiedPalwAttestationV3`]
//!   have no public constructor: the ONLY way to obtain one is [`verify_commitment_entry_v3`]
//!   / [`verify_attestation_entry_v3`], which demand the caller's ML-DSA-87 verifier say yes
//!   to the exact binding digest. A credit consumer whose API takes the wrapper cannot be
//!   handed an unverified claim — the compiler is the reviewer.
//!
//! Key resolution stays where it lives today (the bond registry maps `bond_outpoint` to a
//! validator key — [`crate::palw_carriage`]'s idiom); this module takes the verifier as a
//! closure so it stays pure and consensus-inert. Nothing on any shipped network constructs
//! any of this until the ADR-0038 change set wires and activates together.

use crate::tx::TransactionOutpoint;
use kaspa_hashes::{Hash, Hash64};
use thiserror::Error;

// ---------------------------------------------------------------------------------------------
// Domains and constants
// ---------------------------------------------------------------------------------------------

/// Keyed-BLAKE2b domain of the V3 job identity (ADR-0037 Decision 3's `MISAKA/PALW/JOB/V3`).
pub const PALW_JOB_DOMAIN_JOB_ID: &[u8] = b"misaka-palw/job-id/v3";

/// Keyed-BLAKE2b domain of the V3 commitment signing digest (`MISAKA/PALW/COMMIT/V3`).
pub const PALW_JOB_DOMAIN_COMMIT_MESSAGE: &[u8] = b"misaka-palw/job-commitment-message/v3";

/// ML-DSA-87 signing context for a V3 commitment signature.
pub const PALW_JOB_MLDSA87_COMMIT_CONTEXT: &[u8] = b"misaka-palw/job-commitment/mldsa87/v3";

/// Keyed-BLAKE2b domain of the V3 attestation signing digest (`MISAKA/PALW/ATTEST/V3`).
pub const PALW_JOB_DOMAIN_ATTEST_MESSAGE: &[u8] = b"misaka-palw/job-attestation-message/v3";

/// ML-DSA-87 signing context for a V3 attestation signature.
pub const PALW_JOB_MLDSA87_ATTEST_CONTEXT: &[u8] = b"misaka-palw/job-attestation/mldsa87/v3";

/// Keyed-BLAKE2b domain of the V3 panel seed (`MISAKA/PALW/PANEL/V3`): the future-anchor
/// and snapshot-bound input ADR-0037 Decision 4 fixes `select_replay_panel_v1`'s caller to.
pub const PALW_JOB_DOMAIN_PANEL_SEED: &[u8] = b"misaka-palw/job-panel-seed/v3";

/// Every domain this module introduces (uniqueness-tested against every other PALW family).
pub const PALW_JOB_ALL_DOMAINS: &[&[u8]] = &[
    PALW_JOB_DOMAIN_JOB_ID,
    PALW_JOB_DOMAIN_COMMIT_MESSAGE,
    PALW_JOB_DOMAIN_ATTEST_MESSAGE,
    PALW_JOB_DOMAIN_PANEL_SEED,
];

/// An attestation may carry at most this many sampled positions: enough for any plausible
/// panel duty, small enough that a claim can never smuggle unbounded data past admission.
pub const PALW_JOB_MAX_ATTESTATION_SAMPLES: usize = 64;

// ---------------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------------

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwJobIdentityError {
    #[error("signature is {got} bytes, not the ML-DSA-87 {expected}")]
    SignatureLength { got: usize, expected: usize },
    #[error("the ML-DSA-87 verifier rejected the {what} signature over its binding digest")]
    SignatureInvalid { what: &'static str },
    #[error("attestation carries {indices} sample indices but {roots} observed roots")]
    SampleArityMismatch { indices: usize, roots: usize },
    #[error("attestation carries {got} samples, above the cap {cap} (or zero)")]
    SampleCountOutOfRange { got: usize, cap: usize },
    #[error("attestation sample indices are not strictly ascending")]
    SampleIndicesNotSorted,
}

// ---------------------------------------------------------------------------------------------
// Identity and signing digests
// ---------------------------------------------------------------------------------------------

fn keyed64(domain: &[u8]) -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(64).key(domain).to_state()
}

fn keyed32(domain: &[u8]) -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(32).key(domain).to_state()
}

fn update_len_prefixed(state: &mut blake2b_simd::State, bytes: &[u8]) {
    state.update(&(bytes.len() as u32).to_le_bytes());
    state.update(bytes);
}

fn update_outpoint(state: &mut blake2b_simd::State, outpoint: &TransactionOutpoint) {
    state.update(outpoint.transaction_id.as_byte_slice());
    state.update(&outpoint.index.to_le_bytes());
}

fn finalize64(state: blake2b_simd::State) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

fn finalize32(state: blake2b_simd::State) -> Hash {
    let mut out = [0u8; 32];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash::from_bytes(out)
}

/// The V3 job identity: a pure function of the funding request. One outpoint of one accepted
/// transaction, one nonce, one band — so two jobs can never share an id, and a replayed
/// request body lands on the same id (where first-accepted-wins dedup catches it) instead of
/// minting a sibling.
pub fn palw_job_id_v3(
    network_id: &[u8],
    request_txid: Hash64,
    request_output_index: u32,
    requester_nonce: u64,
    model_band_id: Hash64,
) -> Hash64 {
    let mut state = keyed64(PALW_JOB_DOMAIN_JOB_ID);
    update_len_prefixed(&mut state, network_id);
    state.update(request_txid.as_byte_slice());
    state.update(&request_output_index.to_le_bytes());
    state.update(&requester_nonce.to_le_bytes());
    state.update(model_band_id.as_byte_slice());
    finalize64(state)
}

/// The V3 commitment signing digest: network, job, context, class, bond, and all three roots.
/// Layout mirrors [`crate::palw_slash::palw_execution_attestation_message_v1`]: length-prefixed
/// network id, then fixed-width fields in struct order.
#[allow(clippy::too_many_arguments)]
pub fn palw_commit_message_v3(
    network_id: &[u8],
    job_id: Hash64,
    job_context_hash: Hash64,
    execution_class_id: Hash64,
    executor_bond_outpoint: &TransactionOutpoint,
    commitment_root: Hash64,
    trace_root: Hash64,
    output_root: Hash64,
) -> Hash {
    let mut state = keyed32(PALW_JOB_DOMAIN_COMMIT_MESSAGE);
    update_len_prefixed(&mut state, network_id);
    state.update(job_id.as_byte_slice());
    state.update(job_context_hash.as_byte_slice());
    state.update(execution_class_id.as_byte_slice());
    update_outpoint(&mut state, executor_bond_outpoint);
    state.update(commitment_root.as_byte_slice());
    state.update(trace_root.as_byte_slice());
    state.update(output_root.as_byte_slice());
    finalize32(state)
}

/// The V3 attestation signing digest: the verifier's bond and the exact samples it claims to
/// have recomputed. `sample_indices` and `observed_roots` are count-prefixed pairwise, so an
/// attestation is a claim about *positions and values*, never a bare success count.
#[allow(clippy::too_many_arguments)]
pub fn palw_attest_message_v3(
    network_id: &[u8],
    job_id: Hash64,
    job_context_hash: Hash64,
    execution_class_id: Hash64,
    verifier_bond_outpoint: &TransactionOutpoint,
    sample_indices: &[u32],
    observed_roots: &[Hash64],
    verdict: PalwAttestationVerdictV3,
) -> Hash {
    let mut state = keyed32(PALW_JOB_DOMAIN_ATTEST_MESSAGE);
    update_len_prefixed(&mut state, network_id);
    state.update(job_id.as_byte_slice());
    state.update(job_context_hash.as_byte_slice());
    state.update(execution_class_id.as_byte_slice());
    update_outpoint(&mut state, verifier_bond_outpoint);
    state.update(&(sample_indices.len() as u32).to_le_bytes());
    for index in sample_indices {
        state.update(&index.to_le_bytes());
    }
    state.update(&(observed_roots.len() as u32).to_le_bytes());
    for root in observed_roots {
        state.update(root.as_byte_slice());
    }
    state.update(&[verdict as u8]);
    finalize32(state)
}

/// The V3 panel seed (ADR-0037 Decision 4): the input that fixes `select_replay_panel_v1`'s
/// caller. The anchor is a block that finalized AFTER the commitment; the snapshot root is the
/// eligible set at that anchor — so no caller can hardcode eligibility again.
pub fn palw_panel_seed_v3(
    network_id: &[u8],
    job_id: Hash64,
    commitment_root: Hash64,
    future_anchor_block_hash: Hash64,
    eligible_set_snapshot_root: Hash64,
) -> Hash64 {
    let mut state = keyed64(PALW_JOB_DOMAIN_PANEL_SEED);
    update_len_prefixed(&mut state, network_id);
    state.update(job_id.as_byte_slice());
    state.update(commitment_root.as_byte_slice());
    state.update(future_anchor_block_hash.as_byte_slice());
    state.update(eligible_set_snapshot_root.as_byte_slice());
    finalize64(state)
}

// ---------------------------------------------------------------------------------------------
// Verified-entry wrappers
// ---------------------------------------------------------------------------------------------

/// A panel attestation's verdict over its samples. `Mismatch` is an alarm (the refutation and
/// dispute machinery take over) — never itself a ruling (ADR-0038 Decision C).
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwAttestationVerdictV3 {
    Match = 0,
    Mismatch = 1,
}

/// An unverified commitment claim, exactly as carried. Constructible by anyone; convertible
/// into the verified form only through [`verify_commitment_entry_v3`].
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwJobCommitmentClaimV3 {
    pub job_id: Hash64,
    pub job_context_hash: Hash64,
    pub execution_class_id: Hash64,
    pub executor_bond_outpoint: TransactionOutpoint,
    pub commitment_root: Hash64,
    pub trace_root: Hash64,
    pub output_root: Hash64,
    /// ML-DSA-87 over [`palw_commit_message_v3`] under [`PALW_JOB_MLDSA87_COMMIT_CONTEXT`].
    pub signature: Vec<u8>,
}

/// An unverified attestation claim, exactly as carried.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwJobAttestationClaimV3 {
    pub job_id: Hash64,
    pub job_context_hash: Hash64,
    pub execution_class_id: Hash64,
    pub verifier_bond_outpoint: TransactionOutpoint,
    /// Strictly ascending sampled positions, pairwise with `observed_roots`.
    pub sample_indices: Vec<u32>,
    pub observed_roots: Vec<Hash64>,
    pub verdict: PalwAttestationVerdictV3,
    /// ML-DSA-87 over [`palw_attest_message_v3`] under [`PALW_JOB_MLDSA87_ATTEST_CONTEXT`].
    pub signature: Vec<u8>,
}

/// A commitment whose binding digest an ML-DSA-87 verifier accepted. No public constructor:
/// holding one IS the proof of verification, so a consumer whose API takes this type cannot
/// be fed an unverified claim (I5's enforcement).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedPalwCommitmentV3 {
    claim: PalwJobCommitmentClaimV3,
}

impl VerifiedPalwCommitmentV3 {
    pub fn claim(&self) -> &PalwJobCommitmentClaimV3 {
        &self.claim
    }
}

/// An attestation whose shape and binding digest passed [`verify_attestation_entry_v3`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedPalwAttestationV3 {
    claim: PalwJobAttestationClaimV3,
}

impl VerifiedPalwAttestationV3 {
    pub fn claim(&self) -> &PalwJobAttestationClaimV3 {
        &self.claim
    }
}

/// The consumer-entry gate for commitments: exact signature length, then the caller's
/// ML-DSA-87 verifier over the exact binding digest. The verifier closure receives the digest
/// and the signature; key resolution (bond registry) is the caller's, so this stays pure.
pub fn verify_commitment_entry_v3<F>(
    claim: PalwJobCommitmentClaimV3,
    network_id: &[u8],
    verify: F,
) -> Result<VerifiedPalwCommitmentV3, PalwJobIdentityError>
where
    F: FnOnce(&Hash, &[u8]) -> bool,
{
    let expected = crate::dns_finality::STAKE_ATTESTATION_SIG_LEN;
    if claim.signature.len() != expected {
        return Err(PalwJobIdentityError::SignatureLength { got: claim.signature.len(), expected });
    }
    let message = palw_commit_message_v3(
        network_id,
        claim.job_id,
        claim.job_context_hash,
        claim.execution_class_id,
        &claim.executor_bond_outpoint,
        claim.commitment_root,
        claim.trace_root,
        claim.output_root,
    );
    if !verify(&message, &claim.signature) {
        return Err(PalwJobIdentityError::SignatureInvalid { what: "commitment" });
    }
    Ok(VerifiedPalwCommitmentV3 { claim })
}

/// The consumer-entry gate for attestations: shape first (arity, cap, strict ordering — an
/// attestation that cannot name its samples is not evidence), then the binding digest under
/// the caller's verifier.
pub fn verify_attestation_entry_v3<F>(
    claim: PalwJobAttestationClaimV3,
    network_id: &[u8],
    verify: F,
) -> Result<VerifiedPalwAttestationV3, PalwJobIdentityError>
where
    F: FnOnce(&Hash, &[u8]) -> bool,
{
    if claim.sample_indices.len() != claim.observed_roots.len() {
        return Err(PalwJobIdentityError::SampleArityMismatch {
            indices: claim.sample_indices.len(),
            roots: claim.observed_roots.len(),
        });
    }
    if claim.sample_indices.is_empty() || claim.sample_indices.len() > PALW_JOB_MAX_ATTESTATION_SAMPLES {
        return Err(PalwJobIdentityError::SampleCountOutOfRange {
            got: claim.sample_indices.len(),
            cap: PALW_JOB_MAX_ATTESTATION_SAMPLES,
        });
    }
    if !claim.sample_indices.windows(2).all(|w| w[0] < w[1]) {
        return Err(PalwJobIdentityError::SampleIndicesNotSorted);
    }
    let expected = crate::dns_finality::STAKE_ATTESTATION_SIG_LEN;
    if claim.signature.len() != expected {
        return Err(PalwJobIdentityError::SignatureLength { got: claim.signature.len(), expected });
    }
    let message = palw_attest_message_v3(
        network_id,
        claim.job_id,
        claim.job_context_hash,
        claim.execution_class_id,
        &claim.verifier_bond_outpoint,
        &claim.sample_indices,
        &claim.observed_roots,
        claim.verdict,
    );
    if !verify(&message, &claim.signature) {
        return Err(PalwJobIdentityError::SignatureInvalid { what: "attestation" });
    }
    Ok(VerifiedPalwAttestationV3 { claim })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dns_finality::STAKE_ATTESTATION_SIG_LEN;

    const NET: &[u8] = b"misaka-testnet-11";

    fn outpoint(seed: u64) -> TransactionOutpoint {
        TransactionOutpoint::new(Hash64::from_u64_word(seed), (seed % 5) as u32)
    }

    fn commitment_claim() -> PalwJobCommitmentClaimV3 {
        PalwJobCommitmentClaimV3 {
            job_id: Hash64::from_u64_word(1),
            job_context_hash: Hash64::from_u64_word(2),
            execution_class_id: Hash64::from_u64_word(3),
            executor_bond_outpoint: outpoint(4),
            commitment_root: Hash64::from_u64_word(5),
            trace_root: Hash64::from_u64_word(6),
            output_root: Hash64::from_u64_word(7),
            signature: vec![0x5A; STAKE_ATTESTATION_SIG_LEN],
        }
    }

    fn attestation_claim() -> PalwJobAttestationClaimV3 {
        PalwJobAttestationClaimV3 {
            job_id: Hash64::from_u64_word(1),
            job_context_hash: Hash64::from_u64_word(2),
            execution_class_id: Hash64::from_u64_word(3),
            verifier_bond_outpoint: outpoint(8),
            sample_indices: vec![4, 27, 51],
            observed_roots: vec![Hash64::from_u64_word(9), Hash64::from_u64_word(10), Hash64::from_u64_word(11)],
            verdict: PalwAttestationVerdictV3::Match,
            signature: vec![0x33; STAKE_ATTESTATION_SIG_LEN],
        }
    }

    /// Every V3 domain is unique against every other PALW family's domain set — one shared
    /// key anywhere would let one family's digest masquerade as another's.
    #[test]
    fn domains_are_unique_across_all_palw_families() {
        let mut all: Vec<&[u8]> = Vec::new();
        all.extend(PALW_JOB_ALL_DOMAINS);
        all.extend(crate::palw_schedule::PALW_SCHEDULE_ALL_DOMAINS);
        all.extend(crate::palw_slash::PALW_S_ALL_DOMAINS);
        all.extend(crate::palw_routing::PALW_ROUTING_ALL_DOMAINS);
        all.extend(crate::palw_registry::PALW_REGISTRY_ALL_DOMAINS);
        all.push(PALW_JOB_MLDSA87_COMMIT_CONTEXT);
        all.push(PALW_JOB_MLDSA87_ATTEST_CONTEXT);
        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                assert_ne!(a, b, "domain collision: {:?}", String::from_utf8_lossy(a));
            }
        }
    }

    /// The job id moves with every one of its five inputs — and only with them.
    #[test]
    fn job_id_binds_every_input() {
        let base = palw_job_id_v3(NET, Hash64::from_u64_word(1), 2, 3, Hash64::from_u64_word(4));
        assert_eq!(base, palw_job_id_v3(NET, Hash64::from_u64_word(1), 2, 3, Hash64::from_u64_word(4)));
        assert_ne!(base, palw_job_id_v3(b"other-net", Hash64::from_u64_word(1), 2, 3, Hash64::from_u64_word(4)));
        assert_ne!(base, palw_job_id_v3(NET, Hash64::from_u64_word(9), 2, 3, Hash64::from_u64_word(4)));
        assert_ne!(base, palw_job_id_v3(NET, Hash64::from_u64_word(1), 9, 3, Hash64::from_u64_word(4)));
        assert_ne!(base, palw_job_id_v3(NET, Hash64::from_u64_word(1), 2, 9, Hash64::from_u64_word(4)));
        assert_ne!(base, palw_job_id_v3(NET, Hash64::from_u64_word(1), 2, 3, Hash64::from_u64_word(9)));
    }

    /// The commitment digest moves with every bound field (I5: network, job, context, class,
    /// bond, and each root — a signature can never be replayed across any of them).
    #[test]
    fn commit_message_binds_every_field() {
        let c = commitment_claim();
        let digest = |c: &PalwJobCommitmentClaimV3, net: &[u8]| {
            palw_commit_message_v3(
                net,
                c.job_id,
                c.job_context_hash,
                c.execution_class_id,
                &c.executor_bond_outpoint,
                c.commitment_root,
                c.trace_root,
                c.output_root,
            )
        };
        let base = digest(&c, NET);
        assert_ne!(base, digest(&c, b"other-net"));
        for field in 0..6 {
            let mut m = c.clone();
            match field {
                0 => m.job_id = Hash64::from_u64_word(99),
                1 => m.job_context_hash = Hash64::from_u64_word(99),
                2 => m.execution_class_id = Hash64::from_u64_word(99),
                3 => m.commitment_root = Hash64::from_u64_word(99),
                4 => m.trace_root = Hash64::from_u64_word(99),
                _ => m.output_root = Hash64::from_u64_word(99),
            }
            assert_ne!(base, digest(&m, NET), "field {field} did not bind");
        }
        let mut m = c.clone();
        m.executor_bond_outpoint = outpoint(99);
        assert_ne!(base, digest(&m, NET), "bond outpoint did not bind");
        let mut m = c;
        m.executor_bond_outpoint.index += 1;
        assert_ne!(base, digest(&m, NET), "bond outpoint index did not bind");
    }

    /// The attestation digest binds positions, values, verdict and bond — moving one sampled
    /// root or one index moves the digest, so "successes = 3" can never be forged from a
    /// different sample set.
    #[test]
    fn attest_message_binds_samples_and_verdict() {
        let a = attestation_claim();
        let digest = |a: &PalwJobAttestationClaimV3| {
            palw_attest_message_v3(
                NET,
                a.job_id,
                a.job_context_hash,
                a.execution_class_id,
                &a.verifier_bond_outpoint,
                &a.sample_indices,
                &a.observed_roots,
                a.verdict,
            )
        };
        let base = digest(&a);
        let mut m = a.clone();
        m.sample_indices[1] = 28;
        assert_ne!(base, digest(&m));
        let mut m = a.clone();
        m.observed_roots[2] = Hash64::from_u64_word(99);
        assert_ne!(base, digest(&m));
        let mut m = a.clone();
        m.verdict = PalwAttestationVerdictV3::Mismatch;
        assert_ne!(base, digest(&m));
        let mut m = a;
        m.verifier_bond_outpoint = outpoint(99);
        assert_ne!(base, digest(&m));
    }

    /// Count-prefixing pins the index/root boundary: moving a value from the tail of the
    /// indices to the head of the roots cannot collide.
    #[test]
    fn attest_message_is_boundary_safe() {
        let two = palw_attest_message_v3(
            NET,
            Hash64::from_u64_word(1),
            Hash64::from_u64_word(2),
            Hash64::from_u64_word(3),
            &outpoint(4),
            &[1, 2],
            &[Hash64::from_u64_word(5), Hash64::from_u64_word(6)],
            PalwAttestationVerdictV3::Match,
        );
        let one = palw_attest_message_v3(
            NET,
            Hash64::from_u64_word(1),
            Hash64::from_u64_word(2),
            Hash64::from_u64_word(3),
            &outpoint(4),
            &[1],
            &[Hash64::from_u64_word(5), Hash64::from_u64_word(6)],
            PalwAttestationVerdictV3::Match,
        );
        assert_ne!(two, one);
    }

    /// The panel seed binds all five Decision-4 inputs — a caller can no longer hardcode
    /// eligibility, because the seed will not reproduce without the real anchor and snapshot.
    #[test]
    fn panel_seed_binds_anchor_and_snapshot() {
        let base =
            palw_panel_seed_v3(NET, Hash64::from_u64_word(1), Hash64::from_u64_word(2), Hash64::from_u64_word(3), Hash64::from_u64_word(4));
        assert_ne!(
            base,
            palw_panel_seed_v3(NET, Hash64::from_u64_word(1), Hash64::from_u64_word(2), Hash64::from_u64_word(9), Hash64::from_u64_word(4))
        );
        assert_ne!(
            base,
            palw_panel_seed_v3(NET, Hash64::from_u64_word(1), Hash64::from_u64_word(2), Hash64::from_u64_word(3), Hash64::from_u64_word(9))
        );
    }

    /// The commitment gate: wrong length refuses before the verifier runs; a rejecting
    /// verifier refuses; an accepting verifier yields the sealed wrapper carrying the claim.
    #[test]
    fn commitment_entry_gate_refuses_and_admits() {
        let mut short = commitment_claim();
        short.signature = vec![0x5A; 64];
        assert_eq!(
            verify_commitment_entry_v3(short, NET, |_, _| unreachable!("verifier must not run on bad length")),
            Err(PalwJobIdentityError::SignatureLength { got: 64, expected: STAKE_ATTESTATION_SIG_LEN })
        );
        assert_eq!(
            verify_commitment_entry_v3(commitment_claim(), NET, |_, _| false),
            Err(PalwJobIdentityError::SignatureInvalid { what: "commitment" })
        );
        let verified = verify_commitment_entry_v3(commitment_claim(), NET, |message, signature| {
            // The gate hands the verifier the exact binding digest, not raw bytes.
            let c = commitment_claim();
            let expected = palw_commit_message_v3(
                NET,
                c.job_id,
                c.job_context_hash,
                c.execution_class_id,
                &c.executor_bond_outpoint,
                c.commitment_root,
                c.trace_root,
                c.output_root,
            );
            *message == expected && signature.len() == STAKE_ATTESTATION_SIG_LEN
        })
        .unwrap();
        assert_eq!(verified.claim(), &commitment_claim());
    }

    /// The attestation gate: shape refusals (arity, emptiness, cap, ordering) come before the
    /// verifier; a well-shaped claim under an accepting verifier admits.
    #[test]
    fn attestation_entry_gate_checks_shape_first() {
        let mut arity = attestation_claim();
        arity.observed_roots.pop();
        assert_eq!(
            verify_attestation_entry_v3(arity, NET, |_, _| unreachable!()),
            Err(PalwJobIdentityError::SampleArityMismatch { indices: 3, roots: 2 })
        );
        let mut empty = attestation_claim();
        empty.sample_indices.clear();
        empty.observed_roots.clear();
        assert_eq!(
            verify_attestation_entry_v3(empty, NET, |_, _| unreachable!()),
            Err(PalwJobIdentityError::SampleCountOutOfRange { got: 0, cap: PALW_JOB_MAX_ATTESTATION_SAMPLES })
        );
        let mut over = attestation_claim();
        over.sample_indices = (0..(PALW_JOB_MAX_ATTESTATION_SAMPLES as u32 + 1)).collect();
        over.observed_roots = vec![Hash64::from_u64_word(1); PALW_JOB_MAX_ATTESTATION_SAMPLES + 1];
        assert!(matches!(
            verify_attestation_entry_v3(over, NET, |_, _| unreachable!()),
            Err(PalwJobIdentityError::SampleCountOutOfRange { .. })
        ));
        let mut unsorted = attestation_claim();
        unsorted.sample_indices = vec![4, 4, 51];
        assert_eq!(verify_attestation_entry_v3(unsorted, NET, |_, _| unreachable!()), Err(PalwJobIdentityError::SampleIndicesNotSorted));
        assert!(verify_attestation_entry_v3(attestation_claim(), NET, |_, _| true).is_ok());
        assert_eq!(
            verify_attestation_entry_v3(attestation_claim(), NET, |_, _| false),
            Err(PalwJobIdentityError::SignatureInvalid { what: "attestation" })
        );
    }

    /// Borsh roundtrip of both carried claim shapes.
    #[test]
    fn claims_roundtrip_borsh() {
        let c = commitment_claim();
        let bytes = borsh::to_vec(&c).unwrap();
        assert_eq!(c, borsh::from_slice::<PalwJobCommitmentClaimV3>(&bytes).unwrap());
        let a = attestation_claim();
        let bytes = borsh::to_vec(&a).unwrap();
        assert_eq!(a, borsh::from_slice::<PalwJobAttestationClaimV3>(&bytes).unwrap());
    }
}
