//! **G14 lane F03b2: the per-request admission budget and the deterministic schedule of the claim-common shared frontier
//! (ADR-0192 A/A addendum, PROVISIONAL — the user may change it).**
//!
//! Both values are ESTIMATES (目安) under the 2026-10-22 value delegation, to be replaced by the M-C measurement:
//! * bytes: one court packet, `PALW_G14_PACKET_BYTES_V1` (V1, 34,000 B);
//! * work: the `op_work` of the request's operation class (V11, `[5, 13, 1, 11]`).
//!
//! An operation over either bound is refused before any state is touched (the caller checks first).
//!
//! **Schedule.** Requests at the same coordinate coalesce (the fold's `Coalesced`). Distinct legal choices are served in a
//! deterministic order: earliest response deadline first, then ascending coordinate id. Feasibility within the original's hard
//! window follows from the capacity reserved at acceptance. Let `a` be the accepting DAA, `h = a + W` the hard DAA, `c` the
//! reserved service packets per DAA (`PALW_G14_SERVICE_BUDGET_PACKETS_PER_DAA_V1`), and `Q` the size of the closed request
//! universe. Each request costs at most one packet, because the byte budget is one packet. Admission requires `Q <= c * W`
//! ([`shared_frontier_capacity_admits_v1`]). EDF over unit jobs then serves the k-th job (1-based) at
//! `a + ceil(k / c) - 1 < a + ceil(Q / c) <= a + W = h`. Every service DAA is therefore inside the hard window, whatever subset
//! the coalition requests first: coalition requests are themselves coordinates of the same universe, and coalescing never adds a
//! job. Per-job deadlines are checked by [`schedule_shared_choices_v1`] (EDF is optimal for unit jobs on one resource).
use crate::palw_court_capacity_v1::PalwCourtOpClassV1;
use crate::palw_g14_values_v1::{PALW_G14_COURT_CAPACITY_POLICY_ESTIMATE_V1, PALW_G14_PACKET_BYTES_V1};

/// Status string for the register: both bounds are estimates.
pub const PALW_SHARED_FRONTIER_BUDGET_STATUS_V1: &str =
    "provisional estimate 2026-10-26 (lane F03b2): bytes = 1 packet (V1), work = op_work[class] (V11); replace by M-C measurement";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedFrontierBudgetRefusalV1 {
    Bytes { carried: u64, budget: u64 },
    Work { demand: u64, budget: u64 },
}

/// The budget of one shared-frontier operation of `class`: (bytes, work).
pub fn shared_frontier_budget_v1(class: PalwCourtOpClassV1) -> (u64, u64) {
    (PALW_G14_PACKET_BYTES_V1, PALW_G14_COURT_CAPACITY_POLICY_ESTIMATE_V1.op_work[class as usize])
}

/// Refuse an operation whose carried bytes or work demand exceeds its class budget (equality is admitted).
pub fn check_shared_frontier_budget_v1(class: PalwCourtOpClassV1, carried_bytes: u64, work: u64) -> Result<(), SharedFrontierBudgetRefusalV1> {
    let (bytes, work_budget) = shared_frontier_budget_v1(class);
    if carried_bytes > bytes {
        return Err(SharedFrontierBudgetRefusalV1::Bytes { carried: carried_bytes, budget: bytes });
    }
    if work > work_budget {
        return Err(SharedFrontierBudgetRefusalV1::Work { demand: work, budget: work_budget });
    }
    Ok(())
}

/// `Q <= c * W`: the reserved service capacity of the hard window covers the whole closed universe at one packet each.
pub fn shared_frontier_capacity_admits_v1(queries: u64, packets_per_daa: u64, window_daa: u64) -> bool {
    packets_per_daa.checked_mul(window_daa).is_some_and(|cap| queries <= cap)
}

/// One distinct legal choice: its response deadline (exclusive upper DAA) and coordinate id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SharedChoiceV1 {
    pub deadline_daa: u64,
    pub coordinate: [u8; 64],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedScheduleRefusalV1 {
    ZeroCapacity,
    /// A choice could not be served before its deadline or the hard DAA (cannot happen under `shared_frontier_capacity_admits_v1`
    /// with deadlines at hard; reported, never silently dropped).
    Late { coordinate: [u8; 64], service_daa: u64 },
    Overflow,
}

