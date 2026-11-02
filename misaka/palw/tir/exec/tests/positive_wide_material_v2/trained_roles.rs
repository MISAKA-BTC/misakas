//! Opt-in local role measurements, not native Assignment/P/credit or activation.
//! The retained original Pythia class/max-context/arithmetic and saved 16/1 job
//! are pinned independently. A short noncanonical job is NOT full-context or
//! canonical native qualification. No alternate weights, zero models or traces.
use super::*;
use kaspa_consensus_core::{
    palw_artifact::artifact_leaf_parts_v1,
    palw_native_resource_v1::require_source_program_local_profile_v1,
    palw_positive_check_v2::full_relation::wide_v2::stream::{StreamRecordLimitsV2, StreamSubmitV2},
    palw_prompt_ids_v1::prompt_token_ids_commitment_v1,
    palw_tir_artifact_v1::PALW_TIR_ROW_PIECE_BYTES_V1,
    palw_tir_class_v1::PalwTirLayoutV1,
    palw_tir_court_v1::PalwTirInventoryIndexV1,
};
use misaka_palw_tir_artifact::bounded_v1::PalwTirCheckedHeaderV1;
use misaka_palw_tir_exec::node::{
    TirByteSourceV1, TirInventoryTreeV1,
    memory::TirMemoryLeaseV1,
    owned_artifact_params_v2::forward::{metadata::*, *},
};
use std::{
    cell::Cell,
    fs::File,
    io::Read,
    os::unix::fs::{FileExt, MetadataExt},
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};
const ARTIFACT: &str = "/Users/wata/Downloads/misaka-trained-llm-cohort-20261031/EleutherAI__pythia-70m/run-wide-q24-v2/model.palwtir.private-wide-q24.palwtir";
const FIXTURE: &str =
    "/Users/wata/Downloads/misaka-trained-llm-cohort-20261031/EleutherAI__pythia-70m/run-wide-q24-v2/registration-fixture.json";
const CLASS: &str =
    "356fbf891ef8ff98cc008c5187c7105bbc9eb32a53225a54361493c96fdc75f3af9d7acb1789a4fa9866771442fa1a55a8217371278bf88130ece5d2e4af91e1";
const ROOT: &str =
    "427a912e722f173d1d0fe4b02dac4148709e9a70d245c3012768a5962fa3f6736ea9cb921b8824b9712a1f718f5dc389c00c1d81fa9c8f552fd358c96ba10ef4";
const TOKENIZER: &str =
    "9ccc3c319505ec74ccccb984a97bbd955c2c3f30e4c8f2dbe95ff8e13b213d84074d705cf61decf466eb2026e4a1b9ebf3e33f32abdc0653382ea0a669941a43";
const CONTEXT: &str =
    "d9ca9eae0abcafa087e4e06ba9fcd8a8f71f0f435735e3302a7725d2465fb9a22eb728f4a7d3075bffcdc6208d791b999e6f1f25574864ac7b41d792379ee868";
