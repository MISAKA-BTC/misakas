//! **The Hugging Face weight-name mapping**: every HL param ← an expression over HF tensors.
//!
//! This is the only place (with `crate::hf_config`) that knows HF tensor names and fused-weight
//! layouts: `qkv_proj` row blocks, NeoX/BLOOM per-head `[q,k,v]` interleaving, Falcon/InternLM2
//! kv-group blocks, Qwen3-Next's per-key-head `in_proj_qkvz`, gpt-oss's interleaved
//! `gate_up_proj` stored `[E, in, out]`, GPT-2 `Conv1D`, `A_log → −exp(A_log)`, RWKV's inference
//! rescale. The HL graph only names math-level roles; a GGUF or ONNX importer would provide its
//! own mapping onto the same roles.

use crate::error::{LowerError, Result};
use crate::hl::HlProgram;
use crate::spec::*;
use crate::weights::{Binding, MapFn, Pick, Src};
use std::collections::BTreeMap;

struct M<'a> {
    st: &'a HfStorage,
    out: BTreeMap<String, Src>,
}

impl M<'_> {
    fn role(&self, role: &str) -> Result<String> {
        self.st
            .name(role)
            .map(str::to_string)
            .ok_or_else(|| LowerError::eval(format!("internal: no HF tensor name for role `{role}`")))
    }
    fn put(&mut self, name: impl Into<String>, src: Src) -> Result<()> {
        let name = name.into();
        if let Some(old) = self.out.get(&name)
            && *old != src
        {
            return Err(LowerError::eval(format!("internal: param `{name}` bound two ways")));
        }
        self.out.insert(name, src);
        Ok(())
    }
    /// The `[out, in]` weight of a linear role (GPT-2's `Conv1D` stores `[in, out]`); a
    /// pre-quantised checkpoint's projection reads its stored integers ([`Src::Quant`]).
    fn w(&self, role: &str) -> Result<Src> {
        if let Some(fmt) = self.quantised(role)? {
            return Ok(Src::Quant { module: self.role(role)?, fmt });
        }
        let t = Src::t(format!("{}.weight", self.role(role)?));
        Ok(if self.st.conv1d_weights { t.transpose() } else { t })
    }
    /// The format a linear role is stored in, when the checkpoint is pre-quantised and the role is
    /// one of a block's projections the config converts (`crate::prequant::QuantConfig`).
    fn quantised(&self, role: &str) -> Result<Option<crate::prequant::QFormat>> {
        const PROJECTIONS: &[&str] = &[
            "attn.q",
            "attn.k",
            "attn.v",
            "attn.qkv",
            "attn.o",
            "mlp.gate",
            "mlp.up",
            "mlp.gate_up",
            "mlp.down",
            "moe.gate",
            "moe.up",
            "moe.down",
            "moe.shared.gate",
            "moe.shared.up",
            "moe.shared.down",
        ];
        let Some(q) = &self.st.quant else { return Ok(None) };
        // GGUF: every tensor has its own type; the modules stored block-quantised are listed.
        if let crate::prequant::QFormat::Gguf { .. } = q.fmt {
            return Ok(match self.st.name(role) {
                Some(t) => q.per_module.get(t).map(|layout| crate::prequant::QFormat::Gguf { layout: *layout }),
                None => None,
            });
        }
        if role == "lm_head" {
            return Ok(q.lm_head.then(|| q.fmt.clone()));
        }
        if !PROJECTIONS.contains(&role) {
            return Ok(None);
        }
        let t = self.role(role)?;
        let (a, b) = (q.converts(&t.replace("{L}", "0")), q.converts(&t.replace("{L}", "1")));
        if a != b {
            return Err(LowerError::not_lowerable(format!("`{t}`: quantised in some layers and not in others")));
        }
        Ok(a.then(|| q.fmt.clone()))
    }
    fn b(&self, role: &str) -> Result<Src> {
        Ok(Src::t(format!("{}.bias", self.role(role)?)))
    }
    /// A plain linear: HL `name.w`/`name.b` ← HF `role.weight`/`role.bias`.
    fn lin(&mut self, name: &str, role: &str, bias: bool) -> Result<()> {
        let w = self.w(role)?;
        self.put(format!("{name}.w"), w)?;
        if bias {
            let b = self.b(role)?;
            self.put(format!("{name}.b"), b)?;
        }
        Ok(())
    }
    /// A norm: HL `name.gain`/`name.bias` ← HF `role.weight`/`role.bias`.
    fn norm(&mut self, name: &str, role: &str, spec: &NormSpec) -> Result<()> {
        if spec.gain != Gain::None {
            let r = self.role(role)?;
            self.put(format!("{name}.gain"), Src::t(format!("{r}.weight")))?;
        }
        if spec.bias {
            let r = self.role(role)?;
            self.put(format!("{name}.bias"), Src::t(format!("{r}.bias")))?;
        }
        Ok(())
    }
}

