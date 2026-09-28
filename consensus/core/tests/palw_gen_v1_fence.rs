//! **RFC-0003: the `palw_gen_v1` fence** — dormant on every network, fingerprinted with its value
//! where armed, invisible to the identity until it fires, named by the fork id, armed by the drill
//! mover alone, and refused by `validate_palw_v2` when it names another build's primitive,
//! randomness or output set, another program or court version, ceilings past the format's caps, or
//! prerequisites the ruleset lacks.
//!
//! **Byte identity.** The field is hashed Some-only and collapsed whole from `Some(never())`, so a
//! `None` writes nothing: every shipped ruleset's three ids are pinned — measured before this field
//! existed — in `palw_tir_fences_are_dormant.rs` (every preset, every `Params::from` door, every
//! testnet-12 release and a salted drill) and `palw_tir_flag_day_t12.rs` (the armed testnet-12 rows),
//! and this change leaves those pins green. This file adds no pin (a quoted id here would be one
//! `t12-repin.sh` does not know); it checks the field itself.
//!
//! The armed cases below run over testnet-12 as shipped (`palw_t12_shipped_params`: `palw_tir_v1` at
//! DAA 2,000, the generative fence's prerequisite) at a height no fence uses.

use kaspa_consensus_core::config::drill::{PALW_DRILL_SALT_LEN_V1, PalwDrillSaltV1, palw_drill_gen_fence_at_v1};
use kaspa_consensus_core::config::params::{
    DEVNET_PARAMS, ForkActivation, MAINNET_PARAMS, PALW_T12_CAPACITY_FENCES_V1, PALW_T12_CAPACITY_RHO10_FENCES_V1,
    PALW_T12_POST_LAUNCH_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V2, PALW_T12_POST_LAUNCH_FENCES_V3, PALW_T12_TIR_FLAG_DAY_FENCES_V1,
    Params, SIMNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS, devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params,
    palw_t12_drill_params_v1, palw_t12_launch_params_v1, palw_t12_release_v1_params, palw_t12_release_v2_params,
    palw_t12_release_v3_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::fork_id_gate_fences_v1;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_gen_v1::{
    PALW_DRILL_GEN_CEILINGS_V1, PALW_DRILL_GEN_FENCES_V1, PALW_DRILL_GEN_V1_ENTRY, PALW_GEN_COURT_VERSION_V1,
    PALW_GEN_PROGRAM_VERSION_V1, PalwGenFenceV1, PalwGenProfileCeilingsV1, PalwGenProfileV1, palw_gen_court_root_v1,
    palw_gen_output_set_id_v1, palw_gen_rand_set_id_v1,
};
use kaspa_consensus_core::palw_tir_v1::{PALW_TIR_COURT_VERSION_V1, palw_tir_court_root_v1, palw_tir_prim_set_id_v1};

/// A height no testnet-12 fence uses (above its IR flag day at 2,000).
const AT: u64 = 9_999_993;

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn salt() -> PalwDrillSaltV1 {
    PalwDrillSaltV1::from_bytes([0x3c; PALW_DRILL_SALT_LEN_V1]).expect("a legal salt")
}

/// Every ruleset a node can run, as it ships.
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
        ("palw_t12_drill_params_v1", palw_t12_drill_params_v1(&salt())),
    ]
}

fn t12_armed(at: ForkActivation) -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_gen_v1 = Some(PalwGenFenceV1::drill_v1(at));
    p
}

#[test]
fn every_shipped_ruleset_leaves_it_dormant_and_no_release_lists_it() {
    for (name, p) in rulesets() {
        assert!(p.palw_gen_v1.is_none(), "{name}: dormant");
        assert!(p.palw_gen_v1_fence().is_none() && !p.palw_gen_v1_active_at(u64::MAX), "{name}: the accessor agrees");
        assert!(p.palw_fences_v1().contains(&("palw_gen_v1", None)), "{name}: the exhaustive fence list names it");
        assert!(p.validate_palw_gen_v1().is_ok(), "{name}: nothing to refuse");
    }
    // No testnet-12 flag day carries it: the only list naming it is the drill's.
    for list in [
        PALW_T12_POST_LAUNCH_FENCES_V1,
        PALW_T12_POST_LAUNCH_FENCES_V2,
        PALW_T12_POST_LAUNCH_FENCES_V3,
        PALW_T12_CAPACITY_FENCES_V1,
        PALW_T12_CAPACITY_RHO10_FENCES_V1,
        PALW_T12_TIR_FLAG_DAY_FENCES_V1,
    ] {
        assert!(list.iter().all(|f| f.name != "palw_gen_v1"), "no testnet-12 release arms the generative fence");
    }
    let names: Vec<&str> = PALW_DRILL_GEN_FENCES_V1.iter().map(|f| f.name).collect();
    assert_eq!(names, ["palw_gen_v1"]);
}

