//! Profile-bound local ordinary CPU output (flat/tiled I32, strict wide I64),
//! with NO trace binding or native qualification.
//! Existing source/compiler/header owners stay independently reserved by borrow.
//! No wire/domain/operation, fees, result admission, saved pass or activation.
/// Same-owned-payload dense comparison; no native authority.
pub mod measurement;
/// Whole same-descriptor trace-free metadata owner; no native authority.
pub mod metadata;
/// Trace-free resident parameters to the existing one-pass wide Panel producer.
#[cfg(feature = "positive-panel")]
mod panel;
use super::*;
use crate::{Elem, NoSink, TirExecutor, plan::LocalCompiledTirPlanV2, with_slice};
use kaspa_consensus_core::{
    palw_attempt_rules_v1::palw_attempt_output_root_v1,
    palw_decode_select_v2::PalwDecodeSamplingV2,
    palw_logits_q24_v2 as wide,
    palw_native_resource_v1::LocalSourceProgramOwnerV1,
    palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_match_v1},
    palw_step_refute::{base0_decode_token_select_v1, flat_logits_scheme_id_v1, tiled_logits_scheme_id_v1},
    palw_tir_class_v1::{PalwTirClassV1, PalwTirLayoutV1},
    palw_tir_step_v1::{PalwTirJobShapeV1, PalwTirStepSpaceV1, PalwTirStepSpaceV2},
    palw_v2::PALW_V2_MAX_NETWORK_ID_BYTES,
};
use misaka_palw_tir::{
    admit::node_cost,
    program::{MAX_BLOCKS, MAX_PROGRAM_BYTES, MAX_STATES},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalForwardErrorV2 {
    Overflow,
    Limit,
    Pins,
    Params,
    Guard,
    Execute,
    Output,
    UnsupportedProfile,
}
use LocalForwardErrorV2 as F;
/// Operational provider quote only. Unknown retention/scratch rejects before
/// any new owner or payload IO; backing aliases/cache/OS deadlines stay external.
pub trait LocalForwardReaderV2: Read + Seek {
    fn retained_request_bytes(&self) -> Option<u64> {
        None
    }
    fn scratch_request_bytes(&self) -> Option<u64> {
        None
    }
}
impl LocalForwardReaderV2 for std::fs::File {
    fn retained_request_bytes(&self) -> Option<u64> {
        Some(size_of::<Self>() as u64)
    }
    fn scratch_request_bytes(&self) -> Option<u64> {
        Some(0)
    }
}
impl LocalForwardReaderV2 for std::io::Cursor<Vec<u8>> {
    fn retained_request_bytes(&self) -> Option<u64> {
        (size_of::<Self>() as u64).checked_add(self.get_ref().capacity() as u64)
    }
    fn scratch_request_bytes(&self) -> Option<u64> {
        Some(0)
    }
}
struct BoundedProvider<'a, R> {
    inner: &'a mut R,
    left: u64,
}
impl<R> BoundedProvider<'_, R> {
    fn call(&mut self) -> std::io::Result<()> {
        if self.left == 0 {
            return Err(std::io::ErrorKind::Other.into());
        }
        self.left -= 1;
        Ok(())
    }
}
impl<R: Read> Read for BoundedProvider<'_, R> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        self.call()?;
        self.inner.read(out)
    }
}
impl<R: Seek> Seek for BoundedProvider<'_, R> {
    fn seek(&mut self, p: SeekFrom) -> std::io::Result<u64> {
        self.call()?;
        self.inner.seek(p)
    }
}
fn plus(a: u64, b: u64) -> Result<u64, F> {
    a.checked_add(b).ok_or(F::Overflow)
}
fn times(a: u64, b: u64) -> Result<u64, F> {
    a.checked_mul(b).ok_or(F::Overflow)
}
fn count(n: usize) -> Result<u64, F> {
    u64::try_from(n).map_err(|_| F::Overflow)
}
fn cap(n: u64, max: u64) -> Result<(), F> {
    if n > max { Err(F::Limit) } else { Ok(()) }
}
fn vec_bytes<T>(n: u64) -> Result<u64, F> {
    times(growing_vec_request_capacity_v1::<T>(n).ok_or(F::Overflow)?, size_of::<T>() as u64)
}
// Existing CanonicalWriter starts with512, then the public growing-Vec envelope.
fn writer_bytes(n: u64) -> Result<u64, F> {
    vec_bytes::<u8>(n.max(512))
}
/// Inferred from actual class program AND original context, never caller flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalForwardProfileV2 {
    FlatI32,
    TiledI32,
    WideI64Q24,
}
impl LocalForwardProfileV2 {
    fn width(self) -> u64 {
        match self {
            Self::FlatI32 | Self::TiledI32 => 4,
            Self::WideI64Q24 => 8,
        }
    }
}
/// Opaque immutable legacy geometry: public mutable V1 DTO fields are not an
/// admission source. Construct fresh canonical geometry under the external
/// WHOLE metadata owner/ticket. No raw mutable accessor/Clone is supplied.
/// Caller must pre-reserve class decode/validation/layout/geometry/error scratch;
/// this constructor is data-only and NOT a metadata quote or native capability.
pub struct LocalNarrowForwardSpaceV2<'a> {
    inner: PalwTirStepSpaceV1,
    _ticket: &'a (dyn Send + Sync),
}
impl<'a> LocalNarrowForwardSpaceV2<'a> {
    pub fn new(class: &PalwTirClassV1, ticket: &'a (dyn Send + Sync)) -> Result<Self, F> {
        if class.version != kaspa_consensus_core::palw_tir_class_v1::PALW_TIR_CLASS_VERSION_V1
            || class.program.len() > MAX_PROGRAM_BYTES
            || class.layout.commit_tiles.len() > MAX_BLOCKS * MAX_NODES_PER_BLOCK
            || class.layout.state_tiles.len() > MAX_STATES
        {
            return Err(F::Pins);
        }
        let inner = PalwTirStepSpaceV1::new(class).map_err(|_| F::Pins)?;
        Ok(Self { inner, _ticket: ticket })
    }
    /// Immutable compatibility view; deep copies need separate ownership.
    pub fn legacy_space(&self) -> &PalwTirStepSpaceV1 {
        &self.inner
    }
}
#[derive(Clone, Copy)]
pub enum LocalForwardSpaceV2<'a> {
    Narrow(&'a LocalNarrowForwardSpaceV2<'a>),
    Wide(&'a PalwTirStepSpaceV2),
}
impl LocalForwardSpaceV2<'_> {
    fn program(&self) -> &misaka_palw_tir::TirProgramV1 {
        match self {
            Self::Narrow(s) => &s.inner.program,
            Self::Wide(s) => s.program(),
        }
    }
    fn layout(&self) -> &PalwTirLayoutV1 {
        match self {
            Self::Narrow(s) => &s.inner.layout,
            Self::Wide(s) => s.layout(),
        }
    }
    fn occurrences(&self) -> &[(u8, Option<u16>)] {
        match self {
            Self::Narrow(s) => s.inner.occurrences(),
            Self::Wide(s) => s.occurrences(),
        }
    }
    fn window(&self, block: usize) -> u64 {
        match self {
            Self::Narrow(s) => u64::from(s.inner.info.blocks[block].window.unwrap_or(1)),
            Self::Wide(s) => u64::from(s.info().blocks[block].window.unwrap_or(1)),
        }
    }
    fn job_shape(&self, ctx: &PalwJobContextV2) -> Result<PalwTirJobShapeV1, F> {
        match self {
            Self::Narrow(s) => s.inner.job_shape(ctx).map_err(|_| F::Pins),
            Self::Wide(s) => s.job_shape(ctx).map_err(|_| F::Pins),
        }
    }
    fn executor_bytes(&self, plan: &TirPlan, ctx: &PalwJobContextV2) -> Result<u64, F> {
        match self {
            Self::Narrow(s) => memory::tir_executor_storage_need_v1(&s.inner, plan, ctx).map_err(|_| F::Limit),
            Self::Wide(s) => memory::tir_executor_storage_need_wide_q24_v2(s, plan, ctx).map_err(|_| F::Limit),
        }
    }
    pub fn wide_space(&self) -> Option<&PalwTirStepSpaceV2> {
        match self {
            Self::Wide(s) => Some(s),
            _ => None,
        }
    }
}
fn profile(i: LocalForwardInputsV2<'_>) -> Result<LocalForwardProfileV2, F> {
    let p = i.source.program();
    let out = &p.blocks[p.schedule.post as usize].nodes[p.logits as usize].out;
    let scheme = Hash64::from_bytes(p.logits_scheme_id);
    let ctx = i.original_context;
    let (resolved, expected_trace) = if scheme == flat_logits_scheme_id_v1() && out.dtype == DType::I32 {
        (LocalForwardProfileV2::FlatI32, kaspa_consensus_core::palw_v2::trace_scheme_id_v2())
    } else if scheme == tiled_logits_scheme_id_v1() && out.dtype == DType::I32 {
        (LocalForwardProfileV2::TiledI32, tiled_logits_scheme_id_v1())
    } else if p.wide_logits_q24() && out.dtype == DType::I64 {
        (LocalForwardProfileV2::WideI64Q24, wide::scheme_id())
    } else {
        return Err(F::UnsupportedProfile);
    };
    if ctx.version != 2 || ctx.trace_scheme_id != expected_trace {
        return Err(F::UnsupportedProfile);
    }
    if !matches!(ctx.declared_prefill_tokens.checked_add(ctx.exact_decode_tokens),Some(n) if n<=ctx.max_context_tokens) {
        return Err(F::Pins);
    }
    match (resolved, i.space) {
        (LocalForwardProfileV2::WideI64Q24, LocalForwardSpaceV2::Wide(_)) => {}
        (LocalForwardProfileV2::FlatI32 | LocalForwardProfileV2::TiledI32, LocalForwardSpaceV2::Narrow(_)) => {}
        _ => return Err(F::UnsupportedProfile),
    }
    Ok(resolved)
}
/// No trace/step/execution-tree roots. Expected class/artifact/job MUST come from
/// independent metadata, not the candidate header. The external ticket is a
/// trusted CALLER retention contract, not a native capability or authentication.
/// All header/source/compiler/class/space/context/provider/ticket/error owners
/// and aliases remain reserved for this whole borrowed lifetime. Same checked FD
/// and bounded provider internals/stamp checks are caller preconditions.
#[derive(Clone, Copy)]
pub struct LocalForwardInputsV2<'a> {
    pub checked: &'a PalwTirCheckedHeaderV1<'a>,
    pub source: &'a LocalSourceProgramOwnerV1,
    pub compiled: &'a LocalCompiledTirPlanV2,
    pub class: &'a PalwTirClassV1,
    pub artifact: Hash64,
    pub expected_class: Hash64,
    pub space: LocalForwardSpaceV2<'a>,
    pub original_context: &'a PalwJobContextV2,
    pub expected_execution: Hash64,
    pub external_retained_bytes: u64,
    pub external_ticket: &'a (dyn Send + Sync),
}
#[derive(Clone, Copy, Debug)]
pub struct LocalForwardLoadLimitsV2 {
    /// External field MUST be zero; this aggregate supplies the complete sum.
    pub params: OwnedParamLimitsV2,
    pub max_new_owner_bytes: u64,
    pub max_operation_bytes: u64,
    pub max_retained_requests: u64,
    pub max_provider_calls_per_attempt: u64,
    pub max_total_provider_calls: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LocalForwardLoadUsageV2 {
    pub params: OwnedParamUsageV2,
    pub retained_requests: u64,
    pub provider_calls_reserved: u64,
}
/// Owner-local persistent accounting. Creating another budget is not native
/// permission or an authorization to reset an actual service/session budget.
pub struct LocalForwardLoadBudgetV2 {
    limits: LocalForwardLoadLimitsV2,
    params: OwnedParamBudgetV2,
    retained_requests: u64,
    provider_calls_reserved: u64,
}
impl LocalForwardLoadBudgetV2 {
    pub fn new(limits: LocalForwardLoadLimitsV2) -> Result<Self, F> {
        if limits.params.external_retained_bytes != 0
            || [
                limits.max_new_owner_bytes,
                limits.max_operation_bytes,
                limits.max_retained_requests,
                limits.max_provider_calls_per_attempt,
                limits.max_total_provider_calls,
            ]
            .contains(&0)
        {
            return Err(F::Limit);
        }
        Ok(Self { params: OwnedParamBudgetV2::new(limits.params), limits, retained_requests: 0, provider_calls_reserved: 0 })
    }
    pub fn usage(&self) -> LocalForwardLoadUsageV2 {
        LocalForwardLoadUsageV2 {
            params: self.params.usage(),
            retained_requests: self.retained_requests,
            provider_calls_reserved: self.provider_calls_reserved,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalForwardLoadQuoteV2 {
    pub params: OwnedParamQuoteV2,
    pub metadata_hash_bytes: u64,
    pub metadata_hash_scratch: u64,
    pub requested_bytes: u64,
    /// Original metadata/provider owners plus ALL retained load-budget claims,
    /// including earlier successful or failed attempts. Not a new host lease.
    pub operation_retained_bytes: u64,
}
/// Native Params owned and root-checked; no Clone, raw-plan/Params export or
/// full-binding input. This is NOT self-owned generic startup: all borrowed
/// bounded source/header/compiled/class/job owners must stay reserved externally.
pub struct LocalWideForwardArtifactV2<'a> {
    params: TirParams<'static>,
    inputs: LocalForwardInputsV2<'a>,
    job: PalwTirJobShapeV1,
    profile: LocalForwardProfileV2,
    quote: LocalForwardLoadQuoteV2,
    layout_origin: OwnedParamLayoutOriginV2,
    _memory: TirMemoryLeaseV1,
}
impl LocalWideForwardArtifactV2<'_> {
    pub fn profile(&self) -> LocalForwardProfileV2 {
        self.profile
    }
    pub fn original_context(&self) -> &PalwJobContextV2 {
        self.inputs.original_context
    }
    pub fn artifact_root(&self) -> Hash64 {
        self.inputs.artifact
    }
    pub fn load_quote(&self) -> LocalForwardLoadQuoteV2 {
        self.quote
    }
    pub fn layout_origin(&self) -> OwnedParamLayoutOriginV2 {
        self.layout_origin
    }
}
fn cheap(i: LocalForwardInputsV2<'_>) -> Result<(), F> {
    if i.class.version != kaspa_consensus_core::palw_tir_class_v1::PALW_TIR_CLASS_VERSION_V1
        || !cfg!(target_pointer_width = "64")
        || i.class.program.len() > MAX_PROGRAM_BYTES
        || i.class.layout.commit_tiles.len() > MAX_BLOCKS * MAX_NODES_PER_BLOCK
        || i.class.layout.state_tiles.len() > MAX_STATES
        || i.original_context.network_id.is_empty()
        || i.original_context.network_id.len() > PALW_V2_MAX_NETWORK_ID_BYTES
    {
        return Err(F::Pins);
    }
    Ok(())
}
fn metadata_quote(i: LocalForwardInputsV2<'_>) -> Result<(u64, u64), F> {
    // Layout:u16+3*u32+two Vec<u32> lengths and payload. Existing writer sizes.
    let layout = plus(22, times(plus(count(i.class.layout.commit_tiles.len())?, count(i.class.layout.state_tiles.len())?)?, 4)?)?;
    let context = plus(2 + 4 + 11 * 64 + 32 + 3 * 4, count(i.original_context.network_id.len())?)?;
    let hash_bytes = plus(plus(count(i.class.program.len())?, layout)?, plus(context, 2 + 4 * 64)?)?;
    // Existing borsh::to_vec helper starts1024; context CanonicalWriter starts512.
    Ok((hash_bytes, plus(vec_bytes::<u8>(layout.max(1024))?, writer_bytes(context)?)?))
}
fn literal_pins(i: LocalForwardInputsV2<'_>) -> Result<OwnedParamLayoutOriginV2, F> {
    let h = i.checked.header();
    if h.program != i.class.program
        || i.checked.program() != i.source.program()
        || i.source.program() != &i.compiled.plan().program
        || i.space.program() != i.source.program()
        || i.space.layout() != &i.class.layout
        || h.tokenizer_id != i.class.tokenizer_id.as_bytes()
    {
        return Err(F::Pins);
    }
    match i.checked.layout().map_err(|_| F::Pins)? {
        None => Ok(OwnedParamLayoutOriginV2::IndependentlyExpectedOnly),
        Some(v) => {
            let x = &i.class.layout;
            if v.version != x.version
                || v.max_context != x.max_context
                || v.checkpoint_interval != x.checkpoint_interval
                || v.h_tile != x.h_tile
                || !v.commit_tiles().eq(x.commit_tiles.iter().copied())
                || !v.state_tiles().eq(x.state_tiles.iter().copied())
            {
                return Err(F::Pins);
            }
            Ok(OwnedParamLayoutOriginV2::DeclaredInFile)
        }
    }
}
/// Loads from the same checked descriptor under a REQUIRED same-ledger lease.
/// Accepted attempts/work/read/retention are never refunded after guard, pins,
/// IO or root failure. Final inventory is recomputed from owned native values.
/// Input construction/copies and provider scratch/calls remain externally owned.
pub fn load_local_wide_forward_v2<'a, R: LocalForwardReaderV2>(
    reader: &mut R,
    inputs: LocalForwardInputsV2<'a>,
    budget: &mut LocalForwardLoadBudgetV2,
    reserver: &TirMemoryReserverV1,
) -> Result<LocalWideForwardArtifactV2<'a>, F> {
    cheap(inputs)?;
    let provider_retained = reader.retained_request_bytes().ok_or(F::Limit)?;
    let provider_scratch = reader.scratch_request_bytes().ok_or(F::Limit)?;
    // This is ONLY a known partial floor: Source.owner_requests is its root
    // struct delta, NOT the full decoded Source heap. The actual external
    // ticket must cover every deep source/header/class/space/plan/provider and
    // alias allocation, not merely this floor or any nonzero declared number.
    let external_floor = plus(inputs.source.quote().owner_requests(), inputs.compiled.quote().retained_bytes())?;
    cap(plus(external_floor, provider_retained)?, inputs.external_retained_bytes)?;
    let base_work = budget.params.start_checked(inputs.checked).map_err(|_| F::Limit)?;
    let (hash_bytes, hash_scratch) = metadata_quote(inputs)?;
    let work = plus(base_work, hash_bytes)?;
    cap(work, budget.limits.params.max_work_per_attempt)?;
    let total_work = plus(budget.params.usage.work, hash_bytes)?;
    cap(total_work, budget.limits.params.max_total_work)?;
    budget.params.usage.work = total_work;
    let mut pl = budget.limits.params;
    let q = quote_checked(inputs.checked, pl, work).map_err(|_| F::Params)?;
    let extra = plus(size_of::<LocalWideForwardArtifactV2<'_>>() as u64, plus(hash_scratch, provider_scratch)?)?;
    let requested = plus(q.new_owner_peak_bytes, extra)?;
    cap(requested, budget.limits.max_new_owner_bytes)?;
    cap(plus(plus(requested, budget.retained_requests)?, inputs.external_retained_bytes)?, budget.limits.max_operation_bytes)?;
    let next = plus(budget.retained_requests, requested)?;
    cap(next, budget.limits.max_retained_requests)?;
    let prior_other = budget.retained_requests.checked_sub(budget.params.usage.retained_claims).ok_or(F::Overflow)?;
    pl.external_retained_bytes = plus(inputs.external_retained_bytes, plus(prior_other, extra)?)?;
    let calls = plus(budget.provider_calls_reserved, budget.limits.max_provider_calls_per_attempt)?;
    cap(calls, budget.limits.max_total_provider_calls)?;
    budget.params.limits = pl;
    budget.params.admit(q).map_err(|_| F::Limit)?;
    budget.retained_requests = next;
    budget.provider_calls_reserved = calls;
    let lease = reserver("tir-local-wide-forward-load-v2", inputs.expected_class, inputs.expected_execution, requested)
        .map_err(|_| F::Guard)?;
    let origin = literal_pins(inputs)?;
    if inputs.class.class_id(&inputs.artifact) != inputs.expected_class
        || inputs.original_context.shape_profile_id != inputs.expected_class
        || inputs.original_context.context_hash() != inputs.expected_execution
    {
        return Err(F::Pins);
    }
    let resolved = profile(inputs)?;
    let job = inputs.space.job_shape(inputs.original_context)?;
    let mut provider = BoundedProvider { inner: reader, left: budget.limits.max_provider_calls_per_attempt };
    let (params, _) =
        read_owned_payload_v2(&mut provider, inputs.checked, inputs.compiled.plan(), inputs.artifact, q).map_err(|_| F::Params)?;
    Ok(LocalWideForwardArtifactV2 {
        params,
        inputs,
        job,
        profile: resolved,
        quote: LocalForwardLoadQuoteV2 {
            params: q,
            metadata_hash_bytes: hash_bytes,
            metadata_hash_scratch: hash_scratch,
            requested_bytes: requested,
            operation_retained_bytes: plus(inputs.external_retained_bytes, next)?,
        },
        layout_origin: origin,
        _memory: lease,
    })
}
#[derive(Clone, Copy, Debug)]
pub struct LocalWideForwardLimitsV2 {
    pub max_positions: u32,
    pub max_generated: u32,
    pub max_logits_lanes: u64,
    pub max_work_per_attempt: u64,
    /// Full working + small output guard handoff overlap, before work release.
    pub max_requested_bytes: u64,
    pub max_operation_bytes: u64,
    pub max_attempts: u64,
    pub max_total_work: u64,
    pub max_retained_requests: u64,
    /// Loaded backing+all external input/prompt/provider/callback/error owners.
    pub external_retained_bytes: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LocalWideForwardUsageV2 {
    pub attempts: u64,
    pub work: u64,
    pub retained_requests: u64,
}
/// No reset/refund: the actual session must retain this owner across attempts.
/// A newly constructed local budget grants no native/service budget authority.
pub struct LocalWideForwardBudgetV2 {
    limits: LocalWideForwardLimitsV2,
    usage: LocalWideForwardUsageV2,
}
impl LocalWideForwardBudgetV2 {
    pub fn new(limits: LocalWideForwardLimitsV2) -> Result<Self, F> {
        if [
            u64::from(limits.max_positions),
            u64::from(limits.max_generated),
            limits.max_logits_lanes,
            limits.max_work_per_attempt,
            limits.max_requested_bytes,
            limits.max_operation_bytes,
            limits.max_attempts,
            limits.max_total_work,
            limits.max_retained_requests,
        ]
        .contains(&0)
        {
            return Err(F::Limit);
        }
        Ok(Self { limits, usage: LocalWideForwardUsageV2::default() })
    }
    pub fn usage(&self) -> LocalWideForwardUsageV2 {
        self.usage
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalWideForwardQuoteV2 {
    pub positions: u32,
    pub generated: u32,
    pub vocabulary: u64,
    pub node_visits: u64,
    pub work: u64,
    pub executor_bytes: u64,
    pub row_bytes: u64,
    pub generated_bytes: u64,
    pub hashing_scratch: u64,
    pub requested_bytes: u64,
}
/// Returned ordinary data retains ONLY this typed output root plus the exact
/// generated-ID Vec backing. exact() checks capacity == generated count. There
/// is no retained executor/row, plan, Params or borrowed context in this object.
/// The opaque lease HANDLE is included in sizeof; callback-owned guard/ledger
/// backing remains its required host contract (the concrete caller charges its
/// own Reservation backing on the same ledger before allocating that guard).
/// This arithmetic is data, not native admission or a physical RSS quote.
pub fn local_forward_retained_output_requests_v2(generated: u32) -> u64 {
    size_of::<LocalWideForwardOutputV2>() as u64 + u64::from(generated) * size_of::<u32>() as u64
}
impl LocalWideForwardQuoteV2 {
    /// Unchanged requested_bytes is the FULL conservative working reservation.
    pub fn retained_output_requests(&self) -> u64 {
        local_forward_retained_output_requests_v2(self.generated)
    }
    /// Both real guards coexist briefly at handoff. Charge that conservative
    /// overlap before CPU; accepted work/request budgets are never refunded.
    pub fn handoff_peak_requests(&self) -> Result<u64, F> {
        plus(self.requested_bytes, self.retained_output_requests())
    }
}
/// Ordinary local data only: no serialization, trace roots, native receipt,
/// Result constructor or success capability. Copies require separate ownership.
pub struct LocalWideForwardOutputV2 {
    generated: Vec<u32>,
    output_root: Hash64,
    execution: Hash64,
    quote: LocalWideForwardQuoteV2,
    _memory: TirMemoryLeaseV1,
}
impl LocalWideForwardOutputV2 {
    pub fn generated(&self) -> &[u32] {
        &self.generated
    }
    /// EXACT existing palw_attempt_output_root_v1; no new recipe/domain.
    pub fn output_root(&self) -> Hash64 {
        self.output_root
    }
    pub fn execution(&self) -> Hash64 {
        self.execution
    }
    pub fn quote(&self) -> LocalWideForwardQuoteV2 {
        self.quote
    }
    /// Actual reservation that survives CPU completion, NOT the working peak.
    pub fn retained_requested_bytes(&self) -> u64 {
        self.quote.retained_output_requests()
    }
}
impl LocalWideForwardArtifactV2<'_> {
    fn quote_scan_work(&self) -> Result<u64, F> {
        plus(
            count(self.inputs.class.program.len())?,
            times(count(self.inputs.space.occurrences().len())?, MAX_NODES_PER_BLOCK as u64)?,
        )
    }
    /// Borrowed allocation-free success path. Direct quote calls are caller
    /// work, not a session-run receipt. forward_local prepays its own quote scan.
    /// Conservative existing executor envelope retains commit/export scratch;
    /// this is no tight RAM/RSS or native admission claim.
    pub fn forward_quote(&self, l: LocalWideForwardLimitsV2) -> Result<LocalWideForwardQuoteV2, F> {
        cap(u64::from(self.job.positions), u64::from(l.max_positions))?;
        cap(u64::from(self.inputs.original_context.exact_decode_tokens), u64::from(l.max_generated))?;
        let p = self.inputs.source.program();
        let out = &p.blocks[p.schedule.post as usize].nodes[p.logits as usize].out;
        let expected_dtype = if self.profile == LocalForwardProfileV2::WideI64Q24 { DType::I64 } else { DType::I32 };
        if out.dtype != expected_dtype {
            return Err(F::Pins);
        }
        let vocab = out.elements_at(1);
        if vocab == 0 || vocab > u64::from(u32::MAX) {
            return Err(F::Pins);
        }
        cap(vocab, l.max_logits_lanes)?;
        let mut cost = 0;
        let mut nodes = 0;
        for &(b, _) in self.inputs.space.occurrences() {
            let h = self.inputs.space.window(b as usize);
            for ni in 0..p.blocks[b as usize].nodes.len() {
                nodes = plus(nodes, 1)?;
                let c = node_cost(p, b as usize, ni, h);
                for v in [c.macs, c.elementwise, c.transcendentals, c.bytes_read, c.bytes_written] {
                    if v == u64::MAX {
                        return Err(F::Limit);
                    }
                    cost = plus(cost, v)?
                }
            }
        }
        let positions = u64::from(self.job.positions);
        let decode = u64::from(self.inputs.original_context.exact_decode_tokens);
        let prefill = u64::from(self.job.prefill);
        let context = count(borsh::object_length(self.inputs.original_context).map_err(|_| F::Pins)?)?;
        let encoded = count(borsh::object_length(p).map_err(|_| F::Pins)?)?;
        // Explicit symbolic byte/element visits, not CPU instructions. First
        // executor refinement may read EVERY native parameter for range memos,
        // including whole embeddings even when execution gathers only a row.
        let metadata = plus(times(encoded, 8)?, times(context, 4)?)?;
        let host =
            plus(plus(times(times(vocab, decode)?, 128)?, times(prefill, 64)?)?, plus(metadata, self.quote.params.payload_bytes)?)?;
        let states = count(self.inputs.compiled.plan().instances.len())?;
        let structural = times(times(positions, plus(count(self.inputs.space.occurrences().len())?, plus(states, 1)?)?)?, 64)?;
        let work = plus(self.quote_scan_work()?, plus(times(cost, positions)?, plus(host, structural)?)?)?;
        cap(work, l.max_work_per_attempt)?;
        let executor = self.inputs.space.executor_bytes(self.inputs.compiled.plan(), self.inputs.original_context)?;
        let row = times(vocab, self.profile.width())?;
        let generated = times(decode, 4)?;
        let output_writer = writer_bytes(plus(132, generated)?)?;
        let hash =
            plus(writer_bytes(context)?, plus(writer_bytes(plus(4, times(prefill, 4)?)?)?, plus(output_writer, writer_bytes(4)?)?)?)?;
        let requested = plus(size_of::<LocalWideForwardOutputV2>() as u64, plus(executor, plus(row, plus(generated, hash)?)?)?)?;
        let handoff = plus(requested, local_forward_retained_output_requests_v2(self.inputs.original_context.exact_decode_tokens))?;
        cap(handoff, l.max_requested_bytes)?;
        Ok(LocalWideForwardQuoteV2 {
            positions: self.job.positions,
            generated: self.inputs.original_context.exact_decode_tokens,
            vocabulary: vocab,
            node_visits: times(nodes, positions)?,
            work,
            executor_bytes: executor,
            row_bytes: row,
            generated_bytes: generated,
            hashing_scratch: hash,
            requested_bytes: requested,
        })
    }
    /// ONE ordinary CPU pass for the resolved original profile. No observer, all-row retention, leaf enumeration,
    /// preimage/hash tree, checkpoint export, trace or evidence tape. NoSink's
    /// existing noncontiguous commit scratch is retained under the quote.
    /// Required guard is acquired before hashes, executor/range refinement and
    /// output allocation. Accepted attempts/work/retention never reset on failure.
    pub fn forward_local(
        &self,
        prompt: &[u32],
        budget: &mut LocalWideForwardBudgetV2,
        reserver: &TirMemoryReserverV1,
    ) -> Result<LocalWideForwardOutputV2, F> {
        let l = budget.limits;
        let attempts = plus(budget.usage.attempts, 1)?;
        cap(attempts, l.max_attempts)?;
        let scans = self.quote_scan_work()?;
        let prior_work = plus(budget.usage.work, scans)?;
        cap(prior_work, l.max_total_work)?;
        cap(scans, l.max_work_per_attempt)?;
        budget.usage.attempts = attempts;
        budget.usage.work = prior_work;
        let q = self.forward_quote(l)?;
        cap(q.work, l.max_work_per_attempt)?;
        let remaining_work = q.work.checked_sub(scans).ok_or(F::Overflow)?;
        let total_work = plus(prior_work, remaining_work)?;
        cap(total_work, l.max_total_work)?;
        let handoff = q.handoff_peak_requests()?;
        let retained = plus(budget.usage.retained_requests, handoff)?;
        cap(retained, l.max_retained_requests)?;
        cap(self.quote.operation_retained_bytes, l.external_retained_bytes)?;
        cap(plus(plus(l.external_retained_bytes, budget.usage.retained_requests)?, handoff)?, l.max_operation_bytes)?;
        budget.usage.work = total_work;
        budget.usage.retained_requests = retained;
        let work_lease =
            reserver("tir-local-wide-forward-run-v2", self.inputs.expected_class, self.inputs.expected_execution, q.requested_bytes)
                .map_err(|_| F::Guard)?;
        let ctx = self.inputs.original_context;
        if prompt.len() != ctx.declared_prefill_tokens as usize
            || prompt.iter().any(|id| *id >= self.inputs.source.program().token_bound)
            || !prompt_token_ids_match_v1(PalwPromptIdsFormV1::Flat, prompt, &ctx.prompt_token_ids_hash)
        {
            return Err(F::Pins);
        }
        let mut exec = TirExecutor::new(self.inputs.compiled.plan(), &self.params).map_err(|_| F::Execute)?;
        let mut ids = exact::<u32>(q.generated as usize).map_err(|_| F::Limit)?;
        let mut row = SelectionRowV2::new(self.profile, usize::try_from(q.vocabulary).map_err(|_| F::Overflow)?)?;
        for a in 0..self.job.positions {
            let token = if a < self.job.prefill { prompt[a as usize] } else { ids[(a - self.job.prefill) as usize] };
            let post = self.job.runs_post(a);
            exec.step_opt(token, &mut NoSink, post).map_err(|_| F::Execute)?;
            if post {
                let (_, logits) = exec.logits();
                if logits.len() as u64 != q.vocabulary {
                    return Err(F::Output);
                }
                let id = row.select(logits, ids.len() as u32)?;
                ids.push(u32::try_from(id).map_err(|_| F::Output)?);
            }
        }
        if ids.len() != q.generated as usize {
            return Err(F::Output);
        }
        let output_root = palw_attempt_output_root_v1(ctx, &ids);
        // Keep the full original work guard through ALL CPU/hash activity. The
        // executor and selection row actually release their backing BEFORE its
        // handoff; generated IDs/root remain covered by that full work guard.
        drop(row);
        drop(exec);
        let output_lease = reserver(
            "tir-local-wide-forward-output-v2",
            self.inputs.expected_class,
            self.inputs.expected_execution,
            q.retained_output_requests(),
        )
        .map_err(|_| F::Guard)?;
        // Real output guard is now live; no allocation/reset/permission replaces
        // the old guard. Late denial returns no result, with spent budgets kept.
        drop(work_lease);
        Ok(LocalWideForwardOutputV2 {
            generated: ids,
            output_root,
            execution: self.inputs.expected_execution,
            quote: q,
            _memory: output_lease,
        })
    }
}

enum SelectionRowV2 {
    Narrow(Vec<i32>),
    Wide(Vec<i64>),
}
impl SelectionRowV2 {
    fn new(p: LocalForwardProfileV2, n: usize) -> Result<Self, F> {
        Ok(match p {
            LocalForwardProfileV2::FlatI32 | LocalForwardProfileV2::TiledI32 => Self::Narrow(exact(n).map_err(|_| F::Limit)?),
            LocalForwardProfileV2::WideI64Q24 => Self::Wide(exact(n).map_err(|_| F::Limit)?),
        })
    }
    fn select(&mut self, logits: Slice<'_>, position: u32) -> Result<usize, F> {
        match self {
            Self::Narrow(row) => {
                if logits.dtype() == DType::Idx {
                    return Err(F::Output);
                }
                row.clear();
                if logits.len() > row.capacity() || logits.is_empty() {
                    return Err(F::Output);
                }
                with_slice!(logits,values=>{for v in values{row.push(i32::try_from(v.to_i128()).map_err(|_|F::Output)?);}});
                Ok(base0_decode_token_select_v1(row))
            }
            Self::Wide(row) => {
                row.clear();
                if logits.len() > row.capacity() || logits.is_empty() {
                    return Err(F::Output);
                }
                with_slice!(logits,values=>{for v in values{row.push(i64::try_from(v.to_i128()).map_err(|_|F::Output)?);}});
                wide::select(row, &PalwDecodeSamplingV2::GREEDY, position).ok_or(F::Output)
            }
        }
    }
}
#[cfg(test)]
mod row_tests {
    use super::*;
    #[test]
    fn narrow_row_retains_legacy_idx_refusal_and_checks_storage_values_without_clipping() {
        let mut row = SelectionRowV2::new(LocalForwardProfileV2::TiledI32, 4).unwrap();
        assert_eq!(row.select(Slice::Idx(&[0, 1, 2, 3]), 0), Err(F::Output));
        assert_eq!(row.select(Slice::Idx(&[u32::MAX, 0]), 0), Err(F::Output));
        assert_eq!(row.select(Slice::I64(&[i32::MIN as i64, i32::MAX as i64, i32::MAX as i64, 0]), 0), Ok(1));
        assert_eq!(row.select(Slice::I64(&[i32::MAX as i64 + 1]), 0), Err(F::Output));
        assert_eq!(row.select(Slice::I16(&[-2, 9, 9, 0]), 0), Ok(1));
        assert_eq!(row.select(Slice::I32(&[]), 0), Err(F::Output));
    }
}
