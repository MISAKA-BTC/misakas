//! **The generic feature lowerers** (RFC-0002 lane G): hyper-connection residual streams, hashed
//! n-gram per-layer embeddings, dilated depthwise convolutions and sparse block attention, each a
//! combination of the 25 frozen primitives — no new primitive, no new court kernel.
//!
//! Every function here lowers one HL op ([`crate::hl::Op`]) that a *feature* introduced
//! ([`crate::model::REGISTRY`]); nothing in them knows a model's name.
//!
//! # Data-dependent values, never shapes
//!
//! A sparse-attention layer reads a different set of blocks at every position, but every tensor
//! keeps its static shape: the block scores are computed for ALL `blocks` rows, the incomplete ones
//! masked to a floor, a fixed-`K` `TopK` picks the ids, and the attention logits of the keys outside
//! the picked blocks are set to the softmax's floor. The `TopK` tie rule is 04b §6.6's: the higher
//! score first, equal scores the lower index first, the set in ascending index order.
//!
//! # The n-gram hash in `i64` arithmetic
//!
//! `mixed_n = t_0·m_0 ⊕ … ⊕ t_{n−1}·m_{n−1}` needs an XOR the primitive set does not have. It is
//! computed by bit decomposition: `bit_k(a) = ⌊a / 2^k⌋ − 2·⌊a / 2^(k+1)⌋` for the 63 bits of the
//! (non-negative, `< 2^63`) products, the XOR of a set of bits is their sum mod 2, and the bits
//! recompose by a weighted sum. MEASURED: 26 nodes for a trigram layer's ids (14 more for the table
//! read); with bit primitives (XOR, shifts, a wrapping multiply, a remainder) it would be about 6 —
//! recorded in the feature registry as evidence for a possible one-time primitive-set extension, not a
//! requirement. [`tests::the_in_program_hash_equals_the_reference_at_published_scale`] runs the
//! arithmetic at a vocabulary of 248,320 and head tables of 20 M rows against the reference function.

use super::*;
use crate::ngram::NgramTables;
use crate::spec::NgramPleSpec;
use tir::Cmp;

/// What the generic lowerers share inside one block.
#[derive(Default)]
pub(super) struct Shared {
    pos: BTreeMap<usize, BlockPos>,
    /// HL block-key state → the matrix after this position's write (shared by the selection that
    /// reads it and the write that stores it).
    keys_new: BTreeMap<u32, tir::Ref>,
}

/// The position arithmetic of keys pooled in blocks of `ratio` positions, made on first use (a
/// node nothing reads is a dead node, which normal form forbids).
#[derive(Default, Clone, Copy)]
struct BlockPos {
    blk: Option<tir::Ref>,
    start: Option<tir::Ref>,
    pm: Option<tir::Ref>,
    is_first: Option<tir::Ref>,
    completing: Option<tir::Ref>,
    after: Option<tir::Ref>,
}

fn pos_ref() -> tir::Ref {
    tir::Ref::Input(INPUT_POS)
}

fn bp_get(lb: &Lb, ratio: usize) -> BlockPos {
    lb.gx.pos.get(&ratio).copied().unwrap_or_default()
}

fn bp_put(lb: &mut Lb, ratio: usize, f: impl FnOnce(&mut BlockPos)) {
    f(lb.gx.pos.entry(ratio).or_default());
}

/// `⌊pos / ratio⌋`: the position's block, `i64`.
fn bp_blk(b: &mut BlockBuilder<'_>, lb: &mut Lb, ratio: usize) -> tir::Ref {
    if let Some(r) = bp_get(lb, ratio).blk {
        return r;
    }
    let d = b.c(DType::I64, ratio as i128);
    let r = b.div(pos_ref(), d, Rounding::Floor, DType::I64);
    bp_put(lb, ratio, |p| p.blk = Some(r));
    r
}

/// The first position of the position's block, `i64`.
fn bp_start(b: &mut BlockBuilder<'_>, lb: &mut Lb, ratio: usize) -> tir::Ref {
    if let Some(r) = bp_get(lb, ratio).start {
        return r;
    }
    let blk = bp_blk(b, lb, ratio);
    let d = b.c(DType::I64, ratio as i128);
    let r = b.mul(blk, d, DType::I64);
    bp_put(lb, ratio, |p| p.start = Some(r));
    r
}

/// `pos mod ratio`, `i64` (a difference: the analysis cannot see that it is non-negative; it is only compared).
fn bp_pm(b: &mut BlockBuilder<'_>, lb: &mut Lb, ratio: usize) -> tir::Ref {
    if let Some(r) = bp_get(lb, ratio).pm {
        return r;
    }
    let start = bp_start(b, lb, ratio);
    let r = b.sub(pos_ref(), start, DType::I64);
    bp_put(lb, ratio, |p| p.pm = Some(r));
    r
}

