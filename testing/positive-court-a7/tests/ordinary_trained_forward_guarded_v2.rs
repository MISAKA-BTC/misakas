//! Opt-in LOCAL requested-storage/CPU measurement caller, never native authority.
//! The three ignored cases are separately selected in fresh OS processes. They
//! read real retained cohort files ONLY when explicitly executed by the owner.
//! Ordinary forward is NoSink/no trace; the separately labeled dense comparison
//! intentionally retains the old dense roots. Matching is SELF CONSISTENCY ONLY.
//! Physical RSS quota is UNSUPPORTED here; an outer sampled PG monitor is only
//! kill-after-sample, not kernel memory admission or a native permit.
#[cfg(not(unix))]
compile_error!("the opt-in local caller requires checked native Unix CPU limits");
use kaspa_consensus_core::{
    Hash64,
    palw_native_resource_v1::{SourceProgramLocalLimitsV1, growing_vec_request_capacity_v1, require_source_program_local_profile_v1},
    palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1},
    palw_tir_artifact_v1::PALW_TIR_ROW_PIECE_BYTES_V1,
    palw_tir_class_v1::PalwTirLayoutV1,
    palw_v2::PalwJobContextV2,
};
use misaka_palw_tir::program::{MAX_BLOCKS, MAX_NODES_PER_BLOCK, MAX_PROGRAM_BYTES, MAX_STATES};
use misaka_palw_tir_artifact::bounded_v1::PalwTirHeaderLimitsV1;
use misaka_palw_tir_exec::{
    node::{
        memory::{TirMemoryLeaseV1, TirMemoryReserverV1},
        owned_artifact_params_v2::{
            OwnedParamLimitsV2,
            forward::{
                LocalForwardLoadBudgetV2, LocalForwardLoadLimitsV2, LocalWideForwardBudgetV2, LocalWideForwardLimitsV2,
                measurement::{DenseMeasurementBudgetV2, DenseMeasurementLimitsV2},
                metadata::{MetadataOwnerBudgetV2, MetadataOwnerInputsV2, MetadataOwnerLimitsV2, load_local_forward_metadata_v2},
            },
        },
        run_wide_q24_v2::TirWideRunLimitsV2,
    },
    plan::TirPlanCompileLimitsV2,
};
use misaka_positive_a7_tests::ordinary_forward_measurement::{
    FixedModelPinV2, FixedTrainedModelV2, HeaderFileStampV2, TrainedOrdinaryMeasurementV2, header_file_stamp_v2,
    measure_prepared_trained_v2,
};
use sha2::{Digest, Sha256};
use std::{
    ffi::CStr,
    fmt::{self, Write as _},
    fs::{File, Metadata},
    io::Read,
    mem::size_of,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
};
const REQUESTED_LIMIT: u64 = 8 * (1 << 30);
const NETWORK: &[u8] = b"a7-local-self-consistency";
const LAYOUT_KEY: &[u8] = b"\"layout_borsh_hex\"";
static STARTED: AtomicBool = AtomicBool::new(false);
static LEDGER: Ledger = Ledger::new();
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Error {
    Options,
    Overflow,
    Limit,
    AlreadyStarted,
    SourceProfile,
    Cpu,
    Guard,
    Io,
    Changed,
    Fixture,
    Allocation,
    Pins,
    Metadata,
    Measurement,
    Mismatch,
    Report,
}
type Result<T> = std::result::Result<T, Error>;
fn add(a: u64, b: u64) -> Result<u64> {
    a.checked_add(b).ok_or(Error::Overflow)
}
fn mul(a: u64, b: u64) -> Result<u64> {
    a.checked_mul(b).ok_or(Error::Overflow)
}
fn count(n: usize) -> Result<u64> {
    u64::try_from(n).map_err(|_| Error::Overflow)
}
fn check(value: bool, error: Error) -> Result<()> {
    if value { Ok(()) } else { Err(error) }
}
fn request<T>(n: u64) -> Result<u64> {
    mul(growing_vec_request_capacity_v1::<T>(n).ok_or(Error::Overflow)?, size_of::<T>() as u64)
}
fn exact<T>(n: usize) -> Result<Vec<T>> {
    let mut v = Vec::new();
    v.try_reserve_exact(n).map_err(|_| Error::Allocation)?;
    check(v.capacity() == n, Error::Allocation)?;
    Ok(v)
}
struct Ledger {
    live: AtomicU64,
    peak: AtomicU64,
    cumulative: AtomicU64,
}
impl Ledger {
    const fn new() -> Self {
        Self { live: AtomicU64::new(0), peak: AtomicU64::new(0), cumulative: AtomicU64::new(0) }
    }
    fn reserve(&self, bytes: u64) -> Result<Reservation<'_>> {
        check(bytes > 0, Error::Limit)?;
        let old = self
            .live
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_add(bytes).filter(|x| *x <= REQUESTED_LIMIT))
            .map_err(|_| Error::Guard)?;
        if self.cumulative.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_add(bytes)).is_err() {
            self.live.fetch_sub(bytes, Ordering::SeqCst);
            return Err(Error::Overflow);
        }
        self.peak.fetch_max(old + bytes, Ordering::SeqCst);
        Ok(Reservation { ledger: self, bytes })
    }
}
struct Reservation<'a> {
    ledger: &'a Ledger,
    bytes: u64,
}
impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        self.ledger.live.fetch_sub(self.bytes, Ordering::SeqCst);
    }
}
type ReserveFn = fn(&'static str, Hash64, Hash64, u64) -> std::result::Result<TirMemoryLeaseV1, String>;
// Same audited two-counter-plus-payload Arc recipe already used by Core's
// NativeWire Arc census. No closure capture/cache or global OnceLock is used.
#[repr(C)]
struct ReserverArcAllocation {
    _strong: AtomicUsize,
    _weak: AtomicUsize,
    _data: ReserveFn,
}
fn required_reserve(_: &'static str, _: Hash64, _: Hash64, bytes: u64) -> std::result::Result<TirMemoryLeaseV1, String> {
    // Guard backing is charged by this SAME ledger before Box allocation too.
    let bytes = bytes.checked_add(size_of::<Reservation<'static>>() as u64).ok_or_else(String::new)?;
    let guard = LEDGER.reserve(bytes).map_err(|_| String::new())?;
    Ok(Box::new(guard))
}
#[derive(Clone, Copy)]
struct Options {
    cpu_seconds: u64,
    fixture_bytes: u64,
    header_bytes: u64,
    header_native: u64,
    metadata_bytes: u64,
    source_bytes: u64,
    nf_items: u64,
    parameter_bytes: u64,
    ordinary_bytes: u64,
    dense_bytes: u64,
    work: u64,
    io_calls: u64,
    leaves: u64,
    lanes: u64,
    geometry_visits: u64,
}
fn positive_ascii(bytes: &[u8]) -> Result<u64> {
    check(!bytes.is_empty() && bytes.len() <= 20, Error::Options)?;
    let mut n = 0u64;
    for b in bytes {
        check(b.is_ascii_digit(), Error::Options)?;
        n = add(mul(n, 10)?, u64::from(*b - b'0'))?;
    }
    check(n > 0 && n < u64::MAX, Error::Options)?;
    Ok(n)
}
fn numeric_env(name: &CStr) -> Result<u64> {
    // Caller/OS environment is immutable for this exact isolated test process.
    // Bounded native getenv reads avoid allocating arbitrary OsString before E.
    let ptr = unsafe { libc::getenv(name.as_ptr()) };
    check(!ptr.is_null(), Error::Options)?;
    let mut buf = [0u8; 20];
    for index in 0..=20 {
        let b = unsafe { *ptr.add(index) as u8 };
        if b == 0 {
            return positive_ascii(&buf[..index]);
        }
        if index == 20 {
            return Err(Error::Options);
        }
        buf[index] = b;
    }
    Err(Error::Options)
}
fn options() -> Result<Options> {
    check(numeric_env(c"POSITIVE_ORDINARY_REQUESTED_BYTES_CAP")? == REQUESTED_LIMIT, Error::Options)?;
    Ok(Options {
        cpu_seconds: numeric_env(c"POSITIVE_ORDINARY_CPU_SECONDS")?,
        fixture_bytes: numeric_env(c"POSITIVE_ORDINARY_FIXTURE_BYTES")?,
        header_bytes: numeric_env(c"POSITIVE_ORDINARY_HEADER_BYTES")?,
        header_native: numeric_env(c"POSITIVE_ORDINARY_HEADER_NATIVE")?,
        metadata_bytes: numeric_env(c"POSITIVE_ORDINARY_METADATA_BYTES")?,
        source_bytes: numeric_env(c"POSITIVE_ORDINARY_SOURCE_BYTES")?,
        nf_items: numeric_env(c"POSITIVE_ORDINARY_NF_ITEMS")?,
        parameter_bytes: numeric_env(c"POSITIVE_ORDINARY_PARAMETER_BYTES")?,
        ordinary_bytes: numeric_env(c"POSITIVE_ORDINARY_OUTPUT_BYTES")?,
        dense_bytes: numeric_env(c"POSITIVE_ORDINARY_DENSE_BYTES")?,
        work: numeric_env(c"POSITIVE_ORDINARY_WORK")?,
        io_calls: numeric_env(c"POSITIVE_ORDINARY_IO_CALLS")?,
        leaves: numeric_env(c"POSITIVE_ORDINARY_LEAVES")?,
        lanes: numeric_env(c"POSITIVE_ORDINARY_LANES")?,
        geometry_visits: numeric_env(c"POSITIVE_ORDINARY_GEOMETRY_VISITS")?,
    })
}
// Pure conversion: platform infinity is not a finite resource limit. On macOS
// it is i64::MAX despite unsigned64 rlim_t, so rejecting only u64::MAX fails.
// Values above that sentinel are refused, never clipped/saturated to it.
fn finite_cpu_limit(seconds: u64) -> Result<libc::rlim_t> {
    let value = seconds as libc::rlim_t;
    check(value as u128 == u128::from(seconds) && value > 0 && value < libc::RLIM_INFINITY, Error::Cpu)?;
    Ok(value)
}
fn install_cpu_limit(seconds: u64) -> Result<()> {
    let value = finite_cpu_limit(seconds)?;
    let mut old = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    check(unsafe { libc::getrlimit(libc::RLIMIT_CPU, &mut old) } == 0, Error::Cpu)?;
    check(old.rlim_max == libc::RLIM_INFINITY || value <= old.rlim_max, Error::Cpu)?;
    let desired = libc::rlimit { rlim_cur: value, rlim_max: value };
    check(unsafe { libc::setrlimit(libc::RLIMIT_CPU, &desired) } == 0, Error::Cpu)?;
    let mut actual = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    check(
        unsafe { libc::getrlimit(libc::RLIMIT_CPU, &mut actual) } == 0 && actual.rlim_cur == value && actual.rlim_max == value,
        Error::Cpu,
    )
}
fn fixed_paths(model: FixedTrainedModelV2) -> (&'static Path, &'static Path, &'static str) {
    let (artifact, fixture, sha) = match model {
        FixedTrainedModelV2::SmolLm2360M => (
            "/Users/wata/Downloads/misaka-trained-llm-cohort-20261031/HuggingFaceTB__SmolLM2-360M/run/model.palwtir.testnet-12.palwtir",
            "/Users/wata/Downloads/misaka-trained-llm-cohort-20261031/HuggingFaceTB__SmolLM2-360M/run/registration-fixture.json",
            "c56ab4d0491f2be025b4efc37f0769b8bee4c7bd23564e0373270ad5d1a3bc40",
        ),
        FixedTrainedModelV2::SmolLm2135M => (
            "/Users/wata/Downloads/misaka-trained-llm-cohort-20261031/HuggingFaceTB__SmolLM2-135M/run/model.palwtir.testnet-12.palwtir",
            "/Users/wata/Downloads/misaka-trained-llm-cohort-20261031/HuggingFaceTB__SmolLM2-135M/run/registration-fixture.json",
            "252c20d0aab613bdc3e7349ba85e1a1306167cf06185ffff4bf12dea6df6e1b9",
        ),
        FixedTrainedModelV2::Pythia70M => (
            "/Users/wata/Downloads/misaka-trained-llm-cohort-20261031/EleutherAI__pythia-70m/run-wide-q24-v2/model.palwtir.private-wide-q24.palwtir",
            "/Users/wata/Downloads/misaka-trained-llm-cohort-20261031/EleutherAI__pythia-70m/run-wide-q24-v2/registration-fixture.json",
            "29c0d221c7f998dee2e4778bd8bfd9f9b8b4e610b17a62b1bf6f4484b7f367a8",
        ),
    };
    (Path::new(artifact), Path::new(fixture), sha)
}
fn hex_byte(bytes: &[u8]) -> Result<u8> {
    fn digit(b: u8) -> Result<u8> {
        match b {
            b'0'..=b'9' => Ok(b - b'0'),
            b'a'..=b'f' => Ok(b - b'a' + 10),
            b'A'..=b'F' => Ok(b - b'A' + 10),
            _ => Err(Error::Fixture),
        }
    }
    check(bytes.len() == 2, Error::Fixture)?;
    Ok((digit(bytes[0])? << 4) | digit(bytes[1])?)
}
fn hash64(text: &str) -> Result<Hash64> {
    check(text.len() == 128, Error::Pins)?;
    let mut out = [0u8; 64];
    for (target, b) in out.iter_mut().zip(text.as_bytes().chunks_exact(2)) {
        *target = hex_byte(b)?;
    }
    Ok(Hash64::from_bytes(out))
}
fn fixture_layout_hex(bytes: &[u8]) -> Result<&[u8]> {
    let mut at = None;
    for (index, chunk) in bytes.windows(LAYOUT_KEY.len()).enumerate() {
        if chunk == LAYOUT_KEY {
            check(at.is_none(), Error::Fixture)?;
            at = Some(index + LAYOUT_KEY.len());
        }
    }
    let mut at = at.ok_or(Error::Fixture)?;
    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        at += 1;
    }
    check(bytes.get(at) == Some(&b':'), Error::Fixture)?;
    at += 1;
    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        at += 1;
    }
    check(bytes.get(at) == Some(&b'"'), Error::Fixture)?;
    at += 1;
    let start = at;
    while bytes.get(at).is_some_and(|b| b.is_ascii_hexdigit()) {
        at += 1;
    }
    check(bytes.get(at) == Some(&b'"') && (at - start).is_multiple_of(2), Error::Fixture)?;
    Ok(&bytes[start..at])
}
struct HexReader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl HexReader<'_> {
    fn word<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut out = [0; N];
        for b in &mut out {
            let end = self.at.checked_add(2).ok_or(Error::Overflow)?;
            *b = hex_byte(self.bytes.get(self.at..end).ok_or(Error::Fixture)?)?;
            self.at = end;
        }
        Ok(out)
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.word()?))
    }
    fn tiles(&mut self, max: usize) -> Result<Vec<u32>> {
        let n = usize::try_from(self.u32()?).map_err(|_| Error::Overflow)?;
        check(n <= max, Error::Fixture)?;
        let end = self.at.checked_add(n.checked_mul(8).ok_or(Error::Overflow)?).ok_or(Error::Overflow)?;
        check(end <= self.bytes.len(), Error::Fixture)?;
        let mut out = exact(n)?;
        for _ in 0..n {
            out.push(self.u32()?);
        }
        Ok(out)
    }
}
fn decode_layout(hex: &[u8]) -> Result<PalwTirLayoutV1> {
    let max_bytes = add(22, mul(count(MAX_BLOCKS * MAX_NODES_PER_BLOCK + MAX_STATES)?, 4)?)?;
    check(count(hex.len())? <= mul(max_bytes, 2)?, Error::Fixture)?;
    let mut r = HexReader { bytes: hex, at: 0 };
    let version = u16::from_le_bytes(r.word()?);
    let max_context = r.u32()?;
    let checkpoint_interval = r.u32()?;
    let h_tile = r.u32()?;
    let commit_tiles = r.tiles(MAX_BLOCKS * MAX_NODES_PER_BLOCK)?;
    let state_tiles = r.tiles(MAX_STATES)?;
    check(r.at == hex.len(), Error::Fixture)?;
    Ok(PalwTirLayoutV1 { version, max_context, checkpoint_interval, h_tile, commit_tiles, state_tiles })
}
fn read_fixture(path: &Path, max: u64, expected_sha: &str, mut calls: u64) -> Result<Vec<u8>> {
    let mut file = File::open(path).map_err(|_| Error::Io)?;
    let before = header_file_stamp_v2(&file).map_err(|_| Error::Io)?;
    check(before.bytes <= max, Error::Limit)?;
    let len = usize::try_from(before.bytes).map_err(|_| Error::Overflow)?;
    let mut bytes = exact(len)?;
    bytes.resize(len, 0);
    let mut at = 0;
    while at < len {
        check(calls > 0, Error::Limit)?;
        calls -= 1;
        // No retrying Interrupted/short EOF outside the finite syscall allowance.
        let got = file.read(&mut bytes[at..]).map_err(|_| Error::Io)?;
        check(got > 0, Error::Io)?;
        at = at.checked_add(got).ok_or(Error::Overflow)?;
    }
    let after = header_file_stamp_v2(&file).map_err(|_| Error::Io)?;
    check(before == after, Error::Changed)?;
    check(expected_sha.len() == 64, Error::Pins)?;
    let digest = Sha256::digest(&bytes);
    for (actual, chars) in digest.iter().zip(expected_sha.as_bytes().chunks_exact(2)) {
        check(*actual == hex_byte(chars)?, Error::Fixture)?;
    }
    Ok(bytes)
}
fn original_job(model: FixedTrainedModelV2) -> Result<PalwJobContextV2> {
    let p = model.pin();
    let zero = Hash64::default();
    let mut network = exact(NETWORK.len())?;
    network.extend_from_slice(NETWORK);
    let ctx = PalwJobContextV2 {
        version: 2,
        network_id: network,
        job_id: Hash64::from_u64_word(7),
        job_nullifier: zero,
        assignment_id: zero,
        execution_seed: [7; 32],
        model_profile_id: zero,
        runtime_manifest_hash: zero,
        runtime_class_id: zero,
        shape_profile_id: hash64(p.class_id)?,
        trace_scheme_id: match model {
            FixedTrainedModelV2::Pythia70M => kaspa_consensus_core::palw_logits_q24_v2::scheme_id(),
            _ => kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1(),
        },
        cu_ruleset_id: zero,
        tokenizer_id: zero,
        prompt_token_ids_hash: prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, p.prompt).map_err(|_| Error::Pins)?,
        declared_prefill_tokens: 16,
        exact_decode_tokens: 1,
        max_context_tokens: p.original_max_context,
    };
    check(p.prompt.len() == 16 && ctx.context_hash() == hash64(p.original_context_hash)?, Error::Pins)?;
    Ok(ctx)
}
fn bootstrap_requests(o: Options) -> Result<u64> {
    // All INTRODUCED logical typed roots. Explicit conservative overlaps with
    // component results; no compiler-stack/RSS or preexisting libtest census.
    let roots = [
        size_of::<Options>(),
        size_of::<FixedModelPinV2>(),
        size_of::<[u8; 32]>(),
        size_of::<std::io::Error>(),
        size_of::<Ledger>(),
        size_of::<Reservation<'static>>(),
        size_of::<TirMemoryReserverV1>(),
        size_of::<ReserverArcAllocation>(),
        size_of::<PalwJobContextV2>(),
        size_of::<PalwTirLayoutV1>(),
        size_of::<Vec<u8>>(),
        size_of::<[File; 2]>(),
        size_of::<[Metadata; 2]>(),
        size_of::<[HeaderFileStampV2; 3]>(),
        size_of::<[libc::rlimit; 3]>(),
        size_of::<Sha256>(),
        size_of::<MetadataOwnerBudgetV2>(),
        size_of::<LocalForwardLoadBudgetV2>(),
        size_of::<LocalWideForwardBudgetV2>(),
        size_of::<DenseMeasurementBudgetV2>(),
        size_of::<Result<TrainedOrdinaryMeasurementV2>>(),
        size_of::<Report>(),
        size_of::<[Hash64; 4]>(),
        size_of::<[u8; 20]>(),
    ];
    let roots = roots.into_iter().try_fold(0u64, |n, x| add(n, x as u64))?;
    let layout = add(request::<u32>(count(MAX_BLOCKS * MAX_NODES_PER_BLOCK)?)?, request::<u32>(count(MAX_STATES)?)?)?;
    let context_encoded = add(2 + 4 + 11 * 64 + 32 + 3 * 4, count(NETWORK.len())?)?;
    // Existing prompt CanonicalWriter starts512, context writer then grows from512.
    let writers = add(request::<u8>(512)?, request::<u8>(context_encoded.max(512))?)?;
    add(roots, add(add(request::<u8>(o.fixture_bytes)?, layout)?, add(request::<u8>(count(NETWORK.len())?)?, writers)?)?)
}
fn metadata_limits(o: Options, bytes: u64) -> MetadataOwnerLimitsV2 {
    MetadataOwnerLimitsV2 {
        header: PalwTirHeaderLimitsV1 {
            max_file_bytes: bytes,
            max_header_bytes: o.header_bytes,
            max_program_bytes: MAX_PROGRAM_BYTES as u64,
            max_layout_bytes: o.header_bytes,
            max_tensors: o.nf_items,
            max_meta_bytes: o.header_bytes,
            max_native_bytes: o.header_native,
            max_copy_bytes: o.header_native,
            max_work: o.work,
            max_operation_bytes: REQUESTED_LIMIT,
            external_retained_bytes: 0,
        },
        source: SourceProgramLocalLimitsV1 {
            max_program_bytes: MAX_PROGRAM_BYTES as u64,
            max_scan_steps: o.work,
            max_nf_items: o.nf_items,
            max_native_requests: o.source_bytes,
        },
        compile: TirPlanCompileLimitsV2 {
            max_quote_visits: o.work,
            max_retained_bytes: o.metadata_bytes,
            max_scratch_bytes: o.metadata_bytes,
            max_operation_bytes: REQUESTED_LIMIT,
            external_retained_bytes: 0,
        },
        max_new_owner_bytes: o.metadata_bytes,
        max_operation_bytes: REQUESTED_LIMIT,
        max_attempts: 1,
        max_retained_requests: o.metadata_bytes,
        max_total_read_bytes: bytes,
        max_total_work: o.work,
        max_total_scan_steps: o.work,
        max_io_calls_per_attempt: o.io_calls,
        max_total_io_calls: o.io_calls,
    }
}
fn load_limits(o: Options, bytes: u64) -> LocalForwardLoadLimitsV2 {
    LocalForwardLoadLimitsV2 {
        params: OwnedParamLimitsV2 {
            max_tensor_bytes: o.parameter_bytes,
            max_payload_bytes: o.parameter_bytes,
            max_chunk_bytes: PALW_TIR_ROW_PIECE_BYTES_V1,
            max_metadata_bytes: o.parameter_bytes,
            max_native_bytes: o.parameter_bytes,
            max_inventory_leaves: o.leaves,
            max_work_per_attempt: o.work,
            max_attempts: 1,
            max_total_read_bytes: bytes,
            max_total_inventory_leaves: o.leaves,
            max_total_work: o.work,
            max_operation_bytes: REQUESTED_LIMIT,
            external_retained_bytes: 0,
        },
        max_new_owner_bytes: o.parameter_bytes,
        max_operation_bytes: REQUESTED_LIMIT,
        max_retained_requests: o.parameter_bytes,
        max_provider_calls_per_attempt: o.io_calls,
        max_total_provider_calls: o.io_calls,
    }
}
fn run_limits(o: Options, max: u32, external: u64) -> LocalWideForwardLimitsV2 {
    LocalWideForwardLimitsV2 {
        max_positions: max,
        max_generated: 1,
        max_logits_lanes: o.lanes,
        max_work_per_attempt: o.work,
        max_requested_bytes: o.ordinary_bytes,
        max_operation_bytes: REQUESTED_LIMIT,
        max_attempts: 1,
        max_total_work: o.work,
        max_retained_requests: o.ordinary_bytes,
        external_retained_bytes: external,
    }
}
fn dense_limits(o: Options, max: u32, external: u64) -> DenseMeasurementLimitsV2 {
    DenseMeasurementLimitsV2 {
        runner: TirWideRunLimitsV2 {
            max_positions: max,
            max_step_leaves: o.leaves,
            max_row_lanes: o.lanes,
            max_work: o.work,
            max_requested_bytes: o.dense_bytes,
        },
        max_extra_bytes: o.dense_bytes,
        max_combined_peak_bytes: o.dense_bytes,
        max_operation_bytes: REQUESTED_LIMIT,
        max_geometry_visits: o.geometry_visits,
        max_work_per_attempt: o.work,
        max_attempts: 1,
        max_total_work: o.work,
        max_retained_requests: o.dense_bytes,
        external_retained_bytes: external,
    }
}
struct Report {
    bytes: [u8; 4096],
    used: usize,
}
impl fmt::Write for Report {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let end = self.used.checked_add(s.len()).ok_or(fmt::Error)?;
        let target = self.bytes.get_mut(self.used..end).ok_or(fmt::Error)?;
        target.copy_from_slice(s.as_bytes());
        self.used = end;
        Ok(())
    }
}
fn emit(
    model: FixedTrainedModelV2,
    result: &TrainedOrdinaryMeasurementV2,
    cpu_seconds: u64,
    external: u64,
    metadata: u64,
) -> Result<()> {
    let mut report = Report { bytes: [0; 4096], used: 0 };
    let times = result.times();
    write!(report,"ORDINARY_LOCAL_SELF_CONSISTENCY_ONLY model={} REQUESTED_BYTES_CAP={} physical_rss_quota=UNSUPPORTED outer_monitor=EXTERNAL_NOT_ATTESTED outer_monitor_semantics=sampled_pg_kill_after_sample_only cpu_limit_process_seconds={} context_max={} prefill=16 decode=1 metadata_allowance={} earlier_E={} peak_requested={} cumulative_requested={} ordinary_no_trace=1 dense_comparison_has_trace=1 node_claim=0 native_authority=0 public_P=0 full_context_inference=0 independent_HF_expected=0 load_ns={} ordinary_ns={} dense_prepare_ns={} dense_ns={} generated=",model.pin().model,REQUESTED_LIMIT,cpu_seconds,model.pin().original_max_context,metadata,external,LEDGER.peak.load(Ordering::SeqCst),LEDGER.cumulative.load(Ordering::SeqCst),times.owned_parameter_load_ns,times.ordinary_ns,times.dense_prepare_ns,times.dense_run_ns).map_err(|_|Error::Report)?;
    for id in result.ordinary().generated() {
        write!(report, "{id},").map_err(|_| Error::Report)?;
    }
    report.write_str(" output_hash=").map_err(|_| Error::Report)?;
    for b in result.ordinary().output_root().as_byte_slice() {
        write!(report, "{b:02x}").map_err(|_| Error::Report)?;
    }
    report.write_str("\n").map_err(|_| Error::Report)?;
    // No stdio/capture Vec ownership: fixed bounded buffer, direct OS write.
    // Outer process owner supplies wall deadline/cancellation/output retention.
    let mut at = 0;
    while at < report.used {
        let n = unsafe { libc::write(libc::STDOUT_FILENO, report.bytes[at..report.used].as_ptr().cast(), report.used - at) };
        check(n > 0, Error::Report)?;
        at = at.checked_add(n as usize).ok_or(Error::Overflow)?;
    }
    Ok(())
}
fn guarded_case(model: FixedTrainedModelV2) -> Result<()> {
    // No payload touch without explicit opt-in exact ignored case and all caps.
    check(!STARTED.swap(true, Ordering::SeqCst), Error::AlreadyStarted)?;
    require_source_program_local_profile_v1().map_err(|_| Error::SourceProfile)?;
    let o = options()?;
    let external = bootstrap_requests(o)?;
    let total = add(add(add(add(external, o.metadata_bytes)?, o.parameter_bytes)?, o.ordinary_bytes)?, o.dense_bytes)?;
    check(total <= REQUESTED_LIMIT && o.source_bytes < o.metadata_bytes && o.header_native < o.metadata_bytes, Error::Limit)?;
    // Named fixture passes: zero/read/SHA, boundary+hex scan, hex decode,
    // and tile insertion, plus key comparison at every byte. Symbolic only;
    // the checked process RLIMIT_CPU is the actual cumulative CPU cutoff.
    check(mul(o.fixture_bytes, add(count(LAYOUT_KEY.len())?, 6)?)? <= o.work, Error::Limit)?;
    let early = LEDGER.reserve(external)?; // BEFORE Arc/context/layout/File allocations.
    {
        install_cpu_limit(o.cpu_seconds)?;
        let reserver: TirMemoryReserverV1 = Arc::new(required_reserve as ReserveFn);
        let pin = model.pin();
        let (artifact_path, fixture_path, fixture_sha) = fixed_paths(model);
        let fixture = read_fixture(fixture_path, o.fixture_bytes, fixture_sha, o.io_calls)?;
        let layout = decode_layout(fixture_layout_hex(&fixture)?)?;
        check(layout.max_context == pin.original_max_context, Error::Pins)?;
        let ctx = original_job(model)?;
        let inputs = MetadataOwnerInputsV2 {
            expected_class: hash64(pin.class_id)?,
            artifact: hash64(pin.artifact_root)?,
            registered_tokenizer: hash64(pin.registered_tokenizer)?,
            registered_max_context: pin.original_max_context,
            original_context: &ctx,
            expected_execution: hash64(pin.original_context_hash)?,
            expected_layout: Some(&layout),
            external_retained_bytes: external,
            external_ticket: &early,
        };
        let file = File::open(artifact_path).map_err(|_| Error::Io)?;
        let at_header = header_file_stamp_v2(&file).map_err(|_| Error::Io)?;
        check(at_header.bytes == pin.artifact_bytes, Error::Pins)?;
        let mut metadatas = MetadataOwnerBudgetV2::new(metadata_limits(o, pin.artifact_bytes)).map_err(|_| Error::Metadata)?;
        let mut metadata = load_local_forward_metadata_v2(file, inputs, &mut metadatas, &reserver).map_err(|_| Error::Metadata)?;
        let metadata_external = metadata.quote().bridge_external_requests;
        let mut loads = LocalForwardLoadBudgetV2::new(load_limits(o, pin.artifact_bytes)).map_err(|_| Error::Limit)?;
        let run_external = add(metadata_external, o.parameter_bytes)?;
        let mut runs =
            LocalWideForwardBudgetV2::new(run_limits(o, pin.original_max_context, run_external)).map_err(|_| Error::Limit)?;
        let mut denses =
            DenseMeasurementBudgetV2::new(dense_limits(o, pin.original_max_context, add(run_external, o.ordinary_bytes)?))
                .map_err(|_| Error::Limit)?;
        // SAME mutable descriptor only; NO clone/replace/detach/reopen or owned class
        // copies. HRTB is borrow control, not a claim it forbids arbitrary FD clones.
        let measured = metadata
            .with_inputs(&mut metadatas, |file, prepared| {
                measure_prepared_trained_v2(
                    model,
                    file,
                    at_header,
                    prepared,
                    pin.prompt,
                    &mut loads,
                    &mut runs,
                    &mut denses,
                    &reserver,
                )
            })
            .map_err(|_| Error::Metadata)?
            .map_err(|_| Error::Measurement)?;
        check(
            measured.dense().matches_ordinary()
                && measured.dense().generated() == measured.ordinary().generated()
                && measured.dense().output_root() == measured.ordinary().output_root(),
            Error::Mismatch,
        )?;
        emit(model, &measured, o.cpu_seconds, external, o.metadata_bytes)?;
    } // All introduced owners, budgets, Arc and File data drop BEFORE earlier E.
    check(LEDGER.live.load(Ordering::SeqCst) == external, Error::Guard)?;
    drop(early);
    check(LEDGER.live.load(Ordering::SeqCst) == 0, Error::Guard)
}
#[test]
#[ignore = "requires owner-selected real cohort files, explicit caps and an isolated CPU/outer-monitor process"]
fn ordinary_trained_smollm2_360m() -> Result<()> {
    guarded_case(FixedTrainedModelV2::SmolLm2360M)
}
#[test]
#[ignore = "requires owner-selected real cohort files, explicit caps and an isolated CPU/outer-monitor process"]
fn ordinary_trained_smollm2_135m() -> Result<()> {
    guarded_case(FixedTrainedModelV2::SmolLm2135M)
}
#[test]
#[ignore = "requires owner-selected real cohort files, explicit caps and an isolated CPU/outer-monitor process"]
fn ordinary_trained_pythia_70m() -> Result<()> {
    guarded_case(FixedTrainedModelV2::Pythia70M)
}
#[test]
fn requested_ledger_denial_and_drop_never_create_an_unreserved_lease() {
    let ledger = Ledger::new();
    let first = ledger.reserve(REQUESTED_LIMIT - 1).unwrap();
    assert!(matches!(ledger.reserve(2), Err(Error::Guard)));
    assert_eq!(ledger.live.load(Ordering::SeqCst), REQUESTED_LIMIT - 1);
    drop(first);
    assert_eq!(ledger.live.load(Ordering::SeqCst), 0);
    assert_eq!(ledger.cumulative.load(Ordering::SeqCst), REQUESTED_LIMIT - 1);
}
#[test]
fn bounded_numeric_options_reject_zero_overflow_and_nondecimal() {
    assert_eq!(positive_ascii(b"8589934592"), Ok(REQUESTED_LIMIT));
    for text in [b"0".as_slice(), b"-1", b"18446744073709551616", b"12x", b""] {
        assert!(positive_ascii(text).is_err());
    }
}
#[test]
fn exact_layout_parser_rejects_duplicate_trailing_and_oversized_counts() {
    let bytes = b"{\"layout_borsh_hex\":\"0100400000000200000002000000010000000400000000000000\"}";
    let layout = decode_layout(fixture_layout_hex(bytes).unwrap()).unwrap();
    assert_eq!(layout.version, 1);
    assert_eq!(layout.max_context, 64);
    assert_eq!(layout.commit_tiles, vec![4]);
    assert!(layout.state_tiles.is_empty());
    assert_eq!(fixture_layout_hex(b"{\"layout_borsh_hex\":\"00\",\"layout_borsh_hex\":\"00\"}"), Err(Error::Fixture));
    assert!(decode_layout(b"0100400000000200000002000000ffffffff").is_err());
    assert!(decode_layout(b"0100400000000200000002000000000000000000000000").is_err());
}

