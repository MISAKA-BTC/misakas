//! **Which `MatMul` a seat checks how, read off the program's dataflow** (RFC-0007 Part II, §II.2).
//!
//! An IR class has no names a node may read meaning into (RFC-0002 G7); what a seat may sketch is
//! decided by provenance alone:
//!
//! * a value is **static** when it is a function of params and consts only and has no `H` — the
//!   same at every position of an occurrence, so a function of the artifact;
//! * a value is **routed** when it is `Gather(axis 0, batch_dims 0)` of a static tensor by an index
//!   the execution computes — a `TopK` over router logits picking expert stacks, a token picking an
//!   embedding row;
//! * everything else is **dynamic**.
//!
//! A `MatMul` with one static or routed operand and one dynamic operand is a **weight** `MatMul`:
//! the seat checks it against a sketch of the weight it built once per epoch (per expert for a
//! routed one — the gathered axes must be batch axes of the product, so that one expert's matrix is
//! one batch slice). A `MatMul` of two dynamic operands is an **activation × activation** product
//! (attention's `Q·Kᵀ` and `P·V`, the MoE combine): the seat checks it with a fresh vector, or
//! recomputes it, as its policy says. Everything else is recomputed exactly.
//!
//! Every other node is recomputed exactly from the values the check has already established, so
//! the seat needs only the params those nodes read ([`TirSketchAnalysisV1::held_params`]): norms'
//! gains, narrowing multipliers, activation tables, embedding rows. The weight matrices are not
//! among them — that is the point of the exercise, and `tests/soundness.rs` runs the checker on a
//! param source that does not have them.

use std::collections::BTreeSet;

use misaka_palw_tir::program::{Ref, TirProgramV1};
use misaka_palw_tir::{Dim, Prim};

/// Where a value comes from (module note).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TirProvenanceV1 {
    Static,
    Routed,
    Dynamic,
}

/// Which operand of a `MatMul` is the weight: `a` (left) or `b` (right).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TirSideV1 {
    Left,
    Right,
}

/// The weight operand of a weight `MatMul`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TirWeightSourceV1 {
    /// A static value: a param, or a static node over params and consts.
    Static(Ref),
    /// `Gather(data, idx)` along axis 0 of a static `data`: the product's first `idx_rank` batch
    /// axes are the gathered ones.
    Routed { data: Ref, idx: Ref, idx_rank: u8 },
}

impl TirWeightSourceV1 {
    /// The static tensor a sketch is built from (the whole stack for a routed weight).
    pub fn data(&self) -> Ref {
        match *self {
            TirWeightSourceV1::Static(r) => r,
            TirWeightSourceV1::Routed { data, .. } => data,
        }
    }
}

/// How one `MatMul` is checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TirMatMulKindV1 {
    /// Against a sketch of its weight.
    Weight { side: TirSideV1, source: TirWeightSourceV1 },
    /// Both operands depend on the execution.
    ActAct,
    /// Recomputed exactly: both operands static, or a weight gathered along a matrix axis.
    Exact,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TirMatMulSiteV1 {
    pub node: u16,
    pub kind: TirMatMulKindV1,
}

/// What a seat does with an activation × activation `MatMul` (RFC-0007 Part II, §II.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TirActActPolicyV1 {
    /// Recompute it exactly; nothing is served.
    Recompute,
    /// The producer serves it; a fresh vector checks it.
    Served,
    /// Served when its contraction (at the running `H`) is at least `k` long, recomputed otherwise:
    /// `P·V` over a long history is worth serving, `Q·Kᵀ` over a 128-wide head is not.
    ServedFrom { k: u32 },
}

/// A seat's choices for one check. Not a consensus value: two seats may choose differently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TirCheckPolicyV1 {
    pub act_act: TirActActPolicyV1,
}

impl Default for TirCheckPolicyV1 {
    fn default() -> Self {
        Self { act_act: TirActActPolicyV1::Served }
    }
}

#[derive(Clone, Debug)]
pub struct TirBlockAnalysisV1 {
    /// Per node.
    pub provenance: Vec<TirProvenanceV1>,
    /// Every `MatMul` of the block, in node order.
    pub matmuls: Vec<TirMatMulSiteV1>,
}

/// The analysis of one program: per block, per node.
#[derive(Clone, Debug)]
pub struct TirSketchAnalysisV1 {
    pub blocks: Vec<TirBlockAnalysisV1>,
}

fn ref_provenance(prov: &[TirProvenanceV1], r: Ref) -> TirProvenanceV1 {
    match r {
        Ref::Param(_) | Ref::Const(_) => TirProvenanceV1::Static,
        Ref::Node(j) => prov[j as usize],
        Ref::CarryIn(_) | Ref::State(_) | Ref::Input(_) => TirProvenanceV1::Dynamic,
    }
}

