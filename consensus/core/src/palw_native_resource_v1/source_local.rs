//! Explicit, owner-local Source program preparation, independent of native flags.
//!
//! The quote is a logical allocation-request envelope for the existing memory-only
//! scanner and canonical decoder/normal-form validator. It is not a host lease,
//! global concurrency budget, allocator/RSS bound or guarantee of total validator
//! CPU. Compilation, StepSpace, artifact authentication and model execution are
//! outside this API. Canonical data is not a model review or computation verdict.
//!
//! Backend owned diagnostic strings are classified and dropped inside this module;
//! only a fixed-size public error escapes. Caller-created formatting, program
//! clones, retained input bytes and simultaneous owners need separate reservations.

mod bootstrap;
#[cfg(test)]
mod tests;

use super::{
    NativeCoverageV1, NativeDecodeErrorV1,
    ledger_expansion::{SourceProgramScanObservationV1, source_tir_program_shape_observed_v1},
};
use misaka_palw_tir::{TirErrorKind, TirProgramV1, program::MAX_PROGRAM_BYTES};
use std::{fmt, mem::size_of};

/// Positive, explicit local ceilings. These do not activate native admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceProgramLocalLimitsV1 {
    pub max_program_bytes: u64,
    pub max_scan_steps: u64,
    pub max_nf_items: u64,
    pub max_native_requests: u64,
}

/// No owned diagnostic allocation occurs when constructing these errors.
/// Encoding, normal-form and shape failures retain the canonical decoder's kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceProgramLocalErrorV1 {
    ZeroLimit,
    ProgramBytes,
    ScanSteps,
    NormalFormItems,
    NativeRequests,
    ArithmeticOverflow,
    UnsupportedCoverage,
    Encoding,
    NormalForm,
    Shape,
}
impl fmt::Display for SourceProgramLocalErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ZeroLimit => "local Source limits must be positive",
            Self::ProgramBytes => "local Source program byte limit",
            Self::ScanSteps => "local Source scanner step limit",
            Self::NormalFormItems => "local Source structural item limit",
            Self::NativeRequests => "local Source native request limit",
            Self::ArithmeticOverflow => "local Source arithmetic overflow",
            Self::UnsupportedCoverage => "local Source coverage unavailable",
            Self::Encoding => "Source program encoding",
            Self::NormalForm => "Source program normal form",
            Self::Shape => "Source program shape",
        })
    }
}
impl std::error::Error for SourceProgramLocalErrorV1 {}
type Error = SourceProgramLocalErrorV1;
type Result<T> = std::result::Result<T, Error>;

fn add(a: u64, b: u64) -> Result<u64> {
    a.checked_add(b).ok_or(Error::ArithmeticOverflow)
}
fn mul(a: u64, b: u64) -> Result<u64> {
    a.checked_mul(b).ok_or(Error::ArithmeticOverflow)
}
fn native_size<T>() -> Result<u64> {
    u64::try_from(size_of::<T>()).map_err(|_| Error::ArithmeticOverflow)
}

/// Private build-script evidence, never a caller/peer supplied support boolean.
/// The script additionally checks exact compiler and dependency versions.
fn supported_build() -> bool {
    cfg!(all(
        misaka_source_local_audited_rust193_v1,
        target_pointer_width = "64",
        target_endian = "little",
        any(target_os = "linux", target_os = "macos")
    )) && matches!(option_env!("MISAKA_SOURCE_LOCAL_BUILD_PROFILE_V1"), Some("rust-1.93.0-borsh-1.5.1-v1"))
}

/// Require the private audited Source build profile without parsing bytes,
/// allocating diagnostics, consuming scanner work or granting native authority.
/// Owner-local startup uses this before its native-File diagnostic census.
/// It is not a caller-supplied support boolean or installed-source attestation.
/// The scanner independently checks the same private profile on every attempt.
pub fn require_source_program_local_profile_v1() -> std::result::Result<(), SourceProgramLocalErrorV1> {
    if supported_build() { Ok(()) } else { Err(Error::UnsupportedCoverage) }
}

