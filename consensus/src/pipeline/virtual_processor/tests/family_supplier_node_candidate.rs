//! One explicit experimental candidate for the joint family/native39/stage2
//! campaign. It is not a shipped activation or a substitute for its readiness
//! records. All changes happen before genesis or any original is executed.
use kaspa_consensus_core::config::Config;
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};

type Premine = Vec<(TransactionOutpoint, UtxoEntry)>;

pub(crate) fn family_native39_campaign_config_v2() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    use kaspa_consensus_core::config::params::{
        PALW_T12_G14_17000_FENCES_V1, palw_t12_arm_after_int13_at_v1, palw_t12_arm_full_activation_at_v1,
    };
    let mut parts = super::g14_kernel_route_e2e::model_conformance::flat_native_v9::funded11::owning39::master::node_e2e::native39_node_campaign_config_v1(true, true);
    let budget = parts.0.params.palw_bond_budget_v1.clone().expect("native39 shared V3 budget");
    assert!(budget.policy.slice_rights_by_rho, "formula2 keeps sliced rights");
    palw_t12_arm_after_int13_at_v1(&mut parts.0.params, PALW_T12_G14_17000_FENCES_V1, Some(0));
    palw_t12_arm_full_activation_at_v1(&mut parts.0.params, Some(0));
    // Full activation has its own budget estimate. This joint campaign uses
    // the SAME pre-existing V3 formula2 policy as its native39 construction.
    parts.0.params.palw_bond_budget_v1 = Some(budget);
    parts.0.params.sync_palw_bond_budget_v1();
    parts.0.params.sync_palw_g14_gates_v1();
    let PalwConsensusMode::ConsensusV2(bundle) = &parts.0.params.palw_consensus_mode else {
        panic!("joint candidate requires consensus V2")
    };
    parts.1 = bundle.clone();
    assert!(parts.1.state.family_supplier_active_at(0));
    assert!(parts.1.state.claim_local_settlement_active_at(0));
    assert!(parts.1.state.bond_budget_active_at(0));
    assert!(parts.1.state.settlement_once_active_at(0));
    assert!(parts.0.params.palw_tir_public_conviction_v1.is_some_and(|f| f.is_active(0)));
    assert!(parts.0.params.palw_fp_answer_accounting_v1.is_some_and(|f| f.is_active(0)));
    parts
}

#[test]
fn family_native39_joint_candidate_preserves_real_funding_and_policy_domain() {
    let parts = family_native39_campaign_config_v2();
    let original = super::g14_kernel_route_e2e::model_conformance::flat_native_v9::funded11::owning39::master::node_e2e::native39_node_campaign_config_v1(true, true);
    assert_eq!(parts.0.params.genesis.hash, original.0.params.genesis.hash);
    assert_eq!(parts.2, original.2, "arming introduces no premine funds or collateral");
    assert_eq!(parts.3, original.3);
    let network = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        parts.0.params.net.to_string().as_bytes(),
        Some(parts.0.params.genesis.hash),
    );
    assert_eq!(parts.0.params.palw_native_execution_v39.as_ref().unwrap().policies.network_domain, network.as_bytes());
    assert!(parts.0.params.palw_bond_budget_v1.as_ref().unwrap().policy.slice_rights_by_rho);
    let key = crate::consensus::test_consensus::TestConsensus::palw_v2_registry_keypair(0).verification_key.as_ref();
    let producer = parts
        .1
        .genesis_objects
        .iter()
        .find_map(|object| match object {
            kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::BondRegistered { bond, pubkey, .. }
                if pubkey.as_slice() == key =>
            {
                Some(*bond)
            }
            _ => None,
        })
        .expect("actual funded native card0 bond exists before any original");
    for card in 0..8 {
        let key = crate::consensus::test_consensus::TestConsensus::palw_v2_registry_keypair(card).verification_key.as_ref();
        let bond = parts
            .1
            .genesis_objects
            .iter()
            .find_map(|object| match object {
                kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::BondRegistered { bond, pubkey, .. }
                    if pubkey.as_slice() == key =>
                {
                    Some(*bond)
                }
                _ => None,
            })
            .unwrap();
        eprintln!("[family-joint-executor] card={card} bond={}:{}", bond.0.transaction_id, bond.0.index);
    }
    eprintln!(
        "[family-joint-candidate] network-domain={network} genesis={} executor-test-card=0 executor-bond={}:{}",
        parts.0.params.genesis.hash, producer.0.transaction_id, producer.0.index
    );
}
