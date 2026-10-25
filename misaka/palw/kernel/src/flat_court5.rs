//! Staged complete *court kinds* for a new all-dtype internal flat-v5 profile.
//! Every relation still has to fit admission bounds; this is not full model,
//! node/DA/Final, sampled-security or RFC02 completion. No shipping path selects it.
use crate::element::element_value_v1;
use crate::flat_court4::{self as common, FlatCourtBoundsV4};
use crate::hash::Digest;
use crate::merkle3::LayoutV3;
use crate::merkle5::{FlatLeafHashV5, FlatLeafValueV5};
use crate::seg::{NodeOpeningV1, PromptTileOpeningV1};
use crate::verify::DismissalV1;
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::{DType, Tensor};
use std::io::{self, Cursor};
pub type FlatSegContextV5<'a> = common::FlatSegContext<'a, 5>;
pub type FlatOperandV5 = common::FlatOperand<5>;
pub type FlatLeafFaultV5 = common::FlatLeafFault<5>;
pub use common::{
    FlatCourtBoundsV4 as FlatCourtBoundsV5, FlatDependencyUnionV4 as FlatDependencyUnionV5,
    FlatLeafConvictionV4 as FlatLeafConvictionV5,
};
pub const FLAT_FAULT_VERSION_V5: u16 = 5;
pub fn flat_trace_profile_id_v5() -> Digest {
    common::profile_id::<5>()
}
pub fn flat_profile_commitment_v5(t: &Tensor) -> io::Result<Digest> {
    common::profile_commitment::<5>(t)
}
pub fn flat_leaf_court_bounds_v5(
    c: &FlatSegContextV5<'_>,
    p: u32,
    s: u16,
    n: u16,
    index: u64,
) -> Result<FlatCourtBoundsV5, DismissalV1> {
    common::leaf_bounds::<5>(c, p, s, n, index)
}
pub fn verify_flat_leaf_fault_v5(c: &FlatSegContextV5<'_>, f: &FlatLeafFaultV5) -> Result<FlatLeafConvictionV5, DismissalV1> {
    common::verify_leaf::<5>(c, f)
}
pub fn flat_leaf_dependencies_v5(
    c: &FlatSegContextV5<'_>,
    p: u32,
    s: u16,
    n: u16,
    index: u64,
    own: &[Tensor],
) -> Result<FlatDependencyUnionV5, DismissalV1> {
    common::leaf_dependencies::<5>(c, p, s, n, index, own)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlatConvictionKindV5 {
    Leaf { leaf: u64 },
    Whole,
    Malformed,
    Decode { index: u32 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlatConvictionV5 {
    pub position: u32,
    pub occurrence: u16,
    pub node: u16,
    pub kind: FlatConvictionKindV5,
}
fn na(s: &str) -> DismissalV1 {
    DismissalV1::NotAuthentic(s.into())
}
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid bounded flat-v5 fault")
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatWholeFaultV5 {
    pub output: NodeOpeningV1,
    pub inputs: Vec<FlatOperandV5>,
    pub token: Option<PromptTileOpeningV1>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub enum FlatHeaderV5 {
    /// Header of a v3 typed commitment, also proving a wrong grammar on a large node.
    Legacy { dtype: u8, shape: Vec<u64>, row_root: Digest, col_root: Digest },
    /// A flat-v5 typed header; no leaf digest or output element is exposed.
    Flat { dtype: u8, shape: Vec<u64>, root: Digest },
}
impl FlatHeaderV5 {
    fn metadata(&self) -> (u8, &[u64]) {
        match self {
            Self::Legacy { dtype, shape, .. } | Self::Flat { dtype, shape, .. } => (*dtype, shape),
        }
    }
    fn commitment(&self) -> Option<Digest> {
        let (tag, shape) = self.metadata();
        let d = DType::ALL.into_iter().find(|d| d.tag() == tag)?;
        if shape.len() > 4 {
            return None;
        }
        match self {
            Self::Flat { root, .. } => crate::merkle4::header_commitment::<5>(tag, shape, root),
            Self::Legacy { row_root, col_root, .. } => {
                let mut h = crate::hash::keyed(crate::merkle3::TENSOR_COMMITMENT_DOMAIN_V3);
                h.update(&[d.tag(), shape.len() as u8]);
                for dim in shape {
                    h.update(&dim.to_le_bytes());
                }
                h.update(row_root).update(col_root);
                Some(crate::hash::finish(h))
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatMalformedFaultV5 {
    pub output: NodeOpeningV1,
    pub header: FlatHeaderV5,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatDecodeFaultV5 {
    pub index: u32,
    pub rival: u32,
    pub logits: FlatOperandV5,
}
/// This inner grammar is separate from every frozen SegFault/route discriminant.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum FlatFaultV5 {
    Leaf(FlatLeafFaultV5) = 0,
    Whole(FlatWholeFaultV5) = 1,
    Malformed(FlatMalformedFaultV5) = 2,
    Decode(FlatDecodeFaultV5) = 3,
}

pub fn flat_whole_court_bounds_v5(c: &FlatSegContextV5<'_>, p: u32, s: u16, n: u16) -> Result<FlatCourtBoundsV5, DismissalV1> {
    let pr = common::prepare(c)?;
    small_bounds(c, &pr, p, s, n)
}
fn small_bounds(
    c: &FlatSegContextV5<'_>,
    pr: &crate::element::PreparedV1<'_>,
    p: u32,
    s: u16,
    n: u16,
) -> Result<FlatCourtBoundsV5, DismissalV1> {
    if p >= c.claim.positions || !pr.valid(s, n) {
        return Err(na("no such small relation"));
    }
    let (_, shape) = pr.declared(s, n, p);
    let count = LayoutV3::try_of(&shape).ok_or_else(|| na("invalid shape"))?.len;
    if count == 0 || count >= 64 {
        return Err(na("whole court only prices sub-64 values"));
    }
    common::relation_bounds(c, pr, p, s, n, count, 0)
}
pub fn flat_whole_dependencies_v5(
    c: &FlatSegContextV5<'_>,
    p: u32,
    s: u16,
    n: u16,
    own: &[Tensor],
) -> Result<FlatDependencyUnionV5, DismissalV1> {
    let pr = common::prepare(c)?;
    let b = small_bounds(c, &pr, p, s, n)?;
    let (_, shape) = pr.declared(s, n, p);
    let count = LayoutV3::of(&shape).len as usize;
    common::record_span(c, &pr, p, s, n, 0, count, own, b)
}
fn whole(
    c: &FlatSegContextV5<'_>,
    pr: &crate::element::PreparedV1<'_>,
    f: &FlatWholeFaultV5,
) -> Result<FlatConvictionV5, DismissalV1> {
    let (p, s, n) = (f.output.position, f.output.occurrence, f.output.node);
    let b = small_bounds(c, pr, p, s, n)?;
    if !common::authenticate_node(c, pr, &f.output, (p, s, n)) {
        return Err(na("small output not bound"));
    }
    if common::size_with_limit(f, common::FLAT_FAULT_MAX_BYTES_V4).map_err(|_| na("oversized small fault"))? > b.filing_bytes {
        return Err(na("small fault exceeds relation bound"));
    }
    let ops = common::authenticate_inputs(c, pr, p, s, n, &f.inputs, &f.token, &b.operand_leaves)?;
    let node = pr.w.node(s, n);
    let sh = pr.shapes(p, s, n);
    let count = LayoutV3::of(&sh.out).len;
    let convict = || Ok(FlatConvictionV5 { position: p, occurrence: s, node: n, kind: FlatConvictionKindV5::Whole });
    let mut values = Vec::with_capacity(count as usize);
    for e in 0..count {
        match element_value_v1(c.claim.program, node, &sh, e, &ops) {
            Err(_) => return Err(na("missing small relation dependency")),
            Ok(None) => return convict(),
            Ok(Some(v)) => values.push(v),
        }
    }
    let expected = match Tensor::new(node.out.dtype, sh.out, values) {
        Ok(t) => t,
        Err(_) => return convict(),
    };
    if flat_profile_commitment_v5(&expected).map_err(|_| na("invalid reference commitment"))? == f.output.commitment {
        Err(DismissalV1::NoFault)
    } else {
        convict()
    }
}
fn malformed(
    c: &FlatSegContextV5<'_>,
    pr: &crate::element::PreparedV1<'_>,
    f: &FlatMalformedFaultV5,
) -> Result<FlatConvictionV5, DismissalV1> {
    let o = &f.output;
    let (p, s, n) = (o.position, o.occurrence, o.node);
    if !common::authenticate_node(c, pr, o, (p, s, n)) || f.header.commitment() != Some(o.commitment) {
        return Err(na("typed header not bound"));
    }
    let (dtype, shape) = pr.declared(s, n, p);
    let (tag, dims) = f.header.metadata();
    let grammar_ok = matches!(f.header, FlatHeaderV5::Flat { .. }) == common::uses_flat::<5>(dtype, &shape);
    if grammar_ok && tag == dtype.tag() && dims.iter().copied().eq(shape.iter().map(|n| *n as u64)) {
        return Err(DismissalV1::NoFault);
    }
    Ok(FlatConvictionV5 { position: p, occurrence: s, node: n, kind: FlatConvictionKindV5::Malformed })
}
fn decode(
    c: &FlatSegContextV5<'_>,
    pr: &crate::element::PreparedV1<'_>,
    f: &FlatDecodeFaultV5,
) -> Result<FlatConvictionV5, DismissalV1> {
    let delivered = *c.claim.generated.get(f.index as usize).ok_or_else(|| na("no delivered id"))?;
    let p = c.claim.prompt_len.checked_sub(1).and_then(|x| x.checked_add(f.index)).ok_or_else(|| na("no selecting position"))?;
    let s = (pr.w.occurrences.len() - 1) as u16;
    let n = c.claim.program.logits;
    let o = f.logits.node.as_ref().ok_or_else(|| na("missing logits node"))?;
    if !common::authenticate_node(c, pr, o, (p, s, n)) {
        return Err(na("borrowed logits node"));
    }
    let (dtype, shape) = pr.declared(s, n, p);
    let len = LayoutV3::of(&shape).len;
    let b = decode_bounds(c, pr, p, s, n)?;
    if f.logits.flat.len() + f.logits.legacy.len() > 2
        || common::size_with_limit(f, common::FLAT_FAULT_MAX_BYTES_V4).map_err(|_| na("oversized decode fault"))? > b.filing_bytes
    {
        return Err(na("decode dependency bound exceeded"));
    }
    let convict =
        || Ok(FlatConvictionV5 { position: p, occurrence: s, node: n, kind: FlatConvictionKindV5::Decode { index: f.index } });
    if delivered as u64 >= len {
        if !f.logits.flat.is_empty() || !f.logits.legacy.is_empty() {
            return Err(na("out-of-range decode only needs node material"));
        }
        return convict();
    }
    let flat = common::uses_flat::<5>(dtype, &shape);
    let want: Vec<_> = shape.iter().map(|n| *n as u64).collect();
    if flat {
        if !f.logits.legacy.is_empty()
            || f.logits.flat.is_empty()
            || f.logits
                .flat
                .iter()
                .any(|l| l.opening.dtype != dtype.tag() || l.opening.shape != want || !l.authenticates(&o.commitment))
            || f.logits.flat.windows(2).any(|w| w[0].opening.index >= w[1].opening.index)
        {
            return Err(na("bad flat logits proof"));
        }
    } else if !f.logits.flat.is_empty()
        || f.logits.legacy.is_empty()
        || f.logits
            .legacy
            .iter()
            .any(|l| l.dtype != dtype.tag() || l.shape != want || l.axis != crate::merkle::AXIS_ROW || !l.authenticates(&o.commitment))
        || f.logits.legacy.windows(2).any(|w| (w[0].line, w[0].tile) >= (w[1].line, w[1].tile))
    {
        return Err(na("bad small logits proof"));
    }
    if f.rival == delivered || f.rival as u64 >= len {
        return Err(na("invalid rival"));
    }
    let got = common::value_in_opening(&f.logits, dtype, &shape, delivered as u64).ok_or_else(|| na("missing delivered logit"))?;
    let rival = common::value_in_opening(&f.logits, dtype, &shape, f.rival as u64).ok_or_else(|| na("missing rival logit"))?;
    if rival > got || (rival == got && f.rival < delivered) { convict() } else { Err(DismissalV1::NoFault) }
}
pub(crate) fn decode_bounds(
    c: &FlatSegContextV5<'_>,
    pr: &crate::element::PreparedV1<'_>,
    p: u32,
    s: u16,
    n: u16,
) -> Result<FlatCourtBoundsV5, DismissalV1> {
    decode_bounds_program(c.claim.program, pr, p, s, n)
}
pub(crate) fn decode_bounds_program(
    program: &misaka_palw_tir::program::TirProgramV1,
    pr: &crate::element::PreparedV1<'_>,
    p: u32,
    s: u16,
    n: u16,
) -> Result<FlatCourtBoundsV5, DismissalV1> {
    let (dtype, shape) = pr.declared(s, n, p);
    let flat = common::uses_flat::<5>(dtype, &shape);
    let each = if flat {
        crate::merkle5::flat_material_bounds_v5(dtype, &shape.iter().map(|n| *n as u64).collect::<Vec<_>>())
            .ok_or_else(|| na("invalid logits"))?
            .leaf_value_bytes
    } else {
        94 + shape.len() as u64 * 8
            + LayoutV3::of(&shape).len * dtype.width() as u64
            + 64 * crate::merkle::depth(LayoutV3::of(&shape).leaves(0))
    };
    let bytes = 8 + 9 + crate::element::node_opening_bytes_v1(pr.node_count) + 2 * each;
    let program =
        common::size_with_limit(program, misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64).map_err(|_| na("large program"))?;
    Ok(FlatCourtBoundsV4 {
        filing_bytes: bytes,
        work: bytes * 16 + program * 1024 + pr.node_count * 256 + 8192,
        scratch_bytes: 8 << 20,
        operand_leaves: vec![2],
    })
}
/// Dispatch all terminal court kinds under the explicitly bound v5 profile.
pub fn verify_flat_fault_v5(c: &FlatSegContextV5<'_>, f: &FlatFaultV5) -> Result<FlatConvictionV5, DismissalV1> {
    f.check_structure().map_err(|_| na("malformed bounded flat-v5 fault"))?;
    let wire = common::size_with_limit(f, common::FLAT_FAULT_MAX_BYTES_V4 - 2).map_err(|_| na("oversized flat-v5 fault"))? + 2;
    let pr = common::prepare(c)?;
    let b = fault_bounds_prepared(c, &pr, f)?;
    if wire > b.filing_bytes {
        return Err(na("fault exceeds derived court bound"));
    }
    match f {
        FlatFaultV5::Leaf(f) => common::verify_leaf_prepared(
            c,
            &pr,
            f,
            common::size_with_limit(f, common::FLAT_FAULT_MAX_BYTES_V4).map_err(|_| na("oversized leaf"))?,
        )
        .map(|v| FlatConvictionV5 {
            position: v.position,
            occurrence: v.occurrence,
            node: v.node,
            kind: FlatConvictionKindV5::Leaf { leaf: v.leaf },
        }),
        FlatFaultV5::Whole(f) => whole(c, &pr, f),
        FlatFaultV5::Malformed(f) => malformed(c, &pr, f),
        FlatFaultV5::Decode(f) => decode(c, &pr, f),
    }
}

pub fn flat_fault_court_bounds_v5(c: &FlatSegContextV5<'_>, f: &FlatFaultV5) -> Result<FlatCourtBoundsV5, DismissalV1> {
    f.check_structure().map_err(|_| na("invalid court structure"))?;
    let pr = common::prepare(c)?;
    fault_bounds_prepared(c, &pr, f)
}
pub(crate) fn malformed_bounds(
    program: &misaka_palw_tir::program::TirProgramV1,
    pr: &crate::element::PreparedV1<'_>,
) -> Result<FlatCourtBoundsV5, DismissalV1> {
    let bytes = crate::element::node_opening_bytes_v1(pr.node_count) + 2 + 4 + 32 + 1 + 128;
    let program =
        common::size_with_limit(program, misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64).map_err(|_| na("large program"))?;
    Ok(FlatCourtBoundsV4 {
        filing_bytes: bytes,
        work: bytes * 16 + program * 1024 + pr.node_count * 256,
        scratch_bytes: 8 << 20,
        operand_leaves: vec![],
    })
}

fn fault_bounds_prepared(
    c: &FlatSegContextV5<'_>,
    pr: &crate::element::PreparedV1<'_>,
    f: &FlatFaultV5,
) -> Result<FlatCourtBoundsV5, DismissalV1> {
    let mut b = match f {
        FlatFaultV5::Leaf(f) => common::bounds(c, pr, f.output.position, f.output.occurrence, f.output.node, f.leaf.index)?,
        FlatFaultV5::Whole(f) => small_bounds(c, pr, f.output.position, f.output.occurrence, f.output.node)?,
        FlatFaultV5::Malformed(f) => {
            if !pr.valid(f.output.occurrence, f.output.node) || f.output.position >= c.claim.positions {
                return Err(na("no malformed target"));
            }
            malformed_bounds(c.claim.program, pr)?
        }
        FlatFaultV5::Decode(f) => {
            if f.index as usize >= c.claim.generated.len() {
                return Err(na("no decode index"));
            }
            let p =
                c.claim.prompt_len.checked_sub(1).and_then(|x| x.checked_add(f.index)).ok_or_else(|| na("no selecting position"))?;
            if p >= c.claim.positions {
                return Err(na("decode position out of extent"));
            }
            decode_bounds(c, pr, p, (pr.w.occurrences.len() - 1) as u16, c.claim.program.logits)?
        }
    };
    b.filing_bytes += 3; // explicit outer version and fault discriminant
    if b.filing_bytes > common::FLAT_FAULT_MAX_BYTES_V4
        || b.work > common::FLAT_FAULT_MAX_WORK_V4
        || b.scratch_bytes > common::FLAT_FAULT_MAX_SCRATCH_V4
    {
        return Err(na("flat-v5 court resource cap"));
    }
    Ok(b)
}
fn read_operand<R: std::io::Read>(r: &mut R, total: &mut usize, max: usize) -> io::Result<FlatOperandV5> {
    let node = common::optional(r, common::read_node)?;
    let flat = common::vector(r, max - *total, FlatLeafValueV5::deserialize_reader)?;
    *total += flat.len();
    let legacy = common::vector(r, max - *total, common::read_legacy)?;
    *total += legacy.len();
    Ok(FlatOperandV5 { node, flat, legacy })
}
fn read_token<R: std::io::Read>(r: &mut R) -> io::Result<Option<PromptTileOpeningV1>> {
    common::optional(r, |r| {
        Ok(PromptTileOpeningV1 {
            index: u32::deserialize_reader(r)?,
            ids: common::vector(r, 4096, u32::deserialize_reader)?,
            siblings: common::vector(r, 10, Digest::deserialize_reader)?,
        })
    })
}
impl FlatFaultV5 {
    fn check_structure(&self) -> io::Result<()> {
        match self {
            Self::Leaf(f) => common::structural(f),
            Self::Whole(f) => common::parts_structure(Some(&f.output), &f.inputs, &f.token),
            Self::Malformed(f) => {
                common::parts_structure::<5>(Some(&f.output), &[], &None)?;
                if f.header.metadata().1.len() > 4 || f.header.commitment().is_none() {
                    return Err(invalid());
                }
                Ok(())
            }
            Self::Decode(f) => {
                if f.logits.flat.len() + f.logits.legacy.len() > 2 {
                    return Err(invalid());
                }
                common::parts_structure(None, std::slice::from_ref(&f.logits), &None)
            }
        }
    }
    pub fn to_bytes(&self) -> io::Result<Vec<u8>> {
        self.check_structure()?;
        common::size_with_limit(self, common::FLAT_FAULT_MAX_BYTES_V4 - 2)?;
        let mut out = borsh::to_vec(&FLAT_FAULT_VERSION_V5)?;
        self.serialize(&mut out)?;
        Ok(out)
    }
    pub fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() as u64 > common::FLAT_FAULT_MAX_BYTES_V4 {
            return Err(invalid());
        }
        let mut r = Cursor::new(bytes);
        if u16::deserialize_reader(&mut r)? != FLAT_FAULT_VERSION_V5 {
            return Err(invalid());
        }
        let out = match u8::deserialize_reader(&mut r)? {
            0 => {
                if u16::deserialize_reader(&mut r)? != FLAT_FAULT_VERSION_V5 {
                    return Err(invalid());
                }
                let output = common::read_node(&mut r)?;
                let leaf = FlatLeafHashV5::deserialize_reader(&mut r)?;
                let mut total = 0;
                let inputs = common::vector(&mut r, 9, |r| read_operand(r, &mut total, 1024))?;
                let token = read_token(&mut r)?;
                Self::Leaf(FlatLeafFaultV5 { version: FLAT_FAULT_VERSION_V5, output, leaf, inputs, token })
            }
            1 => {
                let output = common::read_node(&mut r)?;
                let mut total = 0;
                let inputs = common::vector(&mut r, 9, |r| read_operand(r, &mut total, 1024))?;
                let token = read_token(&mut r)?;
                Self::Whole(FlatWholeFaultV5 { output, inputs, token })
            }
            2 => {
                let output = common::read_node(&mut r)?;
                let kind = u8::deserialize_reader(&mut r)?;
                if kind > 1 {
                    return Err(invalid());
                }
                let dtype = u8::deserialize_reader(&mut r)?;
                if !DType::ALL.iter().any(|d| d.tag() == dtype) {
                    return Err(invalid());
                }
                let shape = common::vector(&mut r, 4, u64::deserialize_reader)?;
                let header = if kind == 0 {
                    FlatHeaderV5::Legacy {
                        dtype,
                        shape,
                        row_root: Digest::deserialize_reader(&mut r)?,
                        col_root: Digest::deserialize_reader(&mut r)?,
                    }
                } else {
                    FlatHeaderV5::Flat { dtype, shape, root: Digest::deserialize_reader(&mut r)? }
                };
                Self::Malformed(FlatMalformedFaultV5 { output, header })
            }
            3 => {
                let index = u32::deserialize_reader(&mut r)?;
                let rival = u32::deserialize_reader(&mut r)?;
                let logits = read_operand(&mut r, &mut 0, 2)?;
                Self::Decode(FlatDecodeFaultV5 { index, rival, logits })
            }
            _ => return Err(invalid()),
        };
        if r.position() != bytes.len() as u64 {
            return Err(invalid());
        }
        out.check_structure()?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::{OperandSourceV1, SegClaimContextV1, operand_sources};
    use crate::flat_court4::tests::{identity, primitive_fixture, state_fixture};
    use crate::merkle3::{LeafOpeningV3, TreesV3};
    use crate::merkle5::{FlatLayoutV5, FlatTreeV5};
    use crate::seg::{SegmentedCommitmentsV1, prompt_root_of_ids_v1};
    use crate::trace::{ParamCommitmentsV1, SourceV1, TraceV1, trace_v1};
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::program::{HISTORY_BOUND_V1_SMALL, Ref, TirProgramV1};
    use misaka_palw_tir::{MapParams, TensorType};
    use std::collections::BTreeSet;
    struct Trial {
        program: TirProgramV1,
        params: MapParams,
        pc: ParamCommitmentsV1,
        prompt: Vec<u32>,
        generated: Vec<u32>,
        own: TraceV1,
        public: SegmentedCommitmentsV1,
        roots: Vec<Digest>,
    }
    impl Trial {
        fn of(
            program: TirProgramV1,
            params: MapParams,
            prompt: Vec<u32>,
            generated: Vec<u32>,
            wrong: Option<(u32, u16, u16, u64)>,
        ) -> Self {
            let mut tokens = prompt.clone();
            tokens.extend_from_slice(&generated[..generated.len().saturating_sub(1)]);
            let own = trace_v1(&program, &params, &tokens).unwrap();
            let mut posted = own.clone();
            if let Some((p, s, n, e)) = wrong {
                posted.values[p as usize][s as usize][n as usize].data[e as usize] += 1;
            }
            let commitments = posted
                .values
                .iter()
                .map(|p| p.iter().map(|s| s.iter().map(|t| flat_profile_commitment_v5(t).unwrap()).collect()).collect())
                .collect();
            let public = SegmentedCommitmentsV1::new(commitments);
            let roots = public.segment_roots();
            let pc = ParamCommitmentsV1::of_v3(&params);
            Self { program, params, pc, prompt, generated, own, public, roots }
        }
        fn context(&self) -> FlatSegContextV5<'_> {
            FlatSegContextV5 {
                profile: flat_trace_profile_id_v5(),
                claim: SegClaimContextV1 {
                    program: &self.program,
                    params: &self.pc,
                    segment_roots: &self.roots,
                    positions: self.public.positions(),
                    prompt_len: self.prompt.len() as u32,
                    prompt_root: prompt_root_of_ids_v1(&self.prompt),
                    inline_prompt: Some(&self.prompt),
                    generated: &self.generated,
                    decode: crate::job::DecodeRuleV1::Greedy,
                    encoder: None,
                },
            }
        }
        fn own_inputs(&self, p: u32, s: u16, n: u16) -> Vec<Tensor> {
            let c = self.context();
            let pr = common::prepare(&c).unwrap();
            operand_sources(&pr, p, s, n)
                .unwrap()
                .iter()
                .map(|src| match src {
                    OperandSourceV1::Token => {
                        Tensor::scalar(DType::Idx, crate::element::token_at(&c.claim, p, None).unwrap() as i128).unwrap()
                    }
                    OperandSourceV1::Position => Tensor::scalar(DType::Idx, p as i128).unwrap(),
                    OperandSourceV1::Source(SourceV1::Node { position, occurrence, node }) => {
                        self.own.value(*position, *occurrence, *node).unwrap().clone()
                    }
                    OperandSourceV1::Source(SourceV1::Param { index, layer }) => self.params.tensors[&(*index, *layer)].clone(),
                    OperandSourceV1::Source(SourceV1::Const(j)) => crate::trace::const_tensor(&self.program, *j).unwrap(),
                    OperandSourceV1::Source(SourceV1::Zeros { dtype, shape }) => Tensor::zeros(*dtype, shape),
                    _ => panic!("fixture source"),
                })
                .collect()
        }
        fn inputs(&self, p: u32, s: u16, n: u16, index: Option<u64>) -> Vec<FlatOperandV5> {
            let c = self.context();
            let own = self.own_inputs(p, s, n);
            let pr = common::prepare(&c).unwrap();
            let sources = operand_sources(&pr, p, s, n).unwrap();
            let union = if let Some(i) = index {
                flat_leaf_dependencies_v5(&c, p, s, n, i, &own).unwrap()
            } else {
                flat_whole_dependencies_v5(&c, p, s, n, &own).unwrap()
            };
            own.iter()
                .zip(sources)
                .zip(union.leaves)
                .map(|((t, source), reads)| {
                    let mut o = FlatOperandV5::default();
                    match source {
                        OperandSourceV1::Source(SourceV1::Node { position, occurrence, node }) => {
                            o.node = self.public.node_opening(position, occurrence, node)
                        }
                        OperandSourceV1::Source(SourceV1::Param { .. }) => {}
                        _ => return o,
                    }
                    if o.node.is_some() && t.data.len() >= 64 {
                        let tree = FlatTreeV5::of(t).unwrap();
                        o.flat = reads.into_iter().map(|(i, _)| FlatLeafValueV5::of(t, &tree, i).unwrap()).collect();
                    } else {
                        let tree = TreesV3::of(t);
                        o.legacy =
                            reads.into_iter().map(|(row, tile)| LeafOpeningV3::of_trees(t, &tree, 0, row, tile).unwrap()).collect();
                    }
                    o
                })
                .collect()
        }
        fn leaf(&self, p: u32, s: u16, n: u16, public_hash: FlatLeafHashV5) -> FlatFaultV5 {
            FlatFaultV5::Leaf(FlatLeafFaultV5 {
                version: 5,
                output: self.public.node_opening(p, s, n).unwrap(),
                inputs: self.inputs(p, s, n, Some(public_hash.index)),
                leaf: public_hash,
                token: None,
            })
        }
        fn whole(&self, p: u32, s: u16, n: u16) -> FlatFaultV5 {
            FlatFaultV5::Whole(FlatWholeFaultV5 {
                output: self.public.node_opening(p, s, n).unwrap(),
                inputs: self.inputs(p, s, n, None),
                token: None,
            })
        }
        fn set_root(&mut self, p: u32, s: u16, n: u16, root: Digest) {
            self.public.commitments[p as usize][s as usize][n as usize] = root;
            self.public = SegmentedCommitmentsV1::new(self.public.commitments.clone());
            self.roots = self.public.segment_roots();
        }
        fn decode(&self, index: u32, rival: u32) -> FlatFaultV5 {
            let p = self.prompt.len() as u32 - 1 + index;
            let s = (self.program.occurrences().len() - 1) as u16;
            let n = self.program.logits;
            let t = self.own.value(p, s, n).unwrap();
            let delivered = self.generated[index as usize];
            let mut o = FlatOperandV5 { node: self.public.node_opening(p, s, n), ..Default::default() };
            if (delivered as usize) < t.data.len() {
                let reads = BTreeSet::from([delivered as u64, rival as u64]);
                if t.data.len() >= 64 {
                    let tree = FlatTreeV5::of(t).unwrap();
                    let ids: BTreeSet<_> = reads.into_iter().map(|e| tree.layout().leaf_of(e).unwrap()).collect();
                    o.flat = ids.into_iter().map(|i| FlatLeafValueV5::of(t, &tree, i).unwrap()).collect();
                } else {
                    let trees = TreesV3::of(t);
                    let ids: BTreeSet<_> = reads
                        .into_iter()
                        .map(|e| {
                            let (r, t, _) = trees.layout.row_leaf_of(e).unwrap();
                            (r, t)
                        })
                        .collect();
                    o.legacy = ids.into_iter().map(|(r, tile)| LeafOpeningV3::of_trees(t, &trees, 0, r, tile).unwrap()).collect();
                }
            }
            FlatFaultV5::Decode(FlatDecodeFaultV5 { index, rival, logits: o })
        }
    }
    fn hash(t: &Tensor, index: u64) -> FlatLeafHashV5 {
        FlatTreeV5::of(t).unwrap().opening(index).unwrap()
    }
    fn assert_codec(trial: &Trial, f: &FlatFaultV5) {
        let bytes = f.to_bytes().unwrap();
        let decoded = FlatFaultV5::from_bytes(&bytes).unwrap();
        assert_eq!(*f, decoded);
        assert_eq!(verify_flat_fault_v5(&trial.context(), f), verify_flat_fault_v5(&trial.context(), &decoded));
        assert!(bytes.len() as u64 <= flat_fault_court_bounds_v5(&trial.context(), f).unwrap().filing_bytes);
    }
    #[test]
    fn flat_v5_all_25_primitive_samples_and_all_tail_leaves_preserve_reference_courts() {
        for tag in 0..23 {
            let (p, m, n) = primitive_fixture(tag);
            let honest = Trial::of(p.clone(), m.clone(), vec![1], vec![], None);
            let out = honest.own.value(0, 0, n).unwrap();
            let l = FlatLayoutV5::of(&out.shape.iter().map(|n| *n as u64).collect::<Vec<_>>()).unwrap();
            for i in 0..l.leaves() {
                let f = honest.leaf(0, 0, n, hash(out, i));
                assert_eq!(verify_flat_fault_v5(&honest.context(), &f), Err(DismissalV1::NoFault), "tag {tag}");
                assert_codec(&honest, &f);
            }
            let i = l.leaves() - 1;
            let (first, count) = l.span(i).unwrap();
            let e = first + count as u64 - 1;
            let bad = Trial::of(p, m, vec![1], vec![], Some((0, 0, n, e)));
            let mut actual = out.clone();
            actual.data[e as usize] += 1;
            let f = bad.leaf(0, 0, n, hash(&actual, i));
            assert_eq!(verify_flat_fault_v5(&bad.context(), &f).unwrap().kind, FlatConvictionKindV5::Leaf { leaf: i });
            assert_codec(&bad, &f);
        }
        let (p, m, write, hist) = state_fixture();
        let honest = Trial::of(p.clone(), m.clone(), vec![1, 2, 3, 1], vec![], None);
        for position in 0..4 {
            for n in [write, hist] {
                let t = honest.own.value(position, 0, n).unwrap();
                let l = FlatLayoutV5::of(&t.shape.iter().map(|n| *n as u64).collect::<Vec<_>>()).unwrap();
                for i in 0..l.leaves() {
                    let f = honest.leaf(position, 0, n, hash(t, i));
                    assert_eq!(verify_flat_fault_v5(&honest.context(), &f), Err(DismissalV1::NoFault));
                }
                let i = l.leaves() - 1;
                let (first, count) = l.span(i).unwrap();
                let e = first + count as u64 - 1;
                let bad = Trial::of(p.clone(), m.clone(), vec![1, 2, 3, 1], vec![], Some((position, 0, n, e)));
                let mut actual = t.clone();
                actual.data[e as usize] += 1;
                let f = bad.leaf(position, 0, n, hash(&actual, i));
                assert!(verify_flat_fault_v5(&bad.context(), &f).is_ok());
                assert_codec(&bad, &f);
            }
        }
    }
    fn wide_small() -> (TirProgramV1, MapParams, u16, u16) {
        let mut pb = ProgramBuilder::new(4, HISTORY_BOUND_V1_SMALL);
        let w = pb.param("independent.source", DType::I64, &[129], false);
        let mut pre = pb.block("pre", vec![]);
        let wide = pre.cast(w, DType::I128);
        let small = pre.slice(wide, 0, 123, 6);
        let narrowed = pre.clamp(small, -100, 100, DType::I32);
        let Ref::Node(wn) = wide else { unreachable!() };
        let Ref::Node(sn) = small else { unreachable!() };
        let pre = pre.finish(&[narrowed]);
        let mut post = pb.block("post", vec![TensorType::fixed(DType::I32, &[6])]);
        let y = post.cast(Ref::CarryIn(0), DType::I32);
        post.commit(y);
        let post = post.finish(&[]);
        let program = pb.finish(pre, vec![], post, 0);
        let mut params = MapParams::default();
        params.tensors.insert((0, None), Tensor::new(DType::I64, vec![129], (0..129).map(|n| n as i128 - 64).collect()).unwrap());
        (program, params, wn, sn)
    }
    #[test]
    fn flat_v5_first_wide_lie_and_small_whole_relation_need_no_producer_values() {
        let (p, m, wn, sn) = wide_small();
        let honest = Trial::of(p.clone(), m.clone(), vec![1], vec![], None);
        let f = honest.whole(0, 0, sn);
        assert_eq!(verify_flat_fault_v5(&honest.context(), &f), Err(DismissalV1::NoFault));
        assert_codec(&honest, &f);
        let bad = Trial::of(p.clone(), m.clone(), vec![1], vec![], Some((0, 0, sn, 5)));
        let f = bad.whole(0, 0, sn);
        assert_eq!(verify_flat_fault_v5(&bad.context(), &f).unwrap().kind, FlatConvictionKindV5::Whole);
        assert_codec(&bad, &f);
        if let FlatFaultV5::Whole(w) = &f {
            assert_eq!(w.inputs[0].flat.len(), 1);
            assert_eq!(w.inputs[0].flat[0].opening.dtype, DType::I128.tag());
            assert_eq!(w.inputs[0].flat[0].bytes.len(), 65 * 16);
        } else {
            unreachable!()
        }
        let mut missing = f.clone();
        if let FlatFaultV5::Whole(w) = &mut missing {
            w.inputs[0].flat.clear();
        }
        assert!(verify_flat_fault_v5(&bad.context(), &missing).is_err());
        let bad = Trial::of(p, m, vec![1], vec![], Some((0, 0, wn, 128)));
        let mut actual = honest.own.value(0, 0, wn).unwrap().clone();
        actual.data[128] += 1;
        let f = bad.leaf(0, 0, wn, hash(&actual, 1));
        assert!(verify_flat_fault_v5(&bad.context(), &f).is_ok());
        assert_codec(&bad, &f);
        let mut forbidden = bad.program.clone();
        forbidden.blocks[0].nodes[wn as usize].commit = true;
        assert!(misaka_palw_tir::validate::validate(&forbidden).is_err());
        let mut c = bad.context();
        c.profile = common::flat_trace_profile_id_v4();
        assert!(verify_flat_fault_v5(&c, &f).is_err());
    }
    #[test]
    fn flat_v5_small_whole_courts_cover_all_dtypes_and_scalar_rank() {
        for dtype in DType::ALL {
            for len in [1, 2, 63] {
                let mut pb = ProgramBuilder::new(4, HISTORY_BOUND_V1_SMALL);
                let shape = if len == 1 { vec![] } else { vec![len] };
                let w = pb.param("source", if dtype == DType::I128 { DType::I64 } else { dtype }, &shape, false);
                let mut pre = pb.block("pre", vec![]);
                let x = pre.cast(w, dtype);
                let n = 0;
                let y = pre.clamp(x, 0, 100, DType::I32);
                let carry = pre.ty(y);
                let pre = pre.finish(&[y]);
                let mut post = pb.block("post", vec![carry]);
                let y = post.cast(Ref::CarryIn(0), DType::I32);
                post.commit(y);
                let post = post.finish(&[]);
                let p = pb.finish(pre, vec![], post, 0);
                let mut m = MapParams::default();
                m.tensors.insert(
                    (0, None),
                    Tensor::new(
                        if dtype == DType::I128 { DType::I64 } else { dtype },
                        shape.iter().map(|n| *n as usize).collect(),
                        vec![7; len as usize],
                    )
                    .unwrap(),
                );
                let honest = Trial::of(p.clone(), m.clone(), vec![1], vec![], None);
                let f = honest.whole(0, 0, n);
                assert_eq!(verify_flat_fault_v5(&honest.context(), &f), Err(DismissalV1::NoFault));
                assert_codec(&honest, &f);
                let bad = Trial::of(p, m, vec![1], vec![], Some((0, 0, n, len as u64 - 1)));
                let f = bad.whole(0, 0, n);
                assert!(verify_flat_fault_v5(&bad.context(), &f).is_ok());
                assert_codec(&bad, &f);
            }
        }
    }
    #[test]
    fn flat_v5_malformed_headers_convict_type_shape_overflow_and_wrong_grammar_only_when_bound() {
        let (p, m) = identity(129);
        let mut trial = Trial::of(p, m, vec![1], vec![], None);
        let t = trial.own.value(0, 1, 0).unwrap();
        let tree = FlatTreeV5::of(t).unwrap();
        let valid = FlatHeaderV5::Flat { dtype: t.dtype.tag(), shape: vec![129], root: tree.root() };
        let f = FlatFaultV5::Malformed(FlatMalformedFaultV5 {
            output: trial.public.node_opening(0, 1, 0).unwrap(),
            header: valid.clone(),
        });
        assert_eq!(verify_flat_fault_v5(&trial.context(), &f), Err(DismissalV1::NoFault));
        assert_codec(&trial, &f);
        let legacy = TreesV3::of(t);
        let wrong_grammar =
            FlatHeaderV5::Legacy { dtype: t.dtype.tag(), shape: vec![129], row_root: legacy.row_root(), col_root: legacy.col_root() };
        assert_eq!(wrong_grammar.commitment(), Some(crate::merkle3::tensor_commitment_v3(t)));
        for h in [
            wrong_grammar,
            FlatHeaderV5::Flat { dtype: DType::I128.tag(), shape: vec![129], root: tree.root() },
            FlatHeaderV5::Flat { dtype: DType::I16.tag(), shape: vec![3, 43], root: tree.root() },
            FlatHeaderV5::Flat { dtype: DType::I16.tag(), shape: vec![u64::MAX, 2], root: [7; 64] },
            FlatHeaderV5::Legacy { dtype: DType::I16.tag(), shape: vec![0], row_root: [1; 64], col_root: [2; 64] },
        ] {
            trial.set_root(0, 1, 0, h.commitment().unwrap());
            let f = FlatFaultV5::Malformed(FlatMalformedFaultV5 {
                output: trial.public.node_opening(0, 1, 0).unwrap(),
                header: h.clone(),
            });
            assert_eq!(verify_flat_fault_v5(&trial.context(), &f).unwrap().kind, FlatConvictionKindV5::Malformed);
            assert_codec(&trial, &f);
            let mut borrowed = f.clone();
            if let FlatFaultV5::Malformed(f) = &mut borrowed {
                f.header = valid.clone();
            }
            assert!(verify_flat_fault_v5(&trial.context(), &borrowed).is_err());
        }
    }
    #[test]
    fn flat_v5_decode_binds_delivered_index_position_ties_and_logits_without_producer_values() {
        for n in [7, 129] {
            let (mut p, mut m) = identity(n);
            p.token_bound = n + 3;
            m.tensors.get_mut(&(0, None)).unwrap().data.fill(0);
            m.tensors.get_mut(&(0, None)).unwrap().data[1] = 10;
            for (delivered, rival, convicts) in [(1, 2, false), (2, 1, true), (1, 0, false)] {
                let t = Trial::of(p.clone(), m.clone(), vec![1, 2], vec![1, delivered], None);
                let f = t.decode(1, rival);
                let result = verify_flat_fault_v5(&t.context(), &f);
                assert_eq!(result.is_ok(), convicts);
                if !convicts {
                    assert_eq!(result, Err(DismissalV1::NoFault));
                }
                assert_codec(&t, &f);
                let mut borrowed = f.clone();
                if let FlatFaultV5::Decode(d) = &mut borrowed {
                    d.logits.node = t.public.node_opening(0, 1, 0);
                }
                assert!(verify_flat_fault_v5(&t.context(), &borrowed).is_err());
            }
            m.tensors.get_mut(&(0, None)).unwrap().data[2] = 10;
            let t = Trial::of(p.clone(), m.clone(), vec![1], vec![2], None);
            let f = t.decode(0, 1);
            assert!(verify_flat_fault_v5(&t.context(), &f).is_ok());
            let t = Trial::of(p.clone(), m.clone(), vec![1], vec![1], None);
            let f = t.decode(0, 2);
            assert_eq!(verify_flat_fault_v5(&t.context(), &f), Err(DismissalV1::NoFault));
            let t = Trial::of(p, m, vec![1], vec![n], None);
            let f = t.decode(0, 0);
            if let FlatFaultV5::Decode(d) = &f {
                assert!(d.logits.flat.is_empty() && d.logits.legacy.is_empty());
            }
            assert!(verify_flat_fault_v5(&t.context(), &f).is_ok());
            assert_codec(&t, &f);
        }
    }
    #[test]
    fn flat_v6_sealed_invalid_or_honest_proof_retains_slot_without_conviction() {
        use crate::flat_seal6::*;
        let (p, m) = identity(129);
        let t = Trial::of(p, m, vec![1], vec![0], None);
        // An honest first relation is a NoFault control. Sealing its bytes does
        // not turn authenticated evidence into a conviction.
        let value = t.own.value(0, 0, 0).unwrap();
        let proof = t.leaf(0, 0, 0, hash(value, 0)).to_bytes().unwrap();
        let (v, a, salt) = ([1; 64], [2; 64], [3; 64]);
        let mut book =
            FlatSealBookV6::new(FlatSealPolicyV6 { maturity_daa: 3, ttl_daa: 20, max_rows: 4, max_rows_per_actor: 2 }).unwrap();
        book.post(v, a, flat_proof_seal_v6(&v, &a, &salt, &proof).unwrap(), 10, 100).unwrap();
        assert_eq!(book.reveal(&v, &a, &salt, &proof, 13, &t.context()), Err(FlatSealErrorV6::InvalidCourt("NoFault".into())));
        assert!(!book.row(&v, &a).unwrap().spent);
        assert_eq!(book.rows_for(&a), 1);
        let bad = [5, 0, 255];
        book.post([4; 64], a, flat_proof_seal_v6(&[4; 64], &a, &salt, &bad).unwrap(), 10, 100).unwrap();
        assert!(matches!(book.reveal(&[4; 64], &a, &salt, &bad, 13, &t.context()), Err(FlatSealErrorV6::InvalidCourt(_))));
        assert!(!book.row(&[4; 64], &a).unwrap().spent);
        assert_eq!(book.rows_for(&a), 2);
    }
    #[test]
    fn flat_v6_own_mature_seal_blocks_equivalent_convicting_decode_encodings() {
        use crate::flat_seal6::*;
        let (mut p, mut m) = identity(129);
        p.token_bound = 132;
        m.tensors.get_mut(&(0, None)).unwrap().data.fill(0);
        m.tensors.get_mut(&(0, None)).unwrap().data[1] = 10;
        m.tensors.get_mut(&(0, None)).unwrap().data[2] = 9;
        // Two authentic rivals prove the same incorrectly delivered id. Their
        // exact proof bytes/digests differ even though their verdict is identical.
        for delivered in [0, 129] {
            let t = Trial::of(p.clone(), m.clone(), vec![1], vec![delivered], None);
            let f = t.decode(0, 1);
            let alternate = t.decode(0, 2);
            let expected = verify_flat_fault_v5(&t.context(), &f).unwrap();
            assert_eq!(verify_flat_fault_v5(&t.context(), &alternate).unwrap(), expected);
            let bytes = f.to_bytes().unwrap();
            let changed = alternate.to_bytes().unwrap();
            assert_ne!(bytes, changed);
            assert_ne!(flat_proof_digest_v6(&bytes).unwrap(), flat_proof_digest_v6(&changed).unwrap());
            let (vector, finder, copyist, salt) = ([1; 64], [2; 64], [3; 64], [4; 64]);
            let mut seals =
                FlatSealBookV6::new(FlatSealPolicyV6 { maturity_daa: 3, ttl_daa: 20, max_rows: 4, max_rows_per_actor: 2 }).unwrap();
            let sealed = flat_proof_seal_v6(&vector, &finder, &salt, &bytes).unwrap();
            seals.post(vector, finder, sealed, 10, 100).unwrap();
            // Copying another actor's opaque seal is ineffective after maturity.
            seals.post(vector, copyist, sealed, 10, 100).unwrap();
            assert_eq!(seals.reveal(&vector, &copyist, &salt, &changed, 13, &t.context()), Err(FlatSealErrorV6::SealMismatch));
            assert_eq!(seals.reveal(&vector, &finder, &salt, &changed, 13, &t.context()), Err(FlatSealErrorV6::SealMismatch));
            let r = seals.reveal(&vector, &finder, &salt, &bytes, 13, &t.context()).unwrap();
            assert_eq!(r.conviction, expected);
            assert_eq!(r.actor, finder);
            assert_eq!(seals.reveal(&vector, &finder, &salt, &bytes, 14, &t.context()), Err(FlatSealErrorV6::Spent));
            // A new, correctly rebound seal made after seeing a reveal still
            // cannot enter a court in the reveal's DAA interval.
            seals.expire(&vector, &copyist, 30).unwrap();
            let changed_seal = flat_proof_seal_v6(&vector, &copyist, &salt, &changed).unwrap();
            seals.post(vector, copyist, changed_seal, 30, 100).unwrap();
            assert_eq!(seals.reveal(&vector, &copyist, &salt, &changed, 30, &t.context()), Err(FlatSealErrorV6::Immature));
        }
    }
    #[test]
    fn flat_v5_all_fault_codecs_refuse_truncations_surplus_versions_and_nested_counts() {
        let (p, m, wn, sn) = wide_small();
        let trial = Trial::of(p, m, vec![1], vec![1], None);
        let header = FlatHeaderV5::Flat {
            dtype: DType::I128.tag(),
            shape: vec![129],
            root: FlatTreeV5::of(trial.own.value(0, 0, wn).unwrap()).unwrap().root(),
        };
        let cases = [
            trial.leaf(0, 0, wn, hash(trial.own.value(0, 0, wn).unwrap(), 1)),
            trial.whole(0, 0, sn),
            FlatFaultV5::Malformed(FlatMalformedFaultV5 { output: trial.public.node_opening(0, 0, wn).unwrap(), header }),
            trial.decode(0, 0),
        ];
        for f in cases {
            let bytes = f.to_bytes().unwrap();
            assert_codec(&trial, &f);
            for cut in 0..bytes.len() {
                assert!(FlatFaultV5::from_bytes(&bytes[..cut]).is_err(), "cut {cut}");
            }
            let mut bad = bytes.clone();
            bad.push(0);
            assert!(FlatFaultV5::from_bytes(&bad).is_err());
            let mut bad = bytes.clone();
            bad[0] = 4;
            assert!(FlatFaultV5::from_bytes(&bad).is_err());
            let mut bad = bytes;
            bad[2] = 4;
            assert!(FlatFaultV5::from_bytes(&bad).is_err());
        }
        let mut bad = trial.whole(0, 0, sn);
        if let FlatFaultV5::Whole(f) = &mut bad {
            f.inputs.resize(10, FlatOperandV5::default());
        }
        assert!(bad.to_bytes().is_err());
        let mut bad = trial.decode(0, 0);
        if let FlatFaultV5::Decode(f) = &mut bad {
            let copy = f.logits.legacy[0].clone();
            f.logits.legacy.resize(3, copy);
        }
        assert!(bad.to_bytes().is_err());
        let mut bytes = vec![5, 0, 2];
        let o = trial.public.node_opening(0, 0, wn).unwrap();
        o.serialize(&mut bytes).unwrap();
        bytes.push(1);
        bytes.push(DType::I128.tag());
        bytes.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(FlatFaultV5::from_bytes(&bytes).is_err());
    }
    #[test]
    fn flat_v5_history_crosses_small_whole_to_large_leaf_and_sliding_window_with_bound_sources() {
        let p = crate::flat_scope5::tests::history(1, 65);
        let scope = crate::flat_scope5::flat_resource_scope_v5(&p, 66, None).unwrap();
        let honest = Trial::of(p.clone(), MapParams::default(), vec![1; 66], vec![], None);
        for position in [0, 62, 63, 64, 65] {
            let value = honest.own.value(position, 0, 2).unwrap();
            let f = if value.data.len() < 64 { honest.whole(position, 0, 2) } else { honest.leaf(position, 0, 2, hash(value, 0)) };
            assert_eq!(verify_flat_fault_v5(&honest.context(), &f), Err(DismissalV1::NoFault));
            assert_codec(&honest, &f);
            let mut actual = value.clone();
            let e = actual.data.len() - 1;
            actual.data[e] += 1;
            let bad = Trial::of(p.clone(), MapParams::default(), vec![1; 66], vec![], Some((position, 0, 2, e as u64)));
            let f = if value.data.len() < 64 { bad.whole(position, 0, 2) } else { bad.leaf(position, 0, 2, hash(&actual, 0)) };
            assert!(verify_flat_fault_v5(&bad.context(), &f).is_ok());
            let bound = scope.relations.iter().find(|r| r.block == 0 && r.node == 2).unwrap();
            assert!(f.to_bytes().unwrap().len() as u64 <= bound.filing_bytes);
            if position > 0 {
                let mut missing = f.clone();
                let inputs = match &mut missing {
                    FlatFaultV5::Whole(f) => &mut f.inputs,
                    FlatFaultV5::Leaf(f) => &mut f.inputs,
                    _ => unreachable!(),
                };
                inputs[1].flat.clear();
                inputs[1].legacy.clear();
                assert!(verify_flat_fault_v5(&bad.context(), &missing).is_err());
                let mut borrowed = f.clone();
                let inputs = match &mut borrowed {
                    FlatFaultV5::Whole(f) => &mut f.inputs,
                    FlatFaultV5::Leaf(f) => &mut f.inputs,
                    _ => unreachable!(),
                };
                inputs[1].node = bad.public.node_opening(position, 0, 2);
                assert!(verify_flat_fault_v5(&bad.context(), &borrowed).is_err());
            }
        }
    }
}
