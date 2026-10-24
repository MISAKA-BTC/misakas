//! Construction/identity controls only. Explicit files are mechanics inputs,
//! not source independence, execution campaigns or approved economic policy.
use super::*;
use kaspa_consensus_core::config::drill::PalwDrillSaltV1;
use misaka_palw_kernel::claim_paid_bootstrap_source_v6::PaidBootstrapSourcePolicyV6;
use std::path::PathBuf;

fn inputs() -> (tempfile::TempDir, PalwDrillSaltV1, PathBuf, PathBuf) {
    let directory = tempfile::tempdir_in(std::fs::canonicalize(std::env::temp_dir()).unwrap()).unwrap();
    let salt = PalwDrillSaltV1::from_bytes([0x67; 32]).unwrap();
    let base = private_paid_claim_params_v4(&salt).unwrap();
    let mut ruleset = [0; 64];
    ruleset[..32].copy_from_slice(&base.consensus_identity_id().as_bytes());
    let network =
        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(base.net.to_string().as_bytes(), Some(base.genesis.hash));
    let source = PalwSelectedSourceBootstrapPolicyV1 {
        version: 1,
        chain_genesis: base.genesis.hash,
        network,
        ruleset: kaspa_consensus_core::Hash64::from_bytes(ruleset),
        epoch: 1,
        freeze_cutoff_daa: 10,
        anchor_cutoff_daa: 20,
        starts_at_daa: 30,
        ends_at_daa: 100,
        stake_release_daa: 3000,
        // Canonical fixed-field Borsh input avoids adding a node dependency
        // solely to name the nested challenge policy type in this fixture.
        selection: borsh::from_slice(&borsh::to_vec(&(1u16, 3u16, 2u16, 10u32, 64u16, 2u32)).unwrap()).unwrap(),
        source_profile: kaspa_consensus_core::Hash64::from_bytes([11; 64]),
        checker_scope: kaspa_consensus_core::Hash64::from_bytes([12; 64]),
        maximum_claim_bytes: 4096,
        maximum_check_work: 1000,
        maximum_history_headers: 64,
        security_review: kaspa_consensus_core::Hash64::from_bytes([13; 64]),
        economic_review: kaspa_consensus_core::Hash64::from_bytes([14; 64]),
    };
    source.validate().unwrap();
    let consumer = PaidBootstrapSourcePolicyV6 {
        version: 6,
        network: network.as_bytes(),
        bootstrap_policy: source.id().as_bytes(),
        required_sources: 1,
        maximum_events: 2,
        anchor_delay: 1,
        window: 10,
        settlement_depth: 1,
        maximum_consumers: 8,
        source_liability_slack: 1,
    };
    consumer.validate().unwrap();
    let source_file = directory.path().join("source.borsh");
    let consumer_file = directory.path().join("consumer.borsh");
    std::fs::write(&source_file, borsh::to_vec(&(5u64, source)).unwrap()).unwrap();
    std::fs::write(&consumer_file, borsh::to_vec(&(40u64, consumer)).unwrap()).unwrap();
    (directory, salt, source_file, consumer_file)
}

#[test]
fn explicit_source_paid_v6_constructs_without_retired_compact_or_optional_economics() {
    let (_directory, salt, source_file, consumer_file) = inputs();
    let baseline = private_source_bootstrap_paid_params_with_consumer_v6(&salt, &source_file, None).unwrap();
    let combined = private_source_bootstrap_paid_params_with_consumer_v6(&salt, &source_file, Some(&consumer_file)).unwrap();
    assert!(has_private_paid_v4_scope_v1(&combined));
    assert!(combined.palw_claim_compact_v4.is_none());
    assert!(combined.palw_source_verifier_economics_v2.is_none());
    assert_eq!(combined.palw_claim_challenge_v2.unwrap().activation, ForkActivation::new(0));
    assert_eq!(combined.palw_material_release_recovery_v2.as_ref().unwrap().activation, ForkActivation::new(0));
    validate_private_source_bootstrap_relations_v1(&combined, &salt).unwrap();
    assert_ne!(baseline.consensus_params_id(), combined.consensus_params_id());
    assert_eq!(baseline.consensus_identity_id(), combined.consensus_identity_id());
    // Private construction never removes either ordinary public-startup refusal.
    assert!(combined.validate_palw_selected_source_bootstrap_v1().is_err());
    assert!(combined.validate_palw_paid_bootstrap_source_v6().is_err());
}

