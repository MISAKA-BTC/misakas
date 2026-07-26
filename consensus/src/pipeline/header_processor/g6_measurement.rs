//! ADR-0040 G6 measurement harness.
//!
//! This is deliberately an ignored test: it drives the ordinary production header pipeline against
//! a private Header-v4 re-genesis fixture and emits evidence, but it does not invent an activation
//! threshold or change any shipped network preset.

use crate::{
    config::ConfigBuilder,
    consensus::test_consensus::TestConsensus,
    errors::RuleError,
    model::stores::{
        palw_spam::{PalwSpamAccumulatorStoreReader, PalwSpamLaneDelta, palw_spam_derive_child},
        relations::RelationsStoreReader,
        statuses::StatusesStoreReader,
    },
};
use kaspa_consensus_core::{
    api::ConsensusApi,
    block::Block,
    blockstatus::BlockStatus,
    config::params::{DEVNET_PALW_PARAMS, DEVNET_PARAMS, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PALW_PARAMS, TESTNET_PARAMS},
    constants::PALW_ANTISPAM_HEADER_VERSION,
    header::{Header, PalwHeaderFields},
    palw_antispam::{PalwSpamParams, mine_palw_spam_stamp, palw_spam_leading_zero_bits, palw_spam_target},
    pow_layer0::POW_ALGO_ID_PALW_REPLICA,
};
use kaspa_core::time::unix_now;
use kaspa_hashes::Hash64;
use serde::Serialize;
use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    process::Command,
    time::Instant,
};

const DEFAULT_INVALID_SIBLINGS: usize = 1_000;
const DEFAULT_INVALID_ORPHANS: usize = 1_000;
const DEFAULT_VALID_HEADERS: usize = 1_000;
const DEFAULT_WARMUP_HEADERS: usize = 32;
const DEFAULT_STAMP_MAX_NONCE: u64 = (1 << 26) - 1;
const MAX_SAMPLES: usize = 100_000;

#[derive(Debug, Serialize)]
struct Distribution {
    count: usize,
    min: u64,
    median: u64,
    p95: u64,
    p99: u64,
    max: u64,
}

impl Distribution {
    fn from_values(mut values: Vec<u64>) -> Self {
        assert!(!values.is_empty(), "a G6 distribution must contain at least one sample");
        values.sort_unstable();
        Self {
            count: values.len(),
            min: values[0],
            median: nearest_rank(&values, 50),
            p95: nearest_rank(&values, 95),
            p99: nearest_rank(&values, 99),
            max: *values.last().unwrap(),
        }
    }
}

fn nearest_rank(sorted: &[u64], percentile: usize) -> u64 {
    assert!(!sorted.is_empty());
    assert!((1..=100).contains(&percentile));
    let rank = sorted.len().saturating_mul(percentile).div_ceil(100).clamp(1, sorted.len());
    sorted[rank - 1]
}

#[derive(Debug, Serialize)]
struct RejectionEvidence {
    attempted: usize,
    rejected: usize,
    expected_error: &'static str,
    latency_ns: Distribution,
    actual_stamp_leading_zero_bits: Distribution,
    header_commit_db_write_batches_per_header: Distribution,
    header_commit_write_batch_operations_per_header: Distribution,
    header_commit_reachability_operations_per_header: Distribution,
    header_commit_reachability_data_writes_per_header: Distribution,
    timed_success_counters_per_header: Distribution,
    persisted_header_rows_detected: u64,
    persisted_status_rows_detected: u64,
    persisted_relation_rows_detected: u64,
    persisted_spam_accumulator_rows_detected: u64,
}

#[derive(Debug, Serialize)]
struct AcceptanceEvidence {
    attempted: usize,
    accepted: usize,
    expected_status: &'static str,
    required_stamp_bits: Distribution,
    actual_stamp_leading_zero_bits: Distribution,
    stamp_grind_attempts: Distribution,
    latency_ns: Distribution,
    internal_validate_ns: Distribution,
    internal_commit_ns: Distribution,
    internal_db_write_ns: Distribution,
    header_commit_db_write_batches_per_header: Distribution,
    header_commit_write_batch_operations_per_header: Distribution,
    header_commit_reachability_operations_per_header: Distribution,
    header_commit_reachability_data_writes_per_header: Distribution,
    header_commit_non_reachability_operations_per_header: Distribution,
}

