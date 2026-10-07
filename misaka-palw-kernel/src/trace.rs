//! **The producer's trace, the evidence it commits, and the wiring every check authenticates by.**
//!
//! A claim's evidence is the commitment of **every node value of every block occurrence of every
//! position**, fixed before any challenge exists. Nothing else needs committing: every input of a
//! relation is *wired* to something already committed or public ([`SourceV1`]):
//!
//! * a node input → that node's committed value at the same position and occurrence;
//! * a carry-in → the previous occurrence's carry-out node, committed;
//! * a param → the artifact's param commitment; a const → the program's bytes;
//! * a `Fixed` state → the committed `StateWrite` output of the previous position, or zeros at
//!   position 0 (initialization), so continuity is not a separate claim the producer could fabricate;
//! * a `Hist` window's prior rows → the committed appended rows of the earlier positions, in order;
//! * the token and position inputs → the job's public input.
//!
//! A verifier therefore never trusts an "entry state" the producer states for a segment: it opens the
//! predecessor's committed exit (RFC-0011 §15.2's boundary row).

use std::collections::BTreeMap;

use misaka_palw_tir::eval::eval_primitive;
use misaka_palw_tir::program::{INPUT_TOKEN, Ref, StateKind, TirProgramV1};
use misaka_palw_tir::validate::ProgramInfo;
use misaka_palw_tir::{DType, ParamSource, Prim, Tensor, TirError, TirResult};

use crate::hash::{Digest, finish, keyed};

pub const TENSOR_COMMITMENT_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/tensor/v1";
pub const EVIDENCE_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/evidence/v1";
pub const OUTPUT_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/output/v1";
pub const PARAM_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/params/v1";

/// `H(dtype ‖ rank ‖ dims ‖ canonical little-endian elements)`.
pub fn tensor_commitment(t: &Tensor) -> Digest {
    let mut s = keyed(TENSOR_COMMITMENT_DOMAIN_V1);
    s.update(&[t.dtype.tag(), t.shape.len() as u8]);
    for d in &t.shape {
        s.update(&(*d as u64).to_le_bytes());
    }
    s.update(&t.to_le_bytes());
    finish(s)
}

/// The commitment of every param instance the artifact holds: `(param, layer) → commitment`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParamCommitmentsV1 {
    pub by_instance: BTreeMap<(u16, Option<u16>), Digest>,
}

impl ParamCommitmentsV1 {
    pub fn of(params: &misaka_palw_tir::MapParams) -> Self {
        Self { by_instance: params.tensors.iter().map(|(k, t)| (*k, tensor_commitment(t))).collect() }
    }

    /// The root an artifact binding commits to.
    pub fn root(&self) -> Digest {
        let mut s = keyed(PARAM_ROOT_DOMAIN_V1);
        s.update(&(self.by_instance.len() as u64).to_le_bytes());
        for ((j, l), d) in &self.by_instance {
            s.update(&j.to_le_bytes());
            match l {
                Some(l) => s.update(&[1]).update(&l.to_le_bytes()),
                None => s.update(&[0]),
            };
            s.update(d);
        }
        finish(s)
    }
}

/// A claim's evidence: `commitments[position][occurrence][node]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceV1 {
    pub commitments: Vec<Vec<Vec<Digest>>>,
}

impl EvidenceV1 {
    pub fn root(&self) -> Digest {
        let mut s = keyed(EVIDENCE_ROOT_DOMAIN_V1);
        s.update(&(self.commitments.len() as u64).to_le_bytes());
        for pos in &self.commitments {
            s.update(&(pos.len() as u64).to_le_bytes());
            for occ in pos {
                s.update(&(occ.len() as u64).to_le_bytes());
                for d in occ {
                    s.update(d);
                }
            }
        }
        finish(s)
    }

    pub fn at(&self, p: u32, s: u16, n: u16) -> Option<&Digest> {
        self.commitments.get(p as usize)?.get(s as usize)?.get(n as usize)
    }

    /// The output root the claim states: every position's logits commitment, in order.
    pub fn output_root(&self, program: &TirProgramV1) -> Digest {
        let post = program.schedule.layers.len() + 1;
        let mut s = keyed(OUTPUT_ROOT_DOMAIN_V1);
        s.update(&(self.commitments.len() as u64).to_le_bytes());
        for pos in &self.commitments {
            if let Some(d) = pos.get(post).and_then(|o| o.get(program.logits as usize)) {
                s.update(d);
            }
        }
        finish(s)
    }
}

/// Every node value: `values[position][occurrence][node]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceV1 {
    pub values: Vec<Vec<Vec<Tensor>>>,
}

impl TraceV1 {
    pub fn evidence(&self) -> EvidenceV1 {
        EvidenceV1 {
            commitments: self.values.iter().map(|p| p.iter().map(|o| o.iter().map(tensor_commitment).collect()).collect()).collect(),
        }
    }

    pub fn value(&self, p: u32, s: u16, n: u16) -> Option<&Tensor> {
        self.values.get(p as usize)?.get(s as usize)?.get(n as usize)
    }
}

