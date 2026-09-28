//! Structural normal form and type/shape inference (spec 04b §3.4–3.6, PALW-TIR-6/7/8).
//!
//! This is the part of admission the interpreter itself needs in order to be total: after
//! [`validate`] passes, every node's operands exist, have the types the primitive requires, and
//! produce exactly the declared output shape at every `H`. The range, cost and court-cone analyses
//! (PALW-TIR-9/12/13) are `tir_admit_v1`, Gate 2.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::{TirErrorKind, TirResult, err};
use crate::prim::Prim;
use crate::program::*;
use crate::types::{DType, Dim, MAX_DIM, MAX_ELEMENTS, MAX_RANK, TensorType, broadcast_shapes};

/// What validation learned about one block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockInfo {
    /// The window every `Hist` state this block appends to shares; `H = min(pos + 1, window)`.
    pub window: Option<u32>,
    pub is_pre: bool,
    pub is_post: bool,
    pub is_layer: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramInfo {
    pub blocks: Vec<BlockInfo>,
    /// The carry signature every layer block reads and writes.
    pub carry: Vec<TensorType>,
}

fn nf<T>(msg: impl Into<String>) -> TirResult<T> {
    err(TirErrorKind::NormalForm, msg)
}

fn shape_err<T>(block: usize, node: usize, prim: &Prim, msg: impl Into<String>) -> TirResult<T> {
    err(TirErrorKind::Shape, format!("block {block} node {node} ({}): {}", prim.name(), msg.into()))
}

fn check_name(what: &str, name: &str) -> TirResult<()> {
    if name.is_empty() || name.len() > MAX_NAME_BYTES {
        return nf(format!("{what} name must be 1..={MAX_NAME_BYTES} bytes"));
    }
    Ok(())
}

fn check_static_shape(what: &str, shape: &[u32], max_rank: usize) -> TirResult<u64> {
    if shape.len() > max_rank {
        return nf(format!("{what}: rank {} exceeds {max_rank}", shape.len()));
    }
    let mut n = 1u64;
    for d in shape {
        if *d == 0 || *d > MAX_DIM {
            return nf(format!("{what}: dimension {d} outside [1, 2^24]"));
        }
        n = n.saturating_mul(*d as u64);
    }
    if n > MAX_ELEMENTS {
        return nf(format!("{what}: {n} elements exceed 2^28"));
    }
    Ok(n)
}

fn check_tensor_type(t: &TensorType, window: Option<u32>) -> Result<(), String> {
    if t.rank() > MAX_RANK {
        return Err(format!("rank {} exceeds {MAX_RANK}", t.rank()));
    }
    let mut hs = 0;
    for d in &t.shape {
        match d {
            Dim::Fixed(n) => {
                if *n == 0 || *n > MAX_DIM {
                    return Err(format!("dimension {n} outside [1, 2^24]"));
                }
            }
            Dim::H => hs += 1,
        }
    }
    if hs > 1 {
        return Err("more than one H dimension".into());
    }
    if hs == 1 && window.is_none() {
        return Err("an H dimension in a block that appends to no history".into());
    }
    if t.elements_at(window.unwrap_or(1) as u64) > MAX_ELEMENTS {
        return Err("more than 2^28 elements at the worst-case H".into());
    }
    Ok(())
}

fn fixed_shape(shape: &[u32]) -> Vec<Dim> {
    shape.iter().map(|d| Dim::Fixed(*d)).collect()
}

fn prod(dims: &[Dim]) -> u64 {
    dims.iter().map(|d| if let Dim::Fixed(n) = d { *n as u64 } else { 1 }).product()
}

fn reshape_compatible(a: &[Dim], b: &[Dim]) -> bool {
    let ha = a.iter().position(|d| d.is_h());
    let hb = b.iter().position(|d| d.is_h());
    match (ha, hb) {
        (None, None) => prod(a) == prod(b),
        (Some(i), Some(j)) => prod(&a[..i]) == prod(&b[..j]) && prod(&a[i + 1..]) == prod(&b[j + 1..]),
        _ => false,
    }
}

