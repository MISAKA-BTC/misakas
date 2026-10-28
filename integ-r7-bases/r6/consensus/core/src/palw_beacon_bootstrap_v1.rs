//! **N3 — the Work Beacon's independent bootstrap, kept apart from the ordinary Beacon and from Panel=0** (register row N3,
//! REQ-N3-1/2/3; RFC-0007 改定 2026-10-12 §VI.3/§VI.8; RFC-0015 改定: Panel=0 is not enabled; the 2026-10-12 user ruling "N3 成立まで新
//! profile 休眠"; `panel1-bond-budget-model-capital-amendments-2026-10-12.md` §6, tests S01–S04).
//!
//! **Why this module exists.** [`crate::palw_opv_bootstrap_v1`]'s graph reaches the ordinary Beacon's sources only through
//! `OpvFinal` — the Final of a Panel=0 (OPV) claim. The revised RFC-0007 forbids depending on Panel=0 and the revised RFC-0015 does
//! not enable it, so under the revisions that route is unreachable and the ordinary Beacon has no root. This module is the separate
//! root the revisions describe, as data and pure predicates:
//!
//! ```text
//!   BootstrapExact commitment ──► IndependentValidityVerified ──► EntropyEligibilityVerified ──► BootstrapHolds ──► new reward profiles
//!        (own graph: no ordinary Beacon, no Panel selection, no candidate conformance, no future PALW Final, no Panel=0)
//! ```
//!
//! * **REQ-N3-1 — two graphs.** [`PALW_BOOTSTRAP_DEPENDENCIES_V1`] is its own graph; its only references into the ordinary graph
//!   ([`PalwOpvNodeV1`]) are to nodes the ordinary Beacon does not feed (`Genesis`, `KernelActive`). [`PALW_BOOTSTRAP_FORBIDDEN_V1`]
//!   names every ordinary node the bootstrap may never read (the Beacon, its sources, Panel selection, Panel=0 claims and Finals,
//!   the candidate's own conformance); the tests check the graph against it.
//! * **REQ-N3-2 — two separate judgements.** [`palw_bootstrap_independent_validity_v1`] (exact re-execution passed, bounded resources,
//!   no forbidden dependency) yields [`IndependentValidityVerifiedV1`]; only then may [`palw_bootstrap_entropy_eligibility_v1`] judge
//!   unpredictability (inputs fixed only AFTER the target's commitment, from an origin that is not header/signature/carrier/seed/RNG
//!   material), grinding resistance (canonical selection, every trial committed on chain, a reviewed adversarial bias bound covering
//!   the withholding rule — never a statistical randomness test, never the reward cap as a trial bound), reorg reproducibility
//!   (derived from selected-chain facts at the profile's settlement depth) and rewrap resistance (content-only work identity), and
//!   yield [`EntropyEligibilityVerifiedV1`]. Both witnesses have private fields: nothing else constructs them. A correct,
//!   deterministic, exactly-verified output that is predictable gets validity and no entropy eligibility (S03).
//! * **REQ-N3-3 — dormancy.** [`palw_new_reward_profile_gate_v1`] is the gate a NEW paid profile consults; it is `Dormant` unless
//!   [`palw_bootstrap_holds_v1`] holds over an APPROVED production entropy profile — and [`approved_bootstrap_entropy_profiles_v1`]
//!   is empty in this release (no kernel, resource values or entropy assumptions are approved), so every new profile is dormant.
//!
//! **No economic parameter values are chosen here.** Every number a judgement compares against (resource bound, settlement depth,
//! independent contributors, sources required, bias bound, attack budget) is a field of a reviewed [`PalwBootstrapEntropyProfileV1`];
//! none is approved. Source-only bootstrap records earn no PALW reward, block right or Final weight (amendments §6).
//!
//! **Round 3 (2026-10-15): the carried object and its fold stage** (section VI). The dormant fence `palw_beacon_bootstrap_v1`
//! (Some-only, refused when armed) owns kernel-route inner 73 (`misaka_palw_kernel::beacon_bootstrap_v1::BootstrapExactContributionV1`:
//! a bond's seal, later its reveal, in one `(profile, target, slot)` round) and kernel-route aux table 130 (the rounds). The fold
//! admits seals only after the target's commitment and inside the round's derived windows, judges each round once its reveal window
//! closed and its inputs settled the profile's depth, and stores the judgement; the judgement re-derives every fact itself
//! ([`palw_bootstrap_judge_round_v1`]) and runs the two predicates above — nothing a contributor asserts is a fact. The restore fence
//! re-runs every stored judgement. [`palw_bootstrap_holds_in_state_v1`] feeds [`palw_bootstrap_holds_v1`] the re-derived witnesses;
//! [`approved_bootstrap_entropy_profiles_v1`] stays empty, so it holds nowhere and nothing activates. No hidden fallback to a block hash, heartbeat, BASE-0, signature,
//! committee seed or local RNG exists: an origin of that kind is refused, and an empty source set leaves the profile dormant.

use crate::Hash64;
use crate::palw_opv_bootstrap_v1::PalwOpvNodeV1;

// =================================================================================================================================
// I. The bootstrap graph (REQ-N3-1)
// =================================================================================================================================

/// A state of the bootstrap path. `Ordinary` names a node of the ordinary onboarding → OPV → Beacon graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PalwBootstrapNodeV1 {
    Genesis,
    Ordinary(PalwOpvNodeV1),
    /// A reviewed BootstrapExact source profile and its entropy profile are approved for production. **No row**: none is.
    ApprovedProfile,
    /// A source's work is committed under an approved BootstrapExact profile.
    BootstrapExactCommitted,
    /// [`palw_bootstrap_independent_validity_v1`] passed.
    IndependentValidityVerified,
    /// [`palw_bootstrap_entropy_eligibility_v1`] passed.
    EntropyEligibilityVerified,
    /// [`palw_bootstrap_holds_v1`]: enough distinct eligible sources under one approved production profile.
    BootstrapHolds,
    /// [`palw_new_reward_profile_gate_v1`] is `Open`.
    NewRewardProfileOpen,
}

/// One derivation of the bootstrap graph (several rows of one node are alternatives).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwBootstrapDependencyV1 {
    pub node: PalwBootstrapNodeV1,
    pub requires: &'static [PalwBootstrapNodeV1],
    pub enforced_by: &'static str,
}

use PalwBootstrapNodeV1 as B;
use PalwOpvNodeV1 as N;

const fn bdep(
    node: PalwBootstrapNodeV1,
    requires: &'static [PalwBootstrapNodeV1],
    enforced_by: &'static str,
) -> PalwBootstrapDependencyV1 {
    PalwBootstrapDependencyV1 { node, requires, enforced_by }
}

/// **The bootstrap graph.** `ApprovedProfile` has no row ([`approved_bootstrap_entropy_profiles_v1`] is empty), so nothing past
/// `IndependentValidityVerified` is reachable in this release — by construction, not by a fence.
pub const PALW_BOOTSTRAP_DEPENDENCIES_V1: &[PalwBootstrapDependencyV1] = &[
    bdep(B::Ordinary(N::Genesis), &[B::Genesis], "the chain's genesis"),
    bdep(B::Ordinary(N::KernelActive), &[B::Ordinary(N::Genesis)], "KernelScheduleV1::standing_at (fed by no Beacon)"),
    bdep(
        B::BootstrapExactCommitted,
        &[B::Ordinary(N::KernelActive)],
        "a source-only exact-validation commitment, sealed after its target's commitment (not a Panel=0 claim)",
    ),
    bdep(B::IndependentValidityVerified, &[B::BootstrapExactCommitted], "palw_bootstrap_independent_validity_v1"),
    bdep(
        B::EntropyEligibilityVerified,
        &[B::IndependentValidityVerified, B::ApprovedProfile],
        "palw_bootstrap_entropy_eligibility_v1",
    ),
    bdep(B::BootstrapHolds, &[B::EntropyEligibilityVerified, B::ApprovedProfile], "palw_bootstrap_holds_v1"),
    bdep(B::NewRewardProfileOpen, &[B::BootstrapHolds], "palw_new_reward_profile_gate_v1"),
];

/// **Every ordinary node the bootstrap may never depend on** (amendments §6: no ordinary Work Beacon, no Panel selection by it, no
/// candidate conformance, no future PALW Final; RFC-0015 改定: no Panel=0). Includes every node that reaches the ordinary Beacon
/// only through `OpvFinal`.
pub const PALW_BOOTSTRAP_FORBIDDEN_V1: &[PalwOpvNodeV1] = &[
    N::OpvEligible,
    N::OpvClassRegistered,
    N::OpvClaim,
    N::OpvFinal,
    N::EligibleSourcesNonEmpty,
    N::BeaconLocked,
    N::SampledEvidence,
    N::CommittedSampled,
    N::ConformancePassed,
    N::G14Eligible,
    N::V2Active,
    N::V2Onboarded,
    N::ApprovedPanelScheme,
    N::PanelAssignmentBeacon,
    N::V3PanelBound,
    N::WorkSliceBeacon,
];

