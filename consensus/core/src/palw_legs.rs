//! PALW execution-commitment legs v1 — the activation and checkpoint commitments that turn the
//! v2 logits trace into refutable execution evidence.
//!
//! Normative sources: ADR-0026 §2 (proof deeper than logits), ADR-0027 §1/§2 (the refutation
//! model these legs exist to feed) and its consequences ("activation + GEMM legs are the next
//! Land increment under the existing Merkle discipline — new leaf kinds and profile pins, not a
//! new mechanism"), the v0.1 slash design §6 (composite proof material, checkpoint ancestry).
//!
//! # Scope and stage — read this before wiring anything to consensus
//!
//! **Land**-stage code: types, hashing, and adjudication predicates only. Nothing in consensus
//! validation, fork choice, the header pipeline, or acceptance consumes it. The worker does not
//! yet *capture* activations or checkpoints — instrumenting the runtime is its own increment,
//! and the schema is deliberately frozen first so the capture code has one layout to hit
//! (the same order v2 itself was built in).
//!
//! # What is pinned here vs. what is deliberately opaque
//!
//! Pinned by this module: every preimage layout, the leaf trees and their derived ordering, the
//! ancestry chain, the composite root, and the structural fault rules. Deliberately opaque —
//! carried as identities and pinned at class/profile registration, after measurement:
//!
//! * **`tap_semantics_id`** — *which tensor* a "tap" reads (e.g. the post-block residual
//!   stream). Cross-runtime tap semantics are a measurement question; freezing a guess here
//!   would be inventing determinism the fleet has not demonstrated.
//! * **`state_layout_id`** — the canonical byte layout of a checkpoint's replay state (KV
//!   layout, dtype, ordering). Same reason, stronger: the runtime's KV cache is not even f32.
//! * Tap layer indices and the checkpoint interval are **profile parameters**, validated for
//!   shape here, given values at registration.
//! * The **GEMM/tile leg is absent**, not pending: it requires the step function pinned at tile
//!   granularity (ADR-0027 consequences), which does not exist. Adding it is a new commitment
//!   version, never a field bolted onto this one.
//!
//! # Scheme identity
//!
//! The composite is a **new scheme family** (`misaka-palw/execution-commitment/v1`), not a v3 of
//! the logits trace: the logits leg it binds is *exactly* the frozen v2 root, so leg-era job
//! contexts keep `trace_scheme_id = trace_scheme_id_v2()` (changing it would fork every v2
//! golden for no semantic reason). Which commitment form a class produces — bare v2 root or
//! composite — is a registry/manifest fact, decided at registration, not encoded in the context.
//!
//! # Why exact rows, not projections
//!
//! ADR-0026 §2 sketched `Project(S8_i)` — a size optimization inherited from the Ambient
//! reading. Under ADR-0027 the legs feed *openings and one-step recomputation*, and a projection
//! is neither an input state nor cheaper to refute; the hash already is the compression, and
//! only challenged rows ever travel. So leaves commit the **exact canonical f32-LE bytes**, and
//! the v2 fail-closed rule extends: a non-finite value in any committed row is itself a
//! refutable fault — an honest execution aborts with no receipt instead of committing it.
//!
//! # The ancestry win
//!
//! Checkpoint leaves chain (`prev_checkpoint_leaf_hash`, genesis-bound to the job context), so
//! v0.1 §17.2's `M-C4` "broken checkpoint ancestry" — a *computation* offense needing an appeal
//! jury there — becomes an **objective** offense here: two adjacent openings whose hashes do not
//! chain convict by themselves, no recomputation, no jury (ADR-0027 §4 table gains a row).

use crate::palw_slash::{check_job_context_shape, PalwSlashError};
use crate::palw_v2::{PalwJobContextV2, PalwLogitsDtypeV2, PALW_V2_MAX_TRACE_EVENTS};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::Hash64;
use thiserror::Error;

// ---------------------------------------------------------------------------------------------
// Versions, domains, caps
// ---------------------------------------------------------------------------------------------

/// Wire version of every legs-v1 object in this module.
pub const PALW_LEGS_OBJECT_VERSION_V1: u16 = 1;

/// The composite scheme name; [`execution_commitment_scheme_id_v1`] is its identity.
pub const PALW_EXECUTION_COMMITMENT_SCHEME_NAME_V1: &str = "misaka-palw/execution-commitment/v1";

pub const PALW_LEGS_DOMAIN_SCHEME_ID: &[u8] = b"misaka-palw/execution-commitment-scheme-id/v1";
pub const PALW_LEGS_DOMAIN_TAP_PROFILE: &[u8] = b"misaka-palw/activation-tap-profile/v1";
pub const PALW_LEGS_DOMAIN_CHECKPOINT_PROFILE: &[u8] = b"misaka-palw/checkpoint-profile/v1";
pub const PALW_LEGS_DOMAIN_ACTIVATION_LEAF: &[u8] = b"misaka-palw/activation-leaf/v1";
pub const PALW_LEGS_DOMAIN_ACTIVATION_MERKLE_LEAF: &[u8] = b"misaka-palw/activation-merkle-leaf/v1";
pub const PALW_LEGS_DOMAIN_ACTIVATION_MERKLE_NODE: &[u8] = b"misaka-palw/activation-merkle-node/v1";
pub const PALW_LEGS_DOMAIN_ACTIVATION_LEG: &[u8] = b"misaka-palw/activation-leg/v1";
pub const PALW_LEGS_DOMAIN_CHECKPOINT_LEAF: &[u8] = b"misaka-palw/checkpoint-leaf/v1";
pub const PALW_LEGS_DOMAIN_CHECKPOINT_MERKLE_LEAF: &[u8] = b"misaka-palw/checkpoint-merkle-leaf/v1";
pub const PALW_LEGS_DOMAIN_CHECKPOINT_MERKLE_NODE: &[u8] = b"misaka-palw/checkpoint-merkle-node/v1";
pub const PALW_LEGS_DOMAIN_CHECKPOINT_LEG: &[u8] = b"misaka-palw/checkpoint-leg/v1";
pub const PALW_LEGS_DOMAIN_CHECKPOINT_GENESIS: &[u8] = b"misaka-palw/checkpoint-genesis/v1";
pub const PALW_LEGS_DOMAIN_CHECKPOINT_EMPTY: &[u8] = b"misaka-palw/checkpoint-empty/v1";
pub const PALW_LEGS_DOMAIN_EXECUTION_COMMITMENT: &[u8] = b"misaka-palw/execution-commitment/v1";
pub const PALW_LEGS_DOMAIN_EVIDENCE_ID: &[u8] = b"misaka-palw/legs-refutation-evidence-id/v1";

/// Every domain this module introduces (uniqueness-tested against the v2 / PALW-S / reference
/// lists — one string shared across families is a preimage bridge).
pub const PALW_LEGS_ALL_DOMAINS: &[&[u8]] = &[
    PALW_LEGS_DOMAIN_SCHEME_ID,
    PALW_LEGS_DOMAIN_TAP_PROFILE,
    PALW_LEGS_DOMAIN_CHECKPOINT_PROFILE,
    PALW_LEGS_DOMAIN_ACTIVATION_LEAF,
    PALW_LEGS_DOMAIN_ACTIVATION_MERKLE_LEAF,
    PALW_LEGS_DOMAIN_ACTIVATION_MERKLE_NODE,
    PALW_LEGS_DOMAIN_ACTIVATION_LEG,
    PALW_LEGS_DOMAIN_CHECKPOINT_LEAF,
    PALW_LEGS_DOMAIN_CHECKPOINT_MERKLE_LEAF,
    PALW_LEGS_DOMAIN_CHECKPOINT_MERKLE_NODE,
    PALW_LEGS_DOMAIN_CHECKPOINT_LEG,
    PALW_LEGS_DOMAIN_CHECKPOINT_GENESIS,
    PALW_LEGS_DOMAIN_CHECKPOINT_EMPTY,
    PALW_LEGS_DOMAIN_EXECUTION_COMMITMENT,
    PALW_LEGS_DOMAIN_EVIDENCE_ID,
];

/// Most taps a profile may declare.
pub const PALW_LEGS_MAX_TAPS: usize = 16;
/// Largest hidden dimension a tap profile may declare.
pub const PALW_LEGS_MAX_HIDDEN_DIM: u32 = 65_536;
/// Cap on activation-tree leaves: `MAX_TAPS × (MAX_PROMPT + MAX_EVENTS − 1)`, rounded up to a
/// clean power of two.
pub const PALW_LEGS_MAX_ACTIVATION_LEAVES: usize = 1 << 17;
/// Cap on checkpoint-tree leaves (one per interval boundary among ≤ 4095 decode calls).
pub const PALW_LEGS_MAX_CHECKPOINTS: usize = PALW_V2_MAX_TRACE_EVENTS;
/// Deepest opening either leg tree can require: `ceil(log2(MAX_ACTIVATION_LEAVES))`.
pub const PALW_LEGS_MAX_OPENING_SIBLINGS: usize = PALW_LEGS_MAX_ACTIVATION_LEAVES.ilog2() as usize;
/// Cap on one carried activation row (bytes): `4 × MAX_HIDDEN_DIM`.
pub const PALW_LEGS_MAX_ROW_BYTES: usize = 4 * PALW_LEGS_MAX_HIDDEN_DIM as usize;

pub fn execution_commitment_scheme_id_v1() -> Hash64 {
    keyed64(PALW_LEGS_DOMAIN_SCHEME_ID, &[PALW_EXECUTION_COMMITMENT_SCHEME_NAME_V1.as_bytes()])
}

