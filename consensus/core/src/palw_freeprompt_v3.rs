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
/// **v2, because the commitment gained `execution_root`.** The golden-vector rule this module
/// states — "a field addition moves these bytes, and moving them is a NEW object family (a new
/// domain suffix), never an in-place edit" — is followed rather than waived: a v1 claim id and a
/// v2 claim id are now different values for different objects, and no reader can mistake one
/// layout for the other. Nothing had persisted a v1 id (the whole lane is consensus-inert on
/// every shipped preset), so this costs nothing and keeps the rule intact for the time it will.
pub const PALW_FP_V3_DOMAIN_CLAIM_ID: &[u8] = b"misaka-palw/fp-v3/claim-id/v2";
pub const PALW_FP_V3_DOMAIN_QUANTUM_TICKET: &[u8] = b"misaka-palw/fp-v3/quantum-ticket/v1";
pub const PALW_FP_V3_DOMAIN_SPEND_ID: &[u8] = b"misaka-palw/fp-v3/spend-id/v1";
pub const PALW_FP_V3_DOMAIN_SPEND_L1_TAG: &[u8] = b"misaka-palw/fp-v3/spend-l1-tag/v1";
/// ML-DSA-87 signing context for the commitment envelope (audit P0-6: one context per family).
pub const PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT: &[u8] = b"misaka-palw/fp-v3/commitment-mldsa87/v1";
/// ML-DSA-87 signing context for the spend envelope — its own context, because a spend and a
/// commitment are different promises by the same key, and one context serving both is how a
/// signature crosses meanings.
pub const PALW_FP_V3_MLDSA87_SPEND_CONTEXT: &[u8] = b"misaka-palw/fp-v3/spend-mldsa87/v1";
/// Keyed hash over the raw worker-request frame — the wire-level echo the caller re-verifies.
pub const PALW_FP_V3_DOMAIN_WORKER_REQUEST: &[u8] = b"misaka-palw/fp-v3/worker-request/v1";
/// The spend's header-position binding (see [`spend_challenge_v3`]).
pub const PALW_FP_V3_DOMAIN_SPEND_CHALLENGE: &[u8] = b"misaka-palw/fp-v3/spend-challenge/v1";
/// The header-carriage wire magic for a spend envelope (see [`PalwReceiptSpendEnvelopeV3::encode`]).
pub const PALW_FP_V3_SPEND_CARRIAGE_MAGIC: [u8; 4] = *b"PFS3";
/// One retained-trace chunk's digest (see [`fp_trace_chunk_digest_v3`]).
pub const PALW_FP_V3_DOMAIN_TRACE_CHUNK: &[u8] = b"misaka-palw/fp-v3/trace-chunk/v1";
/// The retained-trace manifest root (see [`fp_trace_manifest_root_v3`]).
pub const PALW_FP_V3_DOMAIN_TRACE_MANIFEST: &[u8] = b"misaka-palw/fp-v3/trace-manifest/v1";

/// Every domain this module keys, so a duplicate is a test failure rather than a silent collision.
pub const PALW_FP_V3_ALL_DOMAINS: &[&[u8]] = &[
    PALW_FP_V3_DOMAIN_JOB_ID,
    PALW_FP_V3_DOMAIN_CLAIM_ID,
    PALW_FP_V3_DOMAIN_QUANTUM_TICKET,
    PALW_FP_V3_DOMAIN_SPEND_ID,
    PALW_FP_V3_DOMAIN_SPEND_L1_TAG,
    PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT,
    PALW_FP_V3_MLDSA87_SPEND_CONTEXT,
    PALW_FP_V3_DOMAIN_WORKER_REQUEST,
    PALW_FP_V3_DOMAIN_TRACE_CHUNK,
    PALW_FP_V3_DOMAIN_TRACE_MANIFEST,
    PALW_FP_V3_DOMAIN_SPEND_CHALLENGE,
];

// ---------------------------------------------------------------------------------------------
// Retained-trace data availability (the commitment's DA obligation trio, made honest)
// ---------------------------------------------------------------------------------------------

/// Events per retained-trace chunk. At the 4096-event trace cap this is at most 16 chunks of
/// 16 KiB — small enough to serve whole, large enough that a manifest is a page, not a book.
pub const PALW_FP_TRACE_CHUNK_EVENTS_V3: u32 = 256;

/// What the producer retains and serves: the ORDERED EVENT-HASH LIST, chunked. Deliberately not
/// the logits themselves — the execution is deterministic, so a replayer recomputes every row
/// from chain data; what the list adds is *localization*: a challenger comparing their replay's
/// event hashes against the served list finds the first diverging index without holding the
/// executor's machine, and `trace_event_opening_v2` proves any served event against the
/// committed trace root. (Step-level leg/checkpoint material for the arithmetic court is a
/// future retention profile; a receipt claim at drill stage is refuted by panel replay.)
///
/// `H(domain ‖ binding ‖ chunk_index ‖ count ‖ events…)` — index-bound so chunks cannot be
/// reordered, binding-bound so one job's chunks cannot serve another's manifest.
pub fn fp_trace_chunk_digest_v3(trace_binding: Hash64, chunk_index: u32, events: &[Hash64]) -> Hash64 {
    let mut state = keyed(PALW_FP_V3_DOMAIN_TRACE_CHUNK);
    state.update(trace_binding.as_byte_slice());
    state.update(&chunk_index.to_le_bytes());
    state.update(&(events.len() as u32).to_le_bytes());
    for event in events {
        state.update(event.as_byte_slice());
    }
    finish(state)
}

/// `H(domain ‖ binding ‖ chunk_size ‖ count ‖ digests…)` over the per-chunk digests, in order.
/// Flat, not a tree: at ≤16 chunks the manifest is served whole, and a flat list has no odd-node
/// arm to get wrong.
pub fn fp_trace_manifest_root_v3(trace_binding: Hash64, chunk_digests: &[Hash64]) -> Hash64 {
    let mut state = keyed(PALW_FP_V3_DOMAIN_TRACE_MANIFEST);
    state.update(trace_binding.as_byte_slice());
    state.update(&PALW_FP_TRACE_CHUNK_EVENTS_V3.to_le_bytes());
    state.update(&(chunk_digests.len() as u32).to_le_bytes());
    for digest in chunk_digests {
        state.update(digest.as_byte_slice());
    }
    finish(state)
}

