//! Bounded, row-at-a-time spill decoding. This is framing validation, not evidence of a valid imported state.
use super::{PALW_ROW_SPILL_COLLECTION_V1, PALW_ROW_SPILL_END_V1, PALW_ROW_SPILL_ROW_V1};
use std::io::{self, Error, ErrorKind, Read};

/// Caller-owned local limits. These do not change any consensus or wire limit.
#[derive(Clone, Copy, Debug)]
pub struct PalwRowSpillLimitsV1 {
    pub max_bytes: u64,
    pub max_collections: u64,
    pub max_rows: u64,
    pub max_label_bytes: u32,
    pub max_key_bytes: u32,
    pub max_value_bytes: u32,
}

/// Slices are valid only for the callback. The reader retains at most one label, key and value.
#[derive(Debug)]
pub enum PalwRowSpillEventV1<'a> {
    Collection { label: &'a [u8], rows: u64 },
    Row { key: &'a [u8], value: &'a [u8] },
    End,
}

struct Budgeted<R> {
    reader: R,
    remaining: u64,
}
impl<R: Read> Budgeted<R> {
    fn read_exact(&mut self, out: &mut [u8]) -> io::Result<()> {
        self.charge(out.len() as u64)?;
        self.reader.read_exact(out)
    }
    fn charge(&mut self, bytes: u64) -> io::Result<()> {
        self.remaining = self.remaining.checked_sub(bytes).ok_or_else(|| Error::other("spill: local byte budget exceeded"))?;
        Ok(())
    }
    fn u32(&mut self) -> io::Result<u32> {
        let mut bytes = [0; 4];
        self.read_exact(&mut bytes)?;
        Ok(u32::from_le_bytes(bytes))
    }
    fn bytes(&mut self, max: u32) -> io::Result<Vec<u8>> {
        let len = self.u32()?;
        if len > max {
            return Err(Error::other("spill: field length exceeds local limit"));
        }
        // Check the total budget before requesting allocation or consuming the body.
        self.charge(u64::from(len))?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(len as usize)
            .map_err(|e| Error::new(ErrorKind::OutOfMemory, format!("spill: local allocation refused: {e}")))?;
        bytes.resize(len as usize, 0);
        self.reader.read_exact(&mut bytes)?;
        Ok(bytes)
    }
    fn tag(&mut self) -> io::Result<u8> {
        let mut tag = [0];
        self.read_exact(&mut tag)?;
        Ok(tag[0])
    }
    fn next_collection(&mut self) -> io::Result<bool> {
        // Probe EOF even when the budget is exhausted. One extra byte refuses the stream; it is never allocated or accepted.
        let mut tag = [0];
        loop {
            match self.reader.read(&mut tag) {
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
                Ok(0) => return Ok(false),
                Ok(_) => break,
            }
        }
        self.charge(1)?;
        if tag[0] != PALW_ROW_SPILL_COLLECTION_V1 {
            return Err(invalid("spill: expected a collection header"));
        }
        Ok(true)
    }
}
fn invalid(message: &'static str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}

/// Local limits return `ErrorKind::Other`, allocation refusal returns `OutOfMemory`, and malformed framing returns
/// `InvalidData`. Local refusal is not peer misbehavior.
///
/// Decode the existing node-local spill format without collecting its rows. Every length and cumulative count is
/// checked before allocation. Callback failures abort immediately; a callback must stage writes and promote them only
/// after the caller's authenticated-stream and whole-state checks succeed. This function grants no verification token.
pub fn palw_visit_row_spill_v1(
    reader: impl Read,
    limits: PalwRowSpillLimitsV1,
    mut visit: impl FnMut(PalwRowSpillEventV1<'_>) -> io::Result<()>,
) -> io::Result<()> {
    let mut input = Budgeted { reader, remaining: limits.max_bytes };
    let mut collections = 0u64;
    let mut total_rows = 0u64;
    while input.next_collection()? {
        collections = collections
            .checked_add(1)
            .filter(|n| *n <= limits.max_collections)
            .ok_or_else(|| Error::other("spill: collection count exceeds local limit"))?;
        let label = input.bytes(limits.max_label_bytes)?;
        let mut count = [0; 8];
        input.read_exact(&mut count)?;
        let rows = u64::from_le_bytes(count);
        total_rows = total_rows
            .checked_add(rows)
            .filter(|n| *n <= limits.max_rows)
            .ok_or_else(|| Error::other("spill: row count exceeds local limit"))?;
        visit(PalwRowSpillEventV1::Collection { label: &label, rows })?;
        for _ in 0..rows {
            if input.tag()? != PALW_ROW_SPILL_ROW_V1 {
                return Err(invalid("spill: expected a row"));
            }
            let key = input.bytes(limits.max_key_bytes)?;
            let value = input.bytes(limits.max_value_bytes)?;
            visit(PalwRowSpillEventV1::Row { key: &key, value: &value })?;
        }
        if input.tag()? != PALW_ROW_SPILL_END_V1 {
            return Err(invalid("spill: collection not closed"));
        }
        visit(PalwRowSpillEventV1::End)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_row_stream_v1::PalwRowStreamVerifierV1;
    use crate::palw_state_v2::{palw_collection_root_of_entries_v1, palw_state_root_of_preimage_v1};
    use std::io::Cursor;

    fn limits(bytes: usize) -> PalwRowSpillLimitsV1 {
        PalwRowSpillLimitsV1 {
            max_bytes: bytes as u64,
            max_collections: 2,
            max_rows: 4,
            max_label_bytes: 16,
            max_key_bytes: 8,
            max_value_bytes: 16,
        }
    }
    fn spill() -> Vec<u8> {
        let rows = [(b"k".to_vec(), b"value".to_vec()), (b"l".to_vec(), b"other".to_vec())];
        let root = palw_collection_root_of_entries_v1(b"test", rows.len(), rows.iter().cloned());
        let preimage = root.as_byte_slice().to_vec();
        let mut verifier =
            PalwRowStreamVerifierV1::new(palw_state_root_of_preimage_v1(&preimage), preimage, Vec::new(), 1024).unwrap();
        verifier.begin_collection(b"test", 2, 0).unwrap();
        for (key, value) in rows {
            verifier.row(&key, &value).unwrap();
        }
        verifier.end_collection().unwrap();
        verifier.finish().unwrap().into_sink()
    }
    #[test]
    fn actual_verifier_spill_visits_identical_rows_and_exact_budget() {
        let bytes = spill();
        let mut actual = Vec::new();
        palw_visit_row_spill_v1(&bytes[..], limits(bytes.len()), |event| {
            match event {
                PalwRowSpillEventV1::Collection { label, rows } => {
                    assert_eq!(label, b"test");
                    assert_eq!(rows, 2);
                }
                PalwRowSpillEventV1::Row { key, value } => actual.push((key.to_vec(), value.to_vec())),
                PalwRowSpillEventV1::End => (),
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(actual, vec![(b"k".to_vec(), b"value".to_vec()), (b"l".to_vec(), b"other".to_vec())]);
        assert!(palw_visit_row_spill_v1(&bytes[..], limits(bytes.len() - 1), |_| Ok(())).is_err());
    }
    #[test]
    fn oversized_length_refuses_before_reading_or_allocating_body() {
        let mut bytes = vec![PALW_ROW_SPILL_COLLECTION_V1];
        bytes.extend_from_slice(&u32::MAX.to_le_bytes());
        let mut cursor = Cursor::new(bytes);
        assert_eq!(palw_visit_row_spill_v1(&mut cursor, limits(1024), |_| Ok(())).unwrap_err().kind(), ErrorKind::Other);
        assert_eq!(cursor.position(), 5);
    }
    #[test]
    fn total_budget_refuses_field_body_before_reading_it() {
        let bytes = spill();
        let mut cursor = Cursor::new(bytes);
        assert!(palw_visit_row_spill_v1(&mut cursor, limits(7), |_| Ok(())).is_err());
        assert_eq!(cursor.position(), 5);
    }
    #[test]
    fn declared_count_and_callback_errors_stop_before_row_consumption() {
        let bytes = spill();
        let mut cap = limits(bytes.len());
        cap.max_rows = 1;
        let mut calls = 0;
        assert!(
            palw_visit_row_spill_v1(&bytes[..], cap, |_| {
                calls += 1;
                Ok(())
            })
            .is_err()
        );
        assert_eq!(calls, 0);
        let error = palw_visit_row_spill_v1(&bytes[..], limits(bytes.len()), |_| Err(Error::other("staging refused"))).unwrap_err();
        assert_eq!(error.to_string(), "staging refused");
        for end in 1..bytes.len() {
            assert!(palw_visit_row_spill_v1(&bytes[..end], limits(bytes.len()), |_| Ok(())).is_err(), "truncation at {end}");
        }
    }
    #[test]
    fn every_field_and_collection_limit_is_enforced() {
        let bytes = spill();
        for field in 0..4 {
            let mut cap = limits(bytes.len());
            match field {
                0 => cap.max_label_bytes = 3,
                1 => cap.max_key_bytes = 0,
                2 => cap.max_value_bytes = 4,
                _ => cap.max_collections = 0,
            }
            assert_eq!(palw_visit_row_spill_v1(&bytes[..], cap, |_| Ok(())).unwrap_err().kind(), ErrorKind::Other);
        }
        let mut doubled = bytes.clone();
        doubled.extend_from_slice(&bytes);
        let mut cap = limits(doubled.len());
        cap.max_rows = 3;
        assert!(palw_visit_row_spill_v1(&doubled[..], cap, |_| Ok(())).is_err(), "row limit must be cumulative");
        cap.max_rows = 4;
        cap.max_collections = 1;
        assert!(palw_visit_row_spill_v1(&doubled[..], cap, |_| Ok(())).is_err());
        let mut trailing = bytes.clone();
        trailing.push(PALW_ROW_SPILL_ROW_V1);
        assert!(palw_visit_row_spill_v1(&trailing[..], limits(trailing.len()), |_| Ok(())).is_err());
    }
    #[test]
    fn impossible_offset_is_refused_without_panicking() {
        let root = palw_collection_root_of_entries_v1(b"first", 0, std::iter::empty());
        let preimage = root.as_byte_slice().to_vec();
        let mut verifier =
            PalwRowStreamVerifierV1::new(palw_state_root_of_preimage_v1(&preimage), preimage, Vec::new(), 1024).unwrap();
        verifier.begin_collection(b"first", 0, 0).unwrap();
        verifier.end_collection().unwrap();
        assert!(verifier.begin_collection(b"overflow", 0, usize::MAX).is_err());
        assert!(matches!(verifier.finish(), Err(super::super::PalwRowStreamErrorV1::Poisoned)));
    }
}