// ---------------------------------------------------------------------------------------------
// Errors — same two-meaning split as `palw_slash`: malformed/unaddressed evidence rejects, and
// NoFaultFound is the verdict that costs a challenger their bond at acceptance stage.
// ---------------------------------------------------------------------------------------------

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwLegsError {
    #[error("unsupported palw-legs object version {got} (expected {expected})")]
    UnsupportedVersion { got: u16, expected: u16 },
    #[error("job context is malformed: {0}")]
    Context(PalwSlashError),
    #[error("leaf count {got} is outside 1..={max}")]
    LeafCountOutOfRange { got: u32, max: usize },
    #[error("leaf index {index} is not below leaf count {count}")]
    LeafIndexOutOfRange { index: u32, count: u32 },
    #[error("opening carries {got} siblings, exceeding the {max}-level cap")]
    OpeningTooDeep { got: usize, max: usize },
    #[error("opening path ended short of the root")]
    OpeningPathTooShort,
    #[error("opening path carries {extra} sibling(s) past the root")]
    OpeningPathTooLong { extra: usize },
    #[error("opening does not reproduce the committed {tree} merkle root")]
    OpeningRootMismatch { tree: &'static str },
    #[error("carried binding does not recompute the committed execution commitment root")]
    CommittedRootMismatch,
    #[error("carried {leaf} preimage does not hash to the opened leaf hash")]
    LeafPreimageMismatch { leaf: &'static str },
    #[error("carried activation row exceeds the {max}-byte cap (got {got})")]
    RowBytesTooLarge { got: usize, max: usize },
    #[error("chain evidence must open adjacent checkpoint indices (got {earlier} then {later})")]
    ChainEvidenceNotAdjacent { earlier: u32, later: u32 },
    #[error("the addressed material is honest under every pinned rule — refutation rejected")]
    NoFaultFound,
}

fn keyed64(key: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for p in parts {
        h.update(p);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// Little-endian canonical writer (field order = struct order, var-length fields u32-prefixed) —
/// the same conventions every PALW preimage uses.
struct Writer(Vec<u8>);
impl Writer {
    fn new() -> Self {
        Self(Vec::with_capacity(256))
    }
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn hash64(&mut self, v: &Hash64) {
        self.0.extend_from_slice(v.as_byte_slice());
    }
    fn u16_seq(&mut self, v: &[u16]) {
        self.u32(v.len() as u32);
        for x in v {
            self.u16(*x);
        }
    }
    fn keyed64(self, domain: &[u8]) -> Hash64 {
        keyed64(domain, &[&self.0])
    }
}

// ---------------------------------------------------------------------------------------------
// Profiles — shape validated here, values pinned at registration
// ---------------------------------------------------------------------------------------------

/// Which layers are tapped and what a tap row looks like. `tap_semantics_id` names *which
/// tensor* a tap reads — an opaque identity whose preimage is defined at registration, exactly
/// like `state_layout_id` below and `tokenizer_id` before it.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwActivationTapProfileV1 {
    /// = [`PALW_LEGS_OBJECT_VERSION_V1`].
    pub version: u16,
    pub tap_semantics_id: Hash64,
    /// Strictly ascending, non-empty, every index below `model_total_layers`.
    pub tap_layer_indices: Vec<u16>,
    pub model_total_layers: u16,
    pub hidden_dim: u32,
    pub dtype: PalwLogitsDtypeV2,
}

impl PalwActivationTapProfileV1 {
    pub fn validate_shape(&self) -> Result<(), &'static str> {
        if self.version != PALW_LEGS_OBJECT_VERSION_V1 {
            return Err("tap profile version is not v1");
        }
        if self.tap_layer_indices.is_empty() || self.tap_layer_indices.len() > PALW_LEGS_MAX_TAPS {
            return Err("tap count is empty or over the cap");
        }
        if !self.tap_layer_indices.windows(2).all(|w| w[0] < w[1]) {
            return Err("tap layers are not strictly ascending");
        }
        if *self.tap_layer_indices.last().expect("non-empty") >= self.model_total_layers {
            return Err("a tap layer is not below the model layer count");
        }
        if self.hidden_dim == 0 || self.hidden_dim > PALW_LEGS_MAX_HIDDEN_DIM {
            return Err("hidden dim is zero or over the cap");
        }
        Ok(())
    }

    pub fn profile_hash(&self) -> Hash64 {
        let mut w = Writer::new();
        w.u16(self.version);
        w.hash64(&self.tap_semantics_id);
        w.u16_seq(&self.tap_layer_indices);
        w.u16(self.model_total_layers);
        w.u32(self.hidden_dim);
        w.u8(self.dtype.wire_byte());
        w.keyed64(PALW_LEGS_DOMAIN_TAP_PROFILE)
    }

    pub fn tap_count(&self) -> u32 {
        self.tap_layer_indices.len() as u32
    }
}

/// How often a replay checkpoint is committed and what its state bytes mean. `state_layout_id`
/// is opaque here: the canonical KV/replay-state byte layout is runtime-measurement work and is
/// pinned at registration — freezing a guess would commit the network to bytes no runtime has
/// demonstrated it can produce deterministically.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwCheckpointProfileV1 {
    /// = [`PALW_LEGS_OBJECT_VERSION_V1`].
    pub version: u16,
    /// A checkpoint after every `checkpoint_interval` decode calls. ≥ 1.
    pub checkpoint_interval: u32,
    pub state_layout_id: Hash64,
}

impl PalwCheckpointProfileV1 {
    pub fn validate_shape(&self) -> Result<(), &'static str> {
        if self.version != PALW_LEGS_OBJECT_VERSION_V1 {
            return Err("checkpoint profile version is not v1");
        }
        if self.checkpoint_interval == 0 {
            return Err("checkpoint interval is zero");
        }
        Ok(())
    }

    pub fn profile_hash(&self) -> Hash64 {
        let mut w = Writer::new();
        w.u16(self.version);
        w.u32(self.checkpoint_interval);
        w.hash64(&self.state_layout_id);
        w.keyed64(PALW_LEGS_DOMAIN_CHECKPOINT_PROFILE)
    }
}

// ---------------------------------------------------------------------------------------------
// Canonical counts and coordinates — pure functions of (context, profiles); every honest
// commitment satisfies them and every violation is a refutable fault.
// ---------------------------------------------------------------------------------------------

/// Decode calls under the exact-decode schedule: `D` events = 1 prefill + `D−1` decode calls.
/// Total (saturating): a shape-invalid context with `D = 0` is rejected by every checker before
/// this value is used, but a public helper must not be able to panic on any input.
pub fn canonical_decode_calls(context: &PalwJobContextV2) -> u32 {
    context.exact_decode_tokens.saturating_sub(1)
}

/// Activation leaves: every tap taps every position of the prefill call and the single position
/// of every decode call — `taps × (P + (D−1))`.
pub fn canonical_activation_leaf_count(context: &PalwJobContextV2, taps: u32) -> u64 {
    taps as u64 * (context.declared_prefill_tokens as u64 + canonical_decode_calls(context) as u64)
}

/// Checkpoints: one after every `interval` decode calls — `⌊(D−1) / interval⌋`.
pub fn canonical_checkpoint_count(context: &PalwJobContextV2, interval: u32) -> u32 {
    canonical_decode_calls(context) / interval
}

/// The pinned leaf order of the activation tree: prefill positions first (tap-major within a
/// call), then each decode call's single position. Returns `None` when the coordinates are not
/// canonical for `(context, taps)` — which, on a committed leaf, is itself the fault.
pub fn canonical_activation_leaf_index(
    context: &PalwJobContextV2,
    taps: u32,
    call_index: u32,
    tap_slot: u32,
    position: u32,
) -> Option<u64> {
    let prefill = context.declared_prefill_tokens;
    if tap_slot >= taps || call_index >= context.exact_decode_tokens {
        return None;
    }
    if call_index == 0 {
        if position >= prefill {
            return None;
        }
        Some(tap_slot as u64 * prefill as u64 + position as u64)
    } else {
        if position != 0 {
            return None;
        }
        Some(taps as u64 * prefill as u64 + (call_index as u64 - 1) * taps as u64 + tap_slot as u64)
    }
}

// ---------------------------------------------------------------------------------------------
// Leaves
// ---------------------------------------------------------------------------------------------

/// The transparent preimage of one committed activation row. `values_le_bytes` is the exact
/// canonical byte payload (f32 little-endian); like the logits refutation preimage, it can carry
/// rows an honest producer could never emit — that is what makes them refutable.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwActivationLeafV1 {
    pub call_index: u32,
    pub tap_slot: u32,
    pub position: u32,
    pub hidden_dim: u32,
    pub value_count: u32,
    pub values_le_bytes: Vec<u8>,
}

/// Adjudication-side hash of an activation leaf. Performs no validity checks — validity is what
/// the fault scan judges afterwards.
pub fn activation_leaf_hash_v1(job_context_hash: &Hash64, tap_profile_hash: &Hash64, leaf: &PalwActivationLeafV1) -> Hash64 {
    let mut w = Writer::new();
    w.hash64(job_context_hash);
    w.hash64(tap_profile_hash);
    w.u32(leaf.call_index);
    w.u32(leaf.tap_slot);
    w.u32(leaf.position);
    w.u32(leaf.hidden_dim);
    w.u8(PalwLogitsDtypeV2::F32Le.wire_byte());
    w.u32(leaf.value_count);
    keyed64(PALW_LEGS_DOMAIN_ACTIVATION_LEAF, &[&w.0, &leaf.values_le_bytes])
}

/// The transparent preimage of one committed checkpoint. `state_root` is the (opaque at this
/// stage) commitment to the canonical replay state under `state_layout_id`; the ancestry chain
/// is what this module can already adjudicate.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwCheckpointLeafV1 {
    pub checkpoint_index: u32,
    /// The decode call this checkpoint covers: `(checkpoint_index + 1) × interval`.
    pub covered_decode_call: u32,
    pub state_layout_id: Hash64,
    pub state_root: Hash64,
    pub prev_checkpoint_leaf_hash: Hash64,
}

pub fn checkpoint_leaf_hash_v1(job_context_hash: &Hash64, checkpoint_profile_hash: &Hash64, leaf: &PalwCheckpointLeafV1) -> Hash64 {
    let mut w = Writer::new();
    w.hash64(job_context_hash);
    w.hash64(checkpoint_profile_hash);
    w.u32(leaf.checkpoint_index);
    w.u32(leaf.covered_decode_call);
    w.hash64(&leaf.state_layout_id);
    w.hash64(&leaf.state_root);
    w.hash64(&leaf.prev_checkpoint_leaf_hash);
    w.keyed64(PALW_LEGS_DOMAIN_CHECKPOINT_LEAF)
}

/// What checkpoint 0 must carry as its predecessor: job-context-bound, so a chain (or any suffix
/// of one) can never be transplanted between jobs.
pub fn checkpoint_genesis_prev_v1(job_context_hash: &Hash64) -> Hash64 {
    keyed64(PALW_LEGS_DOMAIN_CHECKPOINT_GENESIS, &[job_context_hash.as_byte_slice()])
}

