//! **G14 lane F03b2: packet accounting and deterministic schedule of the claim-common shared frontier (ADR-0192 A/A
//! addendum, PROVISIONAL, the user may change it).**
//!
//! These are estimates (目安) made under the 2026-10-22 value delegation, to be replaced by the M-C measurement. The packet is
//! `PALW_G14_PACKET_BYTES_V1` (V1, 34,000 B). Size alone never refuses. A coordinate whose maximum response is B bytes is charged
//! `⌈B / 34,000⌉` packets against the capacity reserved at original acceptance. The work bound per packet is the V11 op_work of
//! its class and is recorded in the register; it is not separately enforced, since no per-op work measurement exists yet.
//!
//! **Feasibility.** Let `a` be the accepting DAA, `h = a + W`, `c` the reserved packets per DAA, `p_i` the packets of coordinate `i`,
//! and `Q = Σ p_i` over the closed universe (every branch). Admission requires `Q ≤ c·W`; otherwise the original is refused at
//! acceptance with no state change. Each coordinate opens at most one row (same coordinate coalesces), so the k-th served job ends
//! at `a + ⌈(Σ_{j≤k} p_j)/c⌉ − 1 ≤ a + ⌈Q/c⌉ − 1 < h` when served back to back from `a`. A request that arrives after the cursor
//! starts at its own DAA, and the fold refuses one whose slot cannot end before `h`. That refusal means "no reserved capacity
//! left", not a size refusal.
//!
//! **Order.** Earliest response deadline first, then ascending coordinate id. Every shared request's deadline is the original's
//! `h`, so in the fold the order is arrival order (object order within a block). The general scheduler below is used for the
//! proof and the tests.

/// `Q ≤ c · W` (Q in packets).
pub fn shared_frontier_capacity_admits_v1(queries_packets: u64, packets_per_daa: u64, window_daa: u64) -> bool {
    packets_per_daa.checked_mul(window_daa).is_some_and(|cap| queries_packets <= cap)
}

/// Packets for `bytes`: `⌈bytes / packet⌉`, at least 1.
pub fn packets_for_bytes_v1(bytes: u64) -> u64 {
    bytes.div_ceil(crate::palw_g14_values_v1::PALW_G14_PACKET_BYTES_V1).max(1)
}

/// One distinct legal choice: response deadline (exclusive), coordinate id, and packets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SharedChoiceV1 {
    pub deadline_daa: u64,
    pub coordinate: [u8; 64],
    pub packets: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedScheduleRefusalV1 {
    ZeroCapacity,
    Late { coordinate: [u8; 64], end_daa: u64 },
    Overflow,
}

/// Deterministic EDF over packets from `start_daa`. Duplicates coalesce to the earliest deadline. Returns (choice, start, end).
pub fn schedule_shared_choices_v1(
    choices: &[SharedChoiceV1],
    start_daa: u64,
    packets_per_daa: u64,
    hard_daa: u64,
) -> Result<Vec<(SharedChoiceV1, u64, u64)>, SharedScheduleRefusalV1> {
    if packets_per_daa == 0 {
        return Err(SharedScheduleRefusalV1::ZeroCapacity);
    }
    let mut by = std::collections::BTreeMap::<[u8; 64], (u64, u64)>::new();
    for c in choices {
        let e = by.entry(c.coordinate).or_insert((c.deadline_daa, c.packets));
        e.0 = e.0.min(c.deadline_daa);
        e.1 = e.1.max(c.packets);
    }
    let mut jobs: Vec<SharedChoiceV1> =
        by.into_iter().map(|(coordinate, (deadline_daa, packets))| SharedChoiceV1 { deadline_daa, coordinate, packets }).collect();
    jobs.sort();
    let mut used = 0u64;
    let mut out = Vec::with_capacity(jobs.len());
    for job in jobs {
        let first = start_daa.checked_add(used / packets_per_daa).ok_or(SharedScheduleRefusalV1::Overflow)?;
        used = used.checked_add(job.packets).ok_or(SharedScheduleRefusalV1::Overflow)?;
        let end = start_daa.checked_add(used.div_ceil(packets_per_daa) - 1).ok_or(SharedScheduleRefusalV1::Overflow)?;
        if end >= job.deadline_daa || end >= hard_daa {
            return Err(SharedScheduleRefusalV1::Late { coordinate: job.coordinate, end_daa: end });
        }
        out.push((job, first, end));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f03b2_packets_charge_size_and_never_refuse_it() {
        assert_eq!(packets_for_bytes_v1(0), 1);
        assert_eq!(packets_for_bytes_v1(33_999), 1);
        assert_eq!(packets_for_bytes_v1(34_000), 1);
        assert_eq!(packets_for_bytes_v1(34_001), 2);
        assert_eq!(packets_for_bytes_v1(10 * 34_000 + 1), 11);
        // capacity boundary in packets: c·W − 1, c·W admitted; c·W + 1 refused
        assert!(shared_frontier_capacity_admits_v1(9_999, 1, 10_000));
        assert!(shared_frontier_capacity_admits_v1(10_000, 1, 10_000));
        assert!(!shared_frontier_capacity_admits_v1(10_001, 1, 10_000));
    }

    #[test]
    fn f03b2_overlapping_choices_serve_the_outsider_oversized_choice_within_deadline() {
        let (a, w, c) = (1_000u64, 10_000u64, 1u64);
        let hard = a + w;
        let coord = |i: u64| {
            let mut x = [0u8; 64];
            x[..8].copy_from_slice(&i.to_be_bytes());
            x
        };
        let mut choices = Vec::new();
        for i in 1..200u64 {
            choices.push(SharedChoiceV1 { deadline_daa: a + 400 + 3 * i, coordinate: coord(i), packets: 2 });
            choices.push(SharedChoiceV1 { deadline_daa: hard, coordinate: coord(i), packets: 2 }); // refill duplicate
        }
        let outsider = SharedChoiceV1 { deadline_daa: hard, coordinate: coord(0), packets: packets_for_bytes_v1(500_000) };
        choices.push(outsider);
        let total: u64 = 199 * 2 + outsider.packets;
        assert!(shared_frontier_capacity_admits_v1(total, c, w));
        let s = schedule_shared_choices_v1(&choices, a, c, hard).unwrap();
        assert_eq!(s.len(), 200);
        let (_, _, end) = *s.iter().find(|(j, _, _)| j.coordinate == outsider.coordinate).unwrap();
        assert!(end < hard && end < a + total);
        let mut rev = choices.clone();
        rev.reverse();
        assert_eq!(schedule_shared_choices_v1(&rev, a, c, hard).unwrap(), s);
        assert!(matches!(
            schedule_shared_choices_v1(&[SharedChoiceV1 { deadline_daa: hard, coordinate: coord(1), packets: w + 1 }], a, c, hard),
            Err(SharedScheduleRefusalV1::Late { .. })
        ));
    }
}
