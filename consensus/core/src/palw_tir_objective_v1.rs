//! Native original-domain TIR predicates shared by live courts and prospective
//! public adjudication. This is not an owned paid court or a filed certificate.
//! The fold must supply its accepted work/byte allocation and root any result
//! before a small StagedCourt reference can attribute an offence to it.
use crate::palw_offence_attribution_v1::{PalwClaimSourceKindV1, PalwOffenceTargetV1};
use crate::palw_state_v2::{PalwChainStateV2, PalwCourtVerdictV2};
use crate::{
    Hash64,
    palw_court_v2::{PalwCourtV2Error, PalwCourtVerdictProofV2},
};

/// A predicate finding, not a native offence/certificate. No deserializer can
/// manufacture it, and it contains no producer state or challenger checkpoint.
#[derive(Clone, Debug)]
pub struct PalwTirObjectiveFindingV1 {
    target: PalwOffenceTargetV1,
    proof_id: Hash64,
    step_leaf_count: u64,
    disputed_leaf: u64,
    verdict: crate::palw_step_leg::PalwStepRefutationVerdictV1,
}
impl PalwTirObjectiveFindingV1 {
    pub fn target_v1(&self) -> &PalwOffenceTargetV1 {
        &self.target
    }
    pub fn proof_id_v1(&self) -> Hash64 {
        self.proof_id
    }
    pub fn step_leaf_count_v1(&self) -> u64 {
        self.step_leaf_count
    }
    pub fn disputed_leaf_v1(&self) -> u64 {
        self.disputed_leaf
    }
    /// Native fault and its evidence identity. The nominated leaf alone does not
    /// establish a partial receipt's coverage of all canonical dependencies.
    pub fn verdict_v1(&self) -> &crate::palw_step_leg::PalwStepRefutationVerdictV1 {
        &self.verdict
    }
}

