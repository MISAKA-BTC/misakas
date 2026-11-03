//! Actual native/store guard controls. No pruning-depth override or accepted
//! claim/Final facts are supplied by these snapshot reader fixtures.
#![allow(warnings)]
use super::*;
use crate::{model::stores::block_transactions::BlockTransactionsStoreReader, pipeline::virtual_processor::PalwPruningCaptureFaultV1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_database::{
    prelude::{CachePolicy, DbKey},
    registry::DatabaseStorePrefixes,
};

fn context() -> TestContext {
    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle_funded_for(&catalog, 8);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    TestContext::new(TestConsensus::new(&config))
}
/// Real configured genesis classes have canonical builtin declarations. The
/// corruption-only fixture above deliberately uses synthetic class IDs and
/// cannot be a complete class-material serving fixture.
fn rfc37_context() -> TestContext {
    let authentic = kaspa_consensus_core::config::params::palw_t12_launch_params_v1();
    assert!(matches!(authentic.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)));
    let config = ConfigBuilder::new(authentic)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    TestContext::new(TestConsensus::new(&config))
}

async fn rfc37_build_points(ctx: &mut TestContext) -> Vec<kaspa_hashes::Hash64> {
    let mut points = Vec::new();
    for nonce in 0..3 {
        // The actual builder commits its timestamp into the active EVM root.
        // Preserve that timestamp and the live lane while advancing real blocks.
        let template = ctx.build_block_template_keeping_time(nonce);
        let (template, _) = ctx.consensus.heartbeat_adapt_block_template(template).unwrap();
        let block = template.block.to_immutable();
        let hash = block.hash();
        // Advance the existing instance-local fixture clock to the real slot
        // chosen by the adapter; never alter a committed header or drift rule.
        ctx.simulated_time = ctx.simulated_time.max(block.header.timestamp);
        ctx.consensus.set_validation_time_for_test(ctx.simulated_time);
        ctx.validate_and_insert_block(block).await;
        assert_eq!(ctx.consensus.get_sink(), hash, "the unchanged active-lane template must become the real sink");
        points.push(hash);
    }
    points
}

struct RestoreRow {
    db: std::sync::Arc<kaspa_database::prelude::DB>,
    key: DbKey,
    old: Vec<u8>,
}
impl RestoreRow {
    fn corrupt(db: std::sync::Arc<kaspa_database::prelude::DB>, key: DbKey) -> Self {
        let old = db.get(&key).unwrap().unwrap();
        db.put(&key, [0xff]).unwrap();
        Self { db, key, old }
    }
}
impl Drop for RestoreRow {
    fn drop(&mut self) {
        self.db.put(&self.key, &self.old).unwrap();
    }
}

#[tokio::test]
async fn pruning_snapshot_readiness_fault_preserves_body_delta_and_same_point_repairs() {
    let mut ctx = context();
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    let vp = ctx.consensus.virtual_processor();
    let point = ctx.consensus.get_sink();
    let before_body = vp.block_transactions_store.get(point).unwrap();
    assert!(vp.palw_state_v2_store.read().has_delta(point).unwrap());
    let tip_key = DbKey::prefix_only(&Vec::<u8>::from(DatabaseStorePrefixes::PalwStateV2Tip));
    let fault = RestoreRow::corrupt(vp.db.clone(), tip_key);
    let entered = std::cell::Cell::new(0);
    let refusal = vp.with_pruning_snapshots_before_delete_v1(point, || {
        entered.set(1);
    });
    assert!(matches!(refusal, Err(PalwPruningCaptureFaultV1::Store(_))));
    assert_eq!(entered.get(), 0);
    assert!(vp.palw_state_v2_store.read().has_delta(point).unwrap());
    assert_eq!(vp.block_transactions_store.get(point).unwrap().len(), before_body.len());
    drop(fault);
    vp.with_pruning_snapshots_before_delete_v1(point, || {
        entered.set(entered.get() + 1);
        let mut batch = rocksdb::WriteBatch::default();
        let mut palw = vp.palw_state_v2_store.write();
        palw.delete_delta_batch(&mut batch, point).unwrap();
        vp.block_transactions_store.delete_batch(&mut batch, point).unwrap();
        vp.db.write(batch).unwrap();
    })
    .unwrap();
    assert_eq!(entered.get(), 1, "same-point retry enters deletion exactly once after repair");
    assert!(!vp.palw_state_v2_store.read().has_delta(point).unwrap());
    assert!(vp.block_transactions_store.get(point).is_err());
    assert_eq!(ctx.consensus.pruning_point_palw_state_shared_v1(point).unwrap().last_point.as_ref().unwrap().block, point);
}

