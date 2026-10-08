//! **C4 round 3, F-C4R3-04 (P2, release process): a fence that joins an already-scheduled height is invisible to the fork id.**
//!
//! The fork id names heights, not fences (`fork_id_v1`'s own doc: "it must be a height no other fence uses"). testnet-12's int-13 list
//! arms at DAA 9,000, and its doc invites the lanes that are not ready (`palw_probabilistic_constraints_v1`, `palw_panel_free_v1`,
//! `palw_permissionless_panel_v1`, …) to "join by one line here plus the re-pin". Once a fleet runs the int-13 build (`2e567642…`),
//! a later build that adds ANY fence at 9,000 has the same fired set and the same next fence at every height: the handshake keeps the
//! two as peers past 9,000, where they disagree about blocks — a silent fork with only a schedule-id warning. This pins the hazard
//! (it passes): the safe rule is a fresh height, or a frozen int-13 list once its build is deployed.
use kaspa_consensus_core::config::params::{ForkActivation, PALW_T12_INT13_DAA, Params, palw_t12_shipped_params};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_v1};

#[test]
fn c4r3_observation_a_fence_joining_the_int13_height_leaves_the_fork_id_unchanged() {
    let shipped = palw_t12_shipped_params();
    let at = PALW_T12_INT13_DAA.expect("the int-13 flag day has a height");
    assert!(shipped.fence_schedule_v1().contains(&at), "9,000 is already a scheduled height");
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