/// The least fixed point of the bootstrap graph from `Genesis`, with `disabled` nodes' rows removed.
pub fn palw_bootstrap_reachable_v1(rows: &[PalwBootstrapDependencyV1], disabled: &[PalwBootstrapNodeV1]) -> Vec<PalwBootstrapNodeV1> {
    let mut reached = vec![B::Genesis];
    loop {
        let before = reached.len();
        for row in rows {
            if !disabled.contains(&row.node) && !reached.contains(&row.node) && row.requires.iter().all(|r| reached.contains(r)) {
                reached.push(row.node);
            }
        }
        if reached.len() == before {
            reached.sort();
            return reached;
        }
    }
}

// =================================================================================================================================
// II. Profiles (reviewed values live here; none is approved)
// =================================================================================================================================

/// Production profiles may open new reward profiles; synthetic ones exist only for unit/integration tests (amendments §6: synthetic
/// randomness is separated by type and configuration and is never production).
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwBootstrapProfileClassV1 {
    Production,
    SyntheticTestOnly,
}

/// What a profile's bias bound rests on. Only an adversarial analysis over a stated attack budget counts (S04: "単なる統計乱数test
/// で合格にしない").
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwBiasEvidenceKindV1 {
    /// Bias bounded against an adversary with the profile's `attack_budget_trials`, including private candidates, source selection,
    /// withholding and rewrapping.
    AdversarialAnalysis,
    /// Output passes statistical randomness tests. Never sufficient.
    StatisticalTestsOnly,
    /// Nothing reviewed.
    None,
}

/// What happens to a committed source whose reveal is withheld.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwWithholdingRuleV1 {
    /// The commitment's deposit is forfeited and the round proceeds with a fixed, pre-committed substitute.
    ForfeitAndSubstitute,
    /// The withholder is skipped (the last revealer chooses between two outputs).
    SkipWithholder,
    /// The round aborts and restarts (the last revealer can force a redraw).
    AbortRound,
}

/// **A reviewed bootstrap entropy profile.** Every comparison value of the two judgements is a field here; this module chooses none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwBootstrapEntropyProfileV1 {
    pub id: Hash64,
    pub class: PalwBootstrapProfileClassV1,
    /// The most resource units one BootstrapExact computation may cost (exact re-execution by every validator).
    pub max_resource_units: u64,
    /// The selected-chain depth below which source facts are not read (reorg reproducibility).
    pub min_settlement_depth: u64,
    /// The fewest independent contributors (distinct bonds of distinct operators) a source's inputs combine.
    pub min_independent_contributors: u32,
    /// The distinct eligible sources [`palw_bootstrap_holds_v1`] requires.
    pub sources_required: u32,
    pub bias_evidence: PalwBiasEvidenceKindV1,
    /// The withholding rule the bias analysis covered.
    pub evaluated_withholding_rule: PalwWithholdingRuleV1,
    /// The adversary's trial budget the analysis assumed (> 0 when evidence is adversarial).
    pub attack_budget_trials: u64,
    /// The reviewed bias bound, in millibits, under that budget (> 0 when evidence is adversarial: zero bias is never claimed).
    pub bias_bound_millibits: u32,
}

/// **The profiles approved for production — none in this release.** Candidate kernels, resource values and entropy assumptions are
/// not decided (amendments §6, "未決の値"); adding one is a reviewed release change, never a runtime option.
pub fn approved_bootstrap_entropy_profiles_v1() -> Vec<PalwBootstrapEntropyProfileV1> {
    Vec::new()
}

// =================================================================================================================================
// III. IndependentValidityVerified (REQ-N3-2, first judgement)
// =================================================================================================================================

/// One BootstrapExact source candidate's validity facts, each derived by the validator, never asserted by the contributor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwBootstrapExactCandidateV1 {
    pub profile_id: Hash64,
    /// Content-derived work identity (see [`PalwWorkIdentityBindingV1`]).
    pub work_id: Hash64,
    /// The validator's own exact re-execution: `Ok(output digest)` or why it failed.
    pub exact: Result<Hash64, String>,
    pub resource_units: u64,
    /// Every ordinary-graph node the candidate's validity, settlement or source admission reads.
    pub reads: Vec<PalwOpvNodeV1>,
    /// The candidate's validity reads the conformance, Panel or semantics of the model/kernel whose challenge it would seed.
    pub reads_its_consumer: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwBootstrapRefusalV1 {
    pub code: &'static str,
    pub why: String,
}

fn refuse<T>(code: &'static str, why: impl Into<String>) -> Result<T, PalwBootstrapRefusalV1> {
    Err(PalwBootstrapRefusalV1 { code, why: why.into() })
}

/// The first judgement's witness. Private fields: only [`palw_bootstrap_independent_validity_v1`] constructs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndependentValidityVerifiedV1 {
    profile_id: Hash64,
    work_id: Hash64,
    output_digest: Hash64,
}

impl IndependentValidityVerifiedV1 {
    pub fn work_id(&self) -> Hash64 {
        self.work_id
    }
    pub fn output_digest(&self) -> Hash64 {
        self.output_digest
    }
    pub fn profile_id(&self) -> Hash64 {
        self.profile_id
    }
}

/// **Independent validity**: the exact re-execution passed, within the profile's resource bound, and nothing it reads is the
/// ordinary Beacon, Panel=0, Panel selection or its own consumer. Says NOTHING about randomness.
pub fn palw_bootstrap_independent_validity_v1(
    candidate: &PalwBootstrapExactCandidateV1,
    profile: &PalwBootstrapEntropyProfileV1,
) -> Result<IndependentValidityVerifiedV1, PalwBootstrapRefusalV1> {
    if candidate.profile_id != profile.id {
        return refuse("WRONG_PROFILE", "the candidate names another BootstrapExact profile");
    }
    let output_digest = match &candidate.exact {
        Ok(digest) => *digest,
        Err(why) => return refuse("EXACT_FAILED", why.clone()),
    };
    if candidate.resource_units > profile.max_resource_units {
        return refuse(
            "RESOURCE_BOUND",
            format!("{} units over the profile's {}", candidate.resource_units, profile.max_resource_units),
        );
    }
    if let Some(n) = candidate.reads.iter().find(|n| PALW_BOOTSTRAP_FORBIDDEN_V1.contains(n)) {
        return refuse("DEPENDS_ON_ORDINARY_BEACON", format!("validity reads {n:?}: the ordinary Beacon, Panel selection or Panel=0"));
    }
    if candidate.reads_its_consumer {
        return refuse("SELF_DEPENDENT", "the source's validity reads the candidate it would seed (a cycle)");
    }
    Ok(IndependentValidityVerifiedV1 { profile_id: profile.id, work_id: candidate.work_id, output_digest })
}

// =================================================================================================================================
// IV. EntropyEligibilityVerified (REQ-N3-2, second judgement)
// =================================================================================================================================

/// Where a source computation's inputs come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwEntropyOriginV1 {
    /// Commit-then-reveal contributions of independent bonds, every commitment sealed after the target's commitment.
    IndependentCommitReveal,
    /// Inputs public before the target's commitment (a fixed dataset, a past output).
    PublicBeforeTarget,
    BlockHeaderMaterial,
    Signature,
    CarrierOrClaimId,
    SettlementAnchorHash,
    Heartbeat,
    Base0,
    CommitteeOrAdminSeed,
    LocalRng,
    /// The ordinary Work Beacon (circular for its own bootstrap).
    OrdinaryWorkBeacon,
}

/// How candidates enter the source set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwSourceSelectionV1 {
    /// The first `k` distinct work identities in `(settlement_position, occurrence_index, work_id)` order (RFC-0007 §VI.4).
    CanonicalFirstK,
    /// A contributor, producer or relayer chooses among candidates.
    Chosen,
}

/// What bounds the trials an adversary can try without publishing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwTrialAccountingV1 {
    /// Every trial is a sealed on-chain commitment before any reveal; an unpublished trial cannot enter.
    EveryTrialCommittedOnChain,
    /// Only the per-bond reward cap (amendments §6: never a bound on unpublished trials).
    RewardCapOnly,
    Unbounded,
}

