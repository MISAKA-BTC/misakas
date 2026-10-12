//! **RFC-0004 §0 on the kernel route: the epoch pins its kernels, a candidate binds one, scores carry their assurance, and the
//! promotion keeps its statistics apart from its computational error.**
//!
//! * [`EpochKernelPolicyV1`] is fixed when an epoch opens (§4 `Open`): the descriptors whose claims may be graded, the evaluation
//!   composition (task, tokenizer, output schema, score definition) and the cross-kernel pairs a composition profile allows. Its
//!   digest is what the epoch commits; a change is a new epoch's policy, never a mid-epoch reinterpretation — a descriptor
//!   deprecated after `Open` still grades this epoch's claims under the pinned rule, and one activated after it grades nothing here.
//! * [`admit_candidate_v1`] (§6.1): a candidate is a NEW class admitted under its pinned active kernel (`ELIGIBLE_AT`), of the parent's
//!   family — the same tokenizer/input schema and output interface — under a permitted descriptor; a kernel different from the
//!   parent's needs a pinned composition profile for that pair, so changing kernels cannot change the epoch's score definition.
//! * [`EvaluationResultV1`] carries the claim, the item, the subject and the score with its [`AssuranceModeV1`]; a result whose
//!   descriptor the epoch did not pin is refused (an invalid suite change), and a missing evaluation counts for the incumbent (§7.2).
//! * [`promotion_decision_v1`] (§7.5): `n ≥ n_min`, `b − c ≥ δ·n` and an **exact one-sided sign test** at `α/K` from an integer
//!   binomial table — no float — and, separately, the union-bound computational error of the claims it rests on
//!   ([`crate::assurance::promotion_error_bits_v1`]). The two are never combined into one number (RFC-0004 §0, fourth bullet).

use std::collections::BTreeMap;

use borsh::{BorshDeserialize, BorshSerialize};

use crate::assurance::{AssuranceModeV1, EvaluationEvidenceLabelV1, TrustLabelV1, promotion_error_bits_v1};
use crate::descriptor::ModelKernelBindingV1;
use crate::hash::{Digest, object_id};
use crate::outcome::RegistrationOutcomeV1;

pub const EPOCH_KERNEL_POLICY_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/improve/epoch-policy/v1";

/// What every subject of an epoch is graded on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct EvaluationCompositionV1 {
    pub task_root: Digest,
    pub tokenizer_or_input_schema_root: Digest,
    pub task_output_schema: Digest,
    /// The scoring pipelines and their weights, the regression and safety suites (§3 `eval_spec`).
    pub score_definition_root: Digest,
}

/// The policy an epoch commits when it opens.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct EpochKernelPolicyV1 {
    pub line: Digest,
    pub epoch: u64,
    pub opened_daa: u64,
    /// Descriptors whose claims this epoch grades (each was active at `opened_daa`).
    pub permitted_descriptors: Vec<Digest>,
    pub composition: EvaluationCompositionV1,
    /// `(parent descriptor, candidate descriptor)` pairs a reviewed composition profile allows across kernels.
    pub cross_kernel_pairs: Vec<(Digest, Digest)>,
}

impl EpochKernelPolicyV1 {
    pub fn digest(&self) -> Digest {
        object_id(EPOCH_KERNEL_POLICY_DOMAIN_V1, self)
    }

