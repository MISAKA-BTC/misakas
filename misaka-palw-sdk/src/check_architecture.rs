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
use misaka_palw_tir_lower::hf_schema::ReadOptions;
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
    /// **`ADMISSIBLE_GENERIC`** (RFC-0002 §8): registrable as data, and some wide patterns have no fused kernel in this build, so a
    /// node runs them on the generic kernels. `slowdown_permille` is the estimate ([`generic_slowdown_v1`]) of the generic-kernel
    /// position time over the fully fused one, in thousandths (1,300 = 1.3x); `patterns` names what runs generic. Speed only: nothing
    /// of it reaches a consensus object (F-3).
    AdmissibleGeneric {
        slowdown_permille: u32,
        patterns: Vec<String>,
    },
    /// **`LOWERABLE_UNVERIFIED`** (RFC-0002 §8): lowered from an architecture whose float reference is remote code this tool cannot run
    /// offline, so the fidelity column is empty; `inner` is the verdict the program itself earned, which holds.
    LowerableUnverified {
        inner: Box<ArchVerdictV1>,
    },
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
            Self::AdmissibleGeneric { slowdown_permille, patterns } => write!(
                f,
                "ADMISSIBLE_GENERIC (estimated slowdown {}.{:02}x on generic kernels: {})",
                slowdown_permille / 1000,
                (slowdown_permille % 1000) / 10,
                patterns.join("; ")
            ),
            Self::LowerableUnverified { inner } => write!(f, "LOWERABLE_UNVERIFIED (the program's own verdict: {inner})"),
        }
    }
}

impl ArchVerdictV1 {
    pub fn is_admissible(&self) -> bool {
        match self {
            Self::Admissible { .. } | Self::AdmissibleGeneric { .. } => true,
            Self::LowerableUnverified { inner } => inner.is_admissible(),
            _ => false,
        }
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
    /// **The seat need**: the bytes of the integer parameters the artifact carries (every parameter at its dtype,
    /// a per-layer one once per layer occurrence), i.e. what a seat must hold resident before any state. A
    /// registration may carry any artifact size: seat resources gate READINESS (staged enablement), not admission,
    /// so this is reported, never judged.
    pub artifact_bytes: u128,
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
    /// The feature report of a config ([`misaka_palw_tir_lower::model::ArchitectureReport`]): the
    /// model_type (informational), the features each `SUPPORTED` or `MISSING` with the capability that
    /// would close the gap, the support level, the adapter used, and whether the protocol would need a
    /// new primitive or court kernel. `None` for a program, which has no config.
    pub architecture_report: Option<misaka_palw_tir_lower::model::ArchitectureReport>,
}

/// `tile_len` and the canonical history chunk IR mode admits with unless given (a layout's
/// `commit_tiles` and `h_tile`).
pub const IR_DEFAULT_TILE_LEN_V1: u32 = 64;
pub const IR_DEFAULT_H_CHUNK_V1: u32 = 64;
/// The source length (the encoder's padded axis) IR mode judges an encoder–decoder at unless given: the encoder is ONE
/// position over this many rows, so its cost and cones scale with it (`docs/design/palw/tir/frontend-as-data-v1.md` §3.4).
pub const IR_DEFAULT_SOURCE_LEN_V1: u32 = 128;
/// The decoder's window (target positions its histories keep) it is judged at unless given.
pub const IR_DEFAULT_TARGET_LEN_V1: u32 = 128;

/// The lengths an encoder–decoder is judged at. These are DEFAULTS (128 source rows, 128 target positions): a
/// registration declares its own, and the preflight reports what fits (`palw-class check-architecture
/// --source-len N --target-len M`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IrShapeV1 {
    pub source_len: u32,
    pub target_len: u32,
}

impl Default for IrShapeV1 {
    fn default() -> Self {
        IrShapeV1 { source_len: IR_DEFAULT_SOURCE_LEN_V1, target_len: IR_DEFAULT_TARGET_LEN_V1 }
    }
}

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

/// The lowering IR mode checks: the program's history bound (`--held` for the long one), nothing
/// else set.
fn ir_lower_opts(long_history: bool) -> misaka_palw_tir_lower::lower::LowerOpts {
    misaka_palw_tir_lower::lower::LowerOpts {
        history_bound: if long_history {
            misaka_palw_tir::program::HISTORY_BOUND_V1_HELD
        } else {
            misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL
        },
        ..Default::default()
    }
}

/// **IR mode from a config** with `tile_len` and the history chunk given.
pub fn check_ir_config_at_v1(params: &Params, config_text: &str, long_history: bool, tile_len: u32, h_chunk: u32) -> IrReportV1 {
    check_ir_config_read_v1(params, config_text, None, &ReadOptions::default(), long_history, tile_len, h_chunk)
}

/// **IR mode from a config**, read through the adapter `read` names (the built-in that claims it, a
/// user-supplied adapter file, none) and, when given, the checkpoint's tensor names (which a Level-B
/// adapter's name templates are checked against): the feature report first, then lowering and
/// `tir_admit_v1`. A model is judged by the features it combines, never by its `model_type`.
pub fn check_ir_config_read_v1(
    params: &Params,
    config_text: &str,
    tensors: Option<&misaka_palw_tir_lower::hf_schema::TensorIndex>,
    read: &ReadOptions,
    long_history: bool,
    tile_len: u32,
    h_chunk: u32,
) -> IrReportV1 {
    check_ir_config_shaped_v1(params, config_text, tensors, read, long_history, tile_len, h_chunk, IrShapeV1::default())
}

