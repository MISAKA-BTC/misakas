//! **The court scope's node-level predicate and cumulative bound** (ADR-0177 D2, RFC-0014 §16.4; lane DA16, G14C GAP-06) — in the
//! kernel so the ledger's demand and response paths call it directly (consensus-core `palw_court_scope_v1` re-exports it beside the
//! route-level inventory).
//!
//! ```text
//! court_node_materials_v1(program, model_params)   [occurrence][node]: Public | Claim | ModelDependent | ModelCopy | ModelOnly
//! court_model_exposure_v1(program, model_params)   how few clear positions determine a model relation (1 elementwise; K a MatMul)
//! court_clear_position_budget_v1(exposure)         clear model-dependent positions ONE MODEL may ever serve: K_min − 1, 0 if any
//!                                                  elementwise model relation exists — below every relation's rank: no rebuild
//! court_position_disclosure_v1(served, exposure)   Clear inside the budget, CommitmentOnly past it — never a refusal
//! court_withheld_mask_v1(program, m, disclosure)   what a response omits: model bytes always; model-dependent values when
//!                                                  CommitmentOnly (owed as commitments at COURT_HIDING_LEAF_ELEMENTS_V1)
//! court_scope_record_v1(tally, requester, unit)    ≤ 16 distinct units per (claim, requester): no one draws on another's allowance
//! ```
//!
//! **G14 reachability (the producer and every seat colluding, a model-holding verifier — ADR-0177 D7).** A demand is never refused for
//! the budget, so a withheld unit is still the producer's objective default. A wrong `CommitmentOnly` value is convicted without its
//! elements: the verifier re-executes, finds the first commitment that differs, and the court recomputes ONE hiding-width leaf from
//! authenticated operands (its own earlier values, which match their commitments; param leaves from its own copy against the registered
//! commitments) and compares that leaf's hash with the producer's authenticated one. An honest claim matches everywhere: dismissed.
//! Leaves narrower than the hiding width are never owed for a model-dependent value (a one-element hash is brute-forced).

use misaka_palw_tir::Prim;
use misaka_palw_tir::program::{Ref, TirProgramV1};

use crate::hash::Digest;

/// **The per-requester bound**: distinct units of ONE claim that ONE requester may demand (INTERIM, as R-core DA-8's "16 ever"). What one
/// prosecution needs is far below it: two positions plus at most ten root probes under withholding (K2S), or one retrieval entry.
pub const COURT_SCOPE_MAX_UNITS_PER_REQUESTER_V1: usize = 16;

/// **The hiding width**: a model-dependent value owed as commitments is owed as leaf hashes over at least this many elements (≥ 512
/// bits of an int8 value) — never a one-element leaf, whose hash brute-forces to the value — and the leaf court recomputes one such leaf.
pub const COURT_HIDING_LEAF_ELEMENTS_V1: u64 = 64;

/// **Record one demanded unit in a claim's per-requester tally** (`[(requester, units)]`, both ascending): a unit the requester already
/// named costs nothing again; a new one is refused once the requester holds [`COURT_SCOPE_MAX_UNITS_PER_REQUESTER_V1`].
pub fn court_scope_record_v1(
    requested: &mut Vec<(Digest, Vec<(u8, u32)>)>,
    requester: Digest,
    unit: (u8, u32),
) -> Result<(), &'static str> {
    let at = match requested.binary_search_by(|(r, _)| r.cmp(&requester)) {
        Ok(at) => at,
        Err(at) => {
            requested.insert(at, (requester, Vec::new()));
            at
        }
    };
    let units = &mut requested[at].1;
    if let Err(i) = units.binary_search(&unit) {
        if units.len() >= COURT_SCOPE_MAX_UNITS_PER_REQUESTER_V1 {
            return Err("the requester has named the most units of this claim one requester may (the court scope)");
        }
        units.insert(i, unit);
    }
    Ok(())
}

// ---- node values: which ones are model bytes -----------------------------------------------------------------------------------

/// **What one committed node value is**, by its sources (params below `model_params` are weights; params at or above it are a
/// pipeline stage's lifted inputs; consts are the registered program, public; token / position inputs, states and carries are the
/// claim's).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NodeMaterialV1 {
    /// From consts alone: anyone computes it from the public program.
    Public,
    /// Model-INDEPENDENT claim material (no weight reaches it).
    Claim,
    /// Model-DEPENDENT claim material (an activation): claim-specific, the relational residual (module doc).
    ModelDependent,
    /// Contains model elements by copy, selected by claim data (`Gather` of an embedding row, a slice / view / cast of a weight).
    ModelCopy,
    /// A function of the model alone (a dequantized or transposed weight): the same at every position — model bytes.
    ModelOnly,
}

