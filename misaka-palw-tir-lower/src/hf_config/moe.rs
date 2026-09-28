//! Mixture-of-experts decoders (C3, C8): Mixtral, Qwen2/3-MoE, OLMoE, GraniteMoE, DeepSeek-V2/V3
//! (with MLA), gpt-oss.

use super::*;
use crate::rope::RopeStyle;

fn router(scoring: Scoring, normalize: bool) -> RouterSpec {
    RouterSpec { scoring, linear_bias: false, selection_bias: false, groups: None, normalize, norm_eps: 0.0, scale: 1.0 }
}

fn moe(experts: usize, top_k: usize, inter: usize, act: Act, r: RouterSpec) -> MoeSpec {
    MoeSpec { experts, top_k, intermediate: inter, act, glu: Glu::Standard, expert_bias: false, router: r, shared: None }
}

fn check_topk(arch: &str, e: usize, k: usize) -> Result<()> {
    if k == 0 || k > e {
        return Err(LowerError::bad(format!("{arch}: top-k {k} of {e} experts")));
    }
    Ok(())
}

/// Mixtral: softmax over all experts, top-k, renormalised.
pub(crate) fn mixtral(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 32000)?;
    let hidden = p.cfg.usize_or("hidden_size", 4096)?;
    let inter = p.cfg.usize_or("intermediate_size", 14336)?;
    let n = p.cfg.usize_or("num_hidden_layers", 32)?;
    let (h, kv, hd) = heads(p, hidden, "num_attention_heads", 32, Some(8), None)?;
    let act = p.act("hidden_act", "silu")?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 4096 * 32)?;
    let eps = p.cfg.f64_or("rms_norm_eps", 1e-5)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", false)?;
    let e = p.cfg.alias_usize(&["num_local_experts", "num_experts"])?.unwrap_or(8);
    let k = p.cfg.usize_or("num_experts_per_tok", 2)?;
    check_topk(&p.arch(), e, k)?;
    let sw = p.cfg.opt_usize("sliding_window")?;
    // Jitter noise is applied only in training.
    p.cfg.inert(&["router_jitter_noise"]);
    let rope = p.rope(hd, RopeStyle::Half, Some(1_000_000.0), None, 1.0, Some(max_pos), None)?;
    let norm = NormSpec::rms(eps);
    let mut at = attn(h, kv, hd, Position::Rope(rope), (false, false));
    at.window = sw;
    let m = moe(e, k, inter, act, router(Scoring::Softmax, true));
    let layers = (0..n)
        .map(|_| LayerSpec {
            mixer: Mixer::Attention(at.clone()),
            ffn: Ffn::Moe(m.clone()),
            residual: pre_norm(norm),
            post_scale: 1.0,
        })
        .collect();
    let mut nm = llama_names("model.", if tied { "model.embed_tokens" } else { "lm_head" });
    let l = "model.layers.{L}.block_sparse_moe.";
    nm.insert("moe.router".into(), format!("{l}gate"));
    nm.insert("moe.gate".into(), format!("{l}experts.{{E}}.w1"));
    nm.insert("moe.up".into(), format!("{l}experts.{{E}}.w3"));
    nm.insert("moe.down".into(), format!("{l}experts.{{E}}.w2"));
    Ok(p.finish_spec(SpecParts {
        model_type: "mixtral",
        families: vec!["C3", "C8"],
        vocab,
        hidden,
        max_pos: Some(max_pos),
        embedding: plain_embedding(hidden),
        layers,
        final_norm: Some(norm),
        head: plain_head(tied),
        names: nm,
        prefix_aliases: vec![],
        conv1d: false,
    }))
}