#[test]
fn fixed_original_jobs_keep_recorded_hashes_prompt_and_zero_tokenizer_cu() {
    let ledger = Ledger::new();
    let bytes = add(
        size_of::<PalwJobContextV2>() as u64,
        add(request::<u8>(NETWORK.len() as u64).unwrap(), add(request::<u8>(512).unwrap(), request::<u8>(1024).unwrap()).unwrap())
            .unwrap(),
    )
    .unwrap();
    let _early = ledger.reserve(bytes).unwrap();
    for model in [FixedTrainedModelV2::SmolLm2360M, FixedTrainedModelV2::SmolLm2135M, FixedTrainedModelV2::Pythia70M] {
        let ctx = original_job(model).unwrap();
        assert_eq!(ctx.context_hash(), hash64(model.pin().original_context_hash).unwrap());
        assert_eq!(ctx.tokenizer_id, Hash64::default());
        assert_eq!(ctx.cu_ruleset_id, Hash64::default());
        assert_eq!(ctx.max_context_tokens, model.pin().original_max_context);
        assert_eq!(ctx.execution_seed, [7; 32]);
        assert_eq!(ctx.job_id, Hash64::from_u64_word(7));
        assert_eq!((ctx.declared_prefill_tokens, ctx.exact_decode_tokens), (16, 1));
    }
}

#[test]
fn finite_cpu_conversion_rejects_platform_infinity_zero_and_overflow_without_syscalls() {
    assert_eq!(finite_cpu_limit(0), Err(Error::Cpu));
    assert_eq!(finite_cpu_limit(1), Ok(1));
    let infinity = libc::RLIM_INFINITY as u64;
    assert_eq!(finite_cpu_limit(infinity), Err(Error::Cpu));
    assert_eq!(finite_cpu_limit(u64::MAX), Err(Error::Cpu));
    if let Some(above) = infinity.checked_add(1) {
        assert_eq!(finite_cpu_limit(above), Err(Error::Cpu));
    }
    // Conversion itself has no get/setrlimit call or CPU/process mutation.
}
