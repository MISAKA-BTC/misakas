//! H dissection (04b §9.5, PALW-TIR-37), written from the text alone: the reductions over `H` of a
//! committed tile's cone and its site (§9.5.1), the root claim — its finalize and element closure
//! (§9.5.3) — the cut, the rounds and their folds (§9.5.4), the bottom (§9.5.5), and the admission
//! obligations a dissected cone owes (§9.5.6). Range evaluation (§9.5.2) is `demand::eval_range`.
//!
//! The chain carriage (§9.5.9: objects, signatures, clocks, the carriages' evidence) is Phase F's
//! and is not modelled: a source stands for the evidence a carriage holds.

use std::collections::{BTreeMap, BTreeSet};

use crate::admit::{Interval, cone};
use crate::demand::{Ctx, DemandError, Limits, Source, Supply, eval_range, reduces_over_h};
use crate::normal_form::block_window;
use crate::program::{Prim, Program, Ref};
use crate::tensor::count;
use crate::types::Dim;

/// `PALW_TIR_DISSECT_MAX_REDUCTIONS`.
pub const MAX_REDUCTIONS: usize = 16;
/// `PALW_TIR_DISSECT_MAX_VALUES`.
pub const MAX_VALUES: usize = 4096;
/// The move's frame and the carrier (§9.5.6, O-5).
pub const MOVE_FRAME_BYTES: u64 = 4_764;
pub const CARRIER_BYTES: u64 = 100_000;
pub const CLOSE_FRAME_BYTES: u64 = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fold {
    Sum,
    Max,
}

/// §9.5.1: the site of a dissected tile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Site {
    pub ctx: Ctx,
    pub block: usize,
    pub node: u16,
    pub reductions: Vec<u16>,
    pub folds: Vec<Fold>,
    pub bounds: Vec<Interval>,
    pub counts: Vec<u64>,
    pub h: u64,
    pub h_tile: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SiteError {
    NoSuchCommitPoint,
    TooManyReductions(usize),
}

fn occurrence_block(p: &Program, occ: u32) -> Option<usize> {
    let o = occ as usize;
    let l = p.schedule.layers.len();
    if o == 0 {
        Some(p.schedule.pre as usize)
    } else if o <= l {
        Some(p.schedule.layers[o - 1] as usize)
    } else if o == l + 1 {
        Some(p.schedule.post as usize)
    } else {
        None
    }
}

fn history_len(p: &Program, b: usize, pos: u64) -> u64 {
    match block_window(p, b).ok().flatten() {
        Some(w) => (pos + 1).min(w as u64),
        None => 1,
    }
}

/// §9.5.1: the cone's reductions over `H`, in node index order (`n` itself included if it reduces
/// over `H`; another commit point never is one — it is a leaf of the cone).
pub fn cone_reductions(p: &Program, b: usize, n: u16) -> Vec<u16> {
    cone(p, b, n as usize).0.into_iter().filter(|&i| reduces_over_h(p, b, i as usize)).collect()
}

/// §9.5.1: the site of commit point `n` in context `ctx`, `None` if its tile is not dissected.
pub fn site(p: &Program, intervals: &[Vec<Interval>], ctx: Ctx, n: u16, h_tile: u64) -> Result<Option<Site>, SiteError> {
    let b = occurrence_block(p, ctx.occ).ok_or(SiteError::NoSuchCommitPoint)?;
    let node = p.blocks[b].nodes.get(n as usize).ok_or(SiteError::NoSuchCommitPoint)?;
    if !node.commit {
        return Err(SiteError::NoSuchCommitPoint);
    }
    let reductions = cone_reductions(p, b, n);
    if reductions.is_empty() {
        return Ok(None);
    }
    if reductions.len() > MAX_REDUCTIONS {
        return Err(SiteError::TooManyReductions(reductions.len()));
    }
    let h = history_len(p, b, ctx.pos);
    let block = &p.blocks[b];
    let folds = reductions
        .iter()
        .map(|&r| if matches!(block.nodes[r as usize].prim, Prim::ReduceMax { .. }) { Fold::Max } else { Fold::Sum })
        .collect();
    let bounds = reductions.iter().map(|&r| intervals[b][r as usize]).collect();
    let counts = reductions.iter().map(|&r| count(&block.nodes[r as usize].out.extents(h))).collect();
    Ok(Some(Site { ctx, block: b, node: n, reductions, folds, bounds, counts, h, h_tile }))
}

