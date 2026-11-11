use super::*;
use crate::model::stores::{
    ghostdag::GhostdagStoreReader,
    pruning::PruningStoreReader,
    virtual_state::{VirtualStateStore, VirtualStateStoreReader},
};
use kaspa_consensus_core::{
    Hash64,
    palw_freeprompt_v3::{
        PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION, PalwFpStopReasonV3, PalwFreePromptCommitmentV3,
        PalwFreePromptJobV3,
    },
    palw_mode_v2::PalwConsensusMode,
    palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    palw_rendering_resources_v1::PalwRenderingResourcePlanV1,
    palw_tir_public_allocation_v1::PalwTirDoorDiagnosticsV1,
    palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, PalwJobContextV2, prompt_token_ids_hash_v2, trace_scheme_id_v2},
};
use std::sync::{Arc, atomic::Ordering};

async fn quote_parent_fixture() -> TestContext {
    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle_funded_for(&catalog, 8);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
            p.palw_tir_public_conviction_v1 = None;
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    ctx
}

/// Bounded wire inputs reach the parent read. With the conviction fence
/// dormant the control is Ok(None), so these inputs need no registered model.
fn quote_request() -> (PalwFreePromptCommitmentV3, PalwPublicTirBindingHeaderV1) {
    let h = Hash64::from_u64_word;
    let prompt_hash = prompt_token_ids_hash_v2(&[1, 2]);
    let original = PalwFreePromptCommitmentV3 {
        job: PalwFreePromptJobV3 {
            version: PALW_FP_V3_VERSION,
            network_domain: h(1),
            class_id: h(2),
            executor_bond: TransactionOutpoint::new(h(3), 0),
            executor_pubkey: vec![4; 8],
            operator_id: h(5),
            anchor_block: h(6),
            anchor_daa: 7,
            job_nonce: [8; 32],
            tokenizer_id: h(9),
            prompt_token_ids_hash: prompt_hash,
            prompt_tokens: 2,
            decode_token_limit: 3,
            max_context_tokens: 16,
            privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: PALW_FP_PROMPT_MODE_USER,
            sampling_seed: [0; 32],
            temperature_q: 0,
            decode: None,
            tail: None,
        },
        trace_root: h(10),
        output_root: h(11),
        schedule_root: h(12),
        execution_root: h(13),
        decode_tokens_executed: 3,
        stop_reason: PalwFpStopReasonV3::EndOfGeneration,
        work_leaves: 1,
        trace_manifest_root: h(14),
        trace_chunk_count: 1,
        trace_retention_daa: 1_000,
    };
    let header = PalwPublicTirBindingHeaderV1 {
        version: 1,
        job_context: PalwJobContextV2 {
            version: PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: b"quote-parent-fixture".to_vec(),
            job_id: h(15),
            job_nullifier: h(16),
            assignment_id: h(17),
            execution_seed: [18; 32],
            model_profile_id: h(19),
            runtime_manifest_hash: h(20),
            runtime_class_id: h(21),
            shape_profile_id: original.job.class_id,
            trace_scheme_id: trace_scheme_id_v2(),
            cu_ruleset_id: h(22),
            tokenizer_id: original.job.tokenizer_id,
            prompt_token_ids_hash: prompt_hash,
            declared_prefill_tokens: 2,
            exact_decode_tokens: 3,
            max_context_tokens: 16,
        },
        artifact_root: h(23),
        full_logits_trace_root: original.trace_root,
        step_leaf_count: original.work_leaves,
        step_merkle_root: h(24),
        committed_execution_root: original.execution_root,
    };
    (original, header)
}

fn previous_diagnostics() -> Option<Box<PalwTirDoorDiagnosticsV1>> {
    let resources = PalwRenderingResourcePlanV1 {
        prefix_tokens: 0,
        vocab: 0,
        maximum_stream_bytes: 0,
        maximum_stream_leaves: 0,
        maximum_eog_bytes: 0,
        maximum_moves: 0,
        maximum_parser_steps: 0,
        maximum_blake_blocks: 0,
        inclusion_daa: 0,
        rent_sompi: 0,
        deposit_sompi: 0,
        row_bytes: 0,
        resident_ram_bytes: 0,
    };
    Some(Box::new(PalwTirDoorDiagnosticsV1 {
        accepted_daa: 1,
        preparation_daa: 0,
        open_window_daa: 0,
        owner_queue_wait_daa: 0,
        acquisition_daa: 0,
        semantic_daa: 0,
        semantic_families: Vec::new(),
        semantic_inventory: None,
        semantic_resources: None,
        batch_tokenizer_openings: None,
        filing_daa: 0,
        collection_daa: 0,
        total_daa: 0,
        limit_daa: 0,
        reorg_replay_moves: 0,
        reorg_reacquire_logical_units: 0,
        maximum_fresh_logical_units: 0,
        maximum_owner_turns: 0,
        maximum_native_signature_checks: 0,
        reorg_retention_daa: 0,
        acquisition_resources: resources,
        total_resources: resources,
        prepared_disk_bytes: 0,
        executor_disk_bytes: 0,
        maximum_terminal_witness_bytes: 0,
    }))
}

