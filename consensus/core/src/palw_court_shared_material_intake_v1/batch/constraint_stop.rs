//! Native inputs for one constrained Stop branch of the Batch graph.
use super::{BatchResponsePositionV1, Stage, admission::MaximumBatchDemandV1};
use crate::{
    Hash64,
    palw_court_service_v1::{ServiceCost, ServicePacket, ServiceReservations, causal::CausalServicePacketV1},
    palw_freeprompt_v3::{PalwFpStopReasonV3, PalwFreePromptCommitmentV3, fp_claim_id_v3},
    palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    palw_state_v2::{PalwChainStateV2, PalwTransitionExtrasV1},
    palw_tokenizer_rendering_v2::PalwRootedTokenizerOpeningBoundsV1,
};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

// SignedConstraintStopIntentV1 with one fixed-length ML-DSA-87 signature.
// This is the signed intent size, before the EOG material and BCT1 framing.
pub(crate) const STOP_INTENT_BYTES_V1: u64 =
    2 + 5 * 64 + 4 + 1 + (64 + 4) + 4 + crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64;
/// One Stop delivery per original. Ordinary source demands use lower ordinals.
pub(crate) const BATCH_STOP_ORDINAL_V1: u64 = u64::MAX;

pub(crate) struct ConstraintStopBatchOperandsV1 {
    claim: Hash64,
    scope: Hash64,
    registration: Hash64,
    full_ids_range: [u64; 1],
    pub generated_ids: u32,
    pub eog_replay_calls: u64,
    pub signed_operands_bytes: u64,
}

impl ConstraintStopBatchOperandsV1 {
    fn packet_ids_v1(&self, ordinal: u64) -> Result<([u8; 64], [u8; 64]), String> {
        let id = |stage| {
            crate::palw_court_shared_tir_stream_v1::stream_digest(
                b"misaka-palw/batch-constraint-stop/v1",
                &(self.claim, self.scope, self.registration, ordinal, stage),
            )
            .map(|id| id.as_bytes())
        };
        Ok((id(0u8)?, id(1u8)?))
    }

    pub(crate) fn carrier_bytes_v1(&self) -> u64 {
        self.signed_operands_bytes + crate::palw_batch_court_v1::carriage::BATCH_STOP_WIRE_OVERHEAD_V1
    }

    /// Derive before the original is installed, from the accepted tokenizer
    /// registration and the exact original being planned. The caller must
    /// already have checked the native frame and its source binding.
    pub(crate) fn derive_v1(
        state: &PalwChainStateV2,
        extras: &PalwTransitionExtrasV1,
        accepted_daa: u64,
        scope: Hash64,
        original: &PalwFreePromptCommitmentV3,
        header: &PalwPublicTirBindingHeaderV1,
    ) -> Option<Self> {
        let bounds = crate::palw_tokenizer_rendering_v2::palw_rooted_tokenizer_opening_bounds_v1(
            state,
            extras,
            accepted_daa,
            original,
            header,
        )?;
        Self::from_bounds_v1(scope, original, bounds)
    }

    pub(crate) fn from_bounds_v1(
        scope: Hash64,
        original: &PalwFreePromptCommitmentV3,
        bounds: PalwRootedTokenizerOpeningBoundsV1,
    ) -> Option<Self> {
        let count = original.decode_tokens_executed;
        let limit = original.job.decode_token_limit;
        if !original.job.is_constraint()
            || crate::palw_fp_constraint_job_v1::palw_fp_constraint_table_root_v1(&original.job).is_none()
            || count == 0
            || match original.stop_reason {
                PalwFpStopReasonV3::ExactBudgetReached => count != limit,
                PalwFpStopReasonV3::EndOfGeneration => count >= limit,
            }
        {
            return None;
        }
        Some(Self {
            claim: fp_claim_id_v3(original),
            scope,
            registration: bounds.registration_id,
            full_ids_range: [bounds.full_ids_body_bytes?],
            generated_ids: count,
            eog_replay_calls: bounds.eog_replay_calls,
            // The tuple carries an empty rendering vector after the EOG opening.
            signed_operands_bytes: STOP_INTENT_BYTES_V1 + bounds.eog_opening_bytes + 4,
        })
    }

    /// Join one complete FullIds delivery to its verifier and signed Stop.
    /// The native profile prices the ID scan, EOG work, signature and I/O.
    /// The caller joins the returned Stop node to the case terminal.
    pub(crate) fn graph_v1(
        &self,
        window: Range<u64>,
        verification_cost: ServiceCost,
        stop_cost: ServiceCost,
    ) -> Result<(MaximumBatchDemandV1<'_>, [CausalServicePacketV1; 2]), String> {
        if stop_cost.bytes < self.carrier_bytes_v1() {
            return Err("Batch Stop cost omits signed carrier bytes".into());
        }
        let ordinal = BATCH_STOP_ORDINAL_V1;
        let (verification, stop) = self.packet_ids_v1(ordinal)?;
        let packet = |id, cost| ServicePacket { id, release_daa: window.start, service_before: window.end, cost };
        let demand = MaximumBatchDemandV1 { ordinal, range_maximums: &self.full_ids_range, predecessors: &[], verification };
        Ok((
            demand,
            [
                CausalServicePacketV1 { packet: packet(verification, verification_cost), predecessors: vec![] },
                CausalServicePacketV1 { packet: packet(stop, stop_cost), predecessors: vec![verification] },
            ],
        ))
    }

