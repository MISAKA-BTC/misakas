//! RFC-0051 carrier framing and ephemeral UTXO fee handoff. A prepared
//! carrier can neither install a session nor claim a registration charge.

use crate::{
    Hash64, TransactionId,
    mldsa87_primitives::MLDSA87_SIGNATURE_LEN,
    palw_court_carrier_fee_v1::CarrierFeeBudgetV1,
    palw_court_service_v1::ServiceCost,
    palw_court_shared_tir_stream_v1::stream_digest,
    palw_native_resource_v1::{
        NativeCoverageV1, NativeDecodeErrorV1, NativeRetainedV1, TirNativeRequestsV1, btree_node_envelope_bytes_v1,
        reserve_native_pending_requests_v1,
    },
    palw_state_v2::PalwBlockContextV2,
    palw_tokenizer_registration_state_v1::{TokenizerRegistrationSessionV1, TokenizerRegistrationStatusV1},
    palw_tokenizer_registration_v1::{
        EOG_SIBLINGS_MAX, TOKENIZER_REGISTRATION_PART_BYTES_V1, TOKENIZER_REGISTRATION_VERSION_V1, TokenizerRegistrationFinishBodyV1,
        TokenizerRegistrationOpenBodyV1, TokenizerRegistrationPartBodyV1, TokenizerRegistrationPolicyV1, TokenizerRegistrationQuoteV1,
        TokenizerRegistrationWireV1,
    },
    palw_tokenizer_rendering_v2::ROOTED_TOKENIZER_LIMITS_V1,
    tx::{Transaction, ValidatedTransaction},
};
use borsh::BorshDeserialize;
use std::sync::Arc;

pub const TOKENIZER_REGISTRATION_CARRIER_MAGIC_V1: &[u8; 4] = b"TKR1";

fn body(tx: &Transaction) -> Option<&[u8]> {
    if tx.subnetwork_id != crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE {
        return None;
    }
    tx.payload.strip_prefix(TOKENIZER_REGISTRATION_CARRIER_MAGIC_V1)
}

pub fn encode_tokenizer_registration_payload_v1(wire: &TokenizerRegistrationWireV1) -> Result<Vec<u8>, String> {
    use TokenizerRegistrationWireV1::{Finish, Open, Part};
    match wire {
        Open(control) => control.signing_message_v1()?,
        Part(control) => control.signing_message_v1()?,
        Finish(control) => control.signing_message_v1()?,
    };
    let mut payload = TOKENIZER_REGISTRATION_CARRIER_MAGIC_V1.to_vec();
    borsh::to_writer(&mut payload, wire).map_err(|why| why.to_string())?;
    Ok(payload)
}

fn charged_carrier_bytes(payload_len: usize, policy: &TokenizerRegistrationPolicyV1) -> Result<u64, &'static str> {
    let prefix = TOKENIZER_REGISTRATION_CARRIER_MAGIC_V1.len() as u64;
    if policy.carrier_overhead_bytes < prefix {
        return Err("TKR1 policy omits carrier framing bytes");
    }
    (payload_len as u64)
        .checked_sub(prefix)
        .and_then(|len| len.checked_add(policy.carrier_overhead_bytes))
        .ok_or("TKR1 carrier byte count overflow")
}

