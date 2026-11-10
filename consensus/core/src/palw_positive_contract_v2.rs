//! A1 implementation of the A0 r1 NodeChecker port. Private interchange only.
//! A4 must resolve the rooted admission inputs and authorize each carrier before
//! calling this adapter. Signatures here authenticate authors, not native
//! admission, drawn membership, funding, deadlines or computation truth.

pub mod record;

use crate::palw_positive_check_v2::{CheckError, CheckLimits, CheckOutcome, CheckProgress, CoveragePlan, ExactChecker};
use crate::{
    Hash64, palw_attempt_rules_v1::palw_attempt_output_root_v1, palw_decode_select_v2::PalwDecodeSamplingV2,
    palw_prompt_ids_v1::PalwPromptIdsFormV1, palw_tir_class_v1::PalwTirClassV1, palw_tir_step_v1::PalwTirStepBindingV1,
    palw_v2::PalwJobContextV2,
};
use misaka_palw_positive_contract::{
    ContractError,
    identity::{ChainScope, Digest, PUBKEY_BYTES, PrincipalId, ResultId, SIGNATURE_BYTES, WitnessId},
    messages::{
        AdmissionContext, AggregateBudget, CheckProfile, ClaimForm, CoverageCursor, CoverageDescriptor, MinerResultCommit,
        PanelWitnessCommit, coverage_plan_id,
    },
    ports::{NodeChecker, require_next},
    public_one_honest::{
        FirstNoticeDemand, ManifestV1, MaterialCaseDemand, OriginalClock, OriginalCommit, OriginalId, ProtectedRescueDemand,
        VerificationSchedule,
    },
    reservations::RemainingCapacity,
    rows::AggregateUsage,
    tir::{EXACT_PART_HEADER_BYTES, decode_frame, exact_derivation_id, greedy_selector_id, provenance_id, raw_part_budget},
    wire::{AuthenticatedEvidenceFrame, EVIDENCE_HEADER_BYTES, EvidenceFrame},
};

pub const PART_OVERHEAD: u64 = (EVIDENCE_HEADER_BYTES + EXACT_PART_HEADER_BYTES) as u64;

/// Transport quote only; A4 reserves native wrapper/transaction overhead too.
/// Existing frame limits remain unsigned and are never silently reinterpreted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignedTransportQuote {
    pub max_packet_bytes: u64,
    pub unsigned_frame_bytes: u64,
    pub signature_bytes: u64,
    pub total_packet_bytes: u64,
}
pub(crate) fn signed_quote(count: u64, frame_cap: u64) -> Result<SignedTransportQuote, AdapterError> {
    let max_packet_bytes = frame_cap.checked_add(SIGNATURE_BYTES as u64).ok_or(CheckError::Budget)?;
    let unsigned_frame_bytes = count.checked_mul(frame_cap).ok_or(CheckError::Budget)?;
    let signature_bytes = count.checked_mul(SIGNATURE_BYTES as u64).ok_or(CheckError::Budget)?;
    let total_packet_bytes = unsigned_frame_bytes.checked_add(signature_bytes).ok_or(CheckError::Budget)?;
    Ok(SignedTransportQuote { max_packet_bytes, unsigned_frame_bytes, signature_bytes, total_packet_bytes })
}
fn digest(hash: Hash64) -> Digest {
    Digest(hash.as_bytes())
}

/// Facts resolved from the canonical job, class and funded execution owner.
/// The native admission path supplies these independently of the candidate.
pub struct PublicOriginalAdmissionV1<'a> {
    pub scope: ChainScope,
    pub job: &'a PalwJobContextV2,
    pub class: &'a PalwTirClassV1,
    pub artifact: Hash64,
    pub miner: PrincipalId,
    pub prompt: Digest,
    pub initial_state: Digest,
    pub dependencies: Digest,
    pub service_contract: Digest,
    pub max_manifest_bytes: u32,
}

/// Native reservation inputs from the accepted profile and current ledger.
pub struct PublicOriginalReservationV1<'a> {
    pub inclusion_daa: u64,
    pub notice: FirstNoticeDemand,
    pub case: MaterialCaseDemand,
    pub initial_supply: RemainingCapacity,
    pub rescue: ProtectedRescueDemand,
    pub usage: &'a AggregateUsage,
    pub budget: &'a AggregateBudget,
}

/// Checked inputs for the native fold to book and persist atomically.
#[derive(Debug, PartialEq, Eq)]
pub struct PreparedPublicOriginalV1 {
    pub original: OriginalId,
    pub schedule: VerificationSchedule,
    pub capacity: RemainingCapacity,
    inclusion_daa: u64,
    initial_supply: RemainingCapacity,
    rescue: ProtectedRescueDemand,
}

/// Fixed-size record to store in the same state update as the reservation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PublicOriginalAdmissionRecordV1 {
    original: OriginalId,
    inclusion_daa: u64,
    reservation: Digest,
    initial_supply: RemainingCapacity,
    rescue: ProtectedRescueDemand,
    deadlines: [u64; 7],
}

