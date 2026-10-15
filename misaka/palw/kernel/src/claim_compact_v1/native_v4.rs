//! Native V4 contract enrollment and two phase compact proofs. The old V3
//! message fixes the opening before future entropy; this new binding fixes the
//! seed-dependent proof only after the source lock, without a hash cycle.
use super::native_v2::*;
use super::*;
use crate::claim_interactive_v3::CompiledClaimProofPlanV3;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CompactClaimRegistrationV4 {
    pub version: u16,
    pub registrant: Digest,
    pub contract: NativeCompactContractV2,
    pub coverage: NativeCompactCoverageV2,
    pub plan: CompiledClaimProofPlanV3,
    pub adversary: CompactAdversaryBudgetV4,
}
impl CompactClaimRegistrationV4 {
    pub fn id(&self) -> Digest {
        self.plan.id()
    }
    pub fn validate(&self, ledger: &crate::ledger::KernelLedgerV1) -> Result<NativeCompactContractSummaryV2, String> {
        if self.version != 4
            || self.registrant == [0; 64]
            || self.plan.version != 4
            || self.plan.compiler != compact_suite_id_v4(&self.contract, &self.coverage, self.adversary)
            || self.plan.target != self.coverage.target
        {
            return Err("compact enrollment substitutes version/registrant/plan/whole-class suite".into());
        }
        self.plan.validate(self.coverage.target, self.plan.mode, self.plan.rounds, self.plan.maximum_stream_blocks)?;
        if native_proof_bytes_bound_v4(&self.coverage)? > self.contract.maximum_proof_bytes {
            return Err("compact proof location/framing exceeds whole-native byte contract".into());
        }
        let mut bound = self.contract.check(ledger, &self.coverage, &self.adversary.budget())?;
        if self.coverage.target == PaidClaimTargetV2::Segmented {
            // Existing exact roots/decode replay, the additional streaming
            // proof-operand replay, and at most one authenticated prior-position
            // read/hash per position are separate charged contributions.
            bound.verifier_work = bound
                .verifier_work
                .checked_add(self.coverage.exact_verifier_work_upper)
                .and_then(|n| n.checked_add(self.coverage.exact_verifier_work_upper))
                .ok_or("segmented compact verifier work overflow")?;
            if bound.verifier_work > self.contract.maximum_verifier_work {
                return Err("segmented replay/prior authentication exceeds signed verifier work".into());
            }
        }
        Ok(bound)
    }
}
/// Stage and scoped seed prevent cross-stage/step/scope replay even when the
/// same registered TIR plan is reused by several native components.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub struct CompactInstanceV4 {
    pub stage: u32,
    pub scoped_seed: Digest,
    pub plan_root: Digest,
    pub position: u32,
    pub occurrence: u16,
    pub relation: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct LocatedCompactProofV4 {
    pub instance: CompactInstanceV4,
    pub proof: CompactRelationProofV1,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeCompactProofV4 {
    pub version: u16,
    pub plan: Digest,
    pub seed: Digest,
    pub proofs: Vec<LocatedCompactProofV4>,
}
impl NativeCompactProofV4 {
    pub fn root(&self) -> Digest {
        object_id(b"MISAKA/PALW/NATIVE-COMPACT-PROOF/V4", self)
    }
    pub fn validate_shape(&self, plan: Digest, seed: Digest, maximum_bytes: u64) -> Result<(), String> {
        if self.version != 4
            || self.plan != plan
            || self.seed != seed
            || self.proofs.windows(2).any(|p| p[0].instance >= p[1].instance)
            || self.proofs.iter().any(|p| {
                p.instance.relation != p.proof.relation
                    || p.instance.position != p.proof.position
                    || p.instance.occurrence != p.proof.occurrence
            })
            || borsh::to_vec(self).map_err(|e| e.to_string())?.len() as u64 > maximum_bytes
        {
            return Err("compact proof version/plan/seed/order/encoding exceeds signed contract".into());
        }
        Ok(())
    }
}
pub fn compact_transcript_root_v4(base: Digest, proof: Digest) -> Digest {
    object_id(b"MISAKA/PALW/CLAIM-PROOF-BOUND-TRANSCRIPT/V4", &(base, proof))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CompactAdversaryBudgetV4 {
    pub retry_limit: u32,
    pub grinding_bits_per_beacon: u32,
    pub beacons_per_attempt: u32,
    pub adaptive_statements: u128,
}
impl CompactAdversaryBudgetV4 {
    pub fn budget(&self) -> AdversaryBudgetV1 {
        AdversaryBudgetV1 {
            retry_limit: self.retry_limit,
            grinding_bits_per_beacon: self.grinding_bits_per_beacon,
            beacons_per_attempt: self.beacons_per_attempt,
            adaptive_statements: self.adaptive_statements,
        }
    }
}

/// Invoked only after the ordinary verifier has authenticated operands/output
/// and their types. Returning false preserves that relation's exact checker.
pub trait CompactRelationConsumerV4 {
    fn applies(&self, stage: u32, plan: Digest, block: u8, node: u16) -> bool;
    fn complete(&self) -> Result<(), String>;
    fn relation(
        &self,
        stage: u32,
        seed: Digest,
        plan: Digest,
        position: u32,
        occurrence: u16,
        block: u8,
        node: u16,
        inputs: &[Tensor],
        output: &Tensor,
    ) -> Result<bool, String>;
}
/// One bounded complete verifier invocation. The same mutable word budget and
/// proof inventory are shared across all scopes, native stages and memory steps.
pub struct CompactVerificationSessionV4 {
    coverage: NativeCompactCoverageV2,
    plan: Digest,
    seed: Digest,
    maximum_bytes: u64,
    state: std::cell::RefCell<CompactSessionStateV4>,
}
struct CompactSessionStateV4 {
    budget: CompactStreamBudgetV1,
    proofs: std::collections::BTreeMap<CompactInstanceV4, CompactRelationProofV1>,
    visited: std::collections::BTreeSet<CompactInstanceV4>,
    proving: bool,
}
impl CompactVerificationSessionV4 {
    pub fn prover(
        registration: &CompactClaimRegistrationV4,
        seed: Digest,
        maximum_words: u64,
        maximum_bytes: u64,
    ) -> Result<Self, String> {
        Self::new(registration, seed, maximum_words, maximum_bytes, None)
    }
    pub fn verifier(
        registration: &CompactClaimRegistrationV4,
        seed: Digest,
        maximum_words: u64,
        maximum_bytes: u64,
        proof: &NativeCompactProofV4,
    ) -> Result<Self, String> {
        proof.validate_shape(registration.plan.id(), seed, maximum_bytes)?;
        Self::new(registration, seed, maximum_words, maximum_bytes, Some(proof))
    }
    fn new(
        registration: &CompactClaimRegistrationV4,
        seed: Digest,
        maximum_words: u64,
        maximum_bytes: u64,
        proof: Option<&NativeCompactProofV4>,
    ) -> Result<Self, String> {
        if maximum_words == 0 || maximum_bytes == 0 {
            return Err("compact session needs finite shared bounds".into());
        }
        let mut proofs = std::collections::BTreeMap::new();
        if let Some(proof) = proof {
            for p in &proof.proofs {
                if proofs.insert(p.instance.clone(), p.proof.clone()).is_some() {
                    return Err("duplicate compact proof instance".into());
                }
            }
        }
        Ok(Self {
            coverage: registration.coverage.clone(),
            plan: registration.plan.id(),
            seed,
            maximum_bytes,
            state: std::cell::RefCell::new(CompactSessionStateV4 {
                budget: CompactStreamBudgetV1 { remaining_words: maximum_words },
                proofs,
                visited: std::collections::BTreeSet::new(),
                proving: proof.is_none(),
            }),
        })
    }
    /// Called only after every actual native exact boundary and relation passes.
    /// Surplus, missing and replayed proofs never form a complete receipt.
    pub(crate) fn matches(&self, registration: &CompactClaimRegistrationV4, seed: Digest) -> bool {
        self.plan == registration.plan.id() && self.coverage == registration.coverage && self.seed == seed
    }
    pub fn finish(&self) -> Result<NativeCompactProofV4, String> {
        let state = self.state.borrow();
        if state.proofs.len() != state.visited.len() || state.proofs.keys().any(|k| !state.visited.contains(k)) {
            return Err("compact proof has unused or absent native relation instances".into());
        }
        let proof = NativeCompactProofV4 {
            version: 4,
            plan: self.plan,
            seed: self.seed,
            proofs: state
                .proofs
                .iter()
                .map(|(instance, proof)| LocatedCompactProofV4 { instance: instance.clone(), proof: proof.clone() })
                .collect(),
        };
        proof.validate_shape(self.plan, self.seed, self.maximum_bytes)?;
        Ok(proof)
    }
}
impl CompactRelationConsumerV4 for CompactVerificationSessionV4 {
    fn applies(&self, stage: u32, plan: Digest, block: u8, node: u16) -> bool {
        let memory = self.coverage.exact_boundaries.iter().any(|b| matches!(b, NativeExactBoundaryV2::Memory { .. }));
        self.coverage.parts.iter().any(|p| {
            p.coverage.plan_root == plan
                && (if memory { stage < p.maximum_executions } else { u32::from(p.stage) == stage })
                && p.coverage.compact().any(|r| r.block == block && r.node == node)
        })
    }
    fn complete(&self) -> Result<(), String> {
        if self.state.borrow().proving {
            return Err("a producer session cannot issue a verified receipt".into());
        }
        self.finish().map(|_| ())
    }
    fn relation(
        &self,
        stage: u32,
        seed: Digest,
        plan: Digest,
        position: u32,
        occurrence: u16,
        block: u8,
        node: u16,
        inputs: &[Tensor],
        output: &Tensor,
    ) -> Result<bool, String> {
        let memory = self.coverage.exact_boundaries.iter().any(|b| matches!(b, NativeExactBoundaryV2::Memory { .. }));
        let part = self
            .coverage
            .parts
            .iter()
            .find(|p| p.coverage.plan_root == plan && if memory { stage < p.maximum_executions } else { u32::from(p.stage) == stage })
            .ok_or("compact verifier encountered an unregistered native stage/step/plan")?;
        if position >= part.coverage.max_positions {
            return Err("compact position exceeds actual registered plan".into());
        }
        let Some(compiled) = part.coverage.compact().find(|p| p.block == block && p.node == node) else { return Ok(false) };
        let instance =
            CompactInstanceV4 { stage, scoped_seed: seed, plan_root: plan, position, occurrence, relation: compiled.relation };
        let mut state = self.state.borrow_mut();
        if !state.visited.insert(instance.clone()) {
            return Err("compact native instance visited twice".into());
        }
        let proof = if state.proving {
            let proof = prove_relation_v1(compiled, &seed, position, occurrence, inputs, output, &mut state.budget)
                .map_err(|e| e.to_string())?;
            state.proofs.insert(instance.clone(), proof.clone());
            proof
        } else {
            state.proofs.get(&instance).cloned().ok_or("compact proof missing actual authenticated relation")?
        };
        verify_relation_v1(compiled, &seed, inputs, output, &proof, &mut state.budget).map_err(|e| e.to_string())?;
        Ok(true)
    }
}

impl CompactClaimRegistrationV4 {
    pub fn validate_for_class(
        &self,
        class: &crate::claim_paid_v2::PaidClaimClassV2,
        ledger: &crate::ledger::KernelLedgerV1,
    ) -> Result<NativeCompactContractSummaryV2, String> {
        let bound = self.validate(ledger)?;
        if class.interactive_plan.as_ref() != Some(&self.plan)
            || class.base_class != self.contract.base_class
            || class.target != self.coverage.target
            || class.registrant != self.registrant
            || self.adversary.retry_limit < class.source_policy.retry_limit
            || self.adversary.beacons_per_attempt < self.plan.rounds
            || self.adversary.adaptive_statements != 1
            || bound.effective_bits < i64::from(class.source_policy.security_bits)
        {
            return Err("compact class source/retry/round/binding/security scope differs from registered contract".into());
        }
        Ok(bound)
    }
}

/// Segmented v4/v5 retain the existing exact streaming replay as the boundary
/// authenticator. Compact relations are then checked against the same replayed
/// values; this does not claim a sublinear material or execution improvement.
pub(crate) fn check_segmented_compact_v4(
    c: &crate::element::SegClaimContextV1<'_>,
    plan: Digest,
    material: &dyn crate::element::SegMaterialV1,
    artifact: &dyn Fn(u16, Option<u16>) -> Option<Tensor>,
    tokens: &[u32],
    randomness: &crate::challenge::CheckerRandomnessV2,
) -> Result<(), String> {
    let Some(consumer) = &randomness.compact else { return Ok(()) };
    let wiring = crate::trace::WiringV1::for_stage(c.program, None).map_err(|e| e.to_string())?;
    let modes = crate::seg_scope::seg_value_modes_v1(c.program);
    let job_inputs = if let Some(encoder) = c.encoder {
        let (ids, count) = encoder.inputs(tokens)?;
        Some((encoder.first_input, ids, count))
    } else {
        None
    };
    crate::seg_detect::replay_values_v1(c, artifact, tokens, None, &mut |position, values| {
        let mut prior_cache: Option<Vec<Vec<Tensor>>> = None;
        let mut resolve = |source: crate::trace::SourceV1| -> Result<Tensor, String> {
            use crate::trace::SourceV1;
            match source {
                SourceV1::Node { position: p, occurrence, node } => {
                    if p == position {
                        return values
                            .get(occurrence as usize)
                            .and_then(|o| o.get(node as usize))
                            .cloned()
                            .ok_or("compact replay node missing".into());
                    }
                    if p.checked_add(1) != Some(position) {
                        return Err("compact replay encountered nonlocal state dependency".into());
                    }
                    if prior_cache.is_none() {
                        let prior = material.position(p).ok_or("compact segmented prior position unavailable")?;
                        let root = crate::seg::position_root_of_v1(p, &crate::seg_scope::seg_position_commitments_v1(&modes, &prior));
                        let siblings = material.position_siblings(p).ok_or("compact prior authentication path unavailable")?;
                        if !crate::seg::position_in_segment(p, &root, &siblings, c.segment_roots, c.positions) {
                            return Err("compact segmented prior values are not committed".into());
                        }
                        prior_cache = Some(prior);
                    }
                    prior_cache
                        .as_ref()
                        .unwrap()
                        .get(occurrence as usize)
                        .and_then(|o| o.get(node as usize))
                        .cloned()
                        .ok_or("compact prior node missing".into())
                }
                SourceV1::Param { index, layer } => {
                    if let Some((first, ids, count)) = &job_inputs {
                        if index == *first {
                            return Ok(ids.clone());
                        }
                        if index == *first + 1 {
                            return Ok(count.clone());
                        }
                    }
                    let value = artifact(index, layer).ok_or("compact segmented artifact unavailable")?;
                    if c.params.by_instance.get(&(index, layer)) != Some(&crate::merkle3::tensor_commitment_v3(&value)) {
                        return Err("compact segmented parameter commitment mismatch".into());
                    }
                    Ok(value)
                }
                SourceV1::Const(index) => crate::trace::const_tensor(c.program, index).map_err(|e| e.to_string()),
                SourceV1::Zeros { dtype, shape } => Ok(Tensor::zeros(dtype, &shape)),
                SourceV1::Public(value) => Tensor::scalar(misaka_palw_tir::DType::Idx, i128::from(value)).map_err(|e| e.to_string()),
                SourceV1::Input { .. } => Err("segmented compact contract encountered pipeline input".into()),
            }
        };
        for (occurrence, (block, _)) in wiring.occurrences.iter().enumerate() {
            for (node, definition) in c.program.blocks[*block as usize].nodes.iter().enumerate() {
                if !consumer.applies(0, plan, *block, node as u16) {
                    continue;
                }
                let mut inputs = Vec::with_capacity(definition.inputs.len());
                for index in 0..definition.inputs.len() {
                    inputs.push(
                        resolve(
                            wiring
                                .input_source(tokens, position, occurrence as u16, node as u16, index)
                                .map_err(|e| misaka_palw_tir::TirError::new(misaka_palw_tir::TirErrorKind::Operand, e.to_string()))?,
                        )
                        .map_err(|e| misaka_palw_tir::TirError::new(misaka_palw_tir::TirErrorKind::Operand, e))?,
                    );
                }
                consumer
                    .relation(
                        0,
                        randomness.seed,
                        plan,
                        position,
                        occurrence as u16,
                        *block,
                        node as u16,
                        &inputs,
                        &values[occurrence][node],
                    )
                    .map_err(|e| misaka_palw_tir::TirError::new(misaka_palw_tir::TirErrorKind::Operand, e))?;
            }
        }
        Ok(())
    })
}

impl NativeCompactProofV4 {
    /// Exact allocation-free wire walk before Borsh decodes nested transcript
    /// vectors. A short frame cannot authorize a large announced allocation.
    pub fn decode_bounded(bytes: &[u8], maximum_bytes: u64, maximum_heap: u64) -> Result<Self, String> {
        fn take<'a>(r: &mut &'a [u8], n: usize) -> Result<&'a [u8], String> {
            if n > r.len() {
                return Err("truncated compact proof".into());
            }
            let (a, b) = r.split_at(n);
            *r = b;
            Ok(a)
        }
        fn count(r: &mut &[u8]) -> Result<usize, String> {
            Ok(u32::from_le_bytes(take(r, 4)?.try_into().unwrap()) as usize)
        }
        if bytes.len() as u64 > maximum_bytes {
            return Err("compact proof frame exceeds capacity".into());
        }
        let mut r = bytes;
        take(&mut r, 2 + 64 + 64)?;
        let n = count(&mut r)?;
        if n > r.len() / 157 {
            return Err("compact proof count exceeds exact frame".into());
        }
        let mut heap = (bytes.len() as u128).checked_mul(2).ok_or("compact proof heap overflow")?;
        heap = heap.checked_add(n as u128 * 512).ok_or("compact proof index heap overflow")?;
        for _ in 0..n {
            take(&mut r, 4 + 64 + 64 + 4 + 2 + 4 + 1 + 4 + 4 + 2)?;
            let outer = count(&mut r)?;
            if outer > r.len() / 4 {
                return Err("compact transcript count exceeds frame".into());
            }
            heap = heap.checked_add(outer as u128 * 24).ok_or("compact transcript heap overflow")?;
            for _ in 0..outer {
                let rounds = count(&mut r)?;
                if rounds > r.len() / 4 {
                    return Err("compact sumcheck rounds exceed frame".into());
                }
                heap = heap.checked_add(rounds as u128 * 24).ok_or("compact round heap overflow")?;
                for _ in 0..rounds {
                    let words = count(&mut r)?;
                    let n = words.checked_mul(16).ok_or("compact word bytes overflow")?;
                    take(&mut r, n)?;
                    heap = heap.checked_add(n as u128).ok_or("compact word heap overflow")?;
                }
            }
            if heap > u128::from(maximum_heap) {
                return Err("compact proof decoded workspace exceeds capacity".into());
            }
        }
        if !r.is_empty() || heap > u128::from(maximum_heap) {
            return Err("compact proof trailing bytes/workspace overflow".into());
        }
        borsh::from_slice(bytes).map_err(|e| e.to_string())
    }
}

