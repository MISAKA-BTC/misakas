//! **RFC-0002 Phase F, step F7: the generic history dissection of an IR class**
//! (`docs/design/palw/tir/phase-f-integration.md` §2.8; spec 04b §9.5).
//!
//! A committed tile whose cone reduces over the history — a `ReduceSum` or `ReduceMax` along an `H`
//! axis, a `MatMul` contracting `H` — costs `O(H)` to recompute, so past some context it cannot be
//! closed in one move. ADR-0082 dissected the one such site the legacy classes have (the fused
//! attention, three reductions: the row max, the exponent sum, the value sum); an IR program says
//! which reductions it has, so the dissection is over EVERY reduction over `H` in the disputed
//! tile's cone, `r_1 … r_n` (node order), each one's partials folding exactly — `Σ` for a sum or a
//! contraction, `max` for a maximum — because every integer sum and maximum is exact:
//!
//! 1. **The root claim** (the responder, when the ladder is terminal on a dissected leaf): for each
//!    `r_i`, the elements of it the tile's evaluation demands and its totals over `[0, H)`. It is
//!    admitted only if the tile evaluated with every `r_i` SUPPLIED from the claim reproduces the
//!    committed tile (the "finalize", spec 04b §9.5), and every total lies in `r_i`'s proven interval.
//! 2. **Rounds.** The disputed range of history tiles (`h_tile` positions each) is cut at pinned
//!    points into `k` children ([`crate::palw_attn_dissect::palw_attn_child_ranges_v1`]); the
//!    responder claims every `r_i`'s partials over each child, computed against the ROOT's totals
//!    wherever an `H`-local node consumes another reduction (the softmax's `exp(s − m*)`); the court
//!    checks the fold and the intervals; the challenger names a child.
//! 3. **The bottom** (a `TirDissection` close): one tile. The court evaluates every `r_i` over that
//!    tile's positions only, the others supplied from the root, and compares with the claim.
//!
//! This module holds the objects, the phase and the site; the court's two evaluations (the finalize
//! and the bottom) are [`crate::palw_tir_court_v1`]'s.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::Hash64;
use crate::palw_attn_dissect::{PalwAttnDissectError, palw_attn_arity_is_legal_v1, palw_attn_child_ranges_v1, palw_kary_rounds_v1};
use crate::palw_bisect::{PalwBisectNoShowV1, PalwBisectPartyV1, PalwBisectTurnV1};
use crate::palw_tir_court_v1::PalwTirConeRefutationV1;
use crate::palw_tir_step_v1::{PalwTirLeafKindV1, PalwTirLeafV1, PalwTirStepSpaceV1};
use misaka_palw_tir::demand::{DemandContext, history_length_v1, reduces_over_h_v1};
use misaka_palw_tir::interval::Interval;
use misaka_palw_tir::program::{Block, TirProgramV1};
use misaka_palw_tir::{Prim, Ref};

/// Wire version of every object in this module.
pub const PALW_TIR_DISSECT_OBJECT_VERSION_V1: u16 = 1;
/// The most reductions over `H` one dissected cone may carry (a softmax attention has three).
pub const PALW_TIR_DISSECT_MAX_REDUCTIONS: usize = 16;
/// The most demanded elements of all reductions together one claim may carry.
pub const PALW_TIR_DISSECT_MAX_VALUES: usize = 4096;

/// How a reduction's partials fold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwTirFoldV1 {
    /// A `ReduceSum` or a `MatMul` contracting `H`: the parent is the children's sum.
    Sum = 0,
    /// A `ReduceMax`: the parent is the children's maximum.
    Max = 1,
}

/// One range's claim: for each reduction (node order), one value per demanded element, in the root
/// claim's element order.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirRangeClaimV1 {
    pub partials: Vec<Vec<i128>>,
}

/// **The responder's root claim** (the terminal move on a dissected leaf).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirRootClaimV1 {
    pub version: u16,
    /// For each reduction over `H` of the disputed tile's cone (node order), the elements of it the
    /// tile's evaluation demands, ascending.
    pub elements: Vec<Vec<u32>>,
    /// Their totals over the whole history `[0, H)`.
    pub totals: PalwTirRangeClaimV1,
    /// The disputed leaf, its binding, and every unit the finalize evaluation reads — the form of a
    /// cone close, the reductions supplied from `totals` instead of computed.
    pub finalize: Box<PalwTirConeRefutationV1>,
}

/// One round's disclosure: the children of the disputed range, in the pinned order.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirDissectRoundV1 {
    pub version: u16,
    pub children: Vec<PalwTirRangeClaimV1>,
}

/// The challenger's choice of child.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirDissectChoiceV1 {
    pub version: u16,
    pub session_id: Hash64,
    pub round: u32,
    pub child: u8,
}

