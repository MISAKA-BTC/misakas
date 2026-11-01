//! Opt-in, logical-cap-bounded PALWTIR1 metadata parsing. Existing loaders are
//! unchanged. This module neither decodes a TIR program nor authenticates a
//! registered class, artifact root or execution context. The caller must retain
//! its independently bounded canonical-program owner through every borrowed view.
//! No tensor is read, mapped, indexed into a heap tree, or executed here.

use crate::{
    PALW_TIR_CONTAINER_MAGIC_V1, PALW_TIR_CONTAINER_VERSION_V1, PALW_TIR_MAX_HEADER_BYTES_V1, PALW_TIR_TENSOR_ALIGN_V1, PREFIX,
    PalwTirContainerError as Error, PalwTirContainerHeaderV1, PalwTirTensorEntryV1, Result,
};
use borsh::BorshSerialize;
use misaka_palw_tir::{
    Ref, TirProgramV1,
    program::{MAX_BLOCKS, MAX_LAYERS, MAX_NODE_INPUTS, MAX_NODES_PER_BLOCK, MAX_PARAMS, MAX_PROGRAM_BYTES, MAX_STATES},
};
use std::{
    io::{self, Read, Write},
    mem::size_of,
};

const SCRATCH_BYTES: u64 = 8192;

/// Per-operation local policy, not a host lease or a physical RSS claim.
/// `max_copy_bytes` covers retained header Vec element storage, including tensor
/// headers. The encoded header frame has its own `max_header_bytes` bound.
/// External program owner, open descriptor, mappings and prior exports must be
/// pre-summed in `external_retained_bytes` while they coexist with this operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwTirHeaderLimitsV1 {
    pub max_file_bytes: u64,
    pub max_header_bytes: u64,
    pub max_program_bytes: u64,
    pub max_layout_bytes: u64,
    pub max_tensors: u64,
    pub max_meta_bytes: u64,
    pub max_native_bytes: u64,
    pub max_copy_bytes: u64,
    pub max_work: u64,
    pub max_operation_bytes: u64,
    pub external_retained_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwTirHeaderQuoteV1 {
    pub header_bytes: u64,
    pub native_bytes: u64,
    pub copy_bytes: u64,
    pub work: u64,
    pub operation_peak_bytes: u64,
}

/// Retained metadata only. Parsed layout words remain borrowed raw bytes; no
/// consensus type or alternate layout serialization is introduced.
pub struct BoundedPalwTirHeaderV1 {
    header: PalwTirContainerHeaderV1,
    file_len: u64,
    header_len: u64,
}

fn bad(message: &'static str) -> Error {
    Error::Header(message.into())
}
fn add(a: u64, b: u64) -> Result<u64> {
    a.checked_add(b).ok_or_else(|| bad("bounded metadata addition overflow"))
}
fn mul(a: u64, b: u64) -> Result<u64> {
    a.checked_mul(b).ok_or_else(|| bad("bounded metadata multiplication overflow"))
}
fn count(n: usize) -> Result<u64> {
    u64::try_from(n).map_err(|_| bad("bounded metadata native width"))
}
fn cap(value: u64, maximum: u64, message: &'static str) -> Result<()> {
    if value > maximum { Err(bad(message)) } else { Ok(()) }
}
fn exact<T>(n: usize) -> Result<Vec<T>> {
    let mut out = Vec::new();
    out.try_reserve_exact(n).map_err(|_| bad("bounded metadata allocation refused"))?;
    if out.capacity() != n {
        return Err(bad("allocator exceeded the exact metadata request"));
    }
    Ok(out)
}
fn peak(frame: u64, native: u64, limits: PalwTirHeaderLimitsV1) -> Result<u64> {
    let result = add(add(frame, native)?, add(SCRATCH_BYTES, limits.external_retained_bytes)?)?;
    cap(native, limits.max_native_bytes, "decoded metadata native ceiling")?;
    cap(result, limits.max_operation_bytes, "metadata operation ceiling")?;
    Ok(result)
}
fn check_frame(header: u64, file: u64, limits: PalwTirHeaderLimitsV1) -> Result<u64> {
    cap(file, limits.max_file_bytes, "file byte ceiling")?;
    cap(header, limits.max_header_bytes.min(u64::from(PALW_TIR_MAX_HEADER_BYTES_V1)), "header byte ceiling")?;
    if add(PREFIX, header)? > file {
        return Err(bad("header runs past file end"));
    }
    let work = mul(add(add(header, PREFIX)?, 1)?, 128)?;
    cap(work, limits.max_work, "header parser work ceiling")?;
    peak(header, count(size_of::<BoundedPalwTirHeaderV1>())?, limits)?;
    Ok(work)
}

