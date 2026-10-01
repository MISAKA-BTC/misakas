//! **Bidirectional encoders** (BERT, RoBERTa, XLM-R) as ONE position over a padded token axis
//! (RFC-0003 Part II.3: "a bidirectional encoder is a single-position program over a padded token
//! axis").
//!
//! The program reads two inputs, declared here as global params and lifted by
//! [`crate::encoder::bidir_v2`]:
//! * `input.ids`, an `idx [L]` (the job's template, padded: `JobTokens`);
//! * `input.count`, an `idx []` (the unpadded length: `JobTokenCount`).
//!
//! Every value is a `[L, …]` tensor: the rows of all tokens at once. Attention is full over the
//! token axis, a fixed axis rather than `H`, with the keys at or past `count` masked
//! (`Compare(Iota < count)` → `Select` to `i32::MIN`, which the softmax's `IntExp` maps to exactly
//! 0). Pad rows are computed like any other and never reach a real row or the pooling.
//!
//! The per-position HL program of the same spec supplies the params, their binding, the
//! occurrences and the fills' float weights. Its blocks are not lowered: the blocks below are.
//! Scales are the calibration's, per site and occurrence, from [`float_forward`]'s statistics,
//! whose site names match the ones here.

use super::*;
use crate::float_ref::{ParamStore, SiteStat};
use crate::spec::{ArchSpec, Ffn, Mixer, NormSpec, Residual};

/// How the class pools the token rows into one vector (sentence-transformers' pooling modes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pooling {
    /// Row 0 (`[CLS]` / `<s>`).
    Cls,
    /// The mean over the `count` real rows.
    Mean,
}

/// The class's choices around the model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BidirCfg {
    /// The padded token axis `L` (the template's `pad.to_len`).
    pub lmax: u32,
    pub pooling: Pooling,
    /// L2-normalise the pooled vector (sentence-transformers' `Normalize`).
    pub normalize: bool,
}

/// The input params' names (lifted into inputs in this order).
pub const IDS_PARAM: &str = "input.ids";
pub const COUNT_PARAM: &str = "input.count";

/// What a bidirectional lowering needs from the spec.
struct Arch {
    heads: u32,
    head_dim: u32,
    scale: f64,
    eps_embed: f64,
    eps_mix: f64,
    eps_ffn: f64,
    act: crate::spec::Act,
    pos_offset: usize,
    /// MPNet's bias over bucketed relative positions.
    rel: Option<crate::spec::RelBiasSpec>,
}

fn arch_of(spec: &ArchSpec) -> Result<Arch> {
    let bad = |m: &str| Err(LowerError::not_lowerable(format!("{}: {m}", spec.architecture)));
    let Some(first) = spec.layers.first() else { return bad("no layers") };
    if spec.layers.iter().any(|l| l != first) {
        return bad("layers of more than one kind");
    }
    let Mixer::Attention(at) = &first.mixer else { return bad("a mixer other than attention") };
    if at.kv_heads != at.heads || at.head_dim != at.v_head_dim || at.qk_norm.is_some() || at.window.is_some() || at.sinks {
        return bad("attention other than plain multi-head");
    }
    if !(at.q_bias && at.k_bias && at.v_bias && at.o_bias) {
        return bad("attention without biases");
    }
    let Ffn::Mlp(mlp) = &first.ffn else { return bad("an FFN other than a plain MLP") };
    if mlp.gated || !(mlp.up_bias && mlp.down_bias) {
        return bad("a gated or bias-free MLP");
    }
    let Residual::PostNorm { mixer_norm, ffn_norm } = first.residual else { return bad("a residual other than post-LN") };
    let e = &spec.embedding;
    let Some(en) = e.norm else { return bad("no embedding LayerNorm") };
    let Some(pos) = &e.positions else { return bad("no learned positions") };
    let layer_eps = |n: NormSpec| -> Result<f64> {
        if n.kind != NormKind::Layer || !n.bias {
            return Err(LowerError::not_lowerable("a norm other than LayerNorm with a bias"));
        }
        Ok(n.eps)
    };
    Ok(Arch {
        heads: at.heads as u32,
        head_dim: at.head_dim as u32,
        scale: at.scale,
        eps_embed: layer_eps(en)?,
        eps_mix: layer_eps(mixer_norm)?,
        eps_ffn: layer_eps(ffn_norm)?,
        act: mlp.act,
        pos_offset: pos.offset,
        rel: e.rel_bias,
    })
}

pub(super) fn hl_param(hl: &HlProgram, name: &str) -> Result<u32> {
    hl.params
        .iter()
        .position(|p| p.name == name)
        .map(|i| i as u32)
        .ok_or_else(|| LowerError::eval(format!("internal: the HL program has no param `{name}`")))
}

