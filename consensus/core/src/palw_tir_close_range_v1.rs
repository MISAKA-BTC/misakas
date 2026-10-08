//! **Spec 04b §10.3 under `Params::palw_tir_fence2`: the close sizing's RANGE twin** — the read sets
//! of [`crate::palw_tir_close_size_v1`]'s element twin, computed over ranges of elements instead of
//! one element at a time.
//!
//! The element twin (the DAA-2,000 release's, unchanged below the fence) walks every demanded element
//! of every node of a cone, one at a time: a tile whose cone replays a `Fixed` state over `C − 1`
//! positions visits the whole state at every one of them, and a real-size class (Qwen3.5, Gemma-3,
//! Llama-3.2, Qwen3-8B) passes the sizing's work cap before its closes are sized. The range twin keeps
//! each node's demand, per context, as sorted disjoint half-open ranges of its row-major elements, and
//! maps a range through the primitive's index map at once: the range is cut into at most `2·rank − 1`
//! boxes of the node's shape, each box is mapped to the box of the operand it reads, and that box is
//! cut back into ranges of the operand's row-major order.
//!
//! **The same read sets, exactly.** Every index map the element twin applies to one element's
//! coordinates is a product of per-axis maps — identity, a shift (`Slice`, `Concat`'s parts), a
//! permutation (`Transpose`), a collapse to 0 (a broadcast axis), a whole axis or a span of it (a
//! reduction, `TopK`, a contraction, a `Gather`'s data fiber), a row split (`HistAppend`) — so the
//! image of a box is a box, and the union of the images of a range's boxes is the union of the images
//! of its elements. Everything after the index maps is per element and position-exact in both twins:
//! a step leaf is a tile of a commit point, a checkpoint or a history tile (a run of consecutive tiles
//! is a run of consecutive leaves, placed at ONE index lookup), an inventory leaf is the piece holding
//! an element's first byte (a range of elements is a range of pieces: pieces and rows are whole
//! elements), a location-free row is keyed by its index element. So each request's units — step
//! leaves, history marks, hypothetical leaves, inventory leaves, location-free rows, the token, a
//! dissection's supplied elements, the row pattern — are the element twin's, and the price of a read
//! set is a function of the set: every bound is the element twin's, byte for byte.
//! `tests/palw_tir_close_range.rs` holds both equal request by request (every mode) and bound by bound
//! on the corpus; the SDK's `tir_close_range_differential` does so on real-size classes.
//!
//! **The worst close** ([`palw_tir_worst_closes_range_work_v1`]) is the element twin's driver
//! (`palw_tir_worst_closes_work_v1`, spec 04b §10.3's structure) asking the same requests as ranges:
//! the same positions, tiles, rows, windows, probes and bottoms, the same prices and the same
//! counting outside the twin.
//!
//! **Work** counts what the range twin does, in steps that track the time they take: a context made
//! (one step; a block's node shapes are resolved once per history length, a step a node); a range
//! demanded, visited or cut into boxes, and each range a box is cut back into; a step leaf's index
//! looked up (`16` plus the program's commit points and occurrences, once per run of tiles) and each
//! leaf placed; each inventory leaf and row piece recorded; each position a replay walks; a request's
//! seed. The cap is admission's ([`crate::palw_tir_close_size_v1::PALW_TIR_CLOSE_SIZING_WORK_CAP_V1`]).

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use crate::palw_step::PalwStepCoordinateV1;
use crate::palw_tir_artifact_v1::PALW_TIR_ROW_PIECE_BYTES_V1;
use crate::palw_tir_close_size_v1::{
    HistPattern, PALW_TIR_CLOSE_SIZING_OVER_CAP_V1, PalwGenClosePricingV1, PalwGenTwinInputV1, PalwGenTwinStageV1,
    PalwTirCloseBoundV1, PalwTirClosePriceV1, PalwTirCloseReadsV1, PalwTirCloseSizingV1, element_count, leaf_cost_of, reaches_hist,
    size_of_reads, step_preimage_bytes, step_runs, strides,
};
use crate::palw_tir_court_v1::PalwTirInventoryIndexV1;
use crate::palw_tir_step_v1::{PalwTirJobShapeV1, PalwTirStepSpaceV1};
use crate::palw_v2::PalwJobContextV2;
use misaka_palw_tir::demand::{DemandContext, carry_in_node_v1, hist_row_node_v1, history_length_v1, state_writer_v1};
use misaka_palw_tir::program::StateKind;
use misaka_palw_tir::types::MAX_RANK;
use misaka_palw_tir::{Prim, Ref};

/// **Which twin sizes a class's closes** — the element twin (the DAA-2,000 release's) or the range
/// twin (`palw_tir_fence2`'s): the same bounds, different work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwTirCloseTwinV1 {
    Element,
    Range,
}

// =================================================================================================
// Ranges and boxes
// =================================================================================================

/// **Element indices as ranges**: sorted, disjoint, non-adjacent half-open `[start, end)`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct PalwTirRangesV1(Vec<(usize, usize)>);

impl PalwTirRangesV1 {
    /// `[start, end)` (empty when `start ≥ end`).
    pub fn single(start: usize, end: usize) -> Self {
        if start < end { Self(vec![(start, end)]) } else { Self::default() }
    }

    /// Any ranges, normalized.
    pub fn from_ranges(mut v: Vec<(usize, usize)>) -> Self {
        v.retain(|(a, b)| a < b);
        v.sort_unstable();
        let mut out: Vec<(usize, usize)> = Vec::with_capacity(v.len());
        for (a, b) in v {
            match out.last_mut() {
                Some(last) if a <= last.1 => last.1 = last.1.max(b),
                _ => out.push((a, b)),
            }
        }
        Self(out)
    }

    /// Any elements, as ranges.
    pub fn from_elements(elements: &[usize]) -> Self {
        Self::from_ranges(elements.iter().map(|e| (*e, e + 1)).collect())
    }

    pub fn ranges(&self) -> &[(usize, usize)] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The elements the ranges hold.
    pub fn elements(&self) -> u64 {
        self.0.iter().map(|(a, b)| (b - a) as u64).sum()
    }

    /// Every element, in order.
    pub fn iter_elements(&self) -> impl Iterator<Item = usize> + '_ {
        self.0.iter().flat_map(|(a, b)| *a..*b)
    }

    /// `self ∪ other`.
    pub fn union(&self, other: &Self) -> Self {
        let (a, b) = (&self.0, &other.0);
        let mut v: Vec<(usize, usize)> = Vec::with_capacity(a.len() + b.len());
        let (mut i, mut j) = (0, 0);
        while i < a.len() || j < b.len() {
            let next = if j >= b.len() || (i < a.len() && a[i].0 <= b[j].0) {
                i += 1;
                a[i - 1]
            } else {
                j += 1;
                b[j - 1]
            };
            match v.last_mut() {
                Some(last) if next.0 <= last.1 => last.1 = last.1.max(next.1),
                _ => v.push(next),
            }
        }
        Self(v)
    }

    /// `self ∖ other`.
    pub fn minus(&self, other: &Self) -> Self {
        let o = &other.0;
        let mut out = Vec::new();
        let mut j = 0;
        for &(start, end) in &self.0 {
            while j < o.len() && o[j].1 <= start {
                j += 1;
            }
            let mut a = start;
            let mut k = j;
            while a < end {
                if k >= o.len() || o[k].0 >= end {
                    out.push((a, end));
                    break;
                }
                let (c, d) = o[k];
                if c > a {
                    out.push((a, c));
                }
                a = a.max(d);
                k += 1;
            }
        }
        Self(out)
    }
}

/// A box of a shape: one `[lo, hi)` per axis (the first `rank` are used).
type Bx = [(usize, usize); MAX_RANK];

const NO_BOX: Bx = [(0, 1); MAX_RANK];

/// **The boxes of shape `sh` whose union is the row-major range `[a, b)`** (`a < b ≤ Π sh`),
/// appended to `out`: at most `2·rank − 1`, disjoint.
fn boxes_of(a: usize, b: usize, sh: &[usize], out: &mut Vec<Bx>) {
    fn rec(a: usize, b: usize, sh: &[usize], dim: usize, prefix: &mut Bx, out: &mut Vec<Bx>) {
        if dim == sh.len() {
            out.push(*prefix);
            return;
        }
        let inner: usize = sh[dim + 1..].iter().product::<usize>().max(1);
        let (r0, r1) = (a / inner, (b - 1) / inner);
        if r0 == r1 {
            prefix[dim] = (r0, r0 + 1);
            rec(a - r0 * inner, b - r0 * inner, sh, dim + 1, prefix, out);
            return;
        }
        let mut lo = r0;
        if !a.is_multiple_of(inner) {
            prefix[dim] = (r0, r0 + 1);
            rec(a - r0 * inner, inner, sh, dim + 1, prefix, out);
            lo = r0 + 1;
        }
        let tail = !b.is_multiple_of(inner);
        let hi = if tail { r1 } else { r1 + 1 };
        if lo < hi {
            prefix[dim] = (lo, hi);
            for (d, n) in sh.iter().enumerate().skip(dim + 1) {
                prefix[d] = (0, *n);
            }
            out.push(*prefix);
        }
        if tail {
            prefix[dim] = (r1, r1 + 1);
            rec(0, b - r1 * inner, sh, dim + 1, prefix, out);
        }
    }
    if a < b {
        let mut prefix = NO_BOX;
        rec(a, b, sh, 0, &mut prefix, out);
    }
}

/// **How many ranges [`box_ranges`] cuts box `bx` of shape `sh` into** — counted before any is made,
/// so the work is charged before the memory is spent.
fn box_range_count(bx: &Bx, sh: &[usize]) -> u64 {
    let r = sh.len();
    if (0..r).any(|d| bx[d].0 >= bx[d].1) {
        return 0;
    }
    if r == 0 {
        return 1;
    }
    let mut k = r - 1;
    while k > 0 && bx[k] == (0, sh[k]) {
        k -= 1;
    }
    (0..k).fold(1u64, |acc, d| acc.saturating_mul((bx[d].1 - bx[d].0) as u64))
}

