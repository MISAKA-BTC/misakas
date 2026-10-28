//! Allocation-free Borsh shape preflight. The scanner reads the actual stream;
//! no peer-provided resource estimate or encoded-byte multiplier is authority.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Read},
    mem::size_of,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeWireShapeV1 {
    pub heap: u64,
    pub collection_entries: u64,
    pub outer_entries: u64,
    /// Actual byte-vector elements within this parsed codec, distinct from frame bytes.
    pub byte_entries: u64,
    /// Actual composite-stage references parsed from versioned spec roots.
    pub composite_components: u64,
}
impl NativeWireShapeV1 {
    pub fn merge(self, rhs: Self) -> io::Result<Self> {
        Ok(Self {
            heap: self.heap.checked_add(rhs.heap).ok_or_else(overflow)?,
            collection_entries: self.collection_entries.checked_add(rhs.collection_entries).ok_or_else(overflow)?,
            outer_entries: self.outer_entries.max(rhs.outer_entries),
            byte_entries: self.byte_entries.checked_add(rhs.byte_entries).ok_or_else(overflow)?,
            composite_components: self.composite_components.checked_add(rhs.composite_components).ok_or_else(overflow)?,
        })
    }
    pub fn allocation(mut self, count: u64, unit: usize) -> io::Result<Self> {
        self.heap = self.heap.checked_add(count.checked_mul(unit as u64).ok_or_else(overflow)?).ok_or_else(overflow)?;
        Ok(self)
    }
}
fn overflow() -> io::Error {
    io::Error::new(io::ErrorKind::OutOfMemory, "native Borsh shape arithmetic overflow")
}
pub(crate) fn wire_bad(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why)
}

pub struct NativeWireReaderV1<R> {
    inner: R,
    remaining: u64,
    steps: u64,
    step_limit: u64,
}
impl<R: Read> NativeWireReaderV1<R> {
    pub fn new(inner: R, bytes: u64) -> Self {
        // CPU-only admission bound; zero-sized sequences are not allowed to
        // force billions of iterations from a four-byte length word.
        Self { inner, remaining: bytes, steps: 0, step_limit: bytes.saturating_mul(8).saturating_add(4096) }
    }
    pub fn remaining(&self) -> u64 {
        self.remaining
    }
    pub fn into_inner(self) -> R {
        self.inner
    }
    pub fn step(&mut self) -> io::Result<()> {
        self.steps = self.steps.checked_add(1).ok_or_else(overflow)?;
        if self.steps > self.step_limit {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "native Borsh shape CPU budget exceeded"));
        }
        Ok(())
    }
    pub fn skip(&mut self, bytes: u64) -> io::Result<()> {
        if bytes > self.remaining {
            return Err(wire_bad("native Borsh field exceeds remaining frame"));
        }
        if bytes <= 64 {
            let mut scratch = [0u8; 64];
            self.read_exact(&mut scratch[..bytes as usize])?;
            return Ok(());
        }
        let mut scratch = [0u8; 8192];
        let mut left = bytes;
        while left != 0 {
            let n = left.min(scratch.len() as u64) as usize;
            self.read_exact(&mut scratch[..n])?;
            left -= n as u64;
        }
        Ok(())
    }
    pub fn byte(&mut self) -> io::Result<u8> {
        let mut value = [0];
        self.read_exact(&mut value)?;
        Ok(value[0])
    }
    pub fn word(&mut self) -> io::Result<u32> {
        let mut value = [0; 4];
        self.read_exact(&mut value)?;
        Ok(u32::from_le_bytes(value))
    }
    pub fn short(&mut self) -> io::Result<u16> {
        let mut value = [0; 2];
        self.read_exact(&mut value)?;
        Ok(u16::from_le_bytes(value))
    }
}
impl<R: Read> Read for NativeWireReaderV1<R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let max = bytes.len().min(self.remaining.min(usize::MAX as u64) as usize);
        if max == 0 {
            return Ok(0);
        }
        let n = self.inner.read(&mut bytes[..max])?;
        self.remaining -= n as u64;
        Ok(n)
    }
}
pub trait NativeWireScanV1: Sized {
    fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1>;
}
macro_rules! wire_fixed { ($($ty:ty => $bytes:expr),* $(,)?) => {$ (
    impl NativeWireScanV1 for $ty {
        fn scan_native_wire_v1<R:Read>(reader:&mut NativeWireReaderV1<R>)->io::Result<NativeWireShapeV1> {
            reader.step()?;reader.skip($bytes)?;Ok(NativeWireShapeV1::default())
        }
    }
)*}; }
wire_fixed!(u8=>1,u16=>2,u32=>4,u64=>8,u128=>16,usize=>8,isize=>8,i8=>1,i16=>2,i32=>4,i64=>8,i128=>16,f32=>4,f64=>8,()=>0,
 crate::Hash64=>64,kaspa_hashes::Hash=>32,kaspa_math::Uint512=>64,kaspa_math::Uint576=>72);
