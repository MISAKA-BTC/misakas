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
use crate::weights::{Binding, CombineFn, MapFn, Pick, Src};
use std::collections::{BTreeMap, BTreeSet};

/// Stands for a role the adapter does not name while the defaults of the overridden params are derived
/// (see [`M::role`]); `\u{2}` ends the role.
const MISSING: &str = "\u{1}missing-role:";

struct M<'a> {
    st: &'a HfStorage,
    out: BTreeMap<String, Src>,
    /// The HL params an adapter binds by an expression (`spec.hf.weights`, `WEIGHTS_EXPR_V1`): their default
    /// binding is not derived, so the roles it would read need not be named.
    overrides: BTreeSet<String>,
    /// A role namespace (`ATTN_SHARED_BLOCK_V1`: the shared branch's roles are `pb.attn.q`, `pb.mlp.gate_up`, … while its binder reuses the
    /// attention and MLP binders, which ask for `attn.q`, `mlp.gate_up`): a role is looked up under `{scope}.{role}` first, then as itself.
    scope: Option<String>,
    /// Placeholders a layer's roles may carry, filled from the layer's spec: `{B}` the layer that stores a shared branch's weights, `{O}`
    /// this layer's ordinal among the layers with a pre-branch (`LAYER_PRE_BRANCH_V1`).
    subs: Vec<(&'static str, String)>,
}

/// The role a marker in `s` stands for, if any.
fn missing_role(s: &Src) -> Option<String> {
    let mark = |t: &str| -> Option<String> {
        let rest = t.split_once(MISSING)?.1;
        Some(rest.split('\u{2}').next().unwrap_or(rest).to_string())
    };
    match s {
        Src::Tensor(t) => mark(t),
        Src::Quant { module, .. } => mark(module),
        Src::Take { src, .. }
        | Src::Transpose(src)
        | Src::Stack { src, .. }
        | Src::Map { src, .. }
        | Src::Reshape { src, .. }
        | Src::PadRows { src, .. } => missing_role(src),
        Src::Combine { srcs, .. } => srcs.iter().find_map(missing_role),
    }
}

/// `name` + `suffix`, the suffix on EVERY alternative of a name that lists some (`TENSOR_NAME_ALTERNATIVES_V1`: `a|b` + `.weight`
/// is `a.weight|b.weight`).
fn suffixed(name: &str, suffix: &str) -> String {
    name.split('|').map(|a| format!("{a}{suffix}")).collect::<Vec<_>>().join("|")
}

impl M<'_> {
    fn role(&self, role: &str) -> Result<String> {
        let scoped = self.scope.as_ref().and_then(|s| self.st.name(&format!("{s}.{role}")));
        match scoped.or_else(|| self.st.name(role)) {
            Some(n) => Ok(self.subs.iter().fold(n.to_string(), |t, (k, v)| t.replace(k, v))),
            // With expressions in play a role may be absent on purpose: every param that would read it is
            // overridden. `put` turns the marker back into the error for any other.
            None if !self.overrides.is_empty() => Ok(format!("{MISSING}{role}\u{2}")),
            None => Err(LowerError::eval(format!("internal: no HF tensor name for role `{role}`"))),
        }
    }
    fn put(&mut self, name: impl Into<String>, src: Src) -> Result<()> {
        let name = name.into();
        // An expression of the adapter replaces the default binding.
        if self.overrides.contains(&name) {
            return Ok(());
        }
        if let Some(role) = missing_role(&src) {
            return Err(LowerError::eval(format!("internal: no HF tensor name for role `{role}`")));
        }
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
        let t = Src::t(suffixed(&self.role(role)?, ".weight"));
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
        // A format that decodes to floats (FP8) reads whatever the quantiser converted — a projection of
        // any mixer — as the float weight it is; one that yields integers is lowered from them, and only
        // the plain attention and MLP projections are (`crate::lower::qlinear`).
        const ALSO_FLOAT: &[&str] = &[
            "mla.q_a", "mla.q_b", "mla.q", "mla.kv_a", "mla.kv_b", "mla.o", "gdn.qkvz", "gdn.ba", "gdn.qkv", "gdn.z", "gdn.b", "gdn.a", "gdn.out",
            "kda.q", "kda.k", "kda.v", "kda.b", "kda.f_a", "kda.f_b", "kda.g_a", "kda.g_b", "kda.out",
            "mamba.in", "mamba.x", "mamba.dt", "mamba.out", "mamba2.in", "mamba2.out",
        ];
        let Some(q) = &self.st.quant else { return Ok(None) };
        // A virtual format's tensors are served as the float tensors the export would have: no module is
        // bound to the format.
        if q.fmt.is_virtual() {
            return Ok(None);
        }
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
        if !(PROJECTIONS.contains(&role) || (!q.fmt.is_integers() && ALSO_FLOAT.contains(&role))) {
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
        Ok(Src::t(suffixed(&self.role(role)?, ".bias")))
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
            self.put(format!("{name}.gain"), Src::t(suffixed(&r, ".weight")))?;
        }
        if spec.bias {
            let r = self.role(role)?;
            self.put(format!("{name}.bias"), Src::t(suffixed(&r, ".bias")))?;
        }
        Ok(())
    }
}