    pub fn permits(&self, descriptor: &Digest) -> bool {
        self.permitted_descriptors.contains(descriptor)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CandidateRefusalV1 {
    #[error("the candidate is not a class admitted under an active kernel: {0}")]
    NotAdmitted(String),
    #[error("the candidate is the parent's class, not a new one")]
    NotNew,
    #[error("the candidate's kernel is not one this epoch pinned")]
    KernelNotPinned,
    #[error("the candidate changes the kernel without a pinned composition profile for the pair")]
    CrossKernelWithoutProfile,
    #[error("the candidate is not of the parent's family: {0}")]
    OtherFamily(&'static str),
}

/// **May `candidate` enter this epoch against `parent`?** `admission`: the candidate's static outcome under its kernel.
pub fn admit_candidate_v1(
    policy: &EpochKernelPolicyV1,
    parent: &ModelKernelBindingV1,
    candidate: &ModelKernelBindingV1,
    admission: &RegistrationOutcomeV1,
) -> Result<(), CandidateRefusalV1> {
    match admission {
        RegistrationOutcomeV1::EligibleAt { descriptor, plan_root, .. }
            if *descriptor == candidate.descriptor_digest && *plan_root == candidate.plan_root => {}
        RegistrationOutcomeV1::EligibleAt { .. } => {
            return Err(CandidateRefusalV1::NotAdmitted("admitted under another kernel or plan".into()));
        }
        other => return Err(CandidateRefusalV1::NotAdmitted(other.code().into())),
    }
    if candidate.class_binding_id() == parent.class_binding_id() {
        return Err(CandidateRefusalV1::NotNew);
    }
    if !policy.permits(&candidate.descriptor_digest) {
        return Err(CandidateRefusalV1::KernelNotPinned);
    }
    if candidate.descriptor_digest != parent.descriptor_digest
        && !policy.cross_kernel_pairs.contains(&(parent.descriptor_digest, candidate.descriptor_digest))
    {
        return Err(CandidateRefusalV1::CrossKernelWithoutProfile);
    }
    let c = &policy.composition;
    if candidate.tokenizer_or_input_schema_root != parent.tokenizer_or_input_schema_root
        || candidate.tokenizer_or_input_schema_root != c.tokenizer_or_input_schema_root
    {
        return Err(CandidateRefusalV1::OtherFamily("tokenizer / input schema"));
    }
    if candidate.task_output_schema != parent.task_output_schema || candidate.task_output_schema != c.task_output_schema {
        return Err(CandidateRefusalV1::OtherFamily("output interface"));
    }
    if candidate.context_and_state_policy.max_positions < parent.context_and_state_policy.max_positions {
        return Err(CandidateRefusalV1::OtherFamily("a shorter context than the parent's"));
    }
    Ok(())
}

/// Who an evaluation job evaluated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub enum SubjectV1 {
    Parent,
    Candidate(Digest),
}

/// One Final evaluation claim's committed score.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvaluationResultV1 {
    pub claim_id: Digest,
    pub item: u32,
    pub subject: SubjectV1,
    /// The committed score (ExactMatch: 1 pass / 0 fail; RefLogLik: the Q24 log-likelihood).
    pub score: i64,
    pub mode: AssuranceModeV1,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EvaluationRefusalV1 {
    #[error("claim {0:?}: graded under a kernel this epoch did not pin (an invalid suite change)")]
    UnpinnedKernel(Digest),
    #[error("claim {0:?}: a legacy result has no stated computational bound on this route")]
    NoBound(Digest),
}

/// The promotion rule's pinned parameters (§3 `eval_spec`): `δ = delta_num / delta_den`, `α = alpha_num / alpha_den`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PromotionRuleV1 {
    pub n_min: u32,
    pub delta_num: u64,
    pub delta_den: u64,
    pub alpha_num: u64,
    pub alpha_den: u64,
    /// The candidate count `K` of the Bonferroni split.
    pub candidates: u32,
}

/// The largest decisive count the integer sign-test table covers (`C(m, i)` and `2^m` stay inside `u128`).
pub const SIGN_TEST_MAX_DECISIVE_V1: u32 = 120;

fn binom(m: u32, i: u32) -> u128 {
    let i = i.min(m - i);
    (0..i).fold(1u128, |acc, j| acc * (m - j) as u128 / (j + 1) as u128)
}

/// **The exact one-sided sign test's critical value**: the smallest `k` with `P[X ≥ k] ≤ α'` for `X ~ Bin(m, 1/2)`, where
/// `α' = alpha_num / (alpha_den · K)`, in integers: `Σ_{i≥k} C(m,i) · alpha_den · K ≤ alpha_num · 2^m`. `None` past the table, or
/// when no `k ≤ m` reaches the level (then no candidate is eligible).
pub fn sign_test_critical_v1(m: u32, alpha_num: u64, alpha_den: u64, k_candidates: u32) -> Option<u32> {
    if m == 0 || m > SIGN_TEST_MAX_DECISIVE_V1 {
        return None;
    }
    let lhs_scale = (alpha_den as u128).checked_mul(k_candidates.max(1) as u128)?;
    let rhs = (alpha_num as u128).checked_mul(1u128 << m)?;
    let mut tail = 0u128;
    let mut best = None;
    for k in (0..=m).rev() {
        tail += binom(m, k);
        if tail.checked_mul(lhs_scale).is_some_and(|t| t <= rhs) {
            best = Some(k);
        } else {
            break;
        }
    }
    best
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromotionDecisionV1 {
    pub candidate: Digest,
    /// Items counted, wins, losses.
    pub n: u32,
    pub wins: u32,
    pub losses: u32,
    pub eligible: bool,
    pub why_not: Option<String>,
    /// The union-bound computational error of every result the decision read, in bits; `None` if one rests on no stated bound.
    pub computational_error_bits: Option<u16>,
}

/// **§7.5's rule for one candidate against the head**, over `items`. Higher scores win; a missing candidate result is a loss, a
/// missing parent result a candidate loss as well (missing data can block, never make, a promotion).
pub fn promotion_decision_v1(
    policy: &EpochKernelPolicyV1,
    rule: &PromotionRuleV1,
    candidate: Digest,
    items: &[u32],
    results: &[EvaluationResultV1],
) -> Result<PromotionDecisionV1, EvaluationRefusalV1> {
    let mut by: BTreeMap<(u32, SubjectV1), &EvaluationResultV1> = BTreeMap::new();
    let mut labels = Vec::new();
    for r in results {
        if r.subject != SubjectV1::Parent && r.subject != SubjectV1::Candidate(candidate) {
            continue;
        }
        match r.mode {
            AssuranceModeV1::Probabilistic { descriptor, .. } if !policy.permits(&descriptor) => {
                return Err(EvaluationRefusalV1::UnpinnedKernel(r.claim_id));
            }
            AssuranceModeV1::Legacy => return Err(EvaluationRefusalV1::NoBound(r.claim_id)),
            _ => {}
        }
        // The first Final claim per job is the one (§7.2).
        by.entry((r.item, r.subject)).or_insert(r);
    }
    let (mut wins, mut losses) = (0u32, 0u32);
    for item in items {
        let p = by.get(&(*item, SubjectV1::Parent));
        let c = by.get(&(*item, SubjectV1::Candidate(candidate)));
        for r in [p, c].into_iter().flatten() {
            labels.push(EvaluationEvidenceLabelV1 { claim_id: r.claim_id, label: TrustLabelV1::Verified, mode: r.mode });
        }
        match (p, c) {
            (Some(p), Some(c)) if c.score > p.score => wins += 1,
            (Some(p), Some(c)) if c.score < p.score => losses += 1,
            (Some(_), Some(_)) => {}
            _ => losses += 1,
        }
    }
    let n = items.len() as u32;
    let mut why_not = None;
    if n < rule.n_min {
        why_not = Some(format!("{n} items, fewer than n_min {}", rule.n_min));
    } else if ((wins as i128 - losses as i128) * rule.delta_den as i128) < (rule.delta_num as i128 * n as i128) {
        why_not = Some(format!("b − c = {} is below δ·n", wins as i64 - losses as i64));
    } else {
        match sign_test_critical_v1(wins + losses, rule.alpha_num, rule.alpha_den, rule.candidates) {
            Some(k) if wins >= k => {}
            Some(k) => why_not = Some(format!("{wins} wins of {} decisive, under the critical value {k}", wins + losses)),
            None => why_not = Some(format!("{} decisive items: outside the pinned table or no level reached", wins + losses)),
        }
    }
    Ok(PromotionDecisionV1 {
        candidate,
        n,
        wins,
        losses,
        eligible: why_not.is_none(),
        why_not,
        computational_error_bits: promotion_error_bits_v1(&labels),
    })
}

/// One evaluation claim's prosecutability (RFC-0004's 2026-10-07 amendment): the kernel profile it ran under, and whether the
/// inputs its check and court need become public, authenticated and retrievable once the item is drawn (holdout secrecy **before**
/// the draw is kept).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvaluationProsecutabilityV1 {
    pub claim_id: Digest,
    pub profile: Digest,
    pub inputs_public_after_draw: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ImprovementRewardBlockV1 {
    #[error("the promotion rule did not pass: {0}")]
    NotPromoted(String),
    #[error("a result the decision read has no stated computational bound (a judged/legacy result never decides computation)")]
    NoComputationalBound,
    #[error("evaluation claim {0:?} is not publicly prosecutable: {1}")]
    NotProsecutable(Digest, String),
}

/// **May a promotion enable a new reward?** The sign test (quality) passed, every result it read has a stated computational bound
/// (no jury majority decides whether a computation was right), and **every** evaluation claim of the parent and the candidate can
/// be prosecuted by an ordinary public bond: its inputs are public after the draw and its profile's G14 gate is complete with
/// public material. Quality comparison and computational agreement are separate claims and both must hold.
pub fn improvement_reward_gate_v1(
    decision: &PromotionDecisionV1,
    results: &[EvaluationResultV1],
    prosecutability: &[EvaluationProsecutabilityV1],
    gates: &[crate::public::ProsecutionGateV1],
) -> Result<(), ImprovementRewardBlockV1> {
    if !decision.eligible {
        return Err(ImprovementRewardBlockV1::NotPromoted(decision.why_not.clone().unwrap_or_default()));
    }
    if decision.computational_error_bits.is_none() {
        return Err(ImprovementRewardBlockV1::NoComputationalBound);
    }
    for r in results {
        if r.subject != SubjectV1::Parent && r.subject != SubjectV1::Candidate(decision.candidate) {
            continue;
        }
        let blocked = |why: &str| ImprovementRewardBlockV1::NotProsecutable(r.claim_id, why.into());
        let p = prosecutability.iter().find(|p| p.claim_id == r.claim_id).ok_or_else(|| blocked("no prosecutability record"))?;
        if !p.inputs_public_after_draw {
            return Err(blocked("its inputs stay private after the draw"));
        }
        let gate = gates.iter().find(|g| g.profile == p.profile).ok_or_else(|| blocked("no G14 drill for its profile"))?;
        if !gate.complete() {
            return Err(blocked(&format!("its profile's G14 drill misses {:?}", gate.missing())));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::descriptor::ContextPolicyV1;

    fn binding(descriptor: u8, plan: u8, root: u8) -> ModelKernelBindingV1 {
        ModelKernelBindingV1 {
            descriptor_digest: [descriptor; 64],
            plan_root: [plan; 64],
            program_root: [root; 64],
            artifact_root: [root; 64],
            tokenizer_or_input_schema_root: [20; 64],
            task_output_schema: [21; 64],
            context_and_state_policy: ContextPolicyV1 { max_positions: 4096 },
        }
    }

    fn policy() -> EpochKernelPolicyV1 {
        EpochKernelPolicyV1 {
            line: [1; 64],
            epoch: 3,
            opened_daa: 1_000,
            permitted_descriptors: vec![[10; 64]],
            composition: EvaluationCompositionV1 {
                task_root: [22; 64],
                tokenizer_or_input_schema_root: [20; 64],
                task_output_schema: [21; 64],
                score_definition_root: [23; 64],
            },
            cross_kernel_pairs: vec![],
        }
    }

    fn eligible(b: &ModelKernelBindingV1) -> RegistrationOutcomeV1 {
        RegistrationOutcomeV1::EligibleAt { daa: 1_000, descriptor: b.descriptor_digest, plan_root: b.plan_root, error_bits: 200 }
    }

    #[test]
    fn a_candidate_is_a_new_admitted_class_of_the_parents_family_under_a_pinned_kernel() {
        let (p, parent) = (policy(), binding(10, 1, 1));
        let cand = binding(10, 2, 2);
        admit_candidate_v1(&p, &parent, &cand, &eligible(&cand)).unwrap();
        assert_eq!(admit_candidate_v1(&p, &parent, &parent, &eligible(&parent)), Err(CandidateRefusalV1::NotNew));
        let other_kernel = binding(11, 2, 2);
        assert_eq!(admit_candidate_v1(&p, &parent, &other_kernel, &eligible(&other_kernel)), Err(CandidateRefusalV1::KernelNotPinned));
        let mut p2 = p.clone();
        p2.permitted_descriptors.push([11; 64]);
        assert_eq!(
            admit_candidate_v1(&p2, &parent, &other_kernel, &eligible(&other_kernel)),
            Err(CandidateRefusalV1::CrossKernelWithoutProfile)
        );
        p2.cross_kernel_pairs.push(([10; 64], [11; 64]));
        admit_candidate_v1(&p2, &parent, &other_kernel, &eligible(&other_kernel)).unwrap();
        let mut tok = cand.clone();
        tok.tokenizer_or_input_schema_root = [99; 64];
        assert!(matches!(admit_candidate_v1(&p, &parent, &tok, &eligible(&tok)), Err(CandidateRefusalV1::OtherFamily(_))));
        let not_active = RegistrationOutcomeV1::KernelNotActive { descriptor: [10; 64], status: None };
        assert!(matches!(admit_candidate_v1(&p, &parent, &cand, &not_active), Err(CandidateRefusalV1::NotAdmitted(_))));
        assert!(matches!(
            admit_candidate_v1(&p, &parent, &cand, &eligible(&binding(10, 9, 2))),
            Err(CandidateRefusalV1::NotAdmitted(_))
        ));
    }

    #[test]
    fn the_sign_test_table_is_exact_in_integers() {
        // m = 10, α = 0.05: P[X ≥ 9] = 11/1024 ≈ 0.0107 ≤ 0.05 < P[X ≥ 8] = 56/1024 ≈ 0.0547.
        assert_eq!(sign_test_critical_v1(10, 5, 100, 1), Some(9));
        // Split over K = 2 candidates (α/2 = 0.025): still 9 (0.0107 ≤ 0.025).
        assert_eq!(sign_test_critical_v1(10, 5, 100, 2), Some(9));
        // m = 5 cannot reach 0.01 (P[X ≥ 5] = 1/32 ≈ 0.031).
        assert_eq!(sign_test_critical_v1(5, 1, 100, 1), None);
        assert_eq!(sign_test_critical_v1(121, 5, 100, 1), None, "outside the pinned table");
        assert!(sign_test_critical_v1(120, 5, 100, 1).is_some());
    }

    fn result(item: u32, subject: SubjectV1, score: i64, descriptor: u8) -> EvaluationResultV1 {
        EvaluationResultV1 {
            claim_id: [item as u8; 64],
            item,
            subject,
            score,
            mode: AssuranceModeV1::Probabilistic { descriptor: [descriptor; 64], error_bits: 200 },
        }
    }

    #[test]
    fn promotion_keeps_statistics_and_computational_error_apart_and_refuses_unpinned_suites() {
        let p = policy();
        let rule = PromotionRuleV1 { n_min: 10, delta_num: 1, delta_den: 10, alpha_num: 5, alpha_den: 100, candidates: 1 };
        let cand = SubjectV1::Candidate([7; 64]);
        let items: Vec<u32> = (0..12).collect();
        let mut rs = Vec::new();
        for i in &items {
            rs.push(result(*i, SubjectV1::Parent, 0, 10));
            rs.push(result(*i, cand, if *i < 11 { 1 } else { 0 }, 10));
        }
        let d = promotion_decision_v1(&p, &rule, [7; 64], &items, &rs).unwrap();
        assert!(d.eligible, "{d:?}");
        assert_eq!((d.wins, d.losses), (11, 0));
        assert_eq!(d.computational_error_bits, Some(200 - 5), "24 results: 2^-200 · 24 ≤ 2^-195");
        // A missing candidate evaluation is a loss: drop three and the rule no longer holds.
        let thin: Vec<_> = rs.iter().copied().filter(|r| !(r.subject == cand && r.item < 3)).collect();
        let d = promotion_decision_v1(&p, &rule, [7; 64], &items, &thin).unwrap();
        assert!(!d.eligible && d.losses == 3, "{d:?}");
        // A result graded under a kernel the epoch did not pin.
        let mut bad = rs.clone();
        bad[5] = result(2, cand, 1, 11);
        assert!(matches!(promotion_decision_v1(&p, &rule, [7; 64], &items, &bad), Err(EvaluationRefusalV1::UnpinnedKernel(_))));
        // The pinned policy is the epoch's identity: a change is another digest.
        let mut later = p.clone();
        later.permitted_descriptors.push([11; 64]);
        assert_ne!(p.digest(), later.digest());
    }
}