#[tokio::test]
async fn tir_original_quote_rejects_virtual_generation_change_during_cold_prepare() {
    let ctx = quote_parent_fixture().await;
    let vp = ctx.consensus.virtual_processor().clone();
    let captured = vp.virtual_stores.read().state.get().unwrap();
    let writer = vp.clone();
    *vp.template_parent_snapshot_v1.load_probe.lock() = Some(Box::new(move || {
        let mut stores = writer.virtual_stores.try_write().expect("quote releases the virtual reader before cold loading");
        let original = stores.state.get().unwrap();
        stores.state.set(Arc::new((*original).clone())).unwrap();
    }));

    let loads = vp.template_parent_snapshot_v1.loads.load(Ordering::Relaxed);
    let (original, header) = quote_request();
    let mut diagnostics = previous_diagnostics();
    let result = vp.palw_tir_public_original_quote_v1_impl(original.clone(), header.clone(), Some(&mut diagnostics));
    assert!(matches!(result, Err(ref why) if why.contains("selected view changed")), "{result:?}");
    assert!(diagnostics.is_none());
    assert_eq!(vp.template_parent_snapshot_v1.loads.load(Ordering::Relaxed), loads + 1);
    let current = vp.virtual_stores.read().state.get().unwrap();
    assert_eq!(captured.ghostdag_data.selected_parent, current.ghostdag_data.selected_parent);
    assert!(!Arc::ptr_eq(&captured, &current));
    assert!(matches!(vp.palw_tir_public_original_quote_v1_impl(original, header, Some(&mut diagnostics)), Ok(None)));
    assert!(diagnostics.is_none());
}

#[tokio::test]
async fn tir_original_quote_refuses_stale_lkg_reader_and_active_import() {
    let ctx = quote_parent_fixture().await;
    let vp = ctx.consensus.virtual_processor().clone();
    let params = vp.palw_state_params_v2.as_ref().unwrap();
    let (sink, sink_state) = vp.palw_state_v2_store.read().load_tip_cached(params).unwrap().unwrap();
    assert_eq!(sink, vp.lkg_virtual_state.load().ghostdag_data.selected_parent);
    let parent = vp.ghostdag_store.get_selected_parent(sink).unwrap();
    let (_, delta) = vp.palw_state_v2_store.read().delta_of(sink).unwrap();
    let parent_state = kaspa_consensus_core::palw_state_v2::revert_delta_v2(&sink_state, &delta, params).unwrap();
    let (original, header) = quote_request();
    assert!(matches!(vp.palw_tir_public_original_quote_v1_impl(original.clone(), header.clone(), None), Ok(None)));

    // The ordinary LKG reader can still serve its cached sink after the
    // history required to authenticate that sink has become unavailable.
    {
        let mut store = vp.palw_state_v2_store.write();
        store.set_tip_for_tests(parent, &parent_state).unwrap();
        store.delete_delta_for_tests(sink).unwrap();
    }
    *vp.palw_reader_snapshot.lock() = Some((sink, Arc::clone(&sink_state)));
    let (reader_sink, reader_state) = vp.palw_v2_reader_state(params).unwrap().unwrap();
    assert_eq!(reader_sink, sink);
    assert!(Arc::ptr_eq(&reader_state, &sink_state));
    let mut diagnostics = previous_diagnostics();
    let result = vp.palw_tir_public_original_quote_v1_impl(original.clone(), header.clone(), Some(&mut diagnostics));
    assert!(matches!(result, Err(ref why) if why.contains("parent is not authenticated")), "{result:?}");
    assert!(diagnostics.is_none());

    // Restore the exact materialized tip. A warm checked parent must still
    // be refused as soon as a pruning import becomes active.
    vp.palw_state_v2_store.write().set_tip_for_tests(sink, &sink_state).unwrap();
    assert!(matches!(vp.palw_tir_public_original_quote_v1_impl(original.clone(), header.clone(), None), Ok(None)));
    *vp.palw_reader_snapshot.lock() = Some((sink, sink_state));
    let pruning = vp.pruning_point_store.read().pruning_point().unwrap();
    vp.begin_pruning_point_import(pruning).unwrap();
    assert!(vp.pruning_import_status().unwrap().0.active.is_some());
    assert!(vp.palw_v2_reader_state(params).unwrap().is_some(), "the LKG path could still return its memo");
    let loads = vp.template_parent_snapshot_v1.loads.load(Ordering::Relaxed);
    let mut diagnostics = previous_diagnostics();
    let result = vp.palw_tir_public_original_quote_v1_impl(original, header, Some(&mut diagnostics));
    assert!(matches!(result, Err(ref why) if why.contains("parent is not authenticated")), "{result:?}");
    assert!(diagnostics.is_none());
    assert_eq!(vp.template_parent_snapshot_v1.loads.load(Ordering::Relaxed), loads);
}
