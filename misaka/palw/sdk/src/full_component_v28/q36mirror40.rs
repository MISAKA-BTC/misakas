//! Exact PALWQ361 forward mirror. Raw legacy templates are only construction
//! recipes: exported authority is independently validated canonical Program39.
//! No old V1/V2 admission proxy, calibration or float re-quantization is used.
use misaka_palw_tir::{
    Cmp, DType, Dim, Ref, Rounding, TensorType,
    builder::{BlockBuilder, ProgramBuilder},
    library::Narrowing,
    program::{INPUT_POS, INPUT_TOKEN},
};
use misaka_palw_tir_lower::weights::TensorSource;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeShapeV40 {
    pub layer_types: Vec<u8>,
    pub d_model: u32,
    pub n_heads: u32,
    pub n_kv_heads: u32,
    pub head_dim: u32,
    pub rotary_dim: u32,
    pub linear_k_heads: u32,
    pub linear_v_heads: u32,
    pub linear_head_dim: u32,
    pub conv_kernel: u32,
    pub n_experts: u32,
    pub experts_per_token: u32,
    pub moe_dim: u32,
    pub shared_dim: u32,
    pub vocab: u32,
    pub max_position: u32,
    pub eps_q: i64,
    pub router_up_bits: u8,
}
impl RuntimeShapeV40 {
    fn validate(&self) -> Result<(), String> {
        let s = self;
        if s.layer_types.is_empty()
            || s.layer_types.len() > 254
            || s.layer_types.iter().any(|k| *k > 1)
            || s.d_model == 0
            || s.n_heads == 0
            || s.n_kv_heads == 0
            || s.n_heads % s.n_kv_heads != 0
            || s.head_dim == 0
            || s.rotary_dim > s.head_dim
            || s.rotary_dim % 2 != 0
            || s.n_experts == 0
            || s.experts_per_token == 0
            || s.experts_per_token > s.n_experts
            || s.experts_per_token > 64
            || s.moe_dim == 0
            || s.vocab == 0
            || s.max_position == 0
            || s.router_up_bits > 62
        {
            return Err("Q36_MIRROR40_ORIGINAL_SHAPE".into());
        }
        if s.layer_types.contains(&0)
            && (s.linear_k_heads == 0
                || s.linear_v_heads == 0
                || s.linear_v_heads % s.linear_k_heads != 0
                || s.linear_head_dim == 0
                || s.conv_kernel != 4)
        {
            return Err("Q36_MIRROR40_ORIGINAL_FOUR_TAP_GDN_GEOMETRY".into());
        }
        for a in [
            s.d_model,
            s.n_heads,
            s.n_kv_heads,
            s.head_dim,
            s.linear_k_heads,
            s.linear_v_heads,
            s.linear_head_dim,
            s.n_experts,
            s.moe_dim,
            s.shared_dim,
            s.vocab,
            s.max_position,
        ] {
            if a > 1 << 24 {
                return Err("Q36_MIRROR40_DIMENSION_BOUND".into());
            }
        }
        Ok(())
    }
    fn hybrid(&self) -> bool {
        self.layer_types.contains(&0)
    }
}
#[derive(Clone, Debug, Serialize)]
pub enum RowPolicyV40 {
    Exact,
    BroadcastOne,
    RepeatLast,
}
/// A source fragment is an exact typed plane/code range, never a float import.
#[derive(Clone, Debug, Serialize)]
pub struct ParameterSourceV40 {
    pub parameter: u16,
    pub source: Vec<String>,
    pub dtype: String,
    pub shape: Vec<u32>,
    pub rows_per_source: u32,
    pub policy: RowPolicyV40,
}
#[derive(Clone, Copy)]
struct T {
    m: Ref,
    s: Ref,
    z: Ref,
}
impl T {
    fn n(self) -> Narrowing {
        Narrowing { m: self.m, s: self.s, z: Some(self.z) }
    }
}
#[derive(Clone, Copy)]
struct Projection {
    w: Ref,
    e: Option<Ref>,
    n: T,
}
struct Deck<'a> {
    p: ProgramBuilder,
    source: &'a dyn TensorSource,
    mappings: Vec<ParameterSourceV40>,
    used: BTreeSet<String>,
}
impl Deck<'_> {
    fn declare(
        &mut self,
        name: &str,
        dtype: DType,
        shape: &[u32],
        sources: Vec<String>,
        rows_per_source: u32,
        policy: RowPolicyV40,
    ) -> Result<Ref, String> {
        let target = shape.iter().try_fold(1u64, |n, d| n.checked_mul(*d as u64)).ok_or("Q36_MIRROR40_PARAMETER_PRODUCT")?;
        if target == 0 || target > 1 << 34 || self.p.params.len() >= u16::MAX as usize {
            return Err("Q36_MIRROR40_PARAMETER_PROFILE_BOUND".into());
        }
        for source in &sources {
            let meta = self.source.metadata(source).ok_or_else(|| format!("Q36_MIRROR40_MISSING_SOURCE:{source}"))?;
            let elements = meta.shape.iter().try_fold(1u64, |n, d| n.checked_mul(*d as u64)).ok_or("Q36_MIRROR40_SOURCE_PRODUCT")?;
            let want = rows_per_source as u64;
            let valid = match policy {
                RowPolicyV40::Exact => elements == want,
                RowPolicyV40::BroadcastOne => elements == 1 || elements == want,
                RowPolicyV40::RepeatLast => elements > 0,
            };
            if !valid || !meta.dtype.eq_ignore_ascii_case(dtype.name()) {
                return Err(format!("Q36_MIRROR40_SOURCE_SHAPE_DTYPE:{source}"));
            }
            self.used.insert(source.clone());
        }
        let r = self.p.param(name, dtype, shape, false);
        let Ref::Param(parameter) = r else { unreachable!() };
        self.mappings.push(ParameterSourceV40 {
            parameter,
            source: sources,
            dtype: dtype.name().into(),
            shape: shape.to_vec(),
            rows_per_source,
            policy,
        });
        Ok(r)
    }
    fn raw(&mut self, name: &str, shape: &[u32]) -> Result<Ref, String> {
        let count = shape.iter().try_fold(1u32, |n, d| n.checked_mul(*d)).ok_or("Q36_MIRROR40_RAW_PRODUCT")?;
        self.declare(name, DType::I8, shape, vec![name.into()], count, RowPolicyV40::Exact)
    }
    fn plane(&mut self, name: &str, field: &str, want: u32, policy: RowPolicyV40) -> Result<Ref, String> {
        let dtype = if field == "s" { DType::I8 } else { DType::I64 };
        let source = format!("palwq361.parameter.{name}.{field}");
        self.declare(&format!("{name}.{field}"), dtype, &[want], vec![source], want, policy)
    }
    fn triple(&mut self, name: &str, want: u32) -> Result<T, String> {
        Ok(T {
            m: self.plane(name, "m", want, RowPolicyV40::BroadcastOne)?,
            s: self.plane(name, "s", want, RowPolicyV40::BroadcastOne)?,
            z: self.plane(name, "z", want, RowPolicyV40::BroadcastOne)?,
        })
    }
    fn head_triple(&mut self, name: &str, want: u32) -> Result<T, String> {
        Ok(T {
            m: self.plane(name, "m", want, RowPolicyV40::RepeatLast)?,
            s: self.plane(name, "s", want, RowPolicyV40::RepeatLast)?,
            z: self.plane(name, "z", want, RowPolicyV40::RepeatLast)?,
        })
    }
    fn project(&mut self, name: &str, rows: u32, cols: u32) -> Result<Projection, String> {
        let w = self.raw(name, &[rows, cols])?;
        let ename = format!("{name}.exp");
        let e = if self.source.metadata(&ename).is_some() { Some(self.raw(&ename, &[rows, cols.div_ceil(32)])?) } else { None };
        let n = self.triple(&format!("{name}.a16"), rows)?;
        Ok(Projection { w, e, n })
    }
}
fn proj(b: &mut BlockBuilder<'_>, p: Projection, x: Ref, wide: bool) -> Ref {
    let out = if let Some(e) = p.e { b.q36_matmul_grouped(p.w, e, x, &p.n.n(), wide) } else { b.a16_matmul(p.w, x, &p.n.n(), wide) };
    b.commit(out)
}
fn codes(b: &mut BlockBuilder<'_>, x: Ref, t: T) -> Ref {
    let y = b.narrow_codes(x, &t.n());
    b.commit(y)
}
fn wide(b: &mut BlockBuilder<'_>, x: Ref, t: T) -> Ref {
    let y = b.narrow_wide(x, &t.n());
    b.commit(y)
}
fn softmax_dynamic(b: &mut BlockBuilder<'_>, x: Ref, up: Ref) -> Ref {
    let axis = b.shape(x).len() - 1;
    let p2 = b.pow2_of(up);
    let min = b.c(DType::I64, i32::MIN as i128);
    let floor = b.div(min, p2, Rounding::Floor, DType::I64);
    let max = b.reduce_max(x, axis);
    let diff = b.sub(x, max, DType::I64);
    let too_low = b.compare(diff, floor, Cmp::Lt);
    let diff = b.select(too_low, floor, diff, DType::I64);
    let scaled = b.mul(diff, p2, DType::I128);
    let arg = b.clamp(scaled, i32::MIN as i64, 0, DType::I32);
    let e = b.int_exp(arg);
    let sum = b.reduce_sum(e, axis, DType::I64);
    let recip = b.int_recip(sum);
    let p = b.mul(e, recip, DType::I128);
    let w = b.shr(p, 24, Rounding::Floor, DType::I64);
    b.clamp(w, 0, 1 << 25, DType::I32)
}
/// Lowest-index ties in probability selection, then expert INDEX ascending,
/// exactly as the runtime's q36_router_topk committed row.
fn router(b: &mut BlockBuilder<'_>, logits: Ref, k: u32, up: Ref) -> (Ref, Ref) {
    let probabilities = softmax_dynamic(b, logits, up);
    let selected = b.topk(probabilities, 0, k);
    let selected = b.commit(selected);
    let kept = b.gather(probabilities, selected, 0, 0);
    let sum = b.reduce_sum(kept, 0, DType::I64);
    let reciprocal = b.int_recip(sum);
    let product = b.mul(kept, reciprocal, DType::I128);
    let scaled = b.shr(product, 24, Rounding::Floor, DType::I32);
    let uniform = b.c(DType::I32, ((1u64 << 24) / k as u64) as i128);
    let zero = b.c(DType::I64, 0);
    let empty = b.compare(sum, zero, Cmp::Le);
    let weights = b.select(empty, uniform, scaled, DType::I32);
    (selected, b.commit(weights))
}
impl Deck<'_> {
    fn stacked(
        &mut self,
        name: &str,
        dtype: DType,
        shape: &[u32],
        sources: Vec<String>,
        per_source: u32,
        policy: RowPolicyV40,
    ) -> Result<Ref, String> {
        self.declare(name, dtype, shape, sources, per_source, policy)
    }
    fn stack_triple(&mut self, name: &str, source_names: &[String], rows: u32) -> Result<T, String> {
        let mut planes = Vec::new();
        for field in ["m", "s", "z"] {
            let dtype = if field == "s" { DType::I8 } else { DType::I64 };
            planes.push(self.stacked(
                &format!("{name}.{field}"),
                dtype,
                &[source_names.len() as u32, rows],
                source_names.iter().map(|s| format!("palwq361.parameter.{s}.{field}")).collect(),
                rows,
                RowPolicyV40::BroadcastOne,
            )?);
        }
        Ok(T { m: planes[0], s: planes[1], z: planes[2] })
    }
    fn stack_projection(&mut self, name: &str, source_names: &[String], rows: u32, cols: u32) -> Result<Projection, String> {
        let count = rows.checked_mul(cols).ok_or("Q36_MIRROR40_EXPERT_SOURCE_SIZE")?;
        let w = self.stacked(
            &format!("{name}.weight"),
            DType::I8,
            &[source_names.len() as u32, rows, cols],
            source_names.to_vec(),
            count,
            RowPolicyV40::Exact,
        )?;
        let exp_names = source_names.iter().map(|n| format!("{n}.exp")).collect::<Vec<_>>();
        let present = exp_names.iter().filter(|n| self.source.metadata(n).is_some()).count();
        if present != 0 && present != source_names.len() {
            return Err("Q36_MIRROR40_MIXED_EXPERT_GROUP_FORMAT_REQUIRES_EXPLICIT_BANKS".into());
        }
        let groups = cols.div_ceil(32);
        let e = if present == 0 {
            None
        } else {
            Some(self.stacked(
                &format!("{name}.exp"),
                DType::I8,
                &[source_names.len() as u32, rows, groups],
                exp_names,
                rows.checked_mul(groups).ok_or("Q36_MIRROR40_EXPERT_GROUP_SIZE")?,
                RowPolicyV40::Exact,
            )?)
        };
        let pnames = source_names.iter().map(|n| format!("{n}.a16")).collect::<Vec<_>>();
        let n = self.stack_triple(&format!("{name}.a16"), &pnames, rows)?;
        Ok(Projection { w, e, n })
    }
}
#[derive(Clone, Copy)]
struct Expert {
    gate: Projection,
    up: Projection,
    down: Projection,
    silu: T,
    gated: T,
}
fn select_t(b: &mut BlockBuilder<'_>, t: T, index: Ref) -> T {
    T { m: b.gather(t.m, index, 0, 0), s: b.gather(t.s, index, 0, 0), z: b.gather(t.z, index, 0, 0) }
}
fn select_projection(b: &mut BlockBuilder<'_>, p: Projection, index: Ref) -> Projection {
    Projection { w: b.gather(p.w, index, 0, 0), e: p.e.map(|e| b.gather(e, index, 0, 0)), n: select_t(b, p.n, index) }
}
fn expert_forward(b: &mut BlockBuilder<'_>, e: Expert, x: Ref) -> Ref {
    let gate = proj(b, e.gate, x, true);
    let up = proj(b, e.up, x, false);
    let activation = b.silu(gate);
    let activation = wide(b, activation, e.silu);
    let gated = b.q36_mul_wide(activation, up, &e.gated.n());
    let gated = b.commit(gated);
    proj(b, e.down, gated, true)
}
impl Deck<'_> {
    fn expert(&mut self, base: &str, d: u32, mid: u32) -> Result<Expert, String> {
        Ok(Expert {
            gate: self.project(&format!("{base}_gate.weight"), mid, d)?,
            up: self.project(&format!("{base}_up.weight"), mid, d)?,
            down: self.project(&format!("{base}_down.weight"), d, mid)?,
            silu: self.triple(&format!("{base}_silu.a16"), mid)?,
            gated: self.triple(&format!("{base}_gated.a16"), mid)?,
        })
    }
    fn experts(&mut self, layer: usize, s: &RuntimeShapeV40) -> Result<Expert, String> {
        let names = |tail: &str| (0..s.n_experts).map(|e| format!("blk.{layer}.ffn_expert.{e}_{tail}")).collect::<Vec<_>>();
        let base = format!("blk.{layer}.ffn_expert.stack");
        Ok(Expert {
            gate: self.stack_projection(&format!("{base}.gate"), &names("gate.weight"), s.moe_dim, s.d_model)?,
            up: self.stack_projection(&format!("{base}.up"), &names("up.weight"), s.moe_dim, s.d_model)?,
            down: self.stack_projection(&format!("{base}.down"), &names("down.weight"), s.d_model, s.moe_dim)?,
            silu: self.stack_triple(&format!("{base}.silu"), &names("silu.a16"), s.moe_dim)?,
            gated: self.stack_triple(&format!("{base}.gated"), &names("gated.a16"), s.moe_dim)?,
        })
    }
}
struct Mixture {
    router: Projection,
    router_codes: T,
    up: Ref,
    expert: Expert,
    combine: T,
    shared: Option<(Expert, Projection, T)>,
    out: T,
}
impl Deck<'_> {
    fn mixture(&mut self, layer: usize, s: &RuntimeShapeV40) -> Result<Mixture, String> {
        let n = |v: &str| format!("blk.{layer}.{v}");
        Ok(Mixture {
            router: self.project(&n("ffn_router.weight"), s.n_experts, s.d_model)?,
            router_codes: self.triple(&n("ffn_router.a16"), s.n_experts)?,
            up: self.plane(&n("ffn_router_up.a16"), "z", 1, RowPolicyV40::Exact)?,
            expert: self.experts(layer, s)?,
            combine: self.triple(&n("ffn_combine.a16"), 1)?,
            shared: if s.shared_dim == 0 {
                None
            } else {
                Some((
                    self.expert(&n("ffn_shared_expert"), s.d_model, s.shared_dim)?,
                    self.project(&n("ffn_shared_gate.weight"), 1, s.d_model)?,
                    self.triple(&n("ffn_shared_gated.a16"), 1)?,
                ))
            },
            out: self.triple(&n("ffn_moe_out.a16"), s.d_model)?,
        })
    }
}
fn mixture_forward(b: &mut BlockBuilder<'_>, m: &Mixture, x: Ref, k: u32, d: u32) -> Ref {
    let raw = proj(b, m.router, x, true);
    let logits = codes(b, raw, m.router_codes);
    let (ids, weights) = router(b, logits, k, m.up);
    let mut values = Vec::new();
    for i in 0..k {
        let at = b.c(DType::Idx, i as i128);
        let id = b.gather(ids, at, 0, 0);
        let e = Expert {
            gate: select_projection(b, m.expert.gate, id),
            up: select_projection(b, m.expert.up, id),
            down: select_projection(b, m.expert.down, id),
            silu: select_t(b, m.expert.silu, id),
            gated: select_t(b, m.expert.gated, id),
        };
        let y = expert_forward(b, e, x);
        values.push(b.reshape_fixed(y, &[1, d]));
    }
    // Concat has at most8 operands; preserve expert-index order across the tree.
    while values.len() > 1 {
        values = values.chunks(8).map(|xs| if xs.len() == 1 { xs[0] } else { b.concat(xs, 0) }).collect();
    }
    let divisor = b.pow2_of(m.combine.s);
    let combined = b.moe_combine_q36(values[0], weights, m.combine.m, divisor, m.combine.z, -32767, 32767, DType::I16);
    let combined = b.commit(combined);
    let sum = if let Some((expert, gate, n)) = m.shared {
        let y = expert_forward(b, expert, x);
        let g = proj(b, gate, x, true);
        let g = b.int_sigmoid(g);
        let y = b.q36_mul_wide(y, g, &n.n());
        b.add(combined, y, DType::I32)
    } else {
        b.cast(combined, DType::I32)
    };
    codes(b, sum, m.out)
}
struct LinearArm {
    q: Projection,
    k: Projection,
    v: Projection,
    z: Projection,
    dt: Projection,
    beta: Projection,
    o: Projection,
    conv: Ref,
    conv_scale: T,
    conv_act: T,
    decay: Ref,
    bias: Ref,
    read: T,
    delta: T,
    write: Ref,
    out: T,
    norm: T,
    norm_eps_z: Ref,
    norm_eps_s: Ref,
    gated: T,
    conv_state: u16,
    gdn_state: u16,
}
impl Deck<'_> {
    fn linear(&mut self, layer: usize, s: &RuntimeShapeV40) -> Result<LinearArm, String> {
        let d = s.d_model;
        let dk = s.linear_k_heads.checked_mul(s.linear_head_dim).ok_or("Q36_MIRROR40_KEY_WIDTH")?;
        let dv = s.linear_v_heads.checked_mul(s.linear_head_dim).ok_or("Q36_MIRROR40_VALUE_WIDTH")?;
        let width = dk.checked_mul(2).and_then(|v| v.checked_add(dv)).ok_or("Q36_MIRROR40_CONV_WIDTH")?;
        let n = |x: &str| format!("blk.{layer}.{x}");
        let conv_state = self.p.fixed_state(&n("linear_conv.state"), DType::I16, &[3, width], -32767, 32767, false);
        let gdn_state = self.p.fixed_state(
            &n("linear_gdn.state"),
            DType::I32,
            &[s.linear_v_heads, s.linear_head_dim, s.linear_head_dim],
            -(i32::MAX as i64),
            i32::MAX as i64,
            false,
        );
        Ok(LinearArm {
            q: self.project(&n("linear_q.weight"), dk, d)?,
            k: self.project(&n("linear_k.weight"), dk, d)?,
            v: self.project(&n("linear_v.weight"), dv, d)?,
            z: self.project(&n("linear_z.weight"), dv, d)?,
            dt: self.project(&n("linear_dt.weight"), s.linear_v_heads, d)?,
            beta: self.project(&n("linear_beta.weight"), s.linear_v_heads, d)?,
            o: self.project(&n("linear_o.weight"), d, dv)?,
            conv: self.raw(&n("linear_conv.weight"), &[width, 4])?,
            conv_scale: self.triple(&n("linear_conv.a16"), width)?,
            conv_act: self.triple(&n("linear_conv_act.a16"), width)?,
            decay: self.plane(&n("linear_decay_c.a16"), "z", s.linear_v_heads, RowPolicyV40::RepeatLast)?,
            bias: self.plane(&n("linear_dt_bias.a16"), "z", s.linear_v_heads, RowPolicyV40::RepeatLast)?,
            read: self.head_triple(&n("linear_read.a16"), s.linear_v_heads)?,
            delta: self.head_triple(&n("linear_delta.a16"), s.linear_v_heads)?,
            write: self.plane(&n("linear_write.a16"), "z", s.linear_v_heads, RowPolicyV40::RepeatLast)?,
            out: self.head_triple(&n("linear_out.a16"), s.linear_v_heads)?,
            norm: self.triple(&n("linear_norm.a16"), dv)?,
            norm_eps_z: self.plane(&n("linear_norm_eps.a16"), "z", s.linear_v_heads, RowPolicyV40::RepeatLast)?,
            norm_eps_s: self.plane(&n("linear_norm_eps.a16"), "s", s.linear_v_heads, RowPolicyV40::RepeatLast)?,
            gated: self.triple(&n("linear_gated.a16"), dv)?,
            conv_state,
            gdn_state,
        })
    }
}
fn linear_forward(b: &mut BlockBuilder<'_>, a: &LinearArm, x: Ref, s: &RuntimeShapeV40) -> Ref {
    let (dk, dv, hd, h) =
        (s.linear_k_heads * s.linear_head_dim, s.linear_v_heads * s.linear_head_dim, s.linear_head_dim, s.linear_v_heads);
    let q = proj(b, a.q, x, false);
    let k = proj(b, a.k, x, false);
    let v = proj(b, a.v, x, false);
    let z = proj(b, a.z, x, true);
    let current = b.concat(&[q, k, v], 0);
    let width = 2 * dk + dv;
    let current = b.reshape_fixed(current, &[1, width]);
    let window = b.concat(&[Ref::State(a.conv_state), current], 0);
    let next = b.slice(window, 0, 1, 3);
    b.state_write(a.conv_state, next);
    let convolved = b.q36_ssm_conv(window, a.conv, &a.conv_scale.n());
    let activated = b.silu(convolved);
    let activated = codes(b, activated, a.conv_act);
    let qc = b.slice(activated, 0, 0, dk);
    let kc = b.slice(activated, 0, dk, dk);
    let vc = b.slice(activated, 0, 2 * dk, dv);
    let qc = b.reshape_fixed(qc, &[s.linear_k_heads, hd]);
    let kc = b.reshape_fixed(kc, &[s.linear_k_heads, hd]);
    let qc = b.l2_norm_q15(qc);
    let kc = b.l2_norm_q15(kc);
    let ratio = h / s.linear_k_heads;
    let qc = b.map_heads_tile(qc, ratio);
    let kc = b.map_heads_tile(kc, ratio);
    let vc = b.reshape_fixed(vc, &[h, hd]);
    let dt = proj(b, a.dt, x, true);
    // Reproduce Rust's signed i64→i32 wrapping conversion, not a narrowing clamp.
    let half = b.c(DType::I128, 1i128 << 31);
    let modulo = b.c(DType::I128, 1i128 << 32);
    let shifted = b.add(a.bias, half, DType::I128);
    let turns = b.div(shifted, modulo, Rounding::Floor, DType::I128);
    let turns = b.mul(turns, modulo, DType::I128);
    let wrapped = b.sub(a.bias, turns, DType::I128);
    let bias = b.clamp(wrapped, i32::MIN as i64, i32::MAX as i64, DType::I32);
    let biased = b.add(dt, bias, DType::I64);
    let biased = b.clamp(biased, i32::MIN as i64, i32::MAX as i64, DType::I32);
    let decay = b.decay_q36(biased, a.decay);
    let beta = proj(b, a.beta, x, true);
    let beta = b.int_sigmoid(beta);
    let read_div = b.pow2_of(a.read.s);
    let delta_div = b.pow2_of(a.delta.s);
    let out_div = b.pow2_of(a.out.s);
    let out = b.gdn_step_q36(
        a.gdn_state,
        kc,
        vc,
        qc,
        decay,
        beta,
        (a.read.m, read_div, a.read.z),
        (a.delta.m, delta_div, a.delta.z),
        a.write,
        (a.out.m, out_div, a.out.z),
    );
    let out = b.commit(out);
    let eps_z = b.reshape_fixed(a.norm_eps_z, &[h, 1]);
    let eps_s = b.reshape_fixed(a.norm_eps_s, &[h, 1]);
    let unit = b.rms_norm_wide_q36_exact(out, eps_z, eps_s);
    let unit = b.reshape_fixed(unit, &[dv]);
    let normed = wide(b, unit, a.norm);
    let gate = b.silu(z);
    let gated = b.q36_mul_wide(normed, gate, &a.gated.n());
    let gated = b.commit(gated);
    proj(b, a.o, gated, false)
}
struct FullArm {
    q: Projection,
    k: Projection,
    v: Projection,
    gate: Option<(Projection, T)>,
    o: Projection,
    qn: T,
    kn: T,
    rope: T,
    logits: T,
    up: Ref,
    prob: T,
    values: T,
    kh: u16,
    vh: u16,
}
impl Deck<'_> {
    fn full(&mut self, layer: usize, s: &RuntimeShapeV40) -> Result<FullArm, String> {
        let n = |x: &str| format!("blk.{layer}.{x}");
        let qdim = s.n_heads.checked_mul(s.head_dim).ok_or("Q36_MIRROR40_QUERY_WIDTH")?;
        let kv = s.n_kv_heads.checked_mul(s.head_dim).ok_or("Q36_MIRROR40_KV_WIDTH")?;
        let kh = self.p.hist_state(&n("attn_k.cache"), DType::I16, &[kv], s.max_position, false);
        let vh = self.p.hist_state(&n("attn_v.cache"), DType::I16, &[kv], s.max_position, false);
        Ok(FullArm {
            q: self.project(&n("attn_q.weight"), qdim, s.d_model)?,
            k: self.project(&n("attn_k.weight"), kv, s.d_model)?,
            v: self.project(&n("attn_v.weight"), kv, s.d_model)?,
            gate: if s.hybrid() {
                Some((self.project(&n("attn_gate.weight"), qdim, s.d_model)?, self.triple(&n("attn_gated.a16"), 1)?))
            } else {
                None
            },
            o: self.project(&n("attn_o.weight"), s.d_model, qdim)?,
            qn: self.triple(&n("attn_q_norm.a16"), s.head_dim)?,
            kn: self.triple(&n("attn_k_norm.a16"), s.head_dim)?,
            rope: self.triple(&n("attn_rope.a16"), 1)?,
            logits: self.triple(&n("attn_logits.a16"), 1)?,
            up: self.plane(&n("attn_softmax_up.a16"), "z", 1, RowPolicyV40::Exact)?,
            prob: self.triple(&n("attn_probs.a16"), 1)?,
            values: self.triple(&n("attn_values.a16"), 1)?,
            kh,
            vh,
        })
    }
}
fn full_forward(b: &mut BlockBuilder<'_>, a: &FullArm, x: Ref, s: &RuntimeShapeV40, cos: Ref, sin: Ref) -> Ref {
    let q = proj(b, a.q, x, false);
    let k = proj(b, a.k, x, false);
    let v = proj(b, a.v, x, false);
    let q = b.reshape_fixed(q, &[s.n_heads, s.head_dim]);
    let k = b.reshape_fixed(k, &[s.n_kv_heads, s.head_dim]);
    let q = b.rms_norm_a16(q, s.eps_q);
    let k = b.rms_norm_a16(k, s.eps_q);
    let q = codes(b, q, a.qn);
    let k = codes(b, k, a.kn);
    let q = b.reshape_fixed(q, &[s.n_heads * s.head_dim]);
    let k = b.reshape_fixed(k, &[s.n_kv_heads * s.head_dim]);
    let q = if s.rotary_dim == 0 { q } else { b.q36_rope_partial(q, s.head_dim, s.rotary_dim, cos, sin, &a.rope.n()) };
    let k = if s.rotary_dim == 0 { k } else { b.q36_rope_partial(k, s.head_dim, s.rotary_dim, cos, sin, &a.rope.n()) };
    let q = b.commit(q);
    let k = b.commit(k);
    let keys = b.hist_append(a.kh, k);
    let values = b.hist_append(a.vh, v);
    let scores = b.a16_attn_scores(q, keys, s.n_heads, s.n_kv_heads, s.head_dim, &a.logits.n());
    let scores = b.commit(scores);
    let probabilities = softmax_dynamic(b, scores, a.up);
    let probabilities = codes(b, probabilities, a.prob);
    let attended = b.a16_attn_values(probabilities, values, s.n_heads, s.n_kv_heads, s.head_dim, &a.values.n());
    let attended = b.commit(attended);
    let attended = if let Some((projection, n)) = a.gate {
        let g = proj(b, projection, x, true);
        let g = b.int_sigmoid(g);
        let gated = b.q36_gate_apply(attended, g, &n.n());
        b.commit(gated)
    } else {
        attended
    };
    proj(b, a.o, attended, false)
}
/// A canonical new39 program and exact source mappings. The constructor refuses
/// unknown source tensors instead of attributing them to the complete decoder.
pub struct RuntimeMirrorV40 {
    pub program: misaka_palw_tir::program_v39::TirProgramV39,
    pub mappings: Vec<ParameterSourceV40>,
    pub source_tensor_inventory: Vec<String>,
}
fn import_block39(b: misaka_palw_tir::program::Block) -> misaka_palw_tir::program_v39::BlockV39 {
    use misaka_palw_tir::program_v39::{BlockV39, NodeV39, TensorTypeV39};
    BlockV39 {
        name: b.name,
        carry_in: b.carry_in.iter().map(TensorTypeV39::from_frozen).collect(),
        nodes: b
            .nodes
            .iter()
            .map(|n| {
                let mut value = NodeV39::from_frozen(n);
                if matches!(n.prim, misaka_palw_tir::Prim::StateWrite { .. } | misaka_palw_tir::Prim::HistAppend { .. }) {
                    value.commit = true;
                }
                value
            })
            .collect(),
        carry_out: b.carry_out,
    }
}
/// Build from the exact native shape/configuration returned by PALWQ361 source
/// acquisition. This function emits no old canonical program or old admission.
fn build_runtime_mirror_inner_v40(
    source: &dyn TensorSource,
    shape: &RuntimeShapeV40,
    rope_head_dim: u32,
    rope_positions: u32,
    profile: misaka_palw_tir::program_v39::SourceProfileV39,
) -> Result<RuntimeMirrorV40, String> {
    use misaka_palw_tir::{program_v2::OutputDecl, program_v39::TirProgramV39};
    shape.validate()?;
    profile.check()?;
    validate_original_domain_v40(source, shape)?;
    let s = shape;
    if s.eps_q < 0
        || s.head_dim > 1 << 17
        || s.linear_head_dim > 1 << 17
        || s.max_position > misaka_palw_tir::program::HISTORY_BOUND_V1_HELD
        || rope_head_dim % 2 != 0
        || rope_head_dim < s.rotary_dim
        || rope_positions < s.max_position
    {
        return Err("Q36_MIRROR40_ORIGINAL_POSITION_OR_REDUCTION_DOMAIN".into());
    }
    let mut deck = Deck { p: ProgramBuilder::new(s.vocab, s.max_position), source, mappings: Vec::new(), used: BTreeSet::new() };
    let embedding = deck.raw("token_embd.weight", &[s.vocab, s.d_model])?;
    // The runtime embedding permits one shared row OR one row per token.
    let embed_lift = deck.triple("embed_lift.a16", s.vocab)?;
    let rope = if s.layer_types.contains(&1) && s.rotary_dim > 0 {
        let count = rope_positions.checked_mul(rope_head_dim / 2).ok_or("Q36_MIRROR40_ROPE_EXTENT")?;
        Some((
            deck.declare(
                "rope.cos",
                DType::I32,
                &[rope_positions, rope_head_dim / 2],
                vec!["palwq361.rope.cos".into()],
                count,
                RowPolicyV40::Exact,
            )?,
            deck.declare(
                "rope.sin",
                DType::I32,
                &[rope_positions, rope_head_dim / 2],
                vec!["palwq361.rope.sin".into()],
                count,
                RowPolicyV40::Exact,
            )?,
        ))
    } else {
        None
    };
    let pre = {
        let mut b = deck.p.block_for_program_v39("pre", vec![], &profile)?;
        let row = b.gather(embedding, Ref::Input(INPUT_TOKEN), 0, 0);
        let n = select_t(&mut b, embed_lift, Ref::Input(INPUT_TOKEN));
        let h = codes(&mut b, row, n);
        b.finish(&[h])
    };
    let carry = vec![TensorType::fixed(DType::I16, &[s.d_model])];
    let mut layers = Vec::new();
    for (layer, kind) in s.layer_types.iter().enumerate() {
        let n = |tail: &str| format!("blk.{layer}.{tail}");
        let attn_norm = deck.triple(&n("attn_norm.a16"), s.d_model)?;
        let attn_align = deck.triple(&n("attn_align.a16"), s.d_model)?;
        let attn_residual = deck.triple(&n("attn_residual.a16"), s.d_model)?;
        let ffn_norm = deck.triple(&n("ffn_norm.a16"), s.d_model)?;
        let ffn_align = deck.triple(&n("ffn_align.a16"), s.d_model)?;
        let ffn_residual = deck.triple(&n("ffn_residual.a16"), s.d_model)?;
        let linear = if *kind == 0 { Some(deck.linear(layer, s)?) } else { None };
        let full = if *kind == 1 { Some(deck.full(layer, s)?) } else { None };
        let mixture = deck.mixture(layer, s)?;
        let mut b = deck.p.block_for_program_v39(&format!("original-layer-{layer}"), carry.clone(), &profile)?;
        let x = Ref::CarryIn(0);
        let unit = b.rms_norm_a16(x, s.eps_q);
        let normed = codes(&mut b, unit, attn_norm);
        let delta = if let Some(a) = &linear {
            linear_forward(&mut b, a, normed, s)
        } else {
            let (cos, sin) = if let Some((cos, sin)) = rope {
                let row = b.clamp(Ref::Input(INPUT_POS), 0, s.max_position as i64 - 1, DType::Idx);
                let c = b.gather(cos, row, 0, 0);
                let sn = b.gather(sin, row, 0, 0);
                (b.slice(c, 0, 0, s.rotary_dim / 2), b.slice(sn, 0, 0, s.rotary_dim / 2))
            } else {
                let z = b.c(DType::I32, 0);
                (z, z)
            };
            full_forward(&mut b, full.as_ref().ok_or("Q36_MIRROR40_ARM")?, normed, s, cos, sin)
        };
        let aligned = codes(&mut b, x, attn_align);
        let sum = b.add(aligned, delta, DType::I32);
        let h = codes(&mut b, sum, attn_residual);
        let unit = b.rms_norm_a16(h, s.eps_q);
        let normed = codes(&mut b, unit, ffn_norm);
        let delta = mixture_forward(&mut b, &mixture, normed, s.experts_per_token, s.d_model);
        let aligned = codes(&mut b, h, ffn_align);
        let sum = b.add(aligned, delta, DType::I32);
        let h = codes(&mut b, sum, ffn_residual);
        layers.push(b.finish(&[h]));
    }
    let final_norm = deck.triple("final_norm.a16", s.d_model)?;
    let head = deck.project("output.weight", s.vocab, s.d_model)?;
    let (post, output) = {
        let mut b = deck.p.block_for_program_v39("post", carry, &profile)?;
        let unit = b.rms_norm_a16(Ref::CarryIn(0), s.eps_q);
        let h = codes(&mut b, unit, final_norm);
        let logits = proj(&mut b, head, h, false);
        let Ref::Node(output) = logits else {
            return Err("Q36_MIRROR40_LOGITS_NODE".into());
        };
        (b.finish(&[]), output)
    };
    let p = deck.p;
    let program = TirProgramV39 {
        version: 39,
        profile,
        prim_set_id: misaka_palw_tir::program_v39::PRIM_SET_ID_V39,
        token_bound: s.vocab,
        history_bound: s.max_position,
        inputs: Vec::new(),
        params: p.params,
        consts: p.consts,
        states: p.states,
        blocks: p.blocks.into_iter().map(import_block39).collect(),
        schedule: misaka_palw_tir::program::Schedule { pre, layers, post },
        output: OutputDecl::Logits { node: output, scheme_id: [0; 64] },
    };
    misaka_palw_tir::validate_v39::validate_v39(&program)?;
    let original = source.names().into_iter().collect::<BTreeSet<_>>();
    // Unused typed quant fields and unused original rotary tables are retained
    // as source-format metadata under the independently pinned whole file.
    // Every actual WEIGHT/EXP tensor must participate in the complete graph.
    if original.iter().any(|name| !name.starts_with("palwq361.") && !deck.used.contains(name)) {
        return Err("Q36_MIRROR40_UNREAD_ORIGINAL_WEIGHT_TENSOR".into());
    }
    Ok(RuntimeMirrorV40 { program, mappings: deck.mappings, source_tensor_inventory: original.into_iter().collect() })
}

