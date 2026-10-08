//! **Every frontend refusal has a machine-readable onboarding class** (RFC-0011 §16.2 outcomes; `misaka-palw-challenge`'s
//! `OnboardingFailureV1`; the model onboarding matrix of 2026-10-08): *who has to change something* for the model to move on.
//!
//! ```text
//! FRONTEND_REQUIRED          the importer / lowerer / adapter data cannot read or lower it although the IR could express it
//! KERNEL_EXTENSION_REQUIRED  a semantic primitive, relation, memory rule or court is missing from every shipped kernel
//! KERNEL_NOT_ACTIVE          the kernel exists and the fence that activates it is not in force
//! LAYOUT_REQUIRED            no exact layout is pinned (pack → preflight → registration)
//! RESOURCE_REFUSED           a mandatory node / DA / court / census bound is over its ceiling (never answered by raising a cap)
//! PROFILE_REQUIRED           the declared task has no canonical job profile (an RFC-level, consensus decision; recorded as a
//!                            KERNEL_EXTENSION_REQUIRED in the lifecycle, because the court path of the job is what is missing)
//! EXTERNAL_BLOCKER           the source material is missing, gated, unpinned or malformed (nothing in this tree can supply it)
//! NOT_RUN                    a depth or data the run did not read: **not a verdict**, never a gap of any kind
//! UNMAPPED                   a code this table does not know. Never a pass; a test keeps the published codes out of it.
//! ```
//!
//! The rule that matters most is the last-but-one: **a gate that was not run because the bytes were not read is not a semantic
//! gap.** `NOT_RUN_NEEDS_TENSOR_DATA` (a GGUF's `rope_freqs.weight`, 128–256 bytes the header census does not fetch),
//! `NOT_RUN_NEEDS_WEIGHTS`, `NOT_RUN_DEPTH_HEADERS`, `NOT_RUN_AFTER_<GATE>` and the rest of the `NOT_RUN_*` family are
//! [`GapClassV1::NotRun`], and [`GapClassV1::is_semantic_gap`] is `false` for every class but `KERNEL_EXTENSION_REQUIRED` and
//! `PROFILE_REQUIRED`. A `TENSOR_MISSING` is the binder's (`FRONTEND_REQUIRED`), never the kernel's.
//!
//! Nothing here reads a model name. The only table consulted besides the code vocabulary is the feature registry
//! (`misaka_palw_tir_lower::model::feature_info`): a feature whose protocol requirement is a [`Requirement::Capability`] is a kernel
//! gap, one whose requirement is `None` (every `Missing` and `Specified` feature of this build) is the lowerer's.

use super::codes::{self, Gate};
use crate::preflight::Blocker;
use misaka_palw_tir_lower::model::{Requirement, feature_info};
use serde::{Deserialize, Serialize};

/// The reason text of a lifecycle record whose failure is a missing job profile (`KERNEL_EXTENSION_REQUIRED`, "task profile").
pub const TASK_PROFILE_REASON: &str = "task profile";

/// Who has to change something. See the module documentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GapClassV1 {
    FrontendRequired,
    KernelExtensionRequired,
    KernelNotActive,
    LayoutRequired,
    ResourceRefused,
    ProfileRequired,
    ExternalBlocker,
    NotRun,
    Unmapped,
}

