//! Hand-checkable kernel tests: each case is small enough to verify on paper, or compares a
//! recurrence against the closed form it unrolls to.

use super::*;
use crate::rope::{AlibiSpec, RopeFreqs, alibi_slopes_bloom};
use crate::spec::{Gain, GroupRouting, NormKind};

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol * (1.0 + b.abs())
}

#[test]
fn erf_matches_reference_values() {
    for (x, want) in [
        (0.0, 0.0),
        (0.5, 0.520_499_877_813_046_5),
        (1.0, 0.842_700_792_949_714_9),
        (2.0, 0.995_322_265_018_952_7),
        (3.5, 0.999_999_256_901_627_7),
    ] {
        assert!(close(erf(x), want, 1e-12), "erf({x}) = {} vs {want}", erf(x));
        assert!(close(erf(-x), -want, 1e-12));
    }
    // gelu(1) exact = 0.8413447460685429; the tanh form differs in the 4th digit.
    assert!(close(act(Act::Gelu, 1.0) as f64, 0.841_344_746_068_542_9, 1e-7));
    assert!(close(act(Act::GeluTanh, 1.0) as f64, 0.841_191_990_607_477_2, 1e-7));
    // torch softplus switches to the identity above 20.
    assert_eq!(act(Act::Softplus, 30.0), 30.0);
    assert!(close(act(Act::Softplus, 0.0) as f64, 2f64.ln(), 1e-7));
}

#[test]
fn rope_rotates_pairs_by_position_times_frequency() {
    // One head, dim 4, theta 1: frequencies [1, 1/1^0.5 = 1]; use theta 100 → [1, 0.1].
    let f = RopeFreqs::plain(100.0, 4);
    let (c, s) = f.cos_sin(2);
    let x = [1.0f32, 0.0, 0.0, 0.0];
    // Half style pairs (0,2) and (1,3): x0' = x0·cos(2), x2' = x0·sin(2).
    let y = rope(&x, 1, 4, 4, 0, RopeStyle::Half, &c, &s);
    assert!(close(y[0] as f64, 2f64.cos(), 1e-6) && close(y[2] as f64, 2f64.sin(), 1e-6) && y[1] == 0.0);
    // Interleaved pairs (0,1),(2,3).
    let y = rope(&x, 1, 4, 4, 0, RopeStyle::Interleaved, &c, &s);
    assert!(close(y[0] as f64, 2f64.cos(), 1e-6) && close(y[1] as f64, 2f64.sin(), 1e-6));
}

#[test]
fn half_and_interleaved_rope_are_one_rotation_under_a_permutation() {
    let f = RopeFreqs::plain(10000.0, 8);
    let (c, s) = f.cos_sin(7);
    let x: Vec<f32> = (0..8).map(|i| (i as f32 * 0.37).sin()).collect();
    // interleaved index 2i ↔ half index i, 2i+1 ↔ i + 4.
    let to_half = |v: &[f32]| -> Vec<f32> { (0..8).map(|j| if j < 4 { v[2 * j] } else { v[2 * (j - 4) + 1] }).collect() };
    let a = to_half(&rope(&x, 1, 8, 8, 0, RopeStyle::Interleaved, &c, &s));
    let b = rope(&to_half(&x), 1, 8, 8, 0, RopeStyle::Half, &c, &s);
    for (p, q) in a.iter().zip(&b) {
        assert!((p - q).abs() < 1e-6);
    }
}

#[test]
fn partial_rope_and_offset_leave_the_other_dims_alone() {
    let f = RopeFreqs::plain(10000.0, 2);
    let (c, s) = f.cos_sin(3);
    let x = [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0];
    // MLA-style: 2 heads of 3 dims, rotate dims [1, 3) of each head.
    let y = rope(&x, 2, 3, 2, 1, RopeStyle::Half, &c, &s);
    assert_eq!((y[0], y[3]), (1.0, 4.0));
    assert!(close(y[1] as f64, 2.0 * 3f64.cos() - 3.0 * 3f64.sin(), 1e-6));
}

fn kv(rows: &[&[f32]]) -> Vec<Vec<f32>> {
    rows.iter().map(|r| r.to_vec()).collect()
}

const SH1: AttnShape = AttnShape { heads: 1, kv_heads: 1, head_dim: 1, v_head_dim: 1 };

