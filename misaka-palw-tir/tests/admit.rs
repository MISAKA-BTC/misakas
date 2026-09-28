//! **`tir_admit_v1`**: the corpus programs and a Qwen2.5-1.5B-sized dense decoder are admitted with
//! the derived numbers the court needs (cone tiles, dissection chunks, state replay groups and
//! checkpoint intervals), in bounded CPU time; and every class of defect — overflow, an index out
//! of range, oversize, a history-mixing primitive, a bad divisor or shift, a dead node,
//! non-canonical bytes, a cone or replay past its ceiling — is refused, by rule.

mod common;

use std::time::{Duration, Instant};

use common::admission::head_local_delta_rule;
use common::models::*;
use misaka_palw_tir::admit::{
    LeafV1, TirAdmissionV1, TirAdmitError, TirAdmitInputsV1, TirCeilingsV1, tir_admit_program_v1, tir_admit_v1,
};
use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::library::Narrowing;
use misaka_palw_tir::library::attn::AttnCfg;
use misaka_palw_tir::program::{HISTORY_BOUND_V1_SMALL, INPUT_POS, INPUT_TOKEN};
use misaka_palw_tir::{DType, Dim, Node, Prim, Ref, Rounding, TensorType, TirErrorKind, TirProgramV1};

fn inputs() -> TirAdmitInputsV1 {
    TirAdmitInputsV1 { tile_len: 64, h_chunk: 16, ceilings: TirCeilingsV1::legacy_court_v1() }
}

fn admit(p: &TirProgramV1) -> Result<TirAdmissionV1, TirAdmitError> {
    tir_admit_program_v1(p, &inputs())
}

fn timed(name: &str, p: &TirProgramV1) -> (TirAdmissionV1, Duration) {
    let t0 = Instant::now();
    let a = admit(p).unwrap_or_else(|e| panic!("{name} is admitted: {e}"));
    let dt = t0.elapsed();
    eprintln!(
        "{name:<28} {:>6} B program, {:>4} commit points, position {:>14} MACs {:>10} transc, state {:>12} B, {:>6} step leaves, worst terminal tile {:>10} MACs, C = {:>5}, admitted in {:?}",
        p.encode().len(),
        a.cones.len(),
        a.position.cost.macs,
        a.position.cost.transcendentals,
        a.position.state_bytes,
        a.position.step_leaves,
        a.cones.iter().map(|c| c.terminal().macs).max().unwrap_or(0),
        a.checkpoint_interval,
        dt
    );
    (a, dt)
}

