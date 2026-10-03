//! **RFC-0001's five dormant fences** — `palw_fp_decode_constraint` (ADR-0096 D6-8, built here), `palw_fp_prefix_state`
//! (§2.6 stage 2), `palw_fp_tokenizer_match` (§2.9), `palw_fp_constraint_v2` (§2.5) and `palw_adapter_class_v1` (§2.10).
//!
//! Each is written in four places (the field, `for_each_fence`, the Some-only writes of both ids, the `never()`
//! collapse), `None` on every shipped preset, named by `palw_fences_v1` and the fork id's probe, refused by
//! `validate_palw_v2` without its prerequisites BY NAME, and armed by a drill entry whose result assembles.

use kaspa_consensus_core::config::params::{
    ForkActivation, MAINNET_PARAMS, PALW_T12_INT11_FLAG_DAY_DAA, Params, SIMNET_PARAMS, TESTNET11_PARAMS, mainnet_shipped_params,
    palw_t12_release_v3_params, palw_t12_release_v5_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::fork_id_gate_fences_v1;
use kaspa_consensus_core::network::{NetworkId, NetworkType};

type Entry = kaspa_consensus_core::config::params::PalwPostLaunchFenceV1;

/// `(name, drill entry, the prerequisites validate_palw_v2 must name)`.
fn fences() -> Vec<(&'static str, Entry, Vec<&'static str>)> {
    vec![
        (
            "palw_fp_decode_constraint",
            kaspa_consensus_core::palw_fp_constraint_job_v1::PALW_DRILL_FP_DECODE_CONSTRAINT_ENTRY,
            vec![],
        ),
        (
            "palw_fp_prefix_state",
            kaspa_consensus_core::palw_fp_prefix_v1::PALW_DRILL_FP_PREFIX_STATE_ENTRY,
            vec!["palw_fp_derived_work", "palw_fp_decode_rules"],
        ),
        (
            "palw_fp_prefix_inherit",
            kaspa_consensus_core::palw_fp_prefix_v1::PALW_DRILL_FP_PREFIX_INHERIT_ENTRY,
            vec!["palw_fp_prefix_state"],
        ),
        (
            "palw_fp_tokenizer_match",
            kaspa_consensus_core::palw_fp_tokenizer_v1::PALW_DRILL_FP_TOKENIZER_MATCH_ENTRY,
            vec!["palw_tir_v1", "palw_fp_decode_rules"],
        ),
        (
            "palw_fp_constraint_v2",
            kaspa_consensus_core::palw_fp_constraint_v2::PALW_DRILL_FP_CONSTRAINT_V2_ENTRY,
            vec!["palw_fp_decode_constraint", "palw_fp_decode_rules"],
        ),
        (
            "palw_adapter_class_v1",
            kaspa_consensus_core::palw_adapter_class_v1::PALW_DRILL_ADAPTER_CLASS_V1_ENTRY,
            vec!["palw_tir_v1", "palw_improvement_v1"],
        ),
    ]
}

fn get(p: &Params, name: &str) -> Option<ForkActivation> {
    match name {
        "palw_fp_decode_constraint" => p.palw_fp_decode_constraint,
        "palw_fp_prefix_state" => p.palw_fp_prefix_state,
        "palw_fp_prefix_inherit" => p.palw_fp_prefix_inherit,
        "palw_fp_tokenizer_match" => p.palw_fp_tokenizer_match,
        "palw_fp_constraint_v2" => p.palw_fp_constraint_v2,
        "palw_adapter_class_v1" => p.palw_adapter_class_v1,
        other => panic!("{other}"),
    }
}

fn validate_one(p: &Params, name: &str) -> Result<(), String> {
    match name {
        "palw_fp_decode_constraint" => p.validate_palw_v2().map(|_| ()),
        "palw_fp_prefix_state" => p.validate_palw_fp_prefix_state_v1(),
        "palw_fp_prefix_inherit" => p.validate_palw_fp_prefix_inherit_v1(),
        "palw_fp_tokenizer_match" => p.validate_palw_fp_tokenizer_match_v1(),
        "palw_fp_constraint_v2" => p.validate_palw_fp_constraint_v2_v1(),
        "palw_adapter_class_v1" => p.validate_palw_adapter_class_v1_v1(),
        other => panic!("{other}"),
    }
    .map_err(|e| e.to_string())
}

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

#[test]
fn every_rfc1_fence_is_dormant_on_every_shipped_preset_and_named() {
    let shipped = [
        ("MAINNET_PARAMS", MAINNET_PARAMS),
        ("TESTNET11_PARAMS", TESTNET11_PARAMS),
        ("SIMNET_PARAMS", SIMNET_PARAMS),
        ("mainnet_shipped_params", mainnet_shipped_params()),
        ("palw_t12_release_v5_params", palw_t12_release_v5_params()),
        ("palw_t12_release_v3_params", palw_t12_release_v3_params()),
        ("palw_t12_shipped_params", palw_t12_shipped_params()),
        ("from(testnet-12)", Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))),
        ("from(testnet-11)", Params::from(NetworkId::with_suffix(NetworkType::Testnet, 11))),
        ("from(testnet-10)", Params::from(NetworkId::with_suffix(NetworkType::Testnet, 10))),
        ("from(mainnet)", Params::from(NetworkId::new(NetworkType::Mainnet))),
        ("from(devnet)", Params::from(NetworkId::new(NetworkType::Devnet))),
    ];
    for (name, p) in &shipped {
        // int-12: testnet-12's release arms every RFC-0001 fence at the 5,300 flag day (the list); the baseline rows stay dormant.
        if *name == "palw_t12_shipped_params" || *name == "from(testnet-12)" {
            for (fence, _, _) in fences() {
                assert_eq!(
                    get(p, fence).map(|a| a.daa_score()),
                    PALW_T12_INT11_FLAG_DAY_DAA,
                    "{name}: {fence} arms at the int-12 flag day"
                );
            }
            continue;
        }
        for (fence, _, _) in fences() {
            assert!(get(p, fence).is_none(), "{name}: {fence} is dormant");
            assert!(p.palw_fences_v1().contains(&(fence, None)), "{name}: {fence} is named by palw_fences_v1");
            assert!(validate_one(p, fence).is_ok(), "{name}: {fence}: a dormant fence validates");
        }
    }
}