/// Chunk an ordered event list and derive `(manifest_root, chunk_count, chunk_digests)`. The
/// worker calls this over what it retains; a verifier calls it over what was served; equality is
/// the availability check.
pub fn fp_trace_manifest_v3(trace_binding: Hash64, events: &[Hash64]) -> (Hash64, u32, Vec<Hash64>) {
    let digests: Vec<Hash64> = events
        .chunks(PALW_FP_TRACE_CHUNK_EVENTS_V3 as usize)
        .enumerate()
        .map(|(index, chunk)| fp_trace_chunk_digest_v3(trace_binding, index as u32, chunk))
        .collect();
    (fp_trace_manifest_root_v3(trace_binding, &digests), digests.len() as u32, digests)
}

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
    /// The executor's `committed_execution_root` (ADR-0030's `PalwStepBindingV2`) — the single
    /// value that fixes the SHAPE of the execution being claimed: the job context, both profiles,
    /// the leaf and checkpoint counts and their roots all recompute into it.
    ///
    /// **The free-prompt lane needs it for exactly the reason the attempt lane does (audit C3),
    /// and the integration found it missing.** `adjudicate_court_close_v2` tests a refutation's
    /// binding against the CLAIM's `execution_root`; a free-prompt claim built without one had to
    /// borrow some other field, and the nearest — `schedule_root` — is a different quantity that
    /// no honest binding can ever recompute to. Every free-prompt dispute would therefore have
    /// died at `ExecutionRootMismatch`: fail-closed, and useless, because a producer no court can
    /// convict is a producer that can commit arithmetic fraud with impunity. Carrying the real
    /// root is what makes the free-prompt lane adjudicable at all, and it is a distinct field
    /// rather than a reuse for the same reason it is on the attempt: `verify_binding` recomputes
    /// it from every component, so pinning the root pins the whole shape to the EXECUTOR'S claim
    /// instead of the accuser's.
    pub execution_root: Hash64,
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
    /// = [`spend_challenge_v3`] over this spend's HEADER POSITION.
    ///
    /// Without it a spend is position-free: its id — and therefore the L1 tag the finalizer
    /// consumes — is constant, while the header's `nonce` and `timestamp` still enter the block
    /// hash. One signature would mint unlimited distinct valid block identities, which is audit
    /// P0-1's shape arriving through the door that has no PoW at all. With it, every alternative
    /// position is a different spend id, a different tag, and a different signature: identities
    /// stop being free even though the lane's work is not a hash.
    ///
    /// (Re-signing at another position is still cheap, and deliberately so — those are CONFLICTING
    /// spends of one quantum, resolved like a double spend by the branch-scoped spent set, not
    /// extra weight. Relay in-flight limits, not a consensus rule, bound the flood.)
    pub challenge: Hash64,
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

/// `H(network_domain ‖ pre_pow_hash ‖ timestamp ‖ nonce ‖ claim_id ‖ quantum_index ‖ bond)` —
/// the spend's header position, in the shape the attempt lane's `challenge_v2` established.
///
/// The claim and quantum are inside it, not merely beside it: without them one solved position
/// could be re-announced for a different quantum of the same receipt at no extra cost, which is
/// the same substitution the attempt lane's class/bond binding refuses.
pub fn spend_challenge_v3(
    network_domain: Hash64,
    pre_pow_hash: Hash64,
    timestamp: u64,
    nonce: u64,
    claim_id: Hash64,
    quantum_index: u32,
    producer_bond: &TransactionOutpoint,
) -> Hash64 {
    let mut state = keyed(PALW_FP_V3_DOMAIN_SPEND_CHALLENGE);
    state.update(network_domain.as_byte_slice());
    state.update(pre_pow_hash.as_byte_slice());
    state.update(&timestamp.to_le_bytes());
    state.update(&nonce.to_le_bytes());
    state.update(claim_id.as_byte_slice());
    state.update(&quantum_index.to_le_bytes());
    state.update(producer_bond.transaction_id.as_byte_slice());
    state.update(&producer_bond.index.to_le_bytes());
    finish(state)
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

/// The free-prompt lane's network constants — a REQUIRED part of the `ConsensusV2` bundle
/// (ADR-0044 Decision 9): there is no fence that enables the receipt lane, only a ruleset that
/// includes it, hashed into `palw_ruleset_id_v2`. Constructed only through
/// [`PalwFreePromptParamsV3::new`]; no `Default`. The source split is deliberately NOT here —
/// it lives in the state params ([`crate::palw_state_v2::PalwStateParamsV2`]), because the
/// transition's retarget consumes it, and one fact in two places is two facts.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwFreePromptParamsV3 {
    /// == [`crate::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3`]; a bundle claiming another id is
    /// another ruleset.
    receipt_algorithm_id: u8,
    /// One quantum's CU — the uniform slice a certified job divides into (Decision 5).
    quantum_cu: u128,
    /// The chain weight one spent quantum contributes.
    pwu_per_quantum: u64,
    /// The CU pricing rule (Decision 7).
    cu_weights: PalwFpCuWeightsV3,
    /// The per-receipt jackpot bound.
    max_quanta_per_receipt: u32,
    /// ≤ [`crate::palw_v2::PALW_V2_MAX_PROMPT_TOKENS`] — the wire frame is the outer bound.
    max_prompt_tokens: u32,
    /// ≤ [`crate::palw_v2::PALW_V2_MAX_TRACE_EVENTS`] — one decode step is one trace event.
    max_decode_tokens: u32,
    /// DAA delay from a claim's `Final` to its draw slot; the startup gate demands it exceed the
    /// reorg margin, so the draw beacon is never inside the reorgable fringe of the very
    /// certification it draws for.
    receipt_maturity_daa: u64,
    /// How long a win stays usable past its beacon (invariant F14).
    receipt_use_window_daa: u64,
    /// The declared worst-case gap to the next attempt-class chain block. Enforced against the
    /// panel windows at startup: a late beacon must still bind inside `window_bind`.
    max_beacon_gap_daa: u64,
}