#[tokio::test]
async fn pruning_snapshot_readiness_reuses_cold_exact_snapshot_after_partial_prune() {
    let mut ctx = context();
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    let vp = ctx.consensus.virtual_processor();
    let point = ctx.consensus.get_sink();
    vp.capture_pruning_point_palw_state(point).unwrap();
    let record = vp.palw_state_v2_store.read().pruning_snapshot_record().unwrap().unwrap();
    let mut batch = rocksdb::WriteBatch::default();
    vp.palw_state_v2_store.write().delete_delta_batch(&mut batch, point).unwrap();
    vp.db.write(batch).unwrap();
    let old_store = vp.palw_state_v2_store.read().clone_with_new_cache(CachePolicy::Empty);
    *vp.palw_state_v2_store.write() = old_store;
    // Recovery must use the validated exact snapshot even if the old tip source
    // cannot now be decoded. The restored tip fault is unrelated to that snapshot.
    let tip_key = DbKey::prefix_only(&Vec::<u8>::from(DatabaseStorePrefixes::PalwStateV2Tip));
    let fault = RestoreRow::corrupt(vp.db.clone(), tip_key);
    vp.with_pruning_snapshots_before_delete_v1(point, || ()).unwrap();
    let served = ctx.consensus.pruning_point_palw_state_shared_v1(point).unwrap();
    assert_eq!(served.last_point.as_ref().unwrap().block, point);
    assert_eq!(vp.palw_state_v2_store.read().pruning_snapshot_record().unwrap().unwrap().state_root, record.state_root);
    assert!(!vp.palw_state_v2_store.read().has_delta(point).unwrap());
    drop(fault);
}

#[tokio::test]
async fn pruning_snapshot_readiness_valid_genesis_and_inactive_lane_keep_contract() {
    let ctx = context();
    let genesis = ctx.consensus.params().genesis.hash;
    let vp = ctx.consensus.virtual_processor();
    vp.capture_pruning_point_palw_state(genesis).unwrap();
    let served = ctx.consensus.pruning_point_palw_state_shared_v1(genesis).unwrap();
    assert!(served.last_point.as_ref().is_none_or(|p| p.block == genesis));
    let config = ConfigBuilder::new(MAINNET_PARAMS).build();
    let inactive = TestContext::new(TestConsensus::new(&config));
    inactive.consensus.virtual_processor().capture_pruning_point_palw_state(inactive.consensus.get_sink()).unwrap();
}

#[tokio::test]
async fn pruning_snapshot_readiness_missing_walk_history_never_enters_delete() {
    let mut ctx = context();
    let mut points = Vec::new();
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
        points.push(ctx.consensus.get_sink());
    }
    let vp = ctx.consensus.virtual_processor();
    let point = points[0];
    // A genuine missing rollback delta, rather than an empty substitute. Only
    // the newest delta is removed; the protected point/body must remain intact.
    let mut batch = rocksdb::WriteBatch::default();
    vp.palw_state_v2_store.write().delete_delta_batch(&mut batch, points[2]).unwrap();
    vp.db.write(batch).unwrap();
    let entered = std::cell::Cell::new(false);
    let refusal = vp.with_pruning_snapshots_before_delete_v1(point, || entered.set(true));
    assert!(matches!(refusal, Err(PalwPruningCaptureFaultV1::Walk(
        crate::processes::palw_state_walk::PalwStateWalkError::MissingDelta(hash)
    )) if hash == points[2]));
    assert!(!entered.get());
    assert!(vp.palw_state_v2_store.read().has_delta(point).unwrap());
    assert!(vp.block_transactions_store.get(point).is_ok());
}