/// Qwen2-MoE (shared expert with a sigmoid gate) and Qwen3-MoE (per-head QK-norm, no shared
/// expert). `mlp_only_layers` and `decoder_sparse_step` choose the dense layers.
pub(crate) fn qwen_moe(p: &mut P, v3: bool) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 151936)?;
    let hidden = p.cfg.usize_or("hidden_size", 2048)?;
    let inter = p.cfg.usize_or("intermediate_size", if v3 { 6144 } else { 5632 })?;
    let n = p.cfg.usize_or("num_hidden_layers", 24)?;
    let (h, kv, hd) = if v3 {
        heads(p, hidden, "num_attention_heads", 32, Some(4), None)?
    } else {
        heads(p, hidden, "num_attention_heads", 16, Some(16), None)?
    };
    let act = p.act("hidden_act", "silu")?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 32768)?;
    let eps = p.cfg.f64_or("rms_norm_eps", 1e-6)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", false)?;
    let e = p.cfg.alias_usize(&["num_experts", "num_local_experts"])?.unwrap_or(if v3 { 128 } else { 60 });
    let k = p.cfg.usize_or("num_experts_per_tok", if v3 { 8 } else { 4 })?;
    check_topk(&p.arch(), e, k)?;
    let moe_inter = p.cfg.usize_or("moe_intermediate_size", if v3 { 768 } else { 1408 })?;
    let norm_topk = p.cfg.bool_or("norm_topk_prob", false)?;
    let step = p.cfg.usize_or("decoder_sparse_step", 1)?;
    let mlp_only = p.cfg.opt_usize_list("mlp_only_layers")?.unwrap_or_default();
    let use_sw = p.cfg.bool_or("use_sliding_window", false)?;
    let sw_raw = p.cfg.opt_usize("sliding_window")?.filter(|w| *w > 0);
    let sw = if use_sw { sw_raw } else { None };
    let mwl = p.cfg.usize_or("max_window_layers", 28)?;
    let types = p.layer_types(n, &["full_attention", "sliding_attention"], |i| {
        if sw.is_some() && i >= mwl { "sliding_attention" } else { "full_attention" }
    })?;
    let (qkv_bias, o_bias) = if v3 {
        let b = p.cfg.bool_or("attention_bias", false)?;
        (b, b)
    } else {
        (p.cfg.bool_or("qkv_bias", true)?, false)
    };
    let shared = if v3 {
        None
    } else {
        let si = p.cfg.usize_or("shared_expert_intermediate_size", 5632)?;
        (si > 0).then_some(SharedExpertSpec { intermediate: si, sigmoid_gate: true })
    };
    let rope = p.rope(hd, RopeStyle::Half, Some(10000.0), None, 1.0, Some(max_pos), None)?;
    let norm = NormSpec::rms(eps);
    let mut m = moe(e, k, moe_inter, act, router(Scoring::Softmax, norm_topk));
    m.shared = shared;
    let layers = (0..n)
        .map(|i| {
            let mut at = attn(h, kv, hd, Position::Rope(rope.clone()), (qkv_bias, o_bias));
            at.window = window_for(&types[i], sw);
            if v3 {
                at.qk_norm = Some(QkNorm { norm, scope: QkNormScope::PerHeadShared });
            }
            let sparse = !mlp_only.contains(&i) && e > 0 && step > 0 && (i + 1) % step == 0;
            let ffn = if sparse { Ffn::Moe(m.clone()) } else { Ffn::Mlp(gated_mlp(inter, act, false)) };
            LayerSpec { mixer: Mixer::Attention(at), ffn, residual: pre_norm(norm), post_scale: 1.0 }
        })
        .collect();
    let mut nm = llama_names("model.", if tied { "model.embed_tokens" } else { "lm_head" });
    let l = "model.layers.{L}.mlp.";
    nm.insert("moe.router".into(), format!("{l}gate"));
    nm.insert("moe.gate".into(), format!("{l}experts.{{E}}.gate_proj"));
    nm.insert("moe.up".into(), format!("{l}experts.{{E}}.up_proj"));
    nm.insert("moe.down".into(), format!("{l}experts.{{E}}.down_proj"));
    nm.insert("moe.shared.gate".into(), format!("{l}shared_expert.gate_proj"));
    nm.insert("moe.shared.up".into(), format!("{l}shared_expert.up_proj"));
    nm.insert("moe.shared.down".into(), format!("{l}shared_expert.down_proj"));
    nm.insert("moe.shared_gate".into(), format!("{l}shared_expert_gate"));
    Ok(p.finish_spec(SpecParts {
        model_type: if v3 { "qwen3_moe" } else { "qwen2_moe" },
        families: vec!["C3", "C8"],
        vocab,
        hidden,
        max_pos: Some(max_pos),
        embedding: plain_embedding(hidden),
        layers,
        final_norm: Some(norm),
        head: plain_head(tied),
        names: nm,
        prefix_aliases: vec![],
        conv1d: false,
    }))
}

