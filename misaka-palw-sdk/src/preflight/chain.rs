//! **The register and mine stages of a preflight** (RFC-0002 Part II §II.2.3 items 6–9): the chain's own
//! functions, asked of the shape-only program at a height. This module has no opinion of its own — each
//! number is a consensus function's — it names the conditions, computes each as *needed against limit*, and
//! maps a refusal to a stable code.
//!
//! * admission: `tir_admit_v1` over the program (the same call `check-architecture` makes);
//! * the registration gate: admission v10 on the class under the layout the SDK would declare (typed: the
//!   refusal is a [`PalwClassAdmissionError`], not a string), at the height, under the rules in force there;
//! * the court window: `palw_attn_court_admits_row_v1` at the declared context — computed on its own, so a
//!   model that fails an earlier wall still shows the window it would meet next;
//! * the canonical job, the fences, the registry's derived profile (`palw_derive_profile_v1`, the one the
//!   fold writes) for the forecast.

use super::model::Analysis;
use super::source::Source;
use super::{Blocker, Condition, Options, Stage, StageVerdict};
use kaspa_consensus_core::config::params::{ForkActivation, Params};
use kaspa_consensus_core::network::NetworkId;
use kaspa_consensus_core::palw_class_admission_v2::PalwClassAdmissionError;
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_tir_admission_v1::{
    PalwTirAdmissionRulesV1, palw_tir_carriable_close_bytes_v1, palw_tir_post_genesis_registration_v1,
    palw_tir_registration_preflight_at_v1,
};
use kaspa_consensus_core::palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_attempt_canonical_of_v1, palw_tir_job_context_v1};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1, PalwTirLayoutV1};
use kaspa_hashes::Hash64;
use misaka_palw_tir::TirProgramV1;
use serde::Serialize;

use crate::check_architecture::{ArchVerdictV1, check_ir_program_at_v1, tir_admit_inputs_v1, tir_ceilings_v1};
use crate::tir_layout::{TirLayoutChoiceV1, tir_choose_layout_v1};

/// One tier of seat: a name and the memory share a seat of that tier declares.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SeatShare {
    pub name: String,
    pub bytes: u64,
}

/// The seat tiers of the testnet-12 fleet (coordinator, 2026-10-01): the 5.104 seats hold a memory share of 3.5 GiB
/// (MemoryMax 9 GiB) and the ibm/.113 seats 8 GiB (MemoryMax 16-20 GiB). `--seat-share` replaces them.
pub fn default_seat_shares() -> Vec<SeatShare> {
    vec![
        SeatShare { name: "5.104 seats (MemoryMax 9 GiB)".into(), bytes: 3_758_096_384 },
        SeatShare { name: "ibm/.113 seats (MemoryMax 16-20 GiB)".into(), bytes: 8 << 30 },
    ]
}

/// `[name=]GiB` (a decimal GiB) as a tier.
pub fn parse_seat_share(s: &str) -> Result<SeatShare, String> {
    let (name, gib) = match s.split_once('=') {
        Some((n, g)) => (n.to_string(), g),
        None => (format!("{s} GiB"), s),
    };
    let g: f64 = gib.parse().map_err(|e| format!("--seat-share {s}: {e}"))?;
    if g.is_nan() || g < 0.0 || !g.is_finite() {
        return Err(format!("--seat-share {s}: a share in GiB"));
    }
    Ok(SeatShare { name, bytes: (g * (1u64 << 30) as f64) as u64 })
}

/// A network the preflight judges on.
pub struct PreflightNetwork {
    pub id: String,
    pub network_id: NetworkId,
    pub params: Params,
    pub bundle: PalwConsensusParamsV2,
}

