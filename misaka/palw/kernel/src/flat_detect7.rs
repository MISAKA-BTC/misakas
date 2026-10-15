//! Complete mixed-court replay using independently acquired parameters and job
//! ids. A wrong TopK needs authenticated selected-value replies; withholding or
//! malformed replies produce demands, never correctness or a conviction.
use crate::{
    element::{OperandSourceV1, PreparedV1, operand_sources},
    flat_court5::{FlatOperandV5, flat_profile_commitment_v5},
    flat_court7::{FlatContextV7, FlatFaultV7, verify_flat_fault_v7},
    flat_detect5::{FlatFindingV5, FlatPublicReadsV5, FlatReadV5},
    flat_topk7::{TopKFaultKindV7, TopKFaultV7, TopKScanBudgetV7, at, detect_topk_line_v7, output_point},
    merkle3::LayoutV3,
    merkle5::FlatTreeV5,
    trace::SourceV1,
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::{Prim, Tensor};
use std::{
    collections::BTreeSet,
    io::{self, Cursor},
};

pub const TOPK_VALUE_MAX_BYTES_V7: u64 = 16 << 10;
/// One authenticated output leaf; raw reply bytes are bounded before decoding.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatValueV7 {
    pub version: u16,
    pub value: FlatOperandV5,
}
impl FlatValueV7 {
    fn structure(&self) -> io::Result<()> {
        if self.version != 7 || self.value.node.is_none() || self.value.flat.len() + self.value.legacy.len() != 1 {
            return Err(invalid());
        }
        crate::flat_court4::parts_structure(None, std::slice::from_ref(&self.value), &None)
    }
    pub fn to_bytes(&self) -> io::Result<Vec<u8>> {
        self.structure()?;
        crate::flat_court4::size_with_limit(self, TOPK_VALUE_MAX_BYTES_V7)?;
        borsh::to_vec(self)
    }
    pub fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() as u64 > TOPK_VALUE_MAX_BYTES_V7 {
            return Err(invalid());
        }
        let mut r = Cursor::new(bytes);
        let version = u16::deserialize_reader(&mut r)?;
        if version != 7 {
            return Err(invalid());
        }
        let out = Self { version, value: crate::flat_topk7::operand(&mut r, 1)? };
        if r.position() != bytes.len() as u64 {
            return Err(invalid());
        }
        out.structure()?;
        Ok(out)
    }
}
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid bounded flat-v7 value reply")
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlatValueReadV7 {
    pub node: FlatReadV5,
    pub element: u64,
}
pub trait FlatPublicReadsV7: FlatPublicReadsV5 {
    fn value(&self, read: FlatValueReadV7) -> Option<Vec<u8>>;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlatFindingV7 {
    Clean,
    Fault(FlatFaultV7),
    Demand(FlatReadV5),
    ValueDemand(FlatValueReadV7),
    Inconsistent(String),
}
impl From<FlatFindingV5> for FlatFindingV7 {
    fn from(f: FlatFindingV5) -> Self {
        match f {
            FlatFindingV5::Clean => Self::Clean,
            FlatFindingV5::Fault(f) => Self::Fault(FlatFaultV7::V5(f)),
            FlatFindingV5::Demand(r) => Self::Demand(r),
            FlatFindingV5::Inconsistent(e) => Self::Inconsistent(e),
        }
    }
}
fn error(e: impl std::fmt::Debug) -> FlatFindingV7 {
    FlatFindingV7::Inconsistent(format!("{e:?}"))
}
fn checked(c: &FlatContextV7<'_>, f: FlatFaultV7) -> FlatFindingV7 {
    match f.to_bytes().and_then(|bytes| FlatFaultV7::from_bytes(&bytes)) {
        Err(e) => error(e),
        Ok(f) => match verify_flat_fault_v7(c, &f) {
            Ok(_) => FlatFindingV7::Fault(f),
            Err(e) => error(e),
        },
    }
}
#[allow(clippy::result_large_err)] // the finding IS the evidence, returned once per check; boxing it would change every caller
fn point(
    public: &dyn FlatPublicReadsV7,
    unit: FlatReadV5,
    output: &crate::seg::NodeOpeningV1,
    shape: &[usize],
    element: u64,
) -> Result<(u32, FlatOperandV5), FlatFindingV7> {
    let read = FlatValueReadV7 { node: unit, element };
    let reply = public.value(read).and_then(|b| FlatValueV7::from_bytes(&b).ok());
    let Some(reply) = reply.filter(|r| r.value.node.as_ref() == Some(output)) else { return Err(FlatFindingV7::ValueDemand(read)) };
    let value = output_point(&reply.value, shape, element).ok().and_then(|v| u32::try_from(v).ok());
    value.map(|v| (v, reply.value)).ok_or(FlatFindingV7::ValueDemand(read))
}
pub(crate) fn topk_relation_rows(
    c: &FlatContextV7<'_>,
    pr: &PreparedV1<'_>,
    public: &dyn FlatPublicReadsV7,
    unit: FlatReadV5,
    expected: &Tensor,
    own: &dyn Fn() -> Result<Vec<Tensor>, String>,
    scan: &mut TopKScanBudgetV7,
    selected_row: Option<u64>,
    opened_output: Option<crate::seg::NodeOpeningV1>,
) -> FlatFindingV7 {
    let output = match opened_output {
        Some(o) if crate::flat_court4::authenticate_node(&c.base, pr, &o, (unit.position, unit.occurrence, unit.node)) => o,
        Some(_) => return FlatFindingV7::Demand(unit),
        None => match crate::flat_detect5::read_node(&c.base, pr, public, unit) {
            Ok(o) => o,
            Err(f) => return f.into(),
        },
    };
    let (p, s, n) = (unit.position, unit.occurrence, unit.node);
    let sh = pr.shapes(p, s, n);
    if expected.dtype != misaka_palw_tir::DType::Idx || expected.shape != sh.out {
        return error("reference TopK metadata differs from the program");
    }
    match flat_profile_commitment_v5(expected) {
        Ok(root) if root == output.commitment => return FlatFindingV7::Clean,
        Err(e) => return error(e),
        _ => {}
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
    if selected_row.is_some_and(|r| r >= rows) {
        return error("selected TopK row outside output");
    }
    let row_range = selected_row.map_or(0..rows, |r| r..r + 1);
    for row in row_range {
        let mut selected = Vec::new();
        if selected.try_reserve_exact(k as usize).is_err() {
            return error("TopK selected buffer refused");
        }
        for slot in 0..k {
            let element = match at(&sh.out, axis, row, slot) {
                Ok(i) => i,
                Err(e) => return error(e),
            };
            match point(public, unit, &output, &sh.out, element) {
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
            let o = match point(public, unit, &output, &sh.out, i) {
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
        return checked(c, FlatFaultV7::TopK(TopKFaultV7 { version: 7, row, kind: fault, output: out, scores }));
    }
    error("all selected rows agree but their typed commitment differs")
}
/// Internal engine; public callers use the precharged v7 replay session.
pub(crate) fn check_flat_claim_v7(
    c: &FlatContextV7<'_>,
    public: &dyn FlatPublicReadsV7,
    artifact: &dyn Fn(u16, Option<u16>) -> Option<Tensor>,
    tokens: &[u32],
    scan: &mut TopKScanBudgetV7,
) -> FlatFindingV7 {
    let result = crate::flat_detect5::check_flat_claim_engine(&c.base, public, artifact, tokens, |pr, unit, expected, own, token| {
        let finding = match c.scope.uses_point(pr, unit.position, unit.occurrence, unit.node) {
            Ok(true) => topk_relation_rows(c, pr, public, unit, expected, own, scan, None, None),
            Ok(false) => crate::flat_detect5::relation(&c.base, pr, public, unit, expected, own, token).into(),
            Err(e) => error(e),
        };
        match finding {
            FlatFindingV7::Clean => Ok(()),
            f => Err(f),
        }
    });
    match result {
        Err(FlatFindingV7::Fault(f)) => checked(c, f),
        Err(f) => f,
        Ok(()) => FlatFindingV7::Clean,
    }
}