impl PreparedPublicOriginalV1 {
    pub fn record(&self, reservation: Digest) -> PublicOriginalAdmissionRecordV1 {
        PublicOriginalAdmissionRecordV1 {
            original: self.original,
            inclusion_daa: self.inclusion_daa,
            reservation,
            initial_supply: self.initial_supply,
            rescue: self.rescue,
            deadlines: [
                self.schedule.first_notice_by(),
                self.schedule.first_material_request_by(),
                self.schedule.material_ready_by(),
                self.schedule.inspection_by(),
                self.schedule.localized_by(),
                self.schedule.filed_by(),
                self.schedule.terminal_by(),
            ],
        }
    }
}

impl PublicOriginalAdmissionRecordV1 {
    /// Match against admission re-derived at the original DAA and against the
    /// reservation from native state. A stored record alone grants no rights.
    pub fn check_restore(&self, prepared: &PreparedPublicOriginalV1, reservation: Digest) -> Result<(), ContractError> {
        if *self != prepared.record(reservation) {
            return Err(ContractError::Scope);
        }
        Ok(())
    }
}

impl PublicOriginalAdmissionV1<'_> {
    pub fn prepare(
        &self,
        original: &OriginalCommit,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
        reservation: &PublicOriginalReservationV1<'_>,
    ) -> Result<PreparedPublicOriginalV1, ContractError> {
        let body_bytes = u64::from(ManifestV1::fetch_size(self.max_manifest_bytes)?);
        if reservation.initial_supply.retained_bytes < body_bytes
            || reservation.rescue.per_drawn_bond.transmitted_bytes < body_bytes
            || (reservation.rescue.max_outsiders != 0 && reservation.rescue.per_outsider.transmitted_bytes < body_bytes)
        {
            return Err(ContractError::Budget);
        }
        let clock = OriginalClock::new(reservation.inclusion_daa)?;
        if reservation.budget.retention_until < clock.hard() {
            return Err(ContractError::Budget);
        }
        let schedule = VerificationSchedule::new(clock, reservation.notice, reservation.case)?;
        let capacity = reservation.initial_supply.checked_add(reservation.rescue.total()?)?;
        capacity.check(reservation.usage, reservation.budget)?;
        let original = self.authenticate(original, key, signature)?;
        Ok(PreparedPublicOriginalV1 {
            original,
            schedule,
            capacity,
            inclusion_daa: reservation.inclusion_daa,
            initial_supply: reservation.initial_supply,
            rescue: reservation.rescue,
        })
    }

    /// Verify the signed original against native facts. The caller must still
    /// reserve resources and persist admission before drawing any Panel bonds.
    pub fn authenticate(
        &self,
        original: &OriginalCommit,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<OriginalId, ContractError> {
        ManifestV1::fetch_size(self.max_manifest_bytes)?;
        let job = self.job;
        let class = self.class;
        if original.scope != self.scope
            || original.miner != self.miner
            || original.job != digest(job.job_id)
            || original.artifact != digest(self.artifact)
            || original.registered_class != digest(job.shape_profile_id)
            || original.prompt != self.prompt
            || original.input_tokens != digest(job.prompt_token_ids_hash)
            || original.tokenizer != digest(job.tokenizer_id)
            || original.execution_rules != digest(job.cu_ruleset_id)
            || original.generation_seed != job.execution_seed
            || original.initial_state != self.initial_state
            || original.dependencies != self.dependencies
            || original.prefill != job.declared_prefill_tokens
            || original.decode != job.exact_decode_tokens
            || original.max_context != job.max_context_tokens
            || original.service_contract != self.service_contract
            || original.manifest_root == Digest([0; 64])
        {
            return Err(ContractError::Scope);
        }
        if original.execution != digest(job.context_hash())
            || original.registered_class != digest(class.class_id(&self.artifact))
            || original.program != digest(class.graph_ir_root())
        {
            return Err(ContractError::Scope);
        }
        original.authenticate(key, signature)?;
        Ok(original.id())
    }
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
    // Exact independently resolved metadata, retained for primitive record
    // reconstruction. These are not decoded from an evidence carrier.
    native_class: PalwTirClassV1,
    native_job: PalwJobContextV2,
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
        if result.temperature_q24 != 0 || result.selector != greedy_selector_id() {
            return Err(CheckError::UnsupportedProfile.into());
        }
        let mut inner = limits;
        inner.max_part_bytes = raw_part_budget(limits.max_part_bytes)?;
        // Determine the canonical count, then reserve ALL outer/prefix bytes.
        let provisional = CoveragePlan::derive(registered, artifact, job, form, inner)?;
        provisional
            .leaf_count()
            .checked_mul(limits.max_part_bytes)
            .filter(|n| *n <= limits.max_total_bytes)
            .ok_or(CheckError::Budget)?;
        let overhead = provisional.leaf_count().checked_mul(PART_OVERHEAD).ok_or(CheckError::Budget)?;
        inner.max_total_bytes = limits.max_total_bytes.checked_sub(overhead).ok_or(CheckError::Budget)?;
        let plan = CoveragePlan::derive(registered, artifact, job, form, inner)?;
        let cost = plan.cost();
        let descriptor = CoverageDescriptor {
            scope: result.scope,
            result: result.id(),
            plan: coverage_plan_id(&result, CheckProfile::ExactStreamV1, exact_derivation_id(), plan.leaf_count())?,
            profile: CheckProfile::ExactStreamV1,
            mandatory_relations: plan.leaf_count(),
            reserved_elements: cost.evaluation_elements,
            reserved_terms: cost.evaluation_terms,
            max_part_bytes: limits.max_part_bytes,
        };
        Ok(Self { result, plan, descriptor, form, limits, native_class: registered.clone(), native_job: job.clone() })
    }
    pub fn descriptor(&self) -> CoverageDescriptor {
        self.descriptor
    }
    pub fn reserved_wire_bytes(&self) -> u64 {
        // Both checked during construction and bounded by max_total_bytes.
        self.plan.leaf_count() * self.limits.max_part_bytes
    }
    pub fn signed_transport_quote(&self) -> Result<SignedTransportQuote, AdapterError> {
        signed_quote(self.plan.leaf_count(), self.limits.max_part_bytes)
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
        if witness.plan != self.descriptor.plan
            || witness.provenance != provenance_id(&self.result)
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
    /// Compare the complete opaque bindings and limits for an idempotent join.
    /// Native ingress must reserve this bounded comparison's work.
    pub fn same_identity(&self, other: &Self) -> bool {
        self.identity == other.identity
    }

    /// Identity of the already authenticated candidate, for the Court adapter.
    /// This accessor grants no native admission or computation authority.
    pub fn witness_id(&self) -> WitnessId {
        self.witness.id()
    }
    pub fn signed_transport_quote(&self) -> Result<SignedTransportQuote, AdapterError> {
        signed_quote(self.identity.descriptor.mandatory_relations, self.identity.limits.max_part_bytes)
    }
    pub fn start(&self) -> Result<ContractProgress, AdapterError> {
        Ok(ContractProgress {
            identity: self.identity.clone(),
            wire_bytes: 0,
            signature_bytes: 0,
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
    signature_bytes: u64,
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
    pub fn charged_signature_bytes(&self) -> u64 {
        self.signature_bytes
    }
    pub fn charged_transport_bytes(&self) -> Result<u64, AdapterError> {
        self.wire_bytes.checked_add(self.signature_bytes).ok_or(CheckError::Budget.into())
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
impl TirNodeChecker {
    /// Failed signature/foreign scope leaves the candidate and counters intact.
    /// A4 still owns current drawn membership, admission and duty authorization.
    pub fn check_signed_part(
        &self,
        context: &CheckContext,
        progress: &mut ContractProgress,
        frame: EvidenceFrame<'_>,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<(), AdapterError> {
        let quote = context.signed_transport_quote()?;
        let authenticated = frame.authenticate(&context.witness, key, signature, quote.max_packet_bytes)?;
        self.check_authenticated_part(context, progress, authenticated)
    }
    pub fn check_authenticated_part(
        &self,
        context: &CheckContext,
        progress: &mut ContractProgress,
        part: AuthenticatedEvidenceFrame<'_>,
    ) -> Result<(), AdapterError> {
        if context.identity != progress.identity || part.author() != context.witness.author {
            return Err(ContractError::Identity.into());
        }
        if progress.failure.is_some() {
            return Err(AdapterError::Closed);
        }
        let frame = part.frame();
        require_next(
            &context.identity.descriptor,
            CoverageCursor { plan: context.identity.descriptor.plan, next_relation: progress.checker.progress().next_leaf },
            &context.witness,
            &frame,
        )?;
        if frame.header().relation_count != 1 {
            return Err(ContractError::NonCanonical.into());
        }
        let quote = context.signed_transport_quote()?;
        // Author-authenticated same-scope attempts pay the signature even if
        // inner material fails. A4 owns aggregate retries and ingress refusals.
        progress.signature_bytes = progress
            .signature_bytes
            .checked_add(SIGNATURE_BYTES as u64)
            .filter(|n| *n <= quote.signature_bytes)
            .ok_or(CheckError::Budget)?;
        self.check_part(context, progress, frame)
    }
}
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
        // Trusted A4/court port only. Untrusted packet ingress must use the
        // author-authenticating helpers above before candidate attribution.
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
            let payload = decode_frame(&part, context.identity.descriptor.max_part_bytes)?;
            progress.checker.submit_encoded(payload.cone(), payload.trace())?;
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
