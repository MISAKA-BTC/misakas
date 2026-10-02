//! **`palw-tir-check`'s report**: the ArchSpec, the HL block/schedule summary, per-position cost
//! and state estimates, an optional weights check, and the verdict. Gate 1 verdicts are
//! `LOWERABLE_TO_HL` or `NOT_LOWERABLE(reason)`; the consensus verdicts (`ADMISSIBLE`,
//! `EXCEEDS`, `NEEDS_PRIMITIVE`) arrive with TIR expansion and the verifier in Gate 2.

use crate::error::{LowerError, Result};
use crate::hl::{self, HlProgram, cost};
use crate::rope::RopeStyle;
use crate::spec::*;
use crate::weights::{self, Checkpoint, IndexOnly, WeightReport};
use crate::hf_weights;
use std::fmt::Write;
use std::path::Path;

pub enum WeightsArg<'a> {
    /// A `.safetensors` file, a directory, or an index whose shards are present.
    Files(&'a Path),
    /// An index whose shards may be absent: names only.
    IndexNamesOnly(&'a Path),
}

pub struct Checked {
    pub spec: ArchSpec,
    pub program: HlProgram,
    pub cost: cost::CostReport,
    pub weights: Option<WeightReport>,
    /// Pre-quantised projections' TIR structure (`LowerOpts::quant`).
    pub quant: std::collections::BTreeMap<u32, crate::prequant::QLayout>,
}

pub fn check(config_text: &str, weights: Option<WeightsArg>) -> Result<Checked> {
    check_read(config_text, &crate::hf_schema::ReadOptions::default(), weights)
}

/// [`check`] reading the config through the adapter `read` names (a user-supplied adapter file, a
/// built-in by id, none).
pub fn check_read(config_text: &str, read: &crate::hf_schema::ReadOptions, weights: Option<WeightsArg>) -> Result<Checked> {
    let spec = crate::hf_config::parse_config_str_read(config_text, read, crate::quantfmt::QuantRegistry::builtin())?;
    let program = hl::build_program(&spec)?;
    let binding = hf_weights::bind(&spec, &program)?;
    let cost = cost::estimate(&program);
    let weights = match weights {
        None => None,
        Some(WeightsArg::Files(p)) => {
            let ck = Checkpoint::open(p)?;
            Some(weights::check_weights(&program, &binding, &ck))
        }
        Some(WeightsArg::IndexNamesOnly(p)) => {
            let idx = IndexOnly::open(p)?;
            Some(weights::check_names(&program, &binding, &idx.0))
        }
    };
    let quant = weights::quant_layouts(&program, &binding)?;
    Ok(Checked { spec, program, cost, weights, quant })
}

fn human(n: u64) -> String {
    let f = n as f64;
    if f >= 1e12 {
        format!("{:.2}T", f / 1e12)
    } else if f >= 1e9 {
        format!("{:.2}G", f / 1e9)
    } else if f >= 1e6 {
        format!("{:.2}M", f / 1e6)
    } else if f >= 1e3 {
        format!("{:.1}K", f / 1e3)
    } else {
        format!("{n}")
    }
}

fn norm(n: &NormSpec) -> String {
    n.short()
}

fn position(p: &Position) -> String {
    match p {
        Position::None => "no positional term (NoPE)".into(),
        Position::Alibi(a) => format!(
            "ALiBi ({} slopes{}{})",
            a.slopes.len(),
            if a.scaled_by_softmax_scale { ", scaled with the scores" } else { "" },
            if a.bf16_bias { ", bf16-rounded bias" } else { "" }
        ),
        Position::Rope(r) => {
            let f = &r.freqs;
            let mut s = format!(
                "RoPE {} θ={} rotary {}/{}{} {}",
                f.rope_type,
                f.theta,
                r.rotary_dim,
                f.dim,
                if r.offset > 0 { format!(" at offset {}", r.offset) } else { String::new() },
                match r.style {
                    RopeStyle::Half => "rotate_half",
                    RopeStyle::Interleaved => "interleaved",
                }
            );
            if f.attention_factor != 1.0 {
                let _ = write!(s, " cos/sin×{:.6}", f.attention_factor);
            }
            if let Some(d) = &f.dynamic {
                let _ = write!(s, " dynamic-NTK factor {} beyond {}", d.factor, d.max_pos);
            }
            if let Some(l) = &f.longrope {
                let _ = write!(s, " long factors beyond {}", l.original_max);
            }
            s
        }
    }
}

fn mixer(m: &Mixer) -> String {
    match m {
        Mixer::Attention(a) => {
            let mut s = format!("attention {}q/{}kv × {}", a.heads, a.kv_heads, a.head_dim);
            if a.v_head_dim != a.head_dim {
                let _ = write!(s, " (v {})", a.v_head_dim);
            }
            let _ = write!(s, ", scale {:.6}, {}", a.scale, position(&a.position));
            if let Some(w) = a.window {
                let _ = write!(s, ", window {w}");
            }
            if let Some(c) = a.softcap {
                let _ = write!(s, ", score soft-cap {c}");
            }
            if let Some(q) = &a.qk_norm {
                let _ = write!(s, ", QK-norm {:?} {}", q.scope, norm(&q.norm));
            }
            if let Some(c) = a.clip_qkv {
                let _ = write!(s, ", clip_qkv {c}");
            }
            if a.sinks {
                s.push_str(", sinks");
            }
            if a.output_gate {
                s.push_str(", sigmoid output gate");
            }
            let b: Vec<&str> =
                [(a.q_bias, "q"), (a.k_bias, "k"), (a.v_bias, "v"), (a.o_bias, "o")].iter().filter(|x| x.0).map(|x| x.1).collect();
            if !b.is_empty() {
                let _ = write!(s, ", bias on {}", b.join(","));
            }
            s
        }
        Mixer::Mla(m) => format!(
            "MLA {} heads, q_lora {:?}, kv_lora {}, nope {} + rope {} (v {}), scale {:.6}, {}",
            m.heads,
            m.q_lora_rank,
            m.kv_lora_rank,
            m.qk_nope_head_dim,
            m.qk_rope_head_dim,
            m.v_head_dim,
            m.scale,
            position(&Position::Rope(m.rope.clone()))
        ),
        Mixer::GatedDeltaNet(g) => format!(
            "gated delta net {} k-heads × {} / {} v-heads × {}, conv {}, head map {:?}",
            g.k_heads, g.k_dim, g.v_heads, g.v_dim, g.conv_kernel, g.head_map
        ),
        Mixer::Mamba(m) => format!(
            "Mamba inner {} state {} conv {} dt_rank {}{}",
            m.inner,
            m.state,
            m.conv_kernel,
            m.dt_rank,
            if m.bcdt_norm.is_some() { ", RMS-normed dt/B/C" } else { "" }
        ),
        Mixer::Mamba2(m) => {
            let norm = match m.norm_mode {
                Mamba2Norm::GateFirst => "",
                Mamba2Norm::NormFirst => ", norm before gate",
                Mamba2Norm::Ungated => ", gate only (no norm)",
            };
            format!(
                "Mamba2 {} heads × {} state {} groups {} conv {}{norm}{}",
                m.heads,
                m.head_dim,
                m.state,
                m.groups,
                m.conv_kernel,
                if m.chunk_scales.is_some() { ", muP chunk scales" } else { "" }
            )
        }
        Mixer::RwkvTime(r) => format!("RWKV-{} time mix, attention dim {}", r.version, r.attn_dim),
        Mixer::ShortConv(c) => format!("gated short convolution, kernel {}", c.kernel),
        Mixer::None => "none (the layer is its FFN)".into(),
        Mixer::Parallel(bs) => {
            format!("parallel: {}", bs.iter().map(|b| format!("[{}]×{}→×{}", mixer(&b.mixer), b.in_scale, b.out_scale)).collect::<Vec<_>>().join(" + "))
        }
    }
}

fn ffn(f: &Ffn) -> String {
    match f {
        Ffn::None => "none".into(),
        Ffn::Mlp(m) => format!(
            "{} MLP {} {:?}{}",
            if m.gated { "gated" } else { "plain" },
            m.intermediate,
            m.act,
            if m.up_bias { " +bias" } else { "" }
        ),
        Ffn::Moe(m) => {
            let r = &m.router;
            let mut s = format!("MoE {} experts top-{} × {} {:?}, router {:?}", m.experts, m.top_k, m.intermediate, m.act, r.scoring);
            if let Some(g) = &r.groups {
                let _ = write!(s, " groups {}→{} by {:?}", g.n_group, g.topk_group, g.score);
            }
            if r.selection_bias {
                s.push_str(" +selection bias");
            }
            if r.normalize {
                s.push_str(", renormalised");
            }
            if r.scale != 1.0 {
                let _ = write!(s, ", ×{}", r.scale);
            }
            if let Some(sh) = &m.shared {
                let _ = write!(s, ", shared expert {}{}", sh.intermediate, if sh.sigmoid_gate { " (sigmoid-gated)" } else { "" });
            }
            if let Glu::ClampedSwiGlu { alpha, limit } = m.glu {
                let _ = write!(s, ", clamped SwiGLU α={alpha} limit={limit}");
            }
            if !m.gated {
                s.push_str(", plain experts (no gate)");
            }
            if let Some(l) = m.latent {
                let _ = write!(s, ", latent {l}");
            }
            s
        }
        Ffn::RwkvChannel(c) => format!("RWKV-{} channel mix {}", c.version, c.intermediate),
        Ffn::MlpMoe(mm) => format!("{} beside {}", ffn(&Ffn::Mlp(mm.mlp.clone())), ffn(&Ffn::Moe(mm.moe.clone()))),
    }
}

fn residual(r: &Residual) -> String {
    match r {
        Residual::Sequential { pre_mixer, post_mixer, pre_ffn, post_ffn, multiplier } => {
            let o = |n: &Option<NormSpec>| n.as_ref().map(norm).unwrap_or_else(|| "–".into());
            let mut s =
                format!("sequential pre {} / post {} | pre {} / post {}", o(pre_mixer), o(post_mixer), o(pre_ffn), o(post_ffn));
            if *multiplier != 1.0 {
                let _ = write!(s, ", branch ×{multiplier}");
            }
            s
        }
        Residual::Parallel { norm: n, ffn_norm } => {
            format!("parallel ({}{})", norm(n), ffn_norm.as_ref().map(|f| format!(" / {}", norm(f))).unwrap_or_default())
        }
        Residual::PostNorm { mixer_norm, .. } => format!("post-LN ({})", norm(mixer_norm)),
        Residual::Sandwich { pre_mixer, ple, layer_scalar, .. } => format!(
            "sandwich ({}){}{}",
            norm(pre_mixer),
            ple.as_ref().map(|p| format!(", per-layer input {}", p.dim)).unwrap_or_default(),
            if *layer_scalar { ", × layer scalar" } else { "" }
        ),
        Residual::HyperConnection { ple } => format!(
            "hyper-connections{}",
            ple.as_ref().map(|p| format!(", n-gram per-layer embedding ({}-grams × {} heads)", p.ngram_size, p.heads_per_ngram)).unwrap_or_default()
        ),
    }
}

pub fn render(c: &Checked) -> String {
    let s = &c.spec;
    let mut o = String::new();
    let _ = writeln!(o, "architecture   {} (model_type {})", s.architecture, s.model_type);
    let _ = writeln!(o, "reference      {:?}", s.reference);
    match &s.confidence {
        Confidence::Known => {
            let _ = writeln!(o, "semantics      transformers 5.17 modeling code (confirmed by fixtures for this family)");
        }
        Confidence::Unsure(u) => {
            let _ = writeln!(o, "semantics      UNVERIFIED — confirm with a fixture:");
            for x in u {
                let _ = writeln!(o, "                 • {x}");
            }
        }
    }
    let _ = writeln!(o, "corpus         {}", s.families.join(" "));
    let _ = writeln!(
        o,
        "dims           vocab {} · hidden {} · layers {} · max positions {:?}",
        s.vocab_size,
        s.hidden_size,
        s.num_layers(),
        s.max_position_embeddings
    );
    let e = &s.embedding;
    let _ = writeln!(
        o,
        "embedding      width {}{}{}{}{}",
        e.dim,
        if e.scale != 1.0 { format!(", ×{:.6}", e.scale) } else { String::new() },
        e.positions.as_ref().map(|p| format!(", learned positions {} (+{})", p.rows, p.offset)).unwrap_or_default(),
        e.norm.as_ref().map(|n| format!(", {}", norm(n))).unwrap_or_default(),
        if e.proj_in { ", project_in" } else { "" }
    );
    let h = &s.head;
    let _ = writeln!(
        o,
        "head           {}{}{}{}{}",
        if h.tied { "tied to the embedding" } else { "own weights" },
        if h.bias { ", bias" } else { "" },
        if h.pre_scale != 1.0 { format!(", input ×{:.6}", h.pre_scale) } else { String::new() },
        if h.logit_scale != 1.0 { format!(", logits ×{:.6}", h.logit_scale) } else { String::new() },
        h.softcap.map(|c| format!(", logit soft-cap {c}")).unwrap_or_default()
    );
    if let Some(n) = &s.final_norm {
        let _ = writeln!(o, "final norm     {}", norm(n));
    }
    let _ = writeln!(o, "layer kinds");
    let mut seen: Vec<(&LayerSpec, Vec<usize>)> = Vec::new();
    for (i, l) in s.layers.iter().enumerate() {
        match seen.iter_mut().find(|(x, _)| *x == l) {
            Some((_, v)) => v.push(i),
            None => seen.push((l, vec![i])),
        }
    }
    for (k, (l, idx)) in seen.iter().enumerate() {
        let ls = if idx.len() > 6 { format!("{} layers: {:?}…", idx.len(), &idx[..6]) } else { format!("layers {idx:?}") };
        let _ = writeln!(o, "  [{k}] {ls}");
        let _ = writeln!(o, "      mixer    {}", mixer(&l.mixer));
        let _ = writeln!(o, "      ffn      {}", ffn(&l.ffn));
        let _ = writeln!(
            o,
            "      residual {}{}",
            residual(&l.residual),
            if l.post_scale != 1.0 { format!(", output ×{}", l.post_scale) } else { String::new() }
        );
    }
    for n in &s.notes {
        let _ = writeln!(o, "note           {n}");
    }
    o.push('\n');
    o.push_str(&c.program.summary());
    let cr = &c.cost;
    let hmax = s.max_position_embeddings.unwrap_or(32768).max(1);
    let _ = writeln!(o, "\nper-position estimates (HL level; exact costs come from the TIR expansion)");
    let _ = writeln!(o, "  params          {}", human(cr.param_elems));
    let _ = writeln!(
        o,
        "  MACs            {} at H=1 · {} at H=4096 · {} at H={hmax}",
        human(cr.macs_at(1)),
        human(cr.macs_at(4096)),
        human(cr.macs_at(hmax))
    );
    if cr.routed_expert_macs > 0 {
        let _ = writeln!(o, "                  of which routed experts (active top-k only): {}", human(cr.routed_expert_macs));
    }
    let _ = writeln!(o, "  elementwise     {} ({} transcendental)", human(cr.elementwise), human(cr.transcendental));
    let _ = writeln!(o, "  Fixed state     {} elements over all layers", human(cr.fixed_state_elems));
    let _ = writeln!(
        o,
        "  Hist per pos    {} elements over all layers ({} B/position at i8, {} B at i16){}",
        human(cr.hist_elems_per_pos),
        human(cr.hist_elems_per_pos),
        human(2 * cr.hist_elems_per_pos),
        if cr.windowed.is_empty() {
            String::new()
        } else {
            format!("; windowed layers keep ≤ window rows {:?}", cr.windowed.iter().map(|w| w.0).collect::<Vec<_>>())
        }
    );
    if let Some(w) = &c.weights {
        let _ = writeln!(
            o,
            "\nweights         {} bindings checked, {} errors, {} checkpoint tensors unused",
            w.bound,
            w.errors.len(),
            w.unused.len()
        );
        for e in w.errors.iter().take(20) {
            let _ = writeln!(o, "  error  {e}");
        }
        for u in w.unused.iter().take(20) {
            let _ = writeln!(o, "  unused {u}");
        }
    }
    let _ = writeln!(o, "\n{}", verdict(c));
    o
}

pub fn verdict(c: &Checked) -> String {
    if let Some(w) = &c.weights
        && !w.errors.is_empty()
    {
        return format!("NOT_LOWERABLE(weights do not match the config: {})", w.errors[0]);
    }
    match &c.spec.confidence {
        Confidence::Known => {
            "LOWERABLE_TO_HL (Gate 1: ADMISSIBLE / EXCEEDS / NEEDS_PRIMITIVE come with TIR expansion and the verifier)".into()
        }
        Confidence::Unsure(u) => format!("LOWERABLE_TO_HL_UNVERIFIED ({} semantic point(s) to confirm with a fixture)", u.len()),
    }
}

pub fn refusal(e: &LowerError) -> String {
    match e {
        LowerError::NotLowerable(s) => format!("NOT_LOWERABLE({s})"),
        other => format!("ERROR({other})"),
    }
}
