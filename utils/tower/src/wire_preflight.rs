//! Allocation-free inspection of pinned prost wire schemas. This is a heap
//! admission estimate, never a proof of message or model computation validity.
//! Runtime decoder/owner integration is separate from this schema inspector.
use std::fmt;

pub const MAX_NESTING: usize = 100; // prost 0.14.3's default recursion limit
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreflightError(pub &'static str);
impl fmt::Display for PreflightError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for PreflightError {}
pub type Result<T> = std::result::Result<T, PreflightError>;
fn add(a: u64, b: u64) -> Result<u64> {
    a.checked_add(b).ok_or(PreflightError("protobuf heap estimate overflow"))
}
fn mul(a: u64, b: u64) -> Result<u64> {
    a.checked_mul(b).ok_or(PreflightError("protobuf heap estimate overflow"))
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
/// Bounds owned Vec/String/Box capacities and their growth while decoding through
/// tonic's contiguous DecodeBuf (whose byte slices share the input allocation).
/// Input/decompression allocations, allocator metadata/fragmentation and stack
/// space need separate admission. Arbitrary Buf adapters may make extra copies.
pub struct HeapBound {
    pub retained_bytes: u64,
    pub peak_bytes: u64,
}
impl HeapBound {
    pub fn owned(&mut self, bytes: u64, peak: u64) -> Result<()> {
        self.peak_bytes = self.peak_bytes.max(add(self.retained_bytes, peak)?);
        self.retained_bytes = add(self.retained_bytes, bytes)?;
        Ok(())
    }
    pub fn child(&mut self, child: Self) -> Result<()> {
        self.owned(child.retained_bytes, child.peak_bytes)
    }
    fn grow(&mut self, old: u64, new: u64) -> Result<()> {
        // Reallocation may temporarily own both old and new allocations.
        self.peak_bytes = self.peak_bytes.max(add(self.retained_bytes, new)?);
        self.retained_bytes = add(self.retained_bytes, new.checked_sub(old).ok_or(PreflightError("capacity decreased"))?)?;
        Ok(())
    }
    pub fn blob(&mut self, bytes: usize, state: &mut FieldState) -> Result<()> {
        let bytes = bytes as u64;
        if bytes > state.capacity {
            let next = bytes.max(mul(state.capacity, 2)?).max(8);
            self.grow(state.capacity, next)?;
            state.capacity = next;
        }
        Ok(())
    }
    pub fn array<T>(&mut self, count: u64, state: &mut FieldState) -> Result<()> {
        state.elements = add(state.elements, count)?;
        let size = std::mem::size_of::<T>() as u64;
        if size == 0 || count == 0 {
            return Ok(());
        }
        if state.elements > state.capacity {
            // RawVec::MIN_NON_ZERO_CAP and geometric push growth, pinned by
            // capacity comparison fixtures against this toolchain/prost version.
            let minimum = if size == 1 {
                8
            } else if size <= 1024 {
                4
            } else {
                1
            };
            let mut next = if state.capacity == 0 { minimum } else { mul(state.capacity, 2)? };
            let mut old = state.capacity;
            loop {
                self.grow(mul(old, size)?, mul(next, size)?)?;
                if next >= state.elements {
                    break;
                }
                old = next;
                next = mul(next, 2)?;
            }
            state.capacity = next;
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct FieldState {
    capacity: u64,
    elements: u64,
    message_seen: bool,
}
impl FieldState {
    /// Prost merges repeated occurrences of a singular message into one value.
    /// Independent fragment bounds cannot model that value's aggregate Vec
    /// growth. Until merge-aware state exists, do not return a false bound.
    pub fn singular_message_once(&mut self) -> Result<()> {
        if self.message_seen {
            return Err(PreflightError("unsupported repeated singular/message-oneof merge"));
        }
        self.message_seen = true;
        Ok(())
    }
}

pub trait WirePreflight {
    fn wire_heap_bound(bytes: &[u8], depth: usize) -> Result<HeapBound>;
}
pub trait OneofPreflight {
    fn is_message_tag(tag: u32) -> bool;
    fn wire_field_bound(tag: u32, wire: WireType, cursor: &mut Cursor<'_>, depth: usize) -> Result<HeapBound>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WireType {
    Varint = 0,
    Fixed64 = 1,
    Bytes = 2,
    StartGroup = 3,
    EndGroup = 4,
    Fixed32 = 5,
}
#[derive(Debug)]
pub struct Cursor<'a> {
    data: &'a [u8],
}
impl<'a> Cursor<'a> {
    pub fn new(data: &'a [u8], depth: usize) -> Result<Self> {
        if depth > MAX_NESTING {
            return Err(PreflightError("protobuf recursion limit"));
        }
        Ok(Self { data })
    }
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    pub fn varint(&mut self) -> Result<u64> {
        let mut value = 0;
        for shift in (0..70).step_by(7) {
            let (&byte, rest) = self.data.split_first().ok_or(PreflightError("truncated protobuf varint"))?;
            self.data = rest;
            if shift == 63 && byte > 1 {
                return Err(PreflightError("protobuf varint overflow"));
            }
            value |= u64::from(byte & 127) << shift;
            if byte < 128 {
                return Ok(value);
            }
        }
        Err(PreflightError("protobuf varint overflow"))
    }
    pub fn key(&mut self) -> Result<Option<(u32, WireType)>> {
        if self.data.is_empty() {
            return Ok(None);
        }
        let key = self.varint()?;
        if key > u32::MAX as u64 || key >> 3 == 0 {
            return Err(PreflightError("invalid protobuf field tag"));
        }
        let wire = match key & 7 {
            0 => WireType::Varint,
            1 => WireType::Fixed64,
            2 => WireType::Bytes,
            3 => WireType::StartGroup,
            4 => WireType::EndGroup,
            5 => WireType::Fixed32,
            _ => return Err(PreflightError("invalid protobuf wire type")),
        };
        Ok(Some(((key >> 3) as u32, wire)))
    }
    pub fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        if len > self.data.len() {
            return Err(PreflightError("truncated protobuf field"));
        }
        let (value, rest) = self.data.split_at(len);
        self.data = rest;
        Ok(value)
    }
    pub fn bytes(&mut self, wire: WireType) -> Result<&'a [u8]> {
        self.expect(wire, WireType::Bytes)?;
        let len = usize::try_from(self.varint()?).map_err(|_| PreflightError("protobuf length overflow"))?;
        self.take(len)
    }
    pub fn expect(&self, actual: WireType, expected: WireType) -> Result<()> {
        if actual != expected { Err(PreflightError("protobuf field wire type mismatch")) } else { Ok(()) }
    }
    pub fn scalar(&mut self, wire: WireType, expected: WireType) -> Result<()> {
        self.expect(wire, expected)?;
        match wire {
            WireType::Varint => {
                self.varint()?;
            }
            WireType::Fixed64 => {
                self.take(8)?;
            }
            WireType::Fixed32 => {
                self.take(4)?;
            }
            _ => return Err(PreflightError("invalid scalar wire type")),
        }
        Ok(())
    }
    pub fn scalar_count(&mut self, wire: WireType, expected: WireType, depth: usize) -> Result<u64> {
        if wire != WireType::Bytes {
            self.scalar(wire, expected)?;
            return Ok(1);
        }
        let mut packed = Cursor::new(self.bytes(wire)?, depth)?;
        let mut count = 0u64;
        while !packed.is_empty() {
            packed.scalar(expected, expected)?;
            count = add(count, 1)?;
        }
        Ok(count)
    }
    pub fn skip(&mut self, tag: u32, wire: WireType, depth: usize) -> Result<()> {
        if depth > MAX_NESTING {
            return Err(PreflightError("protobuf recursion limit"));
        }
        match wire {
            WireType::Varint | WireType::Fixed32 | WireType::Fixed64 => self.scalar(wire, wire),
            WireType::Bytes => {
                self.bytes(wire)?;
                Ok(())
            }
            WireType::StartGroup => loop {
                let (inner, kind) = self.key()?.ok_or(PreflightError("unterminated protobuf group"))?;
                if kind == WireType::EndGroup {
                    return if inner == tag { Ok(()) } else { Err(PreflightError("mismatched protobuf end group")) };
                }
                self.skip(inner, kind, depth + 1)?;
            },
            WireType::EndGroup => Err(PreflightError("unexpected protobuf end group")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arithmetic_and_varint_overflow_refuse_without_allocation() {
        assert!(HeapBound::default().owned(u64::MAX, u64::MAX).is_ok());
        let mut heap = HeapBound { retained_bytes: u64::MAX, peak_bytes: u64::MAX };
        assert!(heap.owned(1, 1).is_err());
        assert!(HeapBound::default().array::<u64>(u64::MAX, &mut FieldState::default()).is_err());
        for bytes in [&[0x80][..], &[0xff; 10][..], &[0x80; 11][..]] {
            assert!(Cursor::new(bytes, 0).unwrap().varint().is_err());
        }
    }
}
