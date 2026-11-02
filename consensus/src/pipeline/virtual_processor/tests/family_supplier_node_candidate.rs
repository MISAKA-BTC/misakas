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
    // Independent constructors collect a UTXO map into a Vec; its iteration
    // order is not part of the genesis commitment. Compare every exact output
    // in canonical outpoint order, without dropping duplicates or any value.
    let canonical_funds = |funds: &Premine| {
        let mut funds = funds.clone();
        funds.sort_by_key(|(outpoint, _)| (outpoint.transaction_id, outpoint.index));
        assert!(funds.windows(2).all(|pair| pair[0].0 != pair[1].0), "duplicate genesis outpoint");
        funds
    };
    assert_eq!(canonical_funds(&parts.2), canonical_funds(&original.2), "arming introduces no premine funds or collateral");
    assert_eq!(canonical_funds(&parts.3), canonical_funds(&original.3));
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

#[test]
fn family_joint_candidate_financial_hard_matches_its_actual_combined_clock() {
    use kaspa_consensus_core::palw_court_clock_v2::palw_claim_x3_cap_hard_at_checked_v2 as hard;
    let parts=family_native39_campaign_config_v2();
    let state=&parts.1.state;
    let x3=state.claim_hard_deadline_v1().expect("joint private candidate keeps native39 X3");
    assert_eq!(x3.from_daa,0);
    assert_eq!(x3.policy.prepare_daa+x3.policy.observe_daa+x3.policy.resolve_daa,300,"existing private native39 probe is preserved, not replaced by an invented economic policy");
    assert_eq!(state.court_clock_v2_from_daa(),Some(0),"the joint candidate explicitly composes the existing Court clock");
    assert_eq!(hard(state,0),Ok(Some(10_000)));
    assert_eq!(hard(state,7),Ok(Some(10_007)));
    let only=state.clone().with_court_clock_v2_from_daa(None);
    assert_eq!(hard(&only,7),Ok(Some(307)),"the same actual candidate with clock-v2 absent keeps X3-only semantics");
    assert_eq!(hard(&state.clone().with_claim_hard_deadline_v1(None),7),Ok(None));
}
