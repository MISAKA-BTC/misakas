//! Opt-in producer-owned logical request quote for unchanged TirPlan::compile.
//! Accepts an immutable canonical/NF Source owner, not a mutable public plan.
//! Quote walks do not compile/validate/encode; consuming Prepared invokes the
//! unchanged compiler and its second validation exactly once. No model execution,
//! lease, registry or wire change.
//! Source owner's full envelope remains independently retained; one SECOND full
//! admitted envelope conservatively funds compile's unchanged validate call.

use super::{BlockPlan, NodePlan, StateInstance, TirPlan};
use crate::{
    Buf,
    tiers::{TirRowGatherV1, TirRowGroupV1},
};
use kaspa_consensus_core::palw_native_resource_v1::{
    LocalSourceProgramOwnerV1, btree_node_envelope_bytes_v1, growing_vec_request_capacity_v1,
};
use misaka_palw_tir::{
    DType, Dim, Prim, Ref, TensorType, TirError, TirErrorKind,
    interval::Interval,
    program::{Block, ConstDecl, Node, ParamDecl, StateDecl, TirProgramV1},
    validate::BlockInfo,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    mem::size_of,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TirPlanCompileQuoteErrorV2 {
    ZeroLimit,
    Overflow,
    Geometry,
    QuoteVisits,
    RetainedBytes,
    ScratchBytes,
    OperationBytes,
    CompileEncoding,
    CompileNormalForm,
    CompileShape,
    CompileInvariant,
}
use TirPlanCompileQuoteErrorV2 as E;
type Result<T> = std::result::Result<T, E>;
fn add(a: u64, b: u64) -> Result<u64> {
    a.checked_add(b).ok_or(E::Overflow)
}
fn mul(a: u64, b: u64) -> Result<u64> {
    a.checked_mul(b).ok_or(E::Overflow)
}
fn count(n: usize) -> Result<u64> {
    u64::try_from(n).map_err(|_| E::Overflow)
}
fn size<T>() -> Result<u64> {
    count(size_of::<T>())
}
fn array<T>(n: u64) -> Result<u64> {
    mul(n, size::<T>()?)
}
fn grow<T>(n: u64) -> Result<u64> {
    array::<T>(growing_vec_request_capacity_v1::<T>(n).ok_or(E::Overflow)?)
}
fn tree<K, V>(n: u64) -> Result<u64> {
    mul(n, count(btree_node_envelope_bytes_v1::<K, V>())?)
}
fn accumulate(sum: &mut u64, amount: u64) -> Result<()> {
    *sum = add(*sum, amount)?;
    Ok(())
}
fn shape(ty: &TensorType) -> Result<u64> {
    grow::<Dim>(count(ty.shape.len())?)
}
fn prim(prim: &Prim) -> Result<u64> {
    match prim {
        Prim::Transpose { perm } => grow::<u8>(count(perm.len())?),
        Prim::Reshape
        | Prim::Slice { .. }
        | Prim::Concat { .. }
        | Prim::Broadcast
        | Prim::Iota { .. }
        | Prim::Gather { .. }
        | Prim::Cast
        | Prim::Add
        | Prim::Sub
        | Prim::Mul
        | Prim::MatMul
        | Prim::ReduceSum { .. }
        | Prim::ReduceMax { .. }
        | Prim::Div { .. }
        | Prim::Clamp { .. }
        | Prim::Log2Floor
        | Prim::IntExp
        | Prim::IntRsqrt
        | Prim::IntLn
        | Prim::Compare { .. }
        | Prim::Select
        | Prim::TopK { .. }
        | Prim::StateWrite { .. }
        | Prim::HistAppend { .. } => Ok(0),
    }
}

fn rank(p: &TirProgramV1, block: usize, r: Ref) -> Result<u64> {
    let b = p.blocks.get(block).ok_or(E::Geometry)?;
    Ok(match r {
        Ref::Node(j) => count(b.nodes.get(j as usize).ok_or(E::Geometry)?.out.shape.len())?,
        Ref::CarryIn(j) => count(b.carry_in.get(j as usize).ok_or(E::Geometry)?.shape.len())?,
        Ref::Param(j) => count(p.params.get(j as usize).ok_or(E::Geometry)?.shape.len())?,
        Ref::Const(j) => count(p.consts.get(j as usize).ok_or(E::Geometry)?.shape.len())?,
        Ref::State(j) => count(p.states.get(j as usize).ok_or(E::Geometry)?.shape.len())?,
        Ref::Input(_) => 0,
    })
}
fn native_payload(dtype: DType, elements: u64) -> Result<u64> {
    match dtype {
        DType::I8 => grow::<i8>(elements),
        DType::I16 => grow::<i16>(elements),
        DType::I32 => grow::<i32>(elements),
        DType::I64 => grow::<i64>(elements),
        DType::I128 => grow::<i128>(elements),
        DType::Idx => grow::<u32>(elements),
    }
}

/// All fields are explicit local limits, not host/system or protocol promises.
/// Quote visits bound only this allocation-free count walk, not validate CPU.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TirPlanCompileLimitsV2 {
    pub max_quote_visits: u64,
    pub max_retained_bytes: u64,
    pub max_scratch_bytes: u64,
    pub max_operation_bytes: u64,
    /// Pre-summed independent owners excluding the Source owner automatically
    /// included below: header/file/class/context, previous plans/exports, etc.
    pub external_retained_bytes: u64,
}