/// OLMoE: OLMo-2-style whole-projection QK RMSNorm, pre-norm residual, softmax top-8 of 64.
pub(crate) fn olmoe(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 50304)?;
    let hidden = p.cfg.usize_or("hidden_size", 2048)?;
    let inter = p.cfg.usize_or("intermediate_size", 2048)?;
    let n = p.cfg.usize_or("num_hidden_layers", 16)?;
    let (h, kv, hd) = heads(p, hidden, "num_attention_heads", 16, None, None)?;
    let act = p.act("hidden_act", "silu")?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 4096)?;
    let eps = p.cfg.f64_or("rms_norm_eps", 1e-5)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", false)?;
    let bias = p.cfg.bool_or("attention_bias", false)?;
    let clip = p.cfg.opt_f64("clip_qkv")?;
    let e = p.cfg.alias_usize(&["num_experts", "num_local_experts"])?.unwrap_or(64);
    let k = p.cfg.usize_or("num_experts_per_tok", 8)?;
    check_topk(&p.arch(), e, k)?;
    let norm_topk = p.cfg.bool_or("norm_topk_prob", false)?;
    let rope = p.rope(hd, RopeStyle::Half, Some(10000.0), None, 1.0, Some(max_pos), None)?;
    let norm = NormSpec::rms(eps);
    let mut at = attn(h, kv, hd, Position::Rope(rope), (bias, bias));
    at.qk_norm = Some(QkNorm { norm, scope: QkNormScope::Whole });
    at.clip_qkv = clip;
    let m = moe(e, k, inter, act, router(Scoring::Softmax, norm_topk));
    let layers = (0..n)
        .map(|_| LayerSpec {
            mixer: Mixer::Attention(at.clone()),
            ffn: Ffn::Moe(m.clone()),
            residual: pre_norm(norm),
            post_scale: 1.0,
        })
        .collect();
    let mut nm = llama_names("model.", if tied { "model.embed_tokens" } else { "lm_head" });
    let l = "model.layers.{L}.mlp.";
    nm.insert("moe.router".into(), format!("{l}gate"));
    nm.insert("moe.gate".into(), format!("{l}experts.{{E}}.gate_proj"));
    nm.insert("moe.up".into(), format!("{l}experts.{{E}}.up_proj"));
    nm.insert("moe.down".into(), format!("{l}experts.{{E}}.down_proj"));
    Ok(p.finish_spec(SpecParts {
        model_type: "olmoe",
        families: vec!["C3", "C8"],
        vocab,
        hidden,
        max_pos: Some(max_pos),
        embedding: plain_embedding(hidden),
        layers,
        final_norm: Some(norm),
        head: plain_head(tied),
        names: nm,
        prefix_aliases: vec![],
        conv1d: false,
    }))
}

