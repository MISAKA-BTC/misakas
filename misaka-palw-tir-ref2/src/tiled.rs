//! **RFC-0013 §5 — row/range readers and tiled primitives, for a tensor that does not fit in memory.**
//!
//! The evaluator of [`crate::eval`] holds every param whole, as 128-bit integers: a 1.9 GB artifact is 16× that in `i128`, and a model whose
//! single tensor exceeds a worker's memory cannot be checked at all. This module is the same evaluation with the two primitives that read
//! the big tensors — `MatMul` (the weights) and `Gather` (an embedding table) — consuming a param **in row tiles**, never whole:
//!
//! ```text
//!   RowSource  ── rows [r0, r1) of axis 0, contiguous ──▶  a tile (≤ max(tile_elems, one row) elements, decoded)
//!       │                                                        │
//!   container / authenticated reader / a map                     ▼
//!                                         MatMul: every element of the tile adds its product to the output element(s) it feeds
//!                                         Gather: only the row an index names is read
//! ```
//!
//! # Why the result is bit-identical (and what is *not* claimed)
//!
//! * **`MatMul` sums are order-free** (04b §6.3, PALW-TIR-24): the value is `P + N`, the exact sum of the positive terms plus the exact sum of
//!   the negative terms, and the node fails unless `P ≤ max` and `N ≥ min` of the output dtype — a property of the two totals, not of the order
//!   the terms arrive in. A tile therefore adds its terms to the same per-output accumulators ([`OrderFree`]) the whole-tensor path does, in
//!   whatever order the tiles come, and the accumulators are finished in the output's linear order, so the first failing output is the same
//!   output. Every product is the exact 256-bit [`Wide::mul_i128`]; nothing is rounded, narrowed or reordered by the tiling.
//! * **`Gather` is a copy**: the index tensor is read first, each index checked against the axis extent in output order (the same `Index`
//!   refusal at the same element), and the row it names is read and copied.
//! * **The shape rules are the whole path's, on the declared shapes**: the same `Shape` refusals, from the declaration and the activation's
//!   shape, before any element is read.
//! * **Not claimed**: that the error *class* of a program with SEVERAL simultaneous faults is the one the whole-tensor path reports first
//!   (a malformed element is found when its tile is read, not when the tensor is loaded; §9.3 lets an input that breaks several rules report
//!   any one of them, and the normative property — success versus failure, and every value — is exact). A fault on its own reports the same
//!   class and reason.
//!
//! # The tiling bound
//!
//! A param is read in rows of `rest = Π shape[1..]` elements; a tile is `⌊tile_elems / rest⌋` rows, at least one. So **no tile ever holds more
//! than `max(tile_elems, rest)` elements** ([`tile_elems_bound`]), and a tile is released before the next is read. Beside it the evaluation
//! holds the activation operands (already in memory in the whole path too) and one accumulator per output element (80 bytes each; a decode
//! step's outputs are a row). Params of at most one tile are loaded whole (a norm gain: cheaper and identical), and a param larger than a
//! tile that a primitive with no tiled form reads is loaded whole too — counted in the meter ([`TileReport::whole_peak_elems`]), or refused
//! under [`TiledParams::strict`], which turns the bound into a guarantee instead of a measurement.
//!
//! `eval_cone` (the court's cone evaluation) is not tiled: its pre-check reads each param whole by design. This module serves `step` and
//! `run`, the conformance path.

use std::cell::Cell;

use crate::error::{Class, Res, err};
use crate::eval::{ParamSource, Params};
use crate::prims::{OrderFree, broadcast_index, broadcasts_to, shape_err};
use crate::program::Prim;
use crate::tensor::{Tensor, count, ravel, unravel};
use crate::types::DType;
use crate::wide::Wide;

/// **A param read by contiguous row ranges along axis 0** — the reader a tiled evaluation stands on. An implementation over a container
/// decodes the bytes of the rows it is asked for (and, over an authenticated reader, checks them against the artifact root); one over a map
/// slices. It never needs the whole tensor.
pub trait RowSource {
    /// The declared dtype and shape of `(index, layer)`, or `None` when the source holds no such instance.
    fn shape(&self, index: u16, layer: Option<u32>) -> Res<Option<(DType, Vec<u64>)>>;
    /// Rows `[row_start, row_start + rows)` of axis 0, as a tensor of shape `[rows, rest…]`. A rank-0 param has one "row": `(0, 1)` returns
    /// the scalar (shape `[]`). A range outside the tensor, a corrupt read or an I/O failure is an `Err` — never a silent zero.
    fn rows(&self, index: u16, layer: Option<u32>, row_start: u64, rows: u64) -> Res<Tensor>;
}

/// **A [`RowSource`] over the in-memory param map** — slices, validating nothing (a malformed entry reaches the tile check, as it reaches
/// `check_tensor` in the whole path). What the equivalence tests and any caller that already holds `Params` use.
pub struct MapRowSource<'a>(pub &'a Params);

