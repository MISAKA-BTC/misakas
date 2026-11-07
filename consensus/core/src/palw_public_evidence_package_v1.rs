//! One claim-bound court evidence package for a localized primitive.
//!
//! A node recomputes that primitive from the committed input. The package is
//! accepted as a conviction only when its openings match the original claim and
//! the recomputed value differs from the recorded output. A Panel signature or
//! quorum is not an input. Batch clock version 5 refuses, at plan time, a
//! package that does not fit the admission byte cap already applied to an
//! objection artifact. The package is not clipped.
//!
//! This is not a measured-class record, not an activation, and not a preset.

use crate::Hash64;
use crate::palw_step_leg::{PalwStepOpeningV1, step_opening_root_capped_v1};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::{DType, Prim, Tensor};

/// Rule this package is judged under. A different version is refused.
pub const PALW_PUBLIC_EVIDENCE_RULE_VERSION_V1: u16 = 1;

/// The original claim the package must match. These fields are the court's,
/// not the filer's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwPublicOriginalClaimV1 {
    pub claim_id: Hash64,
    pub model: Hash64,
    pub input: Hash64,
    pub execution: Hash64,
    pub trace: Hash64,
    pub output: Hash64,
    pub coordinate: u64,
    pub rule_version: u16,
    /// Width of the tree the openings are checked against.
    pub leaf_count: u64,
}

/// Claim binding carried in the package: model, input, execution, trace,
/// output, coordinate, and rule version, plus the claim id those roots name.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPublicClaimBindingV1 {
    pub claim_id: Hash64,
    pub model: Hash64,
    pub input: Hash64,
    pub execution: Hash64,
    pub trace: Hash64,
    pub output: Hash64,
    pub coordinate: u64,
    pub rule_version: u16,
}

/// One committed scalar and the Merkle opening of its identity.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPublicScalarOpeningV1 {
    pub dtype: DType,
    pub value: i128,
    pub opening: PalwStepOpeningV1,
}

/// The operation the court runs on the committed input. The recorded output
/// is not an input to this operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwPublicExpectedOperationV1 {
    /// Saturate the committed scalar into `[lo, hi]`.
    Clamp { lo: i64, hi: i64, out: DType },
}

/// Court evidence for one localized mismatch.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPublicEvidencePackageV1 {
    pub binding: PalwPublicClaimBindingV1,
    pub committed_input: PalwPublicScalarOpeningV1,
    pub expected_operation: PalwPublicExpectedOperationV1,
    pub recorded_output: i128,
    /// Merkle opening of the recorded output's identity against the claim output.
    pub identity_opening: PalwStepOpeningV1,
}

/// What the native primitive evaluator returned for this package.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwPublicEvidenceJudgmentV1 {
    /// The opening matched and the recomputed value differs from the record.
    Mismatch { recomputed: i128, recorded: i128 },
    /// The opening matched and the recomputed value is the recorded output.
    Holds { recomputed: i128 },
}

/// Plan-time admission of one package on Batch clock version 5.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPublicEvidenceAdmissionV1 {
    pub admitted: bool,
    pub package_bytes: u64,
    pub refusal: Option<crate::palw_tir_frame_cost_v1::BatchPublicObjectionRefusalV1>,
}

/// Deadlines and the court whose close-byte cap is the admission cap.
#[derive(Clone, Copy, Debug)]
pub struct PalwBatchV5EvidencePlanV1<'a> {
    pub clock_version: u16,
    pub court: &'a crate::palw_mode_v2::PalwCourtParamsV2,
    pub localization_daa: u64,
    pub filing_daa: u64,
    pub accepted_daa: u64,
    pub collect_until_daa: u64,
}

fn scalar_identity_v1(dtype: DType, value: i128) -> Hash64 {
    let mut body = Vec::with_capacity(1 + 16);
    body.push(dtype.tag());
    body.extend_from_slice(&value.to_le_bytes());
    let digest = blake2b_simd::Params::new()
        .hash_length(64)
        .key(b"misaka-palw/public-evidence-scalar/v1")
        .hash(&body);
    Hash64::from_bytes(digest.as_bytes().try_into().expect("64"))
}

fn output_dtype_v1(operation: PalwPublicExpectedOperationV1) -> DType {
    match operation {
        PalwPublicExpectedOperationV1::Clamp { out, .. } => out,
    }
}