/// Why a dissection object is refused. Every arm is a refusal of the MOVE, never a verdict: the
/// party whose move it was keeps its turn and its deadline.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwTirDissectError {
    #[error("unsupported object version {got}")]
    UnsupportedVersion { got: u16 },
    #[error("the leaf is not a dissected leaf of this class: {0}")]
    NotDissected(&'static str),
    #[error("the claim's shape is not the site's: {0}")]
    Shape(&'static str),
    #[error("reduction {reduction} element {element}: {value} is outside the proven [{lo}, {hi}]")]
    OutsideInterval { reduction: usize, element: usize, value: i128, lo: i128, hi: i128 },
    #[error("reduction {reduction} element {element}: the children fold to {folded}, the parent claims {claimed}")]
    DoesNotFold { reduction: usize, element: usize, claimed: i128, folded: i128 },
    #[error("expected {expected} children, got {got}")]
    ChildCount { got: usize, expected: usize },
    #[error("it is not this move's turn ({0})")]
    Turn(&'static str),
    #[error("round {got} is not the phase's round {expected}")]
    Round { got: u32, expected: u32 },
    #[error("the session is not this phase's")]
    Session,
    #[error("child {got} is outside the {children} children")]
    ChildOutOfRange { got: u8, children: usize },
    #[error("the dissection is past its round budget {bound}")]
    RoundBudget { bound: u32 },
    #[error("a rung window of zero")]
    ZeroWindow,
    #[error("the deadline {deadline} has not passed at {observed}")]
    DeadlineNotReached { deadline: u64, observed: u64 },
    #[error("the phase is already terminal")]
    AlreadyTerminal,
    #[error("{0}")]
    Arity(String),
}

impl From<PalwAttnDissectError> for PalwTirDissectError {
    fn from(e: PalwAttnDissectError) -> Self {
        PalwTirDissectError::Arity(e.to_string())
    }
}

// =================================================================================================
// The site
// =================================================================================================

/// **What a dissected leaf reduces over the history** — derived from the class's program and the
/// leaf's coordinate, never supplied by a mover.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwTirDissectSiteV1 {
    pub ctx: DemandContext,
    pub node: u16,
    /// The reductions over `H` of the node's cone (node order): the cone stops at the other commit
    /// points of the occurrence, which are leaves.
    pub reductions: Vec<u16>,
    pub folds: Vec<PalwTirFoldV1>,
    /// Each reduction's proven interval: a partial over any sub-range lies in it too (an order-free
    /// partial sum is bounded by the sum's interval, a partial maximum by the maximum's).
    pub bounds: Vec<Interval>,
    /// `H` at the leaf's position, and the history tile (`h_tile`).
    pub history_positions: u32,
    pub tile_positions: u32,
    /// Each reduction's element count at that `H`.
    pub counts: Vec<u64>,
}

/// **The computed nodes of commit point `node`'s cone** (spec 04b §10.2), by index: the nodes of
/// `block` reachable backwards from `node` through `Node` refs, the walk stopping AT every other
/// commit point (a leaf of the cone, read, never computed). `computed[i]` is true for `node` and for
/// every non-commit node the walk reaches; a reached commit point other than `node` is a leaf and is
/// not in it. Empty for a node the block does not have.
pub fn palw_tir_cone_computed_v1(block: &Block, node: u16) -> Vec<bool> {
    let n = block.nodes.len();
    let mut computed = vec![false; n];
    if node as usize >= n {
        return computed;
    }
    let mut seen = vec![false; n];
    let mut stack = vec![node as usize];
    while let Some(i) = stack.pop() {
        if std::mem::replace(&mut seen[i], true) {
            continue;
        }
        if i != node as usize && block.nodes[i].commit {
            continue;
        }
        computed[i] = true;
        stack.extend(block.nodes[i].inputs.iter().filter_map(|r| if let Ref::Node(j) = r { Some(*j as usize) } else { None }));
    }
    computed
}

/// **The reductions over `H` of commit point `node`'s cone** (spec 04b §9.5.1), in node order —
/// `node` itself included when it reduces over `H`. Empty: the cone is closed whole, not dissected.
pub fn palw_tir_cone_reductions_v1(block: &Block, node: u16) -> Vec<u16> {
    let computed = palw_tir_cone_computed_v1(block, node);
    (0..block.nodes.len() as u16).filter(|i| computed[*i as usize] && reduces_over_h_v1(block, *i)).collect()
}

/// **Every dissected commit point of a program** — `(block, node)`, in block then node order: the
/// commit points whose cone reduces over `H`. A class with any is one whose ladder's terminal move
/// may be a root claim (`PalwClassStateV2::fused_attention` for an IR class).
pub fn palw_tir_dissected_commit_points_v1(program: &TirProgramV1) -> Vec<(u8, u16)> {
    let mut out = Vec::new();
    for (bi, b) in program.blocks.iter().enumerate() {
        for (ni, n) in b.nodes.iter().enumerate() {
            if n.commit && !palw_tir_cone_reductions_v1(b, ni as u16).is_empty() {
                out.push((bi as u8, ni as u16));
            }
        }
    }
    out
}

/// Why a dissected cone breaks an admission obligation (spec 04b §9.5.6).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwTirDissectObligationV1 {
    /// O-1.
    #[error("the cone reduces over the history {got} times; at most {max}")]
    TooManyReductions { got: usize, max: usize },
    /// O-2: a `Gather` whose indices carry `H` reads data that depends on a reduction over `H`, or a
    /// `Select` whose condition carries `H` chooses between operands one of which does (and which its
    /// condition does not read itself) — the element read would be chosen by a value that varies
    /// along the history, so the one-index probe of the element closure would not name it.
    #[error("node {node} ({prim}) chooses by a history-varying value between reads of a reduction's output")]
    DataDependentRead { node: u16, prim: &'static str },
    /// O-3: a reduction over `H` whose output still carries `H` (its total is not one value per
    /// `H`-free element).
    #[error("reduction {node} keeps an H axis in its output")]
    TotalCarriesH { node: u16 },
}

/// **The admission obligations of a dissected cone** (spec 04b §9.5.6, O-1 to O-3), asked of commit
/// point `node` of `block`: at most [`PALW_TIR_DISSECT_MAX_REDUCTIONS`] reductions over `H`; no
/// `Gather` with `H`-carrying indices whose data depends (through computed nodes of the cone) on a
/// reduction over `H`, and no `Select` with an `H`-carrying condition one of whose value operands
/// does; every reduction's output `H`-free. `Ok` for a cone with no reduction over `H`. Linear in
/// the block.
pub fn palw_tir_dissect_obligations_v1(block: &Block, node: u16) -> Result<(), PalwTirDissectObligationV1> {
    let computed = palw_tir_cone_computed_v1(block, node);
    let reductions: Vec<u16> =
        (0..block.nodes.len() as u16).filter(|i| computed[*i as usize] && reduces_over_h_v1(block, *i)).collect();
    if reductions.len() > PALW_TIR_DISSECT_MAX_REDUCTIONS {
        return Err(PalwTirDissectObligationV1::TooManyReductions { got: reductions.len(), max: PALW_TIR_DISSECT_MAX_REDUCTIONS });
    }
    if let Some(r) = reductions.iter().find(|r| block.nodes[**r as usize].out.has_h()) {
        return Err(PalwTirDissectObligationV1::TotalCarriesH { node: *r });
    }
    // Which computed nodes of the cone depend on a reduction over H (refs are strictly backward, so
    // one pass in index order sees every input first; a leaf of the cone is read, never derived).
    let mut derived = vec![false; block.nodes.len()];
    for (i, n) in block.nodes.iter().enumerate() {
        if !computed[i] {
            continue;
        }
        derived[i] = reductions.contains(&(i as u16))
            || n.inputs.iter().any(|r| matches!(r, Ref::Node(j) if computed[*j as usize] && derived[*j as usize]));
    }
    let depends = |r: &Ref| matches!(r, Ref::Node(j) if computed[*j as usize] && derived[*j as usize]);
    // An operand carries H iff its declared type does (a param, const, state or input never does).
    let carries_h = |r: Option<&Ref>| match r {
        Some(Ref::Node(j)) => block.nodes.get(*j as usize).is_some_and(|m| m.out.has_h()),
        Some(Ref::CarryIn(k)) => block.carry_in.get(*k as usize).is_some_and(|t| t.has_h()),
        _ => false,
    };
    // A `Select` reads its condition, then ONLY the chosen operand (spec 04b §9.4) — so a value
    // operand that depends on a reduction is a history-varying read unless the condition reads it
    // anyway, at the same element: the condition a `Compare` of the `Select`'s shape with that
    // operand, of the same shape, as a direct input (a shifted softmax's clamp-by-select,
    // `select(x − m < floor, floor, x − m)`, whose read set is its condition's).
    let read_by_the_condition = |select: &misaka_palw_tir::Node, operand: &Ref| -> bool {
        let (Some(Ref::Node(c)), Ref::Node(j)) = (select.inputs.first(), operand) else { return false };
        let (Some(condition), Some(value)) = (block.nodes.get(*c as usize), block.nodes.get(*j as usize)) else { return false };
        matches!(condition.prim, Prim::Compare { .. })
            && condition.inputs.contains(operand)
            && condition.out.shape == select.out.shape
            && value.out.shape == select.out.shape
    };
    for (i, n) in block.nodes.iter().enumerate() {
        if !computed[i] {
            continue;
        }
        let bad = match n.prim {
            Prim::Gather { .. } => (carries_h(n.inputs.get(1)) && n.inputs.first().is_some_and(depends)).then_some("Gather"),
            Prim::Select => (carries_h(n.inputs.first())
                && n.inputs.iter().skip(1).any(|r| depends(r) && !read_by_the_condition(n, r)))
            .then_some("Select"),
            _ => None,
        };
        if let Some(prim) = bad {
            return Err(PalwTirDissectObligationV1::DataDependentRead { node: i as u16, prim });
        }
    }
    Ok(())
}

/// **An upper bound on the values one claim of a dissected tile of commit point `node` carries** —
/// `Σ_i |L_i|` of spec 04b §9.5.3 — by the box demand of §10.3 at `H = 1`: the tile's `tile_len`
/// elements demand, for the cone's computed nodes in descending index order, `d · K` of each
/// `MatMul` operand, `d · x.shape[axis]` of a reduction's, `⌈d / k⌉ · x.shape[axis]` of a `TopK`'s and
/// `d` of every other operand (each capped at the operand's element count), and a reduction over
/// `H` passes on exactly what its one-history-index probe reads. The bound is the sum over the
/// cone's reductions of the demand arriving at each (capped at its element count): the finalize and
/// the probes read at most that many of its elements, whatever the values (a count, not a set, so a
/// value-chosen read in the `H`-free region is bounded too). What admission sizes a round, a root
/// claim and the value cap against.
pub fn palw_tir_dissect_value_bound_v1(program: &TirProgramV1, block: &Block, node: u16, tile_len: u32) -> u64 {
    let computed = palw_tir_cone_computed_v1(block, node);
    let n = block.nodes.len();
    if node as usize >= n {
        return 0;
    }
    let count = |i: usize| block.nodes[i].out.elements_at(1);
    let mut demand = vec![0u64; n];
    demand[node as usize] = (tile_len as u64).min(count(node as usize));
    // Every operand's shape at `H = 1`, whatever it refers to — a param, a constant or a state is as
    // much a `MatMul`'s first operand as a node is, and its `K` passes `d · K` to the second (ref2's
    // H2: a leaf's shape read as absent passed only `d`).
    let operand_shape = |r: &Ref| -> Option<Vec<usize>> {
        let fixed = |shape: &[u32]| shape.iter().map(|d| *d as usize).collect::<Vec<_>>();
        match r {
            Ref::Node(j) => block.nodes.get(*j as usize).map(|m| m.out.resolve(1)),
            Ref::CarryIn(k) => block.carry_in.get(*k as usize).map(|t| t.resolve(1)),
            Ref::Param(j) => program.params.get(*j as usize).map(|p| fixed(&p.shape)),
            Ref::Const(j) => program.consts.get(*j as usize).map(|c| fixed(&c.shape)),
            Ref::State(j) => program.states.get(*j as usize).map(|s| fixed(&s.shape)),
            Ref::Input(_) => Some(Vec::new()),
        }
    };
    for i in (0..n).rev() {
        let d = demand[i];
        if d == 0 || !computed[i] {
            continue;
        }
        let nd = &block.nodes[i];
        let first = nd.inputs.first().and_then(|r| operand_shape(r));
        let per_operand = match nd.prim {
            Prim::MatMul => d.saturating_mul(first.as_ref().and_then(|s| s.last()).copied().unwrap_or(1) as u64),
            Prim::ReduceSum { axis } | Prim::ReduceMax { axis } => {
                d.saturating_mul(first.as_ref().and_then(|s| s.get(axis as usize)).copied().unwrap_or(1) as u64)
            }
            Prim::TopK { axis, k } => d
                .div_ceil(k.max(1) as u64)
                .saturating_mul(first.as_ref().and_then(|s| s.get(axis as usize)).copied().unwrap_or(1) as u64),
            _ => d,
        };
        for r in &nd.inputs {
            if let Ref::Node(j) = r
                && computed[*j as usize]
            {
                let j = *j as usize;
                demand[j] = demand[j].saturating_add(per_operand).min(count(j));
            }
        }
    }
    (0..n).filter(|i| computed[*i] && reduces_over_h_v1(block, *i as u16)).map(|i| demand[i].min(count(i))).sum()
}

/// **The bytes one round of a dissection weighs on the wire** at arity `arity`, each child claiming
/// `values` values — the borsh encoding of [`PalwTirDissectRoundV1`] with `arity` children of one
/// claim each, every reduction's list prefixed (`reductions` of them): what admission compares with
/// one carrier.
pub fn palw_tir_dissect_round_bytes_v1(arity: u8, reductions: usize, values: u64) -> u64 {
    let child = 4u64.saturating_add(4u64.saturating_mul(reductions as u64)).saturating_add(values.saturating_mul(16));
    2u64.saturating_add(4).saturating_add((arity as u64).saturating_mul(child))
}

/// **The site of a commit leaf, if its cone reduces over `H`** (`None` otherwise: the leaf is closed
/// by a cone close, not dissected).
pub fn palw_tir_dissect_site_v1(
    space: &PalwTirStepSpaceV1,
    intervals: &[Vec<Interval>],
    leaf: &PalwTirLeafV1,
) -> Option<PalwTirDissectSiteV1> {
    let PalwTirLeafKindV1::Commit { occurrence, block, node, .. } = leaf.kind else { return None };
    let b = space.program.blocks.get(block as usize)?;
    let reductions = palw_tir_cone_reductions_v1(b, node);
    if reductions.is_empty() || reductions.len() > PALW_TIR_DISSECT_MAX_REDUCTIONS {
        return None;
    }
    let h = history_length_v1(&space.info, block, leaf.position)? as u64;
    let folds = reductions
        .iter()
        .map(|r| if matches!(b.nodes[*r as usize].prim, Prim::ReduceMax { .. }) { PalwTirFoldV1::Max } else { PalwTirFoldV1::Sum })
        .collect();
    let bounds = reductions
        .iter()
        .map(|r| intervals.get(block as usize).and_then(|v| v.get(*r as usize)).copied())
        .collect::<Option<Vec<_>>>()?;
    let counts = reductions.iter().map(|r| b.nodes[*r as usize].out.elements_at(h)).collect();
    Some(PalwTirDissectSiteV1 {
        ctx: DemandContext { pos: leaf.position, occurrence: occurrence as u16 },
        node,
        reductions,
        folds,
        bounds,
        history_positions: h as u32,
        tile_positions: space.layout.h_tile,
        counts,
    })
}

// =================================================================================================
// The phase
// =================================================================================================

/// **One court session's IR dissection phase**, kept in the chain state's `tir_dissections` beside the
/// session (the legacy phase lives in the session record; this one is appended as its own table, so a
/// state with none roots as before). `turn()` and `last_deadline_daa()` are what the session clock
/// reads, exactly as for the legacy phase.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirDissectPhaseV1 {
    session_id: Hash64,
    arity: u8,
    /// The leaf the ladder narrowed to.
    leaf_index: u64,
    /// The site, frozen at the opening.
    reductions: Vec<u16>,
    folds: Vec<PalwTirFoldV1>,
    bounds: Vec<(i128, i128)>,
    elements: Vec<Vec<u32>>,
    history_positions: u32,
    tile_positions: u32,
    /// The root's totals — supplied to every level's `H`-local nodes.
    root: PalwTirRangeClaimV1,
    /// The claim of the range under dispute: the root's at round 0, then the named child's.
    claim: PalwTirRangeClaimV1,
    /// The disputed range, in tiles.
    tile_first: u64,
    tile_count: u64,
    round: u32,
    turn: PalwBisectTurnV1,
    last_deadline_daa: u64,
    pending: Vec<PalwTirRangeClaimV1>,
}

