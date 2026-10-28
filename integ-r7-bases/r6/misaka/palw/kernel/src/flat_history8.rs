//! Explicit bounded history-axis selection relation, version 8. This is a
//! typed extension component, not a whole-model program or native activation.
//! It requires a separately checked source HistAppend relation; an authenticated
//! history root does not prove that history's state/input continuity.
use crate::{
    flat_court4 as common,
    flat_court5::{FlatOperandV5, flat_profile_commitment_v5},
    flat_detect5::{FlatReadV5, value_proof},
    flat_detect7::{FlatPublicReadsV7, FlatValueReadV7, FlatValueV7, TOPK_VALUE_MAX_BYTES_V7},
    flat_topk7::{TopKFaultKindV7, TopKScanBudgetV7, at, detect_topk_line_v7, operand, output_point},
    hash::{Digest, object_id},
    merkle::AXIS_ROW,
    merkle3::LayoutV3,
    merkle5::flat_material_bounds_v5,
    seg::{NodeOpeningV1, segment_count_v1},
    verify::DismissalV1,
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::{DType, Tensor};
use std::{
    cmp::Reverse,
    collections::{BTreeSet, BinaryHeap},
    io::{self, Cursor},
};

pub const HISTORY_SELECTION_MAX_POSITIONS_V8: u32 = 10_485_760;
pub const HISTORY_SELECTION_MAX_FAULT_BYTES_V8: u64 = 128 << 10;
pub const HISTORY_SELECTION_MAX_AUX_BYTES_V8: u128 = 32 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct HistoryNodeV8 {
    pub occurrence: u16,
    pub node: u16,
    pub slot: u64,
}
/// Metadata binds a relation to an outer typed graph and exact source/output
/// coordinates. The outer graph must include its other relations and courts.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct HistorySelectionPlanV8 {
    pub version: u16,
    pub graph_root: Digest,
    pub source: HistoryNodeV8,
    pub output: HistoryNodeV8,
    pub nodes_per_position: u64,
    pub dtype: DType,
    pub row_shape: Vec<u32>,
    pub window: u32,
    pub selection_cap: u32,
    pub max_positions: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct HistorySelectionJobV8 {
    pub version: u16,
    pub plan_root: Digest,
    pub input_root: Digest,
    pub positions: u32,
    pub nonce: Digest,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct HistorySelectionScopeV8 {
    pub version: u16,
    pub plan_root: Digest,
    pub max_history: u32,
    pub max_selected: u32,
    pub rows: u64,
    pub source_storage_bytes: u128,
    pub output_storage_bytes: u128,
    pub source_reference_bytes: u128,
    pub output_reference_bytes: u128,
    pub source_hash_bytes_per_attempt: u128,
    pub source_tree_bytes: u128,
    pub selection_work_per_claim: u128,
    pub selection_auxiliary_bytes: u128,
    pub selected_reads_per_claim: u128,
    pub selected_reply_bytes_per_claim: u128,
    pub filing_bytes: u64,
    pub court_work: u64,
    pub court_scratch_bytes: u64,
}
impl HistorySelectionScopeV8 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/history-selection-scope/v8", self)
    }
}
fn na(s: &str) -> DismissalV1 {
    DismissalV1::NotAuthentic(s.into())
}
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid bounded history-selection-v8 bytes")
}
fn line_work(n: u32, k: u32) -> u128 {
    (n as u128 + k as u128) * (4 * (32 - k.leading_zeros()) as u128 + 16)
}
impl HistorySelectionPlanV8 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/history-selection-plan/v8", self)
    }
    pub fn validate(&self) -> Result<u64, String> {
        if self.version != 8
            || self.max_positions == 0
            || self.max_positions > HISTORY_SELECTION_MAX_POSITIONS_V8
            || self.window == 0
            || self.window > self.max_positions
            || self.selection_cap == 0
            || self.selection_cap > self.window
            || self.row_shape.len() > 3
            || self.nodes_per_position == 0
            || self.nodes_per_position > 525_312
            || self.source.slot >= self.output.slot
            || self.output.slot >= self.nodes_per_position
            || (self.source.occurrence, self.source.node) >= (self.output.occurrence, self.output.node)
        {
            return Err("invalid bounded history-selection graph metadata".into());
        }
        let rows = self
            .row_shape
            .iter()
            .try_fold(1u64, |n, d| if *d == 0 || *d > misaka_palw_tir::types::MAX_DIM { None } else { n.checked_mul(*d as u64) })
            .ok_or("invalid history-selection row shape")?;
        if rows.checked_mul(self.window as u64).is_none_or(|n| n > misaka_palw_tir::types::MAX_ELEMENTS)
            || 64 * self.selection_cap as u128 > HISTORY_SELECTION_MAX_AUX_BYTES_V8
        {
            return Err("history-selection source or temporary selection exceeds cap".into());
        }
        Ok(rows)
    }
    pub fn shapes(&self, p: u32) -> Result<(Vec<usize>, Vec<usize>), String> {
        self.validate()?;
        if p >= self.max_positions {
            return Err("history-selection position exceeds admitted extent".into());
        }
        let h = p.saturating_add(1).min(self.window);
        let k = h.min(self.selection_cap);
        let mut source = vec![h as usize];
        source.extend(self.row_shape.iter().map(|d| *d as usize));
        let mut output = source.clone();
        output[0] = k as usize;
        Ok((source, output))
    }
    /// Selected ids address the current window, whose oldest absolute position
    /// is `p + 1 - H`. A caller cannot interpret them as global token ids.
    pub fn absolute_position(&self, p: u32, local: u32) -> Result<u32, String> {
        let (source, _) = self.shapes(p)?;
        let h = source[0] as u32;
        if local >= h {
            return Err("history-selection local index is outside the admitted window".into());
        }
        p.checked_add(1)
            .and_then(|end| end.checked_sub(h))
            .and_then(|first| first.checked_add(local))
            .ok_or("history-selection absolute coordinate overflow".into())
    }
    pub fn scope(&self) -> Result<HistorySelectionScopeV8, String> {
        let rows = self.validate()?;
        let (source, output) = self.shapes(self.max_positions - 1)?;
        let value_bound = |d: DType, shape: &[usize]| -> Result<u64, String> {
            if shape.iter().product::<usize>() >= 64 {
                let shape: Vec<_> = shape.iter().map(|n| *n as u64).collect();
                return flat_material_bounds_v5(d, &shape).map(|b| b.leaf_value_bytes).ok_or("bad history-selection material".into());
            }
            Ok(1 + 4 + shape.len() as u64 * 8 + 1 + 8 + 8 + 4 + 63 * d.width() as u64 + 6 * 64 + 64)
        };
        // Earlier H can use the legacy layout. Enumerate only its bounded
        // first 63 extents and maximum H, never a context-sized position list.
        let (mut source_leaf, mut output_leaf) = (0, 0);
        for h in 1..=63.min(self.window) {
            let (s, o) = self.shapes(h - 1)?;
            source_leaf = source_leaf.max(value_bound(self.dtype, &s)?);
            output_leaf = output_leaf.max(value_bound(DType::Idx, &o)?);
        }
        source_leaf = source_leaf.max(value_bound(self.dtype, &source)?);
        output_leaf = output_leaf.max(value_bound(DType::Idx, &output)?);
        let opening = crate::element::node_opening_bytes_v1(self.nodes_per_position);
        let filing_bytes = 256 + 2 * opening + 2 * source_leaf + 3 * output_leaf;
        let meta = self.row_shape.len() as u64 + self.nodes_per_position;
        let court_work = filing_bytes * 128 + meta * 1024 + 16384;
        let court_scratch_bytes = filing_bytes * 16 + meta * 64 + 4096;
        if filing_bytes > HISTORY_SELECTION_MAX_FAULT_BYTES_V8 || court_work > (1 << 30) || court_scratch_bytes > (32 << 20) {
            return Err("history-selection terminal envelope exceeds cap".into());
        }
        let h = source[0] as u32;
        let k = output[0] as u32;
        let material = flat_material_bounds_v5(self.dtype, &source.iter().map(|d| *d as u64).collect::<Vec<_>>());
        let source_hash_bytes_per_attempt = material
            .map(|m| 2 * m.hash_input_bytes + 16384)
            .unwrap_or(rows as u128 * h as u128 * self.dtype.width() as u128 * 4 + 16384);
        let source_tree_bytes = material.map(|m| m.retained_tree_bytes).unwrap_or(8192);
        let selected_reads_per_claim = rows as u128 * (k as u128 + 3) * self.max_positions as u128;
        Ok(HistorySelectionScopeV8 {
            version: 8,
            plan_root: self.root(),
            max_history: h,
            max_selected: k,
            rows,
            source_storage_bytes: rows as u128 * h as u128 * self.dtype.width() as u128,
            output_storage_bytes: rows as u128 * k as u128 * 4,
            source_reference_bytes: rows as u128 * h as u128 * 16,
            output_reference_bytes: rows as u128 * k as u128 * 16,
            source_hash_bytes_per_attempt,
            source_tree_bytes,
            selection_work_per_claim: rows as u128 * line_work(h, k) * self.max_positions as u128,
            selection_auxiliary_bytes: 64 * k as u128,
            selected_reads_per_claim,
            selected_reply_bytes_per_claim: selected_reads_per_claim * TOPK_VALUE_MAX_BYTES_V7 as u128,
            filing_bytes,
            court_work,
            court_scratch_bytes,
        })
    }
    pub fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() > 256 {
            return Err(invalid());
        }
        let mut r = Cursor::new(bytes);
        let version = u16::deserialize_reader(&mut r)?;
        if version != 8 {
            return Err(invalid());
        }
        let graph_root = Digest::deserialize_reader(&mut r)?;
        let source = HistoryNodeV8::deserialize_reader(&mut r)?;
        let output = HistoryNodeV8::deserialize_reader(&mut r)?;
        let nodes_per_position = u64::deserialize_reader(&mut r)?;
        let dtype = DType::deserialize_reader(&mut r)?;
        let n = u32::deserialize_reader(&mut r)? as usize;
        if n > 3 || n > bytes.len().saturating_sub(r.position() as usize) / 4 {
            return Err(invalid());
        }
        let row_shape = (0..n).map(|_| u32::deserialize_reader(&mut r)).collect::<io::Result<Vec<_>>>()?;
        let out = Self {
            version,
            graph_root,
            source,
            output,
            nodes_per_position,
            dtype,
            row_shape,
            window: u32::deserialize_reader(&mut r)?,
            selection_cap: u32::deserialize_reader(&mut r)?,
            max_positions: u32::deserialize_reader(&mut r)?,
        };
        if r.position() as usize != bytes.len() || out.validate().is_err() {
            return Err(invalid());
        }
        Ok(out)
    }
}
impl HistorySelectionJobV8 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/history-selection-job/v8", self)
    }
    pub fn check(&self, p: &HistorySelectionPlanV8) -> Result<(), String> {
        p.validate()?;
        if self.version != 8 || self.plan_root != p.root() || self.positions == 0 || self.positions > p.max_positions {
            return Err("history-selection job length or program differs from admission".into());
        }
        Ok(())
    }
}
pub struct HistorySelectionContextV8<'a> {
    pub plan: &'a HistorySelectionPlanV8,
    pub job: &'a HistorySelectionJobV8,
    pub scope: &'a HistorySelectionScopeV8,
    pub segment_roots: &'a [Digest],
}
impl HistorySelectionContextV8<'_> {
    pub fn evidence_root(&self) -> Digest {
        object_id(
            b"misaka-palw/kernel/history-selection-evidence/v8",
            &(self.plan.root(), self.job.root(), self.scope.root(), crate::seg::claim_root_v1(self.job.positions, self.segment_roots)),
        )
    }
    fn check(&self) -> Result<(), DismissalV1> {
        self.job.check(self.plan).map_err(|s| na(&s))?;
        if self.plan.scope().map_err(|s| na(&s))? != *self.scope
            || self.segment_roots.len() != segment_count_v1(self.job.positions) as usize
        {
            return Err(na("history-selection scope or evidence extent differs"));
        }
        Ok(())
    }
    fn node(&self, o: &NodeOpeningV1, at: HistoryNodeV8, p: u32) -> bool {
        o.position == p
            && p < self.job.positions
            && o.occurrence == at.occurrence
            && o.node == at.node
            && o.authenticates(self.segment_roots, self.job.positions, at.slot, self.plan.nodes_per_position)
    }
}
/// Efficient exact selection over a caller-owned, independently acquired
/// history tensor. The source HistAppend correctness is checked separately.
pub struct HistorySelectionExecutionLimitsV8 {
    pub source_reference_bytes: u128,
    pub output_reference_bytes: u128,
    pub selection_work: u128,
    pub auxiliary_bytes: u128,
}
pub fn evaluate_history_selection_v8(
    plan: &HistorySelectionPlanV8,
    p: u32,
    source: &Tensor,
    limits: &HistorySelectionExecutionLimitsV8,
) -> Result<Tensor, String> {
    let (shape, out_shape) = plan.shapes(p)?;
    let (h, k) = (shape[0], out_shape[0]);
    let rows: usize = plan.row_shape.iter().map(|d| *d as usize).product();
    if source.dtype != plan.dtype || source.shape != shape || source.data.len() != rows * h {
        return Err("history-selection source differs from declared typed extent".into());
    }
    if rows as u128 * h as u128 * 16 > limits.source_reference_bytes
        || rows as u128 * k as u128 * 16 > limits.output_reference_bytes
        || rows as u128 * line_work(h as u32, k as u32) > limits.selection_work
        || 64 * k as u128 > limits.auxiliary_bytes
    {
        return Err("history-selection execution exceeds declared resource limits".into());
    }
    if !crate::verify::canonical_tensor_v1(source) {
        return Err("history-selection source has noncanonical values".into());
    }
    let mut out = vec![0i128; rows * k];
    for row in 0..rows {
        let mut kept: BinaryHeap<(Reverse<i128>, u32)> = BinaryHeap::new();
        kept.try_reserve_exact(k).map_err(|_| "history-selection allocation refused")?;
        for t in 0..h {
            let key = (Reverse(source.data[t * rows + row]), t as u32);
            if kept.len() < k {
                kept.push(key);
            } else if key < *kept.peek().unwrap() {
                *kept.peek_mut().unwrap() = key;
            }
        }
        let mut ids: Vec<_> = kept.into_iter().map(|(_, i)| i).collect();
        ids.sort_unstable();
        for (slot, id) in ids.into_iter().enumerate() {
            out[slot * rows + row] = id as i128;
        }
    }
    Tensor::new(DType::Idx, out_shape, out).map_err(|e| e.to_string())
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct HistorySelectionFaultV8 {
    pub version: u16,
    pub plan_root: Digest,
    pub job_root: Digest,
    pub row: u64,
    pub kind: TopKFaultKindV7,
    pub output: FlatOperandV5,
    pub source: FlatOperandV5,
}
impl HistorySelectionFaultV8 {
    fn structure(&self) -> io::Result<()> {
        if self.version != 8
            || self.output.node.is_none()
            || self.output.flat.len() + self.output.legacy.len() > 3
            || self.source.flat.len() + self.source.legacy.len() > 2
            || (!matches!(self.kind, TopKFaultKindV7::Dominance { .. }) && self.source != FlatOperandV5::default())
        {
            return Err(invalid());
        }
        common::parts_structure(None, std::slice::from_ref(&self.output), &None)?;
        common::parts_structure(None, std::slice::from_ref(&self.source), &None)
    }
    pub fn to_bytes(&self) -> io::Result<Vec<u8>> {
        self.structure()?;
        common::size_with_limit(self, HISTORY_SELECTION_MAX_FAULT_BYTES_V8)?;
        borsh::to_vec(self)
    }
    pub fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() as u64 > HISTORY_SELECTION_MAX_FAULT_BYTES_V8 {
            return Err(invalid());
        }
        let mut r = Cursor::new(bytes);
        let version = u16::deserialize_reader(&mut r)?;
        if version != 8 {
            return Err(invalid());
        }
        let out = Self {
            version,
            plan_root: Digest::deserialize_reader(&mut r)?,
            job_root: Digest::deserialize_reader(&mut r)?,
            row: u64::deserialize_reader(&mut r)?,
            kind: TopKFaultKindV7::deserialize_reader(&mut r)?,
            output: operand(&mut r, 3)?,
            source: operand(&mut r, 2)?,
        };
        if r.position() as usize != bytes.len() {
            return Err(invalid());
        }
        out.structure()?;
        Ok(out)
    }
}
pub(crate) fn source_point(o: &FlatOperandV5, dtype: DType, shape: &[usize], index: u64) -> Result<i128, DismissalV1> {
    let root = o.node.as_ref().ok_or_else(|| na("missing history source node"))?.commitment;
    let dims: Vec<_> = shape.iter().map(|n| *n as u64).collect();
    if shape.iter().product::<usize>() >= 64 {
        if !o.legacy.is_empty()
            || o.flat.windows(2).any(|w| w[0].opening.index >= w[1].opening.index)
            || o.flat.iter().any(|l| l.opening.dtype != dtype.tag() || l.opening.shape != dims || !l.authenticates(&root))
        {
            return Err(na("unauthentic history score leaves"));
        }
        for l in &o.flat {
            let (first, count) =
                l.opening.layout().and_then(|x| x.span(l.opening.index)).ok_or_else(|| na("bad history source layout"))?;
            if index >= first && index - first < count as u64 {
                let at = (index - first) as usize * dtype.width();
                return Ok(dtype.decode_le(&l.bytes[at..at + dtype.width()]));
            }
        }
    } else {
        if !o.flat.is_empty()
            || o.legacy.windows(2).any(|w| (w[0].line, w[0].tile) >= (w[1].line, w[1].tile))
            || o.legacy.iter().any(|l| l.dtype != dtype.tag() || l.shape != dims || l.axis != AXIS_ROW || !l.authenticates(&root))
        {
            return Err(na("unauthentic legacy history score leaves"));
        }
        let (line, tile, off) = LayoutV3::of(shape).row_leaf_of(index).ok_or_else(|| na("bad history score index"))?;
        if let Some(l) = o.legacy.iter().find(|l| l.line == line && l.tile == tile) {
            return l.values.get(off as usize).copied().ok_or_else(|| na("missing history score"));
        }
    }
    Err(na("missing history score point"))
}
pub fn verify_history_selection_fault_v8(c: &HistorySelectionContextV8<'_>, f: &HistorySelectionFaultV8) -> Result<(), DismissalV1> {
    c.check()?;
    f.structure().map_err(|_| na("malformed history-selection proof"))?;
    if f.plan_root != c.plan.root() || f.job_root != c.job.root() || common::size_with_limit(f, c.scope.filing_bytes).is_err() {
        return Err(na("history-selection proof is outside its job/program/scope"));
    }
    let output = f.output.node.as_ref().unwrap();
    let p = output.position;
    if !c.node(output, c.plan.output, p) {
        return Err(na("history-selection output node differs"));
    }
    let (shape, out) = c.plan.shapes(p).map_err(|s| na(&s))?;
    let (h, k) = (shape[0] as u32, out[0] as u32);
    let selected = |slot| output_point(&f.output, &out, at(&out, 0, f.row, slot)?);
    let convict = match f.kind {
        TopKFaultKindV7::Range { slot } => selected(slot)? >= h as i128,
        TopKFaultKindV7::Order { left } => {
            selected(left)? >= selected(left.checked_add(1).ok_or_else(|| na("history order overflow"))?)?
        }
        TopKFaultKindV7::Dominance { selected: slot, rival, gap } => {
            let a = selected(slot)?;
            if a >= h as i128
                || rival >= h
                || gap > k
                || rival as i128 == a
                || (gap > 0 && selected(gap - 1)? >= rival as i128)
                || (gap < k && selected(gap)? <= rival as i128)
            {
                return Err(na("invalid history dominance gap"));
            }
            let source = f.source.node.as_ref().ok_or_else(|| na("missing history source node"))?;
            if !c.node(source, c.plan.source, p) {
                return Err(na("history score node differs"));
            }
            let av = source_point(&f.source, c.plan.dtype, &shape, at(&shape, 0, f.row, a as u32)?)?;
            let bv = source_point(&f.source, c.plan.dtype, &shape, at(&shape, 0, f.row, rival)?)?;
            bv > av || (bv == av && (rival as i128) < a)
        }
    };
    if convict { Ok(()) } else { Err(DismissalV1::NoFault) }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistorySelectionFindingV8 {
    Clean,
    Fault(HistorySelectionFaultV8),
    Demand(FlatValueReadV7),
    Limit(&'static str),
    Inconsistent(String),
}
pub struct HistorySelectionBudgetV8 {
    pub attempts: u32,
    pub selection_work: u128,
    pub auxiliary_bytes: u128,
    pub selected_reads: u128,
    pub reply_bytes: u128,
    pub source_hash_bytes: u128,
    pub source_tree_bytes: u128,
    pub selected_auth_hash_bytes: u128,
}
/// The caller supplies source/output node openings, authenticated input/job
/// data and its own history. Every attempt/read is debited before its callback;
/// missing or substituted material spends the attempt and never returns Clean.
pub fn check_history_selection_v8(
    c: &HistorySelectionContextV8<'_>,
    p: u32,
    source: &Tensor,
    source_node: &NodeOpeningV1,
    output: &NodeOpeningV1,
    public: &dyn FlatPublicReadsV7,
    budget: &mut HistorySelectionBudgetV8,
) -> HistorySelectionFindingV8 {
    use HistorySelectionFindingV8 as F;
    let run = (|| -> Result<F, String> {
        c.check().map_err(|e| format!("{e:?}"))?;
        let (shape, out) = c.plan.shapes(p)?;
        if !c.node(source_node, c.plan.source, p)
            || !c.node(output, c.plan.output, p)
            || source.dtype != c.plan.dtype
            || source.shape != shape
            || source.data.len() as u128 != shape.iter().map(|n| *n as u128).product::<u128>()
        {
            return Err("history-selection replay source metadata/node differs".into());
        }
        let (h, k) = (shape[0] as u32, out[0] as u32);
        let rows = c.scope.rows;
        let work = rows as u128 * line_work(h, k);
        let hash = c.scope.source_hash_bytes_per_attempt;
        if budget.attempts == 0 {
            return Ok(F::Limit("attempts"));
        }
        if budget.auxiliary_bytes < 64 * k as u128 {
            return Ok(F::Limit("selection auxiliary bytes"));
        }
        if budget.selection_work < work {
            return Ok(F::Limit("selection work"));
        }
        if budget.source_hash_bytes < hash {
            return Ok(F::Limit("source hash bytes"));
        }
        if budget.source_tree_bytes < c.scope.source_tree_bytes {
            return Ok(F::Limit("source tree bytes"));
        }
        budget.attempts -= 1;
        budget.selection_work -= work;
        budget.source_hash_bytes -= hash;
        if !crate::verify::canonical_tensor_v1(source) {
            return Err("history-selection source has noncanonical values".into());
        }
        if flat_profile_commitment_v5(source).ok() != Some(source_node.commitment) {
            return Err("independent history is not the committed source".into());
        }
        let unit = FlatReadV5 { position: p, occurrence: c.plan.output.occurrence, node: c.plan.output.node, leaf: None };
        let auxiliary_ceiling = budget.auxiliary_bytes;
        let mut take = |element: u64| -> Result<(u32, FlatOperandV5), F> {
            let read = FlatValueReadV7 { node: unit, element };
            if budget.selected_reads == 0 {
                return Err(F::Limit("selected reads"));
            }
            if budget.reply_bytes < TOPK_VALUE_MAX_BYTES_V7 as u128 {
                return Err(F::Limit("selected reply bytes"));
            }
            if budget.selected_auth_hash_bytes < 4 * TOPK_VALUE_MAX_BYTES_V7 as u128 {
                return Err(F::Limit("selected authentication hash bytes"));
            }
            budget.selected_reads -= 1;
            budget.reply_bytes -= TOPK_VALUE_MAX_BYTES_V7 as u128;
            budget.selected_auth_hash_bytes -= 4 * TOPK_VALUE_MAX_BYTES_V7 as u128;
            let Some(reply) =
                public.value(read).and_then(|b| FlatValueV7::from_bytes(&b).ok()).filter(|r| r.value.node.as_ref() == Some(output))
            else {
                return Err(F::Demand(read));
            };
            let Some(id) = output_point(&reply.value, &out, element).ok().and_then(|v| u32::try_from(v).ok()) else {
                return Err(F::Demand(read));
            };
            Ok((id, reply.value))
        };
        let mut internal = TopKScanBudgetV7 { remaining_work: work, auxiliary_ceiling };
        for row in 0..rows {
            let mut selected = Vec::with_capacity(k as usize);
            for slot in 0..k {
                match take(at(&out, 0, row, slot).map_err(|e| format!("{e:?}"))?) {
                    Ok((id, _)) => selected.push(id),
                    Err(f) => return Ok(f),
                }
            }
            let kind =
                detect_topk_line_v7(h, k, &selected, &mut internal, |t| source.data.get((t as u64 * rows + row) as usize).copied())?;
            let Some(kind) = kind else { continue };
            let slots: BTreeSet<_> = match kind {
                TopKFaultKindV7::Range { slot } => [slot].into(),
                TopKFaultKindV7::Order { left } => [left, left + 1].into(),
                TopKFaultKindV7::Dominance { selected, gap, .. } => {
                    [Some(selected), gap.checked_sub(1), (gap < k).then_some(gap)].into_iter().flatten().collect()
                }
            };
            let mut proof = FlatOperandV5 { node: Some(output.clone()), flat: vec![], legacy: vec![] };
            for slot in slots {
                let reply = match take(at(&out, 0, row, slot).map_err(|e| format!("{e:?}"))?) {
                    Ok((_, p)) => p,
                    Err(f) => return Ok(f),
                };
                for leaf in reply.flat {
                    if !proof.flat.contains(&leaf) {
                        proof.flat.push(leaf);
                    }
                }
                for leaf in reply.legacy {
                    if !proof.legacy.contains(&leaf) {
                        proof.legacy.push(leaf);
                    }
                }
            }
            proof.flat.sort_by_key(|l| l.opening.index);
            proof.legacy.sort_by_key(|l| (l.line, l.tile));
            let own = if let TopKFaultKindV7::Dominance { selected: slot, rival, .. } = kind {
                let indices = [selected[slot as usize], rival].into_iter().map(|t| t as u64 * rows + row);
                let leaves = indices
                    .map(|i| {
                        if source.data.len() >= 64 {
                            (
                                crate::merkle5::FlatLayoutV5::of(&shape.iter().map(|d| *d as u64).collect::<Vec<_>>())
                                    .unwrap()
                                    .leaf_of(i)
                                    .unwrap(),
                                0,
                            )
                        } else {
                            let (r, t, _) = LayoutV3::of(&shape).row_leaf_of(i).unwrap();
                            (r, t)
                        }
                    })
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();
                value_proof(source, Some(source_node.clone()), &leaves).map_err(|e| format!("{e:?}"))?
            } else {
                FlatOperandV5::default()
            };
            let f = HistorySelectionFaultV8 {
                version: 8,
                plan_root: c.plan.root(),
                job_root: c.job.root(),
                row,
                kind,
                output: proof,
                source: own,
            };
            let f = f.to_bytes().and_then(|b| HistorySelectionFaultV8::from_bytes(&b)).map_err(|e| e.to_string())?;
            verify_history_selection_fault_v8(c, &f).map_err(|e| format!("{e:?}"))?;
            return Ok(F::Fault(f));
        }
        Ok(F::Clean)
    })();
    run.unwrap_or_else(F::Inconsistent)
}
