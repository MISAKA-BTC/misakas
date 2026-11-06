//! Bounded private replay journal. Only signed commitments and authenticated
//! evidence are authoritative; no saved pass flag, cursor or fee total is read.
//! This is NOT a native state snapshot/IBD codec. Native recovery must bind the
//! carrier's branch and consensus time before interpreting any historical event.
use super::*;
use std::io::{Read, Write};

const MAGIC: &[u8; 8] = b"PALWPCJ2";
const HEADER: u64 = 8 + 3 * 64;

fn identities(case: &ExactCaseV2) -> [Hash64; 3] {
    [case.result.id(), case.witness.id(), case.policy.id()]
}
fn io_error(_: std::io::Error) -> ErrorV2 {
    ErrorV2::Wire
}

/// Writes the identities of a freshly admitted case. Role signatures/registered
/// class/policy are supplied and checked again by admission on restart.
pub fn write_header<W: Write>(case: &ExactCaseV2, writer: &mut W) -> Result<(), ErrorV2> {
    writer.write_all(MAGIC).map_err(io_error)?;
    for id in identities(case) {
        writer.write_all(id.as_byte_slice()).map_err(io_error)?;
    }
    Ok(())
}
pub fn write_part<W: Write>(writer: &mut W, cone: &[u8], trace: Option<&[u8]>, part_cap: u64) -> Result<(), ErrorV2> {
    let len = (cone.len() as u64).checked_add(trace.map_or(0, |x| x.len() as u64)).ok_or(ErrorV2::Budget)?;
    if len > part_cap {
        return Err(ErrorV2::Budget);
    }
    let cone_len = u32::try_from(cone.len()).map_err(|_| ErrorV2::Budget)?;
    let trace_len = trace.map(|x| u32::try_from(x.len()).map_err(|_| ErrorV2::Budget)).transpose()?;
    if trace_len == Some(u32::MAX) {
        return Err(ErrorV2::Budget);
    }
    writer.write_all(&[1]).map_err(io_error)?;
    writer.write_all(&cone_len.to_le_bytes()).map_err(io_error)?;
    // u32::MAX denotes absence and cannot allocate a vector during replay.
    writer.write_all(&trace_len.unwrap_or(u32::MAX).to_le_bytes()).map_err(io_error)?;
    writer.write_all(cone).map_err(io_error)?;
    if let Some(bytes) = trace {
        writer.write_all(bytes).map_err(io_error)?;
    }
    Ok(())
}
/// Merely requests a fresh full-coverage finish check during replay.
pub fn write_finish<W: Write>(writer: &mut W) -> Result<(), ErrorV2> {
    writer.write_all(&[2]).map_err(io_error)
}

/// Resume a freshly admitted case using the receiving Node's current clock.
/// No previously recorded clock can extend a deadline. A complete record prefix
/// without a finish record stays Checking; malformed/truncated frames fail.
pub fn replay<R: Read>(case: &mut ExactCaseV2, reader: &mut R, now: u64) -> Result<ClaimStatusV2, ErrorV2> {
    case.ensure_open(now)?;
    if case.cursor() != 0 {
        return Err(ErrorV2::Order);
    }
    let bound = HEADER
        .checked_add(case.policy.max_total_bytes)
        .and_then(|x| case.witness.binding.step_leaf_count.checked_mul(9).and_then(|n| x.checked_add(n)))
        .and_then(|x| x.checked_add(1))
        .ok_or(ErrorV2::Budget)?;
    let mut reader = reader.take(bound);
    let mut magic = [0; 8];
    reader.read_exact(&mut magic).map_err(io_error)?;
    if &magic != MAGIC {
        return Err(ErrorV2::Binding);
    }
    for expected in identities(case) {
        let mut id = [0; 64];
        reader.read_exact(&mut id).map_err(io_error)?;
        if Hash64::from_bytes(id) != expected {
            return Err(ErrorV2::Binding);
        }
    }
    loop {
        let mut tag = [0];
        if reader.read(&mut tag).map_err(io_error)? == 0 {
            return Ok(case.statuses().0);
        }
        match tag[0] {
            1 => {
                if case.cursor() >= case.witness.binding.step_leaf_count {
                    return Err(ErrorV2::Order);
                }
                let mut size = [0; 4];
                reader.read_exact(&mut size).map_err(io_error)?;
                let cone_len = u32::from_le_bytes(size) as u64;
                reader.read_exact(&mut size).map_err(io_error)?;
                let trace_len = u32::from_le_bytes(size);
                let actual_trace_len = if trace_len == u32::MAX { 0 } else { trace_len as u64 };
                let frame = cone_len.checked_add(actual_trace_len).ok_or(ErrorV2::Budget)?;
                if frame > case.policy.max_part_bytes
                    || frame > case.policy.max_total_bytes.saturating_sub(case.bytes)
                    || frame > reader.limit()
                {
                    return Err(ErrorV2::Budget);
                }
                // Allocation is bounded by the actual locally admitted part cap.
                // Borsh's inner vector lengths are scanned by submit_encoded.
                let mut cone = vec![0; usize::try_from(cone_len).map_err(|_| ErrorV2::Budget)?];
                reader.read_exact(&mut cone).map_err(io_error)?;
                let mut trace = vec![0; usize::try_from(actual_trace_len).map_err(|_| ErrorV2::Budget)?];
                reader.read_exact(&mut trace).map_err(io_error)?;
                case.submit_encoded(&cone, if trace_len == u32::MAX { None } else { Some(&trace) }, now)?;
            }
            2 => {
                // Do not mutate to Passed until the entire envelope ends.
                let mut extra = [0];
                if reader.read(&mut extra).map_err(io_error)? != 0 {
                    return Err(ErrorV2::Wire);
                }
                if reader.into_inner().read(&mut extra).map_err(io_error)? != 0 {
                    return Err(ErrorV2::Wire);
                }
                return case.finish(now);
            }
            _ => return Err(ErrorV2::Wire),
        }
    }
}