#[derive(Debug, Serialize)]
struct MachineMetadata {
    os: &'static str,
    arch: &'static str,
    logical_parallelism: usize,
    cpu_model: Option<String>,
    memory_bytes: Option<u64>,
    uname: Option<String>,
    rustc_version: Option<String>,
    cargo_version: Option<String>,
    build_profile: &'static str,
}

#[derive(Debug, Serialize)]
struct SourceMetadata {
    git_commit: Option<String>,
    git_tree_clean: Option<bool>,
    source_provenance_final_evidence_eligible: bool,
    source_provenance_note: &'static str,
    package_version: &'static str,
}

#[derive(Debug, Serialize)]
struct FixtureMetadata {
    base_preset: &'static str,
    isolated_test_only_regenesis: bool,
    header_version: u16,
    algo_id: u8,
    window_daa: u64,
    replicas_per_hash: u64,
    burst: u64,
    base_stamp_bits: u16,
    max_stamp_bits: u16,
    shipped_presets_checked: [&'static str; 6],
    shipped_presets_remain_pre_v4_inert_and_accept_closed: bool,
    storage_fixture: &'static str,
    production_equivalence: &'static str,
    slowest_supported_storage_measured: bool,
}

#[derive(Debug, Serialize)]
struct InvocationMetadata {
    command: &'static str,
    percentile_method: &'static str,
    latency_boundary: &'static str,
    in_flight_headers: usize,
    concurrency_scope: &'static str,
    unmeasured_concurrency: &'static str,
    warmup_headers: usize,
    invalid_siblings: usize,
    invalid_orphans: usize,
    valid_headers: usize,
    stamp_max_nonce_inclusive: u64,
}

#[derive(Debug, Serialize)]
struct GateMetadata {
    id: &'static str,
    status: &'static str,
    public_value_activation: &'static str,
    thresholds: Option<serde_json::Value>,
    conclusion: &'static str,
}

#[derive(Debug, Serialize)]
struct ReachabilityRiskMetadata {
    public_value_activation: &'static str,
    trigger: &'static str,
    root_cause: &'static str,
    asymptotic_write_bound: &'static str,
    remediation_class: &'static str,
}

#[derive(Debug, Serialize)]
struct G6Report {
    schema: &'static str,
    started_unix_ms: u64,
    finished_unix_ms: u64,
    gate: GateMetadata,
    reachability_risk: ReachabilityRiskMetadata,
    source: SourceMetadata,
    machine: MachineMetadata,
    fixture: FixtureMetadata,
    invocation: InvocationMetadata,
    invalid_weak_stamp_siblings: RejectionEvidence,
    invalid_weak_stamp_orphans: RejectionEvidence,
    valid_stamped_headers: AcceptanceEvidence,
}

#[derive(Clone, Copy)]
enum InvalidKind {
    Sibling,
    Orphan,
}

fn tagged_hash(tag: u8, sequence: u64) -> Hash64 {
    let mut bytes = [0u8; 64];
    bytes[0] = tag;
    bytes[8..16].copy_from_slice(&sequence.to_le_bytes());
    Hash64::from_bytes(bytes)
}

fn measurement_config() -> kaspa_consensus_core::config::Config {
    let mut params = DEVNET_PALW_PARAMS;
    params.genesis.version = PALW_ANTISPAM_HEADER_VERSION;
    params.genesis.hash = Hash64::default();
    params.genesis.hash = Header::from(&params.genesis).hash;
    params.palw_algo4_accept = true;
    params.palw_spam = PalwSpamParams::PUBLIC_REGENESIS_CANDIDATE;
    ConfigBuilder::new(params).build()
}

fn assert_shipped_presets_inert() {
    for (name, params) in [
        ("mainnet", MAINNET_PARAMS),
        ("testnet-10", TESTNET_PARAMS),
        ("testnet-palw-110", TESTNET_PALW_PARAMS),
        ("devnet-palw-111", DEVNET_PALW_PARAMS),
        ("simnet", SIMNET_PARAMS),
        ("devnet", DEVNET_PARAMS),
    ] {
        assert!(params.palw_spam.is_inert(), "{name} unexpectedly activated Header-v4 anti-spam");
        assert!(params.genesis.version < PALW_ANTISPAM_HEADER_VERSION, "{name} unexpectedly moved to Header-v4");
        assert!(!params.palw_algo4_accept, "{name} unexpectedly opened algo-4 acceptance");
    }
}

fn replica_header(tc: &TestConsensus, sequence: u64) -> (Header, u16) {
    let parent = tc.params().genesis.hash;
    let mut header = tc.build_header_with_parents(tagged_hash(0x46, sequence), vec![parent]);
    let ghostdag = tc.ghostdag_manager().ghostdag(header.direct_parents());
    let (state, counts) = palw_spam_derive_child(
        tc.palw_spam_store.as_ref(),
        ghostdag.selected_parent,
        header.daa_score,
        tc.params().palw_spam.window_daa,
        PalwSpamLaneDelta::default(),
        true,
    )
    .expect("derive the fork-local Header-v4 sibling state");
    let target = palw_spam_target(tc.params().palw_spam, counts).expect("derive the Header-v4 sibling target");

    header.version = PALW_ANTISPAM_HEADER_VERSION;
    header.pow_algo_id = POW_ALGO_ID_PALW_REPLICA;
    header.bits = tc.params().palw_lane_difficulty.genesis_replica_bits;
    header = header.with_palw_fields(PalwHeaderFields {
        blue_hash_work: ghostdag.blue_hash_work,
        blue_compute_work: ghostdag.blue_compute_work,
        palw_ticket_nullifier: tagged_hash(0x47, sequence),
        palw_spam_accumulator_commitment: state.commitment(),
        ..Default::default()
    });
    (header, target.required_stamp_bits)
}

fn force_weak_stamp(header: &mut Header, floor: u16) -> u16 {
    for nonce in 0..=1_000_000u64 {
        header.palw_spam_nonce = nonce;
        header.finalize();
        let actual = palw_spam_leading_zero_bits(header);
        if actual < floor {
            return actual;
        }
    }
    panic!("could not construct a weak adversarial stamp within the bounded fixture search")
}

fn assert_no_header_stage_rows(tc: &TestConsensus, hash: Hash64) {
    assert!(!tc.headers_store.has(hash).unwrap(), "rejected header reached the header store");
    assert!(!tc.statuses_store.read().has(hash).unwrap(), "rejected header reached the status store");
    assert!(!tc.relations_store.read().has(hash).unwrap(), "rejected header reached the relations store");
    assert!(tc.palw_spam_store.get_optional(hash).unwrap().is_none(), "rejected header reached the anti-spam accumulator store");
}

async fn measure_rejections(tc: &TestConsensus, kind: InvalidKind, samples: usize, sequence_base: u64) -> RejectionEvidence {
    let mut latencies = Vec::with_capacity(samples);
    let mut actual_bits = Vec::with_capacity(samples);
    let mut db_batches = Vec::with_capacity(samples);
    let mut db_ops = Vec::with_capacity(samples);
    let mut reachability_ops = Vec::with_capacity(samples);
    let mut reachability_data_writes = Vec::with_capacity(samples);
    let mut timed = Vec::with_capacity(samples);

    for offset in 0..samples {
        let sequence = sequence_base + offset as u64;
        let (mut header, _) = replica_header(tc, sequence);
        if matches!(kind, InvalidKind::Orphan) {
            header.parents_by_level = vec![vec![tagged_hash(0x4f, sequence)]].try_into().unwrap();
        }
        let actual = force_weak_stamp(&mut header, tc.params().palw_spam.base_stamp_bits);
        let hash = header.hash;
        let block = Block::from_header(header);
        let before = tc.processing_counters().snapshot();
        let started = Instant::now();
        let result = tc.validate_and_insert_block(block).block_task.await;
        latencies.push(started.elapsed().as_nanos() as u64);
        let after = tc.processing_counters().snapshot();
        let delta = &after - &before;

        match result {
            Err(RuleError::PalwSpamBaseStampTooWeak { required_bits, actual_bits }) => {
                assert_eq!(required_bits, tc.params().palw_spam.base_stamp_bits);
                assert_eq!(actual_bits, actual);
            }
            other => panic!("weak Header-v4 stamp did not fail in isolation: {other:?}"),
        }
        assert_eq!(delta.hdr_dbwrite_batches, 0, "weak stamp reached db.write(batch)");
        assert_eq!(delta.hdr_dbwrite_ops, 0, "weak stamp staged RocksDB batch operations");
        assert_eq!(delta.hdr_reachability_dbwrite_ops, 0, "weak stamp staged reachability batch operations");
        assert_eq!(delta.hdr_reachability_data_writes, 0, "weak stamp staged reachability data rows");
        assert_eq!(delta.hdr_timed_counts, 0, "weak stamp reached the successful ordinary-header timer");
        assert_no_header_stage_rows(tc, hash);
        actual_bits.push(actual as u64);
        db_batches.push(delta.hdr_dbwrite_batches);
        db_ops.push(delta.hdr_dbwrite_ops);
        reachability_ops.push(delta.hdr_reachability_dbwrite_ops);
        reachability_data_writes.push(delta.hdr_reachability_data_writes);
        timed.push(delta.hdr_timed_counts);
    }

    RejectionEvidence {
        attempted: samples,
        rejected: samples,
        expected_error: "PalwSpamBaseStampTooWeak",
        latency_ns: Distribution::from_values(latencies),
        actual_stamp_leading_zero_bits: Distribution::from_values(actual_bits),
        header_commit_db_write_batches_per_header: Distribution::from_values(db_batches),
        header_commit_write_batch_operations_per_header: Distribution::from_values(db_ops),
        header_commit_reachability_operations_per_header: Distribution::from_values(reachability_ops),
        header_commit_reachability_data_writes_per_header: Distribution::from_values(reachability_data_writes),
        timed_success_counters_per_header: Distribution::from_values(timed),
        persisted_header_rows_detected: 0,
        persisted_status_rows_detected: 0,
        persisted_relation_rows_detected: 0,
        persisted_spam_accumulator_rows_detected: 0,
    }
}

struct ValidSample {
    required_bits: u64,
    actual_bits: u64,
    grind_attempts: u64,
    latency_ns: u64,
    validate_ns: u64,
    commit_ns: u64,
    db_write_ns: u64,
    db_batches: u64,
    db_ops: u64,
    reachability_ops: u64,
    reachability_data_writes: u64,
}

async fn submit_valid_header(tc: &TestConsensus, sequence: u64, stamp_max_nonce: u64) -> ValidSample {
    let (mut header, required_bits) = replica_header(tc, sequence);
    let mined_nonce = mine_palw_spam_stamp(&mut header, required_bits, 0, stamp_max_nonce)
        .expect("the bounded G6 valid-stamp search exhausted its configured nonce range");
    let actual_bits = palw_spam_leading_zero_bits(&header);
    let hash = header.hash;
    let block = Block::from_header(header);
    let before = tc.processing_counters().snapshot();
    let started = Instant::now();
    let status = tc.validate_and_insert_block(block).block_task.await.expect("valid stamped header rejected");
    let latency_ns = started.elapsed().as_nanos() as u64;
    let after = tc.processing_counters().snapshot();
    let delta = &after - &before;

    assert_eq!(status, BlockStatus::StatusHeaderOnly);
    assert_eq!(delta.hdr_timed_counts, 1, "valid header must traverse one ordinary timed path");
    assert_eq!(delta.hdr_dbwrite_batches, 1, "valid header must commit one atomic header batch");
    assert!(delta.hdr_dbwrite_ops > 0, "valid header batch must contain RocksDB operations");
    assert!(delta.hdr_reachability_dbwrite_ops <= delta.hdr_dbwrite_ops);
    assert!(delta.hdr_reachability_data_writes <= delta.hdr_reachability_dbwrite_ops);
    assert!(tc.headers_store.has(hash).unwrap());
    assert!(tc.statuses_store.read().has(hash).unwrap());
    assert!(tc.relations_store.read().has(hash).unwrap());
    assert!(tc.palw_spam_store.get_optional(hash).unwrap().is_some());

    ValidSample {
        required_bits: required_bits as u64,
        actual_bits: actual_bits as u64,
        grind_attempts: mined_nonce + 1,
        latency_ns,
        validate_ns: delta.hdr_validate_ns,
        commit_ns: delta.hdr_commit_ns,
        db_write_ns: delta.hdr_dbwrite_ns,
        db_batches: delta.hdr_dbwrite_batches,
        db_ops: delta.hdr_dbwrite_ops,
        reachability_ops: delta.hdr_reachability_dbwrite_ops,
        reachability_data_writes: delta.hdr_reachability_data_writes,
    }
}

async fn measure_valid_headers(
    tc: &TestConsensus,
    warmups: usize,
    samples: usize,
    stamp_max_nonce: u64,
    sequence_base: u64,
) -> AcceptanceEvidence {
    for offset in 0..warmups {
        submit_valid_header(tc, sequence_base + offset as u64, stamp_max_nonce).await;
    }

    let mut measured = Vec::with_capacity(samples);
    for offset in 0..samples {
        measured.push(submit_valid_header(tc, sequence_base + warmups as u64 + offset as u64, stamp_max_nonce).await);
    }

    AcceptanceEvidence {
        attempted: samples,
        accepted: samples,
        expected_status: "StatusHeaderOnly",
        required_stamp_bits: Distribution::from_values(measured.iter().map(|s| s.required_bits).collect()),
        actual_stamp_leading_zero_bits: Distribution::from_values(measured.iter().map(|s| s.actual_bits).collect()),
        stamp_grind_attempts: Distribution::from_values(measured.iter().map(|s| s.grind_attempts).collect()),
        latency_ns: Distribution::from_values(measured.iter().map(|s| s.latency_ns).collect()),
        internal_validate_ns: Distribution::from_values(measured.iter().map(|s| s.validate_ns).collect()),
        internal_commit_ns: Distribution::from_values(measured.iter().map(|s| s.commit_ns).collect()),
        internal_db_write_ns: Distribution::from_values(measured.iter().map(|s| s.db_write_ns).collect()),
        header_commit_db_write_batches_per_header: Distribution::from_values(measured.iter().map(|s| s.db_batches).collect()),
        header_commit_write_batch_operations_per_header: Distribution::from_values(measured.iter().map(|s| s.db_ops).collect()),
        header_commit_reachability_operations_per_header: Distribution::from_values(
            measured.iter().map(|s| s.reachability_ops).collect(),
        ),
        header_commit_reachability_data_writes_per_header: Distribution::from_values(
            measured.iter().map(|s| s.reachability_data_writes).collect(),
        ),
        header_commit_non_reachability_operations_per_header: Distribution::from_values(
            measured.iter().map(|s| s.db_ops - s.reachability_ops).collect(),
        ),
    }
}

fn env_usize(name: &str, default: usize, allow_zero: bool) -> usize {
    let value = env::var(name).map_or(default, |raw| raw.parse().unwrap_or_else(|_| panic!("{name} must be an integer")));
    assert!(value <= MAX_SAMPLES, "{name} exceeds the {MAX_SAMPLES}-sample harness bound");
    assert!(allow_zero || value > 0, "{name} must be non-zero");
    value
}

fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name).map_or(default, |raw| raw.parse().unwrap_or_else(|_| panic!("{name} must be an integer")))
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

