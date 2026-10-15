//! Retired compact V4 SDK surface (PRINCIPLES §8, 2026-10-15).
//!
//! The producer proof generation (`prove_compact_material_v4`), the proof-binding and registration drafts, the compact class
//! builder and the compact receipt verifier are removed: no compact proof record is generated, bound, accepted or credited.
//! Only the allocation-bounded material decoder remains; it reads the ordinary paid material wire and is not V4-specific.
use super::*;

/// Allocation-free frame walk for the existing material wire. It preserves the
/// old codec while bounding nested maps, shape vectors and snapshot payloads
/// before Borsh creates any allocation. Mathematical/root authority is checked
/// separately by the actual native verifier.
pub fn decode_material_bounded_v4(bytes: &[u8], maximum_bytes: u64, maximum_heap: u64) -> Result<PaidClaimMaterialWireV2, String> {
    struct Cursor<'a> {
        bytes: &'a [u8],
        heap: u128,
        maximum: u128,
    }
    impl<'a> Cursor<'a> {
        fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
            if n > self.bytes.len() {
                return Err("truncated paid material".into());
            }
            let (a, b) = self.bytes.split_at(n);
            self.bytes = b;
            Ok(a)
        }
        fn charge(&mut self, n: u128) -> Result<(), String> {
            self.heap = self.heap.checked_add(n).ok_or("material heap overflow")?;
            if self.heap > self.maximum {
                return Err("paid material decoded workspace exceeds capacity".into());
            }
            Ok(())
        }
        fn count(&mut self, min_width: usize, entry_heap: u128) -> Result<usize, String> {
            let n = u32::from_le_bytes(self.take(4)?.try_into().unwrap()) as usize;
            if min_width != 0 && n > self.bytes.len() / min_width {
                return Err("paid material count exceeds exact frame".into());
            }
            self.charge((n as u128).checked_mul(entry_heap).ok_or("material vector capacity overflow")?)?;
            Ok(n)
        }
        fn tensor(&mut self) -> Result<(), String> {
            let dtype = misaka_palw_kernel::plan::dtype_of_tag(self.take(1)?[0]).ok_or("unknown material dtype")?;
            let rank = self.count(8, 8)?;
            if rank > 12 {
                return Err("material tensor rank exceeds capacity".into());
            }
            let mut elements = 1u128;
            for _ in 0..rank {
                let d = u64::from_le_bytes(self.take(8)?.try_into().unwrap());
                elements = elements.checked_mul(u128::from(d)).ok_or("material shape overflow")?;
            }
            let n = self.count(1, 1)?;
            if elements.checked_mul(dtype.width() as u128) != Some(n as u128) {
                return Err("material tensor shape/data mismatch".into());
            }
            self.take(n)?;
            self.charge(elements.checked_mul(16).ok_or("material tensor decoded heap overflow")?)
        }
    }
    if bytes.len() as u64 > maximum_bytes {
        return Err("paid material frame exceeds capacity".into());
    }
    let mut c = Cursor { bytes, heap: bytes.len() as u128, maximum: u128::from(maximum_heap) };
    for keybytes in [9usize, 7] {
        let n = c.count(keybytes + 9, 512)?;
        for _ in 0..n {
            c.take(keybytes)?;
            c.tensor()?;
        }
    }
    let n = c.count(4 + 1 + 9, 512)?;
    for _ in 0..n {
        c.take(4)?;
        match c.take(1)?[0] {
            0 => {}
            1 => {
                c.take(2)?;
            }
            _ => return Err("material layer option invalid".into()),
        }
        c.tensor()?;
    }
    let n = c.count(8, 512)?;
    for _ in 0..n {
        c.take(4)?;
        let occurrences = c.count(4, 24)?;
        for _ in 0..occurrences {
            let nodes = c.count(9, 32)?;
            for _ in 0..nodes {
                c.tensor()?;
            }
        }
    }
    let n = c.count(8, 512)?;
    for _ in 0..n {
        c.take(4)?;
        let siblings = c.count(64, 64)?;
        c.take(siblings.checked_mul(64).ok_or("sibling bytes overflow")?)?;
    }
    let n = c.count(4, 4)?;
    c.take(n.checked_mul(4).ok_or("token bytes overflow")?)?;
    let n = c.count(72 + 8, 512)?;
    for _ in 0..n {
        c.take(72)?;
        for _ in 0..2 {
            let words = c.count(4, 4)?;
            c.take(words.checked_mul(4).ok_or("snapshot bytes overflow")?)?;
        }
    }
    if !c.bytes.is_empty() {
        return Err("paid material trailing bytes".into());
    }
    borsh::from_slice(bytes).map_err(|e| e.to_string())
}

#[cfg(test)]
mod wire_tests {
    use super::*;
    #[test]
    fn material_borsh_lengths_are_bounded_before_nested_allocation() {
        let material = PaidClaimMaterialWireV2::default();
        let bytes = borsh::to_vec(&material).unwrap();
        decode_material_bounded_v4(&bytes, 4096, 4096).unwrap();
        for prefix in 0..bytes.len() {
            assert!(decode_material_bounded_v4(&bytes[..prefix], 4096, 4096).is_err());
        }
        let mut bomb = bytes.clone();
        bomb[..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_material_bounded_v4(&bomb, 4096, 4096).is_err());
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(decode_material_bounded_v4(&trailing, 4096, 4096).is_err());
    }
}
