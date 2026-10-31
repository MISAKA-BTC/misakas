//! Independent tests of the dedicated physical local-I/O anchor database.
//! These use real RocksDB and DurableMinerResponder; native fork/row acceptance
//! remains an independent integration boundary and is not simulated as success.
#![cfg(unix)]
use super::super::{
    recovery::{DurableMinerResponderV2, JournalLimitsV2, ProtectedAnchorStoreV2},
    store::{FileStoredProviderV2, StoreLimitsV2, StoredEncodingV2, source_file_name_v2},
    tests::Fixture,
    *,
};
use super::*;
use std::{
    fs::{File, OpenOptions},
    io::{BufRead, Read, Write},
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

fn db_limits() -> RocksAnchorDbLimitsV2 {
    RocksAnchorDbLimitsV2 {
        max_cases: 8,
        max_sequence: 1024,
        max_logical_bytes: 1 << 20,
        write_buffer_bytes: 1 << 20,
        block_cache_bytes: 1 << 20,
        max_open_files: 32,
    }
}
fn process_limits() -> ProcessLimitsV2 {
    ProcessLimitsV2 {
        max_units_per_request: 8,
        max_tracked_units: 16,
        max_provider_reads: 32,
        max_prepared_bytes: 1 << 20,
        max_status_checks: 1024,
    }
}
fn journal_limits() -> JournalLimitsV2 {
    JournalLimitsV2 { max_bytes: 1 << 20, max_records: 1024 }
}
fn anchor(sequence: u64, byte: u8) -> JournalAnchorV2 {
    JournalAnchorV2 { sequence, hash: Digest([byte; 64]) }
}
struct TempScope(PathBuf);
impl TempScope {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!(
            "misaka-a2-rocks-independent-{}-{now}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn db(&self) -> PathBuf {
        self.0.join("local-io-anchors")
    }
    fn journal(&self) -> PathBuf {
        self.0.join("prepared.journal")
    }
}
impl Drop for TempScope {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn resign(f: &mut Fixture) {
    let keys = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([61; 32]);
    f.signature =
        *libcrux_ml_dsa::ml_dsa_87::sign(&keys.signing_key, &f.result.id().0.0, RESULT_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
}

#[test]
fn cloned_database_owners_share_an_exclusive_case_scope_and_persisted_cas_head() {
    let dir = TempScope::new();
    let f = Fixture::new();
    let providers: [&dyn StoredProviderV2; 1] = [&f.provider];
    let db = RocksAnchorDbV2::create(&dir.db(), db_limits()).unwrap();
    let clone = db.clone();
    let fresh = f.responder(&providers, process_limits());
    let mut first = db.bind(&fresh).unwrap();
    assert!(clone.bind(&fresh).is_err(), "another DB handle acquired the active case");
    assert_eq!(first.load().unwrap(), None);
    first.commit(None, anchor(0, 10)).unwrap();
    assert!(first.commit(Some(anchor(0, 11)), anchor(1, 21)).is_err());
    first.commit(Some(anchor(0, 10)), anchor(1, 20)).unwrap();
    assert!(first.commit(Some(anchor(1, 20)), anchor(3, 30)).is_err(), "sequence gap was accepted");
    assert_eq!(clone.case_count().unwrap(), 1);
    drop(first);
    let mut rebound = clone.bind(&fresh).unwrap();
    assert_eq!(rebound.load().unwrap(), Some(anchor(1, 20)));
    assert!(rebound.commit(None, anchor(0, 10)).is_err());
    assert!(db.bind(&fresh).is_err());
}

#[test]
fn same_case_cannot_rebind_result_service_or_process_limits() {
    let dir = TempScope::new();
    let f = Fixture::new();
    let providers: [&dyn StoredProviderV2; 1] = [&f.provider];
    let db = RocksAnchorDbV2::create(&dir.db(), db_limits()).unwrap();
    let fresh = f.responder(&providers, process_limits());
    let original = db.bind(&fresh).unwrap();
    drop(original);
    let logical = db.logical_bytes().unwrap();
    let mut changed = Fixture::new();
    changed.result.output = Digest([177; 64]);
    resign(&mut changed);
    assert_eq!(changed.result.case_id(), f.result.case_id());
    assert!(db.bind(&changed.responder(&providers, process_limits())).is_err());
    let mut changed = Fixture::new();
    changed.contract.budget.fee += 1;
    changed.result.service_contract = changed.contract.id();
    resign(&mut changed);
    assert_eq!(changed.result.case_id(), f.result.case_id());
    assert!(db.bind(&changed.responder(&providers, process_limits())).is_err());
    let mut tighter = process_limits();
    tighter.max_provider_reads -= 1;
    assert!(db.bind(&f.responder(&providers, tighter)).is_err());
    assert_eq!(db.case_count().unwrap(), 1);
    assert_eq!(db.logical_bytes().unwrap(), logical);
    assert!(db.bind(&fresh).is_ok());
    assert_eq!(db.logical_bytes().unwrap(), logical);
}

#[test]
fn fork_shaped_new_namespace_does_not_reset_an_existing_local_case() {
    let dir = TempScope::new();
    let f = Fixture::new();
    let providers: [&dyn StoredProviderV2; 1] = [&f.provider];
    let db = RocksAnchorDbV2::create(&dir.db(), db_limits()).unwrap();
    let mut old = db.bind(&f.responder(&providers, process_limits())).unwrap();
    old.commit(None, anchor(0, 1)).unwrap();
    old.commit(Some(anchor(0, 1)), anchor(1, 2)).unwrap();
    let mut fork = Fixture::new();
    fork.result.scope.branch = Digest([201; 64]);
    fork.admission.scope = fork.result.scope;
    fork.contract.case = fork.result.case_id();
    fork.result.service_contract = fork.contract.id();
    resign(&mut fork);
    assert_ne!(fork.result.case_id(), f.result.case_id());
    let new = db.bind(&fork.responder(&providers, process_limits())).unwrap();
    assert_eq!(new.load().unwrap(), None);
    assert_eq!(old.load().unwrap(), Some(anchor(1, 2)));
    assert!(old.commit(None, anchor(0, 1)).is_err());
    // This is physical scope isolation, not a claim that native reorg was run.
    assert_eq!(db.case_count().unwrap(), 2);
}

#[test]
fn old_journal_replay_cannot_rollback_the_dedicated_database_anchor() {
    let dir = TempScope::new();
    let f = Fixture::new();
    let providers: [&dyn StoredProviderV2; 1] = [&f.provider];
    let db = RocksAnchorDbV2::create(&dir.db(), db_limits()).unwrap();
    let fresh = f.responder(&providers, process_limits());
    let protected = db.bind(&fresh).unwrap();
    let mut live = DurableMinerResponderV2::create(&dir.journal(), fresh, journal_limits(), protected).unwrap();
    let old_header = std::fs::read(dir.journal()).unwrap();
    live.respond(&f.duty()).unwrap();
    let saved = live.meter();
    let latest = live.anchor();
    let complete = std::fs::read(dir.journal()).unwrap();
    drop(live);
    std::fs::write(dir.journal(), &old_header).unwrap();
    let fresh = f.responder(&providers, process_limits());
    let protected = db.bind(&fresh).unwrap();
    assert!(DurableMinerResponderV2::resume(&dir.journal(), fresh, journal_limits(), protected).is_err());
    let fresh = f.responder(&providers, process_limits());
    let protected = db.bind(&fresh).unwrap();
    assert!(DurableMinerResponderV2::resume_discarding_unanchored_tail(&dir.journal(), fresh, journal_limits(), protected).is_err());
    let protected = db.bind(&f.responder(&providers, process_limits())).unwrap();
    assert_eq!(protected.load().unwrap(), Some(latest));
    drop(protected);
    std::fs::write(dir.journal(), complete).unwrap();
    let fresh = f.responder(&providers, process_limits());
    let protected = db.bind(&fresh).unwrap();
    let restored = DurableMinerResponderV2::resume(&dir.journal(), fresh, journal_limits(), protected).unwrap();
    assert_eq!(restored.meter(), saved);
}

#[test]
fn native_or_unmarked_database_is_not_adopted_or_overwritten() {
    let dir = TempScope::new();
    let native = dir.0.join("native-database");
    let mut options = ::rocksdb::Options::default();
    options.create_if_missing(true);
    let raw = ::rocksdb::DB::open(&options, &native).unwrap();
    raw.put(b"native-state-sentinel", b"must-stay").unwrap();
    drop(raw);
    let before = std::fs::read_dir(&native).unwrap().map(|e| e.unwrap().file_name()).collect::<std::collections::BTreeSet<_>>();
    assert!(RocksAnchorDbV2::create(&native, db_limits()).is_err());
    assert!(RocksAnchorDbV2::open(&native, db_limits()).is_err());
    let after = std::fs::read_dir(&native).unwrap().map(|e| e.unwrap().file_name()).collect::<std::collections::BTreeSet<_>>();
    assert_eq!(before, after, "failed local-anchor open modified the native directory");
    let raw = ::rocksdb::DB::open(&options, &native).unwrap();
    assert_eq!(raw.get(b"native-state-sentinel").unwrap().unwrap(), b"must-stay");
}

#[test]
fn case_cap_and_creation_limits_survive_database_reopen() {
    let dir = TempScope::new();
    let f = Fixture::new();
    let providers: [&dyn StoredProviderV2; 1] = [&f.provider];
    let mut cap = db_limits();
    cap.max_cases = 1;
    {
        let db = RocksAnchorDbV2::create(&dir.db(), cap).unwrap();
        let _first = db.bind(&f.responder(&providers, process_limits())).unwrap();
        let mut second = Fixture::new();
        second.result.scope.branch = Digest([203; 64]);
        second.admission.scope = second.result.scope;
        second.contract.case = second.result.case_id();
        second.result.service_contract = second.contract.id();
        resign(&mut second);
        assert!(db.bind(&second.responder(&providers, process_limits())).is_err());
        assert_eq!(db.case_count().unwrap(), 1);
    }
    assert!(RocksAnchorDbV2::open(&dir.db(), db_limits()).is_err(), "reopen enlarged creation consent");
    let db = RocksAnchorDbV2::open(&dir.db(), cap).unwrap();
    assert_eq!(db.case_count().unwrap(), 1);
}

const CHILD: &str = "miner::positive_v2::rocksdb::tests::rocksdb_process_child";
const PHASE_ENV: &str = "MISAKA_A2_ROCKS_INDEPENDENT_PHASE";
const ROOT_ENV: &str = "MISAKA_A2_ROCKS_INDEPENDENT_ROOT";
const DONE: &str = "A2_ROCKS_CHILD_DONE";
fn child_command(root: &Path, phase: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.arg("--exact").arg(CHILD).arg("--nocapture").env(PHASE_ENV, phase).env(ROOT_ENV, root);
    command
}
struct RunningChild {
    child: Child,
    lines: mpsc::Receiver<String>,
    reader: Option<JoinHandle<()>>,
    errors: PathBuf,
}
impl RunningChild {
    fn start(root: &Path, phase: &str) -> Self {
        let errors = root.join(format!("{phase}.stderr"));
        let mut command = child_command(root, phase);
        // Keep the parent's write endpoint alive in Child.stdin. A borrowed
        // terminal or null stdin gives the paused child EOF and turns an
        // intended crash into a normal early failure.
        command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::from(File::create(&errors).unwrap()));
        let mut child = command.spawn().unwrap();
        let out = child.stdout.take().unwrap();
        let (send, lines) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in std::io::BufReader::new(out).lines() {
                let Ok(line) = line else { break };
                if send.send(line).is_err() {
                    break;
                }
            }
        });
        Self { child, lines, reader: Some(reader), errors }
    }
    fn error_text(&self) -> String {
        let mut body = String::new();
        File::open(&self.errors).unwrap().take(65536).read_to_string(&mut body).unwrap();
        body
    }
    fn marker(&mut self, marker: &str) {
        let start = Instant::now();
        loop {
            let remaining = Duration::from_secs(30).checked_sub(start.elapsed()).expect("child marker timeout");
            match self.lines.recv_timeout(remaining) {
                Ok(line) if line.contains(marker) => return,
                Ok(_) => {}
                Err(error) => panic!("missing child marker {marker}: {error}; stderr {}", self.error_text()),
            }
        }
    }
    fn finish(mut self) {
        self.marker(DONE);
        let start = Instant::now();
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success(), "child failed: {}", self.error_text());
                return;
            }
            assert!(start.elapsed() < Duration::from_secs(30), "child did not exit");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn paused(&mut self, marker: &str) {
        self.marker(marker);
        assert!(self.child.try_wait().unwrap().is_none(), "paused child already exited: {}", self.error_text());
    }
    fn kill(mut self) {
        assert!(self.child.try_wait().unwrap().is_none(), "child exited before SIGKILL: {}", self.error_text());
        self.child.kill().unwrap();
        let status = self.child.wait().unwrap();
        assert_eq!(status.signal(), Some(9), "child did not terminate through SIGKILL: {status}; {}", self.error_text());
    }
}
impl Drop for RunningChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
struct CheckedProvider<'a> {
    inner: &'a FileStoredProviderV2,
    db: RocksAnchorDbV2,
    key: [u8; CASE_KEY_BYTES],
    stop_before_read: bool,
}
impl StoredProviderV2 for CheckedProvider<'_> {
    fn unit(&self, kind: SourceKind, source: Digest, index: u64) -> Result<&StoredUnitV2, String> {
        assert!(
            read_case(&self.db.owner.db, &self.key).unwrap().unwrap().current.unwrap().sequence >= 3,
            "RocksDB CAS did not precede provider read"
        );
        if self.stop_before_read {
            println!("A2_ROCKS_PROVIDER_RESERVED");
            std::io::stdout().flush().unwrap();
            std::io::stdin().read_exact(&mut [0u8; 1]).unwrap();
        }
        self.inner.unit(kind, source, index)
    }
}
fn file_provider(root: &Path, f: &Fixture) -> FileStoredProviderV2 {
    let name = source_file_name_v2(f.result.artifact, 0, StoredEncodingV2::Artifact);
    if !root.join(&name).exists() {
        let StoredUnitV2::Artifact(opening) = f.provider.unit(SourceKind::ArtifactInventory, f.result.artifact, 0).unwrap() else {
            unreachable!()
        };
        let mut file = OpenOptions::new().write(true).create_new(true).open(root.join(name)).unwrap();
        file.write_all(&borsh::to_vec(opening).unwrap()).unwrap();
        file.sync_all().unwrap();
    }
    let mut provider = FileStoredProviderV2::open(
        root,
        StoreLimitsV2 {
            max_unit_bytes: 4096,
            max_cached_bytes: 8192,
            max_cached_units: 2,
            max_file_reads: 2,
            max_reserved_read_bytes: 8192,
        },
    )
    .unwrap();
    provider.load(f.result.artifact, 0, StoredEncodingV2::Artifact).unwrap();
    provider
}

