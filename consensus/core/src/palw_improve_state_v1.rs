//! **RFC-0004: the Model Improvement Protocol's state-side types** — the line's policy, the governed
//! line's row (`improvement_lines`), the epoch's row (`improvement_epochs`) with its candidates, the
//! evaluation job, scores, promotion and rewards (spec [17](../../../docs/spec/palw/17-model-improvement.md)).
//!
//! The skeleton (step 0) fixes their shapes so every lane builds against one definition. What a lane
//! owns instead lives in its own module: the candidate's wire payload and the composite artifact
//! (`palw_improve_candidate_v1`, `palw_improve_artifact_v1`) and the material payloads
//! (`palw_improve_material_v1`). Nothing here is read by a rule until `Params::palw_improvement_v1`
//! is armed, and both tables are empty on every network, so every root and every carriage is
//! byte-identical to a build without them.

use crate::Hash64;
use crate::palw_state_v2::PalwBondKeyV2;
use borsh::{BorshDeserialize, BorshSerialize};

/// Key of [`palw_improvement_policy_digest_v1`].
pub const PALW_IMPROVE_POLICY_DOMAIN_V1: &[u8] = b"misaka-palw/improve/policy/v1";
/// The policy version this build reads.
pub const PALW_IMPROVEMENT_POLICY_VERSION_V1: u16 = 1;
/// The most head-history entries a line keeps in its row (older ones stay in spec 15's version rows).
pub const PALW_IMPROVE_HEAD_HISTORY_MAX_V1: usize = 64;

fn keyed64(key: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for part in parts {
        state.update(part);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

// ---- the closed vocabularies (RFC-0004 §2.2, §5.2, §5.3, §7.3) ----

/// **Trust labels** (RFC-0004 §2.2): what a reward or a shown quantity rests on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwTrustLabelV1 {
    /// Recomputed by the court or computed by the fold.
    Verified = 1,
    /// A statement someone staked on, challengeable.
    Bonded = 2,
    /// A registered judge's output: deterministic, and gameable.
    Judged = 3,
    /// Not checkable; an assumption.
    Trusted = 4,
}

/// **Verification types of Phase A** (RFC-0004 §5.2). EXEC, CRITIC and PROOF are RFC-0005 Phase C's
/// and have no value here: a v1 decoder refuses their tags.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwVerificationTypeV1 {
    Exact = 1,
    Likelihood = 2,
    Judged = 3,
    Human = 4,
}

/// **Teacher classes** (RFC-0004 §5.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwTeacherClassV1 {
    OpenDistill = 1,
    LicensedDistill = 2,
    SelfPlay = 3,
    Human = 4,
    ToolVerified = 5,
    PublicData = 6,
}

impl PalwTeacherClassV1 {
    pub const ALL: [PalwTeacherClassV1; 6] = [
        PalwTeacherClassV1::OpenDistill,
        PalwTeacherClassV1::LicensedDistill,
        PalwTeacherClassV1::SelfPlay,
        PalwTeacherClassV1::Human,
        PalwTeacherClassV1::ToolVerified,
        PalwTeacherClassV1::PublicData,
    ];

    /// The class's bit in a policy's or a declaration's teacher-class mask.
    pub const fn bit(self) -> u8 {
        1 << (self as u8 - 1)
    }
}

/// **Teaching-artifact kinds of Phase A** (RFC-0004 §5.3). `TestCase`, `Counterexample`,
/// `VerifiedCode`, `ToolTrace` and `FormalProof` need execution and arrive with RFC-0005 Phase C.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwTeachingArtifactKindV1 {
    Answer = 1,
    PreferencePair = 2,
    SyntheticProblem = 3,
    HardCaseVariant = 4,
    RewardSignal = 5,
    Critique = 6,
}

