//! Real signed deposit/order/F002 pipeline and cold reopen. No filled market,
//! mature-work mapping, model examination or Safe/Finalized result is supplied.
use super::*;
#[tokio::test]
async fn evm_market_production_actual_settlements_withdraw_once_and_cold_reopen() {
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    let p = parts(None); // No test settlement/retirement policy is installed.
    p.0.params.validate_palw_v2().expect("full unchanged private matrix contract");
    let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, _rx) = async_channel::unbounded();
    let first = TestConsensus::with_db(db.clone(), &p.0, sender);
    let mut rig = Rig::on(first, &p, 0x12_ed00_0000);
    let run = scripted_run(&mut rig, "e3-withdraw").await;
    let lock_block = run.names.iter().find(|(name, _)| *name == "e1-lock").unwrap().1;
    let body = rig.api().get_block(lock_block).unwrap();
    let locks: Vec<_> = body
        .transactions
        .iter()
        .flat_map(|tx| {
            tx.outputs.iter().enumerate().filter_map(|(index, out)| {
                (kaspa_txscript::script_class::ScriptClass::from_script(&out.script_public_key)
                    == kaspa_txscript::script_class::ScriptClass::EvmDepositLock)
                    .then_some((TransactionOutpoint::new(tx.id(), index as u32), out.value))
            })
        })
        .collect();
    assert_eq!(locks.len(), 1);
    assert_eq!(locks[0].1, DEPOSIT);
    assert!(rig.api().get_virtual_utxo_entry(locks[0].0).is_none(), "actual signed lock was consumed by the automatic claim");
    let parent = rig.chain.sink();
    let expected = {
        let vp = rig.vp();
        let state = vp.evm_template_parent_owned_v2(parent, rig.api().get_header(parent).unwrap().daa_score).unwrap().unwrap();
        state.evm_settlements()
    };
    assert_eq!(expected.len(), 2, "real signed sells must first reach the fold, no prefilled queue surrogate");
    assert!(expected.iter().all(|s| matches!(s.outcome, PalwEvmSettlementOutcomeV1::Refused { reason: refusal::MARKET_MISSING })));
    let block = rig.attempt(4, vec![], no_evm());
    let carried: Vec<_> = block
        .evm_payload
        .system_ops
        .iter()
        .filter_map(|op| if let EvmSystemOp::MarketSettle(s) = op { Some(*s) } else { None })
        .collect();
    assert_eq!(carried, expected, "actual own template must carry the complete exact parent list");
    let accepted = rig.take(block, "owned market settlements and F002").await;
    let accepting = accepted.header.hash;
    let (withdraw_raw, spk) = withdrawal();
    let tx = kaspa_evm::tx::tx_hash(&withdraw_raw);
    let receipt = rig.api().get_evm_tx_receipt(tx).unwrap().expect("actual signed F002 executed");
    assert!(receipt.receipt.succeeded);
    assert_eq!(receipt.accepting_block, accepting);
    let paid = |r: &Rig| {
        r.api()
            .get_virtual_utxos(None, 10_000_000, false)
            .into_iter()
            .filter(|(_, entry)| entry.script_public_key == spk)
            .collect::<Vec<_>>()
    };
    let outputs = paid(&rig);
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].1.amount, WITHDRAW_SOMPI);
    let final_balance = balance_of(&rig, ACCOUNT);
    assert!(
        final_balance < (DEPOSIT - WITHDRAW_SOMPI) as u128 * EVM_NATIVE_SCALE as u128,
        "actual deposit credit minus withdrawal and signed-call gas; no artificial account credit"
    );
    let next = rig.attempt(5, vec![], no_evm());
    assert!(
        next.evm_payload.system_ops.iter().all(|op| !matches!(op, EvmSystemOp::MarketSettle(_))),
        "settled parent list leaves the rooted state once"
    );
    rig.take(next, "no repeated settlement").await;
    assert_eq!(paid(&rig), outputs);
    assert_eq!(balance_of(&rig, ACCOUNT), final_balance);
    let (sink, wallets, nonce, time, reopen_nonce) =
        (rig.chain.sink(), rig.wallets.clone(), rig.nonce, rig.chain.ctx.simulated_time, rig.chain.nonce_for_reopen());
    assert!(run.names.iter().any(|(name, _)| *name == "e2-sells"));
    drop(rig);
    let mut config = p.0.clone();
    config.process_genesis = false;
    let (sender, _rx) = async_channel::unbounded();
    let reopened = TestConsensus::with_db(db.clone(), &config, sender);
    let chain = t12_reopened_chain(reopened, &config, &p.1, time, reopen_nonce);
    let resumed = (config, p.1.clone(), p.2.clone(), p.3.clone());
    let mut rig = Rig::resumed(chain, &resumed, wallets, nonce);
    assert_eq!(rig.chain.sink(), sink);
    assert_eq!(paid(&rig), outputs);
    assert_eq!(balance_of(&rig, ACCOUNT), final_balance);
    assert_eq!(rig.api().get_evm_tx_receipt(tx).unwrap().unwrap().accepting_block, accepting);
    let next = rig.attempt(6, vec![], no_evm());
    assert!(next.evm_payload.system_ops.iter().all(|op| !matches!(op, EvmSystemOp::MarketSettle(_))));
    rig.take(next, "cold native no replay").await;
    assert_eq!(paid(&rig), outputs);
    assert_eq!(balance_of(&rig, ACCOUNT), final_balance);
}