impl GapClassV1 {
    /// The stable machine token (the serialized form).
    pub const fn code(self) -> &'static str {
        match self {
            GapClassV1::FrontendRequired => "FRONTEND_REQUIRED",
            GapClassV1::KernelExtensionRequired => "KERNEL_EXTENSION_REQUIRED",
            GapClassV1::KernelNotActive => "KERNEL_NOT_ACTIVE",
            GapClassV1::LayoutRequired => "LAYOUT_REQUIRED",
            GapClassV1::ResourceRefused => "RESOURCE_REFUSED",
            GapClassV1::ProfileRequired => "PROFILE_REQUIRED",
            GapClassV1::ExternalBlocker => "EXTERNAL_BLOCKER",
            GapClassV1::NotRun => "NOT_RUN",
            GapClassV1::Unmapped => "UNMAPPED",
        }
    }

    /// The `OnboardingFailureV1::code()` this class is recorded as in the lifecycle, when it is one of its failures (the lifecycle
    /// has no `EXTERNAL_BLOCKER` — a source problem is not a model-lifecycle step — and `NOT_RUN` is not a failure at all).
    pub const fn onboarding_failure_code(self) -> Option<&'static str> {
        match self {
            GapClassV1::FrontendRequired => Some("FRONTEND_REQUIRED"),
            // A missing job profile is the chain's court path of the job, not the importer's.
            GapClassV1::KernelExtensionRequired | GapClassV1::ProfileRequired => Some("KERNEL_EXTENSION_REQUIRED"),
            GapClassV1::KernelNotActive => Some("KERNEL_NOT_ACTIVE"),
            GapClassV1::LayoutRequired => Some("LAYOUT_REQUIRED"),
            GapClassV1::ResourceRefused => Some("RESOURCE_REFUSED"),
            GapClassV1::ExternalBlocker | GapClassV1::NotRun | GapClassV1::Unmapped => None,
        }
    }

    /// The reason text a lifecycle record carries beside [`onboarding_failure_code`](Self::onboarding_failure_code), so that the matrix
    /// can count the classes that share a code separately: a missing job profile is recorded as `KERNEL_EXTENSION_REQUIRED` **with the
    /// reason `"task profile"`** — never bare, and no contract change (the reason is text, not a new code).
    pub const fn onboarding_reason(self) -> Option<&'static str> {
        match self {
            GapClassV1::ProfileRequired => Some(TASK_PROFILE_REASON),
            _ => None,
        }
    }

    /// The model's *semantics* need something the chain does not have (a relation, a court path, a job profile). Only these two
    /// classes say so; a frontend, layout, resource, external or not-run outcome never does.
    pub const fn is_semantic_gap(self) -> bool {
        matches!(self, GapClassV1::KernelExtensionRequired | GapClassV1::ProfileRequired)
    }

    /// A verdict about the model, as opposed to a depth or data this run did not read.
    pub const fn is_verdict(self) -> bool {
        !matches!(self, GapClassV1::NotRun | GapClassV1::Unmapped)
    }

    /// The class of a `misaka_palw_kernel::outcome::RegistrationOutcomeV1::code()` — the same split as its
    /// [`coverage_bucket`](misaka_palw_kernel::outcome::RegistrationOutcomeV1::coverage_bucket) (`ELIGIBLE_AT` is no gap: `None`).
    pub fn of_registration_outcome(code: &str) -> Option<GapClassV1> {
        Some(match code {
            "ELIGIBLE_AT" => return None,
            "FRONTEND_REQUIRED" | "PLAN_FORGED" => GapClassV1::FrontendRequired,
            "KERNEL_EXTENSION_REQUIRED" => GapClassV1::KernelExtensionRequired,
            "KERNEL_NOT_ACTIVE" => GapClassV1::KernelNotActive,
            // The bucket `resource_or_lifecycle_gap` of §16.4: bounds, capacity, and a plan that omits a relation or a budget.
            "BOUNDS_EXCEEDED" | "READINESS_OR_CAPACITY" | "INCOMPLETE_COVERAGE" => GapClassV1::ResourceRefused,
            "EXTERNAL_BLOCKER" => GapClassV1::ExternalBlocker,
            _ => GapClassV1::Unmapped,
        })
    }
}

/// `NOT_RUN_*` — a depth, bytes or a chain this run did not have. Never a verdict.
pub fn is_not_run_code(code: &str) -> bool {
    code.starts_with("NOT_RUN")
}

