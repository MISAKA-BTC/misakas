//! Typed native-court boundary shared by retained evidence assembly and the
//! actual held terminal consumer. It adds no verdict or evidence acceptance.
use kaspa_consensus_core::{
    palw_native_resource_v1::NativeDecodeErrorV1,
    palw_step_refute::{PalwStepRefuteError, memory_v2::PalwGdnCourtErrorV2},
};

/// Preserve local capacity exactly, including when a predecessor callback also
/// recorded a semantic/recompute refusal. Legacy String wrappers may format
/// this only at their outermost compatibility boundary.
pub fn base0_gdn_court_error_with_replay_v3(
    error: PalwGdnCourtErrorV2,
    replay_failure: Option<String>,
) -> NativeDecodeErrorV1<String> {
    match error {
        PalwGdnCourtErrorV2::Resource(capacity) => NativeDecodeErrorV1::Resource(capacity),
        other @ PalwGdnCourtErrorV2::Evidence(_) => NativeDecodeErrorV1::Decode(replay_failure.unwrap_or_else(|| other.to_string())),
    }
}

pub fn base0_gdn_court_error_v3(error: PalwGdnCourtErrorV2) -> NativeDecodeErrorV1<String> {
    base0_gdn_court_error_with_replay_v3(error, None)
}

/// Only the native terminal's exact authenticated NoFaultFound advances to the
/// next candidate. Neither local capacity nor another evidence refusal is an
/// honest result or a conviction.
pub fn base0_gdn_terminal_fault_v3<T>(verdict: Result<T, PalwGdnCourtErrorV2>) -> Result<bool, NativeDecodeErrorV1<String>> {
    match verdict {
        Ok(_) => Ok(true),
        Err(PalwGdnCourtErrorV2::Evidence(PalwStepRefuteError::NoFaultFound)) => Ok(false),
        Err(error) => Err(base0_gdn_court_error_v3(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1;

    #[test]
    fn native_court_capacity_is_not_evidence_or_honesty_even_with_callback_failure() {
        let capacity = NativeCoverageV1::unsupported::<u8>("court test", "local admission");
        let mapped = base0_gdn_court_error_with_replay_v3(
            PalwGdnCourtErrorV2::Resource(capacity.clone()),
            Some("earlier predecessor callback refusal".into()),
        );
        assert!(matches!(mapped, NativeDecodeErrorV1::Resource(found) if found == capacity));
        assert!(matches!(
            base0_gdn_terminal_fault_v3::<()>(Err(PalwGdnCourtErrorV2::Resource(capacity))),
            Err(NativeDecodeErrorV1::Resource(_))
        ));
        assert!(!base0_gdn_terminal_fault_v3::<()>(Err(PalwGdnCourtErrorV2::Evidence(PalwStepRefuteError::NoFaultFound))).unwrap());
        assert!(base0_gdn_terminal_fault_v3(Ok(())).unwrap());
        assert!(matches!(
            base0_gdn_terminal_fault_v3::<()>(Err(PalwGdnCourtErrorV2::Evidence(PalwStepRefuteError::InputSetNotCanonical(
                "foreign evidence"
            )))),
            Err(NativeDecodeErrorV1::Decode(_))
        ));
        let mapped = base0_gdn_court_error_with_replay_v3(
            PalwGdnCourtErrorV2::Evidence(PalwStepRefuteError::Unadjudicable),
            Some("original callback refusal".into()),
        );
        assert!(matches!(mapped, NativeDecodeErrorV1::Decode(reason) if reason == "original callback refusal"));
    }
}
