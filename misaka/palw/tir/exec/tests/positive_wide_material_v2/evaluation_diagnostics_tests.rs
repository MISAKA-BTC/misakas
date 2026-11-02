//! Observation never changes failure classification, spent charges or coverage.
use super::*;
use kaspa_consensus_core::palw_tir_court_v1::source_only::wide_v2::{EvaluationDiagnosticsV2, EvaluationStageV2};

fn preparation(signed: &SignedMiner, f: &Fixture<'_>, limits: FullRelationLimits) -> FullRelationPreparationV2 {
    FullRelationPreparationV2::authenticate(
        &signed.rooted,
        &f.class,
        f.artifact,
        &f.job,
        PalwPromptIdsFormV1::Flat,
        limits,
        domains(),
        signed.result.clone(),
        &signed.key,
        &signed.signature,
    )
    .unwrap()
}

#[test]
fn actual_work_read_and_workspace_refusals_keep_legacy_outcomes_and_charges() {
    for failure in 0..3 {
        run_wide_common::with_fixture(1, |f, _| {
            let space = PalwTirStepSpaceV2::new(&f.class).unwrap();
            let runner = TirClassRunnerWideQ24V2::new(&space, f.plan, f.params, f.job.shape_profile_id).unwrap();
            let work = WidePanelWorkV2::new(bounds(), f.job.shape_profile_id, f.job.context_hash(), &disk_reserve()).unwrap();
            let capture = capture_one_pass_v2(
                CaptureExpectedV2 { class: &f.class, artifact: f.artifact, job: &f.job, space: &space, prompt: PROMPT },
                &runner,
                run_bounds(),
                &work,
                tempfile::tempfile().unwrap(),
                &reserve(),
            )
            .unwrap();
            let signed = SignedMiner::new(f, false);
            let mut limits = full_limits();
            match failure {
                0 => {
                    limits.plan.max_elements_per_relation = 1;
                    limits.plan.max_terms_per_relation = 1;
                }
                1 => limits.source.max_workspace_bytes = 1,
                _ => limits.source.max_source_reads = 1,
            }
            let prep = preparation(&signed, f, limits);
            let source = TirParamsSourceV1(f.params);
            let inventory = TirHeldInventoryV1::build(f.program, &source).unwrap();
            let mut owner = owners();
            owner.material_limits.max_source_reads = owner.material_limits.max_source_reads.min(limits.source.max_source_reads);
            owner.material_limits.max_workspace_bytes =
                owner.material_limits.max_workspace_bytes.min(limits.source.max_workspace_bytes);
            owner.source_auth_bytes = limits.source.max_auth_storage_bytes;
            owner.metadata_auth_bytes = full_limits().source.max_workspace_bytes;
            let authenticated = signed.authenticate();
            let builder = WidePanelBuilderV2::prepare(
                &capture,
                &prep,
                &authenticated,
                &signed.rooted,
                &inventory,
                provider(),
                owner,
                &reserve(),
            )
            .unwrap();
            let tape = builder.claims_tape(tempfile::tempfile().unwrap()).unwrap();
            let mut claims = vec![];
            tape.visit(&mut |_, values| {
                claims.push(values.to_vec());
                Ok(())
            })
            .unwrap();
            let panel = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([98; 32]);
            let record = || {
                prep.freeze_record(
                    capture.binding().unwrap(),
                    capture.run().generated().to_vec(),
                    principal(panel.verification_key.as_ref()),
                    claims.clone(),
                )
                .unwrap()
            };
            let original = record();
            let signature = *libcrux_ml_dsa::ml_dsa_87::sign(
                &panel.signing_key,
                &original.signing_digest().0,
                original.signature_context(),
                [0; 32],
            )
            .unwrap()
            .as_ref();
            let mut plain = preparation(&signed, f, limits).begin(original, panel.verification_key.as_ref(), &signature).unwrap();
            let mut observed = preparation(&signed, f, limits).begin(record(), panel.verification_key.as_ref(), &signature).unwrap();
            let mut refused = false;
            for index in 0..prep.plan().summary().relations {
                let frontier = plain.progress().legacy_frontier;
                let material = builder.material(index, frontier).unwrap();
                let expected = plain.submit(&material.as_borrowed(), material.target());
                // A stale observation cannot survive an attempted submission.
                let mut diagnostics = EvaluationDiagnosticsV2 { stage: EvaluationStageV2::Complete, ..Default::default() };
                let actual = observed.submit_with_diagnostics(&material.as_borrowed(), material.target(), &mut diagnostics);
                assert_eq!(expected, actual);
                assert_eq!(plain.progress(), observed.progress());
                if actual.is_err() {
                    assert_eq!(actual, Err(FullRelationError::Budget));
                    assert_eq!(observed.progress().outcome, CheckOutcome::EndedUnverified);
                    assert_eq!(observed.progress().next_relation, index);
                    match failure {
                        0 => {
                            assert_eq!(diagnostics.stage, EvaluationStageV2::Evaluation);
                            let work = diagnostics.work.expect("actual WorkLimit cost must be preserved");
                            assert!(work.elements > 1 || work.terms > 1);
                        }
                        1 => {
                            assert_eq!(diagnostics.stage, EvaluationStageV2::Workspace);
                            assert_eq!(diagnostics.workspace_bytes, None);
                            assert_eq!(diagnostics.work, None);
                            assert_eq!(diagnostics.source_reads, 0);
                        }
                        _ => {
                            assert_eq!(diagnostics.stage, EvaluationStageV2::Evaluation);
                            assert_eq!(diagnostics.source_reads, 1);
                        }
                    }
                    let spent = observed.progress();
                    assert_eq!(
                        observed.submit_with_diagnostics(&material.as_borrowed(), material.target(), &mut diagnostics),
                        Err(FullRelationError::Closed)
                    );
                    assert_eq!(observed.progress(), spent);
                    assert_eq!(diagnostics, EvaluationDiagnosticsV2::default());
                    refused = true;
                    break;
                }
            }
            assert!(refused, "the actual bounded evaluator must reach the intended refusal");
        });
    }
}
