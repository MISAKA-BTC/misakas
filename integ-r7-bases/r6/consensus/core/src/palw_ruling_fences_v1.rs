//! **The rules of the user's 2026-10-12 rulings** (`docs/design/palw/integration/mainnet-spec-register-2026-10-12.md` §3,
//! `docs/audit/palw/design-reviews/rfc-adr-review-resolution-2026-10-12.md` "ユーザー判断 — 2026-10-12") — three DORMANT fences, each `None` on every
//! preset and refused when armed by [`Params::validate_palw_2026_10_12_ruling_fences_v1`]:
//!
//! ```text
//! palw_claim_hard_deadline_v1     X3   a claim accepted at/after the fence has ONE absolute dispute deadline, fixed at acceptance (d0):
//!                                      receipt_deadline = d0 + T_prepare, open_cutoff = receipt_deadline + T_observe,
//!                                      hard = open_cutoff + T_resolve                                      (REQ-X3-1)
//!                                      its material is owed through hard: retention = max(retention, hard),
//!                                      raised once at creation (Lead default 2026-10-15 round 3)          (REQ-X3-6)
//!                                      its Final deadline is min(proposed, hard)                           (REQ-X3-2)
//!                                      nothing re-grants or extends it: no held-decoy re-arm, no DA pause credit, no staged-court
//!                                      or model-window retention extension                                  (REQ-X3-3)
//!                                      a pursuit starts only if start <= open_cutoff and start + B_remaining <= hard  (REQ-X3-4)
//!                                      reaching hard never Finals a claim an eligible pursuit still holds    (REQ-X3-5)
//! palw_public_evidence_reward_v1  N4   a free-prompt claim accepted at/after the fence whose required evidence cannot be public
//!                                      (PanelDa prompt ids, FP Job V5 images / source ids) takes no new reward or Final weight:
//!                                      the claim gate refuses it, as G14-for-rewards refuses a non-onboarded class  (REQ-N4-1)
//!                                      a claim accepted before the fence keeps its old ruleset                (REQ-N4-2)
//!                                      a weightless tensor claim (tag 87) is asked the same question at the one site a later
//!                                      fence would grant it reward or weight (the grant guard)               (REQ-N4-3)
//! palw_court_model_scope_v1       D2   the four ARMED t12 routes that compel model bytes (`HeldStepLeaf`, `AttnRootClaimOperands`,
//!                                      `TirGenRootClaimParams`, `SeatReadinessLeaves`) are not admissible for a claim accepted
//!                                      at/after the fence (readiness: for a class registered at/after it); old claims keep them
//!                                      (REQ-T6-3, restricted); each has a replacement route that compels no model bytes
//!                                      (`palw_court_model_scope_routes_v1`, REQ-T6-4)
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
//! network's DA disclose window and `B_court` its court window (the existing sources REQ-T12-3 names). X3's retention cost term
//! (`retention_cost_per_daa_sompi`) is a named policy field with no production value; the pricing callers add
//! [`palw_x3_retention_cost_sompi_v1`] to their material term. D2's replacement routes (DA16 design §7.5 items 1–2) are
//! [`crate::palw_court_model_scope_routes_v1`]'s: the claim part of a leaf with the accuser's own rows in the one move, and a named-leaf
//! dissection whose root claim's model operands the CHALLENGER publishes in the accusation that opens it (the accused signs coordinates
//! and claim values only, and relays public bytes).

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
    /// **`c_retain`: the provider/material hold cost of one DAA of retention past the claim's own obligation** (sompi per DAA), the
    /// price of REQ-X3-6's extension (`retention = max(retention, hard)`). A named input of the material term the pricing contracts
    /// read ([`palw_x3_retention_cost_sompi_v1`]); `0` is UNMEASURED, never free — a caller pricing a positive extension at `0` is
    /// refused (the econ rule: unknown costs are no bound, not zero).
    pub retention_cost_per_daa_sompi: u64,
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
                retention_cost_per_daa_sompi: 1,
            },
        }
    }

    /// **G-09: the X3 CANDIDATE derived from the existing horizons** (2026-10-16, cross-lane lane) — an UNARMED PLACEHOLDER: not approved
    /// by the user, and validation refuses every armed X3 value whatever its policy. Derivation (every term an existing window of the
    /// bundle the claim is accepted under, read at the activation height):
    ///
    /// ```text
    /// T_prepare = window_bind + window_receipt            acceptance → receipt deadline (DL-1's PanelBound row: bind, then receipt)
    /// T_observe = window_challenge_at(activation)         receipt deadline → open cutoff (the challenge window a licence opens)
    /// T_resolve = B_disclose + B_court                    open cutoff → hard: the two B_remaining terms with an existing source
    ///           = window_challenge + window_court         (palw_da_disclose_window_daa_v1 = the UNSHORTENED window_challenge)
    /// hard − d0 = bind + receipt + window_challenge_at(activation) + window_challenge + court
    ///           = 600 + 600 + 120 + 1,200 + 3,000 = 5,520 DAA on testnet-12 at DAA 14,000 (short window in force)
    /// ```
    ///
    /// Properties: (i) a pursuit started at the open cutoff finishes exactly at `hard` while the five unmeasured `B_*` terms are 0
    /// (REQ-X3-4 holds with equality; each measured term must be ADDED to `T_resolve`); (ii) `hard` exceeds the producer's existing
    /// retention (`bind + receipt + window_challenge + court`, 5,400 on testnet-12) by `window_challenge_at` (120), the extension
    /// REQ-X3-6 prices at `c_retain`; (iii) for
    /// any licence `L <= d0 + bind + receipt`, `hard >= L + challenge + court` — the legacy FP liability floor of
    /// `palw_reward_liability_horizon_v1` (G-08) — so X3 never Finals a claim before its liability horizon. UNMEASURED and left 0
    /// ([`palw_x3_unmeasured_terms_v1`]): `B_cold_fetch`, `B_check`, `B_localize`, `B_carrier`, `B_reorg_slack` and `c_retain`.
    pub fn candidate_from_horizons_v1(activation: ForkActivation, params: &PalwStateParamsV2) -> Self {
        let challenge = params.window_challenge_at(activation.daa_score());
        Self {
            activation,
            policy: PalwClaimHardDeadlinePolicyV1 {
                version: PALW_CLAIM_HARD_DEADLINE_POLICY_VERSION_V1,
                prepare_daa: params.window_bind().saturating_add(params.window_receipt()),
                observe_daa: challenge,
                resolve_daa: palw_da_disclose_window_daa_v1(params).saturating_add(params.window_court()),
                cold_fetch_daa: 0,
                check_daa: 0,
                localize_daa: 0,
                carrier_daa: 0,
                reorg_slack_daa: 0,
                retention_cost_per_daa_sompi: 0,
            },
        }
    }
}