/// [`check_ir_config_read_v1`] with the lengths an encoder–decoder is judged at given (a decoder ignores them).
#[allow(clippy::too_many_arguments)]
pub fn check_ir_config_shaped_v1(
    params: &Params,
    config_text: &str,
    tensors: Option<&misaka_palw_tir_lower::hf_schema::TensorIndex>,
    read: &ReadOptions,
    long_history: bool,
    tile_len: u32,
    h_chunk: u32,
    shape: IrShapeV1,
) -> IrReportV1 {
    let parsed = serde_json::from_str::<serde_json::Value>(&misaka_palw_tir_lower::hf_config::sanitize_json(config_text)).ok();
    let report = parsed.as_ref().map(|c| misaka_palw_tir_lower::model::analyze(c, tensors, read));
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
        artifact_bytes: 0,
        blocks: 0,
        nodes: 0,
        unrolled_nodes: 0,
        max_context: 0,
        graph_ir_root: None,
        inputs: None,
        admission_text: String::new(),
        admission_json: serde_json::Value::Null,
        architecture_report: report.clone(),
    };
    // An encoder–decoder is two programs (the encoder over the padded source, then the decoder's text stage): each is
    // admitted on its own, and the model is admissible when both are (ENCDEC_FROM_SPEC_V1).
    if let Some(c) = parsed.as_ref().filter(|c| misaka_palw_tir_lower::hf_schema::is_encoder_decoder(c)) {
        return check_ir_encdec_v1(params, c, tensors, read, tile_len, h_chunk, shape, report);
    }
    let spec = match misaka_palw_tir_lower::hf_config::parse_config_str_read(config_text, read, misaka_palw_tir_lower::quantfmt::QuantRegistry::builtin()) {
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
    r.architecture_report = report;
    if matches!(spec.reference, misaka_palw_tir_lower::spec::Reference::RemoteCode { .. }) {
        r.unverified
            .push("the architecture is remote code: the lowering follows its source, no installed transformers reference".into());
        r.verdict = ArchVerdictV1::LowerableUnverified { inner: Box::new(r.verdict.clone()) };
    }
    r
}

