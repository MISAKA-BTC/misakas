//! **Where a class registration stopped, named so an operator does not have to guess.**
//!
//! `model add` used to construct a `ClassRegistered` and print "ok" after the carrier left the
//! mempool. The 2M Qwen row showed why that is not enough: constructed and submitted can both be
//! true while the processor refuses `GLOBAL_WINDOW_EXCEEDED`, or the mempool can accept a carrier
//! that never folds into a registry row. The pipeline is five distinct facts:
//!
//! constructed → submitted → accepted (mempool/validate) → included (a block) → folded (class table)
//!
//! A hold always carries a machine token ([`PalwModelRegistrationCodeV1`]) and a sentence. Preflight
//! uses the same tokens the processor's admission gate returns, plus the positive checks a person
//! wants to see before they spend a fee (`FitsGlobalWindow`, `CourtCovered`).

use crate::Hash64;
use crate::config::params::Params;
use crate::palw_class_admission_v2::{PalwClassAdmissionError, palw_admission_shape_at_v1, verify_class_admission_v9};
use crate::palw_e2e_adjudicability::PalwE2eFamilyV1;
use crate::palw_mode_v2::PalwConsensusParamsV2;
use crate::palw_model_fit_v1::{PalwFitVerdictV1, PalwFitWallV1, palw_model_fit_v2};
use crate::palw_state_v2::PalwConsensusObjectV2;

/// Identity of a constructed registration object: keyed BLAKE2b-512 of its borsh bytes.
pub fn palw_registration_object_id_v1(bytes: &[u8]) -> Hash64 {
    kaspa_hashes::blake2b_512_keyed(b"PALW_MODEL_REG_V1", bytes)
}

/// Where the registration currently sits. Later stages imply earlier ones, except that
/// `Accepted` can fail without ever becoming `Included`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PalwModelRegistrationStageV1 {
    Constructed,
    Submitted,
    Accepted,
    Included,
    Folded,
}

impl PalwModelRegistrationStageV1 {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Constructed => "constructed",
            Self::Submitted => "submitted",
            Self::Accepted => "accepted",
            Self::Included => "included",
            Self::Folded => "folded",
        }
    }
}

/// Stable CLI/RPC token. Display of [`PalwClassAdmissionError`] stays the human sentence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PalwModelRegistrationCodeV1 {
    ModelAlreadyRegistered,
    ArtifactRootMismatch,
    CourtKernelUncovered,
    GlobalWindowExceeded,
    FamilyFenceClosed,
    NotEndToEndCertified,
    RegistrationNotIncluded,
    ReadySeatsInsufficient,
    FitsGlobalWindow,
    CourtCovered,
    KimiFamilyNeedsItsFence,
    AdmissionOk,
}