impl NativeWireScanV1 for bool {
    fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        reader.step()?;
        if reader.byte()? > 1 {
            return Err(wire_bad("invalid Borsh boolean"));
        }
        Ok(NativeWireShapeV1::default())
    }
}
impl<T: NativeWireScanV1, const N: usize> NativeWireScanV1 for [T; N] {
    fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        if std::any::type_name::<T>() == "u8" {
            reader.step()?;
            reader.skip(N as u64)?;
            return Ok(NativeWireShapeV1::default());
        }
        let mut sum = NativeWireShapeV1::default();
        for _ in 0..N {
            sum = sum.merge(T::scan_native_wire_v1(reader)?)?;
        }
        Ok(sum)
    }
}
impl<T: NativeWireScanV1> NativeWireScanV1 for Option<T> {
    fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => Ok(NativeWireShapeV1::default()),
            1 => T::scan_native_wire_v1(reader),
            _ => Err(wire_bad("invalid Borsh option")),
        }
    }
}
impl<T: NativeWireScanV1> NativeWireScanV1 for Box<T> {
    fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        T::scan_native_wire_v1(reader)?.allocation(1, size_of::<T>())
    }
}
impl<T: NativeWireScanV1> NativeWireScanV1 for Vec<T> {
    fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        reader.step()?;
        let count = reader.word()? as u64;
        if std::any::type_name::<T>() == "u8" {
            reader.skip(count)?;
            return NativeWireShapeV1 { outer_entries: count, byte_entries: count, ..Default::default() }
                .allocation(count.checked_mul(2).ok_or_else(overflow)?, 1);
        }
        if size_of::<T>() == 0 && count != 0 {
            return Err(io::Error::new(io::ErrorKind::Unsupported, "Borsh zero-size sequence is unsupported"));
        }

