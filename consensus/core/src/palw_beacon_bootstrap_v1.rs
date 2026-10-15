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
//! none is approved. No fence, no `Params` field, no tag, no table; the fold reads nothing here. Source-only bootstrap records earn
//! no PALW reward, block right or Final weight (amendments §6). No hidden fallback to a block hash, heartbeat, BASE-0, signature,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwBootstrapProfileClassV1 {
    Production,
    SyntheticTestOnly,
}

/// What a profile's bias bound rests on. Only an adversarial analysis over a stated attack budget counts (S04: "単なる統計乱数test
/// で合格にしない").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwWithholdingRuleV1 {
    /// The commitment's deposit is forfeited and the round proceeds with a fixed, pre-committed substitute.
    ForfeitAndSubstitute,
    /// The withholder is skipped (the last revealer chooses between two outputs).
    SkipWithholder,
    /// The round aborts and restarts (the last revealer can force a redraw).
    AbortRound,
}

/// **A reviewed bootstrap entropy profile.** Every comparison value of the two judgements is a field here; this module chooses none.
#[derive(Clone, Debug, PartialEq, Eq)]
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
