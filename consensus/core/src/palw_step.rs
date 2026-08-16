//! PALW step function v1 — the coordinate system of ADR-0030.
//!
//! Normative sources: ADR-0030 (all of it), ADR-0027 §1/§2 (the one-step refutation this
//! gives coordinates to), and the kernel maps of 2026-08-16 the ADR's Facts record.
//!
//! # What this module is — and is not
//!
//! This is the **step space**: the frozen operator taxonomy (`PalwStepOpKindV1`), the shape
//! profile v3 schema and its identity (`shape_profile_id_v3`), and the bijection between a
//! step index and its coordinates `(call, node, position, tile)`. It is what lets a
//! refutation *name* a step and every observer derive the same meaning for the name.
//!
//! It is deliberately **not** the commitment (the step leg is the execution-commitment v2
//! increment), **not** the kernel programs (`kernel_semantics_id` names a frozen program;
//! the catalog is its own increment, validated against the pinned kernels per class before
//! any id may appear in a registered profile), and **not** consensus-wired.
//!
//! # Pinned here vs registration-measured
//!
//! Pinned: the schema, its validation rules, the id derivations, the enumeration order and
//! both directions of the bijection. Registration-measured, per class (the tap-profile
//! discipline): every value a `PalwShapeProfileV3` carries — geometry, execution flags, node
//! tables, kernel ids, transcendental bindings, contraction facts. A profile in this module's
//! tests is a schema exercise, never a network fact.

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::Hash64;
use thiserror::Error;

use crate::palw_v2::PalwJobContextV2;

// ---------------------------------------------------------------------------------------------
// Versions, domains, caps
// ---------------------------------------------------------------------------------------------

/// Wire version of every step-v1 object in this module.
pub const PALW_STEP_OBJECT_VERSION_V1: u16 = 1;

/// The shape-profile v3 identity domain. v2's shape-string domain stays frozen; deployed
/// contexts keep meaning what they meant. Which id a class's jobs carry is a registration
/// fact (ADR-0030 §2).
pub const PALW_STEP_DOMAIN_SHAPE_PROFILE_V3: &[u8] = b"misaka-palw/shape-profile/v3";
/// The registration preimage of a `kernel_semantics_id`: a canonical descriptor string naming
/// one frozen reduction-order program (ADR-0030 premise: order is code named by id, never
/// interpreted data).
pub const PALW_STEP_DOMAIN_KERNEL_SEMANTICS: &[u8] = b"misaka-palw/kernel-semantics-id/v1";
/// The registration preimage of a transcendental algorithm id (ADR-0031's namespace anchor).
pub const PALW_STEP_DOMAIN_TRANSCENDENTAL: &[u8] = b"misaka-palw/transcendental-algorithm-id/v1";
/// The registration preimage of the checkpoint state chunk map (ADR-0030 §3).
pub const PALW_STEP_DOMAIN_STATE_CHUNK_MAP: &[u8] = b"misaka-palw/state-chunk-map-id/v1";

/// Every domain this module introduces (uniqueness-tested against every other PALW family).
pub const PALW_STEP_ALL_DOMAINS: &[&[u8]] = &[
    PALW_STEP_DOMAIN_SHAPE_PROFILE_V3,
    PALW_STEP_DOMAIN_KERNEL_SEMANTICS,
    PALW_STEP_DOMAIN_TRANSCENDENTAL,
    PALW_STEP_DOMAIN_STATE_CHUNK_MAP,
];

/// Cap on step-leg leaves per job (ADR-0030 §3 sizing: the pinned geometry at the credited
/// ceiling is ≈ 3.26 M — inside with headroom, tight against adversarial allocation).
pub const PALW_STEP_MAX_LEAVES: u64 = 1 << 22;
/// Most nodes one layer template may declare.
pub const PALW_STEP_MAX_NODES_PER_TABLE: usize = 64;
/// Tile length bounds (elements per committed tile).
pub const PALW_STEP_MIN_TILE_LEN: u32 = 16;
pub const PALW_STEP_MAX_TILE_LEN: u32 = 65_536;
/// Bounds on the KV aux-chunk width (calls per chunk leaf); 0 disables the aux series.
pub const PALW_STEP_MAX_KV_CHUNK_CALLS: u32 = 4096;

/// `kernel_semantics_id`: the identity of ONE frozen reduction-order program. The descriptor
/// string is the registration-time claim (kernel family, arch path, lane structure, source
/// tree commit); the program itself is code in the catalog, golden-tested against the pinned
/// kernel before the id may be registered.
pub fn kernel_semantics_id_v1(descriptor: &str) -> Hash64 {
    keyed64(PALW_STEP_DOMAIN_KERNEL_SEMANTICS, &[descriptor.as_bytes()])
}

/// `transcendental_algorithm_id`: same discipline for exp/log/sin/cos provenance —
/// `source-polynomial/…` for programs transcribed from the pinned tree, `libm/…` for the
/// class's libm algorithms (ADR-0030 Fact 15).
pub fn transcendental_algorithm_id_v1(descriptor: &str) -> Hash64 {
    keyed64(PALW_STEP_DOMAIN_TRANSCENDENTAL, &[descriptor.as_bytes()])
}