/// **The X3 policy composed from the network's windows** (user decision 2026-10-18): `T_prepare = window_bind + window_receipt`,
/// `T_observe = window_challenge_at(activation)`, `T_resolve = B_disclose + B_court + Δ` with
/// `Δ = B_cold_fetch + B_check + B_localize + B_carrier + B_reorg_slack`. `base` supplies the five `B_*` terms and `c_retain`; its three
/// spans are overwritten by the composition (a caller can never choose them apart from the windows).
pub fn palw_x3_composed_policy_v1(
    params: &PalwStateParamsV2,
    activation: ForkActivation,
    base: PalwClaimHardDeadlinePolicyV1,
) -> PalwClaimHardDeadlinePolicyV1 {
    let delta = [base.cold_fetch_daa, base.check_daa, base.localize_daa, base.carrier_daa, base.reorg_slack_daa]
        .into_iter()
        .fold(0u64, u64::saturating_add);
    PalwClaimHardDeadlinePolicyV1 {
        prepare_daa: params.window_bind().saturating_add(params.window_receipt()),
        observe_daa: params.window_challenge_at(activation.daa_score()),
        resolve_daa: palw_da_disclose_window_daa_v1(params).saturating_add(params.window_court()).saturating_add(delta),
        ..base
    }
}

/// **Refuses an X3 policy whose spans are not the composition of the windows read from `params` at `activation`** (asked by
/// validation, so a mismatch between the policy and the network's windows is refused rather than silently used). `None` when they agree.
pub fn palw_x3_composition_refusal_v1(
    params: &PalwStateParamsV2,
    activation: ForkActivation,
    policy: &PalwClaimHardDeadlinePolicyV1,
) -> Option<&'static str> {
    let composed = palw_x3_composed_policy_v1(params, activation, *policy);
    if (policy.prepare_daa, policy.observe_daa, policy.resolve_daa) != (composed.prepare_daa, composed.observe_daa, composed.resolve_daa) {
        return Some(
            "palw_claim_hard_deadline_v1: T_prepare, T_observe and T_resolve must be the composition of the windows read at activation (REQ-X3-1)",
        );
    }
    None
}

