//! MISAKA Verified LLM Token-Weighted BFT (`MISAKA_Verified_LLM_Token_BFT_v0.1`)
//! — the **voting-weight** layer that replaces bonded capital with verified
//! useful compute as the source of DNS-finality voting power.
//!
//! See [ADR-0024](../../docs/adr/0024-verified-llm-token-weighted-bft.md) for the decision
//! record, the activation plan, and the calibration rationale. Section references (§3.2, §4,
//! §6, …) throughout this module are to the v0.1 paper.
//!
//! # What this replaces
//!
//! The pre-existing DNS finality overlay ([`crate::dns_finality`]) is a
//! stake-weighted attestation scheme: a validator's voting weight *is* its
//! bonded amount, and an epoch earns graded StakeScore credit through the φS
//! quality floor. That is the "Money → Power" mapping of ordinary PoS.
//!
//! This module implements the paper's replacement — "Verified Useful Compute →
//! Power":
//!
//! ```text
//!   x_j     = ρ(S_j)·(a·t_j^in + b·t_j^out)   if Verify(S_j, R_j, C_j) = 1, else 0   (§3.2)
//!   X_i(e)  = Σ_j x_j                          over validator i's certified jobs in epoch e
//!   C_i(E)  = Σ_{τ=1..K} d_τ · X_i(E − τ)      1 = d_1 ≥ d_2 ≥ … ≥ d_K > 0           (§4, eq. 5)
//!   W_i(E)  = min{ C_i(E), λ·B_i(E) }                                                (§4, eq. 6)
//!   W(E)    = Σ_i W_i(E),   Q(E) = ⌊2W(E)/3⌋ + 1                                     (§4, eq. 7)
//! ```
//!
//! The three load-bearing properties, and where each is enforced here:
//!
//! * **Bond is collateral, not voting power.** `B_i` enters [`effective_voting_weight`]
//!   only as the `λ·B_i` *cap*. Adding bond with `C_i(E) = 0` yields `W_i(E) = 0` —
//!   capital alone buys nothing. The 20M-KAS `min_bond_amount_sompi` floor is
//!   **unchanged** by this module: it stays the participation/slashable-collateral
//!   requirement it always was (see [`crate::dns_finality::DnsParams::min_bond_amount_sompi`]).
//! * **Compute credit cannot bootstrap its own fork.** `C_i(E)` reads only epochs
//!   `≤ E − credit_delay_epochs` (≥ 1 by construction, §4/§8.3), so VLT minted on the
//!   current fork cannot inflate that fork's own voting power — the eq. (5) epoch delay.
//! * **Stale compute decays.** `d_τ` is geometric in [`decay_coefficient`], so a
//!   validator that stops producing verified compute loses weight instead of holding
//!   it forever (§4).
//!
//! # What this module is not
//!
//! It is the **pure, deterministic** consensus surface: types, normalization,
//! decay, weight, quorum, and the sortition/verdict predicates. It performs no
//! I/O and runs no model. Actually executing an LLM job and re-executing it as a
//! verifier is node-side software that consumes [`LlmJobSpec`] / [`ComputeReceipt`]
//! and produces a [`ComputeCertificatePayload`]; consensus only ever checks the
//! commitments, never the tensors.
//!
//! v0.1 pins [`VerificationScheme::CanonicalFullReplay`] as the only
//! consensus-eligible scheme (§6 requires the acceptance condition be deterministic
//! in consensus code). Full replay is deterministic by construction: the JobSpec
//! fixes model weights, runtime, quantization, input, sampling seed, and token
//! limit, so an honest verifier re-executing it must reproduce the executor's
//! receipt hash **byte-for-byte**. The other scheme ids are reserved and rejected.
//!
//! # Activation
//!
//! Every consumer is fenced behind [`VltParams::vlt_activation_daa_score`]. Below
//! the fence the overlay keeps its legacy bonded-stake weight byte-for-byte
//! ([`VltParams::INERT`] is the shipped default on every current network), so
//! adopting this module is not by itself a consensus change. Switching a network's
//! fence to a live DAA score is the hard-forking step, and must not be done before
//! the active set can actually produce verified compute — with no VLT, `W(E) = 0`
//! and no epoch reaches quorum (the paper's §2 "計算 bootstrap 期間" caveat).

use std::collections::BTreeMap;
use std::fmt::{self, Display, Formatter};

use blake2b_simd::Params as Blake2bParams;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::{Hash, Hash64};

use crate::constants::SOMPI_PER_KASPA;
use crate::tx::TransactionOutpoint;
use crate::{BlockHash, TransactionId};

// ---------------------------------------------------------------------
// Scale constants.
// ---------------------------------------------------------------------

/// Fixed-point scale for every VLT rational quantity — `ρ`, `a`, `b`, and the
/// decay coefficients `d_τ`. All VLT arithmetic is integer `u128`; there are no
/// floats anywhere in the weight path, because two nodes computing the same
/// weight to different last bits would split consensus.
pub const VLT_MICRO: u128 = 1_000_000;

/// Basis-point denominator for [`VltParams::credit_decay_bps`].
pub const VLT_BPS: u128 = 10_000;

/// Wire-format version for every payload in this module.
pub const VLT_PAYLOAD_VERSION_V1: u16 = 1;

/// Maximum verifier attestations a single [`ComputeCertificatePayload`] may carry.
/// Bounds the per-tx ML-DSA-87 verification cost (each attestation is a 4627-byte
/// signature) exactly as `MAX_ATTESTATIONS_PER_SHARD` bounds the attestation shard.
pub const MAX_VERIFIER_ATTESTATIONS: usize = 8;

/// Maximum entries in a network's [`ModelCostTable`]. Fixed-capacity (rather than a
/// `Vec`) so [`VltParams`] stays `const`-constructible alongside the other per-network
/// preset params in `config::params`.
pub const MAX_MODEL_COST_ENTRIES: usize = 16;

/// Maximum size of the job input a [`ComputeCommitmentPayload`] may carry.
///
/// The input has to be **on chain** because [`VerificationScheme::CanonicalFullReplay`] means a
/// verifier re-runs the job, and [`LlmJobSpec`] carries only `p_j` — a commitment to the input,
/// not the input. A verifier that cannot obtain the prompt cannot replay, and under
/// refutation-dominant acceptance a committee that cannot replay is a committee that never
/// confirms. Any off-chain distribution channel would have to exist, be reachable, and be waited
/// on before a verdict could be formed; publishing the bytes in the phase-1 commitment removes
/// that dependency the same way standalone verdicts removed the executor→verifier round trip.
///
/// The commitment is the right carrier rather than the certificate: it is published *before* the
/// sortition beacon exists, so the whole job is public before anyone knows who audits it, and
/// re-stating the input on the certificate would only create a second place for it to disagree.
///
/// A stateless cap rather than a [`VltParams`] field so [`crate::dns_finality::validate_compute_commitment_payload`]
/// can enforce it without threading per-network params into isolation validation — the same
/// reason [`MAX_VERIFIER_ATTESTATIONS`] is a constant. 8 KiB is far above the token budget
/// [`ModelCostEntry::max_tokens`] allows a job to consume and far below the 4627-byte ML-DSA-87
/// signature's order of magnitude for the transaction as a whole.
pub const MAX_JOB_INPUT_BYTES: usize = 8192;

// ---------------------------------------------------------------------
// Domain separators. Each is a distinct BLAKE2b key so a digest from one
// role can never be replayed as a digest from another.
// ---------------------------------------------------------------------

/// Keyed-BLAKE2b-512 domain for [`job_spec_id`] (`H(S_j)`).
pub const JOB_SPEC_ID_KEY: &[u8] = b"misaka-vlt-jobspec-v1";
/// Keyed-BLAKE2b-512 domain for [`job_input_commitment`] (`p_j`).
pub const JOB_INPUT_COMMITMENT_KEY: &[u8] = b"misaka-vlt-job-input-v1";
/// Keyed-BLAKE2b-512 domain for [`compute_receipt_hash`] (`R_j`, §3.1 eq. 2).
pub const COMPUTE_RECEIPT_KEY: &[u8] = b"misaka-vlt-receipt-v1";
/// Keyed-BLAKE2b-256 domain for the executor's signed certificate message.
pub const COMPUTE_CERT_MESSAGE_DOMAIN: &[u8] = b"misaka-vlt-v1/compute-cert";
/// Keyed-BLAKE2b-256 domain for a verifier's signed verdict message.
pub const VERIFIER_VERDICT_MESSAGE_DOMAIN: &[u8] = b"misaka-vlt-v1/verifier-verdict";
/// Keyed-BLAKE2b-256 domain for a challenger's signed fraud-proof message.
pub const COMPUTE_CHALLENGE_MESSAGE_DOMAIN: &[u8] = b"misaka-vlt-v1/compute-challenge";
/// Keyed-BLAKE2b-256 domain for a validator's signed compute-capability declaration.
pub const COMPUTE_CAPABILITY_MESSAGE_DOMAIN: &[u8] = b"misaka-vlt-v1/compute-capability";
/// Keyed-BLAKE2b-256 domain for an executor's phase-1 job commitment.
pub const COMPUTE_COMMITMENT_MESSAGE_DOMAIN: &[u8] = b"misaka-vlt-v1/compute-commitment";
/// Keyed-BLAKE2b-512 domain for the §6 post-commit verifier sortition.
pub const VERIFIER_SORTITION_KEY: &[u8] = b"misaka-vlt-verifier-sortition-v1";

/// ML-DSA-87 signing context for an executor's compute-certificate signature.
pub const COMPUTE_CERT_MLDSA87_CONTEXT: &[u8] = b"misaka-vlt-v1/cert/mldsa87";
/// ML-DSA-87 signing context for a verifier's verdict signature.
pub const VERIFIER_VERDICT_MLDSA87_CONTEXT: &[u8] = b"misaka-vlt-v1/verdict/mldsa87";
/// ML-DSA-87 signing context for a challenger's fraud-proof signature.
pub const COMPUTE_CHALLENGE_MLDSA87_CONTEXT: &[u8] = b"misaka-vlt-v1/challenge/mldsa87";
/// ML-DSA-87 signing context for a compute-capability declaration.
pub const COMPUTE_CAPABILITY_MLDSA87_CONTEXT: &[u8] = b"misaka-vlt-v1/capability/mldsa87";
/// ML-DSA-87 signing context for an executor's phase-1 job commitment.
pub const COMPUTE_COMMITMENT_MLDSA87_CONTEXT: &[u8] = b"misaka-vlt-v1/commitment/mldsa87";

// ---------------------------------------------------------------------
// Job specification (§3.1).
// ---------------------------------------------------------------------

/// Numeric profile `q` of [`LlmJobSpec`]. Part of the spec because the same
/// weights at a different precision are a different amount of compute **and** a
/// different output — a replay verifier that quantized differently would refute an
/// honest receipt.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum QuantizationProfile {
    #[default]
    Fp32 = 0,
    Bf16 = 1,
    Fp16 = 2,
    Int8 = 3,
    Int4 = 4,
}

impl QuantizationProfile {
    /// Reject unknown discriminants at the stateless layer so an unrecognised
    /// profile can never reach the weight path.
    pub fn is_known(self) -> bool {
        matches!(self, Self::Fp32 | Self::Bf16 | Self::Fp16 | Self::Int8 | Self::Int4)
    }
}

/// Verification relation `v_j` (§6). Only [`Self::CanonicalFullReplay`] is
/// consensus-eligible in v0.1; the other two are reserved wire ids so adding them
/// later is not a payload-format break.
///
/// The §6 requirement is that the *acceptance condition* be deterministic in
/// consensus code. Full replay satisfies it structurally: the JobSpec fixes every
/// input to the computation, so "the verifier's independently recomputed `R_j`
/// equals the executor's" is a byte comparison, not a judgement call.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum VerificationScheme {
    /// An independently-sortitioned verifier re-executes the whole job and must
    /// reproduce the executor's receipt hash byte-for-byte.
    #[default]
    CanonicalFullReplay = 0,
    /// Reserved (§6): commitment + randomized trace spot-check. Not accepted in v0.1.
    RandomizedTraceChallenge = 1,
    /// Reserved (§6): succinct proof of correct execution. Not accepted in v0.1.
    SuccinctProof = 2,
}

impl VerificationScheme {
    /// Whether consensus will mint VLT for a job carrying this scheme.
    pub fn is_consensus_eligible(self) -> bool {
        matches!(self, Self::CanonicalFullReplay)
    }
}

/// `S_j = (h_M, h_R, q, p_j, s_j, L_j, v_j)` — the job specification fixed
/// **before** execution (§3.1 eq. 1).
///
/// Every field exists to make the computation reproducible. Drop any one of them
/// and an honest verifier's replay can legitimately differ from the executor's
/// receipt, which would make refutation meaningless and the whole VLT supply
/// unverifiable.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct LlmJobSpec {
    pub version: u16,
    /// `h_M` — commitment to the exact model weights.
    pub model_weights_hash: Hash64,
    /// `h_R` — commitment to the exact inference runtime build (kernels, fusion,
    /// reduction order). Two runtimes that reduce in different orders produce
    /// different logits; without this the replay relation is not a function.
    pub runtime_hash: Hash64,
    /// `q` — numeric/quantization profile.
    pub quantization: QuantizationProfile,
    /// `p_j` — commitment to the input (prompt + tokenizer + any context).
    pub input_commitment: Hash64,
    /// `s_j` — sampling seed. Fixed so sampling is deterministic; without it the
    /// same spec has many valid outputs.
    pub sampling_seed: [u8; 32],
    /// `L_j` — hard token limit. Bounds a verifier's replay cost, so an executor
    /// cannot submit an unboundedly expensive job to grief its verifiers.
    pub max_tokens: u32,
    /// `v_j` — verification relation.
    pub verification_scheme: VerificationScheme,
}

/// `p_j` — the commitment to a job's input bytes, keyed by [`JOB_INPUT_COMMITMENT_KEY`].
///
/// This is the value [`LlmJobSpec::input_commitment`] carries, and the link between the input
/// published in a job's [`ComputeCommitmentPayload`] and the spec its certificate names: the
/// credit walk requires `job_input_commitment(commitment.input) == cert.spec.input_commitment`,
/// so an executor cannot commit to one prompt and certify a receipt for another.
pub fn job_input_commitment(input: &[u8]) -> Hash64 {
    let mut hasher = Blake2bParams::new().hash_length(64).key(JOB_INPUT_COMMITMENT_KEY).to_state();
    hasher.update(&(input.len() as u64).to_le_bytes());
    hasher.update(input);
    let mut out = [0u8; 64];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// `H(S_j)` — the canonical job identity, keyed by [`JOB_SPEC_ID_KEY`].
///
/// Field-by-field with fixed-width little-endian scalars rather than a borsh
/// re-encode: the digest is a consensus identity and must not move if the borsh
/// derive's layout ever changes.
pub fn job_spec_id(spec: &LlmJobSpec) -> Hash64 {
    let mut hasher = Blake2bParams::new().hash_length(64).key(JOB_SPEC_ID_KEY).to_state();
    hasher.update(&spec.version.to_le_bytes());
    hasher.update(spec.model_weights_hash.as_byte_slice());
    hasher.update(spec.runtime_hash.as_byte_slice());
    hasher.update(&[spec.quantization as u8]);
    hasher.update(spec.input_commitment.as_byte_slice());
    hasher.update(&spec.sampling_seed);
    hasher.update(&spec.max_tokens.to_le_bytes());
    hasher.update(&[spec.verification_scheme as u8]);
    let mut out = [0u8; 64];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash64::from_bytes(out)
}

// ---------------------------------------------------------------------
// Receipt (§3.1 eq. 2).
// ---------------------------------------------------------------------

/// What the executor claims it produced. Hashed into `R_j` by
/// [`compute_receipt_hash`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ComputeReceipt {
    pub version: u16,
    /// `h_{y_j}` — commitment to the output token sequence.
    pub output_commitment: Hash64,
    /// `t_j^in` — prefill token count.
    pub prefill_tokens: u32,
    /// `t_j^out` — decode token count.
    pub decode_tokens: u32,
    /// `z_j` — commitment to the execution trace. Unused by
    /// [`VerificationScheme::CanonicalFullReplay`] acceptance (full replay compares
    /// the whole receipt hash) but committed anyway so a future
    /// [`VerificationScheme::RandomizedTraceChallenge`] can challenge historical
    /// receipts without a payload-format break.
    pub trace_commitment: Hash64,
}

