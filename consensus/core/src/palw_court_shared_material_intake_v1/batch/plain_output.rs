//! Prospective plain TFP1 output control and replay against accepted FullIds.

pub(crate) mod projection;
mod quote;
pub use quote::{
    BatchPlainOutputTerminalQuoteV2, BatchPlainOutputVerifyQuoteV2, quote_batch_plain_output_terminal_v2,
    quote_batch_plain_output_verify_v2,
};

use super::{BatchResponsePositionV1, Stage, admission::batch_numerical_service_ids_v1};
use crate::{Hash64, palw_batch_court_v1::BatchCourtErrorV1, palw_court_service_v1::ServiceCost};

/// The plain Batch branch uses a separate context. Its constructor checks
/// the already funded graph and the original's accepted tokenizer.
pub(crate) const BATCH_PLAIN_OUTPUT_CONTEXT_VERSION_V2: u16 = 3;

pub(crate) fn plain_output_context_v2(
    state: &crate::palw_state_v2::PalwChainStateV2,
    claim: Hash64,
) -> Result<&crate::palw_court_shared_tir_state_v1::SharedTirContextV1, String> {
    let context = crate::palw_court_shared_tir_state_v1::context(state, claim)?;
    if context.version != BATCH_PLAIN_OUTPUT_CONTEXT_VERSION_V2 {
        return Err("plain Batch Output requires a V2-capable original".into());
    }
    Ok(context)
}

/// The plain Output/Stop branch needs FullIds even without a numerical leaf.
pub(crate) const BATCH_PLAIN_OUTPUT_ORDINAL_V2: u64 = u64::MAX - 1;
pub(crate) const PLAIN_OUTPUT_SIGNATURE_BYTES_V2: usize = crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
pub const PLAIN_OUTPUT_SIGNATURE_CONTEXT_V2: &[u8] = b"misaka-palw/batch-plain-output/v2/mldsa87";

/// Canonical signed operands for the prospective FullIds branch. Outer
/// carriage and the accepted-state writer are separate from this codec.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[allow(dead_code)]
pub struct SignedBatchPlainOutputV2 {
    pub version: u16,
    pub claim: Hash64,
    pub scope: Hash64,
    pub source: Hash64,
    pub notice: Hash64,
    pub policy: Hash64,
    pub material: Hash64,
    pub filer: crate::palw_state_v2::PalwBondKeyV2,
    pub signature: [u8; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
}

#[allow(dead_code)]
impl SignedBatchPlainOutputV2 {
    // Version, six digests, bond outpoint, and a fixed-width signature.
    pub(crate) const BYTES: u64 = 2 + 6 * 64 + (64 + 4) + PLAIN_OUTPUT_SIGNATURE_BYTES_V2 as u64;

    pub fn signing_message_v2(&self, network: Hash64) -> Result<Hash64, String> {
        if self.version != 2
            || [network, self.claim, self.scope, self.source, self.notice, self.policy, self.material, self.filer.0.transaction_id]
                .contains(&Hash64::default())
        {
            return Err("plain Batch Output has an invalid signing scope".into());
        }
        let control = batch_plain_output_control_id_v2(self.claim, self.scope)
            .map_err(|why| format!("plain Batch Output control identity: {why:?}"))?;
        let (_, terminal) = batch_numerical_service_ids_v1(self.claim, self.scope)
            .map_err(|why| format!("plain Batch Output terminal identity: {why:?}"))?;
        crate::palw_court_shared_tir_stream_v1::stream_digest(
            b"misaka-palw/batch-plain-output-signing/v2",
            &(
                network,
                self.version,
                self.claim,
                self.scope,
                self.source,
                self.notice,
                self.policy,
                self.material,
                self.filer,
                control,
                terminal,
            ),
        )
    }

