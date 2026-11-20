//! Reconstruct the protected original's admission inputs from native history.
//! The archive owner supplies the actual Full-validated carrier, immutable
//! selected parent, executed prefix, and rules resolved at original acceptance.
//! No current-tip policy or caller-supplied reservation substitutes for these.

use super::{
    PreparedTfp1PublicOriginalV1, funding::admission::Tfp1ProtectedFundingAdmissionErrorV1, native_frame::PublicTfp1FrameV1,
    native_plan::NativeTfp1OriginalPlanV3, preclaim::ProtectedBatchPreclaimTermsV1,
    public_policy::resolve_protected_planning_policy_v3, service_booking::ProtectedServiceBaseV1,
};
use crate::{
    palw_funded_original_coordinator_v1::PalwFundedOriginalPairingV1,
    palw_native_resource_v1::{NativeDecodeErrorV1, NativeRetainedV1, reserve_native_pending_requests_v1},
    palw_state_v2::{NativeAcceptedCaseCarriageV2, PalwBlockContextV2, PalwChainStateV2, PalwStateParamsV2, PalwTransitionExtrasV1},
    palw_tfp1_original_history_v1::rooted_tfp1_original_history_v1,
    palw_tir_funded_original_envelope_v1::actual_funded_payload_v1,
    palw_tir_funded_original_envelope_v3::PalwTirFundedOriginalEnvelopeV3,
};
use misaka_palw_positive_contract::{
    ContractError,
    public_one_honest::{Claimant, ManifestRequestContext, OriginalCommit, stateless_wide_replay_plan_v1},
    rows::{AcceptedOrigin, NativeHash64},
};

type Error = NativeDecodeErrorV1<String>;

/// This owns only immutable admission inputs, not permission to accept a
/// request. The paid-request preparer still checks the actual current prefix,
/// selected draw member, signature, inclusion slot and carrier fee.
pub struct RestoredProtectedOriginalV3 {
    original: PreparedTfp1PublicOriginalV1,
    literal: OriginalCommit,
    terms: ProtectedBatchPreclaimTermsV1,
}

/// Reconstruct from the archive owner's literal carrier, without accepting a
/// separately decoded or caller-substituted pairing. The node must authenticate
/// the selected ancestry, header, delta, Full UTXO fee and original rule context
/// before calling. This private-interchange API does not activate admission.
/// Typed reservations cover pairing decode/ownership; complete request CPU/IO
/// pricing remains the accepting owner's separate responsibility.
pub fn restore_protected_native_carriage_v3(
    current: &PalwChainStateV2,
    original_parent: &PalwChainStateV2,
    archived: &NativeAcceptedCaseCarriageV2,
    point: &PalwBlockContextV2,
    extras: &PalwTransitionExtrasV1,
    params: &PalwStateParamsV2,
) -> Result<RestoredProtectedOriginalV3, NativeDecodeErrorV1<String>> {
    use crate::palw_native_resource_v1::{NativeCoverageV1, native_typed_value_requests_v1};
    use NativeDecodeErrorV1::{Decode, Resource};
    super::super::require_current_public_origin_v1(archived.origin(), point)
        .map_err(|error| Decode(format!("archived original point: {error:?}")))?;
    let transaction = archived.transaction();
    let payload = actual_funded_payload_v1(transaction).map_err(Decode)?;
    let wire = PalwTirFundedOriginalEnvelopeV3::decode_v3(payload).map_err(Decode)?;
    if !wire.is_protected_v3() {
        return Err(Decode("archive is not a protected TFO3 original".into()));
    }
    // Pairing clones the complete transaction and retains its decoded funded
    // envelope. It also holds a decoded header while the FP decoder runs.
    // The protected scanner covers each of those three inner ownership peaks.
    let transaction_bytes = native_typed_value_requests_v1(transaction).map_err(Resource)?;
    let pairing_bytes =
        wire.inner_native_shape_v3().heap.checked_mul(3).and_then(|bytes| bytes.checked_add(transaction_bytes)).ok_or_else(|| {
            Resource(NativeCoverageV1::overflow::<PalwFundedOriginalPairingV1>("archived original pairing workspace"))
        })?;
    let _pairing_owner = reserve_native_pending_requests_v1(pairing_bytes).map_err(Resource)?;
    let pairing =
        crate::palw_funded_original_coordinator_v1::palw_funded_original_pairing_payload_v1(transaction, payload).map_err(Decode)?;
    restore_protected_native_original_v3(current, original_parent, archived, &pairing, point, extras, params)
}