#[tokio::test]
async fn pruning_snapshot_readiness_missing_active_tip_is_context_not_inactive_success() {
    let mut ctx = context();
    ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    let vp = ctx.consensus.virtual_processor();
    let point = ctx.consensus.get_sink();
    let key = DbKey::prefix_only(&Vec::<u8>::from(DatabaseStorePrefixes::PalwStateV2Tip));
    let old = vp.db.get(&key).unwrap().unwrap();
    let restore = RestoreRow { db: vp.db.clone(), key, old };
    vp.db.delete(&restore.key).unwrap();
    let entered = std::cell::Cell::new(false);
    assert!(matches!(
        vp.with_pruning_snapshots_before_delete_v1(point, || entered.set(true)),
        Err(PalwPruningCaptureFaultV1::Context("active PALW tip is absent"))
    ));
    assert!(!entered.get());
    assert!(vp.palw_state_v2_store.read().has_delta(point).unwrap());
    assert!(vp.block_transactions_store.get(point).is_ok());
    drop(restore);
}

fn row_suffix_v1(
    snapshot: &kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2,
    declarations: &[(kaspa_hashes::Hash64, Vec<u8>)],
) -> Vec<u8> {
    let state = borsh::to_vec(snapshot).unwrap();
    let mut suffix = 1u16.to_le_bytes().to_vec();
    suffix.extend_from_slice(&(state.len() as u64).to_le_bytes());
    suffix.extend_from_slice(&state);
    suffix.extend_from_slice(&borsh::to_vec(declarations).unwrap());
    suffix
}

#[tokio::test]
async fn rfc37_store_bridge_pins_source_and_suffix_before_any_output() {
    let mut ctx = rfc37_context();
    let points = rfc37_build_points(&mut ctx).await;
    let point = points[1];
    let vp = ctx.consensus.virtual_processor();
    vp.capture_pruning_point_palw_state(point).unwrap();
    let snapshot = ctx.consensus.pruning_point_palw_state_shared_v1(point).unwrap();
    let declarations = ctx.consensus.palw_class_carriages_for_snapshot_v1(&snapshot, true).unwrap();
    let suffix = row_suffix_v1(&snapshot, &declarations);
    let before = vp.palw_state_v2_store.read().tip_record().unwrap().unwrap();
    // Force the bridge to reconstruct from the captured DB record, not an Arc
    // cached by the serving API immediately before it.
    let cold = vp.palw_state_v2_store.read().clone_with_new_cache(CachePolicy::Empty);
    *vp.palw_state_v2_store.write() = cold;
    let mut output = Vec::new();
    assert_eq!(
        ctx.consensus
            .write_pruning_point_palw_row_generation_v1(
                points[0],
                &mut std::io::Cursor::new(&suffix),
                suffix.len() as u64,
                &mut output
            )
            .unwrap(),
        None
    );
    assert!(output.is_empty(), "a different captured point never starts a generation");
    let mut changed = suffix.clone();
    changed[10] ^= 1;
    assert!(
        ctx.consensus
            .write_pruning_point_palw_row_generation_v1(point, &mut std::io::Cursor::new(&changed), changed.len() as u64, &mut output)
            .is_err()
    );
    assert!(output.is_empty(), "a mixed source/native suffix is refused before output");
    let length = ctx
        .consensus
        .write_pruning_point_palw_row_generation_v1(point, &mut std::io::Cursor::new(&suffix), suffix.len() as u64, &mut output)
        .unwrap()
        .unwrap();
    assert_eq!(length, output.len() as u64);
    assert_eq!(
        vp.palw_state_v2_store.read().tip_record().unwrap().unwrap().state_root,
        before.state_root,
        "export cannot modify the live tip"
    );
}

