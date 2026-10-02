//! **RFC-0004's epoch watcher, the node's half (work item A10).**
//!
//! Past `palw_improvement_v1` a node follows every governed line's epoch and serves it: it fetches the
//! candidates it can hold (a composite candidate's adapter section over a parent it holds) and runs the
//! evaluation jobs it can (the subject classes it holds), each derived as the chain derives it. The
//! plan is the SDK's (`misaka_palw_sdk::improve::palw_improve_duties_v1`, pure, over the chain view
//! `PalwImproveChainV1`); this module keeps what the panel's loop remembers between ticks — the state
//! it last saw each line's epoch in, for the log — and what the node holds.
//!
//! **Dormant**: nothing here runs below the fence ([`palw_improve_watch_armed_v1`]), and the chain
//! view is read through the one door the core lane exposes for it
//! (`ConsensusApi::palw_improvement_open_epochs_v1`, read into the SDK's `PalwImproveViewsChainV1` —
//! this module never names a row's field).
//!
//! **On the panel's loop** (`palw_panel/improve.rs`, its child module): every
//! [`PALW_IMPROVE_READ_EVERY_V1`] the loop reads the node's doors (the open epochs, the evaluation view,
//! the status) and ticks the watcher; it logs each epoch's state change and the plan, writes the status
//! file ([`palw_improve_status_json_v1`]), and runs the plan: adapter prefetch, evaluation jobs and the
//! seat's replay of evaluation claims.

use std::collections::{BTreeMap, BTreeSet};

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::palw_improve_state_v1::PalwEpochStateV1;
use kaspa_consensus_core::palw_model_registry_v1::PalwModelRegistryReadV1;
use kaspa_core::{info, trace};
use misaka_palw_sdk::improve::{
    PalwImproveChainV1, PalwImproveDutyV1, PalwImproveNodeV1, palw_improve_duties_v1, palw_improve_epoch_moved_v1,
};

/// The most evaluation jobs one tick plans (each is a whole run of a subject class).
pub(crate) const PALW_IMPROVE_JOBS_PER_TICK_V1: usize = 4;

/// How often the panel's loop reads the improvement door (epochs move in tens of DAA).
pub(crate) const PALW_IMPROVE_READ_EVERY_V1: std::time::Duration = std::time::Duration::from_secs(10);

/// **Is the watcher armed at `daa_score`?** Only past `palw_improvement_v1` — `None` on every shipped
/// preset, so on every network today the watcher does nothing.
pub(crate) fn palw_improve_watch_armed_v1(params: &Params, daa_score: u64) -> bool {
    params.palw_improvement_v1_active_at(daa_score)
}

/// **The IR classes this node holds** — every class its loaded IR artifacts register (the node's
/// `--palw-class-artifact` holdings): a line's head it can evaluate, the parent a composite candidate
/// is fetched over.
pub(crate) fn palw_improve_held_classes_v1(holdings: &[misaka_palw_sdk::PalwLoadedArtifactV1]) -> BTreeSet<Hash64> {
    misaka_palw_sdk::tir_registration::tir_entries_of_v1(holdings).iter().map(|entry| entry.class_id()).collect()
}

/// **The classes the chain admits claims of now** (RFC-0004 §17.8.4 A6-4: an evaluation claim is an FP
/// claim of its subject's class, so the registry's claim gate — [`PalwModelLifecycleV1::admits_claims`] —
/// decides). `None` where the registry does not govern at the tip (absent, inactive, or inside its
/// grace): every class is then planned, as the chain would take it. Past the grace: the base class
/// (never gated), every rowed class whose state admits claims, and — below the work target, where a class
/// without a row is not refused — the rowless classes the read lists. The conservative side: a class the
/// read is unsure of stays in, because a run the chain refuses costs a run and a run never made costs
/// the epoch an evaluation.
pub(crate) fn palw_improve_admitting_classes_v1(read: Option<&PalwModelRegistryReadV1>) -> Option<BTreeSet<Hash64>> {
    let read = read?;
    if !read.active || read.tip_daa < read.grace_until_daa {
        return None;
    }
    Some(
        read.classes
            .iter()
            .filter(|c| match &c.row {
                Some(row) => c.is_base_class || row.state.admits_claims(),
                None => c.is_base_class || read.work_target.is_none(),
            })
            .map(|c| c.class_id)
            .collect(),
    )
}

/// **What the watcher remembers between ticks.**
#[derive(Debug, Default)]
pub(crate) struct PalwImproveWatchV1 {
    /// Each governed line's open epoch and the state it was last seen in.
    seen: BTreeMap<Hash64, (u64, PalwEpochStateV1)>,
}

/// One tick's result: what moved (for the log) and what to do.
#[derive(Debug, Default)]
pub(crate) struct PalwImproveTickV1 {
    /// `(line, epoch, the state it entered)` for every line whose epoch moved since the last tick.
    pub moved: Vec<(Hash64, u64, PalwEpochStateV1)>,
    /// Lines that left the governed set or whose epoch closed since the last tick.
    pub gone: Vec<Hash64>,
    pub duties: Vec<PalwImproveDutyV1>,
}

