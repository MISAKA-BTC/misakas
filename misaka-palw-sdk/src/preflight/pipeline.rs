//! **The register and mine stages of a pipeline class** (RFC-0003, `palw_gen_v1`): a model whose route is not the decoder pipeline's —
//! an encoder–decoder (T5, BART, Marian, …) — registers as a *generative* class, a pipeline of stage programs under a layout per
//! stage, and the chain judges it with the pipeline admission (`verify_gen_class_admission_v1`), not `tir_admit_v1`.
//!
//! Until 2026-10-08 the preflight lowered such a model to its stage programs, admitted each program alone and then said the class's
//! rules "are not judged here": register and mine came out `unknown`, and the command exited 0 on a model whose registration it had
//! not asked the chain about. This module asks:
//!
//! * the class is **declared shape-only** at a source and target length ([`ENCDEC_CONVENTIONS_V1`] records what a header cannot give:
//!   the artifact root, the tokenizer id, the source template);
//! * the first layout of a short fixed list that **the node's own gate** (`palw_gen_registration_preflight_at_v1`, under the rules at
//!   the judged height) admits is the class's layout; when none is admitted the first refusal is the blocker, with the chain's code;
//! * a class this build cannot declare shape-only (feature frames with no job binding, an encoder alone, a vision or diffusers
//!   route) is a blocker of its own, `PIPELINE_CLASS_UNDECLARED` — never `unknown`, never a pass;
//! * the seat's memory is judged the way an IR class's is (the weights the class holds, the widest stage's state, peak live bytes
//!   and tile) against the seat tiers.
//!
//! Nothing here invents a rule: every number is a consensus function's. It is opt-in (`Options::pipeline_admission`) because the
//! corpus preflight pins (`tests/golden/corpus_preflight_v1.json`, hashed by the RFC-0011 coverage audit) record the earlier
//! behaviour of `Options::default()`; the CLIs and the census set it.

use super::chain::{ChainOutput, FenceRow, GateNumbers, LayoutInfo, PreflightNetwork, SeatInfo, SeatTier, gate_blocker};
use super::model::RoutedClass;
use super::{Blocker, Condition, Options, Stage};
use crate::gen_class::{GenLayoutChoiceV1, gen_default_layouts_v1};
use kaspa_consensus_core::config::params::{ForkActivation, Params};
use kaspa_consensus_core::palw_gen_admission_v1::{palw_gen_post_genesis_registration_v1, palw_gen_registration_preflight_at_v1};
use kaspa_consensus_core::palw_gen_class_v1::{
    PALW_GEN_CLASS_VERSION_V1, PalwGenClassErrorV1, PalwGenClassV1, PalwGenOffersV1, PalwGenProfileOffersV1,
    palw_gen_class_preflight_v1,
};
use kaspa_consensus_core::palw_gen_v1::{PalwGenFenceV1, PalwGenProfileV1};
use kaspa_hashes::Hash64;
use misaka_palw_gen::OutputSpecV1;
use misaka_palw_tir::pipeline::TirPipelineV1;
use misaka_palw_tir::program_v2::TirProgramV2;
use misaka_palw_tir_lower::model::route::{EncDecStagesV1, encdec_pipeline_v1};
use serde::Serialize;

/// What a header cannot give a shape-only class, as the preflight fixes it (recorded in every report that judges a pipeline class).
pub const ENCDEC_CONVENTIONS_V1: &[&str] = &[
    "the artifact root is a placeholder (the inventory needs the weights); the tokenizer id is a placeholder",
    "the source is templated `ids ‖ </s>` (the configuration's eos id when it has one) and padded to the source length; the class offers sources of at most source length less the template",
    "the decoder stream starts with the configuration's decoder start id, forced by the class; up to 8 prompt ids are offered",
    "the source and target lengths are the declared context (`--max-context`, else the model's declared positions, at most 1,024)",
    "the layout is the first of a short list the pipeline admission admits (commit tile 64/32/16/256 lanes, logits tile 256/1,024, history tile 32/16/8, widest checkpoint interval)",
];