/// `state_chunk_map_id`: the identity of the measured checkpoint state chunk geometry
/// (per-(layer, head) slices; ADR-0030 §3). Opaque until a runtime demonstrates the layout.
pub fn state_chunk_map_id_v1(map_string: &str) -> Hash64 {
    keyed64(PALW_STEP_DOMAIN_STATE_CHUNK_MAP, &[map_string.as_bytes()])
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

// ---------------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------------

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwStepError {
    #[error("unsupported palw-step object version {got} (expected {expected})")]
    UnsupportedVersion { got: u16, expected: u16 },
    #[error("shape profile is not canonical: {0}")]
    ProfileNotCanonical(&'static str),
    #[error("job shape yields {got} step leaves, exceeding the {max} cap")]
    TooManyLeaves { got: u64, max: u64 },
    #[error("step leaf index {index} is not below the leaf count {count}")]
    LeafIndexOutOfRange { index: u64, count: u64 },
    #[error("step coordinates are not canonical for this (profile, context)")]
    CoordinatesNotCanonical,
}

// ---------------------------------------------------------------------------------------------
// The frozen operator taxonomy
// ---------------------------------------------------------------------------------------------

/// The closed set of operator shapes the pinned graph can contain (ADR-0030 §1). A graph
/// needing a kind outside this set is a new profile version, not a stretched meaning.
/// Discriminants are wire-frozen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwStepOpKindV1 {
    /// Token-id row lookup + dequantization of the (quantized) embedding table.
    EmbedLookup = 0,
    /// RMS norm, including the one CPU fusion (× weight) — `1/sqrtf(mean + eps)`.
    RmsNorm = 1,
    /// Quantized-weight matmul (repack gemv/gemm or classic vec_dot; which one is part of the
    /// node's `kernel_semantics_id`, per ADR-0030 Fact 14).
    MatMulQuant = 2,
    /// F16-operand matmul over cache material (attention scores / V mix).
    MatMulF16 = 3,
    /// Interleaved multimodal RoPE (partial rotation; sections; libm sinf/cosf).
    RopeImrope = 4,
    /// Fused softmax row: scale + mask + max + exp + double-sum + reciprocal-multiply.
    SoftMax = 5,
    /// `1/(1+expf(−x))`, scalar libm.
    Sigmoid = 6,
    /// `x > 20 → x`, else `logf(1+expf(x))`, scalar libm.
    Softplus = 7,
    /// The 4-tap causal conv over concatenated qkv channels.
    SsmConv = 8,
    /// Standalone SiLU (the conv activation path).
    Silu = 9,
    /// Fused SwiGLU GLU node (`silu(gate) · up`) — Fact: the FFN emits ONE node, not two.
    Glu = 10,
    /// The fused gated-delta-net recurrence (state update + output), ADR-0030 Fact 12.
    GatedDeltaNet = 11,
    /// `1/max(sqrtf(sum), eps)` — a different composition than RmsNorm (Fact 15).
    L2Norm = 12,
    /// Elementwise multiply (with the pinned broadcast rules).
    MulElem = 13,
    /// Elementwise add (residuals).
    AddElem = 14,
    /// Standalone scale node (none in the pinned graph — kq_scale lives inside SoftMax — but
    /// the kind exists so a future graph that emits one is expressible).
    Scale = 15,
    /// f32 → f16 cache write (SET_ROWS; the software RNE bit-twiddle = ruleset v2 semantics).
    CpyF32F16 = 16,
}

/// What role a node's output plays for the auxiliary series (ADR-0030 §3): K/V cache writes
/// feed the KV-chunk aux leaves that make full-context reductions openable in ~10 chunks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwStepNodeRoleV1 {
    Plain = 0,
    KCacheWrite = 1,
    VCacheWrite = 2,
}

/// How a node's output length is derived. `KvScaled` lengths use the TRUE kv length of the
/// position (`prefill position p` sees `p+1`; decode call `c` sees `P+c`) — never the padded
/// cache length (ADR-0030 Fact 17: padded rows must not be committed).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwStepOutLenV1 {
    /// A fixed element count per position.
    Fixed { elements: u32 } = 0,
    /// `multiplier × kv_len(position)` elements (attention scores/softmax rows).
    KvScaled { multiplier: u32 } = 1,
}

/// One operator invocation slot in a layer template (or the pre/post graph).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwStepNodeV1 {
    pub op_kind: PalwStepOpKindV1,
    pub role: PalwStepNodeRoleV1,
    /// The GGUF tensor name this node consumes as its weight operand (empty = none). Binds
    /// ADR-0030 Fact 4's per-layer dtype variance: `{layer}` in the name is substituted with
    /// the layer index at interpretation time.
    pub weight_name: String,
    /// GGML dtype byte of the weight operand (0 when `weight_name` is empty). Registration
    /// checks it against the pinned GGUF.
    pub weight_dtype: u8,
    pub out_len: PalwStepOutLenV1,
    /// Elements per committed tile (last tile ragged). Bounds: [MIN, MAX]_TILE_LEN.
    pub tile_len: u32,
    /// The frozen reduction-order program adjudicating this node's steps.
    pub kernel_semantics_id: Hash64,
}

/// Layer kinds of the hybrid graph. Which kind a layer is follows from
/// `full_attention_interval` (Fact 1): layer `i` is `Attention` iff `(i+1) % interval == 0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwLayerKindV1 {
    GatedDeltaNet = 0,
    Attention = 1,
}

