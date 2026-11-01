//! Resident-parameter, ordinary CPU implementation of the explicit wide cell
//! subset. No wire program/parameter becomes executable state. The optional host
//! lease covers executor workspace only; IPC/caller-owned copies require their
//! separate logical reservations. Generic artifact startup and lazy row sources
//! are deliberately outside this constructor.

use std::{
    io::{self, Write},
    mem::size_of,
    ops::Range,
};

use borsh::BorshSerialize;
use kaspa_consensus_core::{palw_tir_step_v1::PalwTirVerifiedBindingV2, palw_v2::PalwJobContextV2};
use misaka_palw_tir::{DType, Prim, Ref, StateKind, admit::node_cost};

use super::{
    budget::{WideCellLimitsV2, quote_server},
    codec::{CellParamBlobV1, CellResponseWideV2, CellValueV1},
};
use crate::{
    Elem, Slice, TirExecutor, TirParams, TirPlan,
    exec::StepSink,
    node::memory::{self, TirMemoryLeaseV1},
    with_slice,
};

fn add(a: u64, b: u64) -> Result<u64, String> {
    a.checked_add(b).ok_or_else(|| "wide cell arithmetic overflow".into())
}
fn mul(a: u64, b: u64) -> Result<u64, String> {
    a.checked_mul(b).ok_or_else(|| "wide cell arithmetic overflow".into())
}
fn count(n: usize) -> Result<u64, String> {
    u64::try_from(n).map_err(|_| "wide cell native index width".into())
}
fn size<T>(n: usize) -> Result<u64, String> {
    mul(count(n)?, count(size_of::<T>())?)
}
fn cap(n: u64, max: u64) -> Result<(), String> {
    if n > max { Err("wide cell local resource ceiling".into()) } else { Ok(()) }
}
fn exact<T>(n: usize) -> Result<Vec<T>, String> {
    let mut out = Vec::new();
    out.try_reserve_exact(n).map_err(|_| "wide cell allocation refused")?;
    if out.capacity() != n {
        return Err("wide cell allocation exceeded exact request".into());
    }
    Ok(out)
}

fn scope(plan: &TirPlan, ctx: &PalwJobContextV2, occ: &Range<usize>) -> Result<(), String> {
    if occ.start >= occ.end || occ.end > plan.occurrences.len() {
        return Err("wide cell occurrence scope is invalid".into());
    }
    if occ.start + 1 == plan.occurrences.len() && ctx.declared_prefill_tokens > 1 {
        return Err("wide post-only cell requires an explicit resume API for multi-token prefill".into());
    }
    Ok(())
}

fn needed(plan: &TirPlan, occ: &Range<usize>, param: u16, layer: Option<u16>) -> bool {
    let Some(decl) = plan.program.params.get(param as usize) else {
        return false;
    };
    if !decl.per_layer && layer.is_some() {
        return false;
    }
    plan.occurrences[occ.clone()].iter().any(|(block, running_layer)| {
        (!decl.per_layer || *running_layer == layer)
            && plan.program.blocks[*block as usize].nodes.iter().any(|n| n.inputs.contains(&Ref::Param(param)))
    })
}

fn matches_program(program: &misaka_palw_tir::TirProgramV1, bytes: &[u8]) -> Result<bool, String> {
    if borsh::object_length(program).map_err(|_| "wide cell program size failed")? != bytes.len() {
        return Ok(false);
    }
    struct Compare<'a>(&'a [u8]);
    impl Write for Compare<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if !self.0.starts_with(bytes) {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "wide cell program differs"));
            }
            self.0 = &self.0[bytes.len()..];
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut cmp = Compare(bytes);
    Ok(program.serialize(&mut cmp).is_ok() && cmp.0.is_empty())
}

fn raw_matches(data: Slice<'_>, ty: DType, bytes: &[u8]) -> Result<bool, String> {
    if data.dtype() != ty {
        return Ok(false);
    }
    let width = ty.width();
    if mul(count(data.len())?, count(width)?)? != count(bytes.len())? {
        return Ok(false);
    }
    let mut at = 0;
    let mut equal = true;
    with_slice!(data, values => {
        for value in values {
            let value = value.to_i128();
            let raw = value.to_le_bytes();
            if !ty.contains(value) || bytes[at..at + width] != raw[..width] { equal = false; break; }
            at += width;
        }
    });
    Ok(equal)
}

