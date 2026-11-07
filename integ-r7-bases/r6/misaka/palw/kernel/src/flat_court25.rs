//! Explicit new mixed envelope: ordinary scalar9, committed TopK7 and decode/header5.
//! Complete graph scope is independently rederived before checking a public filing.
//! Its own metadata work/storage is charged as well as the terminal court; no v7
//! decoder or execution-eligibility state accepts this new namespace implicitly.
use crate::{
    element::{PreparedV1, SegClaimContextV1},
    flat_court4::{self as common, FlatCourtBoundsV4},
    flat_court5::{FlatConvictionV5, FlatFaultV5, FlatSegContextV5},
    flat_point25::{FlatPointFaultV25, PointContextV25, ordinary_point_court_id_v25, relation_priced_v25},
    flat_scope25::{FlatResourceScopeV25, check_flat_resource_scope_v25},
    flat_topk7::{TopKContextV7, TopKFaultKindV7, TopKFaultV7, topk_court_id_v7},
    verify::DismissalV1,
};
use std::io;
pub struct FlatContextV25<'a> {
    pub scope: &'a FlatResourceScopeV25,
    pub base: FlatSegContextV5<'a>,
}
impl FlatContextV25<'_> {
    pub(crate) fn prepare(&self) -> Result<PreparedV1<'_>, DismissalV1> {
        if self.base.claim.positions > self.scope.max_positions || self.base.profile != self.scope.profile {
            return Err(bad("v25 claim differs from admitted scope"));
        }
        check_flat_resource_scope_v25(
            self.base.claim.program,
            self.scope.max_positions,
            self.base.claim.encoder.as_ref(),
            self.scope,
        )?;
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
pub enum FlatFaultV25 {
    Ordinary(FlatPointFaultV25),
    TopK(TopKFaultV7),
    Legacy(FlatFaultV5),
}
impl FlatFaultV25 {
    pub fn to_bytes(&self) -> io::Result<Vec<u8>> {
        let (tag, body) = match self {
            Self::Ordinary(f) => (0, f.to_bytes()?),
            Self::TopK(f) => (1, f.to_bytes()?),
            Self::Legacy(f) => (2, f.to_bytes()?),
        };
        if body.len() as u64 + 7 > crate::flat_point25::POINT_FAULT_MAX_BYTES_V25 {
            return Err(invalid());
        }
        let mut out = Vec::with_capacity(body.len() + 7);
        out.extend(25u16.to_le_bytes());
        out.push(tag);
        out.extend((body.len() as u32).to_le_bytes());
        out.extend(body);
        Ok(out)
    }
    pub fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() < 7 || bytes.len() as u64 > crate::flat_point25::POINT_FAULT_MAX_BYTES_V25 || bytes[..2] != 25u16.to_le_bytes()
        {
            return Err(invalid());
        }
        let count = u32::from_le_bytes(bytes[3..7].try_into().map_err(|_| invalid())?) as usize;
        if count != bytes.len() - 7 {
            return Err(invalid());
        }
        match bytes[2] {
            0 => FlatPointFaultV25::from_bytes(&bytes[7..]).map(Self::Ordinary),
            1 => TopKFaultV7::from_bytes(&bytes[7..]).map(Self::TopK),
            2 => FlatFaultV5::from_bytes(&bytes[7..]).map(Self::Legacy),
            _ => Err(invalid()),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlatConvictionV25 {
    Ordinary { position: u32, occurrence: u16, node: u16, element: u64 },
    TopK { position: u32, occurrence: u16, node: u16, row: u64, kind: TopKFaultKindV7 },
    Legacy(FlatConvictionV5),
}
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid bounded flat-v25 mixed fault")
}
fn bad(s: &str) -> DismissalV1 {
    DismissalV1::NotAuthentic(s.into())
}
/// Whole API envelope including complete-scope checker. Persisted class metadata
/// and input wire residency are conservatively added to the terminal scratch.
fn add_preparation(c: &FlatContextV25<'_>, b: FlatCourtBoundsV4) -> Result<FlatCourtBoundsV4, DismissalV1> {
    add_scope_preparation(c.scope, b)
}
fn add_scope_preparation(scope: &FlatResourceScopeV25, mut b: FlatCourtBoundsV4) -> Result<FlatCourtBoundsV4, DismissalV1> {
    let metadata_ram = (8u64 << 20)
        .checked_add((scope.relations.len() as u64).checked_mul(512).ok_or_else(|| bad("v25 metadata RAM overflow"))?)
        .ok_or_else(|| bad("v25 metadata RAM overflow"))?;
    let metadata_work = u64::try_from(scope.metadata_derivation_work).map_err(|_| bad("v25 metadata work overflow"))?;
    b.work = b.work.checked_add(metadata_work).ok_or_else(|| bad("v25 complete API work overflow"))?;
    b.scratch_bytes = b.scratch_bytes.checked_add(metadata_ram).ok_or_else(|| bad("v25 complete API RAM overflow"))?;
    if b.filing_bytes > crate::flat_point25::POINT_FAULT_MAX_BYTES_V25
        || b.work > crate::flat_point25::POINT_FAULT_MAX_WORK_V25
        || b.scratch_bytes > crate::flat_point25::POINT_FAULT_MAX_SCRATCH_V25
        || b.operand_leaves.iter().sum::<u64>() > crate::flat_point25::POINT_FAULT_MAX_LEAVES_V25 as u64
    {
        return Err(bad("v25 complete court API exceeds explicit v25 resource caps"));
    }
    Ok(b)
}
fn bounds(c: &FlatContextV25<'_>, pr: &PreparedV1<'_>, f: &FlatFaultV25) -> Result<FlatCourtBoundsV4, DismissalV1> {
    let b = match f {
        FlatFaultV25::Ordinary(f) => {
            let o = f.output.node.as_ref().ok_or_else(|| bad("missing ordinary25 output"))?;
            let p = relation_priced_v25(
                c.base.claim.program,
                c.base.claim.positions,
                pr,
                o.position,
                o.occurrence,
                o.node,
                common::size_with_limit(c.base.claim.program, misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64)
                    .map_err(|_| bad("oversized v25 program"))?,
            )?;
            crate::flat_scope25::capped_v25(FlatCourtBoundsV4 {
                filing_bytes: p.filing_bytes,
                work: p.work,
                scratch_bytes: p.scratch_bytes,
                operand_leaves: p.operand_leaves,
            })?
        }
        FlatFaultV25::TopK(f) => {
            let o = f.output.node.as_ref().ok_or_else(|| bad("missing TopK25 output"))?;
            let t = crate::flat_topk7::relation_scope_v7(
                c.base.claim.program,
                pr,
                c.base.claim.positions,
                o.position,
                o.occurrence,
                o.node,
                false,
            )?;
            crate::flat_scope25::extend_special_envelope_v25(crate::flat_scope7::capped(
                FlatCourtBoundsV4 {
                    filing_bytes: t.filing_bytes,
                    work: t.court_work,
                    scratch_bytes: t.court_scratch_bytes,
                    operand_leaves: vec![3, 2],
                },
                false,
            )?)?
        }
        FlatFaultV25::Legacy(f) => {
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
                return Err(bad("legacy25 relation must be TopK over a public/initial source"));
            }
            let mut b = crate::flat_court5::flat_fault_court_bounds_v5(&c.base, f)?;
            b.filing_bytes -= 3;
            crate::flat_scope25::extend_special_envelope_v25(crate::flat_scope7::capped(b, true)?)?
        }
    };
    if b.filing_bytes > c.scope.worst_court_bytes
        || b.work > c.scope.worst_court_work
        || b.scratch_bytes > c.scope.worst_court_scratch_bytes
    {
        return Err(bad("terminal court exceeds independently derived complete scope25"));
    }
    add_preparation(c, b)
}
pub fn flat_fault_court_bounds_v25(c: &FlatContextV25<'_>, f: &FlatFaultV25) -> Result<FlatCourtBoundsV4, DismissalV1> {
    bounds(c, &c.prepare()?, f)
}
pub fn verify_flat_fault_v25(c: &FlatContextV25<'_>, f: &FlatFaultV25) -> Result<FlatConvictionV25, DismissalV1> {
    let pr = c.prepare()?;
    let b = bounds(c, &pr, f)?;
    let wire = f.to_bytes().map_err(|_| bad("malformed v25 mixed envelope"))?;
    if wire.len() as u64 > b.filing_bytes {
        return Err(bad("v25 mixed filing exceeds derived bound"));
    }
    match f {
        FlatFaultV25::Ordinary(f) => {
            crate::flat_point25::verify_flat_point_fault_v25(
                &PointContextV25 { court_id: ordinary_point_court_id_v25(), base: c.base() },
                f,
            )?;
            let o = f.output.node.as_ref().unwrap();
            Ok(FlatConvictionV25::Ordinary { position: o.position, occurrence: o.occurrence, node: o.node, element: f.element })
        }
        FlatFaultV25::TopK(f) => {
            crate::flat_topk7::verify_topk_fault_v7(&TopKContextV7 { court_id: topk_court_id_v7(), base: c.base() }, f)?;
            let o = f.output.node.as_ref().unwrap();
            Ok(FlatConvictionV25::TopK {
                position: o.position,
                occurrence: o.occurrence,
                node: o.node,
                row: f.row,
                kind: f.kind.clone(),
            })
        }
        FlatFaultV25::Legacy(f) => crate::flat_court5::verify_flat_fault_v5(&c.base, f).map(FlatConvictionV25::Legacy),
    }
}
/// Full scope25 is still distinct from a complete replay/DA/activation/economic plan.
pub fn complete_court_api_bounds_v25(c: &FlatContextV25<'_>) -> Result<FlatCourtBoundsV4, DismissalV1> {
    c.prepare()?;
    complete_scope_api_bounds_v25(c.scope)
}
/// Pure quantity helper over a scope that the consumer has independently
/// derived/checked against its canonical program and encoder binding. This
/// helper does not authenticate a caller-supplied scope or grant eligibility.
pub fn complete_scope_api_bounds_v25(scope: &FlatResourceScopeV25) -> Result<FlatCourtBoundsV4, DismissalV1> {
    add_scope_preparation(
        scope,
        FlatCourtBoundsV4 {
            filing_bytes: scope.worst_court_bytes,
            work: scope.worst_court_work,
            scratch_bytes: scope.worst_court_scratch_bytes,
            operand_leaves: vec![crate::flat_point25::POINT_FAULT_MAX_LEAVES_V25 as u64],
        },
    )
}

/// Typed checked verdict for native Final27 consumers. The mixed wire cannot
/// borrow a legacy court root; complete scope is independently derived first.
pub struct CheckedFlatConvictionV25 {
    verdict: FlatConvictionV25,
    scope_root: crate::hash::Digest,
}
impl CheckedFlatConvictionV25 {
    pub fn verdict(&self) -> &FlatConvictionV25 {
        &self.verdict
    }
    pub fn scope_root(&self) -> crate::hash::Digest {
        self.scope_root
    }
}
pub fn check_flat_fault_v25(c: &FlatContextV25<'_>, raw: &[u8]) -> Result<CheckedFlatConvictionV25, DismissalV1> {
    c.prepare()?;
    if raw.len() as u64 > c.scope.worst_court_bytes {
        return Err(bad("mixed25 raw filing exceeds original complete scope"));
    }
    let f = FlatFaultV25::from_bytes(raw).map_err(|_| bad("malformed bounded mixed25 filing"))?;
    let verdict = verify_flat_fault_v25(c, &f)?;
    Ok(CheckedFlatConvictionV25 { verdict, scope_root: c.scope.root() })
}
