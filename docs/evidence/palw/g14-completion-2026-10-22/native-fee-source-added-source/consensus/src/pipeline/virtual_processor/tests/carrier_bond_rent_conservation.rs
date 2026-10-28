//! Typed released/slashed bond fixture; real payer signature, UTXO validation, codec and PALW gate.
//! This does not claim a mined released-bond end-to-end chain or a new fee policy.
#![allow(warnings)]
use super::super::utxo_validation::{
    BondSpendFilter, palw_carrier_rent_available_v1, palw_finality_collectible_fee_v1, palw_record_accepted_carrier_fees_v1,
};
use super::*;
use kaspa_consensus_core::{
    config::params::ForkActivation,
    hashing::{
        sighash::{Mldsa87SigHashReusedValuesUnsync, calc_mldsa87_signature_hash},
        sighash_type::SIG_HASH_ALL,
    },
    mass::MassCalculator,
    palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2, palw_lifecycle_objects_from_accepted_txs_v2},
    palw_mode_v2::PalwConsensusMode,
    palw_state_v2::{
        PalwBlockContextV2, PalwBondStatusV2, PalwConsensusObjectV2, PalwStateCarriageV2, palw_object_chunk_group_rent_v1,
    },
    subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
    tx::{PopulatedTransaction, TransactionInput, TransactionOutput, UtxoEntry},
    utxo::utxo_collection::UtxoCollection,
};
use kaspa_txscript::{MLDSA87_TX_CONTEXT, script_builder::ScriptBuilder};
use libcrux_ml_dsa::ml_dsa_87 as mldsa;
use std::collections::HashMap;