#[tokio::test]
async fn rfc37_actual_snapshot_bridge_inventory_validation_and_native_import_agree() {
    use borsh::BorshDeserialize;
    use kaspa_consensus_core::{
        palw_native_resource_v1::NativeDecodeErrorV1, palw_row_stream_v1::*, palw_state_v2::PalwStateCarriageV2,
    };
    use std::io::Read;
    let mut ctx = rfc37_context();
    let points = rfc37_build_points(&mut ctx).await;
    let point = points[1];
    let vp = ctx.consensus.virtual_processor();
    vp.capture_pruning_point_palw_state(point).unwrap();
    let snapshot = ctx.consensus.pruning_point_palw_state_shared_v1(point).unwrap();
    let declarations = ctx.consensus.palw_class_carriages_for_snapshot_v1(&snapshot, true).unwrap();
    let suffix = row_suffix_v1(&snapshot, &declarations);
    assert!(
        ctx.consensus.pruning_point_palw_row_witness_root_v1(points[2]).is_none(),
        "a point without a selected-chain child has no row witness"
    );
    let root =
        ctx.consensus.pruning_point_palw_row_witness_root_v1(point).expect("the exact selected-chain child witnesses this flat state");
    assert_eq!(root, vp.palw_state_v2_store.read().pruning_snapshot_record().unwrap().unwrap().state_root);
    let mut output = Vec::new();
    let length = ctx
        .consensus
        .write_pruning_point_palw_row_generation_v1(point, &mut std::io::Cursor::new(&suffix), suffix.len() as u64, &mut output)
        .unwrap()
        .unwrap();
    let parsed = palw_verify_row_generation_reader_v1(root, &mut std::io::Cursor::new(&output), length).unwrap();
    kaspa_consensus_core::palw_native_resource_v1::preflight_native_snapshot_v1(&output[parsed.offset as usize..], parsed.length)
        .unwrap();
    let mut cursor = std::io::Cursor::new(&output[parsed.offset as usize..]);
    assert_eq!(u16::deserialize_reader(&mut cursor).unwrap(), 1);
    let n = u64::deserialize_reader(&mut cursor).unwrap();
    let restored = PalwStateCarriageV2::deserialize_reader(&mut (&mut cursor).take(n)).unwrap();
    let restored_declarations = Vec::<(kaspa_hashes::Hash64, Vec<u8>)>::deserialize_reader(&mut cursor).unwrap();
    assert_eq!(restored, **snapshot);
    assert_eq!(restored_declarations, declarations);
    ctx.consensus.palw_validate_row_stream_snapshot_v1(point, &restored, &parsed.inventory).unwrap();
    let preimage_len = u32::from_le_bytes(output[22..26].try_into().unwrap()) as usize;
    let omitted = encode_row_stream_generation_v1(&output[26..26 + preimage_len], &[], &suffix).unwrap();
    let omitted = palw_verify_row_generation_reader_v1(root, &mut std::io::Cursor::new(&omitted), omitted.len() as u64).unwrap();
    assert!(
        matches!(
            ctx.consensus.palw_validate_row_stream_snapshot_v1(point, &restored, &omitted.inventory),
            Err(NativeDecodeErrorV1::Decode(_))
        ),
        "a correct native suffix cannot hide omitted stream collections"
    );
    // Use the existing history authority before the ordinary atomic import;
    // row validation on its own did not authorize replacing the live tip.
    ctx.consensus
        .import_pruning_points(vec![
            ctx.consensus.get_header(ctx.consensus.params().genesis.hash).unwrap(),
            ctx.consensus.get_header(point).unwrap(),
        ])
        .unwrap();
    vp.palw_state_v2_store.write().delete_tip_for_tests().unwrap();
    let stale = ctx.consensus.begin_pruning_point_import(point).unwrap();
    let session = ctx.consensus.begin_pruning_point_import(point).unwrap();
    assert!(
        ctx.consensus
            .import_pruning_point_palw_snapshot_for_session(stale, restored.clone(), restored_declarations.clone(), true)
            .is_err(),
        "row authentication cannot authorize a stale import session"
    );
    assert!(vp.palw_state_v2_store.read().tip_record().unwrap().is_none(), "refused session cannot publish a tip");
    let before_classes: Vec<_> =
        restored_declarations.iter().map(|(id, _)| (*id, vp.palw_class_carriage_store.read().get(*id))).collect();
    let mut tampered = restored.clone();
    tampered.pending_payouts.insert(
        kaspa_hashes::Hash64::from_u64_word(0x37_ffff),
        kaspa_consensus_core::palw_state_v2::PalwPayoutV2 { payload: kaspa_hashes::Hash64::from_u64_word(0x37_aaaa), amount: 1 },
    );
    assert!(
        ctx.consensus.import_pruning_point_palw_snapshot_for_session(session, tampered, restored_declarations.clone(), true).is_err(),
        "a correct authenticated row inventory cannot authorize a changed native suffix"
    );
    assert!(vp.palw_state_v2_store.read().tip_record().unwrap().is_none(), "failed whole-state authentication cannot install a tip");
    for (id, before) in before_classes {
        assert_eq!(
            vp.palw_class_carriage_store.read().get(id),
            before,
            "failed whole-state authentication cannot publish declarations"
        );
    }
    ctx.consensus.import_pruning_point_palw_snapshot_for_session(session, restored.clone(), restored_declarations, true).unwrap();
    assert_eq!(vp.palw_state_v2_store.read().tip_record().unwrap().unwrap().state_root, root);
    // Throw away both decoded caches. The imported native rows and declarations
    // must be sufficient for the ordinary persisted-state recovery path.
    let cold = vp.palw_state_v2_store.read().clone_with_new_cache(CachePolicy::Empty);
    *vp.palw_state_v2_store.write() = cold;
    let (loaded_point, loaded) =
        vp.palw_state_v2_store.read().load_tip_retained_v1(vp.palw_state_params_v2.as_ref().unwrap()).unwrap().unwrap();
    assert_eq!(loaded_point, point);
    assert_eq!(loaded.state_root(), root);
    assert_eq!(PalwStateCarriageV2::from_state(&loaded), restored);
    ctx.consensus.palw_validate_row_stream_snapshot_v1(point, &restored, &parsed.inventory).unwrap();
}