#[test]
fn rocksdb_process_child() {
    let Ok(phase) = std::env::var(PHASE_ENV) else { return };
    let root = PathBuf::from(std::env::var_os(ROOT_ENV).unwrap());
    let db_path = root.join("local-io-anchors");
    if phase == "expect-db-lock" {
        assert!(RocksAnchorDbV2::open(&db_path, db_limits()).is_err());
        println!("{DONE}");
        return;
    }
    let f = Fixture::new();
    if phase == "hold-db" {
        let _db = RocksAnchorDbV2::open(&db_path, db_limits()).unwrap();
        println!("A2_ROCKS_DB_HELD");
        std::io::stdout().flush().unwrap();
        std::io::stdin().read_exact(&mut [0u8; 1]).unwrap();
        return;
    }
    let provider = file_provider(&root, &f);
    let first = phase.starts_with("pause-");
    let db =
        if first { RocksAnchorDbV2::create(&db_path, db_limits()) } else { RocksAnchorDbV2::open(&db_path, db_limits()) }.unwrap();
    // Read-only observation is deliberately test-private; the exclusive case
    // handle itself is moved into the one live journal and is never cloned.
    let checked = CheckedProvider {
        inner: &provider,
        db: db.clone(),
        key: case_key(&f.result),
        stop_before_read: phase == "pause-before-provider",
    };
    let providers: [&dyn StoredProviderV2; 1] = [&checked];
    let fresh = f.responder(&providers, process_limits());
    let anchors = db.bind(&fresh).unwrap();
    let path = root.join("prepared.journal");
    let mut live = if first {
        DurableMinerResponderV2::create(&path, fresh, journal_limits(), anchors)
    } else {
        DurableMinerResponderV2::resume(&path, fresh, journal_limits(), anchors)
    }
    .unwrap();
    if phase == "resume-after-provider-kill" {
        assert_eq!(live.meter().provider_reads, 1);
        assert_eq!(live.meter().unique_source_bytes, 0);
        live.respond(&f.duty()).unwrap();
        assert_eq!(live.meter().provider_reads, 2);
        assert_eq!(live.meter().unique_source_bytes, 16);
    } else if phase == "resume-after-response-kill" {
        assert_eq!(live.meter().provider_reads, 1);
        assert_eq!(live.meter().unique_source_bytes, 16);
        live.respond(&f.duty()).unwrap();
        assert_eq!(live.meter().provider_reads, 2);
        assert_eq!(live.meter().unique_source_bytes, 16);
    } else if phase == "pause-before-provider" || phase == "pause-after-response" {
        live.respond(&f.duty()).unwrap();
        println!("A2_ROCKS_RESPONSE_RESERVED");
        std::io::stdout().flush().unwrap();
        std::io::stdin().read_exact(&mut [0u8; 1]).unwrap();
    } else {
        panic!("unknown child phase {phase}");
    }
    println!("{DONE}");
}

