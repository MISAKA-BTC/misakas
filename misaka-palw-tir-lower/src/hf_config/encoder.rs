//! Encoders (RFC-0003 Part II.3, the Embedding profile): programs whose `post` produces an
//! embedding instead of logits.
//!
//! * **CLIP's text tower** (`CLIPTextModel`, `CLIPTextModelWithProjection`) is causal: a GPT-2-like
//!   pre-LN decoder with learned positions and `quick_gelu`, read as a scan over `bos ‖ prompt ‖ eos`.
//!   Its pooled output is the final-LN row at the first `eos`, which is the last position of that
//!   sequence, so it is a `Final` output. `text_projection` follows when the class has one.

use super::*;

/// CLIP's text tower. The attention mask is causal (`_create_4d_causal_attention_mask`), so a
/// per-position scan is the model exactly. `eos_token_id = 2` is the legacy configuration, whose
/// pooling reads `argmax(input_ids)`; the pipeline's token rule makes the `eos` the last id either
/// way.
pub(crate) fn clip_text(p: &mut P, projection: bool) -> Result<ArchSpec> {
    let vocab = p.cfg.usize_or("vocab_size", 49408)?;
    let hidden = p.cfg.usize_or("hidden_size", 512)?;
    let inter = p.cfg.usize_or("intermediate_size", 2048)?;
    let n = p.cfg.usize_or("num_hidden_layers", 12)?;
    let h = p.cfg.usize_or("num_attention_heads", 8)?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", 77)?;
    let act = p.act("hidden_act", "quick_gelu")?;
    let eps = p.cfg.f64_or("layer_norm_eps", 1e-5)?;
    let proj_dim = p.cfg.usize_or("projection_dim", 512)?;
    // Token ids and dropout: the job's template carries bos/eos (class data), dropout is off in eval.
    p.cfg.inert(&[
        "bos_token_id",
        "eos_token_id",
        "pad_token_id",
        "attention_dropout",
        "dropout",
        "initializer_range",
        "initializer_factor",
        "projection_dim",
    ]);
    if hidden % h != 0 {
        return Err(LowerError::bad(format!("clip_text: hidden_size {hidden} not divisible by {h} heads")));
    }
    let hd = hidden / h;
    let norm = NormSpec::layer(eps);
    let layers = (0..n)
        .map(|_| {
            let mut at = attn(h, h, hd, Position::None, (true, true));
            at.scale = 1.0 / (hd as f64).sqrt();
            LayerSpec {
                mixer: Mixer::Attention(at),
                ffn: Ffn::Mlp(plain_mlp(inter, act, true)),
                residual: pre_norm(norm),
                post_scale: 1.0,
            }
        })
        .collect();
    let l = "text_model.encoder.layers.{L}.";
    let mut nm = names(&[
        ("embed", "text_model.embeddings.token_embedding".into()),
        ("pos_embed", "text_model.embeddings.position_embedding".into()),
        ("final_norm", "text_model.final_layer_norm".into()),
        ("norm.mix", format!("{l}layer_norm1")),
        ("norm.ffn", format!("{l}layer_norm2")),
        ("attn.q", format!("{l}self_attn.q_proj")),
        ("attn.k", format!("{l}self_attn.k_proj")),
        ("attn.v", format!("{l}self_attn.v_proj")),
        ("attn.o", format!("{l}self_attn.out_proj")),
        ("mlp.up", format!("{l}mlp.fc1")),
        ("mlp.down", format!("{l}mlp.fc2")),
    ]);
    if projection {
        nm.insert("embed_proj".into(), "text_projection".into());
    }
    let mut emb = plain_embedding(hidden);
    emb.positions = Some(LearnedPositions { rows: max_pos, offset: 0 });
    let mut spec = p.finish_spec(SpecParts {
        model_type: "clip_text_model",
        families: vec!["E1"],
        vocab,
        hidden,
        max_pos: Some(max_pos),
        embedding: emb,
        layers,
        final_norm: Some(norm),
        head: plain_head(false),
        names: nm,
        // `CLIPModel` checkpoints hold the text tower under the same names, beside the vision one.
        prefix_aliases: vec![],
        conv1d: false,
    });
    spec.output = OutputSpec::Embedding { proj: projection.then_some((proj_dim, false)), normalize: false };
    Ok(spec)
}