/// **Scoring kinds** (RFC-0004 §7.3). Each is a TIR program of the scoring library, run as a stage of
/// an evaluation pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwScoringKindV1 {
    /// The answer span of an R-seeded generation equals the key: a pass or a fail. Primary.
    ExactMatch = 1,
    /// The teacher-forced log-likelihood of the reference continuation (Q24). Primary for likelihood tasks.
    RefLogLik = 2,
    /// A registered judge class's score of one output. A guard only.
    Judge = 3,
    /// A judge's comparison of the parent's and a candidate's outputs. A guard only.
    Pairwise = 4,
}

impl PalwScoringKindV1 {
    /// May this kind decide a promotion (§7.5)? Judges only guard.
    pub const fn is_primary(self) -> bool {
        matches!(self, PalwScoringKindV1::ExactMatch | PalwScoringKindV1::RefLogLik)
    }
}

// ---- the policy (RFC-0004 §3) ----

/// How the head's usage is counted for the opening trigger (open question 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwUsageMeasureV1 {
    /// Final attempt and free-prompt claims of the head.
    Claims = 1,
    /// The head's credited work leaves.
    WorkLeaves = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwUsageThresholdV1 {
    pub measure: PalwUsageMeasureV1,
    pub value: u128,
}

/// **The epoch's boundaries**, in DAA (RFC-0004 §4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEpochWindowsV1 {
    /// Epochs open only at multiples of `grid`.
    pub grid: u64,
    /// `Open` → `t_fix`.
    pub w_collect: u64,
    /// `Submission` → `t_close`.
    pub w_submit: u64,
    /// `HoldOut` → `t_draw`.
    pub w_holdout: u64,
    /// `Evaluating` → `t_eval`.
    pub w_eval: u64,
    /// `d`: the epoch seed is the beacon at the first block `d` DAA past `t_draw`.
    pub beacon_delay: u64,
    /// The court window after `t_eval` within which evaluation claims must reach `Final`.
    pub court_margin: u64,
}

/// One scoring stage of the evaluation spec.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwScoringStageV1 {
    pub kind: PalwScoringKindV1,
    /// The stage program's `graph_ir_root` in the scoring library.
    pub program_root: Hash64,
}

/// **The evaluation spec** (RFC-0004 §7).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEvalSpecV1 {
    pub stages: Vec<PalwScoringStageV1>,
    pub regression_suite_root: Hash64,
    pub regression_items: u32,
    pub safety_suite_root: Hash64,
    pub safety_items: u32,
    /// The registered judge classes a judge is drawn from.
    pub judge_set: Vec<Hash64>,
    /// A judge whose anchor accuracy falls below this is excluded for the epoch.
    pub anchor_floor_permille: u16,
    pub n: u32,
    pub n_min: u32,
    /// `δ`, in permille of `n`.
    pub delta_permille: u16,
    /// `ε` on the regression suite, and `ε_s` on the safety suite, in permille.
    pub epsilon_permille: u16,
    pub epsilon_safety_permille: u16,
    /// `α`, one of the pinned table's levels, in permille.
    pub alpha_permille: u16,
    /// Generation bounds for `Generate` items.
    pub max_new_tokens: u32,
    pub stop_ids: Vec<u32>,
    /// `κ`: the most a submitter or steward supplies of an epoch's items, in permille.
    pub setter_cap_permille: u16,
}

/// **The fees and bonds** (RFC-0004 §13), in sompi.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwImprovementFeesV1 {
    pub registration_fee: u64,
    pub candidate_bond: u64,
    pub eval_fee_per_job: u64,
    pub hard_case_fee: u64,
    pub artifact_bond: u64,
    pub setter_bond: u64,
    pub dataset_bond: u64,
}

/// **The provenance policy** (RFC-0004 §2.1 check 2, §9).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwProvenancePolicyV1 {
    /// A mask of [`PalwTeacherClassV1::bit`].
    pub teacher_classes: u8,
    /// Licence classes whose content may train this line's derivatives.
    pub licence_classes: Vec<Hash64>,
    /// May a candidate carry full weights (§6.1, open question 13)?
    pub full_weight_candidates: bool,
    /// The base model's licence class, declared by the owner under a bond; it must permit derivatives.
    pub base_licence_class: Hash64,
}