const INPUT: &[u32] = &[510, 4096, 273, 436, 3368, 310, 281, 1924, 4217, 13782, 342, 247, 10799, 285, 41374, 10636];
const E: u64 = 1 << 28;
const REQUEST_CAP: u64 = 8 << 30;
const PROVIDER_BYTES: u64 = 64 << 20;
const PROVIDER_CALL_LEAVES: usize = 1024;
const PROVIDER_CALL_READ_BYTES: u64 = 1 << 20;
const PROVIDER_CALL_RETURN_BYTES: u64 = 4 << 20;
// R5 charges the conservative per-call envelope, not actual pread bytes. Its
// finite total derives from the opening-attempt cap. Actual requested file IO
// independently remains capped at 4 GiB in IoBudget, with no retry refund.
const PROVIDER_ENVELOPE_TOTAL: u64 = (1 << 20) * PROVIDER_CALL_READ_BYTES;
const DISK_CAP: u64 = 320 << 20; // Geometry predicts 305,797,608 aggregate tape bytes.
const BASELINE_FAILED_DISK_CAP: u64 = 64 << 20;
const FILE_BYTES: u64 = 103402560;
type R<T> = Result<T, String>;
fn add(a: u64, b: u64) -> R<u64> {
    a.checked_add(b).ok_or_else(|| "measurement overflow".into())
}
fn mul(a: u64, b: u64) -> R<u64> {
    a.checked_mul(b).ok_or_else(|| "measurement overflow".into())
}
fn require(ok: bool, why: &'static str) -> R<()> {
    if ok { Ok(()) } else { Err(why.into()) }
}
fn hash(text: &str) -> Hash64 {
    assert_eq!(text.len(), 128);
    let mut a = [0; 64];
    for (i, pair) in text.as_bytes().chunks_exact(2).enumerate() {
        a[i] = u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap();
    }
    Hash64::from_bytes(a)
}
fn exact<T>(n: usize) -> R<Vec<T>> {
    let mut out = Vec::new();
    out.try_reserve_exact(n).map_err(|_| "exact allocation refused")?;
    require(out.capacity() == n, "allocation capacity exceeds request")?;
    Ok(out)
}
#[derive(Default)]
struct Ledger {
    live: AtomicU64,
    peak: AtomicU64,
    cumulative: AtomicU64,
    panel_runs: AtomicU64,
}
struct Guard(Arc<Ledger>, u64);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.live.fetch_sub(self.1, Ordering::SeqCst);
    }
}
fn ledger(l: Arc<Ledger>) -> TirMemoryReserverV1 {
    Arc::new(move |role, _, _, bytes| {
        let old = l
            .live
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_add(bytes).filter(|n| *n <= REQUEST_CAP))
            .map_err(|_| "logical request ledger refused")?;
        l.peak.fetch_max(old + bytes, Ordering::SeqCst);
        if l.cumulative.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_add(bytes)).is_err() {
            l.live.fetch_sub(bytes, Ordering::SeqCst);
            return Err("cumulative request overflow".into());
        }
        if role == "tir-wide-q24-v2-required" {
            l.panel_runs.fetch_add(1, Ordering::SeqCst);
        }
        Ok(Box::new(Guard(l.clone(), bytes)))
    })
}
struct DiskGuard(Arc<AtomicU64>, u64);
impl Drop for DiskGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(self.1, Ordering::SeqCst);
    }
}
fn disk_reserver(held: Arc<AtomicU64>) -> WidePanelDiskReserverV2 {
    Arc::new(move |_, _, bytes| {
        held.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_add(bytes).filter(|n| *n <= DISK_CAP))
            .map_err(|_| "shared disk capacity refused")?;
        Ok(Box::new(DiskGuard(held.clone(), bytes)))
    })
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Usage {
    attempts: u64,
    calls: u64,
    bytes: u64,
    openings: u64,
}
// Private operator diagnostics only. Both windows request the SAME entire
// original job, arithmetic, coverage and source limits. The extended window
// measures costs past the observed 240-s prefix, not a production profile.
#[derive(Clone, Copy)]
enum MeasurementWindow {
    Standard,
    ExtendedCost,
}
impl MeasurementWindow {
    fn node_limits(self) -> FullRelationLimits {
        let mut l = node_limits();
        if matches!(self, Self::ExtendedCost) {
            // Actual position-9 closure stops at 16,385 elements. Keep the
            // same per-relation 1-GiB workspace owner and every other bound;
            // its conservative quote still has to admit each evaluation.
            l.plan.max_elements_per_relation = 1 << 15;
        }
        l
    }
    fn cpu_seconds(self) -> u64 {
        match self {
            Self::Standard => 240,
            Self::ExtendedCost => 1440,
        }
    }
    fn io_limits(self) -> Usage {
        match self {
            Self::Standard => Usage { attempts: 2, calls: 1 << 22, bytes: 4 << 30, openings: 1 << 20 },
            Self::ExtendedCost => Usage { attempts: 2, calls: 1 << 24, bytes: 8 << 30, openings: 1 << 20 },
        }
    }
}
struct IoBudget {
    used: Cell<Usage>,
    max: Usage,
}
impl IoBudget {
    fn new() -> Self {
        Self::for_window(MeasurementWindow::Standard)
    }
    fn for_window(window: MeasurementWindow) -> Self {
        Self { used: Cell::new(Usage::default()), max: window.io_limits() }
    }
    fn charge(&self, delta: Usage) -> R<()> {
        let a = self.used.get();
        let n = Usage {
            attempts: add(a.attempts, delta.attempts)?,
            calls: add(a.calls, delta.calls)?,
            bytes: add(a.bytes, delta.bytes)?,
            openings: add(a.openings, delta.openings)?,
        };
        require(
            n.attempts <= self.max.attempts
                && n.calls <= self.max.calls
                && n.bytes <= self.max.bytes
                && n.openings <= self.max.openings,
            "finite provider budget",
        )?;
        self.used.set(n);
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stamp {
    dev: u64,
    ino: u64,
    len: u64,
    mtime: i64,
    mtimens: i64,
    ctime: i64,
    ctimens: i64,
}
fn stamp(file: &File) -> R<Stamp> {
    let m = file.metadata().map_err(|_| "file stat")?;
    require(m.is_file(), "regular source required")?;
    Ok(Stamp {
        dev: m.dev(),
        ino: m.ino(),
        len: m.len(),
        mtime: m.mtime(),
        mtimens: m.mtime_nsec(),
        ctime: m.ctime(),
        ctimens: m.ctime_nsec(),
    })
}
struct FileBytes<'a> {
    file: &'a File,
    checked: &'a PalwTirCheckedHeaderV1<'a>,
    budget: &'a IoBudget,
    stamp: Stamp,
}
impl TirByteSourceV1 for FileBytes<'_> {
    fn read_bytes(&self, p: u16, layer: Option<u16>, at: u64, buf: &mut [u8]) -> R<()> {
        require(stamp(self.file)? == self.stamp, "source changed before opening")?;
        let (base, len) = self.checked.locate(p, layer).ok_or("tensor absent")?;
        require(add(at, buf.len() as u64)? <= len, "piece leaves tensor")?;
        require(!buf.is_empty() && buf.len() <= PALW_TIR_ROW_PIECE_BYTES_V1 as usize, "single original-file piece cap")?;
        // One bounded syscall per requested piece. A short read is refused and
        // remains charged; retrying it here could exceed the per-call envelope.
        self.budget.charge(Usage { calls: 1, bytes: buf.len() as u64, ..Usage::default() })?;
        let got = self.file.read_at(buf, add(base, at)?).map_err(|_| "bounded pread refused")?;
        require(got == buf.len(), "short source piece")?;
        require(stamp(self.file)? == self.stamp, "source changed during opening")
    }
}
/// TEST caller's finite original-file provider. Borrowed metadata + tree of
/// ordinary identity hashes only; no full tensor copy or model-owned i128 map.
/// Persistent caller budget survives failed construction and every opening.
struct Provider<'a> {
    source: FileBytes<'a>,
    tree: TirInventoryTreeV1,
    max_name: u64,
    _lease: TirMemoryLeaseV1,
}
impl<'a> Provider<'a> {
    fn build(
        file: &'a File,
        checked: &'a PalwTirCheckedHeaderV1<'a>,
        root: Hash64,
        class: Hash64,
        execution: Hash64,
        budget: &'a IoBudget,
        reserve: &TirMemoryReserverV1,
    ) -> R<Self> {
        budget.charge(Usage { attempts: 1, ..Usage::default() })?;
        let p = checked.program();
        require(p.params.len() <= 1024 && p.schedule.layers.len() <= 128, "provider metadata cardinality")?;
        let max_name = p.params.iter().map(|p| p.name.len() as u64).max().unwrap_or(0);
        require(max_name <= 1024, "provider name cap")?;
        let owner = reserve("measurement-original-file-provider", class, execution, PROVIDER_BYTES)?;
        let index = PalwTirInventoryIndexV1::new(p).ok_or("inventory index")?;
        let n = u64::from(index.leaf_count());
        require(n > 0 && n <= 1 << 18, "finite inventory leaves")?;
        // Two live inventory indices, all tree levels, Vec growth/control and
        // one 1024-byte read. This fixed-class caller retains the entire 64 MiB
        // lease; it is not a generic native quote or a claim of physical RSS.
        let floor =
            add(mul(n, 4 * 64)?, add(mul(p.params.len() as u64, mul(add(p.schedule.layers.len() as u64, 1)?, 1024)?)?, 1 << 20)?)?;
        require(floor <= PROVIDER_BYTES, "whole provider ownership cap")?;
        let before = stamp(file)?;
        require(before.len == checked.file_len(), "source file length")?;
        let source = FileBytes { file, checked, budget, stamp: before };
        let mut leaves = exact(n as usize)?;
        let mut buf = [0u8; PALW_TIR_ROW_PIECE_BYTES_V1 as usize];
        for leaf in 0..index.leaf_count() {
            let (j, layer, at, len) = index.piece_of(leaf).ok_or("piece geometry")?;
            source.read_bytes(j, layer, u64::from(at), &mut buf[..len as usize])?;
            leaves.push(artifact_leaf_parts_v1(&p.params[j as usize].name, layer, at, &buf[..len as usize]));
        }
        let tree = TirInventoryTreeV1::from_leaves(p, leaves)?;
        require(tree.root() == root && stamp(file)? == before, "original artifact root/stamp mismatch")?;
        Ok(Self { source, tree, max_name, _lease: owner })
    }
    fn admit(&self, leaves: &[u32]) -> R<()> {
        self.source.budget.charge(Usage { openings: 1, ..Usage::default() })?;
        require(!leaves.is_empty() && leaves.len() <= PROVIDER_CALL_LEAVES, "finite provider opening cardinality")?;
        require(leaves.iter().all(|i| *i < self.tree.leaf_count()), "opening index")?;
        let output = mul(leaves.len() as u64, add(self.max_name, 1024 + 32 * 64 + 256)?)?;
        require(output <= PROVIDER_CALL_RETURN_BYTES, "provider return envelope")
    }
}
impl TirParamOpenerV1 for Provider<'_> {
    fn param_scratch_storage_v1(&self, leaves: &[u32]) -> Option<u64> {
        // sorted leaf IDs plus current/next u64 level vectors coexist.
        // Returned pieces/siblings have their separate result envelope.
        (leaves.len() <= PROVIDER_CALL_LEAVES).then_some(8192 + leaves.len() as u64 * 32)
    }
    fn param_opening(&self, i: u32) -> Option<PalwArtifactOpeningV1> {
        self.admit(&[i]).ok()?;
        self.tree.open_with(self.source.checked.program(), &self.source, i)
    }
    fn param_multiproof(&self, leaves: &[u32]) -> Option<PalwArtifactMultiproofV1> {
        self.admit(leaves).ok()?;
        self.tree.multiproof_with(self.source.checked.program(), &self.source, leaves)
    }
}
struct Pins<'a> {
    class: &'a PalwTirClassV1,
    artifact: Hash64,
    job: &'a PalwJobContextV2,
}
impl OracleFixture for Pins<'_> {
    fn class(&self) -> &PalwTirClassV1 {
        self.class
    }
    fn artifact(&self) -> Hash64 {
        self.artifact
    }
    fn job(&self) -> &PalwJobContextV2 {
        self.job
    }
}
fn node_limits() -> FullRelationLimits {
    let mut l = full_limits();
    l.plan.max_positions = 16;
    l.plan.max_legacy_leaves = 1 << 18;
    l.plan.max_relations = 1 << 20;
    l.plan.max_geometry_visits = 1 << 28;
    l.plan.max_plan_bytes = 1 << 28;
    l.plan.uncommitted_tile_lanes = 64;
    // These are enforced evaluation ceilings, not promised work. The old
    // generous caps implied a >16-GiB conservative workspace despite a 512-MiB
    // owner and therefore refused even relation zero. Tighter finite per-tile
    // limits keep that workspace inside its real owner; any larger closure
    // still refuses. No relation, arithmetic or original model is changed.
    l.plan.max_elements_per_relation = 1 << 14;
    // The original measured closure at node 262 needs 131,376 terms, including
    // its upstream work: 131,072 covers only the matrix product. This finite
    // ceiling still quotes inside the existing real 1-GiB workspace owner.
    l.plan.max_terms_per_relation = 1 << 18;
    l.plan.max_peak_target_bytes = 16 << 20;
    l.source.max_target_bytes = 16 << 20;
    l.source.max_material_bytes = 16 << 20;
    l.source.max_auth_storage_bytes = 256 << 20;
    // 64 lanes x 2048 reduction entries can request two source lanes per
    // term. Keep those enforced reads and the conservative evaluator backing
    // together inside the actual one-GiB builder owner and global 8-GiB ledger.
    l.source.max_source_reads = 1 << 18;
    l.source.max_workspace_bytes = 1 << 30;
    l.max_record_bytes = 256 << 20;
    l.max_record_heap_bytes = 64 << 20;
    l
}
fn panel_limits() -> WidePanelLimitsV2 {
    WidePanelLimitsV2 {
        max_records: 1 << 16,
        max_step_leaves: 1 << 18,
        max_tape_bytes: DISK_CAP,
        max_live_heap_bytes: 6 << 30,
        max_visits: 1 << 46,
        max_io_bytes: 2 * PROVIDER_ENVELOPE_TOTAL,
        max_hash_work: 1 << 46,
        max_proof_attempts: 1 << 20,
        max_provider_bytes: PROVIDER_ENVELOPE_TOTAL,
        max_provider_scratch: 1 << 20,
        max_material_bytes: 16 << 20,
        max_claim_bytes: 16 << 20,
        max_trace_frames: 1 << 18,
        external_retained_bytes: E + PROVIDER_BYTES,
    }
}
fn record_limits(external: u64) -> StreamRecordLimitsV2 {
    StreamRecordLimitsV2 {
        max_record_bytes: 256 << 20,
        max_claim_values: 1 << 20,
        max_read_bytes: 4 << 30,
        max_read_calls: 1 << 24,
        max_extra_heap_bytes: REQUEST_CAP,
        metadata_auth_bytes: 512 << 20,
        signature_workspace_bytes: 1 << 20,
        external_retained_bytes: external,
    }
}
/// Exact local scope only: all actual parents remain alive in run_loaded.
/// The eight-GiB bound still covers existing + introduced requests together.
/// This does not authenticate native funding or make an external DTO a lease.
fn reserve_stream_increment(
    l: &Ledger,
    reserve: &TirMemoryReserverV1,
    existing: u64,
    whole: u64,
    role: &'static str,
) -> Result<TirMemoryLeaseV1, FullRelationError> {
    if l.live.load(Ordering::SeqCst) < existing || whole > REQUEST_CAP {
        return Err(FullRelationError::Budget);
    }
    let new = whole.checked_sub(existing).filter(|n| *n > 0).ok_or(FullRelationError::Budget)?;
    reserve(role, hash(CLASS), hash(CONTEXT), new).map_err(|_| FullRelationError::Budget)
}

