//! Authenticate a literal PMR1 against the rooted original, draw and physical
//! rank reservation. This is a preparation for the paired native writer, not
//! a public admission gate or proof of material delivery.
use super::{
    PreparedTfp1PublicOriginalV1,
    manifest_carrier::{ManifestRequestCarrierContextV1, ManifestRequestCarrierScopeV1, ValidatedManifestRequestCarrierV1},
    manifest_paid::{PaidManifestRequestRowV1, manifest_request_charge_purpose_v1},
    native_link::Tfp1PublicOriginalLinkV1,
    preclaim::initial_request::InitialMaterialRequestPlanV1,
    preclaim::{DrawnBatchPreclaimTermsV1, ProtectedBatchPreclaimTermsV1, ProtectedBatchRoleV1},
};
use crate::{
    Hash64,
    palw_court_service_v1::{ServiceCost, ServicePacket},
    palw_positive_contract_v2::{
        PUBLIC_MANIFEST_PAID_TABLE_V1, PreparedDrawnPublicManifestCaseV1, PreparedPublicDrawV1, require_current_public_origin_v1,
    },
    palw_state_v2::{PalwBlockContextV2, PalwChainStateV2},
};
use misaka_palw_positive_contract::{
    ContractError,
    public_one_honest::{Claimant, ManifestRequestContext, PendingManifestCaseRecordV1},
};

/// Private construction prevents an extras caller from declaring an arbitrary
/// row paid. The writer rechecks the current prefix and consumes the fee last.
pub(crate) struct PreparedPaidManifestRequestV1 {
    link: Tfp1PublicOriginalLinkV1,
    pending: PreparedDrawnPublicManifestCaseV1,
    paid: PaidManifestRequestRowV1,
    carrier: ManifestRequestCarrierContextV1,
    packet: ServicePacket,
    purpose: Hash64,
}

impl PreparedTfp1PublicOriginalV1 {
    /// `context` and `quote_charge` come from the native original/profile and
    /// accepted order, never from submitted request fields. The pricing owner
    /// includes signature, carriage, retention, indexes and undo costs. No
    /// network profile currently supplies this complete production policy.
    pub(crate) fn prepare_paid_manifest_request_v1(
        &self,
        state: &PalwChainStateV2,
        draw: &PreparedPublicDrawV1,
        terms: &DrawnBatchPreclaimTermsV1,
        handoff: &ValidatedManifestRequestCarrierV1,
        context: &ManifestRequestContext<'_>,
        at: PalwBlockContextV2,
        quote_charge: impl FnOnce(ServiceCost, u64) -> Result<u64, ContractError>,
    ) -> Result<PreparedPaidManifestRequestV1, ContractError> {
        if terms.obligation_v1() != self.obligation {
            return Err(ContractError::Scope);
        }
        let plan = terms.initial_request_v1().ok_or(ContractError::Scope)?;
        self.prepare_paid_manifest_with_plan_v1(state, draw, plan, None, handoff, context, at, quote_charge)
    }

    /// Selected-rank request under the explicit all-role reservation domain.
    /// Outsider/defense capacity never grants selected-bond request authority.
    pub(crate) fn prepare_paid_protected_manifest_request_v1(
        &self,
        state: &PalwChainStateV2,
        draw: &PreparedPublicDrawV1,
        terms: &ProtectedBatchPreclaimTermsV1,
        handoff: &ValidatedManifestRequestCarrierV1,
        context: &ManifestRequestContext<'_>,
        at: PalwBlockContextV2,
        quote_charge: impl FnOnce(ServiceCost, u64) -> Result<u64, ContractError>,
    ) -> Result<PreparedPaidManifestRequestV1, ContractError> {
        let plan = terms.initial_request_v1().ok_or(ContractError::Scope)?;
        self.prepare_paid_manifest_with_plan_v1(state, draw, plan, Some(terms), handoff, context, at, quote_charge)
    }