/// The elements of box `bx` over its first `rank` axes.
fn box_elements(bx: &Bx, rank: usize) -> u64 {
    (0..rank).fold(1u64, |acc, d| acc.saturating_mul(bx[d].1.saturating_sub(bx[d].0) as u64))
}

/// **The row-major ranges of box `bx` of shape `sh`** (strides `st`), appended to `out`: one per
/// index of the axes before the last axis the box does not cover whole. Returns how many.
fn box_ranges(bx: &Bx, sh: &[usize], st: &[usize], out: &mut Vec<(usize, usize)>) -> u64 {
    let r = sh.len();
    if (0..r).any(|d| bx[d].0 >= bx[d].1) {
        return 0;
    }
    if r == 0 {
        out.push((0, 1));
        return 1;
    }
    let mut k = r - 1;
    while k > 0 && bx[k] == (0, sh[k]) {
        k -= 1;
    }
    let run = (bx[k].1 - bx[k].0) * st[k];
    let mut idx = [0usize; MAX_RANK];
    for d in 0..k {
        idx[d] = bx[d].0;
    }
    let mut n = 0u64;
    loop {
        let base: usize = (0..k).map(|d| idx[d] * st[d]).sum::<usize>() + bx[k].0 * st[k];
        out.push((base, base + run));
        n += 1;
        let mut d = k;
        loop {
            if d == 0 {
                return n;
            }
            d -= 1;
            idx[d] += 1;
            if idx[d] < bx[d].1 {
                break;
            }
            idx[d] = bx[d].0;
        }
    }
}

/// The box an operand of shape `sh` reads for output box `bx` of rank `out_rank`, broadcast (spec
/// 04b §2.3): a size-1 axis reads index 0, a missing leading axis nothing.
fn broadcast_box(bx: &Bx, out_rank: usize, sh: &[usize]) -> Bx {
    let off = out_rank - sh.len();
    let mut o = NO_BOX;
    for (k, d) in sh.iter().enumerate() {
        o[k] = if *d == 1 { (0, 1) } else { bx[off + k] };
    }
    o
}

// =================================================================================================
// The twin
// =================================================================================================

/// One range request: the target, its element ranges, and the dissection's supplied nodes and range.
#[derive(Clone, Debug)]
pub struct PalwTirCloseRangeRequestV1<'r> {
    pub ctx: DemandContext,
    pub target: u16,
    pub ranges: &'r PalwTirRangesV1,
    /// Nodes of the target's context whose elements are a claim's values (never read from a unit).
    pub supplied: &'r [u16],
    /// The target's reduction runs over history positions `[from, to)` only.
    pub range: Option<(usize, usize)>,
    /// Both-mode: every history row is read as BOTH its history tile and its row commit.
    pub both: bool,
}

/// A block's node shapes at one history length, shared by every context of the block at it.
type Shapes = Rc<Vec<Vec<usize>>>;

/// What the twin keeps across requests: node shapes per `(block, H)`.
#[derive(Default)]
pub struct PalwTirRangeCacheV1 {
    shapes: HashMap<(u8, usize), Shapes>,
}

struct RCtx {
    block: u8,
    layer: Option<u16>,
    h: usize,
    shapes: Shapes,
    /// Pending demand: node → ranges (unnormalized until the node is visited).
    demand: BTreeMap<u16, Vec<(usize, usize)>>,
}

struct RangeTwin<'a> {
    /// A pipeline stage's reading of its inputs and its `post`-written states (RFC-0003 PALW-GEN-20); `None` for an IR class.
    gen_stage: Option<&'a PalwGenTwinStageV1>,
    space: &'a PalwTirStepSpaceV1,
    job_ctx: &'a PalwJobContextV2,
    job: PalwTirJobShapeV1,
    inventory: &'a PalwTirInventoryIndexV1,
    cache: &'a mut PalwTirRangeCacheV1,
    target: DemandContext,
    target_node: u16,
    supplied: BTreeSet<u16>,
    range: Option<(usize, usize)>,
    both: bool,
    ctxs: BTreeMap<(u32, u16), RCtx>,
    /// Contexts with pending demand.
    pending: BTreeSet<(u32, u16)>,
    reads: PalwTirCloseReadsV1,
    hist: PalwTirCloseReadsV1,
    /// A dissection claim's values read: node → ranges (unnormalized).
    supplied_out: BTreeMap<u16, Vec<(usize, usize)>>,
    in_hist: bool,
    /// Step leaves placed, `(through a history row, call, slot, position, tile)`.
    placed: HashSet<(bool, u32, u32, u32, u32)>,
    /// Param ranges already read, `(param, layer, start, end)`.
    params_seen: HashSet<(u16, Option<u16>, usize, usize)>,
    hist_only: bool,
    reaches_hist: Vec<bool>,
    hist_nodes: HashMap<(u16, Option<u16>), (u16, u16)>,
    pattern: Option<HistPattern>,
    leaf_cost: u64,
    work: u64,
    cap: u64,
}

