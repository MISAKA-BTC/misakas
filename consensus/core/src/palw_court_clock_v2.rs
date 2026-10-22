//! **`palw_court_clock_v2` (G14 lane court-clock, survey U06/U07; DORMANT).**
//!
//! RFC-0026 §2–3 and RFC-0014's 2026-10-19 amendment §B/§D: every court clock of a claim is derived from its ORIGINAL acceptance
//! DAA `a` ([`crate::palw_court_policy_v1::CourtClockBounds`]), not from `trace_retention_daa − W_disclose`:
//!
//! | clock | undisputed claim | disputed claim (one extension) |
//! |---|---|---|
//! | initial notice (first reservation) | `a <= d <= a + 3,120` (ADR-0198 A, inclusive) | same; a later message never resets it |
//! | hard deadline / Final hold | `a + 3,120` | `a + 10,000` |
//! | `collect_until` (exclusive) | `hard + 1` | `hard + 1` |
//! | disclosure duty / retention | `hard + W_disclose` | `hard + W_disclose` |
//! | bond release | `max(material, dependency)` from that hard | same |
//!
//! **The extension (U07)** is the legacy dispute record ([`PalwDisputeClaimV1`]) itself: for a governed claim the first reservation
//! writes `hard_deadline_daa = a + 10,000`, and every other clock above is a pure function of `(a, record present)`. So collect,
//! disclosure, retention, the Final hold and the bond release move together in that ONE rooted write — and revert with it on reorg.
//! Per-turn deadlines are untouched (they are read from the turn's own start, never from here).
//!
//! **T15 rule:** a claim accepted below the fence keeps the old clock (`trace_retention_daa − W_disclose`) for its whole life, even
//! when its dispute is filed after the fence.
//!
//! Not a proof system: plain arithmetic over consensus DAA.

use crate::palw_court_policy_v1::{CourtClockBounds, CourtPolicyError, DISPUTE_WINDOW_DAA, NOTICE_WINDOW_DAA};
use crate::palw_legacy_public_filer_v1::{palw_dispute_hard_deadline_v1, PalwDisputeClaimV1};
use crate::palw_state_v2::{PalwClaimStateV2, PalwStateParamsV2};

/// The legacy route carries no separate reorg-retention term; its material reads use `W_disclose` (the follow-up term).
pub const PALW_COURT_CLOCK_V2_REORG_RETENTION_DAA: u64 = 0;

/// Every court clock of one claim, derived at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwCourtClocksV2 {
    /// Last DAA (inclusive) a first reservation / initial notice may land: `a + 3,120`.
    pub notice_until: u64,
    /// Exclusive: a collected item is accepted while `daa < collect_until`.
    pub collect_until: u64,
    /// The dispute hard deadline (inclusive) and the Final hold.
    pub hard_deadline: u64,
    /// The producer's disclosure duty and the material retention.
    pub retention_until: u64,
    /// Earliest DAA the claim's bond may be released for this claim.
    pub bond_release_after: u64,
    /// Did a dispute extend the claim?
    pub extended: bool,
}

