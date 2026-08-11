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
//!   The delay is the floor; [`VltEpochSnapshot`] is the rest of it. Weights are read from a
//!   table pinned at a block every competing branch contains, so a fork weights its votes with
//!   the compute the network agreed on and never with compute that exists only on itself.
//!   That pin is also what makes `W(E)` a shared denominator rather than one each fork writes
//!   for itself, which is what the §8.1 quorum-intersection argument actually needs.
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

use std::collections::{BTreeMap, HashMap};
use std::fmt::{self, Display, Formatter};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering};

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

/// Keyed BLAKE2b-512 domain for [`VltEpochSnapshot::commitment_root`] — the quorum denominator's
/// identity, and what a vote must bind to so two votes counted against different `W(E)` are
/// distinguishable.
pub const VLT_SNAPSHOT_COMMITMENT_KEY: &[u8] = b"misaka-vlt-snapshot-v1";

/// Keyed BLAKE2b-512 domain for [`VltVotingSnapshot::validator_set_root`] — the set, without
/// weights, so "who may vote" and "with how much" stay separately comparable.
pub const VLT_VALIDATOR_SET_ROOT_KEY: &[u8] = b"misaka-vlt-validator-set-v1";

/// Keyed BLAKE2b-512 domain for [`VltVotingSnapshot::snapshot_root`] — the whole frozen
/// denominator, weights included.
pub const VLT_VOTING_SNAPSHOT_ROOT_KEY: &[u8] = b"misaka-vlt-voting-snapshot-v1";

/// Keyed BLAKE2b-512 domain for [`vote_snapshot_commitment`] — the single 64-byte value a vote
/// signs to bind BOTH roots.
pub const VLT_VOTE_SNAPSHOT_COMMITMENT_KEY: &[u8] = b"misaka-vlt-vote-commitment-v1";

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

/// Immutable upstream pins of the **Qwen3.5-2B palw-lite** profile — the small-model profile a
/// real-compute devnet actually runs, where five executors and their verifier committees all
/// share one machine and a 24 GB model per replay is not an experiment anyone can finish.
///
/// Same shape and same derivations as [`palw_pins`], different artifact. The runtime here is the
/// in-repo `misaka-palw-worker` (plain upstream llama.cpp driven through a pinned shim — no
/// observer patch), so `LLAMA_PATCH_SHA256`'s slot carries the literal `"unpatched"`: the derive
/// hashes whatever string is pinned, and an honest "no patch" must still be load-bearing in the
/// identity rather than an empty field two different builds could share.
pub mod qwen35_pins {
    /// SHA-256 of `Qwen3.5-2B-Q4_K_M.gguf` (the Hugging Face LFS object digest).
    pub const GGUF_SHA256: &str = "aaf42c8b7c3cab2bf3d69c355048d4a0ee9973d48f16c731c0520ee914699223";
    /// Bytes.
    pub const GGUF_SIZE: u64 = 1_280_835_840;
    pub const GGUF_FILENAME: &str = "Qwen3.5-2B-Q4_K_M.gguf";
    /// The base metadata (tokenizer, config) the GGUF was converted from. Part of the model
    /// identity because a different tokenizer turns the same weights into a different function
    /// from prompt bytes to tokens.
    pub const BASE_REPO_ID: &str = "Qwen/Qwen3.5-2B";
    pub const BASE_REVISION: &str = "15852e8c16360a2fea060d615a32b45270f8a8fc";

    /// Pinned upstream `ggml-org/llama.cpp` commit the worker links against.
    pub const LLAMA_COMMIT: &str = "030ebb558a5820b444a8f836ed5cdd46c9b4bd7a";
    /// `git rev-list --count` at that commit — llama.cpp's own build-number convention.
    pub const LLAMA_BUILD_NUMBER: u64 = 10_358;
    /// No patch is applied; the literal is hashed so "unpatched" is itself part of the identity.
    pub const LLAMA_PATCH_SHA256: &str = "unpatched";

    /// Canonical tag for the worker's build profile: Release, arm64, Metal on with the shader
    /// library embedded, `GGML_NATIVE` off (no per-host tuning), LTO off, Accelerate on, CUDA
    /// off, all ggml/llama libs linked statically into the worker.
    pub const METAL_BUILD_PROFILE: &str = "release/arm64/metal-embed/no-native/no-lto/accelerate-blas-apple/cuda-off/static/v1";

    /// The determinism class: same fp-per-vendor regime as [`super::palw_pins`] but a distinct
    /// class, because these are different kernels — a byte comparison between this runtime and
    /// the patched 35B one would be meaningless, and [`super::select_verifiers`] must never draw
    /// such a pair.
    pub const METAL_RUNTIME_CLASS: &str = "misaka-palw-lite-fp/apple-metal-arm64/v1";
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

/// Keyed BLAKE2b-512 domain for the devnet VLT fixture's identities.
#[cfg(feature = "devnet-vlt-fixture")]
pub const DEVNET_FIXTURE_KEY: &[u8] = b"misaka-vlt-devnet-fixture-v1";

/// A deterministic compute profile for a **private devnet**, so the BFT half of this design can be
/// exercised without a 24 GB model on disk.
///
/// The point is not to skip the compute path — it is to run the *whole* production path
/// (commitment → certificate → verifier quorum → challenge maturity → credit → epoch snapshot)
/// with only the executor backend replaced by something reproducible. Handing consensus a VLT
/// number directly would test none of it.
///
/// # Why it cannot escape a devnet
///
/// Three independent constraints, because a test hatch that only had one would eventually be
/// found propped open:
///
/// 1. `#[cfg(feature = "devnet-vlt-fixture")]` — absent from a release build entirely.
/// 2. **The genesis hash is in the derivation.** The profile's `(h_M, h_R)` are a function of the
///    network's own genesis, so the devnet fixture profile is a different profile on every
///    network and simply does not exist as a value on mainnet.
/// 3. **Registration is per-preset.** Only `DnsParams::with_vlt_devnet` — devnet/simnet only,
///    refused on a public network by `Args::apply_to_config` — puts it in a `ModelCostTable`. A
///    certificate naming an unregistered `(h_M, h_R)` mints zero (`VltRejection::UnregisteredModel`),
///    so even a fixture-enabled binary pointed at mainnet credits nothing.
///
/// Any one of the three would do. Together they mean the hatch has to be opened deliberately, at
/// build time, at genesis time, and in a preset — which is the standard a test-only path should be
/// held to when the alternative is it quietly going into production.
#[cfg(feature = "devnet-vlt-fixture")]
pub mod devnet_fixture {
    /// Cost per reference token. `1.0` — the fixture exists to make weights *predictable*, not to
    /// model a real model's economics.
    pub const RHO_MICRO: u64 = super::VLT_MICRO as u64;
    /// The **profile's** ceiling: the largest `max_tokens` a fixture job may declare, checked by
    /// `normalize_vlt` against every spec. Small enough that a job is instant. The job the fixture
    /// actually runs is [`JOB_MAX_TOKENS`] — far smaller — and this leaves room to change that
    /// shape without re-registering the profile.
    pub const MAX_TOKENS: u32 = 256;

    /// The one job shape the fixture executes, in prefill and decode tokens.
    ///
    /// Fixed, and fixed *here*, because the asymmetric-weight experiment needs one job to be worth
    /// the same on every node: five validators running the same job then differ only in how many
    /// they complete, so their `W_i(E)` differ only by supplied compute. Measuring the operator's
    /// `--compute-prompt` instead would make a validator's weight a function of the size of a file
    /// on its disk — a difference nobody intends and nobody would notice.
    ///
    /// The pair is not arbitrary. Against this profile's `ρ = 1.0` and the preset's `a = 1.0`,
    /// `b = 8.0`, [`super::normalize_vlt`] prices it at `1·10 + 8·5 = 50` RTE, so a quota of `N`
    /// jobs is exactly `50·N` VLT and a plan written in VLT reads off in whole jobs
    /// (400/250/150/100/100 VLT ⇒ 8/5/3/2/2 jobs). 50 is a *consequence* of `(ρ, a, b)` — nothing
    /// configures it, and `one_fixture_job_is_worth_fifty_vlt` is what keeps it true when one of
    /// the three moves.
    pub const JOB_PREFILL_TOKENS: u32 = 10;
    /// See [`JOB_PREFILL_TOKENS`]. Decode is priced at 8× prefill, so this is 40 of the 50.
    pub const JOB_DECODE_TOKENS: u32 = 5;
    /// The per-job ceiling the shape exactly fills.
    ///
    /// A fixture has no EOS to emit, so the honest reading of "it ran to completion" is that it
    /// decoded until the spec's ceiling stopped it — and then the ceiling *is* the shape. Equal
    /// rather than merely sufficient: `normalize_vlt` rejects a receipt that claims more work than
    /// its spec allowed, so a ceiling below the shape would mint zero on every job, and one above
    /// it would leave the executor free to decode more and mint more.
    pub const JOB_MAX_TOKENS: u32 = JOB_PREFILL_TOKENS + JOB_DECODE_TOKENS;