/// **A root claim's shape and values against its site**: one ascending, distinct element list inside
/// each reduction and one value per element, within the carriage cap; every value in its
/// reduction's proven interval.
pub fn palw_tir_dissect_check_claim_v1(
    site: &PalwTirDissectSiteV1,
    elements: &[Vec<u32>],
    claim: &PalwTirRangeClaimV1,
) -> Result<(), PalwTirDissectError> {
    check_shape(site, elements, claim)?;
    let bounds: Vec<(i128, i128)> = site.bounds.iter().map(|b| (b.lo, b.hi)).collect();
    check_bounds(&bounds, claim)
}

fn check_shape(site: &PalwTirDissectSiteV1, elements: &[Vec<u32>], claim: &PalwTirRangeClaimV1) -> Result<(), PalwTirDissectError> {
    let n = site.reductions.len();
    if elements.len() != n || claim.partials.len() != n {
        return Err(PalwTirDissectError::Shape("one element list and one value list per reduction"));
    }
    let mut total = 0usize;
    for (i, e) in elements.iter().enumerate() {
        // A list may be EMPTY (ref2's H1): a tile that reads none of a reduction of its cone — a
        // `Concat`, `Slice` or `Gather` routing its rows around it — claims nothing of it, and the
        // exact-closure check (spec 04b §9.5.3 step 5) is what then requires the empty list.
        if e.windows(2).any(|w| w[0] >= w[1]) || e.last().is_some_and(|x| *x as u64 >= site.counts[i]) {
            return Err(PalwTirDissectError::Shape("a reduction's elements are ascending, distinct and inside it"));
        }
        if claim.partials[i].len() != e.len() {
            return Err(PalwTirDissectError::Shape("one value per demanded element"));
        }
        total += e.len();
    }
    if total > PALW_TIR_DISSECT_MAX_VALUES {
        return Err(PalwTirDissectError::Shape("more demanded values than a claim may carry"));
    }
    Ok(())
}