/// A range claim, or the totals of a root claim: one value list per reduction.
pub type Values = Vec<Vec<i128>>;

/// A root claim `(L, T)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootClaim {
    pub elements: Vec<Vec<u64>>,
    pub totals: Values,
}

/// A source answering the site's supplied reductions from a claim, in the site's context, and every
/// other question from the evidence. It records every supplied element read, claimed or not.
pub struct Claimed<'a> {
    pub evidence: &'a mut dyn Source,
    ctx: Ctx,
    reductions: &'a [u16],
    lists: &'a [Vec<u64>],
    values: &'a Values,
    /// `(reduction index, element)` of every supplied read.
    pub reads: BTreeSet<(usize, u64)>,
    /// Reads of an element the claim does not list (refused).
    pub unclaimed: BTreeSet<(usize, u64)>,
}

impl<'a> Claimed<'a> {
    pub fn new(evidence: &'a mut dyn Source, site: &'a Site, lists: &'a [Vec<u64>], values: &'a Values) -> Self {
        Claimed {
            evidence,
            ctx: site.ctx,
            reductions: &site.reductions,
            lists,
            values,
            reads: BTreeSet::new(),
            unclaimed: BTreeSet::new(),
        }
    }
}

impl Source for Claimed<'_> {
    fn node(&mut self, ctx: Ctx, node: u16, i: u64) -> Option<i128> {
        if ctx == self.ctx
            && let Some(j) = self.reductions.iter().position(|&r| r == node)
        {
            // Only a supplied reduction is asked here (the evaluated one is the target, computed).
            self.reads.insert((j, i));
            return match self.lists.get(j).and_then(|l| l.binary_search(&i).ok()) {
                Some(k) => self.values.get(j).and_then(|v| v.get(k)).copied(),
                None => {
                    self.unclaimed.insert((j, i));
                    None
                }
            };
        }
        self.evidence.node(ctx, node, i)
    }
    fn param(&mut self, param: u16, layer: Option<u32>, i: u64) -> Option<i128> {
        self.evidence.param(param, layer, i)
    }
    fn state(&mut self, pos: u64, state: u16, layer: Option<u32>, i: u64) -> Option<Supply> {
        self.evidence.state(pos, state, layer, i)
    }
    fn hist_row(&mut self, pos: u64, state: u16, layer: Option<u32>, row_pos: u64, i: u64) -> Option<i128> {
        self.evidence.hist_row(pos, state, layer, row_pos, i)
    }
    fn token(&mut self, pos: u64) -> Option<u64> {
        self.evidence.token(pos)
    }
}

/// `S_i`: every reduction of the site but `r_i` (or but `n` for the finalize).
fn others(site: &Site, except: u16) -> Vec<u16> {
    site.reductions.iter().copied().filter(|&r| r != except).collect()
}

/// §9.5.3 the finalize: the tile's elements with every reduction supplied from the totals; for
/// `n = r_m`, the totals themselves. Returns the values and the supplied elements read.
pub fn finalize(
    p: &Program,
    site: &Site,
    tile: &[u64],
    claim: &RootClaim,
    evidence: &mut dyn Source,
    limits: Limits,
) -> Result<(Vec<i128>, BTreeSet<(usize, u64)>), DemandError> {
    let m = site.reductions.len();
    if site.reductions[m - 1] == site.node {
        let mut vals = Vec::with_capacity(tile.len());
        let mut reads = BTreeSet::new();
        for &e in tile {
            reads.insert((m - 1, e));
            match claim.elements[m - 1].binary_search(&e) {
                Ok(k) => vals.push(claim.totals[m - 1][k]),
                Err(_) => return Err(DemandError::Class(crate::error::Class::Missing)),
            }
        }
        return Ok((vals, reads));
    }
    let mut src = Claimed::new(evidence, site, &claim.elements, &claim.totals);
    let supplied = others(site, site.node);
    let (vals, _) = eval_range(p, site.ctx, site.node, tile, &supplied, None, &mut src, limits)?;
    Ok((vals, src.reads))
}

