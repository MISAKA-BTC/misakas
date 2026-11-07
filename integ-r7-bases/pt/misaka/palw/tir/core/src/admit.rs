//! **`tir_admit_v1` — admission of a PALW-TIR v1 class program** (spec 04b §5, §7, §8, §10.3).
//!
//! Admission is a pure function of the program bytes and a small set of network inputs (the tile
//! length, the canonical `H` chunk, the ceilings). It decides whether a program may be registered,
//! and it derives the numbers the court and the commitment layout are built from:
//!
//! 1. **the canonical form and the normal form** — `decode_canonical` (§4.4, NF-1..22);
//! 2. **types and shapes** — every node's type inferred and compared (NF-16);
//! 3. **ranges** — one interval per node (§7), which is also the domain every committed operand
//!    must lie in (PALW-TIR-33: step leaves, state checkpoint leaves, Hist rows, carry-ins);
//! 4. **costs** — the §8 cost vector of every node at the worst case `H = W`, per position (the sum
//!    over the schedule's occurrences), the state bytes and the peak live bytes (PALW-TIR-12);
//! 5. **court cones** — for every commit point its cone and leaves (§10.2), the cost of its worst
//!    TILE by the box-demand rules of §10.3, the reductions over `H` in it and, where there are
//!    any, the cost of one canonical `H` chunk (the dissection's bottom, PALW-TIR-32);
//! 6. **checkpoint intervals** — for every `Fixed` state the states its update reads (the replay
//!    closure), whether the replay splits into independent groups along axis 0, the replay cost per
//!    position of one group, and `C_j`, the longest replay the terminal ceiling admits (PALW-TIR-13);
//! 7. **ceilings** — each of the above against the network's caps, refused by name and number, and
//!    admission's own work (the cone work), refused at its cap before it is spent.
//!
//! Everything here is deterministic integer arithmetic over the program's structure; two
//! implementations of spec 04b compute the same admission, byte for byte.

use std::borrow::Cow;

use crate::error::{TirError, TirErrorKind, TirResult};
use crate::interval::{Interval, analyze_ranges};
use crate::prim::Prim;
use crate::program::{Ref, StateKind, TirProgramV1};
use crate::types::{DType, Dim, TensorType};
use crate::validate::{ProgramInfo, validate};

/// The network's caps admission checks against (spec 04b §10.3). The legacy court's terminal
/// ceiling is 16 Mi MACs per tile ([`TirCeilingsV1::legacy_court_v1`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TirCeilingsV1 {
    /// Multiply-accumulates one tile's cone may cost at the court (the terminal ceiling).
    pub max_tile_macs: u64,
    /// Transcendental evaluations one tile's cone may cost.
    pub max_tile_transcendentals: u64,
    /// Bytes of committed and artifact operands one tile's cone may open.
    pub max_tile_opened_bytes: u64,
    /// Distinct committed operand sources (commit points, carry-ins, states, histories) one cone reads.
    pub max_tile_operands: u64,
    /// Multiply-accumulates of one position (PALW-TIR-12).
    pub max_position_macs: u64,
    /// Transcendental evaluations of one position.
    pub max_position_transcendentals: u64,
    /// Bytes of `Fixed` and `Hist` state a run holds.
    pub max_state_bytes: u64,
    /// Step leaves (commit-point tiles) one position commits.
    pub max_step_leaves: u64,
    /// The longest checkpoint interval a class may use; `C_j` is capped here.
    pub max_checkpoint_interval: u32,
    /// Admission's own work (spec 04b §10.3): `Σ` over the commit points' cones and the
    /// `StateWrite`s' update cones of their nodes plus those nodes' operand refs. The normal form's
    /// caps alone allow ~2.7 M (~250 commit points per 512-node block, each reaching a 240-node
    /// chain, in 14 layer blocks); a Qwen2.5-1.5B-shaped decoder's is 905.
    pub max_cone_work: u64,
}

impl TirCeilingsV1 {
    /// The legacy court's terminal ceiling (`derive_court_cost_v1`: 16 Mi terminal MACs per tile)
    /// and caps no honest class of the corpus reaches otherwise.
    pub const fn legacy_court_v1() -> Self {
        Self {
            max_tile_macs: 16 << 20,
            max_tile_transcendentals: 1 << 20,
            max_tile_opened_bytes: 64 << 20,
            max_tile_operands: 64,
            max_position_macs: 1 << 40,
            max_position_transcendentals: 1 << 32,
            max_state_bytes: 1 << 36,
            max_step_leaves: 1 << 22,
            max_checkpoint_interval: 1 << 16,
            max_cone_work: 1 << 20,
        }
    }
}

/// Admission's network inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TirAdmitInputsV1 {
    /// Values per step leaf of every commit point: a commit point's value, flattened row-major at
    /// the running `H`, is committed in tiles of `tile_len` values (the last ragged). `1..=2^16`.
    pub tile_len: u32,
    /// Positions per canonical `H` chunk: the window's index space is cut at multiples of
    /// `h_chunk`, and one chunk is the bottom of a history dissection. A power of two, `1..=2^16`.
    pub h_chunk: u32,
    pub ceilings: TirCeilingsV1,
}

/// A §8 cost vector.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CostV1 {
    pub macs: u64,
    pub elementwise: u64,
    pub transcendentals: u64,
    pub bytes_read: u64,
    pub bytes_written: u64,
}

impl CostV1 {
    fn add(&mut self, o: &CostV1) {
        self.macs = self.macs.saturating_add(o.macs);
        self.elementwise = self.elementwise.saturating_add(o.elementwise);
        self.transcendentals = self.transcendentals.saturating_add(o.transcendentals);
        self.bytes_read = self.bytes_read.saturating_add(o.bytes_read);
        self.bytes_written = self.bytes_written.saturating_add(o.bytes_written);
    }
    fn div_ceil(&self, g: u64) -> CostV1 {
        let d = |v: u64| v.div_ceil(g.max(1));
        CostV1 {
            macs: d(self.macs),
            elementwise: d(self.elementwise),
            transcendentals: d(self.transcendentals),
            bytes_read: d(self.bytes_read),
            bytes_written: d(self.bytes_written),
        }
    }
}

