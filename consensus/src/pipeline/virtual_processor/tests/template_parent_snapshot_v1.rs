use super::*;
use crate::model::stores::ghostdag::GhostdagStoreReader;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use std::sync::atomic::Ordering;

#[tokio::test]
async fn template_parent_snapshot_reuses_exact_root_across_tip_walks_without_blocking_writer() {
    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle_funded_for(&catalog, 8);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    let vp = ctx.consensus.virtual_processor().clone();
    let (sink, sink_state) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().unwrap();
    let parent = vp.ghostdag_store.get_selected_parent(sink).unwrap();
    let parent_state = {
        let store = vp.palw_state_v2_store.read();
        let (_, delta) = store.delta_of(sink).unwrap();
        kaspa_consensus_core::palw_state_v2::revert_delta_v2(&sink_state, &delta, &bundle.state).unwrap()
    };
    vp.palw_state_v2_store.write().set_tip_for_tests(parent, &parent_state).unwrap();
    let writer_vp = vp.clone();
    *vp.template_parent_snapshot_v1.load_probe.lock() = Some(Box::new(move || {
        let writer = std::thread::spawn(move || {
            assert!(
                writer_vp.palw_state_v2_store.try_write().is_some(),
                "template miss must release the global reader before decoding/replay"
            );
            assert!(
                writer_vp.virtual_stores.try_write().is_some(),
                "the full template entry must release its virtual reader during cold replay"
            );
        });
        writer.join().unwrap();
    }));
    let before = vp.template_parent_snapshot_v1.loads.load(Ordering::Relaxed);
    struct ReaderFactory {
        vp: std::sync::Arc<crate::pipeline::virtual_processor::VirtualStateProcessor>,
        root: kaspa_consensus_core::Hash64,
        calls: std::sync::atomic::AtomicU64,
    }
    impl kaspa_consensus_core::block::TemplateTransactionSelectorFactory for ReaderFactory {
        fn build_selector(&self, _: Option<u64>) -> Box<dyn TemplateTransactionSelector> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            assert!(self.vp.virtual_stores.try_write().is_none(), "probe runs inside the real coherent UTXO/selector guard");
            let params = self.vp.palw_state_params_v2.as_ref().unwrap();
            let before = self.vp.palw_state_v2_store.read().tip_bytes_decoded();
            let (point, state) = self.vp.palw_v2_reader_state(params).unwrap().unwrap();
            assert_eq!(point, self.vp.lkg_virtual_state.load().ghostdag_data.selected_parent);
            assert_eq!(state.state_root(), self.root);
            assert_eq!(
                self.vp.palw_state_v2_store.read().tip_bytes_decoded(),
                before,
                "the sink reader used by carrier checks must not cold-decode under the UTXO guard"
            );
            Box::new(OnetimeTxSelector::new(vec![]))
        }
    }
    let factory = ReaderFactory { vp: vp.clone(), root: sink_state.state_root(), calls: std::sync::atomic::AtomicU64::new(0) };
    let cold_template = ctx
        .consensus
        .build_block_template_with_selector_factory(ctx.miner_data.clone(), &factory, TemplateBuildMode::Standard)
        .unwrap();
    assert_eq!(factory.calls.load(Ordering::Relaxed), 1);
    assert_eq!(cold_template.block.header.palw_state_root, sink_state.state_root());
    drop(cold_template);
    let first = vp.palw_template_parent_checked_v1(sink).unwrap();
    assert_eq!(first.state_root(), sink_state.state_root(), "real deltas reconstruct the selected parent's root");
    let (reader_point, reader_state) = vp.palw_v2_reader_state(&bundle.state).unwrap().unwrap();
    assert_eq!(reader_point, sink);
    assert_eq!(reader_state.state_root(), first.state_root());
    assert!(std::sync::Arc::ptr_eq(&reader_state, &first), "cold carrier readers use the strict prewarmed sink owner");
    drop(reader_state);
    let second = vp.palw_template_parent_checked_v1(sink).unwrap();
    assert!(std::sync::Arc::ptr_eq(&first, &second));
    // The mutable anchor now moves back to the actual sink, just as a resolver
    // alternates candidates. An unchanged exact target/root needs no replay.
    vp.palw_state_v2_store.write().set_tip_for_tests(sink, &sink_state).unwrap();
    let third = vp.palw_template_parent_checked_v1(sink).unwrap();
    assert!(std::sync::Arc::ptr_eq(&first, &third));
    assert_eq!(vp.template_parent_snapshot_v1.loads.load(Ordering::Relaxed), before + 1);
    let template = ctx.build_block_template(7, ctx.simulated_time + 1);
    assert_eq!(template.block.header.palw_state_root, sink_state.state_root());
    drop(template);
    let (recorded_root, delta) = vp.palw_state_v2_store.read().delta_of(sink).unwrap();
    // Exercise a real target walk: an exact materialized tip is authoritative
    // at its own block and does not need that block's retained delta.
    vp.palw_state_v2_store.write().set_tip_for_tests(parent, &parent_state).unwrap();
    vp.palw_state_v2_store
        .write()
        .set_delta_record_for_tests(
            sink,
            crate::model::stores::palw_state_v2::PalwStateDeltaRecordV2 {
                state_root: kaspa_consensus_core::Hash64::from_u64_word(0xBAD),
                delta_borsh: borsh::to_vec(&delta).unwrap(),
            },
        )
        .unwrap();
    let failures_before = vp.template_parent_snapshot_v1.loads.load(Ordering::Relaxed);
    for _ in 0..2 {
        assert!(
            matches!(
                vp.palw_template_parent_checked_v1(sink),
                Err(crate::pipeline::virtual_processor::processor::PalwWeighFaultV2::StoreUnreadable)
            ),
            "a changed recorded root invalidates the memo and cannot serve the old state"
        );
    }
    assert_eq!(
        vp.template_parent_snapshot_v1.loads.load(Ordering::Relaxed),
        failures_before + 2,
        "a real root-mismatch refusal is not memoized"
    );
    vp.palw_state_v2_store
        .write()
        .set_delta_record_for_tests(
            sink,
            crate::model::stores::palw_state_v2::PalwStateDeltaRecordV2 {
                state_root: recorded_root,
                delta_borsh: borsh::to_vec(&delta).unwrap(),
            },
        )
        .unwrap();
    assert_eq!(vp.palw_template_parent_checked_v1(sink).unwrap().state_root(), sink_state.state_root());
    let weak = std::sync::Arc::downgrade(&first);
    drop((first, second, third));
    let replacement = vp.palw_template_parent_checked_v1(parent).unwrap();
    assert_eq!(replacement.state_root(), parent_state.state_root(), "the real revert leg is equivalent too");
    assert!(weak.upgrade().is_none(), "changing target releases the old retained owner after its consumers finish");
}

