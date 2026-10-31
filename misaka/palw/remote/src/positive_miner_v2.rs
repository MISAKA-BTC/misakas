//! A2's process adapter for the A0 Positive Court contract. No native activation.
//! Miner signs a result and reads stored G14 source units. No method executes a
//! witness builder, reconstructs a trace, accepts a Panel ACK or grants credit.
use std::{collections::BTreeMap, marker::PhantomData};

use kaspa_consensus_core::{
    Hash64,
    palw_artifact::{PalwArtifactOpeningV1, verify_artifact_opening_v1},
    palw_prompt_ids_v1::{
        PALW_PROMPT_IDS_TILE_LEN, PalwPromptIdsFormV1, PalwPromptIdsOpeningV1, prompt_ids_tile_count_v1, prompt_token_ids_match_v1,
        verify_prompt_ids_opening_v1,
    },
};
use misaka_palw_positive_contract::{
    ContractError,
    identity::{Digest, PUBKEY_BYTES, RESULT_SIGN_CONTEXT, SIGNATURE_BYTES},
    messages::{
        AdmissionContext, AggregateBudget, ClaimForm, MinerDataStatus, MinerResultCommit, MinerServiceContract, SourceKind,
        SourceRequest,
    },
    ports::MinerSource,
    wire::encode_result,
};
use misaka_palw_tir_exec::{NoSink, Slice, TirExecutor, TirParams, TirPlan};

#[derive(Clone, Copy, Debug)]
pub struct InferenceLimitsV2 {
    pub max_prefill: u32,
    pub max_decode: u32,
    pub max_positions: u32,
}

/// Ordinary fixed-length greedy inference. `NoSink` records no intermediate
/// values, logits rows, trace leaves or evidence trees. Only normal model state
/// and generated token IDs survive. The caller binds the registered plan/params
/// and output to its admitted job before `sign_result_v2`; this is no Node check.
pub fn infer_tokens_v2(
    plan: &TirPlan,
    params: &TirParams<'_>,
    prompt: &[u32],
    decode: u32,
    limits: InferenceLimitsV2,
) -> Result<Vec<u32>, MinerErrorV2> {
    let prefill = u32::try_from(prompt.len()).map_err(|_| MinerErrorV2::Budget)?;
    let positions = prefill.checked_add(decode.checked_sub(1).ok_or(MinerErrorV2::Binding)?).ok_or(MinerErrorV2::Budget)?;
    if prefill == 0
        || prefill > limits.max_prefill
        || decode > limits.max_decode
        || positions > limits.max_positions
        || prompt.iter().any(|token| *token >= plan.program.token_bound)
    {
        return Err(MinerErrorV2::Budget);
    }
    let mut executor = TirExecutor::new(plan, params).map_err(|_| MinerErrorV2::Binding)?;
    let mut generated = Vec::with_capacity(decode as usize);
    for position in 0..positions {
        let token = if position < prefill { prompt[position as usize] } else { *generated.last().ok_or(MinerErrorV2::Binding)? };
        let post = position + 1 >= prefill;
        executor.step_opt(token, &mut NoSink, post).map_err(|_| MinerErrorV2::Binding)?;
        if post {
            let selected = match executor.logits().1 {
                Slice::I32(values) => greedy_index(values),
                Slice::I64(values) => greedy_index(values),
                _ => None,
            }
            .ok_or(MinerErrorV2::Binding)?;
            let selected = u32::try_from(selected).map_err(|_| MinerErrorV2::Binding)?;
            if selected >= plan.program.token_bound {
                return Err(MinerErrorV2::Binding);
            }
            generated.push(selected);
        }
    }
    Ok(generated)
}