/// Construction refusals, including a template expansion beyond the selected
/// profile, are returned before any output/artifact publication.
pub fn build_runtime_mirror_v40(
    source: &dyn TensorSource,
    shape: &RuntimeShapeV40,
    rope_head_dim: u32,
    rope_positions: u32,
    profile: misaka_palw_tir::program_v39::SourceProfileV39,
) -> Result<RuntimeMirrorV40, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        build_runtime_mirror_inner_v40(source, shape, rope_head_dim, rope_positions, profile)
    }))
    .map_err(|_| "Q36_MIRROR40_RECIPE_EXPANSION_OR_TYPE_BOUND".to_string())?
}
/// Validate the runtime's refusal domain before translating its total templates.
/// Domain validation is source preparation, never inference/fidelity evidence.
fn validate_original_domain_v40(source: &dyn TensorSource, shape: &RuntimeShapeV40) -> Result<(), String> {
    for name in source.names() {
        let meta = source.metadata(&name).ok_or("Q36_MIRROR40_SOURCE_METADATA_CHANGED")?;
        let is_exponent = !name.starts_with("palwq361.") && name.ends_with(".exp");
        let is_shift = name.starts_with("palwq361.parameter.") && name.ends_with(".s");
        if is_exponent || is_shift {
            for at in (0..meta.bytes).step_by(1 << 20) {
                let end = (at + (1 << 20)).min(meta.bytes);
                let raw = source.read_slice(&name, at..end).map_err(|e| e.to_string())?;
                if raw.len() as u64 != end - at || raw.iter().any(|v| if is_exponent { *v > 20 } else { *v > 62 }) {
                    return Err(format!("Q36_MIRROR40_ORIGINAL_EXPONENT_OR_SHIFT_DOMAIN:{name}"));
                }
            }
        }
    }
    for (layer, kind) in shape.layer_types.iter().enumerate() {
        if *kind != 0 {
            continue;
        }
        for (suffix, limit, negative) in [
            ("linear_read.a16.m", 1i64 << 30, true),
            ("linear_delta.a16.m", 1i64 << 30, true),
            ("linear_out.a16.m", 1i64 << 30, true),
            ("linear_write.a16.z", 20, false),
            ("linear_norm_eps.a16.z", i64::MAX, false),
        ] {
            let name = format!("palwq361.parameter.blk.{layer}.{suffix}");
            let meta = source.metadata(&name).ok_or("Q36_MIRROR40_ORIGINAL_GDN_PARAMETER_ABSENT")?;
            let rows = meta.bytes / 8;
            if rows == 0 {
                return Err("Q36_MIRROR40_EMPTY_GDN_COEFFICIENT".into());
            }
            let n = rows.min(shape.linear_v_heads as u64);
            let raw = source.read_slice(&name, 0..n * 8).map_err(|e| e.to_string())?;
            for value in raw.chunks_exact(8) {
                let value = i64::from_le_bytes(value.try_into().map_err(|_| "Q36_MIRROR40_COEFFICIENT_WIDTH")?);
                let valid = if suffix == "linear_write.a16.z" {
                    (-62..=20).contains(&value)
                } else if negative {
                    value.unsigned_abs() <= limit as u64
                } else {
                    value >= 0 && value <= limit
                };
                if !valid {
                    return Err(format!("Q36_MIRROR40_ORIGINAL_GDN_DOMAIN:{name}"));
                }
            }
        }
    }
    Ok(())
}
impl RuntimeMirrorV40 {
    /// Materialise exact typed parameters and construct original v3 PC in bounded
    /// buffers. The large table is never decoded to an all-parameter i128 Tensor.
    pub fn write_parameters(
        &self,
        source: &dyn TensorSource,
        directory: &std::path::Path,
        block_bytes: usize,
        workspace: u64,
        total_cap: u64,
    ) -> Result<misaka_palw_kernel::trace::ParamCommitmentsV1, String> {
        use std::io::Write;
        if directory.exists() || !(8..=16 << 20).contains(&block_bytes) || workspace == 0 || workspace > 256 << 20 {
            return Err("Q36_MIRROR40_FRESH_OUTPUT_OR_WORKSPACE_BOUND".into());
        }
        if self.mappings.len() != self.program.params.len() {
            return Err("Q36_MIRROR40_ORIGINAL_PARAMETER_MAP_INCOMPLETE".into());
        }
        let mut aggregate = 0u64;
        for (i, m) in self.mappings.iter().enumerate() {
            let p = self.program.params.get(i).ok_or("Q36_MIRROR40_PARAMETER_ABSENT")?;
            if m.parameter as usize != i || m.shape != p.shape || !m.dtype.eq_ignore_ascii_case(p.dtype.name()) || m.source.is_empty()
            {
                return Err("Q36_MIRROR40_ORIGINAL_PARAMETER_MAP_CHANGED".into());
            }
            let count = p.shape.iter().try_fold(1u64, |n, d| n.checked_mul(*d as u64)).ok_or("Q36_MIRROR40_ELEMENT_OVERFLOW")?;
            if count != m.rows_per_source as u64 * m.source.len() as u64 {
                return Err("Q36_MIRROR40_TARGET_FRAGMENT_EXTENT_DIFFERS".into());
            }
            let bytes = count.checked_mul(p.dtype.width() as u64).ok_or("Q36_MIRROR40_BYTE_OVERFLOW")?;
            aggregate = aggregate.checked_add(bytes).ok_or("Q36_MIRROR40_AGGREGATE_OVERFLOW")?;
            if aggregate > total_cap {
                return Err("Q36_MIRROR40_AGGREGATE_BYTE_CAP".into());
            }
            let shape = p.shape.iter().map(|n| *n as usize).collect::<Vec<_>>();
            let bound = misaka_palw_kernel::merkle3_stream::streamed_commitment_shape_v3(p.dtype, &shape)?;
            if bound.workspace_bytes > workspace {
                return Err("Q36_MIRROR40_STREAMED_PC_WORKSPACE_BOUND".into());
            }
        }
        source.validate_snapshot().map_err(|e| e.to_string())?;
        std::fs::create_dir(directory).map_err(|e| e.to_string())?;
        let mut pc = misaka_palw_kernel::trace::ParamCommitmentsV1::default();
        for m in &self.mappings {
            let p = &self.program.params[m.parameter as usize];
            let width = p.dtype.width() as u64;
            let elements = m.rows_per_source as u64;
            let path = directory.join(format!("{:05}.tensor", m.parameter));
            let mut file = std::fs::OpenOptions::new().create_new(true).write(true).open(&path).map_err(|e| e.to_string())?;
            for name in &m.source {
                let meta = source.metadata(name).ok_or("Q36_MIRROR40_ORIGINAL_TENSOR_DISAPPEARED")?;
                let rows = meta.bytes / width;
                let valid = match m.policy {
                    RowPolicyV40::Exact => rows == elements,
                    RowPolicyV40::BroadcastOne => rows == 1 || rows == elements,
                    RowPolicyV40::RepeatLast => rows > 0,
                };
                if !valid {
                    return Err("Q36_MIRROR40_ORIGINAL_FRAGMENT_CHANGED".into());
                }
                let tail = if rows < elements {
                    Some(source.read_slice(name, (rows - 1) * width..rows * width).map_err(|e| e.to_string())?)
                } else {
                    None
                };
                let chunk_elements = (block_bytes as u64 / width).max(1);
                let mut at = 0;
                while at < elements {
                    let end = (at + chunk_elements).min(elements);
                    let in_end = end.min(rows);
                    let in_start = at.min(in_end);
                    let mut raw = if in_start < in_end {
                        source.read_slice(name, in_start * width..in_end * width).map_err(|e| e.to_string())?
                    } else {
                        Vec::new()
                    };
                    if raw.len() as u64 != (in_end - in_start) * width {
                        return Err("Q36_MIRROR40_SHORT_ORIGINAL_RANGE".into());
                    }
                    if end > rows {
                        let last = tail.as_ref().ok_or("Q36_MIRROR40_REPEATED_ROW_ABSENT")?;
                        if last.len() as u64 != width {
                            return Err("Q36_MIRROR40_REPEATED_ROW_WIDTH".into());
                        }
                        for _ in at.max(rows)..end {
                            raw.extend_from_slice(last);
                        }
                    }
                    file.write_all(&raw).map_err(|e| e.to_string())?;
                    at = end;
                }
            }
            let expected = m.rows_per_source as u64 * m.source.len() as u64 * width;
            if file.metadata().map_err(|e| e.to_string())?.len() != expected {
                return Err("Q36_MIRROR40_EXACT_OUTPUT_EXTENT_DIFFERS".into());
            }
            file.flush().map_err(|e| e.to_string())?;
            drop(file);
            let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
            let shape = p.shape.iter().map(|v| *v as usize).collect::<Vec<_>>();
            let (root, _) =
                misaka_palw_kernel::merkle3_stream::tensor_commitment_streamed_v3(p.dtype, &shape, workspace, &mut |at, out| {
                    crate::tir_stream::pread(&file, out, at).map_err(|e| e.to_string())
                })?;
            pc.by_instance.insert((m.parameter, None), root);
        }
        source.validate_snapshot().map_err(|e| e.to_string())?;
        Ok(pc)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeCompileDeclarationV40 {
    pub schema: String,
    pub original: super::frontend::OriginalSource,
    pub runtime_file: String,
    pub tokenizer_file: String,
    pub selected_profile: String,
    pub block_bytes: usize,
    pub commitment_workspace_bytes: u64,
    pub maximum_output_bytes: u64,
}
/// Complete actual source→Program39/realPC/R writer. The raw SHA pins and
/// independent original task remain in its immutable provenance, never a grant.
pub fn compile_runtime_q36_v40(
    declaration_path: &std::path::Path,
    declaration_sha: [u8; 32],
    source_directory: &std::path::Path,
    output: &std::path::Path,
) -> Result<serde_json::Value, String> {
    use super::frontend::{bytes, decode, relative};
    use misaka_palw_kernel::{
        flat_source39::{FlatSourceWireV39, OriginalTensorV39, derive_inventory_root_v39},
        hash::object_id,
    };
    use misaka_palw_tir_lower::frontend_pack::{FrontendPack, FrontendSource};
    use std::io::Write;
    if output.exists() {
        return Err("Q36_MIRROR40_FRESH_OUTPUT_REQUIRED".into());
    }
    let raw = bytes(declaration_path, 16 << 20)?;
    if super::hash(declaration_path, 16 << 20)? != declaration_sha {
        return Err("Q36_MIRROR40_DECLARATION_PIN_DIFFERS".into());
    }
    let declaration: RuntimeCompileDeclarationV40 =
        serde_json::from_slice(&raw).map_err(|e| format!("Q36_MIRROR40_DECLARATION:{e}"))?;
    let original = &declaration.original;
    let source_directory = source_directory.canonicalize().map_err(|e| e.to_string())?;
    if declaration.schema != "misaka.palw.runtime-q36-source-compiler.v40"
        || declaration.selected_profile != "full-source39"
        || original.original_contexts.len() != 1
        || original.components.len() != 1
        || original.components[0].name != "decoder"
        || original.components[0].stages != vec![0]
        || original.source_files.is_empty()
        || original.source_files.len() > 4096
        || original.repository.is_empty()
        || original.advertised_task.is_empty()
        || declaration.maximum_output_bytes == 0
        || declaration.maximum_output_bytes > 512 << 30
    {
        return Err("Q36_MIRROR40_COMPLETE_ORIGINAL_SCOPE_OR_OUTPUT_BOUND".into());
    }
    decode::<20>(&original.revision)?;
    let mut pins = BTreeMap::new();
    for pin in &original.source_files {
        let path = relative(&source_directory, &pin.path)?;
        let expected = decode::<32>(&pin.sha256)?;
        if pins.insert(pin.path.clone(), expected).is_some()
            || pin.bytes > 512 << 30
            || std::fs::metadata(&path).map_err(|e| e.to_string())?.len() != pin.bytes
            || super::hash(&path, 512 << 30)? != expected
        {
            return Err("Q36_MIRROR40_ORIGINAL_SOURCE_FILE_PIN".into());
        }
    }
    let files = original.components[0].source_files.iter().cloned().collect::<BTreeSet<_>>();
    if files.len() != original.components[0].source_files.len() || files != pins.keys().cloned().collect() {
        return Err("Q36_MIRROR40_ORIGINAL_COMPONENT_FILE_COVERAGE".into());
    }
    if !pins.contains_key(&declaration.runtime_file)
        || !pins.contains_key(&declaration.tokenizer_file)
        || pins.get(&original.task_metadata_file) != Some(&decode::<32>(&original.task_metadata_sha256)?)
    {
        return Err("Q36_MIRROR40_TASK_TOKENIZER_RUNTIME_NOT_PINNED".into());
    }
    let runtime = relative(&source_directory, &declaration.runtime_file)?;
    if runtime.extension().is_none_or(|s| s != "palwq36") {
        return Err("Q36_MIRROR40_EXPLICIT_PALWQ361_FILE_REQUIRED".into());
    }
    let pack=FrontendPack::parse(&serde_json::json!({"format":misaka_palw_tir_lower::frontend_pack::FORMAT,"id":"runtime-q36-exact-source40","scope":{"task":original.advertised_task,"completeness":"full","components":["decoder"]},"program":null,"bindings":[]}).to_string()).map_err(|e|e.to_string())?;
    let source = FrontendSource::open(&runtime, &pack).map_err(|e| e.to_string())?;
    let config = source.configuration(serde_json::json!({})).map_err(|e| e.to_string())?;
    let shape: RuntimeShapeV40 =
        serde_json::from_value(config["palwq361.shape"].clone()).map_err(|e| format!("Q36_MIRROR40_ORIGINAL_SHAPE:{e}"))?;
    if original.original_contexts[0] != shape.max_position {
        return Err("Q36_MIRROR40_RUNTIME_CONTEXT_CANNOT_BE_PROMOTED".into());
    }
    let inventory = source.names().into_iter().map(|name| format!("{}::{name}", declaration.runtime_file)).collect::<BTreeSet<_>>();
    let declared = original.complete_tensor_inventory.iter().cloned().collect::<BTreeSet<_>>();
    if declared.len() != original.complete_tensor_inventory.len() || inventory != declared {
        return Err("Q36_MIRROR40_COMPLETE_ORIGINAL_SOURCE_INVENTORY_DIFFERS".into());
    }
    let rope = &config["palwq361.rope"];
    let rh = u32::try_from(rope["d_head"].as_u64().ok_or("Q36_MIRROR40_ROPE_HEAD")?).map_err(|_| "Q36_MIRROR40_ROPE_HEAD")?;
    let rp = u32::try_from(rope["max_position"].as_u64().ok_or("Q36_MIRROR40_ROPE_POSITIONS")?)
        .map_err(|_| "Q36_MIRROR40_ROPE_POSITIONS")?;
    let mirror = build_runtime_mirror_v40(&source, &shape, rh, rp, misaka_palw_tir::program_v39::SourceProfileV39::full_source())?;
    let task = bytes(&relative(&source_directory, &original.task_metadata_file)?, 16 << 20)?;
    let task_root = object_id(b"misaka-palw/sdk/original-runtime-task-metadata/v40", &(&raw, task));
    let tokenizer = bytes(&relative(&source_directory, &declaration.tokenizer_file)?, 64 << 20)?;
    let tokenizer = misaka_palw_tir_lower::artifact::tokenizer_id_of(&tokenizer);
    // Original PC/R are computed BEFORE candidate files exist, directly from
    // the exact original virtual views. A generated file cannot define truth.
    let records = mirror.original_inventory_from_source(&source, None, declaration.commitment_workspace_bytes)?;
    let original_pc = misaka_palw_kernel::trace::ParamCommitmentsV1 {
        by_instance: records.iter().map(|r| ((r.param, r.layer), r.source_tensor)).collect(),
    };
    let inventory_root = derive_inventory_root_v39(&mirror.program, task_root, &records)?;
    let wire = FlatSourceWireV39 {
        version: 39,
        program_bytes: mirror.program.encode(),
        inventory_root,
        original_inventory: records.clone(),
        real_parameters: original_pc,
        original_context_max: shape.max_position,
        original_task_metadata: task_root,
    };
    let checked = wire.check()?;
    let resources = misaka_palw_kernel::flat_resources39::derive_flat_resources_v39(&checked, shape.max_position)
        .map_err(|e| format!("Q36_MIRROR40_ORIGINAL_FULL_SCOPE_ADMISSION:{e}"))?;
    std::fs::create_dir(output).map_err(|e| e.to_string())?;
    let candidate_pc = mirror.write_parameters(
        &source,
        &output.join("parameters"),
        declaration.block_bytes,
        declaration.commitment_workspace_bytes,
        declaration.maximum_output_bytes,
    )?;
    if &candidate_pc != checked.parameters() {
        return Err("Q36_MIRROR40_GENERATED_BYTES_DIFFER_FROM_INDEPENDENT_ORIGINAL_PC".into());
    }
    for pin in &original.source_files {
        if super::hash(&relative(&source_directory, &pin.path)?, 512 << 30)? != decode::<32>(&pin.sha256)? {
            return Err("Q36_MIRROR40_ORIGINAL_SOURCE_CHANGED_DURING_CONVERSION".into());
        }
    }
    if super::hash(declaration_path, 16 << 20)? != declaration_sha {
        return Err("Q36_MIRROR40_DECLARATION_CHANGED_DURING_CONVERSION".into());
    }
    let hex = crate::runtime_pack::commit::hex;
    let report = serde_json::json!({"schema":"misaka.palw.runtime-q36-source-conversion.v40","original":original,"declaration_sha256":hex(&declaration_sha),"program_root":hex(&checked.program_root()),"source":hex(&checked.root()),"source_profile":hex(&checked.profile_root()),"real_parameter":hex(&checked.parameters().root()),"inventory":hex(&checked.inventory_root()),"tokenizer":hex(&tokenizer),"maximum_scope":hex(&resources.scope().root()),"maximum_resources":hex(&resources.root()),"original_context":shape.max_position,"lowered_context":shape.max_position,"source_tensors":mirror.source_tensor_inventory,"parameter_mapping":mirror.mappings,"source_fidelity":"NOT_RUN","native_final":"NOT_RUN","A6_or_A8_credit":false});
    for (name, data) in [
        ("program.tir39", wire.program_bytes.clone()),
        ("source.borsh", wire.to_bytes()),
        ("maximum-scope.borsh", borsh::to_vec(resources.scope()).map_err(|e| e.to_string())?),
        ("maximum-resources.borsh", borsh::to_vec(resources.vector()).map_err(|e| e.to_string())?),
        ("original-inventory.borsh", borsh::to_vec(&records).map_err(|e| e.to_string())?),
        ("conversion.json", serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?),
    ] {
        std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(output.join(name))
            .map_err(|e| e.to_string())?
            .write_all(&data)
            .map_err(|e| e.to_string())?;
    }
    Ok(report)
}
impl RuntimeMirrorV40 {
    /// Independently read the ORIGINAL virtual parameter views. This never reads
    /// the generated parameter files or accepts a candidate root as source truth.
    fn original_inventory_from_source(
        &self,
        source: &dyn TensorSource,
        candidate: Option<&misaka_palw_kernel::trace::ParamCommitmentsV1>,
        workspace: u64,
    ) -> Result<Vec<misaka_palw_kernel::flat_source39::OriginalTensorV39>, String> {
        use misaka_palw_kernel::{flat_source39::OriginalTensorV39, merkle3_stream::tensor_commitment_streamed_v3};
        let mut records = Vec::with_capacity(self.program.params.len());
        for mapping in &self.mappings {
            let p = &self.program.params[mapping.parameter as usize];
            let width = p.dtype.width() as u64;
            let fragment = mapping.rows_per_source as u64;
            let shape = p.shape.iter().map(|n| *n as usize).collect::<Vec<_>>();
            let (root, bound) = tensor_commitment_streamed_v3(p.dtype, &shape, workspace, &mut |offset, out| {
                if offset % width != 0 || out.len() as u64 % width != 0 {
                    return Err("Q36_MIRROR40_ORIGINAL_VIEW_ALIGNMENT".into());
                }
                let start = offset / width;
                let count = out.len() as u64 / width;
                let end = start.checked_add(count).ok_or("Q36_MIRROR40_ORIGINAL_VIEW_OVERFLOW")?;
                let total = fragment.checked_mul(mapping.source.len() as u64).ok_or("Q36_MIRROR40_ORIGINAL_VIEW_EXTENT")?;
                if end > total {
                    return Err("Q36_MIRROR40_ORIGINAL_VIEW_RANGE".into());
                }
                let mut cursor = start;
                while cursor < end {
                    let index = (cursor / fragment) as usize;
                    let relative = cursor % fragment;
                    let stop = end.min((index as u64 + 1) * fragment);
                    let n = stop - cursor;
                    let name = &mapping.source[index];
                    let meta = source.metadata(name).ok_or("Q36_MIRROR40_ORIGINAL_VIEW_DISAPPEARED")?;
                    if !meta.dtype.eq_ignore_ascii_case(p.dtype.name()) || meta.bytes % width != 0 {
                        return Err("Q36_MIRROR40_ORIGINAL_VIEW_DTYPE".into());
                    }
                    let original = meta.bytes / width;
                    let valid = match mapping.policy {
                        RowPolicyV40::Exact => original == fragment,
                        RowPolicyV40::BroadcastOne => original == 1 || original == fragment,
                        RowPolicyV40::RepeatLast => original > 0,
                    };
                    if !valid {
                        return Err("Q36_MIRROR40_ORIGINAL_VIEW_ROW_POLICY".into());
                    }
                    // Split at the original extent. The remaining logical rows
                    // read the actual LAST scalar, including broadcast-one.
                    let prefix = n.min(original.saturating_sub(relative));
                    let dst = ((cursor - start) * width) as usize;
                    if prefix > 0 {
                        let bytes =
                            source.read_slice(name, relative * width..(relative + prefix) * width).map_err(|e| e.to_string())?;
                        if bytes.len() as u64 != prefix * width {
                            return Err("Q36_MIRROR40_ORIGINAL_VIEW_SHORT_PREFIX".into());
                        }
                        out[dst..dst + bytes.len()].copy_from_slice(&bytes);
                    }
                    if prefix < n {
                        let scalar = source.read_slice(name, (original - 1) * width..original * width).map_err(|e| e.to_string())?;
                        if scalar.len() as u64 != width {
                            return Err("Q36_MIRROR40_ORIGINAL_VIEW_SHORT_LAST_ROW".into());
                        }
                        for at in prefix..n {
                            let dst = dst + (at * width) as usize;
                            out[dst..dst + width as usize].copy_from_slice(&scalar);
                        }
                    }
                    cursor = stop;
                }
                Ok(())
            })?;
            if candidate.is_some_and(|pc| pc.by_instance.get(&(mapping.parameter, None)) != Some(&root)) {
                return Err(format!("Q36_MIRROR40_GENERATED_BYTES_DIFFER_FROM_ORIGINAL_VIEW:{}", mapping.parameter));
            }
            records.push(OriginalTensorV39 {
                param: mapping.parameter,
                layer: None,
                dtype: p.dtype,
                shape: p.shape.clone(),
                raw_bytes: bound.tensor_bytes,
                source_tensor: root,
            });
        }
        if candidate.is_some_and(|pc| records.len() != pc.by_instance.len()) {
            return Err("Q36_MIRROR40_EXTRA_CANDIDATE_PC_INSTANCE".into());
        }
        Ok(records)
    }
}
/// Local read-only39 parameter source. Its constructor independently checks every
/// generated tensor against the checked ORIGINAL PC, using bounded v3 hashing.
/// This is reference material, never a native Final capability.
pub struct SourceDirectoryV39<'a> {
    source: &'a misaka_palw_kernel::flat_source39::CheckedSourceV39,
    files: BTreeMap<(u16, Option<u16>), (std::fs::File, u64, Option<std::time::SystemTime>)>,
    max_read_elements: usize,
}
impl<'a> SourceDirectoryV39<'a> {
    pub fn open(
        source: &'a misaka_palw_kernel::flat_source39::CheckedSourceV39,
        directory: &std::path::Path,
        workspace: u64,
        max_read_elements: usize,
    ) -> Result<Self, String> {
        if max_read_elements == 0
            || max_read_elements as u64 > source.profile().max_value_elements
            || workspace == 0
            || workspace > 256 << 20
        {
            return Err("SOURCE39_REFERENCE_READER_BUDGET".into());
        }
        let mut files = BTreeMap::new();
        for (&(index, layer), expected) in &source.parameters().by_instance {
            let p = &source.program().params[index as usize];
            let name = match layer {
                None => format!("{index:05}.tensor"),
                Some(l) => format!("{index:05}-layer-{l:05}.tensor"),
            };
            let path = directory.join(name);
            if std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?.file_type().is_symlink() {
                return Err("SOURCE39_REFERENCE_SYMLINK".into());
            }
            let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
            let meta = file.metadata().map_err(|e| e.to_string())?;
            let shape = p.shape.iter().map(|v| *v as usize).collect::<Vec<_>>();
            let (root, bound) =
                misaka_palw_kernel::merkle3_stream::tensor_commitment_streamed_v3(p.dtype, &shape, workspace, &mut |offset, out| {
                    crate::tir_stream::pread(&file, out, offset).map_err(|e| e.to_string())
                })?;
            if &root != expected || meta.len() != bound.tensor_bytes {
                return Err("SOURCE39_REFERENCE_BYTES_DIFFER_FROM_ORIGINAL_PC".into());
            }
            files.insert((index, layer), (file, meta.len(), meta.modified().ok()));
        }
        Ok(Self { source, files, max_read_elements })
    }
    pub fn source_root(&self) -> misaka_palw_kernel::hash::Digest {
        self.source.root()
    }
}
impl misaka_palw_tir::interp_v39::ParamSourceV39 for SourceDirectoryV39<'_> {
    fn descriptor(&self, index: u16, layer: Option<u16>) -> Option<misaka_palw_tir::interp_v39::ParamDescriptorV39> {
        self.files.get(&(index, layer))?;
        let p = self.source.program().params.get(index as usize)?;
        Some(misaka_palw_tir::interp_v39::ParamDescriptorV39 { dtype: p.dtype, shape: p.shape.clone() })
    }
    fn read_range(&self, index: u16, layer: Option<u16>, offset: u64, elements: usize) -> misaka_palw_tir::TirResult<Vec<i128>> {
        use misaka_palw_tir::{TirError, TirErrorKind};
        let err = |s: &str| TirError::new(TirErrorKind::Operand, s);
        let (file, bytes, modified) =
            self.files.get(&(index, layer)).ok_or_else(|| err("source39 original parameter instance absent"))?;
        let p = &self.source.program().params[index as usize];
        let width = p.dtype.width() as u64;
        let start = offset.checked_mul(width).ok_or_else(|| err("source39 parameter offset overflow"))?;
        let n = elements.checked_mul(width as usize).ok_or_else(|| err("source39 parameter byte extent overflow"))?;
        if elements > self.max_read_elements || start.checked_add(n as u64).is_none_or(|end| end > *bytes) {
            return Err(err("source39 parameter range exceeds original tensor or explicit reader budget"));
        }
        let check = || -> misaka_palw_tir::TirResult<()> {
            let m = file.metadata().map_err(|e| err(&e.to_string()))?;
            if m.len() != *bytes || m.modified().ok() != *modified {
                return Err(err("source39 authenticated local material changed"));
            }
            Ok(())
        };
        check()?;
        let mut raw = Vec::new();
        raw.try_reserve_exact(n).map_err(|_| err("source39 parameter range allocation refused"))?;
        raw.resize(n, 0);
        crate::tir_stream::pread(file, &mut raw, start).map_err(|e| err(&e.to_string()))?;
        check()?;
        let mut values = Vec::new();
        values.try_reserve_exact(elements).map_err(|_| err("source39 decoded range allocation refused"))?;
        values.extend(raw.chunks_exact(width as usize).map(|v| p.dtype.decode_le(v)));
        Ok(values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_base0::qwen36::*;
    fn pack() -> misaka_palw_tir_lower::frontend_pack::FrontendPack {
        misaka_palw_tir_lower::frontend_pack::FrontendPack::parse(&serde_json::json!({"format":misaka_palw_tir_lower::frontend_pack::FORMAT,"id":"exact-runtime-fixture","scope":{"task":"text-generation","completeness":"full","components":["decoder"]},"program":null,"bindings":[]}).to_string()).unwrap()
    }
    fn differential(artifact: Qwen36ArtifactV1) {
        let directory = std::env::temp_dir().join(format!(
            "q36-mirror39-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("tiny.palwq36");
        let names = artifact.tensor_names();
        let plan = names.iter().map(|name| (name.to_string(), artifact.tensor_len(name).unwrap())).collect::<Vec<_>>();
        let mut writer = Qwen36Writer::create(&path, &artifact.shape, &artifact.rope, artifact.params_map(), plan).unwrap();
        for name in &names {
            writer.push(name, &artifact.tensor(name).unwrap()).unwrap();
        }
        writer.finish().unwrap();
        let source = misaka_palw_tir_lower::frontend_pack::FrontendSource::open(&path, &pack()).unwrap();
        let configuration = source.configuration(serde_json::json!({})).unwrap();
        let shape: RuntimeShapeV40 = serde_json::from_value(configuration["palwq361.shape"].clone()).unwrap();
        let mirror = build_runtime_mirror_v40(
            &source,
            &shape,
            artifact.rope.d_head as u32,
            artifact.rope.max_position as u32,
            misaka_palw_tir::program_v39::SourceProfileV39::full_source(),
        )
        .unwrap();
        let decoded = misaka_palw_tir::program_v39::TirProgramV39::decode_canonical(&mirror.program.encode()).unwrap();
        assert_eq!(decoded, mirror.program);
        let pc = mirror.write_parameters(&source, &directory.join("params"), 256, 16 << 20, 64 << 20).unwrap();
        let records = mirror.original_inventory_from_source(&source, Some(&pc), 16 << 20).unwrap();
        let task = [7; 64];
        let inventory = misaka_palw_kernel::flat_source39::derive_inventory_root_v39(&mirror.program, task, &records).unwrap();
        let checked = misaka_palw_kernel::flat_source39::FlatSourceWireV39 {
            version: 39,
            program_bytes: mirror.program.encode(),
            inventory_root: inventory,
            original_inventory: records.clone(),
            real_parameters: pc,
            original_context_max: shape.max_position,
            original_task_metadata: task,
        }
        .check()
        .unwrap();
        let params = SourceDirectoryV39::open(&checked, &directory.join("params"), 16 << 20, 1 << 20).unwrap();
        let interpreter = misaka_palw_tir::interp_v39::InterpreterV39::new(&mirror.program).unwrap();
        let inputs = misaka_palw_tir::interp_v2::MapInputs::default();
        let mut state = misaka_palw_tir::interp::RunState::default();
        let mut cache = Qwen36Cache::new(&artifact.shape);
        let engine = Qwen36Engine::new(&artifact);
        for (position, token) in [1u32, 3, 2, 4, 0].iter().enumerate() {
            let original = engine.forward_token(&mut cache, *token as usize, position).unwrap();
            let actual = interpreter.step(&params, &inputs, &mut state, *token).unwrap();
            assert_eq!(
                actual.output.data,
                original.into_iter().map(i128::from).collect::<Vec<_>>(),
                "original token position{position}"
            );
        }
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn hybrid_mirror_preserves_four_tap_gdn_tiled_heads_partial_rope_and_shared_moe() {
        differential(qwen36_dev_fixture_heads(4, 4, 2, 4, 4, 16));
    }
    #[test]
    fn full_attention_moe_mirror_has_no_invented_gate_or_shared_expert() {
        differential(qwen3moe_dev_fixture(2, 4));
    }
    #[test]
    fn zero_group_exponents_preserve_original_projection_bits() {
        let a = qwen36_dev_fixture(4, 4);
        let length = a.shape.vocab * a.shape.d_model.div_ceil(32);
        differential(a.with_tensor("output.weight.exp", vec![0i8; length]));
    }
}