impl PalwImproveWatchV1 {
    /// **One tick**: every governed line's epoch read through `chain`, what moved since the last tick,
    /// and the plan for `node` at `current_daa`.
    pub(crate) fn tick(&mut self, chain: &dyn PalwImproveChainV1, node: &PalwImproveNodeV1, current_daa: u64) -> PalwImproveTickV1 {
        let mut tick = PalwImproveTickV1::default();
        let mut now: BTreeMap<Hash64, (u64, PalwEpochStateV1)> = BTreeMap::new();
        for line_id in chain.governed_lines() {
            let Some(line) = chain.line(&line_id) else { continue };
            let current = line.open_epoch.and_then(|e| chain.epoch(&line_id, e)).map(|epoch| (epoch.epoch, epoch.state));
            if let Some((epoch, state)) = palw_improve_epoch_moved_v1(self.seen.get(&line_id).copied(), current) {
                tick.moved.push((line_id, epoch, state));
            }
            if let Some(current) = current {
                now.insert(line_id, current);
            }
        }
        tick.gone = self.seen.keys().filter(|line| !now.contains_key(line)).copied().collect();
        self.seen = now;
        tick.duties = palw_improve_duties_v1(chain, node, current_daa, PALW_IMPROVE_JOBS_PER_TICK_V1);
        tick
    }
}

/// **A tick, logged**: each epoch's state change once (with the plan's size then), each epoch that
/// closed or line that left governance; the plan itself at trace level.
pub(crate) fn palw_improve_log_tick_v1(tick: &PalwImproveTickV1) {
    let prefetch = tick.duties.iter().filter(|d| matches!(d, PalwImproveDutyV1::Prefetch { .. })).count();
    let evaluate = tick.duties.len() - prefetch;
    for (line, epoch, state) in &tick.moved {
        info!(
            "[palw-improve] line {line}: epoch {epoch} is {state:?} — this node plans {prefetch} prefetch(es) and {evaluate} \
             evaluation(s) (RFC-0004 A10)"
        );
    }
    for line in &tick.gone {
        info!("[palw-improve] line {line}: its open epoch closed, or the line left governance (RFC-0004)");
    }
    for duty in &tick.duties {
        trace!("[palw-improve] planned: {duty:?}");
    }
}

/// A bond key as `txid:index`.
/// A candidate's (or the regression check's) promotion counts as JSON: wins, losses and ties per kind and
/// whether the candidate is eligible.
fn counts_json(k: kaspa_consensus_core::palw_improve_state_v1::PalwPromotionCountsV1) -> serde_json::Value {
    use serde_json::json;
    json!({
        "primary": {"wins": k.primary.wins, "losses": k.primary.losses, "ties": k.primary.ties},
        "regression": {"wins": k.regression.wins, "losses": k.regression.losses, "ties": k.regression.ties},
        "safety": {"wins": k.safety.wins, "losses": k.safety.losses, "ties": k.safety.ties},
        "judge": {"wins": k.judge.wins, "losses": k.judge.losses, "ties": k.judge.ties},
        "pairwise": {"wins": k.pairwise.wins, "losses": k.pairwise.losses, "ties": k.pairwise.ties},
        "eligible": k.eligible,
    })
}

fn bond_text(bond: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2) -> String {
    format!("{}:{}", bond.0.transaction_id, bond.0.index)
}

