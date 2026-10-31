//! Exact aux151 row schemas. All fields are fixed size or Option<fixed size>:
//! no untrusted collection length can allocate during decoding. These are DTOs,
//! never admission, funding, local-check or credit capabilities. Native owners
//! must authenticate origins, resolve references and replay canonical events.
use crate::{
    CONTRACT_REVISION, CONTRACT_VERSION, ContractError,
    funding::{FundingConsent, FundingPlanBasis, funding_scope},
    identity::*,
    messages::*,
    native::{NativeRowKey, ruleset_id},
};
use borsh::{BorshDeserialize, BorshSerialize};

/// Existing native BlockHash and TransactionId are 64 bytes (ADR-0008).
/// Kept distinct from semantic PALW digests despite the equal encoded width.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeHash64(pub [u8; 64]);

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeOutpoint {
    pub transaction: NativeHash64,
    pub index: u32,
}

/// Supplied by the actual accepted-prefix fold. Row contents do not establish
/// acceptance. No post-state root is stored inside the row that it commits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AcceptedOrigin {
    pub accepting_block: NativeHash64,
    pub carrier_block: NativeHash64,
    pub carrier_transaction: NativeHash64,
    pub accepted_tx_ordinal: u32,
    pub event_ordinal: u32,
    pub accepted_daa: u64,
    pub accepted_blue_score: u64,
}

