pub use kaspa_consensus_core::*;
use kaspa_consensus_core::palw_backend::*;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
#[path="/Users/wata/.codex/worktrees/rfc14-g14-integration-candidate/MISAKA-testnet/consensus/core/src/palw_backend/operations.rs"]
mod operations;
use operations::*;
#[cfg(test)]
mod operation_view_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Configured {
        rules: crate::palw_attempt_rules_v1::PalwAttemptRulesV1,
        calls: AtomicUsize,
    }
    impl PalwExecutionBackendV1 for Configured {
        fn model_id(&self) -> &str { "configured-operation-view" }
        fn job_for_anchor(&self, _anchor: Hash64) -> Result<(PalwJobContextV2, Vec<usize>), String> {
            Err("no anchor fixture".into())
        }
        fn set_attempt_rules_v1(&mut self, rules: crate::palw_attempt_rules_v1::PalwAttemptRulesV1) {
            self.rules = rules;
        }
        fn attempt_rules_v1(&self) -> crate::palw_attempt_rules_v1::PalwAttemptRulesV1 { self.rules }
        fn execute(&self, _job: &PalwJobContextV2, prompt: &[usize]) -> Result<PalwExecutionOutcomeV1, String> {
            assert_eq!(prompt, &[3, 5]);
            self.calls.fetch_add(1, Ordering::SeqCst);
            let answer = if self.rules == crate::palw_attempt_rules_v1::PalwAttemptRulesV1::CoreV1 { 11 } else { 12 };
            Ok(PalwExecutionOutcomeV1 {
                trace_root: Hash64::from_u64_word(1), output_root: Hash64::from_u64_word(answer),
                execution_root: Hash64::from_u64_word(3), trace_manifest_root: Hash64::from_u64_word(4),
                trace_chunk_count: 1, material: Vec::new(),
            })
        }
        fn verify_material(&self, _material: &[u8], _claim: PalwClaimRootsV1) -> PalwMaterialVerdictV1 {
            PalwMaterialVerdictV1::Unverifiable
        }
    }
    fn configured() -> Configured {
        let mut backend = Configured { rules: Default::default(), calls: AtomicUsize::new(0) };
        backend.set_attempt_rules_v1(crate::palw_attempt_rules_v1::PalwAttemptRulesV1::CoreV1);
        backend
    }

    #[test]
    fn operation_views_keep_the_configured_instance_and_its_inherited_replay() {
        let backend = configured();
        let job = crate::palw_base0_profile::rc_job_context(
            &crate::palw_base0_profile::base0_profile_v1(crate::palw_base0_profile::PALW_RC_BASE0_GEOMETRY).unwrap(), 8, 4,
        );
        let execution = PalwExecutionOpsV1::new(&backend);
        assert_eq!(execution.attempt_rules_v1(), crate::palw_attempt_rules_v1::PalwAttemptRulesV1::CoreV1);
        let direct = execution.execute(&job, &[3, 5]).unwrap();
        // The compatibility trait's default replay calls THIS instance's overridden execution.
        let replay = PalwReplayOpsV1::new(&backend).execute_for_verdict(&job, &[3, 5]).unwrap();
        assert_eq!(direct.output_root, Hash64::from_u64_word(11));
        assert_eq!(replay.output_root, Some(direct.output_root));
        assert_eq!(backend.calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn unsupported_operations_stay_refusals_or_absent_in_every_view() {
        let backend = configured();
        assert!(PalwMaterialOpsV1::new(&backend).open_fp_interval(&[], 0, &[]).is_err());
        assert!(PalwCourtEvidenceOpsV1::new(&backend).attn_site_evidence_held_v1(&[], 0, None, None).is_err());
        assert!(PalwResourceOpsV1::new(&backend).runtime_profile_v1().is_none());
        assert!(PalwResourceOpsV1::new(&backend).capture_shape(&[]).is_none());
        assert_eq!(backend.calls.load(Ordering::SeqCst), 0, "an absent capability does not start a substitute execution");
    }
}
