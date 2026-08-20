//! PALW full-logits trace scheme **v2** — the canonical types, domains and preimage layouts.
//!
//! Normative sources: `docs/palw-full-logits-trace-v2-design.md` (safety model, identifiers,
//! activation gates), `docs/misaka-palw-pow-detailed-design-v0.1-ja.md` §10–§11 (execution and
//! projection), `docs/misaka-palw-vps-canonical-worker-design-v0.1-ja.md` §5–§7 (job envelope,
//! canonical policy, trace binding). Where those documents disagreed, this file is the
//! reconciliation and the docs were amended to match it:
//!
//! * **Domain keys are uniformly `misaka-palw/<name>/v2`.** The v2 design's §6 draft wrote two of
//!   them as `misaka/palw/…`; the detailed design (§10.4–10.7), the VPS design (§7.2–7.5) and the
//!   scheme *name* itself (`misaka-palw/full-logits-trace/v2`) all use the hyphenated prefix, so
//!   that form is canonical.
//! * **Every logits event binds `job_context_hash`.** The v2 design's §6 event formula omitted
//!   it; the detailed design §10.5 and the VPS design §7.3 include it, and per-event context
//!   binding is strictly stronger (an event can never be replayed into another job, network,
//!   runtime or budget — not even at event granularity). The stronger rule wins.
//! * **The ordered event list is committed as a Merkle root, then bound inside a flat keyed
//!   outer hash.** The v2 design's §6 hashes the ordered event list directly; the detailed design
//!   §10.6 and VPS design §7.4 require a Merkle root (so a future TraceVM can open single events
//!   with log-size proofs). Both are satisfied: `trace_event_merkle_root_v2` commits the ordered
//!   list, and [`full_logits_trace_root_v2`] binds that root together with every metadata field
//!   §6 lists (directly or via `job_context_hash`).
//!
//! # Scope and stage — read this before wiring anything to consensus
//!
//! This module is **Land**-stage code (detailed design §24.1): types, hashing and tests only.
//! Nothing in consensus validation, fork choice, difficulty, emission or the header pipeline
//! consumes it, and it must stay that way until the staged activations (`Accept`, then `Value`)
//! each pass their own gates. The permitted operating envelope for everything here is
//! **devnet, shadow mode, and consensus-visible zero-credit observation** — no reward, no work,
//! no fork-choice weight (v2 design §1). A trace root is an **audited commitment**, not a
//! cryptographic proof of computation: any claimant can announce an arbitrary root, and only
//! canonical replay by independent bonded verifiers, bond/slashing, challenge windows and reward
//! maturity make a false one costly (v2 design §4).
//!
//! # The `2` that is not `pow_algo_id = 2`
//!
//! [`PALW_EXECUTION_ALGO_ID_V2`] is a **PALW-internal namespace value**. The header-level
//! `pow_algo_id = 2` is the historical Argon2id Layer-1 ([`crate::pow_layer0::POW_ALGO_ID_ARGON2ID`])
//! and is **not** reused, reassigned or aliased by this scheme. [`PalwExecutionAlgoId`]
//! deliberately implements no conversion to or from `u8`/header types so the two namespaces
//! cannot mix by coercion; the numeric coincidence is expected and harmless *because* the types
//! never meet.

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::Hash64;
use thiserror::Error;

// ---------------------------------------------------------------------------------------------
// Identifiers and namespaces
// ---------------------------------------------------------------------------------------------

/// The PALW-internal execution-algorithm identifier — a namespace **separate from the block
/// header's `pow_algo_id`**. No `From`/`Into`/`PartialEq` against integers or header fields is
/// provided, on purpose: v2 design §2.1 forbids mixing the namespaces by type conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwExecutionAlgoId(u8);

impl PalwExecutionAlgoId {
    /// The wire byte for PALW-internal serialization contexts only. Never write this into a
    /// block header, and never compare it with `pow_algo_id`.
    pub const fn wire_byte(self) -> u8 {
        self.0
    }
}

/// `palw_execution_algo_id = 2` (detailed design §0.1). PALW-internal; see the type docs.
pub const PALW_EXECUTION_ALGO_ID_V2: PalwExecutionAlgoId = PalwExecutionAlgoId(2);

/// The trace scheme's canonical name; [`trace_scheme_id_v2`] is derived from it.
pub const PALW_TRACE_SCHEME_NAME_V2: &str = "misaka-palw/full-logits-trace/v2";

/// `runtime_manifest_version` (v2 design §2.1).
///
/// **v3 (2026-08-17):** the manifest gained `libm_identity` + `libm_arithmetic_digest`
/// (mainnet-readiness audit B8 — ADR-0031 makes libm normative PoW arithmetic, and the manifest
/// never named it). A v2 manifest and a v3 manifest of the same host are deliberately *different*
/// class identities: v2 could not distinguish two libms, so its class claim was under-specified
/// and must not be honoured as equal to a v3 one.
pub const PALW_RUNTIME_MANIFEST_VERSION_V3: u16 = 3;
/// The superseded v2 manifest version — retained so historical manifests remain *parseable*
/// (their hashes are still derivable); it is not a version any new manifest may claim.
pub const PALW_RUNTIME_MANIFEST_VERSION_V2: u16 = 2;

/// The frozen probe vector for [`PalwRuntimeManifestV2::libm_arithmetic_digest`] — f32 bit
/// patterns, hashed as `expf(x)` then `logf(|x|)` outputs in this order.
///
/// Chosen to straddle the places float libms actually differ, not to be pretty: subnormal and
/// zero, the small-|x| region where `expf` is near 1 and cancellation shows, the ordinary decay
/// range the GDN gate lives in (ADR-0031's `glibc_expf_v1` call site), the argument-reduction
/// boundaries near `ln 2` multiples where table-vs-polynomial implementations diverge in the last
/// ulp, and the overflow/underflow edges. Frozen: changing this list changes every class id.
/// Measured on this vector (Apple libsystem_m, 2026-08-17): `expf` yields **11 distinct results
/// out of 12** with a single deliberate infinity, one subnormal, and `logf` 10 distinct — i.e.
/// nearly every entry carries discriminating information. An earlier draft saturated three entries
/// to `+inf`, which carry none; it was corrected before freezing, because changing this vector
/// later re-keys every registered class.
pub const PALW_LIBM_PROBE_V1: &[u32] = &[
    0x0000_0001, // smallest positive subnormal
    0x0080_0000, // smallest positive normal
    0x3F80_0000, // 1.0
    0x3F31_7218, // ln 2 — the argument-reduction boundary table and polynomial libms split on
    0x4038_AA3B, // ~2.885 (2/ln 2)
    0xBF80_0000, // -1.0
    0xC120_0000, // -10.0
    0xC2AF_0000, // -87.5 — expf lands in the SUBNORMAL range; gradual-underflow handling differs
    0x42B0_0000, // +88.0 — the largest finite expf, the hardest rounding case before overflow
    0x3727_C5AC, // 1e-5 — expf ≈ 1+x, where cancellation shows
    0x4120_0000, // 10.0 — ordinary range
    0x7F7F_FFFF, // f32::MAX — the logf edge (expf saturates here by design)
];
/// `trace_commitment_version` (v2 design §2.1).
pub const PALW_TRACE_COMMITMENT_VERSION_V2: u16 = 2;
/// The job envelope / job result wire version (VPS design §5.2).
pub const PALW_JOB_WIRE_VERSION_V2: u16 = 2;

/// Schema version of a registered golden vector set — **deliberately its own constant**, not
/// [`PALW_JOB_WIRE_VERSION_V2`].
///
/// A golden set is a LOCAL artifact: a file an operator generates on one host and registers via
/// `MISAKA_PALW_GOLDEN`. It never crosses the wire between peers. The job envelope / result /
/// health / capability documents do, and they are what `PALW_JOB_WIRE_VERSION_V2` speaks for.
/// Sharing one constant meant the set's schema could not evolve without invalidating every v2
/// wire message — including the agent↔worker UDS protocol — so the two are separated here.
///
/// `3` because the set gained `cmake_cache_sha256` + `llama_static_library_sha256` (see the
/// struct docs): sets written under the previous layout are refused with an explicit
/// `UnsupportedVersion` instead of a bare Borsh decode error.
pub const PALW_GOLDEN_SET_VERSION_V2: u16 = 3;

// ---------------------------------------------------------------------------------------------
// Domain-separation keys. All keyed BLAKE2b-512; a BLAKE2b key is at most 64 bytes, which the
// tests assert for every constant here so a future rename cannot panic at first use.
// ---------------------------------------------------------------------------------------------

/// Derives `trace_scheme_id` from the scheme name.
pub const PALW_V2_DOMAIN_TRACE_SCHEME_ID: &[u8] = b"misaka-palw/trace-scheme-id/v2";
/// Binds one job to its network, identifiers, seed, runtime and token budget (§7.2 VPS).
pub const PALW_V2_DOMAIN_JOB_CONTEXT: &[u8] = b"misaka-palw/job-context/v2";
/// Hash of the canonical prompt token ids (token ids are the execution identity, not raw text).
pub const PALW_V2_DOMAIN_PROMPT_TOKEN_IDS: &[u8] = b"misaka-palw/prompt-token-ids/v2";
/// One full-logits event: the entire vocab logit row after one canonical decode call.
pub const PALW_V2_DOMAIN_LOGITS_EVENT: &[u8] = b"misaka-palw/full-logits-event/v2";
/// Merkle leaf over `(index, event_hash)` — distinct from interior nodes by key, so a leaf can
/// never be confused with a node (the classic Merkle ambiguity classes are closed by
/// construction).
pub const PALW_V2_DOMAIN_TRACE_MERKLE_LEAF: &[u8] = b"misaka-palw/trace-merkle-leaf/v2";
/// Merkle interior node over `(left, right)`.
pub const PALW_V2_DOMAIN_TRACE_MERKLE_NODE: &[u8] = b"misaka-palw/trace-merkle-node/v2";
/// The outer trace root — `full_logits_sequence_root` (field name `full_logits_trace_root`).
pub const PALW_V2_DOMAIN_TRACE_ROOT: &[u8] = b"misaka-palw/full-logits-trace/v2";
/// Hash of the generated token-id sequence (bound inside the trace root).
pub const PALW_V2_DOMAIN_OUTPUT_TOKEN_IDS: &[u8] = b"misaka-palw/output-token-ids/v2";
/// Hash of the rendered (display) bytes — auxiliary; token ids are the consensus identity.
pub const PALW_V2_DOMAIN_RENDERED_OUTPUT: &[u8] = b"misaka-palw/rendered-output/v2";
/// The output commitment (§7.5 VPS / §10.7 detailed).
pub const PALW_V2_DOMAIN_OUTPUT: &[u8] = b"misaka-palw/output/v2";
/// Streaming operation-schedule commitment (one record per canonical decode call).
pub const PALW_V2_DOMAIN_SCHEDULE: &[u8] = b"misaka-palw/schedule/v2";
/// The runtime manifest hash (v2 design §7).
pub const PALW_V2_DOMAIN_RUNTIME_MANIFEST: &[u8] = b"misaka-palw/runtime-manifest/v2";
/// The CU ruleset identifier.
pub const PALW_V2_DOMAIN_CU_RULESET: &[u8] = b"misaka-palw/cu-ruleset/v2";
/// The shape profile identifier (over the canonical shape string the worker executes).
pub const PALW_V2_DOMAIN_SHAPE: &[u8] = b"misaka-palw/shape/v2";
/// The tokenizer identity (embedded in the pinned GGUF; see [`tokenizer_id_v2_for_gguf`]).
pub const PALW_V2_DOMAIN_TOKENIZER_ID: &[u8] = b"misaka-palw/tokenizer-id/v2";
/// Hash of a received job-request frame, echoed in the response (VPS §9.2).
pub const PALW_V2_DOMAIN_JOB_REQUEST: &[u8] = b"misaka-palw/job-request/v2";
/// Sentinel derivation for a manifest whose golden vectors are not yet populated.
pub const PALW_V2_DOMAIN_GOLDEN_VECTOR_ROOT: &[u8] = b"misaka-palw/golden-vector-root/v2";
/// Root of a registered golden vector set (the boot self-test corpus).
pub const PALW_V2_DOMAIN_GOLDEN_SET: &[u8] = b"misaka-palw/golden-vector-set/v2";
/// Deterministic per-name identifiers for golden self-test jobs.
pub const PALW_V2_DOMAIN_GOLDEN_JOB_ID: &[u8] = b"misaka-palw/golden-job-id/v2";
pub const PALW_V2_DOMAIN_GOLDEN_JOB_NULLIFIER: &[u8] = b"misaka-palw/golden-job-nullifier/v2";
pub const PALW_V2_DOMAIN_GOLDEN_ASSIGNMENT_ID: &[u8] = b"misaka-palw/golden-assignment-id/v2";

/// Every domain key in this module, for the length/uniqueness tests and for documentation
/// tooling. Keep in sync when adding a domain.
pub const PALW_V2_ALL_DOMAINS: &[&[u8]] = &[
    PALW_V2_DOMAIN_TRACE_SCHEME_ID,
    PALW_V2_DOMAIN_JOB_CONTEXT,
    PALW_V2_DOMAIN_PROMPT_TOKEN_IDS,
    PALW_V2_DOMAIN_LOGITS_EVENT,
    PALW_V2_DOMAIN_TRACE_MERKLE_LEAF,
    PALW_V2_DOMAIN_TRACE_MERKLE_NODE,
    PALW_V2_DOMAIN_TRACE_ROOT,
    PALW_V2_DOMAIN_OUTPUT_TOKEN_IDS,
    PALW_V2_DOMAIN_RENDERED_OUTPUT,
    PALW_V2_DOMAIN_OUTPUT,
    PALW_V2_DOMAIN_SCHEDULE,
    PALW_V2_DOMAIN_RUNTIME_MANIFEST,
    PALW_V2_DOMAIN_CU_RULESET,
    PALW_V2_DOMAIN_SHAPE,
    PALW_V2_DOMAIN_TOKENIZER_ID,
    PALW_V2_DOMAIN_JOB_REQUEST,
    PALW_V2_DOMAIN_GOLDEN_VECTOR_ROOT,
    PALW_V2_DOMAIN_GOLDEN_SET,
    PALW_V2_DOMAIN_GOLDEN_JOB_ID,
    PALW_V2_DOMAIN_GOLDEN_JOB_NULLIFIER,
    PALW_V2_DOMAIN_GOLDEN_ASSIGNMENT_ID,
];