impl NodeMaterialV1 {
    /// **Level 1** (ADR-0177 D2's ban on weights and file ranges): never owed in the clear by any court.
    pub const fn is_model_bytes(self) -> bool {
        matches!(self, Self::ModelCopy | Self::ModelOnly)
    }

    /// **Level 2** (also no relational reconstruction): owed only as commitment structure (hashes).
    pub const fn is_model_dependent(self) -> bool {
        matches!(self, Self::ModelDependent | Self::ModelCopy | Self::ModelOnly)
    }
}

/// Dependency flags of a value: on the model, on the claim, and whether some element is a copy of a model element.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Taint {
    model: bool,
    claim: bool,
    copy: bool,
}

impl Taint {
    const PUBLIC: Taint = Taint { model: false, claim: false, copy: false };
    const CLAIM: Taint = Taint { model: false, claim: true, copy: false };
    const WEIGHT: Taint = Taint { model: true, claim: false, copy: true };

    fn join(self, o: Taint) -> Taint {
        Taint { model: self.model || o.model, claim: self.claim || o.claim, copy: self.copy || o.copy }
    }

    /// Copying this value copies model elements: it is (a copy of) model bytes itself.
    fn copies_model(self) -> bool {
        self.copy || (self.model && !self.claim)
    }

    fn material(self) -> NodeMaterialV1 {
        match (self.model, self.claim) {
            (false, false) => NodeMaterialV1::Public,
            (false, true) => NodeMaterialV1::Claim,
            (true, false) => NodeMaterialV1::ModelOnly,
            (true, true) if self.copy => NodeMaterialV1::ModelCopy,
            (true, true) => NodeMaterialV1::ModelDependent,
        }
    }
}

/// The inputs whose ELEMENTS a primitive's output copies (exactly, or by an exact conversion / saturation); `None`: arithmetic.
fn copied_inputs(prim: &Prim) -> Option<&'static [usize]> {
    match prim {
        Prim::Reshape
        | Prim::Transpose { .. }
        | Prim::Slice { .. }
        | Prim::Broadcast
        | Prim::Cast
        | Prim::Clamp { .. }
        | Prim::ReduceMax { .. }
        | Prim::StateWrite { .. }
        | Prim::HistAppend { .. } => Some(&[0]),
        // The data, not the index.
        Prim::Gather { .. } => Some(&[0]),
        // `a` and `b`, not the condition.
        Prim::Select => Some(&[1, 2]),
        Prim::Concat { .. } => Some(&[0, 1, 2, 3, 4, 5, 6, 7]),
        _ => None,
    }
}

/// **Every committed node value's material, per occurrence** — `[occurrence][node]`, aligned with the kernel's `derived_mask_v1`.
/// `model_params`: how many of the program's params are weights (`program.params.len()` for a single program; a pipeline stage's
/// real count for a TIR v2 stage, whose higher params are its lifted inputs). States and carries are solved to a fixpoint over
/// positions (a state written from model bytes is model bytes at the next position).
pub fn court_node_materials_v1(program: &TirProgramV1, model_params: usize) -> Vec<Vec<NodeMaterialV1>> {
    let occurrences = program.occurrences();
    let mut states = vec![Taint::PUBLIC; program.states.len()];
    loop {
        let mut written = states.clone();
        let mut out = Vec::with_capacity(occurrences.len());
        let mut carry: Vec<Taint> = Vec::new();
        for (b, _) in &occurrences {
            let Some(block) = program.blocks.get(*b as usize) else {
                out.push(Vec::new());
                continue;
            };
            let mut taints: Vec<Taint> = Vec::with_capacity(block.nodes.len());
            for node in &block.nodes {
                let source = |r: &Ref, taints: &[Taint]| -> Taint {
                    match *r {
                        Ref::Node(i) => taints.get(i as usize).copied().unwrap_or(Taint::PUBLIC),
                        Ref::CarryIn(k) => carry.get(k as usize).copied().unwrap_or(Taint::PUBLIC),
                        Ref::Param(j) if (j as usize) < model_params => Taint::WEIGHT,
                        Ref::Param(_) => Taint::CLAIM,
                        Ref::Const(_) => Taint::PUBLIC,
                        Ref::State(j) => states.get(j as usize).copied().unwrap_or(Taint::PUBLIC),
                        Ref::Input(_) => Taint::CLAIM,
                    }
                };
                let inputs: Vec<Taint> = node.inputs.iter().map(|r| source(r, &taints)).collect();
                let mut t = inputs.iter().fold(Taint::PUBLIC, |acc, x| acc.join(*x));
                t.copy = match copied_inputs(&node.prim) {
                    Some(data) => data.iter().filter_map(|i| inputs.get(*i)).any(|x| x.copies_model()),
                    None => false,
                };
                if let Prim::StateWrite { state } = node.prim
                    && let Some(w) = written.get_mut(state as usize)
                {
                    *w = w.join(t);
                }
                taints.push(t);
            }
            carry = block.carry_out.iter().map(|i| taints.get(*i as usize).copied().unwrap_or(Taint::PUBLIC)).collect();
            out.push(taints.into_iter().map(Taint::material).collect());
        }
        if written == states {
            return out;
        }
        states = written;
    }
}

