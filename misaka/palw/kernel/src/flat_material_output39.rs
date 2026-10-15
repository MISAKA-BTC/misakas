//! Owning39 upstream output certificates. The native adapter authenticates the
//! original Final/once-credit book; this checker authenticates every declared
//! output value to that accepted claim, with one bounded opening resident.
use crate::{
    flat_material39::{
        MATERIAL_MAX_RAW_V39, MATERIAL_PAGE_BYTES_V39, MaterialDescriptorV39, MaterialOriginV39, MaterialPageReaderV39,
        MaterialPageV39, read_material_range_v39, tensor_commitment_streamed_v39,
    },
    flat_point39::FlatClaimV39,
    flat_prepared39::PreparedV39,
    flat_source39::CheckedSourceV39,
    hash::{Digest, object_id},
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::{DType, program_v2::OutputDecl};
use std::io::Cursor;

/// Before-page contract, signed and paid together with the material descriptor.
/// `bytes` is the exact canonical stream extent, bounded independently from the
/// original program/claim. There is no caller-supplied row or tensor authority.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum OutputProjectionV39 {
    Whole,
    Rows { rows_from: u32, drop: u32, pad_to: Option<u32> },
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct OutputCertificatePlanV39 {
    pub version: u16,
    pub source: Digest,
    pub claim: Digest,
    pub output_declaration: Digest,
    pub tensor: Digest,
    pub dtype: DType,
    pub shape: Vec<u32>,
    pub projection: OutputProjectionV39,
    pub count: u32,
    pub bytes: u64,
    pub page_root: Digest,
}
impl OutputCertificatePlanV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/upstream-output-certificate/v39", self)
    }
    pub fn content_binding(&self) -> Digest {
        object_id(
            b"misaka-palw/kernel/upstream-output-certificate-content/v39",
            &(
                self.version,
                self.source,
                self.claim,
                self.output_declaration,
                self.tensor,
                self.dtype,
                &self.shape,
                &self.projection,
                self.count,
                self.bytes,
            ),
        )
    }
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        let mut r = crate::flat_codec39::Reader39::new(raw, 512)?;
        let p = Self {
            version: r.get()?,
            source: r.get()?,
            claim: r.get()?,
            output_declaration: r.get()?,
            tensor: r.get()?,
            dtype: r.get()?,
            shape: r.words(4)?,
            projection: r.get()?,
            count: r.get()?,
            bytes: r.get()?,
            page_root: r.get()?,
        };
        r.finish(&p)?;
        Ok(p)
    }
    pub fn to_bytes(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("output39 Vec encoding")
    }
}
/// Private checked geometry and fixed before-page resource maxima. Native
/// money and shared21 reservations must include these quantities at Start61.
pub struct CheckedOutputCertificatePlanV39 {
    record: OutputCertificatePlanV39,
    occurrence: u16,
    node: u16,
    node_count: u64,
    row_shape: Vec<usize>,
    first_position: u32,
    maximum_opening_bytes: usize,
}
impl CheckedOutputCertificatePlanV39 {
    pub fn check(
        source: &CheckedSourceV39,
        claim: &FlatClaimV39,
        descriptor: &MaterialDescriptorV39,
        record: &OutputCertificatePlanV39,
    ) -> Result<Self, String> {
        if claim.version != 39
            || claim.source != source.root()
            || claim.positions == 0
            || claim.positions > source.original_context_max()
            || claim.segment_roots.len() != crate::seg::segment_count_v1(claim.positions) as usize
            || claim.segment_roots.iter().any(|r| *r == [0; 64])
        {
            return Err("output39 original accepted claim/source geometry differs".into());
        }
        let MaterialOriginV39::Upstream { source: original_source, claim: original_claim, output, .. } = &descriptor.origin else {
            return Err("output39 material has no upstream output origin".into());
        };
        let pr = PreparedV39::new(source, claim.positions)?;
        let occurrence = u16::try_from(pr.occurrences().len() - 1).map_err(|_| "output39 occurrence")?;
        let node = source.program().output.node();
        let declaration = pr.node(occurrence, node)?;
        let last = claim.positions - 1;
        let row_shape = pr.shapes(last, occurrence, node)?.out;
        let (count, first_position, expected_shape) = match (&source.program().output, &record.projection) {
            (OutputDecl::Rows { .. }, projection) => {
                if pr.shapes(0, occurrence, node)?.out != row_shape {
                    return Err("output39 Rows shape changes across positions".into());
                }
                let (from, pad) = match projection {
                    OutputProjectionV39::Whole => (0, None),
                    OutputProjectionV39::Rows { rows_from, drop, pad_to } => {
                        (rows_from.checked_add(*drop).ok_or("output39 original row offset overflow")?, *pad_to)
                    }
                };
                let count = claim.positions.checked_sub(from).ok_or("output39 row projection exceeds original actual positions")?;
                let rows = pad.unwrap_or(count);
                if rows == 0 || rows < count {
                    return Err("output39 original row projection/padding extent".into());
                }
                (count, from, std::iter::once(rows).chain(row_shape.iter().map(|d| *d as u32)).collect::<Vec<_>>())
            }
            (OutputDecl::Final { .. }, OutputProjectionV39::Whole) => (1, last, row_shape.iter().map(|d| *d as u32).collect()),
            (OutputDecl::Final { .. }, _) => return Err("output39 Final cannot substitute a Rows projection".into()),
            (OutputDecl::Logits { .. }, _) => {
                return Err("output39 logits require the separate original decode-token output contract".into());
            }
        };
        let output_declaration =
            object_id(b"misaka-palw/kernel/original-output-declaration/v39", &(source.root(), &source.program().output));
        let max_path = crate::merkle::depth(pr.node_count())
            .checked_add(crate::merkle::depth(crate::seg::SEG_LEN_V4 as u64))
            .ok_or("output39 path bound")?;
        let maximum_opening_bytes = usize::try_from(144 + max_path * 64).map_err(|_| "output39 opening bound")?;
        let maximum_bytes = 6u64
            .checked_add((count as u64).checked_mul(4 + maximum_opening_bytes as u64).ok_or("output39 certificate extent")?)
            .ok_or("output39 certificate extent")?;
        let minimum_bytes = 6u64.checked_add(count as u64 * 148).ok_or("output39 minimum extent")?;
        if record.version != 39
            || record.source != source.root()
            || record.claim != claim.root()
            || *original_source != record.source
            || *original_claim != record.claim
            || *output != record.root()
            || record.output_declaration != output_declaration
            || record.tensor != descriptor.tensor
            || record.dtype != declaration.out.dtype
            || record.dtype != descriptor.dtype
            || record.shape != expected_shape
            || descriptor.shape != expected_shape
            || record.shape.len() > 4
            || record.count != count
            || record.bytes < minimum_bytes
            || record.bytes > maximum_bytes
            || record.bytes > MATERIAL_MAX_RAW_V39
            || record.page_root == [0; 64]
        {
            return Err("output39 exact original output/descriptor/certificate contract differs".into());
        }
        Ok(Self {
            record: record.clone(),
            occurrence,
            node,
            node_count: pr.node_count(),
            row_shape,
            first_position,
            maximum_opening_bytes,
        })
    }
    pub fn record(&self) -> &OutputCertificatePlanV39 {
        &self.record
    }
    pub fn root(&self) -> Digest {
        self.record.root()
    }
    pub fn pages(&self) -> u16 {
        self.record.bytes.div_ceil(MATERIAL_PAGE_BYTES_V39 as u64) as u16
    }
    pub fn stored_bytes(&self) -> u64 {
        self.record.bytes
    }
    pub fn delivery_bytes(&self) -> u64 {
        self.record.bytes * 2
    }
    /// Both original failed seal attempts traverse the canonical certificate,
    /// material whole C_v3 and each row C_v3 (row and column leaf passes).
    pub fn hashing_bytes(&self, material_raw_bytes: u64) -> u128 {
        self.record.bytes as u128 * 4 + material_raw_bytes as u128 * 8
    }
    pub fn read_calls(&self, material_elements: u64) -> u128 {
        self.record.bytes as u128 * 2 + material_elements as u128 * 8
    }
    pub fn scratch_bytes(&self) -> u64 {
        1 << 20
    }
}
pub fn output_certificate_page_leaf_v39(plan: &OutputCertificatePlanV39, index: u16, body: &[u8]) -> Digest {
    object_id(b"misaka-palw/kernel/output-certificate-page/v39", &(plan.content_binding(), index, body))
}
pub fn output_certificate_page_node_v39(a: &Digest, b: &Digest) -> Digest {
    object_id(b"misaka-palw/kernel/output-certificate-page-node/v39", &(a, b))
}
pub trait OutputCertificatePageReaderV39 {
    fn page(&self, certificate: Digest, index: u16) -> Result<MaterialPageV39, String>;
}
/// Canonical framing: u16=39,u32=count, then count*(u32 openingLen,
/// Borsh NodeOpeningV1). A checked page cache holds at most one32KiB page.
struct CertificateStream<'a> {
    plan: &'a CheckedOutputCertificatePlanV39,
    reader: &'a dyn OutputCertificatePageReaderV39,
    at: u64,
    page: Option<MaterialPageV39>,
}
impl CertificateStream<'_> {
    fn take(&mut self, n: usize) -> Result<Vec<u8>, String> {
        let end =
            self.at.checked_add(n as u64).filter(|n| *n <= self.plan.record.bytes).ok_or("output39 certificate stream truncated")?;
        if n > 8192 {
            return Err("output39 bounded original opening allocation".into());
        }
        let mut out = Vec::with_capacity(n);
        while self.at < end {
            let index = (self.at / MATERIAL_PAGE_BYTES_V39 as u64) as u16;
            if self.page.as_ref().is_none_or(|p| p.index != index) {
                let page = self.reader.page(self.plan.root(), index)?;
                let exact = (self.plan.record.bytes - index as u64 * MATERIAL_PAGE_BYTES_V39 as u64)
                    .min(MATERIAL_PAGE_BYTES_V39 as u64) as usize;
                if page.index != index
                    || page.body.len() != exact
                    || page.siblings.len() > 14
                    || crate::merkle3::tree_root_from_path(
                        output_certificate_page_leaf_v39(self.plan.record(), index, &page.body),
                        index as u64,
                        self.plan.pages() as u64,
                        &page.siblings,
                        output_certificate_page_node_v39,
                    ) != Some(self.plan.record.page_root)
                {
                    return Err("output39 original certificate page/path differs".into());
                }
                self.page = Some(page);
            }
            let page = self.page.as_ref().unwrap();
            let from = (self.at % MATERIAL_PAGE_BYTES_V39 as u64) as usize;
            let count = ((end - self.at) as usize).min(page.body.len() - from);
            out.extend_from_slice(&page.body[from..from + count]);
            self.at += count as u64;
        }
        Ok(out)
    }
}
pub struct CheckedUpstreamMaterialOutputV39 {
    descriptor: Digest,
    certificate: Digest,
    source: Digest,
    claim: Digest,
}
impl CheckedUpstreamMaterialOutputV39 {
    pub fn descriptor(&self) -> Digest {
        self.descriptor
    }
    pub fn certificate(&self) -> Digest {
        self.certificate
    }
    pub fn source(&self) -> Digest {
        self.source
    }
    pub fn claim(&self) -> Digest {
        self.claim
    }
}
/// A successful token proves tensor equality to every original trace output.
/// It does not prove Final status, original master linkage or actual credit;
/// the native adapter must authenticate those rows before using this token.
pub fn check_upstream_material_output_v39(
    source: &CheckedSourceV39,
    accepted_claim: Digest,
    claim: &FlatClaimV39,
    descriptor: &MaterialDescriptorV39,
    plan: &CheckedOutputCertificatePlanV39,
    material: &dyn MaterialPageReaderV39,
    certificates: &dyn OutputCertificatePageReaderV39,
) -> Result<CheckedUpstreamMaterialOutputV39, String> {
    if claim.root() != accepted_claim {
        return Err("output39 accepted original claim differs".into());
    }
    let again = CheckedOutputCertificatePlanV39::check(source, claim, descriptor, plan.record())?;
    let shape = descriptor.shape.iter().map(|d| *d as usize).collect::<Vec<_>>();
    let width = descriptor.dtype.width();
    let commitment = |shape: &[usize], base: u64| {
        tensor_commitment_streamed_v39(descriptor.dtype, shape, |offset, count| {
            let first = base.checked_add(offset).and_then(|n| n.checked_mul(width as u64)).ok_or("output39 raw tensor offset")?;
            let raw =
                read_material_range_v39(descriptor, material, first, count.checked_mul(width).ok_or("output39 raw tensor extent")?)?;
            let values = raw.chunks_exact(width).map(|v| descriptor.dtype.decode_le(v)).collect::<Vec<_>>();
            if matches!(source.program().output, OutputDecl::Rows { .. }) {
                let row_cells =
                    again.row_shape.iter().try_fold(1u64, |n, d| n.checked_mul(*d as u64)).ok_or("output39 padded row extent")?;
                let padding = again.record.count as u64 * row_cells;
                for (i, value) in values.iter().enumerate() {
                    if base + offset + i as u64 >= padding && *value != 0 {
                        return Err("output39 original projected padding is not zero".into());
                    }
                }
            }
            Ok(values)
        })
    };
    if commitment(&shape, 0)? != descriptor.tensor {
        return Err("output39 whole original material tensor C differs".into());
    }
    let row_cells = again.row_shape.iter().try_fold(1u64, |n, d| n.checked_mul(*d as u64)).ok_or("output39 row extent")?;
    let mut stream = CertificateStream { plan: &again, reader: certificates, at: 0, page: None };
    if u16::from_le_bytes(stream.take(2)?.try_into().unwrap()) != 39
        || u32::from_le_bytes(stream.take(4)?.try_into().unwrap()) != again.record.count
    {
        return Err("output39 original certificate stream header".into());
    }
    let node_index = PreparedV39::new(source, claim.positions)?.index(again.occurrence, again.node)?;
    for i in 0..again.record.count {
        let n = u32::from_le_bytes(stream.take(4)?.try_into().unwrap()) as usize;
        if n > again.maximum_opening_bytes || n < 144 {
            return Err("output39 opening original bound before allocation".into());
        }
        let raw = stream.take(n)?;
        let mut cursor = Cursor::new(raw.as_slice());
        let opening = crate::flat_court4::read_node(&mut cursor).map_err(|_| "output39 bounded original node opening")?;
        if cursor.position() != raw.len() as u64 || borsh::to_vec(&opening).map_err(|e| e.to_string())? != raw {
            return Err("output39 noncanonical opening".into());
        }
        let p = again.first_position.checked_add(i).ok_or("output39 projected original position")?;
        if (opening.position, opening.occurrence, opening.node) != (p, again.occurrence, again.node)
            || !opening.authenticates(&claim.segment_roots, claim.positions, node_index, again.node_count)
        {
            return Err("output39 output node not original accepted trace".into());
        }
        let row = if matches!(source.program().output, OutputDecl::Rows { .. }) {
            commitment(&again.row_shape, i as u64 * row_cells)?
        } else {
            descriptor.tensor
        };
        if opening.commitment != row {
            return Err("output39 authentic output differs from material row C".into());
        }
    }
    if stream.at != again.record.bytes {
        return Err("output39 unused certificate stream bytes".into());
    }
    Ok(CheckedUpstreamMaterialOutputV39 {
        descriptor: descriptor.root(),
        certificate: again.root(),
        source: source.root(),
        claim: accepted_claim,
    })
}

pub fn output_certificate_page_tree_root_v39(leaves: &[Digest]) -> Result<Digest, String> {
    if leaves.is_empty() || leaves.len() > crate::flat_material39::MATERIAL_MAX_PAGES_V39 as usize {
        return Err("output39 original certificate page count".into());
    }
    Ok(crate::merkle3::tree_root(
        leaves.to_vec(),
        output_certificate_page_node_v39,
        b"misaka-palw/kernel/output-certificate-page-empty/v39",
    ))
}
pub fn output_certificate_page_path_v39(leaves: &[Digest], index: u16) -> Result<Vec<Digest>, String> {
    if leaves.is_empty() || leaves.len() > crate::flat_material39::MATERIAL_MAX_PAGES_V39 as usize || index as usize >= leaves.len() {
        return Err("output39 original certificate page index/count".into());
    }
    Ok(crate::merkle3::tree_path(leaves.to_vec(), index as usize, output_certificate_page_node_v39))
}
