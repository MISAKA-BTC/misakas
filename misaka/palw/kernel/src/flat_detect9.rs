//! Independently replayed full-graph locator for the isolated mixed court9.
//! The provider supplies authenticated headers/hash paths and one typed output
//! leaf. Own input values come only from independently acquired registered
//! parameters and replay of the authenticated job. Missing or borrowed replies
//! produce demands; no successful replay grants native execution or Final.
use crate::{
    element::{OperandSourceV1, PreparedV1, operand_sources},
    flat_court4 as common,
    flat_court5::{FlatFaultV5, FlatHeaderV5, FlatMalformedFaultV5, FlatOperandV5, flat_profile_commitment_v5},
    flat_court9::{FlatContextV9, FlatFaultV9, verify_flat_fault_v9},
    flat_detect5::{FlatFindingV5, FlatPublicReadsV5, FlatReadV5},
    flat_point9::{FlatPointFaultV9, PointContextV9, PointOperandKindV9, ordinary_point_court_id_v9, point_dependencies_v9},
    flat_topk7::{TopKFaultKindV7, TopKFaultV7, TopKScanBudgetV7, at, detect_topk_line_v7},
    hash::Digest,
    merkle::{AXIS_COL, AXIS_ROW},
    merkle3::{LayoutV3, LeafOpeningV3, TreesV3},
    merkle5::{FlatDescentV5, FlatTreeV5},
    trace::SourceV1,
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::{DType, Prim, Tensor};
use std::{
    collections::BTreeSet,
    io::{self, Cursor},
};
pub const TYPED_VALUE_MAX_BYTES_V9: u64 = 16 << 10;
pub const TYPED_HEADER_MAX_BYTES_V9: u64 = 512;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatValueV9 {
    pub version: u16,
    pub scope: Digest,
    pub value: FlatOperandV5,
}
impl FlatValueV9 {
    fn structure(&self) -> io::Result<()> {
        if self.version != 9 || self.value.node.is_none() || self.value.flat.len() + self.value.legacy.len() != 1 {
            return Err(invalid());
        }
        common::parts_structure(None, std::slice::from_ref(&self.value), &None)
    }
    pub fn to_bytes(&self) -> io::Result<Vec<u8>> {
        self.structure()?;
        common::size_with_limit(self, TYPED_VALUE_MAX_BYTES_V9)?;
        borsh::to_vec(self)
    }
    pub fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() as u64 > TYPED_VALUE_MAX_BYTES_V9 {
            return Err(invalid());
        }
        let mut r = Cursor::new(bytes);
        let version = u16::deserialize_reader(&mut r)?;
        if version != 9 {
            return Err(invalid());
        }
        let scope = Digest::deserialize_reader(&mut r)?;
        let value = crate::flat_topk7::operand(&mut r, 1)?;
        if r.position() != bytes.len() as u64 {
            return Err(invalid());
        }
        let out = Self { version, scope, value };
        out.structure()?;
        Ok(out)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatHeaderValueV9 {
    pub version: u16,
    pub scope: Digest,
    pub header: FlatHeaderV5,
}
impl FlatHeaderValueV9 {
    /// Authenticate only the bounded typed header image. The caller separately
    /// authenticates the node, scope and job. Zero, small or overflowing shapes
    /// are intentionally hashable: an authentic wrong header must remain
    /// available to the existing malformed-header court.
    pub fn authenticates(&self, commitment: &Digest) -> bool {
        if self.version != 9 {
            return false;
        }
        let (tag, shape) = match &self.header {
            FlatHeaderV5::Flat { dtype, shape, .. } | FlatHeaderV5::Legacy { dtype, shape, .. } => (*dtype, shape),
        };
        if shape.len() > 4 || !DType::ALL.iter().any(|d| d.tag() == tag) {
            return false;
        }
        let root = match &self.header {
            FlatHeaderV5::Flat { root, .. } => crate::merkle4::header_commitment::<5>(tag, shape, root),
            FlatHeaderV5::Legacy { row_root, col_root, .. } => {
                let mut h = crate::hash::keyed(crate::merkle3::TENSOR_COMMITMENT_DOMAIN_V3);
                h.update(&[tag, shape.len() as u8]);
                for dim in shape {
                    h.update(&dim.to_le_bytes());
                }
                h.update(row_root).update(col_root);
                Some(crate::hash::finish(h))
            }
        };
        root.as_ref() == Some(commitment)
    }
    pub fn to_bytes(&self) -> io::Result<Vec<u8>> {
        if self.version != 9 {
            return Err(invalid());
        }
        let shape = match &self.header {
            FlatHeaderV5::Flat { shape, .. } | FlatHeaderV5::Legacy { shape, .. } => shape,
        };
        if shape.len() > 4 {
            return Err(invalid());
        }
        common::size_with_limit(self, TYPED_HEADER_MAX_BYTES_V9)?;
        borsh::to_vec(self)
    }
    pub fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() as u64 > TYPED_HEADER_MAX_BYTES_V9 {
            return Err(invalid());
        }
        let mut r = Cursor::new(bytes);
        let version = u16::deserialize_reader(&mut r)?;
        if version != 9 {
            return Err(invalid());
        }
        let scope = Digest::deserialize_reader(&mut r)?;
        let tag = u8::deserialize_reader(&mut r)?;
        let dtype = u8::deserialize_reader(&mut r)?;
        let shape = common::vector(&mut r, 4, u64::deserialize_reader)?;
        let header = match tag {
            0 => FlatHeaderV5::Legacy {
                dtype,
                shape,
                row_root: Digest::deserialize_reader(&mut r)?,
                col_root: Digest::deserialize_reader(&mut r)?,
            },
            1 => FlatHeaderV5::Flat { dtype, shape, root: Digest::deserialize_reader(&mut r)? },
            _ => return Err(invalid()),
        };
        if r.position() != bytes.len() as u64 {
            return Err(invalid());
        }
        Ok(Self { version, scope, header })
    }
}
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid bounded flat9 typed reply")
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlatValueReadV9 {
    pub scope: Digest,
    pub node: FlatReadV5,
    pub element: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlatHeaderReadV9 {
    pub scope: Digest,
    pub node: FlatReadV5,
}
pub trait FlatPublicReadsV9: FlatPublicReadsV5 {
    fn value(&self, read: FlatValueReadV9) -> Option<Vec<u8>>;
    fn header(&self, read: FlatHeaderReadV9) -> Option<Vec<u8>>;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlatFindingV9 {
    Clean,
    Fault(FlatFaultV9),
    Demand(FlatReadV5),
    ValueDemand(FlatValueReadV9),
    HeaderDemand(FlatHeaderReadV9),
    Inconsistent(String),
}
impl From<FlatFindingV5> for FlatFindingV9 {
    fn from(f: FlatFindingV5) -> Self {
        match f {
            FlatFindingV5::Clean => Self::Clean,
            FlatFindingV5::Fault(f) => Self::Fault(FlatFaultV9::Legacy(f)),
            FlatFindingV5::Demand(r) => Self::Demand(r),
            FlatFindingV5::Inconsistent(e) => Self::Inconsistent(e),
        }
    }
}
fn error(e: impl std::fmt::Debug) -> FlatFindingV9 {
    FlatFindingV9::Inconsistent(format!("{e:?}"))
}
fn checked(c: &FlatContextV9<'_>, f: FlatFaultV9) -> FlatFindingV9 {
    match f.to_bytes().and_then(|bytes| FlatFaultV9::from_bytes(&bytes)) {
        Err(e) => error(e),
        Ok(f) => match verify_flat_fault_v9(c, &f) {
            Ok(_) => FlatFindingV9::Fault(f),
            Err(e) => error(e),
        },
    }
}
/// Authenticating a reply does not establish the relation: only the mixed court
/// independently derives semantics from authenticated own-source input proofs.
pub fn authenticated_output_point_v9(o: &FlatOperandV5, dtype: DType, shape: &[usize], index: u64) -> Option<i128> {
    if o.flat.len() + o.legacy.len() != 1 {
        return None;
    }
    let root = o.node.as_ref()?.commitment;
    let dims = shape.iter().map(|n| *n as u64).collect::<Vec<_>>();
    if common::uses_flat::<5>(dtype, shape) {
        if !o.legacy.is_empty()
            || o.flat.iter().any(|l| l.opening.dtype != dtype.tag() || l.opening.shape != dims || !l.authenticates(&root))
        {
            return None;
        }
    } else if !o.flat.is_empty()
        || o.legacy.iter().any(|l| l.dtype != dtype.tag() || l.shape != dims || l.axis != AXIS_ROW || !l.authenticates(&root))
    {
        return None;
    }
    common::value_in_opening::<5>(o, dtype, shape, index)
}
#[allow(clippy::result_large_err)] // the finding IS the evidence, returned once per check; boxing it would change every caller
fn typed_point(
    scope_root: &Digest,
    public: &dyn FlatPublicReadsV9,
    unit: FlatReadV5,
    output: &crate::seg::NodeOpeningV1,
    dtype: DType,
    shape: &[usize],
    element: u64,
) -> Result<(i128, FlatOperandV5), FlatFindingV9> {
    let read = FlatValueReadV9 { scope: *scope_root, node: unit, element };
    let reply = public.value(read).and_then(|b| FlatValueV9::from_bytes(&b).ok());
    let Some(reply) = reply.filter(|r| r.scope == read.scope && r.value.node.as_ref() == Some(output)) else {
        return Err(FlatFindingV9::ValueDemand(read));
    };
    authenticated_output_point_v9(&reply.value, dtype, shape, element)
        .map(|v| (v, reply.value))
        .ok_or(FlatFindingV9::ValueDemand(read))
}
#[allow(clippy::result_large_err)] // the finding IS the evidence, returned once per check; boxing it would change every caller
fn point(
    scope_root: &Digest,
    public: &dyn FlatPublicReadsV9,
    unit: FlatReadV5,
    output: &crate::seg::NodeOpeningV1,
    shape: &[usize],
    element: u64,
) -> Result<(u32, FlatOperandV5), FlatFindingV9> {
    let (v, o) = typed_point(scope_root, public, unit, output, DType::Idx, shape, element)?;
    u32::try_from(v).map(|v| (v, o)).map_err(error)
}
#[allow(clippy::result_large_err)] // the finding IS the evidence, returned once per check; boxing it would change every caller
fn header(
    c: &FlatContextV9<'_>,
    scope_root: &Digest,
    public: &dyn FlatPublicReadsV9,
    unit: FlatReadV5,
    output: &crate::seg::NodeOpeningV1,
) -> Result<(), FlatFindingV9> {
    let read = FlatHeaderReadV9 { scope: *scope_root, node: unit };
    let Some(reply) = public.header(read).and_then(|b| FlatHeaderValueV9::from_bytes(&b).ok()).filter(|r| r.scope == read.scope)
    else {
        return Err(FlatFindingV9::HeaderDemand(read));
    };
    let f = FlatFaultV9::Legacy(FlatFaultV5::Malformed(FlatMalformedFaultV5 { output: output.clone(), header: reply.header }));
    match verify_flat_fault_v9(c, &f) {
        Ok(_) => Err(checked(c, f)),
        Err(crate::verify::DismissalV1::NoFault) => Ok(()),
        Err(_) => Err(FlatFindingV9::HeaderDemand(read)),
    }
}
fn topk_relation(
    c: &FlatContextV9<'_>,
    scope_root: &Digest,
    pr: &PreparedV1<'_>,
    public: &dyn FlatPublicReadsV9,
    unit: FlatReadV5,
    expected: &Tensor,
    own: &dyn Fn() -> Result<Vec<Tensor>, String>,
    scan: &mut TopKScanBudgetV7,
) -> FlatFindingV9 {
    let output = match crate::flat_detect5::read_node(&c.base, pr, public, unit) {
        Ok(o) => o,
        Err(f) => return f.into(),
    };
    let (p, s, n) = (unit.position, unit.occurrence, unit.node);
    let sh = pr.shapes(p, s, n);
    if expected.dtype != misaka_palw_tir::DType::Idx || expected.shape != sh.out {
        return error("reference TopK metadata differs from the program");
    }
    match flat_profile_commitment_v5(expected) {
        Ok(root) if root == output.commitment => return FlatFindingV9::Clean,
        Err(e) => return error(e),
        _ => {}
    }
    if let Err(f) = header(c, scope_root, public, unit, &output) {
        return f;
    }
    let Prim::TopK { axis, k } = pr.w.node(s, n).prim else { return error("wrong TopK relation") };
    let axis = axis as usize;
    let axis_len = sh.inputs[0][axis] as u32;
    let rows = LayoutV3::of(&sh.out).len / k as u64;
    let sources = match operand_sources(pr, p, s, n) {
        Ok(v) => v,
        Err(e) => return error(e),
    };
    let tensors = match own() {
        Ok(v) => v,
        Err(e) => return error(e),
    };
    let Some(t) = tensors.first().filter(|t| {
        t.dtype == sh.in_dtypes[0]
            && t.shape == sh.inputs[0]
            && t.data.len() as u64 == LayoutV3::of(&sh.inputs[0]).len
            && crate::verify::canonical_tensor_v1(t)
    }) else {
        return error("wrong independent TopK operand");
    };
    let source_node = match sources.as_slice() {
        [OperandSourceV1::Source(SourceV1::Param { index, layer })] => {
            if c.base.claim.params.by_instance.get(&(*index, *layer)) != Some(&crate::merkle3::tensor_commitment_v3(t)) {
                return error("own TopK parameter differs from the registered model");
            }
            None
        }
        [OperandSourceV1::Source(SourceV1::Node { position, occurrence, node })] => {
            let u = FlatReadV5 { position: *position, occurrence: *occurrence, node: *node, leaf: None };
            let o = match crate::flat_detect5::read_node(&c.base, pr, public, u) {
                Ok(o) => o,
                Err(f) => return f.into(),
            };
            if flat_profile_commitment_v5(t).ok() != Some(o.commitment) {
                return error("prior TopK source differs: not the first faulty relation");
            }
            Some(o)
        }
        _ => return error("uncommitted source cannot select the point court"),
    };
    for row in 0..rows {
        let mut selected = Vec::new();
        if selected.try_reserve_exact(k as usize).is_err() {
            return error("TopK selected buffer refused");
        }
        for slot in 0..k {
            let element = match at(&sh.out, axis, row, slot) {
                Ok(i) => i,
                Err(e) => return error(e),
            };
            match point(scope_root, public, unit, &output, &sh.out, element) {
                Ok((v, _)) => selected.push(v),
                Err(f) => return f,
            }
        }
        let fault = match detect_topk_line_v7(axis_len, k, &selected, scan, |i| {
            at(&t.shape, axis, row, i).ok().and_then(|i| t.data.get(i as usize).copied())
        }) {
            Ok(Some(f)) => f,
            Ok(None) => continue,
            Err(e) => return error(e),
        };
        let mut slots = BTreeSet::new();
        let mut score_elements = Vec::new();
        match fault {
            TopKFaultKindV7::Range { slot } => {
                slots.insert(slot);
            }
            TopKFaultKindV7::Order { left } => {
                slots.extend([left, left + 1]);
            }
            TopKFaultKindV7::Dominance { selected: slot, rival, gap } => {
                slots.insert(slot);
                if gap > 0 {
                    slots.insert(gap - 1);
                }
                if gap < k {
                    slots.insert(gap);
                }
                for id in [selected[slot as usize], rival] {
                    match at(&t.shape, axis, row, id) {
                        Ok(i) => score_elements.push(i),
                        Err(e) => return error(e),
                    }
                }
            }
        }
        let mut out = FlatOperandV5 { node: Some(output.clone()), ..Default::default() };
        for slot in slots {
            let i = match at(&sh.out, axis, row, slot) {
                Ok(i) => i,
                Err(e) => return error(e),
            };
            let o = match point(scope_root, public, unit, &output, &sh.out, i) {
                Ok((_, o)) => o,
                Err(f) => return f,
            };
            out.flat.extend(o.flat);
            out.legacy.extend(o.legacy);
        }
        out.flat.sort_by_key(|l| l.opening.index);
        out.flat.dedup_by_key(|l| l.opening.index);
        out.legacy.sort_by_key(|l| (l.line, l.tile));
        out.legacy.dedup_by_key(|l| (l.line, l.tile));
        let scores = if score_elements.is_empty() {
            FlatOperandV5::default()
        } else {
            let reads = if source_node.is_some() && t.data.len() >= 64 {
                let tree = match FlatTreeV5::of(t) {
                    Ok(t) => t,
                    Err(e) => return error(e),
                };
                score_elements.iter().filter_map(|i| tree.layout().leaf_of(*i).map(|i| (i, 0))).collect::<BTreeSet<_>>()
            } else {
                let l = LayoutV3::of(&t.shape);
                score_elements.iter().filter_map(|i| l.row_leaf_of(*i).map(|(row, tile, _)| (row, tile))).collect()
            };
            match crate::flat_detect5::value_proof(t, source_node, &reads.into_iter().collect::<Vec<_>>()) {
                Ok(o) => o,
                Err(f) => return f.into(),
            }
        };
        return checked(c, FlatFaultV9::TopK(TopKFaultV7 { version: 7, row, kind: fault, output: out, scores }));
    }
    error("all selected rows agree but their typed commitment differs")
}

fn ordinary_relation(
    c: &FlatContextV9<'_>,
    scope_root: &Digest,
    pr: &PreparedV1<'_>,
    public: &dyn FlatPublicReadsV9,
    unit: FlatReadV5,
    expected: &Tensor,
    own: &dyn Fn() -> Result<Vec<Tensor>, String>,
    token: &dyn Fn() -> Option<crate::seg::PromptTileOpeningV1>,
) -> FlatFindingV9 {
    let output = match crate::flat_detect5::read_node(&c.base, pr, public, unit) {
        Ok(o) => o,
        Err(f) => return f.into(),
    };
    let (p, s, n) = (unit.position, unit.occurrence, unit.node);
    let (dtype, shape) = pr.declared(s, n, p);
    if expected.dtype != dtype || expected.shape != shape {
        return error("own output differs from declared metadata");
    }
    match flat_profile_commitment_v5(expected) {
        Ok(root) if root == output.commitment => return FlatFindingV9::Clean,
        Err(e) => return error(e),
        _ => {}
    }
    if let Err(f) = header(c, scope_root, public, unit, &output) {
        return f;
    }
    let (first, count) = if expected.data.len() >= 64 {
        let tree = match FlatTreeV5::of(expected) {
            Ok(t) => t,
            Err(e) => return error(e),
        };
        let maximum = tree.layout().bounds_for::<5>(dtype).expect("validated tree").localization_replies;
        let mut next = 0;
        let mut found = None;
        for _ in 0..maximum {
            let read = FlatReadV5 { leaf: Some(next), ..unit };
            let Some(hash) = public.hash(read).filter(|h| h.index == next) else {
                return FlatFindingV9::Demand(read);
            };
            match hash.descend(&output.commitment, &tree) {
                Ok(FlatDescentV5::Probe(i)) => next = i,
                Ok(FlatDescentV5::DivergentLeaf(i)) => {
                    found = tree.layout().span(i);
                    break;
                }
                _ => return FlatFindingV9::Demand(read),
            }
        }
        match found {
            Some(v) => (v.0, v.1 as u64),
            None => return error("point9 localization exceeded derived bound"),
        }
    } else {
        (0, expected.data.len() as u64)
    };
    let mut cached = None;
    let mut selected = None;
    for e in first..first + count {
        let value = cached.as_ref().and_then(|o| authenticated_output_point_v9(o, dtype, &shape, e));
        let got = match value {
            Some(v) => v,
            None => match typed_point(scope_root, public, unit, &output, dtype, &shape, e) {
                Ok((v, o)) => {
                    cached = Some(o);
                    v
                }
                Err(f) => return f,
            },
        };
        if expected.data[e as usize] != got {
            selected = Some(e);
            break;
        }
    }
    let Some(element) = selected else {
        return error("divergent authenticated output leaf has no divergent typed point");
    };
    let own = match own() {
        Ok(v) => v,
        Err(e) => return error(e),
    };
    let point_context = PointContextV9 { court_id: ordinary_point_court_id_v9(), base: c.base() };
    let deps = match point_dependencies_v9(&point_context, p, s, n, element, &own) {
        Ok(d) => d,
        Err(e) => return error(e),
    };
    let sources = match operand_sources(pr, p, s, n) {
        Ok(v) => v,
        Err(e) => return error(e),
    };
    let mut inputs = Vec::with_capacity(sources.len());
    for (i, ((src, t), reads)) in sources.iter().zip(&own).zip(&deps.leaves).enumerate() {
        let node = match src {
            OperandSourceV1::Source(SourceV1::Node { position, occurrence, node }) => {
                let unit = FlatReadV5 { position: *position, occurrence: *occurrence, node: *node, leaf: None };
                let o = match crate::flat_detect5::read_node(&c.base, pr, public, unit) {
                    Ok(o) => o,
                    Err(f) => return f.into(),
                };
                if flat_profile_commitment_v5(t).ok() != Some(o.commitment) {
                    return error("own prior operand differs from public first-fault prefix");
                }
                Some(o)
            }
            OperandSourceV1::Source(SourceV1::Param { index, layer }) => {
                if c.base.claim.params.by_instance.get(&(*index, *layer)) != Some(&crate::merkle3::tensor_commitment_v3(t)) {
                    return error("own parameter differs from registered model");
                }
                None
            }
            _ => {
                inputs.push(FlatOperandV5::default());
                continue;
            }
        };
        if deps.kinds[i] == PointOperandKindV9::Column {
            let tree = TreesV3::of(t);
            let mut o = FlatOperandV5 { node, ..Default::default() };
            for &(line, tile) in reads {
                let Some(l) = LeafOpeningV3::of_trees(t, &tree, AXIS_COL, line, tile) else {
                    return error("missing own column leaf");
                };
                o.legacy.push(l);
            }
            inputs.push(o);
        } else {
            match crate::flat_detect5::value_proof(t, node, reads) {
                Ok(o) => inputs.push(o),
                Err(f) => return f.into(),
            }
        }
    }
    checked(c, FlatFaultV9::Ordinary(FlatPointFaultV9 { version: 9, element, output: cached.unwrap(), inputs, token: token() }))
}
/// Locate a discrepancy in one independently replayed ordinary relation. The
/// complete program scope and exact relation coordinate are checked before any
/// public or own-source callback. The provider is queried by generic hash
/// descent and returns only the requested authenticated typed output leaf.
///
/// The caller must establish the first divergent relation from its own replay
/// and reserve execution, source preparation, metadata checking and all public
/// read/retry quotas before calling. This helper does not replay the model or
/// implement a session quota; use `FlatReplaySessionV9` for that contract.
pub fn locate_ordinary_relation_v9(
    c: &FlatContextV9<'_>,
    public: &dyn FlatPublicReadsV9,
    unit: FlatReadV5,
    expected: &Tensor,
    own: &dyn Fn() -> Result<Vec<Tensor>, String>,
    token: &dyn Fn() -> Option<crate::seg::PromptTileOpeningV1>,
) -> FlatFindingV9 {
    let pr = match c.prepare() {
        Ok(pr) => pr,
        Err(e) => return error(e),
    };
    if unit.position >= c.base.claim.positions
        || unit.leaf.is_some()
        || !pr.valid(unit.occurrence, unit.node)
        || matches!(pr.w.node(unit.occurrence, unit.node).prim, Prim::TopK { .. })
    {
        return error("ordinary9 locator requires an in-scope ordinary node coordinate");
    }
    ordinary_relation(c, &c.scope.root(), &pr, public, unit, expected, own, token)
}
/// Internal engine: the public caller must reserve a complete independent replay
/// attempt, typed/header replies and all retries before invoking any callback.
pub(crate) fn check_flat_claim_v9(
    c: &FlatContextV9<'_>,
    public: &dyn FlatPublicReadsV9,
    artifact: &dyn Fn(u16, Option<u16>) -> Option<Tensor>,
    tokens: &[u32],
    scan: &mut TopKScanBudgetV7,
) -> FlatFindingV9 {
    if let Err(e) = c.prepare() {
        return error(e);
    }
    let scope_root = c.scope.root();
    let result = crate::flat_detect5::check_flat_claim_engine(&c.base, public, artifact, tokens, |pr, unit, expected, own, token| {
        let finding = if matches!(pr.w.node(unit.occurrence, unit.node).prim, Prim::TopK { .. }) {
            match crate::flat_scope7::point_relation(pr, unit.position, unit.occurrence, unit.node) {
                Ok(true) => topk_relation(c, &scope_root, pr, public, unit, expected, own, scan),
                Ok(false) => match crate::flat_detect5::read_node(&c.base, pr, public, unit) {
                    Err(f) => f.into(),
                    Ok(o) => {
                        if flat_profile_commitment_v5(expected).ok() != Some(o.commitment) {
                            header(c, &scope_root, public, unit, &o)?
                        }
                        crate::flat_detect5::relation(&c.base, pr, public, unit, expected, own, token).into()
                    }
                },
                Err(e) => error(e),
            }
        } else {
            ordinary_relation(c, &scope_root, pr, public, unit, expected, own, token)
        };
        match finding {
            FlatFindingV9::Clean => Ok(()),
            f => Err(f),
        }
    });
    match result {
        Err(FlatFindingV9::Fault(f)) => checked(c, f),
        Err(f) => f,
        Ok(()) => FlatFindingV9::Clean,
    }
}