/// The committed value of an EMPTY checkpoint tree (interval longer than the decode run). An
/// explicit sentinel, not zeroes — absence must be visible, and it is domain-separated from
/// every real root.
pub fn checkpoint_empty_root_v1(job_context_hash: &Hash64) -> Hash64 {
    keyed64(PALW_LEGS_DOMAIN_CHECKPOINT_EMPTY, &[job_context_hash.as_byte_slice()])
}

// ---------------------------------------------------------------------------------------------
// The leg Merkle — the v2 construction (index-bound leaves, domain-separated nodes, odd nodes
// promoted unchanged), generalized over the two leg domains. The equivalence test freezes this
// generalization against BOTH the v2 production root and the PALW-S opening verifier.
// ---------------------------------------------------------------------------------------------

/// Membership proof of one leaf in a leg tree; `siblings` carry only paired levels — promote
/// levels are derived from `(leaf_index, leaf_count)` and consume nothing (an opening cannot
/// smuggle its own tree shape).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwLegOpeningV1 {
    pub leaf_index: u32,
    pub leaf_hash: Hash64,
    pub siblings: Vec<Hash64>,
}

fn leg_merkle_leaf(leaf_domain: &[u8], index: u32, leaf_hash: &Hash64) -> Hash64 {
    let mut w = Writer::new();
    w.u32(index);
    w.hash64(leaf_hash);
    w.keyed64(leaf_domain)
}

/// Root of a leg tree over ordered leaf hashes.
pub fn leg_merkle_root_v1(
    leaf_domain: &[u8],
    node_domain: &[u8],
    ordered_leaf_hashes: &[Hash64],
    max_leaves: usize,
) -> Result<Hash64, PalwLegsError> {
    if ordered_leaf_hashes.is_empty() || ordered_leaf_hashes.len() > max_leaves {
        return Err(PalwLegsError::LeafCountOutOfRange { got: ordered_leaf_hashes.len() as u32, max: max_leaves });
    }
    let mut level: Vec<Hash64> =
        ordered_leaf_hashes.iter().enumerate().map(|(i, leaf)| leg_merkle_leaf(leaf_domain, i as u32, leaf)).collect();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut chunks = level.chunks_exact(2);
        for pair in &mut chunks {
            next.push(keyed64(node_domain, &[pair[0].as_byte_slice(), pair[1].as_byte_slice()]));
        }
        if let [odd] = chunks.remainder() {
            next.push(*odd);
        }
        level = next;
    }
    Ok(level[0])
}

/// Recomputes the root a valid opening implies; the caller compares it to the committed root.
pub fn leg_opening_root_v1(
    leaf_domain: &[u8],
    node_domain: &[u8],
    leaf_count: u32,
    opening: &PalwLegOpeningV1,
    max_leaves: usize,
) -> Result<Hash64, PalwLegsError> {
    if leaf_count == 0 || leaf_count as usize > max_leaves {
        return Err(PalwLegsError::LeafCountOutOfRange { got: leaf_count, max: max_leaves });
    }
    if opening.leaf_index >= leaf_count {
        return Err(PalwLegsError::LeafIndexOutOfRange { index: opening.leaf_index, count: leaf_count });
    }
    if opening.siblings.len() > PALW_LEGS_MAX_OPENING_SIBLINGS {
        return Err(PalwLegsError::OpeningTooDeep { got: opening.siblings.len(), max: PALW_LEGS_MAX_OPENING_SIBLINGS });
    }
    let mut current = leg_merkle_leaf(leaf_domain, opening.leaf_index, &opening.leaf_hash);
    let mut position = opening.leaf_index as usize;
    let mut width = leaf_count as usize;
    let mut siblings = opening.siblings.iter();
    while width > 1 {
        let promoted = !width.is_multiple_of(2) && position == width - 1;
        if !promoted {
            let Some(sibling) = siblings.next() else {
                return Err(PalwLegsError::OpeningPathTooShort);
            };
            current = if position.is_multiple_of(2) {
                keyed64(node_domain, &[current.as_byte_slice(), sibling.as_byte_slice()])
            } else {
                keyed64(node_domain, &[sibling.as_byte_slice(), current.as_byte_slice()])
            };
        }
        position /= 2;
        width = width.div_ceil(2);
    }
    let leftover = siblings.count();
    if leftover != 0 {
        return Err(PalwLegsError::OpeningPathTooLong { extra: leftover });
    }
    Ok(current)
}

// ---------------------------------------------------------------------------------------------
// Leg roots and the composite
// ---------------------------------------------------------------------------------------------

/// Outer hash of the activation leg: context, tap profile, the canonical counts, and the tree.
pub fn activation_leg_root_v1(
    job_context_hash: &Hash64,
    tap_profile_hash: &Hash64,
    prefill_tokens: u32,
    decode_calls: u32,
    leaf_count: u32,
    merkle_root: &Hash64,
) -> Hash64 {
    let mut w = Writer::new();
    w.u16(PALW_LEGS_OBJECT_VERSION_V1);
    w.hash64(job_context_hash);
    w.hash64(tap_profile_hash);
    w.u32(prefill_tokens);
    w.u32(decode_calls);
    w.u32(leaf_count);
    w.hash64(merkle_root);
    w.keyed64(PALW_LEGS_DOMAIN_ACTIVATION_LEG)
}

/// Outer hash of the checkpoint leg. An empty leg (`checkpoint_count = 0`) carries
/// [`checkpoint_empty_root_v1`] as its `merkle_root` — the canonical-form rule is
/// `count == 0 ⟺ merkle_root == empty sentinel`, and violating it is a fault.
pub fn checkpoint_leg_root_v1(
    job_context_hash: &Hash64,
    checkpoint_profile_hash: &Hash64,
    decode_calls: u32,
    checkpoint_count: u32,
    merkle_root: &Hash64,
) -> Hash64 {
    let mut w = Writer::new();
    w.u16(PALW_LEGS_OBJECT_VERSION_V1);
    w.hash64(job_context_hash);
    w.hash64(checkpoint_profile_hash);
    w.u32(decode_calls);
    w.u32(checkpoint_count);
    w.hash64(merkle_root);
    w.keyed64(PALW_LEGS_DOMAIN_CHECKPOINT_LEG)
}

/// The composite execution commitment: the frozen v2 logits root plus both legs, context-bound.
/// Profile hashes are already inside the leg roots and are NOT carried again (the dual-source
/// rule). There is no GEMM field: adding that leg is a new scheme version.
pub fn execution_commitment_root_v1(
    job_context_hash: &Hash64,
    full_logits_trace_root: &Hash64,
    activation_leg_root: &Hash64,
    checkpoint_leg_root: &Hash64,
) -> Hash64 {
    let mut w = Writer::new();
    w.u16(PALW_LEGS_OBJECT_VERSION_V1);
    w.hash64(job_context_hash);
    w.hash64(full_logits_trace_root);
    w.hash64(activation_leg_root);
    w.hash64(checkpoint_leg_root);
    w.keyed64(PALW_LEGS_DOMAIN_EXECUTION_COMMITMENT)
}

// ---------------------------------------------------------------------------------------------
// Refutations
// ---------------------------------------------------------------------------------------------

/// The pinned rule a legs refutation proves broken. Discriminants are wire-frozen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwLegsFaultV1 {
    /// The committed tap profile fails its own shape rules.
    TapProfileNotCanonical = 0,
    /// The committed checkpoint profile fails its own shape rules.
    CheckpointProfileNotCanonical = 1,
    /// `activation.leaf_count ≠ taps × (P + D−1)`.
    ActivationLeafCountNotCanonical = 2,
    /// `checkpoint_count ≠ ⌊(D−1)/interval⌋`, or the empty-sentinel rule is violated.
    CheckpointCountNotCanonical = 3,
    /// An opened activation leaf's `(call, tap, position)` is not a canonical coordinate.
    ActivationCoordinatesNotCanonical = 4,
    /// A canonical coordinate committed at the wrong tree index.
    ActivationLeafIndexNotCanonical = 5,
    /// The row byte payload is not `4 × value_count`.
    ActivationBytesNotFourPerValue = 6,
    /// The row declares `value_count ≠ hidden_dim`.
    ActivationValueCountNotHiddenDim = 7,
    /// The leaf's `hidden_dim` is not the profile's.
    ActivationHiddenDimNotProfile = 8,
    /// A committed activation value is non-finite (the fail-closed rule: an honest execution
    /// aborts with no receipt instead of committing it).
    ActivationNonFinite { value_index: u32 } = 9,
    /// An opened checkpoint leaf committed at a tree index other than its own
    /// `checkpoint_index`.
    CheckpointIndexNotCanonical = 10,
    /// `covered_decode_call ≠ (checkpoint_index + 1) × interval`.
    CheckpointCoveredCallNotCanonical = 11,
    /// The leaf's `state_layout_id` is not the profile's.
    CheckpointStateLayoutNotProfile = 12,
    /// Checkpoint 0's `prev` is not the job-bound genesis value.
    CheckpointGenesisPrevMismatch = 13,
    /// Adjacent checkpoints whose hashes do not chain — v0.1 §17.2 `M-C4`, now objective.
    CheckpointChainBroken = 14,
}

impl PalwLegsFaultV1 {
    fn evidence_words(self) -> (u8, u32) {
        match self {
            PalwLegsFaultV1::TapProfileNotCanonical => (0, 0),
            PalwLegsFaultV1::CheckpointProfileNotCanonical => (1, 0),
            PalwLegsFaultV1::ActivationLeafCountNotCanonical => (2, 0),
            PalwLegsFaultV1::CheckpointCountNotCanonical => (3, 0),
            PalwLegsFaultV1::ActivationCoordinatesNotCanonical => (4, 0),
            PalwLegsFaultV1::ActivationLeafIndexNotCanonical => (5, 0),
            PalwLegsFaultV1::ActivationBytesNotFourPerValue => (6, 0),
            PalwLegsFaultV1::ActivationValueCountNotHiddenDim => (7, 0),
            PalwLegsFaultV1::ActivationHiddenDimNotProfile => (8, 0),
            PalwLegsFaultV1::ActivationNonFinite { value_index } => (9, value_index),
            PalwLegsFaultV1::CheckpointIndexNotCanonical => (10, 0),
            PalwLegsFaultV1::CheckpointCoveredCallNotCanonical => (11, 0),
            PalwLegsFaultV1::CheckpointStateLayoutNotProfile => (12, 0),
            PalwLegsFaultV1::CheckpointGenesisPrevMismatch => (13, 0),
            PalwLegsFaultV1::CheckpointChainBroken => (14, 0),
        }
    }
}