/// **A governed line's policy** (RFC-0004 §3), signed by the owner.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwImprovementPolicyV1 {
    pub version: u16,
    pub usage: PalwUsageThresholdV1,
    pub windows: PalwEpochWindowsV1,
    pub eval: PalwEvalSpecV1,
    pub k_max: u8,
    pub fees: PalwImprovementFeesV1,
    /// `φ`: the share of the owner's market leg routed to the pool, in permille.
    pub phi_permille: u16,
    /// The share of the pool S1 may spend in an epoch, in permille.
    pub bounty_share_permille: u16,
    /// S2: the trainer's share of the winner's epoch reward, and the caps per dataset and contributor.
    pub s2_trainer_permille: u16,
    pub s2_dataset_cap_permille: u16,
    pub s2_contributor_cap_permille: u16,
    pub provenance: PalwProvenancePolicyV1,
    /// `R`: the owner may roll a promotion back within this many epochs.
    pub rollback_epochs: u32,
    /// `v`: rewards vest over this many epochs.
    pub vest_epochs: u32,
    /// A rolled-back winner's submitter may not submit for this many epochs.
    pub ban_epochs: u32,
}

/// **A policy's digest**: the keyed hash of its borsh bytes.
pub fn palw_improvement_policy_digest_v1(policy: &PalwImprovementPolicyV1) -> Hash64 {
    let bytes = borsh::to_vec(policy).expect("a policy is borsh-serializable");
    keyed64(PALW_IMPROVE_POLICY_DOMAIN_V1, &[&bytes])
}

// ---- the governed line's row: `improvement_lines` (RFC-0004 §3, §4) ----

/// Why a head moved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwHeadCauseV1 {
    /// The head the line had when it opted in.
    OptIn = 0,
    Promoted = 1,
    RolledBackByOwner = 2,
    RolledBackByProof = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwLineageHeadEntryV1 {
    pub epoch: u64,
    pub class_id: Hash64,
    pub previous: Option<Hash64>,
    pub daa: u64,
    pub cause: PalwHeadCauseV1,
}

/// **The line's pool** (RFC-0004 §8.5), in sompi.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwImprovementPoolV1 {
    pub balance: u64,
    pub deposited: u128,
    pub paid: u128,
    pub forfeited_in: u128,
    pub refunded: u128,
}

/// **A governed line's row** in `improvement_lines`, keyed by the line id.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwImprovementLineV1 {
    pub line_id: Hash64,
    pub owner: PalwBondKeyV2,
    pub policy: PalwImprovementPolicyV1,
    pub policy_digest: Hash64,
    pub governed_from_daa: u64,
    /// A policy signed during an epoch, in force from the next `Idle`.
    pub pending_policy: Option<Box<PalwImprovementPolicyV1>>,
    /// Opting out takes effect after this epoch and the delay.
    pub opt_out_after_epoch: Option<u64>,
    /// The current head: the line's current version's class (§3).
    pub head: Hash64,
    pub head_history: Vec<PalwLineageHeadEntryV1>,
    /// The head's usage counter when the last epoch opened (the trigger's baseline).
    pub usage_baseline: u128,
    pub next_epoch: u64,
    pub open_epoch: Option<u64>,
    pub pool: PalwImprovementPoolV1,
    /// Submitters barred until an epoch, after a rollback (§7.6).
    pub barred_submitters: Vec<(PalwBondKeyV2, u64)>,
}

// ---- the epoch's row: `improvement_epochs` (RFC-0004 §4) ----

/// **The epoch's states** (RFC-0004 §4). `Idle` is the line's, not an epoch's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwEpochStateV1 {
    Open = 1,
    Submission = 2,
    HoldOut = 3,
    Drawn = 4,
    Evaluating = 5,
    Scoring = 6,
    Decided = 7,
    Vesting = 8,
    Closed = 9,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEpochTimesV1 {
    pub t_open: u64,
    pub t_fix: u64,
    pub t_close: u64,
    pub t_draw: u64,
    pub t_eval: u64,
}