/// A transcendental call site in the pinned graph, bound to the algorithm that computes it
/// (ADR-0030 Fact 15 / ADR-0031). Discriminants wire-frozen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwTranscendentalSiteV1 {
    /// The vectorized exp polynomial (softmax body, silu body, swiglu body).
    VectorExpPolynomial = 0,
    /// libm `expf` (vector-op tails, sigmoid, softplus, the GDN decay).
    LibmExpf = 1,
    /// libm `logf` (softplus).
    LibmLogf = 2,
    /// libm `sinf` (RoPE).
    LibmSinf = 3,
    /// libm `cosf` (RoPE).
    LibmCosf = 4,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTranscendentalBindingV1 {
    pub site: PalwTranscendentalSiteV1,
    pub algorithm_id: Hash64,
}

/// A named scalar site whose FMA-contraction state is a measured per-class fact
/// (ADR-0030 Fact 9: source cannot resolve it; only the shipped binary's disassembly can).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwContractionSiteV1 {
    /// RoPE's `x0·cos − x1·sin` / `x0·sin + x1·cos` pair.
    RopeRotate = 0,
    /// The classic q6_K trailing `d_all · yd · (isum − 32·mins)` chain (the LM head path).
    Q6KTail = 1,
    /// The ssm_conv 4-tap `sumf += s·c` chain.
    SsmConvTaps = 2,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwContractionFactV1 {
    pub site: PalwContractionSiteV1,
    /// 1 = the binary fuses (fmla/vfmadd), 0 = it does not. Measured, never assumed.
    pub contracted: u8,
}

// ---------------------------------------------------------------------------------------------
// Shape profile v3
// ---------------------------------------------------------------------------------------------

/// Everything `shape_profile_id` binds under ADR-0030 §2. Floats are carried as raw bit
/// patterns — a float never appears in a preimage.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwShapeProfileV3 {
    /// = [`PALW_STEP_OBJECT_VERSION_V1`].
    pub version: u16,

    // --- model geometry (restated from the pinned GGUF so the profile is self-contained) ---
    pub layer_count: u16,
    /// Layer `i` is Attention iff `(i+1) % full_attention_interval == 0`. 0 = no attention
    /// layers (a pure-recurrent graph); 1 = every layer.
    pub full_attention_interval: u16,
    pub hidden_dim: u32,
    pub ffn_dim: u32,
    pub attn_heads: u16,
    pub attn_kv_heads: u16,
    pub attn_head_dim: u32,
    pub rope_dims: u16,
    pub rope_sections: [u16; 4],
    /// f32 bit pattern of the rope frequency base.
    pub rope_freq_base_bits: u32,
    /// f32 bit patterns of the two norm epsilons.
    pub rms_eps_bits: u32,
    pub l2_eps_bits: u32,
    pub gdn_heads: u16,
    pub gdn_head_k_dim: u32,
    pub gdn_head_v_dim: u32,
    pub gdn_conv_kernel: u16,
    pub vocab_size: u32,

    // --- execution-shape facts (the previously under-pinned flags, ADR-0030 Fact 8) ---
    pub repack_on: u8,
    pub llamafile_on: u8,
    /// MUST be 1: the flash path reintroduces a cross-thread float reduction (Fact 7).
    pub flash_attn_disabled: u8,
    pub fused_gdn_on: u8,
    pub use_ref_off: u8,
    /// 1 = f16 KV cache.
    pub kv_cache_f16: u8,
    pub n_ctx: u32,
    pub n_batch: u32,
    pub n_ubatch: u32,
    pub n_seq: u32,
    pub n_threads: u32,

    // --- the node tables (ADR-0030 §1) ---
    pub pre_nodes: Vec<PalwStepNodeV1>,
    pub gdn_nodes: Vec<PalwStepNodeV1>,
    pub attn_nodes: Vec<PalwStepNodeV1>,
    pub post_nodes: Vec<PalwStepNodeV1>,

    // --- adjudication bindings ---
    /// The arithmetic the kernel programs are written in (ruleset v2 or later).
    pub reference_ruleset_id: Hash64,
    pub transcendental_bindings: Vec<PalwTranscendentalBindingV1>,
    pub contraction_facts: Vec<PalwContractionFactV1>,

    // --- aux series (ADR-0030 §3) ---
    /// Calls per KV aux-chunk leaf; 0 disables the series (then full-context reductions open
    /// per-call leaves via chunked carriage instead).
    pub kv_chunk_calls: u32,
    /// The measured checkpoint state chunk geometry (opaque until registration).
    pub state_chunk_map_id: Hash64,
}