/// The shape-only conventions the census row keeps beside the verdict.
pub const PIPELINE_PROMPT_IDS_V1: u32 = 8;

/// One stage of the declared class.
#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct PipelineStageInfo {
    pub name: String,
    pub max_trip: u32,
    pub nodes: usize,
    pub commit_tile: u32,
    pub h_tile: u32,
    pub checkpoint_interval: u32,
}

/// The pipeline class a report judged.
#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct PipelineInfo {
    /// The route's kind (`encdec`).
    pub kind: String,
    /// `Text` (the RFC-0003 profile).
    pub profile: String,
    pub adapter: String,
    pub source_len: u32,
    pub target_len: u32,
    pub stages: Vec<PipelineStageInfo>,
    /// The source's price floor in prompt tokens that admission derived (`⌈encoder work / per-token work⌉`).
    pub source_token_floor: u32,
    /// The pipeline admission admitted the class under the layout below; else the layout is the first one tried (the refusal's).
    pub admitted: bool,
    pub conventions: Vec<String>,
}

/// Layouts tried in order: (commit tile, logits tile, history tile, widest checkpoint interval).
const LAYOUTS: [(u32, Option<u32>, u32, u32); 7] = [
    (64, Some(256), 32, 64),
    (32, Some(256), 32, 64),
    (16, Some(256), 32, 64),
    (256, Some(256), 32, 64),
    (64, Some(1024), 32, 8),
    (64, Some(256), 16, 8),
    (64, Some(256), 8, 1),
];

/// The bytes of the weights a class holds (every program's artifact params, per layer).
fn weights_bytes(programs: &[TirProgramV2]) -> u64 {
    programs
        .iter()
        .enumerate()
        .map(|(k, p)| {
            let view = kaspa_consensus_core::palw_gen_artifact_v1::palw_gen_param_view_v1(k as u16, p);
            kaspa_consensus_core::palw_tir_work_v1::palw_tir_work_shape_v1(&view)
                .map(|s| s.param_bytes().min(u64::MAX as u128) as u64)
                .unwrap_or(0)
        })
        .fold(0u64, |a, b| a.saturating_add(b))
}

/// The class of an encoder–decoder under one layout, with `floor` as the source's price floor.
fn encdec_class(
    st: &EncDecStagesV1,
    pipeline: &TirPipelineV1,
    programs: &[TirProgramV2],
    suffix_len: u32,
    choice: &GenLayoutChoiceV1,
    floor: u32,
) -> PalwGenClassV1 {
    PalwGenClassV1 {
        version: PALW_GEN_CLASS_VERSION_V1,
        profile: PalwGenProfileV1::Text as u8,
        pipeline: pipeline.encode(),
        programs: programs.iter().map(|p| p.encode()).collect(),
        layouts: gen_default_layouts_v1(pipeline, programs, choice, choice.checkpoint_interval),
        output: OutputSpecV1::tokens(st.target_len),
        offers: PalwGenOffersV1 {
            steps: vec![],
            scalars: vec![],
            max_prompt_tokens: PIPELINE_PROMPT_IDS_V1.min(st.target_len),
            max_negative_tokens: 0,
            images: vec![],
            max_source_tokens: st.source_len.saturating_sub(suffix_len),
            forced_prompt_prefix: vec![st.decoder_start],
            source_token_floor: floor,
            profile: PalwGenProfileOffersV1::None,
        },
        tokenizer_id: Hash64::from_bytes([0; 64]),
    }
}