fn opening_matches_v1(leaf_count: u64, coordinate: u64, root: Hash64, dtype: DType, value: i128, opening: &PalwStepOpeningV1) -> Result<(), String> {
    if opening.leaf_index != coordinate {
        return Err("public evidence opening names another coordinate".into());
    }
    if opening.leaf_hash != scalar_identity_v1(dtype, value) {
        return Err("public evidence identity opening does not match the carried value".into());
    }
    let got = step_opening_root_capped_v1(leaf_count, opening, leaf_count).map_err(|e| e.to_string())?;
    if got != root {
        return Err("public evidence Merkle opening does not match the original claim".into());
    }
    Ok(())
}

/// Byte length of the package. The bytes are not shortened.
pub fn public_evidence_package_bytes_v1(package: &PalwPublicEvidencePackageV1) -> Result<u64, String> {
    let len = borsh::object_length(package).map_err(|e| e.to_string())?;
    u64::try_from(len).map_err(|_| "public evidence package length exceeds u64".to_string())
}

/// Recompute the one named primitive from the committed input.
/// The recorded output is not read.
pub fn recompute_committed_primitive_v1(package: &PalwPublicEvidencePackageV1) -> Result<i128, String> {
    let input = &package.committed_input;
    let tensor = Tensor::new(input.dtype, vec![1], vec![input.value]).map_err(|e| e.to_string())?;
    let value = match package.expected_operation {
        PalwPublicExpectedOperationV1::Clamp { lo, hi, out } => {
            let got = misaka_palw_tir::eval::eval_primitive(&Prim::Clamp { lo, hi }, &[tensor], out, &[1]).map_err(|e| e.to_string())?;
            *got.data.first().ok_or("public evidence primitive returned no value")?
        }
    };
    Ok(value)
}

fn binding_matches_v1(claim: &PalwPublicOriginalClaimV1, binding: &PalwPublicClaimBindingV1) -> Result<(), String> {
    if claim.rule_version != PALW_PUBLIC_EVIDENCE_RULE_VERSION_V1 || binding.rule_version != PALW_PUBLIC_EVIDENCE_RULE_VERSION_V1 {
        return Err("public evidence rule version is not the pinned rule".into());
    }
    if binding.claim_id != claim.claim_id {
        return Err("public evidence package names a different claim id".into());
    }
    if (binding.model, binding.input, binding.execution, binding.trace, binding.output, binding.coordinate, binding.rule_version)
        != (claim.model, claim.input, claim.execution, claim.trace, claim.output, claim.coordinate, claim.rule_version)
    {
        return Err("public evidence binding does not match the original claim".into());
    }
    Ok(())
}

/// Check the openings, recompute the primitive, and report the mismatch.
/// `allocated_proof_bytes` is the same body ceiling [`crate::palw_tir_objective_v1::adjudicate_original_v1`]
/// applies. A package over that ceiling is refused whole.
pub fn judge_public_evidence_package_v1(
    claim: &PalwPublicOriginalClaimV1,
    package: &PalwPublicEvidencePackageV1,
    allocated_proof_bytes: u64,
) -> Result<PalwPublicEvidenceJudgmentV1, String> {
    let package_bytes = public_evidence_package_bytes_v1(package)?;
    if allocated_proof_bytes == 0 || package_bytes > allocated_proof_bytes {
        return Err("public evidence package exceeds its original allocated body".into());
    }
    binding_matches_v1(claim, &package.binding)?;
    if claim.leaf_count == 0 {
        return Err("public evidence claim has no leaf tree".into());
    }
    opening_matches_v1(
        claim.leaf_count,
        claim.coordinate,
        claim.input,
        package.committed_input.dtype,
        package.committed_input.value,
        &package.committed_input.opening,
    )?;
    opening_matches_v1(
        claim.leaf_count,
        claim.coordinate,
        claim.output,
        output_dtype_v1(package.expected_operation),
        package.recorded_output,
        &package.identity_opening,
    )?;
    let recomputed = recompute_committed_primitive_v1(package)?;
    if recomputed == package.recorded_output {
        Ok(PalwPublicEvidenceJudgmentV1::Holds { recomputed })
    } else {
        Ok(PalwPublicEvidenceJudgmentV1::Mismatch { recomputed, recorded: package.recorded_output })
    }
}