impl PalwShapeProfileV3 {
    pub fn validate_shape(&self) -> Result<(), PalwStepError> {
        use PalwStepError::ProfileNotCanonical as bad;
        if self.version != PALW_STEP_OBJECT_VERSION_V1 {
            return Err(PalwStepError::UnsupportedVersion { got: self.version, expected: PALW_STEP_OBJECT_VERSION_V1 });
        }
        if self.layer_count == 0 {
            return Err(bad("layer count is zero"));
        }
        if self.hidden_dim == 0 || self.vocab_size == 0 {
            return Err(bad("zero geometry dimension"));
        }
        if self.flash_attn_disabled != 1 {
            return Err(bad("flash attention must be pinned disabled (ADR-0030 Fact 7)"));
        }
        if self.n_threads == 0 {
            return Err(bad("thread count is zero"));
        }
        let has_attention = self.attention_layer_exists();
        if has_attention && self.attn_nodes.is_empty() {
            return Err(bad("attention layers exist but the attention node table is empty"));
        }
        if !self.gdn_layer_exists() && !self.gdn_nodes.is_empty() {
            return Err(bad("no GDN layers but a GDN node table is declared"));
        }
        if self.gdn_layer_exists() && self.gdn_nodes.is_empty() {
            return Err(bad("GDN layers exist but the GDN node table is empty"));
        }
        if self.post_nodes.is_empty() {
            return Err(bad("post table must at least produce logits"));
        }
        for (name, table) in
            [("pre", &self.pre_nodes), ("gdn", &self.gdn_nodes), ("attn", &self.attn_nodes), ("post", &self.post_nodes)]
        {
            if table.len() > PALW_STEP_MAX_NODES_PER_TABLE {
                let _ = name;
                return Err(bad("node table exceeds the per-table cap"));
            }
            for node in table.iter() {
                if node.tile_len < PALW_STEP_MIN_TILE_LEN || node.tile_len > PALW_STEP_MAX_TILE_LEN {
                    return Err(bad("node tile length is out of bounds"));
                }
                match node.out_len {
                    PalwStepOutLenV1::Fixed { elements } => {
                        if elements == 0 {
                            return Err(bad("node output length is zero"));
                        }
                    }
                    PalwStepOutLenV1::KvScaled { multiplier } => {
                        if multiplier == 0 {
                            return Err(bad("kv-scaled multiplier is zero"));
                        }
                        if !has_attention {
                            return Err(bad("kv-scaled node in a graph with no attention layers"));
                        }
                    }
                }
                if node.weight_name.is_empty() != (node.weight_dtype == 0) {
                    return Err(bad("weight name and dtype must be both present or both absent"));
                }
                if node.weight_name.len() > 128 {
                    return Err(bad("weight name exceeds the cap"));
                }
            }
        }
        if self.kv_chunk_calls > PALW_STEP_MAX_KV_CHUNK_CALLS {
            return Err(bad("kv chunk width exceeds the cap"));
        }
        if self.kv_chunk_calls != 0 && !has_attention {
            return Err(bad("kv aux series declared without attention layers"));
        }
        Ok(())
    }

    pub fn attention_layer_exists(&self) -> bool {
        self.full_attention_interval != 0 && (self.layer_count as u32) >= self.full_attention_interval as u32
    }

    pub fn gdn_layer_exists(&self) -> bool {
        self.full_attention_interval != 1 || self.layer_count == 0
    }

    /// Layer kind under the pinned rule (Fact 1). `layer` must be `< layer_count`.
    pub fn layer_kind(&self, layer: u16) -> PalwLayerKindV1 {
        if self.full_attention_interval != 0 && (layer as u32 + 1).is_multiple_of(self.full_attention_interval as u32) {
            PalwLayerKindV1::Attention
        } else {
            PalwLayerKindV1::GatedDeltaNet
        }
    }

    fn layer_table(&self, layer: u16) -> &[PalwStepNodeV1] {
        match self.layer_kind(layer) {
            PalwLayerKindV1::GatedDeltaNet => &self.gdn_nodes,
            PalwLayerKindV1::Attention => &self.attn_nodes,
        }
    }

    /// Global node slots: pre ‖ layer 0 ‖ layer 1 ‖ … ‖ post, each layer expanding to its
    /// kind's table. The slot count is a pure profile fact.
    pub fn global_node_count(&self) -> u32 {
        let mut n = self.pre_nodes.len() as u32 + self.post_nodes.len() as u32;
        for layer in 0..self.layer_count {
            n += self.layer_table(layer).len() as u32;
        }
        n
    }

    /// Resolves a global node slot to `(its node, the layer it belongs to)` (`None` layer for
    /// pre/post nodes).
    pub fn resolve_node_slot(&self, slot: u32) -> Option<(&PalwStepNodeV1, Option<u16>)> {
        let mut cursor = slot;
        if (cursor as usize) < self.pre_nodes.len() {
            return Some((&self.pre_nodes[cursor as usize], None));
        }
        cursor -= self.pre_nodes.len() as u32;
        for layer in 0..self.layer_count {
            let table = self.layer_table(layer);
            if (cursor as usize) < table.len() {
                return Some((&table[cursor as usize], Some(layer)));
            }
            cursor -= table.len() as u32;
        }
        self.post_nodes.get(cursor as usize).map(|n| (n, None))
    }

    /// `shape_profile_id` (v3): the canonical Borsh bytes under the v3 domain. Borsh is the
    /// wire encoding of every field above (enums carry their frozen discriminants), so the
    /// preimage and the wire object cannot drift apart.
    pub fn shape_profile_id(&self) -> Hash64 {
        let bytes = borsh::to_vec(self).expect("borsh serialization of an owned struct cannot fail");
        keyed64(PALW_STEP_DOMAIN_SHAPE_PROFILE_V3, &[&bytes])
    }
}

// ---------------------------------------------------------------------------------------------
// The step space — enumeration and bijection
// ---------------------------------------------------------------------------------------------

/// Coordinates of one step (ADR-0030 §1). `call_index` 0 is the prefill call (positions
/// `0..P`); calls `1..D` are decode calls (single position). The KV length of `(call 0, p)`
/// is `p+1`; of decode call `c` it is `P+c`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct PalwStepCoordinateV1 {
    pub call_index: u32,
    pub node_slot: u32,
    pub position: u32,
    pub tile_index: u32,
}

fn tiles_for(len: u64, tile_len: u32) -> u64 {
    len.div_ceil(tile_len as u64)
}

fn node_out_len(node: &PalwStepNodeV1, kv_len: u64) -> u64 {
    match node.out_len {
        PalwStepOutLenV1::Fixed { elements } => elements as u64,
        PalwStepOutLenV1::KvScaled { multiplier } => multiplier as u64 * kv_len,
    }
}

