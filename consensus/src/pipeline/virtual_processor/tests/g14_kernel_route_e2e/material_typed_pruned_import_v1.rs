//! Fresh retained-source import through the existing native pruning APIs.
//! Private mechanism fixture; a completed source manifest is required.
//! Explicit opt-in: --ignored --exact <this-test>; missing source is FAILURE.
//! No P2P transport, crash, campaign, K3 or settlement-policy acceptance claim.
use super::*;
use kaspa_consensus_core::{
    blockstatus::BlockStatus,
    pruning::{PruningImportSessionV1, PruningProofMetadata},
    trusted::{TrustedGhostdagData, TrustedHeader},
};
use kaspa_database::prelude::ConnBuilder;
use kaspa_p2p_lib::convert::model::trusted::{TrustedDataEntry, TrustedDataPackage};
use std::{path::PathBuf, sync::Arc};

/// An explicit output directory retains target and scratch-source DBs on failure.
/// The authoritative retained source is never used for the post-import new block.
#[tokio::test]
#[ignore = "requires a completed non-genesis retained Run7 source and explicit fresh output directory"]
async fn retained_source_fresh_pruned_import_and_one_native_block_then_cold_reopen() {
    let original = required_path("MISAKA_RFC12_PRUNED_SOURCE_DB_V1").canonicalize().expect("existing retained source DB");
    let requested_run = required_path("MISAKA_RFC12_PRUNED_IMPORT_RUN_V1");
    let run = requested_run
        .parent()
        .expect("output parent")
        .canonicalize()
        .expect("existing output parent")
        .join(requested_run.file_name().expect("output directory name"));
    assert!(!run.exists(), "output must be a new directory, never an earlier target DB");
    assert!(!run.starts_with(&original), "experiment output must not be inside the retained source");
    std::fs::create_dir(&run).expect("new output directory with existing parent");
    let inputs = original.join("source-fixture-input-v1");
    let manifest: Manifest =
        serde_json::from_slice(&std::fs::read(inputs.join("manifest.json")).expect("completed source manifest required"))
            .expect("source manifest schema");
    validate_inputs(&inputs, &manifest);
    let reader: ReaderInput = borsh::from_slice(&std::fs::read(inputs.join("reader-input")).unwrap()).unwrap();
    let admission: SignedAdmission = borsh::from_slice(&std::fs::read(inputs.join("signed-admission")).unwrap()).unwrap();
    let replacement: SignedReplacement = borsh::from_slice(&std::fs::read(inputs.join("signed-replacement")).unwrap()).unwrap();
    assert_eq!(admission.request, reader.request.id());
    let material_root = reader.request.intent.manifest.root;
    assert_eq!(material_root.to_string(), manifest.material_root);
    let consents = [admission.consent, replacement.consent];
    assert_ne!(consents[0], consents[1], "initial and replacement generations are distinct");
    let (mut cfg, bundle, _, _) = config();
    assert_eq!(cfg.params.genesis.hash.to_string(), manifest.genesis);
    assert_eq!(cfg.params.consensus_params_id().to_string(), manifest.consensus_params_id);
    cfg.process_genesis = false;
    let tip: Hash64 = manifest.tip.parse().expect("recorded native tip");
    let point: Hash64 = manifest.pruning_point.parse().expect("recorded pruning point");
    let release: Hash64 = manifest.release.parse().expect("recorded Release block");
    assert_ne!(point, cfg.params.genesis.hash, "genesis transfer is not pruned IBD");
    assert_eq!(reader.expected_tip, tip);
    assert_eq!((reader.simulated_time, reader.nonce), (manifest.simulated_time, manifest.nonce));

    // ConnBuilder has no read-only open mode. The original must be CLOSED; its
    // exclusive RocksDB open can perform recovery. No Consensus/native writer is
    // constructed over it. Only the native RocksDB checkpoint is used for work.
    let source_db =
        ConnBuilder::default().with_files_limit(20).with_db_path(original.clone()).with_create_if_missing(false).build().unwrap();
    let scratch_path = run.join("source-checkpoint");
    rocksdb::checkpoint::Checkpoint::new(&**source_db).unwrap().create_checkpoint(&scratch_path).unwrap();
    drop(source_db);
    eprintln!(
        "PRUNED_IMPORT_V1 source={original:?} scratch={scratch_path:?} original_open_may_recover=true source_must_be_closed=true target_is_empty=true"
    );

    let scratch_db =
        ConnBuilder::default().with_files_limit(20).with_db_path(scratch_path.clone()).with_create_if_missing(false).build().unwrap();
    let (source_tx, _source_rx) = async_channel::unbounded();
    let source_node = TestConsensus::with_db(scratch_db.clone(), &cfg, source_tx);
    arm_recorded_clock(&source_node, reader.simulated_time);
    let mut source = t12_reopened_chain(source_node, &cfg, &bundle, reader.simulated_time, reader.nonce);
    check_source(&source, &manifest, point, tip, release);

    eprintln!("PRUNED_IMPORT_V1 phase=proof_export event=start pp={point} tip={tip}");
    let proof = source.ctx.consensus.get_pruning_point_proof();
    eprintln!("PRUNED_IMPORT_V1 phase=proof_export event=complete pp={point} tip={tip}");
    assert_eq!(proof.first().and_then(|level| level.last()).map(|h| h.hash), Some(point));
    // Normal IBD validates on a current consensus, then applies on empty staging.
    // A separate genesis-initialized validator preserves that distinction.
    let mut validator_cfg = cfg.clone();
    validator_cfg.process_genesis = true;
    let validator = TestConsensus::new(&validator_cfg);
    arm_recorded_clock(&validator, reader.simulated_time);
    eprintln!("PRUNED_IMPORT_V1 phase=proof_validate event=start pp={point} tip={tip}");
    validator
        .validate_pruning_proof(&proof, &PruningProofMetadata::new(source.ctx.consensus.get_header(tip).unwrap().blue_work))
        .unwrap();
    eprintln!("PRUNED_IMPORT_V1 phase=proof_validate event=complete pp={point} tip={tip}");
    drop(validator);

    let trust = source.ctx.consensus.get_pruning_point_anticone_and_trusted_data().unwrap();
    let entries = trust
        .anticone
        .iter()
        .map(|hash| {
            TrustedDataEntry::new(source.ctx.consensus.get_block(*hash).expect("retained pruning-point anticone body"), vec![], vec![])
        })
        .collect();
    let package = TrustedDataPackage::new(
        trust.daa_window_blocks.iter().map(|item| TrustedHeader::new(item.header.clone(), item.ghostdag.clone())).collect(),
        trust.ghostdag_blocks.iter().map(|item| TrustedGhostdagData::new(item.hash, item.ghostdag.clone())).collect(),
    );
    // Reuse shipped missing-parent/GHOSTDAG/topological conversion verbatim.
    let trusted = package.build_trusted_subdag(entries).unwrap();
    let history = source.ctx.consensus.pruning_point_headers();
    assert_eq!(history.last().map(|h| h.hash), Some(point));
    let target_path = run.join("fresh-target");
    let target_db =
        ConnBuilder::default().with_files_limit(20).with_db_path(target_path.clone()).with_create_if_missing(true).build().unwrap();
    let (target_tx, _target_rx) = async_channel::unbounded();
    let target_node = TestConsensus::with_db(target_db.clone(), &cfg, target_tx);
    arm_recorded_clock(&target_node, reader.simulated_time);
    let mut target = t12_reopened_chain(target_node, &cfg, &bundle, reader.simulated_time, reader.nonce);
    eprintln!("PRUNED_IMPORT_V1 phase=proof_apply event=start pp={point} tip={tip}");
    target.ctx.consensus.apply_pruning_proof((*proof).clone(), &trusted).unwrap();
    target.ctx.consensus.import_pruning_points(history).unwrap();
    eprintln!("PRUNED_IMPORT_V1 phase=proof_apply event=complete pp={point} tip={tip}");
    let trusted_count = trusted.len();
    eprintln!("PRUNED_IMPORT_V1 phase=trusted event=start count={trusted_count} pp={point} tip={tip}");
    for block in trusted {
        target.ctx.consensus.validate_and_insert_trusted_block(block).virtual_state_task.await.unwrap();
    }
    eprintln!("PRUNED_IMPORT_V1 phase=trusted event=complete count={trusted_count} pp={point} tip={tip}");
    target.ctx.consensus.clear_body_missing_anticone_set();
    headers_after_point(&source.ctx.consensus, &target.ctx.consensus, point, tip).await;
    target.ctx.consensus.validate_pruning_points(tip).unwrap();
    check_source(&source, &manifest, point, tip, release);

    import_snapshot(&source.ctx.consensus, &target.ctx.consensus, point, &cfg);
    // The target decides what is missing. Never replay full genesis history.
    let missing = target.ctx.consensus.get_missing_block_body_hashes(tip).unwrap();
    assert!(!missing.contains(&release), "a physically pruned Release body cannot be required");
    let body_count = missing.len();
    eprintln!("PRUNED_IMPORT_V1 phase=bodies event=start count={body_count} pp={point} tip={tip}");
    for (ordinal, hash) in missing.into_iter().enumerate() {
        let block = source.ctx.consensus.get_block(hash).expect("source retains requested post-point body");
        target.ctx.consensus.validate_and_insert_block(block).virtual_state_task.await.unwrap();
        let processed = ordinal + 1;
        if processed % 1000 == 0 {
            eprintln!("PRUNED_IMPORT_V1 phase=bodies event=progress processed={processed} count={body_count} pp={point} tip={tip}");
        }
    }
    eprintln!("PRUNED_IMPORT_V1 phase=bodies event=complete count={body_count} pp={point} tip={tip}");
    assert_same_tip(&source.ctx.consensus, &target.ctx.consensus, &bundle, tip, material_root, &consents);
    assert!(!target.vp().block_transactions_store.has(release).unwrap());
    assert!(!target.vp().palw_state_v2_store.read().has_delta(release).unwrap());

    // Only scratch source is advanced. The original retained artifact stays reusable.
    let block = source.heartbeat(cfg.params.target_time_per_block(), vec![]).await;
    let accepted = block.hash();
    let final_clock = source.ctx.simulated_time;
    assert!(final_clock >= block.header.timestamp);
    assert!(source.ctx.consensus.has_validation_time_for_test(), "heartbeat advances the existing source clock");
    target.ctx.simulated_time = final_clock;
    arm_recorded_clock(&target.ctx.consensus, final_clock);
    target.ctx.consensus.validate_and_insert_block(block).virtual_state_task.await.unwrap();
    assert_eq!(target.ctx.consensus.block_status(accepted), BlockStatus::StatusUTXOValid);
    assert_same_tip(&source.ctx.consensus, &target.ctx.consensus, &bundle, accepted, material_root, &consents);
    let expected_rows = virtual_rows(&target.ctx.consensus);
    let expected_root = target.tip_state().1.state_root();
    eprintln!("PRUNED_IMPORT_V1 phase=cold_reopen event=start accepted={accepted} pp={point}");
    drop(target);
    let weak = Arc::downgrade(&target_db);
    drop(target_db);
    assert_eq!(weak.strong_count(), 0, "cold reopen requires all target DB owners closed");
    let cold_db = ConnBuilder::default().with_files_limit(20).with_db_path(target_path).with_create_if_missing(false).build().unwrap();
    let (cold_tx, _cold_rx) = async_channel::unbounded();
    let cold = TestConsensus::with_db(cold_db, &cfg, cold_tx);
    arm_recorded_clock(&cold, final_clock);
    assert_eq!(cold.get_sink(), accepted);
    assert!(cold.is_pruning_utxoset_stable());
    assert_eq!(virtual_rows(&cold), expected_rows);
    let (_, state) = cold.virtual_processor().palw_state_v2_store.read().load_tip(&bundle.state).unwrap().unwrap();
    assert_eq!(state.state_root(), expected_root);
    assert_same_receipts(&source.ctx.consensus, &cold, material_root, &consents);
    eprintln!("PRUNED_IMPORT_V1 phase=cold_reopen event=complete accepted={accepted} pp={point}");
    eprintln!(
        "PRUNED_IMPORT_V1 native_store_import_and_one_block=PASS pp={point} accepted={accepted} run={run:?} wire_crash_campaign=NOT_RUN"
    );
}