/// A Qwen2.5-1.5B-shaped dense decoder written with `tir_library_v1` the way a lowerer writes it:
/// an `i32` residual stream, A16 codes at the projections, per-channel `(m, s, z)` narrowings, the
/// wide RMSNorm, half-split RoPE from two-level angle tables, GQA over a `2^18` window, a SiLU table,
/// and an `i32` LM head.
#[allow(clippy::too_many_arguments)]
fn dense_sized(layers: u16, d: u32, heads: u32, kv: u32, hd: u32, ffn: u32, vocab: u32) -> TirProgramV1 {
    let mut pb = ProgramBuilder::new(vocab, HISTORY_BOUND_V1_SMALL);
    let site = |pb: &mut ProgramBuilder, name: &str, n: u32, per_layer: bool| -> Narrowing {
        let m = pb.param(&format!("{name}.m"), DType::I64, &[n], per_layer);
        let s = pb.param(&format!("{name}.s"), DType::I8, &[n], per_layer);
        let z = pb.param(&format!("{name}.z"), DType::I64, &[n], per_layer);
        Narrowing::new(m, s, Some(z))
    };
    let eps = |pb: &mut ProgramBuilder, name: &str, per_layer: bool| -> (Ref, Ref) {
        (
            pb.param(&format!("{name}.eps_m"), DType::I64, &[1], per_layer),
            pb.param(&format!("{name}.eps_s"), DType::I8, &[1], per_layer),
        )
    };
    // Globals: the embedding and its per-token lift, the angle tables, the LM head.
    let emb = pb.param("tok_embd", DType::I8, &[vocab, d], false);
    let lift_m = pb.param("tok_embd.lift.m", DType::I64, &[vocab], false);
    let lift_s = pb.param("tok_embd.lift.s", DType::I8, &[vocab], false);
    let tabs: Vec<Ref> = ["rope.cos_hi", "rope.sin_hi", "rope.cos_lo", "rope.sin_lo"]
        .iter()
        .map(|n| pb.param(n, DType::I32, &[512, hd / 2], false))
        .collect();
    let out_eps = eps(&mut pb, "out_norm", false);
    let out_site = site(&mut pb, "out_norm", d, false);
    let lm = pb.param("lm_head", DType::I8, &[vocab, d], false);
    let lm_site = site(&mut pb, "lm_head", vocab, false);
    // Per layer.
    let e1 = eps(&mut pb, "attn_norm", true);
    let n1 = site(&mut pb, "attn_norm", d, true);
    let wq = pb.param("wq", DType::I8, &[heads * hd, d], true);
    let nq = site(&mut pb, "q", heads * hd, true);
    let wk = pb.param("wk", DType::I8, &[kv * hd, d], true);
    let nk = site(&mut pb, "k", kv * hd, true);
    let wv = pb.param("wv", DType::I8, &[kv * hd, d], true);
    let nv = site(&mut pb, "v", kv * hd, true);
    let nscore = site(&mut pb, "score", 1, true);
    let nctx = site(&mut pb, "ctx", 1, true);
    let wo = pb.param("wo", DType::I8, &[d, heads * hd], true);
    let no = site(&mut pb, "o", d, true);
    let e2 = eps(&mut pb, "ffn_norm", true);
    let n2 = site(&mut pb, "ffn_norm", d, true);
    let wg = pb.param("wg", DType::I8, &[ffn, d], true);
    let ng = site(&mut pb, "gate", ffn, true);
    let silu = pb.param("silu", DType::I16, &[65536], true);
    let wu = pb.param("wu", DType::I8, &[ffn, d], true);
    let nu = site(&mut pb, "up", ffn, true);
    let nm = site(&mut pb, "mul", ffn, true);
    let wd = pb.param("wd", DType::I8, &[d, ffn], true);
    let nd = site(&mut pb, "down", d, true);
    let ks = pb.hist_state("k", DType::I16, &[kv * hd], HISTORY_BOUND_V1_SMALL, true);
    let vs = pb.hist_state("v", DType::I16, &[kv * hd], HISTORY_BOUND_V1_SMALL, true);
    let carry = vec![TensorType::fixed(DType::I32, &[d])];
    let i32r = (i32::MIN as i64, i32::MAX as i64);

    let pre = {
        let mut b = pb.block("pre", vec![]);
        let tok = Ref::Input(INPUT_TOKEN);
        let row = b.embed(emb, tok);
        let m = b.gather(lift_m, tok, 0, 0);
        let s = b.gather(lift_s, tok, 0, 0);
        let x = b.narrow(row, &Narrowing::new(m, s, None), i32r.0, i32r.1, DType::I32);
        let x = b.commit(x);
        b.finish(&[x])
    };
    let layer = {
        let mut b = pb.block("layer", carry.clone());
        let x = Ref::CarryIn(0);
        let u = b.rms_norm_wide_q36(x, e1.0, e1.1);
        let h = b.narrow_codes(u, &n1);
        let h = b.commit(h);
        let q = b.a16_matmul(wq, h, &nq, false);
        let k = b.a16_matmul(wk, h, &nk, false);
        let v = b.a16_matmul(wv, h, &nv, false);
        let v = b.commit(v);
        let (c, s) = b.rope_angles_two_level(Ref::Input(INPUT_POS), tabs[0], tabs[1], tabs[2], tabs[3], 9);
        let q2 = b.reshape_fixed(q, &[heads, hd]);
        let q2 = b.rope_half(q2, c, s, -32767, 32767, DType::I16);
        let q2 = b.reshape_fixed(q2, &[heads * hd]);
        let q2 = b.commit(q2);
        let k2 = b.reshape_fixed(k, &[kv, hd]);
        let k2 = b.rope_half(k2, c, s, -32767, 32767, DType::I16);
        let k2 = b.reshape_fixed(k2, &[kv * hd]);
        let kw = b.hist_append(ks, k2);
        let vw = b.hist_append(vs, v);
        let cfg = AttnCfg {
            heads,
            kv_heads: kv,
            head_dim: hd,
            score: nscore,
            softcap: None,
            alibi: None,
            sink: None,
            up_bits: 0,
            value: nctx,
        };
        let att = b.attention(q2, kw, vw, &cfg);
        let att = b.commit(att);
        let o = b.a16_matmul(wo, att, &no, true);
        let x1 = b.add(x, o, DType::I64);
        let x1 = b.clamp(x1, i32r.0, i32r.1, DType::I32);
        let x1 = b.commit(x1);
        let u2 = b.rms_norm_wide_q36(x1, e2.0, e2.1);
        let h2 = b.narrow_codes(u2, &n2);
        let h2 = b.commit(h2);
        let g = b.a16_matmul(wg, h2, &ng, false);
        let ga = b.act_table(g, silu);
        let up = b.a16_matmul(wu, h2, &nu, false);
        let gu = b.mul(ga, up, DType::I32);
        let m = b.narrow_codes(gu, &nm);
        let m = b.commit(m);
        let dn = b.a16_matmul(wd, m, &nd, true);
        let x2 = b.add(x1, dn, DType::I64);
        let x2 = b.clamp(x2, i32r.0, i32r.1, DType::I32);
        b.finish(&[x2])
    };
    let post = {
        let mut b = pb.block("post", carry);
        let u = b.rms_norm_wide_q36(Ref::CarryIn(0), out_eps.0, out_eps.1);
        let h = b.narrow_codes(u, &out_site);
        let h = b.commit(h);
        let logits = b.a16_matmul(lm, h, &lm_site, true);
        b.commit(logits);
        b.finish(&[])
    };
    let logits = pb.blocks[post as usize].nodes.len() as u16 - 1;
    pb.finish(pre, vec![layer; layers as usize], post, logits)
}

