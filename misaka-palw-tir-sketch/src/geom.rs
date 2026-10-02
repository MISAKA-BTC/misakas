//! **How one `MatMul`'s check is laid out** (RFC-0007 Part II, §II.3) — the index arithmetic the
//! sketch builder and the checker share.
//!
//! A `MatMul` is `out[β, r, c] = Σ_t a[β_a, r, t] · b[β_b, t, c]` with the batch index `β` broadcast
//! into each operand (spec 04b §6.3). Call the weight operand `W` (left or right) and the other one
//! the activation `X`. An output axis is **free for the weight** when `X` does not vary along it:
//! the weight's own free matrix axis (`c` when `W = b`, `r` when `W = a`), and every batch axis along
//! which `X` is broadcast (declared `Fixed(1)` or absent) and `W` is not gathered. The check draws a
//! random vector `v` over the free axes and compares, for every remaining index `α` (an **A-row**),
//!
//! ```text
//! LHS(α) = Σ_f v[f] · out[α ⊕ f]          — |out| multiply-adds in all
//! RHS(α) = Σ_t X[α, t] · S[α, t]           — K per A-row
//! S[w, t] = Σ_f v[f] · W[w ⊕ f, t]         — the sketch: once per epoch for a weight
//! ```
//!
//! `LHS − RHS = Σ_f v[f] · E[α ⊕ f]` for the error `E = out − X·W`, so a nonzero row of `E` (modulo
//! `p`) passes with probability exactly `1/p` over `v` — and every A-row is compared, so one is
//! enough. A recompute costs `|A-rows| · |free| · K`; the check costs `|out| + |A-rows| · K`, and the
//! sketch `S` is `|free|` times smaller than `W`.
//!
//! A **routed** weight (`Gather` of an expert stack by a computed index) is gathered along batch
//! axes, which are never compressed: each A-row names one expert, whose own sketch is used — so the
//! sketch store holds one sketch per expert, and the index is the one the seat itself computed.
//! An **activation × activation** product is laid out the same way with `W = b`, its sketch built
//! from `b`'s value with a fresh vector at check time.

use misaka_palw_tir::{Dim, TensorType};

use crate::analysis::TirSideV1;
use crate::field::TirSketchModulusV1;

/// The layout of one `MatMul`'s check at one history length.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirCheckGeomV1 {
    pub side: TirSideV1,
    /// Output batch extents.
    pub eo: Vec<usize>,
    /// The activation's batch extents aligned to the output's (1 where it has no axis).
    pub ex: Vec<usize>,
    /// The weight's.
    pub ew: Vec<usize>,
    /// Output batch axes the weight is gathered along.
    pub routed: Vec<bool>,
    /// Output batch axes the check compresses.
    pub cb: Vec<bool>,
    pub m: usize,
    pub k: usize,
    pub n: usize,
    /// The weight's batch rank and its first output axis.
    wb_rank: usize,
    wb_first: usize,
    /// The activation's batch rank.
    xb_rank: usize,
    routed_rank: usize,
}

/// Visit every multi-index of `extents` in row-major order.
pub(crate) fn for_each_index(extents: &[usize], mut f: impl FnMut(&[usize])) {
    let total: usize = extents.iter().product();
    let mut idx = vec![0usize; extents.len()];
    for _ in 0..total {
        f(&idx);
        for d in (0..extents.len()).rev() {
            idx[d] += 1;
            if idx[d] < extents[d] {
                break;
            }
            idx[d] = 0;
        }
    }
}

fn row_major(extents: &[usize], idx: &[usize]) -> usize {
    extents.iter().zip(idx).fold(0usize, |acc, (e, i)| acc * e + i)
}

