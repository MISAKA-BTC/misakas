//! Actual capital return after rooted native liability closure.
use super::*;

pub(crate) async fn refund_family_escrow_v2(net: &mut Net, capture: &FamilyCaptureV2, admitted: &AdmittedFamilyOriginalV2) {
    refund_family_escrow_variant_v3(net, capture, admitted, false).await;
}

async fn refund_family_escrow_variant_v3(net: &mut Net, capture: &FamilyCaptureV2, admitted: &AdmittedFamilyOriginalV2, use_relay_template: bool) -> Vec<Block> {
    use kaspa_consensus_core::palw_funded_escrow_v1 as escrow;
    let before = net.chain.tip_state().1;
    let row = *before.funded_escrows_v1().values().find(|r| r.claim == admitted.claim).unwrap();
    assert!(!row.liability_open && net.daa() >= row.hard_deadline_daa,
        "return requires actual native liability closure after the original hard horizon");
    assert!(row.settled_by.is_none(), "the actual escrow has not already settled");
    let remaining = row.unreleased_v1().unwrap();
    assert!(remaining > 0);
    let [work, relay, burn] = kaspa_consensus_core::palw_g14_values_v1::palw_g14_escrow_split_v1(row.fee_sompi);
    let (work_due, relay_due, burn_due) = if row.fee_sompi == 0 {
        // Generic historical rows have no recorded native fee. Their prior
        // effects, if any, do not create new dues under today's native split.
        (0, 0, 0)
    } else {
        let work_due = if row.paid_sompi == 0 { work } else { 0 };
        let relay_due = work.checked_add(relay).unwrap().checked_sub(row.paid_sompi).unwrap().checked_sub(work_due).unwrap();
        (work_due, relay_due, burn.checked_sub(row.burned_sompi).unwrap())
    };
    let fee_due = work_due.checked_add(relay_due).unwrap().checked_add(burn_due).unwrap();
    let capital = remaining.checked_sub(fee_due).unwrap();
    assert!(capital > 0);
    let pay = |amount, payload: Hash64| TransactionOutput::new(amount,
        kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&payload.as_bytes()));
    let locked = net.chain.ctx.consensus.get_virtual_utxo_entry(row.funding_outpoint).unwrap();
    assert_eq!(locked.amount, remaining);
    let (fee_outpoint, _) = net.funding[capture.card].clone();
    let fee_entry = net.chain.ctx.consensus.get_virtual_utxo_entry(fee_outpoint).unwrap();
    let fee_change = fee_entry.amount.checked_sub(FEE).unwrap();
    let mut outputs = Vec::new();
    if work_due > 0 { outputs.push(pay(work_due, row.seat_payee)); }
    if relay_due > 0 { outputs.push(pay(relay_due, row.relay_payee)); }
    let refund_index = outputs.len();
    outputs.push(pay(capital, row.refund_payload));
    let change_index = outputs.len();
    outputs.push(TransactionOutput::new(fee_change, fee_entry.script_public_key.clone()));
    let payload = if use_relay_template {
        escrow::palw_escrow_relay_template_payload_v1(row.contract_id, row.funding_outpoint,
            if fee_due == 0 { escrow::PalwEscrowSpendPathV1::PayerReturnAfterHardDeadline }
            else { escrow::PalwEscrowSpendPathV1::Settlement })
    } else { vec![] };
    let mut refund = Transaction::new(crate::constants::TX_VERSION,
        vec![TransactionInput::new(row.funding_outpoint, vec![], 0, 1), TransactionInput::new(fee_outpoint, vec![], 0, 1)],
        outputs, 0, kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE, 0, payload);
    let signers = vec![(locked.clone(), capture.card), (fee_entry.clone(), capture.card)];
    sign_spend_many(&mut refund, &signers, net.config.params.storage_mass_parameter);
    refund.finalize();
    let schedule = escrow::palw_escrow_fee_schedule_v1(&row);
    let plan = escrow::palw_escrow_qualified_spend_v1(&row, &refund, &[remaining, fee_entry.amount], net.daa(), schedule.as_ref()).unwrap();
    assert_eq!(plan.path, if fee_due == 0 { escrow::PalwEscrowSpendPathV1::PayerReturnAfterHardDeadline }
        else { escrow::PalwEscrowSpendPathV1::Settlement });
    assert_eq!((plan.paid_now, plan.burned_now, plan.returned_now, plan.continued_now), (work_due + relay_due, burn_due, capital, 0));
    assert_eq!((plan.new.paid_sompi, plan.new.burned_sompi), (row.paid_sompi + work_due + relay_due, row.burned_sompi + burn_due),
        "return preserves actual prior effects and discharges every remaining recorded fee in the same spend");
    assert_eq!(plan.new.paid_sompi + plan.new.burned_sompi + plan.new.returned_sompi, row.amount_sompi);
    assert_eq!(remaining + fee_entry.amount - refund.outputs.iter().map(|o| o.value).sum::<u64>(), FEE + burn_due);
    assert_eq!(net.chain.vp().palw_collectible_transaction_fee_v1(&refund, FEE + burn_due, burn_due, net.daa()), FEE);
    let mut stolen = refund.clone();
    stolen.outputs[refund_index].value -= 1;
    stolen.outputs[change_index].value += 1;
    sign_spend_many(&mut stolen, &signers, net.config.params.storage_mass_parameter);
    stolen.finalize();
    let view: kaspa_consensus_core::utxo::utxo_collection::UtxoCollection =
        [(row.funding_outpoint, locked), (fee_outpoint, fee_entry)].into_iter().collect();
    if fee_due > 0 {
        let root = before.state_root();
        let mut bypass = Transaction::new(crate::constants::TX_VERSION, refund.inputs.clone(),
            vec![pay(remaining, row.refund_payload), TransactionOutput::new(fee_change, signers[1].0.script_public_key.clone())],
            0, kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE, 0, vec![]);
        sign_spend_many(&mut bypass, &signers, net.config.params.storage_mass_parameter);
        bypass.finalize();
        net.chain.vp().validate_transaction_in_utxo_context(&bypass, &view, net.daa(),
            crate::processes::transaction_validator::tx_validation_in_utxo_context::TxValidationFlags::Full, None)
            .expect("unpaid fee bypass still has genuine ordinary payer signatures");
        assert!(net.mempool(&bypass).is_err(), "hard/closed liability cannot return recorded native fees unpaid");
        assert_eq!(net.chain.tip_state().1.state_root(), root);
        assert_eq!(net.chain.tip_state().1.funded_escrows_v1()[&row.contract_id], row);
        assert!(net.chain.ctx.consensus.get_virtual_utxo_entry(row.funding_outpoint).is_some());
    }
    net.chain.vp().validate_transaction_in_utxo_context(&stolen, &view, net.daa(),
        crate::processes::transaction_validator::tx_validation_in_utxo_context::TxValidationFlags::Full, None)
        .expect("wrong return amount still carries genuine payer signatures");
    assert!(net.mempool(&stolen).is_err(), "resolved escrow still requires exact capital return");
    net.mempool(&refund).expect("actual closed liability releases the continued escrow UTXO");
    let carrying = net.chain.heartbeat(net.ttpb(), vec![refund.clone()]).await;
    assert!(carrying.transactions.iter().any(|tx| tx.id() == refund.id()));
    let confirmation = net.chain.heartbeat(net.ttpb(), vec![]).await;
    let state = net.chain.tip_state().1;
    assert_eq!(state.funded_escrows_v1()[&row.contract_id], plan.new);
    assert!(net.chain.ctx.consensus.get_virtual_utxo_entry(row.funding_outpoint).is_none());
    for (index, output) in refund.outputs.iter().enumerate() {
        assert_eq!(net.chain.ctx.consensus.get_virtual_utxo_entry(TransactionOutpoint::new(refund.id(), index as u32)).unwrap().amount, output.value);
    }
    let change = TransactionOutpoint::new(refund.id(), change_index as u32);
    let change_entry = net.chain.ctx.consensus.get_virtual_utxo_entry(change).unwrap();
    assert_eq!(change_entry.amount, fee_change);
    net.funding[capture.card] = (change, change_entry);
    assert!(net.mempool(&refund).is_err(), "actual return cannot spend its input twice");
    let restored = PalwStateCarriageV2::from_state(&state).into_state(&net.bundle.state, Some(state.state_root())).unwrap();
    assert_eq!(restored.funded_escrows_v1()[&row.contract_id], plan.new);
    vec![carrying, confirmation]
}