/// Private successful constructor; every byte value comes from the producer's
/// typed inventory and immutable canonical Source-owner provenance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TirPlanCompileQuoteV2 {
    source_owner_requests: u64,
    revalidation_requests: u64,
    program_clone_bytes: u64,
    info_bytes: u64,
    const_bytes: u64,
    block_bytes: u64,
    state_bytes: u64,
    param_membership_bytes: u64,
    row_bytes: u64,
    retained_bytes: u64,
    occurrence_scratch_bytes: u64,
    const_slab_scratch_bytes: u64,
    touch_scratch_bytes: u64,
    row_scratch_bytes: u64,
    producer_frame_bytes: u64,
    scratch_bytes: u64,
    quote_visits_upper: u64,
    occurrence_reference_visits: u64,
    const_interval_lanes: u64,
    const_slab_lanes: u64,
    operation_peak_bytes: u64,
}
impl TirPlanCompileQuoteV2 {
    pub fn source_owner_requests(&self) -> u64 {
        self.source_owner_requests
    }
    pub fn revalidation_requests(&self) -> u64 {
        self.revalidation_requests
    }
    pub fn program_clone_bytes(&self) -> u64 {
        self.program_clone_bytes
    }
    pub fn info_bytes(&self) -> u64 {
        self.info_bytes
    }
    pub fn const_bytes(&self) -> u64 {
        self.const_bytes
    }
    pub fn block_bytes(&self) -> u64 {
        self.block_bytes
    }
    pub fn state_bytes(&self) -> u64 {
        self.state_bytes
    }
    pub fn param_membership_bytes(&self) -> u64 {
        self.param_membership_bytes
    }
    pub fn row_bytes(&self) -> u64 {
        self.row_bytes
    }
    pub fn retained_bytes(&self) -> u64 {
        self.retained_bytes
    }
    pub fn occurrence_scratch_bytes(&self) -> u64 {
        self.occurrence_scratch_bytes
    }
    pub fn const_slab_scratch_bytes(&self) -> u64 {
        self.const_slab_scratch_bytes
    }
    pub fn touch_scratch_bytes(&self) -> u64 {
        self.touch_scratch_bytes
    }
    pub fn row_scratch_bytes(&self) -> u64 {
        self.row_scratch_bytes
    }
    pub fn producer_frame_bytes(&self) -> u64 {
        self.producer_frame_bytes
    }
    pub fn scratch_bytes(&self) -> u64 {
        self.scratch_bytes
    }
    pub fn quote_visits_upper(&self) -> u64 {
        self.quote_visits_upper
    }
    /// Named partial work dimensions only. These are not all compile/NF CPU,
    /// instructions, price, elapsed time or an execution/model-work claim.
    pub fn occurrence_reference_visits(&self) -> u64 {
        self.occurrence_reference_visits
    }
    pub fn const_interval_lanes(&self) -> u64 {
        self.const_interval_lanes
    }
    pub fn const_slab_lanes(&self) -> u64 {
        self.const_slab_lanes
    }
    pub fn operation_peak_bytes(&self) -> u64 {
        self.operation_peak_bytes
    }
}

