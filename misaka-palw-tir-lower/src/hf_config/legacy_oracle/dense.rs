//! Dense decoders in the Llama lineage and their modern relatives (C1/C2).

use super::*;
use crate::rope::RopeStyle;

struct LlamaDefaults {
    vocab: usize,
    hidden: usize,
    inter: usize,
    layers: usize,
    heads: usize,
    kv: Option<usize>,
    hd: Option<usize>,
    max_pos: usize,
    eps: f64,
    tie: bool,
    theta: f64,
}

fn llama_defaults(f: Flavor) -> LlamaDefaults {
    let d = LlamaDefaults {
        vocab: 32000,
        hidden: 4096,
        inter: 11008,
        layers: 32,
        heads: 32,
        kv: None,
        hd: None,
        max_pos: 2048,
        eps: 1e-6,
        tie: false,
        theta: 10000.0,
    };
    match f {
        Flavor::Llama | Flavor::Granite => d,
        Flavor::Mistral => LlamaDefaults { inter: 14336, kv: Some(8), max_pos: 4096 * 32, ..d },
        Flavor::Qwen2 => LlamaDefaults { vocab: 151936, inter: 22016, kv: Some(32), max_pos: 32768, ..d },
        Flavor::Qwen3 => LlamaDefaults { vocab: 151936, inter: 22016, kv: Some(32), hd: Some(128), max_pos: 32768, ..d },
        Flavor::MiniCpm => LlamaDefaults { tie: true, ..d },
        Flavor::SmolLm3 => LlamaDefaults {
            vocab: 128256,
            hidden: 2048,
            layers: 36,
            heads: 16,
            kv: Some(4),
            max_pos: 32768,
            tie: true,
            theta: 2_000_000.0,
            ..d
        },
    }
}

pub(crate) fn llama(p: &mut P, f: Flavor) -> Result<ArchSpec> {
    llama_with_prefix(p, f, "model.", "lm_head")
}

/// Llama, Mistral, Qwen2, Qwen3, Granite, MiniCPM (remote), SmolLM3: one wiring, different knobs.
pub(crate) fn llama_with_prefix(p: &mut P, f: Flavor, model: &str, lm_head: &str) -> Result<ArchSpec> {
    let d = llama_defaults(f);
    let arch = p.arch();
    let vocab = p.cfg.usize_or("vocab_size", d.vocab)?;
    let hidden = p.cfg.usize_or("hidden_size", d.hidden)?;
    let inter = p.cfg.usize_or("intermediate_size", d.inter)?;
    let n = p.cfg.usize_or("num_hidden_layers", d.layers)?;
    let (h, kv, hd) = heads(p, hidden, "num_attention_heads", d.heads, d.kv, d.hd)?;
    let act = p.act("hidden_act", "silu")?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", d.max_pos)?;
    let eps = p.cfg.f64_or("rms_norm_eps", d.eps)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", d.tie)?;
    // `pretraining_tp > 1` splits matmuls into slices whose sums are the same function.
    p.cfg.inert(&["pretraining_tp"]);

    let (qkv_bias, o_bias, mlp_bias) = match f {
        Flavor::Qwen2 => (true, false, false),
        Flavor::Mistral => {
            p.cfg.forbid("attention_bias", "MistralAttention has no biases")?;
            p.cfg.forbid("mlp_bias", "MistralMLP has no biases")?;
            (false, false, false)
        }
        Flavor::Qwen3 => {
            let b = p.cfg.bool_or("attention_bias", false)?;
            p.cfg.forbid("mlp_bias", "Qwen3MLP has no biases")?;
            (b, b, false)
        }
        Flavor::MiniCpm => {
            let b = p.cfg.bool_or("attention_bias", false)?;
            (b, b, false)
        }
        _ => {
            let b = p.cfg.bool_or("attention_bias", false)?;
            let m = p.cfg.bool_or("mlp_bias", false)?;
            (b, b, m)
        }
    };

    // Sliding windows.
    let types: Vec<String> = match f {
        Flavor::Llama | Flavor::Granite | Flavor::MiniCpm => (0..n).map(|_| "full_attention".to_string()).collect(),
        Flavor::Mistral => {
            let sw = p.cfg.usize_or_null("sliding_window", Some(4096))?;
            let t = p.layer_types(n, &["full_attention", "sliding_attention"], |_| "sliding_attention")?;
            return finish_llama(
                p,
                f,
                FinishArgs {
                    vocab,
                    hidden,
                    inter,
                    n,
                    h,
                    kv,
                    hd,
                    act,
                    max_pos,
                    eps,
                    tied,
                    qkv_bias,
                    o_bias,
                    mlp_bias,
                    types: t,
                    sw,
                    model,
                    lm_head,
                    arch,
                },
            );
        }
        Flavor::Qwen2 | Flavor::Qwen3 | Flavor::SmolLm3 => {
            if f == Flavor::Qwen2 {
                // Some Qwen2.5 text configs carry the VL flag; the text model never reads it.
                p.cfg.forbid("use_mrope", "multimodal rope positions are not a text decoder's")?;
            }
            let use_sw = p.cfg.bool_or("use_sliding_window", false)?;
            let sw_default = if f == Flavor::SmolLm3 { None } else { Some(4096) };
            let sw_raw = p.cfg.usize_or_null("sliding_window", sw_default)?;
            let sw = if use_sw { sw_raw } else { None };
            let mwl = p.cfg.usize_or("max_window_layers", 28)?;
            let t = p.layer_types(n, &["full_attention", "sliding_attention"], |i| {
                if sw.is_some() && i >= mwl { "sliding_attention" } else { "full_attention" }
            })?;
            return finish_llama(
                p,
                f,
                FinishArgs {
                    vocab,
                    hidden,
                    inter,
                    n,
                    h,
                    kv,
                    hd,
                    act,
                    max_pos,
                    eps,
                    tied,
                    qkv_bias,
                    o_bias,
                    mlp_bias,
                    types: t,
                    sw,
                    model,
                    lm_head,
                    arch,
                },
            );
        }
    };
    finish_llama(
        p,
        f,
        FinishArgs {
            vocab,
            hidden,
            inter,
            n,
            h,
            kv,
            hd,
            act,
            max_pos,
            eps,
            tied,
            qkv_bias,
            o_bias,
            mlp_bias,
            types,
            sw: None,
            model,
            lm_head,
            arch,
        },
    )
}

struct FinishArgs<'s> {
    vocab: usize,
    hidden: usize,
    inter: usize,
    n: usize,
    h: usize,
    kv: usize,
    hd: usize,
    act: Act,
    max_pos: usize,
    eps: f64,
    tied: bool,
    qkv_bias: bool,
    o_bias: bool,
    mlp_bias: bool,
    types: Vec<String>,
    sw: Option<usize>,
    model: &'s str,
    lm_head: &'s str,
    arch: String,
}