impl TirCheckGeomV1 {
    /// The layout of `MatMul(a, b)` with the weight on `side` (gathered along its first
    /// `routed_rank` axes), at history length `h`. Which axes are compressed is read from the
    /// DECLARED types, so it is the same at every position.
    pub fn new(side: TirSideV1, a: &TensorType, b: &TensorType, h: usize, routed_rank: usize) -> Self {
        let (sa, sb) = (a.resolve(h), b.resolve(h));
        let (ra, rb) = (sa.len(), sb.len());
        let nb = ra.max(rb) - 2;
        let align = |s: &[usize]| -> Vec<usize> {
            let nbs = s.len() - 2;
            (0..nb).map(|i| if i + nbs >= nb { s[i + nbs - nb] } else { 1 }).collect()
        };
        let one_decl = |t: &TensorType| -> Vec<bool> {
            let nbs = t.rank() - 2;
            (0..nb).map(|i| if i + nbs >= nb { t.shape[i + nbs - nb] == Dim::Fixed(1) } else { true }).collect()
        };
        let (ea, eb) = (align(&sa), align(&sb));
        let eo: Vec<usize> = (0..nb).map(|i| ea[i].max(eb[i])).collect();
        let (ex, ew, x_one, wb_rank, xb_rank) = match side {
            TirSideV1::Right => (ea, eb, one_decl(a), rb - 2, ra - 2),
            TirSideV1::Left => (eb, ea, one_decl(b), ra - 2, rb - 2),
        };
        let wb_first = nb - wb_rank;
        let routed: Vec<bool> = (0..nb).map(|i| i >= wb_first && i < wb_first + routed_rank).collect();
        let cb: Vec<bool> = (0..nb).map(|i| !routed[i] && x_one[i] && ew[i] > 1).collect();
        TirCheckGeomV1 {
            side,
            eo,
            ex,
            ew,
            routed,
            cb,
            m: sa[ra - 2],
            k: sa[ra - 1],
            n: sb[rb - 1],
            wb_rank,
            wb_first,
            xb_rank,
            routed_rank,
        }
    }

    fn nb(&self) -> usize {
        self.eo.len()
    }

    /// The weight's free matrix extent (compressed).
    pub fn free(&self) -> usize {
        match self.side {
            TirSideV1::Right => self.n,
            TirSideV1::Left => self.m,
        }
    }

    /// The activation's free matrix extent (one A-row each).
    pub fn rows(&self) -> usize {
        match self.side {
            TirSideV1::Right => self.m,
            TirSideV1::Left => self.n,
        }
    }

    fn cb_extents(&self) -> Vec<usize> {
        (0..self.nb()).filter(|i| self.cb[*i]).map(|i| self.eo[i]).collect()
    }

    /// Entries of the compression vector `v`: the compressed batch extents times the free extent.
    pub fn v_len(&self) -> usize {
        self.cb_extents().iter().product::<usize>() * self.free()
    }

    /// The weight's batch axes a sketch keeps (non-routed, not compressed), as output axes.
    fn kept_axes(&self) -> Vec<usize> {
        (self.wb_first + self.routed_rank..self.nb()).filter(|i| !self.cb[*i]).collect()
    }

    /// Entries of one expert's sketch: the kept batch extents times `K`.
    pub fn s_len(&self) -> usize {
        self.kept_axes().iter().map(|i| self.ew[*i]).product::<usize>() * self.k
    }

    /// A-rows: the uncompressed output batch extents times the activation's free extent.
    pub fn a_rows(&self) -> usize {
        (0..self.nb()).filter(|i| !self.cb[*i]).map(|i| self.eo[i]).product::<usize>() * self.rows()
    }

    /// Elements of the weight body one expert's sketch is built from.
    pub fn body_len(&self) -> usize {
        self.body_batch().iter().product::<usize>() * self.k * self.free()
    }

    fn body_batch(&self) -> Vec<usize> {
        (self.wb_first + self.routed_rank..self.nb()).map(|i| self.ew[i]).collect()
    }