/// Where an input's value comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceV1 {
    /// A committed node value.
    Node {
        position: u32,
        occurrence: u16,
        node: u16,
    },
    Param {
        index: u16,
        layer: Option<u16>,
    },
    Const(u16),
    /// The all-zeros initial value of a state of this dtype/shape.
    Zeros {
        dtype: DType,
        shape: Vec<usize>,
    },
    /// A public job input (`idx` scalar).
    Public(u32),
}

/// The program's wiring, precomputed once.
pub struct WiringV1<'p> {
    pub program: &'p TirProgramV1,
    pub info: ProgramInfo,
    pub occurrences: Vec<(u8, Option<u16>)>,
    /// `(state, layer) → (occurrence, node)` of its `StateWrite`.
    writers: BTreeMap<(u16, Option<u16>), (u16, u16)>,
    /// `(state, layer) → (occurrence, node)` of its `HistAppend`.
    appenders: BTreeMap<(u16, Option<u16>), (u16, u16)>,
}

fn malformed(msg: impl Into<String>) -> TirError {
    TirError::new(misaka_palw_tir::TirErrorKind::Malformed, msg)
}

impl<'p> WiringV1<'p> {
    pub fn new(program: &'p TirProgramV1) -> TirResult<Self> {
        let info = misaka_palw_tir::validate::validate(program)?;
        let occurrences = program.occurrences();
        let mut writers = BTreeMap::new();
        let mut appenders = BTreeMap::new();
        for (s, (b, layer)) in occurrences.iter().enumerate() {
            for (ni, node) in program.blocks[*b as usize].nodes.iter().enumerate() {
                match node.prim {
                    Prim::StateWrite { state } => {
                        writers.insert((state, *layer), (s as u16, ni as u16));
                    }
                    Prim::HistAppend { state } => {
                        appenders.insert((state, *layer), (s as u16, ni as u16));
                    }
                    _ => {}
                }
            }
        }
        Ok(Self { program, info, occurrences, writers, appenders })
    }

    /// `H` at a position for an occurrence's block.
    pub fn h(&self, occurrence: u16, position: u32) -> usize {
        let b = self.occurrences[occurrence as usize].0 as usize;
        self.info.blocks[b].window.map(|w| (position as usize + 1).min(w as usize)).unwrap_or(1)
    }

    pub fn node(&self, occurrence: u16, node: u16) -> &misaka_palw_tir::program::Node {
        &self.program.blocks[self.occurrences[occurrence as usize].0 as usize].nodes[node as usize]
    }

    /// The source of input `idx` of node `(p, s, n)`.
    pub fn input_source(&self, tokens: &[u32], p: u32, s: u16, n: u16, idx: usize) -> TirResult<SourceV1> {
        let layer = self.occurrences[s as usize].1;
        let node = self.node(s, n);
        let r = node.inputs.get(idx).ok_or_else(|| malformed("no such input"))?;
        Ok(match *r {
            Ref::Node(j) => SourceV1::Node { position: p, occurrence: s, node: j },
            Ref::CarryIn(k) => {
                if s == 0 {
                    return Err(malformed("a carry-in in the first occurrence"));
                }
                let prev = self.occurrences[s as usize - 1].0 as usize;
                let out = *self.program.blocks[prev].carry_out.get(k as usize).ok_or_else(|| malformed("no such carry-out"))?;
                SourceV1::Node { position: p, occurrence: s - 1, node: out }
            }
            Ref::Param(j) => {
                let per_layer = self.program.params[j as usize].per_layer;
                SourceV1::Param { index: j, layer: if per_layer { layer } else { None } }
            }
            Ref::Const(j) => SourceV1::Const(j),
            Ref::State(j) => match (p, self.writers.get(&(j, layer))) {
                (p, Some((ws, wn))) if p > 0 => SourceV1::Node { position: p - 1, occurrence: *ws, node: *wn },
                _ => {
                    let st = &self.program.states[j as usize];
                    SourceV1::Zeros { dtype: st.dtype, shape: st.shape.iter().map(|d| *d as usize).collect() }
                }
            },
            Ref::Input(j) => {
                let v = if j == INPUT_TOKEN {
                    let t = *tokens.get(p as usize).ok_or_else(|| malformed("no token for the position"))?;
                    if t >= self.program.token_bound {
                        return Err(malformed(format!("token {t} ≥ token_bound {}", self.program.token_bound)));
                    }
                    t
                } else {
                    p
                };
                SourceV1::Public(v)
            }
        })
    }

    /// The prior rows a `HistAppend` at `(p, s, n)` reads: the appended rows of the previous
    /// `min(p, window − 1)` positions, oldest first.
    pub fn hist_prior_sources(&self, tokens: &[u32], p: u32, s: u16, n: u16) -> TirResult<Vec<SourceV1>> {
        let Prim::HistAppend { state } = self.node(s, n).prim else { return Ok(Vec::new()) };
        let StateKind::Hist { window } = self.program.states[state as usize].kind else {
            return Err(malformed("HistAppend on a Fixed state"));
        };
        let layer = self.occurrences[s as usize].1;
        let (as_, an) = *self.appenders.get(&(state, layer)).ok_or_else(|| malformed("no appender"))?;
        let rows = (p as usize).min(window as usize - 1) as u32;
        (p - rows..p).map(|q| self.input_source(tokens, q, as_, an, 0)).collect()
    }
}