/// **The X3 terms a policy leaves UNMEASURED** (0 is UNKNOWN, never free): the five `B_*` terms with no existing source and `c_retain`.
/// The candidate ([`PalwClaimHardDeadlineFenceV1::candidate_from_horizons_v1`]) leaves all six; a policy the user approves names none.
pub fn palw_x3_unmeasured_terms_v1(policy: &PalwClaimHardDeadlinePolicyV1) -> Vec<&'static str> {
    [
        ("B_cold_fetch", policy.cold_fetch_daa),
        ("B_check", policy.check_daa),
        ("B_localize", policy.localize_daa),
        ("B_carrier", policy.carrier_daa),
        ("B_reorg_slack", policy.reorg_slack_daa),
        ("c_retain", policy.retention_cost_per_daa_sompi),
    ]
    .into_iter()
    .filter(|(_, v)| *v == 0)
    .map(|(n, _)| n)
    .collect()
}

/// **The hard deadline as a function of acceptance alone** (REQ-X3-1): `d0 + T_prepare + T_observe + T_resolve`, saturating. Pure, so
/// a pricing caller can size the retention extension before the claim exists.
pub fn palw_x3_hard_deadline_at_v1(policy: &PalwClaimHardDeadlinePolicyV1, accepted_daa: u64) -> u64 {
    accepted_daa.saturating_add(policy.prepare_daa).saturating_add(policy.observe_daa).saturating_add(policy.resolve_daa)
}

/// **REQ-X3-6's extension, in numbers**: the obligation a claim was accepted with (`base_retention_daa`), the obligation X3 makes it
/// owe (`retention_daa = max(base, hard)`), and the DAA between them that the material term prices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwX3RetentionExtensionV1 {
    pub base_retention_daa: u64,
    pub retention_daa: u64,
    pub extra_daa: u64,
}

/// [`PalwX3RetentionExtensionV1`] of a claim accepted at `accepted_daa` with retention `base_retention_daa` under `policy`.
pub fn palw_x3_retention_extension_at_v1(
    policy: &PalwClaimHardDeadlinePolicyV1,
    accepted_daa: u64,
    base_retention_daa: u64,
) -> PalwX3RetentionExtensionV1 {
    let retention_daa = base_retention_daa.max(palw_x3_hard_deadline_at_v1(policy, accepted_daa));
    PalwX3RetentionExtensionV1 { base_retention_daa, retention_daa, extra_daa: retention_daa - base_retention_daa }
}

/// **The retention cost term** `extra_daa × c_retain` (sompi): what a provider/material hold costs for REQ-X3-6's extension. `Err`
/// when the extension is positive and `c_retain` is unmeasured (`0`), or the product overflows — never a silent zero.
pub fn palw_x3_retention_cost_sompi_v1(policy: &PalwClaimHardDeadlinePolicyV1, extra_daa: u64) -> Result<u64, &'static str> {
    if extra_daa == 0 {
        return Ok(0);
    }
    if policy.retention_cost_per_daa_sompi == 0 {
        return Err("X3: the retention cost per DAA is unmeasured; a positive retention extension cannot be priced");
    }
    extra_daa.checked_mul(policy.retention_cost_per_daa_sompi).ok_or("X3: the retention cost overflows")
}

/// **A material term with X3's retention priced in**: `material_sompi + extra_daa × c_retain` — the input a minimum-price `C_upper`
/// (`palw_econ_contracts_v1::palw_minimum_price_c_upper_v1`'s class cost) and a beacon source's `material_sompi`
/// (`PalwBeaconSourceCostBoundsV1`) must carry for an X3 claim. `extension` is `None` under the old ruleset (the term unchanged).
pub fn palw_x3_material_term_with_retention_v1(
    material_sompi: u64,
    policy: &PalwClaimHardDeadlinePolicyV1,
    extension: Option<&PalwX3RetentionExtensionV1>,
) -> Result<u64, &'static str> {
    let Some(extension) = extension else { return Ok(material_sompi) };
    material_sompi.checked_add(palw_x3_retention_cost_sompi_v1(policy, extension.extra_daa)?).ok_or("X3: the material term overflows")
}