/// What one position costs (spec 04b §8), at the worst case `H = W`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PositionV1 {
    /// The sum over the schedule's occurrences (`pre`, every layer, `post`).
    pub cost: CostV1,
    /// `Σ` Fixed `B(state)` and Hist `W · B(row)` over every state instance.
    pub state_bytes: u64,
    /// The largest block's peak of live output bytes.
    pub peak_live_bytes: u64,
    /// Commit-point values of one position (4-byte lanes).
    pub commit_lanes: u64,
    /// Step leaves of one position: `Σ ⌈lanes / tile_len⌉` over the commit points.
    pub step_leaves: u64,
}

/// A leaf of a cone (spec 04b §10.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LeafV1 {
    /// Another commit point of the occurrence (an opened step leaf).
    Commit(u16),
    CarryIn(u8),
    /// A `Fixed` state's value at the start of the position (a checkpoint leaf, advanced by replay).
    State(u16),
    /// A `Hist` state's prior rows.
    History(u16),
    Param(u16),
    Const(u16),
    Input(u8),
}

impl LeafV1 {
    /// Whether the leaf is a committed operand (counted against `max_tile_operands`).
    pub fn is_committed(&self) -> bool {
        matches!(self, LeafV1::Commit(_) | LeafV1::CarryIn(_) | LeafV1::State(_) | LeafV1::History(_))
    }
}

/// How a param leaf is opened at the court (spec 04b §10.3, §15.5). Every version-1 param is an
/// artifact tensor. A version-2 program's inputs are params of its view (§15.3), and each is opened
/// as what it is: an earlier stage's committed output, job data, or a value the court derives itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ParamLeafV1 {
    /// An artifact tensor: opened at its dtype's width against the artifact root; not a committed
    /// operand.
    Artifact,
    /// A committed value of an earlier stage: 4 bytes a lane, and a committed operand.
    Committed,
    /// Job data (a job scalar, the job's token ids): 4 bytes a lane; not a committed operand.
    JobData,
    /// Derived by the court from the job and the step space (a random input, a row or token count):
    /// nothing opened.
    Derived,
    /// A job image (RFC-0003 §II.4): 1 byte a lane — a `u8` pixel — opened by tiles against the
    /// image's `input_root`, and so an operand of its own.
    JobImage,
}

impl ParamLeafV1 {
    /// Bytes one element of the leaf opens.
    pub fn width(self, dtype: DType) -> u64 {
        match self {
            ParamLeafV1::Artifact => dtype.width() as u64,
            ParamLeafV1::Committed | ParamLeafV1::JobData => 4,
            ParamLeafV1::Derived => 0,
            ParamLeafV1::JobImage => 1,
        }
    }

    /// Is the leaf a rooted source a cone's close opens tiles of with their paths (a committed
    /// value, a job image), and so one of a cone's operands?
    pub fn is_operand(self) -> bool {
        matches!(self, ParamLeafV1::Committed | ParamLeafV1::JobImage)
    }
}

/// One commit point's court cone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConeV1 {
    pub block: u8,
    pub node: u16,
    /// The cone's nodes, ascending (the commit point last).
    pub nodes: Vec<u16>,
    /// Its distinct leaves, sorted.
    pub leaves: Vec<LeafV1>,
    /// Every cone node evaluated whole, at `H = W`.
    pub whole: CostV1,
    /// Tiles the commit point has at `H = W`.
    pub tiles: u64,
    /// The worst tile (`tile_len` values), by the box-demand rules, at `H = W`.
    pub tile: CostV1,
    /// Bytes of operands (committed lanes at 4 bytes, params and consts at their width) the worst
    /// tile opens.
    pub tile_opened_bytes: u64,
    /// Committed operand sources the cone reads.
    pub operands: u64,
    /// The cone's reductions over `H` (`ReduceSum`/`ReduceMax` along `H`, `MatMul` contracting `H`).
    pub h_reductions: Vec<u16>,
    /// With `h_reductions`, the worst tile at `H = h_chunk`: one canonical chunk, the bottom of
    /// the court's history dissection (the claimed partial results above it are checked by folds).
    pub chunk: Option<CostV1>,
    /// With `h_reductions`, the bytes that chunk opens.
    pub chunk_opened_bytes: Option<u64>,
}

impl ConeV1 {
    /// The cost the court's terminal step pays for this commit point: one chunk for a dissected
    /// cone, one tile otherwise.
    pub fn terminal(&self) -> &CostV1 {
        self.chunk.as_ref().unwrap_or(&self.tile)
    }

    /// The bytes the court's terminal step opens: one chunk's for a dissected cone, one tile's
    /// otherwise.
    pub fn terminal_opened_bytes(&self) -> u64 {
        self.chunk_opened_bytes.unwrap_or(self.tile_opened_bytes)
    }
}

/// One `Fixed` state's checkpoint interval.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateCkptV1 {
    pub state: u16,
    /// The states replayed with it — its update reads them (itself included), sorted.
    pub closure: Vec<u16>,
    /// Independent groups along axis 0 (1 when the replay is not provably split).
    pub groups: u64,
    /// One group's replay of one position: over the closure's update cones (a node two cones share
    /// counted in each), the aligned nodes' cost divided among the groups (`⌈a / G⌉`) plus every
    /// free node's whole; unsplit, the whole sum.
    pub per_position: CostV1,
    /// `C_j`: the most positions one group's replay fits the terminal ceiling for (capped).
    pub interval: u32,
}

