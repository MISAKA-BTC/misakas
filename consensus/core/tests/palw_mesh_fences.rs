//! **RFC-0007 Parts II and IV: the `palw_witness_manifest_v1`, `palw_audit_mesh_v1` and `palw_capped_onboarding_v1` fences** — each
//! dormant on every ruleset a node can run (testnet-12 included: none joins a flag-day list), fingerprinted with its value where
//! armed (Some-only, so the identity of every dormant ruleset is the one it had before the fields existed), invisible to the
//! identity until it fires, named by the fork id, armed by the drill movers alone, and refused by `validate_palw_v2` without each
//! prerequisite in force at or below it — **by name** — or with its mirror unsynced.
//!
//! This file adds no id pin (a quoted id here would be one `t12-repin.sh` does not know): it checks the fields.
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_mesh_fences`

use kaspa_consensus_core::config::drill::{
    PALW_DRILL_SALT_LEN_V1, PalwDrillSaltV1, palw_drill_audit_mesh_at_v1, palw_drill_capped_at_v1,
    palw_drill_post_launch_fences_at_v1, palw_drill_post_launch_fences_v2_at_v1, palw_drill_post_launch_fences_v3_at_v1,
    palw_drill_tir_fence_at_v1, palw_drill_vertex_at_v1, palw_drill_witness_at_v1,
};
use kaspa_consensus_core::config::params::{
    DEVNET_PARAMS, ForkActivation, MAINNET_PARAMS, PALW_T12_CAPACITY_FENCES_V1, PALW_T12_CAPACITY_RHO10_FENCES_V1,
    PALW_T12_INT11_FENCES_V1, PALW_T12_INT11_RHO100_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V2,
    PALW_T12_POST_LAUNCH_FENCES_V3, PALW_T12_TIR_FENCE2_FENCES_V1, PALW_T12_TIR_FLAG_DAY_FENCES_V1, Params, SIMNET_PARAMS,
    TESTNET_PARAMS, TESTNET11_PARAMS, devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params,
    palw_t12_arm_int11_flag_day_at_v1, palw_t12_drill_params_v1, palw_t12_launch_params_v1, palw_t12_release_v1_params,
    palw_t12_release_v2_params, palw_t12_release_v3_params, palw_t12_release_v5_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_gate_fences_v1, fork_id_v1};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_mesh_v1::{
    PALW_AUDIT_MESH_ENTRY_V1, PALW_CAPPED_ONBOARDING_ENTRY_V1, PALW_DRILL_AUDIT_MESH_FENCES_V1, PALW_DRILL_CAPPED_FENCES_V1,
    PALW_DRILL_WITNESS_FENCES_V1, PALW_WITNESS_MANIFEST_ENTRY_V1,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_vertex_v1::PALW_VERTEX_ENTRY_V1;

/// A height no testnet-12 fence uses, above every one it schedules (the int-11 flag day is 5,300).
const AT: u64 = 6_543;

const NAMES: [&str; 3] = ["palw_witness_manifest_v1", "palw_audit_mesh_v1", "palw_capped_onboarding_v1"];

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn salt() -> PalwDrillSaltV1 {
    PalwDrillSaltV1::from_bytes([0x3e; PALW_DRILL_SALT_LEN_V1]).expect("a legal salt")
}

fn mirrors(p: &Params) -> [Option<u64>; 3] {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => {
            [bundle.state.witness_manifest_from_daa(), bundle.state.audit_mesh_from_daa(), bundle.state.capped_from_daa()]
        }
        _ => [None; 3],
    }
}

fn dormant_drill() -> Params {
    let mut drill = palw_t12_drill_params_v1(&salt());
    palw_t12_arm_int11_flag_day_at_v1(&mut drill, None);
    drill
}

fn rulesets() -> Vec<(&'static str, Params)> {
    vec![
        ("MAINNET_PARAMS", MAINNET_PARAMS),
        ("TESTNET_PARAMS", TESTNET_PARAMS),
        ("TESTNET11_PARAMS", TESTNET11_PARAMS),
        ("DEVNET_PARAMS", DEVNET_PARAMS),
        ("SIMNET_PARAMS", SIMNET_PARAMS),
        ("from(mainnet)", Params::from(NetworkId::new(NetworkType::Mainnet))),
        ("from(testnet-11)", Params::from(NetworkId::with_suffix(NetworkType::Testnet, 11))),
        ("from(testnet-12)", Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))),
        ("from(devnet)", Params::from(NetworkId::new(NetworkType::Devnet))),
        ("mainnet_shipped_params", mainnet_shipped_params()),
        ("devnet_shipped_params", devnet_shipped_params()),
        ("palw_rc_shipped_params", palw_rc_shipped_params()),
        ("palw_t12_shipped_params", palw_t12_shipped_params()),
        ("palw_t12_release_v5_params", palw_t12_release_v5_params()),
        ("palw_t12_launch_params_v1", palw_t12_launch_params_v1()),
        ("palw_t12_release_v1_params", palw_t12_release_v1_params()),
        ("palw_t12_release_v2_params", palw_t12_release_v2_params()),
        ("palw_t12_release_v3_params", palw_t12_release_v3_params()),
        ("palw_t12_drill_params_v1", palw_t12_drill_params_v1(&salt())),
        ("palw_t12_drill_params_v1 − int-11", dormant_drill()),
    ]
}

/// The release with the fences up to and including the one named armed at their own heights (the chain of prerequisites:
/// vertex ≤ audit mesh ≤ capped; the witness manifest stands alone).
fn armed_with(witness: Option<u64>, vertex: Option<u64>, audit: Option<u64>, capped: Option<u64>) -> Params {
    let mut p = palw_t12_release_v5_params();
    (PALW_VERTEX_ENTRY_V1.set)(&mut p, vertex.map(ForkActivation::new));
    (PALW_WITNESS_MANIFEST_ENTRY_V1.set)(&mut p, witness.map(ForkActivation::new));
    (PALW_AUDIT_MESH_ENTRY_V1.set)(&mut p, audit.map(ForkActivation::new));
    (PALW_CAPPED_ONBOARDING_ENTRY_V1.set)(&mut p, capped.map(ForkActivation::new));
    p
}

#[test]
fn every_ruleset_leaves_all_three_dormant_and_no_flag_day_list_names_them() {
    for (name, p) in rulesets() {
        // int-12: testnet-12's release arms all three at the 5,300 flag day (the list), so its shipped rulesets are armed there.
        if name == "from(testnet-12)" || name == "palw_t12_shipped_params" || name == "palw_t12_drill_params_v1" {
            let listed = p.palw_fences_v1();
            for fence in NAMES {
                assert!(
                    listed.iter().any(|(n, a)| *n == fence && a.map(|a| a.daa_score()) == kaspa_consensus_core::config::params::PALW_T12_INT11_FLAG_DAY_DAA),
                    "{name}: {fence} arms at the int-12 flag day"
                );
            }
            continue;
        }
        assert!(
            p.palw_witness_manifest_v1.is_none() && p.palw_audit_mesh_v1.is_none() && p.palw_capped_onboarding_v1.is_none(),
            "{name}: dormant"
        );
        assert!(
            p.palw_witness_manifest_fence().is_none()
                && p.palw_audit_mesh_fence().is_none()
                && p.palw_capped_onboarding_fence().is_none()
                && !p.palw_witness_manifest_active_at(u64::MAX)
                && !p.palw_audit_mesh_active_at(u64::MAX)
                && !p.palw_capped_onboarding_active_at(u64::MAX),
            "{name}: the accessors agree"
        );
        assert_eq!(mirrors(&p), [None; 3], "{name}: no mirror");
        let listed = p.palw_fences_v1();
        for fence in NAMES {
            assert!(listed.contains(&(fence, None)), "{name}: the exhaustive fence list names {fence}");
        }
        assert!(
            p.validate_palw_witness_manifest_v1().is_ok()
                && p.validate_palw_audit_mesh_v1().is_ok()
                && p.validate_palw_capped_onboarding_v1().is_ok(),
            "{name}: nothing to refuse"
        );
        p.validate_palw_v2().unwrap_or_else(|e| {
            if matches!(p.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
                panic!("{name}: {e}")
            }
        });
    }
    for list in [
        PALW_T12_POST_LAUNCH_FENCES_V1,
        PALW_T12_POST_LAUNCH_FENCES_V2,
        PALW_T12_POST_LAUNCH_FENCES_V3,
        PALW_T12_CAPACITY_FENCES_V1,
        PALW_T12_CAPACITY_RHO10_FENCES_V1,
        PALW_T12_TIR_FLAG_DAY_FENCES_V1,
        PALW_T12_TIR_FENCE2_FENCES_V1,
        PALW_T12_INT11_RHO100_FENCES_V1,
    ] {
        assert!(list.iter().all(|f| !NAMES.contains(&f.name)), "no earlier testnet-12 flag day arms a mesh fence");
    }
    // int-12: the 5,300 list arms all three, in prerequisite order.
    assert!(NAMES.iter().all(|n| PALW_T12_INT11_FENCES_V1.iter().any(|f| f.name == *n)), "the int-12 list arms the mesh fences");
    for (list, name) in [
        (PALW_DRILL_WITNESS_FENCES_V1, NAMES[0]),
        (PALW_DRILL_AUDIT_MESH_FENCES_V1, NAMES[1]),
        (PALW_DRILL_CAPPED_FENCES_V1, NAMES[2]),
    ] {
        assert_eq!(list.iter().map(|f| f.name).collect::<Vec<_>>(), [name]);
    }
}

#[test]
fn a_scheduled_fence_moves_the_ruleset_and_the_schedule_and_never_the_identity_and_the_fork_id_gates_it() {
    let shipped = palw_t12_release_v5_params();
    let base = ids(&shipped);
    for (i, name) in NAMES.iter().enumerate() {
        let p = match i {
            0 => armed_with(Some(AT), None, None, None),
            1 => armed_with(None, Some(AT - 1), Some(AT), None),
            _ => armed_with(None, Some(AT - 2), Some(AT - 1), Some(AT)),
        };
        p.validate_palw_v2().unwrap_or_else(|e| panic!("{name}: testnet-12 past its int-11 flag day can arm it: {e}"));
        assert_eq!(mirrors(&p)[i], Some(AT), "{name}: the fold's mirror");
        let moved = ids(&p);
        assert_ne!(moved.0, base.0, "{name}: the ruleset a node announces names the fence and its value");
        assert_eq!(moved.1, base.1, "{name}: two builds that differ only about a FUTURE height stay peers");
        assert_ne!(moved.2, base.2, "{name}: the schedule reports it");
        assert!(fork_id_gate_fences_v1(&p).contains(&AT), "{name}: past its height an un-upgraded node is refused");
        let old = fork_id_v1(&shipped, AT);
        assert!(evaluate_fork_id_v1(&p, AT, old.fired.as_bytes().as_slice(), old.next).refuses(), "{name}: gated from its height");
    }
    // Set back: the shipped ruleset, to the id.
    let back = armed_with(None, None, None, None);
    assert_eq!(ids(&back), base, "set(None) everywhere is the shipped ruleset");
    assert_eq!(mirrors(&back), [None; 3]);
}

#[test]
fn never_is_absence_for_the_identity() {
    let base = palw_t12_release_v5_params();
    for i in 0..3 {
        let mut never = palw_t12_release_v5_params();
        match i {
            0 => (PALW_WITNESS_MANIFEST_ENTRY_V1.set)(&mut never, Some(ForkActivation::never())),
            1 => (PALW_AUDIT_MESH_ENTRY_V1.set)(&mut never, Some(ForkActivation::never())),
            _ => (PALW_CAPPED_ONBOARDING_ENTRY_V1.set)(&mut never, Some(ForkActivation::never())),
        }
        assert_eq!(never.consensus_identity_id(), base.consensus_identity_id(), "{}: Some(never()) collapses whole", NAMES[i]);
        never.validate_palw_v2().unwrap_or_else(|e| panic!("{}: a dormant value validates: {e}", NAMES[i]));
        assert_eq!(mirrors(&never), [None; 3], "a dormant value has no mirror");
    }
}

/// **Each prerequisite, by name**.
#[test]
fn each_fence_is_refused_below_its_prerequisites_by_name_and_with_its_mirror_unsynced() {
    // Witness manifest: palw_tir_v1, palw_unavailable_abstains.
    for (name, strip) in [
        ("palw_tir_v1", (|p: &mut Params| p.palw_tir_v1 = None) as fn(&mut Params)),
        ("palw_unavailable_abstains", |p| p.palw_unavailable_abstains = None),
    ] {
        let mut p = armed_with(Some(AT), None, None, None);
        strip(&mut p);
        let why = p.validate_palw_witness_manifest_v1().expect_err("a prerequisite missing");
        assert!(format!("{why:?}").contains(name), "witness / {name}: {why:?}");
    }
    // Audit mesh: palw_verification_vertex_v1, palw_tir_v1, palw_panel_economy.
    for (name, strip) in [
        ("palw_verification_vertex_v1", (|p: &mut Params| p.palw_verification_vertex_v1 = None) as fn(&mut Params)),
        ("palw_tir_v1", |p| p.palw_tir_v1 = None),
        ("palw_panel_economy", |p| p.palw_panel_economy = None),
    ] {
        let mut p = armed_with(None, Some(AT - 1), Some(AT), None);
        strip(&mut p);
        let why = p.validate_palw_audit_mesh_v1().expect_err("a prerequisite missing");
        assert!(format!("{why:?}").contains(name), "audit / {name}: {why:?}");
    }
    // …and a vertex fence that arms after the mesh is as good as absent.
    let late = armed_with(None, Some(AT + 1), Some(AT), None);
    assert!(format!("{:?}", late.validate_palw_audit_mesh_v1().expect_err("late")).contains("palw_verification_vertex_v1"));
    // Capped: palw_audit_mesh_v1, palw_admission_independence, palw_registry_resilience.
    for (name, strip) in [
        ("palw_audit_mesh_v1", (|p: &mut Params| p.palw_audit_mesh_v1 = None) as fn(&mut Params)),
        ("palw_admission_independence", |p| p.palw_admission_independence = None),
        ("palw_registry_resilience", |p| p.palw_registry_resilience = None),
    ] {
        let mut p = armed_with(None, Some(AT - 2), Some(AT - 1), Some(AT));
        strip(&mut p);
        let why = p.validate_palw_capped_onboarding_v1().expect_err("a prerequisite missing");
        assert!(format!("{why:?}").contains(name), "capped / {name}: {why:?}");
    }
    let late = armed_with(None, Some(AT - 2), Some(AT + 1), Some(AT));
    assert!(format!("{:?}", late.validate_palw_capped_onboarding_v1().expect_err("late")).contains("palw_audit_mesh_v1"));
    // The mirror: a fence with no mirror, and a mirror with no fence.
    for i in 0..3 {
        let mut unsynced = palw_t12_release_v5_params();
        match i {
            0 => unsynced.palw_witness_manifest_v1 = Some(ForkActivation::new(AT)),
            1 => unsynced.palw_audit_mesh_v1 = Some(ForkActivation::new(AT)),
            _ => unsynced.palw_capped_onboarding_v1 = Some(ForkActivation::new(AT)),
        }
        assert!(
            unsynced.validate_palw_witness_manifest_v1().is_err()
                || unsynced.validate_palw_audit_mesh_v1().is_err()
                || unsynced.validate_palw_capped_onboarding_v1().is_err(),
            "{}: the mirror disagrees with the fence",
            NAMES[i]
        );
    }
    // They are ConsensusV2 rules.
    let mut not_v2 = SIMNET_PARAMS;
    not_v2.palw_witness_manifest_v1 = Some(ForkActivation::new(AT));
    not_v2.palw_audit_mesh_v1 = Some(ForkActivation::new(AT));
    not_v2.palw_capped_onboarding_v1 = Some(ForkActivation::new(AT));
    assert!(not_v2.validate_palw_witness_manifest_v1().is_err() && not_v2.validate_palw_audit_mesh_v1().is_err());
    assert!(not_v2.validate_palw_capped_onboarding_v1().is_err());
    assert!(not_v2.palw_witness_manifest_fence().is_none() && not_v2.palw_audit_mesh_fence().is_none());
    assert!(not_v2.palw_capped_onboarding_fence().is_none(), "the accessors never answer off V2");
}

#[test]
fn the_drill_entries_set_the_fields_they_name() {
    let mut p = palw_t12_release_v5_params();
    (PALW_WITNESS_MANIFEST_ENTRY_V1.set)(&mut p, Some(ForkActivation::new(AT)));
    (PALW_AUDIT_MESH_ENTRY_V1.set)(&mut p, Some(ForkActivation::new(AT + 1)));
    (PALW_CAPPED_ONBOARDING_ENTRY_V1.set)(&mut p, Some(ForkActivation::new(AT + 2)));
    assert_eq!(
        (p.palw_witness_manifest_v1, p.palw_audit_mesh_v1, p.palw_capped_onboarding_v1),
        (Some(ForkActivation::new(AT)), Some(ForkActivation::new(AT + 1)), Some(ForkActivation::new(AT + 2)))
    );
    assert_eq!(mirrors(&p), [Some(AT), Some(AT + 1), Some(AT + 2)], "the entries' own sets write the mirrors");
    for entry in [&PALW_WITNESS_MANIFEST_ENTRY_V1, &PALW_AUDIT_MESH_ENTRY_V1, &PALW_CAPPED_ONBOARDING_ENTRY_V1] {
        assert!(p.palw_fences_v1().iter().any(|(name, _)| *name == entry.name), "{}: a fence's name", entry.name);
        (entry.set)(&mut p, None);
    }
    assert_eq!(ids(&p), ids(&palw_t12_release_v5_params()), "setting them back is the release, byte for byte");
}

#[test]
fn the_drill_movers_arm_them_on_a_salted_drill_and_nowhere_else() {
    let drill = dormant_drill();
    // The witness manifest and the vertex alone: the drill's prerequisites stand at genesis.
    // (The drill's IR fence is its own flag: the witness is an IR class's, so it stands first.)
    let mut witness = drill.clone();
    palw_drill_post_launch_fences_at_v1(&mut witness, 100).expect("the first post-launch flag day");
    palw_drill_post_launch_fences_v2_at_v1(&mut witness, 110).expect("the second");
    palw_drill_post_launch_fences_v3_at_v1(&mut witness, 120).expect("the third");
    palw_drill_tir_fence_at_v1(&mut witness, 130).expect("the IR fence");
    let moves = palw_drill_witness_at_v1(&mut witness, 140).expect("a salted drill arms it low");
    assert_eq!((moves.len(), moves[0].name, moves[0].was, moves[0].at), (1, "palw_witness_manifest_v1", None, 140));
    witness.validate_palw_v2().expect("the witness drill validates");
    // The mesh needs the vertex below it; the capped fence needs the mesh below it.
    let mut chain = drill.clone();
    assert!(palw_drill_audit_mesh_at_v1(&mut chain, 150).is_err(), "the mesh alone has no vertex below it");
    let mut chain = drill.clone();
    // Capped onboarding leans on the registry-resilience and admission-independence fences of the post-launch flag days.
    palw_drill_post_launch_fences_at_v1(&mut chain, 100).expect("the first post-launch flag day");
    palw_drill_post_launch_fences_v2_at_v1(&mut chain, 110).expect("the second");
    palw_drill_post_launch_fences_v3_at_v1(&mut chain, 120).expect("the third");
    palw_drill_tir_fence_at_v1(&mut chain, 130).expect("the IR fence");
    let before = chain.clone();
    palw_drill_vertex_at_v1(&mut chain, 140).expect("the vertex");
    palw_drill_audit_mesh_at_v1(&mut chain, 150).expect("the mesh above it");
    chain.validate_palw_v2().expect("the mesh drill validates above the vertex");
    palw_drill_capped_at_v1(&mut chain, 160).expect("capped onboarding above the mesh");
    chain.validate_palw_v2().expect("the whole chain validates");
    for (name, was) in before.palw_fences_v1() {
        if !NAMES.contains(&name) && name != "palw_verification_vertex_v1" {
            let now = chain.palw_fences_v1().iter().find(|(n, _)| *n == name).map(|(_, v)| *v).unwrap();
            assert_eq!(was, now, "{name} did not move");
        }
    }
    // Refusals leave the ruleset as it came.
    for (at, why) in [(0, "genesis"), (u64::MAX, "never")] {
        let mut p = drill.clone();
        assert!(palw_drill_witness_at_v1(&mut p, at).is_err(), "{why}");
        assert_eq!(ids(&p), ids(&drill), "{why}: untouched");
    }
    let mut public = palw_t12_shipped_params();
    assert!(palw_drill_witness_at_v1(&mut public, AT).is_err(), "public testnet-12's genesis: never");
    let mut mainnet = mainnet_shipped_params();
    assert!(palw_drill_capped_at_v1(&mut mainnet, AT).is_err(), "another network: never");
}