/// The seat of a pipeline class: the weights it holds plus the widest stage's state, peak live bytes and tile a close opens, against
/// the seat tiers. A blocker when no tier holds it; a note when only some do.
fn seat_of(
    programs: &[TirProgramV2],
    state: u64,
    peak: u64,
    tile: u64,
    opts: &Options,
) -> (SeatInfo, Option<Blocker>, Option<String>) {
    let artifact_bytes = weights_bytes(programs);
    let needed = artifact_bytes.saturating_add(state).saturating_add(peak).saturating_add(tile);
    let (tiers_in, tiers_source) = if opts.seat_shares.is_empty() {
        (super::chain::default_seat_shares(), "the testnet-12 fleet's seat tiers (--seat-share replaces them)".to_string())
    } else {
        (opts.seat_shares.clone(), "given (--seat-share)".to_string())
    };
    let tiers: Vec<SeatTier> = tiers_in
        .iter()
        .map(|t| SeatTier { name: t.name.clone(), share_bytes: t.bytes, fits: needed <= t.bytes, fits_at_context: None })
        .collect();
    let mut blocker = None;
    let mut note = None;
    if !tiers.iter().any(|t| t.fits) {
        let biggest = tiers.iter().map(|t| t.share_bytes).max().unwrap_or(0);
        blocker = Some(
            Blocker::new(
                Stage::Mine,
                "SEAT_MEMORY_SHORT",
                "no seat tier holds the class: a seat never becomes ready for it, because it cannot replay a claim",
            )
            .numbers(needed, biggest, "bytes")
            .evidence(tiers.iter().map(|t| format!("{}: share {}", t.name, super::render::size(t.share_bytes))))
            .safe(["a smaller model or context, or a larger seat".to_string()]),
        );
    } else if tiers.iter().any(|t| !t.fits) {
        note = Some(format!(
            "only {} can hold the class; the other seats never become ready for it (RFC-0002 §II.7.3 F4)",
            tiers.iter().filter(|t| t.fits).map(|t| t.name.as_str()).collect::<Vec<_>>().join(", ")
        ));
    }
    let info = SeatInfo {
        artifact_bytes,
        state_bytes: state,
        peak_live_bytes: peak,
        widest_tile_opened_bytes: tile,
        needed_bytes: needed,
        tiers_source,
        tiers,
        note: "an estimate: the weights mapped, the widest stage's state at the declared context, its peak live bytes and the widest tile a close opens".into(),
    };
    (info, blocker, note)
}

/// A structural refusal of the class (`palw_gen_class_preflight_v1`), by its typed name: a ceiling is a resource blocker with the
/// numbers, a malformed declaration (offers, layouts, output) is the declaring tool's. The chain's code for all of them is
/// `GEN_CLASS_REFUSED` (`PalwClassAdmissionError::GenClass`).
fn class_error_blocker(e: &PalwGenClassErrorV1) -> (Blocker, String) {
    let on_chain = "GEN_CLASS_REFUSED";
    let b = match e {
        PalwGenClassErrorV1::AdmissionExceeds { limit, at, value, cap } => Blocker::new(
            Stage::Register,
            if limit.contains("close") { "CLOSE_SIZE_OVER_CAP" } else { "ADMISSION_EXCEEDS" },
            format!("{limit} at {at}: {value} against a cap of {cap}"),
        )
        .arg(*limit)
        .numbers(*value, *cap, "units"),
        PalwGenClassErrorV1::Exceeds { what, value, cap } => {
            Blocker::new(Stage::Register, "ADMISSION_EXCEEDS", format!("{what}: {value} against the profile's ceiling {cap}"))
                .arg(*what)
                .numbers(*value, *cap, "units")
        }
        other => Blocker::new(Stage::Register, "ADMISSION_REFUSED", format!("the pipeline admission refuses the class: {other}"))
            .arg(on_chain),
    };
    (b.evidence([format!("on-chain code {on_chain}")]), on_chain.to_string())
}

fn bond_key() -> kaspa_consensus_core::palw_state_v2::PalwBondKeyV2 {
    kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
        kaspa_consensus_core::tx::TransactionId::from_bytes([0; 64]),
        0,
    ))
}

