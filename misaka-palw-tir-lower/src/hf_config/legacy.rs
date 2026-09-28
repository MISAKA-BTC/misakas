//! The pre-Llama lineages that still make up a large share of hub checkpoints: GPT-2, GPT-Neo,
//! GPT-NeoX (Pythia), GPT-J, Falcon, GPTBigCode (StarCoder-1), BLOOM, MPT, OPT, Phi-1/2.

use super::*;
use crate::rope::{AlibiSpec, RopeFreqs, RopeStyle, alibi_slopes_bloom, alibi_slopes_mpt};

fn ln(eps: f64) -> NormSpec {
    NormSpec::layer(eps)
}

/// The effective partial-rotary factor: `rope_parameters.partial_rotary_factor` (transformers 5),
/// else the listed legacy keys, else the class default.
fn partial_factor(p: &P, legacy: &[&str], default: f64) -> Result<f64> {
    if let Some(rp) = p.cfg.opt_obj("rope_parameters")?
        && let Some(v) = rp.get("partial_rotary_factor").and_then(Value::as_f64)
    {
        for k in legacy {
            if let Some(x) = p.cfg.opt_f64(k)?
                && (x - v).abs() > 1e-12
            {
                return Err(LowerError::not_lowerable(format!("{}: `{k}`={x} disagrees with rope_parameters' {v}", p.cfg.arch)));
            }
        }
        return Ok(v);
    }
    for k in legacy {
        if let Some(x) = p.cfg.opt_f64(k)? {
            return Ok(x);
        }
    }
    Ok(default)
}

