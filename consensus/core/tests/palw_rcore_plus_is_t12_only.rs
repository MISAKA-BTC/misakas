//! **ADR-0152 v3.1 R-core+ (`Params::palw_rcore_plus`) is testnet-12's alone — T24's fence part.**
//!
//! The v22 skeleton declares the fence, C7's list (`Params::palw_rcore_conservative_classes`) and
//! the bundle's three mirrors with every R-core+ writer dormant. So what may move, and where:
//!
//! * **testnet-12** arms the fence at DAA 0 with C7 = `[the 2M class id]`, commits to the
//!   `COMPLETE_V5` context set, and its ids move — the fence and C7 are in its fingerprint.
//! * **testnet-11 and devnet** move by exactly ONE thing: `PALW_STATE_V2_VERSION` 21 -> 22, which
//!   the V2 arm of `consensus_params_id` hashes explicitly. Their params and identity ids re-pin
//!   once; their schedule ids do not move (the fence is Some-only and absent there). The v21 values
//!   below are what `f1192685` (the skeleton's base) produces; the v22 values are this build's.
//! * **mainnet** carries no V2 bundle, so the version is not in its ids: all three are unchanged.
//!
//! And `validate_palw_v2` refuses the fence without each prerequisite ADR §6 lists, beside
//! `palw_shard_licensing`, over any context root but V5, with mirrors that disagree, and a C7 list
//! that is non-empty without the fence or names a class that is not a testnet-12 genesis held row.

use kaspa_consensus_core::config::params::{
    ForkActivation, OverrideParams, PALW_T12_RCORE_CONSERVATIVE_CLASSES, Params, devnet_shipped_params, mainnet_shipped_params,
    palw_rc_shipped_params, palw_t12_genesis_held_class_ids_v1, palw_t12_shipped_params, palw_v2_bond_withdrawal_delay_at_v1,
};
use kaspa_consensus_core::palw_mode_v2::{
    PalwConsensusMode, PalwModeV2Error, palw_v2_signature_contexts_root_v4, palw_v2_signature_contexts_root_v5,
};
use kaspa_hashes::Hash64;

/// `(network, consensus_params_id, consensus_identity_id, consensus_schedule_id)` at `f1192685`, the
/// v21 build the skeleton is laid on (= `palw_clock_floor_is_t12_only`'s `BEFORE_THE_FLOOR`).
const AT_V21: &[(&str, &str, &str, &str)] = &[
    (
        "testnet-11",
        "c99bb4f43891dc637e4d5634816c46b33d89f07a381875e2ce54bd3ef80ac74a",
        "19dbdbb8afd374aafa14f7fd7457fac7304a19df697706f69b34be0e1e995d4e",
        "5a1d8d5679e0e8d7e9022255668fd5d4b3e4c8a6c367acf3882c6a3d480d8b64",
    ),
    (
        "devnet",
        "9acd42be5357a25ee08c1c7037d1610ef00107e8bd47eb59e6c6a6f91c31f502",
        "9acd42be5357a25ee08c1c7037d1610ef00107e8bd47eb59e6c6a6f91c31f502",
        "edd80c01c791d225d602b9136f539f4dfeb506ba1b3071b177b0d873a661142f",
    ),
    (
        "mainnet",
        "badaa8e90f14ef0074048d6b18660864855be8ab854d0ecb01dfbb62171538e1",
        "00d98599bd45867f4a7b3bef1043431ccb8ef0d41cedbc552a87926b5e3b8af5",
        "a1ed7ff07231b84c51d9dc1013a8047ea3efb012bfc9daa36d5dd623709807e4",
    ),
];