/// Map every param of `prog` (built from `spec`) onto HF tensors.
pub fn bind(spec: &ArchSpec, prog: &HlProgram) -> Result<Binding> {
    let mut m = M { st: &spec.hf, out: BTreeMap::new() };
    // Embedding, positions, head.
    m.put("embed.table", Src::t(format!("{}.weight", m.role("embed")?)))?;
    if spec.embedding.proj_in {
        let w = m.w("proj_in")?;
        m.put("embed.proj_in.w", w)?;
    }
    if spec.embedding.positions.is_some() {
        m.put("embed.pos_table", Src::t(format!("{}.weight", m.role("pos_embed")?)))?;
    }
    if let Some(n) = &spec.embedding.norm {
        m.norm("embed.norm", "embed_norm", n)?;
    }
    if spec.embedding.type_rows.is_some() {
        m.put("embed.type_table", Src::t(format!("{}.weight", m.role("type_embed")?)))?;
    }
    if spec.embedding.rel_bias.is_some() {
        m.put("attn.rel_bias", Src::t(format!("{}.weight", m.role("rel_bias")?)))?;
    }
    if let Some(n) = &spec.final_norm {
        m.norm("final_norm", "final_norm", n)?;
    }
    let logits = matches!(spec.output, OutputSpec::Logits);
    if let OutputSpec::Embedding { proj: Some((_, bias)), .. } = spec.output {
        let w = m.w("embed_proj")?;
        m.put("embed.proj.w", w)?;
        if bias {
            m.put("embed.proj.b", Src::t(format!("{}.bias", m.role("embed_proj")?)))?;
        }
    }
    if logits && spec.head.proj_out {
        let w = m.w("proj_out")?;
        m.put("head.proj_out.w", w)?;
    }
    if logits && !spec.head.tied {
        let w = match m.quantised("lm_head")? {
            Some(fmt) => Src::Quant { module: m.role("lm_head")?, fmt },
            None => Src::t(format!("{}.weight", m.role("lm_head")?)),
        };
        m.put("head.w", w)?;
    }
    if logits && spec.head.bias {
        let b = match spec.hf.name("lm_head_bias") {
            Some(n) => Src::t(n),
            None => Src::t(format!("{}.bias", m.role("lm_head")?)),
        };
        m.put("head.b", b)?;
    }
    let rescale = spec.layers.iter().position(|l| l.post_scale != 1.0).map(|i| i + 1);
    let mut seen: Vec<&LayerSpec> = Vec::new();
    for ls in &spec.layers {
        if seen.contains(&ls) {
            continue;
        }
        seen.push(ls);
        layer(&mut m, spec, ls, rescale)?;
    }
    // A LoRA adapter's A and B, from the adapter's own tensors (`base_model.model.<module>.lora_*`).
    if let Some(ad) = &spec.adapter {
        for d in &prog.params {
            for (suffix, b) in [(".lora_a", false), (".lora_b", true)] {
                if let Some(role) = d.name.strip_suffix(suffix) {
                    let t = ad.tensor(role, b).ok_or_else(|| LowerError::eval(format!("internal: adapter role `{role}` has no module")))?;
                    m.put(d.name.clone(), Src::t(t))?;
                }
            }
        }
    }
    let mut srcs = Vec::with_capacity(prog.params.len());
    for d in &prog.params {
        srcs.push(
            m.out
                .get(&d.name)
                .cloned()
                .ok_or_else(|| LowerError::eval(format!("internal: HL param `{}` has no HF source", d.name)))?,
        );
    }
    Ok(Binding { srcs, aliases: spec.hf.prefix_aliases.clone(), ignored_prefixes: spec.hf.ignored_prefixes.clone() })
}