/// **Level 1's mask** — `[occurrence][node]`: the values no court owes in the clear (a copy of model elements, or a function of the
/// model alone). K2S ORs it into a position's omitted set beside `derived_mask_v1` (design doc §7.6).
pub fn court_model_bytes_mask_v1(program: &TirProgramV1, model_params: usize) -> Vec<Vec<bool>> {
    court_node_materials_v1(program, model_params).into_iter().map(|o| o.into_iter().map(|m| m.is_model_bytes()).collect()).collect()
}

/// **Level 2's mask**: every model-dependent value (owed as commitment hashes only).
pub fn court_model_dependent_mask_v1(program: &TirProgramV1, model_params: usize) -> Vec<Vec<bool>> {
    court_node_materials_v1(program, model_params)
        .into_iter()
        .map(|o| o.into_iter().map(|m| m.is_model_dependent()).collect())
        .collect()
}

/// **The relational residual of Level 1**, per program: how few clear positions reveal a model relation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModelExposureV1 {
    /// Elementwise relations between a model operand and a claim operand (`x ⊙ g`, `x + b`): ONE clear position reveals `g`.
    pub elementwise_relations: usize,
    /// The smallest contracted dimension `K` of a `MatMul` with a model operand: `K` clear positions determine its weight.
    pub min_linear_inner_dim: Option<u64>,
}

/// [`ModelExposureV1`] of a program (shapes at `H = 1`).
pub fn court_model_exposure_v1(program: &TirProgramV1, model_params: usize) -> ModelExposureV1 {
    let materials = court_node_materials_v1(program, model_params);
    let mut out = ModelExposureV1::default();
    for ((b, _), mats) in program.occurrences().iter().zip(&materials) {
        let Some(block) = program.blocks.get(*b as usize) else { continue };
        let shape_of = |r: &Ref| -> Option<Vec<u64>> {
            match *r {
                Ref::Param(j) => program.params.get(j as usize).map(|p| p.shape.iter().map(|d| *d as u64).collect()),
                Ref::Node(i) => block.nodes.get(i as usize).map(|n| n.out.resolve(1).into_iter().map(|d| d as u64).collect()),
                _ => None,
            }
        };
        // Is this operand model bytes (a weight, or a model-only / model-copy value)?
        let is_model = |r: &Ref| match *r {
            Ref::Param(j) => (j as usize) < model_params,
            Ref::Node(i) => mats.get(i as usize).is_some_and(|m| m.is_model_bytes()),
            _ => false,
        };
        for (n, node) in block.nodes.iter().enumerate() {
            if mats.get(n) != Some(&NodeMaterialV1::ModelDependent) {
                continue;
            }
            match node.prim {
                Prim::MatMul => {
                    let (a, bb) = (node.inputs.first(), node.inputs.get(1));
                    let k = match (a, bb) {
                        (Some(_), Some(bb)) if is_model(bb) => shape_of(bb).and_then(|s| s.len().checked_sub(2).map(|i| s[i])),
                        (Some(a), Some(_)) if is_model(a) => shape_of(a).and_then(|s| s.last().copied()),
                        _ => None,
                    };
                    if let Some(k) = k {
                        out.min_linear_inner_dim = Some(out.min_linear_inner_dim.map_or(k, |m| m.min(k)));
                    }
                }
                Prim::Add | Prim::Sub | Prim::Mul | Prim::Div { .. } if node.inputs.iter().any(&is_model) => {
                    out.elementwise_relations += 1;
                }
                _ => {}
            }
        }
    }
    out
}

/// **The clear-disclosure budget of ONE MODEL** (F-C4R4-17): how many positions, over every claim of the model and every requester,
/// a court may ever serve with model-dependent values in the clear — below the rank of every model relation, so none is determined:
/// `K_min − 1` for the smallest contracted dimension of a model-linear relation, 0 when any elementwise model relation exists (one
/// clear position reveals its weight), unbounded when the program has no model relation at all.
pub fn court_clear_position_budget_v1(exposure: &ModelExposureV1) -> u64 {
    if exposure.elementwise_relations > 0 {
        return 0;
    }
    exposure.min_linear_inner_dim.map_or(u64::MAX, |k| k.saturating_sub(1))
}