/// Everything admission derived.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirAdmissionV1 {
    pub program: TirProgramV1,
    pub info: ProgramInfo,
    /// Per block, per node: the proven interval — the domain of every committed value
    /// (PALW-TIR-33).
    pub intervals: Vec<Vec<Interval>>,
    /// Per block, per node: its §8 cost at `H = W`.
    pub node_costs: Vec<Vec<CostV1>>,
    pub position: PositionV1,
    pub cones: Vec<ConeV1>,
    pub states: Vec<StateCkptV1>,
    /// `min_j C_j` (the ceiling's cap when the program has no `Fixed` state).
    pub checkpoint_interval: u32,
    /// Admission's own work, as capped by [`TirCeilingsV1::max_cone_work`].
    pub cone_work: u64,
}

/// Why admission refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TirAdmitError {
    /// The bytes, the normal form, the types or the ranges (the class of spec 04b §9.3).
    Program(TirError),
    /// A derived quantity exceeds its ceiling.
    Exceeds { limit: &'static str, at: String, value: u64, cap: u64 },
    /// The network inputs are out of range.
    Inputs(&'static str),
}

impl From<TirError> for TirAdmitError {
    fn from(e: TirError) -> Self {
        TirAdmitError::Program(e)
    }
}

impl std::fmt::Display for TirAdmitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TirAdmitError::Program(e) => write!(f, "{e}"),
            TirAdmitError::Exceeds { limit, at, value, cap } => write!(f, "{limit} {value} exceeds {cap} at {at}"),
            TirAdmitError::Inputs(m) => write!(f, "admission inputs: {m}"),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Shapes and element counts
// ---------------------------------------------------------------------------------------------

/// The type of an operand as seen by the node reading it (borrowed for nodes and carry-ins, the
/// operands admission visits most).
fn ref_type(p: &TirProgramV1, block: usize, r: Ref) -> Cow<'_, TensorType> {
    let b = &p.blocks[block];
    match r {
        Ref::Node(j) => Cow::Borrowed(&b.nodes[j as usize].out),
        Ref::CarryIn(k) => Cow::Borrowed(&b.carry_in[k as usize]),
        Ref::Param(j) => Cow::Owned(TensorType::fixed(p.params[j as usize].dtype, &p.params[j as usize].shape)),
        Ref::Const(j) => Cow::Owned(TensorType::fixed(p.consts[j as usize].dtype, &p.consts[j as usize].shape)),
        Ref::State(j) => Cow::Owned(TensorType::fixed(p.states[j as usize].dtype, &p.states[j as usize].shape)),
        Ref::Input(_) => Cow::Owned(TensorType::scalar(DType::Idx)),
    }
}

fn elems(t: &TensorType, h: u64) -> u64 {
    t.elements_at(h)
}

fn bytes(t: &TensorType, h: u64) -> u64 {
    t.elements_at(h).saturating_mul(t.dtype.width() as u64)
}

fn dim_at(d: Dim, h: u64) -> u64 {
    match d {
        Dim::Fixed(n) => n as u64,
        Dim::H => h,
    }
}

/// The contraction extent of a `MatMul` node: `a.shape[−1]`.
fn contraction(p: &TirProgramV1, block: usize, node: usize, h: u64) -> u64 {
    let n = &p.blocks[block].nodes[node];
    let a = ref_type(p, block, n.inputs[0]);
    dim_at(a.shape[a.rank() - 1], h)
}

// ---------------------------------------------------------------------------------------------
// §8: the cost of a node
// ---------------------------------------------------------------------------------------------

/// A node's §8 cost at `H = h`.
pub fn node_cost(p: &TirProgramV1, block: usize, node: usize, h: u64) -> CostV1 {
    let n = &p.blocks[block].nodes[node];
    let out_e = elems(&n.out, h);
    let out_b = bytes(&n.out, h);
    let ins: Vec<Cow<'_, TensorType>> = n.inputs.iter().map(|r| ref_type(p, block, *r)).collect();
    let in_b: u64 = ins.iter().map(|t| bytes(t, h)).fold(0u64, u64::saturating_add);
    let mut c = CostV1 { bytes_written: out_b, ..Default::default() };
    match &n.prim {
        Prim::Iota { .. } => c.elementwise = out_e,
        Prim::Gather { .. } => {
            c.elementwise = out_e;
            c.bytes_read = out_e.saturating_mul(ins[0].dtype.width() as u64).saturating_add(bytes(&ins[1], h));
        }
        Prim::MatMul => {
            c.macs = out_e.saturating_mul(contraction(p, block, node, h));
            c.bytes_read = in_b;
        }
        Prim::ReduceSum { .. } | Prim::ReduceMax { .. } => {
            c.elementwise = elems(&ins[0], h);
            c.bytes_read = in_b;
        }
        Prim::IntExp | Prim::IntRsqrt | Prim::IntLn => {
            c.transcendentals = out_e;
            c.bytes_read = in_b;
        }
        Prim::TopK { k, .. } => {
            c.elementwise = elems(&ins[0], h).saturating_mul(*k as u64);
            c.bytes_read = in_b;
        }
        Prim::HistAppend { .. } => {
            c.elementwise = out_e;
            c.bytes_read = out_b;
            c.bytes_written = bytes(&ins[0], h);
        }
        _ => {
            c.elementwise = out_e;
            c.bytes_read = in_b;
        }
    }
    c
}

// ---------------------------------------------------------------------------------------------
// §10.2–10.3: cones and box demand
// ---------------------------------------------------------------------------------------------

/// The backward closure of `root` through `Node` refs that stops at every commit point other than
/// `root` (spec 04b §10.2): the cone's nodes, ascending.
pub fn cone_nodes(p: &TirProgramV1, block: usize, root: usize) -> Vec<u16> {
    let b = &p.blocks[block];
    let mut seen = vec![false; b.nodes.len()];
    let mut stack = vec![root];
    while let Some(i) = stack.pop() {
        if std::mem::replace(&mut seen[i], true) {
            continue;
        }
        if i != root && b.nodes[i].commit {
            continue;
        }
        for r in &b.nodes[i].inputs {
            if let Ref::Node(j) = r {
                stack.push(*j as usize);
            }
        }
    }
    (0..b.nodes.len()).filter(|i| seen[*i] && (*i == root || !b.nodes[*i].commit)).map(|i| i as u16).collect()
}