/// Map every param of `prog` (built from `spec`) onto HF tensors.
pub fn bind(spec: &ArchSpec, prog: &HlProgram) -> Result<Binding> {
    // The adapter's expressions (`WEIGHTS_EXPR_V1`) replace the default binding of the params they name.
    let overrides = crate::weights::expr::parse_all(&spec.hf.weights, &prog.params)?;
    let mut m = M { st: &spec.hf, out: BTreeMap::new(), overrides: overrides.keys().cloned().collect(), scope: None, subs: vec![] };
    // Embedding, positions, head.
    m.put("embed.table", Src::t(suffixed(&m.role("embed")?, ".weight")))?;
    if spec.embedding.proj_in {
        m.lin("embed.proj_in", "proj_in", spec.embedding.proj_in_bias)?;
    }
    if spec.embedding.positions.is_some() {
        m.put("embed.pos_table", Src::t(suffixed(&m.role("pos_embed")?, ".weight")))?;
    }
    if let Some(n) = &spec.embedding.norm {
        m.norm("embed.norm", "embed_norm", n)?;
    }
    if spec.embedding.type_rows.is_some() {
        m.put("embed.type_table", Src::t(suffixed(&m.role("type_embed")?, ".weight")))?;
    }
    if let Some(dis) = &spec.embedding.disentangled {
        m.put("rel.table", Src::t(suffixed(&m.role("rel_embed")?, ".weight")))?;
        if let Some(n) = &dis.norm {
            m.norm("rel.norm", "rel_norm", n)?;
        }
    }
    if spec.embedding.rel_bias.is_some() {
        m.put("attn.rel_bias", Src::t(suffixed(&m.role("rel_bias")?, ".weight")))?;
    }
    if let Some(n) = &spec.final_norm {
        m.norm("final_norm", "final_norm", n)?;
    }
    // Hyper-connections: the last gated mix of the streams (it stands where the final norm would).
    if let Some(hy) = &spec.hyper {
        m.norm("hc.final.norm", "hc.final.norm", &hy.norm)?;
        m.lin("hc.final.down", "hc.final.down", false)?;
        m.lin("hc.final.up", "hc.final.up", false)?;
    }
    // Manifold-constrained hyper-connections: the head's collapse (bare tensors).
    if spec.mhc.is_some() {
        m.put("mhc.head.fn.w", Src::t(m.role("mhc.head.fn")?))?;
        m.put("mhc.head.base", Src::t(m.role("mhc.head.base")?))?;
        m.put("mhc.head.scale", Src::t(m.role("mhc.head.scale")?))?;
    }
    // AltUp: the projections that make the other streams from the embedding and bring them back (`{I}` is the projection's index, 0-based).
    if let Some(au) = &spec.altup {
        for i in 1..au.streams {
            for (name, role) in [("altup.proj", "altup.proj"), ("altup.unembed", "altup.unembed")] {
                let t = m.role(role)?.replace("{I}", &(i - 1).to_string());
                m.put(format!("{name}{i}.w"), Src::t(format!("{t}.weight")))?;
            }
        }
    }
    let logits = matches!(spec.output, OutputSpec::Logits);
    if let OutputSpec::Embedding { proj: Some((_, bias)), .. } = spec.output {
        let w = m.w("embed_proj")?;
        m.put("embed.proj.w", w)?;
        if bias {
            m.put("embed.proj.b", Src::t(suffixed(&m.role("embed_proj")?, ".bias")))?;
        }
    }
    if logits && let Some(t) = &spec.head.transform {
        m.lin("head.transform.dense", "head.transform.dense", t.bias)?;
        m.norm("head.transform.norm", "head.transform.norm", &t.norm)?;
    }
    if logits && spec.head.proj_out {
        let w = m.w("proj_out")?;
        m.put("head.proj_out.w", w)?;
    }
    if logits && !spec.head.tied {
        let w = match m.quantised("lm_head")? {
            Some(fmt) => Src::Quant { module: m.role("lm_head")?, fmt },
            None => Src::t(suffixed(&m.role("lm_head")?, ".weight")),
        };
        m.put("head.w", w)?;
    }
    if logits && spec.head.bias {
        let b = match spec.hf.name("lm_head_bias") {
            Some(n) => Src::t(n),
            None => Src::t(suffixed(&m.role("lm_head")?, ".bias")),
        };
        m.put("head.b", b)?;
    }
    let rescale = spec.layers.iter().position(|l| l.post_scale != 1.0).map(|i| i + 1);
    let mut seen: Vec<&LayerSpec> = Vec::new();
    for ls in &spec.layers {
        // A cross-attention layer is skipped in the text-only stage: its tensors are dormant.
        if seen.contains(&ls) || matches!(ls.mixer, Mixer::CrossAttention(_)) {
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
                    let t = ad
                        .tensor(role, b)
                        .ok_or_else(|| LowerError::eval(format!("internal: adapter role `{role}` has no module")))?;
                    m.put(d.name.clone(), Src::t(t))?;
                }
            }
        }
    }
    let mut srcs = Vec::with_capacity(prog.params.len());
    for d in &prog.params {
        srcs.push(match overrides.get(&d.name) {
            Some(expr) => expr.clone(),
            None => m
                .out
                .get(&d.name)
                .cloned()
                .ok_or_else(|| LowerError::eval(format!("internal: HL param `{}` has no HF source", d.name)))?,
        });
    }
    Ok(Binding { srcs, aliases: spec.hf.prefix_aliases.clone(), ignored_prefixes: spec.hf.ignored_prefixes.clone() })
}

