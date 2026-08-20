//! ADR-0044 (FP-01): the free-prompt job, its execution commitment, and the quantized one-shot
//! receipt spend — the data layer of "the user's own inference becomes the consensus work."
//!
//! Three objects, one binding rule, one randomness rule:
//!
//! ```text
//! job          — what the USER chose to run: their tokens, under a registered class, from a bond
//! commitment   — what the executor CLAIMS that run produced: openable roots + the executed shape
//! spend        — one certified quantum of that claim, converted into one block, once
//! ```
//!
//! **The binding rule (total identity).** Every id here is `H(domain ‖ len ‖ canonical-borsh)` over
//! the WHOLE unsigned object. The submitted draft hashed a hand-picked subset of its own fields;
//! on this codebase "bound indirectly through another field" is the recurring audit defect
//! (P0-1's shape), so no field is exempt — a field outside identity is a field two objects can
//! differ in while claiming to be the same object. Signatures are witnesses, never identity
//! (ML-DSA-87 signatures are not guaranteed unique; ADR-0042 Decision 3c).
//!
//! **The randomness rule (ADR-0044 Decision 4/5, invariants F5/F6/F15).** Nothing in this module
//! derives randomness from the job, the output, or any executor-chosen field. The quantum ticket
//! consumes a BEACON — an attempt-class (algo 6) chain block, whose hash costs one inference per
//! re-roll — that does not exist yet when every field of the claim is fixed on chain. The draft's
//! two grinding surfaces (costless block-hash re-rolls, post-hoc `job_nonce` selection) are closed
//! by that ordering, not by a filter.
//!
//! What is deliberately NOT here: model / runtime / manifest identities. The job names its
//! `class_id` and the class registration (`palw_registry`) binds the rest; admission cross-checks
//! the job's `tokenizer_id` against the registered row. Carrying the same fact in two places is
//! how the two drift apart — ids are lookup keys, not passengers.

use crate::Hash64;
use crate::palw_attempt_v2::PALW_ATTEMPT_V2_L1_TAG_BYTES;
use crate::tx::TransactionOutpoint;
use blake2b_simd::Params;

/// Object version for every FP-V3 wire object. A different version is a different object family.
pub const PALW_FP_V3_VERSION: u16 = 3;

/// Width of the expanded spend L1 tag — the attempt lane's width, so the Layer-0 finalizer's call
/// shape is identical across both block kinds.
pub const PALW_FP_V3_L1_TAG_BYTES: usize = PALW_ATTEMPT_V2_L1_TAG_BYTES;

/// The only weight-bearing privacy mode at v1 (ADR-0044 Decision 8): the prompt token ids are
/// carried whole in the commitment transaction. Encrypted modes are a future ADR and are refused
/// here — a mode this module does not understand must not certify, because certification is a
/// promise the panel can replay from chain data alone.
pub const PALW_FP_PRIVACY_PUBLIC_DA: u8 = 1;

pub const PALW_FP_V3_DOMAIN_JOB_ID: &[u8] = b"misaka-palw/fp-v3/job-id/v1";
pub const PALW_FP_V3_DOMAIN_CLAIM_ID: &[u8] = b"misaka-palw/fp-v3/claim-id/v1";
pub const PALW_FP_V3_DOMAIN_QUANTUM_TICKET: &[u8] = b"misaka-palw/fp-v3/quantum-ticket/v1";
pub const PALW_FP_V3_DOMAIN_SPEND_ID: &[u8] = b"misaka-palw/fp-v3/spend-id/v1";
pub const PALW_FP_V3_DOMAIN_SPEND_L1_TAG: &[u8] = b"misaka-palw/fp-v3/spend-l1-tag/v1";
/// ML-DSA-87 signing context for the commitment envelope (audit P0-6: one context per family).
pub const PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT: &[u8] = b"misaka-palw/fp-v3/commitment-mldsa87/v1";
/// ML-DSA-87 signing context for the spend envelope — its own context, because a spend and a
/// commitment are different promises by the same key, and one context serving both is how a
/// signature crosses meanings.
pub const PALW_FP_V3_MLDSA87_SPEND_CONTEXT: &[u8] = b"misaka-palw/fp-v3/spend-mldsa87/v1";

/// Every domain this module keys, so a duplicate is a test failure rather than a silent collision.
pub const PALW_FP_V3_ALL_DOMAINS: &[&[u8]] = &[
    PALW_FP_V3_DOMAIN_JOB_ID,
    PALW_FP_V3_DOMAIN_CLAIM_ID,
    PALW_FP_V3_DOMAIN_QUANTUM_TICKET,
    PALW_FP_V3_DOMAIN_SPEND_ID,
    PALW_FP_V3_DOMAIN_SPEND_L1_TAG,
    PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT,
    PALW_FP_V3_MLDSA87_SPEND_CONTEXT,
];

fn keyed(domain: &[u8]) -> blake2b_simd::State {
    Params::new().hash_length(64).key(domain).to_state()
}

fn finish(state: blake2b_simd::State) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

fn canonical_id(domain: &[u8], object_bytes: &[u8]) -> Hash64 {
    let mut state = keyed(domain);
    state.update(&(object_bytes.len() as u64).to_le_bytes());
    state.update(object_bytes);
    finish(state)
}