#[test]
fn a_sliding_window_sees_exactly_the_last_w_rows() {
    // Row 0 has an enormous key: visible, it dominates; outside the window, it vanishes.
    let keys = kv(&[&[1000.0], &[0.0], &[0.0]]);
    let vals = kv(&[&[7.0], &[1.0], &[3.0]]);
    let (full, _, _) = attention(&[1.0], &keys, &vals, SH1, 1.0, None, None, None, None, false);
    assert!(close(full[0] as f64, 7.0, 1e-9));
    let (win, _, _) = attention(&[1.0], &keys, &vals, SH1, 1.0, None, Some(2), None, None, false);
    assert!(close(win[0] as f64, 2.0, 1e-7), "mean of the last two values");
}

#[test]
fn gqa_groups_query_heads_onto_kv_heads_like_repeat_kv() {
    // 4 query heads, 2 kv heads: heads 0,1 read kv 0; heads 2,3 read kv 1.
    let sh = AttnShape { heads: 4, kv_heads: 2, head_dim: 1, v_head_dim: 1 };
    let keys = kv(&[&[0.0, 0.0]]);
    let vals = kv(&[&[10.0, 20.0]]);
    let (o, _, _) = attention(&[1.0; 4], &keys, &vals, sh, 1.0, None, None, None, None, false);
    assert_eq!(o, vec![10.0, 10.0, 20.0, 20.0]);
}

#[test]
fn softcap_sinks_and_alibi() {
    // Soft-cap: tanh(s/c)·c.
    let keys = kv(&[&[5.0], &[0.0]]);
    let vals = kv(&[&[1.0], &[0.0]]);
    let (_, sc, _) = attention(&[1.0], &keys, &vals, SH1, 1.0, Some(5.0), None, None, None, true);
    assert!(close(sc[0] as f64, 5.0 * 1f64.tanh(), 1e-6));
    // A sink of logit 0 with two zero scores takes 1/3 of the mass.
    let keys = kv(&[&[0.0], &[0.0]]);
    let vals = kv(&[&[3.0], &[3.0]]);
    let (o, _, p) = attention(&[1.0], &keys, &vals, SH1, 1.0, None, None, None, Some(&[0.0]), true);
    assert!(close(o[0] as f64, 2.0, 1e-6) && close(p[0] as f64, 1.0 / 3.0, 1e-6));
    // BLOOM ALiBi with zero scores: p_j ∝ exp(slope·j).
    let slope = alibi_slopes_bloom(1)[0];
    let al = AlibiSpec { slopes: vec![slope], scaled_by_softmax_scale: false, bf16_bias: false };
    let (_, _, p) = attention(&[0.0], &kv(&[&[0.0], &[0.0]]), &kv(&[&[0.0], &[0.0]]), SH1, 0.5, None, None, Some(&al), None, true);
    let want = slope.exp() / (1.0 + slope.exp());
    assert!(close(p[1] as f64, want, 1e-6));
    // Falcon: the bias is scaled with the scores and rounded through bfloat16.
    let al = AlibiSpec { slopes: vec![0.3], scaled_by_softmax_scale: true, bf16_bias: true };
    let (_, sc, _) = attention(
        &[0.0],
        &kv(&[&[0.0], &[0.0], &[0.0], &[0.0]]),
        &kv(&[&[0.0], &[0.0], &[0.0], &[0.0]]),
        SH1,
        0.5,
        None,
        None,
        Some(&al),
        None,
        true,
    );
    let b3 = bf16_round(bf16_round(0.3) * 3.0) as f64 * 0.5;
    assert_eq!(sc[3] as f64, b3 as f32 as f64);
    assert_ne!(b3, 0.3 * 3.0 * 0.5, "bf16 rounding is visible");
}

#[test]
fn top_k_breaks_ties_to_the_lowest_index_and_returns_index_order() {
    assert_eq!(top_k_indices(&[1.0, 3.0, 3.0, 2.0], 2), vec![1, 2]);
    assert_eq!(top_k_indices(&[5.0, 5.0, 5.0], 2), vec![0, 1]);
    assert_eq!(top_k_indices(&[0.1, 0.9, 0.5], 2), vec![1, 2]);
    assert_eq!(top_k_indices(&[2.0, 2.0, 9.0, 2.0], 3), vec![0, 1, 2]);
}

fn router(scoring: Scoring, normalize: bool) -> RouterSpec {
    RouterSpec { scoring, linear_bias: false, selection_bias: false, groups: None, normalize, norm_eps: 0.0, scale: 1.0 }
}

