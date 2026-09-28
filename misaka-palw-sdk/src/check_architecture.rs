//! **`palw-class check-architecture`** (RFC-0002 Phase F, F10; `phase-f-integration.md` §2.11):
//! would this Hugging Face architecture be admitted on this network — as a PALW-TIR program (IR
//! mode), or as a class of a shipped lineage (legacy mode)?
//!
//! **IR mode.** `misaka-palw-tir-lower` (non-consensus) lowers the config to a `TirProgramV1`, and
//! the verdict is `tir_admit_v1`'s (tir/core, spec 04b §10.3) — normal form, types, ranges, the
//! per-position costs, every commit point's court cone, every `Fixed` state's checkpoint interval,
//! admission's own work — under the network's `palw_tir_v1` ceilings (the fence's value where it is
//! armed; testnet-12's provisional v1 ceilings while it is dormant, and the output says which): the
//! network's per-position MACs and state bytes replace tir/core's starting values, the terminal
//! (per-tile) ceilings are tir/core's, and the network's program-byte, unrolled-node and peak-live
//! caps are checked against the same admission's numbers. `tile_len` and the history chunk are the
//! two layout facts admission reads; both are 64 unless given. What admission v10 adds beyond
//! `tir_admit_v1` — a declared layout's tiles and `C ≤ min C_j`, the canonical job, close bytes, the
//! window court — needs a layout, and an admitted program says so.
//!
//! **Legacy mode.** The config is mapped to the shipped lineage whose graph can express it — the
//! dense A16 family (Qwen2.5's graph) or the Qwen3.6 hybrid family (gated delta + gated attention +
//! MoE) — and the processor-same admission gate (`PalwClassSdk::preflight_admission`, which asks
//! `verify_class_admission_v9` under the network's fences) judges each class row: the SHIPPED rows
//! when the config's geometry is one this build tables, otherwise the family's graph PROJECTED at
//! the config's dimensions (a what-if; projected rows are never registrable as such). A config the
//! lineages cannot express is `NEEDS_KERNEL(what)`.
//!
//! Both modes call consensus functions; neither has an opinion of its own.

use crate::{PalwClassEntryV1, PalwClassSdk};
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::palw_class_admission_v2::palw_admission_shape_at_v1;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2;
use kaspa_consensus_core::palw_tir_v1::{PALW_T12_TIR_CEILINGS_V1, PalwTirCeilingsV1, palw_tir_prim_set_id_v1};
use kaspa_hashes::Hash64;
use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir::admit::{TirAdmitError, TirAdmitInputsV1, TirCeilingsV1};
use misaka_palw_tir_lower::spec::{Act, ArchSpec, Ffn, Gain, Glu, Mixer, NormKind, Position, Residual};

/// A verdict of either mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArchVerdictV1 {
    /// Every check this build can run passes. For IR mode, `pending` names what `tir_admit_v1` adds.
    Admissible {
        pending: Vec<String>,
    },
    Exceeds {
        limit: String,
        value: u128,
        cap: u128,
    },
    NeedsPrimitive(String),
    NotLowerable(String),
    /// The admission gate (legacy) or the range analysis / normal form (IR) refused, by name.
    Refused(String),
    /// A check needs something not given (the artifact, a verified reference).
    Unverified(String),
    NeedsKernel(String),
}

impl std::fmt::Display for ArchVerdictV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Admissible { pending } if pending.is_empty() => write!(f, "ADMISSIBLE"),
            Self::Admissible { pending } => write!(f, "ADMISSIBLE (pending {})", pending.join("; ")),
            Self::Exceeds { limit, value, cap } => write!(f, "EXCEEDS({limit}, {value}, {cap})"),
            Self::NeedsPrimitive(p) => write!(f, "NEEDS_PRIMITIVE({p})"),
            Self::NotLowerable(r) => write!(f, "NOT_LOWERABLE({r})"),
            Self::Refused(r) => write!(f, "REFUSED({r})"),
            Self::Unverified(r) => write!(f, "UNVERIFIED({r})"),
            Self::NeedsKernel(k) => write!(f, "NEEDS_KERNEL({k})"),
        }
    }
}

impl ArchVerdictV1 {
    pub fn is_admissible(&self) -> bool {
        matches!(self, Self::Admissible { .. })
    }
}

/// What IR mode reports.
#[derive(Clone, Debug)]
pub struct IrReportV1 {
    pub architecture: String,
    pub verdict: ArchVerdictV1,
    /// Weaker-than-verified facts (a remote-code reference, a class id that needs the artifact).
    pub unverified: Vec<String>,
    /// Which ceilings judged it, in words.
    pub ceilings_source: String,
    pub ceilings: PalwTirCeilingsV1,
    pub program_bytes: usize,
    pub blocks: usize,
    pub nodes: usize,
    /// Nodes of one position: `pre`, every layer's block, `post`.
    pub unrolled_nodes: u64,
    pub max_context: u32,
    pub graph_ir_root: Option<Hash64>,
    /// What admission ran with.
    pub inputs: Option<TirAdmitInputsV1>,
    /// `tir_admit_v1`'s report (text lines and JSON) — its numbers, or its refusal.
    pub admission_text: String,
    pub admission_json: serde_json::Value,
}

