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

// ---------------------------------------------------------------------
// Domain separators. Each is a distinct BLAKE2b key so a digest from one
// role can never be replayed as a digest from another.
// ---------------------------------------------------------------------

/// Keyed-BLAKE2b-512 domain for [`job_spec_id`] (`H(S_j)`).
pub const JOB_SPEC_ID_KEY: &[u8] = b"misaka-vlt-jobspec-v1";
/// Keyed-BLAKE2b-512 domain for [`compute_receipt_hash`] (`R_j`, §3.1 eq. 2).
pub const COMPUTE_RECEIPT_KEY: &[u8] = b"misaka-vlt-receipt-v1";
/// Keyed-BLAKE2b-256 domain for the executor's signed certificate message.
pub const COMPUTE_CERT_MESSAGE_DOMAIN: &[u8] = b"misaka-vlt-v1/compute-cert";
/// Keyed-BLAKE2b-256 domain for a verifier's signed verdict message.
pub const VERIFIER_VERDICT_MESSAGE_DOMAIN: &[u8] = b"misaka-vlt-v1/verifier-verdict";
/// Keyed-BLAKE2b-256 domain for a challenger's signed fraud-proof message.
pub const COMPUTE_CHALLENGE_MESSAGE_DOMAIN: &[u8] = b"misaka-vlt-v1/compute-challenge";
/// Keyed-BLAKE2b-512 domain for the §6 post-commit verifier sortition.
pub const VERIFIER_SORTITION_KEY: &[u8] = b"misaka-vlt-verifier-sortition-v1";

/// ML-DSA-87 signing context for an executor's compute-certificate signature.
pub const COMPUTE_CERT_MLDSA87_CONTEXT: &[u8] = b"misaka-vlt-v1/cert/mldsa87";
/// ML-DSA-87 signing context for a verifier's verdict signature.
pub const VERIFIER_VERDICT_MLDSA87_CONTEXT: &[u8] = b"misaka-vlt-v1/verdict/mldsa87";
/// ML-DSA-87 signing context for a challenger's fraud-proof signature.
pub const COMPUTE_CHALLENGE_MLDSA87_CONTEXT: &[u8] = b"misaka-vlt-v1/challenge/mldsa87";

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
    let mut hasher = Blake2bParams::new().hash_length(64).key(COMPUTE_RECEIPT_KEY).to_state();
    hasher.update(job_spec_id(spec).as_byte_slice());
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