    pub fn verify_signature_v2(&self, network: Hash64, pubkey: &[u8]) -> Result<(), String> {
        if !crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(
            pubkey,
            self.signing_message_v2(network)?.as_byte_slice(),
            &self.signature,
            PLAIN_OUTPUT_SIGNATURE_CONTEXT_V2,
        ) {
            return Err("invalid plain Batch Output signature".into());
        }
        Ok(())
    }
}

/// Receipt payload for the separately versioned branch. A verifier constructs
/// it from the accepted book and material, never from a relayer's asserted verdict.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[allow(dead_code)]
pub struct BatchPlainOutputReceiptV2 {
    pub(crate) signed: SignedBatchPlainOutputV2,
    pub(crate) at: crate::palw_state_v2::PalwBlockContextV2,
    pub(crate) carrier_bytes: u64,
    pub(crate) control_rules: crate::palw_court_close_receipt_v1::NativeCourtRulesV1,
    pub(crate) control_cost: ServiceCost,
    pub(crate) verdict: crate::palw_tir_objective_v1::PalwTirOutputStopVerdictV1,
}

#[allow(dead_code)]
impl BatchPlainOutputReceiptV2 {
    const BLOCK_CONTEXT_BYTES: u64 = 64 + 3 * 8;
    // Court params: five u64, two u32, one u8. Rules add the network,
    // accepting point, ladder, eight bools and three present Option<u64>.
    const CONTROL_RULES_MAX_BYTES: u64 = 64 + Self::BLOCK_CONTEXT_BYTES + (5 * 8 + 2 * 4 + 1) + 8 + 8 + 3 * (1 + 8);
    pub(crate) const MAX_BYTES: u64 =
        SignedBatchPlainOutputV2::BYTES + Self::BLOCK_CONTEXT_BYTES + 8 + Self::CONTROL_RULES_MAX_BYTES + 4 * 8 + 1;
    pub(crate) const ROOTED_MAX_BYTES: u64 = Self::MAX_BYTES + 8;

    pub(crate) fn claim_and_filer_v2(&self) -> (Hash64, crate::palw_state_v2::PalwBondKeyV2) {
        (self.signed.claim, self.signed.filer)
    }

    pub(crate) fn digest_v2(&self) -> Result<Hash64, String> {
        crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/batch-plain-output-receipt/v2", self)
    }

    /// Runtime writer must retain this payload under the branch's own rooted key.
    pub(crate) fn verify_v2(
        state: &crate::palw_state_v2::PalwChainStateV2,
        params: &crate::palw_state_v2::PalwStateParamsV2,
        rules: &crate::palw_court_close_receipt_v1::NativeCourtRulesV1,
        signed: SignedBatchPlainOutputV2,
        carrier_bytes: u64,
        expected_cost: ServiceCost,
    ) -> Result<Self, crate::palw_native_resource_v1::NativeDecodeErrorV1<String>> {
        let verdict = verify_rooted_batch_plain_output_v2(state, params, rules, &signed, carrier_bytes, expected_cost)?;
        Ok(Self { signed, at: rules.at, carrier_bytes, control_rules: *rules, control_cost: expected_cost, verdict })
    }