#[tokio::test]
async fn template_parent_snapshot_virtual_change_retries_before_selector_creation() {
    use crate::model::stores::virtual_state::{VirtualStateStore, VirtualStateStoreReader};
    use kaspa_consensus_core::block::TemplateTransactionSelectorFactory;
    use kaspa_consensus_core::errors::block::RuleError;
    struct ObservedFactory(std::sync::atomic::AtomicU64);
    impl TemplateTransactionSelectorFactory for ObservedFactory {
        fn build_selector(&self, _: Option<u64>) -> Box<dyn TemplateTransactionSelector> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Box::new(OnetimeTxSelector::new(vec![]))
        }
    }
    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle_funded_for(&catalog, 8);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    let vp = ctx.consensus.virtual_processor().clone();
    let writer_vp = vp.clone();
    *vp.template_parent_snapshot_v1.load_probe.lock() = Some(Box::new(move || {
        let mut writer = writer_vp.virtual_stores.try_write().expect("cold prepare must not retain the virtual read guard");
        let original = writer.state.get().unwrap();
        // A real publication of a different owned generation, with unchanged
        // consensus content: the builder must not use its older UTXO capture.
        writer.state.set(std::sync::Arc::new((*original).clone())).unwrap();
    }));
    let factory = ObservedFactory(std::sync::atomic::AtomicU64::new(0));
    let result =
        ctx.consensus.build_block_template_with_selector_factory(ctx.miner_data.clone(), &factory, TemplateBuildMode::Standard);
    assert!(matches!(result, Err(RuleError::LocalNativeStateContextUnavailable(_))));
    assert_eq!(factory.0.load(Ordering::Relaxed), 0, "a stale warmup cannot create a selector or evict its transactions");
    let retry = ctx
        .consensus
        .build_block_template_with_selector_factory(ctx.miner_data.clone(), &factory, TemplateBuildMode::Standard)
        .unwrap();
    assert_eq!(factory.0.load(Ordering::Relaxed), 1);
    assert_eq!(retry.block.header.palw_state_root, vp.palw_template_parent_checked_v1(ctx.consensus.get_sink()).unwrap().state_root());
}

