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
    feature!("EMBED_PROJ_IN_AFTER_NORM_V1", Embedding, "a factorised embedding: positions, token types and the norm at the table's width, then a projection (with bias) to the hidden width", Implemented, [], NoReq, ["encoders::albert_matches_its_hf_fixture"], "ALBERT's embedding_size and embedding_hidden_mapping_in; OPT's project_in (EMBED_PROJ_IN_V1) comes first and has no bias."),
    feature!("ATTN_DISENTANGLED_V1", Attention, "DeBERTa's disentangled attention: content-to-position and position-to-content terms from log-bucketed relative-position embeddings projected by the layer's own key and query weights", Implemented, [], NoReq, ["encoders::deberta_v2_matches_its_hf_fixture"], "One [2*span, d] table (after an optional norm) shared by every layer; per layer pos_key = K(table) and pos_query = Q(table) (share_att_key); two [h, L, 2*span] products, gathered by a pinned [L, L] bucket table, added to the content scores under the common 1/sqrt(dh * (1 + terms)) scale. share_att_key = false and the convolution layer are not lowered."),
    feature!("VISION_FROM_SPEC_V1", Model, "a vision tower lowered from a spec: integer preprocessing, a patch projection, a rows-mode transformer, a pooled or row output", Implemented, [], NoReq, ["vision::vit_class_embedding_and_rows_match_their_hf_fixture", "vision::clip_vision_with_projection_matches_its_hf_fixture", "vision::siglip_vision_with_its_pooling_head_matches_its_hf_fixture", "vision_adapters::every_tower_adapter_reads_what_the_rust_reader_read", "vision::qwen2_vl_vision_with_its_merger_matches_its_hf_fixture", "vision::qwen2_5_vl_vision_with_windows_and_merger_matches_its_hf_fixture", "vision::llava_tower_and_projector_match_their_hf_fixture"], "One position over a fixed patch axis (lower/vision.rs). The canonical image is u8 HWC at the class's size; per-channel (x/255 - mean)/std is folded into the patch matmul, so the projection reads the pixels exactly. Adapters of kind vision instantiate the VisionSpec as data: clip-vision, siglip-vision and vit, and the towers of Qwen2-VL, Qwen2.5-VL and LLaVA (qwen2-vl-vision, qwen2-5-vl-vision, llava-vision; the first two also as -in-vlm, reading the tower inside the wrapper by match.tower_of). parse_vision_rust is their oracle (tests/vision_adapters.rs: every fixture, every real configuration, ~8,000 single-key mutants)."),
    feature!("EMBED_PATCH_CONV_V1", Embedding, "a non-overlapping patch convolution as a matmul over patchified pixels", Implemented, [], NoReq, ["vision::vit_class_embedding_and_rows_match_their_hf_fixture"], "Reshape/Transpose to patches (row-major or merge-block order) and one MatMul; the bias and the pixel normalisation are folded into the weight and bias."),
    feature!("EMBED_CLS_TOKEN_V1", Embedding, "a learned class row prepended to the patch rows", Implemented, [], NoReq, ["vision::vit_class_embedding_and_rows_match_their_hf_fixture"], "CLIP's class_embedding, ViT's cls_token (any shape holding one row)."),
    feature!("HEAD_POOL_ATTENTION_V1", Head, "SigLIP's attention-pooling head: a learned probe queries every row", Implemented, [], NoReq, ["vision::siglip_vision_with_its_pooling_head_matches_its_hf_fixture"], "The probe through W_q is data (one param), then a multihead attention, a residual MLP with its norm."),
    feature!("OUTPUT_ROWS_NORMED_V1", Head, "every row of the tower through its final LayerNorm is the output", Implemented, [], NoReq, ["vision::vit_class_embedding_and_rows_match_their_hf_fixture"], "ViT's last_hidden_state; the class row alone (CLIP's pooled output without a projection, ViT's class embedding) is the other choice."),
    feature!("EMBED_TOKEN_TYPE_V1", Embedding, "token-type embedding", Implemented, [], NoReq, ["encoders::bert_mean_pooled_and_normalised_matches_its_hf_fixture"], "BERT-style segment table (encoders)."),
    feature!("EMBED_PER_LAYER_INPUT_V1", Embedding, "per-layer input from the token", Implemented, [], NoReq, ["fidelity_tiny::gemma4"], "Gemma-4's second, per-layer embedding gating a branch of each layer."),
    feature!("EMBED_NGRAM_PLE_V1", Embedding, "hashed n-gram per-layer embedding", Implemented, ["Concat", "Compare", "Select", "Slice", "StateWrite"], NoReq, ["qwen4_exp::PLE_01_bigram", "qwen4_exp::PLE_02_trigram", "qwen4_exp::PLE_03_hash_boundary", "qwen4_exp::PLE_04_streaming_cache", "qwen4_exp::PLE_05_dilated_conv_boundary", "qwen4_exp::PLE_06_a_table_taller_than_a_chunk_is_read_by_chunks"], "Hashed bigram/trigram rows per layer (XOR of token·multiplier, prime-modulus buckets), a key per stream and a value from the rows, the streams' normed query gating the value through σ(signed √(k·q/√D)), then a dilated depthwise causal convolution (CONV_DEPTHWISE_CAUSAL_V1), added to every stream. The hash is computed IN the program, with no consensus change: bit decomposition of the n products by floor-divisions by 2^k, an XOR as the parity of a bit sum (one 0/1 triangular MatMul for every order), the recomposition as a weighted MatMul into i128, `mixed mod size` as a floor-division and a Mul in i128. MEASURED on the lowered program: 26 nodes for the ids of a trigram layer, about 10 a hash head for the table read (a block of 349 nodes at the published shape, 196 before the per-head layout). The segment window (the last n−1 tokens, an eos ending a segment) is a Fixed state of `token − eos`, so zero is a fresh sequence. A table taller than NF-8's 2^24 rows is a LOWERING matter, not a capability: it is split per hash head (each head owns a contiguous range of prime size) into one `[rows, dim]` i16 param per head at one scale per layer, each head's table cut into chunks of at most 2^24 rows, every chunk read by Gather{axis 0, batch_dims 0} of the param itself and a Select by the chunk index, so no dimension passes the cap AND a runtime can address a row by offset (lane M2's residency; `tests/row_addressing.rs`). ADVISORY, evidence for a possible one-time primitive-set extension (decided later, not required): general integer bit primitives — XOR, shifts, a wrapping multiply, a remainder — would cut the ids from ~26 nodes to ~6 (Mul, two XORs, Rem, Add, Gather)."),
    // ───────────────────────────── norms ─────────────────────────────
    feature!("NORM_RMS_V1", Norm, "RMS normalisation", Implemented, [], NoReq, ["fidelity_tiny::llama"], "x·rsqrt(mean(x²) + ε) with the exponent taken out before `IntRsqrt` (wide template)."),
    feature!("NORM_LAYER_V1", Norm, "layer normalisation", Implemented, [], NoReq, ["fidelity_tiny::gpt2"], "Centres exactly (n·x − Σx), then the RMS template."),
    feature!("NORM_GAIN_ONE_PLUS_V1", Norm, "gain stored as w, applied as 1 + w", Implemented, [], NoReq, ["fidelity_tiny::gemma"], "Gemma, Qwen3-Next, Nemotron."),
    feature!("NORM_UNWEIGHTED_V1", Norm, "norm without a gain", Implemented, [], NoReq, ["fidelity_tiny::olmo"], "OLMo-1's non-parametric LayerNorm, Gemma-4's V-norm."),
    feature!("SUBLAYER_NORMS_V1", Norm, "a norm over the attention output before o_proj and/or over the MLP's hidden activation before down_proj", Implemented, [], NoReq, ["fidelity_tiny::bitnet", "hf_fixtures::bitnet"], "BitNet b1.58's attn_sub_norm (RMSNorm over the heads' concatenated output, hidden wide) and ffn_sub_norm (RMSNorm over the gated product, intermediate wide); the sub-LN of MAGNETO/RetNet-style models. The ordinary RMS/LayerNorm lowering over rows: no node of its own kind, no primitive. BitNet's packed ternary storage with per-token int8 activation quantisation is a quant-format question; the bf16-master checkpoint lowers with the sub-norms."),
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
    feature!("ROPE_REVERSED_V1", Position, "rotation by −θ (the inverse rotate_half)", Implemented, [], NoReq, ["rope_reversed::a_reversed_rope_is_the_inverse_rotation_and_lowers"], "nanochat: [x1·cos + x2·sin, −x1·sin + x2·cos]. Stored as negated inverse frequencies, so every cos/sin table follows; refused over position-dependent or multimodal frequencies."),
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
    feature!("ATTN_OUTPUT_GATE_SEPARATE_V1", Attention, "an attention output gate from a projection of its own, per element or per head", Implemented, [], NoReq, ["attn_gate_scale::a_separate_gate_and_a_value_scale_are_features_that_lower"], "AfMoE (sigmoid per element) and Laguna (softplus per head): o *= act(gate_proj(x)); a per-head gate is repeated over the head's width (HL GroupRepeat: a Broadcast). Exclusive with the fused q_proj gate."),
    feature!("ATTN_VALUE_SCALE_V1", Attention, "a constant on the values after their projection", Implemented, [], NoReq, ["attn_gate_scale::a_separate_gate_and_a_value_scale_are_features_that_lower"], "MiMo-V2-Flash attention_value_scale: the constant joins the value narrowing's multiplier; no node of its own survives."),
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
    feature!("ATTN_TOKEN_INDEXER_V1", Attention, "DeepSeek sparse attention: the softmax runs over the top-k tokens of a learned token indexer", Implemented, ["Iota", "Compare", "ReduceSum", "ReduceMax", "Select", "Transpose"], NoReq, ["dsa::deepseek_v32_float_matches_hf", "dsa::deepseek_v32_integer_follows_float", "dsa::the_dsa_program_is_the_same_on_all_three_implementations"], "A latent-query indexer (wq_b over the MLA's q-latent, a LayerNorm'd key from the layer input, learned head weights; the first qk_rope_head_dim lanes rotated, half-split for DeepSeek-V3.2 and interleaved for GLM-MoE-DSA) scores every token; the MLA softmax keeps the min(topk, H) best, ties to the lowest index of the window. TopK needs a Fixed axis, so the selection is the exact mask kappa >= tau' of a threshold found by a 16-ary radix search by counting (B/4 reductions over H, one committed lane), then Select before the library softmax. Masked-dense: the exact function, no arithmetic saved. See lower/dsa.rs."),
    feature!("MIXER_GDN_V1", Mixer, "gated delta rule (any key:value head ratio)", Implemented, ["StateWrite", "IntLn", "Broadcast", "Compare", "Select", "Concat", "Slice", "Transpose"], NoReq, ["fidelity_tiny::qwen3_next", "fidelity_tiny::qwen3_5"], "S ← S·exp(g); S += k (β(v − Sᵀk))ᵀ per value head, the key heads mapped to the value heads by grouping."),
    feature!("MIXER_MAMBA_V1", Mixer, "Mamba-1 selective scan", Implemented, ["StateWrite", "IntLn", "Compare", "Select", "Concat", "Slice", "Transpose"], NoReq, ["fidelity_tiny::mamba", "fidelity_tiny::falcon_mamba"], "Per-channel, per-state decay."),
    feature!("MIXER_MAMBA2_V1", Mixer, "Mamba-2 state-space duality step", Implemented, ["StateWrite", "IntLn", "Broadcast", "Compare", "Select", "Concat", "Slice", "Transpose"], NoReq, ["fidelity_tiny::mamba2"], "Scalar decay per head, grouped B/C."),
    feature!("MIXER_RWKV4_V1", Mixer, "RWKV-4 time mix", Implemented, ["StateWrite", "Compare", "Select"], NoReq, ["fidelity_tiny::rwkv"], "WKV with the (num, den, max) stabilised state."),
    feature!("MIXER_RWKV56_V1", Mixer, "RWKV-5/6 time mix", Specified, [], NoReq, ["library::rwkv6_step"], "In the TIR library (`rwkv6_step`); no config lowering (remote-code references only)."),
    feature!("MIXER_RWKV7_V1", Mixer, "RWKV-7 time mix", Specified, [], NoReq, ["library::rwkv7_step"], "In the TIR library (`rwkv7_step`); no config lowering."),
    feature!("CONV_DEPTHWISE_CAUSAL_V1", Mixer, "depthwise causal convolution (any dilation)", Implemented, ["StateWrite", "Concat", "Slice", "Transpose"], NoReq, ["fidelity_tiny::qwen3_5", "fidelity_tiny::mamba", "qwen4_exp::PLE_05_dilated_conv_boundary"], "A window state of (kernel − 1)·dilation rows (zero before the sequence: the left pad); the window is state ++ row and the taps read its rows `dilation` apart through one Gather by a constant index vector (dilation 1 is the library's contiguous template)."),
    // ───────────────────────────── feed-forward ─────────────────────────────
    feature!("MLP_DENSE_GATED_V1", Ffn, "gated MLP", Implemented, [], NoReq, ["fidelity_tiny::llama"], "act(gate)·up, then down."),
    feature!("ACT_LEARNED_POINTWISE_V1", Ffn, "xIELU: a pointwise activation whose four scalars are the layer's own tensors, lowered as one 65,536-entry table per layer", Implemented, [], NoReq, ["fidelity_tiny::apertus", "hf_fixtures::apertus"], "Apertus' xIELU: y = x > 0 ? softplus(p)*x^2 + beta*x : (expm1(min(x, eps)) - x)*(beta + softplus(n)) + beta*x with p, n, beta, eps the layer's act_fn tensors. Pointwise on the activation's code, so the same Gather of a pinned i16 table as every other activation, the table made at conversion from the layer's scalars (one per layer in the stacked param). Only a dense MLP reads it; anywhere else it is refused by name."),
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
    feature!("HEAD_TRANSFORM_V1", Head, "a prediction head before the vocabulary projection: dense, activation, norm", Implemented, [], NoReq, ["head_transform::a_head_transform_is_dense_act_norm_before_the_head"], "BERT's cls.predictions.transform, ModernBERT-decoder's and RoBERTa's lm_head: h -> norm(act(dense(h))), then the (tied) vocabulary projection and its bias. Roles head.transform.dense and head.transform.norm; no new node kinds."),
    feature!("OUTPUT_LOGITS_V1", Head, "logits output", Implemented, [], NoReq, ["fidelity_tiny::llama"], "Next-token logits."),
    feature!("OUTPUT_EMBEDDING_V1", Head, "embedding output (pooled hidden row)", Implemented, [], NoReq, ["encoders::clip_text_encoder_matches_its_hf_fixture"], "RFC-0003 Embedding profile."),
    feature!("ENC_BIDIR_V1", Model, "a bidirectional encoder over a padded token axis (BERT, RoBERTa, XLM-R, DistilBERT, MPNet)", Implemented, [], NoReq, ["encoders::bert_cls_pooled_unnormalised_matches_its_hf_fixture", "encoders::mpnet_with_its_relative_bias_matches_its_hf_fixture"], "ONE position over the padded token axis L: every value is a [L, ...] tensor, attention is full over a Fixed axis with the keys at or past the count masked, pad rows never reach a real row or the pooling. Per layer kind: learned positions or a rotate_half rope table per position (ROPE_DEFAULT_V1 and friends), post-LN or pre-norm placement (a norm absent where the checkpoint has none), optional q/k/v/o and MLP biases, a plain or gated MLP, an optional final norm, optional band window; anything else is refused by name (the lowering reads a strict allow-list of the spec)."),
    feature!("ENC_BAND_WINDOW_V1", Attention, "a bidirectional sliding window: a key is visible iff |i - j| < w", Implemented, [], NoReq, ["encoders::modernbert_matches_its_hf_fixture"], "ModernBERT's local layers (local_attention = 2w - 2 of the checkpoint's own value is the adapter's arithmetic): two Iota/Compare/Select bands over the [L, L] scores, no table; the mask is part of the program, not a parameter."),
    feature!("HEAD_POOL_CLS_V1", Head, "pooling: row 0 of the encoder (the class-start token)", Implemented, [], NoReq, ["encoders::bert_cls_pooled_unnormalised_matches_its_hf_fixture"], "An option of the Embedding class (RFC-0003 section II.3), chosen by the operator or the sentence-transformers stack, not read from the model's config."),
    feature!("HEAD_POOL_MEAN_V1", Head, "pooling: the mean over the count real rows", Implemented, [], NoReq, ["encoders::bert_mean_pooled_and_normalised_matches_its_hf_fixture"], "As HEAD_POOL_CLS_V1; the mean is over the unpadded rows only."),
    feature!("HEAD_NORMALIZE_L2_V1", Head, "L2-normalised embedding row (Q30)", Implemented, [], NoReq, ["encoders::bert_mean_pooled_and_normalised_matches_its_hf_fixture"], "sentence-transformers' Normalize: the pooled row divided by its norm, the output unit exactly 2^-30."),
    feature!("OUTPUT_EMBEDDING_CLASS_V1", Model, "an encoder as the pieces of a registered Embedding class (program v2, one-stage pipeline, EmbeddingI32 row)", Implemented, [], NoReq, ["encoders::bert_mean_pooled_and_normalised_matches_its_hf_fixture"], "Lane D's `embedding::lower_bidir_embedding_v1` (RFC-0003 step 7): the pooled vector as the `Final` output of shape [1, d], the pipeline `JobTokens` over prefix, prompt, suffix padded to L, the integer parameters in the v2 program's order. End to end in misaka-palw-sdk/tests/gen_embedding_e2e.rs (registration gate, job, seat replay, court conviction)."),
    feature!("PATCH_EMBED_V1", Embedding, "a latent image as tokens: patchify by a pinned index table, a linear map, a learned position table", Implemented, [], NoReq, ["diffusers_sd3::the_sd3_tiny_pipeline_lowers_validates_and_tracks_the_float_pipeline"], "Lane D's `diffusion::embed`: x:[C,H,W] codes to [gh*gw, d] rows. Patchify is a gather, not a primitive of its own."),
    feature!("EMBED_TIMESTEP_TABLE_V1", Embedding, "a timestep (or step-position) embedding read from a pinned sinusoid table", Implemented, [], NoReq, ["diffusers_sd3::the_sd3_tiny_pipeline_lowers_validates_and_tracks_the_float_pipeline"], "The table is registration-time data indexed by the step's position among the offered step counts (PALW-EX-5)."),
    feature!("MOD_ADALN_V1", Norm, "adaLN: a LayerNorm modulated by a conditioning-derived shift and scale, and the gated residual", Implemented, [], NoReq, ["diffusers_sd3::the_sd3_tiny_pipeline_lowers_validates_and_tracks_the_float_pipeline"], "SD3/DiT conditioning. Existing norm and mul/add primitives; the modulation vectors come from a linear map of the conditioning."),
    feature!("ACT_TABLE_V1", Ffn, "SiLU and GELU-tanh as i16 tables on the code grid", Implemented, [], NoReq, ["diffusers_sd3::the_sd3_tiny_pipeline_lowers_validates_and_tracks_the_float_pipeline"], "The diffusion stack's activations as pinned tables (same lookup primitive as the decoder's activation tables)."),
    feature!("ATTN_JOINT_STREAMS_V1", Attention, "joint attention over two token streams (MMDiT): separate projections, one softmax over both", Implemented, [], NoReq, ["diffusers_sd3::the_sd3_tiny_pipeline_lowers_validates_and_tracks_the_float_pipeline"], "Committed row statistics keep every cone bounded."),
    feature!("CONV_DENSE_V1", Model, "a dense 2-D convolution as im2col by a pinned index table and one linear map", Implemented, [], NoReq, ["diffusers_sd3::the_sd3_tiny_pipeline_lowers_validates_and_tracks_the_float_pipeline", "cnn::resnet_basic_feature_map_matches_its_hf_fixture", "cnn::resnet_bottleneck_feature_map_matches_its_hf_fixture"], "conv2d as matmul: no convolution primitive; the gather table is the only addition. Two lowerers write it: diffusion/conv.rs (a latent as [C, H, W], square kernel) and lower/cnn.rs (an activation as rows [H*W, C]; stride, zero padding and dilation are the table's, a batch norm is folded into the weight)."),
    feature!("CNN_FROM_SPEC_V1", Model, "a convolutional network lowered from a spec: an integer image preprocessing, convolutions with their batch norms, pools, residual units, a feature-map or pooled output", Implemented, [], NoReq, ["cnn::resnet_basic_feature_map_matches_its_hf_fixture", "cnn::resnet_bottleneck_feature_map_matches_its_hf_fixture", "cnn::resnet_pooled_vector_matches_its_hf_fixture", "cnn::a_deep_resnet_crosses_a_block_boundary_with_a_padded_carry", "cnn::real_resnets_lower_and_are_admitted_at_224"], "lower/cnn.rs: the network is a CnnSpec (a tree of convolutions, pools, activations and residual units) that an adapter of kind cnn (resnet) instantiates as data; no network's name is in the lowering. One position over the image; an activation is rows [H*W, C] of i16 codes at a calibrated scale per site, crossing a block boundary as i32 at the residual scale in a flat zero-padded carry (a program has one carry signature and a feature map changes shape from stage to stage). The canonical image is u8 HWC at the class's size (224x224 unless the class says otherwise); the per-channel (x/255 - mean)/std is one narrowing. ResNet-18, -50 and -152 at 224x224 are admitted (3, 5 and 12 blocks; 1.8, 4.1 and 11.5 GMACs per image; cone work 935, 2,284 and 6,752 of 65,536)."),
    feature!("BN_FOLD_V1", Norm, "a batch norm folded exactly into the convolution before it", Implemented, [], NoReq, ["cnn::resnet_basic_feature_map_matches_its_hf_fixture", "cnn::a_depthwise_separable_network_with_dilation_follows_a_naive_reference"], "At inference y = gamma (x - mu)/sqrt(var + eps) + beta is W' = W gamma/sqrt(var + eps) and b' = beta - gamma mu/sqrt(var + eps) + (gamma/sqrt(var + eps)) b: the weight codes are those of the folded rows (the per-row scale absorbs the positive factor) and the bias joins the narrowing's offset, so no batch-norm node exists in a program. Exact up to the weight quantisation; training-mode statistics are not modelled."),
    feature!("CONV_DEPTHWISE_2D_V1", Mixer, "a depthwise 2-D convolution (groups = channels, any stride, padding and dilation) as a gather, a Mul against [k*k, C] and a ReduceSum over the taps", Implemented, [], NoReq, ["cnn::a_depthwise_separable_network_with_dilation_follows_a_naive_reference"], "MobileNet and ConvNeXt's spatial mixers (the causal 1-D one is CONV_DEPTHWISE_CAUSAL_V1). A grouping other than 1 and depthwise is refused by name (cnn::a_grouped_convolution_that_is_not_depthwise_is_refused_by_name)."),
    feature!("POOL_MAX_2D_V1", Mixer, "max pooling over k x k windows: the windows gathered from the rows and a padding row of the code floor, a ReduceMax over the taps", Implemented, ["ReduceMax"], NoReq, ["cnn::resnet_basic_feature_map_matches_its_hf_fixture"], "ResNet's stem pool. Exact on the codes (a max commutes with the monotone code map); the padding is minus infinity."),
    feature!("RESIDUAL_ADD_ACT_V1", Residual, "a residual unit: both branches narrowed to i32 at one calibrated scale, added exactly, the activation applied and the sum narrowed to i16 codes", Implemented, [], NoReq, ["cnn::resnet_basic_feature_map_matches_its_hf_fixture", "cnn::resnet_bottleneck_feature_map_matches_its_hf_fixture", "cnn::a_depthwise_separable_network_with_dilation_follows_a_naive_reference"], "ReLU is a clamp at 0 inside the final narrowing; any other activation is the table lookup after it. An empty shortcut is the identity, a non-empty one is any branch (ResNet's 1x1 projection)."),
    feature!("POOL_AVG_GLOBAL_V1", Head, "global average pooling: the mean over every position, exact sum and one rounded division", Implemented, [], NoReq, ["cnn::resnet_pooled_vector_matches_its_hf_fixture"], "ResNet's pooler_output; the output is [1, C] at the class's fixed point."),
    feature!("CONV_PAD_TF_SAME_V1", Mixer, "TensorFlow SAME padding: asymmetric zero padding computed from the input's extent, stride and kernel (the extra row after, not before)", Implemented, [], NoReq, ["cnn::a_mobilenet_v2_matches_its_hf_fixture"], "MobileNet v1/v2 (`tf_padding`): the window table's padding row is where a tap falls outside, with before = pad/2 and after = pad - pad/2 per axis; no node of its own."),
    feature!("LAYER_SCALE_FOLD_V1", Norm, "a learned per-channel scale after a convolution folded exactly into its weights and bias", Implemented, [], NoReq, ["cnn::a_convnext_matches_its_hf_fixture"], "ConvNeXt's layer scale (gamma per channel multiplying a pointwise convolution's output): W' = gamma W and b' = gamma b, so no multiply exists in a program."),
    feature!("ACT_CLAMP_FIXED_UNIT_V1", Ffn, "ReLU6 as a narrowing into the fixed unit 6/32767 whose clamp at 0 and at 32767 is min(max(x, 0), 6)", Implemented, [], NoReq, ["cnn::a_mobilenet_v2_matches_its_hf_fixture"], "MobileNet v1/v2: the output scale is not calibrated but fixed (the range is exactly [0, 6]); the narrowing's clamp is the activation."),
    feature!("NORM_GROUP_SPATIAL_V1", Norm, "GroupNorm over spatial positions with committed row partials", Implemented, [], NoReq, ["diffusers_sd3::the_sd3_tiny_pipeline_lowers_validates_and_tracks_the_float_pipeline"], "No cone reads a whole tensor."),
    feature!("GEN_STAGE_VAE_V1", Model, "a VAE decoder as a chain of single-position stages (resnet, mid attention, nearest x2 upsample, RGB head)", Implemented, [], NoReq, ["diffusers_sd3::the_sd3_tiny_pipeline_lowers_validates_and_tracks_the_float_pipeline"], "Image output ImageRgb8 HWC; each stage's float reference is checked."),
    // The capabilities the diffusers read path names when a component's lowering lacks them (`hf_schema::diffusers`): each is
    // expressible with the existing primitives once described; none is in the vocabulary yet.
    feature!("MIXER_SHORT_CONV_V1", Mixer, "a gated short convolution: [B | C | x] = in_proj(x), a causal depthwise convolution of B*x with no activation, out_proj of C times it", Implemented, [], NoReq, ["fidelity_tiny::lfm2", "hf_fixtures::lfm2"], "LFM2's conv layers: three linears (the rows of in_proj), a Mul, the existing causal depthwise convolution (CONV_DEPTHWISE_CAUSAL_V1, activation None), a Mul and out_proj; one flag gives biases to all three. No primitive."),
    feature!("ATTN_LOCAL_BIDIR_V1", Attention, "an encoder's self-attention restricted to the keys within a radius of the query on both sides", Implemented, ["Select"], NoReq, ["encdec::longt5_generates_through_the_two_stage_pipeline"], "LongT5's local attention: |i - j| <= local_radius (HF's block split and 3-block neighbourhood masked to |i - j| < block is this band), T5's relative bias over the true distance; a pinned [L, L] 0/1 mask and a Select to i32::MIN, like the count mask."),
    feature!("ATTN_TRANSIENT_GLOBAL_V1", Attention, "local attention plus transient global tokens (each block's tokens pooled) every query also attends to", Missing, [], NoReq, [], "LongT5's transient-global encoder attention (encoder_attention_type = transient-global)."),
    feature!("POS_ROPE_AXES_V1", Embedding, "a rotary embedding over several position axes", Missing, [], NoReq, [], "FLUX: axes_dims_rope splits the head dimension over the ids of the text tokens (zeros) and the image tokens (row, column)."),
    feature!("ATTN_QK_NORM_JOINT_V1", Attention, "an RMS norm of q and k per stream inside a joint-stream attention", Missing, [], NoReq, [], "FLUX and SD3.5's MMDiT; the decoder's ATTN_QK_NORM_V1 lowering is not wired into the diffusion block."),
    feature!("GEN_ATTN_DUAL_V1", Attention, "dual-attention layers: a second self-attention over the image stream", Missing, [], NoReq, [], "SD3.5 (dual_attention_layers)."),
    feature!("GEN_BLOCK_SINGLE_STREAM_V1", Model, "single-stream transformer blocks: attention and MLP in parallel over the concatenated streams", Missing, [], NoReq, [], "FLUX's single_transformer_blocks."),
    feature!("EMBED_GUIDANCE_V1", Embedding, "the guidance scalar's embedding added to the conditioning", Missing, [], NoReq, [], "FLUX.1-dev (guidance_embeds)."),
    feature!("EMBED_CLASS_LABEL_V1", Embedding, "a class-label embedding with an unconditional row", Missing, [], NoReq, [], "DiT (num_embeds_ada_norm)."),
    feature!("GEN_BLOCK_ADALN_ZERO_V1", Model, "single-stream transformer blocks under adaLN-Zero (self-attention and MLP)", Missing, [], NoReq, [], "DiT; MOD_ADALN_V1 is the modulation, the single-stream block is not assembled."),
    feature!("EMBED_POSITION_SINCOS_V1", Embedding, "fixed 2-D sinusoidal positions", Missing, [], NoReq, [], "DiT."),
    feature!("GEN_OUTPUT_LEARNED_SIGMA_V1", Model, "a denoiser output of twice the channels: the noise prediction and a variance", Missing, [], NoReq, [], "DiT; lower_dit_stage refuses an output of other channels than the input's."),
    feature!("GEN_SAMPLER_EPS_V1", Model, "a noise-prediction sampler (DDPM, DDIM, PNDM, Euler-discrete) with classifier-free guidance", Missing, [], NoReq, [], "DiT and the UNets; GEN_SAMPLER_AFFINE_V1 is the flow-matching velocity Euler update."),
    feature!("GEN_UNET_SKIP_V1", Model, "skip connections from a UNet's down path to its up path", Missing, [], NoReq, [], "A stage output read by a later stage: the pipeline's bindings carry it; the lowering of the graph is not written."),
    feature!("GEN_RESNET_TIME_COND_V1", Model, "a ResNet block that adds the projected timestep embedding between its convolutions", Missing, [], NoReq, [], "UNet2DConditionModel; the VAE's resnet (GEN_STAGE_VAE_V1) has no conditioning."),
    feature!("GEN_SPATIAL_TRANSFORMER_V1", Model, "a Transformer2DModel: GroupNorm, proj_in, self- and cross-attention blocks, GEGLU, proj_out", Missing, [], NoReq, [], "UNet2DConditionModel; needs ATTN_CROSS_V1."),
    feature!("EMBED_ADDITION_TEXT_TIME_V1", Embedding, "SDXL's added text and time conditioning", Missing, [], NoReq, [], "addition_embed_type = text_time."),
    feature!("GEN_VAE_LATENT_NORM_V1", Model, "a per-channel latent mean and std applied before the decode", Missing, [], NoReq, [], "AutoencoderKL variants with latents_mean / latents_std."),
    feature!("GEN_VAE_ATTN_UP_BLOCK_V1", Model, "a VAE decoder up block that is not UpDecoderBlock2D", Missing, [], NoReq, [], "AutoencoderKL variants."),
    feature!("GEN_SAMPLER_AFFINE_V1", Model, "an Euler sampler step as an integer affine update over pinned sigma tables", Implemented, [], NoReq, ["diffusers_sd3::the_sd3_tiny_pipeline_lowers_validates_and_tracks_the_float_pipeline"], "The schedule is data; the latent update is one affine map."),
    feature!("IMAGE_INIT_NOISE_V1", Model, "the initial latent noise drawn from a seed by the pinned Gaussian (PALW_GAUSS_Q24_V1)", Implemented, [], NoReq, ["diffusers_sd3::the_sd3_tiny_pipeline_lowers_validates_and_tracks_the_float_pipeline"], "Deterministic from the job's seed."),
    // ───────────────────────────── known gaps (Level C): named, not implemented ─────────────────────────────
    feature!("RESIDUAL_ALTUP_V1", Residual, "AltUp: a predicted/corrected multi-stream residual", Missing, [], NoReq, [], "Gemma-3n. Expressible with the existing primitives once described; not in the vocabulary yet."),
    feature!("RESIDUAL_LAUREL_V1", Residual, "LAuReL: a learned low-rank residual branch", Missing, [], NoReq, [], "Gemma-3n."),
    feature!("FFN_ACTIVATION_SPARSITY_V1", Ffn, "Gaussian top-k sparsity of the gate activation", Missing, [], NoReq, [], "Gemma-3n."),
    feature!("ATTN_CROSS_V1", Attention, "cross-attention to another sequence's states", Missing, [], NoReq, [], "Mllama's decoder layers reading vision states; needs a second history input."),
    feature!("ATTN_PREFIX_LM_V1", Attention, "bidirectional attention over a prompt prefix", Missing, [], NoReq, [], "PaliGemma."),
    feature!("ATTN_BLOCKSPARSE_PATTERN_V1", Attention, "a fixed block-sparse pattern of visible keys", Missing, [], NoReq, [], "Phi-3-small."),
    feature!("SCALE_MUP_V1", Residual, "muP multipliers on the embedding, branches, projection chunks and logits", Implemented, [], NoReq, ["fidelity_tiny::falcon_h1", "hf_fixtures::falcon_h1"], "Falcon-H1's multipliers, each realised by the feature that already scales that place: the embedding and the logits (EMBED_SCALE_V1, the head's logit scale), a branch's input and output (an `Op::Scale` on the branch, a change of scale key and no node of the integer program), the Mamba-2 projection's five chunks (MAMBA2_MUP_V1), the MLP's gate and down (folded exactly into the gate and down weights by the adapter's weights expressions) and the attention key (folded into the score scale)."),
    feature!("REFERENCE_REMOTE_CODE_V1", Storage, "a reference implementation that is remote Python code outside transformers", Missing, [], NoReq, [], "The semantics cannot be pinned to a library version; confirm against the module before an adapter may claim it."),
    feature!("WEIGHTS_QKV_MP_PARTITIONED_V1", Storage, "mp_num-partitioned fused qkv weight layout", Missing, [], NoReq, [], "CodeGen."),
    feature!("LAYER_FFN_ONLY_V1", Mixer, "layers that are a single block (a mixer or an FFN, not both)", Implemented, [], NoReq, ["fidelity_tiny::nemotron_h", "hf_fixtures::nemotron_h"], "Nemotron-H: each layer is Mamba-2, attention, an MLP or a MoE under ONE pre-norm and its residual add; an FFN-only layer is `x + ffn(norm(x))`, a mixer-only layer `x + mixer(norm(x))`. `Mixer::None` with an FFN; no node of its own."),
    feature!("MIXER_PARALLEL_BRANCH_V1", Mixer, "several mixers in parallel in one layer, summed", Implemented, [], NoReq, ["fidelity_tiny::falcon_h1", "hf_fixtures::falcon_h1"], "Falcon-H1: x + (ssm_out · mamba2(ssm_in · n) + attn_out · attention(attn_in · n)), n the layer's one normed input, then the layer's MLP. Each branch has its own input and output scale; a sum of the branch outputs (an Add) and nothing else."),
    feature!("MOE_EXPERTS_PLAIN_V1", Ffn, "routed experts (and the shared expert) that are plain MLPs, down(act(up(x))), with no gate projection", Implemented, [], NoReq, ["fidelity_tiny::nemotron_h", "hf_fixtures::nemotron_h"], "Nemotron-H's experts (relu^2): the lowering skips the gate projection and the product; the table over the up projection is the hidden vector."),
    feature!("MOE_LATENT_PROJ_V1", Ffn, "the routed experts run in a latent space: a projection before them and one after, the router and the shared expert on the layer's input", Implemented, [], NoReq, ["fidelity_tiny::nemotron_h_latent", "hf_fixtures::nemotron_h_latent"], "Nemotron-H's moe_latent_size (fc1_latent_proj, fc2_latent_proj): two linears around the MoE experts; the experts' width is the latent width."),
    feature!("MAMBA2_GATE_NORM_VARIANTS_V1", Mixer, "the Mamba-2 gate and RMS norm in either order, or the gate alone", Implemented, [], NoReq, ["fidelity_tiny::falcon_h1", "hf_fixtures::falcon_h1"], "norm(y*silu(z)) (Mamba-2, Nemotron-H), norm(y)*silu(z) (Falcon-H1 with mamba_norm_before_gate) or y*silu(z) with no norm (Falcon-H1 without mamba_rms_norm): the existing gated RMS norm with its order flag, or an activation and a Mul."),
    feature!("MAMBA2_MUP_V1", Mixer, "a multiplier on each of the five chunks [z | x | B | C | dt] of the Mamba-2 projection, before the convolution", Implemented, [], NoReq, ["fidelity_tiny::falcon_h1", "hf_fixtures::falcon_h1"], "Falcon-H1's mup_vector (ssm_multipliers). The depthwise convolution is channel-wise, so x, B and C run as three linears and three convolutions of their own, each on its scaled chunk: the same function, with `Op::Scale` (a scale-key change) for the multipliers."),
    feature!("ATTN_SHARED_BLOCK_V1", Attention, "one attention block's weights reused at several depths", Missing, [], NoReq, [], "Zamba2."),
    // ───────────────────────────── storage ─────────────────────────────
    feature!("QUANT_GPTQ_V1", Storage, "GPTQ-quantised projections, lowered from the stored integers", Implemented, [], NoReq, ["quantized::gptq_b4_g128_act_asym"], "Grouped integer matmul with the checkpoint's scales."),
    feature!("QUANT_AWQ_V1", Storage, "AWQ-quantised projections", Implemented, [], NoReq, ["quantized::awq_g128"], "Same, activation-aware scales."),
    feature!("QUANT_GGUF_V1", Storage, "GGUF block-quantised tensors", Implemented, [], NoReq, ["gguf::gguf_llama_q4_k_m"], "Q4_0 … Q8_0, K-quants."),
    feature!("ADAPTER_LORA_V1", Storage, "LoRA adapter over the parent (candidate = parent + adapter)", Implemented, [], NoReq, ["lora::llama_r16"], "Unmerged low-rank path."),
    feature!("ENCDEC_FROM_SPEC_V1", Model, "an encoder-decoder as data: two stages (the encoder over the padded source, the decoder with cross-attention) described by an adapter of kind `encdec`", Implemented, [], NoReq, ["encdec_adapters::the_t5_adapter_reads_what_the_rust_reader_read", "encdec_adapters::every_family_adapter_reads_what_the_rust_reader_read"], "Stage 0, the encoder, is ONE position over the padded source axis; its Final output is every decoder layer's cross-attention keys and values (one MatMul against the stacked weights). Stage 1 is the decoder's text stage. What the Rust route hard-wired per family (the five parsers and their tensor-name tables) is the adapter's data; the lowering is unchanged (Phase 1 of docs/design/palw/tir/frontend-as-data-v1.md section 3)."),
    feature!("EMBED_FRAMES_CONV1D_V1", Embedding, "feature frames through a stem of strided 1-D convolutions to the source rows (Whisper's two Conv1d with their activation), then a loaded position table", Implemented, [], NoReq, ["whisper::whisper_matches_its_hf_fixture", "cnn::a_one_dimensional_convolution_stack_follows_a_naive_reference"], "The frames are `i16` codes at a fixed 2^-13 (a normalised log-mel lies in about [-1, 1.5]); each convolution is the im2col gather of lower/cnn.rs with a kernel along the width only, the activation a table. The source is a fixed length (no count, no mask). The log-mel front end itself (STFT, mel filterbank, log) is the class's, FR-23."),
    feature!("OUTPUT_ROWS_V1", Head, "an encoder alone: its final rows are the output (i32 rows at a calibrated power-of-two unit)", Implemented, [], NoReq, ["encdec::t5_encoder_rows_match_hf"], "T5EncoderModel (the text encoder of Flux, SD3, PixArt, Wan, Sana): the stack of an encoder-decoder's encoder and no decoder; the class binds the padded source and its count."),
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
    if f.reversed {
        u.add("ROPE_REVERSED_V1", Some(layer), "");
    }
}