/// `R_j = H(S_j ‖ h_{y_j} ‖ t_j^in ‖ t_j^out ‖ z_j)` (§3.1 eq. 2), keyed by
/// [`COMPUTE_RECEIPT_KEY`].
///
/// This is the single value a full-replay verifier reproduces. Because `S_j` enters
/// through [`job_spec_id`], a receipt is bound to its spec: the same output claimed
/// under a cheaper spec is a different `R_j`.
pub fn compute_receipt_hash(spec: &LlmJobSpec, receipt: &ComputeReceipt) -> Hash64 {
    compute_receipt_hash_for_job(job_spec_id(spec), receipt)
}

/// `R_j` from the job identity directly, for a holder of `H(S_j)` that does not have `S_j`.
///
/// A verdict carries `job_id` but not the spec, and this is what lets its replay proof be checked
/// against the receipt it claims to have produced — statelessly, with no certificate lookup.
pub fn compute_receipt_hash_for_job(job_id: Hash64, receipt: &ComputeReceipt) -> Hash64 {
    let mut hasher = Blake2bParams::new().hash_length(64).key(COMPUTE_RECEIPT_KEY).to_state();
    hasher.update(job_id.as_byte_slice());
    hasher.update(&receipt.version.to_le_bytes());
    hasher.update(receipt.output_commitment.as_byte_slice());
    hasher.update(&receipt.prefill_tokens.to_le_bytes());
    hasher.update(&receipt.decode_tokens.to_le_bytes());
    hasher.update(receipt.trace_commitment.as_byte_slice());
    let mut out = [0u8; 64];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash64::from_bytes(out)
}

// ---------------------------------------------------------------------
// Certificate + verdicts (§3.1 eq. 3, §6).
// ---------------------------------------------------------------------

/// A sortitioned verifier's binary judgement on an executor's receipt.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum VerificationVerdict {
    /// The verifier's own replay reproduced the executor's `R_j`.
    #[default]
    Confirmed = 0,
    /// The verifier's replay produced a different `R_j`.
    Refuted = 1,
}

/// One verifier's signed verdict.
///
/// Published as its own [`ComputeVerdictPayload`] transaction rather than embedded in the
/// certificate. Embedding would force the executor to collect signed verdicts off-chain BEFORE it
/// could publish — an off-chain round trip that has to exist, be reachable, and be waited on. As
/// standalone transactions the verdicts are instead discovered from the chain by the verifiers
/// themselves, publicly auditable, and a contradiction between two of them is provable by anyone
/// reading the chain rather than only by whoever received both.
///
/// `replay_receipt_hash` is recorded even for [`VerificationVerdict::Confirmed`]
/// (where it necessarily equals the executor's `R_j`) because it is what makes the
/// §7(b) "矛盾する verification result" offence objectively provable: two signed
/// verdicts from one verifier over one `job_id` with different
/// `(verdict, replay_receipt_hash)` are self-contradictory on their face, with no
/// re-execution needed to adjudicate.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct VerifierAttestation {
    pub version: u16,
    /// `validator_id` of the verifier (`BLAKE2b-512(validator_pubkey)`).
    pub verifier_id: Hash64,
    /// The verifier's own bond. A verifier must itself be bonded so its verdict is
    /// slashable (§6: "互いに矛盾する Certificate へ署名した場合は slash 対象").
    pub bond_outpoint: TransactionOutpoint,
    pub verdict: VerificationVerdict,
    /// `R_j` as independently recomputed by this verifier.
    pub replay_receipt_hash: Hash64,
    /// ML-DSA-87 signature over [`verifier_verdict_message`] under
    /// [`VERIFIER_VERDICT_MLDSA87_CONTEXT`].
    pub signature: Vec<u8>,
}

/// The on-chain compute certificate — the transaction payload that mints VLT.
///
/// Accepted on `SUBNETWORK_ID_COMPUTE_CERTIFICATE`. Acceptance alone does **not**
/// credit `X_i(e)`: the certificate must additionally survive
/// [`VltParams::challenge_window_blocks`] without a successful
/// [`ComputeChallengePayload`] (§6, "challenge window を経て初めて X_i(E) へ加算").
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ComputeCertificatePayload {
    pub version: u16,
    /// Epoch this job's credit is attributed to. Consensus additionally binds it to
    /// the including block's own epoch, so an executor cannot back-date credit into
    /// an epoch that is already weighting votes.
    pub epoch: u64,
    /// `validator_id` of the executor.
    pub executor_id: Hash64,
    /// The executor's bond — the collateral `λ·B_i` caps this credit against, and
    /// the stake slashed if the certificate is later refuted.
    pub executor_bond_outpoint: TransactionOutpoint,
    /// The phase-1 [`ComputeCommitmentPayload`] transaction this certificate completes. Consensus
    /// derives the sortition beacon from the epoch AFTER the one that accepted it, which is what
    /// makes the committee unguessable at commitment time.
    pub commitment_tx_id: TransactionId,
    pub spec: LlmJobSpec,
    pub receipt: ComputeReceipt,
    /// ML-DSA-87 signature over [`compute_certificate_message`] under
    /// [`COMPUTE_CERT_MLDSA87_CONTEXT`].
    pub executor_signature: Vec<u8>,
}

/// Why a [`ComputeChallengePayload`] says a certificate is fraudulent — the §7(b)/(c)
/// slashable compute offences, each objectively checkable from signed material.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum ComputeFraudKind {
    /// §7(b) 偽造 Receipt — the challenger replayed the spec and got a different
    /// `R_j` than the executor signed. Slashes the **executor**.
    #[default]
    ForgedReceipt = 0,
    /// §7(b) 無効な Compute Certificate — the certificate is structurally invalid
    /// (unknown scheme, unregistered model, verifier not in the sortitioned set,
    /// executor verifying its own job). Slashes the **executor**.
    InvalidCertificate = 1,
    /// §7(b) 矛盾する verification result — one verifier signed two contradictory
    /// verdicts over the same `job_id`. Slashes the **verifier**.
    ContradictoryVerification = 2,
    /// §7(c) Challenge に失敗した実行を正しいものとして claim.
    ///
    /// **Superseded, retained for borsh stability.** Nobody has to report a failed challenge any
    /// more: [`adjudicate_compute_challenge`] settles every challenge against its certificate's own
    /// verdicts when the challenge window closes, and slashes the losing side automatically. A
    /// challenge of this kind decides nothing and slashes nobody.
    FailedChallenge = 3,
}

/// How a challenge resolved once its certificate's verdicts settled (§7(b)/(c)).
///
/// A challenge is a *claim* about a computation, and nothing in its own payload can settle it —
/// consensus cannot re-run the job. What can settle it is the evidence the protocol already
/// gathers: the sortitioned committee's verdicts over that same certificate, each confirmation
/// carrying a [`ReplayResiduals`] proof that its author actually executed it.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum ChallengeOutcome {
    /// The committee has not said enough yet — too few verdicts to decide either way. Neither
    /// party is slashed, and the certificate mints nothing meanwhile.
    #[default]
    Undecided,
    /// The challenge stands: the certificate's own committee refuted it, or it was structurally
    /// invalid. Slashes the executor (§7(b)) and denies the credit.
    Succeeded,
    /// The challenge is disproved: enough sortitioned verifiers independently reproduced the
    /// executor's projection, each proving it. Slashes the **challenger** (§7(c)) and leaves the
    /// credit intact.
    Failed,
}

/// Decide a challenge from its certificate's settled verdict set (§7(c)).
///
/// `resolved` is whether the certificate resolved against the chain at all (executor bond and
/// signature good, commitment present, beacon drawn); `attestations` are the verdicts consensus
/// counted for it, already filtered to committee members with valid replay proofs.
///
/// The asymmetry is deliberate. Slashing an executor needs a positive refutation by a drawn
/// verifier; slashing a challenger needs the certificate to have actually cleared verification.
/// Anything short of either is [`ChallengeOutcome::Undecided`] — a committee that has not
/// published enough is a reason to wait, never a reason to burn somebody's bond.
pub fn adjudicate_compute_challenge(
    kind: ComputeFraudKind,
    resolved: bool,
    executor_receipt_hash: Hash64,
    attestations: &[VerifierAttestation],
    min_confirmations: u8,
    min_refutations: u8,
) -> ChallengeOutcome {
    match kind {
        // Objectively decided from the payload alone at acceptance; nothing to re-decide here.
        ComputeFraudKind::ContradictoryVerification => ChallengeOutcome::Undecided,
        ComputeFraudKind::InvalidCertificate => {
            if !resolved {
                ChallengeOutcome::Succeeded
            } else if verify_compute_certificate(executor_receipt_hash, attestations, min_confirmations, min_refutations) {
                ChallengeOutcome::Failed
            } else {
                ChallengeOutcome::Undecided
            }
        }
        ComputeFraudKind::ForgedReceipt => {
            if !resolved {
                // The certificate credits nothing regardless, and its committee was never drawn, so
                // there is no evidence either way. Not an occasion to slash anyone.
                ChallengeOutcome::Undecided
            } else if refutation_quorum_reached(attestations, min_refutations) {
                // A quorum of drawn verifiers, each having paid for an execution to say so. One
                // dissenting voice is not a fraud proof — it is exactly what a griefer produces.
                ChallengeOutcome::Succeeded
            } else if verify_compute_certificate(executor_receipt_hash, attestations, min_confirmations, min_refutations) {
                ChallengeOutcome::Failed
            } else {
                ChallengeOutcome::Undecided
            }
        }
        // Superseded: adjudication is automatic, so a failed challenge no longer has to be reported
        // by anyone. Retained for borsh stability; it decides nothing.
        ComputeFraudKind::FailedChallenge => ChallengeOutcome::Undecided,
    }
}

/// A fraud proof against an accepted certificate, on
/// `SUBNETWORK_ID_COMPUTE_CHALLENGE`.
///
/// Like [`crate::dns_finality::SlashingEvidencePayload`] this is a pure evidence
/// carrier: it declares no outputs, and the reporter reward is minted by consensus
/// as a side-effect at `(challenge_tx_id, 0)`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ComputeChallengePayload {
    pub version: u16,
    /// The certificate transaction being challenged.
    pub certificate_tx_id: TransactionId,
    /// `H(S_j)` of the challenged job — redundant with `certificate_tx_id` but
    /// carried so the challenge can be indexed and dedup'd without a tx lookup.
    pub job_id: Hash64,
    /// `R_j` as the challenged certificate's executor claimed it.
    ///
    /// Carried so a [`ComputeFraudKind::ContradictoryVerification`] proof is checkable **without
    /// fetching the certificate**: [`verifier_verdict_message`] binds it, and [`VerifierAttestation`]
    /// does not, so without it the two verdict signatures could not be reconstructed and the
    /// "proof" would be two unverifiable blobs. Claiming a false value here does not help an
    /// attacker — the signatures then simply fail to verify.
    pub executor_receipt_hash: Hash64,
    pub kind: ComputeFraudKind,
    /// `validator_id` of the challenger.
    pub challenger_id: Hash64,
    /// The challenger's own bond. A challenger stakes its own collateral, which is
    /// what makes §7(c) — slashing a *failed* challenge — enforceable.
    pub challenger_bond_outpoint: TransactionOutpoint,
    /// The bond a [`ComputeFraudKind::ContradictoryVerification`] proof slashes — pinned by the
    /// stateless check to the contradicting verifier's own bond, so a contradiction proof can never
    /// be aimed at a third party.
    ///
    /// Read for that kind only. The other kinds are claims about a computation rather than proofs,
    /// so they slash nobody at acceptance; who loses is decided later by
    /// [`adjudicate_compute_challenge`], from the certificate's own executor bond or the
    /// challenger's, never from a field the filer chose.
    pub target_bond_outpoint: TransactionOutpoint,
    /// `R_j` as recomputed by the challenger. For
    /// [`ComputeFraudKind::ForgedReceipt`] this must differ from the certificate's.
    pub replay_receipt_hash: Hash64,
    /// For [`ComputeFraudKind::ContradictoryVerification`], the two conflicting
    /// verdicts by one verifier over one `job_id`; empty for the other kinds.
    pub contradictory_verdicts: Vec<VerifierAttestation>,
    /// ML-DSA-87 signature over [`compute_challenge_message`] under
    /// [`COMPUTE_CHALLENGE_MLDSA87_CONTEXT`].
    pub signature: Vec<u8>,
    /// The challenger's declared 64-byte ML-DSA P2PKH spend payload for the reporter
    /// reward, mirroring `SlashingEvidencePayload::reporter_reward_spk_payload`. A
    /// malformed value only misdirects the challenger's own reward.
    pub reporter_reward_spk_payload: [u8; 64],
}

// ---------------------------------------------------------------------
// Signed messages.
// ---------------------------------------------------------------------