/// Where the source facts are derived from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwSourceDerivationV1 {
    /// Selected-chain facts at this depth below the sink.
    SelectedChainAtDepth(u64),
    /// Local arrival order, RPC answers, mempool or wall clock.
    LocalObservation,
}

/// What a source's work identity commits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwWorkIdentityBindingV1 {
    /// The computation's content only: stable under carrier/header reattachment.
    ContentOnly,
    /// Also a carrier, header or claim id: a rewrap mints a new identity.
    IncludesCarrierOrHeader,
}

/// One validity-verified source's entropy facts, each derived from chain state by the validator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwBootstrapEntropyFactsV1 {
    /// The canonical position of the commitment the source's randomness will serve (the target).
    pub target_committed_position: u64,
    /// The earliest canonical position at which EVERY input of the source computation was fixed.
    pub inputs_fixed_position: u64,
    pub origin: PalwEntropyOriginV1,
    pub independent_contributors: u32,
    pub selection: PalwSourceSelectionV1,
    pub trials: PalwTrialAccountingV1,
    pub withholding: PalwWithholdingRuleV1,
    pub derivation: PalwSourceDerivationV1,
    pub identity: PalwWorkIdentityBindingV1,
}

/// The second judgement's witness. Private fields; it can only be built from an [`IndependentValidityVerifiedV1`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntropyEligibilityVerifiedV1 {
    validity: IndependentValidityVerifiedV1,
    target_committed_position: u64,
}

impl EntropyEligibilityVerifiedV1 {
    pub fn validity(&self) -> &IndependentValidityVerifiedV1 {
        &self.validity
    }
    pub fn target_committed_position(&self) -> u64 {
        self.target_committed_position
    }
}

/// **Entropy eligibility**, judged apart from validity: a correct exact output is NOT eligible when anyone could know it before
/// the target committed, when its inputs come from header/signature/carrier/seed/RNG/ordinary-Beacon material, when candidates are
/// chosen or trials go unpublished, when the bias bound is not an adversarial analysis covering this withholding rule, when the facts
/// are not reproducible from the selected chain at the profile's depth, or when a rewrap mints a new identity.
pub fn palw_bootstrap_entropy_eligibility_v1(
    validity: &IndependentValidityVerifiedV1,
    facts: &PalwBootstrapEntropyFactsV1,
    profile: &PalwBootstrapEntropyProfileV1,
) -> Result<EntropyEligibilityVerifiedV1, PalwBootstrapRefusalV1> {
    use PalwEntropyOriginV1 as O;
    if validity.profile_id != profile.id {
        return refuse("WRONG_PROFILE", "validity was judged under another profile");
    }
    // Unpredictability: inputs fixed at or before the target's commitment make the output precomputable, however correct (S03).
    if facts.inputs_fixed_position <= facts.target_committed_position {
        return refuse(
            "PREDICTABLE",
            format!(
                "every input was fixed at position {} ≤ the target's commitment {}: the output was computable before the target",
                facts.inputs_fixed_position, facts.target_committed_position
            ),
        );
    }
    match facts.origin {
        O::IndependentCommitReveal => {}
        O::PublicBeforeTarget => return refuse("PREDICTABLE", "the inputs were public before the target's commitment"),
        O::OrdinaryWorkBeacon => return refuse("CIRCULAR", "the ordinary Work Beacon cannot seed its own bootstrap"),
        other => return refuse("NOT_ENTROPY", format!("{other:?} is not an entropy source (RFC-0007 §VI.3)")),
    }
    if facts.independent_contributors < profile.min_independent_contributors.max(1) {
        return refuse("TOO_FEW_CONTRIBUTORS", format!("{} independent contributors", facts.independent_contributors));
    }
    // Grinding: free selection, unpublished trials and an unreviewed bias bound (S04).
    if facts.selection != PalwSourceSelectionV1::CanonicalFirstK {
        return refuse("SELECTED", "candidates are chosen, not taken in canonical order");
    }
    if facts.trials != PalwTrialAccountingV1::EveryTrialCommittedOnChain {
        return refuse("HIDDEN_TRIALS", format!("{:?}: unpublished trials are not bounded", facts.trials));
    }
    if profile.bias_evidence != PalwBiasEvidenceKindV1::AdversarialAnalysis
        || profile.attack_budget_trials == 0
        || profile.bias_bound_millibits == 0
        || profile.evaluated_withholding_rule != facts.withholding
    {
        return refuse(
            "BIAS_UNEVALUATED",
            "no adversarial bias bound over a stated attack budget covers this withholding rule (a statistical test is not one)",
        );
    }
    // Reorg reproducibility.
    match facts.derivation {
        PalwSourceDerivationV1::SelectedChainAtDepth(depth) if depth >= profile.min_settlement_depth && depth > 0 => {}
        other => return refuse("NOT_REORG_REPRODUCIBLE", format!("{other:?}")),
    }
    // Rewrap resistance.
    if facts.identity != PalwWorkIdentityBindingV1::ContentOnly {
        return refuse("REWRAPPABLE", "a rewrap of the same work mints a new identity");
    }
    Ok(EntropyEligibilityVerifiedV1 { validity: validity.clone(), target_committed_position: facts.target_committed_position })
}

// =================================================================================================================================
// V. BootstrapHolds and the new-profile gate (REQ-N3-3)
// =================================================================================================================================

/// The bootstrap holds for one target under one approved production profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwBootstrapHoldsV1 {
    profile_id: Hash64,
    target_committed_position: u64,
    sources: u32,
}

impl PalwBootstrapHoldsV1 {
    pub fn profile_id(&self) -> Hash64 {
        self.profile_id
    }
    pub fn target_committed_position(&self) -> u64 {
        self.target_committed_position
    }
    pub fn sources(&self) -> u32 {
        self.sources
    }
}

/// **The bootstrap holds** when `approved` (in production: [`approved_bootstrap_entropy_profiles_v1`]) contains `profile_id` as a
/// PRODUCTION profile and at least its `sources_required` (≥ 1) DISTINCT work identities are entropy-eligible under it for one target.
/// An empty source set, a synthetic profile or an unapproved profile is a refusal — never a fallback.
pub fn palw_bootstrap_holds_v1(
    eligible: &[EntropyEligibilityVerifiedV1],
    profile_id: Hash64,
    target_committed_position: u64,
    approved: &[PalwBootstrapEntropyProfileV1],
) -> Result<PalwBootstrapHoldsV1, PalwBootstrapRefusalV1> {
    let Some(profile) = approved.iter().find(|p| p.id == profile_id) else {
        return refuse("NO_APPROVED_PROFILE", "no approved bootstrap entropy profile (none in this release)");
    };
    if profile.class != PalwBootstrapProfileClassV1::Production {
        return refuse("SYNTHETIC_PROFILE", "a synthetic test profile never opens a production profile");
    }
    let mut works: Vec<Hash64> = eligible
        .iter()
        .filter(|e| e.validity.profile_id == profile_id && e.target_committed_position == target_committed_position)
        .map(|e| e.validity.work_id)
        .collect();
    works.sort();
    works.dedup();
    let required = profile.sources_required.max(1);
    if (works.len() as u64) < required as u64 {
        return refuse("TOO_FEW_SOURCES", format!("{} distinct eligible sources of the {required} required", works.len()));
    }
    Ok(PalwBootstrapHoldsV1 { profile_id, target_committed_position, sources: works.len() as u32 })
}

/// The answer a new paid profile receives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwNewRewardProfileGateV1 {
    /// The profile stays dormant (REQ-N3-3). Existing chain operation is unaffected.
    Dormant {
        code: &'static str,
        why: String,
    },
    Open,
}

/// **REQ-N3-3: the gate every NEW reward profile consults** (a new paid route, new reward or work-weight channel, new permissionless
/// mode). `Open` only with a [`PalwBootstrapHoldsV1`] whose profile is still approved for production. In this release no profile
/// is approved, so the gate is `Dormant` for every caller.
pub fn palw_new_reward_profile_gate_v1(
    holds: Option<&PalwBootstrapHoldsV1>,
    approved: &[PalwBootstrapEntropyProfileV1],
) -> PalwNewRewardProfileGateV1 {
    let Some(holds) = holds else {
        return PalwNewRewardProfileGateV1::Dormant {
            code: "N3_BOOTSTRAP_ABSENT",
            why: "no independent Work Beacon bootstrap holds (REQ-N3-3): new reward profiles stay dormant".into(),
        };
    };
    if !approved.iter().any(|p| p.id == holds.profile_id && p.class == PalwBootstrapProfileClassV1::Production) {
        return PalwNewRewardProfileGateV1::Dormant {
            code: "N3_PROFILE_UNAPPROVED",
            why: "the bootstrap held under a profile this release does not approve".into(),
        };
    }
    PalwNewRewardProfileGateV1::Open
}