/// **IR mode on an encoder–decoder config**: the adapter of kind `encdec` builds the spec, both stages are lowered
/// (the encoder at `shape.source_len` source rows, the decoder over `shape.target_len`; defaults [`IrShapeV1`]) and each is
/// judged by [`check_ir_program_at_v1`]. The verdict is the first stage's that is not admissible, else admissible;
/// the numbers add up (`program_bytes`, `blocks`, `nodes`, `unrolled_nodes`), `admission_json` carries both stages'.
#[allow(clippy::too_many_arguments)]
fn check_ir_encdec_v1(
    params: &Params,
    config: &serde_json::Value,
    tensors: Option<&misaka_palw_tir_lower::hf_schema::TensorIndex>,
    read: &ReadOptions,
    tile_len: u32,
    h_chunk: u32,
    shape: IrShapeV1,
    report: Option<misaka_palw_tir_lower::model::ArchitectureReport>,
) -> IrReportV1 {
    use misaka_palw_tir_lower::lower::encdec::{hl_programs, lower_decoder, lower_encoder};
    let (ceilings, source, _) = tir_ceilings_v1(params);
    let refuse = |architecture: String, verdict: ArchVerdictV1| IrReportV1 {
        architecture,
        verdict,
        unverified: Vec::new(),
        ceilings_source: source.clone(),
        ceilings,
        program_bytes: 0,
        artifact_bytes: 0,
        blocks: 0,
        nodes: 0,
        unrolled_nodes: 0,
        max_context: 0,
        graph_ir_root: None,
        inputs: None,
        admission_text: String::new(),
        admission_json: serde_json::Value::Null,
        architecture_report: report.clone(),
    };
    let spec = match misaka_palw_tir_lower::hf_schema::read_encdec(config, read) {
        Ok(r) => r.spec,
        Err(f) => return refuse(String::new(), not_lowerable(f.error)),
    };
    let arch = spec.architecture.clone();
    let has = |n: &str| tensors.is_some_and(|t| t.has(n));
    let (lmax, wmax) = (shape.source_len, shape.target_len);
    let stages = hl_programs(&spec, lmax as usize, &has).and_then(|((ehl, _), (dhl, _))| {
        let enc = lower_encoder(&ehl, &spec, lmax)?;
        let dec = lower_decoder(&dhl, &spec, lmax, wmax)?;
        Ok((enc, dec))
    });
    let (enc, dec) = match stages {
        Ok(p) => p,
        Err(e) => return refuse(arch, not_lowerable(e)),
    };
    let (re, rd) = (check_ir_program_at_v1(params, &enc.program, tile_len, h_chunk), check_ir_program_at_v1(params, &dec.program, tile_len, h_chunk));
    let verdict = if !re.verdict.is_admissible() {
        re.verdict.clone()
    } else if !rd.verdict.is_admissible() {
        rd.verdict.clone()
    } else {
        // Both stages admissible: the model is as slow as its slower stage's estimate says.
        match (&re.verdict, &rd.verdict) {
            (ArchVerdictV1::AdmissibleGeneric { slowdown_permille: a, patterns: pa }, ArchVerdictV1::AdmissibleGeneric { slowdown_permille: b, patterns: pb }) => {
                ArchVerdictV1::AdmissibleGeneric { slowdown_permille: (*a).max(*b), patterns: pa.iter().chain(pb).cloned().collect() }
            }
            (g @ ArchVerdictV1::AdmissibleGeneric { .. }, _) => g.clone(),
            (_, g @ ArchVerdictV1::AdmissibleGeneric { .. }) => g.clone(),
            _ => rd.verdict.clone(),
        }
    };
    let mut unverified = re.unverified.clone();
    unverified.push(format!("stage 0 (encoder, {lmax} source rows): {}", re.verdict));
    unverified.push(format!("stage 1 (decoder, {wmax} target positions): {}", rd.verdict));
    IrReportV1 {
        architecture: arch,
        verdict,
        unverified,
        ceilings_source: re.ceilings_source.clone(),
        ceilings: re.ceilings,
        program_bytes: re.program_bytes + rd.program_bytes,
        artifact_bytes: re.artifact_bytes + rd.artifact_bytes,
        blocks: re.blocks + rd.blocks,
        nodes: re.nodes + rd.nodes,
        unrolled_nodes: re.unrolled_nodes + rd.unrolled_nodes,
        max_context: rd.max_context,
        graph_ir_root: None,
        inputs: re.inputs,
        admission_text: format!("── stage 0: the encoder ──\n{}\n── stage 1: the decoder ──\n{}", re.admission_text, rd.admission_text),
        admission_json: serde_json::json!({"stages": [re.admission_json, rd.admission_json]}),
        architecture_report: report,
    }
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

/// The bytes of a program's integer parameters: each at its dtype, a per-layer one once per layer occurrence.
pub fn artifact_bytes_of(program: &TirProgramV1) -> u128 {
    let layers = program.schedule.layers.len() as u128;
    program
        .params
        .iter()
        .map(|d| d.shape.iter().map(|x| *x as u128).product::<u128>() * d.dtype.width() as u128 * if d.per_layer { layers } else { 1 })
        .sum()
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
        artifact_bytes: artifact_bytes_of(program),
        blocks: program.blocks.len(),
        nodes: program.blocks.iter().map(|b| b.nodes.len()).sum(),
        unrolled_nodes,
        max_context,
        graph_ir_root: Some(kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_graph_ir_root_v1(&bytes)),
        inputs: Some(inputs),
        admission_text: String::new(),
        admission_json: serde_json::Value::Null,
        architecture_report: None,
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
    let pending = vec![
        "admission v10's layout checks (declared tiles, C ≤ min C_j, the canonical job, close bytes, the window court) need a declared layout".into(),
    ];
    r.verdict = match generic_slowdown_v1(program) {
        Some((slowdown_permille, patterns)) => ArchVerdictV1::AdmissibleGeneric { slowdown_permille, patterns },
        None => ArchVerdictV1::Admissible { pending },
    };
    r
}

/// **The weight of a generic wide node beyond a fused pass** in [`generic_slowdown_v1`]: the generic backend runs a node whose working
/// type is `i128` as its own pass over `i128` lanes (two machine words, a table gather or a division per element), where a fused kernel
/// visits each element once in machine words. An assumed weight, not a measurement (`tir-exec-bench --fused-kernels` measures a
/// kernel against its generic form on a given machine; this constant is only the ratio's order); the verdict calls it an estimate.
pub const GENERIC_WIDE_PASS_FACTOR_V1: u64 = 3;

/// Below this share of a position's estimated work the generic wide passes do not change the verdict (`ADMISSIBLE` stays).
pub const GENERIC_SHARE_FLOOR_PERMILLE_V1: u64 = 20;

/// **`ADMISSIBLE_GENERIC`'s estimate** (RFC-0002 §8): `Some((slowdown in thousandths, the patterns that run generic))` when more than
/// [`GENERIC_SHARE_FLOOR_PERMILLE_V1`] of the position's estimated work is in wide (`i128`-working) nodes no fused kernel of this build
/// matches; `None` when the fused kernels cover every such pattern (the verdict stays `ADMISSIBLE`).
///
/// The model, over the program as it stands (before any params exist): the work of a node is its output's element count (a `MatMul`
/// is `m·k·n / 8` — the dot product is one vectorised kernel on either backend, so it is never a fusion target), summed over the
/// position's occurrences with an `H` dimension counted as one row; a node outside every matched region ([`misaka_palw_tir_exec::fused::match_program`],
/// structural, so optimistic: whether a region runs fused on a node also needs its operands' ranges, which an artifact decides) whose
/// working type is `i128` costs [`GENERIC_WIDE_PASS_FACTOR_V1`] times a fused pass. The slowdown is total / (total − extra) over the
/// generic run — i.e. what generic costs against all wide patterns fused.
pub fn generic_slowdown_v1(program: &TirProgramV1) -> Option<(u32, Vec<String>)> {
    use misaka_palw_tir::{DType, Dim, Prim};
    let plan = misaka_palw_tir_exec::TirPlan::compile(program).ok()?;
    let regions = misaka_palw_tir_exec::fused::match_program(program);
    let numel = |t: &misaka_palw_tir::TensorType| -> u64 {
        t.shape.iter().map(|d| if let Dim::Fixed(n) = d { u64::from(*n) } else { 1 }).product::<u64>().max(1)
    };
    let (mut total, mut extra) = (0u64, 0u64);
    let mut by_pattern: std::collections::BTreeMap<String, (u64, u64)> = Default::default();
    for bi in plan.occurrences.iter().map(|(b, _)| *b as usize) {
        let Some(block) = plan.blocks.get(bi) else { continue };
        let inside: std::collections::BTreeSet<u16> = regions.get(bi).into_iter().flatten().flat_map(|r| r.nodes.iter().copied()).collect();
        for (i, n) in block.nodes.iter().enumerate() {
            let work = match &n.prim {
                Prim::MatMul => {
                    let k = n.in_types.first().and_then(|t| t.shape.last()).map_or(1, |d| if let Dim::Fixed(k) = d { u64::from(*k) } else { 1 });
                    (numel(&n.out) * k / 8).max(1)
                }
                _ => numel(&n.out),
            };
            total += work;
            let wide = n.work == misaka_palw_tir_exec::plan::Work::I128 || n.out.dtype == DType::I128;
            let computed = !matches!(n.prim, Prim::MatMul | Prim::Reshape | Prim::Gather { .. } | Prim::StateWrite { .. });
            if wide && computed && !inside.contains(&(i as u16)) {
                let e = work * (GENERIC_WIDE_PASS_FACTOR_V1 - 1);
                extra += e;
                let slot = by_pattern.entry(prim_name(&n.prim)).or_default();
                slot.0 += 1;
                slot.1 += work;
            }
        }
    }
    let total_generic = total + extra;
    if total == 0 || extra * 1000 < total_generic * GENERIC_SHARE_FLOOR_PERMILLE_V1 {
        return None;
    }
    let slowdown = (total_generic * 1000 / total).min(u64::from(u32::MAX)) as u32;
    let mut patterns: Vec<(String, (u64, u64))> = by_pattern.into_iter().collect();
    patterns.sort_by_key(|(_, (_, w))| std::cmp::Reverse(*w));
    let patterns = patterns.into_iter().take(6).map(|(k, (n, w))| format!("{k} x{n} ({w} element-ops)")).collect();
    Some((slowdown, patterns))
}

/// A primitive's name without its attributes.
fn prim_name(p: &misaka_palw_tir::Prim) -> String {
    let d = format!("{p:?}");
    d.split(|c: char| !c.is_alphanumeric()).next().unwrap_or("").to_string()
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
        Mixer::ShortConv(_) => "gated short convolution",
        Mixer::Parallel(_) => "parallel mixer branches",
        Mixer::None => "no mixer (the layer is its feed-forward)",
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

// ───────────────────────────── LoRA budget (RFC-0004) ─────────────────────────────

/// `--lora-budget`'s adapter rank unless given (`lora_alpha` is twice it).
pub const LORA_BUDGET_DEFAULT_RANK_V1: usize = 16;
/// The context a LoRA budget's admission work is sized at unless given.
pub const LORA_BUDGET_DEFAULT_CONTEXT_V1: u32 = 2048;
/// How far past admission's work cap a budget's sizing is followed before it says "past": 4×.
pub const LORA_BUDGET_SIZING_REACH_V1: u64 = 4 * kaspa_consensus_core::palw_tir_close_size_v1::PALW_TIR_CLOSE_SIZING_WORK_CAP_V1;

/// What a LoRA budget is measured at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoraBudgetChoiceV1 {
    /// The adapter's rank `r` (`lora_alpha` is `2r`).
    pub rank: usize,
    /// The context the admission work is sized over (the layout's longest job); `None` for
    /// [`LORA_BUDGET_DEFAULT_CONTEXT_V1`], or the widest the program admits when that is shorter.
    pub context: Option<u32>,
    /// The logits node's tile. With `None`, the widest divisor of 4,096 lanes is used whose
    /// terminal closes the chain can carry, sized over the parent (declare-layout starts from an
    /// estimate that can choose a narrower one: pass the tile the class is declared at).
    pub logits_tile: Option<u32>,
    pub tile_len: u32,
    pub h_chunk: u32,
    /// The held (long) history bound, as IR mode's `--held`.
    pub long_history: bool,
    /// The lowering's window (`palw-tir-fidelity --max-window`), for a model whose state at the
    /// history bound is past the ceiling; `None` keeps the model's own windows.
    pub max_window: Option<u32>,
    /// The layout's checkpoint interval `C` (at most `min_j C_j`); `None` for `min_j C_j`, which a
    /// declaration may narrow.
    pub checkpoint_interval: Option<u32>,
}

impl Default for LoraBudgetChoiceV1 {
    fn default() -> Self {
        Self {
            rank: LORA_BUDGET_DEFAULT_RANK_V1,
            context: None,
            logits_tile: None,
            tile_len: IR_DEFAULT_TILE_LEN_V1,
            h_chunk: IR_DEFAULT_H_CHUNK_V1,
            long_history: false,
            max_window: None,
            checkpoint_interval: None,
        }
    }
}

/// What admission's close sizing measured: its work, the largest terminal close as carried, and
/// PALW-TIR-38's verdict on the closes (`palw_tir_carried_close_bounds_admit_v1`: every close within
/// what the chain carries, every dissected point's root claim within one carrier).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoraSizedV1 {
    pub work: u64,
    pub largest_close: u64,
    pub carried: Result<(), String>,
}