impl RowSource for MapRowSource<'_> {
    fn shape(&self, index: u16, layer: Option<u32>) -> Res<Option<(DType, Vec<u64>)>> {
        Ok(self.0.get(&(index, layer)).map(|t| (t.dtype, t.shape.clone())))
    }

    fn rows(&self, index: u16, layer: Option<u32>, row_start: u64, rows: u64) -> Res<Tensor> {
        let Some(t) = self.0.get(&(index, layer)) else { return err(Class::Missing, "the source holds no such param") };
        if count(&t.shape) != t.data.len() as u64 {
            return err(Class::Operand, "param: malformed tensor");
        }
        if t.shape.is_empty() {
            return if (row_start, rows) == (0, 1) { Ok(t.clone()) } else { err(Class::Operand, "a scalar param has one row") };
        }
        let rest = count(&t.shape[1..]);
        let end = row_start.checked_add(rows).filter(|e| *e <= t.shape[0]);
        let Some(end) = end else { return err(Class::Operand, "a row range outside the param") };
        let mut shape = vec![rows];
        shape.extend_from_slice(&t.shape[1..]);
        Ok(Tensor { dtype: t.dtype, shape, data: t.data[(row_start * rest) as usize..(end * rest) as usize].to_vec() })
    }
}

/// What a tiled evaluation measured. A report, not a limit: the limit is [`tile_elems_bound`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TileReport {
    /// Row tiles read (a `Gather` row counts as one).
    pub tiles: u64,
    /// Param elements read through tiles, in total.
    pub elems_read: u64,
    /// The most param elements a tile held at once.
    pub peak_tile_elems: u64,
    /// Multiply-accumulates made by tiled `MatMul`s (each product pushed into an accumulator once).
    pub macs: u128,
    /// Params loaded whole (a param of at most one tile, or one read by a primitive with no tiled form), and the largest of them.
    pub whole_loads: u64,
    pub whole_peak_elems: u64,
}

impl TileReport {
    /// The most param elements the evaluation held at one moment: the larger of a tile and a whole load.
    pub fn peak_param_elems(&self) -> u64 {
        self.peak_tile_elems.max(self.whole_peak_elems)
    }
}

/// **A param source that tiles.** `tile_elems` is the most decoded elements one tile may hold (a row is the floor).
pub struct TiledParams<'a> {
    source: &'a dyn RowSource,
    tile_elems: u64,
    strict: bool,
    report: Cell<TileReport>,
}

impl<'a> TiledParams<'a> {
    pub fn new(source: &'a dyn RowSource, tile_elems: u64) -> Self {
        Self { source, tile_elems: tile_elems.max(1), strict: false, report: Cell::new(TileReport::default()) }
    }

    /// **Refuse, instead of loading whole, a param larger than a tile that no tiled primitive reads.** Turns the residency bound from a
    /// measurement into a guarantee: the evaluation either stays within it or fails with a named refusal.
    pub fn strict(mut self) -> Self {
        self.strict = true;
        self
    }

    pub fn tile_elems(&self) -> u64 {
        self.tile_elems
    }

    pub fn report(&self) -> TileReport {
        self.report.get()
    }

    fn update(&self, f: impl FnOnce(&mut TileReport)) {
        let mut r = self.report.get();
        f(&mut r);
        self.report.set(r);
    }
}

impl ParamSource for TiledParams<'_> {
    /// The whole-tensor fallback (a small param, or a primitive with no tiled form).
    fn tensor(&self, index: u16, layer: Option<u32>) -> Res<Option<Tensor>> {
        let Some((_, shape)) = self.source.shape(index, layer)? else { return Ok(None) };
        let n = count(&shape);
        if self.strict && n > self.tile_elems {
            return err(
                Class::Operand,
                format!(
                    "param {index} holds {n} elements, above the {} a tile holds, and the primitive that reads it has no tiled form (strict tiling)",
                    self.tile_elems
                ),
            );
        }
        let rows = shape.first().copied().unwrap_or(1);
        let t = self.source.rows(index, layer, 0, rows)?;
        self.update(|r| {
            r.whole_loads += 1;
            r.whole_peak_elems = r.whole_peak_elems.max(n);
        });
        Ok(Some(t))
    }

    fn tiles(&self) -> Option<&dyn TileAccess> {
        Some(self)
    }
}

/// What the evaluator needs of a tiling source (object-safe: [`ParamSource::tiles`] returns it).
pub trait TileAccess {
    fn tile_elems(&self) -> u64;
    /// The declared dtype and shape of an instance, if the source holds it.
    fn decl(&self, index: u16, layer: Option<u32>) -> Res<Option<(DType, Vec<u64>)>>;
    /// One tile, checked (dtype, shape and every value) and metered.
    fn read_rows(&self, index: u16, layer: Option<u32>, row_start: u64, rows: u64, shape: &[u64], dtype: DType) -> Res<Tensor>;
    fn add_macs(&self, macs: u128);
}

