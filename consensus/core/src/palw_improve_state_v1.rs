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

/// **A scoring stage's parameters** (spec 17 §17.4.3): the chain derives the stage's program from its
/// kind, these parameters and the subject's shape, with the builders `scoring_set_id` pins (A7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwScoringParamsV1 {
    /// The answer span between `open` and `close` (−1: no delimiter), compared with a key of at most
    /// `key_cap` ids.
    ExactMatch { open: i32, close: i32, key_cap: u32 },
    /// The subject's logits in Q24 nats per unit (the line's output interface).
    RefLogLik { logit_scale_q24: i32 },
    /// The judge's scalar range.
    Judge { lo: i32, hi: i32 },
    /// The preference margin the stage applies inside the circuit.
    Pairwise { margin: i32 },
}

impl PalwScoringParamsV1 {
    /// The kind these parameters belong to.
    pub const fn kind(&self) -> PalwScoringKindV1 {
        match self {
            PalwScoringParamsV1::ExactMatch { .. } => PalwScoringKindV1::ExactMatch,
            PalwScoringParamsV1::RefLogLik { .. } => PalwScoringKindV1::RefLogLik,
            PalwScoringParamsV1::Judge { .. } => PalwScoringKindV1::Judge,
            PalwScoringParamsV1::Pairwise { .. } => PalwScoringKindV1::Pairwise,
        }
    }
}

/// One scoring stage of the evaluation spec.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwScoringStageV1 {
    pub kind: PalwScoringKindV1,
    pub params: PalwScoringParamsV1,
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
    /// The policy's cap on an epoch's evaluation positions (PALW-MIP-20), within the network's ceiling.
    pub max_eval_positions: u64,
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
    /// S1: the bounty for an exact-match case's first matching `Answer` (§17.11.3).
    pub s1_bounty: u64,
    /// S1: the reward for a `SyntheticProblem` or `HardCaseVariant` the head verifiably fails.
    pub s1_setter_reward: u64,
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
    /// S2: the share of the pool a promotion pays out (trainer and data), in permille.
    pub promotion_share_permille: u16,
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

// ---- the governed line's row: `improvement_lines` (spec 17 §17.3.1) ----

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

/// **One head change** (spec 17 §17.4.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwLineageHeadEntryV1 {
    /// The epoch that made the change (0 for the opt-in).
    pub epoch: u64,
    /// The head from this entry on.
    pub class_id: Hash64,
    /// The head before it.
    pub previous: Option<Hash64>,
    pub daa: u64,
    pub cause: PalwHeadCauseV1,
}

/// **The line's pool** (spec 17 §17.11), in sompi. `balance` is spendable; `held` is bonds and
/// escrows owed back or forfeitable; `unvested` is granted and not yet paid. The in/out counters are
/// the conservation ledger (§17.11.5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwImprovementPoolV1 {
    pub balance: u64,
    pub held: u64,
    pub unvested: u64,
    /// What S1 may still pay in the current period (set at each opening).
    pub s1_budget: u64,
    pub deposited: u128,
    pub fees_in: u128,
    pub phi_in: u128,
    pub held_in: u128,
    pub forfeited_in: u128,
    pub paid: u128,
    pub refunded: u128,
}

/// A governed line's status (spec 17 §17.4.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwImprovementLineStatusV1 {
    Governed,
    /// Opting out: no epoch opens; from `effective_daa` the line is no longer governed. `None` while
    /// an epoch is still open (the effective DAA is set when it ends).
    OptingOut {
        effective_daa: Option<u64>,
    },
    /// Opted out and settled: the line's other rows are gone; the header stays so its policy sequence
    /// continues (an old signed opt-in cannot be replayed).
    Dissolved,
}

/// **The material frontier** (spec 17 §17.6.1): an RFC 6962 Merkle tree kept as the roots of its
/// perfect subtrees, largest first — at most 32 hashes for `count < 2^32`.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwMaterialFrontierV1 {
    pub count: u32,
    pub frontier: Vec<Hash64>,
}

