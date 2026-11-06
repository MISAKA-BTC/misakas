//! Incremental verification of completed frames already received into a spill.
//! A partial row stays on disk; it never grows a reassembly Vec across chunks.
use super::*;
use crate::palw_native_resource_v1::{NativeDemandLeaseV1, reserve_native_requests_v1};
use std::io::{Read, Seek, SeekFrom};

pub struct PalwRowGenerationProgressV1 {
    expected: Hash64,
    total: u64,
    position: u64,
    body_end: u64,
    verifier: Option<PalwRowStreamVerifierV1<std::io::Sink>>,
    collections: u32,
    rows: Option<u64>,
    suffix: Option<PalwRowSuffixV1>,
    _metadata: Option<NativeDemandLeaseV1>,
}
fn bad() -> PalwRowStreamErrorV1 {
    PalwRowStreamErrorV1::TruncatedGeneration
}
fn ioerr(e: std::io::Error) -> PalwRowStreamErrorV1 {
    if e.raw_os_error().is_some() { PalwRowStreamErrorV1::Sink(e.to_string()) } else { bad() }
}
fn number<const N: usize>(r: &mut impl Read) -> Result<[u8; N], PalwRowStreamErrorV1> {
    let mut b = [0; N];
    r.read_exact(&mut b).map_err(ioerr)?;
    Ok(b)
}
fn reserve(n: u64) -> Result<Option<NativeDemandLeaseV1>, PalwRowStreamErrorV1> {
    reserve_native_requests_v1(n).map_err(PalwRowStreamErrorV1::NativeResource)
}
impl PalwRowGenerationProgressV1 {
    pub fn new(expected: Hash64, total: u64) -> Result<Self, PalwRowStreamErrorV1> {
        if total > PALW_ROW_STREAM_MAX_GENERATION_BYTES_V1 as u64 || total < 30 {
            return Err(bad());
        }
        Ok(Self {
            expected,
            total,
            position: 0,
            body_end: 0,
            verifier: None,
            collections: 0,
            rows: None,
            suffix: None,
            _metadata: None,
        })
    }
    pub fn expected_root(&self) -> Hash64 {
        self.expected
    }
    pub fn suffix(&self) -> Option<PalwRowSuffixV1> {
        self.suffix
    }
    /// Call only after the received bytes have passed their authenticated chunk
    /// identity/hash and been written to this same private spill.
    pub fn advance<R: Read + Seek>(&mut self, r: &mut R, available: u64) -> Result<(), PalwRowStreamErrorV1> {
        if available > self.total {
            return Err(bad());
        }
        if self.suffix.is_some() {
            return Ok(());
        }
        if self.verifier.is_none() {
            if available < 26 {
                return Ok(());
            }
            r.seek(SeekFrom::Start(0)).map_err(ioerr)?;
            if u32::from_le_bytes(number(r)?) != PALW_ROW_STREAM_GENERATION_MAGIC_V1
                || u16::from_le_bytes(number(r)?) != PALW_ROW_STREAM_GENERATION_VERSION_V1
            {
                return Err(bad());
            }
            let generation = u64::from_le_bytes(number(r)?);
            if generation != PALW_ROW_STREAM_CURRENT_GENERATION_V1 {
                return Err(PalwRowStreamErrorV1::UnexpectedGeneration {
                    got: generation,
                    current: PALW_ROW_STREAM_CURRENT_GENERATION_V1,
                });
            }
            let body = u64::from_le_bytes(number(r)?);
            if body > self.total - 30 {
                return Err(bad());
            }
            self.body_end = 22 + body;
            let n = u32::from_le_bytes(number(r)?) as usize;
            if n > PALW_ROW_STREAM_MAX_PREIMAGE_BYTES_V1 {
                return Err(PalwRowStreamErrorV1::PreimageTooLarge { len: n, max: PALW_ROW_STREAM_MAX_PREIMAGE_BYTES_V1 });
            }
            if 30 + n as u64 > self.body_end {
                return Err(bad());
            }
            if available < 30 + n as u64 {
                return Ok(());
            }
            r.seek(SeekFrom::Start(26 + n as u64)).map_err(ioerr)?;
            self.collections = u32::from_le_bytes(number(r)?);
            if self.collections as usize > n / 64 {
                return Err(bad());
            }
            let node = crate::palw_native_resource_v1::btree_node_envelope_bytes_v1::<Vec<u8>, ()>()
                + std::mem::size_of::<(Vec<u8>, u64)>() * 6
                + std::mem::size_of::<usize>() * 6
                + PALW_ROW_STREAM_MAX_LABEL_BYTES_V1 * 6;
            self._metadata =
                reserve((n as u64).checked_add((self.collections as u64).checked_mul(node as u64).ok_or_else(bad)?).ok_or_else(bad)?)?;
            let mut preimage = vec![0; n];
            r.seek(SeekFrom::Start(26)).map_err(ioerr)?;
            r.read_exact(&mut preimage).map_err(ioerr)?;
            self.verifier = Some(PalwRowStreamVerifierV1::new(self.expected, preimage, std::io::sink(), self.total)?);
            self.position = 30 + n as u64;
        }
        loop {
            if let Some(rows) = self.rows {
                if rows == 0 {
                    self.verifier.as_mut().unwrap().end_collection()?;
                    self.rows = None;
                    self.collections -= 1;
                    continue;
                }
                if available < self.position + 4 {
                    return Ok(());
                }
                r.seek(SeekFrom::Start(self.position)).map_err(ioerr)?;
                let k = u32::from_le_bytes(number(r)?) as u64;
                let value_header = self.position.checked_add(4).and_then(|n| n.checked_add(k)).ok_or_else(bad)?;
                if value_header + 4 > self.body_end {
                    return Err(bad());
                }
                if available < value_header + 4 {
                    return Ok(());
                }
                r.seek(SeekFrom::Start(value_header)).map_err(ioerr)?;
                let v = u32::from_le_bytes(number(r)?) as u64;
                let end = value_header.checked_add(4).and_then(|n| n.checked_add(v)).ok_or_else(bad)?;
                if end > self.body_end {
                    return Err(bad());
                }
                if available < end {
                    return Ok(());
                }
                let _row = reserve(
                    k.checked_mul(3).and_then(|n| n.checked_add(v.checked_mul(2)?)).and_then(|n| n.checked_add(64)).ok_or_else(bad)?,
                )?;
                let mut key = vec![0; k as usize];
                let mut value = vec![0; v as usize];
                r.seek(SeekFrom::Start(self.position + 4)).map_err(ioerr)?;
                r.read_exact(&mut key).map_err(ioerr)?;
                r.seek(SeekFrom::Start(value_header + 4)).map_err(ioerr)?;
                r.read_exact(&mut value).map_err(ioerr)?;
                self.verifier.as_mut().unwrap().row(&key, &value)?;
                self.position = end;
                self.rows = Some(rows - 1);
            } else if self.collections > 0 {
                if available < self.position + 4 {
                    return Ok(());
                }
                r.seek(SeekFrom::Start(self.position)).map_err(ioerr)?;
                let n = u32::from_le_bytes(number(r)?) as usize;
                if n > PALW_ROW_STREAM_MAX_LABEL_BYTES_V1 {
                    return Err(bad());
                }
                let end = self.position + 4 + n as u64 + 16;
                if end > self.body_end {
                    return Err(bad());
                }
                if available < end {
                    return Ok(());
                }
                let mut label = vec![0; n];
                r.read_exact(&mut label).map_err(ioerr)?;
                let rows = u64::from_le_bytes(number(r)?);
                let offset = usize::try_from(u64::from_le_bytes(number(r)?)).map_err(|_| bad())?;
                if rows > (self.body_end - end) / 8 {
                    return Err(bad());
                }
                let verifier = self.verifier.as_mut().unwrap();
                match projection::for_label(&label) {
                    Some(p) => verifier.begin_projected_collection(&label, rows, offset, p)?,
                    None => verifier.begin_collection(&label, rows, offset)?,
                }
                self.rows = Some(rows);
                self.position = end;
            } else {
                if self.position + 1 != self.body_end {
                    return Err(bad());
                }
                if available < self.position + 9 {
                    return Ok(());
                }
                r.seek(SeekFrom::Start(self.position)).map_err(ioerr)?;
                if number::<1>(r)?[0] != PALW_ROW_SPILL_END_V1 {
                    return Err(bad());
                }
                let length = u64::from_le_bytes(number(r)?);
                let offset = self.position + 9;
                if offset.checked_add(length) != Some(self.total) {
                    return Err(bad());
                }
                let inventory = self.verifier.take().unwrap().finish()?.inventory();
                self.suffix = Some(PalwRowSuffixV1 { offset, length, inventory });
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Hash64, Vec<u8>) {
        let label = b"progress-test";
        let rows = vec![(vec![1], vec![7u8; 5 << 20])];
        let root = crate::palw_state_v2::palw_collection_root_of_entries_v1(label, 1, rows.clone().into_iter());
        let preimage = root.as_byte_slice();
        let wire = encode_row_stream_generation_v1(
            preimage,
            &[PalwRowStreamCollectionV1 { label: label.to_vec(), offset: 0, rows }],
            b"native-suffix",
        )
        .unwrap();
        (palw_state_root_of_preimage_v1(preimage), wire)
    }
    #[test]
    fn rfc37_progress_keeps_partial_rows_on_disk_and_finishes_the_exact_suffix() {
        let (root, wire) = fixture();
        let mut progress = PalwRowGenerationProgressV1::new(root, wire.len() as u64).unwrap();
        let mut reader = std::io::Cursor::new(&wire);
        progress.advance(&mut reader, 4 << 20).unwrap();
        assert!(progress.suffix().is_none());
        progress.advance(&mut reader, wire.len() as u64).unwrap();
        let suffix = progress.suffix().unwrap();
        assert_eq!(&wire[suffix.offset as usize..], b"native-suffix");
        reader.set_position(0);
        let full = palw_verify_row_generation_reader_v1(root, &mut reader, wire.len() as u64).unwrap();
        assert_eq!((suffix.offset, suffix.length), (full.offset, full.length));
    }
    #[test]
    fn rfc37_progress_rejects_the_preimage_before_waiting_for_later_chunks() {
        let (root, mut wire) = fixture();
        wire[26] ^= 1;
        let mut progress = PalwRowGenerationProgressV1::new(root, wire.len() as u64).unwrap();
        assert!(matches!(
            progress.advance(&mut std::io::Cursor::new(&wire), 4 << 20),
            Err(PalwRowStreamErrorV1::PreimageDoesNotMatchRoot(_))
        ));
    }
    #[test]
    fn rfc37_progress_rejects_a_completed_bad_collection_before_receiving_the_suffix() {
        let rows = vec![(vec![1], vec![7; 1024])];
        let label = b"early-abort";
        let root = crate::palw_state_v2::palw_collection_root_of_entries_v1(label, 1, rows.clone().into_iter());
        let suffix = vec![9; 5 << 20];
        let mut wire = encode_row_stream_generation_v1(
            root.as_byte_slice(),
            &[PalwRowStreamCollectionV1 { label: label.to_vec(), offset: 0, rows }],
            &suffix,
        )
        .unwrap();
        let at = wire.windows(1024).position(|bytes| bytes == [7; 1024]).unwrap();
        wire[at] ^= 1;
        let mut progress =
            PalwRowGenerationProgressV1::new(palw_state_root_of_preimage_v1(root.as_byte_slice()), wire.len() as u64).unwrap();
        assert!(matches!(
            progress.advance(&mut std::io::Cursor::new(&wire), 4 << 20),
            Err(PalwRowStreamErrorV1::CollectionMismatch(_))
        ));
    }
}
