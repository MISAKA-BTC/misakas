//! **Nothing the 2026-09-19/20 reward work added is live, and this is where that is checked.**
//!
//! The re-audit's most important sentence is not a finding: every fence this work adds is `None` on
//! every shipped preset, so testnet-11 behaves exactly as it did before any of it. That claim is
//! easy to make and easy to be wrong about — a fence that reaches the fingerprint changes network
//! identity on deploy day whether or not it is armed, and a fence that is armed by accident changes
//! consensus. So it is pinned to the number itself.
//!
//! If a change is meant to be dormant and this file goes red, the change is not dormant. If a
//! change is meant to ARM, this file is the one to update deliberately, in the commit that arms it,
//! with the new height and the new fingerprint written down together.

use kaspa_consensus_core::config::params::{MAINNET_PARAMS, palw_rc_shipped_params};

/// The shipped release's identity, unchanged since the 2026-09-19 roll.
const T11_CONSENSUS_PARAMS_ID: &str = "c3a5e91dfc9336b02d2280ccb10327e19058123b8589da2d0aa0754f719e9a5f";
const T11_CONSENSUS_IDENTITY_ID: &str = "12e975effe2ef067e039c07b1af4199b7c4122068da7ccc2dda989cf3f4ec4d2";
const T11_CONSENSUS_SCHEDULE_ID: &str = "7494fb6a98c0467b7a7baf1be0e2614a85a6c7762f14bd621a209c7a268b596e";
const MAINNET_CONSENSUS_PARAMS_ID: &str = "badaa8e90f14ef0074048d6b18660864855be8ab854d0ecb01dfbb62171538e1";

#[test]
fn the_shipped_release_fingerprint_did_not_move() {
    let rc = palw_rc_shipped_params();
    assert_eq!(rc.consensus_params_id().to_string(), T11_CONSENSUS_PARAMS_ID, "the ruleset a node announces");
    assert_eq!(rc.consensus_identity_id().to_string(), T11_CONSENSUS_IDENTITY_ID, "the identity two nodes must share to peer");
    assert_eq!(rc.consensus_schedule_id().to_string(), T11_CONSENSUS_SCHEDULE_ID, "the schedule the operator log names");
    assert_eq!(
        MAINNET_PARAMS.consensus_params_id().to_string(),
        MAINNET_CONSENSUS_PARAMS_ID,
        "and mainnet, which this work must not have touched at all"
    );
}

/// **Every fence the reward work added is dormant on every preset**, read at runtime rather than
/// off the base constants — the process error that produced a false CRITICAL in the first round.
#[test]
fn every_fence_the_reward_work_added_is_dormant_everywhere() {
    use kaspa_consensus_core::config::params::{DEVNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS};
    for (net, p) in [
        ("shipped release", palw_rc_shipped_params()),
        ("mainnet", MAINNET_PARAMS),
        ("testnet", TESTNET_PARAMS),
        ("simnet", SIMNET_PARAMS),
        ("devnet", DEVNET_PARAMS),
    ] {
        for (name, fence) in [
            ("palw_canonical_work", p.palw_canonical_work),
            ("palw_admission_independence", p.palw_admission_independence),
            ("palw_fp_derived_work", p.palw_fp_derived_work),
        ] {
            assert_eq!(fence.map(|f| f.daa_score()), None, "{net}: {name} must be dormant until a height is chosen");
        }
    }
}

/// **And the bundle is what an arming build has to satisfy**, so the height cannot be chosen for
/// one fence in isolation later. The shipped release does not satisfy it — it arms none of the
/// three and `palw_artifact_root_ownership` is commented out on its card — which is exactly why a
/// build that wants the economy has to change more than one line.
#[test]
fn arming_one_fence_of_the_bundle_on_the_release_is_refused() {
    use kaspa_consensus_core::config::params::ForkActivation;
    let rc = palw_rc_shipped_params();
    let registry = rc.palw_model_registry.expect("the shipped release arms the registry").daa_score();

    let mut one = rc.clone();
    one.palw_canonical_work = Some(ForkActivation::new(registry + 1_000));
    let refusal = one.validate_palw_v2().expect_err("one fence of the bundle, on the release, is refused");
    assert!(format!("{refusal:?}").contains("arm together or not at all"), "{refusal:?}");

    let mut three = rc.clone();
    for f in [&mut three.palw_canonical_work, &mut three.palw_admission_independence, &mut three.palw_fp_derived_work] {
        *f = Some(ForkActivation::new(registry + 1_000));
    }
    let refusal = three.validate_palw_v2().expect_err("and three without ADR-0143 is still refused");
    assert!(format!("{refusal:?}").contains("palw_artifact_root_ownership"), "{refusal:?}");

    three.palw_artifact_root_ownership = Some(ForkActivation::new(registry));
    three.validate_palw_v2().expect("with ADR-0143 at or below it, the bundle assembles");
}