/// Phi-1 / 1.5 / 2: ONE LayerNorm feeding a parallel attention+MLP, partial rotary, biases
/// everywhere including the LM head, optional per-head QK LayerNorm (shared gain).
pub(crate) fn phi(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 51200)?;
    let hidden = p.cfg.usize_or("hidden_size", 2048)?;
    let inter = p.cfg.usize_or("intermediate_size", 8192)?;
    let n = p.cfg.usize_or("num_hidden_layers", 24)?;
    let (h, kv, hd) = heads(p, hidden, "num_attention_heads", 32, None, None)?;
    let act = p.act("hidden_act", "gelu_new")?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 2048)?;
    let eps = p.cfg.f64_or("layer_norm_eps", 1e-5)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", false)?;
    let qk_ln = p.cfg.bool_or("qk_layernorm", false)?;
    let partial = partial_factor(p, &["partial_rotary_factor"], 0.5)?;
    let rd = (hd as f64 * partial) as usize;
    let rope = p.rope(rd, RopeStyle::Half, Some(10000.0), None, partial, Some(max_pos), None)?;
    let norm = ln(eps);
    let mut at = attn(h, kv, hd, Position::Rope(rope), (true, true));
    if qk_ln {
        at.qk_norm = Some(QkNorm { norm: ln(eps), scope: QkNormScope::PerHeadShared });
    }
    let layers = (0..n)
        .map(|_| LayerSpec {
            mixer: Mixer::Attention(at.clone()),
            ffn: Ffn::Mlp(plain_mlp(inter, act, true)),
            residual: Residual::Parallel { norm, ffn_norm: None },
            post_scale: 1.0,
        })
        .collect();
    let l = "model.layers.{L}.";
    let nm = names(&[
        ("embed", "model.embed_tokens".into()),
        ("final_norm", "model.final_layernorm".into()),
        ("lm_head", if tied { "model.embed_tokens".into() } else { "lm_head".into() }),
        ("norm.mix", format!("{l}input_layernorm")),
        ("attn.q", format!("{l}self_attn.q_proj")),
        ("attn.k", format!("{l}self_attn.k_proj")),
        ("attn.v", format!("{l}self_attn.v_proj")),
        ("attn.o", format!("{l}self_attn.dense")),
        ("attn.q_norm", format!("{l}self_attn.q_layernorm")),
        ("attn.k_norm", format!("{l}self_attn.k_layernorm")),
        ("mlp.up", format!("{l}mlp.fc1")),
        ("mlp.down", format!("{l}mlp.fc2")),
    ]);
    let mut head = plain_head(tied);
    head.bias = true;
    let mut nm = nm;
    nm.insert("lm_head_bias".into(), "lm_head.bias".into());
    Ok(p.finish_spec(SpecParts {
        model_type: "phi",
        families: vec!["C1"],
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

/// GPT-2: learned positions, `Conv1D` weights stored `[in, out]`, fused `c_attn`.
pub(crate) fn gpt2(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 50257)?;
    let hidden = p.cfg.alias_usize(&["n_embd", "hidden_size"])?.unwrap_or(768);
    let n = p.cfg.alias_usize(&["n_layer", "num_hidden_layers"])?.unwrap_or(12);
    let h = p.cfg.alias_usize(&["n_head", "num_attention_heads"])?.unwrap_or(12);
    let n_pos = p.cfg.alias_usize(&["n_positions", "max_position_embeddings"])?.unwrap_or(1024);
    let inter = p.cfg.opt_usize("n_inner")?.unwrap_or(4 * hidden);
    let act = p.act("activation_function", "gelu_new")?;
    let eps = p.cfg.f64_or("layer_norm_epsilon", 1e-5)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", true)?;
    let scale_w = p.cfg.bool_or("scale_attn_weights", true)?;
    let inv_layer = p.cfg.bool_or("scale_attn_by_inverse_layer_idx", false)?;
    // `reorder_and_upcast_attn` changes where the float32 upcast happens, not the function.
    p.cfg.inert(&["reorder_and_upcast_attn", "n_ctx"]);
    if hidden % h != 0 {
        return Err(LowerError::bad(format!("gpt2: n_embd {hidden} not divisible by n_head {h}")));
    }
    let hd = hidden / h;
    let norm = ln(eps);
    p.layouts.qkv = QkvLayout::FusedConcat;
    let layers = (0..n)
        .map(|i| {
            let mut at = attn(h, h, hd, Position::None, (true, true));
            at.scale = if scale_w { 1.0 / (hd as f64).sqrt() } else { 1.0 };
            if inv_layer {
                at.scale /= (i + 1) as f64;
            }
            LayerSpec { mixer: Mixer::Attention(at), ffn: Ffn::Mlp(plain_mlp(inter, act, true)), residual: pre_norm(norm), post_scale: 1.0 }
        })
        .collect();
    let l = "transformer.h.{L}.";
    let nm = names(&[
        ("embed", "transformer.wte".into()),
        ("pos_embed", "transformer.wpe".into()),
        ("final_norm", "transformer.ln_f".into()),
        ("lm_head", if tied { "transformer.wte".into() } else { "lm_head".into() }),
        ("norm.mix", format!("{l}ln_1")),
        ("norm.ffn", format!("{l}ln_2")),
        ("attn.qkv", format!("{l}attn.c_attn")),
        ("attn.o", format!("{l}attn.c_proj")),
        ("mlp.up", format!("{l}mlp.c_fc")),
        ("mlp.down", format!("{l}mlp.c_proj")),
    ]);
    let mut emb = plain_embedding(hidden);
    emb.positions = Some(LearnedPositions { rows: n_pos, offset: 0 });
    Ok(p.finish_spec(SpecParts {
        model_type: "gpt2",
        families: vec!["C1"],
        vocab,
        hidden,
        max_pos: Some(n_pos),
        embedding: emb,
        layers,
        final_norm: Some(norm),
        head: plain_head(tied),
        names: nm,
        // The hub's original `gpt2`/`distilgpt2` files have no `transformer.` prefix.
        prefix_aliases: vec![("transformer.".into(), String::new())],
        conv1d: true,
    }))
}

/// GPT-Neo: learned positions, alternating global/local (windowed) attention, and NO `1/√d`
/// score scaling.
pub(crate) fn gpt_neo(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 50257)?;
    let hidden = p.cfg.usize_or("hidden_size", 2048)?;
    let n = p.cfg.alias_usize(&["num_layers", "num_hidden_layers"])?.unwrap_or(24);
    let h = p.cfg.alias_usize(&["num_heads", "num_attention_heads"])?.unwrap_or(16);
    let max_pos = p.cfg.usize_or("max_position_embeddings", 2048)?;
    let inter = p.cfg.opt_usize("intermediate_size")?.unwrap_or(4 * hidden);
    let window = p.cfg.usize_or("window_size", 256)?;
    let act = p.act("activation_function", "gelu_new")?;
    let eps = p.cfg.f64_or("layer_norm_epsilon", 1e-5)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", true)?;
    let mut types: Vec<String> = Vec::new();
    match p.cfg.opt_list("attention_types")? {
        Some(list) => {
            for item in list {
                let pair = item.as_array().filter(|a| a.len() == 2).ok_or_else(|| LowerError::bad("gpt_neo: attention_types entry"))?;
                let kinds = pair[0].as_array().ok_or_else(|| LowerError::bad("gpt_neo: attention_types kinds"))?;
                let reps = pair[1].as_u64().ok_or_else(|| LowerError::bad("gpt_neo: attention_types count"))?;
                for _ in 0..reps {
                    for k in kinds {
                        types.push(k.as_str().unwrap_or("").to_string());
                    }
                }
            }
        }
        None => types = (0..n).map(|i| if i % 2 == 0 { "global".into() } else { "local".into() }).collect(),
    }
    if let Some(al) = p.cfg.opt_str_list("attention_layers")?
        && al != types
    {
        return Err(LowerError::bad("gpt_neo: attention_layers disagrees with attention_types"));
    }
    if types.len() != n || types.iter().any(|t| t != "global" && t != "local") {
        return Err(LowerError::not_lowerable(format!("gpt_neo: attention layer pattern {types:?} does not match {n} layers of global/local")));
    }
    let hd = hidden / h;
    let norm = ln(eps);
    let layers = (0..n)
        .map(|i| {
            let mut at = attn(h, h, hd, Position::None, (false, true));
            at.scale = 1.0;
            at.window = if types[i] == "local" { Some(window) } else { None };
            LayerSpec { mixer: Mixer::Attention(at), ffn: Ffn::Mlp(plain_mlp(inter, act, true)), residual: pre_norm(norm), post_scale: 1.0 }
        })
        .collect();
    let l = "transformer.h.{L}.";
    let nm = names(&[
        ("embed", "transformer.wte".into()),
        ("pos_embed", "transformer.wpe".into()),
        ("final_norm", "transformer.ln_f".into()),
        ("lm_head", if tied { "transformer.wte".into() } else { "lm_head".into() }),
        ("norm.mix", format!("{l}ln_1")),
        ("norm.ffn", format!("{l}ln_2")),
        ("attn.q", format!("{l}attn.attention.q_proj")),
        ("attn.k", format!("{l}attn.attention.k_proj")),
        ("attn.v", format!("{l}attn.attention.v_proj")),
        ("attn.o", format!("{l}attn.attention.out_proj")),
        ("mlp.up", format!("{l}mlp.c_fc")),
        ("mlp.down", format!("{l}mlp.c_proj")),
    ]);
    let mut emb = plain_embedding(hidden);
    emb.positions = Some(LearnedPositions { rows: max_pos, offset: 0 });
    Ok(p.finish_spec(SpecParts {
        model_type: "gpt_neo",
        families: vec!["C1", "C2"],
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

/// GPT-NeoX / Pythia: per-head interleaved `query_key_value`, partial rotary, parallel residual
/// with TWO LayerNorms.
pub(crate) fn gpt_neox(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 50432)?;
    let hidden = p.cfg.usize_or("hidden_size", 6144)?;
    let inter = p.cfg.usize_or("intermediate_size", 24576)?;
    let n = p.cfg.usize_or("num_hidden_layers", 44)?;
    let (h, kv, hd) = heads(p, hidden, "num_attention_heads", 64, None, None)?;
    if kv != h {
        return Err(LowerError::not_lowerable("gpt_neox: num_key_value_heads ≠ heads"));
    }
    let act = p.act("hidden_act", "gelu")?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 2048)?;
    let eps = p.cfg.f64_or("layer_norm_eps", 1e-5)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", false)?;
    let parallel = p.cfg.bool_or("use_parallel_residual", true)?;
    let bias = p.cfg.bool_or("attention_bias", true)?;
    let partial = partial_factor(p, &["rotary_pct", "partial_rotary_factor"], 0.25)?;
    let theta = p.cfg.opt_f64("rotary_emb_base")?.unwrap_or(10000.0);
    let rd = (hd as f64 * partial) as usize;
    let rope = p.rope(rd, RopeStyle::Half, Some(theta), None, partial, Some(max_pos), None)?;
    let norm = ln(eps);
    let at = attn(h, h, hd, Position::Rope(rope), (bias, bias));
    p.layouts.qkv = QkvLayout::FusedPerHead;
    let residual = if parallel { Residual::Parallel { norm, ffn_norm: Some(norm) } } else { pre_norm(norm) };
    let layers = (0..n)
        .map(|_| LayerSpec { mixer: Mixer::Attention(at.clone()), ffn: Ffn::Mlp(plain_mlp(inter, act, true)), residual: residual.clone(), post_scale: 1.0 })
        .collect();
    let l = "gpt_neox.layers.{L}.";
    let nm = names(&[
        ("embed", "gpt_neox.embed_in".into()),
        ("final_norm", "gpt_neox.final_layer_norm".into()),
        ("lm_head", if tied { "gpt_neox.embed_in".into() } else { "embed_out".into() }),
        ("norm.mix", format!("{l}input_layernorm")),
        ("norm.ffn", format!("{l}post_attention_layernorm")),
        ("attn.qkv", format!("{l}attention.query_key_value")),
        ("attn.o", format!("{l}attention.dense")),
        ("mlp.up", format!("{l}mlp.dense_h_to_4h")),
        ("mlp.down", format!("{l}mlp.dense_4h_to_h")),
    ]);
    Ok(p.finish_spec(SpecParts {
        model_type: "gpt_neox",
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

/// GPT-J: interleaved (`rotate_every_two`) rotary on the first `rotary_dim` dims, one LayerNorm
/// feeding a parallel attention+MLP, biased LM head.
pub(crate) fn gptj(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 50400)?;
    let hidden = p.cfg.alias_usize(&["n_embd", "hidden_size"])?.unwrap_or(4096);
    let n = p.cfg.alias_usize(&["n_layer", "num_hidden_layers"])?.unwrap_or(28);
    let h = p.cfg.alias_usize(&["n_head", "num_attention_heads"])?.unwrap_or(16);
    let n_pos = p.cfg.alias_usize(&["n_positions", "max_position_embeddings"])?.unwrap_or(2048);
    let inter = p.cfg.opt_usize("n_inner")?.unwrap_or(4 * hidden);
    let act = p.act("activation_function", "gelu_new")?;
    let eps = p.cfg.f64_or("layer_norm_epsilon", 1e-5)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", false)?;
    let hd = hidden / h;
    let rd = p.cfg.usize_or_null("rotary_dim", Some(64))?.unwrap_or(hd);
    let rope = RopeSpec { rotary_dim: rd, offset: 0, style: RopeStyle::Interleaved, freqs: RopeFreqs::plain(10000.0, rd) };
    let norm = ln(eps);
    let at = attn(h, h, hd, Position::Rope(rope), (false, false));
    let layers = (0..n)
        .map(|_| LayerSpec {
            mixer: Mixer::Attention(at.clone()),
            ffn: Ffn::Mlp(plain_mlp(inter, act, true)),
            residual: Residual::Parallel { norm, ffn_norm: None },
            post_scale: 1.0,
        })
        .collect();
    let l = "transformer.h.{L}.";
    let nm = names(&[
        ("embed", "transformer.wte".into()),
        ("final_norm", "transformer.ln_f".into()),
        ("lm_head", if tied { "transformer.wte".into() } else { "lm_head".into() }),
        ("norm.mix", format!("{l}ln_1")),
        ("attn.q", format!("{l}attn.q_proj")),
        ("attn.k", format!("{l}attn.k_proj")),
        ("attn.v", format!("{l}attn.v_proj")),
        ("attn.o", format!("{l}attn.out_proj")),
        ("mlp.up", format!("{l}mlp.fc_in")),
        ("mlp.down", format!("{l}mlp.fc_out")),
    ]);
    let mut head = plain_head(tied);
    head.bias = true;
    let mut nm = nm;
    nm.insert("lm_head_bias".into(), "lm_head.bias".into());
    Ok(p.finish_spec(SpecParts {
        model_type: "gptj",
        families: vec!["C1"],
        vocab,
        hidden,
        max_pos: Some(n_pos),
        embedding: plain_embedding(hidden),
        layers,
        final_norm: Some(norm),
        head,
        names: nm,
        prefix_aliases: vec![],
        conv1d: false,
    }))
}

/// Falcon (7B MQA-parallel, 40B/180B new decoder with kv groups, RW-1B ALiBi) and the legacy
/// `RWForCausalLM` remote-code configs.
pub(crate) fn falcon(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 65024)?;
    let hidden = p.cfg.usize_or("hidden_size", 4544)?;
    let n = p.cfg.alias_usize(&["num_hidden_layers", "n_layer"])?.unwrap_or(32);
    let h = p.cfg.alias_usize(&["num_attention_heads", "n_head"])?.unwrap_or(71);
    let kv_cfg = p.cfg.alias_usize(&["num_kv_heads", "n_head_kv"])?.unwrap_or(h);
    let new_arch = p.cfg.bool_or("new_decoder_architecture", false)?;
    let mq = p.cfg.bool_or("multi_query", true)?;
    let parallel = p.cfg.bool_or("parallel_attn", true)?;
    let bias = p.cfg.bool_or("bias", false)?;
    let alibi = p.cfg.bool_or("alibi", false)?;
    let eps = p.cfg.f64_or("layer_norm_epsilon", 1e-5)?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 2048)?;
    let ffn = p.cfg.opt_usize("ffn_hidden_size")?.unwrap_or(4 * hidden);
    let act = p.act("activation", "gelu")?;
    let tied = p.cfg.bool_or("tie_word_embeddings", true)?;
    let n_ln = p.cfg.opt_usize("num_ln_in_parallel_attn")?;
    p.cfg.forbid("apply_residual_connection_post_layernorm", "RW post-LN residual variant is not modelled")?;
    p.cfg.inert(&["head_dim"]);
    if hidden % h != 0 {
        return Err(LowerError::bad("falcon: hidden not divisible by heads"));
    }
    let hd = hidden / h;
    let (kv, layout) = if new_arch {
        (kv_cfg, QkvLayout::FusedPerKvGroup)
    } else if mq {
        (1, QkvLayout::FusedConcat)
    } else {
        if kv_cfg != h {
            return Err(LowerError::not_lowerable("falcon: MHA with num_kv_heads ≠ heads"));
        }
        (h, QkvLayout::FusedPerHead)
    };
    if h % kv != 0 {
        return Err(LowerError::bad("falcon: heads not a multiple of kv heads"));
    }
    let position = if alibi {
        Position::Alibi(AlibiSpec { slopes: alibi_slopes_bloom(h), scaled_by_softmax_scale: true, bf16_bias: true })
    } else {
        Position::Rope(p.rope(hd, RopeStyle::Half, Some(10000.0), None, 1.0, Some(max_pos), None)?)
    };
    if alibi {
        p.notes.push("ALiBi as HF's SDPA path computes it: (q·k + bf16(bf16(slope)·j))/√d; the eager path in transformers 5.17 adds the bias twice and is not the reference".into());
        p.cfg.inert(&["rope_theta", "rope_scaling", "rope_parameters"]);
    }
    let norm = ln(eps);
    let at = attn(h, kv, hd, position, (bias, bias));
    p.layouts.qkv = layout;
    let residual = if new_arch {
        match n_ln.unwrap_or(2) {
            2 => Residual::Parallel { norm, ffn_norm: Some(norm) },
            1 => Residual::Parallel { norm, ffn_norm: None },
            x => return Err(LowerError::not_lowerable(format!("falcon: num_ln_in_parallel_attn={x}"))),
        }
    } else if parallel {
        Residual::Parallel { norm, ffn_norm: None }
    } else {
        pre_norm(norm)
    };
    let two_ln = matches!(residual, Residual::Parallel { ffn_norm: Some(_), .. });
    let layers = (0..n)
        .map(|_| LayerSpec { mixer: Mixer::Attention(at.clone()), ffn: Ffn::Mlp(plain_mlp(ffn, act, bias)), residual: residual.clone(), post_scale: 1.0 })
        .collect();
    let l = "transformer.h.{L}.";
    let (mix, ffn_n) = if two_ln { ("ln_attn", "ln_mlp") } else { ("input_layernorm", "post_attention_layernorm") };
    let nm = names(&[
        ("embed", "transformer.word_embeddings".into()),
        ("final_norm", "transformer.ln_f".into()),
        ("lm_head", if tied { "transformer.word_embeddings".into() } else { "lm_head".into() }),
        ("norm.mix", format!("{l}{mix}")),
        ("norm.ffn", format!("{l}{ffn_n}")),
        ("attn.qkv", format!("{l}self_attention.query_key_value")),
        ("attn.o", format!("{l}self_attention.dense")),
        ("mlp.up", format!("{l}mlp.dense_h_to_4h")),
        ("mlp.down", format!("{l}mlp.dense_4h_to_h")),
    ]);
    if p.cfg.arch == "RWForCausalLM" {
        p.unsure.push("RWForCausalLM (legacy remote code) read through FalconConfig's aliases".into());
    }
    Ok(p.finish_spec(SpecParts {
        model_type: "falcon",
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

/// GPTBigCode (StarCoder-1, SantaCoder): GPT-2 wiring with `nn.Linear`, MQA.
pub(crate) fn gpt_bigcode(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 50257)?;
    let hidden = p.cfg.alias_usize(&["n_embd", "hidden_size"])?.unwrap_or(768);
    let n = p.cfg.alias_usize(&["n_layer", "num_hidden_layers"])?.unwrap_or(12);
    let h = p.cfg.alias_usize(&["n_head", "num_attention_heads"])?.unwrap_or(12);
    let n_pos = p.cfg.alias_usize(&["n_positions", "max_position_embeddings"])?.unwrap_or(1024);
    let inter = p.cfg.opt_usize("n_inner")?.unwrap_or(4 * hidden);
    let act = p.act("activation_function", "gelu_pytorch_tanh")?;
    let eps = p.cfg.f64_or("layer_norm_epsilon", 1e-5)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", true)?;
    let scale_w = p.cfg.bool_or("scale_attn_weights", true)?;
    let mq = p.cfg.bool_or("multi_query", true)?;
    if let Some(k) = p.cfg.opt_usize("num_key_value_heads")?
        && k != if mq { 1 } else { h }
    {
        return Err(LowerError::bad("gpt_bigcode: num_key_value_heads disagrees with multi_query"));
    }
    // Upcasting flags change only where float32 is used; the fp32 reference is the function.
    p.cfg.inert(&["attention_softmax_in_fp32", "scale_attention_softmax_in_fp32"]);
    let hd = hidden / h;
    let kv = if mq { 1 } else { h };
    let norm = ln(eps);
    let mut at = attn(h, kv, hd, Position::None, (true, true));
    p.layouts.qkv = if mq { QkvLayout::FusedConcat } else { QkvLayout::FusedPerHead };
    at.scale = if scale_w { 1.0 / (hd as f64).sqrt() } else { 1.0 };
    let layers = (0..n)
        .map(|_| LayerSpec { mixer: Mixer::Attention(at.clone()), ffn: Ffn::Mlp(plain_mlp(inter, act, true)), residual: pre_norm(norm), post_scale: 1.0 })
        .collect();
    let l = "transformer.h.{L}.";
    let nm = names(&[
        ("embed", "transformer.wte".into()),
        ("pos_embed", "transformer.wpe".into()),
        ("final_norm", "transformer.ln_f".into()),
        ("lm_head", if tied { "transformer.wte".into() } else { "lm_head".into() }),
        ("norm.mix", format!("{l}ln_1")),
        ("norm.ffn", format!("{l}ln_2")),
        ("attn.qkv", format!("{l}attn.c_attn")),
        ("attn.o", format!("{l}attn.c_proj")),
        ("mlp.up", format!("{l}mlp.c_fc")),
        ("mlp.down", format!("{l}mlp.c_proj")),
    ]);
    let mut emb = plain_embedding(hidden);
    emb.positions = Some(LearnedPositions { rows: n_pos, offset: 0 });
    Ok(p.finish_spec(SpecParts {
        model_type: "gpt_bigcode",
        families: vec!["C1", "C2"],
        vocab,
        hidden,
        max_pos: Some(n_pos),
        embedding: emb,
        layers,
        final_norm: Some(norm),
        head: plain_head(tied),
        names: nm,
        prefix_aliases: vec![],
        conv1d: false,
    }))
}

/// BLOOM: embedding LayerNorm, per-head interleaved fused qkv, ALiBi (unscaled, added after the
/// `1/√d` product), GELU-tanh.
pub(crate) fn bloom(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 250880)?;
    let hidden = p.cfg.alias_usize(&["hidden_size", "n_embed"])?.unwrap_or(64);
    let n = p.cfg.alias_usize(&["n_layer", "num_hidden_layers"])?.unwrap_or(2);
    let h = p.cfg.alias_usize(&["n_head", "num_attention_heads"])?.unwrap_or(8);
    let eps = p.cfg.f64_or("layer_norm_epsilon", 1e-5)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", true)?;
    p.cfg.forbid("apply_residual_connection_post_layernorm", "BLOOM's post-LN residual variant is not modelled")?;
    p.cfg.forbid("n_inner", "BLOOM's MLP is fixed at 4·hidden")?;
    // Megatron training knobs `modeling_bloom.py` never reads.
    p.cfg.inert(&[
        "pretraining_tp",
        "slow_but_exact",
        "attention_softmax_in_fp32",
        "bias_dropout_fusion",
        "masked_softmax_fusion",
        "offset_alibi",
        "skip_bias_add",
        "skip_bias_add_qkv",
        "seq_length",
    ]);
    let hd = hidden / h;
    let norm = ln(eps);
    let at = attn(h, h, hd, Position::Alibi(AlibiSpec { slopes: alibi_slopes_bloom(h), scaled_by_softmax_scale: false, bf16_bias: false }), (true, true));
    p.layouts.qkv = QkvLayout::FusedPerHead;
    let layers = (0..n)
        .map(|_| LayerSpec { mixer: Mixer::Attention(at.clone()), ffn: Ffn::Mlp(plain_mlp(4 * hidden, Act::GeluTanh, true)), residual: pre_norm(norm), post_scale: 1.0 })
        .collect();
    let l = "transformer.h.{L}.";
    let nm = names(&[
        ("embed", "transformer.word_embeddings".into()),
        ("embed_norm", "transformer.word_embeddings_layernorm".into()),
        ("final_norm", "transformer.ln_f".into()),
        ("lm_head", if tied { "transformer.word_embeddings".into() } else { "lm_head".into() }),
        ("norm.mix", format!("{l}input_layernorm")),
        ("norm.ffn", format!("{l}post_attention_layernorm")),
        ("attn.qkv", format!("{l}self_attention.query_key_value")),
        ("attn.o", format!("{l}self_attention.dense")),
        ("mlp.up", format!("{l}mlp.dense_h_to_4h")),
        ("mlp.down", format!("{l}mlp.dense_4h_to_h")),
    ]);
    let mut emb = plain_embedding(hidden);
    emb.norm = Some(norm);
    p.notes.push("BLOOM's gelu uses the constant 0.79788456 for √(2/π); modelled as gelu-tanh (difference < 1e-8 relative)".into());
    Ok(p.finish_spec(SpecParts {
        model_type: "bloom",
        families: vec!["C1"],
        vocab,
        hidden,
        max_pos: None,
        embedding: emb,
        layers,
        final_norm: Some(norm),
        head: plain_head(tied),
        names: nm,
        prefix_aliases: vec![("transformer.".into(), String::new())],
        conv1d: false,
    }))
}

/// MPT (native `MptForCausalLM`, also the remote `MPTForCausalLM` configs): ALiBi, bias-free
/// LayerNorm and projections, fused `Wqkv` with optional clamp, exact GELU, 4× MLP.
pub(crate) fn mpt(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 50368)?;
    let hidden = p.cfg.alias_usize(&["d_model", "hidden_size"])?.unwrap_or(2048);
    let n = p.cfg.alias_usize(&["n_layers", "num_hidden_layers"])?.unwrap_or(24);
    let h = p.cfg.alias_usize(&["n_heads", "num_attention_heads"])?.unwrap_or(16);
    let max_seq = p.cfg.usize_or("max_seq_len", 2048)?;
    let eps = p.cfg.f64_or("layer_norm_epsilon", 1e-5)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", true)?;
    p.cfg.require_eq("expansion_ratio", &serde_json::json!(4), "MptMLP hard-codes 4·hidden")?;
    p.cfg.require_eq("no_bias", &serde_json::json!(true), "MptForCausalLM has no biases")?;
    p.cfg.forbid("logit_scale", "MptForCausalLM does not scale logits")?;
    match p.cfg.opt_str("norm_type")?.as_deref() {
        None | Some("low_precision_layernorm") | Some("layernorm") => {}
        Some(o) => return Err(LowerError::not_lowerable(format!("mpt: norm_type {o}"))),
    }
    if let Some(ffn) = p.cfg.opt_obj("ffn_config")? {
        match ffn.get("ffn_type").and_then(Value::as_str) {
            None | Some("mptmlp") => {}
            Some(o) => return Err(LowerError::not_lowerable(format!("mpt: ffn_type {o}"))),
        }
        if ffn.keys().any(|k| k != "ffn_type") {
            return Err(LowerError::not_lowerable("mpt: ffn_config keys beyond ffn_type"));
        }
    }
    // `learned_pos_emb` has no effect with ALiBi (the native model has no `wpe`); training and
    // placement knobs are inert.
    p.cfg.inert(&["learned_pos_emb", "init_device", "verbose", "embedding_fraction", "init_config", "fc_type", "tokenizer_name", "use_pad_tok_in_ffn"]);
    let mut clip = None;
    let mut softmax_scale = None;
    if let Some(ac) = p.cfg.opt_obj("attn_config")? {
        let ac_cfg = Cfg::new(p.cfg.arch.clone(), ac, "attn_config");
        ac_cfg.inert(&["attn_pdrop", "attn_impl", "attn_uses_sequence_id"]);
        match ac_cfg.opt_str("attn_type")?.as_deref() {
            None | Some("multihead_attention") => {}
            Some(o) => return Err(LowerError::not_lowerable(format!("mpt: attn_type {o} (the native port is MHA only)"))),
        }
        ac_cfg.forbid("prefix_lm", "prefix-LM attention is bidirectional over the prompt")?;
        ac_cfg.forbid("qk_ln", "MptAttention has no QK LayerNorm")?;
        ac_cfg.require_eq("alibi", &serde_json::json!(true), "the native port always adds ALiBi")?;
        ac_cfg.require_eq("alibi_bias_max", &serde_json::json!(8), "the native port always uses alibi_bias_max = 8")?;
        ac_cfg.forbid("rope", "llm-foundry rope variants are not in the native port")?;
        ac_cfg.forbid("sliding_window_size", "not in the native port")?;
        clip = ac_cfg.opt_f64("clip_qkv")?;
        softmax_scale = ac_cfg.opt_f64("softmax_scale")?;
        ac_cfg.finish()?;
    }
    let hd = hidden / h;
    let norm = NormSpec::layer_nobias(eps);
    let mut at = attn(h, h, hd, Position::Alibi(AlibiSpec { slopes: alibi_slopes_mpt(h, 8.0), scaled_by_softmax_scale: false, bf16_bias: false }), (false, false));
    p.layouts.qkv = QkvLayout::FusedConcat;
    at.clip_qkv = clip;
    at.scale = softmax_scale.unwrap_or(1.0 / (hd as f64).sqrt());
    let layers = (0..n)
        .map(|_| LayerSpec { mixer: Mixer::Attention(at.clone()), ffn: Ffn::Mlp(plain_mlp(4 * hidden, Act::Gelu, false)), residual: pre_norm(norm), post_scale: 1.0 })
        .collect();
    let l = "transformer.blocks.{L}.";
    let nm = names(&[
        ("embed", "transformer.wte".into()),
        ("final_norm", "transformer.norm_f".into()),
        ("lm_head", if tied { "transformer.wte".into() } else { "lm_head".into() }),
        ("norm.mix", format!("{l}norm_1")),
        ("norm.ffn", format!("{l}norm_2")),
        ("attn.qkv", format!("{l}attn.Wqkv")),
        ("attn.o", format!("{l}attn.out_proj")),
        ("mlp.up", format!("{l}ffn.up_proj")),
        ("mlp.down", format!("{l}ffn.down_proj")),
    ]);
    Ok(p.finish_spec(SpecParts {
        model_type: "mpt",
        families: vec!["C1"],
        vocab,
        hidden,
        max_pos: Some(max_seq),
        embedding: plain_embedding(hidden),
        layers,
        final_norm: Some(norm),
        head: plain_head(tied),
        names: nm,
        prefix_aliases: vec![],
        conv1d: false,
    }))
}

/// OPT: learned positions at `pos + 2`, pre-LN (or post-LN for OPT-350m), `project_in/out` when
/// the embedding width differs, ReLU MLP.
pub(crate) fn opt(p: &mut P) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 50272)?;
    let hidden = p.cfg.usize_or("hidden_size", 768)?;
    let n = p.cfg.usize_or("num_hidden_layers", 12)?;
    let ffn = p.cfg.usize_or("ffn_dim", 3072)?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 2048)?;
    let pre = p.cfg.bool_or("do_layer_norm_before", true)?;
    let remove_final = p.cfg.bool_or("_remove_final_layer_norm", false)?;
    let wdim = p.cfg.opt_usize("word_embed_proj_dim")?.unwrap_or(hidden);
    let h = p.cfg.usize_or("num_attention_heads", 12)?;
    let act = p.act("activation_function", "relu")?;
    let bias = p.cfg.bool_or("enable_bias", true)?;
    let affine = p.cfg.bool_or("layer_norm_elementwise_affine", true)?;
    let tied = p.cfg.bool_or("tie_word_embeddings", true)?;
    let hd = hidden / h;
    let norm = if affine { NormSpec::layer(1e-5) } else { NormSpec { kind: NormKind::Layer, eps: 1e-5, gain: Gain::None, bias: false } };
    let residual = if pre { pre_norm(norm) } else { Residual::PostNorm { mixer_norm: norm, ffn_norm: norm } };
    let at = attn(h, h, hd, Position::None, (bias, bias));
    let layers = (0..n).map(|_| LayerSpec { mixer: Mixer::Attention(at.clone()), ffn: Ffn::Mlp(plain_mlp(ffn, act, bias)), residual: residual.clone(), post_scale: 1.0 }).collect();
    let l = "model.decoder.layers.{L}.";
    let mut nm = names(&[
        ("embed", "model.decoder.embed_tokens".into()),
        ("pos_embed", "model.decoder.embed_positions".into()),
        ("final_norm", "model.decoder.final_layer_norm".into()),
        ("lm_head", if tied { "model.decoder.embed_tokens".into() } else { "lm_head".into() }),
        ("norm.mix", format!("{l}self_attn_layer_norm")),
        ("norm.ffn", format!("{l}final_layer_norm")),
        ("attn.q", format!("{l}self_attn.q_proj")),
        ("attn.k", format!("{l}self_attn.k_proj")),
        ("attn.v", format!("{l}self_attn.v_proj")),
        ("attn.o", format!("{l}self_attn.out_proj")),
        ("mlp.up", format!("{l}fc1")),
        ("mlp.down", format!("{l}fc2")),
    ]);
    let proj = wdim != hidden;
    if proj {
        nm.insert("proj_in".into(), "model.decoder.project_in".into());
        nm.insert("proj_out".into(), "model.decoder.project_out".into());
    }
    let mut emb = plain_embedding(wdim);
    emb.positions = Some(LearnedPositions { rows: max_pos + 2, offset: 2 });
    emb.proj_in = proj;
    let mut head = plain_head(tied);
    head.proj_out = proj;
    let final_norm = if pre && !remove_final { Some(norm) } else { None };
    Ok(p.finish_spec(SpecParts {
        model_type: "opt",
        families: vec!["C1"],
        vocab,
        hidden,
        max_pos: Some(max_pos),
        embedding: emb,
        layers,
        final_norm,
        head,
        names: nm,
        prefix_aliases: vec![("model.".into(), String::new())],
        conv1d: false,
    }))
}