fn greedy_index<T: Ord>(values: &[T]) -> Option<usize> {
    values.iter().enumerate().reduce(|a, b| if b.1 > a.1 { b } else { a }).map(|x| x.0)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MinerErrorV2 {
    Contract(ContractError),
    Budget,
    Unavailable,
    Closed,
    Binding,
    Signer(String),
    Journal(String),
}
impl From<ContractError> for MinerErrorV2 {
    fn from(error: ContractError) -> Self {
        Self::Contract(error)
    }
}

/// Sign a COMPLETE inference result with the A0 domain. Output/conditions are
/// fixed by that commit, with no trace-root field or Panel-wide endorsement.
/// This authenticates the Miner's claim; only Node checking judges its truth.
pub fn sign_result_v2(
    admission: &AdmissionContext,
    result: MinerResultCommit,
    contract: &MinerServiceContract,
    now: u64,
    key: &[u8; PUBKEY_BYTES],
    sign: impl FnOnce(&[u8], &[u8]) -> Result<[u8; SIGNATURE_BYTES], String>,
) -> Result<(MinerResultCommit, [u8; SIGNATURE_BYTES]), MinerErrorV2> {
    admission.check(ClaimForm::PositiveResultV2, &result)?;
    contract.validate(&result, now)?;
    encode_result(&result)?;
    // Check key identity before invoking the key holder.
    if misaka_palw_positive_contract::identity::principal(key) != result.miner {
        return Err(ContractError::Identity.into());
    }
    let signature = sign(&result.id().0.0, RESULT_SIGN_CONTEXT).map_err(MinerErrorV2::Signer)?;
    result.authenticate(key, &signature)?;
    Ok((result, signature))
}

/// Existing G14 data types; this enum is a process DTO, without a wire codec.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoredUnitV2 {
    Artifact(PalwArtifactOpeningV1),
    Input(PalwPromptIdsOpeningV1),
    /// Existing whole-prompt G14 input under the legacy flat commitment.
    FlatInput(Vec<u32>),
}
impl StoredUnitV2 {
    fn bounded(&self, max: u64) -> bool {
        match self {
            Self::Artifact(unit) => {
                unit.operand.bytes.len() as u64 <= max && unit.operand.tensor_name.len() as u64 <= max && unit.path.len() <= 32
            }
            Self::Input(unit) => unit.tile_ids.len() <= PALW_PROMPT_IDS_TILE_LEN as usize && unit.opening.siblings.len() <= 32,
            Self::FlatInput(ids) => (ids.len() as u64).checked_mul(4).is_some_and(|n| n <= max),
        }
    }
    fn sizes(&self) -> Result<(u64, u64), MinerErrorV2> {
        match self {
            Self::FlatInput(ids) => Ok((ids.len() as u64 * 4, 4 + ids.len() as u64 * 4)),
            Self::Artifact(unit) => {
                Ok((unit.operand.bytes.len() as u64, borsh::object_length(unit).map_err(|_| MinerErrorV2::Binding)? as u64))
            }
            Self::Input(unit) => {
                Ok((unit.tile_ids.len() as u64 * 4, borsh::object_length(unit).map_err(|_| MinerErrorV2::Binding)? as u64))
            }
        }
    }
    fn authenticates(&self, result: &MinerResultCommit, request: &SourceRequest, index: u64, units: u64) -> bool {
        match (self, request.kind) {
            (Self::Artifact(unit), SourceKind::ArtifactInventory) => {
                request.source == result.artifact
                    && u64::from(unit.leaf_index) == index
                    && u64::from(unit.leaf_count) == units
                    && verify_artifact_opening_v1(unit, Hash64::from_bytes(request.source.0)).is_ok()
            }
            (Self::Input(unit), SourceKind::Input) => {
                request.source == result.input
                    && u64::from(unit.tile_index) == index
                    && prompt_ids_tile_count_v1(u64::from(result.prefill)) == Some(units)
                    && verify_prompt_ids_opening_v1(&Hash64::from_bytes(request.source.0), result.prefill, unit).is_ok()
            }
            (Self::FlatInput(ids), SourceKind::Input) => {
                request.source == result.input
                    && units == 1
                    && index == 0
                    && ids.len() == result.prefill as usize
                    && prompt_token_ids_match_v1(PalwPromptIdsFormV1::Flat, ids, &Hash64::from_bytes(request.source.0))
            }
            _ => false,
        }
    }
}

