//! Linear-attention, SSM, recurrent and hybrid decoders (C4–C7): Qwen3-Next and Qwen3.5 (gated
//! delta net + gated attention), Jamba (attention + Mamba + MoE), Mamba, FalconMamba, Mamba2,
//! RWKV-4. RWKV-5/6/7 exist only as remote code / flash-linear-attention and are refused here;
//! their recurrences are still HL ops (see `crate::hl`).

use super::*;
use crate::rope::RopeStyle;

fn qwen_gdn_names(nm: &mut BTreeMap<String, String>, l: &str, fused: bool) {
    let la = format!("{l}linear_attn.");
    if fused {
        nm.insert("gdn.qkvz".into(), format!("{la}in_proj_qkvz"));
        nm.insert("gdn.ba".into(), format!("{la}in_proj_ba"));
    } else {
        nm.insert("gdn.qkv".into(), format!("{la}in_proj_qkv"));
        nm.insert("gdn.z".into(), format!("{la}in_proj_z"));
        nm.insert("gdn.b".into(), format!("{la}in_proj_b"));
        nm.insert("gdn.a".into(), format!("{la}in_proj_a"));
    }
    nm.insert("gdn.conv".into(), format!("{la}conv1d"));
    nm.insert("gdn.dt_bias".into(), format!("{la}dt_bias"));
    nm.insert("gdn.A_log".into(), format!("{la}A_log"));
    nm.insert("gdn.norm".into(), format!("{la}norm"));
    nm.insert("gdn.out".into(), format!("{la}out_proj"));
}

fn qwen_moe_names(nm: &mut BTreeMap<String, String>, l: &str) {
    let m = format!("{l}mlp.");
    nm.insert("moe.router".into(), format!("{m}gate"));
    nm.insert("moe.gate".into(), format!("{m}experts.{{E}}.gate_proj"));
    nm.insert("moe.up".into(), format!("{m}experts.{{E}}.up_proj"));
    nm.insert("moe.down".into(), format!("{m}experts.{{E}}.down_proj"));
    nm.insert("moe.shared.gate".into(), format!("{m}shared_expert.gate_proj"));
    nm.insert("moe.shared.up".into(), format!("{m}shared_expert.up_proj"));
    nm.insert("moe.shared.down".into(), format!("{m}shared_expert.down_proj"));
    nm.insert("moe.shared_gate".into(), format!("{m}shared_expert_gate"));
}

/// Shared by Qwen3-Next and Qwen3.5: the hybrid GDN + gated-attention stack with zero-centred
/// `(1+w)` RMSNorms.
struct QwenHybrid {
    vocab: usize,
    hidden: usize,
    n: usize,
    h: usize,
    kv: usize,
    hd: usize,
    act: Act,
    max_pos: usize,
    eps: f64,
    tied: bool,
    bias: bool,
    rope: RopeSpec,
    gdn: GdnSpec,
    types: Vec<String>,
}