/// The same presets at v22: the params and identity ids of the two V2 presets re-pinned for the
/// version, every schedule id and all of mainnet unchanged.
const AT_V22: &[(&str, &str, &str, &str)] = &[
    (
        "testnet-11",
        "bd633ce933974d4134676efbdaf46b269dc2fb78f007e0907479aabd4d743f29",
        "44cb8fd729e9575a6e3b1e72c466b8abce4b9ecd81bb556685c9ba225487117f",
        "5a1d8d5679e0e8d7e9022255668fd5d4b3e4c8a6c367acf3882c6a3d480d8b64",
    ),
    (
        "devnet",
        "7a27f341e49902ebb5e15ea79a45806fbd37b65daaddf8f0a5a10a15f9bfd4a8",
        "7a27f341e49902ebb5e15ea79a45806fbd37b65daaddf8f0a5a10a15f9bfd4a8",
        "edd80c01c791d225d602b9136f539f4dfeb506ba1b3071b177b0d873a661142f",
    ),
    (
        "mainnet",
        "badaa8e90f14ef0074048d6b18660864855be8ab854d0ecb01dfbb62171538e1",
        "00d98599bd45867f4a7b3bef1043431ccb8ef0d41cedbc552a87926b5e3b8af5",
        "a1ed7ff07231b84c51d9dc1013a8047ea3efb012bfc9daa36d5dd623709807e4",
    ),
];

fn shipped(name: &str) -> Params {
    match name {
        "testnet-11" => palw_rc_shipped_params(),
        "devnet" => devnet_shipped_params(),
        "mainnet" => mainnet_shipped_params(),
        other => panic!("no such preset {other}"),
    }
}

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn refusal_text(result: Result<(), PalwModeV2Error>) -> String {
    match result {
        Err(PalwModeV2Error::Invalid(why)) => why.to_string(),
        other => panic!("expected a named refusal, got {other:?}"),
    }
}

/// **Armed on testnet-12 from genesis, with C7 = [2M], the V5 contexts and its mirrors; dormant
/// everywhere else.**
#[test]
fn the_fence_is_armed_on_testnet12_only() {
    let t12 = palw_t12_shipped_params();
    assert_eq!(t12.palw_rcore_plus, Some(ForkActivation::always()), "testnet-12 arms R-core+ from genesis");
    assert!(t12.palw_rcore_plus_active_at(0));
    assert_eq!(t12.palw_rcore_conservative_classes, PALW_T12_RCORE_CONSERVATIVE_CLASSES.as_slice(), "C7 = [2M]");
    assert!(palw_t12_genesis_held_class_ids_v1().contains(&PALW_T12_RCORE_CONSERVATIVE_CLASSES[0]));
    t12.validate_palw_v2().expect("testnet-12 validates with the fence");
    t12.validate_palw_rcore_plus_v1().expect("and with every R-core+ rule");
    let PalwConsensusMode::ConsensusV2(bundle) = &t12.palw_consensus_mode else { panic!("testnet-12 is a V2 ruleset") };
    assert_eq!(bundle.signature_contexts_root, palw_v2_signature_contexts_root_v5(), "its bundle commits to COMPLETE_V5");
    let delay = palw_v2_bond_withdrawal_delay_at_v1(bundle, t12.palw_da_court, 0);
    println!("testnet-12: withdrawal delay mirror {delay} (bond {})", bundle.bond.withdrawal_delay_daa());
    assert_eq!(bundle.state.rcore_plus_from_daa(), Some(0));
    assert_eq!(bundle.state.withdrawal_delay_daa(), delay, "the processor's delay, DA lattice included");
    assert!(delay > bundle.bond.withdrawal_delay_daa(), "not the bond's bare delay");
    assert_eq!(delay, 12_900, "S-SPEC §6 T23: 7,500 + the DA lattice's 5,400 on testnet-12");
    assert_eq!(bundle.state.rcore_conservative_classes(), PALW_T12_RCORE_CONSERVATIVE_CLASSES.as_slice());
    assert_eq!(
        Some(kaspa_consensus_core::palw_state_v2::PALW_RCORE_REPORTER_REWARD_BPS_V1),
        t12.dns_params.as_ref().map(|dns| dns.reward_params.slashing_reporter_reward_bps),
        "the reporter's share is the DNS slashing split's reporter share on testnet-12"
    );
    for (name, _, _, _) in AT_V21 {
        let p = shipped(name);
        assert_eq!(p.palw_rcore_plus, None, "{name}: dormant");
        assert!(p.palw_rcore_conservative_classes.is_empty(), "{name}: no C7");
        assert!(!p.palw_rcore_plus_active_at(u64::MAX), "{name}: never in force");
        p.validate_palw_rcore_plus_v1().expect("a dormant preset has nothing to refuse");
        if let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode {
            assert_eq!(
                (bundle.state.rcore_plus_from_daa(), bundle.state.withdrawal_delay_daa(), bundle.state.rcore_conservative_classes()),
                (None, 0, [].as_slice()),
                "{name}: the mirrors are the dormant values"
            );
        }
    }
}

