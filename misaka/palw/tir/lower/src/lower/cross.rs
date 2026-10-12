//! **`ATTN_CROSS_V1`, lowered** (FR-21, Mllama): the vision states are a DECLARED INPUT, never computed here (RFC-0003 §II.2.1: the
//! tower's projected rows, decoded integers, `input.cross_states`, `i32 [rows, hidden]` at a fixed unit). Two programs:
//!
//! * **stage 0** ([`lower_cross_kv`]): one position over the rows, out the stack of every cross layer's keys and values
//!   `i16 [Dc, 2, rows, inner]` — `K = RMS_head(Wk·s)` with the layer's `k_norm`, `V = Wv·s` — each narrowed to its layer's own code
//!   scale (the statistics the float text stage records at the layer's `xattn.ctx.k` / `.v` sites, read through [`Base::At`]);
//! * **the text stage** (a generic decoder program whose `Op::CrossAttention` calls [`lower_cross_attention`]): a layer gathers its
//!   `[2, rows, inner]` slice of the stack input `input.xkv` by a per-layer index, masked by nothing (every text position sees every
//!   row), softmax over the rows, grouped heads, no rotation.
//!
//! The pattern is FR-18's (the encoder-decoder's stacked cross K/V). The binding of `input.cross_states` is the class's: the vision
//! stage's rows (`JobImage` tower) in a full pipeline; a standalone run binds the tensor directly.

use super::bidir::{codes_rows, input_fill, linear_rows, norm_rows_kind, note_site, rows_val, site_key};
use super::encdec::{finish, new_cx, new_lb};
use super::*;
use crate::spec::{CrossAttnSpec, ModelSpec, NormKind};
use crate::weights::{Binding, Src};

/// The declared input of stage 0: the projected vision rows, `i32 [rows, hidden]` in a fixed point of `unit`.
pub const STATES_PARAM: &str = "input.cross_states";
/// The declared input of the text stage: stage 0's stack, `i16 [Dc, 2, rows, inner]`.
pub const XKV_PARAM: &str = "input.xkv";

/// The text stage's cross-attention over stage 0's stack (`Op::CrossAttention`).
#[allow(clippy::too_many_arguments)]
pub(super) fn lower_cross_attention(
    b: &mut BlockBuilder<'_>,
    cx: &mut Cx<'_>,
    lb: &mut Lb,
    q: &Val,
    heads: usize,
    kv: usize,
    hd: usize,
    scale: f64,
    rows: usize,
    slots: &[usize],
    site: &str,
    want: &Want,
) -> Result<Val> {
    let (inner, dc) = (kv * hd, slots.len());
    if q.len != heads * hd || q.dt != DType::I16 || q.key.split() != 0 || dc == 0 || heads % kv != 0 {
        return Err(LowerError::eval(format!("internal: cross-attention `{site}` reads {:?} of {} for {heads}×{hd}", q.dt, q.len)));
    }
    let (g, kv32, hd32, n32) = ((heads / kv) as u32, kv as u32, hd as u32, rows as u32);
    let shape = vec![dc, 2, rows, inner];
    let xkv = decl(b, cx, lb, XKV_PARAM, DType::I16, &shape, false, input_fill(DType::I16, shape.clone()))?;
    let slots_v = slots.to_vec();
    let li = decl(
        b,
        cx,
        lb,
        &format!("{site}.slot"),
        DType::Idx,
        &[],
        true,
        Arc::new(move |c| Ok(IntTensor::idx(vec![], vec![slots_v.iter().position(|m| Some(*m) == c.layer).unwrap_or(0) as u32]))),
    )?;
    let li = b.clamp(li, 0, dc as i64 - 1, DType::Idx);
    let kvs = b.gather(xkv, li, 0, 0); // [2, rows, inner]
    let kk = b.slice(kvs, 0, 0, 1);
    let kk = b.reshape_fixed(kk, &[n32, kv32, hd32]);
    let kt = b.transpose(kk, &[1, 2, 0]); // [kv, hd, rows]
    let vv = b.slice(kvs, 0, 1, 1);
    let vv = b.reshape_fixed(vv, &[n32, kv32, hd32]);
    let vt = b.transpose(vv, &[1, 0, 2]); // [kv, rows, hd]
    let qg = b.reshape_fixed(q.r, &[kv32, g, hd32]);
    let sc = b.matmul(qg, kt, DType::I64); // [kv, g, rows]
    let (kq, kk_key, vk_key) = (q.key.clone(), site_key(&format!("{site}.k")), site_key(&format!("{site}.v")));
    let (m, s) = decl_ms(b, cx, lb, &format!("{site}.scores"), 1, Arc::new(move |c| Ok(vec![c.scale(&kq)? * c.scale(&kk_key)? * scale * (1u64 << LOGIT_Q) as f64])))?;
    let logits = narrow(b, sc, m, s, None, DType::I32);
    let pm = b.softmax_shifted(logits, 24 - LOGIT_Q);
    let o = b.matmul(pm, vt, DType::I64); // [kv, g, hd]
    let out_key = if want.dt == DType::I16 { want.key.clone() } else { site_key(site) };
    let ko = out_key.clone();
    let (m, s) = decl_ms(b, cx, lb, site, 1, Arc::new(move |c| Ok(vec![c.scale(&vk_key)? / (1u64 << 24) as f64 / c.scale(&ko)?])))?;
    let r = narrow(b, o, m, s, None, DType::I16);
    b.commit(r);
    let r = b.reshape_fixed(r, &[(heads * hd) as u32]);
    Ok(Val { r, dt: DType::I16, key: out_key, len: heads * hd, site: site.to_string() })
}

