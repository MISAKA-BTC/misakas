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
    palw_court_clocks_v2(claim.accepted_daa, true, w_disclose, claim.trace_retention_daa, 0)
        .map(|c| c.hard_deadline)
        .map_err(|e| match e {
            CourtPolicyError::DependencyTooShort => {
                "palw_court_clock_v2: the claim's signed retention does not cover a + 10,000 + W_disclose (F07)"
            }
            CourtPolicyError::Overflow => "palw_court_clock_v2: the claim's clocks overflow",
        })
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
}
