//! RFC-0045 native money qualification. Assignment funds are a separate purpose
//! from TFO1 and from the fresh, output-dependent Case funds. These calculations
//! never replace the ordinary transaction scripts or an atomic native writer.
use crate::{
    Hash64,
    tx::{Transaction, TransactionOutpoint, UtxoEntry},
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_positive_contract::{
    ContractError,
    assignment::rows::AssignmentFundingRow,
    assignment::{AssignmentAdmissionBundleV2, AssignmentTermsV2, policy::AssignmentPolicyV2},
    identity::{Digest, PUBKEY_BYTES, principal},
    native::KERNEL_AUX_TABLE,
    rows::{AcceptedMaterial, BalanceLeg, NativeHash64, NativeOutpoint},
    rows::{NativeRowBody, NativeRowValue},
};
use std::collections::HashMap;

fn hash<T: BorshSerialize>(domain: &[u8], value: &T) -> Digest {
    struct DigestWriter(blake2b_simd::State);
    impl std::io::Write for DigestWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = DigestWriter(blake2b_simd::Params::new().hash_length(64).key(domain).to_state());
    value.serialize(&mut writer).expect("fixed Assignment money preimage serialization");
    Digest(writer.0.finalize().as_bytes().try_into().expect("64-byte purpose digest"))
}

pub type AssignmentFundingLocksV2 = HashMap<TransactionOutpoint, (AssignmentFundingRow, Option<AssignmentSpendAuthorizationV2>)>;

pub fn assignment_prover_funding_locks_v2(
    state: &crate::palw_state_v2::PalwChainStateV2,
) -> Result<
    HashMap<TransactionOutpoint, misaka_palw_positive_contract::assignment::rows::AssignmentProverFundingRow>,
    AssignmentFundsErrorV2,
> {
    let mut locks = HashMap::new();
    let case_locks = positive_case_funding_locks_v2(state)?;
    let Some(route) = state.kernel_route_state_v1() else { return Ok(locks) };
    for ((table, key), bytes) in &route.aux {
        if *table != KERNEL_AUX_TABLE {
            continue;
        }
        let row = NativeRowValue::decode(key, bytes)?;
        let NativeRowBody::AssignmentProverFunding(funding) = row.body else { continue };
        funding.validate_shape()?;
        if let Some(point) = funding.current_outpoint {
            let point = native_assignment_outpoint_v2(point);
            if case_locks.contains_key(&point)
                || state.funded_escrows_v1().values().any(|old| old.settled_by.is_none() && old.funding_outpoint == point)
                || locks.insert(point, funding).is_some()
            {
                return Err(AssignmentFundsErrorV2::DuplicateLock);
            }
        }
    }
    Ok(locks)
}

/// Purpose C stays distinct: a fresh r1 funding row is never an Assignment A
/// authorization and never an ordinary payer output after native admission.
pub fn positive_case_funding_locks_v2(
    state: &crate::palw_state_v2::PalwChainStateV2,
) -> Result<HashMap<TransactionOutpoint, misaka_palw_positive_contract::rows::FundingRow>, AssignmentFundsErrorV2> {
    let mut locks = HashMap::new();
    let Some(route) = state.kernel_route_state_v1() else { return Ok(locks) };
    for ((table, key), bytes) in &route.aux {
        if *table != KERNEL_AUX_TABLE {
            continue;
        }
        let row = NativeRowValue::decode(key, bytes)?;
        let NativeRowBody::Funding(funding) = row.body else { continue };
        if let Some(point) = funding.current_outpoint {
            let point = native_assignment_outpoint_v2(point);
            if state.funded_escrows_v1().values().any(|old| old.settled_by.is_none() && old.funding_outpoint == point)
                || locks.insert(point, funding).is_some()
            {
                return Err(AssignmentFundsErrorV2::DuplicateLock);
            }
        }
    }
    Ok(locks)
}

/// Rooted lock projection; no positive acknowledgement is inferred from the
/// row. Entitlements are absent until the actual native service/P owner derives
/// them. Corrupt/colliding rows are errors, never an empty permissive map.
pub fn assignment_funding_locks_v2(
    state: &crate::palw_state_v2::PalwChainStateV2,
) -> Result<AssignmentFundingLocksV2, AssignmentFundsErrorV2> {
    let mut locks = HashMap::new();
    let case_locks = positive_case_funding_locks_v2(state)?;
    let prover_locks = assignment_prover_funding_locks_v2(state)?;
    let Some(route) = state.kernel_route_state_v1() else { return Ok(locks) };
    for ((table, key), bytes) in &route.aux {
        if *table != KERNEL_AUX_TABLE {
            continue;
        }
        let row = NativeRowValue::decode(key, bytes)?;
        let NativeRowBody::AssignmentFunding(funding) = row.body else { continue };
        funding.validate_shape()?;
        if let Some(point) = funding.current_outpoint {
            let point = native_assignment_outpoint_v2(point);
            if case_locks.contains_key(&point)
                || prover_locks.contains_key(&point)
                || state.funded_escrows_v1().values().any(|old| old.settled_by.is_none() && old.funding_outpoint == point)
                || locks.insert(point, (funding, None)).is_some()
            {
                return Err(AssignmentFundsErrorV2::DuplicateLock);
            }
        }
    }
    Ok(locks)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssignmentFundsErrorV2 {
    Contract(ContractError),
    Payer,
    Expired,
    Output,
    CurrentUtxo,
    Conservation,
    DuplicateLock,
    Input,
    Payload,
    Due,
    ProtectedFloor,
    NoMovement,
    Schedule,
    PhysicalProfile,
}

/// Full purpose-specific price preimage, committed by the adopted Assignment
/// policy. Values are sompi per named unit; no preset supplies any value. A
/// private activation must explicitly bind a complete finite schedule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentFeeScheduleV2 {
    pub version: u16,
    pub revision: u16,
    pub admission: u64,
    pub per_signature: u64,
    pub per_admission_byte: u64,
    pub per_retained_byte_daa: u64,
    pub per_replay_byte: u64,
    pub per_utxo_write: u64,
    pub capital_per_retained_byte: u64,
}
impl AssignmentFeeScheduleV2 {
    pub fn id(&self) -> Digest {
        hash(b"misaka/positive/assignment-fee-schedule/v2/r1", self)
    }
    pub fn reserve_id(&self) -> Digest {
        hash(b"misaka/positive/assignment-capital-schedule/v2/r1", self)
    }
    pub fn validate(&self) -> Result<(), AssignmentFundsErrorV2> {
        if self.version != 2
            || self.revision != 1
            || [
                self.admission,
                self.per_signature,
                self.per_admission_byte,
                self.per_retained_byte_daa,
                self.per_replay_byte,
                self.per_utxo_write,
                self.capital_per_retained_byte,
            ]
            .into_iter()
            .any(|value| value == 0 || value == u64::MAX)
        {
            return Err(AssignmentFundsErrorV2::Schedule);
        }
        Ok(())
    }
}

/// Physical envelope preimage, distinct from its actual finite native leases.
/// These ceilings never create supply. The native owner must reserve matching
/// real RAM/history/replay/UTXO capacity at the exclusive current prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentPhysicalProfileV2 {
    pub version: u16,
    pub revision: u16,
    pub max_admission_bytes: u64,
    pub max_retained_bytes: u64,
    pub max_replay_bytes: u64,
    pub max_heap_bytes: u64,
    pub max_utxo_writes: u64,
    pub max_signatures: u64,
    pub max_retention_daa: u64,
}
impl AssignmentPhysicalProfileV2 {
    pub fn id(&self) -> Digest {
        hash(b"misaka/positive/assignment-physical-profile/v2/r1", self)
    }
    pub fn validate(&self) -> Result<(), AssignmentFundsErrorV2> {
        if self.version != 2
            || self.revision != 1
            || [
                self.max_admission_bytes,
                self.max_retained_bytes,
                self.max_replay_bytes,
                self.max_heap_bytes,
                self.max_utxo_writes,
                self.max_signatures,
                self.max_retention_daa,
            ]
            .into_iter()
            .any(|value| value == 0 || value == u64::MAX)
        {
            return Err(AssignmentFundsErrorV2::PhysicalProfile);
        }
        Ok(())
    }
}