#[test]
fn the_corpus_programs_and_a_qwen25_1_5b_sized_decoder_are_admitted_in_bounded_time() {
    let mut worst = Duration::ZERO;
    for (name, p) in [
        ("dense-gqa-2layer", dense(&[HISTORY_BOUND_V1_SMALL, HISTORY_BOUND_V1_SMALL]).0),
        ("sliding-global", dense(&[3, HISTORY_BOUND_V1_SMALL, 3]).0),
        ("gdn-k2-v4-grouped", gdn_program(true).0),
        ("mamba2", mamba2_program().0),
        ("moe-top2-shared", moe_program().0),
    ] {
        let (a, dt) = timed(name, &p);
        worst = worst.max(dt);
        assert!(a.checkpoint_interval >= 1);
        assert_eq!(a.intervals.len(), p.blocks.len());
    }
    // Qwen2.5-1.5B: 28 layers, d 1536, 12 heads / 2 kv heads of 128, ffn 8960, vocab 151,936.
    let p = dense_sized(28, 1536, 12, 2, 128, 8960, 151_936);
    let (a, dt) = timed("qwen2.5-1.5b-sized", &p);
    worst = worst.max(dt);
    // The LM head's tile is `tile_len · d` MACs, not the whole vocabulary: the box demand.
    let post = p.schedule.post;
    let head = a.cones.iter().find(|c| c.block == post && c.node == p.logits).expect("the logits are a commit point");
    assert_eq!(head.tile.macs, 64 * 1536, "one logits tile is tile_len rows of the head");
    assert_eq!(head.tiles, 151_936u64.div_ceil(64));
    // Attention is dissected over H: its terminal cost is one 16-position chunk.
    let attn = a.cones.iter().find(|c| !c.h_reductions.is_empty()).expect("an attention cone");
    assert!(attn.chunk.expect("a chunk").macs < attn.tile.macs / 1000, "the chunk is a sliver of the whole window");
    // Two Hist states per layer at a 2^18 window.
    assert_eq!(a.position.state_bytes, 28 * 2 * (1u64 << 18) * 256 * 2);
    eprintln!("worst admission time {worst:?}");
    assert!(worst < Duration::from_secs(10), "admission is a registration-time computation, not a search");
}

#[test]
fn state_replay_splits_into_groups_where_the_update_is_head_local() {
    let p = head_local_delta_rule(8, 16, 16);
    let a = admit(&p).unwrap();
    let ck = &a.states[0];
    assert_eq!((ck.state, ck.closure.clone()), (0, vec![0]), "the update reads only S itself");
    assert_eq!(ck.groups, 8, "one replay group per head");
    let whole: u64 = a.states[0].per_position.macs * 8;
    assert!(
        whole > 0 && ck.interval as u64 == (16u64 << 20) / ck.per_position.macs.max(1),
        "C_j = the terminal ceiling over one group's replay"
    );
    // The corpus GDN layer does not commit the conv output, so replaying S also replays the conv
    // window — a state whose axis 0 is not the heads: no split is provable, and the replay is whole.
    let (g, _) = gdn_program(true);
    let a = admit(&g).unwrap();
    let s = g.states.iter().position(|s| s.name == "S").unwrap() as u16;
    let ck = a.states.iter().find(|c| c.state == s).unwrap();
    assert_eq!(ck.closure.len(), 2, "S and the conv window replay together");
    assert_eq!(ck.groups, 1);
    // Every Fixed state has an interval, and C is their minimum.
    let fixed = g.states.iter().filter(|s| matches!(s.kind, misaka_palw_tir::StateKind::Fixed { .. })).count();
    assert_eq!(a.states.len(), fixed);
    assert_eq!(a.checkpoint_interval, a.states.iter().map(|c| c.interval).min().unwrap());
}