/// **The improvement protocol's status as JSON** — what the status file holds, one read of the node's
/// status door: every line (header, usage, pool, head history) and every epoch row the chain still keeps
/// (state and clock, candidates with their counts, items, the outcome and the grants). Ids are hex, bonds
/// `txid:index`, enums their names. The format is the drill watcher's and an operator's; nothing in
/// consensus reads it.
pub(crate) fn palw_improve_status_json_v1(
    status: &kaspa_consensus_core::palw_improve_node_v1::PalwImprovementStatusV1,
    evals: &[kaspa_consensus_core::palw_improve_node_v1::PalwImprovementEvalViewV1],
    daa: u64,
) -> serde_json::Value {
    use kaspa_consensus_core::palw_improve_state_v1::PalwEvalSubjectV1;
    use serde_json::json;
    let lines: Vec<serde_json::Value> = status
        .lines
        .iter()
        .map(|l| {
            let epochs: Vec<serde_json::Value> = l
                .epochs
                .iter()
                .map(|e| {
                    let h = &e.epoch;
                    json!({
                        "epoch": h.epoch,
                        "state": format!("{:?}", h.state),
                        "times": {
                            "t_open": h.times.t_open, "t_fix": h.times.t_fix, "t_close": h.times.t_close,
                            "t_draw": h.times.t_draw, "t_eval": h.times.t_eval, "t_score": h.times.t_score,
                        },
                        "parent": h.parent.to_string(),
                        "previous": h.previous.map(|p| p.to_string()),
                        "dataset_root": h.dataset_root.map(|r| r.to_string()),
                        "seed": h.seed.map(|r| r.to_string()),
                        "counts": {
                            "candidates": h.candidates, "pool_entries": h.pool_entries, "holdout_cases": h.holdout_cases,
                            "setter_sets": h.setter_sets, "items": h.items, "grants": h.grants,
                        },
                        "outcome": h.outcome.map(|o| format!("{o:?}")),
                        "decided_daa": h.decided_daa,
                        "previous_counts": h.previous_counts.map(counts_json),
                        "retire": format!("{:?}", h.retire),
                        "escrow": {
                            "parent": h.escrow.parent, "parent_spent": h.escrow.parent_spent,
                            "previous": h.escrow.previous, "previous_spent": h.escrow.previous_spent,
                        },
                        "candidates": e.candidates.iter().map(|c| json!({
                            "class": c.class_id.to_string(),
                            "submitter": bond_text(&c.submitter),
                            "artifact": match c.artifact {
                                kaspa_consensus_core::palw_improve_artifact_v1::PalwTirArtifactRefV1::Single { .. } => "Single",
                                kaspa_consensus_core::palw_improve_artifact_v1::PalwTirArtifactRefV1::Composite { .. } => "Composite",
                            },
                            "fee_paid": c.fee_paid, "bond": c.bond, "escrow": c.escrow, "escrow_spent": c.escrow_spent,
                            "submitted_daa": c.submitted_daa,
                            "counts": c.counts.map(counts_json),
                        })).collect::<Vec<_>>(),
                        "grants": e.grants.iter().map(|g| json!({
                            "recipient": bond_text(&g.recipient), "stage": format!("{:?}", g.stage), "amount": g.amount,
                            "label": format!("{:?}", g.label), "vest_from_daa": g.vest_from_daa, "vest_unit_daa": g.vest_unit_daa,
                            "vest_epochs": g.vest_epochs, "vested": g.vested, "forfeited": g.forfeited,
                        })).collect::<Vec<_>>(),
                    })
                })
                .collect();
            json!({
                "line_id": l.line.line_id.to_string(),
                "class_id": l.line.class_id.to_string(),
                "status": format!("{:?}", l.line.status),
                "head": l.line.head.to_string(),
                "head_seq": l.line.head_seq,
                "open_epoch": l.line.open_epoch,
                "next_epoch": l.line.next_epoch,
                "next_due_daa": l.line.next_due_daa,
                "next_check_daa": l.line.next_check_daa,
                "policy_sequence": l.line.policy_sequence,
                "governed_from_daa": l.line.governed_from_daa,
                "last_promotion": l.line.last_promotion.map(|p| json!({"epoch": p.epoch, "owner_until_daa": p.owner_until_daa, "ban_daa": p.ban_daa})),
                "regression_epoch": l.line.regression_epoch,
                "regression_check": l.line.regression_check.map(|c| c.to_string()),
                "barred": l.line.barred.iter().map(|(b, until)| json!({"bond": bond_text(b), "until": until})).collect::<Vec<_>>(),
                "usage": l.usage.map(|u| json!({"usage": u.usage.to_string(), "since_daa": u.since_daa})),
                "pool": l.pool.map(|p| json!({
                    "balance": p.balance, "held": p.held, "unvested": p.unvested, "s1_budget": p.s1_budget,
                    "deposited": p.deposited.to_string(), "fees_in": p.fees_in.to_string(), "phi_in": p.phi_in.to_string(),
                    "held_in": p.held_in.to_string(), "forfeited_in": p.forfeited_in.to_string(),
                    "paid": p.paid.to_string(), "refunded": p.refunded.to_string(),
                })),
                "heads": l.heads.iter().map(|(seq, h)| json!({
                    "seq": seq, "epoch": h.epoch, "class": h.class_id.to_string(),
                    "previous": h.previous.map(|p| p.to_string()), "daa": h.daa, "cause": format!("{:?}", h.cause),
                })).collect::<Vec<_>>(),
                "epochs": epochs,
            })
        })
        .collect();
    // The evaluation view: for every open epoch past its draw, each drawn item (whether its prompt is
    // disclosed, its reference's kind) and every job that holds a claim — the drill's watcher counts the
    // claims, their finality and the voids from it, and tells when a setter's keys may be revealed.
    let evaluation: Vec<serde_json::Value> = evals
        .iter()
        .map(|e| {
            json!({
                "line_id": e.line_id.to_string(),
                "epoch": e.epoch,
                "items": e.items.iter().map(|i| json!({
                    "item": i.item,
                    "dropped": i.dropped,
                    "prompt_disclosed": i.prompt_ids.is_some(),
                    "prompt_len": i.prompt_ids.as_ref().map(Vec::len),
                    "reference": format!("{:?}", i.reference).split(' ').next().unwrap_or("").to_string(),
                    "reference_disclosed": i.reference_ids.is_some(),
                })).collect::<Vec<_>>(),
                "jobs": e.jobs.iter().map(|j| {
                    let (subject, class) = match j.job.subject {
                        PalwEvalSubjectV1::Parent => ("Parent", None),
                        PalwEvalSubjectV1::Candidate(c) => ("Candidate", Some(c)),
                        PalwEvalSubjectV1::Previous(c) => ("Previous", Some(c)),
                    };
                    json!({
                        "item": j.job.item,
                        "subject": subject,
                        "subject_class": class.map(|c| c.to_string()),
                        "kind": format!("{:?}", j.job.kind),
                        "part": j.job.part,
                        "claim": j.claim.as_ref().map(|c| json!({
                            "id": c.claim_id.to_string(),
                            "bond": bond_text(&c.bond),
                            "accepted_daa": c.accepted_daa,
                            "final_daa": c.final_daa,
                            "voided": c.voided,
                            "work_leaves": c.work_leaves,
                            "score": c.score,
                        })),
                    })
                }).collect::<Vec<_>>(),
            })
        })
        .collect();
    // The composite candidate classes the chain recorded: what a seat's possession proof of one opens (the adapter
    // section's root), kept for as long as the class lives.
    let composite_classes: Vec<serde_json::Value> = status
        .composite_classes
        .iter()
        .map(|(class, r)| {
            json!({
                "class": class.to_string(), "parent_class": r.parent_class.to_string(), "parent_root": r.parent_root.to_string(),
                "adapter_root": r.adapter_root.to_string(), "p": r.p,
            })
        })
        .collect();
    json!({
        "schema": "misaka.palw.improve-status.v1", "daa": daa, "lines": lines, "evaluation": evaluation,
        "composite_classes": composite_classes,
    })
}