/// Gemma-3n/4's per-layer input: the layer's slices of the two tensors packed over the layers, the norms and the branch's projections.
fn bind_ple(m: &mut M, p: &PleSpec) -> Result<()> {
    m.put("ple.proj.w", Src::t(suffixed(&m.role("ple.proj")?, ".weight")).take(0, Pick::PerLayer { len: p.dim }))?;
    m.put("ple.table", Src::t(suffixed(&m.role("ple.table")?, ".weight")).take(1, Pick::PerLayer { len: p.dim }))?;
    m.norm("ple.norm", "ple.norm", &p.norm)?;
    m.lin("ple.gate", "ple.gate", false)?;
    m.lin("ple.out", "ple.out", false)?;
    m.norm("ple.post_norm", "ple.post_norm", &p.post_norm)?;
    Ok(())
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
        Residual::Sandwich { pre_mixer, post_mixer, pre_ffn, post_ffn, ple, layer_scalar } => {
            for (n, name) in
                [(pre_mixer, "norm.mix"), (post_mixer, "norm.post_mix"), (pre_ffn, "norm.ffn"), (post_ffn, "norm.post_ffn")]
            {
                m.norm(name, name, n)?;
            }
            if let Some(p) = ple {
                bind_ple(m, p)?;
            }
            if *layer_scalar {
                m.put("layer.scalar", Src::t(m.role("layer.scalar")?))?;
            }
        }
        Residual::Mhc { pre_mixer, pre_ffn } => {
            for tag in ["attn", "ffn"] {
                // The mixing parameters are bare tensors (`hc_attn_fn`, `hc_attn_base`, `hc_attn_scale`).
                m.put(format!("mhc.{tag}.fn.w"), Src::t(m.role(&format!("mhc.{tag}.fn"))?))?;
                m.put(format!("mhc.{tag}.base"), Src::t(m.role(&format!("mhc.{tag}.base"))?))?;
                m.put(format!("mhc.{tag}.scale"), Src::t(m.role(&format!("mhc.{tag}.scale"))?))?;
            }
            m.norm("norm.mix", "norm.mix", pre_mixer)?;
            m.norm("norm.ffn", "norm.ffn", pre_ffn)?;
        }
        Residual::AltUp { pre_mixer, post_mixer, pre_ffn, post_ffn, router_norm, laurel, ple } => {
            for (n, name) in
                [(pre_mixer, "norm.mix"), (post_mixer, "norm.post_mix"), (pre_ffn, "norm.ffn"), (post_ffn, "norm.post_ffn")]
            {
                m.norm(name, name, n)?;
            }
            m.norm("altup.router_norm", "altup.router_norm", router_norm)?;
            for name in ["altup.router", "altup.pred_coefs", "altup.correction_coefs"] {
                m.lin(name, name, false)?;
            }
            if spec.altup.as_ref().is_some_and(|a| a.correct_scale) && ple.is_some() {
                m.put("altup.correct_scale", Src::t(m.role("altup.correct_scale")?))?;
            }
            if let Some(l) = laurel {
                m.lin("laurel.left", "laurel.left", false)?;
                m.lin("laurel.right", "laurel.right", false)?;
                m.norm("laurel.norm", "laurel.norm", &l.post_norm)?;
            }
            if let Some(p) = ple {
                bind_ple(m, p)?;
            }
        }
        Residual::HyperConnection { ple } => {
            let hy = spec.hyper.as_ref().ok_or_else(|| LowerError::eval("internal: a hyper-connection layer without the model's streams"))?;
            for tag in ["mix", "ffn"] {
                let role = |part: &str| format!("hc.{tag}.{part}");
                m.norm(&role("norm"), &role("norm"), &hy.norm)?;
                for part in ["down", "up", "inject"] {
                    m.lin(&role(part), &role(part), false)?;
                }
            }
            if let Some(p) = ple {
                ple_ngram(m, spec, p)?;
            }
        }
    }
    if let Some(pb) = &ls.pre_branch {
        pre_branch(m, pb)?;
    }
    bind_mixer(m, spec, &ls.mixer, rescale)?;
    match &ls.ffn {
        Ffn::None => {}
        Ffn::Mlp(mm) => mlp(m, mm, "mlp", m.st.mlp)?,
        Ffn::Moe(mm) => moe(m, mm)?,
        Ffn::RwkvChannel(c) => rwkv_channel(m, c, rescale, spec.hidden_size)?,
        // `FFN_SHORTCUT_MOE_V1`: the dense MLP's params, and the MoE's in the layer that produces it.
        Ffn::MlpShortcut(sc) => {
            mlp(m, &sc.mlp, "mlp", m.st.mlp)?;
            if let ShortcutSide::Produce(mo) = &sc.side {
                moe(m, mo)?;
            }
        }
        Ffn::MlpMoe(mm) => {
            mlp(m, &mm.mlp, "mlp", m.st.mlp)?;
            moe(m, &mm.moe)?;
            for (n, name) in [(&mm.mlp_post, "norm.post_mlp"), (&mm.moe_pre, "norm.moe"), (&mm.moe_post, "norm.post_moe")] {
                m.norm(name, name, n)?;
            }
            // Gemma-4 stores the router's gain as a bare vector (`router.scale`).
            if mm.router_norm.gain != Gain::None {
                m.put("moe.router_norm.gain", Src::t(m.role("moe.router_norm")?))?;
            }
        }
    }
    Ok(())
}

