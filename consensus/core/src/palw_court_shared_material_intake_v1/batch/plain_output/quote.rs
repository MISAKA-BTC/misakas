use super::{
    BATCH_PLAIN_OUTPUT_ORDINAL_V2, BatchPlainOutputReceiptV2, BatchResponsePositionV1, SignedBatchPlainOutputV2, Stage,
    batch_numerical_service_ids_v1, batch_plain_output_control_id_v2, plain_output_context_v2,
};
use crate::{
    Hash64,
    palw_batch_court_v1::carriage::{BATCH_PLAIN_OUTPUT_WIRE_HEADER_BYTES_V2, BatchPlainOutputTerminalV2},
    palw_court_close_receipt_v1::NativeCourtRulesV1,
    palw_court_shared_tir_state_v1::BatchNoticeTargetV1,
    palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row},
    palw_state_v2::{PalwBondKeyV2, PalwBondStatusV2, PalwChainStateV2, PalwStateParamsV2},
};

/// A signing request for an accepted plain Output notice at its reserved slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BatchPlainOutputVerifyQuoteV2 {
    pub unsigned: SignedBatchPlainOutputV2,
    pub signing_digest: Hash64,
    pub slot_daa: u64,
}

pub fn quote_batch_plain_output_verify_v2(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    rules: &NativeCourtRulesV1,
    claim: Hash64,
    filer: PalwBondKeyV2,
) -> Result<BatchPlainOutputVerifyQuoteV2, String> {
    rules.check()?;
    let context = plain_output_context_v2(state, claim)?;
    let source = context.source(state)?;
    let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, claim).ok_or("plain Output original absent")?;
    let service = state.court_service_row_v3_v1().ok_or("plain Output requires a causal service book")?;
    context.check_binding(&service.state)?;
    let binding = service.state.claims.get(&claim).ok_or("plain Output lost its service binding")?;
    let book = &service.state.book;
    let control = batch_plain_output_control_id_v2(claim, context.scope_id).map_err(|why| format!("{why:?}"))?;
    let slot_daa = book.assigned_daa(&control).ok_or("plain Output has no assigned Verify slot")?;
    let carrier_bytes = SignedBatchPlainOutputV2::BYTES + BATCH_PLAIN_OUTPUT_WIRE_HEADER_BYTES_V2;
    if !rules.tir_active
        || rules.step_ladder != frame.plan_v1().owner_work.rules.max_step_leaf_count
        || rules.at.daa_score != slot_daa
        || slot_daa >= context.hard_daa
        || state.last_point().is_none_or(|prior| prior.daa_score > slot_daa || prior.blue_score >= rules.at.blue_score)
        || carrier_bytes > rules.court.max_close_bytes()
        || book.packet(&control).is_none_or(|packet| {
            packet.cost.bytes < BatchPlainOutputReceiptV2::ROOTED_MAX_BYTES || packet.cost.controls != 1 || packet.cost.work == 0
        })
    {
        return Err("plain Output Verify point or funded capacity differs".into());
    }
    let (_, key) = state.court_shared_initial_notice_v1(claim).ok_or("plain Output requires the accepted initial notice")?;
    let Some(Row::CourtSharedQuery(notice)) = state.staged_court_row_v1(key) else {
        return Err("plain Output initial notice is absent".into());
    };
    if notice.opened_by != filer || notice.check_context_and_batch_target(state)? != Some(BatchNoticeTargetV1::PlainOutputFullIds) {
        return Err("plain Output filer or notice differs from the accepted branch".into());
    }
    super::super::check_external_batch_filer_v1(frame, filer)?;
    let bond = state.bond(&filer).ok_or("plain Output filer bond is absent")?;
    if !matches!(bond.status, PalwBondStatusV2::Active) || bond.collateral < params.min_collateral_sompi() {
        return Err("plain Output filer is inactive or below the floor".into());
    }
    let demand_key = Key::CourtBatchDemand(claim, BATCH_PLAIN_OUTPUT_ORDINAL_V2);
    let Some(Row::CourtBatchDemand(demand)) = state.staged_court_row_v1(&demand_key) else {
        return Err("plain Output requires an accepted FullIds demand".into());
    };
    demand.check(&demand_key)?;
    demand.check_context(state)?;
    let Some(Row::CourtSharedIds(ids)) = state.staged_court_row_v1(&Key::CourtSharedIds(claim)) else {
        return Err("plain Output requires accepted FullIds".into());
    };
    ids.check_context(state)?;
    let finish = BatchResponsePositionV1 { demand_ordinal: BATCH_PLAIN_OUTPUT_ORDINAL_V2, range_index: 0 }.packet_id(
        claim,
        context.scope_id,
        Stage::Finish,
    )?;
    let (_, terminal) = batch_numerical_service_ids_v1(claim, context.scope_id).map_err(|why| format!("{why:?}"))?;
    let depends = |child, parent| service.causal.get(&child).is_some_and(|parents| parents.binary_search(&parent).is_ok());
    if ![finish, control, terminal].iter().all(|id| binding.packet_ids.contains(id))
        || !depends(control, finish)
        || !depends(terminal, control)
        || ids.accepted_daa < notice.opened_daa
        || book.assigned_daa(&finish).is_none_or(|daa| ids.accepted_daa > daa || daa >= slot_daa)
        || book.assigned_daa(&terminal).is_none_or(|daa| daa <= slot_daa || daa >= context.hard_daa)
    {
        return Err("plain Output FullIds arrived outside the funded causal path".into());
    }
    let authenticated = source.retain_complete_ids_v1(ids.rows_root, &ids.ids)?;
    let unsigned = SignedBatchPlainOutputV2 {
        version: 2,
        claim,
        scope: context.scope_id,
        source: context.source_id,
        notice: notice.request_id,
        policy: crate::palw_court_shared_tir_root_v1::shared_tir_root_policy_id_v1(rules)?,
        material: crate::palw_court_shared_tir_builder_store_v1::output_terminal::ids_material_id_v1(&authenticated)?,
        filer,
        signature: [0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
    };
    let signing_digest = unsigned.signing_message_v2(rules.network_domain)?;
    Ok(BatchPlainOutputVerifyQuoteV2 { unsigned, signing_digest, slot_daa })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatchPlainOutputTerminalQuoteV2 {
    pub control: BatchPlainOutputTerminalV2,
    pub slot_daa: u64,
}

pub fn quote_batch_plain_output_terminal_v2(
    state: &PalwChainStateV2,
    claim: Hash64,
) -> Result<BatchPlainOutputTerminalQuoteV2, String> {
    let context = plain_output_context_v2(state, claim)?;
    let service = state.court_service_row_v3_v1().ok_or("plain Output terminal needs a causal service book")?;
    context.check_binding(&service.state)?;
    let (_, terminal) = batch_numerical_service_ids_v1(claim, context.scope_id).map_err(|why| format!("{why:?}"))?;
    let binding = service.state.claims.get(&claim).ok_or("plain Output lost its service binding")?;
    let slot_daa = service.state.book.assigned_daa(&terminal).ok_or("plain Output terminal has no assigned slot")?;
    if !binding.packet_ids.contains(&terminal)
        || slot_daa >= context.hard_daa
        || state.last_point().is_none_or(|point| point.daa_score > slot_daa)
    {
        return Err("plain Output terminal slot is outside its original window".into());
    }
    let Some(Row::CourtBatchPlainOutputReceiptV2(row)) = state.staged_court_row_v1(&Key::CourtBatchPlainOutputReceiptV2(claim)) else {
        return Err("plain Output terminal has no accepted receipt".into());
    };
    row.check_context(state)?;
    if row.receipt.at.daa_score >= slot_daa || row.receipt.verdict == crate::palw_tir_objective_v1::PalwTirOutputStopVerdictV1::Holds {
        return Err("plain Output terminal requires a prior fault receipt".into());
    }
    let control = BatchPlainOutputTerminalV2 { claim, receipt_digest: row.receipt.digest_v2()? };
    Ok(BatchPlainOutputTerminalQuoteV2 { control, slot_daa })
}
