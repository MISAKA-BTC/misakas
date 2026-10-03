//! **One machine-readable row per repository** (`misaka.palw.hf-census-row.v1`, RFC-0002 §II.10.3): six gates, one stable `blocking`
//! code per failed gate, `NOT_RUN_AFTER_<GATE>` after a failure, and why a gate was not run when nothing failed before it.
//!
//! * **Two views.** `technical` judges every gate with the rights decision left out; `gates` (the strict view, the headline's) applies
//!   the rights policy on top: a repository whose rights are not confirmed fails `source` with `RIGHTS_UNCONFIRMED` and every later gate
//!   is `NOT_RUN_AFTER_SOURCE`. Both are measurements; neither infers a pass.
//! * **Depths.** A repository only listed is judged at the `listing` depth (the gates the listing decides; the others are
//!   `NOT_RUN_NOT_SAMPLED`); a fetched one at the `headers` depth (the preflight's convert stage for `lower`, its register stage at the
//!   shape depth for `admit`). `pack` needs the weights and `seat`/`final` a chain: in a census they are never `PASS`.

use super::codes::{self, Gate, GateStatus};
use super::listing::{ArtifactKind, ListingV1, SelectedV1, StrataV1, TaskV1, select, strata_of, task_of};
use super::rights::{RightsPolicy, RightsV1, rights_of};
use super::store::{self, Fetched};
use super::tasks::Profile;
use crate::preflight::{self, Report, Stage, StageStatus};
use serde::Serialize;

pub const ROW_SCHEMA_V1: &str = "misaka.palw.hf-census-row.v1";

/// One gate's result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GateResultV1 {
    pub gate: Gate,
    pub status: GateStatus,
    /// FAIL: the gate's one code (the highest-ranked of `codes`); NOT_RUN: why; PASS: none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking: Option<String>,
    /// The blocking code's argument (a feature, a task, a file, a fence).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arg: Option<String>,
    /// Every code the gate found, ranked.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub codes: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
    /// The depth the result was established at (`listing`, `headers`, `shape`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<String>,
}

/// A code found at a gate, with its argument and evidence.
#[derive(Clone, Debug)]
struct Found {
    code: String,
    arg: Option<String>,
    evidence: Vec<String>,
}

fn found(code: &str, arg: Option<String>, evidence: Vec<String>) -> Found {
    Found { code: code.to_string(), arg, evidence }
}

fn fail(gate: Gate, mut f: Vec<Found>, depth: &str) -> GateResultV1 {
    f.sort_by_key(|x| codes::priority(gate, &x.code));
    let mut cs: Vec<String> = Vec::new();
    for x in &f {
        if !cs.contains(&x.code) {
            cs.push(x.code.clone());
        }
    }
    let first = f.first().cloned().expect("a failure has a code");
    let mut evidence: Vec<String> = first.evidence.clone();
    evidence.truncate(8);
    GateResultV1 {
        gate,
        status: GateStatus::Fail,
        blocking: Some(first.code),
        arg: first.arg,
        codes: cs,
        evidence,
        depth: Some(depth.into()),
    }
}

fn pass(gate: Gate, depth: &str, evidence: Vec<String>) -> GateResultV1 {
    GateResultV1 { gate, status: GateStatus::Pass, blocking: None, arg: None, codes: vec![], evidence, depth: Some(depth.into()) }
}

fn not_run(gate: Gate, why: &str, evidence: Vec<String>) -> GateResultV1 {
    GateResultV1 { gate, status: GateStatus::NotRun, blocking: Some(why.into()), arg: None, codes: vec![], evidence, depth: None }
}

/// What the preflight said, summarised for the row.
#[derive(Clone, Debug, Serialize)]
pub struct PreflightSummaryV1 {
    pub depth_reached: String,
    pub stopped_at: Option<String>,
    pub level: Option<String>,
    pub model_type: Option<String>,
    pub architectures: Vec<String>,
    pub spec_digest: Option<String>,
    pub scope_task: Option<String>,
    pub text_only: Option<bool>,
    pub features_missing: Vec<String>,
    /// Every blocker, `stage:CODE(arg)`.
    pub blockers: Vec<String>,
    pub max_context: Option<u32>,
    pub artifact_bytes: Option<u64>,
    pub download_bytes_needed: Option<u64>,
    pub notes: Vec<String>,
}

/// The ruleset a row was judged against.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RulesetV1 {
    pub network: String,
    pub height: Option<u64>,
    /// The context every class is declared at (`None`: the preflight's search for the widest the gate admits).
    pub max_context: Option<u32>,
    /// The source tree the census binary was built from (its git commit), as the runner states it.
    pub tree: String,
    pub tasks: String,
}

