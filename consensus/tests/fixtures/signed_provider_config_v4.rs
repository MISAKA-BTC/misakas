//! Shared explicit private probe configuration for the signed-unit native fixture only.
//! Included by test modules in Consensus and kaspad; never part of public preset selection.
use kaspa_consensus_core::{
    config::{
        Config, ConfigBuilder,
        params::{ForkActivation, Params},
    },
    mldsa87_primitives::p2pkh_mldsa87_spk,
    muhash::MuHashExtensions,
    network::{NetworkId, NetworkType},
    palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2},
    palw_state_v2::PalwConsensusObjectV2 as Obj,
    tx::{TransactionOutpoint, UtxoEntry},
};
use kaspa_hashes::Hash64;
use kaspa_muhash::MuHash;
type Premine = Vec<(TransactionOutpoint, UtxoEntry)>;
pub fn seed(card: usize) -> [u8; 32] {
    let mut s = [0xB0; 32];
    s[0] += card as u8;
    s
}
fn card_pubkey(card: usize) -> Vec<u8> {
    libcrux_ml_dsa::ml_dsa_87::generate_key_pair(seed(card)).verification_key.as_ref().to_vec()
}
fn card_payout_payload(card: usize) -> Hash64 {
    kaspa_hashes::blake2b_512_address_payload(&card_pubkey(card))
}
fn card_payout_spk(card: usize) -> kaspa_consensus_core::tx::ScriptPublicKey {
    p2pkh_mldsa87_spk(card_payout_payload(card).as_byte_slice())
}
fn base() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let shipped = kaspa_consensus_core::config::params::palw_t12_launch_params_v1();
    let keep_evm = false;
    use kaspa_consensus_core::config::params::PALW_T12_GENESIS_BONDS;
    use kaspa_consensus_core::config::premine::{PALW_RC_BOND_FEE_FLOAT_SOMPI, genesis_premine_utxos_for, premine_outpoint_for};
    assert!(!keep_evm || cfg!(feature = "evm"), "a build without the `evm` feature cannot build a template for an active EVM lane");
    debug_assert_eq!(
        shipped.genesis.hash,
        Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12)).genesis.hash,
        "the network id's genesis"
    );
    let mut params = shipped.clone();
    assert_eq!(PALW_T12_GENESIS_BONDS.len(), 8);

    // The cards: keys and payouts only.
    {
        let PalwConsensusMode::ConsensusV2(bundle) = &mut params.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
        let mut row = 0usize;
        for object in bundle.genesis_objects.iter_mut() {
            if let Obj::BondRegistered { bond, pubkey, payout_payload, .. } = object {
                assert_eq!(
                    bond.0,
                    premine_outpoint_for(shipped.net, PALW_T12_GENESIS_BONDS[row].premine_index),
                    "card {row} keeps its collateral outpoint"
                );
                *pubkey = card_pubkey(row);
                *payout_payload = card_payout_payload(row);
                row += 1;
            }
        }
        assert_eq!(row, 8, "testnet-12 registers eight genesis bonds");
    }

    // The premine: each card's fee float, re-addressed to that card's harness payout.
    let mut utxos: Vec<(TransactionOutpoint, UtxoEntry)> = genesis_premine_utxos_for(params.net).into_iter().collect();
    let mut floats: Vec<Option<(TransactionOutpoint, UtxoEntry)>> = vec![None; 8];
    for (outpoint, entry) in utxos.iter_mut() {
        for (i, card) in PALW_T12_GENESIS_BONDS.iter().enumerate() {
            if entry.amount == PALW_RC_BOND_FEE_FLOAT_SOMPI && entry.script_public_key == p2pkh_mldsa87_spk(&card.payout_payload) {
                assert!(floats[i].is_none(), "one float per card");
                entry.script_public_key = card_payout_spk(i);
                floats[i] = Some((*outpoint, entry.clone()));
            }
        }
    }
    let floats: Vec<_> =
        floats.into_iter().enumerate().map(|(i, f)| f.unwrap_or_else(|| panic!("card {i} has a fee float"))).collect();
    let mut multiset = MuHash::new();
    for (outpoint, entry) in &utxos {
        multiset.add_utxo(outpoint, entry);
    }
    params.genesis.utxo_commitment = multiset.finalize();
    params.genesis.hash = kaspa_consensus_core::header::Header::from(&params.genesis).hash;

    // The divergence is stated, not assumed: testnet-12 ships the lane at genesis.
    assert_eq!(shipped.evm_activation_daa_score, 0, "testnet-12 ships the EVM lane active from genesis");
    let config = ConfigBuilder::new(params)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            if !keep_evm {
                p.evm_activation_daa_score = u64::MAX;
                // ADR-0089 Decision 9: the market's EVM face may not be armed on a lane made inert.
                p.palw_model_evm = None;
            }
        })
        .build();
    config.params.validate_palw_v2().expect("testnet-12 with harness cards is a runnable ruleset");
    assert_eq!(config.params.is_evm_active(0), keep_evm, "the EVM lane is as asked");

    // What this test leans on is testnet-12's, and is asserted rather than assumed.
    assert_eq!(config.params.palw_execution_lane, shipped.palw_execution_lane, "the lane as shipped");
    assert_eq!(config.params.palw_execution_quanta, shipped.palw_execution_quanta);
    assert!(config.params.palw_economic_safety.is_some_and(|f| f.is_active(0)), "ADR-0151's bundle is armed from genesis");
    assert!(config.params.palw_audit_2026_09_23.is_some_and(|f| f.is_active(0)), "the 2026-09-23 audit fence is armed from genesis");
    assert_eq!(config.params.target_time_per_block(), shipped.target_time_per_block());
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!() };
    let PalwConsensusMode::ConsensusV2(shipped_bundle) = &shipped.palw_consensus_mode else { unreachable!() };
    assert_eq!(bundle.state, shipped_bundle.state, "every window as shipped");
    assert_eq!(bundle.panel, shipped_bundle.panel, "the panel as shipped");
    let bundle = bundle.clone();
    (config, bundle, utxos, floats)
}
pub fn config() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    use kaspa_consensus_core::{
        palw_beacon_source_fee_v1::PalwBeaconSourceFeeFenceV1,
        palw_beacon_source_opt_in_v1::PalwBeaconSourceFenceV1,
        palw_claim_challenge_v2::PalwClaimChallengeFenceV2,
        palw_material_release_recovery_v2::PalwMaterialReleaseRecoveryFenceV2,
        palw_monetary_material_v1::{PalwMonetaryMaterialFenceV1, PalwMonetaryMaterialPolicyV1},
        palw_panel_free_v1::PalwPanelFreeFenceV1,
    };
    let (c, _, premine, floats) = base();
    let mut p = c.params;
    p.palw_probabilistic_constraints_v1 = Some(ForkActivation::new(0));
    p.palw_panel_free_v1 = Some(PalwPanelFreeFenceV1::interim_v1(ForkActivation::new(1), Vec::new()));
    p.arm_palw_opv_mechanics_budget_v1();
    p.palw_reorg_strict_economic_win = Some(ForkActivation::new(0));
    let mut paid = PalwClaimChallengeFenceV2::unapproved_probe_at(ForkActivation::new(1));
    paid.policy.bootstrap_seed = Some([0xB5; 64]);
    p.palw_claim_challenge_v2 = Some(paid);
    p.palw_beacon_source_opt_in_v1 = Some(PalwBeaconSourceFenceV1::unapproved_probe_at(ForkActivation::new(1)));
    p.palw_beacon_source_fee_v1 = Some(PalwBeaconSourceFeeFenceV1::unapproved_probe_at(ForkActivation::new(1)));
    p.palw_provider_court_v1 = Some(ForkActivation::new(1));
    p.palw_reporter_income_v2 = Some(ForkActivation::new(1));
    p.palw_material_income_v3 = Some(ForkActivation::new(1));
    p.palw_court_commitment_scope_v1 = Some(ForkActivation::new(1));
    let window = p.palw_bond_budget_v1.as_ref().unwrap().policy.window_daa;
    p.palw_monetary_material_v1 = Some(PalwMonetaryMaterialFenceV1 {
        activation: ForkActivation::new(1),
        policy: PalwMonetaryMaterialPolicyV1 {
            version: 1,
            max_contracts: 64,
            max_contracts_per_provider: 32,
            max_bytes_per_contract: 1_000_000,
            max_bytes_per_provider: 32_000_000,
            minimum_collateral_sompi: 100_000_000,
            minimum_fee_sompi: 1_000_000,
            window_daa: window,
            response_window_daa: 64,
            max_enrollment_daa: 200,
            cost_report_root: Hash64::from_bytes([0xD4; 64]),
        },
    });
    p.palw_material_release_recovery_v2 = Some(PalwMaterialReleaseRecoveryFenceV2::unapproved_probe_at(ForkActivation::new(1)));
    p.blockrate.finality_depth = 20;
    p.sync_palw_claim_challenge_v2();
    p.sync_palw_bond_budget_v1();
    assert!(p.validate_palw_v2().is_err());
    assert!(p.validate_palw_monetary_material_v1().is_err());
    assert!(p.validate_palw_material_release_recovery_v2().is_err());
    let PalwConsensusMode::ConsensusV2(b) = &p.palw_consensus_mode else { unreachable!() };
    let b = b.clone();
    (Config::new(p), b, premine, floats)
}

#[derive(borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct CheckerHandoff {
    pub writer_pid: u32,
    pub checkpoint: Hash64,
    pub daa: u64,
    pub root: Hash64,
    pub enrollment: Hash64,
    pub consent: Hash64,
    pub checker: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    pub provider: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    pub checker_card: u32,
    pub provider_card: u32,
    pub funding: TransactionOutpoint,
}
