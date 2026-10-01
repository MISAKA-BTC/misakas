//! **The feature vocabulary of `ModelSpec` V1.**
//!
//! A model is never "a Qwen4" to the lowerer, the artifact or the court: it is a *combination* of
//! features from this finite, versioned vocabulary, and a feature is lowered once, by a generic
//! lowerer, to the existing PALW-TIR primitives. A new model is a new combination (or an adapter
//! file selecting one), never a new primitive, runtime or court kernel. When a model needs something
//! the vocabulary cannot express, the gap is a *missing capability*, named in [`Requirement`] together
//! with the smallest **general** primitive that would close it — never a per-model primitive.
//!
//! Each [`FeatureInfo`] states
//!
//! * its stable id (`<AREA>_<NAME>_V<n>`; a change of meaning is a new id, never an edit),
//! * whether the generic lowerer implements it ([`Lowering`]),
//! * the TIR primitives its lowering emits (checked against every lowered fixture: a program may use
//!   no primitive that none of its features declares, `tests/feature_registry.rs`),
//! * the protocol requirement ([`Requirement`]): none, or a named capability the chain lacks,
//! * the tests that exercise it.
//!
//! [`ModelSpec::features`] reads a spec and lists the features it uses.

use crate::spec::*;
use serde::Serialize;
use std::collections::BTreeMap;

/// A stable, versioned feature identifier, e.g. `ATTN_SPARSE_BLOCK_V1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct FeatureId(pub &'static str);

impl std::fmt::Display for FeatureId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

/// Where a feature sits in a model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Area {
    Embedding,
    Norm,
    Position,
    Attention,
    Mixer,
    Ffn,
    Residual,
    Head,
    Storage,
    /// What kind of model the whole is (an encoder–decoder, …), not one layer's part.
    Model,
}

/// Does the generic lowerer turn this feature into a TIR program?
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Lowering {
    /// Lowered, calibrated and run on the three implementations by the named tests.
    Implemented,
    /// The spec can describe it and the float reference runs it, but the generic lowerer does not
    /// lower it yet (the lowerer refuses with the feature's id).
    Specified,
    /// Not describable yet: a model needing it is refused by name (Level C).
    Missing,
}

/// What the *protocol* would need for this feature, beyond the 25 primitives of `prim_set_id` v1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Requirement {
    /// Expressible with the existing primitives and the existing court paths.
    None,
    /// Not expressible, or only at a cost the chain should not pay: a missing capability. Never a
    /// per-model primitive: `general_primitive` names the smallest *general* addition that closes it.
    Capability {
        id: &'static str,
        general_primitive: &'static str,
        /// The addition is a new TIR primitive (a change of `prim_set_id`).
        new_primitive: bool,
        /// The addition needs a court kernel the court does not have.
        new_court_kernel: bool,
    },
}

/// One entry of the registry.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct FeatureInfo {
    pub id: FeatureId,
    pub area: Area,
    pub title: &'static str,
    pub lowering: Lowering,
    /// TIR primitives the lowering emits for this feature, beyond the base every model shares
    /// ([`BASE_PRIMITIVES`]).
    pub primitives: &'static [&'static str],
    pub protocol: Requirement,
    /// `<test file>::<test or fixture>`: what exercises the feature.
    pub tests: &'static [&'static str],
    pub doc: &'static str,
}

/// The primitives every lowered program uses: the embedding gather, the projections' `MatMul`s, the
/// narrowings between scales (`Mul`, `Div`, `Add`, `Clamp`, the `Pow2` gather), the reshapes around
/// them, and a norm's `Sub`, `ReduceSum`, `Log2Floor`, `IntRsqrt` and the `IntExp` of the tables'
/// sigmoid. Measured: the 68 HF fixtures all use exactly these and no other in common.
pub const BASE_PRIMITIVES: &[&str] =
    &["Gather", "MatMul", "Mul", "Add", "Sub", "Div", "Clamp", "Reshape", "Cast", "ReduceSum", "Log2Floor", "IntRsqrt", "IntExp"];

macro_rules! feature {
    ($id:literal, $area:ident, $title:literal, $lowering:ident, [$($prim:literal),*], $proto:expr, [$($test:literal),*], $doc:literal) => {
        FeatureInfo {
            id: FeatureId($id),
            area: Area::$area,
            title: $title,
            lowering: Lowering::$lowering,
            primitives: &[$($prim),*],
            protocol: $proto,
            tests: &[$($test),*],
            doc: $doc,
        }
    };
}

use Requirement::None as NoReq;

