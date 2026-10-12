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

/// One violated rule: its name (`"NF-7"`) and the class §9.3 assigns to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    pub rule: &'static str,
    pub class: Class,
    pub reason: String,
}

/// Collects violations; a rule whose preconditions do not hold (a ref that does not exist, a
/// schedule that names no block) is not evaluated, so every entry is a rule the program breaks.
struct Collector {
    v: Vec<Violation>,
}

impl Collector {
    fn fail(&mut self, rule: &'static str, reason: impl Into<String>) {
        let class = if rule == "NF-16" { Class::Shape } else { Class::NormalForm };
        self.v.push(Violation { rule, class, reason: reason.into() });
    }
    fn when(&mut self, bad: bool, rule: &'static str, reason: impl Into<String>) {
        if bad {
            self.fail(rule, reason);
        }
    }
}

/// Every rule of NF-1..NF-22 the program breaks (04b §5), each with its §9.3 class: `Shape` for
/// NF-16, `NormalForm` for every other rule. Empty iff the program is in normal form.
pub fn violations(p: &Program) -> Vec<Violation> {
    let mut c = Collector { v: Vec::new() };
    let nb = p.blocks.len();
    // NF-1
    c.when(p.version != 1, "NF-1", "version is not 1");
    c.when(p.prim_set_id != prim_set_id_v1(), "NF-1", "prim_set_id is not PRIM_SET_ID_V1");
    c.when(!HISTORY_BOUNDS.contains(&p.history_bound), "NF-1", "history_bound is not 2^18 or 2^21");
    c.when(p.token_bound < 1, "NF-1", "token_bound is 0");
    // NF-2
    c.when(!(2..=16).contains(&nb), "NF-2", "block count outside 2..=16");
    c.when(p.schedule.layers.len() > 1024, "NF-2", "more than 1024 layers");
    c.when(p.params.len() > 4096, "NF-2", "more than 4096 params");
    c.when(p.states.len() > 64, "NF-2", "more than 64 states");
    c.when(p.states.iter().filter(|s| s.per_layer).count() > 16, "NF-2", "more than 16 per-layer states");
    // NF-3
    let (pre, post) = (p.schedule.pre as usize, p.schedule.post as usize);
    let pre_ok = pre < nb;
    let post_ok = post < nb;
    let layers_ok = p.schedule.layers.iter().all(|&l| (l as usize) < nb);
    c.when(!(pre_ok && post_ok && layers_ok), "NF-3", "a schedule entry names no block");
    c.when(pre == post, "NF-3", "pre = post");
    c.when(p.schedule.layers.iter().any(|&l| l as usize == pre || l as usize == post), "NF-3", "pre or post in layers");
    for b in 0..nb {
        c.when(
            b != pre && b != post && !p.schedule.layers.iter().any(|&l| l as usize == b),
            "NF-3",
            format!("block {b} is not scheduled"),
        );
    }
    let schedule_ok = !c.v.iter().any(|x| x.rule == "NF-3");
    // NF-12
    for (b, block) in p.blocks.iter().enumerate() {
        c.when(block.nodes.is_empty() || block.nodes.len() > 512, "NF-12", format!("block {b} has {} nodes", block.nodes.len()));
    }
    // NF-4
    if pre_ok {
        c.when(!p.blocks[pre].carry_in.is_empty(), "NF-4", "pre has a carry-in");
    }
    if post_ok {
        c.when(!p.blocks[post].carry_out.is_empty(), "NF-4", "post has a carry-out");
    }
    let mut carry_ok = vec![true; nb];
    for (b, block) in p.blocks.iter().enumerate() {
        c.when(block.carry_in.len() > 8 || block.carry_out.len() > 8, "NF-4", format!("block {b} has more than 8 carries"));
        if block.carry_out.iter().any(|&x| x as usize >= block.nodes.len()) {
            carry_ok[b] = false;
            c.fail("NF-4", format!("block {b} carries out a node that does not exist"));
        }
    }
    // NF-7
    for d in &p.params {
        c.when(d.name.is_empty() || d.name.len() > 128, "NF-7", "param name length outside 1..=128");
        c.when(d.dtype == DType::I128, "NF-7", "i128 param");
    }
    for s in &p.states {
        c.when(s.name.is_empty() || s.name.len() > 128, "NF-7", "state name length outside 1..=128");
    }
    for b in &p.blocks {
        c.when(b.name.is_empty() || b.name.len() > 128, "NF-7", "block name length outside 1..=128");
    }
    for i in 0..p.params.len() {
        c.when((0..i).any(|j| p.params[i].name == p.params[j].name), "NF-7", "duplicate param name");
    }
    for i in 0..p.states.len() {
        c.when((0..i).any(|j| p.states[i].name == p.states[j].name), "NF-7", "duplicate state name");
    }
    // NF-8
    for d in &p.params {
        if let Err(e) = check_decl_shape("param", &d.shape, MAX_RANK, 1u128 << 40) {
            c.fail("NF-8", e.reason);
        }
    }
    for k in &p.consts {
        if let Err(e) = check_decl_shape("const", &k.shape, MAX_RANK, 1u128 << 28) {
            c.fail("NF-8", e.reason);
        }
    }
    for s in &p.states {
        let max_rank = match s.kind {
            StateKind::Fixed { .. } => MAX_RANK,
            StateKind::Hist { .. } => MAX_RANK - 1,
        };
        if let Err(e) = check_decl_shape("state", &s.shape, max_rank, 1u128 << 28) {
            c.fail("NF-8", e.reason);
        }
    }
    // NF-9
    let mut total: u128 = 0;
    for k in &p.consts {
        c.when(
            k.data.len() as u128 != elements(&k.shape) * k.dtype.width() as u128,
            "NF-9",
            "const data length is not elements × width",
        );
        total += k.data.len() as u128;
    }
    c.when(total > 65_536, "NF-9", "consts above 65,536 bytes");
    for i in 0..p.consts.len() {
        c.when((0..i).any(|j| p.consts[i] == p.consts[j]), "NF-9", "duplicate const");
    }
    // NF-10
    for s in &p.states {
        c.when(!matches!(s.dtype, DType::I8 | DType::I16 | DType::I32), "NF-10", "state dtype is not i8/i16/i32");
        match s.kind {
            StateKind::Fixed { lo, hi } => c.when(
                !(lo <= 0 && 0 <= hi && s.dtype.contains(lo as i128) && s.dtype.contains(hi as i128)),
                "NF-10",
                "Fixed range does not contain 0 or leaves the dtype",
            ),
            StateKind::Hist { window } => c.when(window < 1 || window > p.history_bound, "NF-10", "window outside [1, history_bound]"),
        }
    }
    // NF-13
    let mut windows: Vec<Option<Option<u32>>> = Vec::with_capacity(nb);
    for b in 0..nb {
        match block_window(p, b) {
            Ok(w) => windows.push(Some(w)),
            Err(e) => {
                c.fail("NF-13", e.reason);
                windows.push(None);
            }
        }
    }
    // NF-14 (per node: arity and ref existence), remembered for the rules that need them.
    let mut node_ok: Vec<Vec<bool>> = Vec::with_capacity(nb);
    for (b, block) in p.blocks.iter().enumerate() {
        let mut oks = Vec::with_capacity(block.nodes.len());
        for (i, n) in block.nodes.iter().enumerate() {
            let (lo, hi) = n.prim.arity();
            let mut ok = true;
            if n.inputs.len() < lo || n.inputs.len() > hi || n.inputs.len() > 8 {
                c.fail("NF-14", format!("block {b} node {i} has {} inputs", n.inputs.len()));
                ok = false;
            }
            for r in &n.inputs {
                if ref_type(p, b, i, r).is_none() {
                    c.fail("NF-14", format!("block {b} node {i} refers to {r:?}, which does not exist"));
                    ok = false;
                }
            }
            oks.push(ok);
        }
        node_ok.push(oks);
    }
    // NF-15 (roles are defined only by a valid schedule)
    if schedule_ok {
        for (b, block) in p.blocks.iter().enumerate() {
            let layer = role_of(p, b) == Role::Layer;
            for (i, n) in block.nodes.iter().enumerate() {
                for r in &n.inputs {
                    if ref_type(p, b, i, r).is_none() {
                        continue;
                    }
                    match *r {
                        Ref::Param(j) => {
                            c.when(p.params[j as usize].per_layer && !layer, "NF-15", "per-layer param outside a layer block")
                        }
                        Ref::State(j) => {
                            c.when(p.states[j as usize].per_layer != layer, "NF-15", "state read from a block of the other role")
                        }
                        _ => {}
                    }
                }
                if let Prim::StateWrite { state } | Prim::HistAppend { state } = n.prim
                    && let Some(s) = p.states.get(state as usize)
                {
                    c.when(s.per_layer != layer, "NF-15", "state written from a block of the other role");
                }
            }
        }
    }
    // NF-16 (legality with the block's window, and the type rule, where NF-14 holds)
    for (b, block) in p.blocks.iter().enumerate() {
        for (i, n) in block.nodes.iter().enumerate() {
            if let Some(w) = windows[b]
                && let Err(e) = n.out.check_legal(w)
            {
                c.fail("NF-16", e.reason);
            }
            if node_ok[b][i] {
                let ins: Vec<TensorType> = n.inputs.iter().filter_map(|r| ref_type(p, b, i, r)).collect();
                if let Err(e) = check_type(&n.prim, &ins, &n.out, &p.states) {
                    c.fail("NF-16", e.reason);
                }
            }
        }
    }
    // NF-5
    if pre_ok && carry_ok[pre] {
        let sig: Vec<TensorType> = p.blocks[pre].carry_out.iter().map(|&x| p.blocks[pre].nodes[x as usize].out.clone()).collect();
        c.when(sig.iter().any(|t| !t.dtype.committable() || t.h_count() != 0), "NF-5", "carry signature has a wide dtype or H");
        if schedule_ok {
            for (b, block) in p.blocks.iter().enumerate() {
                if role_of(p, b) == Role::Layer {
                    c.when(block.carry_in != sig, "NF-5", format!("layer block {b}'s carry-in is not the signature"));
                    if carry_ok[b] {
                        let outs: Vec<TensorType> = block.carry_out.iter().map(|&x| block.nodes[x as usize].out.clone()).collect();
                        c.when(outs != sig, "NF-5", format!("layer block {b}'s carry-out is not the signature"));
                    }
                }
            }
        }
        if post_ok {
            c.when(p.blocks[post].carry_in != sig, "NF-5", "post's carry-in is not the signature");
        }
    }
    // NF-6
    if post_ok {
        match p.blocks[post].nodes.get(p.logits as usize) {
            Some(n) if n.commit && n.out.dtype.committable() && n.out.h_count() == 0 => {}
            _ => c.fail("NF-6", "logits is not a committed, committable, H-free node of post"),
        }
    }
    // NF-17 … NF-21
    for (b, block) in p.blocks.iter().enumerate() {
        let mut writes: BTreeMap<u16, u32> = BTreeMap::new();
        let mut appends: BTreeMap<u16, u32> = BTreeMap::new();
        for (i, n) in block.nodes.iter().enumerate() {
            c.when(n.commit && !n.out.dtype.committable(), "NF-17", format!("block {b} node {i} commits a wide dtype"));
            match n.prim {
                Prim::TopK { .. } => c.when(!n.commit, "NF-18", "TopK not committed"),
                Prim::StateWrite { state } => *writes.entry(state).or_default() += 1,
                Prim::HistAppend { state } => {
                    *appends.entry(state).or_default() += 1;
                    let ok = match n.inputs.first() {
                        Some(Ref::Node(k)) if (*k as usize) < i => block.nodes[*k as usize].commit,
                        Some(Ref::Node(_)) => true, // a forward ref: NF-14's, not NF-20's
                        Some(Ref::CarryIn(_)) => true,
                        Some(_) => false,
                        None => true, // arity: NF-14's
                    };
                    c.when(!ok, "NF-20", "HistAppend input is neither a committed node nor a carry-in");
                }
                _ => {}
            }
        }
        c.when(
            writes.values().any(|&k| k > 1) || appends.values().any(|&k| k > 1),
            "NF-19",
            format!("block {b} writes or appends a state twice"),
        );
        c.when(b == post && (!writes.is_empty() || !appends.is_empty()), "NF-19", "post contains a StateWrite or a HistAppend");
        if carry_ok[b] {
            c.when(
                block.carry_out.iter().any(|&x| !block.nodes[x as usize].commit),
                "NF-21",
                format!("block {b} carries out an uncommitted node"),
            );
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
        for &x in &block.carry_out {
            if (x as usize) < n {
                live[x as usize] = true;
            }
        }
        if b == post && (p.logits as usize) < n {
            live[p.logits as usize] = true;
        }
        // Reachability through every Node ref that names an existing node (a forward ref breaks
        // NF-14, not NF-22), by a worklist rather than one reverse pass.
        let mut work: Vec<usize> = (0..n).filter(|&i| live[i]).collect();
        while let Some(i) = work.pop() {
            for r in &block.nodes[i].inputs {
                if let Ref::Node(k) = *r
                    && (k as usize) < n
                    && !live[k as usize]
                {
                    live[k as usize] = true;
                    work.push(k as usize);
                }
            }
        }
        if let Some(i) = live.iter().position(|l| !l) {
            c.fail("NF-22", format!("block {b} node {i} is dead"));
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
                    Ref::Param(j) if (j as usize) < p.params.len() => used_param[j as usize] = true,
                    Ref::Const(j) if (j as usize) < p.consts.len() => used_const[j as usize] = true,
                    Ref::State(j) if (j as usize) < p.states.len() => used_state[j as usize] = true,
                    _ => {}
                }
            }
            if let Prim::StateWrite { state } | Prim::HistAppend { state } = n.prim
                && (state as usize) < p.states.len()
            {
                used_state[state as usize] = true;
            }
        }
    }
    c.when(used_param.iter().chain(used_const.iter()).chain(used_state.iter()).any(|u| !u), "NF-11", "an unused declaration");
    c.v
}

/// NF-1..NF-22: the first violation in this crate's order, or `Ok`.
pub fn check(p: &Program) -> Res<()> {
    match violations(p).into_iter().next() {
        None => Ok(()),
        Some(v) => err(v.class, format!("{}: {}", v.rule, v.reason)),
    }
}