    fn prepare_paid_manifest_with_plan_v1(
        &self,
        state: &PalwChainStateV2,
        draw: &PreparedPublicDrawV1,
        plan: InitialMaterialRequestPlanV1,
        protected: Option<&ProtectedBatchPreclaimTermsV1>,
        handoff: &ValidatedManifestRequestCarrierV1,
        context: &ManifestRequestContext<'_>,
        at: PalwBlockContextV2,
        quote_charge: impl FnOnce(ServiceCost, u64) -> Result<u64, ContractError>,
    ) -> Result<PreparedPaidManifestRequestV1, ContractError> {
        let link = self.rooted_link_v1(state)?;
        if protected.is_some_and(|terms| {
            !link.uses_public_frame_v1() || terms.obligation_v1() != self.obligation || terms.rescue_v1() != self.companion.rescue
        }) {
            return Err(ContractError::Scope);
        }
        plan.check_schedule_v1(
            self.companion.carrier.origin.accepted_daa,
            self.companion.material_request_ready_by,
            self.companion.schedule,
        )
        .map_err(|_| ContractError::Scope)?;
        let Claimant::DrawnBond(bond) = context.claimant else { return Err(ContractError::Identity) };
        let rank = self.companion.rooted_drawn_batch_opportunity_v1(state, draw, bond)?;
        let scope = ManifestRequestCarrierScopeV1 {
            claim: link.claim_v1(),
            source: link.source_v1(),
            reservation_scope: match protected {
                Some(terms) => terms.reservation_scope_v1(self.companion.original, ProtectedBatchRoleV1::Drawn, rank.index_v1())?,
                None => self.drawn_batch_reservation_scope_v1(rank.index_v1())?,
            },
        };
        require_current_public_origin_v1(context.origin, &at)?;
        if context.origin.event_ordinal != 0 {
            return Err(ContractError::Scope);
        }
        let carrier = ManifestRequestCarrierContextV1 {
            scope,
            at,
            carrier_block: Hash64::from_bytes(context.origin.carrier_block.0),
            accepted_ordinal: usize::try_from(context.origin.accepted_tx_ordinal).map_err(|_| ContractError::Range)?,
            carrier: Hash64::from_bytes(context.origin.carrier_transaction.0),
        };
        let bytes = handoff.request_at_v1(&carrier).map_err(|_| ContractError::Scope)?;
        let pending = self.companion.prepare_drawn_manifest_storage_record(link.reservation_v1(), draw, bytes, context)?;
        let request = match protected {
            Some(terms) => {
                let service = state.court_service_row_v3_v1().ok_or(ContractError::Scope)?;
                let mut selected = None;
                for slot in 0..terms.request_count_v2() {
                    let request = terms.request_v2(scope.claim, scope.reservation_scope, slot).map_err(|_| ContractError::Scope)?;
                    if service.state.book.assigned_daa(&request.packet.id) == Some(at.daa_score) {
                        if selected.is_some() { return Err(ContractError::Scope); }
                        selected = Some(request);
                    }
                }
                selected.ok_or(ContractError::Scope)?
            },
            None => plan.request_v1(scope.claim, scope.reservation_scope, self.companion.carrier.origin.accepted_daa)
                .map_err(|_| ContractError::Scope)?,
        };
        let packet = request.packet;
        check_slot(state, &link, &packet, &at)?;
        let charge = quote_charge(packet.cost, request.retained_bytes)?;
        if charge == 0 || handoff.remaining_fee_v1() < charge {
            return Err(ContractError::Budget);
        }
        let purpose = manifest_request_charge_purpose_v1(link.original_v1(), packet.id)?;
        let paid = if protected.is_some() {
            PaidManifestRequestRowV1::new_for_slot_v2(scope, link.original_v1(), pending.case, context.origin, pending.row.request_digest(), packet.id, charge)?
        } else {
            PaidManifestRequestRowV1::new_v1(scope, link.original_v1(), pending.case, context.origin, pending.row.request_digest(), purpose, charge)?
        };
        let prepared = PreparedPaidManifestRequestV1 { link, pending, paid, carrier, packet, purpose };
        prepared.storage_rows_at_v1(state, &at)?;
        Ok(prepared)
    }
}

