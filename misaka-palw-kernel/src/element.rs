//! **Element courts (K2-TIR-v4)** — `docs/design/palw/k2-real-scale.md` §3.
//!
//! A fault names ONE output element of one committed value of a segmented claim. The filing opens one leaf of the output holding it
//! and, per input, the leaves covering the element's **dependency line** (one element for an elementwise or structure primitive, the
//! index then the datum for a `Gather`, the reduced line for `ReduceSum` / `ReduceMax` / `TopK`, a row of `X` and a column of `W` for a
//! `MatMul`, the new row or the previous position's window for a `HistAppend`), every leaf a [`LeafOpeningV3`] authenticated through a
//! [`NodeOpeningV1`] against the claim's on-chain segment roots (or against the class's v3 param commitments). The court evaluates
//! the element with the reference semantics on the reduced operands ([`element_value_v1`]: `eval_primitive` on the line, the
//! `1×k · k×1` product or a scalar — the exact-result rule, `Cast`'s fit, `Div`'s divisor and `Gather`'s bounds included) and convicts
//! iff the committed element differs or the semantics refuse. Opening sizes are bounded by the tile, not by the tensor.
//!
//! **One evaluator for both sides.** The court reads operands from the filed leaves; the prover (a fresh outsider) runs the SAME
//! [`element_value_v1`] over full values while recording what it reads, and opens exactly the leaves covering those reads — so a fault
//! the prover files is a fault the court convicts, and the dependency line is never written twice.
//!
//! Soundness: an honest trace has every leaf of every value equal to the reference value, so every element court dismisses.
//! Completeness (every family): take the first wrong value in `(position, occurrence, node)` order and a wrong element of any of its
//! leaves; every operand it reads is an earlier value (or a param / const / zeros / token, authenticated), hence right, so the element
//! court on it convicts. A window is judged against the previous position's window and the new row, a view against its window.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::eval::eval_primitive;
use misaka_palw_tir::program::{INPUT_TOKEN, Node, Ref, StateKind, TirProgramV1};
use misaka_palw_tir::types::strides;
use misaka_palw_tir::{DType, Prim, Tensor};

use crate::hash::Digest;
use crate::job::DecodeRuleV1;
use crate::merkle::{AXIS_COL, AXIS_ROW};
use crate::merkle3::{LayoutV3, LeafOpeningV3, TreesV3, commit_v3, tensor_commitment_v3};
use crate::seg::{
    NodeOpeningV1, PROMPT_TILE_IDS_V1, PromptTileOpeningV1, position_in_segment, position_node_opening_v1, position_root_of_v1,
};
use crate::trace::{ParamCommitmentsV1, SourceV1, WiringV1, const_tensor, eval_node};
use crate::verify::DismissalV1;

/// A parse bound on a filed segmented fault.
pub const MAX_SEG_FAULT_BYTES_V1: usize = 64 << 20;

/// **Everything public a segmented claim's courts read**: the class's program and v3 param commitments, the claim's segment roots and
/// positions, the job's prompt (by length and root; the ids when the job carries them inline) and the delivered ids.
pub struct SegClaimContextV1<'a> {
    pub program: &'a TirProgramV1,
    pub params: &'a ParamCommitmentsV1,
    pub segment_roots: &'a [Digest],
    pub positions: u32,
    pub prompt_len: u32,
    pub prompt_root: Digest,
    pub inline_prompt: Option<&'a [u32]>,
    pub generated: &'a [u32],
    pub decode: DecodeRuleV1,
    /// K2-TIR-v5: the program's job-bound inputs (`crate::seg_encoder`). For such a claim the `tokens` every check takes are the job's
    /// prompt ids (one position; no node reads a per-position token).
    pub encoder: Option<crate::seg_encoder::EncoderBindingV1>,
}

/// One operand of a filing: the node it is a value of (`None` for a param) and the leaves the court reads.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct OperandOpeningV1 {
    pub node: Option<NodeOpeningV1>,
    pub leaves: Vec<LeafOpeningV3>,
}

impl OperandOpeningV1 {
    pub fn byte_len(&self) -> u64 {
        self.node.as_ref().map_or(1, |n| n.byte_len()) + self.leaves.iter().map(LeafOpeningV3::byte_len).sum::<u64>() + 8
    }
}

/// **One wrong element.** `inputs` follows the node's inputs; a `HistAppend` has two: the new row (input 0) and the window of the
/// previous position.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ElementFaultV1 {
    pub position: u32,
    pub occurrence: u16,
    pub node: u16,
    /// The flat (row-major) index of the element in the value.
    pub element: u64,
    pub output: OperandOpeningV1,
    pub inputs: Vec<OperandOpeningV1>,
    /// The prompt tile holding the token the node reads (a tiled job's prompt position).
    pub token: Option<PromptTileOpeningV1>,
}

/// **A committed value whose type is not its node's**: its header (dtype, shape and both roots) opened against its commitment.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MalformedFaultV1 {
    pub opening: NodeOpeningV1,
    pub dtype: u8,
    pub shape: Vec<u64>,
    pub row_root: Digest,
    pub col_root: Digest,
}

/// **A delivered id that is not the decode of its committed logits**: the greedy rule convicts with two elements — the delivered id and
/// a rival that beats it (`>`, or `=` at a lower index).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SegDecodeFaultV1 {
    pub index: u32,
    pub rival: u32,
    pub logits: OperandOpeningV1,
}

/// A whole committed value a filer supplies (its own re-execution of a withheld value), authenticated by its commitment.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct WholeTensorV1 {
    pub dtype: u8,
    pub shape: Vec<u64>,
    /// The elements at the dtype's width, little endian.
    pub bytes: Vec<u8>,
}

impl WholeTensorV1 {
    pub fn of(t: &Tensor) -> Self {
        Self { dtype: t.dtype.tag(), shape: t.shape.iter().map(|d| *d as u64).collect(), bytes: t.to_le_bytes() }
    }

    pub fn tensor(&self) -> Option<Tensor> {
        let dtype = DType::ALL.into_iter().find(|d| d.tag() == self.dtype)?;
        let shape: Vec<usize> = self.shape.iter().map(|d| usize::try_from(*d).ok()).collect::<Option<_>>()?;
        Tensor::from_le_bytes(dtype, &shape, &self.bytes).ok()
    }
}

/// One operand of a whole-value court: its node opening, and either leaves or the whole value.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct WholeOperandV1 {
    pub node: Option<NodeOpeningV1>,
    pub leaves: Vec<LeafOpeningV3>,
    pub whole: Option<WholeTensorV1>,
}

/// **A withheld value whose commitment is not the value's relation** (`crate::seg_scope`, DA16b option (c)): the court recomputes
/// every element from the operands and compares the commitment. The output is opened by its node opening alone.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct WholeValueFaultV1 {
    pub position: u32,
    pub occurrence: u16,
    pub node: u16,
    pub output: NodeOpeningV1,
    pub inputs: Vec<WholeOperandV1>,
    pub token: Option<PromptTileOpeningV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum SegFaultV1 {
    Element(ElementFaultV1) = 0,
    Malformed(MalformedFaultV1) = 1,
    Decode(SegDecodeFaultV1) = 2,
    /// A withheld value's whole-value court (`crate::seg_scope`).
    WholeValue(WholeValueFaultV1) = 3,
}

impl SegFaultV1 {
    /// The value a fault names, `(position, occurrence, node)` (`None` for a decode fault).
    pub fn at(&self) -> Option<(u32, u16, u16)> {
        match self {
            Self::Element(e) => Some((e.position, e.occurrence, e.node)),
            Self::WholeValue(w) => Some((w.position, w.occurrence, w.node)),
            Self::Malformed(m) => Some((m.opening.position, m.opening.occurrence, m.opening.node)),
            Self::Decode(_) => None,
        }
    }

    /// The prompt tile a fault opens, if any.
    pub fn token(&self) -> Option<&PromptTileOpeningV1> {
        match self {
            Self::Element(e) => e.token.as_ref(),
            Self::WholeValue(w) => w.token.as_ref(),
            _ => None,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("in-memory borsh")
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_SEG_FAULT_BYTES_V1 {
            return Err("a segmented fault past its parse bound".into());
        }
        borsh::from_slice(bytes).map_err(|e| format!("not a segmented fault: {e}"))
    }
}

/// What a segmented court convicted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SegConvictionKindV1 {
    /// The committed element is not the reference value (or the semantics refuse every value).
    Element { element: u64 },
    /// The committed value is not of its node's declared type.
    Malformed,
    /// Two authenticated leaves of one operand disagree on an element: no tensor has that commitment.
    Inconsistent { input: u16 },
    /// A delivered id is not the decode of its logits.
    Decode { index: u32 },
    /// A withheld value's commitment is not its relation's (or the semantics refuse an element of it).
    WholeValue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SegConvictionV1 {
    pub position: u32,
    pub occurrence: u16,
    pub node: u16,
    pub kind: SegConvictionKindV1,
}

/// An element a filing does not open: `(operand, flat index)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MissingV1(pub usize, pub u64);

/// Where [`element_value_v1`] reads operand elements from.
pub trait OperandsV1 {
    fn get(&self, operand: usize, flat: u64) -> Result<i128, MissingV1>;
}

/// The shapes an element is evaluated at.
#[derive(Clone, Debug)]
pub struct ElementShapesV1 {
    pub out: Vec<usize>,
    pub inputs: Vec<Vec<usize>>,
    pub in_dtypes: Vec<DType>,
    /// `(H at p, H at p − 1, elements of one row)` for a `HistAppend`.
    pub hist: Option<(usize, usize, usize)>,
}

fn unravel(mut i: u64, st: &[usize]) -> Vec<usize> {
    st.iter()
        .map(|s| {
            let q = i / *s as u64;
            i %= *s as u64;
            q as usize
        })
        .collect()
}

fn ravel(ix: &[usize], st: &[usize]) -> u64 {
    ix.iter().zip(st).map(|(a, b)| (*a as u64) * (*b as u64)).sum()
}

/// The flat index of the element of `in_shape` that out-index `o` reads under numpy broadcasting.
fn bcast(o: &[usize], in_shape: &[usize]) -> u64 {
    let ist = strides(in_shape);
    let off = o.len() - in_shape.len();
    (0..in_shape.len()).map(|k| if in_shape[k] == 1 { 0 } else { o[off + k] as u64 * ist[k] as u64 }).sum()
}

fn fit(v: i128, dtype: DType) -> Option<i128> {
    dtype.contains(v).then_some(v)
}

fn scalar_eval(prim: &Prim, vals: &[(DType, i128)], out: DType) -> Option<i128> {
    let ts: Vec<Tensor> = vals.iter().map(|(d, v)| Tensor { dtype: *d, shape: Vec::new(), data: vec![*v] }).collect();
    eval_primitive(prim, &ts, out, &[]).ok().map(|t| t.data[0])
}

fn line_eval(prim: &Prim, line: Vec<i128>, dtype: DType, out: DType, out_len: usize) -> Option<Tensor> {
    let n = line.len();
    let t = Tensor { dtype, shape: vec![n], data: line };
    eval_primitive(prim, &[t], out, &[out_len]).ok()
}

/// **The reference value of element `e` of `node`'s output**, reading operands through `ops`. `Ok(None)`: the semantics refuse (no
/// value is right); `Err`: an element `ops` does not hold. The ONE evaluator the court and the prover share.
pub fn element_value_v1(
    program: &TirProgramV1,
    node: &Node,
    sh: &ElementShapesV1,
    e: u64,
    ops: &dyn OperandsV1,
) -> Result<Option<i128>, MissingV1> {
    // A rank-one lookup table gathered by an arbitrary-shaped index tensor has no
    // coordinate transform: its output's flat position IS the index tensor's position.
    // Check the index before touching the table, exactly as the generic branch does.
    if flat_gather_v1(node, &sh.inputs, &sh.out) {
        let index = ops.get(1, e)?;
        return Ok(if index < 0 || index >= sh.inputs[0][0] as i128 { None } else { Some(ops.get(0, index as u64)?) });
    }
    let out_dtype = node.out.dtype;
    let ost = strides(&sh.out);
    let o = unravel(e, &ost);
    let ins = &sh.inputs;
    let dt = |i: usize| sh.in_dtypes.get(i).copied().unwrap_or(DType::I128);
    Ok(match &node.prim {
        Prim::Reshape | Prim::Cast => fit(ops.get(0, e)?, out_dtype),
        Prim::Transpose { perm } => {
            let mut j = vec![0usize; o.len()];
            for (k, p) in perm.iter().enumerate() {
                j[*p as usize] = o[k];
            }
            Some(ops.get(0, ravel(&j, &strides(&ins[0])))?)
        }
        Prim::Slice { axis, start } => {
            let mut j = o.clone();
            j[*axis as usize] += *start as usize;
            Some(ops.get(0, ravel(&j, &strides(&ins[0])))?)
        }
        Prim::Concat { axis } => {
            let a = *axis as usize;
            let mut j = o.clone();
            let mut found = None;
            for (i, shape) in ins.iter().enumerate() {
                if j[a] < shape[a] {
                    found = Some(ops.get(i, ravel(&j, &strides(shape)))?);
                    break;
                }
                j[a] -= shape[a];
            }
            found
        }
        Prim::Broadcast => Some(ops.get(0, bcast(&o, &ins[0]))?),
        Prim::Iota { axis, start, step } => fit(*start as i128 + (*step as i128) * (o[*axis as usize] as i128), out_dtype),
        Prim::Gather { axis, batch_dims } => {
            let (a, b) = (*axis as usize, *batch_dims as usize);
            let m = ins[1].len() - b;
            let mut xi: Vec<usize> = o[..b].to_vec();
            xi.extend_from_slice(&o[a..a + m]);
            let v = ops.get(1, ravel(&xi, &strides(&ins[1])))?;
            if v < 0 || v >= ins[0][a] as i128 {
                None
            } else {
                let mut di: Vec<usize> = o[..a].to_vec();
                di.push(v as usize);
                di.extend_from_slice(&o[a + m..]);
                Some(ops.get(0, ravel(&di, &strides(&ins[0])))?)
            }
        }
        Prim::Add | Prim::Sub | Prim::Mul | Prim::Div { .. } | Prim::Compare { .. } => {
            let (x, y) = (ops.get(0, bcast(&o, &ins[0]))?, ops.get(1, bcast(&o, &ins[1]))?);
            scalar_eval(&node.prim, &[(dt(0), x), (dt(1), y)], out_dtype)
        }
        Prim::Select => {
            let c = ops.get(0, bcast(&o, &ins[0]))?;
            let x = ops.get(1, bcast(&o, &ins[1]))?;
            let y = ops.get(2, bcast(&o, &ins[2]))?;
            scalar_eval(&node.prim, &[(dt(0), c), (dt(1), x), (dt(2), y)], out_dtype)
        }
        Prim::Clamp { .. } | Prim::Log2Floor | Prim::IntExp | Prim::IntRsqrt | Prim::IntLn => {
            scalar_eval(&node.prim, &[(dt(0), ops.get(0, bcast(&o, &ins[0]))?)], out_dtype)
        }
        Prim::MatMul => {
            let r = sh.out.len();
            let (xs, ws) = (&ins[0], &ins[1]);
            let (xr, wr) = (xs.len(), ws.len());
            let (m, k, n) = (xs[xr - 2], xs[xr - 1], ws[wr - 1]);
            let ob = &o[..r - 2];
            let xb = bcast(ob, &xs[..xr - 2]) * (m * k) as u64;
            let wb = bcast(ob, &ws[..wr - 2]) * (k * n) as u64;
            let (i, j) = (o[r - 2], o[r - 1]);
            let mut xrow = Vec::with_capacity(k);
            let mut wcol = Vec::with_capacity(k);
            for t in 0..k {
                xrow.push(ops.get(0, xb + (i * k + t) as u64)?);
            }
            for t in 0..k {
                wcol.push(ops.get(1, wb + (t * n + j) as u64)?);
            }
            let x = Tensor { dtype: dt(0), shape: vec![1, k], data: xrow };
            let w = Tensor { dtype: dt(1), shape: vec![k, 1], data: wcol };
            eval_primitive(&Prim::MatMul, &[x, w], out_dtype, &[1, 1]).ok().map(|t| t.data[0])
        }
        Prim::ReduceSum { axis } | Prim::ReduceMax { axis } => {
            let a = *axis as usize;
            let ist = strides(&ins[0]);
            let mut line = Vec::with_capacity(ins[0][a]);
            for t in 0..ins[0][a] {
                let mut j = o.clone();
                j[a] = t;
                line.push(ops.get(0, ravel(&j, &ist))?);
            }
            let reduced =
                if matches!(node.prim, Prim::ReduceSum { .. }) { Prim::ReduceSum { axis: 0 } } else { Prim::ReduceMax { axis: 0 } };
            line_eval(&reduced, line, dt(0), out_dtype, 1).map(|t| t.data[0])
        }
        Prim::TopK { axis, k } => {
            let a = *axis as usize;
            let ist = strides(&ins[0]);
            let mut line = Vec::with_capacity(ins[0][a]);
            for t in 0..ins[0][a] {
                let mut j = o.clone();
                j[a] = t;
                line.push(ops.get(0, ravel(&j, &ist))?);
            }
            line_eval(&Prim::TopK { axis: 0, k: *k }, line, dt(0), out_dtype, *k as usize).and_then(|t| t.data.get(o[a]).copied())
        }
        Prim::StateWrite { state } => match program.states.get(*state as usize).map(|s| &s.kind) {
            Some(StateKind::Fixed { lo, hi }) => Some(ops.get(0, e)?.clamp(*lo as i128, *hi as i128)),
            _ => None,
        },
        Prim::HistAppend { .. } => {
            let Some((h_cur, h_prev, row)) = sh.hist else { return Ok(None) };
            let (hrow, rest) = ((e / row as u64) as usize, e % row as u64);
            if hrow + 1 == h_cur {
                Some(ops.get(0, rest)?)
            } else {
                // The window slid by one row when it was already full at p − 1 (h_cur == h_prev), else it only grew.
                let delta = 1 + h_prev - h_cur;
                Some(ops.get(1, ((hrow + delta) * row) as u64 + rest)?)
            }
        }
    })
}

/// The shared structural predicate for the allocation-free element lookup and its price.
/// Other Gather axes/batch dimensions retain the general coordinate evaluator.
pub(crate) fn flat_gather_v1(node: &Node, inputs: &[Vec<usize>], out: &[usize]) -> bool {
    matches!(node.prim, Prim::Gather { axis: 0, batch_dims: 0 }) && inputs.len() == 2 && inputs[0].len() == 1 && inputs[1] == out
}

/// The program's per-position structure the courts read: the wiring and the flat index of each occurrence's first node.
struct PreparedV1<'a> {
    w: WiringV1<'a>,
    offsets: Vec<u64>,
    node_count: u64,
    /// `[occurrence][node]`: values never served (`crate::seg_scope`).
    withheld: Vec<Vec<bool>>,
}

