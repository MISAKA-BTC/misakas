//! **Where a product does not fit, the rate rule refuses at every ready-seat count** — 2026-09-24
//! audit #4 review item 6.
//!
//! At e93be0f2 the pure functions saturated instead: a crafted permissionless class with a draw
//! cost of 2^97 at window `u32::MAX` refused up to 10^6 ready seats, but its own room turned
//! positive at about 2^24 ready seats, and a normal class read room against a saturated term (≈ MAX
//! divided by the window, an undercount) from about the same point — unreachable, and wrong in the
//! admitting direction. Past the review the term is `u128::MAX` and the capacity 0 wherever a
//! product does not fit, so neither ever admits.
//!
//! Run: cargo test -p kaspa-consensus-core --test panel_room_saturation_fails_closed

use kaspa_consensus_core::palw_model_registry_v1::PALW_REGISTRY_GLOBALS_V1 as G;
use kaspa_consensus_core::palw_work_target_v1::{palw_panel_capacity_by_rate_v1 as capacity, palw_panel_demand_term_v1 as term};

fn per_span(ready: u128) -> u128 {
    ready * G.reference_work_per_span * (G.utilization_permille as u128) / 1_000
}

#[test]
fn a_saturating_class_never_admits_and_never_lets_another_class_admit_against_it() {
    let w = u32::MAX as u64;
    // A crafted class whose draw cost's Q32 image does not fit (cost × 2^32 ≥ 2^128).
    let cost = 1u128 << 97;
    let t = term(1, cost, w);
    println!("the crafted class's one-claim term at w = u32::MAX: {t:#x}");
    assert_eq!(t, u128::MAX, "its term is a demand no budget holds");
    let mut first_admit_own = None;
    let mut first_admit_other = None;
    for k in 0..40u32 {
        let ready = 1u128 << k;
        if first_admit_own.is_none() && capacity(per_span(ready), 0, w, cost) > 0 {
            first_admit_own = Some(k);
        }
        // Another (normal) class read beside the crafted class's one claim.
        if first_admit_other.is_none() && capacity(per_span(ready), t, 2, 1_000_000) > 0 {
            first_admit_other = Some(k);
        }
    }
    println!("first admit at ready = 2^k: crafted class {first_admit_own:?}; a normal class beside it {first_admit_other:?}");
    assert_eq!(first_admit_own, None, "the crafted class admits at no ready-seat count up to 2^39");
    assert_eq!(first_admit_other, None, "and nothing admits against its claim");
    for ready in [1u128, 8, 100, 10_000, 1_000_000] {
        assert_eq!(capacity(per_span(ready), 0, w, cost), 0, "ready {ready}");
        assert_eq!(capacity(per_span(ready), t, 2, 1_000_000), 0, "ready {ready}");
    }
}