/// **A candidate as the epoch keeps it** (RFC-0004 §6): what `CandidateSubmitted` writes.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEpochCandidateV1 {
    pub class_id: Hash64,
    pub submitter: PalwBondKeyV2,
    pub artifact: crate::palw_improve_artifact_v1::PalwTirArtifactRefV1,
    /// The digest of the submission's declarations (datasets, licences, teacher classes).
    pub declarations_digest: Hash64,
    pub fees_paid: u64,
    pub bond: u64,
    pub submitted_daa: u64,
}

/// Where an evaluation item came from (RFC-0004 §7.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwItemSourceV1 {
    HoldOut,
    Setter { set_id: Hash64 },
    Regression,
    Safety,
    Anchor,
}

/// **A drawn item** (RFC-0004 §7.1–7.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEvalItemV1 {
    pub item: u32,
    pub case_id: Hash64,
    pub source: PalwItemSourceV1,
    /// [`crate::palw_improve_eval_v1::palw_improve_eval_seed_v1`]: the same for every subject.
    pub seed: Hash64,
    /// The judge drawn for the item, when the spec has judged stages.
    pub judge: Option<Hash64>,
}

/// **The subject of an evaluation job** (RFC-0004 §7.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub enum PalwEvalSubjectV1 {
    Parent,
    Candidate(Hash64),
}

/// **A final score** (RFC-0004 §7.3): a scoring stage's committed output, from a final claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEvalScoreV1 {
    pub job_id: Hash64,
    pub claim: Hash64,
    pub kind: PalwScoringKindV1,
    /// ExactMatch: 0 or 1. RefLogLik: Q24 log-likelihood. Judge: the judge's scalar. Pairwise: −1, 0, 1.
    pub value: i64,
}

/// The scores of one item and subject.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEvalResultV1 {
    pub item: u32,
    pub subject: PalwEvalSubjectV1,
    pub scores: Vec<PalwEvalScoreV1>,
}

/// **An item's paired outcome** for a candidate against the parent (RFC-0004 §7.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwItemOutcomeV1 {
    Win = 1,
    Loss = 2,
    Tie = 3,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPairedCountsV1 {
    pub wins: u32,
    pub losses: u32,
    pub ties: u32,
}

/// **A candidate's counts** (RFC-0004 §7.5): primary, regression, safety and the judge guards.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPromotionCountsV1 {
    pub primary: PalwPairedCountsV1,
    pub regression: PalwPairedCountsV1,
    pub safety: PalwPairedCountsV1,
    pub judge: PalwPairedCountsV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwNoChangeReasonV1 {
    NoCandidate = 1,
    TooFewItems = 2,
    NoneEligible = 3,
    EvaluationIncomplete = 4,
}

/// **The epoch's decision** (RFC-0004 §7.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwPromotionOutcomeV1 {
    Promoted { class_id: Hash64, wins: u32, losses: u32 },
    NoChange { reason: PalwNoChangeReasonV1 },
}

/// **What a reward pays for** (RFC-0004 §8).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwRewardStageV1 {
    S1Bounty = 1,
    S1Setter = 2,
    S2Trainer = 3,
    S2Dataset = 4,
    S3Ablation = 5,
    EvalFee = 6,
}

/// **A grant from the pool** (RFC-0004 §8): who, for what, how much, on what it rests, and its vesting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwRewardGrantV1 {
    pub recipient: PalwBondKeyV2,
    pub stage: PalwRewardStageV1,
    pub amount: u64,
    pub label: PalwTrustLabelV1,
    pub vest_from_epoch: u64,
    pub vest_epochs: u32,
    pub vested: u64,
    pub forfeited: bool,
}

