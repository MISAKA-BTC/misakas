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
//! **testnet-12's IR flag day** (`PALW_T12_TIR_FLAG_DAY_FENCES_V1`, `palw_tir_v1` at DAA 2,000, the
//! user's decision of 2026-09-28) arms the one fence on the three testnet-12 rows that follow the
//! shipped assembly — `from(testnet-12)`, `palw_t12_shipped_params` and the salted drill. Those rows are
//! pinned here with that ONE list set back to dormant (at int-7's values still: the list is all the IR
//! release moves), beside the new `palw_t12_release_v3_params` (int-7's shipped ids); the armed rows are
//! held against them in `every_armed_row_is_its_pinned_row_with_the_ir_flag_day_at_2000` and, fence by
//! fence, in `palw_tir_flag_day_t12.rs`. The ids the armed rows carry are `t12-repin.sh`'s to pin.
//!
//! If a Phase F change turns this file red, the change is not dormant. A change that ARMS a Phase F
//! fence on a preset re-pins the moved row here, in the commit that arms it.

use kaspa_consensus_core::config::drill::{PALW_DRILL_SALT_LEN_V1, PalwDrillSaltV1};
use kaspa_consensus_core::config::params::{
    DEVNET_PARAMS, ForkActivation, MAINNET_PARAMS, PALW_T12_INT11_FLAG_DAY_DAA, PALW_T12_MODEL_COURT_WINDOW_FENCES_V1, PALW_T12_TIR_FENCE2_DAA,
    PALW_T12_TIR_FENCE2_FENCES_V1, PALW_T12_TIR_FLAG_DAY_DAA, PALW_T12_TIR_FLAG_DAY_FENCES_V1, Params, SIMNET_PARAMS, TESTNET_PARAMS,
    TESTNET11_PARAMS,
    devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params, palw_t12_drill_params_v1, palw_t12_launch_params_v1,
    palw_t12_arm_int11_flag_day_at_v1, palw_t12_release_v1_params, palw_t12_release_v2_params, palw_t12_release_v3_params,
    palw_t12_shipped_params,
};
use kaspa_consensus_core::network::{NetworkId, NetworkType};

/// A fixed drill salt, `7a1f00…00a5`: the drill's ids are a function of its salt, so the pin needs one.
/// Built from bytes (a quoted 64-hex literal in this file would read to `t12-repin.sh` as a pin).
fn drill_salt() -> PalwDrillSaltV1 {
    let mut bytes = [0u8; PALW_DRILL_SALT_LEN_V1];
    bytes[0] = 0x7a;
    bytes[1] = 0x1f;
    bytes[PALW_DRILL_SALT_LEN_V1 - 1] = 0xa5;
    PalwDrillSaltV1::from_bytes(bytes).expect("the pinned salt is a legal salt")
}

/// The key `t12-repin.sh` harvests a row's id under: `tirdorm.<slug>.<id>`, the slug the row name's
/// ASCII letters and digits, lowercased, every other run one `_` (`scripts/t12_repin.py`, `_tirdorm_slug`).
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

/// `p` with testnet-12's IR flag day set back to dormant (its one list, through each entry's `set`) — and the
/// DAA-3,600 flag day's list with it (`palw_tir_fence2`, plus the court window, which is dormant everywhere) and the int-11 flag
/// day's (the generative and improvement lists need the IR fence below them), which build on the IR fence and which int-7, the
/// pins' release, did not carry either.
fn without_the_ir_flag_day(mut p: Params) -> Params {
    palw_t12_arm_int11_flag_day_at_v1(&mut p, None);
    for f in PALW_T12_TIR_FENCE2_FENCES_V1.iter().chain(PALW_T12_MODEL_COURT_WINDOW_FENCES_V1).chain(PALW_T12_TIR_FLAG_DAY_FENCES_V1) {
        (f.set)(&mut p, None);
    }
    p
}

/// The three testnet-12 rows the IR flag day arms, as they ship.
fn armed_rows() -> [(&'static str, Params); 3] {
    let salt = drill_salt();
    [
        ("from(testnet-12)", Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))),
        ("palw_t12_shipped_params", palw_t12_shipped_params()),
        ("palw_t12_drill_params_v1", palw_t12_drill_params_v1(&salt)),
    ]
}