impl<'a> PreparedV1<'a> {
    fn new(program: &'a TirProgramV1, encoder: Option<&crate::seg_encoder::EncoderBindingV1>) -> Result<Self, String> {
        let w = match encoder {
            Some(e) => WiringV1::for_stage(program, Some(&e.stage())),
            None => WiringV1::new(program),
        }
        .map_err(|e| format!("the program does not validate: {e}"))?;
        let mut offsets = Vec::with_capacity(w.occurrences.len());
        let mut at = 0u64;
        for (b, _) in &w.occurrences {
            offsets.push(at);
            at += program.blocks[*b as usize].nodes.len() as u64;
        }
        Ok(Self { w, offsets, node_count: at, withheld: crate::seg_scope::seg_withheld_mask_v1(program) })
    }

    fn is_withheld(&self, s: u16, n: u16) -> bool {
        self.withheld.get(s as usize).and_then(|o| o.get(n as usize)).copied().unwrap_or(false)
    }

    fn nodes_in(&self, s: u16) -> Option<u16> {
        let (b, _) = self.w.occurrences.get(s as usize)?;
        Some(self.w.program.blocks[*b as usize].nodes.len() as u16)
    }

    fn valid(&self, s: u16, n: u16) -> bool {
        self.nodes_in(s).is_some_and(|c| n < c)
    }

    fn index(&self, s: u16, n: u16) -> u64 {
        self.offsets[s as usize] + n as u64
    }

    /// The declared type of `(s, n)`'s value at `p`.
    fn declared(&self, s: u16, n: u16, p: u32) -> (DType, Vec<usize>) {
        let node = self.w.node(s, n);
        (node.out.dtype, node.out.resolve(self.w.h(s, p)))
    }

    /// The shapes `(p, s, n)`'s element is evaluated at.
    fn shapes(&self, p: u32, s: u16, n: u16) -> ElementShapesV1 {
        let block = self.w.occurrences[s as usize].0 as usize;
        let node = self.w.node(s, n);
        let h = self.w.h(s, p);
        let mut inputs = Vec::new();
        let mut in_dtypes = Vec::new();
        for r in &node.inputs {
            let t = crate::plan::ref_type(self.w.program, block, r);
            inputs.push(t.resolve(h));
            in_dtypes.push(t.dtype);
        }
        let hist = match node.prim {
            Prim::HistAppend { state } => {
                let row: usize = self.w.program.states[state as usize].shape.iter().map(|d| *d as usize).product();
                let prev = if p == 0 { 0 } else { self.w.h(s, p - 1) };
                if p > 0 {
                    inputs.push(node.out.resolve(prev));
                    in_dtypes.push(node.out.dtype);
                }
                Some((h, prev, row))
            }
            _ => None,
        };
        ElementShapesV1 { out: node.out.resolve(h), inputs, in_dtypes, hist }
    }
}

/// Where an operand of `(p, s, n)` comes from, `i` counting a `HistAppend`'s previous window as operand 1.
#[derive(Clone, Debug, PartialEq, Eq)]
enum OperandSourceV1 {
    Source(SourceV1),
    Token,
    Position,
}

fn operand_sources(pr: &PreparedV1<'_>, p: u32, s: u16, n: u16) -> Result<Vec<OperandSourceV1>, String> {
    let node = pr.w.node(s, n);
    let mut out = Vec::new();
    for (i, r) in node.inputs.iter().enumerate() {
        out.push(match r {
            Ref::Input(j) if *j == INPUT_TOKEN => OperandSourceV1::Token,
            Ref::Input(_) => OperandSourceV1::Position,
            _ => OperandSourceV1::Source(pr.w.input_source(&[], p, s, n, i).map_err(|e| e.to_string())?),
        });
    }
    if matches!(node.prim, Prim::HistAppend { .. }) && p > 0 {
        out.push(OperandSourceV1::Source(SourceV1::Node { position: p - 1, occurrence: s, node: n }));
    }
    Ok(out)
}

/// The court's operand: authenticated elements by flat index, a const, zeros, or one scalar.
enum CourtOperand {
    Leaves(BTreeMap<u64, i128>),
    Full(Tensor),
    Zeros,
    Scalar(i128),
}

struct CourtOperands(Vec<CourtOperand>);

impl OperandsV1 for CourtOperands {
    fn get(&self, operand: usize, flat: u64) -> Result<i128, MissingV1> {
        match self.0.get(operand) {
            Some(CourtOperand::Leaves(m)) => m.get(&flat).copied().ok_or(MissingV1(operand, flat)),
            Some(CourtOperand::Full(t)) => t.data.get(flat as usize).copied().ok_or(MissingV1(operand, flat)),
            Some(CourtOperand::Zeros) => Ok(0),
            Some(CourtOperand::Scalar(v)) if flat == 0 => Ok(*v),
            _ => Err(MissingV1(operand, flat)),
        }
    }
}

/// The elements of authenticated leaves, by flat index; `Err` names the first element two leaves disagree on.
fn leaf_map(leaves: &[LeafOpeningV3]) -> Result<BTreeMap<u64, i128>, u64> {
    let mut m = BTreeMap::new();
    for l in leaves {
        for (e, v) in l.elements().unwrap_or_default() {
            if let Some(old) = m.insert(e, v)
                && old != v
            {
                return Err(e);
            }
        }
    }
    Ok(m)
}

fn header_of(l: &LeafOpeningV3) -> Option<(DType, Vec<usize>)> {
    Some((l.dtype()?, l.shape_usize()?))
}

/// The token a node reads at `p` (the job's prompt, a tile, or a delivered id).
fn token_at(c: &SegClaimContextV1<'_>, p: u32, tile: Option<&PromptTileOpeningV1>) -> Option<u32> {
    if p < c.prompt_len {
        match c.inline_prompt {
            Some(ids) => ids.get(p as usize).copied(),
            None => tile.filter(|t| t.authenticates(c.prompt_len, &c.prompt_root)).and_then(|t| t.id_at(p)),
        }
    } else {
        let r = (p - c.prompt_len) as usize;
        (r + 1 < c.generated.len()).then(|| c.generated[r])
    }
}