/// What stage 0 needs of a spec: the cross layers (model indices), their one shape, the rows.
#[derive(Clone, Debug)]
pub struct CrossKv {
    pub slots: Vec<usize>,
    pub spec: CrossAttnSpec,
    pub rows: usize,
    pub hidden: usize,
}

impl CrossKv {
    pub fn of(spec: &ModelSpec) -> Result<CrossKv> {
        let rows = spec.cross_states.ok_or_else(|| LowerError::not_lowerable("ATTN_CROSS_V1: no states are declared (`ModelSpec::cross_states`)"))?.rows;
        let mut slots = Vec::new();
        let mut one: Option<CrossAttnSpec> = None;
        for (i, l) in spec.layers.iter().enumerate() {
            if let crate::spec::Mixer::CrossAttention(c) = &l.mixer {
                match &one {
                    Some(o) if o != c => return Err(LowerError::not_lowerable("ATTN_CROSS_V1: cross layers of different shapes share one stack")),
                    _ => one = Some(c.clone()),
                }
                slots.push(i);
            }
        }
        let spec_c = one.ok_or_else(|| LowerError::not_lowerable("ATTN_CROSS_V1: the model has no cross-attention layer"))?;
        if spec_c.k_norm.gain == crate::spec::Gain::None || spec_c.k_norm.bias || spec_c.k_norm.kind != NormKind::Rms {
            return Err(LowerError::not_lowerable("ATTN_CROSS_V1: stage 0 lowers a gained RMS key norm"));
        }
        Ok(CrossKv { slots, spec: spec_c, rows, hidden: spec.hidden_size })
    }
    pub fn inner(&self) -> usize {
        self.spec.kv_heads * self.spec.head_dim
    }
}

/// The key of a cross layer's `site` in the TEXT stage's statistics (`L{m}.xattn.ctx.{site}`), from stage 0.
fn at(layer: usize, site: &str, wide: bool) -> ScaleKey {
    ScaleKey {
        base: Base::At { prefix: format!("L{layer}."), base: Box::new(Base::Site { names: vec![format!("xattn.ctx.{site}")], wide, split: 0 }) },
        factor: 1.0,
    }
}