#[test]
fn the_cones_name_their_leaves() {
    let (p, _) = dense(&[HISTORY_BOUND_V1_SMALL, HISTORY_BOUND_V1_SMALL]);
    let a = admit(&p).unwrap();
    // Every attention cone reads histories, and every cone that reads one is dissected.
    for c in &a.cones {
        let hist = c.leaves.iter().any(|l| matches!(l, LeafV1::History(_)));
        assert_eq!(hist, !c.h_reductions.is_empty(), "block {} node {}: histories and H-reductions go together here", c.block, c.node);
        assert!(c.nodes.last() == Some(&c.node), "the commit point closes its cone");
    }
}

// ---- refusals ------------------------------------------------------------------------------------

fn tiny(
    build: impl FnOnce(&mut misaka_palw_tir::builder::BlockBuilder<'_>, &mut Vec<Ref>) -> Ref,
    params: &[(&str, DType, &[u32])],
) -> TirProgramV1 {
    let mut pb = ProgramBuilder::new(16, HISTORY_BOUND_V1_SMALL);
    let mut refs: Vec<Ref> = params.iter().map(|(n, d, s)| pb.param(n, *d, s, false)).collect();
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let r = build(&mut b, &mut refs);
        let r = b.clamp(r, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let r = b.reshape(r, &[Dim::Fixed(b.ty(r).elements_at(1) as u32)]);
        let r = b.commit(r);
        b.finish(&[r])
    };
    let t = pb.blocks[pre as usize].nodes.last().unwrap().out.clone();
    let post = {
        let mut b = pb.block("post", vec![t.clone()]);
        let l = b.reshape(Ref::CarryIn(0), &t.shape);
        b.commit(l);
        b.finish(&[])
    };
    pb.finish(pre, vec![], post, 0)
}

fn refused(p: &TirProgramV1) -> TirAdmitError {
    admit(p).expect_err("refused")
}

fn class(e: &TirAdmitError) -> Option<TirErrorKind> {
    match e {
        TirAdmitError::Program(e) => Some(e.kind),
        _ => None,
    }
}

#[test]
fn every_defect_class_is_refused_by_rule() {
    // Overflow: two i32 params multiplied into an i32 (range analysis, §7).
    let p = tiny(|b, r| b.mul(r[0], r[1], DType::I32), &[("a", DType::I32, &[4]), ("b", DType::I32, &[4])]);
    assert_eq!(class(&refused(&p)), Some(TirErrorKind::Overflow));
    // An index out of range: a 4-row table gathered by a token that reaches 15.
    let p = tiny(|b, r| b.gather(r[0], Ref::Input(INPUT_TOKEN), 0, 0), &[("table", DType::I16, &[4, 3])]);
    assert_eq!(class(&refused(&p)), Some(TirErrorKind::Index));
    // A bad divisor: a param divisor whose interval reaches 0.
    let p = tiny(|b, r| b.div(r[0], r[1], Rounding::Floor, DType::I32), &[("x", DType::I16, &[4]), ("d", DType::I16, &[4])]);
    assert_eq!(class(&refused(&p)), Some(TirErrorKind::Divisor));
    // A bad shift: an unclamped shift used as an index into the Pow2 table.
    let p = tiny(
        |b, r| {
            let t = b.pb.konst(DType::I64, &[4], &[1, 2, 4, 8]);
            let s = b.cast(r[0], DType::Idx);
            b.gather(t, s, 0, 0)
        },
        &[("s", DType::I8, &[4])],
    );
    assert!(
        matches!(class(&refused(&p)), Some(TirErrorKind::Overflow | TirErrorKind::Index)),
        "a shift that is not clamped into its table"
    );
    // Oversize: a node of 2^29 elements.
    let p = tiny(|b, r| b.broadcast(r[0], &[Dim::Fixed(1 << 15), Dim::Fixed(1 << 14)]), &[("x", DType::I8, &[1])]);
    assert!(matches!(class(&refused(&p)), Some(TirErrorKind::NormalForm | TirErrorKind::Shape)));
    // Oversize bytes: more than 262,144 bytes of program.
    let mut big = tiny(|b, r| b.cast(r[0], DType::I32), &[("x", DType::I8, &[1])]);
    for i in 0..3000 {
        big.params.push(misaka_palw_tir::ParamDecl { name: format!("{i:0>120}"), dtype: DType::I8, shape: vec![1], per_layer: false });
    }
    assert_eq!(class(&refused(&big)), Some(TirErrorKind::Encoding));
    // A dead node.
    let mut dead = tiny(|b, r| b.cast(r[0], DType::I32), &[("x", DType::I8, &[4])]);
    dead.blocks[0]
        .nodes
        .insert(0, Node { prim: Prim::Cast, inputs: vec![Ref::Param(0)], out: TensorType::fixed(DType::I64, &[4]), commit: false });
    for n in dead.blocks[0].nodes.iter_mut().skip(1) {
        for r in n.inputs.iter_mut() {
            if let Ref::Node(j) = r {
                *j += 1;
            }
        }
    }
    let last = dead.blocks[0].nodes.len() as u16 - 1;
    dead.blocks[0].carry_out = vec![last];
    assert_eq!(class(&refused(&dead)), Some(TirErrorKind::NormalForm));
    // Non-canonical bytes: a trailing byte, and a bool of 2.
    let ok = tiny(|b, r| b.cast(r[0], DType::I32), &[("x", DType::I8, &[4])]);
    let mut bytes = ok.encode();
    bytes.push(0);
    assert!(matches!(tir_admit_v1(&bytes, &inputs()), Err(TirAdmitError::Program(e)) if e.kind == TirErrorKind::Encoding));
    // Bad inputs are refused by name.
    let bad = TirAdmitInputsV1 { h_chunk: 12, ..inputs() };
    assert!(matches!(tir_admit_program_v1(&ok, &bad), Err(TirAdmitError::Inputs(_))));
}

/// Primitives that would mix history positions do not type (§6): a TopK, a Slice, a Gather or a
/// Concat along `H` — so a non-dissectable program cannot be written, and admission refuses each.
#[test]
fn a_history_mixing_primitive_does_not_type() {
    let (p, _) = dense(&[HISTORY_BOUND_V1_SMALL, HISTORY_BOUND_V1_SMALL]);
    let layer = p.schedule.layers[0] as usize;
    let hist = p.blocks[layer].nodes.iter().position(|n| matches!(n.prim, Prim::HistAppend { .. })).unwrap();
    let t = p.blocks[layer].nodes[hist].out.clone();
    let mut variants = Vec::new();
    for prim in [Prim::TopK { axis: 0, k: 1 }, Prim::Slice { axis: 0, start: 0 }] {
        let mut q = p.clone();
        let mut out = t.clone();
        out.shape[0] = Dim::Fixed(1);
        if matches!(prim, Prim::TopK { .. }) {
            out.dtype = DType::Idx;
        }
        q.blocks[layer].nodes.push(Node { prim, inputs: vec![Ref::Node(hist as u16)], out, commit: true });
        variants.push(q);
    }
    {
        let mut q = p.clone();
        let idx = Ref::Input(INPUT_POS);
        let mut out = t.clone();
        out.shape.remove(0);
        q.blocks[layer].nodes.push(Node {
            prim: Prim::Gather { axis: 0, batch_dims: 0 },
            inputs: vec![Ref::Node(hist as u16), idx],
            out,
            commit: true,
        });
        variants.push(q);
    }
    {
        let mut q = p.clone();
        let mut out = t.clone();
        out.shape[0] = Dim::H;
        q.blocks[layer].nodes.push(Node {
            prim: Prim::Concat { axis: 0 },
            inputs: vec![Ref::Node(hist as u16), Ref::Node(hist as u16)],
            out,
            commit: true,
        });
        variants.push(q);
    }
    for (i, q) in variants.iter().enumerate() {
        let e = refused(q);
        assert!(matches!(class(&e), Some(TirErrorKind::Shape | TirErrorKind::NormalForm)), "variant {i}: {e}");
    }
}

#[test]
fn a_cone_or_a_replay_past_its_ceiling_is_refused_by_name() {
    let p = dense_sized(2, 256, 4, 2, 64, 512, 1024);
    let base = inputs();
    assert!(tir_admit_program_v1(&p, &base).is_ok());
    // The LM head tile: 64 rows of 256 MACs = 16,384 > 10,000.
    let tight = TirAdmitInputsV1 { ceilings: TirCeilingsV1 { max_tile_macs: 10_000, ..base.ceilings }, ..base };
    assert!(matches!(tir_admit_program_v1(&p, &tight), Err(TirAdmitError::Exceeds { limit: "max_tile_macs", .. })));
    let few = TirAdmitInputsV1 { ceilings: TirCeilingsV1 { max_tile_operands: 1, ..base.ceilings }, ..base };
    assert!(matches!(tir_admit_program_v1(&p, &few), Err(TirAdmitError::Exceeds { limit: "max_tile_operands", .. })));
    let leaves = TirAdmitInputsV1 { ceilings: TirCeilingsV1 { max_step_leaves: 10, ..base.ceilings }, ..base };
    assert!(matches!(tir_admit_program_v1(&p, &leaves), Err(TirAdmitError::Exceeds { limit: "max_step_leaves", .. })));
    // A recurrence whose one-position replay is past the terminal ceiling.
    let (g, _) = gdn_program(true);
    let starved = TirAdmitInputsV1 { ceilings: TirCeilingsV1 { max_tile_macs: 8, ..base.ceilings }, ..base };
    let e = tir_admit_program_v1(&g, &starved).expect_err("refused");
    assert!(matches!(e, TirAdmitError::Exceeds { .. }), "{e}");
}

/// About the most admission work the normal form allows: 14 distinct layer blocks of 512 nodes
/// scheduled over 1,024 layers, 16 per-layer states (15 `Fixed`, one `Hist` at the `2^18` window),
/// each block a 240-node chain that reads every `Fixed` state and the window's maximum, which every
/// one of its 252 other commit points and its 15 `StateWrite`s reach — so every cone is the whole chain
/// and dissected, and every replay closure is all 15 states.
fn admission_worst_case() -> TirProgramV1 {
    let mut pb = ProgramBuilder::new(1, HISTORY_BOUND_V1_SMALL);
    let fixed: Vec<u16> = (0..15).map(|i| pb.fixed_state(&format!("f{i}"), DType::I32, &[1], -1, 1, true)).collect();
    let hist = pb.hist_state("h", DType::I16, &[1], HISTORY_BOUND_V1_SMALL, true);
    let carry = TensorType::fixed(DType::I16, &[1]);
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let c = b.iota(DType::I16, &[Dim::Fixed(1)], 0, 0, 0);
        b.finish(&[c])
    };
    let mut layers = Vec::new();
    for k in 0..14 {
        let mut b = pb.block(&format!("layer{k}"), vec![carry.clone()]);
        let w = b.hist_append(hist, Ref::CarryIn(0));
        let m = b.reduce_max(w, 0);
        let m = b.cast(m, DType::I32);
        let m = b.reshape_fixed(m, &[1]);
        // A clamp that never fires in practice keeps the chain's product in `[-1, 1]` for §7.
        let m = b.clamp(m, -1, 1, DType::I32);
        let mut chain = vec![m];
        for f in &fixed {
            let prev = *chain.last().unwrap();
            chain.push(b.mul(prev, Ref::State(*f), DType::I32));
        }
        while chain.len() < 240 {
            let (x, y) = (chain[chain.len() - 1], chain[chain.len() - 2]);
            chain.push(b.mul(x, y, DType::I32));
        }
        let end = *chain.last().unwrap();
        for f in &fixed {
            b.state_write(*f, end);
        }
        let out = b.reshape_fixed(Ref::CarryIn(0), &[1]);
        let Ref::Node(last) = out else { unreachable!() };
        for i in 0..(512 - 1 - last as usize) {
            let c = if i < chain.len() { b.mul(end, chain[i], DType::I32) } else { b.mul(chain[i - chain.len()], end, DType::I32) };
            b.commit(c);
        }
        layers.push(b.finish(&[out]));
    }
    let post = {
        let mut b = pb.block("post", vec![carry]);
        let l = b.reshape_fixed(Ref::CarryIn(0), &[1]);
        b.commit(l);
        b.finish(&[])
    };
    let schedule = (0..1024).map(|l| layers[l % layers.len()]).collect();
    pb.finish(pre, schedule, post, 0)
}