#[tokio::test]
async fn detached_pruning_capture_allows_real_store_writer_and_serves_exact_historical_root() {
    let mut ctx = context();
    let mut points = Vec::new();
    for _ in 0..4 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
        points.push(ctx.consensus.get_sink());
    }
    let vp = ctx.consensus.virtual_processor();
    let point = points[0];
    let expected = {
        let store = vp.palw_state_v2_store.read().clone();
        let (tip, state) = store.load_tip_retained_v1(vp.palw_state_params_v2.as_ref().unwrap()).unwrap().unwrap();
        let path = vp.dag_traversal_manager.calculate_chain_path(tip, point, None);
        let state = crate::processes::palw_state_walk::walk_chain_path_retained_v1(
            &store,
            vp.palw_state_params_v2.as_ref().unwrap(),
            state,
            &path.removed.to_vec(),
            &path.added.to_vec(),
        )
        .unwrap();
        kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2::from_state(&state)
    };
    let checks = std::cell::Cell::new(0usize);
    let deleted = std::cell::Cell::new(false);
    vp.with_pruning_snapshots_before_delete_while_v1(
        point,
        || {
            // The actual production capture pauses here between real delta replays.
            // A different OS thread must acquire the same writer virtual commits use.
            let writer_entered =
                std::thread::scope(|scope| scope.spawn(|| vp.palw_state_v2_store.try_write().is_some()).join().unwrap());
            assert!(writer_entered, "snapshot replay must not retain the global PALW read/write guard");
            checks.set(checks.get() + 1);
            true
        },
        || deleted.set(true),
    )
    .unwrap();
    assert!(checks.get() >= 8, "the fixture traversed real historical deltas");
    assert!(deleted.get());
    let served = ctx.consensus.pruning_point_palw_state_shared_v1(point).unwrap();
    assert_eq!(&**served, &expected, "detaching changes lock ownership, never the historical serving bytes/root");
    assert_eq!(served.last_point.as_ref().unwrap().block, point);
}