impl RestoredProtectedOriginalV3 {
    /// The native claim reconstructed from this literal original.
    pub fn claim_v3(&self) -> crate::Hash64 {
        self.original.obligation_v1().facts_v1().native_identity_v1().0
    }

    /// A restored original alone grants no request authority. The existing
    /// paid-request preparer rechecks its rooted link at this exact prefix.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_paid_request_v3(
        &self,
        state: &PalwChainStateV2,
        handoff: &super::manifest_carrier::ValidatedManifestRequestCarrierV1,
        at: PalwBlockContextV2,
        quote_charge: impl FnOnce(
            crate::palw_court_service_v1::ServiceCost,
            u64,
        ) -> Result<u64, misaka_palw_positive_contract::ContractError>,
    ) -> Result<super::manifest_request::PreparedPaidManifestRequestV1, NativeDecodeErrorV1<ContractError>> {
        use NativeDecodeErrorV1::Decode;
        let draw = self.original.restore_bound_request_draw_v3(state)?;
        // The submitted message selects only a bond; the native plan and draw
        // provide every expected authority and material value.
        let claimant = handoff.request_message_v2().map_err(|_| Decode(ContractError::Scope))?.claimant;
        let context = native_request_context_v3(
            self.original.companion_v1(),
            &self.literal,
            &draw,
            claimant,
            self.terms.manifest_supply_v3().map_err(Decode)?,
            handoff.context_v1(),
            at,
        )
        .map_err(Decode)?;
        self.original
            .prepare_paid_protected_manifest_request_v1(state, &draw, &self.terms, handoff, &context, at, quote_charge)
            .map_err(Decode)
    }
}

impl PreparedTfp1PublicOriginalV1 {
    /// Reconstruct the stored draw from this claim's native bound panel. The
    /// accepted draw row supplies its authenticated inclusion point; no request
    /// field selects a beacon, member list, key, reservation or relation plan.
    pub(crate) fn restore_bound_request_draw_v3(
        &self,
        state: &PalwChainStateV2,
    ) -> Result<NativeRetainedV1<super::super::PreparedPublicDrawV1>, NativeDecodeErrorV1<ContractError>> {
        use crate::palw_native_resource_v1::{BTreeNodeEnvelopeV1, TirNativeRequestsV1};
        use NativeDecodeErrorV1::{Decode, Resource};
        use misaka_palw_positive_contract::{identity::Digest, public_one_honest::PublicDrawMemberV1};
        let claim = self.obligation_v1().facts_v1().native_identity_v1().0;
        let panel = state.panel(&claim).ok_or(Decode(ContractError::Scope))?;
        let count = u64::try_from(panel.seats.len()).map_err(|_| Decode(ContractError::Range))?;
        let original = self.companion_v1();
        if count == 0 || count > original.rescue.max_drawn_bonds {
            return Err(Decode(ContractError::Budget));
        }
        // Rebuilding uses a temporary member Vec, the returned member map,
        // and a duplicate-reservation set. Hashing streams without a Vec.
        // Retain the conservative combined lease through the later verifier.
        let mut q = TirNativeRequestsV1::new();
        q.vec_v1::<PublicDrawMemberV1>(count).map_err(Resource)?;
        q.exact_v1::<BTreeNodeEnvelopeV1<(Digest, u32), PublicDrawMemberV1>>(count).map_err(Resource)?;
        q.exact_v1::<BTreeNodeEnvelopeV1<Digest, ()>>(count).map_err(Resource)?;
        // Fixed auxiliary lookup keys coexist with nested admission/link reads.
        q.exact_v1::<[u8; 64]>(4).map_err(Resource)?;
        let lease = reserve_native_pending_requests_v1(q.bytes_v1()).map_err(Resource)?;
        let link = self.rooted_link_v1(state).map_err(Decode)?;
        if !link.uses_public_frame_v1() {
            return Err(Decode(ContractError::Scope));
        }
        let record = state.public_draw_record_v1(original.original).map_err(Decode)?.ok_or(Decode(ContractError::Scope))?;
        let point = record.draw_point_v1();
        if panel.bound_daa > point.daa_score {
            return Err(Decode(ContractError::Scope));
        }
        let members = crate::palw_state_v2::included_replay::seat_members_v1(state, panel, link.reservation_v1())
            .ok_or(Decode(ContractError::Identity))?;
        let draw = original
            .restore_draw(&record, Digest(panel.anchor.as_bytes()), point, &members, stateless_wide_replay_plan_v1())
            .map_err(Decode)?;
        Ok(NativeRetainedV1::from_owner_reserved_v1(draw, lease))
    }
}