/// `tile_len` and the canonical history chunk IR mode admits with unless given (a layout's
/// `commit_tiles` and `h_tile`).
pub const IR_DEFAULT_TILE_LEN_V1: u32 = 64;
pub const IR_DEFAULT_H_CHUNK_V1: u32 = 64;

/// **The inputs `tir_admit_v1` runs with on this network**: the layout facts, tir/core's terminal
/// ceilings, and the network's per-position MACs, state bytes and admission work cap in place of
/// tir/core's.
pub fn tir_admit_inputs_v1(ceilings: &PalwTirCeilingsV1, tile_len: u32, h_chunk: u32) -> TirAdmitInputsV1 {
    TirAdmitInputsV1 {
        tile_len,
        h_chunk,
        ceilings: TirCeilingsV1 {
            max_position_macs: ceilings.max_macs_per_position,
            max_state_bytes: ceilings.max_state_bytes,
            max_cone_work: ceilings.max_cone_work,
            ..TirCeilingsV1::legacy_court_v1()
        },
    }
}

/// The ceilings a network judges IR programs by, and how that was decided.
pub fn tir_ceilings_v1(params: &Params) -> (PalwTirCeilingsV1, String, Hash64) {
    match params.palw_tir_v1 {
        Some(f) => (f.ceilings, format!("the network's palw_tir_v1 fence (activation {:?})", f.activation), f.prim_set_id),
        None => (
            PALW_T12_TIR_CEILINGS_V1,
            "palw_tir_v1 is dormant on this network: testnet-12's provisional v1 ceilings".to_string(),
            palw_tir_prim_set_id_v1(),
        ),
    }
}

/// **IR mode from a config**, at the default layout facts.
pub fn check_ir_config_v1(params: &Params, config_text: &str, long_history: bool) -> IrReportV1 {
    check_ir_config_at_v1(params, config_text, long_history, IR_DEFAULT_TILE_LEN_V1, IR_DEFAULT_H_CHUNK_V1)
}

/// **IR mode from a config** with `tile_len` and the history chunk given.
pub fn check_ir_config_at_v1(params: &Params, config_text: &str, long_history: bool, tile_len: u32, h_chunk: u32) -> IrReportV1 {
    let opts = misaka_palw_tir_lower::lower::LowerOpts {
        history_bound: if long_history {
            misaka_palw_tir::program::HISTORY_BOUND_V1_HELD
        } else {
            misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL
        },
        ..Default::default()
    };
    let (ceilings, source, _) = tir_ceilings_v1(params);
    let empty = |architecture: String, verdict: ArchVerdictV1| IrReportV1 {
        architecture,
        verdict,
        unverified: Vec::new(),
        ceilings_source: source.clone(),
        ceilings,
        program_bytes: 0,
        blocks: 0,
        nodes: 0,
        unrolled_nodes: 0,
        max_context: 0,
        graph_ir_root: None,
        inputs: None,
        admission_text: String::new(),
        admission_json: serde_json::Value::Null,
    };
    let spec = match misaka_palw_tir_lower::parse_config_str(config_text) {
        Ok(s) => s,
        Err(e) => return empty(String::new(), not_lowerable(e)),
    };
    let arch = spec.architecture.clone();
    let prep = match misaka_palw_tir_lower::hl::build_program(&spec).and_then(|hl| misaka_palw_tir_lower::lower::lower(&hl, &opts)) {
        Ok(lw) => lw,
        Err(e) => return empty(arch, not_lowerable(e)),
    };
    let mut r = check_ir_program_at_v1(params, &prep.program, tile_len, h_chunk);
    r.architecture = arch;
    if matches!(spec.reference, misaka_palw_tir_lower::spec::Reference::RemoteCode { .. }) {
        r.unverified
            .push("the architecture is remote code: the lowering follows its source, no installed transformers reference".into());
    }
    r
}

fn not_lowerable(e: misaka_palw_tir_lower::LowerError) -> ArchVerdictV1 {
    match e {
        misaka_palw_tir_lower::LowerError::NotLowerable(m) => ArchVerdictV1::NotLowerable(m),
        other => ArchVerdictV1::NotLowerable(other.to_string()),
    }
}

