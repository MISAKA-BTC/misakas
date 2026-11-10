//! TKR1 registration burn inside a real signed Round body's fee limit.
use super::*;
use crate::pipeline::virtual_processor::tests::g14_registration_e2e::tokenizer_registration::{
    configure_tokenizer_registration, prepare_round_class,
};
use kaspa_consensus_core::{
    config::params::ForkActivation,
    palw_bond_budget_v1::{
        PALW_BUDGET_BLOCK_UNIT_V1, PalwBondBudgetFenceV1, PalwBondBudgetPolicyV1, PalwBudgetOriginV1, PalwRoundRightsPolicyV1,
        palw_round_income_id_v1,
    },
    palw_execution_lane_v1::palw_execution_permits_v1,
    palw_tokenizer_registration_carriage_v1::encode_tokenizer_registration_payload_v1,
    palw_tokenizer_registration_v1::{
        TOKENIZER_REGISTRATION_OPEN_CONTEXT_V1, TokenizerRegistrationOpenV1, TokenizerRegistrationWireV1,
    },
};

const TOTAL_CAP: u64 = kaspa_consensus_core::config::params::PALW_T12_GENESIS_BLOCK_SUBSIDY_SOMPI / 1_000 * 620;
const OPENER_CARD: usize = 4;

fn configure(params: &mut Params) {
    configure_tokenizer_registration(params);
    // The existing Round mechanics test uses this isolated V2 cap to put
    // a known boundary on collectible fees. Public arming requires V3.

    let mut policy = PalwBondBudgetPolicyV1::unapproved_probe_v1();
    policy.window_daa = 280;
    policy.capital_unit_sompi = kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
    policy.claims_per_unit = 1_000_000;
    policy.block_units_per_unit = 1_000_000 * PALW_BUDGET_BLOCK_UNIT_V1;
    policy.reward_per_unit_sompi = TOTAL_CAP;
    policy.final_weight_per_unit = 1_000_000;
    policy.max_open_claims_per_bond = 1_000_000;
    policy.round_rights = PalwRoundRightsPolicyV1::ExecutionCap { rights_per_unit: 1 };
    params.palw_bond_budget_v1 = Some(PalwBondBudgetFenceV1 { activation: ForkActivation::new(264), policy });
    params.sync_palw_bond_budget_v1();
}

fn signed_carrier(
    chain: &T12Chain,
    floats: &[(TransactionOutpoint, UtxoEntry)],
    fee: u64,
    wire: &TokenizerRegistrationWireV1,
) -> Transaction {
    let mut inputs = Vec::new();
    let mut funding = Vec::new();
    let mut total = 0;
    for card in [OPENER_CARD, 5, 6] {
        let (outpoint, entry) = floats[card].clone();
        assert_eq!(chain.ctx.consensus.get_virtual_utxo_entry(outpoint), Some(entry.clone()));
        total += entry.amount;
        inputs.push(TransactionInput::new(outpoint, Vec::new(), 0, 1));
        funding.push((entry, card));
        if total > fee {
            break;
        }
    }
    assert!(total > fee, "the imported card floats cover the real fee");
    let mut tx = Transaction::new(
        crate::constants::TX_VERSION,
        inputs,
        vec![TransactionOutput::new(total - fee, card_payout_spk(OPENER_CARD))],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        encode_tokenizer_registration_payload_v1(wire).unwrap(),
    );
    sign_spend_many(&mut tx, &funding, chain.config.params.storage_mass_parameter);
    tx
}

fn standard_template(at: &AtTheTargetSpan, tx: &Transaction) -> kaspa_consensus_core::block::BlockTemplate {
    at.chain
        .ctx
        .consensus
        .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(vec![tx.clone()])), TemplateBuildMode::Standard)
        .expect("ordinary template carries the signed TKR1 body")
}

#[test]
fn signed_tokenizer_registration_round_fee_body_boundary_is_atomic() {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(run()))
        .unwrap()
        .join()
        .unwrap();
}