#[tokio::test]
async fn template_parent_snapshot_sink_seed_refuses_wrong_point_root_and_import() {
    use crate::model::stores::{pruning::PruningStoreReader, virtual_state::VirtualStateStoreReader};
    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle_funded_for(&catalog, 8);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..2 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    let vp = ctx.consensus.virtual_processor().clone();
    let actual = vp.virtual_stores.read().state.get().unwrap();
    let sink = actual.ghostdag_data.selected_parent;
    let parent = vp.ghostdag_store.get_selected_parent(sink).unwrap();
    let mut wrong = (*actual).clone();
    wrong.ghostdag_data.selected_parent = parent;
    let wrong = std::sync::Arc::new(wrong);
    let wrong_prepared = vp.prepare_template_parent_v1(&wrong).unwrap();
    *vp.palw_reader_snapshot.lock() = None;
    assert!(vp.seed_template_sink_reader_v1(&actual, &wrong_prepared).is_err(), "never alias a parent state to the current sink");
    assert!(vp.palw_reader_snapshot.lock().is_none());
    let prepared = vp.prepare_template_parent_v1(&actual).unwrap();
    vp.seed_template_sink_reader_v1(&actual, &prepared).unwrap();
    let expected = vp.palw_template_parent_checked_v1(sink).unwrap().state_root();
    let (point, reader) = vp.palw_v2_reader_state(&bundle.state).unwrap().unwrap();
    assert_eq!((point, reader.state_root()), (sink, expected));
    let (root, delta) = vp.palw_state_v2_store.read().delta_of(sink).unwrap();
    let parent_state = kaspa_consensus_core::palw_state_v2::revert_delta_v2(&reader, &delta, &bundle.state).unwrap();
    drop(reader);
    vp.palw_state_v2_store.write().set_tip_for_tests(parent, &parent_state).unwrap();
    vp.palw_state_v2_store
        .write()
        .set_delta_record_for_tests(
            sink,
            crate::model::stores::palw_state_v2::PalwStateDeltaRecordV2 {
                state_root: kaspa_consensus_core::Hash64::from_u64_word(0xBAD),
                delta_borsh: borsh::to_vec(&delta).unwrap(),
            },
        )
        .unwrap();
    *vp.palw_reader_snapshot.lock() = None;
    assert!(vp.seed_template_sink_reader_v1(&actual, &prepared).is_err(), "recorded root changed since warmup");
    assert!(vp.palw_reader_snapshot.lock().is_none());
    vp.palw_state_v2_store
        .write()
        .set_delta_record_for_tests(
            sink,
            crate::model::stores::palw_state_v2::PalwStateDeltaRecordV2 {
                state_root: root,
                delta_borsh: borsh::to_vec(&delta).unwrap(),
            },
        )
        .unwrap();
    let pruning_point = vp.pruning_point_store.read().pruning_point().unwrap();
    vp.begin_pruning_point_import(pruning_point).unwrap();
    assert!(vp.seed_template_sink_reader_v1(&actual, &prepared).is_err(), "an import/epoch transition invalidates seeding");
    assert!(vp.palw_reader_snapshot.lock().is_none());
}