/// Compose expectations without reading signed packet fields other than its
/// proposed claimant. The common verifier subsequently authenticates all fields.
fn native_request_context_v3<'a>(
    original: &super::super::PreparedPublicOriginalV1,
    literal: &'a OriginalCommit,
    draw: &super::super::PreparedPublicDrawV1,
    claimant: Claimant,
    supply: (misaka_palw_positive_contract::identity::Digest, u64),
    carrier: super::manifest_carrier::ManifestRequestCarrierContextV1,
    at: PalwBlockContextV2,
) -> Result<ManifestRequestContext<'a>, ContractError> {
    if carrier.at != at {
        return Err(ContractError::Scope);
    }
    let Claimant::DrawnBond(bond) = claimant else { return Err(ContractError::Identity) };
    let expected = original.drawn_manifest_request(draw, literal, bond, stateless_wide_replay_plan_v1(), supply.0)?;
    Ok(ManifestRequestContext {
        original: literal,
        claimant: expected.claimant,
        signer: expected.signer,
        inspection_scope: expected.inspection_scope,
        supply_obligation: expected.supply_obligation,
        reservation: expected.reservation,
        max_manifest_bytes: original.max_manifest_bytes,
        manifest_limits: original.manifest_limits,
        manifest_body_bytes_reserved: supply.1,
        schedule: original.schedule,
        origin: AcceptedOrigin {
            accepting_block: NativeHash64(at.block.as_bytes()),
            carrier_block: NativeHash64(carrier.carrier_block.as_bytes()),
            carrier_transaction: NativeHash64(carrier.carrier.as_bytes()),
            accepted_tx_ordinal: u32::try_from(carrier.accepted_ordinal).map_err(|_| ContractError::Range)?,
            event_ordinal: 0,
            accepted_daa: at.daa_score,
            accepted_blue_score: at.blue_score,
        },
    })
}