/// Stored data and PREEXISTING openings only. Returning a borrowed unit lets
/// A2 enforce the byte cap before cloning it into an outbound response. Network
/// providers must apply these same read bounds in their transport adapter.
pub trait StoredProviderV2 {
    fn unit(&self, kind: SourceKind, source: Digest, index: u64) -> Result<&StoredUnitV2, String>;
}

/// A4's capability supplies these finite bounds from the accepted, signed
/// service contract, including retention and fees. A2 creates no financial hold.
#[derive(Clone, Copy, Debug)]
pub struct DutyBoundsV2 {
    pub source_units: u64,
    pub max_unit_bytes: u64,
    /// A4-derived conservative read+authentication cost per attempt.
    pub work_per_read: u64,
    pub budget: AggregateBudget,
    pub now: u64,
    pub cutoff: u64,
}

/// Connection to A4's authenticated canonical duty. A peer DTO is not a duty.
/// `authorize` must check result/case/contract/source/range, current branch,
/// actor scope, reserved service opportunity, fees and exclusive clocks from
/// rooted state. `status` must see provider/direct submissions without Panel ACKs.
/// No production implementation is installed until A4 has that native state.
pub trait CanonicalDataDutyV2 {
    fn request(&self) -> &SourceRequest;
    fn authorize(&self, result: &MinerResultCommit) -> Result<DutyBoundsV2, MinerErrorV2>;
    fn status(&self, index: u64) -> Result<MinerDataStatus, MinerErrorV2>;
}

/// Local preparation only; neither this value nor a transport receipt records
/// canonical completion. A4 authenticates each returned G14 unit on actual intake.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedDataV2 {
    pub request: SourceRequest,
    pub units: Vec<(u64, StoredUnitV2)>,
}

#[derive(Clone, Copy, Debug)]
pub struct ProcessLimitsV2 {
    pub max_units_per_request: u64,
    pub max_tracked_units: u64,
    pub max_provider_reads: u64,
    /// Cumulative canonical status lookups, including already completed units.
    pub max_status_checks: u64,
    pub max_prepared_bytes: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PreparationMeterV2 {
    pub rounds: u64,
    pub unique_source_bytes: u64,
    /// G14 unit body bytes, including openings and retries. A4 additionally
    /// meters actual outer framing/carriers in its native transmission ledger.
    pub prepared_bytes: u64,
    pub provider_reads: u64,
    pub status_checks: u64,
    pub additional_work: u64,
}

/// Keep ONE session per admitted case, across all Panel identities and attempts.
/// A6 must retain/replay the meter and union with the canonical case journal on
/// restart/reorg. A freshly constructed local session is not recovery authority.
pub struct MinerResponderV2<'p, D> {
    result: MinerResultCommit,
    contract: MinerServiceContract,
    providers: &'p [&'p dyn StoredProviderV2],
    limits: ProcessLimitsV2,
    meter: PreparationMeterV2,
    // Canonical unit identity excludes Panel/nonce and absorbs overlapping ranges.
    prepared: BTreeMap<(SourceKind, Digest, u64), u64>,
    duty: PhantomData<D>,
    poisoned: bool,
}
impl<'p, D: CanonicalDataDutyV2> MinerResponderV2<'p, D> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        admission: &AdmissionContext,
        result: MinerResultCommit,
        contract: MinerServiceContract,
        now: u64,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
        providers: &'p [&'p dyn StoredProviderV2],
        limits: ProcessLimitsV2,
    ) -> Result<Self, MinerErrorV2> {
        admission.check(ClaimForm::PositiveResultV2, &result)?;
        contract.validate(&result, now)?;
        result.authenticate(key, signature)?;
        if limits.max_units_per_request == 0
            || limits.max_tracked_units == 0
            || limits.max_provider_reads == 0
            || limits.max_status_checks == 0
            || limits.max_prepared_bytes == 0
        {
            return Err(MinerErrorV2::Budget);
        }
        Ok(Self {
            result,
            contract,
            providers,
            limits,
            meter: PreparationMeterV2::default(),
            prepared: BTreeMap::new(),
            duty: PhantomData,
            poisoned: false,
        })
    }
    pub fn meter(&self) -> PreparationMeterV2 {
        self.meter
    }
}