/// Every feature `ModelSpec` V1 can describe or name as missing, in id order within each area.
pub static REGISTRY: &[FeatureInfo] = &[
    // ───────────────────────────── embedding ─────────────────────────────
    feature!("EMBED_TOKEN_V1", Embedding, "token-id embedding table", Implemented, [], NoReq, ["hf_fixtures::llama", "fidelity_tiny::llama"], "A row of the table per token id; a tied head reads the same table."),
    feature!("EMBED_SCALE_V1", Embedding, "constant multiplier on the embedding", Implemented, [], NoReq, ["fidelity_tiny::gemma", "fidelity_tiny::granite"], "Gemma's √d, Granite's multiplier: folded into the value's scale."),
    feature!("EMBED_NORM_V1", Embedding, "norm on the embedding", Implemented, [], NoReq, ["fidelity_tiny::bloom"], "BLOOM's word-embedding LayerNorm."),
    feature!("EMBED_POSITION_LEARNED_V1", Embedding, "learned absolute positions", Implemented, [], NoReq, ["fidelity_tiny::gpt2", "fidelity_tiny::opt"], "A table row per position."),
    feature!("EMBED_PROJ_IN_V1", Embedding, "projection from the embedding width to the hidden width", Implemented, [], NoReq, ["fidelity_tiny::opt_postln_proj"], "OPT-350m's `project_in`."),
    feature!("EMBED_TOKEN_TYPE_V1", Embedding, "token-type embedding", Implemented, [], NoReq, ["encoders::bert_mean_pooled_and_normalised_matches_its_hf_fixture"], "BERT-style segment table (encoders)."),
    feature!("EMBED_PER_LAYER_INPUT_V1", Embedding, "per-layer input from the token", Implemented, [], NoReq, ["fidelity_tiny::gemma4"], "Gemma-4's second, per-layer embedding gating a branch of each layer."),
    feature!("EMBED_NGRAM_PLE_V1", Embedding, "hashed n-gram per-layer embedding", Implemented, ["Concat", "Compare", "Select", "Slice", "StateWrite"], NoReq, ["qwen4_exp::PLE_01_bigram", "qwen4_exp::PLE_02_trigram", "qwen4_exp::PLE_03_hash_boundary", "qwen4_exp::PLE_04_streaming_cache", "qwen4_exp::PLE_05_dilated_conv_boundary", "qwen4_exp::PLE_06_a_table_taller_than_a_chunk_is_read_by_chunks"], "Hashed bigram/trigram rows per layer (XOR of token·multiplier, prime-modulus buckets), a key per stream and a value from the rows, the streams' normed query gating the value through σ(signed √(k·q/√D)), then a dilated depthwise causal convolution (CONV_DEPTHWISE_CAUSAL_V1), added to every stream. The hash is computed IN the program, with no consensus change: bit decomposition of the n products by floor-divisions by 2^k, an XOR as the parity of a bit sum (one 0/1 triangular MatMul for every order), the recomposition as a weighted MatMul into i128, `mixed mod size` as a floor-division and a Mul in i128. MEASURED on the lowered program: 26 nodes for the ids of a trigram layer and 14 for the table read, a 187-node block with the three norms. The segment window (the last n−1 tokens, an eos ending a segment) is a Fixed state of `token − eos`, so zero is a fresh sequence. A table taller than NF-8's 2^24 rows is a LOWERING matter, not a capability: it is split per hash head (each head owns a contiguous range of prime size) into `[heads, rows, dim]` i16 codes at one scale per layer and cut into chunks of at most 2^24 rows, one batched Gather (batch_dims 1) per chunk and a Select by the chunk index, so no dimension passes the cap. ADVISORY, evidence for a possible one-time primitive-set extension (decided later, not required): general integer bit primitives — XOR, shifts, a wrapping multiply, a remainder — would cut the ids from ~26 nodes to ~6 (Mul, two XORs, Rem, Add, Gather)."),
    // ───────────────────────────── norms ─────────────────────────────
    feature!("NORM_RMS_V1", Norm, "RMS normalisation", Implemented, [], NoReq, ["fidelity_tiny::llama"], "x·rsqrt(mean(x²) + ε) with the exponent taken out before `IntRsqrt` (wide template)."),
    feature!("NORM_LAYER_V1", Norm, "layer normalisation", Implemented, [], NoReq, ["fidelity_tiny::gpt2"], "Centres exactly (n·x − Σx), then the RMS template."),
    feature!("NORM_GAIN_ONE_PLUS_V1", Norm, "gain stored as w, applied as 1 + w", Implemented, [], NoReq, ["fidelity_tiny::gemma"], "Gemma, Qwen3-Next, Nemotron."),
    feature!("NORM_UNWEIGHTED_V1", Norm, "norm without a gain", Implemented, [], NoReq, ["fidelity_tiny::olmo"], "OLMo-1's non-parametric LayerNorm, Gemma-4's V-norm."),
    feature!("NORM_GROUPED_V1", Norm, "norm over equal groups of a vector", Implemented, [], NoReq, ["fidelity_tiny::qwen3_moe"], "Per-head QK-norm; the streams of a hyper-connection."),
    // ───────────────────────────── positions ─────────────────────────────
    feature!("ROPE_DEFAULT_V1", Position, "rotary position embedding", Implemented, ["Concat", "Slice"], NoReq, ["fidelity_tiny::llama"], "Angles from two pinned tables by angle addition (04b §11.3); half-split pairs rotate by slices."),
    feature!("ROPE_LINEAR_V1", Position, "linear-scaled rope", Implemented, [], NoReq, ["fidelity_tiny::llama_linear_tied"], "inv_freq / factor."),
    feature!("ROPE_DYNAMIC_NTK_V1", Position, "dynamic-NTK rope", Implemented, [], NoReq, ["fidelity_tiny::qwen2_dynamic"], "The base grows with the sequence length: one table row per position."),
    feature!("ROPE_YARN_V1", Position, "YaRN rope", Implemented, [], NoReq, ["fidelity_tiny::qwen3_yarn"], "Ramp between interpolated and extrapolated frequencies, with an attention factor."),
    feature!("ROPE_LLAMA3_V1", Position, "Llama-3.1 rope scaling", Implemented, [], NoReq, ["fidelity_tiny::llama"], "Smooth interpolation by wavelength."),
    feature!("ROPE_LONGROPE_V1", Position, "LongRoPE (Phi-3)", Implemented, [], NoReq, ["fidelity_tiny::phi3_longrope"], "Short/long factor lists switching at the original length."),
    feature!("ROPE_PROPORTIONAL_V1", Position, "proportional rope", Implemented, [], NoReq, ["fidelity_tiny::gemma4", "fidelity_tiny::gemma4_kvshare"], "Gemma-4's full-attention rope: frequencies on the first `partial_rotary_factor` of the head width, zeros after (those pairs are not rotated), divided by `factor`."),
    feature!("ROPE_PARTIAL_V1", Position, "rotation of a prefix of each head", Implemented, [], NoReq, ["fidelity_tiny::phi", "fidelity_tiny::stablelm_parallel"], "`partial_rotary_factor` < 1: the rest of the head passes through."),
    feature!("ROPE_INTERLEAVED_V1", Position, "interleaved rotary pairs", Implemented, [], NoReq, ["fidelity_tiny::gptj", "fidelity_tiny::glm"], "(2i, 2i+1) pairs instead of (i, i + d/2)."),
    feature!("ROPE_MROPE_V1", Position, "multimodal rope sections", Implemented, [], NoReq, ["fidelity_tiny::qwen3_5"], "Sections of the frequencies take the t/h/w position components; text-only they are all the position."),
    feature!("POS_ALIBI_V1", Position, "ALiBi distance bias", Implemented, ["Iota"], NoReq, ["fidelity_tiny::bloom", "fidelity_tiny::falcon_alibi", "fidelity_tiny::mpt"], "A per-head slope times the key distance, added to the scores."),
    feature!("POS_NOPE_V1", Position, "attention layers without a positional term", Implemented, [], NoReq, ["fidelity_tiny::smollm3"], "SmolLM3, Cohere-2 and Llama-4 global layers."),
    feature!("POS_RELATIVE_BIAS_V1", Position, "bucketed relative-position bias", Implemented, ["Iota"], NoReq, ["encoders::mpnet_with_its_relative_bias_matches_its_hf_fixture"], "T5/MPNet bias table indexed by bucketed distance."),
    // ───────────────────────────── attention ─────────────────────────────
    feature!("ATTN_GQA_V1", Attention, "grouped-query attention (covers MHA and MQA)", Implemented, ["HistAppend", "ReduceMax", "Transpose"], NoReq, ["fidelity_tiny::llama", "fidelity_tiny::falcon_mq", "fidelity_tiny::gpt_bigcode_mha"], "Two-pass softmax over the history through the library's `attention` template."),
    feature!("ATTN_SLIDING_V1", Attention, "sliding-window attention", Implemented, [], NoReq, ["fidelity_tiny::mistral_window", "fidelity_tiny::qwen2_sliding"], "The history keeps `window` rows."),
    feature!("ATTN_CHUNKED_V1", Attention, "chunked attention", Implemented, ["Iota", "Compare", "Select"], NoReq, ["fidelity_tiny::llama4"], "A query sees only the keys of its own chunk (Llama-4)."),
    feature!("ATTN_SOFTCAP_V1", Attention, "tanh soft-capping of the scores", Implemented, ["Compare", "Select"], NoReq, ["fidelity_tiny::gemma2"], "cap·tanh(s/cap) through the integer sigmoid."),
    feature!("ATTN_SINKS_V1", Attention, "attention sinks", Implemented, [], NoReq, ["fidelity_tiny::gpt_oss"], "A learned logit per head that joins the softmax and is dropped."),
    feature!("ATTN_OUTPUT_GATE_V1", Attention, "sigmoid gate on the attention output", Implemented, [], NoReq, ["fidelity_tiny::qwen3_next", "fidelity_tiny::qwen3_5"], "q_proj also emits a per-head gate."),
    feature!("ATTN_QK_NORM_V1", Attention, "norm on the queries and keys", Implemented, [], NoReq, ["fidelity_tiny::qwen3_moe", "fidelity_tiny::olmoe", "fidelity_tiny::cohere"], "Per-head shared, per-head separate or whole-projection scope."),
    feature!("ATTN_QK_NORM_POST_ROPE_V1", Attention, "the q/k norms act after the rotation", Implemented, [], NoReq, ["qk_norm_post_rope::the_order_of_the_norm_and_the_rotation_is_part_of_the_function"], "Hunyuan: q = rope(q); q = RMSNorm_head(q). Qwen3's order is the reverse. A rotation preserves a head's L2 norm but a per-channel gain does not commute with it, so the orders are different functions (8.9e-2 and 4.6e-2 of the logit scale on the two Hunyuan fixtures). The history keeps the normed, rotated key; no node is new, only their order."),
    feature!("ATTN_V_NORM_V1", Attention, "norm on the values", Implemented, [], NoReq, ["fidelity_tiny::gemma4"], "Gemma-4."),
    feature!("ATTN_CLIP_QKV_V1", Attention, "clamp of q, k and v after projection", Implemented, [], NoReq, ["fidelity_tiny::olmo"], "OLMo `clip_qkv`."),
    feature!("ATTN_QUERY_TEMPERATURE_V1", Attention, "position-dependent query scaling", Implemented, [], NoReq, ["fidelity_tiny::llama4", "fidelity_tiny::ministral3"], "Llama-4 / Ministral-3."),
    feature!("ATTN_KV_SHARE_V1", Attention, "layers reading another layer's keys and values", Implemented, ["Iota"], NoReq, ["fidelity_tiny::gemma4_kvshare"], "Rows ride between layers as carries."),
    feature!("ATTN_K_EQ_V_V1", Attention, "values equal to the key projection", Implemented, [], NoReq, ["fidelity_tiny::gemma4"], "Gemma-4 `attention_k_eq_v`."),
    feature!("ATTN_BIAS_V1", Attention, "biases on the attention projections", Implemented, [], NoReq, ["fidelity_tiny::qwen2_sliding"], "q, k, v and/or o."),
    feature!("ATTN_SPARSE_BLOCK_V1", Attention, "block-sparse key selection by a learned index", Implemented, ["Iota", "Compare", "Select", "TopK", "StateWrite", "Transpose", "ReduceMax", "Slice"], NoReq, ["qwen4_exp::QSA_01_k_1", "qwen4_exp::QSA_02_k_max", "qwen4_exp::QSA_03_score_tie", "qwen4_exp::QSA_04_incomplete_trailing_block", "qwen4_exp::QSA_05_causal_boundary", "qwen4_exp::QSA_06_cached_generation"], "Mean-pooled block keys (a running sum state restarted at each block's first position, normed and rotated at the block's START position), ReLU scores Σ_h relu(q_h·K_b) summed over the index heads (the 1/√dim of the float form moves no rank and is dropped), a fixed-K TopK over EVERY block the program holds keys for with the incomplete blocks masked to −1 (higher score first, equal scores the lower block index, the set in ascending index order: 04b §6.6), the incomplete tail always visible. Values are data-dependent, shapes never: the attention logits of the keys outside the selected blocks take the softmax's floor through a Select. The block-key matrix is a Fixed state written by a one-hot Select (cost O(blocks·dim) a position, evidence for a dynamic-row-write primitive, not required)."),
    // ───────────────────────────── mixers ─────────────────────────────
    feature!("MIXER_MLA_V1", Mixer, "multi-head latent attention", Implemented, ["HistAppend", "ReduceMax", "Transpose", "Concat", "Slice"], NoReq, ["fidelity_tiny::deepseek_v2", "fidelity_tiny::deepseek_v3"], "A compressed latent history; the absorbed query reads it."),
    feature!("MIXER_GDN_V1", Mixer, "gated delta rule (any key:value head ratio)", Implemented, ["StateWrite", "IntLn", "Broadcast", "Compare", "Select", "Concat", "Slice", "Transpose"], NoReq, ["fidelity_tiny::qwen3_next", "fidelity_tiny::qwen3_5"], "S ← S·exp(g); S += k (β(v − Sᵀk))ᵀ per value head, the key heads mapped to the value heads by grouping."),
    feature!("MIXER_MAMBA_V1", Mixer, "Mamba-1 selective scan", Implemented, ["StateWrite", "IntLn", "Compare", "Select", "Concat", "Slice", "Transpose"], NoReq, ["fidelity_tiny::mamba", "fidelity_tiny::falcon_mamba"], "Per-channel, per-state decay."),
    feature!("MIXER_MAMBA2_V1", Mixer, "Mamba-2 state-space duality step", Implemented, ["StateWrite", "IntLn", "Broadcast", "Compare", "Select", "Concat", "Slice", "Transpose"], NoReq, ["fidelity_tiny::mamba2"], "Scalar decay per head, grouped B/C."),
    feature!("MIXER_RWKV4_V1", Mixer, "RWKV-4 time mix", Implemented, ["StateWrite", "Compare", "Select"], NoReq, ["fidelity_tiny::rwkv"], "WKV with the (num, den, max) stabilised state."),
    feature!("MIXER_RWKV56_V1", Mixer, "RWKV-5/6 time mix", Specified, [], NoReq, ["library::rwkv6_step"], "In the TIR library (`rwkv6_step`); no config lowering (remote-code references only)."),
    feature!("MIXER_RWKV7_V1", Mixer, "RWKV-7 time mix", Specified, [], NoReq, ["library::rwkv7_step"], "In the TIR library (`rwkv7_step`); no config lowering."),
    feature!("CONV_DEPTHWISE_CAUSAL_V1", Mixer, "depthwise causal convolution (any dilation)", Implemented, ["StateWrite", "Concat", "Slice", "Transpose"], NoReq, ["fidelity_tiny::qwen3_5", "fidelity_tiny::mamba", "qwen4_exp::PLE_05_dilated_conv_boundary"], "A window state of (kernel − 1)·dilation rows (zero before the sequence: the left pad); the window is state ++ row and the taps read its rows `dilation` apart through one Gather by a constant index vector (dilation 1 is the library's contiguous template)."),
    // ───────────────────────────── feed-forward ─────────────────────────────
    feature!("MLP_DENSE_GATED_V1", Ffn, "gated MLP", Implemented, [], NoReq, ["fidelity_tiny::llama"], "act(gate)·up, then down."),
    feature!("MLP_DENSE_PLAIN_V1", Ffn, "ungated MLP", Implemented, [], NoReq, ["fidelity_tiny::gpt2"], "act(fc1), then fc2."),
    feature!("MLP_GLU_CLAMPED_V1", Ffn, "clamped SwiGLU", Implemented, ["Compare", "Select"], NoReq, ["fidelity_tiny::gpt_oss"], "gpt-oss: (clamp(up) + 1)·g·σ(αg)."),
    feature!("MLP_BIAS_V1", Ffn, "biases on the MLP projections", Implemented, [], NoReq, ["fidelity_tiny::starcoder2"], "up and/or down."),
    feature!("MLP_MOE_TOPK_V1", Ffn, "routed experts: top-k of the router scores", Implemented, ["TopK"], NoReq, ["fidelity_tiny::mixtral", "fidelity_tiny::qwen3_moe"], "TopK is a commit point; ties go to the lower expert index and the set is in index order (04b §6.6). Any expert count and k."),
    feature!("MLP_MOE_ROUTER_SOFTMAX_V1", Ffn, "softmax router, top-k of the probabilities", Implemented, ["ReduceMax"], NoReq, ["fidelity_tiny::mixtral"], "Mixtral, Qwen-MoE, OLMoE, DeepSeek-V2."),
    feature!("MLP_MOE_ROUTER_SIGMOID_V1", Ffn, "sigmoid router", Implemented, [], NoReq, ["fidelity_tiny::deepseek_v3"], "DeepSeek-V3, GLM-4.5."),
    feature!("MLP_MOE_ROUTER_TOPK_SOFTMAX_V1", Ffn, "top-k of the logits, then softmax over the k", Implemented, [], NoReq, ["fidelity_tiny::gpt_oss", "fidelity_tiny::granitemoe"], "gpt-oss, GraniteMoE."),
    feature!("MLP_MOE_ROUTER_TOPK_SIGMOID_V1", Ffn, "top-k of the logits, then sigmoid of each", Implemented, [], NoReq, ["fidelity_tiny::llama4"], "Llama-4."),
    feature!("MLP_MOE_ROUTER_SPARSEMIXER_V1", Ffn, "sparsemixer top-2", Implemented, ["Compare", "Select", "Iota"], NoReq, ["fidelity_tiny::phimoe"], "Phi-3.5-MoE at inference."),
    feature!("MLP_MOE_NORM_TOPK_V1", Ffn, "renormalised top-k weights", Implemented, [], NoReq, ["fidelity_tiny::mixtral", "fidelity_tiny::qwen3_moe"], "The k selected weights sum to one."),
    feature!("MLP_MOE_SHARED_EXPERT_V1", Ffn, "shared expert beside the routed ones", Implemented, [], NoReq, ["fidelity_tiny::qwen2_moe", "fidelity_tiny::deepseek_v2"], "Always on; optionally scaled by a sigmoid gate."),
    feature!("MLP_MOE_SHARED_GATE_V1", Ffn, "sigmoid gate on the shared expert", Implemented, [], NoReq, ["fidelity_tiny::qwen2_moe", "fidelity_tiny::qwen3_next"], "σ(w·x) scales the shared expert."),
    feature!("MLP_MOE_GROUPED_ROUTING_V1", Ffn, "group-limited routing", Implemented, ["Broadcast", "Compare", "Iota", "Select"], NoReq, ["fidelity_tiny::deepseek_v2", "fidelity_tiny::deepseek_v3"], "Keep the best groups, mask the rest."),
    feature!("MLP_MOE_SELECTION_BIAS_V1", Ffn, "selection-only router bias", Implemented, [], NoReq, ["fidelity_tiny::deepseek_v3"], "DeepSeek-V3 `e_score_correction_bias`."),
    feature!("MLP_MOE_INPUT_SCALED_V1", Ffn, "routing weight scales the expert input", Implemented, [], NoReq, ["fidelity_tiny::llama4"], "Llama-4."),
    feature!("MLP_MOE_EXPERT_BIAS_V1", Ffn, "biases on the expert projections", Implemented, [], NoReq, ["fidelity_tiny::gpt_oss"], "gpt-oss."),
    feature!("MLP_MOE_EXPERT_SCALE_V1", Ffn, "learned per-expert output scale", Implemented, [], NoReq, ["fidelity_tiny::gemma4"], "Gemma-4."),
    feature!("MLP_MOE_BESIDE_DENSE_V1", Ffn, "a MoE block beside the dense MLP", Implemented, [], NoReq, ["fidelity_tiny::gemma4"], "Gemma-4: both read the residual."),
    feature!("FFN_RWKV_CHANNEL_V1", Ffn, "RWKV channel mix", Implemented, ["StateWrite"], NoReq, ["fidelity_tiny::rwkv"], "Token shift, squared ReLU, receptance gate."),
    // ───────────────────────────── residual wiring ─────────────────────────────
    feature!("RESIDUAL_PRE_NORM_V1", Residual, "pre-norm sequential residual", Implemented, [], NoReq, ["fidelity_tiny::llama"], "x += mixer(norm(x)); x += ffn(norm(x))."),
    feature!("RESIDUAL_POST_NORM_V1", Residual, "post-norm residual", Implemented, [], NoReq, ["fidelity_tiny::opt_postln_proj"], "x = norm(x + mixer(x))."),
    feature!("RESIDUAL_PARALLEL_V1", Residual, "parallel attention and FFN", Implemented, [], NoReq, ["fidelity_tiny::gpt_neox", "fidelity_tiny::phi"], "x += mixer(n(x)) + ffn(n'(x))."),
    feature!("RESIDUAL_SANDWICH_V1", Residual, "norms before and after each branch", Implemented, [], NoReq, ["fidelity_tiny::gemma2", "fidelity_tiny::gemma3"], "Gemma-2/3/4: four norms per layer."),
    feature!("RESIDUAL_SCALED_V1", Residual, "constant multiplier on a branch or on the layer output", Implemented, [], NoReq, ["fidelity_tiny::granite"], "Granite, MiniCPM, RWKV rescale."),
    feature!("RESIDUAL_LAYER_SCALAR_V1", Residual, "learned per-layer scalar on the layer output", Implemented, [], NoReq, ["fidelity_tiny::gemma4"], "Gemma-4."),
    feature!("RESIDUAL_GATED_HC_V1", Residual, "gated residual: hyper-connections over several streams", Implemented, ["Concat"], NoReq, ["qwen4_exp::GR_01_hc_count_1", "qwen4_exp::GR_02_hc_count_4", "qwen4_exp::GR_03_gate_edge_values"], "A residual of `streams` rows of the hidden width (the carry is streams × hidden i32; the embedding is repeated into every stream, and one stream is the plain residual). Each block reads a gated mean of the streams — a grouped RMS norm (1 + w), a low-rank pair of projections with a SiLU between them, a sigmoid gate on the normed streams — and writes back through learned injection weights 2σ(W·normed/streams) into every stream; the last mix, with no injection, brings the streams to the head. Two blocks a layer (mixer half, FFN half) keep each under NF-12's 512 nodes; a layer with an n-gram embedding runs it as a third."),
    // ───────────────────────────── head ─────────────────────────────
    feature!("HEAD_TIED_V1", Head, "head tied to the embedding table", Implemented, [], NoReq, ["fidelity_tiny::llama_linear_tied"], "One `i16` table read twice."),
    feature!("HEAD_BIAS_V1", Head, "bias on the head", Implemented, [], NoReq, ["fidelity_tiny::falcon_new"], ""),
    feature!("HEAD_SOFTCAP_V1", Head, "tanh soft-capping of the logits", Implemented, [], NoReq, ["fidelity_tiny::gemma2"], "Gemma-2/4."),
    feature!("HEAD_LOGIT_SCALE_V1", Head, "constant multiplier on the logits", Implemented, [], NoReq, ["fidelity_tiny::cohere", "fidelity_tiny::granite"], "Cohere, Granite."),
    feature!("HEAD_PRE_SCALE_V1", Head, "constant multiplier before the head", Implemented, [], NoReq, ["real_configs::minicpm"], "MiniCPM."),
    feature!("HEAD_PROJ_OUT_V1", Head, "projection to the embedding width before the head", Implemented, [], NoReq, ["fidelity_tiny::opt_postln_proj"], "OPT-350m."),
    feature!("OUTPUT_LOGITS_V1", Head, "logits output", Implemented, [], NoReq, ["fidelity_tiny::llama"], "Next-token logits."),
    feature!("OUTPUT_EMBEDDING_V1", Head, "embedding output (pooled hidden row)", Implemented, [], NoReq, ["encoders::clip_text_encoder_matches_its_hf_fixture"], "RFC-0003 Embedding profile."),
    // ───────────────────────────── known gaps (Level C): named, not implemented ─────────────────────────────
    feature!("RESIDUAL_ALTUP_V1", Residual, "AltUp: a predicted/corrected multi-stream residual", Missing, [], NoReq, [], "Gemma-3n. Expressible with the existing primitives once described; not in the vocabulary yet."),
    feature!("RESIDUAL_LAUREL_V1", Residual, "LAuReL: a learned low-rank residual branch", Missing, [], NoReq, [], "Gemma-3n."),
    feature!("FFN_ACTIVATION_SPARSITY_V1", Ffn, "Gaussian top-k sparsity of the gate activation", Missing, [], NoReq, [], "Gemma-3n."),
    feature!("ATTN_CROSS_V1", Attention, "cross-attention to another sequence's states", Missing, [], NoReq, [], "Mllama's decoder layers reading vision states; needs a second history input."),
    feature!("ATTN_PREFIX_LM_V1", Attention, "bidirectional attention over a prompt prefix", Missing, [], NoReq, [], "PaliGemma."),
    feature!("ATTN_BLOCKSPARSE_PATTERN_V1", Attention, "a fixed block-sparse pattern of visible keys", Missing, [], NoReq, [], "Phi-3-small."),
    feature!("SCALE_MUP_V1", Residual, "muP multipliers on the embedding, branches and logits", Missing, [], NoReq, [], "Per-model constants over existing multiplier features; an adapter once a rule names them."),
    feature!("REFERENCE_REMOTE_CODE_V1", Storage, "a reference implementation that is remote Python code outside transformers", Missing, [], NoReq, [], "The semantics cannot be pinned to a library version; confirm against the module before an adapter may claim it."),
    feature!("WEIGHTS_QKV_MP_PARTITIONED_V1", Storage, "mp_num-partitioned fused qkv weight layout", Missing, [], NoReq, [], "CodeGen."),
    feature!("MIXER_LAYER_PATTERN_HYBRID_V1", Mixer, "per-layer mixer pattern with Mamba-2 and attention sharing an MoE feed-forward", Missing, [], NoReq, [], "Granite-4 hybrid: all parts exist; the layer pattern keys are not mapped yet."),
    feature!("LAYER_FFN_ONLY_V1", Mixer, "layers that are a single block (a mixer or an FFN, not both)", Missing, [], NoReq, [], "Nemotron-H."),
    feature!("MIXER_PARALLEL_BRANCH_V1", Mixer, "two mixers in parallel in one layer, summed", Missing, [], NoReq, [], "Falcon-H1."),
    feature!("ATTN_SHARED_BLOCK_V1", Attention, "one attention block's weights reused at several depths", Missing, [], NoReq, [], "Zamba2."),
    // ───────────────────────────── storage ─────────────────────────────
    feature!("QUANT_GPTQ_V1", Storage, "GPTQ-quantised projections, lowered from the stored integers", Implemented, [], NoReq, ["quantized::gptq_b4_g128_act_asym"], "Grouped integer matmul with the checkpoint's scales."),
    feature!("QUANT_AWQ_V1", Storage, "AWQ-quantised projections", Implemented, [], NoReq, ["quantized::awq_g128"], "Same, activation-aware scales."),
    feature!("QUANT_GGUF_V1", Storage, "GGUF block-quantised tensors", Implemented, [], NoReq, ["gguf::gguf_llama_q4_k_m"], "Q4_0 … Q8_0, K-quants."),
    feature!("ADAPTER_LORA_V1", Storage, "LoRA adapter over the parent (candidate = parent + adapter)", Implemented, [], NoReq, ["lora::llama_r16"], "Unmerged low-rank path."),
    feature!("ENCDEC_FROM_SPEC_V1", Model, "an encoder-decoder as data: two stages (the encoder over the padded source, the decoder with cross-attention) described by an adapter of kind `encdec`", Implemented, [], NoReq, ["encdec_adapters::the_t5_adapter_reads_what_the_rust_reader_read", "encdec_adapters::every_family_adapter_reads_what_the_rust_reader_read"], "Stage 0, the encoder, is ONE position over the padded source axis; its Final output is every decoder layer's cross-attention keys and values (one MatMul against the stacked weights). Stage 1 is the decoder's text stage. What the Rust route hard-wired per family (the five parsers and their tensor-name tables) is the adapter's data; the lowering is unchanged (Phase 1 of docs/design/palw/tir/frontend-as-data-v1.md section 3)."),
    feature!("ATTN_CROSS_ENCDEC_V1", Attention, "cross-attention of a decoder to its encoder's output (the stage-0 Final)", Implemented, [], NoReq, ["encdec_adapters::every_family_adapter_reads_what_the_rust_reader_read"], "Keys and values come from the encoder through StageFinal, per decoder layer; attention runs over the source axis (a Fixed axis, not H) with the keys at or past the source length masked. Distinct from ATTN_CROSS_V1, a decoder-only model reading another sequence's states, which is not modelled."),
    feature!("POS_SINUSOID_V1", Position, "sinusoidal absolute positions (computed, or the checkpoint's table)", Implemented, [], NoReq, ["encdec_adapters::every_family_adapter_reads_what_the_rust_reader_read"], "Marian computes the table, Pegasus stores it: sin in the first half of the width and cos in the second."),
    feature!("WEIGHTS_EXPR_V1", Storage, "weights as data: a tensor expression (reshape, rows, take, stack, transpose, pad, three maps) per HL param, written in the adapter", Implemented, [], NoReq, ["weights_expr::dbrx_flat_experts_bind_by_expression", "weights_expr::the_enumerated_layouts_are_expressions"], "Replaces the default binding of the params it names: a checkpoint layout the enumerated ones (fused qkv, fused gate/up, stacked experts, conv1d) do not describe needs no Rust. Every step is an exact copy or re-indexing (no arithmetic on weights beyond neg_exp, scale and rescale_by_layer), so conversion stays a pure re-indexing; no primitive and no protocol change (the artifact is what the checkpoint holds). The grammar is closed: an unknown step key is an error. docs/design/palw/tir/frontend-as-data-v1.md section 2."),
];

