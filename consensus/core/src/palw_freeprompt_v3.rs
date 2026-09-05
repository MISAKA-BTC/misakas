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
//! `class_id` and the class registration (`palw_registry`) binds the rest. Carrying the same fact
//! in two places is how the two drift apart — ids are lookup keys, not passengers.
//!
//! **What NOTHING on chain checks today, stated so nobody budgets their suspicion on a guard that
//! is not there:** the job's `tokenizer_id`. This doc used to say admission cross-checks it against
//! the registered row; no such check exists (`palw_fp_admission_v3` never reads the field; its only
//! consumer folds it into the job context hash). It cannot be built here either: a class
//! registration carries `class_id` and `artifact_root`, and a court-capable row's root is the
//! A16 inventory root, which has no tokenizer input — so the chain holds no tokenizer identity to
//! compare against. Binding one is a change to what a class IS, not a check to add. The exposure
//! this leaves is at the gateway, where text becomes ids: a seat replays from ids and is not
//! affected; the person whose prompt was tokenized is. (ADR-0082 stream M, 2026-09-03.)

use crate::Hash64;
use crate::palw_attempt_v2::PALW_ATTEMPT_V2_L1_TAG_BYTES;
use crate::tx::TransactionOutpoint;
use blake2b_simd::Params;

/// Object version for every FP-V3 wire object. A different version is a different object family.
/// **3 → 4** (2026-09-02, ADR-0074): the job carries `prompt_mode`, the commitment carries
/// `work_leaves` in place of `cu`, and a quantum is a fraction of the class's own canonical job.
/// **4 → 5** (2026-09-03, ADR-0082 Decision 11): the job carries `sampling_seed` and
/// `temperature_q`. Two fields inside the job id means every claim id moves, which is exactly
/// what a ruleset move is for — and the alternative, a sampler whose inputs live outside the
/// identity the executor signed, is a seed the executor can change after the fact.
/// A node on the old wire cannot decode the new payload, and must not.
pub const PALW_FP_V3_VERSION: u16 = 5;

/// **The widest `work_leaves` any RULESET may make prosecutable** — the bound a caller that holds
/// no bundle uses, and the only honest one for a context-free door (ADR-0082 Decision 1).
///
/// A commitment's leaf count is bounded by the ladder its network froze
/// (`PalwCourtParamsV2::max_step_leaf_count`), and the widest a ladder may be is
/// [`crate::palw_context_ladder::PALW_CONTEXT_LADDER_MAX_STEP_LEAVES`] — the constant that sits
/// inside `palw_ruleset_id_v2`, so raising it is a re-mint and never a value a running chain can
/// exceed. Every entry point here that is not handed a bundle passes THIS, because the door's own
/// contract is that it is never stricter than the walk: bounding it at the executor's
/// `palw_step::PALW_STEP_MAX_LEAVES` (`2^22`) made transaction isolation refuse carriers the
/// `2^26` walk would have credited, which is the one direction that gate is forbidden to fail in.
pub const PALW_FP_STRUCTURAL_WORK_LEAVES_CAP: u64 = crate::palw_context_ladder::PALW_CONTEXT_LADDER_MAX_STEP_LEAVES;

/// Width of the expanded spend L1 tag — the attempt lane's width, so the Layer-0 finalizer's call
/// shape is identical across both block kinds.
pub const PALW_FP_V3_L1_TAG_BYTES: usize = PALW_ATTEMPT_V2_L1_TAG_BYTES;

/// The only weight-bearing privacy mode at v1 (ADR-0044 Decision 8): the prompt token ids are
/// carried whole in the commitment transaction. Encrypted modes are a future ADR and are refused
/// here — a mode this module does not understand must not certify, because certification is a
/// promise the panel can replay from chain data alone.
pub const PALW_FP_PRIVACY_PUBLIC_DA: u8 = 1;
/// **`PanelDa` — privacy mode 2 (ADR-0077 Decision 16): the prompt ids travel with the capture
/// the executor serves to its panel, never in the commitment transaction.** The job still carries
/// `prompt_token_ids_hash`; a seat checks `H(ids) == prompt_token_ids_hash` before it reads
/// anything else and files `Valid` only for a claim whose ids it holds; withholding is the
/// two-sided quorum's `ProducerDefaulted` arm; a court close that addresses a gather carries the
/// ids as it does now, so a disputed prompt becomes public. *Private unless disputed* — five seats
/// see the prompt, a dispute publishes it, and nothing here is confidentiality.
///
/// Admission refuses this mode until the network arms it — `Params::palw_panel_da`, `None` on
/// every shipped preset — exactly as the worker refuses every non-PublicDA mode today: a mode the
/// panel cannot replay must not execute. See [`crate::palw_panel_da_v1`] for the disclosure the
/// gateway owes a first-time user, the predicate a seat runs before it reads anything, and what
/// arming does NOT buy (ADR-0077 SA-5).
pub const PALW_FP_PRIVACY_PANEL_DA: u8 = 2;

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
/// ADR-0074 Decision 1: the anchor a CANONICAL claim's prompt derives from.
pub const PALW_FP_V3_DOMAIN_CANONICAL_ANCHOR: &[u8] = b"misaka-palw/fp-v3/canonical-anchor/v1";
/// ADR-0074 Decision 4: the identity of the WORK a claim commits — one inference, one claim.
pub const PALW_FP_V3_DOMAIN_WORK_ID: &[u8] = b"misaka-palw/fp-v3/work-id/v1";
/// ADR-0073 SA-1: the fold of the first `k` attempt blocks at or after a draw slot (see
/// [`fp_beacon_fold_v3`]).
pub const PALW_FP_V3_DOMAIN_BEACON_FOLD: &[u8] = b"misaka-palw/fp-v3/beacon-fold/v1";

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
    PALW_FP_V3_DOMAIN_CANONICAL_ANCHOR,
    PALW_FP_V3_DOMAIN_WORK_ID,
    PALW_FP_V3_DOMAIN_BEACON_FOLD,
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
    /// The tokenizer the executor read the prompt under, as the executor names it. A token-id
    /// sequence read under the wrong tokenizer is a different prompt with the same bytes — but
    /// **no consensus rule compares this field to anything** (the registration carries no tokenizer
    /// identity; see the module doc). It is folded into the job context hash and nothing more.
    pub tokenizer_id: Hash64,
    pub prompt_token_ids_hash: Hash64,
    pub prompt_tokens: u32,
    /// Ceiling, not exact count: EOG is a legitimate stop for a user answer (Decision 7). The
    /// executed count lives in the commitment, and the CU rule prices what ran, not the ceiling.
    pub decode_token_limit: u32,
    pub max_context_tokens: u32,
    /// [`PALW_FP_PRIVACY_PUBLIC_DA`] (the ids ride the commitment transaction) or
    /// [`PALW_FP_PRIVACY_PANEL_DA`] (they ride the capture the executor serves its panel, and the
    /// transaction carries none — ADR-0077 Decision 16). Anything else is refused, and mode 2 is
    /// refused too until this network arms it (`Params::palw_panel_da`).
    ///
    /// **Not a security field, and it does not become one by being here** (ADR-0079 Decision 2):
    /// it selects where the ids travel, which is a shape rule the whole network evaluates the
    /// same way. It is priced and committed like every other job field, and the ADR-0072 D8
    /// classification places it as a chain equality — the mode the ruleset admits — not as a
    /// posture a producer declares about its own host.
    ///
    /// It is inside the job id, so a claim cannot change its mind about the mode after the fact:
    /// the mode a user chose is part of the identity the executor signed.
    pub privacy_mode: u8,
    /// [`PALW_FP_PROMPT_MODE_USER`]: the prompt is the user's, its ids carried on chain and in the
    /// job material, hash-bound. [`PALW_FP_PROMPT_MODE_CANONICAL`]: the prompt is the family's
    /// canonical prompt for [`fp_canonical_anchor_v1`] of this job — the network's own job, run
    /// when nobody is asking (ADR-0074 Decision 1); the ids are a pure function of the job and
    /// travel only in the job material. Anything else is refused.
    pub prompt_mode: u8,
    /// **ADR-0082 Decision 11: the seed the decode sampler draws under.**
    ///
    /// `[0u8; 32]` is the greedy default, and at `temperature_q == 0` this field is INERT — the
    /// key never reads it (`crate::palw_decode_select_v2::decode_lane_key_v2`). Carried anyway,
    /// and hashed anyway, because a field outside identity is a field two objects can differ in
    /// while claiming to be the same object (the module note above).
    ///
    /// It is inside [`fp_job_id_v3`] BY CONSTRUCTION — that function hashes the whole canonical
    /// borsh of this struct — and therefore inside the claim id. Two consequences, and they are
    /// the reason Decision 11 puts the seed here rather than in the commitment:
    ///
    /// * **A seed cannot be changed after the fact.** The executor signed an identity that
    ///   already contains it, so "I meant a different seed" is a different job.
    /// * **Grinding a seed costs a WHOLE INFERENCE per draw**, which is ADR-0072's rule kept (one
    ///   inference, one ticket) rather than broken. Nothing here is a lottery input either: the
    ///   quantum ticket consumes a beacon that does not exist when this field is fixed on chain
    ///   (ADR-0044 F6), so a chosen seed buys a different ANSWER and never a better draw.
    pub sampling_seed: [u8; 32],
    /// **ADR-0082 Decision 11: the sampling temperature, in Q24** — `1 << 24` is a temperature of
    /// one in the class's own logit units.
    ///
    /// [`crate::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY`] (zero) is the shipped
    /// rule byte for byte, and is what every row carries while `Params::palw_fp_decode_rules` is
    /// dormant; a job declaring anything else is refused BY NAME
    /// ([`PalwFpV3Error::SamplingNotArmed`]) rather than quietly executed greedily, because a
    /// user who asked for a temperature and silently got greedy has been told a false thing about
    /// what ran.
    pub temperature_q: u32,
}

/// `H(canonical(job))` — every field, no exceptions.
pub fn fp_job_id_v3(job: &PalwFreePromptJobV3) -> Hash64 {
    let bytes = borsh::to_vec(job).expect("PalwFreePromptJobV3 is borsh-serializable");
    canonical_id(PALW_FP_V3_DOMAIN_JOB_ID, &bytes)
}

/// The prompt is the user's (ADR-0044's lane, unchanged).
pub const PALW_FP_PROMPT_MODE_USER: u8 = 0;
/// The prompt is the family's canonical prompt for the job's own anchor (ADR-0074 Decision 1).
pub const PALW_FP_PROMPT_MODE_CANONICAL: u8 = 1;

/// **The anchor a canonical claim's prompt derives from** (ADR-0074 Decision 1):
/// `H(domain ‖ network ‖ class ‖ bond ‖ anchor_block ‖ anchor_daa ‖ job_nonce)`.
///
/// Every input is a fact about the claim's own job, so a seat derives the same value and hands it
/// to the family's `job_for_anchor` — the same derivation the attempt lane runs from a block's
/// template — and compares the prompt it gets to `prompt_token_ids_hash`. The bond is inside:
/// two executors never share a canonical job, so a canonical claim is per-executor work by
/// construction. The nonce is inside: one executor's next job is its next nonce, and each is a
/// different inference.
pub fn fp_canonical_anchor_v1(job: &PalwFreePromptJobV3) -> Hash64 {
    let mut state = keyed(PALW_FP_V3_DOMAIN_CANONICAL_ANCHOR);
    state.update(job.network_domain.as_byte_slice());
    state.update(job.class_id.as_byte_slice());
    state.update(job.executor_bond.transaction_id.as_bytes().as_slice());
    state.update(&job.executor_bond.index.to_le_bytes());
    state.update(job.anchor_block.as_byte_slice());
    state.update(&job.anchor_daa.to_le_bytes());
    state.update(&job.job_nonce);
    finish(state)
}