/// The job a user's gateway submits: **their tokens, unmodified** (invariant F1). Chain freshness
/// is bound OUTSIDE the model's input — `anchor_block`/`anchor_daa`/`job_nonce` live in the
/// identity, never in the token stream — so PALW on or off, the model consumes byte-identical
/// input and the user's answer cannot depend on mining metadata. The legacy VLT executor's
/// DAA-suffix (`new_job_input`) must never be ported to this path.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwFreePromptJobV3 {
    pub version: u16,
    /// The network's domain separator — the same value the attempt lane binds, so a testnet job
    /// cannot certify on mainnet.
    pub network_domain: Hash64,
    /// The registered execution class. Model, runtime manifest, shape profile and artifact root
    /// resolve THROUGH the registry row; the job does not carry second copies of those facts.
    pub class_id: Hash64,
    pub executor_bond: TransactionOutpoint,
    /// MUST equal the bond record's key at admission. Carried so the signature is checkable
    /// before any chain lookup.
    pub executor_pubkey: Vec<u8>,
    /// Registered at bond time; the panel dedups on it.
    pub operator_id: Hash64,
    /// A recent chain block: freshness binding. Admission bounds how old it may be; an unbounded
    /// anchor lets a stockpile of pre-signed jobs certify long after their world changed.
    pub anchor_block: Hash64,
    pub anchor_daa: u64,
    /// Uniqueness only. This field carries **no lottery meaning** (invariant F6): nothing in the
    /// protocol derives randomness from it, which is exactly why grinding it buys nothing — the
    /// draw beacon postdates the claim it would try to aim at.
    pub job_nonce: [u8; 32],
    /// MUST equal the class row's `tokenizer_id`. Carried as a cross-check because a token-id
    /// sequence read under the wrong tokenizer is a different prompt with the same bytes.
    pub tokenizer_id: Hash64,
    pub prompt_token_ids_hash: Hash64,
    pub prompt_tokens: u32,
    /// Ceiling, not exact count: EOG is a legitimate stop for a user answer (Decision 7). The
    /// executed count lives in the commitment, and the CU rule prices what ran, not the ceiling.
    pub decode_token_limit: u32,
    pub max_context_tokens: u32,
    /// See [`PALW_FP_PRIVACY_PUBLIC_DA`].
    pub privacy_mode: u8,
}

/// `H(canonical(job))` — every field, no exceptions.
pub fn fp_job_id_v3(job: &PalwFreePromptJobV3) -> Hash64 {
    let bytes = borsh::to_vec(job).expect("PalwFreePromptJobV3 is borsh-serializable");
    canonical_id(PALW_FP_V3_DOMAIN_JOB_ID, &bytes)
}

/// Why the run stopped. Canonicalized, not descriptive: `decode_tokens_executed ==
/// decode_token_limit` MUST be `ExactBudgetReached`, and `EndOfGeneration` MUST come with
/// `executed < limit` — otherwise the same execution admits two encodings, and two encodings of
/// one fact are two claim ids for one claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwFpStopReasonV3 {
    ExactBudgetReached = 0,
    EndOfGeneration = 1,
}

/// What the executor claims the run produced. The claim id over this object is the receipt
/// identity for the rest of the receipt's life: the lattice keys claims by it, the ticket binds
/// it, the spend names it.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwFreePromptCommitmentV3 {
    pub job: PalwFreePromptJobV3,
    /// `full_logits_trace_root_v2`-shaped: openable, court-grade. A flat digest here would make
    /// every dispute `Unadjudicable`, which freezes the class (the `palw_block_commitment`
    /// lesson, kept).
    pub trace_root: Hash64,
    pub output_root: Hash64,
    pub schedule_root: Hash64,
    /// What actually ran (≤ the job's ceiling). CU prices this, not the ceiling.
    pub decode_tokens_executed: u32,
    pub stop_reason: PalwFpStopReasonV3,
    /// The executor's CU claim. **Checked, never trusted**: every validator recomputes
    /// [`fp_cu_v3`] from the committed shape and a mismatch is invalid — a claim, not an input.
    pub cu: u128,
    /// Data-availability obligation trio (ADR-0042 Decision 7 shape): the manifest the producer
    /// must serve, how many chunks stand behind it, and until when. Failing a request inside the
    /// window defaults the producer.
    pub trace_manifest_root: Hash64,
    pub trace_chunk_count: u32,
    pub trace_retention_daa: u64,
}

/// The signed envelope. The signature is a **witness**, never identity.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwFreePromptCommitmentEnvelopeV3 {
    pub commitment: PalwFreePromptCommitmentV3,
    pub signature: Vec<u8>,
}

/// `H(canonical(commitment))` — the claim id, and therefore the receipt id. Fixed on chain when
/// the commitment's transaction is accepted, which is strictly before any beacon that will ever
/// draw for it exists (invariants F4/F5).
pub fn fp_claim_id_v3(commitment: &PalwFreePromptCommitmentV3) -> Hash64 {
    let bytes = borsh::to_vec(commitment).expect("PalwFreePromptCommitmentV3 is borsh-serializable");
    canonical_id(PALW_FP_V3_DOMAIN_CLAIM_ID, &bytes)
}

/// The CU pricing weights (ADR-0044 Decision 7). Part of the consensus bundle — a different
/// price table is a different ruleset. The invariant the calibration harness must uphold when
/// choosing them: **no workload shape yields more CU per real second than the pure-decode
/// reference shape** — mispricing may only ever under-pay, so a shape-crafted job never beats
/// honest decode-heavy work per FLOP.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwFpCuWeightsV3 {
    pub prefill_weight: u32,
    pub decode_weight: u32,
}

/// `prompt·prefill_weight + executed·decode_weight`, in u128 — two u32×u32 products cannot
/// overflow it, so this is total with no saturation arm to mask a bad input.
pub fn fp_cu_v3(prompt_tokens: u32, decode_tokens_executed: u32, weights: &PalwFpCuWeightsV3) -> u128 {
    (prompt_tokens as u128) * (weights.prefill_weight as u128) + (decode_tokens_executed as u128) * (weights.decode_weight as u128)
}

/// How many uniform quanta a certified CU total yields: `min(⌊cu / quantum_cu⌋, cap)`.
///
/// Floor, deliberately: the sub-quantum remainder certifies (the audit still happened) but never
/// draws — a partial ticket would need a scaled target, and a scaled target re-opens the
/// variable-lottery arithmetic the quantization exists to delete. The cap bounds the per-receipt
/// jackpot (the draft's `MAX_PWU_PER_RECEIPT`, made a primitive).
pub fn fp_quanta_v3(cu: u128, quantum_cu: u128, max_quanta_per_receipt: u32) -> u32 {
    if quantum_cu == 0 {
        // A zero quantum is refused by the bundle's startup invariants; answering 0 here (never
        // "everything divides") keeps the function total without minting on a broken config.
        return 0;
    }
    let full = cu / quantum_cu;
    full.min(max_quanta_per_receipt as u128) as u32
}