/// **The row.**
#[derive(Clone, Debug, Serialize)]
pub struct CensusRowV1 {
    pub schema: &'static str,
    pub snapshot: String,
    pub repo: String,
    pub revision: Option<String>,
    pub object_id: String,
    /// `listing` or `headers`.
    pub depth: String,
    pub task: TaskV1,
    pub strata: StrataV1,
    pub selected: SelectedV1,
    pub downloads: u64,
    pub downloads_all: u64,
    pub likes: u64,
    pub params: Option<u64>,
    pub rights: RightsV1,
    /// The strict view: the rights policy applied.
    pub gates: Vec<GateResultV1>,
    /// The technical view: every gate with the rights decision left out.
    pub technical: Vec<GateResultV1>,
    /// The gate the strict view stopped at (its first gate that is not PASS), and the technical view's.
    pub stopped_at: String,
    pub technical_stopped_at: String,
    /// `source`, `lower` and `admit` PASS (the shape depth) in the technical view; `pack` is not run.
    pub shape_ready: bool,
    /// `source` through `admit` PASS in the strict view, `pack` included: never true in a header census.
    pub registration_ready: bool,
    pub preflight: Option<PreflightSummaryV1>,
    /// The content identity of the selected weights (the inventory's LFS ids).
    pub weights_identity: Option<String>,
    pub weights_bytes: Option<u64>,
    pub ruleset: RulesetV1,
}

/// The census's fixed inputs.
pub struct CensusContext {
    pub snapshot: String,
    pub policy: RightsPolicy,
    pub options: preflight::Options,
    pub ruleset: RulesetV1,
}

impl CensusContext {
    /// The preflight options of a census: the shape depth on `network`, at `height` (`None`: the last scheduled fence height, as
    /// the preflight chooses it), the reference seat tiers.
    pub fn new(snapshot: &str, network: &str, height: Option<u64>, policy: RightsPolicy, tree: &str) -> CensusContext {
        let options =
            preflight::Options { depth: preflight::Depth::Shape, network: Some(network.to_string()), height, ..Default::default() };
        CensusContext {
            snapshot: snapshot.to_string(),
            policy,
            options,
            ruleset: RulesetV1 {
                network: network.to_string(),
                height,
                max_context: None,
                tree: tree.to_string(),
                tasks: super::tasks::tasks_digest(),
            },
        }
    }
}

/// The source gate's codes the listing decides (access, files, the base of an adapter, the shard set's completeness by name).
fn source_listing(l: &ListingV1, sel: &SelectedV1) -> Vec<Found> {
    let mut f = Vec::new();
    if l.disabled {
        f.push(found(codes::REPO_DISABLED, None, vec![]));
    }
    if let Some(g) = l.gated() {
        f.push(found(codes::GATED_ACCESS, Some(g.clone()), vec![format!("gated: {g}")]));
    }
    if sel.kind == ArtifactKind::None {
        f.push(found(codes::MISSING_WEIGHTS, None, vec![format!("{} files, none a weight file", l.siblings.len())]));
    }
    if sel.kind == ArtifactKind::Adapter {
        let mut ids: Vec<String> = l.base_ids.clone();
        if let Some(b) = l.config.get("peft").and_then(|p| p.get("base_model_name_or_path")).and_then(|b| b.as_str())
            && !ids.iter().any(|x| x == b)
        {
            ids.push(b.to_string());
        }
        match ids.len() {
            0 => f.push(found(codes::BASE_UNPINNED, Some("absent".into()), vec!["an adapter that names no base".into()])),
            1 => {
                let r = l.base_resolved.iter().find(|r| r.id == ids[0]);
                match r {
                    Some(r) if r.found && !r.gated && !r.disabled && r.sha.is_some() => {}
                    Some(r) if r.found && r.gated => {
                        f.push(found(codes::BASE_UNPINNED, Some("base_gated".into()), vec![format!("base {} is gated", r.id)]))
                    }
                    _ => f.push(found(
                        codes::BASE_UNPINNED,
                        Some("not_in_snapshot".into()),
                        vec![format!(
                            "base `{}` is not a public repository of the snapshot (a local path, a renamed or private repository)",
                            ids[0]
                        )],
                    )),
                }
            }
            n => f.push(found(codes::BASE_UNPINNED, Some("ambiguous".into()), vec![format!("{n} bases: {}", ids.join(", "))])),
        }
    }
    // A shard set named `-0000k-of-0000n` with a part missing from the listing.
    let mut by_set: std::collections::BTreeMap<(String, u32), Vec<u32>> = Default::default();
    for w in &sel.weights {
        let name = w.rsplit('/').next().unwrap_or(w);
        if let Some(stem) = name.strip_suffix(".safetensors")
            && let Some((head, count)) = stem.rsplit_once("-of-")
            && let Some((prefix, part)) = head.rsplit_once('-')
            && let (Ok(p), Ok(c)) = (part.parse::<u32>(), count.parse::<u32>())
        {
            by_set.entry((format!("{}/{prefix}", w.rsplit_once('/').map(|x| x.0).unwrap_or("")), c)).or_default().push(p);
        }
    }
    for ((prefix, count), parts) in by_set {
        if (parts.len() as u32) < count {
            f.push(found(
                codes::WEIGHTS_INCOMPLETE,
                Some(prefix.trim_start_matches('/').to_string()),
                vec![format!("{} of {count} shards listed", parts.len())],
            ));
        }
    }
    f
}