/// Run7 opts into this existing instance-local clock in pruning::started.
/// Reopening a T12Chain restores ctx time but does not enable header validation's
/// clock. Match the actual saved source clock before proof/header validation;
/// never widen timestamp tolerance or stamp the incoming headers backwards.
fn arm_recorded_clock(native: &TestConsensus, now: u64) {
    assert!(now > 0, "a zero clock disables the existing opt-in");
    native.set_validation_time_for_test(now);
    assert!(native.has_validation_time_for_test());
}

fn required_path(name: &str) -> PathBuf {
    let path =
        PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("explicit {name} is required; source absence is not PASS")));
    assert!(path.is_absolute(), "{name} must be absolute");
    path
}

fn validate_inputs(dir: &Path, manifest: &Manifest) {
    assert_eq!(manifest.version, 1);
    assert_eq!(manifest.factory, "material_typed_ingress_v4::config + pruning::private_funding_config");
    let expected: Vec<_> = ["MISAKA_TYPED_PRUNING_CHILD_V4", "MISAKA_TYPED_REPLACEMENT_CHILD_V4", "MISAKA_TYPED_TERMINAL_CHILD_V4"]
        .into_iter()
        .map(|name| (name.to_owned(), "1".to_owned()))
        .collect();
    assert_eq!(manifest.required_fixture_env, expected);
    for (name, value) in &expected {
        assert_eq!(std::env::var(name).as_ref(), Ok(value), "caller must explicitly select recorded private factory flags");
    }
    assert_eq!(manifest.files.len(), FILES.len());
    for (record, expected_name) in manifest.files.iter().zip(FILES) {
        assert_eq!(record.name, expected_name, "fixed allowlist/order; no manifest-selected path traversal");
        let bytes = std::fs::read(dir.join(expected_name)).unwrap();
        assert_eq!(u64::try_from(bytes.len()).unwrap(), record.bytes);
        assert_eq!(kaspa_hashes::blake2b_512_keyed(DOMAIN, &bytes).to_string(), record.blake2b512);
    }
}

