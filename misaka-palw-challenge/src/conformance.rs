//! **RFC-0013 §9's conformance records**: the pre-beacon commitment and the evidence of the checks its locked beacon selected.
//!
//! * [`ConformanceCommitmentV1::statement_root`] covers every bound root and policy — never its own submission provenance
//!   (`commitment_object_id`, `canonical_commitment_position`), so an object id is never hashed into itself. Changing the artifact,
//!   program, tokenizer, layout, plan, constraints, kernel, policy, implementation set or scope is a NEW commitment.
//! * [`verify_conformance_evidence_v1`] recomputes what the evidence claims: the same commitment, the same policy, the seed this
//!   node derives from its own locked beacon; then only `Passed` with every required check run and none failed or missing is a
//!   pass. `Skipped`, `Incomplete` or a stale/forged seed is never a pass.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::hash::{DOMAIN_CONFORMANCE_COMMITMENT, DOMAIN_CONFORMANCE_EVIDENCE, Digest, object_id};
use crate::subject::{ChallengeSubjectV1, RootV1, SubjectKindV1};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ConformanceCommitmentV1 {
    pub version: u16,
    pub chain_genesis: Digest,
    pub ruleset_id: Digest,
    /// `ModelConformance` or `KernelConformance`.
    pub subject_kind: SubjectKindV1,
    pub candidate_id: Digest,
    pub kernel_descriptor_id: Digest,
    pub challenge_policy_id: Digest,
    pub artifact_root: Digest,
    pub program_root: Digest,
    pub tokenizer_or_input_schema_root: RootV1,
    pub layout_root: Digest,
    pub verification_plan_root: Digest,
    pub constraint_root: RootV1,
    /// Reference, independent and backend implementations (revisions, math and layout policies), fixed before randomness.
    pub implementation_set_root: Digest,
    pub test_scope_root: Digest,
    pub calibration_id: RootV1,
    pub input_and_state_binding_root: RootV1,
    pub resource_profile_id: Digest,
    /// Provenance, outside the statement: the accepted object's id and position.
    pub commitment_object_id: Option<Digest>,
    pub canonical_commitment_position: Option<u64>,
}

/// The statement fields, in order (everything but provenance).
#[derive(BorshSerialize)]
struct StatementV1<'a> {
    version: u16,
    chain_genesis: &'a Digest,
    ruleset_id: &'a Digest,
    subject_kind: SubjectKindV1,
    candidate_id: &'a Digest,
    kernel_descriptor_id: &'a Digest,
    challenge_policy_id: &'a Digest,
    artifact_root: &'a Digest,
    program_root: &'a Digest,
    tokenizer_or_input_schema_root: &'a RootV1,
    layout_root: &'a Digest,
    verification_plan_root: &'a Digest,
    constraint_root: &'a RootV1,
    implementation_set_root: &'a Digest,
    test_scope_root: &'a Digest,
    calibration_id: &'a RootV1,
    input_and_state_binding_root: &'a RootV1,
    resource_profile_id: &'a Digest,
}

impl ConformanceCommitmentV1 {
    /// The pre-beacon statement digest (`commitment_root`).
    pub fn statement_root(&self) -> Digest {
        object_id(
            DOMAIN_CONFORMANCE_COMMITMENT,
            &StatementV1 {
                version: self.version,
                chain_genesis: &self.chain_genesis,
                ruleset_id: &self.ruleset_id,
                subject_kind: self.subject_kind,
                candidate_id: &self.candidate_id,
                kernel_descriptor_id: &self.kernel_descriptor_id,
                challenge_policy_id: &self.challenge_policy_id,
                artifact_root: &self.artifact_root,
                program_root: &self.program_root,
                tokenizer_or_input_schema_root: &self.tokenizer_or_input_schema_root,
                layout_root: &self.layout_root,
                verification_plan_root: &self.verification_plan_root,
                constraint_root: &self.constraint_root,
                implementation_set_root: &self.implementation_set_root,
                test_scope_root: &self.test_scope_root,
                calibration_id: &self.calibration_id,
                input_and_state_binding_root: &self.input_and_state_binding_root,
                resource_profile_id: &self.resource_profile_id,
            },
        )
    }