        let mut sum = NativeWireShapeV1::default();
        for _ in 0..count {
            sum = sum.merge(T::scan_native_wire_v1(reader)?)?;
        }
        // Pinned Borsh Vec deserialization uses a bounded initial hint followed by
        // geometric Vec growth; twice actual count covers the requested capacity.
        // This is a typed element/capacity term, never bytes-to-RSS scaling.
        sum = sum.allocation(count.checked_mul(2).ok_or_else(overflow)?, size_of::<T>())?;
        if std::any::type_name::<T>() != "u8" {
            sum.collection_entries = sum.collection_entries.checked_add(count).ok_or_else(overflow)?;
        }
        sum.outer_entries = count;
        Ok(sum)
    }
}
// The Borsh tree decoders first read a Vec, then collect its entries into a tree.
// Include both allocations concurrently, even where the implementation could
// free a consumed Vec early. One conservative node envelope per entry.
impl<K: NativeWireScanV1, V: NativeWireScanV1> NativeWireScanV1 for BTreeMap<K, V> {
    fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        let shape = Vec::<(K, V)>::scan_native_wire_v1(reader)?;
        shape.allocation(shape.outer_entries, size_of::<super::typed::BTreeNodeEnvelopeV1<K, V>>())
    }
}
impl<T: NativeWireScanV1> NativeWireScanV1 for BTreeSet<T> {
    fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        let shape = Vec::<T>::scan_native_wire_v1(reader)?;
        shape.allocation(shape.outer_entries, size_of::<super::typed::BTreeNodeEnvelopeV1<T, ()>>())
    }
}
impl NativeWireScanV1 for String {
    fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        let bytes = reader.word()? as u64;
        reader.skip(bytes)?;
        // String restores from Borsh's byte Vec. Match the existing typed
        // String/Vec geometric-capacity envelope; do not underquote names in
        // nested artifact openings. This changes requests, never bytes or caps.
        NativeWireShapeV1::default().allocation(bytes.checked_mul(2).ok_or_else(overflow)?, 1)
    }
}
macro_rules! wire_tuple { ($($ty:ident),+) => {
 impl<$($ty:NativeWireScanV1),+> NativeWireScanV1 for ($($ty,)+) {
  fn scan_native_wire_v1<R:Read>(reader:&mut NativeWireReaderV1<R>)->io::Result<NativeWireShapeV1> {
   let mut sum=NativeWireShapeV1::default();$(sum=sum.merge($ty::scan_native_wire_v1(reader)?)?;)+Ok(sum)
  }
 }
}; }
wire_tuple!(A);
wire_tuple!(A, B);
wire_tuple!(A, B, C);
wire_tuple!(A, B, C, D);
wire_tuple!(A, B, C, D, E);
wire_tuple!(A, B, C, D, E, F);
wire_tuple!(A, B, C, D, E, F, G);