#[test]
fn an_armed_fence_is_fingerprinted_some_only_and_collapses_from_never() {
    let h = PALW_T12_INT11_FLAG_DAY_DAA.expect("the int-11 flag day has a height");
    let at = h + 1_000;
    // int-12: the shipped ruleset arms these at 5,300 already, so start from it with the RFC-0001 fences taken away (their
    // prerequisites — decode rules, the improvement fence, the IR — stay armed at the flag day, as a later flag day would find them).
    let mut base = palw_t12_shipped_params();
    for (_, entry, _) in fences() {
        (entry.set)(&mut base, None);
    }
    base.validate_palw_v2().expect("the shipped testnet-12 without the RFC-0001 fences assembles");
    for (fence, entry, _) in fences() {
        let mut armed = base.clone();
        (entry.set)(&mut armed, Some(ForkActivation::new(at)));
        assert_eq!(entry.name, fence);
        assert_eq!(get(&armed, fence), Some(ForkActivation::new(at)), "{fence}: the entry arms it");
        let (b, a) = (ids(&base), ids(&armed));
        assert_ne!(a.0, b.0, "{fence}: armed, the params id names it");
        assert_ne!(a.2, b.2, "{fence}: and the schedule reports it");
        assert!(fork_id_gate_fences_v1(&armed).contains(&at), "{fence}: the fork id gate sees its height");
        assert!(armed.palw_fences_v1().contains(&(fence, Some(ForkActivation::new(at)))), "{fence}: named with its height");
        // Some(never()) is absence, for the identity too.
        let mut never = base.clone();
        (entry.set)(&mut never, Some(ForkActivation::never()));
        assert_eq!(ids(&never).1, b.1, "{fence}: a never() fence fingerprints as none (identity id)");
        assert!(validate_one(&never, fence).is_ok(), "{fence}: never() is dormant and passes");
    }
}