/// GraniteMoE: Granite multipliers, top-k of the raw router logits then softmax over the k, fused
/// `input_linear` experts (gate rows first).
pub(crate) fn granite_moe(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 32000)?;
    let hidden = p.cfg.usize_or("hidden_size", 4096)?;
    let inter = p.cfg.usize_or("intermediate_size", 11008)?;
    let n = p.cfg.usize_or("num_hidden_layers", 32)?;
    let (h, kv, hd) = heads(p, hidden, "num_attention_heads", 32, None, None)?;
    let act = p.act("hidden_act", "silu")?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 2048)?;
    let eps = p.cfg.f64_or("rms_norm_eps", 1e-6)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", false)?;
    let bias = p.cfg.bool_or("attention_bias", false)?;
    let e = p.cfg.alias_usize(&["num_local_experts", "num_experts"])?.unwrap_or(8);
    let k = p.cfg.usize_or("num_experts_per_tok", 2)?;
    check_topk(&p.arch(), e, k)?;
    let emb_mult = p.cfg.f64_or("embedding_multiplier", 1.0)?;
    let res_mult = p.cfg.f64_or("residual_multiplier", 1.0)?;
    let attn_mult = p.cfg.f64_or("attention_multiplier", 1.0)?;
    let logits_scaling = p.cfg.f64_or("logits_scaling", 1.0)?;
    let rope = p.rope(hd, RopeStyle::Half, Some(10000.0), None, 1.0, Some(max_pos), None)?;
    let norm = NormSpec::rms(eps);
    let mut at = attn(h, kv, hd, Position::Rope(rope), (bias, bias));
    at.scale = attn_mult;
    let m = moe(e, k, inter, act, router(Scoring::TopKThenSoftmax, false));
    p.layouts.experts = MlpLayout::FusedGateFirst;
    let residual =
        Residual::Sequential { pre_mixer: Some(norm), post_mixer: None, pre_ffn: Some(norm), post_ffn: None, multiplier: res_mult };
    let layers = (0..n)
        .map(|_| LayerSpec {
            mixer: Mixer::Attention(at.clone()),
            ffn: Ffn::Moe(m.clone()),
            residual: residual.clone(),
            post_scale: 1.0,
        })
        .collect();
    let mut nm = llama_names("model.", if tied { "model.embed_tokens" } else { "lm_head" });
    let l = "model.layers.{L}.block_sparse_moe.";
    nm.insert("moe.router".into(), format!("{l}router.layer"));
    nm.insert("moe.gate_up.stacked".into(), format!("{l}input_linear.weight"));
    nm.insert("moe.down.stacked".into(), format!("{l}output_linear.weight"));
    let mut emb = plain_embedding(hidden);
    emb.scale = emb_mult;
    let mut head = plain_head(tied);
    head.logit_scale = 1.0 / logits_scaling;
    Ok(p.finish_spec(SpecParts {
        model_type: "granitemoe",
        families: vec!["C3", "C8"],
        vocab,
        hidden,
        max_pos: Some(max_pos),
        embedding: emb,
        layers,
        final_norm: Some(norm),
        head,
        names: nm,
        prefix_aliases: vec![],
        conv1d: false,
    }))
}

