//! RFC-0039 actual-node fee-only cash boundary, using the existing explicitly
//! private material_gc_v4 mechanics policy unchanged. This is not a public
//! activation/measured-policy record or a claim certification/material-availability
//! test. A real executed job's prospective claim stays unpublished: fee-only H
//! is independent of publication/Final and does not shorten original service.
use super::*;
use kaspa_consensus_core::palw_monetary_material_v1::{
    PalwMaterialFeeStatusV2, PalwMonetaryMaterialActionV1, PalwMonetaryMaterialEnrollmentV1, PalwMonetaryMaterialOfferV1,
    PalwMonetaryMaterialTermsV1, fee_v2::monetary_material_fee_object_v2,
};

fn config() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (mut c, _, premine, floats) = super::material_gc_v4::config();
    let material_policy = c.params.palw_monetary_material_v1.as_ref().unwrap().policy.clone();
    let window = c.params.palw_bond_budget_v1.as_ref().unwrap().policy.window_daa;
    c.params.palw_settlement_once_v1 = Some(ForkActivation::new(0));
    c.params.sync_palw_settlement_once_v1();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &c.params.palw_consensus_mode else {
        panic!("existing material mechanics world must carry V2")
    };
    assert_eq!(c.params.palw_monetary_material_v1.as_ref().unwrap().policy, material_policy);
    assert_eq!(material_policy.window_daa, window, "service/material windows remain the existing policy's");
    assert!(bundle.state.settlement_once_active_at(0));
    assert!(c.params.validate_palw_monetary_material_v1().is_err(), "private mechanics policy is not public activation evidence");
    let bundle = bundle.clone();
    (c, bundle, premine, floats)
}

fn signed_enrollment(net: &Net, enrollment: PalwMonetaryMaterialEnrollmentV1) -> Obj {
    let ruleset = Hash64::from_bytes(net.ledger().policy.ruleset_digest);
    monetary_material_fee_object_v2(net.domain, ruleset, enrollment, None, |bond, message, context| {
        let card = net.chain.bonds.iter().position(|b| b == bond)?;
        let key = TestConsensus::palw_v2_registry_keypair(card as u64);
        libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, message, context, [0x39; 32]).ok().map(|s| s.as_ref().to_vec())
    })
    .expect("all original service, finite fee and sponsor authorities are locally available")
}

async fn send_saved_carrier(net: &mut Net, tx: Transaction) {
    net.mempool(&tx).expect("the real signed UTXO carrier passes transaction policy");
    let carrying = net.chain.heartbeat(net.ttpb(), vec![tx.clone()]).await;
    assert!(carrying.transactions.iter().any(|t| t.id() == tx.id()));
    net.chain.heartbeat(net.ttpb(), vec![]).await;
    assert!(
        chain_blocks(&net.chain, net.chain.sink()).iter().any(|b| {
            net.chain
                .ctx
                .consensus
                .get_block_acceptance_data(b.header.hash)
                .unwrap()
                .iter()
                .any(|m| m.accepted_transactions.iter().any(|accepted| accepted.transaction_id == tx.id()))
        }),
        "actual selected-chain acceptance, not merely template inclusion"
    );
}

