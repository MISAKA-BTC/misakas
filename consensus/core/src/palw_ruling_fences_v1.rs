//! **The rules of the user's 2026-10-12 rulings** (`docs/design/palw/mainnet-spec-register-2026-10-12.md` §3,
//! `docs/design/palw/rfc-adr-review-resolution-2026-10-12.md` "ユーザー判断 — 2026-10-12") — three DORMANT fences, each `None` on every
//! preset and refused when armed by [`Params::validate_palw_2026_10_12_ruling_fences_v1`]:
//!
//! ```text
//! palw_claim_hard_deadline_v1     X3   a claim accepted at/after the fence has ONE absolute dispute deadline, fixed at acceptance (d0):
//!                                      receipt_deadline = d0 + T_prepare, open_cutoff = receipt_deadline + T_observe,
//!                                      hard = min(open_cutoff + T_resolve, trace_retention_daa)            (REQ-X3-1)
//!                                      its Final deadline is min(proposed, hard)                           (REQ-X3-2)
//!                                      nothing re-grants or extends it: no held-decoy re-arm, no DA pause credit, no staged-court
//!                                      or model-window retention extension                                  (REQ-X3-3)
//!                                      a pursuit starts only if start <= open_cutoff and start + B_remaining <= hard  (REQ-X3-4)
//!                                      reaching hard never Finals a claim an eligible pursuit still holds    (REQ-X3-5)
//! palw_public_evidence_reward_v1  N4   a free-prompt claim accepted at/after the fence whose required evidence cannot be public
//!                                      (PanelDa prompt ids, FP Job V5 images / source ids) takes no new reward or Final weight:
//!                                      the claim gate refuses it, as G14-for-rewards refuses a non-onboarded class  (REQ-N4-1)
//!                                      a claim accepted before the fence keeps its old ruleset                (REQ-N4-2)
//! palw_court_model_scope_v1       D2   the four ARMED t12 routes that compel model bytes (`HeldStepLeaf`, `AttnRootClaimOperands`,
//!                                      `TirGenRootClaimParams`, `SeatReadinessLeaves`) are not admissible for a claim accepted
//!                                      at/after the fence (readiness: for a class registered at/after it); old claims keep them
//!                                      (REQ-T6-3, restricted)
//! ```
//!
//! **A-2 uniformity.** Every predicate here answers "old rule" for a claim accepted below its fence, and every fence's mirror on the
//! V2 bundle's state params is `None` on every shipped preset, so each hook in the fold is a no-op on every live state: the same
//! verdicts, the same writes, the same roots. Old claims keep old rulesets (no retroactivity): every rule keys on the CLAIM's
//! acceptance DAA (or the class's registration DAA), never on the block's alone.
//!
//! **Not built here (named, not invented):** the X3 policy terms (`T_prepare`, `T_observe`, `T_resolve`, and the `B_*` budget terms
//! with no existing source) are fields of the fence's policy with NO production value — only [`PalwClaimHardDeadlineFenceV1::
//! unapproved_probe_at`]'s structural fixture values exist, and validation refuses every real activation. `B_disclose` is the
//! network's DA disclose window and `B_court` its court window (the existing sources REQ-T12-3 names). D2's replacement routes
//! (a dissection whose root claim names coordinates only, the challenger supplying the openings — DA16 design §7.5 items 1–2) are not
//! built: past the fence a held dissection on a new claim is simply not opened, and a one-move verdict with accuser-supplied openings
//! stays the route.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::Hash64;
use crate::config::params::{ForkActivation, Params};
use crate::palw_court_scope_v1::{CourtScopeRefusalV1, CourtUnitKindV1, palw_court_unit_scope_v1};
use crate::palw_freeprompt_v3::{PALW_FP_PRIVACY_PANEL_DA, PalwFpJobTailV1, PalwFreePromptCommitmentV3, PalwFreePromptJobV3};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
use crate::palw_state_v2::{PalwClaimStateV2, PalwStateParamsV2, palw_da_disclose_window_daa_v1};

