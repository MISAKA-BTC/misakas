//! **G14 item 3 / RFC-0026 F03 (lane r5-f03-ingress, 2026-10-22): the paid public court ingress** — so one outsider completes
//! disclosure → localization → verdict even when every owner slot is held by colluders who keep refilling.
//!
//! Dormant twice over: behind the existing `Params::palw_court_outsider_lane_v1` (court-owner lane; no new fence) AND behind the
//! fee schedule below, which is **typed-missing in every non-test build** (`PalwCourtIngressMissingV1::Schedule`). With the schedule
//! missing, the fold keeps the court-owner lane's rule byte for byte; nothing here is armed or adopted.
//!
//! ```text
//! advance reservation  every claim admitted past the lane fence holds ONE outside-side court slot from its admission to its hard
//! (ADR-0192)           deadline (the admission coordinator calls `palw_court_ingress_claim_admits_v1`; at most
//!                      R = ⌊cap / 2⌋ claims hold one at a time, so the non-reserved room `cap − R` is never empty)
//! reserved slot        the claim's FIRST outside-side session (opener neither the producer nor a panel seat) opens on its claim's
//!                      own reservation: no other claim's session, no inside session and no follow-on can occupy it
//! non-reserved room    every other session competes for `cap − #reserved claims` (and keeps the court-owner follow-on share)
//! fee                  every session opened through the paid ingress pays `fee_sompi_per_session` ONCE, at admission (a queued
//!                      prosecution pays when its row is created, never again at its ticket); burned like the court occupancy fee
//! queue                past `palw_court_capacity_v1` the existing deadline-aware queue decides (queued honest prosecution is granted
//!                      first, G-07); the ingress only adds its once-only fee there
//! ```
//!
//! No cryptographic proof of computation is used (PRINCIPLES §8): admission is accounting over public state.

/// The values the paid ingress needs that are NOT adopted (user decision; RFC-0025 is Draft and its numbers are not used).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwCourtIngressScheduleV1 {
    /// The once-only ingress fee a session opened through the paid path burns from its opener's bond, sompi.
    pub fee_sompi_per_session: u64,
}

/// Why the paid ingress cannot run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwCourtIngressMissingV1 {
    /// The ingress fee (amount, payer = opener) is not adopted.
    Schedule,
}

#[cfg(test)]
thread_local! {
    static TEST_SCHEDULE: std::cell::Cell<Option<PalwCourtIngressScheduleV1>> = const { std::cell::Cell::new(None) };
}

/// TEST ONLY: per-thread injection; no non-test build can reach a value.
#[cfg(test)]
pub(crate) fn set_palw_court_ingress_test_schedule_v1(schedule: Option<PalwCourtIngressScheduleV1>) {
    TEST_SCHEDULE.with(|c| c.set(schedule));
}

/// Production: **V4 — USER-APPROVED 2026-10-26 (estimate used as is; 「目安の値をそのまま使う」承認). Review condition: replace by measurement.** Basis: lane VAL [`crate::palw_g14_values_v1::PALW_G14_COURT_INGRESS_ESTIMATE_V1`].
/// A test may still inject its own schedule per thread.
pub fn palw_court_ingress_schedule_v1() -> Result<PalwCourtIngressScheduleV1, PalwCourtIngressMissingV1> {
    #[cfg(test)]
    if let Some(schedule) = TEST_SCHEDULE.with(|c| c.get()) {
        return Ok(schedule);
    }
    Ok(crate::palw_g14_values_v1::PALW_G14_COURT_INGRESS_ESTIMATE_V1)
}

/// **How many claims may hold an outside-side reservation at once**: `⌊cap / 2⌋`. The non-reserved room is then at least
/// `⌈cap / 2⌉ ≥ 1`. A pure function of the unchanged network cap (no adopted number).
pub fn palw_court_ingress_reserved_claims_cap_v1(cap: u64) -> u64 {
    cap / 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r5_f03_ingress_schedule_is_the_approved_estimate_unless_injected() {
        set_palw_court_ingress_test_schedule_v1(None);
        assert_eq!(palw_court_ingress_schedule_v1(), Ok(crate::palw_g14_values_v1::PALW_G14_COURT_INGRESS_ESTIMATE_V1));
        let s = PalwCourtIngressScheduleV1 { fee_sompi_per_session: 7 };
        set_palw_court_ingress_test_schedule_v1(Some(s));
        assert_eq!(palw_court_ingress_schedule_v1(), Ok(s));
        set_palw_court_ingress_test_schedule_v1(None);
    }

    #[test]
    fn r5_f03_reserved_claims_leave_non_reserved_room() {
        for cap in 1..64u64 {
            let r = palw_court_ingress_reserved_claims_cap_v1(cap);
            assert!(cap - r >= 1 && r <= cap / 2, "cap {cap}");
        }
    }
}