/// One adapter target set, lowered as a LoRA candidate of the config's model.
#[derive(Clone, Debug)]
pub struct LoraBudgetRowV1 {
    /// A module leaf (`q_proj`), a set (`attention`, `mlp`), `all-linear`, or `fallback`.
    pub name: String,
    /// The leaves the adapter targets (for `all-linear`, what it resolved to).
    pub targets: Vec<String>,
    /// Why the adapter was not attached or lowered (a fused projection, for one), when it was not.
    pub refused: Option<String>,
    /// The adapter section: its params (the candidate's params `P..`) and their bytes over every
    /// instance.
    pub adapter_params: usize,
    pub adapter_bytes: u64,
    /// The candidate's nodes, block by block.
    pub block_nodes: Vec<usize>,
    /// Every block within 512 nodes (NF-12). Admission refuses a block past it by name.
    pub fits: bool,
    /// Admission's close sizing of the composite (`PalwTirParamFormV1::Composite { p }`), or why it
    /// was not measured (past [`LORA_BUDGET_SIZING_REACH_V1`] included).
    pub sized: Result<LoraSizedV1, String>,
}

impl LoraBudgetRowV1 {
    fn refused(name: &str, targets: Vec<String>, why: String) -> Self {
        Self {
            name: name.to_string(),
            targets,
            refused: Some(why),
            adapter_params: 0,
            adapter_bytes: 0,
            block_nodes: Vec::new(),
            fits: false,
            sized: Err("not sized".into()),
        }
    }

    /// Within every budget a composite's admission holds it to: the blocks' nodes, the sizing's
    /// work cap, and the largest close the chain can carry.
    pub fn admissible(&self, cap: u64) -> bool {
        self.refused.is_none() && self.fits && matches!(&self.sized, Ok(s) if s.work <= cap && s.carried.is_ok())
    }
}

/// **What a LoRA candidate of a config costs**, per adapter target set (RFC-0004 §6.3): which sets
/// fit a block's 512 nodes, and what admission's close sizing of the composite takes against its
/// cap, with the largest close against what the chain can carry.
#[derive(Clone, Debug)]
pub struct LoraBudgetReportV1 {
    pub architecture: String,
    pub rank: usize,
    /// Every block's name, its occurrences in the schedule, and the parent's nodes in it.
    pub blocks: Vec<(String, usize, usize)>,
    /// The parent's own sizing (`Multiproof`), for scale.
    pub parent: Result<LoraSizedV1, String>,
    pub rows: Vec<LoraBudgetRowV1>,
    /// The layout the work was sized at.
    pub context: u32,
    pub logits_tile: u32,
    pub checkpoint_interval: u32,
    pub tile_len: u32,
    pub h_chunk: u32,
    /// Admission's work cap (`PALW_TIR_CLOSE_SIZING_WORK_CAP_V1`) and the most one close may carry
    /// (`palw_tir_carriable_close_bytes_v1`).
    pub work_cap: u64,
    pub carriable: u64,
}

