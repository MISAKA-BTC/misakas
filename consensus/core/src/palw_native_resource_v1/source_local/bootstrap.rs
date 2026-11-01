//! Private pinned-std diagnostic arithmetic; no decoder or native authority.
//!
//! Audited scope: Rust1.93.0, Borsh1.5.1, 64-bit little-endian Linux/macOS,
//! borrowed-memory Source scanner only. Every value is a type, literal length,
//! checked geometric request or a finite branch maximum. No guessed scratch
//! constant, codec-byte multiplier, arbitrary Read error or public support flag.

use super::{Error, LocalSourceProgramOwnerV1, Result, ScannedSourceProgramLocalV1, SourceProgramLocalQuoteV1, add, mul, native_size};
use crate::palw_native_resource_v1::{
    BTreeNodeEnvelopeV1 as Tree, NativeDecodeErrorV1, TirNativeRequestsV1, growing_vec_request_capacity_v1,
    ledger_expansion::SourceProgramScanObservationV1,
};
use misaka_palw_tir::{
    TirError,
    program::TirProgramV1,
    types::{DType, Dim, TensorType},
    validate::BlockInfo,
};
use std::{any::type_name, collections::BTreeSet, error::Error as StdError, io, mem::align_of, string::FromUtf8Error};

fn sum(parts: &[u64]) -> Result<u64> {
    parts.iter().try_fold(0, |a, b| add(a, *b))
}
fn grow(bytes: u64) -> Result<u64> {
    growing_vec_request_capacity_v1::<u8>(bytes).ok_or(Error::ArithmeticOverflow)
}
fn length(text: &str) -> Result<u64> {
    u64::try_from(text.len()).map_err(|_| Error::ArithmeticOverflow)
}
fn coverage_backing(component: &str, native_type: &str, reason: Option<&str>) -> Result<u64> {
    // String descriptors are inside NativeDecodeErrorV1< String > already.
    sum(&[grow(length(component)?)?, grow(length(native_type)?)?, grow(length(reason.unwrap_or(""))?)?])
}
fn io_custom_backing() -> Result<u64> {
    // Rust1.93 Custom: ErrorKind + Box<dyn Error+Send+Sync>, repr(align(4)).
    // Two field-order/trailing alignment boundaries bound the pinned box's
    // requested layout. This is not a cross-version ABI or allocator-RSS claim.
    let alignment = 4usize.max(align_of::<io::ErrorKind>()).max(align_of::<Box<dyn StdError + Send + Sync>>());
    let padding = mul(u64::try_from(alignment - 1).map_err(|_| Error::ArithmeticOverflow)?, 2)?;
    sum(&[native_size::<io::ErrorKind>()?, native_size::<Box<dyn StdError + Send + Sync>>()?, padding])
}

pub(super) fn scan_bootstrap_requests_v1() -> Result<u64> {
    if !super::supported_build() {
        return Err(Error::UnsupportedCoverage);
    }
    let headers = sum(&[
        native_size::<Error>()?,
        native_size::<NativeDecodeErrorV1<String>>()?,
        native_size::<ScannedSourceProgramLocalV1<'static>>()?,
        native_size::<SourceProgramScanObservationV1>()?,
    ])?;
    // Current finite scanner literal inventory: maximum47 UTF8 bytes; std
    // memory EOF27; direct Source trailing35. No input text is interpolated.
    // StringError is the pinned std single-String boxed wrapper.
    let io_owned = sum(&[native_size::<io::Error>()?, io_custom_backing()?, native_size::<String>()?, grow(47)?])?;
    let mut peak = sum(&[headers, io_owned, grow(47)?])?;
    peak = peak.max(sum(&[headers, native_size::<io::Error>()?, grow(27)?])?);
    peak = peak.max(add(headers, grow(35)?)?);
    let program_type = type_name::<TirProgramV1>();
    peak = peak.max(sum(&[headers, io_owned, coverage_backing("Source program wire shape", program_type, None)?])?);
    peak = peak.max(sum(&[
        headers,
        io_owned,
        coverage_backing("Source program wire shape", program_type, Some("local wire exploration/platform coverage"))?,
    ])?);
    peak = peak.max(add(headers, coverage_backing("Source normal-form cardinality", program_type, None)?)?);
    // Exactly the finite q error type family, using actual compile-time names.
    // MAX across alternative failures, never SUM of all possible messages.
    for native_type in [
        type_name::<TirProgramV1>(),
        type_name::<u8>(),
        type_name::<bool>(),
        type_name::<usize>(),
        type_name::<Dim>(),
        type_name::<TensorType>(),
        type_name::<BlockInfo>(),
        type_name::<Tree<&str, ()>>(),
        type_name::<Tree<(DType, Vec<u32>, Vec<u8>), ()>>(),
        type_name::<Tree<u16, ()>>(),
        type_name::<Tree<&str, BTreeSet<u16>>>(),
        type_name::<TirNativeRequestsV1>(),
    ] {
        for component in ["TIR concrete array request", "TIR growing Vec capacity", "coexisting TIR typed requests"] {
            peak = peak.max(add(headers, coverage_backing(component, native_type, None)?)?);
        }
    }
    Ok(peak)
}

