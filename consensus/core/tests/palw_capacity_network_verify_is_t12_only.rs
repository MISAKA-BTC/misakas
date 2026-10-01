//! **int-11 — F-N's static verification term (`Params::palw_capacity_network_verify`, lane P's `L_ver`) is dormant on every
//! shipped preset, and arming it is a scheduled fence like any other** (rfc4/int-capdrill, 2026-10-01).
//!
//! `L_net = min(L_seat, L_carry, L_anchor)` bounds the unlicensed queue by capital and carriage, not by what the seats
//! verify: with the verification supply collapsed (1.2–1.86 licences/DAA against a healthy 5.7) a queue pinned at the level
//! waits past the receipt window, and the second timeout slashes the honest producer. Past this fence the level is also
//! capped by `L_ver = ⌊1.5 × (window_receipt − anchor_delay) / 2⌋` — 435 at the shipped 600 / 20 — a constant, by the staged
//! capacity rule (the adaptive form is stage 6).
//!
//! As shipped the field is `None` everywhere, testnet-12 included, and testnet-12's three ids are the DAA-1,300 release's to
//! the byte; armed at a height it moves `consensus_params_id` and `consensus_schedule_id` but NOT `consensus_identity_id`;
//! `Some(never())` is absence; `validate_palw_v2` refuses it off ConsensusV2, without F-N at or below it, and with a bundle
//! mirror that disagrees. It is in no flag-day list (the int-11 flag day names its height) and not in the capacity list.

use kaspa_consensus_core::config::params::{
    ForkActivation, MAINNET_PARAMS, PALW_T12_CAPACITY_FENCES_V1, PALW_T12_CAPACITY_NETWORK_VERIFY_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V1,
    Params, PalwPostLaunchFenceV1, SIMNET_PARAMS, TESTNET_PARAMS, devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params,
    palw_t12_release_v2_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_v1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_network_room_v1::palw_network_verify_level_v1;

/// testnet-12 as THIS build ships its DAA-1,300 release — params, identity, schedule (the F-N test's own pin: a dormant
/// fence must not move it).
// re-pin 2026-09-27 @b2bf20a78b0d: third post-launch flag day: capacity tests judge their fence against the DAA-1,300 release (release_v2) (was cbe9152f…, f78b02ad…)
const T12_RELEASE: (&str, &str, &str) = (
    "24e1aec3e9a102fa40d559cd28005ad5944c32caa485d685bed65c52e4c056ff",
    "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
    "d263d7f2971f4e20b57b26d7b7428bd8f9346c3728bbb6927341d8b36b0c1c3a",
);

const NAME: &str = "palw_capacity_network_verify";
const ROOM: &str = "palw_capacity_network_room";

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

/// The fold's mirror of the term, and the two numbers `L_ver` reads.
fn mirror(p: &Params) -> (Option<u64>, u64, u64) {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => {
            (bundle.state.capacity_network_verify_from_daa(), bundle.state.window_receipt(), bundle.state.capacity_network_anchor_delay())
        }
        _ => (None, 0, 0),
    }
}

fn entry() -> &'static PalwPostLaunchFenceV1 {
    PALW_T12_CAPACITY_NETWORK_VERIFY_FENCES_V1.iter().find(|f| f.name == NAME).expect("listed")
}

/// The release with every capacity fence (F-N included) armed at `h`, the term still dormant.
fn base_at(h: u64) -> Params {
    let mut p = palw_t12_release_v2_params();
    for f in PALW_T12_CAPACITY_FENCES_V1.iter() {
        (f.set)(&mut p, Some(ForkActivation::new(h)));
    }
    p.validate_palw_v2().expect("the capacity list over the release validates");
    p
}

#[test]
fn the_fence_is_dormant_on_every_shipped_preset_and_testnet12_is_the_release() {
    for (name, p) in presets() {
        assert_eq!(p.palw_capacity_network_verify, None, "{name}: the term ships dormant");
        assert_eq!(p.palw_capacity_network_verify_fence(), None, "{name}");
        assert_eq!(mirror(&p).0, None, "{name}: the fold's mirror is empty");
        assert!(p.palw_fences_v1().iter().any(|(n, f)| *n == NAME && f.is_none()), "{name}: on the fork-id list");
        assert_eq!(p.validate_palw_capacity_network_verify_v1(), Ok(()), "{name}: nothing to refuse");
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
        (entry().set)(&mut armed, Some(ForkActivation::new(h)));
        armed.validate_palw_v2().unwrap_or_else(|e| panic!("the term at {h}: {e:?}"));
        let (p, i, s) = ids(&armed);
        let (bp, _, bs) = ids(&base);
        assert_ne!(p, bp, "the term at {h}: the ruleset names it");
        assert_ne!(s, bs, "the term at {h}: the schedule names it");
        assert_eq!(i, identity, "the term at {h}: the identity does not move");
        assert_eq!(armed.palw_capacity_network_verify_fence(), Some(ForkActivation::new(h)));
        let (from, receipt, delay) = mirror(&armed);
        assert_eq!(from, Some(h));
        // The constant the term caps the level at: 435 at the shipped windows.
        assert_eq!(palw_network_verify_level_v1(receipt, delay), 435, "L_ver at the shipped receipt window {receipt} / anchor delay {delay}");
        assert!(armed.palw_capacity_network_verify.is_some() && base.palw_capacity_network_verify.is_none());
        assert!(!bundle_of(&armed).capacity_network_verify_active_at(h - 1) && bundle_of(&armed).capacity_network_verify_active_at(h));
        // What the fold caps `L_net` with: nothing below the height, 435 from it (and after).
        assert_eq!(bundle_of(&armed).capacity_network_verify_level_at(h - 1), None);
        assert_eq!(bundle_of(&armed).capacity_network_verify_level_at(h), Some(435));
        assert_eq!(bundle_of(&armed).capacity_network_verify_level_at(h + 10_000), Some(435));
        assert_eq!(bundle_of(&base).capacity_network_verify_level_at(h + 10_000), None, "the term unset: today's level, always");
        let s = fork_id_v1(&shipped, h);
        assert!(evaluate_fork_id_v1(&armed, h, s.fired.as_bytes().as_slice(), s.next).refuses(), "gated from the height");
        let mut never = base.clone();
        (entry().set)(&mut never, Some(ForkActivation::never()));
        never.validate_palw_v2().expect("a never-armed fence is no fence");
        assert_eq!(mirror(&never).0, None);
        assert_eq!(never.consensus_identity_id().to_string(), identity, "Some(never()) is absence in the identity");
    }
}