/// The sizing admission runs, at `choice`'s layout, in `form`: what it measures (its work followed
/// up to [`LORA_BUDGET_SIZING_REACH_V1`]), and the layout's checkpoint interval.
fn close_sizing(
    params: &Params,
    program: &TirProgramV1,
    choice: &crate::tir_layout::TirLayoutChoiceV1,
    interval: Option<u32>,
    form: kaspa_consensus_core::palw_tir_close_size_v1::PalwTirParamFormV1,
    carriable: u64,
) -> Result<(LoraSizedV1, u32), String> {
    use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1};
    use kaspa_consensus_core::palw_tir_close_size_v1::{
        PALW_TIR_CLOSE_SIZING_OVER_CAP_V1, PalwTirCloseSizingV1, palw_tir_worst_closes_work_v1,
    };
    let mut layout = crate::tir_layout::tir_default_layout_v1(params, program, choice)?;
    if let Some(c) = interval {
        if c == 0 || c > layout.checkpoint_interval {
            return Err(format!(
                "a checkpoint interval of {c}: admission takes 1..={} (min C_j) for this program",
                layout.checkpoint_interval
            ));
        }
        layout.checkpoint_interval = c;
    }
    let c = layout.checkpoint_interval;
    let class =
        PalwTirClassV1 { version: PALW_TIR_CLASS_VERSION_V1, program: program.encode(), layout, tokenizer_id: Hash64::default() };
    let id = class.class_id(&Hash64::default());
    let space = kaspa_consensus_core::palw_tir_step_v1::PalwTirStepSpaceV1::new(&class).map_err(|e| e.to_string())?;
    let longest = kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_canonical_context_v1(&class, id, (1, class.layout.max_context))
        .ok_or("the layout has no longest job")?;
    let inventory = kaspa_consensus_core::palw_tir_court_v1::PalwTirInventoryIndexV1::new(&space.program)
        .ok_or("the program's inventory has no index")?;
    let sizing = PalwTirCloseSizingV1 { form, court: true, cap: LORA_BUDGET_SIZING_REACH_V1, stop_above: None };
    match palw_tir_worst_closes_work_v1(&space, &inventory, &longest, &sizing) {
        Ok((bounds, work)) => {
            let largest_close = bounds.iter().map(|b| b.close_bytes).max().unwrap_or(0);
            let carried = kaspa_consensus_core::palw_tir_admission_v1::palw_tir_carried_close_bounds_admit_v1(&bounds, carriable)
                .map_err(|e| e.to_string());
            Ok((LoraSizedV1 { work, largest_close, carried }, c))
        }
        Err(e) if e == PALW_TIR_CLOSE_SIZING_OVER_CAP_V1 => {
            Err(format!("past {LORA_BUDGET_SIZING_REACH_V1} steps (4× the cap) at checkpoint interval {c}"))
        }
        Err(e) => Err(e),
    }
}