fn report(phase: &str, start: Instant, l: &Ledger, extra: serde_json::Value) {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    let status = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    assert_eq!(status, 0);
    let u = unsafe { usage.assume_init() };
    println!(
        "ROLE_MEASURE {}",
        serde_json::json!({"phase":phase,"wall_ns":start.elapsed().as_nanos(),"requested_live":l.live.load(Ordering::SeqCst),
        "requested_peak":l.peak.load(Ordering::SeqCst),"requested_cumulative":l.cumulative.load(Ordering::SeqCst),
        "process_maxrss_native_units":u.ru_maxrss,"node_process_isolated":false,"process_user_seconds":u.ru_utime.tv_sec as f64+u.ru_utime.tv_usec as f64/1e6,
        "process_system_seconds":u.ru_stime.tv_sec as f64+u.ru_stime.tv_usec as f64/1e6,"physical_rss_hard_quota":false,
        "native_authority":false,"canonical_job":false,"full_context_execution":false,"detail":extra})
    );
}
fn cpu_limit(window: MeasurementWindow) {
    let value = window.cpu_seconds() as libc::rlim_t;
    assert!(value < libc::RLIM_INFINITY);
    let limit = libc::rlimit { rlim_cur: value, rlim_max: value };
    assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_CPU, &limit) }, 0);
    let file = libc::rlimit { rlim_cur: DISK_CAP as libc::rlim_t, rlim_max: DISK_CAP as libc::rlim_t };
    assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_FSIZE, &file) }, 0);
}