#[test]
fn a_scheduled_fence_moves_the_ruleset_and_the_schedule_and_never_the_identity() {
    let base = ids(&palw_t12_shipped_params());
    let armed = t12_armed(ForkActivation::new(AT));
    armed.validate_palw_v2().unwrap_or_else(|e| panic!("testnet-12 past its IR flag day can arm it: {e}"));
    let moved = ids(&armed);
    assert_ne!(moved.0, base.0, "the ruleset a node announces names the fence and its value");
    assert_eq!(moved.1, base.1, "two builds that differ only about a FUTURE height stay peers");
    assert_ne!(moved.2, base.2, "the schedule reports it");
    assert!(armed.palw_gen_v1_fence().is_some() && !armed.palw_gen_v1_active_at(AT - 1) && armed.palw_gen_v1_active_at(AT));
}

#[test]
fn never_is_absence_for_the_identity_and_genesis_is_a_rule() {
    let base = palw_t12_shipped_params();
    let never = t12_armed(ForkActivation::never());
    assert_eq!(never.consensus_identity_id(), base.consensus_identity_id(), "Some(never()) collapses whole in the normaliser");
    never.validate_palw_v2().unwrap_or_else(|e| panic!("a dormant value validates: {e}"));
    assert!(!never.palw_gen_v1_active_at(u64::MAX - 1));
    let genesis = t12_armed(ForkActivation::always());
    assert_ne!(genesis.consensus_identity_id(), base.consensus_identity_id(), "a fence in force at genesis separates identities");
}

#[test]
fn the_value_is_fingerprinted_profile_by_profile() {
    let a = t12_armed(ForkActivation::new(AT));
    let with = |edit: &dyn Fn(&mut PalwGenFenceV1)| {
        let mut p = a.clone();
        let mut f = p.palw_gen_v1.unwrap();
        edit(&mut f);
        p.palw_gen_v1 = Some(f);
        p
    };
    for profile in PalwGenProfileV1::ALL {
        let b = with(&|f| {
            let c = match profile {
                PalwGenProfileV1::Image => &mut f.ceilings.image,
                PalwGenProfileV1::Embedding => &mut f.ceilings.embedding,
                PalwGenProfileV1::Audio => &mut f.ceilings.audio,
                PalwGenProfileV1::Video => &mut f.ceilings.video,
                PalwGenProfileV1::Text => &mut f.ceilings.text,
            };
            c.max_job_macs -= 1;
        });
        assert_ne!(a.consensus_params_id(), b.consensus_params_id(), "{profile:?}'s ceilings are part of the value");
        assert_ne!(a.consensus_schedule_id(), b.consensus_schedule_id(), "and the schedule report says so");
        assert_eq!(a.consensus_identity_id(), b.consensus_identity_id(), "while the height is in the future they peer");
    }
    // Two profiles' ceilings swapped are a different ruleset: each set is written under its profile.
    let swapped = with(&|f| {
        f.ceilings.image.max_stages = 3;
        f.ceilings.video.max_stages = 4;
    });
    let swapped_back = with(&|f| {
        f.ceilings.image.max_stages = 4;
        f.ceilings.video.max_stages = 3;
    });
    assert_ne!(swapped.consensus_params_id(), swapped_back.consensus_params_id());
}

#[test]
fn the_fork_id_gate_names_it_at_its_height() {
    let armed = t12_armed(ForkActivation::new(AT));
    assert!(fork_id_gate_fences_v1(&armed).contains(&AT), "past its height an un-upgraded node is refused");
    assert!(!fork_id_gate_fences_v1(&palw_t12_shipped_params()).contains(&AT));
    assert!(armed.palw_fences_v1().contains(&("palw_gen_v1", Some(ForkActivation::new(AT)))));
}