/// Borrowed trusted CPU source. The caller supplies an unmodified result of
/// `TirPlan::compile` and its matching resident parameters. Public compiled plan
/// fields are not authenticated or reconstructed by this prepared-source API.
/// Connection work/step/IO limits are enforced by `serve_wide_v2_for_plan`, not
/// automatically by direct calls to this low-level CPU source or its stepper.
pub struct PreparedWideCellV2<'a> {
    pub(crate) verified: &'a PalwTirVerifiedBindingV2,
    pub(crate) ctx: &'a PalwJobContextV2,
    pub(crate) plan: &'a TirPlan,
    params: &'a TirParams<'a>,
    limits: WideCellLimitsV2,
    workspace: u64,
}

impl<'a> PreparedWideCellV2<'a> {
    /// Check original program/context identity and the local resource envelope.
    /// This is not validation of arbitrary caller-mutated compiled plan fields;
    /// the caller must establish the trusted-source precondition of this type.
    pub fn new(
        verified: &'a PalwTirVerifiedBindingV2,
        ctx: &'a PalwJobContextV2,
        plan: &'a TirPlan,
        params: &'a TirParams<'a>,
        limits: WideCellLimitsV2,
    ) -> Result<Self, String> {
        // The combined local envelope precedes context hashing and construction.
        let quote = quote_server(verified.space(), plan, ctx, limits)?;
        if verified.class_id() != ctx.shape_profile_id
            || verified.context_hash() != ctx.context_hash()
            || verified.space().program() != &plan.program
            || verified.job() != verified.space().job_shape(ctx).map_err(|_| "wide cell job shape")?
        {
            return Err("wide cell original class/context/plan differs".into());
        }
        Ok(Self { verified, ctx, plan, params, limits, workspace: quote.workspace_bytes })
    }

    /// A conservative whole-class charge covers streamed comparison, repeated
    /// needed-parameter scans and new_cell's range refinement. Callers admit it
    /// to their connection work counter before check_open/open_cell.
    pub fn open_work(&self, limits: WideCellLimitsV2) -> Result<u64, String> {
        let encoded = count(borsh::object_length(&self.plan.program).map_err(|_| "wide cell program size failed")?)?;
        cap(encoded, limits.max_program_bytes)?;
        let mut payload = 0;
        for p in &self.plan.program.params {
            let lanes = p.shape.iter().try_fold(1, |n, d| mul(n, u64::from(*d)))?;
            let instances = if p.per_layer { count(self.plan.program.schedule.layers.len())? } else { 1 };
            payload = add(payload, mul(mul(lanes, count(p.dtype.width())?)?, instances)?)?;
        }
        let nodes =
            self.plan.occurrences.iter().try_fold(0, |n, (b, _)| add(n, count(self.plan.program.blocks[*b as usize].nodes.len())?))?;
        let geometry = add(
            add(nodes, count(self.plan.occurrences.len())?)?,
            add(count(self.plan.instances.len())?, count(self.plan.program.params.len())?)?,
        )?;
        let work = mul(
            mul(add(add(encoded, payload)?, 4096)?, add(geometry, 1)?)?,
            mul(add(count(self.plan.program.params.len())?, 1)?, 128)?,
        )?;
        cap(work, limits.max_work)?;
        Ok(work)
    }

