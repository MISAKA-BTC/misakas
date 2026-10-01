//! **One primitive through three implementations**: the reference evaluator
//! (`misaka_palw_tir::eval::eval_primitive`), the CPU executor (`TirExecutor`, the node of a
//! one-node program), and the device (`Recorder::node` under the CPU executor's own refined plan).
//!
//! The operands are the program's params, so the plan the device receives is exactly the one a
//! node holding these "weights" would run — `TirPlan::refine` with their actual ranges — and the
//! device is asked to honour the same proof the CPU kernel relies on.

#![allow(dead_code)]

use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::{Cmp, DType, Dim, Prim, Ref, Tensor, TensorType, TirErrorKind};
use misaka_palw_tir_exec::elem::Buf;
use misaka_palw_tir_exec::plan::NodePlan;
use misaka_palw_tir_exec::{NodeValue, ParamData, StepSink, TirExecutor, TirParams, TirPlan};
use misaka_palw_tir_gpu::{DevTensor, DeviceFailure, Form, GpuDevice, Recorder, Unsupported};

/// What an evaluation produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Value(Vec<i128>),
    Error(TirErrorKind),
}

/// One primitive application: operands (all become params) and the declared output type.
#[derive(Clone, Debug)]
pub struct NodeCase {
    pub prim: Prim,
    pub inputs: Vec<Tensor>,
    pub out: TensorType,
}

/// The three results of one case.
#[derive(Debug)]
pub struct Report {
    pub reference: Outcome,
    pub cpu: Outcome,
    pub gpu: Result<Outcome, Unsupported>,
    pub plan: NodePlan,
    /// The kernels the device dispatched.
    pub kernels: Vec<String>,
}

impl Report {
    /// Panic unless the CPU executor equals the reference and the device (when it ran) equals both.
    pub fn assert_agree(&self, what: &str) {
        assert_eq!(self.cpu, self.reference, "{what}: CPU executor ≠ reference");
        if let Ok(g) = &self.gpu {
            assert_eq!(
                *g, self.cpu,
                "{what}: DEVICE ≠ CPU executor (plan work {:?} acc {:?} check_out {})",
                self.plan.work, self.plan.acc, self.plan.check_out
            );
        }
    }
}

struct Grab(Option<Vec<i128>>);
impl StepSink for Grab {
    fn every_node(&self) -> bool {
        true
    }
    fn node(&mut self, v: &NodeValue<'_>) {
        if v.block == 0 && v.node == 0 {
            self.0 = Some(v.data.to_i128s());
        }
    }
}

/// The one-node program of `case`: `pre` computes the node from params and commits
/// `Compare(node, node)`; `post` casts that to the logits. `None` when the case is not a valid
/// program node (a type error, an `i128` operand — params are never `i128`).
pub fn program_of(case: &NodeCase) -> Option<misaka_palw_tir::TirProgramV1> {
    if case.inputs.iter().any(|t| t.dtype == DType::I128) || case.out.shape.contains(&Dim::H) {
        return None;
    }
    let mut pb = ProgramBuilder::new(1, 1 << 18);
    let refs: Vec<Ref> = case
        .inputs
        .iter()
        .enumerate()
        .map(|(k, t)| pb.param(&format!("x{k}"), t.dtype, &t.shape.iter().map(|d| *d as u32).collect::<Vec<_>>(), false))
        .collect();
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let n0 = b.push(case.prim.clone(), refs, case.out.clone());
        if matches!(case.prim, Prim::TopK { .. }) {
            b.commit(n0);
        }
        let c = b.compare(n0, n0, Cmp::Eq);
        b.finish(&[c])
    };
    let post = {
        let mut b = pb.block("post", vec![TensorType::new(DType::I8, case.out.shape.clone())]);
        let l = b.cast(Ref::CarryIn(0), DType::I32);
        b.commit(l);
        b.finish(&[])
    };
    Some(pb.finish(pre, vec![], post, 0))
}