#[test]
fn softmax_router_renormalises_the_selected_probabilities() {
    // p ∝ [1, 2, 1, 4]; top-2 = experts 1 and 3; renormalised weights 2/6 and 4/6.
    let logits = [0.0f32, 2f32.ln(), 0.0, 4f32.ln()];
    let (idx, w) = route(&logits, None, &router(Scoring::Softmax, true), 4, 2);
    assert_eq!(idx, vec![1, 3]);
    assert!(close(w[0] as f64, 2.0 / 6.0, 1e-6) && close(w[1] as f64, 4.0 / 6.0, 1e-6));
    // Without renormalisation the weights are the global probabilities.
    let (_, w) = route(&logits, None, &router(Scoring::Softmax, false), 4, 2);
    assert!(close(w[1] as f64, 4.0 / 8.0, 1e-6));
    // gpt-oss / GraniteMoE: softmax over the selected logits only.
    let (idx, w) = route(&[1.0, 2.0, 3.0], None, &router(Scoring::TopKThenSoftmax, false), 3, 2);
    assert_eq!(idx, vec![1, 2]);
    assert!(close(w[1] as f64, 1.0 / (1.0 + (-1f64).exp()), 1e-6));
}

#[test]
fn deepseek_v3_group_limited_routing_masks_whole_groups() {
    // 4 experts in 2 groups; expert 0 is the single best, but group 1's top-2 sum is larger.
    let mut r = router(Scoring::Sigmoid, true);
    r.selection_bias = true;
    r.groups = Some(GroupRouting { n_group: 2, topk_group: 1, score: GroupScore::Top2Sum });
    r.norm_eps = 1e-20;
    r.scale = 2.5;
    let logits = [3.0f32, -3.0, 2.0, 2.0];
    let (idx, w) = route(&logits, Some(&[0.0; 4]), &r, 4, 2);
    assert_eq!(idx, vec![2, 3], "group 0 (experts 0,1) loses on its top-2 sum");
    assert!(close(w[0] as f64 + w[1] as f64, 2.5, 1e-6), "renormalised then × routed_scaling_factor");
    // The selection bias moves the choice but NOT the weights.
    let (idx, w) = route(&logits, Some(&[0.0, 0.0, 5.0, -5.0]), &r, 4, 2);
    assert_eq!(idx, vec![2, 3]);
    assert!(close(w[0] as f64, 2.5 * 0.5, 1e-6), "weights are the unbiased sigmoids, equal here");
}

#[test]
fn deepseek_v2_group_limited_greedy_fills_masked_experts_with_zero() {
    let mut r = router(Scoring::Softmax, false);
    r.groups = Some(GroupRouting { n_group: 2, topk_group: 1, score: GroupScore::Max });
    r.scale = 16.0;
    // Group 1 holds the max; top-3 needs a third expert, which is a masked one at weight 0.
    let (idx, w) = route(&[0.0, 0.0, 5.0, 4.0], None, &r, 4, 3);
    assert_eq!(idx, vec![0, 2, 3]);
    assert_eq!(w[0], 0.0);
}

#[test]
fn gated_delta_single_step_is_beta_v_times_k_dot_q() {
    // Fresh state: S = k (β v)ᵀ, o = Sᵀ q · scale = β v (k·q) scale.
    let (dk, dv) = (2, 1);
    let mut s = vec![0f32; dk * dv];
    let q = [1.0f32, 2.0];
    let k = [0.5f32, 0.25];
    let o = gated_delta(&q, &k, &[4.0], &[0.0], &[0.5], &mut s, 1, 1, dk, dv, HeadMap::Group, 0.5);
    assert!(close(o[0] as f64, 0.5 * 4.0 * (0.5 + 0.5) * 0.5, 1e-7));
    // A second step decays, reads back the stored value (delta rule) and corrects it.
    let o2 = gated_delta(&q, &k, &[4.0], &[(0.5f64).ln() as f32], &[1.0], &mut s, 1, 1, dk, dv, HeadMap::Group, 1.0);
    // S = 0.5·[1, 0.5]·… : after decay S = [0.5, 0.25]; kv_mem = 0.3125; δ = 4 − 0.3125.
    let s_after: Vec<f64> = vec![0.5 + 0.5 * 3.6875, 0.25 + 0.25 * 3.6875];
    assert!(close(o2[0] as f64, s_after[0] * 1.0 + s_after[1] * 2.0, 1e-6));
}