    pub const MODEL_TAG: &str = "model";
    pub const RUNTIME_TAG: &str = "runtime";
    pub const CLASS_TAG: &str = "class";
}

/// Derive one of the fixture's three identities for `genesis_hash`.
///
/// The genesis hash is mixed in so the fixture profile of one network is not the fixture profile
/// of another — the constraint that makes a fixture certificate meaningless off the devnet it was
/// built for, independently of any feature flag.
#[cfg(feature = "devnet-vlt-fixture")]
pub fn devnet_fixture_id(genesis_hash: Hash64, tag: &str) -> Hash64 {
    let mut hasher = Blake2bParams::new().hash_length(64).key(DEVNET_FIXTURE_KEY).to_state();
    hasher.update(genesis_hash.as_byte_slice());
    hasher.update(tag.as_bytes());
    let mut out = [0u8; 64];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// The devnet fixture's registrable [`ModelCostEntry`] for `genesis_hash`.
#[cfg(feature = "devnet-vlt-fixture")]
pub fn devnet_fixture_entry(genesis_hash: Hash64) -> ModelCostEntry {
    ModelCostEntry {
        model_weights_hash: devnet_fixture_id(genesis_hash, devnet_fixture::MODEL_TAG),
        runtime_hash: devnet_fixture_id(genesis_hash, devnet_fixture::RUNTIME_TAG),
        runtime_class_id: devnet_fixture_id(genesis_hash, devnet_fixture::CLASS_TAG),
        rho_micro: devnet_fixture::RHO_MICRO,
        max_tokens: devnet_fixture::MAX_TOKENS,
    }
}

/// What one canonical fixture job normalizes to, in µRTE, under `(a, b)`.
///
/// [`normalize_vlt`] is the definition; this is the same arithmetic for a job that is known in
/// advance, so a preset can size a threshold against the profile it registers instead of against a
/// number copied from a different profile. `devnet_fixture_job_vlt_matches_normalize_vlt` holds the
/// two together.
#[cfg(feature = "devnet-vlt-fixture")]
pub fn devnet_fixture_job_vlt(prefill_cost_micro: u64, decode_cost_micro: u64) -> u128 {
    let token_cost = (prefill_cost_micro as u128)
        .saturating_mul(devnet_fixture::JOB_PREFILL_TOKENS as u128)
        .saturating_add((decode_cost_micro as u128).saturating_mul(devnet_fixture::JOB_DECODE_TOKENS as u128));
    (devnet_fixture::RHO_MICRO as u128).saturating_mul(token_cost) / VLT_MICRO
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

/// Per-job token ceiling for the Qwen3.5-2B palw-lite profile. `prefill + decode` per job is
/// bounded by this (see `normalize_vlt`'s `ReceiptExceedsSpecLimit`), so it is the audit-cost
/// bound for a committee that must fully re-execute every accepted job — sized so one replay is
/// seconds on the machine class the profile names.
pub const PALW_QWEN35_2B_MAX_TOKENS: u32 = 512;

/// The registered [`ModelCostEntry`] for the pinned Qwen3.5-2B palw-lite Metal profile.
pub fn palw_qwen35_2b_metal_entry() -> ModelCostEntry {
    ModelCostEntry {
        model_weights_hash: derive_model_weights_hash(
            qwen35_pins::GGUF_SHA256,
            qwen35_pins::GGUF_SIZE,
            qwen35_pins::GGUF_FILENAME,
            qwen35_pins::BASE_REPO_ID,
            qwen35_pins::BASE_REVISION,
        ),
        runtime_hash: derive_runtime_hash(
            qwen35_pins::LLAMA_COMMIT,
            qwen35_pins::LLAMA_PATCH_SHA256,
            qwen35_pins::LLAMA_BUILD_NUMBER,
            qwen35_pins::METAL_BUILD_PROFILE,
        ),
        runtime_class_id: derive_runtime_class_id(qwen35_pins::METAL_RUNTIME_CLASS),
        // ρ = 1.0 — a devnet calibration, stated as such: on the devnet this profile is the only
        // model actually executed, so it serves as its own reference and one VLT unit is one of
        // its reference-token-equivalents. A production registration alongside the 35B profile
        // would calibrate this ρ against that reference instead (§8.4), not reuse 1.0.
        rho_micro: VLT_MICRO as u64,
        max_tokens: PALW_QWEN35_2B_MAX_TOKENS,
    }
}

/// The floor job a real-compute devnet's `min_network_compute` is sized against: 8 prefill and
/// 8 decode tokens, in µRTE under `(a, b)` — deliberately smaller than any real prompt+decode, so
/// the activation gate asks for "a committee's worth of modest real jobs", not for a 35B-sized
/// window nothing on a devnet can fill (the production 1e11 default is ~three 4096-token jobs of
/// the [`palw_pins`] profile — several hundred small-model jobs, i.e. an overlay that reports
/// inactive forever while behaving correctly).
pub fn palw_devnet_floor_job_vlt(prefill_cost_micro: u64, decode_cost_micro: u64) -> u128 {
    const FLOOR_PREFILL_TOKENS: u128 = 8;
    const FLOOR_DECODE_TOKENS: u128 = 8;
    let token_cost = (prefill_cost_micro as u128)
        .saturating_mul(FLOOR_PREFILL_TOKENS)
        .saturating_add((decode_cost_micro as u128).saturating_mul(FLOOR_DECODE_TOKENS));
    // ρ = 1.0 for the profile this floor is sized against (`palw_qwen35_2b_metal_entry`); same
    // µ-arithmetic as `devnet_fixture_job_vlt`, so the two floors are comparable numbers.
    VLT_MICRO.saturating_mul(token_cost) / VLT_MICRO
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

    /// The registry a **real-compute devnet** ships: both pinned Metal profiles, so the operator
    /// chooses the model by which worker binary they point `--compute-worker` at — the node
    /// resolves its entry from the worker's probed `runtime_hash`, and the two runtime hashes are
    /// distinct by construction. On one machine that choice is effectively
    /// [`palw_qwen35_2b_metal_entry`]; the 35B entry stays registered so pointing a real PALW
    /// worker at the same devnet is a configuration, not a fork.
    pub fn palw_metal_devnet() -> Self {
        let mut table = Self::EMPTY;
        table.len = 2;
        table.entries[0] = palw_qwen36_metal_entry();
        table.entries[1] = palw_qwen35_2b_metal_entry();
        table
    }

    /// A table holding only the devnet fixture profile for `genesis_hash`.
    #[cfg(feature = "devnet-vlt-fixture")]
    pub fn devnet_fixture(genesis_hash: Hash64) -> Self {
        let mut table = Self::EMPTY;
        table.len = 1;
        table.entries[0] = devnet_fixture_entry(genesis_hash);
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
/// [`Self::vlt_shadow_activation_daa_score`] and [`Self::vlt_activation_daa_score`].
///
/// # Two fences, because activation is two different risks
///
/// Turning the compute overlay on and handing it the vote are separate decisions with
/// separate failure modes, and one fence could not express that. Everything the overlay
/// *does* — crediting certificates, drawing committees, paying the audit fee, slashing a
/// settled challenge, filling the credit accumulator — runs at and above the **shadow**
/// fence, while finality keeps running on bonded stake exactly as before. Only at and above
/// the **weight** fence does `W_i(E) = min{C_i(E), λ·B_i(E)}` become voting power.
///
/// The gap between them is not slack, it is the soak: `C_i(E)` sums a `credit_window_epochs`
/// window, so a network that flipped both fences at once would switch its voting power to a
/// table that is still empty — `W(E) = 0`, no epoch reaches quorum, DNS finality stops on the
/// spot. [`crate::dns_finality::DnsParams::vlt_params_consistent`] requires the weight fence
/// to sit at least one full credit window above the shadow fence, so that failure is a preset
/// error caught before launch rather than a stall discovered afterwards.
#[derive(Copy, Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct VltParams {
    /// DAA score at which the compute overlay starts **running but not voting**: certificates
    /// are credited into `X_i(e)`, committees are drawn, verdicts are counted and paid the
    /// [`Self::audit_fee_sompi`], a settled challenge slashes the side that lost, and the
    /// credit accumulator fills. Finality still runs on bonded stake and the φS graded rule, so
    /// nothing here can stall it.
    ///
    /// This is a consensus change of its own — it moves coinbase value and it slashes bonds —
    /// so moving it is a hard fork. That is the point of having it: it is the hard fork whose
    /// blast radius excludes finality, taken first, so the mesh can produce and police real
    /// verified compute before anything depends on the result.
    ///
    /// `u64::MAX` (inert) on every shipped preset.
    pub vlt_shadow_activation_daa_score: u64,

    /// DAA score at which voting weight switches from bonded stake to
    /// `W_i(E) = min{C_i(E), λ·B_i(E)}`, and the epoch credit switches from the φS
    /// graded floor to the §4 `Q(E) = ⌊2W(E)/3⌋ + 1` quorum.
    ///
    /// `u64::MAX` (inert) on every shipped preset: below the fence the overlay is
    /// byte-identical to the pre-VLT stake-weighted behaviour. Activating it is a
    /// hard fork and must be coordinated across the mesh — and must not be scheduled
    /// before the active set can actually produce verified compute, since with no VLT
    /// every `W_i(E)` is 0, `W(E)` is 0, and no epoch can reach quorum. Running above
    /// [`Self::vlt_shadow_activation_daa_score`] for a full credit window first is what
    /// turns that from a hope into an observation.
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

    /// How many validators must hold credit before weighted finality may activate.
    ///
    /// `min_network_compute` bounds the total; this bounds its *concentration*. `Q(E)` is a
    /// fraction of whatever weight exists, so a single validator holding all of it clears any
    /// magnitude floor and then finalizes the chain alone — which is not a network's compute
    /// whatever the number says.
    ///
    /// Shaped like the floor it accompanies: `1 + min_verifier_confirmations`, one executor plus a
    /// committee that can confirm it, so the two thresholds cannot drift apart in meaning.
    pub min_active_validators: u8,

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
        vlt_shadow_activation_daa_score: u64::MAX,
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
        // 1 + min_verifier_confirmations: the same "one executor plus a confirming committee" shape
        // `min_network_compute` is derived from.
        min_active_validators: 4,
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

    /// Whether the compute overlay is **running** at `daa_score` — certificates credited,
    /// committees drawn, verdicts paid, challenges adjudicated, accumulator filling. Says nothing
    /// about who votes; see [`Self::weight_active_at`] for that.
    pub fn shadow_active_at(&self, daa_score: u64) -> bool {
        daa_score >= self.vlt_shadow_activation_daa_score
    }

    /// Whether verified compute is the **voting weight** at `daa_score`.
    ///
    /// Implies [`Self::shadow_active_at`] on any preset that passes
    /// [`crate::dns_finality::DnsParams::vlt_params_consistent`], which requires the weight fence
    /// to sit above the shadow fence. It is not enforced here, because a param-shape predicate
    /// that silently rewrote a misordered preset would hide exactly the error worth catching.
    pub fn weight_active_at(&self, daa_score: u64) -> bool {
        daa_score >= self.vlt_activation_daa_score
    }

    /// Commitment to the model cost table in force — `ρ(S_j)` prices voting power directly, so a
    /// [`VltVotingSnapshot`] records which pricing its weights were computed under (§5's
    /// `model_table_hash`). Hashes only the `len` live entries: two tables that differ solely in
    /// dead capacity are the same table.
    pub fn model_table_hash(&self) -> Hash64 {
        let mut hasher = Blake2bParams::new().hash_length(64).key(b"misaka-vlt-model-table-v1").to_state();
        let live = &self.model_cost_table.entries[..self.model_cost_table.len as usize];
        hasher.update(&(live.len() as u64).to_le_bytes());
        for e in live {
            hasher.update(e.model_weights_hash.as_byte_slice());
            hasher.update(e.runtime_hash.as_byte_slice());
            hasher.update(e.runtime_class_id.as_byte_slice());
            hasher.update(&e.rho_micro.to_le_bytes());
            hasher.update(&e.max_tokens.to_le_bytes());
        }
        let mut out = [0u8; 64];
        out.copy_from_slice(hasher.finalize().as_bytes());
        Hash64::from_bytes(out)
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
    /// * the shadow fence above the weight fence — the vote would switch to an overlay that is
    ///   not running yet.
    pub fn is_coherent(&self) -> Result<(), &'static str> {
        if self.vlt_shadow_activation_daa_score > self.vlt_activation_daa_score {
            return Err(
                "vlt_shadow_activation_daa_score must be <= vlt_activation_daa_score (weight cannot switch before the overlay runs)",
            );
        }
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

// ---------------------------------------------------------------------
// The pinned weight table (§4 eq. 7, §8.1).
// ---------------------------------------------------------------------

/// The verified-compute credits `X_i(e)` a quorum is measured against, together with the block
/// they were read at.
///
/// `W(E) = Σ_i W_i(E)` is the quorum **denominator**, and `Q(E) = ⌊2W(E)/3⌋ + 1` is a two-thirds
/// threshold only if everyone arguing about epoch `E` divides by the same `W(E)`. Derived
/// per-branch it is not a threshold at all: a branch that omits other validators' certificates
/// shrinks its own `W(E)`, and with it its own `Q(E)`, until the weight it *does* hold clears the
/// bar. Two branches can then each "reach quorum" for the same epoch over disjoint validator
/// sets — and the §8.1 quorum-intersection argument, which is the entire safety claim of the
/// overlay, is void. A denominator each fork writes for itself is not a denominator.
///
/// So the table is taken at a **pin**: a block that every branch being compared contains. The
/// walk that fills it starts at the pin and can therefore only see certificates in the shared
/// prefix, and every DAA-stamped test inside it is evaluated at [`Self::pin_daa_score`] or below,
/// where the branches agree by construction. Those two properties are what make two branches
/// derive a byte-identical table — see [`Self::pinned`] for the obligation that carries.
///
/// This subsumes and strengthens the eq. (5) `credit_delay_epochs` delay. The delay stops a fork
/// from weighting votes with VLT it minted in the *same* epoch; the pin stops a fork from
/// weighting votes with VLT that exists only on itself, at any epoch distance (§8.3).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VltEpochSnapshot {
    pin: BlockHash,
    pin_daa_score: u64,
    credits: HashMap<Hash64, BTreeMap<u64, u128>>,
    /// Whether every certificate in the window reached a verdict — credited, `Pending` or
    /// `Invalid` — rather than one this node could not load a dependency for.
    ///
    /// Deliberately **outside** [`Self::commitment_root`]. "I could not load it" is a fact about
    /// this node's storage, not about the chain, so two honest nodes may disagree about it while
    /// agreeing byte-for-byte on the table itself — which is exactly what the root has to mean.
    /// It is a local licence to act, not a consensus value: a node with an incomplete table refuses
    /// to cache it and refuses to activate on it, and simply waits until it can do better.
    resolution_complete: bool,
}

impl VltEpochSnapshot {
    /// The empty table: no validator holds credit, so `W(E) = 0` at every epoch and no epoch
    /// reaches quorum. This is what every consumer gets below the VLT fence — and what a network
    /// that moved its fence before the active set could produce compute would get, which is the
    /// ADR-0024 "Activation" caveat expressed as a value.
    pub fn inert() -> Self {
        // Complete, not incomplete: a dormant overlay has nothing to resolve, and "empty because
        // the fence has not opened" is a final answer. Only a node that tried and could not finish
        // is incomplete — see [`Self::unresolved`].
        Self { resolution_complete: true, ..Self::default() }
    }

    /// The empty table from a walk that could not be performed — a header that would not read, a
    /// pin whose blue score is unavailable. Empty like [`Self::inert`] and, unlike it, **not** a
    /// licence to cache or activate.
    pub fn unresolved() -> Self {
        Self { resolution_complete: false, ..Self::default() }
    }

    /// Build a table pinned at `pin`.
    ///
    /// The caller owes this constructor the invariant the type exists for: `credits` must have
    /// been derived from the chain **ending at `pin`**, with every DAA-stamped decision inside
    /// that derivation — bond status, challenge-window survival, epoch finalization — taken at
    /// `pin_daa_score` or lower. A table built from a branch tip instead satisfies the type and
    /// none of its meaning.
    pub fn pinned(pin: BlockHash, pin_daa_score: u64, credits: HashMap<Hash64, BTreeMap<u64, u128>>) -> Self {
        Self { pin, pin_daa_score, credits, resolution_complete: true }
    }

    /// As [`Self::pinned`], but recording that at least one certificate's dependency could not be
    /// loaded. See [`Self::resolution_complete`].
    pub fn pinned_incomplete(pin: BlockHash, pin_daa_score: u64, credits: HashMap<Hash64, BTreeMap<u64, u128>>) -> Self {
        Self { pin, pin_daa_score, credits, resolution_complete: false }
    }

    /// Whether this table is an answer at all. `false` ⇒ do not cache it, do not activate on it.
    pub fn resolution_complete(&self) -> bool {
        self.resolution_complete
    }

    /// The block this table was read at. Every branch weighted by it must contain this block.
    pub fn pin(&self) -> BlockHash {
        self.pin
    }

    /// The DAA score of [`Self::pin`] — the horizon below which the branches agree, and therefore
    /// the newest bond a validator may vote with (see `validator_voting_weight`).
    pub fn pin_daa_score(&self) -> u64 {
        self.pin_daa_score
    }

    /// `validator_id → (epoch → X_i(epoch))`, for the credit accumulator store and diagnostics.
    pub fn credits(&self) -> &HashMap<Hash64, BTreeMap<u64, u128>> {
        &self.credits
    }

    pub fn is_empty(&self) -> bool {
        self.credits.is_empty()
    }

    /// `C_i(E)` — the decayed recent-compute score this table gives `validator_id` at `epoch`.
    /// Absent validators score 0, which is the whole point of the replacement: an active,
    /// fully-bonded validator that supplied no verified compute has no voting power.
    pub fn recent_compute(&self, validator_id: &Hash64, epoch: u64, params: &VltParams) -> u128 {
        self.credits.get(validator_id).map(|per_epoch| recent_compute_score(epoch, per_epoch, params)).unwrap_or(0)
    }

    /// `X_i(e)` — one validator's raw credit at one epoch, undecayed.
    pub fn credited(&self, validator_id: &Hash64, epoch: u64) -> u128 {
        self.credits.get(validator_id).and_then(|per_epoch| per_epoch.get(&epoch)).copied().unwrap_or(0)
    }

    /// A commitment over the whole pinned table: the pin, its DAA score, and every
    /// `(validator, epoch, X_i)` in canonical order.
    ///
    /// Two nodes weighting the same branch must produce the same root, and two nodes weighting
    /// *different* denominators must produce different ones. That is what makes the root usable as
    /// the thing a vote binds to: without it in the signed message, a vote counted against one
    /// `W(E)` is indistinguishable from a vote counted against another, and the two-thirds
    /// threshold silently stops meaning anything. It is also what lets an operator compare five
    /// nodes with one string instead of diffing tables.
    ///
    /// Canonical by construction: validators sorted by id, epochs by number (`BTreeMap`), and the
    /// pin included so a table taken at a different block never collides with this one. The empty
    /// table has a well-defined root rather than a zero, so "no credit" and "no snapshot" stay
    /// distinguishable.
    pub fn commitment_root(&self) -> Hash64 {
        let mut hasher = Blake2bParams::new().hash_length(64).key(VLT_SNAPSHOT_COMMITMENT_KEY).to_state();
        hasher.update(self.pin.as_byte_slice());
        hasher.update(&self.pin_daa_score.to_le_bytes());
        let mut validators: Vec<&Hash64> = self.credits.keys().collect();
        validators.sort();
        hasher.update(&(validators.len() as u64).to_le_bytes());
        for v in validators {
            hasher.update(v.as_byte_slice());
            let per_epoch = &self.credits[v];
            hasher.update(&(per_epoch.len() as u64).to_le_bytes());
            for (epoch, x) in per_epoch {
                hasher.update(&epoch.to_le_bytes());
                hasher.update(&x.to_le_bytes());
            }
        }
        let mut out = [0u8; 64];
        out.copy_from_slice(hasher.finalize().as_bytes());
        Hash64::from_bytes(out)
    }
}

/// Where a network sits on the two-fence activation path, as one value rather than two booleans.
///
/// Two booleans could express states that cannot exist (`weight && !shadow`) and, worse, could not
/// express the one that matters most: **the weight fence is behind us and there is still nothing
/// to vote with.** That is not "VLT finality is on with zero weight" — under §4 an epoch whose
/// `W(E)` is below `min_network_compute` does not finalize at all, so the honest description is
/// that the fence was reached and finality is waiting for a usable snapshot. Collapsing that into
/// "active" is how a network ends up reporting healthy while finalizing nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VltActivationState {
    /// Below the shadow fence: the compute overlay is dormant.
    PreShadow,
    /// The soak. The overlay credits, audits, pays and slashes; finality is still bonded stake.
    Shadow,
    /// At or above the weight fence, and no snapshot is eligible to switch onto yet. Bootstrap
    /// (bonded-stake) weight continues; the base ledger keeps advancing, because the overlay is
    /// liveness-first.
    ///
    /// The fence is the EARLIEST point the switch may happen, not the point at which it does. A
    /// fence that switched unconditionally would move the vote onto an empty credit table, and
    /// from there `W(E) = 0` means no epoch reaches `Q(E)`, no anchor is DNS-confirmed, credit
    /// needs an anchor, an anchor needs quorum — a closed loop with no way out. `blocker` names
    /// which condition is not met, so waiting can be told from stuck.
    AwaitingEligibleSnapshot { weight_fence_daa: u64, blocker: VltActivationBlocker },
    /// An eligible snapshot was found and committed; the switch happens at the next epoch
    /// boundary. Bootstrap weight is still in force until then.
    ///
    /// Never mid-epoch: the validator set and its weights are fixed within an epoch, and compute
    /// finalized in `E` is usable from `E+1` at the earliest.
    ActivationScheduled { activation_epoch: u64, source_anchor: Hash64, snapshot_root: Hash64, total_weight: u128 },
    /// Weighted finality is live: a snapshot with enough weight to take two thirds of.
    Active { epoch: u64, snapshot_root: Hash64, total_weight: u128, quorum_weight: u128 },
    /// Weight has fallen back below the floor **after** something was already finalized. Distinct
    /// from `FenceReachedNoSnapshot` because there is a confirmed anchor to hold: §4 defers the
    /// transition rather than finalizing over too little compute, and the last finalized anchor is
    /// what the network stays on meanwhile.
    Recovery { last_finalized_anchor: Hash64, total_weight: u128, min_network_compute: u128 },
}

/// Persisted schema version for [`VltActivationRecord`].
pub const VLT_ACTIVATION_RECORD_VERSION_V1: u16 = 1;

/// The persisted arm of [`VltActivationState`] — the three positions that must survive a restart.
///
/// `PreShadow`/`Shadow` are pure functions of the DAA fences and need no record, and `Recovery` is
/// `Active` seen through a failing eligibility check, so persisting it would store a conclusion the
/// next recompute re-derives anyway. What cannot be re-derived from the chain alone is exactly
/// these three: whether the switch has been reserved, for which epoch, on which snapshot — and
/// whether it already happened, which is the fact that forbids ever returning to bootstrap weight.
/// Before this was persisted, a restart forgot all three: a node that had activated came back
/// deriving its position from "was anything ever DNS-confirmed", which bootstrap finality also
/// makes true, and a committed reservation simply vanished.
#[derive(Copy, Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub enum PersistedVltActivationState {
    /// Past the weight fence with no eligible snapshot committed. Also the cancellation target for
    /// a reservation whose snapshot lost eligibility before its boundary.
    AwaitingEligibleSnapshot,
    /// An eligible snapshot was found in `scheduled_at_epoch`; the switch is reserved for
    /// `activation_epoch`. Bootstrap weight remains in force until the boundary re-evaluation.
    ActivationScheduled,
    /// Weighted finality took effect at `activation_epoch`.
    Active,
    /// Weighted finality took effect and the snapshot then stopped being eligible (§10.2): the
    /// last finalized anchor is held, PoW keeps advancing the base ledger, and no NEW anchor is
    /// finalized until an eligible snapshot returns.
    ///
    /// Persisted, and that is the point. "Which side of activation am I on" cannot be re-derived
    /// once weight has collapsed — an unpersisted recovery would look exactly like a network that
    /// never activated, which is the one state that may still return to bonded-stake bootstrap.
    /// From here the only exits are [`Self::ActivationScheduled`] (weight returned, re-prove it at
    /// a boundary) and back to itself; never [`Self::AwaitingEligibleSnapshot`].
    Recovery,
}

impl PersistedVltActivationState {
    /// Stable snake_case label for logs. Treat as an API: operators grep for these.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AwaitingEligibleSnapshot => "awaiting_eligible_snapshot",
            Self::ActivationScheduled => "activation_scheduled",
            Self::Active => "active",
            Self::Recovery => "recovery",
        }
    }

    /// Whether weighted finality has ever taken effect on this consensus. `Recovery` counts:
    /// it is post-activation, so the vote is VLT-denominated and bootstrap is behind us for
    /// good. This is the predicate that forbids a return to bonded stake.
    pub fn has_activated(&self) -> bool {
        matches!(self, Self::Active | Self::Recovery)
    }
}

/// The consensus-side record of where this network is on the §6 activation state machine.
///
/// One singleton row, stepped at most once per blue-score epoch by [`tick_vlt_activation`] and
/// written in the same batch as the `DnsState` it travels with. It exists to make two facts
/// durable rather than in-memory:
///
/// * **A reservation is a commitment, not a mood.** Eligibility found in epoch `E` reserves the
///   switch for `E+1` (`scheduled_at_epoch` / `activation_epoch`), is re-evaluated at that
///   boundary, and is cancelled — explicitly, back to `AwaitingEligibleSnapshot` — if the snapshot
///   stopped being eligible in between. A node that restarts mid-reservation resumes the same
///   reservation instead of deriving a fresh opinion.
/// * **Activation is one-way.** Once `state == Active`, weight trouble is reported as
///   [`VltActivationState::Recovery`] (hold the last finalized anchor), never as a return to
///   bonded-stake bootstrap: two authorities on one chain would let a fork pick the one it
///   prefers.
///
/// The snapshot fields are stamped at the transition they describe (`ActivationScheduled`: the
/// snapshot that proved eligible; `Active`: the snapshot the boundary re-evaluation approved) and
/// then hold still — live weights belong to `DnsState`/[`VltMetrics`], not here.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct VltActivationRecord {
    pub version: u16,
    pub state: PersistedVltActivationState,
    /// The DNS-confirmed anchor the eligibility check was grounded on — the "fact about the
    /// shared prefix" that made the snapshot usable.
    pub source_anchor: Hash64,
    /// Newest scored epoch of the snapshot stamped into this record.
    pub snapshot_epoch: u64,
    pub snapshot_root: Hash64,
    /// Epoch in which the eligible snapshot was found and the reservation committed.
    pub scheduled_at_epoch: u64,
    /// Epoch the switch is reserved for (`scheduled_at_epoch + 1`), and — once `state` is
    /// `Active` — the epoch weighted finality is in force from.
    pub activation_epoch: u64,
    /// Post-cap `W(E)` of the stamped snapshot.
    pub total_weight: u128,
    /// `Q(E) = ⌊2·total_weight/3⌋ + 1` of the stamped snapshot.
    pub quorum_weight: u128,
}

