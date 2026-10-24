//! Typed acquisition boundaries. No display text is used to infer capacity or
//! remote misconduct. These helpers retain native resource causes before any
//! legacy String/Refusal compatibility conversion.
use super::*;
use kaspa_consensus_core::palw_native_resource_v1::{
    NativeCoverageV1, NativeDecodeErrorV1, native_kernel_parameter_auth_requests_v4, native_kernel_position_auth_requests_v4,
    native_kernel_segment_auth_requests_v4,
};

pub(super) fn capacity(error: impl ToString) -> ProviderReaderErrorV4 {
    ProviderReaderErrorV4::LocalCapacity(error.to_string())
}
pub(super) fn invalid(error: impl ToString) -> ProviderReaderErrorV4 {
    ProviderReaderErrorV4::InvalidMaterial(error.to_string())
}
pub(super) fn shape_error(error: std::io::Error) -> ProviderReaderErrorV4 {
    if matches!(error.kind(), std::io::ErrorKind::Unsupported | std::io::ErrorKind::OutOfMemory) {
        capacity(error)
    } else {
        invalid(error)
    }
}
pub(super) fn classified_decode<E: std::fmt::Display>(error: NativeDecodeErrorV1<E>) -> ProviderReaderErrorV4 {
    match error {
        NativeDecodeErrorV1::Resource(error) => capacity(error),
        NativeDecodeErrorV1::Decode(error) => invalid(error),
    }
}
pub(super) fn projection_error<E: std::fmt::Display>(error: NativeDecodeErrorV1<E>) -> ProviderReaderErrorV4 {
    match error {
        NativeDecodeErrorV1::Resource(error) => capacity(error),
        NativeDecodeErrorV1::Decode(error) => bad(error),
    }
}
fn decode<T: NativeWireScanV1 + borsh::BorshDeserialize>(bytes: &[u8]) -> Result<NativeRetainedV1<T>, ProviderReaderErrorV4> {
    preflight_native_value_v1::<T>(bytes, bytes.len() as u64).map_err(shape_error)?;
    retain_native_value_decode_classified_v1(bytes, || borsh::from_slice(bytes)).map_err(classified_decode)
}
fn quoted<T: NativeWireScanV1 + borsh::BorshDeserialize>(
    bytes: &[u8],
    quote: impl FnOnce(&T) -> Result<u64, NativeCoverageV1>,
) -> Result<u64, ProviderReaderErrorV4> {
    let value = decode::<T>(bytes)?;
    quote(&value).map_err(capacity)
}
/// The same native authentication request functions as the historical core
/// String adapter, with explicit retained decoder owners. The caller separately
/// admits codec storage and the returned authentication request before use.
pub(super) fn auth_requests(stage: u8, bytes: &[u8], ledger: &KernelLedgerV1, claim: &Digest) -> Result<u64, ProviderReaderErrorV4> {
    use kaspa_consensus_core::palw_kernel_parameter_material_v4::{PALW_KERNEL_SEGMENTED_PART_STAGE_V4, PalwKernelSegmentedPartV4};
    // Validate the existing exact structural quote even when its caller already
    // did so. Its opaque String errors remain Unknown, never text-classified.
    kernel_material_unit_requests_v4(stage, bytes).map_err(bad)?;
    if stage == PALW_KERNEL_PARAMETER_STAGE_V4 {
        quoted::<PalwKernelParameterRowV4>(bytes, native_kernel_parameter_auth_requests_v4)
    } else if stage == PALW_KERNEL_SEGMENTED_PART_STAGE_V4 {
        let unit = decode::<PalwKernelSegmentedPartV4>(bytes)?;
        let row = ledger.claims.get(claim).ok_or_else(|| bad("unit authentication claim absent"))?;
        let class = ledger.classes.get(&row.class_binding_id).ok_or_else(|| bad("unit authentication class absent"))?;
        quoted::<misaka_palw_kernel::seg_da::SegPartResponseV1>(&unit.bytes, |part| {
            native_kernel_segment_auth_requests_v4(&class.program, unit.position, part)
        })
    } else if stage < misaka_palw_kernel::spec::SNAPSHOT_STAGE_BASE_V1 {
        quoted::<PositionResponseV1>(bytes, native_kernel_position_auth_requests_v4)
    } else {
        // Snapshot/entry projections are counted by the existing structural
        // quote; their borrowed authentication does not build a snapshot tree.
        Ok(0)
    }
}