/// Lower a bidirectional encoder: pre (embeddings), one block per layer kind, post (pooling). The
/// program's "logits" node is the pooled vector `[1, d]` in the class's fixed point (`Q30` when
/// normalised, a power-of-two unit otherwise).
pub fn lower_bidir(hl: &HlProgram, spec: &ArchSpec, cfg: &BidirCfg) -> Result<Lowered> {
    let a = arch_of(spec)?;
    let l = cfg.lmax;
    let max_rows = spec.embedding.positions.as_ref().map_or(0, |p| p.rows);
    if l == 0 || a.pos_offset + l as usize > max_rows {
        return Err(LowerError::not_lowerable(format!(
            "{}: a padded length of {l} needs positions {}..{} of {max_rows}",
            spec.architecture,
            a.pos_offset,
            a.pos_offset + l as usize
        )));
    }
    let hb = tir::program::HISTORY_BOUND_V1_SMALL;
    let token_bound = u32::try_from(hl.vocab).map_err(|_| LowerError::not_lowerable("vocabulary beyond u32"))?;
    let mut pb = ProgramBuilder::new(token_bound, hb);
    let mut cx = Cx {
        hl,
        fills: Vec::new(),
        row_params: BTreeMap::new(),
        resid_sites: BTreeMap::new(),
        tstate: BTreeMap::new(),
        history_bound: hb,
        max_window: hb,
        logits_key: None,
        site_nodes: BTreeMap::new(),
        shared: Default::default(),
        tables: Default::default(),
        image_rows: None,
        image_cursor: None,
        image_cursor_layer: None,
        split_max_readers: 0,
        quant: BTreeMap::new(),
        table_shift: 0,
        carry_keys: BTreeMap::new(),
    };
    let mut block_map = vec![u8::MAX; hl.blocks.len()];
    let mut order: Vec<usize> = vec![hl.pre];
    for k in &hl.schedule {
        if !order.contains(&(*k as usize)) {
            order.push(*k as usize);
        }
    }
    order.push(hl.post);
    let mut out_node = None;
    for &hbk in &order {
        let (tb, o) = bidir_block(&mut pb, &mut cx, hbk, &a, cfg)?;
        block_map[hbk] = tb;
        if o.is_some() {
            out_node = o;
        }
    }
    let out_node = out_node.ok_or_else(|| LowerError::eval("internal: no output node"))?;
    let layers: Vec<u8> = hl.schedule.iter().map(|k| block_map[*k as usize]).collect();
    let program = pb.finish(block_map[hl.pre], layers, block_map[hl.post], out_node);
    tir::validate::validate(&program)
        .map_err(|e| LowerError::eval(format!("internal: the encoder program is not in normal form: {e}")))?;
    let fills = cx.fills;
    let resid_sites = cx.resid_sites.into_iter().map(|((k, _), f)| (k, f)).collect();
    let logits_key = cx.logits_key.ok_or_else(|| LowerError::eval("internal: no output scale"))?;
    Ok(Lowered { program, fills, row_params: cx.row_params, resid_sites, logits_key, block_map, site_nodes: cx.site_nodes, budget_fallbacks: vec![] })
}

/// A value of this lowering: `[L, n]` (or `[1, n]`) rows.
pub(super) fn rows_val(r: tir::Ref, dt: DType, key: ScaleKey, n: usize, site: &str) -> Val {
    Val { r, dt, key, len: n, site: site.to_string() }
}

pub(super) fn site_key(site: &str) -> ScaleKey {
    ScaleKey::site(vec![site.to_string()], false)
}

/// Record a committed node's site for the per-site diagnosis.
pub(super) fn note_site(cx: &mut Cx<'_>, tb: u8, v: &Val) {
    if let tir::Ref::Node(n) = v.r {
        cx.site_nodes.entry((tb, n)).or_insert((v.site.clone(), v.key.clone(), v.len));
    }
}

