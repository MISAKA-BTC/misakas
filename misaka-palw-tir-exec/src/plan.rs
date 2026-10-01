//! Compilation: a validated `TirProgramV1` becomes a [`TirPlan`] — per node, the working type and
//! the checks the reference's runtime rules still need (from [`crate::ranges`]), plus the tables
//! the executor indexes at run time (consts as typed buffers, state instances, param instances).

use std::collections::BTreeSet;

use misaka_palw_tir::interval::Interval;
use misaka_palw_tir::program::{StateKind, TirProgramV1};
use misaka_palw_tir::validate::{ProgramInfo, validate};
use misaka_palw_tir::{DType, Prim, Ref, TensorType, TirError, TirErrorKind, TirResult};

use crate::elem::Buf;
use crate::ranges::{Facts, dtype_iv, in_i64, leaf_interval, node_facts, within};

/// The integer type an elementwise node computes in before its result is narrowed to `out`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Work {
    /// Every operand and every exact result fits `i64`.
    I64,
    I128,
}

/// How an exact reduction (`MatMul`, `ReduceSum`) accumulates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Acc {
    /// Every partial sum in every order fits `i64` and the declared dtype: any order, no check.
    Fast64,
    /// Every partial sum fits `i128` and the declared dtype: any order, no check.
    Fast128,
    /// Every partial sum fits `i64` but not necessarily the dtype: the positive and negative terms
    /// are summed apart and checked (PALW-TIR-24).
    Pn64,
    /// Nothing is proved: the reference's checked accumulation of each sign in `i128`.
    Pn128,
}

#[derive(Clone, Debug)]
pub struct NodePlan {
    pub prim: Prim,
    pub inputs: Vec<Ref>,
    pub out: TensorType,
    pub commit: bool,
    pub in_types: Vec<TensorType>,
    pub in_ivs: Vec<Interval>,
    pub facts: Facts,
    /// The working type of an elementwise / selection / reduction node.
    pub work: Work,
    /// `MatMul` and `ReduceSum` only.
    pub acc: Acc,
    /// The exact result may leave the declared dtype: check every element (PALW-TIR-23).
    pub check_out: bool,
    /// `Add`/`Sub`/`Mul` in `i128` whose exact result may not even fit `i128`: checked arithmetic.
    pub checked_arith: bool,
    /// `Gather`: an index may leave its axis. `Div`: a divisor may be below 1.
    pub check_operand: bool,
    /// The type the value is STORED in: the declared dtype, except that a computed `i128` node
    /// whose every successful value provably fits `i64` is held in `i64` (the same integers).
    pub store: DType,
    /// The node returns its input's values unchanged: a `Clamp` whose input interval lies inside
    /// `[lo, hi]`, a `Cast` that cannot fail. Its value is a view of its input.
    pub identity: bool,
}