fn check_slot(
    state: &PalwChainStateV2,
    link: &Tfp1PublicOriginalLinkV1,
    expected: &ServicePacket,
    at: &PalwBlockContextV2,
) -> Result<(), ContractError> {
    let service = state.court_service_row_v3_v1().ok_or(ContractError::Scope)?;
    let binding = service.state.claims.get(&link.claim_v1()).ok_or(ContractError::Scope)?;
    if binding.claim != link.claim_v1()
        || binding.source_id != link.source_v1()
        || binding.allocation_id != link.service_frame_reference_v1()
        || binding.original_accepted_daa != link.carrier_v1().origin.accepted_daa
        || binding.hard_deadline_daa
            != misaka_palw_positive_contract::public_one_honest::OriginalClock::new(link.carrier_v1().origin.accepted_daa)?.hard()
        || !binding.packet_ids.contains(&expected.id)
        || service.state.book.packet(&expected.id) != Some(expected)
        || service.state.book.assigned_daa(&expected.id) != Some(at.daa_score)
        || service.causal.contains_key(&expected.id)
    {
        return Err(ContractError::Scope);
    }
    Ok(())
}

impl PreparedPaidManifestRequestV1 {
    pub(crate) fn pending_key_v1(&self) -> [u8; PendingManifestCaseRecordV1::KEY_BYTES] {
        PendingManifestCaseRecordV1::key(self.link.original_v1(), self.pending.case)
    }

    pub(crate) fn carrier_v1(&self) -> &ManifestRequestCarrierContextV1 {
        &self.carrier
    }

    pub(crate) fn charge_purpose_v1(&self) -> Hash64 {
        self.purpose
    }

    pub(crate) fn charge_sompi_v1(&self) -> u64 {
        self.paid.charged_sompi_v1()
    }

    /// Existing identical paired rows are an idempotent no-op. A legacy
    /// pending row alone cannot be promoted to a paid new-profile request.
    pub(crate) fn storage_rows_at_v1(
        &self,
        state: &PalwChainStateV2,
        at: &PalwBlockContextV2,
    ) -> Result<Option<(&PendingManifestCaseRecordV1, &PaidManifestRequestRowV1)>, ContractError> {
        if &self.carrier.at != at || state.public_tfp1_original_link_v1(self.link.claim_v1())? != Some(self.link) {
            return Err(ContractError::Scope);
        }
        check_slot(state, &self.link, &self.packet, at)?;
        let pending = self.pending.storage_row_at_v1(state, at)?;
        let route = state.kernel_route_state_v1().ok_or(ContractError::Scope)?;
        let old = route.aux.get(&(PUBLIC_MANIFEST_PAID_TABLE_V1, self.paid.key_v1().to_vec()));
        match (pending, old) {
            (Some((_, row)), None) => Ok(Some((row, &self.paid))),
            (None, Some(bytes)) if PaidManifestRequestRowV1::decode_row(&self.paid.key_v1(), bytes)? == self.paid => Ok(None),
            _ => Err(ContractError::Scope),
        }
    }
}

