//! Current-prefix money derivation and the same native checkpoint's settlement
//! writer. A persisted pass, caller boolean or arbitrary provider trait cannot
//! create the authenticated P/service/liability replay session missing below.
use super::*;
use crate::tx::Transaction;
use crate::{
    palw_positive_assignment_funds_v2::{
        AssignmentAuthorizedDueV2, AssignmentFundsErrorV2, AssignmentSpendAuthorizationV2, AssignmentSpendPlanV2,
        native_assignment_outpoint_v2, qualify_assignment_spend_v2,
    },
    palw_positive_court_v2::coordinator::assignment_body::{
        AssignmentBodyLimitsV2, PositiveAssignmentNativeBodyV2, PositiveAssignmentNativeOperationV2, decode_assignment_body_v2,
    },
    palw_positive_rows_v2::PositiveRowsV2,
    palw_settlement_once_v1::{
        PALW_SETTLEMENT_ONCE_TABLE_V1, PalwSettlementConsumerV1, PalwSettlementDebitV1, PalwSettlementLegV1,
        PalwSettlementObligationV1, PalwSettlementOnceRowV1, palw_settlement_subject_v1,
    },
};
use misaka_palw_positive_contract::{
    assignment::{
        AssignmentId,
        journal::{AssignmentJournalEventKindV2, assignment_frame_v2},
        rows::{
            AssignmentFundingRow, AssignmentSettlementFundingV2, AssignmentSettlementLegV2, AssignmentSettlementRow,
            AssignmentStatusV2,
        },
    },
    identity::Digest,
    native::{KERNEL_AUX_TABLE, NativeRowKey},
    rows::{AcceptedMaterial, AcceptedOrigin, NativeRowBody, NativeRowValue},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssignmentMoneyOwnerErrorV2 {
    Payload,
    CurrentFunding,
    MissingAssignment,
    WrongCase,
    /// Exact native source history, P draw/decision, earned receipts, remaining
    /// duties and liability have not been replayed by their authenticated owner.
    AuthenticatedNativeMoneySessionUnavailable,
    Funds(AssignmentFundsErrorV2),
}
fn refused(why: impl std::fmt::Debug) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(format!("Assignment money: {why:?}"))
}

/// The shared mempool/template/block entrypoint reads a real current-prefix
/// snapshot and exact candidate. It creates no accepted origin and grants no
/// positive authority from stored status/floor rows. The complete native replay
/// session is intentionally unavailable until its actual owner is connected.
pub fn qualify_rooted_assignment_candidate_v2(
    state: &PalwChainStateV2,
    transaction: &Transaction,
    input_amounts: &[u64],
    now: u64,
    limits: AssignmentBodyLimitsV2,
) -> Result<AssignmentSpendPlanV2, AssignmentMoneyOwnerErrorV2> {
    let authorization = derive_rooted_assignment_authorization_v2(state, transaction, input_amounts, now, limits)?;
    qualify_assignment_spend_v2(&authorization.before, &authorization, transaction, input_amounts, now)
        .map_err(AssignmentMoneyOwnerErrorV2::Funds)
}

