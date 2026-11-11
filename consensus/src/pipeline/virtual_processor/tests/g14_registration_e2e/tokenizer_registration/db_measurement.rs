//! Opt-in storage measurement over two actual node databases. The control
//! replays the shared prerequisites, then produces the same clock steps and
//! block count with empty bodies while the workload folds TKR1.
//! This whitespace-padded payload is deliberately compressible. Directory
//! scans are sequential observations while RocksDB background work may run.
//! The measurement exercises retention and GC, not a disk capacity bound.
use super::*;
use kaspa_database::{
    prelude::{DB, DbKey},
    registry::DatabaseStorePrefixes,
};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

const PART_SIZE: usize = TOKENIZER_REGISTRATION_PART_BYTES_V1;

#[test]
#[ignore = "opt-in RocksDB measurement; requires TKR_DB_MEASUREMENT_DIR"]
fn measure_tokenizer_registration_rocksdb_history_and_gc() {
    let root = PathBuf::from(std::env::var_os("TKR_DB_MEASUREMENT_DIR").expect("set a new measurement directory"));
    std::fs::create_dir(&root).expect("measurement requires a new directory");
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(run(root)))
        .unwrap()
        .join()
        .unwrap();
}

fn open_db(path: &Path, create: bool) -> Arc<DB> {
    ConnBuilder::default().with_files_limit(10).with_db_path(path.to_owned()).with_create_if_missing(create).build().unwrap()
}

#[derive(Default)]
struct Files {
    total: u64,
    wal: u64,
    sst: u64,
    allocated: u64,
    files: u64,
    disappeared: u64,
}
impl Files {
    fn visit(&mut self, dir: &Path) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    self.disappeared += 1;
                    continue;
                }
                Err(error) => panic!("{}: {error}", path.display()),
            };
            if metadata.is_dir() {
                self.visit(&path);
            } else if metadata.is_file() {
                let bytes = metadata.len();
                self.total += bytes;
                self.files += 1;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    self.allocated += metadata.blocks() * 512;
                }
                match path.extension().and_then(|ext| ext.to_str()) {
                    Some("log") => self.wal += bytes,
                    Some("sst") => self.sst += bytes,
                    _ => {}
                }
            }
        }
    }
}

fn footprint(db: &DB) -> Value {
    let mut files = Files::default();
    files.visit(db.path());
    let property = |name| db.property_int_value(name).unwrap();
    json!({
        "fileBytes": files.total, "walFileBytes": files.wal, "sstFileBytes": files.sst,
        "allocatedFileBytes": if cfg!(unix) { Some(files.allocated) } else { None },
        "files": files.files, "filesDisappearedDuringSample": files.disappeared,
        "rocksdbLiveDataEstimateBytes": property("rocksdb.estimate-live-data-size"),
        "rocksdbLiveSstBytes": property("rocksdb.live-sst-files-size"),
        "rocksdbTotalSstBytes": property("rocksdb.total-sst-files-size"),
        "rocksdbAllMemtablesBytes": property("rocksdb.size-all-mem-tables"),
    })
}

fn sample(stage: &str, workload: (&T12Chain, &DB), control: (&T12Chain, &DB), phase: Option<Value>) {
    let describe = |(chain, db): (&T12Chain, &DB)| {
        let (tip, state) = chain.tip_state();
        json!({"tip": tip.to_string(), "daa": chain.daa_of(tip), "stateRoot": state.state_root().to_string(), "db": footprint(db)})
    };
    println!(
        "{}",
        json!({"stage": stage, "scope": "isolated selected-source node storage", "workload": describe(workload),
        "control": describe(control), "phase": phase})
    );
}

fn flush_memtable(db: &DB) -> u128 {
    let start = Instant::now();
    db.flush_wal(true).unwrap();
    let mut options = rocksdb::FlushOptions::default();
    options.set_wait(true);
    db.flush_opt(&options).unwrap();
    start.elapsed().as_nanos()
}

async fn beat_pair(workload: &mut T12Chain, control: &mut T12Chain, interval: u64, txs: Vec<Transaction>) -> (Block, u128, u128) {
    let miner = MinerData::new(card_payout_spk(OPENER), Vec::new());
    let start = Instant::now();
    let block = workload.heartbeat_paying(miner.clone(), interval, txs).await;
    let work_ns = start.elapsed().as_nanos();
    let start = Instant::now();
    let baseline = control.heartbeat_paying(miner, interval, vec![]).await;
    let control_ns = start.elapsed().as_nanos();
    assert_eq!(
        (block.header.daa_score, block.header.blue_score, block.header.timestamp),
        (baseline.header.daa_score, baseline.header.blue_score, baseline.header.timestamp)
    );
    (block, work_ns, control_ns)
}