impl VltActivationRecord {
    /// The fence-reached, nothing-committed record: every field that describes a snapshot is
    /// deliberately zero, because there is none.
    pub fn awaiting() -> Self {
        Self {
            version: VLT_ACTIVATION_RECORD_VERSION_V1,
            state: PersistedVltActivationState::AwaitingEligibleSnapshot,
            source_anchor: Hash64::default(),
            snapshot_epoch: 0,
            snapshot_root: Hash64::default(),
            scheduled_at_epoch: 0,
            activation_epoch: 0,
            total_weight: 0,
            quorum_weight: 0,
        }
    }

    /// Whether weighted finality has taken effect — the fact that forbids bootstrap fallback.
    /// True in `Recovery` too: recovery is a pause in finalizing, not an un-activation.
    pub fn is_active(&self) -> bool {
        self.state.has_activated()
    }
}

/// Persisted schema version for [`VltVotingSnapshot`].
pub const VLT_VOTING_SNAPSHOT_VERSION_V1: u16 = 1;

/// One validator's row in a frozen [`VltVotingSnapshot`] (§5): the identity that may vote, the
/// collateral that makes the vote slashable, and the three numbers `W_i(E)` decomposes into.
///
/// `raw_recent_compute` and `bond_cap` are both carried even though only their `min` votes,
/// because an operator diagnosing a weight has to see WHICH bound is binding — "your compute
/// decayed" and "your bond is too small for your compute" are different problems with the same
/// `effective_weight`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct VltValidatorWeight {
    pub validator_id: Hash64,
    /// The bond's 2592-byte ML-DSA-87 validator public key — carried whole so a snapshot (and
    /// later a checkpoint package) is sufficient to verify this validator's votes without the
    /// bond registry.
    pub consensus_key: Vec<u8>,
    pub bond_outpoint: TransactionOutpoint,
    /// `C_i(E)` — decayed recent verified compute at [`VltVotingSnapshot::snapshot_epoch`].
    pub raw_recent_compute: u128,
    /// `λ·B_i` — the collateral cap.
    pub bond_cap: u128,
    /// `W_i(E) = min{C_i(E), λ·B_i(E)}` — what actually votes.
    pub effective_weight: u128,
}

/// The §5 frozen voting snapshot: the complete denominator for one epoch, as one persistable,
/// root-committed value.
///
/// The BFT safety argument needs every voter dividing by the same `W(E)`, which makes the
/// denominator itself consensus-relevant state rather than something each recompute re-derives
/// and forgets. This struct is that state. It is derived once per wall epoch, pinned at a
/// canonical lag-buried anchor (`source_finalized_anchor`) so every node — and every branch that
/// contains that anchor — derives it byte-identically, and then committed by two roots:
///
/// * [`Self::validator_set_root`] — who may vote (id, key, bond), weights excluded.
/// * [`Self::snapshot_root`] — the whole thing, weights and provenance included.
///
/// A vote then signs [`vote_snapshot_commitment`] over both (§5.1): a signature under one
/// denominator can no longer be counted against another, which closes the "same vote, different
/// `W(E)`" replay the paper calls out. `resolution_complete` stays OUTSIDE both roots for the
/// same reason it is outside [`VltEpochSnapshot::commitment_root`]: it is a fact about this
/// node's storage, not about the chain.
///
/// `validators` is sorted ascending by `validator_id` (ties by bond outpoint) — a consensus
/// rule, not a convenience: the roots hash the vector in order.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct VltVotingSnapshot {
    pub version: u16,
    /// The canonical lag-buried anchor the whole derivation is pinned at.
    pub source_finalized_anchor: Hash64,
    pub source_anchor_daa: u64,
    /// The epoch the weights are evaluated at — the newest ready epoch at the freeze.
    pub snapshot_epoch: u64,
    /// The wall epoch this snapshot is the denominator FOR. Weights frozen here are usable from
    /// this epoch and not before — the §4 delay, restated as data.
    pub activation_epoch: u64,
    /// Commitment to the model cost table in force — `ρ` moves voting power, so the snapshot
    /// records which pricing it was computed under.
    pub model_table_hash: Hash64,
    /// Commitment to the capability declarations live at the pin (the committee candidate pool).
    pub capability_set_root: Hash64,
    pub validator_set_root: Hash64,
    /// [`VltEpochSnapshot::commitment_root`] of the pinned credit table the weights came from.
    pub credit_table_root: Hash64,
    pub snapshot_root: Hash64,
    pub validators: Vec<VltValidatorWeight>,
    pub total_weight: u128,
    pub quorum_weight: u128,
    /// Local licence, not consensus data (outside every root): `false` means a dependency could
    /// not be loaded and this snapshot must not be frozen or activated on.
    pub resolution_complete: bool,
}

impl VltVotingSnapshot {
    /// The root over WHO may vote: count then `validator_id || key || bond outpoint` per row, in
    /// the vector's (sorted) order. Weights deliberately excluded — see the struct doc.
    pub fn compute_validator_set_root(validators: &[VltValidatorWeight]) -> Hash64 {
        let mut hasher = Blake2bParams::new().hash_length(64).key(VLT_VALIDATOR_SET_ROOT_KEY).to_state();
        hasher.update(&(validators.len() as u64).to_le_bytes());
        for v in validators {
            hasher.update(v.validator_id.as_byte_slice());
            hasher.update(&(v.consensus_key.len() as u64).to_le_bytes());
            hasher.update(&v.consensus_key);
            hasher.update(v.bond_outpoint.transaction_id.as_byte_slice());
            hasher.update(&v.bond_outpoint.index.to_le_bytes());
        }
        let mut out = [0u8; 64];
        out.copy_from_slice(hasher.finalize().as_bytes());
        Hash64::from_bytes(out)
    }

    /// The root over the WHOLE snapshot: provenance, both sub-roots, every weight row in order,
    /// and the totals. Everything consensus-meaningful except `resolution_complete`.
    pub fn compute_snapshot_root(&self) -> Hash64 {
        let mut hasher = Blake2bParams::new().hash_length(64).key(VLT_VOTING_SNAPSHOT_ROOT_KEY).to_state();
        hasher.update(&self.version.to_le_bytes());
        hasher.update(self.source_finalized_anchor.as_byte_slice());
        hasher.update(&self.source_anchor_daa.to_le_bytes());
        hasher.update(&self.snapshot_epoch.to_le_bytes());
        hasher.update(&self.activation_epoch.to_le_bytes());
        hasher.update(self.model_table_hash.as_byte_slice());
        hasher.update(self.capability_set_root.as_byte_slice());
        hasher.update(self.validator_set_root.as_byte_slice());
        hasher.update(self.credit_table_root.as_byte_slice());
        hasher.update(&(self.validators.len() as u64).to_le_bytes());
        for v in &self.validators {
            hasher.update(v.validator_id.as_byte_slice());
            hasher.update(&v.raw_recent_compute.to_le_bytes());
            hasher.update(&v.bond_cap.to_le_bytes());
            hasher.update(&v.effective_weight.to_le_bytes());
        }
        hasher.update(&self.total_weight.to_le_bytes());
        hasher.update(&self.quorum_weight.to_le_bytes());
        let mut out = [0u8; 64];
        out.copy_from_slice(hasher.finalize().as_bytes());
        Hash64::from_bytes(out)
    }

    /// Sort the rows into consensus order, recompute both roots and the totals, and return the
    /// sealed snapshot. The ONLY way the roots should ever be produced — a snapshot whose fields
    /// were hand-set can claim any root it likes, which is why consumers compare against
    /// [`Self::compute_snapshot_root`] rather than trusting the field.
    pub fn seal(mut self) -> Self {
        self.validators.sort_by(|a, b| {
            (a.validator_id, a.bond_outpoint.transaction_id, a.bond_outpoint.index).cmp(&(
                b.validator_id,
                b.bond_outpoint.transaction_id,
                b.bond_outpoint.index,
            ))
        });
        self.total_weight = self.validators.iter().fold(0u128, |acc, v| acc.saturating_add(v.effective_weight));
        self.quorum_weight = bft_quorum(self.total_weight);
        self.validator_set_root = Self::compute_validator_set_root(&self.validators);
        self.snapshot_root = self.compute_snapshot_root();
        self
    }

    /// The 64-byte value a vote signs for this snapshot — both roots, bound.
    pub fn vote_commitment(&self) -> Hash64 {
        vote_snapshot_commitment(self.snapshot_root, self.validator_set_root)
    }
}