#[test]
fn gdn_value_heads_group_in_hf_and_tile_in_the_live_kernel() {
    // k_heads 2, v_heads 4, dk = dv = 1. Group: vh 0,1 → kh 0; vh 2,3 → kh 1. Tile: vh 0,2 → kh 0.
    let q = [1.0f32, 10.0];
    let k = [1.0f32, 1.0];
    let v = [1.0f32, 2.0, 3.0, 4.0];
    let g = [0.0f32; 4];
    let beta = [1.0f32; 4];
    let mut sg = vec![0f32; 4];
    let og = gated_delta(&q, &k, &v, &g, &beta, &mut sg, 2, 4, 1, 1, HeadMap::Group, 1.0);
    assert_eq!(og, vec![1.0, 2.0, 30.0, 40.0]);
    let mut st = vec![0f32; 4];
    let ot = gated_delta(&q, &k, &v, &g, &beta, &mut st, 2, 4, 1, 1, HeadMap::Tile, 1.0);
    assert_eq!(ot, vec![1.0, 20.0, 3.0, 40.0]);
    // Tile on value heads permuted to the group order equals Group: vh_t ↦ (vh % nk)·rep + vh / nk.
    let perm = |vh: usize| (vh % 2) * 2 + vh / 2;
    let vt: Vec<f32> = (0..4).map(|vh| v[perm(vh)]).collect();
    let mut st2 = vec![0f32; 4];
    let ot2 = gated_delta(&q, &k, &vt, &g, &beta, &mut st2, 2, 4, 1, 1, HeadMap::Tile, 1.0);
    for vh in 0..4 {
        assert_eq!(ot2[vh], og[perm(vh)]);
    }
}

#[test]
fn causal_conv_is_a_zero_padded_convolution_over_the_last_k_inputs() {
    let (ch, k) = (1, 3);
    let w = [1.0f32, 10.0, 100.0];
    let mut st = vec![0f32; (k - 1) * ch];
    let xs = [1.0f32, 2.0, 3.0, 4.0];
    let outs: Vec<f32> = xs.iter().map(|x| causal_conv(&[*x], &mut st, &w, None, ch, k, None)[0]).collect();
    // out_t = 1·x_{t−2} + 10·x_{t−1} + 100·x_t.
    assert_eq!(outs, vec![100.0, 210.0, 321.0, 432.0]);
}

#[test]
fn selective_scan_single_channel_hand_check() {
    let mut h = vec![0f32];
    // h = exp(dt·A)·h + dt·B·x; y = h·C + D·x.
    let y1 = selective_scan(&[2.0], &[0.5], &[3.0], &[4.0], &[-1.0], &[0.25], &mut h, 1, 1);
    assert!(close(y1[0] as f64, 3.0 * 4.0 + 0.5, 1e-6));
    let y2 = selective_scan(&[1.0], &[0.5], &[3.0], &[4.0], &[-1.0], &[0.25], &mut h, 1, 1);
    let h2 = (-0.5f64).exp() * 3.0 + 1.5;
    assert!(close(y2[0] as f64, h2 * 4.0 + 0.25, 1e-6));
}

#[test]
fn mamba2_recurrent_steps_equal_the_chunked_ssd_form() {
    // One group, 2 heads of P=2, N=3, T=6: y_t = Σ_{s≤t} (C_t·B_s) exp(A·Σ_{r=s+1..t} dt_r) dt_s x_s + D x_t.
    let (heads, p, groups, n, t) = (2usize, 2usize, 1usize, 3usize, 6usize);
    let a = [-0.7f32, -0.2];
    let d = [0.3f32, 1.1];
    let val = |i: usize, j: usize| ((i * 7 + j * 3) as f32 * 0.37).sin();
    let xs: Vec<Vec<f32>> = (0..t).map(|i| (0..heads * p).map(|j| val(i, j)).collect()).collect();
    let dts: Vec<Vec<f32>> = (0..t).map(|i| (0..heads).map(|j| 0.2 + 0.1 * val(i, j + 11).abs()).collect()).collect();
    let bs: Vec<Vec<f32>> = (0..t).map(|i| (0..groups * n).map(|j| val(i, j + 20)).collect()).collect();
    let cs: Vec<Vec<f32>> = (0..t).map(|i| (0..groups * n).map(|j| val(i, j + 30)).collect()).collect();
    let mut h = vec![0f32; heads * p * n];
    let rec: Vec<Vec<f32>> = (0..t).map(|i| ssd_step(&xs[i], &dts[i], &bs[i], &cs[i], &a, &d, &mut h, heads, p, groups, n)).collect();
    for ti in 0..t {
        for hh in 0..heads {
            for pp in 0..p {
                let mut y = 0f64;
                for s in 0..=ti {
                    let cb: f64 = (0..n).map(|k| cs[ti][k] as f64 * bs[s][k] as f64).sum();
                    let decay: f64 = ((s + 1)..=ti).map(|r| dts[r][hh] as f64 * a[hh] as f64).sum::<f64>().exp();
                    y += cb * decay * dts[s][hh] as f64 * xs[s][hh * p + pp] as f64;
                }
                y += d[hh] as f64 * xs[ti][hh * p + pp] as f64;
                assert!(close(rec[ti][hh * p + pp] as f64, y, 1e-5), "t={ti} h={hh} p={pp}: {} vs {y}", rec[ti][hh * p + pp]);
            }
        }
    }
}