fn bidir_block(pb: &mut ProgramBuilder, cx: &mut Cx<'_>, hbk: usize, a: &Arch, cfg: &BidirCfg) -> Result<(u8, Option<u16>)> {
    let hl = cx.hl;
    let blk = &hl.blocks[hbk];
    let d = hl.hidden;
    let l = cfg.lmax;
    let carry_sig = vec![TensorType::fixed(DType::I32, &[l, d as u32])];
    let prefixes: Vec<String> = match blk.role {
        BlockRole::Pre => vec!["pre.".into()],
        BlockRole::Post => vec!["post.".into()],
        BlockRole::Layer => {
            hl.schedule.iter().enumerate().filter(|(_, k)| **k as usize == hbk).map(|(li, _)| format!("L{li}.")).collect()
        }
    };
    let suffix = if blk.role == BlockRole::Layer && hl.schedule.first().map(|k| *k as usize) != Some(hbk) {
        format!("#{hbk}")
    } else {
        String::new()
    };
    let n = blk.nodes.len();
    let mut lb = Lb {
        hb: hbk,
        role: blk.role,
        prefixes,
        vals: Vec::new(),
        wants: vec![None; n],
        rope_sites: vec![Vec::new(); n],
        split: vec![0; n],
        windows: BTreeMap::new(),
        angles: BTreeMap::new(),
        requants: 0,
        absorbed: vec![false; n],
        gdn: BTreeMap::new(),
        ssm: BTreeMap::new(),
        wide: vec![false; n],
        w16: vec![false; n],
        w16_now: false,
        mrope_pos: None,
        suffix,
        appended: BTreeMap::new(),
        carry_in: Vec::new(),
    };
    let tb = pb.blocks.len() as u8;
    let mut b = pb.block(&blk.name, if blk.role == BlockRole::Pre { vec![] } else { carry_sig });
    let resid = ScaleKey::resid();
    match blk.role {
        BlockRole::Pre => {
            // The inputs, declared first so that lifting them leaves every weight's index alone.
            let ids = decl(&mut b, cx, &lb, IDS_PARAM, DType::Idx, &[l as usize], false, input_fill(DType::Idx, vec![l as usize]))?;
            let count = decl(&mut b, cx, &lb, COUNT_PARAM, DType::Idx, &[], false, input_fill(DType::Idx, vec![]))?;
            let _ = count;
            // Word rows, each at its table row's scale, narrowed to the residual scale.
            let tp = hl_param(hl, "embed.table")?;
            let (rows, cols) = (hl.params[tp as usize].shape[0], hl.params[tp as usize].shape[1]);
            let table = decl_rows(&mut b, cx, &lb, "embed.table", &[rows, cols], false, RowKind::T16, tp)?;
            let key = resid.clone();
            let (m, s) = decl_ms(
                &mut b,
                cx,
                &lb,
                "embed",
                rows,
                Arc::new(move |c| {
                    let scales = c.row_scales(tp, true)?;
                    let to = c.scale(&key)?;
                    Ok(scales.iter().map(|sw| sw / to).collect())
                }),
            )?;
            let row = b.gather(table, ids, 0, 0);
            let mt = b.gather(m, ids, 0, 0);
            let st = b.gather(s, ids, 0, 0);
            let mt = b.reshape_fixed(mt, &[l, 1]);
            let st = b.reshape_fixed(st, &[l, 1]);
            let word = narrow(&mut b, row, mt, st, None, DType::I32);
            // Position rows `offset + i` plus the token-type row 0, one param at the residual scale.
            let (pp, tp2) = (hl_param(hl, "embed.pos_table")?, hl_param(hl, "embed.type_table").ok());
            let off = a.pos_offset;
            let lr = l as usize;
            let pos = decl(
                &mut b,
                cx,
                &lb,
                "embed.pos_rows",
                DType::I32,
                &[lr, d],
                false,
                Arc::new(move |c| {
                    let sr = c.scale(&ScaleKey::resid())?;
                    let p = c.f(pp)?;
                    let t = match tp2 {
                        Some(t) => Some(c.f(t)?),
                        None => None,
                    };
                    let mut v = Vec::with_capacity(lr * d);
                    for i in 0..lr {
                        for j in 0..d {
                            let x = p.data[(off + i) * d + j] as f64 + t.as_ref().map_or(0.0, |t| t.data[j] as f64);
                            v.push((x / sr).round().clamp(i32::MIN as f64, i32::MAX as f64) as i32);
                        }
                    }
                    Ok(IntTensor::i32(vec![lr, d], v))
                }),
            )?;
            let sum = b.add(word, pos, DType::I64);
            let sum = b.clamp(sum, i32::MIN as i64, i32::MAX as i64, DType::I32);
            let sum = rows_val(sum, DType::I32, resid.clone(), d, "embed.sum");
            note_resid(cx, &lb, &sum);
            let normed = norm_rows(&mut b, cx, &mut lb, &sum, a.eps_embed, "embed.norm", &Want { dt: DType::I32, key: resid.clone() })?;
            b.commit(normed.r);
            note_resid(cx, &lb, &normed);
            note_site(cx, tb, &normed);
            Ok((b.finish(&[normed.r]), None))
        }
        BlockRole::Layer => {
            let x = rows_val(tir::Ref::CarryIn(0), DType::I32, resid.clone(), d, "carry0");
            let xc = codes_rows(&mut b, cx, &mut lb, &x)?;
            let (h, dh) = (a.heads, a.head_dim);
            // q, k, v: codes [L, d], committed head-major [h, L, dh] (one head's K/V is whole leaves).
            let heads_of = |b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, name: &str| -> Result<Val> {
                let v = linear_rows(b, cx, lb, &xc, &format!("{name}.w"), Some(&format!("{name}.b")), name, &Want { dt: DType::I16, key: site_key(name) })?;
                let r = b.reshape_fixed(v.r, &[l, h, dh]);
                let r = b.transpose(r, &[1, 0, 2]);
                b.commit(r);
                let hv = Val { r, ..v };
                note_site(cx, tb, &hv);
                Ok(hv)
            };
            let q = heads_of(&mut b, cx, &mut lb, "attn.q")?;
            let k = heads_of(&mut b, cx, &mut lb, "attn.k")?;
            let v = heads_of(&mut b, cx, &mut lb, "attn.v")?;
            // Scores: q·kᵀ (exact i64) to Q14 logits, the 1/√d scale and both code scales in m.
            let kt = b.transpose(k.r, &[0, 2, 1]);
            let s = b.matmul(q.r, kt, DType::I64);
            let (kq, kk, sc) = (q.key.clone(), k.key.clone(), a.scale);
            let (m, sh) = decl_ms(
                &mut b,
                cx,
                &lb,
                "attn.score",
                1,
                Arc::new(move |c| Ok(vec![c.scale(&kq)? * c.scale(&kk)? * sc * (1u64 << LOGIT_Q) as f64])),
            )?;
            let mut logits = narrow(&mut b, s, m, sh, None, DType::I32);
            // MPNet: `table[bucket(j − i), head]` in Q`LOGIT_Q` on the scaled scores, one table and
            // one pinned `[L, L]` bucket map for every layer.
            if let Some(rb) = a.rel {
                let tp = hl_param(hl, "attn.rel_bias")?;
                let (nb, hh) = (rb.buckets, h as usize);
                let table = decl(
                    &mut b,
                    cx,
                    &lb,
                    "attn.rel_bias.q",
                    DType::I32,
                    &[nb, hh],
                    false,
                    Arc::new(move |c| {
                        let t = c.f(tp)?;
                        Ok(IntTensor::i32(
                            vec![nb, hh],
                            t.data.iter().map(|v| (*v as f64 * (1u64 << LOGIT_Q) as f64).round().clamp(i32::MIN as f64, i32::MAX as f64) as i32).collect(),
                        ))
                    }),
                )?;
                let lu = l as usize;
                let ids: Vec<u32> = (0..lu)
                    .flat_map(|i| (0..lu).map(move |j| super::encdec::t5_bucket(j as i64 - i as i64, true, rb.buckets, rb.max_distance) as u32))
                    .collect();
                let bk = decl(&mut b, cx, &lb, "attn.buckets", DType::Idx, &[lu, lu], false, Arc::new(move |_| Ok(IntTensor::idx(vec![lu, lu], ids.clone()))))?;
                let bk = b.clamp(bk, 0, nb as i64 - 1, DType::Idx);
                let bias = b.gather(table, bk, 0, 0); // [L, L, h]
                let bias = b.transpose(bias, &[2, 0, 1]);
                let sum = b.add(logits, bias, DType::I64);
                logits = b.clamp(sum, i32::MIN as i64, i32::MAX as i64, DType::I32);
            }
            // The mask: keys at or past `count` score i32::MIN, which IntExp maps to exactly 0.
            let count = b.pb.params.iter().position(|p| p.name == COUNT_PARAM);
            let count = match count {
                Some(i) => tir::Ref::Param(i as u16),
                None => return Err(LowerError::eval("internal: no count input")),
            };
            let iota = b.iota(DType::Idx, &[Dim::Fixed(l)], 0, 0, 1);
            let keep = b.compare(iota, count, tir::Cmp::Lt);
            let neg = b.c(DType::I32, i32::MIN as i128);
            let masked = b.select(keep, logits, neg, DType::I32);
            let p = softmax_rows(&mut b, masked, h, l, dh);
            // Context: P·V (Q24 × codes, exact i64) back to codes, [h, L, dh] → [L, h·dh].
            let o = b.matmul(p, v.r, DType::I64);
            let ctx_key = site_key("attn.ctx");
            let (kv, kc) = (v.key.clone(), ctx_key.clone());
            let (m, sh) = decl_ms(
                &mut b,
                cx,
                &lb,
                "attn.ctx",
                1,
                Arc::new(move |c| Ok(vec![c.scale(&kv)? / (1u64 << 24) as f64 / c.scale(&kc)?])),
            )?;
            let ctx = narrow(&mut b, o, m, sh, None, DType::I16);
            b.commit(ctx);
            let ctxv = rows_val(ctx, DType::I16, ctx_key, (h * dh) as usize, "attn.ctx");
            note_site(cx, tb, &ctxv);
            let ctx = b.transpose(ctx, &[1, 0, 2]);
            let ctx = b.reshape_fixed(ctx, &[l, h * dh]);
            let ctxv = Val { r: ctx, ..ctxv };
            let att = linear_rows(&mut b, cx, &mut lb, &ctxv, "attn.o.w", Some("attn.o.b"), "attn.o", &Want { dt: DType::I32, key: resid.clone() })?;
            // Post-LN: x = LN(x + attn), x = LN(x + ffn).
            let r1 = add_rows(&mut b, cx, &lb, &x, &att, "resid.mix")?;
            if resid_commit_needed(l, d, (h * dh) as usize) {
                b.commit(r1.r);
            }
            let x1 = norm_rows(&mut b, cx, &mut lb, &r1, a.eps_mix, "norm.mix", &Want { dt: DType::I32, key: resid.clone() })?;
            b.commit(x1.r);
            note_resid(cx, &lb, &x1);
            note_site(cx, tb, &x1);
            let x1c = codes_rows(&mut b, cx, &mut lb, &x1)?;
            let up = linear_rows(&mut b, cx, &mut lb, &x1c, "mlp.up.w", Some("mlp.up.b"), "mlp.up", &Want { dt: DType::I16, key: site_key("mlp.up") })?;
            let act = lower_table_named(&mut b, cx, &mut lb, &up, TableFn::Act(a.act), "mlp.act")?;
            note_site(cx, tb, &act);
            let down = linear_rows(&mut b, cx, &mut lb, &act, "mlp.down.w", Some("mlp.down.b"), "mlp.down", &Want { dt: DType::I32, key: resid.clone() })?;
            let r2 = add_rows(&mut b, cx, &lb, &x1, &down, "resid.ffn")?;
            let inter = hl.params[hl_param(hl, "mlp.down.w")? as usize].shape[1];
            if resid_commit_needed(l, d, inter) {
                b.commit(r2.r);
            }
            let x2 = norm_rows(&mut b, cx, &mut lb, &r2, a.eps_ffn, "norm.ffn", &Want { dt: DType::I32, key: resid.clone() })?;
            note_resid(cx, &lb, &x2);
            note_site(cx, tb, &x2);
            Ok((b.finish(&[x2.r]), None))
        }
        BlockRole::Post => {
            let x = rows_val(tir::Ref::CarryIn(0), DType::I32, resid.clone(), d, "carry0");
            let pooled = match cfg.pooling {
                Pooling::Cls => b.slice(x.r, 0, 0, 1),
                Pooling::Mean => {
                    let count = match b.pb.params.iter().position(|p| p.name == COUNT_PARAM) {
                        Some(i) => tir::Ref::Param(i as u16),
                        None => return Err(LowerError::eval("internal: no count input")),
                    };
                    let iota = b.iota(DType::Idx, &[Dim::Fixed(l), Dim::Fixed(1)], 0, 0, 1);
                    let keep = b.compare(iota, count, tir::Cmp::Lt);
                    let zero = b.c(DType::I32, 0);
                    let kept = b.select(keep, x.r, zero, DType::I32);
                    let sum = b.reduce_sum(kept, 0, DType::I64);
                    // `count ≥ 1` for every template (it holds the template's own ids); the clamp
                    // keeps the division total.
                    let n = b.clamp(count, 1, l as i64, DType::I64);
                    let mean = b.div(sum, n, tir::Rounding::HalfAwayFromZero, DType::I64);
                    b.clamp(mean, i32::MIN as i64, i32::MAX as i64, DType::I32)
                }
            };
            b.commit(pooled);
            let pv = rows_val(pooled, DType::I32, resid.clone(), d, "pool");
            note_resid(cx, &lb, &pv);
            note_site(cx, tb, &pv);
            let out = if cfg.normalize {
                let pc = codes_rows(&mut b, cx, &mut lb, &pv)?;
                let u = b.l2_unit_q15(pc.r);
                let key = ScaleKey { base: Base::Fixed(1.0 / (1u64 << 30) as f64), factor: 1.0 };
                let f = b.c(DType::I32, 1 << 15);
                let r = b.mul(u, f, DType::I32);
                rows_val(r, DType::I32, key, d, "embed.normed")
            } else {
                let key = ScaleKey { base: Base::Pow2Site { names: vec!["pool".into()] }, factor: 1.0 };
                coerce(&mut b, cx, &mut lb, &pv, DType::I32, &key)?
            };
            let out = ensure_node(&mut b, &out);
            let tir::Ref::Node(oi) = out.r else { unreachable!("ensure_node") };
            b.commit(out.r);
            note_site(cx, tb, &out);
            cx.logits_key = Some(out.key.clone());
            Ok((b.finish(&[]), Some(oi)))
        }
    }
}