fn run_loaded(
    file: &mut File,
    inputs: LocalForwardInputsV2<'_>,
    l: &Ledger,
    reserve: &TirMemoryReserverV1,
    window: MeasurementWindow,
) -> R<()> {
    let job = inputs.original_context;
    let start = Instant::now();
    let mut ll = super::trace_free_bridge_tests::load_limits();
    ll.params.max_tensor_bytes = 256 << 20;
    ll.params.max_payload_bytes = 256 << 20;
    ll.params.max_native_bytes = 256 << 20;
    ll.params.max_metadata_bytes = 64 << 20;
    ll.params.max_inventory_leaves = 1 << 18;
    ll.params.max_total_inventory_leaves = 1 << 18;
    ll.params.max_total_read_bytes = FILE_BYTES;
    ll.params.max_work_per_attempt = 1 << 46;
    ll.params.max_total_work = 1 << 46;
    ll.max_new_owner_bytes = 256 << 20;
    ll.max_retained_requests = 256 << 20;
    ll.max_provider_calls_per_attempt = 1 << 22;
    ll.max_total_provider_calls = 1 << 22;
    let mut lb = LocalForwardLoadBudgetV2::new(ll).map_err(|_| "load budget")?;
    let loaded = load_local_wide_forward_v2(file, inputs, &mut lb, reserve).map_err(|_| "owned original weights")?;
    report("original-owned-weights", start, l, serde_json::json!({"read_bytes":lb.usage().params.read_bytes}));
    let start = Instant::now();
    let ordinary_limits = LocalWideForwardLimitsV2 {
        max_positions: 2048,
        max_generated: 1,
        max_logits_lanes: 65536,
        max_work_per_attempt: 1 << 46,
        max_requested_bytes: 9 << 29,
        max_operation_bytes: REQUEST_CAP,
        max_attempts: 1,
        max_total_work: 1 << 46,
        max_retained_requests: 9 << 29,
        external_retained_bytes: loaded.load_quote().operation_retained_bytes,
    };
    let q = loaded.forward_quote(ordinary_limits).map_err(|e| format!("ordinary preflight {e:?}"))?;
    report(
        "ordinary-preflight",
        start,
        l,
        serde_json::json!({"requested_bytes":q.requested_bytes,"work":q.work,"handoff_peak":q.handoff_peak_requests().map_err(|_| "handoff quote")?}),
    );
    let mut rb = LocalWideForwardBudgetV2::new(ordinary_limits).map_err(|_| "ordinary budget")?;
    let start = Instant::now();
    let ordinary = loaded
        .forward_local(INPUT, &mut rb, reserve)
        .map_err(|e| format!("ordinary Miner inference {e:?}; budget={:?}", rb.usage()))?;
    require(l.panel_runs.load(Ordering::SeqCst) == 0, "Miner used witness runner")?;
    let signed = SignedMiner::for_output(&Pins { class: inputs.class, artifact: hash(ROOT), job }, digest(ordinary.output_root()));
    let generated = ordinary.generated().to_vec();
    report(
        "ordinary-miner",
        start,
        l,
        serde_json::json!({"generated":generated,"output":format!("{}",ordinary.output_root()),"witness_runs":0}),
    );
    drop(ordinary);
    let mut panel_bounds = panel_limits();
    let planned_relations = match window {
        MeasurementWindow::Standard => None,
        MeasurementWindow::ExtendedCost => {
            // Metadata-only geometry, under the still-live bootstrap owner.
            // Keep every original charge: the producer prepays 2*material cap
            // per relation, not actual file traffic. The previous 2-TiB IO
            // allowance exhausted that distinct copy counter at relation 65,529.
            let started = Instant::now();
            let geometry = kaspa_consensus_core::palw_positive_check_v2::expanded_plan::wide_v2::ExpandedPlanV2::derive(
                inputs.class,
                hash(ROOT),
                job,
                window.node_limits().plan,
            )
            .map_err(|e| format!("extended copy geometry {e:?}"))?;
            let relations = geometry.summary().relations;
            let material_copy = mul(relations, mul(panel_bounds.max_material_bytes, 2)?)?;
            panel_bounds.max_io_bytes = add(panel_bounds.max_io_bytes, material_copy)?;
            report(
                "cumulative-copy-preflight",
                started,
                l,
                serde_json::json!({"relations":relations,"per_relation_material_copy":2*panel_bounds.max_material_bytes,
                "material_copy_allowance":material_copy,"existing_io_and_copy_headroom":panel_limits().max_io_bytes,
                "cumulative_io_and_copy_cap":panel_bounds.max_io_bytes,"actual_provider_read_cap":window.io_limits().bytes,
                "actual_file_traffic":false,"diagnostic_only":true}),
            );
            Some(relations)
        }
    };
    let io = IoBudget::for_window(window);
    let start = Instant::now();
    let provider = Provider::build(file, inputs.checked, hash(ROOT), hash(CLASS), hash(CONTEXT), &io, reserve)?;
    report("original-source-index", start, l, serde_json::json!({"read_bytes":io.used.get().bytes,"read_calls":io.used.get().calls}));
    let start = Instant::now();
    let disk_held = Arc::new(AtomicU64::new(0));
    let disk = disk_reserver(disk_held.clone());
    let work = loaded.wide_panel_work(panel_bounds, &disk).map_err(|_| "Panel work")?;
    let capture = loaded
        .capture_wide_panel_one_pass(
            INPUT,
            TirWideRunLimitsV2 {
                max_positions: 2048,
                max_step_leaves: 1 << 18,
                max_row_lanes: 65536,
                max_work: 1 << 46,
                max_requested_bytes: 6 << 30,
            },
            &work,
            tempfile::tempfile().map_err(|_| "capture tape")?,
            reserve,
        )
        .map_err(|e| {
            format!(
                "Panel one-pass capture {e:?}; meter={:?}; runs={}; requested_live={}",
                work.meter(),
                l.panel_runs.load(Ordering::SeqCst),
                l.live.load(Ordering::SeqCst)
            )
        })?;
    require(capture.run().generated() == generated && l.panel_runs.load(Ordering::SeqCst) == 1, "single Panel run output")?;
    report(
        "one-pass-panel-capture",
        start,
        l,
        serde_json::json!({"leaves":capture.run().leaf_count(),"held_run_requests":capture.run().reserved_bytes(),"tape_bytes":work.meter().tape_bytes}),
    );
    let start = Instant::now();
    let prep = FullRelationPreparationV2::authenticate(
        &signed.rooted,
        inputs.class,
        hash(ROOT),
        job,
        PalwPromptIdsFormV1::Flat,
        window.node_limits(),
        domains(),
        signed.result.clone(),
        &signed.key,
        &signed.signature,
    )
    .map_err(|e| format!("Node preparation {e:?}"))?;
    if let Some(expected) = planned_relations {
        require(prep.plan().summary().relations == expected, "copy quote does not match actual independent preparation")?;
    }
    let authenticated = signed.authenticate();
    let mut own = owners();
    own.metadata_auth_bytes = 1 << 30;
    own.metadata_auth_work = 1 << 24;
    own.source_auth_bytes = 256 << 20;
    own.retained_record_bytes = 64 << 20;
    own.material_limits = prep.source_limits();
    let builder = WidePanelBuilderV2::prepare(
        &capture,
        &prep,
        &authenticated,
        &signed.rooted,
        &provider,
        WideProviderEnvelopeV2 {
            per_call_io: PROVIDER_CALL_READ_BYTES,
            per_call_work: 1 << 24,
            max_result_bytes: PROVIDER_CALL_RETURN_BYTES,
        },
        own,
        reserve,
    )?;
    let mut tape = builder.claims_tape(tempfile::tempfile().map_err(|_| "claims tape")?)?;
    let panel = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([97; 32]);
    // This isolated caller keeps actual loaded/provider/capture/builder owners
    // alive through both record and checker use. The Core quote includes those
    // existing owners in its WHOLE operation bound. Reserve only its introduced
    // increment; do not count the same live leases twice. No peer scalar or
    // native authority is accepted here, and no owned result escapes this scope.
    let record_external = l.live.load(Ordering::SeqCst);
    let record_acquire = |whole| reserve_stream_increment(l, reserve, record_external, whole, "measurement-record-increment");
    let record = prep
        .freeze_stream_record(
            capture.binding()?,
            generated,
            principal(panel.verification_key.as_ref()),
            &mut tape,
            record_limits(record_external),
            &record_acquire,
        )
        .map_err(|e| format!("record admission {e:?}"))?;
    let sig = *libcrux_ml_dsa::ml_dsa_87::sign(&panel.signing_key, &record.signing_digest().0, record.signature_context(), [0; 32])
        .map_err(|_| "Panel signature")?
        .as_ref();
    report(
        "panel-record",
        start,
        l,
        serde_json::json!({"relations":prep.plan().summary().relations,"provider_bytes":io.used.get().bytes,"tape_bytes":work.meter().tape_bytes}),
    );
    tape.rewind();
    let start = Instant::now();
    let node_external = l.live.load(Ordering::SeqCst);
    let node_reserve = |whole| reserve_stream_increment(l, reserve, node_external, whole, "measurement-node-increment");
    let fresh = FullRelationPreparationV2::authenticate(
        &signed.rooted,
        inputs.class,
        hash(ROOT),
        job,
        PalwPromptIdsFormV1::Flat,
        window.node_limits(),
        domains(),
        signed.result.clone(),
        &signed.key,
        &signed.signature,
    )
    .map_err(|e| format!("fresh Node preparation {e:?}"))?;
    let mut node = fresh
        .begin_stream(
            record.binding().clone(),
            record.generated().to_vec(),
            record.author(),
            record.signing_digest(),
            tape,
            *panel.verification_key.as_ref(),
            sig,
            record_limits(node_external),
            &node_reserve,
        )
        .map_err(|e| format!("Node begin {e:?}"))?;
    let relations = prep.plan().summary().relations;
    report(
        "node-started",
        start,
        l,
        serde_json::json!({"required_relations":relations,"file_io_hard_cap":io.max.bytes,"conservative_envelope_cap":PROVIDER_ENVELOPE_TOTAL}),
    );
    let mut frontier = 0;
    let mut producer_material_ns = 0u128;
    let mut node_check_ns = 0u128;
    let mut observed_evaluations = 0u64;
    let mut peak_elements = 0u64;
    let mut peak_terms = 0u64;
    let mut peak_reads = 0u64;
    let mut peak_workspace = 0u64;
    let mut total_elements = 0u64;
    let mut total_terms = 0u64;
    let mut total_source_reads = 0u64;
    for i in 0..relations {
        let relation = prep.plan().relation_at(i).ok_or("relation omitted")?;
        let material_started = Instant::now();
        let material = builder.material(i, frontier).map_err(|e| {
            format!(
                "material {i}/{relations}, checked={}, file_usage={:?}, work={:?}: {e}",
                node.checked_relations(),
                io.used.get(),
                work.meter()
            )
        })?;
        producer_material_ns += material_started.elapsed().as_nanos();
        let node_started = Instant::now();
        let mut diagnostics = kaspa_consensus_core::palw_tir_court_v1::source_only::wide_v2::EvaluationDiagnosticsV2::default();
        let result = node.submit_with_diagnostics(&material.as_borrowed(), material.target(), &mut diagnostics).map_err(|e| {
            report(
                "node-refused",
                start,
                l,
                serde_json::json!({"checked_relations":node.checked_relations(),"required_relations":relations,
                "source_reads":diagnostics.source_reads,"workspace_bytes":diagnostics.workspace_bytes,
                "computed_elements":diagnostics.work.map(|w|w.elements),"reduction_terms":diagnostics.work.map(|w|w.terms),
                "diagnostic_stage":format!("{:?}",diagnostics.stage),"producer_material_ns":producer_material_ns,
                "previous_successful_node_ns":node_check_ns,"failed_node_ns":node_started.elapsed().as_nanos(),
                "completed_elements":total_elements,"completed_terms":total_terms,"completed_source_reads":total_source_reads,
                "per_relation_elements_cap":window.node_limits().plan.max_elements_per_relation,
                "per_relation_terms_cap":window.node_limits().plan.max_terms_per_relation,"whole_record_authenticated":false}),
            );
            format!(
                "Node relation {i}/{relations}: {e:?}; kind={:?}; material={:?}; file_usage={:?}; work={:?}; evaluation={diagnostics:?}",
                relation.kind,
                material.as_borrowed().preflight(prep.source_limits()),
                io.used.get(),
                work.meter()
            )
        })?;
        node_check_ns += node_started.elapsed().as_nanos();
        require(
            result == if i + 1 == relations { StreamSubmitV2::NeedEof } else { StreamSubmitV2::Continue },
            "Node stream completion order",
        )?;
        let cost = diagnostics.work.ok_or("successful evaluation missing work observation")?;
        observed_evaluations += 1;
        peak_elements = peak_elements.max(cost.elements);
        peak_terms = peak_terms.max(cost.terms);
        total_elements = add(total_elements, cost.elements)?;
        total_terms = add(total_terms, cost.terms)?;
        total_source_reads = add(total_source_reads, diagnostics.source_reads)?;
        peak_reads = peak_reads.max(diagnostics.source_reads);
        if let Some(bytes) = diagnostics.workspace_bytes {
            peak_workspace = peak_workspace.max(bytes);
        }
        if matches!(relation.kind, ExpandedRelationKind::Legacy(_)) {
            frontier += 1;
        }
        if (i + 1) % 1024 == 0 {
            report(
                "node-progress",
                start,
                l,
                serde_json::json!({"checked_relations":node.checked_relations(),"required_relations":relations,"file_usage":format!("{:?}",io.used.get()),"producer_material_ns":producer_material_ns,"node_check_ns":node_check_ns,"observed_evaluations":observed_evaluations,
                "peak_elements":peak_elements,"peak_terms":peak_terms,"peak_reads":peak_reads,"peak_workspace_bytes":peak_workspace,
                "completed_elements":total_elements,"completed_terms":total_terms,"completed_source_reads":total_source_reads}),
            );
        }
    }
    require(observed_evaluations == relations, "incomplete work observations")?;
    let authentication_started = Instant::now();
    let verdict = node.finish().map_err(|e| format!("Node EOF {e:?}"))?;
    let record_auth_finish_ns = authentication_started.elapsed().as_nanos();
    require(verdict == CheckOutcome::Matched, "Node result")?;
    let record_reads = node.read_meter();
    require(l.panel_runs.load(Ordering::SeqCst) == 1, "extra executor run")?;
    report(
        "complete-node",
        start,
        l,
        serde_json::json!({"checked_relations":relations,"provider_bytes":io.used.get().bytes,"provider_calls":io.used.get().calls,"opening_attempts":io.used.get().openings,"model_runs":0,"producer_material_ns":producer_material_ns,"node_check_ns":node_check_ns,"producer_retained":true,"verdict":"Matched",
        "observed_evaluations":observed_evaluations,"completed_elements":total_elements,"completed_terms":total_terms,"completed_source_reads":total_source_reads,
        "peak_elements":peak_elements,"peak_terms":peak_terms,"peak_reads":peak_reads,"peak_workspace_bytes":peak_workspace,
        "record_auth_finish_ns":record_auth_finish_ns,"claim_tape_read_bytes":record_reads.read_bytes,"claim_tape_read_calls":record_reads.read_calls,"claim_relations_read":record_reads.relations_read}),
    );
    Ok(())
}