/// The shared transparent preimage of a committed execution commitment: everything needed to
/// recompute [`execution_commitment_root_v1`] from parts. Carried once per refutation; if it
/// does not reproduce the committed root, the refutation is about some other commitment and is
/// rejected before any fault is considered.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwLegsBindingV1 {
    /// = [`PALW_LEGS_OBJECT_VERSION_V1`].
    pub version: u16,
    pub job_context: PalwJobContextV2,
    pub tap_profile: PalwActivationTapProfileV1,
    pub checkpoint_profile: PalwCheckpointProfileV1,
    /// The v2 logits root, carried opaquely (its own refutations live in `palw_slash`).
    pub full_logits_trace_root: Hash64,
    pub activation_leaf_count: u32,
    pub activation_merkle_root: Hash64,
    pub checkpoint_count: u32,
    pub checkpoint_merkle_root: Hash64,
    /// The root the miner announced; the binding must recompute exactly it.
    pub committed_execution_root: Hash64,
}

/// The variable half of a refutation: which committed object is being refuted, with openings.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwLegsEvidenceV1 {
    /// No opening needed: a fault decidable from the binding alone (profiles, counts).
    Shape = 0,
    /// One opened activation leaf plus its claimed preimage.
    ActivationLeaf { opening: PalwLegOpeningV1, preimage: PalwActivationLeafV1 } = 1,
    /// One opened checkpoint leaf plus its claimed preimage.
    CheckpointLeaf { opening: PalwLegOpeningV1, preimage: PalwCheckpointLeafV1 } = 2,
    /// Two adjacent opened checkpoints; convicts iff their hashes do not chain.
    CheckpointChain {
        earlier_opening: PalwLegOpeningV1,
        earlier_preimage: PalwCheckpointLeafV1,
        later_opening: PalwLegOpeningV1,
        later_preimage: PalwCheckpointLeafV1,
    } = 3,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwLegsRefutationV1 {
    pub binding: PalwLegsBindingV1,
    pub evidence: PalwLegsEvidenceV1,
}

/// A finished adjudication: the fault plus the §24.1 dedup key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwLegsRefutationVerdictV1 {
    pub fault: PalwLegsFaultV1,
    pub evidence_id: Hash64,
}

fn evidence_id(committed_root: &Hash64, evidence_kind: u8, leaf_index: u32, fault: PalwLegsFaultV1) -> Hash64 {
    let (code, argument) = fault.evidence_words();
    let mut w = Writer::new();
    w.hash64(committed_root);
    w.u8(evidence_kind);
    w.u32(leaf_index);
    w.u8(code);
    w.u32(argument);
    w.keyed64(PALW_LEGS_DOMAIN_EVIDENCE_ID)
}

/// Recomputes the committed root from the binding. Returns the context hash and both profile
/// hashes on success. Rejection here means the evidence is not about this commitment; it never
/// means the commitment is honest.
fn verify_binding(binding: &PalwLegsBindingV1) -> Result<(Hash64, Hash64, Hash64), PalwLegsError> {
    if binding.version != PALW_LEGS_OBJECT_VERSION_V1 {
        return Err(PalwLegsError::UnsupportedVersion { got: binding.version, expected: PALW_LEGS_OBJECT_VERSION_V1 });
    }
    check_job_context_shape(&binding.job_context).map_err(PalwLegsError::Context)?;
    let context_hash = binding.job_context.context_hash();
    let tap_profile_hash = binding.tap_profile.profile_hash();
    let checkpoint_profile_hash = binding.checkpoint_profile.profile_hash();
    let decode_calls = canonical_decode_calls(&binding.job_context);
    let activation_root = activation_leg_root_v1(
        &context_hash,
        &tap_profile_hash,
        binding.job_context.declared_prefill_tokens,
        decode_calls,
        binding.activation_leaf_count,
        &binding.activation_merkle_root,
    );
    let checkpoint_root = checkpoint_leg_root_v1(
        &context_hash,
        &checkpoint_profile_hash,
        decode_calls,
        binding.checkpoint_count,
        &binding.checkpoint_merkle_root,
    );
    let recomputed =
        execution_commitment_root_v1(&context_hash, &binding.full_logits_trace_root, &activation_root, &checkpoint_root);
    if recomputed != binding.committed_execution_root {
        return Err(PalwLegsError::CommittedRootMismatch);
    }
    Ok((context_hash, tap_profile_hash, checkpoint_profile_hash))
}

/// Shape faults decidable from the binding alone, in frozen order. The committed root already
/// matched, so every violation here is the miner's own committed inconsistency.
fn scan_shape_faults(binding: &PalwLegsBindingV1, context_hash: &Hash64) -> Option<PalwLegsFaultV1> {
    if binding.tap_profile.validate_shape().is_err() {
        return Some(PalwLegsFaultV1::TapProfileNotCanonical);
    }
    if binding.checkpoint_profile.validate_shape().is_err() {
        return Some(PalwLegsFaultV1::CheckpointProfileNotCanonical);
    }
    let expected_leaves = canonical_activation_leaf_count(&binding.job_context, binding.tap_profile.tap_count());
    if binding.activation_leaf_count as u64 != expected_leaves {
        return Some(PalwLegsFaultV1::ActivationLeafCountNotCanonical);
    }
    let expected_checkpoints = canonical_checkpoint_count(&binding.job_context, binding.checkpoint_profile.checkpoint_interval);
    let empty_ok = binding.checkpoint_merkle_root == checkpoint_empty_root_v1(context_hash);
    if binding.checkpoint_count != expected_checkpoints || (binding.checkpoint_count == 0) != empty_ok {
        return Some(PalwLegsFaultV1::CheckpointCountNotCanonical);
    }
    None
}

/// Adjudicates a legs refutation. Every path is unilateral and objective; `NoFaultFound` is the
/// challenger-loses verdict.
pub fn check_legs_refutation_v1(refutation: &PalwLegsRefutationV1) -> Result<PalwLegsRefutationVerdictV1, PalwLegsError> {
    let binding = &refutation.binding;
    let (context_hash, tap_profile_hash, checkpoint_profile_hash) = verify_binding(binding)?;
    let committed = &binding.committed_execution_root;

    // Shape faults are checked FIRST for every evidence kind: an opening into a tree whose own
    // declared shape is fraudulent must not be reachable (the coordinate rules below assume a
    // canonical shape to derive against).
    let shape_fault = scan_shape_faults(binding, &context_hash);

    match &refutation.evidence {
        PalwLegsEvidenceV1::Shape => match shape_fault {
            Some(fault) => Ok(PalwLegsRefutationVerdictV1 { fault, evidence_id: evidence_id(committed, 0, 0, fault) }),
            None => Err(PalwLegsError::NoFaultFound),
        },
        PalwLegsEvidenceV1::ActivationLeaf { opening, preimage } => {
            if let Some(fault) = shape_fault {
                return Ok(PalwLegsRefutationVerdictV1 { fault, evidence_id: evidence_id(committed, 0, 0, fault) });
            }
            if preimage.values_le_bytes.len() > PALW_LEGS_MAX_ROW_BYTES {
                return Err(PalwLegsError::RowBytesTooLarge { got: preimage.values_le_bytes.len(), max: PALW_LEGS_MAX_ROW_BYTES });
            }
            let computed_root = leg_opening_root_v1(
                PALW_LEGS_DOMAIN_ACTIVATION_MERKLE_LEAF,
                PALW_LEGS_DOMAIN_ACTIVATION_MERKLE_NODE,
                binding.activation_leaf_count,
                opening,
                PALW_LEGS_MAX_ACTIVATION_LEAVES,
            )?;
            if computed_root != binding.activation_merkle_root {
                return Err(PalwLegsError::OpeningRootMismatch { tree: "activation" });
            }
            if activation_leaf_hash_v1(&context_hash, &tap_profile_hash, preimage) != opening.leaf_hash {
                return Err(PalwLegsError::LeafPreimageMismatch { leaf: "activation" });
            }
            // Fault scan, frozen order: coordinates, placement, encoding, identity, values.
            let taps = binding.tap_profile.tap_count();
            let fault = match canonical_activation_leaf_index(
                &binding.job_context,
                taps,
                preimage.call_index,
                preimage.tap_slot,
                preimage.position,
            ) {
                None => Some(PalwLegsFaultV1::ActivationCoordinatesNotCanonical),
                Some(expected_index) if expected_index != opening.leaf_index as u64 => {
                    Some(PalwLegsFaultV1::ActivationLeafIndexNotCanonical)
                }
                Some(_) => {
                    if (preimage.value_count as u64) * 4 != preimage.values_le_bytes.len() as u64 {
                        Some(PalwLegsFaultV1::ActivationBytesNotFourPerValue)
                    } else if preimage.value_count != preimage.hidden_dim {
                        Some(PalwLegsFaultV1::ActivationValueCountNotHiddenDim)
                    } else if preimage.hidden_dim != binding.tap_profile.hidden_dim {
                        Some(PalwLegsFaultV1::ActivationHiddenDimNotProfile)
                    } else {
                        preimage
                            .values_le_bytes
                            .chunks_exact(4)
                            .position(|c| !f32::from_le_bytes([c[0], c[1], c[2], c[3]]).is_finite())
                            .map(|i| PalwLegsFaultV1::ActivationNonFinite { value_index: i as u32 })
                    }
                }
            };
            match fault {
                Some(fault) => {
                    Ok(PalwLegsRefutationVerdictV1 { fault, evidence_id: evidence_id(committed, 1, opening.leaf_index, fault) })
                }
                None => Err(PalwLegsError::NoFaultFound),
            }
        }
        PalwLegsEvidenceV1::CheckpointLeaf { opening, preimage } => {
            if let Some(fault) = shape_fault {
                return Ok(PalwLegsRefutationVerdictV1 { fault, evidence_id: evidence_id(committed, 0, 0, fault) });
            }
            self::open_checkpoint(binding, &context_hash, &checkpoint_profile_hash, opening, preimage)?;
            let interval = binding.checkpoint_profile.checkpoint_interval;
            let fault = if preimage.checkpoint_index != opening.leaf_index {
                Some(PalwLegsFaultV1::CheckpointIndexNotCanonical)
            } else if preimage.covered_decode_call as u64 != (preimage.checkpoint_index as u64 + 1) * interval as u64 {
                Some(PalwLegsFaultV1::CheckpointCoveredCallNotCanonical)
            } else if preimage.state_layout_id != binding.checkpoint_profile.state_layout_id {
                Some(PalwLegsFaultV1::CheckpointStateLayoutNotProfile)
            } else if preimage.checkpoint_index == 0 && preimage.prev_checkpoint_leaf_hash != checkpoint_genesis_prev_v1(&context_hash)
            {
                Some(PalwLegsFaultV1::CheckpointGenesisPrevMismatch)
            } else {
                None
            };
            match fault {
                Some(fault) => {
                    Ok(PalwLegsRefutationVerdictV1 { fault, evidence_id: evidence_id(committed, 2, opening.leaf_index, fault) })
                }
                None => Err(PalwLegsError::NoFaultFound),
            }
        }
        PalwLegsEvidenceV1::CheckpointChain { earlier_opening, earlier_preimage, later_opening, later_preimage } => {
            if let Some(fault) = shape_fault {
                return Ok(PalwLegsRefutationVerdictV1 { fault, evidence_id: evidence_id(committed, 0, 0, fault) });
            }
            if later_opening.leaf_index != earlier_opening.leaf_index + 1 {
                return Err(PalwLegsError::ChainEvidenceNotAdjacent {
                    earlier: earlier_opening.leaf_index,
                    later: later_opening.leaf_index,
                });
            }
            self::open_checkpoint(binding, &context_hash, &checkpoint_profile_hash, earlier_opening, earlier_preimage)?;
            self::open_checkpoint(binding, &context_hash, &checkpoint_profile_hash, later_opening, later_preimage)?;
            // The chain rule: later.prev must be the earlier leaf's own hash — the opened leaf
            // hash, which the preimage already proved it matches.
            if later_preimage.prev_checkpoint_leaf_hash != earlier_opening.leaf_hash {
                let fault = PalwLegsFaultV1::CheckpointChainBroken;
                Ok(PalwLegsRefutationVerdictV1 { fault, evidence_id: evidence_id(committed, 3, later_opening.leaf_index, fault) })
            } else {
                Err(PalwLegsError::NoFaultFound)
            }
        }
    }
}

