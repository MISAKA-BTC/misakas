//! **Model onboarding lifecycle** (RFC-0011 §17's three stages, the onboarding directive's states and failures), machine-readable.
//!
//! ```text
//! SOURCE_DISCOVERED → FRONTEND_READY → STATIC_ADMITTED → REGISTERED_DORMANT → CHALLENGE_PENDING
//!                   → CONFORMANCE_PASSED → G14_ELIGIBLE → ACTIVE_REWARDABLE
//! ```
//!
//! Each step needs its own evidence ([`OnboardingStepV1`]); none is inferred from another. Converted is not Registered,
//! Registered is not Active, and Active is not "every claim is correct". `BEACON_UNAVAILABLE` leaves the record
//! `REGISTERED_DORMANT` (pending, retryable within the policy's counted retries) — never a pass and never fraud. A failure is a
//! recorded outcome with its code, so `FRONTEND_REQUIRED` and `KERNEL_EXTENSION_REQUIRED` are never confused.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::hash::Digest;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum OnboardingStateV1 {
    SourceDiscovered = 0,
    FrontendReady = 1,
    StaticAdmitted = 2,
    RegisteredDormant = 3,
    ChallengePending = 4,
    ConformancePassed = 5,
    G14Eligible = 6,
    ActiveRewardable = 7,
}

impl OnboardingStateV1 {
    pub const fn code(self) -> &'static str {
        match self {
            Self::SourceDiscovered => "SOURCE_DISCOVERED",
            Self::FrontendReady => "FRONTEND_READY",
            Self::StaticAdmitted => "STATIC_ADMITTED",
            Self::RegisteredDormant => "REGISTERED_DORMANT",
            Self::ChallengePending => "CHALLENGE_PENDING",
            Self::ConformancePassed => "CONFORMANCE_PASSED",
            Self::G14Eligible => "G14_ELIGIBLE",
            Self::ActiveRewardable => "ACTIVE_REWARDABLE",
        }
    }

    /// Only `ActiveRewardable` earns rewards or work weight.
    pub const fn rewardable(self) -> bool {
        matches!(self, Self::ActiveRewardable)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum OnboardingFailureV1 {
    /// The importer/lowerer cannot read or lower the source although the kernel could represent it.
    FrontendRequired = 1,
    /// A semantic primitive, relation, memory rule or court is missing from every active kernel.
    KernelExtensionRequired = 2,
    KernelNotActive = 3,
    /// No exact layout is pinned (pack → preflight → registration).
    LayoutRequired = 4,
    ResourceRefused = 5,
    ConformanceFailed = 6,
    /// Fewer than `k` qualifying sources in the window: pending, not fraud.
    BeaconUnavailable = 7,
    /// `PUBLIC_PROSECUTION_COMPLETE(plan, profile)` fails.
    PublicProsecutionIncomplete = 8,
    /// The model's public material is not available/retained as the class requires.
    AvailabilityRequired = 9,
}

impl OnboardingFailureV1 {
    pub const fn code(self) -> &'static str {
        match self {
            Self::FrontendRequired => "FRONTEND_REQUIRED",
            Self::KernelExtensionRequired => "KERNEL_EXTENSION_REQUIRED",
            Self::KernelNotActive => "KERNEL_NOT_ACTIVE",
            Self::LayoutRequired => "LAYOUT_REQUIRED",
            Self::ResourceRefused => "RESOURCE_REFUSED",
            Self::ConformanceFailed => "CONFORMANCE_FAILED",
            Self::BeaconUnavailable => "BEACON_UNAVAILABLE",
            Self::PublicProsecutionIncomplete => "PUBLIC_PROSECUTION_INCOMPLETE",
            Self::AvailabilityRequired => "AVAILABILITY_REQUIRED",
        }
    }

    /// Failures that leave the record where it was, to be retried (a pending condition, not a verdict on the model).
    pub const fn is_pending(self) -> bool {
        matches!(self, Self::BeaconUnavailable | Self::AvailabilityRequired)
    }
}