/// A zero-filled input param (the program's inputs are lifted before it runs; a version-1 run of
/// the lowered program gets its inputs by overwriting these).
pub(super) fn input_fill(dt: DType, shape: Vec<usize>) -> FillFn {
    Arc::new(move |_c| {
        let n: usize = shape.iter().product();
        Ok(match dt {
            DType::Idx => IntTensor { dtype: DType::Idx, shape: shape.clone(), data: crate::lower::IntData::Idx(vec![0; n]) },
            DType::I16 => IntTensor::i16(shape.clone(), vec![0; n]),
            _ => IntTensor::i32(shape.clone(), vec![0; n]),
        })
    })
}

/// `i16` codes of a row value at its own site's scale (the rows' version of `codes`).
pub(super) fn codes_rows(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, v: &Val) -> Result<Val> {
    if v.dt == DType::I16 {
        return Ok(v.clone());
    }
    let key = site_key(&v.site);
    let (from, to) = (v.key.clone(), key.clone());
    let (m, s) = decl_ms(b, cx, lb, &format!("{}.rq", v.site), 1, Arc::new(move |c| Ok(vec![c.scale(&from)? / c.scale(&to)?])))?;
    let r = narrow(b, v.r, m, s, None, DType::I16);
    b.commit(r);
    Ok(Val { r, dt: DType::I16, key, len: v.len, site: v.site.clone() })
}