/// What one admitted position demand owes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DisclosureV1 {
    /// Every committed value but model bytes, in the clear (Level 1).
    Clear,
    /// Model-independent values in the clear; model-dependent values as their commitments at the hiding width only (Level 2).
    CommitmentOnly,
}

/// **The cumulative-scope predicate** the ledger calls when it opens a position demand: `served_clear_for_model` is how many positions
/// of this MODEL were already served `Clear` (over all its claims). Inside the budget: `Clear`; past it: `CommitmentOnly`. Never a
/// refusal — the bound changes what is disclosed, not whether the producer must answer.
pub fn court_position_disclosure_v1(served_clear_for_model: u64, exposure: &ModelExposureV1) -> DisclosureV1 {
    if served_clear_for_model < court_clear_position_budget_v1(exposure) { DisclosureV1::Clear } else { DisclosureV1::CommitmentOnly }
}

/// **What a response under `disclosure` omits** (`[occurrence][node]`, OR it into `derived_mask_v1`): model bytes always; every
/// model-dependent value when `CommitmentOnly`.
pub fn court_withheld_mask_v1(program: &TirProgramV1, model_params: usize, disclosure: DisclosureV1) -> Vec<Vec<bool>> {
    match disclosure {
        DisclosureV1::Clear => court_model_bytes_mask_v1(program, model_params),
        DisclosureV1::CommitmentOnly => court_model_dependent_mask_v1(program, model_params),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::{DType, Dim};

    fn toy() -> TirProgramV1 {
        let mut pb = ProgramBuilder::new(16, 8);
        let emb = pb.param("emb", DType::I32, &[16, 4], false);
        let w = pb.param("w", DType::I32, &[4, 4], false);
        let g = pb.param("g", DType::I32, &[1, 4], false);
        let u = pb.param("u", DType::I32, &[4, 16], false);
        let pre = {
            let mut b = pb.block("pre", vec![]);
            let e = b.gather(emb, Ref::Input(0), 0, 0); // 0: ModelCopy
            let e2 = b.reshape_fixed(e, &[1, 4]); // 1: ModelCopy (a view of the copy)
            let _t = b.transpose(w, &[1, 0]); // 2: ModelOnly
            let h = b.matmul(e2, w, DType::I32); // 3: ModelDependent (an embedding row times a weight — model-only data, but
            //    selected by the token: claim AND model, arithmetic)
            let gh = b.mul(h, g, DType::I32); // 4: ModelDependent
            let _p = b.cast(Ref::Input(1), DType::I32); // 5: Claim
            let c = b.c(DType::I32, 3); // 6: Public (an Iota-free const view)
            let _k = b.add(c, c, DType::I32); // 7: Public
            b.finish(&[gh])
        };
        let layer = {
            let ty = misaka_palw_tir::types::TensorType::new(DType::I32, vec![Dim::Fixed(1), Dim::Fixed(4)]);
            let mut b = pb.block("layer", vec![ty]);
            let x = b.add(Ref::CarryIn(0), Ref::CarryIn(0), DType::I32);
            b.finish(&[x])
        };
        let post = {
            let ty = misaka_palw_tir::types::TensorType::new(DType::I32, vec![Dim::Fixed(1), Dim::Fixed(4)]);
            let mut b = pb.block("post", vec![ty]);
            let l = b.matmul(Ref::CarryIn(0), u, DType::I32);
            b.finish(&[l])
        };
        pb.finish(pre, vec![layer, layer], post, 0)
    }

    #[test]
    fn the_node_materials_separate_weights_copies_and_activations() {
        use NodeMaterialV1 as N;
        let p = toy();
        let m = court_node_materials_v1(&p, p.params.len());
        assert_eq!(m.len(), 4, "pre, two layer occurrences, post");
        // The pre block's committed values, in node order (`b.c` is a const reference, not a node):
        let pre: Vec<N> = m[0].clone();
        assert_eq!(pre[0], N::ModelCopy, "an embedding row selected by the token is model bytes");
        assert_eq!(pre[1], N::ModelCopy, "a view of that copy too");
        assert_eq!(pre[2], N::ModelOnly, "a transposed weight is the model alone");
        assert_eq!(pre[3], N::ModelDependent, "a product of model data is an activation, not a copy");
        assert_eq!(pre[4], N::ModelDependent);
        assert_eq!(pre[5], N::Claim, "the position, cast: model-independent claim material");
        assert!(pre[6..].iter().all(|x| *x == N::Public), "consts are the registered program: public");
        assert_eq!(m[1], vec![N::ModelDependent], "a carry of an activation stays one, across occurrences");
        assert_eq!(m[3], vec![N::ModelDependent], "the logits");
        let mask = court_model_bytes_mask_v1(&p, p.params.len());
        assert_eq!(mask[0][..3], [true, true, true]);
        assert!(!mask[0][3] && !mask[1][0] && !mask[3][0], "Level 1 owes activations in the clear");
        let strong = court_model_dependent_mask_v1(&p, p.params.len());
        assert!(strong[0][3] && strong[1][0] && strong[3][0] && !strong[0][5], "Level 2 owes them as hashes; claim-only stays clear");
        // The same program with every param lifted to a stage input (`model_params = 0`): nothing is model bytes.
        assert!(court_node_materials_v1(&p, 0).iter().flatten().all(|x| matches!(x, N::Claim | N::Public)));
    }

    #[test]
    fn a_state_written_from_model_bytes_is_model_bytes_at_the_next_position() {
        let mut pb = ProgramBuilder::new(16, 8);
        let w = pb.param("w", DType::I32, &[4], false);
        let s = pb.fixed_state("s", DType::I32, &[4], -1_000, 1_000, false);
        let pre = {
            let mut b = pb.block("pre", vec![]);
            let r = b.add(Ref::State(s), Ref::State(s), DType::I32); // 0: reads the state
            let _ = b.state_write(s, w); // 1: writes a copy of the weight
            b.finish(&[r])
        };
        let p = pb.finish(pre, vec![], pre, 0);
        let m = court_node_materials_v1(&p, p.params.len());
        assert_eq!(m[0][1], NodeMaterialV1::ModelOnly, "the write is the weight");
        assert_eq!(m[0][0], NodeMaterialV1::ModelOnly, "the next position reads it: the fixpoint carries it");
    }

    #[test]
    fn the_exposure_names_the_one_position_and_the_k_position_relations() {
        let p = toy();
        let e = court_model_exposure_v1(&p, p.params.len());
        // pre's `MatMul(e2, W)` (W is [4, 4]: K = 4), post's `MatMul(x, U)` (U is [4, 16]: K = 4); `Mul(h, G)` is elementwise.
        assert_eq!(e.min_linear_inner_dim, Some(4));
        assert_eq!(e.elementwise_relations, 1);
    }

    #[test]
    fn the_budget_stays_below_every_relation_and_never_refuses() {
        let p = toy();
        let e = court_model_exposure_v1(&p, p.params.len());
        assert_eq!(court_clear_position_budget_v1(&e), 0, "an elementwise model relation: one clear position would reveal its weight");
        let linear = ModelExposureV1 { elementwise_relations: 0, min_linear_inner_dim: Some(20) };
        assert_eq!(court_clear_position_budget_v1(&linear), 19, "K − 1: a K-column weight is never determined");
        assert_eq!(court_position_disclosure_v1(18, &linear), DisclosureV1::Clear);
        assert_eq!(
            court_position_disclosure_v1(19, &linear),
            DisclosureV1::CommitmentOnly,
            "past the budget: commitments, still owed"
        );
        assert_eq!(court_position_disclosure_v1(0, &e), DisclosureV1::CommitmentOnly);
        let withheld = court_withheld_mask_v1(&p, p.params.len(), DisclosureV1::CommitmentOnly);
        assert!(withheld[0][3] && !withheld[0][5], "activations withheld, model-independent values still clear");
        assert_eq!(court_withheld_mask_v1(&p, p.params.len(), DisclosureV1::Clear), court_model_bytes_mask_v1(&p, p.params.len()));
    }

    #[test]
    fn the_per_requester_tally_bounds_each_requester_and_never_another() {
        let (a, b) = ([1u8; 64], [2u8; 64]);
        let mut t = Vec::new();
        for p in 0..COURT_SCOPE_MAX_UNITS_PER_REQUESTER_V1 as u32 {
            court_scope_record_v1(&mut t, b, (0, p)).unwrap();
        }
        court_scope_record_v1(&mut t, b, (0, 3)).expect("a unit already named costs nothing again");
        assert!(court_scope_record_v1(&mut t, b, (0, 999)).is_err(), "the requester's allowance is spent");
        court_scope_record_v1(&mut t, a, (0, 999)).unwrap();
        assert_eq!(t.iter().map(|(r, u)| (*r, u.len())).collect::<Vec<_>>(), vec![(a, 1), (b, 16)]);
    }
}
