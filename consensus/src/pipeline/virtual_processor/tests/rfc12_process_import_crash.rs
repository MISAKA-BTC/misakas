//! OS-kill import publication controls. These use authentic harness premine
//! UTXOs at genesis; they do not establish useful-work/P2P/pruning/market or
//! hardware power-loss acceptance. Execute only in the consolidated test phase.
#![allow(warnings)]
use super::*;
use kaspa_database::prelude::ConnBuilder;
use kaspa_muhash::MuHash;

#[derive(borsh::BorshSerialize, borsh::BorshDeserialize)]
struct Manifest {
    point: BlockHash,
    epoch: u64,
    rows: Vec<(TransactionOutpoint, UtxoEntry)>,
    palw_root: Hash64,
}
fn read_manifest(root: &std::path::Path) -> Manifest {
    borsh::from_slice(&std::fs::read(root.join("manifest.borsh")).unwrap()).unwrap()
}
fn assert_stable(consensus: &TestConsensus, manifest: &Manifest) {
    assert!(consensus.is_pruning_utxoset_stable());
    let session = consensus.pruning_import_session().unwrap();
    assert!(session.active.is_none() && !session.utxos_started && !session.utxos_imported);
    assert!(session.epoch >= manifest.epoch);
    assert_eq!(
        sorted_process_utxos(consensus),
        manifest.rows,
        "completed generation retains every actual premine outpoint exactly once"
    );
    let vp = consensus.virtual_processor();
    let tip = vp.palw_state_v2_store.read().tip_record().unwrap().unwrap();
    assert_eq!((tip.block, tip.state_root), (manifest.point, manifest.palw_root));
    assert_eq!(vp.pruning_meta_stores.read().utxoset_position().unwrap(), manifest.point);
    if let Some(heads) = consensus.get_evm_canonical_heads().unwrap() {
        assert_eq!(heads.latest_head(), Some(manifest.point));
    }
}

#[tokio::test]
async fn rfc12_process_utxo_import_crash_worker() {
    let Ok(root) = std::env::var("MISAKA_RFC12_IMPORT_WORKER_DIRECTORY") else { return };
    let phase = std::env::var("MISAKA_RFC12_IMPORT_WORKER_PHASE").unwrap();
    let stage = std::env::var("MISAKA_RFC12_IMPORT_WORKER_STAGE").unwrap();
    let root = std::path::PathBuf::from(root);
    let db = ConnBuilder::default()
        .with_files_limit(10)
        .with_db_path(root.join("db"))
        .with_create_if_missing(phase == "seed")
        .build()
        .unwrap();
    let mut p = parts(None);
    if phase != "seed" {
        p.0.process_genesis = false;
    }
    let (sender, _rx) = async_channel::unbounded();
    let consensus = TestConsensus::with_db(db, &p.0, sender);
    if phase == "seed" {
        let rig = Rig::on(consensus, &p, 0x12_ca57_0000);
        let point = rig.config.params.genesis.hash;
        let rows = rig
            .vp()
            .pruning_meta_stores
            .read()
            .utxo_set
            .iterator()
            .map(|row| {
                let (out, entry) = row.unwrap();
                (out, (*entry).clone())
            })
            .collect::<Vec<_>>();
        assert!(!rows.is_empty(), "a crash test over an empty money ledger proves too little");
        let expected = sorted_process_utxos(rig.api());
        assert_eq!(rows.len(), expected.len());
        let root_before = rig.vp().palw_state_v2_store.read().tip_record().unwrap().unwrap().state_root;
        let token = rig.api().begin_pruning_point_import(point).unwrap();
        let manifest = Manifest { point, epoch: token.epoch, rows: expected, palw_root: root_before };
        use std::io::Write;
        let mut file = std::fs::File::create(root.join("manifest.borsh")).unwrap();
        file.write_all(&borsh::to_vec(&manifest).unwrap()).unwrap();
        file.sync_all().unwrap();
        let mut armed = std::fs::File::create(root.join("armed")).unwrap();
        armed.write_all(stage.as_bytes()).unwrap();
        armed.sync_all().unwrap();
        rig.api().clear_pruning_utxo_set_for_session(token).unwrap();
        let mut hash = MuHash::new();
        rig.api().append_imported_pruning_point_utxos_for_session(token, &rows, &mut hash).unwrap();
        rig.api().import_pruning_point_utxo_set_for_session(token, hash).unwrap();
        rig.api().complete_pruning_point_import_for_session(token).unwrap();
        panic!("the requested OS-kill stage was not reached: {stage}");
    }
    let manifest = read_manifest(&root);
    if phase == "verify" {
        assert_stable(&consensus, &manifest);
        return;
    }
    assert_eq!(phase, "recover");
    if stage == "import-after-complete" {
        assert_stable(&consensus, &manifest);
        return;
    }
    assert!(!consensus.is_pruning_utxoset_stable(), "a killed incomplete generation cannot become stable on cold load");
    assert!(consensus.is_consensus_in_transitional_ibd_state());
    assert!(consensus.get_evm_canonical_heads().is_err(), "partial UTXO/sidecar state cannot publish ETH tags");
    assert!(consensus.get_native_settlement_snapshot().is_err(), "partial state cannot publish a native finality generation");
    let stale = kaspa_consensus_core::pruning::PruningImportSessionV1 { point: manifest.point, epoch: manifest.epoch };
    assert_eq!(consensus.pruning_import_session().unwrap().active, Some(stale));
    let retry = consensus.begin_pruning_point_import(manifest.point).unwrap();
    assert!(retry.epoch > stale.epoch);
    let refused = |r: kaspa_consensus_core::errors::pruning::PruningImportResult<()>| {
        assert!(
            matches!(r, Err(kaspa_consensus_core::errors::pruning::PruningImportError::PruningImportSessionRefused { .. })),
            "old-process token rejected before every write: {r:?}"
        );
    };
    refused(consensus.clear_pruning_utxo_set_for_session(stale));
    refused(consensus.append_imported_pruning_point_utxos_for_session(stale, &manifest.rows, &mut MuHash::new()));
    refused(consensus.import_pruning_point_utxo_set_for_session(stale, MuHash::new()));
    refused(consensus.complete_pruning_point_import_for_session(stale));
    consensus.clear_pruning_utxo_set_for_session(retry).unwrap();
    let mut hash = MuHash::new();
    consensus.append_imported_pruning_point_utxos_for_session(retry, &manifest.rows, &mut hash).unwrap();
    consensus.import_pruning_point_utxo_set_for_session(retry, hash).unwrap();
    consensus.complete_pruning_point_import_for_session(retry).unwrap();
    assert_stable(&consensus, &manifest);
}

