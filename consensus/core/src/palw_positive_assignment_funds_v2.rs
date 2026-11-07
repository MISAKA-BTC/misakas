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
    assignment::{AssignmentAdmissionBundleV2, AssignmentTermsV2, policy::AssignmentPolicyV2},
    assignment::{
        control_material::{ASSIGNMENT_CONTROL_OVERHEAD_BYTES_V2, AssignmentControlViewV2},
        journal::{AssignmentJournalEventKindV2, assignment_payload_digest_v2},
        rows::{AssignmentFundingRow, AssignmentJournalFrameRow},
    },
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
    let mut writer = DigestWriter(
        blake2b_simd::Params::new()
            .hash_length(64)
            .key(domain)
            .to_state(),
    );
    value
        .serialize(&mut writer)
        .expect("fixed Assignment money preimage serialization");
    Digest(
        writer
            .0
            .finalize()
            .as_bytes()
            .try_into()
            .expect("64-byte purpose digest"),
    )
}

pub(crate) fn case_relay_rotation_reservation_v2(
    origin: misaka_palw_positive_contract::rows::AcceptedOrigin,
    case_reservation: AcceptedMaterial,
    pending_reservation: AcceptedMaterial,
) -> AcceptedMaterial {
    let digest = blake2b_simd::Params::new()
        .hash_length(64)
        .key(b"misaka/positive/case-relay-rotation/v2/r1")
        .to_state()
        .update(&case_reservation.digest.0)
        .update(&pending_reservation.digest.0)
        .finalize();
    AcceptedMaterial { origin, digest: Digest(*digest.as_array()) }
}

pub(crate) fn relay_origin_precedes_v2(
    accepted: misaka_palw_positive_contract::rows::AcceptedOrigin,
    origin: misaka_palw_positive_contract::rows::AcceptedOrigin,
) -> bool {
    if accepted.accepting_block == origin.accepting_block {
        accepted.accepted_daa == origin.accepted_daa
            && accepted.accepted_blue_score == origin.accepted_blue_score
            && (accepted.accepted_tx_ordinal, accepted.event_ordinal) < (origin.accepted_tx_ordinal, origin.event_ordinal)
    } else {
        accepted.accepted_daa <= origin.accepted_daa && accepted.accepted_blue_score < origin.accepted_blue_score
    }
}

pub type AssignmentFundingLocksV2 =
    HashMap<TransactionOutpoint, (AssignmentFundingRow, Option<AssignmentSpendAuthorizationV2>)>;

pub fn assignment_prover_funding_locks_v2(
    state: &crate::palw_state_v2::PalwChainStateV2,
) -> Result<
    HashMap<
        TransactionOutpoint,
        misaka_palw_positive_contract::assignment::rows::AssignmentProverFundingRow,
    >,
    AssignmentFundsErrorV2,