#[test]
fn admission_work_is_capped_and_the_worst_case_of_the_normal_form_is_refused_by_it() {
    let p = admission_worst_case();
    assert_eq!(p.blocks.iter().map(|b| b.nodes.len()).max(), Some(512));
    // Uncapped, it is admissible: this is the work the cone-work ceiling bounds.
    let uncapped = |work: u64| TirAdmitInputsV1 { ceilings: TirCeilingsV1 { max_cone_work: work, ..inputs().ceilings }, ..inputs() };
    let t0 = Instant::now();
    let a = tir_admit_program_v1(&p, &uncapped(u64::MAX)).expect("admissible but for its admission work");
    let whole = t0.elapsed();
    assert_eq!(a.cones.len(), 14 * (252 + 1) + 2, "every layer commit point and carry, pre's carry and the logits");
    assert!(a.cones.iter().filter(|c| c.nodes.len() > 240).count() >= 14 * 252, "every cone is the whole chain");
    assert!(a.states.iter().all(|s| s.closure.len() == 15), "every replay closure is every Fixed state");
    // At the default ceiling it is refused by name, having done at most the ceiling's work.
    let t0 = Instant::now();
    let e = admit(&p).expect_err("past the default cone-work ceiling");
    let refused_in = t0.elapsed();
    assert!(matches!(e, TirAdmitError::Exceeds { limit: "max_cone_work", .. }), "{e}");
    // The ceiling is exact: the program's own work admits it, one less refuses it.
    assert!(tir_admit_program_v1(&p, &uncapped(a.cone_work)).is_ok());
    assert!(tir_admit_program_v1(&p, &uncapped(a.cone_work - 1)).is_err());
    eprintln!(
        "worst case of the normal form: {} B, cone work {}, admitted uncapped in {whole:?}, refused at the default ceiling ({}) in {refused_in:?}",
        p.encode().len(),
        a.cone_work,
        TirCeilingsV1::legacy_court_v1().max_cone_work
    );
    assert!(whole < Duration::from_secs(10) && refused_in < whole);
    // The corpus and the 1.5B-sized decoder are three orders of magnitude inside the ceiling.
    let q = admit(&dense_sized(28, 1536, 12, 2, 128, 8960, 151_936)).unwrap();
    assert!(q.cone_work * 100 < TirCeilingsV1::legacy_court_v1().max_cone_work, "{}", q.cone_work);
    eprintln!("qwen2.5-1.5b-sized cone work {}", q.cone_work);
}