// ---------------------------------------------------------------------------------------------
// Size caps — every variable-length input has a hard bound checked BEFORE it is read or hashed.
// ---------------------------------------------------------------------------------------------

/// Hard cap on one IPC frame (`u32-le length ‖ Borsh payload`, VPS §5.1). An envelope at the
/// maximum context is ~17 KiB; 256 KiB leaves headroom without letting a peer make us buffer
/// arbitrary bytes.
pub const PALW_V2_MAX_FRAME_BYTES: u32 = 256 * 1024;
/// Cap on `network_id` length inside the envelope and context preimage.
pub const PALW_V2_MAX_NETWORK_ID_BYTES: usize = 64;
/// Cap on the canonical prompt length in tokens (also bounded by `max_context_tokens`).
pub const PALW_V2_MAX_PROMPT_TOKENS: usize = 4096;
/// Cap on trace events per job (the initial calibration shape uses 16).
pub const PALW_V2_MAX_TRACE_EVENTS: usize = 4096;

// ---------------------------------------------------------------------------------------------
// Canonical encoding: explicit little-endian writer. Hash preimages are built with THIS, never
// with a serde/Borsh serializer, so the frozen byte layout cannot drift with a library version.
// Variable-length fields carry a u32-le length prefix — string concatenation is banned (v2 §6).
// ---------------------------------------------------------------------------------------------

struct CanonicalWriter(Vec<u8>);

impl CanonicalWriter {
    fn new() -> Self {
        Self(Vec::with_capacity(512))
    }
    fn put_u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn put_u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn put_u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn put_hash64(&mut self, v: &Hash64) {
        self.0.extend_from_slice(v.as_byte_slice());
    }
    fn put_fixed32(&mut self, v: &[u8; 32]) {
        self.0.extend_from_slice(v);
    }
    /// Length-prefixed variable bytes. Callers must have applied the relevant cap already; the
    /// `u32` prefix itself is the format-level bound.
    fn put_var_bytes(&mut self, v: &[u8]) {
        debug_assert!(v.len() <= u32::MAX as usize);
        self.put_u32(v.len() as u32);
        self.0.extend_from_slice(v);
    }
    fn put_var_str(&mut self, v: &str) {
        self.put_var_bytes(v.as_bytes());
    }
    /// Count-prefixed u32 sequence (token ids).
    fn put_u32_seq(&mut self, v: &[u32]) {
        debug_assert!(v.len() <= u32::MAX as usize);
        self.put_u32(v.len() as u32);
        for x in v {
            self.put_u32(*x);
        }
    }
    fn keyed64(self, domain: &[u8]) -> Hash64 {
        keyed64(domain, &[&self.0])
    }
}

/// Keyed BLAKE2b-512 over `parts`, domain-separated by `key` (same primitive the v1 worker and
/// the Layer-0 finalizer use).
fn keyed64(key: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for p in parts {
        h.update(p);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

// ---------------------------------------------------------------------------------------------
// Errors — the closed, fail-closed taxonomy. Parsing and validation never panic on untrusted
// input; every rejection is a variant so a supervisor can classify without string matching.
// ---------------------------------------------------------------------------------------------

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwV2Error {
    #[error("unsupported palw v2 wire version {got} (expected {expected})")]
    UnsupportedVersion { got: u16, expected: u16 },
    #[error("unknown palw v2 job mode byte")]
    UnknownMode,
    #[error("frame length {got} exceeds the {max}-byte cap")]
    OversizedFrame { got: u32, max: u32 },
    #[error("frame truncated: expected {expected} payload bytes, got {got}")]
    TruncatedFrame { expected: usize, got: usize },
    #[error("frame carries {0} trailing bytes after the declared payload")]
    TrailingBytes(usize),
    #[error("borsh decode failed: {0}")]
    Decode(String),
    #[error("network id is empty or exceeds {PALW_V2_MAX_NETWORK_ID_BYTES} bytes")]
    NetworkIdOutOfRange,
    #[error("prompt token ids are empty")]
    EmptyPrompt,
    #[error("prompt has {got} tokens, exceeding the {max}-token cap")]
    PromptTooLong { got: usize, max: usize },
    #[error("token id {token} at position {index} is not below the vocab size {n_vocab}")]
    TokenOutOfRange { index: usize, token: u32, n_vocab: u32 },
    #[error("exact_decode_tokens must be at least 1")]
    ZeroDecodeBudget,
    #[error("token budget overflows or exceeds max_context_tokens: prefill {prefill} + decode {decode} > {max_context}")]
    BudgetExceedsContext { prefill: u32, decode: u32, max_context: u32 },
    #[error("max_context_tokens {got} does not equal the activated profile value {profile}")]
    ContextProfileMismatch { got: u32, profile: u32 },
    #[error("trace has no events")]
    EmptyTrace,
    #[error("trace has {got} events, exceeding the {max}-event cap")]
    TooManyTraceEvents { got: usize, max: usize },
    #[error("logits event {event_index}: logit {logit_index} is not finite (fail closed, no receipt)")]
    NonFiniteLogit { event_index: u32, logit_index: usize },
    #[error("logits event carries {got} values but the declared vocab size is {expected}")]
    LogitsCountMismatch { got: usize, expected: usize },
    #[error("trace commitment is internally inconsistent: {0}")]
    InconsistentCommitment(String),
    #[error("trace event index {index} is not below the event count {count}")]
    EventIndexOutOfRange { index: u32, count: u32 },
    #[error("opening carries {got} siblings, exceeding the {max} a {PALW_V2_MAX_TRACE_EVENTS}-leaf tree can need")]
    OpeningTooDeep { got: usize, max: usize },
    #[error("opening path ended before reaching the root")]
    OpeningPathTooShort,
    #[error("opening path carries {extra} siblings past the root")]
    OpeningPathTooLong { extra: usize },
    #[error("runtime identity mismatch on {field}: envelope declares a runtime this worker is not")]
    RuntimeIdentityMismatch { field: &'static str },
    #[error("floating-point environment violates the canonical profile: {0}")]
    FpEnvironmentMismatch(String),
    #[error("golden vector set is invalid: {0}")]
    GoldenSetInvalid(String),
}

// ---------------------------------------------------------------------------------------------
// Scheme / profile identifiers
// ---------------------------------------------------------------------------------------------

/// `palw_trace_scheme_id = H(palw_trace_scheme_name)` (v2 design §2.1).
pub fn trace_scheme_id_v2() -> Hash64 {
    keyed64(PALW_V2_DOMAIN_TRACE_SCHEME_ID, &[PALW_TRACE_SCHEME_NAME_V2.as_bytes()])
}

/// The v2 CU ruleset. Same formula as v1 for now — the coefficient is calibration-pending and
/// any change is a new ruleset id, never an in-place edit (detailed design §8.5).
pub const PALW_CU_RULESET_V2: &str = "cu = prefill + 8*decode / v2";

pub fn cu_ruleset_id_v2() -> Hash64 {
    keyed64(PALW_V2_DOMAIN_CU_RULESET, &[PALW_CU_RULESET_V2.as_bytes()])
}

/// Canonical work units for one job under [`PALW_CU_RULESET_V2`]. Recomputed by every verifier
/// from the token counts — worker-reported FLOPs, wall time or utilization are never trusted
/// (detailed design §8.3).
pub fn canonical_compute_units_v2(prefill_tokens: u32, exact_decode_tokens: u32) -> u128 {
    prefill_tokens as u128 + 8 * exact_decode_tokens as u128
}

pub fn shape_profile_id_v2(shape_string: &str) -> Hash64 {
    keyed64(PALW_V2_DOMAIN_SHAPE, &[shape_string.as_bytes()])
}

/// The tokenizer identity for a profile whose tokenizer is embedded in the pinned GGUF: the
/// artifact digest IS the tokenizer identity. A standalone tokenizer artifact would get its own
/// digest here instead.
pub fn tokenizer_id_v2_for_gguf(gguf_sha256_hex: &str) -> Hash64 {
    keyed64(PALW_V2_DOMAIN_TOKENIZER_ID, &[b"embedded-in-gguf/", gguf_sha256_hex.as_bytes()])
}

/// Hash of a received request frame's Borsh payload, echoed in [`PalwJobResultV2::request_hash`]
/// so a response is bound to the exact request bytes it answers (VPS §9.2).
pub fn job_request_hash_v2(request_payload: &[u8]) -> Hash64 {
    keyed64(PALW_V2_DOMAIN_JOB_REQUEST, &[request_payload])
}

/// The explicit "no golden vectors registered yet" sentinel. A manifest carrying this value is
/// visibly incomplete — class registration (v2 design §10) must replace it with the root of real
/// full-64-byte vectors; a zeroed field would look like data.
pub fn golden_vector_root_unpopulated_v2() -> Hash64 {
    keyed64(PALW_V2_DOMAIN_GOLDEN_VECTOR_ROOT, &[b"unpopulated/v2"])
}

// ---------------------------------------------------------------------------------------------
// Job envelope (VPS design §5.2) — the canonical Borsh IPC object, and its validation predicate.
// ---------------------------------------------------------------------------------------------

/// Execute vs Replay. Same computation by construction — a verifier's replay *is* the executor's
/// job recomputed; the mode exists for bookkeeping and scheduling, never for the arithmetic.
/// Unknown discriminants fail Borsh decoding (fail closed).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
pub enum PalwJobModeV2 {
    Execute = 0,
    Replay = 1,
}

/// The canonical job a supervisor hands a worker. Token ids are the execution identity — the
/// worker never tokenizes, normalizes or templates text on this path (VPS §5.3).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwJobEnvelopeV2 {
    pub version: u16,
    pub network_id: Vec<u8>,
    pub job_id: Hash64,
    pub job_nullifier: Hash64,
    pub mode: PalwJobModeV2,

    pub model_profile_id: Hash64,
    pub runtime_manifest_hash: Hash64,
    pub runtime_class_id: Hash64,
    pub shape_profile_id: Hash64,
    pub trace_scheme_id: Hash64,
    pub cu_ruleset_id: Hash64,

    /// **Audit H7's free field, and it is still free here.** Nothing in carriage inspects it, so
    /// a supervisor may set any value and the entropy gate item 5 asks about would be measuring a
    /// number the job's own author chose.
    ///
    /// Scope, corrected 2026-08-20: this is the SUPERVISOR's job object for the replay/legs
    /// paths. The V2 attempt lane has no `execution_seed` — `PalwAttemptUnsignedV2` binds
    /// `challenge_v2` over the header's own position instead, and the finalizer refuses a
    /// mismatch — and the free-prompt lane derives its seed from the job's chain anchor
    /// (`palw_fp_execution_seed_v3`). This envelope is the one place the hole remains, and it
    /// reaches consensus through [`PalwJobContextV2::from_envelope`].
    pub execution_seed: [u8; 32],
    pub prompt_token_ids: Vec<u32>,
    pub exact_decode_tokens: u32,
    pub max_context_tokens: u32,

    pub assignment_id: Hash64,
    pub assignment_epoch: u64,
    /// `0` means "no deadline" and is permitted only in harness/dev flows; production
    /// supervisors always set it and additionally enforce the kill themselves.
    pub deadline_unix_ms: u64,
}

impl PalwJobEnvelopeV2 {
    /// Structural validation that needs no model: version, caps, budget arithmetic
    /// (checked — an overflowing budget is a rejection, not a wrap), profile context equality.
    /// Token-range validation needs the vocab size and lives in [`Self::validate_against_vocab`].
    pub fn validate_shape(&self, profile_max_context_tokens: u32) -> Result<(), PalwV2Error> {
        if self.version != PALW_JOB_WIRE_VERSION_V2 {
            return Err(PalwV2Error::UnsupportedVersion { got: self.version, expected: PALW_JOB_WIRE_VERSION_V2 });
        }
        if self.network_id.is_empty() || self.network_id.len() > PALW_V2_MAX_NETWORK_ID_BYTES {
            return Err(PalwV2Error::NetworkIdOutOfRange);
        }
        if self.prompt_token_ids.is_empty() {
            return Err(PalwV2Error::EmptyPrompt);
        }
        if self.prompt_token_ids.len() > PALW_V2_MAX_PROMPT_TOKENS {
            return Err(PalwV2Error::PromptTooLong { got: self.prompt_token_ids.len(), max: PALW_V2_MAX_PROMPT_TOKENS });
        }
        if self.exact_decode_tokens == 0 {
            return Err(PalwV2Error::ZeroDecodeBudget);
        }
        if self.max_context_tokens != profile_max_context_tokens {
            return Err(PalwV2Error::ContextProfileMismatch { got: self.max_context_tokens, profile: profile_max_context_tokens });
        }
        let prefill = self.prompt_token_ids.len() as u32;
        match prefill.checked_add(self.exact_decode_tokens) {
            Some(total) if total <= self.max_context_tokens => Ok(()),
            _ => Err(PalwV2Error::BudgetExceedsContext {
                prefill,
                decode: self.exact_decode_tokens,
                max_context: self.max_context_tokens,
            }),
        }
    }

    /// Every prompt token id must be a real row of the model's vocab.
    pub fn validate_against_vocab(&self, n_vocab: u32) -> Result<(), PalwV2Error> {
        for (index, token) in self.prompt_token_ids.iter().enumerate() {
            if *token >= n_vocab {
                return Err(PalwV2Error::TokenOutOfRange { index, token: *token, n_vocab });
            }
        }
        Ok(())
    }

    pub fn declared_prefill_tokens(&self) -> u32 {
        self.prompt_token_ids.len() as u32
    }
}

// ---------------------------------------------------------------------------------------------
// Job context — the single binding value every event, output and root chains back to.
// ---------------------------------------------------------------------------------------------

/// The full binding context of one canonical execution (VPS design §7.2, plus `tokenizer_id`
/// which v2 design §6 requires the commitment to carry). Everything a replayed event could be
/// smuggled across — network, job, assignment, seed, model, runtime, class, shape, scheme,
/// CU rule, tokenizer, prompt, budgets — is inside this one hash.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwJobContextV2 {
    pub version: u16,
    pub network_id: Vec<u8>,
    pub job_id: Hash64,
    pub job_nullifier: Hash64,
    pub assignment_id: Hash64,
    pub execution_seed: [u8; 32],
    pub model_profile_id: Hash64,
    pub runtime_manifest_hash: Hash64,
    pub runtime_class_id: Hash64,
    pub shape_profile_id: Hash64,
    pub trace_scheme_id: Hash64,
    pub cu_ruleset_id: Hash64,
    pub tokenizer_id: Hash64,
    pub prompt_token_ids_hash: Hash64,
    pub declared_prefill_tokens: u32,
    pub exact_decode_tokens: u32,
    pub max_context_tokens: u32,
}

/// Hash of the canonical prompt token ids.
pub fn prompt_token_ids_hash_v2(prompt_token_ids: &[u32]) -> Hash64 {
    let mut w = CanonicalWriter::new();
    w.put_u32_seq(prompt_token_ids);
    w.keyed64(PALW_V2_DOMAIN_PROMPT_TOKEN_IDS)
}

impl PalwJobContextV2 {
    /// Builds the context from a validated envelope plus the identities the envelope does not
    /// carry explicitly. Call [`PalwJobEnvelopeV2::validate_shape`] first.
    pub fn from_envelope(envelope: &PalwJobEnvelopeV2, tokenizer_id: Hash64) -> Self {
        Self {
            version: PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: envelope.network_id.clone(),
            job_id: envelope.job_id,
            job_nullifier: envelope.job_nullifier,
            assignment_id: envelope.assignment_id,
            execution_seed: envelope.execution_seed,
            model_profile_id: envelope.model_profile_id,
            runtime_manifest_hash: envelope.runtime_manifest_hash,
            runtime_class_id: envelope.runtime_class_id,
            shape_profile_id: envelope.shape_profile_id,
            trace_scheme_id: envelope.trace_scheme_id,
            cu_ruleset_id: envelope.cu_ruleset_id,
            tokenizer_id,
            prompt_token_ids_hash: prompt_token_ids_hash_v2(&envelope.prompt_token_ids),
            declared_prefill_tokens: envelope.declared_prefill_tokens(),
            exact_decode_tokens: envelope.exact_decode_tokens,
            max_context_tokens: envelope.max_context_tokens,
        }
    }

    /// The frozen preimage layout. Field order is the struct order; variable-length fields are
    /// length-prefixed; integers are little-endian. Golden vectors in the tests pin these bytes.
    pub fn context_hash(&self) -> Hash64 {
        let mut w = CanonicalWriter::new();
        w.put_u16(self.version);
        w.put_var_bytes(&self.network_id);
        w.put_hash64(&self.job_id);
        w.put_hash64(&self.job_nullifier);
        w.put_hash64(&self.assignment_id);
        w.put_fixed32(&self.execution_seed);
        w.put_hash64(&self.model_profile_id);
        w.put_hash64(&self.runtime_manifest_hash);
        w.put_hash64(&self.runtime_class_id);
        w.put_hash64(&self.shape_profile_id);
        w.put_hash64(&self.trace_scheme_id);
        w.put_hash64(&self.cu_ruleset_id);
        w.put_hash64(&self.tokenizer_id);
        w.put_hash64(&self.prompt_token_ids_hash);
        w.put_u32(self.declared_prefill_tokens);
        w.put_u32(self.exact_decode_tokens);
        w.put_u32(self.max_context_tokens);
        w.keyed64(PALW_V2_DOMAIN_JOB_CONTEXT)
    }
}

// ---------------------------------------------------------------------------------------------
// Logits events and the trace root
// ---------------------------------------------------------------------------------------------

/// Which canonical call produced an event. `Prefill` is the single batch that feeds the prompt;
/// every later call is `Decode`. Wire byte is the discriminant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
pub enum PalwTracePhaseV2 {
    Prefill = 0,
    Decode = 1,
}

impl PalwTracePhaseV2 {
    pub const fn wire_byte(self) -> u8 {
        match self {
            PalwTracePhaseV2::Prefill => 0,
            PalwTracePhaseV2::Decode => 1,
        }
    }
}

/// The only logits dtype this profile admits: IEEE-754 binary32, little-endian, raw bits
/// (signed zero preserved, non-finite rejected before hashing).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
pub enum PalwLogitsDtypeV2 {
    F32Le = 0,
}

impl PalwLogitsDtypeV2 {
    pub const fn wire_byte(self) -> u8 {
        0
    }
}

/// Why the job stopped. Under the exact-decode policy the ONLY receipt-bearing stop is the
/// budget itself: early EOG never terminates execution (it is telemetry), and every other
/// termination is a failure with no receipt. The u16 width matches the projection wire field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
pub enum PalwStopReasonV2 {
    ExactBudgetReached = 0,
}

impl PalwStopReasonV2 {
    pub const fn wire_u16(self) -> u16 {
        0
    }
}

/// One full-logits event: keyed hash over the job context, the call's phase and step, and the
/// entire logit row as canonical little-endian f32 bytes. Any non-finite value anywhere in the
/// row invalidates the execution — fail closed, no receipt (v2 design §8).
///
/// `scratch` is a caller-owned buffer reused across events (a row is `n_vocab × 4` bytes,
/// ~1 MB at the measured vocab); it is overwritten, never read.
pub fn logits_event_hash_v2(
    job_context_hash: &Hash64,
    phase: PalwTracePhaseV2,
    phase_step: u32,
    event_index: u32,
    n_vocab: u32,
    logits: &[f32],
    scratch: &mut Vec<u8>,
) -> Result<Hash64, PalwV2Error> {
    if logits.len() != n_vocab as usize {
        return Err(PalwV2Error::LogitsCountMismatch { got: logits.len(), expected: n_vocab as usize });
    }
    scratch.clear();
    scratch.reserve(logits.len() * 4);
    for (logit_index, l) in logits.iter().enumerate() {
        if !l.is_finite() {
            return Err(PalwV2Error::NonFiniteLogit { event_index, logit_index });
        }
        scratch.extend_from_slice(&l.to_le_bytes());
    }
    let mut w = CanonicalWriter::new();
    w.put_hash64(job_context_hash);
    w.put_u8(phase.wire_byte());
    w.put_u32(phase_step);
    w.put_u32(n_vocab);
    w.put_u8(PalwLogitsDtypeV2::F32Le.wire_byte());
    w.put_u32(logits.len() as u32);
    let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_V2_DOMAIN_LOGITS_EVENT).to_state();
    h.update(&w.0);
    h.update(scratch);
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Ok(Hash64::from_bytes(out))
}