/// §5.1: the single digest-sized commitment a Prevote/Precommit signs to bind BOTH the frozen
/// denominator (`snapshot_root`) and the eligible voter set (`validator_set_root`).
///
/// Without this in the signed message, a vote aggregated under one denominator is
/// indistinguishable from the same vote aggregated under another, and `Q(E)` silently stops
/// meaning two thirds of anything. One value rather than two because the attestation wire format
/// has exactly one commitment slot (`validator_set_commitment`) — and one keyed hash of both
/// roots loses nothing: forging either root still changes the commitment.
pub fn vote_snapshot_commitment(snapshot_root: Hash64, validator_set_root: Hash64) -> Hash64 {
    let mut hasher = Blake2bParams::new().hash_length(64).key(VLT_VOTE_SNAPSHOT_COMMITMENT_KEY).to_state();
    hasher.update(snapshot_root.as_byte_slice());
    hasher.update(validator_set_root.as_byte_slice());
    let mut out = [0u8; 64];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// Step the persisted activation record one recompute forward and report the resulting
/// [`VltActivationState`] — one function, because the report is only honest about the position
/// the record is actually in *after* this recompute's eligibility verdict has been applied.
///
/// The state machine (§6), with every transition epoch-granular:
///
/// ```text
///  AwaitingEligibleSnapshot ──eligible at E──▶ ActivationScheduled(E+1)
///  ActivationScheduled ──ineligible before/at the boundary──▶ AwaitingEligibleSnapshot
///  ActivationScheduled ──boundary re-evaluation passes──▶ Active
///  Active ──▶ Active                                    (terminal; weight loss reports Recovery)
/// ```
///
/// * `current_epoch` is the sink's blue-score epoch — the coordinate the recompute throttle
///   already runs on, so the step advances at most once per epoch.
/// * The reservation holds its schedule-time snapshot fields; it is **not** re-stamped while it
///   waits (re-stamping every recompute would make "the snapshot that proved eligible"
///   unrecoverable). The boundary re-evaluation then judges the **live** snapshot, and `Active`
///   stamps that one — the schedule-time root is a full epoch stale by construction.
/// * `current_epoch >= activation_epoch`, not `==`: a node that crossed several epochs in one
///   commit (IBD, restart) still activates, at the reserved epoch, provided the re-evaluation
///   passes.
/// * Below the fences the record passes through untouched: fences are a fact about the DAA
///   score, not about the machine, and a devnet lowering its fences back must not lose history.
///
/// Returns `(record, state)`; `record` is `None` only below the weight fence, where nothing may
/// be persisted. The caller persists the record iff it differs from `prev` — the step is
/// idempotent within an epoch, so re-running it (restart, reorg inside one epoch) rewrites
/// nothing.
/// The §10.2 report: finality is paused, and this is the anchor the network stays on meanwhile.
/// One helper because every arm that reaches it must name the same anchor and the same floor —
/// a recovery that reported a different anchor per call site would be a recovery to nowhere.
fn recovery_state(last_finalized_anchor: Hash64, total_weight: u128, blocker: VltActivationBlocker) -> VltActivationState {
    VltActivationState::Recovery {
        last_finalized_anchor,
        total_weight,
        min_network_compute: match blocker {
            VltActivationBlocker::BelowMinNetworkCompute { min_network_compute, .. } => min_network_compute,
            _ => 0,
        },
    }
}

/// How far back the caller's canonical-reservation scan looks for the start of the current
/// contiguous run of magnitude-eligible frozen snapshots. A constant, because the scan bound is
/// part of what makes the derived epoch canonical: two nodes with different bounds could name
/// different run starts for one chain.
pub const CANONICAL_RESERVATION_SCAN_CAP: u64 = 64;

/// The **magnitude** half of [`vlt_activation_eligibility`], over a persisted
/// [`VltVotingSnapshot`] row: weight at/above the floor, spread over enough validators. The
/// local-availability half (resolution completeness, a finalized source anchor) is deliberately
/// absent — those gate when THIS node may act, and canonical history must not depend on them.
pub fn snapshot_row_magnitude_eligible(total_weight: u128, credited_validators: usize, params: &VltParams) -> bool {
    total_weight > 0 && total_weight >= params.min_network_compute && credited_validators >= params.min_active_validators as usize
}

#[allow(clippy::too_many_arguments)]
pub fn tick_vlt_activation(
    shadow_active: bool,
    weight_active: bool,
    prev: Option<&VltActivationRecord>,
    current_epoch: u64,
    newest: Option<(u64, u128)>,
    snapshot_root: Hash64,
    last_finalized_anchor: Hash64,
    eligibility: Result<(), VltActivationBlocker>,
    weight_fence_daa: u64,
    // The chain-canonical epoch a reservation must be stamped with: the start of the current
    // contiguous run of magnitude-eligible frozen snapshots (the caller derives it from the
    // write-once per-epoch rows). `current_epoch` is when this node OBSERVED eligibility — a
    // recompute-cadence fact that trails the chain by a sync-path-dependent amount, so stamping
    // it into the record made `activation_epoch` differ between a node that lived the epoch and
    // one that replayed or imported it: the §12 identity tuple then disagreed on its one
    // machine-derived field while every root matched. Found live: five present nodes said 37, a
    // pruning-imported sixth said 36 — and 36 is the canonical answer.
    canonical_scheduled_epoch: u64,
) -> (Option<VltActivationRecord>, VltActivationState) {
    // Never schedule the future: the canonical epoch comes from persisted rows so it cannot
    // exceed the wall epoch, but the machine's invariants should not rest on a caller's walk.
    let canonical_scheduled_epoch = canonical_scheduled_epoch.min(current_epoch);
    use PersistedVltActivationState as P;
    if !shadow_active {
        return (prev.cloned(), VltActivationState::PreShadow);
    }
    if !weight_active {
        return (prev.cloned(), VltActivationState::Shadow);
    }
    let (epoch, total_weight) = newest.unwrap_or((0, 0));
    match prev {
        // Active: finalizing, until the snapshot stops being eligible. §4/§10.2's answer to weight
        // loss is to hold the last finalized anchor and stop finalizing — never to re-denominate
        // onto bonded stake, which would put two authorities on one chain and let a fork pick the
        // one it prefers. The record moves to `Recovery` so that fact survives a restart: an
        // unpersisted recovery is indistinguishable from a network that never activated, and that
        // network IS allowed back onto bootstrap weight.
        Some(r) if r.state == P::Active => match eligibility {
            Ok(()) => (
                Some(r.clone()),
                VltActivationState::Active { epoch, snapshot_root, total_weight, quorum_weight: bft_quorum(total_weight) },
            ),
            Err(blocker) => (
                Some(VltActivationRecord { state: P::Recovery, ..r.clone() }),
                recovery_state(last_finalized_anchor, total_weight, blocker),
            ),
        },
        // §10.2, the return path: weight is back, so RESERVE the switch again rather than
        // resuming mid-epoch. Coming back from recovery is exactly as dangerous as activating the
        // first time — a snapshot that just became eligible may stop being so before its boundary
        // — so it goes through the same reservation and the same boundary re-evaluation. The
        // record keeps its ORIGINAL `activation_epoch`: that is when this consensus first
        // finalized under VLT weight, and re-entry does not rewrite history. The new reservation
        // rides `scheduled_at_epoch`.
        Some(r) if r.state == P::Recovery => match eligibility {
            Err(blocker) => (Some(r.clone()), recovery_state(last_finalized_anchor, total_weight, blocker)),
            Ok(()) => {
                // The re-reservation is stamped with the start of the CURRENT eligibility run —
                // the epoch weight came back, not the epoch this node noticed. Same canonical
                // coordinate as first activation, same reason: a replayer must derive the same
                // record.
                let record = VltActivationRecord {
                    state: P::ActivationScheduled,
                    source_anchor: last_finalized_anchor,
                    snapshot_epoch: epoch,
                    snapshot_root,
                    scheduled_at_epoch: canonical_scheduled_epoch,
                    total_weight,
                    quorum_weight: bft_quorum(total_weight),
                    ..r.clone()
                };
                let state = VltActivationState::ActivationScheduled {
                    activation_epoch: canonical_scheduled_epoch + 1,
                    source_anchor: last_finalized_anchor,
                    snapshot_root,
                    total_weight,
                };
                (Some(record), state)
            }
        },
        // A committed reservation holds until its boundary — unless its snapshot loses
        // eligibility first (a successful challenge can do that), in which case it cancels rather
        // than activating on a proof that no longer holds. Where it cancels TO depends on whether
        // this consensus has ever finalized under VLT weight: a first-time reservation falls back
        // to Awaiting (bootstrap weight is still in force), a re-entry falls back to Recovery
        // (bootstrap is behind us for good — §10.2).
        Some(r) if r.state == P::ActivationScheduled => match eligibility {
            Err(blocker) if r.activation_epoch != 0 && r.scheduled_at_epoch >= r.activation_epoch => (
                Some(VltActivationRecord { state: P::Recovery, ..r.clone() }),
                recovery_state(last_finalized_anchor, total_weight, blocker),
            ),
            Err(blocker) => {
                (Some(VltActivationRecord::awaiting()), VltActivationState::AwaitingEligibleSnapshot { weight_fence_daa, blocker })
            }
            Ok(()) if current_epoch > r.scheduled_at_epoch && current_epoch >= r.activation_epoch => {
                // The boundary re-evaluation just passed: weighted finality takes effect, at the
                // epoch that was reserved, stamped with the snapshot the re-evaluation approved.
                let record = VltActivationRecord {
                    version: VLT_ACTIVATION_RECORD_VERSION_V1,
                    state: P::Active,
                    source_anchor: last_finalized_anchor,
                    snapshot_epoch: epoch,
                    snapshot_root,
                    scheduled_at_epoch: r.scheduled_at_epoch,
                    activation_epoch: r.activation_epoch,
                    total_weight,
                    quorum_weight: bft_quorum(total_weight),
                };
                let state = VltActivationState::Active { epoch, snapshot_root, total_weight, quorum_weight: bft_quorum(total_weight) };
                (Some(record), state)
            }
            Ok(()) => (
                Some(r.clone()),
                VltActivationState::ActivationScheduled {
                    activation_epoch: r.activation_epoch,
                    source_anchor: r.source_anchor,
                    snapshot_root: r.snapshot_root,
                    total_weight: r.total_weight,
                },
            ),
        },
        // No record yet, or Awaiting: an eligible snapshot reserves the epoch AFTER the run of
        // eligible snapshots began — never the run-start itself. The validator set and its
        // weights are fixed within an epoch, and compute finalized in `E` is usable from `E+1`
        // at the earliest. The stamp is the CANONICAL epoch, not this node's observation epoch:
        // a node whose recompute noticed the run late still records the same reservation a
        // replaying or importing node derives, and the `>=` activation gates below mean lateness
        // costs it nothing but the delay it already had.
        _ => match eligibility {
            Err(blocker) => {
                (Some(VltActivationRecord::awaiting()), VltActivationState::AwaitingEligibleSnapshot { weight_fence_daa, blocker })
            }
            Ok(()) => {
                let record = VltActivationRecord {
                    version: VLT_ACTIVATION_RECORD_VERSION_V1,
                    state: P::ActivationScheduled,
                    source_anchor: last_finalized_anchor,
                    snapshot_epoch: epoch,
                    snapshot_root,
                    scheduled_at_epoch: canonical_scheduled_epoch,
                    activation_epoch: canonical_scheduled_epoch + 1,
                    total_weight,
                    quorum_weight: bft_quorum(total_weight),
                };
                let state = VltActivationState::ActivationScheduled {
                    activation_epoch: canonical_scheduled_epoch + 1,
                    source_anchor: last_finalized_anchor,
                    snapshot_root,
                    total_weight,
                };
                (Some(record), state)
            }
        },
    }
}

/// The live gauge set a scraper reads, updated once per recompute.
///
/// Atomics rather than a lock because this is written on the virtual-processor's commit path and
/// read by RPC threads: a metrics read must never be able to stall consensus, and a torn read of
/// a gauge is a worse outcome than a stale one only if the fields have to agree — they do not,
/// each is independently meaningful.
///
/// `u128` weights are stored as two `u64` halves for the same reason (no atomic u128 on the
/// targets we build for); a reader that catches a half-update sees a wrong weight for one scrape,
/// never a wrong *state*, which is the field alerts key on.
#[derive(Debug, Default)]
pub struct VltMetrics {
    shadow_active: AtomicBool,
    weight_fence_reached: AtomicBool,
    finality_active: AtomicBool,
    total_weight_lo: AtomicU64,
    total_weight_hi: AtomicU64,
    quorum_weight_lo: AtomicU64,
    quorum_weight_hi: AtomicU64,
    snapshot_epoch: AtomicU64,
    /// The snapshot root, as 8 `u64` limbs. Zero while there is no active snapshot.
    snapshot_root: [AtomicU64; 8],
    /// The sink DAA the gauges were last written at, so a scraper can tell a stalled recompute
    /// from a steady state.
    sink_daa_score: AtomicU64,
    /// Last recompute's credit walk: candidates seen, candidates credited, and the per-reason
    /// skips indexed by [`VltCreditSkipReason::index`].
    credit_candidates: AtomicU64,
    credit_accepted: AtomicU64,
    credit_skipped: [AtomicU64; 17],
}

impl VltMetrics {
    pub fn record(&self, state: &VltActivationState, sink_daa_score: u64) {
        let g = state.gauges();
        self.shadow_active.store(g.shadow_active, AtomicOrdering::Relaxed);
        self.weight_fence_reached.store(g.weight_fence_reached, AtomicOrdering::Relaxed);
        self.finality_active.store(g.finality_active, AtomicOrdering::Relaxed);
        self.total_weight_lo.store(g.total_weight as u64, AtomicOrdering::Relaxed);
        self.total_weight_hi.store((g.total_weight >> 64) as u64, AtomicOrdering::Relaxed);
        self.quorum_weight_lo.store(g.quorum_weight as u64, AtomicOrdering::Relaxed);
        self.quorum_weight_hi.store((g.quorum_weight >> 64) as u64, AtomicOrdering::Relaxed);
        self.snapshot_epoch.store(g.snapshot_epoch, AtomicOrdering::Relaxed);
        let root = g.snapshot_root.as_bytes();
        for (i, slot) in self.snapshot_root.iter().enumerate() {
            let mut limb = [0u8; 8];
            limb.copy_from_slice(&root[i * 8..i * 8 + 8]);
            slot.store(u64::from_le_bytes(limb), AtomicOrdering::Relaxed);
        }
        self.sink_daa_score.store(sink_daa_score, AtomicOrdering::Relaxed);
    }

    /// Publish the latest credit-walk tally. Gauges, not counters: they are overwritten each
    /// recompute rather than accumulated, so a scraper reads "why is credit not forming right now"
    /// instead of a total that never comes down after one bad epoch.
    pub fn record_credit(&self, tally: &VltCreditTally) {
        self.credit_candidates.store(tally.candidates, AtomicOrdering::Relaxed);
        self.credit_accepted.store(tally.accepted, AtomicOrdering::Relaxed);
        for (slot, n) in self.credit_skipped.iter().zip(tally.skipped.iter()) {
            slot.store(*n, AtomicOrdering::Relaxed);
        }
    }

    pub fn read_credit(&self) -> VltCreditTally {
        let mut skipped = [0u64; 17];
        for (out, slot) in skipped.iter_mut().zip(self.credit_skipped.iter()) {
            *out = slot.load(AtomicOrdering::Relaxed);
        }
        VltCreditTally {
            candidates: self.credit_candidates.load(AtomicOrdering::Relaxed),
            accepted: self.credit_accepted.load(AtomicOrdering::Relaxed),
            skipped,
        }
    }

    pub fn read(&self) -> (VltGauges, u64) {
        let mut root = [0u8; 64];
        for (i, slot) in self.snapshot_root.iter().enumerate() {
            root[i * 8..i * 8 + 8].copy_from_slice(&slot.load(AtomicOrdering::Relaxed).to_le_bytes());
        }
        let wide = |lo: &AtomicU64, hi: &AtomicU64| {
            ((hi.load(AtomicOrdering::Relaxed) as u128) << 64) | lo.load(AtomicOrdering::Relaxed) as u128
        };
        (
            VltGauges {
                shadow_active: self.shadow_active.load(AtomicOrdering::Relaxed),
                weight_fence_reached: self.weight_fence_reached.load(AtomicOrdering::Relaxed),
                finality_active: self.finality_active.load(AtomicOrdering::Relaxed),
                total_weight: wide(&self.total_weight_lo, &self.total_weight_hi),
                quorum_weight: wide(&self.quorum_weight_lo, &self.quorum_weight_hi),
                snapshot_epoch: self.snapshot_epoch.load(AtomicOrdering::Relaxed),
                snapshot_root: Hash64::from_bytes(root),
            },
            self.sink_daa_score.load(AtomicOrdering::Relaxed),
        )
    }
}

/// Why a certificate in the credit window credited nothing.
///
/// The credit walk drops a certificate in a dozen places, and every one of them used to be a bare
/// `continue`. The only outward sign was `0 validator(s) with credit` — which is also exactly what
/// a network running no compute at all looks like, so "nobody is executing" and "everybody is, and
/// none of it counts" were indistinguishable from outside the node.
///
/// The variants are the *code paths*, not a tidy taxonomy: a reason that lumps two branches
/// together sends the reader to the wrong half of the walk, which is the failure this enum exists
/// to end. Several are transient by design — [`Self::BeaconNotReady`] and
/// [`Self::ChallengeNotMature`] mean "not yet", not "never".
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum VltCreditSkipReason {
    /// The certificate's own epoch has no canonical anchor **yet**. An epoch anchor is only ready
    /// once buried by `attestation_lag_blue_score` past the epoch's end, so every certificate is
    /// briefly in this state immediately after it lands. Transient by construction.
    EpochAnchorNotReady,
    /// The certificate's epoch is below the credit window: its anchor is not merely unready, it is
    /// out of reach from this pin and always will be. Permanent, and rescuing it later by
    /// assigning some other anchor would rewrite consensus history.
    EpochAnchorOutsideWindow,
    /// No bond record for the outpoint the certificate names.
    BondMissing,
    /// The bond exists but is not this executor's, or was not Active at the epoch anchor.
    BondInactive,
    /// The executor's ML-DSA-87 signature over its own receipt does not verify.
    ExecutorSignatureInvalid,
    /// The phase-1 commitment was not loaded — the walk did not reach it, an index lookup came
    /// back empty, or a header could not be read. **Incomplete, not invalid**: it says nothing
    /// about whether the commitment exists, only that this node has not got it yet, so a snapshot
    /// carrying one is an unknown answer rather than a smaller one.
    ///
    /// This distinction is the whole of the 2026-08-09 failure. The walk stopped at the oldest
    /// uncached epoch while commitments legitimately sit up to `max_commitment_age_blocks` below
    /// their certificates, every certificate resolved to "missing", and the resulting empty
    /// accumulator rows were then written write-once — sealing a local loading limit into consensus
    /// history as a permanent zero.
    CommitmentNotLoaded,
    /// The commitment is genuinely absent from the canonical history under the pin: the walk read
    /// every block down to the dependency horizon and it is not there. Permanent, and a certificate
    /// naming it is invalid rather than early.
    CommitmentAbsentFromCanonicalHistory,
    /// The commitment is there but names a different job, executor, bond, or input.
    CommitmentMismatch,
    /// The certificate predates its commitment, or the commitment is older than
    /// `max_commitment_age_blocks`.
    CommitmentOutOfRange,
    /// The sortition beacon's epoch has not anchored yet. Transient: not creditable *yet*.
    BeaconNotReady,
    /// The certificate was accepted before its own beacon — it revealed before the randomness that
    /// picks its auditors was fixed.
    CertificatePredatesBeacon,
    /// `(h_M, h_R)` is not in the network's model cost table, so the job normalizes to zero.
    UnregisteredProfile,
    /// The verifier committee has not reached `min_verifier_confirmations`, or reached
    /// `min_verifier_refutations`. Transient while verdicts are still landing.
    NotVerified,
    /// Verified, but `normalize_vlt` priced it at zero.
    ZeroValued,
    /// Still inside its challenge window. Transient by construction.
    ChallengeNotMature,
    /// A fraud proof against it was adjudicated as standing (§6), or it failed to resolve — which
    /// is itself proof for an `InvalidCertificate` challenge.
    CertificateRefuted,
    /// This executor was already credited for this `job_id`.
    AlreadyCredited,
}

impl VltCreditSkipReason {
    /// Every variant, for metric registration and exhaustive reporting. Adding a variant without
    /// adding it here is caught by `every_skip_reason_is_registered`.
    pub const ALL: [Self; 17] = [
        Self::EpochAnchorNotReady,
        Self::EpochAnchorOutsideWindow,
        Self::BondMissing,
        Self::BondInactive,
        Self::ExecutorSignatureInvalid,
        Self::CommitmentNotLoaded,
        Self::CommitmentAbsentFromCanonicalHistory,
        Self::CommitmentMismatch,
        Self::CommitmentOutOfRange,
        Self::BeaconNotReady,
        Self::CertificatePredatesBeacon,
        Self::UnregisteredProfile,
        Self::NotVerified,
        Self::ZeroValued,
        Self::ChallengeNotMature,
        Self::CertificateRefuted,
        Self::AlreadyCredited,
    ];

    /// Stable snake_case label for logs and the `reason=` metric dimension. Treat these as an API:
    /// an operator's alert rule matches on the string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::EpochAnchorNotReady => "epoch_anchor_not_ready",
            Self::EpochAnchorOutsideWindow => "epoch_anchor_outside_window",
            Self::BondMissing => "bond_missing",
            Self::BondInactive => "bond_inactive",
            Self::ExecutorSignatureInvalid => "executor_signature_invalid",
            Self::CommitmentNotLoaded => "commitment_not_loaded",
            Self::CommitmentAbsentFromCanonicalHistory => "commitment_absent_from_canonical_history",
            Self::CommitmentMismatch => "commitment_mismatch",
            Self::CommitmentOutOfRange => "commitment_out_of_range",
            Self::BeaconNotReady => "beacon_not_ready",
            Self::CertificatePredatesBeacon => "certificate_predates_beacon",
            Self::UnregisteredProfile => "unregistered_profile",
            Self::NotVerified => "not_verified",
            Self::ZeroValued => "zero_valued",
            Self::ChallengeNotMature => "challenge_not_mature",
            Self::CertificateRefuted => "certificate_refuted",
            Self::AlreadyCredited => "already_credited",
        }
    }

    /// Index into the metric array. Position in [`Self::ALL`], so the two cannot drift.
    pub fn index(&self) -> usize {
        Self::ALL.iter().position(|r| r == self).expect("every variant is in ALL")
    }

    /// What kind of answer this is. The three differ in what the caller may DO with a snapshot
    /// containing one, which is why they are not all just "skipped":
    ///
    /// * [`VltCreditSeverity::Pending`] — correct and final for now, will resolve itself. The
    ///   snapshot is authoritative and may be cached.
    /// * [`VltCreditSeverity::Invalid`] — correct and final forever. Also cacheable.
    /// * [`VltCreditSeverity::Incomplete`] — **not an answer at all**. This node could not load
    ///   what it needed. A snapshot containing one must never be written to the write-once
    ///   accumulator and must never satisfy an activation check: caching it converts a local
    ///   loading limit into permanent consensus history.
    pub fn severity(&self) -> VltCreditSeverity {
        match self {
            Self::EpochAnchorNotReady | Self::BeaconNotReady | Self::NotVerified | Self::ChallengeNotMature => {
                VltCreditSeverity::Pending
            }
            Self::CommitmentNotLoaded => VltCreditSeverity::Incomplete,
            _ => VltCreditSeverity::Invalid,
        }
    }

    /// Whether this is a "not yet" rather than a "never" — for operator-facing text only. Use
    /// [`Self::severity`] for anything that decides behaviour.
    pub fn is_transient(&self) -> bool {
        self.severity() == VltCreditSeverity::Pending
    }
}