fn derive_rooted_assignment_authorization_v2(
    state: &PalwChainStateV2,
    transaction: &Transaction,
    input_amounts: &[u64],
    now: u64,
    limits: AssignmentBodyLimitsV2,
) -> Result<AssignmentSpendAuthorizationV2, AssignmentMoneyOwnerErrorV2> {
    if transaction.subnetwork_id != crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE
        || !transaction.id_is_current()
        || !crate::palw_lifecycle_objects_v2::palw_positive_lifecycle_payload_v2(&transaction.payload)
        || transaction.payload.get(..2) != Some(&[1, 0])
    {
        return Err(AssignmentMoneyOwnerErrorV2::Payload);
    }
    let decoded = decode_assignment_body_v2(&transaction.payload[3..], limits).map_err(|_| AssignmentMoneyOwnerErrorV2::Payload)?;
    let PositiveAssignmentNativeOperationV2::SettleAssignment { assignment, case, .. } = decoded.operation else {
        return Err(AssignmentMoneyOwnerErrorV2::Payload);
    };
    let rooted = PositiveRowsV2::new(state).assignment(assignment).map_err(|_| AssignmentMoneyOwnerErrorV2::MissingAssignment)?;
    if case.is_some_and(|case| !matches!(rooted.assignment.status, AssignmentStatusV2::Consumed {case:current,..} if current==case)) {
        return Err(AssignmentMoneyOwnerErrorV2::WrongCase);
    }
    let current = rooted.funding.current_outpoint.ok_or(AssignmentMoneyOwnerErrorV2::CurrentFunding)?;
    if transaction.inputs.first().is_none_or(|i| i.previous_outpoint != native_assignment_outpoint_v2(current))
        || transaction.inputs.len() != input_amounts.len()
        || input_amounts.first().copied() != Some(rooted.funding.remaining().map_err(|_| AssignmentMoneyOwnerErrorV2::CurrentFunding)?)
        || now < rooted.funding.funding_origin.accepted_daa
    {
        return Err(AssignmentMoneyOwnerErrorV2::CurrentFunding);
    }
    // Required future connection: resolve the ORIGINAL accepted policy/material
    // and native P/source/liability sessions at this exact prefix. Readiness
    // does not follow from PositivePassed, saved floors, zero pending counters,
    // arbitrary AcceptedMaterial refs or a user-implemented trait.
    Err(AssignmentMoneyOwnerErrorV2::AuthenticatedNativeMoneySessionUnavailable)
}

fn source_leg(row: &AssignmentFundingRow, leg: u8) -> Result<misaka_palw_positive_contract::rows::BalanceLeg, PalwStateV2Error> {
    [row.worker, row.fee, row.capital, row.surplus].get(leg as usize).copied().ok_or_else(|| refused("unknown source leg"))
}

/// Service/worker/burn identity is independent of which funding account pays
/// it. Refund tranches instead bind the original actual coin and previous
/// refunded quantity. No caller-chosen settlement-row ordinal resets the once.
fn obligation(row: &AssignmentFundingRow, due: &AssignmentAuthorizedDueV2) -> Result<PalwSettlementObligationV1, PalwStateV2Error> {
    use AssignmentSettlementLegV2 as L;
    let (leg, subject, ordinal) = match due.kind {
        L::WorkerPayment => (
            PalwSettlementLegV1::AssignmentWorkerPayment,
            palw_settlement_subject_v1(b"assignment-worker/v2", &[&row.assignment.0.0]),
            0,
        ),
        L::ServiceFee => (
            PalwSettlementLegV1::AssignmentServiceFee,
            palw_settlement_subject_v1(b"assignment-service/v2", &[&row.assignment.0.0, &due.receipt.digest.0]),
            0,
        ),
        L::Burn => (
            PalwSettlementLegV1::AssignmentContractualBurn,
            palw_settlement_subject_v1(b"assignment-burn/v2", &[&row.assignment.0.0, &due.receipt.digest.0]),
            0,
        ),
        L::CapitalReturn | L::Refund => {
            let index = row.original_outpoint.index.to_le_bytes();
            let source = [due.source_leg];
            let subject = palw_settlement_subject_v1(
                b"assignment-return/v2",
                &[&row.assignment.0.0, &row.original_outpoint.transaction.0, &index, &source],
            );
            let leg = if due.kind == L::CapitalReturn {
                PalwSettlementLegV1::AssignmentCapitalReturn
            } else {
                PalwSettlementLegV1::AssignmentRefund
            };
            (leg, subject, source_leg(row, due.source_leg)?.refunded)
        }
    };
    Ok(PalwSettlementObligationV1::with_party(leg, subject, due.party.0.0, ordinal))
}

