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
use super::onboarding::{GapClassV1, classify_census_code_v1};
use super::listing::{ArtifactKind, ListingV1, SelectedV1, StrataV1, TaskV1, select, strata_of, task_of, torch_checkpoint_only};
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
    /// **Who has to change something** (`FRONTEND_REQUIRED`, `KERNEL_EXTENSION_REQUIRED`, `LAYOUT_REQUIRED`, `RESOURCE_REFUSED`, …,
    /// or `NOT_RUN` for a gate that was not run): [`super::onboarding`]. Absent on a PASS.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<GapClassV1>,
    /// The reason text the lifecycle record of this failure carries (`"task profile"` for a missing job profile, which is recorded
    /// as `KERNEL_EXTENSION_REQUIRED`); absent otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class_reason: Option<&'static str>,
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
    let class = classify_census_code_v1(gate, &first.code, first.arg.as_deref(), &first.evidence);
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
        class: Some(class),
        class_reason: class.onboarding_reason(),
    }
}

fn pass(gate: Gate, depth: &str, evidence: Vec<String>) -> GateResultV1 {
    GateResultV1 { gate, status: GateStatus::Pass, blocking: None, arg: None, codes: vec![], evidence, depth: Some(depth.into()), class: None, class_reason: None }
}

fn not_run(gate: Gate, why: &str, evidence: Vec<String>) -> GateResultV1 {
    GateResultV1 {
        gate,
        status: GateStatus::NotRun,
        blocking: Some(why.into()),
        arg: None,
        codes: vec![],
        evidence,
        depth: None,
        class: Some(GapClassV1::NotRun),
        class_reason: None,
    }
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
    /// RFC-0011 §16.4 (ADR-0172): the K2-TIR-v1 kernel's route — `shipped/hypothetical` codes, e.g.
    /// `KERNEL_NOT_ACTIVE/ELIGIBLE_AT` — and the bucket of the shipped outcome. Reported, never counted as coverage.
    pub kernel_route: Option<String>,
    pub kernel_bucket: Option<String>,
}

/// The ruleset a row was judged against.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RulesetV1 {
    pub network: String,
    pub height: Option<u64>,
    /// The rule that chose each class's declared context ([`ContextRule`]).
    pub context_rule: String,
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
    /// The context the class was declared at, and where it came from; the retry's, when there was one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<ContextV1>,
    /// The admit gate at the retry context, when the primary context failed only on context-dependent codes. Never merged into
    /// `technical`: a class admissible only at the retry context is reported as its own stratum.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub admit_retry: Option<GateResultV1>,
    /// `source` and `lower` PASS and the admit gate PASS at the retry context only.
    pub shape_ready_at_retry: bool,
    /// The content identity of the selected weights (the inventory's LFS ids).
    pub weights_identity: Option<String>,
    pub weights_bytes: Option<u64>,
    /// **A chat model's other stages under the dormant RFC-0003 fences** (`palw_gen_v1`'s `Text` profile with an image slot, FP Job
    /// V5's `palw_fp_job_v5`): the vision tower of `vision_config`, read, lowered shape-only and admitted stage-alone
    /// (`probe_data_route`, `tir_admit_program_v2` at default inputs). Not the pipeline admission and not the job lane, which are
    /// dormant: a measurement of what the fences would have to carry, never counted as shape-ready.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_stage_probe: Option<serde_json::Value>,
    pub ruleset: RulesetV1,
}

/// How a class's declared context is chosen. A class is a model at a context, so a census declares one per repository.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextRule {
    /// Every class at this context.
    Fixed(u32),
    /// The model's declared maximum positions, capped (testnet-12's classes are 8k); when that fails only on context-dependent codes
    /// (`CONTEXT_BOUND`, `COURT_BUDGET`, `COURT_WINDOW_EXCEEDED`, `CLOSE_TOO_LARGE`, `DA_LADDER_EXCEEDED`), one retry at `retry`, recorded
    /// apart (the lead's rule of 2026-10-03).
    ModelCapped { cap: u32, retry: u32 },
}

impl ContextRule {
    pub fn describe(self) -> String {
        match self {
            ContextRule::Fixed(c) => format!("fixed {c}"),
            ContextRule::ModelCapped { cap, retry } => format!(
                "primary min(declared max positions, {cap}); judged at {retry} first, the primary only when {retry} admits (or refuses on a code a wider context could change); a class admitted at {retry} only is its own stratum"
            ),
        }
    }
}