fn layer(m: &mut M, spec: &ArchSpec, ls: &LayerSpec, rescale: Option<usize>) -> Result<()> {
    match &ls.residual {
        Residual::Sequential { pre_mixer, post_mixer, pre_ffn, post_ffn, .. } => {
            for (n, name) in
                [(pre_mixer, "norm.mix"), (post_mixer, "norm.post_mix"), (pre_ffn, "norm.ffn"), (post_ffn, "norm.post_ffn")]
            {
                if let Some(n) = n {
                    m.norm(name, name, n)?;
                }
            }
        }
        Residual::Parallel { norm, ffn_norm } => {
            m.norm("norm.mix", "norm.mix", norm)?;
            if let Some(n) = ffn_norm {
                m.norm("norm.ffn", "norm.ffn", n)?;
            }
        }
        Residual::PostNorm { mixer_norm, ffn_norm } => {
            m.norm("norm.mix", "norm.mix", mixer_norm)?;
            m.norm("norm.ffn", "norm.ffn", ffn_norm)?;
        }
    }
    match &ls.mixer {
        Mixer::Attention(a) => attention(m, a)?,
        Mixer::Mla(a) => mla(m, a)?,
        Mixer::GatedDeltaNet(g) => gdn(m, g)?,
        Mixer::Mamba(mm) => mamba(m, mm)?,
        Mixer::Mamba2(mm) => mamba2(m, mm)?,
        Mixer::RwkvTime(r) => rwkv_time(m, r, rescale, spec.hidden_size)?,
    }
    match &ls.ffn {
        Ffn::None => {}
        Ffn::Mlp(mm) => mlp(m, mm, "mlp", m.st.mlp)?,
        Ffn::Moe(mm) => moe(m, mm)?,
        Ffn::RwkvChannel(c) => rwkv_channel(m, c, rescale, spec.hidden_size)?,
    }
    Ok(())
}

