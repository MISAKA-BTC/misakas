//! A1 implementation of the A0 r1 NodeChecker port. Private interchange only.
//! A4 must resolve the rooted admission inputs and authorize each carrier before
//! calling this adapter. Signatures here authenticate authors, not native
//! admission, drawn membership, funding, deadlines or computation truth.

use super::positive_check::{CheckError, CheckLimits, CheckOutcome, CheckProgress, CoveragePlan, ExactChecker};
use kaspa_consensus_core::{
    Hash64, palw_attempt_rules_v1::palw_attempt_output_root_v1, palw_decode_select_v2::PalwDecodeSamplingV2,
    palw_prompt_ids_v1::PalwPromptIdsFormV1, palw_tir_class_v1::PalwTirClassV1, palw_tir_step_v1::PalwTirStepBindingV1,
    palw_v2::PalwJobContextV2,
};
use misaka_palw_positive_contract::{
    ContractError,
    identity::{Digest, PUBKEY_BYTES, ResultId, SIGNATURE_BYTES, WitnessId},
    messages::{
        AdmissionContext, CheckProfile, ClaimForm, CoverageCursor, CoverageDescriptor, MinerResultCommit, PanelWitnessCommit,
        coverage_plan_id,
    },
    ports::{NodeChecker, require_next},
    wire::{EVIDENCE_HEADER_BYTES, EvidenceFrame},
};

/// A0 owns derivation/selector/provenance and the canonical 16-byte G14 payload.
pub const PART_OVERHEAD: u64 = (EVIDENCE_HEADER_BYTES + misaka_palw_positive_contract::tir::EXACT_PART_HEADER_BYTES) as u64;
pub fn derivation_id() -> Digest {
    misaka_palw_positive_contract::tir::exact_derivation_id()
}