// =================================================================================================================================
// X3 — `palw_claim_hard_deadline_v1`
// =================================================================================================================================

/// The X3 policy's wire version.
pub const PALW_CLAIM_HARD_DEADLINE_POLICY_VERSION_V1: u16 = 1;

/// **X3's terms** (RFC-0014 2026-10-12 amendment §7.2補足; REQ-T12-3's start budget). Every value is POLICY with no approved
/// production value: a ruleset may carry it only through a fence validation refuses.
///
/// `B_remaining = B_cold_fetch + B_check + B_localize + B_disclose + B_court + B_carrier + B_reorg_slack`, with `B_disclose` the
/// network's DA disclose window ([`palw_da_disclose_window_daa_v1`]) and `B_court` its court window (`window_court`) — the two
/// terms with an existing source — and the other five named here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwClaimHardDeadlinePolicyV1 {
    pub version: u16,
    /// `T_prepare`: acceptance → the receipt deadline.
    pub prepare_daa: u64,
    /// `T_observe`: the receipt deadline → the open cutoff (the last DAA a pursuit may start).
    pub observe_daa: u64,
    /// `T_resolve`: the open cutoff → the hard deadline.
    pub resolve_daa: u64,
    /// `B_cold_fetch`: a verifier obtaining the claim's public material from cold.
    pub cold_fetch_daa: u64,
    /// `B_check`: checking it.
    pub check_daa: u64,
    /// `B_localize`: the worst-case localization (the legacy route's descent, or the class's plan envelope).
    pub localize_daa: u64,
    /// `B_carrier`: the carrier inclusion allowance for the pursuit's objects.
    pub carrier_daa: u64,
    /// `B_reorg_slack`: reorg slack.
    pub reorg_slack_daa: u64,
}

impl PalwClaimHardDeadlinePolicyV1 {
    /// The policy's identity, hashed (Some-only) into both fingerprints beside the fence's height.
    pub fn id(&self) -> Hash64 {
        Hash64::from_bytes(misaka_palw_challenge::hash::object_id(b"MISAKA/PALW/CLAIM-HARD-DEADLINE-POLICY/V1", self))
    }

    /// Structural refusals only (no value here is approved): the version, every span at least one DAA, and the deadline sum finite.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != PALW_CLAIM_HARD_DEADLINE_POLICY_VERSION_V1 {
            return Err("palw_claim_hard_deadline_v1: an unknown policy version");
        }
        if [self.prepare_daa, self.observe_daa, self.resolve_daa].contains(&0) {
            return Err("palw_claim_hard_deadline_v1: T_prepare, T_observe and T_resolve must each be at least one DAA");
        }
        let total = [self.prepare_daa, self.observe_daa, self.resolve_daa]
            .into_iter()
            .chain([self.cold_fetch_daa, self.check_daa, self.localize_daa, self.carrier_daa, self.reorg_slack_daa])
            .try_fold(0u64, u64::checked_add);
        if total.is_none_or(|t| t == u64::MAX) {
            return Err("palw_claim_hard_deadline_v1: the policy's spans overflow");
        }
        Ok(())
    }
}

/// **The X3 fence**: its height and its policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwClaimHardDeadlineFenceV1 {
    pub activation: ForkActivation,
    pub policy: PalwClaimHardDeadlinePolicyV1,
}

impl PalwClaimHardDeadlineFenceV1 {
    /// Structural private-fixture values for the fork-id probe and the A-2 harness. This grants no policy approval: validation
    /// refuses every armed value whatever its policy.
    pub fn unapproved_probe_at(activation: ForkActivation) -> Self {
        Self {
            activation,
            policy: PalwClaimHardDeadlinePolicyV1 {
                version: PALW_CLAIM_HARD_DEADLINE_POLICY_VERSION_V1,
                prepare_daa: 40,
                observe_daa: 60,
                resolve_daa: 200,
                cold_fetch_daa: 4,
                check_daa: 4,
                localize_daa: 8,
                carrier_daa: 2,
                reorg_slack_daa: 2,
            },
        }
    }
}