/// Reuse original material/funding admission on a disposable historical book.
/// Nothing is installed or charged. The caller must authenticate archive
/// ancestry, header/delta/UTXO data and the original rule context before entry.
/// In particular, this DATA adapter cannot establish Full validation itself.
#[allow(clippy::too_many_arguments)]
pub(crate) fn restore_protected_native_original_v3(
    current: &PalwChainStateV2,
    original_parent: &PalwChainStateV2,
    archived: &NativeAcceptedCaseCarriageV2,
    pairing: &PalwFundedOriginalPairingV1,
    point: &PalwBlockContextV2,
    extras: &PalwTransitionExtrasV1,
    params: &PalwStateParamsV2,
) -> Result<RestoredProtectedOriginalV3, Error> {
    use NativeDecodeErrorV1::{Decode, Resource};
    let refused = |reason: &str| Decode(reason.to_string());
    let contract = |error| Decode(format!("archived protected original: {error:?}"));
    let claim = pairing.claim();
    let history = rooted_tfp1_original_history_v1(current, claim)
        .map_err(contract)?
        .ok_or_else(|| refused("protected original history unavailable"))?;
    let prefix = archived.prefix_state().map_err(|_| refused("original executed prefix unavailable"))?;
    if history.origin != archived.origin()
        || history.selected_parent.0 != archived.selected_parent().as_bytes()
        || original_parent.last_point().map(|p| p.block) != Some(archived.selected_parent())
        || prefix.state_root().as_bytes() != history.pre_event_root.0
        || pairing.carrier() != archived.transaction()
        || !pairing.carrier().id_is_current()
        || !extras.native_court_rules_v1.as_ref().is_some_and(|rules| rules.at == *point)
    {
        return Err(refused("original archive carrier, parent, prefix or rule point differs"));
    }
    super::super::require_current_public_origin_v1(history.origin, point).map_err(contract)?;
    history.check_populated_data(archived.transaction(), archived.entries(), archived.fee()).map_err(contract)?;
    let stored_frame =
        current.public_tfp1_frame_v1(claim).map_err(contract)?.ok_or_else(|| refused("protected original frame unavailable"))?;
    if stored_frame.origin_v1() != history.origin || stored_frame.original_id_v1() != history.original {
        return Err(refused("protected original frame differs from native history"));
    }
    let payload = actual_funded_payload_v1(pairing.carrier()).map_err(Decode)?;
    let wire = PalwTirFundedOriginalEnvelopeV3::decode_v3(payload).map_err(Decode)?;
    if !wire.is_protected_v3() || !pairing.envelope().is_protected_v2() {
        return Err(refused("archive is not a protected TFO3 original"));
    }
    // The scanner covers decoded FP/header buffers and their shape-check
    // temporaries. The pairing itself is already owned by the archive caller.
    // Keep this scratch ticket through all borrowed original planning work.
    // One decoded FP remains live while the independent header decoder runs
    // its own FP/header shape checks. Retain both scanner bounds concurrently.
    let scratch = wire.inner_native_shape_v3().heap.checked_mul(2).ok_or_else(|| {
        Resource(crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<RestoredProtectedOriginalV3>(
            "archived protected original scratch",
            "concurrent decoder request overflow",
        ))
    })?;
    let _scratch = reserve_native_pending_requests_v1(scratch).map_err(Resource)?;
    let fp = pairing.envelope().original_fp_v2().map_err(Decode)?;
    fp.validate_signature_v3(crate::palw_attempt_material_v3::verify_ready_signature_v3)
        .map_err(|e| Decode(format!("archived original executor signature: {e:?}")))?;
    let header = pairing.envelope().original_header_v2().map_err(Decode)?;
    let original = &fp.commitment;
    let producer = prefix
        .bond(&crate::palw_state_v2::PalwBondKeyV2(original.job.executor_bond))
        .ok_or_else(|| refused("original executor absent at its executed prefix"))?;
    if producer.pubkey != original.job.executor_pubkey || !matches!(producer.status, crate::palw_state_v2::PalwBondStatusV2::Active) {
        return Err(refused("original executor key or status differs at its executed prefix"));
    }
    let prompt = pairing.public_prompt_bytes_v3().ok_or_else(|| refused("archived original prompt unavailable"))?;
    let descriptor = pairing.public_hf_descriptor_bytes_v2().ok_or_else(|| refused("archived original descriptor unavailable"))?;
    let form = extras.prompt_ids_form_v1();
    // Same split as original admission: resolve policy/service at the actual
    // ordered prefix, immutable material identities at the selected parent.
    let schedules = super::public_policy::ProtectedPlanningSchedulesV3::selected_v3(extras.protected_planning_v3).map_err(|e| e.into_native_v1())?;
    let pinned = resolve_protected_planning_policy_v3(
        prefix,
        extras,
        params,
        point.daa_score,
        point.blue_score,
        original,
        &header,
        prompt,
        descriptor,
        form,
        wire.uses_relative_service_v4(),
        wire.prompt_encoding_v5(),
        schedules,
    )
    .map_err(|e| e.into_native_v1())?;
    let policy = pinned.policy();
    let plan = NativeTfp1OriginalPlanV3::derive_v3(
        original_parent,
        extras,
        params,
        point.daa_score,
        point.blue_score,
        original,
        &header,
        form,
        prompt,
        descriptor,
        &policy,
        |stage, bytes| pinned.price_delivery_v1(stage, bytes),
    )?;
    if plan.terms_v3().obligation_v1().facts_v1().native_identity_v1().0 != claim {
        return Err(refused("archived original plan changes the claim"));
    }
    let admission = plan.admission_v3(original, prompt)?;
    let reservation = plan.actual_v3(history.origin)?;
    // Includes the raw admission-input commitment in table 206, not only
    // deadlines or the companion digest. This also authenticates the signed
    // companion and its current rooted native link.
    let restored = admission.restore_tfp1_pairing_v1(current, pairing, &reservation).map_err(contract)?;
    let base = ProtectedServiceBaseV1::resolve_v1(prefix, params, extras, point.daa_score, &policy.service_book_profile)?;
    let mut book = NativeRetainedV1::try_from_clone_v1(base.book_v1()).map_err(Resource)?;
    let (prepared, booking, _funding) = admission
        .prepare_tfp1_material_funding_v3(
            original_parent,
            extras,
            point,
            form,
            original,
            plan.source_v3(),
            pairing,
            &reservation,
            plan.projection_v3(),
            plan.terms_v3(),
            &mut book,
            policy.maximum_packets,
            |stage, bytes| pinned.price_delivery_v1(stage, bytes),
            policy.funding,
        )
        .map_err(|e| match e {
            Tfp1ProtectedFundingAdmissionErrorV1::LocalResource(coverage) => Resource(coverage),
            other => Decode(format!("archived original material/reservation/funding: {other:?}")),
        })?;
    let class =
        original_parent.tir_class_v1(&original.job.class_id).ok_or_else(|| refused("archived original parent class unavailable"))?;
    let frame = PublicTfp1FrameV1::prepare_with_native_service_v3(
        &prepared,
        original,
        plan.source_v3(),
        class,
        point,
        policy.maximum_leaves,
        &booking,
    )
    .map_err(|e| match e {
        Resource(coverage) => Resource(coverage),
        Decode(error) => contract(error),
    })?;
    if prepared != restored || &*frame != stored_frame {
        return Err(refused("reconstructed original admission or frame differs from rooted history"));
    }
    Ok(RestoredProtectedOriginalV3 {
        original: restored,
        literal: wire.companion_v3().map_err(contract)?.original,
        terms: *plan.terms_v3(),
    })
}

