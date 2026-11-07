//! Panel-owned, bounded submission preparation over the shared private Court API.
//! A relay keeps the original author's signature. Only the Node's authenticated
//! cursor determines what to send after restart; these objects grant no credit.
use super::{PanelBuildStageV2, PositivePursuitStepV2, plan_positive_pursuit_v2};
use crate::bundle::DetachedSigner;
use kaspa_consensus_core::{
    Hash64,
    mldsa87_primitives::{MLDSA87_PUBKEY_LEN, mldsa87_key_id},
    palw_positive_court_v2::{
        AdmissionScopeV2, ErrorV2, ExactCaseV2, ExactPartV2, ExactPolicyV2, MinerResultV2, PanelWitnessV2, SignatureV2,
        WITNESS_CONTEXT,
        coordinator::{CandidateEffectV2, EncodedPartV2, PositiveCoordinatorV2},
    },
};

/// Local transport limits, constrained again by the receiving coordinator.
#[derive(Clone, Copy, Debug)]
pub struct PanelSendLimitsV2 {
    pub max_parts: u64,
    pub max_bytes: u64,
}

/// A candidate authenticated against the original author's resolved key. This
/// is not a persisted completion/progress record or a Node acceptance decision.
pub struct PanelCandidateV2 {
    result: MinerResultV2,
    scope: AdmissionScopeV2,
    policy: ExactPolicyV2,
    witness: PanelWitnessV2,
    signature: SignatureV2,
    author: Hash64,
}
impl PanelCandidateV2 {
    /// Signing is delegated to the identity holder, using the common domain.
    /// A caller that persists this signature can relay it without re-signing.
    pub fn sign(
        result: &MinerResultV2,
        scope: AdmissionScopeV2,
        policy: ExactPolicyV2,
        witness: PanelWitnessV2,
        signer: &dyn DetachedSigner,
    ) -> Result<Self, String> {
        Self::check_binding(result, scope, policy, &witness).map_err(|e| format!("Panel candidate: {e:?}"))?;
        let signature = SignatureV2(signer.sign_with_context(witness.id().as_byte_slice(), WITNESS_CONTEXT)?);
        Self::from_signed(result, scope, policy, witness, signature, &signer.public_key())
            .map_err(|e| format!("Panel signature: {e:?}"))
    }

    /// Reauthenticate an existing candidate for takeover. `author_key` comes
    /// from role admission, not from an untrusted capture or relay identity.
    pub fn from_signed(
        result: &MinerResultV2,
        scope: AdmissionScopeV2,
        policy: ExactPolicyV2,
        witness: PanelWitnessV2,
        signature: SignatureV2,
        author_key: &[u8],
    ) -> Result<Self, ErrorV2> {
        Self::check_binding(result, scope, policy, &witness)?;
        if author_key.len() != MLDSA87_PUBKEY_LEN || !signature.verifies(author_key, &witness.id(), WITNESS_CONTEXT) {
            return Err(ErrorV2::Authentication);
        }
        Ok(Self { result: result.clone(), scope, policy, witness, signature, author: mldsa87_key_id(author_key) })
    }

    fn check_binding(
        result: &MinerResultV2,
        scope: AdmissionScopeV2,
        policy: ExactPolicyV2,
        witness: &PanelWitnessV2,
    ) -> Result<(), ErrorV2> {
        if scope.check_result(result).is_err()
            || result.version != 2
            || witness.version != 2
            || witness.result != result.id()
            || witness.policy != policy.id()
            || witness.binding.job_context != result.job
            || witness.binding.artifact_root != result.artifact
            || witness.binding.class.class_id(&result.artifact) != scope.registered_class_id
            || witness.generated.len() != result.job.exact_decode_tokens as usize
        {
            return Err(ErrorV2::Binding);
        }
        if !result.sampling.is_greedy() {
            return Err(ErrorV2::UnsupportedProfile);
        }
        if witness.binding.step_leaf_count == 0
            || witness.binding.step_leaf_count > policy.max_leaves
            || policy.max_part_bytes == 0
            || policy.max_total_bytes == 0
            || borsh::object_length(witness).map_err(|_| ErrorV2::Wire)? as u64 > policy.max_commit_bytes
            || borsh::object_length(result).map_err(|_| ErrorV2::Wire)? as u64 > policy.max_commit_bytes
        {
            return Err(ErrorV2::Budget);
        }
        Ok(())
    }
    pub fn witness(&self) -> &PanelWitnessV2 {
        &self.witness
    }
    pub fn signature(&self) -> &SignatureV2 {
        &self.signature
    }
    pub fn author(&self) -> Hash64 {
        self.author
    }

    pub fn open(&self, node: &mut PositiveCoordinatorV2, now: u64) -> Result<CandidateEffectV2, ErrorV2> {
        if node.result() != &self.result {
            return Err(ErrorV2::Binding);
        }
        node.open_candidate(self.author, self.witness.clone(), &self.signature, now)
    }