/// **A governed line's header** in `improvement_lines`, keyed by the line id (spec 17 §17.3.1). O(1):
/// the policy, the usage counter, the head history, the pool and the material live in their own
/// keyed tables, so a frequent write (a usage count, a fee) journals a few hundred bytes.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwImprovementLineV1 {
    pub line_id: Hash64,
    /// The line's IR class at opt-in (spec 15's `PalwModelLineV1.class_id`).
    pub class_id: Hash64,
    /// The digest of the policy in force (`improvement_policies`).
    pub policy_digest: Hash64,
    /// The sequence of the last accepted policy object (§17.4.2).
    pub policy_sequence: u64,
    pub status: PalwImprovementLineStatusV1,
    pub governed_from_daa: u64,
    /// The head: an IR class id (§17.4.1).
    pub head: Hash64,
    /// How many head entries were ever appended; the history keeps `(line, seq)` for the last
    /// [`PALW_IMPROVE_HEAD_HISTORY_MAX_V1`] of them.
    pub head_seq: u32,
    /// The next epoch's number (epochs count from 1).
    pub next_epoch: u64,
    pub open_epoch: Option<u64>,
    /// When the fold next advances the line (§17.5.2).
    pub next_due_daa: u64,
    /// Submitters barred after a rollback, each with the DAA its bar ends (§17.10.3), at most
    /// [`PALW_IMPROVE_BARRED_MAX_V1`]; an expired bar is pruned when the line is advanced.
    pub barred: Vec<(PalwBondKeyV2, u64)>,
    /// The epoch of the head's latest promotion — kept (with its winner's row) while a rollback can
    /// name it (§17.10).
    pub last_promotion: Option<u64>,
    /// The epoch that ran the regression check of the latest promotion, kept for its proof.
    pub regression_epoch: Option<u64>,
    /// The predecessor the next epoch evaluates as `Previous` (§17.10.2), set at a promotion.
    pub regression_check: Option<Hash64>,
}

/// The most bars a line keeps (the oldest-ending is dropped past it).
pub const PALW_IMPROVE_BARRED_MAX_V1: usize = 16;

/// **A line's policy record** in `improvement_policies` (spec 17 §17.3.1): the policy in force and a
/// pending one.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwImprovementPolicyRecordV1 {
    pub policy: PalwImprovementPolicyV1,
    /// A policy accepted while an epoch was open, in force from the epoch's end.
    pub pending: Option<Box<PalwImprovementPolicyV1>>,
}

/// **A line's usage counter** in `improvement_usage` (spec 17 §17.4.5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwImprovementUsageV1 {
    pub usage: u128,
    pub since_daa: u64,
}

impl PalwImprovementLineV1 {
    /// Is the line governed at `daa` (opted in and not yet out)?
    pub fn governed_at(&self, daa: u64) -> bool {
        match self.status {
            PalwImprovementLineStatusV1::Governed => true,
            PalwImprovementLineStatusV1::OptingOut { effective_daa } => effective_daa.is_none_or(|at| daa < at),
            PalwImprovementLineStatusV1::Dissolved => false,
        }
    }

    /// Is `bond` barred from submitting at `daa`?
    pub fn is_barred(&self, bond: &PalwBondKeyV2, daa: u64) -> bool {
        self.barred.iter().any(|(b, until)| b == bond && daa < *until)
    }
}

// ---- the epoch's row: `improvement_epochs` (spec 17 §17.3.2) ----

/// **The epoch's states** (spec 17 §17.5.1). The line is idle when it has no open epoch; `Vesting` is
/// not a state [E13].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwEpochStateV1 {
    Open = 1,
    Submission = 2,
    HoldOut = 3,
    /// Past `t_draw`, waiting for the first block at `t_draw + beacon_delay` [E3].
    Drawing = 4,
    Evaluating = 5,
    /// Past `t_eval`, waiting for the evaluation claims to finalise, at the latest `t_score` [E4].
    Closing = 6,
    Decided = 7,
}