/// Reference to a canonical, accepted material preimage. A peer's reference is
/// not a receipt. Resolve from accepted history and check the literal digest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AcceptedMaterial {
    pub origin: AcceptedOrigin,
    pub digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ResultRow {
    pub result: MinerResultCommit,
    pub miner_key: [u8; PUBKEY_BYTES],
    pub miner_signature: [u8; SIGNATURE_BYTES],
    pub origin: AcceptedOrigin,
    pub policy: Digest,
    pub fired_fork: Digest,
    pub funding_scope: FundingScopeId,
    /// Canonical dependency list, including its authenticated empty preimage.
    pub dependencies: AcceptedMaterial,
    /// Native role/bond assignment; not a miner-selected role authority.
    pub role_assignment: AcceptedMaterial,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ClaimIndexRow {
    pub claim: ClaimId,
    pub result: ResultId,
    pub case: CaseId,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AggregateUsage {
    pub rounds: u64,
    pub unique_source_bytes: u64,
    pub transmitted_bytes: u64,
    pub additional_work: u64,
    pub retained_bytes: u64,
    pub fee: u64,
    pub witness_attempts: u32,
}
impl AggregateUsage {
    pub fn validate(&self, budget: &AggregateBudget) -> Result<(), ContractError> {
        if self.rounds > budget.rounds
            || self.unique_source_bytes > budget.unique_source_bytes
            || self.transmitted_bytes > budget.transmitted_bytes
            || self.additional_work > budget.additional_work
            || self.retained_bytes > budget.retained_bytes
            || self.fee > budget.fee
            || self.witness_attempts > budget.witness_attempts
        {
            return Err(ContractError::Budget);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CaseRow {
    pub case: CaseId,
    pub result: ResultId,
    pub service: MinerServiceContract,
    pub coverage: CoverageDescriptor,
    pub status: ClaimStatus,
    // Deliberately separate: a failed/absent Panel cannot erase miner service.
    pub miner_data: MinerDataStatus,
    pub witness_status: WitnessStatus,
    pub positive_witness: Option<WitnessId>,
    pub usage: AggregateUsage,
    pub reservation: AcceptedMaterial,
    pub liability: AcceptedMaterial,
    pub maturity_daa: u64,
    pub unresolved_fraud: u32,
    pub last_event: AcceptedOrigin,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct WitnessRow {
    pub witness: PanelWitnessCommit,
    pub panel_key: [u8; PUBKEY_BYTES],
    pub panel_signature: [u8; SIGNATURE_BYTES],
    pub origin: AcceptedOrigin,
    pub record_header: AcceptedMaterial,
    pub generated_tokens: AcceptedMaterial,
    pub role_assignment: AcceptedMaterial,
}

/// Persisted cache only. A1 CheckedTerminal/A6 full replay is still mandatory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum CoverageState {
    Checking,
    Matched,
    Mismatched,
    EndedUnverified,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CoverageRow {
    pub witness: WitnessId,
    pub case: CaseId,
    pub result: ResultId,
    pub plan: PlanId,
    pub mandatory_relations: u64,
    pub next_relation: u64,
    /// A1 charged_wire_bytes: complete UNSIGNED EvidenceFrame bytes. Includes
    /// failed/retried authenticated attempts; it is not a receipt-only sum.
    pub frame_bytes: u64,
    /// Number of charged packet signatures, not their byte count.
    pub signatures: u64,
    pub elements: u64,
    pub terms: u64,
    pub state: CoverageState,
    pub last_event: AcceptedOrigin,
}

/// Union is across every author/attempt. Its canonical interval preimage is
/// recovered from accepted request history; this summary cannot authorize a
/// dedupe discount without that reconstruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SourceUnionRow {
    pub case: CaseId,
    pub kind: SourceKind,
    pub source: Digest,
    pub source_units: u64,
    pub requested_units: u64,
    pub completed_units: u64,
    pub union: AcceptedMaterial,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ServiceReceiptRow {
    pub case: CaseId,
    pub kind: SourceKind,
    pub source: Digest,
    pub unit: u64,
    pub result: ResultId,
    pub duty: AcceptedMaterial,
    pub response: AcceptedMaterial,
    pub provider: PrincipalId,
    /// Exact raw unique source data bytes, excluding opening/wire overhead.
    pub bytes: u64,
    /// Deterministic case service-schedule preparation charge for this unit,
    /// not physical CPU time or Node LLM arithmetic work.
    pub additional_work: u64,
    /// Literal once-only settlement schedule/event, not a paid boolean.
    pub settlement: Option<AcceptedMaterial>,
}

/// Conservation for one accounting leg, checked without wraparound.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BalanceLeg {
    pub initial: u64,
    pub paid: u64,
    pub refunded: u64,
    pub burned: u64,
    pub remaining: u64,
}
impl BalanceLeg {
    pub fn validate(&self) -> Result<(), ContractError> {
        let total = self
            .paid
            .checked_add(self.refunded)
            .and_then(|n| n.checked_add(self.burned))
            .and_then(|n| n.checked_add(self.remaining))
            .ok_or(ContractError::Budget)?;
        if total != self.initial {
            return Err(ContractError::Budget);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FundingRow {
    pub consent: FundingConsent,
    pub basis: FundingPlanBasis,
    pub payer_key: [u8; PUBKEY_BYTES],
    pub payer_signature: [u8; SIGNATURE_BYTES],
    pub funding_origin: AcceptedOrigin,
    pub original_outpoint: NativeOutpoint,
    pub current_outpoint: Option<NativeOutpoint>,
    pub initial_locked: u64,
    pub fee: BalanceLeg,
    pub capital: BalanceLeg,
    /// Any excess of actual output over quoted fee+capital is conserved too.
    pub surplus: BalanceLeg,
    pub liability: AcceptedMaterial,
    pub last_settlement: Option<AcceptedMaterial>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PacketReceiptRow {
    // Current r1 ExactPart authenticates exactly ONE mandatory relation. This
    // row records one successfully checked relation, not a failed/retried part.
    // Failed/retried authored material and its costs remain in canonical history.
    pub witness: WitnessId,
    pub first_relation: u64,
    pub frame: EvidencePartId,
    pub author: PrincipalId,
    pub signed_bytes: u64,
    pub origin: AcceptedOrigin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct JournalHeadRow {
    pub case: CaseId,
    pub sequence: u64,
    pub previous: Digest,
    pub head: Digest,
    pub history: AcceptedMaterial,
}

/// Variant order matches NativeRowKey, but key and value are always checked
/// independently. Append only; never infer a row type from byte size.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[allow(clippy::large_enum_variant)]
pub enum NativeRowBody {
    Result(ResultRow),
    ClaimIndex(ClaimIndexRow),
    Case(CaseRow),
    Witness(WitnessRow),
    Coverage(CoverageRow),
    SourceUnion(SourceUnionRow),
    ServiceReceipt(ServiceReceiptRow),
    Funding(FundingRow),
    PacketReceipt(PacketReceiptRow),
    JournalHead(JournalHeadRow),
}
impl NativeRowBody {
    pub fn key(&self) -> NativeRowKey {
        match self {
            Self::Result(r) => NativeRowKey::Result(r.result.id()),
            Self::ClaimIndex(r) => NativeRowKey::ClaimIndex(r.claim),
            Self::Case(r) => NativeRowKey::Case(r.case),
            Self::Witness(r) => NativeRowKey::Witness(r.witness.id()),
            Self::Coverage(r) => NativeRowKey::Coverage(r.witness),
            Self::SourceUnion(r) => NativeRowKey::SourceUnion { case: r.case, kind: r.kind, source: r.source },
            Self::ServiceReceipt(r) => NativeRowKey::ServiceReceipt { case: r.case, kind: r.kind, source: r.source, unit: r.unit },
            Self::Funding(r) => NativeRowKey::Funding(r.consent.case),
            Self::PacketReceipt(r) => NativeRowKey::PacketReceipt { witness: r.witness, first_relation: r.first_relation },
            Self::JournalHead(r) => NativeRowKey::JournalHead(r.case),
        }
    }

    /// Shape/local consistency only, with no accepted-origin, signature, live
    /// lock or arithmetic authority. Cross-row checks belong to native owners.
    pub fn validate_shape(&self) -> Result<(), ContractError> {
        match self {
            Self::Result(r) => {
                r.result.validate_shape()?;
                if r.result.scope.ruleset != ruleset_id(r.fired_fork, r.policy)
                    || r.dependencies.digest != r.result.dependencies
                    || principal(&r.miner_key) != r.result.miner
                {
                    return Err(ContractError::Identity);
                }
            }
            Self::Case(r) => {
                if r.service.version != CONTRACT_VERSION || r.service.revision != CONTRACT_REVISION {
                    return Err(ContractError::Version);
                }
                if r.case != r.service.case
                    || r.result != r.coverage.result
                    || r.coverage.profile != CheckProfile::ExactStreamV1
                    || r.coverage.mandatory_relations == 0
                    || r.service.budget.witness_attempts == 0
                    || r.service.budget.retention_until < r.service.deadline
                    || (r.status == ClaimStatus::PositivePassed
                        && (r.positive_witness.is_none() || r.witness_status != WitnessStatus::Complete))
                {
                    return Err(ContractError::Identity);
                }
                r.usage.validate(&r.service.budget)?;
            }
            Self::Witness(r) => {
                r.witness.validate_shape()?;
                if principal(&r.panel_key) != r.witness.author {
                    return Err(ContractError::Identity);
                }
            }
            Self::Coverage(r) => {
                if r.mandatory_relations == 0
                    || r.next_relation > r.mandatory_relations
                    || (matches!(r.state, CoverageState::Matched | CoverageState::Mismatched)
                        && r.next_relation != r.mandatory_relations)
                {
                    return Err(ContractError::Range);
                }
            }
            Self::SourceUnion(r) => {
                if r.completed_units > r.requested_units || r.requested_units > r.source_units {
                    return Err(ContractError::Range);
                }
            }
            Self::Funding(r) => {
                if r.consent.version != CONTRACT_VERSION || r.consent.revision != CONTRACT_REVISION {
                    return Err(ContractError::Version);
                }
                r.fee.validate()?;
                r.capital.validate()?;
                r.surplus.validate()?;
                let initial = r
                    .fee
                    .initial
                    .checked_add(r.capital.initial)
                    .and_then(|n| n.checked_add(r.surplus.initial))
                    .ok_or(ContractError::Budget)?;
                let remaining = r
                    .fee
                    .remaining
                    .checked_add(r.capital.remaining)
                    .and_then(|n| n.checked_add(r.surplus.remaining))
                    .ok_or(ContractError::Budget)?;
                if initial != r.initial_locked
                    || r.initial_locked > r.consent.max_locked
                    || r.fee.initial != r.basis.required_fee
                    || r.fee.initial > r.consent.max_fees
                    || r.capital.initial != r.basis.required_capital
                    || r.original_outpoint.transaction != r.funding_origin.carrier_transaction
                    || r.original_outpoint.index != r.consent.funding_output_index
                    || principal(&r.payer_key) != r.consent.payer
                    || r.basis.profile != CheckProfile::ExactStreamV1
                    || r.basis.mandatory_relations == 0
                    || (remaining > 0 && r.current_outpoint.is_none())
                {
                    return Err(ContractError::Budget);
                }
            }
            Self::PacketReceipt(r) => {
                if r.signed_bytes
                    < (crate::wire::EVIDENCE_HEADER_BYTES + crate::tir::EXACT_PART_HEADER_BYTES) as u64 + SIGNATURE_BYTES as u64
                {
                    return Err(ContractError::Range);
                }
            }
            Self::ClaimIndex(_) | Self::ServiceReceipt(_) | Self::JournalHead(_) => {}
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeRowValue {
    pub version: u16,
    pub revision: u16,
    pub body: NativeRowBody,
}

/// Bound is a codec ceiling, not a resource quote or a native activation.
pub const MAX_NATIVE_ROW_BYTES: usize = 16_384;
pub const MAX_NATIVE_KEY_BYTES: usize = 142;

/// Checks literal signed links at the ORIGINAL admission clock. The result is
/// still not an accepted-origin, registration, source, coin-lock or credit grant.
pub fn validate_bundle(index: &ClaimIndexRow, result: &ResultRow, case: &CaseRow, funding: &FundingRow) -> Result<(), ContractError> {
    NativeRowBody::Result(result.clone()).validate_shape()?;
    NativeRowBody::Case(*case).validate_shape()?;
    NativeRowBody::Funding(funding.clone()).validate_shape()?;
    let r = &result.result;
    if index.claim != r.claim_id()
        || index.result != r.id()
        || index.case != r.case_id()
        || case.case != index.case
        || case.result != index.result
        || case.service.id() != r.service_contract
        || case.coverage.scope != r.scope
        || result.policy != funding.basis.policy
        || result.funding_scope != funding.consent.scope
        || result.funding_scope != funding_scope(r, &case.service, &funding.basis)
        || result.origin != funding.funding_origin
        || case.coverage.profile != funding.basis.profile
        || case.coverage.mandatory_relations != funding.basis.mandatory_relations
        || case.coverage.plan
            != coverage_plan_id(r, funding.basis.profile, funding.basis.derivation, funding.basis.mandatory_relations)?
    {
        return Err(ContractError::Identity);
    }
    r.authenticate(&result.miner_key, &result.miner_signature)?;
    funding.consent.authenticate(
        r,
        &case.service,
        &funding.basis,
        result.origin.accepted_daa,
        &funding.payer_key,
        &funding.payer_signature,
    )?;
    Ok(())
}

/// A persisted Matched cursor alone cannot supply the A1 arithmetic capability.
/// This checks signed identity/plan/attempt and cache links, not arithmetic.
pub fn validate_witness_bundle(
    result: &ResultRow,
    case: &CaseRow,
    witness: &WitnessRow,
    coverage: &CoverageRow,
) -> Result<(), ContractError> {
    NativeRowBody::Witness(witness.clone()).validate_shape()?;
    NativeRowBody::Coverage(*coverage).validate_shape()?;
    let r = &result.result;
    let w = &witness.witness;
    w.authenticate(r, &witness.panel_key, &witness.panel_signature)?;
    if case.case != r.case_id()
        || case.result != r.id()
        || w.attempt >= case.service.budget.witness_attempts
        || w.plan != case.coverage.plan
        || w.profile != case.coverage.profile
        || coverage.witness != w.id()
        || coverage.case != w.case
        || coverage.result != w.result
        || coverage.plan != w.plan
        || coverage.mandatory_relations != case.coverage.mandatory_relations
        || coverage.elements > case.coverage.reserved_elements
        || coverage.terms > case.coverage.reserved_terms
        || (case.positive_witness == Some(w.id())
            && case.status == ClaimStatus::PositivePassed
            && coverage.state != CoverageState::Matched)
    {
        return Err(ContractError::Identity);
    }
    Ok(())
}

impl NativeRowValue {
    pub fn new(body: NativeRowBody) -> Result<Self, ContractError> {
        body.validate_shape()?;
        Ok(Self { version: CONTRACT_VERSION, revision: CONTRACT_REVISION, body })
    }
    pub fn encode(&self) -> Result<Vec<u8>, ContractError> {
        self.validate()?;
        let bytes = borsh::to_vec(self).map_err(|_| ContractError::NonCanonical)?;
        if bytes.len() > MAX_NATIVE_ROW_BYTES {
            return Err(ContractError::Budget);
        }
        Ok(bytes)
    }
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.version != CONTRACT_VERSION || self.revision != CONTRACT_REVISION {
            return Err(ContractError::Version);
        }
        self.body.validate_shape()
    }
    pub fn decode(key: &[u8], bytes: &[u8]) -> Result<Self, ContractError> {
        if key.len() > MAX_NATIVE_KEY_BYTES || bytes.len() > MAX_NATIVE_ROW_BYTES {
            return Err(ContractError::Budget);
        }
        let (version, revision, parsed_key): (u16, u16, NativeRowKey) =
            borsh::from_slice(key).map_err(|_| ContractError::NonCanonical)?;
        if version != CONTRACT_VERSION || revision != CONTRACT_REVISION {
            return Err(ContractError::Version);
        }
        let row: Self = borsh::from_slice(bytes).map_err(|_| ContractError::NonCanonical)?;
        row.validate()?;
        if row.body.key() != parsed_key {
            return Err(ContractError::Identity);
        }
        Ok(row)
    }
}