/// **`LAYER_PRE_BRANCH_V1`**: the shared branch's weights (the attention, its two norms and the MLP: the group's, read from the layer that
/// stores them, `{B}`), this layer's low-rank adapters (index `{O}` of the checkpoint's adapter lists) and its own `out` projection.
fn pre_branch(m: &mut M, pb: &PreBranch) -> Result<()> {
    m.scope = Some("pb".into());
    m.subs = vec![("{B}", pb.weights_layer.to_string()), ("{O}", pb.ordinal.to_string())];
    let r = (|| -> Result<()> {
        m.norm(&pb.in_norm_name(), "in_norm", &pb.in_norm)?;
        let mut attn = pb.attn.clone();
        attn.param_prefix = Some(pb.attn_prefix());
        attention(m, &attn)?;
        m.norm(&pb.mid_norm_name(), "mid_norm", &pb.mid_norm)?;
        let mut mlp_spec = pb.mlp.clone();
        mlp_spec.name = Some(pb.mlp_name());
        if m.st.mlp != MlpLayout::FusedGateFirst {
            return Err(LowerError::not_lowerable("LAYER_PRE_BRANCH_V1: the shared MLP's gate and up projections are one tensor, `[gate | up]` rows"));
        }
        mlp(m, &mlp_spec, "mlp", m.st.mlp)?;
        if let Some(lr) = pb.lowrank {
            let base = pb.adapter_base();
            let i = pb.mlp.intermediate;
            if lr.attn {
                for part in ["q", "k", "v"] {
                    let a = Src::t(suffixed(&m.role(&format!("attn.{part}_adapter.a"))?, ".weight"));
                    let b = Src::t(suffixed(&m.role(&format!("attn.{part}_adapter.b"))?, ".weight"));
                    m.put(format!("{base}.attn.{part}.lora_a"), a)?;
                    m.put(format!("{base}.attn.{part}.lora_b"), b)?;
                }
            }
            // The fused gate/up adapter: one `A`, and `B`'s first half of rows goes to the gate, the second to the up projection.
            let a = Src::t(suffixed(&m.role("mlp.gate_up_adapter.a")?, ".weight"));
            let b = Src::t(suffixed(&m.role("mlp.gate_up_adapter.b")?, ".weight"));
            m.put(format!("{base}.mlp.gate.lora_a"), a.clone())?;
            m.put(format!("{base}.mlp.up.lora_a"), a)?;
            m.put(format!("{base}.mlp.gate.lora_b"), b.clone().rows(Pick::Range { start: 0, len: i }))?;
            m.put(format!("{base}.mlp.up.lora_b"), b.rows(Pick::Range { start: i, len: i }))?;
        }
        Ok(())
    })();
    m.scope = None;
    m.subs.clear();
    r?;
    m.lin("pb.out", "pb.out", false)
}

/// The params of one mixer (a [`Mixer::Parallel`]'s branches each in turn; [`Mixer::None`] has none).
fn bind_mixer(m: &mut M, spec: &ArchSpec, mixer: &Mixer, rescale: Option<usize>) -> Result<()> {
    match mixer {
        Mixer::Attention(a) => attention(m, a)?,
        Mixer::Mla(a) => mla(m, a)?,
        Mixer::GatedDeltaNet(g) => gdn(m, g)?,
        Mixer::Kda(k) => kda(m, k)?,
        Mixer::Mamba(mm) => mamba(m, mm)?,
        Mixer::Mamba2(mm) => mamba2(m, mm)?,
        Mixer::ShortConv(c) => short_conv(m, c, spec.hidden_size)?,
        Mixer::RwkvTime(r) => rwkv_time(m, r, rescale, spec.hidden_size)?,
        Mixer::None => {}
        Mixer::CrossAttention(_) => return Err(LowerError::eval("internal: a cross-attention layer is bound only when it is not skipped")),
        Mixer::Parallel(bs) => {
            for b in bs {
                bind_mixer(m, spec, &b.mixer, rescale)?;
            }
        }
        Mixer::SharedKv(a) => shared_kv(m, a)?,
    }
    Ok(())
}

