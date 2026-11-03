//! RFC-0045 fixed aux151 schemas. These values are data, never native authority.
use super::{consents::*, role_actions::*, *};
use crate::{
    native::ruleset_id,
    rows::{AcceptedMaterial, AcceptedOrigin, BalanceLeg},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum AssignmentStatusV2 {
    Open,
    Consumed { case: CaseId, result: ResultId },
    Cancelled,
    ExpiredUnconsumed,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentRow {
    pub bundle: AssignmentAdmissionBundleV2,
    pub source_consent: AssignmentSourceConsentV2,
    pub prover_consent: AssignmentProverConsentV2,
    pub origin: AcceptedOrigin,
    pub worker_key: [u8; PUBKEY_BYTES],
    pub positive_policy: Digest,
    pub assignment_policy: Digest,
    pub fired_rules: Digest,
    pub job: Digest,
    pub input: Digest,
    pub context: Digest,
    pub registered_metadata: Digest,
    pub status: AssignmentStatusV2,
    pub derivation: AcceptedMaterial,
    pub dependencies: AcceptedMaterial,
    pub reservation: AcceptedMaterial,
    pub liability: AcceptedMaterial,
    pub last_event: AcceptedOrigin,
    /// Exact full frozen admission material, retained in the distinct journal
    /// Open wrapper. Same original accepted origin; never a current Params DTO.
    pub admission_material: AcceptedMaterial,
}
impl AssignmentRow {
    pub fn id(&self) -> AssignmentId {
        self.bundle.terms.id()
    }
    pub fn validate_shape(&self) -> Result<(), ContractError> {
        self.bundle.validate_shape()?;
        self.source_consent.check(&self.bundle)?;
        self.prover_consent.check(&self.bundle, self.positive_policy)?;
        let t = &self.bundle.terms;
        if t.ruleset != ruleset_id(self.fired_rules, self.positive_policy)
            || self.context != self.bundle.job_context
            || self.dependencies.digest != t.empty_dependencies_id()
            || self.origin.accepted_daa >= t.accept_before_daa
            || self.origin.accepted_daa > self.last_event.accepted_daa
            || self.admission_material.origin != self.origin
            || [self.job, self.input, self.registered_metadata, self.assignment_policy].contains(&Digest([0; 64]))
        {
            return Err(ContractError::Identity);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentNonceRow {
    pub poster: NativeOutpoint,
    pub nonce: AssignmentNonceV2,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentUseRow {
    pub assignment: AssignmentId,
    pub case: CaseId,
    pub result: ResultId,
    pub origin: AcceptedOrigin,
    pub binding: AcceptedMaterial,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentFundingRow {
    pub assignment: AssignmentId,
    pub authorization: AssignmentFundingAuthorizationV2,
    pub payer_key: [u8; PUBKEY_BYTES],
    /// The initial poster-purpose signature over the full original bundle.
    pub payer_signature: [u8; SIGNATURE_BYTES],
    pub funding_origin: AcceptedOrigin,
    pub original_outpoint: NativeOutpoint,
    pub current_outpoint: Option<NativeOutpoint>,
    pub initial_locked: u64,
    pub worker: BalanceLeg,
    pub fee: BalanceLeg,
    pub capital: BalanceLeg,
    pub surplus: BalanceLeg,
    pub worker_reserve: u64,
    pub earned_service_due: u64,
    pub liability_floor: u64,
    pub reservation: AcceptedMaterial,
    pub liability: AcceptedMaterial,
    pub last_settlement: Option<AcceptedMaterial>,
}
impl AssignmentFundingRow {
    pub fn remaining(&self) -> Result<u64, ContractError> {
        [self.worker.remaining, self.fee.remaining, self.capital.remaining, self.surplus.remaining]
            .into_iter()
            .try_fold(0u64, |a, b| a.checked_add(b).ok_or(ContractError::Budget))
    }
    pub fn validate_shape(&self) -> Result<(), ContractError> {
        for leg in [&self.worker, &self.fee, &self.capital, &self.surplus] {
            leg.validate()?;
        }
        let initial = [self.worker.initial, self.fee.initial, self.capital.initial, self.surplus.initial]
            .into_iter()
            .try_fold(0u64, |a, b| a.checked_add(b).ok_or(ContractError::Budget))?;
        let remaining = self.remaining()?;
        // A continuation output must come from its literal accepted spend.
        // Without such a settlement this is still the original unspent lock;
        // integer balances cannot move it onto an arbitrary existing UTXO.
        let legs = [self.worker, self.fee, self.capital, self.surplus];
        validate_funding_cursor_v2(self.original_outpoint, self.current_outpoint, self.funding_origin, self.last_settlement, &legs)?;

        let floor = self
            .worker_reserve
            .checked_add(self.earned_service_due)
            .and_then(|n| n.checked_add(self.liability_floor))
            .ok_or(ContractError::Budget)?;
        if self.assignment != self.authorization.assignment
            || principal(&self.payer_key) != self.authorization.payer
            || self.original_outpoint.transaction != self.funding_origin.carrier_transaction
            || self.original_outpoint.index != self.authorization.funding_output_index
            || initial != self.initial_locked
            || initial == 0
            || initial > self.authorization.max_locked
            || self.worker.initial > self.authorization.max_reward
            || self.fee.initial > self.authorization.max_fee
            || self.capital.initial > self.authorization.max_capital
            || floor > remaining
            || self.worker_reserve > self.worker.remaining
            || self.earned_service_due > self.fee.remaining
            || self.liability_floor > self.capital.remaining
            || (remaining > 0) != self.current_outpoint.is_some()
        {
            return Err(ContractError::Budget);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum AssignmentSettlementLegV2 {
    WorkerPayment,
    ServiceFee,
    CapitalReturn,
    Refund,
    Burn,
}
/// Value-only account discriminator. The already-full 142-byte key uses an
/// ordinal unique across ALL accounts for each assignment/leg/party tuple.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum AssignmentSettlementFundingV2 {
    Poster,
    Prover(NativeOutpoint),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentSettlementRow {
    pub assignment: AssignmentId,
    pub funding: AssignmentSettlementFundingV2,
    pub leg: AssignmentSettlementLegV2,
    pub party: PrincipalId,
    pub ordinal: u64,
    pub debit: NativeOutpoint,
    pub recipient: Digest,
    pub amount: u64,
    pub receipt: AcceptedMaterial,
    pub once: AcceptedMaterial,
    pub origin: AcceptedOrigin,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[allow(clippy::large_enum_variant)]
pub enum AssignmentRoleAuthorizationV2 {
    Initial,
    Joined(AssignmentProverJoinV2),
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentRoleRow {
    pub assignment: AssignmentId,
    pub role: AssignmentRoleV2,
    pub bond: NativeOutpoint,
    pub key: [u8; PUBKEY_BYTES],
    pub signature: [u8; SIGNATURE_BYTES],
    pub bundle: Digest,
    pub authorization: AssignmentRoleAuthorizationV2,
    pub admission: AcceptedMaterial,
    pub funding: Option<AcceptedMaterial>,
    pub lease: AcceptedMaterial,
    pub origin: AcceptedOrigin,
    pub retired: bool,
}
impl AssignmentRoleRow {
    pub fn authenticate(&self, assignment: &AssignmentRow) -> Result<(), ContractError> {
        if self.assignment != assignment.id() || self.bundle != assignment.bundle.id() {
            return Err(ContractError::Identity);
        }
        match &self.authorization {
            AssignmentRoleAuthorizationV2::Initial => {
                let bond = match self.role {
                    AssignmentRoleV2::Poster => assignment.bundle.terms.poster,
                    AssignmentRoleV2::Worker => assignment.bundle.terms.worker,
                    AssignmentRoleV2::Prover => assignment.bundle.terms.prover,
                };
                if bond != self.bond || self.origin != assignment.origin {
                    return Err(ContractError::Identity);
                }
                if self.role == AssignmentRoleV2::Worker && self.key != assignment.worker_key {
                    return Err(ContractError::Identity);
                }
                assignment.bundle.authenticate_role(self.role, &self.key, &self.signature)
            }
            AssignmentRoleAuthorizationV2::Joined(join) => {
                if self.role != AssignmentRoleV2::Prover || join.prover != self.bond || principal(&self.key) != join.payer {
                    return Err(ContractError::Identity);
                }
                join.authenticate(&assignment.bundle, &self.key, &self.signature, self.origin.accepted_daa)
            }
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentJournalHeadRow {
    pub assignment: AssignmentId,
    pub next_ordinal: u64,
    pub last_frame: Digest,
    pub origin: AcceptedOrigin,
    pub retention_until_daa: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentJournalFrameRow {
    pub header: super::journal::AssignmentJournalHeaderV2,
    pub payload: AcceptedMaterial,
}

/// Separate op13 payer account. Kind18 is required because key13 deliberately
/// names only the initial poster account. One join/bond/assignment cannot reset
/// its identity by changing a consent digest or buying another global budget.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentProverFundingRow {
    pub assignment: AssignmentId,
    pub consent: AssignmentProverJoinV2,
    pub payer_key: [u8; PUBKEY_BYTES],
    pub payer_signature: [u8; SIGNATURE_BYTES],
    pub funding_origin: AcceptedOrigin,
    pub original_outpoint: NativeOutpoint,
    pub current_outpoint: Option<NativeOutpoint>,
    pub initial_locked: u64,
    pub fee: BalanceLeg,
    pub capital: BalanceLeg,
    pub surplus: BalanceLeg,
    pub earned_service_due: u64,
    pub liability_floor: u64,
    pub reservation: AcceptedMaterial,
    pub liability: AcceptedMaterial,
    pub last_settlement: Option<AcceptedMaterial>,
}
impl AssignmentProverFundingRow {
    pub fn remaining(&self) -> Result<u64, ContractError> {
        self.fee
            .remaining
            .checked_add(self.capital.remaining)
            .and_then(|n| n.checked_add(self.surplus.remaining))
            .ok_or(ContractError::Budget)
    }
    pub fn validate_shape(&self) -> Result<(), ContractError> {
        for leg in [&self.fee, &self.capital, &self.surplus] {
            leg.validate()?;
        }
        let initial = self
            .fee
            .initial
            .checked_add(self.capital.initial)
            .and_then(|n| n.checked_add(self.surplus.initial))
            .ok_or(ContractError::Budget)?;
        let remaining = self.remaining()?;
        // A continuation output must come from its literal accepted spend.
        // Without such a settlement this is still the original unspent lock;
        // integer balances cannot move it onto an arbitrary existing UTXO.
        let legs = [self.fee, self.capital, self.surplus];
        validate_funding_cursor_v2(self.original_outpoint, self.current_outpoint, self.funding_origin, self.last_settlement, &legs)?;

        if self.assignment != self.consent.assignment
            || principal(&self.payer_key) != self.consent.payer
            || self.original_outpoint.transaction != self.funding_origin.carrier_transaction
            || self.original_outpoint.index != self.consent.funding_output_index
            || initial != self.initial_locked
            || initial == 0
            || initial > self.consent.max_locked
            || self.fee.initial > self.consent.max_fee
            || self.capital.initial > self.consent.max_capital
            || self.earned_service_due > self.fee.remaining
            || self.liability_floor > self.capital.remaining
            || self.earned_service_due.checked_add(self.liability_floor).is_none_or(|n| n > remaining)
            || (remaining > 0) != self.current_outpoint.is_some()
        {
            return Err(ContractError::Budget);
        }
        Ok(())
    }
}

fn validate_funding_cursor_v2(
    original: NativeOutpoint,
    current: Option<NativeOutpoint>,
    origin: AcceptedOrigin,
    settlement: Option<AcceptedMaterial>,
    legs: &[BalanceLeg],
) -> Result<(), ContractError> {
    let moved = legs.iter().any(|l| l.paid != 0 || l.refunded != 0 || l.burned != 0);
    match settlement {
        None if !moved && current == Some(original) => Ok(()),
        Some(receipt)
            if moved
                && receipt.origin.accepted_daa >= origin.accepted_daa
                && current.is_none_or(|p| p.transaction == receipt.origin.carrier_transaction) =>
        {
            Ok(())
        }
        _ => Err(ContractError::Budget),
    }
}
