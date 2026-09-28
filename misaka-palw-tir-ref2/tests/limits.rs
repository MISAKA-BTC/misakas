//! Every size cap and boundary of 04b §4.4 and §5 (NF-1..NF-22) at its edge and one past it, and the
//! declaration rules whose readings could differ (names in bytes, duplicates, roles). Each case
//! states what the text requires; this implementation must match the text and the first one must
//! match this one.

mod common;

use common::bridge::*;
use misaka_palw_tir_ref2 as ref2;
use ref2::build::{ProgBuilder, fixed, ty};
use ref2::codec::{decode_canonical, encode};
use ref2::{DType, Dim, Prim, Program, Ref, StateKind};

const HB: u32 = 1 << 18;

/// pre: Iota → committed carry i32[2]; post: Clamp → logits.
fn base() -> ProgBuilder {
    let mut b = ProgBuilder::new(HB, 16);
    let pre = b.block("pre", vec![]);
    let n0 = b.node(pre, Prim::Iota { axis: 0, start: 0, step: 1 }, &[], fixed(DType::I32, &[2]), true);
    b.carry_out(pre, &[n0]);
    let post = b.block("post", vec![fixed(DType::I32, &[2])]);
    let m0 = b.node(post, Prim::Clamp { lo: -5, hi: 5 }, &[Ref::CarryIn(0)], fixed(DType::I32, &[2]), true);
    b.schedule(pre, &[], post, m0);
    b
}

fn with(f: impl FnOnce(&mut ProgBuilder)) -> Program {
    let mut b = base();
    f(&mut b);
    b.finish()
}

/// Makes every param, const and state used: each one is read by a committed node of the block of
/// its role (Concat of up to 8 same-typed operands, Cast when alone).
fn use_all(b: &mut ProgBuilder, block: usize, refs: &[Ref], dtype: DType) {
    for chunk in refs.chunks(8) {
        if chunk.len() == 1 {
            b.node(block, Prim::Cast, chunk, fixed(dtype, &[1]), true);
        } else {
            b.node(block, Prim::Concat { axis: 0 }, chunk, fixed(dtype, &[chunk.len() as u32]), true);
        }
    }
}

struct Case {
    name: String,
    bytes: Vec<u8>,
    text_accepts: bool,
}

fn case(name: impl Into<String>, p: &Program, text_accepts: bool) -> Case {
    Case { name: name.into(), bytes: encode(p), text_accepts }
}