/// **The release-level answer**: whether a new reward profile may be armed by this binary at all. `Err` while
/// [`approved_bootstrap_entropy_profiles_v1`] is empty — a validator of a new paid fence calls this before accepting an armed height.
pub fn palw_new_reward_profile_admissible_v1(profile: &str) -> Result<(), String> {
    if approved_bootstrap_entropy_profiles_v1().iter().any(|p| p.class == PalwBootstrapProfileClassV1::Production) {
        return Ok(());
    }
    Err(format!(
        "{profile} cannot be armed: no independent Work Beacon bootstrap is approved (N3, REQ-N3-3; RFC-0007 改定 2026-10-12): new \
         reward profiles stay dormant, and Panel=0 (OpvFinal) is not a bootstrap"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_opv_bootstrap_v1::reachable_v1;

    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }

    /// A synthetic profile whose every value is a TEST value (never production).
    fn synthetic() -> PalwBootstrapEntropyProfileV1 {
        PalwBootstrapEntropyProfileV1 {
            id: h(1),
            class: PalwBootstrapProfileClassV1::SyntheticTestOnly,
            max_resource_units: 1_000,
            min_settlement_depth: 10,
            min_independent_contributors: 2,
            sources_required: 2,
            bias_evidence: PalwBiasEvidenceKindV1::AdversarialAnalysis,
            evaluated_withholding_rule: PalwWithholdingRuleV1::ForfeitAndSubstitute,
            attack_budget_trials: 64,
            bias_bound_millibits: 500,
        }
    }

    fn candidate(work: u8) -> PalwBootstrapExactCandidateV1 {
        PalwBootstrapExactCandidateV1 {
            profile_id: h(1),
            work_id: h(work),
            exact: Ok(h(work ^ 0x55)),
            resource_units: 100,
            reads: vec![N::Genesis, N::KernelActive],
            reads_its_consumer: false,
        }
    }

    fn facts() -> PalwBootstrapEntropyFactsV1 {
        PalwBootstrapEntropyFactsV1 {
            target_committed_position: 100,
            inputs_fixed_position: 120,
            origin: PalwEntropyOriginV1::IndependentCommitReveal,
            independent_contributors: 3,
            selection: PalwSourceSelectionV1::CanonicalFirstK,
            trials: PalwTrialAccountingV1::EveryTrialCommittedOnChain,
            withholding: PalwWithholdingRuleV1::ForfeitAndSubstitute,
            derivation: PalwSourceDerivationV1::SelectedChainAtDepth(10),
            identity: PalwWorkIdentityBindingV1::ContentOnly,
        }
    }

    fn code<T: std::fmt::Debug>(r: Result<T, PalwBootstrapRefusalV1>) -> &'static str {
        r.expect_err("refused").code
    }

    /// REQ-N3-1 / S01: the bootstrap reaches independent validity with EVERY ordinary Beacon, Panel=0 and Panel-selection node
    /// disabled, and no bootstrap row names a forbidden ordinary node.
    #[test]
    fn n3_1_the_bootstrap_graph_is_apart_from_the_ordinary_beacon_and_panel_zero() {
        for row in PALW_BOOTSTRAP_DEPENDENCIES_V1 {
            for r in row.requires.iter().chain([&row.node]) {
                if let B::Ordinary(n) = r {
                    assert!(!PALW_BOOTSTRAP_FORBIDDEN_V1.contains(n), "{:?} reads forbidden {n:?}", row.node);
                }
            }
            assert!(!row.enforced_by.contains("OpvFinal") && !row.enforced_by.contains("panel_free"), "{row:?}");
        }
        // Every forbidden ordinary node is fed (in the ordinary graph) only through OpvFinal or the Beacon: disabling Panel=0's
        // Final removes them all there, and the bootstrap does not notice.
        let ordinary = reachable_v1(crate::palw_opv_bootstrap_v1::PALW_OPV_DEPENDENCIES_V1, &[N::OpvFinal]);
        for n in [N::EligibleSourcesNonEmpty, N::BeaconLocked, N::WorkSliceBeacon, N::PanelAssignmentBeacon] {
            assert!(!ordinary.contains(&n), "{n:?} is the ordinary Beacon's, reached only through Panel=0");
        }
        let reached = palw_bootstrap_reachable_v1(PALW_BOOTSTRAP_DEPENDENCIES_V1, &[]);
        assert!(reached.contains(&B::IndependentValidityVerified), "independent validity needs no Beacon and no Panel=0");
        // REQ-N3-3: no approved profile, so nothing past validity — and no new reward profile.
        for n in [B::ApprovedProfile, B::EntropyEligibilityVerified, B::BootstrapHolds, B::NewRewardProfileOpen] {
            assert!(!reached.contains(&n), "{n:?} must be unreachable while no profile is approved");
        }
        // With an approved profile (a reviewed release change), the whole path closes — still without any ordinary Beacon node.
        let mut rows = PALW_BOOTSTRAP_DEPENDENCIES_V1.to_vec();
        rows.push(bdep(B::ApprovedProfile, &[B::Genesis], "a reviewed release approval (test only)"));
        assert!(palw_bootstrap_reachable_v1(&rows, &[]).contains(&B::NewRewardProfileOpen));
    }

    /// REQ-N3-2: validity is judged first and alone; a forbidden dependency or a cycle is refused before randomness is considered.
    #[test]
    fn n3_2_validity_refuses_ordinary_beacon_panel_zero_and_self_dependence() {
        let p = synthetic();
        assert!(palw_bootstrap_independent_validity_v1(&candidate(9), &p).is_ok());
        for n in [N::OpvFinal, N::BeaconLocked, N::PanelAssignmentBeacon, N::WorkSliceBeacon, N::ConformancePassed] {
            let mut c = candidate(9);
            c.reads.push(n);
            assert_eq!(code(palw_bootstrap_independent_validity_v1(&c, &p)), "DEPENDS_ON_ORDINARY_BEACON", "{n:?}");
        }
        let mut c = candidate(9);
        c.reads_its_consumer = true;
        assert_eq!(code(palw_bootstrap_independent_validity_v1(&c, &p)), "SELF_DEPENDENT");
        let mut c = candidate(9);
        c.exact = Err("leaf 3 differs".into());
        assert_eq!(code(palw_bootstrap_independent_validity_v1(&c, &p)), "EXACT_FAILED");
        let mut c = candidate(9);
        c.resource_units = p.max_resource_units + 1;
        assert_eq!(code(palw_bootstrap_independent_validity_v1(&c, &p)), "RESOURCE_BOUND");
    }

    /// **S03: a correct but predictable bootstrap output gets no entropy eligibility** — exact PASS (validity verified), inputs fixed
    /// at or before the target's commitment, or public before it, or taken from header/signature/seed/RNG/Beacon material.
    #[test]
    fn s03_a_correct_but_predictable_output_gets_no_entropy_eligibility() {
        let p = synthetic();
        let valid = palw_bootstrap_independent_validity_v1(&candidate(9), &p).expect("exact pass: validity holds");
        assert!(palw_bootstrap_entropy_eligibility_v1(&valid, &facts(), &p).is_ok(), "the unpredictable control is eligible");
        for fixed in [0, 99, 100] {
            let f = PalwBootstrapEntropyFactsV1 { inputs_fixed_position: fixed, ..facts() };
            assert_eq!(code(palw_bootstrap_entropy_eligibility_v1(&valid, &f, &p)), "PREDICTABLE", "inputs fixed at {fixed}");
        }
        let f = PalwBootstrapEntropyFactsV1 { origin: PalwEntropyOriginV1::PublicBeforeTarget, ..facts() };
        assert_eq!(code(palw_bootstrap_entropy_eligibility_v1(&valid, &f, &p)), "PREDICTABLE");
        for origin in [
            PalwEntropyOriginV1::BlockHeaderMaterial,
            PalwEntropyOriginV1::Signature,
            PalwEntropyOriginV1::CarrierOrClaimId,
            PalwEntropyOriginV1::SettlementAnchorHash,
            PalwEntropyOriginV1::Heartbeat,
            PalwEntropyOriginV1::Base0,
            PalwEntropyOriginV1::CommitteeOrAdminSeed,
            PalwEntropyOriginV1::LocalRng,
        ] {
            let f = PalwBootstrapEntropyFactsV1 { origin, ..facts() };
            assert_eq!(code(palw_bootstrap_entropy_eligibility_v1(&valid, &f, &p)), "NOT_ENTROPY", "{origin:?}");
        }
        let f = PalwBootstrapEntropyFactsV1 { origin: PalwEntropyOriginV1::OrdinaryWorkBeacon, ..facts() };
        assert_eq!(code(palw_bootstrap_entropy_eligibility_v1(&valid, &f, &p)), "CIRCULAR");
    }

    /// **S04: private candidates, source selection, withholding and rewrapping** — each refused unless the profile's adversarial bias
    /// bound covers it; a statistical randomness test never passes; reorg-unreproducible facts are refused.
    #[test]
    fn s04_private_candidates_selection_withholding_and_rewrapping_are_refused() {
        let p = synthetic();
        let valid = palw_bootstrap_independent_validity_v1(&candidate(9), &p).unwrap();
        let judge =
            |f: PalwBootstrapEntropyFactsV1, p: &PalwBootstrapEntropyProfileV1| palw_bootstrap_entropy_eligibility_v1(&valid, &f, p);
        // Private candidates: trials not all committed on chain (the reward cap is not a trial bound).
        for trials in [PalwTrialAccountingV1::RewardCapOnly, PalwTrialAccountingV1::Unbounded] {
            assert_eq!(code(judge(PalwBootstrapEntropyFactsV1 { trials, ..facts() }, &p)), "HIDDEN_TRIALS");
        }
        // Source selection.
        assert_eq!(code(judge(PalwBootstrapEntropyFactsV1 { selection: PalwSourceSelectionV1::Chosen, ..facts() }, &p)), "SELECTED");
        // Withholding: a rule the analysis did not cover, or no adversarial analysis at all.
        for withholding in [PalwWithholdingRuleV1::SkipWithholder, PalwWithholdingRuleV1::AbortRound] {
            assert_eq!(code(judge(PalwBootstrapEntropyFactsV1 { withholding, ..facts() }, &p)), "BIAS_UNEVALUATED");
        }
        for evidence in [PalwBiasEvidenceKindV1::StatisticalTestsOnly, PalwBiasEvidenceKindV1::None] {
            let q = PalwBootstrapEntropyProfileV1 { bias_evidence: evidence, ..synthetic() };
            let v = palw_bootstrap_independent_validity_v1(&candidate(9), &q).unwrap();
            assert_eq!(code(palw_bootstrap_entropy_eligibility_v1(&v, &facts(), &q)), "BIAS_UNEVALUATED", "{evidence:?}");
        }
        for (budget, bound) in [(0, 500), (64, 0)] {
            let q = PalwBootstrapEntropyProfileV1 { attack_budget_trials: budget, bias_bound_millibits: bound, ..synthetic() };
            let v = palw_bootstrap_independent_validity_v1(&candidate(9), &q).unwrap();
            assert_eq!(code(palw_bootstrap_entropy_eligibility_v1(&v, &facts(), &q)), "BIAS_UNEVALUATED");
        }
        // Rewrapping.
        let f = PalwBootstrapEntropyFactsV1 { identity: PalwWorkIdentityBindingV1::IncludesCarrierOrHeader, ..facts() };
        assert_eq!(code(judge(f, &p)), "REWRAPPABLE");
        // Private fork / reorg: facts from a local view, or shallower than the profile's depth.
        for derivation in [PalwSourceDerivationV1::LocalObservation, PalwSourceDerivationV1::SelectedChainAtDepth(9)] {
            assert_eq!(code(judge(PalwBootstrapEntropyFactsV1 { derivation, ..facts() }, &p)), "NOT_REORG_REPRODUCIBLE");
        }
        // Several bonds of one contributor do not make independent contributors.
        assert_eq!(code(judge(PalwBootstrapEntropyFactsV1 { independent_contributors: 1, ..facts() }, &p)), "TOO_FEW_CONTRIBUTORS");
        // A rewrap of the same work does not count twice toward the sources required.
        let e1 = judge(facts(), &p).unwrap();
        let twice = [e1.clone(), e1];
        let approved = [PalwBootstrapEntropyProfileV1 { class: PalwBootstrapProfileClassV1::Production, ..synthetic() }];
        assert_eq!(code(palw_bootstrap_holds_v1(&twice, h(1), 100, &approved)), "TOO_FEW_SOURCES");
    }

    /// REQ-N3-3: dormant in this release for every caller; synthetic profiles never open; only an approved production profile with
    /// enough distinct eligible sources does.
    #[test]
    fn n3_3_new_reward_profiles_stay_dormant_until_the_bootstrap_holds() {
        assert!(approved_bootstrap_entropy_profiles_v1().is_empty());
        assert!(palw_new_reward_profile_admissible_v1("palw_example_paid_profile").is_err());
        let release = approved_bootstrap_entropy_profiles_v1();
        assert!(matches!(palw_new_reward_profile_gate_v1(None, &release), PalwNewRewardProfileGateV1::Dormant { .. }));
        let p = synthetic();
        let eligible: Vec<_> = [9u8, 10]
            .iter()
            .map(|w| {
                let v = palw_bootstrap_independent_validity_v1(&candidate(*w), &p).unwrap();
                palw_bootstrap_entropy_eligibility_v1(&v, &facts(), &p).unwrap()
            })
            .collect();
        assert_eq!(code(palw_bootstrap_holds_v1(&eligible, h(1), 100, &release)), "NO_APPROVED_PROFILE");
        assert_eq!(code(palw_bootstrap_holds_v1(&eligible, h(1), 100, std::slice::from_ref(&p))), "SYNTHETIC_PROFILE");
        let production = [PalwBootstrapEntropyProfileV1 { class: PalwBootstrapProfileClassV1::Production, ..synthetic() }];
        assert_eq!(code(palw_bootstrap_holds_v1(&eligible, h(1), 101, &production)), "TOO_FEW_SOURCES", "another target");
        assert_eq!(code(palw_bootstrap_holds_v1(&[], h(1), 100, &production)), "TOO_FEW_SOURCES", "no fallback when empty");
        let holds = palw_bootstrap_holds_v1(&eligible, h(1), 100, &production).unwrap();
        assert_eq!(holds.sources(), 2);
        assert_eq!(palw_new_reward_profile_gate_v1(Some(&holds), &production), PalwNewRewardProfileGateV1::Open);
        // The release approves nothing, so the same holds still opens nothing here.
        assert!(matches!(palw_new_reward_profile_gate_v1(Some(&holds), &release), PalwNewRewardProfileGateV1::Dormant { .. }));
    }
}