struct Reader<'a> {
    bytes: &'a [u8],
    frame: u64,
    native: u64,
    copied: u64,
    limits: PalwTirHeaderLimitsV1,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if n > self.bytes.len() {
            return Err(bad("metadata frame ends early"));
        }
        let (value, rest) = self.bytes.split_at(n);
        self.bytes = rest;
        Ok(value)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().expect("two bytes")))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("four bytes")))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().expect("eight bytes")))
    }
    fn length(&mut self, minimum: usize, maximum: u64) -> Result<usize> {
        let n = u64::from(self.u32()?);
        cap(n, maximum, "metadata field count ceiling")?;
        let n = usize::try_from(n).map_err(|_| bad("metadata count exceeds native indices"))?;
        if minimum == 0 || n > self.bytes.len() / minimum {
            return Err(bad("metadata length exceeds remaining frame"));
        }
        Ok(n)
    }
    fn reserve<T>(&mut self, n: usize) -> Result<Vec<T>> {
        let bytes = mul(count(n)?, count(size_of::<T>())?)?;
        let native = add(self.native, bytes)?;
        let copied = add(self.copied, bytes)?;
        cap(copied, self.limits.max_copy_bytes, "metadata copy ceiling")?;
        peak(self.frame, native, self.limits)?;
        let out = exact(n)?;
        self.native = native;
        self.copied = copied;
        Ok(out)
    }
    fn bytes(&mut self, maximum: u64) -> Result<Vec<u8>> {
        let n = self.length(1, maximum)?;
        let mut out = self.reserve(n)?;
        out.extend_from_slice(self.take(n)?);
        Ok(out)
    }
    fn finish(&self) -> Result<()> {
        if self.bytes.is_empty() { Ok(()) } else { Err(bad("trailing metadata bytes")) }
    }
}