/// How far below the certificate floor a credit walk must search before it may call a phase-1
/// commitment absent.
///
/// The two floors are **not** the same number, and conflating them is what made twenty executed,
/// certified and verifier-confirmed jobs worth nothing on 2026-08-09. The certificate floor is
/// raised to the oldest epoch the caller still needs re-derived — in the steady state, one epoch
/// back. A commitment sits below its certificate by at least one full epoch, because the
/// certificate cannot exist until the beacon (the anchor of the epoch *after* the commitment's)
/// does, and legally by up to `max_commitment_age_blocks`. Bound the dependency by the certificate
/// floor and every certificate becomes unresolvable in the steady state.
pub fn commitment_dependency_horizon(certificate_floor_blue: u64, max_commitment_age_blocks: u64) -> u64 {
    certificate_floor_blue.saturating_sub(max_commitment_age_blocks)
}

/// Whether a commitment accepted at `commitment_blue` is one a certificate accepted at
/// `certificate_blue` may legally reference: at or below it, and no older than the age bound.
///
/// The DAA-denominated test in the credit walk is the authoritative one; this is the blue-score
/// bound the *search* uses, and blue score advances no faster than DAA, so it errs by searching
/// slightly too far rather than too little.
pub fn commitment_within_dependency_horizon(commitment_blue: u64, certificate_blue: u64, max_commitment_age_blocks: u64) -> bool {
    commitment_blue <= certificate_blue && certificate_blue.saturating_sub(commitment_blue) <= max_commitment_age_blocks
}

/// What a [`VltCreditSkipReason`] entitles the caller to do with the snapshot that produced it.
///
/// The distinction that matters is `Incomplete` against the other two. `Pending` and `Invalid` are
/// *answers* — the certificate will credit later, or never — and a snapshot full of them is a
/// correct snapshot. `Incomplete` is the absence of an answer, and a snapshot containing one is
/// not a smaller table but an unknown one.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum VltCreditSeverity {
    /// Not yet. Will resolve without intervention.
    Pending,
    /// Never. Settled against this certificate for good.
    Invalid,
    /// Unknown — this node could not load a dependency. Never cache, never activate on it.
    Incomplete,
}

/// One recompute's credit-walk tally: how many certificates were candidates, how many credited,
/// and why the rest did not.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VltCreditTally {
    pub candidates: u64,
    pub accepted: u64,
    /// Indexed by [`VltCreditSkipReason::index`].
    pub skipped: [u64; 17],
}

impl VltCreditTally {
    pub fn note_candidate(&mut self) {
        self.candidates += 1;
    }

    pub fn note_accepted(&mut self) {
        self.accepted += 1;
    }

    pub fn note_skipped(&mut self, reason: VltCreditSkipReason) {
        self.skipped[reason.index()] += 1;
    }

    pub fn count(&self, reason: VltCreditSkipReason) -> u64 {
        self.skipped[reason.index()]
    }

    /// Whether any dependency could not be loaded. **The snapshot this tally came from must not be
    /// written to the write-once accumulator, and must not satisfy an activation check.** Both
    /// would turn "this node has not got it yet" into "nobody ever will".
    pub fn is_incomplete(&self) -> bool {
        VltCreditSkipReason::ALL.iter().any(|r| r.severity() == VltCreditSeverity::Incomplete && self.count(*r) > 0)
    }

    /// `reason=count` for every reason that fired, in [`VltCreditSkipReason::ALL`] order. Empty
    /// when nothing was skipped.
    pub fn summary(&self) -> String {
        VltCreditSkipReason::ALL
            .iter()
            .filter(|r| self.count(**r) > 0)
            .map(|r| format!("{}={}", r.as_str(), self.count(*r)))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// The flat gauge set behind [`VltActivationState::gauges`] — one struct so the RPC view, the
/// metrics exporter and any test assert on the same fields.
///
/// `weight_fence_reached && !finality_active` is the alertable condition: the fork happened and
/// the network is finalizing nothing. Do not collapse the two into one "active" flag.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct VltGauges {
    pub shadow_active: bool,
    pub weight_fence_reached: bool,
    pub finality_active: bool,
    pub total_weight: u128,
    pub quorum_weight: u128,
    pub snapshot_epoch: u64,
    pub snapshot_root: Hash64,
}

/// Which single condition stops a snapshot from being eligible to activate weighted finality.
///
/// Named rather than counted: an operator watching a network sit at the fence needs to know which
/// of four things to wait for or fix, and "not eligible" is the answer that sent the last
/// investigation down a walk it did not need to read.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum VltActivationBlocker {
    /// A dependency could not be loaded, so the table is an unknown rather than a small answer.
    /// Never activate on one: see [`VltCreditSeverity::Incomplete`].
    ResolutionIncomplete,
    /// `Σ_i min{C_i(E), λ·B_i(E)}` is below `min_network_compute`. §4 defers the set transition
    /// rather than finalizing over too little compute.
    BelowMinNetworkCompute { total_weight: u128, min_network_compute: u128 },
    /// Enough weight, too few validators holding it. `Q(E)` is a *fraction*, so weight concentrated
    /// in one validator lets that validator finalize the chain for everybody however large it is.
    TooFewCreditedValidators { credited: usize, required: usize },
    /// The snapshot is pinned at a block that is not DNS-confirmed, so it is not yet a fact about
    /// the shared prefix — two branches could still disagree about the denominator.
    SourceAnchorNotFinalized,
}

impl VltActivationBlocker {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ResolutionIncomplete => "resolution_incomplete",
            Self::BelowMinNetworkCompute { .. } => "below_min_network_compute",
            Self::TooFewCreditedValidators { .. } => "too_few_credited_validators",
            Self::SourceAnchorNotFinalized => "source_anchor_not_finalized",
        }
    }
}

/// Whether a snapshot may be used to switch voting weight onto verified compute, or what stops it.
///
/// `Ok(())` is deliberately narrow. "The credit table is non-empty" is far too weak a test: one
/// validator holding one job satisfies it, and `Q(E) = ⌊2W(E)/3⌋ + 1` is a fraction, so at
/// `W(E) = 50 VLT` the quorum is 34 and a single validator finalizes the chain for everyone. That
/// is the vacuous case `min_network_compute` exists to exclude, arriving through a different door.
///
/// `total_effective_weight` must be the post-cap weight — `Σ_i min{C_i(E), λ·B_i(E)}` — because
/// that is what will actually be voted with, not the credit that was minted.
pub fn vlt_activation_eligibility(
    resolution_complete: bool,
    total_effective_weight: u128,
    credited_validators: usize,
    source_anchor_finalized: bool,
    params: &VltParams,
) -> Result<(), VltActivationBlocker> {
    if !resolution_complete {
        return Err(VltActivationBlocker::ResolutionIncomplete);
    }
    if !source_anchor_finalized {
        return Err(VltActivationBlocker::SourceAnchorNotFinalized);
    }
    if total_effective_weight == 0 || total_effective_weight < params.min_network_compute {
        return Err(VltActivationBlocker::BelowMinNetworkCompute {
            total_weight: total_effective_weight,
            min_network_compute: params.min_network_compute,
        });
    }
    if credited_validators < params.min_active_validators as usize {
        return Err(VltActivationBlocker::TooFewCreditedValidators {
            credited: credited_validators,
            required: params.min_active_validators as usize,
        });
    }
    Ok(())
}