    /// **One expert's sketch** `S[w, t] = Σ_f v[f] · W[w ⊕ f, t]` from its body — the weight with the
    /// gathered axes removed, row-major (`[batch…, K, N]` on the right, `[batch…, M, K]` on the left).
    pub fn sketch(&self, body: &[i128], v: &[u64], md: TirSketchModulusV1) -> Vec<u64> {
        debug_assert_eq!(body.len(), self.body_len());
        debug_assert_eq!(v.len(), self.v_len());
        let (k, free) = (self.k, self.free());
        let mut s = vec![0u64; self.s_len()];
        let body_axes: Vec<usize> = (self.wb_first + self.routed_rank..self.nb()).collect();
        let cb_ext = self.cb_extents();
        let kept_ext: Vec<usize> = self.kept_axes().iter().map(|i| self.ew[*i]).collect();
        let mut cb_idx = Vec::with_capacity(cb_ext.len());
        let mut kept_idx = Vec::with_capacity(kept_ext.len());
        let mut row = vec![0u64; free];
        let mut wrow = vec![0u64; k];
        let bb = self.body_batch();
        for_each_index(&bb, |wb| {
            cb_idx.clear();
            kept_idx.clear();
            for (j, &i) in body_axes.iter().enumerate() {
                if self.cb[i] {
                    cb_idx.push(wb[j]);
                } else {
                    kept_idx.push(wb[j]);
                }
            }
            let vbase = row_major(&cb_ext, &cb_idx) * free;
            let sbase = row_major(&kept_ext, &kept_idx) * k;
            let off = row_major(&bb, wb) * k * free;
            let vv = &v[vbase..vbase + free];
            match self.side {
                TirSideV1::Right => {
                    // W[w, t, c]: S[t] += Σ_c v[c]·W[t, c] — one dot per row t.
                    for t in 0..k {
                        for (c, x) in row.iter_mut().enumerate() {
                            *x = md.reduce_i128(body[off + t * free + c]);
                        }
                        s[sbase + t] = md.add(s[sbase + t], md.dot(&row, vv));
                    }
                }
                TirSideV1::Left => {
                    // W[w, r, t]: S[t] += v[r]·W[r, t] — one scaled row per r.
                    for (r, vr) in vv.iter().enumerate() {
                        for (t, x) in wrow.iter_mut().enumerate() {
                            *x = md.reduce_i128(body[off + r * k + t]);
                        }
                        for t in 0..k {
                            s[sbase + t] = md.add(s[sbase + t], md.mul(*vr, wrow[t]));
                        }
                    }
                }
            }
        });
        s
    }

    /// The A-rows in check order: every uncompressed batch index (compressed coordinates 0), then
    /// the activation's free index.
    fn for_each_a_row(&self, mut f: impl FnMut(&[usize], usize)) {
        let ext: Vec<usize> = (0..self.nb()).map(|i| if self.cb[i] { 1 } else { self.eo[i] }).collect();
        let rows = self.rows();
        for_each_index(&ext, |beta| {
            for x in 0..rows {
                f(beta, x);
            }
        });
    }

    /// **The left-hand sides** `Σ_f v[f] · out[α ⊕ f]`, one per A-row.
    pub fn lhs(&self, out: &[i128], v: &[u64], md: TirSketchModulusV1) -> Vec<u64> {
        let (m, n, free) = (self.m, self.n, self.free());
        let cb_ext = self.cb_extents();
        let comp: Vec<usize> = (0..self.nb()).map(|i| if self.cb[i] { self.eo[i] } else { 1 }).collect();
        let mut res = Vec::with_capacity(self.a_rows());
        let mut beta = vec![0usize; self.nb()];
        let mut cbi = Vec::with_capacity(cb_ext.len());
        let mut buf = vec![0u64; free];
        self.for_each_a_row(|ba, x| {
            let mut acc = 0u64;
            for_each_index(&comp, |bc| {
                cbi.clear();
                for (i, slot) in beta.iter_mut().enumerate() {
                    *slot = ba[i] + bc[i];
                    if self.cb[i] {
                        cbi.push(bc[i]);
                    }
                }
                let ob = row_major(&self.eo, &beta) * m * n;
                for (f, slot) in buf.iter_mut().enumerate() {
                    let (r, c) = match self.side {
                        TirSideV1::Right => (x, f),
                        TirSideV1::Left => (f, x),
                    };
                    *slot = md.reduce_i128(out[ob + r * n + c]);
                }
                let vbase = row_major(&cb_ext, &cbi) * free;
                acc = md.add(acc, md.dot(&buf, &v[vbase..vbase + free]));
            });
            res.push(acc);
        });
        res
    }