/// Which of a line's evaluations a drill's lie may spoil: those of the parent (the line's head), or those of a candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PalwImproveTamperSubjectV1 {
    Parent,
    Candidate,
}

/// **A drill's tamper** (`--palw-drill-tamper-eval=<fault>[@<line>][/<parent|candidate>]`): the fault this node's executor
/// commits ([`PalwEvalFaultV1`]: `leaf:<index>`, `output` or `score`), the line whose evaluation it spoils — a hex prefix of
/// the line id, empty for any — and which subject's evaluations (a parent's, a candidate's, or either). One lie per node: it
/// lies on a matching evaluation until a lie lands on the chain ([`PalwImproveLieV1`]), then is honest (a convicted bond is slashed).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PalwImproveTamperV1 {
    pub fault: misaka_palw_sdk::improve_eval::PalwEvalFaultV1,
    pub line_prefix: String,
    pub subject: Option<PalwImproveTamperSubjectV1>,
}

impl PalwImproveTamperV1 {
    /// Does the tamper spoil an evaluation of this line, for this subject?
    pub(crate) fn applies_to(&self, line: &Hash64, subject: &kaspa_consensus_core::palw_improve_state_v1::PalwEvalSubjectV1) -> bool {
        use kaspa_consensus_core::palw_improve_state_v1::PalwEvalSubjectV1 as S;
        line.to_string().starts_with(&self.line_prefix)
            && match (self.subject, subject) {
                (None, _) => true,
                (Some(PalwImproveTamperSubjectV1::Parent), S::Parent) => true,
                (Some(PalwImproveTamperSubjectV1::Candidate), S::Candidate(_)) => true,
                _ => false,
            }
    }
}

/// Parse `--palw-drill-tamper-eval`'s value: `<fault>`, optionally `@<line hex prefix>` and `/<parent|candidate>`.
pub(crate) fn palw_improve_tamper_spec_v1(spec: &str) -> Result<PalwImproveTamperV1, String> {
    let (rest, subject) = match spec.rsplit_once('/') {
        Some((rest, "parent")) => (rest, Some(PalwImproveTamperSubjectV1::Parent)),
        Some((rest, "candidate")) => (rest, Some(PalwImproveTamperSubjectV1::Candidate)),
        Some((_, other)) => return Err(format!("`{other}` is not a subject: parent or candidate")),
        None => (spec, None),
    };
    let (fault, line) = match rest.split_once('@') {
        Some((fault, line)) => (fault, line.trim().to_ascii_lowercase()),
        None => (rest, String::new()),
    };
    if !line.chars().all(|c| c.is_ascii_hexdigit()) || line.len() > 128 {
        return Err(format!("`{line}` is not a hex prefix of a line id"));
    }
    Ok(PalwImproveTamperV1 { fault: misaka_palw_sdk::improve_eval::PalwEvalFaultV1::parse(fault)?, line_prefix: line, subject })
}

/// A carried lie waits this many DAA for the chain to show it before it is given up as lost (the carrier never mined).
pub(crate) const PALW_IMPROVE_LIE_PATIENCE_DAA_V1: u64 = 36;

/// **Where a drill liar's lie stands** — one lie per node, told until it lands. Two executors race for every evaluation job
/// (the first valid claim takes it), so a lie that loses the race is no lie told: the liar keeps lying on matching jobs, one at a
/// time, until the chain holds a lying claim of its own; then it is honest, and a bond is slashed once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PalwImproveLieV1 {
    /// No lie in flight: the next matching evaluation is run with the fault.
    #[default]
    Idle,
    /// The faulted run of this job is running or waits for a carrier.
    Pending { job: Hash64 },
    /// The lying claim was carried at `daa` and the chain's view does not show it yet.
    Carried { job: Hash64, claim: Hash64, daa: u64 },
    /// The chain holds the lying claim: the lie is told.
    Spent,
}