/// **testnet-11 and devnet move by the v22 version re-pin alone, mainnet not at all.**
///
/// The params and identity ids of the V2 presets move once, because `PALW_STATE_V2_VERSION` is
/// hashed into the V2 arm of `consensus_params_id`; the schedule ids do not move, because the fence
/// is Some-only and absent. And `Some(never())` on testnet-11 is absence to the identity.
#[test]
fn every_other_preset_moves_only_by_the_v22_version_re_pin() {
    let mut report = Vec::new();
    for ((name, p21, i21, s21), (_, p22, i22, s22)) in AT_V21.iter().zip(AT_V22) {
        let now = ids(&shipped(name));
        println!("{name}: params {} identity {} schedule {}", now.0, now.1, now.2);
        if (now.0.as_str(), now.1.as_str(), now.2.as_str()) != (*p22, *i22, *s22) {
            report.push(format!("{name}: {now:?}"));
        }
        assert_eq!(s21, s22, "{name}: the schedule id is not moved by the version");
        if *name == "mainnet" {
            assert_eq!((p21, i21), (p22, i22), "mainnet carries no V2 bundle: nothing moved");
        } else {
            assert_ne!(p21, p22, "{name}: the version is hashed into the V2 arm, so the params id re-pins once");
        }
    }
    assert!(report.is_empty(), "a preset that does not arm R-core+ moved beyond the v22 re-pin: {report:?}");
    let t11 = palw_rc_shipped_params();
    let mut never = t11.clone();
    never.palw_rcore_plus = Some(ForkActivation::never());
    // The identity two nodes compare normalizes `Some(never())` to absence (the collapse in
    // `normalize_values_a_scheduled_fence_drags_with_it`), so a node that wrote it still peers.
    assert_eq!(never.consensus_identity_id(), t11.consensus_identity_id(), "Some(never()) is absence on testnet-11");
}