/// Leaves contributed by one (call, position) pair across all global node slots.
fn leaves_per_position(profile: &PalwShapeProfileV3, kv_len: u64, with_logits: bool) -> u64 {
    let mut leaves = 0u64;
    for node in &profile.pre_nodes {
        leaves += tiles_for(node_out_len(node, kv_len), node.tile_len);
    }
    for layer in 0..profile.layer_count {
        for node in profile.layer_table(layer) {
            leaves += tiles_for(node_out_len(node, kv_len), node.tile_len);
        }
    }
    if with_logits {
        for node in &profile.post_nodes {
            leaves += tiles_for(node_out_len(node, kv_len), node.tile_len);
        }
    }
    leaves
}

/// KV aux-chunk leaves for the whole job (ADR-0030 §3): per attention layer, per kv head,
/// per {K, V}, `ceil(total_positions / kv_chunk_calls)` leaves.
pub fn kv_aux_leaf_count(profile: &PalwShapeProfileV3, context: &PalwJobContextV2) -> u64 {
    if profile.kv_chunk_calls == 0 {
        return 0;
    }
    let attn_layers = (0..profile.layer_count).filter(|&l| profile.layer_kind(l) == PalwLayerKindV1::Attention).count() as u64;
    let positions = context.declared_prefill_tokens as u64 + context.exact_decode_tokens.saturating_sub(1) as u64;
    attn_layers * profile.attn_kv_heads as u64 * 2 * positions.div_ceil(profile.kv_chunk_calls as u64)
}

/// Total step-leg leaves for `(profile, context)`: the main enumeration then the aux series.
/// Errors when the job shape exceeds the cap.
pub fn step_leaf_count(profile: &PalwShapeProfileV3, context: &PalwJobContextV2) -> Result<u64, PalwStepError> {
    profile.validate_shape()?;
    let prefill = context.declared_prefill_tokens as u64;
    let decode_calls = context.exact_decode_tokens.saturating_sub(1) as u64;
    let mut total = 0u64;
    // Prefill call: per position p, kv_len = p+1; logits only at the last position.
    for p in 0..prefill {
        total += leaves_per_position(profile, p + 1, p + 1 == prefill);
    }
    // Decode calls c = 1..=decode_calls: kv_len = prefill + c, logits always.
    for c in 1..=decode_calls {
        total += leaves_per_position(profile, prefill + c, true);
    }
    total += kv_aux_leaf_count(profile, context);
    if total > PALW_STEP_MAX_LEAVES {
        return Err(PalwStepError::TooManyLeaves { got: total, max: PALW_STEP_MAX_LEAVES });
    }
    Ok(total)
}

/// The pinned enumeration: main leaves ordered call-major → position → global node slot →
/// tile; the KV aux series appended after all main leaves, ordered (attention layer, kv head,
/// K then V, chunk). Returns the coordinates of a main leaf, or `None` for aux leaves (they
/// have their own coordinate space) and out-of-range indices.
pub fn canonical_step_coordinates(
    profile: &PalwShapeProfileV3,
    context: &PalwJobContextV2,
    leaf_index: u64,
) -> Option<PalwStepCoordinateV1> {
    let prefill = context.declared_prefill_tokens as u64;
    let decode_calls = context.exact_decode_tokens.saturating_sub(1) as u64;
    let mut cursor = leaf_index;
    for call in 0..=decode_calls {
        let positions = if call == 0 { prefill } else { 1 };
        for p in 0..positions {
            let kv_len = if call == 0 { p + 1 } else { prefill + call };
            let with_logits = if call == 0 { p + 1 == prefill } else { true };
            let here = leaves_per_position(profile, kv_len, with_logits);
            if cursor >= here {
                cursor -= here;
                continue;
            }
            // Inside this position: walk global slots.
            let slot_count = profile.global_node_count();
            for slot in 0..slot_count {
                let (node, _layer) = profile.resolve_node_slot(slot).expect("slot < count");
                let is_post = slot >= slot_count - profile.post_nodes.len() as u32;
                if is_post && !with_logits {
                    continue;
                }
                let tiles = tiles_for(node_out_len(node, kv_len), node.tile_len);
                if cursor < tiles {
                    return Some(PalwStepCoordinateV1 {
                        call_index: call as u32,
                        node_slot: slot,
                        position: p as u32,
                        tile_index: cursor as u32,
                    });
                }
                cursor -= tiles;
            }
            unreachable!("leaves_per_position and the slot walk disagree");
        }
    }
    None // aux territory or out of range
}

