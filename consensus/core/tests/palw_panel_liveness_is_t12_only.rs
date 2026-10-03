//! **Lane PL (ADR-0166) — the three panel-liveness fences are dormant on every shipped preset, and arming them is a scheduled
//! fence like any other** (rcore/panel-liveness, 2026-10-03): `palw_panel_unavailable_expiry` (C), `palw_panel_fast_switch` (D),
//! `palw_seat_availability` (E).
//!
//! As shipped each field is `None` everywhere, testnet-12 included, and testnet-12's three ids are the release's to the byte;
//! armed at a height each moves `consensus_params_id` and `consensus_schedule_id` but NOT `consensus_identity_id`;
//! `Some(never())` is absence; `validate_palw_v2` refuses each off ConsensusV2, without its prerequisites at or below it, and with a
//! bundle mirror that disagrees.

use kaspa_consensus_core::config::params::{
    ForkActivation, MAINNET_PARAMS, PALW_T12_PANEL_LIVENESS_FENCES_V1, Params, SIMNET_PARAMS, TESTNET_PARAMS, devnet_shipped_params,
    mainnet_shipped_params, palw_rc_shipped_params, palw_t12_release_v2_params,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

// The release's ruleset (the F-N verification test's own pin: a dormant fence must not move it).
const T12_RELEASE: (&str, &str, &str) = (
    "24e1aec3e9a102fa40d559cd28005ad5944c32caa485d685bed65c52e4c056ff",
    "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
    "d263d7f2971f4e20b57b26d7b7428bd8f9346c3728bbb6927341d8b36b0c1c3a",
);

const C: &str = "palw_panel_unavailable_expiry";
const D: &str = "palw_panel_fast_switch";
const E: &str = "palw_seat_availability";

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

fn mirrors(p: &Params) -> (Option<u64>, Option<u64>, Option<u64>) {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => {
            (b.state.panel_unavailable_expiry_from_daa(), b.state.panel_fast_switch_from_daa(), b.state.seat_availability_from_daa())
        }
        _ => (None, None, None),
    }
}

fn entry(name: &str) -> &'static kaspa_consensus_core::config::params::PalwPostLaunchFenceV1 {
    PALW_T12_PANEL_LIVENESS_FENCES_V1.iter().find(|f| f.name == name).expect("listed")
}

fn validate_all(p: &Params) -> Result<(), String> {
    p.validate_palw_panel_unavailable_expiry_v1()
        .and_then(|_| p.validate_palw_panel_fast_switch_v1())
        .and_then(|_| p.validate_palw_seat_availability_v1())
        .map_err(|e| format!("{e:?}"))
}

#[test]
fn the_fences_are_dormant_on_every_shipped_preset_and_testnet12_is_the_release() {
    for (name, p) in presets() {
        assert_eq!((p.palw_panel_unavailable_expiry, p.palw_panel_fast_switch, p.palw_seat_availability), (None, None, None), "{name}");
        assert_eq!(mirrors(&p), (None, None, None), "{name}: the fold's mirrors are empty");
        for fence in [C, D, E] {
            assert!(p.palw_fences_v1().iter().any(|(n, f)| *n == fence && f.is_none()), "{name}: {fence} is on the fork-id list");
        }
        assert_eq!(validate_all(&p), Ok(()), "{name}: nothing to refuse");
    }
    let t12 = palw_t12_release_v2_params();
    t12.validate_palw_v2().expect("testnet-12 as shipped validates");
    let now = ids(&t12);
    assert_eq!((now.0.as_str(), now.1.as_str(), now.2.as_str()), T12_RELEASE, "testnet-12 is the release's ruleset, to the id");
    assert_eq!(PALW_T12_PANEL_LIVENESS_FENCES_V1.iter().map(|f| f.name).collect::<Vec<_>>(), [C, D, E], "C before D (D needs C)");
}