#[tokio::test]
async fn template_parent_snapshot_legacy_pruning_fallback_is_an_optimization_skip() {
    use crate::model::stores::{pruning::PruningStoreReader, virtual_state::VirtualStateStoreReader};
    use kaspa_consensus_core::errors::block::RuleError;
    for armed_future in [false, true] {
        let catalog = palw_v2_test_catalog();
        let bundle = palw_v2_test_bundle_funded_for(&catalog, 8);
        let mut config = ConfigBuilder::new(MAINNET_PARAMS)
            .skip_proof_of_work()
            .edit_consensus_params(|p| {
                p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
                *p = p.clone().with_palw_v2_cadence();
            })
            .build();
        if armed_future {
            // Explicit private fixture, not a public activation: startup's
            // refusal to ship selected-source bootstrap remains unchanged.
            use kaspa_consensus_core::palw_source_bootstrap_v1::{
                PalwSelectedSourceBootstrapFenceV1, PalwSelectedSourceBootstrapPolicyV1,
            };
            let h = kaspa_consensus_core::Hash64::from_u64_word;
            let policy = PalwSelectedSourceBootstrapPolicyV1 {
                version: 1,
                chain_genesis: config.params.genesis.hash,
                network: h(11),
                ruleset: h(12),
                epoch: 1,
                freeze_cutoff_daa: 100,
                anchor_cutoff_daa: 101,
                starts_at_daa: 102,
                ends_at_daa: 103,
                stake_release_daa: 104,
                selection: misaka_palw_challenge::bootstrap_epoch_v1::BootstrapPolicyV1 {
                    version: 1,
                    seats: 1,
                    quorum: 1,
                    maximum_population: 4,
                    maximum_draw_attempts: 4,
                    maximum_source_works: 1,
                },
                source_profile: h(13),
                checker_scope: h(14),
                maximum_claim_bytes: 1024,
                maximum_check_work: 1024,
                maximum_history_headers: 16,
                security_review: h(15),
                economic_review: h(16),
            };
            policy.validate().unwrap();
            config.params.palw_selected_source_bootstrap_v1 = Some(PalwSelectedSourceBootstrapFenceV1 {
                at: kaspa_consensus_core::config::params::ForkActivation::new(100),
                policy,
            });
            config.params.sync_palw_selected_source_bootstrap_v1();
        }
        let mut ctx = TestContext::new(TestConsensus::new(&config));
        for _ in 0..3 {
            ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
        }
        let vp = ctx.consensus.virtual_processor().clone();
        let actual = vp.virtual_stores.read().state.get().unwrap();
        let (sink, sink_state) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().unwrap();
        let parent = vp.ghostdag_store.get_selected_parent(sink).unwrap();
        let (original_root, original_delta) = vp.palw_state_v2_store.read().delta_of(sink).unwrap();
        let parent_state = {
            let store = vp.palw_state_v2_store.read();
            let (_, delta) = store.delta_of(sink).unwrap();
            kaspa_consensus_core::palw_state_v2::revert_delta_v2(&sink_state, &delta, &bundle.state).unwrap()
        };
        {
            let mut store = vp.palw_state_v2_store.write();
            store.set_pruning_snapshot_for_tests(sink, &sink_state).unwrap();
            store.set_tip_for_tests(parent, &parent_state).unwrap();
            store.delete_delta_for_tests(sink).unwrap();
        }
        assert!(vp.palw_candidate_state_v2_checked(sink).is_err(), "the strict old candidate walk really lacks history");
        let old = vp.old_template_parent_for_tests_v1(sink).unwrap().1;
        assert_eq!(old.state_root(), sink_state.state_root(), "the ORIGINAL template fallback succeeds");
        let prepared = vp.prepare_template_parent_v1(&actual).unwrap();
        *vp.palw_reader_snapshot.lock() = None;
        vp.seed_template_sink_reader_v1(&actual, &prepared).unwrap();
        assert!(vp.palw_reader_snapshot.lock().is_none(), "a fallback is never promoted to the strict reader memo");
        let template = ctx
            .consensus
            .build_block_template(ctx.miner_data.clone(), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard)
            .unwrap();
        assert_eq!(template.block.header.palw_state_root, old.state_root(), "historical/unarmed template success is preserved");
        // A sidecar-only pruning import has not completed the UTXO import.
        // That disables memo/seeding, but was never an unarmed template gate.
        let pruning_point = vp.pruning_point_store.read().pruning_point().unwrap();
        vp.begin_pruning_point_import(pruning_point).unwrap();
        let during_import = vp.prepare_template_parent_v1(&actual).unwrap();
        *vp.palw_reader_snapshot.lock() = None;
        vp.seed_template_sink_reader_v1(&actual, &during_import).unwrap();
        assert!(vp.palw_reader_snapshot.lock().is_none(), "an incomplete import cannot seed the strict memo");
        let during_import_template = ctx
            .consensus
            .build_block_template(ctx.miner_data.clone(), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard)
            .unwrap();
        assert_eq!(during_import_template.block.header.palw_state_root, old.state_root());
        if armed_future {
            vp.palw_state_v2_store
                .write()
                .set_delta_record_for_tests(
                    sink,
                    crate::model::stores::palw_state_v2::PalwStateDeltaRecordV2 {
                        state_root: original_root,
                        delta_borsh: borsh::to_vec(&original_delta).unwrap(),
                    },
                )
                .unwrap();
            assert!(vp.palw_candidate_state_v2_checked(sink).is_ok(), "the strict history is now complete");
            let mut future = (*actual).clone();
            future.daa_score = 101;
            let result = vp.build_block_template_from_virtual_state(
                std::sync::Arc::new(future),
                Default::default(),
                Default::default(),
                ctx.miner_data.clone(),
                vec![],
                vec![],
                Default::default(),
                vec![],
            );
            assert!(
                matches!(
                    result,
                    Err(RuleError::LocalNativeStateContextUnavailable(
                        kaspa_consensus_core::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::HistoryUnavailable
                    ))
                ),
                "once active, strict source preflight STILL refuses the incomplete import, even with complete history"
            );
        }
    }
}