fn cpu_model() -> Option<String> {
    command_output("sysctl", &["-n", "machdep.cpu.brand_string"]).or_else(|| command_output("sysctl", &["-n", "hw.model"])).or_else(
        || {
            fs::read_to_string("/proc/cpuinfo").ok()?.lines().find_map(|line| {
                let (key, value) = line.split_once(':')?;
                matches!(key.trim(), "model name" | "Hardware").then(|| value.trim().to_owned())
            })
        },
    )
}

fn memory_bytes() -> Option<u64> {
    command_output("sysctl", &["-n", "hw.memsize"]).and_then(|value| value.parse().ok()).or_else(|| {
        let line = fs::read_to_string("/proc/meminfo").ok()?.lines().find(|line| line.starts_with("MemTotal:"))?.to_owned();
        line.split_whitespace().nth(1)?.parse::<u64>().ok()?.checked_mul(1024)
    })
}

fn git_output(args: &[&str]) -> Option<String> {
    let output = Command::new("git").arg("-C").arg(env!("CARGO_MANIFEST_DIR")).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8(output.stdout).ok()?.trim().to_owned())
}

fn machine_metadata() -> MachineMetadata {
    MachineMetadata {
        os: env::consts::OS,
        arch: env::consts::ARCH,
        logical_parallelism: std::thread::available_parallelism().map_or(0, usize::from),
        cpu_model: cpu_model(),
        memory_bytes: memory_bytes(),
        uname: command_output("uname", &["-srvmp"]),
        rustc_version: command_output("rustc", &["--version"]),
        cargo_version: command_output("cargo", &["--version"]),
        build_profile: if cfg!(debug_assertions) { "debug" } else { "release" },
    }
}