/// Actual canonical byte/work counts supplied by the native derivation owner,
/// never copied from a peer quote or obtained from payer caps. Retained bytes
/// include all initial and possible bounded role/journal rows; replay includes
/// the full bounded canonical history. No pre-Case source/prover service exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentPhysicalUsageV2 {
    pub admission_bytes: u64,
    pub retained_bytes: u64,
    pub replay_bytes: u64,
    pub heap_bytes: u64,
    pub utxo_writes: u64,
    pub signatures: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentMoneyQuoteV2 {
    pub fee: u64,
    pub capital: u64,
    pub usage: AssignmentPhysicalUsageV2,
    pub retention_daa: u64,
    pub fee_schedule: Digest,
    pub reserve_schedule: Digest,
    pub physical_profile: Digest,
}
impl AssignmentMoneyQuoteV2 {
    pub fn id(&self) -> Digest {
        hash(b"misaka/positive/assignment-money-quote/v2/r1", self)
    }
}

/// Deterministic finite A-purpose quote, independently derived before any
/// funding output qualifies. Full preimages must match the accepted policy.
pub fn quote_assignment_money_v2(
    policy: &AssignmentPolicyV2,
    terms: &AssignmentTermsV2,
    schedule: &AssignmentFeeScheduleV2,
    physical: &AssignmentPhysicalProfileV2,
    usage: AssignmentPhysicalUsageV2,
    now: u64,
) -> Result<AssignmentMoneyQuoteV2, AssignmentFundsErrorV2> {
    policy.check_terms(terms, now)?;
    schedule.validate()?;
    physical.validate()?;
    if schedule.id() != policy.assignment_fee_schedule
        || schedule.reserve_id() != policy.reserve_schedule
        || physical.id() != policy.physical_service_profile
        || schedule.reserve_id() != terms.reserve_schedule
    {
        return Err(AssignmentFundsErrorV2::Schedule);
    }
    let retention_daa = terms.reserve_until_daa.checked_sub(now).ok_or(AssignmentFundsErrorV2::Expired)?;
    if usage.admission_bytes == 0
        || usage.retained_bytes == 0
        || usage.replay_bytes == 0
        || usage.heap_bytes == 0
        || usage.utxo_writes == 0
        || usage.signatures < 3
        || retention_daa == 0
        || usage.admission_bytes > physical.max_admission_bytes
        || usage.retained_bytes > physical.max_retained_bytes.min(policy.max_retained_bytes)
        || usage.replay_bytes > physical.max_replay_bytes.min(policy.max_replay_bytes)
        || usage.heap_bytes > physical.max_heap_bytes.min(policy.max_heap_bytes)
        || usage.utxo_writes > physical.max_utxo_writes
        || usage.signatures > physical.max_signatures
        || retention_daa > physical.max_retention_daa
    {
        return Err(AssignmentFundsErrorV2::PhysicalProfile);
    }
    let mul = |a: u64, b: u64| a.checked_mul(b).ok_or(AssignmentFundsErrorV2::Conservation);
    let fee = add([
        schedule.admission,
        mul(usage.signatures, schedule.per_signature)?,
        mul(usage.admission_bytes, schedule.per_admission_byte)?,
        mul(mul(usage.retained_bytes, retention_daa)?, schedule.per_retained_byte_daa)?,
        mul(usage.replay_bytes, schedule.per_replay_byte)?,
        mul(usage.utxo_writes, schedule.per_utxo_write)?,
    ])?;
    let capital = mul(usage.retained_bytes, schedule.capital_per_retained_byte)?;
    if fee > terms.max_assignment_fee
        || capital > terms.max_assignment_capital
        || add([fee, capital, terms.worker_reward])? > terms.max_assignment_locked
    {
        return Err(AssignmentFundsErrorV2::Contract(ContractError::Budget));
    }
    Ok(AssignmentMoneyQuoteV2 {
        fee,
        capital,
        usage,
        retention_daa,
        fee_schedule: schedule.id(),
        reserve_schedule: schedule.reserve_id(),
        physical_profile: physical.id(),
    })
}
impl From<ContractError> for AssignmentFundsErrorV2 {
    fn from(error: ContractError) -> Self {
        Self::Contract(error)
    }
}