/// **An epoch's row** in `improvement_epochs`, keyed by `(line id, epoch number)`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwImprovementEpochV1 {
    pub line_id: Hash64,
    pub epoch: u64,
    pub state: PalwEpochStateV1,
    pub times: PalwEpochTimesV1,
    /// The head when the epoch opened: every candidate's parent.
    pub parent: Hash64,
    /// The training material admitted while `Open`: a running accumulator and its count.
    pub material_acc: Hash64,
    pub material_count: u32,
    /// Fixed at `t_fix` (RFC-0004 §4).
    pub dataset_root: Option<Hash64>,
    /// Hard cases admitted in `HoldOut`: the evaluation pool.
    pub holdout_cases: Vec<Hash64>,
    /// Setter sets committed before `t_close`.
    pub setter_sets: Vec<Hash64>,
    pub candidates: Vec<PalwEpochCandidateV1>,
    /// The epoch seed: the beacon at the first block `beacon_delay` past `t_draw`.
    pub seed: Option<Hash64>,
    pub items: Vec<PalwEvalItemV1>,
    pub results: Vec<PalwEvalResultV1>,
    pub counts: Vec<(Hash64, PalwPromotionCountsV1)>,
    pub outcome: Option<PalwPromotionOutcomeV1>,
    pub grants: Vec<PalwRewardGrantV1>,
}

// ---- the evaluation job (RFC-0004 §7.2) ----
//
// The job, its mode, its seed and its id are the evaluation lane's (`crate::palw_improve_eval_v1`).

// ---- the payloads of the core lane's own objects (tags 70, 81, 82) ----

/// **`ImprovementPolicySet` (tag 70)**: a line opts in, or changes its policy between epochs.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwImprovementPolicySetV1 {
    pub line_id: Hash64,
    pub policy: PalwImprovementPolicyV1,
}

/// Why a rollback happens (RFC-0004 §7.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwRollbackCauseV1 {
    /// The owner, within `rollback_epochs`, without proof.
    Owner,
    /// A final claim of the promoted class failing a committed canary or safety item.
    CanaryFailed { claim: Hash64, item: Hash64 },
    /// A regression shown on items drawn in a later epoch.
    LaterRegression { epoch: u64 },
    /// A licence violation upheld on a bonded challenge.
    LicenceViolation { challenge: Hash64 },
}

/// **`LineageHeadRolledBack` (tag 81)**: restores the previous head.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwLineageRollbackV1 {
    pub line_id: Hash64,
    /// The epoch whose promotion is rolled back.
    pub epoch: u64,
    /// The head it restores (the promotion's `previous`).
    pub to_class: Hash64,
    pub cause: PalwRollbackCauseV1,
}

/// **`ImprovementPoolFunded` (tag 82)** (RFC-0004 §8.5): a sponsor's deposit into a governed line's
/// pool, bound to its carrier's sink output as a model sink is bound to its `ModelBuy` (PALW-MK-11).
/// Unsigned, like `ModelBuy`: the carrier's inputs pay it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwImprovementPoolFundingV1 {
    pub line_id: Hash64,
    /// The deposit, in sompi: exactly the value of the carrier's sink output at `sink_index`.
    pub amount: u64,
    pub sink_index: u32,
}

/// **Test rows** for the state module's root and carriage suites: one of each table's rows, varied by
/// `seed`, with every field populated.
#[cfg(test)]
pub(crate) mod test_rows {
    use super::*;
    use crate::tx::TransactionOutpoint;

    fn h(seed: u8, lane: u8) -> Hash64 {
        let mut bytes = [seed; 64];
        bytes[0] = lane;
        Hash64::from_bytes(bytes)
    }