fn check_source(source: &T12Chain, manifest: &Manifest, point: Hash64, tip: Hash64, release: Hash64) {
    let native = source.ctx.consensus.as_ref();
    let vp = source.vp();
    assert_eq!(native.get_sink(), tip);
    assert_eq!(native.pruning_point(), point);
    assert!(native.is_pruning_utxoset_stable());
    assert_eq!(source.tip_state().1.state_root().to_string(), manifest.tip_palw_root);
    assert_eq!(native.get_header(tip).unwrap().daa_score, manifest.tip_daa);
    assert_eq!(vp.pruning_meta_stores.read().utxoset_position().unwrap(), point);
    {
        let pruning = vp.pruning_point_store.read();
        assert_eq!(pruning.retention_checkpoint().unwrap(), pruning.retention_period_root().unwrap());
    }
    let identity = vp.palw_state_v2_store.read().pruning_snapshot_key().unwrap().unwrap();
    assert_eq!(identity.0, point);
    assert_eq!(identity.1.to_string(), manifest.pruning_snapshot_root);
    assert!(native.pruning_point_palw_state_shared_v1(point).is_some(), "snapshot must actually be served");
    assert!(!vp.block_transactions_store.has(release).unwrap());
    assert!(!vp.palw_state_v2_store.read().has_delta(release).unwrap());
}