#[tokio::test]
async fn rfc39_provider_fee_actual_carrier_coinbase_ack_and_same_db_restart() {
    use kaspa_consensus_core::palw_state_v2::PalwBondStatusV2;
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    kaspa_core::log::try_init_logger("warn");
    let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, receiver) = async_channel::unbounded();
    let mut net = Net::over_cfg(config(), |c| TestConsensus::with_db(db.clone(), c, sender));
    net._keep.push(Box::new(receiver));
    let mut w = World::on(net).await;
    let job = w.job().await;
    // Derive the exact future identity from the real registered program/job,
    // bound native producer, ordinary deterministic execution and commitments.
    // Do not submit it or turn matching replay into selected-verifier approval.
    let ledger = w.net.ledger();
    let generated = greedy(&w.fx, &ledger, &w.class, &job.prompt, job.max_new_tokens as usize);
    let produced = produce(&w.fx, &ledger, &w.class, &job, w.net.kid(0), generated, |_| {});
    let root = Hash64::from_bytes(produced.claim.id());
    assert_eq!(produced.claim.job_id, job.id());
    assert!(ledger.jobs.contains_key(&job.id()) && ledger.classes.contains_key(&w.class));
    assert!(!ledger.claims.contains_key(&root.as_bytes()));
    let from = w.net.config.params.palw_monetary_material_v1.as_ref().unwrap().activation.daa_score();
    w.net.beat_to(from).await;
    let policy = w.net.config.params.palw_monetary_material_v1.as_ref().unwrap().policy.clone();
    // Existing signed-contract interval, not a reduced public response/service
    // policy: the fee horizon is exactly the enrollment ceiling both parties sign.
    let horizon = w.net.daa().checked_add(policy.response_window_daa).unwrap();
    assert!(policy.response_window_daa <= policy.max_enrollment_daa);
    let sponsor_card = 1;
    let providers = [2usize, 3];
    let sponsor = w.net.bond(sponsor_card);
    let fee = policy.minimum_fee_sompi;
    let total_fee = fee.checked_mul(providers.len() as u64).unwrap();
    let mut offers: Vec<_> = providers
        .iter()
        .map(|card| PalwMonetaryMaterialOfferV1 {
            provider: w.net.bond(*card),
            collateral_sompi: policy.minimum_collateral_sompi,
            prepaid_fee_sompi: fee,
            signature: vec![],
        })
        .collect();
    offers.sort_by_key(|o| o.provider);
    let terms = PalwMonetaryMaterialTermsV1 {
        version: 1,
        root,
        dependencies: vec![],
        exec_plan_root: None,
        sponsor,
        nonce: Hash64::from_bytes([0x39; 64]),
        policy: policy.id(),
        max_material_bytes: policy.max_bytes_per_contract,
        material_manifest: None,
        native_session: None,
        window_daa: policy.window_daa,
        enrollment_until_daa: horizon,
        payment_relative_until_finality: true,
    };
    let enrollment = PalwMonetaryMaterialEnrollmentV1 { terms: terms.clone(), offers };
    let context = enrollment.id();
    let object = signed_enrollment(&w.net, enrollment);
    let initial = w.net.chain.tip_state().1;
    let sponsor_before = initial.bond(&sponsor).unwrap().collateral;
    let mut operators = std::collections::BTreeSet::new();
    operators.insert(initial.bond(&sponsor).unwrap().operator_id);
    let provider_principal: Vec<_> = providers
        .iter()
        .map(|card| {
            let b = initial.bond(&w.net.bond(*card)).unwrap();
            assert!(matches!(b.status, PalwBondStatusV2::Active));
            assert!(operators.insert(b.operator_id), "real independent non-sponsor providers");
            (*card, b.collateral)
        })
        .collect();
    // A genuine native spend can carry an invalid lifecycle object. The
    // existing transaction mempool policy is separate from the acceptance
    // filter that authenticates the finite-fee signatures. Observe that real
    // filter before claiming contract admission or any collateral debit.
    let mut bad = object.clone();
    let Obj::MonetaryMaterialActionV1 { action, .. } = &mut bad else { unreachable!() };
    let PalwMonetaryMaterialActionV1::EnrollFeeV2(fee_terms) = action.as_mut() else { unreachable!() };
    fee_terms.provider_fee_signatures[0][0] ^= 1;
    let bad_input = w.net.funding[sponsor_card].0;
    let bad_tx = w.net.carrier(sponsor_card, &bad);
    send_saved_carrier(&mut w.net, bad_tx).await;
    let refused = w.net.chain.tip_state().1;
    assert!(refused.monetary_material_row_v1(root).is_none(), "invalid fee signature cannot install a service contract");
    for card in providers {
        assert!(refused.material_fee_row_checked_v2(context, w.net.bond(card)).unwrap().is_none());
    }
    assert_eq!(w.net.collateral(sponsor_card), sponsor_before, "invalid lifecycle object cannot debit sponsor collateral");
    assert!(!Net::utxos_of(&w.net.chain).contains_key(&bad_input), "the carrier is an actual separately qualified native spend");
    let funding = w.net.funding[sponsor_card].clone();
    assert!(Net::utxos_of(&w.net.chain).contains_key(&funding.0));
    let tx = w.net.carrier(sponsor_card, &object);
    let tx_id = tx.id();
    assert_eq!(funding.1.amount - tx.outputs.iter().map(|o| o.value).sum::<u64>(), FEE, "carrier fee and service escrow are distinct");
    send_saved_carrier(&mut w.net, tx).await;
    let accepted = w.net.chain.tip_state().1;
    assert_eq!(w.net.collateral(sponsor_card), sponsor_before - total_fee);
    assert!(!Net::utxos_of(&w.net.chain).contains_key(&funding.0));
    assert!(Net::utxos_of(&w.net.chain).contains_key(&TransactionOutpoint::new(tx_id, 0)));
    assert_eq!(accepted.monetary_material_row_v1(root).unwrap().terms, terms);
    for card in providers {
        let held = accepted.material_fee_row_checked_v2(context, w.net.bond(card)).unwrap().unwrap();
        assert_eq!((held.amount, held.horizon, held.status), (fee, horizon, PalwMaterialFeeStatusV2::Held));
        assert_eq!(w.net.owed(card), 0);
    }
    assert!(w.net.daa() <= horizon, "the accepted contract has a live finite hold");
    while w.net.daa() <= horizon {
        for card in providers {
            assert_eq!(
                w.net.chain.tip_state().1.material_fee_row_v2(context, w.net.bond(card)).unwrap().status,
                PalwMaterialFeeStatusV2::Held
            );
            assert_eq!(w.net.owed(card), 0, "no provider fee before or at its signed cutoff");
        }
        let t = w.net.ttpb();
        w.net.chain.heartbeat(t, vec![]).await;
    }
    let queued = w.net.chain.tip_state().1;
    let contract = queued.monetary_material_row_v1(root).unwrap();
    assert!(!contract.root_seen, "fee-only maturity grants no claim publication, availability or credit");
    assert!(!w.net.ledger().claims.contains_key(&root.as_bytes()));
    for card in providers {
        assert_eq!(queued.material_fee_row_v2(context, w.net.bond(card)).unwrap().status, PalwMaterialFeeStatusV2::ProviderQueued);
        assert_eq!(w.net.owed(card), fee);
        assert!(
            contract
                .queued_entitlements
                .values()
                .any(|id| *id == contract.providers.iter().find(|p| p.provider == w.net.bond(card)).unwrap().fee_income)
        );
    }
    let queued_root = queued.state_root();
    w = w.restart(db.clone());
    assert_eq!(
        w.net.chain.tip_state().1.state_root(),
        queued_root,
        "same DB preserves cash and exact fee entitlements before payment"
    );
    let mut physical = Vec::new();
    for _ in 0..8 {
        let t = w.net.ttpb();
        let block = w.net.chain.heartbeat(t, vec![]).await;
        let coinbase = &block.transactions[0];
        for (index, output) in coinbase.outputs.iter().enumerate() {
            if providers.iter().any(|card| output.value == fee && output.script_public_key == card_payout_spk(*card)) {
                physical.push((TransactionOutpoint::new(coinbase.id(), index as u32), output.clone()));
            }
        }
        let state = w.net.chain.tip_state().1;
        if state.monetary_material_row_v1(root).unwrap().providers.iter().all(|p| p.fee_paid) {
            break;
        }
    }
    let paid = w.net.chain.tip_state().1;
    let row = paid.monetary_material_row_v1(root).unwrap();
    assert!(row.providers.iter().all(|p| p.fee_paid), "the actual coinbase drain, not a test writer, acknowledges every provider");
    assert_eq!(row.service_fee_paid_sompi, u128::from(total_fee));
    assert_eq!(physical.len(), providers.len(), "exactly one physical provider output per signed fee");
    let utxos = Net::utxos_of(&w.net.chain);
    for (outpoint, output) in &physical {
        let entry = utxos.get(outpoint).expect("actual paid coinbase output exists in the real virtual UTXO set");
        assert!(entry.is_coinbase);
        assert_eq!((entry.amount, &entry.script_public_key), (output.value, &output.script_public_key));
    }
    assert_eq!(w.net.collateral(sponsor_card), sponsor_before - total_fee);
    for (card, before) in &provider_principal {
        assert_eq!(w.net.collateral(*card), *before, "paying the prepaid fee does not consume the separate service collateral");
        assert_eq!(w.net.owed(*card), 0);
    }
    let paid_root = paid.state_root();
    w = w.restart(db.clone());
    assert_eq!(w.net.chain.tip_state().1.state_root(), paid_root);
    assert_eq!(Net::utxos_of(&w.net.chain), utxos, "same DB restart preserves the actual paid UTXOs");
    for _ in 0..3 {
        let t = w.net.ttpb();
        w.net.chain.heartbeat(t, vec![]).await;
    }
    let final_row = w.net.chain.tip_state().1.monetary_material_row_v1(root).unwrap();
    assert_eq!(final_row.service_fee_paid_sompi, u128::from(total_fee), "continued blocks do not recreate earned fees");
    for card in providers {
        assert_eq!(w.net.owed(card), 0);
    }
}
