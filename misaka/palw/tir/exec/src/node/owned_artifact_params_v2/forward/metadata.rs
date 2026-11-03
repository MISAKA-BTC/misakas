//! Same-descriptor trace-free metadata owner. ONE real required configured
//! guard precedes header/class/Source/Space/compiler allocations. All component
//! quotes are logical request inventories, not allocator RSS/all-CPU guarantees.
//! No parameter payload is read here, no trace binding is accepted/constructed,
//! no native assignment/result/role/funding/P/credit/public flag is installed.
use super::*;
use crate::plan::{
    LocalCompiledTirPlanV2, TirPlanCompileLimitsV2, TirPlanCompileQuoteV2, prepare_tir_plan_compile_v2,
    tir_plan_compile_quote_visits_upper_v2,
};
use kaspa_consensus_core::{
    palw_native_resource_v1::{
        LocalScanWorkBudgetV1, LocalSourceProgramOwnerV1, SourceProgramLocalLimitsV1, require_source_program_local_profile_v1,
        scan_source_program_local_v1,
    },
    palw_tir_step_v1::{
        LocalTirStepSpaceFixedRequestsV1, quote_local_tir_step_space_additional_v1, quote_local_tir_step_space_fixed_requests_v1,
    },
};
use misaka_palw_tir_artifact::bounded_v1::{
    BoundedPalwTirHeaderV1, PalwTirHeaderLimitsV1, read_header_bounded_v1,
    startup_quote::{ConfiguredSameBytesFileHeaderQuoteV1, quote_configured_same_bytes_file_header_v1},
};
use std::fs::{File, Metadata};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetadataOwnerErrorV2 {
    Unsupported,
    Limit,
    Overflow,
    Pin,
    MissingLayout,
    File,
    Changed,
    Header,
    Source,
    Geometry,
    Compile,
    Allocation,
    Guard,
}
use MetadataOwnerErrorV2 as M;
fn add(a: u64, b: u64) -> Result<u64, M> {
    a.checked_add(b).ok_or(M::Overflow)
}
fn mul(a: u64, b: u64) -> Result<u64, M> {
    a.checked_mul(b).ok_or(M::Overflow)
}
fn n(a: usize) -> Result<u64, M> {
    u64::try_from(a).map_err(|_| M::Overflow)
}
fn cap(a: u64, max: u64) -> Result<(), M> {
    if a > max { Err(M::Limit) } else { Ok(()) }
}
fn vec_req<T>(n: u64) -> Result<u64, M> {
    mul(growing_vec_request_capacity_v1::<T>(n).ok_or(M::Overflow)?, size_of::<T>() as u64)
}
fn copy_exact<T: Copy>(values: &[T]) -> Result<Vec<T>, M> {
    let mut out = super::super::exact(values.len()).map_err(|_| M::Allocation)?;
    out.extend_from_slice(values);
    Ok(out)
}
fn collect_exact(values: impl ExactSizeIterator<Item = u32>) -> Result<Vec<u32>, M> {
    let mut out = super::super::exact(values.len()).map_err(|_| M::Allocation)?;
    out.extend(values);
    Ok(out)
}
/// Independent metadata pins/context and ACTUAL external control/alias owner.
/// Neither this public data nor successful metadata parsing is native authority.
/// External includes context/network backing, optional exact layout preimage,
/// this budget, callback/error/OS/cache/IO/deadline resources and every alias.
/// No concurrently mutating file/descriptor-offset aliases may remain active.
#[derive(Clone, Copy)]
pub struct MetadataOwnerInputsV2<'a> {
    pub expected_class: Hash64,
    pub artifact: Hash64,
    pub registered_tokenizer: Hash64,
    pub registered_max_context: u32,
    pub original_context: &'a PalwJobContextV2,
    pub expected_execution: Hash64,
    /// Required exact independently pinned preimage ONLY for undeclared layout;
    /// if also provided for declared layout it must match, never overwrite it.
    pub expected_layout: Option<&'a PalwTirLayoutV1>,
    pub external_retained_bytes: u64,
    pub external_ticket: &'a (dyn Send + Sync),
}
#[derive(Clone, Copy, Debug)]
pub struct MetadataOwnerLimitsV2 {
    pub header: PalwTirHeaderLimitsV1,
    pub source: SourceProgramLocalLimitsV1,
    pub compile: TirPlanCompileLimitsV2,
    pub max_new_owner_bytes: u64,
    pub max_operation_bytes: u64,
    pub max_attempts: u64,
    pub max_retained_requests: u64,
    pub max_total_read_bytes: u64,
    pub max_total_work: u64,
    pub max_total_scan_steps: u64,
    pub max_io_calls_per_attempt: u64,
    pub max_total_io_calls: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MetadataOwnerUsageV2 {
    pub attempts: u64,
    pub retained_requests: u64,
    pub read_bytes: u64,
    pub work: u64,
    pub io_calls_reserved: u64,
    pub borrowed_validations: u64,
}
pub struct MetadataOwnerBudgetV2 {
    limits: MetadataOwnerLimitsV2,
    usage: MetadataOwnerUsageV2,
    source: LocalScanWorkBudgetV1,
}
impl MetadataOwnerBudgetV2 {
    pub fn new(l: MetadataOwnerLimitsV2) -> Result<Self, M> {
        if l.header.external_retained_bytes != 0 || l.compile.external_retained_bytes != 0 {
            return Err(M::Pin);
        }
        if [
            l.max_new_owner_bytes,
            l.max_operation_bytes,
            l.max_attempts,
            l.max_retained_requests,
            l.max_total_read_bytes,
            l.max_total_work,
            l.max_total_scan_steps,
            l.max_io_calls_per_attempt,
            l.max_total_io_calls,
        ]
        .contains(&0)
        {
            return Err(M::Limit);
        }
        Ok(Self {
            limits: l,
            usage: MetadataOwnerUsageV2::default(),
            source: LocalScanWorkBudgetV1::new(l.max_total_scan_steps).map_err(|_| M::Source)?,
        })
    }
    pub fn usage(&self) -> MetadataOwnerUsageV2 {
        self.usage
    }
    pub fn source_observations(&self) -> (u64, u64, u64, u64, u64) {
        (
            self.source.attempts(),
            self.source.charged_steps(),
            self.source.observed_steps(),
            self.source.charged_reader_bytes(),
            self.source.observed_reader_bytes(),
        )
    }
    fn begin(&mut self, i: MetadataOwnerInputsV2<'_>) -> Result<u64, M> {
        let prior = self.usage.retained_requests;
        let attempts = add(self.usage.attempts, 1)?;
        cap(attempts, self.limits.max_attempts)?;
        let next = add(prior, self.limits.max_new_owner_bytes)?;
        cap(next, self.limits.max_retained_requests)?;
        cap(add(i.external_retained_bytes, next)?, self.limits.max_operation_bytes)?;
        let calls = add(self.usage.io_calls_reserved, self.limits.max_io_calls_per_attempt)?;
        cap(calls, self.limits.max_total_io_calls)?;
        self.usage.attempts = attempts;
        self.usage.retained_requests = next;
        self.usage.io_calls_reserved = calls;
        Ok(prior)
    }
    fn charge(&mut self, reads: u64, work: u64) -> Result<(), M> {
        let r = add(self.usage.read_bytes, reads)?;
        let w = add(self.usage.work, work)?;
        cap(r, self.limits.max_total_read_bytes)?;
        cap(w, self.limits.max_total_work)?;
        self.usage.read_bytes = r;
        self.usage.work = w;
        Ok(())
    }
    fn stage(&self, peak: u64, external: u64) -> Result<u64, M> {
        cap(peak, self.limits.max_new_owner_bytes)?;
        let total = add(peak, external)?;
        cap(total, self.limits.max_operation_bytes)?;
        Ok(total)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MetadataFileStampV2 {
    pub device: u64,
    pub inode: u64,
    pub bytes: u64,
    pub mtime: i64,
    pub mtime_ns: i64,
    pub ctime: i64,
    pub ctime_ns: i64,
}
fn stamp(file: &File) -> Result<MetadataFileStampV2, M> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let s = file.metadata().map_err(|_| M::File)?;
        if !s.is_file() {
            return Err(M::File);
        }
        Ok(MetadataFileStampV2 {
            device: s.dev(),
            inode: s.ino(),
            bytes: s.len(),
            mtime: s.mtime(),
            mtime_ns: s.mtime_nsec(),
            ctime: s.ctime(),
            ctime_ns: s.ctime_nsec(),
        })
    }
    #[cfg(not(unix))]
    {
        let _ = file;
        Err(M::Unsupported)
    }
}
struct HeaderIo<'a> {
    file: &'a mut File,
    left_calls: u64,
    left_bytes: u64,
}
impl HeaderIo<'_> {
    fn call(&mut self) -> std::io::Result<()> {
        if self.left_calls == 0 {
            return Err(std::io::ErrorKind::Other.into());
        }
        self.left_calls -= 1;
        Ok(())
    }
}
impl Read for HeaderIo<'_> {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        self.call()?;
        if b.len() as u64 > self.left_bytes {
            return Err(std::io::ErrorKind::Other.into());
        }
        let got = self.file.read(b)?;
        self.left_bytes -= got as u64;
        Ok(got)
    }
}
impl Seek for HeaderIo<'_> {
    fn seek(&mut self, p: SeekFrom) -> std::io::Result<u64> {
        self.call()?;
        self.file.seek(p)
    }
}
enum OwnedGeometryV2<'a> {
    Narrow(LocalNarrowForwardSpaceV2<'a>),
    Wide(PalwTirStepSpaceV2),
}
impl OwnedGeometryV2<'_> {
    fn view(&self) -> LocalForwardSpaceV2<'_> {
        match self {
            Self::Narrow(s) => LocalForwardSpaceV2::Narrow(s),
            Self::Wide(s) => LocalForwardSpaceV2::Wide(s),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct MetadataOwnerQuoteV2 {
    pub fixed_roots: u64,
    pub configured_header: ConfiguredSameBytesFileHeaderQuoteV1,
    pub header_retained: u64,
    pub class_requests: u64,
    pub source_complete: u64,
    pub space_decode: u64,
    pub space_revalidate: u64,
    pub space_layout: u64,
    pub space_geometry: u64,
    pub space_fixed: LocalTirStepSpaceFixedRequestsV1,
    pub space_peak: u64,
    pub hash_scratch: u64,
    pub compile: TirPlanCompileQuoteV2,
    pub header_stage_peak: u64,
    pub source_stage_peak: u64,
    pub class_stage_peak: u64,
    pub space_stage_peak: u64,
    pub compile_stage_peak: u64,
    pub conservative_retained_requests: u64,
    pub actual_guard_allowance: u64,
    pub bridge_external_requests: u64,
}
/// No Clone/extraction/raw mutable plan or File getter. Every owned field drops
/// BEFORE the real lease. Original context/preimage/control ticket remain under
/// external lifetime; owned narrow wrapper anchors that independent ticket, not
/// this object's own lease (no self-reference). Actual space is paid by M.
pub struct LocalForwardMetadataOwnerV2<'a> {
    file: File,
    header: BoundedPalwTirHeaderV1,
    class: PalwTirClassV1,
    source: LocalSourceProgramOwnerV1,
    space: OwnedGeometryV2<'a>,
    compiled: LocalCompiledTirPlanV2,
    inputs: MetadataOwnerInputsV2<'a>,
    stamp: MetadataFileStampV2,
    header_limits: PalwTirHeaderLimitsV1,
    quote: MetadataOwnerQuoteV2,
    validation_work: u64,
    _memory: TirMemoryLeaseV1,
}
fn root_requests() -> Result<u64, M> {
    // Logical typed roots, not compiler stack/RSS accounting. Component quotes
    // additionally contain their own embedded roots: explicit conservative
    // aggregate/move overlap. Context/budget/callback roots are external E.
    let owner = size_of::<LocalForwardMetadataOwnerV2<'_>>() as u64;
    let returned = (size_of::<Result<LocalForwardMetadataOwnerV2<'_>, M>>() as u64).checked_sub(owner).ok_or(M::Overflow)?;
    add(
        add(owner, returned)?,
        add(
            add(size_of::<HeaderIo<'_>>() as u64, mul(size_of::<MetadataFileStampV2>() as u64, 2)?)?,
            add(size_of::<Result<Metadata, M>>() as u64, size_of::<M>() as u64)?,
        )?,
    )
}
fn source_scan_upper(bytes: u64) -> Result<u64, M> {
    let intrinsic = |n| add(mul(n, 8)?, 4096);
    add(add(intrinsic(add(bytes, 4)?)?, mul(intrinsic(bytes)?, 3)?)?, 4)
}
fn header_validation_work(p: &misaka_palw_tir::TirProgramV1, work: u64) -> Result<u64, M> {
    let nodes = mul(n(p.blocks.len())?, MAX_NODES_PER_BLOCK as u64)?;
    let refs = mul(nodes, MAX_NODE_INPUTS as u64)?;
    let scans = mul(mul(n(p.params.len())?, add(n(p.schedule.layers.len())?, 1)?)?, add(add(nodes, refs)?, 1)?)?;
    add(work, mul(add(scans, 1)?, 128)?)
}
fn class_quote(header: &BoundedPalwTirHeaderV1, expected: Option<&PalwTirLayoutV1>) -> Result<(u64, u64), M> {
    let (commits, states) = match header.layout().map_err(|_| M::Header)? {
        Some(v) => (v.commit_tiles().len(), v.state_tiles().len()),
        None => {
            let x = expected.ok_or(M::MissingLayout)?;
            (x.commit_tiles.len(), x.state_tiles.len())
        }
    };
    if commits > MAX_BLOCKS * MAX_NODES_PER_BLOCK || states > MAX_STATES {
        return Err(M::Limit);
    }
    let layout = add(vec_req::<u32>(n(commits)?)?, vec_req::<u32>(n(states)?)?)?;
    Ok((add(size_of::<PalwTirClassV1>() as u64, add(vec_req::<u8>(n(header.header().program.len())?)?, layout)?)?, layout))
}
fn build_class(header: &BoundedPalwTirHeaderV1, expected: Option<&PalwTirLayoutV1>) -> Result<PalwTirClassV1, M> {
    let layout = match header.layout().map_err(|_| M::Header)? {
        Some(v) => PalwTirLayoutV1 {
            version: v.version,
            max_context: v.max_context,
            checkpoint_interval: v.checkpoint_interval,
            h_tile: v.h_tile,
            commit_tiles: collect_exact(v.commit_tiles())?,
            state_tiles: collect_exact(v.state_tiles())?,
        },
        None => {
            let e = expected.ok_or(M::MissingLayout)?;
            PalwTirLayoutV1 {
                version: e.version,
                max_context: e.max_context,
                checkpoint_interval: e.checkpoint_interval,
                h_tile: e.h_tile,
                commit_tiles: copy_exact(&e.commit_tiles)?,
                state_tiles: copy_exact(&e.state_tiles)?,
            }
        }
    };
    if expected.is_some_and(|e| e != &layout) {
        return Err(M::Pin);
    }
    Ok(PalwTirClassV1 {
        version: kaspa_consensus_core::palw_tir_class_v1::PALW_TIR_CLASS_VERSION_V1,
        program: copy_exact(&header.header().program)?,
        layout,
        tokenizer_id: Hash64::from_bytes(header.header().tokenizer_id),
    })
}
fn cheap_inputs(i: MetadataOwnerInputsV2<'_>) -> Result<(), M> {
    let c = i.original_context;
    if c.version != 2
        || c.network_id.is_empty()
        || c.network_id.len() > PALW_V2_MAX_NETWORK_ID_BYTES
        || c.declared_prefill_tokens == 0
        || c.exact_decode_tokens == 0
        || c.shape_profile_id != i.expected_class
        || c.max_context_tokens != i.registered_max_context
        || !matches!(c.declared_prefill_tokens.checked_add(c.exact_decode_tokens),Some(n) if n<=c.max_context_tokens)
    {
        return Err(M::Pin);
    }
    if let Some(l) = i.expected_layout
        && (l.commit_tiles.len() > MAX_BLOCKS * MAX_NODES_PER_BLOCK || l.state_tiles.len() > MAX_STATES)
    {
        return Err(M::Limit);
    }
    Ok(())
}
fn hash_quote(header: &BoundedPalwTirHeaderV1, i: MetadataOwnerInputsV2<'_>) -> Result<(u64, u64), M> {
    let (c, s) = match header.layout().map_err(|_| M::Header)? {
        Some(l) => (l.commit_tiles().len(), l.state_tiles().len()),
        None => {
            let l = i.expected_layout.ok_or(M::MissingLayout)?;
            (l.commit_tiles.len(), l.state_tiles.len())
        }
    };
    let layout = add(22, mul(add(n(c)?, n(s)?)?, 4)?)?;
    let context = add(2 + 4 + 11 * 64 + 32 + 3 * 4, n(i.original_context.network_id.len())?)?;
    // Existing Borsh to_vec starts with 1024; CanonicalWriter with 512.
    // Both writers are conservatively retained together, even though sequential.
    let scratch = add(vec_req::<u8>(layout.max(1024))?, vec_req::<u8>(context.max(512))?)?;
    // Actual program/layout/context preimages, plus the fixed class-id writer.
    Ok((add(add(n(header.header().program.len())?, layout)?, add(context, 2 + 4 * 64)?)?, scratch))
}
/// Own a single open native File and all metadata needed by the ordinary CPU
/// API, without accepting/generating ANY trace binding. The independent class,
/// artifact, registered tokenizer/max-context and original context are pins,
/// never inferred expectations from the candidate header. No payload IO here.
///
/// The callback MUST use the same actual host ledger as subsequent parameters
/// and runs. A real configured M lease is obtained before ANY backend metadata,
/// header quote/parse, Source/Space/compiler or identity-encoding allocation.
/// Source native envelopes are FULL admitted requests, not owner struct deltas.
/// Failure claims/work/IO allowances persist in the supplied budget. Component
/// inventories do not issue second host reservations, nor grant native credit.
/// The caller forbids concurrent file writes/offset aliases; stat observations
/// supplement the parameter loader's actual inventory-root check, not a snapshot.
pub fn load_local_forward_metadata_v2<'a>(
    mut file: File,
    inputs: MetadataOwnerInputsV2<'a>,
    budget: &mut MetadataOwnerBudgetV2,
    reserver: &TirMemoryReserverV1,
) -> Result<LocalForwardMetadataOwnerV2<'a>, M> {
    cheap_inputs(inputs)?;
    require_source_program_local_profile_v1().map_err(|_| M::Unsupported)?;
    let prior = budget.begin(inputs)?;
    let allowance = budget.limits.max_new_owner_bytes;
    let memory = reserver("tir-local-forward-metadata-v2", inputs.expected_class, inputs.expected_execution, allowance)
        .map_err(|_| M::Guard)?;
    let before = stamp(&file)?;
    let base = root_requests()?;
    let external = add(inputs.external_retained_bytes, prior)?;
    let mut hl = budget.limits.header;
    hl.external_retained_bytes = add(external, base)?;
    let hq = quote_configured_same_bytes_file_header_v1(before.bytes, hl).map_err(|_| M::Header)?;
    let header_stage_peak = add(base, hq.new_stage_peak_bytes)?;
    budget.stage(header_stage_peak, external)?;
    budget.charge(hq.read_bytes_upper, hq.parser_work_upper)?;
    let mut io = HeaderIo { file: &mut file, left_calls: budget.limits.max_io_calls_per_attempt, left_bytes: hq.read_bytes_upper };
    io.seek(SeekFrom::Start(0)).map_err(|_| M::File)?;
    let header = read_header_bounded_v1(&mut io, before.bytes, hl).map_err(|_| M::Header)?;
    let actual_header = header.quote(hl).map_err(|_| M::Header)?;
    let h = actual_header.native_bytes;
    let source_stage_peak = add(add(base, h)?, budget.limits.source.max_native_requests)?;
    budget.stage(source_stage_peak, external)?;
    budget.charge(0, source_scan_upper(n(header.header().program.len())?)?)?;
    let source = scan_source_program_local_v1(&header.header().program, budget.limits.source, &mut budget.source)
        .map_err(|_| M::Source)?
        .decode()
        .map_err(|_| M::Source)?;
    let s = source.quote().admitted_native_requests();
    let validation_work = header_validation_work(source.program(), actual_header.work)?;
    cap(validation_work, hl.max_work)?;
    budget.charge(0, validation_work)?;
    budget.stage(add(header_stage_peak, s)?, external)?;
    hl.external_retained_bytes = add(hl.external_retained_bytes, s)?;
    // Temporary borrowed check ends before the owner moves. Recreated only in
    // a non-escaping callback later; no transmuted/self-referential checked DTO.
    header.validate_with_program(source.program(), hl).map_err(|_| M::Header)?;
    let (k, l) = class_quote(&header, inputs.expected_layout)?;
    let (hash_work, hash_scratch) = hash_quote(&header, inputs)?;
    let class_stage_peak = add(add(add(add(base, h)?, s)?, k)?, hash_scratch)?;
    budget.stage(class_stage_peak, external)?;
    budget.charge(0, hash_work)?;
    let class = build_class(&header, inputs.expected_layout)?;
    if class.tokenizer_id != inputs.registered_tokenizer
        || class.layout.max_context != inputs.registered_max_context
        || class.class_id(&inputs.artifact) != inputs.expected_class
        || inputs.original_context.context_hash() != inputs.expected_execution
    {
        return Err(M::Pin);
    }
    let fixed = quote_local_tir_step_space_fixed_requests_v1().map_err(|_| M::Geometry)?;
    // Defensive geometry-quote errors allocate within its exact Core-owned
    // diagnostic census. Admit that envelope BEFORE invoking the getter.
    budget.stage(add(class_stage_peak, fixed.diagnostic_peak_bytes)?, external)?;
    budget.charge(0, add(n(source.program().blocks.len())?, n(source.program().states.len())?)?)?;
    let geometry = quote_local_tir_step_space_additional_v1(&source).map_err(|_| M::Geometry)?;
    // Two real complete Source phases: fresh canonical decode/NF and the
    // unchanged explicit validate inside new(). Original S remains separate.
    let sp = add(
        add(add(add(add(mul(s, 2)?, l)?, geometry)?, fixed.space_root_bytes)?, fixed.diagnostic_peak_bytes)?,
        size_of::<OwnedGeometryV2<'_>>() as u64,
    )?;
    let space_stage_peak = add(class_stage_peak, sp)?;
    budget.stage(space_stage_peak, external)?;
    // This is a symbolic validation visit allowance, not a claim that all
    // NF/compiler CPU is metered by bytes or a runtime deadline guarantee.
    budget.charge(0, mul(source_scan_upper(n(class.program.len())?)?, 2)?)?;
    let space = if source.program().wide_logits_q24() {
        OwnedGeometryV2::Wide(PalwTirStepSpaceV2::new(&class).map_err(|_| M::Geometry)?)
    } else {
        OwnedGeometryV2::Narrow(LocalNarrowForwardSpaceV2 {
            inner: PalwTirStepSpaceV1::new(&class).map_err(|_| M::Geometry)?,
            _ticket: inputs.external_ticket,
        })
    };
    // Reject unsupported final widths/schemes before expensive compilation.
    let p = source.program();
    let dtype = p.blocks[p.schedule.post as usize].nodes[p.logits as usize].out.dtype;
    let scheme = Hash64::from_bytes(p.logits_scheme_id);
    let expected_trace = if p.wide_logits_q24() && dtype == DType::I64 {
        wide::scheme_id()
    } else if scheme == flat_logits_scheme_id_v1() && dtype == DType::I32 {
        kaspa_consensus_core::palw_v2::trace_scheme_id_v2()
    } else if scheme == tiled_logits_scheme_id_v1() && dtype == DType::I32 {
        tiled_logits_scheme_id_v1()
    } else {
        return Err(M::Unsupported);
    };
    if inputs.original_context.trace_scheme_id != expected_trace {
        return Err(M::Pin);
    }
    space.view().job_shape(inputs.original_context).map_err(|_| M::Geometry)?;
    let visits = tir_plan_compile_quote_visits_upper_v2(&source).map_err(|_| M::Compile)?;
    budget.charge(0, visits)?;
    let mut cl = budget.limits.compile;
    // Compiler itself adds original complete S ONCE; exclude it from external.
    cl.external_retained_bytes = add(external, add(add(add(add(base, h)?, k)?, sp)?, hash_scratch)?)?;
    let prepared = prepare_tir_plan_compile_v2(&source, cl).map_err(|_| M::Compile)?;
    let compile = *prepared.quote();
    let compile_stage_peak = compile.operation_peak_bytes().checked_sub(external).ok_or(M::Overflow)?;
    budget.stage(compile_stage_peak, external)?;
    let compiled = prepared.compile().map_err(|_| M::Compile)?;
    let retained = add(add(add(add(add(add(base, h)?, k)?, s)?, sp)?, hash_scratch)?, compile.retained_bytes())?;
    budget.stage(retained, external)?;
    let after = stamp(&file)?;
    if before != after {
        return Err(M::Changed);
    }
    let quote = MetadataOwnerQuoteV2 {
        fixed_roots: base,
        configured_header: hq,
        header_retained: h,
        class_requests: k,
        source_complete: s,
        space_decode: s,
        space_revalidate: s,
        space_layout: l,
        space_geometry: geometry,
        space_fixed: fixed,
        space_peak: sp,
        hash_scratch,
        compile,
        header_stage_peak,
        source_stage_peak,
        class_stage_peak,
        space_stage_peak,
        compile_stage_peak,
        conservative_retained_requests: retained,
        actual_guard_allowance: allowance,
        bridge_external_requests: add(external, allowance)?,
    };
    Ok(LocalForwardMetadataOwnerV2 {
        file,
        header,
        class,
        source,
        space,
        compiled,
        inputs,
        stamp: after,
        header_limits: hl,
        quote,
        validation_work,
        _memory: memory,
    })
}
impl LocalForwardMetadataOwnerV2<'_> {
    pub fn quote(&self) -> MetadataOwnerQuoteV2 {
        self.quote
    }
    pub fn file_stamp(&self) -> MetadataFileStampV2 {
        self.stamp
    }
    /// Actual guard remains live for this WHOLE owner. Revalidation consumes the
    /// persistent supplied budget before looping; no successful callback refund.
    /// HRTB prevents any checked/metadata/FD borrow escaping as R. Owned ordinary
    /// output may escape only after its independent run lease is established.
    /// Taking/replacing arbitrary budget owners is not session authorization.
    pub fn with_inputs<R>(
        &mut self,
        budget: &mut MetadataOwnerBudgetV2,
        body: impl for<'b> FnOnce(&'b mut File, LocalForwardInputsV2<'b>) -> R,
    ) -> Result<R, M> {
        let validations = add(budget.usage.borrowed_validations, 1)?;
        budget.charge(0, self.validation_work)?;
        budget.usage.borrowed_validations = validations;
        if stamp(&self.file)? != self.stamp {
            return Err(M::Changed);
        }
        let checked = self.header.validate_with_program(self.source.program(), self.header_limits).map_err(|_| M::Header)?;
        let inputs = LocalForwardInputsV2 {
            checked: &checked,
            source: &self.source,
            compiled: &self.compiled,
            class: &self.class,
            artifact: self.inputs.artifact,
            expected_class: self.inputs.expected_class,
            space: self.space.view(),
            original_context: self.inputs.original_context,
            expected_execution: self.inputs.expected_execution,
            external_retained_bytes: self.quote.bridge_external_requests,
            external_ticket: &self._memory,
        };
        // No identity hash/parameter reopen/second owned parameter inventory.
        let out = body(&mut self.file, inputs);
        if stamp(&self.file)? != self.stamp {
            return Err(M::Changed);
        }
        Ok(out)
    }
}