/// Stage 0's synthetic HL program (the params of every cross layer's key and value projections and key norm) and its binding.
pub fn hl_cross_kv(spec: &ModelSpec) -> Result<(HlProgram, Binding)> {
    use crate::hl::{Block, CarryDecl, HlType, Init, Node, ParamDecl, Ref};
    let ck = CrossKv::of(spec)?;
    let (inner, hd, d) = (ck.inner(), ck.spec.head_dim, ck.hidden);
    let name_of = |role: &str, layer: usize| -> Result<String> {
        spec.hf.name(role).map(|t| t.replace("{L}", &layer.to_string())).ok_or_else(|| LowerError::eval(format!("internal: no tensor name for role `{role}`")))
    };
    let (mut params, mut srcs) = (Vec::new(), Vec::new());
    for (i, m) in ck.slots.iter().enumerate() {
        for (p, role, shape, suffix) in [
            ("k.w", "xattn.k", vec![inner, d], "weight"),
            ("v.w", "xattn.v", vec![inner, d], "weight"),
            ("k_norm.gain", "xattn.k_norm", vec![hd], "weight"),
        ] {
            params.push(ParamDecl { name: format!("xkv{i}.{p}"), shape, per_layer: false, init: Init::Normal(0.1) });
            srcs.push(Src::t(format!("{}.{suffix}", name_of(role, *m)?)));
        }
    }
    let post_inputs: Vec<Ref> = (0..params.len()).map(|i| Ref::Param(i as u32)).collect();
    let anchor = |inputs: Vec<Ref>| Node {
        op: crate::hl::Op::Scale { c: 1.0 },
        inputs,
        outs: vec![vec![d]],
        out_types: vec![HlType::F32],
        site: None,
        writes: vec![],
    };
    let blocks = vec![
        Block { name: "pre".into(), role: BlockRole::Pre, nodes: vec![anchor(vec![])], outputs: vec![Ref::Node(0, 0)] },
        Block { name: "post".into(), role: BlockRole::Post, nodes: vec![anchor(post_inputs)], outputs: vec![Ref::Node(0, 0)] },
    ];
    let hl = HlProgram {
        architecture: format!("{}:cross_kv", spec.architecture),
        output: crate::hl::HlOutput::Logits,
        vocab: spec.vocab_size,
        hidden: d,
        carries: vec![CarryDecl { name: "rows".into(), shape: vec![ck.rows, d], resid: false }],
        params,
        states: vec![],
        rope_tables: vec![],
        blocks,
        pre: 0,
        post: 1,
        schedule: vec![],
        layer_of: vec![],
    };
    hl.validate().map_err(|e| LowerError::eval(format!("internal: the cross K/V stage's HL program: {e}")))?;
    Ok((hl, Binding { srcs, aliases: spec.hf.prefix_aliases.clone(), ignored_prefixes: vec![] }))
}

