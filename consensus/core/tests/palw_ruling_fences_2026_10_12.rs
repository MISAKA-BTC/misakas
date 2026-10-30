//! **The three fences of the user's 2026-10-12 rulings** (docs/design/palw/integration/mainnet-spec-register-2026-10-12.md §3):
//! `palw_court_model_scope_v1` (D2: no court route compels model bytes), `palw_public_evidence_reward_v1` (N4: a claim whose
//! evidence cannot be published earns no new reward or weight) and `palw_claim_hard_deadline_v1` (X3: a claim's dispute hard
//! deadline is fixed at acceptance and never extended).
//!
//! Each is a bare height, dormant everywhere and refused when armed until its rule is built — so on every ruleset that ships,
//! the live testnet-12 one included, nothing moves: no fingerprint, no schedule, no rule. These tests pin exactly that.

#![allow(warnings)]
use kaspa_consensus_core::config::params::{
    DEVNET_PARAMS, ForkActivation, MAINNET_PARAMS, Params, SIMNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS, devnet_shipped_params,
    mainnet_shipped_params, palw_rc_shipped_params, palw_t12_release_v6_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::network::{NetworkId, NetworkType};

const FENCES: [&str; 3] = ["palw_court_model_scope_v1", "palw_public_evidence_reward_v1", "palw_claim_hard_deadline_v1"];

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
        ("from(simnet)", Params::from(NetworkId::new(NetworkType::Simnet))),
        ("mainnet_shipped_params", mainnet_shipped_params()),
        ("devnet_shipped_params", devnet_shipped_params()),
        ("palw_rc_shipped_params", palw_rc_shipped_params()),
        ("palw_t12_shipped_params", palw_t12_shipped_params()),
    ]
}

fn set(p: &mut Params, name: &str, at: Option<ForkActivation>) {
    match name {
        "palw_court_model_scope_v1" => p.palw_court_model_scope_v1 = at,
        "palw_public_evidence_reward_v1" => p.palw_public_evidence_reward_v1 = at,
        "palw_claim_hard_deadline_v1" => {
            p.palw_claim_hard_deadline_v1 =
                at.map(kaspa_consensus_core::palw_ruling_fences_v1::PalwClaimHardDeadlineFenceV1::unapproved_probe_at)
        }
        other => panic!("not a ruling fence: {other}"),
    }
}

#[test]
fn every_shipped_ruleset_leaves_the_three_ruling_fences_dormant() {
    for (what, p) in rulesets() {
        let t12 = p.net == NetworkId::with_suffix(NetworkType::Testnet, 12);
        for name in FENCES {
            let expected = if t12 && name == "palw_claim_hard_deadline_v1" { Some(ForkActivation::new(17_400)) } else { None };
            assert!(p.palw_fences_v1().contains(&(name, expected)), "{what}: {name} has its declared schedule");
        }
        assert!(!p.palw_court_model_scope_v1_active_at(u64::MAX - 1), "{what}");
        assert!(!p.palw_public_evidence_reward_v1_active_at(u64::MAX - 1), "{what}");
        assert_eq!(p.palw_claim_hard_deadline_v1_active_at(u64::MAX - 1), t12, "{what}");
        p.validate_palw_2026_10_12_ruling_fences_v1().unwrap_or_else(|e| panic!("{what}: nothing to refuse: {e:?}"));
    }
}

#[test]
fn a_dormant_ruling_fence_moves_no_params_id_and_never_collapses_to_none() {
    // `consensus_params_id` normalises `Some(never())` to `None` (as for every fence); the schedule id is taken over the
    // presets as they are built, which never write `Some(never())` for these fences.
    let base = palw_t12_release_v6_params();
    let params_id = base.consensus_params_id();
    for name in FENCES {
        let mut never = base.clone();
        set(&mut never, name, Some(ForkActivation::never()));
        assert_eq!(never.consensus_params_id(), params_id, "{name}: Some(never()) hashes as None (Some-only)");
        never.validate_palw_2026_10_12_ruling_fences_v1().expect("never() is not armed");
    }
}

#[test]
fn an_armed_ruling_fence_is_named_in_both_fingerprints_and_refused_until_its_rule_is_built() {
    let base = palw_t12_shipped_params();
    for name in FENCES {
        let mut armed = base.clone();
        set(&mut armed, name, Some(ForkActivation::new(1_000)));
        assert_ne!(armed.consensus_params_id(), base.consensus_params_id(), "{name}: an armed height is in the params id");
        assert_ne!(armed.consensus_schedule_id(), base.consensus_schedule_id(), "{name}: and in the schedule id");
        assert!(armed.palw_fences_v1().contains(&(name, Some(ForkActivation::new(1_000)))), "{name}: the registry names it");
        let why = armed.validate_palw_2026_10_12_ruling_fences_v1().expect_err("refused while the rule is unbuilt");
        assert!(format!("{why:?}").contains(name), "{name}: the refusal names the fence: {why:?}");
    }
}