fn bundle_of(p: &Params) -> &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2 {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => &bundle.state,
        _ => panic!("ConsensusV2"),
    }
}

#[test]
fn the_fence_needs_f_n_at_or_below_it_and_its_mirror() {
    let h = 1_500;
    // F-N alone is absent: the release over no capacity list at all.
    let mut alone = palw_t12_release_v2_params();
    (entry().set)(&mut alone, Some(ForkActivation::new(h)));
    let why = alone.validate_palw_capacity_network_verify_v1().expect_err("the term without F-N");
    assert!(format!("{why:?}").contains("without palw_capacity_network_room at or below it"), "{why:?}");
    // F-N one DAA above the term: there is no level to cap yet at the term's height.
    let mut above = base_at(h);
    let room = PALW_T12_CAPACITY_FENCES_V1.iter().find(|f| f.name == ROOM).expect("F-N is in the capacity list");
    (room.set)(&mut above, Some(ForkActivation::new(h + 1)));
    (entry().set)(&mut above, Some(ForkActivation::new(h)));
    let why = above.validate_palw_capacity_network_verify_v1().expect_err("F-N above the term");
    assert!(format!("{why:?}").contains("without palw_capacity_network_room at or below it"), "{why:?}");
    // F-N at the same height is enough.
    let mut level = base_at(h);
    (entry().set)(&mut level, Some(ForkActivation::new(h)));
    assert_eq!(level.validate_palw_capacity_network_verify_v1(), Ok(()));
    // A field armed without its mirror disagrees with the bundle.
    let mut unsynced = base_at(h);
    unsynced.palw_capacity_network_verify = Some(ForkActivation::new(h));
    let why = unsynced.validate_palw_capacity_network_verify_v1().expect_err("unsynced");
    assert!(format!("{why:?}").contains("disagrees with the V2 bundle's mirror"), "{why:?}");
    // A mirror without the field: the bundle says armed, the ruleset does not.
    let mut orphan = base_at(h);
    if let PalwConsensusMode::ConsensusV2(bundle) = &mut orphan.palw_consensus_mode {
        bundle.state = bundle.state.clone().with_capacity_network_verify_mirror(Some(h));
    }
    let why = orphan.validate_palw_capacity_network_verify_v1().expect_err("a mirror with the fence unarmed");
    assert!(format!("{why:?}").contains("without the fence armed"), "{why:?}");
    // Off ConsensusV2 the fold that reads it is not running.
    let mut v1 = MAINNET_PARAMS;
    v1.palw_capacity_network_verify = Some(ForkActivation::new(h));
    assert!(v1.validate_palw_capacity_network_verify_v1().is_err_and(|e| format!("{e:?}").contains("off ConsensusV2")));
    for (name, mut p) in presets() {
        p.palw_capacity_network_verify = Some(ForkActivation::never());
        assert_eq!(p.validate_palw_capacity_network_verify_v1(), Ok(()), "{name}: never() is dormant");
    }
}

#[test]
fn it_is_in_no_list_but_its_own_and_the_drill_names_it() {
    assert!(PALW_T12_POST_LAUNCH_FENCES_V1.iter().all(|f| f.name != NAME), "not in the DAA-750 list");
    assert!(PALW_T12_CAPACITY_FENCES_V1.iter().all(|f| f.name != NAME), "not in the capacity list (the int-11 flag day names its height)");
    assert_eq!(PALW_T12_CAPACITY_NETWORK_VERIFY_FENCES_V1.len(), 1);
    let mut p = base_at(1_001);
    (entry().set)(&mut p, Some(ForkActivation::new(1_001)));
    assert_eq!(mirror(&p).0, Some(1_001));
    // Clearing it clears the mirror and returns the exact ids.
    let released = base_at(1_001);
    (entry().set)(&mut p, None);
    assert_eq!(mirror(&p).0, None);
    assert_eq!(ids(&p), ids(&released));
}
