//! Local same-owned-parameter dense comparison only. This is not another Miner
//! route, native authority, proof of arithmetic, Panel/Node cost measurement or
//! a cold/independent implementation benchmark. No model reopen/copy is used.
use super::*;
use crate::node::{
    run::{TirClassRunnerV1, TirJobRunV1, dense_measurement_constructor_requests_v2},
    run_wide_q24_v2::{TirClassRunnerWideQ24V2, TirJobRunWideQ24V2, TirWideRunLimitsV2},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DenseMeasurementErrorV2 {
    Limit,
    Overflow,
    Identity,
    Execute,
    Guard,
    Unsupported,
}
use DenseMeasurementErrorV2 as D;
fn add(a: u64, b: u64) -> Result<u64, D> {
    a.checked_add(b).ok_or(D::Overflow)
}
fn mul(a: u64, b: u64) -> Result<u64, D> {
    a.checked_mul(b).ok_or(D::Overflow)
}
fn limit(n: u64, max: u64) -> Result<(), D> {
    if n > max { Err(D::Limit) } else { Ok(()) }
}
fn vec_request<T>(count: u64) -> Result<u64, D> {
    mul(growing_vec_request_capacity_v1::<T>(count).ok_or(D::Overflow)?, size_of::<T>() as u64)
}
fn hash_writer(n: u64) -> Result<u64, D> {
    vec_request::<u8>(n.max(1024))
}
/// Operational requests/work only. The caller supplies finite policy, actual
/// same-ledger callbacks and the complete coexisting external ownership census.
#[derive(Clone, Copy, Debug)]
pub struct DenseMeasurementLimitsV2 {
    pub runner: TirWideRunLimitsV2,
    pub max_extra_bytes: u64,
    pub max_combined_peak_bytes: u64,
    pub max_operation_bytes: u64,
    pub max_geometry_visits: u64,
    pub max_work_per_attempt: u64,
    pub max_attempts: u64,
    pub max_total_work: u64,
    pub max_retained_requests: u64,
    pub external_retained_bytes: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DenseMeasurementUsageV2 {
    pub attempts: u64,
    pub work: u64,
    pub retained_requests: u64,
}
pub struct DenseMeasurementBudgetV2 {
    limits: DenseMeasurementLimitsV2,
    usage: DenseMeasurementUsageV2,
}
impl DenseMeasurementBudgetV2 {
    pub fn new(limits: DenseMeasurementLimitsV2) -> Result<Self, D> {
        if [
            u64::from(limits.runner.max_positions),
            limits.runner.max_step_leaves,
            limits.runner.max_row_lanes,
            limits.runner.max_work,
            limits.runner.max_requested_bytes,
            limits.max_extra_bytes,
            limits.max_combined_peak_bytes,
            limits.max_operation_bytes,
            limits.max_geometry_visits,
            limits.max_work_per_attempt,
            limits.max_attempts,
            limits.max_total_work,
            limits.max_retained_requests,
        ]
        .contains(&0)
        {
            return Err(D::Limit);
        }
        Ok(Self { limits, usage: DenseMeasurementUsageV2::default() })
    }
    pub fn usage(&self) -> DenseMeasurementUsageV2 {
        self.usage
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DenseRunnerReservationScopeV2 {
    WideFullRun,
    NarrowExecutorOnly,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DenseMeasurementQuoteV2 {
    pub profile: LocalForwardProfileV2,
    pub leaves: u64,
    pub vocabulary: u64,
    pub geometry_visits: u64,
    pub quote_scan_work: u64,
    pub work: u64,
    /// Underlying dense runner work, separately capped for BOTH widths.
    pub runner_work: u64,
    /// Wide: existing required run quote (ALL run backing; retained by run).
    /// Narrow: existing executor workspace request (released after commit).
    pub runner_requested_bytes: u64,
    pub runner_scope: DenseRunnerReservationScopeV2,
    /// Dense objects OUTSIDE the runner-request inventory. Wide only wrapper
    /// metadata; narrow constructor/retained rows/hashes/roots/encoding buffers.
    pub extra_requested_bytes: u64,
    pub combined_peak_bytes: u64,
    pub constructor_bytes: u64,
    pub retained_data_bytes: u64,
    pub conversion_bytes: u64,
    pub emitter_bytes: u64,
    pub tree_bytes: u64,
    pub trace_bytes: u64,
    pub writer_bytes: u64,
    /// Bridge validates prompt BEFORE acquiring the runner guard, so its
    /// distinct CanonicalWriter allocation belongs to EXTRA in both profiles.
    pub bridge_prompt_writer_bytes: u64,
}
/// One shot, borrowing the SAME loaded native Params and actual ordinary output.
/// No raw plan/Params getters, second loader/model/owner, native marker or saved
/// pass. The ordinary guard remains live and must be in external inventory.
pub struct PreparedDenseMeasurementV2<'a, 's> {
    owner: &'a LocalWideForwardArtifactV2<'s>,
    ordinary: &'a LocalWideForwardOutputV2,
    limits: DenseMeasurementLimitsV2,
    quote: DenseMeasurementQuoteV2,
}
enum DenseDataV2 {
    Narrow(TirJobRunV1),
    Wide(TirJobRunWideQ24V2),
}
impl DenseDataV2 {
    fn generated(&self) -> &[u32] {
        match self {
            Self::Narrow(r) => &r.generated,
            Self::Wide(r) => r.generated(),
        }
    }
    fn output_root(&self) -> Hash64 {
        match self {
            Self::Narrow(r) => r.output_root,
            Self::Wide(r) => r.output_root(),
        }
    }
    fn leaf_count(&self) -> u64 {
        match self {
            Self::Narrow(r) => r.leaf_count,
            Self::Wide(r) => r.leaf_count(),
        }
    }
}
/// Local matching data only: equality is not Court/P/credit or an independent
/// implementation correctness judgment. Inner dense data drops before extra
/// guard. Wide inner run also owns its independently acquired runner guard.
pub struct DenseMeasurementOutputV2 {
    data: DenseDataV2,
    matches_ordinary: bool,
    quote: DenseMeasurementQuoteV2,
    _extra: TirMemoryLeaseV1,
}
impl DenseMeasurementOutputV2 {
    pub fn generated(&self) -> &[u32] {
        self.data.generated()
    }
    pub fn output_root(&self) -> Hash64 {
        self.data.output_root()
    }
    pub fn leaf_count(&self) -> u64 {
        self.data.leaf_count()
    }
    pub fn matches_ordinary(&self) -> bool {
        self.matches_ordinary
    }
    pub fn quote(&self) -> DenseMeasurementQuoteV2 {
        self.quote
    }
}
fn narrow_quote(owner: &LocalWideForwardArtifactV2<'_>, l: DenseMeasurementLimitsV2, scan: u64) -> Result<DenseMeasurementQuoteV2, D> {
    let LocalForwardSpaceV2::Narrow(space) = owner.inputs.space else { return Err(D::Unsupported) };
    let ctx = owner.inputs.original_context;
    let plan = owner.inputs.compiled.plan();
    let p = owner.inputs.source.program();
    let leaves = space.inner.leaf_count_capped(ctx, l.runner.max_step_leaves).map_err(|_| D::Limit)?;
    let vocab = p.blocks[p.schedule.post as usize].nodes[p.logits as usize].out.elements_at(1);
    let decode = u64::from(ctx.exact_decode_tokens);
    limit(vocab, l.runner.max_row_lanes)?;
    limit(u64::from(owner.job.positions), u64::from(l.runner.max_positions))?;
    if vocab == 0 || vocab > u64::from(u32::MAX) {
        return Err(D::Unsupported);
    }
    let payload = mul(space.inner.maximum_leaf_lanes_v1(&owner.job), 4)?;
    let encoded = borsh::object_length(p).map_err(|_| D::Identity)? as u64;
    let geometry = mul(leaves, add(encoded, 1)?)?;
    let runner = memory::tir_executor_storage_need_v1(&space.inner, plan, ctx).map_err(|_| D::Limit)?;
    limit(runner, l.runner.max_requested_bytes)?;
    let constructor = dense_measurement_constructor_requests_v2(p).map_err(|_| D::Overflow)?;
    // Legacy converts an owned i128 Vec via IntoIter.map.collect. Quote the
    // consumed i128 backing even if std specializes/reuses it for smaller i32.
    let conversion = vec_request::<i128>(vocab)?;
    let row = conversion.max(vec_request::<i32>(vocab)?);
    let retained = add(
        vec_request::<Hash64>(leaves)?,
        add(vec_request::<Vec<i32>>(decode)?, add(mul(decode, row)?, vec_request::<u32>(decode)?)?)?,
    )?;
    let emitter = vec_request::<u8>(payload)?;
    let tree = memory::tir_full_tree_storage_need_v1(leaves).map_err(|_| D::Limit)?;
    let tiles = vocab.div_ceil(kaspa_consensus_core::palw_step_refute::PALW_LOGITS_TILE_LANES as u64);
    let trace = if owner.profile == LocalForwardProfileV2::TiledI32 {
        add(
            add(vec_request::<Hash64>(decode)?, vec_request::<Hash64>(tiles)?)?,
            add(
                memory::tir_full_tree_storage_need_v1(decode).map_err(|_| D::Limit)?,
                add(
                    memory::tir_full_tree_storage_need_v1(tiles).map_err(|_| D::Limit)?,
                    vec_request::<u8>(mul(vocab.min(4096), 4)?)?,
                )?,
            )?,
        )?
    } else {
        0
    };
    let context = borsh::object_length(ctx).map_err(|_| D::Identity)? as u64;
    // Step leaf Writer:2version+2hash64+4coord*u32+count+length=154.
    // Context, output and empty-rendering writers coexist conservatively.
    let writers = add(
        hash_writer(add(154, payload)?)?,
        add(hash_writer(context)?, add(hash_writer(add(132, mul(decode, 4)?)?)?, hash_writer(4)?)?)?,
    )?;
    let prompt_bytes = add(4, mul(u64::from(owner.job.prefill), 4)?)?;
    let prompt_writer = hash_writer(prompt_bytes)?;
    let extra = [
        prompt_writer,
        size_of::<DenseMeasurementOutputV2>() as u64,
        constructor,
        retained,
        conversion,
        emitter,
        tree,
        trace,
        writers,
    ]
    .into_iter()
    .try_fold(0, add)?;
    let fl = LocalWideForwardLimitsV2 {
        max_positions: l.runner.max_positions,
        max_generated: l.runner.max_positions,
        max_logits_lanes: l.runner.max_row_lanes,
        max_work_per_attempt: l.max_work_per_attempt,
        max_requested_bytes: l.max_combined_peak_bytes,
        max_operation_bytes: l.max_operation_bytes,
        max_attempts: l.max_attempts,
        max_total_work: l.max_total_work,
        max_retained_requests: l.max_retained_requests,
        external_retained_bytes: l.external_retained_bytes,
    };
    let ordinary = owner.forward_quote(fl).map_err(|_| D::Limit)?;
    let byte_visits = add(
        mul(mul(leaves, add(payload, 154)?)?, 2)?,
        add(mul(mul(vocab, decode)?, 20)?, mul(add(leaves, add(decode, tiles)?)?, 256)?)?,
    )?;
    let runner_work = add(
        ordinary.work.checked_sub(owner.quote_scan_work().map_err(|_| D::Overflow)?).ok_or(D::Overflow)?,
        add(geometry, byte_visits)?,
    )?;
    limit(runner_work, l.runner.max_work)?;
    // Two bridge prompt byte visits: fill CanonicalWriter then hash it. This
    // happens outside the runner's own distinct later validation/allocation.
    let work = add(scan, add(runner_work, mul(prompt_bytes, 2)?)?)?;
    Ok(DenseMeasurementQuoteV2 {
        profile: owner.profile,
        leaves,
        vocabulary: vocab,
        geometry_visits: geometry,
        quote_scan_work: scan,
        work,
        runner_work,
        runner_requested_bytes: runner,
        runner_scope: DenseRunnerReservationScopeV2::NarrowExecutorOnly,
        extra_requested_bytes: extra,
        combined_peak_bytes: add(runner, extra)?,
        constructor_bytes: constructor,
        retained_data_bytes: retained,
        conversion_bytes: conversion,
        emitter_bytes: emitter,
        tree_bytes: tree,
        trace_bytes: trace,
        writer_bytes: writers,
        bridge_prompt_writer_bytes: prompt_writer,
    })
}
impl<'s> LocalWideForwardArtifactV2<'s> {
    /// Pure metadata projection, not an execution/admission/ownership receipt.
    /// Direct query work remains caller-owned; prepare_dense_measurement prepays
    /// the SAME quote scan before using it. Useful for long-prefill sizing with
    /// no ordinary CPU run or fabricated successful output.
    pub fn quote_dense_measurement(&self, l: DenseMeasurementLimitsV2) -> Result<DenseMeasurementQuoteV2, D> {
        let scan = mul(self.quote_scan_work().map_err(|_| D::Overflow)?, 8)?;
        self.dense_projection(l, scan)
    }
    fn dense_projection(&self, l: DenseMeasurementLimitsV2, scan: u64) -> Result<DenseMeasurementQuoteV2, D> {
        let quote = match self.inputs.space {
            LocalForwardSpaceV2::Narrow(_) => narrow_quote(self, l, scan)?,
            LocalForwardSpaceV2::Wide(space) => {
                let runner =
                    TirClassRunnerWideQ24V2::new(space, self.inputs.compiled.plan(), &self.params, self.inputs.expected_class)
                        .map_err(|_| D::Identity)?;
                let q = runner.quote(self.inputs.original_context, l.runner).map_err(|_| D::Limit)?;
                let vocab = self.inputs.source.program().blocks[self.inputs.source.program().schedule.post as usize].nodes
                    [self.inputs.source.program().logits as usize]
                    .out
                    .elements_at(1);
                let prompt_bytes = add(4, mul(u64::from(self.job.prefill), 4)?)?;
                let prompt_writer = hash_writer(prompt_bytes)?;
                let extra =
                    add((size_of::<DenseMeasurementOutputV2>() + size_of::<TirClassRunnerWideQ24V2<'_>>()) as u64, prompt_writer)?;
                let geometry = mul(q.leaf_count, add(self.inputs.class.program.len() as u64, 1)?)?;
                // Existing wide run q owns ALL its rows/hashes/root scratch;
                // add first range-memo weight scan omitted by that old work q.
                DenseMeasurementQuoteV2 {
                    profile: self.profile,
                    leaves: q.leaf_count,
                    vocabulary: vocab,
                    geometry_visits: geometry,
                    quote_scan_work: scan,
                    work: add(scan, add(add(q.work, self.quote.params.payload_bytes)?, mul(prompt_bytes, 2)?)?)?,
                    runner_work: q.work,
                    runner_requested_bytes: q.requested_bytes,
                    runner_scope: DenseRunnerReservationScopeV2::WideFullRun,
                    extra_requested_bytes: extra,
                    combined_peak_bytes: add(q.requested_bytes, extra)?,
                    constructor_bytes: size_of::<TirClassRunnerWideQ24V2<'_>>() as u64,
                    retained_data_bytes: 0,
                    conversion_bytes: 0,
                    emitter_bytes: 0,
                    tree_bytes: 0,
                    trace_bytes: 0,
                    writer_bytes: 0,
                    bridge_prompt_writer_bytes: prompt_writer,
                }
            }
        };
        limit(quote.work, l.max_work_per_attempt)?;
        limit(quote.geometry_visits, l.max_geometry_visits)?;
        limit(quote.extra_requested_bytes, l.max_extra_bytes)?;
        limit(quote.combined_peak_bytes, l.max_combined_peak_bytes)?;
        Ok(quote)
    }
    /// One-shot preparation; accepts no replacement roots, class, job or Params.
    /// Full work/request admission is retained on dropped preparation/failed run.
    /// The ledger must include complete metadata/loaded backing + the still-live
    /// ordinary output guard, aliases, controls/errors and all other attempts.
    pub fn prepare_dense_measurement<'a>(
        &'a self,
        ordinary: &'a LocalWideForwardOutputV2,
        budget: &mut DenseMeasurementBudgetV2,
    ) -> Result<PreparedDenseMeasurementV2<'a, 's>, D>
    where
        's: 'a,
    {
        let l = budget.limits;
        let attempt = add(budget.usage.attempts, 1)?;
        limit(attempt, l.max_attempts)?;
        // Eight named metadata passes: geometry/payload, constructor, Borsh,
        // forward-cost, executor/retained, identity and root scratch projections.
        let scan = mul(self.quote_scan_work().map_err(|_| D::Overflow)?, 8)?;
        let precharged = add(budget.usage.work, scan)?;
        limit(scan, l.max_work_per_attempt)?;
        limit(precharged, l.max_total_work)?;
        budget.usage.attempts = attempt;
        budget.usage.work = precharged;
        if ordinary.execution() != self.inputs.expected_execution
            || ordinary.generated().len() != self.inputs.original_context.exact_decode_tokens as usize
        {
            return Err(D::Identity);
        }
        let quote = self.dense_projection(l, scan)?;
        let needed_external = add(self.quote.operation_retained_bytes, ordinary.retained_requested_bytes())?;
        limit(needed_external, l.external_retained_bytes)?;
        let retained = add(budget.usage.retained_requests, quote.combined_peak_bytes)?;
        limit(retained, l.max_retained_requests)?;
        limit(add(l.external_retained_bytes, retained)?, l.max_operation_bytes)?;
        let rest = quote.work.checked_sub(scan).ok_or(D::Overflow)?;
        let total = add(precharged, rest)?;
        limit(total, l.max_total_work)?;
        budget.usage.work = total;
        budget.usage.retained_requests = retained;
        Ok(PreparedDenseMeasurementV2 { owner: self, ordinary, limits: l, quote })
    }
}
impl PreparedDenseMeasurementV2<'_, '_> {
    pub fn quote(&self) -> DenseMeasurementQuoteV2 {
        self.quote
    }
    /// TWO disjoint acquisitions on the SAME supplied ledger: extra first,
    /// then the explicit required runner entry. No optional-global inspection,
    /// fallback or fabricated sublease. New extra guard stays through result.
    /// State/order/range memos are shared Params but each executor starts fresh;
    /// a prior ordinary run may warm range/OS caches. Never claim a cold run.
    pub fn run(self, prompt: &[u32], reserver: &TirMemoryReserverV1) -> Result<DenseMeasurementOutputV2, D> {
        let owner = self.owner;
        let q = self.quote;
        let ctx = owner.inputs.original_context;
        let extra = reserver(
            "tir-dense-measurement-extra-v2",
            owner.inputs.expected_class,
            owner.inputs.expected_execution,
            q.extra_requested_bytes,
        )
        .map_err(|_| D::Guard)?;
        if prompt.len() != ctx.declared_prefill_tokens as usize
            || !prompt_token_ids_match_v1(PalwPromptIdsFormV1::Flat, prompt, &ctx.prompt_token_ids_hash)
        {
            return Err(D::Identity);
        }
        let data = match owner.inputs.space {
            LocalForwardSpaceV2::Narrow(space) => {
                let runner =
                    TirClassRunnerV1::new(&space.inner, owner.inputs.compiled.plan(), &owner.params, owner.inputs.expected_class)
                        .map_err(|_| D::Identity)?;
                DenseDataV2::Narrow(
                    runner
                        .run_required_v2(
                            ctx,
                            prompt,
                            self.limits.runner.max_step_leaves,
                            owner.inputs.expected_execution,
                            reserver,
                            &mut |_| {},
                        )
                        .map_err(|_| D::Execute)?,
                )
            }
            LocalForwardSpaceV2::Wide(space) => {
                let runner =
                    TirClassRunnerWideQ24V2::new(space, owner.inputs.compiled.plan(), &owner.params, owner.inputs.expected_class)
                        .map_err(|_| D::Identity)?;
                DenseDataV2::Wide(
                    runner
                        .run_required_v2(ctx, prompt, owner.inputs.expected_execution, self.limits.runner, &mut |_| {}, reserver)
                        .map_err(|_| D::Execute)?,
                )
            }
        };
        if data.leaf_count() != q.leaves {
            return Err(D::Identity);
        }
        let matches_ordinary = data.generated() == self.ordinary.generated() && data.output_root() == self.ordinary.output_root();
        Ok(DenseMeasurementOutputV2 { data, matches_ordinary, quote: q, _extra: extra })
    }
}