#[test]
fn rwkv4_stabilised_recurrence_equals_the_direct_sum() {
    // wkv_t = (Σ_{i<t} e^{(t−1−i)w + k_i} v_i + e^{u+k_t} v_t) / (Σ_{i<t} e^{(t−1−i)w + k_i} + e^{u+k_t}).
    let (w, u) = (-0.6f64, 0.4f64);
    let ks = [0.3f64, -1.2, 2.5, 0.1, 40.0];
    let vs = [1.0f64, -2.0, 0.5, 3.0, -1.0];
    let (mut num, mut den, mut mx) = (vec![0f32], vec![0f32], vec![-1e38f32]);
    for t in 0..ks.len() {
        let o = wkv4(&[ks[t] as f32], &[vs[t] as f32], &[w as f32], &[u as f32], &mut num, &mut den, &mut mx)[0] as f64;
        let mut a = (u + ks[t]).exp() * vs[t];
        let mut b = (u + ks[t]).exp();
        for i in 0..t {
            let e = ((t - 1 - i) as f64 * w + ks[i]).exp();
            a += e * vs[i];
            b += e;
        }
        assert!(close(o, a / b, 1e-5), "t={t}: {o} vs {}", a / b);
    }
}

#[test]
fn rwkv6_recurrence_equals_the_direct_sum() {
    // o_t[j] = Σ_i r_t[i] (u[i] k_t[i] v_t[j] + Σ_{s<t} Π_{m=s+1}^{t−1} w_m[i] · k_s[i] v_s[j]).
    let n = 2;
    let val = |t: usize, i: usize| ((t * 5 + i * 3) as f32 * 0.41).cos();
    let steps = 4;
    let r: Vec<Vec<f32>> = (0..steps).map(|t| (0..n).map(|i| val(t, i)).collect()).collect();
    let k: Vec<Vec<f32>> = (0..steps).map(|t| (0..n).map(|i| val(t, i + 7)).collect()).collect();
    let v: Vec<Vec<f32>> = (0..steps).map(|t| (0..n).map(|i| val(t, i + 13)).collect()).collect();
    let w: Vec<Vec<f32>> = (0..steps).map(|t| (0..n).map(|i| 0.5 + 0.4 * val(t, i + 19).abs()).collect()).collect();
    let u = [0.3f32, -0.2];
    let mut s = vec![0f32; n * n];
    for t in 0..steps {
        let o = wkv6(&r[t], &k[t], &v[t], &w[t], &u, &mut s, 1, n);
        for j in 0..n {
            let mut want = 0f64;
            for i in 0..n {
                let mut acc = u[i] as f64 * k[t][i] as f64 * v[t][j] as f64;
                for s_ in 0..t {
                    let decay: f64 = ((s_ + 1)..t).map(|m| w[m][i] as f64).product();
                    acc += decay * k[s_][i] as f64 * v[s_][j] as f64;
                }
                want += r[t][i] as f64 * acc;
            }
            assert!(close(o[j] as f64, want, 1e-5), "t={t} j={j}");
        }
    }
}

#[test]
fn rwkv7_single_state_hand_check() {
    // n = 1: S' = S·w + (S·a)·b + v·k; o = S'·r.
    let mut s = vec![2.0f32];
    let o = wkv7(&[3.0], &[0.5], &[4.0], &[5.0], &[0.25], &[-1.0], &mut s, 1, 1);
    let s1 = 2.0 * 0.5 - (2.0 * 0.25) + 5.0 * 4.0;
    assert_eq!(s[0], s1);
    assert_eq!(o[0], s1 * 3.0);
}