/// **IR mode on a program**, at the default layout facts.
pub fn check_ir_program_v1(params: &Params, program: &TirProgramV1) -> IrReportV1 {
    check_ir_program_at_v1(params, program, IR_DEFAULT_TILE_LEN_V1, IR_DEFAULT_H_CHUNK_V1)
}

/// **IR mode on a program**: the network's primitive set, then `tir_admit_v1` under the network's
/// ceilings, then the network caps admission does not know (program bytes, unrolled nodes, peak
/// live bytes) against admission's own numbers.
pub fn check_ir_program_at_v1(params: &Params, program: &TirProgramV1, tile_len: u32, h_chunk: u32) -> IrReportV1 {
    let (ceilings, source, prim_set) = tir_ceilings_v1(params);
    let bytes = program.encode();
    let max_context = program.history_bound.min(ceilings.max_context);
    let occurrences = std::iter::once(program.schedule.pre)
        .chain(program.schedule.layers.iter().copied())
        .chain(std::iter::once(program.schedule.post));
    let unrolled_nodes = occurrences.map(|b| program.blocks.get(b as usize).map_or(0, |b| b.nodes.len() as u64)).sum();
    let inputs = tir_admit_inputs_v1(&ceilings, tile_len, h_chunk);
    let mut r = IrReportV1 {
        architecture: String::new(),
        verdict: ArchVerdictV1::Admissible { pending: Vec::new() },
        unverified: vec!["the class id commits to the artifact root, which needs the artifact".into()],
        ceilings_source: source,
        ceilings,
        program_bytes: bytes.len(),
        blocks: program.blocks.len(),
        nodes: program.blocks.iter().map(|b| b.nodes.len()).sum(),
        unrolled_nodes,
        max_context,
        graph_ir_root: Some(kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_graph_ir_root_v1(&bytes)),
        inputs: Some(inputs),
        admission_text: String::new(),
        admission_json: serde_json::Value::Null,
    };
    // The primitive set first: a program over another set is not this network's to judge.
    if program.prim_set_id[..] != *prim_set.as_byte_slice() {
        r.verdict = ArchVerdictV1::NeedsPrimitive("the program names another primitive set than this network's prim_set_id".into());
        return r;
    }
    // `tir_admit_v1` over the canonical bytes: decode, normal form, types, ranges, costs, cones,
    // checkpoint intervals, cone work — refused by rule, or by limit and number.
    let verdict = misaka_palw_tir::admit::tir_admit_v1(&bytes, &inputs);
    r.admission_text = misaka_palw_tir_lower::admission::render(program, &inputs, &verdict);
    r.admission_json = misaka_palw_tir_lower::admission::to_json(program, &inputs, &verdict);
    let a = match verdict {
        Ok(a) => a,
        Err(TirAdmitError::Exceeds { limit, at, value, cap }) => {
            let limit = if at == "the position" || at == "the run" { limit.to_string() } else { format!("{limit} at {at}") };
            r.verdict = ArchVerdictV1::Exceeds { limit, value: value as u128, cap: cap as u128 };
            return r;
        }
        Err(TirAdmitError::Program(e)) => {
            r.verdict = ArchVerdictV1::Refused(format!("tir_admit_v1: {e}"));
            return r;
        }
        Err(TirAdmitError::Inputs(m)) => {
            r.verdict = ArchVerdictV1::Refused(format!("tir_admit_v1 inputs: {m}"));
            return r;
        }
    };
    let checks: [(&str, u128, u128); 3] = [
        ("max_program_bytes", bytes.len() as u128, ceilings.max_program_bytes as u128),
        ("max_unrolled_nodes", unrolled_nodes as u128, ceilings.max_unrolled_nodes as u128),
        ("max_peak_live_bytes", a.position.peak_live_bytes as u128, ceilings.max_peak_live_bytes as u128),
    ];
    for (limit, value, cap) in checks {
        if value > cap {
            r.verdict = ArchVerdictV1::Exceeds { limit: limit.into(), value, cap };
            return r;
        }
    }
    r.verdict = ArchVerdictV1::Admissible {
        pending: vec![
            "admission v10's layout checks (declared tiles, C ≤ min C_j, the canonical job, close bytes, the window court) need a declared layout".into(),
        ],
    };
    r
}

// ───────────────────────────── legacy mode ─────────────────────────────

/// One class row legacy mode judged.
#[derive(Clone, Debug)]
pub struct LegacyRowV1 {
    pub model_id: String,
    pub n_ctx: u32,
    /// A row this build tables (registrable), or the family's graph projected at the config's
    /// dimensions (a what-if).
    pub shipped: bool,
    pub class_id: Hash64,
    pub verdict: ArchVerdictV1,
}

#[derive(Clone, Debug)]
pub struct LegacyReportV1 {
    pub architecture: String,
    /// The lineage the config was mapped to.
    pub lineage: Option<&'static str>,
    pub verdict: ArchVerdictV1,
    pub rows: Vec<LegacyRowV1>,
}

