//! Construction bounds shared by legacy and chunked snapshot serving.
use borsh::BorshSerialize;
use std::io::{self, Write};

pub(crate) fn declaration_metadata_bytes(classes: usize) -> io::Result<u64> {
    (classes as u64)
        .checked_mul(512)
        .and_then(|n| n.checked_add(4096))
        .ok_or_else(|| io::Error::other("snapshot declaration metadata overflow"))
}
pub(crate) fn encoded_size<T: BorshSerialize>(value: &T, cap: u64) -> io::Result<u64> {
    struct Count {
        bytes: u64,
        cap: u64,
    }
    impl Write for Count {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            self.bytes = self
                .bytes
                .checked_add(data.len() as u64)
                .filter(|n| *n <= self.cap)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "snapshot exceeds canonical cap"))?;
            Ok(data.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count { bytes: 0, cap };
    value.serialize(&mut count)?;
    Ok(count.bytes)
}
pub(crate) fn encode_exact<T: BorshSerialize>(value: &T, bytes: u64) -> io::Result<Vec<u8>> {
    struct Exact<'a> {
        value: &'a mut Vec<u8>,
        remaining: u64,
    }
    impl Write for Exact<'_> {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            if data.len() as u64 > self.remaining {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "snapshot exceeds preallocated size"));
            }
            self.value.extend_from_slice(data);
            self.remaining -= data.len() as u64;
            Ok(data.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let size = usize::try_from(bytes).map_err(|_| io::Error::other("snapshot allocation size overflow"))?;
    let mut value_bytes = Vec::with_capacity(size);
    let mut exact = Exact { value: &mut value_bytes, remaining: bytes };
    value.serialize(&mut exact)?;
    if exact.remaining != 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "snapshot is shorter than preallocated size"));
    }
    Ok(value_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Pieces;
    impl BorshSerialize for Pieces {
        fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
            for _ in 0..10001 {
                writer.write_all(&[17; 64])?;
            }
            Ok(())
        }
    }
    #[test]
    fn exact_legacy_serialization_avoids_geometric_capacity_growth() {
        let bytes = encoded_size(&Pieces, 1 << 20).unwrap();
        let actual = encode_exact(&Pieces, bytes).unwrap();
        assert_eq!(actual.capacity(), bytes as usize);
        assert_eq!(actual.len(), bytes as usize);
        assert_eq!(actual, borsh::to_vec(&Pieces).unwrap());
        assert!(encode_exact(&Pieces, bytes - 1).is_err());
        assert!(encoded_size(&Pieces, bytes - 1).is_err());
    }
    #[test]
    fn metadata_covers_original_and_protobuf_vectors_during_conversion() {
        for n in [0usize, 1, 2, 3, 4, 5, 4097] {
            let old_capacity = if n == 0 { 0 } else { n.next_power_of_two().max(4) };
            let peak = old_capacity * (64 + 24) + n * (48 + 64);
            assert!(declaration_metadata_bytes(n).unwrap() >= peak as u64);
        }
        if usize::BITS == 64 {
            assert!(declaration_metadata_bytes(usize::MAX).is_err());
        }
    }
}
