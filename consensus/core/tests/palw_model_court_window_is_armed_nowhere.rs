//! **`palw_model_court_window` is armed on no network but testnet-12 as shipped, at the int-13 flag day's one height (DAA 9,000), and
//! its place in the delta enum is the END's** — the integration's pins for the release line's per-model court window (`rcore/int-10`,
//! merged into `rfc4/int` on 2026-10-01; armed by the int-13 list on 2026-10-08).
//!
//! The window was a dormant fence: testnet-12 charges every class the held clock, under which the window the
//! fence derives is the network's for every admissible class and no admission verdict moves
//! (`palw_t12_court_window_changes_no_admission.rs`), so the DAA-3,600 flag day is `palw_tir_fence2` alone and the int-11 / int-12
//! releases (the fleet's) never armed it. The int-13 flag day (`palw_t12_flag_day_9000.rs`) arms it with the other code-change-only
//! fences — a no-op for every verdict, a stated rule in the ruleset. The delta entry that records a window
//! (`PalwDeltaEntryV2::ClassCourtWindow`) was declared at position 90 on the release line; RFC-0003's `GenClass` holds 90
//! (spec 17 §17.0), so on this line the entry sits at the END of the enum — 100 — and the int-12 release the fleet runs already has it
//! there. That move is safe only while no stored delta carries the variant, which holds on every deployed chain until DAA 9,000 and is
//! the same position on every node that can cross it (an int-10 node is refused at the fork id long before); so the facts are pinned
//! together, here: a ruleset other than testnet-12 as shipped that arms the window, or a flag-day list other than the int-13 one that
//! names it with a height, fails this file before the position can matter.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::drill::{PALW_DRILL_SALT_LEN_V1, PalwDrillSaltV1};
use kaspa_consensus_core::config::params::{
    DEVNET_PARAMS, ForkActivation, MAINNET_PARAMS, PALW_T12_CAPACITY_FENCES_V1, PALW_T12_CAPACITY_RHO10_FENCES_V1, PALW_T12_DECODE_RULES_FENCES_V1,
    PALW_T12_INT11_FENCES_V1, PALW_T12_INT13_DAA, PALW_T12_INT13_FENCES_V1, PALW_T12_MODEL_COURT_WINDOW_DAA,
    PALW_T12_MODEL_COURT_WINDOW_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V1,
    PALW_T12_POST_LAUNCH_FENCES_V2, PALW_T12_POST_LAUNCH_FENCES_V3, PALW_T12_TIR_FENCE2_FENCES_V1, PALW_T12_TIR_FLAG_DAY_FENCES_V1,
    Params, SIMNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS, devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params,
    palw_t12_drill_params_v1, palw_t12_launch_params_v1, palw_t12_release_v1_params, palw_t12_release_v2_params,
    palw_t12_release_v3_params, palw_t12_release_v4_params, palw_t12_release_v5_params, palw_t12_release_v6_params,
    palw_t12_shipped_params,
};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_state_v2::PalwDeltaEntryV2;

fn salt() -> PalwDrillSaltV1 {
    PalwDrillSaltV1::from_bytes([0x4d; PALW_DRILL_SALT_LEN_V1]).expect("a legal salt")
}

/// Every ruleset a node can run, as it ships (the baselines of testnet-12's releases included) — **but testnet-12 as shipped and its
/// drill**, which carry the int-13 list ([`shipped_testnet_12`]).
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
        ("from(devnet)", Params::from(NetworkId::new(NetworkType::Devnet))),
        ("from(simnet)", Params::from(NetworkId::new(NetworkType::Simnet))),
        ("mainnet_shipped_params", mainnet_shipped_params()),
        ("devnet_shipped_params", devnet_shipped_params()),
        ("palw_rc_shipped_params", palw_rc_shipped_params()),
        ("palw_t12_launch_params_v1", palw_t12_launch_params_v1()),
        ("palw_t12_release_v1_params", palw_t12_release_v1_params()),
        ("palw_t12_release_v2_params", palw_t12_release_v2_params()),
        ("palw_t12_release_v3_params", palw_t12_release_v3_params()),
        ("palw_t12_release_v4_params", palw_t12_release_v4_params()),
        ("palw_t12_release_v5_params", palw_t12_release_v5_params()),
        ("palw_t12_release_v6_params", palw_t12_release_v6_params()),
    ]
}

/// The rulesets that ship the int-13 list: testnet-12 by its three doors and a salted drill chain (a drill drills what ships).
fn shipped_testnet_12() -> Vec<(&'static str, Params)> {
    vec![
        ("from(testnet-12)", Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))),
        ("palw_t12_shipped_params", palw_t12_shipped_params()),
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

/// **Testnet-12 as shipped arms the window at the int-13 flag day's height, in force from it and not below, and validates.**
#[test]
fn testnet_12_as_shipped_arms_the_model_court_window_at_the_int13_height_only() {
    let at = PALW_T12_INT13_DAA.expect("the int-13 flag day");
    for (name, p) in shipped_testnet_12() {
        assert_eq!(p.palw_model_court_window, Some(ForkActivation::new(at)), "{name}: armed at the int-13 height");
        assert!(!p.palw_model_court_window_active_at(at - 1) && p.palw_model_court_window_active_at(at), "{name}");
        assert!(p.palw_fences_v1().contains(&("palw_model_court_window", Some(ForkActivation::new(at)))), "{name}");
        p.validate_palw_model_court_window().unwrap_or_else(|e| panic!("{name}: the window validates: {e:?}"));
        p.validate_palw_v2().unwrap_or_else(|e| panic!("{name}: the ruleset validates: {e:?}"));
    }
}

#[test]
fn its_testnet_12_height_is_none_and_no_flag_day_list_but_the_int13_one_and_its_own_names_it() {
    assert_eq!(PALW_T12_MODEL_COURT_WINDOW_DAA, None, "its own list names no height: the int-13 list arms its entry");
    assert!(PALW_T12_INT13_FENCES_V1.iter().any(|f| f.name == "palw_model_court_window"), "the int-13 list carries the window");
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
        ("int-11 flag day", PALW_T12_INT11_FENCES_V1),
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