async fn headers_after_point(source: &TestConsensus, target: &TestConsensus, point: Hash64, tip: Hash64) {
    let page = usize::try_from(source.params().mergeset_size_limit_history().upper_bound()).unwrap().checked_add(1).unwrap().max(4096);
    let mut low = point;
    let mut processed = 0usize;
    eprintln!("PRUNED_IMPORT_V1 phase=headers event=start pp={point} tip={tip}");
    while low != tip {
        let (hashes, reached) = source.get_hashes_between(low, tip, page).unwrap();
        assert_ne!(reached, low, "real native header export must make progress");
        for hash in hashes {
            let header = source.get_header(hash).unwrap();
            target.validate_and_insert_block(Block::from_header_arc(header)).virtual_state_task.await.unwrap();
            processed = processed.checked_add(1).unwrap();
            if processed % 1000 == 0 {
                eprintln!("PRUNED_IMPORT_V1 phase=headers event=progress processed={processed} pp={point} tip={tip}");
            }
        }
        low = reached;
    }
    eprintln!("PRUNED_IMPORT_V1 phase=headers event=complete processed={processed} pp={point} tip={tip}");
}

fn import_snapshot(source: &TestConsensus, target: &TestConsensus, point: Hash64, cfg: &Config) {
    let tip = source.get_sink();
    eprintln!("PRUNED_IMPORT_V1 phase=snapshot_import event=start pp={point} tip={tip}");
    let snapshot = source.pruning_point_palw_state_shared_v1(point).expect("public exact-point snapshot");
    let declarations = source.palw_class_carriages_for_snapshot_v1(&snapshot, true).expect("complete snapshot declarations");
    source.palw_validate_snapshot_declarations_v1(&snapshot, &declarations, true).unwrap();
    let first = target.begin_pruning_point_import(point).unwrap();
    let session = target.begin_pruning_point_import(point).unwrap();
    assert!(session.epoch > first.epoch);
    let sample = source.get_pruning_point_utxos(point, None, 1, false).unwrap();
    assert!(!sample.is_empty(), "money transfer must exercise a real UTXO set");
    let mut stale_hash = kaspa_muhash::MuHash::new();
    assert!(target.append_imported_pruning_point_utxos_for_session(first, &sample, &mut stale_hash).is_err());
    assert!(
        target
            .append_imported_pruning_point_utxos_for_session(
                PruningImportSessionV1 { point: cfg.params.genesis.hash, epoch: session.epoch },
                &sample,
                &mut stale_hash
            )
            .is_err()
    );
    let context = target.pruning_import_session().unwrap();
    assert_eq!(context.active, Some(session));
    assert!(!context.utxos_started && !context.utxos_imported);
    assert!(target.complete_pruning_point_import_for_session(session).is_err(), "an incomplete generation cannot publish stable");
    assert!(!target.is_pruning_utxoset_stable());
    if let Some(overlay) = source.pruning_point_overlay_snapshot() {
        assert_eq!(overlay.pruning_point, point);
        target.import_pruning_point_overlay_snapshot_for_session(session, overlay.snapshot).unwrap();
    } else {
        assert!(cfg.params.dns_params.is_none(), "active overlay cannot disappear");
    }
    let copied = snapshot.try_clone_v1().expect("clone preserves source owner and admission error");
    let (carriage, ticket) = copied.into_parts_v1();
    target.import_pruning_point_palw_snapshot_for_session(session, carriage, declarations, true).unwrap();
    drop(ticket); // owned importer input has retired before its clone ticket.
    target.clear_pruning_utxo_set_for_session(session).unwrap();
    let mut hash = kaspa_muhash::MuHash::new();
    let mut from = None;
    let mut rows = 0usize;
    let mut pages = 0usize;
    eprintln!("PRUNED_IMPORT_V1 phase=utxos event=start pp={point} tip={tip}");
    loop {
        let chunk = source.get_pruning_point_utxos(point, from, 4096, from.is_some()).unwrap();
        if chunk.is_empty() {
            break;
        }
        from = chunk.last().map(|(out, _)| *out);
        rows = rows.checked_add(chunk.len()).unwrap();
        target.append_imported_pruning_point_utxos_for_session(session, &chunk, &mut hash).unwrap();
        pages = pages.checked_add(1).unwrap();
        eprintln!("PRUNED_IMPORT_V1 phase=utxos event=progress pages={pages} rows={rows} pp={point} tip={tip}");
    }
    assert!(rows > 0);
    assert_eq!(hash.clone().finalize(), source.get_header(point).unwrap().utxo_commitment);
    target.import_pruning_point_utxo_set_for_session(session, hash).unwrap();
    eprintln!("PRUNED_IMPORT_V1 phase=utxos event=complete pages={pages} rows={rows} pp={point} tip={tip}");
    if let Some((header, state)) = source.pruning_point_evm_state(point) {
        target.import_pruning_point_evm_state_for_session(session, header, state).unwrap();
    } else {
        assert!(!cfg.params.is_evm_active(source.get_header(point).unwrap().daa_score), "active EVM cannot disappear");
    }
    assert_eq!(source.pruning_point(), point, "never combine export points");
    target.complete_pruning_point_import_for_session(session).unwrap();
    assert!(target.is_pruning_utxoset_stable());
    let context = target.pruning_import_session().unwrap();
    assert!(context.active.is_none() && !context.utxos_started && !context.utxos_imported);
    eprintln!("PRUNED_IMPORT_V1 phase=snapshot_import event=complete pp={point} tip={tip}");
}

