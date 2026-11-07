//! **Past the fence op 186's Held reason names the window verdict the read computed: window,
//! seats, both, or recovering** — 2026-09-24 audit #4 review item 5.
//!
//! At e93be0f2 the past-fence reason inferred the cause from the row alone: short of
//! `required_ready_seats` read as "seats", everything else as "the window does not fit". So a class
//! held for its window while 5 ≤ ready < 7 read only as short of seats, and a row held for
//! utilization before the fence (ready ≥ required, window fits), read past it before its next
//! boundary, was labelled "window does not fit". `palw_lifecycle_reason_v2` now takes the window
//! verdict (`window_fits`, what op 186 computes from the class's window and receipt deadline).
//!
//! Run: cargo test -p kaspa-consensus-core --test panel_room_held_reason_names_the_window_verdict

use kaspa_consensus_core::palw_model_registry_v1::*;

fn row(ready: u32, util: u32, window: u32) -> PalwModelLifecycleRowV1 {
    PalwModelLifecycleRowV1 {
        state: PalwModelLifecycleV1::Held,
        work: PalwModelWorkV1::default(),
        profile: PalwDerivedProfileV1 { verification_window_spans: window, required_ready_seats: 7, ..Default::default() },
        since_span: 0,
        probes_passed: 0,
        probes_failed: 0,
        probes_passed_this_span: 0,
        probes_failed_this_span: 0,
        ready_seats: ready,
        inflight_claims: 3,
        utilization_permille: util,
        admission_milli: 0,
        cap_utilization_permille: 0,
        priced_share_permille: 0,
    }
}

#[test]
fn the_held_reason_names_window_seats_both_or_recovering() {
    let g = PALW_REGISTRY_GLOBALS_V1;
    assert_eq!(g.seat_count, 5);
    let r = |ready, util, fits| palw_lifecycle_reason_v2(&row(ready, util, 2), false, &g, false, fits);
    let below_seats = r(3, 0, Some(true));
    let short_of_seats = r(6, 0, Some(true));
    let window = r(7, 0, Some(false));
    let both = r(6, 0, Some(false));
    // A row held under the old rule for utilization (ready 7 ≥ required, window fits), read past the
    // fence before its next boundary.
    let recovering = r(7, 1_292, Some(true));
    // A reader that cannot ask the window.
    let unknown = r(7, 0, None);
    println!("below seats: {below_seats}\nshort of seats: {short_of_seats}\nwindow: {window}\nboth: {both}");
    println!("recovering: {recovering}\nunknown window: {unknown}");

    assert!(below_seats.contains("needed for a panel"));
    assert!(short_of_seats.contains("re-enter probation") && !short_of_seats.contains("does not fit"));
    assert!(window.contains("does not fit the receipt deadline") && !window.contains("re-enter probation"));
    assert!(
        both.contains("does not fit the receipt deadline") && both.contains("re-enter probation"),
        "a class held for its window with 5 ≤ ready < 7 names both causes"
    );
    assert!(
        !recovering.contains("does not fit") && recovering.contains("re-enters probation at the next boundary"),
        "a utilization hold read past the fence is not labelled a window hold"
    );
    assert!(unknown.contains("does not fit") && unknown.contains("or its readiness lapsed"), "an unasked window says so");
    // Below the fence the utilization reading is the reason, whatever the window says.
    assert!(palw_lifecycle_reason_v2(&row(7, 1_292, 2), false, &g, true, Some(true)).contains("overloaded"));
}