/// **The segmented court.** Every opening authenticated against the claim's segment roots or the class's param commitments; nothing
/// read from the producer.
pub fn verify_seg_fault_v1(c: &SegClaimContextV1<'_>, fault: &SegFaultV1) -> Result<SegConvictionV1, DismissalV1> {
    let na = DismissalV1::NotAuthentic;
    let pr = PreparedV1::new(c.program, c.encoder.as_ref()).map_err(na)?;
    let node_ok = |o: &NodeOpeningV1, p: u32, s: u16, n: u16| -> Result<(), DismissalV1> {
        if (o.position, o.occurrence, o.node) != (p, s, n) || p >= c.positions || !pr.valid(s, n) {
            return Err(DismissalV1::NotAuthentic(format!(
                "an opening of ({}, {}, {}) where ({p}, {s}, {n}) is read",
                o.position, o.occurrence, o.node
            )));
        }
        if !o.authenticates(c.segment_roots, c.positions, pr.index(s, n), pr.node_count) {
            return Err(DismissalV1::NotAuthentic(format!("({p}, {s}, {n}): the commitment is not the claim's")));
        }
        Ok(())
    };
    match fault {
        SegFaultV1::Malformed(f) => {
            let o = &f.opening;
            node_ok(o, o.position, o.occurrence, o.node)?;
            let dtype = DType::ALL.into_iter().find(|d| d.tag() == f.dtype).ok_or_else(|| na("an unknown dtype".into()))?;
            let shape: Vec<usize> =
                f.shape.iter().map(|d| usize::try_from(*d)).collect::<Result<_, _>>().map_err(|e| na(e.to_string()))?;
            if commit_v3(dtype, &shape, &f.row_root, &f.col_root) != o.commitment {
                return Err(na("the header is not the committed value's".into()));
            }
            if (dtype, shape) == pr.declared(o.occurrence, o.node, o.position) {
                return Err(DismissalV1::NoFault);
            }
            Ok(SegConvictionV1 { position: o.position, occurrence: o.occurrence, node: o.node, kind: SegConvictionKindV1::Malformed })
        }
        SegFaultV1::Decode(f) => {
            let r = f.index as usize;
            let delivered = *c.generated.get(r).ok_or_else(|| na("no such delivered id".into()))?;
            let p =
                c.prompt_len.checked_sub(1).and_then(|x| x.checked_add(f.index)).ok_or_else(|| na("no selecting position".into()))?;
            let post = (pr.w.occurrences.len() - 1) as u16;
            let logits = c.program.logits;
            let opening = f.logits.node.as_ref().ok_or_else(|| na("the logits are not opened".into()))?;
            node_ok(opening, p, post, logits)?;
            if f.logits.leaves.is_empty() || f.logits.leaves.iter().any(|l| !l.authenticates(&opening.commitment)) {
                return Err(na("a logits leaf is not the committed value's".into()));
            }
            let convicted = |kind| Ok(SegConvictionV1 { position: p, occurrence: post, node: logits, kind });
            let (dtype, shape) = header_of(&f.logits.leaves[0]).ok_or_else(|| na("a malformed leaf".into()))?;
            if (dtype, shape.clone()) != pr.declared(post, logits, p) {
                return convicted(SegConvictionKindV1::Malformed);
            }
            let len = LayoutV3::of(&shape).len;
            if delivered as u64 >= len {
                return convicted(SegConvictionKindV1::Decode { index: f.index });
            }
            let m = match leaf_map(&f.logits.leaves) {
                Ok(m) => m,
                Err(_) => return convicted(SegConvictionKindV1::Inconsistent { input: 0 }),
            };
            match c.decode {
                DecodeRuleV1::Greedy => {
                    if f.rival as u64 >= len || f.rival == delivered {
                        return Err(na("the rival is not another id of the logits".into()));
                    }
                    let (vt, vj) = match (m.get(&(delivered as u64)), m.get(&(f.rival as u64))) {
                        (Some(a), Some(b)) => (*a, *b),
                        _ => return Err(na("the delivered id or the rival is not opened".into())),
                    };
                    if vj > vt || (vj == vt && f.rival < delivered) {
                        convicted(SegConvictionKindV1::Decode { index: f.index })
                    } else {
                        Err(DismissalV1::NoFault)
                    }
                }
            }
        }
        SegFaultV1::WholeValue(f) => {
            let (p, s, n) = (f.position, f.occurrence, f.node);
            node_ok(&f.output, p, s, n)?;
            if !pr.is_withheld(s, n) {
                return Err(na("a whole-value court is not priced for this node".into()));
            }
            let convicted = || Ok(SegConvictionV1 { position: p, occurrence: s, node: n, kind: SegConvictionKindV1::WholeValue });
            let sources = operand_sources(&pr, p, s, n).map_err(na)?;
            if f.inputs.len() != sources.len() {
                return Err(na(format!("{} operands where the node reads {}", f.inputs.len(), sources.len())));
            }
            let mut ops = Vec::with_capacity(sources.len());
            for (i, (src, opening)) in sources.iter().zip(&f.inputs).enumerate() {
                // A committed operand: whole (its commitment recomputed) or by authenticated leaves.
                let committed = |commitment: &Digest, want: (DType, Vec<usize>)| -> Result<CourtOperand, DismissalV1> {
                    if let Some(w) = &opening.whole {
                        let t = w.tensor().ok_or_else(|| na(format!("operand {i}: a malformed whole value")))?;
                        if (t.dtype, t.shape.clone()) != want || tensor_commitment_v3(&t) != *commitment {
                            return Err(na(format!("operand {i}: the whole value is not the committed one")));
                        }
                        return Ok(CourtOperand::Full(t));
                    }
                    if opening.leaves.iter().any(|l| !l.authenticates(commitment) || header_of(l) != Some(want.clone())) {
                        return Err(na(format!("operand {i}: a leaf is not the committed value's")));
                    }
                    leaf_map(&opening.leaves)
                        .map(CourtOperand::Leaves)
                        .map_err(|_| na(format!("operand {i}: two authenticated leaves disagree (file against the source)")))
                };
                let op = match src {
                    OperandSourceV1::Token => CourtOperand::Scalar(
                        token_at(c, p, f.token.as_ref()).ok_or_else(|| na("the token at the position is not opened".into()))? as i128,
                    ),
                    OperandSourceV1::Position => CourtOperand::Scalar(p as i128),
                    OperandSourceV1::Source(SourceV1::Node { position, occurrence, node }) => {
                        let o = opening.node.as_ref().ok_or_else(|| na(format!("operand {i}: its node is not opened")))?;
                        node_ok(o, *position, *occurrence, *node)?;
                        committed(&o.commitment, pr.declared(*occurrence, *node, *position))?
                    }
                    OperandSourceV1::Source(SourceV1::Param { index, layer }) => {
                        let commitment = *c
                            .params
                            .by_instance
                            .get(&(*index, *layer))
                            .ok_or_else(|| na(format!("no commitment for param {index}")))?;
                        let d = &c.program.params[*index as usize];
                        committed(&commitment, (d.dtype, d.shape.iter().map(|x| *x as usize).collect()))?
                    }
                    OperandSourceV1::Source(SourceV1::Const(j)) => {
                        CourtOperand::Full(const_tensor(c.program, *j).map_err(|e| na(e.to_string()))?)
                    }
                    OperandSourceV1::Source(SourceV1::Zeros { .. }) => CourtOperand::Zeros,
                    OperandSourceV1::Source(SourceV1::Public(v)) => CourtOperand::Scalar(*v as i128),
                    OperandSourceV1::Source(SourceV1::Input { k, .. }) => {
                        let e = c.encoder.as_ref().ok_or_else(|| na("a pipeline stage input on a single-program claim".into()))?;
                        if *k == 1 {
                            CourtOperand::Full(e.count(c.prompt_len).map_err(na)?)
                        } else {
                            let prompt: Vec<u32> = match c.inline_prompt {
                                Some(ids) => ids.to_vec(),
                                None => {
                                    let t = f.token.as_ref().ok_or_else(|| na("the job's ids are not opened".into()))?;
                                    if t.index != 0
                                        || t.ids.len() != c.prompt_len as usize
                                        || !t.authenticates(c.prompt_len, &c.prompt_root)
                                    {
                                        return Err(na("the opened ids are not the job's".into()));
                                    }
                                    t.ids.clone()
                                }
                            };
                            CourtOperand::Full(e.ids(&prompt).map_err(na)?)
                        }
                    }
                };
                ops.push(op);
            }
            let sh = pr.shapes(p, s, n);
            let (dtype, shape) = pr.declared(s, n, p);
            let ops = CourtOperands(ops);
            let len = LayoutV3::of(&shape).len;
            let mut data = Vec::with_capacity(len as usize);
            for e in 0..len {
                match element_value_v1(c.program, pr.w.node(s, n), &sh, e, &ops) {
                    Err(MissingV1(i, x)) => return Err(na(format!("operand {i}: element {x} is not opened"))),
                    Ok(None) => return convicted(),
                    Ok(Some(v)) => data.push(v),
                }
            }
            let recomputed = Tensor::new(dtype, shape, data).map_err(|e| na(e.to_string()))?;
            if tensor_commitment_v3(&recomputed) == f.output.commitment { Err(DismissalV1::NoFault) } else { convicted() }
        }
        SegFaultV1::Element(f) => {
            let (p, s, n) = (f.position, f.occurrence, f.node);
            let out_open = f.output.node.as_ref().ok_or_else(|| na("the output is not opened".into()))?;
            node_ok(out_open, p, s, n)?;
            if f.output.leaves.len() != 1 || !f.output.leaves[0].authenticates(&out_open.commitment) {
                return Err(na("the output's leaf is not the committed value's".into()));
            }
            let convicted = |kind| Ok(SegConvictionV1 { position: p, occurrence: s, node: n, kind });
            let leaf = &f.output.leaves[0];
            let (dtype, shape) = header_of(leaf).ok_or_else(|| na("a malformed leaf".into()))?;
            if (dtype, shape) != pr.declared(s, n, p) {
                return convicted(SegConvictionKindV1::Malformed);
            }
            let claimed = leaf_map(std::slice::from_ref(leaf))
                .ok()
                .and_then(|m| m.get(&f.element).copied())
                .ok_or_else(|| na("the output leaf does not hold the element".into()))?;
            let sources = operand_sources(&pr, p, s, n).map_err(na)?;
            if f.inputs.len() != sources.len() {
                return Err(na(format!("{} operands where the node reads {}", f.inputs.len(), sources.len())));
            }
            let mut ops = Vec::with_capacity(sources.len());
            for (i, (src, opening)) in sources.iter().zip(&f.inputs).enumerate() {
                let authenticated =
                    |commitment: &Digest, want: (DType, Vec<usize>)| -> Result<CourtOperand, Result<SegConvictionV1, DismissalV1>> {
                        if opening.leaves.iter().any(|l| !l.authenticates(commitment)) {
                            return Err(Err(na(format!("operand {i}: a leaf is not the committed value's"))));
                        }
                        if opening.leaves.iter().any(|l| header_of(l) != Some(want.clone())) {
                            return Err(Err(na(format!("operand {i} is of another type than its source: file against the source"))));
                        }
                        match leaf_map(&opening.leaves) {
                            Ok(m) => Ok(CourtOperand::Leaves(m)),
                            Err(_) => Err(convicted(SegConvictionKindV1::Inconsistent { input: i as u16 })),
                        }
                    };
                let op = match src {
                    OperandSourceV1::Token => CourtOperand::Scalar(
                        token_at(c, p, f.token.as_ref()).ok_or_else(|| na("the token at the position is not opened".into()))? as i128,
                    ),
                    OperandSourceV1::Position => CourtOperand::Scalar(p as i128),
                    OperandSourceV1::Source(SourceV1::Node { position, occurrence, node }) => {
                        let o = opening.node.as_ref().ok_or_else(|| na(format!("operand {i}: its node is not opened")))?;
                        node_ok(o, *position, *occurrence, *node)?;
                        match authenticated(&o.commitment, pr.declared(*occurrence, *node, *position)) {
                            Ok(op) => op,
                            Err(r) => return r,
                        }
                    }
                    OperandSourceV1::Source(SourceV1::Param { index, layer }) => {
                        let commitment = *c
                            .params
                            .by_instance
                            .get(&(*index, *layer))
                            .ok_or_else(|| na(format!("no commitment for param {index}")))?;
                        let d = &c.program.params[*index as usize];
                        match authenticated(&commitment, (d.dtype, d.shape.iter().map(|x| *x as usize).collect())) {
                            Ok(op) => op,
                            Err(r) => return r,
                        }
                    }
                    OperandSourceV1::Source(SourceV1::Const(j)) => {
                        CourtOperand::Full(const_tensor(c.program, *j).map_err(|e| na(e.to_string()))?)
                    }
                    OperandSourceV1::Source(SourceV1::Zeros { .. }) => CourtOperand::Zeros,
                    OperandSourceV1::Source(SourceV1::Public(v)) => CourtOperand::Scalar(*v as i128),
                    OperandSourceV1::Source(SourceV1::Input { k, .. }) => {
                        // K2-TIR-v5: input 0 is the job's ids (its inline prompt, or its one prompt tile, authenticated); input 1 is
                        // their count, which the job states.
                        let e = c.encoder.as_ref().ok_or_else(|| na("a pipeline stage input on a single-program claim".into()))?;
                        if *k == 1 {
                            CourtOperand::Full(e.count(c.prompt_len).map_err(na)?)
                        } else {
                            let prompt: Vec<u32> = match c.inline_prompt {
                                Some(ids) => ids.to_vec(),
                                None => {
                                    let t = f.token.as_ref().ok_or_else(|| na("the job's ids are not opened".into()))?;
                                    if t.index != 0
                                        || t.ids.len() != c.prompt_len as usize
                                        || !t.authenticates(c.prompt_len, &c.prompt_root)
                                    {
                                        return Err(na("the opened ids are not the job's".into()));
                                    }
                                    t.ids.clone()
                                }
                            };
                            CourtOperand::Full(e.ids(&prompt).map_err(na)?)
                        }
                    }
                };
                ops.push(op);
            }
            let sh = pr.shapes(p, s, n);
            if f.element >= LayoutV3::of(&sh.out).len {
                return Err(na("the element is outside the value".into()));
            }
            match element_value_v1(c.program, pr.w.node(s, n), &sh, f.element, &CourtOperands(ops)) {
                Err(MissingV1(i, e)) => Err(na(format!("operand {i}: element {e} of its dependency line is not opened"))),
                Ok(Some(v)) if v == claimed => Err(DismissalV1::NoFault),
                Ok(_) => convicted(SegConvictionKindV1::Element { element: f.element }),
            }
        }
    }
}

// ---- the prover: a fresh outsider with the material in hand ---------------------------------------------------------------------

/// **Where an outsider reads committed values**: every committed value of a position (derived windows included), and the position
/// root's path to its segment root. Served off-chain, or on chain in answer to a demand.
pub trait SegMaterialV1 {
    /// Every value of position `p` as served (a withheld value may be any tensor: it is never served — `crate::seg_scope`).
    fn position(&self, p: u32) -> Option<Vec<Vec<Tensor>>>;
    fn position_siblings(&self, p: u32) -> Option<Vec<Digest>>;
    /// Every node commitment of position `p` (served beside the values; a withheld value's is all that is served of it). By default
    /// derived from the values.
    fn commitments(&self, p: u32) -> Option<Vec<Vec<Digest>>> {
        let values = self.position(p)?;
        if !values.iter().flatten().all(crate::verify::canonical_tensor_v1) {
            return None;
        }
        Some(values.iter().map(|occ| occ.iter().map(tensor_commitment_v3).collect()).collect())
    }
    /// Position `p`'s root and its path to its segment root, **without its values** where the source keeps them apart (part 0 of a
    /// demand, a stream's position-root list): what the re-execution check descends a segment with (`crate::seg_detect`). By default
    /// it is derived from the values.
    fn position_path(&self, p: u32) -> Option<(Digest, Vec<Digest>)> {
        let commitments = self.commitments(p)?;
        Some((position_root_of_v1(p, &commitments), self.position_siblings(p)?))
    }
}

/// What a check of some positions found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SegFindingV1 {
    Clean,
    Fault(Box<SegFaultV1>),
    /// Positions whose material is missing or does not authenticate: demand them.
    Demand(Vec<u32>),
    /// The verifier could not run (a bug or a missing artifact), never a verdict.
    Inconsistent(String),
}

struct Loaded {
    values: Vec<Vec<Tensor>>,
    commitments: Vec<Vec<Digest>>,
    siblings: Vec<Digest>,
    /// Withheld values the verifier's own value does not authenticate (or it has none): the first one is a whole-value fault.
    unmatched: BTreeSet<(u16, u16)>,
    /// Whether the verifier's own position differs from the committed one anywhere (`None`: it supplied none).
    diverges: Option<bool>,
}

/// The full operands of one node, recording every element the evaluator reads.
struct FullOperands<'a> {
    ops: Vec<Option<&'a Tensor>>,
    scalars: Vec<Option<i128>>,
    reads: RefCell<Vec<BTreeSet<u64>>>,
}

impl OperandsV1 for FullOperands<'_> {
    fn get(&self, operand: usize, flat: u64) -> Result<i128, MissingV1> {
        if let Some(Some(v)) = self.scalars.get(operand) {
            return if flat == 0 { Ok(*v) } else { Err(MissingV1(operand, flat)) };
        }
        let t = self.ops.get(operand).copied().flatten().ok_or(MissingV1(operand, flat))?;
        let v = t.data.get(flat as usize).copied().ok_or(MissingV1(operand, flat))?;
        if let Some(r) = self.reads.borrow_mut().get_mut(operand) {
            r.insert(flat);
        }
        Ok(v)
    }
}

/// The fewest leaves of either tree holding every flat index of `reads` (ties to the row tree).
pub fn leaves_covering_v1(t: &Tensor, trees: &TreesV3, reads: &BTreeSet<u64>) -> Vec<LeafOpeningV3> {
    let (axis, set, _) = cheaper_cover(&trees.layout, t.shape.len(), t.dtype.width() as u64, reads);
    set.into_iter().filter_map(|(line, tile)| LeafOpeningV3::of_trees(t, trees, axis, line, tile)).collect()
}