// ---- §10.3's split rule and the refusals ref2 read differently (A1–A7) -----------------------

fn state_of<'a>(a: &'a TirAdmissionV1, p: &TirProgramV1, name: &str) -> Option<&'a misaka_palw_tir::admit::StateCkptV1> {
    let j = p.states.iter().position(|s| s.name == name).expect("a declared state") as u16;
    a.states.iter().find(|c| c.state == j)
}

/// `Σ` of the §8 costs of `nodes` of the layer block (block 1), component by component.
fn cost_of(a: &TirAdmissionV1, nodes: &[usize]) -> misaka_palw_tir::admit::CostV1 {
    let mut c = misaka_palw_tir::admit::CostV1::default();
    for i in nodes {
        let n = &a.node_costs[1][*i];
        c.macs += n.macs;
        c.elementwise += n.elementwise;
        c.transcendentals += n.transcendentals;
        c.bytes_read += n.bytes_read;
        c.bytes_written += n.bytes_written;
    }
    c
}

/// **A1**: a free node never blocks the split, a commit point is a free leaf (another member's
/// committed `StateWrite` too), and one group pays `⌈aligned / G⌉ + free`, every free node whole.
#[test]
fn the_split_rule_on_the_readings_that_differed() {
    use common::admission::*;
    // A free update splits, and each group pays the free nodes whole: Clamp (0), StateWrite (1).
    let p = free_update();
    let a = admit(&p).unwrap();
    let s = state_of(&a, &p, "S").unwrap();
    assert_eq!(s.groups, 4, "a free update splits");
    assert_eq!(s.per_position, cost_of(&a, &[0, 1]), "every free node whole, per group");
    // A free node inside an aligned update: ⌈(Add + StateWrite) / 4⌉ + Clamp.
    let p = free_node_in_an_aligned_update();
    let a = admit(&p).unwrap();
    let s = state_of(&a, &p, "S").unwrap();
    assert_eq!(s.groups, 4);
    let (aligned, free) = (cost_of(&a, &[1, 2]), cost_of(&a, &[0]));
    assert_eq!(s.per_position.elementwise, aligned.elementwise.div_ceil(4) + free.elementwise);
    assert_eq!(s.per_position.bytes_read, aligned.bytes_read.div_ceil(4) + free.bytes_read);
    // A committed member write is a free leaf: S2's closure is {S1, S2} and it splits.
    let p = a_committed_member_write_is_a_free_leaf();
    let a = admit(&p).unwrap();
    let s2 = state_of(&a, &p, "S2").unwrap();
    assert_eq!((s2.closure.len(), s2.groups), (2, 4), "the Transpose of an opened value is free");
    assert_eq!(state_of(&a, &p, "S1").unwrap().groups, 4);
    // A reduction across groups keeps the replay whole.
    let p = a_reduction_across_groups();
    let a = admit(&p).unwrap();
    assert_eq!(state_of(&a, &p, "S").unwrap().groups, 1);
    // A member of another width keeps it whole too, and a state nobody writes has no C_j (A7).
    let p = a_member_of_another_width_and_a_state_nobody_writes();
    let a = admit(&p).unwrap();
    let s = state_of(&a, &p, "S").unwrap();
    assert_eq!((s.closure.len(), s.groups), (2, 1));
    assert!(state_of(&a, &p, "T").is_none(), "T is read and never written: no C_j");
    assert_eq!(a.checkpoint_interval, s.interval, "C is S's alone");
    // Where the rule decides the verdict: a free product is paid whole by every group ...
    let tight = |macs: u64| TirAdmitInputsV1 { ceilings: TirCeilingsV1 { max_tile_macs: macs, ..inputs().ceilings }, ..inputs() };
    let p = a_free_product_update();
    let s = state_of(&admit(&p).unwrap(), &p, "S").unwrap().clone();
    assert_eq!((s.groups, s.per_position.macs), (4, 512), "the free product, whole, per group");
    let e = tir_admit_program_v1(&p, &tight(256)).unwrap_err();
    assert!(matches!(e, TirAdmitError::Exceeds { limit: "max_tile_macs", value: 512, cap: 256, .. }), "{e}");
    // ... and a committed member write splits the replay four ways: 512 aligned MACs, 128 a group.
    // (Tiles of 16 lanes, so the committed w1's own tile — 16 x 4 MACs — is inside the ceiling and
    // only the replay is at it.)
    let p = a_committed_member_write_decides_the_split();
    let a = tir_admit_program_v1(&p, &TirAdmitInputsV1 { tile_len: 16, ..tight(128) }).expect("admitted at C_j = 1");
    let s2 = state_of(&a, &p, "S2").unwrap();
    assert_eq!((s2.closure.len(), s2.groups, s2.per_position.macs, s2.interval), (2, 4, 128, 1));
    assert_eq!(a.checkpoint_interval, 1);
}