#[tokio::test]
async fn pruning_capture_cancellation_between_deltas_never_publishes_or_deletes() {
    let mut ctx = context();
    let mut points = Vec::new();
    for _ in 0..4 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
        points.push(ctx.consensus.get_sink());
    }
    let vp = ctx.consensus.virtual_processor();
    let calls = std::cell::Cell::new(0usize);
    let deleted = std::cell::Cell::new(false);
    let refusal = vp.with_pruning_snapshots_before_delete_while_v1(
        points[0],
        || {
            calls.set(calls.get() + 1);
            calls.get() < 5
        },
        || deleted.set(true),
    );
    assert!(matches!(refusal, Err(PalwPruningCaptureFaultV1::Context("pruning capture interrupted"))));
    assert!(!deleted.get());
    assert!(ctx.consensus.pruning_point_palw_state_shared_v1(points[0]).is_none());
    for point in points {
        assert!(vp.palw_state_v2_store.read().has_delta(point).unwrap());
        assert!(vp.block_transactions_store.get(point).is_ok());
    }
}

#[tokio::test]
async fn pruning_capture_same_point_new_import_epoch_refuses_stale_publication() {
    let mut ctx = context();
    let mut points = Vec::new();
    for _ in 0..4 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
        points.push(ctx.consensus.get_sink());
    }
    let vp = ctx.consensus.virtual_processor();
    let genesis = ctx.consensus.params().genesis.hash;
    let calls = std::cell::Cell::new(0usize);
    let deleted = std::cell::Cell::new(false);
    let refusal = vp.with_pruning_snapshots_before_delete_while_v1(
        points[0],
        || {
            calls.set(calls.get() + 1);
            if calls.get() == 4 {
                // Real durable session begin at the SAME pruning point, not a mocked epoch.
                vp.begin_pruning_point_import(genesis).unwrap();
            }
            true
        },
        || deleted.set(true),
    );
    assert!(matches!(refusal, Err(PalwPruningCaptureFaultV1::Context("pruning/import generation changed during capture"))));
    assert!(!deleted.get());
    assert!(ctx.consensus.pruning_point_palw_state_shared_v1(points[0]).is_none());
    assert!(vp.palw_state_v2_store.read().has_delta(points[0]).unwrap());
    assert!(vp.block_transactions_store.get(points[0]).is_ok());
}

#[tokio::test]
async fn pruning_capture_uses_shorter_verified_checkpoint_instead_of_far_live_tip() {
    let mut ctx = context();
    let mut points = Vec::new();
    for _ in 0..6 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
        points.push(ctx.consensus.get_sink());
    }
    let vp = ctx.consensus.virtual_processor();
    let params = vp.palw_state_params_v2.as_ref().unwrap();
    let target = points[1];
    let (expected, expected_root) = {
        let store = vp.palw_state_v2_store.read().clone();
        let (tip, state) = store.load_tip_retained_v1(params).unwrap().unwrap();
        let path = vp.dag_traversal_manager.calculate_chain_path(tip, target, None);
        let state =
            crate::processes::palw_state_walk::walk_chain_path_retained_v1(&store, params, state, &path.removed, &path.added).unwrap();
        (kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2::from_state(&state), state.state_root())
    };
    // Install a real verified earlier checkpoint using the production capture.
    vp.capture_pruning_point_palw_state(points[0]).unwrap();
    // The tip's long rollback path now lacks a delta unrelated to the short
    // retained checkpoint -> target path. Success proves that path is not used.
    let mut batch = rocksdb::WriteBatch::default();
    vp.palw_state_v2_store.write().delete_delta_batch(&mut batch, points[5]).unwrap();
    vp.db.write(batch).unwrap();
    let entered = std::cell::Cell::new(false);
    vp.with_pruning_snapshots_before_delete_v1(target, || entered.set(true)).unwrap();
    assert!(entered.get());
    let served = ctx.consensus.pruning_point_palw_state_shared_v1(target).unwrap();
    assert_eq!(&**served, &expected);
    assert_eq!(vp.palw_state_v2_store.read().pruning_snapshot_record().unwrap().unwrap().state_root, expected_root);
    assert_eq!(served.last_point.as_ref().unwrap().block, target);
    assert!(vp.block_transactions_store.get(target).is_ok());
}