/// The declared context of a row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ContextV1 {
    pub primary: u32,
    /// `config.<key>` the declared maximum came from, or `assumed` (none declared: the cap).
    pub source: String,
    pub declared: Option<u64>,
    pub retry: Option<u32>,
    /// The contexts the preflight actually ran at, in order.
    pub judged_at: Vec<u32>,
    /// The primary context was not run: the narrower context's refusal holds there (its codes are limits that only grow with the
    /// context, or do not depend on it). The admit gate is FAIL with the narrower context's codes, and says so.
    pub primary_implied: bool,
}

/// Codes a narrower declared context can change: limits that only grow with the context (close sizes, the court's costs and window,
/// the DA ladder's leaves, the canonical job's bounds within 16..=32,783 positions).
pub const CONTEXT_DEPENDENT_CODES: &[&str] =
    &[codes::CONTEXT_BOUND, codes::COURT_BUDGET, "COURT_WINDOW_EXCEEDED", codes::CLOSE_TOO_LARGE, "DA_LADDER_EXCEEDED"];

/// Codes no declared context changes (a fence not in force, a root already registered).
pub const CONTEXT_INDEPENDENT_CODES: &[&str] = &["FENCE_NOT_ARMED", "ARTIFACT_ROOT_KNOWN"];

/// **The refusals at the narrower context that imply the primary's**, each with the argument (the lead's guard of 2026-10-03: a code
/// whose limit could move non-monotonically with the tiling is not implied, its primary is run):
///
/// * `COURT_BUDGET` — the court's per-tile MACs and cone work depend on the tiles, and the layout search offers the same power-of-two
///   tiles at every context of at least the tile; the dissection's root claim and its rounds only grow with the history.
/// * `COURT_WINDOW_EXCEEDED` — the dissection's duration is non-decreasing in the context (more history rounds, never fewer).
/// * `CLOSE_TOO_LARGE` — a carried close spans a tile and the history it reads; neither shrinks when the context grows.
/// * `DA_LADDER_EXCEEDED` — the longest job's step leaves are proportional to its positions.
/// * `FENCE_NOT_ARMED`, `ARTIFACT_ROOT_KNOWN` — do not depend on the context.
///
/// `CONTEXT_BOUND` (the canonical job's bounds) is **not** implied: both 2,048 and 8,192 lie inside its 16..=32,783 bounds, so a
/// refusal at 2,048 is not explained by the context's size and the primary is judged on its own. Implied rows are checked, not
/// assumed: a seeded subset is judged at the primary for real (`tools/hf_census/verify_implied.py`).
pub const IMPLIED_AT_WIDER_CONTEXT: &[&str] = &[
    codes::COURT_BUDGET,
    "COURT_WINDOW_EXCEEDED",
    codes::CLOSE_TOO_LARGE,
    "DA_LADDER_EXCEEDED",
    "FENCE_NOT_ARMED",
    "ARTIFACT_ROOT_KNOWN",
];

/// The model's declared maximum positions, from its configuration (the text decoder's, when nested).
pub fn declared_positions(config: &serde_json::Value) -> Option<(u64, String)> {
    const KEYS: &[&str] = &["max_position_embeddings", "n_positions", "max_seq_len", "seq_length", "max_sequence_length", "n_ctx"];
    for scope in
        [Some(config), config.get("text_config"), config.get("llm_config"), config.get("language_config")].into_iter().flatten()
    {
        for k in KEYS {
            if let Some(v) = scope.get(*k).and_then(|v| v.as_u64()).filter(|v| *v > 0) {
                let at = if std::ptr::eq(scope, config) { format!("config.{k}") } else { format!("config.<nested>.{k}") };
                return Some((v, at));
            }
        }
    }
    None
}

/// The census's fixed inputs.
pub struct CensusContext {
    pub snapshot: String,
    pub policy: RightsPolicy,
    pub options: preflight::Options,
    pub ruleset: RulesetV1,
    pub context_rule: ContextRule,
    /// The chain's judgment shared by identical programs (one process, every worker).
    pub cache: preflight::JudgeCache,
    /// A wall-clock budget for one class's layout search at one context (`None`: none). A search that spends it leaves the admit gate
    /// `NOT_RUN_JUDGMENT_BUDGET` — counted as not passing, never as a verdict.
    pub judge_budget: Option<std::time::Duration>,
    /// A frame's task for a repository that declares none (RFC-0002 §II.12: a GGUF the Hub lists without a task is in the local-LLM
    /// frame as text generation). `None` in the Hub-wide census, where an undeclared task is `TASK_UNKNOWN`.
    pub assume_task: Option<String>,
}