/// **Does `palw_court_clock_v2` govern `claim`?** Only a claim whose ORIGINAL acceptance is at/after the fence (T15).
pub fn palw_court_clock_v2_governs(params: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> bool {
    params.court_clock_v2_from_daa().is_some_and(|from| claim.accepted_daa >= from)
}

/// **The obligation a governed claim owes from acceptance** (RFC-0026 §3.3, F07): the maximal extension, `a + 10,000 + W_disclose`.
/// The fold's one claim writer raises `trace_retention_daa` to at least this at creation (as X3's REQ-X3-6 does), so an extension
/// never needs the producer's fresh consent and never outruns the signed horizon.
pub fn palw_court_clock_v2_required_retention(accepted_daa: u64, w_disclose: u64) -> Result<u64, CourtPolicyError> {
    accepted_daa
        .checked_add(DISPUTE_WINDOW_DAA)
        .and_then(|h| h.checked_add(w_disclose.max(PALW_COURT_CLOCK_V2_REORG_RETENTION_DAA)))
        .ok_or(CourtPolicyError::Overflow)
}

/// **Derive all clocks** from `accepted_daa`. `signed_retention` is the claim's signed horizon (`trace_retention_daa`); it must cover
/// the MAXIMAL extension even for an undisputed claim (a dispute may still come), else `DependencyTooShort`.
pub fn palw_court_clocks_v2(
    accepted_daa: u64,
    extended: bool,
    w_disclose: u64,
    signed_retention: u64,
    other_bond_liability: u64,
) -> Result<PalwCourtClocksV2, CourtPolicyError> {
    let bounds = CourtClockBounds::derive(
        accepted_daa,
        PALW_COURT_CLOCK_V2_REORG_RETENTION_DAA,
        w_disclose,
        signed_retention,
        other_bond_liability,
    )?;
    let hard = if extended { bounds.hard_deadline } else { bounds.notice_boundary };
    let add = |x: u64, y: u64| x.checked_add(y).ok_or(CourtPolicyError::Overflow);
    let material = add(hard, PALW_COURT_CLOCK_V2_REORG_RETENTION_DAA)?;
    let retention_until = add(hard, w_disclose)?.max(material);
    Ok(PalwCourtClocksV2 {
        notice_until: bounds.notice_boundary,
        collect_until: add(hard, 1)?,
        hard_deadline: hard,
        retention_until,
        bond_release_after: retention_until.max(other_bond_liability),
        extended,
    })
}

/// The hard deadline an undisputed governed claim finishes at (`a + 3,120`); saturating like the legacy formula it replaces at the
/// read sites (a DAA near `u64::MAX` never occurs on a live chain; [`palw_court_clocks_v2`] is the checked form).
pub fn palw_court_clock_v2_normal_hard(accepted_daa: u64) -> u64 {
    accepted_daa.saturating_add(NOTICE_WINDOW_DAA)
}

/// The hard deadline a dispute extends a governed claim to (`a + 10,000`, from the ORIGINAL acceptance; never from the filing).
pub fn palw_court_clock_v2_extended_hard(accepted_daa: u64) -> u64 {
    accepted_daa.saturating_add(DISPUTE_WINDOW_DAA)
}

/// **The claim's court hard deadline, as every fold site reads it.**
///
/// * Not governed (below the fence, every shipped ruleset): the old formula `trace_retention_daa − W_disclose`, byte for byte.
/// * Governed, disputed (`record` present): the record's fixed `hard_deadline_daa` (`a + 10,000`, written once by the extension).
/// * Governed, undisputed: `a + 3,120`.
pub fn palw_claim_court_hard_deadline_v2(
    params: &PalwStateParamsV2,
    record: Option<&PalwDisputeClaimV1>,
    claim: &PalwClaimStateV2,
    w_disclose: u64,
) -> u64 {
    if !palw_court_clock_v2_governs(params, claim) {
        return palw_dispute_hard_deadline_v1(claim, w_disclose);
    }
    match record {
        Some(record) => record.hard_deadline_daa,
        None => palw_court_clock_v2_normal_hard(claim.accepted_daa),
    }
}

/// **The worst-case hard deadline a claim must be provisioned for at acceptance** (RFC-0026 §3.1: capacity is reserved for the
/// maximal dispute, never the normal window). The old formula below the fence.
pub fn palw_claim_court_reserve_hard_v2(params: &PalwStateParamsV2, claim: &PalwClaimStateV2, w_disclose: u64) -> u64 {
    if palw_court_clock_v2_governs(params, claim) {
        palw_court_clock_v2_extended_hard(claim.accepted_daa)
    } else {
        palw_dispute_hard_deadline_v1(claim, w_disclose)
    }
}

/// **The dispute record's hard deadline at its creation** (the extension, U07). For a governed claim the first reservation must land
/// inside the initial-notice window `a <= now <= a + 3,120` (ADR-0198 A); it then writes `a + 10,000`. A record re-created after an
/// earlier one was deleted still measures from `a` (never reset). Not governed: the old formula.
pub fn palw_dispute_record_hard_deadline_v2(
    params: &PalwStateParamsV2,
    claim: &PalwClaimStateV2,
    w_disclose: u64,
    reorg_retention: u64,
    dependencies: &[PalwCourtDependencyV2],
    now: u64,
) -> Result<u64, &'static str> {
    if !palw_court_clock_v2_governs(params, claim) {
        return Ok(palw_dispute_hard_deadline_v1(claim, w_disclose));
    }
    match crate::palw_court_policy_v1::shared_initial_notice_daa_in_window_v1(claim.accepted_daa, now) {
        Ok(true) => {}
        Ok(false) => {
            return Err("palw_court_clock_v2: the first dispute must land within a <= d <= a + 3,120 of the original acceptance");
        }
        Err(_) => return Err("palw_court_clock_v2: the claim's clocks overflow"),
    }
    // G14 r5 item 3 (RFC-0026 §3.3 F07): EVERY dependency — the claim's own trace (the EXEC worker's material), and each EXEC /
    // provider / custody contract the caller hands over — must cover `a + 10,000 + max(W_disclose, reorg_retention)`. One short
    // dependency refuses the extension, i.e. the whole reservation write (atomic: the caller has written nothing yet).
    let own = PalwCourtDependencyV2 { kind: PalwCourtDependencyKindV2::ClaimTrace, signed_horizon_daa: claim.trace_retention_daa };
    palw_court_clock_v2_dependencies_cover_v2(
        claim.accepted_daa,
        w_disclose,
        reorg_retention,
        std::iter::once(&own).chain(dependencies.iter()),
    )
    .map_err(|e| match e {
        PalwCourtDependencyRefusalV2::Overflow => "palw_court_clock_v2: the claim's clocks overflow",
        PalwCourtDependencyRefusalV2::TooShort(PalwCourtDependencyKindV2::ClaimTrace) => {
            "palw_court_clock_v2: the claim's signed retention does not cover a + 10,000 + max(W_disclose, reorg) (F07)"
        }
        PalwCourtDependencyRefusalV2::TooShort(PalwCourtDependencyKindV2::Exec) => {
            "palw_court_clock_v2: an EXEC dependency's signed horizon does not cover the extended deadline (F07)"
        }
        PalwCourtDependencyRefusalV2::TooShort(PalwCourtDependencyKindV2::Provider) => {
            "palw_court_clock_v2: a provider dependency's signed horizon does not cover the extended deadline (F07)"
        }
        PalwCourtDependencyRefusalV2::TooShort(PalwCourtDependencyKindV2::Custody) => {
            "palw_court_clock_v2: a custody / route dependency's signed horizon does not cover the extended deadline (F07)"
        }
    })?;
    palw_court_clocks_v2_with(claim.accepted_daa, true, w_disclose, reorg_retention, claim.trace_retention_daa, 0)
        .map(|c| c.hard_deadline)
        .map_err(|_| "palw_court_clock_v2: the claim's clocks overflow or its signed retention is short (F07)")
}

// =================================================================================================================================
// G14 r5 (item 3): dependency contracts and the reorg-retention term
// =================================================================================================================================

/// The kinds of signed dependency an extension must not outrun (RFC-0026 §3.3 F07).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwCourtDependencyKindV2 {
    /// The claim's own `trace_retention_daa` (the producer / EXEC worker's material for this claim).
    ClaimTrace,
    /// An EXEC root / worker contract the claim depends on.
    Exec,
    /// A material provider lease.
    Provider,
    /// A custody / route contract (staged route `material_until_daa`, TIR frame custody).
    Custody,
}

/// One signed dependency horizon (inclusive: material is owed through this DAA).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwCourtDependencyV2 {
    pub kind: PalwCourtDependencyKindV2,
    pub signed_horizon_daa: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwCourtDependencyRefusalV2 {
    Overflow,
    TooShort(PalwCourtDependencyKindV2),
}

/// **The reorg-retention term from params** (r5: no longer 0 on governed claims): the staged public court's
/// `reorg_retention_daa` when that config is in force, else 0 (the legacy route carries none).
pub fn palw_court_clock_v2_reorg_retention(
    staged: Option<&crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1>,
) -> u64 {
    staged.map_or(PALW_COURT_CLOCK_V2_REORG_RETENTION_DAA, |c| c.reorg_retention_daa)
}