impl VltActivationState {
    /// Whether weighted finality can actually finalize anything right now. False in every state
    /// but [`Self::Active`] — including both fence-reached states, which is the distinction the
    /// booleans could not draw.
    pub fn finality_active(&self) -> bool {
        matches!(self, Self::Active { .. })
    }

    /// Whether the weight fence is behind us, regardless of whether anything can be finalized.
    pub fn weight_fence_reached(&self) -> bool {
        matches!(
            self,
            Self::AwaitingEligibleSnapshot { .. } | Self::ActivationScheduled { .. } | Self::Active { .. } | Self::Recovery { .. }
        )
    }

    /// The scrape-shaped view of this state: the gauges an operator alerts on.
    ///
    /// Flat and numeric on purpose — a monitoring system cannot match on a Rust enum, and the
    /// distinction that matters most (`weight_fence_reached && !finality_active`) has to be
    /// expressible as a query. `snapshot_root` is carried as the hash so five nodes can be
    /// compared by one string.
    pub fn gauges(&self) -> VltGauges {
        let (total_weight, quorum_weight, epoch, root) = match self {
            Self::PreShadow | Self::Shadow => (0, 0, 0, Hash64::default()),
            Self::AwaitingEligibleSnapshot { blocker, .. } => (
                match blocker {
                    VltActivationBlocker::BelowMinNetworkCompute { total_weight, .. } => *total_weight,
                    _ => 0,
                },
                0,
                0,
                Hash64::default(),
            ),
            Self::ActivationScheduled { total_weight, snapshot_root, activation_epoch, .. } => {
                (*total_weight, 0, *activation_epoch, *snapshot_root)
            }
            Self::Recovery { total_weight, .. } => (*total_weight, 0, 0, Hash64::default()),
            Self::Active { epoch, snapshot_root, total_weight, quorum_weight } => {
                (*total_weight, *quorum_weight, *epoch, *snapshot_root)
            }
        };
        VltGauges {
            shadow_active: !matches!(self, Self::PreShadow),
            weight_fence_reached: self.weight_fence_reached(),
            finality_active: self.finality_active(),
            total_weight,
            quorum_weight,
            snapshot_epoch: epoch,
            snapshot_root: root,
        }
    }