/// `x + y` of two residual-scale rows.
pub(super) fn add_rows(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &Lb, x: &Val, y: &Val, site: &str) -> Result<Val> {
    if !x.key.same(&ScaleKey::resid()) || !y.key.same(&ScaleKey::resid()) {
        return Err(LowerError::eval(format!("internal: `{site}` adds values off the residual scale")));
    }
    let s = b.add(x.r, y.r, DType::I64);
    let r = b.clamp(s, i32::MIN as i64, i32::MAX as i64, DType::I32);
    let v = rows_val(r, DType::I32, ScaleKey::resid(), x.len, site);
    note_resid(cx, lb, &v);
    Ok(v)
}

/// `x·Wᵀ + b` over rows: `x:code[L, in]`, `W:i8[out, in]` per-row codes, stored transposed as
/// `[in, out]` so the product is one `MatMul` into `[L, out]`; narrowed per output channel.
#[allow(clippy::too_many_arguments)]
pub(super) fn linear_rows(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    x: &Val,
    w_name: &str,
    b_name: Option<&str>,
    site: &str,
    want: &Want,
) -> Result<Val> {
    let hl = cx.hl;
    let w = hl_param(hl, w_name)?;
    let (out, inp) = (hl.params[w as usize].shape[0], hl.params[w as usize].shape[1]);
    if x.dt != DType::I16 || x.len != inp {
        return Err(LowerError::eval(format!("internal: `{site}` reads {:?} rows of {}, W has {inp} columns", x.dt, x.len)));
    }
    let pl = per_layer(lb);
    let wt = decl(
        b,
        cx,
        lb,
        &format!("{w_name}.t"),
        DType::I8,
        &[inp, out],
        pl,
        Arc::new(move |c| {
            let rc = c.rows(w)?;
            let mut t = vec![0i8; inp * out];
            for o in 0..out {
                for i in 0..inp {
                    t[i * out + o] = rc.codes[o * inp + i];
                }
            }
            Ok(IntTensor::i8(vec![inp, out], t))
        }),
    )?;
    let (kx, ky) = (x.key.clone(), want.key.clone());
    let (m, s) = decl_ms(
        b,
        cx,
        lb,
        site,
        out,
        Arc::new(move |c| {
            let scales = c.rows(w)?.scales.clone();
            let (sx, sy) = (c.scale(&kx)?, c.scale_vec(&ky, out)?);
            Ok(scales.iter().zip(&sy).map(|(sw, sy)| sw * sx / sy).collect())
        }),
    )?;
    let z = match b_name {
        Some(bn) => {
            let bp = hl_param(hl, bn)?;
            let ky = want.key.clone();
            Some(decl(
                b,
                cx,
                lb,
                &format!("{site}.z"),
                DType::I64,
                &[out],
                pl,
                Arc::new(move |c| {
                    let bv = c.f(bp)?;
                    let sy = c.scale_vec(&ky, out)?;
                    Ok(IntTensor::i64(vec![out], bv.data.iter().zip(&sy).map(|(v, s)| (*v as f64 / s).round() as i64).collect()))
                }),
            )?)
        }
        None => None,
    };
    let acc = b.matmul(x.r, wt, DType::I64);
    let r = narrow(b, acc, m, s, z, want.dt);
    if want.dt == DType::I16 {
        b.commit(r);
    }
    let v = rows_val(r, want.dt, want.key.clone(), out, site);
    note_resid(cx, lb, &v);
    Ok(v)
}