    /// Recheck the stored ruling and reconstruct the existing TIR Output
    /// evidence for settlement. The writer must provide the stored receipt
    /// and rooted native rules at both service points, never a relayer body.
    pub(crate) fn recheck_terminal_v2(
        &self,
        state: &crate::palw_state_v2::PalwChainStateV2,
        params: &crate::palw_state_v2::PalwStateParamsV2,
        control_rules: &crate::palw_court_close_receipt_v1::NativeCourtRulesV1,
        terminal_rules: &crate::palw_court_close_receipt_v1::NativeCourtRulesV1,
        expected_digest: Hash64,
        control_cost: ServiceCost,
    ) -> Result<
        Option<
            crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1>,
        >,
        crate::palw_native_resource_v1::NativeDecodeErrorV1<String>,
    > {
        use crate::palw_native_resource_v1::{
            NativeDecodeErrorV1 as E, NativeRetainedV1, TirNativeRequestsV1, reserve_native_requests_v1,
        };
        use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
        use crate::palw_tir_objective_v1::PalwTirOutputStopVerdictV1;
        use crate::palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1;
        let refused = |why: &str| E::Decode(why.into());
        let context = plain_output_context_v2(state, self.signed.claim).map_err(E::Decode)?;
        let service = state.court_service_row_v3_v1().ok_or_else(|| refused("plain Batch Output terminal has no service book"))?;
        context.check_binding(&service.state).map_err(E::Decode)?;
        let (_, terminal) =
            batch_numerical_service_ids_v1(self.signed.claim, self.signed.scope).map_err(|why| E::Decode(format!("{why:?}")))?;
        if self.digest_v2().map_err(E::Decode)? != expected_digest
            || self.at != control_rules.at
            || self.control_rules != *control_rules
            || self.control_cost != control_cost
            || control_rules.network_domain != terminal_rules.network_domain
            || !terminal_rules.tir_active
            || service.state.book.assigned_daa(&terminal) != Some(terminal_rules.at.daa_score)
        {
            return Err(refused("plain Batch Output terminal substitutes receipt, network or assigned point"));
        }
        let actual =
            verify_rooted_batch_plain_output_v2(state, params, control_rules, &self.signed, self.carrier_bytes, control_cost)?;
        if actual != self.verdict {
            return Err(refused("plain Batch Output terminal recheck changes the stored verdict"));
        }
        if actual == PalwTirOutputStopVerdictV1::Holds {
            return Ok(None);
        }
        let Some(Row::CourtSharedIds(ids)) = state.staged_court_row_v1(&Key::CourtSharedIds(self.signed.claim)) else {
            return Err(refused("plain Batch Output terminal lost its authenticated FullIds"));
        };
        let mut resources = TirNativeRequestsV1::new();
        resources.exact_v1::<PalwTirPublicConvictionEvidenceV1>(1).map_err(E::Resource)?;
        resources.exact_v1::<u32>(ids.ids.len() as u64).map_err(E::Resource)?;
        let lease = reserve_native_requests_v1(resources.bytes_v1()).map_err(E::Resource)?;
        let evidence = PalwTirPublicConvictionEvidenceV1::Output { rows_root: ids.rows_root, ids: ids.ids.clone() };
        Ok(Some(NativeRetainedV1::from_owner_reserved_v1(evidence, lease)))
    }
}

/// Reconstruct a prospective V2 ruling from accepted state. The caller
/// must select the adopted profile and supply its calibrated service cost.
/// Current BCT ingress has no authority to call this verifier or write a receipt.
fn verify_rooted_batch_plain_output_v2(
    state: &crate::palw_state_v2::PalwChainStateV2,
    params: &crate::palw_state_v2::PalwStateParamsV2,
    rules: &crate::palw_court_close_receipt_v1::NativeCourtRulesV1,
    signed: &SignedBatchPlainOutputV2,
    carrier_bytes: u64,
    expected_cost: ServiceCost,
) -> Result<crate::palw_tir_objective_v1::PalwTirOutputStopVerdictV1, crate::palw_native_resource_v1::NativeDecodeErrorV1<String>> {
    use crate::palw_batch_court_v1::BatchRangeAuthenticationV1;
    use crate::palw_native_resource_v1::NativeDecodeErrorV1 as E;
    use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
    use crate::palw_state_v2::PalwBondStatusV2;
    let refused = |why: &str| E::Decode(why.into());
    rules.check().map_err(E::Decode)?;
    let (claim, scope, at) = (signed.claim, signed.scope, rules.at.daa_score);
    let context = plain_output_context_v2(state, claim).map_err(E::Decode)?;
    let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, claim)
        .ok_or_else(|| refused("plain Batch Output original is absent"))?;
    if !rules.tir_active
        || rules.step_ladder != frame.plan_v1().owner_work.rules.max_step_leaf_count
        || !(context.original_accepted_daa..context.hard_daa).contains(&at)
        || carrier_bytes
            != SignedBatchPlainOutputV2::BYTES + crate::palw_batch_court_v1::carriage::BATCH_PLAIN_OUTPUT_WIRE_HEADER_BYTES_V2
        || expected_cost.bytes < BatchPlainOutputReceiptV2::ROOTED_MAX_BYTES
        || expected_cost.work == 0
        || expected_cost.controls != 1
        || carrier_bytes > rules.court.max_close_bytes()
        || (scope, signed.source) != (context.scope_id, context.source_id)
        || signed.policy != crate::palw_court_shared_tir_root_v1::shared_tir_root_policy_id_v1(rules).map_err(E::Decode)?
    {
        return Err(refused("plain Batch Output substitutes original, source, policy or accepting point"));
    }
    let notice_key = Key::CourtSharedQuery(signed.notice);
    let Some(Row::CourtSharedQuery(notice)) = state.staged_court_row_v1(&notice_key) else {
        return Err(refused("plain Batch Output lacks its accepted notice"));
    };
    notice.check(&notice_key).map_err(E::Decode)?;
    if notice.claim != claim
        || notice.scope_id != scope
        || notice.opened_by != signed.filer
        || state.court_shared_initial_notice_v1(claim) != Some((notice.opened_daa, &notice_key))
    {
        return Err(refused("plain Batch Output notice or filer differs from the accepted initial notice"));
    }
    super::check_external_batch_filer_v1(frame, signed.filer).map_err(E::Decode)?;
    let filer = state.bond(&signed.filer).ok_or_else(|| refused("plain Batch Output filer bond is absent"))?;
    if !matches!(filer.status, PalwBondStatusV2::Active) || filer.collateral < params.min_collateral_sompi() {
        return Err(refused("plain Batch Output filer is inactive or below the floor"));
    }
    signed.verify_signature_v2(rules.network_domain, &filer.pubkey).map_err(E::Decode)?;

