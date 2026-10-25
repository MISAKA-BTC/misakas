//! Exact-unit provenance from the bounded reader, never a last-request guess.
use super::ProviderReaderErrorV4;
use misaka_palw_kernel::claim_paid_v2::PaidClaimTargetV2;

/// One acquisition account for every adapter target (Program, Pipeline,
/// TypedSpec, Segmented), returned on success and on a reader failure alike.
/// Counts provider unit requests, authenticated raw bytes and the bounded
/// workspace bytes this reader charged. It is an IO/byte account only: checker
/// CPU work, process RSS and court cost are not measured here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProviderAcquisitionReportV4 {
    pub target: PaidClaimTargetV2,
    pub unit_requests: u32,
    pub fetched_bytes: u64,
    pub workspace_bytes_charged: u64,
}

#[derive(Debug)]
pub struct ProviderReaderFailureV4 {
    cause: ProviderReaderErrorV4,
    invalid_unit: Option<(u8, u32)>,
    report: Option<ProviderAcquisitionReportV4>,
}
impl ProviderReaderFailureV4 {
    pub fn cause(&self) -> &ProviderReaderErrorV4 {
        &self.cause
    }
    /// A raw unit's own failed framing/decode/authentication, not a cross-unit
    /// tensor/position reconstruction failure. Still no native duty authority.
    pub fn invalid_unit(&self) -> Option<(u8, u32)> {
        self.invalid_unit
    }
    /// None only when the failure precedes any reader (limits/binding/ledger).
    pub fn acquisition_report(&self) -> Option<ProviderAcquisitionReportV4> {
        self.report
    }
    pub fn into_cause(self) -> ProviderReaderErrorV4 {
        self.cause
    }
    pub(super) fn from_reader(
        cause: ProviderReaderErrorV4,
        unit: Option<(u8, u32)>,
        report: ProviderAcquisitionReportV4,
    ) -> Self {
        let invalid_unit = if matches!(&cause, ProviderReaderErrorV4::InvalidMaterial(_)) { unit } else { None };
        Self { cause, invalid_unit, report: Some(report) }
    }
}
impl From<ProviderReaderErrorV4> for ProviderReaderFailureV4 {
    fn from(cause: ProviderReaderErrorV4) -> Self {
        Self { cause, invalid_unit: None, report: None }
    }
}