fn rank_of(program: &TirProgramV1, block: usize, r: Ref) -> usize {
    let b = &program.blocks[block];
    match r {
        Ref::Node(j) => b.nodes[j as usize].out.rank(),
        Ref::CarryIn(k) => b.carry_in[k as usize].rank(),
        Ref::Param(j) => program.params[j as usize].shape.len(),
        Ref::Const(j) => program.consts[j as usize].shape.len(),
        Ref::State(j) => program.states[j as usize].shape.len(),
        Ref::Input(_) => 0,
    }
}

impl TirSketchAnalysisV1 {
    /// Classify every node and every `MatMul` of a validated program.
    pub fn of(program: &TirProgramV1) -> Self {
        let blocks = (0..program.blocks.len()).map(|bi| Self::block(program, bi)).collect();
        Self { blocks }
    }

    fn block(program: &TirProgramV1, bi: usize) -> TirBlockAnalysisV1 {
        let b = &program.blocks[bi];
        let mut provenance: Vec<TirProvenanceV1> = Vec::with_capacity(b.nodes.len());
        let mut matmuls = Vec::new();
        for (ni, n) in b.nodes.iter().enumerate() {
            let has_h = n.out.shape.contains(&Dim::H);
            let ins: Vec<TirProvenanceV1> = n.inputs.iter().map(|r| ref_provenance(&provenance, *r)).collect();
            let p = match &n.prim {
                Prim::StateWrite { .. } | Prim::HistAppend { .. } => TirProvenanceV1::Dynamic,
                Prim::Gather { axis: 0, batch_dims: 0 }
                    if ins[0] == TirProvenanceV1::Static && ins[1] == TirProvenanceV1::Dynamic && !has_h =>
                {
                    TirProvenanceV1::Routed
                }
                _ if !has_h && ins.iter().all(|p| *p == TirProvenanceV1::Static) => TirProvenanceV1::Static,
                _ => TirProvenanceV1::Dynamic,
            };
            if n.prim == Prim::MatMul {
                let (a, bb) = (n.inputs[0], n.inputs[1]);
                let weight = |r: Ref, side: TirSideV1| -> Option<TirMatMulKindV1> {
                    match ref_provenance(&provenance, r) {
                        TirProvenanceV1::Static => Some(TirMatMulKindV1::Weight { side, source: TirWeightSourceV1::Static(r) }),
                        TirProvenanceV1::Routed => {
                            let Ref::Node(g) = r else { return None };
                            let gather = &b.nodes[g as usize];
                            let (data, idx) = (gather.inputs[0], gather.inputs[1]);
                            let idx_rank = rank_of(program, bi, idx);
                            // The gathered axes must all be batch axes of this operand: one
                            // expert's matrix is then one batch slice, and its sketch is the
                            // expert's own.
                            (idx_rank + 2 <= gather.out.rank()).then_some(TirMatMulKindV1::Weight {
                                side,
                                source: TirWeightSourceV1::Routed { data, idx, idx_rank: idx_rank as u8 },
                            })
                        }
                        TirProvenanceV1::Dynamic => None,
                    }
                };
                let (pa, pb) = (ref_provenance(&provenance, a), ref_provenance(&provenance, bb));
                let kind = match (pa, pb) {
                    (TirProvenanceV1::Dynamic, TirProvenanceV1::Dynamic) => TirMatMulKindV1::ActAct,
                    (TirProvenanceV1::Dynamic, _) => weight(bb, TirSideV1::Right).unwrap_or(TirMatMulKindV1::Exact),
                    (_, TirProvenanceV1::Dynamic) => weight(a, TirSideV1::Left).unwrap_or(TirMatMulKindV1::Exact),
                    _ => TirMatMulKindV1::Exact,
                };
                matmuls.push(TirMatMulSiteV1 { node: ni as u16, kind });
            }
            provenance.push(p);
        }
        TirBlockAnalysisV1 { provenance, matmuls }
    }

    pub fn site(&self, block: u8, node: u16) -> Option<&TirMatMulSiteV1> {
        self.blocks.get(block as usize)?.matmuls.iter().find(|s| s.node == node)
    }

