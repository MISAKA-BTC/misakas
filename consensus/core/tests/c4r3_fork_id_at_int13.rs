//! **C4 round 3, F-C4R3-04 (P2, release process): a fence that joins an already-scheduled height is invisible to the fork id.**
//!
//! The fork id names heights, not fences (`fork_id_v1`'s own doc: "it must be a height no other fence uses"). testnet-12's int-13 list
//! was to arm at DAA 9,000 (the user cancelled that flag day on 2026-10-08: the list now waits for the full-activation release and
//! takes a fresh height then), and its doc invites the lanes that are not ready (`palw_probabilistic_constraints_v1`,
//! `palw_panel_free_v1`, `palw_permissionless_panel_v1`, …) to "join by one line here plus the re-pin". Once a fleet runs a build that
//! has the list armed at some height, a later build that adds ANY fence at that height has the same fired set and the same next fence at
//! every height: the handshake keeps the two as peers past the height, where they disagree about blocks — a silent fork with only a
//! schedule-id warning. This pins the hazard (it passes) on a list ARMED EXPLICITLY at a test height: the safe rule is a fresh height,
//! or a frozen int-13 list once its build is deployed.
use kaspa_consensus_core::config::params::{
    ForkActivation, PALW_T12_INT13_DAA, Params, palw_t12_arm_int13_flag_day_at_v1, palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_v1};

/// The test height the list is armed at (the cancelled flag day's 9,000; no other fence uses it).
const AT: u64 = 9_000;

#[test]
fn c4r3_observation_a_fence_joining_the_int13_height_leaves_the_fork_id_unchanged() {
    // The shipped ruleset schedules the list nowhere (no DAA-9,000 flag day); the hazard is stated on the list armed at a test height,
    // the shape of the build the future release will deploy.
    assert_eq!(PALW_T12_INT13_DAA, None, "no DAA-9,000 flag day (user, 2026-10-08)");
    assert!(!palw_t12_shipped_params().fence_schedule_v1().contains(&AT), "{AT} is not scheduled on the shipped ruleset");
    let mut shipped = palw_t12_shipped_params();
    palw_t12_arm_int13_flag_day_at_v1(&mut shipped, Some(AT));
    let at = AT;
    assert!(shipped.fence_schedule_v1().contains(&at), "the list's height is a scheduled height once the list is armed");
    let joins: Vec<(&str, Box<dyn Fn(&mut Params)>)> = vec![
        (
            "palw_probabilistic_constraints_v1",
            Box::new(move |p: &mut Params| p.palw_probabilistic_constraints_v1 = Some(ForkActivation::new(at))),
        ),
        ("palw_signed_registration_v1", Box::new(move |p: &mut Params| p.palw_signed_registration_v1 = Some(ForkActivation::new(at)))),
        (
            "palw_panel_free_v1",
            Box::new(move |p: &mut Params| {
                p.palw_panel_free_v1 =
                    Some(kaspa_consensus_core::palw_panel_free_v1::PalwPanelFreeFenceV1::at(ForkActivation::new(at)))
            }),
        ),
    ];
    for (name, join) in joins {
        let mut joined = shipped.clone();
        join(&mut joined);
        assert_ne!(joined.consensus_params_id(), shipped.consensus_params_id(), "{name}: it IS another ruleset");
        for daa in [at - 1, at, at + 1, at + 10_000] {
            assert_eq!(
                fork_id_v1(&joined, daa),
                fork_id_v1(&shipped, daa),
                "{name}: the fork id at {daa} cannot tell the builds apart"
            );
        }
        // Past the height the two disagree about blocks; neither refuses the other.
        let theirs = fork_id_v1(&shipped, at + 1);
        assert!(!evaluate_fork_id_v1(&joined, at + 1, theirs.fired.as_bytes().as_slice(), theirs.next).refuses(), "{name}");
        let ours = fork_id_v1(&joined, at + 1);
        assert!(!evaluate_fork_id_v1(&shipped, at + 1, ours.fired.as_bytes().as_slice(), ours.next).refuses(), "{name}");
    }
}