#[test]
fn validate_refuses_every_value_this_build_cannot_run() {
    let ok = t12_armed(ForkActivation::new(AT));
    assert!(ok.validate_palw_gen_v1().is_ok());
    let with = |edit: &dyn Fn(&mut PalwGenFenceV1)| {
        let mut p = ok.clone();
        let mut fence = p.palw_gen_v1.unwrap();
        edit(&mut fence);
        p.palw_gen_v1 = Some(fence);
        p.validate_palw_gen_v1()
    };
    let other = kaspa_consensus_core::Hash64::from_bytes([7u8; 64]);
    assert!(with(&|f| f.prim_set_id = other).is_err(), "another primitive set");
    assert!(with(&|f| f.rand_set_id = other).is_err(), "another randomness set");
    assert!(with(&|f| f.output_set_id = other).is_err(), "another output set");
    assert!(with(&|f| f.program_version = 1).is_err(), "version 1 programs are palw_tir_v1's");
    assert!(with(&|f| f.program_version = PALW_GEN_PROGRAM_VERSION_V1 + 1).is_err(), "a program version this build lacks");
    assert!(with(&|f| f.court_version = PALW_GEN_COURT_VERSION_V1 + 1).is_err(), "another court");
    let caps = PalwGenProfileCeilingsV1::FORMAT_CAPS_V1;
    assert!(with(&|f| f.ceilings.image.max_job_macs = 0).is_err(), "a zero ceiling admits nothing");
    assert!(with(&|f| f.ceilings.audio.max_stages = 0).is_err(), "no stage admits nothing");
    assert!(with(&|f| f.ceilings.video.max_stages = caps.max_stages + 1).is_err(), "past the pipeline's sixteen stages");
    assert!(
        with(&|f| f.ceilings.embedding.max_position_step_leaves = caps.max_position_step_leaves + 1).is_err(),
        "past a position's leaves"
    );
    assert!(with(&|f| f.ceilings.image.max_job_step_leaves = caps.max_job_step_leaves + 1).is_err(), "past the ladder");
    assert!(with(&|f| f.ceilings.image.max_job_cone_work = caps.max_job_cone_work + 1).is_err(), "past admission's work");
    assert!(with(&|f| f.ceilings.image.max_class_bytes = caps.max_class_bytes + 1).is_err(), "past the carriage's bytes");
    assert!(with(&|f| f.ceilings.image = caps).is_ok(), "the format's caps themselves are legal");
    assert!(with(&|f| f.activation = ForkActivation::never()).is_ok(), "a dormant value is never refused");

    let mut no_audit = ok.clone();
    no_audit.palw_audit_2026_09_11 = None;
    assert!(no_audit.validate_palw_gen_v1().is_err(), "without A-2 an older build fails the block that carries a generative object");
    let mut no_tir = ok.clone();
    no_tir.palw_tir_v1 = None;
    no_tir.sync_palw_tir_v1();
    assert!(no_tir.validate_palw_gen_v1().is_err(), "the generative court is the IR court's extension");
    let mut late_tir = ok.clone();
    late_tir.palw_tir_v1 = Some(kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1::testnet12_v1(ForkActivation::new(AT + 1)));
    late_tir.sync_palw_tir_v1();
    assert!(late_tir.validate_palw_gen_v1().is_err(), "the IR fence must be in force at or below it");
    let mut never_tir = ok.clone();
    never_tir.palw_tir_v1 = Some(kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1::testnet12_v1(ForkActivation::never()));
    never_tir.sync_palw_tir_v1();
    assert!(never_tir.validate_palw_gen_v1().is_err(), "a dormant IR fence is no IR fence");
    let mut same_height = ok.clone();
    same_height.palw_gen_v1 = Some(PalwGenFenceV1::drill_v1(ForkActivation::new(2_000)));
    assert!(same_height.validate_palw_gen_v1().is_ok(), "at the IR fence's own height");

    let mut not_v2 = SIMNET_PARAMS;
    assert!(!matches!(not_v2.palw_consensus_mode, kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(_)));
    not_v2.palw_gen_v1 = Some(PalwGenFenceV1::drill_v1(ForkActivation::new(AT)));
    assert!(not_v2.validate_palw_gen_v1().is_err(), "a ConsensusV2 rule");
    assert!(not_v2.palw_gen_v1_fence().is_none(), "and the accessor never answers off V2");
}