/// Borrowed interpretation of exactly the existing version1 layout fields. This
/// is a read-only view of its existing bytes, not a new wire DTO or identifier.
#[derive(Clone, Copy, Debug)]
pub struct PalwTirLayoutBytesV1<'a> {
    pub version: u16,
    pub max_context: u32,
    pub checkpoint_interval: u32,
    pub h_tile: u32,
    commits: &'a [u8],
    states: &'a [u8],
}
impl PalwTirLayoutBytesV1<'_> {
    pub fn commit_tiles(&self) -> impl ExactSizeIterator<Item = u32> + '_ {
        self.commits.chunks_exact(4).map(|v| u32::from_le_bytes(v.try_into().expect("four bytes")))
    }
    pub fn state_tiles(&self) -> impl ExactSizeIterator<Item = u32> + '_ {
        self.states.chunks_exact(4).map(|v| u32::from_le_bytes(v.try_into().expect("four bytes")))
    }
}
fn layout_view(bytes: &[u8]) -> Result<Option<PalwTirLayoutBytesV1<'_>>> {
    if bytes.is_empty() {
        return Ok(None);
    } // Preserve a lowerer's undeclared layout.
    let mut remaining = bytes;
    fn take<'a>(r: &mut &'a [u8], n: usize) -> Result<&'a [u8]> {
        if n > r.len() {
            return Err(bad("layout frame ends early"));
        }
        let (v, rest) = r.split_at(n);
        *r = rest;
        Ok(v)
    }
    fn u32(r: &mut &[u8]) -> Result<u32> {
        Ok(u32::from_le_bytes(take(r, 4)?.try_into().expect("four bytes")))
    }
    let version = u16::from_le_bytes(take(&mut remaining, 2)?.try_into().expect("two bytes"));
    if version != 1 {
        return Err(bad("unsupported explicit layout version"));
    }
    let max_context = u32(&mut remaining)?;
    let checkpoint_interval = u32(&mut remaining)?;
    let h_tile = u32(&mut remaining)?;
    let nc = u32(&mut remaining)? as usize;
    if nc > MAX_BLOCKS * MAX_NODES_PER_BLOCK {
        return Err(bad("layout commit count exceeds existing format bound"));
    }
    let commits = take(&mut remaining, nc.checked_mul(4).ok_or_else(|| bad("layout word count overflow"))?)?;
    let ns = u32(&mut remaining)? as usize;
    if ns > MAX_STATES {
        return Err(bad("layout state count exceeds existing format bound"));
    }
    let states = take(&mut remaining, ns.checked_mul(4).ok_or_else(|| bad("layout word count overflow"))?)?;
    if !remaining.is_empty() {
        return Err(bad("trailing layout bytes"));
    }
    Ok(Some(PalwTirLayoutBytesV1 { version, max_context, checkpoint_interval, h_tile, commits, states }))
}

/// Parse an already framed PALWTIR1 header. The borrowed frame is included once
/// in the operation peak. No generic Borsh deserialize is used, including layout.
pub fn decode_header_bounded_v1(body: &[u8], file_len: u64, limits: PalwTirHeaderLimitsV1) -> Result<BoundedPalwTirHeaderV1> {
    let frame = count(body.len())?;
    check_frame(frame, file_len, limits)?;
    let mut r = Reader { bytes: body, frame, native: count(size_of::<BoundedPalwTirHeaderV1>())?, copied: 0, limits };
    let version = r.u16()?;
    if version != PALW_TIR_CONTAINER_VERSION_V1 {
        return Err(bad("unsupported header version"));
    }
    let program = r.bytes(limits.max_program_bytes.min(MAX_PROGRAM_BYTES as u64))?;
    // u16 + three u32 + two u32 lengths + the existing bounded word arrays.
    let layout_format_bytes = add(22, mul(4, count(MAX_BLOCKS * MAX_NODES_PER_BLOCK + MAX_STATES)?)?)?;
    let layout = r.bytes(limits.max_layout_bytes.min(layout_format_bytes))?;
    layout_view(&layout)?;
    let tokenizer_id = r.take(64)?.try_into().expect("64 bytes");
    let format_tensors = mul(count(MAX_PARAMS)?, count(MAX_LAYERS.max(1))?)?;
    // One entry uses at least param u16 + option u8 + two u64 = 19 bytes.
    let nt = r.length(19, limits.max_tensors.min(format_tensors))?;
    let mut tensors = r.reserve(nt)?;
    for _ in 0..nt {
        let param = r.u16()?;
        let layer = match r.u8()? {
            0 => None,
            1 => Some(r.u16()?),
            _ => return Err(bad("invalid tensor layer option tag")),
        };
        tensors.push(PalwTirTensorEntryV1 { param, layer, offset: r.u64()?, bytes: r.u64()? });
    }
    let meta_len = r.length(1, limits.max_meta_bytes)?;
    std::str::from_utf8(&r.bytes[..meta_len]).map_err(|_| bad("metadata provenance is not UTF-8"))?;
    let mut meta = r.reserve(meta_len)?;
    meta.extend_from_slice(r.take(meta_len)?);
    let meta = String::from_utf8(meta).map_err(|_| bad("metadata provenance is not UTF-8"))?;
    r.finish()?;
    Ok(BoundedPalwTirHeaderV1 {
        header: PalwTirContainerHeaderV1 { version, program, layout, tokenizer_id, tensors, meta },
        file_len,
        header_len: frame,
    })
}