// Geometry-only storage projection of the EXISTING capture and Borsh claims
// encodings. No transcript, model execution, accepted record or credit is made.
fn quote_storage(inputs: LocalForwardInputsV2<'_>, l: &Ledger) -> R<()> {
    use misaka_palw_tir::{Dim, Prim, demand::history_length_v1};
    let start = Instant::now();
    let space = inputs.space.wide_space().ok_or("wide storage profile")?;
    let job = inputs.original_context;
    let shape = space.job_shape(job).map_err(|_| "storage job shape")?;
    let mut records = 0u64;
    let mut node_bytes = 0u64;
    let mut node_target_lanes = 0u64;
    let mut largest = (0u64, 0u32, 0usize, 0usize);
    for position in 0..shape.positions {
        for (occ, (block, _)) in space.occurrences().iter().enumerate() {
            if occ + 1 == space.occurrences().len() && !shape.runs_post(position) {
                continue;
            }
            let h = history_length_v1(space.info(), *block, position).ok_or("storage history shape")?;
            for (index, node) in space.program().blocks[*block as usize].nodes.iter().enumerate() {
                let lanes = node.out.shape.iter().try_fold(1u64, |n, d| {
                    mul(
                        n,
                        match d {
                            Dim::Fixed(v) => u64::from(*v),
                            Dim::H => u64::try_from(h).map_err(|_| "history native width")?,
                        },
                    )
                })?;
                node_target_lanes = add(node_target_lanes, lanes)?;
                records = add(records, 1)?;
                let stored = if let Prim::HistAppend { state } = node.prim {
                    records = add(records, 1)?;
                    let row = space.program().states[state as usize].shape.iter().try_fold(1u64, |n, d| mul(n, u64::from(*d)))?;
                    mul(row, node.out.dtype.width() as u64)?
                } else if node.commit {
                    0
                } else {
                    mul(lanes, node.out.dtype.width() as u64)?
                };
                if stored > largest.0 {
                    largest = (stored, position, occ, index);
                }
                node_bytes = add(node_bytes, stored)?;
            }
        }
        for fixed in space.fixed_instances() {
            records = add(records, 1)?;
            node_bytes = add(node_bytes, mul(fixed.elements, fixed.dtype.width() as u64)?)?;
        }
    }
    let count = space.leaf_count_capped(job, 1 << 18).map_err(|_| "storage leaf count")?;
    let mut leaf_bytes = 0u64;
    for i in 0..count {
        let leaf = space.leaf_at(job, i).ok_or("storage leaf geometry")?;
        let width = if leaf.dtype == misaka_palw_tir::DType::I64 { 8 } else { 4 };
        leaf_bytes = add(leaf_bytes, mul(u64::from(leaf.value_count), width)?)?;
    }
    let raw_capture = add(node_bytes, leaf_bytes)?;
    // Every scheduled node's output appears in the full relation claims. This
    // lower bound omits outer/per-relation Borsh lengths and extra checkpoint
    // state/history relations; it cannot overstate their required payload.
    let claims_lower = mul(node_target_lanes, 16)?;
    let candidate = SignedMiner::for_output(
        &Pins { class: inputs.class, artifact: inputs.artifact, job },
        digest(palw_attempt_output_root_v1(job, &[273])),
    );
    let plan = FullRelationPreparationV2::authenticate(
        &candidate.rooted,
        inputs.class,
        inputs.artifact,
        job,
        PalwPromptIdsFormV1::Flat,
        node_limits(),
        domains(),
        candidate.result.clone(),
        &candidate.key,
        &candidate.signature,
    );
    let plan_status = match plan {
        Ok(prep) => {
            serde_json::json!({"status":"constructed-within-local-limits","relations":prep.plan().summary().relations,"target_lanes":prep.plan().summary().target_lanes})
        }
        Err(e) => serde_json::json!({"status":"refused-by-current-local-limits","error":format!("{e:?}")}),
    };
    report(
        "storage-only-preflight",
        start,
        l,
        serde_json::json!({"model_executed":false,"capture_records":records,
        "node_and_fixed_native_tape_bytes":node_bytes,"mixed_leaf_tape_bytes":leaf_bytes,"raw_capture_bytes":raw_capture,
        "node_target_lanes":node_target_lanes,"canonical_i128_claim_bytes_lower_bound":claims_lower,
        "configured_shared_tape_cap":DISK_CAP,"baseline_failed_shared_tape_cap":BASELINE_FAILED_DISK_CAP,"fits_capture_alone":raw_capture<=DISK_CAP,
        "fits_capture_and_claims_lower_bound":add(raw_capture,claims_lower)?<=DISK_CAP,
        "largest_native_record":{"bytes":largest.0,"position":largest.1,"occurrence":largest.2,"node":largest.3},"node_plan":plan_status}),
    );
    require(raw_capture > BASELINE_FAILED_DISK_CAP, "original measured capture refusal no longer explained by geometry")?;
    Ok(())
}