pub fn native_assignment_outpoint_v2(point: NativeOutpoint) -> TransactionOutpoint {
    TransactionOutpoint::new(Hash64::from_bytes(point.transaction.0), point.index)
}
fn point(point: TransactionOutpoint) -> NativeOutpoint {
    NativeOutpoint { transaction: NativeHash64(point.transaction_id.as_bytes()), index: point.index }
}
fn add(values: impl IntoIterator<Item = u64>) -> Result<u64, AssignmentFundsErrorV2> {
    values.into_iter().try_fold(0u64, |a, b| a.checked_add(b)).ok_or(AssignmentFundsErrorV2::Conservation)
}
fn payload(output: &crate::tx::TransactionOutput) -> Option<Digest> {
    crate::mldsa87_primitives::p2pkh_mldsa87_payload(&output.script_public_key).map(|p| Digest(p.as_bytes()))
}

/// Exact actual-output facts. Possession is not an admission capability: the
/// native writer must still own the accepted-prefix generation, verify roles,
/// policy/quote and capacity, reject existing locks, and journal atomically.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssignmentFundingOutputV2 {
    outpoint: NativeOutpoint,
    amount: u64,
    worker: u64,
    fee: u64,
    capital: u64,
    surplus: u64,
}
impl AssignmentFundingOutputV2 {
    pub fn outpoint(&self) -> NativeOutpoint {
        self.outpoint
    }
    pub fn amount(&self) -> u64 {
        self.amount
    }
    pub fn initial_legs(&self) -> [BalanceLeg; 4] {
        [self.worker, self.fee, self.capital, self.surplus].map(|amount| BalanceLeg {
            initial: amount,
            paid: 0,
            refunded: 0,
            burned: 0,
            remaining: amount,
        })
    }
}

