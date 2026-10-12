//! **A seat's sketch store: one class, one epoch** (RFC-0007 Part II, §II.4).
//!
//! Built once per epoch from the artifact the seat already holds — in one pass over every param
//! instance — and then the only form in which the class's weight matrices take part in a check:
//!
//! 1. **ranges** — the `[min, max]` of every param instance. With them the plan is refined
//!    ([`misaka_palw_tir_exec::TirPlan::refine`]): each node's proven interval tightens from the
//!    type-worst-case weights of admission to the weights at hand, which is what lets an `i64`
//!    accumulator be checked over one 61-bit prime;
//! 2. **moduli** — per weight `MatMul`, the fewest rungs of the ladder whose product exceeds the
//!    span of its refined interval ([`crate::field::tir_sketch_moduli_for_span_v1`]);
//! 3. **sketches** — per weight `MatMul`, per modulus, per expert: `S = W·v` with `v` the site's
//!    vector from the seat's keys ([`crate::geom::TirCheckGeomV1::sketch`]).
//!
//! After the build the store needs none of the weights it sketched. A store is as secret as the
//! seat's secret (`crate::secret`): it has no encoding and no `Debug` of its contents.
//!
//! **Through the residency store.** This prototype reads params through the reference
//! [`ParamSource`]; a node reads them through its residency's row source (lane M2's
//! `TirRowSourceV1`), one row at a time: a sketch is a sum over the rows of `W` (`S += v[r]·W[r, :]`
//! on the left, `S[t] = W[t, :]·v` on the right), so it streams in row order and never holds more
//! than a row. An expert stack is one row range per expert.

use std::collections::BTreeMap;

use misaka_palw_tir::interval::Interval;
use misaka_palw_tir::program::Ref;
use misaka_palw_tir::{ConeEnv, Interpreter, ParamSource, Tensor, TirError, TirErrorKind, TirResult};
use misaka_palw_tir_exec::TirPlan;
use misaka_palw_tir_exec::plan::BlockPlan;

use crate::analysis::{TirMatMulKindV1, TirSketchAnalysisV1, TirWeightSourceV1};
use crate::field::{TirSketchModulusV1, tir_sketch_moduli_for_span_v1};
use crate::geom::TirCheckGeomV1;
use crate::secret::TirSketchKeysV1;

/// One weight `MatMul`'s sketches in one occurrence.
pub struct TirNodeSketchV1 {
    /// The moduli the node is checked over.
    pub moduli: Vec<TirSketchModulusV1>,
    /// Per modulus: the compression vector (the check's left side reads it).
    pub v: Vec<Vec<u64>>,
    /// Per modulus: `experts × s_len` sketch entries, expert-major.
    pub s: Vec<Vec<u64>>,
    /// 1 for a static weight; the stack's depth for a routed one.
    pub experts: usize,
    /// Entries of one expert's sketch.
    pub s_len: usize,
    /// Elements of the weight the sketches stand for (all experts).
    pub weight_elements: u64,
    /// Bytes of that weight at its param width.
    pub weight_bytes: u64,
    /// **Row-block sketches** (§II.8): the free-axis blocks (`ceil(weight_bytes / F)`, `F` = [`TIR_BLOCK_FETCH_CAP_BYTES_V1`]) and, per
    /// modulus, the sketch of each (`experts × s_len` entries, expert-major). Their sum is `s`. With one block (a weight within the
    /// fetch cap) nothing extra is stored: block 0 is the whole sketch.
    pub blocks: usize,
    pub s_blocks: Vec<Vec<Vec<u64>>>,
}

/// **The fetch cap `F`** of a failed check's escalation (§II.8, proposed 2 MiB): a seat names the failing block and fetches only its
/// `|W| / B <= F` bytes of weight.
pub const TIR_BLOCK_FETCH_CAP_BYTES_V1: u64 = 2 * 1024 * 1024;

/// **How many blocks a weight of `weight_bytes` is sketched in**: `ceil(weight_bytes / F)`, at least one.
pub fn tir_block_count_v1(weight_bytes: u64) -> usize {
    weight_bytes.div_ceil(TIR_BLOCK_FETCH_CAP_BYTES_V1).max(1) as usize
}

impl TirNodeSketchV1 {
    /// The bytes of weight one block stands for (the transfer bound of one escalation: at most `F`, or the whole weight if smaller).
    pub fn block_fetch_bytes(&self) -> u64 {
        self.weight_bytes.div_ceil(self.blocks.max(1) as u64)
    }

    /// Block `b`'s sketch over modulus `mi` (`experts × s_len` entries): the whole sketch when there is one block.
    pub fn block_sketch(&self, mi: usize, b: usize) -> Option<&[u64]> {
        if self.blocks <= 1 {
            return (b == 0).then(|| &self.s[mi][..]);
        }
        self.s_blocks.get(mi)?.get(b).map(|v| &v[..])
    }
}