fn rulesets() -> Vec<(&'static str, Params)> {
    let salt = drill_salt();
    vec![
        ("MAINNET_PARAMS", MAINNET_PARAMS),
        ("TESTNET_PARAMS", TESTNET_PARAMS),
        ("TESTNET11_PARAMS", TESTNET11_PARAMS),
        ("DEVNET_PARAMS", DEVNET_PARAMS),
        ("SIMNET_PARAMS", SIMNET_PARAMS),
        ("from(mainnet)", Params::from(NetworkId::new(NetworkType::Mainnet))),
        ("from(testnet-10)", Params::from(NetworkId::with_suffix(NetworkType::Testnet, 10))),
        ("from(testnet-11)", Params::from(NetworkId::with_suffix(NetworkType::Testnet, 11))),
        ("from(testnet-12) − IR flag day", without_the_ir_flag_day(Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12)))),
        ("from(devnet)", Params::from(NetworkId::new(NetworkType::Devnet))),
        ("from(simnet)", Params::from(NetworkId::new(NetworkType::Simnet))),
        ("mainnet_shipped_params", mainnet_shipped_params()),
        ("devnet_shipped_params", devnet_shipped_params()),
        ("palw_rc_shipped_params", palw_rc_shipped_params()),
        ("palw_t12_shipped_params − IR flag day", without_the_ir_flag_day(palw_t12_shipped_params())),
        ("palw_t12_launch_params_v1", palw_t12_launch_params_v1()),
        ("palw_t12_release_v1_params", palw_t12_release_v1_params()),
        ("palw_t12_release_v2_params", palw_t12_release_v2_params()),
        ("palw_t12_release_v3_params", palw_t12_release_v3_params()),
        ("palw_t12_drill_params_v1 − IR flag day", without_the_ir_flag_day(palw_t12_drill_params_v1(&salt))),
    ]
}