/// Persistent logical accounting across attempts, including failures and retries.
/// Observations are evidence, not another debit or a refund of precharged work.
/// This owner is deliberately neither Clone nor Copy and offers no reset.
pub struct LocalScanWorkBudgetV1 {
    limit: u64,
    charged_steps: u64,
    observed_steps: u64,
    charged_reader_bytes: u64,
    observed_reader_bytes: u64,
    attempts: u64,
}
impl LocalScanWorkBudgetV1 {
    pub fn new(max_total_scan_steps: u64) -> Result<Self> {
        if max_total_scan_steps == 0 {
            return Err(Error::ZeroLimit);
        }
        Ok(Self {
            limit: max_total_scan_steps,
            charged_steps: 0,
            observed_steps: 0,
            charged_reader_bytes: 0,
            observed_reader_bytes: 0,
            attempts: 0,
        })
    }
    pub fn limit(&self) -> u64 {
        self.limit
    }
    pub fn charged_steps(&self) -> u64 {
        self.charged_steps
    }
    pub fn observed_steps(&self) -> u64 {
        self.observed_steps
    }
    /// Sum of reader-boundary bytes, including nested traversal and u32 framing.
    /// This is not unique input bytes, filesystem IO or hardware reads.
    pub fn charged_reader_bytes(&self) -> u64 {
        self.charged_reader_bytes
    }
    pub fn observed_reader_bytes(&self) -> u64 {
        self.observed_reader_bytes
    }
    pub fn attempts(&self) -> u64 {
        self.attempts
    }
    fn charge(&mut self, steps: u64, bytes: u64) -> Result<()> {
        let next_steps = add(self.charged_steps, steps)?;
        let next_bytes = add(self.charged_reader_bytes, bytes)?;
        let next_attempts = add(self.attempts, 1)?;
        if next_steps > self.limit {
            return Err(Error::ScanSteps);
        }
        // Commit all charged counters together, before scanner entry.
        self.charged_steps = next_steps;
        self.charged_reader_bytes = next_bytes;
        self.attempts = next_attempts;
        Ok(())
    }
    fn observe(&mut self, observation: &SourceProgramScanObservationV1, upper_steps: u64, upper_bytes: u64) -> Result<()> {
        let (steps, bytes, readers) = observation.totals_v1().ok_or(Error::UnsupportedCoverage)?;
        if readers == 0 || readers > 4 || steps > upper_steps || bytes > upper_bytes {
            return Err(Error::UnsupportedCoverage);
        }
        let next_steps = add(self.observed_steps, steps)?;
        let next_bytes = add(self.observed_reader_bytes, bytes)?;
        if next_steps > self.charged_steps || next_bytes > self.charged_reader_bytes {
            return Err(Error::UnsupportedCoverage);
        }
        self.observed_steps = next_steps;
        self.observed_reader_bytes = next_bytes;
        Ok(())
    }
}

/// Scalar provenance of this exact same-bytes scan. No public constructor.
/// The existing Source quote already includes four diagnostic byte writers.
/// `diagnostic_requests` adds only typed decode error/header backing. Admission
/// is MAX(scan-bootstrap phase, Source + diagnostic extra + owner delta), not
/// a second charge for those four writers or a sum of sequential phases.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceProgramLocalQuoteV1 {
    program_bytes: u64,
    scan_steps_upper: u64,
    reader_bytes_upper: u64,
    nf_items: u64,
    source_native_requests: u64,
    diagnostic_requests: u64,
    owner_requests: u64,
    admitted_native_requests: u64,
}
impl SourceProgramLocalQuoteV1 {
    pub fn program_bytes(&self) -> u64 {
        self.program_bytes
    }
    pub fn scan_steps_upper(&self) -> u64 {
        self.scan_steps_upper
    }
    pub fn reader_bytes_upper(&self) -> u64 {
        self.reader_bytes_upper
    }
    pub fn nf_items(&self) -> u64 {
        self.nf_items
    }
    pub fn source_native_requests(&self) -> u64 {
        self.source_native_requests
    }
    pub fn diagnostic_requests(&self) -> u64 {
        self.diagnostic_requests
    }
    pub fn owner_requests(&self) -> u64 {
        self.owner_requests
    }
    pub fn admitted_native_requests(&self) -> u64 {
        self.admitted_native_requests
    }
}

/// Borrow ties quote and consuming decode to precisely the scanned immutable bytes.
/// No replacement bytes, supplied shape/count, callback or unchecked constructor.
pub struct ScannedSourceProgramLocalV1<'a> {
    bytes: &'a [u8],
    quote: SourceProgramLocalQuoteV1,
}
impl ScannedSourceProgramLocalV1<'_> {
    pub fn quote(&self) -> &SourceProgramLocalQuoteV1 {
        &self.quote
    }
    pub fn decode(self) -> Result<LocalSourceProgramOwnerV1> {
        // Exact existing canonical decoder and validation order, once. All its
        // owned error payloads remain local and are dropped before fixed return.
        match TirProgramV1::decode_canonical(self.bytes) {
            Ok(program) => Ok(LocalSourceProgramOwnerV1 { program, quote: self.quote }),
            Err(error) => {
                let kind = match error.kind {
                    TirErrorKind::Encoding => Error::Encoding,
                    TirErrorKind::NormalForm => Error::NormalForm,
                    TirErrorKind::Shape => Error::Shape,
                    // Runtime-only kinds have no audited decode phase here.
                    _ => Error::UnsupportedCoverage,
                };
                drop(error);
                Err(kind)
            }
        }
    }
}