#[test]
fn the_ids_are_keyed_hashes_of_what_they_name() {
    let keyed = |key: &[u8], bytes: &[u8]| blake2b_simd::Params::new().hash_length(64).key(key).hash(bytes).as_bytes().to_vec();
    let rand = misaka_palw_gen::rand::rand_set_descriptor_v1();
    assert_eq!(
        palw_gen_rand_set_id_v1().as_byte_slice(),
        &keyed(misaka_palw_gen::rand::RAND_SET_ID_KEY_V1, rand.as_bytes())[..],
        "rand_set_id = H_key(descriptor)"
    );
    let output = misaka_palw_gen::output::output_set_descriptor_v1();
    assert_eq!(
        palw_gen_output_set_id_v1().as_byte_slice(),
        &keyed(misaka_palw_gen::output::OUTPUT_SET_ID_KEY_V1, output.as_bytes())[..],
        "output_set_id = H_key(descriptor)"
    );
    assert_ne!(palw_gen_rand_set_id_v1(), palw_gen_output_set_id_v1());
    let fence = PalwGenFenceV1::drill_v1(ForkActivation::new(AT));
    assert_eq!(fence.prim_set_id, palw_tir_prim_set_id_v1(), "version 2 moves no primitive (PALW-TIR-41)");
    let (p, r, o) = (fence.prim_set_id, fence.rand_set_id, fence.output_set_id);
    let root = palw_gen_court_root_v1(&p, 2, &r, &o, 1);
    assert_ne!(root, palw_gen_court_root_v1(&p, 3, &r, &o, 1), "the program version is inside the root");
    assert_ne!(root, palw_gen_court_root_v1(&p, 2, &o, &r, 1), "the rand and output sets, in their places");
    assert_ne!(root, palw_gen_court_root_v1(&p, 2, &r, &o, 2), "the generative court version");
    assert_ne!(root, palw_gen_court_root_v1(&r, 2, &r, &o, 1), "the primitive set");
    assert_ne!(root, palw_tir_court_root_v1(&p, PALW_TIR_COURT_VERSION_V1), "and it is not the IR court's root");
}

#[test]
fn the_drill_entry_sets_the_field_it_names() {
    let mut p = palw_t12_shipped_params();
    (PALW_DRILL_GEN_V1_ENTRY.set)(&mut p, Some(ForkActivation::new(AT)));
    assert_eq!(p.palw_gen_v1, Some(PalwGenFenceV1::this_build_v1(ForkActivation::new(AT), PALW_DRILL_GEN_CEILINGS_V1)));
    assert!(p.palw_fences_v1().iter().any(|(name, _)| *name == PALW_DRILL_GEN_V1_ENTRY.name), "the entry's name is a fence's");
    (PALW_DRILL_GEN_V1_ENTRY.set)(&mut p, None);
    assert_eq!(ids(&p), ids(&palw_t12_shipped_params()), "setting it back is the release, byte for byte");
}

#[test]
fn the_drill_mover_arms_it_on_a_salted_drill_and_nowhere_else() {
    let drill = palw_t12_drill_params_v1(&salt());
    // Above the drill's IR flag day (2,000): armed, the one fence, and the result validates.
    let mut moved = drill.clone();
    let moves = palw_drill_gen_fence_at_v1(&mut moved, 2_345).expect("a salted drill arms it above palw_tir_v1");
    assert_eq!(moves.len(), 1);
    assert_eq!((moves[0].name, moves[0].was, moves[0].at), ("palw_gen_v1", None, 2_345));
    assert!(moved.palw_gen_v1_active_at(2_345) && !moved.palw_gen_v1_active_at(2_344));
    for ((name, was), (_, now)) in drill.palw_fences_v1().iter().zip(moved.palw_fences_v1().iter()) {
        if *name != "palw_gen_v1" {
            assert_eq!(was, now, "{name} did not move");
        }
    }
    assert_eq!(ids(&moved).1, ids(&drill).1, "a future height: the identity is the drill's");
    // Refusals leave the ruleset as it came.
    for (at, why) in [(1_500, "below the IR flag day"), (2_000, "palw_tir_v1's own height"), (0, "genesis"), (u64::MAX, "never")] {
        let mut p = drill.clone();
        assert!(palw_drill_gen_fence_at_v1(&mut p, at).is_err(), "{why}");
        assert_eq!(ids(&p), ids(&drill), "{why}: untouched");
    }
    let mut public = palw_t12_shipped_params();
    assert!(palw_drill_gen_fence_at_v1(&mut public, 2_345).is_err(), "public testnet-12's genesis: never");
    let mut mainnet = mainnet_shipped_params();
    assert!(palw_drill_gen_fence_at_v1(&mut mainnet, 2_345).is_err(), "another network: never");
}