fn qwen_hybrid(p: &mut P, d_vocab: usize, d_hidden: usize, d_layers: usize, d_kv: usize, layout: GdnLayout) -> Result<QwenHybrid> {
    let vocab = p.cfg.usize_or("vocab_size", d_vocab)?;
    let hidden = p.cfg.usize_or("hidden_size", d_hidden)?;
    let n = p.cfg.usize_or("num_hidden_layers", d_layers)?;
    let (h, kv, hd) = heads(p, hidden, "num_attention_heads", 16, Some(d_kv), Some(256))?;
    let act = p.act("hidden_act", "silu")?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 32768)?;
    let eps = p.cfg.f64_or("rms_norm_eps", 1e-6)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", false)?;
    let bias = p.cfg.bool_or("attention_bias", false)?;
    let partial = match p.cfg.opt_obj("rope_parameters")?.and_then(|r| r.get("partial_rotary_factor")).and_then(Value::as_f64) {
        Some(v) => {
            if let Some(t) = p.cfg.opt_f64("partial_rotary_factor")?
                && (t - v).abs() > 1e-12
            {
                return Err(LowerError::not_lowerable("partial_rotary_factor disagrees with rope_parameters"));
            }
            v
        }
        None => p.cfg.f64_or("partial_rotary_factor", 0.25)?,
    };
    let rd = (hd as f64 * partial) as usize;
    let rope = p.rope(rd, RopeStyle::Half, Some(10000.0), None, partial, Some(max_pos), None)?;
    let interval = p.cfg.opt_usize("full_attention_interval")?.unwrap_or(4);
    let raw_types = p.layer_types(n, &["full_attention", "linear_attention", "attention", "mamba"], |i| {
        if (i + 1) % interval != 0 { "linear_attention" } else { "full_attention" }
    })?;
    let types: Vec<String> = raw_types
        .into_iter()
        .map(|t| match t.as_str() {
            "attention" => "full_attention".to_string(),
            "mamba" => "linear_attention".to_string(),
            _ => t,
        })
        .collect();
    let k_heads = p.cfg.usize_or("linear_num_key_heads", 16)?;
    let v_heads = p.cfg.usize_or("linear_num_value_heads", 32)?;
    let k_dim = p.cfg.usize_or("linear_key_head_dim", 128)?;
    let v_dim = p.cfg.usize_or("linear_value_head_dim", 128)?;
    let conv = p.cfg.usize_or("linear_conv_kernel_dim", 4)?;
    if k_heads == 0 || v_heads % k_heads != 0 {
        return Err(LowerError::bad(format!("{}: {v_heads} value heads over {k_heads} key heads", p.cfg.arch)));
    }
    // Multi-token-prediction heads are a separate module the next-token forward never runs.
    p.cfg.inert(&["mtp_num_hidden_layers", "mtp_use_dedicated_embeddings", "num_nextn_predict_layers"]);
    p.ignored_prefixes.push("mtp.".into());
    p.layouts.gdn = layout;
    let gdn = GdnSpec { k_heads, v_heads, k_dim, v_dim, conv_kernel: conv, head_map: HeadMap::Group, norm_eps: eps, l2_eps: 1e-6 };
    Ok(QwenHybrid { vocab, hidden, n, h, kv, hd, act, max_pos, eps, tied, bias, rope, gdn, types })
}

fn qwen_hybrid_layer(q: &QwenHybrid, i: usize, ffn: Ffn) -> LayerSpec {
    let norm = NormSpec::rms_1p(q.eps);
    let mixer = if q.types[i] == "full_attention" {
        let mut at = attn(q.h, q.kv, q.hd, Position::Rope(q.rope.clone()), (q.bias, q.bias));
        at.output_gate = true;
        at.qk_norm = Some(QkNorm { norm, scope: QkNormScope::PerHeadShared });
        Mixer::Attention(at)
    } else {
        Mixer::GatedDeltaNet(q.gdn.clone())
    };
    LayerSpec { mixer, ffn, residual: pre_norm(norm), post_scale: 1.0 }
}

/// Qwen3-Next: 3 GDN layers : 1 gated-attention layer, MoE with a sigmoid-gated shared expert.
pub(crate) fn qwen3_next(p: &mut P) -> Result<ArchSpec> {
    let q = qwen_hybrid(p, 151936, 2048, 48, 2, GdnLayout::FusedPerKeyHead)?;
    let inter = p.cfg.usize_or("intermediate_size", 5632)?;
    let e = p.cfg.alias_usize(&["num_experts", "num_local_experts"])?.unwrap_or(512);
    let k = p.cfg.usize_or("num_experts_per_tok", 10)?;
    let moe_inter = p.cfg.usize_or("moe_intermediate_size", 512)?;
    let shared = p.cfg.usize_or("shared_expert_intermediate_size", 512)?;
    let norm_topk = p.cfg.bool_or("norm_topk_prob", true)?;
    let step = p.cfg.usize_or("decoder_sparse_step", 1)?;
    let mlp_only = p.cfg.opt_usize_list("mlp_only_layers")?.unwrap_or_default();
    p.cfg.forbid("use_sliding_window", "Qwen3-Next has no sliding-window attention")?;
    if k == 0 || k > e.max(1) {
        return Err(LowerError::bad("qwen3_next: top-k"));
    }
    let m = MoeSpec {
        experts: e,
        top_k: k,
        intermediate: moe_inter,
        act: q.act,
        glu: Glu::Standard,
        expert_bias: false,
        router: RouterSpec {
            scoring: Scoring::Softmax,
            linear_bias: false,
            selection_bias: false,
            groups: None,
            normalize: norm_topk,
            norm_eps: 0.0,
            scale: 1.0,
            jitter_eps: 0.0,
        },
        shared: (shared > 0).then_some(SharedExpertSpec { intermediate: shared, sigmoid_gate: true }),
        input_scaled: false,
    };
    let layers = (0..q.n)
        .map(|i| {
            let sparse = !mlp_only.contains(&i) && e > 0 && step > 0 && (i + 1) % step == 0;
            let ffn = if sparse { Ffn::Moe(m.clone()) } else { Ffn::Mlp(gated_mlp(inter, q.act, false)) };
            qwen_hybrid_layer(&q, i, ffn)
        })
        .collect();
    let mut nm = llama_names("model.", if q.tied { "model.embed_tokens" } else { "lm_head" });
    qwen_gdn_names(&mut nm, "model.layers.{L}.", true);
    qwen_moe_names(&mut nm, "model.layers.{L}.");
    p.notes.push(
        "GDN value head vh reads key head vh / (v_heads/k_heads) (repeat_interleave), not the live Q36 kernel's vh % k_heads".into(),
    );
    Ok(p.finish_spec(SpecParts {
        model_type: "qwen3_next",
        families: vec!["C3", "C4", "C7"],
        vocab: q.vocab,
        hidden: q.hidden,
        max_pos: Some(q.max_pos),
        embedding: plain_embedding(q.hidden),
        layers,
        final_norm: Some(NormSpec::rms_1p(q.eps)),
        head: plain_head(q.tied),
        names: nm,
        prefix_aliases: vec![],
        conv1d: false,
    }))
}