/// Digest an executor signs to claim a receipt.
///
/// `network_id` and `bond_outpoint` are bound in for the same reason
/// [`crate::dns_finality::stake_attestation_message`] binds them: without them a
/// signature could be replayed onto another network or re-associated with a
/// different bond.
pub fn compute_certificate_message(
    network_id: &[u8],
    epoch: u64,
    job_id: Hash64,
    receipt_hash: Hash64,
    bond_outpoint: TransactionOutpoint,
) -> Hash {
    let mut hasher = Blake2bParams::new().hash_length(32).key(COMPUTE_CERT_MESSAGE_DOMAIN).to_state();
    hasher.update(network_id);
    hasher.update(&epoch.to_le_bytes());
    hasher.update(job_id.as_byte_slice());
    hasher.update(receipt_hash.as_byte_slice());
    hasher.update(bond_outpoint.transaction_id.as_byte_slice());
    hasher.update(&bond_outpoint.index.to_le_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash::from_bytes(out)
}

/// Digest a verifier signs for its verdict.
///
/// The verdict discriminant and the verifier's own `replay_receipt_hash` are both
/// inside the digest, so a verifier cannot later claim it signed the other verdict —
/// which is exactly what makes [`ComputeFraudKind::ContradictoryVerification`]
/// provable from two signatures alone.
///
/// `certificate_tx_id` is bound in so a verdict cannot be lifted onto a different certificate.
/// It also pins what "contradictory" means: two verdicts by one verifier over the SAME
/// certificate that disagree. Judging two different certificates differently is not an
/// offence — they are different claims, and a verifier may legitimately confirm one and refute
/// the other.
pub fn verifier_verdict_message(
    network_id: &[u8],
    certificate_tx_id: TransactionId,
    job_id: Hash64,
    executor_receipt_hash: Hash64,
    verdict: VerificationVerdict,
    replay_receipt_hash: Hash64,
    bond_outpoint: TransactionOutpoint,
) -> Hash {
    let mut hasher = Blake2bParams::new().hash_length(32).key(VERIFIER_VERDICT_MESSAGE_DOMAIN).to_state();
    hasher.update(network_id);
    hasher.update(certificate_tx_id.as_byte_slice());
    hasher.update(job_id.as_byte_slice());
    hasher.update(executor_receipt_hash.as_byte_slice());
    hasher.update(&[verdict as u8]);
    hasher.update(replay_receipt_hash.as_byte_slice());
    hasher.update(bond_outpoint.transaction_id.as_byte_slice());
    hasher.update(&bond_outpoint.index.to_le_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash::from_bytes(out)
}

/// **Phase 1 of the two-phase sortition** (§6): an executor's on-chain commitment to run a
/// specific job, published *before* the randomness that picks its auditors exists.
///
/// # Why the certificate alone is not enough
///
/// §6 requires verifiers be drawn "after the executor has committed, using randomness from the
/// finalized chain". A single-transaction certificate cannot satisfy that: whatever chain value
/// is used as the beacon is already on chain when the executor decides what to submit, so the
/// executor can vary `sampling_seed` — and therefore `job_id`, and therefore its committee —
/// until it draws auditors it likes. Grinding a hash is cheap; running the job is not, but the
/// executor only has to grind before doing the work once.
///
/// Splitting the flow removes the freedom rather than pricing it:
///
/// 1. The executor publishes this commitment, pinning `(job_id, executor_id)`.
/// 2. The beacon becomes the canonical lagged anchor of the epoch **after** the one that
///    accepted the commitment — a block that did not exist when the commitment was made.
/// 3. Only then does the certificate name its verifiers.
///
/// At step 1 the executor has already fixed the spec (hence the seed, hence `job_id`), and the
/// beacon at step 2 is determined by blocks it does not control. Grinding at step 1 is grinding
/// against randomness that has not been drawn.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ComputeCommitmentPayload {
    pub version: u16,
    /// `H(S_j)` of the job being committed to. Binds the entire spec — model, runtime,
    /// quantization, input, seed and token limit — so none of it can change afterwards.
    pub job_id: Hash64,
    pub executor_id: Hash64,
    pub executor_bond_outpoint: TransactionOutpoint,
    /// The job's input bytes — the prompt (plus any context) the spec's `p_j` commits to.
    ///
    /// Published here, at most [`MAX_JOB_INPUT_BYTES`], because a full-replay verifier needs the
    /// actual input and the spec carries only its digest. It rides the commitment rather than the
    /// certificate so the entire job is public *before* the beacon that draws its committee
    /// exists — a verifier learns nothing at audit time that the rest of the network did not
    /// already have.
    pub input: Vec<u8>,
    /// ML-DSA-87 signature over [`compute_commitment_message`] under
    /// [`COMPUTE_COMMITMENT_MLDSA87_CONTEXT`].
    pub signature: Vec<u8>,
}

/// Digest an executor signs to commit to a job.
///
/// `input_commitment` is bound in as well as `job_id`, even though `job_id` already covers it
/// through [`job_spec_id`]: without it the input bytes would be the one part of the commitment no
/// signature covers, and anyone relaying the transaction could swap them for bytes with a
/// different digest. The resulting commitment would still verify, but no certificate could ever
/// resolve against it — a free grief against an executor that has already paid for the job.
pub fn compute_commitment_message(
    network_id: &[u8],
    job_id: Hash64,
    input_commitment: Hash64,
    bond_outpoint: TransactionOutpoint,
) -> Hash {
    let mut hasher = Blake2bParams::new().hash_length(32).key(COMPUTE_COMMITMENT_MESSAGE_DOMAIN).to_state();
    hasher.update(network_id);
    hasher.update(job_id.as_byte_slice());
    hasher.update(input_commitment.as_byte_slice());
    hasher.update(bond_outpoint.transaction_id.as_byte_slice());
    hasher.update(&bond_outpoint.index.to_le_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash::from_bytes(out)
}

/// Keyed-BLAKE2b-512 domain for [`residual_commitment`].
///
/// This is PALW's own `MatchProjectionV1` residual domain, and it must stay byte-identical to it:
/// the value it produces is what a runtime writes into [`ComputeReceipt::trace_commitment`], so a
/// consensus fold that disagreed with the runtime's would reject every honest replay proof.
/// `misaka_palw` re-exports this constant rather than defining its own.
pub const REPLAY_RESIDUAL_COMMITMENT_KEY: &[u8] = b"misaka-vlt-palw-residual-v1";

/// The execution-derived values a [`ComputeReceipt`] folds into its `trace_commitment` — the
/// preimage of that digest.
///
/// # Why a verdict has to reveal these
///
/// Without them a `Confirmed` verdict says only `replay_receipt_hash == executor_receipt_hash`,
/// and the executor **published** `executor_receipt_hash` in its certificate. Copying one field
/// produces a verdict that is self-consistent, signature-valid, and counted — with no job run.
/// Once auditing pays, copying strictly dominates auditing, so paying for verdicts in that shape
/// would fund rubber-stamping and make a forged receipt reliably confirmable.
///
/// These fields are the fold's *preimage*, and [`residual_commitment`] is one-way, so a verifier
/// that only saw the certificate cannot produce them. A verifier that ran the job has them for
/// free. That asymmetry is the whole proof.
///
/// # What it does not prove
///
/// A later committee member can copy them from an earlier verdict already on chain, so this
/// establishes that *someone* replayed independently, not that every confirmer did. It removes
/// the case that matters — a whole committee confirming from the certificate alone, with nobody
/// executing — and leaves a weaker one where the first honest replay still gates the rest.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ReplayResiduals {
    pub job_nullifier: Hash64,
    pub request_commitment: Hash64,
    pub model_profile_id: Hash64,
    pub runtime_class_id: Hash64,
    pub runtime_manifest_hash: Hash64,
    pub shape_profile_id: Hash64,
    pub cu_ruleset_id: Hash64,
    pub canonical_compute_units: u64,
    pub operation_schedule_commitment: Hash64,
    pub schedule_event_count: u64,
    pub trace_scheme_id: Hash64,
    pub gemm_trace_root: Hash64,
    pub trace_event_count: u64,
}

/// What a verifier publishes to show it actually ran the job: the receipt its own execution
/// produced, and the residuals that receipt's `trace_commitment` folds.
///
/// Required for **both** verdicts, and that symmetry is the point. A refutation used to be the one
/// claim in the protocol that cost nothing to fabricate — "my replay gave something else" is
/// satisfied by any hash at all — while being strong enough on its own to deny an honest executor
/// its credit. Once the §6 audit fee pays for refutations too, fabricating them became profitable.
/// Carrying the receipt makes a refutation cost exactly what a confirmation costs: one execution.
///
/// Everything about it is checkable from the verdict alone, with no certificate lookup:
/// `compute_receipt_hash_for_job(job_id, receipt)` must equal the declared `replay_receipt_hash`,
/// and [`residual_commitment`] of the residuals must equal `receipt.trace_commitment`. For a
/// confirmation that chain of equalities reaches the certificate's own `trace_commitment` by
/// construction — the receipt hash covers it — so the certificate binding falls out rather than
/// needing its own rule.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ReplayProof {
    /// The receipt this verifier's own execution produced.
    pub receipt: ComputeReceipt,
    /// The preimage of `receipt.trace_commitment`.
    pub residuals: ReplayResiduals,
}

impl ReplayProof {
    /// Whether the proof is internally consistent and produces `replay_receipt_hash` for `job_id`.
    ///
    /// Fabricating one means producing residuals that fold to a chosen digest — a preimage attack
    /// on BLAKE2b-512 — so the cheap way to satisfy this is to run the job.
    pub fn attests(&self, job_id: Hash64, replay_receipt_hash: Hash64) -> bool {
        residual_commitment(&self.residuals) == self.receipt.trace_commitment
            && compute_receipt_hash_for_job(job_id, &self.receipt) == replay_receipt_hash
    }
}

/// Fold [`ReplayResiduals`] into the `trace_commitment` a [`ComputeReceipt`] carries.
///
/// Fixed-width little-endian scalars in a fixed field order, exactly as the runtime bridge writes
/// them — a consensus identity must not move if a serializer's layout changes.
pub fn residual_commitment(r: &ReplayResiduals) -> Hash64 {
    let mut h = Blake2bParams::new().hash_length(64).key(REPLAY_RESIDUAL_COMMITMENT_KEY).to_state();
    h.update(r.job_nullifier.as_byte_slice());
    h.update(r.request_commitment.as_byte_slice());
    h.update(r.model_profile_id.as_byte_slice());
    h.update(r.runtime_class_id.as_byte_slice());
    h.update(r.runtime_manifest_hash.as_byte_slice());
    h.update(r.shape_profile_id.as_byte_slice());
    h.update(r.cu_ruleset_id.as_byte_slice());
    h.update(&r.canonical_compute_units.to_le_bytes());
    h.update(r.operation_schedule_commitment.as_byte_slice());
    h.update(&r.schedule_event_count.to_le_bytes());
    h.update(r.trace_scheme_id.as_byte_slice());
    h.update(r.gemm_trace_root.as_byte_slice());
    h.update(&r.trace_event_count.to_le_bytes());
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// A sortitioned verifier's standalone verdict transaction — phase 3 of a job's life.
///
/// The verifier discovers it was drawn for a job by re-deriving the committee from the chain
/// (the certificate and its commitment are both on chain, and so is the beacon), re-executes the
/// job independently, and publishes this. Consensus collects the verdicts belonging to each
/// certificate and applies [`verify_compute_certificate`] to them.
///
/// Nothing here is taken on trust from the executor: `executor_receipt_hash` records what the
/// executor claimed, `replay_receipt_hash` records what this verifier independently computed, and
/// the verdict must agree with their comparison.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ComputeVerdictPayload {
    pub version: u16,
    /// The certificate transaction this verdict judges.
    pub certificate_tx_id: TransactionId,
    /// `H(S_j)` — redundant with the certificate but carried so verdicts can be indexed and
    /// contradiction-checked without a transaction lookup.
    pub job_id: Hash64,
    /// `R_j` as the executor claimed it.
    pub executor_receipt_hash: Hash64,
    pub verifier_id: Hash64,
    /// The verifier's own bond. A verdict must be slashable, or refuting honest work would be
    /// free (§6: "互いに矛盾する Certificate へ署名した場合は slash 対象").
    pub bond_outpoint: TransactionOutpoint,
    pub verdict: VerificationVerdict,
    /// `R_j` as this verifier independently recomputed it.
    pub replay_receipt_hash: Hash64,
    /// Proof that this verifier actually executed the job — required for **both** verdicts.
    ///
    /// See [`ReplayProof`]. A confirmation without it is one field copied off the certificate; a
    /// refutation without it is any hash at all, and under the §6 audit fee both would be paid.
    pub replay_proof: ReplayProof,
    /// ML-DSA-87 signature over [`verifier_verdict_message`] under
    /// [`VERIFIER_VERDICT_MLDSA87_CONTEXT`].
    pub signature: Vec<u8>,
}

impl ComputeVerdictPayload {
    /// The [`VerifierAttestation`] view consensus feeds to [`verify_compute_certificate`].
    pub fn as_attestation(&self) -> VerifierAttestation {
        VerifierAttestation {
            version: self.version,
            verifier_id: self.verifier_id,
            bond_outpoint: self.bond_outpoint,
            verdict: self.verdict,
            replay_receipt_hash: self.replay_receipt_hash,
            signature: self.signature.clone(),
        }
    }

    /// Whether this verdict is self-consistent: the declared verdict must be what comparing the
    /// two receipt hashes actually implies.
    ///
    /// A "Confirmed" over a hash the verifier did not reproduce, or a "Refuted" over one it did,
    /// is a lie on its face — rejected here rather than silently counted.
    /// Whether this verdict stands on its own: the declared verdict is what comparing the two
    /// receipt hashes implies, and the replay proof actually produces the replay hash it declares.
    ///
    /// Entirely stateless — no certificate is needed. For a confirmation the proof necessarily
    /// reaches the certificate's own `trace_commitment`, because `replay_receipt_hash` equals the
    /// executor's and a receipt hash covers its trace commitment.
    pub fn is_self_consistent(&self) -> bool {
        let compared = match self.verdict {
            VerificationVerdict::Confirmed => self.replay_receipt_hash == self.executor_receipt_hash,
            VerificationVerdict::Refuted => self.replay_receipt_hash != self.executor_receipt_hash,
        };
        compared && self.replay_proof.attests(self.job_id, self.replay_receipt_hash)
    }
}

/// A validator's on-chain declaration that it can execute — and therefore audit — a specific
/// `(model, runtime, determinism class)` profile.
///
/// This exists because verifier sortition must draw *within* a determinism class
/// ([`select_verifiers`]), and consensus has to learn each validator's class from somewhere.
/// Deriving it from a validator's past certificates cannot bootstrap: the very first
/// certificate on a network would have no in-class candidates to audit it. A signed
/// declaration breaks that circularity without touching [`crate::dns_finality::StakeBondPayload`],
/// so existing bonds stay valid.
///
/// Declarations **expire**. A validator that stops running the runtime stops being drawn into
/// committees instead of silently sinking every job it is sampled for — under
/// refutation-dominant acceptance, an absent verifier is the difference between a job that
/// mints and one that does not.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ComputeCapabilityPayload {
    pub version: u16,
    /// `validator_id` of the declaring validator; must equal its bond's `validator_pubkey_hash`.
    pub validator_id: Hash64,
    /// The bond that makes this declaration slashable — a validator that declares a class it
    /// cannot actually run has staked collateral behind the claim.
    pub bond_outpoint: TransactionOutpoint,
    /// `(h_M, h_R)` of the profile, and the determinism class they belong to. All three are
    /// checked against the network's [`ModelCostTable`], so a validator cannot declare a class
    /// for an unregistered profile or mis-state a registered one's class.
    pub model_weights_hash: Hash64,
    pub runtime_hash: Hash64,
    pub runtime_class_id: Hash64,
    /// DAA score at which the declaration lapses. Consensus caps this at
    /// [`VltParams::max_capability_validity_blocks`] past acceptance, so a one-off declaration
    /// cannot keep a long-departed operator in committees forever.
    pub expiry_daa_score: u64,
    /// ML-DSA-87 signature over [`compute_capability_message`] under
    /// [`COMPUTE_CAPABILITY_MLDSA87_CONTEXT`].
    pub signature: Vec<u8>,
}