/// Infer (or, for the primitives whose target IS the declared type, check) one node's type.
fn check_node_type(prim: &Prim, ins: &[TensorType], out: &TensorType, states: &[StateDecl]) -> Result<(), String> {
    let same_dtype = |t: &TensorType| -> Result<(), String> {
        if t.dtype != out.dtype {
            Err(format!("out dtype {} must equal the input's {}", out.dtype.name(), t.dtype.name()))
        } else {
            Ok(())
        }
    };
    let want_shape = |s: Vec<Dim>| -> Result<(), String> {
        if s != out.shape { Err(format!("declared shape {:?} is not the inferred {:?}", out.shape, s)) } else { Ok(()) }
    };
    let not_wide_or_idx = |t: &TensorType, what: &str| -> Result<(), String> {
        if matches!(t.dtype, DType::I128) { Err(format!("{what} may not be i128")) } else { Ok(()) }
    };
    match prim {
        Prim::Reshape => {
            same_dtype(&ins[0])?;
            if !reshape_compatible(&ins[0].shape, &out.shape) {
                return Err(format!("cannot reshape {:?} into {:?}", ins[0].shape, out.shape));
            }
            Ok(())
        }
        Prim::Transpose { perm } => {
            same_dtype(&ins[0])?;
            let r = ins[0].rank();
            let mut seen = vec![false; r];
            if perm.len() != r || perm.iter().any(|p| (*p as usize) >= r || std::mem::replace(&mut seen[*p as usize], true)) {
                return Err(format!("{perm:?} is not a permutation of rank {r}"));
            }
            want_shape(perm.iter().map(|p| ins[0].shape[*p as usize]).collect())
        }
        Prim::Slice { axis, start } => {
            same_dtype(&ins[0])?;
            let a = *axis as usize;
            if a >= ins[0].rank() || out.rank() != ins[0].rank() {
                return Err("axis or rank mismatch".into());
            }
            let (Dim::Fixed(n), Dim::Fixed(len)) = (ins[0].shape[a], out.shape[a]) else {
                return Err("a slice axis must be constant on both sides".into());
            };
            if *start as u64 + len as u64 > n as u64 {
                return Err(format!("[{start}, {start}+{len}) exceeds {n}"));
            }
            let mut s = ins[0].shape.clone();
            s[a] = Dim::Fixed(len);
            want_shape(s)
        }
        Prim::Concat { axis } => {
            let a = *axis as usize;
            if a >= out.rank() {
                return Err("axis out of range".into());
            }
            let mut total = 0u64;
            for t in ins {
                same_dtype(t)?;
                if t.rank() != out.rank() {
                    return Err("rank mismatch".into());
                }
                for (i, (x, y)) in t.shape.iter().zip(&out.shape).enumerate() {
                    if i == a {
                        match x {
                            Dim::Fixed(n) => total += *n as u64,
                            Dim::H => return Err("a concat axis must be constant".into()),
                        }
                    } else if x != y {
                        return Err(format!("dimension {i} differs"));
                    }
                }
            }
            if out.shape[a] != Dim::Fixed(total.min(u32::MAX as u64) as u32) {
                return Err(format!("concat axis must total {total}"));
            }
            Ok(())
        }
        Prim::Broadcast => {
            same_dtype(&ins[0])?;
            let (i, o) = (&ins[0].shape, &out.shape);
            if i.len() > o.len() {
                return Err("broadcast cannot drop dimensions".into());
            }
            for (k, d) in i.iter().enumerate() {
                let od = o[o.len() - i.len() + k];
                if *d != od && *d != Dim::Fixed(1) {
                    return Err(format!("dimension {d:?} cannot broadcast to {od:?}"));
                }
            }
            Ok(())
        }
        Prim::Iota { axis, .. } => {
            if (*axis as usize) >= out.rank() {
                return Err("axis out of range".into());
            }
            Ok(())
        }
        Prim::Gather { axis, batch_dims } => {
            let (d, ix) = (&ins[0], &ins[1]);
            same_dtype(d)?;
            not_wide_or_idx(ix, "a gather index")?;
            let (a, b) = (*axis as usize, *batch_dims as usize);
            if a >= d.rank() || b > a || b > ix.rank() {
                return Err("axis/batch_dims out of range".into());
            }
            if d.shape[..b] != ix.shape[..b] {
                return Err("batch dimensions differ".into());
            }
            if d.shape[a].is_h() {
                return Err("cannot gather along H".into());
            }
            let mut s: Vec<Dim> = d.shape[..a].to_vec();
            s.extend_from_slice(&ix.shape[b..]);
            s.extend_from_slice(&d.shape[a + 1..]);
            want_shape(s)
        }
        Prim::Cast | Prim::Log2Floor => want_shape(ins[0].shape.clone()),
        Prim::IntExp | Prim::IntRsqrt | Prim::IntLn => {
            not_wide_or_idx(&ins[0], "a transcendental's input")?;
            want_shape(ins[0].shape.clone())
        }
        Prim::Add | Prim::Sub | Prim::Mul | Prim::Div { .. } => {
            let s = broadcast_shapes(&ins[0].shape, &ins[1].shape).ok_or("operands do not broadcast")?;
            want_shape(s)
        }
        Prim::Compare { .. } => {
            if out.dtype != DType::I8 {
                return Err("a comparison is i8".into());
            }
            let s = broadcast_shapes(&ins[0].shape, &ins[1].shape).ok_or("operands do not broadcast")?;
            want_shape(s)
        }
        Prim::Select => {
            let s = broadcast_shapes(&ins[0].shape, &ins[1].shape).and_then(|s| broadcast_shapes(&s, &ins[2].shape));
            want_shape(s.ok_or("operands do not broadcast")?)
        }
        Prim::MatMul => {
            let (x, y) = (&ins[0], &ins[1]);
            for t in [x, y] {
                if t.rank() < 2 {
                    return Err("matmul operands have rank ≥ 2".into());
                }
                if matches!(t.dtype, DType::I128 | DType::Idx) {
                    return Err("matmul operands are i8..i64".into());
                }
            }
            if out.dtype == DType::Idx {
                return Err("a matmul does not accumulate into idx".into());
            }
            let (xr, yr) = (x.rank(), y.rank());
            if x.shape[xr - 1] != y.shape[yr - 2] {
                return Err("contraction dimensions differ".into());
            }
            let mut s = broadcast_shapes(&x.shape[..xr - 2], &y.shape[..yr - 2]).ok_or("batch dimensions do not broadcast")?;
            s.push(x.shape[xr - 2]);
            s.push(y.shape[yr - 1]);
            want_shape(s)
        }
        Prim::ReduceSum { axis } | Prim::ReduceMax { axis } => {
            if matches!(prim, Prim::ReduceMax { .. }) {
                same_dtype(&ins[0])?;
            }
            let a = *axis as usize;
            if a >= ins[0].rank() {
                return Err("axis out of range".into());
            }
            let mut s = ins[0].shape.clone();
            s[a] = Dim::Fixed(1);
            want_shape(s)
        }
        Prim::Clamp { lo, hi } => {
            if lo > hi {
                return Err("lo > hi".into());
            }
            if !out.dtype.contains(*lo as i128) || !out.dtype.contains(*hi as i128) {
                return Err(format!("[{lo}, {hi}] is not inside {}", out.dtype.name()));
            }
            want_shape(ins[0].shape.clone())
        }
        Prim::TopK { axis, k } => {
            if out.dtype != DType::Idx {
                return Err("TopK returns idx".into());
            }
            let a = *axis as usize;
            if a >= ins[0].rank() {
                return Err("axis out of range".into());
            }
            let Dim::Fixed(n) = ins[0].shape[a] else { return Err("TopK along H".into()) };
            if *k == 0 || *k > n {
                return Err(format!("k = {k} outside [1, {n}]"));
            }
            let mut s = ins[0].shape.clone();
            s[a] = Dim::Fixed(*k);
            want_shape(s)
        }
        Prim::StateWrite { state } => {
            let st = states.get(*state as usize).ok_or("no such state")?;
            if !matches!(st.kind, StateKind::Fixed { .. }) {
                return Err("StateWrite targets a Fixed state".into());
            }
            if out.dtype != st.dtype || out.shape != fixed_shape(&st.shape) || ins[0].shape != out.shape {
                return Err("StateWrite's input and output have the state's shape, its output the state's dtype".into());
            }
            Ok(())
        }
        Prim::HistAppend { state } => {
            let st = states.get(*state as usize).ok_or("no such state")?;
            if !matches!(st.kind, StateKind::Hist { .. }) {
                return Err("HistAppend targets a Hist state".into());
            }
            let row = TensorType::new(st.dtype, fixed_shape(&st.shape));
            if ins[0] != row {
                return Err("the appended row must be exactly the state's row type".into());
            }
            let mut s = vec![Dim::H];
            s.extend(fixed_shape(&st.shape));
            if out.dtype != st.dtype {
                return Err("dtype".into());
            }
            want_shape(s)
        }
    }
}