/// The lower gate's codes the listing decides: the task, the artifact's form.
fn lower_listing(l: &ListingV1, task: &TaskV1, sel: &SelectedV1) -> Vec<Found> {
    let mut f = Vec::new();
    if task.task == "unknown" {
        f.push(found(
            codes::TASK_UNKNOWN,
            None,
            vec!["no pipeline_tag, and the configuration does not name a causal language model".into()],
        ));
    } else if task.profile == Profile::None {
        f.push(found(
            codes::MODALITY_PROFILE_MISSING,
            Some(task.task.clone()),
            vec![format!("task `{}` has no canonical job profile", task.task)],
        ));
    } else if task.profile == Profile::PartialTextStage {
        f.push(found(
            codes::PARTIAL_TASK_ONLY,
            Some(task.task.clone()),
            vec![format!("task `{}`: the class this build's preflight produces is the text stage only", task.task)],
        ));
    }
    match sel.kind {
        ArtifactKind::Other => f.push(found(
            codes::FORMAT_UNSUPPORTED,
            Some(sel.formats.join("+")),
            vec![format!("weights only as {}", sel.formats.join(", "))],
        )),
        ArtifactKind::Adapter => f.push(found(
            codes::ADAPTER_UNCHECKED,
            l.config.get("peft").and_then(|p| p.get("task_type")).and_then(|t| t.as_str()).map(str::to_string),
            vec!["the census does not yet compose an adapter with its base".into()],
        )),
        ArtifactKind::Gguf if sel.weights.len() > 1 => f.push(found(
            codes::FORMAT_UNSUPPORTED,
            Some("gguf-split".into()),
            vec![format!("a split GGUF in {} parts", sel.weights.len())],
        )),
        ArtifactKind::SafetensorsOther if sel.config.is_none() => f.push(found(
            codes::CONFIG_MISSING,
            None,
            vec![format!(
                "safetensors without a configuration: {}",
                sel.weights.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
            )],
        )),
        _ => {}
    }
    if l.remote_code() && sel.kind == ArtifactKind::Diffusers {
        f.push(found(codes::CUSTOM_CODE_UNMODELLED, Some("custom_pipeline".into()), vec!["a custom diffusers pipeline".into()]));
    }
    f
}

/// The preflight's blockers of one stage as found codes, by the one mapping table.
fn mapped(r: &Report, stage: Stage) -> Vec<(Gate, Found)> {
    let v = match stage {
        Stage::Convert => &r.verdict.convert,
        Stage::Register => &r.verdict.register,
        Stage::Mine => &r.verdict.mine,
    };
    v.blockers
        .iter()
        .map(|b| {
            let (gate, code) = codes::gate_code_of_preflight(&b.code).unwrap_or((
                match stage {
                    Stage::Convert => Gate::Lower,
                    Stage::Register => Gate::Admit,
                    Stage::Mine => Gate::Seat,
                },
                codes::UNMAPPED_PREFLIGHT_CODE,
            ));
            let mut ev = vec![format!("preflight {}: {}", b.code, b.what)];
            ev.extend(b.evidence.iter().take(3).cloned());
            if let (Some(h), Some(n)) = (b.have, b.need) {
                ev.push(format!("have {h} need {n} {}", b.unit.clone().unwrap_or_default()));
            }
            (gate, found(code, b.arg.clone().or_else(|| (code == codes::UNMAPPED_PREFLIGHT_CODE).then(|| b.code.clone())), ev))
        })
        .collect()
}