/// Payloads are ALREADY funded by Source q's four growing diagnostic buffers.
/// Add only actual error/String headers and pinned std box backing. Taking the
/// two phase maximum explicitly covers inner/outer formatting and the Borsh
/// invalid-UTF8 Vec owner -> StringError/Custom -> TirError conversion. No owned
/// error escapes and there is no additional terminal error.to_string here.
pub(super) fn decode_diagnostic_extra_v1() -> Result<u64> {
    // The quote's inline storage is already in the owner delta. The consuming
    // same-bytes wrapper additionally retains its slice coordinate during
    // decode; Result framing is derived from the actual selected Rust types.
    let frame = native_size::<ScannedSourceProgramLocalV1<'static>>()?
        .checked_sub(native_size::<SourceProgramLocalQuoteV1>()?)
        .ok_or(Error::UnsupportedCoverage)?;
    let returned = native_size::<Result<LocalSourceProgramOwnerV1>>()?
        .checked_sub(native_size::<LocalSourceProgramOwnerV1>()?)
        .ok_or(Error::UnsupportedCoverage)?;
    let decoded = native_size::<std::result::Result<TirProgramV1, TirError>>()?
        .checked_sub(native_size::<TirProgramV1>()?)
        .ok_or(Error::UnsupportedCoverage)?;
    let utf8 = sum(&[
        native_size::<FromUtf8Error>()?,
        native_size::<io::Error>()?,
        io_custom_backing()?,
        native_size::<String>()?, // Box<StringError(String)>
        native_size::<TirError>()?,
        native_size::<Error>()?,
    ])?;
    let nf = sum(&[
        native_size::<String>()?, // Inner shape_err String overlaps outer TirError.
        native_size::<TirError>()?,
        native_size::<Error>()?,
    ])?;
    sum(&[frame, returned, decoded, utf8.max(nf)])
}

/// Finite typed Display census of current validate.rs and Borsh1.5.1. This does
/// not publish the private NF allocation quote or assign all-NF CPU coverage.
/// No arbitrary program Debug rendering is used by this decode path.
pub(super) fn check_nf_text_envelope_v1(items: u64, program_bytes: u64) -> Result<()> {
    let source_writer_text = add(mul(items, 60)?, 1024)?;
    // Dim Debug <=17B; Vec separators give19B/Dim. Gather may duplicate an
    // operand rank; HistAppend adds H. Full typed literal/slot census:
    // want_shape <=57n+472; reshape <=38n+438; perm <=5n+465; scalar <=822.
    let mut text = 822u64;
    for (factor, fixed) in [(57, 472), (38, 438), (5, 465)] {
        text = text.max(add(mul(items, factor)?, fixed)?);
    }
    // Utf8Error Display has fixed literal skeletons44/42, u8 error_len (<=3
    // decimal digits), and usize index bounded by source byte count. It never
    // Debug-escapes or prints the invalid byte Vec. Include TirError prefix12.
    let digits = if program_bytes == 0 { 1 } else { u64::from(program_bytes.ilog10()) + 1 };
    text = text.max(add(add(44 + 3, digits)?, 12)?);
    // Pinned format! initial estimate <=2*max literal length64 =128. The
    // existing growing Vec envelope then funds each writer's final text.
    let needed_capacity = grow(text.max(128))?;
    if text > source_writer_text || needed_capacity > grow(source_writer_text)? {
        return Err(Error::UnsupportedCoverage);
    }
    Ok(())
}
