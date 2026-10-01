//! **RFC-0004 §3 / spec 17 §17.4: a governed line's policy** — its check (§17.4.3), the owner's
//! signed message (§17.4.2), the nominal epoch length `L_e`, the most jobs a subject can be given (the
//! escrow's size, §17.11.1), and an example policy the drills and the vectors use.
//!
//! Everything here is a function of its arguments. What needs the chain — that the judge classes are
//! admitted IR classes, that the line is an IR line — is the fold's (`apply_improvement_policy_set_v1`).

use crate::Hash64;
use crate::palw_improve_promotion_v1::{PALW_IMPROVE_SIGN_K_MAX_V1, PALW_IMPROVE_SIGN_N_MAX_V1, palw_improve_sign_alpha_is_tabled_v1};
use crate::palw_improve_state_v1::{
    PALW_IMPROVEMENT_POLICY_VERSION_V1, PalwEpochWindowsV1, PalwEvalSpecV1, PalwImprovementFeesV1, PalwImprovementPolicyV1,
    PalwProvenancePolicyV1, PalwScoringKindV1, PalwScoringParamsV1, PalwScoringStageV1, PalwTeacherClassV1, PalwUsageMeasureV1,
    PalwUsageThresholdV1, palw_improvement_policy_digest_v1,
};
use crate::palw_improve_v1::PalwImprovementCeilingsV1;

/// The least claiming a draw leaves: `w_eval` must exceed `beacon_delay` by more than this [E12].
pub const PALW_IMPROVE_MIN_CLAIM_WINDOW_DAA_V1: u64 = 32;
/// The longest any one window may be.
pub const PALW_IMPROVE_MAX_WINDOW_DAA_V1: u64 = 1 << 32;
pub const PALW_IMPROVE_MAX_STAGES_V1: usize = 8;
pub const PALW_IMPROVE_MAX_JUDGES_V1: usize = 16;
pub const PALW_IMPROVE_MAX_SUITE_ITEMS_V1: u32 = 1_024;
pub const PALW_IMPROVE_MAX_NEW_TOKENS_V1: u32 = 4_096;
pub const PALW_IMPROVE_MAX_STOP_IDS_V1: usize = 16;
/// The most ids a judge's verdict sequence has (spec 17 §17.4.3 row 7).
pub const PALW_IMPROVE_MAX_VERDICT_IDS_V1: usize = 8;
pub const PALW_IMPROVE_MAX_LICENCE_CLASSES_V1: usize = 32;
pub const PALW_IMPROVE_MAX_VEST_EPOCHS_V1: u32 = 64;
pub const PALW_IMPROVE_MAX_ROLLBACK_EPOCHS_V1: u32 = 16;
pub const PALW_IMPROVE_MAX_BAN_EPOCHS_V1: u32 = 256;
/// An epoch's hold-out pool holds at most this many times `n` cases (spec 17 §17.6.2).
pub const PALW_IMPROVE_HOLDOUT_FACTOR_V1: u32 = 4;
/// An epoch takes at most this many setter sets (spec 17 §17.6.2).
pub const PALW_IMPROVE_MAX_SETTER_SETS_V1: u32 = 16;
/// Key of [`palw_improvement_policy_message_v1`].
pub const PALW_IMPROVE_POLICY_SET_DOMAIN_V1: &[u8] = b"misaka-palw/improve/policy-set/v1";
/// The ML-DSA-87 context the owner signs a policy object under.
pub const PALW_IMPROVE_POLICY_MLDSA87_CONTEXT: &[u8] = b"misaka-palw-improve-policy-v1";
/// Key of a rollback's message (spec 17 §17.10.1).
pub const PALW_IMPROVE_ROLLBACK_DOMAIN_V1: &[u8] = b"misaka-palw/improve/rollback/v1";
/// The ML-DSA-87 context a rollback is signed under.
pub const PALW_IMPROVE_ROLLBACK_MLDSA87_CONTEXT: &[u8] = b"misaka-palw-improve-rollback-v1";
/// Key of an improvement payout's key (spec 17 §17.11.5).
pub const PALW_IMPROVE_PAYOUT_DOMAIN_V1: &[u8] = b"misaka-palw/improve/payout/v1";

