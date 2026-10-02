//! **ADR-0160 stage 4 — F-N (`Params::palw_capacity_network_room`, the network level and the
//! work-conserving fair share) is dormant on every shipped preset, and arming it is a scheduled fence like
//! any other** (rcore/cap-s1).
//!
//! As shipped the field is `None` everywhere, testnet-12 included, and testnet-12's three ids are the
//! DAA-1,300 release's to the byte; armed at a future height it moves `consensus_params_id` and
//! `consensus_schedule_id` but NOT `consensus_identity_id`; `Some(never())` is absence; `validate_palw_v2`
//! refuses it off ConsensusV2, without F-R, F-S and lane A at or below it, and with a bundle mirror that
//! disagrees. An entry of `PALW_T12_CAPACITY_FENCES_V1`, never of the DAA-750 list.

use kaspa_consensus_core::config::params::{
    ForkActivation, MAINNET_PARAMS, PALW_T12_CAPACITY_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V1, Params, SIMNET_PARAMS,
    TESTNET_PARAMS, devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params, palw_t12_arm_capacity_fences_v1,
    palw_t12_release_v2_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_v1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

/// testnet-12 as THIS build ships it — the DAA-1,300 release (rcore/int-5), every capacity
/// fence dormant (rcore/cap-s1): params, identity, schedule.
// re-pin 2026-09-27 @b2bf20a78b0d: third post-launch flag day: capacity tests judge their fence against the DAA-1,300 release (release_v2) (was cbe9152f…, f78b02ad…)
const T12_RELEASE: (&str, &str, &str) = (
    "24e1aec3e9a102fa40d559cd28005ad5944c32caa485d685bed65c52e4c056ff",
    "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
    "d263d7f2971f4e20b57b26d7b7428bd8f9346c3728bbb6927341d8b36b0c1c3a",
);

const NAME: &str = "palw_capacity_network_room";

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn presets() -> Vec<(&'static str, Params)> {
    vec![
        ("testnet-12", palw_t12_release_v2_params()),
        ("testnet-11", palw_rc_shipped_params()),
        ("devnet", devnet_shipped_params()),
        ("mainnet", mainnet_shipped_params()),
        ("testnet-10", Params::from(TESTNET_PARAMS.net)),
        ("simnet", Params::from(SIMNET_PARAMS.net)),
    ]
}

fn mirror(p: &Params) -> (Option<u64>, u64) {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => {
            (bundle.state.capacity_network_from_daa(), bundle.state.capacity_network_anchor_delay())
        }
        _ => (None, 0),
    }
}

fn entry(name: &str) -> &'static kaspa_consensus_core::config::params::PalwPostLaunchFenceV1 {
    PALW_T12_CAPACITY_FENCES_V1.iter().find(|f| f.name == name).expect("listed")
}

/// The release with every capacity fence but F-N armed at `h`.
fn base_at(h: u64) -> Params {
    let mut p = palw_t12_release_v2_params();
    for f in PALW_T12_CAPACITY_FENCES_V1.iter().filter(|f| f.name != NAME) {
        (f.set)(&mut p, Some(ForkActivation::new(h)));
    }
    p.validate_palw_v2().expect("the capacity list without F-N validates over the release");
    p
}

#[test]
fn the_fence_is_dormant_on_every_shipped_preset_and_testnet12_is_the_release() {
    for (name, p) in presets() {
        assert_eq!(p.palw_capacity_network_room, None, "{name}: F-N ships dormant");
        assert_eq!(p.palw_capacity_network_room_fence(), None, "{name}");
        assert_eq!(mirror(&p), (None, 0), "{name}: the fold's mirror is empty");
        assert!(p.palw_fences_v1().iter().any(|(n, f)| *n == NAME && f.is_none()), "{name}: on the fork-id list");
    }
    let t12 = palw_t12_release_v2_params();
    t12.validate_palw_v2().expect("testnet-12 as shipped validates");
    let now = ids(&t12);
    assert_eq!((now.0.as_str(), now.1.as_str(), now.2.as_str()), T12_RELEASE, "testnet-12 is the release's ruleset, to the id");
}

#[test]
fn arming_the_fence_moves_the_params_and_schedule_ids_but_not_the_identity() {
    let shipped = palw_t12_release_v2_params();
    let (_, identity, _) = ids(&shipped);
    for h in [1_001u64, 1_500, 5_000] {
        let base = base_at(h);
        let mut armed = base.clone();
        (entry(NAME).set)(&mut armed, Some(ForkActivation::new(h)));
        armed.validate_palw_v2().unwrap_or_else(|e| panic!("F-N at {h}: {e:?}"));
        let (p, i, s) = ids(&armed);
        let (bp, _, bs) = ids(&base);
        assert_ne!(p, bp, "F-N at {h}: the ruleset names it");
        assert_ne!(s, bs, "F-N at {h}: the schedule names it");
        assert_eq!(i, identity, "F-N at {h}: the identity does not move");
        assert_eq!(armed.palw_capacity_network_room_fence(), Some(ForkActivation::new(h)));
        let (from, delay) = mirror(&armed);
        assert_eq!(from, Some(h));
        assert!(delay > 0, "the mirror carries the panel's anchor delay");
        let s = fork_id_v1(&shipped, h);
        assert!(evaluate_fork_id_v1(&armed, h, s.fired.as_bytes().as_slice(), s.next).refuses(), "gated from the height");
        let mut never = base.clone();
        (entry(NAME).set)(&mut never, Some(ForkActivation::never()));
        never.validate_palw_v2().expect("a never-armed fence is no fence");
        assert_eq!(mirror(&never), (None, 0));
        assert_eq!(never.consensus_identity_id().to_string(), identity, "Some(never()) is absence in the identity");
    }
}

#[test]
fn the_fence_needs_f_r_f_s_and_lane_a_and_its_mirror() {
    let h = 1_500;
    let mut alone = palw_t12_release_v2_params();
    (entry(NAME).set)(&mut alone, Some(ForkActivation::new(h)));
    let why = alone.validate_palw_capacity_stage2_v1().expect_err("F-N alone");
    assert!(format!("{why:?}").contains("palw_capacity_network_room is armed without"), "{why:?}");
    let mut unsynced = base_at(h);
    unsynced.palw_capacity_network_room = Some(ForkActivation::new(h));
    let why = unsynced.validate_palw_capacity_stage2_v1().expect_err("unsynced");
    assert!(format!("{why:?}").contains("disagrees with the V2 bundle's mirror"), "{why:?}");
    let mut v1 = MAINNET_PARAMS;
    v1.palw_capacity_network_room = Some(ForkActivation::new(h));
    assert!(v1.validate_palw_capacity_stage2_v1().is_err_and(|e| format!("{e:?}").contains("not ConsensusV2")));
    for (name, mut p) in presets() {
        p.palw_capacity_network_room = Some(ForkActivation::never());
        assert_eq!(p.validate_palw_capacity_stage2_v1(), Ok(()), "{name}: never() is dormant");
    }
}

#[test]
fn the_capacity_list_carries_it_and_the_750_list_does_not() {
    assert!(PALW_T12_POST_LAUNCH_FENCES_V1.iter().all(|f| f.name != NAME));
    assert!(PALW_T12_CAPACITY_FENCES_V1.iter().any(|f| f.name == NAME));
    let mut p = palw_t12_release_v2_params();
    palw_t12_arm_capacity_fences_v1(&mut p, Some(ForkActivation::new(1_001))).expect("the whole list at one height");
    assert_eq!(mirror(&p).0, Some(1_001));
}