/// The class of a feature a refusal names (`ARCH_NEEDS_FEATURE(<arg>)`, the census's `FEATURE_C`): the registry's protocol
/// requirement when the argument is a feature id, else (`an adapter for \`X\``, a description) the lowerer's.
fn class_of_feature(arg: Option<&str>, general_primitive_named: bool) -> GapClassV1 {
    if let Some(info) = arg.and_then(feature_info) {
        return match info.protocol {
            Requirement::Capability { .. } => GapClassV1::KernelExtensionRequired,
            Requirement::None => GapClassV1::FrontendRequired,
        };
    }
    // Not in the registry: a gap that names a *general primitive that would close it* is a protocol gap (the frontend's own
    // refusals never do — "a data adapter can map a missing key or tensor name, never a missing computation").
    if general_primitive_named { GapClassV1::KernelExtensionRequired } else { GapClassV1::FrontendRequired }
}

/// **The class of one census code** (the gate it was found at, its `blocking` code, its argument and evidence).
///
/// Total: a code outside the vocabulary is [`GapClassV1::Unmapped`], never a guess.
pub fn classify_census_code_v1(gate: Gate, code: &str, arg: Option<&str>, evidence: &[String]) -> GapClassV1 {
    use GapClassV1::*;
    if is_not_run_code(code) {
        return NotRun;
    }
    match code {
        // ---- source: nothing in this tree can supply the material --------------------------------------------------------------
        codes::REPO_UNREACHABLE
        | codes::REPO_DISABLED
        | codes::GATED_ACCESS
        | codes::MISSING_WEIGHTS
        | codes::BASE_UNPINNED
        | codes::FETCH_FAILED
        | codes::HEADER_INVALID
        | codes::WEIGHTS_INCOMPLETE
        | codes::RIGHTS_UNCONFIRMED => ExternalBlocker,
        // The census's own cap on a header it will read.
        codes::HEADER_TOO_LARGE => ResourceRefused,

        // ---- lower: the task ------------------------------------------------------------------------------------------------------
        codes::TASK_UNKNOWN | "TASK_MISMATCH" => FrontendRequired,
        codes::MODALITY_PROFILE_MISSING | codes::PARTIAL_TASK_ONLY => ProfileRequired,

        // ---- lower: the artifact's form and the reader --------------------------------------------------------------------------
        codes::FORMAT_UNSUPPORTED | codes::ADAPTER_UNCHECKED | "ADAPTER_REFUSED" => FrontendRequired,
        // A repository with weights and no configuration, or a configuration that is not JSON: the source's.
        codes::CONFIG_MISSING => ExternalBlocker,
        "CONFIG_INVALID" => ExternalBlocker,
        // Repository code is never run; an adapter (data) that models it is the way in.
        codes::CUSTOM_CODE_UNMODELLED => FrontendRequired,
        codes::FEATURE_C => class_of_feature(arg, evidence.iter().any(|e| e.contains("smallest general"))),
        "ARCH_REFUSED" | "CONFIG_KEY_UNREAD" | "TOKENIZER_MISSING" | "TENSOR_MISSING" | "TENSOR_SHAPE" => FrontendRequired,
        codes::QUANT_DESCRIPTOR_MISSING | "QUANT_REFUSED" => FrontendRequired,

        // ---- admit: the bounds -----------------------------------------------------------------------------------------------------
        codes::COURT_BUDGET
        | codes::CLOSE_TOO_LARGE
        | codes::CONTEXT_BOUND
        | "COURT_WINDOW_EXCEEDED"
        | "DA_LADDER_EXCEEDED"
        | "ADMISSION_EXCEEDS" => ResourceRefused,
        "ADMISSION_REFUSED" => {
            // "no layout can be derived for the program" is the layout's; any other refusal of a lowered program is the
            // program's (the frontend wrote something admission does not accept).
            if evidence.iter().any(|e| e.contains("no layout")) { LayoutRequired } else { FrontendRequired }
        }
        "FENCE_NOT_ARMED" => KernelNotActive,
        "ARTIFACT_ROOT_KNOWN" => ExternalBlocker,

        // ---- pack / seat -------------------------------------------------------------------------------------------------------------
        "PACK_NOT_VERIFIED" => LayoutRequired,
        codes::SEAT_MEMORY | "READY_SEATS_SHORT" | "INDEPENDENCE_SHORT" => ResourceRefused,

        // A panic of the preflight is the tool's.
        "PREFLIGHT_PANIC" => FrontendRequired,
        _ => {
            let _ = gate;
            Unmapped
        }
    }
}

