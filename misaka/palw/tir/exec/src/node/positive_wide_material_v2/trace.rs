//! Producer-only dependency walk. NO eval_demanded / numerical primitives.
//! Captured values choose dynamic addresses only. Node independently recomputes
//! those choices and refuses missing/noncanonical material if this data is wrong.
use super::proof::WidePanelBuilderV2;
use super::*;
use kaspa_consensus_core::{
    palw_positive_check_v2::expanded_plan::{ExpandedRelation, ExpandedRelationKind},
    palw_step::PalwStepCoordinateV1,
    palw_tir_step_v1::PalwTirLeafKindV1,
};
use misaka_palw_tir::{
    Prim, Ref,
    demand::{DemandContext, carry_in_node_v1, hist_row_node_v1, history_length_v1, state_writer_v1},
    lane_geometry::{broadcast_index, gather_data_coordinate, gather_index_coordinate, ravel, unravel},
    types::strides,
};
use std::collections::{BTreeSet, HashSet};
#[derive(Default)]
pub(super) struct Reads {
    pub steps: BTreeSet<u64>,
    pub params: BTreeSet<u32>,
    pub prompt: bool,
    pub decode: bool,
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Task {
    Node(DemandContext, u16, usize, bool),
    Value(DemandContext, RefKey, usize),
    Fixed(u32, u16, Option<u16>, usize),
    History(u32, u16, Option<u16>, u32, usize),
}
// With the pinned std table implementation, even eight transient buckets per
// admitted frame plus pending storage fit the existing 1024-byte/frame owner.
// Force a review if the private task grows beyond the conservative lane size.
const _: () = assert!(std::mem::size_of::<Task>() <= 64);
// Ref itself lacks Ord; this private key has no serialized representation.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum RefKey {
    Node(u16),
    Carry(u8),
    Param(u16),
    Const(u16),
    State(u16),
    Input(u8),
}
impl From<Ref> for RefKey {
    fn from(r: Ref) -> Self {
        match r {
            Ref::Node(n) => Self::Node(n),
            Ref::CarryIn(n) => Self::Carry(n),
            Ref::Param(n) => Self::Param(n),
            Ref::Const(n) => Self::Const(n),
            Ref::State(n) => Self::State(n),
            Ref::Input(n) => Self::Input(n),
        }
    }
}
struct Walk<'b, 'c, 'a> {
    b: &'b WidePanelBuilderV2<'c, 'a>,
    frontier: u64,
    pending: Vec<Task>,
    // Membership only: traversal order comes from pending, never this table.
    // Reads remain ordered sets for canonical source/proof construction. Keep
    // the existing 1024-byte/frame owner and all spent-work/frame limits; no
    // smaller memory quote or refunded visits are inferred from this change.
    seen: HashSet<Task>,
    reads: Reads,
}
impl Walk<'_, '_, '_> {
    fn push(&mut self, t: Task) -> Result<(), Error> {
        self.b.capture.session.charge(Charge::Visit, 128)?;
        if self.seen.contains(&t) {
            return Ok(());
        }
        if self.seen.len() as u64 >= self.b.capture.session.limits.max_trace_frames
            || self.pending.len() as u64 >= self.b.capture.session.limits.max_trace_frames
        {
            return Err("wide trace frame cap".into());
        }
        self.seen.insert(t);
        self.pending.push(t);
        Ok(())
    }
    fn value(&mut self, ctx: DemandContext, r: Ref, lane: usize) -> Result<(), Error> {
        self.push(Task::Value(ctx, r.into(), lane))
    }
    fn step_leaf(&mut self, index: u64) -> Result<(), Error> {
        if index >= self.frontier {
            return Err("wide source does not precede target".into());
        }
        if self.reads.steps.len() as u64 >= self.b.capture.session.limits.max_trace_frames {
            return Err("wide source set cap".into());
        }
        self.reads.steps.insert(index);
        Ok(())
    }
    fn coord(&mut self, pos: u32, slot: u32, tile: u64) -> Result<(), Error> {
        let builder = self.b;
        let space = builder.capture.expected.space;
        let job = space.job_shape(self.b.capture.expected.job).map_err(|e| e.to_string())?;
        let (call_index, position) = job.call_position(pos);
        let coord = PalwStepCoordinateV1 {
            call_index,
            node_slot: slot,
            position,
            tile_index: u32::try_from(tile).map_err(|_| "wide tile index")?,
        };
        let index = space.leaf_index(self.b.capture.expected.job, &coord).ok_or("wide source coordinate absent")?;
        self.step_leaf(index)
    }
    fn node(&mut self, ctx: DemandContext, node: u16, lane: usize, force: bool) -> Result<(), Error> {
        let builder = self.b;
        let space = builder.capture.expected.space;
        let (block, layer) = *space.occurrences().get(ctx.occurrence as usize).ok_or("wide trace occurrence")?;
        let p = space.program();
        let n = p.blocks[block as usize].nodes.get(node as usize).ok_or("wide trace node")?;
        let h = history_length_v1(space.info(), block, ctx.pos).ok_or("wide trace history shape")?;
        let shape = n.out.resolve(h);
        if lane >= shape.iter().product::<usize>() {
            return Err("wide trace target lane".into());
        }
        if n.commit && !force {
            let tile = space.commit_tile_len(block, node).ok_or("wide commit tile")?;
            let slot = space.node_slot(ctx.occurrence as usize, node).ok_or("wide commit slot")?;
            return self.coord(ctx.pos, slot, (lane / tile as usize) as u64);
        }
        let inputs = &n.inputs;
        let o = unravel(lane, &strides(&shape));
        let ish = |r: Ref| builder.ref_shape(ctx, r);
        match &n.prim {
            Prim::Reshape
            | Prim::Cast
            | Prim::Clamp { .. }
            | Prim::Log2Floor
            | Prim::IntExp
            | Prim::IntRsqrt
            | Prim::IntLn
            | Prim::StateWrite { .. } => self.value(ctx, inputs[0], lane)?,
            Prim::Transpose { perm } => {
                let sh = ish(inputs[0])?;
                let mut ix = vec![0; o.len()];
                for (k, p) in perm.iter().enumerate() {
                    ix[*p as usize] = o[k]
                }
                self.value(ctx, inputs[0], ravel(&ix, &strides(&sh)))?
            }
            Prim::Slice { axis, start } => {
                let sh = ish(inputs[0])?;
                let mut ix = o;
                ix[*axis as usize] += *start as usize;
                self.value(ctx, inputs[0], ravel(&ix, &strides(&sh)))?
            }
            Prim::Concat { axis } => {
                let a = *axis as usize;
                let mut ix = o;
                let mut found = false;
                for r in inputs {
                    let sh = ish(*r)?;
                    if ix[a] < sh[a] {
                        self.value(ctx, *r, ravel(&ix, &strides(&sh)))?;
                        found = true;
                        break;
                    }
                    ix[a] -= sh[a]
                }
                if !found {
                    return Err("wide concat extent".into());
                }
            }
            Prim::Broadcast => {
                let sh = ish(inputs[0])?;
                self.value(ctx, inputs[0], broadcast_index(&o, &sh))?
            }
            Prim::Iota { .. } => {}
            Prim::Gather { axis, batch_dims } => {
                let d = ish(inputs[0])?;
                let x = ish(inputs[1])?;
                let a = *axis as usize;
                let bd = *batch_dims as usize;
                let xi = ravel(&gather_index_coordinate(&o, a, bd, x.len()), &strides(&x));
                self.value(ctx, inputs[1], xi)?;
                let selected = self.b.ref_value(ctx, inputs[1], xi)?;
                if selected < 0 || selected >= d[a] as i128 {
                    return Err("wide captured Gather index".into());
                }
                let di = ravel(&gather_data_coordinate(&o, a, x.len(), bd, selected as usize), &strides(&d));
                self.value(ctx, inputs[0], di)?
            }
            Prim::Add | Prim::Sub | Prim::Mul | Prim::Div { .. } | Prim::Compare { .. } => {
                for r in inputs {
                    let sh = ish(*r)?;
                    self.value(ctx, *r, broadcast_index(&o, &sh))?
                }
            }
            Prim::Select => {
                let sh = ish(inputs[0])?;
                let at = broadcast_index(&o, &sh);
                self.value(ctx, inputs[0], at)?;
                let choice = if self.b.ref_value(ctx, inputs[0], at)? != 0 { inputs[1] } else { inputs[2] };
                let sh = ish(choice)?;
                self.value(ctx, choice, broadcast_index(&o, &sh))?
            }
            Prim::MatMul => {
                let x = ish(inputs[0])?;
                let y = ish(inputs[1])?;
                let (xr, yr, or) = (x.len(), y.len(), shape.len());
                let (m, k, n) = (x[xr - 2], x[xr - 1], y[yr - 1]);
                let batch = &o[..or - 2];
                let xb = broadcast_index(batch, &x[..xr - 2]);
                let yb = broadcast_index(batch, &y[..yr - 2]);
                let (xo, yo) = (xb * m * k + o[or - 2] * k, yb * k * n + o[or - 1]);
                for t in 0..k {
                    self.value(ctx, inputs[0], xo + t)?;
                    self.value(ctx, inputs[1], yo + t * n)?
                }
            }
            Prim::ReduceSum { axis } | Prim::ReduceMax { axis } | Prim::TopK { axis, .. } => {
                let sh = ish(inputs[0])?;
                let mut ix = o;
                for t in 0..sh[*axis as usize] {
                    ix[*axis as usize] = t;
                    self.value(ctx, inputs[0], ravel(&ix, &strides(&sh)))?
                }
            }
            Prim::HistAppend { state } => {
                let s = &p.states[*state as usize];
                let rows = s.shape.iter().map(|d| *d as usize).product::<usize>();
                let (t, within) = (lane / rows, lane % rows);
                if t + 1 == h {
                    self.value(ctx, inputs[0], within)?
                } else {
                    let row = (ctx.pos as usize + 1 - h + t) as u32;
                    let layer = s.per_layer.then_some(layer).flatten();
                    self.push(Task::History(ctx.pos, *state, layer, row, within))?
                }
            }
        }
        Ok(())
    }
    fn run_task(&mut self, t: Task) -> Result<(), Error> {
        let builder = self.b;
        let e = &builder.capture.expected;
        let space = e.space;
        match t {
            Task::Node(ctx, n, l, force) => self.node(ctx, n, l, force),
            Task::Value(ctx, r, lane) => {
                let (block, layer) = space.occurrences()[ctx.occurrence as usize];
                let p = space.program();
                match r {
                    RefKey::Node(n) => self.push(Task::Node(ctx, n, lane, false)),
                    RefKey::Carry(k) => {
                        let (c, n) = carry_in_node_v1(p, ctx, k).ok_or("wide carry mapping")?;
                        self.push(Task::Node(c, n, lane, false))
                    }
                    RefKey::Param(j) => {
                        let d = &p.params[j as usize];
                        let layer = d.per_layer.then_some(layer).flatten();
                        let at = mul(count(lane)?, d.dtype.width() as u64)?;
                        let leaf = self.b.inventory.leaf_of(j, layer, at).ok_or("wide param inventory mapping")?;
                        self.reads.params.insert(leaf);
                        Ok(())
                    }
                    RefKey::State(j) => {
                        let s = &p.states[j as usize];
                        let layer = s.per_layer.then_some(layer).flatten();
                        self.push(Task::Fixed(ctx.pos, j, layer, lane))
                    }
                    RefKey::Const(_) => Ok(()),
                    RefKey::Input(j) => {
                        if j == 0 {
                            let job = space.job_shape(e.job).map_err(|e| e.to_string())?;
                            if ctx.pos < job.prefill { self.reads.prompt = true } else { self.reads.decode = true }
                        }
                        let _ = block;
                        Ok(())
                    }
                }
            }
            Task::Fixed(mut pos, s, layer, lane) => {
                while pos > 0 {
                    if pos.is_multiple_of(space.layout().checkpoint_interval) {
                        let k = space.fixed_instance_index(s, layer).ok_or("wide Fixed instance")?;
                        let inst = space.fixed_instances()[k];
                        return self.coord(pos - 1, space.program_slots() + k as u32, (lane / inst.tile_lanes as usize) as u64);
                    }
                    if let Some((occ, n)) = state_writer_v1(space.program(), s, layer) {
                        return self.push(Task::Node(DemandContext { pos: pos - 1, occurrence: occ }, n, lane, false));
                    }
                    self.b.capture.session.charge(Charge::Visit, 1)?;
                    pos -= 1
                }
                Ok(())
            }
            Task::History(pos, s, layer, row, lane) => {
                let tile = space.layout().h_tile;
                let start = row - row % tile;
                let end = start + (tile - 1);
                if end < pos {
                    let k = space.hist_instance_index(s, layer).ok_or("wide Hist instance")?;
                    let inst = space.hist_instances()[k];
                    self.coord(
                        end,
                        space.program_slots() + space.fixed_instances().len() as u32 + k as u32,
                        (lane / inst.tile_lanes as usize) as u64,
                    )
                } else {
                    let (ctx, n) = hist_row_node_v1(space.program(), s, layer, row).ok_or("wide Hist append source")?;
                    self.push(Task::Node(ctx, n, lane, false))
                }
            }
        }
    }
}
pub(super) fn discover(b: &WidePanelBuilderV2<'_, '_>, r: &ExpandedRelation) -> Result<Reads, Error> {
    let cap = usize::try_from(b.capture.session.limits.max_trace_frames).map_err(|_| "wide trace capacity")?;
    let mut w = Walk { b, frontier: r.legacy_frontier, pending: vec_exact(cap)?, seen: HashSet::new(), reads: Reads::default() };
    match r.kind {
        ExpandedRelationKind::RecomputeNode { ctx, node, first_lane, lane_count, .. } => {
            for l in first_lane..first_lane + u64::from(lane_count) {
                w.push(Task::Node(ctx, node, l as usize, true))?
            }
        }
        ExpandedRelationKind::Legacy(leaf) => match leaf.kind {
            PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. } => {
                for l in first_element..first_element + u64::from(leaf.value_count) {
                    w.push(Task::Node(DemandContext { pos: leaf.position, occurrence: occurrence as u16 }, node, l as usize, true))?
                }
            }
            PalwTirLeafKindV1::State { state, layer, first_element, .. } => {
                for l in first_element..first_element + u64::from(leaf.value_count) {
                    if let Some((occ, n)) = state_writer_v1(b.capture.expected.space.program(), state, layer) {
                        w.push(Task::Node(DemandContext { pos: leaf.position, occurrence: occ }, n, l as usize, false))?
                    } else {
                        w.push(Task::Fixed(leaf.position, state, layer, l as usize))?
                    }
                }
            }
            PalwTirLeafKindV1::HistTile { state, layer, first_lane, row_lanes, first_position, .. } => {
                for pos in first_position..=leaf.position {
                    let (ctx, n) =
                        hist_row_node_v1(b.capture.expected.space.program(), state, layer, pos).ok_or("wide Hist target input")?;
                    for l in first_lane..first_lane + u64::from(row_lanes) {
                        w.push(Task::Node(ctx, n, l as usize, false))?
                    }
                }
            }
        },
    }
    while let Some(t) = w.pending.pop() {
        w.b.capture.session.charge(Charge::Visit, 1)?;
        w.run_task(t)?;
    }
    Ok(w.reads)
}