/// The new reward profiles REQ-N3-3 keeps dormant until an independent bootstrap holds: every fence whose rules pay, weight or
/// price sourced/attested work. A new reward fence is added here in the same change that adds it.
pub const PALW_N3_GATED_REWARD_FENCES_V1: &[&str] = &[
    "palw_beacon_source_opt_in_v1",
    "palw_beacon_source_fee_v1",
    "palw_claim_challenge_v2",
    "palw_verifier_pay_v1",
    "palw_model_verification_gate_v1",
    "palw_minimum_price_v1",
    "palw_monetary_material_v1",
    "palw_material_release_recovery_v2",
    "palw_panel_lifetime_v2",
];

impl crate::config::params::Params {
    /// **REQ-N3-3 as a second lock in `validate_palw_v2`.** Each listed fence's own validator already refuses any real
    /// activation; this refuses again, by the bootstrap's absence, should one of them ever stop refusing. `never()`/`None` pass.
    /// Private integration profiles never reach it: they validate an explicitly dormant copy.
    pub fn validate_palw_new_reward_profiles_n3_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        for (name, activation) in self.palw_fences_v1() {
            if PALW_N3_GATED_REWARD_FENCES_V1.contains(&name)
                && activation.is_some_and(|a| a != crate::config::params::ForkActivation::never())
            {
                palw_new_reward_profile_admissible_v1(name).map_err(crate::palw_mode_v2::PalwModeV2Error::InvalidOwned)?;
            }
        }
        Ok(())
    }
}

// =================================================================================================================================
// VI. The carried object and its fold stage (round 3, 2026-10-15): fence `palw_beacon_bootstrap_v1`, inner 73, aux 130
// =================================================================================================================================

use misaka_palw_kernel::beacon_bootstrap_v1::{
    BootstrapExactRoundV1, bootstrap_exact_commitment_v1, bootstrap_exact_output_v1, bootstrap_exact_work_id_v1,
};

/// Kernel route aux table: BootstrapExact source rounds (key: the round id, 64 bytes). Allocated from the top of the free range.
pub const PALW_BEACON_BOOTSTRAP_TABLE_V1: u8 = 130;