/// The dense A16 family's requirements, first unmet one named.
fn dense_a16_misfit(s: &ArchSpec) -> Option<String> {
    if s.embedding.scale != 1.0 || s.embedding.positions.is_some() || s.embedding.norm.is_some() || s.embedding.proj_in {
        return Some("embedding scale/positions/norm/projection".into());
    }
    if s.head.bias || s.head.pre_scale != 1.0 || s.head.logit_scale != 1.0 || s.head.softcap.is_some() || s.head.proj_out {
        return Some("a head bias, scale, soft-cap or projection".into());
    }
    match s.final_norm {
        Some(n) if n.kind == NormKind::Rms && n.gain == Gain::W && !n.bias => {}
        _ => return Some("a final norm other than RMSNorm·w".into()),
    }
    for l in &s.layers {
        let Mixer::Attention(a) = &l.mixer else { return Some(format!("{} layers", mixer_name(&l.mixer))) };
        let Position::Rope(rope) = &a.position else { return Some("attention without RoPE (ALiBi or none)".into()) };
        if rope.style != misaka_palw_tir_lower::rope::RopeStyle::Half || rope.offset != 0 || rope.rotary_dim != a.head_dim {
            return Some("partial or interleaved RoPE".into());
        }
        if rope.freqs.rope_type != "default" {
            return Some(format!("RoPE scaling `{}`", rope.freqs.rope_type));
        }
        if a.qk_norm.is_some() {
            return Some("q/k norm".into());
        }
        if a.window.is_some() {
            return Some("sliding-window attention".into());
        }
        if a.softcap.is_some() || a.sinks || a.output_gate || a.o_bias || a.clip_qkv.is_some() || a.v_head_dim != a.head_dim {
            return Some("attention soft-cap, sinks, output gate, o bias, qkv clipping or unequal v heads".into());
        }
        match &l.ffn {
            Ffn::Mlp(m) if m.gated && m.act == Act::Silu && m.glu == Glu::Standard && !m.up_bias && !m.down_bias => {}
            Ffn::Mlp(m) => {
                return Some(format!("an MLP other than SwiGLU without biases ({:?}{})", m.act, if m.gated { ", gated" } else { "" }));
            }
            Ffn::Moe(_) => return Some("mixture-of-experts layers".into()),
            other => return Some(format!("{other:?} feed-forward")),
        }
        match &l.residual {
            Residual::Sequential { pre_mixer: Some(a1), post_mixer: None, pre_ffn: Some(a2), post_ffn: None, multiplier }
                if *multiplier == 1.0 && [a1, a2].iter().all(|n| n.kind == NormKind::Rms && n.gain == Gain::W && !n.bias) => {}
            _ => return Some("a residual wiring other than pre-RMSNorm·w (parallel, post-norm, sandwich, multipliers)".into()),
        }
        if l.post_scale != 1.0 {
            return Some("a residual rescale".into());
        }
    }
    None
}

fn mixer_name(m: &Mixer) -> &'static str {
    match m {
        Mixer::Attention(_) => "attention",
        Mixer::Mla(_) => "multi-head latent attention",
        Mixer::GatedDeltaNet(_) => "gated-delta",
        Mixer::Mamba(_) => "Mamba",
        Mixer::Mamba2(_) => "Mamba2",
        Mixer::RwkvTime(_) => "RWKV",
    }
}

/// The dense A16 geometry of a config the family can express.
fn dense_geometry(s: &ArchSpec) -> Option<kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1> {
    let Mixer::Attention(a) = &s.layers.first()?.mixer else { return None };
    let Ffn::Mlp(m) = &s.layers.first()?.ffn else { return None };
    Some(kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 {
        layer_count: u16::try_from(s.layers.len()).ok()?,
        hidden_dim: u32::try_from(s.hidden_size).ok()?,
        ffn_dim: u32::try_from(m.intermediate).ok()?,
        attn_heads: u16::try_from(a.heads).ok()?,
        attn_kv_heads: u16::try_from(a.kv_heads).ok()?,
        attn_head_dim: u32::try_from(a.head_dim).ok()?,
        vocab_size: u32::try_from(s.vocab_size).ok()?,
        ..kaspa_consensus_core::palw_qwen25_profile::QWEN25_1_5B
    })
}

