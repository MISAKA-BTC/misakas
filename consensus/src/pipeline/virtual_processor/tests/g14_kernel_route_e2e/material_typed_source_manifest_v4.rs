//! Durable input retention for a later fresh-DB pruning-import experiment.
//! This records source readiness, never successful IBD or a signed attestation.
use super::*;
use crate::model::stores::pruning::PruningStoreReader;
use serde::{Deserialize, Serialize};
use std::{fs::OpenOptions, io::Write, path::Path};

const FILES: [&str; 4] = ["reader-input", "terminal-input", "signed-admission", "signed-replacement"];
const DOMAIN: &[u8] = b"MISAKA_RFC12_SOURCE_FILE_V1";

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct FileIdentity {
    name: String,
    bytes: u64,
    // Keyed BLAKE2b-512, explicitly not a SHA256 binary fingerprint.
    blake2b512: String,
}
#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Manifest {
    version: u32,
    factory: String,
    required_fixture_env: Vec<(String, String)>,
    genesis: String,
    consensus_params_id: String,
    tip: String,
    tip_daa: u64,
    tip_palw_root: String,
    simulated_time: u64,
    nonce: u64,
    pruning_point: String,
    pruning_snapshot_root: String,
    material_root: String,
    tail: String,
    release: String,
    files: Vec<FileIdentity>,
    binary_binding: String,
    acceptance_scope: String,
}

pub(super) fn retain(w: &World, db: &Path, dir: &Path) {
    for name in ["MISAKA_TYPED_PRUNING_CHILD_V4", "MISAKA_TYPED_REPLACEMENT_CHILD_V4", "MISAKA_TYPED_TERMINAL_CHILD_V4"] {
        assert_eq!(std::env::var(name).as_deref(), Ok("1"), "retained source uses the exact long private factory");
    }
    let reader: ReaderInput = borsh::from_slice(&std::fs::read(dir.join("reader-input")).unwrap()).unwrap();
    // Existing TerminalInput Borsh field order: tail, release_block, finalized.
    let (tail, release, finalized): (Hash64, Hash64, bool) =
        borsh::from_slice(&std::fs::read(dir.join("terminal-input")).unwrap()).unwrap();
    let native = w.net.chain.ctx.consensus.as_ref();
    let (tip, state) = w.net.chain.tip_state();
    assert_eq!(reader.expected_tip, tip, "last cold reader input must describe the retained source tip");
    assert_eq!(native.get_sink(), tip);
    assert_eq!(reader.simulated_time, w.net.chain.ctx.simulated_time);
    assert_eq!(reader.nonce, w.net.chain.nonce_for_reopen());
    assert!(finalized, "retain only after final cold/GC role succeeds");
    let vp = w.net.chain.vp();
    let point = native.pruning_point();
    assert_ne!(point, w.net.config.params.genesis.hash);
    let (snapshot_point, snapshot_root) =
        vp.palw_state_v2_store.read().pruning_snapshot_key().unwrap().expect("exact snapshot identity");
    assert_eq!(snapshot_point, point);
    assert_eq!(vp.pruning_meta_stores.read().utxoset_position().unwrap(), point);
    {
        let p = vp.pruning_point_store.read();
        assert_eq!(p.retention_checkpoint().unwrap(), p.retention_period_root().unwrap());
    }
    let served = native.pruning_point_palw_state_shared_v1(point).expect("current pruning point must remain publicly servable");
    assert_eq!(served.last_point.as_ref().map(|p| p.block), Some(point));
    assert!(!vp.block_transactions_store.has(release).unwrap());
    assert!(!vp.palw_state_v2_store.read().has_delta(release).unwrap());
    let manifest = Manifest {
        version: 1,
        factory: "material_typed_ingress_v4::config + pruning::private_funding_config".into(),
        required_fixture_env: ["MISAKA_TYPED_PRUNING_CHILD_V4", "MISAKA_TYPED_REPLACEMENT_CHILD_V4", "MISAKA_TYPED_TERMINAL_CHILD_V4"]
            .into_iter()
            .map(|key| (key.into(), "1".into()))
            .collect(),
        genesis: w.net.config.params.genesis.hash.to_string(),
        consensus_params_id: w.net.config.params.consensus_params_id().to_string(),
        tip: tip.to_string(),
        tip_daa: native.get_header(tip).unwrap().daa_score,
        tip_palw_root: state.state_root().to_string(),
        simulated_time: reader.simulated_time,
        nonce: reader.nonce,
        pruning_point: point.to_string(),
        pruning_snapshot_root: snapshot_root.to_string(),
        material_root: reader.request.intent.manifest.root.to_string(),
        tail: tail.to_string(),
        release: release.to_string(),
        files: vec![],
        binary_binding:
            "Use the external parent pinned-binary SHA256/source record; this manifest is not a signature or binary fingerprint".into(),
        acceptance_scope:
            "source readiness only; fresh DB proof/header/trusted/UTXO/overlay/PALW/EVM import and post-prune block acceptance NOT RUN"
                .into(),
    };
    publish(db, dir, manifest);
}

fn publish(db: &Path, source: &Path, mut manifest: Manifest) {
    let stage = db.join("source-fixture-input-v1.partial");
    let final_dir = db.join("source-fixture-input-v1");
    assert!(!final_dir.exists(), "never replace a previous retained input identity");
    std::fs::create_dir(&stage).unwrap();
    for name in FILES {
        let bytes = std::fs::read(source.join(name)).unwrap();
        write_new(&stage.join(name), &bytes);
        manifest.files.push(FileIdentity {
            name: name.into(),
            bytes: u64::try_from(bytes.len()).unwrap(),
            blake2b512: kaspa_hashes::blake2b_512_keyed(DOMAIN, &bytes).to_string(),
        });
    }
    let encoded = serde_json::to_vec_pretty(&manifest).unwrap();
    assert_eq!(serde_json::from_slice::<Manifest>(&encoded).unwrap(), manifest);
    write_new(&stage.join("manifest.json"), &encoded);
    std::fs::File::open(&stage).unwrap().sync_all().unwrap();
    std::fs::rename(&stage, &final_dir).unwrap();
    std::fs::File::open(db).unwrap().sync_all().unwrap();
    eprintln!(
        "TYPED_PRUNING_V4 PERSISTED_source_manifest={:?} scope=source-readiness-only binary_binding=external-parent-record",
        final_dir.join("manifest.json")
    );
}

fn write_new(path: &Path, bytes: &[u8]) {
    let mut file = OpenOptions::new().create_new(true).write(true).open(path).unwrap();
    file.write_all(bytes).unwrap();
    file.sync_all().unwrap();
}