fn cases() -> Vec<Case> {
    let mut v = Vec::new();
    v.push(case("base", &with(|_| {}), true));
    // NF-1
    for (hb, ok) in [(1u32 << 18, true), (1 << 21, true), ((1 << 18) - 1, false), (1 << 19, false), (1 << 22, false), (0, false)] {
        v.push(case(format!("history_bound {hb}"), &with(|b| b.p.history_bound = hb), ok));
    }
    for (tb, ok) in [(0u32, false), (1, true), (u32::MAX, true)] {
        v.push(case(format!("token_bound {tb}"), &with(|b| b.p.token_bound = tb), ok));
    }
    v.push(case("version 0", &with(|b| b.p.version = 0), false));
    // prim_set_id and logits_scheme_id: the text checks neither (see F-PRIMSET).
    v.push(case("prim_set_id all 0xFF", &with(|b| b.p.prim_set_id = [0xFF; 64]), true));
    v.push(case("logits_scheme_id all 0xFF", &with(|b| b.p.logits_scheme_id = [0xFF; 64]), true));
    // NF-2 blocks: 16 accepted, 17 refused (every block scheduled once as a layer).
    for (n, ok) in [(16usize, true), (17, false)] {
        v.push(case(
            format!("{n} blocks"),
            &with(|b| {
                let mut layers = Vec::new();
                for i in 0..n - 2 {
                    let l = b.block(&format!("l{i}"), vec![fixed(DType::I32, &[2])]);
                    let x = b.node(l, Prim::Clamp { lo: -9, hi: 9 }, &[Ref::CarryIn(0)], fixed(DType::I32, &[2]), true);
                    b.carry_out(l, &[x]);
                    layers.push(l);
                }
                b.schedule(0, &layers, 1, 0);
            }),
            ok,
        ));
    }
    // NF-2 layers: 1024 accepted, 1025 refused.
    for (n, ok) in [(1024usize, true), (1025, false)] {
        v.push(case(
            format!("{n} layers"),
            &with(|b| {
                let l = b.block("l", vec![fixed(DType::I32, &[2])]);
                let x = b.node(l, Prim::Clamp { lo: -9, hi: 9 }, &[Ref::CarryIn(0)], fixed(DType::I32, &[2]), true);
                b.carry_out(l, &[x]);
                b.schedule(0, &vec![l; n], 1, 0);
            }),
            ok,
        ));
    }
    // NF-2 params: 4096 accepted, 4097 refused (all used).
    for (n, ok) in [(4096usize, true), (4097, false)] {
        v.push(case(
            format!("{n} params"),
            &with(|b| {
                let refs: Vec<Ref> = (0..n).map(|i| Ref::Param(b.param(&format!("p{i}"), DType::I8, &[1], false))).collect();
                // pre has 1 node already; 511 more nodes of 8 params, the rest in post.
                let (a, c) = refs.split_at(511 * 8);
                use_all(b, 0, a, DType::I8);
                use_all(b, 1, c, DType::I8);
            }),
            ok,
        ));
    }
    // NF-2 states: 64 accepted / 65 refused; 16 per-layer accepted / 17 refused.
    for (global, per_layer, ok) in [(48usize, 16usize, true), (49, 16, false), (47, 17, false), (64, 0, true), (65, 0, false)] {
        v.push(case(
            format!("{global} global + {per_layer} per-layer states"),
            &with(|b| {
                let g: Vec<Ref> = (0..global).map(|i| Ref::State(b.fixed_state(&format!("g{i}"), DType::I8, &[1], 0, 1, false))).collect();
                let pl: Vec<Ref> = (0..per_layer).map(|i| Ref::State(b.fixed_state(&format!("l{i}"), DType::I8, &[1], 0, 1, true))).collect();
                use_all(b, 0, &g, DType::I8);
                if per_layer > 0 {
                    let l = b.block("layer", vec![fixed(DType::I32, &[2])]);
                    let x = b.node(l, Prim::Clamp { lo: -9, hi: 9 }, &[Ref::CarryIn(0)], fixed(DType::I32, &[2]), true);
                    b.carry_out(l, &[x]);
                    use_all(b, l, &pl, DType::I8);
                    b.schedule(0, &[l], 1, 0);
                }
            }),
            ok,
        ));
    }
    // NF-12 nodes: 512 accepted, 513 refused.
    for (n, ok) in [(512usize, true), (513, false)] {
        v.push(case(
            format!("{n} nodes in pre"),
            &with(|b| {
                for i in 1..n {
                    b.node(0, Prim::Iota { axis: 0, start: i as i64, step: 1 }, &[], fixed(DType::I32, &[2]), true);
                }
            }),
            ok,
        ));
    }
    v.push(case("0 nodes in post", &with(|b| b.p.blocks[1].nodes.clear()), false));
    // NF-4 carries: 8 accepted, 9 refused.
    for (n, ok) in [(8usize, true), (9, false)] {
        v.push(case(
            format!("{n} carries"),
            &with(|b| {
                let outs: Vec<u16> = (0..n).map(|i| b.node(0, Prim::Iota { axis: 0, start: i as i64, step: 1 }, &[], fixed(DType::I32, &[2]), true)).collect();
                b.carry_out(0, &outs);
                b.p.blocks[1].carry_in = vec![fixed(DType::I32, &[2]); n];
            }),
            ok,
        ));
    }
    // NF-14 inputs: Concat of 8 accepted, 9 refused.
    for (n, ok) in [(8usize, true), (9, false)] {
        v.push(case(
            format!("Concat of {n}"),
            &with(|b| {
                let ins = vec![Ref::Node(0); n];
                b.node(0, Prim::Concat { axis: 0 }, &ins, fixed(DType::I32, &[2 * n as u32]), true);
            }),
            ok,
        ));
    }
    // NF-7 names, in bytes.
    for (name, ok) in [
        ("x".repeat(128), true),
        ("x".repeat(129), false),
        ("é".repeat(64), true),
        (format!("{}a", "é".repeat(64)), false),
        (String::new(), false),
    ] {
        let l = format!("{}{}", name.len(), if name.is_ascii() { "" } else { " (UTF-8 two-byte)" });
        v.push(case(
            format!("param name of {l} bytes"),
            &with(|b| {
                let j = b.param(&name, DType::I32, &[2], false);
                b.node(1, Prim::Add, &[Ref::CarryIn(0), Ref::Param(j)], fixed(DType::I64, &[2]), true);
                b.p.blocks[1].nodes.last_mut().unwrap().out.dtype = DType::I32;
                b.p.blocks[1].nodes.last_mut().unwrap().commit = true;
            }),
            ok,
        ));
        v.push(case(
            format!("state name of {l} bytes"),
            &with(|b| {
                let j = b.fixed_state(&name, DType::I32, &[2], 0, 0, false);
                b.node(0, Prim::StateWrite { state: j }, &[Ref::Node(0)], fixed(DType::I32, &[2]), false);
            }),
            ok,
        ));
        v.push(case(format!("block name of {l} bytes"), &with(|b| b.p.blocks[0].name = name.clone()), ok));
    }
    v.push(case(
        "duplicate param names",
        &with(|b| {
            let a = b.param("w", DType::I32, &[2], false);
            let c = b.param("w", DType::I32, &[2], false);
            b.node(1, Prim::Concat { axis: 0 }, &[Ref::Param(a), Ref::Param(c)], fixed(DType::I32, &[4]), true);
        }),
        false,
    ));
    v.push(case(
        "a param and a state of the same name",
        &with(|b| {
            let a = b.param("w", DType::I32, &[2], false);
            let s = b.fixed_state("w", DType::I32, &[2], 0, 0, false);
            b.node(1, Prim::Concat { axis: 0 }, &[Ref::Param(a), Ref::State(s)], fixed(DType::I32, &[4]), true);
        }),
        true,
    ));
    v.push(case("duplicate block names", &with(|b| b.p.blocks[1].name = "pre".into()), true));
    // NF-9 consts: identical refused; same bytes of another dtype accepted; 65,536 bytes accepted.
    v.push(case(
        "duplicate consts",
        &with(|b| {
            b.p.consts.push(ref2::ConstDecl { dtype: DType::I8, shape: vec![2], data: vec![1, 2] });
            b.p.consts.push(ref2::ConstDecl { dtype: DType::I8, shape: vec![2], data: vec![1, 2] });
            b.node(1, Prim::Concat { axis: 0 }, &[Ref::Const(0), Ref::Const(1)], fixed(DType::I8, &[4]), true);
        }),
        false,
    ));
    v.push(case(
        "same const bytes, another dtype",
        &with(|b| {
            b.p.consts.push(ref2::ConstDecl { dtype: DType::I8, shape: vec![2], data: vec![1, 2] });
            b.p.consts.push(ref2::ConstDecl { dtype: DType::I16, shape: vec![1], data: vec![1, 2] });
            b.node(1, Prim::Cast, &[Ref::Const(0)], fixed(DType::I8, &[2]), true);
            b.node(1, Prim::Cast, &[Ref::Const(1)], fixed(DType::I8, &[1]), false);
            b.p.blocks[1].nodes.last_mut().unwrap().commit = true;
        }),
        true,
    ));
    for (n, ok) in [(65_536u32, true), (65_537, false)] {
        v.push(case(
            format!("consts of {n} bytes"),
            &with(|b| {
                b.p.consts.push(ref2::ConstDecl { dtype: DType::I8, shape: vec![n], data: vec![3; n as usize] });
                b.node(1, Prim::ReduceMax { axis: 0 }, &[Ref::Const(0)], fixed(DType::I8, &[1]), true);
            }),
            ok,
        ));
    }
    v.push(case(
        "const data one byte short",
        &with(|b| {
            b.p.consts.push(ref2::ConstDecl { dtype: DType::I16, shape: vec![2], data: vec![1, 2, 3] });
            b.node(1, Prim::Cast, &[Ref::Const(0)], fixed(DType::I8, &[2]), true);
        }),
        false,
    ));
    // NF-8 declared shapes.
    for (shape, ok) in [
        (vec![1u32 << 24, 1 << 16], true),
        (vec![1 << 24, (1 << 16) + 1], false),
        (vec![(1 << 24) + 1], false),
        (vec![0], false),
        (vec![1, 1, 1, 1], true),
        (vec![1, 1, 1, 1, 1], false),
    ] {
        v.push(case(
            format!("param shape {shape:?}"),
            &with(|b| {
                let j = b.param("w", DType::I8, &shape, false);
                let mut out = shape.clone();
                if !out.is_empty() {
                    out[0] = 1;
                }
                let axis = 0;
                if shape.first().is_some_and(|&d| d >= 1) {
                    b.node(1, Prim::Slice { axis, start: 0 }, &[Ref::Param(j)], fixed(DType::I8, &out), true);
                } else {
                    b.node(1, Prim::Cast, &[Ref::Param(j)], fixed(DType::I8, &shape), true);
                }
            }),
            ok,
        ));
    }
    v.push(case(
        "i128 param",
        &with(|b| {
            let j = b.param("w", DType::I128, &[2], false);
            b.node(1, Prim::Cast, &[Ref::Param(j)], fixed(DType::I32, &[2]), true);
        }),
        false,
    ));
    v.push(case(
        "idx param and i128 const",
        &with(|b| {
            let j = b.param("w", DType::Idx, &[2], false);
            b.p.consts.push(ref2::ConstDecl { dtype: DType::I128, shape: vec![1], data: vec![0xFF; 16] });
            b.node(1, Prim::Add, &[Ref::Param(j), Ref::Const(0)], fixed(DType::I128, &[2]), false);
            b.node(1, Prim::Clamp { lo: -1, hi: 1 }, &[Ref::Node(1)], fixed(DType::I32, &[2]), true);
        }),
        true,
    ));
    for (shape, ok) in [(vec![1u32 << 14, 1 << 14], true), (vec![1 << 14, 1 << 14, 2], false)] {
        v.push(case(
            format!("Fixed state shape {shape:?}"),
            &with(|b| {
                let s = b.fixed_state("s", DType::I8, &shape, 0, 0, false);
                b.node(1, Prim::ReduceMax { axis: 0 }, &[Ref::State(s)], fixed(DType::I8, &{
                    let mut o = shape.clone();
                    o[0] = 1;
                    o
                }), true);
            }),
            ok,
        ));
    }
    // NF-10 states.
    for (dt, lo, hi, ok) in [
        (DType::I8, -128i64, 127i64, true),
        (DType::I8, -129, 0, false),
        (DType::I8, 0, 128, false),
        (DType::I8, 1, 5, false),
        (DType::I8, -5, -1, false),
        (DType::I64, 0, 0, false),
        (DType::Idx, 0, 0, false),
        (DType::I128, 0, 0, false),
    ] {
        v.push(case(
            format!("Fixed state {} [{lo}, {hi}]", dt.name()),
            &with(|b| {
                let s = b.fixed_state("s", dt, &[2], lo, hi, false);
                b.node(1, Prim::Cast, &[Ref::State(s)], fixed(DType::I32, &[2]), true);
            }),
            ok,
        ));
    }
    for (w, rank, ok) in [(1u32, 1usize, true), (HB, 1, true), (HB + 1, 1, false), (0, 1, false), (3, 3, true), (3, 4, false)] {
        v.push(case(
            format!("Hist window {w} row rank {rank}"),
            &with(|b| {
                let row: Vec<u32> = vec![1; rank];
                let h = b.hist_state("h", DType::I32, &row, w, false);
                let r = b.node(0, Prim::Reshape, &[Ref::Node(0)], fixed(DType::I32, &{
                    let mut x = row.clone();
                    x[rank - 1] = 2;
                    x
                }), true);
                let mut rs = row.clone();
                rs[rank - 1] = 2;
                b.p.states[h as usize].shape = rs.clone();
                let mut out = vec![Dim::H];
                out.extend(rs.iter().map(|&d| Dim::Fixed(d)));
                b.node(0, Prim::HistAppend { state: h }, &[Ref::Node(r)], ty(DType::I32, &out), false);
            }),
            ok,
        ));
    }
    // Roles (NF-15), H in carries (NF-5) and logits (NF-6), HistAppend inputs (NF-20).
    v.push(case(
        "per-layer param read in post",
        &with(|b| {
            let j = b.param("w", DType::I32, &[2], true);
            b.node(1, Prim::Cast, &[Ref::Param(j)], fixed(DType::I32, &[2]), true);
        }),
        false,
    ));
    v.push(case(
        "global param read in a layer block",
        &with(|b| {
            let j = b.param("w", DType::I32, &[2], false);
            let l = b.block("l", vec![fixed(DType::I32, &[2])]);
            b.node(l, Prim::Add, &[Ref::CarryIn(0), Ref::Param(j)], fixed(DType::I64, &[2]), false);
            let x = b.node(l, Prim::Clamp { lo: -9, hi: 9 }, &[Ref::Node(0)], fixed(DType::I32, &[2]), true);
            b.carry_out(l, &[x]);
            b.schedule(0, &[l, l], 1, 0);
        }),
        true,
    ));
    v.push(case(
        "HistAppend of a param row",
        &with(|b| {
            let j = b.param("w", DType::I32, &[2], false);
            let h = b.hist_state("h", DType::I32, &[2], 4, false);
            b.node(0, Prim::HistAppend { state: h }, &[Ref::Param(j)], ty(DType::I32, &[Dim::H, Dim::Fixed(2)]), false);
        }),
        false,
    ));
    v.push(case(
        "HistAppend of an uncommitted node",
        &with(|b| {
            let h = b.hist_state("h", DType::I32, &[2], 4, false);
            let r = b.node(0, Prim::Iota { axis: 0, start: 0, step: 2 }, &[], fixed(DType::I32, &[2]), false);
            b.node(0, Prim::HistAppend { state: h }, &[Ref::Node(r)], ty(DType::I32, &[Dim::H, Dim::Fixed(2)]), false);
        }),
        false,
    ));
    v.push(case(
        "HistAppend of a carry-in (post)",
        &with(|b| {
            let h = b.hist_state("h", DType::I32, &[2], 4, false);
            b.node(1, Prim::HistAppend { state: h }, &[Ref::CarryIn(0)], ty(DType::I32, &[Dim::H, Dim::Fixed(2)]), false);
        }),
        true,
    ));
    v.push(case(
        "H in a block without a window",
        &with(|b| {
            b.node(0, Prim::Iota { axis: 0, start: 0, step: 1 }, &[], ty(DType::I32, &[Dim::H]), true);
        }),
        false,
    ));
    v.push(case(
        "two H in one shape",
        &with(|b| {
            let h = b.hist_state("h", DType::I32, &[2], 4, false);
            b.node(0, Prim::HistAppend { state: h }, &[Ref::Node(0)], ty(DType::I32, &[Dim::H, Dim::Fixed(2)]), false);
            b.node(0, Prim::Iota { axis: 0, start: 0, step: 1 }, &[], ty(DType::I32, &[Dim::H, Dim::H]), true);
        }),
        false,
    ));
    v.push(case(
        "logits with H",
        &with(|b| {
            let h = b.hist_state("h", DType::I32, &[2], 4, false);
            let a = b.node(1, Prim::HistAppend { state: h }, &[Ref::CarryIn(0)], ty(DType::I32, &[Dim::H, Dim::Fixed(2)]), true);
            b.p.logits = a;
        }),
        false,
    ));
    v.push(case(
        "an i64 carry",
        &with(|b| {
            b.p.blocks[0].nodes[0].out.dtype = DType::I64;
            b.p.blocks[1].carry_in[0].dtype = DType::I64;
        }),
        false,
    ));
    v.push(case("an i64 committed node", &with(|b| {
        b.node(0, Prim::Cast, &[Ref::Node(0)], fixed(DType::I64, &[2]), true);
    }), false));
    v.push(case("TopK uncommitted", &with(|b| {
        b.node(0, Prim::TopK { axis: 0, k: 1 }, &[Ref::Node(0)], fixed(DType::Idx, &[1]), false);
        b.node(0, Prim::Cast, &[Ref::Node(1)], fixed(DType::I32, &[1]), true);
    }), false));
    v.push(case("two StateWrites of one state in a block", &with(|b| {
        let s = b.fixed_state("s", DType::I32, &[2], -1, 1, false);
        b.node(0, Prim::StateWrite { state: s }, &[Ref::Node(0)], fixed(DType::I32, &[2]), false);
        b.node(0, Prim::StateWrite { state: s }, &[Ref::Node(0)], fixed(DType::I32, &[2]), false);
    }), false));
    v.push(case("StateWrite of a Hist state", &with(|b| {
        let s = b.hist_state("h", DType::I32, &[2], 3, false);
        b.node(0, Prim::StateWrite { state: s }, &[Ref::Node(0)], fixed(DType::I32, &[2]), false);
    }), false));
    v.push(case("a State ref to a Hist state", &with(|b| {
        let s = b.hist_state("h", DType::I32, &[2], 3, false);
        b.node(0, Prim::HistAppend { state: s }, &[Ref::Node(0)], ty(DType::I32, &[Dim::H, Dim::Fixed(2)]), false);
        b.node(0, Prim::Cast, &[Ref::State(s)], fixed(DType::I32, &[2]), true);
    }), false));
    v.push(case("an unused Hist state", &with(|b| {
        b.hist_state("h", DType::I32, &[2], 3, false);
    }), false));
    v.push(case("Input(2)", &with(|b| {
        b.node(0, Prim::Cast, &[Ref::Input(2)], fixed(DType::I32, &[]), true);
    }), false));
    v.push(case("a node reading itself", &with(|b| {
        b.node(0, Prim::Cast, &[Ref::Node(1)], fixed(DType::I32, &[2]), true);
    }), false));
    v.push(case("one block (pre = post)", &with(|b| {
        b.p.blocks.truncate(1);
        b.p.blocks[0].carry_out.clear();
        b.schedule(0, &[], 0, 0);
    }), false));
    v.push(case("a layer block with windows 3 and 4", &with(|b| {
        let h1 = b.hist_state("h1", DType::I32, &[2], 3, false);
        let h2 = b.hist_state("h2", DType::I32, &[2], 4, false);
        b.node(0, Prim::HistAppend { state: h1 }, &[Ref::Node(0)], ty(DType::I32, &[Dim::H, Dim::Fixed(2)]), false);
        b.node(0, Prim::HistAppend { state: h2 }, &[Ref::Node(0)], ty(DType::I32, &[Dim::H, Dim::Fixed(2)]), false);
    }), false));
    v.push(case("a global Hist appended by pre and post", &with(|b| {
        let h = b.hist_state("h", DType::I32, &[2], 3, false);
        b.node(0, Prim::HistAppend { state: h }, &[Ref::Node(0)], ty(DType::I32, &[Dim::H, Dim::Fixed(2)]), false);
        b.node(1, Prim::HistAppend { state: h }, &[Ref::CarryIn(0)], ty(DType::I32, &[Dim::H, Dim::Fixed(2)]), false);
    }), true));
    v
}