impl TransitionBuilder<'_> {
    /// Called inside the raw op12 interleaving checkpoint after ordinary native
    /// UTXO/script validation. The raw input is rescanned and requalified using
    /// this builder's CURRENT state; a previous preview is never consumed as a
    /// receipt. No standalone payout queue or minted balance is created.
    pub(super) fn settle_positive_assignment_raw_funds_v2(
        &mut self,
        ctx: &PalwBlockContextV2,
        action: &PositiveAssignmentRawActionV2,
        body: &PositiveAssignmentNativeBodyV2,
    ) -> Result<(), PalwStateV2Error> {
        let tx = action.transaction();
        let origin = action.origin();
        if origin.accepting_block.0 != ctx.block.as_bytes()
            || origin.accepted_daa != ctx.daa_score
            || origin.accepted_blue_score != ctx.blue_score
            || origin.carrier_transaction.0 != tx.id().as_bytes()
        {
            return Err(refused("settlement does not have the actual accepted native origin"));
        }
        let limits = action.rules().body_limits().map_err(refused)?;
        let literal_body = tx.payload.get(3..).ok_or_else(|| refused("missing exact op12 payload"))?;
        let decoded = decode_assignment_body_v2(literal_body, limits).map_err(refused)?;
        if &decoded != body {
            return Err(refused("raw op12 body changed"));
        }
        if self.native_writers_tracking_v1() {
            let mut quote = native_writers::NativeWriteQuoteV1::new("Assignment settlement input amount workspace");
            quote.exact::<u64>(action.entries().len() as u64)?;
            self.admit_native_write_v1(quote)?;
        }
        let amounts: Vec<u64> = action.entries().iter().map(|entry| entry.amount).collect();
        let plan = qualify_rooted_assignment_candidate_v2(&self.state, tx, &amounts, ctx.daa_score, limits).map_err(refused)?;
        self.commit_authorized_assignment_spend_v2(ctx, origin, tx, &plan)
    }

    fn next_assignment_settlement_ordinal_v2(
        &mut self,
        id: AssignmentId,
        leg: AssignmentSettlementLegV2,
        party: misaka_palw_positive_contract::identity::PrincipalId,
    ) -> Result<u64, PalwStateV2Error> {
        if self.native_writers_tracking_v1() {
            let mut quote = native_writers::NativeWriteQuoteV1::new("Assignment settlement ordinal prefix keys");
            quote.exact::<u8>(2 * 142)?;
            self.admit_native_write_v1(quote)?;
        }
        let mut prefix = NativeRowKey::AssignmentSettlement { assignment: id, leg, party, ordinal: 0 }.encode();
        prefix.truncate(prefix.len() - 8);
        let route = self.state.kernel_route.as_ref().ok_or_else(|| refused("missing kernel route"))?;
        let mut next = 0u64;
        for ((table, key), bytes) in
            route.aux.range((std::ops::Bound::Included((KERNEL_AUX_TABLE, prefix.clone())), std::ops::Bound::Unbounded))
        {
            if *table != KERNEL_AUX_TABLE || !key.starts_with(&prefix) {
                break;
            }
            let NativeRowBody::AssignmentSettlement(row) = NativeRowValue::decode(key, bytes).map_err(refused)?.body else {
                return Err(refused("wrong settlement row kind"));
            };
            if row.assignment == id && row.leg == leg && row.party == party {
                next = next.max(row.ordinal.checked_add(1).ok_or_else(|| refused("settlement ordinal exhausted"))?);
            }
        }
        Ok(next)
    }

    fn commit_authorized_assignment_spend_v2(
        &mut self,
        ctx: &PalwBlockContextV2,
        origin: AcceptedOrigin,
        tx: &Transaction,
        plan: &AssignmentSpendPlanV2,
    ) -> Result<(), PalwStateV2Error> {
        let id = plan.old.assignment;
        let rooted = PositiveRowsV2::new(&self.state).assignment(id).map_err(refused)?;
        if rooted.funding != plan.old || plan.transaction != tx.id() || plan.daa != ctx.daa_score {
            return Err(refused("settlement CAS or actual transaction changed"));
        }
        if !self.params.settlement_once_active_at(ctx.daa_score) {
            return Err(refused("actual settlement-once owner is dormant"));
        }
        if rooted.journal.next_ordinal >= u64::from(plan.authorization.max_history_frames) {
            return Err(refused("retained original history frame budget exhausted"));
        }
        let payload = self.encode_assignment_control_payload_v2(id, &tx.payload[3..], plan.authorization.max_frame_bytes)?;
        let frame = assignment_frame_v2(
            id,
            rooted.journal.next_ordinal,
            rooted.journal.last_frame,
            AssignmentJournalEventKindV2::Settlement,
            origin,
            &payload,
            plan.authorization.max_frame_bytes,
        )
        .map_err(refused)?;
        let new = plan.accepted_funding_row_v2(frame.payload).map_err(refused)?;
        if let Some(point) = new.current_outpoint {
            self.require_assignment_output_unlocked_v2(point)?;
        }
        let checkpoint = self.try_checkpoint()?;
        let result = (|| {
            for due in &plan.authorization.dues {
                let obligation = obligation(&plan.old, due)?;
                match self.settle_once_v1(
                    ctx.daa_score,
                    obligation,
                    PalwSettlementConsumerV1::AssignmentNativeUtxo,
                    due.amount,
                    PalwSettlementDebitV1::NativeUtxo {
                        outpoint: native_assignment_outpoint_v2(
                            plan.old.current_outpoint.ok_or_else(|| refused("drained before debit"))?,
                        ),
                        amount: due.amount,
                    },
                )? {
                    super::palw_settlement_once_fold_v1::PalwSettleOnceV1::First => {}
                    _ => return Err(refused("settlement is dormant, refused or already consumed")),
                }
                let ordinal = self.next_assignment_settlement_ordinal_v2(id, due.kind, due.party)?;
                let row = AssignmentSettlementRow {
                    assignment: id,
                    funding: AssignmentSettlementFundingV2::Poster,
                    leg: due.kind,
                    party: due.party,
                    ordinal,
                    debit: plan.old.current_outpoint.ok_or_else(|| refused("missing actual debit"))?,
                    recipient: due.recipient,
                    amount: due.amount,
                    receipt: due.receipt,
                    once: AcceptedMaterial { origin, digest: Digest(obligation.id_v1()) },
                    origin,
                };
                let key = NativeRowKey::AssignmentSettlement { assignment: id, leg: due.kind, party: due.party, ordinal };
                if PositiveRowsV2::new(&self.state).read(key).map_err(refused)?.is_some() {
                    return Err(refused("settlement key already exists"));
                }
                self.write_assignment_aux_v2(NativeRowBody::AssignmentSettlement(row))?;
            }
            self.write_assignment_aux_v2(NativeRowBody::AssignmentFunding(new))?;
            let mut assignment = rooted.assignment;
            assignment.last_event = origin;
            self.write_assignment_aux_v2(NativeRowBody::Assignment(assignment))?;
            self.append_assignment_frame_v2(frame, rooted.journal.retention_until_daa)?;
            self.emit_assignment_material_v2(&frame, &payload, plan.authorization.max_frame_bytes)
        })();
        if result.is_err() {
            self.restore(checkpoint);
        }
        result
    }
}