/// **A carried lie against the chain's view of its job** (`claim_of_job`: the claim the view holds for it, if any): landed when the
/// view's claim is the lie's; lost (told again on the next job) when another claim took the job, or when nothing shows after the
/// patience.
pub(crate) fn palw_improve_lie_carried_step_v1(
    job: Hash64,
    claim: Hash64,
    carried_daa: u64,
    claim_of_job: Option<Hash64>,
    daa: u64,
) -> PalwImproveLieV1 {
    match claim_of_job {
        Some(c) if c == claim => PalwImproveLieV1::Spent,
        Some(_) => PalwImproveLieV1::Idle,
        None if daa > carried_daa.saturating_add(PALW_IMPROVE_LIE_PATIENCE_DAA_V1) => PalwImproveLieV1::Idle,
        None => PalwImproveLieV1::Carried { job, claim, daa: carried_daa },
    }
}

/// **A dispute this node found**: an evaluation claim whose replay on this node's weights differs, with where it
/// parts from the honest run when the accused's capture was at hand, and how the dispute stands (what was filed, or
/// why nothing was).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PalwImproveDisputeV1 {
    pub claim_id: Hash64,
    pub job_id: Hash64,
    /// `leaf`, `output` or `score` (located from the capture), or `unlocated`: the replay differs and nothing says where.
    pub kind: &'static str,
    pub detail: String,
    pub found_daa: u64,
    pub filing: String,
}

