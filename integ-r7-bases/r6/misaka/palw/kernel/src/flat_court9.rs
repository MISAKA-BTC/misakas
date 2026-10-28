//! Explicit new mixed envelope: ordinary scalar9, committed TopK7 and decode/header5.
//! Complete graph scope is independently rederived before checking a public filing.
//! Its own metadata work/storage is charged as well as the terminal court; no v7
//! decoder or execution-eligibility state accepts this new namespace implicitly.
use crate::{
    element::{PreparedV1, SegClaimContextV1},
    flat_court4::{self as common, FlatCourtBoundsV4},
    flat_court5::{FlatConvictionV5, FlatFaultV5, FlatSegContextV5},
    flat_point9::{FlatPointFaultV9, PointContextV9, ordinary_point_court_id_v9, relation_priced_v9},
    flat_scope9::{FlatResourceScopeV9, check_flat_resource_scope_v9},
    flat_topk7::{TopKContextV7, TopKFaultKindV7, TopKFaultV7, topk_court_id_v7},
    verify::DismissalV1,
};
use std::io;
pub struct FlatContextV9<'a> {
    pub scope: &'a FlatResourceScopeV9,
    pub base: FlatSegContextV5<'a>,
}
impl FlatContextV9<'_> {
    pub(crate) fn prepare(&self) -> Result<PreparedV1<'_>, DismissalV1> {
        if self.base.claim.positions > self.scope.max_positions || self.base.profile != self.scope.profile {
            return Err(bad("v9 claim differs from admitted scope"));
        }
        check_flat_resource_scope_v9(self.base.claim.program, self.scope.max_positions, self.base.claim.encoder.as_ref(), self.scope)?;
        common::prepare(&self.base)
    }
    pub(crate) fn base(&self) -> FlatSegContextV5<'_> {
        let c = &self.base.claim;
        FlatSegContextV5 {
            profile: self.base.profile,
            claim: SegClaimContextV1 {
                program: c.program,
                params: c.params,
                segment_roots: c.segment_roots,
                positions: c.positions,
                prompt_len: c.prompt_len,
                prompt_root: c.prompt_root,
                inline_prompt: c.inline_prompt,
                generated: c.generated,
                decode: c.decode,
                encoder: c.encoder,
            },
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlatFaultV9 {
    Ordinary(FlatPointFaultV9),
    TopK(TopKFaultV7),
    Legacy(FlatFaultV5),
}
impl FlatFaultV9 {
    pub fn to_bytes(&self) -> io::Result<Vec<u8>> {
        let (tag, body) = match self {
            Self::Ordinary(f) => (0, f.to_bytes()?),
            Self::TopK(f) => (1, f.to_bytes()?),
            Self::Legacy(f) => (2, f.to_bytes()?),
        };
        if body.len() as u64 + 3 > common::FLAT_FAULT_MAX_BYTES_V4 {
            return Err(invalid());
        }
        let mut out = Vec::with_capacity(body.len() + 3);
        out.extend(9u16.to_le_bytes());
        out.push(tag);
        out.extend(body);
        Ok(out)
    }
    pub fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() < 3 || bytes.len() as u64 > common::FLAT_FAULT_MAX_BYTES_V4 || bytes[..2] != 9u16.to_le_bytes() {
            return Err(invalid());
        }
        let f = match bytes[2] {
            0 => Self::Ordinary(FlatPointFaultV9::from_bytes(&bytes[3..])?),
            1 => Self::TopK(TopKFaultV7::from_bytes(&bytes[3..])?),
            2 => Self::Legacy(FlatFaultV5::from_bytes(&bytes[3..])?),
            _ => return Err(invalid()),
        };
        Ok(f)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlatConvictionV9 {
    Ordinary { position: u32, occurrence: u16, node: u16, element: u64 },
    TopK { position: u32, occurrence: u16, node: u16, row: u64, kind: TopKFaultKindV7 },
    Legacy(FlatConvictionV5),
}
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid bounded flat-v9 mixed fault")
}
fn bad(s: &str) -> DismissalV1 {
    DismissalV1::NotAuthentic(s.into())
}
/// Whole API envelope including complete-scope checker. Persisted class metadata
/// and input wire residency are conservatively added to the terminal scratch.
fn add_preparation(c: &FlatContextV9<'_>, b: FlatCourtBoundsV4) -> Result<FlatCourtBoundsV4, DismissalV1> {
    add_scope_preparation(c.scope, b)
}
fn add_scope_preparation(scope: &FlatResourceScopeV9, mut b: FlatCourtBoundsV4) -> Result<FlatCourtBoundsV4, DismissalV1> {
    let metadata_ram = (8u64 << 20)
        .checked_add((scope.relations.len() as u64).checked_mul(512).ok_or_else(|| bad("v9 metadata RAM overflow"))?)
        .ok_or_else(|| bad("v9 metadata RAM overflow"))?;
    let metadata_work = u64::try_from(scope.metadata_derivation_work).map_err(|_| bad("v9 metadata work overflow"))?;
    b.work = b.work.checked_add(metadata_work).ok_or_else(|| bad("v9 complete API work overflow"))?;
    b.scratch_bytes = b.scratch_bytes.checked_add(metadata_ram).ok_or_else(|| bad("v9 complete API RAM overflow"))?;
    if b.filing_bytes > common::FLAT_FAULT_MAX_BYTES_V4
        || b.work > common::FLAT_FAULT_MAX_WORK_V4
        || b.scratch_bytes > common::FLAT_FAULT_MAX_SCRATCH_V4
        || b.operand_leaves.iter().sum::<u64>() > common::FLAT_FAULT_MAX_LEAVES_V4 as u64
    {
        return Err(bad("v9 complete court API exceeds frozen court resource caps"));
    }
    Ok(b)
}
fn bounds(c: &FlatContextV9<'_>, pr: &PreparedV1<'_>, f: &FlatFaultV9) -> Result<FlatCourtBoundsV4, DismissalV1> {
    let b = match f {
        FlatFaultV9::Ordinary(f) => {
            let o = f.output.node.as_ref().ok_or_else(|| bad("missing ordinary9 output"))?;
            let p = relation_priced_v9(
                c.base.claim.program,
                c.base.claim.positions,
                pr,
                o.position,
                o.occurrence,
                o.node,
                common::size_with_limit(c.base.claim.program, misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64)
                    .map_err(|_| bad("oversized v9 program"))?,
            )?;
            crate::flat_scope7::capped(
                FlatCourtBoundsV4 {
                    filing_bytes: p.filing_bytes,
                    work: p.work,
                    scratch_bytes: p.scratch_bytes,
                    operand_leaves: p.operand_leaves,
                },
                false,
            )?
        }
        FlatFaultV9::TopK(f) => {
            let o = f.output.node.as_ref().ok_or_else(|| bad("missing TopK9 output"))?;
            let t = crate::flat_topk7::relation_scope_v7(
                c.base.claim.program,
                pr,
                c.base.claim.positions,
                o.position,
                o.occurrence,
                o.node,
                false,
            )?;
            crate::flat_scope7::capped(
                FlatCourtBoundsV4 {
                    filing_bytes: t.filing_bytes,
                    work: t.court_work,
                    scratch_bytes: t.court_scratch_bytes,
                    operand_leaves: vec![3, 2],
                },
                false,
            )?
        }
        FlatFaultV9::Legacy(f) => {
            let output = match f {
                FlatFaultV5::Leaf(f) => Some(&f.output),
                FlatFaultV5::Whole(f) => Some(&f.output),
                _ => None,
            };
            if let Some(o) = output
                && (o.position >= c.base.claim.positions
                    || !pr.valid(o.occurrence, o.node)
                    || !matches!(pr.w.node(o.occurrence, o.node).prim, misaka_palw_tir::Prim::TopK { .. })
                    || crate::flat_scope7::point_relation(pr, o.position, o.occurrence, o.node)?)
            {
                return Err(bad("legacy9 relation must be TopK over a public/initial source"));
            }
            let mut b = crate::flat_court5::flat_fault_court_bounds_v5(&c.base, f)?;
            b.filing_bytes -= 3;
            crate::flat_scope7::capped(b, true)?
        }
    };
    if b.filing_bytes > c.scope.worst_court_bytes
        || b.work > c.scope.worst_court_work
        || b.scratch_bytes > c.scope.worst_court_scratch_bytes
    {
        return Err(bad("terminal court exceeds independently derived complete scope9"));
    }
    add_preparation(c, b)
}
pub fn flat_fault_court_bounds_v9(c: &FlatContextV9<'_>, f: &FlatFaultV9) -> Result<FlatCourtBoundsV4, DismissalV1> {
    bounds(c, &c.prepare()?, f)
}
pub fn verify_flat_fault_v9(c: &FlatContextV9<'_>, f: &FlatFaultV9) -> Result<FlatConvictionV9, DismissalV1> {
    let pr = c.prepare()?;
    let b = bounds(c, &pr, f)?;
    let wire = f.to_bytes().map_err(|_| bad("malformed v9 mixed envelope"))?;
    if wire.len() as u64 > b.filing_bytes {
        return Err(bad("v9 mixed filing exceeds derived bound"));
    }
    match f {
        FlatFaultV9::Ordinary(f) => {
            crate::flat_point9::verify_flat_point_fault_v9(
                &PointContextV9 { court_id: ordinary_point_court_id_v9(), base: c.base() },
                f,
            )?;
            let o = f.output.node.as_ref().unwrap();
            Ok(FlatConvictionV9::Ordinary { position: o.position, occurrence: o.occurrence, node: o.node, element: f.element })
        }
        FlatFaultV9::TopK(f) => {
            crate::flat_topk7::verify_topk_fault_v7(&TopKContextV7 { court_id: topk_court_id_v7(), base: c.base() }, f)?;
            let o = f.output.node.as_ref().unwrap();
            Ok(FlatConvictionV9::TopK {
                position: o.position,
                occurrence: o.occurrence,
                node: o.node,
                row: f.row,
                kind: f.kind.clone(),
            })
        }
        FlatFaultV9::Legacy(f) => crate::flat_court5::verify_flat_fault_v5(&c.base, f).map(FlatConvictionV9::Legacy),
    }
}
/// Full scope9 is still distinct from a complete replay/DA/activation/economic plan.
pub fn complete_court_api_bounds_v9(c: &FlatContextV9<'_>) -> Result<FlatCourtBoundsV4, DismissalV1> {
    c.prepare()?;
    complete_scope_api_bounds_v9(c.scope)
}
/// Pure quantity helper over a scope that the consumer has independently
/// derived/checked against its canonical program and encoder binding. This
/// helper does not authenticate a caller-supplied scope or grant eligibility.
pub fn complete_scope_api_bounds_v9(scope: &FlatResourceScopeV9) -> Result<FlatCourtBoundsV4, DismissalV1> {
    add_scope_preparation(
        scope,
        FlatCourtBoundsV4 {
            filing_bytes: scope.worst_court_bytes,
            work: scope.worst_court_work,
            scratch_bytes: scope.worst_court_scratch_bytes,
            operand_leaves: vec![common::FLAT_FAULT_MAX_LEAVES_V4 as u64],
        },
    )
}