/// Reads the carrier's own output and the CURRENT composed UTXO entry for that
/// same outpoint. A spent output, another transaction's entry, another payer,
/// an incorrect native refund script, expired consent and cap overflow refuse.
/// The `current_entry` lookup and accepted-carrier authentication belong to the
/// native exclusive writer; public transaction/entry DTOs alone authorize none.
pub fn qualify_assignment_funding_output_v2(
    bundle: &AssignmentAdmissionBundleV2,
    transaction: &Transaction,
    payer_key: &[u8; PUBKEY_BYTES],
    current_entry: Option<&UtxoEntry>,
    required_fee: u64,
    required_capital: u64,
    now: u64,
) -> Result<AssignmentFundingOutputV2, AssignmentFundsErrorV2> {
    bundle.validate_shape()?;
    if principal(payer_key) != bundle.terms.payer
        || Digest(kaspa_hashes::blake2b_512_address_payload(payer_key).as_bytes()) != bundle.terms.refund_payload
    {
        return Err(AssignmentFundsErrorV2::Payer);
    }
    if now >= bundle.funding.admission_expiry || now >= bundle.terms.accept_before_daa {
        return Err(AssignmentFundsErrorV2::Expired);
    }
    let output = transaction.outputs.get(bundle.funding.funding_output_index as usize).ok_or(AssignmentFundsErrorV2::Output)?;
    if payload(output) != Some(bundle.terms.refund_payload) {
        return Err(AssignmentFundsErrorV2::Output);
    }
    let entry = current_entry.ok_or(AssignmentFundsErrorV2::CurrentUtxo)?;
    if entry.amount != output.value || entry.script_public_key != output.script_public_key || entry.is_coinbase {
        return Err(AssignmentFundsErrorV2::CurrentUtxo);
    }
    bundle.check_quoted_amounts(required_fee, required_capital, output.value)?;
    let allocated = add([bundle.terms.worker_reward, required_fee, required_capital])?;
    let surplus = output.value.checked_sub(allocated).ok_or(AssignmentFundsErrorV2::Conservation)?;
    Ok(AssignmentFundingOutputV2 {
        outpoint: point(TransactionOutpoint::new(transaction.id(), bundle.funding.funding_output_index)),
        amount: output.value,
        worker: bundle.terms.worker_reward,
        fee: required_fee,
        capital: required_capital,
        surplus,
    })
}

