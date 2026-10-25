//! Exact-unit provenance from the bounded reader, never a last-request guess.
use super::ProviderReaderErrorV4;

#[derive(Debug)]
pub struct ProviderReaderFailureV4 {
    cause: ProviderReaderErrorV4,
    invalid_unit: Option<(u8, u32)>,
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
    pub fn into_cause(self) -> ProviderReaderErrorV4 {
        self.cause
    }
    pub(super) fn from_reader(cause: ProviderReaderErrorV4, unit: Option<(u8, u32)>) -> Self {
        let invalid_unit = if matches!(&cause, ProviderReaderErrorV4::InvalidMaterial(_)) { unit } else { None };
        Self { cause, invalid_unit }
    }
}
impl From<ProviderReaderErrorV4> for ProviderReaderFailureV4 {
    fn from(cause: ProviderReaderErrorV4) -> Self {
        Self { cause, invalid_unit: None }
    }
}