/// Digest a validator signs to declare a compute capability.
pub fn compute_capability_message(
    network_id: &[u8],
    validator_id: Hash64,
    bond_outpoint: TransactionOutpoint,
    model_weights_hash: Hash64,
    runtime_hash: Hash64,
    runtime_class_id: Hash64,
    expiry_daa_score: u64,
) -> Hash {
    let mut hasher = Blake2bParams::new().hash_length(32).key(COMPUTE_CAPABILITY_MESSAGE_DOMAIN).to_state();
    hasher.update(network_id);
    hasher.update(validator_id.as_byte_slice());
    hasher.update(bond_outpoint.transaction_id.as_byte_slice());
    hasher.update(&bond_outpoint.index.to_le_bytes());
    hasher.update(model_weights_hash.as_byte_slice());
    hasher.update(runtime_hash.as_byte_slice());
    hasher.update(runtime_class_id.as_byte_slice());
    hasher.update(&expiry_daa_score.to_le_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash::from_bytes(out)
}

/// Digest a challenger signs for a fraud proof.
pub fn compute_challenge_message(
    network_id: &[u8],
    certificate_tx_id: TransactionId,
    job_id: Hash64,
    kind: ComputeFraudKind,
    executor_receipt_hash: Hash64,
    replay_receipt_hash: Hash64,
    bond_outpoint: TransactionOutpoint,
) -> Hash {
    let mut hasher = Blake2bParams::new().hash_length(32).key(COMPUTE_CHALLENGE_MESSAGE_DOMAIN).to_state();
    hasher.update(network_id);
    hasher.update(certificate_tx_id.as_byte_slice());
    hasher.update(job_id.as_byte_slice());
    hasher.update(&[kind as u8]);
    hasher.update(executor_receipt_hash.as_byte_slice());
    hasher.update(replay_receipt_hash.as_byte_slice());
    hasher.update(bond_outpoint.transaction_id.as_byte_slice());
    hasher.update(&bond_outpoint.index.to_le_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash::from_bytes(out)
}

// ---------------------------------------------------------------------
// PALW runtime registry — the pinned Qwen3.6-35B-A3B Metal profile.
// ---------------------------------------------------------------------

/// Keyed-BLAKE2b-512 domain for [`derive_model_weights_hash`].
pub const MODEL_IDENTITY_KEY: &[u8] = b"misaka-vlt-model-identity-v1";
/// Keyed-BLAKE2b-512 domain for [`derive_runtime_hash`].
pub const RUNTIME_IDENTITY_KEY: &[u8] = b"misaka-vlt-runtime-identity-v1";
/// Keyed-BLAKE2b-512 domain for [`derive_runtime_class_id`].
pub const RUNTIME_CLASS_KEY: &[u8] = b"misaka-vlt-runtime-class-v1";

/// Immutable upstream pins of the supported PALW profile, copied verbatim from the runtime
/// repository's `config/runtime-pins.sh`. Public identifiers only.
///
/// These are what make `h_M` / `h_R` *checkable*: anyone can re-derive the registered hashes
/// from these strings and confirm the consensus entry names the artifact they actually have.
pub mod palw_pins {
    /// `PALW_GGUF_SHA256` — the Q4_K_M GGUF content digest (also the Ollama blob revision).
    pub const GGUF_SHA256: &str = "1dc494614bee8a3bc00e79fe5a49da0fc1c36b3b118c4156e223e98e5a0a671b";
    /// `PALW_GGUF_SIZE` — bytes.
    pub const GGUF_SIZE: u64 = 23_938_321_728;
    /// `PALW_GGUF_FILENAME`.
    pub const GGUF_FILENAME: &str = "Qwen3.6-abliterated-35b-Claude-4.7-Q4_K_M.gguf";
    /// `PALW_BASE_REPO_ID` / `PALW_BASE_REVISION` — the base metadata (tokenizer, config) the
    /// GGUF was produced from. Part of the model identity because a different tokenizer turns
    /// the same weights into a different function from prompt bytes to tokens.
    pub const BASE_REPO_ID: &str = "huihui-ai/Huihui-Qwen3.6-35B-A3B-Claude-4.7-Opus-abliterated";
    pub const BASE_REVISION: &str = "ac18882735d037f6074a7630eb68d85db8234c25";

    /// `PALW_LLAMA_COMMIT` — pinned llama.cpp commit.
    pub const LLAMA_COMMIT: &str = "12127defda4f41b7679cb2477a4b0d65ee6a0c8f";
    /// `PALW_LLAMA_VERSION` — `LLAMA_BUILD_NUMBER`.
    pub const LLAMA_BUILD_NUMBER: u64 = 10_015;
    /// `PALW_LLAMA_PATCH_SHA256` — the PALW observer patch applied on top of the commit.
    pub const LLAMA_PATCH_SHA256: &str = "d155a88b7c11ee74f48011760cb1a37773a694c8cab28258ee108c85e2f9e02c";

    /// Canonical tag for the build profile in `PALW_METAL_CMAKE_ARGS`: Release, arm64, Metal on
    /// (embedded shader library), CUDA off, LTO off, native off, Accelerate/BLAS on.
    ///
    /// Every one of those flags can change floating-point results, so the build profile is part
    /// of the runtime identity, not metadata about it.
    pub const METAL_BUILD_PROFILE: &str =
        "release/arm64/metal-embed/no-native/no-lto/no-kleidiai/accelerate-blas-apple/cuda-off/shared";

    /// The determinism **class** tag. PALW's currently-production class is "fp per-vendor":
    /// byte-identical results only within one microarchitecture and toolchain. This tag names
    /// that class for Apple Silicon + Metal.
    pub const METAL_RUNTIME_CLASS: &str = "palw-fp-per-vendor/apple-metal-arm64/v1";
}

/// `h_M` for a GGUF-distributed model: keyed digest over the content digest, size, filename, and
/// the base-metadata revision the GGUF was converted from.
///
/// Widens the upstream 32-byte SHA-256 into the overlay's 64-byte [`Hash64`] identity space
/// while binding the surrounding facts, so two different conversions of the same weights (a
/// different tokenizer revision, say) are different models to consensus — as they must be, since
/// they map the same prompt to different tokens.
pub fn derive_model_weights_hash(
    gguf_sha256_hex: &str,
    gguf_size: u64,
    filename: &str,
    base_repo: &str,
    base_revision: &str,
) -> Hash64 {
    let mut hasher = Blake2bParams::new().hash_length(64).key(MODEL_IDENTITY_KEY).to_state();
    hasher.update(gguf_sha256_hex.as_bytes());
    hasher.update(&gguf_size.to_le_bytes());
    hasher.update(filename.as_bytes());
    hasher.update(base_repo.as_bytes());
    hasher.update(base_revision.as_bytes());
    let mut out = [0u8; 64];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// `h_R` — the exact inference runtime build: upstream commit, applied patch, build number, and
/// the build profile. Corresponds to PALW's `runtime_manifest_hash` role in `MatchProjectionV1`.
pub fn derive_runtime_hash(commit: &str, patch_sha256_hex: &str, build_number: u64, build_profile: &str) -> Hash64 {
    let mut hasher = Blake2bParams::new().hash_length(64).key(RUNTIME_IDENTITY_KEY).to_state();
    hasher.update(commit.as_bytes());
    hasher.update(patch_sha256_hex.as_bytes());
    hasher.update(&build_number.to_le_bytes());
    hasher.update(build_profile.as_bytes());
    let mut out = [0u8; 64];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// Per-job token ceiling for the PALW profile. Deliberately far below the model's 262 144
/// context: a verifier must FULLY re-execute every accepted job under
/// [`VerificationScheme::CanonicalFullReplay`], so this is an audit-cost bound, not a
/// capability bound. Raising it multiplies every honest verifier's workload.
pub const PALW_QWEN36_MAX_TOKENS: u32 = 4096;

/// The registered [`ModelCostEntry`] for the pinned PALW Qwen3.6-35B-A3B Metal profile.
pub fn palw_qwen36_metal_entry() -> ModelCostEntry {
    ModelCostEntry {
        model_weights_hash: derive_model_weights_hash(
            palw_pins::GGUF_SHA256,
            palw_pins::GGUF_SIZE,
            palw_pins::GGUF_FILENAME,
            palw_pins::BASE_REPO_ID,
            palw_pins::BASE_REVISION,
        ),
        runtime_hash: derive_runtime_hash(
            palw_pins::LLAMA_COMMIT,
            palw_pins::LLAMA_PATCH_SHA256,
            palw_pins::LLAMA_BUILD_NUMBER,
            palw_pins::METAL_BUILD_PROFILE,
        ),
        runtime_class_id: derive_runtime_class_id(palw_pins::METAL_RUNTIME_CLASS),
        // Single registered model ⇒ it IS the reference, so ρ = 1.0 and one VLT unit is one
        // reference-token-equivalent. A second model would be calibrated against this one.
        rho_micro: VLT_MICRO as u64,
        max_tokens: PALW_QWEN36_MAX_TOKENS,
    }
}

/// The **determinism class** two replicas must share for a byte-exact comparison to be a fair
/// test. Corresponds to PALW's `runtime_class_id`.
///
/// This is coarser than [`derive_runtime_hash`] on purpose: several runtime builds can belong to
/// one class, but a comparison *across* classes is meaningless under the current fp-per-vendor
/// regime. See [`select_verifiers`] for why consensus must enforce it.
pub fn derive_runtime_class_id(class_tag: &str) -> Hash64 {
    let mut hasher = Blake2bParams::new().hash_length(64).key(RUNTIME_CLASS_KEY).to_state();
    hasher.update(class_tag.as_bytes());
    let mut out = [0u8; 64];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash64::from_bytes(out)
}

// ---------------------------------------------------------------------
// Verifier sortition (§6).
// ---------------------------------------------------------------------

/// Deterministically select the verifier committee for a job (§6).
///
/// The paper's requirement is that verifiers are chosen **after** the executor has
/// committed, using randomness from the finalized chain, so "Executor が事前に協力者
/// だけを選ぶことを防ぐ". Both halves matter and both are enforced here:
///
/// * `beacon` must be a block hash from history the executor could not influence when
///   it built the receipt — the caller supplies the DNS-confirmed anchor at the
///   certificate's acceptance height. Passing an executor-chosen value would hand the
///   committee back to the executor.
/// * `executor_id` is excluded from the result, so an executor can never verify its
///   own job (§6 "Executor と Verifier を分離").
///
/// Selection is a keyed-hash sort — each candidate's ticket is
/// `BLAKE2b-512(VERIFIER_SORTITION_KEY, job_id ‖ executor_id ‖ beacon ‖ candidate)`
/// and the `k` lowest tickets win, ties broken by `validator_id`. Deterministic and
/// independent of candidate ordering, so every node derives the same committee.
///
/// This is **weight-blind on purpose**: verification is an audit role, not a voting
/// role, so sampling it uniformly keeps a high-`W_i` validator from also dominating
/// the audit of its competitors' compute.
///
/// # Determinism class matching (not optional)
///
/// `candidates` is `(validator_id, runtime_class_id)` and only candidates whose class equals
/// `runtime_class_id` are drawable. This is a **correctness requirement**, not a policy
/// preference, and it follows from what the runtime actually guarantees.
///
/// PALW's production determinism class is *fp per-vendor*: byte-identical results hold only
/// within one microarchitecture and toolchain (its integration spec: "k=2 pairs must be from
/// same vendor class"; the cross-vendor integer-canonical class is still under development).
/// A verifier in a different class re-executing an honest executor's job would legitimately
/// compute a different `R_j` and sign `Refuted`. Because acceptance is refutation-dominant
/// ([`verify_compute_certificate`]), a single such verifier would zero an honest validator's
/// VLT — and a `ForgedReceipt` challenge built on that divergence would slash an honest
/// executor's bond. Cross-class sampling would therefore not merely be noisy, it would make
/// the honest strategy unprofitable and the slashing conditions unsound.
///
/// A job whose class has fewer than `k` other members simply draws a smaller committee; the
/// caller's `min_verifier_confirmations` then decides whether that is enough to mint.
pub fn select_verifiers(
    job_id: Hash64,
    executor_id: Hash64,
    beacon: BlockHash,
    runtime_class_id: Hash64,
    candidates: &[(Hash64, Hash64)],
    k: usize,
) -> Vec<Hash64> {
    if k == 0 {
        return Vec::new();
    }
    let mut ticketed: Vec<(Hash64, Hash64)> = candidates
        .iter()
        .filter(|(id, class)| *id != executor_id && *class == runtime_class_id)
        .map(|(c, _)| {
            let mut hasher = Blake2bParams::new().hash_length(64).key(VERIFIER_SORTITION_KEY).to_state();
            hasher.update(job_id.as_byte_slice());
            hasher.update(executor_id.as_byte_slice());
            hasher.update(beacon.as_bytes().as_slice());
            hasher.update(c.as_byte_slice());
            let mut out = [0u8; 64];
            out.copy_from_slice(hasher.finalize().as_bytes());
            (Hash64::from_bytes(out), *c)
        })
        .collect();
    // Ticket first, `validator_id` as the tie-break: a pair of candidates whose
    // tickets collide must still order identically on every node.
    ticketed.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    ticketed.truncate(k);
    ticketed.into_iter().map(|(_, id)| id).collect()
}

// ---------------------------------------------------------------------
// Model cost table + params.
// ---------------------------------------------------------------------

/// One consensus-registered model/runtime pair and its `ρ` cost multiplier.
///
/// `ρ(S_j)` is a **consensus parameter, not an executor input** (§3.2: "Executor は
/// 変更できない"). Registering the pair here is what makes a job's compute cost
/// agreed rather than self-declared: a job naming an unregistered
/// `(model_weights_hash, runtime_hash)` mints zero VLT, so nobody can invent a
/// fictitious expensive model to inflate their own weight.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ModelCostEntry {
    pub model_weights_hash: Hash64,
    pub runtime_hash: Hash64,
    /// The determinism class this runtime belongs to — PALW's `runtime_class_id`. Consensus
    /// draws a job's verifier committee only from validators declaring this same class, because
    /// a byte-exact replay comparison is only a fair test within a class. See
    /// [`select_verifiers`].
    pub runtime_class_id: Hash64,
    /// `ρ` in [`VLT_MICRO`] fixed point (`1_000_000` = the reference model, ×1.0).
    /// Reflects active parameters, context length, precision, and sparsity — i.e. how
    /// much real compute one token of this model costs relative to the reference.
    pub rho_micro: u64,
    /// Per-job token ceiling for this model. A spec whose `max_tokens` exceeds it is
    /// not consensus-eligible, bounding verifier replay cost per model.
    pub max_tokens: u32,
}

/// The per-network registry of consensus-eligible models (§3.2, §8.4 "Model table …
/// は consensus security parameter").
///
/// Fixed-capacity so [`VltParams`] is `const`-constructible; `len` entries are live
/// and the rest are the zero default.
#[derive(Copy, Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ModelCostTable {
    pub len: u8,
    pub entries: [ModelCostEntry; MAX_MODEL_COST_ENTRIES],
}

impl Default for ModelCostTable {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl ModelCostTable {
    /// No registered model — every job normalizes to zero VLT. The correct default
    /// for a network whose fence is inert.
    pub const EMPTY: Self = Self {
        len: 0,
        entries: [ModelCostEntry {
            model_weights_hash: Hash64::from_bytes([0u8; 64]),
            runtime_hash: Hash64::from_bytes([0u8; 64]),
            runtime_class_id: Hash64::from_bytes([0u8; 64]),
            rho_micro: 0,
            max_tokens: 0,
        }; MAX_MODEL_COST_ENTRIES],
    };

    /// The single-model registry for the pinned PALW Qwen3.6-35B-A3B Metal profile.
    ///
    /// `ρ = 1.0`: with one registered model it *is* the reference, so `x_j` reduces to
    /// `a·t_in + b·t_out` and one VLT unit is one reference-token-equivalent. Introducing a
    /// second model later means calibrating its `ρ` against this one, not renumbering this.
    ///
    /// `max_tokens = 4096` bounds what one job can cost a verifier. The model's own context is
    /// 262 144, but a verifier must *fully re-execute* every accepted job under
    /// [`VerificationScheme::CanonicalFullReplay`], so the ceiling is an audit-cost decision,
    /// not a capability one. Raising it multiplies the honest verifier's workload.
    ///
    /// Not `const` because the identities are keyed BLAKE2b digests of the pinned strings
    /// ([`palw_pins`]); `tests::palw_registry_derives_from_the_published_pins` re-derives them,
    /// so a changed pin is a failing test rather than a silent identity drift.
    pub fn palw_qwen36_metal() -> Self {
        let mut table = Self::EMPTY;
        table.len = 1;
        table.entries[0] = palw_qwen36_metal_entry();
        table
    }

    pub fn live(&self) -> &[ModelCostEntry] {
        &self.entries[..(self.len as usize).min(MAX_MODEL_COST_ENTRIES)]
    }

    /// Look up the `(model, runtime)` pair a spec names. `None` ⇒ unregistered ⇒
    /// the job mints zero VLT.
    pub fn lookup(&self, model_weights_hash: Hash64, runtime_hash: Hash64) -> Option<&ModelCostEntry> {
        self.live().iter().find(|e| e.model_weights_hash == model_weights_hash && e.runtime_hash == runtime_hash)
    }
}

/// Per-network VLT / weighted-BFT parameters.
///
/// Carried inside [`crate::dns_finality::DnsParams`] so it inherits the same
/// `Option` gating the rest of the overlay has, and fenced independently by
/// [`Self::vlt_activation_daa_score`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct VltParams {
    /// DAA score at which voting weight switches from bonded stake to
    /// `W_i(E) = min{C_i(E), λ·B_i(E)}`, and the epoch credit switches from the φS
    /// graded floor to the §4 `Q(E) = ⌊2W(E)/3⌋ + 1` quorum.
    ///
    /// `u64::MAX` (inert) on every shipped preset: below the fence the overlay is
    /// byte-identical to the pre-VLT stake-weighted behaviour. Activating it is a
    /// hard fork and must be coordinated across the mesh — and must not be scheduled
    /// before the active set can actually produce verified compute, since with no VLT
    /// every `W_i(E)` is 0, `W(E)` is 0, and no epoch can reach quorum.
    ///
    /// [`Self::min_network_compute`] is the graded form of that same warning: a network above the
    /// fence but below `W_min` does not finalize on the compute dimension either, and falls back
    /// to PoW ordering until it has done enough work. Scheduling the fence is therefore a question
    /// about how much verified compute the mesh will be producing by that height, not merely
    /// whether it produces any.
    pub vlt_activation_daa_score: u64,

    /// `K` — credit window length in epochs (§4 eq. 5). Compute older than `K`
    /// epochs contributes nothing at all.
    pub credit_window_epochs: u32,

    /// Geometric decay ratio for `d_τ` in basis points: `d_1 = 1.0` and
    /// `d_{τ+1} = d_τ × credit_decay_bps / 10_000`. `10_000` = no decay (a flat
    /// window); smaller = faster forgetting. Satisfies the eq. (5) requirement
    /// `1 = d_1 ≥ d_2 ≥ … ≥ d_K > 0` for any value in `1..=10_000`.
    pub credit_decay_bps: u16,

    /// Minimum epoch delay before compute counts (§4, §8.3). **Must be ≥ 1**: it is
    /// what stops VLT minted on a fork from inflating that same fork's voting power.
    /// [`Self::is_coherent`] rejects 0.
    pub credit_delay_epochs: u32,

    /// `λ` — maximum VLT weight one whole bonded KAS may collateralize, in
    /// [`VLT_MICRO`] units of reference-token-equivalent. Converts the bond into the
    /// `W_i(E)` ceiling; see [`effective_voting_weight`].
    ///
    /// Note the direction of the constraint: raising λ does **not** give a validator
    /// more voting power, it only lets more of the compute it actually performed
    /// count. Lowering λ tightens how much verified compute a given bond may back.
    pub lambda_vlt_per_kas: u64,

    /// `a` — relative cost of one prefill token, in [`VLT_MICRO`] units.
    pub prefill_cost_micro: u64,
    /// `b` — relative cost of one decode token, in [`VLT_MICRO`] units. Larger than
    /// `a` on every sensible calibration: decode is memory-bandwidth-bound and
    /// dominates real serving cost per token.
    pub decode_cost_micro: u64,

    /// Blocks an accepted certificate must survive un-challenged before its VLT is
    /// credited to `X_i(e)` (§6). Long enough for an honest verifier to actually
    /// re-execute the largest permitted job.
    pub challenge_window_blocks: u64,

    /// Committee size drawn by [`select_verifiers`].
    pub verifier_committee_size: u8,
    /// Confirming verdicts required for `Verify(S_j, R_j, C_j) = 1`. Must be ≥ 1 and
    /// ≤ [`Self::verifier_committee_size`] ([`Self::is_coherent`]).
    pub min_verifier_confirmations: u8,

    /// How many drawn verifiers must refute before the committee's answer is "refuted".
    ///
    /// Refutation dominates acceptance, so this is the bar in front of *destroying* a job's credit
    /// and it has to be a real one. At 1 — the old, implicit value — a single drawn verifier could
    /// zero an honest executor's credit with a made-up hash, and the §6 audit fee would pay it for
    /// doing so. Setting it alongside [`Self::min_verifier_confirmations`] means confirming and
    /// refuting take the same collusion.
    ///
    /// Must be ≥ 1 and ≤ [`Self::verifier_committee_size`] ([`Self::is_coherent`]).
    pub min_verifier_refutations: u8,

    /// Maximum blocks a certificate's phase-1 commitment may lie behind the certificate itself.
    /// Bounds the lookback the credit walk must do to resolve a beacon, and stops an executor
    /// from hoarding old commitments to pick a favourable moment to reveal.
    pub max_commitment_age_blocks: u64,

    /// Maximum blocks past its accepting block that a [`ComputeCapabilityPayload`] may remain
    /// valid. Caps a declaration's reach so an operator that stops running the runtime drops out
    /// of verifier committees instead of sinking every job it is sampled for.
    pub max_capability_validity_blocks: u64,

    /// `W_min` — the minimum total network compute, in µRTE, for an epoch's quorum to mean
    /// anything (§4: "新しい Validator set は W(E) が minimum network compute W_min 以上の場合に
    /// のみ有効化される。下回る場合、set transition を延期し、recovery rule へ移行する").
    ///
    /// # Why a floor is needed at all
    ///
    /// `Q(E) = ⌊2W(E)/3⌋ + 1` is a *fraction* of whatever weight exists, so it is trivially
    /// reachable when almost none does: at `W(E) = 1` the quorum is 1, and a single validator
    /// holding one µRTE of credit finalizes the chain for everybody. [`meets_bft_quorum`] already
    /// refuses `W(E) = 0` for exactly this reason; `W_min` is that same guard with a number behind
    /// it instead of a special case.
    ///
    /// # What happens below it
    ///
    /// The epoch earns no credit and reports as degraded, so `StakeScore` stops accumulating,
    /// the DNS-confirmed anchor stops advancing, and the reorg gate abstains — the overlay steps
    /// back and PoW alone orders the chain. That is the paper's "recovery rule": the network keeps
    /// running on the dimension that still has weight behind it rather than letting a trivial
    /// amount of compute speak for the whole validator set.
    ///
    /// `0` disables the floor, leaving only the `W(E) = 0` guard.
    pub min_network_compute: u128,

    /// §6 audit fee: sompi minted to each verifier whose verdict was counted for a certificate,
    /// paid once, at the block where that certificate leaves its challenge window.
    ///
    /// # Why this exists
    ///
    /// §6 says "Verifier は audit fee を受け取る", and without it verification is unpaid work:
    /// a verifier spends a full re-execution plus a transaction fee and receives nothing, while
    /// the executor it audits collects the VLT. Under refutation-dominant acceptance an absent
    /// verifier does not merely abstain — it denies an honest executor its credit — so a network
    /// that does not pay for auditing does not get audited, and then does not mint.
    ///
    /// # Why it is paid for refutations too
    ///
    /// Both verdicts are paid identically. Paying only confirmations would make refusing to
    /// confirm cost money, which is precisely the bias a fraud-detection role must not have.
    ///
    /// `0` disables the payment. Meaningful only above [`Self::vlt_activation_daa_score`]; below
    /// the fence no verdict is ever counted, so nothing is paid regardless of this value.
    pub audit_fee_sompi: u64,

    /// The consensus-registered model set (§3.2).
    pub model_cost_table: ModelCostTable,
}

impl VltParams {
    /// The shipped default on every current network: fence at `u64::MAX`, no
    /// registered model. Every VLT code path is dormant and the overlay keeps its
    /// legacy bonded-stake weight.
    ///
    /// The non-fence values are the recommended calibration, live as soon as a
    /// network moves its fence — see [`RECOMMENDED_*`](Self::RECOMMENDED_NOTE) on the
    /// individual fields for the reasoning.
    pub const INERT: Self = Self {
        vlt_activation_daa_score: u64::MAX,
        // K = 96 epochs. At the shipped `attestation_epoch_length_blue_score = 100`
        // and ~10 bps this is ~16 minutes of compute history — long enough that a
        // single slow job does not swing a validator's weight, short enough that
        // stopped hardware loses its power quickly (§4).
        credit_window_epochs: 96,
        // 0.97 per epoch ⇒ half-life ~23 epochs (~4 min), and d_96 ≈ 0.054 so the
        // window truncation at K discards very little weight.
        credit_decay_bps: 9_700,
        // The paper's minimum. Do not raise without re-reading §8.3.
        credit_delay_epochs: 1,
        // λ = 100 reference-token-equivalents per bonded KAS. At the unchanged 20M-KAS
        // bond floor this collateralizes 2e9 RTE of weight — comfortably above what a
        // single honest operator produces in a 16-epoch-decayed window, so the cap
        // binds concentration (§4: "VLT を大量に獲得しても十分な slashable Bond がなければ
        // その全量を投票力へ変換できない") without throttling ordinary participation.
        lambda_vlt_per_kas: 100_000_000,
        // a = 1.0, b = 8.0: one decode token counts as eight prefill tokens.
        prefill_cost_micro: 1_000_000,
        decode_cost_micro: 8_000_000,
        // 300 blocks (~30 s at 10 bps), matching the overlay's `max_reorg_horizon_blocks`
        // so a certificate is challengeable for at least as long as its including block
        // is reorgable.
        challenge_window_blocks: 300,
        // Five drawn, three to decide either way. Three is what makes the two quorums mutually
        // exclusive (3 + 3 > 5, checked by `is_coherent`) while still tolerating two committee
        // members being absent, faulty, or hostile — the smallest committee where one bad verifier
        // is neither decisive nor pivotal. The audit cost is five full replays per job, which is
        // the price of that tolerance and the reason not to go higher.
        verifier_committee_size: 5,
        min_verifier_confirmations: 3,
        min_verifier_refutations: 3,
        // ~10 min at 10 bps: comfortably longer than one epoch (so the beacon epoch can mature)
        // plus a full job, and short enough that stockpiled commitments expire.
        max_commitment_age_blocks: 6_000,
        // ~1 day at 10 bps. Long enough that renewal is routine, short enough that a departed
        // operator leaves the committee pool within a day.
        max_capability_validity_blocks: 864_000,
        // W_min = 1e11 µRTE (100 000 reference-token-equivalents). Derived from the smallest
        // network that can verify anything at all: activation needs `1 + min_verifier_confirmations`
        // = 4 validators in one runtime class, and one job at the registered profile's 4096-token
        // ceiling is worth ~3.3e10 µRTE (b = 8.0 per decode token). A handful of validators each
        // having completed roughly one full job is therefore the point below which "the network's
        // compute" is not a meaningful quantity to take two thirds of. A running mesh clears this in its
        // first epochs and never sees it again — the floor exists to exclude the vacuous case, not
        // to throttle a healthy one.
        min_network_compute: 100_000_000_000,
        // 50 000 000 sompi (0.5 KAS) per counted verdict. Calibrated against the cost it has to
        // beat: a full replay at the registered profile's 4096-token ceiling, plus the overlay
        // transaction that carries the verdict (whose own mass-based fee is ~250 000 sompi). Two
        // orders of magnitude above that fee leaves the margin an operator is actually paid for
        // the GPU time, which is the scarce input. It is a per-verdict constant rather than a
        // share of a pool because the work is per-verdict and does not vary with stake.
        audit_fee_sompi: 50_000_000,
        model_cost_table: ModelCostTable::EMPTY,
    };

    /// Doc anchor for the [`Self::INERT`] calibration rationale.
    pub const RECOMMENDED_NOTE: () = ();

    /// Whether the overlay's VLT weighting is live at `daa_score`.
    pub fn is_active_at(&self, daa_score: u64) -> bool {
        daa_score >= self.vlt_activation_daa_score
    }

    /// Internal-consistency check for a preset. Not a consensus rule — a startup /
    /// test assertion that catches a preset which would be unsafe or inert-by-accident
    /// if its fence were ever moved.
    ///
    /// Returns `Err` with a human-readable reason:
    /// * `credit_delay_epochs == 0` — VLT minted on a fork could weight that fork (§8.3).
    /// * `credit_window_epochs == 0` — `C_i(E)` is identically 0, so `W(E) = 0` and
    ///   nothing ever reaches quorum.
    /// * `credit_decay_bps == 0` — `d_2..d_K` are 0, violating eq. (5)'s `d_K > 0`.
    /// * `credit_decay_bps > 10_000` — `d_τ` would *grow*, so old compute would outweigh
    ///   new: the opposite of §4.
    /// * verifier committee / confirmation counts inconsistent, or zero confirmations
    ///   (which would accept an unverified receipt as VLT).
    pub fn is_coherent(&self) -> Result<(), &'static str> {
        if self.credit_delay_epochs == 0 {
            return Err("credit_delay_epochs must be >= 1 (§8.3: same-fork compute must not weight its own fork)");
        }
        if self.credit_window_epochs == 0 {
            return Err("credit_window_epochs must be >= 1 (K = 0 makes every W_i(E) zero)");
        }
        if self.credit_decay_bps == 0 {
            return Err("credit_decay_bps must be > 0 (eq. 5 requires d_K > 0)");
        }
        if self.credit_decay_bps as u128 > VLT_BPS {
            return Err("credit_decay_bps must be <= 10_000 (d_tau must be non-increasing)");
        }
        if self.verifier_committee_size == 0 {
            return Err("verifier_committee_size must be >= 1");
        }
        if self.min_verifier_confirmations == 0 {
            return Err("min_verifier_confirmations must be >= 1 (0 would mint VLT for unverified receipts)");
        }
        if self.min_verifier_confirmations > self.verifier_committee_size {
            return Err("min_verifier_confirmations must be <= verifier_committee_size (otherwise unsatisfiable)");
        }
        if self.min_verifier_refutations == 0 {
            return Err("min_verifier_refutations must be >= 1 (0 would make an empty committee a refutation)");
        }
        if self.min_verifier_refutations > self.verifier_committee_size {
            return Err("min_verifier_refutations must be <= verifier_committee_size (otherwise unsatisfiable)");
        }
        // The two quorums must not be simultaneously reachable, or one committee could both
        // confirm and refute the same job and the answer would depend on which rule ran first.
        if (self.min_verifier_confirmations as u16) + (self.min_verifier_refutations as u16) <= self.verifier_committee_size as u16 {
            return Err("min_verifier_confirmations + min_verifier_refutations must exceed verifier_committee_size");
        }
        if self.verifier_committee_size as usize > MAX_VERIFIER_ATTESTATIONS {
            return Err("verifier_committee_size must be <= MAX_VERIFIER_ATTESTATIONS");
        }
        Ok(())
    }

    /// The §7 unbonding bound: `U ≥ credit window + max challenge period`, so a
    /// validator cannot exit while compute it still draws weight from is challengeable.
    ///
    /// Returned in blocks, given the epoch length in blocks, for a caller to compare
    /// against `DnsParams::unbonding_period_blocks`.
    pub fn min_unbonding_period_blocks(&self, epoch_length_blocks: u64) -> u64 {
        (self.credit_window_epochs as u64)
            .saturating_add(self.credit_delay_epochs as u64)
            .saturating_mul(epoch_length_blocks)
            .saturating_add(self.challenge_window_blocks)
    }
}

// ---------------------------------------------------------------------
// Normalization (§3.2 eq. 4).
// ---------------------------------------------------------------------

/// One epoch's finalized per-validator VLT credit, as persisted by the credit accumulator store.
///
/// Only **finalized** epochs are ever written: an epoch is finalized once it is buried past both
/// the challenge window (no challenge can still zero one of its certificates) and the reorg
/// horizon (no branch under consideration can still change its contents). Below that depth every
/// branch shares the same history, so a cached value is branch-independent and it is sound to
/// read it on the sink path *and* while scoring a candidate branch in the reorg gate.
///
/// This is what turns `C_i(E)`'s `credit_window_epochs`-deep sum from a full rewalk per virtual
/// commit into a walk of only the unfinalized tail.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct VltEpochCredits {
    /// `(validator_id, X_i(epoch))` in µRTE, sorted ascending by `validator_id` so the encoding
    /// is byte-deterministic.
    pub credits: Vec<(Hash64, u128)>,
}