/// The epoch's times, fixed at its opening (spec 17 §17.5.3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEpochTimesV1 {
    pub t_open: u64,
    pub t_fix: u64,
    pub t_close: u64,
    pub t_draw: u64,
    pub t_eval: u64,
    /// `t_eval + court_margin`: scoring happens at the latest here.
    pub t_score: u64,
}

/// **A candidate as the epoch keeps it** (spec 17 §17.7): what `CandidateSubmitted` writes.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEpochCandidateV1 {
    pub class_id: Hash64,
    pub submitter: PalwBondKeyV2,
    pub artifact: crate::palw_improve_artifact_v1::PalwTirArtifactRefV1,
    /// The digest of the submission's declarations (datasets, licences, teacher classes).
    pub declarations_digest: Hash64,
    /// Registered datasets the candidate declared, with their weights in permille (S2).
    pub datasets: Vec<(Hash64, u16)>,
    /// The registration fee it paid (to the pool's balance).
    pub fee_paid: u64,
    /// Its bond (held).
    pub bond: u64,
    /// Its evaluation escrow (held), and what the executors have been paid from it.
    pub escrow: u64,
    pub escrow_spent: u64,
    pub submitted_daa: u64,
    /// Its counts and eligibility, once scored (§17.9.3).
    pub counts: Option<PalwPromotionCountsV1>,
}

/// Where an evaluation item came from (spec 17 §17.8.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwItemSourceV1 {
    HoldOut,
    Setter { set_id: Hash64, index: u32 },
    Regression { index: u32 },
    Safety { index: u32 },
}

/// **A drawn item** (spec 17 §17.8.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEvalItemV1 {
    pub item: u32,
    /// The case id, the setter item's id or the suite item's id.
    pub case_id: Hash64,
    pub source: PalwItemSourceV1,
    pub supplier: Option<PalwBondKeyV2>,
    /// [`crate::palw_improve_eval_v1::palw_improve_eval_seed_v1`]: the same for every subject.
    pub seed: Hash64,
    /// The judge drawn for the item, when the spec has judged stages.
    pub judge: Option<Hash64>,
    /// Dropped for every subject (a setter that never revealed).
    pub dropped: bool,
}

/// **The subject of an evaluation job** (spec 17 §17.8.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub enum PalwEvalSubjectV1 {
    Parent,
    Candidate(Hash64),
    /// The head's predecessor, in the regression check (§17.10.2).
    Previous(Hash64),
}

/// **A final score** (spec 17 §17.8.3): a scoring stage's committed output. Slim (the reviewer's note):
/// the job id is derivable from `(line, epoch, item, subject, kind)`, and the claim is the evaluation
/// lane's job row's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEvalScoreV1 {
    pub kind: PalwScoringKindV1,
    /// ExactMatch: 1 pass, 0 fail. RefLogLik: the Q24 log-likelihood. Judge: the judge's scalar.
    /// Pairwise: the stage's committed outcome from the candidate's side (+1, 0, −1).
    pub value: i64,
}

/// The scores of one item and subject.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEvalResultV1 {
    pub item: u32,
    pub subject: PalwEvalSubjectV1,
    pub scores: Vec<PalwEvalScoreV1>,
}

/// **An item's paired outcome** for a candidate against the parent (spec 17 §17.9.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwItemOutcomeV1 {
    Win = 1,
    Loss = 2,
    Tie = 3,
}

/// Wins, losses and ties of one count (spec 17 §17.9.3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPairedCountsV1 {
    pub wins: u32,
    pub losses: u32,
    pub ties: u32,
}

impl PalwPairedCountsV1 {
    pub fn add(&mut self, outcome: PalwItemOutcomeV1) {
        match outcome {
            PalwItemOutcomeV1::Win => self.wins += 1,
            PalwItemOutcomeV1::Loss => self.losses += 1,
            PalwItemOutcomeV1::Tie => self.ties += 1,
        }
    }

    /// Every counted item.
    pub fn total(&self) -> u32 {
        self.wins + self.losses + self.ties
    }
}

