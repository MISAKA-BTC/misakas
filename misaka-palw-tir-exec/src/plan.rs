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
}

fn work_of(ivs: &[Interval]) -> Work {
    if ivs.iter().all(|i| in_i64(*i)) { Work::I64 } else { Work::I128 }
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
        let mut reads_token = false;
        for (bi, b) in p.blocks.iter().enumerate() {
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
                        _ => leaf_interval(p, r).expect("a leaf"),
                    };
                    if *r == Ref::Input(misaka_palw_tir::program::INPUT_TOKEN) {
                        reads_token = true;
                    }
                    in_types.push(t);
                    in_ivs.push(iv);
                }
                let facts = node_facts(&n.prim, &in_ivs, &in_types, &n.out, window, p);
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
                };
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
            blocks.push(BlockPlan {
                window,
                is_layer: info.blocks[bi].is_layer,
                nodes,
                carry_in: b.carry_in.clone(),
                carry_out: b.carry_out.clone(),
            });
        }
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
        })
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