#[tokio::test]
async fn template_parent_snapshot_exact_carriage_tip_has_precedence_over_older_delta() {
    use kaspa_consensus_core::Hash64;
    use kaspa_consensus_core::palw_execution_lane_v1::PalwExecFinalV1;
    use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
    let (config, bundle) = adr0125_config_funded_for(8);
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    let vp = ctx.consensus.virtual_processor().clone();
    let sink = ctx.consensus.get_sink();
    let (tip, state) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().unwrap();
    assert_eq!(tip, sink);
    let recorded_root = vp.palw_state_v2_store.read().state_root_of(sink).unwrap();
    let bond = state.bond(&adr0125_harness_bond()).unwrap();
    let schedule = adr0125_schedule_of(
        0,
        &[PalwExecFinalV1 {
            domain: bundle.base_class_id,
            bond: adr0125_harness_bond(),
            operator_id: bond.operator_id,
            claim_id: Hash64::from_u64_word(0xC1A1),
            execution_root: Hash64::from_u64_word(0xE0),
            credit: 1,
            accepted_blue_score: 0,
        }],
    );
    let mut carriage = PalwStateCarriageV2::from_state(&state);
    carriage.round_schedules.insert(0, schedule);
    let installed = carriage.into_state(&bundle.state, None).unwrap();
    assert_ne!(installed.state_root(), recorded_root);
    vp.palw_state_v2_store.write().set_tip_for_tests(sink, &installed).unwrap();
    let old = vp.old_template_parent_for_tests_v1(sink).unwrap().1;
    assert_eq!(old.state_root(), installed.state_root(), "the original exact-tip reader accepts this carriage");
    let checked = vp.palw_template_parent_checked_v1(sink).unwrap();
    assert_eq!(checked.state_root(), old.state_root());
    let template = ctx
        .consensus
        .build_block_template(ctx.miner_data.clone(), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard)
        .unwrap();
    assert_eq!(template.block.header.palw_state_root, old.state_root());
}

