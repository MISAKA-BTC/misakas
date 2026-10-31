//! Actual accepted native round → untouched EVM producer template → validator.
//! Private harness keys/trace roots/seat signatures, not K3, real inference or
//! fabricated native39 settlement/maturity facts. Market remains inactive.
#![allow(warnings)]
use super::*;
use kaspa_consensus_core::{
    evm::{EVM_ROUND_GAS_BUDGET_V1, MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK, evm_distinct_permitted_rounds_v1, evm_user_gas_cap_v1},
    palw_execution_lane_v1::{PALW_EXEC_ROUND_MS, palw_execution_permits_v1},
};

#[tokio::test]
async fn evm_round_gas_actual_permitted_merge_with_market_inactive_matches_producer_and_validator() {
    let mut at = a_floor_final_scheduled_with_edit(SeedDraw::MintsATicket, None, false, |params| {
        // Restore the shipped EVM lane, which the generic round fixture disables
        // for its historical clock tests. Do not open the model-market EVM face.
        params.evm_activation_daa_score = 0;
        params.palw_model_evm = None;
        params.validate_palw_v2().expect("fully validated private EVM/round configuration");
    })
    .await;
    assert!(at.chain.config.params.is_evm_active(at.chain.sink_daa()));
    assert!(at.chain.config.params.palw_model_evm.is_none());
    assert!(at.chain.config.params.palw_execution_lane.is_some());
    let ticket = at
        .schedule
        .quanta
        .iter()
        .filter(|q| q.final_id == at.claim_id)
        .min_by_key(|q| q.scheduled_round)
        .copied()
        .expect("real accepted Final earned a ticket");
    let round = ticket.scheduled_round;
    let width = at.lane.width_of_span_len(at.target, at.span_daa);
    let permit = palw_execution_permits_v1(&at.schedule, round, width)
        .into_iter()
        .find(|p| p.bond == at.executor)
        .expect("actual schedule permits the executor");
    assert_eq!(permit.quantum_id, ticket.quantum_id);
    let (round_block, _) = a_round_block_in_the_target_span(&mut at, round, permit.index, 500_000).await;
    assert!(at.chain.ctx.consensus.block_status(round_block.header.hash).has_block_body(), "native round body actually accepted");

    at.chain.ctx.simulated_time =
        at.chain.ctx.simulated_time.max(at.chain.config.params.genesis.timestamp + (round + 1) * PALW_EXEC_ROUND_MS);
    let (parent, permitted_count) = {
        let vp = at.chain.vp();
        let virtual_state = vp.virtual_stores.read().state.get().unwrap();
        assert!(virtual_state.ghostdag_data.mergeset_reds.contains(&round_block.header.hash));
        let (parent, state) = at.chain.tip_state();
        assert_eq!(parent, virtual_state.ghostdag_data.selected_parent);
        let verdicts = vp
            .palw_round_verdicts_v1(&state, &virtual_state.ghostdag_data, virtual_state.daa_score)
            .expect("actual execution lane is open");
        assert!(verdicts.round_blocks.contains(&round_block.header.hash));
        let count = evm_distinct_permitted_rounds_v1(&verdicts.uses);
        assert!(count > 0, "actual accepted signed round must contribute a permitted gas budget");
        (parent, count)
    };
    let expected_gas = evm_user_gas_cap_v1(Some(permitted_count));
    assert!(expected_gas >= MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK + EVM_ROUND_GAS_BUDGET_V1);

    // build_attempt uses the actual node producer API. In EVM mode its stamp
    // helper is assertion-only; no gas_limit or EVM commitment is hand-written.
    let (candidate, _) = at.chain.build_attempt(2, 1_000, vec![], &|_| true);
    let candidate = candidate.to_immutable();
    let hash = candidate.header.hash;
    assert!(candidate.evm_payload.transactions.is_empty(), "the candidate carries no EVM user tx");
    let actual_parent = at.chain.vp().ghostdag_store.get_selected_parent(parent).unwrap();
    eprintln!(
        "[evm-round-gas] parent={parent} parent-parent={actual_parent} round={} permitted={} expected_gas={} candidate={} daa={} timestamp={} market=None",
        round, permitted_count, expected_gas, hash, candidate.header.daa_score, candidate.header.timestamp
    );
    let result = at.chain.ctx.consensus.validate_and_insert_block(candidate.clone()).virtual_state_task.await;
    eprintln!("[evm-round-gas] actual-validator={result:?} status={:?}", at.chain.ctx.consensus.block_status(hash));
    assert!(result.is_ok(), "real producer template must pass FullUTXO validation with an accepted round: {result:?}");
    assert_eq!(at.chain.ctx.consensus.block_status(hash), BlockStatus::StatusUTXOValid);
    assert_eq!(at.chain.sink(), hash);
    let execution = at.chain.ctx.consensus.get_evm_header_of(hash).unwrap().expect("actual accepted EVM execution");
    // EVMFIX: an EMPTY acceptance commits the historical base limit (testnet-12 history),
    // not the round cap; producer and validator agree on it.
    assert_eq!(execution.gas_limit, MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK, "empty acceptance commits the base gas limit");
    assert_eq!(execution.commitment_root(), candidate.header.evm_commitment_root);
}