/// `a + 10,000 + max(W_disclose, reorg_retention)`: what every dependency of a governed claim must cover.
pub fn palw_court_clock_v2_required_retention_with(
    accepted_daa: u64,
    w_disclose: u64,
    reorg_retention: u64,
) -> Result<u64, CourtPolicyError> {
    accepted_daa
        .checked_add(DISPUTE_WINDOW_DAA)
        .and_then(|h| h.checked_add(w_disclose.max(reorg_retention)))
        .ok_or(CourtPolicyError::Overflow)
}

/// **G14 r6: a provider quorum's horizon** — given each distinct serving operator's longest live lease end, the last DAA through
/// which at least `quorum` of them still serve (the `quorum`-th largest). Fewer operators than the quorum: `0` (covers nothing).
pub fn palw_provider_quorum_horizon_of_v2(per_operator_until: impl IntoIterator<Item = u64>, quorum: usize) -> u64 {
    let mut ends: Vec<u64> = per_operator_until.into_iter().collect();
    ends.sort_unstable_by(|a, b| b.cmp(a));
    match quorum {
        0 => u64::MAX,
        q => ends.get(q - 1).copied().unwrap_or(0),
    }
}

/// **Do ALL dependencies cover the maximal extension?** `Err(TooShort(kind))` names the first short one (boundary: `horizon ==
/// need` covers; `need − 1` does not).
pub fn palw_court_clock_v2_dependencies_cover_v2<'a>(
    accepted_daa: u64,
    w_disclose: u64,
    reorg_retention: u64,
    dependencies: impl IntoIterator<Item = &'a PalwCourtDependencyV2>,
) -> Result<u64, PalwCourtDependencyRefusalV2> {
    let need = palw_court_clock_v2_required_retention_with(accepted_daa, w_disclose, reorg_retention)
        .map_err(|_| PalwCourtDependencyRefusalV2::Overflow)?;
    for dep in dependencies {
        if dep.signed_horizon_daa < need {
            return Err(PalwCourtDependencyRefusalV2::TooShort(dep.kind));
        }
    }
    Ok(need)
}

/// [`palw_court_clocks_v2`] with an explicit reorg-retention term.
pub fn palw_court_clocks_v2_with(
    accepted_daa: u64,
    extended: bool,
    w_disclose: u64,
    reorg_retention: u64,
    signed_retention: u64,
    other_bond_liability: u64,
) -> Result<PalwCourtClocksV2, CourtPolicyError> {
    let bounds = CourtClockBounds::derive(accepted_daa, reorg_retention, w_disclose, signed_retention, other_bond_liability)?;
    let hard = if extended { bounds.hard_deadline } else { bounds.notice_boundary };
    let add = |x: u64, y: u64| x.checked_add(y).ok_or(CourtPolicyError::Overflow);
    let material = add(hard, reorg_retention)?;
    let retention_until = add(hard, w_disclose)?.max(material);
    Ok(PalwCourtClocksV2 {
        notice_until: bounds.notice_boundary,
        collect_until: add(hard, 1)?,
        hard_deadline: hard,
        retention_until,
        bond_release_after: retention_until.max(other_bond_liability),
        extended,
    })
}

// =================================================================================================================================
// G14 r5 (item 1): every other court route's collection door
// =================================================================================================================================

/// **A route's collection door (exclusive) under `palw_court_clock_v2`.** TIR public frames, staged prefix / tokenizer / constraint
/// / Float32 / history / selected-source owner bundles and the shared court carry their own signed `collect_until_daa`. For a claim
/// accepted at/after the fence the door is the claim's v2 door `hard_v2 + 1` (`a + 3,121` undisputed, `a + 10,001` once the dispute
/// record exists), never SHORTER than the route's own signed door (a signed door is an obligation the route already priced). Not
/// governed (or no claim row): the signed door, byte for byte. A route whose material does not cover the extension never sees
/// one: the extension itself is refused at the dispute record ([`palw_dispute_record_hard_deadline_v2`] takes the route as a
/// `Custody` dependency).
pub fn palw_route_collect_until_v2(
    params: &PalwStateParamsV2,
    record: Option<&PalwDisputeClaimV1>,
    claim: Option<&PalwClaimStateV2>,
    w_disclose: u64,
    signed_collect_until: u64,
) -> u64 {
    match claim {
        Some(claim) if palw_court_clock_v2_governs(params, claim) => {
            palw_claim_court_hard_deadline_v2(params, record, claim, w_disclose).saturating_add(1).max(signed_collect_until)
        }
        _ => signed_collect_until,
    }
}

/// Is `now` inside the route's original door `[accepted, collect_until_v2)`?
pub fn palw_route_door_open_v2(
    params: &PalwStateParamsV2,
    record: Option<&PalwDisputeClaimV1>,
    claim: Option<&PalwClaimStateV2>,
    w_disclose: u64,
    signed_accepted: u64,
    signed_collect_until: u64,
    now: u64,
) -> bool {
    now >= signed_accepted && now < palw_route_collect_until_v2(params, record, claim, w_disclose, signed_collect_until)
}

// =================================================================================================================================
// G14 r5 (item 2): X3 × clock v2
// =================================================================================================================================

/// **The combined start rule when `palw_court_clock_v2` governs the claim** (read by X3's
/// [`crate::palw_ruling_fences_v1::palw_claim_pursuit_may_start_v1`] before its own rule). Clock v2 owns the deadlines: the
/// open cutoff is the initial-notice window `a <= start <= a + 3,120` (ADR-0198 A) and the hard deadline the pursuit must finish in
/// is the extended `a + 10,000` (a reservation IS the extension). If X3 is ALSO armed for the claim, its `B_remaining` is kept as
/// the budget: `start + B_remaining <= a + 10,000`; X3's own (policy) hard and cutoff are not read for this claim. Without X3 the
/// budget term is 0. Overflow refuses.
pub fn palw_claim_pursuit_may_start_v2(
    params: &PalwStateParamsV2,
    claim: &PalwClaimStateV2,
    start: u64,
) -> Result<(), &'static str> {
    match crate::palw_court_policy_v1::shared_initial_notice_daa_in_window_v1(claim.accepted_daa, start) {
        Ok(true) => {}
        Ok(false) => return Err("clock v2: a pursuit starts only within a <= start <= a + 3,120 (palw_court_clock_v2)"),
        Err(_) => return Err("clock v2: the claim's clocks overflow"),
    }
    let budget = palw_claim_pursuit_budget_v2(params, claim);
    let hard = claim.accepted_daa.checked_add(DISPUTE_WINDOW_DAA).ok_or("clock v2: the claim's clocks overflow")?;
    match start.checked_add(budget) {
        Some(end) if end <= hard => Ok(()),
        _ => Err("clock v2 × X3: start + B_remaining exceeds a + 10,000; the pursuit could not finish (palw_court_clock_v2)"),
    }
}