/// **The identity of the work a claim commits** (ADR-0074 Decision 4):
/// `H(domain ‖ class ‖ prompt hash ‖ bond)`. One execution committed under N nonces, N retention
/// values or N anchors is N lottery entries at the cost of hashing; the transition refuses a
/// commitment whose work identity a live claim already holds.
///
/// # Why `decode_tokens_executed` is NOT in it (audit H-4)
///
/// It was, and that made the same prompt run once and committed at limits `1, 2, ..., D` into `D`
/// distinct work ids and `D` live claims — each of them a truthful, seat-replayable execution
/// whose leaves the executor already holds, none of them colliding, each drawing up to
/// `fp_max_quanta_per_receipt`. Tickets earned went from `min(leaves(D)/quantum, 64)` for one
/// inference to `Sum_d min(leaves(d)/quantum, 64)`, up to `64 D`; and every one of those claims
/// costs the network a full panel of five seats replaying a PREFIX of the same job, against the
/// lane's own daily payout ceiling.
///
/// The prefixes are exactly what ADR-0082 Decision 10 prices at zero and for the same reason: "a
/// deterministic causal model makes every prefix leaf recomputable at zero cost by the bond that
/// computed it once, so a subsidy on them is a subsidy on replay". Nesting is not the
/// SEGMENTATION the census showed to be unprofitable (disjoint continuations, where each segment
/// re-prefills the last one's answer); it is prefixes of one run, where nothing is re-prefilled.
///
/// So the identity is `(class, prompt, bond)` and nothing else the executor chooses: **one
/// prompt, one live claim per bond** — which is what "one inference, one claim" means when the
/// executor picks the length. A second commitment on the same prompt is `DuplicateWork` until the
/// first claim leaves the live set, exactly as a re-nonced one already was.
///
/// The sampler fields stay out for the reason they were always out — N seeds over one execution
/// must collide, not multiply — and they are inside `fp_job_id_v3` and therefore inside the claim
/// id, where they belong.
pub fn fp_work_id_v1(class_id: &Hash64, prompt_token_ids_hash: &Hash64, executor_bond: &TransactionOutpoint) -> Hash64 {
    let mut state = keyed(PALW_FP_V3_DOMAIN_WORK_ID);
    state.update(class_id.as_byte_slice());
    state.update(prompt_token_ids_hash.as_byte_slice());
    state.update(executor_bond.transaction_id.as_bytes().as_slice());
    state.update(&executor_bond.index.to_le_bytes());
    finish(state)
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
    /// **The work, in step leaves** (ADR-0074 Decision 5): the capture's `step_leaf_count`, the
    /// same unit the attempt lane's `pwu_per_inference` is counted in. A claim, verified by the
    /// seats against the capture they have authenticated (`verify_material` → `capture_shape`):
    /// the class state holds no profile, only the family can count leaves, and a claim whose
    /// price the seats refuse never licenses. Shape-crafting is gone by construction — leaves
    /// are the work.
    pub work_leaves: u64,
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

/// **A quantum is a fraction of the class's own canonical job** (ADR-0074 Decision 5):
/// `max(1, pwu_per_inference / quanta_per_canonical_job)` leaves. No network-wide leaf constant:
/// a floor job and a QWEN36 job each earn draws in units of their OWN class's work, which is
/// what the per-class receipt retarget normalises anyway, and `pwu = quanta × quantum` stays in
/// leaves so `safe_weight` is one unit across lanes and classes.
pub fn fp_class_quantum_leaves_v1(pwu_per_inference: u64, quanta_per_canonical_job: u32) -> u64 {
    if quanta_per_canonical_job == 0 {
        return 0;
    }
    (pwu_per_inference / quanta_per_canonical_job as u64).max(1)
}

/// How many uniform quanta a certified leaf count yields: `min(⌊work_leaves / quantum⌋, cap)`.
///
/// Floor, deliberately: the sub-quantum remainder certifies (the audit still happened) but never
/// draws — a partial ticket would need a scaled target, and a scaled target re-opens the
/// shape-grinding surface the quantum exists to close. Capped, so one claim is at most `cap`
/// lottery entries however large its job; more work is more claims, each paying its own fee.
pub fn fp_quanta_v3(work_leaves: u64, quantum_leaves: u64, max_quanta_per_receipt: u32) -> u32 {
    if quantum_leaves == 0 {
        return 0;
    }
    let full = work_leaves / quantum_leaves;
    full.min(max_quanta_per_receipt as u64) as u32
}

/// **ADR-0082 Decision 10: which leaves a free-prompt claim is CREDITED for.**
///
/// One spelling, so the transition, a builder and a seat cannot answer it differently.
///
/// * `decode_rules == false` (every shipped preset, `Params::palw_fp_decode_rules` dormant): the
///   whole capture, `work_leaves`, exactly as the lane prices it today.
/// * `decode_rules == true`: the leaves of the DECODE calls
///   ([`crate::palw_step::decode_leaf_count_v1`]). The prefill of a user-chosen prompt is priced
///   at ZERO.
///
/// The rationale is arithmetic, not policy. A pinned integer model is deterministic and causal, so
/// every leaf of a prefix is a pure function of the prefix: the same bond re-submitting a
/// 32,000-token prompt with one new token recomputes nothing and, under the shipped rule, is
/// credited everything. Prefill leaves are therefore not scarce, and a subsidy on them is a
/// subsidy on replay.
///
/// **The quantum is unchanged** (ADR-0074 Decision 5): the denominator stays the class's own
/// canonical job, so a class's own scale still normalises its draws. What moves is the numerator
/// and nothing else — which is why this is a function of one argument pair and not a new pricing
/// rule.
///
/// **The attempt lane never calls this.** Its canonical job is drawn from the beacon and cannot be
/// cached, so there is no replay to subsidise and nothing to remove.
pub fn fp_credited_leaves_v1(decode_rules: bool, work_leaves: u64, decode_leaves: u64) -> u64 {
    if decode_rules { decode_leaves } else { work_leaves }
}

/// The quantum's lottery draw: leading 128 bits (big-endian, matching `palw_ticket_v1`'s reading)
/// of `H(network ‖ beacon ‖ claim ‖ q)`.
///
/// Everything executor-chosen in this preimage (`claim_id`) is irrevocable on chain before
/// `beacon_block` exists, and the beacon is an attempt-class block whose every alternative sample
/// costs one inference (Decision 4). Compare against the class's receipt target with
/// [`crate::palw_pwu::palw_ticket_admits_v1`] — one ticket space, two lanes.
/// **One spendable quantum of a certified free-prompt claim, as a producer needs it (FP-R5).**
///
/// A row exists only for a quantum whose whole story already holds on this chain: the claim is
/// `Final`, the quantum is unspent, the beacon fact derived, and the ticket compared against the
/// class's receipt target AT THE POINT the facts were read. `wins` is that comparison — carried,
/// with its inputs, so a producer never re-derives the lottery it cannot influence and never
/// builds a block the admission is known to refuse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwFpSpendableQuantumV3 {
    pub claim_id: Hash64,
    pub class_id: Hash64,
    pub quantum_index: u32,
    pub beacon: PalwBeaconFactV3,
    pub receipt_target: u128,
    pub ticket: u128,
    pub wins: bool,
    /// The last DAA a spending block may carry (invariant F14's window end).
    pub spend_deadline_daa: u64,
}

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
/// **Past ADR-0073 SA-1's fence the two block fields describe a FOLD rather than one block**, and
/// the struct is deliberately unchanged so nothing downstream has to learn the difference:
/// `beacon_block` is [`fp_beacon_fold_v3`] of the first `k` attempt blocks at or after the slot
/// (identically the single block's own hash at `k = 1`, which is what keeps the fence-off bytes),
/// and `beacon_daa` is the `k`-th of them — the height at which the draw becomes DETERMINED, which
/// is what the use window must start from. `prev_attempt_daa` is unchanged in both regimes: it is
/// still the last attempt block strictly below the slot, so `prev < slot ≤ beacon_daa` still says
/// the fold begins exactly at the slot boundary and nothing was skipped under it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwBeaconFactV3 {
    pub beacon_block: Hash64,
    pub beacon_daa: u64,
    /// DAA score of the last attempt-class chain block strictly before the slot. Genesis-shaped
    /// networks with no prior attempt block use 0 — the slot of the first real claim is always
    /// past genesis.
    pub prev_attempt_daa: u64,
}