#[test]
fn a_fence_arms_over_its_prerequisites_and_is_refused_without_them_by_name() {
    let h = PALW_T12_INT11_FLAG_DAY_DAA.expect("the int-11 flag day has a height");
    let at = h + 1_000;
    let mut base = palw_t12_shipped_params();
    for (_, entry, _) in fences() {
        (entry.set)(&mut base, None);
    }
    for (fence, entry, prereqs) in fences() {
        let mut armed = base.clone();
        (entry.set)(&mut armed, Some(ForkActivation::new(at)));
        if fence == "palw_fp_constraint_v2" {
            // The second form rides the first fence, which this build can now arm.
            armed.palw_fp_decode_constraint = Some(ForkActivation::new(at - 1));
            armed.sync_palw_fp_decode_constraint_v1();
        }
        if fence == "palw_fp_prefix_inherit" {
            // Version 12 is a prefix-state job: the fence it rides is armed (with its own prerequisites) below it.
            armed.palw_fp_prefix_state = Some(ForkActivation::new(at - 1));
        }
        armed.validate_palw_v2().unwrap_or_else(|e| panic!("{fence} over testnet-12's prerequisites: {e}"));
        // Below a prerequisite, by name.
        for prereq in &prereqs {
            let mut early = armed.clone();
            match *prereq {
                "palw_fp_derived_work" => early.palw_fp_derived_work = Some(ForkActivation::new(at + 1)),
                "palw_fp_decode_rules" => early.palw_fp_decode_rules = Some(ForkActivation::new(at + 1)),
                "palw_tir_v1" => {
                    if let Some(f) = early.palw_tir_v1.as_mut() {
                        f.activation = ForkActivation::new(at + 1);
                    }
                }
                "palw_fp_prefix_state" => early.palw_fp_prefix_state = Some(ForkActivation::new(at + 1)),
                "palw_fp_decode_constraint" => {
                    early.palw_fp_decode_constraint = Some(ForkActivation::new(at + 1));
                    early.sync_palw_fp_decode_constraint_v1();
                }
                "palw_improvement_v1" => {
                    if let Some(f) = early.palw_improvement_v1.as_mut() {
                        f.activation = ForkActivation::new(at + 1);
                    }
                }
                other => panic!("{other}"),
            }
            let why = validate_one(&early, fence).expect_err(&format!("{fence} below {prereq}"));
            assert!(why.contains(fence) && why.contains(prereq), "{why}");
        }
        // Off a ConsensusV2 network (the constraint fence has no validate of its own: its refusal is inside `validate_palw_v2`,
        // which meets the other fences' mirrors first on a ruleset with no bundle).
        if fence == "palw_fp_decode_constraint" {
            continue;
        }
        let mut v1 = armed.clone();
        v1.palw_consensus_mode = kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::Disabled;
        let why = validate_one(&v1, fence).expect_err("not ConsensusV2");
        assert!(why.contains("not ConsensusV2"), "{why}");
    }
}

#[test]
fn the_fences_are_in_no_testnet_12_release_list_before_the_int12_flag_day() {
    use kaspa_consensus_core::config::params::{
        PALW_T12_INT11_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V2, PALW_T12_POST_LAUNCH_FENCES_V3,
        PALW_T12_TIR_FLAG_DAY_FENCES_V1,
    };
    for list in [
        PALW_T12_POST_LAUNCH_FENCES_V1,
        PALW_T12_POST_LAUNCH_FENCES_V2,
        PALW_T12_POST_LAUNCH_FENCES_V3,
        PALW_T12_TIR_FLAG_DAY_FENCES_V1,
    ] {
        for (fence, _, _) in fences() {
            assert!(list.iter().all(|f| f.name != fence), "{fence} is in no earlier testnet-12 flag-day list");
        }
    }
    // int-12: the 5,300 list arms every one of them.
    for (fence, _, _) in fences() {
        assert!(PALW_T12_INT11_FENCES_V1.iter().any(|f| f.name == fence), "{fence} is on the int-12 list");
    }
}