#[test]
fn every_limit_at_and_past_its_edge() {
    let mut disagree = Vec::new();
    let mut text = Vec::new();
    let all = cases();
    for c in &all {
        let a = mine(decode_canonical(&c.bytes)).is_ok();
        let f = first_decode(&c.bytes);
        let fs = match &f {
            Outcome::Ok(_) => "accept".to_string(),
            Outcome::Err(k) => format!("refuse ({})", k.name()),
            Outcome::Panic(s) => format!("PANIC {s}"),
        };
        let ms = match decode_canonical(&c.bytes) {
            Ok(_) => "accept".to_string(),
            Err(e) => format!("refuse ({})", e.class.name()),
        };
        println!("{:55} text {:7} ref2 {:22} first {}", c.name, if c.text_accepts { "accept" } else { "refuse" }, ms, fs);
        if a != c.text_accepts {
            text.push(c.name.clone());
        }
        if a != f.is_ok() || matches!(f, Outcome::Panic(_)) {
            disagree.push(c.name.clone());
        }
    }
    println!("{} limit cases; ref2 differs from the text on {:?}; ref2 and first differ on {:?}", all.len(), text, disagree);
    assert!(text.is_empty());
    assert!(disagree.is_empty());
}

#[test]
fn the_program_byte_cap() {
    // A program of exactly 262,144 bytes is decoded; one of 262,145 is refused (§4.4). Built from
    // params with 128-byte names, then padded exactly with a const.
    let build = |pad: u32| -> Program {
        let mut b = base();
        let mut refs = Vec::new();
        for i in 0..1400 {
            let name = format!("{i:0>128}");
            refs.push(Ref::Param(b.param(&name, DType::I8, &[1], false)));
        }
        use_all(&mut b, 1, &refs, DType::I8);
        b.p.consts.push(ref2::ConstDecl { dtype: DType::I8, shape: vec![pad], data: vec![1; pad as usize] });
        let c = (b.p.consts.len() - 1) as u16;
        b.node(1, Prim::ReduceMax { axis: 0 }, &[Ref::Const(c)], fixed(DType::I8, &[1]), true);
        b.finish()
    };
    let l0 = encode(&build(1)).len() as u32 - 1;
    assert!(l0 < 262_144 && 262_144 - l0 <= 65_536, "{l0}");
    for (target, ok) in [(262_144u32, true), (262_145, false)] {
        let p = build(target - l0);
        let bytes = encode(&p);
        assert_eq!(bytes.len() as u32, target);
        let a = decode_canonical(&bytes);
        let f = first_decode(&bytes);
        println!("program of {} bytes: ref2 {:?} / first {:?}", bytes.len(), a.as_ref().map(|_| ()).map_err(|e| e.class), f.clone().is_ok());
        assert_eq!(a.is_ok(), ok);
        assert_eq!(f.is_ok(), ok);
    }
    let _ = StateKind::Hist { window: 1 };
}