/// **The X3 mirror** the fold reads (`PalwStateParamsV2::claim_hard_deadline_v1`), written only by
/// [`Params::sync_palw_2026_10_12_ruling_fences_v1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwClaimHardDeadlineMirrorV1 {
    pub from_daa: u64,
    pub policy: PalwClaimHardDeadlinePolicyV1,
}

/// **One X3 claim's fixed deadlines** — a pure function of the claim's acceptance DAA and the fence's policy (REQ-X3-1). The claim's
/// retention obligation follows the hard deadline, never the reverse (REQ-X3-6), and nothing extends either after acceptance
/// (REQ-X3-3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwClaimDeadlinesV1 {
    pub accepted_daa: u64,
    pub receipt_deadline_daa: u64,
    pub open_cutoff_daa: u64,
    pub hard_deadline_daa: u64,
}

/// **REQ-X3-1: a claim's deadlines, fixed at acceptance (d0)** — `None` for a claim accepted below the fence (or where no network
/// arms it: every shipped one), which keeps every old rule.
///
/// **Lead default (2026-10-15, round 3): retention EXTENDS to the hard deadline** (REQ-X3-6). The first source capped `hard` at the
/// claim's retention (`bind + receipt + challenge + court`), so under most policies a pursuit could barely start after licensing. Now
/// `hard` is the policy's alone, and the claim's material obligation is raised to it once, at creation
/// ([`palw_claim_x3_retention_daa_v1`], written by the fold's one claim writer): every pursuit X3 admits runs on material the producer
/// still owes, and prosecution may start from public material any time before `hard − B_remaining`.
pub fn palw_claim_deadlines_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> Option<PalwClaimDeadlinesV1> {
    let mirror = params.claim_hard_deadline_v1()?;
    if claim.accepted_daa < mirror.from_daa {
        return None;
    }
    let p = mirror.policy;
    let receipt_deadline_daa = claim.accepted_daa.saturating_add(p.prepare_daa);
    let open_cutoff_daa = receipt_deadline_daa.saturating_add(p.observe_daa);
    let hard_deadline_daa = palw_x3_hard_deadline_at_v1(&p, claim.accepted_daa);
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

/// **REQ-X3-3: the DA pause an X3 claim gives back.** The ADR-0062 pause return (`resume_claim_after_da_session_v2`) and the V6 verify
/// credit move a claim's own anchors forward by the session's length; for an X3 claim that is a deadline extension, so the credit is
/// ZERO. Under the old ruleset the pause passes through unchanged (every shipped ruleset, so the identity).
pub fn palw_x3_pause_credit_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2, paused_daa: u64) -> u64 {
    if palw_claim_x3_governs_v1(params, claim) { 0 } else { paused_daa }
}

/// **Does X3 govern this claim?** (Accepted at/after the fence.) Every REQ-X3-3 guard asks this one question.
pub fn palw_claim_x3_governs_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> bool {
    palw_claim_deadlines_v1(params, claim).is_some()
}

/// **REQ-X3-6: an X3 claim's retention obligation** — `max(trace_retention_daa, hard)`; `None` under the old ruleset. The fold's claim
/// writer applies it once, when it CREATES the record (`TransitionBuilder::write_claim`), so the claim trace, the DA material units
/// (`palw_da_rcore_v1`'s availability, the DA and court retention checks) and every material hold that reads the record are owed
/// through the hard deadline. Applied at creation it is part of the deadline fixed at d0, not an extension (REQ-X3-3).
pub fn palw_claim_x3_retention_daa_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> Option<u64> {
    palw_claim_deadlines_v1(params, claim).map(|d| claim.trace_retention_daa.max(d.hard_deadline_daa))
}