/// Allocation-free header-context shape check. Authority, the payload digest,
/// EOF and the EOG opening remain native checks under the work scanner.
pub(crate) fn tokenizer_registration_payload_shape_v1(
    payload: &[u8],
    policy: &TokenizerRegistrationPolicyV1,
) -> Result<bool, &'static str> {
    const INVALID: &str = "malformed TKR1 carrier shape";
    let Some(body) = payload.strip_prefix(TOKENIZER_REGISTRATION_CARRIER_MAGIC_V1) else {
        return Ok(false);
    };
    if charged_carrier_bytes(payload.len(), policy)? > policy.max_carrier_bytes {
        return Err(INVALID);
    }
    let (&kind, mut fields) = body.split_first().ok_or(INVALID)?;
    let version = match kind {
        0 => TokenizerRegistrationOpenBodyV1::deserialize(&mut fields).map_err(|_| INVALID)?.version,
        1 => TokenizerRegistrationPartBodyV1::deserialize(&mut fields).map_err(|_| INVALID)?.version,
        2 => TokenizerRegistrationFinishBodyV1::deserialize(&mut fields).map_err(|_| INVALID)?.version,
        _ => return Err(INVALID),
    };
    if version != TOKENIZER_REGISTRATION_VERSION_V1 {
        return Err(INVALID);
    }
    let mut suffix = fields.get(MLDSA87_SIGNATURE_LEN..).ok_or(INVALID)?;
    match kind {
        0 => {}
        1 => {
            let len = u32::deserialize(&mut suffix).map_err(|_| INVALID)? as usize;
            if len == 0 || len > TOKENIZER_REGISTRATION_PART_BYTES_V1 || len as u64 > policy.max_raw_bytes || suffix.len() != len {
                return Err(INVALID);
            }
            return Ok(true);
        }
        2 => {
            let ids = u32::deserialize(&mut suffix).map_err(|_| INVALID)? as usize;
            if ids == 0 || ids > policy.max_eog_ids as usize {
                return Err(INVALID);
            }
            // EOG IDs, opening leaf index and hash, then the sibling vector.
            suffix = suffix.get(ids * 4 + 8 + 64..).ok_or(INVALID)?;
            let siblings = u32::deserialize(&mut suffix).map_err(|_| INVALID)? as usize;
            if siblings > EOG_SIBLINGS_MAX || suffix.len() != siblings * 64 {
                return Err(INVALID);
            }
            return Ok(true);
        }
        _ => return Err(INVALID),
    }
    if !suffix.is_empty() {
        return Err(INVALID);
    }
    Ok(true)
}