    let demand_key = Key::CourtBatchDemand(claim, BATCH_PLAIN_OUTPUT_ORDINAL_V2);
    let Some(Row::CourtBatchDemand(demand)) = state.staged_court_row_v1(&demand_key) else {
        return Err(refused("plain Batch Output lacks its accepted FullIds demand"));
    };
    demand.check(&demand_key).map_err(E::Decode)?;
    let [range] = demand.demand.ranges.as_slice() else {
        return Err(refused("plain Batch Output must use exactly one FullIds range"));
    };
    let source = context.source(state).map_err(E::Decode)?;
    let full_ids = PlainBatchOutputV2::full_ids_bound_v2(frame.original_v1(), &source)
        .map_err(|why| E::Decode(format!("invalid plain Batch Output descriptor: {why:?}")))?;
    let set = crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame, &source, context.profile_id).map_err(E::Decode)?;
    if notice.body.is_some()
        || notice.service.is_some()
        || notice.material_until_daa != context.material_until_daa
        || !crate::palw_court_policy_v1::shared_initial_notice_daa_in_window_v1(context.original_accepted_daa, notice.opened_daa)
            .map_err(|_| refused("plain Batch Output notice clock overflows"))?
        || set.request_id(notice.ordinal).map_err(E::Decode)? != signed.notice
        || set.request_at(notice.ordinal).map_err(E::Decode)?
            != (crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::Ids { offset: 0 })
    {
        return Err(refused("plain Batch Output notice must name original FullIds"));
    }
    if demand.scope_id != scope
        || demand.notice_id != signed.notice
        || demand.material_until() != context.material_until_daa
        || demand.demand.case_window.claim_accepted_daa != context.original_accepted_daa
        || demand.demand.case_window.collect_until_daa != context.hard_daa
        || range.id != super::MaterialTargetV1::FullIds.id_v1(claim).map_err(E::Decode)?
        || range.authentication != BatchRangeAuthenticationV1::WholeRange
        || demand.demand.case_window.challenge_opened_daa != notice.opened_daa
        || demand.demand.response_deadline_daa > context.hard_daa
        || demand.demand.case_id
            != crate::palw_batch_court_v1::rooted::batch_case_id_v1(
                claim,
                scope,
                signed.notice,
                notice.opened_block,
                notice.opened_daa,
                BATCH_PLAIN_OUTPUT_ORDINAL_V2,
            )
            .map_err(E::Decode)?
        || u64::from(range.bytes) != full_ids
    {
        return Err(refused("plain Batch Output demand differs from the original or its FullIds bound"));
    }
    let ids_key = Key::CourtSharedIds(claim);
    let Some(Row::CourtSharedIds(ids)) = state.staged_court_row_v1(&ids_key) else {
        return Err(refused("plain Batch Output lacks accepted FullIds"));
    };
    ids.check(&ids_key).map_err(E::Decode)?;
    let service = state.court_service_row_v3_v1().ok_or_else(|| refused("plain Batch Output lacks its causal service book"))?;
    context.check_binding(&service.state).map_err(E::Decode)?;
    let binding = service.state.claims.get(&claim).ok_or_else(|| refused("plain Batch Output lacks its service binding"))?;
    let book = &service.state.book;
    let finish = BatchResponsePositionV1 { demand_ordinal: BATCH_PLAIN_OUTPUT_ORDINAL_V2, range_index: 0 }
        .packet_id(claim, scope, Stage::Finish)
        .map_err(E::Decode)?;
    let control = batch_plain_output_control_id_v2(claim, scope).map_err(|why| E::Decode(format!("{why:?}")))?;
    let (_, terminal) = batch_numerical_service_ids_v1(claim, scope).map_err(|why| E::Decode(format!("{why:?}")))?;
    let depends = |child, parent| service.causal.get(&child).is_some_and(|parents| parents.binary_search(&parent).is_ok());
    if ids.scope_id != scope
        || ids.material_until_daa != context.material_until_daa
        || ids.accepted_daa < notice.opened_daa
        || ![finish, control, terminal].iter().all(|id| binding.packet_ids.contains(id))
        || !depends(control, finish)
        || !depends(terminal, control)
        || book.assigned_daa(&finish).is_none_or(|daa| ids.accepted_daa > daa || daa >= at)
        || book.assigned_daa(&control) != Some(at)
        || book.assigned_daa(&terminal).is_none_or(|daa| daa <= at || daa >= context.hard_daa)
        || book.packet(&control).is_none_or(|packet| packet.cost != expected_cost)
    {
        return Err(refused("plain Batch Output lacks its funded causal slot or timely FullIds"));
    }

    let authenticated = source.retain_complete_ids_classified_v2(ids.rows_root, &ids.ids)?;
    if signed.material
        != crate::palw_court_shared_tir_builder_store_v1::output_terminal::ids_material_id_v1(&authenticated).map_err(E::Decode)?
    {
        return Err(refused("plain Batch Output signature names different authenticated FullIds"));
    }
    crate::palw_tir_objective_v1::adjudicate_original_output_stop_v1(state, frame, &source, &authenticated).map_err(E::Decode)
}