    /// Check the retained original, actual FullIds acceptance and the exact
    /// Stop reservation, returning the accepted notice signer. The scheduled
    /// verifier slot is a capacity promise.
    pub(crate) fn check_rooted_slot_v1(
        &self,
        state: &PalwChainStateV2,
        at: u64,
        carrier_bytes: u64,
    ) -> Result<crate::palw_state_v2::PalwBondKeyV2, String> {
        use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
        let ordinal = BATCH_STOP_ORDINAL_V1;
        let key = Key::CourtBatchDemand(self.claim, ordinal);
        let Some(Row::CourtBatchDemand(demand)) = state.staged_court_row_v1(&key) else {
            return Err("Batch Stop has no accepted demand".into());
        };
        demand.check(&key)?;
        demand.check_context(state)?;
        let Some(Row::CourtSharedQuery(notice)) = state.staged_court_row_v1(&Key::CourtSharedQuery(demand.notice_id)) else {
            return Err("Batch Stop has no accepted notice".into());
        };
        let [range] = demand.demand.ranges.as_slice() else {
            return Err("Batch Stop requires its single FullIds range".into());
        };
        if demand.scope_id != self.scope
            || range.id != super::MaterialTargetV1::FullIds.id_v1(self.claim)?
            || u64::from(range.bytes) != self.full_ids_range[0]
            || range.authentication != crate::palw_batch_court_v1::BatchRangeAuthenticationV1::WholeRange
            || at >= demand.demand.case_window.collect_until_daa
        {
            return Err("Batch Stop differs from its accepted FullIds demand or collection window".into());
        }
        let ids_key = Key::CourtSharedIds(self.claim);
        let Some(Row::CourtSharedIds(ids)) = state.staged_court_row_v1(&ids_key) else {
            return Err("Batch Stop has no accepted FullIds".into());
        };
        ids.check(&ids_key)?;
        if ids.scope_id != self.scope
            || ids.material_until_daa != demand.material_until()
            || ids.ids.len() != self.generated_ids as usize
        {
            return Err("Batch Stop FullIds scope, retention or count differs".into());
        }
        let service = state.court_service_row_v3_v1().ok_or("Batch Stop has no rooted causal service book")?;
        let context = crate::palw_court_shared_tir_state_v1::context(state, self.claim)?;
        context.check_binding(&service.state)?;
        if at >= context.hard_daa || ids.accepted_daa < context.original_accepted_daa {
            return Err("Batch Stop or its FullIds fall outside the original clock".into());
        }
        let binding = service.state.claims.get(&self.claim).ok_or("Batch Stop has no live original binding")?;
        self.check_slots_v1(ordinal, ids.accepted_daa, at, carrier_bytes, &service.state.book, &binding.packet_ids, &service.causal)?;
        Ok(notice.opened_by)
    }