#[derive(Clone, Debug)]
pub enum PalwTirObjectiveOutcomeV1 {
    Convicts(PalwTirObjectiveFindingV1),
    /// Only the nominated primitive holds; never global claim honesty.
    PredicateHolds {
        claim: Hash64,
        proof_id: Hash64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwTirOutputFaultV1 {
    /// Claim-proving only. Do not forfeit an honest execution root's other users.
    OutputRootMismatch,
    /// Execution-proving: no selecting row can emit an ID outside its vocabulary.
    IdOutsideVocabulary { position: u32 },
}
#[derive(Clone, Debug)]
pub struct PalwTirOutputFindingV1 {
    target: PalwOffenceTargetV1,
    fault: PalwTirOutputFaultV1,
    computed_output: Hash64,
}
impl PalwTirOutputFindingV1 {
    pub fn target_v1(&self) -> &PalwOffenceTargetV1 {
        &self.target
    }
    pub fn fault_v1(&self) -> PalwTirOutputFaultV1 {
        self.fault
    }
    pub fn computed_output_v1(&self) -> Hash64 {
        self.computed_output
    }
}
#[derive(Clone, Debug)]
pub enum PalwTirOutputOutcomeV1 {
    Fault(PalwTirOutputFindingV1),
    /// Only output identity/domain hold. Sampling, arithmetic and semantics remain.
    OutputHolds {
        claim: Hash64,
    },
}

/// Reads an EOF-authenticated original ID sequence, not IDs from an independent
/// replay. Native acquisition must reconstruct this marker from accepted public
/// material after restart. Neither the source nor this finding is a paid grant.
pub fn adjudicate_authenticated_output_v1(
    state: &PalwChainStateV2,
    claim: Hash64,
    source: &crate::palw_tir_public_source_v1::PalwTirPublicSourceV1,
    ids: &crate::palw_tir_public_source_v1::PalwTirPublicAuthenticatedIdsV1,
) -> Result<PalwTirOutputOutcomeV1, String> {
    let target = crate::palw_offence_attribution_v1::palw_offence_target_v1(state, &claim).ok_or("TIR output target is absent")?;
    state.tir_class_v1(&target.class_id).ok_or("TIR output target has no native registered program")?;
    adjudicate_output_target_v1(target, source, ids)
}

fn original_job_matches_v1(target: &PalwOffenceTargetV1, context: &crate::palw_v2::PalwJobContextV2) -> bool {
    target.job_identity != Hash64::default()
        && match target.lane {
            Some(PalwClaimSourceKindV1::FreePrompt) => {
                crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(context) == target.job_identity
            }
            Some(PalwClaimSourceKindV1::Attempt) => context.job_id == target.job_identity,
            None => false,
        }
}

fn adjudicate_output_target_v1(
    target: PalwOffenceTargetV1,
    source: &crate::palw_tir_public_source_v1::PalwTirPublicSourceV1,
    ids: &crate::palw_tir_public_source_v1::PalwTirPublicAuthenticatedIdsV1,
) -> Result<PalwTirOutputOutcomeV1, String> {
    let header = source.header_v1();
    if ids.source_id_v1() != source.source_id_v1()
        || ids.ids_v1().len() as u64 != u64::from(header.job_context.exact_decode_tokens)
        || header.job_context.shape_profile_id != target.class_id
        || header.artifact_root != target.artifact_root
        || header.committed_execution_root != target.execution_root
        || header.full_logits_trace_root != target.trace_root
        || !original_job_matches_v1(&target, &header.job_context)
    {
        return Err("TIR output evidence changes native original class, roots, job or authenticated ID source".into());
    }
    grade_output_ids_v1(target, &header.job_context, source.vocab_v1(), ids.ids_v1())
}

/// Flat EOF pins use their original registered Flat scheme, never a tiled
/// substitute. The native owner/source row must retain the EOF-authenticated pin.
pub fn adjudicate_flat_output_v1(
    state: &PalwChainStateV2,
    claim: Hash64,
    header: &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    pin: &crate::palw_flat_constraint_stream_v1::PalwFlatConstraintPinV1,
    step_ladder: u64,
) -> Result<PalwTirOutputOutcomeV1, String> {
    let target =
        crate::palw_offence_attribution_v1::palw_offence_target_v1(state, &claim).ok_or("TIR Flat output target is absent")?;
    let record = state.tir_class_v1(&target.class_id).ok_or("TIR Flat output target has no native registered program")?;
    let view = crate::palw_decode_view_v1::PalwDecodeViewV1::from_tir_header_v1(header, record, step_ladder)?;
    if !pin.matches_view_v1(&view)
        || view.class_id != target.class_id
        || header.artifact_root != target.artifact_root
        || view.committed_execution_root != target.execution_root
        || view.full_logits_trace_root != target.trace_root
        || !original_job_matches_v1(&target, &view.job_context)
    {
        return Err("TIR Flat output pin changes native original class, roots, job or scheme".into());
    }
    grade_output_ids_v1(target, &view.job_context, view.vocab_size, pin.generated_ids_v1())
}

fn grade_output_ids_v1(
    target: PalwOffenceTargetV1,
    context: &crate::palw_v2::PalwJobContextV2,
    vocab: u32,
    ids: &[u32],
) -> Result<PalwTirOutputOutcomeV1, String> {
    let computed_output = crate::palw_attempt_rules_v1::palw_attempt_output_root_v1(context, ids);
    let invalid =
        ids.iter().position(|id| *id >= vocab).map(|position| PalwTirOutputFaultV1::IdOutsideVocabulary { position: position as u32 });
    if invalid.is_none() && target.output_root == Hash64::default() {
        return Err("native original output commitment was not retained; unknown is not an output mismatch".into());
    }
    let fault = invalid.or_else(|| (computed_output != target.output_root).then_some(PalwTirOutputFaultV1::OutputRootMismatch));
    Ok(match fault {
        Some(fault) => PalwTirOutputOutcomeV1::Fault(PalwTirOutputFindingV1 { target, fault, computed_output }),
        None => PalwTirOutputOutcomeV1::OutputHolds { claim: target.claim_id },
    })
}

/// Exact evidence grading used by the existing live TIR close. The caller must
/// first fill the registered program and pin its class/artifact/execution/trace.
/// A dissection bottom cannot be adjudicated without its accepted phase.
pub(crate) fn adjudicate_filled_v1(
    proof: &PalwCourtVerdictProofV2,
    court: &crate::palw_mode_v2::PalwCourtParamsV2,
    step_ladder: u64,
    prompt_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<PalwCourtVerdictV2, PalwCourtV2Error> {
    grade_filled_v1(proof, court, step_ladder, prompt_form).map(|judged| match judged {
        PrimitiveV1::Fault(_) => PalwCourtVerdictV2::ExecutorGuilty,
        PrimitiveV1::Holds => PalwCourtVerdictV2::ChallengerDefeated,
    })
}
enum PrimitiveV1 {
    Fault(crate::palw_step_leg::PalwStepRefutationVerdictV1),
    Holds,
}
fn grade_filled_v1(
    proof: &PalwCourtVerdictProofV2,
    court: &crate::palw_mode_v2::PalwCourtParamsV2,
    step_ladder: u64,
    prompt_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<PrimitiveV1, PalwCourtV2Error> {
    use crate::palw_tir_court_v1 as tir;
    let rules = tir::PalwTirCourtRulesV1 {
        max_step_leaf_count: step_ladder,
        prompt_form,
        limits: crate::palw_court_v2::palw_tir_court_limits_v1(court),
    };
    let result = match proof {
        PalwCourtVerdictProofV2::TirCone { refutation } => tir::check_tir_cone_refutation_v1(refutation, &rules),
        PalwCourtVerdictProofV2::TirLogits { accusation } => tir::check_tir_logits_consistency_v1(accusation, &rules),
        PalwCourtVerdictProofV2::TirDecodeTokenTiled { binding, pin } => tir::check_tir_decode_token_tiled_v1(binding, pin, &rules),
        PalwCourtVerdictProofV2::TirDecodeToken { binding, pin, position } => {
            tir::check_tir_decode_token_flat_v1(binding, pin, *position)
        }
        PalwCourtVerdictProofV2::TirDissection { .. } => return Err(PalwCourtV2Error::DissectionCloseNeedsItsSession),
        _ => return Err(PalwCourtV2Error::DoesNotAdjudicate("not an IR close".into())),
    };
    match result {
        Ok(verdict) => Ok(PrimitiveV1::Fault(verdict)),
        Err(crate::palw_step_refute::PalwStepRefuteError::NoFaultFound)
        | Err(crate::palw_step_refute::PalwStepRefuteError::Leg(crate::palw_step_leg::PalwStepLegError::NoFaultFound)) => {
            Ok(PrimitiveV1::Holds)
        }
        Err(error) => Err(PalwCourtV2Error::DoesNotAdjudicate(error.to_string())),
    }
}

/// A public arithmetic/logits proof against a live OR retained native target.
/// All terms come from the accepting fold, not the carrier: passing a large
/// budget here does not itself admit a source, extend liability or buy work.
/// Decode selection is intentionally separate: a FreePrompt's sampling/mask
/// cannot be replaced by the plain greedy primitive of a live attempt court.
pub fn adjudicate_original_v1(
    state: &PalwChainStateV2,
    claim: Hash64,
    proof: &PalwCourtVerdictProofV2,
    court: &crate::palw_mode_v2::PalwCourtParamsV2,
    step_ladder: u64,
    prompt_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    allocated_proof_bytes: u64,
) -> Result<PalwTirObjectiveOutcomeV1, String> {
    let target = crate::palw_offence_attribution_v1::palw_offence_target_v1(state, &claim).ok_or("TIR objective target is absent")?;
    let record = state.tir_class_v1(&target.class_id).ok_or("TIR objective target has no native registered program")?;
    adjudicate_target_v1(target, record, proof, court, step_ladder, prompt_form, allocated_proof_bytes)
}

fn adjudicate_target_v1(
    target: PalwOffenceTargetV1,
    record: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
    carried: &PalwCourtVerdictProofV2,
    court: &crate::palw_mode_v2::PalwCourtParamsV2,
    step_ladder: u64,
    prompt_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    allocated_proof_bytes: u64,
) -> Result<PalwTirObjectiveOutcomeV1, String> {
    let disputed_leaf = match carried {
        PalwCourtVerdictProofV2::TirCone { refutation } => refutation.output_opening.leaf_index,
        PalwCourtVerdictProofV2::TirLogits { accusation } => accusation.step_opening.leaf_index,
        _ => return Err("public TIR arithmetic adjudication needs its own cone/logits predicate, not a session/decode verdict".into()),
    };
    if allocated_proof_bytes == 0 || borsh::object_length(carried).map_err(|e| e.to_string())? as u64 > allocated_proof_bytes {
        return Err("TIR public proof exceeds its original allocated body".into());
    }
    let mut proof = carried.clone();
    let binding = proof.tir_binding_mut_v1().ok_or("not an original TIR proof")?;
    *binding = crate::palw_tir_admission_v1::palw_tir_binding_with_program_v1(binding, record).map_err(str::to_string)?;
    if binding.class.class_id(&binding.artifact_root) != target.class_id
        || binding.artifact_root != target.artifact_root
        || binding.committed_execution_root != target.execution_root
        || binding.full_logits_trace_root != target.trace_root
        || !original_job_matches_v1(&target, &binding.job_context)
    {
        return Err("TIR public proof changes native original class, artifact, execution, trace or job".into());
    }
    let step_leaf_count = binding.step_leaf_count;
    let verdict = grade_filled_v1(&proof, court, step_ladder, prompt_form).map_err(|e| e.to_string())?;
    let bytes = borsh::to_vec(carried).map_err(|e| e.to_string())?;
    let proof_id = Hash64::from_bytes(
        blake2b_simd::Params::new()
            .hash_length(64)
            .key(b"misaka-palw/tir-objective-proof/v1")
            .hash(&bytes)
            .as_bytes()
            .try_into()
            .expect("64"),
    );
    match verdict {
        PrimitiveV1::Fault(verdict) => Ok(PalwTirObjectiveOutcomeV1::Convicts(PalwTirObjectiveFindingV1 {
            target,
            proof_id,
            step_leaf_count,
            disputed_leaf,
            verdict,
        })),
        PrimitiveV1::Holds => Ok(PalwTirObjectiveOutcomeV1::PredicateHolds { claim: target.claim_id, proof_id }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_prompt_ids_v1::PalwPromptIdsFormV1;
    use crate::palw_tir_court_v1::{
        PalwTirCourtRulesV1, build_tir_cone_refutation_v1,
        test_support::{TinyExecution, tiny_execution},
    };
    fn target(x: &TinyExecution) -> PalwOffenceTargetV1 {
        PalwOffenceTargetV1 {
            claim_id: Hash64::from_u64_word(7),
            class_id: x.binding.job_context.shape_profile_id,
            artifact_root: x.binding.artifact_root,
            executor_bond: crate::palw_state_v2::PALW_BOND_KEY_V2_MIN,
            execution_root: x.binding.committed_execution_root,
            lane: Some(PalwClaimSourceKindV1::FreePrompt),
            segment_count: None,
            phase: None,
            job_identity: crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&x.binding.job_context),
            trace_root: x.binding.full_logits_trace_root,
            output_root: Hash64::default(),
            registered_semantics: None,
        }
    }
    fn proof(x: &TinyExecution) -> PalwCourtVerdictProofV2 {
        let rules = PalwTirCourtRulesV1 {
            max_step_leaf_count: 1 << 26,
            prompt_form: PalwPromptIdsFormV1::Flat,
            limits: misaka_palw_tir::demand::DemandLimits::UNLIMITED,
        };
        let mut proof =
            PalwCourtVerdictProofV2::TirCone { refutation: Box::new(build_tir_cone_refutation_v1(&x.binding, 7, x, &rules).unwrap()) };
        proof.tir_strip_program_v1();
        proof
    }
    fn judge(
        x: &TinyExecution,
        target: PalwOffenceTargetV1,
        proof: &PalwCourtVerdictProofV2,
        maximum: u64,
    ) -> Result<PalwTirObjectiveOutcomeV1, String> {
        let (record, _) = crate::palw_tir_admission_v1::palw_tir_class_record_v1(&x.binding.class, &x.binding.artifact_root).unwrap();
        let court = crate::palw_mode_v2::PalwCourtParamsV2::new(1 << 26, 10, 2).unwrap();
        adjudicate_target_v1(target, &record, proof, &court, 1 << 26, PalwPromptIdsFormV1::Flat, maximum)
    }
    #[test]
    fn native_original_arithmetic_finding_is_not_global_honesty_or_a_filed_offence() {
        let honest = tiny_execution(None);
        let bad = tiny_execution(Some((7, 1)));
        assert!(matches!(
            judge(&honest, target(&honest), &proof(&honest), 1 << 22).unwrap(),
            PalwTirObjectiveOutcomeV1::PredicateHolds { .. }
        ));
        let carried = proof(&bad);
        let PalwTirObjectiveOutcomeV1::Convicts(found) = judge(&bad, target(&bad), &carried, 1 << 22).unwrap() else {
            panic!("fault required")
        };
        assert_eq!(found.target_v1().claim_id, target(&bad).claim_id);
        assert_eq!(found.disputed_leaf_v1(), 7);
        assert_eq!(found.step_leaf_count_v1(), bad.binding.step_leaf_count);
        assert_ne!(found.proof_id_v1(), Hash64::default());
        let state = PalwChainStateV2::genesis();
        let court = crate::palw_mode_v2::PalwCourtParamsV2::new(1 << 26, 10, 2).unwrap();
        assert!(
            adjudicate_original_v1(&state, found.target_v1().claim_id, &carried, &court, 1 << 26, PalwPromptIdsFormV1::Flat, 1 << 22)
                .is_err()
        );
    }
    #[test]
    fn changed_original_pins_or_unbought_body_never_produce_a_finding() {
        let x = tiny_execution(Some((7, 1)));
        let carried = proof(&x);
        for field in 0..6 {
            let mut t = target(&x);
            match field {
                0 => t.class_id = Hash64::default(),
                1 => t.artifact_root = Hash64::default(),
                2 => t.execution_root = Hash64::default(),
                3 => t.trace_root = Hash64::default(),
                4 => t.job_identity = Hash64::default(),
                _ => t.lane = None,
            }
            assert!(judge(&x, t, &carried, 1 << 22).is_err());
        }
        assert!(judge(&x, target(&x), &carried, 1).is_err());
        let mut program_on_wire = carried.clone();
        program_on_wire.tir_binding_mut_v1().unwrap().class.program = x.binding.class.program.clone();
        assert!(judge(&x, target(&x), &program_on_wire, 1 << 22).is_err());
        let mut missing = carried;
        let PalwCourtVerdictProofV2::TirCone { ref mut refutation } = missing else { unreachable!() };
        refutation.operands.preimages.clear();
        assert!(judge(&x, target(&x), &missing, 1 << 22).is_err());
    }

    fn output_material(
        x: &TinyExecution,
    ) -> (crate::palw_tir_public_source_v1::PalwTirPublicSourceV1, crate::palw_tir_public_source_v1::PalwTirPublicAuthenticatedIdsV1)
    {
        use crate::palw_tir_public_source_v1::*;
        let (record, _) = crate::palw_tir_admission_v1::palw_tir_class_record_v1(&x.binding.class, &x.binding.artifact_root).unwrap();
        let header = crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1::of_binding_v1(&x.binding);
        let source = PalwTirPublicSourceV1::new_v1(
            &record,
            &x.binding.class.layout,
            &header,
            x.binding.committed_execution_root,
            x.binding.full_logits_trace_root,
            1 << 26,
        )
        .unwrap();
        let rows_root = crate::palw_step_refute::tiled_logits_rows_root_v1(&x.binding.job_context, &x.rows).unwrap();
        let ids = PalwTirPublicIdsAssemblerV1::default()
            .push_v1(
                &source,
                &PalwTirPublicRequestV1::Ids { offset: 0 },
                &PalwTirPublicResponseV1::Ids { rows_root, ids: x.generated.clone() },
            )
            .unwrap()
            .unwrap();
        (source, ids)
    }
    #[test]
    fn authenticated_output_separates_claim_mismatch_domain_fault_and_unknown_commitment() {
        let x = tiny_execution(None);
        let (source, ids) = output_material(&x);
        let computed = crate::palw_attempt_rules_v1::palw_attempt_output_root_v1(&x.binding.job_context, &x.generated);
        let mut t = target(&x);
        t.output_root = computed;
        assert!(matches!(adjudicate_output_target_v1(t.clone(), &source, &ids).unwrap(), PalwTirOutputOutcomeV1::OutputHolds { .. }));
        t.output_root = Hash64::from_u64_word(999);
        let PalwTirOutputOutcomeV1::Fault(fault) = adjudicate_output_target_v1(t.clone(), &source, &ids).unwrap() else {
            panic!("claim mismatch")
        };
        assert_eq!(fault.fault_v1(), PalwTirOutputFaultV1::OutputRootMismatch);
        assert_eq!(fault.computed_output_v1(), computed);
        t.output_root = Hash64::default();
        assert!(adjudicate_output_target_v1(t, &source, &ids).is_err(), "an old/vesting target may not retain output_root");

        // The authenticated original may itself be impossible. Retaining the
        // bad ID is essential; an earlier input filter cannot replace this proof.
        let mut bad = tiny_execution(None);
        bad.generated[0] = source.vocab_v1();
        bad.binding.full_logits_trace_root =
            crate::palw_step_refute::tiled_logits_trace_root_v1(&bad.binding.job_context, &bad.rows, &bad.generated).unwrap();
        bad.binding.committed_execution_root = crate::palw_tir_step_v1::palw_tir_execution_root_v1(
            &bad.binding.job_context.context_hash(),
            &bad.binding.full_logits_trace_root,
            &bad.binding.job_context.shape_profile_id,
            bad.binding.step_leaf_count,
            &bad.binding.step_merkle_root,
        );
        let (bad_source, bad_ids) = output_material(&bad);
        let PalwTirOutputOutcomeV1::Fault(fault) = adjudicate_output_target_v1(target(&bad), &bad_source, &bad_ids).unwrap() else {
            panic!("domain fault")
        };
        assert_eq!(fault.fault_v1(), PalwTirOutputFaultV1::IdOutsideVocabulary { position: 0 });
        assert!(
            adjudicate_output_target_v1(target(&bad), &bad_source, &ids).is_err(),
            "IDs from another original root are not evidence"
        );
    }
}