/// The inverse: rank of canonical coordinates in the pinned enumeration. `None` when the
/// coordinates are not canonical for `(profile, context)` — which, on a committed leaf, is
/// itself the fault (the legs-v1 discipline).
pub fn canonical_step_leaf_index(
    profile: &PalwShapeProfileV3,
    context: &PalwJobContextV2,
    coord: &PalwStepCoordinateV1,
) -> Option<u64> {
    let prefill = context.declared_prefill_tokens as u64;
    let decode_calls = context.exact_decode_tokens.saturating_sub(1) as u64;
    let call = coord.call_index as u64;
    if call > decode_calls {
        return None;
    }
    let positions = if call == 0 { prefill } else { 1 };
    if (coord.position as u64) >= positions {
        return None;
    }
    let mut index = 0u64;
    // Whole calls before this one.
    for c in 0..call {
        let ps = if c == 0 { prefill } else { 1 };
        for p in 0..ps {
            let kv_len = if c == 0 { p + 1 } else { prefill + c };
            index += leaves_per_position(profile, kv_len, if c == 0 { p + 1 == prefill } else { true });
        }
    }
    // Whole positions before this one within the call (prefill only).
    for p in 0..coord.position as u64 {
        index += leaves_per_position(profile, p + 1, p + 1 == prefill);
    }
    let kv_len = if call == 0 { coord.position as u64 + 1 } else { prefill + call };
    let with_logits = if call == 0 { coord.position as u64 + 1 == prefill } else { true };
    // Slots before this one within the position.
    let slot_count = profile.global_node_count();
    if coord.node_slot >= slot_count {
        return None;
    }
    for slot in 0..coord.node_slot {
        let (node, _) = profile.resolve_node_slot(slot)?;
        let is_post = slot >= slot_count - profile.post_nodes.len() as u32;
        if is_post && !with_logits {
            continue;
        }
        index += tiles_for(node_out_len(node, kv_len), node.tile_len);
    }
    let (node, _) = profile.resolve_node_slot(coord.node_slot)?;
    let is_post = coord.node_slot >= slot_count - profile.post_nodes.len() as u32;
    if is_post && !with_logits {
        return None; // post nodes do not exist at non-logit positions
    }
    let tiles = tiles_for(node_out_len(node, kv_len), node.tile_len);
    if (coord.tile_index as u64) >= tiles {
        return None;
    }
    Some(index + coord.tile_index as u64)
}

