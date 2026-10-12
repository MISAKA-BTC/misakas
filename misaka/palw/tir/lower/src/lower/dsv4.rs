//! **The DeepSeek-V4 family of features** (FR-10): manifold-constrained hyper-connections (`RESIDUAL_MHC_SINKHORN_V1`), compressed entries
//! (`ATTN_COMPRESSED_KV_V1`), the entry indexer (`ATTN_ENTRY_INDEXER_V1`), the `√softplus` router — each a combination of the 25 primitives.
//! No new primitive and no new court kernel.
//!
//! * [`mhc_map`] computes `pre`, `post` and the Sinkhorn-projected `comb` in Q24 (the sigmoid and softmax of the library, then
//!   `2·iters − 1` exact divisions by row or column sums), and hands them on as fixed-point codes.
//! * [`window_write`] and [`window_pool`] keep the rows of the window being filled in `Fixed` states (one-hot writes) and pool them with
//!   the library's softmax over the window (a transpose makes the window the last axis); the previous window's first series of an
//!   overlapped (CSA) compressor is a state of its own, masked out for window 0.
//! * [`entry_select`] scores the entries with the learned head weights and takes a fixed-K `TopK` over the whole store.
//! * [`entry_attention`] is one softmax over the window's keys, the visible entries and the sink: the maximum over the three parts, the exact
//!   exponent sums against it, the probabilities of each part times its own rows.
//!
//! Every value is data, never a shape: the store is always scanned whole, the masks are `Select`s.

use super::generic::{bp_after, bp_blk, bp_completing, bp_pm, keys_after, len_of, narrow_to, unsplit};
use super::*;
use tir::Cmp;
use tir::arith::{K, ONE};

/// Q14 of a weight code: the fixed scale `pre` and `post` leave in.
const WEIGHT_Q: u32 = 14;
/// Q15 of a `comb` code.
const COMB_Q: u32 = 15;

fn fixed_key(bits: u32) -> ScaleKey {
    ScaleKey { base: Base::Fixed(1.0 / (1u64 << bits) as f64), factor: 1.0 }
}

/// `x·scale + base` per lane, in Q24 `i32`: the logits of the mHC mapping (`scale_of[i]` the lane's own scale parameter, element `k` of the
/// `scale` param, `base` the `base` param).
#[allow(clippy::too_many_arguments)]
fn affine_q24(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    x: &Val,
    n: usize,
    base_p: u32,
    scale_p: u32,
    scale_of: Arc<dyn Fn(usize) -> usize + Send + Sync>,
    site: &str,
) -> Result<tir::Ref> {
    let kx = x.key.clone();
    let so = scale_of.clone();
    let (m, s) = decl_ms(
        b,
        cx,
        lb,
        &format!("{site}.logit"),
        n,
        Arc::new(move |c| {
            let sm = c.scale(&kx)?;
            let sc = c.f(scale_p)?;
            Ok((0..n).map(|i| sm * sc.data[so(i)] as f64 * ONE as f64).collect())
        }),
    )?;
    let z = decl(
        b,
        cx,
        lb,
        &format!("{site}.base"),
        DType::I64,
        &[n],
        per_layer(lb),
        Arc::new(move |c| {
            let bv = c.f(base_p)?;
            Ok(IntTensor::i64(vec![n], bv.data.iter().map(|v| (*v as f64 * ONE as f64).round() as i64).collect()))
        }),
    )?;
    Ok(narrow(b, x.r, m, s, Some(z), DType::I32))
}

/// A Q24 `i32` vector to `i16` codes at `2^-bits`, clamped.
fn q24_to_codes(b: &mut BlockBuilder<'_>, x: tir::Ref, bits: u32) -> tir::Ref {
    let y = b.shr(x, K - bits, Rounding::HalfAwayFromZero, DType::I64);
    let r = b.clamp(y, 0, 32767, DType::I16);
    b.commit(r)
}