fn attention(m: &mut M, a: &AttnSpec) -> Result<()> {
    let (h, kv, hd, vd) = (a.heads, a.kv_heads, a.head_dim, a.v_head_dim);
    let (qn, kn, vn) = (h * hd, kv * hd, kv * vd);
    match m.st.qkv {
        QkvLayout::Separate => {
            if a.output_gate {
                // q_proj rows per head: [q (d), gate (d)].
                let full = m.w("attn.q")?;
                let qp = Pick::Strided { block: 2 * hd, offset: 0, len: hd, groups: h };
                let gp = Pick::Strided { block: 2 * hd, offset: hd, len: hd, groups: h };
                m.put("attn.q.w", full.clone().rows(qp.clone()))?;
                m.put("attn.gate.w", full.rows(gp))?;
                if a.q_bias {
                    let b = m.b("attn.q")?;
                    m.put("attn.q.b", b.rows(qp))?;
                }
            } else {
                m.lin("attn.q", "attn.q", a.q_bias)?;
            }
            m.lin("attn.k", "attn.k", a.k_bias)?;
            m.lin("attn.v", "attn.v", a.v_bias)?;
        }
        layout => {
            let (pq, pk, pv) = match layout {
                QkvLayout::FusedConcat => {
                    (Pick::Range { start: 0, len: qn }, Pick::Range { start: qn, len: kn }, Pick::Range { start: qn + kn, len: vn })
                }
                QkvLayout::FusedPerHead => {
                    let blk = 3 * hd;
                    (
                        Pick::Strided { block: blk, offset: 0, len: hd, groups: h },
                        Pick::Strided { block: blk, offset: hd, len: hd, groups: h },
                        Pick::Strided { block: blk, offset: 2 * hd, len: hd, groups: h },
                    )
                }
                QkvLayout::FusedPerKvGroup => {
                    let g = h / kv;
                    let blk = (g + 2) * hd;
                    (
                        Pick::Strided { block: blk, offset: 0, len: g * hd, groups: kv },
                        Pick::Strided { block: blk, offset: g * hd, len: hd, groups: kv },
                        Pick::Strided { block: blk, offset: (g + 1) * hd, len: hd, groups: kv },
                    )
                }
                QkvLayout::Separate => unreachable!(),
            };
            let w = m.w("attn.qkv")?;
            m.put("attn.q.w", w.clone().rows(pq.clone()))?;
            m.put("attn.k.w", w.clone().rows(pk.clone()))?;
            m.put("attn.v.w", w.rows(pv.clone()))?;
            for (bias, name, pick) in [(a.q_bias, "attn.q.b", pq), (a.k_bias, "attn.k.b", pk), (a.v_bias, "attn.v.b", pv)] {
                if bias {
                    let b = m.b("attn.qkv")?;
                    m.put(name, b.rows(pick))?;
                }
            }
        }
    }
    m.lin("attn.o", "attn.o", a.o_bias)?;
    if let Some(qk) = &a.qk_norm {
        for (name, heads) in [("attn.q_norm", h), ("attn.k_norm", kv)] {
            let role = m.role(name)?;
            if qk.norm.gain != Gain::None {
                let src = match qk.scope {
                    // StableLM keeps one LayerNorm module per head.
                    QkNormScope::PerHeadSeparate if role.contains("{H}") => Src::t(format!("{role}.weight")).stack('H', heads),
                    _ => Src::t(format!("{role}.weight")),
                };
                m.put(format!("{name}.gain"), src)?;
            }
            if qk.norm.bias {
                let src = match qk.scope {
                    QkNormScope::PerHeadSeparate if role.contains("{H}") => Src::t(format!("{role}.bias")).stack('H', heads),
                    _ => Src::t(format!("{role}.bias")),
                };
                m.put(format!("{name}.bias"), src)?;
            }
        }
    }
    if a.sinks {
        m.put("attn.sinks", Src::t(m.role("attn.sinks")?))?;
    }
    Ok(())
}

fn mla(m: &mut M, a: &MlaSpec) -> Result<()> {
    let (r, rope) = (a.kv_lora_rank, a.qk_rope_head_dim);
    match a.q_lora_rank {
        Some(_) => {
            m.lin("mla.q_a", "mla.q_a", a.a_bias)?;
            m.norm("mla.q_a_norm", "mla.q_a_norm", &a.q_a_norm)?;
            m.lin("mla.q_b", "mla.q_b", false)?;
        }
        None => m.lin("mla.q", "mla.q", false)?,
    }
    let w = m.w("mla.kv_a")?;
    m.put("mla.kv_a.latent.w", w.clone().rows(Pick::Range { start: 0, len: r }))?;
    m.put("mla.kv_a.rope.w", w.rows(Pick::Range { start: r, len: rope }))?;
    if a.a_bias {
        let b = m.b("mla.kv_a")?;
        m.put("mla.kv_a.latent.b", b.clone().rows(Pick::Range { start: 0, len: r }))?;
        m.put("mla.kv_a.rope.b", b.rows(Pick::Range { start: r, len: rope }))?;
    }
    m.norm("mla.kv_a_norm", "mla.kv_a_norm", &a.kv_a_norm)?;
    m.lin("mla.kv_b", "mla.kv_b", false)?;
    m.lin("mla.o", "mla.o", false)
}

fn conv(m: &mut M, name: &str, ch: usize, k: usize, bias: bool) -> Result<()> {
    let r = m.role(name)?;
    m.put(format!("{name}.w"), Src::t(format!("{r}.weight")).reshape(vec![ch, k]))?;
    if bias {
        m.put(format!("{name}.b"), Src::t(format!("{r}.bias")))?;
    }
    Ok(())
}