/// Domain-separated binary Merkle root over the ordered event hashes.
///
/// * Leaves and interior nodes use different keys, so no leaf can be reinterpreted as a node.
/// * Each leaf binds its index, so reordering is a different root even under tree-shape games.
/// * An odd node is **promoted unchanged** — never duplicated, which closes the classic
///   duplicate-leaf ambiguity (CVE-2012-2459 class).
/// * The event *count* is bound by the outer root ([`full_logits_trace_root_v2`]), so trees of
///   different sizes that happen to collide in shape still cannot collide as commitments.
pub fn trace_event_merkle_root_v2(ordered_event_hashes: &[Hash64]) -> Result<Hash64, PalwV2Error> {
    if ordered_event_hashes.is_empty() {
        return Err(PalwV2Error::EmptyTrace);
    }
    if ordered_event_hashes.len() > PALW_V2_MAX_TRACE_EVENTS {
        return Err(PalwV2Error::TooManyTraceEvents { got: ordered_event_hashes.len(), max: PALW_V2_MAX_TRACE_EVENTS });
    }
    let mut level: Vec<Hash64> = ordered_event_hashes
        .iter()
        .enumerate()
        .map(|(i, ev)| {
            let mut w = CanonicalWriter::new();
            w.put_u32(i as u32);
            w.put_hash64(ev);
            w.keyed64(PALW_V2_DOMAIN_TRACE_MERKLE_LEAF)
        })
        .collect();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut chunks = level.chunks_exact(2);
        for pair in &mut chunks {
            next.push(keyed64(PALW_V2_DOMAIN_TRACE_MERKLE_NODE, &[pair[0].as_byte_slice(), pair[1].as_byte_slice()]));
        }
        if let [odd] = chunks.remainder() {
            next.push(*odd);
        }
        level = next;
    }
    Ok(level[0])
}

/// The deepest opening a [`PALW_V2_MAX_TRACE_EVENTS`]-leaf tree can require: one sibling per
/// level, and odd promotion only ever removes levels from a path.
pub const PALW_V2_MAX_TRACE_OPENING_SIBLINGS: usize = PALW_V2_MAX_TRACE_EVENTS.ilog2() as usize + 1;

/// A membership proof for one trace event under [`trace_event_merkle_root_v2`].
///
/// **This is what "what samplers open" in [`crate::palw_block_commitment::PalwBlockCommitmentV1`]
/// means.** A block commitment whose trace root cannot be opened is a block no sampler can
/// challenge: every dispute against it terminates `Unadjudicable`, which under ADR-0038 I10 is
/// rejected-but-unslashed and freezes the class. Committing to a root with no opening API is
/// therefore strictly worse than committing to nothing — it mints work nothing can be held to.
///
/// The tree already had that shape; only the proof was missing. `palw_step_leg` carries the same
/// construction for the step leg, and this mirrors it deliberately rather than inventing a second
/// convention: index-bound leaves, distinct leaf and node keys, and an odd node **promoted
/// unchanged** so the duplicate-leaf ambiguity (CVE-2012-2459 class) has no room.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTraceEventOpeningV2 {
    /// Position of the opened event in the ordered list — bound into the leaf, so an opening
    /// cannot be replayed at another index.
    pub event_index: u32,
    /// The event hash itself ([`logits_event_hash_v2`]'s output), NOT the Merkle leaf: the leaf is
    /// derived here so a prover cannot hand over a leaf whose index binding it chose.
    pub event_hash: Hash64,
    /// Sibling hashes bottom-up. Promoted levels consume none, which is why the verifier derives
    /// promotion from `(index, count)` rather than trusting the path's length.
    pub siblings: Vec<Hash64>,
}

/// Produces the membership proof of `event_index` from the same ordered event hashes the root was
/// built over.
pub fn trace_event_opening_v2(ordered_event_hashes: &[Hash64], event_index: u32) -> Result<PalwTraceEventOpeningV2, PalwV2Error> {
    let count = ordered_event_hashes.len();
    if count == 0 {
        return Err(PalwV2Error::EmptyTrace);
    }
    if count > PALW_V2_MAX_TRACE_EVENTS {
        return Err(PalwV2Error::TooManyTraceEvents { got: count, max: PALW_V2_MAX_TRACE_EVENTS });
    }
    if event_index as usize >= count {
        return Err(PalwV2Error::EventIndexOutOfRange { index: event_index, count: count as u32 });
    }
    let event_hash = ordered_event_hashes[event_index as usize];
    let mut level: Vec<Hash64> = ordered_event_hashes
        .iter()
        .enumerate()
        .map(|(i, ev)| {
            let mut w = CanonicalWriter::new();
            w.put_u32(i as u32);
            w.put_hash64(ev);
            w.keyed64(PALW_V2_DOMAIN_TRACE_MERKLE_LEAF)
        })
        .collect();
    let mut position = event_index as usize;
    let mut siblings = Vec::new();
    while level.len() > 1 {
        let width = level.len();
        let promoted = !width.is_multiple_of(2) && position == width - 1;
        if !promoted {
            siblings.push(level[position ^ 1]);
        }
        let mut next = Vec::with_capacity(width.div_ceil(2));
        let mut chunks = level.chunks_exact(2);
        for pair in &mut chunks {
            next.push(keyed64(PALW_V2_DOMAIN_TRACE_MERKLE_NODE, &[pair[0].as_byte_slice(), pair[1].as_byte_slice()]));
        }
        if let [odd] = chunks.remainder() {
            next.push(*odd);
        }
        position /= 2;
        level = next;
    }
    Ok(PalwTraceEventOpeningV2 { event_index, event_hash, siblings })
}

/// Recomputes the root a valid opening implies; the caller compares it against the committed one.
///
/// `event_count` comes from the commitment's own summary, not from the opening — the count is
/// bound by [`full_logits_trace_root_v2`], so a prover cannot restate it. Promotion is derived
/// from `(index, count)` and consumes no sibling, and a path with anything left over is refused:
/// accepting a longer-than-necessary path would let one event hash prove membership at more than
/// one position.
pub fn trace_event_opening_root_v2(event_count: u32, opening: &PalwTraceEventOpeningV2) -> Result<Hash64, PalwV2Error> {
    if event_count == 0 {
        return Err(PalwV2Error::EmptyTrace);
    }
    if event_count as usize > PALW_V2_MAX_TRACE_EVENTS {
        return Err(PalwV2Error::TooManyTraceEvents { got: event_count as usize, max: PALW_V2_MAX_TRACE_EVENTS });
    }
    if opening.event_index >= event_count {
        return Err(PalwV2Error::EventIndexOutOfRange { index: opening.event_index, count: event_count });
    }
    if opening.siblings.len() > PALW_V2_MAX_TRACE_OPENING_SIBLINGS {
        return Err(PalwV2Error::OpeningTooDeep { got: opening.siblings.len(), max: PALW_V2_MAX_TRACE_OPENING_SIBLINGS });
    }
    let mut current = {
        let mut w = CanonicalWriter::new();
        w.put_u32(opening.event_index);
        w.put_hash64(&opening.event_hash);
        w.keyed64(PALW_V2_DOMAIN_TRACE_MERKLE_LEAF)
    };
    let mut position = opening.event_index as usize;
    let mut width = event_count as usize;
    let mut siblings = opening.siblings.iter();
    while width > 1 {
        let promoted = !width.is_multiple_of(2) && position == width - 1;
        if !promoted {
            let Some(sibling) = siblings.next() else {
                return Err(PalwV2Error::OpeningPathTooShort);
            };
            current = if position.is_multiple_of(2) {
                keyed64(PALW_V2_DOMAIN_TRACE_MERKLE_NODE, &[current.as_byte_slice(), sibling.as_byte_slice()])
            } else {
                keyed64(PALW_V2_DOMAIN_TRACE_MERKLE_NODE, &[sibling.as_byte_slice(), current.as_byte_slice()])
            };
        }
        position /= 2;
        width = width.div_ceil(2);
    }
    let leftover = siblings.count();
    if leftover != 0 {
        return Err(PalwV2Error::OpeningPathTooLong { extra: leftover });
    }
    Ok(current)
}