/// Includes the native located-proof envelope as well as every relation's own
/// worst transcript. Proof headers are not priced as zero-byte Freivalds data.
pub fn native_proof_bytes_bound_v4(coverage: &NativeCompactCoverageV2) -> Result<u128, String> {
    let mut total = 134u128; // version + plan + seed + proof Vec length
    for part in &coverage.parts {
        for relation in part.coverage.compact() {
            let count = u128::from(part.coverage.max_positions)
                .checked_mul(u128::from(part.maximum_executions))
                .and_then(|n| n.checked_mul(u128::from(relation.occurrences)))
                .ok_or("native proof instance bound overflow")?;
            total = total
                .checked_add(
                    proof_bytes_bound_v1(relation)
                        .checked_add(142)
                        .and_then(|n| n.checked_mul(count))
                        .ok_or("native located proof bound overflow")?,
                )
                .ok_or("native compact proof union overflow")?;
        }
    }
    Ok(total)
}

/// Versioned native protocol identity binds the declared adversary scope as
/// well as the relation suite. A review of one retry/grinding/round horizon is
/// never silently reused for another one under the same compiler digest.
pub fn compact_suite_id_v4(
    contract: &NativeCompactContractV2,
    coverage: &NativeCompactCoverageV2,
    adversary: CompactAdversaryBudgetV4,
) -> Digest {
    object_id(b"MISAKA/PALW/NATIVE-COMPACT-SUITE/V4", &(contract.suite_id(coverage), adversary))
}