impl RangeTwin<'_> {
    fn tick(&mut self, n: u64) -> Result<(), String> {
        self.work = self.work.saturating_add(n);
        if self.work > self.cap { Err(PALW_TIR_CLOSE_SIZING_OVER_CAP_V1.to_string()) } else { Ok(()) }
    }

    fn block_of(&self, occurrence: u16) -> Result<(u8, Option<u16>), String> {
        self.space.occurrences().get(occurrence as usize).copied().ok_or_else(|| "no such occurrence".to_string())
    }

    fn ctx(&mut self, key: DemandContext) -> Result<(), String> {
        let k = (key.pos, key.occurrence);
        if self.ctxs.contains_key(&k) {
            return Ok(());
        }
        let (block, layer) = self.block_of(key.occurrence)?;
        let h = history_length_v1(&self.space.info, block, key.pos).ok_or("the program's info does not cover the block")?;
        self.tick(1)?;
        let shapes = match self.cache.shapes.get(&(block, h)) {
            Some(s) => s.clone(),
            None => {
                let b = &self.space.program.blocks[block as usize];
                self.tick(b.nodes.len() as u64)?;
                let s: Shapes = Rc::new(b.nodes.iter().map(|n| n.out.resolve(h)).collect());
                self.cache.shapes.insert((block, h), s.clone());
                s
            }
        };
        self.ctxs.insert(k, RCtx { block, layer, h, shapes, demand: BTreeMap::new() });
        Ok(())
    }

    fn is_leaf(&self, key: DemandContext, node: u16) -> bool {
        let Ok((block, _)) = self.block_of(key.occurrence) else { return false };
        let committed = self.space.program.blocks[block as usize].nodes.get(node as usize).is_some_and(|n| n.commit);
        let in_target = key == self.target;
        (committed && !(in_target && node == self.target_node)) || (in_target && self.supplied.contains(&node))
    }

    fn sink(&mut self) -> &mut PalwTirCloseReadsV1 {
        if self.in_hist { &mut self.hist } else { &mut self.reads }
    }

    fn push_demand(&mut self, key: DemandContext, node: u16, ranges: Vec<(usize, usize)>) {
        let k = (key.pos, key.occurrence);
        self.ctxs.get_mut(&k).expect("live").demand.entry(node).or_default().extend(ranges);
        self.pending.insert(k);
    }

    /// Step leaves `coord(t)` for tiles `t0..=t1` (once each), tile `t` of `values(t)` lanes;
    /// `h_tile` marks an `H`-carrying commit point's tile. Consecutive tiles are consecutive leaves:
    /// one index lookup a run of leaves not yet placed.
    fn place_run(
        &mut self,
        coord: &dyn Fn(u64) -> PalwStepCoordinateV1,
        t0: u64,
        t1: u64,
        values: &dyn Fn(u64) -> u32,
        h_tile: bool,
    ) -> Result<(), String> {
        let mut base: Option<(u64, u64)> = None;
        for t in t0..=t1 {
            let c = coord(t);
            if !self.placed.insert((self.in_hist, c.call_index, c.node_slot, c.position, c.tile_index)) {
                continue;
            }
            let i = match base {
                Some((first_t, first_i)) => first_i + (t - first_t),
                None => {
                    self.tick(self.leaf_cost)?;
                    let i = self.space.leaf_index(self.job_ctx, &c).ok_or_else(|| format!("{c:?} is no leaf of the job"))?;
                    base = Some((t, i));
                    i
                }
            };
            self.tick(1)?;
            let marks = h_tile && !self.in_hist;
            let v = values(t);
            let sink = self.sink();
            sink.steps.insert(i, v);
            if marks {
                sink.h_steps.insert(i);
            }
        }
        Ok(())
    }

    /// The coordinates of reserved slot `reserved`'s tiles at `pos` (a checkpoint's, a history tile's).
    fn reserved_coords(&self, pos: u32, reserved: u32) -> impl Fn(u64) -> PalwStepCoordinateV1 + use<> {
        let (call_index, position) = self.job.call_position(pos);
        let node_slot = self.space.program_slots() + reserved;
        move |tile: u64| PalwStepCoordinateV1 { call_index, node_slot, position, tile_index: tile as u32 }
    }

    /// A committed node's elements `[a, b)` (the court source's `node`).
    fn leaf_read(&mut self, key: DemandContext, node: u16, a: usize, b: usize) -> Result<(), String> {
        if a >= b {
            return Ok(());
        }
        if key == self.target && self.supplied.contains(&node) {
            self.tick(1)?;
            self.supplied_out.entry(node).or_default().push((a, b));
            return Ok(());
        }
        let (block, _) = self.block_of(key.occurrence)?;
        let tile_len = self.space.commit_tile_len(block, node).ok_or_else(|| format!("node {node} is not a commit point"))? as usize;
        let (call_index, position) = self.job.call_position(key.pos);
        let slot = self.space.node_slot(key.occurrence as usize, node).ok_or("no such slot")?;
        let h = history_length_v1(&self.space.info, block, key.pos).ok_or("no block info")?;
        let out = &self.space.program.blocks[block as usize].nodes[node as usize].out;
        let count = element_count(&out.resolve(h));
        let has_h = out.has_h();
        let coord = |t: u64| PalwStepCoordinateV1 { call_index, node_slot: slot, position, tile_index: t as u32 };
        let values = |t: u64| tile_len.min(count.saturating_sub(t as usize * tile_len)) as u32;
        self.place_run(&coord, (a / tile_len) as u64, ((b - 1) / tile_len) as u64, &values, has_h)
    }

    /// The input a view param is, for a pipeline stage (`None` for a declared param, and for every param of an IR class).
    fn input_of(&self, param: u16) -> Option<u16> {
        self.gen_stage.and_then(|g| param.checked_sub(g.first_input))
    }

    /// **Elements `[a, b)` of input `input`, read as the generative court reads them** — the element twin's `input_read` over a
    /// range: a random or job-bound input opens nothing; an edge is the run of commit leaves of the upstream output node that hold
    /// the elements, row by row; an image's bytes are a run of input tiles. The same units as one element at a time.
    fn input_read(&mut self, input: u16, a: usize, b: usize) -> Result<(), String> {
        if a >= b {
            return Ok(());
        }
        self.tick(1)?;
        let g = self.gen_stage.ok_or("an input read outside a pipeline stage")?;
        let model = g.inputs.get(input as usize).ok_or_else(|| format!("no input {input}"))?.clone();
        match model {
            PalwGenTwinInputV1::Free => Ok(()),
            PalwGenTwinInputV1::Edge { stage, rows, tile, elements } => {
                let tile = tile.max(1) as u64;
                let mut put = |this: &mut Self, row: u32, lo: u64, hi: u64| -> Result<(), String> {
                    let (t0, t1) = (lo / tile, (hi - 1) / tile);
                    this.tick(t1 - t0 + 1)?;
                    for t in t0..=t1 {
                        let lanes = tile.min(elements.saturating_sub(t * tile)).max(1) as u32;
                        this.sink().edges.insert((stage, row, t as u32), lanes);
                    }
                    Ok(())
                };
                match rows {
                    Some((drop, per_row)) => {
                        let per_row = per_row.max(1);
                        let (r0, r1) = (a as u64 / per_row, (b as u64 - 1) / per_row);
                        self.tick(r1 - r0 + 1)?;
                        for r in r0..=r1 {
                            let lo = (a as u64).max(r * per_row) - r * per_row;
                            let hi = (b as u64).min((r + 1) * per_row) - r * per_row;
                            put(self, (r as u32).saturating_add(drop), lo, hi)?;
                        }
                        Ok(())
                    }
                    None => put(self, u32::MAX, a as u64, b as u64),
                }
            }
            PalwGenTwinInputV1::Image { image, tile_len } => {
                let tl = tile_len.max(1) as u64;
                let (t0, t1) = (a as u64 / tl, (b as u64 - 1) / tl);
                self.tick(t1 - t0 + 1)?;
                let sink = self.sink();
                for t in t0..=t1 {
                    sink.images.insert((image, t));
                }
                Ok(())
            }
        }
    }

    /// Param elements `[a, b)`: the inventory leaves holding their first bytes — a range of leaves
    /// (a piece and a row are whole elements).
    fn param_read(&mut self, param: u16, layer: Option<u16>, a: usize, b: usize) -> Result<(), String> {
        if a >= b {
            return Ok(());
        }
        self.tick(1)?;
        if !self.params_seen.insert((param, layer, a, b)) {
            return Ok(());
        }
        let d = self.space.program.params.get(param as usize).ok_or("no such param")?;
        let w = d.dtype.width() as u64;
        let (first_byte, last_byte) = ((a as u64).saturating_mul(w), ((b - 1) as u64).saturating_mul(w));
        let first =
            self.inventory.leaf_of(param, layer, first_byte).ok_or_else(|| format!("param {param} has no byte {first_byte}"))?;
        let last = self.inventory.leaf_of(param, layer, last_byte).ok_or_else(|| format!("param {param} has no byte {last_byte}"))?;
        self.tick((last - first) as u64 + 1)?;
        self.reads.params.extend(first..=last);
        Ok(())
    }

    /// A `Fixed` state's elements at the start of `pos` (the court's `state_at_start`).
    fn state_at_start(&mut self, pos: u32, state: u16, layer: Option<u16>, ranges: &PalwTirRangesV1) -> Result<(), String> {
        // **A state `post` writes** (a pipeline stage's, NF-29): its value at the start of `pos` is the committed write of `pos − 1`.
        if let Some(g) = self.gen_stage
            && let Some((_, (occurrence, node))) = g.post_writers.iter().find(|(s, _)| *s == state).copied()
        {
            self.tick(1)?;
            let Some(prev) = pos.checked_sub(1) else { return Ok(()) };
            for &(a, b) in ranges.ranges() {
                self.leaf_read(DemandContext { pos: prev, occurrence }, node, a, b)?;
            }
            return Ok(());
        }
        let c = self.space.layout.checkpoint_interval.max(1);
        let mut at = pos;
        loop {
            self.tick(1)?;
            if at == 0 {
                return Ok(());
            }
            if at.is_multiple_of(c) {
                let k = self.space.fixed_instance_index(state, layer).ok_or("a state with no checkpoint")?;
                let inst = self.space.fixed_instances()[k];
                let lanes = inst.tile_lanes as u64;
                let coord = self.reserved_coords(at - 1, k as u32);
                let values = |t: u64| lanes.min(inst.elements - t * lanes) as u32;
                for &(a, b) in ranges.ranges() {
                    self.place_run(&coord, a as u64 / lanes, (b as u64 - 1) / lanes, &values, false)?;
                }
                return Ok(());
            }
            match state_writer_v1(&self.space.program, state, layer) {
                Some((occ, w)) => {
                    let key = DemandContext { pos: at - 1, occurrence: occ };
                    self.ctx(key)?;
                    if self.is_leaf(key, w) {
                        for &(a, b) in ranges.ranges() {
                            self.leaf_read(key, w, a, b)?;
                        }
                    } else {
                        self.push_demand(key, w, ranges.ranges().to_vec());
                    }
                    return Ok(());
                }
                None => at -= 1,
            }
        }
    }

    /// A past history row's elements `within` (the court source's `hist_row`): its history tile once
    /// complete, the row's commit before — or, in both-mode, both. Recorded apart from every other read.
    fn hist_row(&mut self, pos: u32, state: u16, layer: Option<u16>, row_pos: u32, within: (usize, usize)) -> Result<(), String> {
        self.in_hist = true;
        let read = self.hist_row_reads(pos, state, layer, row_pos, within);
        self.in_hist = false;
        read
    }

    fn hist_row_reads(
        &mut self,
        pos: u32,
        state: u16,
        layer: Option<u16>,
        row_pos: u32,
        within: (usize, usize),
    ) -> Result<(), String> {
        let (a, b) = within;
        if a >= b {
            return Ok(());
        }
        self.tick(1)?;
        let h_tile = self.space.layout.h_tile.max(1);
        let tile_start = row_pos - row_pos % h_tile;
        let tile_end = tile_start + (h_tile - 1);
        let complete = tile_end < pos;
        if complete || self.both {
            let k = self.space.hist_instance_index(state, layer).ok_or("a history with no tiles")?;
            let inst = self.space.hist_instances()[k];
            let lanes = inst.tile_lanes as u64;
            let (s0, s1) = (a as u64 / lanes, (b as u64 - 1) / lanes);
            let values = |sub: u64| (h_tile as u64 * (inst.elements - sub * lanes).min(lanes)) as u32;
            if self.pattern.is_some() {
                self.tick(s1 - s0 + 1)?;
            }
            if let Some(pattern) = self.pattern.as_mut() {
                for sub in s0..=s1 {
                    pattern.tiles.insert((k, sub), values(sub));
                }
            }
            if tile_end < self.job.positions {
                let coord = self.reserved_coords(tile_end, (self.space.fixed_instances().len() + k) as u32);
                self.place_run(&coord, s0, s1, &values, false)?;
            } else {
                for sub in s0..=s1 {
                    self.tick(1)?;
                    if self.placed.insert((true, u32::MAX, k as u32, tile_end, sub as u32)) {
                        self.hist.loose_steps.push(values(sub));
                    }
                }
            }
        }
        if !complete || self.both {
            let (occurrence, node) = match self.hist_nodes.get(&(state, layer)) {
                Some(x) => *x,
                None => {
                    let (key, node) =
                        hist_row_node_v1(&self.space.program, state, layer, 0).ok_or("a history that appends nothing")?;
                    self.hist_nodes.insert((state, layer), (key.occurrence, node));
                    (key.occurrence, node)
                }
            };
            let key = DemandContext { pos: row_pos, occurrence };
            if self.pattern.is_some() {
                let (block, _) = self.block_of(occurrence)?;
                let tile_len = self.space.commit_tile_len(block, node).ok_or("a history row that is no commit point")? as usize;
                let h = history_length_v1(&self.space.info, block, row_pos).ok_or("no block info")?;
                let count = element_count(&self.space.program.blocks[block as usize].nodes[node as usize].out.resolve(h));
                self.tick(((b - 1) / tile_len - a / tile_len + 1) as u64)?;
                if let Some(pattern) = self.pattern.as_mut() {
                    for tile in a / tile_len..=(b - 1) / tile_len {
                        let values = tile_len.min(count.saturating_sub(tile * tile_len)) as u32;
                        pattern.commits.insert((occurrence, node, tile as u32), values);
                    }
                }
            }
            self.leaf_read(key, node, a, b)?;
        }
        Ok(())
    }

    /// Whether a history-only read skips operand `r` of context `key`.
    fn skips(&self, key: DemandContext, r: Ref) -> bool {
        self.hist_only && !(key == self.target && matches!(r, Ref::Node(j) if !self.is_leaf(key, j) && self.reaches_hist[j as usize]))
    }

    /// Operand `r`'s elements `ranges` (any order, overlapping or not) of context `key`.
    fn want(&mut self, key: DemandContext, r: Ref, ranges: Vec<(usize, usize)>) -> Result<(), String> {
        self.tick(ranges.len() as u64)?;
        if ranges.is_empty() || self.skips(key, r) {
            return Ok(());
        }
        let program = &self.space.program;
        match r {
            Ref::Node(j) => {
                if self.is_leaf(key, j) {
                    for (a, b) in PalwTirRangesV1::from_ranges(ranges).0 {
                        self.leaf_read(key, j, a, b)?;
                    }
                } else {
                    self.push_demand(key, j, ranges);
                }
                Ok(())
            }
            Ref::CarryIn(k) => {
                let (prev, node) = carry_in_node_v1(program, key, k).ok_or("a carry-in with no carry-out")?;
                for (a, b) in PalwTirRangesV1::from_ranges(ranges).0 {
                    self.leaf_read(prev, node, a, b)?;
                }
                Ok(())
            }
            Ref::Param(j) => {
                if let Some(input) = self.input_of(j) {
                    for (a, b) in PalwTirRangesV1::from_ranges(ranges).0 {
                        self.input_read(input, a, b)?;
                    }
                    return Ok(());
                }
                let d = program.params.get(j as usize).ok_or("no such param")?;
                let layer = if d.per_layer { self.ctxs[&(key.pos, key.occurrence)].layer } else { None };
                for (a, b) in PalwTirRangesV1::from_ranges(ranges).0 {
                    self.param_read(j, layer, a, b)?;
                }
                Ok(())
            }
            Ref::Const(_) => Ok(()),
            Ref::State(j) => {
                let s = program.states.get(j as usize).ok_or("no such state")?;
                let layer = if s.per_layer { self.ctxs[&(key.pos, key.occurrence)].layer } else { None };
                self.state_at_start(key.pos, j, layer, &PalwTirRangesV1::from_ranges(ranges))
            }
            Ref::Input(0) => {
                self.reads.token = true;
                Ok(())
            }
            Ref::Input(_) => Ok(()),
        }
    }

    fn operand_shape(&self, key: DemandContext, r: Ref) -> Result<Vec<usize>, String> {
        let p = &self.space.program;
        let st = &self.ctxs[&(key.pos, key.occurrence)];
        Ok(match r {
            Ref::Node(j) => st.shapes.get(j as usize).ok_or("no such node")?.clone(),
            Ref::CarryIn(k) => p.blocks[st.block as usize].carry_in.get(k as usize).ok_or("no such carry-in")?.resolve(st.h),
            Ref::Param(j) => p.params.get(j as usize).ok_or("no such param")?.shape.iter().map(|d| *d as usize).collect(),
            Ref::Const(j) => p.consts.get(j as usize).ok_or("no such const")?.shape.iter().map(|d| *d as usize).collect(),
            Ref::State(j) => p.states.get(j as usize).ok_or("no such state")?.shape.iter().map(|d| *d as usize).collect(),
            Ref::Input(_) => Vec::new(),
        })
    }

    fn span(&self, key: DemandContext, node: u16, n: usize) -> (usize, usize) {
        match self.range {
            Some((from, to)) if key == self.target && node == self.target_node => (from.min(n), to.min(n)),
            _ => (0, n),
        }
    }

    /// The boxes of `ranges` in shape `out`, each counted as a step.
    fn boxes(&mut self, ranges: &[(usize, usize)], out: &[usize]) -> Result<Vec<Bx>, String> {
        let mut bxs = Vec::with_capacity(ranges.len());
        for &(a, b) in ranges {
            boxes_of(a, b, out, &mut bxs);
        }
        self.tick(bxs.len() as u64)?;
        Ok(bxs)
    }

    /// Operand `r` (of shape `sh`) at the box `map` gives for each output box: its ranges, wanted.
    fn want_boxes(&mut self, key: DemandContext, r: Ref, bxs: &[Bx], sh: &[usize], map: &dyn Fn(&Bx) -> Bx) -> Result<(), String> {
        if self.skips(key, r) {
            return Ok(());
        }
        let st = strides(sh);
        let mut ranges = Vec::new();
        for bx in bxs {
            let mapped = map(bx);
            self.tick(box_range_count(&mapped, sh))?;
            box_ranges(&mapped, sh, &st, &mut ranges);
        }
        self.want(key, r, ranges)
    }

    /// Operand `r` broadcast to the output (shape `out`): the same ranges where its shape is the
    /// output's, else each box's broadcast image.
    fn want_broadcast(&mut self, key: DemandContext, r: Ref, ranges: &[(usize, usize)], out: &[usize]) -> Result<(), String> {
        let sh = self.operand_shape(key, r)?;
        if sh == out {
            return self.want(key, r, ranges.to_vec());
        }
        let bxs = self.boxes(ranges, out)?;
        let rank = out.len();
        self.want_boxes(key, r, &bxs, &sh, &|bx| broadcast_box(bx, rank, &sh))
    }

    /// What elements `ranges` of computed node `node` read (the court's `step`, over ranges).
    fn visit(&mut self, key: DemandContext, node: u16, ranges: &[(usize, usize)]) -> Result<(), String> {
        self.tick(ranges.len() as u64)?;
        let space = self.space;
        let (block, h, shapes, layer) = {
            let st = &self.ctxs[&(key.pos, key.occurrence)];
            (st.block, st.h, st.shapes.clone(), st.layer)
        };
        let out = &shapes[node as usize];
        let rank = out.len();
        let n = &space.program.blocks[block as usize].nodes[node as usize];
        let inputs = &n.inputs;
        match n.prim {
            Prim::Reshape
            | Prim::Cast
            | Prim::Clamp { .. }
            | Prim::Log2Floor
            | Prim::IntExp
            | Prim::IntRsqrt
            | Prim::IntLn
            | Prim::StateWrite { .. } => self.want(key, inputs[0], ranges.to_vec()),
            Prim::Transpose { ref perm } => {
                let ish = self.operand_shape(key, inputs[0])?;
                let bxs = self.boxes(ranges, out)?;
                self.want_boxes(key, inputs[0], &bxs, &ish, &|bx| {
                    let mut j = NO_BOX;
                    for (k, p) in perm.iter().enumerate() {
                        j[*p as usize] = bx[k];
                    }
                    j
                })
            }
            Prim::Slice { axis, start, .. } => {
                let ish = self.operand_shape(key, inputs[0])?;
                let (a, s) = (axis as usize, start as usize);
                let bxs = self.boxes(ranges, out)?;
                self.want_boxes(key, inputs[0], &bxs, &ish, &|bx| {
                    let mut j = *bx;
                    j[a] = (bx[a].0 + s, bx[a].1 + s);
                    j
                })
            }
            Prim::Concat { axis } => {
                let a = axis as usize;
                let bxs = self.boxes(ranges, out)?;
                let total: usize = inputs.iter().map(|r| self.operand_shape(key, *r).map(|t| t[a])).sum::<Result<usize, String>>()?;
                if bxs.iter().any(|bx| bx[a].1 > total) {
                    return Err("Concat: the inputs do not cover the output".into());
                }
                let mut off = 0usize;
                for r in inputs.iter() {
                    let t = self.operand_shape(key, *r)?;
                    let ta = t[a];
                    let part: Vec<Bx> = bxs
                        .iter()
                        .filter(|bx| bx[a].0.max(off) < bx[a].1.min(off + ta))
                        .map(|bx| {
                            let mut j = *bx;
                            j[a] = (bx[a].0.max(off) - off, bx[a].1.min(off + ta) - off);
                            j
                        })
                        .collect();
                    if !part.is_empty() {
                        self.want_boxes(key, *r, &part, &t, &|bx| *bx)?;
                    }
                    off += ta;
                }
                Ok(())
            }
            Prim::Broadcast => self.want_broadcast(key, inputs[0], ranges, out),
            Prim::Iota { .. } => Ok(()),
            Prim::Gather { axis, batch_dims } => {
                let (a, bd) = (axis as usize, batch_dims as usize);
                let dsh = self.operand_shape(key, inputs[0])?;
                let xsh = self.operand_shape(key, inputs[1])?;
                let m = xsh.len() - bd;
                let bxs = self.boxes(ranges, out)?;
                let index_box = |bx: &Bx| {
                    let mut xb = NO_BOX;
                    xb[..bd].copy_from_slice(&bx[..bd]);
                    xb[bd..bd + m].copy_from_slice(&bx[a..a + m]);
                    xb
                };
                self.want_boxes(key, inputs[1], &bxs, &xsh, &index_box)?;
                // The data element sits at a VALUE's index along `a`: a location-free row for a param
                // gathered along its rows, the whole fiber otherwise.
                match inputs[0] {
                    Ref::Const(_) => Ok(()),
                    Ref::Param(pj) if a == 0 && dsh.len() >= 2 && self.input_of(pj).is_none() => {
                        let d = &space.program.params[pj as usize];
                        let layer = if d.per_layer { layer } else { None };
                        let width = d.dtype.width() as u64;
                        let row_sh = &dsh[1..];
                        let row_st = strides(row_sh);
                        let xst = strides(&xsh);
                        for bx in &bxs {
                            // The row's pieces the box reads: its row sub-box's elements' first bytes.
                            let mut rb = NO_BOX;
                            rb[..row_sh.len()].copy_from_slice(&bx[m..m + row_sh.len()]);
                            let mut within = Vec::new();
                            self.tick(box_range_count(&rb, row_sh))?;
                            box_ranges(&rb, row_sh, &row_st, &mut within);
                            let mut pieces: BTreeSet<u64> = BTreeSet::new();
                            for (w0, w1) in within {
                                let (p0, p1) = (
                                    w0 as u64 * width / PALW_TIR_ROW_PIECE_BYTES_V1,
                                    (w1 as u64 - 1) * width / PALW_TIR_ROW_PIECE_BYTES_V1,
                                );
                                pieces.extend(p0..=p1);
                            }
                            // Each index element of the box names a row of its own.
                            let xb = index_box(bx);
                            let rows = box_elements(&xb, xsh.len());
                            self.tick(rows.saturating_mul(1 + pieces.len() as u64))?;
                            let mut xranges = Vec::new();
                            box_ranges(&xb, &xsh, &xst, &mut xranges);
                            for (x0, x1) in xranges {
                                for xflat in x0..x1 {
                                    self.reads
                                        .wild_rows
                                        .entry((pj, layer, (key.pos, key.occurrence, node, xflat)))
                                        .or_default()
                                        .extend(pieces.iter().copied());
                                }
                            }
                        }
                        Ok(())
                    }
                    r => {
                        let full = dsh[a];
                        self.want_boxes(key, r, &bxs, &dsh, &|bx| {
                            let mut di = NO_BOX;
                            di[..a].copy_from_slice(&bx[..a]);
                            di[a] = (0, full);
                            for (k, slot) in di.iter_mut().enumerate().take(dsh.len()).skip(a + 1) {
                                *slot = bx[k - 1 + m];
                            }
                            di
                        })
                    }
                }
            }
            Prim::Add | Prim::Sub | Prim::Mul | Prim::Div { .. } | Prim::Compare { .. } => {
                for r in [inputs[0], inputs[1]] {
                    self.want_broadcast(key, r, ranges, out)?;
                }
                Ok(())
            }
            // A choice the court makes by a value: the condition and BOTH operands.
            Prim::Select => {
                for r in [inputs[0], inputs[1], inputs[2]] {
                    self.want_broadcast(key, r, ranges, out)?;
                }
                Ok(())
            }
            Prim::MatMul => {
                let xs = self.operand_shape(key, inputs[0])?;
                let ys = self.operand_shape(key, inputs[1])?;
                let (xr, yr) = (xs.len(), ys.len());
                let kk = xs[xr - 1];
                let (from, to) = self.span(key, node, kk);
                if from >= to {
                    return Ok(());
                }
                let bxs = self.boxes(ranges, out)?;
                let batch = rank - 2;
                let (xoff, yoff) = (batch - (xr - 2), batch - (yr - 2));
                self.want_boxes(key, inputs[0], &bxs, &xs, &|bx| {
                    let mut j = NO_BOX;
                    for k in 0..xr - 2 {
                        j[k] = if xs[k] == 1 { (0, 1) } else { bx[xoff + k] };
                    }
                    j[xr - 2] = bx[rank - 2];
                    j[xr - 1] = (from, to);
                    j
                })?;
                self.want_boxes(key, inputs[1], &bxs, &ys, &|bx| {
                    let mut j = NO_BOX;
                    for k in 0..yr - 2 {
                        j[k] = if ys[k] == 1 { (0, 1) } else { bx[yoff + k] };
                    }
                    j[yr - 2] = (from, to);
                    j[yr - 1] = bx[rank - 1];
                    j
                })
            }
            Prim::ReduceSum { axis } | Prim::ReduceMax { axis } => {
                let a = axis as usize;
                let xs = self.operand_shape(key, inputs[0])?;
                let (from, to) = self.span(key, node, xs[a]);
                if from >= to {
                    return Ok(());
                }
                let bxs = self.boxes(ranges, out)?;
                self.want_boxes(key, inputs[0], &bxs, &xs, &|bx| {
                    let mut j = *bx;
                    j[a] = (from, to);
                    j
                })
            }
            Prim::TopK { axis, .. } => {
                let a = axis as usize;
                let xs = self.operand_shape(key, inputs[0])?;
                let bxs = self.boxes(ranges, out)?;
                let full = xs[a];
                self.want_boxes(key, inputs[0], &bxs, &xs, &|bx| {
                    let mut j = *bx;
                    j[a] = (0, full);
                    j
                })
            }
            Prim::HistAppend { state } => {
                let s = space.program.states.get(state as usize).ok_or("no such state")?;
                let StateKind::Hist { .. } = s.kind else { return Err("HistAppend on a Fixed state".into()) };
                let row_len: usize = s.shape.iter().map(|d| *d as usize).product();
                if row_len == 0 {
                    return Err("an empty history row".into());
                }
                let layer = if s.per_layer { layer } else { None };
                let mut own = Vec::new();
                for &(a, b) in ranges {
                    let (t0, t1) = (a / row_len, (b - 1) / row_len);
                    self.tick((t1 - t0 + 1) as u64)?;
                    for t in t0..=t1 {
                        let w0 = if t == t0 { a - t * row_len } else { 0 };
                        let w1 = if t == t1 { b - t * row_len } else { row_len };
                        if t + 1 == h {
                            own.push((w0, w1));
                        } else {
                            let row_pos = (key.pos as usize + 1 - h + t) as u32;
                            self.hist_row(key.pos, state, layer, row_pos, (w0, w1))?;
                        }
                    }
                }
                if own.is_empty() { Ok(()) } else { self.want(key, inputs[0], own) }
            }
        }
    }

    fn run(&mut self) -> Result<(), String> {
        // The largest context with pending demand first: every demand into a context comes from a
        // later one (a replay reads an earlier position) or a later node of its own.
        while let Some(k) = self.pending.pop_last() {
            let key = DemandContext { pos: k.0, occurrence: k.1 };
            while let Some((node, raw)) = self.ctxs.get_mut(&k).expect("live").demand.pop_last() {
                let ranges = PalwTirRangesV1::from_ranges(raw);
                self.tick(1)?;
                if self.is_leaf(key, node) {
                    for &(a, b) in ranges.ranges() {
                        self.leaf_read(key, node, a, b)?;
                    }
                    continue;
                }
                self.visit(key, node, ranges.ranges())?;
            }
            self.pending.remove(&k);
        }
        Ok(())
    }
}

