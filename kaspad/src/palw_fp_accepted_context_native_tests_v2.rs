//! Own native acceptance/body reader with ordinary PQ admission; no service capture pool.
use super::*;
use crate::palw_panel::{accepted_fp_job_context::accepted_fp_job_context_v2, fp_replay_routing::FpReplayRoutingFaultV2};
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fp_accepted_context_reads_native_pq_carrier_without_capture_or_retained_job() {
    native_context_control(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fp_accepted_context_native_decode_capacity_is_retryable() {
    const CHILD: &str = "MISAKA_FP_ACCEPTED_CONTEXT_CAP_CHILD";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let full = format!("{}::fp_accepted_context_native_decode_capacity_is_retryable", module_path!());
        let name = full.split_once("::").unwrap().1;
        assert!(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", name, "--nocapture", "--test-threads=1"])
                .env(CHILD, "1")
                .status()
                .unwrap()
                .success()
        );
        return;
    }
    native_context_control(true).await;
}

async fn native_context_control(capacity: bool) {
    on_service_stack(move || async move {
        let (config, bundle, premine, mut floats) = fixture_config_with(false, false, true, false, false);
        let mut chain = Chain::new(config, bundle, &premine);
        chain.advance(2).await;
        let profile = kaspa_consensus_core::palw_base0_profile::base0_profile_v1(
            kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY,
        )
        .unwrap();
        certify_fp_lane(&mut chain, &mut floats, profile, false).await;
        let (_, _, Some((commitment, prompt))) = public_fp_job(&chain, false, false, None) else { panic!("actual FP job") };
        let claim = carry_fp_commitment(&mut chain, &mut floats, &commitment, &prompt, false).await;
        assert!(chain.view(claim).public_fp_job.is_none(), "fixture intentionally precedes rooted original-job retention");
        let manager = Arc::new(ConsensusManager::new(Arc::new(TestConsensusFactory::new(chain.tc.clone()))).unwrap());
        let session = manager.consensus().unguarded_session();
        // Find the actual accepting block by its native accepted TX index, not the carrying header.
        let accepted_block = chain
            .history
            .iter()
            .rev()
            .find_map(|block| {
                let accepted = session.with_consensus_api_v1(|api| api.get_block_acceptance_data(block.header.hash)).ok()?;
                accepted
                    .iter()
                    .any(|member| {
                        let Ok(body) = session.with_consensus_api_v1(|api| api.get_block_body(member.block_hash)) else {
                            return false;
                        };
                        member.accepted_transactions.iter().any(|entry| {
                            let Some(tx) = body.get(entry.index_within_block as usize) else { return false };
                            if tx.id() != entry.transaction_id
                                || tx.subnetwork_id != kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT
                            {
                                return false;
                            }
                            kaspa_consensus_core::palw_fp_answer_accounting_v1::palw_fp_accounting_original_decode_v1(&tx.payload)
                                .is_ok_and(|original| original.claim_id() == claim)
                        })
                    })
                    .then_some(block.header.hash)
            })
            .expect("native acceptance index includes ordinarily admitted PQ commitment");
        let native = chain.tc.palw_fraud_filer_target_v1(claim).unwrap();
        let duty = kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2 {
            model_panel_one: false,
            accepted_block,
            claim_id: claim,
            class_id: native.class_id,
            artifact_root: native.artifact_root,
            seat_bond: chain.bonds[1],
            executor_bond: native.executor_bond,
            execution_root: native.execution_root,
            trace_root: native.trace_root,
            output_root: native.output_root,
            bound_daa: chain.daa(),
            receipt_deadline: chain.daa() + 100,
            panel_anchor: Default::default(),
            seat_index: 0,
            panel_seat_count: 5,
            pwu: 1,
            quanta: 1,
            free_prompt: true,
            work_leaves: chain.view(claim).work_leaves,
            job_identity: native.job_identity,
            tir_shard: None,
        };
        let backend = public_fp_backend(&chain);
        if capacity {
            kaspa_consensus_core::palw_native_resource_v1::arm_native_logical_limits_v1(Some(
                kaspa_consensus_core::palw_native_resource_v1::NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 },
            ));
            assert!(
                matches!(
                    accepted_fp_job_context_v2(&session, &duty, &commitment.job, &backend),
                    Err(FpReplayRoutingFaultV2::Resource(_))
                ),
                "actual accepted original decoder admission preserves typed capacity"
            );
            drop(session);
            drop(manager);
            drop(chain);
            return;
        }
        let context = accepted_fp_job_context_v2(&session, &duty, &commitment.job, &backend).unwrap();
        assert_eq!(*context, backend.fp_job_context_for_executed_v1(&commitment.job, commitment.decode_tokens_executed).unwrap());
        let mut changed = duty.clone();
        changed.job_identity = Hash64::from_u64_word(0xBAD);
        assert!(matches!(
            accepted_fp_job_context_v2(&session, &changed, &commitment.job, &backend),
            Err(FpReplayRoutingFaultV2::Retry(_))
        ));
        changed = duty.clone();
        changed.output_root = Hash64::from_u64_word(0xBAD);
        assert!(matches!(
            accepted_fp_job_context_v2(&session, &changed, &commitment.job, &backend),
            Err(FpReplayRoutingFaultV2::Retry(_))
        ));
        changed = duty.clone();
        changed.accepted_block = Hash64::from_u64_word(0xBAD);
        assert!(matches!(
            accepted_fp_job_context_v2(&session, &changed, &commitment.job, &backend),
            Err(FpReplayRoutingFaultV2::Retry(_))
        ));
        let mut changed_job = commitment.job.clone();
        changed_job.job_nonce[0] ^= 1;
        assert!(matches!(accepted_fp_job_context_v2(&session, &duty, &changed_job, &backend), Err(FpReplayRoutingFaultV2::Retry(_))));
        drop(context);
        drop(session);
        drop(manager);
        drop(chain);
    })
    .await;
}
