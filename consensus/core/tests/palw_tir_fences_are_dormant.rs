//! **RFC-0002 Phase F: the IR fences leave every shipped ruleset byte for byte where it was.**
//!
//! `palw_tir_v1` (and every later Phase F fence) ships dormant: `None` on every preset, hashed
//! Some-only into `consensus_params_id` and `consensus_schedule_id`, and collapsed whole from
//! `Some(never())` in the identity's normaliser. That claim is only worth anything pinned to the
//! numbers, so this file pins the three ids of every ruleset a node can run — the const presets,
//! every `Params::from(NetworkId)` door (which wraps `with_registered_models`), the testnet-11 and
//! testnet-12 releases the fleet runs, the testnet-12 releases that a node which has not taken a
//! flag day still runs, and a salted testnet-12 drill — at the values the tree had BEFORE the first
//! Phase F field existed (`tir/phase-f` at `ebec854f4`), with the three testnet-12 rows that
//! `rcore/int-7` (`7aba8dd57`, the capacity flag day moved to DAA 1,700) moved taken at int-7's
//! values: the shipped rows at int-7's own pins (params `770fb822…`, schedule `9410712f…`), and the
//! salted drill (which int-7 does not pin) at its schedule `9410712f…` and params `31b913ef…` — the
//! drill's move is the same flag day's (identities unchanged throughout).
//!
//! If a Phase F change turns this file red, the change is not dormant. A change that ARMS a Phase F
//! fence on a preset re-pins the moved row here, in the commit that arms it.

use kaspa_consensus_core::config::drill::PalwDrillSaltV1;
use kaspa_consensus_core::config::params::{
    DEVNET_PARAMS, MAINNET_PARAMS, Params, SIMNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS, devnet_shipped_params,
    mainnet_shipped_params, palw_rc_shipped_params, palw_t12_drill_params_v1, palw_t12_launch_params_v1, palw_t12_release_v1_params,
    palw_t12_release_v2_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::network::{NetworkId, NetworkType};

/// A fixed drill salt: the drill's ids are a function of its salt, so the pin needs one.
const DRILL_SALT_HEX: &str = "7a1f0000000000000000000000000000000000000000000000000000000000a5";

fn rulesets() -> Vec<(&'static str, Params)> {
    let salt = PalwDrillSaltV1::from_hex(DRILL_SALT_HEX).expect("the pinned salt is a legal salt");
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
        ("palw_t12_drill_params_v1", palw_t12_drill_params_v1(&salt)),
    ]
}