/// The Qwen3.6 hybrid family's requirements and geometry (gated delta every layer but each
/// `interval`-th, which is gated full attention; MoE with a shared expert everywhere).
fn qwen36_geometry(s: &ArchSpec) -> Result<kaspa_consensus_core::palw_qwen36_profile::PalwQwen36GeometryV1, String> {
    let n = s.layers.len();
    let full: Vec<usize> = (0..n).filter(|i| matches!(s.layers[*i].mixer, Mixer::Attention(_))).collect();
    let interval = full.first().map(|i| i + 1).ok_or("no full-attention layer")?;
    if full.iter().enumerate().any(|(k, i)| *i != (k + 1) * interval - 1) || n % interval != 0 {
        return Err("a layer pattern other than every n-th layer full attention".into());
    }
    let Mixer::Attention(a) = &s.layers[interval - 1].mixer else { unreachable!("found above") };
    let g = s
        .layers
        .iter()
        .find_map(|l| if let Mixer::GatedDeltaNet(g) = &l.mixer { Some(g) } else { None })
        .ok_or("no gated-delta layer")?;
    if g.k_dim != g.v_dim {
        return Err("gated-delta key and value head dims differ".into());
    }
    let Position::Rope(rope) = &a.position else { return Err("full attention without RoPE".into()) };
    let (experts, top_k, moe_dim, shared) = match &s.layers[0].ffn {
        Ffn::Moe(m) => (m.experts, m.top_k, m.intermediate, m.shared.as_ref().map_or(0, |x| x.intermediate)),
        Ffn::Mlp(m) => (1, 1, m.intermediate, 0),
        other => return Err(format!("{other:?} feed-forward")),
    };
    let t = u32::try_from;
    Ok(kaspa_consensus_core::palw_qwen36_profile::PalwQwen36GeometryV1 {
        layer_count: u16::try_from(n).map_err(|e| e.to_string())?,
        full_attention_interval: u16::try_from(interval).map_err(|e| e.to_string())?,
        hidden_dim: t(s.hidden_size).map_err(|e| e.to_string())?,
        attn_heads: u16::try_from(a.heads).map_err(|e| e.to_string())?,
        attn_kv_heads: u16::try_from(a.kv_heads).map_err(|e| e.to_string())?,
        attn_head_dim: t(a.head_dim).map_err(|e| e.to_string())?,
        rope_dims: u16::try_from(rope.rotary_dim).map_err(|e| e.to_string())?,
        rope_freq_base_bits: (rope.freqs.theta as f32).to_bits(),
        gdn_k_heads: u16::try_from(g.k_heads).map_err(|e| e.to_string())?,
        gdn_v_heads: u16::try_from(g.v_heads).map_err(|e| e.to_string())?,
        gdn_head_dim: t(g.k_dim).map_err(|e| e.to_string())?,
        gdn_conv_kernel: u16::try_from(g.conv_kernel).map_err(|e| e.to_string())?,
        n_experts: t(experts).map_err(|e| e.to_string())?,
        experts_per_token: t(top_k).map_err(|e| e.to_string())?,
        moe_dim: t(moe_dim).map_err(|e| e.to_string())?,
        shared_dim: t(shared).map_err(|e| e.to_string())?,
        attn_output_gate: a.output_gate as u8,
        vocab_size: t(s.vocab_size).map_err(|e| e.to_string())?,
        ..kaspa_consensus_core::palw_qwen36_profile::QWEN36_35B_A3B
    })
}

/// Everything but the class constants (context, epsilon, threads, tile) — what a config states.
fn same_qwen36_dims(
    a: &kaspa_consensus_core::palw_qwen36_profile::PalwQwen36GeometryV1,
    b: &kaspa_consensus_core::palw_qwen36_profile::PalwQwen36GeometryV1,
) -> bool {
    let dims = |g: &kaspa_consensus_core::palw_qwen36_profile::PalwQwen36GeometryV1| -> [u64; 17] {
        [
            g.layer_count as u64,
            g.full_attention_interval as u64,
            g.hidden_dim as u64,
            g.attn_heads as u64,
            g.attn_kv_heads as u64,
            g.attn_head_dim as u64,
            g.rope_dims as u64,
            g.gdn_k_heads as u64,
            g.gdn_v_heads as u64,
            g.gdn_head_dim as u64,
            g.gdn_conv_kernel as u64,
            g.n_experts as u64,
            g.experts_per_token as u64,
            g.moe_dim as u64,
            g.shared_dim as u64,
            g.attn_output_gate as u64,
            g.vocab_size as u64,
        ]
    };
    dims(a) == dims(b)
}

/// The processor-same gate on one entry, under the network's shape at genesis.
fn gate(params: &Params, bundle: &PalwConsensusParamsV2, sdk: &PalwClassSdk, entry: &PalwClassEntryV1) -> ArchVerdictV1 {
    let shape = match palw_admission_shape_at_v1(params, bundle, &entry.profile, 0) {
        Ok(s) => s,
        Err(why) => return ArchVerdictV1::Refused(why),
    };
    // The gate does not read the root; a registration pins the artifact's.
    match sdk.preflight_admission(bundle, entry, Hash64::default(), &shape) {
        Ok(_) => ArchVerdictV1::Admissible { pending: Vec::new() },
        Err(why) => ArchVerdictV1::Refused(why),
    }
}

