//! **`gdn_step_q36`** (`tir_library_v1`, legacy: `q36_gdn_step`): one position of the gated delta
//! rule over a `Fixed` state `S[heads, d_v, d_k]` — the decay, the read narrowing, the delta, the
//! rank-one write with its shift, the state's saturation, and the output narrowing.
//!
//! The template is sixty-odd nodes of which a dozen run over the whole state (`heads · d_v · d_k`
//! elements, several in `i128`). The implementation walks the state once, a head per task: each row
//! of the decayed state, its read (the order-free sum), the delta, the written row saturated into the
//! state's range — stored straight into the pending state, which is the template's `StateWrite` —
//! and the output (the order-free sum over the written row). Every lossy site (the two `>> 24`, the
//! three narrowings, the clamps, the shifted write, the saturation) is computed on exactly the value
//! the template computes it on:
//!
//! * `S · decay` is `I64` in the template and fail-free, so its exact product is an `i64`;
//! * the write's left shift is `clamp_i64(prod · 2^l)`, which is `i64::saturating_mul` exactly;
//! * `S1 + write` is `I128` and then saturated into the state's range `[lo, hi] ⊂ i64`, which is
//!   `clamp(saturating_add)` exactly (a sum past `i64` saturates past `[lo, hi]` too);
//! * the two exact sums run in wrapping `i64`, which the plan's `Fast64` proved exact in every order.

use misaka_palw_tir::builder::BlockBuilder;
use misaka_palw_tir::program::{Node, StateDecl, StateKind};
use misaka_palw_tir::{DType, Dim, Ref, Rounding, TensorType, TirError, TirErrorKind, TirResult};
use rayon::prelude::*;

use super::{Bound, FusedIo, FusedKernelV1, Variant, narrow, pow2_of, values};
use crate::elem::{Buf, Elem};
use crate::scalar::shr_round_i64;

pub struct GdnStep;

const SMAX: i64 = i32::MAX as i64;

/// A per-row operand as `i64` lanes: borrowed when it is stored `i16`/`i32`, else converted.
enum Lanes<'a> {
    I16(&'a [i16]),
    I32(&'a [i32]),
    Wide(Vec<i64>),
}

impl Lanes<'_> {
    fn of<'a>(op: &crate::kernels::Opd<'a>) -> Lanes<'a> {
        if let Some(v) = op.contiguous::<i16>() {
            return Lanes::I16(v);
        }
        if let Some(v) = op.contiguous::<i32>() {
            return Lanes::I32(v);
        }
        Lanes::Wide(values(op).into_iter().map(|v| v as i64).collect())
    }
    #[inline(always)]
    fn at(&self, i: usize) -> i64 {
        match self {
            Lanes::I16(v) => v[i] as i64,
            Lanes::I32(v) => v[i] as i64,
            Lanes::Wide(v) => v[i],
        }
    }
}