/// **`MhcMap`**: `[pre, post, comb]` from the mix logits `m` ([`crate::hl::Op::MhcMap`]). `pre` and `post` leave as Q14 codes, `comb` as
/// Q15 codes, all at fixed scales.
#[allow(clippy::too_many_arguments)]
pub(super) fn mhc_map(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    m: &Val,
    base_p: u32,
    scale_p: u32,
    hc: usize,
    iters: usize,
    eps: f64,
    site: &str,
) -> Result<Vec<Option<Val>>> {
    unsplit(m, site)?;
    if m.dt != DType::I16 {
        return Err(LowerError::eval("internal: the mHC mix logits are read as i16 codes"));
    }
    let n = (2 + hc) * hc;
    // The logits: lanes `[0, hc)` take scale 0, `[hc, 2hc)` scale 1, the rest scale 2.
    let u = affine_q24(b, cx, lb, m, n, base_p, scale_p, Arc::new(move |i| if i < hc { 0 } else if i < 2 * hc { 1 } else { 2 }), site)?;
    let eps_q = (eps * ONE as f64).round() as i64;
    let epsc = b.c(DType::I64, eps_q as i128);
    // pre = σ(u₀) + ε, post = 2σ(u₁): Q24 → Q14 codes.
    let u0 = b.slice(u, 0, 0, hc as u32);
    let u1 = b.slice(u, 0, hc as u32, hc as u32);
    let s0 = b.int_sigmoid(u0);
    let pre = b.add(s0, epsc, DType::I64);
    let pre = q24_to_codes(b, pre, WEIGHT_Q);
    let s1 = b.int_sigmoid(u1);
    let two = b.c(DType::I64, 2);
    let post = b.mul(s1, two, DType::I64);
    let post = q24_to_codes(b, post, WEIGHT_Q);
    // comb: the row softmax of the logits, `+ ε`, then the Sinkhorn divisions — all in Q24, `i64`.
    let u2 = b.slice(u, 0, 2 * hc as u32, (hc * hc) as u32);
    let lg = b.reshape_fixed(u2, &[hc as u32, hc as u32]);
    let p = b.softmax_shifted(lg, 0);
    let mut c = b.add(p, epsc, DType::I64);
    let one = b.c(DType::I64, ONE);
    let normalise = |b: &mut BlockBuilder<'_>, c: tir::Ref, axis: usize| -> tir::Ref {
        let sum = b.reduce_sum(c, axis, DType::I64);
        let den = b.add(sum, epsc, DType::I64);
        // The divisor must be at least 1 whatever ε rounds to (the entries are non-negative, so it is `≥ ε` already).
        let den = if eps_q >= 1 { den } else { b.clamp(den, 1, i64::MAX, DType::I64) };
        let num = b.mul(c, one, DType::I64);
        let q = b.div(num, den, Rounding::Floor, DType::I64);
        b.clamp(q, 0, 1 << 25, DType::I64)
    };
    // Entries are `≥ 0` and `≤ 2^25` from the start (a probability plus ε).
    c = b.clamp(c, 0, 1 << 25, DType::I64);
    c = normalise(b, c, 0);
    for _ in 1..iters {
        c = normalise(b, c, 1);
        c = normalise(b, c, 0);
    }
    let comb = b.shr(c, K - COMB_Q, Rounding::HalfAwayFromZero, DType::I64);
    let comb = b.clamp(comb, 0, 32767, DType::I16);
    let comb = b.reshape_fixed(comb, &[(hc * hc) as u32]);
    b.commit(comb);
    let mk = |r: tir::Ref, len: usize, bits: u32, name: String| Some(Val { r, dt: DType::I16, key: fixed_key(bits), len, site: name });
    Ok(vec![
        mk(pre, hc, WEIGHT_Q, format!("{site}.pre")),
        mk(post, hc, WEIGHT_Q, format!("{site}.post")),
        mk(comb, hc * hc, COMB_Q, format!("{site}.comb")),
    ])
}

