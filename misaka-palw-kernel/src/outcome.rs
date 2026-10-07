//! **RFC-0011 §16.2's structured registration outcomes, and §16.4's coverage buckets.**
//!
//! An outcome says what a registration attempt found, independently of hardware. Only `EligibleAt` is
//! a success, and it is a success of *static* admission only: inclusion, readiness, mineability and a
//! market are separate facts, and the RFC-0014/ADR-0173 public-prosecution gate is separate again.

use crate::descriptor::KernelStatusV1;
use crate::family::ConstraintFamilyV1;
use crate::hash::{Digest, hex};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistrationOutcomeV1 {
    /// The whole task fits the active kernel's relations and every plan/admission bound.
    EligibleAt { daa: u64, descriptor: Digest, plan_root: Digest, error_bits: u16 },
    /// The importer/lowerer is missing or produced something the kernel's grammar refuses
    /// (an invalid program, a plan the reference frontend would not write).
    FrontendRequired { reason: String },
    /// A semantic primitive, verifier relation, memory rule or court is missing.
    KernelExtensionRequired { family: Option<ConstraintFamilyV1>, relation: String, required: String, available: String },
    /// The descriptor exists but is not active at this DAA (or is unknown to this binary).
    KernelNotActive { descriptor: Digest, status: Option<KernelStatusV1> },
    /// Mandatory node/DA/court bounds exceeded.
    BoundsExceeded { what: &'static str, required: u128, limit: u128 },
    /// The plan omits, duplicates or alters a relation, a boundary or a budget.
    IncompleteCoverage { what: String },
    /// The plan names another descriptor, another program, or declares more confidence than derived.
    PlanForged { why: String },
    /// Missing rights/source material, an uncontrolled external input, unbounded computation.
    ExternalBlocker { why: String },
}

impl RegistrationOutcomeV1 {
    /// The machine token (RFC-0011 §16.2).
    pub fn code(&self) -> &'static str {
        match self {
            Self::EligibleAt { .. } => "ELIGIBLE_AT",
            Self::FrontendRequired { .. } => "FRONTEND_REQUIRED",
            Self::KernelExtensionRequired { .. } => "KERNEL_EXTENSION_REQUIRED",
            Self::KernelNotActive { .. } => "KERNEL_NOT_ACTIVE",
            Self::BoundsExceeded { .. } => "BOUNDS_EXCEEDED",
            Self::IncompleteCoverage { .. } => "INCOMPLETE_COVERAGE",
            Self::PlanForged { .. } => "PLAN_FORGED",
            Self::ExternalBlocker { .. } => "EXTERNAL_BLOCKER",
        }
    }

    pub fn is_eligible(&self) -> bool {
        matches!(self, Self::EligibleAt { .. })
    }

    /// **The RFC-0011 §16.4 bucket.** `supported_active_kernel` needs complete-task on-chain registration
    /// evidence under the measured release; an eligibility verdict without it is `untested`.
    pub fn coverage_bucket(&self, onchain_registration_evidence: bool) -> CoverageBucketV1 {
        match self {
            Self::EligibleAt { .. } if onchain_registration_evidence => CoverageBucketV1::SupportedActiveKernel,
            Self::EligibleAt { .. } => CoverageBucketV1::Untested,
            Self::FrontendRequired { .. } | Self::PlanForged { .. } => CoverageBucketV1::FrontendGap,
            Self::KernelExtensionRequired { .. } | Self::KernelNotActive { .. } => CoverageBucketV1::KernelExtensionGap,
            Self::BoundsExceeded { .. } | Self::IncompleteCoverage { .. } => CoverageBucketV1::ResourceOrLifecycleGap,
            Self::ExternalBlocker { .. } => CoverageBucketV1::ExternalGap,
        }
    }
}

impl std::fmt::Display for RegistrationOutcomeV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let short = |d: &Digest| hex(&d[..8]);
        match self {
            Self::EligibleAt { daa, descriptor, plan_root, error_bits } => write!(
                f,
                "ELIGIBLE_AT daa {daa}: kernel {}…, plan {}…, ε_check ≤ 2^-{error_bits} (static admission only)",
                short(descriptor),
                short(plan_root)
            ),
            Self::FrontendRequired { reason } => write!(f, "FRONTEND_REQUIRED: {reason}"),
            Self::KernelExtensionRequired { family, relation, required, available } => write!(
                f,
                "KERNEL_EXTENSION_REQUIRED [{}]: {relation} — requires {required}; available {available}",
                family.map(|x| x.name()).unwrap_or("none")
            ),
            Self::KernelNotActive { descriptor, status } => {
                write!(f, "KERNEL_NOT_ACTIVE: kernel {}… is {status:?}", short(descriptor))
            }
            Self::BoundsExceeded { what, required, limit } => write!(f, "BOUNDS_EXCEEDED: {what} {required} > limit {limit}"),
            Self::IncompleteCoverage { what } => write!(f, "INCOMPLETE_COVERAGE: {what}"),
            Self::PlanForged { why } => write!(f, "PLAN_FORGED: {why}"),
            Self::ExternalBlocker { why } => write!(f, "EXTERNAL_BLOCKER: {why}"),
        }
    }
}

/// RFC-0011 §16.4: one primary blocker per repository. Only the first supplies successes to the
/// all-HF estimator; the rest are failures, and none is credited because a future kernel could
/// represent it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CoverageBucketV1 {
    SupportedActiveKernel,
    FrontendGap,
    KernelExtensionGap,
    ResourceOrLifecycleGap,
    ExternalGap,
    Untested,
}

impl CoverageBucketV1 {
    pub const fn name(self) -> &'static str {
        match self {
            Self::SupportedActiveKernel => "supported_active_kernel",
            Self::FrontendGap => "frontend_gap",
            Self::KernelExtensionGap => "kernel_extension_gap",
            Self::ResourceOrLifecycleGap => "resource_or_lifecycle_gap",
            Self::ExternalGap => "external_gap",
            Self::Untested => "untested",
        }
    }

    pub const fn counts_as_success(self) -> bool {
        matches!(self, Self::SupportedActiveKernel)
    }
}