fn run_trained(
    l: &Ledger,
    reserve: &TirMemoryReserverV1,
    early: &(dyn Send + Sync),
    storage_only: bool,
    window: MeasurementWindow,
) -> R<()> {
    let fixture = File::open(FIXTURE).map_err(|_| "fixture open")?;
    let bytes = fixture.metadata().map_err(|_| "fixture stat")?.len();
    require(bytes <= 1 << 20, "fixture byte cap before JSON")?;
    let mut frame = exact(bytes as usize)?;
    fixture.take(bytes + 1).read_to_end(&mut frame).map_err(|_| "fixture read")?;
    require(frame.len() as u64 == bytes, "fixture size changed")?;
    let fixture: serde_json::Value = serde_json::from_slice(&frame).map_err(|_| "fixture JSON")?;
    let hex = fixture["layout_borsh_hex"].as_str().ok_or("layout missing")?;
    require(hex.len() <= 1 << 20 && hex.len().is_multiple_of(2), "layout cap")?;
    let mut layout_bytes = exact(hex.len() / 2)?;
    for pair in hex.as_bytes().chunks_exact(2) {
        layout_bytes.push(u8::from_str_radix(std::str::from_utf8(pair).map_err(|_| "layout hex")?, 16).map_err(|_| "layout hex")?);
    }
    let layout: PalwTirLayoutV1 = borsh::from_slice(&layout_bytes).map_err(|_| "original layout")?;
    require(layout.max_context == 2048, "original max context")?;
    let zero = Hash64::default();
    let job = PalwJobContextV2 {
        version: 2,
        network_id: b"a7-local-self-consistency".to_vec(),
        job_id: Hash64::from_u64_word(7),
        job_nullifier: zero,
        assignment_id: zero,
        execution_seed: [7; 32],
        model_profile_id: zero,
        runtime_manifest_hash: zero,
        runtime_class_id: zero,
        shape_profile_id: hash(CLASS),
        trace_scheme_id: kaspa_consensus_core::palw_logits_q24_v2::scheme_id(),
        cu_ruleset_id: zero,
        tokenizer_id: zero,
        prompt_token_ids_hash: prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, INPUT).map_err(|_| "prompt")?,
        declared_prefill_tokens: 16,
        exact_decode_tokens: 1,
        max_context_tokens: 2048,
    };
    require(job.context_hash() == hash(CONTEXT), "original job context")?;
    let inputs = MetadataOwnerInputsV2 {
        expected_class: hash(CLASS),
        artifact: hash(ROOT),
        registered_tokenizer: hash(TOKENIZER),
        registered_max_context: 2048,
        original_context: &job,
        expected_execution: hash(CONTEXT),
        expected_layout: Some(&layout),
        external_retained_bytes: E,
        external_ticket: early,
    };
    let mut ml = super::trace_free_bridge_tests::metadata_limits();
    ml.header.max_file_bytes = FILE_BYTES;
    ml.header.max_header_bytes = 1 << 20;
    ml.header.max_tensors = 512; // Original retained header has 329 entries; local parser ceiling only.
    ml.header.max_native_bytes = 64 << 20;
    ml.header.max_copy_bytes = 64 << 20;
    ml.source.max_native_requests = 128 << 20;
    ml.source.max_scan_steps = 1 << 46;
    ml.source.max_nf_items = 1 << 30;
    ml.compile.max_quote_visits = 1 << 46;
    ml.max_total_scan_steps = 1 << 46;
    ml.max_total_work = 1 << 46;
    ml.header.max_work = 1 << 46;
    ml.max_new_owner_bytes = 1 << 30;
    ml.max_retained_requests = 1 << 30;
    ml.max_operation_bytes = REQUEST_CAP;
    let file = File::open(ARTIFACT).map_err(|_| "artifact open")?;
    require(stamp(&file)?.len == FILE_BYTES, "original artifact bytes")?;
    let start = Instant::now();
    let mut mb = MetadataOwnerBudgetV2::new(ml).map_err(|_| "metadata budget")?;
    let mut owner = load_local_forward_metadata_v2(file, inputs, &mut mb, reserve)
        .map_err(|e| format!("trace-free metadata {e:?}; budget={:?}", mb.usage()))?;
    report(
        "trace-free-metadata",
        start,
        l,
        serde_json::json!({"model":"EleutherAI/pythia-70m","revision":"a39f36b100fe8a5377810d56c3f4789b9c53ac42","original_class":CLASS,"original_artifact":ROOT,"context":CONTEXT}),
    );
    owner
        .with_inputs(
            &mut mb,
            |file, inputs| {
                if storage_only { quote_storage(inputs, l) } else { run_loaded(file, inputs, l, reserve, window) }
            },
        )
        .map_err(|_| "metadata changed")??;
    Ok(())
}