/// Validate structure and types; return what the interpreter needs.
pub fn validate(p: &TirProgramV1) -> TirResult<ProgramInfo> {
    if p.version != TIR_PROGRAM_VERSION_V1 {
        return nf(format!("version {} is not {TIR_PROGRAM_VERSION_V1}", p.version));
    }
    if p.history_bound != HISTORY_BOUND_V1_SMALL && p.history_bound != HISTORY_BOUND_V1_HELD {
        return nf("history_bound must be 2^18 or 2^21");
    }
    if p.token_bound == 0 {
        return nf("token_bound must be at least 1");
    }
    if p.blocks.is_empty() || p.blocks.len() > MAX_BLOCKS {
        return nf(format!("1..={MAX_BLOCKS} blocks"));
    }
    if p.schedule.layers.len() > MAX_LAYERS {
        return nf(format!("at most {MAX_LAYERS} layers"));
    }
    // ---- declarations ----------------------------------------------------------------------
    if p.params.len() > MAX_PARAMS {
        return nf(format!("at most {MAX_PARAMS} params"));
    }
    let mut names = BTreeSet::new();
    for (j, d) in p.params.iter().enumerate() {
        check_name("param", &d.name)?;
        if !names.insert(d.name.as_str()) {
            return nf(format!("param name {} is declared twice", d.name));
        }
        if d.dtype == DType::I128 {
            return nf(format!("param {j} may not be i128"));
        }
        check_static_shape(&format!("param {j}"), &d.shape, MAX_RANK)?;
    }
    let mut const_bytes = 0usize;
    let mut seen_consts = BTreeSet::new();
    for (j, c) in p.consts.iter().enumerate() {
        let n = check_static_shape(&format!("const {j}"), &c.shape, MAX_RANK)?;
        if c.data.len() as u64 != n * c.dtype.width() as u64 {
            return nf(format!("const {j}: {} bytes for {n} elements of {}", c.data.len(), c.dtype.name()));
        }
        const_bytes += c.data.len();
        if !seen_consts.insert((c.dtype, c.shape.clone(), c.data.clone())) {
            return nf(format!("const {j} duplicates an earlier const"));
        }
    }
    if const_bytes > MAX_CONST_BYTES {
        return nf(format!("{const_bytes} const bytes exceed {MAX_CONST_BYTES}"));
    }
    if p.states.len() > MAX_STATES {
        return nf(format!("at most {MAX_STATES} states"));
    }
    let mut snames = BTreeSet::new();
    for (j, s) in p.states.iter().enumerate() {
        check_name("state", &s.name)?;
        if !snames.insert(s.name.as_str()) {
            return nf(format!("state name {} is declared twice", s.name));
        }
        if !matches!(s.dtype, DType::I8 | DType::I16 | DType::I32) {
            return nf(format!("state {j} must be i8, i16 or i32"));
        }
        match s.kind {
            StateKind::Fixed { lo, hi } => {
                check_static_shape(&format!("state {j}"), &s.shape, MAX_RANK)?;
                if lo > hi || !s.dtype.contains(lo as i128) || !s.dtype.contains(hi as i128) || lo > 0 || hi < 0 {
                    return nf(format!("state {j}: range [{lo}, {hi}] must be inside {} and contain 0", s.dtype.name()));
                }
            }
            StateKind::Hist { window } => {
                check_static_shape(&format!("state {j}"), &s.shape, MAX_RANK - 1)?;
                if window == 0 || window > p.history_bound {
                    return nf(format!("state {j}: window {window} outside [1, history_bound]"));
                }
            }
        }
    }
    if p.states.iter().filter(|s| s.per_layer).count() > MAX_STATES_PER_LAYER {
        return nf(format!("at most {MAX_STATES_PER_LAYER} per-layer states"));
    }
    // ---- schedule and roles ----------------------------------------------------------------
    let nb = p.blocks.len();
    let (pre, post) = (p.schedule.pre as usize, p.schedule.post as usize);
    if pre >= nb || post >= nb || p.schedule.layers.iter().any(|b| *b as usize >= nb) {
        return nf("the schedule names a block that does not exist");
    }
    if pre == post || p.schedule.layers.iter().any(|b| *b as usize == pre || *b as usize == post) {
        return nf("pre, post and layer blocks are distinct");
    }
    let mut info: Vec<BlockInfo> =
        (0..nb).map(|b| BlockInfo { window: None, is_pre: b == pre, is_post: b == post, is_layer: false }).collect();
    for b in &p.schedule.layers {
        info[*b as usize].is_layer = true;
    }
    if info.iter().any(|i| !i.is_pre && !i.is_post && !i.is_layer) {
        return nf("every block is used by the schedule");
    }
    if !p.blocks[pre].carry_in.is_empty() {
        return nf("the pre block has no carry-in");
    }
    if !p.blocks[post].carry_out.is_empty() {
        return nf("the post block has no carry-out; it ends in the logits");
    }
    for (bi, b) in p.blocks.iter().enumerate() {
        check_name("block", &b.name)?;
        if b.nodes.is_empty() || b.nodes.len() > MAX_NODES_PER_BLOCK {
            return nf(format!("block {bi}: 1..={MAX_NODES_PER_BLOCK} nodes"));
        }
        if b.carry_in.len() > MAX_CARRY || b.carry_out.len() > MAX_CARRY {
            return nf(format!("block {bi}: at most {MAX_CARRY} carried tensors"));
        }
        if b.carry_out.iter().any(|n| *n as usize >= b.nodes.len()) {
            return nf(format!("block {bi}: carry_out names a node that does not exist"));
        }
    }
    let carry: Vec<TensorType> = p.blocks[pre].carry_out.iter().map(|n| p.blocks[pre].nodes[*n as usize].out.clone()).collect();
    for t in &carry {
        if !t.dtype.committable() || t.has_h() {
            return nf("carried tensors are committable and have no H");
        }
    }
    for (bi, b) in p.blocks.iter().enumerate() {
        if (info[bi].is_layer || info[bi].is_post) && b.carry_in != carry {
            return nf(format!("block {bi}: carry-in is not the carry signature"));
        }
        if info[bi].is_layer {
            let out: Vec<TensorType> = b.carry_out.iter().map(|n| b.nodes[*n as usize].out.clone()).collect();
            if out != carry {
                return nf(format!("block {bi}: carry-out is not the carry signature"));
            }
        }
    }
    let post_block = &p.blocks[post];
    let Some(logits) = post_block.nodes.get(p.logits as usize) else {
        return nf("the logits node does not exist");
    };
    if !logits.commit || !logits.out.dtype.committable() || logits.out.has_h() {
        return nf("the logits node is a committed, committable tensor without H");
    }
    // ---- per block: windows, refs, types, effects, liveness ---------------------------------
    for (bi, b) in p.blocks.iter().enumerate() {
        let layer_block = info[bi].is_layer;
        // The window shared by every history this block appends to.
        let mut window: Option<u32> = None;
        for n in &b.nodes {
            if let Prim::HistAppend { state } = n.prim
                && let Some(StateDecl { kind: StateKind::Hist { window: w }, .. }) = p.states.get(state as usize)
            {
                match window {
                    None => window = Some(*w),
                    Some(x) if x == *w => {}
                    Some(_) => return nf(format!("block {bi}: histories with different windows")),
                }
            }
        }
        info[bi].window = window;
        let mut written = BTreeSet::new();
        let mut appended = BTreeSet::new();
        for (ni, n) in b.nodes.iter().enumerate() {
            let (lo, hi) = n.prim.arity();
            if n.inputs.len() < lo || n.inputs.len() > hi || n.inputs.len() > MAX_NODE_INPUTS {
                return shape_err(bi, ni, &n.prim, format!("{} inputs, want {lo}..={hi}", n.inputs.len()));
            }
            check_tensor_type(&n.out, window).or_else(|m| shape_err(bi, ni, &n.prim, m))?;
            let mut ins = Vec::with_capacity(n.inputs.len());
            for r in &n.inputs {
                let t = match *r {
                    Ref::Node(j) => {
                        if j as usize >= ni {
                            return nf(format!("block {bi} node {ni}: refs point strictly backward"));
                        }
                        b.nodes[j as usize].out.clone()
                    }
                    Ref::CarryIn(k) => {
                        b.carry_in.get(k as usize).cloned().ok_or(()).or_else(|_| nf(format!("block {bi}: no carry-in {k}")))?
                    }
                    Ref::Param(j) => {
                        let d = p.params.get(j as usize).ok_or(()).or_else(|_| nf(format!("block {bi}: no param {j}")))?;
                        if d.per_layer && !layer_block {
                            return nf(format!("block {bi}: per-layer param {j} outside a layer block"));
                        }
                        TensorType::new(d.dtype, fixed_shape(&d.shape))
                    }
                    Ref::Const(j) => {
                        let c = p.consts.get(j as usize).ok_or(()).or_else(|_| nf(format!("block {bi}: no const {j}")))?;
                        TensorType::new(c.dtype, fixed_shape(&c.shape))
                    }
                    Ref::State(j) => {
                        let s = p.states.get(j as usize).ok_or(()).or_else(|_| nf(format!("block {bi}: no state {j}")))?;
                        if !matches!(s.kind, StateKind::Fixed { .. }) {
                            return nf(format!("block {bi}: a Hist state is read only through its HistAppend"));
                        }
                        if s.per_layer != layer_block {
                            return nf(format!("block {bi}: state {j}'s per_layer must match the block's role"));
                        }
                        TensorType::new(s.dtype, fixed_shape(&s.shape))
                    }
                    Ref::Input(j) => {
                        if j > INPUT_POS {
                            return nf(format!("block {bi}: no input {j}"));
                        }
                        TensorType::scalar(DType::Idx)
                    }
                };
                ins.push(t);
            }
            check_node_type(&n.prim, &ins, &n.out, &p.states).or_else(|m| shape_err(bi, ni, &n.prim, m))?;
            if n.commit && !n.out.dtype.committable() {
                return nf(format!("block {bi} node {ni}: a commit point is i8, i16, i32 or idx (PALW-TIR-5)"));
            }
            match n.prim {
                Prim::TopK { .. } if !n.commit => {
                    return nf(format!("block {bi} node {ni}: every TopK is a commit point (PALW-TIR-11)"));
                }
                Prim::StateWrite { state } => {
                    if p.states[state as usize].per_layer != layer_block {
                        return nf(format!("block {bi}: state {state}'s per_layer must match the block's role"));
                    }
                    if !written.insert(state) {
                        return nf(format!("block {bi}: state {state} written twice"));
                    }
                }
                Prim::HistAppend { state } => {
                    if p.states[state as usize].per_layer != layer_block {
                        return nf(format!("block {bi}: state {state}'s per_layer must match the block's role"));
                    }
                    if !appended.insert(state) {
                        return nf(format!("block {bi}: history {state} appended twice"));
                    }
                    let committed = match n.inputs[0] {
                        Ref::Node(j) => b.nodes[j as usize].commit,
                        Ref::CarryIn(_) => true,
                        _ => false,
                    };
                    if !committed {
                        return nf(format!("block {bi} node {ni}: the appended row is a commit point (PALW-TIR-14)"));
                    }
                }
                _ => {}
            }
        }
        for n in &b.carry_out {
            if !b.nodes[*n as usize].commit {
                return nf(format!("block {bi}: carry-out node {n} is a commit point (PALW-TIR-14)"));
            }
        }
        // Liveness: every node reaches a root (carry-out, logits, an effect, a commit point).
        let mut live = vec![false; b.nodes.len()];
        let mut stack: Vec<usize> = Vec::new();
        for (ni, n) in b.nodes.iter().enumerate() {
            let root = n.commit || matches!(n.prim, Prim::StateWrite { .. } | Prim::HistAppend { .. });
            if root {
                stack.push(ni);
            }
        }
        stack.extend(b.carry_out.iter().map(|n| *n as usize));
        if info[bi].is_post {
            stack.push(p.logits as usize);
        }
        while let Some(i) = stack.pop() {
            if std::mem::replace(&mut live[i], true) {
                continue;
            }
            for r in &b.nodes[i].inputs {
                if let Ref::Node(j) = r {
                    stack.push(*j as usize);
                }
            }
        }
        if let Some(dead) = live.iter().position(|l| !l) {
            return nf(format!("block {bi} node {dead} is dead"));
        }
    }
    // Every declared param, const and state is used somewhere (no dead declarations).
    let mut used: BTreeMap<&str, BTreeSet<u16>> = BTreeMap::new();
    for b in &p.blocks {
        for n in &b.nodes {
            for r in &n.inputs {
                match r {
                    Ref::Param(j) => used.entry("param").or_default().insert(*j),
                    Ref::Const(j) => used.entry("const").or_default().insert(*j),
                    Ref::State(j) => used.entry("state").or_default().insert(*j),
                    _ => false,
                };
            }
            if let Prim::StateWrite { state } | Prim::HistAppend { state } = n.prim {
                used.entry("state").or_default().insert(state);
            }
        }
    }
    for (what, count) in [("param", p.params.len()), ("const", p.consts.len()), ("state", p.states.len())] {
        let set = used.get(what).cloned().unwrap_or_default();
        if set.len() != count {
            return nf(format!("a declared {what} is never used"));
        }
    }
    Ok(ProgramInfo { blocks: info, carry })
}