/// One verifier's signed verdict, carried inside [`ComputeCertificatePayload`].
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
    pub spec: LlmJobSpec,
    pub receipt: ComputeReceipt,
    /// ML-DSA-87 signature over [`compute_certificate_message`] under
    /// [`COMPUTE_CERT_MLDSA87_CONTEXT`].
    pub executor_signature: Vec<u8>,
    /// Sortitioned verifiers' verdicts. Bounded by [`MAX_VERIFIER_ATTESTATIONS`].
    pub verifier_attestations: Vec<VerifierAttestation>,
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
    /// §7(c) Challenge に失敗した実行を正しいものとして claim — a party that lost a
    /// challenge re-asserted the same execution. Slashes the **claimant**.
    FailedChallenge = 3,
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
    pub kind: ComputeFraudKind,
    /// `validator_id` of the challenger.
    pub challenger_id: Hash64,
    /// The challenger's own bond. A challenger stakes its own collateral, which is
    /// what makes §7(c) — slashing a *failed* challenge — enforceable.
    pub challenger_bond_outpoint: TransactionOutpoint,
    /// The bond a successful challenge slashes. **Who** that is depends on [`Self::kind`]:
    /// the executor for [`ComputeFraudKind::ForgedReceipt`] / [`ComputeFraudKind::InvalidCertificate`],
    /// the contradicting verifier for [`ComputeFraudKind::ContradictoryVerification`], and the
    /// losing claimant for [`ComputeFraudKind::FailedChallenge`].
    ///
    /// Carried explicitly rather than derived, so the bond mutation is decidable from the
    /// challenge transaction alone — the same shape as
    /// [`crate::dns_finality::SlashingEvidencePayload::bond_outpoint`]. For
    /// `ContradictoryVerification` the stateless check pins it to the verdicts' own bond; for the
    /// other kinds, matching it against the named certificate's executor is a stateful
    /// block-validity rule, since it needs the certificate transaction.
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
pub fn verifier_verdict_message(
    network_id: &[u8],
    job_id: Hash64,
    executor_receipt_hash: Hash64,
    verdict: VerificationVerdict,
    replay_receipt_hash: Hash64,
    bond_outpoint: TransactionOutpoint,
) -> Hash {
    let mut hasher = Blake2bParams::new().hash_length(32).key(VERIFIER_VERDICT_MESSAGE_DOMAIN).to_state();
    hasher.update(network_id);
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

/// Digest a challenger signs for a fraud proof.
pub fn compute_challenge_message(
    network_id: &[u8],
    certificate_tx_id: TransactionId,
    job_id: Hash64,
    kind: ComputeFraudKind,
    replay_receipt_hash: Hash64,
    bond_outpoint: TransactionOutpoint,
) -> Hash {
    let mut hasher = Blake2bParams::new().hash_length(32).key(COMPUTE_CHALLENGE_MESSAGE_DOMAIN).to_state();
    hasher.update(network_id);
    hasher.update(certificate_tx_id.as_byte_slice());
    hasher.update(job_id.as_byte_slice());
    hasher.update(&[kind as u8]);
    hasher.update(replay_receipt_hash.as_byte_slice());
    hasher.update(bond_outpoint.transaction_id.as_byte_slice());
    hasher.update(&bond_outpoint.index.to_le_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash::from_bytes(out)
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
pub fn select_verifiers(job_id: Hash64, executor_id: Hash64, beacon: BlockHash, candidates: &[Hash64], k: usize) -> Vec<Hash64> {
    if k == 0 {
        return Vec::new();
    }
    let mut ticketed: Vec<(Hash64, Hash64)> = candidates
        .iter()
        .filter(|c| **c != executor_id)
        .map(|c| {
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
            rho_micro: 0,
            max_tokens: 0,
        }; MAX_MODEL_COST_ENTRIES],
    };

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
        verifier_committee_size: 3,
        min_verifier_confirmations: 2,
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
/// The relation is deliberately **refutation-dominant**: a single
/// [`VerificationVerdict::Refuted`] fails the job even if the confirmation count is
/// met. Under full replay a refutation means some verifier's independent execution
/// disagreed byte-for-byte, and there is no honest reading under which the same
/// fixed spec yields two different receipts — so the safe resolution is to mint
/// nothing and let the §7 challenge path decide who to slash.
///
/// A confirming verdict must also carry a `replay_receipt_hash` equal to the
/// executor's; a "confirmation" of a different hash is self-contradictory and is
/// treated as a refutation rather than silently ignored.
pub fn verify_compute_certificate(executor_receipt_hash: Hash64, verdicts: &[VerifierAttestation], min_confirmations: u8) -> bool {
    let mut confirmations = 0usize;
    for v in verdicts {
        match v.verdict {
            VerificationVerdict::Refuted => return false,
            VerificationVerdict::Confirmed => {
                if v.replay_receipt_hash != executor_receipt_hash {
                    // Claims to confirm, but reports a different replay result.
                    return false;
                }
                confirmations += 1;
            }
        }
    }
    confirmations >= min_confirmations as usize
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

/// Whether an epoch's signed weight reaches [`bft_quorum`].
///
/// `total_weight == 0` is always `false`: an epoch with no weight has not been
/// finalized by anybody, and returning `true` for it would let a network with no
/// verified compute finalize everything vacuously.
pub fn meets_bft_quorum(signed_weight: u128, total_weight: u128) -> bool {
    if total_weight == 0 {
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
        table.entries[0] =
            ModelCostEntry { model_weights_hash: h64(1), runtime_hash: h64(2), rho_micro: 1_000_000, max_tokens: 100_000 };
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

    #[test]
    fn verification_is_refutation_dominant() {
        let r = h64(9);
        let other = h64(10);
        // Threshold met, but one verifier refuted.
        let mixed = [
            verdict(1, VerificationVerdict::Confirmed, r),
            verdict(2, VerificationVerdict::Confirmed, r),
            verdict(3, VerificationVerdict::Refuted, other),
        ];
        assert!(!verify_compute_certificate(r, &mixed, 2), "a single refutation must fail the job");

        let clean = [verdict(1, VerificationVerdict::Confirmed, r), verdict(2, VerificationVerdict::Confirmed, r)];
        assert!(verify_compute_certificate(r, &clean, 2));
        assert!(!verify_compute_certificate(r, &clean[..1], 2), "below the confirmation threshold");

        // "Confirms" while reporting a different replay hash — self-contradictory.
        let liar = [verdict(1, VerificationVerdict::Confirmed, r), verdict(2, VerificationVerdict::Confirmed, other)];
        assert!(!verify_compute_certificate(r, &liar, 2));
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
        // The classic off-by-one: 2 of 3 must NOT be a quorum.
        assert_eq!(bft_quorum(3), 3);
        assert!(!meets_bft_quorum(2, 3));
        assert!(meets_bft_quorum(3, 3));

        assert_eq!(bft_quorum(99), 67);
        assert!(!meets_bft_quorum(66, 99));
        assert!(meets_bft_quorum(67, 99));

        // Zero total weight never finalizes, however much is claimed.
        assert!(!meets_bft_quorum(0, 0));
        assert!(!meets_bft_quorum(u128::MAX, 0));
        // An over-count cannot manufacture a quorum.
        assert!(meets_bft_quorum(u128::MAX, 99));
    }

    #[test]
    fn two_quorums_always_intersect() {
        // §8.1: |A| + |B| > W  =>  A and B share weight. Exhaustive over small W.
        for w in 1u128..=200 {
            let q = bft_quorum(w);
            assert!(q.saturating_mul(2) > w, "two quorums of {q} out of {w} could be disjoint");
        }
    }

    #[test]
    fn verifier_sortition_excludes_the_executor_and_is_deterministic() {
        let candidates: Vec<Hash64> = (0u8..10).map(h64).collect();
        let executor = h64(3);
        let a = select_verifiers(h64(100), executor, BlockHash::from_bytes([1u8; 64]), &candidates, 3);
        assert_eq!(a.len(), 3);
        assert!(!a.contains(&executor), "§6: an executor must never verify its own job");

        // Same inputs => same committee, regardless of candidate ordering.
        let mut shuffled = candidates.clone();
        shuffled.reverse();
        let b = select_verifiers(h64(100), executor, BlockHash::from_bytes([1u8; 64]), &shuffled, 3);
        assert_eq!(a, b, "sortition must not depend on candidate order");

        // A different beacon draws a different committee (the executor cannot pre-pick).
        let c = select_verifiers(h64(100), executor, BlockHash::from_bytes([2u8; 64]), &candidates, 3);
        assert_ne!(a, c);
    }

    #[test]
    fn sortition_handles_undersized_candidate_sets() {
        let candidates = vec![h64(1), h64(2)];
        let picked = select_verifiers(h64(100), h64(1), BlockHash::from_bytes([1u8; 64]), &candidates, 3);
        assert_eq!(picked, vec![h64(2)], "only non-executor candidates are drawable");
        assert!(select_verifiers(h64(100), h64(1), BlockHash::from_bytes([1u8; 64]), &candidates, 0).is_empty());
    }

    #[test]
    fn digests_are_domain_separated() {
        let net = b"misaka-test";
        let s = spec();
        let jid = job_spec_id(&s);
        let rh = compute_receipt_hash(&s, &receipt(1, 1));
        let op = outpoint(1);

        let cert = compute_certificate_message(net, 5, jid, rh, op);
        let verd = verifier_verdict_message(net, jid, rh, VerificationVerdict::Confirmed, rh, op);
        let chal = compute_challenge_message(net, TransactionId::from_bytes([1u8; 64]), jid, ComputeFraudKind::ForgedReceipt, rh, op);
        assert_ne!(cert, verd);
        assert_ne!(cert, chal);
        assert_ne!(verd, chal);

        // A verifier's two possible verdicts are distinct messages, so a signature
        // over one is never a signature over the other.
        let refuted = verifier_verdict_message(net, jid, rh, VerificationVerdict::Refuted, rh, op);
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