/// A leaf of the cone read by `r` (spec 04b §10.2), if `r` is not a cone node.
fn leaf_of(cone: &[bool], r: Ref) -> Option<LeafV1> {
    match r {
        Ref::Node(j) if cone[j as usize] => None,
        Ref::Node(j) => Some(LeafV1::Commit(j)),
        Ref::CarryIn(k) => Some(LeafV1::CarryIn(k)),
        Ref::Param(j) => Some(LeafV1::Param(j)),
        Ref::Const(j) => Some(LeafV1::Const(j)),
        Ref::State(j) => Some(LeafV1::State(j)),
        Ref::Input(j) => Some(LeafV1::Input(j)),
    }
}

/// **Which box-demand rules a sizing reads** (spec 04b §10.3): the DAA-2,000 release's, or those of
/// testnet-12's second IR fence (`Params::palw_tir_fence2`), whose one change is ref2's H7 `TopK`
/// row. A consensus parameter: admission v10 and the value bound `V` are asked under the
/// registering block's rules.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TirDemandRulesV1 {
    /// `⌈d / k⌉ · x.shape[axis]` for a `TopK` — the DAA-2,000 release's row.
    #[default]
    Release2000,
    /// **ref2's H7**: the TopK rows a run of `d` consecutive demanded elements can touch — `min(R, d)`
    /// along an axis that is not the innermost (consecutive indices lie in different rows),
    /// `min(R, ⌈d / k⌉ + 1)` along the innermost (an unaligned run touches one row more), `R` the
    /// rows — times `x.shape[axis]`.
    H7,
}

impl TirDemandRulesV1 {
    /// **The demand a `TopK` passes to its operand** when `d` of its output elements are demanded:
    /// `in_shape` is the operand's shape at the sizing's `H`, `axis` the TopK's axis, `k` its count.
    /// Capped at the operand's element count; H7 never counts less than the release's row.
    pub fn topk_operand_demand(self, d: u64, k: u64, axis: usize, in_shape: &[u64]) -> u64 {
        let elements = in_shape.iter().fold(1u64, |acc, x| acc.saturating_mul(*x));
        let along = in_shape.get(axis).copied().unwrap_or(1).max(1);
        let raw = match self {
            Self::Release2000 => d.div_ceil(k.max(1)).saturating_mul(along),
            Self::H7 => {
                // A TopK row is one index of every axis but `axis`: `elements / along` of them.
                let rows = elements / along;
                let innermost = axis + 1 == in_shape.len();
                let touched = if innermost { rows.min(d.div_ceil(k.max(1)).saturating_add(1)) } else { rows.min(d) };
                touched.saturating_mul(along)
            }
        };
        raw.min(elements)
    }
}

/// The demand a node's operand receives when `d` of the node's output elements are demanded
/// (spec 04b §10.3's box-demand rules, under `rules`). `e_in` is the operand's element count.
fn operand_demand(p: &TirProgramV1, block: usize, node: usize, input: usize, d: u64, h: u64, rules: TirDemandRulesV1) -> u64 {
    let n = &p.blocks[block].nodes[node];
    let t = ref_type(p, block, n.inputs[input]);
    let e_in = elems(&t, h);
    let raw = match &n.prim {
        Prim::MatMul => d.saturating_mul(contraction(p, block, node, h)),
        Prim::ReduceSum { axis } | Prim::ReduceMax { axis } => d.saturating_mul(dim_at(t.shape[*axis as usize], h)),
        Prim::TopK { axis, k } => {
            let shape: Vec<u64> = t.shape.iter().map(|x| dim_at(*x, h)).collect();
            rules.topk_operand_demand(d, *k as u64, *axis as usize, &shape)
        }
        _ => d,
    };
    raw.min(e_in)
}

/// The cost of evaluating `d` demanded elements of a node, given its operands' demands.
fn demanded_cost(p: &TirProgramV1, block: usize, node: usize, d: u64, ins: &[u64], h: u64) -> CostV1 {
    let n = &p.blocks[block].nodes[node];
    let w_out = n.out.dtype.width() as u64;
    let width = |i: usize| ref_type(p, block, n.inputs[i]).dtype.width() as u64;
    let read: u64 = ins.iter().enumerate().map(|(i, e)| e.saturating_mul(width(i))).fold(0, u64::saturating_add);
    let mut c = CostV1 { bytes_written: d.saturating_mul(w_out), bytes_read: read, ..Default::default() };
    match &n.prim {
        Prim::MatMul => c.macs = d.saturating_mul(contraction(p, block, node, h)),
        Prim::ReduceSum { .. } | Prim::ReduceMax { .. } => c.elementwise = ins[0],
        Prim::TopK { k, .. } => c.elementwise = ins[0].saturating_mul(*k as u64),
        Prim::IntExp | Prim::IntRsqrt | Prim::IntLn => c.transcendentals = d,
        _ => c.elementwise = d,
    }
    c
}