pub(crate) fn batch_plain_output_control_id_v2(claim: Hash64, scope: Hash64) -> Result<[u8; 64], BatchCourtErrorV1> {
    crate::palw_court_shared_tir_stream_v1::stream_digest(
        b"misaka-palw/batch-plain-output/v2",
        &(claim, scope, BATCH_PLAIN_OUTPUT_ORDINAL_V2),
    )
    .map(|id| id.as_bytes())
    .map_err(|_| BatchCourtErrorV1::CaseGraph)
}

pub(crate) struct PlainBatchOutputV2 {
    pub(super) claim: Hash64,
    pub(super) full_ids_range: [u64; 1],
    pub(super) control_cost: ServiceCost,
    pub(super) control_daa: u64,
}

impl PlainBatchOutputV2 {
    /// Available to the prospective compiler. The V1 original installer does
    /// not accept this descriptor or its packet grammar. This checks the
    /// literal control bytes, but a native profile still needs a calibrated
    /// replay work bound and the outer carrier size before activation.
    #[allow(dead_code)]
    pub(crate) fn derive_v2(
        original: &crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
        source: &crate::palw_tir_public_source_v1::PalwTirPublicSourceV1,
        control_cost: ServiceCost,
        control_daa: u64,
    ) -> Result<Self, BatchCourtErrorV1> {
        if control_cost.bytes < BatchPlainOutputReceiptV2::ROOTED_MAX_BYTES || control_cost.work == 0 || control_cost.controls != 1 {
            return Err(BatchCourtErrorV1::DeliveryCost);
        }
        if control_daa == 0 {
            return Err(BatchCourtErrorV1::Window);
        }
        let full_ids_bytes = Self::full_ids_bound_v2(original, source)?;
        Ok(Self {
            claim: crate::palw_freeprompt_v3::fp_claim_id_v3(original),
            full_ids_range: [full_ids_bytes],
            control_cost,
            control_daa,
        })
    }
    fn full_ids_bound_v2(
        original: &crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
        source: &crate::palw_tir_public_source_v1::PalwTirPublicSourceV1,
    ) -> Result<u64, BatchCourtErrorV1> {
        let header = source.header_v1();
        if original.job.tail.is_some()
            || original.job.is_prefix_state()
            || original.job.is_constraint()
            || crate::palw_tir_court_v1::PalwTirSelectionRuleV1::of_job_v1(&original.job).is_none()
            || original.output_root == Hash64::default()
            || !source.supports_tiled_trace_v1()
            || source.decode_count_v1() == 0
            || source.decode_count_v1() != original.decode_tokens_executed
            || original.job.class_id != header.job_context.shape_profile_id
            || crate::palw_fp_execution_v3::palw_fp_job_pin_v1(original)
                != crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&header.job_context)
            || original.execution_root != header.committed_execution_root
            || original.trace_root != header.full_logits_trace_root
            || original.work_leaves != source.leaf_count_v1()
        {
            return Err(BatchCourtErrorV1::CaseGraph);
        }
        let full_ids_bytes = crate::palw_tir_public_source_v1::full_ids_wire_bytes_v1(source.decode_count_v1());
        if !(1..=super::MATERIAL_PIPELINED_MAX_BYTES).contains(&full_ids_bytes) {
            return Err(BatchCourtErrorV1::DeliveryCost);
        }
        Ok(full_ids_bytes)
    }
}