/// Read only the existing prefix and header through an already-open caller-owned
/// descriptor. File length must come from that descriptor, not a separate path.
/// The caller owns file mutation checks and retains the descriptor across later
/// authentication/reads; this function supplies no immutable-file guarantee.
pub fn read_header_bounded_v1(reader: &mut impl Read, file_len: u64, limits: PalwTirHeaderLimitsV1) -> Result<BoundedPalwTirHeaderV1> {
    cap(file_len, limits.max_file_bytes, "file byte ceiling")?;
    let mut prefix = [0u8; PREFIX as usize];
    reader.read_exact(&mut prefix)?;
    if &prefix[..8] != PALW_TIR_CONTAINER_MAGIC_V1
        || u16::from_le_bytes(prefix[8..10].try_into().expect("two bytes")) != PALW_TIR_CONTAINER_VERSION_V1
    {
        return Err(Error::Magic);
    }
    let header_len = u64::from(u32::from_le_bytes(prefix[10..14].try_into().expect("four bytes")));
    check_frame(header_len, file_len, limits)?; // Before even the encoded header allocation.
    let mut bytes = exact(usize::try_from(header_len).map_err(|_| bad("header frame native width"))?)?;
    bytes.resize(header_len as usize, 0);
    reader.read_exact(&mut bytes)?;
    decode_header_bounded_v1(&bytes, file_len, limits)
}

