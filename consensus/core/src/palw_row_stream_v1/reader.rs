//! File-backed generation parsing for protocol109. Counts and native memory are
//! admitted before vectors are allocated; the suffix remains in the same spill.
use super::*;
use std::io::{Read, Seek};
#[derive(Debug, Clone, Copy)]
pub struct PalwRowSuffixV1 {
    pub inventory: PalwRowInventoryV1,
    pub offset: u64,
    pub length: u64,
}
fn io_error(error: std::io::Error) -> PalwRowStreamErrorV1 {
    if error.raw_os_error().is_some() {
        PalwRowStreamErrorV1::Sink(format!("row spill read: {error}"))
    } else {
        PalwRowStreamErrorV1::TruncatedGeneration
    }
}
fn u32r(r: &mut impl Read) -> Result<u32, PalwRowStreamErrorV1> {
    let mut b = [0; 4];
    r.read_exact(&mut b).map_err(io_error)?;
    Ok(u32::from_le_bytes(b))
}
fn u64r(r: &mut impl Read) -> Result<u64, PalwRowStreamErrorV1> {
    let mut b = [0; 8];
    r.read_exact(&mut b).map_err(io_error)?;
    Ok(u64::from_le_bytes(b))
}
fn reserve(n: u64) -> Result<Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>, PalwRowStreamErrorV1> {
    crate::palw_native_resource_v1::reserve_native_requests_v1(n).map_err(PalwRowStreamErrorV1::NativeResource)
}
/// Authenticate all streamed roots against a trusted selected-child header.
/// On success the reader is positioned at the native envelope suffix. This is
/// staging evidence only; the caller must run the ordinary whole-state import.
pub fn palw_verify_row_generation_reader_v1<R: Read + Seek>(
    expected_root: Hash64,
    reader: &mut R,
    total: u64,
) -> Result<PalwRowSuffixV1, PalwRowStreamErrorV1> {
    if total > PALW_ROW_STREAM_MAX_GENERATION_BYTES_V1 as u64 {
        return Err(PalwRowStreamErrorV1::GenerationTooLarge { len: total as usize, max: PALW_ROW_STREAM_MAX_GENERATION_BYTES_V1 });
    }
    let start = reader.stream_position().map_err(io_error)?;
    if u32r(reader)? != PALW_ROW_STREAM_GENERATION_MAGIC_V1 {
        return Err(PalwRowStreamErrorV1::TruncatedGeneration);
    }
    let mut version = [0; 2];
    reader.read_exact(&mut version).map_err(io_error)?;
    if u16::from_le_bytes(version) != PALW_ROW_STREAM_GENERATION_VERSION_V1 {
        return Err(PalwRowStreamErrorV1::TruncatedGeneration);
    }
    let generation = u64r(reader)?;
    if generation != PALW_ROW_STREAM_CURRENT_GENERATION_V1 {
        return Err(PalwRowStreamErrorV1::UnexpectedGeneration { got: generation, current: PALW_ROW_STREAM_CURRENT_GENERATION_V1 });
    }
    let body_len = u64r(reader)?;
    if body_len > total.saturating_sub(30) {
        return Err(PalwRowStreamErrorV1::TruncatedGeneration);
    }
    let inventory = {
        let mut body = (&mut *reader).take(body_len);
        let n = u32r(&mut body)? as usize;
        if n > PALW_ROW_STREAM_MAX_PREIMAGE_BYTES_V1 {
            return Err(PalwRowStreamErrorV1::PreimageTooLarge { len: n, max: PALW_ROW_STREAM_MAX_PREIMAGE_BYTES_V1 });
        }
        let _preimage = reserve(n as u64)?;
        let mut preimage = vec![0; n];
        body.read_exact(&mut preimage).map_err(io_error)?;
        let collections = u32r(&mut body)? as usize;
        if collections > n / 64 {
            return Err(PalwRowStreamErrorV1::TruncatedGeneration);
        }
        let meta = crate::palw_native_resource_v1::btree_node_envelope_bytes_v1::<Vec<u8>, ()>()
            + std::mem::size_of::<(Vec<u8>, u64)>() * 6
            + std::mem::size_of::<usize>() * 6
            + PALW_ROW_STREAM_MAX_LABEL_BYTES_V1 * 6;
        let _metadata = reserve((collections as u64).checked_mul(meta as u64).ok_or(PalwRowStreamErrorV1::ByteBudget(total))?)?;
        let mut verifier = PalwRowStreamVerifierV1::new(expected_root, preimage, std::io::sink(), total)?;
        for _ in 0..collections {
            let n = u32r(&mut body)? as usize;
            if n > PALW_ROW_STREAM_MAX_LABEL_BYTES_V1 {
                return Err(PalwRowStreamErrorV1::TruncatedGeneration);
            }
            let mut label = vec![0; n];
            body.read_exact(&mut label).map_err(io_error)?;
            let rows = u64r(&mut body)?;
            let offset = usize::try_from(u64r(&mut body)?).map_err(|_| PalwRowStreamErrorV1::TruncatedGeneration)?;
            if rows > body.limit() / 8 {
                return Err(PalwRowStreamErrorV1::TruncatedGeneration);
            }
            match projection::for_label(&label) {
                Some(project) => verifier.begin_projected_collection(&label, rows, offset, project)?,
                None => verifier.begin_collection(&label, rows, offset)?,
            }
            for _ in 0..rows {
                let k = u32r(&mut body)? as usize;
                if k as u64 > body.limit() {
                    return Err(PalwRowStreamErrorV1::TruncatedGeneration);
                }
                let _key = reserve((k as u64).checked_mul(3).ok_or(PalwRowStreamErrorV1::ByteBudget(total))?)?;
                let mut key = vec![0; k];
                body.read_exact(&mut key).map_err(io_error)?;
                let v = u32r(&mut body)? as usize;
                if v as u64 > body.limit() {
                    return Err(PalwRowStreamErrorV1::TruncatedGeneration);
                }
                let _value = reserve(
                    (v as u64).checked_mul(2).and_then(|n| n.checked_add(64)).ok_or(PalwRowStreamErrorV1::ByteBudget(total))?,
                )?;
                let mut value = vec![0; v];
                body.read_exact(&mut value).map_err(io_error)?;
                verifier.row(&key, &value)?;
            }
            verifier.end_collection()?;
        }
        let mut end = [0];
        body.read_exact(&mut end).map_err(io_error)?;
        if end[0] != PALW_ROW_SPILL_END_V1 || body.limit() != 0 {
            return Err(PalwRowStreamErrorV1::TruncatedGeneration);
        }
        verifier.finish()?.inventory()
    };
    let length = u64r(reader)?;
    let offset = reader.stream_position().map_err(io_error)?;
    if offset.checked_sub(start).and_then(|n| n.checked_add(length)) != Some(total) {
        return Err(PalwRowStreamErrorV1::TruncatedGeneration);
    }
    Ok(PalwRowSuffixV1 { offset, length, inventory })
}