    pub fn well_formed(&self) -> Result<(), &'static str> {
        if self.version != 1 {
            return Err("unknown conformance commitment version");
        }
        if !matches!(self.subject_kind, SubjectKindV1::ModelConformance | SubjectKindV1::KernelConformance) {
            return Err("a conformance commitment is for model or kernel conformance");
        }
        Ok(())
    }

    /// The challenge subject this commitment fixes.
    pub fn subject(&self) -> ChallengeSubjectV1 {
        ChallengeSubjectV1 {
            chain_genesis: self.chain_genesis,
            ruleset_id: self.ruleset_id,
            challenge_policy_id: self.challenge_policy_id,
            subject_kind: self.subject_kind,
            subject_id: self.candidate_id,
            kernel_id: RootV1::Present(self.kernel_descriptor_id),
            verification_plan_root: RootV1::Present(self.verification_plan_root),
            program_root: RootV1::Present(self.program_root),
            artifact_root: RootV1::Present(self.artifact_root),
            tokenizer_or_schema_root: self.tokenizer_or_input_schema_root,
            layout_root: RootV1::Present(self.layout_root),
            input_root: self.input_and_state_binding_root,
            state_root: RootV1::Absent,
            constraint_root: self.constraint_root,
            commitment_root: self.statement_root(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum ConformanceStatusV1 {
    Passed = 0,
    Failed = 1,
    /// Never a pass.
    Skipped = 2,
    Incomplete = 3,
    BeaconUnavailable = 4,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BeaconConformanceEvidenceV1 {
    pub version: u16,
    pub commitment_root: Digest,
    pub challenge_policy_id: Digest,
    pub challenge_anchor: Digest,
    /// Root over the beacon's source references (their eligibility/Final/settlement facts are recomputed, never trusted).
    pub qualifying_source_evidence_root: Digest,
    pub lock_position: u64,
    pub beacon_output: Digest,
    pub challenge_seed: Digest,
    pub selected_vectors_root: Digest,
    pub selected_tensor_ranges_root: Digest,
    pub reference_result_root: Digest,
    pub independent_result_root: Digest,
    pub backend_result_root: Digest,
    pub authenticated_openings_root: Digest,
    pub transcript_root: RootV1,
    pub checks_required: u64,
    pub checks_run: u64,
    pub checks_failed: u64,
    pub missing_checks: Vec<String>,
    pub failures: Vec<String>,
    pub scope_and_fault_model_id: Digest,
    /// The declared conditional error bound, as `-log2 ε` (derived for the committed scope; a claim to check, not a theorem).
    pub derived_epsilon_bits: u16,
    pub status: ConformanceStatusV1,
    pub public_material_locator_root: Digest,
}

impl BeaconConformanceEvidenceV1 {
    pub fn id(&self) -> Digest {
        object_id(DOMAIN_CONFORMANCE_EVIDENCE, self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ConformanceRefusalV1 {
    #[error("unknown evidence version")]
    Version,
    #[error("the evidence names another commitment")]
    Commitment,
    #[error("the evidence names another challenge policy")]
    Policy,
    #[error("the beacon output/anchor/lock is not the one derived from canonical history")]
    Beacon,
    #[error("the challenge seed is not the recomputed seed")]
    Seed,
    #[error("status {0:?} is not a pass")]
    NotPassed(ConformanceStatusV1),
    #[error("{run} of {required} required checks ran, {failed} failed, {missing} missing")]
    Incomplete { required: u64, run: u64, failed: u64, missing: usize },
    #[error("the evidence's epsilon is weaker than the policy's security bits")]
    WeakEpsilon,
}

/// **Recompute and judge conformance evidence**: `derived_seed` and `beacon` are what THIS node derived from canonical history
/// for `commitment` (see [`crate::seed::challenge_seed_v1`]); the evidence's cached values are claims to compare.
pub fn verify_conformance_evidence_v1(
    commitment: &ConformanceCommitmentV1,
    beacon: &crate::beacon::WorkBeaconV1,
    derived_seed: &Digest,
    required_security_bits: u16,
    ev: &BeaconConformanceEvidenceV1,
) -> Result<(), ConformanceRefusalV1> {
    use ConformanceRefusalV1 as R;
    if ev.version != 1 {
        return Err(R::Version);
    }
    if ev.commitment_root != commitment.statement_root() {
        return Err(R::Commitment);
    }
    if ev.challenge_policy_id != commitment.challenge_policy_id {
        return Err(R::Policy);
    }
    if ev.beacon_output != beacon.output || ev.challenge_anchor != beacon.challenge_anchor || ev.lock_position != beacon.lock_position
    {
        return Err(R::Beacon);
    }
    if ev.challenge_seed != *derived_seed {
        return Err(R::Seed);
    }
    if ev.status != ConformanceStatusV1::Passed {
        return Err(R::NotPassed(ev.status));
    }
    if ev.checks_required == 0 || ev.checks_run != ev.checks_required || ev.checks_failed != 0 || !ev.missing_checks.is_empty() {
        return Err(R::Incomplete {
            required: ev.checks_required,
            run: ev.checks_run,
            failed: ev.checks_failed,
            missing: ev.missing_checks.len(),
        });
    }
    if ev.derived_epsilon_bits < required_security_bits {
        return Err(R::WeakEpsilon);
    }
    Ok(())
}