/// **A fresh outsider's check of `positions`** of a segmented claim, from the material served for them and for the positions they
/// read (`p − 1`), the public artifact (authenticated against the class's v3 commitments) and the token stream. Returns the first
/// fault, as a filing the court convicts.
pub fn check_positions_v1(
    c: &SegClaimContextV1<'_>,
    material: &dyn SegMaterialV1,
    artifact: &dyn Fn(u16, Option<u16>) -> Option<Tensor>,
    tokens: &[u32],
    positions: &[u32],
) -> SegFindingV1 {
    let pr = match PreparedV1::new(c.program, c.encoder.as_ref()) {
        Ok(pr) => pr,
        Err(why) => return SegFindingV1::Inconsistent(why),
    };
    let mut ps: Vec<u32> = positions.to_vec();
    ps.sort_unstable();
    ps.dedup();
    if let Some(p) = ps.iter().find(|p| **p >= c.positions) {
        return SegFindingV1::Inconsistent(format!("position {p} is outside the claim"));
    }
    // Authenticate the available public response before doing another model replay. Adaptive DA retries (including partial
    // responses) must not run the whole prefix each time a requested part is still missing.
    let mut loaded = match load_positions_or_demand(c, &pr, material, &ps) {
        Ok(l) => l,
        Err(missing) => return SegFindingV1::Demand(missing),
    };
    let own = match crate::seg_detect::replay_positions_v1(c, artifact, tokens, &needed_positions(&ps), &pr.withheld) {
        Ok(own) => own,
        Err(why) => return SegFindingV1::Inconsistent(why),
    };
    for (q, mut own) in own {
        let here = loaded.get_mut(&q).expect("every requested position was authenticated");
        here.diverges = Some(own.commitments != here.commitments);
        for s in 0..here.values.len() {
            for n in 0..here.values[s].len() {
                if pr.is_withheld(s as u16, n as u16) {
                    match own.withheld[s][n].take() {
                        Some(t) if own.commitments[s][n] == here.commitments[s][n] => here.values[s][n] = t,
                        _ => {
                            here.unmatched.insert((s as u16, n as u16));
                        }
                    }
                }
            }
        }
    }
    // The public root descent identifies the FIRST divergent position. Do not silently expand a local check into an unbounded
    // prefix read when given a later position: its predecessor may itself require a different court.
    if ps.iter().filter_map(|p| p.checked_sub(1)).any(|q| !ps.contains(&q) && loaded[&q].diverges == Some(true)) {
        return SegFindingV1::Inconsistent("an earlier position differs: localize it with the public root descent".into());
    }
    for &p in &ps {
        match check_one(c, &pr, &loaded, artifact, tokens, p) {
            Ok(None) => {}
            Ok(Some(f)) => return SegFindingV1::Fault(Box::new(f)),
            Err(why) => return SegFindingV1::Inconsistent(why),
        }
    }
    SegFindingV1::Clean
}

type OperandsResolvedV1 = (Vec<OperandSourceV1>, Vec<Option<Tensor>>, Vec<Option<i128>>);

/// Every operand of `(p, s, n)` resolved to a full value (committed values, the authenticated artifact, consts, zeros) or a scalar.
#[allow(clippy::too_many_arguments)]
fn resolve_operands(
    c: &SegClaimContextV1<'_>,
    pr: &PreparedV1<'_>,
    loaded: &BTreeMap<u32, Loaded>,
    artifact: &dyn Fn(u16, Option<u16>) -> Option<Tensor>,
    tokens: &[u32],
    (p, s, n): (u32, u16, u16),
) -> Result<OperandsResolvedV1, String> {
    // Params are read into this relation's owned operands and dropped with them. A model holder can read from disk;
    // a cache of every layer's decoded i128 tensors is not needed for the localized court.
    let sources = operand_sources(pr, p, s, n)?;
    let mut owned: Vec<Option<Tensor>> = Vec::new();
    let mut scalars: Vec<Option<i128>> = Vec::new();
    for src in &sources {
        let (t, v) = match src {
            OperandSourceV1::Token => (None, Some(*tokens.get(p as usize).ok_or("no token at the position")? as i128)),
            OperandSourceV1::Position => (None, Some(p as i128)),
            OperandSourceV1::Source(SourceV1::Node { position, occurrence, node }) => {
                let l = loaded.get(position).ok_or("an operand's position is not loaded")?;
                (Some(l.values[*occurrence as usize][*node as usize].clone()), None)
            }
            OperandSourceV1::Source(SourceV1::Param { index, layer }) => {
                let t = artifact(*index, *layer).ok_or_else(|| format!("the public artifact lacks param {index}"))?;
                if !crate::verify::canonical_tensor_v1(&t)
                    || c.params.by_instance.get(&(*index, *layer)) != Some(&tensor_commitment_v3(&t))
                {
                    return Err(format!("the public artifact's param {index} is not the class's"));
                }
                (Some(t), None)
            }
            OperandSourceV1::Source(SourceV1::Const(j)) => (const_tensor(c.program, *j).ok(), None),
            OperandSourceV1::Source(SourceV1::Zeros { dtype, shape }) => (Some(Tensor::zeros(*dtype, shape)), None),
            OperandSourceV1::Source(SourceV1::Public(v)) => (None, Some(*v as i128)),
            OperandSourceV1::Source(SourceV1::Input { k, .. }) => {
                let e = c.encoder.as_ref().ok_or("a stage input")?;
                let prompt = tokens.get(..c.prompt_len as usize).ok_or("the job's ids are not in hand")?;
                let (ids, count) = e.inputs(prompt)?;
                (Some(if *k == 0 { ids } else { count }), None)
            }
        };
        owned.push(t);
        scalars.push(v);
    }
    Ok((sources, owned, scalars))
}

/// **The filing for element `e` of `(p, s, n)`**: the output's leaf and the leaves covering what the court's evaluator reads.
#[allow(clippy::too_many_arguments)]
fn build_element_fault(
    c: &SegClaimContextV1<'_>,
    pr: &PreparedV1<'_>,
    loaded: &BTreeMap<u32, Loaded>,
    resolved: &OperandsResolvedV1,
    tokens: &[u32],
    (p, s, n): (u32, u16, u16),
    e: u64,
) -> Result<ElementFaultV1, String> {
    let (sources, owned, scalars) = resolved;
    let opening = |q: u32, s: u16, n: u16| -> Option<NodeOpeningV1> {
        let l = loaded.get(&q)?;
        position_node_opening_v1(q, &l.commitments, l.siblings.clone(), s, n)
    };
    let out = &loaded.get(&p).ok_or("the position is not loaded")?.values[s as usize][n as usize];
    let ops = FullOperands {
        ops: owned.iter().map(Option::as_ref).collect(),
        scalars: scalars.clone(),
        reads: RefCell::new(vec![BTreeSet::new(); owned.len()]),
    };
    element_value_v1(c.program, pr.w.node(s, n), &pr.shapes(p, s, n), e, &ops)
        .map_err(|MissingV1(i, f)| format!("({p}, {s}, {n}) operand {i} element {f}"))?;
    let reads = ops.reads.into_inner();
    let out_trees = TreesV3::of(out);
    // The output's one leaf holding `e`, on the tree the price assumes (`cheaper_cover`).
    let out_leaves = leaves_covering_v1(out, &out_trees, &BTreeSet::from([e]));
    if out_leaves.len() != 1 {
        return Err("no output leaf".into());
    }
    let output = OperandOpeningV1 { node: opening(p, s, n), leaves: out_leaves };
    let mut inputs = Vec::new();
    let mut token = None;
    for (i, src) in sources.iter().enumerate() {
        let node_opening = match src {
            OperandSourceV1::Source(SourceV1::Node { position, occurrence, node }) => opening(*position, *occurrence, *node),
            _ => None,
        };
        let leaves = match (src, owned[i].as_ref()) {
            (OperandSourceV1::Source(SourceV1::Node { .. } | SourceV1::Param { .. }), Some(t)) => {
                leaves_covering_v1(t, &TreesV3::of(t), &reads[i])
            }
            _ => Vec::new(),
        };
        if matches!(src, OperandSourceV1::Token) && p < c.prompt_len && c.inline_prompt.is_none() {
            token = PromptTileOpeningV1::of(&tokens[..c.prompt_len as usize], p / PROMPT_TILE_IDS_V1 as u32);
        }
        // K2-TIR-v5: the job's ids are opened by the prompt's one tile (`L ≤ 4,096`).
        if matches!(src, OperandSourceV1::Source(SourceV1::Input { k: 0, .. })) && c.encoder.is_some() && c.inline_prompt.is_none() {
            token = PromptTileOpeningV1::of(&tokens[..c.prompt_len as usize], 0);
        }
        inputs.push(OperandOpeningV1 { node: node_opening, leaves });
    }
    Ok(ElementFaultV1 { position: p, occurrence: s, node: n, element: e, output, inputs, token })
}

/// **The whole-value filing for `(p, s, n)`**: the output's node opening and, per operand, what every element of the value reads — a
/// withheld committed operand whole (the verifier's own value, which matches its commitment), a served one by the leaves covering its
/// reads, a param by leaves from the filer's own copy, the token or the job's ids by their tile.
#[allow(clippy::too_many_arguments)]
fn build_whole_value_fault(
    c: &SegClaimContextV1<'_>,
    pr: &PreparedV1<'_>,
    loaded: &BTreeMap<u32, Loaded>,
    resolved: &OperandsResolvedV1,
    tokens: &[u32],
    (p, s, n): (u32, u16, u16),
) -> Result<WholeValueFaultV1, String> {
    let (sources, owned, scalars) = resolved;
    let opening = |q: u32, s: u16, n: u16| -> Option<NodeOpeningV1> {
        let l = loaded.get(&q)?;
        position_node_opening_v1(q, &l.commitments, l.siblings.clone(), s, n)
    };
    let sh = pr.shapes(p, s, n);
    let ops = FullOperands {
        ops: owned.iter().map(Option::as_ref).collect(),
        scalars: scalars.clone(),
        reads: RefCell::new(vec![BTreeSet::new(); owned.len()]),
    };
    for e in 0..LayoutV3::of(&sh.out).len {
        element_value_v1(c.program, pr.w.node(s, n), &sh, e, &ops)
            .map_err(|MissingV1(i, f)| format!("({p}, {s}, {n}) operand {i} element {f}"))?;
    }
    let reads = ops.reads.into_inner();
    let mut inputs = Vec::new();
    let mut token = None;
    for (i, src) in sources.iter().enumerate() {
        let mut operand = WholeOperandV1::default();
        match (src, owned[i].as_ref()) {
            (OperandSourceV1::Source(SourceV1::Node { position, occurrence, node }), Some(t)) => {
                operand.node = opening(*position, *occurrence, *node);
                if pr.is_withheld(*occurrence, *node) {
                    operand.whole = Some(WholeTensorV1::of(t));
                } else {
                    operand.leaves = leaves_covering_v1(t, &TreesV3::of(t), &reads[i]);
                }
            }
            (OperandSourceV1::Source(SourceV1::Param { .. }), Some(t)) => {
                operand.leaves = leaves_covering_v1(t, &TreesV3::of(t), &reads[i]);
            }
            _ => {}
        }
        if matches!(src, OperandSourceV1::Token) && p < c.prompt_len && c.inline_prompt.is_none() {
            token = PromptTileOpeningV1::of(&tokens[..c.prompt_len as usize], p / PROMPT_TILE_IDS_V1 as u32);
        }
        if matches!(src, OperandSourceV1::Source(SourceV1::Input { k: 0, .. })) && c.encoder.is_some() && c.inline_prompt.is_none() {
            token = PromptTileOpeningV1::of(&tokens[..c.prompt_len as usize], 0);
        }
        inputs.push(operand);
    }
    Ok(WholeValueFaultV1 {
        position: p,
        occurrence: s,
        node: n,
        output: opening(p, s, n).ok_or("no opening of the value")?,
        inputs,
        token,
    })
}

/// **The filing for ANY element** of a committed value (right or wrong) — what a prover sends, and what a soundness test puts to the
/// court on an honest claim (it must dismiss every one).
pub fn prove_element_v1(
    c: &SegClaimContextV1<'_>,
    material: &dyn SegMaterialV1,
    artifact: &dyn Fn(u16, Option<u16>) -> Option<Tensor>,
    tokens: &[u32],
    (p, s, n): (u32, u16, u16),
    e: u64,
) -> Result<ElementFaultV1, String> {
    let pr = PreparedV1::new(c.program, c.encoder.as_ref())?;
    if !pr.valid(s, n) || p >= c.positions {
        return Err("no such value".into());
    }
    let loaded = load_opened_positions_for_element(c, &pr, material, &[p])?;
    let resolved = resolve_operands(c, &pr, &loaded, artifact, tokens, (p, s, n))?;
    build_element_fault(c, &pr, &loaded, &resolved, tokens, (p, s, n), e)
}

/// Proof construction may use fully opened values after authenticating EVERY value against the public root. This helper never
/// decides that a claim is correct; `check_positions_v1` must instead replay the registered model, including withheld values.
fn load_opened_positions_for_element(
    c: &SegClaimContextV1<'_>,
    pr: &PreparedV1<'_>,
    material: &dyn SegMaterialV1,
    ps: &[u32],
) -> Result<BTreeMap<u32, Loaded>, String> {
    let mut loaded = BTreeMap::new();
    for q in needed_positions(ps) {
        let values = material.position(q).ok_or_else(|| format!("position {q} is not fully opened"))?;
        let siblings = material.position_siblings(q).ok_or_else(|| format!("position {q} has no path"))?;
        if values.len() != pr.w.occurrences.len()
            || values.iter().enumerate().any(|(s, occ)| pr.nodes_in(s as u16) != Some(occ.len() as u16))
            || values.iter().flatten().any(|t| !crate::verify::canonical_tensor_v1(t))
        {
            return Err(format!("position {q} is malformed"));
        }
        let commitments = values.iter().map(|o| o.iter().map(tensor_commitment_v3).collect()).collect::<Vec<_>>();
        if !position_in_segment(q, &position_root_of_v1(q, &commitments), &siblings, c.segment_roots, c.positions) {
            return Err(format!("position {q}'s opened values do not authenticate"));
        }
        loaded.insert(q, Loaded { values, commitments, siblings, unmatched: BTreeSet::new(), diverges: None });
    }
    Ok(loaded)
}

fn needed_positions(ps: &[u32]) -> BTreeSet<u32> {
    ps.iter().flat_map(|p| std::iter::once(*p).chain(p.checked_sub(1))).collect()
}