/// A range read's outcome: everything but the history rows, the history rows, the supplied elements
/// read, the row pattern when asked for, and the work done.
pub struct PalwTirRangeSplitV1 {
    pub reads: PalwTirCloseReadsV1,
    pub hist: PalwTirCloseReadsV1,
    pub supplied: BTreeMap<u16, PalwTirRangesV1>,
    pub pattern: HistPattern,
    pub work: u64,
}

/// **The range twin's reads of `request`**, split — history-only (`hist_only`) or recording the row
/// pattern (`pattern`) when asked — within `cap` steps; `cache` carries node shapes across requests.
#[allow(clippy::too_many_arguments)]
pub fn palw_tir_close_reads_range_split_v1(
    space: &PalwTirStepSpaceV1,
    job_ctx: &PalwJobContextV2,
    inventory: &PalwTirInventoryIndexV1,
    cache: &mut PalwTirRangeCacheV1,
    request: &PalwTirCloseRangeRequestV1<'_>,
    cap: u64,
    hist_only: bool,
    pattern: bool,
) -> Result<PalwTirRangeSplitV1, String> {
    palw_tir_close_reads_range_split_gen_v1(space, job_ctx, inventory, cache, request, cap, hist_only, pattern, None)
}

/// [`palw_tir_close_reads_range_split_v1`] over a pipeline stage's view (`gen_stage`: its inputs read where the generative court
/// reads them, its `post`-written states as the committed write of the position before) — the element twin's
/// `close_reads_split` with `Some(stage)`, as ranges.
#[allow(clippy::too_many_arguments)]
pub fn palw_tir_close_reads_range_split_gen_v1(
    space: &PalwTirStepSpaceV1,
    job_ctx: &PalwJobContextV2,
    inventory: &PalwTirInventoryIndexV1,
    cache: &mut PalwTirRangeCacheV1,
    request: &PalwTirCloseRangeRequestV1<'_>,
    cap: u64,
    hist_only: bool,
    pattern: bool,
    gen_stage: Option<&PalwGenTwinStageV1>,
) -> Result<PalwTirRangeSplitV1, String> {
    let job = space.job_shape(job_ctx).map_err(|e| e.to_string())?;
    let (block, _) = space.occurrences().get(request.ctx.occurrence as usize).copied().ok_or("no such occurrence")?;
    let leaf_cost = leaf_cost_of(space);
    let mut twin = RangeTwin {
        gen_stage,
        space,
        job_ctx,
        job,
        inventory,
        cache,
        target: request.ctx,
        target_node: request.target,
        supplied: request.supplied.iter().copied().collect(),
        range: request.range,
        both: request.both,
        ctxs: BTreeMap::new(),
        pending: BTreeSet::new(),
        reads: PalwTirCloseReadsV1::default(),
        hist: PalwTirCloseReadsV1::default(),
        supplied_out: BTreeMap::new(),
        in_hist: false,
        placed: HashSet::new(),
        params_seen: HashSet::new(),
        hist_only,
        reaches_hist: if hist_only { reaches_hist(&space.program.blocks[block as usize], request.target) } else { Vec::new() },
        hist_nodes: HashMap::new(),
        pattern: pattern.then(HistPattern::default),
        leaf_cost,
        work: 0,
        cap,
    };
    // A request's setup: its seed, and (history-only) the block's reach.
    twin.tick(
        32 + request.ranges.ranges().len() as u64
            + if hist_only { space.program.blocks[block as usize].nodes.len() as u64 } else { 0 },
    )?;
    twin.ctx(request.ctx)?;
    {
        let st = twin.ctxs.get(&(request.ctx.pos, request.ctx.occurrence)).expect("made above");
        let count = element_count(st.shapes.get(request.target as usize).ok_or("no such target")?);
        if let Some((_, end)) = request.ranges.ranges().last()
            && *end > count
        {
            return Err(format!("element {} outside the target's {count}", end - 1));
        }
    }
    if !request.ranges.is_empty() {
        // The target is computed even when committed: seed it, then run.
        twin.push_demand(request.ctx, request.target, request.ranges.ranges().to_vec());
    }
    twin.run()?;
    let supplied = std::mem::take(&mut twin.supplied_out).into_iter().map(|(n, v)| (n, PalwTirRangesV1::from_ranges(v))).collect();
    Ok(PalwTirRangeSplitV1 {
        reads: twin.reads,
        hist: twin.hist,
        supplied,
        pattern: twin.pattern.unwrap_or_default(),
        work: twin.work,
    })
}