/// `pos mod ratio = 0` (`i8`): this position opens a block.
fn bp_is_first(b: &mut BlockBuilder<'_>, lb: &mut Lb, ratio: usize) -> tir::Ref {
    if let Some(r) = bp_get(lb, ratio).is_first {
        return r;
    }
    let pm = bp_pm(b, lb, ratio);
    let zero = b.c(DType::I64, 0);
    let r = b.compare(pm, zero, Cmp::Eq);
    bp_put(lb, ratio, |p| p.is_first = Some(r));
    r
}

/// `pos mod ratio = ratio − 1` (`i8`): this position completes its block.
fn bp_completing(b: &mut BlockBuilder<'_>, lb: &mut Lb, ratio: usize) -> tir::Ref {
    if let Some(r) = bp_get(lb, ratio).completing {
        return r;
    }
    let pm = bp_pm(b, lb, ratio);
    let last = b.c(DType::I64, ratio as i128 - 1);
    let r = b.compare(pm, last, Cmp::Eq);
    bp_put(lb, ratio, |p| p.completing = Some(r));
    r
}

/// `⌊(pos + 1) / ratio⌋`: the blocks that are complete once this position is in, `i64`.
fn bp_after(b: &mut BlockBuilder<'_>, lb: &mut Lb, ratio: usize) -> tir::Ref {
    if let Some(r) = bp_get(lb, ratio).after {
        return r;
    }
    let one = b.c(DType::I64, 1);
    let p1 = b.add(pos_ref(), one, DType::I64);
    let d = b.c(DType::I64, ratio as i128);
    let r = b.div(p1, d, Rounding::Floor, DType::I64);
    bp_put(lb, ratio, |p| p.after = Some(r));
    r
}

fn unsplit(v: &Val, what: &str) -> Result<()> {
    if v.key.split() > 0 {
        return Err(LowerError::eval(format!("internal: `{what}` reads a value with per-channel scales")));
    }
    Ok(())
}

fn len_of(b: &BlockBuilder<'_>, r: tir::Ref) -> Result<usize> {
    match b.shape(r).as_slice() {
        [Dim::Fixed(n)] => Ok(*n as usize),
        s => Err(LowerError::eval(format!("internal: a vector was expected, got {s:?}"))),
    }
}