/// **`L_e`, the nominal epoch length** (spec 17 §17.4.3): the unit vesting and the rollback window
/// count in.
pub fn palw_improvement_epoch_length_v1(w: &PalwEpochWindowsV1) -> u64 {
    w.w_collect.saturating_add(w.w_submit).saturating_add(w.w_holdout).saturating_add(w.w_eval).saturating_add(w.court_margin)
}

/// **The result rows an epoch of this policy reserves** (spec 17 §17.3): `(n + regression_items +
/// safety_items) × (k_max + 2)` — every item, for the parent, every candidate and the regression check.
pub fn palw_improvement_results_bound_v1(policy: &PalwImprovementPolicyV1) -> u64 {
    let items = policy.eval.n as u64 + policy.eval.regression_items as u64 + policy.eval.safety_items as u64;
    items * (policy.k_max as u64 + 2)
}

/// Does the spec have a stage of `kind`?
pub fn palw_improvement_has_stage_v1(eval: &PalwEvalSpecV1, kind: PalwScoringKindV1) -> bool {
    eval.stages.iter().any(|stage| stage.kind == kind)
}

/// **The most evaluation jobs one subject can be given in an epoch** (spec 17 §17.11.1): one per item for its
/// primary kind [D1] (an ExactMatch job generates and the fold scores it at the key's reveal; a likelihood job is
/// one teacher-forced pipeline), two per drawn item for a Judge stage (one teacher-forced pass per verdict
/// sequence, §17.8.5 [A6]), and four per drawn item for a Pairwise stage (two orders × two verdicts; `pairwise`
/// false for the parent): `J = (n + reg + safety) + 2·n·[Judge] + 4·n·[Pairwise ∧ not the parent]`. The escrow
/// is this times `eval_fee_per_job`; what is not spent comes back (§17.11.2).
pub fn palw_improvement_jobs_per_subject_v1(eval: &PalwEvalSpecV1, pairwise: bool) -> u64 {
    let items = eval.n as u64 + eval.regression_items as u64 + eval.safety_items as u64;
    let mut jobs = items;
    if palw_improvement_has_stage_v1(eval, PalwScoringKindV1::Judge) {
        jobs += 2 * eval.n as u64;
    }
    if pairwise && palw_improvement_has_stage_v1(eval, PalwScoringKindV1::Pairwise) {
        jobs += 4 * eval.n as u64;
    }
    jobs
}