/// Qwen3.5 text decoder (dense or MoE): Qwen3-Next's hybrid with split GDN projections; the MoE
/// always renormalises its top-k.
pub(crate) fn qwen3_5_text(
    p: &mut P,
    moe_variant: bool,
    model: &str,
    lm_head: &str,
    aliases: Vec<(String, String)>,
) -> Result<ArchSpec> {
    let q = if moe_variant {
        qwen_hybrid(p, 248320, 2048, 40, 2, GdnLayout::Split)?
    } else {
        qwen_hybrid(p, 248320, 4096, 32, 4, GdnLayout::Split)?
    };
    // Keys the released Qwen3.5 configs carry that transformers 5.17 does not read:
    // `mlp_only_layers` (deleted by Qwen3_5(Moe)TextConfig's __post_init__), `mamba_ssm_dtype` (the
    // authors' kernel's state dtype; the f32 reference keeps the state in f32); and
    // `attn_output_gate`, which only agrees with the reference as `true` (its attention always
    // gates its output).
    p.cfg.inert(&["mlp_only_layers", "mamba_ssm_dtype"]);
    p.cfg.require_eq("attn_output_gate", &serde_json::json!(true), "transformers' Qwen3.5 attention always gates its output")?;
    let ffn = if moe_variant {
        let e = p.cfg.alias_usize(&["num_experts", "num_local_experts"])?.unwrap_or(256);
        let k = p.cfg.usize_or("num_experts_per_tok", 8)?;
        let moe_inter = p.cfg.usize_or("moe_intermediate_size", 512)?;
        let shared = p.cfg.usize_or("shared_expert_intermediate_size", 512)?;
        if k == 0 || k > e {
            return Err(LowerError::bad("qwen3_5_moe: top-k"));
        }
        Ffn::Moe(MoeSpec {
            experts: e,
            top_k: k,
            intermediate: moe_inter,
            act: q.act,
            glu: Glu::Standard,
            expert_bias: false,
            router: RouterSpec {
                scoring: Scoring::Softmax,
                linear_bias: false,
                selection_bias: false,
                groups: None,
                normalize: true,
                norm_eps: 0.0,
                scale: 1.0,
                jitter_eps: 0.0,
            },
            shared: (shared > 0).then_some(SharedExpertSpec { intermediate: shared, sigmoid_gate: true }),
            input_scaled: false,
        })
    } else {
        Ffn::Mlp(gated_mlp(p.cfg.usize_or("intermediate_size", 12288)?, q.act, false))
    };
    let layers = (0..q.n).map(|i| qwen_hybrid_layer(&q, i, ffn.clone())).collect();
    let emb_name = format!("{model}embed_tokens");
    let mut nm = llama_names(model, if q.tied { &emb_name } else { lm_head });
    let l = format!("{model}layers.{{L}}.");
    qwen_gdn_names(&mut nm, &l, false);
    if moe_variant {
        qwen_moe_names(&mut nm, &l);
    }
    p.notes
        .push("text-only positions: the multimodal rope (mrope) reduces to plain rope when the three position ids are equal".into());
    p.notes.push("GDN value head vh reads key head vh / (v_heads/k_heads) (repeat_interleave)".into());
    Ok(p.finish_spec(SpecParts {
        model_type: if moe_variant { "qwen3_5_moe_text" } else { "qwen3_5_text" },
        families: if moe_variant { vec!["C3", "C4", "C7"] } else { vec!["C4", "C7"] },
        vocab: q.vocab,
        hidden: q.hidden,
        max_pos: Some(q.max_pos),
        embedding: plain_embedding(q.hidden),
        layers,
        final_norm: Some(NormSpec::rms_1p(q.eps)),
        head: plain_head(q.tied),
        names: nm,
        prefix_aliases: aliases,
        conv1d: false,
    }))
}