fn finish_llama(p: &mut P, f: Flavor, a: FinishArgs) -> Result<ArchSpec> {
    let theta = llama_defaults(f).theta;
    // Ministral-3: Mistral with Llama-4's query scaling (`llama_4_scaling_beta`), applied after the
    // rotation on every layer.
    let ministral3 =
        f == Flavor::Mistral && (a.arch == "Ministral3ForCausalLM" || p.cfg.opt_str("model_type")?.as_deref() == Some("ministral3"));
    let (rope, q_temp) = p.rope_q_scaled(a.hd, RopeStyle::Half, Some(theta), None, 1.0, Some(a.max_pos), None, ministral3)?;
    let norm = NormSpec::rms(a.eps);

    let (mut emb_scale, mut multiplier, mut logit_scale, mut pre_scale) = (1.0, 1.0, 1.0, 1.0);
    let mut attn_scale = 1.0 / (a.hd as f64).sqrt();
    match f {
        Flavor::Granite => {
            emb_scale = p.cfg.f64_or("embedding_multiplier", 1.0)?;
            multiplier = p.cfg.f64_or("residual_multiplier", 1.0)?;
            attn_scale = p.cfg.f64_or("attention_multiplier", 1.0)?;
            logit_scale = 1.0 / p.cfg.f64_or("logits_scaling", 1.0)?;
        }
        Flavor::MiniCpm => {
            emb_scale = p.cfg.f64_or("scale_emb", 1.0)?;
            let depth = p.cfg.f64_or("scale_depth", 1.0)?;
            multiplier = depth / (a.n as f64).sqrt();
            let base = p.cfg.f64_or("dim_model_base", 1.0)?;
            pre_scale = base / a.hidden as f64;
            p.unsure.push(
                "MiniCPM remote-code defaults (tie_word_embeddings=true) taken from configuration_minicpm.py as recalled".into(),
            );
        }
        _ => {}
    }
    let no_rope: Vec<bool> = if f == Flavor::SmolLm3 {
        let interval = p.cfg.usize_or("no_rope_layer_interval", 4)?;
        match p.cfg.opt_usize_list("no_rope_layers")? {
            Some(l) if l.len() == a.n => l.iter().map(|v| *v == 0).collect(),
            Some(l) => return Err(LowerError::bad(format!("{}: no_rope_layers has {} entries", a.arch, l.len()))),
            None => (0..a.n).map(|i| (i + 1) % interval == 0).collect(),
        }
    } else {
        vec![false; a.n]
    };
    if f == Flavor::SmolLm3 {
        p.unsure.push("SmolLM3 no_rope_layers convention (1 = rope, 0 = NoPE) as recalled".into());
    }

    let mut layers = Vec::with_capacity(a.n);
    for (i, nope) in no_rope.iter().enumerate() {
        let position = if *nope { Position::None } else { Position::Rope(rope.clone()) };
        let mut at = attn(a.h, a.kv, a.hd, position, (a.qkv_bias, a.o_bias));
        at.scale = attn_scale;
        at.window = window_for(&a.types[i], a.sw);
        at.q_temperature = q_temp;
        if f == Flavor::Qwen3 {
            at.qk_norm = Some(QkNorm { norm, scope: QkNormScope::PerHeadShared });
        }
        let residual =
            Residual::Sequential { pre_mixer: Some(norm), post_mixer: None, pre_ffn: Some(norm), post_ffn: None, multiplier };
        layers.push(LayerSpec {
            mixer: Mixer::Attention(at),
            ffn: Ffn::Mlp(gated_mlp(a.inter, a.act, a.mlp_bias)),
            residual,
            pre_branch: None, post_scale: 1.0,
        });
    }
    let mut nm = llama_names(a.model, a.lm_head);
    if a.tied {
        let e = nm["embed"].clone();
        nm.insert("lm_head".into(), e);
    }
    let model_type = match f {
        Flavor::Llama => "llama",
        Flavor::Mistral if ministral3 => "ministral3",
        Flavor::Mistral => "mistral",
        Flavor::Qwen2 => "qwen2",
        Flavor::Qwen3 => "qwen3",
        Flavor::Granite => "granite",
        Flavor::MiniCpm => "minicpm",
        Flavor::SmolLm3 => "smollm3",
    };
    let families = if a.sw.is_some() || no_rope.iter().any(|x| *x) { vec!["C1", "C2"] } else { vec!["C1"] };
    let mut head = plain_head(a.tied);
    head.logit_scale = logit_scale;
    head.pre_scale = pre_scale;
    let mut emb = plain_embedding(a.hidden);
    emb.scale = emb_scale;
    Ok(p.finish_spec(SpecParts {
        model_type,
        families,
        vocab: a.vocab,
        hidden: a.hidden,
        max_pos: Some(a.max_pos),
        embedding: emb,
        layers,
        final_norm: Some(norm),
        head,
        names: nm,
        prefix_aliases: vec![],
        conv1d: false,
    }))
}