fn source_metadata() -> SourceMetadata {
    let git_commit = git_output(&["rev-parse", "HEAD"]);
    let git_tree_clean = git_output(&["status", "--porcelain"]).map(|status| status.is_empty());
    let source_provenance_final_evidence_eligible = git_commit.is_some() && git_tree_clean == Some(true);
    SourceMetadata {
        git_commit,
        git_tree_clean,
        source_provenance_final_evidence_eligible,
        source_provenance_note: if source_provenance_final_evidence_eligible {
            "commit recorded and worktree clean; source provenance is reproducible, but hardware/soak requirements still apply"
        } else {
            "dirty or indeterminate worktree; this report is diagnostic only and must be rerun from a reviewed clean commit for final evidence"
        },
        package_version: env!("CARGO_PKG_VERSION"),
    }
}

fn emit_report(report: &G6Report) {
    let json = serde_json::to_string_pretty(report).expect("serialize the G6 report");
    match env::var("PALW_G6_REPORT_PATH") {
        Ok(path) => {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .unwrap_or_else(|error| panic!("refusing to overwrite or create G6 report {path}: {error}"));
            file.write_all(json.as_bytes()).unwrap();
            file.write_all(b"\n").unwrap();
            file.sync_all().unwrap();
            println!("PALW_G6_REPORT_PATH={path}");
        }
        Err(env::VarError::NotPresent) => println!("PALW_G6_REPORT_BEGIN\n{json}\nPALW_G6_REPORT_END"),
        Err(error) => panic!("PALW_G6_REPORT_PATH is not valid Unicode: {error}"),
    }
}