#[cfg(test)]
pub(crate) fn rooted_receipt_fixture_v2(
    signed: SignedBatchPlainOutputV2,
) -> crate::palw_batch_plain_output_receipt_v2::RootedBatchPlainOutputReceiptV2 {
    let h = Hash64::from_u64_word;
    let at = crate::palw_state_v2::PalwBlockContextV2 { block: h(11), daa_score: 13, blue_score: 17, subsidy: 0 };
    let mut receipt = BatchPlainOutputReceiptV2 {
        signed,
        at,
        carrier_bytes: SignedBatchPlainOutputV2::BYTES + crate::palw_batch_court_v1::carriage::BATCH_PLAIN_OUTPUT_WIRE_HEADER_BYTES_V2,
        control_rules: crate::palw_court_close_receipt_v1::NativeCourtRulesV1 {
            network_domain: h(8),
            at,
            court: crate::palw_mode_v2::PalwCourtParamsV2::new(8, 1, 1).unwrap(),
            step_ladder: 8,
            prompt_ids_merkle: false,
            held_regime: true,
            decode_close_convicts_only: true,
            fp_decode_from_daa: Some(1),
            cell_closure_from_daa: Some(2),
            memory_routing_from_daa: Some(3),
            tir_active: true,
            gen_active: false,
            task_heads_active: false,
            improvement_active: false,
            constraint_active: false,
        },
        control_cost: ServiceCost { bytes: 8_192, work: 4, io: 2, controls: 1 },
        verdict: crate::palw_tir_objective_v1::PalwTirOutputStopVerdictV1::ExecutionFault,
    };
    receipt.signed.policy = crate::palw_court_shared_tir_root_v1::shared_tir_root_policy_id_v1(&receipt.control_rules).unwrap();
    crate::palw_batch_plain_output_receipt_v2::RootedBatchPlainOutputReceiptV2 { receipt, material_until_daa: 23 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_output_control_length_and_signing_scope_v2() {
        let h = Hash64::from_u64_word;
        let mut signed = SignedBatchPlainOutputV2 {
            version: 2,
            claim: h(1),
            scope: h(2),
            source: h(3),
            notice: h(4),
            policy: h(5),
            material: h(6),
            filer: crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(7), 1)),
            signature: [0; PLAIN_OUTPUT_SIGNATURE_BYTES_V2],
        };
        let original = signed.signing_message_v2(h(8)).unwrap();
        let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([121; 32]);
        let signature =
            libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, original.as_byte_slice(), PLAIN_OUTPUT_SIGNATURE_CONTEXT_V2, [122; 32])
                .unwrap();
        signed.signature.copy_from_slice(signature.as_ref());
        let public_key = key.verification_key.as_ref();
        signed.verify_signature_v2(h(8), public_key).unwrap();
        assert!(signed.verify_signature_v2(h(9), public_key).is_err());

        let bytes = borsh::to_vec(&signed).unwrap();
        assert_eq!(bytes.len() as u64, SignedBatchPlainOutputV2::BYTES);
        let rooted = rooted_receipt_fixture_v2(signed.clone());
        let receipt = &rooted.receipt;
        assert_eq!(borsh::object_length(&receipt.control_rules).unwrap() as u64, BatchPlainOutputReceiptV2::CONTROL_RULES_MAX_BYTES,);
        assert_eq!(borsh::object_length(&receipt).unwrap() as u64, BatchPlainOutputReceiptV2::MAX_BYTES);
        assert_eq!(borsh::object_length(&(&receipt, 23u64)).unwrap() as u64, BatchPlainOutputReceiptV2::ROOTED_MAX_BYTES);
        use crate::palw_batch_plain_output_receipt_v2::RootedBatchPlainOutputReceiptV2;
        use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
        let key = Key::CourtBatchPlainOutputReceiptV2(signed.claim);
        rooted.check(&key).unwrap();
        assert_eq!(borsh::to_vec(&key).unwrap()[0], 35);
        let wire = borsh::to_vec(&rooted).unwrap();
        assert_eq!(wire.len() as u64, BatchPlainOutputReceiptV2::ROOTED_MAX_BYTES);
        assert_eq!(borsh::from_slice::<RootedBatchPlainOutputReceiptV2>(&wire).unwrap(), rooted);
        assert!(borsh::from_slice::<RootedBatchPlainOutputReceiptV2>(&wire[..wire.len() - 1]).is_err());
        let row = Row::CourtBatchPlainOutputReceiptV2(Box::new(rooted.clone()));
        let row_wire = borsh::to_vec(&row).unwrap();
        assert_eq!(row_wire[0], 42);
        use crate::palw_native_resource_v1::preflight_native_value_v1;
        assert_eq!(
            preflight_native_value_v1::<Row>(row_wire.as_slice(), row_wire.len() as u64).unwrap().canonical_bytes_v1(),
            row_wire.len() as u64,
        );
        let truncated_row = &row_wire[..row_wire.len() - 1];
        assert!(preflight_native_value_v1::<Row>(truncated_row, truncated_row.len() as u64).is_err());
        assert_eq!(borsh::from_slice::<Row>(&row_wire).unwrap(), row);
        assert!(borsh::from_slice::<Row>(&row_wire[..row_wire.len() - 1]).is_err());
        assert!(rooted.check(&Key::CourtBatchPlainOutputReceiptV2(h(99))).is_err());
        assert!(rooted.check_context(&crate::palw_state_v2::PalwChainStateV2::genesis()).is_err());
        let mut expired = rooted.clone();
        expired.material_until_daa = expired.receipt.at.daa_score;
        assert!(expired.check(&key).is_err());
        let mut short_carrier = rooted.clone();
        short_carrier.receipt.carrier_bytes -= 1;
        assert!(short_carrier.check(&key).is_err());
        let mut short_reservation = rooted.clone();
        short_reservation.receipt.control_cost.bytes = BatchPlainOutputReceiptV2::ROOTED_MAX_BYTES - 1;
        assert!(short_reservation.check(&key).is_err());
        let mut nonnative = rooted;
        nonnative.receipt.signed.version = 1;
        assert!(nonnative.check(&key).is_err());

        use crate::palw_batch_court_v1::carriage::{
            BATCH_PLAIN_OUTPUT_CARRIER_MAGIC_V2, BatchPlainOutputTerminalV2, BatchPlainOutputWireV2, decode_batch_court_payload_v1,
            decode_batch_plain_output_payload_v2, encode_batch_plain_output_payload_v2,
        };
        let verify = BatchPlainOutputWireV2::Verify(signed.clone());
        let terminal = BatchPlainOutputWireV2::Terminal(BatchPlainOutputTerminalV2 { claim: h(1), receipt_digest: h(12) });
        for control in [verify, terminal] {
            let mut payload = BATCH_PLAIN_OUTPUT_CARRIER_MAGIC_V2.to_vec();
            payload.extend(borsh::to_vec(&control).unwrap());
            assert_eq!(encode_batch_plain_output_payload_v2(&control).unwrap(), payload);
            assert_eq!(decode_batch_plain_output_payload_v2(&payload).unwrap(), Some(control));
            assert!(decode_batch_court_payload_v1(&payload).unwrap().is_none());
            assert!(decode_batch_plain_output_payload_v2(&payload[..payload.len() - 1]).is_err());
        }
        let roundtrip: SignedBatchPlainOutputV2 = borsh::from_slice(&bytes).unwrap();
        assert_eq!(borsh::to_vec(&roundtrip).unwrap(), bytes);
        assert!(borsh::from_slice::<SignedBatchPlainOutputV2>(&bytes[..bytes.len() - 1]).is_err());

        assert_ne!(signed.signing_message_v2(h(9)).unwrap(), original);
        signed.source = h(10);
        assert!(signed.verify_signature_v2(h(8), public_key).is_err());
        assert_ne!(signed.signing_message_v2(h(8)).unwrap(), original);
        signed.source = h(3);
        signed.material = h(10);
        assert!(signed.verify_signature_v2(h(8), public_key).is_err());
        assert_ne!(signed.signing_message_v2(h(8)).unwrap(), original);
        signed.version = 1;
        assert!(signed.signing_message_v2(h(8)).is_err());
    }
}