#[tokio::test]
async fn template_parent_snapshot_missing_compact_root_keeps_prebuilt_legacy_result() {
    use crate::model::stores::virtual_state::VirtualStateStoreReader;
    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle_funded_for(&catalog, 8);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..2 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    let vp = ctx.consensus.virtual_processor().clone();
    let captured = vp.virtual_stores.read().state.get().unwrap();
    let target = captured.ghostdag_data.selected_parent;
    let expected = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().unwrap().1.state_root();
    ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    vp.palw_state_v2_store.write().delete_delta_for_tests(target).unwrap();
    assert!(vp.palw_state_v2_store.read().state_root_of(target).is_err());
    // Reverting the NEXT block does not need the target's own delta. Both
    // old and new checked derivations can still authenticate this exact state.
    let old = vp.old_template_parent_for_tests_v1(target).unwrap().1;
    assert_eq!(old.state_root(), expected);
    let before = vp.template_parent_snapshot_v1.loads.load(Ordering::Relaxed);
    let prepared = vp.prepare_template_parent_v1(&captured).unwrap();
    vp.palw_template_parent_checked_v1(target).unwrap();
    assert_eq!(
        vp.template_parent_snapshot_v1.loads.load(Ordering::Relaxed),
        before + 2,
        "missing compact identity disables memo only"
    );
    *vp.palw_reader_snapshot.lock() = None;
    vp.seed_template_sink_reader_v1(&captured, &prepared).unwrap();
    assert!(vp.palw_reader_snapshot.lock().is_none());
    let template = vp
        .build_block_template_from_virtual_state(
            captured,
            Default::default(),
            Default::default(),
            ctx.miner_data.clone(),
            vec![],
            vec![],
            Default::default(),
            vec![],
        )
        .unwrap();
    assert_eq!(template.block.header.palw_state_root, old.state_root());
}

#[tokio::test]
async fn template_parent_snapshot_non_palw_entry_keeps_baseline() {
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    assert!(ctx.consensus.virtual_processor().palw_state_params_v2.is_none());
    let old_sink = ctx.consensus.get_sink();
    ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    assert_ne!(ctx.consensus.get_sink(), old_sink);
}

#[tokio::test]
async fn producer_context_requires_exact_parent_state_clock_and_stable_import() {
    use crate::model::stores::{pruning::PruningStoreReader, virtual_state::VirtualStateStoreReader};
    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle_funded_for(&catalog, 8);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    let vp = ctx.consensus.virtual_processor().clone();
    let api = ctx.consensus.consensus_clone();
    let actual = vp.virtual_stores.read().state.get().unwrap();
    let sink = actual.ghostdag_data.selected_parent;
    let (point, state) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().unwrap();
    assert_eq!(point, sink);
    let root = vp.palw_committed_root_v1(&state);
    let epoch = api.palw_producer_context_v1(sink, actual.daa_score, root).unwrap();
    assert_eq!(api.palw_producer_context_v1(sink, actual.daa_score, root).unwrap(), epoch);
    let parent = vp.ghostdag_store.get_selected_parent(sink).unwrap();
    assert!(api.palw_producer_context_v1(parent, actual.daa_score, root).is_err());
    assert!(api.palw_producer_context_v1(sink, actual.daa_score + 1, root).is_err());
    assert!(api.palw_producer_context_v1(sink, actual.daa_score, kaspa_consensus_core::Hash64::from_u64_word(0xBAD)).is_err());
    let (_, delta) = vp.palw_state_v2_store.read().delta_of(sink).unwrap();
    let parent_state = kaspa_consensus_core::palw_state_v2::revert_delta_v2(&state, &delta, &bundle.state).unwrap();
    {
        let mut store = vp.palw_state_v2_store.write();
        store.set_tip_for_tests(parent, &parent_state).unwrap();
        store.delete_delta_for_tests(sink).unwrap();
    }
    assert!(api.palw_producer_context_v1(sink, actual.daa_score, root).is_err(), "missing own state cannot authorize inference");
    vp.palw_state_v2_store.write().set_tip_for_tests(sink, &state).unwrap();
    assert_eq!(api.palw_producer_context_v1(sink, actual.daa_score, root).unwrap(), epoch, "restored checked context resumes");
    let pp = vp.pruning_point_store.read().pruning_point().unwrap();
    vp.begin_pruning_point_import(pp).unwrap();
    assert!(
        api.palw_producer_context_v1(sink, actual.daa_score, root).is_err(),
        "an active import holds even an otherwise current template"
    );
}
