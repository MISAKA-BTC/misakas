//! RFC38's actual transaction answer check, separate from the legacy served FPA1 material seam.
use super::*;
use kaspa_consensus_core::palw_native_resource_v1::NativeDecodeErrorV1;

pub(super) fn canonical_price_result_v1(
    result: Result<
        kaspa_consensus_core::palw_state_v2::PalwFpCommitmentPriceV1,
        kaspa_consensus_core::palw_state_v2::PalwStateV2Error,
    >,
) -> Result<kaspa_consensus_core::palw_state_v2::PalwFpCommitmentPriceV1, String> {
    use kaspa_consensus_core::palw_state_v2::PalwStateV2Error;
    match result {
        Ok(price) => Ok(price),
        Err(why @ (PalwStateV2Error::NativeResource(_) | PalwStateV2Error::NativeContextUnavailable(_))) => {
            Err(format!("native canonical price unavailable; claim deferred: {why}"))
        }
        Err(why) => Err(format!("the chain would refuse this canonical claim: {why}")),
    }
}

impl PalwPanelService {
    /// A read failure/ambiguous winning carrier never becomes approval or an accusation.
    /// The accepted TX body is borrowed through its store Arc, not cloned into a caller-adopted claim DTO.
    pub(super) fn fp_accounting_answer_for_seat_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        duty: &kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2,
    ) -> Result<(), NativeDecodeErrorV1<&'static str>> {
        let fail = NativeDecodeErrorV1::Decode;
        if !duty.free_prompt {
            return Ok(());
        }
        let Some(fence) = self.consensus_config.params.palw_fp_answer_accounting_v1_fence() else {
            return Ok(());
        };
        let header =
            session.palw_claim_block_header_v2(duty.accepted_block).ok_or_else(|| fail("accepted claim header unavailable"))?;
        if !fence.is_active(header.daa_score) {
            return Ok(());
        }
        let accepted = session
            .with_consensus_api_v1(|api| api.get_block_acceptance_data(duty.accepted_block))
            .map_err(|_| fail("accepted claim TX index unavailable"))?;
        let backend = self
            .resolve_backend_for_claim_v1(session, duty.class_id, duty.artifact_root, Some(duty.claim_id))
            .map_err(|_| fail("accepted claim backend unavailable"))?;
        check_accepted_accounting_carriers_v1(duty, &accepted, backend.as_ref(), |hash| {
            session.with_consensus_api_v1(|api| api.get_block_body(hash)).map_err(|_| fail("accepted claim TX body unavailable"))
        })
    }
}