/// **The X3 mirror** the fold reads (`PalwStateParamsV2::claim_hard_deadline_v1`), written only by
/// [`Params::sync_palw_2026_10_12_ruling_fences_v1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwClaimHardDeadlineMirrorV1 {
    pub from_daa: u64,
    pub policy: PalwClaimHardDeadlinePolicyV1,
}

/// **One X3 claim's fixed deadlines** — a pure function of the claim's acceptance DAA, the fence's policy and the claim's retention
/// obligation (which nothing extends for an X3 claim, REQ-X3-3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwClaimDeadlinesV1 {
    pub accepted_daa: u64,
    pub receipt_deadline_daa: u64,
    pub open_cutoff_daa: u64,
    pub hard_deadline_daa: u64,
}

/// **REQ-X3-1: a claim's deadlines, fixed at acceptance (d0)** — `None` for a claim accepted below the fence (or where no network
/// arms it: every shipped one), which keeps every old rule. The hard deadline never exceeds the claim's retention obligation: no
/// pursuit can be resolved on material the producer no longer owes.
pub fn palw_claim_deadlines_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> Option<PalwClaimDeadlinesV1> {
    let mirror = params.claim_hard_deadline_v1()?;
    if claim.accepted_daa < mirror.from_daa {
        return None;
    }
    let p = mirror.policy;
    let receipt_deadline_daa = claim.accepted_daa.saturating_add(p.prepare_daa);
    let open_cutoff_daa = receipt_deadline_daa.saturating_add(p.observe_daa);
    let hard_deadline_daa = open_cutoff_daa.saturating_add(p.resolve_daa).min(claim.trace_retention_daa);
    Some(PalwClaimDeadlinesV1 {
        accepted_daa: claim.accepted_daa,
        receipt_deadline_daa,
        open_cutoff_daa: open_cutoff_daa.min(hard_deadline_daa),
        hard_deadline_daa,
    })
}

/// **The claim's `dispute_hard_deadline`** under X3, `None` under the old ruleset. The helper a consent-signed extension path (or
/// any later rule) bounds itself by.
pub fn palw_claim_hard_deadline_of_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> Option<u64> {
    palw_claim_deadlines_v1(params, claim).map(|d| d.hard_deadline_daa)
}

/// **Does X3 govern this claim?** (Accepted at/after the fence.) Every REQ-X3-3 guard asks this one question.
pub fn palw_claim_x3_governs_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> bool {
    palw_claim_deadlines_v1(params, claim).is_some()
}

/// **REQ-X3-2: `effective = min(proposed, hard)`** — `proposed` unchanged where there is no hard deadline (the old ruleset).
pub fn palw_claim_effective_deadline_v1(proposed: u64, hard: Option<u64>) -> u64 {
    hard.map_or(proposed, |hard| proposed.min(hard))
}

/// **`B_remaining`** (REQ-T12-3): the policy's five named terms plus the network's DA disclose window and court window.
pub fn palw_claim_pursuit_budget_v1(params: &PalwStateParamsV2, policy: &PalwClaimHardDeadlinePolicyV1) -> u64 {
    [
        policy.cold_fetch_daa,
        policy.check_daa,
        policy.localize_daa,
        palw_da_disclose_window_daa_v1(params),
        params.window_court(),
        policy.carrier_daa,
        policy.reorg_slack_daa,
    ]
    .into_iter()
    .fold(0u64, u64::saturating_add)
}

/// **REQ-X3-4: may a pursuit start on `claim` at `start`?** `Ok` under the old ruleset. Under X3: only at or before the open cutoff,
/// and only if `start + B_remaining <= hard` — a pursuit that could not finish inside the claim's fixed deadline is not begun.
/// Boundaries: admitted at `d <= deadline`, refused at `d > deadline`.
pub fn palw_claim_pursuit_may_start_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2, start: u64) -> Result<(), &'static str> {
    let Some(deadlines) = palw_claim_deadlines_v1(params, claim) else { return Ok(()) };
    let policy = params.claim_hard_deadline_v1().expect("deadlines exist only under the mirror").policy;
    if start > deadlines.open_cutoff_daa {
        return Err("X3: the claim's open cutoff has passed; no new pursuit starts (palw_claim_hard_deadline_v1)");
    }
    if start.saturating_add(palw_claim_pursuit_budget_v1(params, &policy)) > deadlines.hard_deadline_daa {
        return Err("X3: start + B_remaining exceeds the claim's hard deadline; the pursuit could not finish (palw_claim_hard_deadline_v1)");
    }
    Ok(())
}