// A field may carry its `#[cfg(..)]` (a field that exists, and is encoded, only in that build).
macro_rules! wire_struct { ($ty:ty; $($(#[$attr:meta])* $field:ty),* $(,)?) => {
 impl $crate::palw_native_resource_v1::NativeWireScanV1 for $ty {
  fn scan_native_wire_v1<R:std::io::Read>(reader:&mut $crate::palw_native_resource_v1::NativeWireReaderV1<R>)->std::io::Result<$crate::palw_native_resource_v1::NativeWireShapeV1> {
    reader.step()?;let mut sum=$crate::palw_native_resource_v1::NativeWireShapeV1::default();
    $($(#[$attr])* { sum=sum.merge(<$field as $crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(reader)?)?; })*Ok(sum)
  }
 }
}; }
pub(crate) use wire_struct;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recursive_shape_counts_actual_lengths_and_truncation() {
        let mut rows = BTreeMap::new();
        rows.insert(1u64, vec![7u8; 31]);
        let bytes = borsh::to_vec(&rows).unwrap();
        let mut reader = NativeWireReaderV1::new(bytes.as_slice(), bytes.len() as u64);
        let shape = BTreeMap::<u64, Vec<u8>>::scan_native_wire_v1(&mut reader).unwrap();
        assert_eq!(shape.outer_entries, 1);
        assert_eq!(reader.remaining(), 0);
        assert!(shape.heap >= 62 + size_of::<super::super::typed::BTreeNodeEnvelopeV1<u64, Vec<u8>>>() as u64);
        let mut reader = NativeWireReaderV1::new(&bytes[..bytes.len() - 1], bytes.len() as u64);
        assert!(BTreeMap::<u64, Vec<u8>>::scan_native_wire_v1(&mut reader).is_err());
    }
}

impl NativeWireScanV1 for misaka_palw_panel::PanelCandidatesV1 {
    fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        Vec::<misaka_palw_panel::SeatCandidateV1>::scan_native_wire_v1(reader)?
            .allocation(1, size_of::<Vec<misaka_palw_panel::SeatCandidateV1>>() + size_of::<[usize; 3]>())
    }
}
// Match the existing typed Arc ownership account, including its conservative
// alignment allowance. This reserves memory, never changes a codec or a cap.
fn arc_wire_allocation_v1<T>(shape: NativeWireShapeV1) -> io::Result<NativeWireShapeV1> {
    let bytes = size_of::<T>()
        .checked_add(size_of::<[usize; 2]>())
        .and_then(|n| n.checked_add(std::mem::align_of::<T>()))
        .ok_or_else(overflow)?;
    shape.allocation(1, bytes)
}
/// G14's shared court continuation: Borsh is exactly `T`; restore allocates one `Arc<T>` (two counters plus the payload).
impl<T: NativeWireScanV1> NativeWireScanV1 for crate::palw_staged_court_state_v1::PalwSharedCourtV1<T> {
    fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        arc_wire_allocation_v1::<T>(T::scan_native_wire_v1(reader)?)
    }
}
/// Retained own-state authority is a runtime reservation, not a wire codec.
/// Serializing its inner observation does not let a peer recreate that owner.
impl<T> NativeWireScanV1 for crate::palw_native_resource_v1::NativeRetainedV1<T> {
    fn scan_native_wire_v1<R: Read>(_reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        Err(wire_bad("retained own-state context cannot be reconstructed from wire"))
    }
}
/// A shared `Arc<T>` field: Borsh is exactly `T`; restore allocates one `Arc<T>` (two counters plus the payload).
impl<T: NativeWireScanV1> NativeWireScanV1 for std::sync::Arc<T> {
    fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        arc_wire_allocation_v1::<T>(T::scan_native_wire_v1(reader)?)
    }
}
impl NativeWireScanV1 for crate::palw_model_market_v1::PalwModelMarketV1 {
    fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        reader.skip(6 * 8)?;
        let flags = reader.byte()?;
        if flags & !3 != 0 {
            return Err(wire_bad("unknown model-market flag"));
        }
        reader.skip(2 * 8 + 64 + 2 * 8)?;
        if flags & 2 != 0 {
            reader.skip(8)?;
        }
        Ok(NativeWireShapeV1::default())
    }
}