/// The features one mixer uses (a [`Mixer::Parallel`] recurses into its branches).
fn mixer_features(u: &mut Uses, lay: Option<usize>, l: usize, m: &Mixer) {
    match m {
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
            if let Some(g) = &a.gate {
                u.add("ATTN_OUTPUT_GATE_SEPARATE_V1", lay, format!("{:?}{}", g.act, if g.per_head { " per head" } else { "" }));
            }
            if a.v_scale != 1.0 {
                u.add("ATTN_VALUE_SCALE_V1", lay, "");
            }
            if let Some(n) = &a.o_norm {
                u.add("SUBLAYER_NORMS_V1", lay, "the attention output");
                norm_features(u, n, lay);
            }
            if let Some(q) = &a.qk_norm {
                u.add("ATTN_QK_NORM_V1", lay, format!("{:?}", q.scope));
                norm_features(u, &q.norm, lay);
                u.add("NORM_GROUPED_V1", lay, "");
                if a.qk_norm_after_rope {
                    u.add("ATTN_QK_NORM_POST_ROPE_V1", lay, "");
                }
            }
            if let Some(v) = &a.v_norm {
                u.add("ATTN_V_NORM_V1", lay, "");
                norm_features(u, &v.norm, lay);
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
                norm_features(u, &sp.q_norm, lay);
                norm_features(u, &sp.k_norm, lay);
            }
            match &a.position {
                Position::Rope(r) => rope_features(u, r, a.head_dim, l),
                Position::Alibi(_) => u.add("POS_ALIBI_V1", lay, ""),
                Position::None => u.add("POS_NOPE_V1", lay, ""),
            }
        }
        Mixer::Mla(m) => {
            u.add("MIXER_MLA_V1", lay, format!("{} heads, latent {}", m.heads, m.kv_lora_rank));
            if let Some(ix) = &m.indexer {
                u.add("ATTN_TOKEN_INDEXER_V1", lay, format!("{} heads x {}, top {}", ix.heads, ix.head_dim, ix.topk));
            }
            rope_features(u, &m.rope, m.qk_rope_head_dim, l);
            norm_features(u, &m.kv_a_norm, lay);
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
            if m.norm_mode != Mamba2Norm::GateFirst {
                u.add("MAMBA2_GATE_NORM_VARIANTS_V1", lay, format!("{:?}", m.norm_mode));
            }
            if m.chunk_scales.is_some() {
                u.add("MAMBA2_MUP_V1", lay, "");
                u.add("SCALE_MUP_V1", lay, "the projection's five chunks");
            }
        }
        Mixer::ShortConv(c) => {
            u.add("MIXER_SHORT_CONV_V1", lay, format!("kernel {}", c.kernel));
            u.add("CONV_DEPTHWISE_CAUSAL_V1", lay, format!("kernel {}", c.kernel));
        }
        Mixer::RwkvTime(r) => {
            u.add(if r.version == 4 { "MIXER_RWKV4_V1" } else if r.version == 7 { "MIXER_RWKV7_V1" } else { "MIXER_RWKV56_V1" }, lay, format!("RWKV-{}", r.version));
        }
        Mixer::None => u.add("LAYER_FFN_ONLY_V1", lay, ""),
        Mixer::Parallel(bs) => {
            u.add("MIXER_PARALLEL_BRANCH_V1", lay, format!("{} branches", bs.len()));
            for b in bs {
                if b.in_scale != 1.0 || b.out_scale != 1.0 {
                    u.add("SCALE_MUP_V1", lay, "a branch's input and output scale");
                }
                mixer_features(u, lay, l, &b.mixer);
            }
        }
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
        u.add(if e.proj_after_norm { "EMBED_PROJ_IN_AFTER_NORM_V1" } else { "EMBED_PROJ_IN_V1" }, None, "");
    }
    if e.type_rows.is_some() {
        u.add("EMBED_TOKEN_TYPE_V1", None, "");
    }
    if e.disentangled.is_some() {
        u.add("ATTN_DISENTANGLED_V1", None, "");
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
        mixer_features(&mut u, lay, l, &ls.mixer);
        let mlp = |u: &mut Uses, m: &MlpSpec| {
            u.add(if m.gated { "MLP_DENSE_GATED_V1" } else { "MLP_DENSE_PLAIN_V1" }, lay, format!("{}", m.intermediate));
            if matches!(m.glu, Glu::ClampedSwiGlu { .. }) {
                u.add("MLP_GLU_CLAMPED_V1", lay, "");
            }
            if m.up_bias || m.down_bias {
                u.add("MLP_BIAS_V1", lay, "");
            }
            if let Some(n) = &m.inner_norm {
                u.add("SUBLAYER_NORMS_V1", lay, "the MLP's hidden activation");
                norm_features(u, n, lay);
            }
            if m.act == Act::Xielu {
                u.add("ACT_LEARNED_POINTWISE_V1", lay, "xIELU");
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
            if !m.gated {
                u.add("MOE_EXPERTS_PLAIN_V1", lay, "");
            }
            if let Some(l) = m.latent {
                u.add("MOE_LATENT_PROJ_V1", lay, format!("latent {l}"));
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
        OutputSpec::Embedding { .. } => {
            u.add("OUTPUT_EMBEDDING_V1", None, "");
            // The BERT lineage: post-LN layers under an embedding output (a causal embedder — CLIP's text tower,
            // Qwen3-Embedding — is pre-norm and takes the decoder's route).
            // A pre-norm encoder (ModernBERT, EuroBERT) says so with the encoder family its adapter names.
            let encoder_family = s.families.iter().any(|f| f == "E2");
            if !s.layers.is_empty() && (encoder_family || s.layers.iter().all(|l| matches!(l.residual, Residual::PostNorm { .. }))) {
                u.add("ENC_BIDIR_V1", None, "");
                if s.layers.iter().any(|l| matches!(&l.mixer, Mixer::Attention(a) if a.window.is_some())) {
                    u.add("ENC_BAND_WINDOW_V1", None, "");
                }
            }
        }
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
    if let Some(t) = &h.transform {
        u.add("HEAD_TRANSFORM_V1", None, format!("{:?}", t.act));
        norm_features(&mut u, &t.norm, None);
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
    use crate::lower::encdec::EncInput;
    let mut u = Uses::default();
    u.add("ENCDEC_FROM_SPEC_V1", None, format!("{} + {} layers of {}", s.enc_layers, s.dec_layers, s.d));
    if !s.encoder_only() {
        u.add("ATTN_CROSS_ENCDEC_V1", None, "");
        u.add("EMBED_TOKEN_V1", None, format!("{} × {}", s.vocab, s.d));
    } else {
        u.add("OUTPUT_ROWS_V1", None, "");
        u.add("EMBED_TOKEN_V1", None, format!("{} × {}", s.vocab, s.d));
    }
    if let Some(r) = s.enc_local_radius {
        u.add("ATTN_LOCAL_BIDIR_V1", None, format!("radius {r}"));
    }
    if let EncInput::Frames { bins, frames, stem } = &s.input {
        u.add("EMBED_FRAMES_CONV1D_V1", None, format!("{frames} frames of {bins} bins through {} convolutions", stem.len()));
    }
    match &s.positions {
        Positions::Relative { buckets, .. } => u.add("POS_RELATIVE_BIAS_V1", None, format!("{buckets} buckets")),
        Positions::Learned { .. } => u.add("EMBED_POSITION_LEARNED_V1", None, ""),
        Positions::Sinusoidal { .. } => u.add("POS_SINUSOID_V1", None, ""),
    }
    u.add(if s.rms { "NORM_RMS_V1" } else { "NORM_LAYER_V1" }, None, "");
    u.add(if s.pre_norm { "RESIDUAL_PRE_NORM_V1" } else { "RESIDUAL_POST_NORM_V1" }, None, "");
    u.add(if s.gated { "MLP_DENSE_GATED_V1" } else { "MLP_DENSE_PLAIN_V1" }, None, "");
    if s.bias {
        u.add("ATTN_BIAS_V1", None, if s.k_bias { "" } else { "no bias on the keys" });
        u.add("MLP_BIAS_V1", None, "");
    }
    if s.embed_scale != 1.0 {
        u.add("EMBED_SCALE_V1", None, "");
    }
    if s.embed_norm {
        u.add("EMBED_NORM_V1", None, "");
    }
    if !s.encoder_only() {
        if s.head_scale != 1.0 {
            u.add("HEAD_PRE_SCALE_V1", None, "");
        }
        if s.logits_bias {
            u.add("HEAD_BIAS_V1", None, "");
        }
        u.add("OUTPUT_LOGITS_V1", None, "");
    }
    u.0.into_iter().map(|(id, (layers, details))| FeatureUse { id: FeatureId(id), layers, detail: details.join("; ") }).collect()
}

/// The features a vision tower uses, by id (`VISION_FROM_SPEC_V1`).
pub fn vision_features(s: &crate::lower::vision::VisionSpec) -> Vec<FeatureUse> {
    use crate::lower::vision::VisionOut;
    let mut u = Uses::default();
    u.add("VISION_FROM_SPEC_V1", None, format!("{}x{} px, patch {}, {} layers of {}", s.h, s.w, s.patch, s.layers, s.d));
    u.add("EMBED_PATCH_CONV_V1", None, format!("{} patches", s.patches()));
    if s.cls {
        u.add("EMBED_CLS_TOKEN_V1", None, "");
    }
    if s.learned_pos {
        u.add("EMBED_POSITION_LEARNED_V1", None, "");
    }
    u.add(if s.rms { "NORM_RMS_V1" } else { "NORM_LAYER_V1" }, None, "");
    u.add("RESIDUAL_PRE_NORM_V1", None, "");
    u.add(if s.swiglu { "MLP_DENSE_GATED_V1" } else { "MLP_DENSE_PLAIN_V1" }, None, "");
    match &s.out {
        VisionOut::Rows => u.add("OUTPUT_ROWS_NORMED_V1", None, ""),
        VisionOut::ClipPooled { .. } => u.add("OUTPUT_EMBEDDING_V1", None, "the class row"),
        VisionOut::SiglipHead => u.add("HEAD_POOL_ATTENTION_V1", None, ""),
        VisionOut::Merger { .. } | VisionOut::Projector { .. } => u.add("OUTPUT_EMBEDDING_V1", None, "rows"),
    }
    u.0.into_iter().map(|(id, (layers, details))| FeatureUse { id: FeatureId(id), layers, detail: details.join("; ") }).collect()
}

/// The features an `SD3Transformer2DModel` uses, by id (the denoise stage of RFC-0003 §6).
pub fn sd3_features(c: &crate::diffusion::float::Sd3Config) -> Vec<FeatureUse> {
    let mut u = Uses::default();
    u.add("PATCH_EMBED_V1", None, format!("{} channels, patch {}, grid {}x{}", c.in_channels, c.patch_size, c.grid(), c.grid()));
    u.add("EMBED_TIMESTEP_TABLE_V1", None, "the timestep sinusoid and the pooled text projection");
    u.add("MOD_ADALN_V1", None, format!("{} blocks, the last context-pre-only", c.num_layers));
    u.add("ATTN_JOINT_STREAMS_V1", None, format!("{} heads x {} over both streams", c.heads, c.head_dim));
    u.add("ACT_TABLE_V1", None, "SiLU, GELU-tanh");
    u.add("GEN_SAMPLER_AFFINE_V1", None, "the flow-matching Euler update");
    u.0.into_iter().map(|(id, (layers, details))| FeatureUse { id: FeatureId(id), layers, detail: details.join("; ") }).collect()
}

/// The features an `AutoencoderKL` decoder uses, by id.
pub fn vae_features(c: &crate::diffusion::vae_float::VaeConfig) -> Vec<FeatureUse> {
    let mut u = Uses::default();
    u.add("CONV_DENSE_V1", None, "3x3 and 1x1");
    u.add("NORM_GROUP_SPATIAL_V1", None, format!("{} groups", c.groups));
    u.add("ACT_TABLE_V1", None, "SiLU");
    u.add(
        "GEN_STAGE_VAE_V1",
        None,
        format!("{} levels, {} resnets each{}", c.block_out_channels.len(), c.layers_per_block + 1, if c.mid_attention { ", a mid attention" } else { "" }),
    );
    u.0.into_iter().map(|(id, (layers, details))| FeatureUse { id: FeatureId(id), layers, detail: details.join("; ") }).collect()
}

/// The features a convolutional network uses, by id (`CNN_FROM_SPEC_V1`).
pub fn cnn_features(s: &crate::lower::cnn::CnnSpec) -> Vec<FeatureUse> {
    use crate::lower::cnn::{CnnOp, CnnOut};
    use std::collections::BTreeSet;
    fn walk<'a>(ops: &'a [CnnOp], f: &mut impl FnMut(&'a CnnOp)) {
        for op in ops {
            f(op);
            if let CnnOp::Residual { main, shortcut, .. } = op {
                walk(main, f);
                walk(shortcut, f);
            }
        }
    }
    let mut u = Uses::default();
    let (mut n, mut kernels, mut dilated, mut strided) = (0usize, BTreeSet::new(), false, false);
    let mut acts = BTreeSet::new();
    walk(&s.ops, &mut |op| match op {
        CnnOp::Conv(c) => {
            n += 1;
            kernels.insert(c.k);
            dilated |= c.dilation > 1;
            strided |= c.stride > 1;
            if let Some(a) = c.act {
                acts.insert(format!("{a:?}"));
            }
            let kw = c.kw.unwrap_or(c.k);
            if c.groups > 1 {
                u.add("CONV_DEPTHWISE_2D_V1", None, format!("{}x{} on {} channels", c.k, kw, c.cin));
            } else {
                u.add("CONV_DENSE_V1", None, format!("{}x{}", c.k, kw));
            }
            if c.bn.is_some() {
                u.add("BN_FOLD_V1", None, "");
            }
            if c.tf_same {
                u.add("CONV_PAD_TF_SAME_V1", None, format!("{}x{} stride {}", c.k, kw, c.stride));
            }
            if c.layer_scale.is_some() {
                u.add("LAYER_SCALE_FOLD_V1", None, "");
            }
        }
        CnnOp::ChannelNorm { .. } => u.add("NORM_LAYER_V1", None, "over the channels of the feature map"),
        CnnOp::MaxPool { k, stride, .. } => u.add("POOL_MAX_2D_V1", None, format!("{k}x{k} stride {stride}")),
        CnnOp::Act(a) => {
            acts.insert(format!("{a:?}"));
        }
        CnnOp::Residual { act, .. } => {
            u.add("RESIDUAL_ADD_ACT_V1", None, "");
            if let Some(a) = act {
                acts.insert(format!("{a:?}"));
            }
        }
    });
    let mut detail = format!("{}x{} px, {n} convolutions (kernels {kernels:?})", s.h, s.w);
    if strided {
        detail.push_str(", strided");
    }
    if dilated {
        detail.push_str(", dilated");
    }
    u.add("CNN_FROM_SPEC_V1", None, detail);
    // ReLU, ReLU6 (a clamp in its fixed unit) and no activation are clamps inside a narrowing; every other activation is a table.
    let is_clamp = |a: &str| matches!(a, "Relu" | "Relu6" | "Identity");
    if acts.iter().any(|a| !is_clamp(a)) {
        u.add("ACT_TABLE_V1", None, acts.iter().filter(|a| !is_clamp(a)).cloned().collect::<Vec<_>>().join(", "));
    }
    if acts.contains("Relu6") {
        u.add("ACT_CLAMP_FIXED_UNIT_V1", None, "ReLU6");
    }
    match &s.out {
        CnnOut::Map => u.add("OUTPUT_EMBEDDING_V1", None, "the last feature map as rows"),
        CnnOut::GlobalAvg => u.add("POOL_AVG_GLOBAL_V1", None, ""),
        CnnOut::GlobalAvgNorm { .. } => {
            u.add("POOL_AVG_GLOBAL_V1", None, "");
            u.add("NORM_LAYER_V1", None, "over the pooled row");
        }
    }
    u.0.into_iter().map(|(id, (layers, details))| FeatureUse { id: FeatureId(id), layers, detail: details.join("; ") }).collect()
}

impl ModelSpec {
    /// The features this model uses, by id.
    pub fn features(&self) -> Vec<FeatureUse> {
        detect(self)
    }
}
