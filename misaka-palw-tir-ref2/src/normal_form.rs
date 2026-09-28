//! Normal form (04b §5, NF-1..NF-22). Violations are `NormalForm` refusals; a node whose declared
//! type is not its rule's type is a `Shape` refusal (NF-16), as are illegal tensor types.

use std::collections::BTreeMap;

use crate::error::{Class, Res, err};
use crate::program::*;
use crate::types::{DType, MAX_DIM, MAX_RANK, TensorType};
use crate::typing::check_type;

const HISTORY_BOUNDS: [u32; 2] = [1 << 18, 1 << 21];

fn nf<T>(why: impl Into<String>) -> Res<T> {
    err(Class::NormalForm, why)
}

/// The role of a block in the schedule (§3.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Pre,
    Layer,
    Post,
}

pub fn role_of(p: &Program, b: usize) -> Role {
    if b == p.schedule.pre as usize {
        Role::Pre
    } else if b == p.schedule.post as usize {
        Role::Post
    } else {
        Role::Layer
    }
}

/// The type of a `Ref` inside block `b` (§3.2). `None` if the ref does not exist.
pub fn ref_type(p: &Program, b: usize, node_index: usize, r: &Ref) -> Option<TensorType> {
    let block = p.blocks.get(b)?;
    match *r {
        Ref::Node(i) => {
            if (i as usize) < node_index {
                block.nodes.get(i as usize).map(|n| n.out.clone())
            } else {
                None
            }
        }
        Ref::CarryIn(k) => block.carry_in.get(k as usize).cloned(),
        Ref::Param(j) => p.params.get(j as usize).map(|d| TensorType::fixed(d.dtype, &d.shape)),
        Ref::Const(j) => p.consts.get(j as usize).map(|d| TensorType::fixed(d.dtype, &d.shape)),
        Ref::State(j) => match p.states.get(j as usize) {
            Some(s) if matches!(s.kind, StateKind::Fixed { .. }) => Some(TensorType::fixed(s.dtype, &s.shape)),
            _ => None,
        },
        Ref::Input(j) => {
            if j <= 1 {
                Some(TensorType { dtype: DType::Idx, shape: vec![] })
            } else {
                None
            }
        }
    }
}

/// The window of block `b` (§2.2): the common window of the Hist states its `HistAppend` nodes
/// target, `None` if it appends to none. Targets that are not Hist states are ignored here (they
/// fail the type rule).
pub fn block_window(p: &Program, b: usize) -> Res<Option<u32>> {
    let mut w: Option<u32> = None;
    for n in &p.blocks[b].nodes {
        if let Prim::HistAppend { state } = n.prim
            && let Some(StateDecl { kind: StateKind::Hist { window }, .. }) = p.states.get(state as usize)
        {
            match w {
                None => w = Some(*window),
                Some(x) if x == *window => {}
                Some(_) => return nf("NF-13: a block appends to histories of different windows"),
            }
        }
    }
    Ok(w)
}

fn check_decl_shape(what: &str, shape: &[u32], max_rank: usize, max_elements: u128) -> Res<()> {
    if shape.len() > max_rank {
        return nf(format!("NF-8: {what} rank {} above {max_rank}", shape.len()));
    }
    let mut count: u128 = 1;
    for &d in shape {
        if d == 0 || d as u64 > MAX_DIM {
            return nf(format!("NF-8: {what} dimension {d} outside [1, 2^24]"));
        }
        count *= d as u128;
    }
    if count > max_elements {
        return nf(format!("NF-8: {what} has {count} elements"));
    }
    Ok(())
}

fn elements(shape: &[u32]) -> u128 {
    shape.iter().map(|&d| d as u128).product()
}