/// §9.5.3 the element closure, computed with the claim's totals supplied: the finalize's reads,
/// then every probe `eval_range(ctx, r_i, [e], S_i, (0, 1))` to a fixpoint. A probe that reads an
/// unclaimed element fails (the source refuses it).
pub fn closure(
    p: &Program,
    site: &Site,
    tile: &[u64],
    claim: &RootClaim,
    evidence: &mut dyn Source,
    limits: Limits,
) -> Result<Vec<BTreeSet<u64>>, DemandError> {
    let m = site.reductions.len();
    let mut c: Vec<BTreeSet<u64>> = vec![BTreeSet::new(); m];
    let (_, reads) = finalize(p, site, tile, claim, evidence, limits)?;
    let mut work: Vec<(usize, u64)> = Vec::new();
    for (j, e) in reads {
        if c[j].insert(e) {
            work.push((j, e));
        }
    }
    while let Some((i, e)) = work.pop() {
        let r = site.reductions[i];
        let mut src = Claimed::new(evidence, site, &claim.elements, &claim.totals);
        eval_range(p, site.ctx, r, &[e], &others(site, r), Some((0, 1)), &mut src, limits)?;
        for (j, f) in src.reads {
            if c[j].insert(f) {
                work.push((j, f));
            }
        }
    }
    Ok(c)
}

/// Why a root claim, a round or a bottom carriage is refused (a refused MOVE, never a verdict).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Step 2 or a round's shape.
    Shape(&'static str),
    /// Step 3 or a round value outside its reduction's bound.
    OutsideBound { reduction: usize, element: usize, value: i128 },
    /// Step 4: the finalize fails, or does not reproduce the committed tile at `index`.
    Finalize(DemandError),
    NotTheTile { index: usize },
    /// Step 5: the closure fails, or differs from the claimed lists.
    Closure(DemandError),
    ClosureDiffers { reduction: usize },
    /// A round's children do not fold to the claim under dispute.
    DoesNotFold { reduction: usize, element: usize },
    /// A round with the wrong number of children.
    ChildCount { got: usize, expected: usize },
}

/// Step 2's shape of a claim: the site's reductions, strictly ascending lists below each count, one value per element, at most 4096 values.
fn check_shape(site: &Site, elements: &[Vec<u64>], values: &Values) -> Result<(), Refusal> {
    let m = site.reductions.len();
    if elements.len() != m || values.len() != m {
        return Err(Refusal::Shape("one list and one value list per reduction"));
    }
    let mut total = 0usize;
    for i in 0..m {
        let l = &elements[i];
        if l.windows(2).any(|w| w[0] >= w[1]) {
            return Err(Refusal::Shape("an element list is not strictly ascending"));
        }
        if l.iter().any(|&e| e >= site.counts[i]) {
            return Err(Refusal::Shape("an element past the reduction's count"));
        }
        if values[i].len() != l.len() {
            return Err(Refusal::Shape("a value list of another length than its element list"));
        }
        total += l.len();
    }
    if total > MAX_VALUES {
        return Err(Refusal::Shape("more than 4096 values"));
    }
    Ok(())
}

fn check_bounds(site: &Site, values: &Values) -> Result<(), Refusal> {
    for (i, v) in values.iter().enumerate() {
        let b = site.bounds[i];
        if let Some(k) = v.iter().position(|&x| x < b.lo || x > b.hi) {
            return Err(Refusal::OutsideBound { reduction: i, element: k, value: v[k] });
        }
    }
    Ok(())
}