/// One Clamp package: committed input 150, recomputed 100, `recorded` as given.
///
/// The openings match `claim_id` and the scalar roots this function commits.
/// A Panel signature is not part of the package.
#[cfg(test)]
pub(crate) fn clamp_public_evidence_package_v1(
    claim_id: Hash64,
    recorded: i128,
) -> (PalwPublicOriginalClaimV1, PalwPublicEvidencePackageV1) {
    use crate::palw_step_leg::{step_merkle_root_v1, step_opening_v1};
    const INPUT: i128 = 150;
    const RECOMPUTED: i128 = 100;
    let dtype = DType::I32;
    let operation = PalwPublicExpectedOperationV1::Clamp { lo: 0, hi: RECOMPUTED as i64, out: dtype };
    let input_leaf = scalar_identity_v1(dtype, INPUT);
    let output_leaf = scalar_identity_v1(dtype, recorded);
    let neighbor = Hash64::from_u64_word(9);
    let input_leaves = [input_leaf, neighbor];
    let output_leaves = [output_leaf, neighbor];
    let input = step_merkle_root_v1(&input_leaves).unwrap();
    let output = step_merkle_root_v1(&output_leaves).unwrap();
    let claim = PalwPublicOriginalClaimV1 {
        claim_id,
        model: Hash64::from_u64_word(2),
        input,
        execution: Hash64::from_u64_word(3),
        trace: Hash64::from_u64_word(4),
        output,
        coordinate: 0,
        rule_version: PALW_PUBLIC_EVIDENCE_RULE_VERSION_V1,
        leaf_count: 2,
    };
    let package = PalwPublicEvidencePackageV1 {
        binding: PalwPublicClaimBindingV1 {
            claim_id,
            model: claim.model,
            input,
            execution: claim.execution,
            trace: claim.trace,
            output,
            coordinate: claim.coordinate,
            rule_version: claim.rule_version,
        },
        committed_input: PalwPublicScalarOpeningV1 {
            dtype,
            value: INPUT,
            opening: step_opening_v1(&input_leaves, 0).unwrap(),
        },
        expected_operation: operation,
        recorded_output: recorded,
        identity_opening: step_opening_v1(&output_leaves, 0).unwrap(),
    };
    (claim, package)
}