#[test]
#[ignore = "explicit retained Pythia files, isolated CPU/disk/request limits and outer process monitor required"]
fn pythia70m_original_short_job_ordinary_panel_and_complete_node() {
    run_mode(false, MeasurementWindow::Standard);
}

#[test]
#[ignore = "explicit extended diagnostic opt-in; exact isolated test and finite CPU/disk/request/outer limits required"]
fn pythia70m_original_short_job_extended_cost_measurement() {
    assert_eq!(std::env::var("MISAKA_TRAINED_COST_WINDOW").as_deref(), Ok("1440s-8gib-16mi-calls"));
    run_mode(false, MeasurementWindow::ExtendedCost);
}

#[test]
#[ignore = "requires fixed retained Pythia identity metadata and isolated resource/outer process controls; no model execution"]
fn pythia70m_original_short_job_storage_quote() {
    run_mode(true, MeasurementWindow::Standard);
}

fn run_mode(storage_only: bool, window: MeasurementWindow) {
    assert_eq!(std::env::var("MISAKA_RUN_TRAINED_ROLES").as_deref(), Ok("pythia70m-original-16-1"));
    cpu_limit(window);
    require_source_program_local_profile_v1().expect("audited Source profile required");
    let l = Arc::new(Ledger::default());
    let reserve = ledger(l.clone());
    let early = reserve("measurement-bootstrap-and-export-owners", hash(CLASS), hash(CONTEXT), E).unwrap();
    let io = window.io_limits();
    let node = window.node_limits();
    report(
        "measurement-window",
        Instant::now(),
        &l,
        serde_json::json!({"cpu_seconds":window.cpu_seconds(),"actual_provider_max_bytes":io.bytes,
        "actual_provider_max_calls":io.calls,"max_openings":io.openings,"max_attempts":io.attempts,
        "per_relation_elements":node.plan.max_elements_per_relation,"per_relation_terms":node.plan.max_terms_per_relation,
        "per_relation_source_reads":node.source.max_source_reads,"per_relation_workspace_cap":node.source.max_workspace_bytes,
        "memo_entry_envelope":kaspa_consensus_core::palw_native_resource_v1::btree_node_envelope_bytes_v1::<usize,i128>(),
        "logical_request_cap":REQUEST_CAP,"cumulative_tape_cap":DISK_CAP,"diagnostic_only":true}),
    );
    let result = run_trained(&l, &reserve, &early, storage_only, window);
    if let Err(e) = result {
        println!("ROLE_MEASURE_REFUSED {e}");
        panic!("bounded actual trained role measurement refused: {e}");
    }
    require(l.live.load(Ordering::SeqCst) == E, "role owners leaked").unwrap();
    drop(early);
    assert_eq!(l.live.load(Ordering::SeqCst), 0);
}