/// DeepSeek-V2 / V3 (and Kimi-K2, Moonlight, which ship as V3): MLA with a compressed latent KV,
/// dense first layers, routed + shared experts. V2: softmax scores, greedy or group-limited (max)
/// selection, weights × `routed_scaling_factor` and never renormalised (transformers 5 native);
/// V3: sigmoid scores, `e_score_correction_bias` for selection only, groups scored by their top-2
/// sum, `-inf` outside the chosen groups, renormalised then scaled.
pub(crate) fn deepseek(p: &mut P, v: u8) -> Result<ArchSpec> {
    let v3 = v == 3;
    let vocab = p.cfg.usize_or("vocab_size", if v3 { 129280 } else { 102400 })?;
    let hidden = p.cfg.usize_or("hidden_size", if v3 { 7168 } else { 4096 })?;
    let inter = p.cfg.usize_or("intermediate_size", if v3 { 18432 } else { 11008 })?;
    let moe_inter = p.cfg.usize_or("moe_intermediate_size", if v3 { 2048 } else { 1407 })?;
    let n = p.cfg.usize_or("num_hidden_layers", if v3 { 61 } else { 32 })?;
    let h = p.cfg.usize_or("num_attention_heads", if v3 { 128 } else { 32 })?;
    // MLA has one latent KV "head"; num_key_value_heads is carried but unused by the math.
    p.cfg.inert(&["num_key_value_heads", "head_dim", "qk_head_dim", "ep_size", "num_nextn_predict_layers", "pretraining_tp"]);
    let n_shared = p.cfg.usize_or("n_shared_experts", if v3 { 1 } else { 2 })?;
    let n_routed = p.cfg.alias_usize(&["n_routed_experts", "num_local_experts", "num_experts"])?.unwrap_or(if v3 { 256 } else { 64 });
    let rsf = p.cfg.f64_or("routed_scaling_factor", if v3 { 2.5 } else { 1.0 })?;
    let kv_lora = p.cfg.usize_or("kv_lora_rank", 512)?;
    let q_lora = p.cfg.usize_or_null("q_lora_rank", Some(1536))?;
    let rope_d = p.cfg.usize_or("qk_rope_head_dim", 64)?;
    let v_d = p.cfg.usize_or("v_head_dim", 128)?;
    let nope_d = p.cfg.usize_or("qk_nope_head_dim", 128)?;
    let k = p
        .cfg
        .opt_usize("num_experts_per_tok")?
        .or(if v3 { Some(8) } else { None })
        .ok_or_else(|| LowerError::bad("deepseek: num_experts_per_tok"))?;
    let first_dense = p.cfg.usize_or("first_k_dense_replace", if v3 { 3 } else { 0 })?;
    p.cfg.require_eq(
        "moe_layer_freq",
        &serde_json::json!(1),
        "transformers 5 native DeepSeek ignores it; the remote code honours it",
    )?;
    let act = p.act("hidden_act", "silu")?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", if v3 { 4096 } else { 2048 })?;
    let eps = p.cfg.f64_or("rms_norm_eps", 1e-6)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", false)?;
    let a_bias = p.cfg.bool_or("attention_bias", false)?;
    p.cfg.forbid("mlp_bias", "DeepSeek MLPs have no biases")?;
    check_topk(&p.arch(), n_routed, k)?;
    let (groups, scoring, normalize) = if v3 {
        match p.cfg.opt_str("topk_method")?.as_deref() {
            None | Some("noaux_tc") => {}
            Some(o) => return Err(LowerError::not_lowerable(format!("deepseek_v3: topk_method {o}"))),
        }
        match p.cfg.opt_str("scoring_func")?.as_deref() {
            None | Some("sigmoid") => {}
            Some(o) => return Err(LowerError::not_lowerable(format!("deepseek_v3: scoring_func {o}"))),
        }
        let ng = p.cfg.usize_or("n_group", 8)?;
        let tg = p.cfg.usize_or("topk_group", 4)?;
        let norm = p.cfg.bool_or("norm_topk_prob", true)?;
        (Some(GroupRouting { n_group: ng, topk_group: tg, score: GroupScore::Top2Sum }), Scoring::Sigmoid, norm)
    } else {
        match p.cfg.opt_str("scoring_func")?.as_deref() {
            None | Some("softmax") => {}
            Some(o) => return Err(LowerError::not_lowerable(format!("deepseek_v2: scoring_func {o}"))),
        }
        if p.cfg.bool_or("norm_topk_prob", false)? {
            return Err(LowerError::not_lowerable(
                "deepseek_v2: norm_topk_prob=true — the remote code renormalises, transformers 5 native never does; ambiguous reference",
            ));
        }
        let method = p.cfg.str_or("topk_method", "greedy")?;
        let ng = p.cfg.opt_usize("n_group")?;
        let tg = p.cfg.opt_usize("topk_group")?;
        let groups = match method.as_str() {
            "greedy" => None,
            "group_limited_greedy" => Some(GroupRouting {
                n_group: ng.ok_or_else(|| LowerError::bad("deepseek_v2: group_limited_greedy without n_group"))?,
                topk_group: tg.ok_or_else(|| LowerError::bad("deepseek_v2: group_limited_greedy without topk_group"))?,
                score: GroupScore::Max,
            }),
            o => return Err(LowerError::not_lowerable(format!("deepseek_v2: topk_method {o}"))),
        };
        p.cfg.inert(&["aux_loss_alpha", "seq_aux"]);
        (groups, Scoring::Softmax, false)
    };
    if let Some(g) = &groups
        && (g.n_group == 0 || n_routed % g.n_group != 0 || g.topk_group == 0 || g.topk_group > g.n_group)
    {
        return Err(LowerError::bad(format!("deepseek: {n_routed} experts in {} groups, top {}", g.n_group, g.topk_group)));
    }
    let interleave = if v3 { p.cfg.bool_or("rope_interleave", true)? } else { true };
    let qk_d = nope_d + rope_d;
    let mut rope = p.rope(
        rope_d,
        if interleave { RopeStyle::Interleaved } else { RopeStyle::Half },
        Some(10000.0),
        None,
        1.0,
        Some(max_pos),
        None,
    )?;
    rope.offset = nope_d;
    // `yarn_apply_mscale`: scaling × mscale² when the rope is not default and mscale_all_dim is set.
    let mut scale = 1.0 / (qk_d as f64).sqrt();
    let rc = crate::rope::read_rope_config(&p.cfg, Some(10000.0), None)?;
    if rc.rope_type != "default"
        && let Some(mad) = rc.params.get("mscale_all_dim").and_then(Value::as_f64)
        && mad != 0.0
    {
        let f = rc.params.get("factor").and_then(Value::as_f64).unwrap_or(1.0);
        let ms = if f <= 1.0 { 1.0 } else { 0.1 * mad * f.ln() + 1.0 };
        scale *= ms * ms;
    }
    let norm = NormSpec::rms(eps);
    let mla = MlaSpec {
        heads: h,
        q_lora_rank: q_lora,
        kv_lora_rank: kv_lora,
        qk_nope_head_dim: nope_d,
        qk_rope_head_dim: rope_d,
        v_head_dim: v_d,
        q_a_norm: norm,
        kv_a_norm: norm,
        a_bias,
        rope,
        scale,
    };
    let r = RouterSpec {
        scoring,
        linear_bias: false,
        selection_bias: v3,
        groups,
        normalize,
        norm_eps: if normalize { 1e-20 } else { 0.0 },
        scale: rsf,
    };
    let mut m = moe(n_routed, k, moe_inter, act, r);
    m.shared = (n_shared > 0).then_some(SharedExpertSpec { intermediate: moe_inter * n_shared, sigmoid_gate: false });
    let layers = (0..n)
        .map(|i| {
            let ffn = if i >= first_dense { Ffn::Moe(m.clone()) } else { Ffn::Mlp(gated_mlp(inter, act, false)) };
            LayerSpec { mixer: Mixer::Mla(mla.clone()), ffn, residual: pre_norm(norm), post_scale: 1.0 }
        })
        .collect();
    let l = "model.layers.{L}.";
    let mut nm = llama_names("model.", if tied { "model.embed_tokens" } else { "lm_head" });
    for (k2, v2) in [
        ("mla.q", "self_attn.q_proj"),
        ("mla.q_a", "self_attn.q_a_proj"),
        ("mla.q_a_norm", "self_attn.q_a_layernorm"),
        ("mla.q_b", "self_attn.q_b_proj"),
        ("mla.kv_a", "self_attn.kv_a_proj_with_mqa"),
        ("mla.kv_a_norm", "self_attn.kv_a_layernorm"),
        ("mla.kv_b", "self_attn.kv_b_proj"),
        ("mla.o", "self_attn.o_proj"),
        ("moe.router", "mlp.gate"),
        ("moe.sel_bias", "mlp.gate.e_score_correction_bias"),
        ("moe.gate", "mlp.experts.{E}.gate_proj"),
        ("moe.up", "mlp.experts.{E}.up_proj"),
        ("moe.down", "mlp.experts.{E}.down_proj"),
        ("moe.shared.gate", "mlp.shared_experts.gate_proj"),
        ("moe.shared.up", "mlp.shared_experts.up_proj"),
        ("moe.shared.down", "mlp.shared_experts.down_proj"),
    ] {
        nm.insert(k2.into(), format!("{l}{v2}"));
    }
    p.notes.push("MLA's history is the compressed latent (kv_lora_rank) plus the shared rotary key; per-head keys/values are re-expanded by kv_b_proj, as transformers 5 does".into());
    if !v3 {
        p.notes.push(
            "DeepSeek-V2 routing follows transformers 5 native: weights = scores·routed_scaling_factor, never renormalised".into(),
        );
    }
    Ok(p.finish_spec(SpecParts {
        model_type: if v3 { "deepseek_v3" } else { "deepseek_v2" },
        families: vec!["C3", "C8"],
        vocab,
        hidden,
        max_pos: Some(max_pos),
        embedding: plain_embedding(hidden),
        layers,
        final_norm: Some(norm),
        head: plain_head(tied),
        names: nm,
        prefix_aliases: vec![],
        conv1d: false,
    }))
}