/// Jamba: attention every `attn_layer_period` (NoPE), Mamba-1 elsewhere with RMS-normalised
/// `dt`/`B`/`C`, MoE every `expert_layer_period` (softmax top-k, NOT renormalised).
pub(crate) fn jamba(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 65536)?;
    let hidden = p.cfg.usize_or("hidden_size", 4096)?;
    let inter = p.cfg.usize_or("intermediate_size", 14336)?;
    let n = p.cfg.usize_or("num_hidden_layers", 32)?;
    let (h, kv, hd) = heads(p, hidden, "num_attention_heads", 32, Some(8), None)?;
    let act = p.act("hidden_act", "silu")?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 262144)?;
    let eps = p.cfg.f64_or("rms_norm_eps", 1e-6)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", false)?;
    let e = p.cfg.alias_usize(&["num_experts", "num_local_experts"])?.unwrap_or(16);
    let k = p.cfg.usize_or("num_experts_per_tok", 2)?;
    let ap = p.cfg.usize_or("attn_layer_period", 8)?;
    let ao = p.cfg.usize_or("attn_layer_offset", 4)?;
    let ep = p.cfg.usize_or("expert_layer_period", 2)?;
    let eo = p.cfg.usize_or("expert_layer_offset", 1)?;
    let d_state = p.cfg.usize_or("mamba_d_state", 16)?;
    let d_conv = p.cfg.usize_or("mamba_d_conv", 4)?;
    let expand = p.cfg.usize_or("mamba_expand", 2)?;
    let dt_rank = match p.cfg.raw("mamba_dt_rank") {
        Some(Value::String(s)) if s == "auto" => hidden.div_ceil(16),
        Some(v) => v.as_u64().ok_or_else(|| LowerError::bad("jamba: mamba_dt_rank"))? as usize,
        None => hidden.div_ceil(16),
    };
    let conv_bias = p.cfg.bool_or("mamba_conv_bias", true)?;
    let proj_bias = p.cfg.bool_or("mamba_proj_bias", false)?;
    let sw = p.cfg.opt_usize("sliding_window")?;
    // Kernel choice and the logits-to-keep optimisation do not change the function.
    p.cfg.inert(&["use_mamba_kernels", "use_associative_scan", "use_mambapy", "num_logits_to_keep"]);
    if ap == 0 || ep == 0 || k == 0 || k > e {
        return Err(LowerError::bad("jamba: periods/top-k"));
    }
    let norm = NormSpec::rms(eps);
    let mamba = MambaSpec {
        inner: expand * hidden,
        state: d_state,
        conv_kernel: d_conv,
        dt_rank,
        conv_bias,
        proj_bias,
        bcdt_norm: Some(norm),
    };
    let m = MoeSpec {
        experts: e,
        top_k: k,
        intermediate: inter,
        act,
        glu: Glu::Standard,
        expert_bias: false,
        router: RouterSpec {
            scoring: Scoring::Softmax,
            linear_bias: false,
            selection_bias: false,
            groups: None,
            normalize: false,
            norm_eps: 0.0,
            scale: 1.0,
            jitter_eps: 0.0,
        },
        shared: None,
        input_scaled: false,
    };
    let layers = (0..n)
        .map(|i| {
            let mixer = if i % ap == ao {
                let mut at = attn(h, kv, hd, Position::None, (false, false));
                at.window = sw;
                Mixer::Attention(at)
            } else {
                Mixer::Mamba(mamba.clone())
            };
            let ffn = if i % ep == eo { Ffn::Moe(m.clone()) } else { Ffn::Mlp(gated_mlp(inter, act, false)) };
            LayerSpec { mixer, ffn, residual: pre_norm(norm), post_scale: 1.0 }
        })
        .collect();
    let l = "model.layers.{L}.";
    let mut nm = llama_names("model.", if tied { "model.embed_tokens" } else { "lm_head" });
    nm.insert("final_norm".into(), "model.final_layernorm".into());
    nm.insert("norm.ffn".into(), format!("{l}pre_ff_layernorm"));
    for (k2, v2) in [
        ("mlp.gate", "feed_forward.gate_proj"),
        ("mlp.up", "feed_forward.up_proj"),
        ("mlp.down", "feed_forward.down_proj"),
        ("moe.router", "feed_forward.router"),
        ("moe.gate", "feed_forward.experts.{E}.gate_proj"),
        ("moe.up", "feed_forward.experts.{E}.up_proj"),
        ("moe.down", "feed_forward.experts.{E}.down_proj"),
        ("mamba.in", "mamba.in_proj"),
        ("mamba.conv", "mamba.conv1d"),
        ("mamba.x", "mamba.x_proj"),
        ("mamba.dt", "mamba.dt_proj"),
        ("mamba.A_log", "mamba.A_log"),
        ("mamba.D", "mamba.D"),
        ("mamba.out", "mamba.out_proj"),
        ("mamba.dt_norm", "mamba.dt_layernorm"),
        ("mamba.b_norm", "mamba.b_layernorm"),
        ("mamba.c_norm", "mamba.c_layernorm"),
    ] {
        nm.insert(k2.into(), format!("{l}{v2}"));
    }
    Ok(p.finish_spec(SpecParts {
        model_type: "jamba",
        families: vec!["C3", "C5", "C7"],
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

fn mamba_names(nm: &mut BTreeMap<String, String>) {
    let l = "backbone.layers.{L}.";
    for (k, v) in [
        ("embed", "backbone.embeddings".to_string()),
        ("final_norm", "backbone.norm_f".to_string()),
        ("norm.mix", format!("{l}norm")),
        ("mamba.in", format!("{l}mixer.in_proj")),
        ("mamba.conv", format!("{l}mixer.conv1d")),
        ("mamba.x", format!("{l}mixer.x_proj")),
        ("mamba.dt", format!("{l}mixer.dt_proj")),
        ("mamba.A_log", format!("{l}mixer.A_log")),
        ("mamba.D", format!("{l}mixer.D")),
        ("mamba.out", format!("{l}mixer.out_proj")),
        ("mamba2.in", format!("{l}mixer.in_proj")),
        ("mamba2.conv", format!("{l}mixer.conv1d")),
        ("mamba2.dt_bias", format!("{l}mixer.dt_bias")),
        ("mamba2.A_log", format!("{l}mixer.A_log")),
        ("mamba2.D", format!("{l}mixer.D")),
        ("mamba2.norm", format!("{l}mixer.norm")),
        ("mamba2.out", format!("{l}mixer.out_proj")),
    ] {
        nm.insert(k.to_string(), v);
    }
}

fn time_step_rank(p: &P, hidden: usize) -> Result<usize> {
    Ok(match p.cfg.raw("time_step_rank") {
        Some(Value::String(s)) if s == "auto" => hidden.div_ceil(16),
        Some(v) => v.as_u64().ok_or_else(|| LowerError::bad("mamba: time_step_rank"))? as usize,
        None => hidden.div_ceil(16),
    })
}

/// Mamba-1 and FalconMamba (which adds weightless RMS norms on `dt`, `B`, `C`).
pub(crate) fn mamba(p: &mut P, falcon: bool) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 50280)?;
    let hidden = p.cfg.usize_or("hidden_size", 768)?;
    let n = p.cfg.usize_or("num_hidden_layers", 32)?;
    let state = p.cfg.usize_or("state_size", 16)?;
    let expand = p.cfg.usize_or("expand", 2)?;
    let conv = p.cfg.usize_or("conv_kernel", 4)?;
    let eps = p.cfg.f64_or("layer_norm_epsilon", 1e-5)?;
    let bias = p.cfg.bool_or("use_bias", false)?;
    let conv_bias = p.cfg.bool_or("use_conv_bias", true)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", true)?;
    let act = p.cfg.str_or("hidden_act", "silu")?;
    if act != "silu" && act != "swish" {
        return Err(LowerError::not_lowerable(format!("mamba: hidden_act {act}")));
    }
    // Keys the -hf conversions carried over from mamba_ssm: `n_layer` (an alias that must agree),
    // `fused_add_norm` (a kernel choice), `pad_vocab_size_multiple` (training-time padding, already
    // in vocab_size) and `rms_norm` (MambaRMSNorm is the only norm transformers implements).
    if let Some(nl) = p.cfg.opt_usize("n_layer")?
        && nl != n
    {
        return Err(LowerError::bad(format!("mamba: n_layer {nl} ≠ num_hidden_layers {n}")));
    }
    p.cfg.inert(&["fused_add_norm", "pad_vocab_size_multiple"]);
    p.cfg.require_eq("rms_norm", &serde_json::json!(true), "transformers' Mamba always uses RMSNorm")?;
    // mamba_ssm's own names, which the -hf conversions also carry and transformers 5.17 reads none
    // of: `d_model` (must agree with hidden_size), `dt_rank` (`time_step_rank` is what is read),
    // `d_inner` (not the inner width — state-spaces/mamba-370m-hf says 160 for 2,048) and `ssm_cfg`
    // (must be empty: a mamba_ssm layer choice the reference would not honour).
    if let Some(dm) = p.cfg.opt_usize("d_model")?
        && dm != hidden
    {
        return Err(LowerError::bad(format!("mamba: d_model {dm} ≠ hidden_size {hidden}")));
    }
    p.cfg.inert(&["d_inner", "dt_rank"]);
    p.cfg.forbid("ssm_cfg", "a mamba_ssm layer configuration transformers does not read")?;
    let dt_rank = time_step_rank(p, hidden)?;
    let inner = expand * hidden;
    if let Some(i) = p.cfg.opt_usize("intermediate_size")?
        && i != inner
    {
        return Err(LowerError::bad(format!("mamba: intermediate_size {i} ≠ expand·hidden {inner}")));
    }
    // Init-only and kernel-selection keys; `residual_in_fp32` only changes the dtype of a residual
    // that the f32 reference already keeps in f32.
    p.cfg.inert(&[
        "time_step_scale",
        "time_step_min",
        "time_step_max",
        "time_step_init_scheme",
        "time_step_floor",
        "rescale_prenorm_residual",
        "use_mambapy",
        "use_associative_scan",
        "residual_in_fp32",
        "use_falcon_mambapy",
    ]);
    let bcdt_norm = if falcon {
        let e = p.cfg.f64_or("mixer_rms_eps", 1e-6)?;
        Some(NormSpec { kind: NormKind::Rms, eps: e, gain: Gain::None, bias: false })
    } else {
        None
    };
    let spec = MambaSpec { inner, state, conv_kernel: conv, dt_rank, conv_bias, proj_bias: bias, bcdt_norm };
    let norm = NormSpec::rms(eps);
    let layers = (0..n)
        .map(|_| LayerSpec {
            mixer: Mixer::Mamba(spec.clone()),
            ffn: Ffn::None,
            residual: Residual::Sequential { pre_mixer: Some(norm), post_mixer: None, pre_ffn: None, post_ffn: None, multiplier: 1.0 },
            post_scale: 1.0,
        })
        .collect();
    let mut nm = BTreeMap::new();
    mamba_names(&mut nm);
    nm.insert("lm_head".into(), if tied { "backbone.embeddings".into() } else { "lm_head".into() });
    Ok(p.finish_spec(SpecParts {
        model_type: if falcon { "falcon_mamba" } else { "mamba" },
        families: vec!["C5"],
        vocab,
        hidden,
        max_pos: None,
        embedding: plain_embedding(hidden),
        layers,
        final_norm: Some(norm),
        head: plain_head(tied),
        names: nm,
        prefix_aliases: vec![],
        conv1d: false,
    }))
}

/// Mamba-2: SSD heads with grouped B/C, gate applied BEFORE a full-width RMSNorm.
pub(crate) fn mamba2(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 32768)?;
    let hidden = p.cfg.usize_or("hidden_size", 4096)?;
    let n = p.cfg.usize_or("num_hidden_layers", 64)?;
    let heads_n = p.cfg.usize_or("num_heads", 128)?;
    let head_dim = p.cfg.usize_or("head_dim", 64)?;
    let state = p.cfg.usize_or("state_size", 128)?;
    let expand = p.cfg.usize_or("expand", 2)?;
    let conv = p.cfg.usize_or("conv_kernel", 4)?;
    let groups = p.cfg.usize_or("n_groups", 8)?;
    let eps = p.cfg.f64_or("layer_norm_epsilon", 1e-5)?;
    let bias = p.cfg.bool_or("use_bias", false)?;
    let conv_bias = p.cfg.bool_or("use_conv_bias", true)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", false)?;
    let act = p.cfg.str_or("hidden_act", "silu")?;
    if act != "silu" && act != "swish" {
        return Err(LowerError::not_lowerable(format!("mamba2: hidden_act {act}")));
    }
    p.cfg.require_eq("rms_norm", &serde_json::json!(true), "Mamba2Mixer always applies the gated RMSNorm")?;
    // transformers 5 always multiplies by the gate BEFORE the norm (mamba_ssm's
    // norm_before_gate=False) and never reads this key.
    p.cfg.forbid("norm_before_gate", "transformers' Mamba2 gates before the norm")?;
    p.cfg.inert(&["time_step_init_scheme", "time_step_scale"]);
    let (dt_min, dt_max) = match p.cfg.opt_list("time_step_limit")? {
        None => (0.0, f64::INFINITY),
        Some(l) if l.len() == 2 => {
            let lo = l[0].as_f64().unwrap_or(0.0);
            let hi = l[1].as_f64().unwrap_or(f64::INFINITY);
            (lo, if hi >= 1e300 { f64::INFINITY } else { hi })
        }
        Some(_) => return Err(LowerError::bad("mamba2: time_step_limit")),
    };
    if dt_min > 0.0 || dt_max.is_finite() {
        // transformers 5 clamps dt in the chunked prefill path but not in the one-token decode
        // step; a non-default limit makes the two disagree, so there is no single reference.
        return Err(LowerError::not_lowerable("mamba2: non-default time_step_limit (HF's prefill and decode paths disagree)"));
    }
    p.cfg.inert(&[
        "time_step_rank",
        "time_step_min",
        "time_step_max",
        "time_step_floor",
        "rescale_prenorm_residual",
        "residual_in_fp32",
        "chunk_size",
        "use_associative_scan",
    ]);
    let inner = expand * hidden;
    if heads_n * head_dim != inner {
        return Err(LowerError::bad(format!("mamba2: num_heads·head_dim {} ≠ expand·hidden {inner}", heads_n * head_dim)));
    }
    if groups == 0 || heads_n % groups != 0 {
        return Err(LowerError::bad("mamba2: heads not a multiple of n_groups"));
    }
    let spec = Mamba2Spec {
        heads: heads_n,
        head_dim,
        groups,
        state,
        conv_kernel: conv,
        conv_bias,
        proj_bias: bias,
        norm_eps: eps,
        norm_groups: 1,
        dt_min,
        dt_max,
        d_mlp: 0,
    };
    let norm = NormSpec::rms(eps);
    let layers = (0..n)
        .map(|_| LayerSpec {
            mixer: Mixer::Mamba2(spec.clone()),
            ffn: Ffn::None,
            residual: Residual::Sequential { pre_mixer: Some(norm), post_mixer: None, pre_ffn: None, post_ffn: None, multiplier: 1.0 },
            post_scale: 1.0,
        })
        .collect();
    let mut nm = BTreeMap::new();
    mamba_names(&mut nm);
    nm.insert("lm_head".into(), if tied { "backbone.embeddings".into() } else { "lm_head".into() });
    p.notes.push("gated RMSNorm over the full inner width (transformers 5 MambaRMSNormGated), not per group as mamba_ssm's RMSNormGated(group_size = d_ssm/ngroups)".into());
    Ok(p.finish_spec(SpecParts {
        model_type: "mamba2",
        families: vec!["C5"],
        vocab,
        hidden,
        max_pos: None,
        embedding: plain_embedding(hidden),
        layers,
        final_norm: Some(norm),
        head: plain_head(tied),
        names: nm,
        prefix_aliases: vec![],
        conv1d: false,
    }))
}