#[test]
fn mla_absorbed_form_equals_the_expanded_keys_and_values() {
    let (heads, nope, rope_d, vd, r) = (2usize, 3usize, 2usize, 2usize, 4usize);
    let qd = nope + rope_d;
    let val = |i: usize| ((i * 13 + 5) as f32 * 0.29).sin();
    let q: Vec<f32> = (0..heads * qd).map(val).collect();
    let kvb: Vec<f32> = (0..heads * (nope + vd) * r).map(|i| val(i + 100)).collect();
    let lat: Vec<Vec<f32>> = (0..3).map(|j| (0..r).map(|c| val(j * 10 + c + 300)).collect()).collect();
    let kr: Vec<Vec<f32>> = (0..3).map(|j| (0..rope_d).map(|c| val(j * 10 + c + 500)).collect()).collect();
    let scale = 0.37;
    let got = mla(&q, &kvb, &lat, &kr, heads, nope, rope_d, vd, r, scale);
    for h in 0..heads {
        let rows = &kvb[h * (nope + vd) * r..(h + 1) * (nope + vd) * r];
        let key = |j: usize| -> Vec<f64> {
            let mut k: Vec<f64> = (0..nope).map(|i| (0..r).map(|c| rows[i * r + c] as f64 * lat[j][c] as f64).sum()).collect();
            k.extend(kr[j].iter().map(|x| *x as f64));
            k
        };
        let value = |j: usize| -> Vec<f64> {
            (0..vd).map(|i| (0..r).map(|c| rows[(nope + i) * r + c] as f64 * lat[j][c] as f64).sum()).collect()
        };
        let qh: Vec<f64> = q[h * qd..(h + 1) * qd].iter().map(|x| *x as f64).collect();
        let sc: Vec<f64> = (0..3).map(|j| key(j).iter().zip(&qh).map(|(a, b)| a * b).sum::<f64>() * scale).collect();
        let p = softmax_with_sink(&sc, None);
        for i in 0..vd {
            let want: f64 = (0..3).map(|j| p[j] * value(j)[i]).sum();
            assert!(close(got[h * vd + i] as f64, want, 1e-5));
        }
    }
}

#[test]
fn norms_rms_layer_one_plus_w_groups_and_gates() {
    let x = [1.0f32, 3.0, -1.0, 1.0];
    // RMS: rms = sqrt(3), so x/sqrt(3).
    let y = norm(&x, NormKind::Rms, 0.0, Gain::None, None, None, 1);
    assert!(close(y[1] as f64, 3.0 / 3f64.sqrt(), 1e-6));
    // LayerNorm: mean 1, var 2.
    let y = norm(&x, NormKind::Layer, 0.0, Gain::None, None, None, 1);
    assert!(close(y[1] as f64, 2.0 / 2f64.sqrt(), 1e-6));
    // (1+w) gain, per-group shared gain of length n/groups.
    let g = Tensor::new(vec![2], vec![0.5, -0.5]);
    let y = norm(&x, NormKind::Rms, 0.0, Gain::OnePlusW, Some(&g), None, 2);
    // Group 0 = [1, 3] (rms √5); group 1 = [−1, 1] (rms 1).
    assert!(close(y[0] as f64, 1.5 / 5f64.sqrt(), 1e-6) && close(y[3] as f64, 0.5, 1e-6));
    // Per-group gains [groups, n/groups].
    let g = Tensor::new(vec![2, 2], vec![1.0, 1.0, 2.0, 2.0]);
    let y = norm(&x, NormKind::Rms, 0.0, Gain::W, Some(&g), None, 2);
    assert!(close(y[3] as f64, 2.0, 1e-6));
    // Gated RMSNorm: gate-then-norm (Mamba2) vs norm-then-gate (Qwen3-Next) differ.
    let z = [0.0f32, 5.0, 0.0, 5.0];
    let a = gated_rms_norm(&x, &z, &[1.0; 4], 0.0, 1, true);
    let b = gated_rms_norm(&x, &z, &[1.0; 4], 0.0, 1, false);
    assert_eq!((a[0], b[0]), (0.0, 0.0), "silu(0) = 0 either way");
    assert!((a[1] - b[1]).abs() > 0.1);
}