/// `(ruleset, consensus_params_id, consensus_identity_id, consensus_schedule_id)`, measured on the tree
/// before the first Phase F field (see the module doc). Never transcribed: printed by this test.
const PINS: &[(&str, &str, &str, &str)] = &[
    (
        "MAINNET_PARAMS",
        "eb866c61ca1a8ab58108be6cd1f39f951b582123472545575a5c7dbe0f1e5aa5",
        "7819e5ed2b3df50b3303df3df2f0fec7677ddcb37ed55f1d43455a37ecd9c9a8",
        "a1ed7ff07231b84c51d9dc1013a8047ea3efb012bfc9daa36d5dd623709807e4",
    ),
    (
        "TESTNET_PARAMS",
        "3de9ba33581b12b8478a1d476a4e58889ed21ddb91dc24cc00706f49dd5b5b97",
        "22307d0c46b75110aea7967d16708fdf50a69f31450f7ed44d7309574395972a",
        "7ea3296f36fc827898aa6560a1f71159652a12a7dd69105178564b9f7723d5d0",
    ),
    (
        "TESTNET11_PARAMS",
        "03b8564ece85e237b9252f390d0a73001bd67d02aee26418032a716e94057beb",
        "929a3367d45faebd8c23927923abf10f91e6025cfb4b52a1ecb8768ade2d0e78",
        "cb379fa021867dab89971e6f99a3469a6d044084d688113fa0b8e20f93af04d1",
    ),
    (
        "DEVNET_PARAMS",
        "80cbefe885c82921843f431a0d0ca85d93bec6d6cd74f0bf3413c2fd3827be52",
        "80cbefe885c82921843f431a0d0ca85d93bec6d6cd74f0bf3413c2fd3827be52",
        "edd80c01c791d225d602b9136f539f4dfeb506ba1b3071b177b0d873a661142f",
    ),
    (
        "SIMNET_PARAMS",
        "63238ba10766c824ff6915484829b01eb4fc3c105665a7db2cf6b175bf870dfd",
        "63238ba10766c824ff6915484829b01eb4fc3c105665a7db2cf6b175bf870dfd",
        "f981edc9bff1b71ae46abf030c0c56c40beafabeeae78d8435dd502ad6191f69",
    ),
    (
        "from(mainnet)",
        "eb866c61ca1a8ab58108be6cd1f39f951b582123472545575a5c7dbe0f1e5aa5",
        "7819e5ed2b3df50b3303df3df2f0fec7677ddcb37ed55f1d43455a37ecd9c9a8",
        "a1ed7ff07231b84c51d9dc1013a8047ea3efb012bfc9daa36d5dd623709807e4",
    ),
    (
        "from(testnet-10)",
        "0d9cf361e02dea6d9e873014ff5e414c2e8e6879e8d705cde6c924a3a3f8dd88",
        "2c3067c01e76ac32f0bd3f78ba49cc25f2ea771af4eb848e5aaad17f825a0a64",
        "7ea3296f36fc827898aa6560a1f71159652a12a7dd69105178564b9f7723d5d0",
    ),
    (
        "from(testnet-11)",
        "bd633ce933974d4134676efbdaf46b269dc2fb78f007e0907479aabd4d743f29",
        "44cb8fd729e9575a6e3b1e72c466b8abce4b9ecd81bb556685c9ba225487117f",
        "5a1d8d5679e0e8d7e9022255668fd5d4b3e4c8a6c367acf3882c6a3d480d8b64",
    ),
    (
        "from(testnet-12)",
        "770fb822f6e82c72e31d0c37375400a29047b9430b666018d4a8937d14a95fb9",
        "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
        "9410712f252cbb8aede7f1e337dc5f46c30f5c01914d0b8f2f714a26e69f1906",
    ),
    (
        "from(devnet)",
        "7a27f341e49902ebb5e15ea79a45806fbd37b65daaddf8f0a5a10a15f9bfd4a8",
        "7a27f341e49902ebb5e15ea79a45806fbd37b65daaddf8f0a5a10a15f9bfd4a8",
        "edd80c01c791d225d602b9136f539f4dfeb506ba1b3071b177b0d873a661142f",
    ),
    (
        "from(simnet)",
        "63238ba10766c824ff6915484829b01eb4fc3c105665a7db2cf6b175bf870dfd",
        "63238ba10766c824ff6915484829b01eb4fc3c105665a7db2cf6b175bf870dfd",
        "f981edc9bff1b71ae46abf030c0c56c40beafabeeae78d8435dd502ad6191f69",
    ),
    (
        "mainnet_shipped_params",
        "eb866c61ca1a8ab58108be6cd1f39f951b582123472545575a5c7dbe0f1e5aa5",
        "7819e5ed2b3df50b3303df3df2f0fec7677ddcb37ed55f1d43455a37ecd9c9a8",
        "a1ed7ff07231b84c51d9dc1013a8047ea3efb012bfc9daa36d5dd623709807e4",
    ),
    (
        "devnet_shipped_params",
        "7a27f341e49902ebb5e15ea79a45806fbd37b65daaddf8f0a5a10a15f9bfd4a8",
        "7a27f341e49902ebb5e15ea79a45806fbd37b65daaddf8f0a5a10a15f9bfd4a8",
        "edd80c01c791d225d602b9136f539f4dfeb506ba1b3071b177b0d873a661142f",
    ),
    (
        "palw_rc_shipped_params",
        "bd633ce933974d4134676efbdaf46b269dc2fb78f007e0907479aabd4d743f29",
        "44cb8fd729e9575a6e3b1e72c466b8abce4b9ecd81bb556685c9ba225487117f",
        "5a1d8d5679e0e8d7e9022255668fd5d4b3e4c8a6c367acf3882c6a3d480d8b64",
    ),
    (
        "palw_t12_shipped_params",
        "770fb822f6e82c72e31d0c37375400a29047b9430b666018d4a8937d14a95fb9",
        "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
        "9410712f252cbb8aede7f1e337dc5f46c30f5c01914d0b8f2f714a26e69f1906",
    ),
    (
        "palw_t12_launch_params_v1",
        "b8564b888e55bb5f797e708a3f65e7cd122065123a3ab09cbeb8d10c98715d8f",
        "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
        "93da24cc60f7a77e63c43106e96298d2979644a3fc0c3f82529c8849333127fd",
    ),
    (
        "palw_t12_release_v1_params",
        "dbbc9104a2ee754f0f053a6e1614118979fd2c3dc87cbe6bffcf6dcaf4bd59c9",
        "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
        "7c652212ab5337bda9508deeee2d2e119331856fce0bd27897f19dd66e552397",
    ),
    (
        "palw_t12_release_v2_params",
        "24e1aec3e9a102fa40d559cd28005ad5944c32caa485d685bed65c52e4c056ff",
        "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
        "d263d7f2971f4e20b57b26d7b7428bd8f9346c3728bbb6927341d8b36b0c1c3a",
    ),
    (
        "palw_t12_drill_params_v1",
        "31b913ef15a1617472bc1764978763e530c021b116c98a77e6e5f6f47769faa4",
        "2254a5ae75cc1ed6fa70cc6f9a57281e7ab36aa8f80a820a545b81dad21549c6",
        "9410712f252cbb8aede7f1e337dc5f46c30f5c01914d0b8f2f714a26e69f1906",
    ),
];

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

#[test]
fn every_shipped_ruleset_keeps_its_three_ids() {
    let measured: Vec<(&str, (String, String, String))> = rulesets().iter().map(|(name, p)| (*name, ids(p))).collect();
    let listing: String = measured
        .iter()
        .map(|(name, (params, identity, schedule))| format!("    (\"{name}\", \"{params}\", \"{identity}\", \"{schedule}\"),\n"))
        .collect();
    assert_eq!(PINS.len(), measured.len(), "one pin per ruleset; measured:\n{listing}");
    let moved: Vec<String> = measured
        .iter()
        .zip(PINS)
        .filter_map(|((name, (params, identity, schedule)), (pin_name, pin_params, pin_identity, pin_schedule))| {
            assert_eq!(name, pin_name, "the pins are in the rulesets' order");
            let same = params == pin_params && identity == pin_identity && schedule == pin_schedule;
            (!same).then(|| {
                format!(
                    "  {name}: params {} identity {} schedule {}",
                    if params == pin_params { "same" } else { "MOVED" },
                    if identity == pin_identity { "same" } else { "MOVED" },
                    if schedule == pin_schedule { "same" } else { "MOVED" }
                )
            })
        })
        .collect();
    assert!(moved.is_empty(), "a Phase F change moved a shipped ruleset:\n{}\nmeasured:\n{listing}", moved.join("\n"));
}