impl TileAccess for TiledParams<'_> {
    fn tile_elems(&self) -> u64 {
        self.tile_elems
    }

    fn decl(&self, index: u16, layer: Option<u32>) -> Res<Option<(DType, Vec<u64>)>> {
        self.source.shape(index, layer)
    }

    fn read_rows(&self, index: u16, layer: Option<u32>, row_start: u64, rows: u64, shape: &[u64], dtype: DType) -> Res<Tensor> {
        let t = self.source.rows(index, layer, row_start, rows)?;
        let mut want = vec![rows];
        want.extend_from_slice(shape.get(1..).unwrap_or(&[]));
        let want = if shape.is_empty() { vec![] } else { want };
        if t.dtype != dtype || t.shape != want {
            return err(Class::Operand, "param: dtype or shape is not the declared one");
        }
        if count(&t.shape) != t.data.len() as u64 || t.data.iter().any(|&v| !t.dtype.contains(v)) {
            return err(Class::Operand, "param: malformed tensor");
        }
        let n = t.data.len() as u64;
        self.update(|r| {
            r.tiles += 1;
            r.elems_read += n;
            r.peak_tile_elems = r.peak_tile_elems.max(n);
        });
        Ok(t)
    }

    fn add_macs(&self, macs: u128) {
        self.update(|r| r.macs += macs);
    }
}

/// **The tiling bound**: no tile of a param whose rows are `rest_elems` long holds more than this many elements.
pub fn tile_elems_bound(rest_elems: u64, tile_elems: u64) -> u64 {
    tile_rows(rest_elems, tile_elems) * rest_elems.max(1)
}

/// Rows per tile: `⌊tile_elems / rest⌋`, at least one.
pub fn tile_rows(rest_elems: u64, tile_elems: u64) -> u64 {
    (tile_elems / rest_elems.max(1)).max(1)
}

/// A param the node reads in tiles: where to read it, and what it was declared as.
///
/// With `transposed` the node reads the param **through a `Transpose` `[1, 0]` of a rank-2 param** (RFC-0013 §5): the lowerer declares a linear
/// layer's weights `[out, in]` and feeds the `MatMul` their transpose, so the weights of nearly every lowered model reach their `MatMul` this
/// way. `shape` is always the param's own (`[out, in]`: the rows the container stores and the tiles are read in); the operand the `MatMul` sees is
/// [`Self::logical_shape`]. The transposed tensor is never built.
pub struct LazyParam<'a> {
    pub tiles: &'a dyn TileAccess,
    pub index: u16,
    pub layer: Option<u32>,
    pub dtype: DType,
    pub shape: Vec<u64>,
    pub transposed: bool,
}

impl LazyParam<'_> {
    /// The shape of the operand the consuming primitive sees: the param's, or — through the fused transpose — its reverse.
    pub fn logical_shape(&self) -> Vec<u64> {
        if self.transposed { self.shape.iter().rev().copied().collect() } else { self.shape.clone() }
    }
}

/// The nodes of a block that [`fusable_transposes`] may leave unbuilt: `Transpose` `[1, 0]` of a rank-2 param, never committed, neither a carry-out
/// nor the logits node, and read by exactly one node, a `MatMul`. Such a node's value is observable only through that `MatMul`, which reads the
/// param in tiles. (Whether a given node is left unbuilt also depends on the param being larger than a tile, decided where it is evaluated.)
pub fn fusable_transposes(p: &crate::program::Program, b: usize) -> std::collections::BTreeSet<usize> {
    use crate::program::Ref;
    let block = &p.blocks[b];
    let mut out = std::collections::BTreeSet::new();
    for (t, n) in block.nodes.iter().enumerate() {
        let is_param_transpose = matches!(&n.prim, Prim::Transpose { perm } if perm.as_slice() == [1, 0])
            && matches!(n.inputs.as_slice(), [Ref::Param(j)] if p.params.get(*j as usize).is_some_and(|d| d.shape.len() == 2));
        if !is_param_transpose || n.commit || t == p.logits as usize || block.carry_out.contains(&(t as u16)) {
            continue;
        }
        let me = Ref::Node(t as u16);
        let reads: usize = block.nodes.iter().map(|m| m.inputs.iter().filter(|r| **r == me).count()).sum();
        let reader = block.nodes.iter().find(|m| m.inputs.contains(&me));
        if reads == 1 && reader.is_some_and(|m| matches!(m.prim, Prim::MatMul)) {
            out.insert(t);
        }
    }
    out
}

/// Does `prim` have a tiled form for a param of rank `rank` at input position `pos`? `MatMul` either operand (rank ≥ 2); `Gather`'s data
/// operand along axis 0 with no batch dims (the embedding pattern), rank ≥ 1.
pub fn tiled_form_exists(prim: &Prim, pos: usize, rank: usize) -> bool {
    match prim {
        Prim::MatMul => pos <= 1 && rank >= 2,
        Prim::Gather { axis: 0, batch_dims: 0 } => pos == 0 && rank >= 1,
        _ => false,
    }
}

