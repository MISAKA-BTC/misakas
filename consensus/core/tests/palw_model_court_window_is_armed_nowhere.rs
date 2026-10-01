//! **`palw_model_court_window` is armed on no network, and its place in the delta enum is the END's** —
//! the integration's pins for the release line's per-model court window (`rcore/int-10`, merged into
//! `rfc4/int` on 2026-10-01).
//!
//! The window is a dormant fence: testnet-12 charges every class the held clock, under which the window the
//! fence derives is the network's for every admissible class and no admission verdict moves
//! (`palw_t12_court_window_changes_no_admission.rs`), so the DAA-3,600 flag day is `palw_tir_fence2` alone. The
//! delta entry that records a window (`PalwDeltaEntryV2::ClassCourtWindow`) was declared at position 90 on the
//! release line; RFC-0003's `GenClass` holds 90 (spec 17 §17.0), so on this line the entry sits at the END of
//! the enum — 100. That move is safe only while no stored delta carries the variant, which is exactly what
//! "armed nowhere" says; so the two facts are pinned together, here: a ruleset that arms the window, or a
//! flag-day list that names it with a height, fails this file before the position can matter.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::drill::{PALW_DRILL_SALT_LEN_V1, PalwDrillSaltV1};
use kaspa_consensus_core::config::params::{
    DEVNET_PARAMS, MAINNET_PARAMS, PALW_T12_CAPACITY_FENCES_V1, PALW_T12_CAPACITY_RHO10_FENCES_V1, PALW_T12_DECODE_RULES_FENCES_V1,
    PALW_T12_MODEL_COURT_WINDOW_DAA, PALW_T12_MODEL_COURT_WINDOW_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V1,
    PALW_T12_POST_LAUNCH_FENCES_V2, PALW_T12_POST_LAUNCH_FENCES_V3, PALW_T12_TIR_FENCE2_FENCES_V1, PALW_T12_TIR_FLAG_DAY_FENCES_V1,
    Params, SIMNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS, devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params,
    palw_t12_drill_params_v1, palw_t12_launch_params_v1, palw_t12_release_v1_params, palw_t12_release_v2_params,
    palw_t12_release_v3_params, palw_t12_release_v4_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_state_v2::PalwDeltaEntryV2;

fn salt() -> PalwDrillSaltV1 {
    PalwDrillSaltV1::from_bytes([0x4d; PALW_DRILL_SALT_LEN_V1]).expect("a legal salt")
}

/// Every ruleset a node can run, as it ships (the baselines of testnet-12's releases included).
fn rulesets() -> Vec<(&'static str, Params)> {
    vec![
        ("MAINNET_PARAMS", MAINNET_PARAMS),
        ("TESTNET_PARAMS", TESTNET_PARAMS),
        ("TESTNET11_PARAMS", TESTNET11_PARAMS),
        ("DEVNET_PARAMS", DEVNET_PARAMS),
        ("SIMNET_PARAMS", SIMNET_PARAMS),
        ("from(mainnet)", Params::from(NetworkId::new(NetworkType::Mainnet))),
        ("from(testnet-10)", Params::from(NetworkId::with_suffix(NetworkType::Testnet, 10))),
        ("from(testnet-11)", Params::from(NetworkId::with_suffix(NetworkType::Testnet, 11))),
        ("from(testnet-12)", Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))),
        ("from(devnet)", Params::from(NetworkId::new(NetworkType::Devnet))),
        ("from(simnet)", Params::from(NetworkId::new(NetworkType::Simnet))),
        ("mainnet_shipped_params", mainnet_shipped_params()),
        ("devnet_shipped_params", devnet_shipped_params()),
        ("palw_rc_shipped_params", palw_rc_shipped_params()),
        ("palw_t12_shipped_params", palw_t12_shipped_params()),
        ("palw_t12_launch_params_v1", palw_t12_launch_params_v1()),
        ("palw_t12_release_v1_params", palw_t12_release_v1_params()),
        ("palw_t12_release_v2_params", palw_t12_release_v2_params()),
        ("palw_t12_release_v3_params", palw_t12_release_v3_params()),
        ("palw_t12_release_v4_params", palw_t12_release_v4_params()),
        ("palw_t12_drill_params_v1", palw_t12_drill_params_v1(&salt())),
    ]
}

#[test]
fn no_shipped_ruleset_arms_the_model_court_window() {
    for (name, p) in rulesets() {
        assert!(p.palw_model_court_window.is_none(), "{name}: the window is dormant");
        assert!(!p.palw_model_court_window_active_at(u64::MAX), "{name}: and answers no at every height");
        assert!(
            p.palw_fences_v1().contains(&("palw_model_court_window", None)),
            "{name}: the exhaustive fence list names it, unarmed"
        );
        assert!(p.validate_palw_model_court_window().is_ok(), "{name}: nothing to refuse");
    }
}

#[test]
fn its_testnet_12_height_is_none_and_no_flag_day_list_but_its_own_names_it() {
    assert_eq!(PALW_T12_MODEL_COURT_WINDOW_DAA, None, "the release names no height: the list is a drill's and a later release's");
    assert_eq!(PALW_T12_MODEL_COURT_WINDOW_FENCES_V1.len(), 1);
    assert_eq!(PALW_T12_MODEL_COURT_WINDOW_FENCES_V1[0].name, "palw_model_court_window");
    for (what, list) in [
        ("DAA-750", PALW_T12_POST_LAUNCH_FENCES_V1),
        ("second flag day", PALW_T12_POST_LAUNCH_FENCES_V2),
        ("third flag day", PALW_T12_POST_LAUNCH_FENCES_V3),
        ("capacity", PALW_T12_CAPACITY_FENCES_V1),
        ("capacity rho 10", PALW_T12_CAPACITY_RHO10_FENCES_V1),
        ("IR flag day", PALW_T12_TIR_FLAG_DAY_FENCES_V1),
        ("DAA-3,600 flag day", PALW_T12_TIR_FENCE2_FENCES_V1),
        ("decode rules", PALW_T12_DECODE_RULES_FENCES_V1),
    ] {
        assert!(list.iter().all(|f| f.name != "palw_model_court_window"), "the {what} list does not carry the window");
    }
}

/// The delta entry's borsh discriminant is its position: RFC-0003's `GenClass` holds 90, the window's entry is the
/// last of the enum (100). The state module's pinned-discriminant test checks every position; this one names the
/// two that were once the same number.
#[test]
fn the_window_entry_sits_at_the_end_of_the_delta_enum_and_the_generative_class_keeps_ninety() {
    let key = Hash64::from_bytes([0x51; 64]);
    let window = PalwDeltaEntryV2::ClassCourtWindow { key, old: None, new: Some(9_000) };
    assert_eq!(borsh::to_vec(&window).expect("serialises")[0], 100, "ClassCourtWindow is the last delta entry");
    let gen_class = PalwDeltaEntryV2::GenClass { key, old: None, new: None };
    assert_eq!(borsh::to_vec(&gen_class).expect("serialises")[0], 90, "GenClass keeps its allocated position");
}
