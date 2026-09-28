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
        Flavor::SmolLm3 => {
            LlamaDefaults { vocab: 128256, hidden: 2048, layers: 36, heads: 16, kv: Some(4), max_pos: 32768, tie: true, theta: 2_000_000.0, ..d }
        }
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
            return finish_llama(p, f, FinishArgs { vocab, hidden, inter, n, h, kv, hd, act, max_pos, eps, tied, qkv_bias, o_bias, mlp_bias, types: t, sw, model, lm_head, arch });
        }
        Flavor::Qwen2 | Flavor::Qwen3 | Flavor::SmolLm3 => {
            let use_sw = p.cfg.bool_or("use_sliding_window", false)?;
            let sw_default = if f == Flavor::SmolLm3 { None } else { Some(4096) };
            let sw_raw = p.cfg.usize_or_null("sliding_window", sw_default)?;
            let sw = if use_sw { sw_raw } else { None };
            let mwl = p.cfg.usize_or("max_window_layers", 28)?;
            let t = p.layer_types(n, &["full_attention", "sliding_attention"], |i| {
                if sw.is_some() && i >= mwl { "sliding_attention" } else { "full_attention" }
            })?;
            return finish_llama(p, f, FinishArgs { vocab, hidden, inter, n, h, kv, hd, act, max_pos, eps, tied, qkv_bias, o_bias, mlp_bias, types: t, sw, model, lm_head, arch });
        }
    };
    finish_llama(p, f, FinishArgs { vocab, hidden, inter, n, h, kv, hd, act, max_pos, eps, tied, qkv_bias, o_bias, mlp_bias, types, sw: None, model, lm_head, arch })
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
    let rope = p.rope(a.hd, RopeStyle::Half, Some(theta), None, 1.0, Some(a.max_pos), None)?;
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
            p.unsure.push("MiniCPM remote-code defaults (tie_word_embeddings=true) taken from configuration_minicpm.py as recalled".into());
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
    for i in 0..a.n {
        let position = if no_rope[i] { Position::None } else { Position::Rope(rope.clone()) };
        let mut at = attn(a.h, a.kv, a.hd, position, (a.qkv_bias, a.o_bias));
        at.scale = attn_scale;
        at.window = window_for(&a.types[i], a.sw);
        if f == Flavor::Qwen3 {
            at.qk_norm = Some(QkNorm { norm, scope: QkNormScope::PerHeadShared });
        }
        let residual = Residual::Sequential { pre_mixer: Some(norm), post_mixer: None, pre_ffn: Some(norm), post_ffn: None, multiplier };
        layers.push(LayerSpec { mixer: Mixer::Attention(at), ffn: Ffn::Mlp(gated_mlp(a.inter, a.act, a.mlp_bias)), residual, post_scale: 1.0 });
    }
    let mut nm = llama_names(a.model, a.lm_head);
    if a.tied {
        let e = nm["embed"].clone();
        nm.insert("lm_head".into(), e);
    }
    let model_type = match f {
        Flavor::Llama => "llama",
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
        .map(|_| LayerSpec { mixer: Mixer::Attention(at.clone()), ffn: Ffn::Mlp(gated_mlp(inter, act, false)), residual: pre_norm(norm), post_scale: 1.0 })
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
        .map(|_| LayerSpec { mixer: Mixer::Attention(at.clone()), ffn: Ffn::Mlp(gated_mlp(inter, act, false)), residual: pre_norm(norm), post_scale: 1.0 })
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
                residual: Residual::Sequential { pre_mixer: None, post_mixer: Some(norm), pre_ffn: None, post_ffn: Some(norm), multiplier: 1.0 },
                post_scale: 1.0,
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
    p.unsure.push("EXAONE-4 wiring (post-norm only, NoPE on global layers when sliding_window is set) as recalled from modeling_exaone4.py".into());
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
            post_scale: 1.0,
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
        .map(|_| LayerSpec { mixer: Mixer::Attention(at.clone()), ffn: Ffn::Mlp(gated_mlp(inter, act, false)), residual: residual.clone(), post_scale: 1.0 })
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
    let rope = p.rope(hd, RopeStyle::Half, Some(10000.0), None, 1.0, Some(max_pos), None)?;
    let norm = NormSpec::layer(eps);
    let mut at = attn(h, kv, hd, Position::Rope(rope), (bias, bias));
    at.window = sw;
    let layers = (0..n)
        .map(|_| LayerSpec { mixer: Mixer::Attention(at.clone()), ffn: Ffn::Mlp(plain_mlp(inter, act, bias)), residual: pre_norm(norm), post_scale: 1.0 })
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
    let rope = p.rope(hd, RopeStyle::Half, Some(10000.0), None, 1.0, Some(max_pos), None)?;
    let norm = NormSpec { kind: NormKind::Layer, eps: 1e-5, gain: Gain::None, bias: false };
    let mut at = attn(h, kv, hd, Position::Rope(rope), (bias, bias));
    at.clip_qkv = clip;
    let layers = (0..n)
        .map(|_| LayerSpec { mixer: Mixer::Attention(at.clone()), ffn: Ffn::Mlp(gated_mlp(inter, act, false)), residual: pre_norm(norm), post_scale: 1.0 })
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
            residual: Residual::Sequential { pre_mixer: None, post_mixer: Some(norm), pre_ffn: None, post_ffn: Some(norm), multiplier: 1.0 },
            post_scale: 1.0,
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
    let rope = p.rope(hd, RopeStyle::Interleaved, Some(10000.0), None, 1.0, Some(max_pos), None)?;
    let norm = NormSpec::layer_nobias(eps);
    let (qk_norm, types, sw) = if v2 {
        p.cfg.forbid("use_qk_norm", "Cohere2 has no QK norm")?;
        let sw = p.cfg.usize_or_null("sliding_window", Some(4096))?;
        let pattern = p.cfg.opt_usize("sliding_window_pattern")?.or(p.cfg.opt_usize("_sliding_window_pattern")?).unwrap_or(4);
        let t = p.layer_types(n, &["full_attention", "sliding_attention"], |i| if (i + 1) % pattern != 0 { "sliding_attention" } else { "full_attention" })?;
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
            LayerSpec { mixer: Mixer::Attention(at), ffn: Ffn::Mlp(gated_mlp(inter, act, false)), residual: Residual::Parallel { norm, ffn_norm: None }, post_scale: 1.0 }
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

fn gemma_act(p: &mut P, default: &str) -> Result<Act> {
    let ha = p.cfg.opt_str("hidden_activation")?;
    let hact = p.cfg.opt_str("hidden_act")?;
    let name = match (&ha, &hact) {
        (Some(a), _) => a.clone(),
        (None, Some(b)) if b == "gelu" => {
            p.unsure.push("Gemma: hidden_act=gelu without hidden_activation — transformers 4.40+ uses gelu_pytorch_tanh; followed".into());
            "gelu_pytorch_tanh".into()
        }
        (None, Some(b)) => b.clone(),
        (None, None) => default.into(),
    };
    if let (Some(a), Some(b)) = (&ha, &hact)
        && a != b
        && !(b == "gelu" && a == "gelu_pytorch_tanh")
    {
        p.unsure.push(format!("Gemma: hidden_activation={a} and hidden_act={b} disagree; hidden_activation used"));
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
    let act = gemma_act(p, "gelu_pytorch_tanh")?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 8192)?;
    let eps = p.cfg.f64_or("rms_norm_eps", 1e-6)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", true)?;
    let bias = p.cfg.bool_or("attention_bias", false)?;
    let rope = p.rope(hd, RopeStyle::Half, Some(10000.0), None, 1.0, Some(max_pos), None)?;
    let norm = NormSpec::rms_1p(eps);
    let layers = (0..n)
        .map(|_| LayerSpec {
            mixer: Mixer::Attention(attn(h, kv, hd, Position::Rope(rope.clone()), (bias, bias))),
            ffn: Ffn::Mlp(gated_mlp(inter, act, false)),
            residual: pre_norm(norm),
            post_scale: 1.0,
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
    let act = gemma_act(p, "gelu_pytorch_tanh")?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 8192)?;
    let eps = p.cfg.f64_or("rms_norm_eps", 1e-6)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", true)?;
    let bias = p.cfg.bool_or("attention_bias", false)?;
    let qpas = p.cfg.f64_or("query_pre_attn_scalar", 256.0)?;
    let sw = p.cfg.usize_or_null("sliding_window", Some(4096))?;
    let final_cap = p.cfg.f64_or_null("final_logit_softcapping", Some(30.0))?;
    let attn_cap = p.cfg.f64_or_null("attn_logit_softcapping", Some(50.0))?;
    p.cfg.inert(&["cache_implementation"]);
    let types = p.layer_types(n, &["full_attention", "sliding_attention"], |i| if (i + 1) % 2 != 0 { "sliding_attention" } else { "full_attention" })?;
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
                residual: Residual::Sequential { pre_mixer: Some(norm), post_mixer: Some(norm), pre_ffn: Some(norm), post_ffn: Some(norm), multiplier: 1.0 },
                post_scale: 1.0,
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
    let act = gemma_act(p, "gelu_pytorch_tanh")?;
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
    let types = p.layer_types(n, &["full_attention", "sliding_attention"], |i| if (i + 1) % pattern != 0 { "sliding_attention" } else { "full_attention" })?;
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
                residual: Residual::Sequential { pre_mixer: Some(norm), post_mixer: Some(norm), pre_ffn: Some(norm), post_ffn: Some(norm), multiplier: 1.0 },
                post_scale: 1.0,
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
    let layers = (0..n).map(|_| LayerSpec { mixer: Mixer::Attention(at.clone()), ffn: Ffn::Mlp(mlp.clone()), residual: pre_norm(norm), post_scale: 1.0 }).collect();
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
        p.notes.push(format!("sliding_window={} applies to every layer (HF builds a sliding causal mask when it is set)", sw.unwrap_or(0)));
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