impl BoundedPalwTirHeaderV1 {
    pub fn header(&self) -> &PalwTirContainerHeaderV1 {
        &self.header
    }
    pub fn file_len(&self) -> u64 {
        self.file_len
    }
    pub fn layout(&self) -> Result<Option<PalwTirLayoutBytesV1<'_>>> {
        layout_view(&self.header.layout)
    }
    pub fn quote(&self, limits: PalwTirHeaderLimitsV1) -> Result<PalwTirHeaderQuoteV1> {
        let h = &self.header;
        cap(count(h.program.len())?, limits.max_program_bytes.min(MAX_PROGRAM_BYTES as u64), "program byte ceiling")?;
        cap(count(h.layout.len())?, limits.max_layout_bytes, "layout byte ceiling")?;
        cap(count(h.tensors.len())?, limits.max_tensors, "tensor count ceiling")?;
        cap(count(h.meta.len())?, limits.max_meta_bytes, "provenance byte ceiling")?;
        let copied = [
            count(h.program.capacity())?,
            count(h.layout.capacity())?,
            mul(count(h.tensors.capacity())?, count(size_of::<PalwTirTensorEntryV1>())?)?,
            count(h.meta.capacity())?,
        ]
        .into_iter()
        .try_fold(0, add)?;
        cap(copied, limits.max_copy_bytes, "metadata copy ceiling")?;
        let native = add(count(size_of::<Self>())?, copied)?;
        let work = check_frame(self.header_len, self.file_len, limits)?;
        let operation_peak_bytes = peak(self.header_len, native, limits)?;
        Ok(PalwTirHeaderQuoteV1 {
            header_bytes: self.header_len,
            native_bytes: native,
            copy_bytes: copied,
            work,
            operation_peak_bytes,
        })
    }

    /// Validate only byte equality and the existing tensor-table/file geometry
    /// against an independently canonical-decoded, normal-form-validated program.
    /// This is NOT another program validator: retain the program owner and the
    /// caller's separate reservation for all coexisting startup ownership.
    /// No program/layout/tensor copy or BTreeMap index is constructed here.
    pub fn validate_with_program<'a>(
        &'a self,
        program: &'a TirProgramV1,
        limits: PalwTirHeaderLimitsV1,
    ) -> Result<PalwTirCheckedHeaderV1<'a>> {
        let quote = self.quote(limits)?;
        if program.params.len() > MAX_PARAMS || program.blocks.len() > MAX_BLOCKS || program.schedule.layers.len() > MAX_LAYERS {
            return Err(bad("decoded program structural format ceiling"));
        }
        let mut refs = 0;
        let mut nodes = 0;
        for block in &program.blocks {
            if block.nodes.len() > MAX_NODES_PER_BLOCK {
                return Err(bad("decoded node count format ceiling"));
            }
            nodes = add(nodes, count(block.nodes.len())?)?;
            for node in &block.nodes {
                if node.inputs.len() > MAX_NODE_INPUTS {
                    return Err(bad("decoded input count format ceiling"));
                }
                refs = add(refs, count(node.inputs.len())?)?;
            }
        }
        let scans =
            mul(mul(count(program.params.len())?, add(count(program.schedule.layers.len())?, 1)?)?, add(add(nodes, refs)?, 1)?)?;
        cap(add(quote.work, mul(add(scans, 1)?, 128)?)?, limits.max_work, "split validation work ceiling")?;
        if borsh::object_length(program).map_err(|_| bad("program count failed"))? != self.header.program.len() {
            return Err(Error::Program("decoded original differs from header program bytes".into()));
        }
        struct Compare<'a>(&'a [u8]);
        impl Write for Compare<'_> {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if !self.0.starts_with(bytes) {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "program bytes differ"));
                }
                self.0 = &self.0[bytes.len()..];
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut compare = Compare(&self.header.program);
        if program.serialize(&mut compare).is_err() || !compare.0.is_empty() {
            return Err(Error::Program("decoded original differs from header program bytes".into()));
        }
        let mut cursor = 0usize;
        let mut end = add(PREFIX, self.header_len)?;
        for (j, declaration) in program.params.iter().enumerate() {
            let j = u16::try_from(j).map_err(|_| bad("parameter index width"))?;
            let bytes = mul(declaration.shape.iter().try_fold(1, |n, d| mul(n, u64::from(*d)))?, count(declaration.dtype.width())?)?;
            let mut check = |layer| -> Result<()> {
                let entry =
                    self.header.tensors.get(cursor).ok_or_else(|| Error::Table("missing original parameter instance".into()))?;
                if entry.param != j || entry.layer != layer || entry.bytes != bytes {
                    return Err(Error::Table("tensor order/type geometry differs from original program".into()));
                }
                if !entry.offset.is_multiple_of(PALW_TIR_TENSOR_ALIGN_V1) || entry.offset < end {
                    return Err(Error::Table("misaligned or overlapping tensor extent".into()));
                }
                end = add(entry.offset, entry.bytes)?;
                if end > self.file_len {
                    return Err(Error::Table("tensor runs past file end".into()));
                }
                cursor = cursor.checked_add(1).ok_or_else(|| bad("tensor index overflow"))?;
                Ok(())
            };
            if declaration.per_layer {
                for (layer, block) in program.schedule.layers.iter().enumerate() {
                    let block = program.blocks.get(*block as usize).ok_or_else(|| bad("caller program schedule is not validated"))?;
                    if block.nodes.iter().any(|node| node.inputs.contains(&Ref::Param(j))) {
                        check(Some(u16::try_from(layer).map_err(|_| bad("layer index width"))?))?;
                    }
                }
            } else {
                check(None)?;
            }
        }
        if cursor != self.header.tensors.len() {
            return Err(Error::Table("extra tensor instances".into()));
        }
        Ok(PalwTirCheckedHeaderV1 { metadata: self, program })
    }
}

