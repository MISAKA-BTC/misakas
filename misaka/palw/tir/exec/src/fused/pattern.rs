//! **Structural matching (RFC-0002 §7 F-5)** — a pattern matches by structural equality after
//! canonicalisation, over the canonical program a node decoded from the chain: never over a
//! compiler's internal form.
//!
//! A pattern is a `tir_library_v1` template, and its structure is the template's own emission: the
//! kernel emits the template into a scratch program whose one block takes the pattern's operands (its
//! *holes*) as carry-ins, and matching compares that emission with the program. The comparison has
//! two phases:
//!
//! 1. **The skeleton.** The template is emitted at probe types, and walked backwards from its output
//!    in lockstep with the program from a candidate node: every template node maps to one program
//!    node with the same primitive (tag only) and the same arity, every template carry-in to the
//!    program operand in its place (the hole's binding), every template const to a program const,
//!    every template state to a program state. The map must be one to one and consistent. This binds
//!    the holes; it proves nothing about attributes, types or values.
//! 2. **Exact equality.** The kernel derives its attributes from the bound holes' types (and, where a
//!    template reads a shape, from the matched nodes), the template is emitted again at the ACTUAL
//!    hole types, and every node must equal its program node: the whole primitive with its
//!    attributes, the output type, every operand under the map, and every const by value (dtype,
//!    shape and bytes; a const's index is not structure). Commit flags are not compared: a commit
//!    point is the program's, and a region never swallows one ([`super::Region`]).
//!
//! What is canonical here is the program: the template is re-emitted, never trusted, so a lowerer
//! that wrote the same nodes by hand matches, and one that wrote anything else does not.

use std::collections::BTreeMap;

use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::program::{Block, ConstDecl, HISTORY_BOUND_V1_SMALL, Node, StateDecl, TirProgramV1};
use misaka_palw_tir::{Ref, TensorType};

use super::{Bound, FusedKernelV1};

/// A template emitted into a scratch program.
pub(crate) struct Emitted {
    pub nodes: Vec<Node>,
    pub consts: Vec<ConstDecl>,
    pub output: u16,
}

/// Emit `kernel`'s template with holes of `hole_types` and the scratch states `states`, under
/// `bound`. `None` when the builder refuses the types (it panics on a shape that does not fit, and
/// a variant that cannot be emitted at these types is simply not a match).
pub(crate) fn emit(kernel: &dyn FusedKernelV1, hole_types: &[TensorType], states: &[StateDecl], bound: &Bound) -> Option<Emitted> {
    let run = || {
        let mut pb = ProgramBuilder::new(1, HISTORY_BOUND_V1_SMALL);
        pb.states.extend(states.iter().cloned());
        let mut b = pb.block("fused.template", hole_types.to_vec());
        let holes: Vec<Ref> = (0..hole_types.len()).map(|k| Ref::CarryIn(k as u8)).collect();
        let out = kernel.emit(&mut b, &holes, bound);
        b.finish(&[out]);
        let Ref::Node(output) = out else { return None };
        let block: Block = pb.blocks.pop()?;
        Some(Emitted { nodes: block.nodes, consts: pb.consts, output })
    };
    // The builder's panics are construction-time assertions; a refused emission is no match. The
    // kernels derive attributes that fit the types they were handed, so this is a backstop, and
    // the process-wide panic hook is left alone (a node's other threads keep reporting theirs).
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)).ok().flatten()
}

/// What phase 1 binds: template node → program node, the holes, the states (template → program).
#[derive(Clone, Debug, Default)]
pub(crate) struct Unified {
    pub map: BTreeMap<u16, u16>,
    pub holes: Vec<Option<Ref>>,
    pub states: BTreeMap<u16, u16>,
}