/// The metadata half of the trace root: everything the detailed design §10.6 requires the root
/// to bind beyond the events themselves.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTraceSummaryV2 {
    pub vocab_size: u32,
    pub logits_dtype: PalwLogitsDtypeV2,
    pub declared_prefill_tokens: u32,
    pub exact_decode_tokens: u32,
    pub event_count: u32,
    pub first_event_kind: PalwTracePhaseV2,
    pub last_event_kind: PalwTracePhaseV2,
    pub output_token_ids_hash: Hash64,
    pub stop_reason: PalwStopReasonV2,
}

/// Hash of the generated token ids (bound inside the trace root AND inside the output
/// commitment — the two commitments must not be separable).
pub fn output_token_ids_hash_v2(generated_token_ids: &[u32]) -> Hash64 {
    let mut w = CanonicalWriter::new();
    w.put_u32_seq(generated_token_ids);
    w.keyed64(PALW_V2_DOMAIN_OUTPUT_TOKEN_IDS)
}

/// `full_logits_sequence_root` — the outer keyed hash binding the job context, the trace
/// metadata and the Merkle commitment of the ordered events (reconciliation of v2 design §6 with
/// detailed design §10.6; see the module docs).
pub fn full_logits_trace_root_v2(
    job_context_hash: &Hash64,
    summary: &PalwTraceSummaryV2,
    ordered_event_commitment: &Hash64,
) -> Hash64 {
    let mut w = CanonicalWriter::new();
    w.put_u16(PALW_TRACE_COMMITMENT_VERSION_V2);
    w.put_hash64(job_context_hash);
    w.put_u32(summary.vocab_size);
    w.put_u8(summary.logits_dtype.wire_byte());
    w.put_u32(summary.declared_prefill_tokens);
    w.put_u32(summary.exact_decode_tokens);
    w.put_u32(summary.event_count);
    w.put_u8(summary.first_event_kind.wire_byte());
    w.put_u8(summary.last_event_kind.wire_byte());
    w.put_hash64(ordered_event_commitment);
    w.put_hash64(&summary.output_token_ids_hash);
    w.put_u16(summary.stop_reason.wire_u16());
    w.keyed64(PALW_V2_DOMAIN_TRACE_ROOT)
}

/// Hash of the rendered display bytes (auxiliary — token ids are the identity).
pub fn rendered_output_hash_v2(rendered: &[u8]) -> Hash64 {
    let mut w = CanonicalWriter::new();
    w.put_var_bytes(rendered);
    w.keyed64(PALW_V2_DOMAIN_RENDERED_OUTPUT)
}

/// The output commitment (§10.7 detailed / §7.5 VPS): context-bound token ids plus the rendered
/// hash.
pub fn output_commitment_v2(job_context_hash: &Hash64, generated_token_ids: &[u32], rendered_output_hash: &Hash64) -> Hash64 {
    let mut w = CanonicalWriter::new();
    w.put_hash64(job_context_hash);
    w.put_u32_seq(generated_token_ids);
    w.put_hash64(rendered_output_hash);
    w.keyed64(PALW_V2_DOMAIN_OUTPUT)
}

// ---------------------------------------------------------------------------------------------
// Operation schedule commitment
// ---------------------------------------------------------------------------------------------

/// Streaming commitment over the canonical call schedule: `(call_index, batch_len)` per decode
/// call, context-bound. For this profile the schedule is a pure function of the shape —
/// [`expected_schedule_commitment_v2`] recomputes it without running the model, and a worker
/// asserts its streamed value equals the expectation before emitting a receipt.
pub struct PalwScheduleCommitmentBuilderV2 {
    state: blake2b_simd::State,
    calls: u32,
}

impl PalwScheduleCommitmentBuilderV2 {
    pub fn new(job_context_hash: &Hash64) -> Self {
        let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_V2_DOMAIN_SCHEDULE).to_state();
        state.update(job_context_hash.as_byte_slice());
        Self { state, calls: 0 }
    }

    pub fn record_call(&mut self, batch_len: u32) {
        self.state.update(&self.calls.to_le_bytes());
        self.state.update(&batch_len.to_le_bytes());
        self.calls += 1;
    }

    /// Returns `(commitment, call_count)`.
    pub fn finalize(self) -> (Hash64, u32) {
        let mut out = [0u8; 64];
        out.copy_from_slice(self.state.finalize().as_bytes());
        (Hash64::from_bytes(out), self.calls)
    }
}

/// The schedule this profile mandates: one prefill batch of `prefill_tokens`, then single-token
/// decode calls. With `exact_decode_tokens = D` and the last token never fed back, the call
/// count is `D` (1 prefill + D−1 decode) and the event count equals the call count.
pub fn expected_schedule_commitment_v2(job_context_hash: &Hash64, prefill_tokens: u32, exact_decode_tokens: u32) -> (Hash64, u32) {
    let mut b = PalwScheduleCommitmentBuilderV2::new(job_context_hash);
    b.record_call(prefill_tokens);
    for _ in 1..exact_decode_tokens {
        b.record_call(1);
    }
    b.finalize()
}

// ---------------------------------------------------------------------------------------------
// Trace commitment and result projection
// ---------------------------------------------------------------------------------------------

/// TraceCommitmentV2 (v2 design §6): the self-contained trace object. Carries the full ordered
/// event-hash list so an auditor can localize a divergence to one event; the root alone travels
/// in the projection.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTraceCommitmentV2 {
    pub context: PalwJobContextV2,
    pub summary: PalwTraceSummaryV2,
    pub ordered_event_hashes: Vec<Hash64>,
    pub full_logits_sequence_root: Hash64,
}

impl PalwTraceCommitmentV2 {
    /// Assembles and roots a commitment, enforcing this profile's invariants.
    pub fn assemble(
        context: PalwJobContextV2,
        summary: PalwTraceSummaryV2,
        ordered_event_hashes: Vec<Hash64>,
    ) -> Result<Self, PalwV2Error> {
        let commitment_root = trace_event_merkle_root_v2(&ordered_event_hashes)?;
        let context_hash = context.context_hash();
        Self::check_summary(&context, &summary, ordered_event_hashes.len())?;
        let full_logits_sequence_root = full_logits_trace_root_v2(&context_hash, &summary, &commitment_root);
        Ok(Self { context, summary, ordered_event_hashes, full_logits_sequence_root })
    }

    fn check_summary(context: &PalwJobContextV2, summary: &PalwTraceSummaryV2, event_len: usize) -> Result<(), PalwV2Error> {
        let mism = |what: &str| Err(PalwV2Error::InconsistentCommitment(what.to_string()));
        if summary.event_count as usize != event_len {
            return mism("event_count does not match the event list length");
        }
        // The profile invariant (VPS §7.4): one event per canonical call, D calls total.
        if summary.event_count != context.exact_decode_tokens {
            return mism("event_count must equal exact_decode_tokens under the exact-decode profile");
        }
        if summary.declared_prefill_tokens != context.declared_prefill_tokens
            || summary.exact_decode_tokens != context.exact_decode_tokens
        {
            return mism("summary token counts disagree with the job context");
        }
        if summary.first_event_kind != PalwTracePhaseV2::Prefill {
            return mism("the first event is the prefill call");
        }
        let expected_last = if summary.event_count == 1 { PalwTracePhaseV2::Prefill } else { PalwTracePhaseV2::Decode };
        if summary.last_event_kind != expected_last {
            return mism("last_event_kind disagrees with the event count");
        }
        Ok(())
    }

    /// Recomputes everything recomputable from the carried data and compares against the stored
    /// root. Used by auditors on received commitments; a worker's own assembly already ran it.
    pub fn verify_internal(&self) -> Result<(), PalwV2Error> {
        Self::check_summary(&self.context, &self.summary, self.ordered_event_hashes.len())?;
        let commitment_root = trace_event_merkle_root_v2(&self.ordered_event_hashes)?;
        let recomputed = full_logits_trace_root_v2(&self.context.context_hash(), &self.summary, &commitment_root);
        if recomputed != self.full_logits_sequence_root {
            return Err(PalwV2Error::InconsistentCommitment("stored root does not match the recomputed root".to_string()));
        }
        Ok(())
    }
}

/// The replay-stable projection (detailed design §11.1, restricted to the current logits-only
/// profile). Two independent executions of one job must produce byte-identical projections;
/// consensus equality compares EVERY field — token-only matches are forbidden (§11.4).
///
/// Telemetry (durations, EOG sightings) lives in [`PalwJobTelemetryV2`], a different type, so it
/// can never leak into an equality check by field addition.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwResultProjectionV2 {
    pub job_context_hash: Hash64,
    /// `execution_root` of the current profile — [`full_logits_trace_root_v2`].
    pub full_logits_trace_root: Hash64,
    pub output_commitment: Hash64,
    pub operation_schedule_commitment: Hash64,
    pub canonical_compute_units: u128,
    pub prefill_tokens: u32,
    pub decode_tokens: u32,
    pub trace_event_count: u32,
    pub stop_reason: PalwStopReasonV2,
}

impl PalwResultProjectionV2 {
    /// Exact-match predicate (detailed design §11.4). Deliberately just `==` — the method exists
    /// so call sites read as the rule they implement.
    pub fn matches(&self, other: &Self) -> bool {
        self == other
    }
}

/// Non-consensus observations. NEVER part of projection equality, receipts or hashes.
#[derive(Clone, Debug, PartialEq, Eq, Default, BorshSerialize, BorshDeserialize)]
pub struct PalwJobTelemetryV2 {
    pub model_load_ms: u64,
    pub execute_ms: u64,
    /// First decode ordinal (0-based) at which an EOG token was *generated* — generation does
    /// not stop there under the exact-decode policy (VPS §5.5).
    pub eog_first_seen_at_decode_index: Option<u32>,
}

/// The framed worker response. On any failure the worker emits NOTHING on stdout and exits
/// non-zero — partial results never leave the process (VPS §8.3).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwJobResultV2 {
    pub version: u16,
    /// [`job_request_hash_v2`] of the exact request payload answered (VPS §9.2).
    pub request_hash: Hash64,
    pub job_id: Hash64,
    pub projection: PalwResultProjectionV2,
    pub telemetry: PalwJobTelemetryV2,
}

// ---------------------------------------------------------------------------------------------
// RuntimeManifestV2 (v2 design §7)
// ---------------------------------------------------------------------------------------------

/// The exact-artifact runtime manifest. Class membership is defined by exact artifacts and fixed
/// execution conditions — never by a self-reported CPU name (v2 design §7).
///
/// Fields that a Land-stage build cannot yet pin carry the literal `"unpinned"` (or the
/// [`golden_vector_root_unpopulated_v2`] sentinel): explicitly visible, hash-bound, and a
/// mandatory rejection at class-registration time (activation gate §12 — "exact artifacts and
/// floating-point environment are launch-verified").
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwRuntimeManifestV2 {
    pub version: u16,
    pub target_arch: String,
    pub target_triple: String,
    pub compiler_name: String,
    pub compiler_version: String,
    pub linker_version: String,
    pub cmake_cache_sha256: [u8; 32],
    pub worker_binary_sha256: [u8; 32],
    pub llama_static_library_sha256: [u8; 32],
    pub llama_cpp_commit: String,
    pub patchset_root: String,
    pub exact_cpu_isa_baseline: String,
    pub runtime_cpu_feature_mask: String,
    pub ggml_native: bool,
    pub ggml_openmp: bool,
    pub ggml_blas: bool,
    pub ggml_accelerate: bool,
    pub ggml_sse42: bool,
    pub ggml_avx: bool,
    pub ggml_avx2: bool,
    pub ggml_fma: bool,
    pub ggml_f16c: bool,
    pub ggml_cpu_all_variants: bool,
    pub thread_count: u32,
    pub thread_affinity_policy: String,
    pub floating_point_environment: String,
    pub gguf_sha256: [u8; 32],
    pub tokenizer_sha256: [u8; 32],
    pub prompt_template_sha256: [u8; 32],
    /// Human-readable identity of the resolved libm — e.g. `"glibc/2.39"`, `"apple/libsystem_m"`.
    /// **Diagnostic only**: it names the implementation for an operator reading a refusal message.
    /// The load-bearing field is [`Self::libm_arithmetic_digest`], because a version string is
    /// neither necessary nor sufficient for the arithmetic to agree.
    pub libm_identity: String,
    /// Behavioural fingerprint of the resolved libm's `expf`/`logf` over the frozen probe vector
    /// [`PALW_LIBM_PROBE_V1`] — the missing half of ADR-0031.
    ///
    /// ADR-0031 Facts 2/4 make glibc's `expf`/`logf` **normative arithmetic inside the PoW tag**
    /// (the GDN decay calls `expf` per (token, head) across 18 of 24 layers), and say the binding
    /// is resolvable only by disassembling `libm.so.6`. Until this field existed the manifest —
    /// which *is* the class identity — never named libm at all: two hosts with different glibc
    /// builds produced the same `runtime_manifest_hash` and different arithmetic, so a routine
    /// distro upgrade silently changed the PoW tag with no identity field to announce it
    /// (mainnet-readiness audit B8).
    ///
    /// A **behavioural** digest rather than a build id or a version string, deliberately: a build
    /// id moves on rebuilds that do not change arithmetic (spurious class splits), and a version
    /// string can hide a patched or LD_PRELOAD-ed libm. This digest moves **iff the arithmetic
    /// this class depends on moves**, which is exactly the class property. It is a cheap runtime
    /// resolution of the disassembly ADR-0031 calls for — not a replacement for that audit, which
    /// remains what licenses `libm_transcribed` in the registry.
    pub libm_arithmetic_digest: [u8; 32],
    pub trace_scheme_id: Hash64,
    pub golden_vector_root: Hash64,
}