fn summary(r: &Report) -> PreflightSummaryV1 {
    let m = r.model.as_ref();
    PreflightSummaryV1 {
        depth_reached: r.depth.reached.name().to_string(),
        stopped_at: r.depth.stopped_at.clone(),
        level: m.map(|m| m.level.clone()),
        model_type: m.and_then(|m| m.model_type.clone()),
        architectures: m.map(|m| m.architectures.clone()).unwrap_or_default(),
        spec_digest: m.and_then(|m| m.spec_digest.clone()),
        scope_task: r.scope.as_ref().map(|s| s.task.clone()),
        text_only: r.scope.as_ref().map(|s| s.text_only),
        features_missing: m.map(|m| m.missing.iter().map(|x| x.what.clone()).collect()).unwrap_or_default(),
        blockers: r
            .blockers()
            .iter()
            .map(|b| format!("{}:{}{}", b.stage.name(), b.code, b.arg.as_ref().map(|a| format!("({a})")).unwrap_or_default()))
            .collect(),
        max_context: r.admission.as_ref().and_then(|a| a.layout.as_ref()).map(|l| l.max_context),
        artifact_bytes: r.artifact.as_ref().map(|a| a.estimate_bytes),
        download_bytes_needed: r.artifact.as_ref().and_then(|a| a.download_bytes_needed),
        notes: r.notes.iter().take(6).cloned().collect(),
    }
}

/// The diffusers route of a pipeline or a root component, from its configurations alone: every diffusers component's reading (a missing
/// feature is `FEATURE_C`), a component of a library that is neither diffusers nor transformers is custom code; when every denoiser and
/// decoder reads, the route lowers from the weights, which the census does not read.
fn diffusers_lower(
    comps: &std::collections::BTreeMap<String, (String, String, Option<serde_json::Value>)>,
) -> (Vec<Found>, Vec<String>) {
    let mut f = Vec::new();
    let mut ev = Vec::new();
    for (name, (lib, class, cfg)) in comps {
        match lib.as_str() {
            "diffusers" => {
                if name == "scheduler" || class.ends_with("Scheduler") {
                    continue;
                }
                let Some(cfg) = cfg else {
                    f.push(found(codes::CONFIG_MISSING, Some(name.clone()), vec![format!("{name}: no configuration ({class})")]));
                    continue;
                };
                match misaka_palw_tir_lower::hf_schema::read_diffusers(cfg) {
                    Ok(r) => ev.push(format!("{name}: {class} reads by {}", r.route.id())),
                    Err(e) => match e.missing.first() {
                        Some(m) => {
                            f.push(found(codes::FEATURE_C, Some(m.what.clone()), vec![format!("{name} ({class}): {}", e.error)]))
                        }
                        None if !e.unmapped_config_keys.is_empty() => f.push(found(
                            "CONFIG_KEY_UNREAD",
                            e.unmapped_config_keys.first().cloned(),
                            vec![format!("{name} ({class}): {}", e.error)],
                        )),
                        None => f.push(found("ARCH_REFUSED", Some(class.clone()), vec![format!("{name}: {}", e.error)])),
                    },
                }
            }
            "transformers" => ev.push(format!("{name}: {class} (an encoder stage; read by the encoder route with its weights)")),
            other => f.push(found(
                codes::CUSTOM_CODE_UNMODELLED,
                Some(format!("{other}.{class}")),
                vec![format!("{name}: library `{other}`")],
            )),
        }
    }
    (f, ev)
}

/// Evaluate gates in order: a gate after a failed one is `NOT_RUN_AFTER_<GATE>`.
struct Chain {
    out: Vec<GateResultV1>,
    failed: Option<Gate>,
}

impl Chain {
    fn new() -> Chain {
        Chain { out: Vec::new(), failed: None }
    }
    fn push(&mut self, gate: Gate, eval: impl FnOnce() -> GateResultV1) {
        let r = match self.failed {
            Some(g) => not_run(gate, &g.not_run_after(), vec![]),
            None => eval(),
        };
        if r.status == GateStatus::Fail && self.failed.is_none() {
            self.failed = Some(gate);
        }
        self.out.push(r);
    }
}

fn stopped_at(gates: &[GateResultV1]) -> String {
    gates.iter().find(|g| g.status != GateStatus::Pass).map(|g| g.gate.name().to_string()).unwrap_or_else(|| "none".into())
}