/// **Evaluate a primitive whose input `pos` is a [`LazyParam`]** — the tiled form of `eval_prim`, with the same refusals in the same
/// places, over the declared shape in place of the elements.
pub fn eval_prim_tiled(
    prim: &Prim,
    ins: &[&Tensor],
    pos: usize,
    lazy: &LazyParam<'_>,
    out_dtype: DType,
    out_shape: &[u64],
) -> Res<Tensor> {
    let (amin, amax) = prim.arity();
    if ins.len() < amin || ins.len() > amax {
        return shape_err(prim, "arity");
    }
    let n_out = count(out_shape);
    if n_out > (1 << 28) {
        return shape_err(prim, "output above 2^28 elements");
    }
    match prim {
        Prim::MatMul => matmul_tiled(prim, ins, pos, lazy, out_dtype, out_shape, n_out as usize),
        Prim::Gather { axis: 0, batch_dims: 0 } if pos == 0 && !lazy.transposed => {
            gather_tiled(prim, ins, lazy, out_dtype, out_shape, n_out as usize)
        }
        _ => err(Class::Shape, format!("{} has no tiled form for a param at input {pos}", prim.name())),
    }
}

/// The rows of `lazy`, tile by tile, handed to `each(row_start, tile)`.
fn for_each_tile(lazy: &LazyParam<'_>, mut each: impl FnMut(u64, &Tensor)) -> Res<()> {
    let rest = if lazy.shape.len() <= 1 { 1 } else { count(&lazy.shape[1..]) };
    let n0 = lazy.shape.first().copied().unwrap_or(1);
    let per = tile_rows(rest, lazy.tiles.tile_elems());
    let mut r0 = 0u64;
    while r0 < n0 {
        let rows = per.min(n0 - r0);
        let tile = lazy.tiles.read_rows(lazy.index, lazy.layer, r0, rows, &lazy.shape, lazy.dtype)?;
        each(r0, &tile);
        r0 += rows;
    }
    Ok(())
}

fn matmul_tiled(
    prim: &Prim,
    ins: &[&Tensor],
    pos: usize,
    lazy: &LazyParam<'_>,
    out_dtype: DType,
    out_shape: &[u64],
    n_out: usize,
) -> Res<Tensor> {
    if pos > 1 {
        return shape_err(prim, "arity");
    }
    let out_rank = out_shape.len();
    // The operand shapes: the lazy one is the declaration, the other the activation in memory (the other input is `ins[1 - pos]`).
    let other = ins[1 - pos];
    let lshape = lazy.logical_shape();
    let (ash, bsh): (&[u64], &[u64]) = if pos == 0 { (&lshape, &other.shape) } else { (&other.shape, &lshape) };
    let (ra, rb) = (ash.len(), bsh.len());
    if ra < 2 || rb < 2 || out_rank < 2 || ash[ra - 1] != bsh[rb - 2] {
        return shape_err(prim, "contraction");
    }
    let (m, kk, nn) = (ash[ra - 2], ash[ra - 1], bsh[rb - 1]);
    if out_shape[out_rank - 2] != m || out_shape[out_rank - 1] != nn {
        return shape_err(prim, "M, N");
    }
    let batch = &out_shape[..out_rank - 2];
    let (a_batch, b_batch) = (&ash[..ra - 2], &bsh[..rb - 2]);
    if !broadcasts_to(a_batch, batch) || !broadcasts_to(b_batch, batch) {
        return shape_err(prim, "batch");
    }
    // Each output batch position `beta`, and the batch position of each operand it reads (§2.3 broadcasting).
    let nb = count(batch) as usize;
    let (mut a_lin, mut b_lin) = (Vec::with_capacity(nb), Vec::with_capacity(nb));
    let (mut ba, mut bb) = (Vec::new(), Vec::new());
    let mut o = vec![0u64; batch.len()];
    for beta in 0..nb as u64 {
        unravel(beta, batch, &mut o);
        broadcast_index(&o, a_batch, &mut ba);
        broadcast_index(&o, b_batch, &mut bb);
        a_lin.push(ravel(&ba, a_batch));
        b_lin.push(ravel(&bb, b_batch));
    }
    // The lazy operand's batch position -> the output batch positions that read it.
    let lazy_lin = if pos == 0 { &a_lin } else { &b_lin };
    let lazy_batches = count(if pos == 0 { a_batch } else { b_batch }) as usize;
    let mut readers: Vec<Vec<u32>> = vec![Vec::new(); lazy_batches];
    for (beta, lin) in lazy_lin.iter().enumerate() {
        readers[*lin as usize].push(beta as u32);
    }
    let (m_us, kk_us, nn_us) = (m as usize, kk as usize, nn as usize);

    let mut acc: Vec<OrderFree> = (0..n_out).map(|_| OrderFree::new()).collect();
    let mut macs: u128 = 0;
    // Elements per physical row of the param (the rows the tiles are made of).
    let phys_rest = if lazy.shape.len() <= 1 { 1 } else { count(&lazy.shape[1..]) as usize };
    if pos == 1 {
        // B is the param: element g = bb·(K·N) + t·N + c contributes x[beta, r, t] · y to out[beta, r, c] for every reading beta and r.
        for_each_tile(lazy, |r0, tile| {
            let rest = phys_rest;
            let base = r0 as usize * rest;
            for (e, &y) in tile.data.iter().enumerate() {
                let g = base + e;
                // Through the fused transpose the param element W[pr, pc] is B[t = pc, c = pr] (a rank-2 param: no batch).
                let (c, t, bb) =
                    if lazy.transposed { (g / rest, g % rest, 0) } else { (g % nn_us, (g / nn_us) % kk_us, g / (kk_us * nn_us)) };
                for &beta in &readers[bb] {
                    let beta = beta as usize;
                    let a_row = a_lin[beta] as usize * (m_us * kk_us);
                    for r in 0..m_us {
                        let x = other.data[a_row + r * kk_us + t];
                        acc[beta * (m_us * nn_us) + r * nn_us + c].push(Wide::mul_i128(x, y));
                    }
                    macs += m_us as u128;
                }
            }
        })?;
    } else {
        // A is the param: element g = ba·(M·K) + r·K + t contributes x · y[beta, t, c] to out[beta, r, c] for every reading beta and c.
        for_each_tile(lazy, |r0, tile| {
            let rest = phys_rest;
            let base = r0 as usize * rest;
            for (e, &x) in tile.data.iter().enumerate() {
                let g = base + e;
                // Through the fused transpose the param element W[pr, pc] is A[r = pc, t = pr].
                let (t, r, ba) =
                    if lazy.transposed { (g / rest, g % rest, 0) } else { (g % kk_us, (g / kk_us) % m_us, g / (m_us * kk_us)) };
                for &beta in &readers[ba] {
                    let beta = beta as usize;
                    let b_row = b_lin[beta] as usize * (kk_us * nn_us);
                    for c in 0..nn_us {
                        let y = other.data[b_row + t * nn_us + c];
                        acc[beta * (m_us * nn_us) + r * nn_us + c].push(Wide::mul_i128(x, y));
                    }
                    macs += nn_us as u128;
                }
            }
        })?;
    }
    lazy.tiles.add_macs(macs);
    // Finished in the output's linear order: the first failing output is the whole path's first failing output.
    let mut data = Vec::with_capacity(n_out);
    for a in acc {
        data.push(a.finish(out_dtype)?);
    }
    Ok(Tensor { dtype: out_dtype, shape: out_shape.to_vec(), data })
}