fn with_provider_file(body: impl FnOnce(&File, &PalwTirCheckedHeaderV1<'_>, &Fixture<'_>)) {
    run_wide_common::with_fixture(1, |f, _| {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("bounded-file-provider.palwtir");
        misaka_palw_tir_artifact::write_container_v1(
            &path,
            f.program,
            borsh::to_vec(&f.class.layout).unwrap(),
            f.class.tokenizer_id.as_bytes(),
            "local-file-provider-test".into(),
            &mut |p, l| Ok(f.params.get(p, l).unwrap().to_le_bytes()),
        )
        .unwrap();
        let mut file = std::fs::OpenOptions::new().read(true).write(true).open(path).unwrap();
        let bytes = file.metadata().unwrap().len();
        let limits = super::trace_free_bridge_tests::metadata_limits().header;
        let header = misaka_palw_tir_artifact::bounded_v1::read_header_bounded_v1(&mut file, bytes, limits).unwrap();
        let checked = header.validate_with_program(f.program, limits).unwrap();
        body(&file, &checked, f);
    });
}

#[test]
fn finite_original_file_provider_authenticates_pieces_and_refuses_changed_files() {
    with_provider_file(|file, checked, f| {
        let l = Arc::new(Ledger::default());
        let reserve = ledger(l.clone());
        let budget = IoBudget::new();
        let provider =
            Provider::build(file, checked, f.artifact, f.job.shape_profile_id, f.job.context_hash(), &budget, &reserve).unwrap();
        assert_eq!(l.live.load(Ordering::SeqCst), PROVIDER_BYTES);
        let scratch = provider.param_scratch_storage_v1(&[0; PROVIDER_CALL_LEAVES]).unwrap();
        assert!(scratch >= PROVIDER_CALL_LEAVES as u64 * (4 + 8 + 8) + 4096);
        assert!(scratch <= panel_limits().max_provider_scratch);
        assert_eq!(provider.param_scratch_storage_v1(&[0; PROVIDER_CALL_LEAVES + 1]), None);
        let before = budget.used.get();
        assert!(provider.param_multiproof(&[0, 0]).is_none());
        assert_eq!(budget.used.get().openings, before.openings + 1);
        assert_eq!(budget.used.get().calls, before.calls);
        let opening = provider.param_opening(0).unwrap();
        kaspa_consensus_core::palw_artifact::verify_artifact_opening_v1(&opening, f.artifact).unwrap();
        // Real proof plus the enforced read-cache allowance must fit its owner
        // before expensive trained execution. The old 64-MiB declaration could
        // not even cover this read cap with an otherwise tiny valid material.
        let proof = provider.param_carriage(&[0]).unwrap();
        let empty = kaspa_consensus_core::palw_step_refute::PalwStepInputRowV1 { preimages: vec![], run_siblings: vec![] };
        let material = kaspa_consensus_core::palw_tir_court_v1::source_only::wide_v2::TirSourceOnlyMaterialV2 {
            operands: &empty,
            params: &proof,
            prompt_token_ids: &[],
            prompt_ids_openings: &[],
            decode_tokens: None,
        };
        let limits = node_limits().source;
        let quote = material.preflight(limits).unwrap();
        assert!(quote.logical_auth_storage_bytes > 64 << 20);
        let mut insufficient = limits;
        insufficient.max_auth_storage_bytes = 64 << 20;
        assert!(matches!(
            material.preflight(insufficient),
            Err(kaspa_consensus_core::palw_tir_court_v1::source_only::TirSourceOnlyErrorV1::Budget)
        ));
        let mut oversized = [0u8; PALW_TIR_ROW_PIECE_BYTES_V1 as usize + 1];
        let io_before = budget.used.get();
        assert!(provider.source.read_bytes(0, None, 0, &mut oversized).is_err());
        assert_eq!(budget.used.get(), io_before);
        assert_eq!(budget.used.get().openings, before.openings + 3);
        let (base, _) = checked.locate(0, None).unwrap();
        file.write_at(&[123], base).unwrap();
        let before = budget.used.get();
        assert!(provider.param_opening(0).is_none());
        assert_eq!(budget.used.get().openings, before.openings + 1);
        assert_eq!(budget.used.get().calls, before.calls, "changed file refused before pread");
        drop(provider);
        assert_eq!(l.live.load(Ordering::SeqCst), 0);
    });
}

#[test]
fn provider_guards_io_and_failed_root_attempts_never_refund_the_caller_budget() {
    with_provider_file(|file, checked, f| {
        let deny: TirMemoryReserverV1 = Arc::new(|_, _, _, _| Err("provider RAM refused".into()));
        let mut budget = IoBudget::new();
        assert!(Provider::build(file, checked, f.artifact, f.job.shape_profile_id, f.job.context_hash(), &budget, &deny).is_err());
        assert_eq!(budget.used.get(), Usage { attempts: 1, ..Usage::default() });
        let l = Arc::new(Ledger::default());
        let reserve = ledger(l.clone());
        budget.max.calls = 1;
        assert!(Provider::build(file, checked, f.artifact, f.job.shape_profile_id, f.job.context_hash(), &budget, &reserve).is_err());
        assert_eq!(budget.used.get().calls, 1);
        assert!(budget.used.get().bytes > 0);
        assert_eq!(budget.used.get().attempts, 2);
        assert_eq!(l.live.load(Ordering::SeqCst), 0);
        let before = budget.used.get();
        assert!(Provider::build(file, checked, f.artifact, f.job.shape_profile_id, f.job.context_hash(), &budget, &reserve).is_err());
        assert_eq!(budget.used.get(), before);
        let budget = IoBudget::new();
        assert!(
            Provider::build(
                file,
                checked,
                Hash64::from_u64_word(999),
                f.job.shape_profile_id,
                f.job.context_hash(),
                &budget,
                &reserve
            )
            .is_err()
        );
        let failed = budget.used.get();
        assert!(failed.bytes > 0 && failed.calls > 0);
        assert_eq!(l.live.load(Ordering::SeqCst), 0);
        let provider =
            Provider::build(file, checked, f.artifact, f.job.shape_profile_id, f.job.context_hash(), &budget, &reserve).unwrap();
        assert_eq!(budget.used.get().bytes, 2 * failed.bytes);
        assert_eq!(budget.used.get().calls, 2 * failed.calls);
        drop(provider);
        assert_eq!(l.live.load(Ordering::SeqCst), 0);
    });
}