fn check_bounds(bounds: &[(i128, i128)], claim: &PalwTirRangeClaimV1) -> Result<(), PalwTirDissectError> {
    for (i, (values, (lo, hi))) in claim.partials.iter().zip(bounds).enumerate() {
        if let Some((e, v)) = values.iter().enumerate().find(|(_, v)| **v < *lo || **v > *hi) {
            return Err(PalwTirDissectError::OutsideInterval { reduction: i, element: e, value: *v, lo: *lo, hi: *hi });
        }
    }
    Ok(())
}

impl PalwTirDissectPhaseV1 {
    /// **Open the phase from an admitted root claim.** The finalize (the claim reproduces the
    /// committed tile) is the acceptance layer's, which holds the court's work limits; this checks
    /// the claim's shape against the site and its values against the proven intervals.
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        session_id: Hash64,
        leaf_index: u64,
        site: &PalwTirDissectSiteV1,
        root: &PalwTirRootClaimV1,
        arity: u8,
        opened_at_daa: u64,
        w_round: u64,
    ) -> Result<Self, PalwTirDissectError> {
        Self::open_parts(session_id, leaf_index, site, root.version, &root.elements, &root.totals, arity, opened_at_daa, w_round)
    }

    /// **[`Self::open`] from a root claim's parts** — its version, element lists and totals, whatever
    /// carriage its finalize rides in (RFC-0003's generative root claim opens the same phase).
    #[allow(clippy::too_many_arguments)]
    pub fn open_parts(
        session_id: Hash64,
        leaf_index: u64,
        site: &PalwTirDissectSiteV1,
        version: u16,
        elements: &[Vec<u32>],
        totals: &PalwTirRangeClaimV1,
        arity: u8,
        opened_at_daa: u64,
        w_round: u64,
    ) -> Result<Self, PalwTirDissectError> {
        if version != PALW_TIR_DISSECT_OBJECT_VERSION_V1 {
            return Err(PalwTirDissectError::UnsupportedVersion { got: version });
        }
        if !palw_attn_arity_is_legal_v1(arity) {
            return Err(PalwTirDissectError::Arity(format!("arity {arity} is not a power of two in 2..=64")));
        }
        if w_round == 0 {
            return Err(PalwTirDissectError::ZeroWindow);
        }
        if site.history_positions == 0 || site.tile_positions == 0 {
            return Err(PalwTirDissectError::NotDissected("an empty history"));
        }
        check_shape(site, elements, totals)?;
        let bounds: Vec<(i128, i128)> = site.bounds.iter().map(|b| (b.lo, b.hi)).collect();
        check_bounds(&bounds, totals)?;
        let tile_count = (site.history_positions as u64).div_ceil(site.tile_positions as u64);
        Ok(Self {
            session_id,
            arity,
            leaf_index,
            reductions: site.reductions.clone(),
            folds: site.folds.clone(),
            bounds,
            elements: elements.to_vec(),
            history_positions: site.history_positions,
            tile_positions: site.tile_positions,
            root: totals.clone(),
            claim: totals.clone(),
            tile_first: 0,
            tile_count,
            round: 0,
            turn: if tile_count <= 1 { PalwBisectTurnV1::Terminal } else { PalwBisectTurnV1::AwaitDisclosure },
            last_deadline_daa: opened_at_daa.saturating_add(w_round),
            pending: Vec::new(),
        })
    }

    /// A phase with every field derived from `session_id` alone — for tests that need a row to exist.
    #[cfg(test)]
    pub(crate) fn test_phase_v1(session_id: Hash64) -> Self {
        let claim = PalwTirRangeClaimV1 { partials: vec![vec![1]] };
        Self {
            session_id,
            arity: 2,
            leaf_index: 0,
            reductions: vec![1],
            folds: vec![PalwTirFoldV1::Sum],
            bounds: vec![(0, 9)],
            elements: vec![vec![0]],
            history_positions: 8,
            tile_positions: 2,
            root: claim.clone(),
            claim,
            tile_first: 0,
            tile_count: 4,
            round: 0,
            turn: PalwBisectTurnV1::AwaitDisclosure,
            last_deadline_daa: 9,
            pending: Vec::new(),
        }
    }

    pub fn session_id(&self) -> Hash64 {
        self.session_id
    }
    pub fn leaf_index(&self) -> u64 {
        self.leaf_index
    }
    pub fn turn(&self) -> PalwBisectTurnV1 {
        self.turn
    }
    pub fn round(&self) -> u32 {
        self.round
    }
    pub fn arity(&self) -> u8 {
        self.arity
    }
    pub fn last_deadline_daa(&self) -> u64 {
        self.last_deadline_daa
    }
    pub fn reductions(&self) -> &[u16] {
        &self.reductions
    }
    pub fn elements(&self) -> &[Vec<u32>] {
        &self.elements
    }
    pub fn root(&self) -> &PalwTirRangeClaimV1 {
        &self.root
    }
    pub fn claim(&self) -> &PalwTirRangeClaimV1 {
        &self.claim
    }
    pub fn pending(&self) -> &[PalwTirRangeClaimV1] {
        &self.pending
    }
    pub fn history_positions(&self) -> u32 {
        self.history_positions
    }

    /// The rounds this dissection may take (the pinned cut's own recurrence).
    pub fn round_budget(&self) -> u32 {
        palw_kary_rounds_v1(self.tile_count_at_open(), self.arity).unwrap_or(u32::MAX)
    }

    fn tile_count_at_open(&self) -> u64 {
        (self.history_positions as u64).div_ceil(self.tile_positions.max(1) as u64)
    }

    /// The pinned children of the disputed range, `(first_tile, tile_count)`.
    pub fn child_ranges(&self) -> Vec<(u64, u64)> {
        palw_attn_child_ranges_v1(self.tile_first, self.tile_count, self.arity).unwrap_or_default()
    }

    /// The history positions `[from, to)` the terminal tile covers (ragged at the history's end),
    /// once the phase is terminal on one tile.
    pub fn terminal_range(&self) -> Option<(usize, usize)> {
        if self.turn != PalwBisectTurnV1::Terminal || self.tile_count != 1 {
            return None;
        }
        let from = self.tile_first.checked_mul(self.tile_positions as u64)?;
        let to = (from + self.tile_positions as u64).min(self.history_positions as u64);
        (to > from).then_some((from as usize, to as usize))
    }

    /// Pull the rung's deadline inside the session's assembly reserve (the legacy phase's rule).
    pub fn cap_deadline_to_session_v1(&mut self, session_deadline_daa: u64, assembly_reserve_daa: u64) -> u64 {
        let cap = session_deadline_daa.saturating_sub(assembly_reserve_daa);
        if cap < self.last_deadline_daa {
            self.last_deadline_daa = cap;
        }
        self.last_deadline_daa
    }

    /// **The responder's round**: the children's claims, fold-checked and bound-checked before the
    /// challenger moves.
    pub fn apply_round(&mut self, msg: &PalwTirDissectRoundV1, accepted_daa: u64, w_round: u64) -> Result<(), PalwTirDissectError> {
        if msg.version != PALW_TIR_DISSECT_OBJECT_VERSION_V1 {
            return Err(PalwTirDissectError::UnsupportedVersion { got: msg.version });
        }
        match self.turn {
            PalwBisectTurnV1::AwaitDisclosure => {}
            PalwBisectTurnV1::AwaitVerdict => return Err(PalwTirDissectError::Turn("the challenger names a child")),
            PalwBisectTurnV1::Terminal | PalwBisectTurnV1::Abandoned => return Err(PalwTirDissectError::AlreadyTerminal),
        }
        if w_round == 0 {
            return Err(PalwTirDissectError::ZeroWindow);
        }
        let expected = self.child_ranges();
        if msg.children.len() != expected.len() {
            return Err(PalwTirDissectError::ChildCount { got: msg.children.len(), expected: expected.len() });
        }
        for child in &msg.children {
            if child.partials.len() != self.elements.len()
                || child.partials.iter().zip(&self.elements).any(|(v, e)| v.len() != e.len())
            {
                return Err(PalwTirDissectError::Shape("a child claims one value per demanded element"));
            }
            check_bounds(&self.bounds, child)?;
        }
        for (i, fold) in self.folds.iter().enumerate() {
            for e in 0..self.elements[i].len() {
                let mut values = msg.children.iter().map(|c| c.partials[i][e]);
                let folded = match fold {
                    PalwTirFoldV1::Sum => values.try_fold(0i128, |a, v| a.checked_add(v)),
                    PalwTirFoldV1::Max => values.max(),
                };
                let claimed = self.claim.partials[i][e];
                match folded {
                    Some(f) if f == claimed => {}
                    other => {
                        return Err(PalwTirDissectError::DoesNotFold {
                            reduction: i,
                            element: e,
                            claimed,
                            folded: other.unwrap_or(i128::MAX),
                        });
                    }
                }
            }
        }
        self.pending = msg.children.clone();
        self.last_deadline_daa = accepted_daa.saturating_add(w_round);
        self.turn = PalwBisectTurnV1::AwaitVerdict;
        Ok(())
    }

    /// **The challenger's move**: the named child's claim and range become the disputed ones; one
    /// tile is the bottom.
    pub fn apply_choice(&mut self, msg: &PalwTirDissectChoiceV1, accepted_daa: u64, w_round: u64) -> Result<(), PalwTirDissectError> {
        if msg.version != PALW_TIR_DISSECT_OBJECT_VERSION_V1 {
            return Err(PalwTirDissectError::UnsupportedVersion { got: msg.version });
        }
        if msg.session_id != self.session_id {
            return Err(PalwTirDissectError::Session);
        }
        match self.turn {
            PalwBisectTurnV1::AwaitVerdict => {}
            PalwBisectTurnV1::AwaitDisclosure => return Err(PalwTirDissectError::Turn("the responder discloses")),
            PalwBisectTurnV1::Terminal | PalwBisectTurnV1::Abandoned => return Err(PalwTirDissectError::AlreadyTerminal),
        }
        if msg.round != self.round {
            return Err(PalwTirDissectError::Round { got: msg.round, expected: self.round });
        }
        if w_round == 0 {
            return Err(PalwTirDissectError::ZeroWindow);
        }
        let bound = self.round_budget();
        if self.round + 1 > bound {
            return Err(PalwTirDissectError::RoundBudget { bound });
        }
        let ranges = self.child_ranges();
        let idx = msg.child as usize;
        let Some(&(first, count)) = ranges.get(idx) else {
            return Err(PalwTirDissectError::ChildOutOfRange { got: msg.child, children: ranges.len() });
        };
        let Some(claim) = self.pending.get(idx).cloned() else {
            return Err(PalwTirDissectError::Turn("a round before the choice"));
        };
        self.claim = claim;
        self.tile_first = first;
        self.tile_count = count;
        self.round += 1;
        self.pending = Vec::new();
        self.last_deadline_daa = accepted_daa.saturating_add(w_round);
        self.turn = if count <= 1 { PalwBisectTurnV1::Terminal } else { PalwBisectTurnV1::AwaitDisclosure };
        Ok(())
    }

    /// Silence past a move's deadline — the legacy phase's rule: at `Terminal` the move is a close,
    /// so nobody is charged there and the backstop ends the session.
    pub fn declare_no_show(&mut self, observed_daa: u64) -> Result<PalwBisectNoShowV1, PalwTirDissectError> {
        if observed_daa <= self.last_deadline_daa {
            return Err(PalwTirDissectError::DeadlineNotReached { deadline: self.last_deadline_daa, observed: observed_daa });
        }
        let silent_party = match self.turn {
            PalwBisectTurnV1::AwaitDisclosure => PalwBisectPartyV1::Responder,
            PalwBisectTurnV1::AwaitVerdict => PalwBisectPartyV1::Challenger,
            PalwBisectTurnV1::Terminal | PalwBisectTurnV1::Abandoned => return Err(PalwTirDissectError::AlreadyTerminal),
        };
        self.turn = PalwBisectTurnV1::Abandoned;
        Ok(PalwBisectNoShowV1 {
            version: crate::palw_bisect::PALW_BISECT_OBJECT_VERSION_V1,
            session_id: self.session_id,
            round: self.round,
            silent_party,
            deadline_daa: self.last_deadline_daa,
            observed_daa,
        })
    }
}