/// Borrowed, nonclone preparation; no caller-created numeric quote or different
/// program can be substituted between host reservation and consuming compile.
pub struct PreparedTirPlanCompileV2<'a> {
    source: &'a LocalSourceProgramOwnerV1,
    quote: TirPlanCompileQuoteV2,
}
impl PreparedTirPlanCompileV2<'_> {
    pub fn quote(&self) -> &TirPlanCompileQuoteV2 {
        &self.quote
    }
    /// Caller retains the actual startup host guard acquired from quote() before
    /// entry. This owner is logical data and does not fabricate a lease itself.
    pub fn compile(self) -> Result<LocalCompiledTirPlanV2> {
        match TirPlan::compile(self.source.program()) {
            Ok(plan) => Ok(LocalCompiledTirPlanV2 { plan, quote: self.quote }),
            Err(error) => {
                // NF source is unchanged: semantic errors are invariant failures,
                // not a substitute grammar/profile or model disqualification.
                // Retain kind if an audited backend reports one; payload never
                // escapes this wrapper and its phase is funded by second Source q.
                let kind = match error.kind {
                    TirErrorKind::Encoding => E::CompileEncoding,
                    TirErrorKind::NormalForm => E::CompileNormalForm,
                    TirErrorKind::Shape => E::CompileShape,
                    _ => E::CompileInvariant,
                };
                drop(error);
                Err(kind)
            }
        }
    }
}

/// Owns the fresh unmodified compiler result and its quote. No Source borrow is
/// retained, so a startup aggregate can own both without self-reference. No
/// Clone/Deref/raw public accessor/unchecked conversion is supplied. Plan's
/// OnceLock is interior mutable even through &TirPlan, so exposing that publicly
/// would let callers fill arbitrary fusion data outside this startup quote.
pub struct LocalCompiledTirPlanV2 {
    plan: TirPlan,
    quote: TirPlanCompileQuoteV2,
}
impl LocalCompiledTirPlanV2 {
    pub fn quote(&self) -> &TirPlanCompileQuoteV2 {
        &self.quote
    }
    /// For the same-crate Loaded/Prepared bridge only. Native execution, fusion,
    /// refinement and exports require their own separately admitted stages.
    #[allow(dead_code)] // Loaded aggregate integration is a separate draft.
    pub(crate) fn plan(&self) -> &TirPlan {
        &self.plan
    }
}

/// Pure O(1) upper for this quote's named metadata visits. A persistent startup
/// budget can charge it BEFORE prepare, including failed/retried attempts.
/// This is not all NF/compiler CPU; no callback or allocation occurs here.
pub fn tir_plan_compile_quote_visits_upper_v2(source: &LocalSourceProgramOwnerV1) -> Result<u64> {
    mul(add(source.quote().nf_items(), 1)?, 8)
}
pub fn prepare_tir_plan_compile_v2<'a>(
    source: &'a LocalSourceProgramOwnerV1,
    limits: TirPlanCompileLimitsV2,
) -> Result<PreparedTirPlanCompileV2<'a>> {
    let quote = quote_tir_plan_compile_v2(source, limits)?;
    Ok(PreparedTirPlanCompileV2 { source, quote })
}