/// Constructed only by an actual schema walk; not deserializable peer metadata.
pub struct NativeWirePreflightV1 {
    shape: NativeWireShapeV1,
    restoration: NativeWireShapeV1,
    bytes: u64,
}
impl NativeWirePreflightV1 {
    pub fn memory_requests_v1(&self) -> io::Result<u64> {
        let native = self
            .shape
            .heap
            .checked_add(self.restoration.heap)
            .ok_or_else(overflow)?
            .checked_add(size_of::<crate::palw_state_v2::PalwChainStateV2>() as u64)
            .ok_or_else(overflow)?
            .checked_add(size_of::<crate::palw_state_v2::PalwStateCarriageV2>() as u64)
            .ok_or_else(overflow)?;
        let root_scratch = self.shape.collection_entries.checked_mul(3 * size_of::<crate::Hash64>() as u64).ok_or_else(overflow)?;
        // Separate actual collection/copy terms and known encoded byte Vecs.
        native
            .checked_mul(5)
            .and_then(|n| self.bytes.checked_mul(6).and_then(|b| n.checked_add(b)))
            .and_then(|n| n.checked_add(root_scratch))
            .and_then(|n| n.checked_add((8 << 20) + (256 << 10)))
            .ok_or_else(overflow)
    }
}
/// Scan one exact carriage frame, including every optional tagged tail, and
/// explicitly size the private native tables its restore rebuilds.
pub fn preflight_native_carriage_v1(reader: impl Read, bytes: u64) -> io::Result<NativeWirePreflightV1> {
    if cfg!(not(target_pointer_width = "64")) {
        return Err(io::Error::new(io::ErrorKind::Unsupported, "native BTree wire layout profile only covers 64-bit targets"));
    }
    let mut reader = NativeWireReaderV1::new(reader, bytes);
    let (shape, counts) = crate::palw_state_v2::scan_carriage_native_wire_v1(&mut reader)?;
    if reader.remaining() != 0 {
        return Err(wire_bad("native carriage preflight frame did not end exactly"));
    }
    let restoration = crate::palw_state_v2::restore_index_native_shape_v1(&counts)?;
    Ok(NativeWirePreflightV1 { shape, restoration, bytes })
}
/// Scan the exact framed state and every retained class declaration before
/// native Borsh decoding. Class bytes are parsed using their real admission
/// type, rather than accepting an opaque length as its native demand.
pub fn preflight_native_snapshot_v1(reader: impl Read, total: u64) -> io::Result<NativeWirePreflightV1> {
    let mut reader = NativeWireReaderV1::new(reader, total);
    if reader.short()? != 1 {
        return Err(wire_bad("unknown native snapshot envelope"));
    }
    let mut state_bytes = [0u8; 8];
    reader.read_exact(&mut state_bytes)?;
    let state_bytes = u64::from_le_bytes(state_bytes);
    if state_bytes == 0 || state_bytes > reader.remaining().saturating_sub(4) {
        return Err(wire_bad("native state frame exceeds snapshot"));
    }
    let mut preflight = preflight_native_carriage_v1((&mut reader).take(state_bytes), state_bytes)?;
    let count = reader.word()? as u64;
    if count > reader.remaining() / 68 {
        return Err(wire_bad("native declaration count exceeds remaining snapshot"));
    }
    preflight.shape = preflight.shape.allocation(count.checked_mul(2).ok_or_else(overflow)?, size_of::<(crate::Hash64, Vec<u8>)>())?;
    for _ in 0..count {
        reader.skip(64)?;
        let bytes = reader.word()? as u64;
        if bytes > reader.remaining() {
            return Err(wire_bad("native class declaration exceeds remaining snapshot"));
        }
        let mut frame = NativeWireReaderV1::new((&mut reader).take(bytes), bytes);
        let shape = crate::palw_state_v2::PalwClassAdmissionCarriageV2::scan_native_wire_v1(&mut frame)?;
        if frame.remaining() != 0 {
            return Err(wire_bad("native class admission has trailing/mismatched bytes"));
        }
        preflight.shape = preflight.shape.merge(shape.allocation(bytes.checked_mul(2).ok_or_else(overflow)?, 1)?)?;
    }
    if reader.remaining() != 0 {
        return Err(wire_bad("native snapshot shape has trailing bytes"));
    }
    preflight.bytes = total;
    Ok(preflight)
}

