//! Same-descriptor generic PALWTIR1 startup for the explicit wide profile.
//! Independently admitted binding, opaque V2 verification, original context and
//! external tickets remain borrowed. Newly constructed header/program/plan/native
//! parameter owners and the regular File move here. No defaults, path reopen,
//! registry/native activation, model review or computation-credit verdict.
use super::super::cell_ipc_v2::{PreparedWideCellV2, WideCellLimitsV2, process::ResidentWideOpenV2};
use super::{
    OwnedArtifactParamsV2, OwnedParamBudgetV2, OwnedParamInputsV2, OwnedParamLayoutOriginV2, OwnedParamLimitsV2, OwnedParamQuoteV2,
    OwnedParamUsageV2, load_owned_artifact_params_admitted_v2, quote,
};
use crate::{
    TirParams,
    node::memory::{TirMemoryLeaseV1, TirMemoryReserverV1},
    plan::{
        LocalCompiledTirPlanV2, TirPlanCompileLimitsV2, TirPlanCompileQuoteV2, prepare_tir_plan_compile_v2,
        tir_plan_compile_quote_visits_upper_v2,
    },
};
use kaspa_consensus_core::{
    Hash64,
    palw_native_resource_v1::{
        LocalScanWorkBudgetV1, LocalSourceProgramOwnerV1, SourceProgramLocalLimitsV1, require_source_program_local_profile_v1,
        scan_source_program_local_v1,
    },
    palw_tir_step_v1::{PalwTirStepBindingV1, PalwTirVerifiedBindingV2},
    palw_v2::PalwJobContextV2,
};
use misaka_palw_tir::program::{MAX_NODE_INPUTS, MAX_NODES_PER_BLOCK};
use misaka_palw_tir_artifact::bounded_v1::{
    BoundedPalwTirHeaderV1, PalwTirHeaderLimitsV1, read_header_bounded_v1,
    startup_quote::{ConfiguredSameBytesFileHeaderQuoteV1, quote_configured_same_bytes_file_header_v1},
};
use std::{
    fs::{File, Metadata},
    io::{Read, Seek, SeekFrom, Write},
    mem::size_of,
};