// Vec/String/derive Clone copies visible elements, not the source's spare
// capacity. Use the public shared growing envelope for clones as a conservative
// bound as well as collect/push; no copied private capacity rule.
fn program_clone_heap(p: &TirProgramV1) -> Result<u64> {
    let mut out = grow::<ParamDecl>(count(p.params.len())?)?;
    for d in &p.params {
        accumulate(&mut out, grow::<u8>(count(d.name.len())?)?)?;
        accumulate(&mut out, grow::<u32>(count(d.shape.len())?)?)?;
    }
    accumulate(&mut out, grow::<ConstDecl>(count(p.consts.len())?)?)?;
    for d in &p.consts {
        accumulate(&mut out, grow::<u32>(count(d.shape.len())?)?)?;
        accumulate(&mut out, grow::<u8>(count(d.data.len())?)?)?;
    }
    accumulate(&mut out, grow::<StateDecl>(count(p.states.len())?)?)?;
    for d in &p.states {
        accumulate(&mut out, grow::<u8>(count(d.name.len())?)?)?;
        accumulate(&mut out, grow::<u32>(count(d.shape.len())?)?)?;
    }
    accumulate(&mut out, grow::<Block>(count(p.blocks.len())?)?)?;
    for b in &p.blocks {
        accumulate(&mut out, grow::<u8>(count(b.name.len())?)?)?;
        accumulate(&mut out, grow::<TensorType>(count(b.carry_in.len())?)?)?;
        for t in &b.carry_in {
            accumulate(&mut out, shape(t)?)?;
        }
        accumulate(&mut out, grow::<Node>(count(b.nodes.len())?)?)?;
        for n in &b.nodes {
            accumulate(&mut out, prim(&n.prim)?)?;
            accumulate(&mut out, grow::<Ref>(count(n.inputs.len())?)?)?;
            accumulate(&mut out, shape(&n.out)?)?;
        }
        accumulate(&mut out, grow::<u16>(count(b.carry_out.len())?)?)?;
    }
    accumulate(&mut out, grow::<u8>(count(p.schedule.layers.len())?)?)?;
    Ok(out)
}