impl PalwRuntimeManifestV2 {
    /// The frozen manifest-hash preimage: struct order, LE integers, length-prefixed strings,
    /// bools as one byte. Golden vectors in the tests pin the layout.
    pub fn manifest_hash(&self) -> Hash64 {
        let mut w = CanonicalWriter::new();
        w.put_u16(self.version);
        w.put_var_str(&self.target_arch);
        w.put_var_str(&self.target_triple);
        w.put_var_str(&self.compiler_name);
        w.put_var_str(&self.compiler_version);
        w.put_var_str(&self.linker_version);
        w.put_fixed32(&self.cmake_cache_sha256);
        w.put_fixed32(&self.worker_binary_sha256);
        w.put_fixed32(&self.llama_static_library_sha256);
        w.put_var_str(&self.llama_cpp_commit);
        w.put_var_str(&self.patchset_root);
        w.put_var_str(&self.exact_cpu_isa_baseline);
        w.put_var_str(&self.runtime_cpu_feature_mask);
        for flag in [
            self.ggml_native,
            self.ggml_openmp,
            self.ggml_blas,
            self.ggml_accelerate,
            self.ggml_sse42,
            self.ggml_avx,
            self.ggml_avx2,
            self.ggml_fma,
            self.ggml_f16c,
            self.ggml_cpu_all_variants,
        ] {
            w.put_u8(flag as u8);
        }
        w.put_u32(self.thread_count);
        w.put_var_str(&self.thread_affinity_policy);
        w.put_var_str(&self.floating_point_environment);
        w.put_fixed32(&self.gguf_sha256);
        w.put_fixed32(&self.tokenizer_sha256);
        w.put_fixed32(&self.prompt_template_sha256);
        w.put_var_str(&self.libm_identity);
        w.put_fixed32(&self.libm_arithmetic_digest);
        w.put_hash64(&self.trace_scheme_id);
        w.put_hash64(&self.golden_vector_root);
        w.keyed64(PALW_V2_DOMAIN_RUNTIME_MANIFEST)
    }
}

// ---------------------------------------------------------------------------------------------
// Golden vector set — the boot self-test corpus (VPS design §8.2, v2 design §10 item 2).
// ---------------------------------------------------------------------------------------------

/// The sentinel `runtime_manifest_hash` golden jobs bind in their job context.
///
/// The real manifest hash **includes** `golden_vector_root`, and the golden vectors' expected
/// values depend on the job context — putting the real manifest hash into golden contexts would
/// be circular. The sentinel (all-zero, unreachable by any keyed BLAKE2b output in practice)
/// removes exactly that one self-referential field. Everything else a vector could be smuggled
/// across — class, model, shape, scheme, CU rule, tokenizer, budgets — is still bound: the set
/// header pins the real ids and a loader must refuse a set whose ids are not the worker's own.
/// A golden PASS therefore means "this machine reproduces the class's canonical arithmetic",
/// while real jobs keep the full manifest binding.
pub fn golden_sentinel_manifest_hash() -> Hash64 {
    Hash64::from_bytes([0u8; 64])
}

pub fn golden_job_id_v2(name: &str) -> Hash64 {
    keyed64(PALW_V2_DOMAIN_GOLDEN_JOB_ID, &[name.as_bytes()])
}

pub fn golden_job_nullifier_v2(name: &str) -> Hash64 {
    keyed64(PALW_V2_DOMAIN_GOLDEN_JOB_NULLIFIER, &[name.as_bytes()])
}

pub fn golden_assignment_id_v2(name: &str) -> Hash64 {
    keyed64(PALW_V2_DOMAIN_GOLDEN_ASSIGNMENT_ID, &[name.as_bytes()])
}

/// Caps for golden sets: a boot corpus is small by design.
pub const PALW_V2_MAX_GOLDEN_JOBS: usize = 64;
pub const PALW_V2_MAX_GOLDEN_NAME_BYTES: usize = 128;

/// The full expected projection of one golden job — compared field-exact, full 64 bytes.
/// Prefix comparison is banned as an acceptance criterion (v2 design §5, activation gate §12).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwGoldenExpectedV2 {
    pub job_context_hash: Hash64,
    pub full_logits_trace_root: Hash64,
    pub output_commitment: Hash64,
    pub operation_schedule_commitment: Hash64,
    pub canonical_compute_units: u128,
    pub prefill_tokens: u32,
    pub decode_tokens: u32,
    pub trace_event_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwGoldenJobV2 {
    pub name: String,
    pub network_id: Vec<u8>,
    pub execution_seed: [u8; 32],
    pub prompt_token_ids: Vec<u32>,
    pub exact_decode_tokens: u32,
    pub max_context_tokens: u32,
    pub expected: PalwGoldenExpectedV2,
}

/// A registered golden vector set. The header binds the exact runtime identity the vectors were
/// generated under; [`PalwGoldenVectorSetV2::golden_root`] is the value `RuntimeManifestV2`
/// carries, replacing the [`golden_vector_root_unpopulated_v2`] sentinel.
///
/// # Why the header carries MEASURED build identity, not just the class label
///
/// `runtime_class_id` is derived from a compile-time class STRING (the worker's `RUNTIME_CLASS`,
/// selected by cfg), so it says which class the build *claims*. It cannot distinguish two builds
/// that claim the same class and compute different arithmetic — and the difference that matters
/// most is exactly of that kind: `GGML_OPENMP` moves the matmul's work split and reduction order
/// into an external runtime's scheduling, so an OpenMP build and a non-OpenMP build of the same
/// source are different arithmetic under one class label.
///
/// `RuntimeManifestV2` already covers that (it hashes `cmake_cache_sha256`,
/// `llama_static_library_sha256` and every `ggml_*` flag), and a validator's
/// [`PalwCapabilityDeclarationV2`] stakes a bond on its `runtime_manifest_hash`. The boot gate
/// that loads this set must therefore be at least as strong as the claim it is gating, or a host
/// passes its own self-test against vectors generated under a build it did not declare. So the
/// two measured build hashes travel WITH the set and the loader compares them.
///
/// They are the measured pair rather than the full `runtime_manifest_hash` because that hash
/// includes `golden_vector_root` — comparing it against the set that DEFINES that root would be
/// circular. `cmake_cache_sha256` (the configuration that chose the kernels) and
/// `llama_static_library_sha256` (the archives actually linked) carry no such self-reference.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwGoldenVectorSetV2 {
    pub version: u16,
    pub runtime_class_id: Hash64,
    pub model_profile_id: Hash64,
    pub shape_profile_id: Hash64,
    /// SHA-256 of the `CMakeCache.txt` that configured the llama.cpp tree these vectors were
    /// measured under — the file the `ggml_*` flags (OpenMP included) are read out of.
    pub cmake_cache_sha256: [u8; 32],
    /// Combined SHA-256 over the exact llama.cpp/ggml static archives linked into the generating
    /// worker, in the build's fixed order with each library name bound to its digest.
    pub llama_static_library_sha256: [u8; 32],
    pub jobs: Vec<PalwGoldenJobV2>,
}

impl PalwGoldenVectorSetV2 {
    /// Structural validation — caps and per-job budget arithmetic, fail closed. Identity
    /// equality against the LOADING worker's own ids is the loader's job (the set says what it
    /// was generated under; the worker must refuse a set that is not its own class).
    pub fn validate_shape(&self, profile_max_context_tokens: u32) -> Result<(), PalwV2Error> {
        if self.version != PALW_GOLDEN_SET_VERSION_V2 {
            return Err(PalwV2Error::UnsupportedVersion { got: self.version, expected: PALW_GOLDEN_SET_VERSION_V2 });
        }
        if self.jobs.is_empty() {
            return Err(PalwV2Error::GoldenSetInvalid("a golden set with no jobs gates nothing".into()));
        }
        if self.jobs.len() > PALW_V2_MAX_GOLDEN_JOBS {
            return Err(PalwV2Error::GoldenSetInvalid(format!("{} jobs exceeds the {PALW_V2_MAX_GOLDEN_JOBS}-job cap", self.jobs.len())));
        }
        let mut seen: Vec<&str> = Vec::with_capacity(self.jobs.len());
        for job in &self.jobs {
            if job.name.is_empty() || job.name.len() > PALW_V2_MAX_GOLDEN_NAME_BYTES {
                return Err(PalwV2Error::GoldenSetInvalid("golden job name is empty or over the cap".into()));
            }
            if seen.contains(&job.name.as_str()) {
                return Err(PalwV2Error::GoldenSetInvalid(format!("duplicate golden job name {:?}", job.name)));
            }
            seen.push(&job.name);
            // Reuse the envelope predicate so a golden job can never be shaped in a way a real
            // job could not be.
            self.envelope_for(job).validate_shape(profile_max_context_tokens)?;
        }
        Ok(())
    }

    /// The canonical execution request for one golden job. Deterministic — an executor and a
    /// later auditor derive the identical envelope from the set alone.
    pub fn envelope_for(&self, job: &PalwGoldenJobV2) -> PalwJobEnvelopeV2 {
        PalwJobEnvelopeV2 {
            version: PALW_JOB_WIRE_VERSION_V2,
            network_id: job.network_id.clone(),
            job_id: golden_job_id_v2(&job.name),
            job_nullifier: golden_job_nullifier_v2(&job.name),
            mode: PalwJobModeV2::Execute,
            model_profile_id: self.model_profile_id,
            runtime_manifest_hash: golden_sentinel_manifest_hash(),
            runtime_class_id: self.runtime_class_id,
            shape_profile_id: self.shape_profile_id,
            trace_scheme_id: trace_scheme_id_v2(),
            cu_ruleset_id: cu_ruleset_id_v2(),
            execution_seed: job.execution_seed,
            prompt_token_ids: job.prompt_token_ids.clone(),
            exact_decode_tokens: job.exact_decode_tokens,
            max_context_tokens: job.max_context_tokens,
            assignment_id: golden_assignment_id_v2(&job.name),
            assignment_epoch: 0,
            deadline_unix_ms: 0,
        }
    }

    /// The frozen root over the canonical encoding of the whole set (NOT over file bytes, so a
    /// re-serialized but semantically identical set keeps its root).
    pub fn golden_root(&self) -> Hash64 {
        let mut w = CanonicalWriter::new();
        w.put_u16(self.version);
        w.put_hash64(&self.runtime_class_id);
        w.put_hash64(&self.model_profile_id);
        w.put_hash64(&self.shape_profile_id);
        // The measured build identity is inside the root, so two sets that differ ONLY in the
        // build they were generated under are different sets — and because the root is what
        // `RuntimeManifestV2` carries, that difference propagates into the manifest hash a
        // capability declaration bonds. Leaving it out of the root would let the header field be
        // rewritten without moving any committed value.
        w.put_fixed32(&self.cmake_cache_sha256);
        w.put_fixed32(&self.llama_static_library_sha256);
        w.put_u32(self.jobs.len() as u32);
        for job in &self.jobs {
            w.put_var_str(&job.name);
            w.put_var_bytes(&job.network_id);
            w.put_fixed32(&job.execution_seed);
            w.put_u32_seq(&job.prompt_token_ids);
            w.put_u32(job.exact_decode_tokens);
            w.put_u32(job.max_context_tokens);
            w.put_hash64(&job.expected.job_context_hash);
            w.put_hash64(&job.expected.full_logits_trace_root);
            w.put_hash64(&job.expected.output_commitment);
            w.put_hash64(&job.expected.operation_schedule_commitment);
            w.0.extend_from_slice(&job.expected.canonical_compute_units.to_le_bytes());
            w.put_u32(job.expected.prefill_tokens);
            w.put_u32(job.expected.decode_tokens);
            w.put_u32(job.expected.trace_event_count);
        }
        w.keyed64(PALW_V2_DOMAIN_GOLDEN_SET)
    }
}

// ---------------------------------------------------------------------------------------------
// IPC framing: `u32-le length ‖ Borsh payload`, hard-capped, fail closed (VPS §5.1).
// ---------------------------------------------------------------------------------------------

/// Reads one frame. Enforces the cap BEFORE allocating, requires the payload to be complete,
/// and (because one connection carries one request) requires EOF after it — trailing bytes are
/// an error, not a second message.
///
/// Protocol contract for stream sockets: the SENDER half-closes its write side
/// (`shutdown(SHUT_WR)`) after its frame, so the receiver's EOF probe returns immediately
/// instead of blocking until a read timeout. Subprocess stdin gets the same effect by closing
/// the pipe.
pub fn read_framed(reader: &mut impl std::io::Read, max_bytes: u32) -> Result<Vec<u8>, PalwV2Error> {
    let mut len_bytes = [0u8; 4];
    read_exactly(reader, &mut len_bytes)?;
    let len = u32::from_le_bytes(len_bytes);
    if len > max_bytes {
        return Err(PalwV2Error::OversizedFrame { got: len, max: max_bytes });
    }
    let mut payload = vec![0u8; len as usize];
    read_exactly(reader, &mut payload)?;
    let mut probe = [0u8; 1];
    match reader.read(&mut probe) {
        Ok(0) => Ok(payload),
        Ok(_) => Err(PalwV2Error::TrailingBytes(1)),
        Err(e) => Err(PalwV2Error::Decode(format!("read after frame failed: {e}"))),
    }
}

fn read_exactly(reader: &mut impl std::io::Read, buf: &mut [u8]) -> Result<(), PalwV2Error> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => return Err(PalwV2Error::TruncatedFrame { expected: buf.len(), got: filled }),
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(PalwV2Error::Decode(format!("frame read failed: {e}"))),
        }
    }
    Ok(())
}