/// **ADR-0073 SA-1: `H(domain ‖ k ‖ blocks…)` over the first `k` attempt blocks at or after the
/// slot, in ASCENDING chain order.**
///
/// Why a fold at all: a single-block beacon costs one inference to RE-ROLL but only one block's
/// subsidy to WITHHOLD, and a producer whose block would be the beacon can drop it when the draw
/// disfavours its own pending claims. Folding `k` blocks means a producer holding attempt share
/// `p` controls the draw only when it holds ALL `k` — probability `p^k` instead of `p`.
///
/// The shape is [`fp_trace_manifest_root_v3`]'s, deliberately: domain-keyed, count-prefixed, and
/// ordered, so a shorter fold cannot collide with a longer one and no permutation of the same
/// blocks is the same value. Ascending chain order (the reverse of the validator's descending
/// walk) is the canonical one — it is the order the blocks were produced in, which is the only
/// order two nodes derive without agreeing on a walk direction first.
pub fn fp_beacon_fold_v3(blocks_ascending: &[Hash64]) -> Hash64 {
    let mut state = keyed(PALW_FP_V3_DOMAIN_BEACON_FOLD);
    state.update(&(blocks_ascending.len() as u32).to_le_bytes());
    for block in blocks_ascending {
        state.update(block.as_byte_slice());
    }
    finish(state)
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
    /// **A quantum is this fraction of the class's canonical job** (ADR-0074 Decision 5): the
    /// quantum for a class is `max(1, pwu_per_inference / quanta_per_canonical_job)` leaves.
    quanta_per_canonical_job: u32,
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
        quanta_per_canonical_job: u32,
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
        if quanta_per_canonical_job == 0 {
            return Err(PalwFpV3Error::InvalidParams("a zero quanta-per-canonical-job divides nothing and prices nothing"));
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
            quanta_per_canonical_job,
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
    pub fn quanta_per_canonical_job(&self) -> u32 {
        self.quanta_per_canonical_job
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
    /// `FreePromptCommitted` object: quanta from certified CU, total pwu from quanta. `None`
    /// when the job is sub-quantum — such a commitment never enters the state (ADR-0044
    /// Decision 5: it certifies nothing the chain can act on, so it is not carried).
    /// `(quanta, pwu)` for `work_leaves` of a class whose canonical job is `pwu_per_inference`
    /// leaves — the derivation the transition runs (ADR-0074 Decision 5), exposed so a builder
    /// or a seat can ask the same question before the chain does.
    pub fn derive_quanta_and_pwu(&self, work_leaves: u64, pwu_per_inference: u64) -> Option<(u32, u64)> {
        let quantum = fp_class_quantum_leaves_v1(pwu_per_inference, self.quanta_per_canonical_job);
        let quanta = fp_quanta_v3(work_leaves, quantum, self.max_quanta_per_receipt);
        if quanta == 0 {
            return None;
        }
        let pwu = (quanta as u64).checked_mul(quantum)?;
        Some((quanta, pwu))
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
    #[error("privacy mode {0} is not a mode this build understands (1 = PublicDa, 2 = PanelDa); encrypted modes are a future ADR")]
    UnsupportedPrivacyMode(u8),
    /// **The refusal a reader can act on** (ADR-0077 Decision 16). Mode 2 is a mode this build
    /// understands; it is not a mode THIS NETWORK has armed. Naming it separately from
    /// `UnsupportedPrivacyMode` is the whole point: one says "no build does this", the other says
    /// "this ruleset does not, and the ruleset move that changes it is a named field".
    #[error(
        "privacy mode 2 (PanelDa) is not armed on this network — it arms at a height through Params::palw_panel_da, and this network has none"
    )]
    PanelDaNotArmed,
    /// **ADR-0082 Decision 11's arming refusal.** Not `UnsupportedSampling`: this build understands
    /// the seeded argmax completely (`crate::palw_decode_select_v2`), and what it is refusing is a
    /// RULESET that has not armed it — a fence an operator can schedule, exactly like
    /// [`Self::PanelDaNotArmed`] beside it.
    #[error(
        "sampling (temperature_q {temperature_q}) is not armed on this network — ADR-0082 Decision 11 arms at a height through Params::palw_fp_decode_rules, and this network has none; the greedy defaults are temperature_q 0 with a zero seed"
    )]
    SamplingNotArmed { temperature_q: u32 },
    /// A mode-2 payload that carries the prompt anyway. Refused rather than trimmed: an executor
    /// that published a prompt the user asked to keep off chain has already done the harm, and a
    /// claim built on that payload would be one the chain quietly blessed.
    #[error("a PanelDa commitment carries {0} prompt ids — mode 2 keeps the prompt off chain and this payload publishes it")]
    PanelDaPayloadCarriesPrompt(usize),
    /// The seat-side refusal: nothing was served, so there is nothing to check (ADR-0077
    /// Decision 16 / W8 clause 1). A seat that reaches this files no `Valid`; it files the panel's
    /// `Unavailable` arm, which past ADR-0065 D4 is an abstention and not an accusation.
    #[error("no prompt ids are held for this claim — a seat that holds none can verify nothing and files no Valid")]
    PromptIdsUnavailable,
    #[error("the served prompt is {got} ids; the job declares {declared}")]
    PromptIdsCountMismatch { got: usize, declared: u32 },
    #[error("the served prompt ids do not hash to the job's prompt_token_ids_hash")]
    PromptIdsHashMismatch,
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
    #[error("work_leaves is zero — a run that touched no leaf committed nothing")]
    ZeroWorkLeaves,
    #[error("work_leaves ({got}) is above the step space's own cap ({max})")]
    WorkLeavesAboveCap { got: u64, max: u64 },
    #[error("prompt mode {0} is not user (0) or canonical (1)")]
    UnsupportedPromptMode(u8),
    #[error("a canonical claim's payload carries prompt ids — the prompt is a function of the job and the chain does not carry it")]
    CanonicalPayloadCarriesPrompt,
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
    pub fn validate_stateless_v3(&self, network_domain: Hash64) -> Result<(), PalwFpV3Error> {
        // **`PanelDa` disarmed, because a caller that passed no arming has armed nothing**
        // (ADR-0077 Decision 16). This entry predates the mode; every one of its callers refuses
        // mode 2 today and keeps refusing it until it starts passing the answer.
        self.validate_v3(Some(network_domain), false, false, PALW_FP_STRUCTURAL_WORK_LEAVES_CAP)
    }

    /// The same rules **under this network's arming** — the entry that can admit a `PanelDa`
    /// commitment (ADR-0077 Decision 16). `panel_da_armed` is `Params::palw_panel_da_at` resolved
    /// at the point the caller is judging; a caller that cannot resolve it calls
    /// [`Self::validate_stateless_v3`] and gets the disarmed answer.
    pub fn validate_stateless_under_v3(&self, network_domain: Hash64, panel_da_armed: bool) -> Result<(), PalwFpV3Error> {
        self.validate_v3(Some(network_domain), panel_da_armed, false, PALW_FP_STRUCTURAL_WORK_LEAVES_CAP)
    }

    /// The same rules **under this network's arming AND its ruleset's ladder** (ADR-0082
    /// Decision 1). `max_step_leaf_count` is `PalwCourtParamsV2::max_step_leaf_count` off the
    /// bundle the accepting block is folded under — the only entry that can refuse a commitment
    /// this network's court could not prosecute.
    pub fn validate_stateless_under_ruleset_v3(
        &self,
        network_domain: Hash64,
        panel_da_armed: bool,
        max_step_leaf_count: u64,
    ) -> Result<(), PalwFpV3Error> {
        self.validate_v3(Some(network_domain), panel_da_armed, false, max_step_leaf_count)
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
    ///
    /// **`PanelDa`'s arming is NOT one of the omitted checks** (ADR-0077 Decision 16). It is
    /// asked here, and the answer this entry gives is "no" — the height-free one, so the door
    /// stays exactly as strict as it is today on every shipped preset, and no block becomes
    /// acceptable to this build that was not acceptable to the last one.
    ///
    /// A network that ARMS the mode calls [`Self::validate_shape_under_v3`] with
    /// `Params::palw_panel_da_admissible`, which is `is_some()` on the fence rather than
    /// `is_active(daa)`: height-free, and therefore weaker at every height than the walk's
    /// `palw_panel_da_at`. That ordering is what keeps this gate from rejecting a carrier the
    /// walk would have credited — the one direction it is forbidden to fail in.
    ///
    /// Mode 2's own shape rule, that the payload carries no ids, needs no arming at all and is
    /// checked under both answers.
    pub fn validate_shape_v3(&self) -> Result<(), PalwFpV3Error> {
        self.validate_v3(None, false, false, PALW_FP_STRUCTURAL_WORK_LEAVES_CAP)
    }

    /// The shape half under a known arming — the door on a network that carries the rule, and
    /// what a builder asks before it spends an inference on a job the chain will refuse.
    pub fn validate_shape_under_v3(&self, panel_da_armed: bool) -> Result<(), PalwFpV3Error> {
        self.validate_v3(None, panel_da_armed, false, PALW_FP_STRUCTURAL_WORK_LEAVES_CAP)
    }

    /// The shape half **under a ruleset's ladder** — for a builder that holds the bundle and wants
    /// the answer the walk will give, rather than the weaker one isolation can give.
    pub fn validate_shape_under_ruleset_v3(&self, panel_da_armed: bool, max_step_leaf_count: u64) -> Result<(), PalwFpV3Error> {
        self.validate_v3(None, panel_da_armed, false, max_step_leaf_count)
    }

    fn validate_v3(
        &self,
        network_domain: Option<Hash64>,
        panel_da_armed: bool,
        decode_rules_armed: bool,
        max_step_leaf_count: u64,
    ) -> Result<(), PalwFpV3Error> {
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
        // **Two refusals, not one** (ADR-0077 Decision 16). A mode nothing in this tree implements
        // and a mode this network has not armed are different facts with different fixes: the
        // first is an ADR nobody has written, the second is a fence an operator can schedule.
        // Collapsing them into `UnsupportedPrivacyMode(2)` told a mode-2 executor to go and write
        // the encrypted-DA ADR.
        match job.privacy_mode {
            PALW_FP_PRIVACY_PUBLIC_DA => {}
            PALW_FP_PRIVACY_PANEL_DA if panel_da_armed => {}
            PALW_FP_PRIVACY_PANEL_DA => return Err(PalwFpV3Error::PanelDaNotArmed),
            other => return Err(PalwFpV3Error::UnsupportedPrivacyMode(other)),
        }
        if job.prompt_mode != PALW_FP_PROMPT_MODE_USER && job.prompt_mode != PALW_FP_PROMPT_MODE_CANONICAL {
            return Err(PalwFpV3Error::UnsupportedPromptMode(job.prompt_mode));
        }
        // **ADR-0082 Decision 11, refused BY NAME while the fence is dormant.** The same two-sided
        // shape the privacy modes have directly above, and for the same reason: "no build does
        // this" and "this ruleset does not" are different facts with different fixes. A job
        // declaring a temperature on a network that has not armed `Params::palw_fp_decode_rules`
        // would otherwise be executed GREEDILY by a conforming worker — a user told a false thing
        // about what ran, and a commitment whose id says one rule while its tokens obey another.
        //
        // The seed is only meaningful at a non-zero temperature (the key never reads it at zero),
        // so a non-zero seed with a greedy temperature is refused too: it is a field claiming to
        // decide something that decides nothing, and admitting it would let two jobs with the same
        // execution carry two different ids.
        if !decode_rules_armed
            && (job.temperature_q != crate::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY
                || job.sampling_seed != crate::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY)
        {
            return Err(PalwFpV3Error::SamplingNotArmed { temperature_q: job.temperature_q });
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
        if c.work_leaves == 0 {
            return Err(PalwFpV3Error::ZeroWorkLeaves);
        }
        // **The ladder is the RULESET's, and a caller with none of its own must not be stricter
        // than the walk** (ADR-0082 Decision 1: "the ruleset's is read from the bundle, never
        // typed").
        //
        // This was `crate::palw_step::PALW_STEP_MAX_LEAVES` — the EXECUTOR's `2^22` — on a network
        // whose classes are admitted at `2^26`. The graph-v5 dense 512 row's canonical job counts
        // 6,630,544 leaves, so the admission gate accepted the class and this line refused every
        // honest commitment it can produce: `validate_palw_fp_commitment_tx` rejected the carrier
        // at transaction isolation, so no block could hold one, and the extraction walk skipped it
        // as "not stateless-admissible" if one ever got in. That also inverted this door's own
        // stated invariant — "strictly weaker than the walk, so it can never reject something the
        // walk would have accepted" — because the walk's ladder is the bundle's.
        //
        // The default the arming-free entries pass is therefore the STRUCTURAL top
        // ([`PALW_FP_STRUCTURAL_WORK_LEAVES_CAP`]), the widest ladder any ruleset may freeze, and
        // the `*_under_ruleset_v3` entries pass the bundle's own number.
        if c.work_leaves > max_step_leaf_count {
            return Err(PalwFpV3Error::WorkLeavesAboveCap { got: c.work_leaves, max: max_step_leaf_count });
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
/// One segment of a segment-wise chat prompt (ADR-0077 Decision 6).
///
/// The gateway emits the model's control tokens as `Special` ids it read off the worker's
/// manifest (`PalwFpWorkerManifestV1::special_tokens`), and the user's text as `Text` bytes; the
/// worker encodes every `Text` segment with special-token parsing DISABLED and concatenates the
/// two as ids. Untrusted text can therefore never smuggle a control token, and the model sees the
/// template it was trained on. Consensus sees ids only, so this is executor-side.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwFpPromptSegmentV1 {
    Special(u32) = 0,
    Text(Vec<u8>) = 1,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwFpWorkerInputV3 {
    Text(Vec<u8>) = 0,
    TokenIds(Vec<u32>) = 1,
    /// The segment-wise chat arm (ADR-0077 Decision 6): see [`PalwFpPromptSegmentV1`].
    Segments(Vec<PalwFpPromptSegmentV1>) = 2,
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
    /// [`PALW_FP_PROMPT_MODE_USER`] or [`PALW_FP_PROMPT_MODE_CANONICAL`] (ADR-0074 Decision 1);
    /// copied onto the job verbatim.
    pub prompt_mode: u8,
    /// ADR-0082 Decision 11's sampler inputs, copied onto the job verbatim like `prompt_mode`.
    /// The worker does not choose them: whoever asked for the inference did, and the job id the
    /// worker binds its trace to contains them — so a worker that ignored them would produce a
    /// commitment whose id no replayer can rebuild.
    pub sampling_seed: [u8; 32],
    pub temperature_q: u32,
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
    /// The capture's leaf count — the run's PRICE (ADR-0074 Decision 5), read off the binding.
    pub step_leaf_count: u64,
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
    pub fn validate_against_request(
        &self,
        request: &PalwFpWorkerRequestV3,
        request_hash: Hash64,
        prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    ) -> Result<(), PalwFpV3Error> {
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
            || !crate::palw_prompt_ids_v1::prompt_token_ids_match_v1(prompt_ids_form, &self.prompt_token_ids, &j.prompt_token_ids_hash)
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
    /// nothing else. The price is the capture's leaf count the worker read off its binding
    /// (ADR-0074 Decision 5) — a seat verifies it against the served capture, never a table;
    /// the DA manifest is the worker's own retained-trace measurement, cross-checked by
    /// `validate_against_request`.
    pub fn to_commitment(&self, trace_retention_daa: u64) -> PalwFreePromptCommitmentV3 {
        PalwFreePromptCommitmentV3 {
            job: self.job.clone(),
            trace_root: self.trace_root,
            output_root: self.output_root,
            schedule_root: self.schedule_root,
            execution_root: self.execution_root,
            decode_tokens_executed: self.decode_tokens_executed,
            stop_reason: self.stop_reason,
            work_leaves: self.step_leaf_count,
            trace_manifest_root: self.trace_manifest_root,
            trace_chunk_count: self.trace_chunk_count,
            trace_retention_daa,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The resident worker (ADR-0077 Decision 1, `--mode v3-serve`): one artifact mapping, one
// manifest handshake, then the SAME request/result frames over a persistent stream.
// ---------------------------------------------------------------------------------------------

/// The worker mode a gateway keeps resident: the artifact is mapped once, the manifest is the
/// first frame out, and every job after it is one [`PalwFpWorkerRequestV3`] frame in and a run of
/// [`PalwFpWorkerFrameV1`]s out, ending in exactly one `Result` or `Refused`.
pub const PALW_FP_WORKER_MODE_SERVE_V3: &str = "v3-serve";
/// The one-shot form the drills and the replay arm use: one request frame in, one bare
/// [`PalwFpWorkerResultV3`] frame out, process exit.
pub const PALW_FP_WORKER_MODE_JOB_V3: &str = "v3-job";
pub const PALW_FP_WORKER_MODE_MANIFEST_V3: &str = "v3-manifest";
pub const PALW_FP_WORKER_MANIFEST_V1_VERSION: u16 = 1;

/// **What a worker IS, stated once per process** (the `v3-manifest` answer, as a frame).
///
/// The identity pins a request must match, the width the class registers (`n_ctx`, read from the
/// catalog row and never from the artifact's rotary span — a runtime that answered wider than the
/// court admits would be exactly the two-products split ADR-0077 R0 exists to close), and the
/// control-token ids a gateway needs to build a segment-wise prompt.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwFpWorkerManifestV1 {
    pub version: u16,
    /// The catalog model id this worker serves (e.g. `Qwen/Qwen2.5-1.5B/graph-v2`).
    pub model_id: String,
    pub class_id: Hash64,
    pub model_profile_id: Hash64,
    pub runtime_manifest_hash: Hash64,
    pub runtime_class_id: Hash64,
    pub shape_profile_id: Hash64,
    pub trace_scheme_id: Hash64,
    pub tokenizer_id: Hash64,
    /// The class's registered context: prompt + answer, total.
    pub n_ctx: u32,
    pub prefill_single_batch_cap: u32,
    pub vocab: u32,
    /// The model's special tokens by their tokenizer names (`<|im_start|>`, `<|im_end|>`,
    /// `<|endoftext|>`, …), so a gateway builds [`PalwFpPromptSegmentV1::Special`] from names and
    /// never from ids it guessed.
    pub special_tokens: Vec<(String, u32)>,
    /// The ids at which generation ENDS for this model — the display stop. The execution still
    /// runs to the job's declared budget (the step leaves bind the executed count before the first
    /// leaf is hashed), so the commitment covers every executed token and the answer ends here.
    pub eog_token_ids: Vec<u32>,
}

/// One frame a `v3-serve` worker writes. Per accepted request: zero or more `Token`s in decode
/// order, then exactly one `Result`; a request the worker will not run is answered with exactly
/// one `Refused` and the worker stays up — one bad job must not drop a resident artifact.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwFpWorkerFrameV1 {
    /// The first frame of a serve session, once.
    Manifest(PalwFpWorkerManifestV1) = 0,
    /// One generated id as soon as it is selected (ADR-0077 Decision 2 — the answer streams; the
    /// commitment does not). `rendered` is this id's rendering alone; the gateway re-renders the
    /// committed ids at completion and refuses a stream that does not match them.
    Token {
        token_id: u32,
        rendered: Vec<u8>,
    } = 1,
    Result(Box<PalwFpWorkerResultV3>) = 2,
    Refused {
        reason: String,
    } = 3,
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
    /// The canonical prompt ids the commitment's `prompt_token_ids_hash` binds — **under
    /// `PublicDa` only**. Under `PalwFpPromptMode::Canonical` and under
    /// [`PALW_FP_PRIVACY_PANEL_DA`] this list MUST be empty, and validation requires it (ADR-0077
    /// Decision 16): a mode-2 executor that carried the ids anyway would be publishing a prompt
    /// the user asked to keep off chain, and the chain would have blessed it.
    pub prompt_token_ids: Vec<u32>,
    /// ML-DSA-87 over [`fp_claim_id_v3`] under [`PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT`].
    pub signature: Vec<u8>,
}

impl PalwFpCommitmentTxPayloadV3 {
    /// Stateless acceptance for the payload: the commitment's own stateless rules, plus the two
    /// facts only the payload can state — that the carried ids ARE the ids the commitment binds,
    /// that the PublicDA promise is kept (a non-empty list under the mode that requires one), and
    /// that the `PanelDa` promise is kept too (an EMPTY list under the mode that forbids one).
    ///
    /// The signature is verified by the caller (this crate holds no ML-DSA implementation);
    /// [`Self::signed_message`] is what it must verify over.
    pub fn validate_stateless_v3(
        &self,
        network_domain: Hash64,
        prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    ) -> Result<(), PalwFpV3Error> {
        self.validate_v3(Some(network_domain), false, false, PALW_FP_STRUCTURAL_WORK_LEAVES_CAP, prompt_ids_form)
    }

    /// The same, **under this network's arming** — the entry the extraction walk uses, and the
    /// only one that can admit a `PanelDa` payload (ADR-0077 Decision 16). `panel_da_armed` is
    /// `Params::palw_panel_da_at` at the accepting block's DAA score.
    pub fn validate_stateless_under_v3(
        &self,
        network_domain: Hash64,
        panel_da_armed: bool,
        prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    ) -> Result<(), PalwFpV3Error> {
        self.validate_v3(Some(network_domain), panel_da_armed, false, PALW_FP_STRUCTURAL_WORK_LEAVES_CAP, prompt_ids_form)
    }

    /// The same **under the ruleset's ladder as well as its arming** — what the extraction walk
    /// runs when the caller holds the bundle
    /// ([`PalwFreePromptCommitmentEnvelopeV3::validate_stateless_under_ruleset_v3`]).
    pub fn validate_stateless_under_ruleset_v3(
        &self,
        network_domain: Hash64,
        panel_da_armed: bool,
        max_step_leaf_count: u64,
        prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    ) -> Result<(), PalwFpV3Error> {
        self.validate_v3(Some(network_domain), panel_da_armed, false, max_step_leaf_count, prompt_ids_form)
    }

    /// The context-free half — see [`PalwFreePromptCommitmentEnvelopeV3::validate_shape_v3`] for
    /// why the transaction validator can only run this one, and why the arming it asks is the
    /// height-free one.
    pub fn validate_shape_v3(&self, prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1) -> Result<(), PalwFpV3Error> {
        self.validate_v3(None, false, false, PALW_FP_STRUCTURAL_WORK_LEAVES_CAP, prompt_ids_form)
    }

    /// The shape half under a known arming — `Params::palw_panel_da_admissible` at the door.
    pub fn validate_shape_under_v3(
        &self,
        panel_da_armed: bool,
        prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    ) -> Result<(), PalwFpV3Error> {
        self.validate_v3(None, panel_da_armed, false, PALW_FP_STRUCTURAL_WORK_LEAVES_CAP, prompt_ids_form)
    }

    /// The shape half under a ruleset's ladder — for a builder holding the bundle.
    pub fn validate_shape_under_ruleset_v3(
        &self,
        panel_da_armed: bool,
        max_step_leaf_count: u64,
        prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    ) -> Result<(), PalwFpV3Error> {
        self.validate_v3(None, panel_da_armed, false, max_step_leaf_count, prompt_ids_form)
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

    fn validate_v3(
        &self,
        network_domain: Option<Hash64>,
        panel_da_armed: bool,
        decode_rules_armed: bool,
        max_step_leaf_count: u64,
        prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    ) -> Result<(), PalwFpV3Error> {
        if self.version != PALW_FP_V3_VERSION {
            return Err(PalwFpV3Error::UnsupportedVersion { got: self.version, expected: PALW_FP_V3_VERSION });
        }
        let envelope = PalwFreePromptCommitmentEnvelopeV3 { commitment: self.commitment.clone(), signature: self.signature.clone() };
        envelope.validate_v3(network_domain, panel_da_armed, decode_rules_armed, max_step_leaf_count)?;
        // **`PanelDa` carries NO ids, and the check is a REQUIREMENT, not a tolerance** (ADR-0077
        // Decision 16). Placed before the canonical arm because the privacy mode decides what the
        // chain may hold and the prompt mode decides where the ids come from: a mode-2 payload
        // that carried them is refused whichever prompt mode it declares.
        if self.commitment.job.privacy_mode == PALW_FP_PRIVACY_PANEL_DA {
            if !self.prompt_token_ids.is_empty() {
                return Err(PalwFpV3Error::PanelDaPayloadCarriesPrompt(self.prompt_token_ids.len()));
            }
            // The job still carries `prompt_token_ids_hash` (Decision 16), so the claim is bound
            // to one prompt and one only; what the seats fetch is checked against it by
            // `palw_fp_prompt_ids_admit_v1` before anything else is read.
            return Ok(());
        }
        // A canonical claim's prompt is a pure function of the job (ADR-0074 Decision 1): the
        // chain does not carry it, and a payload that does is not the shipped worker's.
        if self.commitment.job.prompt_mode == PALW_FP_PROMPT_MODE_CANONICAL {
            if !self.prompt_token_ids.is_empty() {
                return Err(PalwFpV3Error::CanonicalPayloadCarriesPrompt);
            }
            return Ok(());
        }
        if self.prompt_token_ids.len() != self.commitment.job.prompt_tokens as usize {
            return Err(PalwFpV3Error::WorkerResultMismatch("the carried prompt length is not the committed prompt length"));
        }
        if !crate::palw_prompt_ids_v1::prompt_token_ids_match_v1(
            prompt_ids_form,
            &self.prompt_token_ids,
            &self.commitment.job.prompt_token_ids_hash,
        ) {
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
            prompt_mode: PALW_FP_PROMPT_MODE_USER,
            sampling_seed: crate::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
            temperature_q: crate::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        }
    }

    fn commitment() -> PalwFreePromptCommitmentV3 {
        let job = job();
        PalwFreePromptCommitmentV3 {
            job,
            trace_root: Hash64::from_u64_word(0x7A),
            output_root: Hash64::from_u64_word(0x00),
            schedule_root: Hash64::from_u64_word(0x5C),
            execution_root: Hash64::from_u64_word(0x4E),
            decode_tokens_executed: 77,
            stop_reason: PalwFpStopReasonV3::EndOfGeneration,
            work_leaves: 4_096,
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
    /// worth seeing in the diff. The job id did NOT move then: the job was unchanged.
    ///
    /// **Re-taken a second time, 2026-09-02**, under ADR-0074: the job gained `prompt_mode` and
    /// the commitment carries `work_leaves` in place of `cu`, so the wire version moved 3 → 4 and
    /// EVERY id below moved with it — the job id included, this time deliberately, because the
    /// job itself changed. A vector that had stayed would have meant the version was a lie.
    #[test]
    fn golden_vector_ids_are_frozen() {
        let job_id = fp_job_id_v3(&job());
        let claim_id = fp_claim_id_v3(&commitment());
        let spend_id = fp_spend_id_v3(&spend());
        let ticket = fp_quantum_ticket_v3(net(), Hash64::from_u64_word(0xBEAC), claim_id, 2);

        assert_eq!(&faster_hex::hex_string(job_id.as_byte_slice())[..32], "c940b5c36ee40846087e6c5927d6e6b5");
        assert_eq!(&faster_hex::hex_string(claim_id.as_byte_slice())[..32], "e75b8d2e3c590e6df59fe1b0db52676b");
        assert_eq!(&faster_hex::hex_string(spend_id.as_byte_slice())[..32], "87a0c79c36bf8ff80678a7c3bc48326e");
        assert_eq!(format!("{ticket:032x}"), "9a3ed14b531af14a6e2071e206f8e711");

        let tag = fp_spend_l1_tag_v3(spend_id);
        assert_eq!(tag.len(), PALW_FP_V3_L1_TAG_BYTES);
        assert_ne!(&tag[..64], &[0u8; 64][..], "the expansion is not degenerate");
        assert_eq!(&faster_hex::hex_string(&tag[..8]), "24e4a611d9d25f45");
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
            ("prompt_mode", |j| j.prompt_mode += 1),
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
            ("work_leaves", |c| c.work_leaves += 1),
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

    /// The price is the capture's leaf count, bounded by the step space's own cap (ADR-0074
    /// Decision 5), and every non-canonical encoding of the executed shape is refused: a zero
    /// price, a price above the cap, an overrun, a zero run, and both wrong stop-reason arms.
    #[test]
    fn work_leaves_are_bounded_and_stop_reasons_are_canonical() {
        let ok = PalwFreePromptCommitmentEnvelopeV3 { commitment: commitment(), signature: sig() };
        assert_eq!(ok.validate_stateless_v3(net()), Ok(()));

        let mut zero = commitment();
        zero.work_leaves = 0;
        let e = PalwFreePromptCommitmentEnvelopeV3 { commitment: zero, signature: sig() };
        assert!(matches!(e.validate_stateless_v3(net()), Err(PalwFpV3Error::ZeroWorkLeaves)));

        // **Above the STRUCTURAL cap, which is what an entry with no bundle bounds at.** This read
        // `PALW_STEP_MAX_LEAVES + 1` and therefore pinned the executor's `2^22` as the chain's
        // answer; a class the `2^26` ruleset admits produces honest counts above it, so the pin
        // was the defect rather than the guard. The ruleset's own number is asked for by
        // `validate_stateless_under_ruleset_v3`, exercised directly below.
        let mut above = commitment();
        above.work_leaves = PALW_FP_STRUCTURAL_WORK_LEAVES_CAP + 1;
        let e = PalwFreePromptCommitmentEnvelopeV3 { commitment: above, signature: sig() };
        assert!(matches!(e.validate_stateless_v3(net()), Err(PalwFpV3Error::WorkLeavesAboveCap { .. })));

        // The executor's constant is no longer a bound anybody applies: a count above `2^22` and
        // inside the structural top is admissible with no bundle in hand, and refused BY THE
        // RULESET when one is.
        let mut wide = commitment();
        wide.work_leaves = crate::palw_step::PALW_STEP_MAX_LEAVES + 1;
        let e = PalwFreePromptCommitmentEnvelopeV3 { commitment: wide, signature: sig() };
        assert_eq!(e.validate_stateless_v3(net()), Ok(()), "the executor's 2^22 is not the chain's ladder");
        assert_eq!(
            e.validate_stateless_under_ruleset_v3(net(), false, crate::palw_step::PALW_STEP_MAX_LEAVES),
            Err(PalwFpV3Error::WorkLeavesAboveCap {
                got: crate::palw_step::PALW_STEP_MAX_LEAVES + 1,
                max: crate::palw_step::PALW_STEP_MAX_LEAVES
            }),
            "a ruleset that froze 2^22 still refuses it, and names its own number"
        );

        let mut overrun = commitment();
        overrun.decode_tokens_executed = overrun.job.decode_token_limit + 1;
        let e = PalwFreePromptCommitmentEnvelopeV3 { commitment: overrun, signature: sig() };
        assert!(matches!(e.validate_stateless_v3(net()), Err(PalwFpV3Error::DecodeOverrun { .. })));

        // EOG exactly at the limit is the budget stop wearing the wrong name…
        let mut eog_at_limit = commitment();
        eog_at_limit.decode_tokens_executed = eog_at_limit.job.decode_token_limit;
        let e = PalwFreePromptCommitmentEnvelopeV3 { commitment: eog_at_limit, signature: sig() };
        assert!(matches!(e.validate_stateless_v3(net()), Err(PalwFpV3Error::NonCanonicalStopReason { .. })));

        // …and the budget stop below the limit claims a budget that did not end.
        let mut budget_below = commitment();
        budget_below.stop_reason = PalwFpStopReasonV3::ExactBudgetReached;
        let e = PalwFreePromptCommitmentEnvelopeV3 { commitment: budget_below, signature: sig() };
        assert!(matches!(e.validate_stateless_v3(net()), Err(PalwFpV3Error::NonCanonicalStopReason { .. })));

        let mut silent = commitment();
        silent.decode_tokens_executed = 0;
        let e = PalwFreePromptCommitmentEnvelopeV3 { commitment: silent, signature: sig() };
        assert!(matches!(e.validate_stateless_v3(net()), Err(PalwFpV3Error::ZeroDecodeExecuted)));

        // A prompt mode the lane does not know is refused by name (ADR-0074 Decision 1).
        let mut mode = commitment();
        mode.job.prompt_mode = 7;
        let e = PalwFreePromptCommitmentEnvelopeV3 { commitment: mode, signature: sig() };
        assert!(matches!(e.validate_stateless_v3(net()), Err(PalwFpV3Error::UnsupportedPromptMode(7))));
    }

    /// Stateless refusals name their reasons: version, network, privacy mode, empty prompt,
    /// zero decode ceiling, context overflow, chunkless trace, key/signature shapes.
    #[test]
    fn stateless_refusals_are_named() {
        let make = |mutate: fn(&mut PalwFreePromptCommitmentV3)| {
            let mut c = commitment();
            mutate(&mut c);
            PalwFreePromptCommitmentEnvelopeV3 { commitment: c, signature: sig() }
        };

        assert!(matches!(
            make(|c| c.job.version = 2).validate_stateless_v3(net()),
            Err(PalwFpV3Error::UnsupportedVersion { got: 2, expected: PALW_FP_V3_VERSION })
        ));
        assert_eq!(
            make(|c| c.job.network_domain = Hash64::from_u64_word(0x99)).validate_stateless_v3(net()),
            Err(PalwFpV3Error::NetworkDomainMismatch)
        );
        // **Mode 2 is no longer unknown — it is unarmed** (ADR-0077 Decision 16). The refusal is
        // the same refusal, and the NAME is the change: `UnsupportedPrivacyMode(2)` told a mode-2
        // executor that no build does this, when the truth is that this network has not scheduled
        // it. Both refusals still exist and this asserts both, because collapsing them again is
        // exactly the regression the two variants were split to prevent.
        assert_eq!(
            make(|c| c.job.privacy_mode = 2).validate_stateless_v3(net()),
            Err(PalwFpV3Error::PanelDaNotArmed),
            "PanelDa is a mode this build knows and this network has not armed"
        );
        assert_eq!(
            make(|c| c.job.privacy_mode = 2).validate_stateless_under_v3(net(), true),
            Ok(()),
            "…and the same commitment certifies where the fence is in force"
        );
        assert_eq!(
            make(|c| c.job.privacy_mode = 3).validate_stateless_under_v3(net(), true),
            Err(PalwFpV3Error::UnsupportedPrivacyMode(3)),
            "an unknown privacy mode must not certify — a panel cannot replay what it cannot read"
        );
        assert_eq!(make(|c| c.job.executor_pubkey = vec![]).validate_stateless_v3(net()), Err(PalwFpV3Error::MissingPublicKey));
        assert_eq!(make(|c| c.job.prompt_tokens = 0).validate_stateless_v3(net()), Err(PalwFpV3Error::EmptyPrompt));
        assert_eq!(make(|c| c.job.decode_token_limit = 0).validate_stateless_v3(net()), Err(PalwFpV3Error::ZeroDecodeLimit));
        assert!(matches!(
            make(|c| c.job.max_context_tokens = 100).validate_stateless_v3(net()),
            Err(PalwFpV3Error::ContextOverflow { .. })
        ));
        assert_eq!(make(|c| c.trace_chunk_count = 0).validate_stateless_v3(net()), Err(PalwFpV3Error::ZeroTraceChunks));

        let mut short = PalwFreePromptCommitmentEnvelopeV3 { commitment: commitment(), signature: sig() };
        short.signature.pop();
        assert!(matches!(short.validate_stateless_v3(net()), Err(PalwFpV3Error::SignatureLength { .. })));

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
        assert_eq!(fp_quanta_v3(u64::MAX, 1, u32::MAX), u32::MAX);
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
            prompt_mode: j.prompt_mode,
            sampling_seed: j.sampling_seed,
            temperature_q: j.temperature_q,
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
            step_leaf_count: 4_096,
            stop_reason: PalwFpStopReasonV3::EndOfGeneration,
            output_token_ids: vec![9; 77],
            rendered: b"an answer".to_vec(),
            model_load_ms: 1,
            execute_ms: 2,
        };
        result
            .validate_against_request(&request, request_hash, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
            .expect("the honest result binds");

        let commitment = result.to_commitment(999_999);
        assert_eq!(commitment.work_leaves, result.step_leaf_count, "the price is the capture's leaf count the worker read");
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
        assert!(
            wrong_echo
                .validate_against_request(&request, Hash64::from_u64_word(0xEE), crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
                .is_err()
        );
        let mut swapped = result.clone();
        swapped.job.job_nonce[0] ^= 1;
        assert!(
            swapped.validate_against_request(&request, request_hash, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat).is_err()
        );
        let mut unbound = result.clone();
        unbound.prompt_token_ids.push(1);
        assert!(
            unbound.validate_against_request(&request, request_hash, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat).is_err()
        );
        let mut foreign_ids = result.clone();
        foreign_ids.prompt_token_ids = (1..=base_job.prompt_tokens).collect();
        foreign_ids.job.prompt_token_ids_hash = crate::palw_v2::prompt_token_ids_hash_v2(&foreign_ids.prompt_token_ids);
        assert!(
            foreign_ids
                .validate_against_request(&request, request_hash, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
                .is_err(),
            "the ids arm must echo its input"
        );
        let mut overrun = result.clone();
        overrun.decode_tokens_executed = j.decode_token_limit + 1;
        assert!(
            overrun.validate_against_request(&request, request_hash, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat).is_err()
        );
        let mut wrong_stop = result.clone();
        wrong_stop.stop_reason = PalwFpStopReasonV3::ExactBudgetReached;
        assert!(
            wrong_stop.validate_against_request(&request, request_hash, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat).is_err()
        );
        let mut ragged_trace = result.clone();
        ragged_trace.trace_event_count += 1;
        assert!(
            ragged_trace
                .validate_against_request(&request, request_hash, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
                .is_err()
        );
        let mut short_answer = result.clone();
        short_answer.output_token_ids.pop();
        assert!(
            short_answer
                .validate_against_request(&request, request_hash, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
                .is_err()
        );
        let mut under_retained = result.clone();
        under_retained.trace_chunk_count = 0;
        assert!(
            under_retained
                .validate_against_request(&request, request_hash, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
                .is_err(),
            "a chunk count off the executed shape"
        );
        let mut hollow_manifest = result;
        hollow_manifest.trace_manifest_root = Hash64::default();
        assert!(
            hollow_manifest
                .validate_against_request(&request, request_hash, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
                .is_err(),
            "a zero manifest retains nothing"
        );
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
        let payload = PalwFpCommitmentTxPayloadV3 {
            version: PALW_FP_V3_VERSION,
            commitment: c,
            prompt_token_ids: ids.clone(),
            signature: sig(),
        };
        payload
            .validate_stateless_v3(net(), crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
            .expect("the honest payload validates");
        assert_eq!(payload.claim_id(), fp_claim_id_v3(&payload.commitment));
        assert_eq!(payload.signed_message(), payload.claim_id(), "the signature covers the identity, which is total");

        let mut short = payload.clone();
        short.prompt_token_ids.pop();
        assert!(
            short.validate_stateless_v3(net(), crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat).is_err(),
            "a prompt of the wrong length"
        );

        let mut swapped = payload.clone();
        swapped.prompt_token_ids = (1..=96u32).collect();
        assert!(
            swapped.validate_stateless_v3(net(), crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat).is_err(),
            "a prompt the commitment does not bind"
        );

        let mut wrong_version = payload;
        wrong_version.version = 2;
        assert!(matches!(
            wrong_version.validate_stateless_v3(net(), crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat),
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

// ---------------------------------------------------------------------------------------------
// The free-prompt DA material (FP-R6): what a seat needs to REPLAY the job
// ---------------------------------------------------------------------------------------------

/// Wire magic for a free-prompt claim's gossiped material. The retention/gossip plumbing carries
/// one bag of bytes per claim id for BOTH lanes, and the two lanes' payloads are different
/// objects: an attempt claim's material is the run's own rows (the seat re-hashes them), a
/// free-prompt claim's is the JOB — the seat re-executes it, which is what `PublicDa` promised
/// ("a mode the panel cannot replay must not execute"). Four magic bytes let a reader say "this
/// is the other lane's payload" instead of feeding one codec's bytes to the other's decoder.
pub const PALW_FP_MATERIAL_V1_MAGIC: [u8; 4] = *b"FPM1";

/// A free-prompt claim's data-availability payload: the job identity and the canonical prompt
/// ids it binds. Deliberately NOT the run — the run's own rows cannot reproduce the step and
/// checkpoint legs the claim's `execution_root` commits (only an instrumented execution can), so
/// shipping them would invite a verifier that checks less than the claim asserts. A seat holding
/// this payload and the registered artifact re-executes and compares every root; the payload is
/// a few hundred bytes where the attempt lane's is megabytes.
///
/// **Under `PanelDa` this is the only place the prompt ids exist off the executor's disk**
/// (ADR-0077 Decision 16): the commitment transaction carries none, so the path a seat already
/// walks — request the claim's material, decode, check the ids bind, then replay — is unchanged,
/// and what changes is that failing to serve it is now the only way to withhold a prompt. That
/// failure is never a verdict about arithmetic; which arm it reaches is
/// [`crate::palw_panel_da_v1::palw_panel_da_withholding_arm_v1`]'s answer.
///
/// # This payload carries the ids WHOLE, and ADR-0081 Decision 3 does not change that
///
/// Decision 3 makes `prompt_token_ids_hash` an openable Merkle root so a COURT CLOSE can prove one
/// id instead of carrying all of them — `n_ctx x 4` bytes on every node of the graph against an
/// 80 KiB carrier. None of that reasoning applies here, and shrinking this payload to an opening
/// would be a regression rather than an optimisation:
///
/// * a seat REPLAYS the job. It needs every id, not the one a dispute happens to address, and an
///   opening of one tile is exactly what it cannot replay from.
/// * a data-availability obligation is not a close. There is no carrier budget on this path — the
///   payload is served peer-to-peer on request, not relayed as a transaction — so the bytes an
///   opening would save are bytes nothing was competing for.
/// * under `PanelDa` this is the ONLY place the ids exist off the executor's disk. A payload that
///   carried less than the whole prompt would make withholding *representable as a well-formed
///   response*, which is precisely the failure Decision 16's disclosure rule exists to prevent.
///
/// So the encoding below is deliberately NOT optimised, and [`palw_fp_prompt_ids_admit_v1`] keeps
/// re-deriving the commitment over the whole list — under the form the ruleset names
/// (`Params::palw_prompt_ids_form_v1`: flat on every preset that ships the fence dormant, the
/// tiled Merkle root where a genesis armed it). Should the form ever move again, the
/// LIST is what is carried either way.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwFpMaterialV1 {
    pub job: PalwFreePromptJobV3,
    pub prompt_token_ids: Vec<u32>,
}

/// **The one check that comes before every other** (ADR-0077 Decision 16; W8 clauses 1 and 2).
///
/// `H(ids) == job.prompt_token_ids_hash`, and the count is the count the job declares. Under
/// `PublicDa` this re-derives a binding the chain already carries; under `PanelDa` it is the ONLY
/// thing that ties the bytes an executor served to the claim it is serving them for, so a seat
/// that read the capture first would be replaying an execution of a prompt nobody has shown is
/// this claim's.
///
/// It lives in consensus-core rather than in the seat because two programs run it — the seat
/// (`kaspad::palw_panel`) before it verifies material, and the payload decoders below — and a
/// second spelling of "the ids bind" is how two spellings come to disagree. It returns a NAMED
/// refusal rather than a bool for the same reason: a seat that files nothing must be able to say
/// which of "nothing was served" and "what was served is not this claim's" happened, because the
/// first is the producer's default arm and the second is a producer serving somebody else's work.
/// **The commitment's FORM is the network's, and every caller passes it in** (ADR-0081 Decision
/// 3 / ADR-0082 Decision 5; private-prompts design, 2026-09-05). `form` is
/// `Params::palw_prompt_ids_form_v1()` — the flat digest on every preset that ships the fence
/// dormant, the tiled Merkle root on a genesis that arms `palw_prompt_ids_merkle` — and the
/// comparison is `prompt_token_ids_match_v1`, the one spelling every writer's reader shares: the
/// seat, the three payload decoders, the worker-result rebinding and the backends' carried-prompt
/// checks. A reader that spelled the flat hash here would admit nothing on a Merkle network and
/// could not say why; a network whose writers and readers disagreed about the form would refuse
/// every honest commitment, which is why the fence is genesis-only and inside the ruleset id.
pub fn palw_fp_prompt_ids_admit_v1(
    job: &PalwFreePromptJobV3,
    prompt_token_ids: &[u32],
    form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<(), PalwFpV3Error> {
    if prompt_token_ids.len() != job.prompt_tokens as usize {
        return Err(PalwFpV3Error::PromptIdsCountMismatch { got: prompt_token_ids.len(), declared: job.prompt_tokens });
    }
    if !crate::palw_prompt_ids_v1::prompt_token_ids_match_v1(form, prompt_token_ids, &job.prompt_token_ids_hash) {
        return Err(PalwFpV3Error::PromptIdsHashMismatch);
    }
    Ok(())
}

/// **What a seat must establish before it reads a claim's capture, or files anything about it**
/// (ADR-0077 Decision 16; W8 clause 1: "a seat holding no ids cannot file `Valid`").
///
/// `held` is what the seat FETCHED — `None` when the executor served nothing, which under
/// `PanelDa` is the only way the ids can be missing (under `PublicDa` they ride the commitment
/// transaction). Withholding is NOT an arithmetic verdict: a seat reaching
/// [`PalwFpV3Error::PromptIdsUnavailable`] files the panel's `Unavailable` arm, and what that
/// reaches is the network's own question — [`crate::palw_panel_da_v1::palw_panel_da_withholding_arm_v1`]
/// answers it, and past ADR-0065 D4 the answer is an abstention that slashes nobody (ADR-0077
/// SA-5). Filing `Valid` here would certify a run the seat never checked, which is the one thing
/// this predicate exists to make unrepresentable.
pub fn palw_fp_seat_prompt_admit_v1(
    job: &PalwFreePromptJobV3,
    held: Option<&[u32]>,
    form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<(), PalwFpV3Error> {
    let Some(ids) = held else {
        return Err(PalwFpV3Error::PromptIdsUnavailable);
    };
    palw_fp_prompt_ids_admit_v1(job, ids, form)
}

/// Encode with the magic prefix. The inverse of [`palw_fp_material_decode_v1`].
pub fn palw_fp_material_encode_v1(job: &PalwFreePromptJobV3, prompt_token_ids: &[u32]) -> Vec<u8> {
    let body = borsh::to_vec(&PalwFpMaterialV1 { job: job.clone(), prompt_token_ids: prompt_token_ids.to_vec() })
        .expect("a material serializes");
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&PALW_FP_MATERIAL_V1_MAGIC);
    out.extend_from_slice(&body);
    out
}

/// Decode and re-check the ONE binding the payload itself can prove: the ids hash to the job's
/// `prompt_token_ids_hash`. Everything else (the roots) is the caller's to establish by
/// re-executing — a decoder that "validated" more would be trusting the bytes about facts only
/// an execution can witness. `None` for foreign magic, junk, or an ids/hash mismatch.
pub fn palw_fp_material_decode_v1(bytes: &[u8], form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1) -> Option<PalwFpMaterialV1> {
    let body = bytes.strip_prefix(&PALW_FP_MATERIAL_V1_MAGIC)?;
    let material: PalwFpMaterialV1 = borsh::from_slice(body).ok()?;
    // Through the shared predicate, not a second copy of it: the seat runs the same call on the
    // same bytes (ADR-0077 Decision 16), and one spelling is what keeps the two answers equal.
    palw_fp_prompt_ids_admit_v1(&material.job, &material.prompt_token_ids, form).ok()?;
    Some(material)
}

pub const PALW_FP_CAPTURE_V1_MAGIC: [u8; 4] = *b"FPC1";

/// **The answer beside the question** (ADR-0073 Decision 1a). `FPM1` carries the job the user
/// fixed on chain and the prompt that hashes to it — the question. A court needs the executor's
/// CAPTURE: the family material tuple `execute_free_prompt` computes into `outcome.material`,
/// which until this existed nothing persisted or gossiped. One payload holding both, so the
/// pool, the resolver and the relay budget treat a free-prompt claim's evidence exactly as an
/// attempt's — by claim id — and a seat or a court that holds the payload holds everything it
/// needs to check the claim without running it.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwFpCaptureV1 {
    pub material: PalwFpMaterialV1,
    /// The family capture, opaque here: the backend that ran it is what decodes it
    /// (`verify_material`, `capture_shape`, `refutation_for_free_prompt_index`).
    pub capture: Vec<u8>,
}

pub fn palw_fp_capture_encode_v1(job: &PalwFreePromptJobV3, prompt_token_ids: &[u32], capture: &[u8]) -> Vec<u8> {
    let body = borsh::to_vec(&PalwFpCaptureV1 {
        material: PalwFpMaterialV1 { job: job.clone(), prompt_token_ids: prompt_token_ids.to_vec() },
        capture: capture.to_vec(),
    })
    .expect("a capture payload serializes");
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&PALW_FP_CAPTURE_V1_MAGIC);
    out.extend_from_slice(&body);
    out
}

/// `None` for anything that is not an `FPC1` payload, for a job material that fails
/// [`palw_fp_material_decode_v1`]'s own checks (the prompt must hash to the job's and count to
/// its length), and for an empty capture — a question with no answer is `FPM1`, not this.
pub fn palw_fp_capture_decode_v1(bytes: &[u8], form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1) -> Option<PalwFpCaptureV1> {
    let body = bytes.strip_prefix(&PALW_FP_CAPTURE_V1_MAGIC)?;
    let payload: PalwFpCaptureV1 = borsh::from_slice(body).ok()?;
    // The ids bind BEFORE the capture is looked at, which is the order ADR-0077 Decision 16 makes
    // load-bearing under `PanelDa`: the served ids are the only tie between these bytes and the
    // claim, so a capture read first would be a replay of a prompt nobody has shown is this
    // claim's. Same predicate the seat calls.
    palw_fp_prompt_ids_admit_v1(&payload.material.job, &payload.material.prompt_token_ids, form).ok()?;
    if payload.capture.is_empty() {
        return None;
    }
    Some(payload)
}

pub const PALW_FP_ANSWER_V1_MAGIC: [u8; 4] = *b"FPA1";

/// **The answer's ids beside the question — and nothing that grows with the history** (ADR-0084
/// Decision 1).
///
/// `FPM1` is the question; `FPC1` is the question and the executor's whole capture. A seat on the
/// interval lane (ADR-0077 Decision 8, ADR-0082 Decision 9) needs neither the capture nor the
/// history: it recomputes the state from the prompt it holds and the ids the commitment binds, and
/// replays `k` openings fetched one at a time. What it could not get without the capture was the
/// ids — `fp_committed_output_ids_v1` read them off an `FPC1` payload's capture, so a claim whose
/// capture exceeded `PALW_MATERIAL_MAX_BYTES` (the graph-v5 fold is ~700 MB at 512) delivered
/// nothing to any seat and voided at its receipt deadline. This payload is the ids' own object:
/// the job, the prompt ids and the answer's ids, `4 × (prompt_tokens + decode_tokens_executed)`
/// bytes plus the job, inside the transport cap at every ladder the ruleset admits.
///
/// **What binds here and what binds at the seat.** The decoder proves the one thing the bytes can
/// prove themselves — the prompt ids hash to the job's `prompt_token_ids_hash`, exactly as `FPM1`
/// — and refuses an empty answer. The ANSWER's ids are bound by the seat to the chain: ADR-0078
/// X6's recompute, `output_commitment_v2(job_context_hash, ids, rendered_output_hash_for_family)`,
/// must equal the claim's committed `output_root` (`PalwExecutionBackendV1::fp_output_root_v1`).
/// That check belongs to the seat because only a family holds the context and the rendered-hash
/// rule; a decoder that claimed to have bound the answer would be trusting the bytes about a fact
/// only the class can witness.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwFpAnswerV1 {
    pub material: PalwFpMaterialV1,
    /// The answer, as the commitment's `output_root` commits it: `decode_tokens_executed` ids.
    pub output_token_ids: Vec<u32>,
}

pub fn palw_fp_answer_encode_v1(job: &PalwFreePromptJobV3, prompt_token_ids: &[u32], output_token_ids: &[u32]) -> Vec<u8> {
    let body = borsh::to_vec(&PalwFpAnswerV1 {
        material: PalwFpMaterialV1 { job: job.clone(), prompt_token_ids: prompt_token_ids.to_vec() },
        output_token_ids: output_token_ids.to_vec(),
    })
    .expect("an answer payload serializes");
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&PALW_FP_ANSWER_V1_MAGIC);
    out.extend_from_slice(&body);
    out
}

/// `None` for anything that is not an `FPA1` payload, for a job material that fails
/// [`palw_fp_material_decode_v1`]'s own checks, and for an empty answer — a question with no answer
/// is `FPM1`, not this. The answer's ids are NOT bound here (see the type's doc): a reader that
/// files anything on them binds them to the claim's `output_root` first.
pub fn palw_fp_answer_decode_v1(bytes: &[u8], form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1) -> Option<PalwFpAnswerV1> {
    let body = bytes.strip_prefix(&PALW_FP_ANSWER_V1_MAGIC)?;
    let payload: PalwFpAnswerV1 = borsh::from_slice(body).ok()?;
    palw_fp_prompt_ids_admit_v1(&payload.material.job, &payload.material.prompt_token_ids, form).ok()?;
    if payload.output_token_ids.is_empty() {
        return None;
    }
    Some(payload)
}

/// **The answer's ids of EITHER payload that carries them** — `FPA1` directly, or `FPC1` through
/// the family that wrote the capture (the seam, never a family decoder named by the caller).
/// `None` for `FPM1`, for junk, and for a capture this family cannot read. What is returned is
/// UNBOUND: the caller binds it to the claim's `output_root` before believing it.
pub fn palw_fp_committed_output_ids_decode_v1(
    bytes: &[u8],
    ids_of_capture: impl Fn(&[u8]) -> Option<Vec<u32>>,
    form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Option<Vec<u32>> {
    if let Some(answer) = palw_fp_answer_decode_v1(bytes, form) {
        return Some(answer.output_token_ids);
    }
    let payload = palw_fp_capture_decode_v1(bytes, form)?;
    ids_of_capture(&payload.capture).filter(|ids| !ids.is_empty())
}

/// **The privacy mode a free-prompt payload declares, read off its prefix and NOTHING else**
/// (ADR-0077 Decision 16's transport half; private-prompts design, 2026-09-05).
///
/// `FPM1`, `FPC1` and `FPA1` all begin with the job, so the mode is inside the first few hundred
/// bytes of any of them and needs no prompt-id form to read — which is the point: the transport
/// decides whether bytes may be announced, relayed or served to a stranger BEFORE it can know
/// whether they are honest, and a peek that had to authenticate the ids first would have to be
/// given the network's form by every relay in the mesh. Nothing here is believed for any other
/// purpose: a payload that claims mode 2 and is junk is dropped either way, and the decoders that
/// admit a payload still run the whole check. `None` for foreign bytes.
pub fn palw_fp_privacy_mode_peek_v1(bytes: &[u8]) -> Option<u8> {
    let body = bytes
        .strip_prefix(&PALW_FP_MATERIAL_V1_MAGIC)
        .or_else(|| bytes.strip_prefix(&PALW_FP_CAPTURE_V1_MAGIC))
        .or_else(|| bytes.strip_prefix(&PALW_FP_ANSWER_V1_MAGIC))?;
    let mut cursor = body;
    let job = <PalwFreePromptJobV3 as borsh::BorshDeserialize>::deserialize(&mut cursor).ok()?;
    Some(job.privacy_mode)
}

/// The job material of ANY free-prompt payload — `FPC1` (question and capture), `FPA1` (question
/// and the answer's ids, ADR-0084) or `FPM1` (question alone). Readers that need only the job and
/// the user's prompt take this.
pub fn palw_fp_job_material_decode_v1(bytes: &[u8], form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1) -> Option<PalwFpMaterialV1> {
    palw_fp_capture_decode_v1(bytes, form)
        .map(|payload| payload.material)
        .or_else(|| palw_fp_answer_decode_v1(bytes, form).map(|answer| answer.material))
        .or_else(|| palw_fp_material_decode_v1(bytes, form))
}

#[cfg(test)]
mod fp_material_tests {
    use super::*;

    fn job(ids: &[u32]) -> PalwFreePromptJobV3 {
        PalwFreePromptJobV3 {
            version: PALW_FP_V3_VERSION,
            network_domain: kaspa_hashes::Hash64::from_u64_word(9),
            class_id: kaspa_hashes::Hash64::from_u64_word(7),
            executor_bond: crate::tx::TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_u64_word(1), index: 0 },
            executor_pubkey: vec![7; 8],
            operator_id: kaspa_hashes::Hash64::from_u64_word(4),
            anchor_block: kaspa_hashes::Hash64::from_u64_word(0xA0),
            anchor_daa: 100,
            job_nonce: [0x5A; 32],
            tokenizer_id: kaspa_hashes::Hash64::default(),
            prompt_token_ids_hash: crate::palw_v2::prompt_token_ids_hash_v2(ids),
            prompt_tokens: ids.len() as u32,
            decode_token_limit: 3,
            max_context_tokens: 16,
            privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: PALW_FP_PROMPT_MODE_USER,
            sampling_seed: crate::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
            temperature_q: crate::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        }
    }

    /// **`FPC1` is `FPM1` with the answer attached** (ADR-0073 Decision 1a): it round-trips, the
    /// either-decoder reads the job material out of both payloads, and every check `FPM1` makes
    /// on the question is made here too — plus one: a payload with no capture is not a capture.
    #[test]
    fn a_capture_payload_round_trips_and_either_payload_yields_the_job_material() {
        let ids = [7u32, 11, 13];
        let job = job(&ids);
        let capture = vec![0xC4u8; 64];
        let bytes = palw_fp_capture_encode_v1(&job, &ids, &capture);
        assert_eq!(&bytes[..4], &PALW_FP_CAPTURE_V1_MAGIC);
        let decoded = palw_fp_capture_decode_v1(&bytes, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
            .expect("a capture payload decodes");
        assert_eq!(decoded.material.job, job);
        assert_eq!(decoded.material.prompt_token_ids, ids);
        assert_eq!(decoded.capture, capture);

        // The job material comes out of either spelling.
        assert_eq!(
            palw_fp_job_material_decode_v1(&bytes, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
                .expect("FPC1 yields its material")
                .job,
            job
        );
        let question_only = palw_fp_material_encode_v1(&job, &ids);
        assert_eq!(
            palw_fp_job_material_decode_v1(&question_only, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
                .expect("FPM1 yields its material")
                .job,
            job
        );
        // …and neither decoder reads the other's magic.
        assert!(
            palw_fp_capture_decode_v1(&question_only, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat).is_none(),
            "FPM1 is not a capture payload"
        );
        assert!(
            palw_fp_material_decode_v1(&bytes, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat).is_none(),
            "FPC1 is not a bare material"
        );

        // The question's checks still apply, and an empty answer is refused.
        let mut wrong = job.clone();
        wrong.prompt_token_ids_hash = Hash64::from_u64_word(0xBAD);
        assert!(
            palw_fp_capture_decode_v1(
                &palw_fp_capture_encode_v1(&wrong, &ids, &capture),
                crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
            )
            .is_none(),
            "ids must hash to the job"
        );
        assert!(
            palw_fp_capture_decode_v1(
                &palw_fp_capture_encode_v1(&job, &ids, &[]),
                crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
            )
            .is_none(),
            "no capture, no payload"
        );
    }

    #[test]
    fn a_material_round_trips_and_binds_its_ids() {
        let ids = [3u32, 9, 17];
        let bytes = palw_fp_material_encode_v1(&job(&ids), &ids);
        let back = palw_fp_material_decode_v1(&bytes, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat).expect("round trip");
        assert_eq!(back.prompt_token_ids, ids);
        assert_eq!(back.job, job(&ids));
    }

    /// **The blast radius of arming ADR-0081 Decision 3, derived rather than described.**
    ///
    /// `prompt_token_ids_hash` is a field of the JOB, so it is inside `fp_job_id_v3` and inside
    /// everything derived from it — the claim id, the spend id, the quantum ticket, the L1 tag —
    /// and it is a field of `PalwJobContextV2`, so it is inside `context_hash()` and therefore
    /// inside every step leaf, every leg root and the execution commitment. That is the whole
    /// reason arming the fence is a RE-MINT and not a schedule, and it is asserted here so the
    /// list cannot quietly fall behind the code.
    ///
    /// This test does NOT pin the armed values. There is no network to pin them for: the fence is
    /// `None` on every preset, and the single re-pin belongs to the genesis cut that arms it.
    #[test]
    fn arming_the_merkle_prompt_form_moves_every_id_derived_from_the_job() {
        use crate::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
        let ids = [7u32, 11, 13];
        let flat = job(&ids);
        assert_eq!(
            flat.prompt_token_ids_hash,
            prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, &ids).expect("the flat form is total"),
            "the shipped job commits the flat digest"
        );

        let mut merkle = flat.clone();
        merkle.prompt_token_ids_hash = prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::MerkleV1, &ids).expect("three ids commit");
        assert_ne!(flat.prompt_token_ids_hash, merkle.prompt_token_ids_hash, "the two forms are different commitments");
        assert_ne!(fp_job_id_v3(&flat), fp_job_id_v3(&merkle), "the job id moves");

        // And the same value inside the CONTEXT moves the execution side. One assertion for the
        // whole subtree, because `context_hash()` is what every leaf, leg and root binds.
        let mut ctx = crate::palw_v2::PalwJobContextV2 {
            version: crate::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: b"blast-radius".to_vec(),
            job_id: fp_job_id_v3(&flat),
            job_nullifier: Hash64::from_u64_word(2),
            assignment_id: Hash64::from_u64_word(3),
            execution_seed: [1; 32],
            model_profile_id: Hash64::from_u64_word(4),
            runtime_manifest_hash: Hash64::from_u64_word(5),
            runtime_class_id: Hash64::from_u64_word(6),
            shape_profile_id: Hash64::from_u64_word(7),
            trace_scheme_id: Hash64::from_u64_word(8),
            cu_ruleset_id: Hash64::from_u64_word(9),
            tokenizer_id: Hash64::from_u64_word(10),
            prompt_token_ids_hash: flat.prompt_token_ids_hash,
            declared_prefill_tokens: ids.len() as u32,
            exact_decode_tokens: 1,
            max_context_tokens: 16,
        };
        let flat_ctx_hash = ctx.context_hash();
        ctx.prompt_token_ids_hash = merkle.prompt_token_ids_hash;
        assert_ne!(flat_ctx_hash, ctx.context_hash(), "the job context hash moves, and every leaf and leg root with it");
    }

    /// **The DA payload still carries the ids WHOLE, and that is the design** (ADR-0081 Decision 3
    /// carve-out).
    ///
    /// Decision 3 makes a COURT CLOSE log-shaped in the context. This payload is not a close: a
    /// seat replaying the job needs every id, there is no carrier budget on a peer-to-peer fetch,
    /// and under `PanelDa` a payload carrying less than the whole prompt would make withholding
    /// representable as a well-formed response. So the encoding is linear in the prompt by
    /// intent, and this test is the tripwire for anyone who reads "the ids became a Merkle root"
    /// and shrinks the wrong thing.
    #[test]
    fn the_da_material_carries_every_prompt_id_and_is_deliberately_not_an_opening() {
        let short: Vec<u32> = (0..8u32).collect();
        let long: Vec<u32> = (0..512u32).collect();
        let a = palw_fp_material_encode_v1(&job(&short), &short);
        let b = palw_fp_material_encode_v1(&job(&long), &long);
        // Linear, not logarithmic: exactly four bytes per additional id and nothing else moved.
        assert_eq!(b.len() - a.len(), (long.len() - short.len()) * 4, "the payload is O(prompt) by design");
        // Every id round-trips — what a replaying seat needs and what one opened tile cannot give.
        assert_eq!(
            palw_fp_material_decode_v1(&b, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat).expect("round trip").prompt_token_ids,
            long
        );
        // …and the COURT's term at the same context is what Decision 3 actually shrank. The two
        // paths are priced differently on purpose; asserting the gap keeps that stated.
        let close = crate::palw_prompt_ids_v1::prompt_ids_close_bytes_v1(
            crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
            long.len() as u64,
        )
        .expect("512 ids open");
        assert_eq!(close, 472, "one tile plus a four-deep path plus the opening header");
        assert!(b.len() as u64 > 4 * close, "the DA payload is {} bytes against a {close}-byte close term", b.len());
    }

    /// The decoder refuses what it can check, and only that: swapped ids (the hash binding), a
    /// count that disagrees with the job, and the other lane's bytes.
    #[test]
    fn the_decoder_refuses_what_it_can_check() {
        let ids = [3u32, 9, 17];
        let mut swapped = palw_fp_material_encode_v1(&job(&ids), &[3, 17, 9]);
        assert!(
            palw_fp_material_decode_v1(&swapped, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat).is_none(),
            "ids that do not hash to the job's binding"
        );
        swapped.clear();
        assert!(palw_fp_material_decode_v1(&swapped, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat).is_none(), "empty");
        assert!(
            palw_fp_material_decode_v1(b"not a material", crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat).is_none(),
            "foreign bytes"
        );
        let mut wrong_count = job(&ids);
        wrong_count.prompt_tokens = 2;
        let body = borsh::to_vec(&PalwFpMaterialV1 { job: wrong_count, prompt_token_ids: ids.to_vec() }).unwrap();
        let mut bytes = PALW_FP_MATERIAL_V1_MAGIC.to_vec();
        bytes.extend_from_slice(&body);
        assert!(
            palw_fp_material_decode_v1(&bytes, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat).is_none(),
            "a count the ids contradict"
        );
    }
}

#[cfg(test)]
mod fp_answer_tests {
    use super::*;

    fn job(ids: &[u32]) -> PalwFreePromptJobV3 {
        PalwFreePromptJobV3 {
            version: PALW_FP_V3_VERSION,
            network_domain: kaspa_hashes::Hash64::from_u64_word(9),
            class_id: kaspa_hashes::Hash64::from_u64_word(7),
            executor_bond: crate::tx::TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_u64_word(1), index: 0 },
            executor_pubkey: vec![7; 8],
            operator_id: kaspa_hashes::Hash64::from_u64_word(4),
            anchor_block: kaspa_hashes::Hash64::from_u64_word(0xA0),
            anchor_daa: 100,
            job_nonce: [0x5A; 32],
            tokenizer_id: kaspa_hashes::Hash64::default(),
            prompt_token_ids_hash: crate::palw_v2::prompt_token_ids_hash_v2(ids),
            prompt_tokens: ids.len() as u32,
            decode_token_limit: 3,
            max_context_tokens: 16,
            privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: PALW_FP_PROMPT_MODE_USER,
            sampling_seed: crate::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
            temperature_q: crate::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        }
    }

    /// **`FPA1` is `FPM1` with the answer's ids attached** (ADR-0084 Decision 1): it round-trips,
    /// the either-decoder reads the job material out of it, the prompt binding is enforced, and an
    /// empty answer is not an answer.
    #[test]
    fn the_answer_envelope_round_trips_and_binds_the_prompt() {
        let prompt = [5u32, 6, 7];
        let job = job(&prompt);
        let answer = [11u32, 12, 13];
        let bytes = palw_fp_answer_encode_v1(&job, &prompt, &answer);
        assert_eq!(&bytes[..4], &PALW_FP_ANSWER_V1_MAGIC);
        let decoded = palw_fp_answer_decode_v1(&bytes, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat).expect("FPA1 decodes");
        assert_eq!(decoded.material.job, job);
        assert_eq!(decoded.material.prompt_token_ids, prompt);
        assert_eq!(decoded.output_token_ids, answer);
        assert_eq!(
            palw_fp_job_material_decode_v1(&bytes, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
                .expect("FPA1 yields its material")
                .job,
            job
        );
        assert!(
            palw_fp_capture_decode_v1(&bytes, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat).is_none(),
            "FPA1 is not a capture payload"
        );
        assert!(
            palw_fp_material_decode_v1(&bytes, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat).is_none(),
            "FPA1 is not a bare material"
        );
        // The prompt binding, through the shared predicate.
        assert!(
            palw_fp_answer_decode_v1(
                &palw_fp_answer_encode_v1(&job, &[5, 6, 8], &answer),
                crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
            )
            .is_none()
        );
        // A question with no answer is FPM1, not this.
        assert!(
            palw_fp_answer_decode_v1(
                &palw_fp_answer_encode_v1(&job, &prompt, &[]),
                crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
            )
            .is_none()
        );
        // Size: the job plus 4 x (prompt + answer) bytes plus borsh lengths — a few hundred bytes,
        // not a capture (ADR-0084 Y1's shape at this width).
        assert!(bytes.len() < 1_024, "{} bytes", bytes.len());
    }

    /// The ids come off `FPA1` directly and off `FPC1` only through the family seam; `FPM1` has
    /// none; an empty seam answer is `None`, never an empty answer.
    #[test]
    fn the_committed_ids_reader_takes_either_carrier_through_the_seam() {
        let prompt = [5u32, 6, 7];
        let job = job(&prompt);
        let answer = vec![11u32, 12, 13];
        let fpa = palw_fp_answer_encode_v1(&job, &prompt, &answer);
        let fpc = palw_fp_capture_encode_v1(&job, &prompt, b"family-capture");
        let fpm = palw_fp_material_encode_v1(&job, &prompt);
        let seam = |capture: &[u8]| (capture == b"family-capture").then(|| answer.clone());
        assert_eq!(
            palw_fp_committed_output_ids_decode_v1(&fpa, seam, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat),
            Some(answer.clone())
        );
        assert_eq!(
            palw_fp_committed_output_ids_decode_v1(&fpc, seam, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat),
            Some(answer.clone())
        );
        assert_eq!(palw_fp_committed_output_ids_decode_v1(&fpm, seam, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat), None);
        assert_eq!(
            palw_fp_committed_output_ids_decode_v1(&fpc, |_| Some(Vec::new()), crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat),
            None,
            "an empty seam answer is no answer"
        );
        assert_eq!(palw_fp_committed_output_ids_decode_v1(&fpc, |_| None, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat), None);
    }
}