/// **`MhcPre`**: `σ(m·s + b) + ε` as Q14 codes (the final collapse's weights).
#[allow(clippy::too_many_arguments)]
pub(super) fn mhc_pre(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    m: &Val,
    base_p: u32,
    scale_p: u32,
    hc: usize,
    eps: f64,
    site: &str,
) -> Result<Val> {
    unsplit(m, site)?;
    let u = affine_q24(b, cx, lb, m, hc, base_p, scale_p, Arc::new(|_| 0), site)?;
    let s = b.int_sigmoid(u);
    let epsc = b.c(DType::I64, (eps * ONE as f64).round() as i128);
    let p = b.add(s, epsc, DType::I64);
    let r = q24_to_codes(b, p, WEIGHT_Q);
    Ok(Val { r, dt: DType::I16, key: fixed_key(WEIGHT_Q), len: hc, site: site.to_string() })
}

/// The `[rows, w]` `i16` buffer state of an HL `Fixed` state, declared once.
fn buffer_state(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, st: u32, rows: usize, w: usize) -> u16 {
    let sd = &cx.hl.states[st as usize];
    match cx.tstate.get(&st) {
        Some(t) => *t,
        None => {
            let t = b.pb.fixed_state(&sd.name, DType::I16, &[rows as u32, w as u32], -32767, 32767, true);
            cx.tstate.insert(st, t);
            t
        }
    }
}

/// **`WindowWrite`**: row `pos mod ratio` of the `[ratio, w]` buffer becomes `row`. The buffer after the write is kept for the pool of the
/// same position, and the rows' scale is recorded for it.
pub(super) fn window_write(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, node: &hl::Node, row: &Val, ratio: usize) -> Result<()> {
    unsplit(row, &row.site)?;
    let hl::Ref::State(st) = node.inputs[1] else { return Err(LowerError::eval("internal: WindowWrite without a state")) };
    let w = len_of(b, row.r)?;
    let ts = buffer_state(b, cx, st, ratio, w);
    let slot = bp_pm(b, lb, ratio);
    let iota = b.iota(DType::I64, &[Dim::Fixed(ratio as u32), Dim::Fixed(1)], 0, 0, 1);
    let hit = b.compare(iota, slot, Cmp::Eq);
    let r1 = b.reshape_fixed(row.r, &[1, w as u32]);
    let new = b.select(hit, r1, tir::Ref::State(ts), DType::I16);
    b.state_write(ts, new);
    lb.gx.win_new.insert(st, new);
    lb.gx.state_keys.insert(st, row.key.clone());
    Ok(())
}