impl PalwFreePromptParamsV3 {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        receipt_algorithm_id: u8,
        quantum_cu: u128,
        pwu_per_quantum: u64,
        cu_weights: PalwFpCuWeightsV3,
        max_quanta_per_receipt: u32,
        max_prompt_tokens: u32,
        max_decode_tokens: u32,
        receipt_maturity_daa: u64,
        receipt_use_window_daa: u64,
        max_beacon_gap_daa: u64,
    ) -> Result<Self, PalwFpV3Error> {
        if receipt_algorithm_id != crate::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3 {
            return Err(PalwFpV3Error::InvalidParams("receipt_algorithm_id is not the receipt-V3 id"));
        }
        if quantum_cu == 0 {
            return Err(PalwFpV3Error::InvalidParams("a zero quantum divides everything and prices nothing"));
        }
        if pwu_per_quantum == 0 {
            return Err(PalwFpV3Error::InvalidParams("a zero per-quantum weight makes receipt blocks weightless"));
        }
        if cu_weights.decode_weight == 0 {
            return Err(PalwFpV3Error::InvalidParams("a zero decode weight prices the reference shape at nothing"));
        }
        if max_quanta_per_receipt == 0 {
            return Err(PalwFpV3Error::InvalidParams("a zero quanta cap certifies receipts that can never draw"));
        }
        if max_prompt_tokens == 0 || max_prompt_tokens as usize > crate::palw_v2::PALW_V2_MAX_PROMPT_TOKENS {
            return Err(PalwFpV3Error::InvalidParams("max_prompt_tokens must be 1..=the wire frame's prompt cap"));
        }
        if max_decode_tokens == 0 || max_decode_tokens as usize > crate::palw_v2::PALW_V2_MAX_TRACE_EVENTS {
            return Err(PalwFpV3Error::InvalidParams("max_decode_tokens must be 1..=the trace event cap"));
        }
        if receipt_use_window_daa == 0 {
            return Err(PalwFpV3Error::InvalidParams("a zero use window makes every win stale at birth"));
        }
        if max_beacon_gap_daa == 0 {
            return Err(PalwFpV3Error::InvalidParams("a zero beacon gap claims the next attempt block is always instant"));
        }
        Ok(Self {
            receipt_algorithm_id,
            quantum_cu,
            pwu_per_quantum,
            cu_weights,
            max_quanta_per_receipt,
            max_prompt_tokens,
            max_decode_tokens,
            receipt_maturity_daa,
            receipt_use_window_daa,
            max_beacon_gap_daa,
        })
    }

    pub fn receipt_algorithm_id(&self) -> u8 {
        self.receipt_algorithm_id
    }
    pub fn quantum_cu(&self) -> u128 {
        self.quantum_cu
    }
    pub fn pwu_per_quantum(&self) -> u64 {
        self.pwu_per_quantum
    }
    pub fn cu_weights(&self) -> &PalwFpCuWeightsV3 {
        &self.cu_weights
    }
    pub fn max_quanta_per_receipt(&self) -> u32 {
        self.max_quanta_per_receipt
    }
    pub fn max_prompt_tokens(&self) -> u32 {
        self.max_prompt_tokens
    }
    pub fn max_decode_tokens(&self) -> u32 {
        self.max_decode_tokens
    }
    pub fn receipt_maturity_daa(&self) -> u64 {
        self.receipt_maturity_daa
    }
    pub fn receipt_use_window_daa(&self) -> u64 {
        self.receipt_use_window_daa
    }
    pub fn max_beacon_gap_daa(&self) -> u64 {
        self.max_beacon_gap_daa
    }

    /// The claim-level derivation the acceptance layer runs before folding a
    /// `FreePromptCommitted` object: quanta from certified CU, total pwu from CU. `None`
    /// when the job is sub-quantum — such a commitment never enters the state (ADR-0044
    /// Decision 5: it certifies nothing the chain can act on, so it is not carried).
    ///
    /// **Weight stays quantum-uniform, and the quantum stays small** (ADR-0066 Decision 3).
    /// The alternative — pwu exactly CU-linear — was written and reverted: the state machine's
    /// spend frontier advances by `pwu / quanta` per spent quantum and its carriage invariant
    /// demands that division be exact (`free-prompt pwu … uniform non-zero quanta`), so a
    /// CU-linear total would either break the invariant or replace uniform slices with a
    /// remainder schedule — real arithmetic risk to shave an error the frozen 100-CU quantum
    /// already bounds at one quantum's weight per receipt (~one decode-token). The rule that
    /// matters survives intact: the rate `pwu_per_quantum / quantum_cu` is frozen, weight tracks
    /// CU to within one quantum, and no model's size ever argues the quantum should move.
    pub fn derive_quanta_and_pwu(&self, cu: u128) -> Option<(u32, u64)> {
        let quanta = fp_quanta_v3(cu, self.quantum_cu, self.max_quanta_per_receipt);
        if quanta == 0 {
            return None;
        }
        let pwu = (quanta as u64).checked_mul(self.pwu_per_quantum)?;
        Some((quanta, pwu))
    }

    /// **The largest CU one job of a class confined to `n_ctx` cached positions can certify
    /// here** — the admission gate's half of ADR-0066 Decision 2.
    ///
    /// The step enumeration's footprint is `prefill + decode − 1 ≤ n_ctx` (the same reading
    /// [`crate::palw_class_admission_v2::verify_class_admission_v2`] checks the canonical job
    /// against), with at least one prompt token and at least one decode step — a job that
    /// decodes nothing certifies nothing. Rather than assuming which weight is dearer, both
    /// extreme assignments are priced and the larger taken, so a future price table cannot make
    /// this bound quietly under-report and over-refuse. Both token caps of this ruleset bound
    /// their halves.
    pub fn max_admissible_cu_for_context(&self, n_ctx: u32) -> u128 {
        if n_ctx == 0 {
            return 0;
        }
        let decode_heavy = {
            let decode = n_ctx.min(self.max_decode_tokens);
            let prompt = (n_ctx - decode + 1).min(self.max_prompt_tokens).max(1);
            fp_cu_v3(prompt, decode, &self.cu_weights)
        };
        let prompt_heavy = {
            let prompt = n_ctx.min(self.max_prompt_tokens).max(1);
            let decode = (n_ctx - prompt + 1).min(self.max_decode_tokens).max(1);
            fp_cu_v3(prompt, decode, &self.cu_weights)
        };
        decode_heavy.max(prompt_heavy)
    }
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwFpV3Error {
    #[error("invalid free-prompt params: {0}")]
    InvalidParams(&'static str),
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
    #[error("the worker result does not bind the request: {0}")]
    WorkerResultMismatch(&'static str),
    #[error("the spend's carried challenge is not the one its header position derives")]
    SpendChallengeMismatch,
    #[error("the header carriage does not decode: {0}")]
    CarriageUndecodable(&'static str),
    #[error("the transaction payload does not decode as a free-prompt commitment")]
    PayloadUndecodable,
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
        self.validate_v3(Some(network_domain), Some(weights))
    }

    /// **The half a context-free caller can run: everything except the two checks that need the
    /// network's own parameters.**
    ///
    /// The transaction validator admits a free-prompt carrier in ISOLATION — no header, no chain
    /// state, and (by the same rule that keeps isolation re-usable across DAA scores) no
    /// per-network bundle. It therefore cannot ask "is this MY network's domain" or "is the
    /// carried cu the price MY weights derive"; both are the extraction walk's, which holds the
    /// bundle and re-derives the price rather than reading it.
    ///
    /// Deliberately the SAME code, parameterized, rather than a second copy of the shape rules:
    /// a divergence between the admission check and the walk's would admit carriers the walk
    /// then silently drops, which is exactly the "reads as nothing" failure this family keeps
    /// closing. The refusal ORDER is preserved too — the omitted checks are skipped in place,
    /// not moved — so a payload wrong in several ways names the same error either way.
    pub fn validate_shape_v3(&self) -> Result<(), PalwFpV3Error> {
        self.validate_v3(None, None)
    }

    fn validate_v3(&self, network_domain: Option<Hash64>, weights: Option<&PalwFpCuWeightsV3>) -> Result<(), PalwFpV3Error> {
        let c = &self.commitment;
        let job = &c.job;
        if job.version != PALW_FP_V3_VERSION {
            return Err(PalwFpV3Error::UnsupportedVersion { got: job.version, expected: PALW_FP_V3_VERSION });
        }
        if let Some(network_domain) = network_domain
            && job.network_domain != network_domain
        {
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
        if let Some(weights) = weights {
            let derived = fp_cu_v3(job.prompt_tokens, c.decode_tokens_executed, weights);
            if c.cu != derived {
                return Err(PalwFpV3Error::CuMismatch { claimed: c.cu, derived });
            }
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
    /// The header-carriage wire form: magic, then borsh.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = PALW_FP_V3_SPEND_CARRIAGE_MAGIC.to_vec();
        out.extend(borsh::to_vec(self).expect("borsh serialization of a plain struct cannot fail"));
        out
    }

    /// Decode a header-extension payload: magic, then borsh, then an exact-length check —
    /// trailing bytes are refused, because a payload is not a container. The magic differs from
    /// the V1 PBC1 and the attempt lane's, so a carriage of one family can never decode as
    /// another even before its fields are read.
    pub fn decode(bytes: &[u8]) -> Result<Self, PalwFpV3Error> {
        let Some(body) = bytes.strip_prefix(&PALW_FP_V3_SPEND_CARRIAGE_MAGIC) else {
            return Err(PalwFpV3Error::CarriageUndecodable("payload does not start with the PFS3 magic"));
        };
        let mut slice = body;
        let decoded = <Self as borsh::BorshDeserialize>::deserialize(&mut slice)
            .map_err(|_| PalwFpV3Error::CarriageUndecodable("borsh body"))?;
        if !slice.is_empty() {
            return Err(PalwFpV3Error::CarriageUndecodable("trailing bytes"));
        }
        Ok(decoded)
    }

    /// Stateless admission for the spend riding a receipt block's header extension.
    ///
    /// The carried `challenge` is RECOMPUTED from the header position rather than trusted —
    /// the same discipline `validate_stateless_v2` applies to an attempt, and for the same
    /// reason: failing here NAMES the mismatch instead of leaving a peer to infer it from a
    /// digest that did not match.
    pub fn validate_stateless_v3(
        &self,
        network_domain: Hash64,
        pre_pow_hash: Hash64,
        timestamp: u64,
        nonce: u64,
    ) -> Result<(), PalwFpV3Error> {
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
        if s.challenge
            != spend_challenge_v3(network_domain, pre_pow_hash, timestamp, nonce, s.claim_id, s.quantum_index, &s.producer_bond)
        {
            return Err(PalwFpV3Error::SpendChallengeMismatch);
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

// ---------------------------------------------------------------------------------------------
// The worker wire (FP-06): one framed request in, one framed result out — the subprocess
// contract `misaka-palw-gateway` drives and a panel replay re-drives.
// ---------------------------------------------------------------------------------------------

/// What the worker executes. Two arms, ONE loop:
///
/// * `Text` — the gateway's canonical-template-rendered UTF-8. The worker tokenizes it under the
///   pinned GGUF tokenizer (the class's `tokenizer_id`) and returns the ids; the CONSENSUS
///   identity is those ids (invariant F2), never the text.
/// * `TokenIds` — the replay arm: a panel seat rebuilds the request from chain data (the
///   commitment carries the ids whole under PublicDA), and MUST reach byte-identical roots.
///
/// The two arms differ only in where tokenization happens; after it, the execution — and
/// therefore the trace — is one code path, which is what makes text-in and ids-in equality a
/// property a smoke test can pin rather than a hope.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwFpWorkerInputV3 {
    Text(Vec<u8>) = 0,
    TokenIds(Vec<u32>) = 1,
}

/// The v3 job request: every field of the eventual [`PalwFreePromptJobV3`] the worker cannot
/// derive itself, plus the runtime identity pins (the worker refuses a request meant for a
/// runtime it is not), plus the input. The worker builds the job, binds the trace to
/// [`fp_job_id_v3`] — a value a replayer can rebuild from CHAIN data alone, which is the whole
/// requirement — and hands the job back for the gateway to cross-check field by field.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwFpWorkerRequestV3 {
    pub version: u16,
    pub network_domain: Hash64,
    pub class_id: Hash64,
    pub executor_bond: TransactionOutpoint,
    pub executor_pubkey: Vec<u8>,
    pub operator_id: Hash64,
    pub anchor_block: Hash64,
    pub anchor_daa: u64,
    pub job_nonce: [u8; 32],
    pub decode_token_limit: u32,
    pub max_context_tokens: u32,
    pub privacy_mode: u8,
    pub input: PalwFpWorkerInputV3,
    /// Runtime identity pins, checked against the worker's own manifest-derived values —
    /// running a job under a mis-declared identity would let one runtime impersonate another's
    /// determinism class. (`tokenizer_id` and the CU rule are not pinned here: the tokenizer is
    /// inside the manifest-pinned GGUF, and CU derivation is the consensus bundle's, applied by
    /// the caller over the returned counts.)
    pub model_profile_id: Hash64,
    pub runtime_manifest_hash: Hash64,
    pub runtime_class_id: Hash64,
    pub shape_profile_id: Hash64,
    pub trace_scheme_id: Hash64,
}

/// `H(domain ‖ len ‖ raw-frame)` — computed over the exact bytes read, echoed in the result, and
/// re-derived by the caller from its OWN canonical encoding, so a worker cannot answer a
/// different request than the one sent.
pub fn fp_worker_request_hash_v3(payload: &[u8]) -> Hash64 {
    canonical_id(PALW_FP_V3_DOMAIN_WORKER_REQUEST, payload)
}

/// The v3 job result: the consensus-grade commitment inputs AND the answer, from ONE execution.
///
/// This is the deliberate amendment to the v2 observability rule (ADR-0044 Decision 10): the v2
/// job path forbids rendered output leaving the process because its caller is a mining pipeline;
/// this path's caller is the USER's gateway, and returning the answer is the point. The
/// *projection-grade fields* (roots, counts) remain hashes — what consensus compares never
/// carries raw text.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwFpWorkerResultV3 {
    pub version: u16,
    pub request_hash: Hash64,
    /// The exact job identity the worker bound the trace to. The caller MUST cross-check every
    /// field against its own request (and the prompt hash against the returned ids) before
    /// building a commitment on it.
    pub job: PalwFreePromptJobV3,
    /// The canonical token ids (the `Text` arm's tokenization, or the `TokenIds` arm echoed) —
    /// what the PublicDA commitment carries whole.
    pub prompt_token_ids: Vec<u32>,
    pub trace_root: Hash64,
    pub output_root: Hash64,
    pub schedule_root: Hash64,
    /// The run's `committed_execution_root` — what the court binds a refutation to. See the
    /// commitment field of the same name: without it the free-prompt lane is unadjudicable.
    pub execution_root: Hash64,
    /// The retained-trace manifest ([`fp_trace_manifest_v3`] over the event list the worker
    /// wrote to its `--trace-out` directory). Retention is NOT optional on this path: a
    /// commitment whose producer kept nothing cannot serve an opening and would default in
    /// court, so the worker refuses to run without a retention directory.
    pub trace_manifest_root: Hash64,
    pub trace_chunk_count: u32,
    pub trace_event_count: u32,
    pub decode_tokens_executed: u32,
    pub stop_reason: PalwFpStopReasonV3,
    pub output_token_ids: Vec<u32>,
    /// The rendered answer bytes — the user's reply.
    pub rendered: Vec<u8>,
    pub model_load_ms: u64,
    pub execute_ms: u64,
}

impl PalwFpWorkerResultV3 {
    /// The caller-side re-binding (the `agent_client` discipline): everything the result claims
    /// is re-verified from the CALLER's own request — the worker is never trusted about what it
    /// was asked.
    pub fn validate_against_request(&self, request: &PalwFpWorkerRequestV3, request_hash: Hash64) -> Result<(), PalwFpV3Error> {
        if self.version != PALW_FP_V3_VERSION || self.job.version != PALW_FP_V3_VERSION {
            return Err(PalwFpV3Error::UnsupportedVersion { got: self.version, expected: PALW_FP_V3_VERSION });
        }
        if self.request_hash != request_hash {
            return Err(PalwFpV3Error::WorkerResultMismatch("the result echoes a different request"));
        }
        let j = &self.job;
        if j.network_domain != request.network_domain
            || j.class_id != request.class_id
            || j.executor_bond != request.executor_bond
            || j.executor_pubkey != request.executor_pubkey
            || j.operator_id != request.operator_id
            || j.anchor_block != request.anchor_block
            || j.anchor_daa != request.anchor_daa
            || j.job_nonce != request.job_nonce
            || j.decode_token_limit != request.decode_token_limit
            || j.max_context_tokens != request.max_context_tokens
            || j.privacy_mode != request.privacy_mode
        {
            return Err(PalwFpV3Error::WorkerResultMismatch("the returned job's fields are not the request's"));
        }
        if j.prompt_tokens as usize != self.prompt_token_ids.len()
            || j.prompt_token_ids_hash != crate::palw_v2::prompt_token_ids_hash_v2(&self.prompt_token_ids)
        {
            return Err(PalwFpV3Error::WorkerResultMismatch("the returned job does not bind the returned token ids"));
        }
        if let PalwFpWorkerInputV3::TokenIds(ids) = &request.input
            && ids != &self.prompt_token_ids
        {
            return Err(PalwFpV3Error::WorkerResultMismatch("the ids arm must echo the ids it was given"));
        }
        if self.decode_tokens_executed == 0 || self.decode_tokens_executed > j.decode_token_limit {
            return Err(PalwFpV3Error::WorkerResultMismatch("executed decode count is outside the job's budget"));
        }
        let canonical = match self.stop_reason {
            PalwFpStopReasonV3::ExactBudgetReached => self.decode_tokens_executed == j.decode_token_limit,
            PalwFpStopReasonV3::EndOfGeneration => self.decode_tokens_executed < j.decode_token_limit,
        };
        if !canonical {
            return Err(PalwFpV3Error::WorkerResultMismatch("the stop reason is not canonical for the executed count"));
        }
        if self.trace_event_count != self.decode_tokens_executed {
            return Err(PalwFpV3Error::WorkerResultMismatch("the trace event count is not the executed decode count"));
        }
        if self.output_token_ids.len() != self.decode_tokens_executed as usize {
            return Err(PalwFpV3Error::WorkerResultMismatch("the answer's token count is not the executed decode count"));
        }
        // The retained-trace shape is recomputable from the executed count alone — a worker
        // cannot under-retain without the mismatch showing here, and a zero manifest is not a
        // manifest.
        let expected_chunks = self.trace_event_count.div_ceil(PALW_FP_TRACE_CHUNK_EVENTS_V3);
        if self.trace_chunk_count != expected_chunks {
            return Err(PalwFpV3Error::WorkerResultMismatch("the retained-trace chunk count is not the executed shape's"));
        }
        if self.trace_manifest_root == Hash64::default() {
            return Err(PalwFpV3Error::WorkerResultMismatch("a zero trace manifest retains nothing"));
        }
        Ok(())
    }

    /// Assemble the consensus commitment from a validated result plus the two pieces only the
    /// caller holds: the retention DEADLINE (a chain-time promise the worker cannot make) and
    /// the bundle's CU weights. The CU is DERIVED here — the worker reports counts, never
    /// prices (invariant F7 starts at assembly, not at admission); the DA manifest is the
    /// worker's own retained-trace measurement, cross-checked by `validate_against_request`.
    pub fn to_commitment(&self, weights: &PalwFpCuWeightsV3, trace_retention_daa: u64) -> PalwFreePromptCommitmentV3 {
        PalwFreePromptCommitmentV3 {
            job: self.job.clone(),
            trace_root: self.trace_root,
            output_root: self.output_root,
            schedule_root: self.schedule_root,
            execution_root: self.execution_root,
            decode_tokens_executed: self.decode_tokens_executed,
            stop_reason: self.stop_reason,
            cu: fp_cu_v3(self.job.prompt_tokens, self.decode_tokens_executed, weights),
            trace_manifest_root: self.trace_manifest_root,
            trace_chunk_count: self.trace_chunk_count,
            trace_retention_daa,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The on-chain commitment payload (FP-08): what rides SUBNETWORK_ID_PALW_FP_COMMITMENT
// ---------------------------------------------------------------------------------------------

/// Wire cap for one FP commitment payload. The body is the commitment (fixed-shape apart from
/// the pubkey), the prompt token ids under PublicDA (≤ 4096 × 4 bytes), and one ML-DSA-87
/// signature (~4.6 KB) — 48 KiB is generous headroom and still far under anything that stresses
/// relay.
pub const PALW_FP_COMMITMENT_TX_MAX_BYTES: usize = 48 * 1024;

/// The transaction payload: the commitment, its PublicDA prompt, and the executor's signature.
///
/// The prompt token ids ride here rather than inside the commitment because the commitment's
/// identity already binds their hash — carrying them in the signed body would double the bytes
/// under the signature for no added binding, and carrying them NOWHERE would leave the panel
/// unable to replay (ADR-0044 Decision 8: PublicDA deletes the withholding failure mode rather
/// than adjudicating it). Acceptance re-derives the hash and refuses a mismatch, so the ids are
/// bound in effect while being carried once.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwFpCommitmentTxPayloadV3 {
    pub version: u16,
    pub commitment: PalwFreePromptCommitmentV3,
    /// The canonical prompt ids the commitment's `prompt_token_ids_hash` binds.
    pub prompt_token_ids: Vec<u32>,
    /// ML-DSA-87 over [`fp_claim_id_v3`] under [`PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT`].
    pub signature: Vec<u8>,
}

impl PalwFpCommitmentTxPayloadV3 {
    /// Stateless acceptance for the payload: the commitment's own stateless rules, plus the two
    /// facts only the payload can state — that the carried ids ARE the ids the commitment binds,
    /// and that the PublicDA promise is kept (a non-empty list under the mode that requires one).
    ///
    /// The signature is verified by the caller (this crate holds no ML-DSA implementation);
    /// [`Self::signed_message`] is what it must verify over.
    pub fn validate_stateless_v3(&self, network_domain: Hash64, weights: &PalwFpCuWeightsV3) -> Result<(), PalwFpV3Error> {
        self.validate_v3(Some(network_domain), Some(weights))
    }

    /// The context-free half — see [`PalwFreePromptCommitmentEnvelopeV3::validate_shape_v3`] for
    /// why the transaction validator can only run this one.
    pub fn validate_shape_v3(&self) -> Result<(), PalwFpV3Error> {
        self.validate_v3(None, None)
    }

    /// **The signature, on the payload that actually rides a transaction.**
    ///
    /// `validate_stateless_v3` says "the signature is verified by the caller" — and there was no
    /// caller. `PalwFreePromptCommitmentEnvelopeV3::validate_signature_v3` existed and the only use
    /// of that method name in the tree was the SPEND envelope's. So a 0x4a transaction from any
    /// stranger created a claim bound to any bond outpoint it named, including the genesis premine
    /// bond pinned in `params.rs`, with a signature nothing looked at.
    ///
    /// Delegates to the envelope so there is one signed message and one context, not two.
    pub fn validate_signature_v3<V>(&self, verify_mldsa87: V) -> Result<(), PalwFpV3Error>
    where
        V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    {
        PalwFreePromptCommitmentEnvelopeV3 { commitment: self.commitment.clone(), signature: self.signature.clone() }
            .validate_signature_v3(verify_mldsa87)
    }

    fn validate_v3(&self, network_domain: Option<Hash64>, weights: Option<&PalwFpCuWeightsV3>) -> Result<(), PalwFpV3Error> {
        if self.version != PALW_FP_V3_VERSION {
            return Err(PalwFpV3Error::UnsupportedVersion { got: self.version, expected: PALW_FP_V3_VERSION });
        }
        let envelope = PalwFreePromptCommitmentEnvelopeV3 { commitment: self.commitment.clone(), signature: self.signature.clone() };
        envelope.validate_v3(network_domain, weights)?;
        if self.prompt_token_ids.len() != self.commitment.job.prompt_tokens as usize {
            return Err(PalwFpV3Error::WorkerResultMismatch("the carried prompt length is not the committed prompt length"));
        }
        if crate::palw_v2::prompt_token_ids_hash_v2(&self.prompt_token_ids) != self.commitment.job.prompt_token_ids_hash {
            return Err(PalwFpV3Error::WorkerResultMismatch("the carried prompt ids are not the ones the commitment binds"));
        }
        Ok(())
    }

    /// The message the signature covers — the claim id, which is total over the commitment.
    pub fn signed_message(&self) -> Hash64 {
        fp_claim_id_v3(&self.commitment)
    }

    /// The claim id this payload creates on acceptance.
    pub fn claim_id(&self) -> Hash64 {
        self.signed_message()
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
            execution_root: Hash64::from_u64_word(0x4E),
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

    /// The header position the spend fixture binds (see [`spend_challenge_v3`]).
    const SPEND_PPH: u64 = 0xB0;
    const SPEND_TS: u64 = 1_700_000_000;
    const SPEND_NONCE: u64 = 7;

    fn spend() -> PalwReceiptSpendUnsignedV3 {
        let claim_id = fp_claim_id_v3(&commitment());
        let bond = op(1);
        PalwReceiptSpendUnsignedV3 {
            version: PALW_FP_V3_VERSION,
            network_domain: net(),
            challenge: spend_challenge_v3(net(), Hash64::from_u64_word(SPEND_PPH), SPEND_TS, SPEND_NONCE, claim_id, 2, &bond),
            claim_id,
            quantum_index: 2,
            beacon_block: Hash64::from_u64_word(0xBEAC),
            producer_bond: bond,
            producer_pubkey: vec![7u8; 32],
        }
    }

    /// **Golden vectors** — the canonical layouts, frozen. A borsh reordering, a domain edit or a
    /// field addition moves these bytes, and moving them is a NEW object family (a new domain
    /// suffix), never an in-place edit.
    ///
    /// **Re-taken once, 2026-08-20**, under exactly that rule: the commitment gained
    /// `execution_root` (without it the free-prompt lane had nothing for
    /// `adjudicate_court_close_v2` to bind a refutation against, so every dispute died at
    /// `ExecutionRootMismatch` and no fraud on that lane could ever be convicted), so
    /// `PALW_FP_V3_DOMAIN_CLAIM_ID` moved to `/v2` and the claim id, the spend id, the quantum
    /// ticket and the L1 tag all moved with it — they are derived from it, which is the property
    /// worth seeing in the diff. The job id did NOT move: the job is unchanged, and a vector that
    /// moved anyway would mean something drifted that should not have.
    #[test]
    fn golden_vector_ids_are_frozen() {
        let job_id = fp_job_id_v3(&job());
        let claim_id = fp_claim_id_v3(&commitment());
        let spend_id = fp_spend_id_v3(&spend());
        let ticket = fp_quantum_ticket_v3(net(), Hash64::from_u64_word(0xBEAC), claim_id, 2);

        assert_eq!(&faster_hex::hex_string(job_id.as_byte_slice())[..32], "d1ef7bce23d0edcc1a409b111d865c2f");
        assert_eq!(&faster_hex::hex_string(claim_id.as_byte_slice())[..32], "056161157ed31114052a6df6a021ff84");
        assert_eq!(&faster_hex::hex_string(spend_id.as_byte_slice())[..32], "f3310411b0d910075dc625617dc90c7d");
        assert_eq!(format!("{ticket:032x}"), "bf1ad833e20d6ff1a390b28c7bc45931");

        let tag = fp_spend_l1_tag_v3(spend_id);
        assert_eq!(tag.len(), PALW_FP_V3_L1_TAG_BYTES);
        assert_ne!(&tag[..64], &[0u8; 64][..], "the expansion is not degenerate");
        assert_eq!(&faster_hex::hex_string(&tag[..8]), "30f21abe1ea9e23f");
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
            ("execution_root", |c| c.execution_root = Hash64::from_u64_word(0x4F)),
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

        let pph = Hash64::from_u64_word(SPEND_PPH);
        let spend_ok = PalwReceiptSpendEnvelopeV3 { spend: spend(), signature: sig() };
        assert_eq!(spend_ok.validate_stateless_v3(net(), pph, SPEND_TS, SPEND_NONCE), Ok(()));
        // The position binding is RECOMPUTED, not trusted: the same spend announced at another
        // nonce is named — which is what stops one signature minting many block identities.
        assert_eq!(spend_ok.validate_stateless_v3(net(), pph, SPEND_TS, SPEND_NONCE + 1), Err(PalwFpV3Error::SpendChallengeMismatch));
        let mut foreign = spend();
        foreign.network_domain = Hash64::from_u64_word(0x99);
        let e = PalwReceiptSpendEnvelopeV3 { spend: foreign, signature: sig() };
        assert_eq!(e.validate_stateless_v3(net(), pph, SPEND_TS, SPEND_NONCE), Err(PalwFpV3Error::NetworkDomainMismatch));
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
        assert!(palw_ticket_admits_v1(base, base), "the boundary is inclusive, as the attempt lane's is");
    }

    /// The beacon fact is a pair of inequalities, both live: the beacon sits at or after the
    /// slot, and no attempt-class block does so earlier.
    #[test]
    fn beacon_fact_boundaries() {
        let fact =
            |beacon_daa, prev| PalwBeaconFactV3 { beacon_block: Hash64::from_u64_word(0xB), beacon_daa, prev_attempt_daa: prev };
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

    /// The worker wire's caller-side re-binding: an honest result validates and assembles a
    /// commitment whose CU is derived (never copied), and every lie the worker could tell about
    /// what it was asked is caught from the caller's own request.
    #[test]
    fn worker_result_rebinding_and_commitment_assembly() {
        let base_job = job();
        let ids: Vec<u32> = (0..base_job.prompt_tokens).collect();
        let mut j = base_job.clone();
        j.prompt_token_ids_hash = crate::palw_v2::prompt_token_ids_hash_v2(&ids);
        let request = PalwFpWorkerRequestV3 {
            version: PALW_FP_V3_VERSION,
            network_domain: j.network_domain,
            class_id: j.class_id,
            executor_bond: j.executor_bond,
            executor_pubkey: j.executor_pubkey.clone(),
            operator_id: j.operator_id,
            anchor_block: j.anchor_block,
            anchor_daa: j.anchor_daa,
            job_nonce: j.job_nonce,
            decode_token_limit: j.decode_token_limit,
            max_context_tokens: j.max_context_tokens,
            privacy_mode: j.privacy_mode,
            input: PalwFpWorkerInputV3::TokenIds(ids.clone()),
            model_profile_id: Hash64::from_u64_word(0x1),
            runtime_manifest_hash: Hash64::from_u64_word(0x2),
            runtime_class_id: Hash64::from_u64_word(0x3),
            shape_profile_id: Hash64::from_u64_word(0x4),
            trace_scheme_id: Hash64::from_u64_word(0x5),
        };
        let request_hash = fp_worker_request_hash_v3(&borsh::to_vec(&request).unwrap());
        let result = PalwFpWorkerResultV3 {
            version: PALW_FP_V3_VERSION,
            request_hash,
            job: j.clone(),
            prompt_token_ids: ids.clone(),
            trace_root: Hash64::from_u64_word(0x7A),
            output_root: Hash64::from_u64_word(0x0B),
            schedule_root: Hash64::from_u64_word(0x5C),
            execution_root: Hash64::from_u64_word(0x4E),
            trace_manifest_root: Hash64::from_u64_word(0xDA),
            trace_chunk_count: 1,
            trace_event_count: 77,
            decode_tokens_executed: 77,
            stop_reason: PalwFpStopReasonV3::EndOfGeneration,
            output_token_ids: vec![9; 77],
            rendered: b"an answer".to_vec(),
            model_load_ms: 1,
            execute_ms: 2,
        };
        result.validate_against_request(&request, request_hash).expect("the honest result binds");

        let commitment = result.to_commitment(&weights(), 999_999);
        assert_eq!(commitment.cu, fp_cu_v3(j.prompt_tokens, 77, &weights()), "CU is derived, never copied");
        assert_eq!(commitment.job, j);
        assert_eq!(
            (commitment.trace_manifest_root, commitment.trace_chunk_count, commitment.trace_retention_daa),
            (Hash64::from_u64_word(0xDA), 1, 999_999),
            "the DA trio: manifest from the worker's retention, deadline from the caller"
        );

        // Lies, each caught: a different request echoed; a job field swapped; ids the job hash
        // does not bind; an ids-arm echo mismatch; an overrun; a non-canonical stop; a trace
        // count that is not the executed count; an answer of the wrong length.
        let wrong_echo = result.clone();
        assert!(wrong_echo.validate_against_request(&request, Hash64::from_u64_word(0xEE)).is_err());
        let mut swapped = result.clone();
        swapped.job.job_nonce[0] ^= 1;
        assert!(swapped.validate_against_request(&request, request_hash).is_err());
        let mut unbound = result.clone();
        unbound.prompt_token_ids.push(1);
        assert!(unbound.validate_against_request(&request, request_hash).is_err());
        let mut foreign_ids = result.clone();
        foreign_ids.prompt_token_ids = (1..=base_job.prompt_tokens).collect();
        foreign_ids.job.prompt_token_ids_hash = crate::palw_v2::prompt_token_ids_hash_v2(&foreign_ids.prompt_token_ids);
        assert!(foreign_ids.validate_against_request(&request, request_hash).is_err(), "the ids arm must echo its input");
        let mut overrun = result.clone();
        overrun.decode_tokens_executed = j.decode_token_limit + 1;
        assert!(overrun.validate_against_request(&request, request_hash).is_err());
        let mut wrong_stop = result.clone();
        wrong_stop.stop_reason = PalwFpStopReasonV3::ExactBudgetReached;
        assert!(wrong_stop.validate_against_request(&request, request_hash).is_err());
        let mut ragged_trace = result.clone();
        ragged_trace.trace_event_count += 1;
        assert!(ragged_trace.validate_against_request(&request, request_hash).is_err());
        let mut short_answer = result.clone();
        short_answer.output_token_ids.pop();
        assert!(short_answer.validate_against_request(&request, request_hash).is_err());
        let mut under_retained = result.clone();
        under_retained.trace_chunk_count = 0;
        assert!(under_retained.validate_against_request(&request, request_hash).is_err(), "a chunk count off the executed shape");
        let mut hollow_manifest = result;
        hollow_manifest.trace_manifest_root = Hash64::default();
        assert!(hollow_manifest.validate_against_request(&request, request_hash).is_err(), "a zero manifest retains nothing");
    }

    /// The on-chain payload: an honest one validates, and the two lies only the payload can tell
    /// — a prompt of the wrong length, and a prompt whose hash the commitment does not bind —
    /// are refused. (PublicDA's whole point is that the panel replays from THESE bytes.)
    #[test]
    fn commitment_tx_payload_binds_its_publicda_prompt() {
        let ids: Vec<u32> = (0..96u32).collect();
        let mut c = commitment();
        c.job.prompt_token_ids_hash = crate::palw_v2::prompt_token_ids_hash_v2(&ids);
        c.job.prompt_tokens = ids.len() as u32;
        c.cu = fp_cu_v3(c.job.prompt_tokens, c.decode_tokens_executed, &weights());
        let payload = PalwFpCommitmentTxPayloadV3 {
            version: PALW_FP_V3_VERSION,
            commitment: c,
            prompt_token_ids: ids.clone(),
            signature: sig(),
        };
        payload.validate_stateless_v3(net(), &weights()).expect("the honest payload validates");
        assert_eq!(payload.claim_id(), fp_claim_id_v3(&payload.commitment));
        assert_eq!(payload.signed_message(), payload.claim_id(), "the signature covers the identity, which is total");

        let mut short = payload.clone();
        short.prompt_token_ids.pop();
        assert!(short.validate_stateless_v3(net(), &weights()).is_err(), "a prompt of the wrong length");

        let mut swapped = payload.clone();
        swapped.prompt_token_ids = (1..=96u32).collect();
        assert!(swapped.validate_stateless_v3(net(), &weights()).is_err(), "a prompt the commitment does not bind");

        let mut wrong_version = payload;
        wrong_version.version = 2;
        assert!(matches!(
            wrong_version.validate_stateless_v3(net(), &weights()),
            Err(PalwFpV3Error::UnsupportedVersion { got: 2, .. })
        ));
    }

    /// The retained-trace manifest: chunking is exact at the boundary, digests bind binding and
    /// index, and the verifier's recomputation equals the producer's.
    #[test]
    fn trace_manifest_chunks_and_binds() {
        let binding = Hash64::from_u64_word(0xB1);
        let events: Vec<Hash64> = (0..257).map(|i| Hash64::from_u64_word(i as u64 + 1)).collect();
        let (root, count, digests) = fp_trace_manifest_v3(binding, &events);
        assert_eq!(count, 2, "257 events at 256/chunk is two chunks");
        assert_eq!(digests.len(), 2);
        assert_eq!(root, fp_trace_manifest_root_v3(binding, &digests), "the composed fn equals its parts");
        let (root_exact, count_exact, _) = fp_trace_manifest_v3(binding, &events[..256]);
        assert_eq!(count_exact, 1, "the boundary is exact");
        assert_ne!(root, root_exact);

        assert_ne!(
            fp_trace_chunk_digest_v3(binding, 0, &events[..256]),
            fp_trace_chunk_digest_v3(binding, 1, &events[..256]),
            "the index is inside the digest — chunks cannot be reordered"
        );
        assert_ne!(
            fp_trace_chunk_digest_v3(binding, 0, &events[..256]),
            fp_trace_chunk_digest_v3(Hash64::from_u64_word(0xB2), 0, &events[..256]),
            "the binding is inside the digest — one job's chunks cannot serve another's manifest"
        );
        assert_ne!(root, Hash64::default());
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