impl VltEpochCredits {
    /// Build from an unordered iterator, canonicalising the order.
    pub fn from_unordered(entries: impl IntoIterator<Item = (Hash64, u128)>) -> Self {
        let mut credits: Vec<(Hash64, u128)> = entries.into_iter().collect();
        credits.sort_by(|a, b| a.0.cmp(&b.0));
        Self { credits }
    }

    pub fn get(&self, validator_id: &Hash64) -> u128 {
        self.credits.binary_search_by(|probe| probe.0.cmp(validator_id)).map(|i| self.credits[i].1).unwrap_or(0)
    }
}

// The store uses an untracked (`Count`) cache policy, so this estimate is never consulted for
// eviction — an empty impl mirrors `EpochTally` / `StakeBondRecord`.
impl kaspa_utils::mem_size::MemSizeEstimator for VltEpochCredits {}

/// Whether an epoch's credits can never change again, and may therefore be cached.
///
/// Requires burial past **both** windows, and both are load-bearing:
/// * `challenge_window_blocks` — a later challenge can still zero one of the epoch's
///   certificates, which would change `X_i(epoch)`.
/// * `max_reorg_horizon_blocks` — above it, a competing branch could carry different
///   certificates for the same epoch, so a single cached value would be wrong for one of them.
pub fn vlt_epoch_finalized(epoch_anchor_daa: u64, tip_daa: u64, challenge_window_blocks: u64, max_reorg_horizon_blocks: u64) -> bool {
    tip_daa.saturating_sub(epoch_anchor_daa) > challenge_window_blocks.saturating_add(max_reorg_horizon_blocks)
}