/// Additional prover's OWN funded op13 purpose, with no worker-success leg.
/// Current native registration and the literal join signature are mandatory at
/// the native writer; this function qualifies only the actual separate coins.
#[allow(clippy::too_many_arguments)]
pub fn qualify_assignment_prover_funding_output_v2(
    bundle: &AssignmentAdmissionBundleV2,
    join: &misaka_palw_positive_contract::assignment::role_actions::AssignmentProverJoinV2,
    payer_key: &[u8; PUBKEY_BYTES],
    transaction: &Transaction,
    current_entry: Option<&UtxoEntry>,
    required_fee: u64,
    required_capital: u64,
    now: u64,
) -> Result<AssignmentFundingOutputV2, AssignmentFundsErrorV2> {
    join.check(bundle, now)?;
    let refund = Digest(kaspa_hashes::blake2b_512_address_payload(payer_key).as_bytes());
    if principal(payer_key) != join.payer {
        return Err(AssignmentFundsErrorV2::Payer);
    }
    let output = transaction.outputs.get(join.funding_output_index as usize).ok_or(AssignmentFundsErrorV2::Output)?;
    let entry = current_entry.ok_or(AssignmentFundsErrorV2::CurrentUtxo)?;
    if output.value != entry.amount
        || output.script_public_key != entry.script_public_key
        || entry.is_coinbase
        || payload(output) != Some(refund)
    {
        return Err(AssignmentFundsErrorV2::CurrentUtxo);
    }
    let required = add([required_fee, required_capital])?;
    if required_fee > join.max_fee || required_capital > join.max_capital || output.value < required || output.value > join.max_locked
    {
        return Err(AssignmentFundsErrorV2::Contract(ContractError::Budget));
    }
    Ok(AssignmentFundingOutputV2 {
        outpoint: point(TransactionOutpoint::new(transaction.id(), join.funding_output_index)),
        amount: output.value,
        worker: 0,
        fee: required_fee,
        capital: required_capital,
        surplus: output.value - required,
    })
}

/// Fresh output-dependent C funding. This preserves the literal r1 two-field
/// projection and requires BOTH the actual r1 payer consent and A's signed
/// future-C bounds. It never transfers or recounts the existing A outpoint.
#[allow(clippy::too_many_arguments)]
pub fn qualify_assigned_case_funding_output_v2(
    assignment: &misaka_palw_positive_contract::assignment::rows::AssignmentRow,
    assignment_funding: &AssignmentFundingRow,
    result: &misaka_palw_positive_contract::messages::MinerResultCommit,
    service: &misaka_palw_positive_contract::messages::MinerServiceContract,
    basis: &misaka_palw_positive_contract::funding::FundingPlanBasis,
    consent: &misaka_palw_positive_contract::funding::FundingConsent,
    payer_key: &[u8; PUBKEY_BYTES],
    transaction: &Transaction,
    current_entry: Option<&UtxoEntry>,
    now: u64,
) -> Result<AssignmentFundingOutputV2, AssignmentFundsErrorV2> {
    assignment.validate_shape()?;
    assignment_funding.validate_shape()?;
    consent.validate(result, service, basis, now)?;
    let bundle = &assignment.bundle;
    let bounds = bundle.future_case;
    let point = point(TransactionOutpoint::new(transaction.id(), consent.funding_output_index));
    if assignment_funding.assignment != assignment.id()
        || assignment_funding.authorization != bundle.funding
        || consent.payer != bundle.terms.payer
        || consent.payer != bounds.payer
        || principal(payer_key) != consent.payer
        || payer_key != &assignment_funding.payer_key
        || result.miner != principal(&assignment.worker_key)
        || consent.refund_payload != bundle.terms.refund_payload
        || consent.refund_payload != bounds.refund_payload
        || Digest(kaspa_hashes::blake2b_512_address_payload(payer_key).as_bytes()) != consent.refund_payload
        || assignment_funding.current_outpoint == Some(point)
        || assignment_funding.original_outpoint == point
    {
        return Err(AssignmentFundsErrorV2::Payer);
    }
    if now >= bounds.case_admission_before_daa
        || now >= bundle.terms.result_before_daa
        || consent.admission_expiry > bounds.case_admission_before_daa
        || service.deadline > bounds.service_before_daa
    {
        return Err(AssignmentFundsErrorV2::Expired);
    }
    let output = transaction.outputs.get(consent.funding_output_index as usize).ok_or(AssignmentFundsErrorV2::Output)?;
    let entry = current_entry.ok_or(AssignmentFundsErrorV2::CurrentUtxo)?;
    if output.value != entry.amount
        || output.script_public_key != entry.script_public_key
        || entry.is_coinbase
        || payload(output) != Some(consent.refund_payload)
    {
        return Err(AssignmentFundsErrorV2::CurrentUtxo);
    }
    let required = add([basis.required_fee, basis.required_capital])?;
    if basis.required_fee > bounds.max_case_fee
        || basis.required_capital > bounds.max_case_capital
        || consent.max_fees > bounds.max_case_fee
        || consent.max_locked > bounds.max_case_locked
        || output.value < required
        || output.value > consent.max_locked
        || output.value > bounds.max_case_locked
    {
        return Err(AssignmentFundsErrorV2::Contract(ContractError::Budget));
    }
    Ok(AssignmentFundingOutputV2 {
        outpoint: point,
        amount: output.value,
        worker: 0,
        fee: basis.required_fee,
        capital: basis.required_capital,
        surplus: output.value - required,
    })
}