/// **The units the court's evaluation of `request` reads**, as [`crate::palw_tir_close_size_v1::palw_tir_close_reads_v1`]
/// reports them (every supplied element listed), by the range twin; with the work done.
pub fn palw_tir_close_reads_range_v1(
    space: &PalwTirStepSpaceV1,
    job_ctx: &PalwJobContextV2,
    inventory: &PalwTirInventoryIndexV1,
    request: &PalwTirCloseRangeRequestV1<'_>,
    cap: u64,
) -> Result<(PalwTirCloseReadsV1, u64), String> {
    let mut cache = PalwTirRangeCacheV1::default();
    let PalwTirRangeSplitV1 { mut reads, hist, supplied, work, .. } =
        palw_tir_close_reads_range_split_v1(space, job_ctx, inventory, &mut cache, request, cap, false, false)?;
    reads.merge(&hist);
    for (n, r) in &supplied {
        reads.supplied.extend(r.iter_elements().map(|e| (*n, e)));
    }
    Ok((reads, work))
}

// =================================================================================================
// The worst close of every commit point, over every job — the element twin's driver, as ranges
// =================================================================================================

/// **[`crate::palw_tir_close_size_v1::palw_tir_worst_closes_work_v1`] by the range twin**: the same
/// bound of every commit point (the same requests, as ranges; the same prices), with the work done.
pub fn palw_tir_worst_closes_range_work_v1(
    space: &PalwTirStepSpaceV1,
    inventory: &PalwTirInventoryIndexV1,
    job_ctx: &PalwJobContextV2,
    sizing: &PalwTirCloseSizingV1,
) -> Result<(Vec<PalwTirCloseBoundV1>, u64), String> {
    let price = PalwTirClosePriceV1::new(space, inventory, job_ctx, sizing.form)?;
    worst_closes_range_priced(space, inventory, job_ctx, sizing, price, None, None)
}