#[tokio::test]
async fn palw_bond_burn_and_object_rent_cannot_spend_the_same_actual_fee() {
    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle_funded_for(&catalog, 64);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params({
            let bundle = bundle.clone();
            move |p| {
                p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
                p.palw_certification_rent = Some(ForkActivation::always());
                *p = p.clone().with_palw_v2_cadence();
            }
        })
        .build();
    let ctx = TestContext::new(TestConsensus::new(&config));
    let vp = ctx.consensus.virtual_processor();
    let (_, initial) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().unwrap();
    let kp = mldsa::generate_key_pair([0x73; 32]);
    let pubkey = kp.verification_key.as_ref().to_vec();
    let payout = kaspa_hashes::blake2b_512_address_payload(&pubkey);
    let spk = p2pkh_mldsa87_spk(&payout.as_bytes());
    let rent = palw_object_chunk_group_rent_v1();
    assert!(rent > 0);
    let now = 10_000_000;
    let (mut block_collectible, mut block_finality) = (0u64, 0u64);
    for (case, burn, fee, priced, accepted_expected) in [
        (0, rent, rent, true, false),
        (1, rent, rent.checked_mul(2).unwrap(), true, true),
        (2, 0, rent, true, true),
        (3, rent, rent, false, true),
        (4, rent, rent.checked_mul(2).unwrap() + 1, true, true),
        (5, rent, rent.checked_mul(2).unwrap() + 1, true, true),
    ] {
        let mut carriage = PalwStateCarriageV2::from_state(&initial);
        let (bond, row) = carriage.bonds.iter_mut().next().unwrap();
        let bond = *bond;
        let value = row.collateral.checked_add(row.slashed).unwrap();
        assert!(value > fee && row.collateral > burn);
        row.pubkey = pubkey.clone();
        row.operator_id = kaspa_consensus_core::palw_state_v2::palw_operator_id_v2(&pubkey);
        row.payout_payload = payout;
        row.collateral = value - burn;
        row.slashed = burn;
        row.status = PalwBondStatusV2::Retiring { since_daa: 0, settled_at_since: 0 };
        let state = carriage.into_state(&bundle.state, None).unwrap();
        let locked = vp.palw_v2_locked_bond_outpoints(&state, now);
        let burns = vp.palw_v2_bond_burn_obligations(&state, now);
        assert!(!locked.contains(&bond.0));
        assert_eq!(burns.get(&bond.0).copied().unwrap_or(0), burn);
        let group = kaspa_hashes::Hash64::from_u64_word(0xB0F0 + case);
        let object = PalwConsensusObjectV2::ObjectChunk { group, index: 0, count: 3, bytes: vec![0xC0, 0xDE] };
        let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).unwrap();
        let bridge = case == 0 || case == 1 || case == 4;
        let outputs = if bridge {
            vec![
                TransactionOutput::new(value - fee - 1, spk.clone()),
                TransactionOutput::new(
                    1,
                    kaspa_txscript::script_class::evm_deposit_lock_script([0x71; 20], now + 100, 0, spk.script()),
                ),
            ]
        } else {
            vec![TransactionOutput::new(value - fee, spk.clone())]
        };
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(bond.0, vec![], 0, 1)],
            outputs,
            0,
            SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            payload,
        );
        let entry = UtxoEntry::new(value, spk.clone(), 0, false);
        let mass = MassCalculator::new(0, 0, 0, config.params.storage_mass_parameter)
            .calc_contextual_masses(&PopulatedTransaction::new(&tx, vec![entry.clone()]))
            .unwrap()
            .storage_mass;
        tx.set_mass(mass);
        let hash = calc_mldsa87_signature_hash(
            &PopulatedTransaction::new(&tx, vec![entry.clone()]),
            0,
            SIG_HASH_ALL,
            &Mldsa87SigHashReusedValuesUnsync::new(),
        );
        let signature = mldsa::sign(&kp.signing_key, hash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT, [0x73; 32]).unwrap();
        let mut item = signature.as_ref().to_vec();
        item.push(SIG_HASH_ALL.to_u8());
        tx.inputs[0].signature_script = ScriptBuilder::new().add_data(&item).unwrap().add_data(&pubkey).unwrap().drain();
        let view: UtxoCollection = [(bond.0, entry)].into_iter().collect();
        let validated = vp
            .validate_transaction_in_utxo_context(
                &tx,
                &view,
                now,
                crate::processes::transaction_validator::tx_validation_in_utxo_context::TxValidationFlags::Full,
                Some(BondSpendFilter::palw_only_for_tests(now, &locked, &burns)),
            )
            .unwrap();
        assert_eq!(validated.calculated_fee, fee);
        let (mut raw, mut available) = (HashMap::new(), HashMap::new());
        palw_record_accepted_carrier_fees_v1(&validated, burns.get(&bond.0).copied().unwrap_or(0), &mut raw, &mut available);
        assert_eq!(raw[&tx.id()], fee, "raw actual fee keeps its established meaning");
        assert_eq!(available[&tx.id()], fee - burn);
        let extracted = palw_lifecycle_objects_from_accepted_txs_v2(std::slice::from_ref(&tx));
        assert_eq!(extracted.objects.len(), 1);
        let carried = extracted.objects.into_iter().next().unwrap();
        let point = PalwBlockContextV2 { block: ctx.consensus.get_sink(), daa_score: now, blue_score: 5, subsidy: 0 };
        let selected_fee = palw_carrier_rent_available_v1(&available, carried.carrier, priced);
        let (accepted, folded) = vp.palw_v2_accepted_priced_objects_for_tests(
            &state,
            &bundle.state,
            &point,
            vec![(carried.object, selected_fee)],
            point.block,
        );
        assert_eq!(!accepted.is_empty(), accepted_expected, "case {case}");
        assert_eq!(folded.pending_chunk_group(&group).is_some(), accepted_expected, "case {case}");
        let collectible = vp.palw_collectible_transaction_fee_v1(&tx, fee, burn, now);
        assert_eq!(collectible, fee.saturating_sub(burn.saturating_add(rent)));
        assert_eq!(palw_finality_collectible_fee_v1(&tx, collectible, true), if bridge { collectible } else { 0 });
        assert_eq!(palw_finality_collectible_fee_v1(&tx, collectible, false), 0);
        block_collectible += collectible;
        block_finality += palw_finality_collectible_fee_v1(&tx, collectible, true);
    }
    assert_eq!(
        (block_finality, block_collectible),
        (1, 2),
        "bridge and nonbridge native carriers contribute only their actual collectible fees"
    );
}