/// One explicit debit from one of the four independently conserved legs.
/// This is an internal money intent, never evidence that a service was earned
/// or WorkerPayment was certified. The root owner resolves those predicates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AssignmentLegDebitV2 {
    pub paid: u64,
    pub burned: u64,
    pub refunded: u64,
}
pub fn debit_assignment_leg_v2(
    before: BalanceLeg,
    debit: AssignmentLegDebitV2,
    protected_after: u64,
) -> Result<BalanceLeg, AssignmentFundsErrorV2> {
    before.validate()?;
    let remaining =
        before.remaining.checked_sub(add([debit.paid, debit.burned, debit.refunded])?).ok_or(AssignmentFundsErrorV2::Conservation)?;
    if remaining < protected_after {
        return Err(AssignmentFundsErrorV2::ProtectedFloor);
    }
    let after = BalanceLeg {
        initial: before.initial,
        paid: add([before.paid, debit.paid])?,
        burned: add([before.burned, debit.burned])?,
        refunded: add([before.refunded, debit.refunded])?,
        remaining,
    };
    after.validate()?;
    Ok(after)
}

/// Native-derived settlement entitlement, separate from the escrow input's
/// ordinary spending authority. Constructible only inside Core, where the
/// state owner must resolve each receipt/P decision and preserve all duties.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssignmentSpendAuthorizationV2 {
    pub(crate) before: AssignmentFundingRow,
    pub(crate) debits: [AssignmentLegDebitV2; 4],
    pub(crate) payees: Vec<(Digest, u64)>,
    pub(crate) protected_after: [u64; 4],
    pub(crate) accepted: AcceptedMaterial,
    /// Exact bounded native op12 carrier payload after the native decoder and
    /// current rooted receipt/P owner have authenticated the subject. Ordinary
    /// scripts bind this payload through their normal transaction sighash.
    pub(crate) payload: Vec<u8>,
}

/// Exact rooted before/after images, recomputed from an ordinary accepted
/// transaction. Native must CAS `old`, reject output collisions, and journal
/// row, receipts and the transaction's UTXO diff in the same checkpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssignmentSpendPlanV2 {
    pub old: AssignmentFundingRow,
    pub new: AssignmentFundingRow,
    pub burned_now: u64,
    pub paid_now: u64,
    pub returned_now: u64,
    pub continued_now: u64,
}