fn load_positions_or_demand(
    c: &SegClaimContextV1<'_>,
    pr: &PreparedV1<'_>,
    material: &dyn SegMaterialV1,
    ps: &[u32],
) -> Result<BTreeMap<u32, Loaded>, Vec<u32>> {
    let need = needed_positions(ps);
    let mut loaded = BTreeMap::new();
    let mut missing = Vec::new();
    for q in need {
        let (Some(values), Some(siblings), Some(commitments)) =
            (material.position(q), material.position_siblings(q), material.commitments(q))
        else {
            missing.push(q);
            continue;
        };
        let shape_ok = |v: &[Vec<Tensor>]| {
            v.len() == pr.w.occurrences.len() && v.iter().enumerate().all(|(s, occ)| pr.nodes_in(s as u16) == Some(occ.len() as u16))
        };
        let commitments_ok = commitments.len() == pr.w.occurrences.len()
            && commitments.iter().enumerate().all(|(s, occ)| pr.nodes_in(s as u16) == Some(occ.len() as u16));
        if !shape_ok(&values) || !commitments_ok {
            missing.push(q);
            continue;
        }
        if !position_in_segment(q, &position_root_of_v1(q, &commitments), &siblings, c.segment_roots, c.positions) {
            missing.push(q);
            continue;
        }
        // A served (clear) value must be its commitment's; a withheld one is never served.
        let served_ok = values.iter().enumerate().all(|(s, occ)| {
            occ.iter().enumerate().all(|(n, t)| {
                pr.is_withheld(s as u16, n as u16)
                    || (crate::verify::canonical_tensor_v1(t) && tensor_commitment_v3(t) == commitments[s][n])
            })
        });
        if !served_ok {
            missing.push(q);
            continue;
        }
        loaded.insert(q, Loaded { values, commitments, siblings, unmatched: BTreeSet::new(), diverges: None });
    }
    if missing.is_empty() { Ok(loaded) } else { Err(missing) }
}

#[allow(clippy::too_many_arguments)]
fn check_one(
    c: &SegClaimContextV1<'_>,
    pr: &PreparedV1<'_>,
    loaded: &BTreeMap<u32, Loaded>,
    artifact: &dyn Fn(u16, Option<u16>) -> Option<Tensor>,
    tokens: &[u32],
    p: u32,
) -> Result<Option<SegFaultV1>, String> {
    let here = &loaded[&p];
    let opening = |q: u32, s: u16, n: u16| -> Option<NodeOpeningV1> {
        let l = loaded.get(&q)?;
        position_node_opening_v1(q, &l.commitments, l.siblings.clone(), s, n)
    };
    for s in 0..pr.w.occurrences.len() as u16 {
        for n in 0..pr.nodes_in(s).unwrap_or(0) {
            let node = pr.w.node(s, n);
            if pr.is_withheld(s, n) {
                // A withheld value: the verifier's own value authenticates it, or it is the first one that does not.
                if here.unmatched.contains(&(s, n)) {
                    if here.diverges.is_none() {
                        return Err(format!("({p}, {s}, {n}) is withheld and the verifier has no value of its own to check it with"));
                    }
                    let resolved = resolve_operands(c, pr, loaded, artifact, tokens, (p, s, n))?;
                    return Ok(Some(SegFaultV1::WholeValue(build_whole_value_fault(c, pr, loaded, &resolved, tokens, (p, s, n))?)));
                }
                continue;
            }
            let out = &here.values[s as usize][n as usize];
            if (out.dtype, out.shape.clone()) != pr.declared(s, n, p) {
                let trees = TreesV3::of(out);
                return Ok(Some(SegFaultV1::Malformed(MalformedFaultV1 {
                    opening: opening(p, s, n).ok_or("no opening of the value")?,
                    dtype: out.dtype.tag(),
                    shape: out.shape.iter().map(|d| *d as u64).collect(),
                    row_root: trees.row_root(),
                    col_root: trees.col_root(),
                })));
            }
            let mut resolved = resolve_operands(c, pr, loaded, artifact, tokens, (p, s, n))?;
            let (_, owned, scalars) = &mut resolved;
            // Fast path: the whole value at once.
            let expected = match node.prim {
                Prim::HistAppend { .. } => {
                    let row = owned[0].as_ref().ok_or("no appended row")?;
                    let mut data = Vec::new();
                    if p > 0 {
                        let (h_cur, h_prev) = (pr.w.h(s, p), pr.w.h(s, p - 1));
                        let prev = owned.get(1).and_then(Option::as_ref).ok_or("no previous window")?;
                        let skip = (1 + h_prev - h_cur) * row.len();
                        data.extend_from_slice(prev.data.get(skip..).unwrap_or_default());
                    }
                    data.extend_from_slice(&row.data);
                    Some(data)
                }
                _ => {
                    let inputs: Vec<Tensor> = owned
                        .iter_mut()
                        .zip(scalars.iter())
                        .map(|(t, v)| {
                            t.take().unwrap_or_else(|| Tensor { dtype: DType::Idx, shape: Vec::new(), data: vec![v.unwrap_or(0)] })
                        })
                        .collect();
                    let expected = eval_node(c.program, node, &inputs, &[], pr.w.h(s, p)).ok().map(|t| t.data);
                    // Return the same allocations to the prover, which needs them if a mismatch is found. Cloning these
                    // operands doubled the largest embedding matrix's live memory for no evaluation benefit.
                    for ((slot, scalar), input) in owned.iter_mut().zip(scalars.iter()).zip(inputs) {
                        if scalar.is_none() {
                            *slot = Some(input);
                        }
                    }
                    expected
                }
            };
            if expected.as_ref() == Some(&out.data) {
                continue;
            }
            // Slow path: the first element the court's own evaluator disagrees with.
            let sh = pr.shapes(p, s, n);
            let ops = FullOperands {
                ops: owned.iter().map(Option::as_ref).collect(),
                scalars: scalars.clone(),
                reads: RefCell::new(vec![BTreeSet::new(); owned.len()]),
            };
            let mut found = None;
            for e in 0..out.len() as u64 {
                match element_value_v1(c.program, node, &sh, e, &ops) {
                    Ok(Some(v)) if v == out.data[e as usize] => continue,
                    Ok(_) => {
                        found = Some(e);
                        break;
                    }
                    Err(MissingV1(i, f)) => return Err(format!("({p}, {s}, {n}) operand {i} element {f}")),
                }
            }
            let Some(e) = found else {
                return Err(format!("({p}, {s}, {n}) disagrees as a whole and in no element"));
            };
            return Ok(Some(SegFaultV1::Element(build_element_fault(c, pr, loaded, &resolved, tokens, (p, s, n), e)?)));
        }
    }
    // The decode relation at a selecting position.
    if p + 1 >= c.prompt_len {
        let r = (p + 1 - c.prompt_len) as usize;
        if let Some(&delivered) = c.generated.get(r) {
            let post = (pr.w.occurrences.len() - 1) as u16;
            let logits = &here.values[post as usize][c.program.logits as usize];
            let chosen = c.decode.select(logits);
            if chosen != Some(delivered) {
                let rival = chosen.unwrap_or(0);
                let trees = TreesV3::of(logits);
                let reads: BTreeSet<u64> =
                    [delivered as u64, rival as u64].into_iter().filter(|e| (*e as usize) < logits.len()).collect();
                let leaves = if reads.is_empty() {
                    vec![LeafOpeningV3::of_trees(logits, &trees, AXIS_ROW, 0, 0).ok_or("empty logits")?]
                } else {
                    leaves_covering_v1(logits, &trees, &reads)
                };
                return Ok(Some(SegFaultV1::Decode(SegDecodeFaultV1 {
                    index: r as u32,
                    rival,
                    logits: OperandOpeningV1 { node: opening(p, post, c.program.logits), leaves },
                })));
            }
        }
    }
    Ok(None)
}

// ---- pricing: what one element court of a relation can cost ---------------------------------------------------------------------

/// Bytes of one node opening at the worst depth, a fault's fixed fields, and a prompt tile with its path.
pub fn node_opening_bytes_v1(node_count: u64) -> u64 {
    8 + 64 * (2 + crate::merkle::depth(node_count) + crate::merkle::depth(crate::seg::SEG_LEN_V4 as u64)) + 16
}

const FAULT_FIXED_BYTES_V1: u64 = 64;
const HASH_WORK_V1: u64 = 64;

/// A dummy operand source: every element zero, every read counted.
struct CountingOperands {
    shapes: Vec<Vec<usize>>,
    reads: RefCell<Vec<BTreeSet<u64>>>,
}

impl OperandsV1 for CountingOperands {
    fn get(&self, operand: usize, flat: u64) -> Result<i128, MissingV1> {
        let len = self.shapes.get(operand).map(|s| LayoutV3::of(s).len).unwrap_or(1);
        if flat >= len {
            return Err(MissingV1(operand, flat));
        }
        if let Some(r) = self.reads.borrow_mut().get_mut(operand) {
            r.insert(flat);
        }
        Ok(0)
    }
}

/// The leaves of tree `axis` covering `reads` of a tensor laid out as `l` (rank `rank`, `width` bytes an element), and the bytes they
/// file: each leaf priced as [`LeafOpeningV3::byte_len`] at the tree's full depth, an upper bound of its wire form.
fn axis_cover(l: &LayoutV3, rank: usize, width: u64, axis: u8, reads: &BTreeSet<u64>) -> (BTreeSet<(u64, u64)>, u64) {
    let set: BTreeSet<(u64, u64)> = reads
        .iter()
        .filter_map(|e| if axis == AXIS_ROW { l.row_leaf_of(*e) } else { l.col_leaf_of(*e) })
        .map(|(a, b, _)| (a, b))
        .collect();
    let path = (crate::merkle::depth(l.leaves(axis)) + 1) * 64 + 8 * rank as u64 + 32;
    let bytes = set.iter().map(|(line, tile)| l.leaf_element_count(axis, *line, *tile).unwrap_or(0) * width + path).sum();
    (set, bytes)
}

/// **The tree a filing opens for `reads`** (`(axis, leaves, bytes)`): the one whose covering leaves file fewer bytes, the row tree on a
/// tie. The prover ([`leaves_covering_v1`], every operand and the output) and the price ([`element_court_cost_in_v1`]) use this one
/// rule, so what the gate holds against the carrier is what a prover files.
fn cheaper_cover(l: &LayoutV3, rank: usize, width: u64, reads: &BTreeSet<u64>) -> (u8, BTreeSet<(u64, u64)>, u64) {
    let (rows, rb) = axis_cover(l, rank, width, AXIS_ROW, reads);
    let (cols, cb) = axis_cover(l, rank, width, AXIS_COL, reads);
    if cb < rb { (AXIS_COL, cols, cb) } else { (AXIS_ROW, rows, rb) }
}

/// The leaves (count, bytes) covering `reads` of a tensor of `shape` and `dtype`, on the tree the prover opens.
fn cover_cost(shape: &[usize], dtype: DType, reads: &BTreeSet<u64>) -> (u64, u64) {
    let (_, set, bytes) = cheaper_cover(&LayoutV3::of(shape), shape.len(), dtype.width() as u64, reads);
    (set.len() as u64, bytes)
}

/// An operand's opening besides its node opening and leaves: the `Option` tag and the leaf vector's length.
const OPERAND_SHELL_BYTES_V1: u64 = 5;

/// A prompt tile of `ids` ids with `siblings` siblings, as filed: the `Some` tag, the index, the ids' and the siblings' vectors.
pub fn prompt_tile_bytes_v1(ids: u64, siblings: u64) -> u64 {
    1 + 4 + (4 + 4 * ids) + (4 + 64 * siblings)
}

/// **What one element court of a K2-TIR-v4 relation can cost** (`(bytes, work)`), at the history `min(window, max_positions)`. Bytes are
/// those of the filing's wire form (`SegFaultV1::Element`), and are an upper bound of every element's filing: the first and the last
/// element are priced (every tile is full but the last of a line, so they hold the widest leaves), a `Concat` prices a leaf of every
/// input (an element reads one of them), every operand's opening is priced whether or not the element reads it, and the tree of every
/// leaf is the prover's ([`cheaper_cover`]).
pub fn element_court_cost_v1(program: &TirProgramV1, block: usize, node: usize, node_count: u64, max_positions: u32) -> (u64, u64) {
    element_court_cost_in_v1(program, block, node, node_count, max_positions, None)
}

/// [`element_court_cost_v1`] for a class whose program has job-bound inputs (K2-TIR-v5, [`crate::seg_encoder`]): the ids are filed as
/// the job's one prompt tile (`L` ids, no sibling), the count as nothing (the job states it).
pub fn element_court_cost_in_v1(
    program: &TirProgramV1,
    block: usize,
    node: usize,
    node_count: u64,
    max_positions: u32,
    encoder: Option<&crate::seg_encoder::EncoderBindingV1>,
) -> (u64, u64) {
    element_court_cost_masked_v1(
        program,
        block,
        node,
        node_count,
        max_positions,
        encoder,
        &crate::seg_scope::seg_withheld_mask_v1(program),
    )
}

/// [`element_court_cost_in_v1`] with the program's withheld mask (`crate::seg_scope::seg_withheld_mask_v1`) computed once by the caller.
pub fn element_court_cost_masked_v1(
    program: &TirProgramV1,
    block: usize,
    node: usize,
    node_count: u64,
    max_positions: u32,
    encoder: Option<&crate::seg_encoder::EncoderBindingV1>,
    withheld: &[Vec<bool>],
) -> (u64, u64) {
    let element = element_court_cost_core(program, block, node, node_count, max_positions, encoder);
    // A withheld value is also judged by its whole-value court (`crate::seg_scope`): the price is the larger.
    let mut worst = element;
    for (s, (b, _)) in program.occurrences().iter().enumerate() {
        if *b as usize == block && withheld[s].get(node).copied().unwrap_or(false) {
            let whole = crate::seg_scope::whole_value_court_cost_v1(program, s, node, node_count);
            worst = (worst.0.max(whole.0), worst.1.max(whole.1));
        }
    }
    worst
}