async fn run() {
    for excess in [0, 1] {
        let (mut at, (mut body, floats)) =
            a_floor_final_scheduled_with_setup(SeedDraw::MintsATicket, None, true, configure, prepare_round_class).await;
        let ticket = at.schedule.quanta.iter().filter(|q| q.final_id == at.claim_id).min_by_key(|q| q.scheduled_round).unwrap();
        let round = ticket.scheduled_round;
        let width = at.lane.width_of_span_len(at.target, at.span_daa);
        let permit = palw_execution_permits_v1(&at.schedule, round, width).into_iter().find(|p| p.bond == at.executor).unwrap();
        at.chain.attempt(3, 1_000, Vec::new(), &|_| true).await;
        assert_eq!(at.chain.vp().ghostdag_store.get_selected_parent(at.chain.sink()).unwrap(), at.opening);

        let (_, parent) = at.chain.tip_state();
        let before = *parent.bond_budget().unwrap().bond_row(&at.executor).unwrap();
        let room = TOTAL_CAP.checked_sub(before.window.reward_sompi).unwrap();
        assert!(room > 0);
        let policy = at.chain.config.params.palw_tokenizer_registration_v1.as_ref().unwrap().policy;
        let daa = at.chain.ctx.consensus.get_virtual_daa_score();
        body.accept_until_daa = daa + 20;
        body.finish_until_daa = daa + 40;
        body.material_until_daa = body.finish_until_daa + policy.retention_daa;
        let open_id = body.id_v1().unwrap();
        let preview = at.chain.ctx.consensus.palw_tokenizer_registration_open_quote_v1(body).unwrap().unwrap();
        assert_eq!(preview.quote_daa, daa);
        let charge = preview.value.unwrap().quote.charge;
        assert!(charge > 0);
        let signature = *libcrux_ml_dsa::ml_dsa_87::sign(
            &card_key(OPENER_CARD).signing_key,
            open_id.as_byte_slice(),
            TOKENIZER_REGISTRATION_OPEN_CONTEXT_V1,
            [0x54; 32],
        )
        .unwrap()
        .as_ref();
        let wire = TokenizerRegistrationWireV1::Open(TokenizerRegistrationOpenV1 { body, signature });

        let fitting = signed_carrier(&at.chain, &floats, room + charge, &wire);
        let mut block = at
            .chain
            .vp()
            .round_adapt_block_template(standard_template(&at, &fitting), round, card_payout_spk(0))
            .expect("gross fee is over budget but collectible fee fits")
            .block;
        let paying = if excess == 0 {
            fitting
        } else {
            let excessive = signed_carrier(&at.chain, &floats, room + charge + excess, &wire);
            assert!(matches!(
                at.chain.vp().round_adapt_block_template(standard_template(&at, &excessive), round, card_payout_spk(0)),
                Err(crate::errors::RuleError::RoundFeeBudgetRefused(_))
            ));
            *block.transactions.iter_mut().find(|tx| tx.id() == fitting.id()).unwrap() = excessive.clone();
            block.header.hash_merkle_root = kaspa_consensus_core::merkle::calc_hash_merkle_root(block.transactions.iter());
            excessive
        };
        let input_before: Vec<_> =
            paying.inputs.iter().map(|input| at.chain.ctx.consensus.get_virtual_utxo_entry(input.previous_outpoint)).collect();
        let block = insert_signed_round_block(&at, round, permit.index, block, &paying).await;
        let merged = merge_the_round_block(&mut at, &block, &paying, round, room).await;
        assert!(merged.permitted && merged.permit_recorded);
        assert_eq!(merged.uses, vec![(at.target, round, permit.index)]);
        assert_eq!(merged.accepted_in_span, 1);
        let accepted = excess == 0;
        assert_eq!(merged.spend_accepted, accepted);
        assert_eq!(merged.fee_paid_to_payout, accepted);
        let (_, after) = at.chain.tip_state();
        let book = after.bond_budget().unwrap();
        let income_id = palw_round_income_id_v1(block.header.hash, &at.executor);
        let lookup = |chain: &T12Chain| chain.ctx.consensus.palw_tokenizer_registration_status_v1(open_id).unwrap().unwrap().value;
        if accepted {
            let status = lookup(&at.chain).expect("the fitting Round folds its Open");
            assert_eq!(status.accepted.block, merged.merging.header.hash);
            assert_eq!(status.quote.charge, charge);
            assert_eq!(status.charge_id, body.charge_id_v1().unwrap());
            let income = book.claim_row(&income_id).expect("accepted Round records collectible income");
            assert_eq!(income.origin, PalwBudgetOriginV1::RoundFee);
            assert_eq!(income.consumed.reward_sompi, room);
            assert_eq!(book.bond_row(&at.executor).unwrap().window.reward_sompi, TOTAL_CAP);
            for input in &paying.inputs {
                assert!(at.chain.ctx.consensus.get_virtual_utxo_entry(input.previous_outpoint).is_none());
            }
        } else {
            assert!(lookup(&at.chain).is_none());
            assert!(book.claim_row(&income_id).is_none());
            assert_eq!(*book.bond_row(&at.executor).unwrap(), before);
            for (input, original) in paying.inputs.iter().zip(input_before) {
                assert_eq!(at.chain.ctx.consensus.get_virtual_utxo_entry(input.previous_outpoint), original);
            }
            assert!(at.chain.ctx.consensus.get_virtual_utxo_entry(TransactionOutpoint::new(paying.id(), 0)).is_none());
            let ttpb = at.chain.config.params.target_time_per_block();
            // Reuse the same signed Open after the refusal. Fund the full
            // prospective charge again and verify the actual fold charges it.
            let retry_quote = at.chain.ctx.consensus.palw_tokenizer_registration_open_quote_v1(body).unwrap().unwrap();
            let retry_gross = retry_quote.value.unwrap().quote.charge + 2_000_000;
            let retry = signed_carrier(&at.chain, &floats, retry_gross, &wire);
            let source = at.chain.heartbeat(ttpb, vec![retry.clone()]).await;
            assert!(source.transactions.iter().any(|tx| tx.id() == retry.id()));
            let at_source = at.chain.ctx.consensus.palw_tokenizer_registration_open_quote_v1(body).unwrap().unwrap();
            let expected_charge = at_source.value.unwrap().quote.charge;
            assert!(expected_charge > 0);
            let virtual_state = at.chain.vp().virtual_stores.read().state.get().unwrap();
            assert_eq!(virtual_state.mergeset_rewards[&source.header.hash].total_fees, retry_gross - expected_charge);
            let folded = at.chain.heartbeat(ttpb, vec![]).await;
            let status = lookup(&at.chain).expect("refusal did not prepay or reserve the retry");
            assert_eq!(status.accepted.block, folded.header.hash);
            assert_eq!(status.quote.charge, expected_charge);
            assert_eq!(status.charge_id, body.charge_id_v1().unwrap());
        }
    }
}
