//! Recover execution geometry from an indexed, accepted original transaction.
//! This selects neither an RFC38 answer carrier nor a payment/approval authority.
use super::fp_replay_routing::FpReplayRoutingFaultV2 as Fault;
use super::*;
use kaspa_consensus_core::{
    palw_backend::PalwExecutionBackendV1,
    palw_freeprompt_v3::{PalwFreePromptJobV3, fp_claim_id_v3},
    palw_native_resource_v1::{NativeCoverageV1, NativeDecodeErrorV1, NativeRetainedV1, reserve_native_requests_v1},
    palw_routed_material_v2::PalwMaterialConstructionRequestsV2,
    palw_v2::PalwJobContextV2,
};
fn unavailable(why: &str) -> Fault {
    Fault::Retry(why.into())
}

pub(super) fn accepted_fp_job_context_v2(
    session: &kaspa_consensusmanager::ConsensusProxy,
    duty: &kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2,
    expected: &PalwFreePromptJobV3,
    backend: &dyn PalwExecutionBackendV1,
) -> Result<NativeRetainedV1<PalwJobContextV2>, Fault> {
    let accepted = session
        .with_consensus_api_v1(|api| api.get_block_acceptance_data(duty.accepted_block))
        .map_err(|_| unavailable("accepted FP index unavailable"))?;
    resolve_accepted_fp_job_context_v2(duty, expected, backend, &accepted, |hash| {
        session
            .with_consensus_api_v1(|api| api.get_block_body(hash))
            .map_err(|_| unavailable("accepted FP body unavailable (possibly pruned)"))
    })
}

fn resolve_accepted_fp_job_context_v2(
    duty: &kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2,
    expected: &PalwFreePromptJobV3,
    backend: &dyn PalwExecutionBackendV1,
    accepted: &kaspa_consensus_core::acceptance_data::AcceptanceData,
    mut read_body: impl FnMut(Hash64) -> Result<kaspa_consensus_core::block::BlockTransactionsV3, Fault>,
) -> Result<NativeRetainedV1<PalwJobContextV2>, Fault> {
    if !duty.free_prompt {
        return Err(unavailable("not a free-prompt duty"));
    }
    for member in accepted {
        let body = read_body(member.block_hash)?;
        for entry in &member.accepted_transactions {
            let tx = body.get(entry.index_within_block as usize).ok_or_else(|| unavailable("accepted FP index outside body"))?;
            if tx.id() != entry.transaction_id || !tx.id_is_current() {
                return Err(unavailable("accepted FP TX identity changed"));
            }
            if tx.subnetwork_id != kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT {
                continue;
            }
            let bytes = if tx.payload.starts_with(b"FPA1") {
                let Some(view) = kaspa_consensus_core::palw_fp_answer_accounting_v1::palw_fp_answer_envelope_view_v1(
                    &tx.payload,
                    u32::MAX as usize,
                ) else {
                    continue;
                };
                view.inner
            } else {
                &tx.payload
            };
            // This decoder scans the bounded codec and reserves expanded heap BEFORE decode.
            let original = match kaspa_consensus_core::palw_fp_answer_accounting_v1::palw_fp_accounting_original_decode_v1(bytes) {
                Ok(original) => original,
                Err(NativeDecodeErrorV1::Resource(why)) => return Err(Fault::Resource(why)),
                Err(NativeDecodeErrorV1::Decode(_)) => continue,
            };
            let c = &original.commitment;
            // fp_claim_id_v3 -> borsh 1.5.1 to_vec starts with 1024 bytes.
            // For final encoded length n, Vec's amortized growth has old capacity
            // <= max(1024,n) and replacement capacity <= 2*max(1024,n).
            // Admit both simultaneously if realloc moves. canonical_id streams
            // this slice into an inline Blake2b state; it creates no second Vec.
            let len = borsh::object_length(c).map_err(|_| unavailable("FP commitment encoded length unavailable"))?;
            let bound = (len as u64)
                .max(1024)
                .checked_mul(3)
                .ok_or_else(|| Fault::Resource(NativeCoverageV1::overflow::<PalwJobContextV2>("FP claim hash scratch")))?;
            let mut q = PalwMaterialConstructionRequestsV2::new();
            q.vec_v1::<u8>(bound).map_err(Fault::Resource)?;
            let _hash = reserve_native_requests_v1(q.bytes_v1()).map_err(Fault::Resource)?;
            if fp_claim_id_v3(c) != duty.claim_id {
                continue;
            }
            if c.job != *expected
                || c.job.class_id != duty.class_id
                || c.job.executor_bond != duty.executor_bond.0
                || c.execution_root != duty.execution_root
                || c.trace_root != duty.trace_root
                || c.output_root != duty.output_root
                || c.work_leaves != duty.work_leaves
            {
                return Err(unavailable("accepted FP commitment differs from duty or served job"));
            }
            let _context = fp_replay_routing::BoundFpReplayV2::context_scratch_v2().map_err(Fault::Resource)?;
            let context = backend
                .fp_job_context_for_executed_v1(&c.job, c.decode_tokens_executed)
                .ok_or_else(|| unavailable("backend cannot describe accepted FP executed count"))?;
            if !duty
                .fp_job_pin_v1()
                .is_some_and(|pin| kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&context) == pin)
            {
                return Err(unavailable("accepted FP executed context differs from native pin"));
            }
            return NativeRetainedV1::try_from_clone_v1(&context).map_err(Fault::Resource);
        }
    }
    Err(unavailable("accepted original FP commitment unavailable; no ceiling substitution"))
}