    #[allow(clippy::too_many_arguments)]
    fn check_slots_v1(
        &self,
        ordinal: u64,
        ids_daa: u64,
        stop_daa: u64,
        carrier_bytes: u64,
        book: &ServiceReservations,
        owned: &BTreeSet<[u8; 64]>,
        causal: &BTreeMap<[u8; 64], Vec<[u8; 64]>>,
    ) -> Result<(), String> {
        let finish =
            BatchResponsePositionV1 { demand_ordinal: ordinal, range_index: 0 }.packet_id(self.claim, self.scope, Stage::Finish)?;
        let (verification, stop) = self.packet_ids_v1(ordinal)?;
        let depends = |child, parent| causal.get(&child).is_some_and(|ids| ids.binary_search(&parent).is_ok());
        let finish_daa = book.assigned_daa(&finish).ok_or("Batch Stop has no FullIds Finish reservation")?;
        if carrier_bytes != self.carrier_bytes_v1()
            || ![finish, verification, stop].iter().all(|id| owned.contains(id))
            || !depends(verification, finish)
            || !depends(stop, verification)
            || ids_daa > finish_daa
            || !book.assigned_daa(&verification).is_some_and(|daa| finish_daa < daa && daa < stop_daa)
            || book.assigned_daa(&stop) != Some(stop_daa)
            || book.packet(&stop).is_none_or(|packet| packet.cost.bytes < carrier_bytes)
        {
            return Err("Batch Stop lacks its funded causal slot or FullIds accepted by Finish".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::{MaterialIntakeScheduleV1, admission::BatchAdmissionV1};
    use super::*;
    use crate::palw_batch_court_v1::BatchCourtWindowV1;
    use crate::palw_court_service_v1::ServiceReservations;

    #[test]
    fn batch_stop_reserves_verified_full_ids_before_the_stop() {
        let h = Hash64::from_u64_word;
        let claim = h(1);
        let scope = h(2);
        let operands = ConstraintStopBatchOperandsV1 {
            claim,
            scope,
            registration: h(3),
            full_ids_range: [crate::palw_tir_public_source_v1::full_ids_wire_bytes_v1(2_050)],
            generated_ids: 2_050,
            eog_replay_calls: 2,
            signed_operands_bytes: STOP_INTENT_BYTES_V1 + 148,
        };
        let cost = |bytes| ServiceCost { bytes, work: 1, io: 1, controls: 1 };
        let first = 3_121;
        let before = 9_999;
        let stop_bytes = operands.carrier_bytes_v1();
        assert!(operands.graph_v1(first..before, cost(1), cost(stop_bytes - 1)).is_err());
        let (demand, [verification, stop]) = operands.graph_v1(first..before, cost(1), cost(stop_bytes)).unwrap();
        let mut terminal = stop.clone();
        terminal.packet.id = h(4).as_bytes();
        terminal.predecessors = vec![stop.packet.id];
        let plan = BatchAdmissionV1 {
            claim,
            scope,
            demands: &[demand],
            terminal: terminal.packet.id,
            recovery_daa: 1,
            maximum_packets: 8,
            window: BatchCourtWindowV1 {
                claim_accepted_daa: 0,
                challenge_opened_daa: first - 1,
                collect_until_daa: 10_000,
                material_until_daa: 11_000,
            },
        };
        let storage = MaterialIntakeScheduleV1 {
            sompi_per_byte: 0,
            draft_ttl_daa: 8,
            cover: crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalParameterCoverBudgetV1 {
                max_original_leaves: 1,
                max_total_edges: 1,
                max_total_coordinates: 1,
                max_root_memberships: 1,
            },
        };
        let mut book = ServiceReservations::new(cost(64 * 1024)).unwrap();
        let verify_id = verification.packet.id;
        let stop_id = stop.packet.id;
        let (required, graph) =
            plan.reserve_v1(&mut book, vec![terminal, stop, verification], storage, |bytes| Ok(cost(bytes))).unwrap();
        let verify_daa = book.assigned_daa(&verify_id).unwrap();
        let stop_daa = book.assigned_daa(&stop_id).unwrap();
        let finish = graph.iter().find(|node| node.packet.id == verify_id).unwrap().predecessors[0];
        assert!(book.assigned_daa(&finish).unwrap() < verify_daa);
        assert!(verify_daa < stop_daa);
        assert!(stop_daa < book.assigned_daa(&plan.terminal).unwrap());
        assert!(required <= 10_000);
        let finish_daa = book.assigned_daa(&finish).unwrap();
        let owned: BTreeSet<_> = graph.iter().map(|node| node.packet.id).collect();
        let mut causal: BTreeMap<_, _> = graph.iter().map(|node| (node.packet.id, node.predecessors.clone())).collect();
        for predecessors in causal.values_mut() {
            predecessors.sort_unstable();
        }
        assert!(operands.check_slots_v1(BATCH_STOP_ORDINAL_V1, finish_daa, stop_daa, stop_bytes, &book, &owned, &causal).is_ok());
        assert!(operands.check_slots_v1(0, finish_daa, stop_daa, stop_bytes, &book, &owned, &causal).is_err());
        assert!(operands.check_slots_v1(BATCH_STOP_ORDINAL_V1, finish_daa - 1, stop_daa, stop_bytes, &book, &owned, &causal).is_ok());
        assert!(operands.check_slots_v1(BATCH_STOP_ORDINAL_V1, finish_daa + 1, stop_daa, stop_bytes, &book, &owned, &causal).is_err());
        assert!(operands.check_slots_v1(BATCH_STOP_ORDINAL_V1, finish_daa, stop_daa + 1, stop_bytes, &book, &owned, &causal).is_err());
        assert!(operands.check_slots_v1(BATCH_STOP_ORDINAL_V1, finish_daa, stop_daa, stop_bytes - 1, &book, &owned, &causal).is_err());
        let mut foreign = owned.clone();
        foreign.remove(&verify_id);
        assert!(operands.check_slots_v1(BATCH_STOP_ORDINAL_V1, finish_daa, stop_daa, stop_bytes, &book, &foreign, &causal).is_err());
        causal.remove(&verify_id);
        assert!(operands.check_slots_v1(BATCH_STOP_ORDINAL_V1, finish_daa, stop_daa, stop_bytes, &book, &owned, &causal).is_err());
    }
}