/// `(ruleset, consensus_params_id, consensus_identity_id, consensus_schedule_id)`, measured on the tree
/// before the first Phase F field (see the module doc). Never transcribed: printed by this test.
const PINS: &[(&str, &str, &str, &str)] = &[
    (
        // re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was eb866c61…, 7819e5ed…, a1ed7ff0…)
        "MAINNET_PARAMS",
        "0befd2fa1da052fb88ef8a04cc1bd6412ea560371aaec606790194dee41ac3fa",
        "db7f8f9869a612c6e90ff8a01806569a0fe7391cfa8226c3c3a89680ae83d277",
        "7b8f4090c31d524312f1b6d8a91d0e47544b031a62560d90b9a1bc65b709e7c6",
    ),
    (
        // re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was 3de9ba33…, 22307d0c…, 7ea3296f…)
        "TESTNET_PARAMS",
        "3cbf3945aaa0b2f4e195ef7e2229e0d71690ed5bcc561da774e274caf8b1fa45",
        "e2fb02a20a1c22c740f1ed4a780c805b8b49fdd68139d5b7490558fe514feb16",
        "e86052cdfea252fde7c2277087aeda5566c41e9161f083705e99bf3180653ff6",
    ),
    (
        // re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was 03b8564e…, 929a3367…, cb379fa0…)
        "TESTNET11_PARAMS",
        "a04d00987327c9e5b051baddea37b014d46344efad2a67632816553d83e93266",
        "8c9f6252df37aded198710cf68e26ea6fa4803e3a17656b163276e4c51af8180",
        "761a82012a0781e618139c5fa25ae9b48821317f16af45b43ec0d7cec910f4fa",
    ),
    (
        // re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was 80cbefe8…, 80cbefe8…, edd80c01…)
        "DEVNET_PARAMS",
        "c5c67b0357c2bbb89f15477f09373c6a13680090ef983ad0cd1d5f9e7bad5964",
        "c5c67b0357c2bbb89f15477f09373c6a13680090ef983ad0cd1d5f9e7bad5964",
        "d82068a0080c47f45f376c81c621ec002aa8541a22c49e9d0a373e263c39c680",
    ),
    (
        // re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was 63238ba1…, 63238ba1…, f981edc9…)
        "SIMNET_PARAMS",
        "5fca6e0c631ea9327fc75e1bd5463b2d78814c3a9937f2d6cea282bdfc05ef56",
        "5fca6e0c631ea9327fc75e1bd5463b2d78814c3a9937f2d6cea282bdfc05ef56",
        "04ccb8fe229ae341d1c2b171b4bd7a578fcc326c3ecbf5f017ccaee878f3aee5",
    ),
    (
        // re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was eb866c61…, 7819e5ed…, a1ed7ff0…)
        "from(mainnet)",
        "0befd2fa1da052fb88ef8a04cc1bd6412ea560371aaec606790194dee41ac3fa",
        "db7f8f9869a612c6e90ff8a01806569a0fe7391cfa8226c3c3a89680ae83d277",
        "7b8f4090c31d524312f1b6d8a91d0e47544b031a62560d90b9a1bc65b709e7c6",
    ),
    (
        // re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was 0d9cf361…, 2c3067c0…, 7ea3296f…)
        "from(testnet-10)",
        "a4153f537905eebd364d43742ccfc6e7264149cce61392f827dc0d45aee4d08d",
        "c9adc147ada258574c0926534fae5dd4eb82b5a2795bdc5300affd74abfe0380",
        "e86052cdfea252fde7c2277087aeda5566c41e9161f083705e99bf3180653ff6",
    ),
    (
        // re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was bd633ce9…, 44cb8fd7…, 5a1d8d56…)
        "from(testnet-11)",
        "0c2c022a11572d73779fc0610cf65d4147feb499a9b58472901d3d73ecfc1f71",
        "bf4cc0b3e2fa208a18cce48d0c33db74e48adf275a069687b534856411f1c83e",
        "f023f74befcdaf732f213616c79fd94782d7728f835151bbfd986498f962bd64",
    ),
    (
        // re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was 770fb822…, 5de80e64…, 9410712f…)
        "from(testnet-12) − IR flag day",
        "5f6c86f85e688c487a8d7e529ad669747f8e479457f774e33e5a4dc7e300d76c",
        "92042711856b239713d0376b7466422e561618fc51e115e1557be34b078e5acd",
        "2549f73158cfeeeb99aa9422fbe7765e987d1475dbb0ce4a5579b93402f7571f",
    ),
    (
        // re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was 7a27f341…, 7a27f341…, edd80c01…)
        "from(devnet)",
        "f17c6555a617736bd13e90eaab80974223149c211d08af516008bd6500010b1f",
        "f17c6555a617736bd13e90eaab80974223149c211d08af516008bd6500010b1f",
        "d82068a0080c47f45f376c81c621ec002aa8541a22c49e9d0a373e263c39c680",
    ),
    (
        // re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was 63238ba1…, 63238ba1…, f981edc9…)
        "from(simnet)",
        "5fca6e0c631ea9327fc75e1bd5463b2d78814c3a9937f2d6cea282bdfc05ef56",
        "5fca6e0c631ea9327fc75e1bd5463b2d78814c3a9937f2d6cea282bdfc05ef56",
        "04ccb8fe229ae341d1c2b171b4bd7a578fcc326c3ecbf5f017ccaee878f3aee5",
    ),
    (
        // re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was eb866c61…, 7819e5ed…, a1ed7ff0…)
        "mainnet_shipped_params",
        "0befd2fa1da052fb88ef8a04cc1bd6412ea560371aaec606790194dee41ac3fa",
        "db7f8f9869a612c6e90ff8a01806569a0fe7391cfa8226c3c3a89680ae83d277",
        "7b8f4090c31d524312f1b6d8a91d0e47544b031a62560d90b9a1bc65b709e7c6",
    ),
    (
        // re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was 7a27f341…, 7a27f341…, edd80c01…)
        "devnet_shipped_params",
        "f17c6555a617736bd13e90eaab80974223149c211d08af516008bd6500010b1f",
        "f17c6555a617736bd13e90eaab80974223149c211d08af516008bd6500010b1f",
        "d82068a0080c47f45f376c81c621ec002aa8541a22c49e9d0a373e263c39c680",
    ),
    (
        // re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was bd633ce9…, 44cb8fd7…, 5a1d8d56…)
        "palw_rc_shipped_params",
        "0c2c022a11572d73779fc0610cf65d4147feb499a9b58472901d3d73ecfc1f71",
        "bf4cc0b3e2fa208a18cce48d0c33db74e48adf275a069687b534856411f1c83e",
        "f023f74befcdaf732f213616c79fd94782d7728f835151bbfd986498f962bd64",
    ),
    (
        // re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was 770fb822…, 5de80e64…, 9410712f…)
        "palw_t12_shipped_params − IR flag day",
        "5f6c86f85e688c487a8d7e529ad669747f8e479457f774e33e5a4dc7e300d76c",
        "92042711856b239713d0376b7466422e561618fc51e115e1557be34b078e5acd",
        "2549f73158cfeeeb99aa9422fbe7765e987d1475dbb0ce4a5579b93402f7571f",
    ),
    (
        // re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was b8564b88…, 5de80e64…, 93da24cc…)
        "palw_t12_launch_params_v1",
        "6187399e51559e7f4bb0ef7935915478ff9b0ea51c8c8202c32a76967c1a2025",
        "92042711856b239713d0376b7466422e561618fc51e115e1557be34b078e5acd",
        "04617b53173b09f997c87e77f1b4d3b2d326ddc5a954373a1248e8782b79c6bf",
    ),
    (
        // re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was dbbc9104…, 5de80e64…, 7c652212…)
        "palw_t12_release_v1_params",
        "41f0b7851b523ec378448d1f2d5fc2cc1562092a73ebfeab98c6999f65a75db3",
        "92042711856b239713d0376b7466422e561618fc51e115e1557be34b078e5acd",
        "6c94fae224479c3e6cdcb3a7ac542749b8903ad457af363a6ca7d75e2cfb2416",
    ),
    (
        // re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was 24e1aec3…, 5de80e64…, d263d7f2…)
        "palw_t12_release_v2_params",
        "deca53d0b0ecffed5fffbdb0df72ae035631c6743a42f3262a101b146f5a4784",
        "92042711856b239713d0376b7466422e561618fc51e115e1557be34b078e5acd",
        "7804951885d60a29bfb6aa5b8f7bc2af6c8b4692636f019bc1320a7f4aad3385",
    ),
    (
        // re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was 770fb822…, 5de80e64…, 9410712f…)
        "palw_t12_release_v3_params",
        "5f6c86f85e688c487a8d7e529ad669747f8e479457f774e33e5a4dc7e300d76c",
        "92042711856b239713d0376b7466422e561618fc51e115e1557be34b078e5acd",
        "2549f73158cfeeeb99aa9422fbe7765e987d1475dbb0ce4a5579b93402f7571f",
    ),
    (
        // re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was 31b913ef…, 2254a5ae…, 9410712f…)
        "palw_t12_drill_params_v1 − IR flag day",
        "76782050bfd25ee908e152f3235d862df998f2ce17193b29d686987f4bdf1988",
        "969d6f24489ef37fab96801978ae0907c7fbbc3f55d0ae38a7dd4995fe057413",
        "2549f73158cfeeeb99aa9422fbe7765e987d1475dbb0ce4a5579b93402f7571f",
    ),
];

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