#[test]
fn source_paid_scope_and_explicit_source_consumer_policy_drift_refuse() {
    let (_directory, salt, source_file, consumer_file) = inputs();
    let valid = private_source_bootstrap_paid_params_with_consumer_v6(&salt, &source_file, Some(&consumer_file)).unwrap();
    let mut inactive = valid.clone();
    inactive.palw_claim_challenge_v2.as_mut().unwrap().activation = ForkActivation::never();
    inactive.sync_palw_claim_challenge_v2();
    assert!(!has_private_paid_v4_scope_v1(&inactive));
    assert!(validate_private_source_bootstrap_relations_v1(&inactive, &salt).is_err());
    let mut recovery = valid.clone();
    recovery.palw_material_release_recovery_v2.as_mut().unwrap().policy.max_provider_streams -= 1;
    assert!(validate_private_source_bootstrap_relations_v1(&recovery, &salt).is_err(), "exact paid-v4 fingerprint is required");
    let mut consumer = valid.clone();
    consumer.palw_paid_bootstrap_source_v6.as_mut().unwrap().policy.maximum_events += 1;
    consumer.sync_palw_paid_bootstrap_source_v6();
    assert!(validate_private_source_bootstrap_relations_v1(&consumer, &salt).is_err());
    let (_, mut wrong): (u64, PaidBootstrapSourcePolicyV6) = borsh::from_slice(&std::fs::read(&consumer_file).unwrap()).unwrap();
    wrong.network = [99; 64];
    std::fs::write(&consumer_file, borsh::to_vec(&(40u64, wrong)).unwrap()).unwrap();
    assert!(private_source_bootstrap_paid_params_with_consumer_v6(&salt, &source_file, Some(&consumer_file)).is_err());
    std::fs::write(&source_file, [255]).unwrap();
    assert!(private_source_bootstrap_paid_params_with_consumer_v6(&salt, &source_file, None).is_err());
}

#[test]
fn explicit_optional_economics_preserves_private_source_paid_scope_and_refuses_drift() {
    use kaspa_consensus_core::palw_source_verifier_economics_v2::PalwSourceVerifierEconomicsPolicyV2;
    let (directory, salt, source_file, consumer_file) = inputs();
    let baseline = private_source_bootstrap_paid_params_with_consumer_v6(&salt, &source_file, Some(&consumer_file)).unwrap();
    let source = baseline.palw_selected_source_bootstrap_v1.unwrap();
    // Explicit mechanics inputs only; these values are neither defaults nor
    // approved source economics or campaign acceptance evidence.
    let policy = PalwSourceVerifierEconomicsPolicyV2 {
        version: 2,
        network: source.policy.network,
        bootstrap_policy: source.policy.id(),
        exposure_per_work_sompi: 7,
        minimum_fee_per_checker_sompi: 5,
        maximum_funded_works: source.policy.selection.maximum_source_works,
        maximum_liabilities: 4,
    };
    let economics_file = directory.path().join("economics.borsh");
    std::fs::write(&economics_file, borsh::to_vec(&(5u64, policy)).unwrap()).unwrap();
    let combined = private_source_verifier_economics_params_v2(baseline.clone(), &salt, &economics_file).unwrap();
    let economics = combined.palw_source_verifier_economics_v2.unwrap();
    assert_eq!(economics.activation, ForkActivation::new(5));
    assert_eq!(economics.policy, policy);
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &combined.palw_consensus_mode else {
        panic!("private source-paid profile must retain its native state");
    };
    assert!(bundle.state.selected_source_economics_v2_mirror_matches(5, &policy));
    assert!(has_private_paid_v4_scope_v1(&combined));
    assert!(combined.palw_claim_compact_v4.is_none());
    validate_private_source_bootstrap_relations_v1(&combined, &salt).unwrap();
    assert_ne!(combined.consensus_params_id(), baseline.consensus_params_id());
    assert_eq!(combined.consensus_identity_id(), baseline.consensus_identity_id());
    assert!(combined.validate_palw_source_verifier_economics_v2().is_err());
    assert!(combined.validate_palw_selected_source_bootstrap_v1().is_err());
    assert!(combined.validate_palw_paid_bootstrap_source_v6().is_err());

    let mut wrong_network = policy;
    wrong_network.network = kaspa_consensus_core::Hash64::from_bytes([99; 64]);
    let mut wrong_source = policy;
    wrong_source.bootstrap_policy = kaspa_consensus_core::Hash64::from_bytes([98; 64]);
    let mut zero_fee = policy;
    zero_fee.minimum_fee_per_checker_sompi = 0;
    for (activation, invalid, label) in [
        (5, wrong_network, "network"),
        (5, wrong_source, "bootstrap policy"),
        (5, zero_fee, "zero fee"),
        (source.policy.freeze_cutoff_daa + 1, policy, "late activation"),
    ] {
        std::fs::write(&economics_file, borsh::to_vec(&(activation, invalid)).unwrap()).unwrap();
        assert!(
            private_source_verifier_economics_params_v2(baseline.clone(), &salt, &economics_file).is_err(),
            "invalid explicit economics {label} must refuse"
        );
    }
    std::fs::write(&economics_file, [255]).unwrap();
    assert!(private_source_verifier_economics_params_v2(baseline, &salt, &economics_file).is_err());
}