/// Restore checks the exact aux140 marker referenced by each settlement. It
/// does not derive entitlement or certify the historical service/P execution.
pub fn validate_assignment_once_reference_v2(
    aux: &BTreeMap<(u8, Vec<u8>), Vec<u8>>,
    row: &AssignmentSettlementRow,
) -> Result<(), AssignmentMoneyOwnerErrorV2> {
    let bytes =
        aux.get(&(PALW_SETTLEMENT_ONCE_TABLE_V1, row.once.digest.0.to_vec())).ok_or(AssignmentMoneyOwnerErrorV2::CurrentFunding)?;
    let marker: PalwSettlementOnceRowV1 = borsh::from_slice(bytes).map_err(|_| AssignmentMoneyOwnerErrorV2::CurrentFunding)?;
    marker.validate_v1(&row.once.digest.0).map_err(|_| AssignmentMoneyOwnerErrorV2::CurrentFunding)?;
    let expected = match row.leg {
        AssignmentSettlementLegV2::WorkerPayment => PalwSettlementLegV1::AssignmentWorkerPayment,
        AssignmentSettlementLegV2::ServiceFee => PalwSettlementLegV1::AssignmentServiceFee,
        AssignmentSettlementLegV2::CapitalReturn => PalwSettlementLegV1::AssignmentCapitalReturn,
        AssignmentSettlementLegV2::Refund => PalwSettlementLegV1::AssignmentRefund,
        AssignmentSettlementLegV2::Burn => PalwSettlementLegV1::AssignmentContractualBurn,
    };
    let funding_key = match row.funding {
        AssignmentSettlementFundingV2::Poster => NativeRowKey::AssignmentFunding(row.assignment),
        AssignmentSettlementFundingV2::Prover(prover) => NativeRowKey::AssignmentProverFunding { assignment: row.assignment, prover },
    };
    let encoded = funding_key.encode();
    let funding = aux.get(&(KERNEL_AUX_TABLE, encoded.clone())).ok_or(AssignmentMoneyOwnerErrorV2::CurrentFunding)?;
    let funding = NativeRowValue::decode(&encoded, funding).map_err(|_| AssignmentMoneyOwnerErrorV2::CurrentFunding)?;
    let (original, legs, payer, refund) = match funding.body {
        NativeRowBody::AssignmentFunding(f) if row.funding == AssignmentSettlementFundingV2::Poster => {
            (f.original_outpoint, [f.worker, f.fee, f.capital, f.surplus], f.authorization.payer, f.authorization.refund_payload)
        }
        NativeRowBody::AssignmentProverFunding(f) => {
            if row.funding != AssignmentSettlementFundingV2::Prover(f.consent.prover)
                || row.leg == AssignmentSettlementLegV2::WorkerPayment
            {
                return Err(AssignmentMoneyOwnerErrorV2::CurrentFunding);
            }
            let zero = misaka_palw_positive_contract::rows::BalanceLeg { initial: 0, paid: 0, refunded: 0, burned: 0, remaining: 0 };
            let refund = Digest(kaspa_hashes::blake2b_512_address_payload(&f.payer_key).as_bytes());
            (f.original_outpoint, [zero, f.fee, f.capital, f.surplus], f.consent.payer, refund)
        }
        _ => return Err(AssignmentMoneyOwnerErrorV2::CurrentFunding),
    };
    let subject_ok = match row.leg {
        AssignmentSettlementLegV2::WorkerPayment => {
            marker.obligation.subject == palw_settlement_subject_v1(b"assignment-worker/v2", &[&row.assignment.0.0])
                && marker.obligation.ordinal == 0
        }
        AssignmentSettlementLegV2::ServiceFee => {
            marker.obligation.subject
                == palw_settlement_subject_v1(b"assignment-service/v2", &[&row.assignment.0.0, &row.receipt.digest.0])
                && marker.obligation.ordinal == 0
        }
        AssignmentSettlementLegV2::Burn => {
            marker.obligation.subject
                == palw_settlement_subject_v1(b"assignment-burn/v2", &[&row.assignment.0.0, &row.receipt.digest.0])
                && marker.obligation.ordinal == 0
        }
        AssignmentSettlementLegV2::CapitalReturn | AssignmentSettlementLegV2::Refund => {
            if row.party != payer || row.recipient != refund {
                return Err(AssignmentMoneyOwnerErrorV2::CurrentFunding);
            }
            let index = original.index.to_le_bytes();
            (0u8..4).any(|source| {
                let allowed = if row.leg == AssignmentSettlementLegV2::CapitalReturn { source == 2 } else { source != 2 };
                allowed
                    && marker.obligation.subject
                        == palw_settlement_subject_v1(
                            b"assignment-return/v2",
                            &[&row.assignment.0.0, &original.transaction.0, &index, &[source]],
                        )
                    && marker.obligation.ordinal.checked_add(row.amount).is_some_and(|n| n <= legs[source as usize].refunded)
            })
        }
    };
    if !subject_ok {
        return Err(AssignmentMoneyOwnerErrorV2::CurrentFunding);
    }
    if row.once.origin != row.origin
        || marker.obligation.leg != expected
        || marker.obligation.party != row.party.0.0
        || marker.consumer != PalwSettlementConsumerV1::AssignmentNativeUtxo
        || marker.amount != row.amount
        || marker.settled_daa != row.origin.accepted_daa
        || marker.debit
            != (PalwSettlementDebitV1::NativeUtxo { outpoint: native_assignment_outpoint_v2(row.debit), amount: row.amount })
    {
        return Err(AssignmentMoneyOwnerErrorV2::CurrentFunding);
    }
    Ok(())
}