/// **The policy check** (spec 17 §17.4.3): `Err` names the first rule a policy breaks. `lifecycle` is the
/// ruleset's unchallenged claim lifecycle bound (`palw_improvement_claim_lifecycle_v1`, mirrored in the state params): `Some`
/// asks rows 19 and 20, which hold the epoch's windows to it; `None` — a fixture that does not set it — does not.
pub fn palw_improvement_policy_check_v1(
    policy: &PalwImprovementPolicyV1,
    ceilings: &PalwImprovementCeilingsV1,
    lifecycle: Option<u64>,
) -> Result<(), &'static str> {
    // 1
    if policy.version != PALW_IMPROVEMENT_POLICY_VERSION_V1 {
        return Err("the policy's version is not 1");
    }
    let bytes = borsh::to_vec(policy).map_err(|_| "the policy does not encode")?;
    if bytes.len() as u64 > ceilings.max_policy_bytes as u64 {
        return Err("the policy is larger than the network's max_policy_bytes");
    }
    // 2
    if policy.usage.value == 0 {
        return Err("a usage threshold of zero opens an epoch at every boundary");
    }
    // 3
    let w = &policy.windows;
    for window in [w.grid, w.w_collect, w.w_submit, w.w_holdout, w.w_eval, w.beacon_delay, w.court_margin] {
        if window == 0 || window > PALW_IMPROVE_MAX_WINDOW_DAA_V1 {
            return Err("every window must be between 1 and 2^32 DAA");
        }
    }
    // [E18] Strictly longer than the epoch, so an epoch that runs to `t_score` ends before the next
    // boundary it could open at.
    if w.grid <= palw_improvement_epoch_length_v1(w) {
        return Err("grid must exceed the epoch's length (w_collect + w_submit + w_holdout + w_eval + court_margin)");
    }
    // 4 [E12]
    if w.w_eval <= w.beacon_delay.saturating_add(PALW_IMPROVE_MIN_CLAIM_WINDOW_DAA_V1) {
        return Err("w_eval must exceed beacon_delay by more than 32 DAA, or the draw leaves no claiming window (E12)");
    }
    let e = &policy.eval;
    // 5, 6
    if e.stages.is_empty() || e.stages.len() > PALW_IMPROVE_MAX_STAGES_V1 {
        return Err("a policy has 1 to 8 scoring stages");
    }
    let mut seen = Vec::with_capacity(e.stages.len());
    for PalwScoringStageV1 { kind, params } in &e.stages {
        if params.kind() != *kind {
            return Err("a scoring stage's parameters are not its kind's");
        }
        if seen.contains(kind) {
            return Err("a scoring kind appears twice");
        }
        seen.push(*kind);
        match *params {
            PalwScoringParamsV1::ExactMatch { open, close, key_cap } => {
                if key_cap == 0 || key_cap > e.max_new_tokens || open < -1 || close < -1 {
                    return Err("ExactMatch needs 1 ≤ key_cap ≤ max_new_tokens and delimiters ≥ −1");
                }
            }
            PalwScoringParamsV1::RefLogLik { logit_scale_q24 } => {
                if logit_scale_q24 <= 0 {
                    return Err("RefLogLik needs a positive logit scale");
                }
            }
            PalwScoringParamsV1::Judge { lo, hi } => {
                if lo >= hi {
                    return Err("Judge needs lo < hi");
                }
            }
            PalwScoringParamsV1::Pairwise { margin } => {
                if margin < 0 {
                    return Err("Pairwise needs a margin of at least 0");
                }
            }
        }
    }
    if !seen.iter().any(|kind| kind.is_primary()) {
        return Err("a policy needs a primary stage (ExactMatch or RefLogLik)");
    }
    // 7
    let judged = seen.contains(&PalwScoringKindV1::Judge) || seen.contains(&PalwScoringKindV1::Pairwise);
    if judged {
        if e.judge_set.is_empty() || e.judge_set.len() > PALW_IMPROVE_MAX_JUDGES_V1 {
            return Err("a Judge or Pairwise stage needs 1 to 16 judge classes");
        }
        let mut sorted = e.judge_set.clone();
        sorted.sort();
        sorted.dedup();
        if sorted.len() != e.judge_set.len() {
            return Err("the judge set names a class twice");
        }
    } else if !e.judge_set.is_empty() {
        return Err("a judge set without a Judge or Pairwise stage");
    }
    if e.anchor_floor_permille > 1000 {
        return Err("anchor_floor_permille is at most 1,000");
    }
    // 7b (RFC-0004 §7.3 as decided 2026-09-30): each judged stage carries the judge's specification.
    for (kind, spec, names) in [
        (PalwScoringKindV1::Judge, &e.judge, &JUDGE_SPEC_REFUSALS),
        (PalwScoringKindV1::Pairwise, &e.pairwise, &PAIRWISE_SPEC_REFUSALS),
    ] {
        match (seen.contains(&kind), spec) {
            (true, None) => return Err(names.missing),
            (false, Some(_)) => return Err(names.without_stage),
            (false, None) => {}
            (true, Some(spec)) => {
                if spec.template_dataset == Hash64::default() {
                    return Err(names.template);
                }
                let ids = 1..=PALW_IMPROVE_MAX_VERDICT_IDS_V1;
                if !ids.contains(&spec.verdict_a.len()) || !ids.contains(&spec.verdict_b.len()) {
                    return Err(names.verdict_len);
                }
                if spec.verdict_a == spec.verdict_b {
                    return Err(names.verdict_same);
                }
                if spec.logit_scale_q24 <= 0 {
                    return Err(names.scale);
                }
            }
        }
    }
    // 8, 9
    let n_cap = (ceilings.max_items_per_epoch).min(PALW_IMPROVE_SIGN_N_MAX_V1);
    if e.n_min == 0 || e.n_min > e.n || e.n > n_cap {
        return Err("n needs 1 ≤ n_min ≤ n ≤ min(max_items_per_epoch, 2048)");
    }
    if e.regression_items > PALW_IMPROVE_MAX_SUITE_ITEMS_V1 || e.safety_items > PALW_IMPROVE_MAX_SUITE_ITEMS_V1 {
        return Err("a suite has at most 1,024 items");
    }
    if (e.regression_items == 0) != (e.regression_dataset == Hash64::default())
        || (e.safety_items == 0) != (e.safety_dataset == Hash64::default())
    {
        return Err("a suite with items names its dataset, and a suite without items names none");
    }
    if e.n as u64 + e.regression_items as u64 + e.safety_items as u64 > ceilings.max_items_per_epoch as u64 {
        return Err("n and the suites together exceed max_items_per_epoch");
    }
    // 10
    if e.delta_permille > 1000 || e.epsilon_permille > 1000 || e.epsilon_safety_permille > 1000 {
        return Err("δ, ε and ε_s are at most 1,000 permille");
    }
    // 11 [P1]
    if !palw_improve_sign_alpha_is_tabled_v1(e.alpha_permille) {
        return Err("α must be one of the sign table's levels (10 or 50 permille)");
    }
    // 12
    if e.max_new_tokens == 0 || e.max_new_tokens > PALW_IMPROVE_MAX_NEW_TOKENS_V1 {
        return Err("max_new_tokens is 1 to 4,096");
    }
    if e.stop_ids.len() > PALW_IMPROVE_MAX_STOP_IDS_V1 {
        return Err("at most 16 stop ids");
    }
    if e.setter_cap_permille == 0 || e.setter_cap_permille > 1000 {
        return Err("setter_cap_permille is 1 to 1,000");
    }
    // 13
    if e.max_eval_positions == 0 || e.max_eval_positions > ceilings.max_eval_positions_per_epoch {
        return Err("max_eval_positions is 1 to the network's max_eval_positions_per_epoch");
    }
    // 13b: the product bound — an epoch this policy opens must fit the network's live results.
    if palw_improvement_results_bound_v1(policy) > ceilings.max_live_results as u64 {
        return Err("(n + suites) × (k_max + 2) exceeds the network's max_live_results");
    }
    // 14
    let k_cap = (ceilings.max_candidates_per_epoch as u32).min(PALW_IMPROVE_SIGN_K_MAX_V1);
    if policy.k_max == 0 || policy.k_max as u32 > k_cap {
        return Err("k_max is 1 to min(max_candidates_per_epoch, 64)");
    }
    // 15
    for share in [policy.phi_permille, policy.bounty_share_permille, policy.promotion_share_permille, policy.s2_trainer_permille] {
        if share > 1000 {
            return Err("a share is at most 1,000 permille");
        }
    }
    for cap in [policy.s2_dataset_cap_permille, policy.s2_contributor_cap_permille] {
        if cap == 0 || cap > 1000 {
            return Err("an S2 cap is 1 to 1,000 permille");
        }
    }
    // 16
    let all = PalwTeacherClassV1::ALL.iter().fold(0u8, |mask, class| mask | class.bit());
    let p = &policy.provenance;
    if p.teacher_classes == 0 || p.teacher_classes & !all != 0 {
        return Err("the teacher-class mask is non-zero and within the six defined classes");
    }
    if p.licence_classes.len() > PALW_IMPROVE_MAX_LICENCE_CLASSES_V1 {
        return Err("at most 32 licence classes");
    }
    let mut licences = p.licence_classes.clone();
    licences.sort();
    licences.dedup();
    if licences.len() != p.licence_classes.len() {
        return Err("a licence class appears twice");
    }
    if p.base_licence_class == Hash64::default() {
        return Err("the base model's licence class must be declared");
    }
    // 17
    if policy.vest_epochs == 0 || policy.vest_epochs > PALW_IMPROVE_MAX_VEST_EPOCHS_V1 {
        return Err("vest_epochs is 1 to 64");
    }
    if policy.rollback_epochs > PALW_IMPROVE_MAX_ROLLBACK_EPOCHS_V1 {
        return Err("rollback_epochs is at most 16");
    }
    if policy.ban_epochs > PALW_IMPROVE_MAX_BAN_EPOCHS_V1 {
        return Err("ban_epochs is at most 256");
    }
    // 18
    if e.seat_pool_permille > ceilings.max_eval_seat_permille {
        return Err("seat_pool_permille is at most the network's max_eval_seat_permille");
    }
    // 19, 20: the epoch's windows against the ruleset's unchallenged claim lifecycle (RFC-0004 §13 as decided 2026-09-30,
    // refined the same day: the court window is not in it — a claim not Final at the decision is a missing evaluation).
    if let Some(lifecycle) = lifecycle {
        if w.court_margin < lifecycle {
            return Err(
                "court_margin is shorter than the ruleset's unchallenged claim lifecycle (anchor, licence and finalization): a claim accepted late could never reach Final in time",
            );
        }
        if judged && w.w_eval <= w.beacon_delay.saturating_add(lifecycle).saturating_add(PALW_IMPROVE_MIN_CLAIM_WINDOW_DAA_V1) {
            return Err(
                "a judged policy's w_eval must exceed beacon_delay, the claim lifecycle and 32 DAA: a judge reads final generations, so the epoch evaluates in two rounds",
            );
        }
    }
    Ok(())
}