/// Shared checkpoint-evidence plumbing: opening verifies against the committed checkpoint tree
/// and the carried preimage is the opened leaf.
fn open_checkpoint(
    binding: &PalwLegsBindingV1,
    context_hash: &Hash64,
    checkpoint_profile_hash: &Hash64,
    opening: &PalwLegOpeningV1,
    preimage: &PalwCheckpointLeafV1,
) -> Result<(), PalwLegsError> {
    let computed_root = leg_opening_root_v1(
        PALW_LEGS_DOMAIN_CHECKPOINT_MERKLE_LEAF,
        PALW_LEGS_DOMAIN_CHECKPOINT_MERKLE_NODE,
        binding.checkpoint_count,
        opening,
        PALW_LEGS_MAX_CHECKPOINTS,
    )?;
    if computed_root != binding.checkpoint_merkle_root {
        return Err(PalwLegsError::OpeningRootMismatch { tree: "checkpoint" });
    }
    if checkpoint_leaf_hash_v1(context_hash, checkpoint_profile_hash, preimage) != opening.leaf_hash {
        return Err(PalwLegsError::LeafPreimageMismatch { leaf: "checkpoint" });
    }
    Ok(())
}

// =============================================================================================
// Tests
// =============================================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_slash::{trace_event_opening_root_v1, PalwTraceEventOpeningV1};
    use crate::palw_slash::PALW_S_ALL_DOMAINS;
    use crate::palw_reference::PALW_REFERENCE_ALL_DOMAINS;
    use crate::palw_v2::{
        trace_event_merkle_root_v2, trace_scheme_id_v2, PALW_V2_ALL_DOMAINS, PALW_V2_DOMAIN_TRACE_MERKLE_LEAF,
        PALW_V2_DOMAIN_TRACE_MERKLE_NODE,
    };

    fn h64(seed: u8) -> Hash64 {
        Hash64::from_bytes([seed; 64])
    }

    /// Small honest world: P = 3 prefill tokens, D = 3 events (2 decode calls), 2 taps,
    /// hidden_dim = 4, checkpoint interval 2 ⇒ 1 checkpoint covering decode call 2.
    fn test_context() -> PalwJobContextV2 {
        PalwJobContextV2 {
            version: crate::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: b"misaka-devnet".to_vec(),
            job_id: h64(0x11),
            job_nullifier: h64(0x12),
            assignment_id: h64(0x13),
            execution_seed: [0x22; 32],
            model_profile_id: h64(0x31),
            runtime_manifest_hash: h64(0x32),
            runtime_class_id: h64(0x33),
            shape_profile_id: h64(0x34),
            trace_scheme_id: trace_scheme_id_v2(),
            cu_ruleset_id: h64(0x36),
            tokenizer_id: h64(0x37),
            prompt_token_ids_hash: h64(0x38),
            declared_prefill_tokens: 3,
            exact_decode_tokens: 3,
            max_context_tokens: 64,
        }
    }

    fn tap_profile() -> PalwActivationTapProfileV1 {
        PalwActivationTapProfileV1 {
            version: PALW_LEGS_OBJECT_VERSION_V1,
            tap_semantics_id: h64(0x41),
            tap_layer_indices: vec![8, 16],
            model_total_layers: 28,
            hidden_dim: 4,
            dtype: PalwLogitsDtypeV2::F32Le,
        }
    }

    fn checkpoint_profile() -> PalwCheckpointProfileV1 {
        PalwCheckpointProfileV1 { version: PALW_LEGS_OBJECT_VERSION_V1, checkpoint_interval: 2, state_layout_id: h64(0x51) }
    }

    fn row(seed: u32) -> PalwActivationLeafV1 {
        // Filled in by the honest builder; placeholder for coordinates.
        PalwActivationLeafV1 {
            call_index: 0,
            tap_slot: 0,
            position: 0,
            hidden_dim: 4,
            value_count: 4,
            values_le_bytes: (0..4u32).flat_map(|i| ((seed as f32) + i as f32 * 0.5).to_le_bytes()).collect(),
        }
    }

    struct HonestWorld {
        context: PalwJobContextV2,
        binding: PalwLegsBindingV1,
        activation_leaves: Vec<PalwActivationLeafV1>,
        activation_hashes: Vec<Hash64>,
        checkpoint_leaves: Vec<PalwCheckpointLeafV1>,
        checkpoint_hashes: Vec<Hash64>,
    }

    fn honest_world() -> HonestWorld {
        let context = test_context();
        let context_hash = context.context_hash();
        let taps = tap_profile();
        let tap_hash = taps.profile_hash();
        let ckpt_profile = checkpoint_profile();
        let ckpt_profile_hash = ckpt_profile.profile_hash();

        // Activation leaves in canonical order: (call 0: taps × positions), then decode calls.
        let mut activation_leaves = Vec::new();
        for tap_slot in 0..taps.tap_count() {
            for position in 0..context.declared_prefill_tokens {
                let mut leaf = row(100 + tap_slot * 10 + position);
                leaf.call_index = 0;
                leaf.tap_slot = tap_slot;
                leaf.position = position;
                activation_leaves.push(leaf);
            }
        }
        for call in 1..context.exact_decode_tokens {
            for tap_slot in 0..taps.tap_count() {
                let mut leaf = row(200 + call * 10 + tap_slot);
                leaf.call_index = call;
                leaf.tap_slot = tap_slot;
                leaf.position = 0;
                activation_leaves.push(leaf);
            }
        }
        // The canonical order groups the prefill call tap-major then position, and decode calls
        // call-major then tap — exactly the index formula. Sanity: recompute and sort by it.
        activation_leaves.sort_by_key(|leaf| {
            canonical_activation_leaf_index(&context, taps.tap_count(), leaf.call_index, leaf.tap_slot, leaf.position).unwrap()
        });
        let activation_hashes: Vec<Hash64> =
            activation_leaves.iter().map(|leaf| activation_leaf_hash_v1(&context_hash, &tap_hash, leaf)).collect();
        let activation_merkle_root = leg_merkle_root_v1(
            PALW_LEGS_DOMAIN_ACTIVATION_MERKLE_LEAF,
            PALW_LEGS_DOMAIN_ACTIVATION_MERKLE_NODE,
            &activation_hashes,
            PALW_LEGS_MAX_ACTIVATION_LEAVES,
        )
        .unwrap();

        // One checkpoint: index 0, covering decode call 2.
        let ckpt0 = PalwCheckpointLeafV1 {
            checkpoint_index: 0,
            covered_decode_call: 2,
            state_layout_id: ckpt_profile.state_layout_id,
            state_root: h64(0x61),
            prev_checkpoint_leaf_hash: checkpoint_genesis_prev_v1(&context_hash),
        };
        let checkpoint_leaves = vec![ckpt0];
        let checkpoint_hashes: Vec<Hash64> =
            checkpoint_leaves.iter().map(|leaf| checkpoint_leaf_hash_v1(&context_hash, &ckpt_profile_hash, leaf)).collect();
        let checkpoint_merkle_root = leg_merkle_root_v1(
            PALW_LEGS_DOMAIN_CHECKPOINT_MERKLE_LEAF,
            PALW_LEGS_DOMAIN_CHECKPOINT_MERKLE_NODE,
            &checkpoint_hashes,
            PALW_LEGS_MAX_CHECKPOINTS,
        )
        .unwrap();

        let binding = PalwLegsBindingV1 {
            version: PALW_LEGS_OBJECT_VERSION_V1,
            job_context: context.clone(),
            tap_profile: taps,
            checkpoint_profile: ckpt_profile,
            full_logits_trace_root: h64(0x71),
            activation_leaf_count: activation_hashes.len() as u32,
            activation_merkle_root,
            checkpoint_count: 1,
            checkpoint_merkle_root,
            committed_execution_root: Hash64::from_bytes([0; 64]), // filled below
        };
        let mut world =
            HonestWorld { context, binding, activation_leaves, activation_hashes, checkpoint_leaves, checkpoint_hashes };
        world.binding.committed_execution_root = recompute_committed(&world.binding);
        world
    }

    /// Test-side recomputation of the composite from a binding (the same arithmetic
    /// `verify_binding` runs — duplicated here so a bug cannot hide in shared code).
    fn recompute_committed(binding: &PalwLegsBindingV1) -> Hash64 {
        let context_hash = binding.job_context.context_hash();
        let decode_calls = binding.job_context.exact_decode_tokens - 1;
        let activation = activation_leg_root_v1(
            &context_hash,
            &binding.tap_profile.profile_hash(),
            binding.job_context.declared_prefill_tokens,
            decode_calls,
            binding.activation_leaf_count,
            &binding.activation_merkle_root,
        );
        let checkpoint = checkpoint_leg_root_v1(
            &context_hash,
            &binding.checkpoint_profile.profile_hash(),
            decode_calls,
            binding.checkpoint_count,
            &binding.checkpoint_merkle_root,
        );
        execution_commitment_root_v1(&context_hash, &binding.full_logits_trace_root, &activation, &checkpoint)
    }

    fn build_levels(leaf_domain: &[u8], node_domain: &[u8], leaf_hashes: &[Hash64]) -> Vec<Vec<Hash64>> {
        let leaves: Vec<Hash64> =
            leaf_hashes.iter().enumerate().map(|(i, leaf)| leg_merkle_leaf(leaf_domain, i as u32, leaf)).collect();
        let mut levels = vec![leaves];
        while levels.last().unwrap().len() > 1 {
            let previous = levels.last().unwrap();
            let mut next = Vec::new();
            for pair in previous.chunks(2) {
                match pair {
                    [left, right] => next.push(keyed64(node_domain, &[left.as_byte_slice(), right.as_byte_slice()])),
                    [odd] => next.push(*odd),
                    _ => unreachable!(),
                }
            }
            levels.push(next);
        }
        levels
    }

    fn opening_for(leaf_domain: &[u8], node_domain: &[u8], leaf_hashes: &[Hash64], index: usize) -> PalwLegOpeningV1 {
        let levels = build_levels(leaf_domain, node_domain, leaf_hashes);
        let mut siblings = Vec::new();
        let mut position = index;
        for level in &levels[..levels.len() - 1] {
            let promoted = level.len() % 2 == 1 && position == level.len() - 1;
            if !promoted {
                siblings.push(level[position ^ 1]);
            }
            position /= 2;
        }
        PalwLegOpeningV1 { leaf_index: index as u32, leaf_hash: leaf_hashes[index], siblings }
    }

    fn activation_opening(world: &HonestWorld, index: usize) -> PalwLegOpeningV1 {
        opening_for(PALW_LEGS_DOMAIN_ACTIVATION_MERKLE_LEAF, PALW_LEGS_DOMAIN_ACTIVATION_MERKLE_NODE, &world.activation_hashes, index)
    }

    fn checkpoint_opening(world: &HonestWorld, index: usize) -> PalwLegOpeningV1 {
        opening_for(PALW_LEGS_DOMAIN_CHECKPOINT_MERKLE_LEAF, PALW_LEGS_DOMAIN_CHECKPOINT_MERKLE_NODE, &world.checkpoint_hashes, index)
    }

    // -----------------------------------------------------------------------------------------
    // Domains and identity
    // -----------------------------------------------------------------------------------------

    #[test]
    fn legs_domains_are_unique_across_all_palw_modules() {
        let mut seen = std::collections::HashSet::new();
        for d in PALW_LEGS_ALL_DOMAINS {
            assert!(seen.insert(*d), "duplicate legs domain: {}", String::from_utf8_lossy(d));
            assert!(d.len() <= 64, "blake2b key cap exceeded");
        }
        for d in PALW_V2_ALL_DOMAINS.iter().chain(PALW_S_ALL_DOMAINS.iter()).chain(PALW_REFERENCE_ALL_DOMAINS.iter()) {
            assert!(!seen.contains(d), "legs module reuses a foreign domain: {}", String::from_utf8_lossy(d));
        }
    }

    #[test]
    fn golden_vectors_are_frozen() {
        let world = honest_world();
        let got: Vec<(&str, String)> = vec![
            ("scheme_id", execution_commitment_scheme_id_v1().to_string()),
            ("tap_profile", tap_profile().profile_hash().to_string()),
            ("checkpoint_profile", checkpoint_profile().profile_hash().to_string()),
            ("committed_root", world.binding.committed_execution_root.to_string()),
        ];
        // Frozen 2026-08-15. A change to any of these is a preimage-layout change: new scheme
        // version, never an in-place edit.
        let expected: Vec<(&str, String)> = vec![
            ("scheme_id", "426f7278957cf5c552e5a583993735712737498515ee9c1f9b880dfcba0abdd2533e6d7782cb38e65029ea91e89e12e331d04425a3e704fe5d9711689c21b653".to_string()),
            ("tap_profile", "c48db4ab8b8af2ab4d164ef87d25c621e7084b6bb673e25a951eedf3fe1004156eff9c05afe0c8178fa66055bd72cf919fc162bc156061262023d6edf0d3e98f".to_string()),
            ("checkpoint_profile", "d5ccb637270deeb4503e656c593d029f43af02018651bd4d06f1c4bd4d751400b17f69b3e29d5371b3eae087a85ddbd00e0b5bb9fea694251a1d9751b028ade3".to_string()),
            ("committed_root", "1bdeb18789f5124cf71261fb57796c5330363aa1fb1f99a68fc923813d0a9678d7985d2c26fbe67df1b40e4c2cc09edd97da80b370d2e351545eaeb33e9eb49d".to_string()),
        ];
        assert_eq!(got, expected);
    }

    // -----------------------------------------------------------------------------------------
    // Canonical counts, coordinates, and the index bijection
    // -----------------------------------------------------------------------------------------

    #[test]
    fn leaf_index_formula_is_a_bijection() {
        for (prefill, events, taps) in [(1u32, 1u32, 1u32), (3, 3, 2), (7, 5, 3), (1, 4, 16), (5, 1, 2)] {
            let mut context = test_context();
            context.declared_prefill_tokens = prefill;
            context.exact_decode_tokens = events;
            let total = canonical_activation_leaf_count(&context, taps);
            let mut seen = vec![false; total as usize];
            // Prefill call coordinates…
            for tap in 0..taps {
                for pos in 0..prefill {
                    let idx = canonical_activation_leaf_index(&context, taps, 0, tap, pos).unwrap();
                    assert!(!seen[idx as usize], "index collision at {idx}");
                    seen[idx as usize] = true;
                }
            }
            // …then decode calls.
            for call in 1..events {
                for tap in 0..taps {
                    let idx = canonical_activation_leaf_index(&context, taps, call, tap, 0).unwrap();
                    assert!(!seen[idx as usize], "index collision at {idx}");
                    seen[idx as usize] = true;
                }
            }
            assert!(seen.into_iter().all(|b| b), "index gap for P={prefill} D={events} T={taps}");
            // Out-of-range coordinates never map.
            assert_eq!(canonical_activation_leaf_index(&context, taps, 0, taps, 0), None);
            assert_eq!(canonical_activation_leaf_index(&context, taps, 0, 0, prefill), None);
            assert_eq!(canonical_activation_leaf_index(&context, taps, events, 0, 0), None);
            if events > 1 {
                assert_eq!(canonical_activation_leaf_index(&context, taps, 1, 0, 1), None, "decode position must be 0");
            }
        }
    }

    #[test]
    fn profile_shape_rules_are_closed() {
        let good = tap_profile();
        assert!(good.validate_shape().is_ok());
        let mut bad = good.clone();
        bad.tap_layer_indices = vec![];
        assert!(bad.validate_shape().is_err());
        let mut bad = good.clone();
        bad.tap_layer_indices = vec![8, 8];
        assert!(bad.validate_shape().is_err(), "non-ascending taps must be rejected");
        let mut bad = good.clone();
        bad.tap_layer_indices = vec![8, 28];
        assert!(bad.validate_shape().is_err(), "tap at model_total_layers must be rejected");
        let mut bad = good.clone();
        bad.hidden_dim = 0;
        assert!(bad.validate_shape().is_err());
        let mut bad = good;
        bad.hidden_dim = PALW_LEGS_MAX_HIDDEN_DIM + 1;
        assert!(bad.validate_shape().is_err());
        let mut bad = checkpoint_profile();
        bad.checkpoint_interval = 0;
        assert!(bad.validate_shape().is_err());
    }

    // -----------------------------------------------------------------------------------------
    // The generalized leg Merkle is the v2 construction
    // -----------------------------------------------------------------------------------------

    #[test]
    fn leg_merkle_with_v2_domains_reproduces_the_v2_production_root_and_openings() {
        for count in 1usize..=17 {
            let events: Vec<Hash64> = (0..count).map(|i| h64(0x80 + i as u8)).collect();
            let production = trace_event_merkle_root_v2(&events).unwrap();
            let generalized =
                leg_merkle_root_v1(PALW_V2_DOMAIN_TRACE_MERKLE_LEAF, PALW_V2_DOMAIN_TRACE_MERKLE_NODE, &events, 4096).unwrap();
            assert_eq!(generalized, production, "construction diverged at count {count}");
            for index in 0..count {
                let opening = opening_for(PALW_V2_DOMAIN_TRACE_MERKLE_LEAF, PALW_V2_DOMAIN_TRACE_MERKLE_NODE, &events, index);
                // The legs verifier agrees…
                let via_legs = leg_opening_root_v1(
                    PALW_V2_DOMAIN_TRACE_MERKLE_LEAF,
                    PALW_V2_DOMAIN_TRACE_MERKLE_NODE,
                    count as u32,
                    &opening,
                    4096,
                )
                .unwrap();
                assert_eq!(via_legs, production);
                // …and so does the frozen PALW-S verifier, over the same bytes.
                let as_trace = PalwTraceEventOpeningV1 {
                    event_index: opening.leaf_index,
                    event_hash: opening.leaf_hash,
                    siblings: opening.siblings.clone(),
                };
                assert_eq!(trace_event_opening_root_v1(count as u32, &as_trace).unwrap(), production);
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Honest world survives; every fault convicts
    // -----------------------------------------------------------------------------------------

    #[test]
    fn honest_world_survives_every_refutation_shape() {
        let world = honest_world();
        let shape = PalwLegsRefutationV1 { binding: world.binding.clone(), evidence: PalwLegsEvidenceV1::Shape };
        assert_eq!(check_legs_refutation_v1(&shape), Err(PalwLegsError::NoFaultFound));
        for index in 0..world.activation_leaves.len() {
            let refutation = PalwLegsRefutationV1 {
                binding: world.binding.clone(),
                evidence: PalwLegsEvidenceV1::ActivationLeaf {
                    opening: activation_opening(&world, index),
                    preimage: world.activation_leaves[index].clone(),
                },
            };
            assert_eq!(check_legs_refutation_v1(&refutation), Err(PalwLegsError::NoFaultFound), "activation index {index}");
        }
        let refutation = PalwLegsRefutationV1 {
            binding: world.binding.clone(),
            evidence: PalwLegsEvidenceV1::CheckpointLeaf {
                opening: checkpoint_opening(&world, 0),
                preimage: world.checkpoint_leaves[0].clone(),
            },
        };
        assert_eq!(check_legs_refutation_v1(&refutation), Err(PalwLegsError::NoFaultFound));
    }

    #[test]
    fn shape_faults_convict_from_the_binding_alone() {
        // Wrong activation leaf count, committed as such.
        let mut world = honest_world();
        world.binding.activation_leaf_count += 1;
        world.binding.committed_execution_root = recompute_committed(&world.binding);
        let refutation = PalwLegsRefutationV1 { binding: world.binding.clone(), evidence: PalwLegsEvidenceV1::Shape };
        assert_eq!(check_legs_refutation_v1(&refutation).unwrap().fault, PalwLegsFaultV1::ActivationLeafCountNotCanonical);

        // Wrong checkpoint count.
        let mut world = honest_world();
        world.binding.checkpoint_count = 2;
        world.binding.committed_execution_root = recompute_committed(&world.binding);
        let refutation = PalwLegsRefutationV1 { binding: world.binding.clone(), evidence: PalwLegsEvidenceV1::Shape };
        assert_eq!(check_legs_refutation_v1(&refutation).unwrap().fault, PalwLegsFaultV1::CheckpointCountNotCanonical);

        // Empty-sentinel rule: count 0 must carry the sentinel.
        let mut world = honest_world();
        world.binding.checkpoint_profile.checkpoint_interval = 10; // canonical count becomes 0
        world.binding.checkpoint_count = 0;
        world.binding.committed_execution_root = recompute_committed(&world.binding);
        let refutation = PalwLegsRefutationV1 { binding: world.binding.clone(), evidence: PalwLegsEvidenceV1::Shape };
        assert_eq!(check_legs_refutation_v1(&refutation).unwrap().fault, PalwLegsFaultV1::CheckpointCountNotCanonical);
        // …and with the sentinel in place the same binding is honest.
        let context_hash = world.binding.job_context.context_hash();
        world.binding.checkpoint_merkle_root = checkpoint_empty_root_v1(&context_hash);
        world.binding.committed_execution_root = recompute_committed(&world.binding);
        let refutation = PalwLegsRefutationV1 { binding: world.binding.clone(), evidence: PalwLegsEvidenceV1::Shape };
        assert_eq!(check_legs_refutation_v1(&refutation), Err(PalwLegsError::NoFaultFound));

        // A malformed committed tap profile convicts even when counts line up with it.
        let mut world = honest_world();
        world.binding.tap_profile.tap_layer_indices = vec![16, 8];
        world.binding.committed_execution_root = recompute_committed(&world.binding);
        let refutation = PalwLegsRefutationV1 { binding: world.binding.clone(), evidence: PalwLegsEvidenceV1::Shape };
        assert_eq!(check_legs_refutation_v1(&refutation).unwrap().fault, PalwLegsFaultV1::TapProfileNotCanonical);
    }

    #[test]
    fn activation_leaf_faults_convict() {
        // Rebuild the world with one poisoned leaf: NaN at value 2 of activation leaf 4.
        let mut world = honest_world();
        let target = 4usize;
        world.activation_leaves[target].values_le_bytes[2 * 4..3 * 4].copy_from_slice(&f32::NAN.to_le_bytes());
        rebuild_activation(&mut world);
        let refutation = PalwLegsRefutationV1 {
            binding: world.binding.clone(),
            evidence: PalwLegsEvidenceV1::ActivationLeaf {
                opening: activation_opening(&world, target),
                preimage: world.activation_leaves[target].clone(),
            },
        };
        assert_eq!(check_legs_refutation_v1(&refutation).unwrap().fault, PalwLegsFaultV1::ActivationNonFinite { value_index: 2 });

        // Wrong-index placement: swap two leaves (coordinates stay canonical, tree order not).
        let mut world = honest_world();
        world.activation_leaves.swap(1, 2);
        rebuild_activation(&mut world);
        let refutation = PalwLegsRefutationV1 {
            binding: world.binding.clone(),
            evidence: PalwLegsEvidenceV1::ActivationLeaf {
                opening: activation_opening(&world, 1),
                preimage: world.activation_leaves[1].clone(),
            },
        };
        assert_eq!(check_legs_refutation_v1(&refutation).unwrap().fault, PalwLegsFaultV1::ActivationLeafIndexNotCanonical);

        // Non-canonical coordinates: decode-call leaf claiming position 1.
        let mut world = honest_world();
        let last = world.activation_leaves.len() - 1;
        world.activation_leaves[last].position = 1;
        rebuild_activation(&mut world);
        let refutation = PalwLegsRefutationV1 {
            binding: world.binding.clone(),
            evidence: PalwLegsEvidenceV1::ActivationLeaf {
                opening: activation_opening(&world, last),
                preimage: world.activation_leaves[last].clone(),
            },
        };
        assert_eq!(check_legs_refutation_v1(&refutation).unwrap().fault, PalwLegsFaultV1::ActivationCoordinatesNotCanonical);

        // Short row: value_count says 4, bytes carry 2 values.
        let mut world = honest_world();
        world.activation_leaves[0].values_le_bytes.truncate(8);
        rebuild_activation(&mut world);
        let refutation = PalwLegsRefutationV1 {
            binding: world.binding.clone(),
            evidence: PalwLegsEvidenceV1::ActivationLeaf {
                opening: activation_opening(&world, 0),
                preimage: world.activation_leaves[0].clone(),
            },
        };
        assert_eq!(check_legs_refutation_v1(&refutation).unwrap().fault, PalwLegsFaultV1::ActivationBytesNotFourPerValue);

        // Consistent row of the wrong width: count = 2, bytes = 8, profile dim = 4.
        let mut world = honest_world();
        world.activation_leaves[0].value_count = 2;
        world.activation_leaves[0].hidden_dim = 2;
        world.activation_leaves[0].values_le_bytes.truncate(8);
        rebuild_activation(&mut world);
        let refutation = PalwLegsRefutationV1 {
            binding: world.binding.clone(),
            evidence: PalwLegsEvidenceV1::ActivationLeaf {
                opening: activation_opening(&world, 0),
                preimage: world.activation_leaves[0].clone(),
            },
        };
        assert_eq!(check_legs_refutation_v1(&refutation).unwrap().fault, PalwLegsFaultV1::ActivationHiddenDimNotProfile);
    }

    fn rebuild_activation(world: &mut HonestWorld) {
        let context_hash = world.context.context_hash();
        let tap_hash = world.binding.tap_profile.profile_hash();
        world.activation_hashes =
            world.activation_leaves.iter().map(|leaf| activation_leaf_hash_v1(&context_hash, &tap_hash, leaf)).collect();
        world.binding.activation_merkle_root = leg_merkle_root_v1(
            PALW_LEGS_DOMAIN_ACTIVATION_MERKLE_LEAF,
            PALW_LEGS_DOMAIN_ACTIVATION_MERKLE_NODE,
            &world.activation_hashes,
            PALW_LEGS_MAX_ACTIVATION_LEAVES,
        )
        .unwrap();
        world.binding.committed_execution_root = recompute_committed(&world.binding);
    }

    fn rebuild_checkpoints(world: &mut HonestWorld) {
        let context_hash = world.context.context_hash();
        let profile_hash = world.binding.checkpoint_profile.profile_hash();
        world.checkpoint_hashes =
            world.checkpoint_leaves.iter().map(|leaf| checkpoint_leaf_hash_v1(&context_hash, &profile_hash, leaf)).collect();
        world.binding.checkpoint_count = world.checkpoint_hashes.len() as u32;
        world.binding.checkpoint_merkle_root = leg_merkle_root_v1(
            PALW_LEGS_DOMAIN_CHECKPOINT_MERKLE_LEAF,
            PALW_LEGS_DOMAIN_CHECKPOINT_MERKLE_NODE,
            &world.checkpoint_hashes,
            PALW_LEGS_MAX_CHECKPOINTS,
        )
        .unwrap();
        world.binding.committed_execution_root = recompute_committed(&world.binding);
    }

    /// A two-checkpoint world (interval 1 over 2 decode calls) for chain evidence.
    fn two_checkpoint_world(break_chain: bool) -> HonestWorld {
        let mut world = honest_world();
        world.binding.checkpoint_profile.checkpoint_interval = 1; // canonical count = 2
        let context_hash = world.context.context_hash();
        let profile_hash = world.binding.checkpoint_profile.profile_hash();
        let ckpt0 = PalwCheckpointLeafV1 {
            checkpoint_index: 0,
            covered_decode_call: 1,
            state_layout_id: world.binding.checkpoint_profile.state_layout_id,
            state_root: h64(0x62),
            prev_checkpoint_leaf_hash: checkpoint_genesis_prev_v1(&context_hash),
        };
        let prev = if break_chain { h64(0xEE) } else { checkpoint_leaf_hash_v1(&context_hash, &profile_hash, &ckpt0) };
        let ckpt1 = PalwCheckpointLeafV1 {
            checkpoint_index: 1,
            covered_decode_call: 2,
            state_layout_id: world.binding.checkpoint_profile.state_layout_id,
            state_root: h64(0x63),
            prev_checkpoint_leaf_hash: prev,
        };
        world.checkpoint_leaves = vec![ckpt0, ckpt1];
        rebuild_checkpoints(&mut world);
        world
    }

    #[test]
    fn checkpoint_faults_convict() {
        // Broken ancestry: the two-opening chain evidence convicts without any recomputation.
        let world = two_checkpoint_world(true);
        let refutation = PalwLegsRefutationV1 {
            binding: world.binding.clone(),
            evidence: PalwLegsEvidenceV1::CheckpointChain {
                earlier_opening: checkpoint_opening(&world, 0),
                earlier_preimage: world.checkpoint_leaves[0].clone(),
                later_opening: checkpoint_opening(&world, 1),
                later_preimage: world.checkpoint_leaves[1].clone(),
            },
        };
        assert_eq!(check_legs_refutation_v1(&refutation).unwrap().fault, PalwLegsFaultV1::CheckpointChainBroken);

        // An intact chain survives the same evidence.
        let world = two_checkpoint_world(false);
        let refutation = PalwLegsRefutationV1 {
            binding: world.binding.clone(),
            evidence: PalwLegsEvidenceV1::CheckpointChain {
                earlier_opening: checkpoint_opening(&world, 0),
                earlier_preimage: world.checkpoint_leaves[0].clone(),
                later_opening: checkpoint_opening(&world, 1),
                later_preimage: world.checkpoint_leaves[1].clone(),
            },
        };
        assert_eq!(check_legs_refutation_v1(&refutation), Err(PalwLegsError::NoFaultFound));

        // Genesis violation on checkpoint 0.
        let mut world = honest_world();
        world.checkpoint_leaves[0].prev_checkpoint_leaf_hash = h64(0xEE);
        rebuild_checkpoints(&mut world);
        let refutation = PalwLegsRefutationV1 {
            binding: world.binding.clone(),
            evidence: PalwLegsEvidenceV1::CheckpointLeaf {
                opening: checkpoint_opening(&world, 0),
                preimage: world.checkpoint_leaves[0].clone(),
            },
        };
        assert_eq!(check_legs_refutation_v1(&refutation).unwrap().fault, PalwLegsFaultV1::CheckpointGenesisPrevMismatch);

        // Covered-call violation.
        let mut world = honest_world();
        world.checkpoint_leaves[0].covered_decode_call = 1;
        rebuild_checkpoints(&mut world);
        let refutation = PalwLegsRefutationV1 {
            binding: world.binding.clone(),
            evidence: PalwLegsEvidenceV1::CheckpointLeaf {
                opening: checkpoint_opening(&world, 0),
                preimage: world.checkpoint_leaves[0].clone(),
            },
        };
        assert_eq!(check_legs_refutation_v1(&refutation).unwrap().fault, PalwLegsFaultV1::CheckpointCoveredCallNotCanonical);

        // Foreign state layout.
        let mut world = honest_world();
        world.checkpoint_leaves[0].state_layout_id = h64(0x99);
        rebuild_checkpoints(&mut world);
        let refutation = PalwLegsRefutationV1 {
            binding: world.binding.clone(),
            evidence: PalwLegsEvidenceV1::CheckpointLeaf {
                opening: checkpoint_opening(&world, 0),
                preimage: world.checkpoint_leaves[0].clone(),
            },
        };
        assert_eq!(check_legs_refutation_v1(&refutation).unwrap().fault, PalwLegsFaultV1::CheckpointStateLayoutNotProfile);
    }

    #[test]
    fn unaddressed_or_mismatched_evidence_is_rejected() {
        let world = honest_world();

        // Foreign committed root.
        let mut binding = world.binding.clone();
        binding.committed_execution_root = h64(0xAA);
        let refutation = PalwLegsRefutationV1 { binding, evidence: PalwLegsEvidenceV1::Shape };
        assert_eq!(check_legs_refutation_v1(&refutation), Err(PalwLegsError::CommittedRootMismatch));

        // Tampered opening.
        let mut opening = activation_opening(&world, 0);
        opening.siblings[0] = h64(0xEE);
        let refutation = PalwLegsRefutationV1 {
            binding: world.binding.clone(),
            evidence: PalwLegsEvidenceV1::ActivationLeaf { opening, preimage: world.activation_leaves[0].clone() },
        };
        assert_eq!(check_legs_refutation_v1(&refutation), Err(PalwLegsError::OpeningRootMismatch { tree: "activation" }));

        // Preimage that is not the opened leaf.
        let mut preimage = world.activation_leaves[0].clone();
        preimage.position = 1; // still canonical coordinates, different leaf
        let refutation = PalwLegsRefutationV1 {
            binding: world.binding.clone(),
            evidence: PalwLegsEvidenceV1::ActivationLeaf { opening: activation_opening(&world, 0), preimage },
        };
        assert_eq!(check_legs_refutation_v1(&refutation), Err(PalwLegsError::LeafPreimageMismatch { leaf: "activation" }));

        // Non-adjacent chain evidence proves nothing about ancestry.
        let two = two_checkpoint_world(false);
        let refutation = PalwLegsRefutationV1 {
            binding: two.binding.clone(),
            evidence: PalwLegsEvidenceV1::CheckpointChain {
                earlier_opening: checkpoint_opening(&two, 1),
                earlier_preimage: two.checkpoint_leaves[1].clone(),
                later_opening: checkpoint_opening(&two, 1),
                later_preimage: two.checkpoint_leaves[1].clone(),
            },
        };
        assert_eq!(check_legs_refutation_v1(&refutation), Err(PalwLegsError::ChainEvidenceNotAdjacent { earlier: 1, later: 1 }));

        // Oversized activation row dies at shape level.
        let mut preimage = world.activation_leaves[0].clone();
        preimage.values_le_bytes = vec![0u8; PALW_LEGS_MAX_ROW_BYTES + 1];
        let refutation = PalwLegsRefutationV1 {
            binding: world.binding.clone(),
            evidence: PalwLegsEvidenceV1::ActivationLeaf { opening: activation_opening(&world, 0), preimage },
        };
        assert!(matches!(check_legs_refutation_v1(&refutation), Err(PalwLegsError::RowBytesTooLarge { .. })));
    }

    // -----------------------------------------------------------------------------------------
    // Composite binding and wire stability
    // -----------------------------------------------------------------------------------------

    #[test]
    fn composite_root_binds_every_component() {
        let world = honest_world();
        let base = world.binding.committed_execution_root;
        let variants: Vec<PalwLegsBindingV1> = vec![
            {
                let mut b = world.binding.clone();
                b.full_logits_trace_root = h64(0x99);
                b
            },
            {
                let mut b = world.binding.clone();
                b.activation_merkle_root = h64(0x99);
                b
            },
            {
                let mut b = world.binding.clone();
                b.checkpoint_merkle_root = h64(0x99);
                b
            },
            {
                let mut b = world.binding.clone();
                b.tap_profile.hidden_dim = 8;
                b
            },
            {
                let mut b = world.binding.clone();
                b.checkpoint_profile.checkpoint_interval = 3;
                b
            },
            {
                let mut b = world.binding.clone();
                b.job_context.job_id = h64(0x99);
                b
            },
        ];
        for (i, variant) in variants.iter().enumerate() {
            assert_ne!(recompute_committed(variant), base, "component {i} does not bind");
        }
    }

    #[test]
    fn borsh_roundtrips_and_fault_discriminants_are_frozen() {
        let world = two_checkpoint_world(true);
        let refutation = PalwLegsRefutationV1 {
            binding: world.binding.clone(),
            evidence: PalwLegsEvidenceV1::CheckpointChain {
                earlier_opening: checkpoint_opening(&world, 0),
                earlier_preimage: world.checkpoint_leaves[0].clone(),
                later_opening: checkpoint_opening(&world, 1),
                later_preimage: world.checkpoint_leaves[1].clone(),
            },
        };
        let bytes = borsh::to_vec(&refutation).unwrap();
        assert_eq!(PalwLegsRefutationV1::try_from_slice(&bytes).unwrap(), refutation);

        let frozen: Vec<(PalwLegsFaultV1, u8)> = vec![
            (PalwLegsFaultV1::TapProfileNotCanonical, 0),
            (PalwLegsFaultV1::CheckpointProfileNotCanonical, 1),
            (PalwLegsFaultV1::ActivationLeafCountNotCanonical, 2),
            (PalwLegsFaultV1::CheckpointCountNotCanonical, 3),
            (PalwLegsFaultV1::ActivationCoordinatesNotCanonical, 4),
            (PalwLegsFaultV1::ActivationLeafIndexNotCanonical, 5),
            (PalwLegsFaultV1::ActivationBytesNotFourPerValue, 6),
            (PalwLegsFaultV1::ActivationValueCountNotHiddenDim, 7),
            (PalwLegsFaultV1::ActivationHiddenDimNotProfile, 8),
            (PalwLegsFaultV1::ActivationNonFinite { value_index: 0x0102_0304 }, 9),
            (PalwLegsFaultV1::CheckpointIndexNotCanonical, 10),
            (PalwLegsFaultV1::CheckpointCoveredCallNotCanonical, 11),
            (PalwLegsFaultV1::CheckpointStateLayoutNotProfile, 12),
            (PalwLegsFaultV1::CheckpointGenesisPrevMismatch, 13),
            (PalwLegsFaultV1::CheckpointChainBroken, 14),
        ];
        for (fault, tag) in frozen {
            let bytes = borsh::to_vec(&fault).unwrap();
            assert_eq!(bytes[0], tag, "discriminant moved for {fault:?}");
            assert_eq!(PalwLegsFaultV1::try_from_slice(&bytes).unwrap(), fault);
        }

        // Evidence kind discriminants are wire too.
        let shape = borsh::to_vec(&PalwLegsEvidenceV1::Shape).unwrap();
        assert_eq!(shape[0], 0);
    }

    #[test]
    fn evidence_ids_separate_by_kind_index_and_fault() {
        let root = h64(0x01);
        let mut ids = vec![
            evidence_id(&root, 0, 0, PalwLegsFaultV1::TapProfileNotCanonical),
            evidence_id(&root, 1, 0, PalwLegsFaultV1::ActivationNonFinite { value_index: 1 }),
            evidence_id(&root, 1, 0, PalwLegsFaultV1::ActivationNonFinite { value_index: 2 }),
            evidence_id(&root, 1, 1, PalwLegsFaultV1::ActivationNonFinite { value_index: 1 }),
            evidence_id(&root, 2, 0, PalwLegsFaultV1::CheckpointGenesisPrevMismatch),
            evidence_id(&root, 3, 1, PalwLegsFaultV1::CheckpointChainBroken),
            evidence_id(&h64(0x02), 3, 1, PalwLegsFaultV1::CheckpointChainBroken),
        ];
        ids.sort_by(|a, b| a.as_byte_slice().cmp(b.as_byte_slice()));
        ids.dedup();
        assert_eq!(ids.len(), 7, "evidence ids collided");
    }
}