impl<D: CanonicalDataDutyV2> MinerSource for MinerResponderV2<'_, D> {
    type AuthenticatedDuty = D;
    type G14Response = PreparedDataV2;
    type Error = MinerErrorV2;

    fn respond(&mut self, duty: &D) -> Result<PreparedDataV2, MinerErrorV2> {
        self.respond_recorded(duty, &mut recovery::EphemeralPreparationJournalV2)
    }
}

impl<D: CanonicalDataDutyV2> MinerResponderV2<'_, D> {
    fn record(
        &mut self,
        journal: &mut impl recovery::PreparationJournalV2,
        added: Option<((SourceKind, Digest, u64), u64)>,
    ) -> Result<(), MinerErrorV2> {
        if let Err(error) = journal.persist(self, added) {
            self.poisoned = true;
            return Err(error);
        }
        Ok(())
    }

    /// Record every spent preparation/control budget before source I/O or
    /// releasing response bytes. A failed persist poisons this live session.
    pub fn respond_recorded(
        &mut self,
        duty: &D,
        journal: &mut impl recovery::PreparationJournalV2,
    ) -> Result<PreparedDataV2, MinerErrorV2> {
        if self.poisoned {
            return Err(MinerErrorV2::Closed);
        }
        let request = duty.request();
        if request.case != self.result.case_id() || request.contract != self.result.service_contract {
            return Err(MinerErrorV2::Binding);
        }
        match request.kind {
            SourceKind::ArtifactInventory if request.source == self.result.artifact => {}
            SourceKind::Input if request.source == self.result.input => {}
            _ => return Err(MinerErrorV2::Binding),
        }
        let bounds = duty.authorize(&self.result)?;
        request.validate(bounds.source_units)?;
        if bounds.cutoff > self.contract.deadline {
            return Err(MinerErrorV2::Binding);
        }
        if bounds.now >= bounds.cutoff || bounds.cutoff > bounds.budget.retention_until {
            return Err(MinerErrorV2::Closed);
        }
        self.contract.validate(&self.result, bounds.now)?;
        let signed = self.contract.budget;
        let count = request.end - request.first;
        if count > self.limits.max_units_per_request || bounds.max_unit_bytes == 0 || bounds.work_per_read == 0 {
            return Err(MinerErrorV2::Budget);
        }
        // Check every unit and reserve a conservative full response before I/O.
        // Already accepted provider/direct units are absent from the reply.
        let mut pending = Vec::new();
        let mut fresh = 0u64;
        for index in request.first..request.end {
            if self.meter.status_checks >= self.limits.max_status_checks {
                return Err(MinerErrorV2::Budget);
            }
            self.meter.status_checks += 1;
            self.record(journal, None)?;
            match duty.status(index)? {
                MinerDataStatus::Complete => continue,
                MinerDataStatus::NotRequested => return Err(MinerErrorV2::Binding),
                MinerDataStatus::Pending => {}
            }
            let next_fresh = fresh + u64::from(!self.prepared.contains_key(&(request.kind, request.source, index)));
            // Check the candidate allocation at each step, before growing the
            // pending vector. A huge request cannot hide behind a tiny consent.
            self.preflight(&bounds, signed, pending.len() as u64 + 1, next_fresh)?;
            fresh = next_fresh;
            pending.push(index);
        }
        if pending.is_empty() {
            return Ok(PreparedDataV2 { request: *request, units: Vec::new() });
        }
        let rounds = self.preflight(&bounds, signed, pending.len() as u64, fresh)?;
        let mut response = Vec::new();
        // Count a round before I/O, including failure, so an unavailable source
        // does not create an unbounded retry loophole for a fresh range.
        self.meter.rounds = rounds;
        self.record(journal, None)?;
        for index in pending {
            let mut found = false;
            for provider in self.providers {
                if self.meter.provider_reads >= self.limits.max_provider_reads {
                    return Err(MinerErrorV2::Budget);
                }
                let work = self.meter.additional_work.checked_add(bounds.work_per_read).ok_or(MinerErrorV2::Budget)?;
                if work > bounds.budget.additional_work.min(signed.additional_work) {
                    return Err(MinerErrorV2::Budget);
                }
                self.meter.additional_work = work;
                self.meter.provider_reads += 1;
                self.record(journal, None)?;
                let Ok(unit) = provider.unit(request.kind, request.source, index) else { continue };
                // Reject oversized fields before any serialization/hash walk.
                if !unit.bounded(bounds.max_unit_bytes) {
                    continue;
                }
                let (data_bytes, wire_bytes) = unit.sizes()?;
                if wire_bytes > bounds.max_unit_bytes || !unit.authenticates(&self.result, request, index, bounds.source_units) {
                    continue;
                }
                let key = (request.kind, request.source, index);
                let added = if let std::collections::btree_map::Entry::Vacant(entry) = self.prepared.entry(key) {
                    entry.insert(data_bytes);
                    self.meter.unique_source_bytes += data_bytes;
                    Some((key, data_bytes))
                } else {
                    None
                };
                self.meter.prepared_bytes += wire_bytes;
                self.record(journal, added)?;
                response.push((index, unit.clone()));
                found = true;
                break;
            }
            if !found {
                return Err(MinerErrorV2::Unavailable);
            }
        }
        Ok(PreparedDataV2 { request: *request, units: response })
    }
    fn preflight(&self, bounds: &DutyBoundsV2, signed: AggregateBudget, pending: u64, fresh: u64) -> Result<u64, MinerErrorV2> {
        let rounds = self.meter.rounds.checked_add(u64::from(fresh > 0)).ok_or(MinerErrorV2::Budget)?;
        let unique_ceiling = fresh
            .checked_mul(bounds.max_unit_bytes)
            .and_then(|n| self.meter.unique_source_bytes.checked_add(n))
            .ok_or(MinerErrorV2::Budget)?;
        let wire_ceiling = pending
            .checked_mul(bounds.max_unit_bytes)
            .and_then(|n| self.meter.prepared_bytes.checked_add(n))
            .ok_or(MinerErrorV2::Budget)?;
        if rounds > bounds.budget.rounds.min(signed.rounds)
            || unique_ceiling > bounds.budget.unique_source_bytes.min(signed.unique_source_bytes)
            || wire_ceiling > bounds.budget.transmitted_bytes.min(signed.transmitted_bytes)
            || wire_ceiling > self.limits.max_prepared_bytes
            || (self.prepared.len() as u64).checked_add(fresh).filter(|n| *n <= self.limits.max_tracked_units).is_none()
            || pending
                .checked_mul(bounds.max_unit_bytes)
                .filter(|n| *n <= bounds.budget.retained_bytes.min(signed.retained_bytes))
                .is_none()
            || self.meter.provider_reads >= self.limits.max_provider_reads
            || pending
                .checked_mul(bounds.work_per_read)
                .and_then(|n| self.meter.additional_work.checked_add(n))
                .filter(|n| *n <= bounds.budget.additional_work.min(signed.additional_work))
                .is_none()
        {
            return Err(MinerErrorV2::Budget);
        }
        Ok(rounds)
    }
}

#[cfg(test)]
#[path = "positive_miner_v2_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "positive_miner_v2_adversarial_tests.rs"]
mod adversarial_tests;
#[path = "positive_miner_v2_recovery.rs"]
pub mod recovery;
#[path = "positive_miner_v2_store.rs"]
pub mod store;

#[path = "positive_miner_v2_inference.rs"]
pub mod inference;