/// The refusals of one judged stage's specification, by name.
struct JudgeSpecRefusalsV1 {
    missing: &'static str,
    without_stage: &'static str,
    template: &'static str,
    verdict_len: &'static str,
    verdict_same: &'static str,
    scale: &'static str,
}

const JUDGE_SPEC_REFUSALS: JudgeSpecRefusalsV1 = JudgeSpecRefusalsV1 {
    missing: "a Judge stage needs a judge specification (template dataset, verdicts, logit scale)",
    without_stage: "a judge specification without a Judge stage",
    template: "a judge specification names no template dataset",
    verdict_len: "a judge's verdict sequences are 1 to 8 ids each",
    verdict_same: "a judge's two verdict sequences are the same",
    scale: "a judge specification needs a positive logit scale",
};

const PAIRWISE_SPEC_REFUSALS: JudgeSpecRefusalsV1 = JudgeSpecRefusalsV1 {
    missing: "a Pairwise stage needs a pairwise specification (template dataset, verdicts, logit scale)",
    without_stage: "a pairwise specification without a Pairwise stage",
    template: "a pairwise specification names no template dataset",
    verdict_len: "a pairwise judge's verdict sequences are 1 to 8 ids each",
    verdict_same: "a pairwise judge's two verdict sequences are the same",
    scale: "a pairwise specification needs a positive logit scale",
};