/// gpt-oss: attention sinks, alternating 128-token sliding / full attention, YaRN (truncate =
/// false), biased router with softmax over the top-k logits, clamped SwiGLU experts with
/// interleaved gate/up columns stored `[E, in, out]`.
pub(crate) fn gpt_oss(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 201088)?;
    let hidden = p.cfg.usize_or("hidden_size", 2880)?;
    let inter = p.cfg.usize_or("intermediate_size", 2880)?;
    let n = p.cfg.usize_or("num_hidden_layers", 36)?;
    let (h, kv, hd) = heads(p, hidden, "num_attention_heads", 64, Some(8), Some(64))?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 131072)?;
    let eps = p.cfg.f64_or("rms_norm_eps", 1e-5)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", false)?;
    let bias = p.cfg.bool_or("attention_bias", true)?;
    let sw = p.cfg.usize_or_null("sliding_window", Some(128))?;
    let e = p.cfg.alias_usize(&["num_local_experts", "num_experts"])?.unwrap_or(128);
    let k = p.cfg.alias_usize(&["num_experts_per_tok", "experts_per_token"])?.unwrap_or(4);
    check_topk(&p.arch(), e, k)?;
    // The experts' SwiGLU is hard-coded (α = 1.702, limit = 7.0) in transformers 5; the MLP
    // activation name is never read.
    p.cfg.require_eq("swiglu_limit", &serde_json::json!(7.0), "GptOssExperts hard-codes limit = 7.0")?;
    p.cfg.inert(&["hidden_act", "initial_context_length", "rope_scaling_factor"]);
    let types =
        p.layer_types(
            n,
            &["full_attention", "sliding_attention"],
            |i| if i % 2 == 0 { "sliding_attention" } else { "full_attention" },
        )?;
    let rope = p.rope(hd, RopeStyle::Half, Some(150000.0), None, 1.0, Some(max_pos), None)?;
    let norm = NormSpec::rms(eps);
    let r = RouterSpec {
        scoring: Scoring::TopKThenSoftmax,
        linear_bias: true,
        selection_bias: false,
        groups: None,
        normalize: false,
        norm_eps: 0.0,
        scale: 1.0,
    };
    let m = MoeSpec {
        experts: e,
        top_k: k,
        intermediate: inter,
        act: Act::Silu,
        glu: Glu::ClampedSwiGlu { alpha: 1.702, limit: 7.0 },
        expert_bias: true,
        router: r,
        shared: None,
    };
    p.layouts.experts = MlpLayout::FusedInterleaved;
    let layers = (0..n)
        .map(|i| {
            let mut at = attn(h, kv, hd, Position::Rope(rope.clone()), (bias, bias));
            at.sinks = true;
            at.window = window_for(&types[i], sw);
            LayerSpec { mixer: Mixer::Attention(at), ffn: Ffn::Moe(m.clone()), residual: pre_norm(norm), post_scale: 1.0 }
        })
        .collect();
    let mut nm = llama_names("model.", if tied { "model.embed_tokens" } else { "lm_head" });
    let l = "model.layers.{L}.";
    nm.insert("attn.sinks".into(), format!("{l}self_attn.sinks"));
    nm.insert("moe.router".into(), format!("{l}mlp.router"));
    nm.insert("moe.gate_up.stacked".into(), format!("{l}mlp.experts.gate_up_proj"));
    nm.insert("moe.gate_up_bias.stacked".into(), format!("{l}mlp.experts.gate_up_proj_bias"));
    nm.insert("moe.down.stacked".into(), format!("{l}mlp.experts.down_proj"));
    nm.insert("moe.down_bias.stacked".into(), format!("{l}mlp.experts.down_proj_bias"));
    Ok(p.finish_spec(SpecParts {
        model_type: "gpt_oss",
        families: vec!["C2", "C3", "C8"],
        vocab,
        hidden,
        max_pos: Some(max_pos),
        embedding: plain_embedding(hidden),
        layers,
        final_norm: Some(norm),
        head: plain_head(tied),
        names: nm,
        prefix_aliases: vec![],
        conv1d: false,
    }))
}