fn gdn(m: &mut M, g: &GdnSpec) -> Result<()> {
    let (nk, nv, dk, dv) = (g.k_heads, g.v_heads, g.k_dim, g.v_dim);
    let rep = nv / nk;
    match m.st.gdn {
        GdnLayout::FusedPerKeyHead => {
            // in_proj_qkvz rows per key head: [q (dk), k (dk), v (rep·dv), z (rep·dv)];
            // in_proj_ba rows per key head: [b (rep), a (rep)]. Value head index = kh·rep + j.
            let blk = 2 * dk + 2 * rep * dv;
            let w = m.w("gdn.qkvz")?;
            let ba = m.w("gdn.ba")?;
            m.put("gdn.q.w", w.clone().rows(Pick::Strided { block: blk, offset: 0, len: dk, groups: nk }))?;
            m.put("gdn.k.w", w.clone().rows(Pick::Strided { block: blk, offset: dk, len: dk, groups: nk }))?;
            m.put("gdn.v.w", w.clone().rows(Pick::Strided { block: blk, offset: 2 * dk, len: rep * dv, groups: nk }))?;
            m.put("gdn.z.w", w.rows(Pick::Strided { block: blk, offset: 2 * dk + rep * dv, len: rep * dv, groups: nk }))?;
            m.put("gdn.b.w", ba.clone().rows(Pick::Strided { block: 2 * rep, offset: 0, len: rep, groups: nk }))?;
            m.put("gdn.a.w", ba.rows(Pick::Strided { block: 2 * rep, offset: rep, len: rep, groups: nk }))?;
        }
        GdnLayout::Split => {
            let w = m.w("gdn.qkv")?;
            m.put("gdn.q.w", w.clone().rows(Pick::Range { start: 0, len: nk * dk }))?;
            m.put("gdn.k.w", w.clone().rows(Pick::Range { start: nk * dk, len: nk * dk }))?;
            m.put("gdn.v.w", w.rows(Pick::Range { start: 2 * nk * dk, len: nv * dv }))?;
            m.lin("gdn.z", "gdn.z", false)?;
            m.lin("gdn.b", "gdn.b", false)?;
            m.lin("gdn.a", "gdn.a", false)?;
        }
    }
    conv(m, "gdn.conv", 2 * nk * dk + nv * dv, g.conv_kernel, false)?;
    m.put("gdn.dt_bias", Src::t(m.role("gdn.dt_bias")?))?;
    m.put("gdn.A", Src::t(m.role("gdn.A_log")?).map(MapFn::NegExp))?;
    m.put("gdn.norm.gain", Src::t(format!("{}.weight", m.role("gdn.norm")?)))?;
    m.lin("gdn.out", "gdn.out", false)
}

fn mamba(m: &mut M, s: &MambaSpec) -> Result<()> {
    let (i, n, r) = (s.inner, s.state, s.dt_rank);
    let w = m.w("mamba.in")?;
    m.put("mamba.in.x.w", w.clone().rows(Pick::Range { start: 0, len: i }))?;
    m.put("mamba.in.z.w", w.rows(Pick::Range { start: i, len: i }))?;
    if s.proj_bias {
        let b = m.b("mamba.in")?;
        m.put("mamba.in.x.b", b.clone().rows(Pick::Range { start: 0, len: i }))?;
        m.put("mamba.in.z.b", b.rows(Pick::Range { start: i, len: i }))?;
    }
    conv(m, "mamba.conv", i, s.conv_kernel, s.conv_bias)?;
    let x = m.w("mamba.x")?;
    m.put("mamba.x.dt.w", x.clone().rows(Pick::Range { start: 0, len: r }))?;
    m.put("mamba.x.B.w", x.clone().rows(Pick::Range { start: r, len: n }))?;
    m.put("mamba.x.C.w", x.rows(Pick::Range { start: r + n, len: n }))?;
    if let Some(ns) = &s.bcdt_norm {
        m.norm("mamba.dt_norm", "mamba.dt_norm", ns)?;
        m.norm("mamba.b_norm", "mamba.b_norm", ns)?;
        m.norm("mamba.c_norm", "mamba.c_norm", ns)?;
    }
    m.lin("mamba.dt", "mamba.dt", true)?;
    m.put("mamba.A", Src::t(m.role("mamba.A_log")?).map(MapFn::NegExp))?;
    m.put("mamba.D", Src::t(m.role("mamba.D")?))?;
    m.lin("mamba.out", "mamba.out", s.proj_bias)
}