/// One uniform narrowing of `x` (an `i64`/`i32` accumulator at `ratio_of` float units per unit) into
/// `want` — the shape every composite below ends in.
#[allow(clippy::too_many_arguments)]
fn narrow_to(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    x: tir::Ref,
    len: usize,
    unit: Arc<dyn Fn(&FillCtx<'_>) -> Result<f64> + Send + Sync>,
    site: &str,
    want: &Want,
) -> Result<Val> {
    let ko = want.key.clone();
    let n = if ko.split() > 0 { len } else { 1 };
    let (m, s) = decl_ms(
        b,
        cx,
        lb,
        site,
        n,
        Arc::new(move |c| {
            let u = unit(c)?;
            Ok(c.scale_vec(&ko, n)?.iter().map(|so| u / so).collect())
        }),
    )?;
    let r = narrow(b, x, m, s, None, want.dt);
    if want.dt == DType::I16 {
        b.commit(r);
    }
    let v = Val { r, dt: want.dt, key: want.key.clone(), len, site: site.to_string() };
    note_resid(cx, lb, &v);
    Ok(v)
}

// ───────────────────────────── multi-stream residuals ─────────────────────────────

/// `out[d] = (1/S) Σ_s x[s·D + d]`: the sum of the streams' codes (exact in `i32`), narrowed once
/// with the `1/S` in its ratio.
pub(super) fn stream_mean(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, x: &Val, streams: usize, site: &str, want: &Want) -> Result<Val> {
    unsplit(x, site)?;
    if x.dt != DType::I16 {
        return Err(LowerError::eval("internal: a stream mean reads i16 codes"));
    }
    let n = len_of(b, x.r)?;
    let d = n / streams;
    let xr = b.reshape_fixed(x.r, &[streams as u32, d as u32]);
    let sum = b.reduce_sum(xr, 0, DType::I32);
    let sum = b.reshape_fixed(sum, &[d as u32]);
    let kx = x.key.clone();
    narrow_to(b, cx, lb, sum, d, Arc::new(move |c| Ok(c.scale(&kx)? / streams as f64)), site, want)
}

/// `out[s·D + d] = o[d] · w[s]`: one exact `i32` product per element, narrowed once.
pub(super) fn stream_outer(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    o: &Val,
    w: &Val,
    streams: usize,
    site: &str,
    want: &Want,
) -> Result<Val> {
    unsplit(o, site)?;
    unsplit(w, site)?;
    let d = len_of(b, o.r)?;
    if len_of(b, w.r)? != streams {
        return Err(LowerError::eval("internal: stream weights of the wrong width"));
    }
    let o2 = b.reshape_fixed(o.r, &[1, d as u32]);
    let w2 = b.reshape_fixed(w.r, &[streams as u32, 1]);
    let p = b.mul(w2, o2, DType::I32);
    let p = b.reshape_fixed(p, &[(streams * d) as u32]);
    let (ko, kw) = (o.key.clone(), w.key.clone());
    narrow_to(b, cx, lb, p, streams * d, Arc::new(move |c| Ok(c.scale(&ko)? * c.scale(&kw)?)), site, want)
}

/// `out[g] = Σ_{j in group g} a[j]·b[j]`: exact products in `i32`, the sums in `i64`, narrowed once.
pub(super) fn group_dot(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    a: &Val,
    c: &Val,
    groups: usize,
    site: &str,
    want: &Want,
) -> Result<Val> {
    unsplit(a, site)?;
    unsplit(c, site)?;
    let n = len_of(b, a.r)?;
    if len_of(b, c.r)? != n || n % groups != 0 {
        return Err(LowerError::eval("internal: a grouped dot of mismatched vectors"));
    }
    let g = n / groups;
    let ar = b.reshape_fixed(a.r, &[groups as u32, g as u32]);
    let cr = b.reshape_fixed(c.r, &[groups as u32, g as u32]);
    let p = b.mul(ar, cr, DType::I32);
    let s = b.reduce_sum(p, 1, DType::I64);
    let s = b.reshape_fixed(s, &[groups as u32]);
    let (ka, kc) = (a.key.clone(), c.key.clone());
    narrow_to(b, cx, lb, s, groups, Arc::new(move |f| Ok(f.scale(&ka)? * f.scale(&kc)?)), site, want)
}

// ───────────────────────────── dilated depthwise causal convolution ─────────────────────────────

/// The library's `causal_conv` with the taps `dilation` positions apart: the state keeps the last
/// `(kernel − 1)·dilation` rows, the window is state ++ row, tap `t` reads row `t·dilation` of it
/// (a `Gather` by a constant index vector), the last tap the new row. `taps` is `[kernel, ch]`
/// (oldest first); the result is the `i64` accumulator `[ch]`.
pub(super) fn causal_conv_dilated(b: &mut BlockBuilder<'_>, state: u16, row: tir::Ref, taps: tir::Ref, kernel: usize, dilation: usize) -> tir::Ref {
    let ch = match b.shape(row).as_slice() {
        [Dim::Fixed(c)] => *c,
        s => panic!("a conv row is a vector, got {s:?}"),
    };
    let keep_rows = ((kernel - 1) * dilation) as u32;
    let r1 = b.reshape_fixed(row, &[1, ch]);
    let win = b.concat(&[tir::Ref::State(state), r1], 0);
    let keep = b.slice(win, 0, 1, keep_rows);
    b.state_write(state, keep);
    let idx: Vec<i128> = (0..kernel).map(|t| (t * dilation) as i128).collect();
    let idx = b.pb.konst(DType::Idx, &[kernel as u32], &idx);
    let sel = b.gather(win, idx, 0, 0);
    let prod = b.mul(sel, taps, DType::I64);
    let acc = b.reduce_sum(prod, 0, DType::I64);
    b.reshape_fixed(acc, &[ch])
}

// ───────────────────────────── hashed n-gram per-layer embedding ─────────────────────────────

/// The hash constants of the model layer a fill runs for.
fn tables_for(c: &FillCtx<'_>, ple: &NgramPleSpec, layers: &[(usize, usize)]) -> Result<Arc<NgramTables>> {
    // `c.layer` is the occurrence; the hash constants follow the model layer.
    let l = c.hl.model_layer(c.layer.ok_or_else(|| LowerError::eval("an n-gram constant is filled outside a layer"))?);
    let idx = layers
        .iter()
        .find(|(ml, _)| *ml == l)
        .map(|(_, i)| *i)
        .ok_or_else(|| LowerError::eval(format!("layer {l} is not a PLE layer of its block")))?;
    let mut s = ple.clone();
    s.layer_index = idx;
    Ok(NgramTables::cached(&s))
}

/// The tallest head table over the block's layers (the params' common shape).
fn max_head_size(ple: &NgramPleSpec, layers: &[(usize, usize)]) -> i64 {
    layers
        .iter()
        .map(|(_, i)| {
            let mut s = ple.clone();
            s.layer_index = *i;
            NgramTables::cached(&s).head_sizes.iter().copied().max().unwrap_or(1)
        })
        .max()
        .unwrap_or(1)
}

/// **The hash's arithmetic** on the `n` tokens of an n-gram (`tv`: `i64 [n]`, in `[0, vocab)`), the layer's
/// multipliers and head sizes (params of the full `i64` range): each head's id within its own table,
/// `idx [heads]`, committed. Bit decomposition, no XOR primitive (see the module docs).
#[allow(clippy::too_many_arguments)]
fn hash_ids(
    b: &mut BlockBuilder<'_>,
    tv: tir::Ref,
    mult: tir::Ref,
    size: tir::Ref,
    n: usize,
    hpn: usize,
    mult_max: i64,
    size_hi: i64,
) -> tir::Ref {
    let w = n - 1;
    let heads = w * hpn;
    // 2. the products (their range is stated: the multipliers are params of the full `i64` range).
    let mc = b.clamp(mult, 0, mult_max, DType::I64);
    let a = b.mul(tv, mc, DType::I64);
    // 3. the 63 bits of each product: `bit_k = ⌊a/2^k⌋ − 2·⌊a/2^(k+1)⌋`.
    let a2 = b.reshape_fixed(a, &[n as u32, 1]);
    let pows: Vec<i128> = (0..63).map(|k| 1i128 << k).collect();
    let p2 = b.pb.konst(DType::I64, &[63], &pows);
    let q = b.div(a2, p2, Rounding::Floor, DType::I64);
    let two = b.c(DType::I64, 2);
    let h = b.div(q, two, Rounding::Floor, DType::I64);
    let h2 = b.mul(h, two, DType::I64);
    let bt = b.sub(q, h2, DType::I64);
    let bits = b.clamp(bt, 0, 1, DType::I8);
    // 4. the XOR of the first `m` products' bits, `m = 2..=n`, is their sum mod 2.
    let mut tri = Vec::with_capacity((n - 1) * n);
    for m in 2..=n {
        for k in 0..n {
            tri.push(i128::from(k < m));
        }
    }
    let tri = b.pb.konst(DType::I8, &[(n - 1) as u32, n as u32], &tri);
    let sums = b.matmul(tri, bits, DType::I64);
    let hh = b.div(sums, two, Rounding::Floor, DType::I64);
    let h3 = b.mul(hh, two, DType::I64);
    let pr = b.sub(sums, h3, DType::I64);
    let par = b.clamp(pr, 0, 1, DType::I8);
    // 5. recompose `Σ_k bit_k · 2^k` (the sum of 63 terms of up to 2^62 is stated in `i128`).
    let p2c = b.pb.konst(DType::I64, &[63, 1], &pows);
    let mx = b.matmul(par, p2c, DType::I128);
    let mx = b.clamp(mx, 0, i64::MAX, DType::I64);
    let mx = b.reshape_fixed(mx, &[w as u32]);
    // 6. each head's order picks its mixed value; `mixed mod size` without a remainder primitive.
    let order: Vec<i128> = (0..heads).map(|h| (h / hpn) as i128).collect();
    let order = b.pb.konst(DType::Idx, &[heads as u32], &order);
    let mh = b.gather(mx, order, 0, 0);
    let sz = b.clamp(size, 1, size_hi, DType::I64);
    let qd = b.div(mh, sz, Rounding::Floor, DType::I64);
    let qs = b.mul(qd, sz, DType::I128);
    let rm = b.sub(mh, qs, DType::I128);
    let ids = b.clamp(rm, 0, size_hi - 1, DType::Idx);
    b.commit(ids);
    ids
}

/// **The hash heads' table rows** of this position, `idx [heads]`: each head's id within its OWN
/// table (`mixed mod size`; [`gather_rows`] reads the per-head tables with it). Op inputs:
/// `[Token, State(window)]`; the window state holds the last `ngram_size − 1` segment-masked tokens
/// as `token − eos` (zero = `eos`: a fresh sequence needs no initialiser).
#[allow(clippy::too_many_arguments)]
pub(super) fn ngram_ids(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    node: &hl::Node,
    ple: &NgramPleSpec,
    layers: &[(usize, usize)],
    site: &str,
) -> Result<Val> {
    let hl = cx.hl;
    let hl::Ref::State(st) = node.inputs[1] else { return Err(LowerError::eval("internal: NgramIds without a window state")) };
    let (n, hpn) = (ple.ngram_size, ple.heads_per_ngram);
    if n < 2 || hpn == 0 || layers.is_empty() {
        return Err(LowerError::bad(format!("EMBED_NGRAM_PLE_V1: ngram_size {n}, {hpn} heads per order")));
    }
    let w = n - 1;
    let heads = w * hpn;
    let vocab = hl.vocab as i64;
    let eos = ple.eos_id as i64;
    if eos < 0 || eos >= vocab {
        return Err(LowerError::not_lowerable(format!("EMBED_NGRAM_PLE_V1: eos id {eos} outside the vocabulary of {vocab}")));
    }
    // The multipliers are bounded so that `token · multiplier` stays inside `i64` for every token.
    let mult_max = i64::MAX / vocab.max(1);
    let size_hi = max_head_size(ple, layers);
    if size_hi >= i64::from(u32::MAX) {
        return Err(LowerError::not_lowerable(format!("EMBED_NGRAM_PLE_V1: a head table of {size_hi} rows is past what an `idx` indexes")));
    }
    let sd = &hl.states[st as usize];
    let ts = match cx.tstate.get(&st) {
        Some(t) => *t,
        None => {
            let t = b.pb.fixed_state(&sd.name, DType::I32, &[w as u32], -eos, vocab - 1 - eos, true);
            cx.tstate.insert(st, t);
            t
        }
    };
    let pl = per_layer(lb);
    let (sp1, sp2, lay1, lay2) = (ple.clone(), ple.clone(), layers.to_vec(), layers.to_vec());
    let mult = decl(
        b,
        cx,
        lb,
        &format!("{site}.mult"),
        DType::I64,
        &[n],
        pl,
        Arc::new(move |c| Ok(IntTensor::i64(vec![n], tables_for(c, &sp1, &lay1)?.multipliers.clone()))),
    )?;
    let size = decl(
        b,
        cx,
        lb,
        &format!("{site}.size"),
        DType::I64,
        &[heads],
        pl,
        Arc::new(move |c| Ok(IntTensor::i64(vec![heads], tables_for(c, &sp2, &lay2)?.head_sizes.clone()))),
    )?;
    // 1. the n tokens of the position's n-gram: this token and the window's.
    let tok = tir::Ref::Input(INPUT_TOKEN);
    let t1 = b.reshape_fixed(tok, &[1]);
    let t1 = b.cast(t1, DType::I64);
    let eosc = b.c(DType::I64, eos as i128);
    let win = b.add(tir::Ref::State(ts), eosc, DType::I64);
    let tv = b.concat(&[t1, win], 0);
    let ids = hash_ids(b, tv, mult, size, n, hpn, mult_max, size_hi);
    // 7. the next window: this token, then the older ones unless this token ends the segment.
    let tm = b.sub(t1, eosc, DType::I64);
    let tm = b.clamp(tm, -eos, vocab - 1 - eos, DType::I32);
    let new = if w == 1 {
        tm
    } else {
        let is_eos = b.compare(tok, eosc, Cmp::Eq);
        let tail = b.slice(tir::Ref::State(ts), 0, 0, (w - 1) as u32);
        let zero = b.c(DType::I32, 0);
        let tail = b.select(is_eos, zero, tail, DType::I32);
        b.concat(&[tm, tail], 0)
    };
    b.state_write(ts, new);
    Ok(Val { r: ids, dt: DType::Idx, key: ScaleKey::q24(), len: heads, site: site.to_string() })
}

/// **A table row per hash head** (`GatherRows` over the ids of [`ngram_ids`]): each head owns a
/// contiguous range of the layer's table, so the table is split per head — `i16` codes at ONE
/// scale per layer, `[heads, rows, dim]` — and ONE batched `Gather` reads every head's row. A
/// head's table taller than NF-8's `2^24` rows is cut into chunks of `2^24` rows (`⌈size / 2^24⌉`
/// params, a `Select` by the chunk index): no table is ever one dimension past the cap, so a table
/// of hundreds of millions of rows needs no new primitive.
#[allow(clippy::too_many_arguments)]
pub(super) fn gather_rows(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    node: &hl::Node,
    ids: &Val,
    table: u32,
    site: &str,
    want: &Want,
) -> Result<Val> {
    let hl = cx.hl;
    let blk = &hl.blocks[lb.hb];
    // The ids must be a hash head's own table index: the producing node says how the table splits.
    let hl::Ref::Node(j, 0) = node.inputs[0] else { return Err(LowerError::not_lowerable("GatherRows over ids no n-gram hash produced")) };
    let Op::NgramIds { ple, layers } = &blk.nodes[j as usize].op else {
        return Err(LowerError::not_lowerable("GatherRows over ids no n-gram hash produced"));
    };
    let Op::GatherRows { heads, dim } = &node.op else { return Err(LowerError::eval("internal: gather_rows on another op")) };
    let (heads, dim) = (*heads, *dim);
    if ids.dt != DType::Idx || ids.len != heads {
        return Err(LowerError::eval("internal: GatherRows ids of the wrong shape"));
    }
    let max_size = max_head_size(ple, layers);
    // NF-8: no dimension past 2^24 rows (`LowerOpts::table_chunk_rows` lowers it for tests).
    let chunk_rows = cx.table_chunk;
    let nch = ((max_size + chunk_rows - 1) / chunk_rows) as usize;
    let rows_of = |k: usize| -> i64 { chunk_rows.min(max_size - k as i64 * chunk_rows) };
    let pl = per_layer(lb);
    let pd = &hl.params[table as usize];
    let mut chunk_params = Vec::with_capacity(nch);
    for k in 0..nch {
        let rows = rows_of(k) as usize;
        let (sp, lay) = (ple.clone(), layers.clone());
        let p = decl(
            b,
            cx,
            lb,
            &format!("{}.c{k}", pd.name),
            DType::I16,
            &[heads, rows, dim],
            pl,
            Arc::new(move |c| {
                let t = tables_for(c, &sp, &lay)?;
                let f = c.f(table)?;
                let scale = table_scale(&t, &f.data, dim);
                let mut v = vec![0i16; heads * rows * dim];
                for h in 0..heads {
                    let (off, size) = (t.head_offsets[h], t.head_sizes[h]);
                    for r in 0..rows {
                        let local = k as i64 * chunk_rows + r as i64;
                        if local >= size {
                            break;
                        }
                        let g = (off + local) as usize;
                        for d in 0..dim {
                            v[(h * rows + r) * dim + d] = (f.data[g * dim + d] as f64 / scale).round().clamp(-32767.0, 32767.0) as i16;
                        }
                    }
                }
                Ok(IntTensor::i16(vec![heads, rows, dim], v))
            }),
        )?;
        chunk_params.push(p);
    }
    // The rows of every head, from the chunk each id lies in.
    let picked = if nch == 1 {
        b.gather(chunk_params[0], ids.r, 1, 1)
    } else {
        let ck = b.c(DType::I64, chunk_rows as i128);
        let chunk = b.div(ids.r, ck, Rounding::Floor, DType::I64);
        let base = b.mul(chunk, ck, DType::I64);
        let local = b.sub(ids.r, base, DType::I64);
        let chunk2 = b.reshape_fixed(chunk, &[heads as u32, 1]);
        let mut rows_k = Vec::with_capacity(nch);
        for k in 0..nch {
            let li = b.clamp(local, 0, rows_of(k) - 1, DType::Idx);
            rows_k.push(b.gather(chunk_params[k], li, 1, 1));
        }
        let mut out = rows_k[nch - 1];
        for k in (0..nch - 1).rev() {
            let kc = b.c(DType::I64, k as i128);
            let hit = b.compare(chunk2, kc, Cmp::Eq);
            out = b.select(hit, rows_k[k], out, DType::I16);
        }
        out
    };
    let flat = b.reshape_fixed(picked, &[(heads * dim) as u32]);
    let (sp, lay) = (ple.clone(), layers.clone());
    let ko = want.key.clone();
    let rv = narrow_to(
        b,
        cx,
        lb,
        flat,
        heads * dim,
        Arc::new(move |c| {
            let t = tables_for(c, &sp, &lay)?;
            let f = c.f(table)?;
            Ok(table_scale(&t, &f.data, dim))
        }),
        site,
        &Want { dt: DType::I16, key: ko },
    )?;
    Ok(rv)
}

/// The table's one code scale: its largest magnitude over the layer's rows maps to ±32767.
fn table_scale(t: &NgramTables, data: &[f32], dim: usize) -> f64 {
    let rows = (t.total_vocab as usize).min(data.len() / dim.max(1));
    let amax = data[..rows * dim].iter().fold(0f64, |m, v| m.max((*v as f64).abs()));
    if amax > 0.0 { amax / 32767.0 } else { 1.0 }
}

// ───────────────────────────── sparse block attention ─────────────────────────────

/// `BlockMean`: the running sum of the keys of the position's block (`Fixed` state, restarted at the
/// block's first position) and `sum / ratio` — the block's mean once complete.
#[allow(clippy::too_many_arguments)]
pub(super) fn block_mean(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    node: &hl::Node,
    k: &Val,
    ratio: usize,
    site: &str,
    want: &Want,
) -> Result<Val> {
    unsplit(k, site)?;
    let hl = cx.hl;
    let hl::Ref::State(st) = node.inputs[1] else { return Err(LowerError::eval("internal: BlockMean without a state")) };
    let dim = len_of(b, k.r)?;
    let sd = &hl.states[st as usize];
    let bound = 32767 * ratio as i64;
    let ts = match cx.tstate.get(&st) {
        Some(t) => *t,
        None => {
            let t = b.pb.fixed_state(&sd.name, DType::I32, &[dim as u32], -bound, bound, true);
            cx.tstate.insert(st, t);
            t
        }
    };
    let first = bp_is_first(b, lb, ratio);
    let zero = b.c(DType::I32, 0);
    let base = b.select(first, zero, tir::Ref::State(ts), DType::I32);
    let sum = b.add(base, k.r, DType::I32);
    let sum = b.clamp(sum, -bound, bound, DType::I32);
    b.state_write(ts, sum);
    let kk = k.key.clone();
    narrow_to(b, cx, lb, sum, dim, Arc::new(move |c| Ok(c.scale(&kk)? / ratio as f64)), site, want)
}

/// The matrix of block keys after this position's write: row `pos / ratio` becomes `cand` when the
/// position completes its block. Made once per block-key state.
fn keys_after(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    st: u32,
    cand: &Val,
    ratio: usize,
    blocks: usize,
) -> Result<(u16, tir::Ref)> {
    let hl = cx.hl;
    let sd = &hl.states[st as usize];
    let dim = len_of(b, cand.r)?;
    let ts = match cx.tstate.get(&st) {
        Some(t) => *t,
        None => {
            let t = b.pb.fixed_state(&sd.name, DType::I16, &[blocks as u32, dim as u32], -32767, 32767, true);
            cx.tstate.insert(st, t);
            t
        }
    };
    if let Some(n) = lb.gx.keys_new.get(&st) {
        return Ok((ts, *n));
    }
    let blk = bp_blk(b, lb, ratio);
    let completing = bp_completing(b, lb, ratio);
    let iota = b.iota(DType::I64, &[Dim::Fixed(blocks as u32), Dim::Fixed(1)], 0, 0, 1);
    let hit = b.compare(iota, blk, Cmp::Eq);
    let write = b.mul(hit, completing, DType::I8);
    let row = b.reshape_fixed(cand.r, &[1, dim as u32]);
    let new = b.select(write, row, tir::Ref::State(ts), DType::I16);
    lb.gx.keys_new.insert(st, new);
    Ok((ts, new))
}

/// `BlockWrite`: store the matrix [`keys_after`] made.
pub(super) fn block_write(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, node: &hl::Node, cand: &Val, ratio: usize, blocks: usize) -> Result<()> {
    let hl::Ref::State(st) = node.inputs[1] else { return Err(LowerError::eval("internal: BlockWrite without a state")) };
    let cand = codes(b, cx, lb, cand)?;
    let (ts, new) = keys_after(b, cx, lb, st, &cand, ratio, blocks)?;
    b.state_write(ts, new);
    Ok(())
}

/// `BlockSelect`: the ids of the `top` best complete blocks. Each block scores `Σ_h relu(q_h · key)`
/// over the heads (the `1/√dim` of the float form moves no rank, so it is dropped); the scores come
/// in `i32` (committed), an incomplete block scores `−1` (every real score is `≥ 0`), and a fixed-K
/// `TopK` over all `blocks` rows picks the ids — ties to the lower index, the set ascending.
#[allow(clippy::too_many_arguments)]
pub(super) fn block_select(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    node: &hl::Node,
    q: &Val,
    cand: &Val,
    heads: usize,
    dim: usize,
    ratio: usize,
    blocks: usize,
    top: usize,
    site: &str,
) -> Result<Val> {
    unsplit(q, site)?;
    unsplit(cand, site)?;
    let hl::Ref::State(st) = node.inputs[2] else { return Err(LowerError::eval("internal: BlockSelect without a state")) };
    let (_, keys) = keys_after(b, cx, lb, st, cand, ratio, blocks)?;
    let qm = b.reshape_fixed(q.r, &[heads as u32, dim as u32]);
    let qt = b.transpose(qm, &[1, 0]);
    let sc = b.matmul(keys, qt, DType::I64);
    let relu = b.clamp(sc, 0, 1 << 62, DType::I64);
    let sum = b.reduce_sum(relu, 1, DType::I64);
    // `heads · dim · 32767²` is below `2^(30 + ⌈log2(heads · dim)⌉)`: shifted by that many bits it is
    // inside `i32` (the ranking needs no more resolution than its codes had).
    let hd = (heads * dim).max(1);
    let shift = usize::BITS - (hd - 1).leading_zeros(); // ceil(log2(hd)), in integers: the program must not depend on a libm
    let sc32 = b.shr(sum, shift, Rounding::Floor, DType::I32);
    let iota = b.iota(DType::I64, &[Dim::Fixed(blocks as u32), Dim::Fixed(1)], 0, 0, 1);
    let after = bp_after(b, lb, ratio);
    let valid = b.compare(iota, after, Cmp::Lt);
    let floor = b.c(DType::I32, -1);
    let masked = b.select(valid, sc32, floor, DType::I32);
    b.commit(masked);
    let ids = b.topk(masked, 0, top as u32);
    let ids = b.reshape_fixed(ids, &[top as u32]);
    Ok(Val { r: ids, dt: DType::Idx, key: ScaleKey::q24(), len: top, site: site.to_string() })
}

/// The attention logits of the keys outside the selected blocks (and outside the incomplete tail)
/// take the logits' floor, which the softmax sends to exp = 0. Key `t` of the window is position
/// `pos − (H − 1) + t`.
#[allow(clippy::too_many_arguments)]
pub(super) fn mask_unselected_blocks(
    b: &mut BlockBuilder<'_>,
    _cx: &mut Cx<'_>,
    lb: &mut Lb,
    logits: tir::Ref,
    ratio: usize,
    ids: tir::Ref,
    blocks: usize,
    window: u32,
) -> Result<tir::Ref> {
    let top = len_of(b, ids)?;
    let pos = pos_ref();
    let hm1 = b.clamp(pos, 0, window as i64 - 1, DType::I64);
    let t = b.iota(DType::I64, &[Dim::H], 0, 0, 1);
    let base = b.sub(pos, hm1, DType::I64);
    let j = b.add(base, t, DType::I64);
    let rr = b.c(DType::I64, ratio as i128);
    let bj = b.div(j, rr, Rounding::Floor, DType::I64);
    let bj = b.clamp(bj, 0, blocks as i64 - 1, DType::Idx);
    // which blocks the ids name
    let bi = b.iota(DType::I64, &[Dim::Fixed(blocks as u32), Dim::Fixed(1)], 0, 0, 1);
    let idr = b.reshape_fixed(ids, &[1, top as u32]);
    let hit = b.compare(bi, idr, Cmp::Eq);
    let sel = b.reduce_max(hit, 1);
    let sel = b.reshape_fixed(sel, &[blocks as u32]);
    let keep = b.gather(sel, bj, 0, 0);
    // the incomplete tail is always visible
    let after = bp_after(b, lb, ratio);
    let tail_start = b.mul(after, rr, DType::I64);
    let in_tail = b.compare(j, tail_start, Cmp::Ge);
    let one = b.c(DType::I8, 1);
    let vis = b.select(in_tail, one, keep, DType::I8);
    let floor = b.c(DType::I32, i32::MIN as i128);
    Ok(b.select(vis, logits, floor, DType::I32))
}

/// `RopeAtBlock`: the rotation at the first position of the position's block.
#[allow(clippy::too_many_arguments)]
pub(super) fn rope_at_block(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    x: &Val,
    heads: usize,
    hd: usize,
    rd: usize,
    off: usize,
    style: RopeStyle,
    table: u32,
    ratio: usize,
) -> Result<Val> {
    if cx.image_rows.is_some_and(|i| i.mrope.is_some()) {
        return Err(LowerError::not_lowerable("a pooled-key rotation under M-RoPE"));
    }
    let start = bp_start(b, lb, ratio);
    let angles = rope_angles_at(b, cx, lb, table, start)?;
    lower_rope_with(b, cx, lb, x, heads, hd, rd, off, style, table, angles)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{Gain, NormKind, NormSpec};
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
    use misaka_palw_tir::{ConeEnv, Interpreter, MapParams, Tensor};

    fn spec(layer_index: usize, ngram_size: usize, heads: usize, vocab: usize, base: usize) -> NgramPleSpec {
        NgramPleSpec {
            ngram_size,
            heads_per_ngram: heads,
            embed_dim: (ngram_size - 1) * heads * 4,
            conv_kernel: 4,
            conv_dilation: ngram_size,
            norm: NormSpec { kind: NormKind::Rms, eps: 1e-6, gain: Gain::OnePlusW, bias: false },
            eos_id: 2,
            seed: 1234,
            vocab_base: base,
            vocab_divisor: 128,
            layer_index,
            unigram_vocab: vocab,
        }
    }

    /// A 64-bit LCG, so no test depends on an RNG crate's stream.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            self.0 ^ (self.0 >> 29)
        }
    }

    /// The in-program hash (bit decomposition in `i64`) equals the reference function — at the published
    /// scale (a vocabulary of 248,320, head tables of 20 M rows, 16 heads) as at the tiny fixtures' —
    /// on random tokens, and the program passes the range analysis (no `i64` can overflow, on any params).
    #[test]
    fn the_in_program_hash_equals_the_reference_at_published_scale() {
        for (vocab, base, n, hpn, layer) in
            [(248_320usize, 20_000_000usize, 3usize, 8usize, 0usize), (248_320, 20_000_000, 3, 8, 37), (248_320, 20_000_000, 2, 8, 5), (151_936, 1_000_003, 4, 3, 2), (64, 61, 3, 2, 1)]
        {
            let t = NgramTables::new(&spec(layer, n, hpn, vocab, base));
            let heads = (n - 1) * hpn;
            let mult_max = i64::MAX / vocab as i64;
            let size_hi = t.head_sizes.iter().copied().max().expect("heads");
            let mut pb = ProgramBuilder::new(vocab as u32, HISTORY_BOUND_V1_SMALL);
            let tvp = pb.param("tv", DType::I64, &[n as u32], false);
            let mp = pb.param("mult", DType::I64, &[n as u32], false);
            let sp = pb.param("size", DType::I64, &[heads as u32], false);
            let (pre, root) = {
                let mut b = pb.block("pre", vec![]);
                let tv = b.clamp(tvp, 0, vocab as i64 - 1, DType::I64);
                let ids = hash_ids(&mut b, tv, mp, sp, n, hpn, mult_max, size_hi);
                let tir::Ref::Node(root) = ids else { panic!("a node") };
                (b.finish(&[ids]), root)
            };
            let carry = pb.blocks[pre as usize].nodes.last().expect("a node").out.clone();
            let post = {
                let mut b = pb.block("post", vec![carry.clone()]);
                let l = b.reshape(tir::Ref::CarryIn(0), &carry.shape);
                b.commit(l);
                b.finish(&[])
            };
            let program = pb.finish(pre, vec![], post, 0);
            tir::interval::analyze_ranges(&program).unwrap_or_else(|e| panic!("vocab {vocab} base {base}: the range analysis refuses the hash: {e}"));
            let interp = Interpreter::new(&program).expect("valid");
            let mut rng = Lcg(0x5EED ^ vocab as u64 ^ layer as u64);
            for case in 0..400 {
                let tokens: Vec<i64> = (0..n)
                    .map(|k| match (case % 5, k) {
                        (0, _) => 0,
                        (1, _) => vocab as i64 - 1,
                        _ => (rng.next() % vocab as u64) as i64,
                    })
                    .collect();
                let mut params = MapParams::default();
                params.tensors.insert((0, None), Tensor::new(DType::I64, vec![n], tokens.iter().map(|x| *x as i128).collect()).expect("tv"));
                params.tensors.insert((1, None), Tensor::new(DType::I64, vec![n], t.multipliers.iter().map(|x| *x as i128).collect()).expect("mult"));
                params.tensors.insert((2, None), Tensor::new(DType::I64, vec![heads], t.head_sizes.iter().map(|x| *x as i128).collect()).expect("size"));
                let got = interp.eval_cone(pre, None, root, &params, &ConeEnv { token: Some(0), pos: 0, ..Default::default() }).expect("the cone");
                let want: Vec<i128> = t.ids(tokens[0], &tokens[1..]).iter().zip(&t.head_offsets).map(|(id, off)| (id - off) as i128).collect();
                assert_eq!(got.data, want, "vocab {vocab} n {n} layer {layer} tokens {tokens:?}");
            }
        }
    }
}
