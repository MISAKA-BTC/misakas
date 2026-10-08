//! **RFC-0008 v2: the `palw_exec_payload_v2` fence** — dormant on every ruleset a node can run (testnet-12 included), fingerprinted
//! with its value where it is set (Some-only, so every dormant ruleset's identity is the one it had before the field existed),
//! `Some(never())` collapsing out of every id, and **refused by `validate_palw_v2` at any real height** until the section 9 gates
//! of `docs/design/palw/rfc-0008-implementation-spec.md` pass (`PALW_EXEC_PAYLOAD_V2_ARMABLE`).
//!
//! The fold's own tests (`palw_state_v2::tests::exec_v2_fold_v1`) arm the fence on the V2 bundle's state params directly, which is
//! how a test arms what validation refuses; this file checks the field, the hashers and the validator, and adds no id pin of its
//! own (a quoted id here would be one `t12-repin.sh` does not know).
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_exec_payload_v2_fence`

use kaspa_consensus_core::config::params::{
    DEVNET_PARAMS, ForkActivation, MAINNET_PARAMS, Params, SIMNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS, devnet_shipped_params,
    mainnet_shipped_params, palw_rc_shipped_params, palw_t12_launch_params_v1, palw_t12_release_v1_params, palw_t12_release_v2_params,
    palw_t12_release_v3_params, palw_t12_release_v5_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_exec_v2::PALW_EXEC_PAYLOAD_V2_ARMABLE;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn mirror(p: &Params) -> Option<u64> {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => bundle.state.exec_v2_from_daa(),
        _ => None,
    }
}

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
        ("palw_t12_release_v5_params", palw_t12_release_v5_params()),
        ("palw_t12_launch_params_v1", palw_t12_launch_params_v1()),
        ("palw_t12_release_v1_params", palw_t12_release_v1_params()),
        ("palw_t12_release_v2_params", palw_t12_release_v2_params()),
        ("palw_t12_release_v3_params", palw_t12_release_v3_params()),
    ]
}

#[test]
fn the_gates_are_open_so_the_fence_is_not_armable() {
    assert!(!PALW_EXEC_PAYLOAD_V2_ARMABLE, "section 9's gates are open: arming is refused on every ruleset");
}

#[test]
fn every_ruleset_leaves_it_dormant() {
    for (name, p) in rulesets() {
        assert!(p.palw_exec_payload_v2.is_none(), "{name}: dormant");
        assert!(!p.palw_exec_payload_v2_active_at(u64::MAX), "{name}: the accessor agrees at every height");
        assert_eq!(mirror(&p), None, "{name}: no mirror");
        assert!(p.palw_fences_v1().contains(&("palw_exec_payload_v2", None)), "{name}: the exhaustive fence list names it");
        assert!(p.validate_palw_exec_payload_v2().is_ok(), "{name}: nothing to refuse");
        if matches!(p.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            p.validate_palw_v2().unwrap_or_else(|e| panic!("{name}: {e}"));
        }
    }
}

#[test]
fn never_is_the_dormant_spelling_and_collapses_out_of_every_id() {
    for (name, base) in [("t12 shipped", palw_t12_shipped_params()), ("t12 v5", palw_t12_release_v5_params())] {
        let mut never = base.clone();
        never.palw_exec_payload_v2 = Some(ForkActivation::never());
        assert_eq!(ids(&never), ids(&base), "{name}: Some(never()) fingerprints exactly as None");
        assert!(never.validate_palw_exec_payload_v2().is_ok(), "{name}: dormant passes the validator");
        assert!(!never.palw_exec_payload_v2_active_at(u64::MAX), "{name}: never is never active");
    }
}

#[test]
fn a_set_height_is_fingerprinted_in_the_ruleset_and_the_schedule_and_never_the_identity() {
    let base = palw_t12_release_v5_params();
    let before = ids(&base);
    let mut armed = base.clone();
    armed.palw_exec_payload_v2 = Some(ForkActivation::new(9_000));
    let after = ids(&armed);
    assert_ne!(after.0, before.0, "the ruleset a node announces names the fence and its value");
    assert_eq!(after.1, before.1, "two builds that differ only about a FUTURE height stay peers");
    assert_ne!(after.2, before.2, "the schedule reports it");
    // Two different heights are two different rulesets.
    let mut other = base.clone();
    other.palw_exec_payload_v2 = Some(ForkActivation::new(9_001));
    assert_ne!(ids(&other).0, after.0);
    assert_ne!(ids(&other).2, after.2);
    assert!(armed.palw_exec_payload_v2_active_at(9_000) && !armed.palw_exec_payload_v2_active_at(8_999));
}

#[test]
fn every_real_height_is_refused_by_name_until_the_gates_pass() {
    for (name, base) in [
        ("t12 shipped", palw_t12_shipped_params()),
        ("t12 v5", palw_t12_release_v5_params()),
        ("devnet shipped", devnet_shipped_params()),
        ("palw rc", palw_rc_shipped_params()),
    ] {
        for at in [0u64, 1, 750, 9_000, 1_234_567, u64::MAX - 1] {
            let mut armed = base.clone();
            armed.palw_exec_payload_v2 = Some(ForkActivation::new(at));
            // Mirrored or not, a real height is refused: first by the mirror disagreeing, and once mirrored by the gates.
            let unsynced = armed.validate_palw_exec_payload_v2();
            if matches!(armed.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
                assert!(unsynced.unwrap_err().to_string().contains("mirror"), "{name}@{at}: an unsynced mirror is refused");
                armed.sync_palw_exec_payload_v2();
                assert_eq!(mirror(&armed), Some(at), "{name}@{at}: the mirror follows");
            }
            let refused = armed.validate_palw_exec_payload_v2().expect_err("a real height is refused");
            let why = refused.to_string();
            if matches!(armed.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
                assert!(why.contains("cannot be armed") && why.contains("section 9"), "{name}@{at}: {why}");
                assert!(armed.validate_palw_v2().is_err(), "{name}@{at}: the whole ruleset is refused");
            } else {
                assert!(why.contains("mirror") || why.contains("cannot be armed"), "{name}@{at}: {why}");
            }
        }
    }
}

#[test]
fn a_mirror_without_the_field_is_refused() {
    let mut p = palw_t12_shipped_params();
    if let PalwConsensusMode::ConsensusV2(bundle) = &mut p.palw_consensus_mode {
        bundle.state = bundle.state.clone().with_exec_v2_from_daa(Some(10));
    }
    assert!(p.validate_palw_exec_payload_v2().unwrap_err().to_string().contains("mirror"));
    assert!(p.validate_palw_v2().is_err());
    p.sync_palw_exec_payload_v2();
    assert_eq!(mirror(&p), None);
    assert!(p.validate_palw_exec_payload_v2().is_ok());
}