fn mamba2(m: &mut M, s: &Mamba2Spec) -> Result<()> {
    let inner = s.heads * s.head_dim;
    let conv_dim = inner + 2 * s.groups * s.state;
    let off = 2 * s.d_mlp;
    let w = m.w("mamba2.in")?;
    let parts =
        [("mamba2.in.z", off, inner), ("mamba2.in.xbc", off + inner, conv_dim), ("mamba2.in.dt", off + inner + conv_dim, s.heads)];
    for (name, start, len) in parts {
        m.put(format!("{name}.w"), w.clone().rows(Pick::Range { start, len }))?;
        if s.proj_bias {
            let b = m.b("mamba2.in")?;
            m.put(format!("{name}.b"), b.rows(Pick::Range { start, len }))?;
        }
    }
    conv(m, "mamba2.conv", conv_dim, s.conv_kernel, s.conv_bias)?;
    m.put("mamba2.dt_bias", Src::t(m.role("mamba2.dt_bias")?))?;
    m.put("mamba2.A", Src::t(m.role("mamba2.A_log")?).map(MapFn::NegExp))?;
    m.put("mamba2.D", Src::t(m.role("mamba2.D")?))?;
    m.put("mamba2.norm.gain", Src::t(format!("{}.weight", m.role("mamba2.norm")?)))?;
    m.lin("mamba2.out", "mamba2.out", s.proj_bias)
}

fn rescaled(src: Src, rescale: Option<usize>) -> Src {
    match rescale {
        Some(e) => src.map(MapFn::RescaleByLayer { every: e }),
        None => src,
    }
}

fn rwkv_time(m: &mut M, r: &RwkvTimeSpec, rescale: Option<usize>, d: usize) -> Result<()> {
    if r.version != 4 {
        return Err(LowerError::not_lowerable(format!("RWKV-{} weights are not mapped", r.version)));
    }
    for mix in ["rwkv.att.mix_k", "rwkv.att.mix_v", "rwkv.att.mix_r"] {
        let role = m.role(mix)?;
        // HF stores `[1, 1, D]`.
        m.put(mix, Src::t(role).reshape(vec![d]))?;
    }
    m.lin("rwkv.att.k", "rwkv.att.k", false)?;
    m.lin("rwkv.att.v", "rwkv.att.v", false)?;
    m.lin("rwkv.att.r", "rwkv.att.r", false)?;
    m.put("rwkv.att.w", Src::t(m.role("rwkv.att.decay")?).map(MapFn::NegExp))?;
    m.put("rwkv.att.u", Src::t(m.role("rwkv.att.first")?))?;
    let o = m.w("rwkv.att.o")?;
    m.put("rwkv.att.o.w", rescaled(o, rescale))
}

fn rwkv_channel(m: &mut M, _c: &RwkvChannelSpec, rescale: Option<usize>, d: usize) -> Result<()> {
    for mix in ["rwkv.ffn.mix_k", "rwkv.ffn.mix_r"] {
        let role = m.role(mix)?;
        m.put(mix, Src::t(role).reshape(vec![d]))?;
    }
    m.lin("rwkv.ffn.k", "rwkv.ffn.k", false)?;
    m.lin("rwkv.ffn.r", "rwkv.ffn.r", false)?;
    let v = m.w("rwkv.ffn.v")?;
    m.put("rwkv.ffn.v.w", rescaled(v, rescale))
}

fn mlp(m: &mut M, s: &MlpSpec, pfx: &str, layout: MlpLayout) -> Result<()> {
    let i = s.intermediate;
    match layout {
        MlpLayout::Separate => {
            if s.gated {
                m.lin(&format!("{pfx}.gate"), &format!("{pfx}.gate"), s.up_bias)?;
            }
            m.lin(&format!("{pfx}.up"), &format!("{pfx}.up"), s.up_bias)?;
        }
        MlpLayout::FusedGateFirst => {
            let w = m.w(&format!("{pfx}.gate_up"))?;
            m.put(format!("{pfx}.gate.w"), w.clone().rows(Pick::Range { start: 0, len: i }))?;
            m.put(format!("{pfx}.up.w"), w.rows(Pick::Range { start: i, len: i }))?;
            if s.up_bias {
                let b = m.b(&format!("{pfx}.gate_up"))?;
                m.put(format!("{pfx}.gate.b"), b.clone().rows(Pick::Range { start: 0, len: i }))?;
                m.put(format!("{pfx}.up.b"), b.rows(Pick::Range { start: i, len: i }))?;
            }
        }
        MlpLayout::FusedInterleaved | MlpLayout::FusedGateFirstInOut => {
            return Err(LowerError::eval("internal: an expert-only layout on a dense MLP"));
        }
    }
    m.lin(&format!("{pfx}.down"), &format!("{pfx}.down"), s.down_bias)
}