#[test]
fn every_shipped_ruleset_keeps_its_three_ids() {
    let measured: Vec<(&str, (String, String, String))> = rulesets().iter().map(|(name, p)| (*name, ids(p))).collect();
    // What `t12-repin.sh` harvests (printed before any assertion, so a drift still reports its value).
    for (name, (params, identity, schedule)) in &measured {
        let key = slug(name);
        println!("REPIN tirdorm.{key}.params_id {params}");
        println!("REPIN tirdorm.{key}.identity_id {identity}");
        println!("REPIN tirdorm.{key}.schedule_id {schedule}");
    }
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

/// **The two flag days are the whole of the three armed rows' move**: each is its pinned row with the IR
/// list at DAA 2,000 and the DAA-3,600 list at DAA 3,600 — to the three ids — and not its pinned row (the
/// flag days are real).
#[test]
fn every_armed_row_is_its_pinned_row_with_the_ir_flag_day_at_2000() {
    let at = PALW_T12_TIR_FLAG_DAY_DAA.expect("testnet-12's IR flag day has a height");
    let day = PALW_T12_TIR_FENCE2_DAA.expect("testnet-12's DAA-3,600 flag day has a height");
    for (name, armed) in armed_rows() {
        let dormant = without_the_ir_flag_day(armed.clone());
        let mut rearmed = dormant.clone();
        for f in PALW_T12_TIR_FLAG_DAY_FENCES_V1 {
            (f.set)(&mut rearmed, Some(ForkActivation::new(at)));
        }
        for f in PALW_T12_TIR_FENCE2_FENCES_V1 {
            (f.set)(&mut rearmed, Some(ForkActivation::new(day)));
        }
        // …and the int-11 flag day, which builds on both and arms on every testnet-12 row at its own height.
        palw_t12_arm_int11_flag_day_at_v1(&mut rearmed, PALW_T12_INT11_FLAG_DAY_DAA);
        assert_eq!(
            ids(&armed),
            ids(&rearmed),
            "{name}: the pinned row with the IR flag day at {at}, the DAA-3,600 flag day at {day} and the int-11 flag day"
        );
        assert_ne!(ids(&armed).0, ids(&dormant).0, "{name}: the flag day moves the params id");
        assert_ne!(ids(&armed).2, ids(&dormant).2, "{name}: and the schedule id");
        assert_eq!(ids(&armed).1, ids(&dormant).1, "{name}: not the identity");
    }
}