    /// A short, stable label for logs and metrics.
    pub fn label(&self) -> &'static str {
        match self {
            Self::PreShadow => "pre_shadow",
            Self::Shadow => "shadow",
            Self::AwaitingEligibleSnapshot { .. } => "awaiting_eligible_snapshot",
            Self::ActivationScheduled { .. } => "activation_scheduled",
            Self::Active { .. } => "active",
            Self::Recovery { .. } => "recovery",
        }
    }
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

    /// The distinction two booleans could not draw, and the reason this is an enum.
    ///
    /// "Past the weight fence" and "finalizing" are different facts. A network that crossed the
    /// fence with nothing to vote with is not active — under §4 an epoch below
    /// `min_network_compute` does not finalize at all — and reporting it as active is how a node
    /// says it is healthy while finalizing nothing. That state is also not `Recovery`: nothing was
    /// ever finalized, so there is no anchor to hold.
    #[test]
    fn activation_state_separates_reaching_the_fence_from_finalizing() {
        let root = h64(9);
        let anchor = h64(7);
        let p = VltParams { min_network_compute: 1_000, min_active_validators: 2, ..VltParams::INERT };
        // One recompute against a fresh (no-record) machine: what does the network look like from
        // here? Activation from scratch takes a reservation plus a boundary, so `Active` is never
        // the answer in one tick — that is the immediate-Active path this machine removed.
        let tick = |newest: Option<(u64, u128)>, validators: usize, confirmed: bool| {
            let e = vlt_activation_eligibility(true, newest.map_or(0, |(_, w)| w), validators, confirmed, &p);
            let last = if confirmed { anchor } else { Hash64::default() };
            tick_vlt_activation(true, true, None, 10, newest, root, last, e, 5_000, 10).1
        };

        let e_ok = vlt_activation_eligibility(true, 5_000, 5, true, &p);
        assert_eq!(
            tick_vlt_activation(false, false, None, 10, None, root, Hash64::default(), e_ok, 5_000, 10).1,
            VltActivationState::PreShadow
        );
        let e_ok = vlt_activation_eligibility(true, 5_000, 5, true, &p);
        assert_eq!(
            tick_vlt_activation(true, false, None, 10, Some((4, 5_000)), root, Hash64::default(), e_ok, 5_000, 10).1,
            VltActivationState::Shadow
        );

        // Past the fence with nothing to vote with: the fence is the EARLIEST point the switch may
        // happen, so this waits on bootstrap weight rather than moving the vote onto an empty table.
        let s = tick(Some((4, 0)), 0, true);
        assert!(matches!(s, VltActivationState::AwaitingEligibleSnapshot { .. }), "got {s:?}");
        assert!(s.weight_fence_reached(), "the fence IS behind us");
        assert!(!s.finality_active(), "and nothing is being finalized — the two are not the same fact");

        // W_min - 1 stays awaiting; W_min earns a RESERVATION for the next epoch — not activation.
        // The boundary is the whole point of the floor, and the epoch delay the whole point of the
        // reservation.
        assert!(matches!(tick(Some((4, 999)), 5, true), VltActivationState::AwaitingEligibleSnapshot { .. }));
        assert!(matches!(tick(Some((4, 1_000)), 5, true), VltActivationState::ActivationScheduled { activation_epoch: 11, .. }));

        // An unconfirmed source anchor never schedules: the snapshot is not yet a fact about the
        // shared prefix, so two branches could still disagree about the denominator.
        assert!(matches!(
            tick(Some((4, 9_000)), 5, false),
            VltActivationState::AwaitingEligibleSnapshot { blocker: VltActivationBlocker::SourceAnchorNotFinalized, .. }
        ));

        // Enough weight, too few holding it: `Q(E)` is a fraction, so one validator with all of it
        // would finalize the chain alone however large the number.
        assert!(matches!(
            tick(Some((4, 3_000)), 1, true),
            VltActivationState::AwaitingEligibleSnapshot { blocker: VltActivationBlocker::TooFewCreditedValidators { .. }, .. }
        ));

        // An incomplete resolution never schedules, whatever the weight says: it is an unknown
        // answer, not a small one.
        assert!(matches!(
            tick_vlt_activation(
                true,
                true,
                None,
                10,
                Some((4, 9_000)),
                root,
                anchor,
                vlt_activation_eligibility(false, 9_000, 5, true, &p),
                5_000,
                10
            )
            .1,
            VltActivationState::AwaitingEligibleSnapshot { blocker: VltActivationBlocker::ResolutionIncomplete, .. }
        ));

        // The same shortfall AFTER activation is `Recovery`, never a fall back to bootstrap: two
        // authorities on one chain would let a fork choose the one it prefers. "After activation"
        // is the persisted record's word, not an inference from bootstrap confirmations.
        let active = VltActivationRecord {
            state: PersistedVltActivationState::Active,
            source_anchor: anchor,
            snapshot_epoch: 3,
            snapshot_root: root,
            scheduled_at_epoch: 8,
            activation_epoch: 9,
            total_weight: 3_000,
            quorum_weight: bft_quorum(3_000),
            ..VltActivationRecord::awaiting()
        };
        let e = vlt_activation_eligibility(true, 0, 0, true, &p);
        let (r, s) = tick_vlt_activation(true, true, Some(&active), 10, Some((4, 0)), root, anchor, e, 5_000, 10);
        assert!(matches!(s, VltActivationState::Recovery { .. }), "got {s:?}");
        assert!(!matches!(s, VltActivationState::AwaitingEligibleSnapshot { .. }), "Active must never fall back to bootstrap");
        // The record follows the report into Recovery — and keeps every stamped field, because
        // recovery is a pause in finalizing, not a re-activation.
        let recovered = r.expect("record");
        assert_eq!(recovered.state, PersistedVltActivationState::Recovery);
        assert!(recovered.is_active(), "recovery is still post-activation: bootstrap is behind us");
        assert_eq!(
            (recovered.scheduled_at_epoch, recovered.activation_epoch, recovered.snapshot_root),
            (active.scheduled_at_epoch, active.activation_epoch, active.snapshot_root)
        );

        // With the record Active and the snapshot healthy again ⇒ active, on live values.
        let e = vlt_activation_eligibility(true, 3_000, 5, true, &p);
        let s = tick_vlt_activation(true, true, Some(&active), 10, Some((4, 3_000)), root, anchor, e, 5_000, 10).1;
        assert_eq!(
            s,
            VltActivationState::Active { epoch: 4, snapshot_root: root, total_weight: 3_000, quorum_weight: bft_quorum(3_000) }
        );
        assert!(s.finality_active());
    }

    /// The §6 lifecycle as the persisted record walks it: reserve at `E` for `E+1`, re-evaluate at
    /// the boundary, activate — or cancel, explicitly, if the proof stopped holding in between.
    /// The record is what a restart resumes from, so every step here checks the RECORD as well as
    /// the reported state.
    #[test]
    fn activation_record_walks_the_reservation_lifecycle() {
        let root = h64(9);
        let anchor = h64(7);
        let p = VltParams { min_network_compute: 1_000, min_active_validators: 2, ..VltParams::INERT };
        let ok = |w: u128| vlt_activation_eligibility(true, w, 5, true, &p);
        let fence = 5_000u64;

        // Epoch 10: eligible ⇒ a reservation for 11, persisted as such.
        let (r, s) = tick_vlt_activation(true, true, None, 10, Some((4, 2_000)), root, anchor, ok(2_000), fence, 10);
        let scheduled = r.expect("above the weight fence the machine always yields a record");
        assert_eq!(scheduled.state, PersistedVltActivationState::ActivationScheduled);
        assert_eq!((scheduled.scheduled_at_epoch, scheduled.activation_epoch), (10, 11));
        assert_eq!((scheduled.snapshot_epoch, scheduled.snapshot_root, scheduled.total_weight), (4, root, 2_000));
        assert_eq!(scheduled.quorum_weight, bft_quorum(2_000));
        assert_eq!(
            s,
            VltActivationState::ActivationScheduled {
                activation_epoch: 11,
                source_anchor: anchor,
                snapshot_root: root,
                total_weight: 2_000
            }
        );

        // Still epoch 10 (a second recompute inside the epoch): the reservation holds, unchanged —
        // the step is idempotent within an epoch, so a restart that replays it rewrites nothing.
        let (r, _) = tick_vlt_activation(true, true, Some(&scheduled), 10, Some((4, 2_500)), h64(10), anchor, ok(2_500), fence, 10);
        assert_eq!(r.as_ref(), Some(&scheduled), "a committed reservation is not re-stamped while it waits");

        // Epoch 11, the boundary: the re-evaluation passes ⇒ Active, stamped with the LIVE
        // snapshot the re-evaluation approved (epoch 5, new root), at the reserved epoch.
        let live_root = h64(11);
        let (r, s) = tick_vlt_activation(true, true, Some(&scheduled), 11, Some((5, 2_400)), live_root, anchor, ok(2_400), fence, 11);
        let active = r.expect("record");
        assert_eq!(active.state, PersistedVltActivationState::Active);
        assert_eq!((active.scheduled_at_epoch, active.activation_epoch), (10, 11));
        assert_eq!((active.snapshot_epoch, active.snapshot_root, active.total_weight), (5, live_root, 2_400));
        assert!(matches!(s, VltActivationState::Active { epoch: 5, total_weight: 2_400, .. }));

        // Terminal: eligibility failing afterwards reports Recovery but the record does not move,
        // and eligibility recovering reports Active again — never a second activation.
        let (r, s) = tick_vlt_activation(
            true,
            true,
            Some(&active),
            12,
            Some((6, 100)),
            h64(12),
            anchor,
            vlt_activation_eligibility(true, 100, 5, true, &p),
            fence,
            12,
        );
        assert_eq!(r.as_ref().map(|r| r.state), Some(PersistedVltActivationState::Recovery), "weight loss pauses finality");
        assert!(matches!(s, VltActivationState::Recovery { total_weight: 100, min_network_compute: 1_000, .. }));
        // Eligibility holding while the record is still Active keeps it Active — the pause is
        // driven by the eligibility verdict, not by the passage of epochs.
        let (r, s) = tick_vlt_activation(true, true, Some(&active), 13, Some((7, 3_000)), h64(13), anchor, ok(3_000), fence, 13);
        assert_eq!(r.as_ref(), Some(&active), "an eligible Active record is not re-stamped");
        assert!(matches!(s, VltActivationState::Active { .. }));

        // The cancel path: a reservation whose snapshot loses eligibility before its boundary —
        // a successful challenge can do that — cancels back to Awaiting rather than activating on
        // a proof that no longer holds. Same rule AT the boundary: re-evaluation failed, no switch.
        let refuted = vlt_activation_eligibility(true, 500, 5, true, &p);
        let (r, s) = tick_vlt_activation(true, true, Some(&scheduled), 10, Some((4, 500)), root, anchor, refuted, fence, 10);
        assert_eq!(r, Some(VltActivationRecord::awaiting()), "cancelled, not silently kept");
        assert!(matches!(s, VltActivationState::AwaitingEligibleSnapshot { .. }));
        let refuted = vlt_activation_eligibility(true, 500, 5, true, &p);
        let (r, s) = tick_vlt_activation(true, true, Some(&scheduled), 11, Some((5, 500)), root, anchor, refuted, fence, 11);
        assert_eq!(r, Some(VltActivationRecord::awaiting()), "the boundary re-evaluation really evaluates");
        assert!(matches!(s, VltActivationState::AwaitingEligibleSnapshot { .. }));

        // From the cancellation, a later eligible snapshot starts a FRESH reservation.
        let (r, _) = tick_vlt_activation(
            true,
            true,
            Some(&VltActivationRecord::awaiting()),
            14,
            Some((8, 2_000)),
            root,
            anchor,
            ok(2_000),
            fence,
            14,
        );
        assert_eq!((r.as_ref().unwrap().scheduled_at_epoch, r.as_ref().unwrap().activation_epoch), (14, 15));

        // A node that crossed several epochs in one commit (restart, IBD) still activates at the
        // reserved epoch: `>=`, not `==`, but only through the same re-evaluation.
        let (r, _) = tick_vlt_activation(true, true, Some(&scheduled), 19, Some((12, 2_000)), h64(19), anchor, ok(2_000), fence, 19);
        let late = r.unwrap();
        assert_eq!((late.state, late.activation_epoch), (PersistedVltActivationState::Active, 11));

        // Below the fences the record passes through untouched — fences are a fact about the DAA
        // score, not about the machine, and lowering them back must not lose history.
        let e = vlt_activation_eligibility(true, 2_000, 5, true, &p);
        let (r, s) = tick_vlt_activation(true, false, Some(&active), 20, None, root, anchor, e, fence, 20);
        assert_eq!((r.as_ref(), s), (Some(&active), VltActivationState::Shadow));
    }

    /// The reservation is stamped with the CHAIN's epoch, not this node's observation epoch. A
    /// recompute that first notices an eligible run late — or a replayer that steps through
    /// history coarsely — must derive the record a node that lived every boundary derives: same
    /// `scheduled_at_epoch`, same `activation_epoch`. Found live as the §12 identity tuple's one
    /// disagreeing field: five present nodes said `activation_epoch=37`, a pruning-imported
    /// sixth said 36 — and 36, the epoch after the eligible run began, is the canonical answer.
    #[test]
    fn late_observation_stamps_the_canonical_reservation() {
        let root = h64(9);
        let anchor = h64(7);
        let p = VltParams { min_network_compute: 1_000, min_active_validators: 2, ..VltParams::INERT };
        let ok = |w: u128| vlt_activation_eligibility(true, w, 5, true, &p);

        // The run of eligible snapshots began at epoch 35; this node first observes at 37.
        let (r, s) = tick_vlt_activation(true, true, None, 37, Some((35, 2_000)), root, anchor, ok(2_000), 5_000, 35);
        let scheduled = r.unwrap();
        assert_eq!((scheduled.scheduled_at_epoch, scheduled.activation_epoch), (35, 36));
        assert!(matches!(s, VltActivationState::ActivationScheduled { activation_epoch: 36, .. }));

        // The very next recompute activates — lateness costs nothing further, and the record
        // carries the canonical epoch, never observation+1.
        let (r, s) = tick_vlt_activation(true, true, Some(&scheduled), 37, Some((36, 2_400)), h64(11), anchor, ok(2_400), 5_000, 35);
        let active = r.unwrap();
        assert_eq!((active.state, active.activation_epoch), (PersistedVltActivationState::Active, 36));
        assert!(matches!(s, VltActivationState::Active { .. }));

        // A canonical epoch from a broken caller can never postdate the wall epoch: the machine
        // clamps rather than scheduling the future.
        let (r, _) = tick_vlt_activation(true, true, None, 10, Some((9, 2_000)), root, anchor, ok(2_000), 5_000, 99);
        assert_eq!((r.as_ref().unwrap().scheduled_at_epoch, r.as_ref().unwrap().activation_epoch), (10, 11));
    }

    /// §10.2's return path, which is the whole of PR 6's state machine: weight collapses, the
    /// network holds its last finalized anchor, and when weight returns it must PROVE itself at a
    /// boundary again rather than resuming mid-epoch. The one transition that must never exist is
    /// recovery → awaiting: that is the door back to bonded-stake bootstrap, and it is shut once
    /// anything has been finalized under VLT weight.
    #[test]
    fn recovery_returns_through_a_reservation_and_never_to_bootstrap() {
        let root = h64(9);
        let anchor = h64(7);
        let p = VltParams { min_network_compute: 1_000, min_active_validators: 2, ..VltParams::INERT };
        let ok = |w: u128| vlt_activation_eligibility(true, w, 5, true, &p);
        let dead = || vlt_activation_eligibility(true, 0, 0, true, &p);
        let fence = 5_000u64;
        let tick = |prev: Option<&VltActivationRecord>, epoch: u64, w: u128, e: Result<(), VltActivationBlocker>| {
            tick_vlt_activation(true, true, prev, epoch, Some((epoch - 2, w)), root, anchor, e, fence, epoch)
        };

        // Reach Active the ordinary way.
        let (r, _) = tick(None, 10, 3_000, ok(3_000));
        let (r, _) = tick(r.as_ref(), 11, 3_000, ok(3_000));
        let active = r.expect("record");
        assert_eq!(active.state, PersistedVltActivationState::Active);

        // Weight collapses (every validator's compute aged out, or the network partitioned away):
        // the record pauses in Recovery and the report names the anchor being held.
        let (r, s) = tick(Some(&active), 12, 0, dead());
        let recovering = r.expect("record");
        assert_eq!(recovering.state, PersistedVltActivationState::Recovery);
        assert!(matches!(s, VltActivationState::Recovery { last_finalized_anchor, .. } if last_finalized_anchor == anchor));
        // It stays there while the weight is gone, however many epochs pass.
        let (r, s) = tick(Some(&recovering), 20, 0, dead());
        assert_eq!(r.as_ref().map(|r| r.state), Some(PersistedVltActivationState::Recovery));
        assert!(matches!(s, VltActivationState::Recovery { .. }));

        // Weight returns ⇒ a RESERVATION, not an immediate resumption. The original
        // `activation_epoch` is history and stays put; the new reservation rides scheduled_at.
        let (r, s) = tick(Some(&recovering), 21, 2_000, ok(2_000));
        let rescheduled = r.expect("record");
        assert_eq!(rescheduled.state, PersistedVltActivationState::ActivationScheduled);
        assert_eq!(rescheduled.scheduled_at_epoch, 21);
        assert_eq!(rescheduled.activation_epoch, active.activation_epoch, "re-entry does not rewrite when this chain first activated");
        assert!(matches!(s, VltActivationState::ActivationScheduled { activation_epoch: 22, .. }));
        // Same epoch again: idempotent, no re-stamp.
        assert_eq!(tick(Some(&rescheduled), 21, 2_500, ok(2_500)).0.as_ref(), Some(&rescheduled));

        // The boundary re-evaluation passes ⇒ finalizing again.
        let (r, s) = tick(Some(&rescheduled), 22, 2_400, ok(2_400));
        assert_eq!(r.as_ref().map(|r| r.state), Some(PersistedVltActivationState::Active));
        assert!(matches!(s, VltActivationState::Active { total_weight: 2_400, .. }));

        // A re-entry that loses eligibility before its boundary falls back to RECOVERY, not to
        // Awaiting: bootstrap weight is not an option once anything has been finalized.
        let (r, s) = tick(Some(&rescheduled), 22, 0, dead());
        assert_eq!(
            r.as_ref().map(|r| r.state),
            Some(PersistedVltActivationState::Recovery),
            "a cancelled re-entry returns to recovery"
        );
        assert!(matches!(s, VltActivationState::Recovery { .. }));

        // Whereas a FIRST-time reservation that loses eligibility does fall back to Awaiting,
        // because bootstrap finality is still what is in force there.
        let (r, _) = tick(None, 30, 2_000, ok(2_000));
        let first = r.expect("record");
        assert_eq!(first.activation_epoch, 31);
        let (r, s) = tick(Some(&first), 30, 0, dead());
        assert_eq!(r, Some(VltActivationRecord::awaiting()), "a pre-activation reservation cancels to awaiting");
        assert!(matches!(s, VltActivationState::AwaitingEligibleSnapshot { .. }));

        // And no input sequence from Recovery reaches Awaiting.
        for w in [0u128, 1, 999, 5_000] {
            let e = if w >= 1_000 { ok(w) } else { vlt_activation_eligibility(true, w, 5, true, &p) };
            let (r, s) = tick(Some(&recovering), 40, w, e);
            assert!(!matches!(s, VltActivationState::AwaitingEligibleSnapshot { .. }), "w={w} reported awaiting");
            assert_ne!(r.as_ref().map(|r| r.state), Some(PersistedVltActivationState::AwaitingEligibleSnapshot), "w={w}");
        }
    }

    /// The frozen snapshot is only a shared denominator if its roots are a pure function of its
    /// consensus content: same rows in any input order ⇒ one root; any consensus field moved ⇒ a
    /// different root; the local `resolution_complete` licence ⇒ no effect at all.
    #[test]
    fn voting_snapshot_roots_commit_content_not_input_order() {
        let row = |id: u8, raw: u128, cap: u128| VltValidatorWeight {
            validator_id: h64(id),
            consensus_key: vec![id; 8],
            bond_outpoint: TransactionOutpoint::new(TransactionId::from_bytes([id; 64]), 0),
            raw_recent_compute: raw,
            bond_cap: cap,
            effective_weight: raw.min(cap),
        };
        let base = VltVotingSnapshot {
            version: VLT_VOTING_SNAPSHOT_VERSION_V1,
            source_finalized_anchor: h64(1),
            source_anchor_daa: 900,
            snapshot_epoch: 4,
            activation_epoch: 7,
            model_table_hash: h64(2),
            capability_set_root: h64(3),
            validator_set_root: Hash64::default(),
            credit_table_root: h64(4),
            snapshot_root: Hash64::default(),
            validators: vec![row(9, 500, 400), row(3, 250, 800)],
            total_weight: 0,
            quorum_weight: 0,
            resolution_complete: true,
        };
        let sealed = base.clone().seal();
        // Sealing sorts into consensus order and derives the totals from the rows.
        assert_eq!(sealed.validators[0].validator_id, h64(3), "validator_id ascending is a consensus rule");
        assert_eq!(sealed.total_weight, 400 + 250);
        assert_eq!(sealed.quorum_weight, bft_quorum(650));
        assert_eq!(sealed.snapshot_root, sealed.compute_snapshot_root(), "the field is only ever the computed value");

        // Same rows, opposite input order ⇒ byte-identical roots.
        let mut flipped = base.clone();
        flipped.validators.reverse();
        let flipped = flipped.seal();
        assert_eq!(flipped.snapshot_root, sealed.snapshot_root);
        assert_eq!(flipped.validator_set_root, sealed.validator_set_root);

        // The local licence is outside every root: two honest nodes may disagree on it while
        // agreeing on the denominator.
        let mut incomplete = base.clone();
        incomplete.resolution_complete = false;
        assert_eq!(incomplete.seal().snapshot_root, sealed.snapshot_root);

        // Every consensus field moves the snapshot root; the set root moves only with the set.
        let mut w = base.clone();
        w.validators[0].effective_weight = 401;
        let w = w.seal();
        assert_ne!(w.snapshot_root, sealed.snapshot_root, "a weight is consensus content");
        assert_eq!(w.validator_set_root, sealed.validator_set_root, "but not part of WHO may vote");
        let mut k = base.clone();
        k.validators[0].consensus_key = vec![0xff; 8];
        let k = k.seal();
        assert_ne!(k.validator_set_root, sealed.validator_set_root);
        let mut m = base.clone();
        m.model_table_hash = h64(99);
        assert_ne!(m.seal().snapshot_root, sealed.snapshot_root, "rho prices voting power, so the table is committed");

        // The vote commitment binds BOTH roots (§5.1): forging either changes what was signed.
        assert_eq!(sealed.vote_commitment(), vote_snapshot_commitment(sealed.snapshot_root, sealed.validator_set_root));
        assert_ne!(sealed.vote_commitment(), vote_snapshot_commitment(sealed.snapshot_root, h64(8)));
        assert_ne!(sealed.vote_commitment(), vote_snapshot_commitment(h64(8), sealed.validator_set_root));
    }

    /// The gauges are what a monitoring query can match on, so the alertable condition has to be
    /// expressible in them — not just in the enum.
    #[test]
    fn gauges_expose_the_alertable_condition() {
        let fence_no_snapshot = VltActivationState::AwaitingEligibleSnapshot {
            weight_fence_daa: 5_000,
            blocker: VltActivationBlocker::BelowMinNetworkCompute { total_weight: 0, min_network_compute: 1_000 },
        };
        let g = fence_no_snapshot.gauges();
        assert!(g.weight_fence_reached && !g.finality_active, "the one alert worth writing");
        assert_eq!(g.quorum_weight, 0, "no quorum is being applied, so do not report one");

        let active = VltActivationState::Active { epoch: 7, snapshot_root: h64(3), total_weight: 900, quorum_weight: 601 };
        let g = active.gauges();
        assert!(g.finality_active && g.weight_fence_reached);
        assert_eq!((g.snapshot_epoch, g.snapshot_root, g.total_weight, g.quorum_weight), (7, h64(3), 900, 601));

        // Round-trip through the atomic gauge set the RPC reads.
        let m = VltMetrics::default();
        m.record(&active, 12_345);
        assert_eq!(m.read(), (g, 12_345));
        m.record(&fence_no_snapshot, 12_400);
        let (back, daa) = m.read();
        assert!(back.weight_fence_reached && !back.finality_active);
        assert_eq!(daa, 12_400, "the scrape can tell a stalled recompute from a steady state");
    }

    /// The snapshot root is the denominator's identity. Two tables that would give a vote a
    /// different `W(E)` must not share a root, or a vote counted against one is indistinguishable
    /// from a vote counted against the other.
    #[test]
    fn snapshot_root_distinguishes_denominators() {
        let credits = |v: u8, x: u128| {
            let mut m: HashMap<Hash64, BTreeMap<u64, u128>> = HashMap::new();
            m.insert(h64(v), BTreeMap::from([(9u64, x)]));
            m
        };
        let base = VltEpochSnapshot::pinned(h64(1), 100, credits(2, 500));
        assert_eq!(base.commitment_root(), VltEpochSnapshot::pinned(h64(1), 100, credits(2, 500)).commitment_root());
        // A different credit, validator, pin or pin height is a different denominator.
        assert_ne!(base.commitment_root(), VltEpochSnapshot::pinned(h64(1), 100, credits(2, 501)).commitment_root());
        assert_ne!(base.commitment_root(), VltEpochSnapshot::pinned(h64(1), 100, credits(3, 500)).commitment_root());
        assert_ne!(base.commitment_root(), VltEpochSnapshot::pinned(h64(4), 100, credits(2, 500)).commitment_root());
        assert_ne!(base.commitment_root(), VltEpochSnapshot::pinned(h64(1), 101, credits(2, 500)).commitment_root());
        // "No credit" and "no snapshot" stay distinguishable.
        assert_ne!(VltEpochSnapshot::inert().commitment_root(), Hash64::default());
    }

    /// The devnet fixture must be devnet-shaped by *construction*, not by convention.
    ///
    /// The feature flag is the outermost of three constraints and the least interesting: a flag is
    /// a thing someone can turn on. The two that survive a mistake are here — the profile is a
    /// function of the network's own genesis, so it differs per network and exists as a value
    /// nowhere else; and it is not the real PALW profile, so a fixture executor cannot pass itself
    /// off as a real one (nor vice versa).
    #[cfg(feature = "devnet-vlt-fixture")]
    #[test]
    fn devnet_fixture_is_bound_to_its_own_genesis() {
        let devnet = h64(0x11);
        let other = h64(0x22);
        let a = devnet_fixture_entry(devnet);
        let b = devnet_fixture_entry(other);

        // Same network ⇒ same profile, or two honest replicas could not agree.
        assert_eq!(a, devnet_fixture_entry(devnet));
        // Different network ⇒ every identity differs, so a certificate minted against one
        // network's fixture names a profile the other has never registered.
        assert_ne!(a.model_weights_hash, b.model_weights_hash);
        assert_ne!(a.runtime_hash, b.runtime_hash);
        assert_ne!(a.runtime_class_id, b.runtime_class_id);

        // And it is nobody's real profile: a fixture node cannot be mistaken for a PALW executor.
        let palw = palw_qwen36_metal_entry();
        assert_ne!(a.model_weights_hash, palw.model_weights_hash);
        assert_ne!(a.runtime_hash, palw.runtime_hash);
        assert_ne!(a.runtime_class_id, palw.runtime_class_id, "a fixture must never share a determinism class with the real runtime");

        // The devnet table holds the fixture and nothing else, so a real PALW executor pointed at
        // this devnet finds its own profile unregistered and mints zero.
        let table = ModelCostTable::devnet_fixture(devnet);
        assert_eq!(table.live(), &[a]);
        assert!(table.lookup(palw.model_weights_hash, palw.runtime_hash).is_none());
        assert!(table.lookup(a.model_weights_hash, a.runtime_hash).is_some());
        // The other network's fixture is equally unregistered here.
        assert!(table.lookup(b.model_weights_hash, b.runtime_hash).is_none());
    }

    /// One fixture job is worth exactly 50 VLT — and 50 is *derived*, not declared anywhere.
    ///
    /// The quota in `kaspad::compute` counts jobs; the experiment's plan is written in VLT. This is
    /// the conversion between the two, and it holds only because the registered fixture profile
    /// prices at `ρ = 1.0` against the preset's `a = 1.0`, `b = 8.0`. Move any one of the three and
    /// a plan of 400/250/150/100/100 quietly becomes something else while every log line still says
    /// the quota was met — so the number is pinned here rather than recomputed by hand each time.
    #[cfg(feature = "devnet-vlt-fixture")]
    #[test]
    fn one_fixture_job_is_worth_fifty_vlt() {
        use devnet_fixture::{JOB_DECODE_TOKENS, JOB_MAX_TOKENS, JOB_PREFILL_TOKENS};
        let genesis = h64(0x11);
        let entry = devnet_fixture_entry(genesis);
        let params = VltParams { model_cost_table: ModelCostTable::devnet_fixture(genesis), ..VltParams::INERT };
        let job = LlmJobSpec {
            version: VLT_PAYLOAD_VERSION_V1,
            model_weights_hash: entry.model_weights_hash,
            runtime_hash: entry.runtime_hash,
            quantization: QuantizationProfile::Int4,
            input_commitment: h64(3),
            sampling_seed: [7u8; 32],
            max_tokens: JOB_MAX_TOKENS,
            verification_scheme: VerificationScheme::CanonicalFullReplay,
        };
        let r = receipt(JOB_PREFILL_TOKENS, JOB_DECODE_TOKENS);
        let one_job = normalize_vlt(&job, &r, &params, true).expect("the devnet preset registers this profile");
        assert_eq!(one_job, 50 * VLT_MICRO, "the fixture job's price is 1·prefill + 8·decode at rho = 1.0");

        // The shape must fit its own ceiling exactly. Below it every job takes the
        // `ReceiptExceedsSpecLimit` path and mints zero — which on a running devnet looks exactly
        // like "the overlay isn't crediting" and not at all like a two-constant arithmetic slip.
        assert_eq!(JOB_PREFILL_TOKENS + JOB_DECODE_TOKENS, JOB_MAX_TOKENS);
        assert!(JOB_MAX_TOKENS <= entry.max_tokens, "a spec above the profile's ceiling mints nothing");

        // And N jobs are exactly N × 50, with no rounding anywhere: the plan is written in VLT, the
        // quota counts jobs, and these are the five targets it has to land on.
        for (jobs, vlt) in [(8u128, 400u128), (5, 250), (3, 150), (2, 100), (2, 100)] {
            assert_eq!(jobs * one_job, vlt * VLT_MICRO, "{jobs} jobs must be exactly {vlt} VLT");
        }

        // The preset sizes `W_min` off this helper, so it must be the same arithmetic
        // `normalize_vlt` performs — a second definition that drifted would move the activation
        // threshold without moving anything that mints.
        assert_eq!(devnet_fixture_job_vlt(params.prefill_cost_micro, params.decode_cost_micro), one_job);
    }

    /// The skip reasons are an operator-facing API: a label is what an alert rule matches on and
    /// what a metric dimension carries. A variant missing from `ALL` silently stops being counted,
    /// and a duplicated label silently merges two different faults into one number.
    #[test]
    fn every_skip_reason_is_registered_and_uniquely_labelled() {
        let mut labels: Vec<&str> = VltCreditSkipReason::ALL.iter().map(|r| r.as_str()).collect();
        labels.sort_unstable();
        let before = labels.len();
        labels.dedup();
        assert_eq!(labels.len(), before, "two skip reasons share a label");

        for (i, r) in VltCreditSkipReason::ALL.iter().enumerate() {
            assert_eq!(r.index(), i, "{} indexes outside its position in ALL", r.as_str());
            assert!(!r.as_str().is_empty());
        }

        // The tally is indexed by `index()`, so the array must be exactly as long as ALL — a
        // mismatch would panic in the credit walk rather than in a test.
        let mut tally = VltCreditTally::default();
        assert_eq!(tally.skipped.len(), VltCreditSkipReason::ALL.len());
        for r in VltCreditSkipReason::ALL {
            tally.note_skipped(r);
        }
        for r in VltCreditSkipReason::ALL {
            assert_eq!(tally.count(r), 1, "{} was not counted in its own slot", r.as_str());
        }
        assert!(tally.summary().contains("epoch_anchor_not_ready=1"));

        // "Not yet" and "never" have to stay distinguishable: an operator watching a network come
        // up should see only the transient ones, and anything else is a real fault.
        assert!(VltCreditSkipReason::ChallengeNotMature.is_transient());
        assert!(VltCreditSkipReason::BeaconNotReady.is_transient());
        // An epoch whose anchor is merely lagging is a "not yet"; one below the window never
        // anchors from this pin. Calling both permanent is what made a routine wait look like a
        // dead certificate on the very first devnet that reached this path.
        assert!(VltCreditSkipReason::EpochAnchorNotReady.is_transient());
        assert!(!VltCreditSkipReason::EpochAnchorOutsideWindow.is_transient());
        assert!(!VltCreditSkipReason::ExecutorSignatureInvalid.is_transient());

        // An empty walk says nothing rather than saying "no skips".
        assert!(VltCreditTally::default().summary().is_empty());
    }

    /// A commitment below the cached-epoch floor but legal from its certificate must still be
    /// searched for. This is the 2026-08-09 failure reduced to arithmetic: the walk stopped at the
    /// certificate floor, every certificate resolved to "commitment missing", and the empty rows
    /// were then sealed write-once.
    #[test]
    fn the_dependency_horizon_reaches_below_the_certificate_floor() {
        const MAX_AGE: u64 = 6_000;
        // The case the devnet actually hit: the certificate floor is the oldest uncached epoch, and
        // the commitment is 4000 blue score below it — far outside the certificate range, and well
        // inside what its own certificate may legally reference.
        let certificate_floor = 10_000;
        let horizon = commitment_dependency_horizon(certificate_floor, MAX_AGE);
        assert_eq!(horizon, 4_000);
        assert!(horizon < 6_000, "a commitment at 6000 must be inside the searched range");
        assert!(commitment_within_dependency_horizon(6_000, 10_500, MAX_AGE));

        // Exactly at the age bound is legal; one beyond it is not. `resolve_certificate` applies
        // the same bound in DAA, and the two must agree about the boundary itself.
        assert!(commitment_within_dependency_horizon(4_500, 10_500, MAX_AGE));
        assert!(!commitment_within_dependency_horizon(4_499, 10_500, MAX_AGE));

        // A commitment cannot come after the certificate that names it.
        assert!(!commitment_within_dependency_horizon(10_501, 10_500, MAX_AGE));
        assert!(commitment_within_dependency_horizon(10_500, 10_500, MAX_AGE));

        // The horizon must never be *above* the certificate floor, or the search would skip the
        // range the first pass already covers and call present commitments absent.
        for floor in [0u64, 1, MAX_AGE - 1, MAX_AGE, MAX_AGE + 1, u64::MAX] {
            assert!(commitment_dependency_horizon(floor, MAX_AGE) <= floor);
        }
        // And it saturates rather than wrapping under a shallow chain.
        assert_eq!(commitment_dependency_horizon(100, MAX_AGE), 0);
    }

    /// "This node has not loaded it" and "the chain does not contain it" are different facts, and
    /// only the second may be cached as a permanent zero. Collapsing them is what turned a walk
    /// that stopped too early into twenty jobs' credit destroyed for good.
    #[test]
    fn an_unloaded_dependency_is_not_an_absent_one() {
        assert_eq!(VltCreditSkipReason::CommitmentNotLoaded.severity(), VltCreditSeverity::Incomplete);
        assert_eq!(VltCreditSkipReason::CommitmentAbsentFromCanonicalHistory.severity(), VltCreditSeverity::Invalid);
        assert_eq!(VltCreditSkipReason::CommitmentOutOfRange.severity(), VltCreditSeverity::Invalid);
        assert_eq!(VltCreditSkipReason::ChallengeNotMature.severity(), VltCreditSeverity::Pending);

        // Only an Incomplete reason makes the whole tally unusable. A window full of "not yet" and
        // "never" is a correct answer that happens to credit nothing.
        let mut pending_only = VltCreditTally::default();
        pending_only.note_skipped(VltCreditSkipReason::ChallengeNotMature);
        pending_only.note_skipped(VltCreditSkipReason::CommitmentAbsentFromCanonicalHistory);
        pending_only.note_skipped(VltCreditSkipReason::EpochAnchorNotReady);
        assert!(!pending_only.is_incomplete(), "pending and invalid are answers; the table stands");

        let mut unloaded = VltCreditTally::default();
        unloaded.note_skipped(VltCreditSkipReason::CommitmentNotLoaded);
        assert!(unloaded.is_incomplete());

        // And the snapshot carries it, because that is what `stage_vlt_credits` reads before
        // writing a row it can never take back.
        let pin = h64(7);
        assert!(VltEpochSnapshot::pinned(pin, 100, Default::default()).resolution_complete());
        assert!(!VltEpochSnapshot::pinned_incomplete(pin, 100, Default::default()).resolution_complete());
        // A dormant overlay HAS resolved: empty is its final answer, and caching it is correct.
        assert!(VltEpochSnapshot::inert().resolution_complete());
        // A walk that could not run has not.
        assert!(!VltEpochSnapshot::unresolved().resolution_complete());

        // `resolution_complete` must stay out of the commitment root: two honest nodes may differ
        // on what they could load while agreeing exactly on the table, and the root is the table.
        let credits = HashMap::from([(h64(1), BTreeMap::from([(4u64, 1_000u128)]))]);
        assert_eq!(
            VltEpochSnapshot::pinned(pin, 100, credits.clone()).commitment_root(),
            VltEpochSnapshot::pinned_incomplete(pin, 100, credits).commitment_root(),
            "completeness is a local licence to act, never a consensus value"
        );
    }

    /// The shipped presets never carry the fixture, feature or no feature. This is the assertion
    /// that would fail if the hatch were ever wired into a public preset by accident.
    #[cfg(feature = "devnet-vlt-fixture")]
    #[test]
    fn no_shipped_preset_registers_the_fixture() {
        use crate::config::params::{GENESIS_ACTIVE_DNS_PARAMS, PRODUCTION_DNS_PARAMS, TESTNET_DNS_PARAMS};
        for (name, p) in
            [("genesis-active", GENESIS_ACTIVE_DNS_PARAMS), ("production", PRODUCTION_DNS_PARAMS), ("testnet", TESTNET_DNS_PARAMS)]
        {
            assert!(p.vlt.model_cost_table.live().is_empty(), "{name} ships with an empty model table");
        }
    }

    #[test]
    fn inert_params_are_dormant_but_coherent() {
        assert_eq!(VltParams::INERT.vlt_activation_daa_score, u64::MAX);
        assert_eq!(VltParams::INERT.vlt_shadow_activation_daa_score, u64::MAX, "the overlay ships dormant, not merely powerless");
        assert!(!VltParams::INERT.weight_active_at(u64::MAX - 1));
        assert!(VltParams::INERT.weight_active_at(u64::MAX));
        assert!(!VltParams::INERT.shadow_active_at(u64::MAX - 1));
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

    /// Same contract for the Qwen3.5-2B palw-lite profile: identities re-derivable from the
    /// published pins, distinct from each other AND from every identity of the 35B profile — the
    /// worker-side registration check keys on `runtime_hash`, and the committee draw on
    /// `runtime_class_id`, so a collision on either would let the wrong pair look comparable.
    #[test]
    fn qwen35_2b_registry_derives_from_the_published_pins() {
        let e = palw_qwen35_2b_metal_entry();
        assert_eq!(
            e.model_weights_hash,
            derive_model_weights_hash(
                "aaf42c8b7c3cab2bf3d69c355048d4a0ee9973d48f16c731c0520ee914699223",
                1_280_835_840,
                "Qwen3.5-2B-Q4_K_M.gguf",
                "Qwen/Qwen3.5-2B",
                "15852e8c16360a2fea060d615a32b45270f8a8fc",
            )
        );
        assert_eq!(
            e.runtime_hash,
            derive_runtime_hash("030ebb558a5820b444a8f836ed5cdd46c9b4bd7a", "unpatched", 10_358, qwen35_pins::METAL_BUILD_PROFILE)
        );
        assert_eq!(e.runtime_class_id, derive_runtime_class_id("misaka-palw-lite-fp/apple-metal-arm64/v1"));
        assert_eq!(e.rho_micro, VLT_MICRO as u64);
        assert_eq!(e.max_tokens, PALW_QWEN35_2B_MAX_TOKENS);

        // Distinct from the 35B profile in every identity dimension.
        let q36 = palw_qwen36_metal_entry();
        assert_ne!(e.model_weights_hash, q36.model_weights_hash);
        assert_ne!(e.runtime_hash, q36.runtime_hash);
        assert_ne!(e.runtime_class_id, q36.runtime_class_id, "distinct kernels must be distinct determinism classes");

        // The devnet registry resolves each worker to exactly its own profile.
        let table = ModelCostTable::palw_metal_devnet();
        assert_eq!(table.live().len(), 2);
        assert!(table.lookup(e.model_weights_hash, e.runtime_hash).is_some());
        assert!(table.lookup(q36.model_weights_hash, q36.runtime_hash).is_some());
        assert!(table.lookup(e.model_weights_hash, q36.runtime_hash).is_none(), "cross-pairing must not resolve");
        assert_eq!(table.live().iter().filter(|entry| entry.runtime_hash == e.runtime_hash).count(), 1);

        // The activation floor is a couple of modest real jobs, not a 35B-sized window.
        let floor = palw_devnet_floor_job_vlt(VLT_MICRO as u64, 8 * VLT_MICRO as u64);
        assert_eq!(floor, 72 * VLT_MICRO, "8 prefill + 8 decode under (a=1, b=8) is 72 VLT");
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