/// **`WindowPool`**: per channel, the softmax over the window's rows of `gate + ape` weights the `kv` rows ([`crate::hl::Op::WindowPool`]).
#[allow(clippy::too_many_arguments)]
pub(super) fn window_pool(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    node: &hl::Node,
    ratio: usize,
    dim: usize,
    overlap: bool,
    site: &str,
    want: &Want,
) -> Result<Val> {
    let (hl::Ref::State(sk), hl::Ref::State(sg), hl::Ref::Param(ape_p)) = (node.inputs[0], node.inputs[1], node.inputs[2]) else {
        return Err(LowerError::eval("internal: WindowPool without its buffers"));
    };
    let m = ratio;
    let cin = dim * if overlap { 2 } else { 1 };
    let (kv_new, g_new) = match (lb.gx.win_new.get(&sk), lb.gx.win_new.get(&sg)) {
        (Some(a), Some(c)) => (*a, *c),
        _ => return Err(LowerError::eval("internal: a window is pooled before its rows are written")),
    };
    let (kk, kg) = match (lb.gx.state_keys.get(&sk), lb.gx.state_keys.get(&sg)) {
        (Some(a), Some(c)) => (a.clone(), c.clone()),
        _ => return Err(LowerError::eval("internal: a window buffer without a scale")),
    };
    // The gate's logits in Q14: `g·s_g·2^14 + ape·2^14` (the position bias is the narrowing's zero term, per row and channel).
    let (m1, s1) = decl_ms(
        b,
        cx,
        lb,
        &format!("{site}.gate_scale"),
        1,
        {
            let kg = kg.clone();
            Arc::new(move |c| Ok(vec![c.scale(&kg)? * (1u64 << LOGIT_Q) as f64]))
        },
    )?;
    let z = decl(
        b,
        cx,
        lb,
        &format!("{site}.ape"),
        DType::I64,
        &[m, cin],
        per_layer(lb),
        Arc::new(move |c| {
            let a = c.f(ape_p)?;
            Ok(IntTensor::i64(vec![m, cin], a.data.iter().map(|v| (*v as f64 * (1u64 << LOGIT_Q) as f64).round() as i64).collect()))
        }),
    )?;
    let off = if overlap { dim as u32 } else { 0 };
    let cur_g = b.slice(g_new, 1, off, dim as u32);
    let cur_z = b.slice(z, 1, off, dim as u32);
    let cur_logit = narrow(b, cur_g, m1, s1, Some(cur_z), DType::I32);
    let cur_kv = b.slice(kv_new, 1, off, dim as u32);
    let (logits, values) = if overlap {
        let (hl::Ref::State(pk), hl::Ref::State(pg)) = (node.inputs[4], node.inputs[5]) else {
            return Err(LowerError::eval("internal: an overlapped WindowPool without its previous-window states"));
        };
        let tpk = buffer_state(b, cx, pk, m, dim);
        let tpg = buffer_state(b, cx, pg, m, dim);
        // The previous window's first series (read before this position may replace it), its logits and its weight-0 mask for window 0.
        let prev_z = b.slice(z, 1, 0, dim as u32);
        let prev_logit = narrow(b, tir::Ref::State(tpg), m1, s1, Some(prev_z), DType::I32);
        let blk = bp_blk(b, lb, m);
        let one = b.c(DType::I64, 1);
        let have = b.compare(blk, one, Cmp::Ge);
        let floor = b.c(DType::I32, i32::MIN as i128);
        let prev_logit = b.select(have, prev_logit, floor, DType::I32);
        let lg = b.concat(&[prev_logit, cur_logit], 0);
        let vs = b.concat(&[tir::Ref::State(tpk), cur_kv], 0);
        // At the end of a window its first series is the next window's previous half.
        let completing = bp_completing(b, lb, m);
        let nk = b.slice(kv_new, 1, 0, dim as u32);
        let ng = b.slice(g_new, 1, 0, dim as u32);
        let wk = b.select(completing, nk, tir::Ref::State(tpk), DType::I16);
        let wg = b.select(completing, ng, tir::Ref::State(tpg), DType::I16);
        b.state_write(tpk, wk);
        b.state_write(tpg, wg);
        (lg, vs)
    } else {
        (cur_logit, cur_kv)
    };
    // Softmax over the rows (the last axis after a transpose), then the weighted sum of the kv rows.
    let lt = b.transpose(logits, &[1, 0]);
    let p = b.softmax_shifted(lt, 24 - LOGIT_Q);
    let vt = b.transpose(values, &[1, 0]);
    let prod = b.mul(p, vt, DType::I64);
    let acc = b.reduce_sum(prod, 1, DType::I64);
    let acc = b.reshape_fixed(acc, &[dim as u32]);
    narrow_to(b, cx, lb, acc, dim, Arc::new(move |c| Ok(c.scale(&kk)? / ONE as f64)), site, want)
}