/// The box demand of one tile of `root` at `H = h`: `(cost, opened bytes, leaves with positive
/// demand)`.
#[allow(clippy::too_many_arguments)]
fn tile_demand(
    p: &TirProgramV1,
    block: usize,
    root: usize,
    cone: &[u16],
    tile_len: u64,
    h: u64,
    param_leaf: &dyn Fn(u16) -> ParamLeafV1,
    rules: TirDemandRulesV1,
) -> (CostV1, u64, Vec<LeafV1>) {
    let b = &p.blocks[block];
    let mut in_cone = vec![false; b.nodes.len()];
    for i in cone {
        in_cone[*i as usize] = true;
    }
    let mut demand = vec![0u64; b.nodes.len()];
    demand[root] = tile_len.min(elems(&b.nodes[root].out, h));
    let mut leaf_demand: std::collections::BTreeMap<LeafV1, u64> = std::collections::BTreeMap::new();
    let mut cost = CostV1::default();
    for &i in cone.iter().rev() {
        let i = i as usize;
        let d = demand[i];
        if d == 0 {
            continue;
        }
        let n = &b.nodes[i];
        let mut ins = Vec::with_capacity(n.inputs.len());
        for (k, r) in n.inputs.iter().enumerate() {
            let delta = operand_demand(p, block, i, k, d, h, rules);
            ins.push(delta);
            match leaf_of(&in_cone, *r) {
                None => {
                    let Ref::Node(j) = r else { unreachable!() };
                    let j = *j as usize;
                    demand[j] = demand[j].saturating_add(delta).min(elems(&b.nodes[j].out, h));
                }
                Some(leaf) => {
                    let cap = elems(&ref_type(p, block, *r), h);
                    let e = leaf_demand.entry(leaf).or_insert(0);
                    *e = e.saturating_add(delta).min(cap);
                }
            }
        }
        if let Prim::HistAppend { state } = n.prim {
            // The prior rows: every demanded element that is not this position's row.
            let row = elems(&ref_type(p, block, n.inputs[0]), h);
            let prior = elems(&n.out, h).saturating_sub(row);
            let e = leaf_demand.entry(LeafV1::History(state)).or_insert(0);
            *e = e.saturating_add(d.min(prior)).min(prior);
        }
        cost.add(&demanded_cost(p, block, i, d, &ins, h));
    }
    let mut opened = 0u64;
    let mut leaves = Vec::new();
    for (leaf, e) in &leaf_demand {
        if *e == 0 {
            continue;
        }
        leaves.push(*leaf);
        let width = match leaf {
            LeafV1::Commit(_) | LeafV1::CarryIn(_) | LeafV1::State(_) | LeafV1::History(_) => 4,
            LeafV1::Param(j) => param_leaf(*j).width(p.params[*j as usize].dtype),
            LeafV1::Const(j) => p.consts[*j as usize].dtype.width() as u64,
            LeafV1::Input(_) => 4,
        };
        opened = opened.saturating_add(e.saturating_mul(width));
    }
    (cost, opened, leaves)
}