/// **Diagnostics only** (RFC-0011 §4.A, `cap_exceeded_at`): [`palw_tir_worst_closes_range_work_v1`], recording after each commit
/// point `(block, node, work done so far)` into `trace`. Decides nothing: the bounds and the work are the range twin's.
pub fn palw_tir_worst_closes_range_trace_v1(
    space: &PalwTirStepSpaceV1,
    inventory: &PalwTirInventoryIndexV1,
    job_ctx: &PalwJobContextV2,
    sizing: &PalwTirCloseSizingV1,
    trace: &mut Vec<(u8, u16, u64)>,
) -> Result<(Vec<PalwTirCloseBoundV1>, u64), String> {
    let price = PalwTirClosePriceV1::new(space, inventory, job_ctx, sizing.form)?;
    worst_closes_range_priced(space, inventory, job_ctx, sizing, price, None, Some(trace))
}

/// **[`crate::palw_tir_close_size_v1::palw_gen_worst_closes_v1`] by the range twin** (RFC-0003 PALW-GEN-20 under
/// `Params::palw_gen_range_twin_v1`): pipeline stage `stage`'s worst terminal closes — the same requests as ranges, the same
/// generative prices, its checkpoint leaves included — with the work done.
#[allow(clippy::too_many_arguments)]
pub fn palw_gen_worst_closes_range_v1(
    space: &PalwTirStepSpaceV1,
    inventory: &PalwTirInventoryIndexV1,
    job_ctx: &PalwJobContextV2,
    sizing: &PalwTirCloseSizingV1,
    stage: usize,
    class_inventory_leaves: u32,
    model: &PalwGenTwinStageV1,
    pricing: PalwGenClosePricingV1,
) -> Result<(Vec<PalwTirCloseBoundV1>, u64), String> {
    let price = PalwTirClosePriceV1::generative(space, inventory, stage, class_inventory_leaves, pricing)?;
    worst_closes_range_priced(space, inventory, job_ctx, sizing, price, Some(model), None)
}