impl PalwModelRegistrationCodeV1 {
    pub const fn code(self) -> &'static str {
        match self {
            Self::ModelAlreadyRegistered => "MODEL_ALREADY_REGISTERED",
            Self::ArtifactRootMismatch => "ARTIFACT_ROOT_MISMATCH",
            Self::CourtKernelUncovered => "COURT_KERNEL_UNCOVERED",
            Self::GlobalWindowExceeded => "GLOBAL_WINDOW_EXCEEDED",
            Self::FamilyFenceClosed => "FAMILY_FENCE_CLOSED",
            Self::NotEndToEndCertified => "NOT_END_TO_END_CERTIFIED",
            Self::RegistrationNotIncluded => "REGISTRATION_NOT_INCLUDED",
            Self::ReadySeatsInsufficient => "READY_SEATS_INSUFFICIENT",
            Self::FitsGlobalWindow => "FitsGlobalWindow",
            Self::CourtCovered => "CourtCovered",
            Self::KimiFamilyNeedsItsFence => "KimiFamilyNeedsItsFence",
            Self::AdmissionOk => "ADMISSION_OK",
        }
    }

    pub const fn message(self) -> &'static str {
        match self {
            Self::ModelAlreadyRegistered => "this class id is already on the chain",
            Self::ArtifactRootMismatch => "the artifact roots to a different value than the class would register",
            Self::CourtKernelUncovered => "the class reaches a kernel this court's catalog does not cover",
            Self::GlobalWindowExceeded => "n_ctx × layers exceeds the global enumeration window",
            Self::FamilyFenceClosed => "the class needs a family fence this network has not armed",
            Self::NotEndToEndCertified => "the class asks for weight but no end-to-end certified family covers it",
            Self::RegistrationNotIncluded => "the carrier left this node but no block has folded the object",
            Self::ReadySeatsInsufficient => "ready seats are below the class's requiredReadySeats",
            Self::FitsGlobalWindow => "the declared shape fits the global enumeration window",
            Self::CourtCovered => "every kernel the graph reaches is in this court's catalog",
            Self::KimiFamilyNeedsItsFence => "the class reaches a Kimi K3 kernel and the Kimi family fence is closed",
            Self::AdmissionOk => "the processor's admission gate would admit this registration",
        }
    }

    pub fn from_admission(err: &PalwClassAdmissionError) -> Self {
        match err {
            PalwClassAdmissionError::CoverageGap => Self::CourtKernelUncovered,
            PalwClassAdmissionError::NotEndToEndCertified { .. } => Self::NotEndToEndCertified,
            PalwClassAdmissionError::KimiFamilyNeedsItsFence => Self::KimiFamilyNeedsItsFence,
            PalwClassAdmissionError::TokenLiftNeedsItsFence | PalwClassAdmissionError::HeldMapNeedsItsFence => {
                Self::FamilyFenceClosed
            }
            PalwClassAdmissionError::Profile(s) if s.contains("enumeration past the work ceiling") => Self::GlobalWindowExceeded,
            _ => Self::FamilyFenceClosed,
        }
    }

    pub fn from_admission_code(code: &str) -> Option<Self> {
        match code {
            "COURT_KERNEL_UNCOVERED" => Some(Self::CourtKernelUncovered),
            "NOT_END_TO_END_CERTIFIED" => Some(Self::NotEndToEndCertified),
            "FAMILY_FENCE_CLOSED" | "KimiFamilyNeedsItsFence" => Some(Self::FamilyFenceClosed),
            "GLOBAL_WINDOW_EXCEEDED" => Some(Self::GlobalWindowExceeded),
            "MODEL_ALREADY_REGISTERED" => Some(Self::ModelAlreadyRegistered),
            "ARTIFACT_ROOT_MISMATCH" => Some(Self::ArtifactRootMismatch),
            "REGISTRATION_NOT_INCLUDED" => Some(Self::RegistrationNotIncluded),
            "READY_SEATS_INSUFFICIENT" => Some(Self::ReadySeatsInsufficient),
            _ => None,
        }
    }

    pub fn from_fit_wall(wall: PalwFitWallV1, ok: bool) -> Option<Self> {
        match (wall, ok) {
            (PalwFitWallV1::GeometryCeiling, true) => Some(Self::FitsGlobalWindow),
            (PalwFitWallV1::GeometryCeiling, false) => Some(Self::GlobalWindowExceeded),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwModelPreflightCheckV1 {
    pub code: String,
    pub ok: bool,
    pub message: String,
}

impl PalwModelPreflightCheckV1 {
    pub fn from_code(code: PalwModelRegistrationCodeV1, ok: bool) -> Self {
        Self { code: code.code().to_string(), ok, message: code.message().to_string() }
    }

    pub fn from_admission_err(err: &PalwClassAdmissionError) -> Self {
        Self { code: err.code().to_string(), ok: false, message: err.to_string() }
    }
}

/// Map a mempool/submit error string onto a stable code when the node already named the gate.
pub fn palw_model_reject_from_submit_text_v1(text: &str) -> Option<PalwModelRegistrationCodeV1> {
    let t = text.to_ascii_uppercase();
    if t.contains("DUPLICATECLASS") || t.contains("ALREADY REGISTERED") || t.contains("MODEL_ALREADY_REGISTERED") {
        return Some(PalwModelRegistrationCodeV1::ModelAlreadyRegistered);
    }
    if t.contains("ENUMERATION PAST THE WORK CEILING")
        || t.contains("GLOBAL_WINDOW")
        || t.contains("GEOMETRY CEILING")
        || t.contains("PALW_STEP_MAX_ENUMERATION")
    {
        return Some(PalwModelRegistrationCodeV1::GlobalWindowExceeded);
    }
    if t.contains("NOT END-TO-END CERTIFIED") || t.contains("NOT_END_TO_END_CERTIFIED") {
        return Some(PalwModelRegistrationCodeV1::NotEndToEndCertified);
    }
    if t.contains("KIMI") && t.contains("FENCE") {
        return Some(PalwModelRegistrationCodeV1::KimiFamilyNeedsItsFence);
    }
    if t.contains("COVERAGE") || t.contains("KERNEL") && t.contains("UNCOVER") {
        return Some(PalwModelRegistrationCodeV1::CourtKernelUncovered);
    }
    if t.contains("ARTIFACT") && t.contains("ROOT") {
        return Some(PalwModelRegistrationCodeV1::ArtifactRootMismatch);
    }
    None
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwModelPreflightReportV1 {
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    pub n_ctx: u32,
    pub layer_count: u16,
    pub graph_profile: String,
    pub canonical_prefill: u32,
    pub canonical_decode: u32,
    pub admissible: bool,
    pub processor_verdict: String,
    pub reject_code: String,
    pub checks: Vec<PalwModelPreflightCheckV1>,
}

impl PalwModelPreflightReportV1 {
    pub fn empty() -> Self {
        Self {
            class_id: Hash64::default(),
            artifact_root: Hash64::default(),
            n_ctx: 0,
            layer_count: 0,
            graph_profile: String::new(),
            canonical_prefill: 0,
            canonical_decode: 0,
            admissible: false,
            processor_verdict: String::new(),
            reject_code: String::new(),
            checks: Vec::new(),
        }
    }
}

/// Processor-same preflight: fit walls plus [`verify_class_admission_v9`].
pub fn palw_model_preflight_v1(
    params: &Params,
    bundle: &PalwConsensusParamsV2,
    object: &PalwConsensusObjectV2,
    certified: &[PalwE2eFamilyV1],
    chain_certified: &[PalwE2eFamilyV1],
    daa_score: u64,
    already_registered: bool,
    registered_root: Option<Hash64>,
) -> Result<PalwModelPreflightReportV1, String> {
    let PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, admission, share_permille, .. } = object else {
        return Err("the object is not a ClassRegistered".into());
    };
    let carriage = admission.as_ref().ok_or_else(|| "the registration has no admission carriage".to_string())?;
    let profile = &carriage.profile;
    let canonical = &carriage.canonical;
    let shape = palw_admission_shape_at_v1(params, bundle, profile, daa_score)?;
    let regime = crate::palw_model_fit_v1::palw_fit_regime_for_v1(shape.held, profile);
    let fit = palw_model_fit_v2(profile, bundle, shape.court, params.palw_prompt_ids_form_at(daa_score), regime);

    let mut checks = Vec::new();
    if already_registered {
        checks.push(PalwModelPreflightCheckV1::from_code(PalwModelRegistrationCodeV1::ModelAlreadyRegistered, false));
    }
    if let Some(root) = registered_root {
        if root != *artifact_root {
            checks.push(PalwModelPreflightCheckV1::from_code(PalwModelRegistrationCodeV1::ArtifactRootMismatch, false));
        }
    }
    if let Some(row) = fit.rows.iter().find(|r| r.wall == PalwFitWallV1::GeometryCeiling) {
        let ok = row.verdict == PalwFitVerdictV1::Admitted;
        checks.push(PalwModelPreflightCheckV1::from_code(
            if ok { PalwModelRegistrationCodeV1::FitsGlobalWindow } else { PalwModelRegistrationCodeV1::GlobalWindowExceeded },
            ok,
        ));
    }

    let admission = verify_class_admission_v9(
        bundle,
        profile,
        canonical,
        object,
        certified,
        chain_certified,
        shape.ladder,
        shape.court,
        false,
        shape.token_lift,
        shape.fused_dissectable,
        params.palw_canonical_work_at(daa_score),
        shape.held,
        shape.kimi_family,
    );

    let mut reject_code = String::new();
    let mut processor_verdict = PalwModelRegistrationCodeV1::AdmissionOk.code().to_string();
    let admissible = match &admission {
        Ok(_) => {
            checks.push(PalwModelPreflightCheckV1::from_code(PalwModelRegistrationCodeV1::CourtCovered, true));
            if *share_permille > 0 {
                checks.push(PalwModelPreflightCheckV1::from_code(PalwModelRegistrationCodeV1::AdmissionOk, true));
            }
            true
        }
        Err(err) => {
            checks.push(PalwModelPreflightCheckV1::from_admission_err(err));
            reject_code = err.code().to_string();
            processor_verdict = err.code().to_string();
            if !matches!(err, PalwClassAdmissionError::CoverageGap) {
                checks.push(PalwModelPreflightCheckV1::from_code(PalwModelRegistrationCodeV1::CourtCovered, true));
            }
            if matches!(err, PalwClassAdmissionError::KimiFamilyNeedsItsFence) {
                checks.push(PalwModelPreflightCheckV1::from_code(PalwModelRegistrationCodeV1::KimiFamilyNeedsItsFence, false));
            }
            false
        }
    };

    Ok(PalwModelPreflightReportV1 {
        class_id: *class_id,
        artifact_root: *artifact_root,
        n_ctx: profile.n_ctx,
        layer_count: profile.layer_count,
        graph_profile: profile.shape_profile_id().to_string(),
        canonical_prefill: canonical.declared_prefill_tokens,
        canonical_decode: canonical.exact_decode_tokens,
        admissible: admissible && !already_registered,
        processor_verdict,
        reject_code,
        checks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admission_errors_carry_the_stable_tokens_operators_grep() {
        assert_eq!(PalwClassAdmissionError::CoverageGap.code(), "COURT_KERNEL_UNCOVERED");
        assert_eq!(PalwClassAdmissionError::NotEndToEndCertified { share: 489 }.code(), "NOT_END_TO_END_CERTIFIED");
        assert_eq!(PalwClassAdmissionError::KimiFamilyNeedsItsFence.code(), "FAMILY_FENCE_CLOSED");
        assert_eq!(
            PalwModelRegistrationCodeV1::from_admission(&PalwClassAdmissionError::CoverageGap).code(),
            "COURT_KERNEL_UNCOVERED"
        );
        assert_eq!(PalwModelRegistrationCodeV1::GlobalWindowExceeded.code(), "GLOBAL_WINDOW_EXCEEDED");
        assert_eq!(PalwModelRegistrationCodeV1::FitsGlobalWindow.code(), "FitsGlobalWindow");
        assert_eq!(PalwModelRegistrationCodeV1::CourtCovered.code(), "CourtCovered");
    }

    #[test]
    fn submit_text_maps_onto_the_2m_window_refusal() {
        let text = "class abc is not admissible: the profile is not well-formed: the declared shape drives an enumeration past the work ceiling";
        assert_eq!(
            palw_model_reject_from_submit_text_v1(text),
            Some(PalwModelRegistrationCodeV1::GlobalWindowExceeded)
        );
    }

    #[test]
    fn stages_are_ordered_so_a_cli_can_print_ticks() {
        assert!(PalwModelRegistrationStageV1::Constructed < PalwModelRegistrationStageV1::Submitted);
        assert!(PalwModelRegistrationStageV1::Submitted < PalwModelRegistrationStageV1::Accepted);
        assert!(PalwModelRegistrationStageV1::Accepted < PalwModelRegistrationStageV1::Included);
        assert!(PalwModelRegistrationStageV1::Included < PalwModelRegistrationStageV1::Folded);
    }
}