/// RWKV-4 (`RwkvForCausalLM`, the only RWKV in transformers): token shift, the (num, den, max)
/// WKV recurrence, squared-ReLU channel mix, and inference-time `rescale_every` (hidden halved
/// every N blocks, output weights pre-divided by `2^(i // N)`).
pub(crate) fn rwkv4(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 50277)?;
    let hidden = p.cfg.usize_or("hidden_size", 4096)?;
    let n = p.cfg.usize_or("num_hidden_layers", 32)?;
    let attn_dim = p.cfg.opt_usize("attention_hidden_size")?.unwrap_or(hidden);
    let inter = p.cfg.opt_usize("intermediate_size")?.unwrap_or(4 * hidden);
    let eps = p.cfg.f64_or("layer_norm_epsilon", 1e-5)?;
    let rescale = p.cfg.usize_or("rescale_every", 6)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", false)?;
    p.cfg.inert(&["context_length"]);
    let norm = NormSpec::layer(eps);
    let layers = (0..n)
        .map(|i| LayerSpec {
            mixer: Mixer::RwkvTime(RwkvTimeSpec {
                version: 4,
                attn_dim,
                heads: 1,
                head_size: attn_dim,
                head_size_divisor: 1.0,
                gn_eps: 0.0,
                lora: vec![],
            }),
            ffn: Ffn::RwkvChannel(RwkvChannelSpec { version: 4, intermediate: inter }),
            residual: pre_norm(norm),
            post_scale: if rescale > 0 && (i + 1) % rescale == 0 { 0.5 } else { 1.0 },
        })
        .collect();
    let l = "rwkv.blocks.{L}.";
    let mut nm = BTreeMap::new();
    for (k, v) in [
        ("embed", "rwkv.embeddings".to_string()),
        ("embed_norm", "rwkv.blocks.0.pre_ln".to_string()),
        ("final_norm", "rwkv.ln_out".to_string()),
        ("lm_head", if tied { "rwkv.embeddings".into() } else { "head".into() }),
        ("norm.mix", format!("{l}ln1")),
        ("norm.ffn", format!("{l}ln2")),
        ("rwkv.att.mix_k", format!("{l}attention.time_mix_key")),
        ("rwkv.att.mix_v", format!("{l}attention.time_mix_value")),
        ("rwkv.att.mix_r", format!("{l}attention.time_mix_receptance")),
        ("rwkv.att.decay", format!("{l}attention.time_decay")),
        ("rwkv.att.first", format!("{l}attention.time_first")),
        ("rwkv.att.k", format!("{l}attention.key")),
        ("rwkv.att.v", format!("{l}attention.value")),
        ("rwkv.att.r", format!("{l}attention.receptance")),
        ("rwkv.att.o", format!("{l}attention.output")),
        ("rwkv.ffn.mix_k", format!("{l}feed_forward.time_mix_key")),
        ("rwkv.ffn.mix_r", format!("{l}feed_forward.time_mix_receptance")),
        ("rwkv.ffn.k", format!("{l}feed_forward.key")),
        ("rwkv.ffn.r", format!("{l}feed_forward.receptance")),
        ("rwkv.ffn.v", format!("{l}feed_forward.value")),
    ] {
        nm.insert(k.to_string(), v);
    }
    let mut emb = plain_embedding(hidden);
    emb.norm = Some(norm);
    if rescale > 0 {
        p.notes.push(format!("rescale_every={rescale}: attention.output and feed_forward.value weights of block i are divided by 2^(i // {rescale}) and the hidden state halves after every {rescale}th block (HF inference mode)"));
    }
    Ok(p.finish_spec(SpecParts {
        model_type: "rwkv",
        families: vec!["C6"],
        vocab,
        hidden,
        max_pos: None,
        embedding: emb,
        layers,
        final_norm: Some(norm),
        head: plain_head(tied),
        names: nm,
        prefix_aliases: vec![],
        conv1d: false,
    }))
}

/// RWKV-5/6 ship as `trust_remote_code` modules that transformers does not contain.
pub(crate) fn rwkv56(p: &mut P, v: u8) -> Result<ArchSpec> {
    Err(LowerError::not_lowerable(format!(
        "{}: RWKV-{v} exists only as remote code (modeling_rwkv{v}.py), not in transformers 5.17; its group-norm eps, rescale and LoRA widths cannot be checked offline — the WKV-{v} recurrence is an HL op, the config is not lowered yet",
        p.cfg.arch
    )))
}

/// RWKV-7 ships through flash-linear-attention (`fla`), not transformers.
pub(crate) fn rwkv7(p: &mut P) -> Result<ArchSpec> {
    Err(LowerError::not_lowerable(format!(
        "{}: RWKV-7 is implemented by flash-linear-attention, not transformers 5.17; its config keys and tensor names cannot be checked offline — the WKV-7 recurrence is an HL op, the config is not lowered yet",
        p.cfg.arch
    )))
}