/// Whether a node reduces over `H`.
fn reduces_h(p: &TirProgramV1, block: usize, node: usize) -> bool {
    let n = &p.blocks[block].nodes[node];
    match &n.prim {
        Prim::ReduceSum { axis } | Prim::ReduceMax { axis } => {
            let t = ref_type(p, block, n.inputs[0]);
            t.shape[*axis as usize].is_h()
        }
        Prim::MatMul => {
            let a = ref_type(p, block, n.inputs[0]);
            a.shape[a.rank() - 1].is_h()
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------------------------
// §10.3: state replay groups
// ---------------------------------------------------------------------------------------------

/// Group-locality along axis 0 (spec 04b §10.3): is every node of the replay's cones "aligned"
/// with `g` groups — its output's axis 0 has extent `g` and element `[i, …]` depends on the
/// replayed states only through their elements `[i, …]`? `Free` is independent of every replayed
/// state.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Locality {
    Free,
    Aligned,
    Mixed,
}

/// `(G, every node's locality)`: the groups a replay of `closure` splits into (1 when it does not)
/// and, over the union of the closure's update cones, which nodes are aligned and which free — the
/// two sums the per-group cost is made of (spec 04b §10.3).
fn replay_groups(p: &TirProgramV1, block: usize, closure: &[u16], cones: &[(usize, Vec<u16>)]) -> (u64, Vec<Locality>) {
    let b = &p.blocks[block];
    let mut loc = vec![Locality::Free; b.nodes.len()];
    let g = match p.states[closure[0] as usize].shape.first() {
        Some(g) if *g > 1 => *g as u64,
        _ => return (1, loc),
    };
    if closure.iter().any(|s| p.states[*s as usize].shape.first().map(|d| *d as u64) != Some(g)) {
        return (1, loc);
    }
    let lead = |t: &TensorType| t.shape.first().copied() == Some(Dim::Fixed(g as u32));
    let mut nodes: Vec<usize> = cones.iter().flat_map(|(_, c)| c.iter().map(|i| *i as usize)).collect();
    nodes.sort_unstable();
    nodes.dedup();
    for &i in &nodes {
        let n = &b.nodes[i];
        let out_rank = n.out.rank();
        // A closure state is aligned; a COMMIT POINT is a free leaf — the court opens it at every
        // replayed position, whatever node it is (another member's committed `StateWrite` too);
        // every other node an update cone reads is in that cone (a cone stops only at commit
        // points), so it has been classified already.
        let operand = |r: Ref| -> (Locality, Cow<'_, TensorType>) {
            let t = ref_type(p, block, r);
            let l = match r {
                Ref::State(s) if closure.contains(&s) => Locality::Aligned,
                Ref::Node(j) if b.nodes[j as usize].commit => Locality::Free,
                Ref::Node(j) => loc[j as usize],
                _ => Locality::Free,
            };
            (l, t)
        };
        let ops: Vec<(Locality, Cow<'_, TensorType>)> = n.inputs.iter().map(|r| operand(*r)).collect();
        if ops.iter().all(|(l, _)| *l == Locality::Free) {
            loc[i] = Locality::Free;
            continue;
        }
        if ops.iter().any(|(l, _)| *l == Locality::Mixed) || !lead(&n.out) {
            loc[i] = Locality::Mixed;
            continue;
        }
        // Every non-free operand must carry the groups on ITS axis 0, and that axis must be the
        // output's axis 0.
        let aligned_operand = |(l, t): &(Locality, Cow<'_, TensorType>)| *l == Locality::Free || (lead(t) && t.rank() == out_rank);
        let ok = match &n.prim {
            Prim::Transpose { perm } => perm.first() == Some(&0) && aligned_operand(&ops[0]),
            Prim::Slice { axis, .. } | Prim::Concat { axis } => *axis != 0 && ops.iter().all(aligned_operand),
            Prim::ReduceSum { axis } | Prim::ReduceMax { axis } | Prim::TopK { axis, .. } => *axis != 0 && aligned_operand(&ops[0]),
            Prim::Reshape => lead(&ops[0].1),
            Prim::Gather { axis, .. } => *axis != 0 && ops[1].0 == Locality::Free && aligned_operand(&ops[0]),
            Prim::MatMul => {
                // A batched product keeps the groups on the batch axis; a rank-2 product keeps them
                // on the rows of `a` when `b` is free of them.
                if out_rank >= 3 { ops.iter().all(aligned_operand) } else { ops[1].0 == Locality::Free && aligned_operand(&ops[0]) }
            }
            Prim::HistAppend { .. } | Prim::Iota { .. } => false,
            _ => ops.iter().all(aligned_operand),
        };
        loc[i] = if ok { Locality::Aligned } else { Locality::Mixed };
    }
    // A free node never blocks the split — it reads no closure state, so every group can compute it
    // from what the court opens — and a mixed one always does.
    let split = nodes.iter().all(|i| loc[*i] != Locality::Mixed);
    (if split { g } else { 1 }, loc)
}

// ---------------------------------------------------------------------------------------------
// The admission
// ---------------------------------------------------------------------------------------------

fn exceeds(limit: &'static str, at: impl Into<String>, value: u64, cap: u64) -> Result<(), TirAdmitError> {
    if value > cap { Err(TirAdmitError::Exceeds { limit, at: at.into(), value, cap }) } else { Ok(()) }
}

/// **`tir_admit_v1`**: admit the canonical bytes of a program, or refuse them by rule and number —
/// under the DAA-2,000 release's box-demand rules ([`tir_admit_with_rules_v1`]).
pub fn tir_admit_v1(program_bytes: &[u8], inputs: &TirAdmitInputsV1) -> Result<TirAdmissionV1, TirAdmitError> {
    tir_admit_with_rules_v1(program_bytes, inputs, TirDemandRulesV1::Release2000)
}

/// [`tir_admit_v1`] under the box-demand rules `rules` — the registering block's.
pub fn tir_admit_with_rules_v1(
    program_bytes: &[u8],
    inputs: &TirAdmitInputsV1,
    rules: TirDemandRulesV1,
) -> Result<TirAdmissionV1, TirAdmitError> {
    check_admit_inputs(inputs)?;
    let p = TirProgramV1::decode_canonical(program_bytes)?;
    let info = validate(&p)?;
    let intervals = analyze_ranges(&p)?;
    admit_core(p, info, intervals, inputs, &|_| ParamLeafV1::Artifact, rules)
}

/// The network inputs' own rules (spec 04b §10.3), checked before anything else.
pub(crate) fn check_admit_inputs(inputs: &TirAdmitInputsV1) -> Result<(), TirAdmitError> {
    if !(1..=1 << 16).contains(&inputs.tile_len) {
        return Err(TirAdmitError::Inputs("tile_len outside [1, 2^16]"));
    }
    if !(1..=1 << 16).contains(&inputs.h_chunk) || !inputs.h_chunk.is_power_of_two() {
        return Err(TirAdmitError::Inputs("h_chunk is not a power of two in [1, 2^16]"));
    }
    if inputs.ceilings.max_checkpoint_interval == 0 {
        return Err(TirAdmitError::Inputs("max_checkpoint_interval is 0"));
    }
    Ok(())
}

/// **Admission's analyses** (spec 04b §8, §10.2, §10.3) of a validated program with its proven
/// intervals: costs, per-position quantities, cones and checkpoint intervals, each against its
/// ceiling. `param_leaf` says how each param leaf is opened — every version-1 param is
/// [`ParamLeafV1::Artifact`]; a version-2 program passes its view and its inputs' kinds (§15.5).
/// `rules` are the box-demand rules the sizing reads (the registering block's).
pub(crate) fn admit_core(
    p: TirProgramV1,
    info: ProgramInfo,
    intervals: Vec<Vec<Interval>>,
    inputs: &TirAdmitInputsV1,
    param_leaf: &dyn Fn(u16) -> ParamLeafV1,
    rules: TirDemandRulesV1,
) -> Result<TirAdmissionV1, TirAdmitError> {
    let ceil = &inputs.ceilings;
    let tile_len = inputs.tile_len as u64;
    let window = |bi: usize| info.blocks[bi].window.unwrap_or(1) as u64;

    // §8, per node at the worst case.
    let node_costs: Vec<Vec<CostV1>> =
        (0..p.blocks.len()).map(|bi| (0..p.blocks[bi].nodes.len()).map(|ni| node_cost(&p, bi, ni, window(bi))).collect()).collect();

    // Per position: the schedule's occurrences.
    let mut position = PositionV1::default();
    let block_cost: Vec<CostV1> = node_costs
        .iter()
        .map(|cs| {
            let mut c = CostV1::default();
            cs.iter().for_each(|x| c.add(x));
            c
        })
        .collect();
    // Per block: its commit lanes and step leaves (commit-point tiles).
    let block_leaves: Vec<(u64, u64)> = (0..p.blocks.len())
        .map(|bi| {
            p.blocks[bi].nodes.iter().filter(|n| n.commit).fold((0u64, 0u64), |(l, t), n| {
                let lanes = elems(&n.out, window(bi));
                (l.saturating_add(lanes), t.saturating_add(lanes.div_ceil(tile_len)))
            })
        })
        .collect();
    for (block, _) in p.occurrences() {
        let bi = block as usize;
        position.cost.add(&block_cost[bi]);
        position.commit_lanes = position.commit_lanes.saturating_add(block_leaves[bi].0);
        position.step_leaves = position.step_leaves.saturating_add(block_leaves[bi].1);
    }
    // State bytes: every instance a run holds. `uses[b][j]`: block `b` reads, writes or appends
    // state `j`.
    let mut uses = vec![vec![false; p.states.len()]; p.blocks.len()];
    for (bi, b) in p.blocks.iter().enumerate() {
        for n in &b.nodes {
            for r in &n.inputs {
                if let Ref::State(j) = r {
                    uses[bi][*j as usize] = true;
                }
            }
            if let Prim::StateWrite { state } | Prim::HistAppend { state } = n.prim {
                uses[bi][state as usize] = true;
            }
        }
    }
    for (j, s) in p.states.iter().enumerate() {
        let instances = if s.per_layer {
            p.schedule.layers.iter().filter(|b| uses[**b as usize][j]).count() as u64
        } else {
            u64::from(uses.iter().any(|u| u[j]))
        };
        let row = TensorType::fixed(s.dtype, &s.shape);
        let per = match s.kind {
            StateKind::Fixed { .. } => bytes(&row, 1),
            StateKind::Hist { window } => bytes(&row, 1).saturating_mul(window as u64),
        };
        position.state_bytes = position.state_bytes.saturating_add(per.saturating_mul(instances));
    }
    // Peak live bytes: per block, an output lives from its node to its last consumer in the block,
    // or to the block's end if it is a root.
    for (bi, b) in p.blocks.iter().enumerate() {
        let n = b.nodes.len();
        let mut last = vec![0usize; n];
        for (i, node) in b.nodes.iter().enumerate() {
            last[i] = i;
            let root = node.commit
                || matches!(node.prim, Prim::StateWrite { .. } | Prim::HistAppend { .. })
                || b.carry_out.contains(&(i as u16))
                || (info.blocks[bi].is_post && i == p.logits as usize);
            if root {
                last[i] = n - 1;
            }
        }
        for (i, node) in b.nodes.iter().enumerate() {
            for r in &node.inputs {
                if let Ref::Node(j) = r {
                    last[*j as usize] = last[*j as usize].max(i);
                }
            }
        }
        // Live bytes at `t` = Σ B(out_i) over `i ≤ t ≤ last[i]`: a running sum of births and deaths
        // (in u128, so no intermediate saturates; the result is capped like every other sum).
        let mut dies = vec![0u128; n + 1];
        let (mut live, mut peak) = (0u128, 0u128);
        for t in 0..n {
            let born = bytes(&b.nodes[t].out, window(bi)) as u128;
            live += born;
            dies[last[t] + 1] += born;
            peak = peak.max(live);
            live -= dies[t + 1];
        }
        position.peak_live_bytes = position.peak_live_bytes.max(u64::try_from(peak).unwrap_or(u64::MAX));
    }
    exceeds("max_position_macs", "the position", position.cost.macs, ceil.max_position_macs)?;
    exceeds("max_position_transcendentals", "the position", position.cost.transcendentals, ceil.max_position_transcendentals)?;
    exceeds("max_state_bytes", "the run", position.state_bytes, ceil.max_state_bytes)?;
    exceeds("max_step_leaves", "the position", position.step_leaves, ceil.max_step_leaves)?;

    // §10.2–10.3: every commit point's cone. Admission's own work is counted as it goes and
    // refused at its ceiling before the cone is costed, so no program costs more than the cap.
    let mut work = 0u64;
    let mut count_work = |nodes: &[u16], bi: usize, at: &dyn Fn() -> String| -> Result<(), TirAdmitError> {
        let refs: u64 = nodes.iter().map(|i| p.blocks[bi].nodes[*i as usize].inputs.len() as u64).sum();
        work = work.saturating_add(nodes.len() as u64).saturating_add(refs);
        exceeds("max_cone_work", at(), work, ceil.max_cone_work)
    };
    let mut cones = Vec::new();
    for (bi, b) in p.blocks.iter().enumerate() {
        let h = window(bi);
        for (ni, n) in b.nodes.iter().enumerate() {
            if !n.commit {
                continue;
            }
            let nodes = cone_nodes(&p, bi, ni);
            count_work(&nodes, bi, &|| format!("block {bi} commit point {ni}"))?;
            let mut in_cone = vec![false; b.nodes.len()];
            nodes.iter().for_each(|i| in_cone[*i as usize] = true);
            let mut leaves: Vec<LeafV1> = Vec::new();
            let mut whole = CostV1::default();
            let mut h_reductions = Vec::new();
            for &i in &nodes {
                whole.add(&node_costs[bi][i as usize]);
                let node = &b.nodes[i as usize];
                for r in &node.inputs {
                    if let Some(l) = leaf_of(&in_cone, *r) {
                        leaves.push(l);
                    }
                }
                if let Prim::HistAppend { state } = node.prim {
                    leaves.push(LeafV1::History(state));
                }
                if reduces_h(&p, bi, i as usize) {
                    h_reductions.push(i);
                }
            }
            leaves.sort_unstable();
            leaves.dedup();
            let (tile, tile_opened_bytes, _) = tile_demand(&p, bi, ni, &nodes, tile_len, h, param_leaf, rules);
            let chunked = (!h_reductions.is_empty())
                .then(|| tile_demand(&p, bi, ni, &nodes, tile_len, (inputs.h_chunk as u64).min(h), param_leaf, rules));
            let (chunk, chunk_opened_bytes) = match chunked {
                Some((c, o, _)) => (Some(c), Some(o)),
                None => (None, None),
            };
            let operands =
                leaves.iter().filter(|l| l.is_committed() || matches!(l, LeafV1::Param(j) if param_leaf(*j).is_operand())).count()
                    as u64;
            let cone = ConeV1 {
                block: bi as u8,
                node: ni as u16,
                nodes,
                leaves,
                whole,
                tiles: elems(&n.out, h).div_ceil(tile_len),
                tile,
                tile_opened_bytes,
                operands,
                h_reductions,
                chunk,
                chunk_opened_bytes,
            };
            let at = format!("block {bi} commit point {ni}");
            let t = *cone.terminal();
            exceeds("max_tile_macs", at.clone(), t.macs, ceil.max_tile_macs)?;
            exceeds("max_tile_transcendentals", at.clone(), t.transcendentals, ceil.max_tile_transcendentals)?;
            exceeds("max_tile_opened_bytes", at.clone(), cone.terminal_opened_bytes(), ceil.max_tile_opened_bytes)?;
            exceeds("max_tile_operands", at, cone.operands, ceil.max_tile_operands)?;
            cones.push(cone);
        }
    }

    // §10.3: every Fixed state's checkpoint interval. `updates[b][j]`: the update cone of state `j`
    // in block `b`, if `b` writes it (NF-19: at most once); `reads[b][j]`: the states (`|states|
    // ≤ 64`, NF-2: a bit each) that cone reads.
    let mut updates: Vec<Vec<Option<(usize, Vec<u16>)>>> = vec![vec![None; p.states.len()]; p.blocks.len()];
    let mut reads = vec![vec![0u64; p.states.len()]; p.blocks.len()];
    for (bi, b) in p.blocks.iter().enumerate() {
        for (w, n) in b.nodes.iter().enumerate() {
            let Prim::StateWrite { state } = n.prim else { continue };
            let cone = cone_nodes(&p, bi, w);
            count_work(&cone, bi, &|| format!("block {bi} StateWrite {w}"))?;
            for &i in &cone {
                for r in &b.nodes[i as usize].inputs {
                    if let Ref::State(t) = r {
                        reads[bi][state as usize] |= 1u64 << t;
                    }
                }
            }
            updates[bi][state as usize] = Some((w, cone));
        }
    }
    let mut states = Vec::new();
    for (j, s) in p.states.iter().enumerate() {
        let j = j as u16;
        if !matches!(s.kind, StateKind::Fixed { .. }) {
            continue;
        }
        let mut worst: Option<StateCkptV1> = None;
        #[allow(clippy::needless_range_loop)]
        for bi in 0..p.blocks.len() {
            if updates[bi][j as usize].is_none() {
                continue;
            }
            // The replay closure: the states the update cones read, transitively, in this block.
            let mut members = 1u64 << j;
            loop {
                let next = (0..p.states.len()).filter(|t| members >> t & 1 == 1).fold(members, |m, t| m | reads[bi][t]);
                if next == members {
                    break;
                }
                members = next;
            }
            let closure: Vec<u16> = (0..p.states.len() as u16).filter(|t| members >> t & 1 == 1).collect();
            let cones_of: Vec<(usize, Vec<u16>)> = closure.iter().filter_map(|t| updates[bi][*t as usize].clone()).collect();
            let (groups, loc) = replay_groups(&p, bi, &closure, &cones_of);
            // One group's replay of one position: the aligned nodes' cost divided among the groups,
            // and every free node whole — each group computes it for itself. Unsplit, the whole sum.
            // Summed per update cone, so a node two cones share counts in each.
            let (mut total, mut aligned, mut free) = (CostV1::default(), CostV1::default(), CostV1::default());
            for (_, c) in &cones_of {
                for i in c {
                    let cost = &node_costs[bi][*i as usize];
                    total.add(cost);
                    match loc[*i as usize] {
                        Locality::Free => free.add(cost),
                        _ => aligned.add(cost),
                    }
                }
            }
            let per_position = if groups > 1 {
                let mut split = aligned.div_ceil(groups);
                split.add(&free);
                split
            } else {
                total
            };
            let fit = |cap: u64, per: u64| if per == 0 { u64::MAX } else { cap / per };
            let interval = fit(ceil.max_tile_macs, per_position.macs)
                .min(fit(ceil.max_tile_transcendentals, per_position.transcendentals))
                .min(ceil.max_checkpoint_interval as u64) as u32;
            if interval == 0 {
                // The component past its cap, MACs first (`max_checkpoint_interval ≥ 1` is an input
                // rule, so one of the two is).
                let (limit, value, cap) = if per_position.macs > ceil.max_tile_macs {
                    ("max_tile_macs", per_position.macs, ceil.max_tile_macs)
                } else {
                    ("max_tile_transcendentals", per_position.transcendentals, ceil.max_tile_transcendentals)
                };
                return Err(TirAdmitError::Exceeds { limit, at: format!("the state replay of {} in block {bi}", s.name), value, cap });
            }
            let candidate = StateCkptV1 { state: j, closure, groups, per_position, interval };
            if worst.as_ref().is_none_or(|w| candidate.interval < w.interval) {
                worst = Some(candidate);
            }
        }
        if let Some(w) = worst {
            states.push(w);
        }
    }
    let checkpoint_interval = states.iter().map(|s| s.interval).min().unwrap_or(ceil.max_checkpoint_interval);
    Ok(TirAdmissionV1 { program: p, info, intervals, node_costs, position, cones, states, checkpoint_interval, cone_work: work })
}

/// [`tir_admit_v1`] of a program already in memory (its canonical encoding).
pub fn tir_admit_program_v1(p: &TirProgramV1, inputs: &TirAdmitInputsV1) -> Result<TirAdmissionV1, TirAdmitError> {
    tir_admit_v1(&p.encode(), inputs)
}

/// PALW-TIR-33 for one committed value: every element inside the node's proven interval. The
/// court runs this on every opened committed operand; a value outside is a malformed commitment.
pub fn check_committed(a: &TirAdmissionV1, block: u8, node: u16, values: &[i128]) -> TirResult<()> {
    let iv = a
        .intervals
        .get(block as usize)
        .and_then(|b| b.get(node as usize))
        .ok_or_else(|| TirError::new(TirErrorKind::Malformed, "no such node"))?;
    match values.iter().find(|v| !iv.contains(**v)) {
        None => Ok(()),
        Some(v) => Err(TirError::new(TirErrorKind::Operand, format!("committed value {v} outside [{}, {}]", iv.lo, iv.hi))),
    }
}