// =============================================================================================
// Tests
// =============================================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_carriage::PALW_CARRIAGE_ALL_DOMAINS;
    use crate::palw_legs::PALW_LEGS_ALL_DOMAINS;
    use crate::palw_reference::PALW_REFERENCE_ALL_DOMAINS;
    use crate::palw_schedule::PALW_SCHEDULE_ALL_DOMAINS;
    use crate::palw_slash::PALW_S_ALL_DOMAINS;
    use crate::palw_v2::PALW_V2_ALL_DOMAINS;

    fn h64(fill: u8) -> Hash64 {
        Hash64::from_bytes([fill; 64])
    }

    fn node(kind: PalwStepOpKindV1, out: PalwStepOutLenV1, tile: u32) -> PalwStepNodeV1 {
        PalwStepNodeV1 {
            op_kind: kind,
            role: PalwStepNodeRoleV1::Plain,
            weight_name: String::new(),
            weight_dtype: 0,
            out_len: out,
            tile_len: tile,
            kernel_semantics_id: h64(0x11),
        }
    }

    /// A tiny synthetic profile: 3 layers, interval 2 → kinds [GDN, Attention, GDN]. Small
    /// enough for exhaustive bijection checks, still exercising fixed + kv-scaled + ragged
    /// tiles and the logits-only-post rule.
    fn tiny_profile() -> PalwShapeProfileV3 {
        PalwShapeProfileV3 {
            version: PALW_STEP_OBJECT_VERSION_V1,
            layer_count: 3,
            full_attention_interval: 2,
            hidden_dim: 8,
            ffn_dim: 16,
            attn_heads: 2,
            attn_kv_heads: 1,
            attn_head_dim: 4,
            rope_dims: 2,
            rope_sections: [1, 1, 0, 0],
            rope_freq_base_bits: 0x4CBE_BC20, // 1e8f — a bit pattern, never a float
            rms_eps_bits: 0x358637BD,
            l2_eps_bits: 0x358637BD,
            gdn_heads: 2,
            gdn_head_k_dim: 4,
            gdn_head_v_dim: 4,
            gdn_conv_kernel: 4,
            vocab_size: 40,
            repack_on: 1,
            llamafile_on: 1,
            flash_attn_disabled: 1,
            fused_gdn_on: 1,
            use_ref_off: 1,
            kv_cache_f16: 1,
            n_ctx: 64,
            n_batch: 64,
            n_ubatch: 64,
            n_seq: 1,
            n_threads: 4,
            pre_nodes: vec![node(PalwStepOpKindV1::EmbedLookup, PalwStepOutLenV1::Fixed { elements: 8 }, 16)],
            gdn_nodes: vec![
                node(PalwStepOpKindV1::RmsNorm, PalwStepOutLenV1::Fixed { elements: 8 }, 16),
                node(PalwStepOpKindV1::MatMulQuant, PalwStepOutLenV1::Fixed { elements: 24 }, 16), // ragged: 2 tiles
                node(PalwStepOpKindV1::GatedDeltaNet, PalwStepOutLenV1::Fixed { elements: 8 }, 16),
            ],
            attn_nodes: vec![
                node(PalwStepOpKindV1::RmsNorm, PalwStepOutLenV1::Fixed { elements: 8 }, 16),
                node(PalwStepOpKindV1::MatMulF16, PalwStepOutLenV1::KvScaled { multiplier: 2 }, 16),
                node(PalwStepOpKindV1::SoftMax, PalwStepOutLenV1::KvScaled { multiplier: 2 }, 16),
                node(PalwStepOpKindV1::MatMulQuant, PalwStepOutLenV1::Fixed { elements: 8 }, 16),
            ],
            post_nodes: vec![
                node(PalwStepOpKindV1::RmsNorm, PalwStepOutLenV1::Fixed { elements: 8 }, 16),
                node(PalwStepOpKindV1::MatMulQuant, PalwStepOutLenV1::Fixed { elements: 40 }, 16), // logits: 3 tiles
            ],
            reference_ruleset_id: h64(0x22),
            transcendental_bindings: vec![PalwTranscendentalBindingV1 {
                site: PalwTranscendentalSiteV1::VectorExpPolynomial,
                algorithm_id: h64(0x33),
            }],
            contraction_facts: vec![PalwContractionFactV1 { site: PalwContractionSiteV1::RopeRotate, contracted: 1 }],
            kv_chunk_calls: 4,
            state_chunk_map_id: h64(0x44),
        }
    }

    fn tiny_context() -> PalwJobContextV2 {
        PalwJobContextV2 {
            version: crate::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: b"step-test".to_vec(),
            job_id: h64(1),
            job_nullifier: h64(2),
            assignment_id: h64(3),
            execution_seed: [7; 32],
            model_profile_id: h64(4),
            runtime_manifest_hash: h64(5),
            runtime_class_id: h64(6),
            shape_profile_id: h64(7),
            trace_scheme_id: h64(8),
            cu_ruleset_id: h64(9),
            tokenizer_id: h64(10),
            prompt_token_ids_hash: h64(11),
            declared_prefill_tokens: 3,
            exact_decode_tokens: 4, // 1 prefill call + 3 decode calls
            max_context_tokens: 64,
        }
    }

    #[test]
    fn step_domains_are_unique_across_all_palw_modules() {
        let mut seen = std::collections::HashSet::new();
        for d in PALW_STEP_ALL_DOMAINS {
            assert!(seen.insert(*d), "duplicate step domain");
            assert!(d.len() <= 64, "blake2b key cap exceeded");
        }
        for d in PALW_V2_ALL_DOMAINS
            .iter()
            .chain(PALW_S_ALL_DOMAINS.iter())
            .chain(PALW_LEGS_ALL_DOMAINS.iter())
            .chain(PALW_REFERENCE_ALL_DOMAINS.iter())
            .chain(PALW_SCHEDULE_ALL_DOMAINS.iter())
            .chain(PALW_CARRIAGE_ALL_DOMAINS.iter())
        {
            assert!(!seen.contains(d), "step module reuses a foreign domain: {}", String::from_utf8_lossy(d));
        }
    }

    #[test]
    fn shape_profile_id_golden_vector() {
        // Frozen 2026-08-16: the SCHEMA hash derivation (canonical borsh under the v3 domain)
        // over the synthetic test profile. This pins the encoding, not a network value — no
        // class profile exists until registration measures one.
        assert_eq!(
            tiny_profile().shape_profile_id().to_string(),
            "0fc08fc60f633712ae0b842190d2a931a37067025759e86b15b7316040a47101\
             212b98d8ee7aeae583d49f89f488de987f3c7fe49f7b01fa88837fb9165a94ff"
        );
    }

    #[test]
    fn shape_profile_id_moves_with_every_field_class() {
        let base = tiny_profile().shape_profile_id();
        let mut p = tiny_profile();
        p.n_threads = 8;
        assert_ne!(p.shape_profile_id(), base, "thread count must move the id");
        let mut p = tiny_profile();
        p.repack_on = 0;
        assert_ne!(p.shape_profile_id(), base, "repack flag must move the id");
        let mut p = tiny_profile();
        p.gdn_nodes[1].kernel_semantics_id = h64(0x99);
        assert_ne!(p.shape_profile_id(), base, "a kernel id must move the id");
        let mut p = tiny_profile();
        p.gdn_nodes[1].tile_len = 32;
        assert_ne!(p.shape_profile_id(), base, "a tile length must move the id");
        let mut p = tiny_profile();
        p.contraction_facts[0].contracted = 0;
        assert_ne!(p.shape_profile_id(), base, "a contraction fact must move the id");
        let mut p = tiny_profile();
        p.rope_freq_base_bits ^= 1;
        assert_ne!(p.shape_profile_id(), base, "a geometry bit must move the id");
    }

    #[test]
    fn validation_rejects_the_named_hazards() {
        let mut p = tiny_profile();
        p.flash_attn_disabled = 0;
        assert!(matches!(p.validate_shape(), Err(PalwStepError::ProfileNotCanonical(_))));
        let mut p = tiny_profile();
        p.gdn_nodes[0].tile_len = 1; // below MIN
        assert!(p.validate_shape().is_err());
        let mut p = tiny_profile();
        p.attn_nodes.clear(); // attention layers exist but no table
        assert!(p.validate_shape().is_err());
        let mut p = tiny_profile();
        p.gdn_nodes[0].weight_name = "w".into(); // name without dtype
        assert!(p.validate_shape().is_err());
        assert!(tiny_profile().validate_shape().is_ok());
    }

    #[test]
    fn layer_kinds_follow_the_interval_rule() {
        let p = tiny_profile();
        assert_eq!(p.layer_kind(0), PalwLayerKindV1::GatedDeltaNet);
        assert_eq!(p.layer_kind(1), PalwLayerKindV1::Attention);
        assert_eq!(p.layer_kind(2), PalwLayerKindV1::GatedDeltaNet);
        // The pinned model's rule: 24 layers, interval 4 → attention at 3, 7, 11, 15, 19, 23.
        let mut q = tiny_profile();
        q.layer_count = 24;
        q.full_attention_interval = 4;
        let attn: Vec<u16> = (0..24).filter(|&l| q.layer_kind(l) == PalwLayerKindV1::Attention).collect();
        assert_eq!(attn, vec![3, 7, 11, 15, 19, 23]);
    }

    #[test]
    fn step_bijection_is_exhaustive_on_the_tiny_job() {
        let p = tiny_profile();
        let ctx = tiny_context();
        let total = step_leaf_count(&p, &ctx).unwrap();
        let aux = kv_aux_leaf_count(&p, &ctx);
        assert!(aux > 0, "the tiny profile must exercise the aux series");
        let main = total - aux;
        let mut seen = std::collections::HashSet::new();
        for i in 0..main {
            let coord =
                canonical_step_coordinates(&p, &ctx, i).unwrap_or_else(|| panic!("main leaf {i} of {main} has no coordinates"));
            assert!(seen.insert(coord), "coordinate collision at {i}: {coord:?}");
            let back = canonical_step_leaf_index(&p, &ctx, &coord).unwrap_or_else(|| panic!("coordinates {coord:?} do not rank back"));
            assert_eq!(back, i, "bijection broke at {i}: {coord:?}");
        }
        // Aux and beyond have no main coordinates.
        assert_eq!(canonical_step_coordinates(&p, &ctx, main), None);
        assert_eq!(canonical_step_coordinates(&p, &ctx, total), None);
        assert_eq!(canonical_step_coordinates(&p, &ctx, total + 1), None);
    }

    #[test]
    fn non_canonical_coordinates_do_not_rank() {
        let p = tiny_profile();
        let ctx = tiny_context();
        // Post node at a non-logit prefill position.
        let post_slot = p.global_node_count() - 1;
        let bad = PalwStepCoordinateV1 { call_index: 0, node_slot: post_slot, position: 0, tile_index: 0 };
        assert_eq!(canonical_step_leaf_index(&p, &ctx, &bad), None);
        // …but the same slot at the last prefill position is canonical.
        let good = PalwStepCoordinateV1 { call_index: 0, node_slot: post_slot, position: 2, tile_index: 0 };
        assert!(canonical_step_leaf_index(&p, &ctx, &good).is_some());
        // Position out of range for a decode call.
        let bad = PalwStepCoordinateV1 { call_index: 1, node_slot: 0, position: 1, tile_index: 0 };
        assert_eq!(canonical_step_leaf_index(&p, &ctx, &bad), None);
        // Tile past the ragged end.
        let bad = PalwStepCoordinateV1 { call_index: 1, node_slot: 2, position: 0, tile_index: 99 };
        assert_eq!(canonical_step_leaf_index(&p, &ctx, &bad), None);
        // Call past the job.
        let bad = PalwStepCoordinateV1 { call_index: 4, node_slot: 0, position: 0, tile_index: 0 };
        assert_eq!(canonical_step_leaf_index(&p, &ctx, &bad), None);
    }

    #[test]
    fn kv_scaled_lengths_use_true_kv_never_padded() {
        // Fact 17: the scores row at prefill position p has kv_len = p+1 elements per
        // multiplier unit — visible as tile counts that grow with p.
        let p = tiny_profile();
        let ctx = tiny_context();
        // attn layer is layer 1; its MatMulF16 node is global slot: pre(1) + gdn(3) + 1 = 5.
        let scores_slot = 5;
        let (node, layer) = p.resolve_node_slot(scores_slot).unwrap();
        assert_eq!(node.op_kind, PalwStepOpKindV1::MatMulF16);
        assert_eq!(layer, Some(1));
        // At prefill position 0, kv_len 1: 2 elements → 1 tile. At decode call 3, kv_len 6:
        // 12 elements → 1 tile (tile 16). Grow the multiplier's effect via leaf counts:
        let idx_p0 = canonical_step_leaf_index(
            &p,
            &ctx,
            &PalwStepCoordinateV1 { call_index: 0, node_slot: scores_slot, position: 0, tile_index: 0 },
        );
        assert!(idx_p0.is_some());
        let no_second_tile = canonical_step_leaf_index(
            &p,
            &ctx,
            &PalwStepCoordinateV1 { call_index: 0, node_slot: scores_slot, position: 0, tile_index: 1 },
        );
        assert_eq!(no_second_tile, None, "kv_len 1 × mult 2 = 2 elements is one tile");
    }

    #[test]
    fn leaf_cap_is_enforced() {
        let p = tiny_profile();
        let mut ctx = tiny_context();
        ctx.declared_prefill_tokens = 4096;
        ctx.exact_decode_tokens = 4095;
        // 4096 prefill positions × (kv-scaled tiles growing to 8192) blows the 2^22 cap for
        // this synthetic profile? If not, push decode.
        match step_leaf_count(&p, &ctx) {
            Ok(n) => assert!(n <= PALW_STEP_MAX_LEAVES),
            Err(PalwStepError::TooManyLeaves { got, max }) => assert!(got > max),
            Err(e) => panic!("unexpected error {e:?}"),
        }
    }

    #[test]
    fn kernel_and_transcendental_ids_are_domain_separated() {
        let k = kernel_semantics_id_v1("repack-gemv/q4_K_8x4_q8_K/neon-dotprod/llama-030ebb558/v1");
        let t = transcendental_algorithm_id_v1("repack-gemv/q4_K_8x4_q8_K/neon-dotprod/llama-030ebb558/v1");
        let s = state_chunk_map_id_v1("repack-gemv/q4_K_8x4_q8_K/neon-dotprod/llama-030ebb558/v1");
        assert_ne!(k, t, "same string under different domains must differ");
        assert_ne!(k, s);
        assert_ne!(t, s);
    }
}