/// **One negative case per refusal** (T24): each prerequisite removed or raised above the fence, the
/// fence past genesis, the second clock unset, shard licensing beside it, a context root other than
/// V5, each mirror disagreeing, and C7 without the fence or outside the genesis held rows. Each is
/// named by `validate_palw_rcore_plus_v1` and refused by `validate_palw_v2`.
#[test]
fn validate_refuses_the_fence_without_each_prerequisite() {
    let t12 = palw_t12_shipped_params();
    type Edit = Box<dyn Fn(&mut Params)>;
    let late = ForkActivation::new(5);
    let cases: Vec<(&str, Edit, &str)> = vec![
        ("past genesis", Box::new(|p| p.palw_rcore_plus = Some(ForkActivation::new(1))), "may only be armed at genesis"),
        ("attribution", Box::new(|p| p.palw_offence_attribution = None), "without palw_offence_attribution"),
        ("independence", Box::new(|p| p.palw_admission_independence = None), "without palw_admission_independence"),
        ("audit fence", Box::new(|p| p.palw_audit_2026_09_23 = None), "without palw_audit_2026_09_23"),
        // M4 review finding 4: SW-8's one state is the walk's object-by-object base.
        ("audit fence 09-11", Box::new(|p| p.palw_audit_2026_09_11 = None), "without palw_audit_2026_09_11"),
        ("audit fence 09-11 above", Box::new(move |p| p.palw_audit_2026_09_11 = Some(late)), "without palw_audit_2026_09_11"),
        ("economic safety", Box::new(|p| p.palw_economic_safety = None), "without palw_economic_safety"),
        ("objective offence", Box::new(|p| p.palw_objective_offence = None), "without palw_objective_offence"),
        ("panel economy", Box::new(|p| p.palw_panel_economy = None), "without palw_panel_economy"),
        ("exposure floor", Box::new(|p| p.palw_panel_exposure_floor = None), "without palw_panel_exposure_floor"),
        ("unavailable abstains", Box::new(|p| p.palw_unavailable_abstains = None), "without palw_unavailable_abstains"),
        ("clock floor", Box::new(|p| p.palw_clock_floor = None), "without palw_clock_floor"),
        ("clock cursor", Box::new(|p| p.palw_clock_cursor = None), "without palw_clock_cursor"),
        ("verification v2 above", Box::new(move |p| p.palw_verification_v2 = Some(late)), "without palw_verification_v2"),
        ("DA court", Box::new(|p| p.palw_da_court = None), "without palw_da_court"),
        ("operator ids", Box::new(|p| p.palw_operator_id_unique = None), "without palw_operator_id_unique"),
        ("second clock", Box::new(|p| p.palw_settled_anchor_depth = None), "without palw_settled_anchor_depth"),
        ("shard licensing", Box::new(|p| p.palw_shard_licensing = Some(ForkActivation::always())), "beside palw_shard_licensing"),
        (
            "context root V4",
            Box::new(|p| {
                if let PalwConsensusMode::ConsensusV2(bundle) = &mut p.palw_consensus_mode {
                    bundle.signature_contexts_root = palw_v2_signature_contexts_root_v4();
                }
            }),
            "not the COMPLETE_V5 set",
        ),
        (
            "delay mirror",
            Box::new(|p| {
                if let PalwConsensusMode::ConsensusV2(bundle) = &mut p.palw_consensus_mode {
                    let s = bundle.state.clone();
                    bundle.state = s.clone().with_rcore_plus_mirrors(
                        s.rcore_plus_from_daa(),
                        s.withdrawal_delay_daa() + 1,
                        s.rcore_conservative_classes().to_vec(),
                    );
                }
            }),
            "disagree with the V2 bundle's mirrors",
        ),
        (
            "fence mirror",
            Box::new(|p| {
                if let PalwConsensusMode::ConsensusV2(bundle) = &mut p.palw_consensus_mode {
                    let s = bundle.state.clone();
                    bundle.state =
                        s.clone().with_rcore_plus_mirrors(None, s.withdrawal_delay_daa(), s.rcore_conservative_classes().to_vec());
                }
            }),
            "disagree with the V2 bundle's mirrors",
        ),
        (
            "C7 mirror",
            Box::new(|p| {
                if let PalwConsensusMode::ConsensusV2(bundle) = &mut p.palw_consensus_mode {
                    let s = bundle.state.clone();
                    bundle.state = s.clone().with_rcore_plus_mirrors(s.rcore_plus_from_daa(), s.withdrawal_delay_daa(), Vec::new());
                }
            }),
            "disagree with the V2 bundle's mirrors",
        ),
        (
            "C7 outside the held rows",
            Box::new(|p| {
                static STRANGER: [Hash64; 1] = [Hash64::from_bytes([0x5A; 64])];
                p.palw_rcore_conservative_classes = &STRANGER;
                p.sync_palw_rcore_plus();
            }),
            "not one of testnet-12's genesis held rows",
        ),
        (
            "C7 without the fence",
            Box::new(|p| {
                p.palw_rcore_plus = None;
                p.sync_palw_rcore_plus();
                p.palw_rcore_conservative_classes = &PALW_T12_RCORE_CONSERVATIVE_CLASSES;
            }),
            "non-empty without palw_rcore_plus armed",
        ),
        (
            "mirrors without the fence",
            Box::new(|p| {
                p.palw_rcore_plus = None;
                p.palw_rcore_conservative_classes = &[];
            }),
            "carries R-core+ mirrors without palw_rcore_plus armed",
        ),
    ];
    for (name, edit, needle) in cases {
        let mut p = t12.clone();
        edit(&mut p);
        let why = refusal_text(p.validate_palw_rcore_plus_v1());
        assert!(why.contains(needle), "{name}: {why}");
        assert!(p.validate_palw_v2().is_err(), "{name}: validate_palw_v2 refuses it too");
    }
}

