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

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum SegFaultV1 {
    Element(ElementFaultV1) = 0,
    Malformed(MalformedFaultV1) = 1,
    Decode(SegDecodeFaultV1) = 2,
}

impl SegFaultV1 {
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

/// The program's per-position structure the courts read: the wiring and the flat index of each occurrence's first node.
struct PreparedV1<'a> {
    w: WiringV1<'a>,
    offsets: Vec<u64>,
    node_count: u64,
}

impl<'a> PreparedV1<'a> {
    fn new(program: &'a TirProgramV1) -> Result<Self, String> {
        let w = WiringV1::new(program).map_err(|e| format!("the program does not validate: {e}"))?;
        let mut offsets = Vec::with_capacity(w.occurrences.len());
        let mut at = 0u64;
        for (b, _) in &w.occurrences {
            offsets.push(at);
            at += program.blocks[*b as usize].nodes.len() as u64;
        }
        Ok(Self { w, offsets, node_count: at })
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
    let pr = PreparedV1::new(c.program).map_err(na)?;
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
                    OperandSourceV1::Source(SourceV1::Input { .. }) => {
                        return Err(na("a pipeline stage input on a single-program claim".into()));
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
    fn position(&self, p: u32) -> Option<Vec<Vec<Tensor>>>;
    fn position_siblings(&self, p: u32) -> Option<Vec<Digest>>;
    /// Position `p`'s root and its path to its segment root, **without its values** where the source keeps them apart (part 0 of a
    /// demand, a stream's position-root list): what the re-execution check descends a segment with (`crate::seg_detect`). By default
    /// it is derived from the values.
    fn position_path(&self, p: u32) -> Option<(Digest, Vec<Digest>)> {
        let values = self.position(p)?;
        let commitments: Vec<Vec<Digest>> = values.iter().map(|occ| occ.iter().map(tensor_commitment_v3).collect()).collect();
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
    let l = trees.layout;
    let rows: BTreeSet<(u64, u64)> = reads.iter().filter_map(|e| l.row_leaf_of(*e).map(|(a, b, _)| (a, b))).collect();
    let cols: BTreeSet<(u64, u64)> = reads.iter().filter_map(|e| l.col_leaf_of(*e).map(|(a, b, _)| (a, b))).collect();
    let (axis, set) = if cols.len() < rows.len() { (AXIS_COL, cols) } else { (AXIS_ROW, rows) };
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
    let pr = match PreparedV1::new(c.program) {
        Ok(pr) => pr,
        Err(why) => return SegFindingV1::Inconsistent(why),
    };
    let mut params: BTreeMap<(u16, Option<u16>), Tensor> = BTreeMap::new();
    let mut ps: Vec<u32> = positions.to_vec();
    ps.sort_unstable();
    ps.dedup();
    if let Some(p) = ps.iter().find(|p| **p >= c.positions) {
        return SegFindingV1::Inconsistent(format!("position {p} is outside the claim"));
    }
    let loaded = match load_positions_or_demand(c, &pr, material, &ps) {
        Ok(l) => l,
        Err(missing) => return SegFindingV1::Demand(missing),
    };
    for &p in &ps {
        match check_one(c, &pr, &loaded, &mut params, artifact, tokens, p) {
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
    params: &mut BTreeMap<(u16, Option<u16>), Tensor>,
    artifact: &dyn Fn(u16, Option<u16>) -> Option<Tensor>,
    tokens: &[u32],
    (p, s, n): (u32, u16, u16),
) -> Result<OperandsResolvedV1, String> {
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
                if !params.contains_key(&(*index, *layer)) {
                    let t = artifact(*index, *layer).ok_or_else(|| format!("the public artifact lacks param {index}"))?;
                    if c.params.by_instance.get(&(*index, *layer)) != Some(&tensor_commitment_v3(&t)) {
                        return Err(format!("the public artifact's param {index} is not the class's"));
                    }
                    params.insert((*index, *layer), t);
                }
                (Some(params[&(*index, *layer)].clone()), None)
            }
            OperandSourceV1::Source(SourceV1::Const(j)) => (const_tensor(c.program, *j).ok(), None),
            OperandSourceV1::Source(SourceV1::Zeros { dtype, shape }) => (Some(Tensor::zeros(*dtype, shape)), None),
            OperandSourceV1::Source(SourceV1::Public(v)) => (None, Some(*v as i128)),
            OperandSourceV1::Source(SourceV1::Input { .. }) => return Err("a stage input".into()),
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
    let output = OperandOpeningV1 {
        node: opening(p, s, n),
        leaves: vec![LeafOpeningV3::holding(out, &out_trees, AXIS_ROW, e).ok_or("no output leaf")?],
    };
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
        inputs.push(OperandOpeningV1 { node: node_opening, leaves });
    }
    Ok(ElementFaultV1 { position: p, occurrence: s, node: n, element: e, output, inputs, token })
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
    let pr = PreparedV1::new(c.program)?;
    if !pr.valid(s, n) || p >= c.positions {
        return Err("no such value".into());
    }
    let loaded = load_positions(c, &pr, material, &[p])?;
    let mut params = BTreeMap::new();
    let resolved = resolve_operands(c, &pr, &loaded, &mut params, artifact, tokens, (p, s, n))?;
    build_element_fault(c, &pr, &loaded, &resolved, tokens, (p, s, n), e)
}

/// Load and authenticate positions `ps` and every position they read (`p − 1`); `Err` lists the positions to demand.
fn load_positions(
    c: &SegClaimContextV1<'_>,
    pr: &PreparedV1<'_>,
    material: &dyn SegMaterialV1,
    ps: &[u32],
) -> Result<BTreeMap<u32, Loaded>, String> {
    match load_positions_or_demand(c, pr, material, ps) {
        Ok(l) => Ok(l),
        Err(missing) => Err(format!("positions {missing:?} are not served")),
    }
}

fn load_positions_or_demand(
    c: &SegClaimContextV1<'_>,
    pr: &PreparedV1<'_>,
    material: &dyn SegMaterialV1,
    ps: &[u32],
) -> Result<BTreeMap<u32, Loaded>, Vec<u32>> {
    let mut need: BTreeSet<u32> = BTreeSet::new();
    for &p in ps {
        need.insert(p);
        if p > 0 {
            need.insert(p - 1);
        }
    }
    let mut loaded = BTreeMap::new();
    let mut missing = Vec::new();
    for q in need {
        let (Some(values), Some(siblings)) = (material.position(q), material.position_siblings(q)) else {
            missing.push(q);
            continue;
        };
        let shape_ok = values.len() == pr.w.occurrences.len()
            && values.iter().enumerate().all(|(s, occ)| pr.nodes_in(s as u16) == Some(occ.len() as u16));
        if !shape_ok {
            missing.push(q);
            continue;
        }
        let commitments: Vec<Vec<Digest>> = values.iter().map(|occ| occ.iter().map(tensor_commitment_v3).collect()).collect();
        if !position_in_segment(q, &position_root_of_v1(q, &commitments), &siblings, c.segment_roots, c.positions) {
            missing.push(q);
            continue;
        }
        loaded.insert(q, Loaded { values, commitments, siblings });
    }
    if missing.is_empty() { Ok(loaded) } else { Err(missing) }
}

#[allow(clippy::too_many_arguments)]
fn check_one(
    c: &SegClaimContextV1<'_>,
    pr: &PreparedV1<'_>,
    loaded: &BTreeMap<u32, Loaded>,
    params: &mut BTreeMap<(u16, Option<u16>), Tensor>,
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
            let resolved = resolve_operands(c, pr, loaded, params, artifact, tokens, (p, s, n))?;
            let (_, owned, scalars) = &resolved;
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
                        .iter()
                        .zip(scalars)
                        .map(|(t, v)| {
                            t.clone().unwrap_or_else(|| Tensor { dtype: DType::Idx, shape: Vec::new(), data: vec![v.unwrap_or(0)] })
                        })
                        .collect();
                    eval_node(c.program, node, &inputs, &[], pr.w.h(s, p)).ok().map(|t| t.data)
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

/// The leaves (count, bytes) covering `reads` of a tensor of `shape` and `dtype`, cheapest tree.
fn cover_cost(shape: &[usize], dtype: DType, reads: &BTreeSet<u64>) -> (u64, u64) {
    let l = LayoutV3::of(shape);
    let w = dtype.width() as u64;
    let mut best = (u64::MAX, u64::MAX);
    for axis in [AXIS_ROW, AXIS_COL] {
        let set: BTreeSet<(u64, u64)> = reads
            .iter()
            .filter_map(|e| if axis == AXIS_ROW { l.row_leaf_of(*e) } else { l.col_leaf_of(*e) })
            .map(|(a, b, _)| (a, b))
            .collect();
        let path = (crate::merkle::depth(l.leaves(axis)) + 1) * 64 + 8 * shape.len() as u64 + 32;
        let bytes: u64 =
            set.iter().map(|(line, tile)| l.leaf_elements(axis, *line, *tile).map_or(0, |v| v.len() as u64) * w + path).sum();
        if (set.len() as u64, bytes) < best {
            best = (set.len() as u64, bytes);
        }
    }
    best
}

/// **The worst element court of `(block, node)`** at the worst `H` a claim of `max_positions` can have (the block's window, at most
/// the positions): `(bytes, work)`. Priced by running the court's own evaluator over zero operands at the first and the last element
/// and covering what it reads with the cheapest tree.
pub fn element_court_cost_v1(program: &TirProgramV1, block: usize, node: usize, node_count: u64, max_positions: u32) -> (u64, u64) {
    let n = &program.blocks[block].nodes[node];
    let h = crate::plan::worst_h(program, block).min(max_positions.max(1) as usize);
    let mut shapes = Vec::new();
    let mut dtypes = Vec::new();
    let mut kinds = Vec::new();
    for r in &n.inputs {
        let t = crate::plan::ref_type(program, block, r);
        shapes.push(t.resolve(h));
        dtypes.push(t.dtype);
        kinds.push(match r {
            Ref::Node(_) | Ref::CarryIn(_) | Ref::State(_) => 1u8,
            Ref::Param(_) => 2,
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
        let reads = ops.reads.into_inner();
        let mut bytes = FAULT_FIXED_BYTES_V1 + node_bytes + out_leaf;
        let mut work = out_leaf / 16 + HASH_WORK_V1 * (crate::merkle::depth(node_count) + 12);
        for (i, r) in reads.iter().enumerate() {
            match kinds[i] {
                3 => bytes += PROMPT_TILE_IDS_V1 as u64 * 4 + 64 * 10,
                1 | 2 if !r.is_empty() => {
                    let (leaves, b) = cover_cost(&shapes[i], dtypes[i], r);
                    bytes += b + if kinds[i] == 1 { node_bytes } else { 8 };
                    work += r.len() as u64 + leaves * HASH_WORK_V1 * 24;
                }
                _ => {}
            }
        }
        worst = (bytes.max(worst.0), work.max(worst.1));
    }
    worst
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seg::{SegmentedCommitmentsV1, prompt_root_of_ids_v1};
    use crate::trace::trace_v1;
    use misaka_palw_tir::MapParams;
    use misaka_palw_tir_sketch::fixture::dense_moe_v1;

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
        }
    }

    #[test]
    fn the_element_evaluator_agrees_with_the_reference_on_every_element_of_every_value() {
        let f = fx(5);
        let trace = trace_v1(&f.program, &f.params, &f.tokens).unwrap();
        let honest = Committed::of(trace.values.clone());
        let roots = honest.c.segment_roots();
        let c = ctx(&f, &roots);
        let pr = PreparedV1::new(&f.program).unwrap();
        let loaded = load_positions(&c, &pr, &honest, &(0..5).collect::<Vec<_>>()).unwrap();
        let mut params = BTreeMap::new();
        let art = |j: u16, l: Option<u16>| f.params.tensors.get(&(j, l)).cloned();
        let mut checked = 0u64;
        let mut prims = BTreeSet::new();
        for p in 0..5u32 {
            for s in 0..pr.w.occurrences.len() as u16 {
                for n in 0..pr.nodes_in(s).unwrap() {
                    let (_, owned, scalars) = resolve_operands(&c, &pr, &loaded, &mut params, &art, &f.tokens, (p, s, n)).unwrap();
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
        let pr = PreparedV1::new(&f.program).unwrap();
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
                    let SegFaultV1::Element(e) = fault.as_ref() else { panic!("an element fault") };
                    assert_eq!((e.position, e.occurrence, e.node), (p, s, n), "{:?}", pr.w.node(s, n).prim);
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
        // A value committed in another shape.
        let mut values = trace.values.clone();
        values[2][1][0] = Tensor::zeros(values[2][1][0].dtype, &[values[2][1][0].len() + 1]);
        let lying = Committed::of(values);
        let roots = lying.c.segment_roots();
        let lc = ctx(&f, &roots);
        let SegFindingV1::Fault(fault) = check_positions_v1(&lc, &lying, &art, &f.tokens, &[2]) else { panic!() };
        assert!(matches!(fault.as_ref(), SegFaultV1::Malformed(_)));
        assert_eq!(verify_seg_fault_v1(&lc, &fault).unwrap().kind, SegConvictionKindV1::Malformed);
        // A delivered id that is not the greedy decode of its logits (position prompt_len - 1 selects generated[0]).
        let honest = Committed::of(trace.values.clone());
        let hroots = honest.c.segment_roots();
        let mut f2 = fx(4);
        let post = (f2.program.occurrences().len() - 1) as usize;
        let logits = &trace.values[2][post][f2.program.logits as usize];
        let best = DecodeRuleV1::Greedy.select(logits).unwrap();
        f2.generated = vec![(best + 1) % logits.len() as u32, 0];
        let c2 = ctx(&f2, &hroots);
        let SegFindingV1::Fault(fault) = check_positions_v1(&c2, &honest, &art, &f2.tokens, &[2]) else { panic!("decode") };
        assert!(matches!(fault.as_ref(), SegFaultV1::Decode(_)));
        assert!(matches!(verify_seg_fault_v1(&c2, &fault).unwrap().kind, SegConvictionKindV1::Decode { index: 0 }));
        f2.generated = vec![best, 0];
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
                let (bytes, work) = element_court_cost_v1(&f.program, b, n, nodes, 64);
                assert!(bytes > 0 && work > 0);
                assert!(bytes < 64 << 10, "block {b} node {n}: {bytes} B");
                // At the program's own window (2^18) the history products' lines are 2^18 long: priced so, never hidden.
                let (wide, _) = element_court_cost_v1(&f.program, b, n, nodes, 1 << 18);
                assert!(wide >= bytes);
            }
        }
    }
}