impl PreflightNetwork {
    pub fn parse(raw: &str) -> Result<PreflightNetwork, String> {
        let network_id: NetworkId = raw.parse().map_err(|e| format!("--network {raw}: {e}"))?;
        let params: Params = network_id.into();
        let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
            return Err(format!("{network_id} has no PALW V2 bundle, so it has no classes to speak of"));
        };
        let bundle = bundle.clone();
        Ok(PreflightNetwork { id: network_id.to_string(), network_id, params, bundle })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct FenceRow {
    pub name: String,
    /// `None`: dormant on this network.
    pub activation: Option<u64>,
    pub in_force: bool,
    /// The class needs it to register.
    pub needed: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct NetworkInfo {
    pub id: String,
    /// The height the conditions were judged at.
    pub daa: u64,
    /// How it was chosen.
    pub daa_choice: String,
    /// `palw_tir_v1` is in force on the network at that height.
    pub tir_armed: bool,
    /// Said when the conditions past the fence were judged as if it were armed.
    pub what_if: Option<String>,
    pub fences: Vec<FenceRow>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LayoutInfo {
    pub max_context: u32,
    pub checkpoint_interval: u32,
    pub h_tile: u32,
    pub commit_tiles: usize,
    pub logits_tile: Option<u32>,
    /// The context was searched for (the widest the gate admits), not given.
    pub searched: bool,
    /// The widest context the program and the network's ceilings admit.
    pub widest_context: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct GateNumbers {
    pub max_step_leaf_count: u64,
    pub canonical_step_leaf_count: u64,
    pub max_close_bytes: u64,
    pub max_terminal_macs: u64,
    pub max_operand_count: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct AdmissionInfo {
    /// `tir_admit_v1`'s verdict over the shape-only program (`ADMISSIBLE`, `EXCEEDS(...)`, ...).
    pub verdict: String,
    pub ceilings: String,
    pub program_bytes: usize,
    pub blocks: usize,
    pub nodes: usize,
    pub unrolled_nodes: u64,
    pub graph_ir_root: Option<String>,
    pub layout: Option<LayoutInfo>,
    /// Admission v10 on the class at the height: `admitted`, or the refusal's code.
    pub gate: String,
    pub gate_detail: Option<String>,
    pub numbers: Option<GateNumbers>,
    /// What the gate asks that a preflight cannot: the artifact root (the class id commits to it), the registrant's
    /// bond, the chain's certified families.
    pub not_asked: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SeatInfo {
    pub artifact_bytes: u64,
    /// The K/V history and recurrent state at the declared context.
    pub state_bytes: u64,
    pub peak_live_bytes: u64,
    pub widest_tile_opened_bytes: u64,
    pub needed_bytes: u64,
    /// Where the tiers came from.
    pub tiers_source: String,
    pub tiers: Vec<SeatTier>,
    pub note: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct SeatTier {
    pub name: String,
    pub share_bytes: u64,
    pub fits: bool,
    /// The widest context at which the class fits this tier, when it does not at the declared one.
    pub fits_at_context: Option<u32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Forecast {
    pub verification_window_spans: u32,
    pub artifact_prefetch_spans: u32,
    pub max_inflight_claims: u32,
    pub required_ready_seats: u32,
    pub registration_bond_sompi: u64,
    pub admission_claims_per_span_milli: u64,
    pub probation_claims: u32,
    pub stable_spans: u32,
    pub audit_period_daa: u64,
    pub span_ms: u64,
    pub path: Vec<String>,
    pub note: String,
    /// **How many independent operators the network has against the seating floor** (RFC-0002 §II.7.5): present when a node was
    /// asked (`--node`) or the network arms `palw_class_seating`; absent otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub independence: Option<ForecastIndependence>,
}

/// The seating floor against the operators the network has.
#[derive(Clone, Debug, Serialize)]
pub struct ForecastIndependence {
    /// `palw_class_seating` is in force on the network at the judged height (its floor is then asked of every claim).
    pub fence_in_force: bool,
    /// The floor in force at the height, or the default a flag day would arm it with (the jury's strict majority of a panel).
    pub independent_floor: u32,
    /// The panel's size: the possession floor (`seat_count` distinct operators besides the executor).
    pub seat_count: u32,
    /// Operators of the network's base population the node reports (the outsider seat's draw), when a node was asked.
    pub base_operators: Option<u32>,
    /// The share of a class's claims whose outsider would hold it once exactly the floor of operators do, `floor × 1000 / base`.
    pub licensable_share_at_floor_permille: Option<u16>,
    pub note: String,
}

#[derive(Default, Clone)]
pub struct ChainOutput {
    pub network: NetworkInfo,
    pub admission: Option<AdmissionInfo>,
    pub conditions: Vec<Condition>,
    /// Blockers the chain's functions raise that belong to the convert stage (a primitive set the network lacks).
    pub convert_extra: Vec<Blocker>,
    pub register: Vec<Blocker>,
    pub mine: Vec<Blocker>,
    pub seat: Option<SeatInfo>,
    pub forecast: Option<Forecast>,
    pub notes: Vec<String>,
}

impl ChainOutput {
    /// The mine stage: the seat's memory is the one thing a class not yet registered can be judged on; ready seats
    /// and possession exist only for a class that is on the chain.
    pub fn mine_verdict(&self, convert: &StageVerdict) -> StageVerdict {
        let _ = convert;
        StageVerdict::of(self.mine.clone())
    }
}

fn cond(id: &str, what: &str, needed: Option<u64>, limit: Option<u64>, unit: &str, ok: Option<bool>, source: &str) -> Condition {
    Condition { id: id.into(), what: what.into(), needed, limit, unit: unit.into(), ok, source: source.into() }
}

fn le(needed: u64, limit: u64) -> Option<bool> {
    Some(needed <= limit)
}

fn bond_key() -> kaspa_consensus_core::palw_state_v2::PalwBondKeyV2 {
    kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
        kaspa_consensus_core::tx::TransactionId::from_bytes([0; 64]),
        0,
    ))
}

/// What the gate said, typed.
enum Gate {
    Admitted(
        Box<(
            kaspa_consensus_core::palw_mode_v2::PalwClassCatalogEntryV2,
            kaspa_consensus_core::palw_tir_admission_v1::PalwTirClassRecordV1,
        )>,
    ),
    Refused(PalwClassAdmissionError),
    /// The object could not be built (a context too narrow for a canonical job): the string says why.
    Unbuildable(String),
}

/// `tir_class_admission_offline_v1`, typed: the object admission v10 judges, built the way the node builds it, and the
/// gate the acceptance path runs at `daa` (`palw_tir_registration_preflight_at_v1`).
fn gate(params: &Params, bundle: &PalwConsensusParamsV2, class: &PalwTirClassV1, artifact_root: Hash64, daa: u64) -> Gate {
    let program = match class.decode_program() {
        Ok(p) => p,
        Err(e) => return Gate::Unbuildable(format!("the program does not decode: {e}")),
    };
    let class_id = class.class_id(&artifact_root);
    let Some(canonical) = kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_attempt_canonical_v1(class) else {
        return Gate::Unbuildable(format!("a context of {} positions is too narrow for a canonical job", class.layout.max_context));
    };
    let facts = PalwTirJobFactsV1::of(class, &program, class_id);
    let job = palw_tir_job_context_v1(&facts, canonical);
    let object = match palw_tir_post_genesis_registration_v1(
        class.clone(),
        job,
        artifact_root,
        0,
        u128::MAX,
        1,
        0,
        bond_key(),
        Vec::new(),
        bundle.court.max_step_leaf_count(),
    ) {
        Ok(o) => o,
        Err(e) => return Gate::Refused(e),
    };
    match palw_tir_registration_preflight_at_v1(params, bundle, &object, daa, &[]) {
        Ok(ok) => Gate::Admitted(Box::new(ok)),
        Err(e) => Gate::Refused(e),
    }
}

/// A refusal of admission v10 as a blocker, with the code it has on the chain mapped to the preflight's.
fn gate_blocker(e: &PalwClassAdmissionError, window_hint: &[String]) -> Blocker {
    use PalwClassAdmissionError as E;
    let on_chain = e.code();
    match e {
        E::TirNeedsItsFence => {
            Blocker::new(Stage::Register, "FENCE_NOT_ARMED", "palw_tir_v1 is not in force at this height").arg("palw_tir_v1")
        }
        E::TirNeedsDissection { block, node } => Blocker::new(
            Stage::Register,
            "FENCE_NOT_ARMED",
            "the class has a cone that reduces over the history, and the court that dissects it is not in force at this height",
        )
        .arg("palw_kary_court")
        .evidence([format!("block {block} node {node}")]),
        E::CourtWindowTooShort { needed, window } => Blocker::new(
            Stage::Register,
            "COURT_WINDOW_EXCEEDED",
            format!("prosecuting the class's widest row takes {needed} DAA and the court's window is {window}"),
        )
        .numbers(*needed, *window, "DAA")
        .evidence([format!("on-chain code {on_chain}")])
        .safe(window_hint.iter().cloned()),
        E::DeeperThanTheLadder { worst, ladder } => Blocker::new(
            Stage::Register,
            "DA_LADDER_EXCEEDED",
            format!("the class's longest job has {worst} step leaves and the ladder in force holds {ladder}"),
        )
        .numbers(*worst, *ladder, "step leaves")
        .evidence([format!("on-chain code {on_chain}")])
        .safe([
            "a smaller --max-context shortens the longest job".to_string(),
            "palw_tir_fence2 raises the ladder past its height".to_string(),
        ]),
        E::CourtCostExceedsCeiling { what, got, ceiling } => {
            let (code, unit) = match *what {
                "IR close bytes"
                | "IR terminal close bytes as carried"
                | "IR dissection root claim bytes"
                | "IR dissection round bytes" => ("CLOSE_SIZE_OVER_CAP", "bytes"),
                "IR tile multiply-accumulates" | "IR cone evaluation work (tile and state replay)" => {
                    ("COURT_COST_OVER_CEILING", "units")
                }
                _ => ("ADMISSION_EXCEEDS", "units"),
            };
            Blocker::new(Stage::Register, code, format!("{what}: {got} against a ceiling of {ceiling}"))
                .arg(*what)
                .numbers(*got, *ceiling, unit)
                .evidence([format!("on-chain code {on_chain}")])
                .safe([
                    "a narrower tile (--tile-len) or logits tile lowers a close; a smaller --max-context lowers the work".to_string()
                ])
        }
        E::TirExceeds { limit, at, value, cap } => Blocker::new(
            Stage::Register,
            if limit.contains("close") { "CLOSE_SIZE_OVER_CAP" } else { "ADMISSION_EXCEEDS" },
            format!("{limit} at {at}: {value} against a cap of {cap}"),
        )
        .arg(*limit)
        .numbers(*value, *cap, "units")
        .evidence([format!("on-chain code {on_chain}")]),
        E::TirCanonicalNotTheFormula(why) => Blocker::new(
            Stage::Register,
            "CANONICAL_JOB_OUT_OF_BOUNDS",
            "the canonical job the class would be paid per is out of bounds",
        )
        .evidence([why.clone(), format!("on-chain code {on_chain}")])
        .safe(["--max-context between 16 and 32,783 positions".to_string()]),
        E::TirPrimSet { .. } => {
            Blocker::new(Stage::Convert, "ARCH_NEEDS_PRIMITIVE", "the program names another primitive set than this network's")
                .evidence([format!("on-chain code {on_chain}")])
        }
        other => Blocker::new(Stage::Register, "ADMISSION_REFUSED", format!("admission v10 refuses the class: {other}"))
            .arg(on_chain)
            .evidence([format!("on-chain code {on_chain}")]),
    }
}

/// The widest context at which `ok(context)` holds, by bisection (`None` when not even 1 holds); `ok` is monotone.
fn widest_context(upto: u32, ok: &dyn Fn(u32) -> bool) -> Option<u32> {
    if upto == 0 || !ok(1) {
        return None;
    }
    let (mut lo, mut hi) = (1u32, upto);
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        if ok(mid) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    Some(lo)
}

/// The tile of the logits node in a layout: its commit tile, in the order the layout lists them.
fn logits_commit_tile(program: &TirProgramV1, layout: &PalwTirLayoutV1) -> Option<u32> {
    let mut i = 0usize;
    for (bi, block) in program.blocks.iter().enumerate() {
        for (ni, node) in block.nodes.iter().enumerate() {
            if node.commit {
                if bi == program.schedule.post as usize && ni == program.logits as usize {
                    return layout.commit_tiles.get(i).copied();
                }
                i += 1;
            }
        }
    }
    None
}

fn n(v: u64) -> String {
    super::render::n(v)
}

/// The recurrent and history state a seat holds at `context` positions: a `Fixed` state whole, a `Hist` state at
/// `min(window, context)` rows, per instance (a per-layer state once per layer block that touches it) — `tir_admit_v1`'s
/// own count, with the context in place of the program's history window.
fn state_bytes_at(program: &TirProgramV1, context: u32) -> u64 {
    use misaka_palw_tir::prim::Prim;
    use misaka_palw_tir::program::StateKind;
    let mut uses = vec![vec![false; program.states.len()]; program.blocks.len()];
    for (bi, b) in program.blocks.iter().enumerate() {
        for n in &b.nodes {
            if let Prim::StateWrite { state } | Prim::HistAppend { state } = n.prim {
                uses[bi][state as usize] = true;
            }
        }
    }
    let mut total = 0u64;
    for (j, s) in program.states.iter().enumerate() {
        let instances = if s.per_layer {
            program.schedule.layers.iter().filter(|b| uses[**b as usize][j]).count() as u64
        } else {
            u64::from(uses.iter().any(|u| u[j]))
        };
        let row = s.shape.iter().fold(s.dtype.width() as u64, |a, d| a.saturating_mul(u64::from(*d)));
        let per = match s.kind {
            StateKind::Fixed { .. } => row,
            StateKind::Hist { window } => row.saturating_mul(u64::from(window.min(context))),
        };
        total = total.saturating_add(per.saturating_mul(instances));
    }
    total
}

/// Judge a shape-only program on a network.
pub fn judge(net: &PreflightNetwork, opts: &Options, program: &TirProgramV1, analysis: &Analysis, src: &Source) -> ChainOutput {
    let _ = src;
    let mut out = ChainOutput::default();
    let params = &net.params;
    let bundle = &net.bundle;

    // ---- the height --------------------------------------------------------------------------------------------------------------
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
    let tir_activation = params.palw_tir_v1_fence().map(|f| f.activation.daa_score());
    let tir_armed = tir_activation.is_some_and(|a| a <= height);
    // Judged at: the height, or the fence's own where the height is below it; a network that has not armed the IR fence
    // is judged as if it had (the class is judged by the gate it will meet, and the fence's absence is its own blocker).
    let judge_daa = height.max(tir_activation.unwrap_or(1)).max(1);
    let (judged, what_if) = match tir_activation {
        Some(a) if a > height => {
            (params.clone(), Some(format!("palw_tir_v1 activates at DAA {a}: the conditions past it are judged at DAA {a}")))
        }
        Some(_) => (params.clone(), None),
        None => {
            let mut armed = params.clone();
            (kaspa_consensus_core::palw_tir_v1::PALW_T12_TIR_V1_ENTRY.set)(&mut armed, Some(ForkActivation::new(1)));
            (armed, Some("palw_tir_v1 is not armed on this network: the conditions past it are judged as if it were (testnet-12's provisional values)".to_string()))
        }
    };
    let params = &judged;

    // ---- the admission sizing --------------------------------------------------------------------------------------------------------
    let ir = check_ir_program_at_v1(params, program, opts.tile_len, opts.h_chunk);
    let (ceilings, _, _) = tir_ceilings_v1(params);
    let mut register: Vec<Blocker> = Vec::new();
    let mut convert_extra: Vec<Blocker> = Vec::new();
    let mut mine: Vec<Blocker> = Vec::new();
    let mut conditions: Vec<Condition> = Vec::new();
    let mut notes: Vec<String> = Vec::new();

    // The fence.
    if !tir_armed {
        register.push(
            Blocker::new(
                Stage::Register,
                "FENCE_NOT_ARMED",
                "palw_tir_v1 is not in force at this height: no IR class can be registered on the network yet",
            )
            .arg("palw_tir_v1")
            .evidence([match tir_activation {
                Some(a) => format!("scheduled at DAA {a}; the height judged is {height}"),
                None => "not scheduled on this network".to_string(),
            }])
            .safe(["the registration waits for the flag day that arms it; everything else below is judged as if it were armed"
                .to_string()]),
        );
    }

    let mut admission = AdmissionInfo {
        verdict: ir.verdict.to_string(),
        ceilings: ir.ceilings_source.clone(),
        program_bytes: ir.program_bytes,
        blocks: ir.blocks,
        nodes: ir.nodes,
        unrolled_nodes: ir.unrolled_nodes,
        graph_ir_root: ir.graph_ir_root.map(|h| h.to_string()),
        layout: None,
        gate: "not asked".into(),
        gate_detail: None,
        numbers: None,
        not_asked: vec![
            "the class id commits to the artifact root, which needs the artifact (a placeholder root is used)".into(),
            "the registrant bond's signature and collateral, and the chain's certified families".into(),
        ],
    };
    let bytes = program.encode();
    let inputs = tir_admit_inputs_v1(&ceilings, opts.tile_len, opts.h_chunk);
    let admitted = misaka_palw_tir::admit::tir_admit_v1(&bytes, &inputs).ok();
    if let Some(a) = &admitted {
        conditions.push(cond(
            "position_macs",
            "multiply-accumulates of one position",
            Some(a.position.cost.macs),
            Some(ceilings.max_macs_per_position),
            "MACs",
            le(a.position.cost.macs, ceilings.max_macs_per_position),
            "tir_admit_v1 (spec 04b §8)",
        ));
        conditions.push(cond(
            "state_bytes",
            "recurrent and history state, at the program's history window",
            Some(a.position.state_bytes),
            Some(ceilings.max_state_bytes),
            "bytes",
            le(a.position.state_bytes, ceilings.max_state_bytes),
            "tir_admit_v1",
        ));
        conditions.push(cond(
            "peak_live_bytes",
            "peak live bytes of one position",
            Some(a.position.peak_live_bytes),
            Some(ceilings.max_peak_live_bytes),
            "bytes",
            le(a.position.peak_live_bytes, ceilings.max_peak_live_bytes),
            "tir_admit_v1 against the fence's ceiling",
        ));
        conditions.push(cond(
            "admission_work",
            "admission's own work (cones)",
            Some(a.cone_work),
            Some(ceilings.max_cone_work),
            "units",
            le(a.cone_work, ceilings.max_cone_work),
            "tir_admit_v1",
        ));
    }
    conditions.push(cond(
        "program_bytes",
        "the program's canonical bytes",
        Some(ir.program_bytes as u64),
        Some(ceilings.max_program_bytes as u64),
        "bytes",
        le(ir.program_bytes as u64, ceilings.max_program_bytes as u64),
        "the fence's max_program_bytes",
    ));
    conditions.push(cond(
        "unrolled_nodes",
        "nodes of one position, unrolled over the schedule",
        Some(ir.unrolled_nodes),
        Some(ceilings.max_unrolled_nodes as u64),
        "nodes",
        le(ir.unrolled_nodes, ceilings.max_unrolled_nodes as u64),
        "the fence's max_unrolled_nodes",
    ));

    // `tir_admit_v1`'s verdict as blockers.
    let mut stop = false;
    // A remote-code reference leaves the fidelity column empty, not the program's own verdict (`LOWERABLE_UNVERIFIED` wraps it).
    let mut judged = &ir.verdict;
    while let ArchVerdictV1::LowerableUnverified { inner } = judged {
        judged = inner;
    }
    match judged {
        // `ADMISSIBLE_GENERIC` is speed only: the registration is not blocked.
        ArchVerdictV1::Admissible { .. } | ArchVerdictV1::AdmissibleGeneric { .. } | ArchVerdictV1::LowerableUnverified { .. } => {}
        ArchVerdictV1::Exceeds { limit, value, cap } => {
            register.push(
                Blocker::new(Stage::Register, "ADMISSION_EXCEEDS", format!("{limit}: {value} against a cap of {cap}"))
                    .arg(limit.clone())
                    .numbers((*value).min(u64::MAX as u128) as u64, (*cap).min(u64::MAX as u128) as u64, "units")
                    .safe(["a narrower model, or the class-specific ceilings of a later fence".to_string()]),
            );
            stop = true;
        }
        ArchVerdictV1::NeedsPrimitive(p) => {
            convert_extra.push(Blocker::new(Stage::Convert, "ARCH_NEEDS_PRIMITIVE", p.clone()).arg("prim_set_id"));
            stop = true;
        }
        ArchVerdictV1::Refused(r) | ArchVerdictV1::NotLowerable(r) | ArchVerdictV1::Unverified(r) | ArchVerdictV1::NeedsKernel(r) => {
            register
                .push(Blocker::new(Stage::Register, "ADMISSION_REFUSED", "tir_admit_v1 refuses the program").evidence([r.clone()]));
            stop = true;
        }
    }

    // ---- the layout and the registration gate --------------------------------------------------------------------------------------------------
    //
    // The context is the class's to declare. Asked for (`--max-context`) it is judged as given; not asked for, it is the WIDEST
    // the registration gate admits — the program's widest context is a ceiling no one declares at (past 32,783 positions the
    // canonical prompt is longer than the chain can attribute), and a verdict "refused at 262,144" would say nothing of the context
    // that registers.
    let mut chosen_layout: Option<PalwTirLayoutV1> = None;
    let mut gate_numbers: Option<GateNumbers> = None;
    let mut canonical_job = None;
    let widest = program.history_bound.min(ceilings.max_context);
    let placeholder_root = Hash64::from_bytes([0; 64]);
    let leaf_estimate = analysis.artifact.as_ref().map(|a| a.inventory_leaves_estimate.min(u32::MAX as u64) as u32).unwrap_or(1 << 16);
    let mut window_hint: Vec<String> = Vec::new();
    if !stop {
        let program_s = crate::tir_layout::tir_program_with_scheme_v1(program, None).unwrap_or_else(|_| program.clone());
        let base = TirLayoutChoiceV1 { max_context: None, tile_len: opts.tile_len, h_chunk: opts.h_chunk, ..Default::default() };
        // Each context is chosen at most once: a choice is a run of the gate (several, to find the logits tile and the interval).
        let memo: std::cell::RefCell<std::collections::BTreeMap<u32, Result<crate::tir_layout::TirChosenLayoutV1, String>>> =
            Default::default();
        let choose = |ctx: u32| -> Result<crate::tir_layout::TirChosenLayoutV1, String> {
            if let Some(hit) = memo.borrow().get(&ctx) {
                return hit.clone();
            }
            let r = tir_choose_layout_v1(
                params,
                bundle,
                &program_s,
                placeholder_root,
                placeholder_root,
                leaf_estimate.max(2),
                &TirLayoutChoiceV1 { max_context: Some(ctx), ..base },
            );
            memo.borrow_mut().insert(ctx, r.clone());
            r
        };
        let passes = |ctx: u32| choose(ctx).map(|c| c.admission.is_ok()).unwrap_or(false);
        let rules = PalwTirAdmissionRulesV1::at(params, judge_daa);
        let has_history_cone = admitted.as_ref().is_some_and(|a| a.cones.iter().any(|c| !c.h_reductions.is_empty()));
        // The court's window, as a function of the declared context: the history dissection's duration.
        let window_model = if has_history_cone {
            rules.as_ref().and_then(|r| r.court).and_then(|k| {
                bundle
                    .court
                    .with_dissection_arity(k.dissection_arity)
                    .ok()
                    .map(|played| (played, k.window_court_daa, rules.as_ref().is_some_and(|r| r.held.armed)))
            })
        } else {
            None
        };
        // **The window the class is judged against** (the class-specific window of release int-10): past `palw_model_court_window` the
        // class carries its OWN finite window, the greater of the network's and the exact minimum its court shape needs
        // (`palw_court_window_for_history_v1`, the very function admission v10 derives it with); below it, or on a network that never
        // armed it (testnet-12 arms it nowhere), the network's `window_court`.
        let model_window_active = rules.as_ref().is_some_and(|r| r.model_court_window_active);
        let limit_at = |ctx: u32, class_specific: bool| -> Option<u64> {
            let (played, window, held) = window_model.as_ref()?;
            if class_specific {
                kaspa_consensus_core::palw_class_admission_v2::palw_court_window_for_history_v1(
                    *window,
                    true,
                    *held,
                    played,
                    u64::from(ctx),
                    opts.h_chunk.max(1),
                )
                .ok()
            } else {
                Some(*window)
            }
        };
        let needed_at = |ctx: u32| -> Option<u64> {
            let (played, _, held) = window_model.as_ref()?;
            let tile = opts.h_chunk.max(1);
            let reserve = kaspa_consensus_core::palw_context_ladder::palw_close_assembly_daa_v1(played.max_close_chunks());
            let worst = if *held {
                played.worst_case_duration_held_daa(u64::from(ctx), tile)
            } else {
                played.worst_case_duration_with_history_daa(u64::from(ctx), tile)
            }?;
            worst.checked_add(reserve)
        };
        let window_at = |ctx: u32| -> Option<(u64, bool)> {
            let needed = needed_at(ctx)?;
            let limit = limit_at(ctx, model_window_active)?;
            Some((needed, needed < limit))
        };
        let (max_context, searched) = match opts.max_context {
            Some(c) => (c, false),
            None => {
                // The ceiling of the search: the program's widest context, and the canonical prompt's inline bound.
                let inline = kaspa_consensus_core::palw_attempt_rules_v1::PALW_J5_INLINE_PROMPT_IDS_V1;
                let mut hi = widest.min(inline.saturating_mul(8).saturating_add(15));
                if window_model.is_some() {
                    hi = widest_context(hi, &|c| window_at(c).is_some_and(|(_, ok)| ok)).unwrap_or(hi);
                }
                // Halve until the gate admits (a handful of tries: each is a run of the gate), then bisect between the last refusal
                // and the first admission.
                let mut c = hi.max(16);
                let mut refused_above: Option<u32> = None;
                let mut found: Option<u32> = None;
                let mut tries = 0;
                while c >= 16 && tries < 7 {
                    if passes(c) {
                        found = Some(c);
                        break;
                    }
                    refused_above = Some(c);
                    c /= 2;
                    tries += 1;
                }
                match (found, refused_above) {
                    (Some(mut lo), Some(mut up)) => {
                        let mut steps = 0;
                        while up - lo > 1 && steps < 5 {
                            let mid = lo + (up - lo) / 2;
                            if passes(mid) {
                                lo = mid;
                            } else {
                                up = mid;
                            }
                            steps += 1;
                        }
                        (lo, true)
                    }
                    (Some(lo), None) => (lo, lo != widest),
                    // None admitted from the ceiling down to 16: judge at the ceiling, where the refusal is the one to act on.
                    (None, _) => (hi.max(16), false),
                }
            }
        };
        match choose(max_context) {
            Ok(chosen) => {
                let class = PalwTirClassV1 {
                    version: PALW_TIR_CLASS_VERSION_V1,
                    program: program_s.encode(),
                    layout: chosen.layout.clone(),
                    tokenizer_id: placeholder_root,
                };
                let logits_tile = logits_commit_tile(&program_s, &chosen.layout);
                admission.layout = Some(LayoutInfo {
                    max_context: chosen.layout.max_context,
                    checkpoint_interval: chosen.layout.checkpoint_interval,
                    h_tile: chosen.layout.h_tile,
                    commit_tiles: chosen.layout.commit_tiles.len(),
                    logits_tile,
                    searched,
                    widest_context: widest,
                });
                // What the program's widest context meets, said once when the declared one is narrower.
                if searched && max_context < widest {
                    let top = widest.min(32_783);
                    let why = match choose(top.max(16)) {
                        Ok(c) => c.admission.err(),
                        Err(e) => Some(e),
                    };
                    notes.push(format!(
                        "the declared context is {} positions: the widest at which admission v10 admits the class (the program reads up to {}; --max-context N declares another){}",
                        n(u64::from(max_context)),
                        n(u64::from(widest)),
                        why.map(|w| format!("; at {} it says: {w}", n(u64::from(top)))).unwrap_or_default()
                    ));
                }
                // The court window on its own: the history dissection at the declared context.
                if has_history_cone {
                    match (&window_model, window_at(max_context)) {
                        (Some((_, window, _)), Some((needed, fits))) => {
                            let limit = limit_at(max_context, model_window_active).unwrap_or(*window);
                            conditions.push(cond(
                                "court_window",
                                if model_window_active {
                                    "DAA the court needs to adjudicate the history dissection at the declared context (strictly below the class's own window)"
                                } else {
                                    "DAA the court needs to adjudicate the history dissection at the declared context (strictly below the window)"
                                },
                                Some(needed),
                                Some(limit),
                                "DAA",
                                Some(fits),
                                if model_window_active {
                                    "palw_court_window_for_history_v1 (the class-specific window: palw_model_court_window is in force at this height)"
                                } else {
                                    "palw_attn_court_admits_row_v1 (ADR-0082 Z4); the network's window: palw_model_court_window is not in force at this height"
                                },
                            ));
                            // What the class's own window would be, said where the fence is dormant (it is armed nowhere today).
                            if !model_window_active && let Some(own) = limit_at(max_context, true) {
                                notes.push(format!(
                                    "court window: under palw_model_court_window (dormant on this network) the class would be given its own window of {} DAA (the network's is {} DAA)",
                                    n(own),
                                    n(*window)
                                ));
                            }
                            if !fits {
                                let widest_fit = widest_context(max_context, &|c| window_at(c).is_some_and(|(_, ok)| ok));
                                window_hint = match widest_fit {
                                    Some(w) => vec![format!(
                                        "declare a context of at most {w} positions (--max-context {w}): the court then needs {} DAA of the {window} the window gives",
                                        window_at(w).map(|x| x.0).unwrap_or(0)
                                    )],
                                    None => vec!["no context fits the court's window on this network".to_string()],
                                };
                                if !model_window_active {
                                    window_hint.push(
                                        "the class-specific court window (palw_model_court_window), where a network arms it, gives the class its own finite window".to_string(),
                                    );
                                }
                            }
                        }
                        _ => conditions.push(cond(
                            "court_window",
                            "the court that dissects a history reduction is not in force at this height",
                            None,
                            None,
                            "DAA",
                            Some(false),
                            "palw_kary_court",
                        )),
                    }
                } else {
                    conditions.push(cond(
                        "court_window",
                        "no cone reduces over the history: nothing is dissected, the window is not asked",
                        None,
                        None,
                        "DAA",
                        Some(true),
                        "tir_admit_v1 cones",
                    ));
                }
                // The canonical job.
                match palw_tir_attempt_canonical_of_v1(max_context) {
                    Some((prefill, decode)) => {
                        let inline = u64::from(kaspa_consensus_core::palw_attempt_rules_v1::PALW_J5_INLINE_PROMPT_IDS_V1);
                        conditions.push(cond(
                            "canonical_job",
                            &format!(
                                "canonical prompt of the attempt formula at a context of {} ({prefill} prefill, {decode} decode)",
                                n(u64::from(max_context))
                            ),
                            Some(u64::from(prefill)),
                            Some(inline),
                            "token ids",
                            le(u64::from(prefill), inline),
                            "palw_tir_attempt_canonical_of_v1; J5b's inline prompt bound",
                        ));
                        let facts = PalwTirJobFactsV1::of(&class, program, class.class_id(&placeholder_root));
                        canonical_job = Some(palw_tir_job_context_v1(&facts, (prefill, decode)));
                    }
                    None => conditions.push(cond(
                        "canonical_job",
                        "the attempt formula needs a context of at least 16 positions",
                        Some(u64::from(max_context)),
                        Some(16),
                        "positions",
                        Some(false),
                        "palw_tir_attempt_canonical_of_v1",
                    )),
                }
                // The registration gate (typed).
                match gate(params, bundle, &class, placeholder_root, judge_daa) {
                    Gate::Admitted(b) => {
                        let (entry, _record) = *b;
                        admission.gate = "admitted".into();
                        let g = GateNumbers {
                            max_step_leaf_count: entry.max_step_leaf_count,
                            canonical_step_leaf_count: entry.canonical_step_leaf_count,
                            max_close_bytes: entry.court_cost.max_close_bytes,
                            max_terminal_macs: entry.court_cost.max_terminal_macs,
                            max_operand_count: u64::from(entry.court_cost.max_operand_count),
                        };
                        let carriable = palw_tir_carriable_close_bytes_v1(&bundle.court);
                        conditions.push(cond(
                            "close_bytes",
                            "the worst terminal close of any commit point",
                            Some(g.max_close_bytes),
                            Some(bundle.court.max_close_bytes()),
                            "bytes",
                            le(g.max_close_bytes, bundle.court.max_close_bytes()),
                            "admission v10 (PALW-TIR-38); carriable in at most the bytes below",
                        ));
                        conditions.push(cond(
                            "carriable_close_bytes",
                            "every terminal close as carried (parameters in the multiproof) fits the chunks the fold assembles",
                            None,
                            Some(carriable),
                            "bytes",
                            Some(true),
                            "palw_tir_carried_closes_admit_v1",
                        ));
                        conditions.push(cond(
                            "terminal_macs",
                            "multiply-accumulates a full node redoes at the terminal step",
                            Some(g.max_terminal_macs),
                            Some(bundle.court.max_terminal_macs()),
                            "MACs",
                            le(g.max_terminal_macs, bundle.court.max_terminal_macs()),
                            "the court's cost ceiling (derive_court_cost_v1)",
                        ));
                        conditions.push(cond(
                            "operand_count",
                            "rows one disputed step reads",
                            Some(g.max_operand_count),
                            Some(u64::from(bundle.court.max_operand_count())),
                            "rows",
                            le(g.max_operand_count, u64::from(bundle.court.max_operand_count())),
                            "the court's cost ceiling",
                        ));
                        conditions.push(cond(
                            "da_ladder",
                            "step leaves of the class's longest job against the ladder in force",
                            Some(g.max_step_leaf_count),
                            Some(bundle.court.max_step_leaf_count()),
                            "step leaves",
                            le(g.max_step_leaf_count, bundle.court.max_step_leaf_count()),
                            "the court's max_step_leaf_count (2^22 below palw_tir_fence2, the class's own ladder past it)",
                        ));
                        gate_numbers = Some(g);
                    }
                    Gate::Refused(e) => {
                        admission.gate = e.code().to_string();
                        admission.gate_detail = Some(e.to_string());
                        // The IR fence has its own blocker above; a gate refusal that is only the fence is not repeated.
                        if !matches!(e, PalwClassAdmissionError::TirNeedsItsFence) {
                            let decided_by = kaspa_consensus_core::palw_refusal_v1::palw_refusal_decided_by_v1(
                                params.palw_held_context_active_at(judge_daa),
                                Some(judge_daa),
                                params.palw_held_context.map(|f| f.daa_score()),
                            );
                            let mut b = gate_blocker(&e, &window_hint);
                            b.evidence.push(format!("refusal {}", e.refusal_v1(&decided_by).to_json()));
                            if b.stage == Stage::Convert {
                                // A primitive-set mismatch belongs to the convert stage.
                                convert_extra.push(b);
                            } else {
                                register.push(b);
                            }
                        }
                        // The walls the refusal names, with their numbers.
                        if let PalwClassAdmissionError::DeeperThanTheLadder { worst, ladder } = &e {
                            conditions.push(cond(
                                "da_ladder",
                                "step leaves of the class's longest job against the ladder in force",
                                Some(*worst),
                                Some(*ladder),
                                "step leaves",
                                Some(false),
                                "the court's max_step_leaf_count",
                            ));
                        }
                        if let PalwClassAdmissionError::CourtCostExceedsCeiling { what, got, ceiling } = &e {
                            conditions.push(cond(
                                "court_cost",
                                what,
                                Some(*got),
                                Some(*ceiling),
                                "units",
                                Some(false),
                                "admission v10",
                            ));
                        }
                    }
                    Gate::Unbuildable(why) => {
                        admission.gate = "not asked".into();
                        admission.gate_detail = Some(why.clone());
                        register.push(
                            Blocker::new(
                                Stage::Register,
                                "CANONICAL_JOB_OUT_OF_BOUNDS",
                                "no canonical job can be built at the declared context",
                            )
                            .evidence([why])
                            .safe(["--max-context between 16 and 32,783 positions".to_string()]),
                        );
                    }
                }
                chosen_layout = Some(chosen.layout);
            }
            Err(e) => {
                register.push(
                    Blocker::new(Stage::Register, "ADMISSION_REFUSED", "no layout can be derived for the program").evidence([e]),
                );
            }
        }
    }
    admission.numbers = gate_numbers;

    // ---- every wall at once ------------------------------------------------------------------------------------------------------------------
    //
    // The gate refuses at the first wall it meets; a model that fails two is not helped by fixing one and meeting the other. The
    // conditions computed on their own that are over their limit become blockers too, unless the gate already named that code.
    for c in conditions.iter().filter(|c| c.ok == Some(false)) {
        let (code, what): (&str, &str) = match c.id.as_str() {
            "court_window" => {
                ("COURT_WINDOW_EXCEEDED", "the court needs more DAA to adjudicate the history dissection than its window gives")
            }
            "canonical_job" => ("CANONICAL_JOB_OUT_OF_BOUNDS", "the canonical job the class would be paid per is out of bounds"),
            "da_ladder" => ("DA_LADDER_EXCEEDED", "the class's longest job has more step leaves than the ladder in force holds"),
            "court_cost" => ("COURT_COST_OVER_CEILING", "a cost the court pays to prosecute the class is over its ceiling"),
            id if id.starts_with("fence:") => continue,
            _ => ("ADMISSION_EXCEEDS", "admission's sizing of the program is over a ceiling"),
        };
        if let Some(b) =
            register.iter_mut().find(|b| b.code == code && (code != "ADMISSION_EXCEEDS" || b.arg.as_deref() == Some(c.id.as_str())))
        {
            // The gate named it without numbers: the condition has them.
            if let (None, Some(need), Some(limit)) = (b.have, c.needed, c.limit) {
                b.have = Some(need);
                b.need = Some(limit);
                b.unit = Some(c.unit.clone());
            }
            continue;
        }
        let mut b = Blocker::new(Stage::Register, code, format!("{what} ({}: {})", c.id, c.what));
        if code == "ADMISSION_EXCEEDS" {
            b = b.arg(c.id.clone());
        }
        if let (Some(need), Some(limit)) = (c.needed, c.limit) {
            b = b.numbers(need, limit, &c.unit);
        }
        if code == "COURT_WINDOW_EXCEEDED" {
            b = b.safe(window_hint.iter().cloned());
        }
        if code == "CANONICAL_JOB_OUT_OF_BOUNDS" {
            b = b.safe(["--max-context between 16 and 32,783 positions".to_string()]);
        }
        register.push(b);
    }

    // ---- the seat ---------------------------------------------------------------------------------------------------------------------------
    let mut seat = None;
    if let (Some(layout), Some(a)) = (&chosen_layout, &admitted) {
        let artifact_bytes = analysis.artifact.as_ref().map(|x| x.params_bytes).unwrap_or(0);
        let ctx = layout.max_context;
        let state = state_bytes_at(program, ctx);
        let widest_tile = a.cones.iter().map(|c| c.tile_opened_bytes).max().unwrap_or(0);
        let peak = a.position.peak_live_bytes;
        let needed = artifact_bytes.saturating_add(state).saturating_add(peak).saturating_add(widest_tile);
        let (tiers_in, tiers_source) = if opts.seat_shares.is_empty() {
            (default_seat_shares(), "the testnet-12 fleet's seat tiers (--seat-share replaces them)".to_string())
        } else {
            (opts.seat_shares.clone(), "given (--seat-share)".to_string())
        };
        let tiers: Vec<SeatTier> = tiers_in
            .iter()
            .map(|t| {
                let fits = needed <= t.bytes;
                let fits_at_context = if fits {
                    None
                } else {
                    widest_context(ctx, &|c| {
                        artifact_bytes.saturating_add(state_bytes_at(program, c)).saturating_add(peak).saturating_add(widest_tile)
                            <= t.bytes
                    })
                };
                SeatTier { name: t.name.clone(), share_bytes: t.bytes, fits, fits_at_context }
            })
            .collect();
        let holding: Vec<&str> = tiers.iter().filter(|t| t.fits).map(|t| t.name.as_str()).collect();
        if holding.is_empty() {
            let biggest = tiers.iter().map(|t| t.share_bytes).max().unwrap_or(0);
            mine.push(
                Blocker::new(
                    Stage::Mine,
                    "SEAT_MEMORY_SHORT",
                    "no seat tier holds the class: a seat never becomes ready for it, because it cannot replay a claim",
                )
                .numbers(needed, biggest, "bytes")
                .evidence(tiers.iter().map(|t| format!("{}: share {}", t.name, super::render::size(t.share_bytes))))
                .safe(
                    tiers
                        .iter()
                        .filter_map(|t| {
                            t.fits_at_context
                                .filter(|c| *c > 0)
                                .map(|c| format!("at a context of {c} positions the class fits {} (--max-context {c})", t.name))
                        })
                        .chain(std::iter::once("or a smaller model, or a larger seat".to_string())),
                ),
            );
        } else if holding.len() < tiers.len() {
            let short: Vec<&str> = tiers.iter().filter(|t| !t.fits).map(|t| t.name.as_str()).collect();
            notes.push(format!(
                "only {} can hold the class; {} cannot, so those seats never become ready for it (RFC-0002 §II.7.3 F4)",
                holding.join(", "),
                short.join(", ")
            ));
        }
        seat = Some(SeatInfo {
            artifact_bytes,
            state_bytes: state,
            peak_live_bytes: peak,
            widest_tile_opened_bytes: widest_tile,
            needed_bytes: needed,
            tiers_source,
            tiers,
            note: "an estimate: the artifact mapped, the state at the declared context, one position's peak live bytes and the widest tile a close opens".into(),
        });
    }

    // ---- the forecast -----------------------------------------------------------------------------------------------------------------------
    let mut forecast = None;
    if let (Some(canonical), true) = (&canonical_job, chosen_layout.is_some()) {
        let min_select = params.palw_tir_fence2.is_some_and(|f| f.is_active(judge_daa));
        if let Ok(work) = kaspa_consensus_core::palw_tir_work_v1::palw_tir_model_work_v2(program, canonical, min_select) {
            let g = kaspa_consensus_core::palw_model_registry_v1::palw_registry_globals_of_bundle_v1(bundle);
            let gate_on = params.palw_seat_gate_possession_at(judge_daa);
            let p = kaspa_consensus_core::palw_model_registry_v1::palw_derive_profile_v1(&work, &g, gate_on);
            let span_ms = kaspa_consensus_core::palw_verification_profile_v1::PALW_SPAN_MS_V1;
            let audit = params.palw_admission_audit_period_daa.unwrap_or_else(|| bundle.state.epoch_length());
            let mut path = vec![
                format!("registered, then an admission jury at the next audit (every {audit} DAA)"),
                format!("prefetch: {} span(s) for the artifact to be paged in", p.artifact_prefetch_spans),
                format!("{} ready seat(s) of distinct operators prove possession", p.required_ready_seats),
                format!("probation: {} probe claims finalise with none failing", g.probation_claims),
                format!("limited, then active after {} stable spans", g.stable_epochs),
            ];
            path.push(format!("a claim's verification window is {} span(s)", p.verification_window_spans));
            forecast = Some(Forecast {
                verification_window_spans: p.verification_window_spans,
                artifact_prefetch_spans: p.artifact_prefetch_spans,
                max_inflight_claims: p.max_inflight_claims,
                required_ready_seats: p.required_ready_seats,
                registration_bond_sompi: p.registration_bond_sompi,
                admission_claims_per_span_milli: p.admission_claims_per_span_milli,
                probation_claims: g.probation_claims,
                stable_spans: g.stable_epochs,
                audit_period_daa: audit,
                span_ms,
                path,
                note: "informational: the registry derives this profile from the class's work (palw_tir_model_work_v2, palw_derive_profile_v1); the chain decides, and how many independent operators the network has against the seating floor is a chain fact (RFC-0002 §II.7.5)".into(),
                independence: {
                    let terms = params.palw_class_seating_terms_at(judge_daa);
                    let floor = terms.map(|t| u32::from(t.independent_floor)).unwrap_or_else(|| {
                        u32::from(kaspa_consensus_core::palw_class_seating_fence_v1::PALW_CLASS_SEATING_T12_INDEPENDENT_FLOOR_V1)
                    });
                    let base = opts.node.as_ref().and_then(|n| n.base_operators());
                    if terms.is_some() || opts.node.is_some() {
                        Some(ForecastIndependence {
                            fence_in_force: terms.is_some(),
                            independent_floor: floor,
                            seat_count: g.seat_count as u32,
                            base_operators: base,
                            licensable_share_at_floor_permille: base.filter(|b| *b > 0).map(|b| ((u64::from(floor) * 1000) / u64::from(b)).min(1000) as u16),
                            note: if terms.is_some() {
                                format!(
                                    "palw_class_seating is in force: a claim is admitted only with {} distinct ready operators besides its executor, {floor} of them independent of the registrant and the executor",
                                    g.seat_count
                                )
                            } else {
                                format!(
                                    "palw_class_seating is not in force on this network at this height; when a flag day arms it, {floor} independent operators (the admission jury's strict majority of a panel) will be needed beside {} ready operators",
                                    g.seat_count
                                )
                            },
                        })
                    } else {
                        None
                    }
                },
            });
        }
    }

    // ---- fences ------------------------------------------------------------------------------------------------------------------------------
    let dissected = admitted.as_ref().is_some_and(|a| a.cones.iter().any(|c| !c.h_reductions.is_empty()));
    let fences: Vec<FenceRow> = net
        .params
        .palw_fences_v1()
        .into_iter()
        .map(|(name, act)| {
            let a = act.map(|f| f.daa_score()).filter(|s| *s != u64::MAX);
            let in_force = if name == "palw_tir_v1" { tir_armed } else { a.is_some_and(|a| a <= judge_daa) };
            FenceRow {
                name: name.to_string(),
                activation: a,
                in_force,
                needed: name == "palw_tir_v1" || (dissected && name == "palw_kary_court"),
            }
        })
        .collect();
    for f in fences.iter().filter(|f| f.needed) {
        conditions.push(cond(
            &format!("fence:{}", f.name),
            &format!("{} is in force", f.name),
            f.activation,
            Some(if f.name == "palw_tir_v1" { height } else { judge_daa }),
            "DAA",
            Some(f.in_force),
            "Params::palw_fences_v1 (the network's own schedule)",
        ));
    }

    out.network = NetworkInfo { id: net.id.clone(), daa: judge_daa, daa_choice, tir_armed, what_if, fences };
    out.admission = Some(admission);
    out.conditions = conditions;
    out.convert_extra = convert_extra;
    out.register = register;
    out.mine = mine;
    out.seat = seat;
    out.forecast = forecast;
    out.notes = notes;
    out
}