// =================================================================================================================================
// N4 — `palw_public_evidence_reward_v1`
// =================================================================================================================================

/// The refusal code the claim gate reports for an N4 claim (`PalwStateV2Error::ClassNotRewardable`'s `code`).
pub const PALW_N4_NONPUBLIC_EVIDENCE_CODE_V1: &str = "N4_NONPUBLIC_EVIDENCE";

/// **Can a free-prompt job's required evidence be public?** `Err` names the non-public route: `PanelDa` (the prompt ids ride only
/// the capture served to the Panel), FP Job V5 (images and source ids travel only to the worker and the Panel, under either privacy
/// mode).
pub fn palw_fp_job_evidence_public_v1(job: &PalwFreePromptJobV3) -> Result<(), &'static str> {
    if job.privacy_mode == PALW_FP_PRIVACY_PANEL_DA {
        return Err("PanelDa: the prompt ids ride only the capture served to the Panel, never the chain");
    }
    if job.version == crate::palw_fp_job_v5::PALW_FP_V5_VERSION || matches!(job.tail, Some(PalwFpJobTailV1::V5(_))) {
        return Err("FP Job V5: the job's images and source ids travel only to the worker and the Panel");
    }
    Ok(())
}

/// **REQ-N4-1 / REQ-N4-2: the claim gate's N4 question** for a free-prompt commitment accepted at `accepted_daa`: `None` below the
/// fence (the old ruleset, REQ-N4-2) or for a commitment whose evidence can be public; `Some(why)` past it for a non-public route —
/// or for a commitment the extractor did not hand over (nothing shows its evidence public).
pub fn palw_public_evidence_reward_refusal_v1(
    params: &PalwStateParamsV2,
    accepted_daa: u64,
    commitment: Option<&PalwFreePromptCommitmentV3>,
) -> Option<&'static str> {
    if !params.public_evidence_reward_active_at(accepted_daa) {
        return None;
    }
    match commitment {
        None => Some("no signed commitment shows this claim's evidence public"),
        Some(commitment) => palw_fp_job_evidence_public_v1(&commitment.job).err(),
    }
}

// =================================================================================================================================
// D2 — `palw_court_model_scope_v1`
// =================================================================================================================================

/// **The four ARMED t12 routes that compel model bytes** (`palw_court_scope_v1`'s inventory test pins exactly these).
pub const PALW_D2_RESTRICTED_UNITS_V1: [CourtUnitKindV1; 4] = [
    CourtUnitKindV1::HeldStepLeaf,
    CourtUnitKindV1::AttnRootClaimOperands,
    CourtUnitKindV1::TirGenRootClaimParams,
    CourtUnitKindV1::SeatReadinessLeaves,
];

/// **Does D2 govern this claim?** (Accepted at/after the fence.)
pub fn palw_court_model_scope_governs_claim_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> bool {
    params.court_model_scope_from_daa().is_some_and(|from| claim.accepted_daa >= from)
}

/// **REQ-T6-3 (restricted): is `kind` admissible on `claim`?** A restricted unit on a D2 claim is refused with the scope's own
/// refusal (model bytes); every other unit, and every unit on an old claim, is what `palw_court_demand_allowed_v1`'s live routes
/// already admit.
pub fn palw_court_model_scope_admits_v1(
    params: &PalwStateParamsV2,
    claim: &PalwClaimStateV2,
    kind: CourtUnitKindV1,
) -> Result<(), CourtScopeRefusalV1> {
    if palw_court_model_scope_governs_claim_v1(params, claim) && PALW_D2_RESTRICTED_UNITS_V1.contains(&kind) {
        return Err(CourtScopeRefusalV1::ModelBytes(palw_court_unit_scope_v1(kind).material));
    }
    Ok(())
}