#[cfg(test)]
mod carriage_preflight_tests {
    use super::*;
    use crate::palw_native_resource_v1::NativeHeapV1;
    #[test]
    fn shared_arc_preflight_covers_typed_heap_without_vector_slack() {
        fn check<T: NativeWireScanV1 + NativeHeapV1 + borsh::BorshSerialize + Clone>(value: T) {
            let raw = borsh::to_vec(&value).unwrap();
            let shared: crate::palw_staged_court_state_v1::PalwSharedCourtV1<T> = Box::new(value.clone()).into();
            let arc = std::sync::Arc::new(value);
            assert_eq!(borsh::to_vec(&shared).unwrap(), raw);
            assert_eq!(borsh::to_vec(&arc).unwrap(), raw);
            let mut scan = NativeWireReaderV1::new(raw.as_slice(), raw.len() as u64);
            let shared_shape = <crate::palw_staged_court_state_v1::PalwSharedCourtV1<T>>::scan_native_wire_v1(&mut scan).unwrap();
            assert_eq!(scan.remaining(), 0);
            assert!(shared_shape.heap >= shared.native_heap_v1("shared").bytes().unwrap());
            let mut scan = NativeWireReaderV1::new(raw.as_slice(), raw.len() as u64);
            let arc_shape = <std::sync::Arc<T>>::scan_native_wire_v1(&mut scan).unwrap();
            assert_eq!(scan.remaining(), 0);
            assert!(arc_shape.heap >= arc.native_heap_v1("arc").bytes().unwrap());
        }
        check(7u64);
        check(7u8);
        check([0u8; 0]);
    }
    #[test]
    fn scanner_resource_limits_are_distinct_from_malformed_codec() {
        assert_eq!(NativeWireShapeV1::default().allocation(u64::MAX, 16).unwrap_err().kind(), io::ErrorKind::OutOfMemory);
        let mut reader = NativeWireReaderV1::new(&[][..], 0);
        for _ in 0..4096 {
            reader.step().unwrap();
        }
        assert_eq!(reader.step().unwrap_err().kind(), io::ErrorKind::TimedOut);
        let mut malformed = NativeWireReaderV1::new(&[2u8][..], 1);
        assert_eq!(bool::scan_native_wire_v1(&mut malformed).unwrap_err().kind(), io::ErrorKind::InvalidData);
    }
    #[test]
    fn exact_carriage_preflight_bounds_actual_heap_and_rejects_truncated_tagged_data() {
        let state = crate::palw_state_v2::PalwChainStateV2::genesis();
        let carriage = crate::palw_state_v2::PalwStateCarriageV2::from_state(&state);
        let bytes = borsh::to_vec(&carriage).unwrap();
        let scan = preflight_native_carriage_v1(bytes.as_slice(), bytes.len() as u64).unwrap();
        assert!(scan.shape.heap >= carriage.native_heap_v1("actual").bytes().unwrap());
        assert!(scan.memory_requests_v1().unwrap() >= scan.shape.heap);
        for length in [0, 1, bytes.len() - 1] {
            assert!(preflight_native_carriage_v1(&bytes[..length], bytes.len() as u64).is_err());
        }
        let mut bad = bytes.clone();
        bad[0] ^= 1;
        assert!(preflight_native_carriage_v1(bad.as_slice(), bad.len() as u64).is_err());
        let mut bomb = bytes.clone();
        bomb[2..6].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(preflight_native_carriage_v1(bomb.as_slice(), bomb.len() as u64).is_err());
    }
    #[test]
    fn explicit_nonzero_enum_discriminants_match_real_borsh_layout() {
        use crate::palw_class_semantics_v1::PalwPrefixStateRelationV1 as Relation;
        for value in [Relation::DenseInt32KvV1, Relation::Qwen36HybridCacheV1, Relation::Base0Int8KvV1] {
            let bytes = borsh::to_vec(&value).unwrap();
            let mut reader = NativeWireReaderV1::new(bytes.as_slice(), bytes.len() as u64);
            Relation::scan_native_wire_v1(&mut reader).unwrap();
            assert_eq!(reader.remaining(), 0);
        }
        let mut reader = NativeWireReaderV1::new([0u8].as_slice(), 1);
        assert!(Relation::scan_native_wire_v1(&mut reader).is_err());
    }
    #[test]
    fn snapshot_unknown_codec_and_declaration_shape_cannot_skip_preallocation_scan() {
        let state = crate::palw_state_v2::PalwStateCarriageV2::from_state(&crate::palw_state_v2::PalwChainStateV2::genesis());
        let bytes = borsh::to_vec(&state).unwrap();
        let mut frame = Vec::new();
        frame.extend_from_slice(&1u16.to_le_bytes());
        frame.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        frame.extend_from_slice(&bytes);
        frame.extend_from_slice(&0u32.to_le_bytes());
        assert!(preflight_native_snapshot_v1(frame.as_slice(), frame.len() as u64).is_ok());
        let end = frame.len();
        frame[end - 4..].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(preflight_native_snapshot_v1(frame.as_slice(), frame.len() as u64).is_err());
    }
}