/// LayerNorm along the last axis of `[L, n]` rows (gain and bias from `{site}.gain`/`{site}.bias`):
/// the decoder's exact centring (`c = n·x − Σx`, brought into `i32` by `2^k`), the Q24 unit row,
/// then one per-channel narrowing with the gain as `m` and the bias as `z`.
fn norm_rows(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, x: &Val, eps: f64, site: &str, want: &Want) -> Result<Val> {
    norm_rows_kind(b, cx, lb, x, NormKind::Layer, eps, site, true, want)
}

/// LayerNorm or RMSNorm along the last axis of `[L, n]` rows, gain `{site}.gain` and (when
/// `bias`) bias `{site}.bias`. LayerNorm centres exactly (`c = n·x − Σx`, brought into `i32` by
/// `2^k`); both then take the Q24 unit row and one per-channel narrowing with the gain as `m`.
#[allow(clippy::too_many_arguments)]
pub(super) fn norm_rows_kind(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    x: &Val,
    kind: NormKind,
    eps: f64,
    site: &str,
    bias: bool,
    want: &Want,
) -> Result<Val> {
    let hl = cx.hl;
    let n = x.len;
    let gp = hl_param(hl, &format!("{site}.gain"))?;
    let bp = if bias { Some(hl_param(hl, &format!("{site}.bias"))?) } else { None };
    let in_bits: i32 = if x.dt == DType::I16 { 16 } else { 32 };
    let k = match kind {
        NormKind::Layer => (in_bits + (n as f64).log2().ceil() as i32 - 31).max(0) as u32,
        NormKind::Rms => 0,
    };
    let kx = x.key.clone();
    let eps_q = Arc::new(move |c: &FillCtx<'_>| -> Result<f64> {
        let sx = c.scale(&kx)?;
        let base = eps * (1u64 << 24) as f64 / (sx * sx);
        Ok(match kind {
            NormKind::Layer => base * (n * n) as f64 / 4f64.powi(k as i32),
            NormKind::Rms => base,
        })
    });
    let eps_p = decl_eps(b, cx, lb, site, eps_q)?;
    let c = match kind {
        NormKind::Layer => {
            let axis = b.shape(x.r).len() - 1;
            let nn = b.c(DType::I64, n as i128);
            let nx = b.mul(x.r, nn, DType::I64);
            let sum = b.reduce_sum(x.r, axis, DType::I64);
            let c = b.sub(nx, sum, DType::I64);
            let c = if k > 0 { b.shr(c, k, tir::Rounding::HalfAwayFromZero, DType::I64) } else { c };
            b.clamp(c, i32::MIN as i64, i32::MAX as i64, DType::I32)
        }
        NormKind::Rms => x.r,
    };
    let u = rms_unit(b, c, eps_p);
    let ky = want.key.clone();
    let (m, s) = decl_ms(
        b,
        cx,
        lb,
        site,
        n,
        Arc::new(move |c| {
            let sy = c.scale_vec(&ky, n)?;
            let g = c.f(gp)?.data.clone();
            Ok((0..n).map(|i| g[i] as f64 / (1u64 << 24) as f64 / sy[i]).collect())
        }),
    )?;
    let ky = want.key.clone();
    let pl = per_layer(lb);
    let z = match bp {
        Some(bp) => Some(decl(
            b,
            cx,
            lb,
            &format!("{site}.z"),
            DType::I64,
            &[n],
            pl,
            Arc::new(move |c| {
                let bv = c.f(bp)?;
                let sy = c.scale_vec(&ky, n)?;
                Ok(IntTensor::i64(vec![n], (0..n).map(|i| (bv.data[i] as f64 / sy[i]).round() as i64).collect()))
            }),
        )?),
        None => None,
    };
    let r = narrow(b, u, m, s, z, want.dt);
    if want.dt == DType::I16 {
        b.commit(r);
    }
    Ok(Val { r, dt: want.dt, key: want.key.clone(), len: n, site: site.to_string() })
}

// ───────────────────────────── admission at real sizes ─────────────────────────────
//
// Admission charges a commit point's tile by box demand (spec 04b §10.3): an operand of a `MatMul`
// is demanded `d · contraction` elements for `d` demanded outputs, and a reduction's operand `d`
// times the reduced axis. Over a fixed token axis that widens two cones past the legacy court's
// tile ceilings at real sizes (BERT-base at 128 tokens: 75 Mi MACs a tile, against 16 Mi):
// * the softmax's row maximum and row sum, broadcast back over their rows, reach the whole
//   `[h, L, L]` score matrix from any tile downstream of it ([`split_softmax`]);
// * a norm reduces a whole row, so a tile of it reaches `d` columns of every row of the projection
//   its input came from ([`resid_commit_needed`]).
// The fix is commit points, which change no value: the masked logits, the row maximum and the row
// reciprocal, and the residual sum a norm reads.

/// Whether a softmax over a fixed axis of `l` keys, `h` heads of `dh`, is split at commit points
/// ([`softmax_committed`]): unsplit, a tile of anything downstream costs `h·L²·dh` MACs and `h·L²`
/// exponentials. Past half the legacy court's tile ceilings it is split.
pub(super) fn split_softmax(h: u32, l: u32, dh: u32) -> bool {
    let c = tir::admit::TirCeilingsV1::legacy_court_v1();
    let (h, l, dh) = (h as u64, l as u64, dh as u64);
    h * l * l * dh > c.max_tile_macs / 2 || h * l * l > c.max_tile_transcendentals / 2
}