    /// **The right-hand sides** `Σ_t X[α, t] · S[α, t]`, one per A-row; `sketch_of(β)` gives the
    /// sketch of the expert batch index `β` names (`None` if it names none).
    pub fn rhs<'s>(
        &self,
        x: &[i128],
        sketch_of: &mut dyn FnMut(&[usize]) -> Option<&'s [u64]>,
        md: TirSketchModulusV1,
    ) -> Option<Vec<u64>> {
        let (m, n, k) = (self.m, self.n, self.k);
        let kept = self.kept_axes();
        let kept_ext: Vec<usize> = kept.iter().map(|i| self.ew[*i]).collect();
        let x_first = self.nb() - self.xb_rank;
        let xb_ext: Vec<usize> = (x_first..self.nb()).map(|i| self.ex[i]).collect();
        let mut res = Vec::with_capacity(self.a_rows());
        let mut xs = vec![0u64; k];
        let mut kept_idx = Vec::with_capacity(kept.len());
        let mut xb_idx = Vec::with_capacity(xb_ext.len());
        let mut ok = true;
        self.for_each_a_row(|beta, xi| {
            if !ok {
                return;
            }
            let Some(s) = sketch_of(beta) else {
                ok = false;
                return;
            };
            kept_idx.clear();
            kept_idx.extend(kept.iter().map(|i| if self.ew[*i] == 1 { 0 } else { beta[*i] }));
            let sb = row_major(&kept_ext, &kept_idx) * k;
            xb_idx.clear();
            xb_idx.extend((x_first..self.nb()).map(|i| if self.ex[i] == 1 { 0 } else { beta[i] }));
            let xbase = row_major(&xb_ext, &xb_idx);
            match self.side {
                TirSideV1::Right => {
                    // X = a [.., M, K]: row xi, contiguous over t.
                    let off = xbase * m * k + xi * k;
                    for (t, slot) in xs.iter_mut().enumerate() {
                        *slot = md.reduce_i128(x[off + t]);
                    }
                }
                TirSideV1::Left => {
                    // X = b [.., K, N]: column xi, strided over t.
                    let off = xbase * k * n + xi;
                    for (t, slot) in xs.iter_mut().enumerate() {
                        *slot = md.reduce_i128(x[off + t * n]);
                    }
                }
            }
            res.push(md.dot(&xs, &s[sb..sb + k]));
        });
        ok.then_some(res)
    }

    /// For a routed weight: the gather index's coordinates of output batch index `beta` (its first
    /// `routed_rank` weight axes).
    pub fn routed_coords<'b>(&self, beta: &'b [usize]) -> &'b [usize] {
        &beta[self.wb_first..self.wb_first + self.routed_rank]
    }

    /// Multiply-adds of a recompute: `|batch| · M · N · K`.
    pub fn recompute_macs(&self) -> u64 {
        self.eo.iter().product::<usize>() as u64 * (self.m * self.n * self.k) as u64
    }

    /// Multiply-adds of the check: the left sides read every output once, the right sides `K` per
    /// A-row.
    pub fn check_terms(&self) -> u64 {
        let out: usize = self.eo.iter().product::<usize>() * self.m * self.n;
        (out + self.a_rows() * self.k) as u64
    }

    /// The activation operand's batch rank (for its strides).
    pub fn x_batch_rank(&self) -> usize {
        self.xb_rank
    }

    /// The weight operand's batch rank.
    pub fn w_batch_rank(&self) -> usize {
        self.wb_rank
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_tir::DType;

    fn t(shape: &[u32]) -> TensorType {
        TensorType::fixed(DType::I32, shape)
    }

    /// Exact `out = a·b` with numpy batch broadcasting, for the tests.
    fn matmul(sa: &[usize], a: &[i128], sb: &[usize], b: &[i128]) -> (Vec<usize>, Vec<i128>) {
        let nb = sa.len().max(sb.len()) - 2;
        let align =
            |s: &[usize]| -> Vec<usize> { (0..nb).map(|i| if i + s.len() - 2 >= nb { s[i + s.len() - 2 - nb] } else { 1 }).collect() };
        let (ea, eb) = (align(sa), align(sb));
        let eo: Vec<usize> = (0..nb).map(|i| ea[i].max(eb[i])).collect();
        let (m, k, n) = (sa[sa.len() - 2], sa[sa.len() - 1], sb[sb.len() - 1]);
        let mut out = Vec::new();
        for_each_index(&eo, |beta| {
            let ia: Vec<usize> = (0..nb).map(|i| if ea[i] == 1 { 0 } else { beta[i] }).collect();
            let ib: Vec<usize> = (0..nb).map(|i| if eb[i] == 1 { 0 } else { beta[i] }).collect();
            let (oa, ob) = (row_major(&ea, &ia) * m * k, row_major(&eb, &ib) * k * n);
            for r in 0..m {
                for c in 0..n {
                    out.push((0..k).map(|tt| a[oa + r * k + tt] * b[ob + tt * n + c]).sum());
                }
            }
        });
        let mut so = eo;
        so.extend([m, n]);
        (so, out)
    }

    fn vals(n: usize, seed: i128) -> Vec<i128> {
        (0..n as i128).map(|i| ((i * 7_919 + seed * 104_729) % 2001) - 1000).collect()
    }

    /// Every layout the lowerers emit, both sides: LHS = RHS on the honest product, and a change
    /// of any one output element breaks exactly its own A-row.
    #[test]
    fn honest_products_balance_and_one_changed_element_breaks_its_row() {
        let md = TirSketchModulusV1::P61;
        let cases: Vec<(TirSideV1, Vec<u32>, Vec<u32>)> = vec![
            (TirSideV1::Left, vec![6, 4], vec![4, 1]),          // dense A16: W [out, in] · x [in, 1]
            (TirSideV1::Right, vec![3, 4], vec![4, 5]),         // x [M, K] · W [K, N]
            (TirSideV1::Left, vec![6, 3, 1, 4], vec![3, 4, 1]), // q36 grouped: [out, G, 1, gs] · [G, gs, 1]
            (TirSideV1::Left, vec![2, 5, 4], vec![4, 1]),       // routed experts: [k, rows, cols] · [cols, 1]
            (TirSideV1::Left, vec![2, 5, 4], vec![2, 4, 1]),    // routed down: [k, d, f] · [k, f, 1]
            (TirSideV1::Right, vec![2, 3, 4], vec![2, 4, 6]),   // batched, nothing broadcast
            (TirSideV1::Right, vec![1, 3, 4], vec![2, 4, 6]),   // the activation broadcast over a weight batch
        ];
        for (side, sa, sb) in cases {
            let (ta, tb) = (t(&sa), t(&sb));
            let (ua, ub): (Vec<usize>, Vec<usize>) =
                (sa.iter().map(|d| *d as usize).collect(), sb.iter().map(|d| *d as usize).collect());
            let (a, b) = (vals(ua.iter().product(), 1), vals(ub.iter().product(), 2));
            let (_, out) = matmul(&ua, &a, &ub, &b);
            let g = TirCheckGeomV1::new(side, &ta, &tb, 1, 0);
            let v: Vec<u64> = (0..g.v_len() as u64).map(|i| md.reduce_u128((i as u128 + 3) * 0xD1B5_4A32_D192_ED03)).collect();
            let (w, x) = match side {
                TirSideV1::Right => (&b, &a),
                TirSideV1::Left => (&a, &b),
            };
            let s = g.sketch(w, &v, md);
            let rhs = g.rhs(x, &mut |_| Some(&s[..]), md).unwrap();
            assert_eq!(g.lhs(&out, &v, md), rhs, "{side:?} {sa:?}·{sb:?}");
            assert!(g.check_terms() < g.recompute_macs() || g.k * g.free() <= 4, "{sa:?}·{sb:?}: the check is cheaper");
            for e in 0..out.len() {
                let mut bad = out.clone();
                bad[e] += 1;
                let l = g.lhs(&bad, &v, md);
                assert_eq!(l.iter().zip(&rhs).filter(|(p, q)| p != q).count(), 1, "{side:?} {sa:?}: element {e}");
            }
        }
    }
}
