//! Native inputs for one constrained Stop branch of the Batch graph.
use super::admission::MaximumBatchDemandV1;
use crate::{
    Hash64,
    palw_court_service_v1::{ServiceCost, ServicePacket, causal::CausalServicePacketV1},
    palw_freeprompt_v3::{PalwFpStopReasonV3, PalwFreePromptCommitmentV3, fp_claim_id_v3},
    palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    palw_state_v2::{PalwChainStateV2, PalwTransitionExtrasV1},
};
use std::ops::Range;

// SignedConstraintStopIntentV1 with one fixed-length ML-DSA-87 signature.
// This is the signed intent size, before the EOG material and BCT1 framing.
pub(crate) const STOP_INTENT_BYTES_V1: u64 =
    2 + 5 * 64 + 4 + 1 + (64 + 4) + 4 + crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64;

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
        let bounds = crate::palw_tokenizer_rendering_v2::palw_rooted_tokenizer_opening_bounds_v1(
            state,
            extras,
            accepted_daa,
            original,
            header,
        )?;
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
        ordinal: u64,
        window: Range<u64>,
        verification_cost: ServiceCost,
        stop_cost: ServiceCost,
    ) -> Result<(MaximumBatchDemandV1<'_>, [CausalServicePacketV1; 2]), String> {
        if stop_cost.bytes < self.signed_operands_bytes + crate::palw_batch_court_v1::carriage::BATCH_STOP_WIRE_OVERHEAD_V1 {
            return Err("Batch Stop cost omits signed carrier bytes".into());
        }
        let id = |stage| {
            crate::palw_court_shared_tir_stream_v1::stream_digest(
                b"misaka-palw/batch-constraint-stop/v1",
                &(self.claim, self.scope, self.registration, ordinal, stage),
            )
            .map(|id| id.as_bytes())
        };
        let verification = id(0u8)?;
        let stop = id(1u8)?;
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
        let stop_bytes = operands.signed_operands_bytes + crate::palw_batch_court_v1::carriage::BATCH_STOP_WIRE_OVERHEAD_V1;
        assert!(operands.graph_v1(0, first..before, cost(1), cost(stop_bytes - 1)).is_err());
        let (demand, [verification, stop]) =
            operands.graph_v1(0, first..before, cost(1), cost(stop_bytes)).unwrap();
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
    }
}