/// Reuse the real paired original/draw/signature fixture. Only the service
/// book and charge policy are fixtures here; this is not public admission or
/// an end-to-end test of the new signed preclaim resource compiler.
#[cfg(test)]
pub(crate) fn exercise_paid_request_v1(
    paired: &PreparedTfp1PublicOriginalV1,
    detached: &PreparedTfp1PublicOriginalV1,
    drawn: &PalwChainStateV2,
    draw: &PreparedPublicDrawV1,
    context: &ManifestRequestContext<'_>,
    wire: &[u8],
    at: PalwBlockContextV2,
    params: &crate::palw_state_v2::PalwStateParamsV2,
) -> (PalwChainStateV2, misaka_palw_positive_contract::rows::AcceptedOrigin) {
    use super::manifest_carrier::{MANIFEST_REQUEST_CARRIER_BYTES_V1, encode_manifest_request_carrier_v1};
    use crate::{
        palw_court_service_release_v1::CourtServiceRowV3,
        palw_court_service_state_v2::{CourtServiceStateV2, ServiceClaimBinding, court_service_profile_id_v2},
        palw_court_service_v1::ServiceReservations,
        palw_state_v2::tests::native_writers_v1::{exercise_paid_manifest_writer_v1, seed_manifest_service_book_v1},
        subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
        tx::{
            PopulatedTransaction, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
            ValidatedTransaction,
        },
    };
    use misaka_palw_positive_contract::{
        public_one_honest::{FirstNoticeDemand, SignedManifestRequestV1},
        rows::NativeHash64,
    };
    use std::collections::{BTreeMap, BTreeSet};
    let link = paired.rooted_link_v1(drawn).unwrap();
    let scope = ManifestRequestCarrierScopeV1 {
        claim: link.claim_v1(),
        source: link.source_v1(),
        reservation_scope: paired.drawn_batch_reservation_scope_v1(0).unwrap(),
    };
    let literal: &[u8; SignedManifestRequestV1::MAX_WIRE_BYTES] = wire.try_into().unwrap();
    let tx_for = |scope, literal| {
        Transaction::new(
            0,
            vec![TransactionInput::new(TransactionOutpoint::new(Hash64::from_u64_word(900), 0), vec![], 0, 0)],
            vec![TransactionOutput::new(80, ScriptPublicKey::default())],
            0,
            SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            encode_manifest_request_carrier_v1(scope, literal).unwrap().to_vec(),
        )
    };
    let tx = tx_for(scope, literal);
    let make_handoff = || {
        let validated = ValidatedTransaction::new(
            PopulatedTransaction::new(&tx, vec![UtxoEntry::new(100, ScriptPublicKey::default(), 1, false)]),
            20,
        );
        ValidatedManifestRequestCarrierV1::prepare_v1(&validated, 8, Hash64::from_bytes(context.origin.carrier_block.0), at, 0)
            .unwrap()
            .unwrap()
    };
    let context = ManifestRequestContext {
        origin: misaka_palw_positive_contract::rows::AcceptedOrigin {
            carrier_transaction: NativeHash64(tx.id().as_bytes()),
            ..context.origin
        },
        ..*context
    };
    let a = paired.companion.carrier.origin.accepted_daa;
    let notice = FirstNoticeDemand {
        draw_ready_by: a + 1,
        material_ready_by: a + 1,
        cold_acquisition_daa: 1,
        full_inspection_daa: 1,
        notice_inclusion_daa: 1,
        missing_material_detected_by: a + 1,
        material_request_inclusion_daa: 1,
    };
    let retained = InitialMaterialRequestPlanV1::minimum_retained_bytes_v1();
    let bytes = MANIFEST_REQUEST_CARRIER_BYTES_V1 as u64;
    let cost = ServiceCost { bytes, work: bytes, io: retained, controls: 1 };
    let plan = InitialMaterialRequestPlanV1::new_v1(a, notice, cost, retained).unwrap();
    let packet = plan.request_v1(scope.claim, scope.reservation_scope, a).unwrap().packet;
    let mut book = ServiceReservations::new(cost).unwrap();
    book.reserve(&[packet.clone()], context.schedule.hard()).unwrap();
    let source_profile = Hash64::from_u64_word(901);
    let service = CourtServiceRowV3 {
        version: 3,
        state: CourtServiceStateV2 {
            version: 3,
            source_profile_id: source_profile,
            profile_id: court_service_profile_id_v2(source_profile),
            book,
            claims: BTreeMap::from([(
                scope.claim,
                ServiceClaimBinding {
                    claim: scope.claim,
                    allocation_id: link.service_frame_reference_v1(),
                    source_id: scope.source,
                    original_accepted_daa: a,
                    hard_deadline_daa: context.schedule.hard(),
                    packet_ids: BTreeSet::from([packet.id]),
                },
            )]),
        },
        resolved: BTreeMap::new(),
        causal: BTreeMap::new(),
    };
    let mut state = drawn.clone();
    service.state.check_context(drawn).unwrap();
    for changed_field in 0..4 {
        let mut wrong = service.state.clone();
        let binding = wrong.claims.get_mut(&scope.claim).unwrap();
        match changed_field {
            0 => binding.allocation_id = Hash64::from_u64_word(902),
            1 => binding.source_id = Hash64::from_u64_word(903),
            2 => {
                binding.original_accepted_daa += 1;
                binding.hard_deadline_daa += 1;
            }
            _ => binding.hard_deadline_daa -= 1,
        }
        assert!(wrong.check_context(drawn).is_err(), "service scope field {changed_field} is native, not caller declared");
    }
    seed_manifest_service_book_v1(&mut state, service, params);
    if let Some(frame) = state.public_tfp1_frame_v1(scope.claim).unwrap() {
        assert!(state.court_service_holds_bond_v1(&frame.producer_v1()));
        assert!(!state.court_service_holds_bond_v1(&crate::palw_state_v2::PalwBondKeyV2(TransactionOutpoint::new(
            Hash64::from_u64_word(904),
            0,
        ))));
    }
    let price = |actual, actual_retained| {
        assert_eq!(actual, cost);
        assert_eq!(actual_retained, retained);
        Ok(7)
    };
    let handoff = make_handoff();
    assert!(paired.prepare_paid_manifest_with_plan_v1(drawn, draw, plan, None, &handoff, &context, at, price).is_err());
    assert!(detached.prepare_paid_manifest_with_plan_v1(&state, draw, plan, None, &handoff, &context, at, price).is_err());
    let prepared = paired.prepare_paid_manifest_with_plan_v1(&state, draw, plan, None, &handoff, &context, at, price).unwrap();
    for fee in [0, 13] {
        assert!(paired.prepare_paid_manifest_with_plan_v1(&state, draw, plan, None, &handoff, &context, at, |_, _| Ok(fee)).is_err());
    }
    let wrong_point = PalwBlockContextV2 { daa_score: at.daa_score + 1, ..at };
    assert!(prepared.storage_rows_at_v1(&state, &wrong_point).is_err());
    let mut wrong = *literal;
    *wrong.last_mut().unwrap() ^= 1;
    let wrong_tx = tx_for(scope, &wrong);
    let invalid = ValidatedTransaction::new(
        PopulatedTransaction::new(&wrong_tx, vec![UtxoEntry::new(100, ScriptPublicKey::default(), 1, false)]),
        20,
    );
    let invalid_handoff =
        ValidatedManifestRequestCarrierV1::prepare_v1(&invalid, 8, Hash64::from_bytes(context.origin.carrier_block.0), at, 0)
            .unwrap()
            .unwrap();
    let invalid_context = ManifestRequestContext {
        origin: misaka_palw_positive_contract::rows::AcceptedOrigin {
            carrier_transaction: NativeHash64(wrong_tx.id().as_bytes()),
            ..context.origin
        },
        ..context
    };
    assert!(
        paired.prepare_paid_manifest_with_plan_v1(&state, draw, plan, None, &invalid_handoff, &invalid_context, at, price).is_err()
    );
    assert_eq!(handoff.remaining_fee_v1(), 12);
    let accepted = exercise_paid_manifest_writer_v1(state, at, params, &prepared, make_handoff);
    assert!(prepared.storage_rows_at_v1(&accepted, &at).unwrap().is_none());
    assert!(paired.rooted_drawn_batch_case_v1(&accepted, draw, wire, &context).is_ok());
    (accepted, context.origin)
}