fn digest(hash: Hash64) -> Digest {
    Digest(hash.as_bytes())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdapterError {
    Contract(ContractError),
    Check(CheckError),
    Binding,
    Closed,
}
impl From<ContractError> for AdapterError {
    fn from(e: ContractError) -> Self {
        Self::Contract(e)
    }
}
impl From<CheckError> for AdapterError {
    fn from(e: CheckError) -> Self {
        Self::Check(e)
    }
}

/// Verified Miner commitment plus independently derived plan. No native credit
/// capability. Limits include the complete outer frame, not only G14 payloads.
pub struct PreparedCheck {
    result: MinerResultCommit,
    plan: CoveragePlan,
    descriptor: CoverageDescriptor,
    form: PalwPromptIdsFormV1,
    limits: CheckLimits,
}
impl PreparedCheck {
    /// Native inputs must be resolved independently of the candidate. The
    /// private TIR adapter uses job_id as `job`, context_hash as `execution`, and
    /// the existing graph root/input root/CU rules digest in their named fields.
    /// A4's rooted selector must designate the greedy rule this derivation pins.
    #[allow(clippy::too_many_arguments)]
    pub fn authenticate(
        rooted: &AdmissionContext,
        registered: &PalwTirClassV1,
        artifact: Hash64,
        job: &PalwJobContextV2,
        form: PalwPromptIdsFormV1,
        limits: CheckLimits,
        result: MinerResultCommit,
        miner_key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<Self, AdapterError> {
        rooted.check(ClaimForm::PositiveResultV2, &result)?;
        result.authenticate(miner_key, signature)?;
        if result.job != digest(job.job_id)
            || result.execution != digest(job.context_hash())
            || result.registered_class != digest(registered.class_id(&artifact))
            || result.artifact != digest(artifact)
            || result.program != digest(registered.graph_ir_root())
            || result.input != digest(job.prompt_token_ids_hash)
            || result.tokenizer != digest(job.tokenizer_id)
            || result.execution_rules != digest(job.cu_ruleset_id)
            || result.generation_seed != job.execution_seed
            || result.prefill != job.declared_prefill_tokens
            || result.decode != job.exact_decode_tokens
            || result.max_context != job.max_context_tokens
        {
            return Err(AdapterError::Binding);
        }
        if result.temperature_q24 != 0 || result.selector != misaka_palw_positive_contract::tir::greedy_selector_id() {
            return Err(CheckError::UnsupportedProfile.into());
        }
        let mut inner = limits;
        inner.max_part_bytes = misaka_palw_positive_contract::tir::raw_part_budget(limits.max_part_bytes)
            .map_err(|_| AdapterError::Check(CheckError::Budget))?;
        // Determine the canonical count, then reserve ALL outer/prefix bytes.
        let provisional = CoveragePlan::derive(registered, artifact, job, form, inner)?;
        let overhead = provisional.leaf_count().checked_mul(PART_OVERHEAD).ok_or(CheckError::Budget)?;
        inner.max_total_bytes = limits.max_total_bytes.checked_sub(overhead).ok_or(CheckError::Budget)?;
        let plan = CoveragePlan::derive(registered, artifact, job, form, inner)?;
        let cost = plan.cost();
        let descriptor = CoverageDescriptor {
            scope: result.scope,
            result: result.id(),
            plan: coverage_plan_id(&result, CheckProfile::ExactStreamV1, derivation_id(), plan.leaf_count())?,
            profile: CheckProfile::ExactStreamV1,
            mandatory_relations: plan.leaf_count(),
            reserved_elements: cost.evaluation_elements,
            reserved_terms: cost.evaluation_terms,
            max_part_bytes: limits.max_part_bytes,
        };
        Ok(Self { result, plan, descriptor, form, limits })
    }
    pub fn descriptor(&self) -> CoverageDescriptor {
        self.descriptor
    }
    pub fn reserved_wire_bytes(&self) -> u64 {
        // Both checked during construction and bounded by max_total_bytes.
        self.plan.cost().encoded_bytes + self.plan.leaf_count() * PART_OVERHEAD
    }
    #[allow(clippy::too_many_arguments)]
    pub fn authenticate_witness(
        self,
        witness: PanelWitnessCommit,
        binding: PalwTirStepBindingV1,
        generated: Vec<u32>,
        panel_key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<CheckContext, AdapterError> {
        witness.authenticate(&self.result, panel_key, signature)?;
        self.plan.validate_witness(&binding, &generated)?;
        if witness.provenance != misaka_palw_positive_contract::tir::provenance_id(&self.result)
            || witness.plan != self.descriptor.plan
            || witness.execution_record != digest(binding.committed_execution_root)
            || witness.candidate_output != digest(palw_attempt_output_root_v1(&binding.job_context, &generated))
        {
            return Err(AdapterError::Binding);
        }
        let identity = ContextIdentity {
            descriptor: self.descriptor,
            witness: witness.id(),
            binding,
            generated,
            expected_output: self.result.output,
            limits: self.limits,
            form: self.form,
            seed: self.result.generation_seed,
        };
        Ok(CheckContext { plan: self.plan, witness, identity })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ContextIdentity {
    descriptor: CoverageDescriptor,
    witness: WitnessId,
    binding: PalwTirStepBindingV1,
    generated: Vec<u32>,
    expected_output: Digest,
    limits: CheckLimits,
    form: PalwPromptIdsFormV1,
    seed: [u8; 32],
}
pub struct CheckContext {
    plan: CoveragePlan,
    witness: PanelWitnessCommit,
    identity: ContextIdentity,
}
impl CheckContext {
    pub fn witness_id(&self) -> WitnessId {
        self.witness.id()
    }
    pub fn start(&self) -> Result<ContractProgress, AdapterError> {
        Ok(ContractProgress {
            identity: self.identity.clone(),
            wire_bytes: 0,
            failure: None,
            checker: ExactChecker::begin_greedy(
                self.plan.clone(),
                self.identity.binding.clone(),
                self.identity.generated.clone(),
                Hash64::from_bytes(self.identity.expected_output.0),
                PalwDecodeSamplingV2 { seed: self.identity.seed, temperature_q: 0 },
            )?,
        })
    }
}
/// No Deserialize or public cursor setter. Recovery replays the same frames.
pub struct ContractProgress {
    identity: ContextIdentity,
    checker: ExactChecker,
    wire_bytes: u64,
    failure: Option<CheckOutcome>,
}
impl ContractProgress {
    pub fn snapshot(&self) -> CheckProgress {
        let mut snapshot = self.checker.progress();
        if let Some(outcome) = self.failure {
            snapshot.outcome = outcome;
        }
        snapshot
    }
    pub fn charged_wire_bytes(&self) -> u64 {
        self.wire_bytes
    }
}
/// A checked local fact, not a Gate C/payment/native verdict authority.
pub struct CheckedTerminal {
    result: ResultId,
    witness: WitnessId,
    descriptor: CoverageDescriptor,
    outcome: CheckOutcome,
}
impl CheckedTerminal {
    pub fn result(&self) -> ResultId {
        self.result
    }
    pub fn witness(&self) -> WitnessId {
        self.witness
    }
    pub fn descriptor(&self) -> CoverageDescriptor {
        self.descriptor
    }
    pub fn outcome(&self) -> CheckOutcome {
        self.outcome
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TirNodeChecker;
impl NodeChecker for TirNodeChecker {
    type Context = CheckContext;
    type Progress = ContractProgress;
    type CheckedTerminal = CheckedTerminal;
    type Error = AdapterError;
    fn derive_plan(&self, context: &CheckContext) -> Result<CoverageDescriptor, AdapterError> {
        Ok(context.identity.descriptor)
    }
    fn check_part(
        &self,
        context: &CheckContext,
        progress: &mut ContractProgress,
        part: EvidenceFrame<'_>,
    ) -> Result<(), AdapterError> {
        if context.identity != progress.identity {
            return Err(ContractError::Identity.into());
        }
        if progress.failure.is_some() {
            return Err(AdapterError::Closed);
        }
        // Foreign identity cannot poison another candidate's progress. A4 must
        // authorize the carrier BEFORE attributing malformed bytes to this Panel.
        let end = require_next(
            &context.identity.descriptor,
            CoverageCursor { plan: context.identity.descriptor.plan, next_relation: progress.checker.progress().next_leaf },
            &context.witness,
            &part,
        )?;
        if part.header().relation_count != 1 {
            return Err(ContractError::NonCanonical.into());
        }
        let result = (|| {
            progress.wire_bytes = progress
                .wire_bytes
                .checked_add(part.wire_bytes())
                .filter(|n| *n <= context.identity.limits.max_total_bytes)
                .ok_or(CheckError::Budget)?;
            let (cone, trace) = decode_payload(part.payload())?;
            progress.checker.submit_encoded(cone, trace)?;
            if progress.checker.progress().next_leaf != end {
                return Err(AdapterError::Binding);
            }
            Ok(())
        })();
        if let Err(error) = &result {
            progress.failure = Some(match error {
                AdapterError::Check(CheckError::Unverified | CheckError::Budget) => CheckOutcome::EndedUnverified,
                _ => CheckOutcome::WitnessRejected,
            });
        }
        result
    }
    fn finish(&self, context: &CheckContext, mut progress: ContractProgress) -> Result<CheckedTerminal, AdapterError> {
        if context.identity != progress.identity {
            return Err(ContractError::Identity.into());
        }
        if progress.failure.is_some() {
            return Err(AdapterError::Closed);
        }
        let outcome = progress.checker.finish()?;
        Ok(CheckedTerminal {
            result: context.identity.descriptor.result,
            witness: context.witness.id(),
            descriptor: context.identity.descriptor,
            outcome,
        })
    }
}

/// Canonical A0 TIR interchange. Length checks precede the inner G14 scanner.
pub fn decode_payload(bytes: &[u8]) -> Result<(&[u8], Option<&[u8]>), AdapterError> {
    let part = misaka_palw_positive_contract::tir::ExactPart::decode(bytes, bytes.len() as u64)
        .map_err(|_| AdapterError::Check(CheckError::Wire))?;
    Ok((part.cone(), part.trace()))
}
pub fn encode_payload(cone: &[u8], trace: Option<&[u8]>, max_payload_bytes: u64) -> Result<Vec<u8>, AdapterError> {
    Ok(misaka_palw_positive_contract::tir::ExactPart::new(cone, trace, max_payload_bytes)?.encode())
}