/// A context/signature regression using an admitted legacy companion. Supply
/// and carrier are explicit fixture inputs here, not evidence of protected
/// restoration, Full validation, physical booking or a paid native request.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(super) fn exercise_request_context_v3(
    original: &super::super::PreparedPublicOriginalV1,
    literal: &OriginalCommit,
    member: misaka_palw_positive_contract::public_one_honest::PublicDrawMemberV1,
    draw_point: PalwBlockContextV2,
    at: PalwBlockContextV2,
    key: &[u8; misaka_palw_positive_contract::identity::PUBKEY_BYTES],
    sign: impl Fn(
        &misaka_palw_positive_contract::public_one_honest::ManifestRequest,
    ) -> [u8; misaka_palw_positive_contract::identity::SIGNATURE_BYTES],
) {
    use super::manifest_carrier::{ManifestRequestCarrierContextV1, ManifestRequestCarrierScopeV1};
    use crate::Hash64;
    use misaka_palw_positive_contract::{identity::Digest, public_one_honest::ManifestV1};
    let scope = stateless_wide_replay_plan_v1();
    let draw = original.prepare_draw(Digest([81; 64]), draw_point, &[member], scope).unwrap();
    let supply = (Digest([82; 64]), u64::from(ManifestV1::WIRE_BYTES));
    let claimant = Claimant::DrawnBond(member.bond);
    let carrier = ManifestRequestCarrierContextV1 {
        scope: ManifestRequestCarrierScopeV1 {
            claim: Hash64::from_u64_word(83),
            source: Hash64::from_u64_word(84),
            reservation_scope: Hash64::from_u64_word(85),
        },
        at,
        carrier_block: Hash64::from_u64_word(86),
        carrier: Hash64::from_u64_word(87),
        accepted_ordinal: 3,
    };
    let context = native_request_context_v3(original, literal, &draw, claimant, supply, carrier, at).unwrap();
    assert_eq!(context.origin.carrier_block.0, carrier.carrier_block.as_bytes());
    assert_eq!(context.origin.carrier_transaction.0, carrier.carrier.as_bytes());
    assert_eq!(context.origin.accepted_tx_ordinal, 3);
    let request = original.drawn_manifest_request(&draw, literal, member.bond, scope, supply.0).unwrap();
    let verify = |request: &misaka_palw_positive_contract::public_one_honest::ManifestRequest| {
        original.verify_drawn_manifest_request(&draw, request, &context, key, &sign(request))
    };
    verify(&request).unwrap();
    // Valid signatures cannot promote the request's own fields into native
    // expectations. Each altered packet is signed anew by the selected key.
    let mut altered = request.clone();
    altered.supply_obligation = Digest([88; 64]);
    assert!(verify(&altered).is_err());
    altered = request.clone();
    altered.inspection_scope = Digest([89; 64]);
    assert!(verify(&altered).is_err());
    altered = request.clone();
    altered.reservation = Digest([90; 64]);
    assert!(verify(&altered).is_err());
    altered = request.clone();
    altered.collect_until += 1;
    assert!(verify(&altered).is_err());
    let wrong_bond = Claimant::DrawnBond(misaka_palw_positive_contract::public_one_honest::PanelBond {
        index: member.bond.index + 1,
        ..member.bond
    });
    assert!(native_request_context_v3(original, literal, &draw, wrong_bond, supply, carrier, at).is_err());
    assert!(native_request_context_v3(original, literal, &draw, Claimant::Outsider(member.principal), supply, carrier, at).is_err());
    let later = PalwBlockContextV2 { daa_score: at.daa_score + 1, ..at };
    assert!(native_request_context_v3(original, literal, &draw, claimant, supply, carrier, later).is_err());
    let foreign_draw = original.prepare_draw(Digest([81; 64]), draw_point, &[member], Digest([91; 64])).unwrap();
    assert!(native_request_context_v3(original, literal, &foreign_draw, claimant, supply, carrier, at).is_err());
    if let Ok(ordinal) = usize::try_from(u64::from(u32::MAX) + 1) {
        assert!(
            native_request_context_v3(
                original,
                literal,
                &draw,
                claimant,
                supply,
                ManifestRequestCarrierContextV1 { accepted_ordinal: ordinal, ..carrier },
                at
            )
            .is_err()
        );
    }
}