/// Why a job minted no VLT. Diagnostic only — every variant normalizes to `0`, which
/// is the entire consensus effect.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum VltRejection {
    /// `v_j` is not consensus-eligible in this version.
    IneligibleScheme(VerificationScheme),
    /// `q` is an unknown discriminant.
    UnknownQuantization,
    /// `(h_M, h_R)` is not in the network's [`ModelCostTable`].
    UnregisteredModel,
    /// `L_j` exceeds the registered model's `max_tokens`.
    TokenLimitExceeded { max_tokens: u32, declared: u32 },
    /// The receipt's token counts exceed the spec's `L_j`.
    ReceiptExceedsSpecLimit { max_tokens: u32, produced: u64 },
    /// `Verify(S_j, R_j, C_j) ≠ 1` — too few confirming verdicts, or any refutation.
    VerificationFailed,
}

impl Display for VltRejection {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::IneligibleScheme(s) => write!(f, "verification scheme {s:?} is not consensus-eligible"),
            Self::UnknownQuantization => write!(f, "unknown quantization profile"),
            Self::UnregisteredModel => write!(f, "(model_weights_hash, runtime_hash) is not in the model cost table"),
            Self::TokenLimitExceeded { max_tokens, declared } => {
                write!(f, "spec max_tokens {declared} exceeds the model's registered limit {max_tokens}")
            }
            Self::ReceiptExceedsSpecLimit { max_tokens, produced } => {
                write!(f, "receipt produced {produced} tokens against a spec limit of {max_tokens}")
            }
            Self::VerificationFailed => write!(f, "Verify(S_j, R_j, C_j) != 1"),
        }
    }
}

/// `Verify(S_j, R_j, C_j) = 1` (§3.1 eq. 3) for
/// [`VerificationScheme::CanonicalFullReplay`], given the verdicts already
/// signature-checked and sortition-checked by the caller.
///
/// Refutation still dominates — a job with a refutation quorum mints nothing whatever its
/// confirmations say — but it takes a **quorum**, not one voice.
///
/// A single refuter used to be decisive, which made griefing an honest executor cost one
/// transaction and a made-up hash: one of three drawn verifiers could zero anybody's credit, and
/// under the §6 audit fee be paid for it. Requiring `min_refutations` puts the same collusion bar
/// in front of destroying a job as in front of confirming one, and [`ReplayProof`] makes each
/// refuter pay for an execution to cast its vote. Below both thresholds the job is simply
/// undecided: it mints nothing yet, and will once the committee finishes speaking.
///
/// A confirming verdict must also carry a `replay_receipt_hash` equal to the executor's; a
/// "confirmation" of a different hash is self-contradictory and counts for nothing rather than
/// being silently ignored.
pub fn verify_compute_certificate(
    executor_receipt_hash: Hash64,
    verdicts: &[VerifierAttestation],
    min_confirmations: u8,
    min_refutations: u8,
) -> bool {
    if refutation_quorum_reached(verdicts, min_refutations) {
        return false;
    }
    let confirmations = verdicts
        .iter()
        .filter(|v| v.verdict == VerificationVerdict::Confirmed && v.replay_receipt_hash == executor_receipt_hash)
        .count();
    confirmations >= min_confirmations as usize
}

/// Whether enough drawn verifiers refuted for the refutation to be the committee's answer rather
/// than one member's.
///
/// `min_refutations == 0` would make an empty verdict set a refutation, so it is read as 1 — the
/// coherence check refuses 0 in a preset, and this keeps a hand-built params value from inverting
/// the rule.
pub fn refutation_quorum_reached(verdicts: &[VerifierAttestation], min_refutations: u8) -> bool {
    let refutations = verdicts.iter().filter(|v| v.verdict == VerificationVerdict::Refuted).count();
    refutations > 0 && refutations >= (min_refutations as usize).max(1)
}

/// `x_j = ρ(S_j)·(a·t_j^in + b·t_j^out)` when `Verify = 1`, else `0` (§3.2 eq. 4).
///
/// Result unit is **µRTE** (micro reference-token-equivalents): with the reference
/// model (`ρ = 1.0`), `a = 1.0`, and `b = 8.0`, one prefill token is `1_000_000` and
/// one decode token is `8_000_000`.
///
/// `verified` is the caller's [`verify_compute_certificate`] outcome, kept as a
/// parameter so this stays a pure function of already-checked facts. Everything is
/// `u128`: the largest representable job (`u32::MAX` tokens at the widest sane `ρ`)
/// is ~1e26, far inside `u128`, and `saturating_*` is defensive rather than expected.
pub fn normalize_vlt(spec: &LlmJobSpec, receipt: &ComputeReceipt, params: &VltParams, verified: bool) -> Result<u128, VltRejection> {
    if !spec.verification_scheme.is_consensus_eligible() {
        return Err(VltRejection::IneligibleScheme(spec.verification_scheme));
    }
    if !spec.quantization.is_known() {
        return Err(VltRejection::UnknownQuantization);
    }
    let Some(entry) = params.model_cost_table.lookup(spec.model_weights_hash, spec.runtime_hash) else {
        return Err(VltRejection::UnregisteredModel);
    };
    if spec.max_tokens > entry.max_tokens {
        return Err(VltRejection::TokenLimitExceeded { max_tokens: entry.max_tokens, declared: spec.max_tokens });
    }
    // A receipt may not claim more work than its own spec permitted; otherwise an
    // executor could mint arbitrary VLT from a cheap, small spec.
    let produced = receipt.prefill_tokens as u64 + receipt.decode_tokens as u64;
    if produced > spec.max_tokens as u64 {
        return Err(VltRejection::ReceiptExceedsSpecLimit { max_tokens: spec.max_tokens, produced });
    }
    if !verified {
        return Err(VltRejection::VerificationFailed);
    }
    let token_cost = (params.prefill_cost_micro as u128)
        .saturating_mul(receipt.prefill_tokens as u128)
        .saturating_add((params.decode_cost_micro as u128).saturating_mul(receipt.decode_tokens as u128));
    // ρ and (a,b) are each VLT_MICRO-scaled, so the product carries VLT_MICRO²;
    // divide once to land back in µRTE.
    Ok((entry.rho_micro as u128).saturating_mul(token_cost) / VLT_MICRO)
}

// ---------------------------------------------------------------------
// Epoch weight (§4).
// ---------------------------------------------------------------------

/// `d_τ` for `τ ∈ 1..=K`, in [`VLT_MICRO`] fixed point (`d_1 = VLT_MICRO`).
///
/// Iterative integer multiplication rather than `powi`: `d_{τ+1} = d_τ × bps / 10_000`
/// with truncation at every step is exactly reproducible on every platform, whereas a
/// floating-point power is not. `τ = 0` is outside the eq. (5) sum and returns 0.
pub fn decay_coefficient(tau: u32, decay_bps: u16) -> u128 {
    if tau == 0 {
        return 0;
    }
    let mut d = VLT_MICRO;
    for _ in 1..tau {
        d = d * decay_bps as u128 / VLT_BPS;
        if d == 0 {
            break;
        }
    }
    d
}

/// `C_i(E) = Σ_{τ=1..K} d_τ · X_i(E − τ)` (§4 eq. 5) — validator `i`'s recent
/// compute score for epoch `E`.
///
/// `credited_by_epoch` maps epoch → that epoch's **challenge-window-survived** VLT for
/// this validator (µRTE). Epochs absent from the map contribute nothing.
///
/// The sum starts at `τ = credit_delay_epochs`, not `τ = 1`, so the newest
/// `credit_delay_epochs` epochs are excluded entirely. With the mandatory delay of 1
/// this is the paper's "Epoch E で確定した計算は、最短でも Epoch E + 1 の投票にのみ使用
/// される" — the property that stops a fork from minting its own voting power. Note
/// this shifts the whole decay profile by the delay: with delay 1, epoch `E−1` gets
/// `d_1 = 1.0`.
pub fn recent_compute_score(epoch: u64, credited_by_epoch: &BTreeMap<u64, u128>, params: &VltParams) -> u128 {
    let mut acc: u128 = 0;
    let delay = params.credit_delay_epochs.max(1);
    for tau in delay..=params.credit_window_epochs.saturating_add(delay).saturating_sub(1) {
        let Some(source_epoch) = epoch.checked_sub(tau as u64) else {
            break; // below genesis; every remaining tau is too.
        };
        let Some(&x) = credited_by_epoch.get(&source_epoch) else {
            continue;
        };
        // Re-base the decay index so the first *counted* epoch gets d_1 = 1.0.
        let d = decay_coefficient(tau - delay + 1, params.credit_decay_bps);
        acc = acc.saturating_add(d.saturating_mul(x) / VLT_MICRO);
    }
    acc
}

/// The `λ·B_i(E)` collateral ceiling in µRTE, for a bond of `bond_sompi`.
///
/// `λ` is denominated per whole KAS, so this is `λ × bond_sompi / SOMPI_PER_KASPA`
/// evaluated in `u128` — the division comes last, keeping sub-KAS bonds monotonic
/// rather than truncating them to zero.
pub fn collateral_weight_cap(bond_sompi: u64, lambda_vlt_per_kas: u64) -> u128 {
    (lambda_vlt_per_kas as u128).saturating_mul(bond_sompi as u128) / (SOMPI_PER_KASPA as u128)
}

/// `W_i(E) = min{ C_i(E), λ·B_i(E) }` (§4 eq. 6) — validator `i`'s effective voting
/// weight for epoch `E`, in µRTE.
///
/// Both directions of the `min` are load-bearing, and each blocks a different attack:
///
/// * `C_i = 0` ⇒ `W_i = 0` **regardless of bond**. Buying stake buys no voting power;
///   this is the whole point of the replacement.
/// * `C_i > λ·B_i` ⇒ the excess compute is discarded. A validator that acquires a
///   large amount of verified compute against a small bond cannot convert all of it,
///   which is what keeps voting power backed by slashable collateral — without it,
///   slashing would lose its economic meaning against a compute-rich, bond-poor
///   attacker (§4).
pub fn effective_voting_weight(recent_compute: u128, bond_sompi: u64, lambda_vlt_per_kas: u64) -> u128 {
    recent_compute.min(collateral_weight_cap(bond_sompi, lambda_vlt_per_kas))
}

/// `Q(E) = ⌊2W(E)/3⌋ + 1` (§4 eq. 7) — the finality quorum for total weight `W(E)`.
///
/// Strictly **more** than two thirds, by construction: the `+1` is what makes any two
/// quorums intersect in positive honest weight, which is the §8.1 safety argument. Do
/// not "simplify" this to `≥ 2W/3`; at `W = 3` that would accept 2, and two disjoint
/// sets of weight 2 out of 3 do not intersect honestly.
///
/// `W(E) = 0` yields `Q(E) = 1`, which is unreachable — zero total weight means no
/// validator can sign anything, so the epoch simply does not finalize.
pub fn bft_quorum(total_weight: u128) -> u128 {
    total_weight.saturating_mul(2) / 3 + 1
}