/// **The retention a claim's material holds must cover**: X3's obligation (REQ-X3-6), else the claim's own — the one spelling for a
/// caller that reads a claim BEFORE the writer raised it (a contract planned beside the claim's creation).
pub fn palw_claim_retention_obligation_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> u64 {
    palw_claim_x3_retention_daa_v1(params, claim).unwrap_or(claim.trace_retention_daa)
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
    // G14 r5 (item 2): where `palw_court_clock_v2` governs the claim, clock v2 owns the cutoff (a + 3,120) and the hard deadline
    // (a + 10,000); X3's `B_remaining` is kept as the budget (`crate::palw_court_clock_v2::palw_claim_pursuit_may_start_v2`).
    if crate::palw_court_clock_v2::palw_court_clock_v2_governs(params, claim) {
        return crate::palw_court_clock_v2::palw_claim_pursuit_may_start_v2(params, claim, start);
    }
    let Some(deadlines) = palw_claim_deadlines_v1(params, claim) else { return Ok(()) };
    let policy = params.claim_hard_deadline_v1().expect("deadlines exist only under the mirror").policy;
    if start > deadlines.open_cutoff_daa {
        return Err("X3: the claim's open cutoff has passed; no new pursuit starts (palw_claim_hard_deadline_v1)");
    }
    if start.saturating_add(palw_claim_pursuit_budget_v1(params, &policy)) > deadlines.hard_deadline_daa {
        return Err(
            "X3: start + B_remaining exceeds the claim's hard deadline; the pursuit could not finish (palw_claim_hard_deadline_v1)",
        );
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

/// **Can a generative (tensor, tag 87) job's required evidence be public?** The same two non-public routes as a free-prompt job's:
/// `PanelDa` (the ids ride only the capture served to the Panel) and image inputs (they travel only to the worker and the Panel, as
/// FP Job V5's do).
pub fn palw_gen_job_evidence_public_v1(job: &crate::palw_gen_job_v1::PalwGenAcceptedJobV1) -> Result<(), &'static str> {
    if !job.public_da {
        return Err("PanelDa: the tensor job's ids ride only the capture served to the Panel, never the chain");
    }
    if !job.images.is_empty() {
        return Err("image inputs: the tensor job's images travel only to the worker and the Panel");
    }
    Ok(())
}

/// **Does a claim record carry any reward or Final weight?** Quanta, pwu, escrowed reward, receipt rights or an immature weight
/// contribution — what N4 forbids a non-public route to TAKE. A tensor claim (tag 87) and an evaluation claim carry none.
pub fn palw_claim_earns_reward_or_weight_v1(claim: &PalwClaimStateV2) -> bool {
    let quanta = match &claim.source {
        crate::palw_state_v2::PalwClaimSourceV2::FreePrompt { quanta, .. } => *quanta > 0,
        _ => false,
    };
    quanta || claim.pwu > 0 || claim.escrowed_reward > 0 || claim.rights_reserved > 0 || claim.immature_contribution > 0
}

/// **REQ-N4-3: N4 on a weightless tensor claim (tag 87) — the grant guard.** A tensor claim is created weightless (RFC-0003 §I.4.6:
/// weight for tensor claims is a later fence), so N4 refuses nothing today. The tensor fold asks THIS at the one site that writes the
/// claim (`palw_gen_claim_fold_v1`), together with [`palw_claim_earns_reward_or_weight_v1`] of the record it writes: the day a fence
/// grants a tensor claim reward or Final weight there, a non-public-evidence job accepted past N4 is refused exactly as REQ-N4-1
/// refuses a free-prompt one. `None` below the fence (REQ-N4-2) or for a job whose evidence can be public.
pub fn palw_public_evidence_tensor_refusal_v1(
    params: &PalwStateParamsV2,
    accepted_daa: u64,
    job: &crate::palw_gen_job_v1::PalwGenAcceptedJobV1,
) -> Option<&'static str> {
    if !params.public_evidence_reward_active_at(accepted_daa) {
        return None;
    }
    palw_gen_job_evidence_public_v1(job).err()
}

// =================================================================================================================================
// D2 — `palw_court_model_scope_v1`
// =================================================================================================================================

/// **The four ARMED t12 routes that compel model bytes** (`palw_court_scope_v1`'s inventory test pins exactly these). Each has a
/// replacement route that convicts the same lie from public authenticated material
/// ([`crate::palw_court_model_scope_routes_v1::PALW_D2_REPLACEMENT_ROUTES_V1`], REQ-T6-4): the held `StepLeaf` by the claim part and
/// the accuser's rows; the two root-claim units by a root claim that signs coordinates and claim values only and relays operands
/// the challenger published at opening (the named-leaf opening checks them, `open_dissection_at_named_leaf_v1`); readiness by a
/// keyed digest.
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