pub fn decode_tokenizer_registration_payload_v1(
    payload: &[u8],
    policy: &TokenizerRegistrationPolicyV1,
) -> Result<Option<TokenizerRegistrationWireV1>, String> {
    let Some(body) = payload.strip_prefix(TOKENIZER_REGISTRATION_CARRIER_MAGIC_V1) else {
        return Ok(None);
    };
    policy.check_v1()?;
    if charged_carrier_bytes(payload.len(), policy)? > policy.max_carrier_bytes {
        return Err("TKR1 payload exceeds the adopted carrier limit".into());
    }
    borsh::from_slice(body).map(Some).map_err(|why| why.to_string())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokenizerRegistrationCarrierContextV1 {
    /// The exact PALW root of the accepted parent used for native preparation.
    pub parent_root: Hash64,
    pub at: PalwBlockContextV2,
    pub accepted_ordinal: usize,
    pub network: Hash64,
    pub policy: Hash64,
}

/// This is a read-only projection of the fee effect. The collector must
/// consume it at this exact accepting ordinal, only after the block commits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokenizerRegistrationFeeV1 {
    pub carrier: TransactionId,
    pub context: TokenizerRegistrationCarrierContextV1,
    pub actual_fee: u64,
    pub collectible_fee: u64,
    pub registration_burn: u64,
    pub charge_id: Option<Hash64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedTokenizerRegistrationCarrierV1 {
    context: TokenizerRegistrationCarrierContextV1,
    policy: TokenizerRegistrationPolicyV1,
    wire: TokenizerRegistrationWireV1,
    budget: CarrierFeeBudgetV1,
}

crate::palw_native_resource_v1::native_struct!(TokenizerRegistrationCarrierContextV1; parent_root, at, accepted_ordinal, network, policy);
crate::palw_native_resource_v1::native_struct!(TokenizerRegistrationFeeV1; carrier, context, actual_fee, collectible_fee, registration_burn, charge_id);
crate::palw_native_resource_v1::native_struct!(ValidatedTokenizerRegistrationCarrierV1; context, policy, wire, budget);

/// One retained UTXO handoff shared across ordered rehearsal and final fold.
pub type SharedValidatedTokenizerRegistrationCarrierV1 = Arc<NativeRetainedV1<ValidatedTokenizerRegistrationCarrierV1>>;

type CarrierError = NativeDecodeErrorV1<String>;

impl ValidatedTokenizerRegistrationCarrierV1 {
    /// Only the transaction validator may pass this handoff's context and UTXO
    /// view. Prior native fees already charged by the same processor must be
    /// supplied here so they cannot also fund this registration.
    pub fn prepare_v1(
        validated: &ValidatedTransaction<'_>,
        context: TokenizerRegistrationCarrierContextV1,
        policy: &TokenizerRegistrationPolicyV1,
        preceding_fee: u64,
    ) -> Result<Option<NativeRetainedV1<Self>>, CarrierError> {
        Self::prepare_retained_v1(validated, context, policy, preceding_fee, false)
    }

    /// Retain one decoded carrier and one immutable fee budget for both
    /// rehearsal and final fold. Shared index and Arc storage are precharged.
    pub fn prepare_shared_v1(
        validated: &ValidatedTransaction<'_>,
        context: TokenizerRegistrationCarrierContextV1,
        policy: &TokenizerRegistrationPolicyV1,
        preceding_fee: u64,
    ) -> Result<Option<SharedValidatedTokenizerRegistrationCarrierV1>, CarrierError> {
        Ok(Self::prepare_retained_v1(validated, context, policy, preceding_fee, true)?.map(Arc::new))
    }

    fn prepare_retained_v1(
        validated: &ValidatedTransaction<'_>,
        context: TokenizerRegistrationCarrierContextV1,
        policy: &TokenizerRegistrationPolicyV1,
        preceding_fee: u64,
        shared: bool,
    ) -> Result<Option<NativeRetainedV1<Self>>, CarrierError> {
        use NativeDecodeErrorV1::{Decode, Resource};
        let Some(body) = body(validated.tx) else { return Ok(None) };
        if [context.parent_root, context.at.block, context.network].contains(&Hash64::default())
            || context.policy != policy.id_v1().map_err(Decode)?
            || charged_carrier_bytes(validated.tx.payload.len(), policy).map_err(|why| Decode(why.into()))? > policy.max_carrier_bytes
        {
            return Err(Decode("TKR1 changes its native context or carrier bound".into()));
        }
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<NativeRetainedV1<Self>>(1).map_err(Resource)?;
        match body.first() {
            Some(0) => {}
            Some(1) => q.exact_v1::<u8>(TOKENIZER_REGISTRATION_PART_BYTES_V1 as u64).map_err(Resource)?,
            Some(2) => {
                q.exact_v1::<u32>(u64::from(ROOTED_TOKENIZER_LIMITS_V1.eog_ids)).map_err(Resource)?;
                q.exact_v1::<Hash64>(EOG_SIBLINGS_MAX as u64).map_err(Resource)?;
            }
            _ => return Err(Decode("unknown TKR1 action".into())),
        }
        // The immutable budget stores only the preceding debit. The
        // registration charge is projected without another allocation.
        q.add_v1(btree_node_envelope_bytes_v1::<[u8; 64], u64>() as u64).map_err(Resource)?;
        if shared {
            q.add_v1((std::mem::size_of::<[usize; 2]>() + std::mem::align_of::<NativeRetainedV1<Self>>()) as u64).map_err(Resource)?;
            q.add_v1(btree_node_envelope_bytes_v1::<TransactionId, SharedValidatedTokenizerRegistrationCarrierV1>() as u64)
                .map_err(Resource)?;
            // Rehearsal and final fee maps retain one exact projection per carrier.
            for _ in 0..2 {
                q.add_v1(btree_node_envelope_bytes_v1::<TransactionId, TokenizerRegistrationFeeV1>() as u64).map_err(Resource)?;
            }
            // Candidate, accepted and final extras indexes can coexist.
            for _ in 0..3 {
                q.vec_v1::<(usize, SharedValidatedTokenizerRegistrationCarrierV1)>(1).map_err(Resource)?;
            }
        }
        let lease = reserve_native_pending_requests_v1(q.bytes_v1()).map_err(Resource)?;
        let scratch = (validated.tx.inputs.len() as u64)
            .checked_mul(btree_node_envelope_bytes_v1::<(TransactionId, u32), ()>() as u64)
            .ok_or_else(|| Resource(NativeCoverageV1::overflow::<Self>("TKR1 fee input set")))?;
        let _scratch = if scratch == 0 { None } else { reserve_native_pending_requests_v1(scratch).map_err(Resource)? };
        let wire = decode_tokenizer_registration_payload_v1(&validated.tx.payload, policy)
            .map_err(Decode)?
            .ok_or_else(|| Decode("validated TKR1 carrier lost its payload".into()))?;
        use TokenizerRegistrationWireV1::{Finish, Open, Part};
        let (network, policy_id) = match &wire {
            Open(control) => (control.body.network, control.body.policy),
            Part(control) => (control.body.network, control.body.policy),
            Finish(control) => (control.body.network, control.body.policy),
        };
        if (network, policy_id) != (context.network, context.policy) {
            return Err(Decode("TKR1 signed network or policy differs from the accepting context".into()));
        }
        let fee_error = |why| Decode(format!("TKR1 native carrier fee: {why:?}"));
        let mut budget = CarrierFeeBudgetV1::from_validated_v1(validated).map_err(fee_error)?;
        if preceding_fee != 0 {
            let id = stream_digest(b"misaka-palw/tokenizer-registration/preceding-fee/v1", &validated.tx.id()).map_err(Decode)?;
            budget.allocate_v1(id.as_bytes(), preceding_fee).map_err(fee_error)?;
        }
        Ok(Some(NativeRetainedV1::from_owner_reserved_v1(Self { context, policy: *policy, wire, budget }, lease)))
    }

    pub fn context_v1(&self) -> TokenizerRegistrationCarrierContextV1 {
        self.context
    }

    pub fn accepted_ordinal_v1(&self) -> usize {
        self.context.accepted_ordinal
    }

    pub fn carrier_v1(&self) -> TransactionId {
        self.budget.carrier_v1()
    }

    pub fn wire_at_v1(
        &self,
        context: &TokenizerRegistrationCarrierContextV1,
        carrier: TransactionId,
    ) -> Result<&TokenizerRegistrationWireV1, String> {
        if self.context != *context || self.budget.carrier_v1() != carrier {
            return Err("TKR1 handoff changes accepting parent, point, ordinal, network, policy or transaction".into());
        }
        Ok(&self.wire)
    }

    /// Check affordability before trying a native Open. This creates neither
    /// a fee allocation nor a session. The caller must use its native quote,
    /// and atomically roll back any insertion if final fee settlement fails.
    pub(crate) fn check_open_fee_v1(
        &self,
        context: &TokenizerRegistrationCarrierContextV1,
        carrier: TransactionId,
        session: &TokenizerRegistrationSessionV1,
    ) -> Result<(), String> {
        let TokenizerRegistrationWireV1::Open(open) = self.wire_at_v1(context, carrier)? else {
            return Err("TKR1 fee check requires an Open".into());
        };
        let id = open.body.id_v1()?;
        if session.open != *open
            || session.policy != self.policy
            || session.accepted != context.at
            || session.status != TokenizerRegistrationStatusV1::Pending
            || !session.received.is_empty()
            || !session.finish_attempts.is_empty()
        {
            return Err("TKR1 native quote substitutes the exact Open or its acceptance".into());
        }
        session.check(id)?;
        if session.quote.charge > self.budget.remaining_v1() {
            return Err("TKR1 Open has insufficient actual native fee for the registration charge".into());
        }
        Ok(())
    }

    /// Project the fee from the exact native fold result. A caller may pass
    /// only the row read back after a new insertion, never a prepared or
    /// previously existing session. An absent row projects zero service burn.
    /// Rehearsal and final fold can each project the same result. The accepting
    /// fee collector must reconcile and apply it once per carrier position.
    pub(crate) fn project_after_fold_v1(
        &self,
        context: &TokenizerRegistrationCarrierContextV1,
        carrier: TransactionId,
        inserted_open: Option<&TokenizerRegistrationSessionV1>,
    ) -> Result<TokenizerRegistrationFeeV1, String> {
        self.wire_at_v1(context, carrier)?;
        let (charge_id, registration_burn) = if let Some(session) = inserted_open {
            self.check_open_fee_v1(context, carrier, session)?;
            (Some(session.open.body.charge_id_v1()?), session.quote.charge)
        } else {
            (None, 0)
        };
        let collectible_fee = self
            .budget
            .remaining_v1()
            .checked_sub(registration_burn)
            .ok_or("TKR1 Open has insufficient actual native fee for the registration charge")?;
        Ok(TokenizerRegistrationFeeV1 {
            carrier,
            context: *context,
            actual_fee: self.budget.actual_fee_v1(),
            collectible_fee,
            registration_burn,
            charge_id,
        })
    }

    /// Compatibility for native callers that own the entire handoff.
    pub(crate) fn settle_after_fold_v1(
        handoff: NativeRetainedV1<Self>,
        context: &TokenizerRegistrationCarrierContextV1,
        carrier: TransactionId,
        inserted_open: Option<&TokenizerRegistrationSessionV1>,
    ) -> Result<TokenizerRegistrationFeeV1, String> {
        handoff.project_after_fold_v1(context, carrier, inserted_open)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokenizerRegistrationWorkScanV1 {
    pub cost: ServiceCost,
    /// The candidate block scanner allows at most one parser invocation for
    /// this Open's assigned Finish slot. All carrier costs still count.
    pub finish_open_id: Option<Hash64>,
}

/// Charge every recognized carrier to the candidate-block preflight, including
/// duplicates and malformed controls. The caller must pass the authenticated
/// stored quote for a Finish when available. No parser or signature verifier
/// runs in this scanner. Unknown controls receive the full work upper bound
/// before the dispatcher rejects them.
pub fn scan_tokenizer_registration_work_v1(
    tx: &Transaction,
    policy: &TokenizerRegistrationPolicyV1,
    stored_quote: impl FnOnce(Hash64) -> Option<TokenizerRegistrationQuoteV1>,
) -> Result<Option<TokenizerRegistrationWorkScanV1>, String> {
    let Some(body) = body(tx) else { return Ok(None) };
    let policy_id = policy.id_v1()?;
    let bytes = charged_carrier_bytes(tx.payload.len(), policy)?;
    let mut finish_open_id = None;
    let fallback = || -> Result<ServiceCost, &'static str> {
        let budget = policy.effective_reservation_budget_v1()?;
        Ok(ServiceCost { bytes: bytes.max(policy.max_carrier_bytes), work: budget.work, io: budget.io, controls: 1 })
    };
    let cost = match body.first().copied() {
        Some(0 | 1) if bytes <= policy.max_carrier_bytes => {
            let part = body.first() == Some(&1);
            let work = if part { policy.part_work_v1(bytes)? } else { policy.open_work_v1(bytes)? };
            let io = if part {
                // Only the bytes after the Part body, signature and vector
                // prefix can be retained as raw. Charging all carrier bytes
                // here could refuse a short Part already reserved by its Open.
                const PART_NON_RAW: usize = 1 + 2 + 4 * 64 + 68 + 4 + 8 + MLDSA87_SIGNATURE_LEN + 4;
                (TOKENIZER_REGISTRATION_PART_BYTES_V1 as u64)
                    .min(policy.max_raw_bytes)
                    .min(body.len().saturating_sub(PART_NON_RAW) as u64)
                    .checked_mul(policy.retention_copies)
                    .and_then(|n| n.checked_add(policy.metadata_bytes))
                    .ok_or("TKR1 scan part IO overflow")?
            } else {
                policy.metadata_bytes
            };
            ServiceCost { bytes, work, io, controls: 1 }
        }
        Some(2) if bytes <= policy.max_carrier_bytes => {
            let mut fields = &body[1..];
            let cost = TokenizerRegistrationFinishBodyV1::deserialize_reader(&mut fields)
                .ok()
                .filter(|finish| finish.policy == policy_id)
                .and_then(|finish| {
                    finish_open_id = Some(finish.open_id);
                    stored_quote(finish.open_id)
                })
                .map(|quote| quote.finish_cost);
            match cost {
                Some(cost) => ServiceCost { bytes: cost.bytes.max(bytes), ..cost },
                None => fallback()?,
            }
        }
        _ => fallback()?,
    };
    Ok(Some(TokenizerRegistrationWorkScanV1 { cost, finish_open_id }))
}

/// Validate the aggregate candidate-block budget before native TKR1 work.
/// Feed the physical transaction stream, before deduplication or refusal.
/// Charge the post-object sweep once for a parent book or an Open that can create one.
/// Snapshot verification is separately metered at its restoration boundary.
pub fn preflight_tokenizer_registration_candidate_v1<'a>(
    transactions: impl IntoIterator<Item = &'a Transaction>,
    policy: &TokenizerRegistrationPolicyV1,
    parent_has_book: bool,
    mut parent_quote: impl FnMut(Hash64) -> Option<TokenizerRegistrationQuoteV1>,
) -> Result<ServiceCost, String> {
    let budget = policy.service_budget;
    let mut total = ServiceCost::default();
    let mut sweep_charged = parent_has_book;
    if parent_has_book {
        total.work = policy.bookkeeping_work_v1()?.sweep;
        if total.work > budget.work {
            return Err("TKR1 candidate sweep exceeds the work budget".into());
        }
    }
    let add = |used: u64, next: u64, limit: u64| {
        used.checked_add(next).filter(|sum| *sum <= limit).ok_or("TKR1 candidate exceeds its service budget")
    };
    for tx in transactions {
        let Some(scan) = scan_tokenizer_registration_work_v1(tx, policy, &mut parent_quote)? else { continue };
        if !sweep_charged && body(tx).is_some_and(|body| body.first() == Some(&0)) {
            total.work = add(total.work, policy.bookkeeping_work_v1()?.sweep, budget.work)?;
            sweep_charged = true;
        }
        let cost = scan.cost;
        total = ServiceCost {
            bytes: add(total.bytes, cost.bytes, budget.bytes)?,
            work: add(total.work, cost.work, budget.work)?,
            io: add(total.io, cost.io, budget.io)?,
            controls: add(total.controls, cost.controls, budget.controls)?,
        };
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        palw_tokenizer_registration_v1::{TokenizerRegistrationPartBodyV1, TokenizerRegistrationPartV1},
        tx::{PopulatedTransaction, ScriptPublicKey, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry},
    };

    #[test]
    fn candidate_preflight_meters_physical_finish_copies_and_sweeps_once() {
        let (mut policy, mut open) = crate::palw_tokenizer_registration_v1::tests::fixture();
        // Leave headroom for two full Finish projections plus the sweep.
        policy.service_budget.work = 20_000_000;
        open.policy = policy.id_v1().unwrap();
        let quote = policy.quote_v1(&open, 20, 10_000).unwrap();
        let open_id = open.id_v1().unwrap();
        let finish = TokenizerRegistrationFinishBodyV1 {
            version: open.version,
            network: open.network,
            policy: open.policy,
            open_id,
            opener: open.opener,
            eog_digest: open.eog_digest,
            finish_until_daa: open.finish_until_daa,
        };
        // Recognizable Finish with a missing signature/EOG must still count.
        let mut payload = TOKENIZER_REGISTRATION_CARRIER_MAGIC_V1.to_vec();
        payload.push(2);
        payload.extend(borsh::to_vec(&finish).unwrap());
        let tx = Transaction::new(0, vec![], vec![], 0, crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE, 0, payload);
        let mut lookups = 0;
        let total = preflight_tokenizer_registration_candidate_v1([&tx, &tx], &policy, true, |id| {
            assert_eq!(id, open_id);
            lookups += 1;
            Some(quote)
        })
        .unwrap();
        let cost = quote.finish_cost;
        let sweep = policy.bookkeeping_work_v1().unwrap().sweep;
        assert_eq!(lookups, 2);
        assert_eq!(total, ServiceCost { bytes: 2 * cost.bytes, work: 2 * cost.work + sweep, io: 2 * cost.io, controls: 2 });
        assert!(preflight_tokenizer_registration_candidate_v1([&tx, &tx, &tx], &policy, true, |_| Some(quote)).is_err());
        assert_eq!(
            preflight_tokenizer_registration_candidate_v1([], &policy, true, |_| None).unwrap(),
            ServiceCost { work: sweep, ..ServiceCost::default() }
        );
        assert_eq!(preflight_tokenizer_registration_candidate_v1([], &policy, false, |_| None).unwrap(), ServiceCost::default());
        assert!(preflight_tokenizer_registration_candidate_v1([&tx, &tx], &policy, false, |_| None).is_err());
        let open_tx = Transaction::new(
            0,
            vec![],
            vec![],
            0,
            crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            encode_tokenizer_registration_payload_v1(&TokenizerRegistrationWireV1::Open(
                crate::palw_tokenizer_registration_v1::TokenizerRegistrationOpenV1 {
                    body: open,
                    signature: [0; MLDSA87_SIGNATURE_LEN],
                },
            ))
            .unwrap(),
        );
        let open_cost = scan_tokenizer_registration_work_v1(&open_tx, &policy, |_| None).unwrap().unwrap().cost;
        let first_open = preflight_tokenizer_registration_candidate_v1([&open_tx], &policy, false, |_| None).unwrap();
        assert_eq!(first_open.work, open_cost.work + sweep);
        assert_eq!(first_open, preflight_tokenizer_registration_candidate_v1([&open_tx], &policy, true, |_| None).unwrap());
    }

    #[test]
    fn real_fee_context_and_zero_service_burn_for_parts() {
        let h = Hash64::from_u64_word;
        let policy = TokenizerRegistrationPolicyV1 {
            version: 1,
            max_raw_bytes: 65_536,
            max_vocab: 16,
            max_key_events: 128,
            max_eog_ids: 8,
            max_rendering_bytes: 65_536,
            max_parser_scratch_bytes: 9_000_000,
            max_carrier_bytes: 100_000,
            max_retained_bytes: 2_000_000,
            max_retained_total_bytes: 4_000_000,
            service_budget: ServiceCost { bytes: 100_000, work: 2_000_000, io: 2_000_000, controls: 2 },
            max_work: 2_000_000,
            max_pending_per_opener: 1,
            max_pending_total: 2,
            max_session_daa: 100,
            retention_daa: 20,
            carrier_overhead_bytes: 4,
            retention_copies: 2,
            metadata_bytes: 32_000,
            work_rates: [1; 7],
            bookkeeping_work_per_step: 1,
            rent_per_carrier_byte: 1,
            rent_per_retained_byte_daa: 1,
            rent_per_work: 1,
            max_registration_charge: 100_000_000,
        };
        let context = TokenizerRegistrationCarrierContextV1 {
            parent_root: h(1),
            at: PalwBlockContextV2 { block: h(2), daa_score: 20, blue_score: 30, subsidy: 0 },
            accepted_ordinal: 3,
            network: h(4),
            policy: policy.id_v1().unwrap(),
        };
        let prevout = TransactionOutpoint::new(h(5), 1);
        let part = TokenizerRegistrationPartV1 {
            body: TokenizerRegistrationPartBodyV1 {
                version: 1,
                network: context.network,
                policy: context.policy,
                open_id: h(6),
                opener: crate::palw_state_v2::PalwBondKeyV2(prevout),
                index: 0,
                offset: 0,
                payload_digest: TokenizerRegistrationPartV1::payload_digest_v1(&[7]).unwrap(),
            },
            signature: [0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
            bytes: vec![7],
        };
        let wire = TokenizerRegistrationWireV1::Part(part);
        let payload = encode_tokenizer_registration_payload_v1(&wire).unwrap();
        assert_eq!(decode_tokenizer_registration_payload_v1(&payload, &policy).unwrap(), Some(wire.clone()));
        let tx = Transaction::new(
            0,
            vec![TransactionInput::new(prevout, vec![], 0, 0)],
            vec![TransactionOutput::new(80, ScriptPublicKey::default())],
            0,
            crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            payload,
        );
        let validated = |fee| {
            ValidatedTransaction::new(
                PopulatedTransaction::new(&tx, vec![UtxoEntry::new(100, ScriptPublicKey::default(), 1, false)]),
                fee,
            )
        };
        let handoff = ValidatedTokenizerRegistrationCarrierV1::prepare_v1(&validated(20), context, &policy, 3).unwrap().unwrap();
        assert!(ValidatedTokenizerRegistrationCarrierV1::prepare_v1(&validated(21), context, &policy, 0).is_err());
        assert!(handoff.wire_at_v1(&TokenizerRegistrationCarrierContextV1 { accepted_ordinal: 4, ..context }, tx.id()).is_err());
        assert!(handoff.wire_at_v1(&context, h(9)).is_err());
        assert_eq!(handoff.wire_at_v1(&context, tx.id()).unwrap(), &wire);
        let fee = ValidatedTokenizerRegistrationCarrierV1::settle_after_fold_v1(handoff, &context, tx.id(), None).unwrap();
        assert_eq!((fee.actual_fee, fee.collectible_fee, fee.registration_burn, fee.charge_id), (20, 17, 0, None));
        let cost = scan_tokenizer_registration_work_v1(&tx, &policy, |_| None).unwrap().unwrap();
        assert_eq!(cost.cost.work, policy.part_work_v1(cost.cost.bytes).unwrap());
        assert!(cost.finish_open_id.is_none());
        // A second physical copy still incurs preflight work.
        assert_eq!(scan_tokenizer_registration_work_v1(&tx, &policy, |_| None).unwrap(), Some(cost));
    }
    #[test]
    fn new_open_burns_once_and_duplicate_projection_keeps_ordinary_fee() {
        use crate::palw_tokenizer_registration_v1::TokenizerRegistrationOpenV1;
        use std::collections::BTreeSet;
        let h = Hash64::from_u64_word;
        let (policy, body) = crate::palw_tokenizer_registration_v1::tests::fixture();
        let at = PalwBlockContextV2 { block: h(61), daa_score: 20, blue_score: 25, subsidy: 0 };
        let context = TokenizerRegistrationCarrierContextV1 {
            parent_root: h(62),
            at,
            accepted_ordinal: 3,
            network: body.network,
            policy: body.policy,
        };
        let open = TokenizerRegistrationOpenV1 { body, signature: [0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN] };
        // The native fold owns this success signal. The test exercises only
        // the fee projection after the exact row has been inserted.
        let quote = policy.quote_v1(&body, at.daa_score, 10_000).unwrap();
        let inserted = TokenizerRegistrationSessionV1 {
            open: open.clone(),
            policy,
            quote,
            accepted: at,
            key_hash: h(63),
            declared_key_events: 10_000,
            received: BTreeSet::new(),
            finish_attempts: BTreeSet::new(),
            status: TokenizerRegistrationStatusV1::Pending,
        };
        let tx = Transaction::new(
            0,
            vec![TransactionInput::new(TransactionOutpoint::new(h(64), 0), vec![], 0, 0)],
            vec![TransactionOutput::new(80, ScriptPublicKey::default())],
            0,
            crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            encode_tokenizer_registration_payload_v1(&TokenizerRegistrationWireV1::Open(open)).unwrap(),
        );
        let scan = scan_tokenizer_registration_work_v1(&tx, &policy, |_| None).unwrap().unwrap();
        assert_eq!(scan.cost, quote.open_cost);
        let actual_fee = quote.charge + 11;
        let validated = ValidatedTransaction::new(
            PopulatedTransaction::new(&tx, vec![UtxoEntry::new(80 + actual_fee, ScriptPublicKey::default(), 1, false)]),
            actual_fee,
        );
        let handoff = ValidatedTokenizerRegistrationCarrierV1::prepare_v1(&validated, context, &policy, 3).unwrap().unwrap();
        handoff.check_open_fee_v1(&context, tx.id(), &inserted).unwrap();
        let fee = ValidatedTokenizerRegistrationCarrierV1::settle_after_fold_v1(handoff, &context, tx.id(), Some(&inserted)).unwrap();
        assert_eq!(
            (fee.actual_fee, fee.collectible_fee, fee.registration_burn, fee.charge_id),
            (actual_fee, 8, quote.charge, Some(body.charge_id_v1().unwrap()))
        );

        let shared = ValidatedTokenizerRegistrationCarrierV1::prepare_shared_v1(&validated, context, &policy, 3).unwrap().unwrap();
        let rehearsal = Arc::clone(&shared);
        assert!(Arc::ptr_eq(&shared, &rehearsal));
        assert_eq!((shared.context_v1(), shared.accepted_ordinal_v1(), shared.carrier_v1()), (context, 3, tx.id()));
        assert_eq!(rehearsal.project_after_fold_v1(&context, tx.id(), Some(&inserted)), Ok(fee));
        assert_eq!(shared.project_after_fold_v1(&context, tx.id(), Some(&inserted)), Ok(fee));
        assert_eq!(shared.project_after_fold_v1(&context, tx.id(), None).unwrap().registration_burn, 0);
        let wrong_parent = TokenizerRegistrationCarrierContextV1 { parent_root: h(99), ..context };
        assert!(shared.project_after_fold_v1(&wrong_parent, tx.id(), Some(&inserted)).is_err());
        assert!(shared.project_after_fold_v1(&context, h(98), Some(&inserted)).is_err());
        let mut wrong_row = inserted.clone();
        wrong_row.finish_attempts.insert(0);
        assert!(shared.project_after_fold_v1(&context, tx.id(), Some(&wrong_row)).is_err());

        let duplicate = TokenizerRegistrationCarrierContextV1 { accepted_ordinal: 4, ..context };
        let handoff = ValidatedTokenizerRegistrationCarrierV1::prepare_v1(&validated, duplicate, &policy, 3).unwrap().unwrap();
        let fee = ValidatedTokenizerRegistrationCarrierV1::settle_after_fold_v1(handoff, &duplicate, tx.id(), None).unwrap();
        assert_eq!((fee.collectible_fee, fee.registration_burn, fee.charge_id), (actual_fee - 3, 0, None));

        let short = ValidatedTokenizerRegistrationCarrierV1::prepare_v1(&validated, context, &policy, 12).unwrap().unwrap();
        assert!(short.check_open_fee_v1(&context, tx.id(), &inserted).is_err());
        let fee = ValidatedTokenizerRegistrationCarrierV1::settle_after_fold_v1(short, &context, tx.id(), None).unwrap();
        assert_eq!(fee.registration_burn, 0);
    }
}