/// The evidence one step presents. Each carries the identity it establishes; a step without its evidence does not advance.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum OnboardingStepV1 {
    /// The frontend lowered the source to a canonical program (or reported why not).
    Frontend(Result<Digest, OnboardingFailureV1>),
    /// Static semantic admission: the plan root (or the failure: frontend, kernel extension, kernel not active, layout, resource).
    StaticAdmission(Result<Digest, OnboardingFailureV1>),
    /// The registration object was accepted on chain: the class id.
    Registered { class_id: Digest },
    /// The conformance commitment was accepted on chain: its statement root.
    ConformanceCommitted { commitment_root: Digest },
    /// The beacon window closed without `k` qualifying sources.
    BeaconUnavailable,
    /// Conformance evidence was checked against the locked beacon: the evidence id, or failure.
    ConformanceChecked(Result<Digest, OnboardingFailureV1>),
    /// The code-derived public-prosecution gate over the registered plan/profile.
    PublicProsecutionGate { complete: bool },
    /// Public availability/retention established, and the chain's activation eligibility reached.
    Activated { availability: bool },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct OnboardingRecordV1 {
    pub state: OnboardingStateV1,
    pub last_failure: Option<OnboardingFailureV1>,
    pub program_root: Option<Digest>,
    pub plan_root: Option<Digest>,
    pub class_id: Option<Digest>,
    pub commitment_root: Option<Digest>,
    pub conformance_evidence_id: Option<Digest>,
    /// Beacon-unavailable windows so far (each counts against the attempt limit).
    pub beacon_retries: u32,
    /// Failed conformance runs so far (each needed a new commitment; each counts against the attempt limit).
    pub conformance_failures: u32,
    /// The policy's bound on conformance attempts (commitments that ended unavailable or failed); then the record refuses another.
    pub attempt_limit: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum OnboardingRefusalV1 {
    #[error("{step} is not a step from {from}")]
    OutOfOrder { from: &'static str, step: &'static str },
    #[error("the failure {0} cannot come from this step")]
    WrongFailure(&'static str),
    #[error("{0} conformance attempts spent: the policy's limit")]
    AttemptsExhausted(u32),
}

impl OnboardingRecordV1 {
    /// A new record whose conformance attempts are bounded by `attempt_limit` (the challenge policy's `retry_limit` + 1).
    pub const fn new(attempt_limit: u32) -> Self {
        Self {
            state: OnboardingStateV1::SourceDiscovered,
            last_failure: None,
            program_root: None,
            plan_root: None,
            class_id: None,
            commitment_root: None,
            conformance_evidence_id: None,
            beacon_retries: 0,
            conformance_failures: 0,
            attempt_limit,
        }
    }

    /// Conformance attempts that ended without a pass.
    pub const fn attempts(&self) -> u32 {
        self.beacon_retries.saturating_add(self.conformance_failures)
    }

    fn step_name(step: &OnboardingStepV1) -> &'static str {
        match step {
            OnboardingStepV1::Frontend(_) => "Frontend",
            OnboardingStepV1::StaticAdmission(_) => "StaticAdmission",
            OnboardingStepV1::Registered { .. } => "Registered",
            OnboardingStepV1::ConformanceCommitted { .. } => "ConformanceCommitted",
            OnboardingStepV1::BeaconUnavailable => "BeaconUnavailable",
            OnboardingStepV1::ConformanceChecked(_) => "ConformanceChecked",
            OnboardingStepV1::PublicProsecutionGate { .. } => "PublicProsecutionGate",
            OnboardingStepV1::Activated { .. } => "Activated",
        }
    }

    /// Apply one step. A failure is recorded (`last_failure`) and leaves the state where the step found it; a success advances.
    pub fn apply(&mut self, step: OnboardingStepV1) -> Result<OnboardingStateV1, OnboardingRefusalV1> {
        use OnboardingFailureV1 as F;
        use OnboardingStateV1 as S;
        use OnboardingStepV1 as P;
        let out_of_order = |s: &Self, p: &P| OnboardingRefusalV1::OutOfOrder { from: s.state.code(), step: Self::step_name(p) };
        let allowed = |f: F, set: &[F]| if set.contains(&f) { Ok(()) } else { Err(OnboardingRefusalV1::WrongFailure(f.code())) };
        match (&self.state, &step) {
            (S::SourceDiscovered, P::Frontend(r)) => match r {
                Ok(program) => {
                    self.program_root = Some(*program);
                    self.state = S::FrontendReady;
                    self.last_failure = None;
                }
                Err(f) => {
                    allowed(*f, &[F::FrontendRequired, F::ResourceRefused])?;
                    self.last_failure = Some(*f);
                }
            },
            (S::FrontendReady, P::StaticAdmission(r)) => match r {
                Ok(plan) => {
                    self.plan_root = Some(*plan);
                    self.state = S::StaticAdmitted;
                    self.last_failure = None;
                }
                Err(f) => {
                    allowed(
                        *f,
                        &[F::FrontendRequired, F::KernelExtensionRequired, F::KernelNotActive, F::LayoutRequired, F::ResourceRefused],
                    )?;
                    self.last_failure = Some(*f);
                }
            },
            (S::StaticAdmitted, P::Registered { class_id }) => {
                self.class_id = Some(*class_id);
                self.state = S::RegisteredDormant;
                self.last_failure = None;
            }
            (S::RegisteredDormant, P::ConformanceCommitted { .. }) if self.attempts() >= self.attempt_limit => {
                return Err(OnboardingRefusalV1::AttemptsExhausted(self.attempts()));
            }
            (S::RegisteredDormant, P::ConformanceCommitted { commitment_root }) => {
                self.commitment_root = Some(*commitment_root);
                self.state = S::ChallengePending;
                self.last_failure = None;
            }
            // The window closed short: back to dormant (a new commitment and window are needed), counted.
            (S::ChallengePending, P::BeaconUnavailable) => {
                self.state = S::RegisteredDormant;
                self.commitment_root = None;
                self.beacon_retries += 1;
                self.last_failure = Some(F::BeaconUnavailable);
            }
            (S::ChallengePending, P::ConformanceChecked(r)) => match r {
                Ok(evidence) => {
                    self.conformance_evidence_id = Some(*evidence);
                    self.state = S::ConformancePassed;
                    self.last_failure = None;
                }
                Err(f) => {
                    allowed(*f, &[F::ConformanceFailed])?;
                    // A failed conformance needs a new commitment (changed artifact/layout/plan/implementation): dormant again.
                    self.state = S::RegisteredDormant;
                    self.conformance_failures += 1;
                    self.commitment_root = None;
                    self.last_failure = Some(*f);
                }
            },
            (S::ConformancePassed, P::PublicProsecutionGate { complete }) => {
                if *complete {
                    self.state = S::G14Eligible;
                    self.last_failure = None;
                } else {
                    self.last_failure = Some(F::PublicProsecutionIncomplete);
                }
            }
            (S::G14Eligible, P::Activated { availability }) => {
                if *availability {
                    self.state = S::ActiveRewardable;
                    self.last_failure = None;
                } else {
                    self.last_failure = Some(F::AvailabilityRequired);
                }
            }
            _ => return Err(out_of_order(self, &step)),
        }
        Ok(self.state)
    }

    /// The machine-readable status line: state code and last failure code.
    pub fn status(&self) -> (&'static str, Option<&'static str>) {
        (self.state.code(), self.last_failure.map(F::code))
    }
}

use OnboardingFailureV1 as F;