// =================================================================================================
// The messages the movers sign
// =================================================================================================

/// The domain the root claim's message opens with. The three moves are signed under the ADR-0082
/// contexts (the responder's for the root claim and the rounds, the challenger's for the choice);
/// the contexts are shared with the legacy moves, so each IR message opens with its own domain and
/// no signature over a legacy move is ever a signature over an IR one.
pub const PALW_TIR_DISSECT_DOMAIN_ROOT_V1: &[u8] = b"misaka-palw/tir/dissect/root/v1";
/// The domain a round's message opens with.
pub const PALW_TIR_DISSECT_DOMAIN_ROUND_V1: &[u8] = b"misaka-palw/tir/dissect/round/v1";
/// The domain a choice's message opens with.
pub const PALW_TIR_DISSECT_DOMAIN_CHOICE_V1: &[u8] = b"misaka-palw/tir/dissect/choice/v1";

/// **The message the responder signs for its root claim**: the domain, the session and the claim —
/// its version, element lists and totals — not the carriage (the carriage is checked against the
/// chain; the claim is the responder's word).
pub fn palw_tir_root_claim_message_v1(session_id: &Hash64, root: &PalwTirRootClaimV1) -> Vec<u8> {
    let mut m = PALW_TIR_DISSECT_DOMAIN_ROOT_V1.to_vec();
    m.extend_from_slice(session_id.as_byte_slice());
    m.extend(borsh::to_vec(&(root.version, &root.elements, &root.totals)).expect("borsh"));
    m
}

/// **The message the responder signs for a round**: the domain, the session, the phase's round
/// (little-endian `u32`, so a disclosure signed for one round is not a move at another) and the
/// round's encoding.
pub fn palw_tir_round_message_v1(session_id: &Hash64, round: u32, msg: &PalwTirDissectRoundV1) -> Vec<u8> {
    let mut m = PALW_TIR_DISSECT_DOMAIN_ROUND_V1.to_vec();
    m.extend_from_slice(session_id.as_byte_slice());
    m.extend(round.to_le_bytes());
    m.extend(borsh::to_vec(msg).expect("borsh"));
    m
}

/// **The message the challenger signs for a choice**: the domain and the choice's encoding (which
/// carries its session and round).
pub fn palw_tir_choice_message_v1(choice: &PalwTirDissectChoiceV1) -> Vec<u8> {
    let mut m = PALW_TIR_DISSECT_DOMAIN_CHOICE_V1.to_vec();
    m.extend(borsh::to_vec(choice).expect("borsh"));
    m
}