async fn fold_at_slot(
    workload: &mut T12Chain,
    control: &mut T12Chain,
    interval: u64,
    slot: u64,
    tx: Transaction,
) -> (Block, Block, Value) {
    while workload.ctx.consensus.get_virtual_daa_score() < slot {
        beat_pair(workload, control, interval, vec![]).await;
    }
    assert_eq!(workload.ctx.consensus.get_virtual_daa_score(), slot);
    assert_eq!(workload.daa_of(workload.sink()), slot - 1);
    let txid = tx.id();
    let (source, source_ns, control_source_ns) = beat_pair(workload, control, interval, vec![tx]).await;
    assert_eq!(source.header.daa_score, slot);
    assert_eq!(workload.ctx.consensus.get_virtual_daa_score(), slot);
    assert!(source.transactions.iter().any(|tx| tx.id() == txid));
    let (folded, fold_ns, control_fold_ns) = beat_pair(workload, control, interval, vec![]).await;
    assert_eq!(folded.header.daa_score, slot);
    assert!(
        workload
            .ctx
            .consensus
            .get_block_acceptance_data(folded.header.hash)
            .unwrap()
            .iter()
            .flat_map(|entry| &entry.accepted_transactions)
            .any(|entry| entry.transaction_id == txid)
    );
    (
        source,
        folded,
        json!({"sourceNs": source_ns, "foldNs": fold_ns, "controlSourceNs": control_source_ns, "controlFoldNs": control_fold_ns}),
    )
}

fn history(chain: &T12Chain, db: &DB, sources: &[Block], folds: &[Block], open_id: Hash64) -> Value {
    let vp = chain.vp();
    let store = vp.palw_state_v2_store.read();
    let delta_bytes: Vec<_> = folds
        .iter()
        .map(|block| {
            let (_, delta) = store.delta_of(block.header.hash).expect("accepted fold journal");
            borsh::object_length(&delta).unwrap()
        })
        .collect();
    let body_bytes: Vec<_> = sources
        .iter()
        .map(|block| {
            let prefix: Vec<u8> = DatabaseStorePrefixes::BlockTransactions.into();
            let key = DbKey::new(&prefix, block.header.hash);
            db.get_pinned(&key).unwrap().expect("accepted source body").len()
        })
        .collect();
    drop(store);
    let (_, state) = chain.tip_state();
    let row_size = |key| state.staged_court_row_v1(&key).map(|row| borsh::object_length(row).unwrap());
    json!({
        "sourceBodyStoreBytes": body_bytes, "foldDeltaBorshBytes": delta_bytes,
        "sessionBorshBytes": row_size(PalwStagedCourtKeyV1::TokenizerRegistrationSession(open_id)),
        "partBorshBytes": row_size(PalwStagedCourtKeyV1::TokenizerRegistrationPart(open_id, 0)),
        "bookBorshBytes": row_size(PalwStagedCourtKeyV1::TokenizerRegistrationBook),
    })
}