/// Shared by the live accepted-data reader and its controls; no material-pool input is accepted.
fn check_accepted_accounting_carriers_v1(
    duty: &kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2,
    accepted: &kaspa_consensus_core::acceptance_data::AcceptanceData,
    backend: &dyn kaspa_consensus_core::palw_backend::PalwExecutionBackendV1,
    mut read_body: impl FnMut(Hash64) -> Result<kaspa_consensus_core::block::BlockTransactionsV3, NativeDecodeErrorV1<&'static str>>,
) -> Result<(), NativeDecodeErrorV1<&'static str>> {
    let fail = NativeDecodeErrorV1::Decode;
    let mut carrier = None;
    for member in accepted.iter() {
        let body = read_body(member.block_hash)?;
        for entry in &member.accepted_transactions {
            let tx = body.get(entry.index_within_block as usize).ok_or_else(|| fail("accepted TX index out of body"))?;
            if tx.id() != entry.transaction_id || !tx.id_is_current() {
                return Err(fail("accepted TX identity changed"));
            }
            if tx.subnetwork_id != kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT || !tx.payload.starts_with(b"FPA1")
            {
                continue;
            }
            let Some(view) =
                kaspa_consensus_core::palw_fp_answer_accounting_v1::palw_fp_answer_envelope_view_v1(&tx.payload, u32::MAX as usize)
            else {
                continue;
            };
            let original = match kaspa_consensus_core::palw_fp_answer_accounting_v1::palw_fp_accounting_original_decode_v1(view.inner)
            {
                Ok(original) => original,
                // An undecodable unrelated FP variant is not evidence about this duty. A missing
                // target still withholds approval; quota refusal must retain its retryable type.
                Err(NativeDecodeErrorV1::Decode(_)) => continue,
                Err(NativeDecodeErrorV1::Resource(why)) => return Err(NativeDecodeErrorV1::Resource(why)),
            };
            if kaspa_consensus_core::palw_freeprompt_v3::fp_claim_id_v3(&original.commitment) != duty.claim_id {
                continue;
            }
            drop(original);
            // No native winning-carrier index exists. Never silently choose one same-claim carrier over another.
            if carrier.replace(tx.id()).is_some() {
                return Err(fail("ambiguous accounting winning carrier; index unavailable"));
            }
            kaspa_consensus_core::palw_fp_answer_accounting_v1::palw_fp_answer_bind_duty_v1(tx, duty, backend)?;
        }
    }
    carrier.ok_or_else(|| fail("accepted claim has no literal accounting carrier"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        acceptance_data::{AcceptedTxEntry, MergesetBlockAcceptanceData},
        palw_backend::PalwExecutionBackendV1,
        palw_fp_answer_accounting_v1::{palw_fp_answer_bind_duty_v1, palw_fp_answer_envelope_retain_encode_v1},
        palw_freeprompt_v3::{PALW_FP_V3_VERSION, PalwFpCommitmentTxPayloadV3},
        palw_producer_v2::PalwSeatDutyV2,
        subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT,
        tx::{ScriptPublicKey, TransactionOutput},
    };
    #[test]
    fn accounting_canonical_quote_preserves_success_and_defers_only_native_faults() {
        use kaspa_consensus_core::{
            palw_native_resource_v1::NativeCoverageV1,
            palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1,
            palw_state_v2::{PalwFpCommitmentPriceV1, PalwStateV2Error},
        };
        for fault in [
            PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<u8>("quote", "capacity")),
            PalwStateV2Error::NativeContextUnavailable(PalwSourceBootstrapContextFaultV1::StoreUnreadable),
        ] {
            let message = canonical_price_result_v1(Err(fault)).unwrap_err();
            assert!(message.starts_with("native canonical price unavailable; claim deferred:"));
            assert!(!message.contains("the chain would refuse"));
        }
        assert!(canonical_price_result_v1(Err(PalwStateV2Error::ZeroQuanta)).unwrap_err().starts_with("the chain would refuse"));
        let price = canonical_price_result_v1(Ok(PalwFpCommitmentPriceV1 {
            quanta: 2,
            pwu: 8,
            reserved: 10,
            rights_reserved: 3,
            priced_in_compute: true,
            derived_work: None,
        }))
        .unwrap();
        assert_eq!((price.quanta, price.pwu, price.reserved, price.rights_reserved), (2, 8, 10, 3));
    }
    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn fixture() -> (
        misaka_palw_base0::backend::Base0Backend,
        PalwSeatDutyV2,
        Transaction,
        Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>,
    ) {
        let backend = crate::palw_panel::seat_s_tests::floor_backend();
        let (job, prompt) = crate::palw_panel::seat_s_tests::floor_fp_job(&backend);
        let run = backend.execute_free_prompt(&job, &prompt.iter().map(|id| *id as usize).collect::<Vec<_>>()).unwrap();
        let shape = backend.capture_shape(&run.outcome.material).unwrap();
        let c = kaspa_consensus_core::palw_fp_execution_v3::palw_fp_commitment_from_context_v3(&job, &shape.job_context, &run, 9999)
            .unwrap();
        let payload = PalwFpCommitmentTxPayloadV3 {
            version: PALW_FP_V3_VERSION,
            commitment: c.clone(),
            prompt_token_ids: prompt,
            signature: vec![0; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        };
        let wire = palw_fp_answer_envelope_retain_encode_v1(&borsh::to_vec(&payload).unwrap(), &run.output_token_ids).unwrap();
        let (wire, lease) = wire.into_parts_v1();
        let tx = Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_PALW_FP_COMMITMENT, 0, wire);
        let duty = PalwSeatDutyV2 {
            model_panel_one: false,
            accepted_block: h(1),
            claim_id: payload.claim_id(),
            class_id: job.class_id,
            artifact_root: misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().unwrap(),
            seat_bond: PalwBondKeyV2(TransactionOutpoint::new(h(2), 0)),
            executor_bond: PalwBondKeyV2(job.executor_bond),
            execution_root: c.execution_root,
            trace_root: c.trace_root,
            output_root: c.output_root,
            bound_daa: 100,
            receipt_deadline: 700,
            panel_anchor: h(3),
            seat_index: 0,
            panel_seat_count: 5,
            pwu: 1,
            quanta: 1,
            free_prompt: true,
            work_leaves: c.work_leaves,
            job_identity: kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_v1(&c),
            tir_shard: None,
        };
        (backend, duty, tx, lease)
    }
    fn acceptance(txs: &[Transaction]) -> kaspa_consensus_core::acceptance_data::AcceptanceData {
        vec![MergesetBlockAcceptanceData {
            block_hash: h(11),
            accepted_transactions: txs
                .iter()
                .enumerate()
                .map(|(i, t)| AcceptedTxEntry { transaction_id: t.id(), index_within_block: i as u32 })
                .collect(),
        }]
    }
    #[test]
    fn accounting_seat_uses_actual_accepted_carrier_and_own_backend_context() {
        // Real BASE-0 execution/root binding; the zero signature is a transport fixture, not a native approval/campaign.
        let (backend, duty, tx, _ticket) = fixture();
        assert_eq!(palw_fp_answer_bind_duty_v1(&tx, &duty, &backend).unwrap(), tx.id());
        let accepted = acceptance(std::slice::from_ref(&tx));
        let body = Arc::new(vec![tx]);
        check_accepted_accounting_carriers_v1(&duty, &accepted, &backend, |_| Ok(body.clone().into())).unwrap();
    }
    #[test]
    fn accounting_seat_rejects_changed_answer_job_pin_and_wrong_roots() {
        let (backend, duty, tx, _ticket) = fixture();
        let mut answer = tx.clone();
        let last = answer.payload.len() - 1;
        answer.payload[last] ^= 1;
        answer.finalize();
        assert!(matches!(palw_fp_answer_bind_duty_v1(&answer, &duty, &backend), Err(NativeDecodeErrorV1::Decode(_))));
        for n in 0..4 {
            let mut changed = duty.clone();
            match n {
                0 => changed.job_identity = h(50),
                1 => changed.output_root = h(51),
                2 => changed.execution_root = h(52),
                _ => changed.claim_id = h(53),
            }
            assert!(palw_fp_answer_bind_duty_v1(&tx, &changed, &backend).is_err());
        }
    }
    #[test]
    fn accounting_seat_refuses_ambiguous_carriers_and_stale_accepted_indices() {
        let (backend, duty, tx, _ticket) = fixture();
        let mut duplicate = tx.clone();
        duplicate.outputs.push(TransactionOutput::new(1, ScriptPublicKey::default()));
        duplicate.finalize();
        assert_ne!(tx.id(), duplicate.id());
        let body = Arc::new(vec![tx.clone(), duplicate]);
        let accepted = acceptance(&body);
        assert!(matches!(
            check_accepted_accounting_carriers_v1(&duty, &accepted, &backend, |_| Ok(body.clone().into())),
            Err(NativeDecodeErrorV1::Decode("ambiguous accounting winning carrier; index unavailable"))
        ));
        let body = Arc::new(vec![tx]);
        let mut accepted = acceptance(&body);
        accepted[0].accepted_transactions[0].transaction_id = h(99);
        assert!(check_accepted_accounting_carriers_v1(&duty, &accepted, &backend, |_| Ok(body.clone().into())).is_err());
        accepted[0].accepted_transactions[0].index_within_block = 8;
        assert!(check_accepted_accounting_carriers_v1(&duty, &accepted, &backend, |_| Ok(body.clone().into())).is_err());
        assert!(
            check_accepted_accounting_carriers_v1(&duty, &accepted, &backend, |_| Err(NativeDecodeErrorV1::Decode("missing body")))
                .is_err()
        );
    }
    #[test]
    fn accounting_seat_skips_unrelated_undecodable_original_before_target() {
        let (backend, duty, tx, _ticket) = fixture();
        let wire = palw_fp_answer_envelope_retain_encode_v1(&[0xff, 0xff], &[1]).unwrap();
        let unrelated = Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_PALW_FP_COMMITMENT, 0, wire.to_vec());
        let body = Arc::new(vec![unrelated, tx]);
        let accepted = acceptance(&body);
        check_accepted_accounting_carriers_v1(&duty, &accepted, &backend, |_| Ok(body.clone().into())).unwrap();
        let accepted = acceptance(&body[..1]);
        assert!(check_accepted_accounting_carriers_v1(&duty, &accepted, &backend, |_| Ok(body.clone().into())).is_err());
    }
    #[test]
    fn accounting_seat_never_substitutes_legacy_served_answer_material() {
        let (backend, duty, tx, _ticket) = fixture();
        let view = kaspa_consensus_core::palw_fp_answer_accounting_v1::palw_fp_answer_envelope_view_v1(&tx.payload, u32::MAX as usize)
            .unwrap();
        let original = kaspa_consensus_core::palw_fp_answer_accounting_v1::palw_fp_accounting_original_decode_v1(view.inner).unwrap();
        let legacy = kaspa_consensus_core::palw_freeprompt_v3::palw_fp_answer_encode_v1(
            &original.commitment.job,
            &original.prompt_token_ids,
            &view.answer_ids_v1().collect::<Vec<_>>(),
        );
        let served = Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_PALW_FP_COMMITMENT, 0, legacy);
        let body = Arc::new(vec![served]);
        let accepted = acceptance(&body);
        assert!(check_accepted_accounting_carriers_v1(&duty, &accepted, &backend, |_| Ok(body.clone().into())).is_err());
    }
}