impl CensusContext {
    /// The preflight options of a census: the shape depth on `network`, at `height` (`None`: the last scheduled fence height, as
    /// the preflight chooses it), the reference seat tiers.
    pub fn new(snapshot: &str, network: &str, height: Option<u64>, policy: RightsPolicy, tree: &str) -> CensusContext {
        let options =
            preflight::Options { depth: preflight::Depth::Shape, network: Some(network.to_string()), height, ..Default::default() };
        let context_rule = ContextRule::ModelCapped { cap: 8_192, retry: 2_048 };
        CensusContext {
            snapshot: snapshot.to_string(),
            policy,
            options,
            ruleset: RulesetV1 {
                network: network.to_string(),
                height,
                context_rule: context_rule.describe(),
                tree: tree.to_string(),
                tasks: super::tasks::tasks_digest(),
            },
            context_rule,
            cache: Default::default(),
            judge_budget: None,
            assume_task: None,
        }
    }

    pub fn with_context_rule(mut self, rule: ContextRule) -> CensusContext {
        self.context_rule = rule;
        self.ruleset.context_rule = rule.describe();
        self
    }
}

/// The preflight at one declared context, a panic caught and named.
fn run_preflight(cs: &store::CensusSource, ctx: &CensusContext, max_context: u32) -> Result<Report, Found> {
    let mut opts = ctx.options.clone();
    opts.max_context = Some(max_context);
    let deadline = ctx.judge_budget.map(|b| std::time::Instant::now() + b);
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::tir_layout::tir_with_search_deadline_v1(deadline, || {
            preflight::run_census_source(&cs.source, &cs.label, cs.bytes_read, &opts, Some(&ctx.cache))
        })
    })) {
        Ok(Ok(r)) => Ok(r),
        Ok(Err(e)) => Err(found("ARCH_REFUSED", None, vec![format!("the preflight could not run: {e}")])),
        Err(p) => {
            let why = p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()));
            Err(found("PREFLIGHT_PANIC", None, vec![format!("the preflight panicked: {}", why.unwrap_or_default())]))
        }
    }
}

/// **The preflight of one source at the declared contexts** ([`ContextRule`]): the headers depth once; else the narrower context first
/// and the primary only when the narrower admits (or refuses on a code a wider context could change). Returns the report the row is
/// built from, the context record and the narrower context's admit gate.
fn judge_at_contexts(
    cs: &store::CensusSource,
    ctx: &CensusContext,
    task: &TaskV1,
) -> (Result<Report, Found>, Option<ContextV1>, Option<GateResultV1>) {
    let (primary, source, declared) = match ctx.context_rule {
        ContextRule::Fixed(c) => (c, "fixed".to_string(), None),
        ContextRule::ModelCapped { cap, .. } => match cs.source.config.as_ref().and_then(declared_positions) {
            Some((d, at)) => (d.min(u64::from(cap)) as u32, at, Some(d)),
            None => (cap, "assumed".to_string(), None),
        },
    };
    let mut cx = ContextV1 { primary, source, declared, retry: None, judged_at: Vec::new(), primary_implied: false };
    let mut admit_retry = None;
    let report = match ctx.context_rule {
        _ if ctx.options.depth == preflight::Depth::Headers => run_preflight(cs, ctx, primary),
        // The narrower context first (admission at 8,192 costs ~30x the CPU of 2,048): a class refused at the narrower context on
        // limits that only grow with the context is refused at the primary too, so the primary is judged only for a class the
        // narrower context admits (or refuses on a code a wider context could change).
        ContextRule::ModelCapped { retry, .. } if primary > retry && !task.profile.is_pipeline() => {
            cx.retry = Some(retry);
            cx.judged_at.push(retry);
            match run_preflight(cs, ctx, retry) {
                Ok(r2) if r2.verdict.convert.status != StageStatus::Ok => Ok(r2),
                Ok(r2) => {
                    let a2 = admit_of(&r2);
                    let implied =
                        a2.status == GateStatus::Fail && a2.codes.iter().all(|c| IMPLIED_AT_WIDER_CONTEXT.contains(&c.as_str()));
                    let spent = a2.blocking.as_deref() == Some(codes::NOT_RUN_JUDGMENT_BUDGET);
                    admit_retry = Some(a2);
                    if spent {
                        // The narrower context's search spent the budget: the wider one costs more, so it is not run either.
                        Ok(r2)
                    } else if implied {
                        cx.primary_implied = true;
                        Ok(r2)
                    } else {
                        cx.judged_at.push(primary);
                        run_preflight(cs, ctx, primary)
                    }
                }
                Err(f) => Err(f),
            }
        }
        _ => {
            cx.judged_at.push(primary);
            run_preflight(cs, ctx, primary)
        }
    };
    (report, Some(cx), admit_retry)
}