/// Writes one frame.
pub fn write_framed(writer: &mut impl std::io::Write, payload: &[u8]) -> std::io::Result<()> {
    let len: u32 = payload.len().try_into().map_err(|_| std::io::Error::other("frame payload exceeds u32"))?;
    writer.write_all(&len.to_le_bytes())?;
    writer.write_all(payload)?;
    writer.flush()
}

/// Decodes a Borsh value from a frame payload, rejecting trailing bytes inside the payload.
pub fn decode_framed_borsh<T: BorshDeserialize>(payload: &[u8]) -> Result<T, PalwV2Error> {
    let mut slice = payload;
    let value = T::deserialize(&mut slice).map_err(|e| PalwV2Error::Decode(e.to_string()))?;
    if !slice.is_empty() {
        return Err(PalwV2Error::TrailingBytes(slice.len()));
    }
    Ok(value)
}

// ---------------------------------------------------------------------------------------------
// Agent wire protocol `misaka-palw-agent-borsh/v1` (VPS design §2.3, §5.1): one framed request,
// one framed response, per connection. Same framing, same caps, same fail-closed decoding.
// ---------------------------------------------------------------------------------------------

/// What a client (kaspad's compute service, or a harness) may ask the agent.
/// Unknown discriminants fail Borsh decoding.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwAgentRequestV1 {
    /// Execute or replay one canonical job.
    Job(PalwJobEnvelopeV2) = 0,
    /// Health probe — never touches the worker or the model.
    Health = 1,
}

/// Agent lifecycle state (VPS design §8.1, collapsed to what Phase A has).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
pub enum PalwAgentStateV1 {
    Ready = 0,
    Busy = 1,
    /// A conformance failure (golden selftest, artifact hash) — every job is rejected until an
    /// operator intervenes. Preferring abstention over answers is the refutation-dominant rule
    /// (VPS design §13).
    Quarantined = 2,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwAgentHealthV1 {
    pub state: PalwAgentStateV1,
    pub selftest_passed: bool,
    pub runtime_manifest_hash: Hash64,
    pub golden_vector_root: Hash64,
    pub max_context_tokens: u32,
    pub jobs_total: u64,
    pub jobs_ok: u64,
    pub jobs_rejected: u64,
    pub jobs_failed: u64,
    pub timeouts_total: u64,
}

/// The agent's answer. `JobRejected` is an admission decision (nothing executed — deadline,
/// budget, duplicate, busy, quarantine); `JobFailed` means a worker ran and did not produce an
/// accepted result (crash, timeout, malformed or mis-bound output). The split matters to a
/// supervisor: rejections are safe to retry elsewhere, failures are evidence about THIS host.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwAgentResponseV1 {
    JobOk(PalwJobResultV2) = 0,
    JobRejected { code: String, message: String } = 1,
    JobFailed { code: String, message: String } = 2,
    Health(PalwAgentHealthV1) = 3,
}