/// **The fence-off twin validates, and the fence is what moves testnet-12's ids.** testnet-12 with
/// `palw_rcore_plus = None` (C7 cleared, mirrors re-synced) is a runnable ruleset; `Some(never())`
/// is that twin; a future height is not yet a rule for the identity but is in the schedule; C7 alone
/// moves the params id. And an override carries the fence (S-SPEC §5), so an overridden testnet-12
/// fails on the prerequisites the override dropped instead of silently disarming.
#[test]
fn the_fence_off_twin_validates_and_the_fence_moves_only_testnet12() {
    let t12 = palw_t12_shipped_params();
    let mut twin = t12.clone();
    twin.palw_rcore_plus = None;
    twin.palw_rcore_conservative_classes = &[];
    twin.sync_palw_rcore_plus();
    twin.validate_palw_v2().expect("the fence-off twin is a runnable ruleset");
    assert_ne!(t12.consensus_params_id(), twin.consensus_params_id(), "testnet-12's ruleset names the fence");
    assert_ne!(t12.consensus_identity_id(), twin.consensus_identity_id(), "in force from block one separates identities");
    assert_ne!(t12.consensus_schedule_id(), twin.consensus_schedule_id(), "and so does its schedule");
    println!("testnet-12 params id with R-core+ {} / fence-off twin {}", t12.consensus_params_id(), twin.consensus_params_id());

    let mut never = twin.clone();
    never.palw_rcore_plus = Some(ForkActivation::never());
    assert_eq!(never.consensus_identity_id(), twin.consensus_identity_id(), "Some(never()) is absence");
    never.validate_palw_v2().expect("and validates as the twin does");

    let mut scheduled = twin.clone();
    scheduled.palw_rcore_plus = Some(ForkActivation::new(1_000));
    assert_eq!(scheduled.consensus_identity_id(), twin.consensus_identity_id(), "a future height is not yet a rule");
    assert_ne!(scheduled.consensus_schedule_id(), twin.consensus_schedule_id(), "but the schedule names it");

    let mut no_c7 = t12.clone();
    no_c7.palw_rcore_conservative_classes = &[];
    no_c7.sync_palw_rcore_plus();
    no_c7.validate_palw_v2().expect("an empty C7 is legal with the fence");
    assert_ne!(no_c7.consensus_params_id(), t12.consensus_params_id(), "a non-empty C7 is in the params id");
    assert_eq!(no_c7.consensus_schedule_id(), t12.consensus_schedule_id(), "and not in the schedule, which names heights");

    let overrides: OverrideParams = serde_json::from_str("{}").expect("an empty override");
    let overridden = t12.clone().override_params(overrides);
    assert_eq!(overridden.palw_rcore_plus, t12.palw_rcore_plus, "the override carries the fence");
    assert_eq!(overridden.palw_rcore_conservative_classes, t12.palw_rcore_conservative_classes, "and C7");
    assert!(overridden.validate_palw_v2().is_err(), "and then refuses the prerequisites it dropped, never disarming silently");
}