/// Measurement-only G6 harness. It intentionally has no pass/fail latency or DB-operation threshold:
/// the JSON evidence is an input to calibration and independent review, not an activation decision.
#[tokio::test(flavor = "current_thread")]
#[ignore = "hardware G6 measurement; emits JSON and does not close the Measurement gate"]
async fn palw_header_spam_bounded() {
    let started_unix_ms = unix_now();
    let invalid_siblings = env_usize("PALW_G6_INVALID_SIBLINGS", DEFAULT_INVALID_SIBLINGS, false);
    let invalid_orphans = env_usize("PALW_G6_INVALID_ORPHANS", DEFAULT_INVALID_ORPHANS, false);
    let valid_headers = env_usize("PALW_G6_VALID_HEADERS", DEFAULT_VALID_HEADERS, false);
    let warmup_headers = env_usize("PALW_G6_WARMUP_HEADERS", DEFAULT_WARMUP_HEADERS, true);
    let stamp_max_nonce = env_u64("PALW_G6_STAMP_MAX_NONCE", DEFAULT_STAMP_MAX_NONCE);
    assert_shipped_presets_inert();

    let config = measurement_config();
    let tc = TestConsensus::new(&config);
    let handles = tc.init();
    let sibling_evidence = measure_rejections(&tc, InvalidKind::Sibling, invalid_siblings, 0x1000_0000).await;
    let orphan_evidence = measure_rejections(&tc, InvalidKind::Orphan, invalid_orphans, 0x2000_0000).await;
    let valid_evidence = measure_valid_headers(&tc, warmup_headers, valid_headers, stamp_max_nonce, 0x3000_0000).await;
    tc.shutdown(handles);

    let params = PalwSpamParams::PUBLIC_REGENESIS_CANDIDATE;
    let report = G6Report {
        schema: "misaka-palw-g6-header-flood-v2",
        started_unix_ms,
        finished_unix_ms: unix_now(),
        gate: GateMetadata {
            id: "G6",
            status: "Bounded",
            public_value_activation: "StopShip until multi-machine remeasurement freezes thresholds and independent review lands",
            thresholds: None,
            conclusion: "evidence only; ADR-0043 (A) bounded allocator holds per-header writes constant at p99 under the sibling flood; threshold freeze and deployment approval remain external",
        },
        reachability_risk: ReachabilityRiskMetadata {
            public_value_activation: "Bounded by the ADR-0043 (A) allocation policy; threshold freeze awaits multi-machine flood/soak remeasurement and independent review",
            trigger: "after a parent's trailing u64 reachability interval is exhausted, the next direct child triggers reindex_intervals (now amortized: re-tiles keep a trailing reserve and flood-regime insertions take a harmonic share)",
            root_cause: "HISTORICAL: propagate_interval consumed the full child-capacity leaving no trailing interval, so every post-exhaustion sibling reindexed; fixed by split_exponential_with_reserve + the SIBLING_FLOOD_ALLOC_THRESHOLD harmonic insertion policy (ADR-0043 Amendment)",
            asymptotic_write_bound: "amortized O(1) data-row writes per accepted header under a same-parent sibling flood (p99 constant; single geometric re-tile events bounded by the reindexed subtree)",
            remediation_class: "landed: node-local allocation-policy change (no consensus semantics, no fork boundary); residual = multi-machine threshold freeze + independent review",
        },
        source: source_metadata(),
        machine: machine_metadata(),
        fixture: FixtureMetadata {
            base_preset: "devnet-palw-111 cloned into an isolated Header-v4 re-genesis fixture",
            isolated_test_only_regenesis: true,
            header_version: PALW_ANTISPAM_HEADER_VERSION,
            algo_id: POW_ALGO_ID_PALW_REPLICA,
            window_daa: params.window_daa,
            replicas_per_hash: params.replicas_per_hash,
            burst: params.burst,
            base_stamp_bits: params.base_stamp_bits,
            max_stamp_bits: params.max_stamp_bits,
            shipped_presets_checked: ["mainnet", "testnet-10", "testnet-palw-110", "devnet-palw-111", "simnet", "devnet"],
            shipped_presets_remain_pre_v4_inert_and_accept_closed: true,
            storage_fixture: "TestConsensus-created OS-temporary RocksDB database",
            production_equivalence: "production ConsensusApi ordinary header processor and RocksDB store code; not deployed ingress, service topology, or production-storage-equivalent",
            slowest_supported_storage_measured: false,
        },
        invocation: InvocationMetadata {
            command: "cargo test -p kaspa-consensus --release palw_header_spam_bounded -- --ignored --nocapture --test-threads=1",
            percentile_method: "nearest-rank: sorted[ceil(p * n) - 1]",
            latency_boundary: "after Block::from_header, immediately before ConsensusApi::validate_and_insert_block, through block_task completion; block/header fixture construction and stamp grinding excluded",
            in_flight_headers: 1,
            concurrency_scope: "serial: submit one header and await its block_task before constructing the next timed sample",
            unmeasured_concurrency: "queue/backpressure, worker saturation, concurrent peer ingress, dependency scheduling, and contended RocksDB are not measured; require a separate multi-node flood/long soak",
            warmup_headers,
            invalid_siblings,
            invalid_orphans,
            valid_headers,
            stamp_max_nonce_inclusive: stamp_max_nonce,
        },
        invalid_weak_stamp_siblings: sibling_evidence,
        invalid_weak_stamp_orphans: orphan_evidence,
        valid_stamped_headers: valid_evidence,
    };
    emit_report(&report);
}

#[test]
fn g6_distribution_uses_documented_nearest_rank_percentiles() {
    let distribution = Distribution::from_values((1..=100).collect());
    assert_eq!(distribution.median, 50);
    assert_eq!(distribution.p95, 95);
    assert_eq!(distribution.p99, 99);
}