/// Deterministic EDF schedule from `start_daa` with `packets_per_daa` unit services per DAA. Duplicates (same coordinate) coalesce
/// to the earliest deadline. Returns (choice, service DAA) in service order.
pub fn schedule_shared_choices_v1(
    choices: &[SharedChoiceV1],
    start_daa: u64,
    packets_per_daa: u64,
    hard_daa: u64,
) -> Result<Vec<(SharedChoiceV1, u64)>, SharedScheduleRefusalV1> {
    if packets_per_daa == 0 {
        return Err(SharedScheduleRefusalV1::ZeroCapacity);
    }
    let mut by_coordinate = std::collections::BTreeMap::<[u8; 64], u64>::new();
    for c in choices {
        let d = by_coordinate.entry(c.coordinate).or_insert(c.deadline_daa);
        *d = (*d).min(c.deadline_daa);
    }
    let mut jobs: Vec<SharedChoiceV1> =
        by_coordinate.into_iter().map(|(coordinate, deadline_daa)| SharedChoiceV1 { deadline_daa, coordinate }).collect();
    jobs.sort(); // (deadline, coordinate) ascending
    let mut out = Vec::with_capacity(jobs.len());
    for (k, job) in jobs.into_iter().enumerate() {
        let slot = (k as u64) / packets_per_daa;
        let service = start_daa.checked_add(slot).ok_or(SharedScheduleRefusalV1::Overflow)?;
        if service >= job.deadline_daa || service >= hard_daa {
            return Err(SharedScheduleRefusalV1::Late { coordinate: job.coordinate, service_daa: service });
        }
        out.push((job, service));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use PalwCourtOpClassV1 as C;

    #[test]
    fn f03b2_budget_refuses_one_over_and_admits_at_and_below() {
        for class in [C::OneMove, C::Opening, C::DissectionMove, C::Close] {
            let (bytes, work) = shared_frontier_budget_v1(class);
            assert_eq!(bytes, 34_000);
            assert_eq!(work, [5, 13, 1, 11][class as usize]);
            assert!(check_shared_frontier_budget_v1(class, bytes - 1, work).is_ok());
            assert!(check_shared_frontier_budget_v1(class, bytes, work).is_ok());
            assert_eq!(check_shared_frontier_budget_v1(class, bytes + 1, work),
                Err(SharedFrontierBudgetRefusalV1::Bytes { carried: bytes + 1, budget: bytes }));
            if work > 0 { assert!(check_shared_frontier_budget_v1(class, bytes, work - 1).is_ok()); }
            assert_eq!(check_shared_frontier_budget_v1(class, bytes, work + 1),
                Err(SharedFrontierBudgetRefusalV1::Work { demand: work + 1, budget: work }));
        }
    }

    /// Overlapping branches: the coalition requests every other coordinate first with earlier deadlines and duplicates them; the
    /// outsider's distinct choice is still served before its deadline and the hard DAA, given `Q <= c * W`.
    #[test]
    fn f03b2_overlapping_choices_serve_the_outsider_within_deadline() {
        let (a, w, c) = (1_000u64, 10_000u64, 1u64);
        let hard = a + w;
        let q = 200u64; // test-only closed universe size
        assert!(shared_frontier_capacity_admits_v1(q, c, w));
        assert!(!shared_frontier_capacity_admits_v1(w + 1, c, w));
        let coord = |i: u64| { let mut x = [0u8; 64]; x[..8].copy_from_slice(&i.to_be_bytes()); x };
        let mut choices = Vec::new();
        for i in 0..q - 1 {
            // coalition: every other coordinate, each twice (refill), deadlines spread before the outsider's
            choices.push(SharedChoiceV1 { deadline_daa: a + 300 + i, coordinate: coord(i + 1) });
            choices.push(SharedChoiceV1 { deadline_daa: hard, coordinate: coord(i + 1) });
        }
        let outsider = SharedChoiceV1 { deadline_daa: hard, coordinate: coord(0) };
        choices.push(outsider);
        let s = schedule_shared_choices_v1(&choices, a, c, hard).unwrap();
        assert_eq!(s.len() as u64, q, "duplicates coalesce");
        let served = s.iter().find(|(j, _)| j.coordinate == outsider.coordinate).unwrap().1;
        assert!(served < outsider.deadline_daa && served < hard);
        assert!(s.iter().all(|(j, d)| *d < j.deadline_daa));
        // Deterministic order: input permutation does not change the schedule.
        let mut rev = choices.clone(); rev.reverse();
        assert_eq!(schedule_shared_choices_v1(&rev, a, c, hard).unwrap(), s);
        // Ties on deadline break by ascending coordinate.
        let tie = schedule_shared_choices_v1(&[SharedChoiceV1 { deadline_daa: hard, coordinate: coord(9) },
            SharedChoiceV1 { deadline_daa: hard, coordinate: coord(3) }], a, c, hard).unwrap();
        assert_eq!(tie[0].0.coordinate, coord(3));
        // Infeasible deadline is reported, not dropped.
        assert!(matches!(schedule_shared_choices_v1(&[SharedChoiceV1 { deadline_daa: a, coordinate: coord(1) }], a, c, hard),
            Err(SharedScheduleRefusalV1::Late { .. })));
    }
}