/// What a store holds, in numbers (the measurement's sketch-to-weight ratio).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TirSketchStatsV1 {
    /// Weight `MatMul` sites sketched (per occurrence).
    pub sites: u64,
    /// Field entries held: sketches and vectors, every modulus.
    pub entries: u64,
    /// The weight elements they stand for.
    pub weight_elements: u64,
    /// The weight bytes they stand for.
    pub weight_bytes: u64,
    /// Sites checked over two moduli, and over three.
    pub wide_sites: u64,
    pub widest_sites: u64,
}

/// **A seat's sketches of one class for one epoch** (module note).
pub struct TirSketchStoreV1 {
    sketches: BTreeMap<(u16, u16), TirNodeSketchV1>,
    ranges: BTreeMap<(u16, Option<u16>), Interval>,
    stats: TirSketchStatsV1,
}

impl std::fmt::Debug for TirSketchStoreV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TirSketchStoreV1({} sites, ..)", self.sketches.len())
    }
}

/// The value of a static operand: a param, a const, or a node over them.
pub(crate) fn static_value(
    interp: &Interpreter<'_>,
    params: &dyn ParamSource,
    block: u8,
    layer: Option<u16>,
    r: Ref,
) -> TirResult<Tensor> {
    let p = interp.program;
    match r {
        Ref::Param(j) => {
            let d = &p.params[j as usize];
            let l = if d.per_layer { layer } else { None };
            let t = params.param(j, l).ok_or_else(|| TirError::new(TirErrorKind::Missing, format!("param {} ({l:?})", d.name)))?;
            let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
            if t.dtype != d.dtype || t.shape != shape || t.data.len() != shape.iter().product::<usize>() {
                return Err(TirError::new(TirErrorKind::Operand, format!("param {} is not its declaration", d.name)));
            }
            Ok(t)
        }
        Ref::Const(j) => {
            let c = &p.consts[j as usize];
            let shape: Vec<usize> = c.shape.iter().map(|x| *x as usize).collect();
            Tensor::from_le_bytes(c.dtype, &shape, &c.data)
        }
        Ref::Node(i) => interp.eval_cone(block, layer, i, params, &ConeEnv::default()),
        _ => Err(TirError::new(TirErrorKind::Malformed, "a static operand that is not static")),
    }
}

/// The interval of a param instance's elements.
fn range_of(t: &Tensor) -> Option<Interval> {
    let lo = t.data.iter().min()?;
    let hi = t.data.iter().max()?;
    Some(Interval::new(*lo, *hi))
}

impl TirSketchStoreV1 {
    /// **Build the store** from the full params (module note, steps 1–3).
    pub fn build(plan: &TirPlan, analysis: &TirSketchAnalysisV1, params: &dyn ParamSource, keys: &TirSketchKeysV1) -> TirResult<Self> {
        Self::build_with(plan, analysis, params, keys, None)
    }

    /// [`Self::build`] with every node forced onto `moduli` — the tests' way of showing what a
    /// node checked over too few moduli lets through. No seat does this.
    #[doc(hidden)]
    pub fn build_with(
        plan: &TirPlan,
        analysis: &TirSketchAnalysisV1,
        params: &dyn ParamSource,
        keys: &TirSketchKeysV1,
        moduli: Option<&[TirSketchModulusV1]>,
    ) -> TirResult<Self> {
        Self::build_inner(plan, analysis, params, keys, moduli, None)
    }

    /// [`Self::build`] with every weight sketched in `blocks` free-axis blocks (clamped to its free extent): the tests' way of showing
    /// §II.8's localisation on weights far below the fetch cap. No seat does this.
    #[doc(hidden)]
    pub fn build_blocked(
        plan: &TirPlan,
        analysis: &TirSketchAnalysisV1,
        params: &dyn ParamSource,
        keys: &TirSketchKeysV1,
        blocks: usize,
    ) -> TirResult<Self> {
        Self::build_inner(plan, analysis, params, keys, None, Some(blocks))
    }