/// A standalone canonical row's allocation proof. Fields are private and it is
/// not deserializable, so a peer cannot replace the schema walk with metadata.
pub struct NativeWireValuePreflightV1 {
    requests: u64,
    canonical: u64,
}
impl NativeWireValuePreflightV1 {
    pub fn logical_requests_v1(&self) -> u64 {
        self.requests
    }
    pub fn canonical_bytes_v1(&self) -> u64 {
        self.canonical
    }
}
pub fn preflight_native_value_v1<T: NativeWireScanV1>(reader: impl Read, bytes: u64) -> io::Result<NativeWireValuePreflightV1> {
    if cfg!(not(target_pointer_width = "64")) {
        return Err(io::Error::new(io::ErrorKind::Unsupported, "standalone native tree shape profile requires 64-bit target"));
    }
    let mut reader = NativeWireReaderV1::new(reader, bytes);
    let shape = T::scan_native_wire_v1(&mut reader)?;
    if reader.remaining() != 0 {
        return Err(wire_bad("standalone native row has trailing/mismatched bytes"));
    }
    let requests = shape.heap.checked_add(size_of::<T>() as u64).ok_or_else(overflow)?;
    Ok(NativeWireValuePreflightV1 { requests, canonical: bytes })
}

/// LeafOpeningV3 has a handwritten wire codec: values use dtype.width(),
/// while the owning Rust Vec stores i128. A derived Vec<i128> scan would read
/// the wrong bytes for every narrow dtype and fail to reserve its expansion.
fn leaf_opening_shape_v3<R: Read>(reader: &mut NativeWireReaderV1<R>, bounded: bool) -> io::Result<NativeWireShapeV1> {
    use misaka_palw_tir::{DType, types::MAX_RANK};
    reader.step()?;
    let tag = reader.byte()?;
    let dtype = DType::ALL.into_iter().find(|d| d.tag() == tag).ok_or_else(|| wire_bad("unknown v3 leaf dtype"))?;
    let rank = reader.word()? as u64;
    if bounded && rank > MAX_RANK as u64 {
        return Err(wire_bad("bounded leaf rank exceeds its codec"));
    }
    reader.skip(rank.checked_mul(8).ok_or_else(overflow)?)?;
    // axis, line, tile
    reader.skip(17)?;
    let values = reader.word()? as u64;
    if bounded && values > misaka_palw_kernel::merkle3::TILE_V3 {
        return Err(wire_bad("bounded leaf value count exceeds its codec"));
    }
    reader.skip(values.checked_mul(dtype.width() as u64).ok_or_else(overflow)?)?;
    let siblings = reader.word()? as u64;
    if bounded && siblings > 64 {
        return Err(wire_bad("bounded leaf siblings exceed its codec"));
    }
    reader.skip(siblings.checked_mul(64).ok_or_else(overflow)?)?;
    reader.skip(64)?;
    let members = rank.checked_add(values).and_then(|n| n.checked_add(siblings)).ok_or_else(overflow)?;
    NativeWireShapeV1 { collection_entries: members, ..Default::default() }
        .allocation(rank.checked_mul(2).ok_or_else(overflow)?, size_of::<u64>())?
        .allocation(values.checked_mul(2).ok_or_else(overflow)?, size_of::<i128>())?
        .allocation(siblings.checked_mul(2).ok_or_else(overflow)?, size_of::<[u8; 64]>())
}
impl NativeWireScanV1 for misaka_palw_kernel::merkle3::LeafOpeningV3 {
    fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        leaf_opening_shape_v3(reader, false)
    }
}
impl NativeWireScanV1 for crate::palw_onboarding_tile_v3::ArtifactTileOpeningV3 {
    fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        leaf_opening_shape_v3(reader, true)
    }
}
/// G14 int: the funded-original pairing and its test-only schedule live only in the in-memory `PalwTransitionExtrasV1` (which has
/// no Borsh codec); they are never persisted, so a wire image claiming to hold one is refused rather than guessed at.
impl NativeWireScanV1 for crate::palw_funded_original_coordinator_v1::PalwFundedOriginalPairingV1 {
    fn scan_native_wire_v1<R: Read>(_reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        Err(wire_bad("funded-original pairing is transient and has no wire form"))
    }
}
impl NativeWireScanV1 for crate::palw_funded_original_coordinator_v1::PalwFundedOriginalScheduleV1 {
    fn scan_native_wire_v1<R: Read>(_reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        Err(wire_bad("funded-original schedule is test-only and has no wire form"))
    }
}
/// F03b2 shared query row: version 1 is the old row byte for byte; version 2 appends the bare service slot (3 × u64).
impl NativeWireScanV1 for crate::palw_court_shared_tir_state_v1::SharedTirQueryV1 {
    fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        reader.step()?;
        let version = reader.short()?;
        if version != 1 && version != crate::palw_court_shared_tir_state_v1::SHARED_QUERY_SERVICE_VERSION_V2 {
            return Err(wire_bad("shared query row version"));
        }
        let mut sum = NativeWireShapeV1::default();
        sum = sum.merge(crate::Hash64::scan_native_wire_v1(reader)?)?;
        sum = sum.merge(crate::Hash64::scan_native_wire_v1(reader)?)?;
        sum = sum.merge(u64::scan_native_wire_v1(reader)?)?;
        sum = sum.merge(crate::Hash64::scan_native_wire_v1(reader)?)?;
        sum = sum.merge(crate::BlockHash::scan_native_wire_v1(reader)?)?;
        sum = sum.merge(u64::scan_native_wire_v1(reader)?)?;
        sum = sum.merge(crate::palw_state_v2::PalwBondKeyV2::scan_native_wire_v1(reader)?)?;
        sum = sum.merge(crate::Hash64::scan_native_wire_v1(reader)?)?;
        sum = sum.merge(u64::scan_native_wire_v1(reader)?)?;
        sum = sum.merge(Option::<crate::palw_court_shared_tir_state_v1::SharedTirBodyV1>::scan_native_wire_v1(reader)?)?;
        if version == crate::palw_court_shared_tir_state_v1::SHARED_QUERY_SERVICE_VERSION_V2 {
            sum = sum.merge(crate::palw_court_shared_tir_state_v1::SharedServiceSlotV1::scan_native_wire_v1(reader)?)?;
        }
        Ok(sum)
    }
}
/// RFC-0024 A shared bottom row (lane r6-p2-shared): version 1 is the whole-tile row byte for byte; version 2 appends exactly one
/// position byte. Not an `Option` tag — the handwritten codec writes the bare byte only for version 2.
impl NativeWireScanV1 for crate::palw_court_shared_tir_bottom_v1::SharedTirBottomRowV1 {
    fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        reader.step()?;
        let version = reader.short()?;
        if version != 1 && version != crate::palw_court_shared_tir_bottom_v1::SHARED_TIR_BOTTOM_ROW_POSITION_VERSION_V2 {
            return Err(wire_bad("shared bottom row version"));
        }
        let mut sum = NativeWireShapeV1::default();
        sum = sum.merge(crate::palw_court_shared_tir_bottom_v1::SignedSharedTirBottomV1::scan_native_wire_v1(reader)?)?;
        sum = sum.merge(crate::palw_court_close_receipt_v1::NativeCourtRulesV1::scan_native_wire_v1(reader)?)?;
        sum = sum.merge(Vec::<u8>::scan_native_wire_v1(reader)?)?;
        sum = sum.merge(u64::scan_native_wire_v1(reader)?)?;
        sum = sum.merge(crate::Hash64::scan_native_wire_v1(reader)?)?;
        if version == crate::palw_court_shared_tir_bottom_v1::SHARED_TIR_BOTTOM_ROW_POSITION_VERSION_V2 {
            sum = sum.merge(u8::scan_native_wire_v1(reader)?)?;
        }
        Ok(sum)
    }
}