/// **`EntrySelect`**: the ids of the `top` best visible entries. Each scores `Σ_h w_h·ReLU(q_h·k)` (the positive factor `dim^-½` moves no
/// rank); the weights may be negative, so an invisible entry takes `i32::MIN`. `TopK` over the whole store, ties to the lower index.
#[allow(clippy::too_many_arguments)]
pub(super) fn entry_select(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    node: &hl::Node,
    q: &Val,
    w: &Val,
    cand: &Val,
    heads: usize,
    dim: usize,
    ratio: usize,
    blocks: usize,
    top: usize,
    site: &str,
) -> Result<Val> {
    unsplit(q, site)?;
    unsplit(w, site)?;
    unsplit(cand, site)?;
    let hl::Ref::State(st) = node.inputs[3] else { return Err(LowerError::eval("internal: EntrySelect without a state")) };
    let (_, keys) = keys_after(b, cx, lb, st, cand, ratio, blocks)?;
    let qm = b.reshape_fixed(q.r, &[heads as u32, dim as u32]);
    let qt = b.transpose(qm, &[1, 0]);
    let sc = b.matmul(keys, qt, DType::I64);
    let relu = b.clamp(sc, 0, 1 << 62, DType::I64);
    let wr = b.reshape_fixed(w.r, &[1, heads as u32]);
    let weighted = b.mul(relu, wr, DType::I64);
    let sum = b.reduce_sum(weighted, 1, DType::I64);
    // `heads · dim · 2^30 · 2^15` is below `2^(45 + ⌈log2(heads · dim)⌉)`: this many bits down it is inside `i32`.
    let hd = (heads * dim).max(1);
    let shift = 14 + (usize::BITS - (hd - 1).leading_zeros());
    let sc32 = b.shr(sum, shift, Rounding::Floor, DType::I32);
    let iota = b.iota(DType::I64, &[Dim::Fixed(blocks as u32), Dim::Fixed(1)], 0, 0, 1);
    let after = bp_after(b, lb, ratio);
    let valid = b.compare(iota, after, Cmp::Lt);
    let floor = b.c(DType::I32, i32::MIN as i128);
    let masked = b.select(valid, sc32, floor, DType::I32);
    b.commit(masked);
    let ids = b.topk(masked, 0, top as u32);
    let ids = b.reshape_fixed(ids, &[top as u32]);
    Ok(Val { r: ids, dt: DType::Idx, key: ScaleKey::q24(), len: top, site: site.to_string() })
}

/// What [`entry_attention`] reads.
pub(super) struct EntryInputs {
    pub heads: usize,
    pub hd: usize,
    pub ratio: usize,
    pub blocks: usize,
    pub scale: f64,
    /// The window history's rows `[H, hd]` and their scale.
    pub window: (tir::Ref, ScaleKey),
    /// The entries' HL state (the lowered store after this position's write is found through it).
    pub store: u32,
    /// The sink logits, `[heads]` Q`LOGIT_Q` (declared by the caller).
    pub sinks: tir::Ref,
    /// The selected ids `[topk]`, when the indexer selects.
    pub ids: Option<tir::Ref>,
}

