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

/// The BERT lineage the bidirectional lowering (`lower::bidir`) models.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BertFlavor {
    /// BERT: positions from 0.
    Bert,
    /// RoBERTa and XLM-R: positions from `padding_idx + 1`
    /// (`create_position_ids_from_input_ids`; a pad slot's position is masked out anyway).
    Roberta,
    /// MPNet: RoBERTa's positions (`padding_idx` hard-coded 1), no token types, and a bias over
    /// bucketed relative positions (T5's bidirectional buckets, 32 of them up to 128) shared by every
    /// layer.
    MPNet,
    /// DistilBERT: BERT without token types, under its own names (`dim`, `n_layers`, …).
    DistilBert,
}

/// BERT, RoBERTa, XLM-R, MPNet and DistilBERT encoders: post-LN layers over word + position
/// (+ token-type-0) embeddings and an embedding LayerNorm. Bidirectional: they lower through
/// `lower::bidir` as one position over a padded token axis. The spec's layers are those of the
/// per-position view, whose params and binding the bidirectional program shares.
pub(crate) fn bert_like(p: &mut P, flavor: BertFlavor) -> Result<ArchSpec> {
    let distil = flavor == BertFlavor::DistilBert;
    let key = |bert: &'static str, distil_key: &'static str| if distil { distil_key } else { bert };
    let vocab = p.cfg.usize_or("vocab_size", if flavor == BertFlavor::MPNet { 30527 } else { 30522 })?;
    let hidden = p.cfg.usize_or(key("hidden_size", "dim"), 768)?;
    let inter = p.cfg.usize_or(key("intermediate_size", "hidden_dim"), 3072)?;
    let n = p.cfg.usize_or(key("num_hidden_layers", "n_layers"), if distil { 6 } else { 12 })?;
    let h = p.cfg.usize_or(key("num_attention_heads", "n_heads"), 12)?;
    let max_pos = p.cfg.usize_or("max_position_embeddings", if flavor == BertFlavor::MPNet { 514 } else { 512 })?;
    let types = match flavor {
        BertFlavor::Bert | BertFlavor::Roberta => Some(p.cfg.usize_or("type_vocab_size", 2)?),
        BertFlavor::MPNet | BertFlavor::DistilBert => None,
    };
    let act = p.act(key("hidden_act", "activation"), "gelu")?;
    // DistilBERT's LayerNorms are hard-coded at 1e-12.
    let eps = if distil { 1e-12 } else { p.cfg.f64_or("layer_norm_eps", if flavor == BertFlavor::MPNet { 1e-5 } else { 1e-12 })? };
    let pad = match flavor {
        // MPNetEmbeddings hard-codes `padding_idx = 1`, whatever the config says.
        BertFlavor::MPNet => 1,
        _ => p.cfg.usize_or("pad_token_id", if flavor == BertFlavor::Bert || distil { 0 } else { 1 })?,
    };
    if p.cfg.bool_or("is_decoder", false)? {
        return Err(LowerError::not_lowerable(format!("{}: is_decoder (a causal BERT) is not modelled", p.cfg.arch)));
    }
    if let Some(t) = p.cfg.opt_str("position_embedding_type")?
        && t != "absolute"
    {
        return Err(LowerError::not_lowerable(format!("{}: position_embedding_type `{t}` is not modelled", p.cfg.arch)));
    }
    let rel_bias = if flavor == BertFlavor::MPNet {
        // `compute_position_bias` always buckets into 32 (its default argument), reading rows of a
        // `relative_attention_num_buckets`-row table.
        let rows = p.cfg.usize_or("relative_attention_num_buckets", 32)?;
        if rows != 32 {
            return Err(LowerError::not_lowerable(format!(
                "{}: relative_attention_num_buckets {rows} (MPNet buckets into 32 whatever the table's size)",
                p.cfg.arch
            )));
        }
        Some(crate::spec::RelBiasSpec { buckets: 32, max_distance: 128, heads: h })
    } else {
        None
    };
    p.cfg.inert(&[
        "attention_probs_dropout_prob",
        "hidden_dropout_prob",
        "classifier_dropout",
        "initializer_range",
        "use_cache",
        "bos_token_id",
        "eos_token_id",
        "tie_word_embeddings",
        "position_embedding_type",
        "is_decoder",
        // DistilBERT's training and head knobs; `sinusoidal_pos_embds` only initialises the table,
        // which the checkpoint carries either way.
        "dropout",
        "attention_dropout",
        "qa_dropout",
        "seq_classif_dropout",
        "tie_weights_",
        "sinusoidal_pos_embds",
    ]);
    if hidden % h != 0 {
        return Err(LowerError::bad(format!("{}: hidden_size {hidden} not divisible by {h} heads", p.cfg.arch)));
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
                residual: Residual::PostNorm { mixer_norm: norm, ffn_norm: norm },
                post_scale: 1.0,
            }
        })
        .collect();
    let nm = match flavor {
        BertFlavor::Bert | BertFlavor::Roberta => {
            let l = "encoder.layer.{L}.";
            names(&[
                ("embed", "embeddings.word_embeddings".into()),
                ("pos_embed", "embeddings.position_embeddings".into()),
                ("type_embed", "embeddings.token_type_embeddings".into()),
                ("embed_norm", "embeddings.LayerNorm".into()),
                ("attn.q", format!("{l}attention.self.query")),
                ("attn.k", format!("{l}attention.self.key")),
                ("attn.v", format!("{l}attention.self.value")),
                ("attn.o", format!("{l}attention.output.dense")),
                ("norm.mix", format!("{l}attention.output.LayerNorm")),
                ("mlp.up", format!("{l}intermediate.dense")),
                ("mlp.down", format!("{l}output.dense")),
                ("norm.ffn", format!("{l}output.LayerNorm")),
            ])
        }
        BertFlavor::MPNet => {
            let l = "encoder.layer.{L}.";
            names(&[
                ("embed", "embeddings.word_embeddings".into()),
                ("pos_embed", "embeddings.position_embeddings".into()),
                ("embed_norm", "embeddings.LayerNorm".into()),
                ("rel_bias", "encoder.relative_attention_bias".into()),
                ("attn.q", format!("{l}attention.attn.q")),
                ("attn.k", format!("{l}attention.attn.k")),
                ("attn.v", format!("{l}attention.attn.v")),
                ("attn.o", format!("{l}attention.attn.o")),
                ("norm.mix", format!("{l}attention.LayerNorm")),
                ("mlp.up", format!("{l}intermediate.dense")),
                ("mlp.down", format!("{l}output.dense")),
                ("norm.ffn", format!("{l}output.LayerNorm")),
            ])
        }
        BertFlavor::DistilBert => {
            let l = "transformer.layer.{L}.";
            names(&[
                ("embed", "embeddings.word_embeddings".into()),
                ("pos_embed", "embeddings.position_embeddings".into()),
                ("embed_norm", "embeddings.LayerNorm".into()),
                ("attn.q", format!("{l}attention.q_lin")),
                ("attn.k", format!("{l}attention.k_lin")),
                ("attn.v", format!("{l}attention.v_lin")),
                ("attn.o", format!("{l}attention.out_lin")),
                ("norm.mix", format!("{l}sa_layer_norm")),
                ("mlp.up", format!("{l}ffn.lin1")),
                ("mlp.down", format!("{l}ffn.lin2")),
                ("norm.ffn", format!("{l}output_layer_norm")),
            ])
        }
    };
    let offset = match flavor {
        BertFlavor::Bert | BertFlavor::DistilBert => 0,
        BertFlavor::Roberta | BertFlavor::MPNet => pad + 1,
    };
    if offset >= max_pos {
        return Err(LowerError::bad(format!("{}: position offset {offset} leaves no position", p.cfg.arch)));
    }
    let mut emb = plain_embedding(hidden);
    emb.positions = Some(LearnedPositions { rows: max_pos, offset });
    emb.norm = Some(norm);
    emb.type_rows = types;
    emb.rel_bias = rel_bias;
    let model_type = match flavor {
        BertFlavor::Bert => "bert",
        BertFlavor::Roberta => "roberta",
        BertFlavor::MPNet => "mpnet",
        BertFlavor::DistilBert => "distilbert",
    };
    // A task head or the pooler (`pooler.dense`, `cls.*`, `lm_head.*`, DistilBERT's `vocab_*`) is
    // not part of the encoder; a `…ForMaskedLM` checkpoint nests the encoder under its own prefix.
    p.ignored_prefixes.extend(
        ["pooler.", "cls.", "lm_head.", "vocab_transform.", "vocab_layer_norm.", "vocab_projector."].map(String::from),
    );
    let aliases: Vec<(String, String)> = match flavor {
        BertFlavor::MPNet => vec![("".into(), "mpnet.".into())],
        BertFlavor::DistilBert => vec![("".into(), "distilbert.".into())],
        _ => vec![],
    };
    let mut spec = p.finish_spec(SpecParts {
        model_type,
        families: vec!["E2"],
        vocab,
        hidden,
        max_pos: Some(max_pos - offset),
        embedding: emb,
        layers,
        final_norm: None,
        head: plain_head(false),
        names: nm,
        prefix_aliases: aliases,
        conv1d: false,
    });
    spec.output = OutputSpec::Embedding { proj: None, normalize: false };
    Ok(spec)
}