/// Phase 1: the skeleton of `t` (emitted with `holes` carry-ins) against block `bi` of `p`, from
/// template node `t.output` and program node `anchor`.
pub(crate) fn unify(t: &Emitted, holes: usize, p: &TirProgramV1, bi: usize, anchor: u16) -> Option<Unified> {
    let block = &p.blocks[bi];
    let mut u = Unified { holes: vec![None; holes], ..Default::default() };
    let mut back: BTreeMap<u16, u16> = BTreeMap::new();
    let mut stack = vec![(t.output, anchor)];
    while let Some((tn, pn)) = stack.pop() {
        if let Some(prev) = u.map.get(&tn) {
            if *prev != pn {
                return None;
            }
            continue;
        }
        if back.get(&pn).is_some_and(|prev| *prev != tn) {
            return None;
        }
        let (tnode, pnode) = (t.nodes.get(tn as usize)?, block.nodes.get(pn as usize)?);
        if tnode.prim.tag() != pnode.prim.tag() || tnode.inputs.len() != pnode.inputs.len() {
            return None;
        }
        // A state the template writes is a state of the program's, bound like an operand.
        if let (
            misaka_palw_tir::Prim::StateWrite { state: ts } | misaka_palw_tir::Prim::HistAppend { state: ts },
            misaka_palw_tir::Prim::StateWrite { state: ps } | misaka_palw_tir::Prim::HistAppend { state: ps },
        ) = (&tnode.prim, &pnode.prim)
            && *u.states.entry(*ts).or_insert(*ps) != *ps
        {
            return None;
        }
        u.map.insert(tn, pn);
        back.insert(pn, tn);
        for (ti, pi) in tnode.inputs.iter().zip(pnode.inputs.iter()) {
            match (*ti, *pi) {
                (Ref::Node(a), Ref::Node(b)) => stack.push((a, b)),
                (Ref::Node(_), _) => return None,
                (Ref::CarryIn(k), r) => {
                    let slot = u.holes.get_mut(k as usize)?;
                    match slot {
                        Some(prev) if *prev != r => return None,
                        _ => *slot = Some(r),
                    }
                }
                (Ref::Const(_), Ref::Const(_)) => {}
                (Ref::State(a), Ref::State(b)) => {
                    if *u.states.entry(a).or_insert(b) != b {
                        return None;
                    }
                }
                _ => return None,
            }
        }
    }
    // Every template node is reached from its output (templates emit no dead node), and every
    // hole is bound.
    if u.map.len() != t.nodes.len() || u.holes.iter().any(Option::is_none) {
        return None;
    }
    Some(u)
}

/// Phase 2: the template emitted at the actual types (`t`, holes and states bound by `u`) equals the
/// program at every mapped node — the whole primitive, the output type, every operand under the
/// map, every const by value.
pub(crate) fn equal(t: &Emitted, u: &Unified, p: &TirProgramV1, bi: usize) -> bool {
    let block = &p.blocks[bi];
    if u.map.len() != t.nodes.len() || u.map.get(&t.output).is_none() {
        return false;
    }
    for (tn, pn) in &u.map {
        let (Some(tnode), Some(pnode)) = (t.nodes.get(*tn as usize), block.nodes.get(*pn as usize)) else { return false };
        let prim_eq = match (&tnode.prim, &pnode.prim) {
            (
                misaka_palw_tir::Prim::StateWrite { state: ts } | misaka_palw_tir::Prim::HistAppend { state: ts },
                misaka_palw_tir::Prim::StateWrite { state: ps } | misaka_palw_tir::Prim::HistAppend { state: ps },
            ) => tnode.prim.tag() == pnode.prim.tag() && u.states.get(ts) == Some(ps),
            (a, b) => a == b,
        };
        if !prim_eq || tnode.out != pnode.out || tnode.inputs.len() != pnode.inputs.len() {
            return false;
        }
        for (ti, pi) in tnode.inputs.iter().zip(pnode.inputs.iter()) {
            let ok = match (*ti, *pi) {
                (Ref::Node(a), Ref::Node(b)) => u.map.get(&a) == Some(&b),
                (Ref::CarryIn(k), r) => u.holes.get(k as usize).copied().flatten() == Some(r),
                (Ref::Const(a), Ref::Const(b)) => match (t.consts.get(a as usize), p.consts.get(b as usize)) {
                    (Some(x), Some(y)) => x == y,
                    _ => false,
                },
                (Ref::State(a), Ref::State(b)) => u.states.get(&a) == Some(&b),
                _ => false,
            };
            if !ok {
                return false;
            }
        }
    }
    true
}