/// Qualifies exactly ONE Assignment escrow at input zero. Extra outputs and
/// fees must be covered by foreign inputs. The block/mempool classifier must
/// reject every second locked input, including locks from another purpose.
/// Exact authenticated op12 payload only; no TFO1 identity is reinterpreted. All ordinary
/// scripts still execute before a native spend fact is committed.
pub fn qualify_assignment_spend_v2(
    row: &AssignmentFundingRow,
    authorization: &AssignmentSpendAuthorizationV2,
    transaction: &Transaction,
    input_amounts: &[u64],
    now: u64,
) -> Result<AssignmentSpendPlanV2, AssignmentFundsErrorV2> {
    row.validate_shape()?;
    if &authorization.before != row || authorization.protected_after[3] != 0 {
        return Err(AssignmentFundsErrorV2::Input);
    }
    if authorization.payload.is_empty() || transaction.payload != authorization.payload {
        return Err(AssignmentFundsErrorV2::Payload);
    }
    let current = native_assignment_outpoint_v2(row.current_outpoint.ok_or(AssignmentFundsErrorV2::Input)?);
    if input_amounts.len() != transaction.inputs.len()
        || transaction.inputs.first().is_none_or(|i| i.previous_outpoint != current)
        || transaction.inputs.iter().filter(|i| i.previous_outpoint == current).count() != 1
        || input_amounts[0] != row.remaining()?
        || now < row.funding_origin.accepted_daa
        || authorization.accepted.origin.accepted_daa != now
        || authorization.accepted.origin.carrier_transaction.0 != transaction.id().as_bytes()
        || authorization.accepted.digest == Digest([0; 64])
    {
        return Err(AssignmentFundsErrorV2::Input);
    }
    let worker = debit_assignment_leg_v2(row.worker, authorization.debits[0], authorization.protected_after[0])?;
    let fee = debit_assignment_leg_v2(row.fee, authorization.debits[1], authorization.protected_after[1])?;
    let capital = debit_assignment_leg_v2(row.capital, authorization.debits[2], authorization.protected_after[2])?;
    let surplus = debit_assignment_leg_v2(row.surplus, authorization.debits[3], authorization.protected_after[3])?;
    let paid_now = add(authorization.debits.iter().map(|d| d.paid))?;
    let burned_now = add(authorization.debits.iter().map(|d| d.burned))?;
    let returned_now = add(authorization.debits.iter().map(|d| d.refunded))?;
    if add([paid_now, burned_now, returned_now])? == 0 {
        return Err(AssignmentFundsErrorV2::NoMovement);
    }
    if add(authorization.payees.iter().map(|(_, amount)| *amount))? != paid_now {
        return Err(AssignmentFundsErrorV2::Due);
    }
    let mut next = 0usize;
    for &(recipient, amount) in &authorization.payees {
        if amount == 0 || transaction.outputs.get(next).is_none_or(|o| o.value != amount || payload(o) != Some(recipient)) {
            return Err(AssignmentFundsErrorV2::Output);
        }
        next += 1;
    }
    if returned_now != 0 {
        if transaction
            .outputs
            .get(next)
            .is_none_or(|o| o.value != returned_now || payload(o) != Some(row.authorization.refund_payload))
        {
            return Err(AssignmentFundsErrorV2::Output);
        }
        next += 1;
    }
    let continued_now = add([worker.remaining, fee.remaining, capital.remaining, surplus.remaining])?;
    let current_outpoint = if continued_now != 0 {
        if transaction
            .outputs
            .get(next)
            .is_none_or(|o| o.value != continued_now || payload(o) != Some(row.authorization.refund_payload))
        {
            return Err(AssignmentFundsErrorV2::Output);
        }
        let output_index = u32::try_from(next).map_err(|_| AssignmentFundsErrorV2::Output)?;
        next += 1;
        Some(point(TransactionOutpoint::new(transaction.id(), output_index)))
    } else {
        None
    };
    if add([paid_now, burned_now, returned_now, continued_now])? != row.remaining()?
        || add(input_amounts.iter().skip(1).copied())? < add(transaction.outputs.iter().skip(next).map(|o| o.value))?
    {
        return Err(AssignmentFundsErrorV2::Conservation);
    }
    let mut new = row.clone();
    new.worker = worker;
    new.fee = fee;
    new.capital = capital;
    new.surplus = surplus;
    new.worker_reserve = authorization.protected_after[0];
    new.earned_service_due = authorization.protected_after[1];
    new.liability_floor = authorization.protected_after[2];
    new.current_outpoint = current_outpoint;
    new.last_settlement = Some(authorization.accepted);
    new.validate_shape()?;
    Ok(AssignmentSpendPlanV2 { old: row.clone(), new, burned_now, paid_now, returned_now, continued_now })
}