fn element_court_cost_core(
    program: &TirProgramV1,
    block: usize,
    node: usize,
    node_count: u64,
    max_positions: u32,
    encoder: Option<&crate::seg_encoder::EncoderBindingV1>,
) -> (u64, u64) {
    let n = &program.blocks[block].nodes[node];
    let h = crate::plan::worst_h(program, block).min(max_positions.max(1) as usize);
    // A tiled prompt holds at most `max_positions` ids: a tile has at most 4,096 of them and the tile tree's depth of siblings.
    let token_tile = prompt_tile_bytes_v1(
        PROMPT_TILE_IDS_V1.min(max_positions.max(1) as usize) as u64,
        crate::merkle::depth(crate::seg::prompt_tiles_v1(max_positions.max(1)) as u64),
    );
    let job_tile = encoder.map_or(0, |b| prompt_tile_bytes_v1(b.l as u64, 0));
    let mut shapes = Vec::new();
    let mut dtypes = Vec::new();
    // What each input is filed as: 1 a committed value (its node opening, and the leaves the element reads), 2 a param (the leaves
    // read), 3 the per-position token (a prompt tile), 4 K2-TIR-v5's job ids (the job's one prompt tile), 0 the operand's shell alone
    // (a const, zeros, a public scalar, K2-TIR-v5's count).
    let mut kinds = Vec::new();
    for r in &n.inputs {
        let t = crate::plan::ref_type(program, block, r);
        shapes.push(t.resolve(h));
        dtypes.push(t.dtype);
        kinds.push(match r {
            Ref::Node(_) | Ref::CarryIn(_) | Ref::State(_) => 1u8,
            Ref::Param(j) => match encoder {
                Some(b) if *j == b.first_input => 4,
                Some(b) if *j == b.first_input + 1 => 0,
                _ => 2,
            },
            Ref::Input(j) if *j == INPUT_TOKEN => 3,
            _ => 0,
        });
    }
    let hist = match n.prim {
        Prim::HistAppend { state } => {
            let row: usize = program.states[state as usize].shape.iter().map(|d| *d as usize).product();
            shapes.push(n.out.resolve(h));
            dtypes.push(n.out.dtype);
            kinds.push(1);
            Some((h, h, row))
        }
        _ => None,
    };
    let out = n.out.resolve(h);
    let sh = ElementShapesV1 { out: out.clone(), inputs: shapes.clone(), in_dtypes: dtypes.clone(), hist };
    let len = LayoutV3::of(&out).len;
    let node_bytes = node_opening_bytes_v1(node_count);
    let out_leaf = cover_cost(&out, n.out.dtype, &BTreeSet::from([0u64])).1;
    let mut worst = (0u64, 0u64);
    for e in [0, len.saturating_sub(1)] {
        let ops = CountingOperands { shapes: shapes.clone(), reads: RefCell::new(vec![BTreeSet::new(); shapes.len()]) };
        let _ = element_value_v1(program, n, &sh, e, &ops);
        let mut reads = ops.reads.into_inner();
        if matches!(n.prim, Prim::Concat { .. }) {
            for r in reads.iter_mut() {
                r.insert(0);
            }
        }
        let mut bytes = FAULT_FIXED_BYTES_V1 + node_bytes + out_leaf;
        let mut work = out_leaf / 16 + HASH_WORK_V1 * (crate::merkle::depth(node_count) + 12);
        for (i, r) in reads.iter().enumerate() {
            // The node opening is filed whether or not the element reads the value (its node is an operand of the relation).
            bytes += match kinds[i] {
                1 => node_bytes,
                2 => 8,
                3 => OPERAND_SHELL_BYTES_V1 + token_tile,
                4 => OPERAND_SHELL_BYTES_V1 + job_tile,
                _ => OPERAND_SHELL_BYTES_V1,
            };
            if matches!(kinds[i], 1 | 2) && !r.is_empty() {
                let (leaves, b) = cover_cost(&shapes[i], dtypes[i], r);
                bytes += b;
                work += r.len() as u64 + leaves * HASH_WORK_V1 * 24;
            }
        }
        worst = (bytes.max(worst.0), work.max(worst.1));
    }
    worst
}