/// **`palw_beacon_bootstrap_v1`'s policy** — every value POLICY, none approved. The `profile` is what the fold JUDGES rounds under;
/// it is not an approval: [`palw_bootstrap_holds_v1`] and the new-profile gate read only [`approved_bootstrap_entropy_profiles_v1`],
/// which is empty in this release.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwBeaconBootstrapPolicyV1 {
    pub version: u16,
    pub profile: PalwBootstrapEntropyProfileV1,
    /// Seals land in `(target committed, target committed + seal_window_daa]`.
    pub seal_window_daa: u64,
    /// Reveals land in `(seal end, seal end + reveal_window_daa]`.
    pub reveal_window_daa: u64,
    pub maximum_contributors_per_round: u32,
    /// Rounds (slots) per target; at least the profile's `sources_required`.
    pub maximum_sources_per_target: u32,
    /// Unjudged rounds held at once.
    pub maximum_open_rounds: u32,
    /// A judged round is retired this long after its judgement.
    pub judged_retention_daa: u64,
    pub security_review_scope: [u8; 64],
}

impl PalwBeaconBootstrapPolicyV1 {
    pub fn id(&self) -> Hash64 {
        Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"MISAKA/PALW/BEACON-BOOTSTRAP-POLICY/V1", self))
    }

    /// Structural invariants only: version 1, finite non-zero windows and capacities, room for the profile's own minimums, a review
    /// identity. Whether the profile is sound is the profile review's (and its approval is a release change, never this field).
    pub fn validate(&self) -> Result<(), &'static str> {
        let windows = self.seal_window_daa.checked_add(self.reveal_window_daa);
        if self.version != 1
            || self.seal_window_daa == 0
            || self.reveal_window_daa == 0
            || windows.is_none_or(|w| w == u64::MAX)
            || self.maximum_contributors_per_round < self.profile.min_independent_contributors.max(1)
            || self.maximum_contributors_per_round > 4096
            || self.maximum_sources_per_target < self.profile.sources_required.max(1)
            || self.maximum_open_rounds == 0
            || self.judged_retention_daa == 0
            || self.judged_retention_daa == u64::MAX
            || self.security_review_scope == [0; 64]
        {
            return Err("the bootstrap policy needs finite windows, capacities covering its profile's minimums and a review scope");
        }
        Ok(())
    }

    /// **NOT A PROPOSAL**: a SYNTHETIC test profile (never production) and structural fixture windows, for tests and private profiles
    /// only. Its evaluated withholding rule is the fold's (`AbortRound`), so the fixture can exercise the eligibility path.
    pub fn unapproved_probe_v1() -> Self {
        Self {
            version: 1,
            profile: PalwBootstrapEntropyProfileV1 {
                id: Hash64::from_bytes([0x42; 64]),
                class: PalwBootstrapProfileClassV1::SyntheticTestOnly,
                max_resource_units: 64,
                min_settlement_depth: 2,
                min_independent_contributors: 2,
                sources_required: 2,
                bias_evidence: PalwBiasEvidenceKindV1::AdversarialAnalysis,
                evaluated_withholding_rule: PalwWithholdingRuleV1::AbortRound,
                attack_budget_trials: 64,
                bias_bound_millibits: 500,
            },
            seal_window_daa: 10,
            reveal_window_daa: 10,
            maximum_contributors_per_round: 16,
            maximum_sources_per_target: 4,
            maximum_open_rounds: 64,
            judged_retention_daa: 1_000,
            security_review_scope: [9; 64],
        }
    }
}

/// `Params::palw_beacon_bootstrap_v1`'s value — Some-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwBeaconBootstrapFenceV1 {
    pub activation: crate::config::params::ForkActivation,
    pub policy: PalwBeaconBootstrapPolicyV1,
}

impl PalwBeaconBootstrapFenceV1 {
    pub fn unapproved_probe_at(activation: crate::config::params::ForkActivation) -> Self {
        Self { activation, policy: PalwBeaconBootstrapPolicyV1::unapproved_probe_v1() }
    }
}

/// One contributor's seal in a round (fold-derived fields: `operator` from the bond, `sealed_daa` / `revealed_daa` the blocks').
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwBootstrapSealRowV1 {
    pub contributor: [u8; 64],
    pub operator: [u8; 64],
    pub commitment: [u8; 64],
    pub sealed_daa: u64,
    /// `(preimage, revealed_daa)` once revealed.
    pub reveal: Option<([u8; 64], u64)>,
}

/// **A round's stored judgement** — what the fold derived and what the two predicates answered. `None` refusal = the witness holds.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwBootstrapJudgementV1 {
    pub judged_daa: u64,
    pub work_id: [u8; 64],
    /// The exact output the fold re-executed; `None` when an input was withheld (the round aborts).
    pub output: Option<[u8; 64]>,
    pub independent_contributors: u32,
    pub depth_daa: u64,
    /// `None`: `IndependentValidityVerified`; otherwise the refusal code (`WITHHELD` for an aborted round).
    pub validity_refusal: Option<String>,
    /// `None`: `EntropyEligibilityVerified`; otherwise the refusal code (`NOT_JUDGED` when validity did not hold).
    pub eligibility_refusal: Option<String>,
}

/// **A BootstrapExact source round** (aux 130, key = the round id).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwBootstrapRoundRowV1 {
    pub round: BootstrapExactRoundV1,
    /// The target claim's commitment position, read from the kernel ledger when the round opened (never from a contribution).
    pub target_committed_daa: u64,
    /// Canonical: ascending `(sealed_daa, contributor)`, distinct contributors.
    pub seals: Vec<PalwBootstrapSealRowV1>,
    pub judgement: Option<PalwBootstrapJudgementV1>,
    pub policy: Hash64,
}

impl PalwBootstrapRoundRowV1 {
    pub fn seal_end(&self, policy: &PalwBeaconBootstrapPolicyV1) -> u64 {
        self.target_committed_daa.saturating_add(policy.seal_window_daa)
    }

    pub fn reveal_end(&self, policy: &PalwBeaconBootstrapPolicyV1) -> u64 {
        self.seal_end(policy).saturating_add(policy.reveal_window_daa)
    }

    /// The last position any input was fixed or revealed at (the depth is counted from here).
    pub fn last_input_daa(&self) -> u64 {
        self.seals
            .iter()
            .map(|s| s.reveal.map_or(s.sealed_daa, |(_, at)| at.max(s.sealed_daa)))
            .max()
            .unwrap_or(self.target_committed_daa)
    }

    /// Whether the fold judges the round at `now`: its reveal window closed, and its inputs settled the profile's depth (an aborted
    /// round is judged at once).
    pub fn due(&self, policy: &PalwBeaconBootstrapPolicyV1, now: u64) -> bool {
        self.judgement.is_none()
            && now > self.reveal_end(policy)
            && (self.seals.iter().any(|s| s.reveal.is_none())
                || now.saturating_sub(self.last_input_daa()) >= policy.profile.min_settlement_depth)
    }

    pub fn validate(&self, policy: &PalwBeaconBootstrapPolicyV1) -> Result<(), &'static str> {
        let (seal_end, reveal_end) = (self.seal_end(policy), self.reveal_end(policy));
        let round = self.round.id();
        if self.policy != policy.id()
            || self.round.profile != policy.profile.id.as_bytes()
            || self.round.slot >= policy.maximum_sources_per_target
            || self.seals.is_empty()
            || self.seals.len() > policy.maximum_contributors_per_round as usize
            || self.seals.windows(2).any(|w| (w[0].sealed_daa, w[0].contributor) >= (w[1].sealed_daa, w[1].contributor))
        {
            return Err("a bootstrap round differs from its policy or its seals are not canonical");
        }
        let mut contributors: Vec<[u8; 64]> = self.seals.iter().map(|s| s.contributor).collect();
        contributors.sort();
        contributors.dedup();
        if contributors.len() != self.seals.len() {
            return Err("a bootstrap round holds two seals of one contributor");
        }
        for s in &self.seals {
            if s.sealed_daa <= self.target_committed_daa
                || s.sealed_daa > seal_end
                || s.reveal.is_some_and(|(preimage, at)| {
                    at <= seal_end
                        || at > reveal_end
                        || bootstrap_exact_commitment_v1(&round, &s.contributor, &preimage) != s.commitment
                })
            {
                return Err("a bootstrap seal or reveal lies outside its round's windows or opens another commitment");
            }
        }
        if let Some(j) = &self.judgement {
            let undue = Self { judgement: None, ..self.clone() };
            if !undue.due(policy, j.judged_daa) {
                return Err("a bootstrap round was judged before it was due");
            }
        }
        Ok(())
    }
}