#[test]
fn real_process_crashes_preserve_reservations_and_release_database_lock() {
    for (first, marker, resume) in [
        ("pause-before-provider", "A2_ROCKS_PROVIDER_RESERVED", "resume-after-provider-kill"),
        ("pause-after-response", "A2_ROCKS_RESPONSE_RESERVED", "resume-after-response-kill"),
    ] {
        let dir = TempScope::new();
        let mut child = RunningChild::start(&dir.0, first);
        child.paused(marker);
        child.kill();
        RunningChild::start(&dir.0, resume).finish();
        let mut holder = RunningChild::start(&dir.0, "hold-db");
        holder.paused("A2_ROCKS_DB_HELD");
        RunningChild::start(&dir.0, "expect-db-lock").finish();
        holder.kill();
        let reopened = RocksAnchorDbV2::open(&dir.db(), db_limits()).unwrap();
        assert_eq!(reopened.case_count().unwrap(), 1);
    }
}

#[test]
fn concurrent_database_clones_cannot_lease_two_writers_for_the_same_case() {
    let dir = TempScope::new();
    let db = RocksAnchorDbV2::create(&dir.db(), db_limits()).unwrap();
    let start = std::sync::Arc::new(std::sync::Barrier::new(2));
    let finish = std::sync::Arc::new(std::sync::Barrier::new(2));
    let jobs: Vec<_> = (0..2)
        .map(|_| {
            let db = db.clone();
            let start = start.clone();
            let finish = finish.clone();
            std::thread::spawn(move || {
                let f = Fixture::new();
                let providers: [&dyn StoredProviderV2; 1] = [&f.provider];
                let fresh = f.responder(&providers, process_limits());
                start.wait();
                let leased = db.bind(&fresh);
                let success = leased.is_ok();
                // Keep the winner alive until the other bind has actually run.
                finish.wait();
                drop(leased);
                success
            })
        })
        .collect();
    let successes = jobs.into_iter().map(|job| job.join().unwrap()).filter(|success| *success).count();
    assert_eq!(successes, 1);
    assert_eq!(db.case_count().unwrap(), 1);
}

