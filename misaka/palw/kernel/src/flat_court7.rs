//! Explicit v7 mixed-court envelope. No existing decoder accepts these bytes.
//! The consumer must bind the independently derived complete graph scope.
use crate::{
    element::{PreparedV1, SegClaimContextV1},
    flat_court4::{self as common, FlatCourtBoundsV4},
    flat_court5::{FlatConvictionV5, FlatFaultV5, FlatSegContextV5},
    flat_scope7::{FlatResourceScopeV7, check_flat_resource_scope_v7},
    flat_topk7::{TopKContextV7, TopKFaultKindV7, TopKFaultV7, topk_court_id_v7},
    verify::DismissalV1,
};
use std::io;

pub struct FlatContextV7<'a> {
    pub scope: &'a FlatResourceScopeV7,
    pub base: FlatSegContextV5<'a>,
}
impl FlatContextV7<'_> {
    pub(crate) fn prepare(&self) -> Result<PreparedV1<'_>, DismissalV1> {
        if self.base.claim.positions > self.scope.max_positions || self.base.profile != self.scope.profile {
            return Err(na("v7 claim differs from its admitted scope"));
        }
        check_flat_resource_scope_v7(self.base.claim.program, self.scope.max_positions, self.base.claim.encoder.as_ref(), self.scope)?;
        common::prepare(&self.base)
    }
    pub(crate) fn topk(&self) -> TopKContextV7<'_> {
        let c = &self.base.claim;
        TopKContextV7 {
            court_id: topk_court_id_v7(),
            base: FlatSegContextV5 {
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
            },
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlatFaultV7 {
    V5(FlatFaultV5),
    TopK(TopKFaultV7),
}
impl FlatFaultV7 {
    pub fn to_bytes(&self) -> io::Result<Vec<u8>> {
        let (tag, body) = match self {
            Self::V5(f) => (0, f.to_bytes()?),
            Self::TopK(f) => (1, f.to_bytes()?),
        };
        if body.len() as u64 + 3 > common::FLAT_FAULT_MAX_BYTES_V4 {
            return Err(invalid());
        }
        let mut out = Vec::with_capacity(body.len() + 3);
        out.extend_from_slice(&7u16.to_le_bytes());
        out.push(tag);
        out.extend_from_slice(&body);
        Ok(out)
    }
    pub fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() < 3 || bytes.len() as u64 > common::FLAT_FAULT_MAX_BYTES_V4 || bytes[..2] != 7u16.to_le_bytes() {
            return Err(invalid());
        }
        match bytes[2] {
            0 => FlatFaultV5::from_bytes(&bytes[3..]).map(Self::V5),
            1 => TopKFaultV7::from_bytes(&bytes[3..]).map(Self::TopK),
            _ => Err(invalid()),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlatConvictionV7 {
    V5(FlatConvictionV5),
    TopK { position: u32, occurrence: u16, node: u16, row: u64, kind: TopKFaultKindV7 },
}
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid bounded mixed flat-v7 fault")
}
fn na(s: &str) -> DismissalV1 {
    DismissalV1::NotAuthentic(s.into())
}
fn bounds(c: &FlatContextV7<'_>, pr: &PreparedV1<'_>, f: &FlatFaultV7) -> Result<FlatCourtBoundsV4, DismissalV1> {
    let b = match f {
        FlatFaultV7::TopK(f) => {
            let o = f.output.node.as_ref().ok_or_else(|| na("missing TopK output"))?;
            let t = crate::flat_topk7::relation_scope_v7(
                c.base.claim.program,
                pr,
                c.base.claim.positions,
                o.position,
                o.occurrence,
                o.node,
                false,
            )?;
            if !c.scope.uses_point(pr, o.position, o.occurrence, o.node)? {
                return Err(na("scope did not select the TopK point court"));
            }
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
        FlatFaultV7::V5(f) => {
            let target = match f {
                FlatFaultV5::Leaf(f) => Some(&f.output),
                FlatFaultV5::Whole(f) => Some(&f.output),
                FlatFaultV5::Malformed(_) | FlatFaultV5::Decode(_) => None,
            };
            if let Some(o) = target {
                if !pr.valid(o.occurrence, o.node) || o.position >= c.base.claim.positions {
                    return Err(na("no v5 relation in v7 scope"));
                }
                if c.scope.uses_point(pr, o.position, o.occurrence, o.node)? {
                    return Err(na("scope requires the TopK point court"));
                }
            }
            let mut b = crate::flat_court5::flat_fault_court_bounds_v5(&c.base, f)?;
            // The v5 dispatcher already includes its own 3-byte wrapper.
            b.filing_bytes -= 3;
            crate::flat_scope7::capped(b, true)?
        }
    };
    if b.filing_bytes > c.scope.worst_court_bytes
        || b.work > c.scope.worst_court_work
        || b.scratch_bytes > c.scope.worst_court_scratch_bytes
    {
        return Err(na("court exceeds the complete admitted v7 scope"));
    }
    Ok(b)
}
pub fn flat_fault_court_bounds_v7(c: &FlatContextV7<'_>, f: &FlatFaultV7) -> Result<FlatCourtBoundsV4, DismissalV1> {
    bounds(c, &c.prepare()?, f)
}
pub fn verify_flat_fault_v7(c: &FlatContextV7<'_>, f: &FlatFaultV7) -> Result<FlatConvictionV7, DismissalV1> {
    let pr = c.prepare()?;
    let b = bounds(c, &pr, f)?;
    let bytes = f.to_bytes().map_err(|_| na("malformed v7 envelope"))?;
    if bytes.len() as u64 > b.filing_bytes {
        return Err(na("mixed fault exceeds its independently derived bound"));
    }
    match f {
        FlatFaultV7::V5(f) => crate::flat_court5::verify_flat_fault_v5(&c.base, f).map(FlatConvictionV7::V5),
        FlatFaultV7::TopK(f) => {
            crate::flat_topk7::verify_topk_fault_v7(&c.topk(), f)?;
            let o = f.output.node.as_ref().unwrap();
            Ok(FlatConvictionV7::TopK {
                position: o.position,
                occurrence: o.occurrence,
                node: o.node,
                row: f.row,
                kind: f.kind.clone(),
            })
        }
    }
}