/// **D2 on seat readiness** (`SeatReadinessLeaves`, a per-(class, bond, span) seat condition, not a per-claim demand): past the
/// fence a readiness proof for a class REGISTERED at/after the fence is refused — every claim of such a class is a new claim, while
/// a class registered before keeps the seating its old claims rely on. `class_registered_daa` is `None` for a class the state does
/// not hold (the readiness arm's own refusals answer for it).
pub fn palw_court_model_scope_admits_readiness_v1(
    params: &PalwStateParamsV2,
    daa_score: u64,
    class_registered_daa: Option<u64>,
) -> Result<(), &'static str> {
    match (params.court_model_scope_from_daa(), class_registered_daa) {
        (Some(from), Some(registered)) if daa_score >= from && registered >= from => Err(
            "palw_court_model_scope_v1: a class registered past the fence publishes no plaintext model leaves for readiness (ADR-0177 D2)",
        ),
        _ => Ok(()),
    }
}

// =================================================================================================================================
// The fences' mirrors on the V2 bundle, and their refusals
// =================================================================================================================================

impl Params {
    /// The X3 fence's mirror value: `Some` only for a real height.
    pub fn palw_claim_hard_deadline_v1_mirror(&self) -> Option<PalwClaimHardDeadlineMirrorV1> {
        self.palw_claim_hard_deadline_v1
            .filter(|f| f.activation != ForkActivation::never())
            .map(|f| PalwClaimHardDeadlineMirrorV1 { from_daa: f.activation.daa_score(), policy: f.policy })
    }

    /// **The three fences' mirrors** on the V2 bundle's state params (`court_model_scope_from_daa`, `public_evidence_reward_from_daa`,
    /// `claim_hard_deadline_v1`), which the fold reads. Written here and nothing else; `None` where a fence is not armed (or is
    /// `never()`).
    pub fn sync_palw_2026_10_12_ruling_fences_v1(&mut self) {
        let scope = self.palw_court_model_scope_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        let evidence = self.palw_public_evidence_reward_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        let deadline = self.palw_claim_hard_deadline_v1_mirror();
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle
                .state
                .clone()
                .with_court_model_scope_from_daa(scope)
                .with_public_evidence_reward_from_daa(evidence)
                .with_claim_hard_deadline_v1(deadline);
        }
    }

    /// **The mirrors agree with the fences** (asked by [`Self::validate_palw_2026_10_12_ruling_fences_v1`] after its arming
    /// refusals), and an X3 policy that is present is structurally sound.
    pub fn validate_palw_2026_10_12_ruling_mirrors_v1(&self) -> Result<(), PalwModeV2Error> {
        if let Some(f) = self.palw_claim_hard_deadline_v1.filter(|f| f.activation != ForkActivation::never()) {
            f.policy.validate().map_err(PalwModeV2Error::Invalid)?;
        }
        let PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode else { return Ok(()) };
        let state = &bundle.state;
        if state.court_model_scope_from_daa()
            != self.palw_court_model_scope_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score())
        {
            return Err(PalwModeV2Error::Invalid(
                "palw_court_model_scope_v1 disagrees with the V2 bundle's mirror: Params::sync_palw_2026_10_12_ruling_fences_v1",
            ));
        }
        if state.public_evidence_reward_from_daa()
            != self.palw_public_evidence_reward_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score())
        {
            return Err(PalwModeV2Error::Invalid(
                "palw_public_evidence_reward_v1 disagrees with the V2 bundle's mirror: Params::sync_palw_2026_10_12_ruling_fences_v1",
            ));
        }
        if state.claim_hard_deadline_v1() != self.palw_claim_hard_deadline_v1_mirror() {
            return Err(PalwModeV2Error::Invalid(
                "palw_claim_hard_deadline_v1 disagrees with the V2 bundle's mirror: Params::sync_palw_2026_10_12_ruling_fences_v1",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