/// **The owner's policy message** (spec 17 §17.4.2): `H("misaka-palw/improve/policy-set/v1",
/// network_domain ‖ line_id ‖ LE u64 sequence ‖ (0x00 | 0x01 ‖ policy_digest))`.
pub fn palw_improvement_policy_message_v1(
    network_domain: Hash64,
    line_id: &Hash64,
    sequence: u64,
    policy: Option<&PalwImprovementPolicyV1>,
) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_IMPROVE_POLICY_SET_DOMAIN_V1).to_state();
    state.update(network_domain.as_byte_slice());
    state.update(line_id.as_byte_slice());
    state.update(&sequence.to_le_bytes());
    match policy {
        None => {
            state.update(&[0u8]);
        }
        Some(policy) => {
            state.update(&[1u8]);
            state.update(palw_improvement_policy_digest_v1(policy).as_byte_slice());
        }
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **A rollback's message** (spec 17 §17.10.1): `H("misaka-palw/improve/rollback/v1", network_domain ‖
/// borsh(payload))`, signed by the filer bond's key.
pub fn palw_improvement_rollback_message_v1(
    network_domain: Hash64,
    payload: &crate::palw_improve_state_v1::PalwLineageRollbackV1,
) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_IMPROVE_ROLLBACK_DOMAIN_V1).to_state();
    state.update(network_domain.as_byte_slice());
    state.update(&borsh::to_vec(payload).expect("a rollback is borsh-serializable"));
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **An example policy** that passes the check under the drill's ceilings — the drills' and the
/// vectors' default. Its numbers are illustrations, not recommendations (RFC-0004 open question 4).
pub fn palw_improvement_policy_example_v1() -> PalwImprovementPolicyV1 {
    let root = |byte: u8| Hash64::from_bytes([byte; 64]);
    PalwImprovementPolicyV1 {
        version: PALW_IMPROVEMENT_POLICY_VERSION_V1,
        usage: PalwUsageThresholdV1 { measure: PalwUsageMeasureV1::Claims, value: 100 },
        windows: PalwEpochWindowsV1 {
            grid: 1_000,
            w_collect: 200,
            w_submit: 200,
            w_holdout: 100,
            w_eval: 300,
            beacon_delay: 10,
            court_margin: 150,
        },
        eval: PalwEvalSpecV1 {
            stages: vec![
                PalwScoringStageV1 {
                    kind: PalwScoringKindV1::ExactMatch,
                    params: PalwScoringParamsV1::ExactMatch { open: -1, close: -1, key_cap: 64 },
                },
                PalwScoringStageV1 {
                    kind: PalwScoringKindV1::RefLogLik,
                    params: PalwScoringParamsV1::RefLogLik { logit_scale_q24: 1 << 24 },
                },
            ],
            regression_dataset: root(0x51),
            regression_items: 32,
            safety_dataset: root(0x52),
            safety_items: 16,
            judge_set: Vec::new(),
            judge: None,
            pairwise: None,
            seat_pool_permille: 100,
            anchor_floor_permille: 900,
            n: 256,
            n_min: 64,
            delta_permille: 20,
            epsilon_permille: 50,
            epsilon_safety_permille: 0,
            alpha_permille: 50,
            max_new_tokens: 256,
            stop_ids: vec![2],
            setter_cap_permille: 250,
            max_eval_positions: 1 << 24,
        },
        k_max: 4,
        fees: PalwImprovementFeesV1 {
            registration_fee: 100_000_000,
            candidate_bond: 1_000_000_000,
            eval_fee_per_job: 100_000,
            hard_case_fee: 1_000_000,
            artifact_bond: 10_000_000,
            setter_bond: 100_000_000,
            dataset_bond: 100_000_000,
            s1_bounty: 10_000_000,
            s1_setter_reward: 10_000_000,
        },
        phi_permille: 100,
        bounty_share_permille: 100,
        promotion_share_permille: 250,
        s2_trainer_permille: 500,
        s2_dataset_cap_permille: 250,
        s2_contributor_cap_permille: 400,
        provenance: PalwProvenancePolicyV1 {
            teacher_classes: PalwTeacherClassV1::OpenDistill.bit()
                | PalwTeacherClassV1::SelfPlay.bit()
                | PalwTeacherClassV1::PublicData.bit(),
            licence_classes: vec![root(0x53)],
            full_weight_candidates: false,
            base_licence_class: root(0x54),
        },
        rollback_epochs: 2,
        vest_epochs: 4,
        ban_epochs: 8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_improve_v1::PALW_DRILL_IMPROVE_CEILINGS_V1;

    fn check(edit: impl Fn(&mut PalwImprovementPolicyV1)) -> Result<(), &'static str> {
        let mut policy = palw_improvement_policy_example_v1();
        edit(&mut policy);
        palw_improvement_policy_check_v1(&policy, &PALW_DRILL_IMPROVE_CEILINGS_V1, None)
    }

    #[test]
    fn the_example_passes_and_every_rule_refuses_by_name() {
        assert_eq!(check(|_| {}), Ok(()));
        let refusals: Vec<(&str, Box<dyn Fn(&mut PalwImprovementPolicyV1)>)> = vec![
            ("version", Box::new(|p| p.version = 2)),
            ("usage", Box::new(|p| p.usage.value = 0)),
            ("zero window", Box::new(|p| p.windows.w_submit = 0)),
            ("grid below the epoch", Box::new(|p| p.windows.grid = 900)),
            ("grid equal to the epoch (E18)", Box::new(|p| p.windows.grid = 950)),
            ("E12", Box::new(|p| p.windows.w_eval = p.windows.beacon_delay + 32)),
            ("no stage", Box::new(|p| p.eval.stages.clear())),
            ("params of another kind", Box::new(|p| p.eval.stages[0].kind = PalwScoringKindV1::RefLogLik)),
            ("a kind twice", Box::new(|p| p.eval.stages[1] = p.eval.stages[0])),
            (
                "key_cap past max_new",
                Box::new(|p| p.eval.stages[0].params = PalwScoringParamsV1::ExactMatch { open: -1, close: -1, key_cap: 257 }),
            ),
            (
                "no primary",
                Box::new(|p| {
                    p.eval.stages = vec![PalwScoringStageV1 {
                        kind: PalwScoringKindV1::Judge,
                        params: PalwScoringParamsV1::Judge { lo: 0, hi: 1 },
                    }];
                    p.eval.judge_set = vec![Hash64::from_bytes([9; 64])];
                }),
            ),
            (
                "judge without a set",
                Box::new(|p| {
                    p.eval.stages.push(PalwScoringStageV1 {
                        kind: PalwScoringKindV1::Judge,
                        params: PalwScoringParamsV1::Judge { lo: 0, hi: 1 },
                    })
                }),
            ),
            ("a set without a judge", Box::new(|p| p.eval.judge_set = vec![Hash64::from_bytes([9; 64])])),
            ("n_min above n", Box::new(|p| p.eval.n_min = p.eval.n + 1)),
            ("n past the ceiling", Box::new(|p| p.eval.n = 1_025)),
            ("suite without a dataset", Box::new(|p| p.eval.regression_dataset = Hash64::default())),
            (
                "items past the ceiling",
                Box::new(|p| {
                    p.eval.n = 1_000;
                    p.eval.regression_items = 32;
                }),
            ),
            ("δ past 1000", Box::new(|p| p.eval.delta_permille = 1_001)),
            ("α untabled", Box::new(|p| p.eval.alpha_permille = 25)),
            ("max_new zero", Box::new(|p| p.eval.max_new_tokens = 0)),
            ("κ zero", Box::new(|p| p.eval.setter_cap_permille = 0)),
            ("positions past the ceiling", Box::new(|p| p.eval.max_eval_positions = (1 << 32) + 1)),
            ("k_max past the ceiling", Box::new(|p| p.k_max = 9)),
            ("φ past 1000", Box::new(|p| p.phi_permille = 1_001)),
            ("S2 cap zero", Box::new(|p| p.s2_dataset_cap_permille = 0)),
            ("teacher mask", Box::new(|p| p.provenance.teacher_classes = 0x40)),
            ("licence twice", Box::new(|p| p.provenance.licence_classes = vec![Hash64::from_bytes([1; 64]); 2])),
            ("base licence", Box::new(|p| p.provenance.base_licence_class = Hash64::default())),
            ("vesting zero", Box::new(|p| p.vest_epochs = 0)),
            ("rollback past 16", Box::new(|p| p.rollback_epochs = 17)),
            ("ban past 256", Box::new(|p| p.ban_epochs = 257)),
        ];
        for (name, edit) in refusals {
            assert!(check(edit).is_err(), "{name} must be refused");
        }
        assert_eq!(check(|p| p.windows.w_eval = p.windows.beacon_delay + 33), Ok(()), "33 DAA of claiming is enough");
    }

    #[test]
    fn the_message_binds_the_sequence_and_the_policy() {
        let domain = Hash64::from_bytes([1; 64]);
        let line = Hash64::from_bytes([2; 64]);
        let policy = palw_improvement_policy_example_v1();
        let a = palw_improvement_policy_message_v1(domain, &line, 1, Some(&policy));
        assert_ne!(a, palw_improvement_policy_message_v1(domain, &line, 2, Some(&policy)), "the sequence");
        assert_ne!(a, palw_improvement_policy_message_v1(domain, &line, 1, None), "an opt-out");
        assert_ne!(a, palw_improvement_policy_message_v1(Hash64::from_bytes([3; 64]), &line, 1, Some(&policy)), "the network");
        let mut other = policy.clone();
        other.k_max = 3;
        assert_ne!(a, palw_improvement_policy_message_v1(domain, &line, 1, Some(&other)), "the policy");
    }

    #[test]
    fn the_epoch_length_and_the_jobs_bound() {
        let policy = palw_improvement_policy_example_v1();
        assert_eq!(palw_improvement_epoch_length_v1(&policy.windows), 950);
        assert_eq!(palw_improvement_jobs_per_subject_v1(&policy.eval, true), 256 + 32 + 16, "[D1] one job per item");
        let mut judged = policy.eval.clone();
        judged.stages.push(PalwScoringStageV1 { kind: PalwScoringKindV1::Judge, params: PalwScoringParamsV1::Judge { lo: 0, hi: 9 } });
        judged.stages.push(PalwScoringStageV1 { kind: PalwScoringKindV1::Pairwise, params: PalwScoringParamsV1::Pairwise { margin: 0 } });
        assert_eq!(palw_improvement_jobs_per_subject_v1(&judged, true), 304 + 256 + 256, "a candidate: + n judged + n pairwise");
        assert_eq!(palw_improvement_jobs_per_subject_v1(&judged, false), 304 + 256, "the parent has no pairwise job");
    }
}