/// X3's `B_remaining` when X3 is armed for this claim (accepted at/after X3's fence), else 0.
pub fn palw_claim_pursuit_budget_v2(params: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> u64 {
    match params.claim_hard_deadline_v1() {
        Some(mirror) if claim.accepted_daa >= mirror.from_daa => {
            crate::palw_ruling_fences_v1::palw_claim_pursuit_budget_v1(params, &mirror.policy)
        }
        _ => 0,
    }
}

/// The hard deadline a route's install must fit under when both clocks may apply: clock v2's extended `a + 10,000` when it governs
/// (it takes precedence), else X3's fixed hard (if armed), else `None`.
pub fn palw_claim_route_hard_v2(params: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> Option<u64> {
    if palw_court_clock_v2_governs(params, claim) {
        return Some(palw_court_clock_v2_extended_hard(claim.accepted_daa));
    }
    crate::palw_ruling_fences_v1::palw_claim_hard_deadline_of_v1(params, claim)
}

/// **G14 r6 (item 3): X3's cap, under the X3 × clock-v2 combined rule** — the hard deadline every site that CAPS a claim by X3's
/// `dispute_hard_deadline` reads. `None` where X3 does not govern the claim (no cap, the old rule — unchanged whether or not clock v2
/// governs). Where X3 governs: clock v2's finish line `a + 10,000` when clock v2 also governs (it takes precedence, as the pursuit start
/// rule [`palw_claim_pursuit_may_start_v2`] already reads it — X3's own policy hard is not read for a v2-governed claim), else X3's
/// policy hard. Fixed at acceptance either way.
pub fn palw_claim_x3_cap_hard_v2(params: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> Option<u64> {
    let x3 = crate::palw_ruling_fences_v1::palw_claim_hard_deadline_of_v1(params, claim)?;
    if palw_court_clock_v2_governs(params, claim) { Some(palw_court_clock_v2_extended_hard(claim.accepted_daa)) } else { Some(x3) }
}

// =================================================================================================================================
// G14 r5 (item 6): T06 — the 6,880 residual budget, mechanised
// =================================================================================================================================

/// The residual after the latest legal initial notice: `10,000 − 3,120 = 6,880`.
pub const PALW_COURT_CLOCK_V2_RESIDUAL_DAA: u64 = DISPUTE_WINDOW_DAA - NOTICE_WINDOW_DAA;

/// Everything that must still fit after a notice filed at exactly `a + 3,120` (RFC-0026 §2 T06, RFC-0014 2026-10-19).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwCourtResidualBudgetV2 {
    /// Turns the court may still need (each answered at its LATEST legal DAA).
    pub turns: u64,
    /// The longest legal response time per turn.
    pub turn_daa: u64,
    /// Final-evidence delivery.
    pub final_evidence_daa: u64,
    /// Carrier inclusion / recording of the final object.
    pub recording_daa: u64,
    /// Recovery (re-fetch after a lost fragment).
    pub recovery_daa: u64,
    /// The reorg-retention term.
    pub reorg_retention_daa: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwCourtResidualRefusalV2 {
    Overflow,
    /// `need` DAA (with the 1-DAA exclusive margin) exceed the 6,880 residual.
    Exceeds { need: u64 },
}

/// **T06**: `turns·turn + evidence + recording + recovery + reorg + 1 <= 6,880`. The `+ 1` is the exclusive `collect_until` margin
/// (the last item must land at `hard`, and `collect_until = hard + 1`), so the boundary is exact: a sum of `6,879` passes, `6,880`
/// is refused. Returns the slack.
pub fn palw_court_clock_v2_t06_v2(b: &PalwCourtResidualBudgetV2) -> Result<u64, PalwCourtResidualRefusalV2> {
    let need = b
        .turns
        .checked_mul(b.turn_daa)
        .and_then(|x| x.checked_add(b.final_evidence_daa))
        .and_then(|x| x.checked_add(b.recording_daa))
        .and_then(|x| x.checked_add(b.recovery_daa))
        .and_then(|x| x.checked_add(b.reorg_retention_daa))
        .and_then(|x| x.checked_add(1))
        .ok_or(PalwCourtResidualRefusalV2::Overflow)?;
    if need > PALW_COURT_CLOCK_V2_RESIDUAL_DAA {
        return Err(PalwCourtResidualRefusalV2::Exceeds { need });
    }
    Ok(PALW_COURT_CLOCK_V2_RESIDUAL_DAA - need)
}

/// **T06 from the armed ruleset**: when X3 is armed beside clock v2, its `B_remaining` (cold fetch, check, localize, DA disclose,
/// court window, carrier, reorg slack) IS the residual demand of a notice at `a + 3,120`, so it must pass [`palw_court_clock_v2_t06_v2`]
/// as one term (turns = 1). Without X3 the legacy route's own demand is `W_disclose + window_court + reorg`. Used by
/// `Params::validate_palw_court_clock_v2` (arming refused when it fails).
pub fn palw_court_clock_v2_t06_of_params_v2(
    params: &PalwStateParamsV2,
    reorg_retention: u64,
) -> Result<u64, PalwCourtResidualRefusalV2> {
    let turn = match params.claim_hard_deadline_v1() {
        Some(mirror) => crate::palw_ruling_fences_v1::palw_claim_pursuit_budget_v1(params, &mirror.policy),
        None => crate::palw_state_v2::palw_da_disclose_window_daa_v1(params).saturating_add(params.window_court()),
    };
    palw_court_clock_v2_t06_v2(&PalwCourtResidualBudgetV2 {
        turns: 1,
        turn_daa: turn,
        final_evidence_daa: 0,
        recording_daa: 0,
        recovery_daa: 0,
        reorg_retention_daa: reorg_retention,
    })
}

// =================================================================================================================================
// G14 r5 (items 4–5): current signing authority and third-party relay
// =================================================================================================================================

/// **The key a court actor answers under at the response DAA** (RFC-0026 §6 F04; G14 handover §5 item 4). Every court response /
/// duty check reads the actor's bond as the chain holds it AT THE RESPONSE (the fold's own state), never a key captured at the
/// claim's admission: a rotated key answers with the NEW key and the old key's fresh signatures are refused; a `Retiring` bond still
/// backs its existing claims to resolution (ADR-0042 Decision 6 item 9), so it still answers; a bond no longer registered cannot.
///
/// **Relay (item 5):** the authority is the SIGNER's bond, never the carrier's — the signed message binds network, claim, scope,
/// request id, signer, query block / DAA and data digest, and names no submitter, so any party may carry it and pay its fee (an
/// offline Miner's signed answer is relayed as-is). Replay is closed by that binding: a signature over one request id / query DAA
/// verifies for no other, and each court row is write-once (immutable on re-write), so an old signature is never a NEW response.
/// A separate designated court-signer key is NOT a bond field today (adding one changes `PalwBondStateV2`'s borsh layout); until a
/// versioned record exists the responsible bond's current key is the only court signer.
pub fn palw_court_current_signer_key_v2<'a>(
    state: &'a crate::palw_state_v2::PalwChainStateV2,
    actor: &crate::palw_state_v2::PalwBondKeyV2,
) -> Result<&'a [u8], &'static str> {
    use crate::palw_state_v2::PalwBondStatusV2;
    let bond = state.bond(actor).ok_or("court actor's bond is not registered at the response DAA")?;
    match bond.status {
        PalwBondStatusV2::Active | PalwBondStatusV2::Retiring { .. } => Ok(bond.pubkey.as_slice()),
    }
}

// =================================================================================================================================
// G14 r5 (item 5, user decision): the receipt use-window clock pauses while an accusation suspends a certified claim
// =================================================================================================================================

/// One claim's receipt-window pause account: the DAA the current suspension began (if one is open) and the total paused DAA of
/// suspensions already ended by dismissal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwReceiptPauseV2 {
    pub open_since_daa: Option<u64>,
    pub paused_total_daa: u64,
}

impl PalwReceiptPauseV2 {
    /// An accusation opened at `daa` (no-op if one is already open: overlapping accusations are one suspension).
    pub fn open(&mut self, daa: u64) {
        self.open_since_daa.get_or_insert(daa);
    }
    /// The last accusation closed by dismissal at `daa`: the suspended span is credited back.
    pub fn dismiss(&mut self, daa: u64) {
        if let Some(since) = self.open_since_daa.take() {
            self.paused_total_daa = self.paused_total_daa.saturating_add(daa.saturating_sub(since));
        }
    }
}

/// **The receipt use window's start, with the pause applied.** `base` is Gate C's `max(final, certified)`. While suspended the lane
/// is closed (`None`); after dismissal the start moves forward by every paused DAA, so the REMAINING window resumes where it stopped
/// (a window `[s, s + W)` suspended at `p` for `d` DAA becomes `[s + d, s + d + W)`: `p − s` used before, `W − (p − s)` after).
pub fn palw_fp_receipt_window_start_v2(base: u64, pause: &PalwReceiptPauseV2) -> Option<u64> {
    if pause.open_since_daa.is_some() {
        return None;
    }
    base.checked_add(pause.paused_total_daa)
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: u64 = 600;

    fn ok(a: u64, ext: bool) -> PalwCourtClocksV2 {
        palw_court_clocks_v2(a, ext, W, a + DISPUTE_WINDOW_DAA + W, 0).unwrap()
    }

    #[test]
    fn court_clock_v2_undisputed_finishes_at_the_normal_window() {
        let c = ok(1_000, false);
        assert_eq!(c.notice_until, 4_120);
        assert_eq!(c.hard_deadline, 4_120);
        assert_eq!(c.collect_until, 4_121);
        assert_eq!(c.retention_until, 4_120 + W);
        assert_eq!(c.bond_release_after, 4_120 + W);
        assert!(!c.extended);
    }

    #[test]
    fn court_clock_v2_extension_moves_every_clock_together() {
        let (n, e) = (ok(1_000, false), ok(1_000, true));
        let shift = DISPUTE_WINDOW_DAA - NOTICE_WINDOW_DAA;
        assert_eq!(e.notice_until, n.notice_until, "the notice window never moves");
        assert_eq!(e.hard_deadline, n.hard_deadline + shift);
        assert_eq!(e.collect_until, n.collect_until + shift);
        assert_eq!(e.retention_until, n.retention_until + shift);
        assert_eq!(e.bond_release_after, n.bond_release_after + shift);
        assert_eq!(e.hard_deadline, 11_000);
    }

    #[test]
    fn court_clock_v2_boundaries_3120_and_10000() {
        let a = 1_000;
        for (d, inside) in [(a + NOTICE_WINDOW_DAA - 1, true), (a + NOTICE_WINDOW_DAA, true), (a + NOTICE_WINDOW_DAA + 1, false)] {
            assert_eq!(crate::palw_court_policy_v1::shared_initial_notice_daa_in_window_v1(a, d), Ok(inside));
        }
        let e = ok(a, true);
        for (d, inside) in [(a + DISPUTE_WINDOW_DAA - 1, true), (a + DISPUTE_WINDOW_DAA, true), (a + DISPUTE_WINDOW_DAA + 1, false)] {
            assert_eq!(d <= e.hard_deadline, inside);
            assert_eq!(d < e.collect_until, inside);
        }
    }

    #[test]
    fn court_clock_v2_signed_retention_must_cover_the_maximal_extension() {
        let a = 1_000;
        let need = palw_court_clock_v2_required_retention(a, W).unwrap();
        assert_eq!(need, a + DISPUTE_WINDOW_DAA + W);
        assert_eq!(palw_court_clocks_v2(a, false, W, need - 1, 0), Err(CourtPolicyError::DependencyTooShort));
        assert!(palw_court_clocks_v2(a, false, W, need, 0).is_ok());
        assert!(palw_court_clocks_v2(a, true, W, need + 1, 0).is_ok());
    }

    #[test]
    fn court_clock_v2_other_bond_liability_holds_only_the_bond() {
        let c = palw_court_clocks_v2(1_000, true, W, u64::MAX, 50_000).unwrap();
        assert_eq!(c.hard_deadline, 11_000);
        assert_eq!(c.bond_release_after, 50_000);
    }

    #[test]
    fn court_clock_v2_overflow_is_refused() {
        assert_eq!(palw_court_clocks_v2(u64::MAX - 9_999, true, W, u64::MAX, 0), Err(CourtPolicyError::Overflow));
        assert_eq!(palw_court_clock_v2_required_retention(u64::MAX - DISPUTE_WINDOW_DAA, 1), Err(CourtPolicyError::Overflow));
    }

    // ----- G14 r5 -----------------------------------------------------------------------------------------------------------------

    const F: u64 = 1_000;

    fn t12_sp() -> PalwStateParamsV2 {
        use crate::network::{NetworkId, NetworkType};
        match crate::config::params::Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12)).palw_consensus_mode {
            crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => bundle.state,
            _ => panic!("testnet-12 is V2"),
        }
    }

    fn clock_sp() -> PalwStateParamsV2 {
        t12_sp().with_court_clock_v2_from_daa(Some(F))
    }

    fn with_x3(sp: PalwStateParamsV2) -> PalwStateParamsV2 {
        let policy = crate::palw_ruling_fences_v1::PalwClaimHardDeadlineFenceV1::unapproved_probe_at(
            crate::config::params::ForkActivation::new(F),
        )
        .policy;
        sp.with_claim_hard_deadline_v1(Some(crate::palw_ruling_fences_v1::PalwClaimHardDeadlineMirrorV1 { from_daa: F, policy }))
    }

    fn claim(accepted: u64, retention: u64) -> PalwClaimStateV2 {
        use borsh::BorshDeserialize;
        let zeros = [0u8; 8192];
        let mut c = <PalwClaimStateV2 as BorshDeserialize>::deserialize(&mut &zeros[..]).expect("a zero claim decodes");
        c.accepted_daa = accepted;
        c.trace_retention_daa = retention;
        c
    }

    fn dep(kind: PalwCourtDependencyKindV2, h: u64) -> PalwCourtDependencyV2 {
        PalwCourtDependencyV2 { kind, signed_horizon_daa: h }
    }

    #[test]
    fn r5_every_dependency_kind_must_cover_the_extension_minus_one_equal_plus_one() {
        let (a, reorg) = (2_000, 256);
        let need = palw_court_clock_v2_required_retention_with(a, W, reorg).unwrap();
        assert_eq!(need, a + DISPUTE_WINDOW_DAA + W.max(reorg));
        for kind in [
            PalwCourtDependencyKindV2::ClaimTrace,
            PalwCourtDependencyKindV2::Exec,
            PalwCourtDependencyKindV2::Provider,
            PalwCourtDependencyKindV2::Custody,
        ] {
            let ok = dep(PalwCourtDependencyKindV2::ClaimTrace, u64::MAX);
            assert_eq!(
                palw_court_clock_v2_dependencies_cover_v2(a, W, reorg, [&ok, &dep(kind, need - 1)]),
                Err(PalwCourtDependencyRefusalV2::TooShort(kind))
            );
            assert_eq!(palw_court_clock_v2_dependencies_cover_v2(a, W, reorg, [&ok, &dep(kind, need)]), Ok(need));
            assert_eq!(palw_court_clock_v2_dependencies_cover_v2(a, W, reorg, [&ok, &dep(kind, need + 1)]), Ok(need));
        }
    }

    #[test]
    fn r5_reorg_retention_term_comes_from_params_and_dominates_when_larger() {
        assert_eq!(palw_court_clock_v2_reorg_retention(None), 0);
        let cfg = crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1();
        assert_eq!(palw_court_clock_v2_reorg_retention(Some(&cfg)), cfg.reorg_retention_daa);
        let big = W + 10;
        assert_eq!(palw_court_clock_v2_required_retention_with(1_000, W, big).unwrap(), 1_000 + DISPUTE_WINDOW_DAA + big);
        let c = palw_court_clocks_v2_with(1_000, true, W, big, u64::MAX, 0).unwrap();
        assert_eq!(c.retention_until, 11_000 + big, "material retention covers the reorg term");
        assert_eq!(
            palw_court_clocks_v2_with(1_000, true, W, big, 11_000 + big - 1, 0),
            Err(CourtPolicyError::DependencyTooShort)
        );
    }

    #[test]
    fn r5_the_extension_is_refused_atomically_by_a_short_custody_dependency() {
        let sp = clock_sp();
        let need = palw_court_clock_v2_required_retention_with(F, W, 256).unwrap();
        let c = claim(F, need);
        let custody = |h| [dep(PalwCourtDependencyKindV2::Custody, h)];
        assert!(palw_dispute_record_hard_deadline_v2(&sp, &c, W, 256, &custody(need - 1), F + 10).is_err());
        assert_eq!(palw_dispute_record_hard_deadline_v2(&sp, &c, W, 256, &custody(need), F + 10), Ok(F + DISPUTE_WINDOW_DAA));
        // The claim's own trace one short refuses too.
        assert!(palw_dispute_record_hard_deadline_v2(&sp, &claim(F, need - 1), W, 256, &[], F + 10).is_err());
        // Below the fence: old formula, dependencies not read.
        let old = claim(F - 1, 5_000);
        assert_eq!(palw_dispute_record_hard_deadline_v2(&sp, &old, W, 256, &custody(0), F), Ok(5_000 - W));
    }

    #[test]
    fn r5_route_doors_switch_to_clock_v2_for_governed_claims_only() {
        let sp = clock_sp();
        let gov = claim(F, u64::MAX);
        let signed = F + 500;
        // Undisputed governed: door = a + 3,121 (longer than the signed one here), boundaries -1/=/+1.
        assert_eq!(palw_route_collect_until_v2(&sp, None, Some(&gov), W, signed), F + NOTICE_WINDOW_DAA + 1);
        for (now, open) in [(F + NOTICE_WINDOW_DAA - 1, true), (F + NOTICE_WINDOW_DAA, true), (F + NOTICE_WINDOW_DAA + 1, false)] {
            assert_eq!(palw_route_door_open_v2(&sp, None, Some(&gov), W, F, signed, now), open);
        }
        // Disputed governed: the record's hard (a + 10,000) moves every route's door with it.
        let record = PalwDisputeClaimV1 { opened_daa: F + 1, hard_deadline_daa: F + DISPUTE_WINDOW_DAA, ..Default::default() };
        for (now, open) in [(F + DISPUTE_WINDOW_DAA - 1, true), (F + DISPUTE_WINDOW_DAA, true), (F + DISPUTE_WINDOW_DAA + 1, false)] {
            assert_eq!(palw_route_door_open_v2(&sp, Some(&record), Some(&gov), W, F, signed, now), open);
        }
        // Never shorter than a longer signed door.
        assert_eq!(palw_route_collect_until_v2(&sp, None, Some(&gov), W, F + 9_000), F + 9_000);
        // Older claim / no claim row / unarmed: the signed door, byte for byte.
        assert_eq!(palw_route_collect_until_v2(&sp, Some(&record), Some(&claim(F - 1, u64::MAX)), W, signed), signed);
        assert_eq!(palw_route_collect_until_v2(&sp, None, None, W, signed), signed);
        assert_eq!(palw_route_collect_until_v2(&t12_sp(), Some(&record), Some(&gov), W, signed), signed);
        assert!(!palw_route_door_open_v2(&sp, None, Some(&gov), W, F, signed, F - 1), "never before acceptance");
    }

    #[test]
    fn r5_x3_and_clock_v2_combined_start_rule() {
        let sp = with_x3(clock_sp());
        let c = claim(F, u64::MAX);
        let budget = palw_claim_pursuit_budget_v2(&sp, &c);
        assert!(budget > 0, "X3's B_remaining is kept as the budget");
        // The cutoff is clock v2's notice window; the finish line is a + 10,000.
        let last = (F + NOTICE_WINDOW_DAA).min(F + DISPUTE_WINDOW_DAA - budget);
        assert_eq!(crate::palw_ruling_fences_v1::palw_claim_pursuit_may_start_v1(&sp, &c, last), Ok(()));
        assert!(crate::palw_ruling_fences_v1::palw_claim_pursuit_may_start_v1(&sp, &c, last + 1).is_err());
        assert!(crate::palw_ruling_fences_v1::palw_claim_pursuit_may_start_v1(&sp, &c, F - 1).is_err());
        // Without X3: budget 0, the notice window alone.
        let only = clock_sp();
        assert_eq!(palw_claim_pursuit_budget_v2(&only, &c), 0);
        assert_eq!(palw_claim_pursuit_may_start_v2(&only, &c, F + NOTICE_WINDOW_DAA), Ok(()));
        assert!(palw_claim_pursuit_may_start_v2(&only, &c, F + NOTICE_WINDOW_DAA + 1).is_err());
        // Route hard: clock v2 wins over X3 when it governs; X3's own when only X3 applies.
        assert_eq!(palw_claim_route_hard_v2(&sp, &c), Some(F + DISPUTE_WINDOW_DAA));
        let x3_only = with_x3(t12_sp());
        assert_eq!(
            palw_claim_route_hard_v2(&x3_only, &c),
            crate::palw_ruling_fences_v1::palw_claim_hard_deadline_of_v1(&x3_only, &c)
        );
    }

    #[test]
    fn r5_t06_residual_6880_boundary() {
        assert_eq!(PALW_COURT_CLOCK_V2_RESIDUAL_DAA, 6_880);
        let at = |sum: u64| PalwCourtResidualBudgetV2 {
            turns: 1,
            turn_daa: sum,
            final_evidence_daa: 0,
            recording_daa: 0,
            recovery_daa: 0,
            reorg_retention_daa: 0,
        };
        assert_eq!(palw_court_clock_v2_t06_v2(&at(6_878)), Ok(1));
        assert_eq!(palw_court_clock_v2_t06_v2(&at(6_879)), Ok(0), "6,879 + 1 exclusive margin = 6,880 fits exactly");
        assert_eq!(palw_court_clock_v2_t06_v2(&at(6_880)), Err(PalwCourtResidualRefusalV2::Exceeds { need: 6_881 }));
        let full = PalwCourtResidualBudgetV2 {
            turns: 4,
            turn_daa: 1_000,
            final_evidence_daa: 1_000,
            recording_daa: 500,
            recovery_daa: 1_000,
            reorg_retention_daa: 256,
        };
        assert_eq!(palw_court_clock_v2_t06_v2(&full), Ok(6_880 - 6_757));
        assert_eq!(
            palw_court_clock_v2_t06_v2(&PalwCourtResidualBudgetV2 { turns: u64::MAX, turn_daa: 2, ..full }),
            Err(PalwCourtResidualRefusalV2::Overflow)
        );
        // From the shipped ruleset (no X3): W_disclose + window_court + reorg must fit.
        let sp = t12_sp();
        let demand = crate::palw_state_v2::palw_da_disclose_window_daa_v1(&sp) + sp.window_court() + 256 + 1;
        assert_eq!(palw_court_clock_v2_t06_of_params_v2(&sp, 256).is_ok(), demand <= 6_880);
    }

    #[test]
    fn r5_receipt_window_pauses_during_suspension_and_resumes_on_dismissal() {
        let (base, window) = (5_000u64, 600u64);
        let mut pause = PalwReceiptPauseV2::default();
        assert_eq!(palw_fp_receipt_window_start_v2(base, &pause), Some(base));
        // 200 DAA used, then an accusation at base + 200.
        pause.open(base + 200);
        assert_eq!(palw_fp_receipt_window_start_v2(base, &pause), None, "suspended: no receipt");
        pause.open(base + 300); // an overlapping accusation is the same suspension
        pause.dismiss(base + 1_200); // suspended for 1,000
        let start = palw_fp_receipt_window_start_v2(base, &pause).unwrap();
        assert_eq!(start, base + 1_000);
        // The remaining 400 of the window resume at dismissal: [base + 1,200, base + 1,600).
        let end = start + window;
        assert_eq!(end - (base + 1_200), window - 200);
        // A second suspension adds up.
        pause.open(base + 1_300);
        pause.dismiss(base + 1_310);
        assert_eq!(palw_fp_receipt_window_start_v2(base, &pause), Some(base + 1_010));
        // dismiss without an open suspension is a no-op.
        pause.dismiss(base + 9_999);
        assert_eq!(palw_fp_receipt_window_start_v2(base, &pause), Some(base + 1_010));
        assert_eq!(palw_fp_receipt_window_start_v2(u64::MAX, &pause), None, "overflow refuses");
    }

    /// **G14 r6 (item 2): the provider quorum horizon** — the `quorum`-th longest operator lease end; fewer operators than the quorum
    /// cover nothing (0).
    #[test]
    fn r6_provider_quorum_horizon_is_the_quorum_th_longest_operator() {
        assert_eq!(palw_provider_quorum_horizon_of_v2([5_000, 9_000, 7_000], 2), 7_000);
        assert_eq!(palw_provider_quorum_horizon_of_v2([9_000, 9_000], 2), 9_000);
        assert_eq!(palw_provider_quorum_horizon_of_v2([9_000], 2), 0, "one operator is no quorum");
        assert_eq!(palw_provider_quorum_horizon_of_v2(std::iter::empty(), 2), 0);
        assert_eq!(palw_provider_quorum_horizon_of_v2([1], 0), u64::MAX);
    }

    /// **G14 r6 (item 2): an EXEC or provider dependency one DAA short refuses the extension; equal covers.** The kinds the fold now
    /// hands over (`court_clock_v2_exec_provider_dependencies_v2`) go through the same atomic check as the claim's trace and custody.
    #[test]
    fn r6_exec_and_provider_dependencies_refuse_a_short_extension() {
        let sp = clock_sp();
        let need = palw_court_clock_v2_required_retention_with(F, W, 256).unwrap();
        let c = claim(F, need);
        for kind in [PalwCourtDependencyKindV2::Exec, PalwCourtDependencyKindV2::Provider] {
            let deps = |h| [dep(PalwCourtDependencyKindV2::Custody, u64::MAX), dep(kind, h)];
            assert!(palw_dispute_record_hard_deadline_v2(&sp, &c, W, 256, &deps(need - 1), F + 10).is_err(), "{kind:?} short");
            assert_eq!(palw_dispute_record_hard_deadline_v2(&sp, &c, W, 256, &deps(need), F + 10), Ok(F + DISPUTE_WINDOW_DAA));
        }
        // A transferred claim whose provider quorum covers nothing (lapsed / one operator) refuses.
        let quorum = palw_provider_quorum_horizon_of_v2([u64::MAX], 2);
        assert!(
            palw_dispute_record_hard_deadline_v2(&sp, &c, W, 256, &[dep(PalwCourtDependencyKindV2::Provider, quorum)], F + 10).is_err()
        );
    }

    /// **G14 r6 (item 3): X3's cap under the combined rule.** X3 alone: X3's hard. X3 and clock v2: `a + 10,000`. Clock v2 alone or
    /// neither: no cap (`None`, the old rule).
    #[test]
    fn r6_x3_cap_reads_the_combined_rule() {
        let c = claim(F, u64::MAX);
        let x3_only = with_x3(t12_sp());
        assert_eq!(
            palw_claim_x3_cap_hard_v2(&x3_only, &c),
            crate::palw_ruling_fences_v1::palw_claim_hard_deadline_of_v1(&x3_only, &c)
        );
        assert!(palw_claim_x3_cap_hard_v2(&x3_only, &c).is_some());
        assert_eq!(palw_claim_x3_cap_hard_v2(&with_x3(clock_sp()), &c), Some(F + DISPUTE_WINDOW_DAA));
        assert_eq!(palw_claim_x3_cap_hard_v2(&clock_sp(), &c), None);
        assert_eq!(palw_claim_x3_cap_hard_v2(&t12_sp(), &c), None);
        // A claim accepted below both fences: no cap.
        assert_eq!(palw_claim_x3_cap_hard_v2(&with_x3(clock_sp()), &claim(F - 1, u64::MAX)), None);
    }

    /// **audit(verify-2) B-06** — expected-vs-actual (FAILS on current code). A governed claim (its own trace raised to the full
    /// extension by the claim writer) whose dependencies sit at horizons ADMISSION accepts: a TIR custody horizon at the collection
    /// door (`collect_until <= material_until` is the plan's only lower bound; the install checks only `hard >= collect_until`) and an
    /// EXEC root floor `expiry + window_court` for an expiry just past the notice window (EXEC admits any `daa < expiry <=
    /// daa + 100,000`). The first in-window dispute (a + 10) is expected to be admitted (capped or at least not refused); on the
    /// current code it is REFUSED, so a claim admitted with those dependencies is not disputable on the legacy route at all.
    #[test]
    fn verify2_b06_an_admissible_dependency_horizon_refuses_the_first_dispute() {
        let sp = clock_sp();
        let reorg = 256;
        let need = palw_court_clock_v2_required_retention_with(F, W, reorg).unwrap();
        let c = claim(F, need); // the claim writer raises its own trace to `need` (transition_builder.rs, U06/U07)
        let hard = palw_claim_route_hard_v2(&sp, &c).expect("governed");
        let collect_until = F + crate::palw_court_policy_v1::NOTICE_WINDOW_DAA + 1;
        assert!(hard >= collect_until, "the TIR install's only clock-v2 check passes");
        let custody = dep(PalwCourtDependencyKindV2::Custody, collect_until);
        let exec = dep(PalwCourtDependencyKindV2::Exec, collect_until + 1 + sp.window_court());
        for deps in [vec![custody], vec![exec]] {
            let got = palw_dispute_record_hard_deadline_v2(&sp, &c, W, reorg, &deps, F + 10);
            eprintln!("[verify2-b06] window_court {}, need {need}, deps {:?}: {got:?}", sp.window_court(),
                deps.iter().map(|d| (d.kind, d.signed_horizon_daa)).collect::<Vec<_>>());
            assert!(got.is_ok(), "B-06: expected the first dispute on an admitted claim to be admitted, got {got:?}");
        }
    }
}