/// **`check-architecture --lora-budget`**: which LoRA adapters of the config's model a composite
/// candidate can carry (RFC-0004 §6.3). Rows, in order:
/// * every module kind an adapter can target on the model, alone;
/// * the attention set and the MLP set;
/// * PEFT's `all-linear`.
///
/// Each is lowered at rank `choice.rank` with its params after the parent's
/// (`lower::adapter_params_last`), as `palw-tir-fidelity --adapter` lowers a candidate. For each,
/// the report gives the adapter section's size, every block's nodes against the 512 a block may
/// hold (NF-12, the composite rule), and admission's close sizing of the composite. The sizing is
/// run as `palw_tir_composite_admits_v1` runs it: every terminal close in the `Composite { p }`
/// form, over the longest job of a layout at `choice`'s context and tiles, with its work against the
/// cap and its largest close against what the chain can carry. When `all-linear` is past any of
/// those budgets, a greedy fallback follows: the targets, in order, that keep all of them.
pub fn check_lora_budget_v1(
    params: &Params,
    bundle: &PalwConsensusParamsV2,
    config_text: &str,
    choice: &LoraBudgetChoiceV1,
) -> Result<LoraBudgetReportV1, String> {
    use kaspa_consensus_core::palw_improve_composite_v1::{PalwTirCompositeErrorV1, palw_tir_composite_rule_v1};
    use kaspa_consensus_core::palw_tir_close_size_v1::{PALW_TIR_CLOSE_SIZING_WORK_CAP_V1, PalwTirParamFormV1};
    use misaka_palw_tir_lower::{hl, lora, lower};
    let opts = misaka_palw_tir_lower::lower::LowerOpts { max_window: choice.max_window, ..ir_lower_opts(choice.long_history) };
    let spec = misaka_palw_tir_lower::parse_config_str(config_text).map_err(|e| e.to_string())?;
    // A program under the tiled logits scheme (the lowerer names none), with `P`.
    // Also the roles given adapter params (`{role}.lora_a` in the HL program): the projections an
    // adapter reaches in some layer.
    let lowered = |spec: &ArchSpec| -> Result<(TirProgramV1, usize, std::collections::BTreeSet<String>), String> {
        let hl = hl::build_program(spec).map_err(|e| e.to_string())?;
        let adapted = hl.params.iter().filter_map(|d| d.name.strip_suffix(".lora_a").map(str::to_string)).collect();
        let mut lw = lower::lower(&hl, &opts).map_err(|e| e.to_string())?;
        let p = match spec.adapter {
            Some(_) => lower::adapter_params_last(&mut lw).map_err(|e| e.to_string())?,
            None => lw.program.params.len(),
        };
        Ok((crate::tir_layout::tir_program_with_scheme_v1(&lw.program, None)?, p, adapted))
    };
    let (parent, _, _) = lowered(&spec)?;
    let (ceilings, _, _) = tir_ceilings_v1(params);
    let widest = parent.history_bound.min(ceilings.max_context).min(choice.max_window.unwrap_or(u32::MAX));
    let context = choice.context.unwrap_or(LORA_BUDGET_DEFAULT_CONTEXT_V1.min(widest));
    let cap = PALW_TIR_CLOSE_SIZING_WORK_CAP_V1;
    let carriable = kaspa_consensus_core::palw_tir_admission_v1::palw_tir_carriable_close_bytes_v1(&bundle.court);
    let layout_at = |logits_tile: u32| crate::tir_layout::TirLayoutChoiceV1 {
        max_context: Some(context),
        tile_len: choice.tile_len,
        h_chunk: choice.h_chunk,
        logits_scheme: None,
        logits_tile: Some(logits_tile),
    };
    // The logits tile: the one given, else the widest whose closes the parent's sizing finds
    // carriable (a candidate's head is its parent's). declare-layout's estimate is not used here: it
    // can rule out a tile the sizing carries (SmolLM2-1.7B's 512).
    let tiles: Vec<u32> = match choice.logits_tile {
        Some(t) => vec![t],
        None => (2..=12).rev().map(|k| 1u32 << k).collect(),
    };
    // A sizing that fails (past its reach, most often) ends the search: a narrower tile only adds
    // closes. With none carriable, the widest tile's result is the one reported.
    type Tried = (u32, Result<(LoraSizedV1, u32), String>);
    let mut widest_tried: Option<Tried> = None;
    let mut carriable_at: Option<Tried> = None;
    for t in tiles {
        let sized =
            close_sizing(params, &parent, &layout_at(t), choice.checkpoint_interval, PalwTirParamFormV1::Multiproof, carriable);
        let (carried, failed) = (matches!(&sized, Ok((s, _)) if s.carried.is_ok()), sized.is_err());
        if carried {
            carriable_at = Some((t, sized));
            break;
        }
        widest_tried.get_or_insert((t, sized));
        if failed {
            break;
        }
    }
    let (logits_tile, parent_sized) = carriable_at.or(widest_tried).ok_or("no logits tile to size at")?;
    let layout_choice = layout_at(logits_tile);
    // The interval sized at: the one given, else the layout's (`min_j C_j`); 0 when none derives.
    let checkpoint_interval = match (&parent_sized, choice.checkpoint_interval) {
        (Ok((_, c)), _) => *c,
        (Err(_), Some(c)) => c,
        (Err(_), None) => {
            crate::tir_layout::tir_default_layout_v1(params, &parent, &layout_choice).map_or(0, |l| l.checkpoint_interval)
        }
    };
    // A composite's closes are its parent's and more: past the cap with the parent, past it with any
    // adapter.
    let parent_past = match &parent_sized {
        Ok((s, _)) if s.work <= cap => None,
        Ok(_) => Some("not sized: the parent alone is past the work cap".to_string()),
        Err(e) => Some(format!("not sized: the parent's sizing fails ({e})")),
    };
    let mut occurrences = vec![0usize; parent.blocks.len()];
    for b in
        std::iter::once(parent.schedule.pre).chain(parent.schedule.layers.iter().copied()).chain(std::iter::once(parent.schedule.post))
    {
        if let Some(n) = occurrences.get_mut(b as usize) {
            *n += 1;
        }
    }
    let blocks = parent.blocks.iter().zip(&occurrences).map(|(b, n)| (b.name.clone(), *n, b.nodes.len())).collect();
    let rank = choice.rank;
    let listed = lora::targets(&spec);
    // One row: the candidate with `targets` (a list of leaves, or PEFT's "all-linear").
    let row = |name: &str, targets: &[String], all_linear: bool| -> LoraBudgetRowV1 {
        let named = if all_linear { vec!["all-linear".to_string()] } else { targets.to_vec() };
        let modules = if all_linear { serde_json::json!("all-linear") } else { serde_json::json!(targets) };
        let config =
            serde_json::json!({ "peft_type": "LORA", "r": rank, "lora_alpha": 2 * rank, "target_modules": modules }).to_string();
        let mut s = spec.clone();
        let (cand, p, adapted) = match lora::attach(&mut s, &config).map_err(|e| e.to_string()).and_then(|_| lowered(&s)) {
            Ok(x) => x,
            Err(e) => return LoraBudgetRowV1::refused(name, named, e),
        };
        let instances = kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_param_instances_v1(&cand);
        let mut r = LoraBudgetRowV1 {
            name: name.to_string(),
            // What "all-linear" resolved to: the projections given adapter params, which are the ones
            // the lowering adapts in some layer (never a router or an expert).
            targets: match all_linear {
                true => listed.iter().filter(|t| adapted.contains(&t.role)).map(|t| t.leaf.clone()).collect(),
                false => named,
            },
            refused: None,
            adapter_params: cand.params.len() - p,
            adapter_bytes: (p..cand.params.len())
                .map(|j| misaka_palw_tir_artifact::tensor_bytes_v1(&cand, j as u16) * instances[j].len() as u64)
                .sum(),
            block_nodes: cand.blocks.iter().map(|b| b.nodes.len()).collect(),
            fits: false,
            sized: Err("not sized".into()),
        };
        match palw_tir_composite_rule_v1(&parent, &cand, p as u32) {
            Ok(()) => {
                r.fits = true;
                r.sized = match &parent_past {
                    Some(why) => Err(why.clone()),
                    None => close_sizing(
                        params,
                        &cand,
                        &layout_choice,
                        choice.checkpoint_interval,
                        PalwTirParamFormV1::Composite { p: p as u32 },
                        carriable,
                    )
                    .map(|(x, _)| x),
                };
            }
            Err(PalwTirCompositeErrorV1::NodeBudget { block, nodes }) => {
                r.sized = Err(format!("not sized: block {block} holds {nodes} nodes, past 512"));
            }
            Err(e) => r.refused = Some(e.to_string()),
        }
        r
    };
    let mut rows = Vec::new();
    for t in &listed {
        rows.push(match t.fused {
            false => row(&t.leaf, std::slice::from_ref(&t.leaf), false),
            true => LoraBudgetRowV1::refused(
                &t.leaf,
                vec![t.leaf.clone()],
                format!("`{}` is a fused projection ({}), which the lowering does not adapt yet", t.leaf, t.role),
            ),
        });
    }
    let unfused: Vec<&lora::LoraTarget> = listed.iter().filter(|t| !t.fused).collect();
    let set =
        |pred: &dyn Fn(&str) -> bool| -> Vec<String> { unfused.iter().filter(|t| pred(&t.role)).map(|t| t.leaf.clone()).collect() };
    for (name, leaves) in
        [("attention", set(&|r| r.starts_with("attn."))), ("mlp", set(&|r| r.starts_with("mlp.") || r.starts_with("moe.shared.")))]
    {
        if leaves.len() > 1 {
            rows.push(row(name, &leaves, false));
        }
    }
    let all = row("all-linear", &[], true);
    let widest_ok = all.admissible(cap);
    rows.push(all);
    if !widest_ok && !unfused.is_empty() && parent_past.is_none() {
        let mut kept: Vec<String> = Vec::new();
        let mut best: Option<LoraBudgetRowV1> = None;
        for t in &unfused {
            // A target past a budget on its own is past it with company.
            if rows.iter().any(|r| r.targets == [t.leaf.clone()] && !r.admissible(cap)) {
                continue;
            }
            let mut trial = kept.clone();
            trial.push(t.leaf.clone());
            let r = row("fallback", &trial, false);
            if r.admissible(cap) {
                kept = trial;
                best = Some(r);
            }
        }
        rows.push(best.unwrap_or_else(|| LoraBudgetRowV1::refused("fallback", Vec::new(), "no target keeps every budget".into())));
    }
    Ok(LoraBudgetReportV1 {
        architecture: spec.architecture.clone(),
        rank,
        blocks,
        parent: parent_sized.map(|(x, _)| x),
        rows,
        context,
        logits_tile,
        checkpoint_interval,
        tile_len: choice.tile_len,
        h_chunk: choice.h_chunk,
        work_cap: cap,
        carriable,
    })
}