fn worst_closes_range_priced(
    space: &PalwTirStepSpaceV1,
    inventory: &PalwTirInventoryIndexV1,
    job_ctx: &PalwJobContextV2,
    sizing: &PalwTirCloseSizingV1,
    price: PalwTirClosePriceV1<'_>,
    gen_stage: Option<&PalwGenTwinStageV1>,
    mut trace: Option<&mut Vec<(u8, u16, u64)>>,
) -> Result<(Vec<PalwTirCloseBoundV1>, u64), String> {
    let program = &space.program;
    let job = space.job_shape(job_ctx).map_err(|e| e.to_string())?;
    let p_max = job.positions.checked_sub(1).ok_or("a job with no position")?;
    let c = space.layout.checkpoint_interval.max(1);
    let has_fixed = !space.fixed_instances().is_empty();
    // The position of `[lo, p_max]` whose replay is the longest: the largest `≡ C − 1 (mod C)`, or
    // the last one where none is (the replay then grows to the end).
    let rep_from = |lo: u32| -> u32 {
        if !has_fixed || p_max + 1 < c {
            return p_max;
        }
        let p = p_max - ((p_max + 1) % c);
        if p >= lo { p } else { p_max }
    };
    let h_tile = space.layout.h_tile.max(1) as usize;
    let depth = price.depth();
    // The misalignment allowance of a part read at one alignment (the dissected bound's): one more
    // step leaf per run.
    let slack = |reads: &PalwTirCloseReadsV1| {
        let widest = reads.steps.values().chain(reads.loose_steps.iter()).copied().max().unwrap_or(0);
        step_runs(reads) * (step_preimage_bytes(widest) + 4 + 2 * 64 * depth)
    };
    // The work left, shared by the twin's reads and the counting outside them.
    let budget = Cell::new(sizing.cap);
    let charge = |n: u64| -> Result<(), String> {
        let left = budget.get();
        if n > left {
            return Err(PALW_TIR_CLOSE_SIZING_OVER_CAP_V1.to_string());
        }
        budget.set(left - n);
        Ok(())
    };
    let cache = RefCell::new(PalwTirRangeCacheV1::default());
    let occurrences: Vec<(u8, Option<u16>)> = space.occurrences().to_vec();
    // One occurrence per block kind and predecessor: `pre`, the first two layers, `post`.
    let mut chosen: Vec<u16> = Vec::new();
    for (o, (b, _)) in occurrences.iter().enumerate() {
        if chosen.iter().filter(|x| occurrences[**x as usize].0 == *b).count() < 2 {
            chosen.push(o as u16);
        }
    }
    // A leaf priced as a run of its own.
    let lone = |values: u32| step_preimage_bytes(values) + 4 + 64 * depth;
    let mut out = Vec::new();
    for (bi, block) in program.blocks.iter().enumerate() {
        for (ni, node) in block.nodes.iter().enumerate() {
            if !node.commit {
                continue;
            }
            let (bi8, ni16) = (bi as u8, ni as u16);
            let tile_len = space.commit_tile_len(bi8, ni16).ok_or("a commit point with no tile")? as usize;
            let reductions = crate::palw_tir_dissect_v1::palw_tir_cone_reductions_v1(block, ni16);
            let dissected = sizing.court && !reductions.is_empty();
            let h_axes = node.out.shape.iter().filter(|d| d.is_h()).count();
            if h_axes > 1 {
                return Err(format!("block {bi} node {ni} carries {h_axes} history axes; the sizing covers one"));
            }
            let has_h = h_axes == 1;
            let h_axis = node.out.shape.iter().position(|d| d.is_h());
            let inner = match h_axis {
                Some(a) => node.out.resolve(1)[a + 1..].iter().product::<usize>().max(1),
                None => 1,
            };
            let h_at = |pos: u32| history_length_v1(&space.info, bi8, pos).unwrap_or(1);
            let count_at = |pos: u32| element_count(&node.out.resolve(h_at(pos)));
            // Every tile of `pos`, `(first element, length)`: its elements are made one tile at a time.
            let tiles_at = |pos: u32| {
                let count = count_at(pos);
                (0..count).step_by(tile_len).map(move |first| (first, tile_len.min(count - first)))
            };
            // The first position at which a tile spans at most two row-parts.
            let mut p_late = 0u32;
            if has_h {
                while p_late <= p_max && h_at(p_late) * inner < tile_len {
                    p_late += 1;
                }
            }
            // An `H`-carrying tile whose cone reduces nothing over `H` is H-LOCAL (spec 04b §10.3).
            let local = !dissected && has_h && reductions.is_empty() && !has_fixed && p_late <= p_max;
            let mut worst =
                PalwTirCloseBoundV1 { block: bi8, node: ni16, checkpoint: None, dissected, close_bytes: 0, root_claim_bytes: 0 };
            for occ in chosen.iter().copied().filter(|o| occurrences[*o as usize].0 == bi8) {
                let twin = |pos: u32,
                            ranges: &PalwTirRangesV1,
                            supplied: &[u16],
                            target: u16,
                            range: Option<(usize, usize)>,
                            both: bool,
                            hist_only: bool,
                            pattern: bool| {
                    let request = PalwTirCloseRangeRequestV1 {
                        ctx: DemandContext { pos, occurrence: occ },
                        target,
                        ranges,
                        supplied,
                        range,
                        both,
                    };
                    let split = palw_tir_close_reads_range_split_gen_v1(
                        space,
                        job_ctx,
                        inventory,
                        &mut cache.borrow_mut(),
                        &request,
                        budget.get(),
                        hist_only,
                        pattern,
                        gen_stage,
                    )?;
                    budget.set(budget.get().saturating_sub(split.work));
                    Ok::<_, String>(split)
                };
                // A whole read: `(everything but the history rows, the history rows, supplied)`.
                macro_rules! read {
                    ($pos:expr, $ranges:expr, $supplied:expr, $target:expr, $range:expr, $both:expr) => {{
                        let split = twin($pos, $ranges, $supplied, $target, $range, $both, false, false)?;
                        (split.reads, split.hist, split.supplied)
                    }};
                }
                let row =
                    |h: usize, o: usize, from: usize, to: usize| PalwTirRangesV1::single((o * h + from) * inner, (o * h + to) * inner);
                if local {
                    let pl = rep_from(p_late);
                    let h = h_at(pl);
                    let a = h_axis.expect("has H");
                    let outer: usize = node.out.resolve(h)[..a].iter().product::<usize>().max(1);
                    let t_len = (tile_len.div_ceil(inner) + 1).min(h);
                    let mut ends = Vec::with_capacity(outer);
                    let mut starts = Vec::with_capacity(outer);
                    let mut tiles_cost = Vec::with_capacity(outer);
                    let mut commits_cost = Vec::with_capacity(outer);
                    let mut patterns = Vec::with_capacity(outer);
                    for o in 0..outer {
                        ends.push(read!(pl, &row(h, o, h - t_len, h), &[], ni16, None, false).0);
                        starts.push(read!(pl, &row(h, o, 0, t_len), &[], ni16, None, false).0);
                        let pattern = if h >= 2 {
                            twin(pl, &row(h, o, h - 2, h - 1), &[], ni16, None, true, true, true)?.pattern
                        } else {
                            HistPattern::default()
                        };
                        tiles_cost.push(pattern.tiles.values().map(|v| lone(*v)).sum::<u64>());
                        commits_cost.push(pattern.commits.values().map(|v| lone(*v)).sum::<u64>());
                        patterns.push(pattern);
                    }
                    let mut outside_of: BTreeMap<(usize, usize), u64> = BTreeMap::new();
                    let mut outside = |lo: usize, hi: usize| -> Result<u64, String> {
                        if let Some(units) = outside_of.get(&(lo, hi)) {
                            return Ok(*units);
                        }
                        let mut u = PalwTirCloseReadsV1::default();
                        for o in lo..=hi {
                            charge(16 + size_of_reads(&ends[o]) + size_of_reads(&starts[o]))?;
                            u.merge(&ends[o]);
                            u.merge(&starts[o]);
                        }
                        let units = price.units(&u, false) + price.h_allowance(&u);
                        outside_of.insert((lo, hi), units);
                        Ok(units)
                    };
                    for p in 0..p_late.min(p_max + 1) {
                        let hp = h_at(p);
                        let row_len = hp * inner;
                        let count = count_at(p);
                        for first in (0..count).step_by(tile_len) {
                            let last = (first + tile_len).min(count) - 1;
                            let (o_lo, o_hi) = (first / row_len, last / row_len);
                            let mut tiles: BTreeSet<(u32, usize, u64)> = BTreeSet::new();
                            let mut commits: BTreeSet<(u32, u16, u16, u32)> = BTreeSet::new();
                            let mut hist_cost = 0u64;
                            charge(16 + (o_hi - o_lo + 1) as u64)?;
                            for (o, pattern) in patterns.iter().enumerate().take(o_hi + 1).skip(o_lo) {
                                let t_lo = if o == o_lo { (first % row_len) / inner } else { 0 };
                                let t_hi = if o == o_hi { (last % row_len) / inner + 1 } else { hp };
                                let rows = t_hi.min(hp.saturating_sub(1)).saturating_sub(t_lo) as u64;
                                charge(rows * (1 + (pattern.tiles.len() + pattern.commits.len()) as u64))?;
                                for t in t_lo..t_hi.min(hp.saturating_sub(1)) {
                                    let row_pos = (p as usize + 1 - hp + t) as u32;
                                    let group = row_pos / h_tile as u32;
                                    if (group as usize + 1) * h_tile - 1 < p as usize {
                                        for ((k, sub), v) in &pattern.tiles {
                                            if tiles.insert((group, *k, *sub)) {
                                                hist_cost += lone(*v);
                                            }
                                        }
                                    } else {
                                        for ((oc, n, tile), v) in &pattern.commits {
                                            if commits.insert((row_pos, *oc, *n, *tile)) {
                                                hist_cost += lone(*v);
                                            }
                                        }
                                    }
                                }
                            }
                            let close = price.frame((last + 1 - first) as u32) + outside(o_lo, o_hi)? + hist_cost;
                            worst.close_bytes = worst.close_bytes.max(close);
                        }
                    }
                    let groups = (t_len.saturating_sub(1)).div_ceil(h_tile) + 1;
                    let part = |o: usize| t_len as u64 * commits_cost[o] + groups as u64 * tiles_cost[o];
                    let mut late = 0u64;
                    for o in 0..outer {
                        late = late.max(outside(o, o)? + part(o));
                        if o + 1 < outer {
                            late = late.max(outside(o, o + 1)? + part(o) + part(o + 1));
                        }
                    }
                    worst.close_bytes = worst.close_bytes.max(price.frame(tile_len as u32) + late);
                    continue;
                }
                // Every tile's whole close, exactly, at `pos` (with the alignment allowance where the
                // position stands for others).
                let whole_at = |pos: u32, allowance: bool| -> Result<u64, String> {
                    let mut worst = 0u64;
                    for (first, len) in tiles_at(pos) {
                        let (mut reads, hist, _) = read!(pos, &PalwTirRangesV1::single(first, first + len), &[], ni16, None, false);
                        reads.merge(&hist);
                        let extra = if allowance { price.h_allowance(&reads) } else { 0 };
                        worst = worst.max(price.close(&reads, len as u32) + extra);
                    }
                    Ok(worst)
                };
                if !dissected {
                    if !has_h {
                        worst.close_bytes = worst.close_bytes.max(whole_at(rep_from(0), true)?);
                        continue;
                    }
                    for p in 0..p_late.min(p_max + 1) {
                        worst.close_bytes = worst.close_bytes.max(whole_at(p, false)?);
                    }
                    if p_late > p_max {
                        continue;
                    }
                    // Late: at most two row-parts — the END of row `o` (with the position's own row)
                    // and the START of row `o + 1`.
                    let pl = rep_from(p_late);
                    let h = h_at(pl);
                    let a = h_axis.expect("has H");
                    let outer: usize = node.out.resolve(h)[..a].iter().product::<usize>().max(1);
                    let t_len = (tile_len.div_ceil(inner) + 1).min(h);
                    let mut ends = Vec::with_capacity(outer);
                    let mut starts = Vec::with_capacity(outer);
                    for o in 0..outer {
                        ends.push(read!(pl, &row(h, o, h - t_len, h), &[], ni16, None, false).0);
                        starts.push(read!(pl, &row(h, o, 0, t_len), &[], ni16, None, false).0);
                    }
                    let mut outside = 0u64;
                    for o in 0..outer {
                        for part in [&ends[o], &starts[o]] {
                            outside = outside.max(price.units(part, false) + price.h_allowance(part));
                        }
                        if o + 1 < outer {
                            let mut pair = ends[o].clone();
                            pair.merge(&starts[o + 1]);
                            outside = outside.max(price.units(&pair, false) + price.h_allowance(&pair));
                        }
                    }
                    // Through the history rows: every `T`-window of every row at every alignment of the
                    // history tiles, in both-mode, every leaf a run of its own.
                    let mut history = 0u64;
                    for o in 0..outer {
                        for r in 0..=h_tile.min(h) {
                            let end = h - r;
                            let start = end.saturating_sub(t_len);
                            if start >= end {
                                continue;
                            }
                            let hist = twin(pl, &row(h, o, start, end), &[], ni16, None, true, true, false)?.hist;
                            history = history.max(price.units(&hist, true));
                        }
                    }
                    worst.close_bytes = worst.close_bytes.max(price.frame(tile_len as u32) + outside + 2 * history);
                    continue;
                }
                // Position 0: the history is one row, the dissection has no rounds, and the whole close
                // is the executor's move — it must be carriable.
                worst.close_bytes = worst.close_bytes.max(whole_at(0, false)?);
                // Every other position: the root claim (one carrier) and the bottom (the cap).
                let mut positions: BTreeSet<u32> = BTreeSet::new();
                if p_max >= 1 {
                    positions.insert(1);
                }
                positions.insert(rep_from(1));
                if has_h {
                    positions.extend(1..p_late.min(p_max + 1));
                    if p_late <= p_max {
                        positions.insert(rep_from(p_late.max(1)));
                    }
                }
                for pos in positions.into_iter().filter(|p| *p >= 1) {
                    let late_h = has_h && pos >= p_late;
                    let parts = if late_h { 2 } else { 1 };
                    let mut root_worst = 0u64;
                    let mut bottom_worst = 0u64;
                    for (first, len) in tiles_at(pos) {
                        let tile = PalwTirRangesV1::single(first, first + len);
                        let supplied: Vec<u16> = reductions.iter().copied().filter(|r| *r != ni16).collect();
                        let (mut finalize, hist, finalize_supplied) = read!(pos, &tile, &supplied, ni16, None, false);
                        finalize.merge(&hist);
                        let mut closure: BTreeMap<u16, PalwTirRangesV1> = finalize_supplied;
                        if reductions.contains(&ni16) {
                            let own = closure.remove(&ni16).unwrap_or_default().union(&tile);
                            closure.insert(ni16, own);
                        }
                        let mut root_reads = finalize.clone();
                        let mut probed: BTreeMap<u16, PalwTirRangesV1> = BTreeMap::new();
                        // The closure's probes at the history's first row, a reduction's pending
                        // elements at once (as the builder asks them).
                        loop {
                            let mut pending: BTreeMap<u16, PalwTirRangesV1> = BTreeMap::new();
                            for (n, es) in &closure {
                                let seen = probed.entry(*n).or_default();
                                let new = es.minus(seen);
                                charge(1 + es.ranges().len() as u64 + seen.ranges().len() as u64)?;
                                if !new.is_empty() {
                                    *seen = seen.union(&new);
                                    pending.insert(*n, new);
                                }
                            }
                            if pending.is_empty() {
                                break;
                            }
                            for (r, es) in pending {
                                let others: Vec<u16> = reductions.iter().copied().filter(|x| *x != r).collect();
                                let (mut probe, hist, probe_supplied) = read!(pos, &es, &others, r, Some((0, 1)), false);
                                probe.merge(&hist);
                                for (n, e2) in probe_supplied {
                                    charge(1 + e2.ranges().len() as u64)?;
                                    let merged = closure.remove(&n).unwrap_or_default().union(&e2);
                                    closure.insert(n, merged);
                                }
                                charge(1 + size_of_reads(&probe))?;
                                root_reads.merge(&probe);
                            }
                        }
                        let values: u64 = closure.values().map(|s| s.elements()).sum();
                        root_reads.supplied.clear();
                        let root_units = price.units(&root_reads, false) + if late_h { slack(&root_reads) } else { 0 };
                        root_worst = root_worst.max(root_units * parts + values * parts * 20);
                        // The bottom: every reduction over the first and the last history tile, in
                        // both-mode — the two together bound any tile at any position.
                        let h = h_at(pos);
                        let tiles_h = h.div_ceil(h_tile);
                        let mut bottom = 0u64;
                        for tau in [0usize, tiles_h.saturating_sub(1)].into_iter().collect::<BTreeSet<_>>() {
                            let from = tau * h_tile;
                            let to = ((tau + 1) * h_tile).min(h);
                            let mut reads = PalwTirCloseReadsV1::default();
                            for r in &reductions {
                                let Some(es) = closure.get(r).filter(|s| !s.is_empty()) else { continue };
                                let others: Vec<u16> = reductions.iter().copied().filter(|x| x != r).collect();
                                let (part, hist, _) = read!(pos, es, &others, *r, Some((from, to)), true);
                                charge(2 + size_of_reads(&part) + size_of_reads(&hist))?;
                                reads.merge(&part);
                                reads.merge(&hist);
                            }
                            reads.supplied.clear();
                            bottom += price.units(&reads, false) + if late_h { slack(&reads) } else { 0 };
                        }
                        bottom_worst = bottom_worst.max(bottom * parts);
                    }
                    worst.root_claim_bytes =
                        worst.root_claim_bytes.max(price.root_frame(tile_len as u32, reductions.len()) + root_worst);
                    worst.close_bytes = worst.close_bytes.max(price.frame(tile_len as u32) + bottom_worst);
                }
            }
            let past = sizing
                .stop_above
                .is_some_and(|(close, root)| worst.close_bytes > close || (worst.dissected && worst.root_claim_bytes > root));
            if let Some(t) = trace.as_deref_mut() {
                t.push((bi8, ni16, sizing.cap - budget.get()));
            }
            out.push(worst);
            if past {
                return Ok((out, sizing.cap - budget.get()));
            }
        }
    }
    // **Checkpoint leaves** (a pipeline stage's) — the element twin's section, each tile a range: a `Fixed` state the stage does
    // not write in `post` has a leaf after every `C`-th position, and a close at it evaluates the state's value after the position.
    if let Some(g) = gen_stage
        && p_max + 1 >= c
    {
        for inst in space.fixed_instances() {
            if g.post_writers.iter().any(|(state, _)| *state == inst.state) {
                continue;
            }
            let Some((occ, writer)) = state_writer_v1(program, inst.state, inst.layer) else { continue };
            let (block, _) = occurrences[occ as usize];
            if !crate::palw_tir_dissect_v1::palw_tir_cone_reductions_v1(&program.blocks[block as usize], writer).is_empty() {
                return Err(format!("the checkpoint leaf of state {} reduces over the history: its close is not sized", inst.state));
            }
            let pos = rep_from(0);
            let (lanes, count) = (inst.tile_lanes as usize, inst.elements as usize);
            let mut bound = PalwTirCloseBoundV1 {
                block,
                node: writer,
                checkpoint: Some(inst.state),
                dissected: false,
                close_bytes: 0,
                root_claim_bytes: 0,
            };
            for first in (0..count).step_by(lanes.max(1)) {
                let last = (first + lanes).min(count);
                charge(16 + (last - first) as u64)?;
                let ranges = PalwTirRangesV1::single(first, last);
                let request = PalwTirCloseRangeRequestV1 {
                    ctx: DemandContext { pos, occurrence: occ },
                    target: writer,
                    ranges: &ranges,
                    supplied: &[],
                    range: None,
                    both: false,
                };
                let split = palw_tir_close_reads_range_split_gen_v1(
                    space,
                    job_ctx,
                    inventory,
                    &mut cache.borrow_mut(),
                    &request,
                    budget.get(),
                    false,
                    false,
                    gen_stage,
                )?;
                budget.set(budget.get().saturating_sub(split.work));
                let mut reads = split.reads;
                reads.merge(&split.hist);
                bound.close_bytes = bound.close_bytes.max(price.close(&reads, (last - first) as u32));
            }
            out.push(bound);
            if sizing.stop_above.is_some_and(|(close, _)| out.last().is_some_and(|b| b.close_bytes > close)) {
                return Ok((out, sizing.cap - budget.get()));
            }
        }
    }
    Ok((out, sizing.cap - budget.get()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn elements_of_box(bx: &Bx, sh: &[usize]) -> Vec<usize> {
        let st = strides(sh);
        let mut v = Vec::new();
        let mut idx = [0usize; MAX_RANK];
        let r = sh.len();
        if (0..r).any(|d| bx[d].0 >= bx[d].1) {
            return v;
        }
        for d in 0..r {
            idx[d] = bx[d].0;
        }
        loop {
            v.push((0..r).map(|d| idx[d] * st[d]).sum());
            let mut d = r;
            loop {
                if d == 0 {
                    return v;
                }
                d -= 1;
                idx[d] += 1;
                if idx[d] < bx[d].1 {
                    break;
                }
                idx[d] = bx[d].0;
            }
        }
    }

    /// **A range is exactly the union of its boxes, and a box exactly the union of its ranges**, on
    /// every range of several shapes (ranks 0 to 4, unit axes included).
    #[test]
    fn boxes_and_ranges_are_exact() {
        for sh in [vec![], vec![7], vec![3, 5], vec![2, 1, 4], vec![3, 4, 2], vec![2, 3, 1, 5], vec![1, 6], vec![4, 1]] {
            let n: usize = sh.iter().product();
            let st = strides(&sh);
            for a in 0..n {
                for b in a + 1..=n {
                    let mut bxs = Vec::new();
                    boxes_of(a, b, &sh, &mut bxs);
                    assert!(bxs.len() <= (2 * sh.len()).max(1), "{sh:?} [{a}, {b}): {} boxes", bxs.len());
                    let mut got: Vec<usize> = bxs.iter().flat_map(|bx| elements_of_box(bx, &sh)).collect();
                    got.sort_unstable();
                    assert_eq!(got, (a..b).collect::<Vec<_>>(), "{sh:?} [{a}, {b})");
                    for bx in &bxs {
                        let mut rs = Vec::new();
                        assert_eq!(box_ranges(bx, &sh, &st, &mut rs), box_range_count(bx, &sh), "{sh:?} {bx:?}: counted before cut");
                        assert_eq!(box_elements(bx, sh.len()), elements_of_box(bx, &sh).len() as u64);
                        let from_ranges: Vec<usize> = rs.iter().flat_map(|(x, y)| *x..*y).collect();
                        assert_eq!(from_ranges, elements_of_box(bx, &sh), "{sh:?} {bx:?}");
                        assert_eq!(PalwTirRangesV1::from_ranges(rs.clone()).ranges(), rs.as_slice(), "the ranges are normalized");
                    }
                }
            }
        }
    }

    #[test]
    fn ranges_union_and_difference() {
        let a = PalwTirRangesV1::from_ranges(vec![(5, 9), (0, 2), (1, 3), (9, 10), (20, 25)]);
        assert_eq!(a.ranges(), &[(0, 3), (5, 10), (20, 25)]);
        let b = PalwTirRangesV1::from_ranges(vec![(2, 6), (8, 21), (30, 31)]);
        assert_eq!(a.union(&b).ranges(), &[(0, 25), (30, 31)]);
        assert_eq!(a.minus(&b).ranges(), &[(0, 2), (6, 8), (21, 25)]);
        assert_eq!(b.minus(&a).ranges(), &[(3, 5), (10, 20), (30, 31)]);
        for x in 0..40 {
            let ab: Vec<usize> = a.minus(&b).iter_elements().collect();
            assert_eq!(ab.contains(&x), a.iter_elements().any(|e| e == x) && !b.iter_elements().any(|e| e == x));
        }
        assert_eq!(a.elements(), 3 + 5 + 5);
        assert_eq!(PalwTirRangesV1::from_elements(&[4, 2, 3, 9]).ranges(), &[(2, 5), (9, 10)]);
    }
}