fn virtual_rows(native: &TestConsensus) -> Vec<(TransactionOutpoint, UtxoEntry)> {
    let mut out = vec![];
    let mut from = None;
    loop {
        let page = native.get_virtual_utxos(from, 4096, from.is_some());
        if page.is_empty() {
            break;
        }
        from = page.last().map(|(key, _)| *key);
        out.extend(page);
    }
    out.sort_unstable_by_key(|(key, _)| (key.transaction_id, key.index));
    out
}
fn assert_same_receipts(source: &TestConsensus, target: &TestConsensus, root: Hash64, consents: &[Hash64; 2]) {
    for consent in consents {
        let left =
            source.palw_material_terminal_receipt_finalized_v2(root, *consent).unwrap().expect("actual source terminal receipt");
        let right = target.palw_material_terminal_receipt_finalized_v2(root, *consent).unwrap().expect("imported terminal receipt");
        assert!(left.1.covers(root, *consent));
        assert_eq!(left, right, "initial and replacement receipt bindings survive native import");
    }
}
fn assert_same_tip(
    source: &TestConsensus,
    target: &TestConsensus,
    bundle: &PalwConsensusParamsV2,
    tip: Hash64,
    root: Hash64,
    consents: &[Hash64; 2],
) {
    assert_eq!(source.get_sink(), tip);
    assert_eq!(target.get_sink(), tip);
    let (_, left) = source.virtual_processor().palw_state_v2_store.read().load_tip(&bundle.state).unwrap().unwrap();
    let (_, right) = target.virtual_processor().palw_state_v2_store.read().load_tip(&bundle.state).unwrap().unwrap();
    assert_eq!(left.state_root(), right.state_root());
    assert_eq!(virtual_rows(source), virtual_rows(target));
    assert_same_receipts(source, target, root, consents);
}

#[path = "material_typed_pruned_wire_v1.rs"]
mod wire;