/// **The class of one preflight blocker** (§II.2.4's codes), with the registry consulted for the feature a refusal names.
pub fn classify_blocker_v1(b: &Blocker) -> GapClassV1 {
    use GapClassV1::*;
    match b.code.as_str() {
        // The preflight's own protocol-gap codes: a new primitive / court kernel is the chain's.
        "ARCH_NEEDS_PRIMITIVE" => KernelExtensionRequired,
        "ARCH_NEEDS_FEATURE" => {
            class_of_feature(b.arg.as_deref(), b.safe_paths.iter().any(|p| p.contains("smallest general")))
        }
        "REMOTE_CODE" | "FORMAT_UNSUPPORTED" => FrontendRequired,
        "SOURCE_INCOMPLETE" => ExternalBlocker,
        "CANONICAL_JOB_OUT_OF_BOUNDS" | "COURT_COST_OVER_CEILING" | "CLOSE_SIZE_OVER_CAP" | "SEAT_MEMORY_SHORT"
        | "READY_SEATS_INSUFFICIENT" | "INDEPENDENT_OPERATORS" => ResourceRefused,
        "QUANT_NO_DESCRIPTOR" | "QUANT_KNOWN_UNDESCRIBED" => FrontendRequired,
        other => {
            let (gate, mapped) = codes::gate_code_of_preflight(other).unwrap_or((Gate::Lower, codes::UNMAPPED_PREFLIGHT_CODE));
            let mut ev: Vec<String> = vec![b.what.clone()];
            ev.extend(b.evidence.iter().cloned());
            classify_census_code_v1(gate, mapped, b.arg.as_deref(), &ev)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_codes() -> Vec<&'static str> {
        vec![
            codes::NOT_RUN_NEEDS_WEIGHTS,
            codes::NOT_RUN_NEEDS_CHAIN,
            codes::NOT_RUN_NOT_SAMPLED,
            codes::NOT_RUN_JUDGMENT_BUDGET,
            codes::NOT_RUN_DEPTH_HEADERS,
            codes::NOT_RUN_PIPELINE_ADMISSION,
            codes::NOT_RUN_NEEDS_TENSOR_DATA,
            "NOT_RUN_AFTER_SOURCE",
            "NOT_RUN_AFTER_LOWER",
            "NOT_RUN_AFTER_ADMIT",
            "NOT_RUN_PREFLIGHT_UNKNOWN",
        ]
    }

    /// **Header-only is never a semantic gap.** Every `NOT_RUN_*` code — the ones a census raises because the bytes of a tensor, the
    /// weights, a chain or a depth were not read — is `NOT_RUN`: not a verdict, not a frontend gap, not a kernel gap.
    #[test]
    fn a_gate_that_was_not_run_is_never_a_gap_of_any_kind() {
        for c in all_codes() {
            for gate in Gate::ALL {
                let k = classify_census_code_v1(gate, c, Some("rope_freqs.weight (256 bytes)"), &["the census reads headers only".into()]);
                assert_eq!(k, GapClassV1::NotRun, "{c} at {gate:?}");
                assert!(!k.is_semantic_gap() && !k.is_verdict() && k.onboarding_failure_code().is_none(), "{c}");
            }
        }
        // The family is closed under the prefix, not under a list: a code a later census invents is still not a verdict.
        assert_eq!(classify_census_code_v1(Gate::Pack, "NOT_RUN_SOMETHING_NEW", None, &[]), GapClassV1::NotRun);
    }

    /// The binder's gaps — a tensor the program reads that is not in the headers, a shape it does not read, a key it has no rule for,
    /// a tokenizer file, an adapter — are the importer's, whatever the argument says.
    #[test]
    fn the_importers_refusals_are_frontend_required_and_never_kernel_extensions() {
        for c in [
            "TENSOR_MISSING",
            "TENSOR_SHAPE",
            "CONFIG_KEY_UNREAD",
            "TOKENIZER_MISSING",
            "ARCH_REFUSED",
            codes::CUSTOM_CODE_UNMODELLED,
            codes::QUANT_DESCRIPTOR_MISSING,
            "QUANT_REFUSED",
            codes::FORMAT_UNSUPPORTED,
            codes::ADAPTER_UNCHECKED,
            "ADAPTER_REFUSED",
            codes::TASK_UNKNOWN,
        ] {
            for arg in [None, Some("rope_freqs.weight"), Some("attn.q.w"), Some("an adapter for `X`")] {
                let k = classify_census_code_v1(Gate::Lower, c, arg, &[]);
                assert_eq!(k, GapClassV1::FrontendRequired, "{c} {arg:?}");
                assert_eq!(k.onboarding_failure_code(), Some("FRONTEND_REQUIRED"));
                assert!(!k.is_semantic_gap());
            }
        }
    }

    #[test]
    fn a_feature_is_classified_by_the_registrys_protocol_requirement_and_not_by_its_name() {
        // Every feature of this build's registry that the lowerer lacks (`Missing` / `Specified`) needs no new capability.
        let missing = misaka_palw_tir_lower::model::REGISTRY
            .iter()
            .filter(|f| !matches!(f.lowering, misaka_palw_tir_lower::model::Lowering::Implemented))
            .collect::<Vec<_>>();
        assert!(!missing.is_empty());
        for f in missing {
            let k = classify_census_code_v1(Gate::Lower, codes::FEATURE_C, Some(f.id.0), &[]);
            let want = match f.protocol {
                Requirement::None => GapClassV1::FrontendRequired,
                Requirement::Capability { .. } => GapClassV1::KernelExtensionRequired,
            };
            assert_eq!(k, want, "{}", f.id.0);
        }
        // A description that is not an id, with no general primitive named: the lowerer's.
        assert_eq!(
            classify_census_code_v1(Gate::Lower, codes::FEATURE_C, Some("an adapter for `NewModel`"), &[]),
            GapClassV1::FrontendRequired
        );
        // A gap that names the smallest general primitive that would close it is a protocol gap.
        let ev = ["the smallest general addition that closes it: a gather over the last axis".to_string()];
        assert_eq!(
            classify_census_code_v1(Gate::Lower, codes::FEATURE_C, Some("a thing"), &ev),
            GapClassV1::KernelExtensionRequired
        );
        let b = Blocker::new(crate::preflight::Stage::Convert, "ARCH_NEEDS_PRIMITIVE", "needs a new primitive");
        assert_eq!(classify_blocker_v1(&b), GapClassV1::KernelExtensionRequired);
    }

    #[test]
    fn bounds_layout_fence_task_profile_and_source_codes_have_their_own_classes() {
        use GapClassV1::*;
        let k = |g, c: &str, ev: &[&str]| classify_census_code_v1(g, c, None, &ev.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        for c in [codes::COURT_BUDGET, codes::CLOSE_TOO_LARGE, codes::CONTEXT_BOUND, "COURT_WINDOW_EXCEEDED", "DA_LADDER_EXCEEDED", "ADMISSION_EXCEEDS"] {
            assert_eq!(k(Gate::Admit, c, &[]), ResourceRefused, "{c}");
        }
        assert_eq!(k(Gate::Admit, "ADMISSION_REFUSED", &["no layout can be derived for the program"]), LayoutRequired);
        assert_eq!(k(Gate::Admit, "ADMISSION_REFUSED", &["tir_admit_v1 refuses the program"]), FrontendRequired);
        assert_eq!(k(Gate::Pack, "PACK_NOT_VERIFIED", &[]), LayoutRequired);
        assert_eq!(k(Gate::Admit, "FENCE_NOT_ARMED", &[]), KernelNotActive);
        assert_eq!(k(Gate::Seat, codes::SEAT_MEMORY, &[]), ResourceRefused);
        assert_eq!(k(Gate::Lower, codes::MODALITY_PROFILE_MISSING, &[]), ProfileRequired);
        assert_eq!(k(Gate::Lower, codes::PARTIAL_TASK_ONLY, &[]), ProfileRequired);
        assert_eq!(ProfileRequired.onboarding_failure_code(), Some("KERNEL_EXTENSION_REQUIRED"));
        // …always with the reason text, so the matrix counts it apart from a relation gap; no other class carries one.
        assert_eq!(ProfileRequired.onboarding_reason(), Some("task profile"));
        for c in [FrontendRequired, KernelExtensionRequired, KernelNotActive, LayoutRequired, ResourceRefused, ExternalBlocker, NotRun, Unmapped] {
            assert_eq!(c.onboarding_reason(), None, "{c:?}");
        }
        assert!(ProfileRequired.onboarding_failure_code() == KernelExtensionRequired.onboarding_failure_code());
        for c in [codes::GATED_ACCESS, codes::MISSING_WEIGHTS, codes::BASE_UNPINNED, codes::HEADER_INVALID, codes::WEIGHTS_INCOMPLETE, codes::CONFIG_MISSING] {
            assert_eq!(k(Gate::Source, c, &[]), ExternalBlocker, "{c}");
            assert_eq!(ExternalBlocker.onboarding_failure_code(), None);
        }
        assert_eq!(k(Gate::Lower, "SOMETHING_NEW", &[]), Unmapped);
    }

    /// Every code the preflight publishes (§II.2.4) and every code the census publishes has a class that is not `UNMAPPED`: a new
    /// code without a row fails here instead of reaching a report as an unclassified failure.
    #[test]
    fn every_published_code_has_a_class() {
        let published_preflight = [
            "ARCH_NEEDS_FEATURE",
            "ARCH_NEEDS_PRIMITIVE",
            "ARCH_REFUSED",
            "CONFIG_KEY_UNREAD",
            "CONFIG_INVALID",
            "REMOTE_CODE",
            "QUANT_NO_DESCRIPTOR",
            "QUANT_KNOWN_UNDESCRIBED",
            "QUANT_REFUSED",
            "TENSOR_MISSING",
            "TENSOR_SHAPE",
            "TOKENIZER_MISSING",
            "ADAPTER_REFUSED",
            "SOURCE_INCOMPLETE",
            "FORMAT_UNSUPPORTED",
            "ADMISSION_EXCEEDS",
            "ADMISSION_REFUSED",
            "CLOSE_SIZE_OVER_CAP",
            "COURT_COST_OVER_CEILING",
            "DA_LADDER_EXCEEDED",
            "COURT_WINDOW_EXCEEDED",
            "CANONICAL_JOB_OUT_OF_BOUNDS",
            "FENCE_NOT_ARMED",
            "ARTIFACT_ROOT_KNOWN",
            "SEAT_MEMORY_SHORT",
            "READY_SEATS_INSUFFICIENT",
            "INDEPENDENT_OPERATORS",
            "PACK_NOT_VERIFIED",
        ];
        for c in published_preflight {
            let b = Blocker::new(crate::preflight::Stage::Convert, c, "x");
            assert_ne!(classify_blocker_v1(&b), GapClassV1::Unmapped, "{c}");
        }
        let census = [
            codes::REPO_UNREACHABLE,
            codes::REPO_DISABLED,
            codes::GATED_ACCESS,
            codes::MISSING_WEIGHTS,
            codes::BASE_UNPINNED,
            codes::WEIGHTS_INCOMPLETE,
            codes::FETCH_FAILED,
            codes::HEADER_TOO_LARGE,
            codes::HEADER_INVALID,
            codes::RIGHTS_UNCONFIRMED,
            codes::TASK_UNKNOWN,
            codes::MODALITY_PROFILE_MISSING,
            codes::PARTIAL_TASK_ONLY,
            codes::FORMAT_UNSUPPORTED,
            codes::ADAPTER_UNCHECKED,
            codes::CONFIG_MISSING,
            codes::CUSTOM_CODE_UNMODELLED,
            codes::FEATURE_C,
            codes::QUANT_DESCRIPTOR_MISSING,
            codes::COURT_BUDGET,
            codes::CLOSE_TOO_LARGE,
            codes::CONTEXT_BOUND,
            codes::SEAT_MEMORY,
            "TASK_MISMATCH",
            "PREFLIGHT_PANIC",
            "READY_SEATS_SHORT",
            "INDEPENDENCE_SHORT",
        ];
        for c in census {
            assert_ne!(classify_census_code_v1(Gate::Lower, c, None, &[]), GapClassV1::Unmapped, "{c}");
        }
        // The census's own unmapped marker is, honestly, unmapped.
        assert_eq!(classify_census_code_v1(Gate::Lower, codes::UNMAPPED_PREFLIGHT_CODE, None, &[]), GapClassV1::Unmapped);
    }

    /// The tokens are the lifecycle's: `OnboardingFailureV1::code()` and the kernel's `RegistrationOutcomeV1::code()` are the
    /// same strings, not a second spelling.
    #[test]
    fn the_tokens_are_the_lifecycles_and_the_kernels() {
        use misaka_palw_challenge::lifecycle::OnboardingFailureV1 as F;
        use misaka_palw_kernel::outcome::RegistrationOutcomeV1 as O;
        for (class, f) in [
            (GapClassV1::FrontendRequired, F::FrontendRequired),
            (GapClassV1::KernelExtensionRequired, F::KernelExtensionRequired),
            (GapClassV1::KernelNotActive, F::KernelNotActive),
            (GapClassV1::LayoutRequired, F::LayoutRequired),
            (GapClassV1::ResourceRefused, F::ResourceRefused),
        ] {
            assert_eq!(class.code(), f.code());
            assert_eq!(class.onboarding_failure_code(), Some(f.code()));
        }
        let digest = [0u8; 64];
        let outcomes = [
            O::FrontendRequired { reason: "x".into() },
            O::KernelExtensionRequired { family: None, relation: "r".into(), required: "q".into(), available: "a".into() },
            O::KernelNotActive { descriptor: digest, status: None },
            O::CapacityPending { what: "x".into() },
            O::BoundsExceeded { what: "x", required: 2, limit: 1 },
            O::IncompleteCoverage { what: "x".into() },
            O::PlanForged { why: "x".into() },
            O::ExternalBlocker { why: "x".into() },
        ];
        for o in outcomes {
            let class = GapClassV1::of_registration_outcome(o.code()).expect("a gap");
            assert_ne!(class, GapClassV1::Unmapped, "{}", o.code());
            // The same split as the kernel's own §16.4 bucket.
            let bucket = o.coverage_bucket(misaka_palw_kernel::outcome::CoverageEvidenceV1::NONE).name();
            let want = match bucket {
                "frontend_gap" => GapClassV1::FrontendRequired,
                "kernel_extension_gap" => {
                    if o.code() == "KERNEL_NOT_ACTIVE" { GapClassV1::KernelNotActive } else { GapClassV1::KernelExtensionRequired }
                }
                "resource_or_lifecycle_gap" => GapClassV1::ResourceRefused,
                "external_gap" => GapClassV1::ExternalBlocker,
                other => panic!("{other}"),
            };
            assert_eq!(class, want, "{}", o.code());
        }
        assert_eq!(GapClassV1::of_registration_outcome("ELIGIBLE_AT"), None);
        assert_eq!(GapClassV1::of_registration_outcome("SOMETHING_NEW"), Some(GapClassV1::Unmapped));
    }
}