// ---------------------------------------------------------------------------------------------
// Tests: golden vectors freeze the preimage layouts; negative controls enforce fail-closed.
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn h64(byte: u8) -> Hash64 {
        Hash64::from_bytes([byte; 64])
    }

    fn test_envelope() -> PalwJobEnvelopeV2 {
        PalwJobEnvelopeV2 {
            version: 2,
            network_id: b"misaka-devnet".to_vec(),
            job_id: h64(0x11),
            job_nullifier: h64(0x22),
            mode: PalwJobModeV2::Execute,
            model_profile_id: h64(0x33),
            runtime_manifest_hash: h64(0x44),
            runtime_class_id: h64(0x55),
            shape_profile_id: h64(0x66),
            trace_scheme_id: h64(0x77),
            cu_ruleset_id: h64(0x88),
            execution_seed: [0xAB; 32],
            prompt_token_ids: vec![5, 6, 7, 8, 9],
            exact_decode_tokens: 4,
            max_context_tokens: 4096,
            assignment_id: h64(0x99),
            assignment_epoch: 7,
            deadline_unix_ms: 0,
        }
    }

    fn test_context() -> PalwJobContextV2 {
        PalwJobContextV2::from_envelope(&test_envelope(), h64(0xAA))
    }

    fn test_summary(ctx: &PalwJobContextV2, event_count: u32, output_ids: &[u32]) -> PalwTraceSummaryV2 {
        PalwTraceSummaryV2 {
            vocab_size: 16,
            logits_dtype: PalwLogitsDtypeV2::F32Le,
            declared_prefill_tokens: ctx.declared_prefill_tokens,
            exact_decode_tokens: ctx.exact_decode_tokens,
            event_count,
            first_event_kind: PalwTracePhaseV2::Prefill,
            last_event_kind: if event_count == 1 { PalwTracePhaseV2::Prefill } else { PalwTracePhaseV2::Decode },
            output_token_ids_hash: output_token_ids_hash_v2(output_ids),
            stop_reason: PalwStopReasonV2::ExactBudgetReached,
        }
    }

    /// A synthetic 16-vocab logits row: finite, includes negative values and a signed zero.
    fn test_logits(salt: f32) -> Vec<f32> {
        (0..16).map(|i| (i as f32 - 8.0) * 0.25 + salt).collect()
    }

    #[test]
    fn domain_keys_fit_blake2b_and_are_unique() {
        for d in PALW_V2_ALL_DOMAINS {
            assert!(d.len() <= 64, "BLAKE2b keys are at most 64 bytes; {:?} is {}", String::from_utf8_lossy(d), d.len());
            assert!(!d.is_empty());
        }
        for (i, a) in PALW_V2_ALL_DOMAINS.iter().enumerate() {
            for b in &PALW_V2_ALL_DOMAINS[i + 1..] {
                assert_ne!(a, b, "duplicate domain key {:?}", String::from_utf8_lossy(a));
            }
        }
    }

    #[test]
    fn execution_algo_id_is_its_own_namespace() {
        // The numeric coincidence with the historical header-level Argon2id id is expected —
        // and the ONLY place they may meet is this assertion documenting that they never mix.
        assert_eq!(PALW_EXECUTION_ALGO_ID_V2.wire_byte(), 2);
        assert_eq!(crate::pow_layer0::POW_ALGO_ID_ARGON2ID, 2);
        // No `From`/`Into` conversions exist; constructing equality between the two types does
        // not compile, which is the real guarantee. `wire_byte()` is the single, named escape
        // hatch for PALW-internal serialization.
    }

    // -----------------------------------------------------------------------------------------
    // Golden vectors: these freeze the preimage layouts. If one of these assertions fails, the
    // wire format changed — that is a NEW scheme version and a re-activation, not a patch.
    // -----------------------------------------------------------------------------------------

    #[test]
    fn golden_vector_scheme_and_ruleset_ids() {
        assert_eq!(
            faster_hex::hex_string(trace_scheme_id_v2().as_byte_slice()),
            "4da708dd27250e3dec500056cf3f1b26684477a5740e14e95591b43316aced60bee1bd76a24b78c9de2b5e57142f016478215ec6d2a5b3ecd34e22d9243ad791"
        );
        assert_eq!(
            faster_hex::hex_string(cu_ruleset_id_v2().as_byte_slice()),
            "b20595ff828165dd61e06f65dc37899ea23d039038056b56dcd334fee05fc459c27d66207d6505d4a22ea92c91b124f0293705045047c47333b12fe56c6677f9"
        );
    }

    #[test]
    fn golden_vector_context_hash() {
        assert_eq!(
            faster_hex::hex_string(test_context().context_hash().as_byte_slice()),
            "8eca809539d3dc5b86e20349365010b4a66968d37047352812edc8cbe5a65a060d5f43e307594ea882c24ffda3ad5e382821acedafaef5a559f7c9e2cd02db62"
        );
    }

    #[test]
    fn golden_vector_event_merkle_root_and_trace_root() {
        let ctx = test_context();
        let ctx_hash = ctx.context_hash();
        let mut scratch = Vec::new();
        let events: Vec<Hash64> = (0..4u32)
            .map(|i| {
                let phase = if i == 0 { PalwTracePhaseV2::Prefill } else { PalwTracePhaseV2::Decode };
                let step = if i == 0 { 0 } else { i - 1 };
                logits_event_hash_v2(&ctx_hash, phase, step, i, 16, &test_logits(i as f32), &mut scratch).unwrap()
            })
            .collect();
        let merkle = trace_event_merkle_root_v2(&events).unwrap();
        assert_eq!(
            faster_hex::hex_string(merkle.as_byte_slice()),
            "b60379905dc49dc092effb4af6a28b19673c5266157d8edba53f79c2a8feb7caa11061ab4df8e34f7cb790b8dbe24ab63100a02d2e18fc1a77e82bd8756b4caf"
        );
        let summary = test_summary(&ctx, 4, &[1, 2, 3, 4]);
        let root = full_logits_trace_root_v2(&ctx_hash, &summary, &merkle);
        assert_eq!(
            faster_hex::hex_string(root.as_byte_slice()),
            "91018708c39e0b3b037eb26bd82b7717dbe468d62d4f0c68cb42f99a1b9e35e2c5470e69a19b706040e1c0714304beb1e53d6da6d5df1f624075f00e66cefff1"
        );
    }

    // -----------------------------------------------------------------------------------------
    // Negative controls (v2 design §11.3): every mutation must change the value or be rejected.
    // -----------------------------------------------------------------------------------------

    #[test]
    fn event_hash_binds_context_phase_step_and_logits() {
        let ctx_hash = test_context().context_hash();
        let mut scratch = Vec::new();
        let base =
            logits_event_hash_v2(&ctx_hash, PalwTracePhaseV2::Decode, 3, 4, 16, &test_logits(0.0), &mut scratch).unwrap();
        let other_ctx =
            logits_event_hash_v2(&h64(0xEE), PalwTracePhaseV2::Decode, 3, 4, 16, &test_logits(0.0), &mut scratch).unwrap();
        let other_phase =
            logits_event_hash_v2(&ctx_hash, PalwTracePhaseV2::Prefill, 3, 4, 16, &test_logits(0.0), &mut scratch).unwrap();
        let other_step =
            logits_event_hash_v2(&ctx_hash, PalwTracePhaseV2::Decode, 2, 4, 16, &test_logits(0.0), &mut scratch).unwrap();
        let other_logits =
            logits_event_hash_v2(&ctx_hash, PalwTracePhaseV2::Decode, 3, 4, 16, &test_logits(1.0e-6), &mut scratch).unwrap();
        assert_ne!(base, other_ctx, "an event must not be replayable under another job context");
        assert_ne!(base, other_phase);
        assert_ne!(base, other_step);
        assert_ne!(base, other_logits, "low-bit logit differences must change the event hash");
    }

    #[test]
    fn signed_zero_is_preserved_and_nonfinite_is_rejected() {
        let ctx_hash = test_context().context_hash();
        let mut scratch = Vec::new();
        let mut pos = test_logits(0.0);
        pos[0] = 0.0;
        let mut neg = test_logits(0.0);
        neg[0] = -0.0;
        let a = logits_event_hash_v2(&ctx_hash, PalwTracePhaseV2::Decode, 0, 1, 16, &pos, &mut scratch).unwrap();
        let b = logits_event_hash_v2(&ctx_hash, PalwTracePhaseV2::Decode, 0, 1, 16, &neg, &mut scratch).unwrap();
        assert_ne!(a, b, "+0.0 and -0.0 are different IEEE-754 bit patterns and must commit differently");

        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut row = test_logits(0.0);
            row[7] = bad;
            let err = logits_event_hash_v2(&ctx_hash, PalwTracePhaseV2::Decode, 5, 9, 16, &row, &mut scratch).unwrap_err();
            assert_eq!(err, PalwV2Error::NonFiniteLogit { event_index: 9, logit_index: 7 });
        }

        let short = vec![0.0f32; 15];
        let err = logits_event_hash_v2(&ctx_hash, PalwTracePhaseV2::Decode, 0, 0, 16, &short, &mut scratch).unwrap_err();
        assert!(matches!(err, PalwV2Error::LogitsCountMismatch { got: 15, expected: 16 }));
    }

    #[test]
    fn merkle_root_detects_delete_duplicate_reorder() {
        let events: Vec<Hash64> = (1..=5u8).map(h64).collect();
        let base = trace_event_merkle_root_v2(&events).unwrap();

        let mut deleted = events.clone();
        deleted.pop();
        assert_ne!(base, trace_event_merkle_root_v2(&deleted).unwrap());

        let mut duplicated = events.clone();
        duplicated.push(events[4]);
        assert_ne!(base, trace_event_merkle_root_v2(&duplicated).unwrap());

        let mut reordered = events.clone();
        reordered.swap(1, 2);
        assert_ne!(base, trace_event_merkle_root_v2(&reordered).unwrap());

        // The classic duplication ambiguity: [a, b] vs [a, b, b] must differ even at a
        // power-of-two boundary because odd nodes are promoted, never duplicated.
        let two = trace_event_merkle_root_v2(&events[..2]).unwrap();
        let padded = [events[0], events[1], events[1]];
        assert_ne!(two, trace_event_merkle_root_v2(&padded).unwrap());

        assert_eq!(trace_event_merkle_root_v2(&[]).unwrap_err(), PalwV2Error::EmptyTrace);
    }

    #[test]
    fn trace_root_binds_summary_fields() {
        let ctx = test_context();
        let ctx_hash = ctx.context_hash();
        let merkle = h64(0xC1);
        let base_summary = test_summary(&ctx, 4, &[1, 2, 3, 4]);
        let base = full_logits_trace_root_v2(&ctx_hash, &base_summary, &merkle);

        let mut s = base_summary.clone();
        s.event_count = 5;
        assert_ne!(base, full_logits_trace_root_v2(&ctx_hash, &s, &merkle));

        let mut s = base_summary.clone();
        s.output_token_ids_hash = output_token_ids_hash_v2(&[1, 2, 3, 5]);
        assert_ne!(base, full_logits_trace_root_v2(&ctx_hash, &s, &merkle));

        let mut s = base_summary.clone();
        s.vocab_size = 17;
        assert_ne!(base, full_logits_trace_root_v2(&ctx_hash, &s, &merkle));

        assert_ne!(base, full_logits_trace_root_v2(&h64(0xDD), &base_summary, &merkle), "root binds the job context");
        assert_ne!(base, full_logits_trace_root_v2(&ctx_hash, &base_summary, &h64(0xC2)), "root binds the event commitment");
    }

    #[test]
    fn context_hash_binds_every_identity_field() {
        let base = test_context();
        let base_hash = base.context_hash();
        // One representative per binding class; the golden vector freezes the full layout.
        let mut c = base.clone();
        c.network_id = b"misaka-testnet".to_vec();
        assert_ne!(base_hash, c.context_hash(), "network binding");
        let mut c = base.clone();
        c.execution_seed[0] ^= 1;
        assert_ne!(base_hash, c.context_hash(), "seed binding");
        let mut c = base.clone();
        c.runtime_manifest_hash = h64(0x45);
        assert_ne!(base_hash, c.context_hash(), "runtime binding");
        let mut c = base.clone();
        c.exact_decode_tokens += 1;
        assert_ne!(base_hash, c.context_hash(), "budget binding");
        let mut c = base.clone();
        c.prompt_token_ids_hash = prompt_token_ids_hash_v2(&[5, 6, 7, 8]);
        assert_ne!(base_hash, c.context_hash(), "prompt binding");
    }

    #[test]
    fn envelope_validation_fails_closed() {
        let profile_ctx = 4096;
        assert!(test_envelope().validate_shape(profile_ctx).is_ok());

        let mut e = test_envelope();
        e.version = 3;
        assert_eq!(e.validate_shape(profile_ctx).unwrap_err(), PalwV2Error::UnsupportedVersion { got: 3, expected: 2 });

        let mut e = test_envelope();
        e.prompt_token_ids.clear();
        assert_eq!(e.validate_shape(profile_ctx).unwrap_err(), PalwV2Error::EmptyPrompt);

        let mut e = test_envelope();
        e.exact_decode_tokens = 0;
        assert_eq!(e.validate_shape(profile_ctx).unwrap_err(), PalwV2Error::ZeroDecodeBudget);

        let mut e = test_envelope();
        e.exact_decode_tokens = 4096;
        assert!(matches!(e.validate_shape(profile_ctx).unwrap_err(), PalwV2Error::BudgetExceedsContext { .. }));

        // Checked arithmetic: a budget crafted to wrap u32 is a rejection, not an acceptance.
        let mut e = test_envelope();
        e.exact_decode_tokens = u32::MAX;
        assert!(matches!(e.validate_shape(profile_ctx).unwrap_err(), PalwV2Error::BudgetExceedsContext { .. }));

        let mut e = test_envelope();
        e.max_context_tokens = 2048;
        assert!(matches!(e.validate_shape(profile_ctx).unwrap_err(), PalwV2Error::ContextProfileMismatch { .. }));

        let mut e = test_envelope();
        e.network_id = vec![0u8; PALW_V2_MAX_NETWORK_ID_BYTES + 1];
        assert_eq!(e.validate_shape(profile_ctx).unwrap_err(), PalwV2Error::NetworkIdOutOfRange);

        let e = test_envelope();
        assert!(e.validate_against_vocab(10).is_ok());
        assert_eq!(
            e.validate_against_vocab(9).unwrap_err(),
            PalwV2Error::TokenOutOfRange { index: 4, token: 9, n_vocab: 9 },
            "a token id equal to n_vocab is out of range"
        );
    }

    #[test]
    fn envelope_borsh_round_trip_and_unknown_mode_fails() {
        let e = test_envelope();
        let bytes = borsh::to_vec(&e).unwrap();
        let back: PalwJobEnvelopeV2 = decode_framed_borsh(&bytes).unwrap();
        assert_eq!(e, back);

        // Corrupt the mode discriminant (directly after version ‖ network_id ‖ 3×Hash64).
        let mode_offset = 2 + 4 + e.network_id.len() + 64 + 64;
        let mut corrupted = bytes.clone();
        assert_eq!(corrupted[mode_offset], 0, "layout check: mode byte located");
        corrupted[mode_offset] = 9;
        assert!(matches!(decode_framed_borsh::<PalwJobEnvelopeV2>(&corrupted), Err(PalwV2Error::Decode(_))));

        // Trailing bytes inside a payload are rejected.
        let mut trailing = bytes;
        trailing.push(0);
        assert!(matches!(decode_framed_borsh::<PalwJobEnvelopeV2>(&trailing), Err(PalwV2Error::TrailingBytes(1))));
    }

    #[test]
    fn framing_is_capped_and_exact() {
        let payload = b"canonical".to_vec();
        let mut frame = Vec::new();
        write_framed(&mut frame, &payload).unwrap();
        assert_eq!(read_framed(&mut frame.as_slice(), 64).unwrap(), payload);

        // Over the cap: rejected before allocation.
        let mut oversized = ((PALW_V2_MAX_FRAME_BYTES + 1).to_le_bytes()).to_vec();
        oversized.extend_from_slice(&[0; 8]);
        assert!(matches!(
            read_framed(&mut oversized.as_slice(), PALW_V2_MAX_FRAME_BYTES),
            Err(PalwV2Error::OversizedFrame { .. })
        ));

        // Truncated payload.
        let mut truncated = 16u32.to_le_bytes().to_vec();
        truncated.extend_from_slice(&[0; 3]);
        assert!(matches!(read_framed(&mut truncated.as_slice(), 64), Err(PalwV2Error::TruncatedFrame { expected: 16, got: 3 })));

        // One connection, one request: trailing bytes after the frame are an error.
        let mut with_extra = Vec::new();
        write_framed(&mut with_extra, &payload).unwrap();
        with_extra.push(0xFF);
        assert!(matches!(read_framed(&mut with_extra.as_slice(), 64), Err(PalwV2Error::TrailingBytes(1))));
    }

    #[test]
    fn trace_commitment_assembles_and_detects_tampering() {
        let ctx = test_context();
        let ctx_hash = ctx.context_hash();
        let mut scratch = Vec::new();
        let events: Vec<Hash64> = (0..4u32)
            .map(|i| {
                let phase = if i == 0 { PalwTracePhaseV2::Prefill } else { PalwTracePhaseV2::Decode };
                let step = if i == 0 { 0 } else { i - 1 };
                logits_event_hash_v2(&ctx_hash, phase, step, i, 16, &test_logits(i as f32), &mut scratch).unwrap()
            })
            .collect();
        let summary = test_summary(&ctx, 4, &[1, 2, 3, 4]);
        let commitment = PalwTraceCommitmentV2::assemble(ctx.clone(), summary.clone(), events.clone()).unwrap();
        commitment.verify_internal().unwrap();

        // Event list tampering after assembly is caught by verify_internal.
        let mut tampered = commitment.clone();
        tampered.ordered_event_hashes.swap(1, 2);
        assert!(tampered.verify_internal().is_err());

        let mut tampered = commitment.clone();
        tampered.full_logits_sequence_root = h64(0x01);
        assert!(tampered.verify_internal().is_err());

        // Profile invariant: event_count must equal exact_decode_tokens.
        let short_summary = test_summary(&ctx, 3, &[1, 2, 3]);
        assert!(matches!(
            PalwTraceCommitmentV2::assemble(ctx.clone(), short_summary, events[..3].to_vec()),
            Err(PalwV2Error::InconsistentCommitment(_))
        ));

        // Wrong last-event kind.
        let mut bad_summary = summary;
        bad_summary.last_event_kind = PalwTracePhaseV2::Prefill;
        assert!(matches!(
            PalwTraceCommitmentV2::assemble(ctx, bad_summary, events),
            Err(PalwV2Error::InconsistentCommitment(_))
        ));
    }

    #[test]
    fn schedule_commitment_matches_expectation_and_binds_context() {
        let ctx_hash = test_context().context_hash();
        let mut b = PalwScheduleCommitmentBuilderV2::new(&ctx_hash);
        b.record_call(5);
        b.record_call(1);
        b.record_call(1);
        b.record_call(1);
        let (streamed, calls) = b.finalize();
        let (expected, expected_calls) = expected_schedule_commitment_v2(&ctx_hash, 5, 4);
        assert_eq!(streamed, expected);
        assert_eq!(calls, expected_calls);
        assert_eq!(calls, 4, "1 prefill call + D-1 decode calls");

        let (other_ctx, _) = expected_schedule_commitment_v2(&h64(0xEF), 5, 4);
        assert_ne!(expected, other_ctx, "schedule commitment binds the job context");
        let (other_prefill, _) = expected_schedule_commitment_v2(&ctx_hash, 6, 4);
        assert_ne!(expected, other_prefill);
    }

    #[test]
    fn projection_equality_is_field_exact_and_telemetry_free() {
        let p = PalwResultProjectionV2 {
            job_context_hash: h64(1),
            full_logits_trace_root: h64(2),
            output_commitment: h64(3),
            operation_schedule_commitment: h64(4),
            canonical_compute_units: canonical_compute_units_v2(5, 4),
            prefill_tokens: 5,
            decode_tokens: 4,
            trace_event_count: 4,
            stop_reason: PalwStopReasonV2::ExactBudgetReached,
        };
        assert!(p.matches(&p.clone()));
        let mut q = p.clone();
        q.canonical_compute_units += 1;
        assert!(!p.matches(&q), "every projection field participates in the exact match");
        assert_eq!(p.canonical_compute_units, 37, "cu = prefill + 8*decode");
        // Telemetry is a different type; it cannot enter this comparison by construction.
        let _telemetry: PalwJobTelemetryV2 = Default::default();
    }

    fn test_golden_set() -> PalwGoldenVectorSetV2 {
        let expected = PalwGoldenExpectedV2 {
            job_context_hash: h64(0xE1),
            full_logits_trace_root: h64(0xE2),
            output_commitment: h64(0xE3),
            operation_schedule_commitment: h64(0xE4),
            canonical_compute_units: 140,
            prefill_tokens: 12,
            decode_tokens: 16,
            trace_event_count: 16,
        };
        PalwGoldenVectorSetV2 {
            version: PALW_GOLDEN_SET_VERSION_V2,
            runtime_class_id: h64(0x55),
            model_profile_id: h64(0x33),
            shape_profile_id: h64(0x66),
            cmake_cache_sha256: [0xC1; 32],
            llama_static_library_sha256: [0xC2; 32],
            jobs: vec![
                PalwGoldenJobV2 {
                    name: "probe-a".into(),
                    network_id: b"misaka-golden".to_vec(),
                    execution_seed: [1; 32],
                    prompt_token_ids: vec![1, 2, 3],
                    exact_decode_tokens: 4,
                    max_context_tokens: 4096,
                    expected: expected.clone(),
                },
                PalwGoldenJobV2 {
                    name: "probe-b".into(),
                    network_id: b"misaka-golden".to_vec(),
                    execution_seed: [2; 32],
                    prompt_token_ids: vec![9, 8],
                    exact_decode_tokens: 2,
                    max_context_tokens: 4096,
                    expected,
                },
            ],
        }
    }

    #[test]
    fn golden_set_validates_and_roots_deterministically() {
        let set = test_golden_set();
        set.validate_shape(4096).unwrap();
        assert_eq!(set.golden_root(), test_golden_set().golden_root());

        // The derived envelope is deterministic, passes the normal predicate, and binds the
        // sentinel manifest hash — never a real one.
        let env = set.envelope_for(&set.jobs[0]);
        env.validate_shape(4096).unwrap();
        assert_eq!(env, set.envelope_for(&set.jobs[0]));
        assert_eq!(env.runtime_manifest_hash, golden_sentinel_manifest_hash());
        assert_eq!(env.job_id, golden_job_id_v2("probe-a"));
        assert_ne!(env.job_id, golden_job_nullifier_v2("probe-a"), "id and nullifier domains are distinct");
    }

    #[test]
    fn golden_root_binds_identity_jobs_and_expectations() {
        let base = test_golden_set().golden_root();

        let mut s = test_golden_set();
        s.runtime_class_id = h64(0x56);
        assert_ne!(base, s.golden_root(), "a set for another class is another set");

        let mut s = test_golden_set();
        s.jobs[1].expected.full_logits_trace_root = h64(0xEE);
        assert_ne!(base, s.golden_root(), "expectations are part of the root");

        let mut s = test_golden_set();
        s.jobs.swap(0, 1);
        assert_ne!(base, s.golden_root(), "job order is part of the root");

        let mut s = test_golden_set();
        s.jobs[0].prompt_token_ids.push(4);
        assert!(s.validate_shape(4096).is_ok());
        assert_ne!(base, s.golden_root());

        // MEASURED build identity is part of the root, not just of the header. Two sets that
        // agree on class, model, shape, jobs AND every expectation, and differ only in the build
        // they were measured under, are different sets — this is the OpenMP case, where the class
        // label is identical and the arithmetic is not.
        let mut s = test_golden_set();
        s.cmake_cache_sha256[0] ^= 1;
        assert!(s.validate_shape(4096).is_ok(), "a differently-configured build is well-formed, just not ours");
        assert_ne!(base, s.golden_root(), "the CMake configuration is part of the root");

        let mut s = test_golden_set();
        s.llama_static_library_sha256[31] ^= 1;
        assert_ne!(base, s.golden_root(), "the linked llama.cpp/ggml archives are part of the root");
    }

    /// The golden-set schema version must stay INDEPENDENT of the job-wire version. They were one
    /// constant, which meant the set's schema could not gain a field without invalidating every
    /// v2 wire message — envelope, result, health, capability, and the agent↔worker UDS protocol.
    /// A future refactor that re-merges them would silently reintroduce that coupling, so the
    /// separation is asserted rather than left to a comment.
    #[test]
    fn golden_set_version_is_independent_of_the_job_wire_version() {
        assert_ne!(
            PALW_GOLDEN_SET_VERSION_V2, PALW_JOB_WIRE_VERSION_V2,
            "a local artifact's schema and the peer wire protocol must be able to move separately"
        );
        // A set written under the previous layout is refused EXPLICITLY, by version, rather than
        // surfacing as a bare Borsh decode error at some offset.
        let mut s = test_golden_set();
        s.version = PALW_JOB_WIRE_VERSION_V2;
        assert_eq!(
            s.validate_shape(4096),
            Err(PalwV2Error::UnsupportedVersion { got: PALW_JOB_WIRE_VERSION_V2, expected: PALW_GOLDEN_SET_VERSION_V2 })
        );
        // The envelopes a set derives still speak the JOB WIRE version — the set's own bump must
        // not leak into the protocol the worker and agent talk.
        let s = test_golden_set();
        assert_eq!(s.envelope_for(&s.jobs[0]).version, PALW_JOB_WIRE_VERSION_V2);
    }

    #[test]
    fn golden_set_rejections_fail_closed() {
        let mut s = test_golden_set();
        s.jobs.clear();
        assert!(matches!(s.validate_shape(4096), Err(PalwV2Error::GoldenSetInvalid(_))));

        let mut s = test_golden_set();
        s.jobs[1].name = "probe-a".into();
        assert!(matches!(s.validate_shape(4096), Err(PalwV2Error::GoldenSetInvalid(_))));

        let mut s = test_golden_set();
        s.version = 1;
        assert!(matches!(s.validate_shape(4096), Err(PalwV2Error::UnsupportedVersion { .. })));

        // A malformed job inside the set is caught by the shared envelope predicate.
        let mut s = test_golden_set();
        s.jobs[0].exact_decode_tokens = 0;
        assert!(matches!(s.validate_shape(4096), Err(PalwV2Error::ZeroDecodeBudget)));

        let mut s = test_golden_set();
        s.jobs[0].max_context_tokens = 2048;
        assert!(matches!(s.validate_shape(4096), Err(PalwV2Error::ContextProfileMismatch { .. })));

        // Borsh round trip for the file format.
        let s = test_golden_set();
        let bytes = borsh::to_vec(&s).unwrap();
        let back: PalwGoldenVectorSetV2 = decode_framed_borsh(&bytes).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn agent_wire_round_trips_and_fails_closed() {
        let req = PalwAgentRequestV1::Job(test_envelope());
        let bytes = borsh::to_vec(&req).unwrap();
        assert_eq!(bytes[0], 0, "Job discriminant");
        let back: PalwAgentRequestV1 = decode_framed_borsh(&bytes).unwrap();
        assert_eq!(req, back);

        let health = borsh::to_vec(&PalwAgentRequestV1::Health).unwrap();
        assert_eq!(health, vec![1], "Health is exactly one tag byte");

        // Unknown request discriminant fails closed.
        assert!(matches!(decode_framed_borsh::<PalwAgentRequestV1>(&[9]), Err(PalwV2Error::Decode(_))));

        let resp = PalwAgentResponseV1::JobRejected { code: "busy".into(), message: "slot occupied".into() };
        let bytes = borsh::to_vec(&resp).unwrap();
        assert_eq!(bytes[0], 1, "JobRejected discriminant");
        let back: PalwAgentResponseV1 = decode_framed_borsh(&bytes).unwrap();
        assert_eq!(resp, back);

        let health_resp = PalwAgentResponseV1::Health(PalwAgentHealthV1 {
            state: PalwAgentStateV1::Quarantined,
            selftest_passed: false,
            runtime_manifest_hash: h64(1),
            golden_vector_root: h64(2),
            max_context_tokens: 4096,
            jobs_total: 5,
            jobs_ok: 1,
            jobs_rejected: 3,
            jobs_failed: 1,
            timeouts_total: 0,
        });
        let bytes = borsh::to_vec(&health_resp).unwrap();
        let back: PalwAgentResponseV1 = decode_framed_borsh(&bytes).unwrap();
        assert_eq!(health_resp, back);
    }

    #[test]
    fn manifest_hash_binds_every_pin() {
        let base = PalwRuntimeManifestV2 {
            version: PALW_RUNTIME_MANIFEST_VERSION_V2,
            target_arch: "x86_64".into(),
            target_triple: "x86_64-unknown-linux-gnu".into(),
            compiler_name: "rustc+cc".into(),
            compiler_version: "rustc 1.85.0".into(),
            linker_version: "unpinned".into(),
            cmake_cache_sha256: [1; 32],
            worker_binary_sha256: [2; 32],
            llama_static_library_sha256: [3; 32],
            llama_cpp_commit: "030ebb558a5820b444a8f836ed5cdd46c9b4bd7a".into(),
            patchset_root: "unpatched".into(),
            exact_cpu_isa_baseline: "unpinned".into(),
            runtime_cpu_feature_mask: "sse4.2=1,avx=1,avx2=1,fma=1,f16c=1".into(),
            ggml_native: false,
            ggml_openmp: false,
            ggml_blas: false,
            ggml_accelerate: false,
            ggml_sse42: true,
            ggml_avx: true,
            ggml_avx2: true,
            ggml_fma: true,
            ggml_f16c: true,
            ggml_cpu_all_variants: false,
            thread_count: 4,
            thread_affinity_policy: "none/v1".into(),
            floating_point_environment: "rounding=rne,ftz=0,daz=0".into(),
            gguf_sha256: [4; 32],
            tokenizer_sha256: [4; 32],
            prompt_template_sha256: [5; 32],
            libm_identity: "glibc/2.39".to_string(),
            libm_arithmetic_digest: [6; 32],
            trace_scheme_id: trace_scheme_id_v2(),
            golden_vector_root: golden_vector_root_unpopulated_v2(),
        };
        let base_hash = base.manifest_hash();

        let mut m = base.clone();
        m.ggml_openmp = true;
        assert_ne!(base_hash, m.manifest_hash(), "OpenMP ON is a different runtime (negative control §11.3)");
        let mut m = base.clone();
        m.thread_count = 8;
        assert_ne!(base_hash, m.manifest_hash());
        let mut m = base.clone();
        m.gguf_sha256[0] ^= 1;
        assert_ne!(base_hash, m.manifest_hash(), "a one-bit GGUF change is a different manifest");
        let mut m = base.clone();
        m.floating_point_environment = "rounding=rne,ftz=1,daz=0".into();
        assert_ne!(base_hash, m.manifest_hash(), "FTZ change is a different runtime");
        let mut m = base.clone();
        m.ggml_cpu_all_variants = true;
        assert_ne!(base_hash, m.manifest_hash(), "runtime kernel dispatch is a different runtime");

        // audit B8: libm is normative PoW arithmetic (ADR-0031 Facts 2/4 — the GDN decay calls
        // `expf` per (token, head) across 18 of 24 layers), so a libm whose arithmetic differs by
        // one ulp MUST be a different class. Before v3 the manifest could not say so at all.
        let mut m = base.clone();
        m.libm_arithmetic_digest[0] ^= 1;
        assert_ne!(base_hash, m.manifest_hash(), "a one-bit libm arithmetic change is a different class");
        let mut m = base.clone();
        m.libm_identity = "musl/unversioned".into();
        assert_ne!(base_hash, m.manifest_hash(), "a different libm implementation is a different manifest");
    }

    /// The probe vector is frozen: its length and contents are class-identity inputs, so a silent
    /// edit would re-key every registered class. Pins the shape rather than a golden digest,
    /// because the digest is host arithmetic and this test must pass off-fleet too.
    #[test]
    fn libm_probe_vector_is_frozen() {
        assert_eq!(PALW_LIBM_PROBE_V1.len(), 12, "probe length is a class-identity input");
        // The edges that make the probe discriminating, asserted by value so a reorder is caught.
        assert_eq!(PALW_LIBM_PROBE_V1[0], 0x0000_0001, "smallest subnormal must lead");
        assert_eq!(PALW_LIBM_PROBE_V1[3], 0x3F31_7218, "ln 2 — the argument-reduction boundary");
        assert_eq!(PALW_LIBM_PROBE_V1[8], 0x42B0_0000, "the largest-finite-expf rounding case");
        assert_eq!(PALW_LIBM_PROBE_V1[11], 0x7F7F_FFFF, "f32::MAX must close the vector");
        // Every entry distinct: a duplicate would silently weaken the probe.
        let mut seen = PALW_LIBM_PROBE_V1.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), PALW_LIBM_PROBE_V1.len(), "probe entries must be distinct");
    }

    /// **Commit / open / verify, held together.** The tree already had the shape; only the proof
    /// was missing, and a committed root with no opening API is strictly worse than committing to
    /// nothing — every dispute against such a block terminates `Unadjudicable`, which ADR-0038 I10
    /// makes rejected-but-unslashed and freezes the class. It would mint work nothing can be held
    /// to.
    ///
    /// Swept over every event count through the odd/even and promotion boundaries, and every index
    /// within each: promotion is the case a hand-picked size misses, because it removes a level
    /// from SOME paths and not others.
    #[test]
    fn every_trace_event_opens_under_the_committed_root() {
        for count in 1usize..=17 {
            let events: Vec<Hash64> = (0..count).map(|i| h64(0xE0 + i as u8)).collect();
            let root = trace_event_merkle_root_v2(&events).expect("roots");
            for index in 0..count as u32 {
                let opening = trace_event_opening_v2(&events, index).expect("opens");
                assert_eq!(opening.event_hash, events[index as usize]);
                assert_eq!(
                    trace_event_opening_root_v2(count as u32, &opening).expect("verifies"),
                    root,
                    "count {count}, index {index}"
                );
                assert!(
                    opening.siblings.len() <= PALW_V2_MAX_TRACE_OPENING_SIBLINGS,
                    "count {count}, index {index}: {} siblings",
                    opening.siblings.len()
                );
            }
        }
    }

    /// The forgeries the construction exists to refuse. Each is a way to make one event hash prove
    /// membership somewhere it does not sit.
    #[test]
    fn a_trace_opening_cannot_be_moved_forged_or_padded() {
        let events: Vec<Hash64> = (0..6).map(|i| h64(0xE0 + i as u8)).collect();
        let root = trace_event_merkle_root_v2(&events).expect("roots");
        let opening = trace_event_opening_v2(&events, 2).expect("opens");

        // Replayed at another index: the index is inside the leaf, so the root moves.
        let mut moved = opening.clone();
        moved.event_index = 3;
        assert_ne!(trace_event_opening_root_v2(6, &moved).expect("computes"), root, "an index-swapped opening must not verify");

        // A different event under the same path.
        let mut swapped = opening.clone();
        swapped.event_hash = h64(0xFF);
        assert_ne!(trace_event_opening_root_v2(6, &swapped).expect("computes"), root);

        // A path with anything left over is REFUSED rather than ignored — accepting slack would
        // let one hash prove membership at more than one position.
        let mut padded = opening.clone();
        padded.siblings.push(h64(0x99));
        assert!(matches!(trace_event_opening_root_v2(6, &padded), Err(PalwV2Error::OpeningPathTooLong { extra: 1 })));

        let mut short = opening.clone();
        short.siblings.pop();
        assert!(matches!(trace_event_opening_root_v2(6, &short), Err(PalwV2Error::OpeningPathTooShort)));

        // The count decides the tree SHAPE, so an opening cut for one count generally will not
        // reconstruct under another — here the 5-leaf path for index 4 is two promotions and one
        // sibling, and the 6-leaf path needs two siblings.
        let five: Vec<Hash64> = (0..5).map(|i| h64(0xE0 + i as u8)).collect();
        let promoted = trace_event_opening_v2(&five, 4).expect("opens");
        assert!(matches!(trace_event_opening_root_v2(6, &promoted), Err(PalwV2Error::OpeningPathTooShort)));
        assert!(matches!(trace_event_opening_root_v2(2, &opening), Err(PalwV2Error::EventIndexOutOfRange { .. })));

        // But shape alone is NOT what stops a prover restating the count — two counts can imply
        // the same path for a given index, and this one does: index 2 opens identically under 6
        // and 7 leaves. What actually binds the count is the OUTER root, which commits to
        // `summary.event_count`; the sampler test below is where that is checked, and this
        // assertion records why the check has to live there rather than here.
        assert_eq!(
            trace_event_opening_root_v2(7, &opening).expect("computes"),
            root,
            "the shape happens to agree at this index — the count is bound by the outer root, not by this function"
        );
        assert!(matches!(trace_event_opening_v2(&events, 6), Err(PalwV2Error::EventIndexOutOfRange { .. })));
        assert!(matches!(trace_event_opening_v2(&[], 0), Err(PalwV2Error::EmptyTrace)));
    }

    /// The opening is against the SAME root a real commitment carries, not against a bare tree —
    /// `full_logits_trace_root_v2` wraps the event root with the summary, so a test that stopped at
    /// the inner root would not show that a sampler holding a block commitment can open anything.
    #[test]
    fn a_sampler_holding_the_commitment_can_open_one_event() {
        let events: Vec<Hash64> = (0..5).map(|i| h64(0xA0 + i as u8)).collect();
        let inner = trace_event_merkle_root_v2(&events).expect("roots");
        let ctx_hash = h64(0xC7);
        let summary = PalwTraceSummaryV2 {
            vocab_size: 32,
            logits_dtype: PalwLogitsDtypeV2::F32Le,
            declared_prefill_tokens: 3,
            exact_decode_tokens: 5,
            event_count: 5,
            first_event_kind: PalwTracePhaseV2::Prefill,
            last_event_kind: PalwTracePhaseV2::Decode,
            output_token_ids_hash: h64(0x11),
            stop_reason: PalwStopReasonV2::ExactBudgetReached,
        };
        let outer = full_logits_trace_root_v2(&ctx_hash, &summary, &inner);

        // What a sampler does: take the count from the summary the outer root binds, verify the
        // opening against the inner root, and re-derive the outer root from it.
        let opening = trace_event_opening_v2(&events, 4).expect("opens");
        let reopened = trace_event_opening_root_v2(summary.event_count, &opening).expect("verifies");
        assert_eq!(full_logits_trace_root_v2(&ctx_hash, &summary, &reopened), outer);

        // And a summary that lies about the count cannot rescue a mismatched opening: the count is
        // inside the outer root, so restating it changes the root the block committed to.
        let mut lying = summary.clone();
        lying.event_count = 4;
        assert_ne!(full_logits_trace_root_v2(&ctx_hash, &lying, &reopened), outer);
    }
}