/// Exercise archive refusal using the existing real-signature legacy original
/// fixture. Matching archive DATA reaches the missing-protected-frame refusal;
/// it is deliberately not promoted to protected admission by this adapter.
#[cfg(test)]
pub(super) fn exercise_archive_scope_rejections_v3(
    current: &PalwChainStateV2,
    prefix: &PalwChainStateV2,
    pairing: &PalwFundedOriginalPairingV1,
    point: &PalwBlockContextV2,
    params: &PalwStateParamsV2,
    entries: &[crate::tx::UtxoEntry],
    fee: u64,
) {
    use crate::{
        Hash64,
        palw_court_close_receipt_v1::NativeCourtRulesV1,
        palw_native_resource_v1::{NativeChainStateV1, native_typed_value_requests_v1},
    };
    let history = rooted_tfp1_original_history_v1(current, pairing.claim()).unwrap().unwrap();
    let parent_block = Hash64::from_bytes(history.selected_parent.0);
    let mut parent = PalwChainStateV2::genesis();
    parent.set_last_point_for_tests(Some(PalwBlockContextV2 { block: parent_block, ..*point }));
    let rules = NativeCourtRulesV1 {
        network_domain: pairing.envelope().funding.terms.network_domain,
        at: *point,
        court: crate::palw_mode_v2::PalwCourtParamsV2::new(100, 10, 2).unwrap(),
        step_ladder: 1,
        prompt_ids_merkle: false,
        held_regime: false,
        decode_close_convicts_only: false,
        fp_decode_from_daa: None,
        cell_closure_from_daa: None,
        memory_routing_from_daa: None,
        tir_active: true,
        gen_active: false,
        task_heads_active: false,
        improvement_active: false,
        constraint_active: false,
    };
    let extras = PalwTransitionExtrasV1 { native_court_rules_v1: Some(rules), ..Default::default() };
    let archive = |prefix: &PalwChainStateV2, fee| {
        let tx_lease =
            reserve_native_pending_requests_v1(native_typed_value_requests_v1(pairing.carrier()).unwrap()).unwrap().unwrap();
        let input_bytes = std::mem::size_of::<NativeAcceptedCaseCarriageV2>() + std::mem::size_of_val(entries);
        let lease = reserve_native_pending_requests_v1(input_bytes as u64).unwrap().unwrap();
        NativeAcceptedCaseCarriageV2::from_native_history_v2(
            pairing.carrier().clone(),
            tx_lease,
            entries.to_vec(),
            fee,
            history.origin,
            parent_block,
            lease,
        )
        .unwrap()
        .with_native_prefix_state_v2(std::sync::Arc::new(NativeChainStateV1::try_from_clone_v1(prefix).unwrap()), prefix.state_root())
        .unwrap()
    };
    let correct = archive(prefix, fee);
    assert!(
        matches!(
            restore_protected_native_carriage_v3(current, &parent, &correct, point, &extras, params),
            Err(NativeDecodeErrorV1::Decode(reason)) if reason == "archive is not a protected TFO3 original"
        ),
        "the node interchange cannot promote a legacy companion into protected admission"
    );
    let wrong_point = PalwBlockContextV2 { daa_score: point.daa_score + 1, ..*point };
    assert!(
        matches!(
            restore_protected_native_carriage_v3(current, &parent, &correct, &wrong_point, &extras, params),
            Err(NativeDecodeErrorV1::Decode(reason)) if reason.starts_with("archived original point:")
        ),
        "reject a substituted inclusion point before pairing decode"
    );
    let scope_error = |result: Result<RestoredProtectedOriginalV3, Error>| matches!(result, Err(NativeDecodeErrorV1::Decode(reason)) if reason == "original archive carrier, parent, prefix or rule point differs");
    assert!(matches!(restore_protected_native_original_v3(current, &parent, &correct, pairing, point, &extras, params),
        Err(NativeDecodeErrorV1::Decode(reason)) if reason == "protected original frame unavailable"));
    assert!(scope_error(restore_protected_native_original_v3(current, prefix, &correct, pairing, point, &extras, params)));
    assert!(scope_error(restore_protected_native_original_v3(
        current,
        &parent,
        &archive(&parent, fee),
        pairing,
        point,
        &extras,
        params
    )));
    let wrong_rules = PalwTransitionExtrasV1 {
        native_court_rules_v1: Some(NativeCourtRulesV1 { at: PalwBlockContextV2 { subsidy: point.subsidy + 1, ..*point }, ..rules }),
        ..Default::default()
    };
    assert!(scope_error(restore_protected_native_original_v3(current, &parent, &correct, pairing, point, &wrong_rules, params)));
    assert!(scope_error(restore_protected_native_original_v3(
        current,
        &parent,
        &correct,
        pairing,
        point,
        &PalwTransitionExtrasV1::default(),
        params
    )));
    assert!(
        matches!(restore_protected_native_original_v3(current, &parent, &archive(prefix, fee + 1), pairing, point, &extras, params),
        Err(NativeDecodeErrorV1::Decode(reason)) if reason.starts_with("archived protected original:"))
    );
    assert!(
        matches!(restore_protected_native_original_v3(&PalwChainStateV2::genesis(), &parent, &correct, pairing, point, &extras, params),
        Err(NativeDecodeErrorV1::Decode(reason)) if reason == "protected original history unavailable")
    );
}