/// The library's `softmax_shifted` over the last axis, bit for bit, with its input (the masked
/// logits), the row maximum and the row reciprocal as commit points. A tile of anything downstream
/// then recomputes `L` exponentials a demanded row and opens those rows; a logits tile costs one
/// score's `dh` MACs. The reciprocal is `i32` (`[2^24 / L, 2^24]`: the row maximum contributes
/// `IntExp(0) = 2^24` to the sum), so its clamp never fires.
pub(super) fn softmax_committed(b: &mut BlockBuilder<'_>, x: tir::Ref, up_bits: u32) -> tir::Ref {
    b.commit(x);
    let axis = b.shape(x).len() - 1;
    let up = up_bits.min(62);
    let max = b.reduce_max(x, axis);
    b.commit(max);
    let diff = b.sub(x, max, DType::I64);
    let d = b.clamp(diff, (i32::MIN as i64) >> up, 0, DType::I64);
    let scale = b.c(DType::I64, 1i128 << up);
    let w = b.mul(d, scale, DType::I64);
    let arg = b.clamp(w, i32::MIN as i64, 0, DType::I32);
    let e = b.int_exp(arg);
    let sum = b.reduce_sum(e, axis, DType::I64);
    let recip = b.int_recip(sum);
    let recip = b.clamp(recip, 0, i32::MAX as i64, DType::I32);
    b.commit(recip);
    let p = b.mul(e, recip, DType::I128);
    let q = b.shr(p, tir::arith::K, tir::Rounding::Floor, DType::I64);
    b.clamp(q, 0, 1 << 25, DType::I32)
}

/// The softmax of an encoder's attention over its `l` rows: split at commit points when
/// [`split_softmax`] says so, else the library's.
pub(super) fn softmax_rows(b: &mut BlockBuilder<'_>, x: tir::Ref, h: u32, l: u32, dh: u32) -> tir::Ref {
    if split_softmax(h, l, dh) { softmax_committed(b, x, 24 - LOGIT_Q) } else { b.softmax_shifted(x, 24 - LOGIT_Q) }
}

/// Whether the residual sum a norm reads is committed: `x + y` with `y` a projection over a
/// `contraction`-wide input, rows of `n`. Unsplit, a tile of the norm (64 lanes) reaches
/// `min(64, L)·n` outputs of the projection, each `contraction` MACs.
pub(super) fn resid_commit_needed(l: u32, n: usize, contraction: usize) -> bool {
    let c = tir::admit::TirCeilingsV1::legacy_court_v1();
    (l.min(64) as u64) * n as u64 * contraction as u64 > c.max_tile_macs / 2
}

// ───────────────────────────── the float reference ─────────────────────────────

/// One padded sequence: the ids `[L]` and the unpadded length.
#[derive(Clone, Debug)]
pub struct Padded {
    pub ids: Vec<usize>,
    pub count: usize,
}