fn moe(m: &mut M, s: &MoeSpec) -> Result<()> {
    let (e, i) = (s.experts, s.intermediate);
    m.lin("moe.router", "moe.router", s.router.linear_bias)?;
    if s.router.selection_bias {
        m.put("moe.sel_bias", Src::t(m.role("moe.sel_bias")?))?;
    }
    match m.st.experts {
        MlpLayout::Separate => {
            for (name, role) in [("moe.experts.gate", "moe.gate"), ("moe.experts.up", "moe.up"), ("moe.experts.down", "moe.down")] {
                let r = m.role(role)?;
                // Pre-quantised experts keep their integers, one stored module per expert.
                let src = match m.quantised(role)? {
                    Some(fmt) => Src::Quant { module: r, fmt }.stack('E', e),
                    None => Src::t(format!("{r}.weight")).stack('E', e),
                };
                m.put(name, src)?;
            }
        }
        MlpLayout::FusedGateFirst => {
            // GraniteMoE: input_linear [E, 2I, D] (gate rows first), output_linear [E, D, I].
            let gu = Src::t(m.role("moe.gate_up.stacked")?);
            m.put("moe.experts.gate", gu.clone().take(1, Pick::Range { start: 0, len: i }))?;
            m.put("moe.experts.up", gu.take(1, Pick::Range { start: i, len: i }))?;
            m.put("moe.experts.down", Src::t(m.role("moe.down.stacked")?))?;
        }
        MlpLayout::FusedGateFirstInOut => {
            // Llama-4: gate_up_proj [E, D, 2I] with the gate columns first, down_proj [E, I, D].
            let gu = Src::t(m.role("moe.gate_up.stacked")?).transpose();
            m.put("moe.experts.gate", gu.clone().take(1, Pick::Range { start: 0, len: i }))?;
            m.put("moe.experts.up", gu.take(1, Pick::Range { start: i, len: i }))?;
            m.put("moe.experts.down", Src::t(m.role("moe.down.stacked")?).transpose())?;
        }
        MlpLayout::FusedInterleaved => {
            // gpt-oss: gate_up_proj [E, D, 2I] with gate/up interleaved, down_proj [E, I, D].
            let gu = Src::t(m.role("moe.gate_up.stacked")?).transpose();
            let gub = Src::t(m.role("moe.gate_up_bias.stacked")?);
            let even = Pick::Strided { block: 2, offset: 0, len: 1, groups: i };
            let odd = Pick::Strided { block: 2, offset: 1, len: 1, groups: i };
            m.put("moe.experts.gate", gu.clone().take(1, even.clone()))?;
            m.put("moe.experts.up", gu.take(1, odd.clone()))?;
            m.put("moe.experts.down", Src::t(m.role("moe.down.stacked")?).transpose())?;
            if s.expert_bias {
                m.put("moe.experts.gate_b", gub.clone().take(1, even))?;
                m.put("moe.experts.up_b", gub.take(1, odd))?;
                m.put("moe.experts.down_b", Src::t(m.role("moe.down_bias.stacked")?))?;
            }
        }
    }
    if let Some(sh) = &s.shared {
        let spec =
            MlpSpec { intermediate: sh.intermediate, act: s.act, gated: true, glu: Glu::Standard, up_bias: false, down_bias: false };
        mlp(m, &spec, "moe.shared", MlpLayout::Separate)?;
        if sh.sigmoid_gate {
            m.lin("moe.shared_gate", "moe.shared_gate", false)?;
        }
    }
    Ok(())
}