/// **A3**: a `C_j` of 0 names the component past its cap, MACs first, and a zero interval cap is an
/// input refusal; **A2**: every refusal names a ceiling by its field.
#[test]
fn a_replay_past_its_ceiling_names_the_component_and_a_zero_interval_is_an_input() {
    use common::admission::*;
    let base = inputs();
    let with = |f: &dyn Fn(&mut TirCeilingsV1)| {
        let mut c = base.ceilings;
        f(&mut c);
        TirAdmitInputsV1 { ceilings: c, ..base }
    };
    let p = a_replay_of_matmuls();
    let one = state_of(&admit(&p).unwrap(), &p, "S").unwrap().clone();
    assert_eq!((one.groups, one.per_position.macs), (2, 64), "128 MACs a position, 64 a group");
    let e = tir_admit_program_v1(&p, &with(&|c| c.max_tile_macs = 63)).unwrap_err();
    assert!(matches!(e, TirAdmitError::Exceeds { limit: "max_tile_macs", value: 64, cap: 63, .. }), "{e}");
    assert!(tir_admit_program_v1(&p, &with(&|c| c.max_tile_macs = 64)).is_ok(), "one position fits exactly");
    let p = a_replay_of_transcendentals();
    let one = state_of(&admit(&p).unwrap(), &p, "S").unwrap().clone();
    assert_eq!((one.groups, one.per_position.transcendentals, one.per_position.macs), (2, 4, 0));
    let e = tir_admit_program_v1(&p, &with(&|c| c.max_tile_transcendentals = 3)).unwrap_err();
    assert!(matches!(e, TirAdmitError::Exceeds { limit: "max_tile_transcendentals", value: 4, cap: 3, .. }), "{e}");
    let e = tir_admit_program_v1(&p, &with(&|c| c.max_checkpoint_interval = 0)).unwrap_err();
    assert!(matches!(e, TirAdmitError::Inputs(_)), "{e}");
}