    fn build_inner(
        plan: &TirPlan,
        analysis: &TirSketchAnalysisV1,
        params: &dyn ParamSource,
        keys: &TirSketchKeysV1,
        moduli: Option<&[TirSketchModulusV1]>,
        force_blocks: Option<usize>,
    ) -> TirResult<Self> {
        let p = &plan.program;
        let interp = Interpreter::new(p)?;
        // 1. Ranges, in one pass over every instance the plan reads.
        let mut ranges = BTreeMap::new();
        for &(j, layer) in &plan.param_instances {
            let t = params.param(j, layer).ok_or_else(|| TirError::new(TirErrorKind::Missing, format!("param {j} ({layer:?})")))?;
            if let Some(iv) = range_of(&t) {
                ranges.insert((j, layer), iv);
            }
        }
        let occ_plans = plan.refine(&|j, l| ranges.get(&(j, l)).copied());
        // 2–3. Moduli and sketches per weight site per occurrence.
        let mut sketches = BTreeMap::new();
        let mut stats = TirSketchStatsV1::default();
        for (occ, &(block, layer)) in plan.occurrences.iter().enumerate() {
            let occ = occ as u16;
            for site in &analysis.blocks[block as usize].matmuls {
                let TirMatMulKindV1::Weight { side, source } = site.kind else { continue };
                let np = &occ_plans[occ as usize].nodes[site.node as usize];
                let facts = np.facts;
                let span = facts.out.hi.abs_diff(facts.out.lo);
                let mods = match moduli {
                    Some(m) => m.to_vec(),
                    None => tir_sketch_moduli_for_span_v1(span),
                };
                let (ta, tb) = (&np.in_types[0], &np.in_types[1]);
                let routed_rank = match source {
                    TirWeightSourceV1::Routed { idx_rank, .. } => idx_rank as usize,
                    TirWeightSourceV1::Static(_) => 0,
                };
                let g = TirCheckGeomV1::new(side, ta, tb, 1, routed_rank);
                let data = static_value(&interp, params, block, layer, source.data())?;
                let experts = if routed_rank > 0 { data.shape[0] } else { 1 };
                let body = g.body_len();
                if data.data.len() != experts * body {
                    return Err(TirError::new(
                        TirErrorKind::Shape,
                        format!(
                            "weight of node {} in occurrence {occ}: {} elements, {experts} × {body} expected",
                            site.node,
                            data.data.len()
                        ),
                    ));
                }
                let mut v_all = Vec::with_capacity(mods.len());
                let mut s_all = Vec::with_capacity(mods.len());
                let mut sb_all = Vec::with_capacity(mods.len());
                let nblocks = force_blocks
                    .unwrap_or_else(|| tir_block_count_v1(data.data.len() as u64 * data.dtype.width() as u64))
                    .clamp(1, g.free().max(1));
                for md in &mods {
                    let v = keys.site_vector(occ, site.node, *md, g.v_len());
                    let mut s = Vec::with_capacity(experts * g.s_len());
                    let mut sb: Vec<Vec<u64>> = if nblocks > 1 { vec![Vec::with_capacity(experts * g.s_len()); nblocks] } else { Vec::new() };
                    for e in 0..experts {
                        let slice = &data.data[e * body..(e + 1) * body];
                        if nblocks > 1 {
                            let parts = g.sketch_blocks(slice, &v, *md, nblocks);
                            let mut whole = vec![0u64; g.s_len()];
                            for part in &parts {
                                for (w, x) in whole.iter_mut().zip(part) {
                                    *w = md.add(*w, *x);
                                }
                            }
                            s.extend(whole);
                            for (dst, part) in sb.iter_mut().zip(parts) {
                                dst.extend(part);
                            }
                        } else {
                            s.extend(g.sketch(slice, &v, *md));
                        }
                    }
                    stats.entries += (v.len() + s.len() + sb.iter().map(Vec::len).sum::<usize>()) as u64;
                    v_all.push(v);
                    s_all.push(s);
                    sb_all.push(sb);
                }
                stats.sites += 1;
                stats.wide_sites += (mods.len() == 2) as u64;
                stats.widest_sites += (mods.len() == 3) as u64;
                let weight_elements = data.data.len() as u64;
                let weight_bytes = weight_elements * data.dtype.width() as u64;
                stats.weight_elements += weight_elements;
                stats.weight_bytes += weight_bytes;
                sketches.insert(
                    (occ, site.node),
                    TirNodeSketchV1 {
                        moduli: mods,
                        v: v_all,
                        s: s_all,
                        experts,
                        s_len: g.s_len(),
                        weight_elements,
                        weight_bytes,
                        blocks: nblocks,
                        s_blocks: sb_all,
                    },
                );
            }
        }
        Ok(Self { sketches, ranges, stats })
    }

    pub fn get(&self, occurrence: u16, node: u16) -> Option<&TirNodeSketchV1> {
        self.sketches.get(&(occurrence, node))
    }

    /// The `[min, max]` of a param instance's elements, as the build read it.
    pub fn param_range(&self, j: u16, layer: Option<u16>) -> Option<Interval> {
        self.ranges.get(&(j, layer)).copied()
    }

    /// The plan refined by the ranges the build read: the intervals every check is bounded by.
    pub fn refined_plans(&self, plan: &TirPlan) -> Vec<BlockPlan> {
        plan.refine(&|j, l| self.param_range(j, l))
    }

    pub fn stats(&self) -> TirSketchStatsV1 {
        self.stats
    }
}