/// The ruleset a pipeline class is judged under: the network's own where `palw_gen_v1` is scheduled, else the same ruleset with the
/// fence armed hypothetically (testnet-12's provisional ceilings) at the IR flag day's height, said so by the caller.
fn gen_params(net: &PreflightNetwork, height: u64) -> Result<(Params, Option<String>), String> {
    if net.params.palw_gen_v1_fence().is_some() {
        return Ok((net.params.clone(), None));
    }
    let mut armed = net.params.clone();
    let at = net.params.palw_tir_v1_fence().map(|f| f.activation.daa_score()).unwrap_or(1).max(1).min(height.max(1));
    armed.palw_gen_v1 = Some(PalwGenFenceV1::testnet12_v1(ForkActivation::new(at)));
    armed.sync_palw_gen_v1();
    armed.validate_palw_v2().map_err(|e| format!("palw_gen_v1 cannot be armed hypothetically at DAA {at}: {e}"))?;
    Ok((
        armed,
        Some(format!(
            "palw_gen_v1 is not scheduled on this network: the class is judged as if it were armed at DAA {at} (testnet-12's provisional ceilings)"
        )),
    ))
}

/// Judge a routed class on a network. The chain output has the same shape as an IR class's: the register and mine stages, the
/// admission numbers, the seat, the fences.
pub fn judge_pipeline(net: &PreflightNetwork, opts: &Options, routed: &RoutedClass) -> ChainOutput {
    let out = ChainOutput::default();
    let params = &net.params;
    let bundle = &net.bundle;

    // ---- the height (the IR judgment's rule) ---------------------------------------------------------------------------------------
    let schedule_end = params.fence_schedule_v1().last().copied().unwrap_or(0);
    let (height, daa_choice) = match (opts.height, opts.node.as_ref()) {
        (Some(h), _) => (h, "given (--height)".to_string()),
        (None, Some(node)) => (node.tip_daa, format!("the node's tip (--node, {})", node.network)),
        (None, None) => (
            schedule_end,
            if schedule_end == 0 {
                "the network schedules no fence: DAA 0".to_string()
            } else {
                format!("the first height at which every fence this network schedules is in force (the last is DAA {schedule_end})")
            },
        ),
    };
    let gen_activation = params.palw_gen_v1_fence().map(|f| f.activation.daa_score());
    let gen_armed = gen_activation.is_some_and(|a| a <= height);
    let judge_daa = height.max(gen_activation.unwrap_or(1)).max(1);
    let tir_armed = params.palw_tir_v1_fence().is_some_and(|f| f.activation.daa_score() <= height);
    let mut what_if = match gen_activation {
        Some(a) if a > height => Some(format!("palw_gen_v1 activates at DAA {a}: the conditions past it are judged at DAA {a}")),
        _ => None,
    };

    let mut register: Vec<Blocker> = Vec::new();
    let mut mine: Vec<Blocker> = Vec::new();
    let mut conditions: Vec<Condition> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let mut admission = super::chain::AdmissionInfo {
        verdict: "pipeline class".into(),
        ceilings: "palw_gen_v1's Text-profile ceilings (the network's fence)".into(),
        program_bytes: 0,
        blocks: 0,
        nodes: 0,
        unrolled_nodes: 0,
        graph_ir_root: None,
        layout: None,
        gate: "not asked".into(),
        gate_detail: None,
        numbers: None,
        not_asked: vec![
            "the class id commits to the artifact root and the tokenizer id, which need the artifact (placeholders are used)".into(),
            "the registrant bond's signature and collateral, and the chain's certified families".into(),
        ],
    };
    let mut seat = None;
    let mut pipeline_info = None;

    match routed {
        RoutedClass::Undeclarable { kind, adapter, why } => {
            register.push(
                Blocker::new(
                    Stage::Register,
                    "PIPELINE_CLASS_UNDECLARED",
                    format!("a {kind} class (adapter {adapter}) cannot be declared and judged shape-only by this build: {why}"),
                )
                .arg(kind.clone())
                .evidence([why.clone()])
                .safe(["the pipeline admission is asked of a declared class; build the class from the checkpoint's weights (`pack`) and preflight at the `full` depth".to_string()]),
            );
            admission.gate = "not asked".into();
            admission.gate_detail = Some(why.clone());
        }
        RoutedClass::EncDec(st) => {
            let (armed_params, hypo) = match gen_params(net, height) {
                Ok(x) => x,
                Err(e) => {
                    register.push(Blocker::new(Stage::Register, "FENCE_NOT_ARMED", e).arg("palw_gen_v1"));
                    return finish(
                        out, net, daa_choice, judge_daa, tir_armed, what_if, admission, conditions, register, mine, None, notes, None,
                    );
                }
            };
            if let Some(h) = hypo {
                what_if = Some(h);
            }
            if !gen_armed {
                register.push(
                    Blocker::new(
                        Stage::Register,
                        "FENCE_NOT_ARMED",
                        "palw_gen_v1 is not in force at this height: no pipeline class can be registered on the network yet",
                    )
                    .arg("palw_gen_v1")
                    .evidence([match gen_activation {
                        Some(a) => format!("scheduled at DAA {a}; the height judged is {height}"),
                        None => "not scheduled on this network".to_string(),
                    }])
                    .safe([
                        "the registration waits for the flag day that arms it; everything else below is judged as if it were armed"
                            .to_string(),
                    ]),
                );
            }
            let suffix: Vec<u32> = st.eos.into_iter().collect();
            let pipeline = encdec_pipeline_v1(st.source_len, st.target_len, st.pad, suffix.clone());
            let programs = [st.encoder.clone(), st.decoder.clone()];
            admission.program_bytes = programs.iter().map(|p| p.encode().len()).sum();
            admission.blocks = programs.iter().map(|p| p.blocks.len()).sum();
            admission.nodes = programs.iter().map(|p| p.blocks.iter().map(|b| b.nodes.len()).sum::<usize>()).sum();
            let fence = armed_params.palw_gen_v1_fence().expect("armed above");
            let artifact_root = Hash64::from_bytes([0; 64]);
            // The first refusal (in the order the layouts are tried), as the blocker it becomes, the chain's code and its text.
            let mut first: Option<(Blocker, String, String)> = None;
            let mut admitted = None;
            let mut first_class: Option<PalwGenClassV1> = None;
            let mut admitted_class: Option<PalwGenClassV1> = None;
            // `PALW_PIPELINE_LAYOUTS=tile:logits:history:checkpoint,…` replaces the list (a diagnosis; never set by a tool).
            let layouts: Vec<(u32, Option<u32>, u32, u32)> = match std::env::var("PALW_PIPELINE_LAYOUTS") {
                Ok(v) => v
                    .split(',')
                    .filter_map(|x| {
                        let p: Vec<u32> = x.split(':').filter_map(|n| n.parse().ok()).collect();
                        (p.len() == 4).then(|| (p[0], Some(p[1]), p[2], p[3]))
                    })
                    .collect(),
                Err(_) => LAYOUTS.to_vec(),
            };
            for (tile_len, output_tile, h_chunk, checkpoint) in layouts {
                let choice = GenLayoutChoiceV1 { tile_len, output_tile, h_chunk, checkpoint_interval: checkpoint };
                let tag = format!("layout ({tile_len},{output_tile:?},{h_chunk},{checkpoint})");
                let mut refuse = |blocker: Blocker, code: String, text: String| {
                    if std::env::var_os("PALW_PIPELINE_TRACE").is_some() {
                        eprintln!("pipeline-trace: {tag}: {text}");
                    }
                    if first.is_none() {
                        first = Some((blocker, code, format!("{tag}: {text}")));
                    }
                };
                // The source's price floor is admission's (`⌈encoder work / per-token work⌉`), a function of the layout: ask the
                // structure with a floor no honest class declares to read it back — and to meet a structural refusal by its own
                // typed name — then declare it.
                let probe = encdec_class(st, &pipeline, &programs, suffix.len() as u32, &choice, u32::MAX);
                let floor = match palw_gen_class_preflight_v1(&probe, &fence) {
                    Ok(r) => r.source_token_floor.max(1),
                    Err(e) => {
                        let (b, code) = class_error_blocker(&e);
                        refuse(b, code, e.to_string());
                        if first_class.is_none() {
                            first_class = Some(probe);
                        }
                        continue;
                    }
                };
                let class = encdec_class(st, &pipeline, &programs, suffix.len() as u32, &choice, floor);
                if first_class.is_none() {
                    first_class = Some(class.clone());
                }
                let object = match palw_gen_post_genesis_registration_v1(
                    class.clone(),
                    artifact_root,
                    0,
                    1 << 100,
                    1,
                    judge_daa,
                    bond_key(),
                    Vec::new(),
                ) {
                    Ok(o) => o,
                    Err(e) => {
                        refuse(gate_blocker(&e, &[]), e.code().to_string(), e.to_string());
                        continue;
                    }
                };
                match palw_gen_registration_preflight_at_v1(&armed_params, bundle, &object, judge_daa) {
                    Ok(a) => {
                        admitted = Some((class, choice, a));
                        break;
                    }
                    Err(e) => refuse(gate_blocker(&e, &[]), e.code().to_string(), e.to_string()),
                }
            }
            let carriable = kaspa_consensus_core::palw_tir_admission_v1::palw_tir_carriable_close_bytes_v1(&bundle.court);
            match admitted {
                Some((class, choice, a)) => {
                    admitted_class = Some(class.clone());
                    admission.gate = "admitted".into();
                    admission.verdict = format!("pipeline class admitted: {} stages, profile Text", class.layouts.len());
                    let entry = &a.entry;
                    let g = GateNumbers {
                        max_step_leaf_count: entry.max_step_leaf_count,
                        canonical_step_leaf_count: entry.canonical_step_leaf_count,
                        max_close_bytes: entry.court_cost.max_close_bytes,
                        max_terminal_macs: entry.court_cost.max_terminal_macs,
                        max_operand_count: u64::from(entry.court_cost.max_operand_count),
                    };
                    let le = |n: u64, l: u64| Some(n <= l);
                    let c =
                        |id: &str, what: &str, needed: Option<u64>, limit: Option<u64>, unit: &str, ok: Option<bool>, source: &str| {
                            Condition { id: id.into(), what: what.into(), needed, limit, unit: unit.into(), ok, source: source.into() }
                        };
                    conditions.push(c(
                        "close_bytes",
                        "the worst terminal close of any commit point of any stage",
                        Some(g.max_close_bytes),
                        Some(bundle.court.max_close_bytes()),
                        "bytes",
                        le(g.max_close_bytes, bundle.court.max_close_bytes()),
                        "the pipeline admission (PALW-GEN-21); carriable in at most the bytes of the carrier",
                    ));
                    conditions.push(c(
                        "carriable_close_bytes",
                        "every terminal close as carried fits the chunks the fold assembles",
                        None,
                        Some(carriable),
                        "bytes",
                        Some(true),
                        "palw_tir_carriable_close_bytes_v1",
                    ));
                    conditions.push(c(
                        "da_ladder",
                        "step leaves of the class's longest job against the ladder in force",
                        Some(g.max_step_leaf_count),
                        Some(bundle.court.max_step_leaf_count()),
                        "step leaves",
                        le(g.max_step_leaf_count, bundle.court.max_step_leaf_count()),
                        "the court's max_step_leaf_count",
                    ));
                    let out_stage = class.layouts.len() - 1;
                    admission.layout = Some(LayoutInfo {
                        max_context: st.target_len,
                        checkpoint_interval: class.layouts[out_stage].checkpoint_interval,
                        h_tile: class.layouts[out_stage].h_tile,
                        commit_tiles: class.layouts.iter().map(|l| l.commit_tiles.len()).sum(),
                        logits_tile: choice.output_tile,
                        searched: false,
                        widest_context: st.target_len,
                    });
                    admission.numbers = Some(g);
                    // ---- the seat: the weights, and the widest stage's state, peak live bytes and tile ----------------------------------
                    let (mut state, mut peak, mut tile) = (0u64, 0u64, 0u64);
                    for s in &a.report.admission.stages {
                        state = state.max(s.admission.view.position.state_bytes);
                        peak = peak.max(s.admission.view.position.peak_live_bytes);
                        tile = tile.max(s.admission.view.cones.iter().map(|c| c.tile_opened_bytes).max().unwrap_or(0));
                    }
                    let (info, blocker, note) = seat_of(&programs, state, peak, tile, opts);
                    mine.extend(blocker);
                    notes.extend(note);
                    seat = Some(info);
                }
                None => {
                    let (mut b, code, text) = first.expect("a layout was tried");
                    admission.gate = code;
                    admission.gate_detail = Some(text);
                    // `palw_gen_v1` not in force is already its own blocker above; a gate refusal that is only the fence is not repeated.
                    if !(b.code == "FENCE_NOT_ARMED" && register.iter().any(|x| x.code == "FENCE_NOT_ARMED")) {
                        b.evidence.push(format!("the first layout of {} tried; its refusal is the widest tile's", LAYOUTS.len()));
                        register.push(b);
                    }
                    // The seat does not wait for the registration: the weights and the stages' own working set (each program admitted
                    // alone at the default tile) say which seat tiers could hold the class were it admitted.
                    let (mut state, mut peak, mut tile, mut sized) = (0u64, 0u64, 0u64, true);
                    for p in &programs {
                        match misaka_palw_tir::admit_v2::tir_admit_program_v2(p, &misaka_palw_tir_lower::admission::default_inputs()) {
                            Ok(a) => {
                                state = state.max(a.view.position.state_bytes);
                                peak = peak.max(a.view.position.peak_live_bytes);
                                tile = tile.max(a.view.cones.iter().map(|c| c.tile_opened_bytes).max().unwrap_or(0));
                            }
                            Err(_) => sized = false,
                        }
                    }
                    if sized {
                        let (info, blocker, note) = seat_of(&programs, state, peak, tile, opts);
                        mine.extend(blocker);
                        notes.extend(note);
                        notes.push("the seat is sized from the stages admitted alone (the class was not admitted)".to_string());
                        seat = Some(info);
                    }
                }
            }
            // The class report: the admitted layout, or the first one tried when none was admitted.
            let shown = admitted_class.as_ref().or(first_class.as_ref());
            if let Some(class) = shown {
                pipeline_info = Some(PipelineInfo {
                    kind: "encdec".into(),
                    profile: "Text".into(),
                    adapter: st.adapter.clone(),
                    source_len: st.source_len,
                    target_len: st.target_len,
                    stages: pipeline
                        .stages
                        .iter()
                        .zip(&class.layouts)
                        .zip(&programs)
                        .map(|((s, l), p)| PipelineStageInfo {
                            name: s.name.clone(),
                            max_trip: s.max_trip,
                            nodes: p.blocks.iter().map(|b| b.nodes.len()).sum(),
                            commit_tile: l.commit_tiles.iter().copied().max().unwrap_or(0),
                            h_tile: l.h_tile,
                            checkpoint_interval: l.checkpoint_interval,
                        })
                        .collect(),
                    // `u32::MAX` is the floor of a probe a structural refusal stopped: admission never derived one
                    source_token_floor: if class.offers.source_token_floor == u32::MAX { 0 } else { class.offers.source_token_floor },
                    admitted: admitted_class.is_some(),
                    conventions: ENCDEC_CONVENTIONS_V1.iter().map(|s| s.to_string()).collect(),
                });
            }
        }
    }

    finish(out, net, daa_choice, judge_daa, tir_armed, what_if, admission, conditions, register, mine, seat, notes, pipeline_info)
}