impl NodePlan {
    /// A stand-alone plan for one kernel call (benchmarks, kernel tests): no operands recorded,
    /// the given output dtype and accumulation, every check off.
    pub fn for_kernel(prim: Prim, out: DType, acc: Acc) -> Self {
        NodePlan {
            prim,
            inputs: Vec::new(),
            out: TensorType::scalar(out),
            commit: false,
            in_types: Vec::new(),
            in_ivs: Vec::new(),
            facts: Facts { exact: None, out: dtype_iv(out), needs_operand_check: false },
            work: Work::I64,
            acc,
            check_out: false,
            checked_arith: false,
            check_operand: false,
            store: out,
            identity: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BlockPlan {
    pub window: Option<u32>,
    pub is_layer: bool,
    pub nodes: Vec<NodePlan>,
    pub carry_in: Vec<TensorType>,
    pub carry_out: Vec<u16>,
}

/// One instance of a declared state: `(state, layer)`, `layer` `None` for a global state.
#[derive(Clone, Debug)]
pub struct StateInstance {
    pub state: u16,
    pub layer: Option<u16>,
    pub dtype: DType,
    /// The value's shape (`Fixed`) or one row's shape (`Hist`).
    pub shape: Vec<usize>,
    pub kind: StateKind,
}

/// A compiled program. Build once per class; share between executors.
#[derive(Clone, Debug)]
pub struct TirPlan {
    pub program: TirProgramV1,
    pub info: ProgramInfo,
    pub occurrences: Vec<(u8, Option<u16>)>,
    pub slot_bases: Vec<u32>,
    pub blocks: Vec<BlockPlan>,
    pub consts: Vec<Buf>,
    /// Some node reads the token (spec 04b §9.1(1): a token past `token_bound` fails the step).
    pub reads_token: bool,
    pub instances: Vec<StateInstance>,
    /// `[state][layer or 0] → instance`.
    pub instance_of: Vec<Vec<Option<u32>>>,
    /// Every `(param, layer)` some occurrence reads.
    pub param_instances: BTreeSet<(u16, Option<u16>)>,
    /// The fused regions of every block ([`crate::fused::match_program`]), matched once per plan on
    /// first use: structural, so they hold for every executor and every artifact of the class.
    pub fused: std::sync::OnceLock<Vec<Vec<crate::fused::Region>>>,
    /// Every row gather and its group ([`crate::tiers::tir_row_sites_v1`]) — where a residency
    /// serves rows instead of a whole param, and where a route's rows are admitted together.
    pub rows: crate::tiers::TirRowSitesV1,
}

fn work_of(ivs: &[Interval]) -> Work {
    if ivs.iter().all(|i| in_i64(*i)) { Work::I64 } else { Work::I128 }
}

/// `[min, max]` of the elements of `data` (shape `shape`) whose index along `axis` lies in `rows`.
fn slab_interval(data: &Buf, shape: &[usize], axis: usize, rows: Interval) -> Option<Interval> {
    let n = shape[axis];
    let (lo, hi) = (rows.lo.max(0), rows.hi.min(n as i128 - 1));
    if lo > hi {
        return None;
    }
    let inner: usize = shape[axis + 1..].iter().product();
    let vals = data.to_i128s();
    let mut acc: Option<Interval> = None;
    for (i, v) in vals.iter().enumerate() {
        let a = (i / inner) % n;
        if (a as i128) >= lo && (a as i128) <= hi {
            acc = Some(acc.map_or(Interval::point(*v), |x| x.union(Interval::point(*v))));
        }
    }
    acc
}

/// Plan every node of block `bi`, with `leaf` giving the interval of every param, const, state and
/// input the block reads.
fn plan_block(p: &TirProgramV1, info: &ProgramInfo, consts: &[Buf], bi: usize, leaf: &dyn Fn(&Ref) -> Interval) -> BlockPlan {
    let b = &p.blocks[bi];
    let window = info.blocks[bi].window;
    let mut nodes: Vec<NodePlan> = Vec::with_capacity(b.nodes.len());
    for n in &b.nodes {
        let mut in_types = Vec::with_capacity(n.inputs.len());
        let mut in_ivs = Vec::with_capacity(n.inputs.len());
        for r in &n.inputs {
            let t = match *r {
                Ref::Node(j) => b.nodes[j as usize].out.clone(),
                Ref::CarryIn(k) => b.carry_in[k as usize].clone(),
                Ref::Param(j) => TensorType::fixed(p.params[j as usize].dtype, &p.params[j as usize].shape),
                Ref::Const(j) => TensorType::fixed(p.consts[j as usize].dtype, &p.consts[j as usize].shape),
                Ref::State(j) => TensorType::fixed(p.states[j as usize].dtype, &p.states[j as usize].shape),
                Ref::Input(_) => TensorType::scalar(DType::Idx),
            };
            let iv = match *r {
                Ref::Node(j) => nodes[j as usize].facts.out,
                Ref::CarryIn(_) => dtype_iv(t.dtype),
                _ => leaf(r),
            };
            in_types.push(t);
            in_ivs.push(iv);
        }
        let mut facts = node_facts(&n.prim, &in_ivs, &in_types, &n.out, window, p);
        // A gather from a const reads only the rows its indices can name: a pinned table such as
        // `[2^0 … 2^62]` indexed by a shift known to be 19 gives exactly 2^19.
        if let (Prim::Gather { axis, .. }, Some(Ref::Const(j))) = (&n.prim, n.inputs.first().copied()) {
            let c = &p.consts[j as usize];
            let shape: Vec<usize> = c.shape.iter().map(|d| *d as usize).collect();
            if let Some(iv) = slab_interval(&consts[j as usize], &shape, *axis as usize, in_ivs[1]) {
                facts.exact = Some(iv);
                facts.out = iv;
            }
        }
        let o = n.out.dtype;
        let fits = facts.fits(o);
        let mut plan = NodePlan {
            prim: n.prim.clone(),
            inputs: n.inputs.clone(),
            out: n.out.clone(),
            commit: n.commit,
            in_types,
            in_ivs,
            facts,
            work: Work::I128,
            acc: Acc::Pn128,
            check_out: !fits,
            checked_arith: false,
            check_operand: facts.needs_operand_check,
            store: o,
            identity: false,
        };
        plan.identity = match n.prim {
            Prim::Clamp { lo, hi } => within(plan.in_ivs[0], Interval::new(lo as i128, hi as i128)),
            Prim::Cast => fits,
            _ => false,
        };
        let computed = matches!(
            n.prim,
            Prim::Add
                | Prim::Sub
                | Prim::Mul
                | Prim::Div { .. }
                | Prim::Select
                | Prim::Cast
                | Prim::Clamp { .. }
                | Prim::Log2Floor
                | Prim::IntExp
                | Prim::IntRsqrt
                | Prim::IntLn
                | Prim::MatMul
                | Prim::ReduceSum { .. }
                | Prim::ReduceMax { .. }
        );
        if o == DType::I128 && computed && in_i64(facts.out) {
            plan.store = DType::I64;
        }
        match &n.prim {
            Prim::Add | Prim::Sub | Prim::Mul => {
                let exact_i64 = facts.exact.is_some_and(in_i64);
                plan.work = if exact_i64 { work_of(&plan.in_ivs) } else { Work::I128 };
                plan.checked_arith = facts.exact.is_none();
            }
            Prim::Div { .. } | Prim::Compare { .. } | Prim::Select | Prim::Cast | Prim::Clamp { .. } | Prim::Log2Floor => {
                plan.work = work_of(&plan.in_ivs);
            }
            Prim::StateWrite { .. } | Prim::ReduceMax { .. } | Prim::TopK { .. } => plan.work = work_of(&plan.in_ivs),
            // Their inputs are never i128 (type rule) and idx fits i64.
            Prim::IntExp | Prim::IntRsqrt | Prim::IntLn => plan.work = Work::I64,
            Prim::MatMul | Prim::ReduceSum { .. } => {
                plan.acc = match facts.exact {
                    Some(e) if within(e, dtype_iv(o)) && in_i64(e) => Acc::Fast64,
                    Some(e) if within(e, dtype_iv(o)) => Acc::Fast128,
                    Some(e) if in_i64(e) => Acc::Pn64,
                    _ => Acc::Pn128,
                };
                // The partial-sum rule, not the final value, decides success (PALW-TIR-24).
                plan.check_out = false;
            }
            _ => {}
        }
        nodes.push(plan);
    }
    BlockPlan { window, is_layer: info.blocks[bi].is_layer, nodes, carry_in: b.carry_in.clone(), carry_out: b.carry_out.clone() }
}

impl TirPlan {
    /// Validate (normal form and types) and compile.
    pub fn compile(program: &TirProgramV1) -> TirResult<Self> {
        let info = validate(program)?;
        let p = program;
        let occurrences = p.occurrences();
        let slot_bases = p.occurrence_slot_bases();
        let consts = p
            .consts
            .iter()
            .map(|c| Buf::from_le_bytes(c.dtype, &c.data).ok_or_else(|| TirError::new(TirErrorKind::Operand, "const bytes")))
            .collect::<TirResult<Vec<_>>>()?;
        let mut blocks = Vec::with_capacity(p.blocks.len());
        for bi in 0..p.blocks.len() {
            blocks.push(plan_block(p, &info, &consts, bi, &|r| leaf_interval(p, r).expect("a leaf")));
        }
        let reads_token = p
            .blocks
            .iter()
            .flat_map(|b| b.nodes.iter())
            .any(|n| n.inputs.contains(&Ref::Input(misaka_palw_tir::program::INPUT_TOKEN)));
        // State instances: one per (state, layer at which a block referencing it runs).
        let layer_slots = p.schedule.layers.len().max(1);
        let mut instance_of: Vec<Vec<Option<u32>>> =
            p.states.iter().map(|s| vec![None; if s.per_layer { layer_slots } else { 1 }]).collect();
        let mut instances = Vec::new();
        let mut param_instances = BTreeSet::new();
        for (block, layer) in &occurrences {
            let b = &p.blocks[*block as usize];
            let mut touch = BTreeSet::new();
            for n in &b.nodes {
                for r in &n.inputs {
                    match *r {
                        Ref::State(j) => {
                            touch.insert(j);
                        }
                        Ref::Param(j) => {
                            let l = if p.params[j as usize].per_layer { *layer } else { None };
                            param_instances.insert((j, l));
                        }
                        _ => {}
                    }
                }
                if let Prim::StateWrite { state } | Prim::HistAppend { state } = n.prim {
                    touch.insert(state);
                }
            }
            for j in touch {
                let s = &p.states[j as usize];
                let (key, l) = if s.per_layer { (layer.map(|l| l as usize).unwrap_or(0), *layer) } else { (0, None) };
                if instance_of[j as usize][key].is_none() {
                    instance_of[j as usize][key] = Some(instances.len() as u32);
                    instances.push(StateInstance {
                        state: j,
                        layer: l,
                        dtype: s.dtype,
                        shape: s.shape.iter().map(|d| *d as usize).collect(),
                        kind: s.kind,
                    });
                }
            }
        }
        Ok(TirPlan {
            program: program.clone(),
            info,
            occurrences,
            slot_bases,
            blocks,
            consts,
            reads_token,
            instances,
            instance_of,
            param_instances,
            fused: std::sync::OnceLock::new(),
            rows: crate::tiers::tir_row_sites_v1(program),
        })
    }

    /// Every instance the program's inventory holds, in inventory order (params ascending, each
    /// one's layers ascending) — the order a container stores them and a pass reads them in.
    pub fn param_instances_in_order(&self) -> Vec<(u16, Option<u16>)> {
        crate::tiers::tir_param_instances_v1(&self.program)
            .into_iter()
            .enumerate()
            .flat_map(|(j, inst)| inst.into_iter().map(move |l| (j as u16, l)))
            .collect()
    }

    /// The fused regions of every block, matched on first use.
    pub fn fused_regions(&self) -> &[Vec<crate::fused::Region>] {
        self.fused.get_or_init(|| crate::fused::match_program(&self.program))
    }

    /// **Per occurrence, the block plan refined by the params actually bound.** A param's interval
    /// is its dtype's full range (weights are untrusted, spec 04b §7) — sound for every artifact,
    /// and the reason an A16 narrowing `x·m` must be planned in `i128`. An executor holds ONE
    /// artifact for its whole life, so the actual `[min, max]` of each param instance it reads is
    /// just as sound for every execution it will run, and much tighter: the narrowing chains then
    /// fit `i64`, and checks the actual weights cannot trip are dropped. Nothing about the values
    /// changes — only the width an exact result is computed in, and which checks can fire.
    pub fn refine(&self, ranges: &dyn Fn(u16, Option<u16>) -> Option<Interval>) -> Vec<BlockPlan> {
        let p = &self.program;
        self.occurrences
            .iter()
            .map(|&(block, layer)| {
                let leaf = |r: &Ref| -> Interval {
                    if let Ref::Param(j) = *r {
                        let l = if p.params[j as usize].per_layer { layer } else { None };
                        if let Some(iv) = ranges(j, l) {
                            return iv;
                        }
                    }
                    leaf_interval(p, r).expect("a leaf")
                };
                plan_block(p, &self.info, &self.consts, block as usize, &leaf)
            })
            .collect()
    }

    /// The instance of state `j` an occurrence at `layer` reads and writes.
    #[inline]
    pub fn instance(&self, j: u16, layer: Option<u16>) -> Option<u32> {
        let row = &self.instance_of[j as usize];
        let key = if self.program.states[j as usize].per_layer { layer.map(|l| l as usize).unwrap_or(0) } else { 0 };
        row.get(key).copied().flatten()
    }

    /// Nodes in the whole unrolled position (the slot count).
    pub fn slots_per_position(&self) -> u32 {
        let last = self.occurrences.len() - 1;
        self.slot_bases[last] + self.program.blocks[self.occurrences[last].0 as usize].nodes.len() as u32
    }
}
