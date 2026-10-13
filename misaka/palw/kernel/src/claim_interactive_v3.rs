//! Explicit full-material opening proof compiler. Every round authenticates the
//! complete immutable claim and runs the actual all-target checker. This contract
//! makes no compact sumcheck or random-oracle security approval assertion.
use crate::claim_paid_v2::PaidClaimTargetV2;
use crate::hash::{Digest, object_id};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_challenge::{InteractiveModeV1, VerifiedWorkBeaconV1};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CompiledClaimProofPlanV3 {
    pub version: u16,
    pub compiler: Digest,
    pub target: PaidClaimTargetV2,
    pub mode: InteractiveModeV1,
    pub rounds: u32,
    pub maximum_material_bytes: u64,
    pub maximum_stream_blocks: u64,
}
impl CompiledClaimProofPlanV3 {
    pub fn compile(
        target: PaidClaimTargetV2,
        mode: InteractiveModeV1,
        rounds: u32,
        maximum_material_bytes: u64,
        maximum_stream_blocks: u64,
    ) -> Result<Self, String> {
        let plan = Self {
            version: 3,
            compiler: full_material_compiler_v3(),
            target,
            mode,
            rounds,
            maximum_material_bytes,
            maximum_stream_blocks,
        };
        plan.validate(target, mode, rounds, maximum_stream_blocks)?;
        Ok(plan)
    }
    pub fn id(&self) -> Digest {
        object_id(b"MISAKA/PALW/CLAIM-PROOF-PLAN/V3", self)
    }
    pub fn validate(&self, target: PaidClaimTargetV2, mode: InteractiveModeV1, rounds: u32, blocks: u64) -> Result<(), String> {
        if self.version != 3
            || self.compiler != full_material_compiler_v3()
            || self.target != target
            || self.mode != mode
            || mode == InteractiveModeV1::NonInteractive
            || self.rounds != rounds
            || rounds == 0
            || rounds > 64
            || self.maximum_material_bytes == 0
            || self.maximum_material_bytes > 64 * 1024 * 1024
            || self.maximum_stream_blocks != blocks
            || blocks == 0
        {
            return Err("unsupported/unbounded full-material interactive proof plan".into());
        }
        Ok(())
    }
    /// Canonical opening must be fixed by the producer's signed message. Each
    /// target verifier subsequently checks all commitments and exact edges.
    pub fn material_root<T: BorshSerialize>(&self, material: &T) -> Result<Digest, String> {
        let bytes = borsh::to_vec(material).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > self.maximum_material_bytes {
            return Err("proof opening exceeds signed byte capacity".into());
        }
        Ok(object_id(b"MISAKA/PALW/CLAIM-PROOF-OPENING/V3", &(self.id(), bytes)))
    }
}
pub fn full_material_compiler_v3() -> Digest {
    crate::hash::id(
        b"MISAKA/PALW/CLAIM-PROOF-COMPILER/V3",
        b"full-material-opening/all-targets/complete-scope/fresh-staged-or-transcript-bound/no-fallback",
    )
}
pub fn round_id_v3(enrollment: Digest, round: u32) -> Digest {
    object_id(b"MISAKA/PALW/CLAIM-PROOF-ROUND/V3", &(enrollment, round))
}
pub fn transcript_root_v3(
    plan: &CompiledClaimProofPlanV3,
    enrollment: Digest,
    round: u32,
    prior: Digest,
    message: Digest,
    context: Digest,
    seed: Digest,
) -> Digest {
    object_id(b"MISAKA/PALW/CLAIM-PROOF-TRANSCRIPT/V3", &(plan.id(), enrollment, round, prior, message, context, seed))
}
/// The statement and compiled plan, all prior messages/challenges, and this
/// message are absorbed. A verified source is mandatory; this is not a public
/// security approval of the transform.
pub fn fs_round_seed_v3(
    plan: &CompiledClaimProofPlanV3,
    statement: Digest,
    policy: Digest,
    source: &VerifiedWorkBeaconV1,
    round: u32,
    prior: Digest,
    message: Digest,
    prior_rounds: &[(Digest, Digest)],
) -> Result<Digest, String> {
    if plan.mode != InteractiveModeV1::TranscriptBoundFiatShamir
        || round >= plan.rounds
        || message == [0; 64]
        || (round == 0) != (prior == [0; 64])
        || prior_rounds.len() != round as usize
    {
        return Err("Fiat-Shamir round order/message mismatch".into());
    }
    let mut transcript = misaka_palw_challenge::FiatShamirTranscriptV1::new(
        object_id(b"MISAKA/PALW/CLAIM-PROOF-FS-STATEMENT/V3", &(plan.id(), statement)),
        policy,
        source,
    );
    for (message, expected) in prior_rounds {
        let actual = transcript.absorb(plan.mode, *message).map_err(|e| e.to_string())?;
        if actual != *expected {
            return Err("previous Fiat-Shamir challenge differs from full transcript recomputation".into());
        }
    }
    transcript.absorb(plan.mode, message).map_err(|e| e.to_string())
}

/// A staged source commits this exact message plus the full previous transcript.
pub fn round_subject_v3(
    initial: &misaka_palw_challenge::ChallengeSubjectV1,
    plan: &CompiledClaimProofPlanV3,
    enrollment: Digest,
    round: u32,
    prior: Digest,
    message: Digest,
) -> misaka_palw_challenge::ChallengeSubjectV1 {
    let mut s = initial.clone();
    s.subject_id = round_id_v3(enrollment, round);
    s.commitment_root =
        object_id(b"MISAKA/PALW/CLAIM-PROOF-STAGED-SUBJECT/V3", &(initial.commitment_root, plan.id(), s.subject_id, prior, message));
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_opening_plan_binds_target_mode_budget_and_canonical_message() {
        let plan =
            CompiledClaimProofPlanV3::compile(PaidClaimTargetV2::Program, InteractiveModeV1::StagedBeacon, 2, 4096, 100).unwrap();
        let message = vec![1u64, 2, 3];
        let root = plan.material_root(&message).unwrap();
        assert_ne!(root, plan.material_root(&vec![1u64, 2, 4]).unwrap());
        let mut pipeline = plan.clone();
        pipeline.target = PaidClaimTargetV2::Pipeline;
        assert_ne!(root, pipeline.material_root(&message).unwrap());
        assert!(pipeline.validate(PaidClaimTargetV2::Program, plan.mode, 2, 100).is_err());
        assert_ne!(round_id_v3([1; 64], 0), round_id_v3([1; 64], 1));
        assert_ne!(
            transcript_root_v3(&plan, [1; 64], 0, [0; 64], root, [2; 64], [3; 64]),
            transcript_root_v3(&plan, [1; 64], 0, [0; 64], root, [2; 64], [4; 64])
        );
        assert!(plan.material_root(&vec![0u64; 1024]).is_err());
        assert!(
            CompiledClaimProofPlanV3::compile(PaidClaimTargetV2::TypedSpec, InteractiveModeV1::NonInteractive, 2, 4096, 100).is_err()
        );
        assert!(
            CompiledClaimProofPlanV3::compile(PaidClaimTargetV2::Segmented, InteractiveModeV1::StagedBeacon, 65, 4096, 100).is_err()
        );
    }
}