/// Allocation-free checked metadata view. It does not carry class/root/context
/// authentication; the node source owner must establish and retain those pins.
pub struct PalwTirCheckedHeaderV1<'a> {
    metadata: &'a BoundedPalwTirHeaderV1,
    program: &'a TirProgramV1,
}
impl PalwTirCheckedHeaderV1<'_> {
    pub fn header(&self) -> &PalwTirContainerHeaderV1 {
        self.metadata.header()
    }
    pub fn program(&self) -> &TirProgramV1 {
        self.program
    }
    pub fn file_len(&self) -> u64 {
        self.metadata.file_len
    }
    pub fn locate(&self, param: u16, layer: Option<u16>) -> Option<(u64, u64)> {
        let entries = &self.metadata.header.tensors;
        let i = entries.binary_search_by_key(&(param, layer), |e| (e.param, e.layer)).ok()?;
        Some((entries[i].offset, entries[i].bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn limits() -> PalwTirHeaderLimitsV1 {
        PalwTirHeaderLimitsV1 {
            max_file_bytes: 1 << 24,
            max_header_bytes: 1 << 20,
            max_program_bytes: MAX_PROGRAM_BYTES as u64,
            max_layout_bytes: 1 << 16,
            max_tensors: 4096,
            max_meta_bytes: 4096,
            max_native_bytes: 1 << 20,
            max_copy_bytes: 1 << 20,
            max_work: 1 << 32,
            max_operation_bytes: 1 << 24,
            external_retained_bytes: 4096,
        }
    }
    fn header(program: Vec<u8>) -> PalwTirContainerHeaderV1 {
        PalwTirContainerHeaderV1 {
            version: 1,
            program,
            layout: Vec::new(),
            tokenizer_id: [18; 64],
            tensors: Vec::new(),
            meta: "literal provenance".into(),
        }
    }
    fn decode(h: &PalwTirContainerHeaderV1, l: PalwTirHeaderLimitsV1) -> Result<BoundedPalwTirHeaderV1> {
        let bytes = borsh::to_vec(h).unwrap();
        decode_header_bounded_v1(&bytes, PREFIX + bytes.len() as u64, l)
    }
    #[test]
    fn exact_native_copy_and_joint_peak_refuse_one_below() {
        let h = header(vec![9; 17]);
        let value = decode(&h, limits()).unwrap();
        let quote = value.quote(limits()).unwrap();
        let mut exact = limits();
        exact.max_native_bytes = quote.native_bytes;
        exact.max_copy_bytes = quote.copy_bytes;
        exact.max_operation_bytes = quote.operation_peak_bytes;
        assert_eq!(decode(&h, exact).unwrap().header(), &h);
        for field in 0..3 {
            let mut short = exact;
            match field {
                0 => short.max_native_bytes -= 1,
                1 => short.max_copy_bytes -= 1,
                _ => short.max_operation_bytes -= 1,
            }
            assert!(decode(&h, short).is_err());
        }
    }
    #[test]
    fn hostile_header_lengths_and_prefix_refuse_before_owned_payload_request() {
        let mut bytes = vec![1, 0];
        bytes.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_header_bounded_v1(&bytes, PREFIX + bytes.len() as u64, limits()).is_err());
        let mut prefix = PALW_TIR_CONTAINER_MAGIC_V1.to_vec();
        prefix.extend_from_slice(&1u16.to_le_bytes());
        prefix.extend_from_slice(&u32::MAX.to_le_bytes());
        let mut input = io::Cursor::new(prefix);
        assert!(read_header_bounded_v1(&mut input, u64::from(u32::MAX) + PREFIX, limits()).is_err());
        assert_eq!(input.position(), 0); // File cap checked before even reading prefix.
        let mut cap = limits();
        cap.max_file_bytes = u64::MAX;
        input.set_position(0);
        assert!(read_header_bounded_v1(&mut input, u64::from(u32::MAX) + PREFIX, cap).is_err());
        assert_eq!(input.position(), PREFIX); // Header cap refuses before body allocation/read.
        let good = borsh::to_vec(&header(vec![])).unwrap();
        for end in 0..good.len() {
            assert!(decode_header_bounded_v1(&good[..end], PREFIX + good.len() as u64, limits()).is_err());
        }
    }
    #[test]
    fn empty_table_borrows_already_decoded_program_without_redecode_or_defaults() {
        use misaka_palw_tir::{DType, builder::ProgramBuilder};
        let mut p = ProgramBuilder::new(4, 1 << 18);
        let mut pre = p.block("pre", vec![]);
        let token = pre.cast(Ref::Input(0), DType::I32);
        pre.commit(token); // Normal form requires a nonempty live pre block.
        let pre = pre.finish(&[]);
        let mut post = p.block("post", vec![]);
        let value = post.cast(Ref::Input(0), DType::I32);
        let value = post.reshape_fixed(value, &[1]);
        post.commit(value);
        let Ref::Node(logits) = value else { unreachable!() };
        let post = post.finish(&[]);
        let program = p.finish(pre, vec![], post, logits);
        // Production uses an independent bounded program owner plus the caller's
        // separate startup reservation, rather than this test-only decode.
        let bytes = program.encode();
        let program = TirProgramV1::decode_canonical(&bytes).unwrap();
        let original = header(bytes);
        let metadata = decode(&original, limits()).unwrap();
        let checked = metadata.validate_with_program(&program, limits()).unwrap();
        assert_eq!(checked.header(), &original);
        assert!(checked.locate(0, None).is_none());
        assert!(metadata.layout().unwrap().is_none());
        let mut changed = program.clone();
        changed.token_bound += 1;
        assert!(metadata.validate_with_program(&changed, limits()).is_err());
    }

    #[test]
    fn nonempty_table_matches_handwritten_instances_and_refuses_bad_extents() {
        use misaka_palw_tir::{DType, builder::ProgramBuilder};
        let mut p = ProgramBuilder::new(4, 1 << 18);
        let pre_only = p.param("pre.only.global", DType::I8, &[3], false); // index0, not read by layer/post.
        let per_layer = p.param("layer.weights", DType::I16, &[2], true); // index1.
        let head = p.param("head.weights", DType::I16, &[64], false); // index2, 128 actual bytes.
        let mut pre = p.block("pre", vec![]);
        let first = pre.cast(pre_only, DType::I32);
        pre.commit(first);
        let pre = pre.finish(&[]);
        let mut layer = p.block("layer", vec![]);
        // Two reads in the SAME block do not duplicate a parameter instance.
        let sum = layer.add(per_layer, per_layer, DType::I32);
        layer.commit(sum);
        let layer = layer.finish(&[]);
        let mut post = p.block("post", vec![]);
        let value = post.cast(head, DType::I32);
        post.commit(value);
        let Ref::Node(logits) = value else { unreachable!() };
        let post = post.finish(&[]);
        // The same layer block occurs twice, at distinct original layer indices.
        let program = p.finish(pre, vec![layer, layer], post, logits);
        let bytes = program.encode();
        let program = TirProgramV1::decode_canonical(&bytes).unwrap();
        // A genuinely unused declaration is NOT normal form. Reject it at
        // canonical decode, never feed it into the split API's validated-owner
        // precondition as a supposedly valid original program.
        let mut unused = program.clone();
        unused.params.push(misaka_palw_tir::program::ParamDecl {
            name: "genuinely.unused".into(),
            dtype: DType::I8,
            shape: vec![3],
            per_layer: false,
        });
        assert!(TirProgramV1::decode_canonical(&unused.encode()).is_err());
        let mut original = header(bytes);
        // Deliberately hand-written, independent of param_instances_v1 and of
        // validate_with_program's iteration. Keep aligned holes as the old
        // format permits; no tensor bytes or model execution are needed here.
        original.tensors = vec![
            PalwTirTensorEntryV1 { param: 0, layer: None, offset: 4096, bytes: 3 },
            PalwTirTensorEntryV1 { param: 1, layer: Some(0), offset: 4160, bytes: 4 },
            PalwTirTensorEntryV1 { param: 1, layer: Some(1), offset: 4224, bytes: 4 },
            PalwTirTensorEntryV1 { param: 2, layer: None, offset: 4288, bytes: 128 },
        ];
        const FILE_LEN: u64 = 4480; // last original extent ends at4416, trailing padding is permitted.
        let parse = |h: &PalwTirContainerHeaderV1| {
            let body = borsh::to_vec(h).unwrap();
            assert!(PREFIX + body.len() as u64 <= 4096, "literal first offset follows the actual header");
            decode_header_bounded_v1(&body, FILE_LEN, limits()).unwrap()
        };
        let metadata = parse(&original);
        let checked = metadata.validate_with_program(&program, limits()).unwrap();
        assert!(std::ptr::eq(checked.program(), &program));
        assert_eq!(checked.header(), &original);
        for (param, layer, expected) in [
            (0, None, Some((4096, 3))),
            (1, Some(0), Some((4160, 4))),
            (1, Some(1), Some((4224, 4))),
            (2, None, Some((4288, 128))),
            (0, Some(0), None),
            (1, None, None),
            (1, Some(2), None),
            (3, None, None),
        ] {
            assert_eq!(checked.locate(param, layer), expected);
        }
        for attack in 0..11 {
            let mut changed = original.clone();
            let reason = match attack {
                0 => {
                    changed.tensors.remove(0);
                    "omitted pre-only global None"
                }
                1 => {
                    changed.tensors.remove(2);
                    "omitted repeated-block layer1"
                }
                2 => {
                    changed.tensors.push(PalwTirTensorEntryV1 { param: 3, layer: None, offset: 4416, bytes: 0 });
                    "extra undeclared instance"
                }
                3 => {
                    changed.tensors.swap(1, 2);
                    "reordered layer instances"
                }
                4 => {
                    changed.tensors[0].layer = Some(0);
                    "global instance incorrectly given a layer"
                }
                5 => {
                    changed.tensors[2].layer = Some(2);
                    "wrong repeated-block layer index"
                }
                6 => {
                    changed.tensors[3].bytes = 127;
                    "wrong declared byte length"
                }
                7 => {
                    changed.tensors[2].offset = 4160;
                    "aligned overlapping extents"
                }
                8 => {
                    changed.tensors[2].offset = 4225;
                    "unaligned extent"
                }
                9 => {
                    // Largest aligned u64 offset + the ORIGINAL128 bytes overflows.
                    changed.tensors[3].offset = u64::MAX - 63;
                    "checked extent addition overflow"
                }
                _ => {
                    changed.tensors[3].offset = 4416;
                    "aligned extent past actual file end"
                }
            };
            let metadata = parse(&changed);
            assert!(metadata.validate_with_program(&program, limits()).is_err(), "accepted {reason}");
            // Parsing preserves the literal false evidence; validation refuses it.
            assert_eq!(metadata.header(), &changed);
        }
    }

    #[test]
    fn layout_is_borrowed_canonical_existing_words_with_no_generic_vec_decode() {
        let mut h = header(vec![]);
        h.layout.extend_from_slice(&1u16.to_le_bytes());
        for v in [16u32, 3, 2, 2, 4, 7, 1, 9] {
            h.layout.extend_from_slice(&v.to_le_bytes());
        }
        let m = decode(&h, limits()).unwrap();
        let layout = m.layout().unwrap().unwrap();
        assert_eq!((layout.max_context, layout.checkpoint_interval, layout.h_tile), (16, 3, 2));
        assert_eq!(layout.commit_tiles().collect::<Vec<_>>(), vec![4, 7]);
        assert_eq!(layout.state_tiles().collect::<Vec<_>>(), vec![9]);
        h.layout[14..18].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode(&h, limits()).is_err());
    }
}