impl LoraBudgetReportV1 {
    fn sized_text(&self, s: &Result<LoraSizedV1, String>) -> String {
        match s {
            Ok(s) => format!(
                "{} ({:.1}%){}  {} B{}",
                s.work,
                100.0 * s.work as f64 / self.work_cap as f64,
                if s.work > self.work_cap { " PAST THE CAP" } else { "" },
                s.largest_close,
                match &s.carried {
                    Ok(()) => String::new(),
                    Err(e) => format!(" NOT CARRIABLE: {e}"),
                }
            ),
            Err(e) => e.clone(),
        }
    }

    /// The report as text lines.
    pub fn render(&self) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        let _ = writeln!(out, "LoRA budget — {}, rank {} (lora_alpha {})", self.architecture, self.rank, 2 * self.rank);
        let _ = writeln!(
            out,
            "  a block holds at most {} nodes (NF-12); admission sizes a composite's closes within {} steps, each carriable in {} B",
            misaka_palw_tir::program::MAX_NODES_PER_BLOCK,
            self.work_cap,
            self.carriable
        );
        let _ = writeln!(
            out,
            "  sized at context {}, logits tile {}, checkpoint interval {}, tile {}, history chunk {}",
            self.context, self.logits_tile, self.checkpoint_interval, self.tile_len, self.h_chunk
        );
        for (i, (name, n, nodes)) in self.blocks.iter().enumerate() {
            let _ = writeln!(out, "  block b{i} `{name}` ×{n}: {nodes} nodes in the parent");
        }
        let cols: String = (0..self.blocks.len()).map(|i| format!("{:>6}", format!("b{i}"))).collect();
        let _ = writeln!(
            out,
            "  {:<16} {:>7} {:>11} {cols}  fits  admission work (of the cap)  largest close",
            "targets", "params", "bytes"
        );
        let parent_cols: String = self.blocks.iter().map(|(_, _, n)| format!("{n:>6}")).collect();
        let _ = writeln!(
            out,
            "  {:<16} {:>7} {:>11} {parent_cols}  {:<4}  {}",
            "(the parent)",
            "-",
            "-",
            "",
            self.sized_text(&self.parent)
        );
        for r in &self.rows {
            if let Some(why) = &r.refused {
                let _ = writeln!(out, "  {:<16} REFUSED: {why}", r.name);
                continue;
            }
            let cols: String = r.block_nodes.iter().map(|n| format!("{n:>6}")).collect();
            let _ = writeln!(
                out,
                "  {:<16} {:>7} {:>11} {cols}  {:<4}  {}",
                r.name,
                r.adapter_params,
                r.adapter_bytes,
                if r.fits { "yes" } else { "NO" },
                self.sized_text(&r.sized)
            );
        }
        for r in self.rows.iter().filter(|r| r.targets.len() > 1 || r.targets.first() != Some(&r.name)) {
            let _ = writeln!(out, "  {} = {}", r.name, if r.targets.is_empty() { "(none)".to_string() } else { r.targets.join(", ") });
        }
        out
    }

    /// The report as JSON.
    pub fn to_json(&self) -> serde_json::Value {
        let sized = |s: &Result<LoraSizedV1, String>| match s {
            Ok(s) => serde_json::json!({ "work": s.work, "of_cap": s.work as f64 / self.work_cap as f64,
                                         "largest_close": s.largest_close, "carried": s.carried.as_ref().err() }),
            Err(e) => serde_json::json!({ "not_sized": e }),
        };
        serde_json::json!({
            "architecture": self.architecture,
            "rank": self.rank,
            "context": self.context,
            "logits_tile": self.logits_tile,
            "checkpoint_interval": self.checkpoint_interval,
            "tile_len": self.tile_len,
            "h_chunk": self.h_chunk,
            "work_cap": self.work_cap,
            "carriable": self.carriable,
            "max_nodes_per_block": misaka_palw_tir::program::MAX_NODES_PER_BLOCK,
            "blocks": self.blocks.iter().map(|(name, n, nodes)| serde_json::json!({ "name": name, "occurrences": n, "parent_nodes": nodes })).collect::<Vec<_>>(),
            "parent": sized(&self.parent),
            "rows": self.rows.iter().map(|r| serde_json::json!({
                "name": r.name,
                "targets": r.targets,
                "refused": r.refused,
                "adapter_params": r.adapter_params,
                "adapter_bytes": r.adapter_bytes,
                "block_nodes": r.block_nodes,
                "fits": r.fits,
                "sized": sized(&r.sized),
                "admissible": r.admissible(self.work_cap),
            })).collect::<Vec<_>>(),
        })
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
        assert_eq!(dirs.len(), 93, "tir-lower's HF tiny fixtures (the 16 Qwen4-Exp ones included)");
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
        // An encoder-decoder is two programs (ENCDEC_FROM_SPEC_V1): the encoder over the padded source and the decoder's
        // text stage, each admitted on its own — it is no longer refused for being one.
        let t5 = check_ir_config_v1(&params, &read("tests/configs/encdec/t5-small.json"), false);
        assert!(!matches!(t5.verdict, ArchVerdictV1::NotLowerable(_)), "{}", t5.verdict);
        assert!(t5.unverified.iter().any(|u| u.starts_with("stage 0 (encoder")) && t5.unverified.iter().any(|u| u.starts_with("stage 1 (decoder")), "{:?}", t5.unverified);
        // Encoder-only models are not decoders.
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

    /// A one-layer program normalising a `[rows, n]` row of `i16` codes by the form `wide` names (the 39-node wide RMS the library
    /// keeps, which no fused kernel covers once its operands are not the kernel's) or by the 21-node unit row.
    fn norm_program(unit: bool, rows: u32, n: u32) -> TirProgramV1 {
        use misaka_palw_tir::builder::ProgramBuilder;
        use misaka_palw_tir::program::{HISTORY_BOUND_V1_SMALL, INPUT_TOKEN};
        use misaka_palw_tir::{DType, Ref, TensorType};
        let mut pb = ProgramBuilder::new(16, HISTORY_BOUND_V1_SMALL);
        let table = pb.param("x.table", DType::I16, &[16, rows * n], false);
        let eps = pb.param("eps", DType::I64, &[1], false);
        let pre = {
            let mut b = pb.block("pre", vec![]);
            let v = b.gather(table, Ref::Input(INPUT_TOKEN), 0, 0);
            b.finish(&[v])
        };
        let layer = {
            let mut b = pb.block("layer", vec![TensorType::fixed(DType::I16, &[rows * n])]);
            let x = b.reshape_fixed(Ref::CarryIn(0), &[rows, n]);
            // The unit row is a fused kernel's pattern; the same value spelled with an extra wide pass is not.
            let y = if unit {
                b.rms_unit_q24(x, eps)
            } else {
                let u = b.rms_unit_q24(x, eps);
                let wide = b.mul(u, u, DType::I128);
                let one = b.c(DType::I64, 1 << 20);
                let wide = b.div(wide, one, misaka_palw_tir::Rounding::Floor, DType::I128);
                b.clamp(wide, i32::MIN as i64, i32::MAX as i64, DType::I32)
            };
            let y = b.reshape_fixed(y, &[rows * n]);
            let y = b.clamp(y, -32768, 32767, DType::I16);
            let y = b.commit(y);
            b.finish(&[y])
        };
        let post = {
            let mut b = pb.block("post", vec![TensorType::fixed(DType::I16, &[rows * n])]);
            let l = b.reshape_fixed(Ref::CarryIn(0), &[rows * n]);
            b.commit(l);
            b.finish(&[])
        };
        pb.finish(pre, vec![layer], post, 0)
    }

    #[test]
    fn a_program_whose_wide_patterns_are_all_fused_is_admissible_and_one_with_unfused_wide_work_is_admissible_generic() {
        let (params, _, _) = network("testnet-11");
        let fused = check_ir_program_v1(&params, &norm_program(true, 8, 256));
        assert!(fused.admission_json["admitted"] == serde_json::json!(true), "{}", fused.verdict);
        assert!(matches!(fused.verdict, ArchVerdictV1::Admissible { .. }), "a fused wide pattern leaves nothing generic: {}", fused.verdict);
        let generic = check_ir_program_v1(&params, &norm_program(false, 8, 256));
        let ArchVerdictV1::AdmissibleGeneric { slowdown_permille, patterns } = &generic.verdict else {
            panic!("unfused wide work is ADMISSIBLE_GENERIC, got {}", generic.verdict)
        };
        assert!(*slowdown_permille > 1_020 && *slowdown_permille < 3_000, "{slowdown_permille}");
        assert!(patterns.iter().any(|p| p.starts_with("Mul") || p.starts_with("Div")), "{patterns:?}");
        assert!(generic.verdict.is_admissible(), "speed only: it registers");
        assert!(generic.verdict.to_string().starts_with("ADMISSIBLE_GENERIC (estimated slowdown 1."), "{}", generic.verdict);
    }

    #[test]
    fn a_remote_code_architecture_is_lowerable_unverified_and_keeps_the_programs_own_verdict() {
        let (params, _, _) = network("testnet-11");
        let text = std::fs::read_to_string(lower_dir("tools/corpus/specs/internlm2/config.json")).expect("a remote-code config");
        let r = check_ir_config_v1(&params, &text, false);
        let ArchVerdictV1::LowerableUnverified { inner } = &r.verdict else { panic!("remote code is LOWERABLE_UNVERIFIED, got {}", r.verdict) };
        assert!(inner.is_admissible(), "the program earned its own verdict: {inner}");
        assert!(r.verdict.is_admissible());
        assert!(r.verdict.to_string().starts_with("LOWERABLE_UNVERIFIED ("), "{}", r.verdict);
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