/// All of NF-1..NF-22.
pub fn check(p: &Program) -> Res<()> {
    // NF-1
    if p.version != 1 {
        return nf("NF-1: version is not 1");
    }
    if !HISTORY_BOUNDS.contains(&p.history_bound) {
        return nf("NF-1: history_bound is not 2^18 or 2^21");
    }
    if p.token_bound < 1 {
        return nf("NF-1: token_bound is 0");
    }
    // NF-2
    if p.blocks.is_empty() || p.blocks.len() > 16 {
        return nf("NF-2: block count outside 1..=16");
    }
    if p.schedule.layers.len() > 1024 {
        return nf("NF-2: more than 1024 layers");
    }
    if p.params.len() > 4096 {
        return nf("NF-2: more than 4096 params");
    }
    if p.states.len() > 64 {
        return nf("NF-2: more than 64 states");
    }
    if p.states.iter().filter(|s| s.per_layer).count() > 16 {
        return nf("NF-2: more than 16 per-layer states");
    }
    // NF-3
    let nb = p.blocks.len();
    let (pre, post) = (p.schedule.pre as usize, p.schedule.post as usize);
    if pre >= nb || post >= nb || p.schedule.layers.iter().any(|&l| l as usize >= nb) {
        return nf("NF-3: a schedule entry names no block");
    }
    if pre == post {
        return nf("NF-3: pre = post");
    }
    if p.schedule.layers.iter().any(|&l| l as usize == pre || l as usize == post) {
        return nf("NF-3: pre or post in layers");
    }
    for b in 0..nb {
        if b != pre && b != post && !p.schedule.layers.iter().any(|&l| l as usize == b) {
            return nf(format!("NF-3: block {b} is not scheduled"));
        }
    }
    // NF-12 (before anything indexes nodes)
    for (b, block) in p.blocks.iter().enumerate() {
        if block.nodes.is_empty() || block.nodes.len() > 512 {
            return nf(format!("NF-12: block {b} has {} nodes", block.nodes.len()));
        }
    }
    // NF-4
    if !p.blocks[pre].carry_in.is_empty() {
        return nf("NF-4: pre has a carry-in");
    }
    if !p.blocks[post].carry_out.is_empty() {
        return nf("NF-4: post has a carry-out");
    }
    for (b, block) in p.blocks.iter().enumerate() {
        if block.carry_in.len() > 8 || block.carry_out.len() > 8 {
            return nf(format!("NF-4: block {b} has more than 8 carries"));
        }
        if block.carry_out.iter().any(|&c| c as usize >= block.nodes.len()) {
            return nf(format!("NF-4: block {b} carries out a node that does not exist"));
        }
    }
    // NF-7
    for d in &p.params {
        if d.name.is_empty() || d.name.len() > 128 {
            return nf("NF-7: param name length outside 1..=128");
        }
        if d.dtype == DType::I128 {
            return nf("NF-7: i128 param");
        }
    }
    for s in &p.states {
        if s.name.is_empty() || s.name.len() > 128 {
            return nf("NF-7: state name length outside 1..=128");
        }
    }
    for b in &p.blocks {
        if b.name.is_empty() || b.name.len() > 128 {
            return nf("NF-7: block name length outside 1..=128");
        }
    }
    for i in 0..p.params.len() {
        for j in 0..i {
            if p.params[i].name == p.params[j].name {
                return nf("NF-7: duplicate param name");
            }
        }
    }
    for i in 0..p.states.len() {
        for j in 0..i {
            if p.states[i].name == p.states[j].name {
                return nf("NF-7: duplicate state name");
            }
        }
    }
    // NF-8
    for d in &p.params {
        check_decl_shape("param", &d.shape, MAX_RANK, 1u128 << 40)?;
    }
    for c in &p.consts {
        check_decl_shape("const", &c.shape, MAX_RANK, 1u128 << 28)?;
    }
    for s in &p.states {
        let max_rank = match s.kind {
            StateKind::Fixed { .. } => MAX_RANK,
            StateKind::Hist { .. } => MAX_RANK - 1,
        };
        check_decl_shape("state", &s.shape, max_rank, 1u128 << 28)?;
    }
    // NF-9
    let mut total: u128 = 0;
    for c in &p.consts {
        if c.data.len() as u128 != elements(&c.shape) * c.dtype.width() as u128 {
            return nf("NF-9: const data length is not elements × width");
        }
        total += c.data.len() as u128;
    }
    if total > 65_536 {
        return nf("NF-9: consts above 65,536 bytes");
    }
    for i in 0..p.consts.len() {
        for j in 0..i {
            if p.consts[i] == p.consts[j] {
                return nf("NF-9: duplicate const");
            }
        }
    }
    // NF-10
    for s in &p.states {
        if !matches!(s.dtype, DType::I8 | DType::I16 | DType::I32) {
            return nf("NF-10: state dtype is not i8/i16/i32");
        }
        match s.kind {
            StateKind::Fixed { lo, hi } => {
                if !(lo <= 0 && 0 <= hi && s.dtype.contains(lo as i128) && s.dtype.contains(hi as i128)) {
                    return nf("NF-10: Fixed range does not contain 0 or leaves the dtype");
                }
            }
            StateKind::Hist { window } => {
                if window < 1 || window > p.history_bound {
                    return nf("NF-10: window outside [1, history_bound]");
                }
            }
        }
    }
    // NF-13
    let mut windows = Vec::with_capacity(nb);
    for b in 0..nb {
        windows.push(block_window(p, b)?);
    }
    // NF-14, NF-15
    for (b, block) in p.blocks.iter().enumerate() {
        let role = role_of(p, b);
        for (i, n) in block.nodes.iter().enumerate() {
            let (lo, hi) = n.prim.arity();
            if n.inputs.len() < lo || n.inputs.len() > hi || n.inputs.len() > 8 {
                return nf(format!("NF-14: block {b} node {i} has {} inputs", n.inputs.len()));
            }
            for r in &n.inputs {
                if ref_type(p, b, i, r).is_none() {
                    return nf(format!("NF-14: block {b} node {i} refers to {r:?}, which does not exist"));
                }
                match *r {
                    Ref::Param(j) if p.params[j as usize].per_layer && role != Role::Layer => {
                        return nf("NF-15: per-layer param outside a layer block");
                    }
                    Ref::State(j) if p.states[j as usize].per_layer != (role == Role::Layer) => {
                        return nf("NF-15: state read from a block of the other role");
                    }
                    _ => {}
                }
            }
            if let Prim::StateWrite { state } | Prim::HistAppend { state } = n.prim
                && let Some(s) = p.states.get(state as usize)
                && s.per_layer != (role == Role::Layer)
            {
                return nf("NF-15: state written from a block of the other role");
            }
        }
    }
    // NF-16
    for (b, block) in p.blocks.iter().enumerate() {
        for (i, n) in block.nodes.iter().enumerate() {
            n.out.check_legal(windows[b])?;
            let ins: Vec<TensorType> = n.inputs.iter().map(|r| ref_type(p, b, i, r).unwrap()).collect();
            check_type(&n.prim, &ins, &n.out, &p.states)?;
        }
    }
    // NF-5
    let sig: Vec<TensorType> = p.blocks[pre].carry_out.iter().map(|&c| p.blocks[pre].nodes[c as usize].out.clone()).collect();
    for t in &sig {
        if !t.dtype.committable() || t.h_count() != 0 {
            return nf("NF-5: carry signature has a wide dtype or H");
        }
    }
    for (b, block) in p.blocks.iter().enumerate() {
        if role_of(p, b) == Role::Layer {
            let outs: Vec<TensorType> = block.carry_out.iter().map(|&c| block.nodes[c as usize].out.clone()).collect();
            if block.carry_in != sig || outs != sig {
                return nf(format!("NF-5: layer block {b} carries are not the signature"));
            }
        }
    }
    if p.blocks[post].carry_in != sig {
        return nf("NF-5: post's carry-in is not the signature");
    }
    // NF-6
    match p.blocks[post].nodes.get(p.logits as usize) {
        Some(n) if n.commit && n.out.dtype.committable() && n.out.h_count() == 0 => {}
        _ => return nf("NF-6: logits is not a committed, committable, H-free node of post"),
    }
    // NF-17, NF-18, NF-19, NF-20, NF-21
    for (b, block) in p.blocks.iter().enumerate() {
        let mut writes: BTreeMap<u16, u32> = BTreeMap::new();
        let mut appends: BTreeMap<u16, u32> = BTreeMap::new();
        for (i, n) in block.nodes.iter().enumerate() {
            if n.commit && !n.out.dtype.committable() {
                return nf(format!("NF-17: block {b} node {i} commits a wide dtype"));
            }
            match n.prim {
                Prim::TopK { .. } if !n.commit => return nf("NF-18: TopK not committed"),
                Prim::StateWrite { state } => *writes.entry(state).or_default() += 1,
                Prim::HistAppend { state } => {
                    *appends.entry(state).or_default() += 1;
                    let ok = match n.inputs[0] {
                        Ref::Node(k) => block.nodes[k as usize].commit,
                        Ref::CarryIn(_) => true,
                        _ => false,
                    };
                    if !ok {
                        return nf("NF-20: HistAppend input is neither a committed node nor a carry-in");
                    }
                }
                _ => {}
            }
        }
        if writes.values().any(|&c| c > 1) || appends.values().any(|&c| c > 1) {
            return nf(format!("NF-19: block {b} writes or appends a state twice"));
        }
        if block.carry_out.iter().any(|&c| !block.nodes[c as usize].commit) {
            return nf(format!("NF-21: block {b} carries out an uncommitted node"));
        }
    }
    // NF-22
    for (b, block) in p.blocks.iter().enumerate() {
        let n = block.nodes.len();
        let mut live = vec![false; n];
        for (i, node) in block.nodes.iter().enumerate() {
            if node.commit || matches!(node.prim, Prim::StateWrite { .. } | Prim::HistAppend { .. }) {
                live[i] = true;
            }
        }
        for &c in &block.carry_out {
            live[c as usize] = true;
        }
        if b == post {
            live[p.logits as usize] = true;
        }
        for i in (0..n).rev() {
            if live[i] {
                for r in &block.nodes[i].inputs {
                    if let Ref::Node(k) = *r {
                        live[k as usize] = true;
                    }
                }
            }
        }
        if let Some(i) = live.iter().position(|l| !l) {
            return nf(format!("NF-22: block {b} node {i} is dead"));
        }
    }
    // NF-11
    let mut used_param = vec![false; p.params.len()];
    let mut used_const = vec![false; p.consts.len()];
    let mut used_state = vec![false; p.states.len()];
    for block in &p.blocks {
        for n in &block.nodes {
            for r in &n.inputs {
                match *r {
                    Ref::Param(j) => used_param[j as usize] = true,
                    Ref::Const(j) => used_const[j as usize] = true,
                    Ref::State(j) => used_state[j as usize] = true,
                    _ => {}
                }
            }
            if let Prim::StateWrite { state } | Prim::HistAppend { state } = n.prim {
                used_state[state as usize] = true;
            }
        }
    }
    if used_param.iter().chain(used_const.iter()).chain(used_state.iter()).any(|u| !u) {
        return nf("NF-11: an unused declaration");
    }
    Ok(())
}