/// The disputes as the status file lists them.
pub(crate) fn palw_improve_disputes_json_v1(disputes: &[PalwImproveDisputeV1]) -> serde_json::Value {
    serde_json::Value::Array(
        disputes
            .iter()
            .map(|d| {
                serde_json::json!({
                    "claim": d.claim_id.to_string(), "job": d.job_id.to_string(), "kind": d.kind, "detail": d.detail,
                    "found_daa": d.found_daa, "filing": d.filing,
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_improve_artifact_v1::PalwTirArtifactRefV1;
    use misaka_palw_sdk::improve::{PalwImproveMemChainV1, PalwImprovePrefetchV1};

    fn h(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    /// **The chain's claim gate, as the node plans by it** (RFC-0004 §17.8.4 A6-4): where the registry does
    /// not govern at the tip every class is planned; past its grace only the base class and the classes
    /// whose lifecycle admits claims (Probation, ActiveLimited, Active) are — Candidate, Prefetching,
    /// Registered and Held are not — and a rowless class stays in below the work target (the conservative
    /// side: the chain would take it) and is out past it.
    #[test]
    fn the_node_plans_only_the_classes_the_registry_admits_claims_of() {
        use kaspa_consensus_core::palw_model_registry_v1::{
            PalwModelLifecycleRowV1, PalwModelLifecycleV1, PalwModelRegistryClassReadV1, PalwModelWorkV1, PalwWorkTargetReadV1,
        };
        let class = |id: u64, base: bool, state: Option<PalwModelLifecycleV1>| PalwModelRegistryClassReadV1 {
            economic_ccu_per_claim: 0,
            work_ratio_permille: 0,
            expected_forwards_q32: 0,
            work_ticket_target: 0,
            class_target: 0,
            panel_room: 0,
            final_work_share_10_permille: 0,
            final_work_share_100_permille: 0,
            class_id: h(id),
            artifact_root: h(0),
            is_base_class: base,
            row: state.map(|state| PalwModelLifecycleRowV1 {
                state,
                work: PalwModelWorkV1::default(),
                profile: Default::default(),
                since_span: 0,
                probes_passed: 0,
                probes_failed: 0,
                probes_passed_this_span: 0,
                probes_failed_this_span: 0,
                ready_seats: 0,
                inflight_claims: 0,
                utilization_permille: 0,
                admission_milli: 0,
                cap_utilization_permille: 0,
                priced_share_permille: 0,
            }),
            ready_seats_now: 0,
            seating: None,
            inflight_now: 0,
            share_permille: None,
            no_capable_panel_voids: 0,
            reason: String::new(),
        };
        let read = |active: bool, tip: u64, work_target: Option<PalwWorkTargetReadV1>| PalwModelRegistryReadV1 {
            active,
            tip_daa: tip,
            grace_until_daa: 100,
            work_target,
            classes: vec![
                class(1, true, None),
                class(2, false, Some(PalwModelLifecycleV1::Candidate)),
                class(3, false, Some(PalwModelLifecycleV1::Prefetching)),
                class(4, false, Some(PalwModelLifecycleV1::Probation { probes_passed: 0 })),
                class(5, false, Some(PalwModelLifecycleV1::ActiveLimited { stable_epochs: 1 })),
                class(6, false, Some(PalwModelLifecycleV1::Active)),
                class(7, false, Some(PalwModelLifecycleV1::Held)),
                class(8, false, None),
            ],
            ..Default::default()
        };
        assert_eq!(palw_improve_admitting_classes_v1(None), None, "no registry read: every class is planned");
        assert_eq!(palw_improve_admitting_classes_v1(Some(&read(false, 500, None))), None, "an inactive registry governs nothing");
        assert_eq!(
            palw_improve_admitting_classes_v1(Some(&read(true, 50, None))),
            None,
            "inside the grace nothing is judged by evidence"
        );
        assert_eq!(
            palw_improve_admitting_classes_v1(Some(&read(true, 500, None))),
            Some([h(1), h(4), h(5), h(6), h(8)].into()),
            "the base class, Probation, ActiveLimited, Active — and the rowless class below the work target"
        );
    }

    /// **The watcher follows an epoch through its states and serves it**: each state change is logged
    /// once; in `Submission` it plans the composite candidate's adapter section; in `Evaluating` the
    /// tasks of the classes it holds, at most the tick's budget; the epoch's close is noticed; an empty
    /// read door plans nothing; the panel's loop ticks it over the door behind the fence; and below the
    /// fence the watcher is not armed on any shipped preset.
    #[test]
    fn the_watcher_follows_an_epoch_and_plans_its_prefetch_and_evaluation() {
        let mut chain: PalwImproveMemChainV1 = misaka_palw_sdk::improve::testing::chain(PalwEpochStateV1::Submission, false);
        let line = h(misaka_palw_sdk::improve::testing::LINE);
        let head = h(misaka_palw_sdk::improve::testing::HEAD);
        let mut node =
            PalwImproveNodeV1 { holds: [head].into(), evaluates: true, prefetch_full: false, admitting: None, ceilings: None };
        let mut watch = PalwImproveWatchV1::default();
        let tick = watch.tick(&chain, &node, 160);
        assert_eq!(tick.moved, vec![(line, 3, PalwEpochStateV1::Submission)]);
        assert!(matches!(
            tick.duties.as_slice(),
            [PalwImproveDutyV1::Prefetch { plan: PalwImprovePrefetchV1::Adapter { p: 40, .. }, .. }]
        ));
        assert!(watch.tick(&chain, &node, 161).moved.is_empty(), "logged once");
        node.holds.insert(h(0xC1));
        chain.epochs.get_mut(&(line, 3)).unwrap().state = PalwEpochStateV1::Evaluating;
        let tick = watch.tick(&chain, &node, 250);
        assert_eq!(tick.moved, vec![(line, 3, PalwEpochStateV1::Evaluating)]);
        let evaluations = tick.duties.iter().filter(|d| matches!(d, PalwImproveDutyV1::Evaluate { .. })).count();
        assert_eq!(evaluations, PALW_IMPROVE_JOBS_PER_TICK_V1, "two runnable items × two subjects, at most the tick's budget");
        chain.lines.get_mut(&line).unwrap().open_epoch = None;
        let tick = watch.tick(&chain, &node, 400);
        assert_eq!(tick.gone, vec![line], "the epoch closed");
        assert!(tick.duties.is_empty());
        // The node's doors with no open epoch: nothing moves, nothing is planned.
        let empty = misaka_palw_sdk::improve::PalwImproveNodeChainV1 { views: &[], eval: &[] };
        let tick = watch.tick(&empty, &node, 401);
        assert!(tick.moved.is_empty() && tick.gone.is_empty() && tick.duties.is_empty());
        // The panel's improvement loop reads the doors behind the fence and ticks the watcher over them.
        let improve = include_str!("palw_panel/improve.rs");
        let at = improve.find("palw_improve_watch_armed_v1(&self.consensus_config.params, current_daa)").expect("armed");
        let read = &improve[at..at + 1600];
        assert!(read.contains("c.palw_improvement_open_epochs_v1()") && read.contains("c.palw_improvement_eval_views_v1()"));
        assert!(read.contains("st.watch.tick(&chain, &node, current_daa)"));
        assert!(include_str!("palw_panel.rs").contains("self.improve_tick_v1(&mut improve, current_daa).await"));
        let _ = PalwTirArtifactRefV1::Single { root: h(0) };
        assert!(palw_improve_held_classes_v1(&[]).is_empty(), "a node with no IR artifact holds no class");
        for net in [
            kaspa_consensus_core::network::NetworkId::with_suffix(kaspa_consensus_core::network::NetworkType::Testnet, 12),
            kaspa_consensus_core::network::NetworkId::new(kaspa_consensus_core::network::NetworkType::Mainnet),
        ] {
            let params = Params::from(net);
            assert!(!palw_improve_watch_armed_v1(&params, u64::MAX / 2), "{net}: dormant");
        }
    }

    /// **The status file's shape**: an empty status is an empty list under the schema tag; the format the
    /// drill's watcher reads is stable JSON (ids as hex, no float).
    #[test]
    fn the_status_json_names_its_schema_and_lists_lines() {
        let json = palw_improve_status_json_v1(&Default::default(), &[], 123);
        assert_eq!(json["schema"], "misaka.palw.improve-status.v1");
        assert_eq!(json["daa"], 123);
        assert_eq!(json["lines"], serde_json::json!([]));
        assert_eq!(json["evaluation"], serde_json::json!([]));
        assert_eq!(json["composite_classes"], serde_json::json!([]));
    }

    /// **The status lists the chain's composite classes** (what a seat's possession proof of one opens), and the
    /// drill's tamper spec and dispute records have their one reader each.
    #[test]
    fn the_status_lists_composite_classes_and_the_drill_specs_parse() {
        use kaspa_consensus_core::palw_improve_composite_v1::PalwTirCompositeRefV1;
        use misaka_palw_sdk::improve_eval::PalwEvalFaultV1;
        let status = kaspa_consensus_core::palw_improve_node_v1::PalwImprovementStatusV1 {
            lines: vec![],
            composite_classes: vec![(
                h(0xC1),
                PalwTirCompositeRefV1 { parent_class: h(0xA0), parent_root: h(0xA1), adapter_root: h(0xA2), p: 17 },
            )],
        };
        let json = palw_improve_status_json_v1(&status, &[], 5);
        let c = &json["composite_classes"][0];
        assert_eq!(c["class"], h(0xC1).to_string());
        assert_eq!(
            (c["parent_class"].as_str(), c["adapter_root"].as_str(), c["p"].as_u64()),
            (Some(h(0xA0).to_string().as_str()), Some(h(0xA2).to_string().as_str()), Some(17))
        );
        // The tamper spec.
        use kaspa_consensus_core::palw_improve_state_v1::PalwEvalSubjectV1 as Subject;
        let any = palw_improve_tamper_spec_v1("leaf:7").expect("a spec");
        assert_eq!((any.fault, any.line_prefix.as_str(), any.subject), (PalwEvalFaultV1::Leaf(7), "", None));
        assert!(any.applies_to(&h(1), &Subject::Parent) && any.applies_to(&h(2), &Subject::Candidate(h(9))));
        let line = h(0x11E).to_string();
        let one = palw_improve_tamper_spec_v1(&format!("output@{line}")).expect("a spec with a line");
        assert_eq!(one.fault, PalwEvalFaultV1::Output);
        assert!(one.applies_to(&h(0x11E), &Subject::Parent) && !one.applies_to(&h(0x11F), &Subject::Parent), "only the named line");
        assert_eq!(palw_improve_tamper_spec_v1("score@AB").expect("case-insensitive").line_prefix, "ab");
        for bad in ["", "leaf", "leaf:x", "tree", "leaf:1@zz", "leaf:1@ab/grandparent", "output/"] {
            assert!(palw_improve_tamper_spec_v1(bad).is_err(), "{bad:?}");
        }
        assert_eq!(palw_improve_tamper_spec_v1("output@").expect("an empty line is any").line_prefix, "");
        // A subject pin: a parent's evaluations, or a candidate's (a composite's among them), never both.
        let cand = palw_improve_tamper_spec_v1(&format!("leaf:1@{line}/candidate")).expect("a pinned spec");
        assert_eq!((cand.fault, cand.subject), (PalwEvalFaultV1::Leaf(1), Some(PalwImproveTamperSubjectV1::Candidate)));
        assert!(cand.applies_to(&h(0x11E), &Subject::Candidate(h(7))) && !cand.applies_to(&h(0x11E), &Subject::Parent));
        assert!(!cand.applies_to(&h(0x11E), &Subject::Previous(h(7))), "the regression check's incumbent is neither");
        let parent = palw_improve_tamper_spec_v1("output@/parent").expect("a subject without a line");
        assert_eq!((parent.line_prefix.as_str(), parent.subject), ("", Some(PalwImproveTamperSubjectV1::Parent)));
        assert!(parent.applies_to(&h(5), &Subject::Parent) && !parent.applies_to(&h(5), &Subject::Candidate(h(7))));
        // The dispute records.
        let json = palw_improve_disputes_json_v1(&[PalwImproveDisputeV1 {
            claim_id: h(1),
            job_id: h(2),
            kind: "leaf",
            detail: "leaf 9".to_string(),
            found_daa: 77,
            filing: "not filed".to_string(),
        }]);
        assert_eq!(
            (json[0]["kind"].as_str(), json[0]["found_daa"].as_u64(), json[0]["claim"].as_str()),
            (Some("leaf"), Some(77), Some(h(1).to_string().as_str()))
        );
        assert_eq!(palw_improve_disputes_json_v1(&[]), serde_json::json!([]));
    }

    /// **A drill liar's lie is told until it lands** (two executors race for every job): a carried lie is spent when the chain's view
    /// holds its claim, lost — to tell again — when another claim took its job or nothing shows within the patience.
    #[test]
    fn a_carried_lie_is_spent_when_it_lands_and_told_again_when_it_is_lost() {
        let (job, lie, other) = (h(0x70B), h(0x11E), h(0x07E));
        let carried = PalwImproveLieV1::Carried { job, claim: lie, daa: 100 };
        let step = |claim_of_job, daa| palw_improve_lie_carried_step_v1(job, lie, 100, claim_of_job, daa);
        assert_eq!(step(Some(lie), 103), PalwImproveLieV1::Spent, "the chain holds the lying claim");
        assert_eq!(step(Some(lie), 400), PalwImproveLieV1::Spent, "however late the view shows it");
        assert_eq!(step(Some(other), 103), PalwImproveLieV1::Idle, "another claim took the job: the lie lost the race");
        assert_eq!(step(None, 105), carried, "nothing yet, within the patience: wait");
        assert_eq!(step(None, 100 + PALW_IMPROVE_LIE_PATIENCE_DAA_V1), carried, "the patience is not past");
        assert_eq!(step(None, 101 + PALW_IMPROVE_LIE_PATIENCE_DAA_V1), PalwImproveLieV1::Idle, "never mined: tell it again");
        assert_eq!(PalwImproveLieV1::default(), PalwImproveLieV1::Idle);
    }

    /// **The status file carries the evaluation view** — the drill's watcher counts the claims, their
    /// finality and voids per job from it, and knows when an item's prompt is disclosed.
    #[test]
    fn the_status_json_lists_the_evaluation_jobs_with_their_claims() {
        use kaspa_consensus_core::palw_improve_eval_v1::{PalwEvalJobV1, PalwEvalModeV1};
        use kaspa_consensus_core::palw_improve_material_v1::PalwCaseReferenceV1;
        use kaspa_consensus_core::palw_improve_node_v1::{
            PalwImprovementEvalClaimViewV1, PalwImprovementEvalItemViewV1, PalwImprovementEvalJobViewV1, PalwImprovementEvalViewV1,
        };
        use kaspa_consensus_core::palw_improve_state_v1::{PalwEvalSubjectV1, PalwScoringKindV1};
        use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
        use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
        let line = h(0x11E);
        let job = PalwEvalJobV1 {
            line_id: line,
            epoch: 3,
            item: 1,
            subject: PalwEvalSubjectV1::Candidate(h(0xC1)),
            kind: PalwScoringKindV1::ExactMatch,
            part: 0,
            mode: PalwEvalModeV1::Generate { seed: h(5), max_new: 3, stop_ids: vec![] },
        };
        let view = PalwImprovementEvalViewV1 {
            line_id: line,
            epoch: 3,
            items: vec![
                PalwImprovementEvalItemViewV1 {
                    item: 1,
                    prompt_ids: Some(vec![1, 5, 9, 7]),
                    reference: PalwCaseReferenceV1::ExactKey { commitment: h(0xB0) },
                    reference_ids: None,
                    domain: 0,
                    dropped: false,
                },
                PalwImprovementEvalItemViewV1 {
                    item: 2,
                    prompt_ids: None,
                    reference: PalwCaseReferenceV1::ExactKey { commitment: h(0xB1) },
                    reference_ids: None,
                    domain: 0,
                    dropped: true,
                },
            ],
            jobs: vec![PalwImprovementEvalJobViewV1 {
                key: (line, 3, 1, job.subject, job.kind, job.part),
                job,
                claim: Some(PalwImprovementEvalClaimViewV1 {
                    claim_id: h(0xC1A),
                    bond: PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(9), index: 2 }),
                    accepted_daa: 351,
                    final_daa: Some(502),
                    voided: false,
                    trace_root: h(1),
                    output_root: h(2),
                    execution_root: h(3),
                    work_leaves: 180,
                    score: None,
                }),
            }],
        };
        let json = palw_improve_status_json_v1(&Default::default(), &[view], 400);
        let e = &json["evaluation"][0];
        assert_eq!(
            (e["epoch"].as_u64(), e["items"].as_array().map(Vec::len), e["jobs"].as_array().map(Vec::len)),
            (Some(3), Some(2), Some(1))
        );
        assert_eq!((e["items"][0]["prompt_disclosed"].as_bool(), e["items"][0]["prompt_len"].as_u64()), (Some(true), Some(4)));
        assert_eq!((e["items"][1]["prompt_disclosed"].as_bool(), e["items"][1]["dropped"].as_bool()), (Some(false), Some(true)));
        assert_eq!(e["items"][0]["reference"], "ExactKey");
        let j = &e["jobs"][0];
        assert_eq!((j["subject"].as_str(), j["kind"].as_str(), j["item"].as_u64()), (Some("Candidate"), Some("ExactMatch"), Some(1)));
        assert_eq!(j["subject_class"].as_str(), Some(h(0xC1).to_string().as_str()));
        assert_eq!(
            (j["claim"]["accepted_daa"].as_u64(), j["claim"]["final_daa"].as_u64(), j["claim"]["voided"].as_bool()),
            (Some(351), Some(502), Some(false))
        );
        assert_eq!(j["claim"]["bond"], format!("{}:2", TransactionId::from_u64_word(9)));
    }
}
