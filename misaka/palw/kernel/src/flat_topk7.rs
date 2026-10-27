//! Opt-in v7 TopK counterexamples over unchanged flat-v5 internal/v3 parameter commitments.
//! Range, adjacent set order, and a dominating omitted member cover the frozen v1
//! TopK rule. This component changes neither v1 semantics nor any existing proof
//! decoder/route. A future consumer must explicitly bind this court id and its
//! complete resource/profile/DA contract; no reward or execution rights are granted.
use crate::{
    element::{OperandSourceV1, OperandsV1, node_opening_bytes_v1, operand_sources},
    flat_court4::{self as common},
    flat_court5::{FlatOperandV5, FlatSegContextV5},
    hash::{Digest, object_id},
    merkle::AXIS_ROW,
    merkle3::LayoutV3,
    merkle5::{FlatLeafValueV5, flat_material_bounds_v5},
    trace::SourceV1,
    verify::DismissalV1,
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::{DType, Prim};
use std::{
    cmp::Reverse,
    collections::BinaryHeap,
    io::{self, Cursor},
};

pub const TOPK_FAULT_VERSION_V7: u16 = 7;
pub const TOPK_FAULT_MAX_BYTES_V7: u64 = 128 << 10;
pub const TOPK_SCAN_MAX_AUX_BYTES_V7: u128 = 32 << 20;
pub fn topk_court_id_v7() -> Digest {
    crate::hash::id(b"misaka-palw/kernel/topk-point-court/v7", b"tir-v1:score-desc/id-asc;set-id-asc;range/order/omitted-dominance;flat-v5-internal/v3-param;3-output+2-input-leaves;128KiB;bounded-codec;scan-N-log-K")
}

pub struct TopKContextV7<'a> {
    pub court_id: Digest,
    pub base: FlatSegContextV5<'a>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum TopKFaultKindV7 {
    Range {
        slot: u32,
    } = 0,
    Order {
        left: u32,
    } = 1,
    /// gap is the insertion slot of rival in the emitted ascending set. Its two
    /// adjacent members (when present) authenticate that rival was omitted.
    Dominance {
        selected: u32,
        rival: u32,
        gap: u32,
    } = 2,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct TopKFaultV7 {
    pub version: u16,
    pub row: u64,
    pub kind: TopKFaultKindV7,
    pub output: FlatOperandV5,
    pub scores: FlatOperandV5,
}
fn bad(s: &str) -> DismissalV1 {
    DismissalV1::NotAuthentic(s.into())
}
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid bounded TopK-v7 fault")
}
pub(crate) fn operand(r: &mut Cursor<&[u8]>, max: usize) -> io::Result<FlatOperandV5> {
    let node = common::optional(r, common::read_node)?;
    let flat = common::vector(r, max, FlatLeafValueV5::deserialize_reader)?;
    let legacy = common::vector(r, max - flat.len(), common::read_legacy)?;
    Ok(FlatOperandV5 { node, flat, legacy })
}
impl TopKFaultV7 {
    pub fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() as u64 > TOPK_FAULT_MAX_BYTES_V7 {
            return Err(invalid());
        }
        let mut r = Cursor::new(bytes);
        let version = u16::deserialize_reader(&mut r)?;
        if version != TOPK_FAULT_VERSION_V7 {
            return Err(invalid());
        }
        let out = Self {
            version,
            row: u64::deserialize_reader(&mut r)?,
            kind: TopKFaultKindV7::deserialize_reader(&mut r)?,
            output: operand(&mut r, 3)?,
            scores: operand(&mut r, 2)?,
        };
        if r.position() != bytes.len() as u64 {
            return Err(invalid());
        }
        out.structure()?;
        Ok(out)
    }
    fn structure(&self) -> io::Result<()> {
        if self.version != TOPK_FAULT_VERSION_V7
            || self.output.node.is_none()
            || self.output.flat.len() + self.output.legacy.len() > 3
            || self.scores.flat.len() + self.scores.legacy.len() > 2
        {
            return Err(invalid());
        }
        common::parts_structure(None, std::slice::from_ref(&self.output), &None)?;
        common::parts_structure(None, std::slice::from_ref(&self.scores), &None)?;
        if !matches!(self.kind, TopKFaultKindV7::Dominance { .. }) && self.scores != FlatOperandV5::default() {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn to_bytes(&self) -> io::Result<Vec<u8>> {
        self.structure()?;
        common::size_with_limit(self, TOPK_FAULT_MAX_BYTES_V7)?;
        borsh::to_vec(self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct TopKResourceScopeV7 {
    pub version: u16,
    pub court_id: Digest,
    pub program: Digest,
    pub position: u32,
    pub occurrence: u16,
    pub node: u16,
    pub axis_len: u32,
    pub k: u32,
    pub rows: u64,
    /// Conservative comparison/selection work, not hardware instructions or PWU.
    pub selection_work: u128,
    /// Own temporary selection/checker memory, excluding source/output residency.
    pub selection_aux_bytes: u128,
    pub filing_bytes: u64,
    pub court_work: u64,
    pub court_scratch_bytes: u64,
}
impl TopKResourceScopeV7 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/topk-resource-scope/v7", self)
    }
}
fn line_resources(n: u32, k: u32) -> Result<(u128, u128), String> {
    if n == 0 || n > misaka_palw_tir::types::MAX_DIM || k == 0 || k > n {
        return Err("invalid TopK extent".into());
    }
    let log = 32 - k.leading_zeros();
    let work = n as u128 * (4 * log as u128 + 16) + k as u128 * (4 * log as u128 + 16);
    let aux = k as u128 * 64;
    Ok((work, aux))
}
fn opening_bytes(dtype: DType, shape: &[usize], flat: bool) -> Result<u64, DismissalV1> {
    let dims: Vec<_> = shape.iter().map(|n| *n as u64).collect();
    if flat {
        return flat_material_bounds_v5(dtype, &dims).map(|b| b.leaf_value_bytes).ok_or_else(|| bad("invalid point layout"));
    }
    let l = LayoutV3::try_of(shape).ok_or_else(|| bad("invalid legacy point layout"))?;
    Ok(128
        + 8 * shape.len() as u64
        + l.row_len.min(crate::merkle3::TILE_V3) * dtype.width() as u64
        + 64 * crate::merkle3::depth_v3(l.leaves(AXIS_ROW)))
}
fn derived(
    c: &TopKContextV7<'_>,
    pr: &crate::element::PreparedV1<'_>,
    p: u32,
    s: u16,
    n: u16,
) -> Result<TopKResourceScopeV7, DismissalV1> {
    if c.court_id != topk_court_id_v7() || p >= c.base.claim.positions || !pr.valid(s, n) {
        return Err(bad("wrong opt-in court or relation"));
    }
    relation_scope_v7(c.base.claim.program, pr, c.base.claim.positions, p, s, n, false)
}
/// Metadata-only derivation for complete graph admission. No parameter image,
/// position/root array or public callback is needed.
pub(crate) struct TopKProgramPriceV7 {
    pub bytes: u64,
    pub root: Digest,
}
impl TopKProgramPriceV7 {
    pub(crate) fn of(program: &misaka_palw_tir::program::TirProgramV1) -> Result<Self, DismissalV1> {
        Ok(Self {
            bytes: common::size_with_limit(program, misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64)
                .map_err(|_| bad("oversized program"))?,
            root: object_id(b"misaka-palw/kernel/topk-program/v7", program),
        })
    }
}
pub(crate) fn relation_scope_v7(
    program: &misaka_palw_tir::program::TirProgramV1,
    pr: &crate::element::PreparedV1<'_>,
    max_positions: u32,
    p: u32,
    s: u16,
    n: u16,
    envelope: bool,
) -> Result<TopKResourceScopeV7, DismissalV1> {
    relation_scope_priced_v7(&TopKProgramPriceV7::of(program)?, pr, max_positions, p, s, n, envelope)
}
/// Serialize/hash the program once for whole-graph admission, rather than once
/// per unrolled TopK relation. The public point API derives this price itself.
pub(crate) fn relation_scope_priced_v7(
    price: &TopKProgramPriceV7,
    pr: &crate::element::PreparedV1<'_>,
    max_positions: u32,
    p: u32,
    s: u16,
    n: u16,
    envelope: bool,
) -> Result<TopKResourceScopeV7, DismissalV1> {
    if p >= max_positions || !pr.valid(s, n) {
        return Err(bad("wrong TopK relation extent"));
    }
    let Prim::TopK { axis, k } = pr.w.node(s, n).prim else { return Err(bad("relation is not TopK")) };
    let sh = pr.shapes(p, s, n);
    let axis = axis as usize;
    let sources = operand_sources(pr, p, s, n).map_err(DismissalV1::NotAuthentic)?;
    if sources.len() != 1 || !matches!(sources[0], OperandSourceV1::Source(SourceV1::Node { .. } | SourceV1::Param { .. })) {
        return Err(bad("point TopK needs a committed node or registered parameter"));
    }
    let source_flat =
        matches!(sources[0], OperandSourceV1::Source(SourceV1::Node { .. })) && common::uses_flat::<5>(sh.in_dtypes[0], &sh.inputs[0]);
    let mut source = opening_bytes(sh.in_dtypes[0], &sh.inputs[0], source_flat)?;
    let mut output = opening_bytes(DType::Idx, &sh.out, common::uses_flat::<5>(DType::Idx, &sh.out))?;
    if envelope {
        // A monotone H may cross the internal 64-element grammar threshold.
        // The earlier legacy tree can have MORE row paths than the final flat
        // tree (e.g. [H, 1] at H=63 vs 64). Price every sub-64 layout, not just
        // the first and last position, without visiting positions.
        let first = pr.shapes(0, s, n);
        let small = |dtype: DType, rank: usize| 128 + 8 * rank as u64 + 63 * dtype.width() as u64 + 64 * crate::merkle3::depth_v3(63);
        if source_flat && LayoutV3::of(&first.inputs[0]).len < 64 {
            source = source.max(small(sh.in_dtypes[0], sh.inputs[0].len()));
        }
        if common::uses_flat::<5>(DType::Idx, &sh.out) && LayoutV3::of(&first.out).len < 64 {
            output = output.max(small(DType::Idx, sh.out.len()));
        }
    }
    let nodes = 2 * node_opening_bytes_v1(pr.node_count);
    let filing_bytes = 1024 + nodes + 2 * source + 3 * output;
    if filing_bytes > TOPK_FAULT_MAX_BYTES_V7 {
        return Err(bad("TopK point filing exceeds cap"));
    }
    let bytes = price.bytes;
    let court_work = bytes
        .checked_mul(bytes)
        .and_then(|w| w.checked_add(bytes * 64))
        .and_then(|w| w.checked_add(filing_bytes * 128))
        .and_then(|w| w.checked_add(pr.node_count * 1024))
        .ok_or_else(|| bad("TopK work overflows"))?;
    if court_work > common::FLAT_FAULT_MAX_WORK_V4 {
        return Err(bad("TopK metadata court exceeds work cap"));
    }
    let axis_len = sh.inputs[0][axis] as u32;
    let rows = LayoutV3::of(&sh.inputs[0]).len / axis_len as u64;
    let (work, aux) = line_resources(axis_len, k).map_err(DismissalV1::NotAuthentic)?;
    // Proof decoding and legacy i128 values plus validated wiring; no axis-sized allocation.
    let court_scratch_bytes = filing_bytes * 16 + bytes * 32 + pr.node_count * 64 + 4096;
    if court_scratch_bytes > common::FLAT_FAULT_MAX_SCRATCH_V4 {
        return Err(bad("TopK court scratch exceeds cap"));
    }
    Ok(TopKResourceScopeV7 {
        version: 7,
        court_id: topk_court_id_v7(),
        program: price.root,
        position: p,
        occurrence: s,
        node: n,
        axis_len,
        k,
        rows,
        selection_work: work * rows as u128,
        selection_aux_bytes: aux,
        filing_bytes,
        court_work,
        court_scratch_bytes,
    })
}
pub fn topk_resource_scope_v7(c: &TopKContextV7<'_>, p: u32, s: u16, n: u16) -> Result<TopKResourceScopeV7, DismissalV1> {
    let pr = common::prepare(&c.base)?;
    derived(c, &pr, p, s, n)
}
pub fn check_topk_resource_scope_v7(c: &TopKContextV7<'_>, scope: &TopKResourceScopeV7) -> Result<(), DismissalV1> {
    if topk_resource_scope_v7(c, scope.position, scope.occurrence, scope.node)? != *scope {
        return Err(bad("TopK scope differs from independent derivation"));
    }
    Ok(())
}
pub(crate) fn at(shape: &[usize], axis: usize, row: u64, slot: u32) -> Result<u64, DismissalV1> {
    let suffix = shape[axis + 1..].iter().product::<usize>() as u64;
    let rows = LayoutV3::of(shape).len / shape[axis] as u64;
    if row >= rows || slot as usize >= shape[axis] {
        return Err(bad("TopK coordinate out of range"));
    }
    Ok(((row / suffix) * shape[axis] as u64 + slot as u64) * suffix + row % suffix)
}
pub(crate) fn output_point(o: &FlatOperandV5, shape: &[usize], index: u64) -> Result<i128, DismissalV1> {
    let root = o.node.as_ref().ok_or_else(|| bad("missing output node"))?.commitment;
    let dims: Vec<_> = shape.iter().map(|n| *n as u64).collect();
    if common::uses_flat::<5>(DType::Idx, shape) {
        if !o.legacy.is_empty()
            || o.flat.windows(2).any(|w| w[0].opening.index >= w[1].opening.index)
            || o.flat.iter().any(|l| l.opening.dtype != DType::Idx.tag() || l.opening.shape != dims || !l.authenticates(&root))
        {
            return Err(bad("unauthentic selected leaves"));
        }
        for l in &o.flat {
            let (first, count) = l.opening.layout().and_then(|x| x.span(l.opening.index)).ok_or_else(|| bad("bad output layout"))?;
            if index >= first && index - first < count as u64 {
                let off = (index - first) as usize * 4;
                return Ok(DType::Idx.decode_le(&l.bytes[off..off + 4]));
            }
        }
    } else {
        if !o.flat.is_empty()
            || o.legacy.windows(2).any(|w| (w[0].line, w[0].tile) >= (w[1].line, w[1].tile))
            || o.legacy.iter().any(|l| l.dtype != DType::Idx.tag() || l.shape != dims || l.axis != AXIS_ROW || !l.authenticates(&root))
        {
            return Err(bad("unauthentic selected legacy leaves"));
        }
        let (line, tile, offset) = LayoutV3::of(shape).row_leaf_of(index).ok_or_else(|| bad("bad output index"))?;
        if let Some(l) = o.legacy.iter().find(|l| l.line == line && l.tile == tile) {
            return l.values.get(offset as usize).copied().ok_or_else(|| bad("missing selected value"));
        }
    }
    Err(bad("missing selected point"))
}
/// No whole-axis reconstruction. Sound even if the emitted set has other faults:
/// a correct sorted TopK set cannot have a dominated member and an authenticated gap.
pub fn verify_topk_fault_v7(c: &TopKContextV7<'_>, f: &TopKFaultV7) -> Result<(), DismissalV1> {
    let pr = common::prepare(&c.base)?;
    verify_topk_prepared_v32(c, f, &pr)
}
pub(crate) fn verify_topk_prepared_v32(
    c: &TopKContextV7<'_>,
    f: &TopKFaultV7,
    pr: &crate::element::PreparedV1<'_>,
) -> Result<(), DismissalV1> {
    f.structure().map_err(|_| bad("malformed TopK fault"))?;
    let o = f.output.node.as_ref().unwrap();
    let (p, s, n) = (o.position, o.occurrence, o.node);
    let scope = derived(c, &pr, p, s, n)?;
    if !common::authenticate_node(&c.base, &pr, o, (p, s, n)) {
        return Err(bad("TopK output not bound"));
    }
    if common::size_with_limit(f, scope.filing_bytes).is_err() {
        return Err(bad("TopK filing exceeds its derived bound"));
    }
    let Prim::TopK { axis, k } = pr.w.node(s, n).prim else { unreachable!() };
    let axis = axis as usize;
    let sh = pr.shapes(p, s, n);
    let selected = |slot| output_point(&f.output, &sh.out, at(&sh.out, axis, f.row, slot)?);
    let convict = match f.kind {
        TopKFaultKindV7::Range { slot } => selected(slot)? >= scope.axis_len as i128,
        TopKFaultKindV7::Order { left } => {
            let right = left.checked_add(1).ok_or_else(|| bad("TopK order overflow"))?;
            selected(left)? >= selected(right)?
        }
        TopKFaultKindV7::Dominance { selected: slot, rival, gap } => {
            let a = selected(slot)?;
            if a >= scope.axis_len as i128 || rival >= scope.axis_len || gap > k || rival as i128 == a {
                return Err(bad("invalid dominance indices"));
            }
            if (gap > 0 && selected(gap - 1)? >= rival as i128) || (gap < k && selected(gap)? <= rival as i128) {
                return Err(bad("rival is not inside the authenticated omitted-member gap"));
            }
            let ops = common::authenticate_inputs(&c.base, &pr, p, s, n, std::slice::from_ref(&f.scores), &None, &[2])?;
            let av = ops.get(0, at(&sh.inputs[0], axis, f.row, a as u32)?).map_err(|_| bad("missing selected score"))?;
            let bv = ops.get(0, at(&sh.inputs[0], axis, f.row, rival)?).map_err(|_| bad("missing rival score"))?;
            bv > av || (bv == av && (rival as i128) < a)
        }
    };
    if convict { Ok(()) } else { Err(DismissalV1::NoFault) }
}

/// One local acquisition/retry session. Precharge before the first callback;
/// an incomplete read spends that attempt. Caller authenticates score/set reads
/// to the same roots; the terminal court independently authenticates its openings.
#[derive(Debug)]
pub struct TopKScanBudgetV7 {
    pub remaining_work: u128,
    pub auxiliary_ceiling: u128,
}
pub fn detect_topk_line_v7(
    n: u32,
    k: u32,
    selected: &[u32],
    budget: &mut TopKScanBudgetV7,
    mut score: impl FnMut(u32) -> Option<i128>,
) -> Result<Option<TopKFaultKindV7>, String> {
    let (work, aux) = line_resources(n, k)?;
    if selected.len() != k as usize
        || aux > TOPK_SCAN_MAX_AUX_BYTES_V7
        || aux > budget.auxiliary_ceiling
        || work > budget.remaining_work
    {
        return Err("TopK scan exceeds its shape/work/auxiliary envelope".into());
    }
    budget.remaining_work -= work;
    if let Some(slot) = selected.iter().position(|v| *v >= n) {
        return Ok(Some(TopKFaultKindV7::Range { slot: slot as u32 }));
    }
    if let Some(left) = selected.windows(2).position(|w| w[0] >= w[1]) {
        return Ok(Some(TopKFaultKindV7::Order { left: left as u32 }));
    }
    let mut kept: BinaryHeap<(Reverse<i128>, u32)> = BinaryHeap::new();
    kept.try_reserve_exact(k as usize).map_err(|_| "TopK heap allocation refused")?;
    for i in 0..n {
        let v = score(i).ok_or("incomplete TopK score acquisition")?;
        let key = (Reverse(v), i);
        if kept.len() < k as usize {
            kept.push(key)
        } else if key < *kept.peek().unwrap() {
            *kept.peek_mut().unwrap() = key;
        }
    }
    let mut expected: Vec<_> = kept.into_iter().map(|(_, i)| i).collect();
    expected.sort_unstable();
    if expected == selected {
        return Ok(None);
    }
    let slot = selected.iter().position(|i| expected.binary_search(i).is_err()).unwrap();
    let rival = *expected.iter().find(|i| selected.binary_search(i).is_err()).unwrap();
    let gap = selected.binary_search(&rival).unwrap_err() as u32;
    Ok(Some(TopKFaultKindV7::Dominance { selected: slot as u32, rival, gap }))
}
