//! **A claim's challenge subject** (RFC-0007 Part VI): what `CLAIM_VERIFICATION` is about, derived from ledger state alone.
//!
//! The claim's check randomness is never a number the ledger stores: it is `challenge_seed_v1(ctx, subject, work_beacon)` from the
//! single challenge contract, drawn only after the subject below is committed and a future PALW Work Beacon is locked. The subject
//! binds the kernel, plan, program, artifact commitments, the job's input, the state boundary roots and, as its commitment root, the
//! claim's evidence root (everything the producer fixed before any challenge existed). A root that does not apply is typed
//! [`RootV1::Absent`] — never a zero wildcard.
//!
//! **Outsiders do not depend on any of this.** A public prosecutor checks with its own salt and files an exact, authenticated proof;
//! a probabilistic check failing never convicts, and the beacon authorizes nothing.
//!
//! RFC-0015: a claim of an `OptimisticPublicVerification` class has no Panel checking and so draws nothing from this subject; the
//! function still describes it (it is a function of the committed claim) and a consumer must not start a Panel check for it.

use misaka_palw_challenge::{ChallengeSubjectV1, RootV1, SubjectKindV1};

use crate::hash::{Digest, object_id};
use crate::ledger::{ClaimBodyV1, KernelLedgerV1};
use crate::public::program_root_v1;

pub const CLAIM_SUBJECT_COMMITMENT_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/claim-subject-commitment/v1";
pub const CLAIM_SUBJECT_STATE_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/claim-subject-state/v1";
pub const CLAIM_SUBJECT_INPUT_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/claim-subject-input/v1";
pub const CLAIM_SUBJECT_ARTIFACTS_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/claim-subject-artifacts/v1";

/// The subject's `commitment_root`: the claim id (job, producer, delivered ids) together with its evidence root. Two claims of
/// different jobs or producers that happen to commit byte-identical evidence still have different commitment roots, so a locked
/// beacon is bound to one claim and no beacon context is shared between claims.
pub fn claim_commitment_root_v1(claim_id: &Digest, evidence_root: &Digest) -> Digest {
    object_id(CLAIM_SUBJECT_COMMITMENT_DOMAIN_V1, &(*claim_id, *evidence_root))
}

impl KernelLedgerV1 {
    /// **The `CLAIM_VERIFICATION` subject of a committed claim** (`None` for an unknown claim).
    ///
    /// * single program: kernel = the descriptor digest, plan = the plan root, program = the program root, artifact = the commitments'
    ///   root, input = the job-input root, state = `H(initial ‖ final state root)`, commitment = [`claim_commitment_root_v1`];
    /// * pipeline: program = the pipeline root (the pipeline and every program), artifact = `H(each program's artifact root)`,
    ///   input = `H(job root ‖ R's binding)`, state absent (each stage's boundary roots are inside its evidence), commitment =
    ///   [`claim_commitment_root_v1`] over the pipeline evidence root;
    /// * tokenizer/input schema, layout and the separate constraint root are absent: the class binds none, and the plan root is the
    ///   constraint set.
    pub fn claim_challenge_subject(&self, claim: &Digest) -> Option<ChallengeSubjectV1> {
        let row = self.claims.get(claim)?;
        let p = &self.policy;
        let base =
            |kernel: Digest, plan: Digest, program: Digest, artifact: Digest, input: Digest, state: RootV1, commitment: Digest| {
                ChallengeSubjectV1 {
                    chain_genesis: p.network_domain,
                    ruleset_id: p.ruleset_digest,
                    challenge_policy_id: p.challenge_policy_id,
                    subject_kind: SubjectKindV1::ClaimVerification,
                    subject_id: *claim,
                    kernel_id: RootV1::Present(kernel),
                    verification_plan_root: RootV1::Present(plan),
                    program_root: RootV1::Present(program),
                    artifact_root: RootV1::Present(artifact),
                    tokenizer_or_schema_root: RootV1::Absent,
                    layout_root: RootV1::Absent,
                    input_root: RootV1::Present(input),
                    state_root: state,
                    constraint_root: RootV1::Absent,
                    commitment_root: commitment,
                }
            };
        match &row.body {
            ClaimBodyV1::Program { evidence, .. } => {
                let class = self.classes.get(&row.class_binding_id)?;
                let state = object_id(CLAIM_SUBJECT_STATE_DOMAIN_V1, &(evidence.initial_state_root, evidence.final_state_root));
                Some(base(
                    class.descriptor.digest(),
                    class.plan.root(),
                    program_root_v1(&class.program_bytes),
                    class.param_commitments.root(),
                    evidence.job_input_root,
                    RootV1::Present(state),
                    claim_commitment_root_v1(claim, &evidence.root()),
                ))
            }
            ClaimBodyV1::Pipeline { evidence, .. } => {
                let class = self.pipeline_classes.get(&row.class_binding_id)?;
                let artifacts = object_id(CLAIM_SUBJECT_ARTIFACTS_DOMAIN_V1, &class.binding.artifact_roots);
                let input = object_id(CLAIM_SUBJECT_INPUT_DOMAIN_V1, &(evidence.job_root, evidence.random_binding));
                Some(base(
                    class.descriptor.digest(),
                    class.plan.root(),
                    class.binding.pipeline_root,
                    artifacts,
                    input,
                    RootV1::Absent,
                    claim_commitment_root_v1(claim, &evidence.root()),
                ))
            }
            // RFC-0004 Part II: kernel = K2-TR-v1, plan = the class id (the specification is the constraint set), input = the job.
            ClaimBodyV1::Spec(b) => {
                let class = self.typed.classes.get(&row.class_binding_id)?;
                let (program, artifact, state) = crate::spec::subject_roots_v1(class, b);
                Some(base(
                    crate::spec::k2_tr_v1_descriptor().digest(),
                    row.class_binding_id,
                    program,
                    artifact,
                    row.job_id,
                    state.map_or(RootV1::Absent, |s| RootV1::Present(object_id(CLAIM_SUBJECT_STATE_DOMAIN_V1, &s))),
                    claim_commitment_root_v1(claim, &b.execution_root()),
                ))
            }
        }
    }
}