const DENSE_PROJECTED: [(u32, &str); 3] = [
    (2048, "projected/dense-a16/graph-v7@2048"),
    (8192, "projected/dense-a16/graph-v7@8192"),
    (2_097_152, "projected/dense-a16/graph-v7@2097152"),
];
const QWEN36_PROJECTED: [(u32, &str); 2] = [(512, "projected/qwen36/graph-v7@512"), (2_097_152, "projected/qwen36/graph-v7@2097152")];

/// **Legacy mode from a config.**
pub fn check_legacy_config_v1(
    params: &Params,
    bundle: &PalwConsensusParamsV2,
    sdk: &PalwClassSdk,
    config_text: &str,
) -> LegacyReportV1 {
    let spec = match misaka_palw_tir_lower::parse_config_str(config_text) {
        Ok(s) => s,
        Err(e) => {
            return LegacyReportV1 { architecture: String::new(), lineage: None, verdict: not_lowerable(e), rows: Vec::new() };
        }
    };
    let arch = spec.architecture.clone();
    let hybrid = spec.layers.iter().any(|l| matches!(l.mixer, Mixer::GatedDeltaNet(_)));
    let court = sdk.court();
    let ledger = sdk.ledger();
    let mut rows = Vec::new();
    let lineage;
    if hybrid {
        lineage = Some(crate::lineages::qwen36::QWEN36_LINEAGE_ID);
        let geometry = match qwen36_geometry(&spec) {
            Ok(g) => g,
            Err(why) => return LegacyReportV1 { architecture: arch, lineage, verdict: ArchVerdictV1::NeedsKernel(why), rows },
        };
        for c in misaka_palw_base0::classes::qwen36_canonical_classes_v1() {
            if !same_qwen36_dims(&c.geometry, &geometry) {
                continue;
            }
            if let Some(entry) = ledger.iter().find(|e| e.model_id == c.model_id) {
                rows.push(LegacyRowV1 {
                    model_id: entry.model_id.to_string(),
                    n_ctx: entry.profile.n_ctx,
                    shipped: true,
                    class_id: entry.class_id(),
                    verdict: gate(params, bundle, sdk, entry),
                });
            }
        }
        if rows.is_empty() {
            for (w, id) in QWEN36_PROJECTED {
                let g = kaspa_consensus_core::palw_qwen36_profile::qwen36_geometry_artifact_eps(
                    kaspa_consensus_core::palw_qwen36_profile::PalwQwen36GeometryV1 { n_ctx: w, ..geometry },
                );
                rows.push(projected_row(params, bundle, sdk, id, w, crate::lineages::qwen36::QWEN36_LINEAGE_ID, || {
                    Ok((
                        kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v7(g).map_err(|e| format!("{e:?}"))?,
                        kaspa_consensus_core::palw_qwen36_profile::qwen36_held_canonical_v1(w),
                    ))
                }));
            }
        }
    } else {
        lineage = Some(crate::lineages::dense::DENSE_LINEAGE_ID);
        if let Some(why) = dense_a16_misfit(&spec) {
            return LegacyReportV1 { architecture: arch, lineage, verdict: ArchVerdictV1::NeedsKernel(why), rows };
        }
        let Some(geometry) = dense_geometry(&spec) else {
            return LegacyReportV1 {
                architecture: arch,
                lineage,
                verdict: ArchVerdictV1::NeedsKernel("a geometry past the family's field widths".into()),
                rows,
            };
        };
        for c in misaka_palw_base0::classes::canonical_classes_v1(court) {
            if !matches!(c.source, misaka_palw_base0::classes::ArtifactSourceV1::ConvertedA16) {
                continue;
            }
            let a = &c.artifact_shape;
            let same = (a.n_layers, a.n_heads, a.n_kv_heads, a.d_head, a.d_ff, a.vocab)
                == (
                    geometry.layer_count as usize,
                    geometry.attn_heads as usize,
                    geometry.attn_kv_heads as usize,
                    geometry.attn_head_dim as usize,
                    geometry.ffn_dim as usize,
                    geometry.vocab_size as usize,
                );
            if !same {
                continue;
            }
            if let Some(entry) = ledger.iter().find(|e| e.model_id == c.model_id) {
                rows.push(LegacyRowV1 {
                    model_id: entry.model_id.to_string(),
                    n_ctx: entry.profile.n_ctx,
                    shipped: true,
                    class_id: entry.class_id(),
                    verdict: gate(params, bundle, sdk, entry),
                });
            }
        }
        if rows.is_empty() {
            for (w, id) in DENSE_PROJECTED {
                let g = kaspa_consensus_core::palw_qwen25_profile::qwen25_geometry_artifact_eps(
                    kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 { n_ctx: w, ..geometry },
                );
                rows.push(projected_row(params, bundle, sdk, id, w, crate::lineages::dense::DENSE_LINEAGE_ID, || {
                    let profile = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v7(g)
                        .map_err(|e| format!("{e:?}"))?;
                    let decode = kaspa_consensus_core::palw_qwen25_profile::QWEN25_A16_CANONICAL.1.max(1);
                    let floor = u32::try_from(kaspa_consensus_core::palw_context_ladder::palw_canonical_footprint_floor_v1(w))
                        .map_err(|e| e.to_string())?;
                    let prefill =
                        floor.checked_add(1).and_then(|x| x.checked_sub(decode)).ok_or("the canonical job does not fit the floor")?;
                    Ok((profile, (prefill, decode)))
                }));
            }
        }
    }
    let verdict = if rows.is_empty() {
        ArchVerdictV1::NeedsKernel("no row of the family projects at these dimensions".into())
    } else if let Some(r) = rows.iter().find(|r| r.verdict.is_admissible()) {
        let _ = r;
        ArchVerdictV1::Admissible { pending: Vec::new() }
    } else {
        rows[0].verdict.clone()
    };
    LegacyReportV1 { architecture: arch, lineage, verdict, rows }
}