/// **A candidate's counts** (spec 17 §17.9.3): primary, the two suites and the two guards.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPromotionCountsV1 {
    pub primary: PalwPairedCountsV1,
    pub regression: PalwPairedCountsV1,
    pub safety: PalwPairedCountsV1,
    pub judge: PalwPairedCountsV1,
    pub pairwise: PalwPairedCountsV1,
    /// Whether the candidate met every rule of §17.9.4.
    pub eligible: bool,
}

/// Why an epoch changed nothing (spec 17 §17.9.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwNoChangeReasonV1 {
    NoCandidate = 1,
    TooFewItems = 2,
    NoneEligible = 3,
    /// A rollback aborted the epoch [E11].
    Aborted = 4,
    /// The pool could not cover the parent's evaluation escrow.
    PoolInsufficient = 5,
}

/// **The epoch's decision** (spec 17 §17.9.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwPromotionOutcomeV1 {
    Promoted { class_id: Hash64, wins: u32, losses: u32 },
    NoChange { reason: PalwNoChangeReasonV1 },
}

/// **What a grant pays for** (spec 17 §17.11.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwRewardStageV1 {
    S1Bounty = 1,
    S1Setter = 2,
    S2Trainer = 3,
    S2Dataset = 4,
    /// Reserved: S3 stays on testnets and has no v1 path.
    S3Ablation = 5,
    /// The winner's candidate bond, which vests with its grant.
    WinnerBond = 6,
}

/// **A grant** (spec 17 §17.11.3–4): who, for what, how much, on what it rests, and its vesting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwRewardGrantV1 {
    pub recipient: PalwBondKeyV2,
    pub stage: PalwRewardStageV1,
    pub amount: u64,
    pub label: PalwTrustLabelV1,
    pub vest_from_daa: u64,
    /// The policy's `L_e` when the grant was made.
    pub vest_unit_daa: u64,
    pub vest_epochs: u32,
    pub vested: u64,
    pub forfeited: bool,
}

/// The evaluation escrow the pool pays for the parent and the regression check (spec 17 §17.11.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEpochEscrowV1 {
    pub parent: u64,
    pub parent_spent: u64,
    pub previous: u64,
    pub previous_spent: u64,
}

/// **An epoch's header** in `improvement_epochs`, keyed by `(line id, epoch number)` (spec 17
/// §17.3.2). O(1): candidates, pool entries, items, results and grants live in their own keyed tables
/// under `(line, epoch, …)`, and the header keeps their counts.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwImprovementEpochV1 {
    pub line_id: Hash64,
    pub epoch: u64,
    pub state: PalwEpochStateV1,
    pub times: PalwEpochTimesV1,
    /// The head when the epoch opened: every candidate's parent.
    pub parent: Hash64,
    /// The head's predecessor, when this epoch runs the regression check (§17.10.2).
    pub previous: Option<Hash64>,
    /// The policy the epoch runs under.
    pub policy_digest: Hash64,
    /// Fixed at `t_fix` (§17.6.1).
    pub dataset_root: Option<Hash64>,
    /// `improvement_candidates[(line, epoch, 0..candidates)]`.
    pub candidates: u32,
    /// `improvement_pool_entries[(line, epoch, 0..pool_entries)]`: hold-out cases and setter sets.
    pub pool_entries: u32,
    pub holdout_cases: u32,
    pub setter_sets: u32,
    /// The epoch seed (§17.8.1).
    pub seed: Option<Hash64>,
    /// `improvement_items[(line, epoch, 0..items)]`.
    pub items: u32,
    /// The result rows the epoch reserved against `max_live_results` when it opened:
    /// `(n + regression_items + safety_items) × (k_max + 2)`.
    pub results_bound: u32,
    pub previous_counts: Option<PalwPromotionCountsV1>,
    pub outcome: Option<PalwPromotionOutcomeV1>,
    pub escrow: PalwEpochEscrowV1,
    /// `improvement_grants[(line, epoch, 0..grants)]`.
    pub grants: u32,
    pub decided_daa: Option<u64>,
    /// The retirement sweep's progress once decided (§17.5.4): detail rows below it are gone.
    pub retire: PalwEpochRetireV1,
}