/// Batch version 5 only. The byte cap is the objection cap
/// [`crate::palw_tir_frame_cost_v1::batch_public_objection_refused_v1`] already
/// uses at admission: the court close ceiling and the carriable close, together
/// with the 6,880 public span and `collect_until <= hard`. An oversized package
/// is not admitted and is not shortened to the cap.
pub fn plan_batch_v5_evidence_package_v1(
    package: &PalwPublicEvidencePackageV1,
    plan: PalwBatchV5EvidencePlanV1<'_>,
) -> Result<PalwPublicEvidenceAdmissionV1, String> {
    if !crate::palw_tir_frame_cost_v1::is_batch_public_clock_v5(plan.clock_version) {
        return Err("public evidence admission applies only to batch clock version 5".into());
    }
    let package_bytes = public_evidence_package_bytes_v1(package)?;
    let refusal = crate::palw_tir_frame_cost_v1::batch_public_objection_refused_v1(
        package_bytes,
        0,
        plan.court,
        plan.localization_daa,
        plan.filing_daa,
        plan.accepted_daa,
        plan.collect_until_daa,
    );
    Ok(PalwPublicEvidenceAdmissionV1 { admitted: refusal.is_none(), package_bytes, refusal })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_tir_frame_cost_v1::BatchPublicObjectionRefusalV1;
    use crate::palw_tir_objective_v1::{PalwTirObjectiveOutcomeV1, adjudicate_original_package_v1};

    const INPUT: i128 = 150;
    const RECOMPUTED: i128 = 100;
    const RECORDED: i128 = 120;

    fn court(max_close_bytes: u64) -> crate::palw_mode_v2::PalwCourtParamsV2 {
        crate::palw_mode_v2::PalwCourtParamsV2::with_cost_ceilings(4, 10, 1, max_close_bytes, 1, 1).unwrap()
    }

    fn fitting_plan(court: &crate::palw_mode_v2::PalwCourtParamsV2) -> PalwBatchV5EvidencePlanV1<'_> {
        let accepted_daa = 1_000;
        PalwBatchV5EvidencePlanV1 {
            clock_version: crate::palw_tir_frame_cost_v1::PALW_TIR_FRAME_CLOCK_BATCH_PUBLIC_V5,
            court,
            localization_daa: 1,
            filing_daa: 1,
            accepted_daa,
            collect_until_daa: accepted_daa + crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA,
        }
    }

    fn package_at(recorded: i128) -> (PalwPublicOriginalClaimV1, PalwPublicEvidencePackageV1) {
        clamp_public_evidence_package_v1(Hash64::from_u64_word(1), recorded)
    }

    #[test]
    fn committed_input_recomputes_to_100_and_recorded_120_convicts() {
        let (claim, package) = package_at(RECORDED);
        let recomputed = recompute_committed_primitive_v1(&package).unwrap();
        assert_eq!(recomputed, RECOMPUTED);
        assert_eq!(package.recorded_output, RECORDED);
        assert_ne!(package.committed_input.value, RECOMPUTED);
        let PalwTirObjectiveOutcomeV1::Convicts(found) = adjudicate_original_package_v1(&claim, &package, 1 << 22).unwrap() else {
            panic!("100 versus 120 must convict");
        };
        assert_eq!(found.target_v1().claim_id, claim.claim_id);
        assert_eq!(found.disputed_leaf_v1(), claim.coordinate);
        assert!(matches!(
            found.verdict_v1().fault,
            crate::palw_step_leg::PalwStepFaultV1::ComputationMismatch { value_index: 0 }
        ));
        // The same committed input recorded as itself is still a mismatch:
        // Clamp(150, 0, 100) is 100, so a raw input comparison would have held.
        let (same_input, recorded_as_input) = package_at(INPUT);
        assert!(matches!(
            adjudicate_original_package_v1(&same_input, &recorded_as_input, 1 << 22).unwrap(),
            PalwTirObjectiveOutcomeV1::Convicts(_)
        ));
        let (honest_claim, honest) = package_at(RECOMPUTED);
        assert!(matches!(
            adjudicate_original_package_v1(&honest_claim, &honest, 1 << 22).unwrap(),
            PalwTirObjectiveOutcomeV1::PredicateHolds { .. }
        ));
    }

    #[test]
    fn tampered_opening_or_different_claim_id_is_refused() {
        let (claim, package) = package_at(RECORDED);
        let mut tampered = package.clone();
        tampered.committed_input.opening.siblings[0] = Hash64::from_u64_word(99);
        assert!(adjudicate_original_package_v1(&claim, &tampered, 1 << 22).is_err());
        let mut moved = package.clone();
        moved.identity_opening.leaf_hash = Hash64::from_u64_word(98);
        assert!(adjudicate_original_package_v1(&claim, &moved, 1 << 22).is_err());
        let mut other = claim.clone();
        other.claim_id = Hash64::from_u64_word(77);
        assert!(adjudicate_original_package_v1(&other, &package, 1 << 22).is_err());
    }

    #[test]
    fn package_over_the_admission_cap_is_refused_and_does_not_admit_the_claim() {
        let (claim, package) = package_at(RECORDED);
        let wide = court(1 << 20);
        let fits = plan_batch_v5_evidence_package_v1(&package, fitting_plan(&wide)).unwrap();
        assert!(fits.admitted);
        assert!(fits.refusal.is_none());
        let capped = court(fits.package_bytes);
        let still = plan_batch_v5_evidence_package_v1(&package, fitting_plan(&capped)).unwrap();
        assert!(still.admitted);
        assert_eq!(still.package_bytes, fits.package_bytes);
        let mut large = package.clone();
        let siblings = large.identity_opening.siblings.len();
        large.identity_opening.siblings.resize(siblings + 8, Hash64::from_u64_word(4));
        let refused = plan_batch_v5_evidence_package_v1(&large, fitting_plan(&capped)).unwrap();
        assert!(!refused.admitted);
        assert!(refused.package_bytes > fits.package_bytes);
        assert_eq!(large.identity_opening.siblings.len(), siblings + 8);
        match refused.refusal {
            Some(BatchPublicObjectionRefusalV1::Size { required, allowed }) => {
                assert_eq!(required, refused.package_bytes);
                assert!(required > allowed);
            }
            other => panic!("size refusal required, got {other:?}"),
        }
        let preflight = crate::palw_state_v2::palw_tir_public_evidence_package_preflight_v1(
            &claim,
            &large,
            fitting_plan(&capped),
            1 << 22,
        );
        assert!(preflight.is_err());
    }

    #[test]
    fn panel_quorum_is_not_consulted() {
        let (claim, package) = package_at(RECORDED);
        let court = court(1 << 20);
        let plan = fitting_plan(&court);
        let quorum = [9u8; 128];
        assert!(!quorum.is_empty());
        let from_preflight =
            crate::palw_state_v2::palw_tir_public_evidence_package_preflight_v1(&claim, &package, plan, 1 << 22).unwrap();
        let from_adjudicator = adjudicate_original_package_v1(&claim, &package, 1 << 22).unwrap();
        let PalwTirObjectiveOutcomeV1::Convicts(found) = &from_preflight else { panic!("quorumless preflight must convict") };
        let PalwTirObjectiveOutcomeV1::Convicts(again) = &from_adjudicator else { panic!("quorumless adjudicator must convict") };
        assert_eq!(found.proof_id_v1(), again.proof_id_v1());
        assert_eq!(found.verdict_v1().evidence_id, again.verdict_v1().evidence_id);
        // The quorum bytes are not passed in. Hashing them would change this id.
        let _not_an_input = quorum;
    }
}