/// The quantum's lottery draw: leading 128 bits (big-endian, matching `palw_ticket_v1`'s reading)
/// of `H(network ‖ beacon ‖ claim ‖ q)`.
///
/// Everything executor-chosen in this preimage (`claim_id`) is irrevocable on chain before
/// `beacon_block` exists, and the beacon is an attempt-class block whose every alternative sample
/// costs one inference (Decision 4). Compare against the class's receipt target with
/// [`crate::palw_pwu::palw_ticket_admits_v1`] — one ticket space, two lanes.
pub fn fp_quantum_ticket_v3(network_domain: Hash64, beacon_block: Hash64, claim_id: Hash64, quantum_index: u32) -> u128 {
    let mut state = keyed(PALW_FP_V3_DOMAIN_QUANTUM_TICKET);
    state.update(network_domain.as_byte_slice());
    state.update(beacon_block.as_byte_slice());
    state.update(claim_id.as_byte_slice());
    state.update(&quantum_index.to_le_bytes());
    let digest = finish(state);
    let mut lead = [0u8; 16];
    lead.copy_from_slice(&digest.as_bytes()[..16]);
    u128::from_be_bytes(lead)
}

/// The draw slot for a claim certified (`Final`) at `final_daa`: the beacon is the first
/// attempt-class chain block at or after `final_daa + receipt_maturity_daa`. `None` on overflow —
/// a slot past the DAA space draws never, not "wraps to soon".
pub fn fp_draw_slot_v3(final_daa: u64, receipt_maturity_daa: u64) -> Option<u64> {
    final_daa.checked_add(receipt_maturity_daa)
}

/// Whether a spending block at `block_daa` uses its win in time: inside
/// `[beacon_daa, beacon_daa + use_window]`, saturating at the DAA ceiling. A win outside the
/// window licenses nothing, forever (invariant F14) — a stockpiled win from the deep past is a
/// reorg lever, not a savings account.
pub fn fp_spend_window_contains_v3(beacon_daa: u64, receipt_use_window_daa: u64, block_daa: u64) -> bool {
    block_daa >= beacon_daa && block_daa <= beacon_daa.saturating_add(receipt_use_window_daa)
}

/// A checkable statement that `beacon_block` is the FIRST attempt-class (algo 6) chain block at
/// or after `slot` — `PalwAnchorFactV2`'s shape with the class filter made explicit:
/// `prev_attempt_daa` is the last attempt-class chain block BEFORE the slot, so "first at or
/// after" is a pair of inequalities rather than an assertion.
///
/// Receipt-class blocks between `prev_attempt_daa` and the beacon are structurally irrelevant —
/// their hashes are costlessly malleable by their producers and carry no randomness (invariant
/// F15), which is the entire reason this fact exists. The PIPELINE that constructs the fact
/// attests the two block-identity claims (that the named blocks are chain blocks of the named
/// classes at the named scores) from its own candidate chain; this validation checks the
/// ordering those claims must satisfy, so a fact from a different slot cannot be replayed here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwBeaconFactV3 {
    pub beacon_block: Hash64,
    pub beacon_daa: u64,
    /// DAA score of the last attempt-class chain block strictly before the slot. Genesis-shaped
    /// networks with no prior attempt block use 0 — the slot of the first real claim is always
    /// past genesis.
    pub prev_attempt_daa: u64,
}

pub fn validate_beacon_fact_v3(slot: u64, fact: &PalwBeaconFactV3) -> Result<(), PalwFpV3Error> {
    if fact.beacon_daa < slot {
        return Err(PalwFpV3Error::BeaconBeforeSlot { beacon_daa: fact.beacon_daa, slot });
    }
    if fact.prev_attempt_daa >= slot {
        return Err(PalwFpV3Error::BeaconNotFirst { prev_attempt_daa: fact.prev_attempt_daa, slot });
    }
    Ok(())
}

/// One certified quantum, spent into one block. The producer is the claim's executor — receipts
/// do not transfer (pooling happens at the gateway/bond layer, where the bond owner is the
/// accountable party).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwReceiptSpendUnsignedV3 {
    pub version: u16,
    pub network_domain: Hash64,
    /// The certified commitment being spent — [`fp_claim_id_v3`] of a claim in `Final`.
    pub claim_id: Hash64,
    pub quantum_index: u32,
    /// The draw this spend claims: MUST be the claim's slot beacon on the candidate chain
    /// (checked statefully against a [`PalwBeaconFactV3`]). Carried so the ticket is recomputable
    /// with zero lookups.
    pub beacon_block: Hash64,
    /// MUST equal the claim's executor bond (stateful item 6) — carried so the signature is
    /// checkable statelessly first.
    pub producer_bond: TransactionOutpoint,
    pub producer_pubkey: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwReceiptSpendEnvelopeV3 {
    pub spend: PalwReceiptSpendUnsignedV3,
    pub signature: Vec<u8>,
}

/// `H(canonical(spend))` — what the receipt block's PoW tag expands (the finalizer consumes
/// `Expand(spend_id)`), so a receipt block's identity is total over its spend and the header
/// cannot swap claims on a fixed digest.
pub fn fp_spend_id_v3(spend: &PalwReceiptSpendUnsignedV3) -> Hash64 {
    let bytes = borsh::to_vec(spend).expect("PalwReceiptSpendUnsignedV3 is borsh-serializable");
    canonical_id(PALW_FP_V3_DOMAIN_SPEND_ID, &bytes)
}