/// **The round's judgement, every fact derived here** (REQ-N3-2): the content-only work identity over the canonical commitments; the
/// combiner re-executed exactly over the revealed inputs (a withheld input aborts the round — the fold's withholding rule is
/// `AbortRound`); the resource units (one per input); the reads (`Genesis`, `KernelActive`: the kernel ledger's target row and the
/// round's own seals — no Beacon, no Panel, no Panel=0, not the target's conformance); the target's commitment position from the
/// ledger; the inputs' fixing position (the last seal, every seal strictly after the target); the independent contributors (distinct
/// operators of the sealing bonds); canonical selection (every seal of the round, in canonical order, under the per-round cap); every
/// trial an on-chain seal; the depth below `now` of the last input. Then [`palw_bootstrap_independent_validity_v1`] and only if it holds
/// [`palw_bootstrap_entropy_eligibility_v1`]. Pure: the fold and the restore fence call it with the same `now` (`judged_daa`).
pub fn palw_bootstrap_judge_round_v1(
    row: &PalwBootstrapRoundRowV1,
    profile: &PalwBootstrapEntropyProfileV1,
    now: u64,
) -> (PalwBootstrapJudgementV1, Option<EntropyEligibilityVerifiedV1>) {
    let round_id = row.round.id();
    let commitments: Vec<[u8; 64]> = row.seals.iter().map(|s| s.commitment).collect();
    let work_id = bootstrap_exact_work_id_v1(&row.round, &commitments);
    let mut operators: Vec<[u8; 64]> = row.seals.iter().map(|s| s.operator).collect();
    operators.sort();
    operators.dedup();
    let mut judgement = PalwBootstrapJudgementV1 {
        judged_daa: now,
        work_id,
        output: None,
        independent_contributors: operators.len() as u32,
        depth_daa: now.saturating_sub(row.last_input_daa()),
        validity_refusal: None,
        eligibility_refusal: None,
    };
    let revealed: Option<Vec<([u8; 64], [u8; 64])>> =
        row.seals.iter().map(|s| s.reveal.map(|(preimage, _)| (s.contributor, preimage))).collect();
    let Some(revealed) = revealed.filter(|r| !r.is_empty()) else {
        let code = if row.seals.is_empty() { "NO_INPUT" } else { "WITHHELD" };
        judgement.validity_refusal = Some(code.to_string());
        judgement.eligibility_refusal = Some("NOT_JUDGED".to_string());
        return (judgement, None);
    };
    let output = bootstrap_exact_output_v1(&round_id, &revealed);
    judgement.output = Some(output);
    let candidate = PalwBootstrapExactCandidateV1 {
        profile_id: Hash64::from_bytes(row.round.profile),
        work_id: Hash64::from_bytes(work_id),
        exact: Ok(Hash64::from_bytes(output)),
        resource_units: row.seals.len() as u64,
        reads: vec![N::Genesis, N::KernelActive],
        reads_its_consumer: false,
    };
    let validity = match palw_bootstrap_independent_validity_v1(&candidate, profile) {
        Ok(validity) => validity,
        Err(refusal) => {
            judgement.validity_refusal = Some(refusal.code.to_string());
            judgement.eligibility_refusal = Some("NOT_JUDGED".to_string());
            return (judgement, None);
        }
    };
    let facts = PalwBootstrapEntropyFactsV1 {
        target_committed_position: row.target_committed_daa,
        inputs_fixed_position: row.seals.iter().map(|s| s.sealed_daa).max().unwrap_or(0),
        origin: PalwEntropyOriginV1::IndependentCommitReveal,
        independent_contributors: judgement.independent_contributors,
        selection: PalwSourceSelectionV1::CanonicalFirstK,
        trials: PalwTrialAccountingV1::EveryTrialCommittedOnChain,
        withholding: PalwWithholdingRuleV1::AbortRound,
        derivation: PalwSourceDerivationV1::SelectedChainAtDepth(judgement.depth_daa),
        identity: PalwWorkIdentityBindingV1::ContentOnly,
    };
    match palw_bootstrap_entropy_eligibility_v1(&validity, &facts, profile) {
        Ok(eligible) => (judgement, Some(eligible)),
        Err(refusal) => {
            judgement.eligibility_refusal = Some(refusal.code.to_string());
            (judgement, None)
        }
    }
}

/// **The eligible sources the chain holds for `(profile, target)`**, re-derived from the stored rounds under the fence's policy (the
/// witnesses have private fields: re-running the pure judgement at its `judged_daa` is the only way to build one). Also the target's
/// commitment position (`None`: no judged round).
pub fn palw_bootstrap_eligible_in_state_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    params: &crate::palw_state_v2::PalwStateParamsV2,
    profile_id: Hash64,
    target: &[u8; 64],
) -> (Option<u64>, Vec<EntropyEligibilityVerifiedV1>) {
    let mut out = (None, Vec::new());
    let Some((_, policy)) = params.econ_contracts_v1().beacon_bootstrap.as_ref() else { return out };
    let Some(route) = state.kernel_route() else { return out };
    if policy.profile.id != profile_id {
        return out;
    }
    for ((_, _), bytes) in
        route.aux.range((PALW_BEACON_BOOTSTRAP_TABLE_V1, Vec::new())..(PALW_BEACON_BOOTSTRAP_TABLE_V1 + 1, Vec::new()))
    {
        let Ok(row) = borsh::from_slice::<PalwBootstrapRoundRowV1>(bytes) else { continue };
        let Some(judged) = row.judgement.as_ref().map(|j| j.judged_daa) else { continue };
        if row.round.target != *target || row.round.profile != profile_id.as_bytes() {
            continue;
        }
        out.0 = Some(row.target_committed_daa);
        if let (_, Some(eligible)) = palw_bootstrap_judge_round_v1(&row, &policy.profile, judged) {
            out.1.push(eligible);
        }
    }
    out
}

/// **[`palw_bootstrap_holds_v1`] over the chain's stored rounds**: the release's approved profiles decide (empty: `NO_APPROVED_PROFILE`
/// everywhere — nothing activates); a synthetic profile never holds; enough distinct eligible works for the target do.
pub fn palw_bootstrap_holds_in_state_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    params: &crate::palw_state_v2::PalwStateParamsV2,
    profile_id: Hash64,
    target: &[u8; 64],
    approved: &[PalwBootstrapEntropyProfileV1],
) -> Result<PalwBootstrapHoldsV1, PalwBootstrapRefusalV1> {
    let (position, eligible) = palw_bootstrap_eligible_in_state_v1(state, params, profile_id, target);
    palw_bootstrap_holds_v1(&eligible, profile_id, position.unwrap_or(0), approved)
}

impl crate::config::params::Params {
    pub fn palw_beacon_bootstrap_activation(&self) -> Option<u64> {
        self.palw_beacon_bootstrap_v1
            .filter(|f| f.activation != crate::config::params::ForkActivation::never())
            .map(|f| f.activation.daa_score())
    }

    /// **The bootstrap fence's refusal**: its policy, the kernel route at or below it (its rows are the route's), then the blanket
    /// refusal — no BootstrapExact profile, kernel or entropy assumption is approved.
    pub fn validate_palw_beacon_bootstrap_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error;
        let Some(at) = self.palw_beacon_bootstrap_activation() else { return Ok(()) };
        let fence = self.palw_beacon_bootstrap_v1.expect("armed");
        fence.policy.validate().map_err(PalwModeV2Error::Invalid)?;
        let route = self
            .palw_probabilistic_constraints_v1
            .filter(|f| *f != crate::config::params::ForkActivation::never())
            .map(|f| f.daa_score());
        if route.is_none_or(|r| r > at) {
            return Err(PalwModeV2Error::Invalid(
                "palw_beacon_bootstrap_v1 requires palw_probabilistic_constraints_v1 (the kernel route holding its rounds) at or below it",
            ));
        }
        Err(PalwModeV2Error::Invalid(
            "palw_beacon_bootstrap_v1 cannot be armed: no BootstrapExact kernel, resource bound or entropy profile is approved (N3, \
             REQ-N3-3); approved_bootstrap_entropy_profiles_v1 is empty",
        ))
    }
}