/// InternLM2 (remote code `modeling_internlm2.py`): Llama math, fused per-kv-group `wqkv`.
pub(crate) fn internlm2(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 103168)?;
    let hidden = p.cfg.usize_or("hidden_size", 4096)?;
    let inter = p.cfg.usize_or("intermediate_size", 11008)?;
    let n = p.cfg.usize_or("num_hidden_layers", 32)?;
    let (h, kv, hd) = heads(p, hidden, "num_attention_heads", 32, None, None)?;
    let act = p.act("hidden_act", "silu")?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 2048)?;
    let eps = p.cfg.f64_or("rms_norm_eps", 1e-6)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", false)?;
    let bias = p.cfg.bool_or("bias", true)?;
    p.cfg.inert(&["pretraining_tp", "attn_implementation"]);
    let rope = p.rope(hd, RopeStyle::Half, Some(10000.0), None, 1.0, Some(max_pos), None)?;
    let norm = NormSpec::rms(eps);
    let at = attn(h, kv, hd, Position::Rope(rope), (bias, bias));
    p.layouts.qkv = QkvLayout::FusedPerKvGroup;
    let layers = (0..n)
        .map(|_| LayerSpec {
            mixer: Mixer::Attention(at.clone()),
            ffn: Ffn::Mlp(gated_mlp(inter, act, false)),
            residual: pre_norm(norm),
            pre_branch: None, post_scale: 1.0,
        })
        .collect();
    let l = "model.layers.{L}.";
    let mut nm = names(&[
        ("embed", "model.tok_embeddings".into()),
        ("final_norm", "model.norm".into()),
        ("lm_head", "output".into()),
        ("norm.mix", format!("{l}attention_norm")),
        ("norm.ffn", format!("{l}ffn_norm")),
        ("attn.qkv", format!("{l}attention.wqkv")),
        ("attn.o", format!("{l}attention.wo")),
        ("mlp.gate", format!("{l}feed_forward.w1")),
        ("mlp.up", format!("{l}feed_forward.w3")),
        ("mlp.down", format!("{l}feed_forward.w2")),
    ]);
    if tied {
        nm.insert("lm_head".into(), "model.tok_embeddings".into());
    }
    p.unsure.push("InternLM2 wqkv row layout [kv][q_0..q_{g-1},k,v][d] from the remote code's einops rearrange, as recalled".into());
    Ok(p.finish_spec(SpecParts {
        model_type: "internlm2",
        families: vec!["C1", "C2"],
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

/// EXAONE 3.x (remote code): Llama math under GPT-2-style names.
pub(crate) fn exaone(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 102400)?;
    let hidden = p.cfg.usize_or("hidden_size", 2048)?;
    let n = p.cfg.usize_or("num_layers", 32)?;
    let inter = p.cfg.opt_usize("intermediate_size")?.unwrap_or(hidden * 4);
    let (h, kv, hd) = heads(p, hidden, "num_attention_heads", 32, None, None)?;
    let act = p.act("activation_function", "silu")?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 2048)?;
    let eps = p.cfg.f64_or("layer_norm_epsilon", 1e-5)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", true)?;
    p.cfg.inert(&["embed_dropout"]);
    let rope = p.rope(hd, RopeStyle::Half, Some(10000.0), None, 1.0, Some(max_pos), None)?;
    let norm = NormSpec::rms(eps);
    let at = attn(h, kv, hd, Position::Rope(rope), (false, false));
    let layers = (0..n)
        .map(|_| LayerSpec {
            mixer: Mixer::Attention(at.clone()),
            ffn: Ffn::Mlp(gated_mlp(inter, act, false)),
            residual: pre_norm(norm),
            pre_branch: None, post_scale: 1.0,
        })
        .collect();
    let l = "transformer.h.{L}.";
    let mut nm = names(&[
        ("embed", "transformer.wte".into()),
        ("final_norm", "transformer.ln_f".into()),
        ("lm_head", "lm_head".into()),
        ("norm.mix", format!("{l}ln_1")),
        ("norm.ffn", format!("{l}ln_2")),
        ("attn.q", format!("{l}attn.attention.q_proj")),
        ("attn.k", format!("{l}attn.attention.k_proj")),
        ("attn.v", format!("{l}attn.attention.v_proj")),
        ("attn.o", format!("{l}attn.attention.out_proj")),
        ("mlp.gate", format!("{l}mlp.c_fc_0")),
        ("mlp.up", format!("{l}mlp.c_fc_1")),
        ("mlp.down", format!("{l}mlp.c_proj")),
    ]);
    if tied {
        nm.insert("lm_head".into(), "transformer.wte".into());
    }
    p.unsure.push("EXAONE-3 tensor names and RMSNorm from modeling_exaone.py as recalled".into());
    Ok(p.finish_spec(SpecParts {
        model_type: "exaone",
        families: vec!["C1", "C2"],
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

/// EXAONE 4.0: post-norms (OLMo-2 style), per-head QK-norm, 3:1 sliding/global with NoPE globals.
pub(crate) fn exaone4(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 102400)?;
    let hidden = p.cfg.usize_or("hidden_size", 4096)?;
    let inter = p.cfg.usize_or("intermediate_size", 16384)?;
    let n = p.cfg.usize_or("num_hidden_layers", 32)?;
    let (h, kv, hd) = heads(p, hidden, "num_attention_heads", 32, Some(32), None)?;
    let act = p.act("hidden_act", "silu")?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 2048)?;
    let eps = p.cfg.f64_or("rms_norm_eps", 1e-5)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", false)?;
    let sw = p.cfg.usize_or_null("sliding_window", Some(4096))?;
    let pattern = p.cfg.usize_or("sliding_window_pattern", 4)?;
    let types = p.layer_types(n, &["full_attention", "sliding_attention"], |i| {
        if sw.is_some() && (i + 1) % pattern != 0 { "sliding_attention" } else { "full_attention" }
    })?;
    let rope = p.rope(hd, RopeStyle::Half, Some(10000.0), None, 1.0, Some(max_pos), None)?;
    let norm = NormSpec::rms(eps);
    let layers = (0..n)
        .map(|i| {
            let sliding = types[i] == "sliding_attention";
            let pos = if sw.is_none() || sliding { Position::Rope(rope.clone()) } else { Position::None };
            let mut at = attn(h, kv, hd, pos, (false, false));
            at.window = if sliding { sw } else { None };
            at.qk_norm = Some(QkNorm { norm, scope: QkNormScope::PerHeadShared });
            LayerSpec {
                mixer: Mixer::Attention(at),
                ffn: Ffn::Mlp(gated_mlp(inter, act, false)),
                residual: Residual::Sequential {
                    pre_mixer: None,
                    post_mixer: Some(norm),
                    pre_ffn: None,
                    post_ffn: Some(norm),
                    multiplier: 1.0,
                },
                pre_branch: None, post_scale: 1.0,
            }
        })
        .collect();
    let mut nm = llama_names("model.", "lm_head");
    nm.remove("norm.mix");
    nm.remove("norm.ffn");
    nm.insert("norm.post_mix".into(), "model.layers.{L}.post_attention_layernorm".into());
    nm.insert("norm.post_ffn".into(), "model.layers.{L}.post_feedforward_layernorm".into());
    if tied {
        nm.insert("lm_head".into(), "model.embed_tokens".into());
    }
    p.unsure.push(
        "EXAONE-4 wiring (post-norm only, NoPE on global layers when sliding_window is set) as recalled from modeling_exaone4.py"
            .into(),
    );
    Ok(p.finish_spec(SpecParts {
        model_type: "exaone4",
        families: vec!["C1", "C2"],
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

/// Nemotron (Minitron): LayerNorm1P `(1+w)·x̂ + b`, squared-ReLU non-gated MLP, partial rotary.
pub(crate) fn nemotron(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 256000)?;
    let hidden = p.cfg.usize_or("hidden_size", 6144)?;
    let inter = p.cfg.usize_or("intermediate_size", 24576)?;
    let n = p.cfg.usize_or("num_hidden_layers", 32)?;
    let (h, kv, hd) = heads(p, hidden, "num_attention_heads", 48, None, None)?;
    let act = p.act("hidden_act", "relu2")?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 4096)?;
    let eps = p.cfg.f64_or("norm_eps", 1e-5)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", false)?;
    let partial = p.cfg.f64_or("partial_rotary_factor", 0.5)?;
    let ab = p.cfg.bool_or("attention_bias", false)?;
    let mb = p.cfg.bool_or("mlp_bias", false)?;
    let rd = (hd as f64 * partial) as usize;
    let rope = p.rope(rd, RopeStyle::Half, Some(10000.0), None, partial, Some(max_pos), None)?;
    let norm = NormSpec { kind: NormKind::Layer, eps, gain: Gain::OnePlusW, bias: true };
    let layers = (0..n)
        .map(|_| LayerSpec {
            mixer: Mixer::Attention(attn(h, kv, hd, Position::Rope(rope.clone()), (ab, ab))),
            ffn: Ffn::Mlp(plain_mlp(inter, act, mb)),
            residual: pre_norm(norm),
            pre_branch: None, post_scale: 1.0,
        })
        .collect();
    let mut nm = llama_names("model.", "lm_head");
    nm.remove("mlp.gate");
    if tied {
        nm.insert("lm_head".into(), "model.embed_tokens".into());
    }
    Ok(p.finish_spec(SpecParts {
        model_type: "nemotron",
        families: vec!["C1"],
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

/// StableLM (2 and 3B-4E1T): LayerNorm, partial rotary, optional per-head QK LayerNorm, optional
/// parallel residual.
pub(crate) fn stablelm(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 50304)?;
    let hidden = p.cfg.usize_or("hidden_size", 2560)?;
    let inter = p.cfg.usize_or("intermediate_size", 6912)?;
    let n = p.cfg.usize_or("num_hidden_layers", 32)?;
    let (h, kv, hd) = heads(p, hidden, "num_attention_heads", 32, Some(32), None)?;
    let act = p.act("hidden_act", "silu")?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 4096)?;
    let eps = p.cfg.f64_or("layer_norm_eps", 1e-5)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", false)?;
    let qkv_bias = p.cfg.bool_or("use_qkv_bias", false)?;
    let qk_ln = p.cfg.bool_or("qk_layernorm", false)?;
    let parallel = p.cfg.bool_or("use_parallel_residual", false)?;
    let partial = p.cfg.f64_or("partial_rotary_factor", 0.25)?;
    let rd = (hd as f64 * partial) as usize;
    let rope = p.rope(rd, RopeStyle::Half, Some(10000.0), None, partial, Some(max_pos), None)?;
    let norm = NormSpec::layer(eps);
    let mut at = attn(h, kv, hd, Position::Rope(rope), (qkv_bias, false));
    at.o_bias = false;
    if qk_ln {
        at.qk_norm = Some(QkNorm { norm: NormSpec::layer_nobias(eps), scope: QkNormScope::PerHeadSeparate });
    }
    let residual = if parallel { Residual::Parallel { norm, ffn_norm: None } } else { pre_norm(norm) };
    let layers = (0..n)
        .map(|_| LayerSpec {
            mixer: Mixer::Attention(at.clone()),
            ffn: Ffn::Mlp(gated_mlp(inter, act, false)),
            residual: residual.clone(),
            pre_branch: None, post_scale: 1.0,
        })
        .collect();
    let mut nm = llama_names("model.", "lm_head");
    nm.insert("attn.q_norm".into(), "model.layers.{L}.self_attn.q_layernorm.norms.{H}".into());
    nm.insert("attn.k_norm".into(), "model.layers.{L}.self_attn.k_layernorm.norms.{H}".into());
    if tied {
        nm.insert("lm_head".into(), "model.embed_tokens".into());
    }
    Ok(p.finish_spec(SpecParts {
        model_type: "stablelm",
        families: vec!["C1"],
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

/// StarCoder2: LayerNorm with bias, biased projections, GELU-tanh MLP, optional sliding window.
pub(crate) fn starcoder2(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 49152)?;
    let hidden = p.cfg.usize_or("hidden_size", 3072)?;
    let inter = p.cfg.usize_or("intermediate_size", 12288)?;
    let n = p.cfg.usize_or("num_hidden_layers", 30)?;
    let (h, kv, hd) = heads(p, hidden, "num_attention_heads", 24, Some(2), None)?;
    let act = p.act("hidden_act", "gelu_pytorch_tanh")?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 4096)?;
    let eps = p.cfg.f64_or("norm_epsilon", 1e-5)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", true)?;
    let bias = p.cfg.bool_or("use_bias", true)?;
    let sw = p.cfg.opt_usize("sliding_window")?;
    // Legacy keys of the original release; transformers never reads them, so they must name
    // the one wiring it implements.
    p.cfg.require_eq("mlp_type", &serde_json::json!("default"), "Starcoder2MLP is the plain c_fc/c_proj MLP")?;
    p.cfg.require_eq("norm_type", &serde_json::json!("layer_norm"), "Starcoder2 uses LayerNorm")?;
    let rope = p.rope(hd, RopeStyle::Half, Some(10000.0), None, 1.0, Some(max_pos), None)?;
    let norm = NormSpec::layer(eps);
    let mut at = attn(h, kv, hd, Position::Rope(rope), (bias, bias));
    at.window = sw;
    let layers = (0..n)
        .map(|_| LayerSpec {
            mixer: Mixer::Attention(at.clone()),
            ffn: Ffn::Mlp(plain_mlp(inter, act, bias)),
            residual: pre_norm(norm),
            pre_branch: None, post_scale: 1.0,
        })
        .collect();
    let mut nm = llama_names("model.", "lm_head");
    nm.remove("mlp.gate");
    nm.insert("mlp.up".into(), "model.layers.{L}.mlp.c_fc".into());
    nm.insert("mlp.down".into(), "model.layers.{L}.mlp.c_proj".into());
    if tied {
        nm.insert("lm_head".into(), "model.embed_tokens".into());
    }
    Ok(p.finish_spec(SpecParts {
        model_type: "starcoder2",
        families: if sw.is_some() { vec!["C1", "C2"] } else { vec!["C1"] },
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

/// OLMo-1 (hf ports): non-parametric LayerNorm (eps fixed at 1e-5 in `OlmoLayerNorm`), `clip_qkv`.
pub(crate) fn olmo(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 50304)?;
    let hidden = p.cfg.usize_or("hidden_size", 4096)?;
    let inter = p.cfg.usize_or("intermediate_size", 11008)?;
    let n = p.cfg.usize_or("num_hidden_layers", 32)?;
    let (h, kv, hd) = heads(p, hidden, "num_attention_heads", 32, None, None)?;
    let act = p.act("hidden_act", "silu")?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 2048)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", false)?;
    let bias = p.cfg.bool_or("attention_bias", false)?;
    let clip = p.cfg.opt_f64("clip_qkv")?;
    p.cfg.inert(&["pretraining_tp"]);
    let rope = p.rope(hd, RopeStyle::Half, Some(10000.0), None, 1.0, Some(max_pos), None)?;
    let norm = NormSpec { kind: NormKind::Layer, eps: 1e-5, gain: Gain::None, bias: false };
    let mut at = attn(h, kv, hd, Position::Rope(rope), (bias, bias));
    at.clip_qkv = clip;
    let layers = (0..n)
        .map(|_| LayerSpec {
            mixer: Mixer::Attention(at.clone()),
            ffn: Ffn::Mlp(gated_mlp(inter, act, false)),
            residual: pre_norm(norm),
            pre_branch: None, post_scale: 1.0,
        })
        .collect();
    let mut nm = llama_names("model.", "lm_head");
    if tied {
        nm.insert("lm_head".into(), "model.embed_tokens".into());
    }
    Ok(p.finish_spec(SpecParts {
        model_type: "olmo",
        families: vec!["C1"],
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

/// OLMo-2: post-norms only, and RMS QK-norm over the WHOLE q/k projection (not per head).
pub(crate) fn olmo2(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 50304)?;
    let hidden = p.cfg.usize_or("hidden_size", 4096)?;
    let inter = p.cfg.usize_or("intermediate_size", 11008)?;
    let n = p.cfg.usize_or("num_hidden_layers", 32)?;
    let (h, kv, hd) = heads(p, hidden, "num_attention_heads", 32, None, None)?;
    let act = p.act("hidden_act", "silu")?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 2048)?;
    let eps = p.cfg.f64_or("rms_norm_eps", 1e-5)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", false)?;
    let bias = p.cfg.bool_or("attention_bias", false)?;
    p.cfg.forbid("clip_qkv", "Olmo2Config has no clip_qkv")?;
    let rope = p.rope(hd, RopeStyle::Half, Some(10000.0), None, 1.0, Some(max_pos), None)?;
    let norm = NormSpec::rms(eps);
    let mut at = attn(h, kv, hd, Position::Rope(rope), (bias, bias));
    at.qk_norm = Some(QkNorm { norm, scope: QkNormScope::Whole });
    let layers = (0..n)
        .map(|_| LayerSpec {
            mixer: Mixer::Attention(at.clone()),
            ffn: Ffn::Mlp(gated_mlp(inter, act, false)),
            residual: Residual::Sequential {
                pre_mixer: None,
                post_mixer: Some(norm),
                pre_ffn: None,
                post_ffn: Some(norm),
                multiplier: 1.0,
            },
            pre_branch: None, post_scale: 1.0,
        })
        .collect();
    let mut nm = llama_names("model.", "lm_head");
    nm.remove("norm.mix");
    nm.remove("norm.ffn");
    nm.insert("norm.post_mix".into(), "model.layers.{L}.post_attention_layernorm".into());
    nm.insert("norm.post_ffn".into(), "model.layers.{L}.post_feedforward_layernorm".into());
    if tied {
        nm.insert("lm_head".into(), "model.embed_tokens".into());
    }
    Ok(p.finish_spec(SpecParts {
        model_type: "olmo2",
        families: vec!["C1"],
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

/// Cohere (Command-R) and Cohere2 (Command R7B / A): bias-free LayerNorm, ONE norm feeding a
/// parallel attention+MLP, GPT-J-interleaved rotary, `logit_scale`. Cohere2 rotates only on its
/// sliding layers; its global layers are NoPE.
pub(crate) fn cohere(p: &mut P, v2: bool) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 256000)?;
    let hidden = p.cfg.usize_or("hidden_size", 8192)?;
    let inter = p.cfg.usize_or("intermediate_size", 22528)?;
    let n = p.cfg.usize_or("num_hidden_layers", 40)?;
    let (h, kv, hd) = heads(p, hidden, "num_attention_heads", 64, None, None)?;
    let act = p.act("hidden_act", "silu")?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 8192)?;
    let eps = p.cfg.f64_or("layer_norm_eps", 1e-5)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", true)?;
    let bias = p.cfg.bool_or("attention_bias", false)?;
    let logit_scale = p.cfg.f64_or("logit_scale", 0.0625)?;
    // A tokenizer limit some Cohere configs carry.
    p.cfg.inert(&["model_max_length", "cache_implementation"]);
    if v2 {
        // Keys of the original Cohere2 release that transformers does not read; each must name
        // the behaviour transformers hard-codes.
        p.cfg.require_eq(
            "order_of_interleaved_layers",
            &serde_json::json!("local_attn_first"),
            "sliding layers come first in each group",
        )?;
        p.cfg.require_eq("position_embedding_type", &serde_json::json!("rope_gptj"), "interleaved (GPT-J) rope")?;
        p.cfg.require_eq("rotary_pct", &serde_json::json!(1.0), "full rotary")?;
        p.cfg.require_eq("use_gated_activation", &serde_json::json!(true), "SwiGLU MLP")?;
        if let Some(share) = p.cfg.opt_bool("use_embedding_sharing")?
            && share != tied
        {
            return Err(LowerError::not_lowerable("cohere2: use_embedding_sharing disagrees with tie_word_embeddings"));
        }
    }
    let rope = p.rope(hd, RopeStyle::Interleaved, Some(if v2 { 10000.0 } else { 500000.0 }), None, 1.0, Some(max_pos), None)?;
    let norm = NormSpec::layer_nobias(eps);
    let (qk_norm, types, sw) = if v2 {
        p.cfg.forbid("use_qk_norm", "Cohere2 has no QK norm")?;
        let sw = p.cfg.usize_or_null("sliding_window", Some(4096))?;
        let pattern = p.cfg.opt_usize("sliding_window_pattern")?.or(p.cfg.opt_usize("_sliding_window_pattern")?).unwrap_or(4);
        let t = p.layer_types(n, &["full_attention", "sliding_attention"], |i| {
            if (i + 1) % pattern != 0 { "sliding_attention" } else { "full_attention" }
        })?;
        (false, t, sw)
    } else {
        (p.cfg.bool_or("use_qk_norm", false)?, vec!["full_attention".to_string(); n], None)
    };
    let layers = (0..n)
        .map(|i| {
            let sliding = types[i] == "sliding_attention";
            let pos = if !v2 || sliding { Position::Rope(rope.clone()) } else { Position::None };
            let mut at = attn(h, kv, hd, pos, (bias, bias));
            at.window = if sliding { sw } else { None };
            if qk_norm {
                at.qk_norm = Some(QkNorm { norm, scope: QkNormScope::PerHeadSeparate });
            }
            LayerSpec {
                mixer: Mixer::Attention(at),
                ffn: Ffn::Mlp(gated_mlp(inter, act, false)),
                residual: Residual::Parallel { norm, ffn_norm: None },
                pre_branch: None, post_scale: 1.0,
            }
        })
        .collect();
    let mut nm = llama_names("model.", "lm_head");
    nm.remove("norm.ffn");
    if tied {
        nm.insert("lm_head".into(), "model.embed_tokens".into());
    }
    let mut head = plain_head(tied);
    head.logit_scale = logit_scale;
    Ok(p.finish_spec(SpecParts {
        model_type: if v2 { "cohere2" } else { "cohere" },
        families: if v2 { vec!["C1", "C2"] } else { vec!["C1"] },
        vocab,
        hidden,
        max_pos: Some(max_pos),
        embedding: plain_embedding(hidden),
        layers,
        final_norm: Some(norm),
        head,
        names: nm,
        prefix_aliases: vec![],
        conv1d: false,
    }))
}

/// Gemma's activation. transformers 5 reads `hidden_act` for Gemma-1 and `hidden_activation` for
/// Gemma-2/3 and ignores the other key; 4.40–4.4x read `hidden_activation` for Gemma-1 too. When
/// both are present and disagree, the transformers-5 choice is followed and the gap is flagged.
fn gemma_act(p: &mut P, primary: &str, other: &str) -> Result<Act> {
    let name = p.cfg.str_or(primary, "gelu_pytorch_tanh")?;
    if let Some(o) = p.cfg.opt_str(other)?
        && o != name
    {
        p.unsure.push(format!(
            "Gemma: `{primary}`={name} is used (transformers 5); `{other}`={o} disagrees and older transformers may have used it"
        ));
    }
    Act::from_hf(&name).ok_or_else(|| LowerError::not_lowerable(format!("{}: activation `{name}`", p.cfg.arch)))
}

/// Gemma-1: `(1+w)` RMSNorm, `√d` embedding scale, GeGLU, tied head.
pub(crate) fn gemma(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 256000)?;
    let hidden = p.cfg.usize_or("hidden_size", 3072)?;
    let inter = p.cfg.usize_or("intermediate_size", 24576)?;
    let n = p.cfg.usize_or("num_hidden_layers", 28)?;
    let (h, kv, hd) = heads(p, hidden, "num_attention_heads", 16, Some(16), Some(256))?;
    let act = gemma_act(p, "hidden_act", "hidden_activation")?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 8192)?;
    let eps = p.cfg.f64_or("rms_norm_eps", 1e-6)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", true)?;
    let bias = p.cfg.bool_or("attention_bias", false)?;
    p.cfg.forbid("use_bidirectional_attention", "bidirectional attention is not a causal LM")?;
    let rope = p.rope(hd, RopeStyle::Half, Some(10000.0), None, 1.0, Some(max_pos), None)?;
    let norm = NormSpec::rms_1p(eps);
    let layers = (0..n)
        .map(|_| LayerSpec {
            mixer: Mixer::Attention(attn(h, kv, hd, Position::Rope(rope.clone()), (bias, bias))),
            ffn: Ffn::Mlp(gated_mlp(inter, act, false)),
            residual: pre_norm(norm),
            pre_branch: None, post_scale: 1.0,
        })
        .collect();
    let mut nm = llama_names("model.", "lm_head");
    if tied {
        nm.insert("lm_head".into(), "model.embed_tokens".into());
    }
    let mut emb = plain_embedding(hidden);
    emb.scale = (hidden as f64).sqrt();
    p.notes.push("embedding × √hidden: HF casts the normalizer to the activation dtype (bf16 in bf16 runs); the f32 reference uses √hidden in f32".into());
    Ok(p.finish_spec(SpecParts {
        model_type: "gemma",
        families: vec!["C1"],
        vocab,
        hidden,
        max_pos: Some(max_pos),
        embedding: emb,
        layers,
        final_norm: Some(norm),
        head: plain_head(tied),
        names: nm,
        prefix_aliases: vec![],
        conv1d: false,
    }))
}

/// Gemma-2: Gemma-1 plus sandwich norms, alternating sliding/global, score and logit soft-caps,
/// `query_pre_attn_scalar`.
pub(crate) fn gemma2(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 256000)?;
    let hidden = p.cfg.usize_or("hidden_size", 2304)?;
    let inter = p.cfg.usize_or("intermediate_size", 9216)?;
    let n = p.cfg.usize_or("num_hidden_layers", 26)?;
    let (h, kv, hd) = heads(p, hidden, "num_attention_heads", 8, Some(4), Some(256))?;
    let act = gemma_act(p, "hidden_activation", "hidden_act")?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 8192)?;
    let eps = p.cfg.f64_or("rms_norm_eps", 1e-6)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", true)?;
    let bias = p.cfg.bool_or("attention_bias", false)?;
    let qpas = p.cfg.f64_or("query_pre_attn_scalar", 256.0)?;
    let sw = p.cfg.usize_or_null("sliding_window", Some(4096))?;
    if let Some(a) = p.cfg.opt_usize("sliding_window_size")?
        && Some(a) != sw
    {
        return Err(LowerError::not_lowerable(format!("gemma2: legacy sliding_window_size {a} disagrees with sliding_window {sw:?}")));
    }
    let final_cap = p.cfg.f64_or_null("final_logit_softcapping", Some(30.0))?;
    let attn_cap = p.cfg.f64_or_null("attn_logit_softcapping", Some(50.0))?;
    p.cfg.forbid("use_bidirectional_attention", "bidirectional attention is not a causal LM")?;
    p.cfg.inert(&["cache_implementation"]);
    let types = p.layer_types(n, &["full_attention", "sliding_attention"], |i| {
        if (i + 1) % 2 != 0 { "sliding_attention" } else { "full_attention" }
    })?;
    let rope = p.rope(hd, RopeStyle::Half, Some(10000.0), None, 1.0, Some(max_pos), None)?;
    let norm = NormSpec::rms_1p(eps);
    let layers = (0..n)
        .map(|i| {
            let mut at = attn(h, kv, hd, Position::Rope(rope.clone()), (bias, bias));
            at.scale = 1.0 / qpas.sqrt();
            at.softcap = attn_cap;
            at.window = window_for(&types[i], sw);
            LayerSpec {
                mixer: Mixer::Attention(at),
                ffn: Ffn::Mlp(gated_mlp(inter, act, false)),
                residual: Residual::Sequential {
                    pre_mixer: Some(norm),
                    post_mixer: Some(norm),
                    pre_ffn: Some(norm),
                    post_ffn: Some(norm),
                    multiplier: 1.0,
                },
                pre_branch: None, post_scale: 1.0,
            }
        })
        .collect();
    let nm = gemma_sandwich_names("model.", if tied { "model.embed_tokens" } else { "lm_head" });
    let mut emb = plain_embedding(hidden);
    emb.scale = (hidden as f64).sqrt();
    let mut head = plain_head(tied);
    head.softcap = final_cap;
    Ok(p.finish_spec(SpecParts {
        model_type: "gemma2",
        families: vec!["C1", "C2"],
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

fn gemma_sandwich_names(model: &str, lm_head: &str) -> BTreeMap<String, String> {
    let mut nm = llama_names(model, lm_head);
    let l = format!("{model}layers.{{L}}.");
    nm.insert("norm.post_mix".into(), format!("{l}post_attention_layernorm"));
    nm.insert("norm.ffn".into(), format!("{l}pre_feedforward_layernorm"));
    nm.insert("norm.post_ffn".into(), format!("{l}post_feedforward_layernorm"));
    nm
}

/// **Gemma-4's text decoder** (`Gemma4ForCausalLM`):
/// * Gemma's four RMSNorms around the mixer and the FFN. The gains are plain `w` here, not
///   Gemma-3's `1 + w`. After them come the per-layer input (PLE, [`PleSpec`], when
///   `hidden_size_per_layer_input` is set) and the learned `layer_scalar` on the layer's output
///   ([`Residual::Sandwich`]).
/// * Attention:
///   - a per-head QK-norm with gains and a weightless V-norm, at scale 1;
///   - sliding layers (`sliding_window`) with the default rope;
///   - full layers with their own head width and KV heads (`per_layer_config`, or
///     `global_head_dim`/`num_global_key_value_heads`) and the proportional rope: frequencies on the
///     first `partial_rotary_factor` of each head, the rest unrotated. With `attention_k_eq_v`
///     their values are the raw key projection.
/// * FFN: a gated GeLU-tanh MLP. With `enable_moe_block`, a softmax top-k MoE runs beside it
///   ([`Ffn::MlpMoe`]); its router reads the residual RMS-normed times a learned vector and
///   `hidden^-½`, renormalises, and scales each weight by its expert's learned scale.
/// * Head: the tied table under the final soft-cap.
///
/// Layers that reuse an earlier layer's keys and values (`num_kv_shared_layers`) are refused by
/// name: the lowering carries only the residual between layers.
pub(crate) fn gemma4_text(p: &mut P, model: &str, lm_head: &str) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 262_144)?;
    let hidden = p.cfg.usize_or("hidden_size", 2304)?;
    let inter = p.cfg.usize_or("intermediate_size", 9216)?;
    let n = p.cfg.usize_or("num_hidden_layers", 30)?;
    let (h, kv, hd) = heads(p, hidden, "num_attention_heads", 8, Some(4), Some(256))?;
    let act = p.act("hidden_activation", "gelu_pytorch_tanh")?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 131_072)?;
    let eps = p.cfg.f64_or("rms_norm_eps", 1e-6)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", true)?;
    let bias = p.cfg.bool_or("attention_bias", false)?;
    let sw = p.cfg.usize_or("sliding_window", 512)?;
    match p.cfg.opt_str("use_bidirectional_attention")?.as_deref() {
        None | Some("vision") => {}
        Some(o) => return Err(LowerError::not_lowerable(format!("gemma4: use_bidirectional_attention `{o}` is not a causal LM"))),
    }
    // The last `num_kv_shared_layers` layers project no keys or values: each attends over those of
    // the last earlier layer of its own type (transformers' `store_full_length_kv`), and with
    // `use_double_wide_mlp` its MLP is twice as wide.
    let kv_shared = p.cfg.usize_or("num_kv_shared_layers", 0)?;
    let double_wide = p.cfg.bool_or("use_double_wide_mlp", false)?;
    let first_shared =
        n.checked_sub(kv_shared).ok_or_else(|| LowerError::bad(format!("gemma4: {kv_shared} KV-sharing layers of {n}")))?;
    if kv_shared > 0 && first_shared == 0 {
        return Err(LowerError::not_lowerable("gemma4: every layer shares keys and values, and no layer computes them"));
    }
    let k_eq_v = p.cfg.bool_or("attention_k_eq_v", false)?;
    let types = p.layer_types(n, &["sliding_attention", "full_attention"], |i| {
        if (i + 1) % 6 == 0 || i + 1 == n { "full_attention" } else { "sliding_attention" }
    })?;
    // One KV slot per layer type that has sharing layers, numbered by where its source is.
    let mut kv_source: BTreeMap<&str, usize> = BTreeMap::new();
    for t in &types[first_shared..] {
        let src = types[..first_shared].iter().rposition(|u| u == t).ok_or_else(|| {
            LowerError::not_lowerable(format!("gemma4: a KV-sharing `{t}` layer, and no earlier `{t}` layer computes keys and values"))
        })?;
        kv_source.insert(t.as_str(), src);
    }
    let mut sources: Vec<usize> = kv_source.values().copied().collect();
    sources.sort_unstable();
    let slot_of_source = |i: usize| sources.iter().position(|s| *s == i);
    // The full layers' head width and KV heads: `per_layer_config` as transformers saves it, else
    // the keys its constructor reads.
    let mut per_layer: BTreeMap<usize, (usize, usize)> = BTreeMap::new();
    match p.cfg.opt_obj("per_layer_config")? {
        Some(o) => {
            for (k, v) in o {
                let i: usize = k.parse().map_err(|_| LowerError::bad(format!("gemma4: per_layer_config key `{k}`")))?;
                let hd_i = v.get("head_dim").and_then(Value::as_u64).map_or(hd, |x| x as usize);
                let kv_i = v.get("num_key_value_heads").and_then(Value::as_u64).map_or(kv, |x| x as usize);
                if let Some(bad) =
                    v.as_object().and_then(|m| m.keys().find(|k| !["head_dim", "num_key_value_heads"].contains(&k.as_str())))
                {
                    return Err(LowerError::not_lowerable(format!("gemma4: per_layer_config `{bad}` is not modelled")));
                }
                per_layer.insert(i, (hd_i, kv_i));
            }
        }
        None => {
            let ghd = p.cfg.usize_or("global_head_dim", 512)?;
            let gkv = p.cfg.opt_usize("num_global_key_value_heads")?.filter(|_| k_eq_v).unwrap_or(kv);
            for (i, t) in types.iter().enumerate() {
                if t == "full_attention" {
                    per_layer.insert(i, (ghd, gkv));
                }
            }
        }
    }
    let d_pl = p.cfg.usize_or("hidden_size_per_layer_input", 256)?;
    let vocab_pl = p.cfg.usize_or("vocab_size_per_layer_input", 262_144)?;
    let moe_on = p.cfg.bool_or("enable_moe_block", false)?;
    let softcap = p.cfg.opt_f64("final_logit_softcapping")?;
    let norm = NormSpec::rms(eps);
    let weightless = NormSpec { kind: NormKind::Rms, eps, gain: Gain::None, bias: false };
    // Rotary tables per layer type, each at that type's head width.
    let rope_for = |p: &P, full: bool, width: usize| -> Result<RopeSpec> {
        let lt = if full { "full_attention" } else { "sliding_attention" };
        let rc = crate::rope::read_rope_config(&p.cfg, Some(if full { 1_000_000.0 } else { 10_000.0 }), Some(lt))?;
        match rc.rope_type.as_str() {
            "proportional" => {
                let prop = rc.params.get("partial_rotary_factor").and_then(Value::as_f64).unwrap_or(1.0);
                let factor = rc.params.get("factor").and_then(Value::as_f64).unwrap_or(1.0);
                if let Some(k) = rc.params.keys().find(|k| !["partial_rotary_factor", "factor"].contains(&k.as_str())) {
                    return Err(LowerError::not_lowerable(format!("gemma4: proportional rope parameter `{k}`")));
                }
                // transformers: `rope_angles = int(prop · head_dim // 2)` frequencies over the head
                // width, then zeros (those pairs are not rotated), all divided by `factor`.
                let angles = (prop * width as f64 / 2.0).floor() as usize;
                let inv: Vec<f32> = (0..width / 2)
                    .map(|i| {
                        if i < angles {
                            let e = (2 * i) as f32 / width as f32;
                            (1.0f32 / crate::detmath::powf(rc.theta, e as f64) as f32) / factor as f32
                        } else {
                            0.0
                        }
                    })
                    .collect();
                Ok(RopeSpec {
                    rotary_dim: width,
                    offset: 0,
                    style: RopeStyle::Half,
                    freqs: crate::rope::RopeFreqs {
                        rope_type: rc.rope_type.clone(),
                        theta: rc.theta,
                        dim: width,
                        inv_freq: inv,
                        attention_factor: 1.0,
                        dynamic: None,
                        longrope: None,
                        mrope: None,
                        reversed: false,
                    },
                })
            }
            _ => p.rope(width, RopeStyle::Half, Some(rc.theta), Some(lt), 1.0, Some(max_pos), None),
        }
    };
    let ple = (d_pl > 0).then(|| PleSpec {
        dim: d_pl,
        vocab: vocab_pl,
        table_scale: (d_pl as f64).sqrt(),
        proj_scale: 1.0 / (hidden as f64).sqrt(),
        norm,
        combine_scale: std::f64::consts::FRAC_1_SQRT_2,
        act,
        post_norm: norm,
    });
    let moe_spec = if moe_on {
        let e = p.cfg.opt_usize("num_experts")?.ok_or_else(|| LowerError::bad("gemma4: enable_moe_block without num_experts"))?;
        let k = p.cfg.opt_usize("top_k_experts")?.ok_or_else(|| LowerError::bad("gemma4: enable_moe_block without top_k_experts"))?;
        if k == 0 || k > e {
            return Err(LowerError::bad(format!("gemma4: top-{k} of {e} experts")));
        }
        let mi = p.cfg.opt_usize("moe_intermediate_size")?.ok_or_else(|| LowerError::bad("gemma4: no moe_intermediate_size"))?;
        let router = RouterSpec {
            scoring: Scoring::Softmax,
            linear_bias: false,
            selection_bias: false,
            groups: None,
            normalize: true,
            norm_eps: 0.0,
            scale: 1.0,
            jitter_eps: 0.0,
            per_expert_scale: true,
        };
        p.layouts.experts = MlpLayout::FusedGateFirst;
        Some(MoeSpec {
            experts: e,
            top_k: k,
            intermediate: mi,
            act,
            glu: Glu::Standard,
            expert_bias: false,
            router,
            shared: None,
            input_scaled: false,
            gated: true,
            latent: None,
            zero_experts: 0,
            out_bias: false,
        })
    } else {
        p.cfg.inert(&["num_experts", "top_k_experts", "moe_intermediate_size"]);
        None
    };
    let mut ropes: BTreeMap<(bool, usize), RopeSpec> = BTreeMap::new();
    let mut layers = Vec::with_capacity(n);
    for (i, t) in types.iter().enumerate() {
        let full = t == "full_attention";
        let (hd_i, kv_i) = per_layer.get(&i).copied().unwrap_or((hd, kv));
        if kv_i == 0 || h % kv_i != 0 {
            return Err(LowerError::bad(format!("gemma4: layer {i}: {h} heads over {kv_i} kv heads")));
        }
        let rope = match ropes.get(&(full, hd_i)) {
            Some(r) => r.clone(),
            None => {
                let r = rope_for(p, full, hd_i)?;
                ropes.insert((full, hd_i), r.clone());
                r
            }
        };
        let mut at = attn(h, kv_i, hd_i, Position::Rope(rope), (bias, bias));
        at.scale = 1.0;
        at.qk_norm = Some(QkNorm { norm, scope: QkNormScope::PerHeadShared });
        at.v_norm = Some(QkNorm { norm: weightless, scope: QkNormScope::PerHeadShared });
        at.window = (!full).then_some(sw);
        at.v_from_k = k_eq_v && full;
        if (hd_i, kv_i) != (hd, kv) {
            at.param_prefix = Some(format!("attn{hd_i}x{kv_i}"));
        }
        let shares = i >= first_shared;
        if shares {
            let src = kv_source[t.as_str()];
            let slot = slot_of_source(src).expect("a source has a slot");
            if per_layer.get(&src).copied().unwrap_or((hd, kv)) != (hd_i, kv_i) {
                return Err(LowerError::bad(format!("gemma4: layer {i} shares the keys of layer {src}, whose heads differ")));
            }
            // It projects queries only: the K = V and V-norm facts are its source's.
            at.kv_share = Some(KvShare::Consumer { slot });
            at.v_from_k = false;
            at.v_norm = None;
        } else if let Some(slot) = slot_of_source(i) {
            at.kv_share = Some(KvShare::Source { slot });
        }
        let mut mlp = gated_mlp(if shares && double_wide { 2 * inter } else { inter }, act, false);
        if shares && double_wide {
            mlp.name = Some("mlp2x".into());
        }
        let ffn = match &moe_spec {
            Some(m) => Ffn::MlpMoe(Box::new(MlpMoeSpec {
                mlp,
                moe: m.clone(),
                mlp_post: norm,
                moe_pre: norm,
                moe_post: norm,
                router_norm: norm,
                router_scale: 1.0 / (hidden as f64).sqrt(),
            })),
            None => Ffn::Mlp(mlp),
        };
        let residual = Residual::Sandwich {
            pre_mixer: norm,
            post_mixer: norm,
            pre_ffn: norm,
            post_ffn: norm,
            ple: ple.clone(),
            layer_scalar: true,
        };
        layers.push(LayerSpec { mixer: Mixer::Attention(at), ffn, residual, pre_branch: None, post_scale: 1.0 });
    }
    let emb = format!("{model}embed_tokens");
    let mut nm = gemma_sandwich_names(model, if tied { &emb } else { lm_head });
    let l = format!("{model}layers.{{L}}.");
    for (k2, v2) in [
        ("norm.post_mlp", format!("{l}post_feedforward_layernorm_1")),
        ("norm.moe", format!("{l}pre_feedforward_layernorm_2")),
        ("norm.post_moe", format!("{l}post_feedforward_layernorm_2")),
        ("moe.router_norm", format!("{l}router.scale")),
        ("moe.router", format!("{l}router.proj")),
        ("moe.expert_scale", format!("{l}router.per_expert_scale")),
        ("moe.gate_up.stacked", format!("{l}experts.gate_up_proj")),
        ("moe.down.stacked", format!("{l}experts.down_proj")),
        ("ple.proj", format!("{model}per_layer_model_projection")),
        ("ple.table", format!("{model}embed_tokens_per_layer")),
        ("ple.norm", format!("{model}per_layer_projection_norm")),
        ("ple.gate", format!("{l}per_layer_input_gate")),
        ("ple.out", format!("{l}per_layer_projection")),
        ("ple.post_norm", format!("{l}post_per_layer_input_norm")),
        ("layer.scalar", format!("{l}layer_scalar")),
    ] {
        nm.insert(k2.into(), v2);
    }
    let mut emb_spec = plain_embedding(hidden);
    emb_spec.scale = (hidden as f64).sqrt();
    let mut head = plain_head(tied);
    head.softcap = softcap;
    Ok(p.finish_spec(SpecParts {
        model_type: "gemma4_text",
        families: vec!["C1", "C2"],
        vocab,
        hidden,
        max_pos: Some(max_pos),
        embedding: emb_spec,
        layers,
        final_norm: Some(norm),
        head,
        names: nm,
        prefix_aliases: vec![],
        conv1d: false,
    }))
}

/// Gemma-3 text decoder (also the decoder of `Gemma3ForConditionalGeneration`): Gemma-2 wiring,
/// per-head `(1+w)` QK-norm instead of score soft-capping, 5:1 sliding/global, and two rotary
/// tables — `rope_local_base_freq` (no scaling) on sliding layers, `rope_theta`+`rope_scaling` on
/// global ones.
pub(crate) fn gemma3_text(p: &mut P, model: &str, lm_head: &str, aliases: Vec<(String, String)>) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 262208)?;
    let hidden = p.cfg.usize_or("hidden_size", 2304)?;
    let inter = p.cfg.usize_or("intermediate_size", 9216)?;
    let n = p.cfg.usize_or("num_hidden_layers", 26)?;
    let (h, kv, hd) = heads(p, hidden, "num_attention_heads", 8, Some(4), Some(256))?;
    let act = gemma_act(p, "hidden_activation", "hidden_act")?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 131072)?;
    let eps = p.cfg.f64_or("rms_norm_eps", 1e-6)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", true)?;
    let bias = p.cfg.bool_or("attention_bias", false)?;
    let qpas = p.cfg.f64_or("query_pre_attn_scalar", 256.0)?;
    let sw = p.cfg.usize_or_null("sliding_window", Some(4096))?;
    let final_cap = p.cfg.opt_f64("final_logit_softcapping")?;
    let attn_cap = p.cfg.opt_f64("attn_logit_softcapping")?;
    let local_theta = p.cfg.f64_or("rope_local_base_freq", 10000.0)?;
    let pattern = p.cfg.opt_usize("sliding_window_pattern")?.or(p.cfg.opt_usize("_sliding_window_pattern")?).unwrap_or(6);
    p.cfg.forbid("use_bidirectional_attention", "bidirectional (embedding) Gemma-3 is not a causal LM")?;
    p.cfg.inert(&["cache_implementation"]);
    let types = p.layer_types(n, &["full_attention", "sliding_attention"], |i| {
        if (i + 1) % pattern != 0 { "sliding_attention" } else { "full_attention" }
    })?;
    let global = if p.cfg.has("rope_parameters") {
        p.rope(hd, RopeStyle::Half, Some(1_000_000.0), Some("full_attention"), 1.0, Some(max_pos), None)?
    } else {
        p.rope(hd, RopeStyle::Half, Some(1_000_000.0), None, 1.0, Some(max_pos), None)?
    };
    let local = if p.cfg.has("rope_parameters") {
        p.rope(hd, RopeStyle::Half, Some(local_theta), Some("sliding_attention"), 1.0, Some(max_pos), None)?
    } else {
        RopeSpec { rotary_dim: hd, offset: 0, style: RopeStyle::Half, freqs: crate::rope::RopeFreqs::plain(local_theta, hd) }
    };
    let norm = NormSpec::rms_1p(eps);
    let layers = (0..n)
        .map(|i| {
            let sliding = types[i] == "sliding_attention";
            let rope = if sliding { local.clone() } else { global.clone() };
            let mut at = attn(h, kv, hd, Position::Rope(rope), (bias, bias));
            at.scale = 1.0 / qpas.sqrt();
            at.softcap = attn_cap;
            at.window = if sliding { sw } else { None };
            at.qk_norm = Some(QkNorm { norm, scope: QkNormScope::PerHeadShared });
            LayerSpec {
                mixer: Mixer::Attention(at),
                ffn: Ffn::Mlp(gated_mlp(inter, act, false)),
                residual: Residual::Sequential {
                    pre_mixer: Some(norm),
                    post_mixer: Some(norm),
                    pre_ffn: Some(norm),
                    post_ffn: Some(norm),
                    multiplier: 1.0,
                },
                pre_branch: None, post_scale: 1.0,
            }
        })
        .collect();
    let emb_name = format!("{model}embed_tokens");
    let nm = gemma_sandwich_names(model, if tied { &emb_name } else { lm_head });
    let mut emb = plain_embedding(hidden);
    emb.scale = (hidden as f64).sqrt();
    let mut head = plain_head(tied);
    head.softcap = final_cap;
    Ok(p.finish_spec(SpecParts {
        model_type: "gemma3_text",
        families: vec!["C1", "C2"],
        vocab,
        hidden,
        max_pos: Some(max_pos),
        embedding: emb,
        layers,
        final_norm: Some(norm),
        head,
        names: nm,
        prefix_aliases: aliases,
        conv1d: false,
    }))
}

/// Phi-3 / 3.5 / 4 / 4-mini: fused `qkv_proj` and `gate_up_proj`, partial rotary (4-mini),
/// LongRoPE with the top-level `original_max_position_embeddings`, optional sliding window.
pub(crate) fn phi3(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 32064)?;
    let hidden = p.cfg.usize_or("hidden_size", 3072)?;
    let inter = p.cfg.usize_or("intermediate_size", 8192)?;
    let n = p.cfg.usize_or("num_hidden_layers", 32)?;
    let (h, kv, hd) = heads(p, hidden, "num_attention_heads", 32, None, None)?;
    let act = p.act("hidden_act", "silu")?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 4096)?;
    let orig = p.cfg.usize_or("original_max_position_embeddings", 4096)?;
    let eps = p.cfg.f64_or("rms_norm_eps", 1e-5)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", false)?;
    let partial = p.cfg.f64_or("partial_rotary_factor", 1.0)?;
    let sw = p.cfg.opt_usize("sliding_window")?;
    let rd = (hd as f64 * partial) as usize;
    let rope = p.rope(rd, RopeStyle::Half, Some(10000.0), None, partial, Some(max_pos), Some(orig))?;
    let norm = NormSpec::rms(eps);
    let mut at = attn(h, kv, hd, Position::Rope(rope), (false, false));
    at.window = sw;
    let mlp = gated_mlp(inter, act, false);
    p.layouts.qkv = QkvLayout::FusedConcat;
    p.layouts.mlp = MlpLayout::FusedGateFirst;
    let layers = (0..n)
        .map(|_| LayerSpec {
            mixer: Mixer::Attention(at.clone()),
            ffn: Ffn::Mlp(mlp.clone()),
            residual: pre_norm(norm),
            pre_branch: None, post_scale: 1.0,
        })
        .collect();
    let l = "model.layers.{L}.";
    let mut nm = names(&[
        ("embed", "model.embed_tokens".into()),
        ("final_norm", "model.norm".into()),
        ("lm_head", if tied { "model.embed_tokens".into() } else { "lm_head".into() }),
        ("norm.mix", format!("{l}input_layernorm")),
        ("norm.ffn", format!("{l}post_attention_layernorm")),
        ("attn.qkv", format!("{l}self_attn.qkv_proj")),
        ("attn.o", format!("{l}self_attn.o_proj")),
        ("mlp.gate_up", format!("{l}mlp.gate_up_proj")),
        ("mlp.down", format!("{l}mlp.down_proj")),
    ]);
    nm.remove("unused");
    if sw.is_some() {
        p.notes.push(format!(
            "sliding_window={} applies to every layer (HF builds a sliding causal mask when it is set)",
            sw.unwrap_or(0)
        ));
    }
    Ok(p.finish_spec(SpecParts {
        model_type: "phi3",
        families: if sw.is_some() { vec!["C1", "C2"] } else { vec!["C1"] },
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

/// GLM (`GlmForCausalLM`: glm-4-9b(-chat)-hf, GLM-Edge) and GLM-4 (`Glm4ForCausalLM`: GLM-4-0414,
/// GLM-Z1): Llama math with a fused `gate_up_proj` (`[gate | up]`), q/k/v biases (`attention_bias`,
/// default true; `o_proj` never has one) and a partial rotary (`partial_rotary_factor`, default 0.5)
/// on interleaved pairs (`rotate_half` over `x[0::2]`, `x[1::2]`, the angles repeated pairwise).
/// GLM-4 wraps each sublayer in a post-norm as well: `post_self_attn_layernorm` after attention,
/// `post_mlp_layernorm` after the MLP (its `post_attention_layernorm` is the pre-MLP norm).
pub(crate) fn glm(p: &mut P, v4: bool) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 151552)?;
    let hidden = p.cfg.usize_or("hidden_size", 4096)?;
    let inter = p.cfg.usize_or("intermediate_size", 13696)?;
    let n = p.cfg.usize_or("num_hidden_layers", 40)?;
    let (h, kv, hd) = heads(p, hidden, "num_attention_heads", 32, Some(2), Some(128))?;
    let act = p.act("hidden_act", "silu")?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 131072)?;
    let eps = p.cfg.f64_or("rms_norm_eps", 1.5625e-7)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", false)?;
    let bias = p.cfg.bool_or("attention_bias", true)?;
    let partial = super::legacy::partial_factor(p, &["partial_rotary_factor"], 0.5)?;
    let rd = (hd as f64 * partial) as usize;
    let rope = p.rope(rd, RopeStyle::Interleaved, Some(10000.0), None, partial, Some(max_pos), None)?;
    let norm = NormSpec::rms(eps);
    let at = attn(h, kv, hd, Position::Rope(rope), (bias, false));
    p.layouts.mlp = MlpLayout::FusedGateFirst;
    let residual = if v4 {
        Residual::Sequential { pre_mixer: Some(norm), post_mixer: Some(norm), pre_ffn: Some(norm), post_ffn: Some(norm), multiplier: 1.0 }
    } else {
        pre_norm(norm)
    };
    let layers = (0..n)
        .map(|_| LayerSpec {
            mixer: Mixer::Attention(at.clone()),
            ffn: Ffn::Mlp(gated_mlp(inter, act, false)),
            residual: residual.clone(),
            pre_branch: None, post_scale: 1.0,
        })
        .collect();
    let l = "model.layers.{L}.";
    let mut nm = llama_names("model.", "lm_head");
    nm.remove("mlp.gate");
    nm.remove("mlp.up");
    nm.insert("mlp.gate_up".into(), format!("{l}mlp.gate_up_proj"));
    if v4 {
        nm.insert("norm.post_mix".into(), format!("{l}post_self_attn_layernorm"));
        nm.insert("norm.post_ffn".into(), format!("{l}post_mlp_layernorm"));
    }
    if tied {
        nm.insert("lm_head".into(), "model.embed_tokens".into());
    }
    Ok(p.finish_spec(SpecParts {
        model_type: if v4 { "glm4" } else { "glm" },
        families: vec!["C1"],
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

/// OLMo-3 (`Olmo3ForCausalLM`): OLMo-2 (post-norms only, RMS QK-norm over the whole projection)
/// with `layer_types` (default: every fourth layer full, the rest sliding) over `sliding_window`
/// (default 4096) and rope parameters per layer type (both default θ 500,000; a legacy
/// `rope_scaling` applies to the full-attention layers).
pub(crate) fn olmo3(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 50304)?;
    let hidden = p.cfg.usize_or("hidden_size", 4096)?;
    let inter = p.cfg.usize_or("intermediate_size", 11008)?;
    let n = p.cfg.usize_or("num_hidden_layers", 32)?;
    let (h, kv, hd) = heads(p, hidden, "num_attention_heads", 32, None, None)?;
    let act = p.act("hidden_act", "silu")?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 2048)?;
    let eps = p.cfg.f64_or("rms_norm_eps", 1e-5)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", false)?;
    let bias = p.cfg.bool_or("attention_bias", false)?;
    let sw = p.cfg.usize_or_null("sliding_window", Some(4096))?;
    p.cfg.forbid("clip_qkv", "Olmo3Config has no clip_qkv")?;
    let types = p.layer_types(n, &["full_attention", "sliding_attention"], |i| {
        if (i + 1) % 4 != 0 { "sliding_attention" } else { "full_attention" }
    })?;
    let keyed = p.cfg.opt_obj("rope_parameters")?.is_some_and(|r| r.contains_key("full_attention") || r.contains_key("sliding_attention"));
    let (full, local) = if keyed {
        (
            p.rope(hd, RopeStyle::Half, Some(500_000.0), Some("full_attention"), 1.0, Some(max_pos), None)?,
            p.rope(hd, RopeStyle::Half, Some(500_000.0), Some("sliding_attention"), 1.0, Some(max_pos), None)?,
        )
    } else {
        let r = p.rope(hd, RopeStyle::Half, Some(500_000.0), None, 1.0, Some(max_pos), None)?;
        (r.clone(), r)
    };
    let norm = NormSpec::rms(eps);
    let layers = (0..n)
        .map(|i| {
            let sliding = types[i] == "sliding_attention";
            let mut at = attn(h, kv, hd, Position::Rope(if sliding { local.clone() } else { full.clone() }), (bias, bias));
            at.qk_norm = Some(QkNorm { norm, scope: QkNormScope::Whole });
            at.window = if sliding { sw } else { None };
            LayerSpec {
                mixer: Mixer::Attention(at),
                ffn: Ffn::Mlp(gated_mlp(inter, act, false)),
                residual: Residual::Sequential { pre_mixer: None, post_mixer: Some(norm), pre_ffn: None, post_ffn: Some(norm), multiplier: 1.0 },
                pre_branch: None, post_scale: 1.0,
            }
        })
        .collect();
    let mut nm = llama_names("model.", "lm_head");
    nm.remove("norm.mix");
    nm.remove("norm.ffn");
    nm.insert("norm.post_mix".into(), "model.layers.{L}.post_attention_layernorm".into());
    nm.insert("norm.post_ffn".into(), "model.layers.{L}.post_feedforward_layernorm".into());
    if tied {
        nm.insert("lm_head".into(), "model.embed_tokens".into());
    }
    Ok(p.finish_spec(SpecParts {
        model_type: "olmo3",
        families: vec!["C1", "C2"],
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