/// Quote only; it does not validate/compile/create ProgramInfo or any collection.
/// The Source owner is immutable and already canonical/NF, so compile's second
/// validate cannot produce a semantic error for an unchanged program. Its actual
/// allocations STILL execute: one additional full admitted Source envelope is
/// charged for them, conservatively including the original parse/diagnostic/root
/// terms that a validate-only pass does not need. Original Source owner remains
/// a separate co-retained term. No private NF policy is exported or recreated.
///
/// Returned plan root+all retained heaps are charged alongside SUM of named
/// temporary phases, even where a MAX could be tighter. Old realloc/allocator
/// internals and optimized stack frames are outside logical request accounting.
/// A caller must acquire/retain its actual startup host guard BEFORE calling
/// unchanged TirPlan::compile(source.program()). Subsequent public-plan mutation,
/// clones, refine/fused_regions/Params/executor/source loading need new contracts.
pub fn quote_tir_plan_compile_v2(source: &LocalSourceProgramOwnerV1, limits: TirPlanCompileLimitsV2) -> Result<TirPlanCompileQuoteV2> {
    if limits.max_quote_visits == 0
        || limits.max_retained_bytes == 0
        || limits.max_scratch_bytes == 0
        || limits.max_operation_bytes == 0
    {
        return Err(E::ZeroLimit);
    }
    let p = source.program();
    let n = source.quote().nf_items();
    // Eight named linear metadata families: program-clone census, consts,
    // BlockPlan/input refs, ProgramInfo/carry, states, params, row sites and
    // final scalar aggregation. No shape/data payload is traversed by the quote.
    let quote_visits_upper = tir_plan_compile_quote_visits_upper_v2(source)?;
    if quote_visits_upper > limits.max_quote_visits {
        return Err(E::QuoteVisits);
    }
    let source_owner_requests = source.quote().admitted_native_requests();
    let revalidation_requests = source_owner_requests;
    let bcount = count(p.blocks.len())?;
    let occurrences = add(count(p.schedule.layers.len())?, 2)?;
    let layer_slots = count(p.schedule.layers.len().max(1))?;
    let program_clone_bytes = program_clone_heap(p)?;

    let pre = p.blocks.get(p.schedule.pre as usize).ok_or(E::Geometry)?;
    let mut info_bytes = grow::<BlockInfo>(bcount)?;
    accumulate(&mut info_bytes, grow::<TensorType>(count(pre.carry_out.len())?)?)?;
    for j in &pre.carry_out {
        accumulate(&mut info_bytes, shape(&pre.nodes.get(*j as usize).ok_or(E::Geometry)?.out)?)?;
    }

    let mut const_bytes = grow::<Buf>(count(p.consts.len())?)?;
    for c in &p.consts {
        let width = count(c.dtype.width())?;
        let bytes = count(c.data.len())?;
        if width == 0 || bytes % width != 0 {
            return Err(E::Geometry);
        }
        accumulate(&mut const_bytes, native_payload(c.dtype, bytes / width)?)?;
    }

    let mut block_bytes = grow::<BlockPlan>(bcount)?;
    let mut const_slab_scratch_bytes = 0;
    let mut const_interval_lanes = 0;
    let mut const_slab_lanes = 0;
    let mut row_bytes = grow::<Vec<Option<TirRowGatherV1>>>(bcount)?;
    accumulate(&mut row_bytes, grow::<Vec<TirRowGroupV1>>(bcount)?)?;
    accumulate(&mut row_bytes, array::<bool>(count(p.params.len())?)?)?;
    let mut row_scratch_bytes = 0;
    for (bi, b) in p.blocks.iter().enumerate() {
        accumulate(&mut block_bytes, grow::<NodePlan>(count(b.nodes.len())?)?)?;
        accumulate(&mut block_bytes, grow::<TensorType>(count(b.carry_in.len())?)?)?;
        for t in &b.carry_in {
            accumulate(&mut block_bytes, shape(t)?)?;
        }
        accumulate(&mut block_bytes, grow::<u16>(count(b.carry_out.len())?)?)?;
        let mut candidates = 0u64;
        let mut type_scratch = 0;
        for node in &b.nodes {
            accumulate(&mut block_bytes, prim(&node.prim)?)?;
            accumulate(&mut block_bytes, grow::<Ref>(count(node.inputs.len())?)?)?;
            accumulate(&mut block_bytes, shape(&node.out)?)?;
            accumulate(&mut block_bytes, grow::<TensorType>(count(node.inputs.len())?)?)?;
            accumulate(&mut block_bytes, grow::<Interval>(count(node.inputs.len())?)?)?;
            for r in &node.inputs {
                accumulate(&mut block_bytes, grow::<Dim>(rank(p, bi, *r)?)?)?;
                if let Ref::Const(j) = r {
                    let c = p.consts.get(*j as usize).ok_or(E::Geometry)?;
                    accumulate(&mut const_interval_lanes, count(c.data.len() / c.dtype.width())?)?;
                }
            }
            if let (Prim::Gather { .. }, Some(Ref::Const(j))) = (&node.prim, node.inputs.first()) {
                let c = p.consts.get(*j as usize).ok_or(E::Geometry)?;
                let lanes = count(c.data.len() / c.dtype.width())?;
                accumulate(&mut const_slab_lanes, lanes)?;
                let scratch = add(
                    add(size::<Vec<usize>>()?, grow::<usize>(count(c.shape.len())?)?)?,
                    add(size::<Vec<i128>>()?, grow::<i128>(lanes)?)?,
                )?;
                const_slab_scratch_bytes = const_slab_scratch_bytes.max(scratch);
            }
            // Structural upper bound includes all axis0/no-batch gathers,
            // even those whose origin is not a transparent parameter view.
            if matches!(node.prim, Prim::Gather { axis: 0, batch_dims: 0 }) {
                candidates = add(candidates, 1)?;
                let data = rank(p, bi, *node.inputs.first().ok_or(E::Geometry)?)?;
                let index = rank(p, bi, *node.inputs.get(1).ok_or(E::Geometry)?)?;
                let scratch = add(
                    mul(size::<TensorType>()?, 2)?,
                    add(size::<Vec<u64>>()?, add(add(grow::<Dim>(data)?, grow::<Dim>(index)?)?, grow::<u64>(data)?)?)?,
                )?;
                type_scratch = type_scratch.max(scratch);
            }
            if matches!(node.prim, Prim::Reshape) {
                type_scratch = type_scratch.max(add(size::<Vec<u64>>()?, grow::<u64>(count(node.out.shape.len())?)?)?);
            }
        }
        accumulate(&mut row_bytes, array::<Option<TirRowGatherV1>>(count(b.nodes.len())?)?)?;
        accumulate(&mut row_bytes, grow::<TirRowGroupV1>(candidates)?)?;
        // For a partition m_i>=1, SUM G(m_i) <= G(SUM m_i) +
        // groups*G(1). groups<=members<=candidates. This counts per-group
        // minimum capacity, which one flat G(total_members) alone misses.
        accumulate(&mut row_bytes, add(grow::<(u16, u16)>(candidates)?, mul(candidates, grow::<(u16, u16)>(1)?)?)?)?;
        let mut temp = add(size::<Vec<Option<u16>>>()?, array::<Option<u16>>(count(b.nodes.len())?)?)?;
        accumulate(&mut temp, add(size::<BTreeSet<u16>>()?, tree::<u16, ()>(count(b.carry_out.len())?)?)?)?;
        // Pinned1.93 BTreeSet::from_iter first collects a Vec<u16>. Stable
        // sort at canonical MAX_CARRY=8 uses insertion sort (<=20), no heap
        // sorting buffer. The input Vec still overlaps the built tree.
        accumulate(&mut temp, add(size::<Vec<u16>>()?, grow::<u16>(count(b.carry_out.len())?)?)?)?;
        accumulate(&mut temp, add(size::<BTreeMap<Ref, usize>>()?, tree::<Ref, usize>(candidates)?)?)?;
        accumulate(&mut temp, type_scratch)?;
        row_scratch_bytes = row_scratch_bytes.max(temp);
    }

    let mut state_bytes = grow::<Vec<Option<u32>>>(count(p.states.len())?)?;
    let mut state_slots = 0;
    for s in &p.states {
        let instances = if s.per_layer { layer_slots } else { 1 };
        state_slots = add(state_slots, instances)?;
        accumulate(&mut state_bytes, array::<Option<u32>>(instances)?)?;
        accumulate(&mut state_bytes, mul(instances, grow::<usize>(count(s.shape.len())?)?)?)?;
    }
    // Fully enumerated declaration slots safely overbound sparse touched
    // instances, without allocating a projection or trusting public plan data.
    accumulate(&mut state_bytes, grow::<StateInstance>(state_slots)?)?;
    let mut param_slots = 0;
    for d in &p.params {
        param_slots = add(param_slots, if d.per_layer { layer_slots } else { 1 })?;
    }
    let param_membership_bytes = tree::<(u16, Option<u16>), ()>(param_slots)?;
    let touch_scratch_bytes = add(size::<BTreeSet<u16>>()?, tree::<u16, ()>(count(p.states.len())?)?)?;
    let occurrence_scratch_bytes = add(size::<Vec<(u8, Option<u16>)>>()?, grow::<(u8, Option<u16>)>(occurrences)?)?;

    // Counts the private quote-owning producer wrapper, including TirPlan root.
    let mut retained_bytes = size::<LocalCompiledTirPlanV2>()?;
    for component in [
        program_clone_bytes,
        info_bytes,
        const_bytes,
        block_bytes,
        state_bytes,
        param_membership_bytes,
        row_bytes,
        grow::<(u8, Option<u16>)>(occurrences)?,
        grow::<u32>(occurrences)?,
    ] {
        accumulate(&mut retained_bytes, component)?;
    }
    let mut scratch_bytes = revalidation_requests;
    let producer_frame_bytes = add(
        size::<PreparedTirPlanCompileV2<'static>>()?.checked_sub(size::<TirPlanCompileQuoteV2>()?).ok_or(E::Geometry)?,
        add(
            size::<Result<LocalCompiledTirPlanV2>>()?.checked_sub(size::<LocalCompiledTirPlanV2>()?).ok_or(E::Geometry)?,
            size::<std::result::Result<TirPlan, TirError>>()?.checked_sub(size::<TirPlan>()?).ok_or(E::Geometry)?,
        )?,
    )?;
    for component in [occurrence_scratch_bytes, const_slab_scratch_bytes, touch_scratch_bytes, row_scratch_bytes] {
        accumulate(&mut scratch_bytes, component)?;
    }
    accumulate(&mut scratch_bytes, producer_frame_bytes)?;
    let operation_peak_bytes = add(add(source_owner_requests, retained_bytes)?, add(scratch_bytes, limits.external_retained_bytes)?)?;
    if retained_bytes > limits.max_retained_bytes {
        return Err(E::RetainedBytes);
    }
    if scratch_bytes > limits.max_scratch_bytes {
        return Err(E::ScratchBytes);
    }
    if operation_peak_bytes > limits.max_operation_bytes {
        return Err(E::OperationBytes);
    }
    Ok(TirPlanCompileQuoteV2 {
        source_owner_requests,
        revalidation_requests,
        program_clone_bytes,
        info_bytes,
        const_bytes,
        block_bytes,
        state_bytes,
        param_membership_bytes,
        row_bytes,
        retained_bytes,
        occurrence_scratch_bytes,
        const_slab_scratch_bytes,
        touch_scratch_bytes,
        row_scratch_bytes,
        producer_frame_bytes,
        scratch_bytes,
        quote_visits_upper,
        occurrence_reference_visits: mul(add(n, 1)?, occurrences)?,
        const_interval_lanes,
        const_slab_lanes,
        operation_peak_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_native_resource_v1::{
        LocalScanWorkBudgetV1, SourceProgramLocalErrorV1, SourceProgramLocalLimitsV1, scan_source_program_local_v1,
    };
    use misaka_palw_tir::{
        builder::ProgramBuilder,
        program::{HISTORY_BOUND_V1_HELD, LOGITS_I64_Q24_SCHEME_V2, MAX_PROGRAM_BYTES},
    };

    fn program(wide: bool, repeated_layer: bool) -> TirProgramV1 {
        let mut b = ProgramBuilder::new(4, HISTORY_BOUND_V1_HELD);
        let table = b.param("table", DType::I32, &[4, 2], false);
        let other = b.param("other", DType::I32, &[4, 2], false);
        let data = b.konst(DType::I64, &[4], &[-3, 4, 5, 6]);
        let index = b.konst(DType::Idx, &[2], &[0, 3]);
        let fixed = b.fixed_state("fixed", DType::I32, &[4], -100, 100, false);
        let history = b.hist_state("history", DType::I32, &[4], 4, false);
        let layer_state = repeated_layer.then(|| b.fixed_state("layer", DType::I32, &[4], -100, 100, true));
        let mut pre = b.block("pre", vec![]);
        let table = pre.reshape_fixed(table, &[4, 2]);
        let gathered = pre.gather(table, index, 0, 0);
        let transposed = pre.transpose(gathered, &[1, 0]);
        let other = pre.reshape_fixed(other, &[4, 2]);
        let other = pre.gather(other, index, 0, 0);
        let sum = pre.add(transposed, other, DType::I32);
        let slab = pre.gather(data, index, 0, 0);
        let slab = pre.cast(slab, DType::I32);
        let slab = pre.reshape_fixed(slab, &[1, 2]);
        let slab = pre.broadcast(slab, &[Dim::Fixed(2), Dim::Fixed(2)]);
        let sum = pre.add(sum, slab, DType::I32);
        let flat = pre.reshape_fixed(sum, &[4]);
        let fixed = pre.state_write(fixed, flat);
        pre.commit(fixed);
        let appended = pre.hist_append(history, fixed);
        pre.commit(appended);
        let pre = pre.finish(&[fixed]);
        let layers = if let Some(state) = layer_state {
            let mut layer = b.block("layer", vec![TensorType::fixed(DType::I32, &[4])]);
            let output = layer.state_write(state, Ref::CarryIn(0));
            layer.commit(output);
            let layer = layer.finish(&[output]);
            vec![layer, layer]
        } else {
            vec![]
        };
        let mut post = b.block("post", vec![TensorType::fixed(DType::I32, &[4])]);
        let output = post.cast(Ref::CarryIn(0), if wide { DType::I64 } else { DType::I32 });
        post.commit(output);
        let Ref::Node(logits) = output else { unreachable!() };
        let post = post.finish(&[]);
        let mut p = b.finish(pre, layers, post, logits);
        if wide {
            p.logits_scheme_id = LOGITS_I64_Q24_SCHEME_V2;
        }
        p
    }
    fn source(p: TirProgramV1) -> Option<LocalSourceProgramOwnerV1> {
        let bytes = p.encode();
        let mut work = LocalScanWorkBudgetV1::new(1 << 40).unwrap();
        let limits = SourceProgramLocalLimitsV1 {
            max_program_bytes: MAX_PROGRAM_BYTES as u64,
            max_scan_steps: 1 << 32,
            max_nf_items: 1 << 32,
            max_native_requests: 1 << 40,
        };
        match scan_source_program_local_v1(&bytes, limits, &mut work) {
            Ok(scanned) => Some(scanned.decode().unwrap()),
            Err(SourceProgramLocalErrorV1::UnsupportedCoverage) => {
                eprintln!("COMPILE_QUOTE_UNSUPPORTED_SOURCE_PROFILE: no compile/quote success claimed");
                None
            }
            Err(error) => panic!("test Source preparation failed: {error:?}"),
        }
    }
    fn limits() -> TirPlanCompileLimitsV2 {
        TirPlanCompileLimitsV2 {
            max_quote_visits: 1 << 40,
            max_retained_bytes: 1 << 40,
            max_scratch_bytes: 1 << 40,
            max_operation_bytes: 1 << 42,
            external_retained_bytes: 4096,
        }
    }
    #[test]
    fn actual_compile_const_slabs_row_groups_transpose_and_state_instances() {
        for wide in [false, true] {
            for repeated in [false, true] {
                let Some(source) = source(program(wide, repeated)) else {
                    return;
                };
                let prepared = prepare_tir_plan_compile_v2(&source, limits()).unwrap();
                let quote = *prepared.quote();
                assert_eq!(quote.source_owner_requests(), source.quote().admitted_native_requests());
                assert_eq!(quote.revalidation_requests(), source.quote().admitted_native_requests());
                assert_eq!(quote.const_slab_lanes(), 4); // Whole literal constant, not only2 selected rows.
                assert!(quote.const_slab_scratch_bytes() >= 4 * 16);
                let owner = prepared.compile().unwrap(); // metadata compilation only, no model execution.
                let plan = owner.plan();
                assert_eq!(&plan.program, source.program());
                assert_eq!(plan.program.wide_logits_q24(), wide);
                assert_eq!(plan.consts[0], Buf::I64(vec![-3, 4, 5, 6]));
                assert_eq!(plan.consts[1], Buf::Idx(vec![0, 3]));
                let pre = plan.program.schedule.pre as usize;
                assert_eq!(plan.rows.groups[pre].len(), 1);
                assert_eq!(plan.rows.groups[pre][0].members.len(), 2);
                assert_eq!(plan.instances.len(), if repeated { 4 } else { 2 });
                assert!(plan.fused.get().is_none());
                assert_eq!(owner.quote(), &quote);
            }
        }
    }
    #[test]
    fn exact_and_one_below_compiler_caps_refuse_before_compile() {
        let Some(source) = source(program(true, true)) else {
            return;
        };
        let q = quote_tir_plan_compile_v2(&source, limits()).unwrap();
        let exact = TirPlanCompileLimitsV2 {
            max_quote_visits: q.quote_visits_upper(),
            max_retained_bytes: q.retained_bytes(),
            max_scratch_bytes: q.scratch_bytes(),
            max_operation_bytes: q.operation_peak_bytes(),
            ..limits()
        };
        for (field, want) in [(0, E::QuoteVisits), (1, E::RetainedBytes), (2, E::ScratchBytes), (3, E::OperationBytes)] {
            let mut low = exact;
            match field {
                0 => low.max_quote_visits -= 1,
                1 => low.max_retained_bytes -= 1,
                2 => low.max_scratch_bytes -= 1,
                _ => low.max_operation_bytes -= 1,
            }
            assert_eq!(prepare_tir_plan_compile_v2(&source, low).err(), Some(want));
        }
        let owner = prepare_tir_plan_compile_v2(&source, exact).unwrap().compile().unwrap();
        assert_eq!(owner.quote(), &q);
    }
    #[test]
    fn shared_vec_tree_arithmetic_overflow_is_fixed_and_nonallocating() {
        assert_eq!(grow::<NodePlan>(u64::MAX), Err(E::Overflow));
        assert_eq!(tree::<u16, ()>(u64::MAX), Err(E::Overflow));
        assert_eq!(add(u64::MAX, 1), Err(E::Overflow));
        assert_eq!(grow::<Dim>(0), Ok(0));
    }
}