/// The float encoder over one padded sequence: the pooled output (normalised when the class
/// says), and, when `stats` is given, every site's statistics over the real rows (keys as the
/// lowering's: `pre.embed.sum`, `L3.attn.q`, `post.pool`, …).
pub fn float_forward(
    hl: &HlProgram,
    spec: &ArchSpec,
    cfg: &BidirCfg,
    params: &ParamStore,
    seq: &Padded,
    mut stats: Option<&mut BTreeMap<String, SiteStat>>,
) -> Result<Vec<f64>> {
    let a = arch_of(spec)?;
    let (l, d) = (cfg.lmax as usize, hl.hidden);
    if seq.ids.len() != l || seq.count == 0 || seq.count > l {
        return Err(LowerError::eval(format!("a padded sequence of {} ids with {} real ones for L = {l}", seq.ids.len(), seq.count)));
    }
    let p = |name: &str, layer: Option<usize>| -> Result<Vec<f64>> {
        let i = hl_param(hl, name)?;
        let t = params.get(i, layer)?;
        Ok(t.data.iter().map(|x| *x as f64).collect())
    };
    let n_real = seq.count;
    let mut observe = |key: String, rows: &[Vec<f64>]| {
        if let Some(st) = stats.as_deref_mut() {
            let e = st.entry(key).or_default();
            let width = rows.first().map_or(0, |r| r.len());
            if e.count == 0 {
                e.chan_absmax = vec![0.0; width];
            }
            for (i, r) in rows.iter().enumerate() {
                let mut row = 0f64;
                for (c, x) in r.iter().enumerate() {
                    let ax = x.abs();
                    row = row.max(ax);
                    e.sum_sq += ax * ax;
                    if let Some(ch) = e.chan_absmax.get_mut(c) {
                        *ch = ch.max(ax as f32);
                    }
                }
                e.absmax = e.absmax.max(row);
                if i == 0 {
                    e.pos0_absmax = e.pos0_absmax.max(row);
                } else {
                    e.rest_absmax = e.rest_absmax.max(row);
                }
                e.count += r.len() as u64;
            }
        }
    };
    let ln = |x: &[f64], g: &[f64], bias: &[f64], eps: f64| -> Vec<f64> {
        let n = x.len() as f64;
        let mu = x.iter().sum::<f64>() / n;
        let var = x.iter().map(|v| (v - mu) * (v - mu)).sum::<f64>() / n;
        let inv = 1.0 / (var + eps).sqrt();
        x.iter().zip(g).zip(bias).map(|((v, g), b)| (v - mu) * inv * g + b).collect()
    };
    let lin = |x: &[f64], w: &[f64], bias: &[f64], out: usize| -> Vec<f64> {
        let inp = x.len();
        (0..out).map(|o| bias[o] + (0..inp).map(|i| w[o * inp + i] * x[i]).sum::<f64>()).collect()
    };
    // Embeddings.
    let (word, pos) = (p("embed.table", None)?, p("embed.pos_table", None)?);
    let typ = p("embed.type_table", None).ok();
    let mut x: Vec<Vec<f64>> = (0..l)
        .map(|i| (0..d).map(|j| word[seq.ids[i] * d + j] + pos[(a.pos_offset + i) * d + j] + typ.as_ref().map_or(0.0, |t| t[j])).collect())
        .collect();
    observe("pre.embed.sum".into(), &x[..n_real]);
    let (g, bb) = (p("embed.norm.gain", None)?, p("embed.norm.bias", None)?);
    x = x.iter().map(|r| ln(r, &g, &bb, a.eps_embed)).collect();
    observe("pre.embed.norm".into(), &x[..n_real]);
    let (h, dh) = (a.heads as usize, a.head_dim as usize);
    for li in 0..hl.schedule.len() {
        let pre = format!("L{li}.");
        let ly = Some(li);
        observe(format!("{pre}carry0"), &x[..n_real]);
        let proj = |name: &str, x: &[Vec<f64>], out: usize| -> Result<Vec<Vec<f64>>> {
            let (w, bias) = (p(&format!("{name}.w"), ly)?, p(&format!("{name}.b"), ly)?);
            Ok(x.iter().map(|r| lin(r, &w, &bias, out)).collect())
        };
        let (q, k, v) = (proj("attn.q", &x, h * dh)?, proj("attn.k", &x, h * dh)?, proj("attn.v", &x, h * dh)?);
        observe(format!("{pre}attn.q"), &q[..n_real]);
        observe(format!("{pre}attn.k"), &k[..n_real]);
        observe(format!("{pre}attn.v"), &v[..n_real]);
        let rel = match a.rel {
            Some(rb) => Some((p("attn.rel_bias", None)?, rb)),
            None => None,
        };
        let mut ctx = vec![vec![0f64; h * dh]; l];
        for hh in 0..h {
            for i in 0..l {
                let sc: Vec<f64> = (0..n_real)
                    .map(|j| {
                        let bias = rel.as_ref().map_or(0.0, |(t, rb)| {
                            t[super::encdec::t5_bucket(j as i64 - i as i64, true, rb.buckets, rb.max_distance) * h + hh]
                        });
                        (0..dh).map(|t| q[i][hh * dh + t] * k[j][hh * dh + t]).sum::<f64>() * a.scale + bias
                    })
                    .collect();
                let mx = sc.iter().cloned().fold(f64::MIN, f64::max);
                let e: Vec<f64> = sc.iter().map(|s| (s - mx).exp()).collect();
                let z: f64 = e.iter().sum();
                for t in 0..dh {
                    ctx[i][hh * dh + t] = (0..n_real).map(|j| e[j] / z * v[j][hh * dh + t]).sum();
                }
            }
        }
        observe(format!("{pre}attn.ctx"), &ctx[..n_real]);
        let o = proj("attn.o", &ctx, d)?;
        observe(format!("{pre}attn.o"), &o[..n_real]);
        let r1: Vec<Vec<f64>> = x.iter().zip(&o).map(|(a, b)| a.iter().zip(b).map(|(a, b)| a + b).collect()).collect();
        observe(format!("{pre}resid.mix"), &r1[..n_real]);
        let (g, bb) = (p("norm.mix.gain", ly)?, p("norm.mix.bias", ly)?);
        let x1: Vec<Vec<f64>> = r1.iter().map(|r| ln(r, &g, &bb, a.eps_mix)).collect();
        observe(format!("{pre}norm.mix"), &x1[..n_real]);
        let inter = hl.params[hl_param(hl, "mlp.up.w")? as usize].shape[0];
        let up = proj("mlp.up", &x1, inter)?;
        observe(format!("{pre}mlp.up"), &up[..n_real]);
        let act: Vec<Vec<f64>> = up.iter().map(|r| r.iter().map(|v| crate::float_ref::act(a.act, *v as f32) as f64).collect()).collect();
        observe(format!("{pre}mlp.act"), &act[..n_real]);
        let down = proj("mlp.down", &act, d)?;
        observe(format!("{pre}mlp.down"), &down[..n_real]);
        let r2: Vec<Vec<f64>> = x1.iter().zip(&down).map(|(a, b)| a.iter().zip(b).map(|(a, b)| a + b).collect()).collect();
        observe(format!("{pre}resid.ffn"), &r2[..n_real]);
        let (g, bb) = (p("norm.ffn.gain", ly)?, p("norm.ffn.bias", ly)?);
        x = r2.iter().map(|r| ln(r, &g, &bb, a.eps_ffn)).collect();
        observe(format!("{pre}norm.ffn"), &x[..n_real]);
    }
    observe("post.carry0".into(), &x[..n_real]);
    let pooled: Vec<f64> = match cfg.pooling {
        Pooling::Cls => x[0].clone(),
        Pooling::Mean => (0..d).map(|j| (0..n_real).map(|i| x[i][j]).sum::<f64>() / n_real as f64).collect(),
    };
    observe("post.pool".into(), std::slice::from_ref(&pooled));
    Ok(if cfg.normalize {
        let nrm = pooled.iter().map(|v| v * v).sum::<f64>().sqrt().max(1e-12);
        let out: Vec<f64> = pooled.iter().map(|v| v / nrm).collect();
        observe("post.embed.normed".into(), std::slice::from_ref(&out));
        out
    } else {
        pooled
    })
}