/// **What a decode filing of a class can cost** (`(bytes, work)`): the logits' node opening and the leaves holding the delivered id and
/// its rival — at most two leaves of the tree the prover opens, each at most a full row leaf. `(0, 0)` for a program with no logits.
pub fn decode_court_cost_v1(program: &TirProgramV1, node_count: u64, max_positions: u32) -> (u64, u64) {
    let Some((b, _)) = program.occurrences().last().copied() else { return (0, 0) };
    let Some(node) = program.blocks.get(b as usize).and_then(|blk| blk.nodes.get(program.logits as usize)) else { return (0, 0) };
    let h = crate::plan::worst_h(program, b as usize).min(max_positions.max(1) as usize);
    let shape = node.out.resolve(h);
    let width = node.out.dtype.width() as u64;
    let row = axis_cover(&LayoutV3::of(&shape), shape.len(), width, AXIS_ROW, &BTreeSet::from([0u64])).1;
    // The variant tag, the index and the rival, and the opening's shell around the node opening.
    let bytes = 1 + 8 + OPERAND_SHELL_BYTES_V1 + node_opening_bytes_v1(node_count) + 2 * row;
    let work = 2 * (crate::merkle3::TILE_V3 + HASH_WORK_V1 * 24) + HASH_WORK_V1 * (crate::merkle::depth(node_count) + 12);
    (bytes, work)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocation_free_leaf_pricing_matches_index_vector_oracle_on_real_weight_shapes() {
        let cases = [
            (vec![], BTreeSet::from([0])),
            (vec![0], BTreeSet::from([0])),
            (vec![4097], BTreeSet::from([0, 4095, 4096, 4097])),
            (vec![3, 8193], (0..8193).step_by(997).collect()),
            (vec![2, 4097, 3], (0..24_582).step_by(997).collect()),
            // The actual pilot's vocabulary projection: a full strided MatMul column.
            (vec![1536, 151_936], (0..1536).map(|k| k * 151_936).collect()),
            // The actual embedding shape: a Gather reads one complete 1,536-value row.
            (vec![151_936, 1536], (0..1536).collect()),
        ];
        for (shape, reads) in cases {
            let l = LayoutV3::of(&shape);
            for dtype in DType::ALL {
                for axis in [AXIS_ROW, AXIS_COL] {
                    let leaves: BTreeSet<(u64, u64)> = reads
                        .iter()
                        .filter_map(|e| if axis == AXIS_ROW { l.row_leaf_of(*e) } else { l.col_leaf_of(*e) })
                        .map(|(a, b, _)| (a, b))
                        .collect();
                    let path = (crate::merkle::depth(l.leaves(axis)) + 1) * 64 + 8 * shape.len() as u64 + 32;
                    let original: u64 = leaves
                        .iter()
                        .map(|(line, tile)| l.leaf_elements(axis, *line, *tile).unwrap().len() as u64 * dtype.width() as u64 + path)
                        .sum();
                    assert_eq!(axis_cover(&l, shape.len(), dtype.width() as u64, axis, &reads), (leaves, original));
                }
            }
        }
    }

    #[test]
    fn allocation_free_pricing_preserves_the_exact_actual_32_position_plan_root() {
        let bytes =
            include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/design/palw/tir/evidence/qwen25-real-pilot-program.tir"));
        let program = TirProgramV1::decode_canonical(bytes).unwrap();
        let descriptor = crate::descriptor::k2_tir_v4_descriptor();
        let plan = crate::plan::plan_for_tir_program_v1(&descriptor, &program, crate::public::program_root_v1(bytes), 32).unwrap();
        let hex: String = plan.root().iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "0e31e5b93b6088bcb5b2b7a2e61faa9aecee0c9d62f9fe51a5d303367a3e762cec4b4a9b79c237bbb7e760b6d2679ecad9182f97088e29d4b3443d999fcd2dd2"
        );
    }
    use crate::seg::{SegmentedCommitmentsV1, prompt_root_of_ids_v1};
    use crate::trace::trace_v1;
    use misaka_palw_tir::MapParams;
    use misaka_palw_tir_sketch::fixture::dense_moe_v1;

    /// Deliberately retains the old coordinate algorithm as a differential oracle. The
    /// independently implemented whole-tensor TIR evaluator is checked as well.
    fn coordinate_gather(node: &Node, sh: &ElementShapesV1, e: u64, ops: &dyn OperandsV1) -> Result<Option<i128>, MissingV1> {
        let Prim::Gather { axis, batch_dims } = node.prim else { panic!("not Gather") };
        let (a, b) = (axis as usize, batch_dims as usize);
        let o = unravel(e, &strides(&sh.out));
        let m = sh.inputs[1].len() - b;
        let mut xi = o[..b].to_vec();
        xi.extend_from_slice(&o[a..a + m]);
        let v = ops.get(1, ravel(&xi, &strides(&sh.inputs[1])))?;
        if v < 0 || v >= sh.inputs[0][a] as i128 {
            return Ok(None);
        }
        let mut di = o[..a].to_vec();
        di.push(v as usize);
        di.extend_from_slice(&o[a + m..]);
        Ok(Some(ops.get(0, ravel(&di, &strides(&sh.inputs[0])))?))
    }

    struct LookupOperands {
        tensors: Vec<Tensor>,
        missing: Option<MissingV1>,
        reads: RefCell<Vec<(usize, u64)>>,
    }
    impl OperandsV1 for LookupOperands {
        fn get(&self, i: usize, e: u64) -> Result<i128, MissingV1> {
            self.reads.borrow_mut().push((i, e));
            if self.missing == Some(MissingV1(i, e)) {
                return Err(MissingV1(i, e));
            }
            self.tensors.get(i).and_then(|t| t.data.get(e as usize)).copied().ok_or(MissingV1(i, e))
        }
    }

    #[test]
    fn flat_gather_preserves_results_and_dependency_reads_for_all_dtypes_and_ranks() {
        let program = dense_moe_v1(7).program;
        for dtype in DType::ALL {
            let data = Tensor::new(dtype, vec![7], vec![dtype.min_value(), dtype.max_value(), 0, 1, 2, 3, 4]).unwrap();
            for shape in [vec![], vec![19], vec![3, 5], vec![2, 3, 4], vec![2, 2, 3, 4]] {
                let len = shape.iter().product::<usize>();
                let indices = Tensor::new(DType::Idx, shape.clone(), (0..len).map(|e| ((e * 11 + 3) % 7) as i128).collect()).unwrap();
                let node = Node {
                    prim: Prim::Gather { axis: 0, batch_dims: 0 },
                    inputs: vec![Ref::Param(0), Ref::Param(1)],
                    out: misaka_palw_tir::TensorType::fixed(dtype, &shape.iter().map(|n| *n as u32).collect::<Vec<_>>()),
                    commit: true,
                };
                let sh = ElementShapesV1 {
                    out: shape.clone(),
                    inputs: vec![data.shape.clone(), shape.clone()],
                    in_dtypes: vec![dtype, DType::Idx],
                    hist: None,
                };
                let expected = eval_primitive(&node.prim, &[data.clone(), indices.clone()], dtype, &shape).unwrap();
                let ops = LookupOperands { tensors: vec![data.clone(), indices], missing: None, reads: RefCell::new(vec![]) };
                assert!(flat_gather_v1(&node, &sh.inputs, &sh.out));
                for e in 0..len as u64 {
                    ops.reads.borrow_mut().clear();
                    let fast = element_value_v1(&program, &node, &sh, e, &ops);
                    let reads = ops.reads.take();
                    let old = coordinate_gather(&node, &sh, e, &ops);
                    assert_eq!(fast, old, "{dtype:?} {shape:?} {e}");
                    assert_eq!(fast, Ok(Some(expected.data[e as usize])));
                    assert_eq!(reads, ops.reads.take(), "proof dependencies changed");
                    assert_eq!(reads.len(), 2);
                }
            }
        }
    }

    #[test]
    fn flat_gather_refuses_invalid_indices_before_table_reads_and_preserves_missing_operands() {
        let program = dense_moe_v1(7).program;
        let node = Node {
            prim: Prim::Gather { axis: 0, batch_dims: 0 },
            inputs: vec![Ref::Param(0), Ref::Param(1)],
            out: misaka_palw_tir::TensorType::fixed(DType::I16, &[6]),
            commit: true,
        };
        let sh =
            ElementShapesV1 { out: vec![6], inputs: vec![vec![3], vec![6]], in_dtypes: vec![DType::I16, DType::I128], hist: None };
        let tensors = vec![
            Tensor::new(DType::I16, vec![3], vec![-7, 0, 32767]).unwrap(),
            Tensor::new(DType::I128, vec![6], vec![-1, 3, i128::MIN, i128::MAX, 0, 2]).unwrap(),
        ];
        for missing in [None, Some(MissingV1(1, 0)), Some(MissingV1(1, 5)), Some(MissingV1(0, 2))] {
            let ops = LookupOperands { tensors: tensors.clone(), missing, reads: RefCell::new(vec![]) };
            for e in 0..6 {
                ops.reads.borrow_mut().clear();
                let fast = element_value_v1(&program, &node, &sh, e, &ops);
                let reads = ops.reads.take();
                assert_eq!(fast, coordinate_gather(&node, &sh, e, &ops));
                assert_eq!(reads, ops.reads.take());
                if e < 4 {
                    assert!(reads.iter().all(|(operand, _)| *operand == 1));
                }
            }
        }
        // Neither an axis/batch variant nor a mismatched output shape enters the specialization.
        for prim in [Prim::Gather { axis: 1, batch_dims: 0 }, Prim::Gather { axis: 0, batch_dims: 1 }, Prim::Reshape] {
            let mut other = node.clone();
            other.prim = prim;
            assert!(!flat_gather_v1(&other, &sh.inputs, &sh.out));
        }
        assert!(!flat_gather_v1(&node, &[vec![3, 1], vec![6]], &[6]));
        assert!(!flat_gather_v1(&node, &sh.inputs, &[2, 3]));
    }

    #[test]
    fn nonflat_gather_axes_and_batches_still_match_the_independent_reference() {
        let program = dense_moe_v1(7).program;
        for (axis, batch, data_shape, index_shape, out_shape, indices) in [
            (0, 0, vec![3, 2], vec![2], vec![2, 2], vec![2, 0]),
            (1, 0, vec![2, 3], vec![2], vec![2, 2], vec![2, 0]),
            (1, 1, vec![2, 3], vec![2, 2], vec![2, 2], vec![2, 0, 1, 2]),
        ] {
            let data = Tensor::new(DType::I32, data_shape.clone(), (0..6).map(|i| i * 71 - 120).collect()).unwrap();
            let ix = Tensor::new(DType::Idx, index_shape.clone(), indices).unwrap();
            let node = Node {
                prim: Prim::Gather { axis, batch_dims: batch },
                inputs: vec![Ref::Param(0), Ref::Param(1)],
                out: misaka_palw_tir::TensorType::fixed(DType::I32, &out_shape.iter().map(|n| *n as u32).collect::<Vec<_>>()),
                commit: true,
            };
            let sh = ElementShapesV1 {
                out: out_shape.clone(),
                inputs: vec![data_shape, index_shape],
                in_dtypes: vec![DType::I32, DType::Idx],
                hist: None,
            };
            assert!(!flat_gather_v1(&node, &sh.inputs, &sh.out));
            let expected = eval_primitive(&node.prim, &[data.clone(), ix.clone()], DType::I32, &out_shape).unwrap();
            let ops = LookupOperands { tensors: vec![data, ix], missing: None, reads: RefCell::new(vec![]) };
            for e in 0..expected.len() as u64 {
                assert_eq!(element_value_v1(&program, &node, &sh, e, &ops), Ok(Some(expected.data[e as usize])));
            }
        }
    }

    /// The claim a producer committed, as values (honest or not), with its segment roots.
    struct Committed {
        values: Vec<Vec<Vec<Tensor>>>,
        c: SegmentedCommitmentsV1,
    }

    impl Committed {
        fn of(values: Vec<Vec<Vec<Tensor>>>) -> Self {
            let c = SegmentedCommitmentsV1::new(
                values.iter().map(|p| p.iter().map(|o| o.iter().map(tensor_commitment_v3).collect()).collect()).collect(),
            );
            Self { values, c }
        }
    }

    impl SegMaterialV1 for Committed {
        fn position(&self, p: u32) -> Option<Vec<Vec<Tensor>>> {
            self.values.get(p as usize).cloned()
        }
        fn position_siblings(&self, p: u32) -> Option<Vec<Digest>> {
            (p < self.c.positions()).then(|| self.c.position_path(p).1)
        }
    }

    struct Fx {
        program: TirProgramV1,
        params: MapParams,
        pc: ParamCommitmentsV1,
        tokens: Vec<u32>,
        generated: Vec<u32>,
        prompt: Vec<u32>,
    }

    /// A claim of `positions` positions whose last token and both delivered ids are the greedy decode (an honest claim).
    fn fx(positions: usize) -> Fx {
        let f = dense_moe_v1(7);
        let prompt: Vec<u32> = (0..positions as u32 - 1).map(|i| (i * 7 + 3) % f.program.token_bound).collect();
        let post = f.program.occurrences().len() - 1;
        let logits = f.program.logits as usize;
        let first = trace_v1(&f.program, &f.params, &prompt).unwrap();
        let g0 = DecodeRuleV1::Greedy.select(&first.values[prompt.len() - 1][post][logits]).unwrap();
        let mut tokens = prompt.clone();
        tokens.push(g0);
        let last = trace_v1(&f.program, &f.params, &tokens).unwrap();
        let g1 = DecodeRuleV1::Greedy.select(&last.values[tokens.len() - 1][post][logits]).unwrap();
        Fx { pc: ParamCommitmentsV1::of_v3(&f.params), program: f.program, params: f.params, generated: vec![g0, g1], tokens, prompt }
    }

    fn ctx<'a>(f: &'a Fx, roots: &'a [Digest]) -> SegClaimContextV1<'a> {
        SegClaimContextV1 {
            program: &f.program,
            params: &f.pc,
            segment_roots: roots,
            positions: f.tokens.len() as u32,
            prompt_len: f.prompt.len() as u32,
            prompt_root: prompt_root_of_ids_v1(&f.prompt),
            inline_prompt: None,
            generated: &f.generated,
            decode: DecodeRuleV1::Greedy,
            encoder: None,
        }
    }

    #[test]
    fn vocabulary_sized_flat_lookup_roots_match_the_frozen_coordinate_evaluator() {
        const N: usize = 151_936;
        struct Direct(Vec<Tensor>);
        impl OperandsV1 for Direct {
            fn get(&self, i: usize, e: u64) -> Result<i128, MissingV1> {
                self.0.get(i).and_then(|t| t.data.get(e as usize)).copied().ok_or(MissingV1(i, e))
            }
        }
        let p = dense_moe_v1(7).program;
        let node = Node {
            prim: Prim::Gather { axis: 0, batch_dims: 0 },
            inputs: vec![Ref::Param(0), Ref::Param(1)],
            out: misaka_palw_tir::TensorType::fixed(DType::I64, &[N as u32]),
            commit: true,
        };
        let sh =
            ElementShapesV1 { out: vec![N], inputs: vec![vec![63], vec![N]], in_dtypes: vec![DType::I64, DType::I32], hist: None };
        let ops = Direct(vec![
            Tensor::new(DType::I64, vec![63], (0..63).map(|i| i * 1051 - 1798).collect()).unwrap(),
            Tensor::new(DType::I32, vec![N], (0..N).map(|i| ((i * 37 + 19) % 63) as i128).collect()).unwrap(),
        ]);
        let evaluate = |fast: bool| {
            (0..N as u64)
                .map(|e| {
                    let e = std::hint::black_box(e);
                    if fast { element_value_v1(&p, &node, &sh, e, &ops) } else { coordinate_gather(&node, &sh, e, &ops) }
                        .unwrap()
                        .unwrap()
                })
                .collect::<Vec<_>>()
        };
        let old = evaluate(false);
        let new = evaluate(true);
        let reference = eval_primitive(&node.prim, &ops.0, DType::I64, &[N]).unwrap();
        assert_eq!(old, reference.data);
        assert_eq!(new, old);
        let root = tensor_commitment_v3(&reference);
        // Alternating warmed trials include both output commitment trees. Timings are a
        // local diagnostic, never a hardware-dependent consensus or test threshold.
        let mut times = [Vec::new(), Vec::new()];
        for trial in 0..6 {
            for fast in if trial % 2 == 0 { [false, true] } else { [true, false] } {
                let start = std::time::Instant::now();
                let data = evaluate(fast);
                let t = Tensor::new(DType::I64, vec![N], data).unwrap();
                assert_eq!(tensor_commitment_v3(&t), root);
                times[fast as usize].push(start.elapsed().as_nanos());
            }
        }
        for values in &mut times {
            values.sort_unstable()
        }
        eprintln!(
            "151936-output lookup + both commitment trees: frozen median {} ns, flat median {} ns; six alternating trials, all roots equal",
            times[0][3], times[1][3]
        );
    }

    /// Actual vocabulary-sized court, deliberately synthetic weights. This exercises the
    /// proof format/authentication at 151,936 outputs; it is not checkpoint fidelity evidence.
    #[test]
    fn vocabulary_sized_lookup_whole_court_dismisses_honest_convicts_lie_and_refuses_missing_or_substituted_inputs() {
        use misaka_palw_tir::builder::ProgramBuilder;
        use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
        use misaka_palw_tir::{Dim, TensorType};
        const VOCAB: u32 = 151_936;
        let mut pb = ProgramBuilder::new(VOCAB, HISTORY_BOUND_V1_SMALL);
        let embeddings = pb.param("embedding", DType::I16, &[VOCAB, 1], false);
        let table = pb.param("lookup", DType::I16, &[63], false);
        let indices = pb.param("indices", DType::I32, &[VOCAB], false);
        let carry = vec![TensorType::fixed(DType::I32, &[1])];
        let pre = {
            let mut b = pb.block("pre", vec![]);
            let x = b.gather(embeddings, Ref::Input(INPUT_TOKEN), 0, 0);
            let x = b.cast(x, DType::I32);
            b.finish(&[x])
        };
        let post = {
            let mut b = pb.block("post", carry);
            let x = b.broadcast(Ref::CarryIn(0), &[Dim::Fixed(VOCAB)]);
            let x = b.add(x, indices, DType::I32);
            let x = b.clamp(x, 0, 62, DType::I32);
            let y = b.gather(table, x, 0, 0);
            b.commit(y);
            let logits = b.cast(y, DType::I32);
            b.commit(logits);
            b.finish(&[])
        };
        let node = pb.blocks[post as usize]
            .nodes
            .iter()
            .position(|n| flat_gather_v1(n, &[vec![63], vec![VOCAB as usize]], &[VOCAB as usize]))
            .unwrap() as u16;
        let logits = (pb.blocks[post as usize].nodes.len() - 1) as u16;
        let program = pb.finish(pre, vec![], post, logits);
        misaka_palw_tir::validate::validate(&program).unwrap();
        let params = MapParams {
            tensors: BTreeMap::from([
                ((0, None), Tensor::new(DType::I16, vec![VOCAB as usize, 1], vec![0; VOCAB as usize]).unwrap()),
                ((1, None), Tensor::new(DType::I16, vec![63], (0..63).map(|i| i * 31 - 900).collect()).unwrap()),
                ((2, None), Tensor::new(DType::I32, vec![VOCAB as usize], (0..VOCAB).map(|i| (i % 63) as i128).collect()).unwrap()),
            ]),
        };
        let tokens = vec![3, 62];
        let f = Fx { pc: ParamCommitmentsV1::of_v3(&params), program, params, tokens, prompt: vec![3], generated: vec![62, 62] };
        let values = trace_v1(&f.program, &f.params, &f.tokens).unwrap().values;
        let honest = Committed::of(values.clone());
        let roots = honest.c.segment_roots();
        let hc = ctx(&f, &roots);
        let pr = PreparedV1::new(&f.program, None).unwrap();
        assert!(pr.is_withheld(1, node));
        let art = |j: u16, l: Option<u16>| f.params.tensors.get(&(j, l)).cloned();
        let loaded = load_opened_positions_for_element(&hc, &pr, &honest, &[0]).unwrap();
        let resolved = resolve_operands(&hc, &pr, &loaded, &art, &f.tokens, (0, 1, node)).unwrap();
        let whole = build_whole_value_fault(&hc, &pr, &loaded, &resolved, &f.tokens, (0, 1, node)).unwrap();
        let proof = SegFaultV1::WholeValue(whole.clone());
        let count = f.program.occurrences().iter().map(|(b, _)| f.program.blocks[*b as usize].nodes.len() as u64).sum();
        let (bytes, work) = crate::seg_scope::whole_value_court_cost_v1(&f.program, 1, node as usize, count);
        assert!(proof.to_bytes().len() as u64 <= bytes);
        assert!(work <= 536_870_912, "whole court exceeds default proof reservation: {work}");
        let started = std::time::Instant::now();
        assert_eq!(verify_seg_fault_v1(&hc, &proof), Err(DismissalV1::NoFault));
        eprintln!(
            "151936-output whole court: {} proof bytes, {} abstract work, {:?} honest verification",
            proof.to_bytes().len(),
            work,
            started.elapsed()
        );
        // Bounds are compared to the pre-specialization price, without changing disclosure.
        let conservative_work = work + VOCAB as u64 * 16 * 64;
        let model_mask = crate::scope::court_model_dependent_mask_v1(&f.program, crate::seg_scope::seg_model_params_v1(&f.program));
        assert!(model_mask[1][node as usize]);
        assert!(
            bytes <= crate::seg_scope::SEG_WHOLE_VALUE_COURT_MAX_BYTES_V1
                && conservative_work <= crate::seg_scope::SEG_WHOLE_VALUE_COURT_MAX_WORK_V1
        );
        for missing in [false, true] {
            let mut tampered = whole.clone();
            if missing {
                tampered.inputs[0].leaves.clear()
            } else {
                tampered.inputs[0].leaves[0].values[0] ^= 1;
            }
            assert!(matches!(verify_seg_fault_v1(&hc, &SegFaultV1::WholeValue(tampered)), Err(DismissalV1::NotAuthentic(_))));
        }
        let mut lying_values = values;
        lying_values[0][1][node as usize].data[VOCAB as usize - 1] += 1;
        let lying = Committed::of(lying_values);
        let lr = lying.c.segment_roots();
        let lc = ctx(&f, &lr);
        let SegFindingV1::Fault(fault) = check_positions_v1(&lc, &lying, &art, &f.tokens, &[0]) else {
            panic!("fresh verifier missed lookup lie")
        };
        assert!(matches!(fault.as_ref(), SegFaultV1::WholeValue(_)));
        assert_eq!(fault.at(), Some((0, 1, node)));
        assert!(fault.to_bytes().len() as u64 <= bytes);
        assert_eq!(verify_seg_fault_v1(&lc, &fault).unwrap().kind, SegConvictionKindV1::WholeValue);
        assert!(matches!(verify_seg_fault_v1(&hc, &fault), Err(DismissalV1::NotAuthentic(_))));
    }

    #[test]
    fn the_element_evaluator_agrees_with_the_reference_on_every_element_of_every_value() {
        let f = fx(5);
        let trace = trace_v1(&f.program, &f.params, &f.tokens).unwrap();
        let honest = Committed::of(trace.values.clone());
        let roots = honest.c.segment_roots();
        let c = ctx(&f, &roots);
        let pr = PreparedV1::new(&f.program, None).unwrap();
        let art = |j: u16, l: Option<u16>| f.params.tensors.get(&(j, l)).cloned();
        let loaded = load_opened_positions_for_element(&c, &pr, &honest, &(0..5).collect::<Vec<_>>()).unwrap();
        let mut checked = 0u64;
        let mut prims = BTreeSet::new();
        for p in 0..5u32 {
            for s in 0..pr.w.occurrences.len() as u16 {
                for n in 0..pr.nodes_in(s).unwrap() {
                    let (_, owned, scalars) = resolve_operands(&c, &pr, &loaded, &art, &f.tokens, (p, s, n)).unwrap();
                    let ops = FullOperands {
                        ops: owned.iter().map(Option::as_ref).collect(),
                        scalars: scalars.clone(),
                        reads: RefCell::new(vec![BTreeSet::new(); owned.len()]),
                    };
                    let out = &trace.values[p as usize][s as usize][n as usize];
                    let sh = pr.shapes(p, s, n);
                    prims.insert(pr.w.node(s, n).prim.tag());
                    for e in 0..out.len() as u64 {
                        let v = element_value_v1(&f.program, pr.w.node(s, n), &sh, e, &ops).unwrap();
                        assert_eq!(v, Some(out.data[e as usize]), "({p}, {s}, {n}) {:?} element {e}", pr.w.node(s, n).prim);
                        checked += 1;
                    }
                }
            }
        }
        assert!(checked > 1000, "{checked}");
        // The fixture exercises structure (Reshape, Transpose), Gather, MatMul, TopK and the history window, among others.
        for tag in [0u8, 1, 6, 11, 22, 24] {
            assert!(prims.contains(&tag), "prim tag {tag} not exercised: {prims:?}");
        }
    }

    #[test]
    fn a_lie_in_any_value_is_found_and_convicted_and_no_honest_element_is() {
        let f = fx(4);
        let trace = trace_v1(&f.program, &f.params, &f.tokens).unwrap();
        let art = |j: u16, l: Option<u16>| f.params.tensors.get(&(j, l)).cloned();
        let honest = Committed::of(trace.values.clone());
        let honest_roots = honest.c.segment_roots();
        let hc = ctx(&f, &honest_roots);
        assert_eq!(check_positions_v1(&hc, &honest, &art, &f.tokens, &[0, 1, 2, 3]), SegFindingV1::Clean);
        let pr = PreparedV1::new(&f.program, None).unwrap();
        let opened = load_opened_positions_for_element(&hc, &pr, &honest, &[0, 1, 2, 3]).unwrap();
        let mut convicted = 0;
        for p in [0u32, 3] {
            for s in 0..pr.w.occurrences.len() as u16 {
                for n in 0..pr.nodes_in(s).unwrap() {
                    // Soundness: the filing for an honest element of the honest claim is dismissed.
                    let honest_filing = prove_element_v1(&hc, &honest, &art, &f.tokens, (p, s, n), 0).unwrap();
                    assert_eq!(
                        verify_seg_fault_v1(&hc, &SegFaultV1::Element(honest_filing.clone())),
                        Err(DismissalV1::NoFault),
                        "({p}, {s}, {n})"
                    );
                    if pr.is_withheld(s, n) {
                        let resolved = resolve_operands(&hc, &pr, &opened, &art, &f.tokens, (p, s, n)).unwrap();
                        let whole = build_whole_value_fault(&hc, &pr, &opened, &resolved, &f.tokens, (p, s, n)).unwrap();
                        assert_eq!(
                            verify_seg_fault_v1(&hc, &SegFaultV1::WholeValue(whole)),
                            Err(DismissalV1::NoFault),
                            "an honest whole-value court ({p}, {s}, {n})"
                        );
                    }
                    // Completeness: the producer commits a wrong element 0 of this value, everything else honest.
                    let mut values = trace.values.clone();
                    let t = &mut values[p as usize][s as usize][n as usize];
                    let v = t.data[0];
                    t.data[0] = if t.dtype.contains(v + 1) { v + 1 } else { v - 1 };
                    let lying = Committed::of(values);
                    let roots = lying.c.segment_roots();
                    let lc = ctx(&f, &roots);
                    let SegFindingV1::Fault(fault) = check_positions_v1(&lc, &lying, &art, &f.tokens, &[p]) else {
                        panic!("({p}, {s}, {n}) {:?}: the lie is not found", pr.w.node(s, n).prim)
                    };
                    // A served value is judged by an element court, a withheld one (`crate::seg_scope`) by its whole-value court.
                    let kind_ok = if pr.is_withheld(s, n) {
                        matches!(fault.as_ref(), SegFaultV1::WholeValue(_))
                    } else {
                        matches!(fault.as_ref(), SegFaultV1::Element(_))
                    };
                    assert!(kind_ok, "({p}, {s}, {n}): {fault:?}");
                    assert_eq!(fault.at(), Some((p, s, n)), "{:?}", pr.w.node(s, n).prim);
                    let conviction = verify_seg_fault_v1(&lc, &fault).unwrap_or_else(|d| panic!("({p}, {s}, {n}): {d:?}"));
                    assert_eq!((conviction.position, conviction.occurrence, conviction.node), (p, s, n));
                    // The same filing against the honest claim authenticates nothing.
                    assert!(matches!(verify_seg_fault_v1(&hc, &fault), Err(DismissalV1::NotAuthentic(_))));
                    // Bounded: the filing is far below a carrier whatever the value.
                    assert!(fault.to_bytes().len() < 64 << 10, "{} B", fault.to_bytes().len());
                    convicted += 1;
                }
            }
        }
        assert!(convicted > 100, "{convicted}");
    }

    #[test]
    fn a_malformed_value_a_forged_decode_and_tampered_openings_are_judged_objectively() {
        let f = fx(4);
        let trace = trace_v1(&f.program, &f.params, &f.tokens).unwrap();
        let art = |j: u16, l: Option<u16>| f.params.tensors.get(&(j, l)).cloned();
        // A served (not withheld) value committed in another shape.
        let pr = PreparedV1::new(&f.program, None).unwrap();
        let (ms, mn) = (1..pr.w.occurrences.len() as u16)
            .flat_map(|s| (0..pr.nodes_in(s).unwrap()).map(move |n| (s, n)))
            .find(|(s, n)| !pr.is_withheld(*s, *n))
            .expect("a served value");
        let mut values = trace.values.clone();
        let t = &values[2][ms as usize][mn as usize];
        values[2][ms as usize][mn as usize] = Tensor::zeros(t.dtype, &[t.len() + 1]);
        let lying = Committed::of(values);
        let roots = lying.c.segment_roots();
        let lc = ctx(&f, &roots);
        let SegFindingV1::Fault(fault) = check_positions_v1(&lc, &lying, &art, &f.tokens, &[2]) else { panic!() };
        assert!(matches!(fault.as_ref(), SegFaultV1::Malformed(_)));
        assert_eq!(verify_seg_fault_v1(&lc, &fault).unwrap().kind, SegConvictionKindV1::Malformed);
        // A tiny filing cannot select an unpriced whole-value execution on a public/large node.
        let output = position_node_opening_v1(2, &lying.commitments(2).unwrap(), lying.c.position_path(2).1, ms, mn).unwrap();
        let unpriced = SegFaultV1::WholeValue(WholeValueFaultV1 {
            position: 2,
            occurrence: ms,
            node: mn,
            output,
            inputs: Vec::new(),
            token: None,
        });
        assert!(matches!(verify_seg_fault_v1(&lc, &unpriced), Err(DismissalV1::NotAuthentic(why))
            if why.contains("not priced")));
        // A delivered id that is not the greedy decode of its logits (position prompt_len - 1 selects generated[0]).
        let honest = Committed::of(trace.values.clone());
        let hroots = honest.c.segment_roots();
        let mut f2 = fx(4);
        let post = (f2.program.occurrences().len() - 1) as usize;
        let logits = &trace.values[2][post][f2.program.logits as usize];
        let best = DecodeRuleV1::Greedy.select(logits).unwrap();
        f2.generated = vec![(best + 1) % logits.len() as u32, 0];
        f2.tokens[3] = f2.generated[0];
        let c2 = ctx(&f2, &hroots);
        let SegFindingV1::Fault(fault) = check_positions_v1(&c2, &honest, &art, &f2.tokens, &[2]) else { panic!("decode") };
        assert!(matches!(fault.as_ref(), SegFaultV1::Decode(_)));
        assert!(matches!(verify_seg_fault_v1(&c2, &fault).unwrap().kind, SegConvictionKindV1::Decode { index: 0 }));
        f2.generated = vec![best, 0];
        f2.tokens[3] = best;
        let c3 = ctx(&f2, &hroots);
        let SegFaultV1::Decode(d) = fault.as_ref() else { unreachable!() };
        let weaker = SegFaultV1::Decode(SegDecodeFaultV1 { rival: (best + 1) % logits.len() as u32, ..d.clone() });
        assert_eq!(verify_seg_fault_v1(&c3, &weaker), Err(DismissalV1::NoFault), "the honest decode is never convicted");
        assert!(matches!(verify_seg_fault_v1(&c3, &fault), Err(DismissalV1::NotAuthentic(_))), "the rival is the delivered id");
        // Tampered element filings: a moved element, a leaf of another value, a forged path — none convicts.
        let hc = ctx(&f, &hroots);
        let good = prove_element_v1(&hc, &honest, &art, &f.tokens, (3, 1, 2), 0).unwrap();
        let mut moved = good.clone();
        moved.element = 1_000_000;
        assert!(matches!(verify_seg_fault_v1(&hc, &SegFaultV1::Element(moved)), Err(DismissalV1::NotAuthentic(_))));
        let mut forged = good.clone();
        if let Some(l) = forged.output.leaves.first_mut() {
            l.values[0] += 1;
        }
        assert!(matches!(verify_seg_fault_v1(&hc, &SegFaultV1::Element(forged)), Err(DismissalV1::NotAuthentic(_))));
        let mut stripped = good.clone();
        for i in &mut stripped.inputs {
            i.leaves.clear();
        }
        let r = verify_seg_fault_v1(&hc, &SegFaultV1::Element(stripped));
        assert!(matches!(r, Err(DismissalV1::NotAuthentic(_)) | Err(DismissalV1::NoFault)), "{r:?}");
        assert!(SegFaultV1::from_bytes(&[9, 9, 9]).is_err(), "junk bytes are no fault");
    }

    #[test]
    fn element_courts_are_priced_by_the_tile_not_the_tensor() {
        let f = dense_moe_v1(7);
        let nodes: u64 = f.program.occurrences().iter().map(|(b, _)| f.program.blocks[*b as usize].nodes.len() as u64).sum();
        for (b, block) in f.program.blocks.iter().enumerate() {
            for n in 0..block.nodes.len() {
                let (bytes, work) = element_court_cost_core(&f.program, b, n, nodes, 64, None);
                assert!(bytes > 0 && work > 0);
                assert!(bytes < 64 << 10, "block {b} node {n}: {bytes} B");
                // At the program's own window (2^18) the history products' lines are 2^18 long: priced so, never hidden.
                let (wide, _) = element_court_cost_core(&f.program, b, n, nodes, 1 << 18, None);
                assert!(wide >= bytes);
                let (priced, priced_work) = element_court_cost_v1(&f.program, b, n, nodes, 64);
                assert!(priced >= bytes && priced_work >= work);
                assert!(priced <= crate::seg_scope::SEG_WHOLE_VALUE_COURT_MAX_BYTES_V1);
            }
        }
    }

    /// **The wire form of a leaf is at the dtype's width** (the encoder-court finding, `k2-real-scale.md` §3): every dtype round-trips,
    /// the encoded length is `byte_len − 2`, a value outside its dtype or an unknown dtype has no wire form, and a count the bytes do not
    /// carry is refused without allocating it.
    #[test]
    fn leaf_openings_file_their_values_at_the_dtype_width() {
        for dtype in DType::ALL {
            let data: Vec<i128> =
                (0..300i128).map(|i| if i % 2 == 0 { dtype.max_value() - i % 64 } else { dtype.min_value() + i % 64 }).collect();
            let t = Tensor::new(dtype, vec![3, 100], data).unwrap();
            let trees = TreesV3::of(&t);
            for axis in [AXIS_ROW, AXIS_COL] {
                let leaf = LeafOpeningV3::of_trees(&t, &trees, axis, 1, 0).unwrap();
                let bytes = borsh::to_vec(&leaf).unwrap();
                let rank = leaf.shape.len() as u64;
                let want = 94 + 8 * rank + leaf.values.len() as u64 * dtype.width() as u64 + 64 * leaf.siblings.len() as u64;
                assert_eq!(bytes.len() as u64, want, "{dtype:?} axis {axis}");
                assert_eq!(leaf.byte_len(), want + 2, "the price is the wire form plus two");
                let back: LeafOpeningV3 = borsh::from_slice(&bytes).unwrap();
                assert_eq!(back, leaf, "{dtype:?} axis {axis}: round trip");
                assert!(back.authenticates(&tensor_commitment_v3(&t)));
            }
            if dtype != DType::I128 {
                let mut out = LeafOpeningV3::of_trees(&t, &trees, AXIS_ROW, 0, 0).unwrap();
                out.values[0] = dtype.max_value() + 1;
                assert!(borsh::to_vec(&out).is_err(), "{dtype:?}: a value outside its dtype has no wire form");
            }
        }
        let t = Tensor::new(DType::I8, vec![4], vec![1, -2, 3, -4]).unwrap();
        let mut leaf = LeafOpeningV3::of_trees(&t, &TreesV3::of(&t), AXIS_ROW, 0, 0).unwrap();
        let bytes = borsh::to_vec(&leaf).unwrap();
        assert!(borsh::from_slice::<LeafOpeningV3>(&bytes[..bytes.len() - 1]).is_err(), "truncated");
        // A count of 2^32 − 1 values over a handful of bytes fails at the first missing element.
        let mut lie = bytes.clone();
        let at = 1 + 4 + 8 + 1 + 8 + 8;
        lie[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(borsh::from_slice::<LeafOpeningV3>(&lie).is_err());
        leaf.dtype = 9;
        assert!(borsh::to_vec(&leaf).is_err(), "an unknown dtype has no wire form");
        let mut unknown = bytes;
        unknown[0] = 9;
        assert!(borsh::from_slice::<LeafOpeningV3>(&unknown).is_err());
    }

    /// **Every filing is within its priced bound**: on the dense-MoE fixture, the filing for the first, a middle and the last element
    /// of every committed value at the first and the last position (tiled prompt, so the token's tile is filed) is no larger than
    /// [`element_court_cost_v1`] of its relation at the claim's length, and a decode filing is no larger than
    /// [`decode_court_cost_v1`] — so the gate's carrier check, which holds the worst price, holds every filing.
    #[test]
    fn every_filing_is_within_its_priced_bound() {
        let f = fx(5);
        let trace = trace_v1(&f.program, &f.params, &f.tokens).unwrap();
        let art = |j: u16, l: Option<u16>| f.params.tensors.get(&(j, l)).cloned();
        let honest = Committed::of(trace.values.clone());
        let roots = honest.c.segment_roots();
        let c = ctx(&f, &roots);
        let pr = PreparedV1::new(&f.program, None).unwrap();
        let positions = f.tokens.len() as u32;
        let (mut filed, mut worst) = (0u32, 0f64);
        for p in [0u32, positions - 1] {
            for s in 0..pr.w.occurrences.len() as u16 {
                let b = pr.w.occurrences[s as usize].0 as usize;
                for n in 0..pr.nodes_in(s).unwrap() {
                    let priced = element_court_cost_v1(&f.program, b, n as usize, pr.node_count, positions).0;
                    let len = trace.values[p as usize][s as usize][n as usize].len() as u64;
                    for e in [0, len / 2, len.saturating_sub(1)] {
                        let filing = prove_element_v1(&c, &honest, &art, &f.tokens, (p, s, n), e).unwrap();
                        let bytes = SegFaultV1::Element(filing).to_bytes().len() as u64;
                        assert!(
                            bytes <= priced,
                            "({p}, {s}, {n}) {:?} element {e}: filed {bytes} > priced {priced}",
                            pr.w.node(s, n).prim
                        );
                        worst = worst.max(bytes as f64 / priced as f64);
                        filed += 1;
                    }
                }
            }
        }
        assert!(filed > 100, "{filed} filings");
        println!("{filed} filings within their prices, the largest at {worst:.3} of its price");
        // A decode lie: the last delivered id is not the greedy one.
        let mut lied = fx(5);
        let bound = f.program.token_bound;
        lied.generated[1] = (lied.generated[1] + 1) % bound;
        let lc = ctx(&lied, &roots);
        let SegFindingV1::Fault(fault) = check_positions_v1(&lc, &honest, &art, &lied.tokens, &[positions - 1]) else {
            panic!("the decode lie is not found")
        };
        assert!(matches!(fault.as_ref(), SegFaultV1::Decode(_)));
        let (priced, _) = decode_court_cost_v1(&f.program, pr.node_count, positions);
        assert!(fault.to_bytes().len() as u64 <= priced, "decode filed {} > priced {priced}", fault.to_bytes().len());
    }

    /// The streamed trace is the trace: every position's values, in order, equal `trace_v1`'s (a history-bearing program).
    #[test]
    fn the_streamed_trace_is_the_trace() {
        let f = fx(9);
        let full = trace_v1(&f.program, &f.params, &f.tokens).unwrap();
        let mut seen = 0u32;
        crate::trace::trace_streaming_v1(&f.program, &f.params, &f.tokens, &mut |p, values| {
            assert_eq!(values, full.values[p as usize].as_slice(), "position {p}");
            assert_eq!(p, seen);
            seen += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(seen, f.tokens.len() as u32);
    }
}