/// `Expand(spend_id)` — the attempt lane's `l1_tag_v2` shape under this family's own domain.
/// Same width, same finalizer call shape, different key: a spend tag can never be mistaken for
/// an attempt tag over equal bytes.
pub fn fp_spend_l1_tag_v3(spend_id: Hash64) -> [u8; PALW_FP_V3_L1_TAG_BYTES] {
    let mut out = [0u8; PALW_FP_V3_L1_TAG_BYTES];
    for (chunk_index, chunk) in out.chunks_mut(64).enumerate() {
        let mut state = keyed(PALW_FP_V3_DOMAIN_SPEND_L1_TAG);
        state.update(spend_id.as_byte_slice());
        state.update(&(chunk_index as u32).to_le_bytes());
        chunk.copy_from_slice(&state.finalize().as_bytes()[..chunk.len()]);
    }
    out
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwFpV3Error {
    #[error("unsupported FP-V3 object version {got} (expected {expected})")]
    UnsupportedVersion { got: u16, expected: u16 },
    #[error("the object's network domain is not this network's")]
    NetworkDomainMismatch,
    #[error("privacy mode {0} is not weight-bearing (v1 accepts only PublicDa)")]
    UnsupportedPrivacyMode(u8),
    #[error("the executor/producer public key is empty")]
    MissingPublicKey,
    #[error("the signature is {got} bytes, not the ML-DSA-87 {expected}")]
    SignatureLength { got: usize, expected: usize },
    #[error("the signature does not verify over the object id under the carried key")]
    SignatureInvalid,
    #[error("prompt_tokens is zero — an empty prompt prices nothing and answers nothing")]
    EmptyPrompt,
    #[error("decode_token_limit is zero — a job that may not decode is not a job")]
    ZeroDecodeLimit,
    #[error("prompt ({prompt}) + decode limit ({limit}) exceeds max_context_tokens ({max_context})")]
    ContextOverflow { prompt: u32, limit: u32, max_context: u32 },
    #[error("decode_tokens_executed is zero — a run that emitted nothing committed nothing")]
    ZeroDecodeExecuted,
    #[error("decode_tokens_executed ({executed}) exceeds the job's ceiling ({limit})")]
    DecodeOverrun { executed: u32, limit: u32 },
    #[error("stop reason is not canonical for the executed count (executed {executed}, limit {limit})")]
    NonCanonicalStopReason { executed: u32, limit: u32 },
    #[error("the carried cu ({claimed}) is not the bundle rule's derivation ({derived})")]
    CuMismatch { claimed: u128, derived: u128 },
    #[error("trace_chunk_count is zero — a trace nobody can fetch is a trace nobody can verify")]
    ZeroTraceChunks,
    #[error("beacon at daa {beacon_daa} sits before the draw slot {slot}")]
    BeaconBeforeSlot { beacon_daa: u64, slot: u64 },
    #[error("an attempt-class block at daa {prev_attempt_daa} already occupies the slot {slot} — the named beacon is not the first")]
    BeaconNotFirst { prev_attempt_daa: u64, slot: u64 },
}

impl PalwFreePromptCommitmentEnvelopeV3 {
    /// Stateless admission: everything checkable without chain state, in refusal-first order.
    /// The CU claim is recomputed under the bundle's weights and a mismatch is named — never
    /// clamped, never trusted (invariant F7).
    pub fn validate_stateless_v3(&self, network_domain: Hash64, weights: &PalwFpCuWeightsV3) -> Result<(), PalwFpV3Error> {
        let c = &self.commitment;
        let job = &c.job;
        if job.version != PALW_FP_V3_VERSION {
            return Err(PalwFpV3Error::UnsupportedVersion { got: job.version, expected: PALW_FP_V3_VERSION });
        }
        if job.network_domain != network_domain {
            return Err(PalwFpV3Error::NetworkDomainMismatch);
        }
        if job.privacy_mode != PALW_FP_PRIVACY_PUBLIC_DA {
            return Err(PalwFpV3Error::UnsupportedPrivacyMode(job.privacy_mode));
        }
        if job.executor_pubkey.is_empty() {
            return Err(PalwFpV3Error::MissingPublicKey);
        }
        let expected = crate::dns_finality::STAKE_ATTESTATION_SIG_LEN;
        if self.signature.len() != expected {
            return Err(PalwFpV3Error::SignatureLength { got: self.signature.len(), expected });
        }
        if job.prompt_tokens == 0 {
            return Err(PalwFpV3Error::EmptyPrompt);
        }
        if job.decode_token_limit == 0 {
            return Err(PalwFpV3Error::ZeroDecodeLimit);
        }
        let budget = (job.prompt_tokens as u64) + (job.decode_token_limit as u64);
        if budget > job.max_context_tokens as u64 {
            return Err(PalwFpV3Error::ContextOverflow {
                prompt: job.prompt_tokens,
                limit: job.decode_token_limit,
                max_context: job.max_context_tokens,
            });
        }
        if c.decode_tokens_executed == 0 {
            return Err(PalwFpV3Error::ZeroDecodeExecuted);
        }
        if c.decode_tokens_executed > job.decode_token_limit {
            return Err(PalwFpV3Error::DecodeOverrun { executed: c.decode_tokens_executed, limit: job.decode_token_limit });
        }
        // Canonical stop reason: exactly one encoding per executed count (see the enum's doc).
        let canonical = match c.stop_reason {
            PalwFpStopReasonV3::ExactBudgetReached => c.decode_tokens_executed == job.decode_token_limit,
            PalwFpStopReasonV3::EndOfGeneration => c.decode_tokens_executed < job.decode_token_limit,
        };
        if !canonical {
            return Err(PalwFpV3Error::NonCanonicalStopReason { executed: c.decode_tokens_executed, limit: job.decode_token_limit });
        }
        let derived = fp_cu_v3(job.prompt_tokens, c.decode_tokens_executed, weights);
        if c.cu != derived {
            return Err(PalwFpV3Error::CuMismatch { claimed: c.cu, derived });
        }
        if c.trace_chunk_count == 0 {
            return Err(PalwFpV3Error::ZeroTraceChunks);
        }
        Ok(())
    }

    /// The signature must verify over [`fp_claim_id_v3`] under the **carried** key, in this
    /// family's own context. Whether the carried key IS the named bond's key is the stateful
    /// side's job, against the candidate-chain bond record. The verifier is passed in because
    /// this crate holds no ML-DSA implementation; the context is not — no caller may supply a
    /// foreign domain.
    pub fn validate_signature_v3<V>(&self, verify_mldsa87: V) -> Result<(), PalwFpV3Error>
    where
        V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    {
        let message = fp_claim_id_v3(&self.commitment);
        if !verify_mldsa87(
            &self.commitment.job.executor_pubkey,
            message.as_byte_slice(),
            &self.signature,
            PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT,
        ) {
            return Err(PalwFpV3Error::SignatureInvalid);
        }
        Ok(())
    }
}

impl PalwReceiptSpendEnvelopeV3 {
    /// Stateless admission for the spend riding a receipt block's header extension.
    pub fn validate_stateless_v3(&self, network_domain: Hash64) -> Result<(), PalwFpV3Error> {
        let s = &self.spend;
        if s.version != PALW_FP_V3_VERSION {
            return Err(PalwFpV3Error::UnsupportedVersion { got: s.version, expected: PALW_FP_V3_VERSION });
        }
        if s.network_domain != network_domain {
            return Err(PalwFpV3Error::NetworkDomainMismatch);
        }
        if s.producer_pubkey.is_empty() {
            return Err(PalwFpV3Error::MissingPublicKey);
        }
        let expected = crate::dns_finality::STAKE_ATTESTATION_SIG_LEN;
        if self.signature.len() != expected {
            return Err(PalwFpV3Error::SignatureLength { got: self.signature.len(), expected });
        }
        Ok(())
    }

    /// Signature over [`fp_spend_id_v3`] under the carried producer key, spend context.
    pub fn validate_signature_v3<V>(&self, verify_mldsa87: V) -> Result<(), PalwFpV3Error>
    where
        V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    {
        let message = fp_spend_id_v3(&self.spend);
        if !verify_mldsa87(&self.spend.producer_pubkey, message.as_byte_slice(), &self.signature, PALW_FP_V3_MLDSA87_SPEND_CONTEXT) {
            return Err(PalwFpV3Error::SignatureInvalid);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_pwu::palw_ticket_admits_v1;

    fn op(seed: u8) -> TransactionOutpoint {
        TransactionOutpoint::new(Hash64::from_bytes([seed; 64]), 0)
    }

    fn net() -> Hash64 {
        Hash64::from_u64_word(0x4E45_5457)
    }

    fn weights() -> PalwFpCuWeightsV3 {
        PalwFpCuWeightsV3 { prefill_weight: 1, decode_weight: 64 }
    }

    fn job() -> PalwFreePromptJobV3 {
        PalwFreePromptJobV3 {
            version: PALW_FP_V3_VERSION,
            network_domain: net(),
            class_id: Hash64::from_u64_word(0xC1),
            executor_bond: op(1),
            executor_pubkey: vec![7u8; 32],
            operator_id: Hash64::from_u64_word(0xE0),
            anchor_block: Hash64::from_u64_word(0xA0),
            anchor_daa: 5_000,
            job_nonce: [0x11; 32],
            tokenizer_id: Hash64::from_u64_word(0x70),
            prompt_token_ids_hash: Hash64::from_u64_word(0x9012),
            prompt_tokens: 96,
            decode_token_limit: 128,
            max_context_tokens: 4096,
            privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        }
    }

    fn commitment() -> PalwFreePromptCommitmentV3 {
        let job = job();
        let cu = fp_cu_v3(job.prompt_tokens, 77, &weights());
        PalwFreePromptCommitmentV3 {
            job,
            trace_root: Hash64::from_u64_word(0x7A),
            output_root: Hash64::from_u64_word(0x00),
            schedule_root: Hash64::from_u64_word(0x5C),
            decode_tokens_executed: 77,
            stop_reason: PalwFpStopReasonV3::EndOfGeneration,
            cu,
            trace_manifest_root: Hash64::from_u64_word(0xD0),
            trace_chunk_count: 8,
            trace_retention_daa: 999_999,
        }
    }

    fn sig() -> Vec<u8> {
        vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN]
    }

    fn spend() -> PalwReceiptSpendUnsignedV3 {
        PalwReceiptSpendUnsignedV3 {
            version: PALW_FP_V3_VERSION,
            network_domain: net(),
            claim_id: fp_claim_id_v3(&commitment()),
            quantum_index: 2,
            beacon_block: Hash64::from_u64_word(0xBEAC),
            producer_bond: op(1),
            producer_pubkey: vec![7u8; 32],
        }
    }

    /// **Golden vectors** — the canonical layouts, frozen. A borsh reordering, a domain edit or a
    /// field addition moves these bytes, and moving them is a NEW object family (a new domain
    /// suffix), never an in-place edit.
    #[test]
    fn golden_vector_ids_are_frozen() {
        let job_id = fp_job_id_v3(&job());
        let claim_id = fp_claim_id_v3(&commitment());
        let spend_id = fp_spend_id_v3(&spend());
        let ticket = fp_quantum_ticket_v3(net(), Hash64::from_u64_word(0xBEAC), claim_id, 2);

        assert_eq!(&faster_hex::hex_string(job_id.as_byte_slice())[..32], "d1ef7bce23d0edcc1a409b111d865c2f");
        assert_eq!(&faster_hex::hex_string(claim_id.as_byte_slice())[..32], "a16aaed813fda6b2c6d991253c089d4c");
        assert_eq!(&faster_hex::hex_string(spend_id.as_byte_slice())[..32], "dd0ab435ff1a3354fb3ddb96d3b878d0");
        assert_eq!(format!("{ticket:032x}"), "d9c4d8515a1466de666e87669235caec");

        let tag = fp_spend_l1_tag_v3(spend_id);
        assert_eq!(tag.len(), PALW_FP_V3_L1_TAG_BYTES);
        assert_ne!(&tag[..64], &[0u8; 64][..], "the expansion is not degenerate");
        assert_eq!(&faster_hex::hex_string(&tag[..8]), "50ee7499628c2709");
    }

    /// Every field of the JOB is identity (total binding). The submitted draft hand-picked its
    /// hash fields and left four of its own struct fields outside; this test is the rule that
    /// refuses that class of design here.
    #[test]
    fn every_job_field_moves_the_job_id() {
        let base = job();
        let baseline = fp_job_id_v3(&base);
        let mutations: Vec<(&str, fn(&mut PalwFreePromptJobV3))> = vec![
            ("version", |j| j.version += 1),
            ("network_domain", |j| j.network_domain = Hash64::from_u64_word(0x99)),
            ("class_id", |j| j.class_id = Hash64::from_u64_word(0xC2)),
            ("executor_bond", |j| j.executor_bond.index += 1),
            ("executor_pubkey", |j| j.executor_pubkey = vec![9u8; 32]),
            ("operator_id", |j| j.operator_id = Hash64::from_u64_word(0xE1)),
            ("anchor_block", |j| j.anchor_block = Hash64::from_u64_word(0xA1)),
            ("anchor_daa", |j| j.anchor_daa += 1),
            ("job_nonce", |j| j.job_nonce[0] ^= 1),
            ("tokenizer_id", |j| j.tokenizer_id = Hash64::from_u64_word(0x71)),
            ("prompt_token_ids_hash", |j| j.prompt_token_ids_hash = Hash64::from_u64_word(0x9013)),
            ("prompt_tokens", |j| j.prompt_tokens += 1),
            ("decode_token_limit", |j| j.decode_token_limit += 1),
            ("max_context_tokens", |j| j.max_context_tokens += 1),
            ("privacy_mode", |j| j.privacy_mode += 1),
        ];
        for (field, mutate) in mutations {
            let mut m = base.clone();
            mutate(&mut m);
            assert_ne!(fp_job_id_v3(&m), baseline, "mutating {field} left the job id unchanged");
        }
    }

    /// Every field of the COMMITMENT is identity, including the nested job and the DA trio, and
    /// the signature is not (a second valid signature is not a second receipt).
    #[test]
    fn every_commitment_field_moves_the_claim_id_and_the_signature_does_not() {
        let base = commitment();
        let baseline = fp_claim_id_v3(&base);
        let mutations: Vec<(&str, fn(&mut PalwFreePromptCommitmentV3))> = vec![
            ("job (nested)", |c| c.job.job_nonce[0] ^= 1),
            ("trace_root", |c| c.trace_root = Hash64::from_u64_word(0xDEAD)),
            ("output_root", |c| c.output_root = Hash64::from_u64_word(0xBEEF)),
            ("schedule_root", |c| c.schedule_root = Hash64::from_u64_word(0x5D)),
            ("decode_tokens_executed", |c| c.decode_tokens_executed += 1),
            ("stop_reason", |c| c.stop_reason = PalwFpStopReasonV3::ExactBudgetReached),
            ("cu", |c| c.cu += 1),
            ("trace_manifest_root", |c| c.trace_manifest_root = Hash64::from_u64_word(0xD1)),
            ("trace_chunk_count", |c| c.trace_chunk_count += 1),
            ("trace_retention_daa", |c| c.trace_retention_daa += 1),
        ];
        for (field, mutate) in mutations {
            let mut m = base.clone();
            mutate(&mut m);
            assert_ne!(fp_claim_id_v3(&m), baseline, "mutating {field} left the claim id unchanged");
        }

        let one = PalwFreePromptCommitmentEnvelopeV3 { commitment: base.clone(), signature: sig() };
        let two = PalwFreePromptCommitmentEnvelopeV3 { commitment: base, signature: vec![0xA5; sig().len()] };
        assert_ne!(one.signature, two.signature);
        assert_eq!(fp_claim_id_v3(&one.commitment), fp_claim_id_v3(&two.commitment), "the signature must not reach identity");
    }

    /// Every field of the SPEND is identity, and the L1 tag follows the spend id — a header
    /// cannot swap claims, quanta or beacons on a fixed digest.
    #[test]
    fn every_spend_field_moves_the_spend_id_and_the_pow_tag() {
        let base = spend();
        let baseline = fp_spend_id_v3(&base);
        let base_tag = fp_spend_l1_tag_v3(baseline);
        let mutations: Vec<(&str, fn(&mut PalwReceiptSpendUnsignedV3))> = vec![
            ("version", |s| s.version += 1),
            ("network_domain", |s| s.network_domain = Hash64::from_u64_word(0x99)),
            ("claim_id", |s| s.claim_id = Hash64::from_u64_word(0xF00)),
            ("quantum_index", |s| s.quantum_index += 1),
            ("beacon_block", |s| s.beacon_block = Hash64::from_u64_word(0xBEAD)),
            ("producer_bond", |s| s.producer_bond.index += 1),
            ("producer_pubkey", |s| s.producer_pubkey = vec![9u8; 32]),
        ];
        for (field, mutate) in mutations {
            let mut m = base.clone();
            mutate(&mut m);
            let id = fp_spend_id_v3(&m);
            assert_ne!(id, baseline, "mutating {field} left the spend id unchanged");
            assert_ne!(fp_spend_l1_tag_v3(id)[..], base_tag[..], "mutating {field} left the PoW tag unchanged");
        }
    }

    /// The CU rule prices the EXECUTED shape and refuses every non-canonical encoding: an
    /// inflated claim, an overrun, a zero run, and both wrong stop-reason arms.
    #[test]
    fn cu_is_recomputed_and_stop_reasons_are_canonical() {
        let w = weights();
        assert_eq!(fp_cu_v3(96, 77, &w), 96 + 77 * 64);
        assert_eq!(fp_cu_v3(0, 0, &w), 0);
        // u32::MAX on both axes stays inside u128 — total, no saturation arm.
        assert_eq!(
            fp_cu_v3(u32::MAX, u32::MAX, &PalwFpCuWeightsV3 { prefill_weight: u32::MAX, decode_weight: u32::MAX }),
            2 * (u32::MAX as u128) * (u32::MAX as u128)
        );

        let ok = PalwFreePromptCommitmentEnvelopeV3 { commitment: commitment(), signature: sig() };
        assert_eq!(ok.validate_stateless_v3(net(), &w), Ok(()));

        let mut inflated = commitment();
        inflated.cu += 1;
        let e = PalwFreePromptCommitmentEnvelopeV3 { commitment: inflated, signature: sig() };
        assert!(matches!(e.validate_stateless_v3(net(), &w), Err(PalwFpV3Error::CuMismatch { .. })));

        let mut overrun = commitment();
        overrun.decode_tokens_executed = overrun.job.decode_token_limit + 1;
        overrun.cu = fp_cu_v3(overrun.job.prompt_tokens, overrun.decode_tokens_executed, &w);
        let e = PalwFreePromptCommitmentEnvelopeV3 { commitment: overrun, signature: sig() };
        assert!(matches!(e.validate_stateless_v3(net(), &w), Err(PalwFpV3Error::DecodeOverrun { .. })));

        // EOG exactly at the limit is the budget stop wearing the wrong name…
        let mut eog_at_limit = commitment();
        eog_at_limit.decode_tokens_executed = eog_at_limit.job.decode_token_limit;
        eog_at_limit.cu = fp_cu_v3(eog_at_limit.job.prompt_tokens, eog_at_limit.decode_tokens_executed, &w);
        let e = PalwFreePromptCommitmentEnvelopeV3 { commitment: eog_at_limit, signature: sig() };
        assert!(matches!(e.validate_stateless_v3(net(), &w), Err(PalwFpV3Error::NonCanonicalStopReason { .. })));

        // …and the budget stop below the limit claims a budget that did not end.
        let mut budget_below = commitment();
        budget_below.stop_reason = PalwFpStopReasonV3::ExactBudgetReached;
        let e = PalwFreePromptCommitmentEnvelopeV3 { commitment: budget_below, signature: sig() };
        assert!(matches!(e.validate_stateless_v3(net(), &w), Err(PalwFpV3Error::NonCanonicalStopReason { .. })));

        let mut silent = commitment();
        silent.decode_tokens_executed = 0;
        silent.cu = fp_cu_v3(silent.job.prompt_tokens, 0, &w);
        let e = PalwFreePromptCommitmentEnvelopeV3 { commitment: silent, signature: sig() };
        assert!(matches!(e.validate_stateless_v3(net(), &w), Err(PalwFpV3Error::ZeroDecodeExecuted)));
    }

    /// Stateless refusals name their reasons: version, network, privacy mode, empty prompt,
    /// zero decode ceiling, context overflow, chunkless trace, key/signature shapes.
    #[test]
    fn stateless_refusals_are_named() {
        let w = weights();
        let make = |mutate: fn(&mut PalwFreePromptCommitmentV3)| {
            let mut c = commitment();
            mutate(&mut c);
            // Keep the CU claim consistent so the refusal under test is the one named.
            c.cu = fp_cu_v3(c.job.prompt_tokens, c.decode_tokens_executed, &weights());
            PalwFreePromptCommitmentEnvelopeV3 { commitment: c, signature: sig() }
        };

        assert!(matches!(
            make(|c| c.job.version = 2).validate_stateless_v3(net(), &w),
            Err(PalwFpV3Error::UnsupportedVersion { got: 2, expected: 3 })
        ));
        assert_eq!(
            make(|c| c.job.network_domain = Hash64::from_u64_word(0x99)).validate_stateless_v3(net(), &w),
            Err(PalwFpV3Error::NetworkDomainMismatch)
        );
        assert_eq!(
            make(|c| c.job.privacy_mode = 2).validate_stateless_v3(net(), &w),
            Err(PalwFpV3Error::UnsupportedPrivacyMode(2)),
            "an unknown privacy mode must not certify — a panel cannot replay what it cannot read"
        );
        assert_eq!(make(|c| c.job.executor_pubkey = vec![]).validate_stateless_v3(net(), &w), Err(PalwFpV3Error::MissingPublicKey));
        assert_eq!(make(|c| c.job.prompt_tokens = 0).validate_stateless_v3(net(), &w), Err(PalwFpV3Error::EmptyPrompt));
        assert_eq!(make(|c| c.job.decode_token_limit = 0).validate_stateless_v3(net(), &w), Err(PalwFpV3Error::ZeroDecodeLimit));
        assert!(matches!(
            make(|c| c.job.max_context_tokens = 100).validate_stateless_v3(net(), &w),
            Err(PalwFpV3Error::ContextOverflow { .. })
        ));
        assert_eq!(make(|c| c.trace_chunk_count = 0).validate_stateless_v3(net(), &w), Err(PalwFpV3Error::ZeroTraceChunks));

        let mut short = PalwFreePromptCommitmentEnvelopeV3 { commitment: commitment(), signature: sig() };
        short.signature.pop();
        assert!(matches!(short.validate_stateless_v3(net(), &w), Err(PalwFpV3Error::SignatureLength { .. })));

        let spend_ok = PalwReceiptSpendEnvelopeV3 { spend: spend(), signature: sig() };
        assert_eq!(spend_ok.validate_stateless_v3(net()), Ok(()));
        let mut foreign = spend();
        foreign.network_domain = Hash64::from_u64_word(0x99);
        let e = PalwReceiptSpendEnvelopeV3 { spend: foreign, signature: sig() };
        assert_eq!(e.validate_stateless_v3(net()), Err(PalwFpV3Error::NetworkDomainMismatch));
    }

    /// Quantization: floor + cap, and the degenerate-config arm mints nothing.
    #[test]
    fn quanta_floor_and_cap() {
        assert_eq!(fp_quanta_v3(0, 1_000, 64), 0, "sub-quantum work certifies but never draws");
        assert_eq!(fp_quanta_v3(999, 1_000, 64), 0);
        assert_eq!(fp_quanta_v3(1_000, 1_000, 64), 1);
        assert_eq!(fp_quanta_v3(6_500, 1_000, 64), 6, "the remainder is floored, never rounded into a draw");
        assert_eq!(fp_quanta_v3(1_000_000, 1_000, 64), 64, "the cap bounds the per-receipt jackpot");
        assert_eq!(fp_quanta_v3(u128::MAX, 1, u32::MAX), u32::MAX);
        assert_eq!(fp_quanta_v3(1_000_000, 0, 64), 0, "a broken quantum config mints nothing, not everything");
    }

    /// The ticket is a pure function of (network, beacon, claim, q) — each input moves it, and
    /// admission runs through the SAME `palw_ticket_admits_v1` the attempt lane uses: one ticket
    /// space, two lanes, no second comparison rule to drift.
    #[test]
    fn tickets_are_beacon_and_claim_bound_and_share_the_admission_rule() {
        let claim = fp_claim_id_v3(&commitment());
        let beacon = Hash64::from_u64_word(0xBEAC);
        let base = fp_quantum_ticket_v3(net(), beacon, claim, 0);
        assert_eq!(base, fp_quantum_ticket_v3(net(), beacon, claim, 0), "deterministic");
        assert_ne!(fp_quantum_ticket_v3(Hash64::from_u64_word(0x99), beacon, claim, 0), base, "network");
        assert_ne!(fp_quantum_ticket_v3(net(), Hash64::from_u64_word(0xBEAD), claim, 0), base, "beacon");
        assert_ne!(fp_quantum_ticket_v3(net(), beacon, Hash64::from_u64_word(0xF00), 0), base, "claim");
        assert_ne!(fp_quantum_ticket_v3(net(), beacon, claim, 1), base, "quantum index");

        assert!(palw_ticket_admits_v1(base, u128::MAX), "the full target admits everything");
        assert!(!palw_ticket_admits_v1(base, 0) || base == 0, "a zero target admits (essentially) nothing");
        assert_eq!(palw_ticket_admits_v1(base, base), true, "the boundary is inclusive, as the attempt lane's is");
    }

    /// The beacon fact is a pair of inequalities, both live: the beacon sits at or after the
    /// slot, and no attempt-class block does so earlier.
    #[test]
    fn beacon_fact_boundaries() {
        let fact = |beacon_daa, prev| PalwBeaconFactV3 { beacon_block: Hash64::from_u64_word(0xB), beacon_daa, prev_attempt_daa: prev };
        assert_eq!(validate_beacon_fact_v3(100, &fact(100, 99)), Ok(()), "at the slot, predecessor strictly before");
        assert_eq!(validate_beacon_fact_v3(100, &fact(140, 0)), Ok(()), "after the slot is first if nothing sat between");
        assert!(matches!(validate_beacon_fact_v3(100, &fact(99, 0)), Err(PalwFpV3Error::BeaconBeforeSlot { .. })));
        assert!(
            matches!(validate_beacon_fact_v3(100, &fact(140, 100)), Err(PalwFpV3Error::BeaconNotFirst { .. })),
            "an attempt block AT the slot means the named later beacon is not the first"
        );
    }

    /// Draw slot and use window arithmetic is checked/saturating at the DAA edges: overflow draws
    /// never, and the window's inclusive ends are exact.
    #[test]
    fn draw_slot_and_use_window_edges() {
        assert_eq!(fp_draw_slot_v3(1_000, 50), Some(1_050));
        assert_eq!(fp_draw_slot_v3(u64::MAX, 1), None, "past the DAA space is never, not soon");
        assert!(fp_spend_window_contains_v3(1_050, 100, 1_050), "the beacon's own score is inside");
        assert!(fp_spend_window_contains_v3(1_050, 100, 1_150), "the far end is inclusive");
        assert!(!fp_spend_window_contains_v3(1_050, 100, 1_151), "one past the window licenses nothing");
        assert!(!fp_spend_window_contains_v3(1_050, 100, 1_049), "before the beacon is not a use of it");
        assert!(fp_spend_window_contains_v3(u64::MAX - 1, u64::MAX, u64::MAX), "saturation keeps the ceiling inside");
    }

    /// This module's domains are distinct among themselves AND against every neighboring PALW
    /// family it composes with — one preimage, one meaning, across module boundaries too.
    #[test]
    fn fp_v3_domains_are_distinct_across_families() {
        let mut all: Vec<&[u8]> = Vec::new();
        all.extend_from_slice(PALW_FP_V3_ALL_DOMAINS);
        all.extend_from_slice(crate::palw_attempt_v2::PALW_ATTEMPT_V2_ALL_DOMAINS);
        all.extend_from_slice(crate::palw_v2::PALW_V2_ALL_DOMAINS);
        all.extend_from_slice(crate::palw_mode_v2::PALW_MODE_V2_ALL_DOMAINS);
        let before = all.len();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), before, "an FP-V3 domain collides with a neighboring family's");
    }

    /// Cross-family separation on equal preimages: the same borsh bytes under the FP job-id key,
    /// the FP claim-id key and the attempt-id key produce three different digests — a V2 object
    /// can never be replayed as an FP one by re-labeling.
    #[test]
    fn equal_bytes_under_different_family_keys_diverge() {
        let bytes = borsh::to_vec(&job()).unwrap();
        let as_job = canonical_id(PALW_FP_V3_DOMAIN_JOB_ID, &bytes);
        let as_claim = canonical_id(PALW_FP_V3_DOMAIN_CLAIM_ID, &bytes);
        let as_spend = canonical_id(PALW_FP_V3_DOMAIN_SPEND_ID, &bytes);
        assert_ne!(as_job, as_claim);
        assert_ne!(as_job, as_spend);
        assert_ne!(as_claim, as_spend);
    }
}