/// Immutable logical owner, with no native checker/ticket/lease or deep Clone.
/// The full preparation quote stays attached after temporary scratch is freed.
/// Explicit copies through the borrowed program getter are caller-owned exports.
pub struct LocalSourceProgramOwnerV1 {
    program: TirProgramV1,
    quote: SourceProgramLocalQuoteV1,
}
impl LocalSourceProgramOwnerV1 {
    pub fn program(&self) -> &TirProgramV1 {
        &self.program
    }
    pub fn quote(&self) -> &SourceProgramLocalQuoteV1 {
        &self.quote
    }
}

fn scan_upper(program_bytes: u64) -> Result<(u64, u64)> {
    // NativeWireReader increments BEFORE refusing L(x), hence four +1s.
    // Passes: outer(n+4), nested(n), StepSpace(n), raw Source(n).
    let intrinsic = |bytes| add(mul(bytes, 8)?, 4096);
    let steps = add(add(intrinsic(add(program_bytes, 4)?)?, mul(intrinsic(program_bytes)?, 3)?)?, 4)?;
    Ok((steps, add(mul(program_bytes, 4)?, 4)?))
}
fn classify_scan(error: NativeDecodeErrorV1<String>) -> Error {
    let kind = match &error {
        NativeDecodeErrorV1::Decode(_) => Error::Encoding,
        NativeDecodeErrorV1::Resource(NativeCoverageV1::Overflow { .. }) => Error::ArithmeticOverflow,
        NativeDecodeErrorV1::Resource(_) => Error::UnsupportedCoverage,
    };
    drop(error);
    kind
}

pub fn scan_source_program_local_v1<'a>(
    bytes: &'a [u8],
    limits: SourceProgramLocalLimitsV1,
    work: &mut LocalScanWorkBudgetV1,
) -> Result<ScannedSourceProgramLocalV1<'a>> {
    if limits.max_program_bytes == 0 || limits.max_scan_steps == 0 || limits.max_nf_items == 0 || limits.max_native_requests == 0 {
        return Err(Error::ZeroLimit);
    }
    let program_bytes = u64::try_from(bytes.len()).map_err(|_| Error::ArithmeticOverflow)?;
    if program_bytes > limits.max_program_bytes || program_bytes > MAX_PROGRAM_BYTES as u64 || program_bytes > u32::MAX as u64 {
        return Err(Error::ProgramBytes);
    }
    let (scan_steps_upper, reader_bytes_upper) = scan_upper(program_bytes)?;
    if scan_steps_upper > limits.max_scan_steps {
        return Err(Error::ScanSteps);
    }
    // Fixed, nonallocating limit refusals above retain their specified order.
    // Unknown profiles still cannot enter the allocating diagnostic backend,
    // even when a caller supplies arbitrarily large numeric limits.
    if !supported_build() {
        return Err(Error::UnsupportedCoverage);
    }
    let scan_bootstrap = bootstrap::scan_bootstrap_requests_v1()?;
    if scan_bootstrap > limits.max_native_requests {
        return Err(Error::NativeRequests);
    }
    work.charge(scan_steps_upper, reader_bytes_upper)?;
    let mut observed = SourceProgramScanObservationV1::default();
    let scanned = source_tir_program_shape_observed_v1(bytes, &mut observed);
    // Observe BOTH success and failure before propagating backend errors. Even
    // an observation coverage failure drops the backend owned error inside here.
    let observation = work.observe(&observed, scan_steps_upper, reader_bytes_upper);
    if let Err(error) = observation {
        drop(scanned);
        return Err(error);
    }
    let (shape, source_native_requests) = scanned.map_err(classify_scan)?;
    if !observed.complete_v1() {
        return Err(Error::UnsupportedCoverage);
    }
    let nf_items = shape.structural_items_v1();
    if nf_items > limits.max_nf_items {
        return Err(Error::NormalFormItems);
    }
    bootstrap::check_nf_text_envelope_v1(nf_items, program_bytes)?;
    let diagnostic_requests = bootstrap::decode_diagnostic_extra_v1()?;
    // The Source quote already includes the TirProgramV1 inline root.
    let owner_requests =
        native_size::<LocalSourceProgramOwnerV1>()?.checked_sub(native_size::<TirProgramV1>()?).ok_or(Error::UnsupportedCoverage)?;
    let decode_phase = add(add(source_native_requests, diagnostic_requests)?, owner_requests)?;
    let admitted_native_requests = scan_bootstrap.max(decode_phase);
    if admitted_native_requests > limits.max_native_requests {
        return Err(Error::NativeRequests);
    }
    Ok(ScannedSourceProgramLocalV1 {
        bytes,
        quote: SourceProgramLocalQuoteV1 {
            program_bytes,
            scan_steps_upper,
            reader_bytes_upper,
            nf_items,
            source_native_requests,
            diagnostic_requests,
            owner_requests,
            admitted_native_requests,
        },
    })
}