/// The chain output from the parts: the fences (the generative one is the one this class needs), the conditions over them.
#[allow(clippy::too_many_arguments)]
fn finish(
    mut out: ChainOutput,
    net: &PreflightNetwork,
    daa_choice: String,
    judge_daa: u64,
    tir_armed: bool,
    what_if: Option<String>,
    admission: super::chain::AdmissionInfo,
    mut conditions: Vec<Condition>,
    register: Vec<Blocker>,
    mine: Vec<Blocker>,
    seat: Option<SeatInfo>,
    notes: Vec<String>,
    pipeline: Option<PipelineInfo>,
) -> ChainOutput {
    let fences: Vec<FenceRow> = net
        .params
        .palw_fences_v1()
        .into_iter()
        .map(|(name, act)| {
            let a = act.map(|f| f.daa_score()).filter(|s| *s != u64::MAX);
            let in_force = a.is_some_and(|a| a <= judge_daa);
            FenceRow { name: name.to_string(), activation: a, in_force, needed: name == "palw_gen_v1" }
        })
        .collect();
    for f in fences.iter().filter(|f| f.needed) {
        conditions.push(Condition {
            id: format!("fence:{}", f.name),
            what: format!("{} is in force", f.name),
            needed: f.activation,
            limit: Some(judge_daa),
            unit: "DAA".into(),
            ok: Some(f.in_force),
            source: "Params::palw_fences_v1 (the network's own schedule)".into(),
        });
    }
    out.network = super::chain::NetworkInfo { id: net.id.clone(), daa: judge_daa, daa_choice, tir_armed, what_if, fences };
    out.admission = Some(admission);
    out.conditions = conditions;
    out.register = register;
    out.mine = mine;
    out.notes = notes;
    out.mine_not_judged = (seat.is_none())
        .then(|| "the class was not declared and admitted: there is no seat to size (see the register stage)".to_string());
    out.seat = seat;
    out.pipeline = pipeline;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A ceiling the pipeline admission names is a resource blocker with its numbers, not a declaration the tool got wrong.** (A BART
    /// encoder is one position of 206 G MACs against 2^37: `ADMISSION_EXCEEDS(max_position_macs)`, never `ADMISSION_REFUSED`.)
    #[test]
    fn a_ceiling_is_a_resource_blocker_and_a_malformed_declaration_is_the_tools() {
        let (b, code) = class_error_blocker(&PalwGenClassErrorV1::AdmissionExceeds {
            limit: "max_position_macs",
            at: "stage 0 (encoder): the position".into(),
            value: 206_158_430_208,
            cap: 137_438_953_472,
        });
        assert_eq!(
            (b.code.as_str(), b.arg.as_deref(), b.have, b.need),
            ("ADMISSION_EXCEEDS", Some("max_position_macs"), Some(206_158_430_208), Some(137_438_953_472))
        );
        assert_eq!(code, "GEN_CLASS_REFUSED");
        assert!(b.evidence.iter().any(|e| e == "on-chain code GEN_CLASS_REFUSED"));
        let (b, _) = class_error_blocker(&PalwGenClassErrorV1::AdmissionExceeds {
            limit: "generative close bytes as carried",
            at: "x".into(),
            value: 2,
            cap: 1,
        });
        assert_eq!(b.code, "CLOSE_SIZE_OVER_CAP");
        let (b, _) = class_error_blocker(&PalwGenClassErrorV1::Exceeds { what: "stages", value: 9, cap: 4 });
        assert_eq!((b.code.as_str(), b.arg.as_deref()), ("ADMISSION_EXCEEDS", Some("stages")));
        let (b, _) = class_error_blocker(&PalwGenClassErrorV1::Offers("a rule reads the source and no source is offered".into()));
        assert_eq!((b.code.as_str(), b.arg.as_deref()), ("ADMISSION_REFUSED", Some("GEN_CLASS_REFUSED")));
        assert!(b.have.is_none(), "a malformed declaration has no numbers");
    }
}