#[test]
fn unknown_internal_marker_and_corrupt_row_are_refused_without_recreation() {
    let dir = TempScope::new();
    let f = Fixture::new();
    let providers: [&dyn StoredProviderV2; 1] = [&f.provider];
    let db = RocksAnchorDbV2::create(&dir.db(), db_limits()).unwrap();
    let original = db.bind(&f.responder(&providers, process_limits())).unwrap();
    db.owner.db.put(&original.key, [0u8; 1]).unwrap();
    assert!(original.load().is_err());
    drop(original);
    assert!(db.bind(&f.responder(&providers, process_limits())).is_err());
    db.owner.db.put(SCHEMA_KEY, b"unknown-owner-schema").unwrap();
    drop(db);
    assert!(RocksAnchorDbV2::open(&dir.db(), db_limits()).is_err());
    assert!(RocksAnchorDbV2::create(&dir.db(), db_limits()).is_err());
}

#[test]
fn two_different_journal_paths_cannot_merge_identical_checkpoints_into_one_anchor() {
    let dir = TempScope::new();
    let f = Fixture::new();
    let providers: [&dyn StoredProviderV2; 1] = [&f.provider];
    let db = RocksAnchorDbV2::create(&dir.db(), db_limits()).unwrap();
    let clone = db.clone();
    let first = f.responder(&providers, process_limits());
    let scope = db.bind(&first).unwrap();
    let mut live = DurableMinerResponderV2::create(&dir.journal(), first, journal_limits(), scope).unwrap();
    // Headers and first counter transitions would be byte-identical at a second
    // path. Idempotent per-record CAS is therefore insufficient without this lease.
    assert!(clone.bind(&f.responder(&providers, process_limits())).is_err());
    live.respond(&f.duty()).unwrap();
    let latest = live.anchor();
    let spent = live.meter();
    drop(live);
    let fresh = f.responder(&providers, process_limits());
    let scope = clone.bind(&fresh).unwrap();
    assert_eq!(scope.load().unwrap(), Some(latest));
    let alias = dir.0.join("another.journal");
    assert!(
        DurableMinerResponderV2::create(&alias, fresh, journal_limits(), scope).is_err(),
        "existing head was recreated at another path"
    );
    let fresh = f.responder(&providers, process_limits());
    let scope = db.bind(&fresh).unwrap();
    let restored = DurableMinerResponderV2::resume(&dir.journal(), fresh, journal_limits(), scope).unwrap();
    assert_eq!(restored.meter(), spent);
    assert_eq!(f.provider.calls.get(), 1);
}