async fn run(root: PathBuf) {
    let (config, bundle, premine, floats, policy) = isolated_config();
    let workload_path = root.join("workload");
    let control_path = root.join("control");
    let db = open_db(&workload_path, true);
    let control_db = open_db(&control_path, true);
    let (sender, _receiver) = async_channel::unbounded();
    let node = TestConsensus::with_db(db.clone(), &config, sender);
    let chain = t12_genesis_chain_on(node, &config, &bundle, &premine, &floats);
    let mut env = Env::around(chain, (config.clone(), bundle.clone(), premine.clone(), floats.clone()));
    warm_to(&mut env, START).await;
    let mut raw = br#"{"model":{"vocab":{"A":0},"merges":[]},"added_tokens":[]}"#.to_vec();
    raw.resize(PART_SIZE, b' ');
    let (mut m, body, eog) = prepare_class_for_raw(env, policy, &raw).await;
    let (sender, _control_receiver) = async_channel::unbounded();
    let node = TestConsensus::with_db(control_db.clone(), &config, sender);
    let mut control = t12_genesis_chain_on(node, &config, &bundle, &premine, &floats);
    for block in chain_blocks(&m.env.chain, m.env.chain.sink()) {
        arrive(&control, block, "shared measurement prerequisite").await;
    }
    control.ctx.simulated_time = m.env.chain.ctx.simulated_time;
    assert_eq!(control.tip_state().1.state_root(), m.env.chain.tip_state().1.state_root());
    let interval = config.params.target_time_per_block().max(kaspa_consensus_core::palw_heartbeat_v1::HEARTBEAT_RECOVERY_INTERVAL_MS);
    let start = [flush_memtable(&db), flush_memtable(&control_db)];
    sample(
        "baseline_flushed",
        (&m.env.chain, &db),
        (&control, &control_db),
        Some(json!({
            "flushNs":start, "rawBytes":raw.len(), "rawShape":"synthetic JSON with trailing whitespace",
            "policyId":body.policy.to_string(), "classId":body.pins.class_id.to_string(), "tokenizerId":body.statement.tokenizer_id.to_string(),
            "openId":body.id_v1().unwrap().to_string(),
        })),
    );

    let open_id = body.id_v1().unwrap();
    let quote = m.env.chain.ctx.consensus.palw_tokenizer_registration_open_quote_v1(body).unwrap().unwrap().value.unwrap();
    let wire = TokenizerRegistrationWireV1::Open(TokenizerRegistrationOpenV1 {
        body,
        signature: signed_bytes(OPENER, open_id.as_byte_slice(), TOKENIZER_REGISTRATION_OPEN_CONTEXT_V1),
    });
    let open_tx = tkr_carrier(&m.env, &wire, OPENER, quote.quote.charge + FEE);
    let (open_source, source_ns, control_source_ns) = beat_pair(&mut m.env.chain, &mut control, interval, vec![open_tx]).await;
    let (open_fold, fold_ns, control_fold_ns) = beat_pair(&mut m.env.chain, &mut control, interval, vec![]).await;
    let read = |chain: &T12Chain| chain.ctx.consensus.palw_tokenizer_registration_status_v1(open_id).unwrap().unwrap().value;
    let status = read(&m.env.chain).unwrap();
    assert_eq!(status.accepted.block, open_fold.header.hash);
    assert_eq!(status.parts.len(), 1);
    assert!(read(&control).is_none());
    sample(
        "open_accepted",
        (&m.env.chain, &db),
        (&control, &control_db),
        Some(json!({
            "sourceNs":source_ns, "foldNs":fold_ns, "controlSourceNs":control_source_ns, "controlFoldNs":control_fold_ns,
            "acceptedRegistrationChargeSompi":status.quote.charge,
        })),
    );

    let mut part = TokenizerRegistrationPartV1 {
        body: TokenizerRegistrationPartBodyV1 {
            version: body.version,
            network: body.network,
            policy: body.policy,
            open_id,
            opener: body.opener,
            index: 0,
            offset: 0,
            payload_digest: TokenizerRegistrationPartV1::payload_digest_v1(&raw).unwrap(),
        },
        signature: [0; MLDSA87_SIGNATURE_LEN],
        bytes: raw,
    };
    part.signature = signed_bytes(OPENER, part.signing_message_v1().unwrap().as_byte_slice(), TOKENIZER_REGISTRATION_PART_CONTEXT_V1);
    let part_tx = tkr_carrier(&m.env, &TokenizerRegistrationWireV1::Part(part), PART_PAYER, FEE);
    let (part_source, part_fold, part_timing) =
        fold_at_slot(&mut m.env.chain, &mut control, interval, status.parts[0].slot_daa, part_tx).await;
    let status = read(&m.env.chain).unwrap();
    assert_eq!(status.parts[0].accepted.unwrap().block, part_fold.header.hash);
    sample("part_accepted", (&m.env.chain, &db), (&control, &control_db), Some(part_timing));

    let mut finish = TokenizerRegistrationFinishV1 {
        body: TokenizerRegistrationFinishBodyV1 {
            version: body.version,
            network: body.network,
            policy: body.policy,
            open_id,
            opener: body.opener,
            eog_digest: body.eog_digest,
            finish_until_daa: body.finish_until_daa,
        },
        signature: [0; MLDSA87_SIGNATURE_LEN],
        eog,
    };
    finish.signature =
        signed_bytes(OPENER, finish.signing_message_v1().unwrap().as_byte_slice(), TOKENIZER_REGISTRATION_FINISH_CONTEXT_V1);
    let finish_tx = tkr_carrier(&m.env, &TokenizerRegistrationWireV1::Finish(finish), FINISH_PAYER, FEE);
    let (finish_source, finish_fold, finish_timing) =
        fold_at_slot(&mut m.env.chain, &mut control, interval, status.finishes[0].slot_daa, finish_tx).await;
    assert!(matches!(read(&m.env.chain).unwrap().status, TokenizerRegistrationStatusV1::Complete { .. }));
    let sources = [open_source, part_source, finish_source];
    let folds = [open_fold, part_fold, finish_fold];
    let before = history(&m.env.chain, &db, &sources, &folds, open_id);
    assert!(before["partBorshBytes"].as_u64().unwrap() > PART_SIZE as u64);
    sample("finish_accepted", (&m.env.chain, &db), (&control, &control_db), Some(json!({"timing":finish_timing,"history":before})));
    let flush = [flush_memtable(&db), flush_memtable(&control_db)];
    sample("finish_flushed", (&m.env.chain, &db), (&control, &control_db), Some(json!({"flushNs":flush})));

    let mut gc = None;
    while m.env.chain.daa_of(m.env.chain.sink()) < body.material_until_daa {
        gc = Some(beat_pair(&mut m.env.chain, &mut control, interval, vec![]).await);
    }
    let (gc, work_ns, control_ns) = gc.unwrap();
    assert_eq!(gc.header.daa_score, body.material_until_daa);
    assert!(read(&m.env.chain).is_none());
    let after = history(&m.env.chain, &db, &sources, &folds, open_id);
    for key in ["sessionBorshBytes", "partBorshBytes", "bookBorshBytes"] {
        assert!(after[key].is_null());
    }
    for key in ["sourceBodyStoreBytes", "foldDeltaBorshBytes"] {
        assert_eq!(after[key], before[key], "native GC keeps ordinary block and journal history");
    }
    let (_, state) = m.env.chain.tip_state();
    assert!(
        state
            .staged_court_row_v1(&PalwStagedCourtKeyV1::VerifiedTokenizerRegistration(body.pins.class_id, body.pins.semantics_id))
            .is_some()
    );
    let gc_delta_bytes = {
        let vp = m.env.chain.vp();
        let (_, delta) = vp.palw_state_v2_store.read().delta_of(gc.header.hash).unwrap();
        borsh::object_length(&delta).unwrap()
    };
    sample(
        "material_gc",
        (&m.env.chain, &db),
        (&control, &control_db),
        Some(json!({"foldNs":work_ns,"controlFoldNs":control_ns,"gcDeltaBorshBytes":gc_delta_bytes,"history":after})),
    );
    let flush = [flush_memtable(&db), flush_memtable(&control_db)];
    sample("material_gc_flushed", (&m.env.chain, &db), (&control, &control_db), Some(json!({"flushNs":flush})));
    let compact = |db: &DB| {
        let start = Instant::now();
        db.compact_range::<&[u8], &[u8]>(None, None);
        start.elapsed().as_nanos()
    };
    let compaction = [compact(&db), compact(&control_db)];
    sample("material_gc_compacted", (&m.env.chain, &db), (&control, &control_db), Some(json!({"compactNs":compaction})));

    let close = |chain: T12Chain| {
        let (tip, state) = chain.tip_state();
        (tip, state.state_root(), chain.ctx.simulated_time, chain.nonce_for_reopen())
    };
    let workload_closed = close(m.env.chain);
    let control_closed = close(control);
    drop(db);
    drop(control_db);
    let mut resumed = config;
    resumed.process_genesis = false;
    let reopen = |path: &Path, (tip, root, time, nonce)| {
        let start = Instant::now();
        let db = open_db(path, false);
        let (sender, _receiver) = async_channel::unbounded();
        let node = TestConsensus::with_db(db.clone(), &resumed, sender);
        let chain = t12_reopened_chain(node, &resumed, &bundle, time, nonce);
        assert_eq!(chain.sink(), tip);
        assert_eq!(chain.tip_state().1.state_root(), root);
        (chain, db, start.elapsed().as_nanos())
    };
    let (work, db, reopen_ns) = reopen(&workload_path, workload_closed);
    let (control, control_db, control_reopen_ns) = reopen(&control_path, control_closed);
    assert!(read(&work).is_none());
    assert_eq!(history(&work, &db, &sources, &folds, open_id), after);
    sample(
        "database_reopen",
        (&work, &db),
        (&control, &control_db),
        Some(json!({"reopenNs":reopen_ns,"controlReopenNs":control_reopen_ns})),
    );
}