/// Genuine conflicting spends over an actual database checkpoint. Both return
/// transactions and the reorg cross the ordinary block/native state processor.
/// The checkpoint avoids replaying the complete native39 history a third time.
pub(crate) async fn refund_family_escrow_reorg_v3(
    net: &mut Net, capture: &FamilyCaptureV2, admitted: &AdmittedFamilyOriginalV2,
    db: std::sync::Arc<kaspa_database::prelude::DB>,
) {
    let fork = net.chain.sink();
    let original = *net.chain.tip_state().1.funded_escrows_v1().values().find(|r| r.claim == admitted.claim).unwrap();
    let time = net.chain.ctx.simulated_time;
    let nonce = net.chain.nonce_for_reopen();
    let validation_time = net.chain.ctx.consensus.has_validation_time_for_test();
    let scratch = kaspa_database::utils::get_kaspa_tempdir();
    let path = scratch.path().join("native-refund-fork");
    rocksdb::checkpoint::Checkpoint::new(&**db).unwrap().create_checkpoint(&path).unwrap();
    let fork_db = kaspa_database::prelude::ConnBuilder::default().with_files_limit(10)
        .with_db_path(path).with_create_if_missing(false).build().unwrap();
    let mut config = net.config.clone();
    config.process_genesis = false;
    let (sender, receiver) = async_channel::unbounded();
    let consensus = TestConsensus::with_db(fork_db.clone(), &config, sender);
    if validation_time { consensus.set_validation_time_for_test(time); }
    let mut chain = t12_reopened_chain(consensus, &config, &net.bundle, time, nonce);
    chain.set_nonce_for_fork(nonce.checked_add(1_000_000).unwrap());
    let mut alternative = net.on_chain(chain);
    alternative._keep.push(Box::new(receiver));
    assert_eq!(alternative.chain.sink(), fork);
    assert_eq!(alternative.chain.ctx.consensus.get_evm_canonical_heads().unwrap(),
        net.chain.ctx.consensus.get_evm_canonical_heads().unwrap(),
        "the real DB checkpoint restores the same EVM heads");
    assert_eq!(alternative.chain.ctx.consensus.get_native_settlement_snapshot().unwrap(),
        net.chain.ctx.consensus.get_native_settlement_snapshot().unwrap(),
        "native settlement publication is in the same checkpoint database");
    assert_eq!(alternative.chain.tip_state().1.state_root(), net.chain.tip_state().1.state_root());
    assert_eq!(alternative.chain.tip_state().1.funded_escrows_v1()[&original.contract_id], original);
    assert_eq!(alternative.chain.ctx.consensus.get_virtual_utxo_entry(original.funding_outpoint).unwrap().amount,
        original.unreleased_v1().unwrap());

    let first_blocks = refund_family_escrow_variant_v3(net, capture, admitted, false).await;
    let first = net.chain.tip_state().1.funded_escrows_v1()[&original.contract_id];
    let first_txid = first.settled_by.unwrap();
    let first_tx = first_blocks.iter().flat_map(|b| b.transactions.iter()).find(|tx| tx.id() == first_txid).unwrap();
    let first_outputs = first_tx.outputs.len();
    let a_work = net.chain.ctx.consensus.get_header(net.chain.sink()).unwrap().blue_work;

    // Same genuine input and exact recorded payees/burn/capital. The existing
    // TER1 authorization payload makes the other valid transaction distinct.
    let mut branch = refund_family_escrow_variant_v3(&mut alternative, capture, admitted, true).await;
    let replacement = alternative.chain.tip_state().1.funded_escrows_v1()[&original.contract_id];
    let replacement_txid = replacement.settled_by.unwrap();
    assert_ne!(first_txid, replacement_txid);
    assert_eq!((first.paid_sompi, first.burned_sompi, first.returned_sompi),
        (replacement.paid_sompi, replacement.burned_sompi, replacement.returned_sompi));
    while alternative.chain.ctx.consensus.get_header(alternative.chain.sink()).unwrap().blue_work <= a_work {
        branch.push(alternative.chain.heartbeat(alternative.ttpb(), vec![]).await);
    }
    let branch_time = alternative.chain.ctx.simulated_time;
    if validation_time { net.chain.ctx.consensus.set_validation_time_for_test(branch_time); }
    for block in &branch { arrive(&net.chain, block.clone(), "actual conflicting escrow return branch").await; }
    net.chain.ctx.simulated_time = net.chain.ctx.simulated_time.max(branch_time);
    assert_eq!(net.chain.sink(), alternative.chain.sink());
    assert_eq!(net.chain.ctx.consensus.get_evm_canonical_heads().unwrap(),
        alternative.chain.ctx.consensus.get_evm_canonical_heads().unwrap(),
        "the conflicting escrow-spend reorg agrees on EVM heads");
    assert_eq!(net.chain.ctx.consensus.get_native_settlement_snapshot().unwrap(),
        alternative.chain.ctx.consensus.get_native_settlement_snapshot().unwrap(),
        "the actual reorg agrees on native settlement publication");
    assert_eq!(net.chain.tip_state().1.state_root(), alternative.chain.tip_state().1.state_root());
    assert_eq!(net.chain.tip_state().1.funded_escrows_v1()[&original.contract_id], replacement);
    for index in 0..first_outputs {
        assert!(net.chain.ctx.consensus.get_virtual_utxo_entry(TransactionOutpoint::new(first_txid, index as u32)).is_none(),
            "reorg removes abandoned payment/refund outputs and normal change with their accounting");
    }
    let accepted_tx = branch.iter().flat_map(|b| b.transactions.iter()).find(|tx| tx.id() == replacement_txid).unwrap();
    for (index, output) in accepted_tx.outputs.iter().enumerate() {
        assert_eq!(net.chain.ctx.consensus.get_virtual_utxo_entry(TransactionOutpoint::new(replacement_txid, index as u32)).unwrap().amount, output.value);
    }
    assert!(net.chain.ctx.consensus.get_virtual_utxo_entry(original.funding_outpoint).is_none());
    assert!(net.mempool(first_tx).is_err());
    assert!(net.mempool(accepted_tx).is_err());
    net.funding[capture.card] = alternative.funding[capture.card].clone();
    let state = net.chain.tip_state().1;
    let restored = PalwStateCarriageV2::from_state(&state).into_state(&net.bundle.state, Some(state.state_root())).unwrap();
    assert_eq!(restored.funded_escrows_v1()[&original.contract_id], replacement);
    drop(alternative);
    drop(fork_db);
    drop(scratch);
}