fn projected_row(
    params: &Params,
    bundle: &PalwConsensusParamsV2,
    sdk: &PalwClassSdk,
    model_id: &'static str,
    n_ctx: u32,
    lineage_id: &'static str,
    build: impl FnOnce() -> Result<(kaspa_consensus_core::palw_step::PalwShapeProfileV3, (u32, u32)), String>,
) -> LegacyRowV1 {
    match build() {
        Ok((profile, canonical_job)) => {
            let entry = PalwClassEntryV1 { model_id, lineage_id, profile, canonical_job, needs_artifact_file: true };
            LegacyRowV1 {
                model_id: model_id.into(),
                n_ctx,
                shipped: false,
                class_id: entry.class_id(),
                verdict: gate(params, bundle, sdk, &entry),
            }
        }
        Err(why) => LegacyRowV1 {
            model_id: model_id.into(),
            n_ctx,
            shipped: false,
            class_id: Hash64::default(),
            verdict: ArchVerdictV1::Refused(why),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::network::NetworkId;
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    use std::path::PathBuf;

    fn network(id: &str) -> (Params, PalwConsensusParamsV2, PalwClassSdk) {
        let network_id: NetworkId = id.parse().expect("a network id");
        let params: Params = network_id.into();
        let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("{id} has no PALW V2 bundle") };
        let bundle = bundle.clone();
        let sdk = PalwClassSdk::builtin_v1(bundle.court, params.palw_prompt_ids_form_v1(), network_id.to_string().into_bytes());
        (params, bundle, sdk)
    }

    fn lower_dir(rel: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../misaka-palw-tir-lower").join(rel)
    }

    fn read(rel: &str) -> String {
        std::fs::read_to_string(lower_dir(rel)).expect("config")
    }

    #[test]
    fn ir_verdicts_on_every_hf_tiny_fixture() {
        let (params, _, _) = network("testnet-11");
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(lower_dir("tests/fixtures/hf"))
            .expect("the fixtures")
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.join("config.json").exists())
            .collect();
        dirs.sort();
        assert_eq!(dirs.len(), 57, "tir-lower's HF tiny fixtures");
        for d in dirs {
            let text = std::fs::read_to_string(d.join("config.json")).expect("config");
            let r = check_ir_config_v1(&params, &text, false);
            let name = d.file_name().expect("name").to_string_lossy().to_string();
            eprintln!("{name:>20}: {} ({} bytes, {} nodes)", r.verdict, r.program_bytes, r.nodes);
            assert!(r.verdict.is_admissible(), "{name}: {}", r.verdict);
            assert!(r.graph_ir_root.is_some() && r.admission_json["admitted"] == serde_json::json!(true), "{name}");
        }
    }

    #[test]
    fn ir_verdicts_on_real_configs_name_what_they_refuse() {
        let (params, _, _) = network("testnet-11");
        let verdict = |f: &str| check_ir_config_v1(&params, &read(&format!("tests/configs/real/{f}")), false).verdict;
        assert!(verdict("qwen2.5-1.5b-instruct.json").is_admissible());
        assert!(verdict("llama-3.1-8b.json").is_admissible());
        // Encoder-only models are not decoders.
        assert!(matches!(verdict("t5-small.json"), ArchVerdictV1::NotLowerable(_)));
        assert!(matches!(verdict("bert-base-uncased.json"), ArchVerdictV1::NotLowerable(_)));
        // 671B parameters at a 2^18-position history: past the provisional per-position MACs.
        assert!(matches!(verdict("deepseek-v3-bf16.json"), ArchVerdictV1::Exceeds { .. }), "{}", verdict("deepseek-v3-bf16.json"));
    }

    #[test]
    fn legacy_mode_lands_on_the_processor_verdicts_for_the_shipped_lineages() {
        for id in ["testnet-11", "devnet"] {
            let (params, bundle, sdk) = network(id);
            let ledger = sdk.ledger();
            // Dense A16: every shipped row of the Qwen2.5-1.5B geometry, each judged by the gate the
            // processor runs, reached here through the ledger rather than through the mapping.
            let r = check_legacy_config_v1(&params, &bundle, &sdk, &read("tests/configs/real/qwen2.5-1.5b-instruct.json"));
            assert_eq!(r.lineage, Some(crate::lineages::dense::DENSE_LINEAGE_ID));
            let want: Vec<&str> = misaka_palw_base0::classes::canonical_classes_v1(sdk.court())
                .into_iter()
                .filter(|c| matches!(c.source, misaka_palw_base0::classes::ArtifactSourceV1::ConvertedA16))
                .filter(|c| (c.artifact_shape.n_layers, c.artifact_shape.d_ff, c.artifact_shape.vocab) == (28, 8960, 151_936))
                .map(|c| c.model_id)
                .collect();
            assert!(!want.is_empty());
            assert_eq!(r.rows.iter().map(|x| x.model_id.as_str()).collect::<Vec<_>>(), want, "{id}");
            for row in &r.rows {
                assert!(row.shipped);
                let entry = ledger.iter().find(|e| e.model_id == row.model_id).expect("a ledger entry");
                assert_eq!(row.class_id, entry.class_id());
                let shape = palw_admission_shape_at_v1(&params, &bundle, &entry.profile, 0);
                let processor = shape.and_then(|s| sdk.preflight_admission(&bundle, entry, Hash64::default(), &s)).is_ok();
                assert_eq!(row.verdict.is_admissible(), processor, "{id} {}: {}", row.model_id, row.verdict);
            }
            // The Qwen3.6 hybrid at its shipped geometry: the lineage's own rows of that geometry.
            let r = check_legacy_config_v1(&params, &bundle, &sdk, &read("tests/configs/legacy/qwen3.6-35b-a3b-geometry.json"));
            assert_eq!(r.lineage, Some(crate::lineages::qwen36::QWEN36_LINEAGE_ID));
            let want: Vec<&str> = misaka_palw_base0::classes::qwen36_canonical_classes_v1()
                .into_iter()
                .filter(|c| c.geometry.hidden_dim == 2048 && c.geometry.layer_count == 40 && c.geometry.attn_output_gate == 1)
                .filter(|c| ledger.iter().any(|e| e.model_id == c.model_id))
                .map(|c| c.model_id)
                .collect();
            assert!(!want.is_empty(), "{id}");
            assert_eq!(r.rows.iter().map(|x| x.model_id.as_str()).collect::<Vec<_>>(), want, "{id}");
            for row in &r.rows {
                let entry = ledger.iter().find(|e| e.model_id == row.model_id).expect("a ledger entry");
                let shape = palw_admission_shape_at_v1(&params, &bundle, &entry.profile, 0);
                let processor = shape.and_then(|s| sdk.preflight_admission(&bundle, entry, Hash64::default(), &s)).is_ok();
                assert_eq!(row.verdict.is_admissible(), processor, "{id} {}: {}", row.model_id, row.verdict);
            }
        }
    }

    #[test]
    fn legacy_mode_projects_a_new_geometry_and_names_a_missing_kernel() {
        let (params, bundle, sdk) = network("testnet-11");
        // A dense decoder the family expresses at other dimensions: projected rows, gate verdicts.
        let r = check_legacy_config_v1(&params, &bundle, &sdk, &read("tests/configs/real/qwen2.5-0.5b.json"));
        assert_eq!(r.lineage, Some(crate::lineages::dense::DENSE_LINEAGE_ID));
        assert!(
            !r.rows.is_empty() && r.rows.iter().all(|x| !x.shipped),
            "{:?}",
            r.rows.iter().map(|x| &x.model_id).collect::<Vec<_>>()
        );
        // Structures no shipped lineage has.
        let gpt2 = check_legacy_config_v1(&params, &bundle, &sdk, &read("tests/fixtures/hf/gpt2/config.json"));
        assert!(matches!(gpt2.verdict, ArchVerdictV1::NeedsKernel(_)), "{}", gpt2.verdict);
        let mixtral = check_legacy_config_v1(&params, &bundle, &sdk, &read("tests/fixtures/hf/mixtral/config.json"));
        assert_eq!(mixtral.verdict, ArchVerdictV1::NeedsKernel("mixture-of-experts layers".into()));
    }
}