impl FusedKernelV1 for GdnStep {
    fn name(&self) -> &'static str {
        "gdn_step_q36"
    }

    fn variants(&self) -> Vec<Variant> {
        let v = |dtype: DType, shape: &[u32]| TensorType::fixed(dtype, shape);
        let per_head = || v(DType::I64, &[2]);
        let mut probe = vec![v(DType::I32, &[2, 4]), v(DType::I32, &[2, 4]), v(DType::I32, &[2, 4])];
        probe.extend([v(DType::I32, &[2]), v(DType::I32, &[2])]);
        probe.extend((0..6).map(|_| per_head()));
        probe.push(v(DType::I32, &[2]));
        probe.extend((0..3).map(|_| per_head()));
        vec![Variant {
            bound: Bound::GdnStep,
            probe,
            states: vec![StateDecl {
                name: "fused.gdn".into(),
                kind: StateKind::Fixed { lo: -SMAX, hi: SMAX },
                dtype: DType::I32,
                shape: vec![2, 4, 4],
                per_layer: true,
            }],
        }]
    }

    fn derive(&self, _variant: &Bound, holes: &[TensorType], _output: &Node) -> Option<Bound> {
        // `k`, `q` `[h, d_k]`, `v` `[h, d_v]`, and every per-head operand `h` elements (the state's
        // `[h, d_v, d_k]` is the template's own reshape targets; the exact comparison checks it).
        let (k, v, q) = (super::static_shape(&holes[0])?, super::static_shape(&holes[1])?, super::static_shape(&holes[2])?);
        let h = *k.first()?;
        let per_head = |i: usize| super::static_shape(&holes[i]).is_some_and(|s| s.iter().product::<usize>() == h);
        (k.len() == 2 && q == k && v.len() == 2 && v[0] == h && (3..15).all(per_head)).then_some(Bound::GdnStep)
    }

    fn emit(&self, b: &mut BlockBuilder<'_>, h: &[Ref], _bound: &Bound) -> Ref {
        b.gdn_step_q36(0, h[0], h[1], h[2], h[3], h[4], (h[5], h[6], h[7]), (h[8], h[9], h[10]), h[11], (h[12], h[13], h[14]))
    }

    fn domain(&self, _bound: &Bound, holes: &[TensorType], out: &TensorType) -> bool {
        // The template's reshapes fixed every shape; what the implementation adds is static shapes
        // and an `i32` output.
        holes.iter().all(|t| t.shape.iter().all(|d| matches!(d, Dim::Fixed(_)))) && out.dtype == DType::I32
    }

    fn run(&self, _bound: &Bound, io: &mut FusedIo<'_, '_>) -> TirResult<()> {
        let bad = |what: &str| TirError::new(TirErrorKind::Shape, format!("gdn_step: {what}"));
        let [h, dv] = [io.out_shape[0], io.out_shape[1]];
        let s_now = *io.states.first().ok_or_else(|| bad("no state"))?;
        let s_len = s_now.len();
        if h == 0 || dv == 0 || s_len % (h * dv) != 0 {
            return Err(bad("state shape"));
        }
        let dk = s_len / (h * dv);
        let (s_lo, s_hi) = io.state_ranges.first().copied().ok_or_else(|| bad("no state range"))?;
        let state_dtype = io.state_next.first().map_or(DType::I32, Buf::dtype);
        // The state as it is held (`i32`, the template's), or converted for any other width.
        let converted: Vec<i32>;
        let s_now: &[i32] = match <i32 as Elem>::slice_of(s_now) {
            Some(v) => v,
            None => {
                converted = s_now.to_i128s().into_iter().map(|v| v as i32).collect();
                &converted
            }
        };
        let (k, v, q) = (Lanes::of(&io.holes[0]), Lanes::of(&io.holes[1]), Lanes::of(&io.holes[2]));
        let hv = |i: usize| values(&io.holes[i]);
        let (decay, beta) = (hv(3), hv(4));
        let (rm, rd, rz) = (hv(5), hv(6), hv(7));
        let (dm, dd, dz) = (hv(8), hv(9), hv(10));
        let ws = hv(11);
        let (om, od, oz) = (hv(12), hv(13), hv(14));
        // The pending state's own buffer, reused (it is the instance's double buffer, `s_len` of
        // the state's dtype): a fresh one per step would fault its pages in every position.
        let pending = io.state_next.first_mut().ok_or_else(|| bad("no pending state"))?;
        let mut next: Vec<i32> = match std::mem::take(pending) {
            Buf::I32(mut v) => {
                v.resize(s_len, 0);
                v
            }
            _ => vec![0i32; s_len],
        };
        let mut out = vec![0i128; h * dv];
        let head = |hh: usize, next: &mut [i32], out: &mut [i128]| {
            let dec = decay[hh] as i64;
            let bt = beta[hh];
            let base = hh * dk;
            let wsh = ws[hh];
            let left_on = wsh >= 0;
            let lp = pow2_of(wsh.clamp(0, 20)) as i64;
            let rp_shift = (0 - wsh).clamp(0, 62) as u32;
            // This head's key and query rows as plain `i64` lanes, once, so the loops below vectorise.
            let kh: Vec<i64> = (0..dk).map(|i| k.at(base + i)).collect();
            let qh: Vec<i64> = (0..dk).map(|i| q.at(base + i)).collect();
            let mut s1 = vec![0i64; dk];
            for j in 0..dv {
                let row = (hh * dv + j) * dk;
                // 1. S1 = clamp(HAFZ(S · decay / 2^24), ±(2^31 − 1)); 2. the read's exact sum.
                for (s, now) in s1.iter_mut().zip(&s_now[row..row + dk]) {
                    *s = shr_round_i64((*now as i64) * dec, 24, Rounding::HalfAwayFromZero).clamp(-SMAX, SMAX);
                }
                let acc = s1.iter().zip(&kh).fold(0i64, |a, (s, k)| a.wrapping_add(s.wrapping_mul(*k)));
                let w = narrow(acc as i128, rm[hh], rd[hh], Some(rz[hh]), i64::MIN as i128, i64::MAX as i128);
                // 3. u = narrow_delta(HAFZ(sat64(sat64(v − w) · β) / 2^24)).
                let diff = (v.at(hh * dv + j) as i128 - w).clamp(i64::MIN as i128, i64::MAX as i128);
                let db = (diff * bt).clamp(i64::MIN as i128, i64::MAX as i128) as i64;
                let scaled = shr_round_i64(db, 24, Rounding::HalfAwayFromZero);
                let u = narrow(scaled as i128, dm[hh], dd[hh], Some(dz[hh]), -((1 << 24) - 1), (1 << 24) - 1) as i64;
                // 4. The rank-one write, saturated into the state (the shift's direction is the
                // head's, so the branch is outside the row); 5. the output's exact sum.
                let nrow = &mut next[j * dk..(j + 1) * dk];
                let write = |prod: i64| {
                    if left_on { prod.saturating_mul(lp) } else { shr_round_i64(prod, rp_shift, Rounding::HalfAwayFromZero) }
                };
                for ((n, s), k) in nrow.iter_mut().zip(&s1).zip(&kh) {
                    *n = s.saturating_add(write(u * k)).clamp(s_lo, s_hi) as i32;
                }
                let acc_o = nrow.iter().zip(&qh).fold(0i64, |a, (s2, q)| a.wrapping_add((*s2 as i64).wrapping_mul(*q)));
                out[j] = narrow(acc_o as i128, om[hh], od[hh], Some(oz[hh]), i32::MIN as i128, i32::MAX as i128);
            }
        };
        let per_head_state = dv * dk;
        if s_len >= 1 << 16 {
            next.par_chunks_mut(per_head_state).zip(out.par_chunks_mut(dv)).enumerate().for_each(|(hh, (n, o))| head(hh, n, o));
        } else {
            for (hh, (n, o)) in next.chunks_mut(per_head_state).zip(out.chunks_mut(dv)).enumerate() {
                head(hh, n, o);
            }
        }
        let pending = io.state_next.first_mut().ok_or_else(|| bad("no pending state"))?;
        *pending = match state_dtype {
            DType::I32 => Buf::I32(next),
            other => Buf::from_i128s(other, &next.iter().map(|v| *v as i128).collect::<Vec<_>>()),
        };
        super::store(&mut out, io.out, io.out_store, io.fault, i32::MAX as i128);
        Ok(())
    }
}