/// DeepSeek-V4's attention: the low-rank query, the one key/value head, the sinks, the grouped output projection (the `o_a` weight is one
/// tensor of `groups·rank` rows: group `g` is its rows `[g·rank, (g+1)·rank)`) and the compressors.
fn shared_kv(m: &mut M, a: &SharedKvSpec) -> Result<()> {
    m.lin("attn.wq_a", "attn.wq_a", false)?;
    m.norm("attn.q_a_norm", "attn.q_a_norm", &a.q_a_norm)?;
    m.lin("attn.wq_b", "attn.wq_b", false)?;
    m.norm("attn.q_b_norm", "attn.q_b_norm", &a.q_b_norm)?;
    m.lin("attn.wkv", "attn.wkv", false)?;
    m.norm("attn.kv_norm", "attn.kv_norm", &a.kv_norm)?;
    if a.sinks {
        m.put("attn.sinks", Src::t(m.role("attn.sinks")?))?;
    }
    let full = Src::t(format!("{}.weight", m.role("attn.wo_a")?));
    for g in 0..a.o_groups {
        m.put(format!("attn.wo_a{g}.w"), full.clone().rows(Pick::Range { start: g * a.o_rank, len: a.o_rank }))?;
    }
    m.lin("attn.wo_b", "attn.wo_b", false)?;
    if let Some(c) = &a.compressed {
        for pfx in [if c.overlap { "attn.csa" } else { "attn.hca" }] {
            m.lin(&format!("{pfx}.wkv"), &format!("{pfx}.wkv"), false)?;
            m.lin(&format!("{pfx}.wgate"), &format!("{pfx}.wgate"), false)?;
            m.put(format!("{pfx}.ape"), Src::t(m.role(&format!("{pfx}.ape"))?))?;
            m.norm(&format!("{pfx}.norm"), &format!("{pfx}.norm"), &c.norm)?;
        }
        if let Some(ix) = &c.indexer {
            let pfx = "attn.idx.comp";
            m.lin(&format!("{pfx}.wkv"), &format!("{pfx}.wkv"), false)?;
            m.lin(&format!("{pfx}.wgate"), &format!("{pfx}.wgate"), false)?;
            m.put(format!("{pfx}.ape"), Src::t(m.role(&format!("{pfx}.ape"))?))?;
            m.norm(&format!("{pfx}.norm"), &format!("{pfx}.norm"), &ix.norm)?;
            m.lin("attn.idx.wq_b", "attn.idx.wq_b", false)?;
            m.lin("attn.idx.weights_proj", "attn.idx.weights_proj", false)?;
        }
    }
    Ok(())
}

/// **`MIXER_MOA_V1`**: the router, the stacked query and output experts, the shared key/value projection and the output bias.
fn moa(m: &mut M, a: &AttnSpec, moa: &MoaSpec) -> Result<()> {
    m.lin("moa.router", "moa.router", moa.router.linear_bias)?;
    m.put("moa.experts.input", Src::t(m.role("moa.input")?))?;
    m.put("moa.experts.output", Src::t(m.role("moa.output")?))?;
    m.lin("attn.kv", "attn.kv", false)?;
    if moa.out_bias {
        m.put("moa.bias", Src::t(m.role("moa.bias")?))?;
    }
    let _ = a;
    Ok(())
}