impl crate::palw_state_v2::PalwChainStateV2 {
    /// **Semantic restore fence of aux 130.** A round exists only past its fence, under its id, canonical and inside its derived
    /// windows; every stored judgement is exactly what the pure judgement re-derives at its `judged_daa`; unjudged rounds within
    /// capacity.
    pub fn assert_beacon_bootstrap_consistency_v1(
        &self,
        params: &crate::palw_state_v2::PalwStateParamsV2,
    ) -> Result<(), crate::palw_state_v2::PalwStateV2Error> {
        use crate::palw_state_v2::PalwStateV2Error;
        let refuse = |why: String| PalwStateV2Error::CarriageInconsistent(format!("beacon bootstrap: {why}"));
        let Some(route) = self.kernel_route() else { return Ok(()) };
        let mut open = 0u64;
        for ((_, key), bytes) in
            route.aux.range((PALW_BEACON_BOOTSTRAP_TABLE_V1, Vec::new())..(PALW_BEACON_BOOTSTRAP_TABLE_V1 + 1, Vec::new()))
        {
            let (_, policy) =
                params.econ_contracts_v1().beacon_bootstrap.as_ref().ok_or_else(|| refuse("a round without its fence".into()))?;
            let row: PalwBootstrapRoundRowV1 = borsh::from_slice(bytes).map_err(|e| refuse(e.to_string()))?;
            row.validate(policy).map_err(|e| refuse(e.into()))?;
            if key.as_slice() != row.round.id().as_slice() {
                return Err(refuse("a round under another key".into()));
            }
            match &row.judgement {
                Some(judgement) => {
                    if palw_bootstrap_judge_round_v1(&row, &policy.profile, judgement.judged_daa).0 != *judgement {
                        return Err(refuse("a stored judgement differs from its re-derivation".into()));
                    }
                }
                None => open += 1,
            }
            if open > u64::from(policy.maximum_open_rounds) {
                return Err(refuse("more unjudged rounds than the policy holds".into()));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod round3_tests {
    use super::*;
    use crate::config::params::{ForkActivation, MAINNET_PARAMS, TESTNET_PARAMS};

    fn policy() -> PalwBeaconBootstrapPolicyV1 {
        PalwBeaconBootstrapPolicyV1::unapproved_probe_v1()
    }

    fn round(slot: u32) -> BootstrapExactRoundV1 {
        BootstrapExactRoundV1 { profile: policy().profile.id.as_bytes(), target: [3; 64], slot }
    }

    /// Target committed at 100; seals at 101/102 (seal end 110); reveals at 112/113 (reveal end 120), `operators[i]` per contributor.
    fn row(slot: u32, operators: [u8; 2], revealed: [bool; 2]) -> PalwBootstrapRoundRowV1 {
        let r = round(slot);
        let seals = (0..2)
            .map(|i| {
                let contributor = [10 + i as u8; 64];
                let preimage = [20 + i as u8; 64];
                PalwBootstrapSealRowV1 {
                    contributor,
                    operator: [operators[i]; 64],
                    commitment: bootstrap_exact_commitment_v1(&r.id(), &contributor, &preimage),
                    sealed_daa: 101 + i as u64,
                    reveal: revealed[i].then_some((preimage, 112 + i as u64)),
                }
            })
            .collect();
        PalwBootstrapRoundRowV1 { round: r, target_committed_daa: 100, seals, judgement: None, policy: policy().id() }
    }

    #[test]
    fn n3_round3_the_fence_is_dormant_everywhere_and_refused_when_armed() {
        policy().validate().unwrap();
        for p in [MAINNET_PARAMS, TESTNET_PARAMS] {
            assert_eq!(p.palw_beacon_bootstrap_v1, None);
            p.validate_palw_beacon_bootstrap_v1().unwrap();
            assert!(p.palw_fences_v1().contains(&("palw_beacon_bootstrap_v1", None)));
        }
        let mut p = TESTNET_PARAMS;
        p.palw_beacon_bootstrap_v1 = Some(PalwBeaconBootstrapFenceV1::unapproved_probe_at(ForkActivation::never()));
        p.validate_palw_beacon_bootstrap_v1().unwrap();
        p.palw_beacon_bootstrap_v1 = Some(PalwBeaconBootstrapFenceV1::unapproved_probe_at(ForkActivation::new(1_000)));
        assert!(p.validate_palw_beacon_bootstrap_v1().is_err(), "no approved profile: refused when armed");
        assert!(!PALW_N3_GATED_REWARD_FENCES_V1.contains(&"palw_beacon_bootstrap_v1"), "the bootstrap is not a reward profile");
    }

    /// The fold's judgement derives every fact itself and runs the two predicates: a fully revealed round of two independent
    /// operators is valid AND eligible under the synthetic profile; re-judging at the same DAA is identical (the restore fence).
    #[test]
    fn n3_round3_a_settled_independent_round_is_judged_valid_and_eligible_by_derived_facts() {
        let p = policy();
        let r = row(0, [1, 2], [true, true]);
        r.validate(&p).unwrap();
        assert!(!r.due(&p, 120), "the reveal window is still open");
        assert!(r.due(&p, 121), "closed, and the last input is 8 deep (≥ 2)");
        let (judgement, eligible) = palw_bootstrap_judge_round_v1(&r, &p.profile, 121);
        assert_eq!((judgement.validity_refusal.as_deref(), judgement.eligibility_refusal.as_deref()), (None, None), "{judgement:?}");
        assert_eq!(judgement.independent_contributors, 2);
        assert_eq!(judgement.depth_daa, 8);
        assert!(judgement.output.is_some());
        let eligible = eligible.expect("eligible");
        assert_eq!(eligible.target_committed_position(), 100);
        assert_eq!(palw_bootstrap_judge_round_v1(&r, &p.profile, 121).0, judgement, "deterministic re-derivation");
        let judged = PalwBootstrapRoundRowV1 { judgement: Some(judgement), ..r.clone() };
        judged.validate(&p).unwrap();
        let early = PalwBootstrapRoundRowV1 { judgement: Some(palw_bootstrap_judge_round_v1(&r, &p.profile, 120).0), ..r };
        assert!(early.validate(&p).is_err(), "never judged before it is due");
    }

    /// Withholding aborts the round (the fold's rule is `AbortRound`); one operator's two bonds are one contributor; neither is a
    /// source. A seal at or before the target's commitment, or a reveal of another preimage, is not a round's.
    #[test]
    fn n3_round3_withheld_or_single_operator_rounds_are_not_sources() {
        let p = policy();
        let withheld = row(0, [1, 2], [true, false]);
        assert!(withheld.due(&p, 121), "an aborted round is judged once its reveal window closed");
        let (j, e) = palw_bootstrap_judge_round_v1(&withheld, &p.profile, 121);
        assert_eq!(j.validity_refusal.as_deref(), Some("WITHHELD"));
        assert!(j.output.is_none() && e.is_none());
        let (j, e) = palw_bootstrap_judge_round_v1(&row(0, [1, 1], [true, true]), &p.profile, 121);
        assert_eq!(j.validity_refusal, None, "valid: the exact output is re-executed");
        assert_eq!(j.eligibility_refusal.as_deref(), Some("TOO_FEW_CONTRIBUTORS"));
        assert!(e.is_none());
        let mut early = row(0, [1, 2], [false, false]);
        early.seals[0].sealed_daa = 100;
        assert!(early.validate(&p).is_err(), "inputs are fixed strictly after the target's commitment");
        let mut forged = row(0, [1, 2], [true, true]);
        forged.seals[0].reveal = Some(([99; 64], 112));
        assert!(forged.validate(&p).is_err(), "a reveal opens its own commitment");
    }

    /// REQ-N3-3 over the stored judgements: two eligible rounds of one target hold only under an approved PRODUCTION profile — the
    /// release approves none, and the synthetic probe profile never holds.
    #[test]
    fn n3_round3_holds_reads_the_judgements_and_the_release_approves_nothing() {
        let p = policy();
        let eligible: Vec<_> = [0, 1]
            .iter()
            .map(|slot| palw_bootstrap_judge_round_v1(&row(*slot, [1, 2], [true, true]), &p.profile, 121).1.unwrap())
            .collect();
        let release = approved_bootstrap_entropy_profiles_v1();
        assert_eq!(palw_bootstrap_holds_v1(&eligible, p.profile.id, 100, &release).unwrap_err().code, "NO_APPROVED_PROFILE");
        assert_eq!(
            palw_bootstrap_holds_v1(&eligible, p.profile.id, 100, std::slice::from_ref(&p.profile)).unwrap_err().code,
            "SYNTHETIC_PROFILE"
        );
        let production = [PalwBootstrapEntropyProfileV1 { class: PalwBootstrapProfileClassV1::Production, ..p.profile }];
        assert_eq!(palw_bootstrap_holds_v1(&eligible, p.profile.id, 100, &production).unwrap().sources(), 2, "test-only approval");
        assert_eq!(palw_bootstrap_holds_v1(&eligible[..1], p.profile.id, 100, &production).unwrap_err().code, "TOO_FEW_SOURCES");
    }
}