    pub fn check_open(
        &self,
        program: &[u8],
        occ: Range<usize>,
        blobs: &[CellParamBlobV1],
        limits: WideCellLimitsV2,
    ) -> Result<(), String> {
        scope(self.plan, self.ctx, &occ)?;
        cap(count(program.len())?, limits.max_program_bytes)?;
        cap(count(blobs.len())?, limits.max_params)?;
        if !matches_program(&self.plan.program, program)? {
            return Err("wide cell wire program is not the prepared original".into());
        }
        let mut bytes = 0;
        for (i, blob) in blobs.iter().enumerate() {
            bytes = add(bytes, count(blob.bytes.len())?)?;
            cap(bytes, limits.max_param_bytes)?;
            if !needed(self.plan, &occ, blob.param, blob.layer)
                || blobs[..i].iter().any(|b| b.param == blob.param && b.layer == blob.layer)
            {
                return Err("wide cell unexpected or duplicate parameter instance".into());
            }
            if self.params.serves_rows(blob.param, blob.layer) {
                return Err("wide cell requires resident parameters".into());
            }
            let data = self.params.get(blob.param, blob.layer).ok_or("wide cell resident parameter is missing")?;
            let declaration = &self.plan.program.params[blob.param as usize];
            let expected_lanes = declaration.shape.iter().try_fold(1, |n, d| mul(n, u64::from(*d)))?;
            if count(data.len())? != expected_lanes {
                return Err("wide cell resident parameter shape differs".into());
            }
            if !raw_matches(data, self.plan.program.params[blob.param as usize].dtype, &blob.bytes)? {
                return Err("wide cell parameter bytes differ from prepared original".into());
            }
        }
        for (block, layer) in &self.plan.occurrences[occ.clone()] {
            for node in &self.plan.program.blocks[*block as usize].nodes {
                for r in &node.inputs {
                    if let Ref::Param(j) = r {
                        let layer = if self.plan.program.params[*j as usize].per_layer { *layer } else { None };
                        if !blobs.iter().any(|b| b.param == *j && b.layer == layer) {
                            return Err("wide cell required parameter instance is absent".into());
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Construct the resident CPU cell after the caller has admitted `open_work`.
    /// This checks local scope/backing and acquires the optional workspace lease;
    /// it does not maintain a cumulative connection work or step counter.
    pub fn open_cell(&self, occ: Range<usize>, limits: WideCellLimitsV2) -> Result<CpuWideCellStepperV2<'a>, String> {
        scope(self.plan, self.ctx, &occ)?;
        let quote = quote_server(self.verified.space(), self.plan, self.ctx, limits)?;
        // Refuse lazy providers before new_cell's range refinement may call get.
        for (b, layer) in &self.plan.occurrences[occ.clone()] {
            for n in &self.plan.program.blocks[*b as usize].nodes {
                for r in &n.inputs {
                    if let Ref::Param(j) = r {
                        let l = if self.plan.program.params[*j as usize].per_layer { *layer } else { None };
                        if self.params.serves_rows(*j, l) || !self.params.has(*j, l) {
                            return Err("wide cell requires complete resident parameters".into());
                        }
                        let decl = &self.plan.program.params[*j as usize];
                        let data = self.params.get(*j, l).ok_or("wide cell resident parameter is missing")?;
                        let lanes = decl.shape.iter().try_fold(1, |n, d| mul(n, u64::from(*d)))?;
                        if data.dtype() != decl.dtype || count(data.len())? != lanes {
                            return Err("wide cell resident parameter type or shape differs".into());
                        }
                    }
                }
            }
        }
        // Range refinement also considers resident parameters outside the scope;
        // a lazy source anywhere would permit hidden whole-row materialization.
        if self.params.row_source().is_some() {
            return Err("wide cell does not materialize lazy row sources".into());
        }
        let memory = memory::reserve_v1(
            &memory::memory_reserver_v1(),
            "tir-wide-cell-v2",
            self.verified.class_id(),
            self.ctx,
            quote.workspace_bytes,
        )?;
        let mut exec = TirExecutor::new_cell(self.plan, self.params, occ.clone()).map_err(|e| e.to_string())?;
        exec.set_hist_tail(self.verified.space().layout().h_tile as usize);
        Ok(CpuWideCellStepperV2 {
            exec,
            verified: self.verified,
            scope: occ,
            limits,
            workspace: quote.workspace_bytes,
            last_post: false,
            _memory: memory,
        })
    }

    pub fn workspace_bytes(&self) -> u64 {
        self.workspace
    }
    pub fn limits(&self) -> WideCellLimitsV2 {
        self.limits
    }
}

/// Private executor ownership prevents use of the legacy i32 logits seam.
/// The executor field drops before its workspace lease.
/// Direct users must separately admit each work quote and maintain cumulative
/// step/work limits. The session server performs those admissions before calls.
pub struct CpuWideCellStepperV2<'a> {
    exec: TirExecutor<'a>,
    verified: &'a PalwTirVerifiedBindingV2,
    scope: Range<usize>,
    limits: WideCellLimitsV2,
    workspace: u64,
    last_post: bool,
    _memory: Option<TirMemoryLeaseV1>,
}

impl CpuWideCellStepperV2<'_> {
    pub fn workspace_bytes(&self) -> u64 {
        self.workspace
    }
    pub fn position(&self) -> u32 {
        self.exec.pos()
    }
    pub fn scope(&self) -> Range<usize> {
        self.scope.clone()
    }

    pub fn expected_range(&self) -> Result<Range<usize>, String> {
        let job = self.verified.job();
        if self.position() >= job.positions {
            return Err("wide cell original job is complete".into());
        }
        let mut occ = self.scope.clone();
        if !job.runs_post(self.position()) && occ.end == self.exec.plan().occurrences.len() {
            occ.end -= 1;
        }
        if occ.start >= occ.end {
            return Err("wide cell has no executable occurrence at this position".into());
        }
        Ok(occ)
    }

    fn check_next(&self, occ: &Range<usize>) -> Result<(), String> {
        if *occ != self.expected_range()? {
            return Err("wide cell occurrence range differs from original schedule".into());
        }
        Ok(())
    }

    pub fn committed_count_for_next(&self, occ: Range<usize>) -> Result<usize, String> {
        self.check_next(&occ)?;
        self.exec.plan().occurrences[occ].iter().try_fold(0usize, |n, (b, _)| {
            n.checked_add(self.exec.plan().program.blocks[*b as usize].nodes.iter().filter(|n| n.commit).count())
                .ok_or_else(|| "wide cell committed count overflow".into())
        })
    }

    fn response_bound(&self, occ: &Range<usize>) -> Result<(u64, u64), String> {
        self.check_next(occ)?;
        let plan = self.exec.plan();
        let mut bytes = count(size_of::<CellResponseWideV2>())?;
        let mut lanes = 0;
        let mut values = 0;
        for (b, _) in &plan.occurrences[occ.clone()] {
            let h = u64::from(plan.info.blocks[*b as usize].window.unwrap_or(1));
            for n in &plan.program.blocks[*b as usize].nodes {
                if n.commit {
                    values = add(values, 1)?;
                    cap(count(n.out.shape.len())?, self.limits.max_rank)?;
                    let v = n.out.elements_at(h);
                    if v == u64::MAX {
                        return Err("wide cell value shape saturated".into());
                    }
                    lanes = add(lanes, v)?;
                    bytes = add(bytes, add(count(size_of::<CellValueV1>())?, add(size::<u32>(n.out.shape.len())?, mul(v, 16)?)?)?)?;
                }
            }
        }
        cap(values, self.limits.max_values)?;
        let last = plan.occurrences[occ.end - 1].0 as usize;
        if occ.end != plan.occurrences.len() {
            let block = &plan.program.blocks[last];
            cap(count(block.carry_out.len())?, self.limits.max_carry_vectors)?;
            bytes = add(bytes, size::<Vec<i128>>(block.carry_out.len())?)?;
            for j in &block.carry_out {
                let n = block.nodes[*j as usize].out.elements_at(1);
                lanes = add(lanes, n)?;
                bytes = add(bytes, mul(n, 16)?)?;
            }
        } else {
            let n = plan.program.blocks[last].nodes[plan.program.logits as usize].out.elements_at(1);
            lanes = add(lanes, n)?;
            bytes = add(bytes, mul(n, 8)?)?;
        }
        cap(lanes, self.limits.max_lanes)?;
        cap(bytes, self.limits.max_decoded_bytes)?;
        Ok((bytes, lanes))
    }

    /// Quote one step's work; returning this number does not consume a budget.
    pub fn kernel_work_for_next(&self, occ: Range<usize>) -> Result<u64, String> {
        self.check_next(&occ)?;
        let (_, lanes) = self.response_bound(&occ)?;
        let plan = self.exec.plan();
        let mut work = mul(add(lanes, 4096)?, 128)?;
        for (b, _) in &plan.occurrences[occ] {
            work = add(work, 128)?;
            let h = u64::from(plan.info.blocks[*b as usize].window.unwrap_or(1));
            for (j, _) in plan.program.blocks[*b as usize].nodes.iter().enumerate() {
                let cost = node_cost(&plan.program, *b as usize, j, h);
                for n in [cost.macs, cost.elementwise, cost.transcendentals, cost.bytes_read, cost.bytes_written] {
                    if n == u64::MAX {
                        return Err("wide cell kernel work saturated".into());
                    }
                    work = add(work, n)?;
                }
                work = add(work, 128)?;
            }
        }
        cap(work, self.limits.max_work)?;
        Ok(work)
    }

    /// Execute once after external work/step admission. Scope, carry and local
    /// response limits are checked here; cumulative work/steps are not metered.
    pub fn step_data(
        &mut self,
        token: u32,
        occ: Range<usize>,
        carry: &[Vec<i128>],
        sink: &mut dyn StepSink,
    ) -> Result<Vec<Vec<i128>>, String> {
        self.check_next(&occ)?;
        if token >= self.exec.plan().program.token_bound || sink.every_node() {
            return Err("wide cell token or sink exceeds explicit scope".into());
        }
        let plan = self.exec.plan();
        let want = &plan.program.blocks[plan.occurrences[occ.start].0 as usize].carry_in;
        let expected = if occ.start == 0 { 0 } else { want.len() };
        if carry.len() != expected {
            return Err("wide cell carry count differs from original type".into());
        }
        cap(count(carry.len())?, self.limits.max_carry_vectors)?;
        let mut lanes = 0;
        for (ty, row) in want.iter().zip(carry) {
            let expected = ty.elements_at(1).max(1);
            if count(row.len())? != expected {
                return Err("wide cell carry shape differs".into());
            }
            lanes = add(lanes, count(row.len())?)?;
        }
        cap(lanes, self.limits.max_lanes)?;
        for (ty, row) in want.iter().zip(carry) {
            if row.iter().any(|v| !ty.dtype.contains(*v)) {
                return Err("wide cell carry dtype differs".into());
            }
        }
        self.response_bound(&occ)?;
        let post = occ.end == plan.occurrences.len();
        let out = self.exec.step_cell(token, occ, carry, sink).map_err(|e| e.to_string())?;
        self.last_post = post;
        Ok(out)
    }

    pub fn logits_lanes(&self) -> Result<Vec<i64>, String> {
        if !self.last_post {
            return Ok(Vec::new());
        }
        let (_, data) = self.exec.logits();
        cap(count(data.len())?, self.limits.max_lanes)?;
        cap(add(count(size_of::<CellResponseWideV2>())?, size::<i64>(data.len())?)?, self.limits.max_decoded_bytes)?;
        let mut out = exact(data.len())?;
        with_slice!(data, values => { for value in values { out.push(i64::try_from(value.to_i128()).map_err(|_| "wide cell logits exceed i64")?); } });
        Ok(out)
    }

    fn owns_state(&self, state: u16, layer: Option<u16>, history: bool) -> Result<(), String> {
        let plan = self.exec.plan();
        let decl = plan.program.states.get(state as usize).ok_or("wide cell state is absent")?;
        if !matches!(decl.dtype, DType::I8 | DType::I16 | DType::I32)
            || matches!(decl.kind, StateKind::Hist { .. }) != history
            || (!decl.per_layer && layer.is_some())
        {
            return Err("wide cell state type or instance differs".into());
        }
        let touched = plan.occurrences[self.scope.clone()].iter().any(|(b, l)| {
            (!decl.per_layer || *l == layer)
                && plan.program.blocks[*b as usize].nodes.iter().any(|n| {
                    n.inputs.contains(&Ref::State(state))
                        || matches!(n.prim, Prim::StateWrite { state: j } | Prim::HistAppend { state: j } if j == state)
                })
        });
        if !touched {
            return Err("wide cell state is outside its occurrence scope".into());
        }
        Ok(())
    }

    fn query_size(&self, lanes: usize) -> Result<usize, String> {
        cap(count(lanes)?, self.limits.max_lanes)?;
        let bytes = size::<i32>(lanes)?;
        cap(add(count(size_of::<CellResponseWideV2>())?, bytes)?, self.limits.max_decoded_bytes)?;
        usize::try_from(bytes).map_err(|_| "wide cell query byte width".into())
    }

    pub fn fixed_lanes(&self, state: u16, layer: Option<u16>, first: usize, n: usize) -> Result<Vec<u8>, String> {
        self.owns_state(state, layer, false)?;
        let data = self.exec.fixed_value(state, layer).ok_or("wide cell Fixed instance is absent")?;
        let end = first.checked_add(n).ok_or("wide cell Fixed query range overflow")?;
        if end > data.len() {
            return Err("wide cell Fixed query is outside row".into());
        }
        let mut out = exact(self.query_size(n)?)?;
        with_slice!(data, values => { for v in &values[first..end] { out.extend_from_slice(&i32::try_from(v.to_i128()).map_err(|_| "wide cell Fixed lane exceeds i32")?.to_le_bytes()); } });
        Ok(out)
    }

    pub fn hist_tile_lanes(
        &self,
        state: u16,
        layer: Option<u16>,
        h: usize,
        first: usize,
        row_lanes: usize,
    ) -> Result<Vec<u8>, String> {
        self.owns_state(state, layer, true)?;
        if h != self.verified.space().layout().h_tile as usize {
            return Err("wide cell history tile differs from original layout".into());
        }
        let tail = self.exec.hist_tail(state, layer).ok_or("wide cell history instance is absent")?;
        let end = first.checked_add(row_lanes).ok_or("wide cell history query range overflow")?;
        // This private executor has no imported history: every retained row was
        // appended at the same original declared fixed row shape.
        if tail.len() < h || tail.back().is_none_or(|row| end > row.len()) {
            return Err("wide cell history query is outside retained rows".into());
        }
        let lanes = h.checked_mul(row_lanes).ok_or("wide cell history query size overflow")?;
        let mut out = exact(self.query_size(lanes)?)?;
        if row_lanes == 0 {
            return Ok(out);
        }
        for row in tail.iter().skip(tail.len() - h) {
            for value in &row[first..end] {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
        Ok(out)
    }
}
