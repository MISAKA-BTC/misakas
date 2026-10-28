//! **The room is exact, so a claim admitted at step 3 with the own attempt reserved leaves the
//! attempt its room at step 4** — 2026-09-24 audit #4 review item 4.
//!
//! At e93be0f2 each class's demand was ONE term rounded up and the room a floored quotient of what
//! was left: `⌈(J+1)x⌉ + ⌈kx⌉ ≤ free` at step 3 could hold while `⌈(J+k)x⌉ + ⌈x⌉ ≤ free` at step 4
//! failed by one Q32 unit — 32 of 29,400 grid cases, and a single class at an exact fit (room 3 at
//! step 3, 0 at step 4). Past the review the class's capacity is the largest `n` whose WHOLE term
//! fits beside the other classes' (`palw_panel_capacity_by_rate_v1`) and the room is capacity less
//! what the class owes, so admitting `k` claims is exactly "the term with them still fits".
//!
//! Pure functions only; the shipped classes never reached the gap (window 2 divides 2^32, and the
//! short-window row's cost is divisible by 3), which is also pinned here.
//!
//! Run: cargo test -p kaspa-consensus-core --test panel_room_capacity_is_exact_between_steps

use kaspa_consensus_core::palw_work_target_v1::{
    PALW_PANEL_DEMAND_SCALE_V1 as Q, palw_panel_capacity_by_rate_v1 as capacity, palw_panel_demand_term_v1 as term,
};

/// A class's room: its capacity beside `others` less what it owes.
fn room(per_span: u128, others: u128, window: u64, cost: u128, owed: u128) -> u128 {
    (capacity(per_span, others, window, cost) as u128).saturating_sub(owed)
}

#[test]
fn a_single_class_at_an_exact_fit_keeps_the_own_attempts_room() {
    // per_span 2, cost 1, window 3: the budget holds exactly 6. J = 2 pending, the own attempt
    // reserved at step 3, a commitment of k = 3 jobs.
    let (p, c, w) = (2u128, 1u128, 3u64);
    let step3 = room(p, 0, w, c, 2 + 1);
    let step4 = room(p, 0, w, c, 2 + 3);
    println!("single class: step-3 room {step3} (needs 3), step-4 room {step4} (needs 1)");
    assert!(step3 >= 3, "the commitment is admitted with the attempt reserved");
    assert!(step4 >= 1, "and the block's own attempt still finds its room at step 4");
}

#[test]
fn window_two_never_rounds() {
    for c in [172919123392u128 * 5, 83102171136 * 5, 18055200736 * 5, 21657728 * 5, 7, 13] {
        for n in 0..50u128 {
            assert_eq!(term(n, c, 2) * 2, n * c * Q, "window 2 term exact");
        }
    }
}

#[test]
fn step3_implies_step4_and_capacity_is_the_feasibility_bound_over_the_grid() {
    // The review's grid: other classes' demand in [0, Q], per_span 1..6, window 1..8, cost 1..6,
    // J 0..6 pending, commitments of k 2..6 jobs.
    let (mut violations, mut total, mut infeasible) = (0u64, 0u64, 0u64);
    for w in 1..8u64 {
        for c in 1..6u128 {
            for p in 1..6u128 {
                for other in [0u128, 1, Q / 3, Q / 3 + 1, Q / 2, Q - 1, Q] {
                    // Capacity is exactly "the class's whole term fits beside the others".
                    let cap = capacity(p, other, w, c) as u128;
                    for n in 0..=cap + 1 {
                        if (term(n, c, w) + other <= p * Q) != (n <= cap) {
                            infeasible += 1;
                        }
                    }
                    for j in 0..6u128 {
                        for k in 2..6u128 {
                            total += 1;
                            let s3 = room(p, other, w, c, j + 1) >= k;
                            let s4 = room(p, other, w, c, j + k) >= 1;
                            if s3 && !s4 {
                                violations += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    println!("step-3 admitted, step-4 refused: {violations} / {total}; capacity disagreeing with feasibility: {infeasible}");
    assert_eq!(violations, 0, "no commitment admitted at step 3 leaves the own attempt without room at step 4");
    assert_eq!(infeasible, 0, "capacity is the largest n whose term fits");
}