fn attention(m: &mut M, a: &AttnSpec) -> Result<()> {
    if let Some(mo) = &a.moa {
        return moa(m, a, mo);
    }
    let (h, kv, hd, vd) = (a.heads, a.kv_heads, a.head_dim, a.v_head_dim);
    let (qn, kn, vn) = (h * hd, kv * hd, kv * vd);
    // The HL names (`AttnSpec::param_prefix`); the checkpoint roles stay `attn.*`.
    let pf = a.param_prefix.clone().unwrap_or_else(|| "attn".into());
    let n = |s: &str| format!("{pf}.{s}");
    // A KV-sharing layer projects no keys or values of its own.
    if let Some(crate::spec::KvShare::Consumer { .. }) = a.kv_share {
        if m.st.qkv != QkvLayout::Separate || a.output_gate || a.qk_norm.is_some_and(|q| q.scope == QkNormScope::PerHeadSeparate) {
            return Err(LowerError::eval("internal: a KV-sharing layer over a fused qkv, a gated query or per-head norm modules"));
        }
        m.lin(&n("q"), "attn.q", a.q_bias)?;
        m.lin(&n("o"), "attn.o", a.o_bias)?;
        if let Some(on) = &a.o_norm {
            m.norm(&n("sub_norm"), "attn.sub_norm", on)?;
        }
        if let Some(qk) = &a.qk_norm
            && (qk.norm.gain != Gain::None || qk.norm.bias)
        {
            m.norm(&n("q_norm"), "attn.q_norm", &qk.norm)?;
        }
        return Ok(());
    }
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
                m.lin(&n("q"), "attn.q", a.q_bias)?;
            }
            m.lin(&n("k"), "attn.k", a.k_bias)?;
            // Gemma-4's `attention_k_eq_v`: no v_proj; the values are the key projection.
            if !a.v_from_k {
                m.lin(&n("v"), "attn.v", a.v_bias)?;
            }
        }
        _ if a.param_prefix.is_some() || a.v_from_k => {
            return Err(LowerError::eval("internal: named or key-valued attention over a fused qkv"));
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
    // `ATTN_DIFFERENTIAL_V1`: λ from the four vectors, and `1 − λ_init`, both `[1]` constants of the layer.
    if let Some(df) = &a.differential {
        let q = |r: &str| -> Result<Src> { Ok(Src::t(m.role(r)?)) };
        let ins = vec![q("attn.lambda_q1")?, q("attn.lambda_k1")?, q("attn.lambda_q2")?, q("attn.lambda_k2")?];
        m.put(n("diff.lambda"), Src::Combine { srcs: ins, f: CombineFn::DiffLambda(df.lambda_init) })?;
        m.put(n("diff.scale"), Src::Combine { srcs: vec![], f: CombineFn::OneMinusLambdaInit(df.lambda_init) })?;
    }
    m.lin(&n("o"), "attn.o", a.o_bias)?;
    // `SUBLAYER_NORMS_V1`: BitNet's `attn_sub_norm`.
    if let Some(on) = &a.o_norm {
        m.norm(&n("sub_norm"), "attn.sub_norm", on)?;
    }
    if a.gate.is_some() {
        m.lin(&n("gate"), "attn.gate", false)?;
    }
    let norms = [(a.qk_norm, "q_norm", h), (a.qk_norm, "k_norm", kv), (a.v_norm, "v_norm", kv)];
    for (qk, which, heads) in norms {
        let Some(qk) = qk else { continue };
        if qk.norm.gain != Gain::None || qk.norm.bias {
            let (name, role) = (n(which), m.role(&format!("attn.{which}"))?);
            let name = name.as_str();
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
        m.put(n("sinks"), Src::t(m.role("attn.sinks")?))?;
    }
    // Sparse block attention's indexer: one fused query+key projection and the two norms.
    if let Some(sp) = &a.sparse {
        m.lin("attn.idx.qk", "attn.idx.qk", false)?;
        m.norm("attn.idx.q_norm", "attn.idx.q_norm", &sp.q_norm)?;
        m.norm("attn.idx.k_norm", "attn.idx.k_norm", &sp.k_norm)?;
    }
    Ok(())
}

/// The per-layer n-gram embedding's params: its two projections, three norms, the dilated depthwise
/// convolution, and the hash heads' table (the checkpoint's row shards, concatenated, padded to the
/// tallest layer's height).
fn ple_ngram(m: &mut M, spec: &ArchSpec, p: &NgramPleSpec) -> Result<()> {
    let streams = spec.hyper.as_ref().map_or(1, |h| h.streams);
    let n = streams * spec.hidden_size;
    m.lin("ple.key", "ple.key", false)?;
    m.lin("ple.value", "ple.value", false)?;
    for name in ["ple.norm_key", "ple.norm_query", "ple.norm_conv"] {
        m.norm(name, name, &p.norm)?;
    }
    conv(m, "ple.conv", n, p.conv_kernel, false)?;
    let rows_max = spec
        .layers
        .iter()
        .filter_map(|l| match &l.residual {
            Residual::HyperConnection { ple: Some(q) } => Some(crate::ngram::NgramTables::cached(q).padded_vocab as usize),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    let shards = m.st.table_shards.max(1);
    let table = Src::t(suffixed(&m.role("ple.table")?, ".weight")).stack('S', shards).pad_rows(rows_max);
    m.put("ple.ngram.table", table)
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
    // The token indexer's own projections (`ATTN_TOKEN_INDEXER_V1`): wq_b, wk, the key's LayerNorm, weights_proj.
    if let Some(ix) = &a.indexer {
        m.lin("mla.idx.q", "mla.idx.q", false)?;
        m.lin("mla.idx.k", "mla.idx.k", false)?;
        m.norm("mla.idx.k_norm", "mla.idx.k_norm", &ix.k_norm)?;
        m.lin("mla.idx.wp", "mla.idx.wp", false)?;
    }
    m.lin("mla.o", "mla.o", false)
}

fn conv(m: &mut M, name: &str, ch: usize, k: usize, bias: bool) -> Result<()> {
    let r = m.role(name)?;
    m.put(format!("{name}.w"), Src::t(suffixed(&r, ".weight")).reshape(vec![ch, k]))?;
    if bias {
        m.put(format!("{name}.b"), Src::t(suffixed(&r, ".bias")))?;
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
    m.put("gdn.norm.gain", Src::t(suffixed(&m.role("gdn.norm")?, ".weight")))?;
    m.lin("gdn.out", "gdn.out", false)
}

/// **`MIXER_KDA_V1`**: the projections, three convolutions (one tensor each: the hub's `q_conv1d`, `k_conv1d`, `v_conv1d`), the low-rank gates, and
/// `A_log` — stored `[1, 1, heads, 1]` — as the per-channel decay rate `-exp(A_log[head])` of every key channel of the head.
fn kda(m: &mut M, k: &KdaSpec) -> Result<()> {
    let n = k.heads * k.head_dim;
    for part in ["q", "k", "v"] {
        m.lin(&format!("kda.{part}"), &format!("kda.{part}"), false)?;
        conv(m, &format!("kda.{part}_conv"), n, k.conv_kernel, false)?;
    }
    for part in ["b", "f_a", "f_b", "g_a", "g_b", "out"] {
        m.lin(&format!("kda.{part}"), &format!("kda.{part}"), false)?;
    }
    m.put("kda.dt_bias", Src::t(m.role("kda.dt_bias")?))?;
    m.put(
        "kda.A",
        Src::t(m.role("kda.A_log")?).reshape(vec![k.heads]).map(MapFn::NegExp).take(0, Pick::Repeat { each: k.head_dim, groups: k.heads }),
    )?;
    m.put("kda.norm.gain", Src::t(suffixed(&m.role("kda.norm")?, ".weight")))
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
    let gn = s.groups * s.state;
    let conv_dim = inner + 2 * gn;
    let off = 2 * s.d_mlp;
    let w = m.w("mamba2.in")?;
    // `MAMBA2_MUP_V1` (chunk scales): x, B and C are three linears and three convolutions of their own (see the HL builder).
    let split = s.chunk_scales.is_some();
    let parts: Vec<(&str, usize, usize)> = if split {
        vec![
            ("mamba2.in.z", off, inner),
            ("mamba2.in.x", off + inner, inner),
            ("mamba2.in.b", off + 2 * inner, gn),
            ("mamba2.in.c", off + 2 * inner + gn, gn),
            ("mamba2.in.dt", off + inner + conv_dim, s.heads),
        ]
    } else {
        vec![("mamba2.in.z", off, inner), ("mamba2.in.xbc", off + inner, conv_dim), ("mamba2.in.dt", off + inner + conv_dim, s.heads)]
    };
    for (name, start, len) in parts {
        m.put(format!("{name}.w"), w.clone().rows(Pick::Range { start, len }))?;
        if s.proj_bias {
            let b = m.b("mamba2.in")?;
            m.put(format!("{name}.b"), b.rows(Pick::Range { start, len }))?;
        }
    }
    if split {
        let r = m.role("mamba2.conv")?;
        for (name, start, ch) in [("x", 0, inner), ("b", inner, gn), ("c", inner + gn, gn)] {
            let pick = Pick::Range { start, len: ch };
            m.put(format!("mamba2.conv.{name}.w"), Src::t(suffixed(&r, ".weight")).rows(pick.clone()).reshape(vec![ch, s.conv_kernel]))?;
            if s.conv_bias {
                m.put(format!("mamba2.conv.{name}.b"), Src::t(suffixed(&r, ".bias")).rows(pick))?;
            }
        }
    } else {
        conv(m, "mamba2.conv", conv_dim, s.conv_kernel, s.conv_bias)?;
    }
    m.put("mamba2.dt_bias", Src::t(m.role("mamba2.dt_bias")?))?;
    m.put("mamba2.A", Src::t(m.role("mamba2.A_log")?).map(MapFn::NegExp))?;
    m.put("mamba2.D", Src::t(m.role("mamba2.D")?))?;
    if s.norm_mode != Mamba2Norm::Ungated {
        m.put("mamba2.norm.gain", Src::t(suffixed(&m.role("mamba2.norm")?, ".weight")))?;
    }
    m.lin("mamba2.out", "mamba2.out", s.proj_bias)
}

/// LFM2's short convolution: `in_proj [3D, D]` rows `[B | C | x]` as three linears, the depthwise convolution `[D, 1, K]`, `out_proj`.
fn short_conv(m: &mut M, c: &ShortConvSpec, d: usize) -> Result<()> {
    let w = m.w("shortconv.in")?;
    for (i, part) in ["b", "c", "x"].into_iter().enumerate() {
        m.put(format!("shortconv.in.{part}.w"), w.clone().rows(Pick::Range { start: i * d, len: d }))?;
        if c.bias {
            let b = m.b("shortconv.in")?;
            m.put(format!("shortconv.in.{part}.b"), b.rows(Pick::Range { start: i * d, len: d }))?;
        }
    }
    conv(m, "shortconv.conv", d, c.kernel, c.bias)?;
    m.lin("shortconv.out", "shortconv.out", c.bias)
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

/// An MLP's params: HL names under the MLP's own name ([`MlpSpec::name`], else `pfx`), checkpoint
/// roles under `pfx`.
fn mlp(m: &mut M, s: &MlpSpec, pfx: &str, layout: MlpLayout) -> Result<()> {
    let i = s.intermediate;
    let hn = s.name.clone().unwrap_or_else(|| pfx.to_string());
    match layout {
        MlpLayout::Separate => {
            if s.gated {
                m.lin(&format!("{hn}.gate"), &format!("{pfx}.gate"), s.up_bias)?;
            }
            m.lin(&format!("{hn}.up"), &format!("{pfx}.up"), s.up_bias)?;
        }
        MlpLayout::FusedGateFirst => {
            let w = m.w(&format!("{pfx}.gate_up"))?;
            m.put(format!("{hn}.gate.w"), w.clone().rows(Pick::Range { start: 0, len: i }))?;
            m.put(format!("{hn}.up.w"), w.rows(Pick::Range { start: i, len: i }))?;
            if s.up_bias {
                let b = m.b(&format!("{pfx}.gate_up"))?;
                m.put(format!("{hn}.gate.b"), b.clone().rows(Pick::Range { start: 0, len: i }))?;
                m.put(format!("{hn}.up.b"), b.rows(Pick::Range { start: i, len: i }))?;
            }
        }
        MlpLayout::FusedInterleaved | MlpLayout::FusedGateFirstInOut => {
            return Err(LowerError::eval("internal: an expert-only layout on a dense MLP"));
        }
    }
    // `SUBLAYER_NORMS_V1`: BitNet's `ffn_sub_norm`.
    if let Some(n) = &s.inner_norm {
        m.norm(&format!("{hn}.sub_norm"), &format!("{pfx}.sub_norm"), n)?;
    }
    // xIELU's four scalars of the layer (`ACT_LEARNED_POINTWISE_V1`): `[1]` tensors and 0-dimensional buffers, read as `[1]`.
    if s.act == Act::Xielu {
        for k in ["alpha_p", "alpha_n", "beta", "eps"] {
            m.put(format!("{hn}.act.{k}"), Src::t(m.role(&format!("{pfx}.act_{k}"))?).reshape(vec![1]))?;
        }
    }
    m.lin(&format!("{hn}.down"), &format!("{pfx}.down"), s.down_bias)
}

fn moe(m: &mut M, s: &MoeSpec) -> Result<()> {
    let (e, i) = (s.experts, s.intermediate);
    m.lin("moe.router", "moe.router", s.router.linear_bias)?;
    if s.latent.is_some() {
        m.lin("moe.latent_in", "moe.latent_in", false)?;
        m.lin("moe.latent_out", "moe.latent_out", false)?;
    }
    if !s.gated && m.st.experts != MlpLayout::Separate {
        return Err(LowerError::not_lowerable("MOE_EXPERTS_PLAIN_V1: plain experts are read from one stored module per expert (the `Separate` layout)"));
    }
    if s.router.selection_bias {
        m.put("moe.sel_bias", Src::t(m.role("moe.sel_bias")?))?;
    }
    if s.router.scoring == Scoring::SqrtSoftplusHash {
        m.put("moe.tid2eid", Src::t(m.role("moe.tid2eid")?))?;
    }
    if s.router.per_expert_scale {
        m.put("moe.expert_scale", Src::t(m.role("moe.expert_scale")?))?;
    }
    if s.out_bias {
        m.put("moe.bias", Src::t(m.role("moe.bias")?))?;
    }
    match m.st.experts {
        MlpLayout::Separate => {
            for (name, role) in [("moe.experts.gate", "moe.gate"), ("moe.experts.up", "moe.up"), ("moe.experts.down", "moe.down")] {
                if !s.gated && role == "moe.gate" {
                    continue;
                }
                let r = m.role(role)?;
                // Pre-quantised experts keep their integers, one stored module per expert.
                let src = match m.quantised(role)? {
                    Some(fmt) => Src::Quant { module: r, fmt }.stack('E', e),
                    None => Src::t(suffixed(&r, ".weight")).stack('E', e),
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
        let spec = MlpSpec {
            intermediate: sh.intermediate,
            act: s.act,
            gated: s.gated,
            glu: if matches!(s.glu, Glu::LimitedGlu { .. }) { s.glu } else { Glu::Standard },
            up_bias: false,
            down_bias: false,
            inner_norm: None,
            name: None,
            sparsity: None,
        };
        mlp(m, &spec, "moe.shared", MlpLayout::Separate)?;
        if sh.sigmoid_gate {
            m.lin("moe.shared_gate", "moe.shared_gate", false)?;
        }
    }
    Ok(())
}