/// Resolve a source from the producer's own values (the tracer) — the verifier resolves the same
/// sources from openings instead.
pub(crate) fn const_tensor(program: &TirProgramV1, j: u16) -> TirResult<Tensor> {
    let c = &program.consts[j as usize];
    Tensor::from_le_bytes(c.dtype, &c.shape.iter().map(|d| *d as usize).collect::<Vec<_>>(), &c.data)
}

/// **One node, from its inputs** — the exact semantics every check and court shares.
pub fn eval_node(
    program: &TirProgramV1,
    node: &misaka_palw_tir::program::Node,
    inputs: &[Tensor],
    prior_rows: &[Tensor],
    h: usize,
) -> TirResult<Tensor> {
    let out_shape = node.out.resolve(h);
    match node.prim {
        Prim::StateWrite { state } => {
            let StateKind::Fixed { lo, hi } = program.states[state as usize].kind else {
                return Err(malformed("StateWrite on a Hist state"));
            };
            let x = inputs.first().ok_or_else(|| malformed("StateWrite without its input"))?;
            Tensor::new(node.out.dtype, out_shape, x.data.iter().map(|v| (*v).clamp(lo as i128, hi as i128)).collect())
        }
        Prim::HistAppend { state } => {
            let s = &program.states[state as usize];
            let row = inputs.first().ok_or_else(|| malformed("HistAppend without its row"))?;
            let mut data = Vec::with_capacity((prior_rows.len() + 1) * row.len());
            for r in prior_rows.iter().chain(std::iter::once(row)) {
                if r.dtype != s.dtype || r.shape != s.shape.iter().map(|d| *d as usize).collect::<Vec<_>>() {
                    return Err(malformed("a history row of another type"));
                }
                data.extend_from_slice(&r.data);
            }
            Tensor::new(s.dtype, out_shape, data)
        }
        _ => eval_primitive(&node.prim, inputs, node.out.dtype, &out_shape),
    }
}

/// **An honest producer's trace**: every node value, position by position.
pub fn trace_v1(program: &TirProgramV1, params: &dyn ParamSource, tokens: &[u32]) -> TirResult<TraceV1> {
    let w = WiringV1::new(program)?;
    let mut values: Vec<Vec<Vec<Tensor>>> = Vec::with_capacity(tokens.len());
    for p in 0..tokens.len() as u32 {
        let mut pos_vals: Vec<Vec<Tensor>> = Vec::with_capacity(w.occurrences.len());
        for s in 0..w.occurrences.len() as u16 {
            let b = w.occurrences[s as usize].0 as usize;
            let mut occ_vals: Vec<Tensor> = Vec::with_capacity(program.blocks[b].nodes.len());
            for n in 0..program.blocks[b].nodes.len() as u16 {
                let resolve = |src: SourceV1, occ_vals: &Vec<Tensor>, pos_vals: &Vec<Vec<Tensor>>| -> TirResult<Tensor> {
                    match src {
                        SourceV1::Node { position, occurrence, node } if position == p && occurrence == s => {
                            occ_vals.get(node as usize).cloned().ok_or_else(|| malformed("a forward node reference"))
                        }
                        SourceV1::Node { position, occurrence, node } if position == p => {
                            Ok(pos_vals[occurrence as usize][node as usize].clone())
                        }
                        SourceV1::Node { position, occurrence, node } => {
                            Ok(values[position as usize][occurrence as usize][node as usize].clone())
                        }
                        SourceV1::Param { index, layer } => params
                            .param(index, layer)
                            .ok_or_else(|| TirError::new(misaka_palw_tir::TirErrorKind::Missing, format!("param {index}"))),
                        SourceV1::Const(j) => const_tensor(program, j),
                        SourceV1::Zeros { dtype, shape } => Ok(Tensor::zeros(dtype, &shape)),
                        SourceV1::Public(v) => Tensor::scalar(DType::Idx, v as i128),
                    }
                };
                let node = w.node(s, n);
                let inputs = (0..node.inputs.len())
                    .map(|i| resolve(w.input_source(tokens, p, s, n, i)?, &occ_vals, &pos_vals))
                    .collect::<TirResult<Vec<_>>>()?;
                let prior = w
                    .hist_prior_sources(tokens, p, s, n)?
                    .into_iter()
                    .map(|src| resolve(src, &occ_vals, &pos_vals))
                    .collect::<TirResult<Vec<_>>>()?;
                occ_vals.push(eval_node(program, node, &inputs, &prior, w.h(s, p))?);
            }
            pos_vals.push(occ_vals);
        }
        values.push(pos_vals);
    }
    Ok(TraceV1 { values })
}