/// **The retirement sweep's cursor** for one decided epoch (spec 17 §17.5.4): pool entries, then
/// items, then results, each in key order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwEpochRetireV1 {
    /// Not decided, or decided and not yet started.
    #[default]
    Pending,
    /// Deleting detail rows; resumes at the next key.
    Sweeping,
    /// Every pool entry, item and result is gone.
    Done,
}

/// **An evaluation-pool entry** in `improvement_pool_entries` (spec 17 §17.6.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwPoolEntryV2 {
    HoldOut { id: Hash64, supplier: PalwBondKeyV2 },
    SetterSet { set_id: Hash64, setter: PalwBondKeyV2, items: u32 },
}

impl PalwImprovementEpochV1 {
    /// Is the epoch past its decision?
    pub fn is_decided(&self) -> bool {
        self.state == PalwEpochStateV1::Decided
    }
}

/// **An open epoch, as a node reads it** (the read door, `ConsensusApi::palw_improvement_open_epochs_v1`):
/// the line's header and policy, the epoch's header, its candidates in acceptance order and its items
/// in item order (dropped ones included). One per open epoch — at most `max_open_epochs`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwImprovementEpochViewV1 {
    pub line: PalwImprovementLineV1,
    pub policy: PalwImprovementPolicyV1,
    pub epoch: PalwImprovementEpochV1,
    pub candidates: Vec<PalwEpochCandidateV1>,
    pub items: Vec<PalwEvalItemV1>,
}

// ---- the evaluation job (RFC-0004 §7.2) ----
//
// The job, its mode, its seed and its id are the evaluation lane's (`crate::palw_improve_eval_v1`);
// the seed is re-exported here for the draw, which names it by this module.
pub use crate::palw_improve_eval_v1::palw_improve_eval_seed_v1;

// ---- the payloads of the core lane's own objects (tags 70, 81, 82) ----