/// The admit gate of a report (the class is not a pipeline class).
fn admit_of(r: &Report) -> GateResultV1 {
    let spent = r.blockers().iter().any(|b| b.evidence.iter().any(|e| e.contains(crate::tir_layout::TIR_SEARCH_BUDGET_SPENT_V1)));
    if spent {
        return not_run(Gate::Admit, codes::NOT_RUN_JUDGMENT_BUDGET, vec![crate::tir_layout::TIR_SEARCH_BUDGET_SPENT_V1.into()]);
    }
    if r.depth.reached == preflight::Depth::Headers {
        // A headers-depth census (the shape depth's admission deferred): not run, never inferred.
        return not_run(Gate::Admit, codes::NOT_RUN_DEPTH_HEADERS, vec![]);
    }
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
                r.admission.as_ref().and_then(|a| a.layout.as_ref()).map(|l| l.max_context.to_string()).unwrap_or_else(|| "?".into())
            )],
        ),
        _ => not_run(Gate::Admit, "NOT_RUN_PREFLIGHT_UNKNOWN", vec![r.verdict.register.unknown_because.clone().unwrap_or_default()]),
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
    if sel.kind == ArtifactKind::Adapter
        && let Err((arg, why)) = crate::census::listing::pinned_base(l)
    {
        f.push(found(codes::BASE_UNPINNED, Some(arg.into()), vec![why]));
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
        // A transformers `pytorch_model.bin` is read by the frontend (without running its pickle); the census has not read this
        // repository's zip directory, which the lower gate reports as not run (`needs_pickle`), never as an unsupported format.
        ArtifactKind::Other if torch_checkpoint_only(l, sel) => {}
        ArtifactKind::Other => f.push(found(
            codes::FORMAT_UNSUPPORTED,
            Some(sel.formats.join("+")),
            vec![format!("weights only as {}", sel.formats.join(", "))],
        )),
        // An adapter without a PEFT configuration (a diffusers LoRA file) is not composed by this build; a PEFT adapter is composed with
        // its pinned base when its headers are read (RFC-0004's LoRA attachment), so the listing does not decide it.
        ArtifactKind::Adapter if sel.config.is_none() => f.push(found(
            codes::ADAPTER_UNCHECKED,
            l.config.get("peft").and_then(|p| p.get("task_type")).and_then(|t| t.as_str()).map(str::to_string),
            vec!["an adapter with no adapter_config.json: the census composes PEFT adapters only".into()],
        )),
        ArtifactKind::Adapter if !sel.weights.iter().any(|w| w.ends_with(".safetensors")) => f.push(found(
            codes::FORMAT_UNSUPPORTED,
            Some("pytorch-adapter".into()),
            vec!["the adapter's weights are only a pickle (adapter_model.bin)".into()],
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
        kernel_route: r.kernel.as_ref().map(|k| format!("{}/{}", k.shipped, k.hypothetical)),
        kernel_bucket: r.kernel.as_ref().map(|k| k.bucket.clone()),
    }
}

/// The diffusers route of a pipeline or a root component, from its configurations alone: every diffusers component's reading (a missing
/// feature is `FEATURE_C`), a component of a library that is neither diffusers nor transformers is custom code; when every denoiser and
/// decoder reads, the route lowers from the weights, which the census does not read.
fn diffusers_lower(
    comps: &std::collections::BTreeMap<String, (String, String, Option<serde_json::Value>)>,
    inventory: &[store::FileV1],
) -> (Vec<Found>, Vec<String>) {
    let mut f = Vec::new();
    let mut ev = Vec::new();
    for (name, (lib, class, cfg)) in comps {
        // Post-filters and preprocessors are not the denoising computation the image job commits to.
        if matches!(name.as_str(), "safety_checker" | "feature_extractor" | "watermarker" | "image_processor")
            || name.starts_with("tokenizer")
        {
            continue;
        }
        // A library name that is neither diffusers nor transformers is custom code only when the repository ships it as Python (a
        // `<library>.py` file, or Python in the component's directory); otherwise it is a diffusers-internal module path
        // (`stable_diffusion`, …).
        let ships_python = inventory.iter().any(|x| {
            x.path.ends_with(".py")
                && (x.path.rsplit('/').next().and_then(|n| n.strip_suffix(".py")) == Some(lib.as_str())
                    || x.path.starts_with(&format!("{name}/")))
        });
        let lib = if lib != "diffusers" && lib != "transformers" && !ships_python { "diffusers" } else { lib.as_str() };
        match lib {
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
/// The class's declared image size for a tower whose configuration declares none (Qwen2-VL): a census convention, recorded.
pub const IMAGE_STAGE_PROBE_SIZE_V1: (u32, u32) = (448, 448);

/// **A chat model's vision tower, read from the whole wrapper configuration** (`parse_vision`: a tower adapter that claims the
/// wrapper — `match.tower_of` — or the tower's own type), lowered shape-only and admitted stage-alone at the default inputs.
pub fn image_stage_probe_v1(config: &serde_json::Value) -> serde_json::Value {
    use misaka_palw_tir_lower::lower::vision;
    let run = || -> Result<serde_json::Value, String> {
        let spec =
            vision::parse_vision(&config.to_string(), Some(IMAGE_STAGE_PROBE_SIZE_V1), None).map_err(|e| format!("read: {e}"))?;
        let (hl, _) = vision::hl_program(&spec).map_err(|e| format!("hl: {e}"))?;
        let lw = vision::lower_vision(&hl, &spec).map_err(|e| format!("lower: {e}"))?;
        let p2 = misaka_palw_tir_lower::encoder::vision_v2(&lw).map_err(|e| format!("v2: {e}"))?;
        // The stage's tile is the registrant's choice (`gen_declare_layout_v1`'s `tile_len`): the narrowest of these that the
        // stage-alone admission accepts, the widest's refusal otherwise.
        let mut last = String::new();
        for tile_len in [64u32, 512, 4096] {
            let inputs = misaka_palw_tir::admit::TirAdmitInputsV1 { tile_len, ..misaka_palw_tir_lower::admission::default_inputs() };
            match misaka_palw_tir::admit_v2::tir_admit_program_v2(&p2, &inputs) {
                Ok(a) => {
                    return Ok(serde_json::json!({"ok": true, "stage": "vision", "tile_len": tile_len,
                        "macs": a.view.position.cost.macs as f64, "size": [IMAGE_STAGE_PROBE_SIZE_V1.0, IMAGE_STAGE_PROBE_SIZE_V1.1]}));
                }
                Err(e) => last = format!("tir_admit_program_v2 refuses at tile {tile_len}: {e}"),
            }
        }
        Err(last)
    };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)) {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => serde_json::json!({"ok": false, "error": e.chars().take(300).collect::<String>()}),
        Err(_) => serde_json::json!({"ok": false, "error": "the probe panicked"}),
    }
}

pub fn evaluate(l: &ListingV1, fetched: Option<&Fetched>, ctx: &CensusContext) -> CensusRowV1 {
    let mut task = task_of(l);
    if task.task == "unknown"
        && let Some(t) = &ctx.assume_task
    {
        let r = super::tasks::task_row(t);
        task = TaskV1 { task: t.clone(), group: r.group.to_string(), profile: r.profile, source: "frame".into() };
    }
    let sel = select(l);
    // An adapter that declares no task carries its pinned base's (inference v3), where the base was read.
    if task.task == "unknown"
        && sel.kind == ArtifactKind::Adapter
        && let Some(t) = fetched.and_then(store::base_task_of)
    {
        let r = super::tasks::task_row(t);
        task = TaskV1 { task: t.to_string(), group: r.group.to_string(), profile: r.profile, source: "inferred:base-architecture".into() };
    }
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
    let mut needs_tensor_data: Option<String> = None;
    // A PyTorch checkpoint whose zip directory the census never fetched (its own, or its adapter's base).
    let mut needs_pickle: Option<String> = torch_checkpoint_only(l, &sel).then(|| "the repository's weights".to_string());
    let mut image_stage_probe: Option<serde_json::Value> = None;
    let mut context: Option<ContextV1> = None;
    let mut admit_retry: Option<GateResultV1> = None;
    if let Some(fx) = fetched
        && src_found.is_empty()
    {
        match sel.kind {
            ArtifactKind::Safetensors | ArtifactKind::SafetensorsOther | ArtifactKind::Gguf
                if lower_found_listing.is_empty() || task.profile == Profile::PartialTextStage =>
            {
                match store::source_of(l, &sel, fx) {
                    Ok(cs) if cs.needs_tensor_data.is_some() => {
                        needs_tensor_data = cs.needs_tensor_data.clone();
                    }
                    Ok(cs) => {
                        let probe_image = task.profile == Profile::PartialTextStage
                            && cs.source.config.as_ref().is_some_and(|c| c.get("vision_config").is_some_and(|v| v.is_object()));
                        let (r, cx, ar) = judge_at_contexts(&cs, ctx, &task);
                        if probe_image && let Some(cfg) = cs.source.config.as_ref() {
                            let primary = cx.as_ref().map(|c| c.primary).unwrap_or(2_048);
                            let mut v = match crate::preflight::chain::PreflightNetwork::parse(&ctx.ruleset.network) {
                                Ok(net) => {
                                    let height = ctx
                                        .options
                                        .height
                                        .unwrap_or_else(|| net.params.fence_schedule_v1().last().copied().unwrap_or(0));
                                    super::vlm::vlm_text_class_admission_v1(
                                        cfg,
                                        &net,
                                        height,
                                        primary,
                                        std::env::var_os("PALW_CENSUS_GEN_RANGE_TWIN").is_some(),
                                    )
                                }
                                Err(e) => serde_json::json!({"ok": false, "error": e}),
                            };
                            if let Ok(r) = &r {
                                let a = admit_of(r);
                                v["text_stage"] = serde_json::json!({"status": a.status, "blocking": a.blocking});
                            }
                            image_stage_probe = Some(v);
                        }
                        match r {
                            Ok(r) => report = Some(r),
                            Err(f) => lower_extra.push(f),
                        }
                        context = cx;
                        admit_retry = ar;
                    }
                    Err(ps) => {
                        for p in ps {
                            store_problems.push(found(p.code, Some(p.path), vec![p.detail]));
                        }
                    }
                }
            }
            ArtifactKind::Adapter if lower_found_listing.is_empty() => match store::adapter_source_of(l, &sel, fx) {
                Ok(None) => lower_extra.push(found(
                    codes::ADAPTER_UNCHECKED,
                    None,
                    vec!["the adapter's base was not read (no base pinned in the snapshot)".into()],
                )),
                Ok(Some((cs, lora))) => {
                    let ctx2 = CensusContext {
                        snapshot: ctx.snapshot.clone(),
                        policy: ctx.policy,
                        options: preflight::Options { lora: Some(lora), ..ctx.options.clone() },
                        ruleset: ctx.ruleset.clone(),
                        context_rule: ctx.context_rule,
                        cache: Default::default(),
                        judge_budget: ctx.judge_budget,
                        assume_task: ctx.assume_task.clone(),
                    };
                    let (r, cx, ar) = judge_at_contexts(&cs, &ctx2, &task);
                    match r {
                        Ok(r) => report = Some(r),
                        Err(f) => lower_extra.push(f),
                    }
                    context = cx;
                    admit_retry = ar;
                }
                Err(ps) => {
                    for p in ps {
                        if p.code == codes::NOT_RUN_NEEDS_PICKLE_DIRECTORY {
                            needs_pickle = Some(p.path);
                        } else if p.code == codes::FORMAT_UNSUPPORTED {
                            lower_extra.push(found(p.code, Some(p.path), vec![p.detail]));
                        } else {
                            store_problems.push(found(p.code, Some(p.path), vec![p.detail]));
                        }
                    }
                }
            },
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
                        let (f, ev) = diffusers_lower(&c, &fx.fetch.inventory);
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
        if let Some(what) = &needs_pickle {
            let mut r = not_run(
                Gate::Lower,
                codes::NOT_RUN_NEEDS_PICKLE_DIRECTORY,
                vec![format!(
                    "{what} is a PyTorch checkpoint (pytorch_model.bin): the frontend reads it without running its pickle, but the census reads safetensors headers only"
                )],
            );
            r.arg = Some("pytorch".into());
            return r;
        }
        if fetched.is_none() {
            return not_run(Gate::Lower, codes::NOT_RUN_NOT_SAMPLED, vec![]);
        }
        if route_needs_weights {
            return not_run(Gate::Lower, codes::NOT_RUN_NEEDS_WEIGHTS, lower_evidence.clone());
        }
        if let Some(t) = &needs_tensor_data {
            let mut r = not_run(
                Gate::Lower,
                codes::NOT_RUN_NEEDS_TENSOR_DATA,
                vec![format!("the frontend reads the data of {t}; the census reads headers only (the user's network policy)")],
            );
            r.arg = Some(t.clone());
            return r;
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
        let mut a = admit_of(r);
        if let Some(cx) = &context
            && cx.primary_implied
        {
            a.depth = Some(format!("shape@{}", cx.retry.unwrap_or(0)));
            a.evidence.insert(
                0,
                format!(
                    "refused at {} positions on limits that only grow with the context (or do not depend on it): refused at the primary {} too (not run there)",
                    cx.retry.unwrap_or(0),
                    cx.primary
                ),
            );
        }
        a
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
                    class: Some(GapClassV1::ExternalBlocker),
                    class_reason: None,
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
    let shape_ready_at_retry = is_pass(&technical, Gate::Source)
        && is_pass(&technical, Gate::Lower)
        && !shape_ready
        && admit_retry.as_ref().is_some_and(|a| a.status == GateStatus::Pass)
        && context.as_ref().is_some_and(|c| c.judged_at.contains(&c.primary));
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
        context,
        admit_retry,
        shape_ready_at_retry,
        weights_identity,
        weights_bytes,
        image_stage_probe,
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

    /// **Every gate of a row carries its onboarding class** (machine-readable, additive to the row): a failure its own class, a gate
    /// that was not run `NOT_RUN` (never a gap), a PASS none.
    #[test]
    fn every_gate_of_a_row_carries_who_has_to_change_something() {
        let r = evaluate(&listing(Some("image-classification"), &["config.json", "model.safetensors"]), None, &ctx());
        assert_eq!(gate(&r.technical, Gate::Source).class, None);
        assert_eq!(gate(&r.technical, Gate::Lower).class, Some(GapClassV1::ProfileRequired));
        assert_eq!(gate(&r.technical, Gate::Lower).class_reason, Some("task profile"));
        assert_eq!(serde_json::to_value(gate(&r.technical, Gate::Lower)).unwrap()["class_reason"], "task profile");
        for g in [Gate::Pack, Gate::Admit, Gate::Seat, Gate::Final] {
            let x = gate(&r.technical, g);
            assert_eq!((x.status, x.class), (GateStatus::NotRun, Some(GapClassV1::NotRun)), "{g:?}");
        }
        // The strict view's rights failure is the source's, not the importer's.
        assert_eq!(gate(&r.gates, Gate::Source).class, Some(GapClassV1::ExternalBlocker));
        // A format with no reader is the importer's; missing weights are the source's.
        let r = evaluate(&listing(Some("text-generation"), &["config.json", "model.onnx"]), None, &ctx());
        assert_eq!(gate(&r.technical, Gate::Lower).class, Some(GapClassV1::FrontendRequired));
        let r = evaluate(&listing(Some("text-generation"), &["README.md"]), None, &ctx());
        assert_eq!(gate(&r.technical, Gate::Source).class, Some(GapClassV1::ExternalBlocker));
        // An unsampled model's lower gate is NOT_RUN: not a verdict, not a gap.
        let r = evaluate(&listing(Some("text-generation"), &["config.json", "model.safetensors"]), None, &ctx());
        let lo = gate(&r.technical, Gate::Lower);
        assert_eq!(lo.class, Some(GapClassV1::NotRun));
        assert!(!lo.class.unwrap().is_semantic_gap());
        // The serialized row carries the token.
        let j = serde_json::to_value(lo).unwrap();
        assert_eq!(j["class"], "NOT_RUN");
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

    /// **A `pytorch_model.bin` is no longer "an unsupported format".** The frontend reads it without running its pickle
    /// (`weights::torchzip`); the census has fetched safetensors headers only, so the lower gate of such a repository is NOT_RUN
    /// (not a verdict, not a gap of any kind) with its own code — and a pytorch file that is not a transformers checkpoint, or a task
    /// with no profile, keeps its own, earlier answer.
    #[test]
    fn a_pytorch_checkpoint_is_not_run_never_an_unsupported_format() {
        for sib in [
            vec!["config.json", "pytorch_model.bin"],
            vec!["config.json", "pytorch_model.bin.index.json", "pytorch_model-00001-of-00002.bin", "pytorch_model-00002-of-00002.bin"],
            vec!["config.json", "pytorch_model-00001-of-00002.bin"],
        ] {
            let r = evaluate(&listing(Some("text-generation"), &sib), None, &ctx());
            let lo = gate(&r.technical, Gate::Lower);
            assert_eq!((lo.status, lo.blocking.as_deref(), lo.arg.as_deref()), (GateStatus::NotRun, Some(codes::NOT_RUN_NEEDS_PICKLE_DIRECTORY), Some("pytorch")), "{sib:?}");
            assert_eq!(lo.class, Some(GapClassV1::NotRun));
            assert!(!lo.class.unwrap().is_semantic_gap());
            assert!(!r.shape_ready, "unmeasured is not ready");
        }
        // Not a transformers checkpoint: a configuration beside `training_args.bin` alone is no weights; a bare `.pt` has no reader.
        let r = evaluate(&listing(Some("text-generation"), &["config.json", "model.pt"]), None, &ctx());
        assert_eq!(gate(&r.technical, Gate::Lower).blocking.as_deref(), Some(codes::FORMAT_UNSUPPORTED));
        // An earlier answer stands: a task with no profile is the profile's, whatever the weights are.
        let r = evaluate(&listing(Some("text-classification"), &["config.json", "pytorch_model.bin"]), None, &ctx());
        assert_eq!(gate(&r.technical, Gate::Lower).blocking.as_deref(), Some(codes::MODALITY_PROFILE_MISSING));
        // Another format beside it (onnx, tensorflow) changes nothing: the transformers checkpoint is the one the frontend reads.
        let r = evaluate(&listing(Some("text-generation"), &["config.json", "pytorch_model.bin", "model.onnx"]), None, &ctx());
        assert_eq!(gate(&r.technical, Gate::Lower).blocking.as_deref(), Some(codes::NOT_RUN_NEEDS_PICKLE_DIRECTORY));
    }

    #[test]
    fn the_format_the_adapter_and_the_missing_weights_are_named() {
        let r = evaluate(&listing(Some("text-generation"), &["config.json", "model.onnx"]), None, &ctx());
        assert_eq!(gate(&r.technical, Gate::Lower).blocking.as_deref(), Some(codes::FORMAT_UNSUPPORTED));
        assert_eq!(gate(&r.technical, Gate::Lower).arg.as_deref(), Some("onnx"));
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
        // A PEFT adapter over a pinned base is composed when its headers are read: the listing does not decide it.
        assert_eq!(gate(&r.technical, Gate::Lower).blocking.as_deref(), Some(codes::NOT_RUN_NOT_SAMPLED));
        let plan = super::super::listing::plan_of(&l, &r.selected);
        assert_eq!(plan.base.as_ref().map(|b| b.repo.as_str()), Some("b/base"), "the fetch reads the base too");
        // Its weights only as a pickle: not read.
        let mut lb = l.clone();
        lb.siblings = vec!["adapter_config.json".into(), "adapter_model.bin".into()];
        let r = evaluate(&lb, None, &ctx());
        assert_eq!(gate(&r.technical, Gate::Lower).blocking.as_deref(), Some(codes::FORMAT_UNSUPPORTED));
        // A LoRA file with no PEFT configuration (a diffusers LoRA) is not composed by this build.
        let mut ld = listing(Some("text-to-image"), &["pytorch_lora_weights.safetensors"]);
        ld.base_relation = Some("adapter".into());
        ld.base_ids = vec!["b/base".into()];
        ld.base_resolved = l.base_resolved.clone();
        let r = evaluate(&ld, None, &ctx());
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