    fn bond(seed: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_bytes([seed; 64]), index: seed as u32 })
    }

    pub(crate) fn policy_v1(seed: u8) -> PalwImprovementPolicyV1 {
        PalwImprovementPolicyV1 {
            version: PALW_IMPROVEMENT_POLICY_VERSION_V1,
            usage: PalwUsageThresholdV1 { measure: PalwUsageMeasureV1::Claims, value: 1_000 + seed as u128 },
            windows: PalwEpochWindowsV1 {
                grid: 1_000,
                w_collect: 400,
                w_submit: 400,
                w_holdout: 200,
                w_eval: 600,
                beacon_delay: 10,
                court_margin: 300,
            },
            eval: PalwEvalSpecV1 {
                stages: vec![PalwScoringStageV1 { kind: PalwScoringKindV1::ExactMatch, program_root: h(seed, 1) }],
                regression_suite_root: h(seed, 2),
                regression_items: 64,
                safety_suite_root: h(seed, 3),
                safety_items: 32,
                judge_set: vec![h(seed, 4)],
                anchor_floor_permille: 900,
                n: 256,
                n_min: 64,
                delta_permille: 20,
                epsilon_permille: 10,
                epsilon_safety_permille: 0,
                alpha_permille: 50,
                max_new_tokens: 256,
                stop_ids: vec![2],
                setter_cap_permille: 250,
            },
            k_max: 4,
            fees: PalwImprovementFeesV1 {
                registration_fee: 1_000,
                candidate_bond: 10_000,
                eval_fee_per_job: 10,
                hard_case_fee: 5,
                artifact_bond: 100,
                setter_bond: 1_000,
                dataset_bond: 1_000,
            },
            phi_permille: 100,
            bounty_share_permille: 200,
            s2_trainer_permille: 500,
            s2_dataset_cap_permille: 300,
            s2_contributor_cap_permille: 400,
            provenance: PalwProvenancePolicyV1 {
                teacher_classes: PalwTeacherClassV1::OpenDistill.bit() | PalwTeacherClassV1::PublicData.bit(),
                licence_classes: vec![h(seed, 5)],
                full_weight_candidates: false,
                base_licence_class: h(seed, 6),
            },
            rollback_epochs: 2,
            vest_epochs: 4,
            ban_epochs: 8,
        }
    }

    pub(crate) fn line_v1(seed: u8) -> PalwImprovementLineV1 {
        let policy = policy_v1(seed);
        PalwImprovementLineV1 {
            line_id: h(seed, 10),
            owner: bond(seed),
            policy_digest: palw_improvement_policy_digest_v1(&policy),
            policy,
            governed_from_daa: 5_000,
            pending_policy: None,
            opt_out_after_epoch: None,
            head: h(seed, 11),
            head_history: vec![PalwLineageHeadEntryV1 {
                epoch: 0,
                class_id: h(seed, 11),
                previous: None,
                daa: 5_000,
                cause: PalwHeadCauseV1::OptIn,
            }],
            usage_baseline: 0,
            next_epoch: 1,
            open_epoch: None,
            pool: PalwImprovementPoolV1::default(),
            barred_submitters: Vec::new(),
        }
    }

    pub(crate) fn epoch_v1(seed: u8) -> PalwImprovementEpochV1 {
        PalwImprovementEpochV1 {
            line_id: h(seed, 10),
            epoch: 1,
            state: PalwEpochStateV1::Open,
            times: PalwEpochTimesV1 { t_open: 6_000, t_fix: 6_400, t_close: 6_800, t_draw: 7_000, t_eval: 7_600 },
            parent: h(seed, 11),
            material_acc: h(seed, 12),
            material_count: 1,
            dataset_root: None,
            holdout_cases: Vec::new(),
            setter_sets: Vec::new(),
            candidates: Vec::new(),
            seed: None,
            items: Vec::new(),
            results: Vec::new(),
            counts: Vec::new(),
            outcome: None,
            grants: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn teacher_class_bits_are_distinct_and_in_one_byte() {
        let mut mask = 0u8;
        for class in PalwTeacherClassV1::ALL {
            assert_eq!(mask & class.bit(), 0, "{class:?} shares a bit");
            mask |= class.bit();
        }
        assert_eq!(mask, 0b0011_1111);
    }

    #[test]
    fn only_exact_match_and_likelihood_are_primary() {
        assert!(PalwScoringKindV1::ExactMatch.is_primary());
        assert!(PalwScoringKindV1::RefLogLik.is_primary());
        assert!(!PalwScoringKindV1::Judge.is_primary());
        assert!(!PalwScoringKindV1::Pairwise.is_primary());
    }

}