/// **`EntryAttention`**: one softmax over the window's keys, the visible entries and the sink, `K = V`.
#[allow(clippy::too_many_arguments)]
pub(super) fn entry_attention(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    q: &Val,
    inp: &EntryInputs,
    site: &str,
    want: &Want,
) -> Result<Val> {
    unsplit(q, site)?;
    let EntryInputs { heads, hd, ratio, blocks, scale, .. } = *inp;
    let store = *lb.gx.keys_new.get(&inp.store).ok_or_else(|| LowerError::eval("internal: the entries are attended before their write"))?;
    let key_e = lb
        .gx
        .keys_key
        .get(&inp.store)
        .cloned()
        .ok_or_else(|| LowerError::eval("internal: the entries' scale is unknown where they are attended"))?;
    let (h32, d32) = (heads as u32, hd as u32);
    // The entries the softmax reads: the whole store (every entry up to the visible ones), or — selecting — the rows `ids` name, gathered
    // (so a terminal tile is `heads · topk · head_dim` MACs, not `heads · blocks · head_dim`).
    let (e_rows, n_e) = match inp.ids {
        Some(ids) => {
            let top = len_of(b, ids)?;
            (b.gather(store, ids, 0, 0), top)
        }
        None => (store, blocks),
    };
    let b32 = n_e as u32;
    let q3 = b.reshape_fixed(q.r, &[1, h32, d32]);
    // The window part: `[1, heads, H]`.
    let kw = b.reshape(inp.window.0, &[Dim::H, Dim::Fixed(1), Dim::Fixed(d32)]);
    let kt = b.transpose(kw, &[1, 2, 0]);
    let sw = b.matmul(q3, kt, DType::I64);
    // The entries' part: `[1, heads, blocks]`.
    let e3 = b.reshape_fixed(e_rows, &[1, b32, d32]);
    let et = b.transpose(e3, &[0, 2, 1]);
    let se = b.matmul(q3, et, DType::I64);
    let (kq, kw_key) = (q.key.clone(), inp.window.1.clone());
    let (ms, ss) = decl_ms(
        b,
        cx,
        lb,
        &format!("{site}.scores_w"),
        1,
        {
            let (kq, kk) = (kq.clone(), kw_key.clone());
            Arc::new(move |c| Ok(vec![c.scale(&kq)? * c.scale(&kk)? * scale * (1u64 << LOGIT_Q) as f64]))
        },
    )?;
    let lw = narrow(b, sw, ms, ss, None, DType::I32);
    // The logits are commit points: a terminal tile of the court is one matrix product, not the scores' and the context's together.
    let lw = b.commit(lw);
    let (me, se_) = decl_ms(
        b,
        cx,
        lb,
        &format!("{site}.scores_e"),
        1,
        {
            let (kq, ke) = (kq.clone(), key_e.clone());
            Arc::new(move |c| Ok(vec![c.scale(&kq)? * c.scale(&ke)? * scale * (1u64 << LOGIT_Q) as f64]))
        },
    )?;
    let le = narrow(b, se, me, se_, None, DType::I32);
    // Visible entries: `t < (pos + 1) / ratio`, and — selecting — `t` named by `ids`.
    let after = bp_after(b, lb, ratio);
    let vis = match inp.ids {
        // The gathered rows: a pick is visible when its entry is (a pick of a not yet complete entry is masked).
        Some(ids) => {
            let ids64 = b.cast(ids, DType::I64);
            b.compare(ids64, after, Cmp::Lt)
        }
        None => {
            let iota = b.iota(DType::I64, &[Dim::Fixed(b32)], 0, 0, 1);
            b.compare(iota, after, Cmp::Lt)
        }
    };
    let vis3 = b.reshape_fixed(vis, &[1, 1, b32]);
    let floor = b.c(DType::I32, i32::MIN as i128);
    let le = b.select(vis3, le, floor, DType::I32);
    let le = b.commit(le);
    // The joint softmax (the library's `softmax_with_sink` over two parts): the maximum over both and the sink, exact exponent sums against it.
    let up = 24 - LOGIT_Q;
    let sink = b.reshape_fixed(inp.sinks, &[1, h32, 1]);
    let mw = b.reduce_max(lw, 2);
    let me2 = b.reduce_max(le, 2);
    let mw = b.cast(mw, DType::I64);
    let me2 = b.cast(me2, DType::I64);
    let sk = b.cast(sink, DType::I64);
    let m1 = b.max2(mw, me2, DType::I64);
    let mx = b.max2(m1, sk, DType::I64);
    let dfloor = (i32::MIN as i64) >> up;
    let upc = b.c(DType::I64, 1i128 << up);
    let exp_of = |b: &mut BlockBuilder<'_>, v: tir::Ref| {
        let diff = b.sub(v, mx, DType::I64);
        let d = b.clamp(diff, dfloor, 0, DType::I64);
        let w = b.mul(d, upc, DType::I64);
        let arg = b.clamp(w, i32::MIN as i64, 0, DType::I32);
        b.int_exp(arg)
    };
    let ew = exp_of(b, lw);
    let ee = exp_of(b, le);
    let es = exp_of(b, sk);
    let sw_sum = b.reduce_sum(ew, 2, DType::I64);
    let se_sum = b.reduce_sum(ee, 2, DType::I64);
    let tot = b.add(sw_sum, se_sum, DType::I64);
    let tot = b.add(tot, es, DType::I64);
    let recip = b.int_recip(tot);
    let prob = |b: &mut BlockBuilder<'_>, e: tir::Ref| {
        let p = b.mul(e, recip, DType::I128);
        let q = b.shr(p, K, Rounding::Floor, DType::I64);
        b.clamp(q, 0, 1 << 25, DType::I32)
    };
    let pw = prob(b, ew);
    let pe = prob(b, ee);
    // The context: each part's probabilities times its own rows (K = V), brought to the output's scale and added.
    let vw = b.reshape(inp.window.0, &[Dim::Fixed(1), Dim::H, Dim::Fixed(d32)]);
    let aw = b.matmul(pw, vw, DType::I64);
    let ae = b.matmul(pe, e3, DType::I64);
    let ko = want.key.clone();
    let n = if ko.split() > 0 { heads * hd } else { 1 };
    let ratio_of = |kv: ScaleKey, ko: ScaleKey| -> Arc<dyn Fn(&FillCtx<'_>) -> Result<Vec<f64>> + Send + Sync> {
        Arc::new(move |c| {
            let sv = c.scale(&kv)? / (1u64 << 24) as f64;
            Ok(c.scale_vec(&ko, n)?.iter().map(|so| sv / so).collect())
        })
    };
    let (mw_, sw_) = decl_ms(b, cx, lb, &format!("{site}.ctx_w"), n, ratio_of(kw_key, ko.clone()))?;
    let (me_, se__) = decl_ms(b, cx, lb, &format!("{site}.ctx_e"), n, ratio_of(key_e, ko.clone()))?;
    let (mw_, sw_, me_, se__) = if n > 1 {
        let shape = [1u32, h32, d32];
        (b.reshape_fixed(mw_, &shape), b.reshape_fixed(sw_, &shape), b.reshape_fixed(me_, &shape), b.reshape_fixed(se__, &shape))
    } else {
        (mw_, sw_, me_, se__)
    };
    let cw = narrow(b, aw, mw_, sw_, None, DType::I32);
    let cw = b.commit(cw);
    let ce = narrow(b, ae, me_, se__, None, DType::I32);
    let ce = b.commit(ce);
    let sum = b.add(cw, ce, DType::I64);
    let r = b.clamp(sum, -32767, 32767, DType::I16);
    let r = b.reshape_fixed(r, &[(heads * hd) as u32]);
    b.commit(r);
    Ok(Val { r, dt: DType::I16, key: ko, len: heads * hd, site: site.to_string() })
}