#[test]
fn rfc12_process_kill_matrix_preserves_import_epoch_money_and_publication() {
    let suffix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let root = std::env::temp_dir().join(format!("misaka-rfc12-import-kill-{}-{suffix}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let worker = format!("{}::rfc12_process_utxo_import_crash_worker", module_path!().split_once("::").unwrap().1);
    for stage in [
        "utxo-before-clear",
        "utxo-after-clear",
        "utxo-before-virtual-copy",
        "utxo-after-virtual-clear",
        "utxo-mid-virtual-copy",
        "utxo-after-virtual-copy",
        "import-before-complete",
        "import-after-complete",
    ] {
        let directory = root.join(stage);
        std::fs::create_dir(&directory).unwrap();
        let command = |phase: &str| {
            let mut c = std::process::Command::new(std::env::current_exe().unwrap());
            c.args(["--exact", &worker, "--nocapture", "--test-threads=1"])
                .env("MISAKA_RFC12_IMPORT_WORKER_DIRECTORY", &directory)
                .env("MISAKA_RFC12_IMPORT_WORKER_PHASE", phase)
                .env("MISAKA_RFC12_IMPORT_WORKER_STAGE", stage);
            if phase == "seed" {
                c.env("MISAKA_RFC12_IMPORT_CRASH_DIRECTORY", &directory).env("MISAKA_RFC12_IMPORT_CRASH_STAGE", stage);
            } else {
                c.env_remove("MISAKA_RFC12_IMPORT_CRASH_DIRECTORY").env_remove("MISAKA_RFC12_IMPORT_CRASH_STAGE");
            }
            c
        };
        let mut child = command("seed").spawn().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
        while !directory.join("ready").exists() {
            if let Some(status) = child.try_wait().unwrap() {
                panic!("{stage} seed exited before its actual boundary: {status}");
            }
            if std::time::Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("{stage} rendezvous timeout");
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(std::fs::read_to_string(directory.join("ready")).unwrap(), stage);
        child.kill().unwrap();
        assert!(!child.wait().unwrap().success());
        assert!(command("recover").status().unwrap().success(), "{stage} cold recovery verified its persisted authority");
        assert!(command("verify").status().unwrap().success(), "{stage} a second cold process verified the recovered durable money");
        std::fs::remove_dir_all(directory).unwrap();
    }
    std::fs::remove_dir(root).unwrap();
}