/// §9.5.3 steps 2–5: whether a root claim is admitted for the committed tile `committed` (the
/// values of elements `tile`). Step 1 and the carriage's own checks are the protocol's.
pub fn admit_root(
    p: &Program,
    site: &Site,
    tile: &[u64],
    committed: &[i128],
    claim: &RootClaim,
    evidence: &mut dyn Source,
    limits: Limits,
) -> Result<(), Refusal> {
    check_shape(site, &claim.elements, &claim.totals)?;
    check_bounds(site, &claim.totals)?;
    let (vals, _) = finalize(p, site, tile, claim, evidence, limits).map_err(Refusal::Finalize)?;
    if let Some(k) = vals.iter().zip(committed).position(|(a, b)| a != b) {
        return Err(Refusal::NotTheTile { index: k });
    }
    let c = closure(p, site, tile, claim, evidence, limits).map_err(Refusal::Closure)?;
    for (i, set) in c.iter().enumerate() {
        if set.iter().copied().collect::<Vec<_>>() != claim.elements[i] {
            return Err(Refusal::ClosureDiffers { reduction: i });
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ rounds and the bottom

/// `T_h = ⌈H / h_tile⌉`.
pub fn history_tiles(site: &Site) -> u64 {
    site.h.div_ceil(site.h_tile)
}

/// §9.5.4 the cut at arity `k` of `(first, count)`, `count ≥ 2`.
pub fn cut(first: u64, count: u64, k: u64) -> Vec<(u64, u64)> {
    let w = count.div_ceil(k);
    let mut out = Vec::new();
    let mut s = 0;
    while s < count {
        out.push((first + s, w.min(count - s)));
        s += w;
    }
    out
}

/// The number of rounds from `t_h` tiles to the bottom at arity `k`.
pub fn round_budget(t_h: u64, k: u64) -> u32 {
    let (mut c, mut n) = (t_h, 0);
    while c > 1 {
        c = c.div_ceil(k);
        n += 1;
    }
    n
}

/// The positions `[from, to)` of a range of history tiles.
pub fn positions(site: &Site, first: u64, count: u64) -> (u64, u64) {
    (first * site.h_tile, ((first + count) * site.h_tile).min(site.h))
}

/// The partials over `[from, to)` for every `(i, e)` of the root's lists, every other reduction
/// supplied from the ROOT's totals.
pub fn partials(
    p: &Program,
    site: &Site,
    root: &RootClaim,
    range: (u64, u64),
    evidence: &mut dyn Source,
    limits: Limits,
) -> Result<Values, DemandError> {
    let mut out = Vec::with_capacity(site.reductions.len());
    for (i, &r) in site.reductions.iter().enumerate() {
        let mut src = Claimed::new(evidence, site, &root.elements, &root.totals);
        let (v, _) = eval_range(p, site.ctx, r, &root.elements[i], &others(site, r), Some(range), &mut src, limits)?;
        out.push(v);
    }
    Ok(out)
}

/// §9.5.4: whether a round (one claim per child of the cut) is admitted against the claim under
/// dispute.
pub fn check_round(site: &Site, root: &RootClaim, disputed: &Values, children: &[Values], expected: usize) -> Result<(), Refusal> {
    if children.len() != expected {
        return Err(Refusal::ChildCount { got: children.len(), expected });
    }
    for ch in children {
        check_shape(site, &root.elements, ch)?;
        check_bounds(site, ch)?;
    }
    for (i, fold) in site.folds.iter().enumerate() {
        for (k, &want) in disputed[i].iter().enumerate() {
            let folded: Option<i128> = match fold {
                // Exact, as unbounded integers: at most 64 children of i128 values, summed in 256 bits.
                Fold::Sum => {
                    let mut acc = crate::wide::Wide::ZERO;
                    for ch in children {
                        acc = acc.checked_add(crate::wide::Wide::from_i128(ch[i][k])).expect("64 i128 values fit 256 bits");
                    }
                    if acc.cmp_wide(&crate::wide::Wide::from_i128(want)) == std::cmp::Ordering::Equal { Some(want) } else { None }
                }
                Fold::Max => children.iter().map(|ch| ch[i][k]).max(),
            };
            if folded != Some(want) {
                return Err(Refusal::DoesNotFold { reduction: i, element: k });
            }
        }
    }
    Ok(())
}

/// §9.5.5's verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The first differing value, flattened `i` major then `e`: the executor is convicted.
    Mismatch { value_index: usize },
    /// No fault: the challenger is defeated.
    Defeated,
}

/// §9.5.5 the bottom: evaluate every `(i, e)` over the tile's positions, every other reduction
/// supplied from the ROOT's totals, and compare with the claim.
pub fn bottom(
    p: &Program,
    site: &Site,
    root: &RootClaim,
    range: (u64, u64),
    claim: &Values,
    evidence: &mut dyn Source,
    limits: Limits,
) -> Result<Verdict, DemandError> {
    let court = partials(p, site, root, range, evidence, limits)?;
    let flat_c: Vec<i128> = court.into_iter().flatten().collect();
    let flat_x: Vec<i128> = claim.iter().flatten().copied().collect();
    Ok(match flat_c.iter().zip(flat_x.iter()).position(|(a, b)| a != b) {
        Some(k) => Verdict::Mismatch { value_index: k },
        None if flat_c.len() != flat_x.len() => Verdict::Mismatch { value_index: flat_c.len().min(flat_x.len()) },
        None => Verdict::Defeated,
    })
}

// ------------------------------------------------------------------ admission obligations

/// §9.5.6: a broken obligation of a dissected cone.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Obligation {
    /// O-1.
    TooManyReductions(usize),
    /// O-2: a computed `Gather` or `Select` of the cone whose history-varying operand chooses an
    /// element of (something depending on) a reduction over `H`.
    DataDependentRead { node: u16 },
    /// O-3: a reduction over `H` whose output has `H`.
    TotalCarriesH { node: u16 },
}

fn carries_h(t: &crate::types::TensorType) -> bool {
    t.shape.contains(&Dim::H)
}

/// §9.5.6 O-1 to O-3 for the cone of commit point `n` of block `b`: every broken obligation.
pub fn obligations(p: &Program, b: usize, n: u16) -> Vec<Obligation> {
    let block = &p.blocks[b];
    let (nodes, _) = cone(p, b, n as usize);
    let in_cone: BTreeSet<u16> = nodes.iter().copied().collect();
    let reds: BTreeSet<u16> = nodes.iter().copied().filter(|&i| reduces_over_h(p, b, i as usize)).collect();
    let mut out = Vec::new();
    if reds.is_empty() {
        return out;
    }
    if reds.len() > MAX_REDUCTIONS {
        out.push(Obligation::TooManyReductions(reds.len()));
    }
    // depends[i]: node i of the cone is, or reads through computed cone nodes, a reduction over H.
    let mut depends: BTreeMap<u16, bool> = BTreeMap::new();
    for &i in &nodes {
        let node = &block.nodes[i as usize];
        let d = reds.contains(&i)
            || node.inputs.iter().any(|r| matches!(*r, Ref::Node(k) if in_cone.contains(&k) && depends.get(&k).copied().unwrap_or(false)));
        depends.insert(i, d);
    }
    let dep = |r: &Ref| matches!(*r, Ref::Node(k) if in_cone.contains(&k) && depends[&k]);
    let ty = |i: u16, r: &Ref| crate::normal_form::ref_type(p, b, i as usize, r).expect("normal form");
    for &i in &nodes {
        let node = &block.nodes[i as usize];
        match node.prim {
            Prim::Gather { .. } if carries_h(&ty(i, &node.inputs[1])) && dep(&node.inputs[0]) => {
                out.push(Obligation::DataDependentRead { node: i });
            }
            Prim::Select if carries_h(&ty(i, &node.inputs[0])) && (dep(&node.inputs[1]) || dep(&node.inputs[2])) => {
                out.push(Obligation::DataDependentRead { node: i });
            }
            _ => {}
        }
    }
    for &r in &reds {
        if carries_h(&block.nodes[r as usize].out) {
            out.push(Obligation::TotalCarriesH { node: r });
        }
    }
    out
}

/// §9.5.6 O-5's `V`: the box demand at `H = 1` arriving at the cone's reductions over `H`, from the
/// tile's `tile_len` elements.
pub fn value_bound(p: &Program, b: usize, n: u16, tile_len: u64) -> u64 {
    let block = &p.blocks[b];
    let (nodes, _) = cone(p, b, n as usize);
    let in_cone: BTreeSet<u16> = nodes.iter().copied().collect();
    let e = |t: &crate::types::TensorType| count(&t.extents(1));
    let mut d: BTreeMap<u16, u64> = BTreeMap::new();
    d.insert(n, tile_len.min(e(&block.nodes[n as usize].out)));
    for &i in nodes.iter().rev() {
        let di = d.get(&i).copied().unwrap_or(0);
        if di == 0 {
            continue;
        }
        let node = &block.nodes[i as usize];
        for r in &node.inputs {
            let Ref::Node(k) = *r else { continue };
            if !in_cone.contains(&k) {
                continue;
            }
            let x = &block.nodes[k as usize].out;
            let raw = match node.prim {
                Prim::MatMul => {
                    let a = crate::normal_form::ref_type(p, b, i as usize, &node.inputs[0]).expect("normal form");
                    di.saturating_mul(*a.extents(1).last().unwrap())
                }
                Prim::ReduceSum { axis } | Prim::ReduceMax { axis } => di.saturating_mul(x.extents(1)[axis as usize]),
                Prim::TopK { axis, k: kk } => di.div_ceil(kk as u64).saturating_mul(x.extents(1)[axis as usize]),
                _ => di,
            };
            let v = d.entry(k).or_insert(0);
            *v = v.saturating_add(raw.min(e(x))).min(e(x));
        }
    }
    nodes.iter().filter(|&&i| reduces_over_h(p, b, i as usize)).map(|i| d.get(i).copied().unwrap_or(0)).fold(0u64, |a, v| a.saturating_add(v))
}

/// §9.5.6 O-5: a round's bytes at arity `k` with `m` reductions and `V` values, without the move's
/// frame.
pub fn round_bytes(k: u64, m: u64, v: u64) -> u64 {
    6 + k * (4 + 4 * m + 16 * v)
}
