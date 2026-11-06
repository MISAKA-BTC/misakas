//! Configured pre-admission for the native-File/same-Source-byte startup route.
//! This is arithmetic only, not a replacement parser or a supported-build token.
//! The Node caller FIRST requires Core's audited Source profile, retains its
//! immutable same-byte Source owner for split validation, and supplies exactly
//! std::fs::File. Arbitrary Read implementations are outside this census.
use super::{BoundedPalwTirHeaderV1, PalwTirHeaderLimitsV1, SCRATCH_BYTES};
use crate::{PALW_TIR_MAX_HEADER_BYTES_V1, PREFIX, PalwTirContainerError};
use std::mem::size_of;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfiguredHeaderQuoteErrorV1 {
    ZeroLimit,
    Overflow,
    Limit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConfiguredSameBytesFileHeaderQuoteV1 {
    pub frame_bytes_upper: u64,
    pub retained_native_bytes_upper: u64,
    pub diagnostic_requests: u64,
    pub new_stage_peak_bytes: u64,
    pub operation_peak_bytes: u64,
    pub read_bytes_upper: u64,
    pub parser_work_upper: u64,
}
use ConfiguredHeaderQuoteErrorV1 as E;
fn add(a: u64, b: u64) -> Result<u64, E> {
    a.checked_add(b).ok_or(E::Overflow)
}
fn mul(a: u64, b: u64) -> Result<u64, E> {
    a.checked_mul(b).ok_or(E::Overflow)
}
// Every current bounded_v1 owned Header/Table/Program diagnostic literal,
// including cap messages. String::from(&str) requests exactly the literal's
// byte length; these branches neither append, Debug-render nor format input.
const LITERALS: &[&str] = &[
    "64 bytes",
    "allocator exceeded the exact metadata request",
    "bounded metadata addition overflow",
    "bounded metadata allocation refused",
    "bounded metadata multiplication overflow",
    "bounded metadata native width",
    "caller program schedule is not validated",
    "decoded input count format ceiling",
    "decoded metadata native ceiling",
    "decoded node count format ceiling",
    "decoded original differs from header program bytes",
    "decoded program structural format ceiling",
    "extra tensor instances",
    "file byte ceiling",
    "header byte ceiling",
    "header frame native width",
    "header parser work ceiling",
    "header runs past file end",
    "invalid tensor layer option tag",
    "layer index width",
    "layout byte ceiling",
    "layout commit count exceeds existing format bound",
    "layout frame ends early",
    "layout state count exceeds existing format bound",
    "layout word count overflow",
    "metadata copy ceiling",
    "metadata count exceeds native indices",
    "metadata field count ceiling",
    "metadata frame ends early",
    "metadata length exceeds remaining frame",
    "metadata operation ceiling",
    "metadata provenance is not UTF-8",
    "misaligned or overlapping tensor extent",
    "missing original parameter instance",
    "parameter index width",
    "program byte ceiling",
    "program bytes differ",
    "program count failed",
    "provenance byte ceiling",
    "split validation work ceiling",
    "tensor count ceiling",
    "tensor index overflow",
    "tensor order/type geometry differs from original program",
    "tensor runs past file end",
    "trailing layout bytes",
    "trailing metadata bytes",
    "unsupported explicit layout version",
    "unsupported header version",
];
/// Pinned std native File read/seek failures are inline OS error codes or
/// constant EOF errors, not arbitrary provider-owned errors. No error Display
/// or to_string is called. Same-byte Borsh object_length/Compare cannot fail:
/// the program is the retained canonical Source decode of these immutable bytes.
/// This helper MUST NOT be reused to admit mismatched programs or generic Read.
pub fn quote_configured_same_bytes_file_header_v1(
    file_len: u64,
    l: PalwTirHeaderLimitsV1,
) -> Result<ConfiguredSameBytesFileHeaderQuoteV1, E> {
    if l.max_file_bytes == 0 || l.max_header_bytes == 0 || l.max_native_bytes == 0 || l.max_work == 0 || l.max_operation_bytes == 0 {
        return Err(E::ZeroLimit);
    }
    if file_len > l.max_file_bytes || l.max_native_bytes < size_of::<BoundedPalwTirHeaderV1>() as u64 {
        return Err(E::Limit);
    }
    let frame = l.max_header_bytes.min(u64::from(PALW_TIR_MAX_HEADER_BYTES_V1)).min(file_len.saturating_sub(PREFIX));
    let mut literal = 0u64;
    for text in LITERALS {
        literal = literal.max(u64::try_from(text.len()).map_err(|_| E::Overflow)?);
    }
    let diagnostic_requests = add(size_of::<PalwTirContainerError>() as u64, literal)?;
    let new_stage_peak_bytes = add(add(frame, l.max_native_bytes)?, add(SCRATCH_BYTES, diagnostic_requests)?)?;
    let operation_peak_bytes = add(new_stage_peak_bytes, l.external_retained_bytes)?;
    let parser_work_upper = mul(add(add(frame, PREFIX)?, 1)?, 128)?;
    if operation_peak_bytes > l.max_operation_bytes || parser_work_upper > l.max_work {
        return Err(E::Limit);
    }
    Ok(ConfiguredSameBytesFileHeaderQuoteV1 {
        frame_bytes_upper: frame,
        retained_native_bytes_upper: l.max_native_bytes,
        diagnostic_requests,
        new_stage_peak_bytes,
        operation_peak_bytes,
        read_bytes_upper: add(frame, PREFIX)?,
        parser_work_upper,
    })
}
