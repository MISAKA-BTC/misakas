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
                            "counts": c.counts.map(|k| json!({
                                "primary": {"wins": k.primary.wins, "losses": k.primary.losses, "ties": k.primary.ties},
                                "regression": {"wins": k.regression.wins, "losses": k.regression.losses, "ties": k.regression.ties},
                                "safety": {"wins": k.safety.wins, "losses": k.safety.losses, "ties": k.safety.ties},
                                "judge": {"wins": k.judge.wins, "losses": k.judge.losses, "ties": k.judge.ties},
                                "pairwise": {"wins": k.pairwise.wins, "losses": k.pairwise.losses, "ties": k.pairwise.ties},
                                "eligible": k.eligible,
                            })),
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
                "policy_sequence": l.line.policy_sequence,
                "governed_from_daa": l.line.governed_from_daa,
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
    json!({ "schema": "misaka.palw.improve-status.v1", "daa": daa, "lines": lines, "evaluation": evaluation })
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
        assert_eq!(palw_improve_admitting_classes_v1(Some(&read(true, 50, None))), None, "inside the grace nothing is judged by evidence");
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
        let mut node = PalwImproveNodeV1 { holds: [head].into(), evaluates: true, prefetch_full: false, admitting: None, ceilings: None };
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
                key: (line, 3, 1, job.subject, job.kind),
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
        assert_eq!((e["epoch"].as_u64(), e["items"].as_array().map(Vec::len), e["jobs"].as_array().map(Vec::len)), (Some(3), Some(2), Some(1)));
        assert_eq!((e["items"][0]["prompt_disclosed"].as_bool(), e["items"][0]["prompt_len"].as_u64()), (Some(true), Some(4)));
        assert_eq!((e["items"][1]["prompt_disclosed"].as_bool(), e["items"][1]["dropped"].as_bool()), (Some(false), Some(true)));
        assert_eq!(e["items"][0]["reference"], "ExactKey");
        let j = &e["jobs"][0];
        assert_eq!((j["subject"].as_str(), j["kind"].as_str(), j["item"].as_u64()), (Some("Candidate"), Some("ExactMatch"), Some(1)));
        assert_eq!(j["subject_class"].as_str(), Some(h(0xC1).to_string().as_str()));
        assert_eq!((j["claim"]["accepted_daa"].as_u64(), j["claim"]["final_daa"].as_u64(), j["claim"]["voided"].as_bool()), (Some(351), Some(502), Some(false)));
        assert_eq!(j["claim"]["bond"], format!("{}:2", TransactionId::from_u64_word(9)));
    }
}