/// **`ModelLineImprovementPolicySet` (tag 70)** (spec 17 §17.4.2): a line opts in (`Some`, no row),
/// changes its policy (`Some`, a row) or opts out (`None`). `sequence` is the row's
/// `policy_sequence + 1` (1 for the opt-in), so an old signed policy cannot be replayed.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwImprovementPolicySetV1 {
    pub line_id: Hash64,
    pub sequence: u64,
    pub policy: Option<PalwImprovementPolicyV1>,
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
        let mut policy = crate::palw_improve_policy_v1::palw_improvement_policy_example_v1();
        policy.usage.value += seed as u128;
        policy
    }

    pub(crate) fn line_v1(seed: u8) -> PalwImprovementLineV1 {
        PalwImprovementLineV1 {
            line_id: h(seed, 10),
            class_id: h(seed, 11),
            policy_digest: palw_improvement_policy_digest_v1(&policy_v1(seed)),
            policy_sequence: 1,
            status: PalwImprovementLineStatusV1::Governed,
            governed_from_daa: 5_000,
            head: h(seed, 11),
            head_seq: 1,
            next_epoch: 2,
            open_epoch: Some(1),
            next_due_daa: 6_400,
            barred: vec![(bond(seed), 9_000)],
            last_promotion: None,
            regression_epoch: None,
            regression_check: None,
        }
    }

    pub(crate) fn policy_record_v1(seed: u8) -> PalwImprovementPolicyRecordV1 {
        PalwImprovementPolicyRecordV1 { policy: policy_v1(seed), pending: None }
    }

    pub(crate) fn usage_v1(seed: u8) -> PalwImprovementUsageV1 {
        PalwImprovementUsageV1 { usage: 3 + seed as u128, since_daa: 5_000 }
    }

    pub(crate) fn head_v1(seed: u8) -> PalwLineageHeadEntryV1 {
        PalwLineageHeadEntryV1 { epoch: 0, class_id: h(seed, 11), previous: None, daa: 5_000, cause: PalwHeadCauseV1::OptIn }
    }

    pub(crate) fn pool_v1(seed: u8) -> PalwImprovementPoolV1 {
        PalwImprovementPoolV1 { balance: 7 + seed as u64, deposited: 7 + seed as u128, ..Default::default() }
    }

    pub(crate) fn material_v1(seed: u8) -> PalwMaterialFrontierV1 {
        PalwMaterialFrontierV1 { count: 1, frontier: vec![h(seed, 12)] }
    }

    pub(crate) fn epoch_v1(seed: u8) -> PalwImprovementEpochV1 {
        PalwImprovementEpochV1 {
            line_id: h(seed, 10),
            epoch: 1,
            state: PalwEpochStateV1::Submission,
            times: PalwEpochTimesV1 { t_open: 6_000, t_fix: 6_400, t_close: 6_800, t_draw: 7_000, t_eval: 7_600, t_score: 7_900 },
            parent: h(seed, 11),
            previous: None,
            policy_digest: h(seed, 13),
            dataset_root: Some(h(seed, 14)),
            candidates: 1,
            pool_entries: 1,
            holdout_cases: 1,
            setter_sets: 0,
            seed: None,
            items: 1,
            results_bound: 3,
            previous_counts: None,
            outcome: None,
            escrow: PalwEpochEscrowV1::default(),
            grants: 1,
            decided_daa: None,
            retire: PalwEpochRetireV1::Pending,
        }
    }

    pub(crate) fn candidate_v1(seed: u8) -> PalwEpochCandidateV1 {
        PalwEpochCandidateV1 {
            class_id: h(seed, 20),
            submitter: bond(seed),
            artifact: crate::palw_improve_artifact_v1::PalwTirArtifactRefV1::Single { root: h(seed, 21) },
            declarations_digest: h(seed, 22),
            datasets: vec![(h(seed, 23), 1_000)],
            fee_paid: 1,
            bond: 2,
            escrow: 3,
            escrow_spent: 0,
            submitted_daa: 6_500,
            counts: None,
        }
    }

    pub(crate) fn pool_entry_v1(seed: u8) -> PalwPoolEntryV2 {
        PalwPoolEntryV2::HoldOut { id: h(seed, 30), supplier: bond(seed) }
    }

    pub(crate) fn item_v1(seed: u8) -> PalwEvalItemV1 {
        PalwEvalItemV1 {
            item: 0,
            case_id: h(seed, 30),
            source: PalwItemSourceV1::HoldOut,
            supplier: Some(bond(seed)),
            seed: h(seed, 31),
            judge: None,
            dropped: false,
        }
    }

    pub(crate) fn result_v1(seed: u8) -> PalwEvalResultV1 {
        PalwEvalResultV1 {
            item: 0,
            subject: PalwEvalSubjectV1::Parent,
            scores: vec![PalwEvalScoreV1 { kind: PalwScoringKindV1::ExactMatch, value: 1 }],
        }
    }

    pub(crate) fn grant_v1(seed: u8) -> PalwRewardGrantV1 {
        PalwRewardGrantV1 {
            recipient: bond(seed),
            stage: PalwRewardStageV1::S2Trainer,
            amount: 100,
            label: PalwTrustLabelV1::Trusted,
            vest_from_daa: 7_900,
            vest_unit_daa: 1_900,
            vest_epochs: 4,
            vested: 0,
            forfeited: false,
        }
    }

    /// One governed line with every row it carries, and one epoch with one of every detail row —
    /// a state the carriage's consistency accepts.
    pub(crate) fn populate_v1(state: &mut crate::palw_state_v2::PalwChainStateV2, seed: u8) {
        state.insert_improvement_rows_for_test_v1(
            line_v1(seed),
            policy_record_v1(seed),
            usage_v1(seed),
            head_v1(seed),
            pool_v1(seed),
            material_v1(seed),
            epoch_v1(seed),
            candidate_v1(seed),
            pool_entry_v1(seed),
            item_v1(seed),
            result_v1(seed),
            grant_v1(seed),
        );
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