/// Whether an epoch's signed weight reaches [`bft_quorum`] over a network whose total weight is
/// itself large enough to be worth taking two thirds of (§4 `W_min`).
///
/// Both floors say the same thing at different scales. `total_weight == 0` is always `false`
/// because an epoch with no weight has not been finalized by anybody; `total_weight <
/// min_network_compute` is `false` because a quorum is a *fraction*, and two thirds of almost
/// nothing is almost nothing. Without the second, a network with one µRTE of verified compute has
/// `Q(E) = 1` and its single holder finalizes the chain for everyone.
///
/// `min_network_compute == 0` leaves only the zero guard — see
/// [`VltParams::min_network_compute`] for what a network below the floor does instead.
pub fn meets_bft_quorum(signed_weight: u128, total_weight: u128, min_network_compute: u128) -> bool {
    if total_weight == 0 || total_weight < min_network_compute {
        return false;
    }
    // Clamp: an over-count (which the callers' dedup already prevents) must not be
    // able to manufacture a quorum that the real weight does not support.
    signed_weight.min(total_weight) >= bft_quorum(total_weight)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h64(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }

    fn outpoint(b: u8) -> TransactionOutpoint {
        TransactionOutpoint::new(TransactionId::from_bytes([b; 64]), 0)
    }

    fn params_with_model() -> VltParams {
        let mut table = ModelCostTable::EMPTY;
        table.len = 1;
        table.entries[0] = ModelCostEntry {
            model_weights_hash: h64(1),
            runtime_hash: h64(2),
            runtime_class_id: h64(3),
            rho_micro: 1_000_000,
            max_tokens: 100_000,
        };
        VltParams { model_cost_table: table, ..VltParams::INERT }
    }

    fn spec() -> LlmJobSpec {
        LlmJobSpec {
            version: VLT_PAYLOAD_VERSION_V1,
            model_weights_hash: h64(1),
            runtime_hash: h64(2),
            quantization: QuantizationProfile::Bf16,
            input_commitment: h64(3),
            sampling_seed: [7u8; 32],
            max_tokens: 100_000,
            verification_scheme: VerificationScheme::CanonicalFullReplay,
        }
    }

    fn receipt(prefill: u32, decode: u32) -> ComputeReceipt {
        ComputeReceipt {
            version: VLT_PAYLOAD_VERSION_V1,
            output_commitment: h64(4),
            prefill_tokens: prefill,
            decode_tokens: decode,
            trace_commitment: h64(5),
        }
    }

    fn verdict(id: u8, verdict: VerificationVerdict, replay: Hash64) -> VerifierAttestation {
        VerifierAttestation {
            version: VLT_PAYLOAD_VERSION_V1,
            verifier_id: h64(id),
            bond_outpoint: outpoint(id),
            verdict,
            replay_receipt_hash: replay,
            signature: vec![0u8; 4],
        }
    }

    /// `p_j` is what ties the input bytes a commitment publishes to the spec a certificate names,
    /// so it has to separate inputs that a naive concatenation would not — otherwise an executor
    /// could publish one prompt and certify a receipt for another that happens to share a digest.
    #[test]
    fn job_input_commitment_separates_distinct_inputs() {
        assert_eq!(job_input_commitment(b"the capital of France is"), job_input_commitment(b"the capital of France is"));
        assert_ne!(job_input_commitment(b"a"), job_input_commitment(b"b"));
        // Length-prefixed: a prefix must not collide with the longer input that extends it.
        assert_ne!(job_input_commitment(b"ab"), job_input_commitment(b"abc"));
        assert_ne!(job_input_commitment(b""), job_input_commitment(b"\0"));

        // The commitment is inside `job_id`, so two jobs over different inputs are different
        // jobs — different identities, different sortition tickets, different receipts.
        let a = LlmJobSpec { input_commitment: job_input_commitment(b"prompt-a"), ..spec() };
        let b = LlmJobSpec { input_commitment: job_input_commitment(b"prompt-b"), ..spec() };
        assert_ne!(job_spec_id(&a), job_spec_id(&b));
    }

    /// The property the replay proof exists for, now on both verdicts.
    ///
    /// A verifier holding only the certificate can produce a self-consistent, signable `Confirmed`
    /// — it copies `executor_receipt_hash` — and could always produce a `Refuted` from thin air,
    /// since "my replay gave something else" is satisfied by any hash. Neither can produce a
    /// receipt whose trace commitment it has the preimage of. Once auditing pays, that gap is the
    /// only thing separating a paid auditor from a paid rubber stamp or a paid griefer.
    #[test]
    fn a_verdict_that_did_not_execute_cannot_be_self_consistent() {
        let job_id = h64(6);
        let residuals = ReplayResiduals {
            job_nullifier: h64(11),
            request_commitment: h64(12),
            model_profile_id: h64(13),
            runtime_class_id: h64(14),
            runtime_manifest_hash: h64(15),
            shape_profile_id: h64(16),
            cu_ruleset_id: h64(17),
            canonical_compute_units: 41_692,
            operation_schedule_commitment: h64(18),
            schedule_event_count: 80,
            trace_scheme_id: h64(19),
            gemm_trace_root: h64(20),
            trace_event_count: 2_466,
        };
        let executor_receipt = ComputeReceipt {
            version: VLT_PAYLOAD_VERSION_V1,
            output_commitment: h64(30),
            prefill_tokens: 100,
            decode_tokens: 20,
            trace_commitment: residual_commitment(&residuals),
        };
        let executor_hash = compute_receipt_hash_for_job(job_id, &executor_receipt);
        let verdict = |v, replay, proof: ReplayProof| ComputeVerdictPayload {
            version: VLT_PAYLOAD_VERSION_V1,
            certificate_tx_id: TransactionId::from_bytes([5u8; 64]),
            job_id,
            executor_receipt_hash: executor_hash,
            verifier_id: h64(21),
            bond_outpoint: outpoint(21),
            verdict: v,
            replay_receipt_hash: replay,
            replay_proof: proof,
            signature: vec![0u8; 4],
        };
        let honest_proof = ReplayProof { receipt: executor_receipt, residuals: residuals.clone() };

        // Reproduced the executor's receipt, and can show the preimage.
        assert!(verdict(VerificationVerdict::Confirmed, executor_hash, honest_proof.clone()).is_self_consistent());

        // The copier has everything the certificate published — the receipt hash — and that is not
        // enough: it cannot exhibit residuals folding to a trace commitment inside that hash.
        let invented =
            ReplayProof { residuals: ReplayResiduals { gemm_trace_root: h64(99), ..residuals.clone() }, ..honest_proof.clone() };
        assert!(!verdict(VerificationVerdict::Confirmed, executor_hash, invented).is_self_consistent());

        // A refutation is no longer free. It must exhibit a receipt that really hashes to the
        // divergent value it reports — i.e. it has to have run something.
        let other_residuals = ReplayResiduals { gemm_trace_root: h64(77), ..residuals };
        let other_receipt = ComputeReceipt { trace_commitment: residual_commitment(&other_residuals), ..other_receipt_base() };
        let other_hash = compute_receipt_hash_for_job(job_id, &other_receipt);
        assert_ne!(other_hash, executor_hash);
        let refute_proof = ReplayProof { receipt: other_receipt, residuals: other_residuals };
        assert!(verdict(VerificationVerdict::Refuted, other_hash, refute_proof.clone()).is_self_consistent());
        // …and a refutation reporting a hash its own proof does not produce is refused.
        assert!(!verdict(VerificationVerdict::Refuted, h64(123), refute_proof.clone()).is_self_consistent());
        // A "refutation" carrying the executor's own proof contradicts itself.
        assert!(!verdict(VerificationVerdict::Refuted, executor_hash, honest_proof.clone()).is_self_consistent());
        // …as does a "confirmation" over a hash it did not reproduce.
        assert!(!verdict(VerificationVerdict::Confirmed, other_hash, refute_proof).is_self_consistent());
    }

    fn other_receipt_base() -> ComputeReceipt {
        ComputeReceipt {
            version: VLT_PAYLOAD_VERSION_V1,
            output_commitment: h64(31),
            prefill_tokens: 100,
            decode_tokens: 21,
            trace_commitment: h64(0),
        }
    }

    /// Every residual field must be load-bearing, or two different executions could fold to one
    /// commitment and a proof of the wrong job would pass.
    #[test]
    fn every_residual_field_moves_the_commitment() {
        let base = ReplayResiduals {
            job_nullifier: h64(1),
            request_commitment: h64(2),
            model_profile_id: h64(3),
            runtime_class_id: h64(4),
            runtime_manifest_hash: h64(5),
            shape_profile_id: h64(6),
            cu_ruleset_id: h64(7),
            canonical_compute_units: 10,
            operation_schedule_commitment: h64(8),
            schedule_event_count: 11,
            trace_scheme_id: h64(9),
            gemm_trace_root: h64(10),
            trace_event_count: 12,
        };
        let root = residual_commitment(&base);
        let mutations: Vec<Box<dyn Fn(&mut ReplayResiduals)>> = vec![
            Box::new(|r: &mut ReplayResiduals| r.job_nullifier = h64(90)),
            Box::new(|r: &mut ReplayResiduals| r.request_commitment = h64(90)),
            Box::new(|r: &mut ReplayResiduals| r.model_profile_id = h64(90)),
            Box::new(|r: &mut ReplayResiduals| r.runtime_class_id = h64(90)),
            Box::new(|r: &mut ReplayResiduals| r.runtime_manifest_hash = h64(90)),
            Box::new(|r: &mut ReplayResiduals| r.shape_profile_id = h64(90)),
            Box::new(|r: &mut ReplayResiduals| r.cu_ruleset_id = h64(90)),
            Box::new(|r: &mut ReplayResiduals| r.canonical_compute_units += 1),
            Box::new(|r: &mut ReplayResiduals| r.operation_schedule_commitment = h64(90)),
            Box::new(|r: &mut ReplayResiduals| r.schedule_event_count += 1),
            Box::new(|r: &mut ReplayResiduals| r.trace_scheme_id = h64(90)),
            Box::new(|r: &mut ReplayResiduals| r.gemm_trace_root = h64(90)),
            Box::new(|r: &mut ReplayResiduals| r.trace_event_count += 1),
        ];
        assert_eq!(mutations.len(), 13, "all residual fields must be covered");
        for mutate in mutations {
            let mut m = base.clone();
            mutate(&mut m);
            assert_ne!(residual_commitment(&m), root);
        }
    }

    #[test]
    fn inert_params_are_dormant_but_coherent() {
        assert_eq!(VltParams::INERT.vlt_activation_daa_score, u64::MAX);
        assert!(!VltParams::INERT.is_active_at(u64::MAX - 1));
        assert!(VltParams::INERT.is_active_at(u64::MAX));
        // The shipped calibration must be usable the moment a network moves its fence.
        VltParams::INERT.is_coherent().expect("shipped preset must be coherent");
        // No registered model => every job mints zero, so a fence moved by accident
        // cannot silently start crediting.
        assert!(VltParams::INERT.model_cost_table.live().is_empty());
    }

    /// §6 pays the auditor, and the calibration has one job: beat the cost of auditing. A fee at
    /// or below the verdict transaction's own relay fee would leave the GPU time unpaid, which is
    /// the same as not paying at all — and once a fee exists at all, the thing it must not do is
    /// pay more for one verdict than the other, or the fraud-detection role becomes the costly one.
    /// §7(c): a challenge is a claim, and the certificate's own committee is what settles it.
    ///
    /// The two failure modes this rule has to avoid pull in opposite directions. Slashing on a
    /// mere accusation lets one bonded party burn any executor's stake for a transaction fee.
    /// Never slashing the accuser makes a baseless challenge free, and a free challenge denies
    /// credit. The resolution is that both sides need positive evidence, and silence buys neither.
    #[test]
    fn a_challenge_is_settled_by_the_certificates_own_verdicts() {
        use ComputeFraudKind::{ContradictoryVerification, FailedChallenge, ForgedReceipt, InvalidCertificate};
        let r = h64(50);
        let confirm = |id| verdict(id, VerificationVerdict::Confirmed, r);
        let refute = |id| verdict(id, VerificationVerdict::Refuted, h64(51));
        let judge = |kind, resolved, atts: &[VerifierAttestation]| adjudicate_compute_challenge(kind, resolved, r, atts, 2, 2);

        // A QUORUM of drawn verifiers refuted, each having paid for an execution to say so: the
        // accusation is corroborated.
        assert_eq!(judge(ForgedReceipt, true, &[refute(1), refute(2)]), ChallengeOutcome::Succeeded);
        // One dissenting voice is not a fraud proof — it is exactly what a griefer produces, and
        // slashing an executor on it would make griefing the profitable strategy.
        assert_eq!(judge(ForgedReceipt, true, &[confirm(1), refute(2)]), ChallengeOutcome::Undecided);
        // The certificate cleared verification: the accusation is disproved, and §7(c) takes the
        // accuser's bond.
        assert_eq!(judge(ForgedReceipt, true, &[confirm(1), confirm(2)]), ChallengeOutcome::Failed);
        // One confirmation is not the threshold. A committee that has not finished speaking is a
        // reason to wait, never a reason to slash either side.
        assert_eq!(judge(ForgedReceipt, true, &[confirm(1)]), ChallengeOutcome::Undecided);
        assert_eq!(judge(ForgedReceipt, true, &[]), ChallengeOutcome::Undecided);
        // A certificate that never resolved has no committee and credits nothing; a forgery claim
        // against it is neither proved nor disproved.
        assert_eq!(judge(ForgedReceipt, false, &[]), ChallengeOutcome::Undecided);

        // `InvalidCertificate` is about the certificate's structure, so failing to resolve IS the
        // proof — and resolving plus verifying is the disproof.
        assert_eq!(judge(InvalidCertificate, false, &[]), ChallengeOutcome::Succeeded);
        assert_eq!(judge(InvalidCertificate, true, &[confirm(1), confirm(2)]), ChallengeOutcome::Failed);
        assert_eq!(judge(InvalidCertificate, true, &[confirm(1)]), ChallengeOutcome::Undecided);

        // A contradiction proof was already decided from its own payload at acceptance, and a
        // failed-challenge report is superseded by this rule existing. Neither is re-decided.
        for superseded in [ContradictoryVerification, FailedChallenge] {
            assert_eq!(judge(superseded, true, &[confirm(1), confirm(2)]), ChallengeOutcome::Undecided);
            assert_eq!(judge(superseded, false, &[]), ChallengeOutcome::Undecided);
        }
    }

    /// A challenge that was merely filed must not deny credit — only one that stands. Otherwise
    /// the cheapest attack on the whole compute overlay is one transaction per certificate.
    #[test]
    fn only_an_adjudicated_challenge_zeroes_the_credit() {
        let cert_tx = TransactionId::from_bytes([7u8; 64]);
        let contribution = crate::dns_finality::ComputeCreditContribution {
            validator_id: h64(1),
            bond_outpoint: outpoint(1),
            epoch: 4,
            certificate_tx_id: cert_tx,
            job_id: h64(2),
            vlt: 1_000,
            accepted_daa_score: 100,
        };
        let credited = |refuted: std::collections::HashSet<TransactionId>| {
            crate::dns_finality::aggregate_compute_credits(std::slice::from_ref(&contribution), &refuted, 1_000, 300)
        };
        assert_eq!(credited(std::collections::HashSet::new()).get(&h64(1)).and_then(|e| e.get(&4)).copied(), Some(1_000));
        assert!(credited([cert_tx].into_iter().collect()).is_empty(), "a challenge that STOOD zeroes the credit");
        // A challenge against some other certificate is irrelevant to this one.
        assert_eq!(
            credited([TransactionId::from_bytes([8u8; 64])].into_iter().collect()).get(&h64(1)).and_then(|e| e.get(&4)).copied(),
            Some(1_000)
        );
    }

    #[test]
    fn the_audit_fee_is_verdict_blind_and_covers_more_than_the_transaction() {
        // `ATTESTATION_TX_FEE_FLOOR_SOMPI` in kaspa-pq-validator-core; restated rather than
        // depended on (consensus-core must not depend on the validator crate).
        const OVERLAY_TX_FEE_FLOOR_SOMPI: u64 = 250_000;
        assert!(
            VltParams::INERT.audit_fee_sompi > OVERLAY_TX_FEE_FLOOR_SOMPI,
            "an audit fee below the verdict's own transaction fee pays nothing for the replay"
        );
        // The fee is a single per-verdict constant with no verdict term anywhere near it: there is
        // deliberately no confirm/refute split to calibrate, because paying them differently is
        // what would bias an auditor.
        assert_eq!(VltParams::INERT.audit_fee_sompi, 50_000_000);
    }

    #[test]
    fn incoherent_presets_are_rejected() {
        let zero_delay = VltParams { credit_delay_epochs: 0, ..VltParams::INERT };
        assert!(zero_delay.is_coherent().is_err(), "delay 0 lets a fork weight itself");
        let growing = VltParams { credit_decay_bps: 10_001, ..VltParams::INERT };
        assert!(growing.is_coherent().is_err(), "d_tau must not grow");
        let no_conf = VltParams { min_verifier_confirmations: 0, ..VltParams::INERT };
        assert!(no_conf.is_coherent().is_err(), "0 confirmations would mint unverified VLT");
        let over = VltParams { min_verifier_confirmations: 4, verifier_committee_size: 3, ..VltParams::INERT };
        assert!(over.is_coherent().is_err(), "unsatisfiable confirmation threshold");
    }

    #[test]
    fn normalization_matches_the_paper_formula() {
        let p = params_with_model();
        // rho = 1.0, a = 1.0, b = 8.0 => 100 prefill + 10 decode = 100 + 80 = 180 RTE.
        let x = normalize_vlt(&spec(), &receipt(100, 10), &p, true).unwrap();
        assert_eq!(x, 180 * VLT_MICRO);
    }

    #[test]
    fn unverified_and_unregistered_jobs_mint_nothing() {
        let p = params_with_model();
        assert_eq!(normalize_vlt(&spec(), &receipt(100, 10), &p, false), Err(VltRejection::VerificationFailed));

        let mut unknown = spec();
        unknown.model_weights_hash = h64(0xAA);
        assert_eq!(normalize_vlt(&unknown, &receipt(100, 10), &p, true), Err(VltRejection::UnregisteredModel));

        let mut reserved = spec();
        reserved.verification_scheme = VerificationScheme::SuccinctProof;
        assert!(matches!(normalize_vlt(&reserved, &receipt(1, 1), &p, true), Err(VltRejection::IneligibleScheme(_))));
    }

    #[test]
    fn a_receipt_cannot_claim_more_work_than_its_spec_allows() {
        let p = params_with_model();
        let mut small = spec();
        small.max_tokens = 10;
        // 100 + 10 tokens produced against a 10-token spec: the cheap-spec inflation attack.
        assert!(matches!(normalize_vlt(&small, &receipt(100, 10), &p, true), Err(VltRejection::ReceiptExceedsSpecLimit { .. })));
    }

    #[test]
    fn a_spec_cannot_exceed_its_models_registered_token_ceiling() {
        let p = params_with_model();
        let mut huge = spec();
        huge.max_tokens = 100_001;
        assert!(matches!(normalize_vlt(&huge, &receipt(1, 1), &p, true), Err(VltRejection::TokenLimitExceeded { .. })));
    }

    /// Refutation still dominates — but by quorum, not by one voice. At one voice, a single drawn
    /// verifier could destroy an honest executor's credit with a hash it made up, and under the §6
    /// audit fee be paid for it; confirming and refuting have to take the same collusion.
    #[test]
    fn refutation_dominates_but_only_as_a_quorum() {
        let r = h64(9);
        let other = h64(10);
        let confirm = |id| verdict(id, VerificationVerdict::Confirmed, r);
        let refute = |id| verdict(id, VerificationVerdict::Refuted, other);
        let verify = |atts: &[VerifierAttestation]| verify_compute_certificate(r, atts, 2, 2);

        // Threshold met, and one dissenter. The job stands.
        let mixed = [confirm(1), confirm(2), refute(3)];
        assert!(verify(&mixed), "one refuter must not overturn a confirmed job");
        assert!(!refutation_quorum_reached(&mixed, 2));

        // A refutation QUORUM dominates, whatever the confirmations say.
        let refuted = [confirm(1), confirm(2), refute(3), refute(4)];
        assert!(refutation_quorum_reached(&refuted, 2));
        assert!(!verify(&refuted));

        let clean = [confirm(1), confirm(2)];
        assert!(verify(&clean));
        assert!(!verify(&clean[..1]), "below the confirmation threshold");

        // "Confirms" while reporting a different replay hash — counts for nothing.
        let liar = [confirm(1), verdict(2, VerificationVerdict::Confirmed, other)];
        assert!(!verify(&liar));

        // An empty set is neither a confirmation nor a refutation.
        assert!(!verify(&[]));
        assert!(!refutation_quorum_reached(&[], 2));
    }

    #[test]
    fn decay_is_monotonic_non_increasing_and_positive_over_the_window() {
        let p = VltParams::INERT;
        assert_eq!(decay_coefficient(1, p.credit_decay_bps), VLT_MICRO, "d_1 == 1.0");
        let mut prev = u128::MAX;
        for tau in 1..=p.credit_window_epochs {
            let d = decay_coefficient(tau, p.credit_decay_bps);
            assert!(d <= prev, "d_tau must be non-increasing");
            assert!(d > 0, "eq. (5) requires d_K > 0; d_{tau} hit zero");
            prev = d;
        }
    }

    #[test]
    fn compute_credit_from_the_current_epoch_is_never_counted() {
        let p = params_with_model();
        let mut credited = BTreeMap::new();
        // All of this validator's compute landed in epoch 100 itself.
        credited.insert(100u64, 1_000 * VLT_MICRO);
        assert_eq!(recent_compute_score(100, &credited, &p), 0, "§8.3: same-epoch compute must not weight its own epoch");
        // From epoch 101 it counts, undecayed (delay 1 re-bases d to 1.0).
        assert_eq!(recent_compute_score(101, &credited, &p), 1_000 * VLT_MICRO);
        // And it decays thereafter.
        let at_102 = recent_compute_score(102, &credited, &p);
        assert!(at_102 < 1_000 * VLT_MICRO && at_102 > 0, "expected decayed-but-positive, got {at_102}");
    }

    #[test]
    fn compute_older_than_the_window_is_dropped_entirely() {
        let p = params_with_model();
        let mut credited = BTreeMap::new();
        credited.insert(0u64, 1_000 * VLT_MICRO);
        let inside = recent_compute_score(p.credit_window_epochs as u64, &credited, &p);
        let outside = recent_compute_score(p.credit_window_epochs as u64 + 10, &credited, &p);
        assert!(inside > 0);
        assert_eq!(outside, 0, "compute beyond K epochs must contribute nothing");
    }

    #[test]
    fn bond_alone_buys_no_voting_power() {
        // The headline property of the replacement: 20M KAS bonded, zero compute.
        let bond = 20_000_000 * SOMPI_PER_KASPA;
        assert_eq!(effective_voting_weight(0, bond, VltParams::INERT.lambda_vlt_per_kas), 0);
    }

    #[test]
    fn compute_is_capped_by_slashable_collateral() {
        let lambda = VltParams::INERT.lambda_vlt_per_kas;
        // The unchanged 20M-KAS floor collateralizes lambda * 20M.
        let bond = 20_000_000 * SOMPI_PER_KASPA;
        let cap = collateral_weight_cap(bond, lambda);
        assert_eq!(cap, (lambda as u128) * 20_000_000);
        // Under the cap: compute is the binding term.
        assert_eq!(effective_voting_weight(cap - 1, bond, lambda), cap - 1);
        // Over it: the excess is discarded, however much compute was acquired.
        assert_eq!(effective_voting_weight(u128::MAX, bond, lambda), cap);
        // A bond-poor, compute-rich validator converts only what its bond backs.
        let small = 10 * SOMPI_PER_KASPA;
        assert_eq!(effective_voting_weight(u128::MAX, small, lambda), (lambda as u128) * 10);
    }

    #[test]
    fn quorum_is_strictly_above_two_thirds() {
        // No W_min here: this is the quorum arithmetic on its own.
        let q = |signed, total| meets_bft_quorum(signed, total, 0);
        // The classic off-by-one: 2 of 3 must NOT be a quorum.
        assert_eq!(bft_quorum(3), 3);
        assert!(!q(2, 3));
        assert!(q(3, 3));

        assert_eq!(bft_quorum(99), 67);
        assert!(!q(66, 99));
        assert!(q(67, 99));

        // Zero total weight never finalizes, however much is claimed.
        assert!(!q(0, 0));
        assert!(!q(u128::MAX, 0));
        // An over-count cannot manufacture a quorum.
        assert!(q(u128::MAX, 99));
    }

    /// §4 `W_min`. A quorum is a fraction, so on a network with almost no verified compute it is
    /// almost nothing: at `W(E) = 1` the quorum is 1 and its single holder finalizes the chain for
    /// everybody. The floor is the same guard `W(E) = 0` already gets, with a number behind it.
    #[test]
    fn a_quorum_over_too_little_compute_is_not_a_quorum() {
        let w_min = VltParams::INERT.min_network_compute;
        assert!(w_min > 0, "the shipped calibration must actually set a floor");

        // Unanimous, and still not finality: the network as a whole has not done enough work for
        // two thirds of it to mean anything.
        assert!(!meets_bft_quorum(w_min - 1, w_min - 1, w_min));
        assert!(!meets_bft_quorum(u128::MAX, 1, w_min), "the W = 1 case the floor exists for");
        // At the floor the ordinary arithmetic resumes, unchanged.
        assert!(meets_bft_quorum(bft_quorum(w_min), w_min, w_min));
        assert!(!meets_bft_quorum(bft_quorum(w_min) - 1, w_min, w_min));

        // The floor is on the NETWORK's weight, not the signers': a well-supplied network whose
        // signers fall short still fails for the ordinary reason.
        assert!(!meets_bft_quorum(1, w_min * 10, w_min));

        // The rule carries the floor, so a call site cannot apply the quorum without it.
        use crate::dns_finality::{EpochCreditRule, STAKE_SCORE_SCALE, epoch_credit};
        let epoch = |signed, total, min| epoch_credit(signed, total, EpochCreditRule::BftQuorum { min_network_compute: min });
        assert_eq!(epoch(3, 3, 0), STAKE_SCORE_SCALE, "no floor configured ⇒ pure quorum");
        assert_eq!(epoch(3, 3, w_min), 0, "below W_min the epoch earns nothing");
        assert_eq!(epoch(w_min, w_min, w_min), STAKE_SCORE_SCALE);
    }

    #[test]
    fn two_quorums_always_intersect() {
        // §8.1: |A| + |B| > W  =>  A and B share weight. Exhaustive over small W.
        for w in 1u128..=200 {
            let q = bft_quorum(w);
            assert!(q.saturating_mul(2) > w, "two quorums of {q} out of {w} could be disjoint");
        }
    }

    /// The one class every candidate in the simple fixtures belongs to.
    fn metal() -> Hash64 {
        derive_runtime_class_id(palw_pins::METAL_RUNTIME_CLASS)
    }

    fn same_class(ids: &[Hash64]) -> Vec<(Hash64, Hash64)> {
        ids.iter().map(|i| (*i, metal())).collect()
    }

    #[test]
    fn verifier_sortition_excludes_the_executor_and_is_deterministic() {
        let ids: Vec<Hash64> = (0u8..10).map(h64).collect();
        let candidates = same_class(&ids);
        let executor = h64(3);
        let a = select_verifiers(h64(100), executor, BlockHash::from_bytes([1u8; 64]), metal(), &candidates, 3);
        assert_eq!(a.len(), 3);
        assert!(!a.contains(&executor), "§6: an executor must never verify its own job");

        // Same inputs => same committee, regardless of candidate ordering.
        let mut shuffled = candidates.clone();
        shuffled.reverse();
        let b = select_verifiers(h64(100), executor, BlockHash::from_bytes([1u8; 64]), metal(), &shuffled, 3);
        assert_eq!(a, b, "sortition must not depend on candidate order");

        // A different beacon draws a different committee (the executor cannot pre-pick).
        let c = select_verifiers(h64(100), executor, BlockHash::from_bytes([2u8; 64]), metal(), &candidates, 3);
        assert_ne!(a, c);
    }

    #[test]
    fn sortition_handles_undersized_candidate_sets() {
        let candidates = same_class(&[h64(1), h64(2)]);
        let picked = select_verifiers(h64(100), h64(1), BlockHash::from_bytes([1u8; 64]), metal(), &candidates, 3);
        assert_eq!(picked, vec![h64(2)], "only non-executor candidates are drawable");
        assert!(select_verifiers(h64(100), h64(1), BlockHash::from_bytes([1u8; 64]), metal(), &candidates, 0).is_empty());
    }

    /// Cross-class sampling is not merely noisy — under PALW's fp-per-vendor determinism it
    /// would let an honest verifier legitimately refute an honest executor, zeroing its VLT and
    /// arming a `ForgedReceipt` slash against it. Consensus must never draw such a verifier.
    #[test]
    fn sortition_never_draws_a_verifier_from_another_determinism_class() {
        let cuda = derive_runtime_class_id("palw-fp-per-vendor/nvidia-sm89/v1");
        assert_ne!(metal(), cuda);
        let mixed: Vec<(Hash64, Hash64)> = (1u8..=8).map(|i| (h64(i), if i % 2 == 0 { metal() } else { cuda })).collect();

        let picked = select_verifiers(h64(100), h64(99), BlockHash::from_bytes([1u8; 64]), metal(), &mixed, 4);
        assert!(!picked.is_empty());
        for id in &picked {
            let class = mixed.iter().find(|(i, _)| i == id).unwrap().1;
            assert_eq!(class, metal(), "drew a verifier from the wrong determinism class");
        }

        // A class with no other members yields no committee at all — the job then simply fails
        // to reach `min_verifier_confirmations` and mints nothing, which is the safe outcome.
        let lonely = derive_runtime_class_id("palw-fp-per-vendor/rocm-gfx1100/v1");
        assert!(select_verifiers(h64(100), h64(99), BlockHash::from_bytes([1u8; 64]), lonely, &mixed, 4).is_empty());
    }

    /// The registered identities must be re-derivable from the published `runtime-pins.sh`
    /// values by anyone holding the artifacts. Changing a pin is therefore a visible consensus
    /// change (this test fails), never a silent identity drift.
    #[test]
    fn palw_registry_derives_from_the_published_pins() {
        let e = palw_qwen36_metal_entry();
        assert_eq!(
            e.model_weights_hash,
            derive_model_weights_hash(
                "1dc494614bee8a3bc00e79fe5a49da0fc1c36b3b118c4156e223e98e5a0a671b",
                23_938_321_728,
                "Qwen3.6-abliterated-35b-Claude-4.7-Q4_K_M.gguf",
                "huihui-ai/Huihui-Qwen3.6-35B-A3B-Claude-4.7-Opus-abliterated",
                "ac18882735d037f6074a7630eb68d85db8234c25",
            )
        );
        assert_eq!(
            e.runtime_hash,
            derive_runtime_hash(
                "12127defda4f41b7679cb2477a4b0d65ee6a0c8f",
                "d155a88b7c11ee74f48011760cb1a37773a694c8cab28258ee108c85e2f9e02c",
                10_015,
                palw_pins::METAL_BUILD_PROFILE,
            )
        );
        // Single registered model ⇒ it is the reference.
        assert_eq!(e.rho_micro, VLT_MICRO as u64);
        assert_eq!(e.max_tokens, PALW_QWEN36_MAX_TOKENS);

        // The three identities are independent domains: a model can never be mistaken for a
        // runtime or a class.
        assert_ne!(e.model_weights_hash, e.runtime_hash);
        assert_ne!(e.runtime_hash, e.runtime_class_id);
        assert_ne!(e.model_weights_hash, e.runtime_class_id);

        // Every pinned input is load-bearing: perturbing any one changes the identity.
        assert_ne!(
            e.model_weights_hash,
            derive_model_weights_hash(
                palw_pins::GGUF_SHA256,
                palw_pins::GGUF_SIZE + 1,
                palw_pins::GGUF_FILENAME,
                palw_pins::BASE_REPO_ID,
                palw_pins::BASE_REVISION,
            )
        );
        // A different tokenizer revision is a different model: same weights, different function
        // from prompt bytes to tokens.
        assert_ne!(
            e.model_weights_hash,
            derive_model_weights_hash(
                palw_pins::GGUF_SHA256,
                palw_pins::GGUF_SIZE,
                palw_pins::GGUF_FILENAME,
                palw_pins::BASE_REPO_ID,
                "0000000000000000000000000000000000000000",
            )
        );
        // A CUDA build of the same commit is a different runtime AND a different class.
        assert_ne!(
            e.runtime_hash,
            derive_runtime_hash(palw_pins::LLAMA_COMMIT, palw_pins::LLAMA_PATCH_SHA256, palw_pins::LLAMA_BUILD_NUMBER, "cuda-sm89")
        );

        let table = ModelCostTable::palw_qwen36_metal();
        assert_eq!(table.live().len(), 1);
        assert!(table.lookup(e.model_weights_hash, e.runtime_hash).is_some());
        assert!(table.lookup(e.runtime_hash, e.model_weights_hash).is_none(), "lookup must not be order-insensitive");
    }

    #[test]
    fn digests_are_domain_separated() {
        let net = b"misaka-test";
        let s = spec();
        let jid = job_spec_id(&s);
        let rh = compute_receipt_hash(&s, &receipt(1, 1));
        let op = outpoint(1);

        let cert = compute_certificate_message(net, 5, jid, rh, op);
        let ctx = TransactionId::from_bytes([0x5A; 64]);
        let verd = verifier_verdict_message(net, ctx, jid, rh, VerificationVerdict::Confirmed, rh, op);
        let chal =
            compute_challenge_message(net, TransactionId::from_bytes([1u8; 64]), jid, ComputeFraudKind::ForgedReceipt, rh, rh, op);
        assert_ne!(cert, verd);
        assert_ne!(cert, chal);
        assert_ne!(verd, chal);

        // A verifier's two possible verdicts are distinct messages, so a signature
        // over one is never a signature over the other.
        let refuted = verifier_verdict_message(net, ctx, jid, rh, VerificationVerdict::Refuted, rh, op);
        assert_ne!(verd, refuted);

        // The receipt hash is bound to its spec: the same output under a cheaper spec
        // is a different R_j.
        let mut cheaper = s.clone();
        cheaper.max_tokens = 1;
        assert_ne!(rh, compute_receipt_hash(&cheaper, &receipt(1, 1)));
    }

    #[test]
    fn unbonding_bound_covers_the_credit_and_challenge_windows() {
        let p = VltParams::INERT;
        let epoch_len = 100;
        let need = p.min_unbonding_period_blocks(epoch_len);
        assert_eq!(need, (96 + 1) * 100 + 300);
        // §7: the production 14-day unbonding window must cover it.
        assert!(14 * 86_400 * 10 > need);
    }
}