/// No backend String or arbitrary provider diagnostic escapes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadedWideErrorV2 {
    UnsupportedProfile,
    ZeroLimit,
    Overflow,
    Limit,
    Pins,
    File,
    NotRegular,
    Changed,
    Guard,
    Header,
    Source,
    Compile,
    Params,
    Prepared,
    Session,
}
/// Fixed terminal server error, preserving all accepted connection charges.
/// Backend diagnostics are dropped locally; no retry implicitly refunds usage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadedWideServeErrorV2 {
    pub kind: LoadedWideErrorV2,
    pub usage: super::super::cell_ipc_v2::WideCellUsageV2,
}
fn zero_session_usage() -> super::super::cell_ipc_v2::WideCellUsageV2 {
    super::super::cell_ipc_v2::WideCellUsageV2 { read_bytes: 0, written_bytes: 0, messages: 0, steps: 0, work: 0, exported_bytes: 0 }
}
use LoadedWideErrorV2 as E;
type Result<T> = std::result::Result<T, E>;
fn plus(a: u64, b: u64) -> Result<u64> {
    a.checked_add(b).ok_or(E::Overflow)
}
fn times(a: u64, b: u64) -> Result<u64> {
    a.checked_mul(b).ok_or(E::Overflow)
}
fn number(a: usize) -> Result<u64> {
    u64::try_from(a).map_err(|_| E::Overflow)
}
fn cap(a: u64, b: u64) -> Result<()> {
    if a > b { Err(E::Limit) } else { Ok(()) }
}
fn fixed() -> Result<u64> {
    // Whole aggregate root plus one native Metadata return object. Component
    // envelopes deliberately keep their inline roots too: a named conservative
    // sum, not a tight stack-layout or allocator/RSS measurement.
    plus(number(size_of::<LoadedWideArtifactV2<'_>>())?, number(size_of::<Metadata>())?)
}

/// Association and external retention are caller preconditions, not identity
/// derived from the candidate file. Verified was admitted for THIS independent
/// expected binding and THIS original context. The external ticket must cover
/// those owners, this budget, callback bookkeeping and independent copies. It is
/// retained by borrow, not interpreted as native authority. No fresh hashes.
#[derive(Clone, Copy)]
pub struct LoadedWideInputsV2<'a> {
    pub expected: &'a PalwTirStepBindingV1,
    pub verified: &'a PalwTirVerifiedBindingV2,
    pub original_context: &'a PalwJobContextV2,
    pub external_retained_bytes: u64,
    pub external_ticket: &'a (dyn Send + Sync),
}
/// Sub-operation external fields MUST be zero: this owner supplies named co-live
/// sums, never raises supplied maxima. Work sums parser currency, scanner steps,
/// quote visits and Params metadata/data work. It does not bound all NF/compile
/// CPU, instructions, elapsed time or OS scheduling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadedWideLimitsV2 {
    pub header: PalwTirHeaderLimitsV1,
    pub source: SourceProgramLocalLimitsV1,
    pub compile: TirPlanCompileLimitsV2,
    pub params: OwnedParamLimitsV2,
    pub max_new_owner_bytes: u64,
    pub max_operation_bytes: u64,
    pub max_attempts: u64,
    pub max_total_read_bytes: u64,
    pub max_total_work: u64,
    pub max_total_source_steps: u64,
    pub max_retained_claims: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoadedWideUsageV2 {
    pub attempts: u64,
    pub charged_read_bytes: u64,
    pub charged_work: u64,
    pub retained_claims: u64,
}
/// No Clone/reset/refund. Each accepted attempt preclaims the ENTIRE finite
/// max_new_owner_bytes allowance before touching File/backend/host callback.
/// Even failed attempts retain that claim: deliberately stronger accounting.
pub struct LoadedWideStartupBudgetV2 {
    limits: LoadedWideLimitsV2,
    usage: LoadedWideUsageV2,
    source: LocalScanWorkBudgetV1,
    params: OwnedParamBudgetV2,
}
impl LoadedWideStartupBudgetV2 {
    pub fn new(l: LoadedWideLimitsV2) -> Result<Self> {
        if l.max_new_owner_bytes == 0
            || l.max_operation_bytes == 0
            || l.max_attempts == 0
            || l.max_total_read_bytes == 0
            || l.max_total_work == 0
            || l.max_total_source_steps == 0
            || l.max_retained_claims == 0
        {
            return Err(E::ZeroLimit);
        }
        if l.header.external_retained_bytes != 0 || l.compile.external_retained_bytes != 0 || l.params.external_retained_bytes != 0 {
            return Err(E::Pins);
        }
        cap(fixed()?, l.max_new_owner_bytes)?;
        let source = LocalScanWorkBudgetV1::new(l.max_total_source_steps).map_err(|_| E::Source)?;
        Ok(Self { limits: l, usage: LoadedWideUsageV2::default(), source, params: OwnedParamBudgetV2::new(l.params) })
    }
    pub fn usage(&self) -> LoadedWideUsageV2 {
        self.usage
    }
    pub fn source_usage(&self) -> (u64, u64, u64, u64, u64) {
        (
            self.source.attempts(),
            self.source.charged_steps(),
            self.source.observed_steps(),
            self.source.charged_reader_bytes(),
            self.source.observed_reader_bytes(),
        )
    }
    pub fn params_usage(&self) -> OwnedParamUsageV2 {
        self.params.usage()
    }
    fn begin(&mut self, i: LoadedWideInputsV2<'_>) -> Result<u64> {
        let prior = self.usage.retained_claims;
        let attempts = plus(self.usage.attempts, 1)?;
        let retained = plus(prior, self.limits.max_new_owner_bytes)?;
        cap(attempts, self.limits.max_attempts)?;
        cap(retained, self.limits.max_retained_claims)?;
        cap(plus(i.external_retained_bytes, retained)?, self.limits.max_operation_bytes)?;
        self.usage.attempts = attempts;
        self.usage.retained_claims = retained;
        Ok(prior)
    }
    fn charge(&mut self, reads: u64, work: u64) -> Result<()> {
        let next_reads = plus(self.usage.charged_read_bytes, reads)?;
        let next_work = plus(self.usage.charged_work, work)?;
        cap(next_reads, self.limits.max_total_read_bytes)?;
        cap(next_work, self.limits.max_total_work)?;
        self.usage.charged_read_bytes = next_reads;
        self.usage.charged_work = next_work;
        Ok(())
    }
    fn stage(&self, new_peak: u64, external: u64, prior: u64) -> Result<u64> {
        cap(new_peak, self.limits.max_new_owner_bytes)?;
        let operation = plus(new_peak, plus(external, prior)?)?;
        cap(operation, self.limits.max_operation_bytes)?;
        Ok(operation)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadedWideFileStampV2 {
    pub device: u64,
    pub inode: u64,
    pub length: u64,
    pub modified_seconds: i64,
    pub modified_nanoseconds: i64,
    pub changed_seconds: i64,
    pub changed_nanoseconds: i64,
}
fn stamp(file: &File) -> Result<LoadedWideFileStampV2> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = file.metadata().map_err(|_| E::File)?;
        if !m.file_type().is_file() {
            return Err(E::NotRegular);
        }
        Ok(LoadedWideFileStampV2 {
            device: m.dev(),
            inode: m.ino(),
            length: m.len(),
            modified_seconds: m.mtime(),
            modified_nanoseconds: m.mtime_nsec(),
            changed_seconds: m.ctime(),
            changed_nanoseconds: m.ctime_nsec(),
        })
    }
    #[cfg(not(unix))]
    {
        let _ = file;
        Err(E::UnsupportedProfile)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadedWideQuoteV2 {
    pub configured_header: ConfiguredSameBytesFileHeaderQuoteV1,
    pub header_native_bytes: u64,
    pub source_requests: u64,
    pub compile: TirPlanCompileQuoteV2,
    pub params: OwnedParamQuoteV2,
    pub retained_bytes: u64,
    /// Bridge floor: WHOLE prepaid startup allowance plus external/prior claims,
    /// deliberately stronger than actual retained typed component envelopes.
    pub operation_retained_bytes: u64,
    pub header_stage_peak: u64,
    pub source_stage_peak: u64,
    pub header_validation_stage_peak: u64,
    pub compile_stage_peak: u64,
    pub params_stage_peak: u64,
    pub reserved_new_owner_allowance: u64,
}
/// No Clone, raw-plan getter, mutable access or owned extraction. Public plan
/// OnceLocks cannot be reached. Prepared APIs still need separate workspace,
/// transport and export reservation. The optional existing host callback lease
/// is acquired before header work and drops after ALL new data. No fabricated
/// guard, global/native reserve, RSS promise or arbitrary-copy tracking exists.
/// Callback bookkeeping/errors must be separately bounded externally.
pub struct LoadedWideArtifactV2<'a> {
    file: File,
    header: BoundedPalwTirHeaderV1,
    source: LocalSourceProgramOwnerV1,
    compiled: LocalCompiledTirPlanV2,
    params: TirParams<'static>,
    inputs: LoadedWideInputsV2<'a>,
    stamp: LoadedWideFileStampV2,
    artifact_root: Hash64,
    layout_origin: OwnedParamLayoutOriginV2,
    quote: LoadedWideQuoteV2,
    _memory: Option<TirMemoryLeaseV1>,
}
impl LoadedWideArtifactV2<'_> {
    pub fn artifact_root(&self) -> Hash64 {
        self.artifact_root
    }
    /// Supplemental metadata consistency only; never an immutable snapshot.
    pub fn recheck_file_stamp(&self) -> Result<()> {
        if stamp(&self.file)? == self.stamp { Ok(()) } else { Err(E::Changed) }
    }
    pub fn source_quote(&self) -> &kaspa_consensus_core::palw_native_resource_v1::SourceProgramLocalQuoteV1 {
        self.source.quote()
    }
    pub fn layout_origin(&self) -> OwnedParamLayoutOriginV2 {
        self.layout_origin
    }
    pub fn original_context(&self) -> &PalwJobContextV2 {
        self.inputs.original_context
    }
    pub fn stamp(&self) -> LoadedWideFileStampV2 {
        self.stamp
    }
    pub fn quote(&self) -> &LoadedWideQuoteV2 {
        &self.quote
    }
    /// Reserve prepared-cell workspace separately first, and include this whole
    /// Loaded backing/external inventory in limits.external_retained_bytes.
    /// Direct CPU methods do not charge session counters; use the existing
    /// session to enforce its connection work/step/IO policy. Same original ctx.
    pub fn prepared_cell(&self, limits: WideCellLimitsV2) -> Result<PreparedWideCellV2<'_>> {
        cap(self.quote.operation_retained_bytes, limits.external_retained_bytes)?;
        PreparedWideCellV2::new(self.inputs.verified, self.inputs.original_context, self.compiled.plan(), &self.params, limits)
            .map_err(|_| E::Prepared)
    }
    /// Borrow original raw program, actual private plan and native values. The
    /// existing process/session separately prepays Open copies/work/wire. No
    /// Tensor mirror or lazy get is introduced here.
    /// The public session route enforces existing cumulative work/step/IO
    /// counters. Caller retains separately admitted IO/deadline adapters and
    /// workspace guard; no std blocking transport default is constructed here.
    pub fn serve(
        &self,
        reader: &mut dyn Read,
        writer: &mut dyn Write,
        limits: WideCellLimitsV2,
    ) -> std::result::Result<super::super::cell_ipc_v2::WideCellUsageV2, LoadedWideServeErrorV2> {
        let prepared = self.prepared_cell(limits).map_err(|kind| LoadedWideServeErrorV2 { kind, usage: zero_session_usage() })?;
        super::super::cell_ipc_v2::serve_wide_v2_for_plan(reader, writer, &prepared, limits).map_err(|error| {
            let usage = error.usage;
            drop(error);
            LoadedWideServeErrorV2 { kind: E::Session, usage }
        })
    }
    pub fn resident_open(&self) -> Result<ResidentWideOpenV2<'_>> {
        ResidentWideOpenV2::new(&self.header.header().program, self.compiled.plan(), &self.params, self.quote.operation_retained_bytes)
            .map_err(|_| E::Prepared)
    }
}
fn source_scan_upper(bytes: u64) -> Result<u64> {
    // Exact current Core four-reader intrinsic upper, failed-step +1 per pass.
    // Scanner currency only; not normal-form or compiler CPU.
    let intrinsic = |x| plus(times(x, 8)?, 4096);
    plus(plus(intrinsic(plus(bytes, 4)?)?, times(intrinsic(bytes)?, 3)?)?, 4)
}
fn validation_work_upper(p: &misaka_palw_tir::TirProgramV1, header_work: u64) -> Result<u64> {
    let nodes = times(number(p.blocks.len())?, MAX_NODES_PER_BLOCK as u64)?;
    let refs = times(nodes, MAX_NODE_INPUTS as u64)?;
    let scans = times(times(number(p.params.len())?, plus(number(p.schedule.layers.len())?, 1)?)?, plus(plus(nodes, refs)?, 1)?)?;
    plus(header_work, times(plus(scans, 1)?, 128)?)
}
fn param_metadata_work(i: OwnedParamInputsV2<'_>) -> Result<u64> {
    let p = i.checked.program();
    let refs = times(times(number(p.blocks.len())?, MAX_NODES_PER_BLOCK as u64)?, MAX_NODE_INPUTS as u64)?;
    let joins = times(times(number(p.params.len())?, plus(number(p.schedule.layers.len())?, 1)?)?, plus(refs, 1)?)?;
    plus(
        plus(times(times(refs, plus(refs, 1)?)?, 2)?, times(joins, 3)?)?,
        plus(number(i.checked.header().program.len())?, number(i.checked.header().tensors.len())?)?,
    )
}
/// Own the caller-open descriptor; never reopen a path or substitute file data
/// for independent pins. Caller excludes concurrent writers/descriptor-offset
/// mutation during startup. Pre/post dev/ino/len/mtime/ctime is supplemental,
/// not an atomic snapshot/authentication; actual owned values are independently
/// matched to expected G14 root. No model execution occurs in this constructor.
pub fn load_wide_artifact_v2<'a>(
    mut file: File,
    inputs: LoadedWideInputsV2<'a>,
    budget: &mut LoadedWideStartupBudgetV2,
    reserver: Option<&TirMemoryReserverV1>,
) -> Result<LoadedWideArtifactV2<'a>> {
    // Private build result: not a supplied support boolean or native authority.
    require_source_program_local_profile_v1().map_err(|_| E::UnsupportedProfile)?;
    if inputs.expected.job_context != *inputs.original_context
        || inputs.original_context.shape_profile_id != inputs.verified.class_id()
    {
        return Err(E::Pins);
    }
    let prior = budget.begin(inputs)?;
    let allowance = budget.limits.max_new_owner_bytes;
    let lease = match reserver {
        Some(reserve) => Some(
            reserve("tir-loaded-wide-artifact-v2", inputs.verified.class_id(), inputs.verified.context_hash(), allowance)
                .map_err(|_| E::Guard)?,
        ),
        None => None,
    };
    let before = stamp(&file)?;
    let base = fixed()?;
    let external = plus(inputs.external_retained_bytes, prior)?;
    let mut hl = budget.limits.header;
    hl.external_retained_bytes = plus(external, base)?;
    let hq = quote_configured_same_bytes_file_header_v1(before.length, hl).map_err(|_| E::Header)?;
    let header_stage_peak = plus(base, hq.new_stage_peak_bytes)?;
    budget.stage(header_stage_peak, inputs.external_retained_bytes, prior)?;
    budget.charge(hq.read_bytes_upper, hq.parser_work_upper)?;
    file.seek(SeekFrom::Start(0)).map_err(|_| E::File)?;
    let header = read_header_bounded_v1(&mut file, before.length, hl).map_err(|_| E::Header)?;
    // The configured header diagnostic envelope still surrounds quote failures.
    let actual_header = header.quote(hl).map_err(|_| E::Header)?;
    let header_native = actual_header.native_bytes;

    let source_stage_peak = plus(plus(base, header_native)?, budget.limits.source.max_native_requests)?;
    budget.stage(source_stage_peak, inputs.external_retained_bytes, prior)?;
    budget.charge(0, source_scan_upper(number(header.header().program.len())?)?)?;
    let source = scan_source_program_local_v1(&header.header().program, budget.limits.source, &mut budget.source)
        .map_err(|_| E::Source)?
        .decode()
        .map_err(|_| E::Source)?;
    let source_requests = source.quote().admitted_native_requests();
    let validation_work = validation_work_upper(source.program(), actual_header.work)?;
    cap(validation_work, hl.max_work)?;
    budget.charge(0, validation_work)?;
    // Include Source during split header validation, both in our whole-stage
    // check and in the existing header API's independent operation check.
    let header_validation_stage_peak = plus(header_stage_peak, source_requests)?;
    budget.stage(header_validation_stage_peak, inputs.external_retained_bytes, prior)?;
    hl.external_retained_bytes = plus(hl.external_retained_bytes, source_requests)?;
    let checked = header.validate_with_program(source.program(), hl).map_err(|_| E::Header)?;

    let visits = tir_plan_compile_quote_visits_upper_v2(&source).map_err(|_| E::Compile)?;
    budget.charge(0, visits)?;
    let mut cl = budget.limits.compile;
    // Source is added automatically by producer; do not add it again here.
    cl.external_retained_bytes = plus(external, plus(base, header_native)?)?;
    let prepared = prepare_tir_plan_compile_v2(&source, cl).map_err(|_| E::Compile)?;
    let compile_quote = *prepared.quote();
    let compile_stage_peak = compile_quote.operation_peak_bytes().checked_sub(external).ok_or(E::Overflow)?;
    budget.stage(compile_stage_peak, inputs.external_retained_bytes, prior)?;
    let compiled = prepared.compile().map_err(|_| E::Compile)?;

    let pin = OwnedParamInputsV2 {
        checked: &checked,
        plan: compiled.plan(),
        expected: inputs.expected,
        verified: inputs.verified,
        original_context: inputs.original_context,
    };
    let metadata_work = param_metadata_work(pin)?;
    budget.charge(0, metadata_work)?;
    // Params prior claims already live inside prior startup allowances. Remove
    // ONLY that duplicated inventory from its nested external accounting.
    let params_prior = budget.params.usage.retained_claims;
    let previous_other = prior.checked_sub(params_prior).ok_or(E::Overflow)?;
    let current_other = plus(base, plus(header_native, plus(source_requests, compile_quote.retained_bytes())?)?)?;
    let mut pl = budget.limits.params;
    pl.external_retained_bytes = plus(inputs.external_retained_bytes, plus(previous_other, current_other)?)?;
    budget.params.limits = pl;
    let own_metadata = budget.params.start(pin).map_err(|_| E::Params)?;
    if own_metadata != metadata_work {
        return Err(E::Params);
    }
    let pq = quote(pin, pl, metadata_work).map_err(|_| E::Params)?;
    let params_stage_peak = plus(current_other, pq.new_owner_peak_bytes)?;
    budget.stage(params_stage_peak, inputs.external_retained_bytes, prior)?;
    budget.charge(pq.payload_bytes, pq.data_work)?;
    budget.params.admit(pq).map_err(|_| E::Params)?;
    // Aggregate actual lease already covers live sum; no second Params lease.
    let owned = load_owned_artifact_params_admitted_v2(&mut file, pin, pq, None).map_err(|_| E::Params)?;
    let OwnedArtifactParamsV2 { params, inputs: _, root, layout_origin, quote: _, _memory: _ } = owned;
    let after = stamp(&file)?;
    if before != after {
        return Err(E::Changed);
    }
    let retained = plus(current_other, pq.retained_bytes)?;
    budget.stage(retained, inputs.external_retained_bytes, prior)?;
    let quote = LoadedWideQuoteV2 {
        configured_header: hq,
        header_native_bytes: header_native,
        source_requests,
        compile: compile_quote,
        params: pq,
        retained_bytes: retained,
        operation_retained_bytes: plus(allowance, external)?,
        header_stage_peak,
        source_stage_peak,
        header_validation_stage_peak,
        compile_stage_peak,
        params_stage_peak,
        reserved_new_owner_allowance: allowance,
    };
    Ok(LoadedWideArtifactV2 {
        file,
        header,
        source,
        compiled,
        params,
        inputs,
        stamp: after,
        artifact_root: root,
        layout_origin,
        quote,
        _memory: lease,
    })
}

#[cfg(feature = "positive-panel")]
mod panel;

#[cfg(test)]
mod tests;