> {
    let mut locks = HashMap::new();
    let case_locks = positive_case_funding_locks_v2(state)?;
    let Some(route) = state.kernel_route_state_v1() else {
        return Ok(locks);
    };
    for ((table, key), bytes) in &route.aux {
        if *table != KERNEL_AUX_TABLE {
            continue;
        }
        let row = NativeRowValue::decode(key, bytes)?;
        let NativeRowBody::AssignmentProverFunding(funding) = row.body else {
            continue;
        };
        funding.validate_shape()?;
        if let Some(point) = funding.current_outpoint {
            let point = native_assignment_outpoint_v2(point);
            if case_locks.contains_key(&point)
                || state
                    .funded_escrows_v1()
                    .values()
                    .any(|old| old.settled_by.is_none() && old.funding_outpoint == point)
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
) -> Result<
    HashMap<TransactionOutpoint, misaka_palw_positive_contract::rows::FundingRow>,
    AssignmentFundsErrorV2,
> {
    let mut locks = HashMap::new();
    let Some(route) = state.kernel_route_state_v1() else {
        return Ok(locks);
    };
    for ((table, key), bytes) in &route.aux {
        if *table != KERNEL_AUX_TABLE {
            continue;
        }
        let row = NativeRowValue::decode(key, bytes)?;
        let NativeRowBody::Funding(funding) = row.body else {
            continue;
        };
        if let Some(point) = funding.current_outpoint {
            let point = native_assignment_outpoint_v2(point);
            if state
                .funded_escrows_v1()
                .values()
                .any(|old| old.settled_by.is_none() && old.funding_outpoint == point)
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
    let Some(route) = state.kernel_route_state_v1() else {
        return Ok(locks);
    };
    for ((table, key), bytes) in &route.aux {
        if *table != KERNEL_AUX_TABLE {
            continue;
        }
        let row = NativeRowValue::decode(key, bytes)?;
        let NativeRowBody::AssignmentFunding(funding) = row.body else {
            continue;
        };
        funding.validate_shape()?;
        if let Some(point) = funding.current_outpoint {
            let point = native_assignment_outpoint_v2(point);
            if case_locks.contains_key(&point)
                || prover_locks.contains_key(&point)
                || state
                    .funded_escrows_v1()
                    .values()
                    .any(|old| old.settled_by.is_none() && old.funding_outpoint == point)
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
    // The signed quote must survive movement of the accepting block. Both
    // admission-window constraints imply an immutable earliest eligible DAA.
    // Quote its maximum retained interval; `now` only checks current eligibility.
    // DAA starts at zero, so a lower bound before genesis contributes zero.
    // This reserves a ceiling and is never an earned retention-service receipt.
    let lower_bound = |deadline: u64, window: u64| deadline.checked_sub(window).unwrap_or_default();
    let earliest_admission = lower_bound(terms.accept_before_daa, policy.accept_window_daa).max(
        lower_bound(terms.reserve_until_daa, policy.retention_window_daa),
    );
    let retention_daa = terms
        .reserve_until_daa
        .checked_sub(earliest_admission)
        .ok_or(AssignmentFundsErrorV2::Expired)?;
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
        mul(
            mul(usage.retained_bytes, retention_daa)?,
            schedule.per_retained_byte_daa,
        )?,
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
/// Price the literal op8 and the bounded state and history it may occupy.
/// Policy ceilings are stable over the signed intake window. The actual
/// Open extent and simultaneous allocations must also fit at the native writer.
pub fn quote_native_assignment_open_v2(
    policy: &AssignmentPolicyV2,
    terms: &AssignmentTermsV2,
    schedule: &AssignmentFeeScheduleV2,
    physical: &AssignmentPhysicalProfileV2,
    native_body: &[u8],
    now: u64,
) -> Result<(AssignmentMoneyQuoteV2, u64), AssignmentFundsErrorV2> {
    use crate::palw_positive_court_v2::coordinator::assignment_body::{
        AssignmentBodyLimitsV2, scan_assignment_body_v2,
    };

    let limits = AssignmentBodyLimitsV2::from_policy(policy)
        .map_err(|_| AssignmentFundsErrorV2::PhysicalProfile)?;
    let body = scan_assignment_body_v2(native_body, limits)
        .map_err(|_| AssignmentFundsErrorV2::Payload)?;
    if body.kind != 8 {
        return Err(AssignmentFundsErrorV2::Payload);
    }
    let usage = native_assignment_open_usage_v2(policy, physical, body)?;
    let money = quote_assignment_money_v2(policy, terms, schedule, physical, usage, now)?;
    Ok((money, body.heap))
}

fn native_assignment_open_usage_v2(
    policy: &AssignmentPolicyV2,
    physical: &AssignmentPhysicalProfileV2,
    body: crate::palw_positive_court_v2::coordinator::assignment_body::AssignmentBodySummaryV2,
) -> Result<AssignmentPhysicalUsageV2, AssignmentFundsErrorV2> {
    use misaka_palw_positive_contract::{
        assignment::journal::ASSIGNMENT_JOURNAL_HEADER_BYTES_V2,
        rows::{MAX_NATIVE_KEY_BYTES, MAX_NATIVE_ROW_BYTES},
    };

    let cap = AssignmentFundsErrorV2::PhysicalProfile;
    let header = ASSIGNMENT_JOURNAL_HEADER_BYTES_V2 as u64;
    let minimum_payload = body
        .canonical_bytes
        .checked_add(ASSIGNMENT_CONTROL_OVERHEAD_BYTES_V2 as u64)
        .ok_or(cap)?;
    let minimum_record = minimum_payload.checked_add(header).ok_or(cap)?;
    if minimum_payload > policy.max_frame_bytes {
        return Err(cap);
    }
    // Assignment, permanent nonce, one Result use, funding, journal head,
    // bounded frames and roles, plus a separate funding row per eligible join.
    // Use the enforced maximum key/value extent for each retained row.
    let roles = u64::from(policy.max_role_candidates);
    let joins = roles
        .checked_sub(3)
        .ok_or(cap)?
        .min(u64::from(policy.max_prover_joins));
    let frames = u64::from(policy.max_history_frames);
    let row_bytes = (MAX_NATIVE_KEY_BYTES + MAX_NATIVE_ROW_BYTES) as u64;
    let retained_rows = row_bytes
        .checked_mul(5 + roles + joins + frames)
        .ok_or(cap)?;
    let retained_cap = policy.max_retained_bytes.min(physical.max_retained_bytes);
    let replay_cap = policy.max_replay_bytes.min(physical.max_replay_bytes);
    let maximum_frame = policy.max_frame_bytes.checked_add(header).ok_or(cap)?;
    let maximum_history = frames.checked_mul(maximum_frame).ok_or(cap)?;
    let history_bytes = policy
        .max_history_bytes
        .min(retained_cap.checked_sub(retained_rows).ok_or(cap)?)
        .min(replay_cap / 2)
        .min(maximum_history);
    let heap_bytes = policy.max_heap_bytes.min(physical.max_heap_bytes);
    if history_bytes < minimum_record || body.heap > heap_bytes {
        return Err(cap);
    }
    Ok(AssignmentPhysicalUsageV2 {
        admission_bytes: body.canonical_bytes,
        retained_bytes: retained_rows + history_bytes,
        replay_bytes: 2 * history_bytes,
        heap_bytes,
        // A owns one protected output. Ordinary carrier inputs/outputs and
        // script costs are accounted for by the enclosing UTXO validator.
        utxo_writes: 1,
        signatures: body.signatures,
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
    NativeOutpoint {
        transaction: NativeHash64(point.transaction_id.as_bytes()),
        index: point.index,
    }
}
fn add(values: impl IntoIterator<Item = u64>) -> Result<u64, AssignmentFundsErrorV2> {
    values
        .into_iter()
        .try_fold(0u64, |a, b| a.checked_add(b))
        .ok_or(AssignmentFundsErrorV2::Conservation)
}
fn payload(output: &crate::tx::TransactionOutput) -> Option<Digest> {
    crate::mldsa87_primitives::p2pkh_mldsa87_payload(&output.script_public_key)
        .map(|p| Digest(p.as_bytes()))
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

    /// Initial A account from the already qualified actual output. Its
    /// accepted-prefix writer owns uniqueness, publication and the journal.
    pub(crate) fn initial_account_v2(
        &self,
        row: &misaka_palw_positive_contract::assignment::rows::AssignmentRow,
        payer_key: [u8; PUBKEY_BYTES],
        payer_signature: [u8; misaka_palw_positive_contract::identity::SIGNATURE_BYTES],
    ) -> Result<AssignmentFundingRow, AssignmentFundsErrorV2> {
        let [worker, fee, capital, surplus] = self.initial_legs();
        let account = AssignmentFundingRow {
            assignment: row.id(),
            authorization: row.bundle.funding,
            payer_key,
            payer_signature,
            funding_origin: row.origin,
            original_outpoint: self.outpoint,
            current_outpoint: Some(self.outpoint),
            initial_locked: self.amount,
            worker,
            fee,
            capital,
            surplus,
            worker_reserve: worker.remaining,
            earned_service_due: 0,
            liability_floor: capital.remaining,
            reservation: row.reservation,
            liability: row.liability,
            last_settlement: None,
        };
        account.validate_shape()?;
        Ok(account)
    }
}

/// Check the signed payer and actual composed output before pricing capacity.
/// The returned outpoint is only a lookup key for the shared lock check.
pub(crate) fn check_assignment_funding_carriage_v2(
    bundle: &AssignmentAdmissionBundleV2,
    transaction: &Transaction,
    payer_key: &[u8; PUBKEY_BYTES],
    current_entry: Option<&UtxoEntry>,
    now: u64,
) -> Result<(NativeOutpoint, u64), AssignmentFundsErrorV2> {
    let (outpoint, amount) = check_assignment_funding_literal_v2(bundle, transaction, payer_key, now)?;
    let output = &transaction.outputs[outpoint.index as usize];
    let entry = current_entry.ok_or(AssignmentFundsErrorV2::CurrentUtxo)?;
    if entry.amount != output.value || entry.script_public_key != output.script_public_key || entry.is_coinbase {
        return Err(AssignmentFundsErrorV2::CurrentUtxo);
    }
    Ok((outpoint, amount))
}

fn check_assignment_funding_literal_v2(
    bundle: &AssignmentAdmissionBundleV2,
    transaction: &Transaction,
    payer_key: &[u8; PUBKEY_BYTES],
    now: u64,
) -> Result<(NativeOutpoint, u64), AssignmentFundsErrorV2> {
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
    if output.value < bundle.terms.worker_reward
        || output.value > bundle.funding.max_locked
        || output.value > bundle.terms.max_assignment_locked
    {
        return Err(AssignmentFundsErrorV2::Contract(ContractError::Budget));
    }
    Ok((point(TransactionOutpoint::new(transaction.id(), bundle.funding.funding_output_index)), output.value))
}

pub(crate) fn check_archived_assignment_funding_v2(
    bundle: &AssignmentAdmissionBundleV2,
    funding: &AssignmentFundingRow,
    transaction: &Transaction,
    money: &AssignmentMoneyQuoteV2,
) -> Result<(), AssignmentFundsErrorV2> {
    funding.validate_shape()?;
    if !transaction.id_is_current()
        || transaction.is_coinbase()
        || funding.authorization != bundle.funding
        || money.id() != bundle.quote
    {
        return Err(AssignmentFundsErrorV2::Input);
    }
    let (outpoint, amount) =
        check_assignment_funding_literal_v2(bundle, transaction, &funding.payer_key, funding.funding_origin.accepted_daa)?;
    let split = split_assignment_funding_v2(bundle, outpoint, amount, money.fee, money.capital)?;
    if split.outpoint() != funding.original_outpoint
        || split.amount() != funding.initial_locked
        || split.initial_legs().map(|leg| leg.initial)
            != [funding.worker.initial, funding.fee.initial, funding.capital.initial, funding.surplus.initial]
    {
        return Err(AssignmentFundsErrorV2::Conservation);
    }
    Ok(())
}

fn split_assignment_funding_v2(
    bundle: &AssignmentAdmissionBundleV2,
    outpoint: NativeOutpoint,
    amount: u64,
    required_fee: u64,
    required_capital: u64,
) -> Result<AssignmentFundingOutputV2, AssignmentFundsErrorV2> {
    bundle.check_quoted_amounts(required_fee, required_capital, amount).map_err(AssignmentFundsErrorV2::Contract)?;
    let allocated = add([bundle.terms.worker_reward, required_fee, required_capital])?;
    let surplus = amount.checked_sub(allocated).ok_or(AssignmentFundsErrorV2::Conservation)?;
    Ok(AssignmentFundingOutputV2 { outpoint, amount, worker: bundle.terms.worker_reward, fee: required_fee, capital: required_capital, surplus })
}

/// Exact initial F/R contract match. Native must derive the funding
/// outpoint and amount from the actual carrier and current UTXO, authenticate
/// the pending R archive, and reserve current-slot custody before binding.
pub fn check_initial_assignment_relay_target_v2(
    terms: &misaka_palw_positive_contract::assignment::funding_relay::FundingRelayTermsV2,
    bundle: &AssignmentAdmissionBundleV2,
    funding: NativeOutpoint,
    amount: u64,
    now: u64,
) -> Result<(), ContractError> {
    use misaka_palw_positive_contract::assignment::funding_relay::{
        RelayFundingPurposeV2, RelayTargetV2,
    };
    terms.validate_shape()?;
    bundle.validate_shape()?;
    if now >= terms.funding_admission_before_daa
        || now >= bundle.funding.admission_expiry
        || now >= bundle.terms.accept_before_daa
    {
        return Err(ContractError::Range);
    }
    let target = RelayTargetV2::InitialAssignment {
        bundle: bundle.id(),
        poster_nonce: bundle.terms.poster_nonce,
    };
    if terms.funding_purpose != RelayFundingPurposeV2::PosterA
        || terms.target != target
        || terms.assignment != bundle.terms.id()
        || terms.contract_network != bundle.terms.network
        || terms.contract_ruleset != bundle.terms.ruleset
        || terms.contract_anchor != bundle.terms.branch_anchor
        || terms.payer_bond != bundle.terms.poster
        || terms.payer != bundle.terms.payer
        || terms.refund_payload != bundle.terms.refund_payload
        || terms.funding_outpoint != funding
        || funding.index != bundle.funding.funding_output_index
        || terms.expected_amount != amount
    {
        return Err(ContractError::Identity);
    }
    Ok(())
}

/// Checks the already accepted contract at the Case relay's own prefix.
pub fn check_case_relay_registration_assignment_v2(
    terms: &misaka_palw_positive_contract::assignment::funding_relay::FundingRelayTermsV2,
    assignment: &misaka_palw_positive_contract::assignment::rows::AssignmentRow,
    origin: misaka_palw_positive_contract::rows::AcceptedOrigin,
) -> Result<(), ContractError> {
    use misaka_palw_positive_contract::assignment::{
        funding_relay::{RelayFundingPurposeV2, RelayTargetV2},
        rows::AssignmentStatusV2,
    };
    terms.validate_shape()?;
    assignment.validate_shape()?;
    let accepted = assignment.origin;
    let precedes = if accepted.accepting_block == origin.accepting_block {
        accepted.accepted_daa == origin.accepted_daa
            && accepted.accepted_blue_score == origin.accepted_blue_score
            && (accepted.accepted_tx_ordinal, accepted.event_ordinal)
                < (origin.accepted_tx_ordinal, origin.event_ordinal)
    } else {
        accepted.accepted_daa <= origin.accepted_daa
            && accepted.accepted_blue_score < origin.accepted_blue_score
    };
    let bundle = &assignment.bundle;
    let bounds = bundle.future_case;
    if !precedes
        || assignment.status != AssignmentStatusV2::Open
        || origin.accepted_daa >= terms.register_before_daa
        || terms.funding_admission_before_daa > bounds.case_admission_before_daa
    {
        return Err(ContractError::Range);
    }
    if terms.funding_purpose != RelayFundingPurposeV2::CaseC
        || !matches!(terms.target, RelayTargetV2::InitialCase { .. })
        || terms.assignment != assignment.id()
        || terms.contract_network != bundle.terms.network
        || terms.contract_ruleset != bundle.terms.ruleset
        || terms.contract_anchor != bundle.terms.branch_anchor
        || terms.payer_bond != bundle.terms.poster
        || terms.payer != bounds.payer
        || terms.refund_payload != bounds.refund_payload
    {
        return Err(ContractError::Identity);
    }
    if terms.expected_amount > bounds.max_case_locked {
        return Err(ContractError::Budget);
    }
    Ok(())
}

/// The previous Case cursor already exists at the relay registration prefix.
/// The proposed successor output is deliberately absent until its own spend.
pub fn check_case_relay_continuation_account_v2(
    terms: &misaka_palw_positive_contract::assignment::funding_relay::FundingRelayTermsV2,
    assignment: &misaka_palw_positive_contract::assignment::rows::AssignmentRow,
    funding: &misaka_palw_positive_contract::rows::FundingRow,
    origin: misaka_palw_positive_contract::rows::AcceptedOrigin,
) -> Result<(), ContractError> {
    use misaka_palw_positive_contract::{
        assignment::{
            funding_relay::{RelayFundingPurposeV2, RelayTargetV2},
            rows::AssignmentStatusV2,
        },
        identity::principal,
    };
    terms.validate_shape()?;
    assignment.validate_shape()?;
    let AssignmentStatusV2::Consumed { case, .. } = assignment.status else {
        return Err(ContractError::Range);
    };
    let RelayTargetV2::Continuation {
        original_funding,
        predecessor,
        original_contract,
        ..
    } = terms.target
    else {
        return Err(ContractError::Identity);
    };
    let accepted = funding
        .last_settlement
        .map_or(funding.funding_origin, |material| material.origin);
    let precedes = if accepted.accepting_block == origin.accepting_block {
        accepted.accepted_daa == origin.accepted_daa
            && accepted.accepted_blue_score == origin.accepted_blue_score
            && (accepted.accepted_tx_ordinal, accepted.event_ordinal)
                < (origin.accepted_tx_ordinal, origin.event_ordinal)
    } else {
        accepted.accepted_daa <= origin.accepted_daa
            && accepted.accepted_blue_score < origin.accepted_blue_score
    };
    if !precedes || origin.accepted_daa >= terms.register_before_daa {
        return Err(ContractError::Range);
    }
    let signed = &assignment.bundle.terms;
    if terms.funding_purpose != RelayFundingPurposeV2::CaseC
        || terms.assignment != assignment.id()
        || terms.contract_network != signed.network
        || terms.contract_ruleset != signed.ruleset
        || terms.contract_anchor != signed.branch_anchor
        || terms.payer_bond != signed.poster
        || terms.payer != signed.payer
        || terms.payer != funding.consent.payer
        || terms.payer != principal(&funding.payer_key)
        || terms.refund_payload != signed.refund_payload
        || terms.refund_payload != funding.consent.refund_payload
        || funding.consent.case != case
        || original_funding != funding.original_outpoint
        || Some(predecessor) != funding.current_outpoint
        || original_contract != funding.consent.id().0
    {
        return Err(ContractError::Identity);
    }
    let remaining = [
        funding.fee.remaining,
        funding.capital.remaining,
        funding.surplus.remaining,
    ]
    .into_iter()
    .try_fold(0u64, u64::checked_add)
    .ok_or(ContractError::Budget)?;
    if terms.expected_amount > remaining {
        return Err(ContractError::Budget);
    }
    Ok(())
}

/// Checks a pending C registration against the original accepted Assignment
/// and the signed Case consent. The writer must qualify the real coin and
/// authenticate the archived registration before it can bind either row.
pub fn check_initial_case_relay_target_v2(
    terms: &misaka_palw_positive_contract::assignment::funding_relay::FundingRelayTermsV2,
    bundle: &AssignmentAdmissionBundleV2,
    result: misaka_palw_positive_contract::identity::ResultId,
    consent: &misaka_palw_positive_contract::funding::FundingConsent,
    funding: NativeOutpoint,
    amount: u64,
    now: u64,
) -> Result<(), ContractError> {
    use misaka_palw_positive_contract::assignment::funding_relay::{
        RelayFundingPurposeV2, RelayTargetV2,
    };
    terms.validate_shape()?;
    let bounds = bundle.future_case;
    if now >= terms.funding_admission_before_daa
        || now >= consent.admission_expiry
        || now >= bounds.case_admission_before_daa
        || now >= bundle.terms.result_before_daa
    {
        return Err(ContractError::Range);
    }
    let target = RelayTargetV2::InitialCase {
        result,
        funding_scope: consent.scope,
        consent: consent.id(),
    };
    if terms.funding_purpose != RelayFundingPurposeV2::CaseC
        || terms.target != target
        || terms.assignment != bundle.terms.id()
        || terms.assignment != bounds.assignment
        || terms.contract_network != bundle.terms.network
        || terms.contract_ruleset != bundle.terms.ruleset
        || terms.contract_anchor != bundle.terms.branch_anchor
        || terms.payer_bond != bundle.terms.poster
        || terms.payer != bundle.terms.payer
        || terms.payer != bounds.payer
        || terms.payer != consent.payer
        || terms.refund_payload != bundle.terms.refund_payload
        || terms.refund_payload != bounds.refund_payload
        || terms.refund_payload != consent.refund_payload
        || terms.funding_outpoint != funding
        || funding.index != consent.funding_output_index
        || terms.expected_amount != amount
    {
        return Err(ContractError::Identity);
    }
    if amount > consent.max_locked || amount > bounds.max_case_locked {
        return Err(ContractError::Budget);
    }
    Ok(())
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
    let (outpoint, amount) =
        check_assignment_funding_carriage_v2(bundle, transaction, payer_key, current_entry, now)?;
    bundle.check_quoted_amounts(required_fee, required_capital, amount)?;
    let allocated = add([bundle.terms.worker_reward, required_fee, required_capital])?;
    let surplus = amount
        .checked_sub(allocated)
        .ok_or(AssignmentFundsErrorV2::Conservation)?;
    Ok(AssignmentFundingOutputV2 {
        outpoint,
        amount,
        worker: bundle.terms.worker_reward,
        fee: required_fee,
        capital: required_capital,
        surplus,
    })
}

/// Validate a joined prover's own composed coin before deriving the priced
/// capacity requirement. This creates no lock or qualifying admission.
pub(crate) fn check_assignment_prover_funding_carriage_v2(
    bundle: &AssignmentAdmissionBundleV2,
    join: &misaka_palw_positive_contract::assignment::role_actions::AssignmentProverJoinV2,
    payer_key: &[u8; PUBKEY_BYTES],
    transaction: &Transaction,
    current_entry: Option<&UtxoEntry>,
    now: u64,
) -> Result<(NativeOutpoint, u64), AssignmentFundsErrorV2> {
    join.check(bundle, now)?;
    let refund = Digest(kaspa_hashes::blake2b_512_address_payload(payer_key).as_bytes());
    if principal(payer_key) != join.payer {
        return Err(AssignmentFundsErrorV2::Payer);
    }
    let output = transaction
        .outputs
        .get(join.funding_output_index as usize)
        .ok_or(AssignmentFundsErrorV2::Output)?;
    let entry = current_entry.ok_or(AssignmentFundsErrorV2::CurrentUtxo)?;
    if output.value != entry.amount
        || output.script_public_key != entry.script_public_key
        || entry.is_coinbase
        || payload(output) != Some(refund)
    {
        return Err(AssignmentFundsErrorV2::CurrentUtxo);
    }
    Ok((
        point(TransactionOutpoint::new(
            transaction.id(),
            join.funding_output_index,
        )),
        output.value,
    ))
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
    let (outpoint, amount) = check_assignment_prover_funding_carriage_v2(
        bundle,
        join,
        payer_key,
        transaction,
        current_entry,
        now,
    )?;
    let required = add([required_fee, required_capital])?;
    if required_fee > join.max_fee
        || required_capital > join.max_capital
        || amount < required
        || amount > join.max_locked
    {
        return Err(AssignmentFundsErrorV2::Contract(ContractError::Budget));
    }
    Ok(AssignmentFundingOutputV2 {
        outpoint,
        amount,
        worker: 0,
        fee: required_fee,
        capital: required_capital,
        surplus: amount - required,
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
    let qualified = qualify_accepted_case_funding_output_v2(
        assignment,
        assignment_funding,
        result,
        service,
        basis,
        consent,
        payer_key,
        transaction,
        now,
    )?;
    let output = &transaction.outputs[consent.funding_output_index as usize];
    let entry = current_entry.ok_or(AssignmentFundsErrorV2::CurrentUtxo)?;
    if output.value != entry.amount
        || output.script_public_key != entry.script_public_key
        || entry.is_coinbase
    {
        return Err(AssignmentFundsErrorV2::CurrentUtxo);
    }
    Ok(qualified)
}

/// Common signed output qualification. In cold replay the caller supplies the
/// original transaction from the authenticated accepting prefix. Live intake
/// additionally compares the current UTXO through its own entry point.
#[allow(clippy::too_many_arguments)]
pub(crate) fn qualify_accepted_case_funding_output_v2(
    assignment: &misaka_palw_positive_contract::assignment::rows::AssignmentRow,
    assignment_funding: &AssignmentFundingRow,
    result: &misaka_palw_positive_contract::messages::MinerResultCommit,
    service: &misaka_palw_positive_contract::messages::MinerServiceContract,
    basis: &misaka_palw_positive_contract::funding::FundingPlanBasis,
    consent: &misaka_palw_positive_contract::funding::FundingConsent,
    payer_key: &[u8; PUBKEY_BYTES],
    transaction: &Transaction,
    now: u64,
) -> Result<AssignmentFundingOutputV2, AssignmentFundsErrorV2> {
    assignment.validate_shape()?;
    assignment_funding.validate_shape()?;
    consent.validate(result, service, basis, now)?;
    let bundle = &assignment.bundle;
    let bounds = bundle.future_case;
    let point = point(TransactionOutpoint::new(
        transaction.id(),
        consent.funding_output_index,
    ));
    if assignment_funding.assignment != assignment.id()
        || assignment_funding.authorization != bundle.funding
        || consent.payer != bundle.terms.payer
        || consent.payer != bounds.payer
        || principal(payer_key) != consent.payer
        || payer_key != &assignment_funding.payer_key
        || result.miner != principal(&assignment.worker_key)
        || consent.refund_payload != bundle.terms.refund_payload
        || consent.refund_payload != bounds.refund_payload
        || Digest(kaspa_hashes::blake2b_512_address_payload(payer_key).as_bytes())
            != consent.refund_payload
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
    let output = transaction
        .outputs
        .get(consent.funding_output_index as usize)
        .ok_or(AssignmentFundsErrorV2::Output)?;
    if payload(output) != Some(consent.refund_payload) {
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
    let remaining = before
        .remaining
        .checked_sub(add([debit.paid, debit.burned, debit.refunded])?)
        .ok_or(AssignmentFundsErrorV2::Conservation)?;
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

/// A rooted native entitlement selected by the actual P/service/liability
/// session. Fields are Core-private: this is never a public success DTO.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssignmentAuthorizedDueV2 {
    pub(crate) source_leg: u8,
    pub(crate) kind: misaka_palw_positive_contract::assignment::rows::AssignmentSettlementLegV2,
    pub(crate) party: misaka_palw_positive_contract::identity::PrincipalId,
    pub(crate) recipient: Digest,
    pub(crate) amount: u64,
    pub(crate) receipt: AcceptedMaterial,
}
impl AssignmentAuthorizedDueV2 {
    pub(crate) fn debit(&self) -> Result<AssignmentLegDebitV2, AssignmentFundsErrorV2> {
        use misaka_palw_positive_contract::assignment::rows::AssignmentSettlementLegV2 as L;
        if self.amount == 0 || self.source_leg >= 4 || self.receipt.digest == Digest([0; 64]) {
            return Err(AssignmentFundsErrorV2::Due);
        }
        match self.kind {
            L::WorkerPayment if self.source_leg == 0 => Ok(AssignmentLegDebitV2 {
                paid: self.amount,
                ..Default::default()
            }),
            L::ServiceFee if self.source_leg == 1 => Ok(AssignmentLegDebitV2 {
                paid: self.amount,
                ..Default::default()
            }),
            L::CapitalReturn if self.source_leg == 2 => Ok(AssignmentLegDebitV2 {
                refunded: self.amount,
                ..Default::default()
            }),
            L::Refund if self.source_leg != 2 => Ok(AssignmentLegDebitV2 {
                refunded: self.amount,
                ..Default::default()
            }),
            L::Burn if self.recipient == Digest([0; 64]) => Ok(AssignmentLegDebitV2 {
                burned: self.amount,
                ..Default::default()
            }),
            _ => Err(AssignmentFundsErrorV2::Due),
        }
    }
}

/// Tentative entitlement has NO accepting origin. Native derives it for the
/// exact candidate and current rooted generation. Mempool/template previews do
/// not invent accepted material. The final writer repeats that derivation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssignmentSpendAuthorizationV2 {
    pub(crate) before: AssignmentFundingRow,
    pub(crate) dues: Vec<AssignmentAuthorizedDueV2>,
    pub(crate) protected_after: [u64; 4],
    pub(crate) daa: u64,
    pub(crate) max_frame_bytes: u64,
    pub(crate) max_history_frames: u32,
    pub(crate) payload: Vec<u8>,
}

/// Monetary projection only, deliberately not a serializable FundingRow. It
/// cannot pass row restore or acquire a fake last_settlement before acceptance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssignmentFundingProjectionV2 {
    pub worker: BalanceLeg,
    pub fee: BalanceLeg,
    pub capital: BalanceLeg,
    pub surplus: BalanceLeg,
    pub worker_reserve: u64,
    pub earned_service_due: u64,
    pub liability_floor: u64,
    pub current_outpoint: Option<NativeOutpoint>,
}

/// Exact tentative money plan. Its private authorization prevents construction
/// from arbitrary before/after row DTOs; final commit still requalifies against
/// the current native session and actual accepted transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssignmentSpendPlanV2 {
    pub old: AssignmentFundingRow,
    pub new: AssignmentFundingProjectionV2,
    pub transaction: crate::TransactionId,
    pub daa: u64,
    pub body_digest: Digest,
    pub burned_now: u64,
    pub paid_now: u64,
    pub returned_now: u64,
    pub continued_now: u64,
    pub(crate) authorization: AssignmentSpendAuthorizationV2,
}
impl AssignmentSpendPlanV2 {
    /// Rebind a preview to the actual populated inputs before admitting the spend.
    pub fn matches_populated_spend_v2(
        &self,
        row: &AssignmentFundingRow,
        transaction: &Transaction,
        amounts: &[u64],
        daa: u64,
    ) -> bool {
        &self.old == row
            && self.transaction == transaction.id()
            && self.daa == daa
            && qualify_assignment_spend_v2(row, &self.authorization, transaction, amounts, daa)
                .is_ok_and(|checked| checked == *self)
    }

    pub fn matches_cooperative_refund_v2(&self, row: &AssignmentFundingRow, tx: &Transaction, amounts: &[u64], daa: u64) -> bool {
        self.burned_now == 0
            && self.paid_now == 0
            && qualify_assignment_spend_v2(row, &self.authorization, tx, amounts, daa).is_ok_and(|expected| expected == *self)
    }

    /// Called only after native accepted-prefix provenance has supplied the
    /// actual op12 material. A tentative preview cannot manufacture this row.
    pub(crate) fn accepted_funding_row_v2(
        &self,
        frame: &AssignmentJournalFrameRow,
        control_payload: &[u8],
    ) -> Result<AssignmentFundingRow, AssignmentFundsErrorV2> {
        let header = &frame.header;
        let accepted = frame.payload;
        if header.kind != AssignmentJournalEventKindV2::Settlement
            || header.assignment != self.old.assignment
            || accepted.origin != header.origin
            || accepted.digest != header.payload_digest
            || accepted.origin.carrier_transaction.0 != self.transaction.as_bytes()
            || accepted.origin.accepted_daa != self.daa
        {
            return Err(AssignmentFundsErrorV2::Input);
        }
        header.check_payload(control_payload, self.authorization.max_frame_bytes)?;
        let control =
            AssignmentControlViewV2::decode(control_payload, self.authorization.max_frame_bytes)?;
        if Some(control.inner()) != self.authorization.payload.get(3..)
            || assignment_payload_digest_v2(control.inner()) != self.body_digest
        {
            return Err(AssignmentFundsErrorV2::Payload);
        }
        let mut new = self.old.clone();
        new.worker = self.new.worker;
        new.fee = self.new.fee;
        new.capital = self.new.capital;
        new.surplus = self.new.surplus;
        new.worker_reserve = self.new.worker_reserve;
        new.earned_service_due = self.new.earned_service_due;
        new.liability_floor = self.new.liability_floor;
        new.current_outpoint = self.new.current_outpoint;
        new.last_settlement = Some(accepted);
        new.validate_shape()?;
        Ok(new)
    }
}

/// Qualifies exactly one current native Assignment A input at index zero.
/// Native authorizes exact op12 receipts, not arbitrary payload bytes. Ordinary
/// scripts remain mandatory. All fees/change outside this account use foreign
/// inputs and the shared spend classifier rejects any second locked input.
pub(crate) fn qualify_unconsumed_assignment_refund_v2(
    funding: AssignmentFundingRow,
    receipt: AcceptedMaterial,
    tx: &Transaction,
    amounts: &[u64],
    now: u64,
    max_frame_bytes: u64,
    max_history_frames: u32,
) -> Result<AssignmentSpendPlanV2, AssignmentFundsErrorV2> {
    use misaka_palw_positive_contract::assignment::rows::AssignmentSettlementLegV2;
    let dues = [(0, funding.worker.remaining), (3, funding.surplus.remaining)]
        .into_iter()
        .filter(|&(_, amount)| amount != 0)
        .map(|(source_leg, amount)| AssignmentAuthorizedDueV2 {
            source_leg,
            kind: AssignmentSettlementLegV2::Refund,
            party: funding.authorization.payer,
            recipient: funding.authorization.refund_payload,
            amount,
            receipt,
        })
        .collect();
    let authorization = AssignmentSpendAuthorizationV2 {
        protected_after: [0, funding.earned_service_due, funding.liability_floor, 0],
        before: funding,
        dues,
        daa: now,
        max_frame_bytes,
        max_history_frames,
        payload: tx.payload.clone(),
    };
    qualify_assignment_spend_v2(&authorization.before, &authorization, tx, amounts, now)
}

pub fn qualify_assignment_spend_v2(
    row: &AssignmentFundingRow,
    authorization: &AssignmentSpendAuthorizationV2,
    transaction: &Transaction,
    input_amounts: &[u64],
    now: u64,
) -> Result<AssignmentSpendPlanV2, AssignmentFundsErrorV2> {
    use misaka_palw_positive_contract::assignment::rows::AssignmentSettlementLegV2 as L;
    row.validate_shape()?;
    if &authorization.before != row
        || authorization.protected_after[3] != 0
        || authorization.daa != now
        || authorization.max_frame_bytes == 0
        || authorization.max_frame_bytes == u64::MAX
        || authorization.max_history_frames == 0
        || authorization.max_history_frames == u32::MAX
        || authorization.dues.len() as u64 > u64::from(authorization.max_history_frames)
        || authorization
            .payload
            .len()
            .checked_sub(3)
            .and_then(|bytes| bytes.checked_add(ASSIGNMENT_CONTROL_OVERHEAD_BYTES_V2))
            .is_none_or(|bytes| {
                bytes as u64 > authorization.max_frame_bytes || bytes as u64 > u64::from(u32::MAX)
            })
    {
        return Err(AssignmentFundsErrorV2::Input);
    }
    if authorization.payload.is_empty()
        || transaction.payload != authorization.payload
        || !transaction.id_is_current()
        || transaction.subnetwork_id != crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE
        || !crate::palw_lifecycle_objects_v2::palw_positive_lifecycle_payload_v2(
            &transaction.payload,
        )
        || transaction.payload.get(7) != Some(&12)
    {
        return Err(AssignmentFundsErrorV2::Payload);
    }
    let current =
        native_assignment_outpoint_v2(row.current_outpoint.ok_or(AssignmentFundsErrorV2::Input)?);
    if input_amounts.len() != transaction.inputs.len()
        || transaction
            .inputs
            .first()
            .is_none_or(|i| i.previous_outpoint != current)
        || transaction
            .inputs
            .iter()
            .filter(|i| i.previous_outpoint == current)
            .count()
            != 1
        || input_amounts[0] != row.remaining()?
        || now < row.funding_origin.accepted_daa
    {
        return Err(AssignmentFundsErrorV2::Input);
    }
    let mut debits = [AssignmentLegDebitV2::default(); 4];
    let mut paid_now = 0u64;
    let mut burned_now = 0u64;
    let mut returned_now = 0u64;
    let mut next = 0usize;
    let mut seen = std::collections::BTreeSet::new();
    for due in &authorization.dues {
        if due.kind == L::WorkerPayment
            && (due.amount != row.worker.initial
                || row.worker.paid != 0
                || row.worker.refunded != 0
                || row.worker.burned != 0)
        {
            return Err(AssignmentFundsErrorV2::Due);
        }
        let debit = due.debit()?;
        if due.receipt.origin.accepted_daa > now
            || !seen.insert((
                due.source_leg,
                due.kind as u8,
                due.receipt.digest.0,
                due.party.0.0,
            ))
        {
            return Err(AssignmentFundsErrorV2::Due);
        }
        let leg = &mut debits[due.source_leg as usize];
        leg.paid = add([leg.paid, debit.paid])?;
        leg.burned = add([leg.burned, debit.burned])?;
        leg.refunded = add([leg.refunded, debit.refunded])?;
        match due.kind {
            L::WorkerPayment | L::ServiceFee => {
                if transaction
                    .outputs
                    .get(next)
                    .is_none_or(|o| o.value != due.amount || payload(o) != Some(due.recipient))
                {
                    return Err(AssignmentFundsErrorV2::Output);
                }
                next += 1;
                paid_now = add([paid_now, due.amount])?;
            }
            L::Burn => {
                burned_now = add([burned_now, due.amount])?;
            }
            L::CapitalReturn | L::Refund => {
                if due.party != row.authorization.payer
                    || due.recipient != row.authorization.refund_payload
                {
                    return Err(AssignmentFundsErrorV2::Due);
                }
                returned_now = add([returned_now, due.amount])?;
            }
        }
    }
    if add([paid_now, burned_now, returned_now])? == 0 {
        return Err(AssignmentFundsErrorV2::NoMovement);
    }
    let worker = debit_assignment_leg_v2(row.worker, debits[0], authorization.protected_after[0])?;
    let fee = debit_assignment_leg_v2(row.fee, debits[1], authorization.protected_after[1])?;
    let capital =
        debit_assignment_leg_v2(row.capital, debits[2], authorization.protected_after[2])?;
    let surplus =
        debit_assignment_leg_v2(row.surplus, debits[3], authorization.protected_after[3])?;
    if returned_now != 0 {
        if transaction.outputs.get(next).is_none_or(|o| {
            o.value != returned_now || payload(o) != Some(row.authorization.refund_payload)
        }) {
            return Err(AssignmentFundsErrorV2::Output);
        }
        next += 1;
    }
    let continued_now = add([
        worker.remaining,
        fee.remaining,
        capital.remaining,
        surplus.remaining,
    ])?;
    let current_outpoint = if continued_now != 0 {
        if transaction.outputs.get(next).is_none_or(|o| {
            o.value != continued_now || payload(o) != Some(row.authorization.refund_payload)
        }) {
            return Err(AssignmentFundsErrorV2::Output);
        }
        let index = u32::try_from(next).map_err(|_| AssignmentFundsErrorV2::Output)?;
        next += 1;
        Some(point(TransactionOutpoint::new(transaction.id(), index)))
    } else {
        None
    };
    if add([paid_now, burned_now, returned_now, continued_now])? != row.remaining()?
        || add(input_amounts.iter().skip(1).copied())?
            < add(transaction.outputs.iter().skip(next).map(|o| o.value))?
    {
        return Err(AssignmentFundsErrorV2::Conservation);
    }
    let projection = AssignmentFundingProjectionV2 {
        worker,
        fee,
        capital,
        surplus,
        worker_reserve: authorization.protected_after[0],
        earned_service_due: authorization.protected_after[1],
        liability_floor: authorization.protected_after[2],
        current_outpoint,
    };
    let body_digest = assignment_payload_digest_v2(&transaction.payload[3..]);
    Ok(AssignmentSpendPlanV2 {
        old: row.clone(),
        new: projection,
        transaction: transaction.id(),
        daa: now,
        body_digest,
        burned_now,
        paid_now,
        returned_now,
        continued_now,
        authorization: authorization.clone(),
    })
}

#[cfg(test)]
#[path = "palw_positive_assignment_funds_v2/tentative_tests.rs"]
pub(crate) mod tentative_tests;