/// Run `case` through the three implementations; `None` when it is not a program node. Operands
/// are uploaded in their param form (packed `i8`/`i16`).
pub fn run_case(dev: &GpuDevice, case: &NodeCase) -> Option<Report> {
    run_case_forms(dev, case, 0)
}

/// [`run_case`], with operand `k` uploaded in its COMPUTED form (lanes) when bit `k` of `computed`
/// is set — the form an activation produced by an earlier node has. A form is storage, never a
/// value: every combination must give the same bytes.
pub fn run_case_forms(dev: &GpuDevice, case: &NodeCase, computed: u32) -> Option<Report> {
    let program = program_of(case)?;
    let plan = TirPlan::compile(&program).ok()?;
    let out_shape: Vec<usize> = case.out.resolve(1);
    let reference = match misaka_palw_tir::eval::eval_primitive(&case.prim, &case.inputs, case.out.dtype, &out_shape) {
        Ok(t) => Outcome::Value(t.data),
        Err(e) => Outcome::Error(e.kind),
    };
    let bufs: Vec<Buf> = case.inputs.iter().map(|t| Buf::from_i128s(t.dtype, &t.data)).collect();
    let mut params = TirParams::new(&plan);
    for (j, b) in bufs.iter().enumerate() {
        params.insert(&plan, j as u16, None, ParamData::from_buf(b.clone()).ok()?).ok()?;
    }
    let mut exec = TirExecutor::new(&plan, &params).ok()?;
    let mut grab = Grab(None);
    let cpu = match exec.step(0, &mut grab) {
        Ok(()) => Outcome::Value(grab.0.expect("the node's value")),
        Err(e) => Outcome::Error(e.kind),
    };
    let node = plan.refine(&|j, l| params.range(j, l))[0].nodes[0].clone();
    let ins: Vec<DevTensor> = bufs
        .iter()
        .zip(&case.inputs)
        .enumerate()
        .map(|(k, (b, t))| {
            let form = if computed >> k & 1 == 1 { Form::computed(t.dtype) } else { Form::param(t.dtype) };
            dev.upload(b.slice(), form.expect("a param dtype"), &t.shape)
        })
        .collect();
    let refs: Vec<&DevTensor> = ins.iter().collect();
    let mut rec = Recorder::new(dev, 1);
    let mut kernels = Vec::new();
    let gpu = match rec.node(&node, &refs, &out_shape, 0) {
        Err(u) => Err(u),
        Ok(o) => {
            kernels = rec.log.clone();
            let status = rec.finish();
            Ok(match DeviceFailure::decode(status[0]) {
                Some(f) => Outcome::Error(f.kind),
                None => Outcome::Value(dev.download(&o).to_i128s()),
            })
        }
    };
    Some(Report { reference, cpu, gpu, plan: node, kernels })
}

/// Tally of a run of cases.
#[derive(Default, Debug)]
pub struct Tally {
    pub on_device: usize,
    pub device_errors: usize,
    pub fallback: std::collections::BTreeMap<String, usize>,
    pub not_a_node: usize,
    /// Dispatches per kernel path, over the cases the device ran.
    pub kernels: std::collections::BTreeMap<String, usize>,
}

impl Tally {
    pub fn add(&mut self, r: &Option<Report>) {
        match r {
            None => self.not_a_node += 1,
            Some(r) => match &r.gpu {
                Ok(Outcome::Value(_)) => {
                    self.on_device += 1;
                    for k in &r.kernels {
                        *self.kernels.entry(k.clone()).or_default() += 1;
                    }
                }
                Ok(Outcome::Error(_)) => {
                    self.on_device += 1;
                    self.device_errors += 1;
                    for k in &r.kernels {
                        *self.kernels.entry(k.clone()).or_default() += 1;
                    }
                }
                Err(u) => *self.fallback.entry(format!("{u:?}")).or_default() += 1,
            },
        }
    }
}