/// Look a feature up by id.
pub fn feature_info(id: &str) -> Option<&'static FeatureInfo> {
    REGISTRY.iter().find(|f| f.id.0 == id)
}

/// A feature a model uses: where, and how much of it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FeatureUse {
    pub id: FeatureId,
    /// The layers using it (empty: the model as a whole).
    pub layers: Vec<usize>,
    /// What the model asks of it (`16:48` heads, `E = 512, k = 10`, …); distinct values, `; `-joined.
    pub detail: String,
}

#[derive(Default)]
struct Uses(BTreeMap<&'static str, (Vec<usize>, Vec<String>)>);

impl Uses {
    fn add(&mut self, id: &'static str, layer: Option<usize>, detail: impl Into<String>) {
        debug_assert!(feature_info(id).is_some(), "{id} is not in the registry");
        let e = self.0.entry(id).or_default();
        if let Some(l) = layer
            && e.0.last() != Some(&l)
        {
            e.0.push(l);
        }
        let d = detail.into();
        if !d.is_empty() && !e.1.contains(&d) {
            e.1.push(d);
        }
    }
}

fn norm_features(u: &mut Uses, n: &NormSpec, layer: Option<usize>) {
    u.add(if n.kind == NormKind::Rms { "NORM_RMS_V1" } else { "NORM_LAYER_V1" }, layer, "");
    match n.gain {
        Gain::OnePlusW => u.add("NORM_GAIN_ONE_PLUS_V1", layer, ""),
        Gain::None => u.add("NORM_UNWEIGHTED_V1", layer, ""),
        Gain::W => {}
    }
}

fn rope_features(u: &mut Uses, r: &crate::rope::RopeSpec, head_dim: usize, layer: usize) {
    let f = &r.freqs;
    let t = match f.rope_type.as_str() {
        "linear" => "ROPE_LINEAR_V1",
        "dynamic" => "ROPE_DYNAMIC_NTK_V1",
        "yarn" => "ROPE_YARN_V1",
        "llama3" => "ROPE_LLAMA3_V1",
        "longrope" | "su" => "ROPE_LONGROPE_V1",
        "proportional" => "ROPE_PROPORTIONAL_V1",
        _ => "ROPE_DEFAULT_V1",
    };
    u.add(t, Some(layer), "");
    if t != "ROPE_DEFAULT_V1" {
        u.add("ROPE_DEFAULT_V1", Some(layer), "");
    }
    if r.rotary_dim < head_dim {
        u.add("ROPE_PARTIAL_V1", Some(layer), format!("{}/{head_dim}", r.rotary_dim));
    }
    if r.style == crate::rope::RopeStyle::Interleaved {
        u.add("ROPE_INTERLEAVED_V1", Some(layer), "");
    }
    if f.mrope.is_some() {
        u.add("ROPE_MROPE_V1", Some(layer), "");
    }
}

fn detect(s: &ModelSpec) -> Vec<FeatureUse> {
    let mut u = Uses::default();
    // embedding
    let e = &s.embedding;
    u.add("EMBED_TOKEN_V1", None, format!("{} × {}", s.vocab_size, e.dim));
    if e.scale != 1.0 {
        u.add("EMBED_SCALE_V1", None, "");
    }
    if let Some(n) = &e.norm {
        u.add("EMBED_NORM_V1", None, "");
        norm_features(&mut u, n, None);
    }
    if e.positions.is_some() {
        u.add("EMBED_POSITION_LEARNED_V1", None, "");
    }
    if e.proj_in {
        u.add("EMBED_PROJ_IN_V1", None, "");
    }
    if e.type_rows.is_some() {
        u.add("EMBED_TOKEN_TYPE_V1", None, "");
    }
    if let Some(rb) = e.rel_bias {
        u.add("POS_RELATIVE_BIAS_V1", None, format!("{} buckets", rb.buckets));
    }
    if let Some(n) = &s.final_norm {
        norm_features(&mut u, n, None);
    }
    if let Some(h) = &s.hyper {
        norm_features(&mut u, &h.norm, None);
        u.add("NORM_GROUPED_V1", None, format!("{} streams", h.streams));
    }
    // layers
    for (l, ls) in s.layers.iter().enumerate() {
        let lay = Some(l);
        match &ls.mixer {
            Mixer::Attention(a) => {
                let ratio = if a.kv_heads == 0 { 0 } else { a.heads / a.kv_heads };
                u.add("ATTN_GQA_V1", lay, format!("{}q/{}kv × {} (group {ratio})", a.heads, a.kv_heads, a.head_dim));
                if let Some(w) = a.window {
                    u.add("ATTN_SLIDING_V1", lay, format!("window {w}"));
                }
                if let Some(c) = a.chunk {
                    u.add("ATTN_CHUNKED_V1", lay, format!("chunk {c}"));
                }
                if a.softcap.is_some() {
                    u.add("ATTN_SOFTCAP_V1", lay, "");
                }
                if a.sinks {
                    u.add("ATTN_SINKS_V1", lay, "");
                }
                if a.output_gate {
                    u.add("ATTN_OUTPUT_GATE_V1", lay, "");
                }
                if let Some(q) = &a.qk_norm {
                    u.add("ATTN_QK_NORM_V1", lay, format!("{:?}", q.scope));
                    norm_features(&mut u, &q.norm, lay);
                    u.add("NORM_GROUPED_V1", lay, "");
                    if a.qk_norm_after_rope {
                        u.add("ATTN_QK_NORM_POST_ROPE_V1", lay, "");
                    }
                }
                if let Some(v) = &a.v_norm {
                    u.add("ATTN_V_NORM_V1", lay, "");
                    norm_features(&mut u, &v.norm, lay);
                }
                if a.clip_qkv.is_some() {
                    u.add("ATTN_CLIP_QKV_V1", lay, "");
                }
                if a.q_temperature.is_some() {
                    u.add("ATTN_QUERY_TEMPERATURE_V1", lay, "");
                }
                if a.kv_share.is_some() {
                    u.add("ATTN_KV_SHARE_V1", lay, "");
                }
                if a.v_from_k {
                    u.add("ATTN_K_EQ_V_V1", lay, "");
                }
                if a.q_bias || a.k_bias || a.v_bias || a.o_bias {
                    u.add("ATTN_BIAS_V1", lay, "");
                }
                if let Some(sp) = &a.sparse {
                    u.add("ATTN_SPARSE_BLOCK_V1", lay, format!("block {} × top {} of {}×{} index", sp.ratio, sp.top_blocks, sp.index_heads, sp.index_dim));
                    norm_features(&mut u, &sp.q_norm, lay);
                    norm_features(&mut u, &sp.k_norm, lay);
                }
                match &a.position {
                    Position::Rope(r) => rope_features(&mut u, r, a.head_dim, l),
                    Position::Alibi(_) => u.add("POS_ALIBI_V1", lay, ""),
                    Position::None => u.add("POS_NOPE_V1", lay, ""),
                }
            }
            Mixer::Mla(m) => {
                u.add("MIXER_MLA_V1", lay, format!("{} heads, latent {}", m.heads, m.kv_lora_rank));
                rope_features(&mut u, &m.rope, m.qk_rope_head_dim, l);
                norm_features(&mut u, &m.kv_a_norm, lay);
            }
            Mixer::GatedDeltaNet(g) => {
                u.add("MIXER_GDN_V1", lay, format!("k:v heads {}:{}", g.k_heads, g.v_heads));
                u.add("CONV_DEPTHWISE_CAUSAL_V1", lay, format!("kernel {}", g.conv_kernel));
            }
            Mixer::Mamba(m) => {
                u.add("MIXER_MAMBA_V1", lay, format!("inner {} state {}", m.inner, m.state));
                u.add("CONV_DEPTHWISE_CAUSAL_V1", lay, format!("kernel {}", m.conv_kernel));
            }
            Mixer::Mamba2(m) => {
                u.add("MIXER_MAMBA2_V1", lay, format!("{} heads × {}", m.heads, m.head_dim));
                u.add("CONV_DEPTHWISE_CAUSAL_V1", lay, format!("kernel {}", m.conv_kernel));
            }
            Mixer::RwkvTime(r) => {
                u.add(if r.version == 4 { "MIXER_RWKV4_V1" } else if r.version == 7 { "MIXER_RWKV7_V1" } else { "MIXER_RWKV56_V1" }, lay, format!("RWKV-{}", r.version));
            }
        }
        let mlp = |u: &mut Uses, m: &MlpSpec| {
            u.add(if m.gated { "MLP_DENSE_GATED_V1" } else { "MLP_DENSE_PLAIN_V1" }, lay, format!("{}", m.intermediate));
            if matches!(m.glu, Glu::ClampedSwiGlu { .. }) {
                u.add("MLP_GLU_CLAMPED_V1", lay, "");
            }
            if m.up_bias || m.down_bias {
                u.add("MLP_BIAS_V1", lay, "");
            }
        };
        let moe = |u: &mut Uses, m: &MoeSpec| {
            u.add("MLP_MOE_TOPK_V1", lay, format!("E = {}, k = {}", m.experts, m.top_k));
            let r = &m.router;
            u.add(
                match r.scoring {
                    Scoring::Softmax => "MLP_MOE_ROUTER_SOFTMAX_V1",
                    Scoring::Sigmoid => "MLP_MOE_ROUTER_SIGMOID_V1",
                    Scoring::TopKThenSoftmax => "MLP_MOE_ROUTER_TOPK_SOFTMAX_V1",
                    Scoring::TopKThenSigmoid => "MLP_MOE_ROUTER_TOPK_SIGMOID_V1",
                    Scoring::SparseMixer => "MLP_MOE_ROUTER_SPARSEMIXER_V1",
                },
                lay,
                "",
            );
            if r.normalize {
                u.add("MLP_MOE_NORM_TOPK_V1", lay, "");
            }
            if r.groups.is_some() {
                u.add("MLP_MOE_GROUPED_ROUTING_V1", lay, "");
            }
            if r.selection_bias {
                u.add("MLP_MOE_SELECTION_BIAS_V1", lay, "");
            }
            if r.per_expert_scale {
                u.add("MLP_MOE_EXPERT_SCALE_V1", lay, "");
            }
            if let Some(sh) = &m.shared {
                u.add("MLP_MOE_SHARED_EXPERT_V1", lay, format!("{}", sh.intermediate));
                if sh.sigmoid_gate {
                    u.add("MLP_MOE_SHARED_GATE_V1", lay, "");
                }
            }
            if m.input_scaled {
                u.add("MLP_MOE_INPUT_SCALED_V1", lay, "");
            }
            if m.expert_bias {
                u.add("MLP_MOE_EXPERT_BIAS_V1", lay, "");
            }
            if matches!(m.glu, Glu::ClampedSwiGlu { .. }) {
                u.add("MLP_GLU_CLAMPED_V1", lay, "");
            }
        };
        match &ls.ffn {
            Ffn::None => {}
            Ffn::Mlp(m) => mlp(&mut u, m),
            Ffn::Moe(m) => moe(&mut u, m),
            Ffn::RwkvChannel(_) => u.add("FFN_RWKV_CHANNEL_V1", lay, ""),
            Ffn::MlpMoe(mm) => {
                u.add("MLP_MOE_BESIDE_DENSE_V1", lay, "");
                mlp(&mut u, &mm.mlp);
                moe(&mut u, &mm.moe);
            }
        }
        match &ls.residual {
            Residual::Sequential { pre_mixer, post_mixer, pre_ffn, post_ffn, multiplier } => {
                u.add("RESIDUAL_PRE_NORM_V1", lay, "");
                for n in [pre_mixer, post_mixer, pre_ffn, post_ffn].into_iter().flatten() {
                    norm_features(&mut u, n, lay);
                }
                if *multiplier != 1.0 {
                    u.add("RESIDUAL_SCALED_V1", lay, "");
                }
                if post_mixer.is_some() || post_ffn.is_some() {
                    u.add("RESIDUAL_SANDWICH_V1", lay, "");
                }
            }
            Residual::Parallel { norm, ffn_norm } => {
                u.add("RESIDUAL_PARALLEL_V1", lay, "");
                norm_features(&mut u, norm, lay);
                if let Some(n) = ffn_norm {
                    norm_features(&mut u, n, lay);
                }
            }
            Residual::PostNorm { mixer_norm, ffn_norm } => {
                u.add("RESIDUAL_POST_NORM_V1", lay, "");
                norm_features(&mut u, mixer_norm, lay);
                norm_features(&mut u, ffn_norm, lay);
            }
            Residual::Sandwich { pre_mixer, post_mixer, pre_ffn, post_ffn, ple, layer_scalar } => {
                u.add("RESIDUAL_SANDWICH_V1", lay, "");
                for n in [pre_mixer, post_mixer, pre_ffn, post_ffn] {
                    norm_features(&mut u, n, lay);
                }
                if let Some(p) = ple {
                    u.add("EMBED_PER_LAYER_INPUT_V1", lay, format!("{}", p.dim));
                }
                if *layer_scalar {
                    u.add("RESIDUAL_LAYER_SCALAR_V1", lay, "");
                }
            }
            Residual::HyperConnection { ple } => {
                let streams = s.hyper.as_ref().map(|h| h.streams).unwrap_or(0);
                u.add("RESIDUAL_GATED_HC_V1", lay, format!("{streams} streams"));
                if let Some(p) = ple {
                    u.add("EMBED_NGRAM_PLE_V1", lay, format!("{}-grams × {} heads", p.ngram_size, p.heads_per_ngram));
                    u.add("CONV_DEPTHWISE_CAUSAL_V1", lay, format!("kernel {} dilation {}", p.conv_kernel, p.conv_dilation));
                    norm_features(&mut u, &p.norm, lay);
                }
            }
        }
        if ls.post_scale != 1.0 {
            u.add("RESIDUAL_SCALED_V1", lay, "");
        }
    }
    // head and output
    match &s.output {
        OutputSpec::Logits => u.add("OUTPUT_LOGITS_V1", None, ""),
        OutputSpec::Embedding { .. } => u.add("OUTPUT_EMBEDDING_V1", None, ""),
    }
    let h = &s.head;
    if matches!(s.output, OutputSpec::Logits) && h.tied {
        u.add("HEAD_TIED_V1", None, "");
    }
    if h.bias {
        u.add("HEAD_BIAS_V1", None, "");
    }
    if h.softcap.is_some() {
        u.add("HEAD_SOFTCAP_V1", None, "");
    }
    if h.logit_scale != 1.0 {
        u.add("HEAD_LOGIT_SCALE_V1", None, "");
    }
    if h.pre_scale != 1.0 {
        u.add("HEAD_PRE_SCALE_V1", None, "");
    }
    if h.proj_out {
        u.add("HEAD_PROJ_OUT_V1", None, "");
    }
    // storage
    if let Some(q) = &s.hf.quant {
        u.add(
            match q.fmt {
                crate::prequant::QFormat::Gguf { .. } => "QUANT_GGUF_V1",
                crate::prequant::QFormat::Awq { .. } => "QUANT_AWQ_V1",
                _ => "QUANT_GPTQ_V1",
            },
            None,
            "",
        );
    }
    if s.adapter.is_some() {
        u.add("ADAPTER_LORA_V1", None, "");
    }
    if !s.hf.weights.is_empty() {
        let names: Vec<&str> = s.hf.weights.keys().map(String::as_str).collect();
        let shown = names.iter().take(4).copied().collect::<Vec<_>>().join(", ");
        u.add("WEIGHTS_EXPR_V1", None, format!("{} param(s) bound by expression: {shown}{}", names.len(), if names.len() > 4 { ", …" } else { "" }));
    }
    u.0.into_iter().map(|(id, (layers, details))| FeatureUse { id: FeatureId(id), layers, detail: details.join("; ") }).collect()
}

/// The features an encoder-decoder spec uses (`ENCDEC_FROM_SPEC_V1`).
pub fn encdec_features(s: &crate::lower::encdec::EncDecSpec) -> Vec<FeatureUse> {
    use crate::lower::encdec::Positions;
    let mut u = Uses::default();
    u.add("ENCDEC_FROM_SPEC_V1", None, format!("{} + {} layers of {}", s.enc_layers, s.dec_layers, s.d));
    u.add("ATTN_CROSS_ENCDEC_V1", None, "");
    u.add("EMBED_TOKEN_V1", None, format!("{} × {}", s.vocab, s.d));
    match &s.positions {
        Positions::Relative { buckets, .. } => u.add("POS_RELATIVE_BIAS_V1", None, format!("{buckets} buckets")),
        Positions::Learned { .. } => u.add("EMBED_POSITION_LEARNED_V1", None, ""),
        Positions::Sinusoidal { .. } => u.add("POS_SINUSOID_V1", None, ""),
    }
    u.add(if s.rms { "NORM_RMS_V1" } else { "NORM_LAYER_V1" }, None, "");
    u.add(if s.pre_norm { "RESIDUAL_PRE_NORM_V1" } else { "RESIDUAL_POST_NORM_V1" }, None, "");
    u.add(if s.gated { "MLP_DENSE_GATED_V1" } else { "MLP_DENSE_PLAIN_V1" }, None, "");
    if s.bias {
        u.add("ATTN_BIAS_V1", None, "");
        u.add("MLP_BIAS_V1", None, "");
    }
    if s.embed_scale != 1.0 {
        u.add("EMBED_SCALE_V1", None, "");
    }
    if s.embed_norm {
        u.add("EMBED_NORM_V1", None, "");
    }
    if s.head_scale != 1.0 {
        u.add("HEAD_PRE_SCALE_V1", None, "");
    }
    if s.logits_bias {
        u.add("HEAD_BIAS_V1", None, "");
    }
    u.add("OUTPUT_LOGITS_V1", None, "");
    u.0.into_iter().map(|(id, (layers, details))| FeatureUse { id: FeatureId(id), layers, detail: details.join("; ") }).collect()
}

impl ModelSpec {
    /// The features this model uses, by id.
    pub fn features(&self) -> Vec<FeatureUse> {
        detect(self)
    }
}