    /// Deadline is pinned to the authenticated policy, never a local resume file.
    pub fn next_step(&self, node: Option<&ExactCaseV2>, current_branch: Hash64, max_parts: u64, now: u64) -> PositivePursuitStepV2 {
        // A rejected predecessor does not reject a newly signed replacement.
        // Aggregate attempt/budget admission remains the coordinator's job.
        if let Some(previous) = node {
            use kaspa_consensus_core::palw_positive_court_v2::{ClaimStatusV2, WitnessStatusV2};
            if previous.statuses() == (ClaimStatusV2::Checking, WitnessStatusV2::Rejected)
                && previous.witness().id() != self.witness.id()
                && previous.result() == &self.result
                && previous.witness().policy == self.policy.id()
                && previous.witness().attempt.checked_add(1) == Some(self.witness.attempt)
                && current_branch == self.scope.branch
                && now < self.policy.deadline
                && max_parts > 0
            {
                return PositivePursuitStepV2::CommitWitness { witness: self.witness.id() };
            }
        }
        plan_positive_pursuit_v2(
            &self.scope,
            &self.result,
            current_branch,
            PanelBuildStageV2::WitnessSigned { witness: self.witness.id(), parts: self.witness.binding.step_leaf_count },
            node,
            max_parts,
            now,
            self.policy.deadline,
        )
    }

    /// Allocate only a bounded batch. Use `stream_parts_range` to feed its exact
    /// canonical range, then submit it to the same G14 checker via `submit`.
    pub fn batch(&self, first: u64, count: u64, limits: PanelSendLimitsV2) -> Result<PanelBatchBuilderV2, ErrorV2> {
        if count == 0
            || count > limits.max_parts
            || limits.max_bytes == 0
            || first.checked_add(count).filter(|n| *n <= self.witness.binding.step_leaf_count).is_none()
        {
            return Err(ErrorV2::Budget);
        }
        // At least one byte per part also bounds descriptor allocations even
        // when hostile local callers choose an enormous part-count limit.
        if count > limits.max_bytes / std::mem::size_of::<OwnedPartV2>() as u64 {
            return Err(ErrorV2::Budget);
        }
        Ok(PanelBatchBuilderV2 {
            result: self.result.id(),
            witness: self.witness.id(),
            first,
            count,
            part_cap: self.policy.max_part_bytes,
            byte_cap: limits.max_bytes.min(self.policy.max_total_bytes),
            bytes: 0,
            parts: Vec::new(),
        })
    }
}

struct OwnedPartV2 {
    cone: Vec<u8>,
    trace: Option<Vec<u8>>,
}
pub struct PanelBatchBuilderV2 {
    result: Hash64,
    witness: Hash64,
    first: u64,
    count: u64,
    part_cap: u64,
    byte_cap: u64,
    bytes: u64,
    parts: Vec<OwnedPartV2>,
}
impl PanelBatchBuilderV2 {
    pub fn push(&mut self, index: u64, part: &ExactPartV2) -> Result<(), ErrorV2> {
        if self.parts.len() as u64 >= self.count || index != self.first + self.parts.len() as u64 {
            return Err(ErrorV2::Order);
        }
        // object_length precedes serialization/allocation. Failed pushes leave
        // the builder unchanged, and cannot silently omit the expected part.
        let cone_len = borsh::object_length(&part.cone).map_err(|_| ErrorV2::Wire)? as u64;
        let trace_len = part.trace.as_ref().map(borsh::object_length).transpose().map_err(|_| ErrorV2::Wire)?.unwrap_or(0) as u64;
        let size = cone_len.checked_add(trace_len).ok_or(ErrorV2::Budget)?;
        let total = self.bytes.checked_add(size).ok_or(ErrorV2::Budget)?;
        if size > self.part_cap || total > self.byte_cap {
            return Err(ErrorV2::Budget);
        }
        let cone = borsh::to_vec(&part.cone).map_err(|_| ErrorV2::Wire)?;
        let trace = part.trace.as_ref().map(borsh::to_vec).transpose().map_err(|_| ErrorV2::Wire)?;
        self.parts.push(OwnedPartV2 { cone, trace });
        self.bytes = total;
        Ok(())
    }
    pub fn finish(self) -> Result<PanelBatchV2, ErrorV2> {
        if self.parts.len() as u64 != self.count {
            return Err(ErrorV2::Incomplete);
        }
        Ok(PanelBatchV2(self))
    }
}
pub struct PanelBatchV2(PanelBatchBuilderV2);
impl PanelBatchV2 {
    pub fn bytes(&self) -> u64 {
        self.0.bytes
    }
    pub fn submit(&self, node: &mut PositiveCoordinatorV2, now: u64) -> Result<u64, ErrorV2> {
        if node.result().id() != self.0.result {
            return Err(ErrorV2::Binding);
        }
        let parts: Vec<_> = self.0.parts.iter().map(|p| EncodedPartV2 { cone: &p.cone, trace: p.trace.as_deref() }).collect();
        node.submit_batch(self.0.witness, self.0.first, &parts, now)
    }
}