#[test]
fn arming_the_list_moves_the_params_and_schedule_ids_but_not_the_identity() {
    let shipped = palw_t12_release_v2_params();
    let (p0, identity, s0) = ids(&shipped);
    for h in [1_001u64, 5_300] {
        let mut armed = shipped.clone();
        for f in PALW_T12_PANEL_LIVENESS_FENCES_V1 {
            (f.set)(&mut armed, Some(ForkActivation::new(h)));
        }
        armed.validate_palw_v2().unwrap_or_else(|e| panic!("the list at {h}: {e:?}"));
        let (p, i, s) = ids(&armed);
        assert_ne!(p, p0, "the ruleset names the list at {h}");
        assert_ne!(s, s0, "the schedule names it");
        assert_eq!(i, identity, "the identity does not move");
        assert_eq!(mirrors(&armed), (Some(h), Some(h), Some(h)));
        // Each fence alone moves the ids too (so a node that lacks one is refused at the height).
        for f in [C, D, E] {
            let mut one = shipped.clone();
            (entry(f).set)(&mut one, Some(ForkActivation::new(h)));
            assert_ne!(ids(&one).0, p0, "{f} is in the fingerprint");
        }
        let mut never = shipped.clone();
        for f in PALW_T12_PANEL_LIVENESS_FENCES_V1 {
            (f.set)(&mut never, Some(ForkActivation::never()));
        }
        never.validate_palw_v2().expect("never-armed is no fence");
        assert_eq!(mirrors(&never), (None, None, None));
        assert_eq!(ids(&never).1, identity);
    }
    // Clearing returns the exact ids.
    let mut p = shipped.clone();
    for f in PALW_T12_PANEL_LIVENESS_FENCES_V1 {
        (f.set)(&mut p, Some(ForkActivation::new(2_000)));
    }
    for f in PALW_T12_PANEL_LIVENESS_FENCES_V1.iter().rev() {
        (f.set)(&mut p, None);
    }
    assert_eq!(ids(&p), ids(&shipped));
}

#[test]
fn each_fence_needs_its_prerequisites_at_or_below_it_and_its_mirror() {
    let h = 1_500;
    let base = palw_t12_release_v2_params();
    // D without C is refused; with C at the same height it is not; with C above it, refused.
    let mut d_alone = base.clone();
    (entry(D).set)(&mut d_alone, Some(ForkActivation::new(h)));
    let why = d_alone.validate_palw_panel_fast_switch_v1().expect_err("D without C");
    assert!(format!("{why:?}").contains("without palw_panel_unavailable_expiry at or below it"), "{why:?}");
    let mut c_above = d_alone.clone();
    (entry(C).set)(&mut c_above, Some(ForkActivation::new(h + 1)));
    assert!(c_above.validate_palw_panel_fast_switch_v1().is_err(), "C one DAA above D");
    let mut level = d_alone.clone();
    (entry(C).set)(&mut level, Some(ForkActivation::new(h)));
    assert_eq!(level.validate_palw_panel_fast_switch_v1(), Ok(()));
    // E needs only rcore_plus, which the release has; C needs only the audit fence, which it has.
    let mut e = base.clone();
    (entry(E).set)(&mut e, Some(ForkActivation::new(h)));
    assert_eq!(e.validate_palw_seat_availability_v1(), Ok(()));
    let mut c = base.clone();
    (entry(C).set)(&mut c, Some(ForkActivation::new(h)));
    assert_eq!(c.validate_palw_panel_unavailable_expiry_v1(), Ok(()));
    // A field armed without its mirror disagrees with the bundle; a mirror without the field is refused.
    let mut unsynced = base.clone();
    unsynced.palw_seat_availability = Some(ForkActivation::new(h));
    assert!(format!("{:?}", unsynced.validate_palw_seat_availability_v1().unwrap_err()).contains("disagrees with the V2 bundle's mirror"));
    let mut orphan = base.clone();
    if let PalwConsensusMode::ConsensusV2(bundle) = &mut orphan.palw_consensus_mode {
        bundle.state = bundle.state.clone().with_panel_unavailable_expiry_mirror(Some(h));
    }
    assert!(format!("{:?}", orphan.validate_palw_panel_unavailable_expiry_v1().unwrap_err()).contains("without the fence armed"));
    // Off ConsensusV2 the fold that reads them is not running.
    let mut v1 = MAINNET_PARAMS;
    v1.palw_panel_unavailable_expiry = Some(ForkActivation::new(h));
    assert!(v1.validate_palw_panel_unavailable_expiry_v1().is_err_and(|e| format!("{e:?}").contains("off ConsensusV2")));
    for (name, mut p) in presets() {
        p.palw_panel_unavailable_expiry = Some(ForkActivation::never());
        p.palw_panel_fast_switch = Some(ForkActivation::never());
        p.palw_seat_availability = Some(ForkActivation::never());
        assert_eq!(validate_all(&p), Ok(()), "{name}: never() is dormant");
    }
}