    /// **Is node `node` of `block` served in the witness**, at history length `h`, under `policy`?
    /// Weight `MatMul`s always are; activation × activation ones as the policy says; nothing else.
    pub fn witnessed(&self, program: &TirProgramV1, block: u8, node: u16, h: usize, policy: &TirCheckPolicyV1) -> bool {
        let Some(site) = self.site(block, node) else { return false };
        match site.kind {
            TirMatMulKindV1::Weight { .. } => true,
            TirMatMulKindV1::Exact => false,
            TirMatMulKindV1::ActAct => match policy.act_act {
                TirActActPolicyV1::Recompute => false,
                TirActActPolicyV1::Served => true,
                TirActActPolicyV1::ServedFrom { k } => {
                    let n = &program.blocks[block as usize].nodes[node as usize];
                    let a = &program.blocks[block as usize];
                    let kdim = match n.inputs[0] {
                        Ref::Node(j) => a.nodes[j as usize].out.shape.last().copied(),
                        Ref::CarryIn(c) => a.carry_in[c as usize].shape.last().copied(),
                        _ => None,
                    };
                    kdim.map(|d| d.at(h) as u64 >= k as u64).unwrap_or(false)
                }
            },
        }
    }

    /// **The nodes of `block` a checker evaluates exactly** at history length `h`: the backward
    /// closure of every root — commit points, carry-outs, state writes, history appends, the
    /// logits, and the operands each served `MatMul`'s check reads — that stops at served nodes.
    /// A weight's own subgraph is reached only if something other than its product reads it.
    pub fn checker_needed(&self, program: &TirProgramV1, block: u8, h: usize, policy: &TirCheckPolicyV1) -> Vec<bool> {
        let b = &program.blocks[block as usize];
        let n = b.nodes.len();
        let served: Vec<bool> = (0..n).map(|i| self.witnessed(program, block, i as u16, h, policy)).collect();
        let mut stack: Vec<usize> = Vec::new();
        let push_ref = |stack: &mut Vec<usize>, r: Ref| {
            if let Ref::Node(j) = r {
                stack.push(j as usize);
            }
        };
        for (i, node) in b.nodes.iter().enumerate() {
            if node.commit || matches!(node.prim, Prim::StateWrite { .. } | Prim::HistAppend { .. }) {
                stack.push(i);
            }
            if served[i] {
                match self.site(block, i as u16).map(|s| s.kind) {
                    Some(TirMatMulKindV1::Weight { side, source }) => {
                        push_ref(&mut stack, if side == TirSideV1::Right { node.inputs[0] } else { node.inputs[1] });
                        if let TirWeightSourceV1::Routed { idx, .. } = source {
                            push_ref(&mut stack, idx);
                        }
                    }
                    _ => {
                        push_ref(&mut stack, node.inputs[0]);
                        push_ref(&mut stack, node.inputs[1]);
                    }
                }
            }
        }
        stack.extend(b.carry_out.iter().map(|c| *c as usize));
        if program.schedule.post == block {
            stack.push(program.logits as usize);
        }
        let mut needed = vec![false; n];
        while let Some(i) = stack.pop() {
            if std::mem::replace(&mut needed[i], true) || served[i] {
                continue;
            }
            for r in &b.nodes[i].inputs {
                push_ref(&mut stack, *r);
            }
        }
        // A served node is supplied, never evaluated.
        for (i, s) in served.iter().enumerate() {
            if *s {
                needed[i] = false;
            }
        }
        needed
    }

    /// **The params a checker reads** — every param an exactly evaluated node names, at the
    /// longest history `h_max` and the shortest (`1`), whose served sets bound every other's.
    pub fn held_params(&self, program: &TirProgramV1, h_max: usize, policy: &TirCheckPolicyV1) -> BTreeSet<u16> {
        let mut held = BTreeSet::new();
        for bi in 0..program.blocks.len() {
            for h in [1, h_max.max(1)] {
                let needed = self.checker_needed(program, bi as u8, h, policy);
                for (i, n) in program.blocks[bi].nodes.iter().enumerate() {
                    if needed[i] {
                        held.extend(n.inputs.iter().filter_map(|r| if let Ref::Param(j) = r { Some(*j) } else { None }));
                    }
                }
            }
        }
        held
    }

    /// **The params a sketch stands in for** — read by a weight `MatMul`'s operand and by no
    /// exactly evaluated node.
    pub fn sketched_params(&self, program: &TirProgramV1, h_max: usize, policy: &TirCheckPolicyV1) -> BTreeSet<u16> {
        let held = self.held_params(program, h_max, policy);
        let mut out = BTreeSet::new();
        for (bi, ba) in self.blocks.iter().enumerate() {
            for s in &ba.matmuls {
                if let TirMatMulKindV1::Weight { source, .. } = s.kind {
                    collect_params(program, bi, source.data(), &mut out);
                }
            }
        }
        out.retain(|j| !held.contains(j));
        out
    }
}

/// Every param a static subgraph reads.
fn collect_params(program: &TirProgramV1, block: usize, r: Ref, out: &mut BTreeSet<u16>) {
    match r {
        Ref::Param(j) => {
            out.insert(j);
        }
        Ref::Node(i) => {
            for x in &program.blocks[block].nodes[i as usize].inputs {
                collect_params(program, block, *x, out);
            }
        }
        _ => {}
    }
}