fn gather_tiled(prim: &Prim, ins: &[&Tensor], lazy: &LazyParam<'_>, out_dtype: DType, out_shape: &[u64], n_out: usize) -> Res<Tensor> {
    let idx_t = ins[1];
    let rd = lazy.shape.len();
    if rd == 0 {
        return shape_err(prim, "axes");
    }
    // out shape = idx.shape ++ data.shape[1..] (axis 0, no batch dims).
    let mut want = idx_t.shape.clone();
    want.extend_from_slice(&lazy.shape[1..]);
    if want != out_shape {
        return shape_err(prim, "shape");
    }
    let extent = lazy.shape[0] as i128;
    let mut out = Vec::with_capacity(n_out);
    for &v in &idx_t.data {
        if !(0 <= v && v < extent) {
            return err(Class::Index, format!("Gather index {v} outside [0, {extent})"));
        }
        let row = lazy.tiles.read_rows(lazy.index, lazy.layer, v as u64, 1, &lazy.shape, lazy.dtype)?;
        out.extend_from_slice(&row.data);
    }
    Ok(Tensor { dtype: out_dtype, shape: out_shape.to_vec(), data: out })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prims::eval_prim;
    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaCha8Rng;

    fn rand_tensor(rng: &mut ChaCha8Rng, dtype: DType, shape: &[u64], lo: i128, hi: i128) -> Tensor {
        let n = count(shape) as usize;
        let data = (0..n).map(|_| rng.gen_range(lo.max(dtype.min())..=hi.min(dtype.max()))).collect();
        Tensor { dtype, shape: shape.to_vec(), data }
    }

    /// Evaluate `prim` with input `pos` read in tiles, from `param`.
    fn tiled(
        prim: &Prim,
        other: &Tensor,
        param: &Tensor,
        pos: usize,
        out_dtype: DType,
        out_shape: &[u64],
        tile: u64,
    ) -> (Res<Tensor>, TileReport) {
        let mut map = Params::new();
        map.insert((0, None), param.clone());
        let source = MapRowSource(&map);
        let tp = TiledParams::new(&source, tile);
        let lazy = LazyParam { tiles: &tp, index: 0, layer: None, dtype: param.dtype, shape: param.shape.clone(), transposed: false };
        let placeholder = Tensor::zeros(DType::I8, vec![]);
        let ins: Vec<&Tensor> = if pos == 0 { vec![&placeholder, other] } else { vec![other, &placeholder] };
        let r = eval_prim_tiled(prim, &ins, pos, &lazy, out_dtype, out_shape);
        (r, tp.report())
    }

    fn whole(prim: &Prim, other: &Tensor, param: &Tensor, pos: usize, out_dtype: DType, out_shape: &[u64]) -> Res<Tensor> {
        let ins: Vec<&Tensor> = if pos == 0 { vec![param, other] } else { vec![other, param] };
        eval_prim(prim, &ins, out_dtype, out_shape, &[], None)
    }

    /// **MatMul, the param either operand, with batch broadcasting, every output dtype (so overflow cases occur), every tile size from one
    /// element up: the result — values AND refusals — is the whole-tensor evaluator's.**
    #[test]
    fn a_tiled_matmul_equals_the_whole_matmul_values_and_refusals() {
        let mut rng = ChaCha8Rng::seed_from_u64(0x0013_5001);
        let mut ok = 0;
        let mut failed = 0;
        for case in 0..600 {
            let (m, kk, nn) = (rng.gen_range(1..=4u64), rng.gen_range(1..=5u64), rng.gen_range(1..=4u64));
            // Batch shapes: none, [b], [b, c], with operand dims broadcast (1) or full.
            let batch: Vec<u64> = match rng.gen_range(0..3) {
                0 => vec![],
                1 => vec![rng.gen_range(1..=3)],
                _ => vec![rng.gen_range(1..=2), rng.gen_range(1..=3)],
            };
            let operand_batch = |rng: &mut ChaCha8Rng| -> Vec<u64> {
                // A suffix of the batch (trailing alignment), each dim kept or collapsed to 1.
                let keep = rng.gen_range(0..=batch.len());
                batch[batch.len() - keep..].iter().map(|d| if rng.gen_bool(0.5) { *d } else { 1 }).collect()
            };
            let (ab, bb) = (operand_batch(&mut rng), operand_batch(&mut rng));
            let a_dtype = *[DType::I8, DType::I16, DType::I32, DType::I64].get(rng.gen_range(0..4)).unwrap();
            let b_dtype = *[DType::I8, DType::I16, DType::I32, DType::I64].get(rng.gen_range(0..4)).unwrap();
            let out_dtype = *[DType::I16, DType::I32, DType::I64, DType::I128].get(rng.gen_range(0..4)).unwrap();
            let (ash, bsh): (Vec<u64>, Vec<u64>) = ([ab.clone(), vec![m, kk]].concat(), [bb.clone(), vec![kk, nn]].concat());
            let out_shape: Vec<u64> = [batch.clone(), vec![m, nn]].concat();
            // Wide values half the time, so some sums leave the output dtype.
            let (a, b) = if rng.gen_bool(0.5) {
                (rand_tensor(&mut rng, a_dtype, &ash, -9, 9), rand_tensor(&mut rng, b_dtype, &bsh, -9, 9))
            } else {
                (
                    rand_tensor(&mut rng, a_dtype, &ash, i128::MIN, i128::MAX),
                    rand_tensor(&mut rng, b_dtype, &bsh, i128::MIN, i128::MAX),
                )
            };
            for pos in [0usize, 1] {
                let (param, other) = if pos == 0 { (&a, &b) } else { (&b, &a) };
                let expected = whole(&Prim::MatMul, other, param, pos, out_dtype, &out_shape);
                for tile in [1u64, 2, 3, 7, 1_000] {
                    let (got, report) = tiled(&Prim::MatMul, other, param, pos, out_dtype, &out_shape, tile);
                    assert_eq!(
                        got, expected,
                        "case {case} pos {pos} tile {tile}: a={ash:?} b={bsh:?} out={out_shape:?} {out_dtype:?}"
                    );
                    if expected.is_ok() {
                        // The sizing: every param element read exactly once, in tiles within the bound; every product made once.
                        let rest = if param.shape.len() <= 1 { 1 } else { count(&param.shape[1..]) };
                        assert_eq!(report.elems_read, count(&param.shape), "each element of the param is read exactly once");
                        assert!(report.peak_tile_elems <= tile_elems_bound(rest, tile), "case {case}: tile {tile}: {report:?}");
                        assert_eq!(report.macs, count(&out_shape) as u128 * kk as u128, "n_out · K products, as the whole path makes");
                    }
                }
                if expected.is_ok() { ok += 1 } else { failed += 1 }
            }
        }
        assert!(ok > 100 && failed > 20, "both the values and the refusals were exercised: {ok} ok, {failed} refused");
    }

    /// **A `MatMul` that reads a rank-2 param through a fused `Transpose [1, 0]`** (the lowerer's linear layer: weights `[out, in]`): the param either
    /// operand, the other operand batched or not, every output dtype, every tile size from one element up — the result, values and refusals, is the
    /// whole `MatMul` of the explicitly transposed tensor, the transposed tensor is never built, and each param element is read exactly once.
    #[test]
    fn a_matmul_through_a_fused_transpose_equals_the_matmul_of_the_transposed_tensor() {
        let mut rng = ChaCha8Rng::seed_from_u64(0x0013_5003);
        let (mut ok, mut failed) = (0, 0);
        for case in 0..400 {
            let (p0, p1) = (rng.gen_range(1..=6u64), rng.gen_range(1..=6u64));
            let (m, batch) = (rng.gen_range(1..=4u64), if rng.gen_bool(0.4) { vec![rng.gen_range(1..=3u64)] } else { vec![] });
            let w_dtype = *[DType::I8, DType::I16, DType::I32, DType::I64].get(rng.gen_range(0..4)).unwrap();
            let x_dtype = *[DType::I8, DType::I16, DType::I32, DType::I64].get(rng.gen_range(0..4)).unwrap();
            let out_dtype = *[DType::I16, DType::I32, DType::I64, DType::I128].get(rng.gen_range(0..4)).unwrap();
            let wide = rng.gen_bool(0.3);
            let (lo, hi) = if wide { (i128::MIN, i128::MAX) } else { (-9, 9) };
            let w = rand_tensor(&mut rng, w_dtype, &[p0, p1], lo, hi);
            let wt = eval_prim(&Prim::Transpose { perm: vec![1, 0] }, &[&w], w_dtype, &[p1, p0], &[], None).expect("transpose");
            for pos in [0usize, 1] {
                // pos 1: B = Wᵀ is [p1, p0], x is [.., m, p1]; pos 0: A = Wᵀ is [p1, p0], y is [.., p0, m].
                let other_shape: Vec<u64> =
                    if pos == 1 { [batch.clone(), vec![m, p1]].concat() } else { [batch.clone(), vec![p0, m]].concat() };
                let other = rand_tensor(&mut rng, x_dtype, &other_shape, lo, hi);
                let out_shape: Vec<u64> =
                    if pos == 1 { [batch.clone(), vec![m, p0]].concat() } else { [batch.clone(), vec![p1, m]].concat() };
                let expected = whole(&Prim::MatMul, &other, &wt, pos, out_dtype, &out_shape);
                for tile in [1u64, 2, 3, 5, 1_000] {
                    let mut map = Params::new();
                    map.insert((0, None), w.clone());
                    let source = MapRowSource(&map);
                    let tp = TiledParams::new(&source, tile);
                    let lazy = LazyParam { tiles: &tp, index: 0, layer: None, dtype: w_dtype, shape: vec![p0, p1], transposed: true };
                    assert_eq!(lazy.logical_shape(), vec![p1, p0]);
                    let placeholder = Tensor::zeros(DType::I8, vec![]);
                    let ins: Vec<&Tensor> = if pos == 0 { vec![&placeholder, &other] } else { vec![&other, &placeholder] };
                    let got = eval_prim_tiled(&Prim::MatMul, &ins, pos, &lazy, out_dtype, &out_shape);
                    assert_eq!(got, expected, "case {case} pos {pos} tile {tile}: W={p0}x{p1} other={other_shape:?} {out_dtype:?}");
                    if expected.is_ok() {
                        let r = tp.report();
                        assert_eq!(r.elems_read, p0 * p1, "each element of the param is read exactly once");
                        assert!(r.peak_tile_elems <= tile_elems_bound(p1, tile), "tile {tile}: {r:?}");
                        assert_eq!(r.whole_loads, 0, "the transposed tensor was never built");
                    }
                }
                if expected.is_ok() { ok += 1 } else { failed += 1 }
            }
        }
        assert!(ok > 100 && failed > 20, "values and refusals were both exercised: {ok} ok, {failed} refused");
    }

    #[test]
    fn a_tiled_gather_equals_the_whole_gather_including_an_out_of_range_index() {
        let mut rng = ChaCha8Rng::seed_from_u64(0x0013_5002);
        let mut refused = 0;
        for case in 0..300 {
            let (v, d) = (rng.gen_range(1..=6u64), rng.gen_range(1..=4u64));
            let rank3 = rng.gen_bool(0.3);
            let dshape = if rank3 { vec![v, 2, d] } else { vec![v, d] };
            let table = rand_tensor(&mut rng, DType::I16, &dshape, i128::MIN, i128::MAX);
            let ishape: Vec<u64> = match rng.gen_range(0..3) {
                0 => vec![],
                1 => vec![rng.gen_range(1..=4)],
                _ => vec![rng.gen_range(1..=2), rng.gen_range(1..=3)],
            };
            // Indices in range most of the time, one out of range now and then.
            let hi = if rng.gen_bool(0.2) { v as i128 + 1 } else { v as i128 - 1 };
            let idx = rand_tensor(&mut rng, DType::Idx, &ishape, -1, hi);
            let out_shape: Vec<u64> = [ishape.clone(), dshape[1..].to_vec()].concat();
            let prim = Prim::Gather { axis: 0, batch_dims: 0 };
            let expected = eval_prim(&prim, &[&table, &idx], DType::I16, &out_shape, &[], None);
            if expected.is_err() {
                refused += 1;
            }
            for tile in [1u64, 2, 5, 1_000] {
                let mut map = Params::new();
                map.insert((3, Some(1)), table.clone());
                let source = MapRowSource(&map);
                let tp = TiledParams::new(&source, tile);
                let lazy =
                    LazyParam { tiles: &tp, index: 3, layer: Some(1), dtype: DType::I16, shape: dshape.clone(), transposed: false };
                let placeholder = Tensor::zeros(DType::I8, vec![]);
                let got = eval_prim_tiled(&prim, &[&placeholder, &idx], 0, &lazy, DType::I16, &out_shape);
                assert_eq!(got, expected, "case {case} tile {tile}");
                if expected.is_ok() {
                    let rest = count(&dshape[1..]);
                    let r = tp.report();
                    assert_eq!(r.elems_read, idx.data.len() as u64 * rest, "only the rows the indices name are read");
                    assert!(r.peak_tile_elems <= rest.max(1), "a gather holds one row");
                }
            }
        }
        assert!(refused > 5, "the Index refusal was exercised ({refused})");
    }

    #[test]
    fn the_tiling_bound_is_a_tile_or_one_row_whichever_is_larger() {
        assert_eq!(tile_rows(10, 25), 2);
        assert_eq!(tile_rows(10, 5), 1, "a row is the floor");
        assert_eq!(tile_rows(0, 5), 5, "a zero rest cannot divide by zero");
        assert_eq!(tile_elems_bound(10, 25), 20);
        assert_eq!(tile_elems_bound(10, 5), 10);
        for (rest, tile) in [(1u64, 1u64), (3, 2), (7, 100), (100, 7), (64, 64), (65, 64)] {
            assert!(tile_elems_bound(rest, tile) <= tile.max(rest), "rest {rest} tile {tile}");
            assert!(tile_elems_bound(rest, tile) >= rest, "one whole row always fits");
        }
        // The bound is met with equality when rows divide the tile.
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let w = rand_tensor(&mut rng, DType::I8, &[20, 5], -3, 3);
        let x = rand_tensor(&mut rng, DType::I8, &[1, 20], -3, 3);
        let (r, report) = tiled(&Prim::MatMul, &x, &w, 1, DType::I64, &[1, 5], 15);
        r.expect("runs");
        assert_eq!(report.tiles, 7, "20 rows at 3 rows a tile");
        assert_eq!(report.peak_tile_elems, 15);
        assert_eq!(report.elems_read, 100);
    }

    #[test]
    fn a_missing_wrong_or_malformed_param_and_a_failing_source_are_refused_not_ignored() {
        struct Broken;
        impl RowSource for Broken {
            fn shape(&self, _: u16, _: Option<u32>) -> Res<Option<(DType, Vec<u64>)>> {
                Ok(Some((DType::I8, vec![4, 2])))
            }
            fn rows(&self, _: u16, _: Option<u32>, r0: u64, _: u64) -> Res<Tensor> {
                if r0 == 0 { err(Class::Missing, "injected source I/O failure") } else { unreachable!() }
            }
        }
        let tp = TiledParams::new(&Broken, 1);
        let lazy = LazyParam { tiles: &tp, index: 0, layer: None, dtype: DType::I8, shape: vec![4, 2], transposed: false };
        let x = Tensor::zeros(DType::I8, vec![1, 4]);
        let ph = Tensor::zeros(DType::I8, vec![]);
        let e = eval_prim_tiled(&Prim::MatMul, &[&x, &ph], 1, &lazy, DType::I64, &[1, 2]).unwrap_err();
        assert_eq!((e.class, e.reason.as_str()), (Class::Missing, "injected source I/O failure"));

        // A tile with a value outside the declared dtype, or another shape, is malformed.
        let mut bad = Params::new();
        bad.insert((0, None), Tensor { dtype: DType::I8, shape: vec![4, 2], data: vec![0, 0, 0, 0, 0, 999, 0, 0] });
        let source = MapRowSource(&bad);
        let tp = TiledParams::new(&source, 2);
        let lazy = LazyParam { tiles: &tp, index: 0, layer: None, dtype: DType::I8, shape: vec![4, 2], transposed: false };
        let e = eval_prim_tiled(&Prim::MatMul, &[&x, &ph], 1, &lazy, DType::I64, &[1, 2]).unwrap_err();
        assert_eq!((e.class, e.reason.as_str()), (Class::Operand, "param: malformed tensor"));
        let short = Tensor { dtype: DType::I8, shape: vec![4, 2], data: vec![0; 7] };
        let mut torn = Params::new();
        torn.insert((0, None), short);
        let e = MapRowSource(&torn).rows(0, None, 0, 1).unwrap_err();
        assert_eq!(e.class, Class::Operand);
        // The shape rules are the whole path's, from the declaration: a contraction that does not match is refused before any read.
        let wrong_x = Tensor::zeros(DType::I8, vec![1, 5]);
        let tp = TiledParams::new(&source, 2);
        let lazy = LazyParam { tiles: &tp, index: 0, layer: None, dtype: DType::I8, shape: vec![4, 2], transposed: false };
        let e = eval_prim_tiled(&Prim::MatMul, &[&wrong_x, &ph], 1, &lazy, DType::I64, &[1, 2]).unwrap_err();
        assert_eq!(e.class, Class::Shape);
        assert_eq!(tp.report().tiles, 0, "nothing was read");
    }

    #[test]
    fn strict_tiling_turns_the_bound_into_a_guarantee() {
        let mut map = Params::new();
        map.insert((0, None), Tensor::zeros(DType::I8, vec![8]));
        map.insert((1, None), Tensor::zeros(DType::I8, vec![2]));
        let source = MapRowSource(&map);
        let loose = TiledParams::new(&source, 4);
        assert!(loose.tensor(0, None).unwrap().is_some(), "a primitive with no tiled form loads a big param whole…");
        assert_eq!(loose.report().whole_peak_elems, 8, "…and the meter says so");
        let strict = TiledParams::new(&source, 4).strict();
        assert!(strict.tensor(0, None).is_err(), "strict refuses it");
        assert!(strict.tensor(1, None).unwrap().is_some(), "a param of at most one tile is loaded whole");
        assert!(strict.tensor(9, None).unwrap().is_none(), "an absent param is absent");
    }
}