/// `√softplus(y)` in Q24 for Q24 logits `y` (`i32`): `softplus(y) = max(y, 0) + ln(1 + e^(−|y|))` (`IntExp`, `IntLn`), and the square root is
/// [`super::altup::isqrt`] of its Q48 form.
pub(super) fn sqrt_softplus_q24(b: &mut BlockBuilder<'_>, y: tir::Ref) -> tir::Ref {
    let zero = b.c(DType::I32, 0);
    let pos = b.compare(y, zero, Cmp::Gt);
    let neg = b.sub(zero, y, DType::I64);
    let neg_abs = b.select(pos, neg, y, DType::I64);
    let neg_abs = b.clamp(neg_abs, i32::MIN as i64, 0, DType::I32);
    let e = b.int_exp(neg_abs);
    let one = b.c(DType::I32, ONE);
    let t = b.add(e, one, DType::I64);
    let ln = b.int_ln(t);
    let relu = b.clamp(y, 0, i32::MAX as i64, DType::I64);
    let sp = b.add(relu, ln, DType::I64);
    let sp = b.clamp(sp, 0, 1 << 31, DType::I64);
    let c24 = b.c(DType::I64, ONE);
    let v = b.mul(sp, c24, DType::I64);
    let v = b.cast(v, DType::I128);
    let root = super::altup::isqrt(b, v);
    b.clamp(root, 0, i32::MAX as i64, DType::I32)
}