/// **The row of one repository**: the listing alone (`fetched` = `None`), or the listing and its header store.
pub fn evaluate(l: &ListingV1, fetched: Option<&Fetched>, ctx: &CensusContext) -> CensusRowV1 {
    let task = task_of(l);
    let sel = select(l);
    let strata = strata_of(l, &task, &sel);
    let rights = rights_of(l, ctx.policy);
    let depth = if fetched.is_some() { "headers" } else { "listing" };

    // ---- source (technical) -------------------------------------------------------------------------------------------------------
    let mut src_found = source_listing(l, &sel);
    if let Some(fx) = fetched {
        let info = &fx.fetch.info;
        if info.status != "ok" {
            let e = info.error.clone().unwrap_or_else(|| "error".into());
            let code = if e == "gated" { codes::GATED_ACCESS } else { codes::REPO_UNREACHABLE };
            src_found.push(found(code, Some(e.clone()), vec![info.detail.clone().unwrap_or_default()]));
        }
        for it in fx.fetch.items.iter().filter(|i| i.status != "ok") {
            let p = store::item_problem(it);
            src_found.push(found(p.code, Some(p.path), vec![p.detail]));
        }
    }

    // ---- the preflight (fetched repositories whose artifact it reads, and whose lower gate the listing has not already decided) ----
    let lower_found_listing = lower_listing(l, &task, &sel);
    let mut report: Option<Report> = None;
    let mut store_problems: Vec<Found> = Vec::new();
    let mut lower_extra: Vec<Found> = Vec::new();
    let mut lower_evidence: Vec<String> = Vec::new();
    let mut route_needs_weights = false;
    if let Some(fx) = fetched
        && src_found.is_empty()
    {
        match sel.kind {
            ArtifactKind::Safetensors | ArtifactKind::SafetensorsOther | ArtifactKind::Gguf
                if lower_found_listing.is_empty() || task.profile == Profile::PartialTextStage =>
            {
                match store::source_of(l, &sel, fx) {
                    Ok(cs) => match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        preflight::run_census_source(&cs.source, &cs.label, cs.bytes_read, &ctx.options)
                    })) {
                        Ok(Ok(r)) => report = Some(r),
                        Ok(Err(e)) => lower_extra.push(found("ARCH_REFUSED", None, vec![format!("the preflight could not run: {e}")])),
                        Err(p) => {
                            let why = p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()));
                            lower_extra.push(found(
                                "PREFLIGHT_PANIC",
                                None,
                                vec![format!("the preflight panicked: {}", why.unwrap_or_default())],
                            ))
                        }
                    },
                    Err(ps) => {
                        for p in ps {
                            store_problems.push(found(p.code, Some(p.path), vec![p.detail]));
                        }
                    }
                }
            }
            ArtifactKind::Diffusers | ArtifactKind::DiffusersComponent if lower_found_listing.is_empty() => {
                let comps = if sel.kind == ArtifactKind::Diffusers {
                    store::diffusers_components(fx)
                } else {
                    // A root component: its own configuration.
                    match fx.fetch.item("config.json").filter(|i| i.status == "ok").and_then(|i| fx.bytes_of(i).ok()) {
                        Some(b) => match serde_json::from_slice::<serde_json::Value>(&b) {
                            Ok(v) => {
                                let class = v.get("_class_name").and_then(|c| c.as_str()).unwrap_or("?").to_string();
                                Ok([("component".to_string(), ("diffusers".to_string(), class, Some(v)))].into_iter().collect())
                            }
                            Err(e) => {
                                Err(store::Problem { code: codes::HEADER_INVALID, path: "config.json".into(), detail: e.to_string() })
                            }
                        },
                        None => {
                            Err(store::Problem { code: codes::FETCH_FAILED, path: "config.json".into(), detail: "not fetched".into() })
                        }
                    }
                };
                match comps {
                    Ok(c) => {
                        let (f, ev) = diffusers_lower(&c);
                        lower_extra.extend(f);
                        lower_evidence.extend(ev);
                        route_needs_weights = true;
                    }
                    Err(p) => store_problems.push(found(p.code, Some(p.path), vec![p.detail])),
                }
            }
            _ => {}
        }
    }
    src_found.extend(store_problems);

    // ---- the technical chain --------------------------------------------------------------------------------------------------------
    let mut ch = Chain::new();
    ch.push(Gate::Source, || {
        if src_found.is_empty() { pass(Gate::Source, depth, vec![]) } else { fail(Gate::Source, src_found.clone(), depth) }
    });
    let rep = report.as_ref();
    ch.push(Gate::Lower, || {
        let mut f = lower_found_listing.clone();
        f.extend(lower_extra.clone());
        if let Some(r) = rep {
            for (gate, x) in mapped(r, Stage::Convert) {
                if gate == Gate::Lower {
                    f.push(x);
                }
            }
            // The class must compute the declared task: a decoder-only checkpoint tagged as an embedding model computes logits.
            if let Some(sc) = &r.scope {
                let class_task = sc.task.as_str();
                let wanted = match task.profile {
                    Profile::TextDecoder => Some("text-generation"),
                    Profile::GenEmbedding => Some("text-embedding"),
                    _ => None,
                };
                let routed = r.notes.iter().any(|n| n.contains("RFC-0003 program"));
                if let Some(w) = wanted
                    && !routed
                    && r.verdict.convert.status == StageStatus::Ok
                    && class_task != w
                {
                    f.push(found(
                        "TASK_MISMATCH",
                        Some(format!("{}→{class_task}", task.task)),
                        vec![format!("the declared task is `{}`; the class computes `{class_task}`", task.task)],
                    ));
                }
            }
        }
        if !f.is_empty() {
            return fail(Gate::Lower, f, depth);
        }
        if fetched.is_none() {
            return not_run(Gate::Lower, codes::NOT_RUN_NOT_SAMPLED, vec![]);
        }
        if route_needs_weights {
            return not_run(Gate::Lower, codes::NOT_RUN_NEEDS_WEIGHTS, lower_evidence.clone());
        }
        match rep {
            Some(r) if r.verdict.convert.status == StageStatus::Ok => {
                let mut ev = vec![format!(
                    "level {}, {}",
                    r.model.as_ref().map(|m| m.level.as_str()).unwrap_or("?"),
                    r.scope.as_ref().map(|s| s.headline()).unwrap_or_default()
                )];
                ev.extend(r.notes.iter().filter(|n| n.contains("RFC-0003") || n.contains("text stage only")).take(2).cloned());
                pass(Gate::Lower, "headers", ev)
            }
            Some(r) => {
                not_run(Gate::Lower, "NOT_RUN_PREFLIGHT_UNKNOWN", vec![r.verdict.convert.unknown_because.clone().unwrap_or_default()])
            }
            None => not_run(Gate::Lower, "NOT_RUN_NO_PREFLIGHT", vec![]),
        }
    });
    ch.push(Gate::Pack, || not_run(Gate::Pack, codes::NOT_RUN_NEEDS_WEIGHTS, vec![]));
    let routed = rep.is_some_and(|r| r.notes.iter().any(|n| n.contains("RFC-0003 program")));
    ch.push(Gate::Admit, || {
        if task.profile.is_pipeline() || routed {
            return not_run(
                Gate::Admit,
                codes::NOT_RUN_PIPELINE_ADMISSION,
                vec![format!("fence {}", task.profile.fence().unwrap_or("-"))],
            );
        }
        let Some(r) = rep else {
            return not_run(Gate::Admit, if fetched.is_none() { codes::NOT_RUN_NOT_SAMPLED } else { "NOT_RUN_NO_PREFLIGHT" }, vec![]);
        };
        let f: Vec<Found> = mapped(r, Stage::Register).into_iter().filter(|(g, _)| *g == Gate::Admit).map(|(_, x)| x).collect();
        if !f.is_empty() {
            return fail(Gate::Admit, f, "shape");
        }
        match r.verdict.register.status {
            StageStatus::Ok => pass(
                Gate::Admit,
                "shape",
                vec![format!(
                    "{} at DAA {}; declared context {}",
                    r.network.as_ref().map(|n| n.id.as_str()).unwrap_or("?"),
                    r.network.as_ref().map(|n| n.daa.to_string()).unwrap_or_else(|| "?".into()),
                    r.admission
                        .as_ref()
                        .and_then(|a| a.layout.as_ref())
                        .map(|l| l.max_context.to_string())
                        .unwrap_or_else(|| "?".into())
                )],
            ),
            _ => {
                not_run(Gate::Admit, "NOT_RUN_PREFLIGHT_UNKNOWN", vec![r.verdict.register.unknown_because.clone().unwrap_or_default()])
            }
        }
    });
    ch.push(Gate::Seat, || {
        if let Some(r) = rep {
            let f: Vec<Found> = mapped(r, Stage::Mine).into_iter().filter(|(g, _)| *g == Gate::Seat).map(|(_, x)| x).collect();
            if !f.is_empty() {
                return fail(Gate::Seat, f, "shape");
            }
        }
        not_run(Gate::Seat, codes::NOT_RUN_NEEDS_CHAIN, vec![])
    });
    ch.push(Gate::Final, || not_run(Gate::Final, codes::NOT_RUN_NEEDS_CHAIN, vec![]));
    let technical = ch.out;

    // ---- the strict view: the rights policy on top ----------------------------------------------------------------------------------
    let gates = if rights.confirmed {
        technical.clone()
    } else {
        let mut g = technical.clone();
        let src = &mut g[0];
        match src.status {
            GateStatus::Fail => src.codes.push(codes::RIGHTS_UNCONFIRMED.into()),
            _ => {
                *src = GateResultV1 {
                    gate: Gate::Source,
                    status: GateStatus::Fail,
                    blocking: Some(codes::RIGHTS_UNCONFIRMED.into()),
                    arg: Some(rights.policy.clone()),
                    codes: vec![codes::RIGHTS_UNCONFIRMED.into()],
                    evidence: vec![rights.why.clone()],
                    depth: Some(depth.into()),
                };
                for x in g.iter_mut().skip(1) {
                    *x = not_run(x.gate, &Gate::Source.not_run_after(), vec![]);
                }
            }
        }
        g
    };

    let is_pass = |v: &[GateResultV1], gate: Gate| v.iter().any(|x| x.gate == gate && x.status == GateStatus::Pass);
    let shape_ready = is_pass(&technical, Gate::Source) && is_pass(&technical, Gate::Lower) && is_pass(&technical, Gate::Admit);
    let registration_ready = [Gate::Source, Gate::Lower, Gate::Pack, Gate::Admit].iter().all(|g| is_pass(&gates, *g));
    let weights: Vec<String> = match (&sel.kind, fetched) {
        (ArtifactKind::Safetensors, Some(fx)) if sel.index.is_some() => {
            // The shards the index named, as the store found them.
            let mut v: Vec<String> = fx.fetch.items.iter().filter(|i| i.kind == "st_header").map(|i| i.path.clone()).collect();
            v.sort();
            v
        }
        _ => sel.weights.clone(),
    };
    let (weights_identity, weights_bytes) = match fetched {
        Some(fx) => {
            (fx.weights_identity(&weights), Some(weights.iter().filter_map(|w| fx.fetch.size_of(w)).sum::<u64>()).filter(|b| *b > 0))
        }
        None => (None, None),
    };
    CensusRowV1 {
        schema: ROW_SCHEMA_V1,
        snapshot: ctx.snapshot.clone(),
        repo: l.id.clone(),
        revision: fetched.map(|f| f.fetch.revision.clone()).or_else(|| l.sha.clone()),
        object_id: l.oid.clone(),
        depth: depth.into(),
        stopped_at: stopped_at(&gates),
        technical_stopped_at: stopped_at(&technical),
        task,
        strata,
        selected: sel,
        downloads: l.downloads,
        downloads_all: l.downloads_all,
        likes: l.likes,
        params: l.params(),
        rights,
        gates,
        technical,
        shape_ready,
        registration_ready,
        preflight: report.as_ref().map(summary),
        weights_identity,
        weights_bytes,
        ruleset: ctx.ruleset.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> CensusContext {
        CensusContext::new("test", "testnet-12", None, RightsPolicy::None, "test")
    }

    fn listing(tag: Option<&str>, siblings: &[&str]) -> ListingV1 {
        ListingV1 {
            id: "o/n".into(),
            oid: "0".into(),
            sha: Some("a".repeat(40)),
            pipeline_tag: tag.map(str::to_string),
            gated: serde_json::Value::Bool(false),
            siblings: siblings.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    fn gate(v: &[GateResultV1], g: Gate) -> &GateResultV1 {
        v.iter().find(|x| x.gate == g).unwrap()
    }

    #[test]
    fn a_gated_repository_fails_source_and_every_later_gate_is_not_run_after_it() {
        let mut l = listing(Some("text-generation"), &["config.json", "model.safetensors"]);
        l.gated = serde_json::Value::String("manual".into());
        let r = evaluate(&l, None, &ctx());
        let s = gate(&r.technical, Gate::Source);
        assert_eq!((s.status, s.blocking.as_deref()), (GateStatus::Fail, Some(codes::GATED_ACCESS)));
        for g in [Gate::Lower, Gate::Pack, Gate::Admit, Gate::Seat, Gate::Final] {
            assert_eq!(gate(&r.technical, g).blocking.as_deref(), Some("NOT_RUN_AFTER_SOURCE"), "{g:?}");
            assert_eq!(gate(&r.technical, g).status, GateStatus::NotRun);
        }
        // The strict view keeps the technical code first and lists the rights code too.
        let s = gate(&r.gates, Gate::Source);
        assert_eq!(s.blocking.as_deref(), Some(codes::GATED_ACCESS));
        assert!(s.codes.contains(&codes::RIGHTS_UNCONFIRMED.to_string()));
        assert!(!r.shape_ready && !r.registration_ready);
        assert_eq!(r.stopped_at, "source");
    }

    #[test]
    fn a_task_without_a_profile_fails_lower_from_the_listing_alone() {
        let l = listing(Some("image-classification"), &["config.json", "model.safetensors"]);
        let r = evaluate(&l, None, &ctx());
        assert_eq!(gate(&r.technical, Gate::Source).status, GateStatus::Pass);
        let lo = gate(&r.technical, Gate::Lower);
        assert_eq!((lo.blocking.as_deref(), lo.arg.as_deref()), (Some(codes::MODALITY_PROFILE_MISSING), Some("image-classification")));
        assert_eq!(gate(&r.technical, Gate::Admit).blocking.as_deref(), Some("NOT_RUN_AFTER_LOWER"));
        // Strict: the rights policy `none` confirms nothing, so the strict view stops at source.
        assert_eq!(gate(&r.gates, Gate::Source).blocking.as_deref(), Some(codes::RIGHTS_UNCONFIRMED));
        assert_eq!(gate(&r.gates, Gate::Lower).blocking.as_deref(), Some("NOT_RUN_AFTER_SOURCE"));
        assert_eq!((r.stopped_at.as_str(), r.technical_stopped_at.as_str()), ("source", "lower"));
    }

    #[test]
    fn an_unsampled_text_model_is_not_run_never_passed() {
        let l = listing(Some("text-generation"), &["config.json", "model.safetensors"]);
        let r = evaluate(&l, None, &ctx());
        let lo = gate(&r.technical, Gate::Lower);
        assert_eq!((lo.status, lo.blocking.as_deref()), (GateStatus::NotRun, Some(codes::NOT_RUN_NOT_SAMPLED)));
        assert_eq!(gate(&r.technical, Gate::Pack).blocking.as_deref(), Some(codes::NOT_RUN_NEEDS_WEIGHTS));
        assert_eq!(gate(&r.technical, Gate::Admit).blocking.as_deref(), Some(codes::NOT_RUN_NOT_SAMPLED));
        assert!(r.technical.iter().all(|g| g.gate == Gate::Source || g.status != GateStatus::Pass));
        assert!(!r.shape_ready);
    }

    #[test]
    fn the_format_the_adapter_and_the_missing_weights_are_named() {
        let r = evaluate(&listing(Some("text-generation"), &["config.json", "pytorch_model.bin"]), None, &ctx());
        assert_eq!(gate(&r.technical, Gate::Lower).blocking.as_deref(), Some(codes::FORMAT_UNSUPPORTED));
        assert_eq!(gate(&r.technical, Gate::Lower).arg.as_deref(), Some("pytorch"));
        let r = evaluate(&listing(Some("text-generation"), &["README.md"]), None, &ctx());
        assert_eq!(gate(&r.technical, Gate::Source).blocking.as_deref(), Some(codes::MISSING_WEIGHTS));
        let mut l = listing(None, &["adapter_config.json", "adapter_model.safetensors"]);
        l.config = serde_json::json!({"peft": {"base_model_name_or_path": "b/base", "task_type": "CAUSAL_LM"}});
        l.base_resolved = vec![super::super::listing::BaseResolvedV1 {
            id: "b/base".into(),
            found: true,
            sha: Some("b".repeat(40)),
            ..Default::default()
        }];
        let r = evaluate(&l, None, &ctx());
        assert_eq!(r.task.task, "text-generation");
        assert_eq!(gate(&r.technical, Gate::Source).status, GateStatus::Pass);
        assert_eq!(gate(&r.technical, Gate::Lower).blocking.as_deref(), Some(codes::ADAPTER_UNCHECKED));
        l.base_resolved.clear();
        let r = evaluate(&l, None, &ctx());
        assert_eq!(gate(&r.technical, Gate::Source).blocking.as_deref(), Some(codes::BASE_UNPINNED));
    }

    #[test]
    fn a_shard_set_with_a_part_missing_from_the_listing_is_incomplete() {
        let l = listing(
            Some("text-generation"),
            &["config.json", "model.safetensors.index.json", "model-00001-of-00003.safetensors", "model-00003-of-00003.safetensors"],
        );
        let r = evaluate(&l, None, &ctx());
        assert_eq!(gate(&r.technical, Gate::Source).blocking.as_deref(), Some(codes::WEIGHTS_INCOMPLETE));
    }

    #[test]
    fn a_vlm_task_is_a_partial_task() {
        let r = evaluate(&listing(Some("image-text-to-text"), &["config.json", "model.safetensors"]), None, &ctx());
        assert_eq!(gate(&r.technical, Gate::Lower).blocking.as_deref(), Some(codes::PARTIAL_TASK_ONLY));
    }
}