/// Lower stage 0 over `hl` ([`hl_cross_kv`]): the states' unit is `unit`.
pub fn lower_cross_kv(hl: &HlProgram, spec: &ModelSpec, unit: f64) -> Result<Lowered> {
    let ck = CrossKv::of(spec)?;
    let (n, d, inner, hd, kv) = (ck.rows, ck.hidden, ck.inner(), ck.spec.head_dim, ck.spec.kv_heads);
    let (n32, d32) = (n as u32, d as u32);
    let hb = tir::program::HISTORY_BOUND_V1_SMALL;
    let mut pb = ProgramBuilder::new(1, hb);
    let mut cx = new_cx(hl, hb, hb);
    let mut block_map = vec![u8::MAX; hl.blocks.len()];
    // pre: the declared states, carried as they are.
    {
        let lb = new_lb(hl, hl.pre);
        let mut b = pb.block("pre", vec![]);
        let rows = decl(&mut b, &mut cx, &lb, STATES_PARAM, DType::I32, &[n, d], false, input_fill(DType::I32, vec![n, d]))?;
        let r = b.clamp(rows, i32::MIN as i64, i32::MAX as i64, DType::I32);
        block_map[hl.pre] = b.finish(&[r]);
    }
    // post: every cross layer's K and V from the codes of the states.
    let mut out_node = None;
    {
        let mut lb = new_lb(hl, hl.post);
        let tb = pb.blocks.len() as u8;
        let mut b = pb.block("post", vec![TensorType::fixed(DType::I32, &[n32, d32])]);
        let x = rows_val(tir::Ref::CarryIn(0), DType::I32, ScaleKey { base: Base::Fixed(unit), factor: 1.0 }, d, "carry0");
        let mut stacks: Vec<tir::Ref> = Vec::new();
        for (i, m) in ck.slots.iter().enumerate() {
            // The states at this layer's own code scale (the float stage's `cs` site).
            let ck_key = at(*m, "cs", false);
            let (xf, xt) = (x.key.clone(), ck_key.clone());
            let (mm, ss) = decl_ms(&mut b, &mut cx, &lb, &format!("xkv{i}.cs"), 1, Arc::new(move |c| Ok(vec![c.scale(&xf)? / c.scale(&xt)?])))?;
            let r = narrow(&mut b, x.r, mm, ss, None, DType::I16);
            b.commit(r);
            let cs = rows_val(r, DType::I16, ck_key, d, &format!("xkv{i}.cs"));
            // K: the projection at a wide scale, then the per-head RMS norm over `[rows·kv, hd]`.
            let raw_key = at(*m, "kraw", true);
            let raw = linear_rows(&mut b, &mut cx, &mut lb, &cs, &format!("xkv{i}.k.w"), None, &format!("xkv{i}.kraw"), &Want { dt: DType::I32, key: raw_key.clone() })?;
            b.commit(raw.r);
            let heads_rows = b.reshape_fixed(raw.r, &[n32 * kv as u32, hd as u32]);
            let hr = rows_val(heads_rows, DType::I32, raw_key, hd, &format!("xkv{i}.kraw"));
            let k_key = at(*m, "k", false);
            let kn = norm_rows_kind(&mut b, &mut cx, &mut lb, &hr, NormKind::Rms, ck.spec.k_norm.eps, &format!("xkv{i}.k_norm"), false, &Want { dt: DType::I16, key: k_key.clone() })?;
            let kr = b.reshape_fixed(kn.r, &[1, n32, inner as u32]);
            // V.
            let v_key = at(*m, "v", false);
            let v = linear_rows(&mut b, &mut cx, &mut lb, &cs, &format!("xkv{i}.v.w"), None, &format!("xkv{i}.v"), &Want { dt: DType::I16, key: v_key })?;
            let vr = b.reshape_fixed(v.r, &[1, n32, inner as u32]);
            let kvp = b.concat(&[kr, vr], 0); // [2, rows, inner]
            stacks.push(b.reshape_fixed(kvp, &[1, 2, n32, inner as u32]));
        }
        // A node takes at most 8 inputs: a tree of concatenations over the layers.
        while stacks.len() > 1 {
            stacks = stacks.chunks(8).map(|c| if c.len() == 1 { c[0] } else { b.concat(c, 0) }).collect();
        }
        let r = stacks[0];
        // The stack carries each layer's own scales; its nominal key is the first layer's keys (what `Lowered::logits_key` asks for).
        let out = rows_val(r, DType::I16, at(ck.slots[0], "k", false), inner, "xkv");
        let _ = &mut lb;
        let out = ensure_node(&mut b, &out);
        let tir::Ref::Node(oi) = out.r else { unreachable!("ensure_node") };
        b.commit(out.r);
        note_site(&mut cx, tb, &out);
        cx.logits_key = Some(out.key.clone());
        block_map[hl.post] = b.finish(&[]);
        out_node = Some(oi);
    }
    let _ = codes_rows;
    finish(pb, cx, hl, block_map, out_node, "cross K/V")
}
