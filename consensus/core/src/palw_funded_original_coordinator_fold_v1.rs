// Included in the private native original fold (`palw_tir_public_fold_v1`). No wire, DTO or caller constructor.
//
// **G14 item 1 / RFC-0035 A §3, §7, §9.1: the funded-original coordinator.**
//
// Seam: `apply_object_observed_v2`'s paired checkpoint → `apply_object_inner` → the FreePromptCommitted arm, right after
// `prepare_paired_original_v1` and BEFORE the first `reserve_for_claim`. Everything is derived from the native preparation that
// already succeeded there (`PreparedOriginalFrameV1` → geometry → requirements → actor catalog → source graph), from the rooted
// state at THIS point of the block (not a parent snapshot), and from the actual carrier transaction (`PalwFundedOriginalPairingV1`).
// No caller flag, profile hash or self-signed manifest is policy: the payer's signed profile ids must EQUAL the schedule's, and the
// schedule has no production value below `tir_public_conviction_active` (typed-missing), so below that fence this refuses with the
// typed reason and the checkpoint leaves FP / frame / escrow deltas empty. Past the fence the pinned estimate is the schedule.
// The carrier remains an ordinary transaction; its funding output is the payer's
// own P2PKH and is spendable by the payer after the mergeset (the provisional lock is same-mergeset only).
//
// Install happens after the ordinary FP writes, inside the same checkpoint: the frame/preparation (`install_native_v1`, the
// existing writer), then the rooted escrow row (`install_funded_escrow_v1`). Any error restores the checkpoint, so FP, frame,
// preparation, rooted graph identity, consents and the escrow row commit together or not at all (RFC-0035 §3 "一括更新").
use crate::palw_funded_original_coordinator_v1::{
    FundedBatchScheduleV1, PALW_FUNDED_ROLE_FEE_CAPITAL_BODIES_V1, PALW_FUNDED_ROLE_SEMANTIC_PROVIDER_V1,
    PALW_FUNDED_ROLE_SERVICE_OPERATOR_V1, PALW_FUNDED_ROLE_WHOLE_BRANCH_V1, PalwFundedOriginalMissingV1 as FundedMissingV1,
    palw_funded_original_schedule_v1,
};
use crate::palw_court_shared_material_intake_v1::batch::admission::BatchAdmissionQuoteV1;
use crate::palw_court_service_release_v1::CourtServiceRowV3;
use crate::palw_funded_original_admission_v1::FundedAdmissionScheduleV1;

/// Typed refusal of the coordinator. `Missing` = a contract/value does not exist (never a fraud finding); `Invalid` = the carrier or
/// its consents do not match the native derivation; `State` = an inner native error; `Resource` = local coverage (never invalid).
#[derive(Debug)]
pub(super) enum FundedOriginalRefusalV1 {
    Missing(FundedMissingV1),
    Invalid(&'static str),
    State(PalwStateV2Error),
    Resource(crate::palw_native_resource_v1::NativeCoverageV1),
}
impl FundedOriginalRefusalV1 {
    fn into_state_v1(self) -> PalwStateV2Error {
        match self {
            Self::Resource(coverage) => PalwStateV2Error::NativeResource(coverage),
            Self::State(e) => e,
            Self::Missing(m) => refused(format!("RFC-0035 A funded original refused: typed-missing {m:?}")),
            Self::Invalid(why) => refused(format!("RFC-0035 A funded original refused: {why}")),
        }
    }
}
fn geometry_refusal(e: PreparedGeometryErrorV1) -> FundedOriginalRefusalV1 {
    match e {
        PreparedGeometryErrorV1::State(s) => FundedOriginalRefusalV1::State(s),
        PreparedGeometryErrorV1::Native(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(c)) => {
            FundedOriginalRefusalV1::Resource(c)
        }
        PreparedGeometryErrorV1::Native(crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(_)) => {
            FundedOriginalRefusalV1::Invalid("native original geometry")
        }
        PreparedGeometryErrorV1::MissingSharedServiceNamespace => {
            FundedOriginalRefusalV1::Missing(FundedMissingV1::SharedServiceNamespace)
        }
        PreparedGeometryErrorV1::MissingNativeRegistrationPolicy => {
            FundedOriginalRefusalV1::Missing(FundedMissingV1::NativeRegistrationPolicy)
        }
    }
}
fn requirements_refusal(e: PreparedRequirementsErrorV1) -> FundedOriginalRefusalV1 {
    match e {
        PreparedRequirementsErrorV1::Geometry(g) => geometry_refusal(g),
        PreparedRequirementsErrorV1::Shape(_) => FundedOriginalRefusalV1::Invalid("native original family shape"),
        PreparedRequirementsErrorV1::Native(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(c)) => {
            FundedOriginalRefusalV1::Resource(c)
        }
        PreparedRequirementsErrorV1::Native(crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(_)) => {
            FundedOriginalRefusalV1::Invalid("native original requirements")
        }
        PreparedRequirementsErrorV1::MissingSharedServiceNamespace => {
            FundedOriginalRefusalV1::Missing(FundedMissingV1::SharedServiceNamespace)
        }
    }
}

/// The checked, not-yet-written funded original: native requirements (frame + Source tickets alive) and the fresh escrow row.
/// No Clone/Borsh; consumed only by [`install_original_admission_v1`] inside the same checkpoint.
pub(super) struct FundedOriginalPlanV1 {
    requirements: PreparedOriginalRequirementsV1,
    row: crate::palw_funded_escrow_v1::PalwFundedEscrowRowV1,
    /// G14 item 1 (lane r5): every legal branch, its packets, resources and capital, derived from the native preparation.
    derivation: crate::palw_funded_original_admission_v1::FundedAdmissionDerivationV1,
    admission: crate::palw_funded_original_admission_v1::FundedAdmissionScheduleV1,
    signers: std::collections::BTreeSet<PalwBondKeyV2>,
    family: Vec<super::palw_court_family_supplier_fold_v1::FamilySupplierPlanV1>,
    semantic_table: Option<crate::palw_native_resource_v1::NativeRetainedV1<Vec<u8>>>,
    family_evidence: Option<PreparedFamilyActorEvidenceV1>,
    _retention_lease: Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>,
    archive: Option<
        crate::palw_native_resource_v1::NativeRetainedV1<original_archive_reservation_proposal_v1::NativeOriginalArchiveReservationV1>,
    >,
    _archive_lease: Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>,
    batch: Option<PreparedBatchFundingV1>,
}

struct PreparedBatchFundingV1 {
    service_before: Option<Row>,
    book: CourtServiceRowV3,
    quote: BatchAdmissionQuoteV1,
}

/// Checked native catalog evidence, produced only after joint supplier signatures,
/// actual escrow funding and complete rooted service reservation have been checked.
/// Anonymous potential slots confer no authority; only literal legacy signers and
/// the separately signed material suppliers are responsible for their own duties.
struct PreparedFamilyActorEvidenceV1 {
    claim: Hash64,
    allocation: Hash64,
    source: Hash64,
    graph: Hash64,
}
impl PreparedFamilyActorEvidenceV1 {
    fn derive_v1(
        requirements: &PreparedOriginalRequirementsV1,
        family: &[super::palw_court_family_supplier_fold_v1::FamilySupplierPlanV1],
        derivation: &crate::palw_funded_original_admission_v1::FundedAdmissionDerivationV1,
        book: &crate::palw_court_service_state_v2::CourtServiceStateV2,
        escrow: &crate::palw_funded_escrow_v1::PalwFundedEscrowRowV1,
        required: u64,
    ) -> Result<Self, FundedOriginalRefusalV1> {
        use FundedOriginalRefusalV1::Invalid;
        let frame = requirements.frame_v1();
        let plan = frame.plan_v1();
        let graph = requirements.source_graph_v1().map_err(requirements_refusal)?.scope_v3();
        let source = requirements.geometry.source_v1().source_id_v1();
        if family.is_empty()
            || escrow.claim != plan.original_claim
            || escrow.accepted_daa != plan.terms.accepted_daa
            || escrow.amount_sompi < required
            || !escrow.liability_open
        {
            return Err(Invalid("family catalog lacks actual funded original"));
        }
        let binding = book.claims.get(&plan.original_claim).ok_or(Invalid("family catalog lacks atomic original reservation"))?;
        let material_barrier =
            super::palw_court_family_supplier_fold_v1::family_material_barrier_v2(family).map_err(FundedOriginalRefusalV1::State)?;
        for packet in derivation
            .signer_packets
            .iter()
            .chain(&derivation.source_packets)
            .chain(std::iter::once(&derivation.retention_packet))
            .chain(&derivation.retention_tail_v2)
        {
            let mut expected = packet.clone();
            if material_barrier != 0 && expected.cost.io > 0 {
                expected.release_daa = expected.release_daa.max(material_barrier);
            }
            if !binding.packet_ids.contains(&packet.id) || book.book.packet(&packet.id) != Some(&expected) {
                return Err(Invalid("family catalog original branch lacks exact reservation"));
            }
        }
        for contract in family {
            let t = &contract.row.consent.terms;
            if t.claim != plan.original_claim
                || t.source != source
                || t.relative_graph != graph
                || contract.row.allocation != plan.allocation_id
                || contract.row.accepted_daa != plan.terms.accepted_daa
            {
                return Err(Invalid("family catalog material actor changes native scope"));
            }
            for packet in &contract.packets {
                if !binding.packet_ids.contains(&packet.id) || book.book.packet(&packet.id) != Some(packet) {
                    return Err(Invalid("family catalog supplier branch lacks exact reservation"));
                }
            }
        }
        Ok(Self { claim: plan.original_claim, allocation: plan.allocation_id, source, graph })
    }
    fn matches_v1(&self, other: &Self) -> bool {
        self.claim == other.claim && self.allocation == other.allocation && self.source == other.source && self.graph == other.graph
    }
}
impl FundedOriginalPlanV1 {
    #[cfg(test)]
    pub(super) fn row_v1(&self) -> &crate::palw_funded_escrow_v1::PalwFundedEscrowRowV1 {
        &self.row
    }
    #[cfg(test)]
    pub(super) fn derivation_v1(&self) -> &crate::palw_funded_original_admission_v1::FundedAdmissionDerivationV1 {
        &self.derivation
    }
}

fn admission_refusal(e: crate::palw_funded_original_admission_v1::FundedAdmissionRefusalV1) -> FundedOriginalRefusalV1 {
    use crate::palw_funded_original_admission_v1::FundedAdmissionRefusalV1 as A;
    match e {
        A::ActorWithoutAuthority => FundedOriginalRefusalV1::Invalid("a responsible actor has no current authority"),
        A::ScheduleInvalid => FundedOriginalRefusalV1::Missing(FundedMissingV1::AdmissionSchedule),
        other => FundedOriginalRefusalV1::State(refused(format!("RFC-0035 A all-branch reservation refused: {other:?}"))),
    }
}

/// The full rooted service-book candidate for this plan against the CURRENT book (lane r5). `(after_signer_step, full)`.
fn funded_book_candidate_v1(
    b: &TransitionBuilder<'_>,
    requirements: &PreparedOriginalRequirementsV1,
    derivation: &crate::palw_funded_original_admission_v1::FundedAdmissionDerivationV1,
    admission: &crate::palw_funded_original_admission_v1::FundedAdmissionScheduleV1,
    signers: &std::collections::BTreeSet<PalwBondKeyV2>,
) -> Result<
    (crate::palw_court_service_state_v2::CourtServiceStateV2, crate::palw_court_service_state_v2::CourtServiceStateV2),
    FundedOriginalRefusalV1,
> {
    let graph = requirements.source_graph_v1().map_err(requirements_refusal)?;
    let plan = requirements.frame_v1().plan_v1();
    crate::palw_funded_original_admission_v1::funded_admission_book_candidate_v1(
        b.state.court_service_state_v1(),
        plan.original_claim,
        plan.allocation_id,
        plan.source_id,
        plan.terms.accepted_daa,
        signers,
        graph,
        derivation,
        admission,
    )
    .map_err(admission_refusal)
}

/// Complete the exact journal row before payer consent is checked. Batch
/// reservations extend this row, including its existing causal edges.
fn funded_journal_row_candidate_v1(
    b: &TransitionBuilder<'_>,
    requirements: &PreparedOriginalRequirementsV1,
    derivation: &crate::palw_funded_original_admission_v1::FundedAdmissionDerivationV1,
    mut full: crate::palw_court_service_state_v2::CourtServiceStateV2,
) -> Result<Row, FundedOriginalRefusalV1> {
    use crate::palw_court_service_release_v1::{COURT_SERVICE_ROW_VERSION_V3, CourtServiceRowV3};
    let edges = crate::palw_funded_original_admission_v1::funded_admission_causal_edges_v1(
        requirements.source_graph_v1().map_err(requirements_refusal)?, derivation,
    ).map_err(admission_refusal)?;
    let previous = b.state.court_service_row_v3_v1();
    if edges.is_empty() && previous.is_none() {
        return Ok(Row::CourtService(Box::new(full).into()));
    }
    full.version = COURT_SERVICE_ROW_VERSION_V3;
    let mut row = CourtServiceRowV3 {
        version: COURT_SERVICE_ROW_VERSION_V3,
        state: full,
        resolved: previous.map(|row| row.resolved.clone()).unwrap_or_default(),
        causal: previous.map(|row| row.causal.clone()).unwrap_or_default(),
    };
    for (packet, predecessors) in edges {
        if row.causal.insert(packet, predecessors).is_some() {
            return Err(FundedOriginalRefusalV1::Invalid("new court reservation replaces an existing causal edge"));
        }
    }
    row.check().map_err(|why| FundedOriginalRefusalV1::State(refused(why)))?;
    Ok(Row::CourtServiceV3(Box::new(row).into()))
}

/// Inputs retained across the native claim writes and consumed inside the
/// same paired checkpoint. Complete protected-profile planning is a separate
/// intake prerequisite. This owner only prepares an installation.
pub(super) struct PreparedProtectedOriginalInstallationV1 {
    frame: crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_positive_contract_v2::PublicTfp1FrameV1>,
    history: crate::palw_positive_contract_v2::tfp1_original::native_link::PreparedTfp1OriginalHistoryV1,
    service: crate::palw_positive_contract_v2::tfp1_original::service_booking::PreparedProtectedServiceBookingV1,
    escrow: crate::palw_funded_escrow_v1::PalwFundedEscrowRowV1,
    prefix: super::objects::Tfp1OriginalPrefixV1,
}

impl PreparedProtectedOriginalInstallationV1 {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_v1(
        b: &TransitionBuilder<'_>,
        ctx: &PalwBlockContextV2,
        frame: crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_positive_contract_v2::PublicTfp1FrameV1>,
        service: crate::palw_positive_contract_v2::tfp1_original::service_booking::PreparedProtectedServiceBookingV1,
        funding: &crate::palw_court_original_escrow_v1::PreparedEscrowFundingV1<'_>,
        pairing: &crate::palw_funded_original_coordinator_v1::PalwFundedOriginalPairingV1,
        admission: &crate::palw_positive_contract_v2::PublicOriginalAdmissionV1<'_>,
        reservation: &crate::palw_positive_contract_v2::PublicOriginalReservationV1<'_>,
    ) -> Result<Self, PalwStateV2Error> {
        use crate::palw_native_resource_v1::{TirNativeRequestsV1, reserve_native_pending_requests_v1};
        use crate::palw_positive_contract_v2::tfp1_original::service_graph::check_frame_service_graph_v2;
        use misaka_palw_positive_contract::public_one_honest::{OriginalClock, SignedPublicOriginalV1};

        let bad = || refused("protected original install inputs do not match the current paired acceptance");
        let claim = frame.claim_v1();
        let prefix = b.tfp1_original_prefix.ok_or_else(bad)?;
        let accepted = pairing.history_inputs_v1().ok_or_else(bad)?;
        let current_pairing = b.extras.tir_funded_originals.get(&claim).ok_or_else(bad)?;
        if b.paired_original_checkpoint_claim != Some(claim)
            || b.state.claim(&claim).is_some()
            || prefix.claim_v1() != claim
            || prefix.point_v1() != *ctx
            || prefix.end_v1() != b.entries.len()
            || b.tfp1_history_journal_parent != Some(prefix.parent_v1())
            || current_pairing.carrier().id() != pairing.carrier().id()
            || current_pairing.history_inputs_v1() != Some(accepted)
            || pairing.claim() != claim
            || accepted.origin_v1() != frame.origin_v1()
            || reservation.origin != frame.origin_v1()
            || admission.job != &frame.header_v1().job_context
            || frame.service_graph_v2().is_none()
            || service.claim_v1() != claim
            || service.before_v1() != b.state.staged_court_row_v1(&Key::CourtService)
        {
            return Err(bad());
        }
        let binding = service.row_v1().state.claims.get(&claim).ok_or_else(bad)?;
        check_frame_service_graph_v2(&frame, service.row_v1(), binding).map_err(|_| bad())?;
        let literal = SignedPublicOriginalV1::decode(pairing.public_companion_v3().ok_or_else(bad)?).map_err(|_| bad())?;
        if literal.original.id() != frame.original_id_v1() {
            return Err(bad());
        }

        let terms = &funding.consent().terms;
        let actual = funding.actual();
        if funding.accepted_daa() != ctx.daa_score
            || funding.consent() != &pairing.envelope().funding
            || funding.payer_pubkey() != pairing.envelope().payer_pubkey.as_slice()
            || terms.claim != claim
            || terms.network_domain != frame.original_v1().job.network_domain
            || terms.job_identity != crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&frame.header_v1().job_context)
            || terms.execution_root != frame.original_v1().execution_root
            || terms.source_id != frame.source_v1()
            || terms.material_until_daa < frame.material_until_daa_v1()
            || actual.outpoint() != pairing.outpoint()
            || actual.output_index() != terms.funding_output_index
            || Some(actual.output()) != pairing.carrier().outputs.get(terms.funding_output_index as usize)
            || actual.output().value != terms.funding_amount_sompi
        {
            return Err(bad());
        }
        let hard =
            OriginalClock::new(ctx.daa_score).map_err(|_| PalwStateV2Error::Overflow("protected original hard deadline"))?.hard();
        let escrow = crate::palw_funded_escrow_v1::PalwFundedEscrowRowV1::fresh_v1(
            terms.id_v1(),
            claim,
            actual.outpoint(),
            actual.output().value,
            terms.payer_key_id,
            terms.refund_payload,
            ctx.daa_score,
            hard,
            true,
        )
        .map_err(|_| bad())?;
        let obligation = frame
            .native_facts_v1()
            .and_then(|facts| facts.service_obligation_v1(frame.profile_v1(), frame.resource_v1()))
            .map_err(|_| bad())?;
        // Signed history authentication reuses the legacy context/class
        // identity helpers, which serialize temporary hash preimages.
        let mut requests = TirNativeRequestsV1::new();
        for bytes in [borsh::object_length(admission.job), borsh::object_length(&admission.class.layout)] {
            requests.vec_v1::<u8>((bytes.map_err(|_| bad())? as u64).max(1024)).map_err(PalwStateV2Error::NativeResource)?;
        }
        let _hash_scratch = reserve_native_pending_requests_v1(requests.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
        let history = admission
            .prepare_tfp1_history_v1(ctx, pairing, obligation, reservation, frame.reservation_v1(), &prefix)
            .map_err(|_| bad())?;
        Ok(Self { frame, history, service, escrow, prefix })
    }
}

/// The preparation consumed by the FP arm's common installation point.
pub(super) enum OriginalAdmissionV1 {
    Plain(PreparedOriginalFrameV1),
    Funded(Box<FundedOriginalPlanV1>),
    Protected(PreparedProtectedOriginalInstallationV1),
}

impl OriginalAdmissionV1 {
    fn legacy_frame_v1(&self) -> Option<&Frame> {
        match self {
            Self::Plain(p) => Some(p.frame_v1()),
            Self::Funded(f) => Some(f.requirements.geometry_v1().frame_v1()),
            Self::Protected(_) => None,
        }
    }

    pub(super) fn material_until_daa_v1(&self) -> u64 {
        match self {
            Self::Plain(p) => p.frame_v1().plan_v1().terms.material_until_daa,
            Self::Funded(f) => f.requirements.geometry_v1().frame_v1().plan_v1().terms.material_until_daa,
            Self::Protected(p) => p.frame.material_until_daa_v1(),
        }
    }

    pub(super) fn retirement_material_v1(
        &self,
        id: Hash64,
        claim: &PalwClaimStateV2,
    ) -> Result<(Option<u64>, Option<u64>), PalwStateV2Error> {
        let bad = || PalwStateV2Error::BondBudgetRefused("prepared original retirement source does not match native claim".into());
        let claim_material = if let Self::Protected(p) = self {
            let frame = &p.frame;
            let origin = frame.origin_v1();
            let fp = frame.original_v1();
            if frame.claim_v1() != id
                || origin.accepting_block.0 != claim.accepted_block.as_bytes()
                || origin.accepted_daa != claim.accepted_daa
                || origin.accepted_blue_score != claim.accepted_blue_score
                || frame.producer_v1() != claim.bond
                || !matches!(&claim.source, PalwClaimSourceV2::FreePrompt { .. })
                || fp.job.class_id != claim.class_id
                || fp.execution_root != claim.execution_root
                || fp.trace_root != claim.trace_root
                || fp.output_root != claim.output_root
                || fp.work_leaves != claim.work_leaves
                || crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&frame.header_v1().job_context) != claim.job_identity
            {
                return Err(bad());
            }
            None
        } else {
            let frame = self.legacy_frame_v1().ok_or_else(bad)?;
            let plan = frame.plan_v1();
            let fp = frame.original_v1();
            if plan.original_claim != id
                || plan.terms.accepted_daa != claim.accepted_daa
                || frame.custody_v1().pledge_v1().producer != claim.bond
                || fp.job.class_id != claim.class_id
                || fp.execution_root != claim.execution_root
                || fp.trace_root != claim.trace_root
                || fp.output_root != claim.output_root
            {
                return Err(bad());
            }
            plan.semantic.as_ref().map(|s| s.terms.material_until_daa)
        };
        Ok((claim_material, Some(self.material_until_daa_v1())))
    }
}

/// **The seam.** `prepared` is what `prepare_paired_original_v1` returned for this object. A claim with a funded pairing is coordinated
/// (and refused whole on any failure); a claim without one keeps the existing TFP1 path unchanged. A funded pairing with no prepared
/// original is refused: its literal TFP1 must never ride in as an unfunded ordinary claim.
pub(super) fn admit_original_v1(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim: Hash64,
    prepared: Option<PreparedOriginalFrameV1>,
) -> Result<Option<OriginalAdmissionV1>, PalwStateV2Error> {
    // A recognized companion cannot be stripped and admitted under the old
    // funded profile. Replace this refusal only with complete native protected
    // planning and atomic TFP1/companion installation, never a DTO success flag.
    check_paired_original_profile_v1(b.extras, claim)?;
    let funded = b.extras.tir_funded_originals.contains_key(&claim);
    if (funded || prepared.is_some())
        && b.params.family_supplier_active_at(ctx.daa_score)
        && (!b.extras.family_supplier_active || b.extras.tir_funded_originals.get(&claim).is_none_or(|p| p.suppliers_v2().is_empty()))
    {
        return Err(refused("new public original requires complete TFO2 supplier contracts past its own fence"));
    }
    match (prepared, funded) {
        (None, false) => Ok(None),
        (Some(p), false) => Ok(Some(OriginalAdmissionV1::Plain(p))),
        (None, true) => Err(refused("RFC-0035 A funded original has no native paired preparation")),
        (Some(p), true) => coordinate_funded_original_v1(b, ctx, claim, p)
            .map(|plan| Some(OriginalAdmissionV1::Funded(Box::new(plan))))
            .map_err(FundedOriginalRefusalV1::into_state_v1),
    }
}

/// Seat and relay payouts for an admitted escrow. The seat is the first drawn panel bond with a payout payload; if the panel
/// names none, the producer's payout. The relay is the producer's payout when it differs, otherwise that same payload. Both are
/// rooted bond payouts, not new amounts. `None` when no payout is recorded.
fn funded_settlement_payees_v1(b: &TransitionBuilder<'_>, frame: &Frame) -> Option<(Hash64, Hash64)> {
    let payout =
        |key: &PalwBondKeyV2| b.state.bond(key).map(|bond| bond.payout_payload).filter(|payload| *payload != Hash64::default());
    let seat = frame.custody_v1().drawn_panel_v1().iter().find_map(payout);
    let producer = payout(&frame.custody_v1().pledge_v1().producer);
    let seat = seat.or(producer)?;
    let relay = producer.filter(|payload| *payload != seat).unwrap_or(seat);
    Some((seat, relay))
}

#[path = "palw_tir_prepared_final_archive_view_v11.rs"]
mod archive_service_view_v11;
#[path = "palw_original_archive_reservation_proposal_v1.rs"]
pub(super) mod original_archive_reservation_proposal_v1;
#[cfg(test)]
#[path = "palw_tir_prepared_final_service_facts_adapter_v11.rs"]
mod prepared_final_service_facts_adapter_v11;

/// Payer choices only. Native geometry, tariffs and paid authority are never
/// supplied by this value. Existing signed profiles retain their old meaning.
#[derive(Clone, Copy, Debug)]
pub struct PalwOriginalArchivePayerBudgetV1 {
    pub additional_carrier_rent_sompi: u64,
    pub dedicated_archive_capital_sompi: u64,
}
/// Read-only native construction result. This is not a funded receipt or an
/// accepted birth. Apply it to the existing TFP/FundingTerms constructors and
/// obtain the same payer's normal FundingConsent signature before broadcasting.
#[derive(Clone, Copy, Debug)]
pub struct PalwOriginalArchiveNativeQuoteV1 {
    claim: Hash64,
    source: Hash64,
    relative_graph: Hash64,
    material_until: u64,
    profiles: [Hash64; 4],
    declared_rent: u64,
    required_funding: u64,
    input_capacity: u64,
}
impl PalwOriginalArchiveNativeQuoteV1 {
    pub fn material_until_daa_v1(&self) -> u64 {
        self.material_until
    }
    pub fn profiles_v1(&self) -> [Hash64; 4] {
        self.profiles
    }
    pub fn declared_rent_sompi_v1(&self) -> u64 {
        self.declared_rent
    }
    pub fn funding_output_amount_sompi_v1(&self) -> u64 {
        self.required_funding
    }
    pub fn input_archive_capacity_bytes_v1(&self) -> u64 {
        self.input_capacity
    }
    pub fn bind_existing_funding_terms_v1(
        &self,
        t: &mut crate::palw_court_original_escrow_v1::FundingTermsV1,
    ) -> Result<(), PalwStateV2Error> {
        if (t.claim, t.source_id, t.relative_graph_id) != (self.claim, self.source, self.relative_graph)
            || t.material_until_daa > self.material_until
        {
            return Err(refused("archive quote changes actual original funding scope"));
        }
        t.material_until_daa = self.material_until;
        t.fee_profile_id = self.profiles[0];
        t.capital_profile_id = self.profiles[1];
        t.retention_profile_id = self.profiles[2];
        t.settlement_policy_id = self.profiles[3];
        t.funding_amount_sompi = self.required_funding;
        Ok(())
    }
}
/// Normal construction API, after the existing family v3 inventory/consent
/// quote and before broadcasting. `extras` contains the actual signed draft
/// TFP/TFO/family consent pairing. It may use the original base profiles; this
/// read-only quote installs nothing. Rebuild the TFP literal with returned rent,
/// bind returned terms and re-sign the same existing FundingConsentV1 context,
/// then rebuild the same TFO2 carrier. Supplier duties/fees stay unchanged.
/// Future block inputs are not fabricated: the payer reserves finite capacity
/// and the actual native full fold must fit it before publication.
pub fn palw_original_archive_native_quote_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    extras: &PalwTransitionExtrasV1,
    object: &PalwConsensusObjectV2,
    budget: PalwOriginalArchivePayerBudgetV1,
) -> Result<PalwOriginalArchiveNativeQuoteV1, PalwStateV2Error> {
    original_archive_native_quote_internal_v2(state, params, ctx, extras, object, budget, false)
}
/// Quote value layout is unchanged; only the explicit constructor selects OAR2.
/// This alias grants no payment or accepted-source standing.
pub type PalwOriginalArchiveNativeQuoteV2 = PalwOriginalArchiveNativeQuoteV1;
/// Same payer fields/units, distinct OAR2 profile and version2 receipt capacity.
pub fn palw_original_archive_native_quote_v2(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    extras: &PalwTransitionExtrasV1,
    object: &PalwConsensusObjectV2,
    budget: PalwOriginalArchivePayerBudgetV1,
) -> Result<PalwOriginalArchiveNativeQuoteV2, PalwStateV2Error> {
    original_archive_native_quote_internal_v2(state, params, ctx, extras, object, budget, true)
}
fn original_archive_native_quote_internal_v2(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    extras: &PalwTransitionExtrasV1,
    object: &PalwConsensusObjectV2,
    budget: PalwOriginalArchivePayerBudgetV1,
    mixed: bool,
) -> Result<PalwOriginalArchiveNativeQuoteV1, PalwStateV2Error> {
    let PalwConsensusObjectV2::FreePromptCommitted { public_job: Some(job), .. } = object else {
        return Err(refused("archive quote requires actual public original"));
    };
    let claim = crate::palw_freeprompt_v3::fp_claim_id_v3(job);
    let b = TransitionBuilder::try_new(state, params, false, false, false, false, extras)?.with_native_original_parent_v19(state);
    let prepared = prepare_paired_original_v1(&b, ctx, object, Some(job))?
        .ok_or_else(|| refused("archive quote lacks actual signed paired original"))?;
    quote_prepared_original_final_archive_v11(&b, ctx, claim, prepared, |view| {
        let ceilings = original_archive_reservation_proposal_v1::OriginalArchivePayerCeilingsV1 {
            additional_carrier_rent: budget.additional_carrier_rent_sompi,
            dedicated_escrow_capital: budget.dedicated_archive_capital_sompi,
        };
        let q = if mixed {
            original_archive_reservation_proposal_v1::quote_original_archive_reservation_v2(&b, ctx, view, object, ceilings)
        } else {
            original_archive_reservation_proposal_v1::quote_original_archive_reservation_v1(&b, ctx, view, object, ceilings)
        }?;
        let scope = view.scope_v11();
        Ok(PalwOriginalArchiveNativeQuoteV1 {
            claim,
            source: scope.1,
            relative_graph: scope.5,
            material_until: q.material_until_v1(),
            profiles: q.profiles_v1(),
            declared_rent: q.total_declared_rent_v1()?,
            required_funding: q.signed_funding_amount_v1()?,
            input_capacity: q.input_capacity_v1(),
        })
    })
    .map_err(FundedOriginalRefusalV1::into_state_v1)
}

// Diagnostic construction only: all ordinary signatures and actual native
// prepared facts are still required before the one real full fold.
#[cfg(test)]
fn palw_original_archive_native_quote_legacy_oar2_v3(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    extras: &PalwTransitionExtrasV1,
    object: &PalwConsensusObjectV2,
    budget: PalwOriginalArchivePayerBudgetV1,
) -> Result<PalwOriginalArchiveNativeQuoteV1, PalwStateV2Error> {
    let PalwConsensusObjectV2::FreePromptCommitted { public_job: Some(job), .. } = object else {
        return Err(refused("archive quote requires actual public original"));
    };
    let claim = crate::palw_freeprompt_v3::fp_claim_id_v3(job);
    let b = TransitionBuilder::try_new(state, params, false, false, false, false, extras)?.with_native_original_parent_v19(state);
    let prepared = prepare_paired_original_v1(&b, ctx, object, Some(job))?
        .ok_or_else(|| refused("archive quote lacks actual signed paired original"))?;
    quote_prepared_original_final_archive_v11(&b, ctx, claim, prepared, |view| {
        let ceilings = original_archive_reservation_proposal_v1::OriginalArchivePayerCeilingsV1 {
            additional_carrier_rent: budget.additional_carrier_rent_sompi,
            dedicated_escrow_capital: budget.dedicated_archive_capital_sompi,
        };
        let q = original_archive_reservation_proposal_v1::quote_original_archive_legacy_oar2_v3(&b, ctx, view, object, ceilings)?;
        let scope = view.scope_v11();
        Ok(PalwOriginalArchiveNativeQuoteV1 {
            claim,
            source: scope.1,
            relative_graph: scope.5,
            material_until: q.material_until_v1(),
            profiles: q.profiles_v1(),
            declared_rent: q.total_declared_rent_v1()?,
            required_funding: q.signed_funding_amount_v1()?,
            input_capacity: q.input_capacity_v1(),
        })
    })
    .map_err(FundedOriginalRefusalV1::into_state_v1)
}

fn prepare_funded_batch_book_v1(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    geometry: &PreparedOriginalGeometryV1,
    admission: &FundedAdmissionScheduleV1,
    batch: FundedBatchScheduleV1,
) -> Result<(CourtServiceRowV3, BatchAdmissionQuoteV1, Hash64, u64), FundedOriginalRefusalV1> {
    use crate::palw_batch_court_v1::BatchCourtErrorV1;
    use crate::palw_court_shared_material_intake_v1::{
        batch::{
            Stage,
            admission::{BatchDeliveryV1, reserve_native_batch_delivery_v1},
            constraint_stop::ConstraintStopBatchOperandsV1,
        },
        material_intake_schedule_v1,
    };
    let missing = FundedOriginalRefusalV1::Missing(FundedMissingV1::BatchSchedule);
    let service = b.state.court_service_state_v1().ok_or(FundedOriginalRefusalV1::Missing(FundedMissingV1::SharedServiceNamespace))?;
    if !b.extras.batch_court_active_v1 || !b.extras.tir_public_conviction_active
        || b.extras.batch_numerical_profile_v1.is_none_or(|profile| {
            profile.service_profile != service.profile_id || profile.budget != batch.delivery.numerical_verification
        })
    {
        return Err(missing);
    }
    let storage = material_intake_schedule_v1().map_err(|_| FundedOriginalRefusalV1::Missing(FundedMissingV1::BatchSchedule))?;
    let envelope = geometry.streaming_batch_envelope_v1(b, ctx, batch.cover, batch.maximum_ranges).map_err(geometry_refusal)?;
    let frame = geometry.frame_v1();
    let stop = ConstraintStopBatchOperandsV1::derive_v1(
        &b.state, b.extras, ctx.daa_score, envelope.scope_v1(), frame.original_v1(), frame.header_v1(),
    );
    let (base, book, base_scope) = crate::palw_funded_original_admission_v1::funded_batch_base_candidate_v1(
        &b.state, frame, admission,
    ).map_err(admission_refusal)?;
    let (quote, book) = reserve_native_batch_delivery_v1(
        book, frame, &envelope,
        stop.as_ref().map(|stop| (stop, batch.stop_verification, batch.stop_filing)),
        batch.delivery, storage,
        |stage, bytes| {
            let (index, units, work_rate, io_rate) = match stage {
                BatchDeliveryV1::Open { range_count, .. } => (0, range_count as u64, batch.open_work_per_range, 0),
                BatchDeliveryV1::Response { stage: Stage::Begin, .. } => (1, 0, 0, 0),
                BatchDeliveryV1::Response { stage: Stage::Part(_), .. } => (2, 0, 0, 0),
                BatchDeliveryV1::Response { stage: Stage::Finish, maximum_body_bytes, .. } => {
                    (3, maximum_body_bytes, batch.finish_work_per_byte, batch.finish_io_per_byte)
                }
            };
            let mut cost = batch.carrier_costs[index];
            let price = |base: u64, rate| units.checked_mul(rate).and_then(|extra| base.checked_add(extra))
                .ok_or_else(|| "Batch delivery service cost overflows".to_string());
            cost.bytes = bytes;
            cost.work = price(cost.work, work_rate)?;
            cost.io = price(cost.io, io_rate)?;
            Ok(cost)
        },
    ).map_err(|why| match why {
        BatchCourtErrorV1::LocalResource(coverage) => FundedOriginalRefusalV1::Resource(coverage),
        other => FundedOriginalRefusalV1::State(refused(format!("Batch Court admission reservation refused: {other:?}"))),
    })?;
    Ok((book, quote, base_scope, base.capital_sompi))
}

/// **The coordinator's checks — reads only; nothing is written.** Order: pairing/literal → schedule (the pinned estimate past
/// `tir_public_conviction_active`, typed-missing below it) → native geometry and requirements (budgets from the schedule) → actor
/// catalog (current authority, re-signed source pledge) and each missing role contract → source graph (service namespace) → native
/// expected scope → existing TFO1 inspection of the actual carrier + payer signature (`prepare_funding_v1`) → derived hard deadline
/// → fresh row and its uniqueness.
pub(super) fn coordinate_funded_original_v1(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim: Hash64,
    prepared: PreparedOriginalFrameV1,
) -> Result<FundedOriginalPlanV1, FundedOriginalRefusalV1> {
    type NoQuoteV11 =
        for<'v> fn(&archive_service_view_v11::PreparedOriginalArchiveServiceViewV11<'v>) -> Result<(), FundedOriginalRefusalV1>;
    match coordinate_original_scoped_archive_v11::<(), NoQuoteV11>(b, ctx, claim, prepared, None)? {
        std::ops::ControlFlow::Continue(plan) => Ok(plan),
        std::ops::ControlFlow::Break(()) => Err(FundedOriginalRefusalV1::Invalid("native funded path returned archive-only result")),
    }
}
/// Quote-only normal success. Callback cannot leak local complete-book refs;
/// an owned B/Item5 fact result is returned BEFORE funding inspection/debit.
/// The original funded path below retains EVERY original funding/evidence check.
fn quote_prepared_original_final_archive_v11<R, F>(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim: Hash64,
    prepared: PreparedOriginalFrameV1,
    consume: F,
) -> Result<R, FundedOriginalRefusalV1>
where
    F: for<'v> FnOnce(&archive_service_view_v11::PreparedOriginalArchiveServiceViewV11<'v>) -> Result<R, FundedOriginalRefusalV1>,
{
    match coordinate_original_scoped_archive_v11(b, ctx, claim, prepared, Some(consume))? {
        std::ops::ControlFlow::Break(facts) => Ok(facts),
        std::ops::ControlFlow::Continue(_) => {
            Err(FundedOriginalRefusalV1::Invalid("native archive-only path unexpectedly produced funded plan"))
        }
    }
}
fn coordinate_original_scoped_archive_v11<R, F>(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim: Hash64,
    prepared: PreparedOriginalFrameV1,
    quote: Option<F>,
) -> Result<std::ops::ControlFlow<R, FundedOriginalPlanV1>, FundedOriginalRefusalV1>
where
    F: for<'v> FnOnce(&archive_service_view_v11::PreparedOriginalArchiveServiceViewV11<'v>) -> Result<R, FundedOriginalRefusalV1>,
{
    use FundedOriginalRefusalV1::{Invalid, Missing as M};
    let pairing = b.extras.tir_funded_originals.get(&claim).ok_or(Invalid("no transaction-local funded pairing"))?;
    if pairing.public_companion_v3().is_some() {
        return Err(M(FundedMissingV1::PublicOriginalProfile));
    }
    let literal = b.extras.tir_public_originals.get(&claim).ok_or(Invalid("funded pairing without its literal TFP1"))?;
    let inner = pairing.envelope().original_v1().map_err(|_| Invalid("TFO1 literal original"))?;
    if pairing.claim() != claim || inner != *literal || prepared.frame_v1().plan_v1().original_claim != claim {
        return Err(Invalid("funded pairing, literal TFP1 and prepared original differ"));
    }
    let schedule = palw_funded_original_schedule_v1(b.extras).map_err(M)?;
    let batch_schedule = schedule.batch_v1();
    if b.extras.batch_court_active_v1 != batch_schedule.is_some() {
        return Err(M(FundedMissingV1::BatchSchedule));
    }
    let required_amount = schedule.required_amount_sompi().ok_or(Invalid("schedule amount overflow"))?;
    if required_amount == 0 {
        return Err(M(FundedMissingV1::Schedule));
    }
    let network = b.extras.tir_public_network_domain_v1.ok_or(M(FundedMissingV1::NetworkDomain))?;
    // Native geometry and requirements, all from the successful preparation; the budgets are part of the schedule.
    let geometry = prepared.enrich_native_geometry_v1(b, ctx).map_err(geometry_refusal)?;
    let mut requirements = geometry
        .prepare_requirements_v1(
            b,
            ctx,
            PreparedRequirementBudgetsV1 {
                numerical: schedule.numerical_budget(),
                input: schedule.input_budget(),
                source: schedule.source_budget(),
            },
        )
        .map_err(requirements_refusal)?;
    if !pairing.suppliers_v2().is_empty() {
        requirements.bind_carrier_inputs_v3(&pairing.carrier().inputs, b).map_err(FundedOriginalRefusalV1::State)?;
    }
    // Responsible actors: current authority re-derived (producer bond active, key, operator, pledge signature) and every role the
    // catalog names as un-contracted must be adopted by the schedule — never assumed for an unconsenting future actor.
    let actors = requirements.actor_catalog_v1(b, ctx).map_err(geometry_refusal)?;
    for missing in actors.missing_contracts_v1() {
        let (bit, typed) = match missing {
            PreparedRoleContractMissingV1::CanonicalWholeBranchActorAssignment => {
                (PALW_FUNDED_ROLE_WHOLE_BRANCH_V1, FundedMissingV1::CanonicalWholeBranchActorAssignment)
            }
            PreparedRoleContractMissingV1::SharedServiceOperatorConsent => {
                (PALW_FUNDED_ROLE_SERVICE_OPERATOR_V1, FundedMissingV1::SharedServiceOperatorConsent)
            }
            PreparedRoleContractMissingV1::FeeCapitalRetentionRecoveryBodies => {
                (PALW_FUNDED_ROLE_FEE_CAPITAL_BODIES_V1, FundedMissingV1::FeeCapitalRetentionRecoveryBodies)
            }
            PreparedRoleContractMissingV1::SemanticMaterialProviderConsent => {
                (PALW_FUNDED_ROLE_SEMANTIC_PROVIDER_V1, FundedMissingV1::SemanticMaterialProviderConsent)
            }
        };
        if !schedule.adopts_role(bit) && pairing.suppliers_v2().is_empty() {
            return Err(M(typed));
        }
    }
    let source_actor = actors.source_actor_v1();
    let frame = requirements.geometry_v1().frame_v1();
    let plan = frame.plan_v1();
    let pledge = frame.custody_v1().pledge_v1();
    if pledge.producer != source_actor.bond || pledge.id_v1() != source_actor.pledge_id {
        return Err(Invalid("source actor consent differs from the frame's pledge"));
    }
    // G14 item 1 (lane r5): the all-branch admission values are part of the unadopted schedule.
    let admission = schedule.admission_v1().ok_or(M(FundedMissingV1::AdmissionSchedule))?;
    // Responsible actors' current authority: every liable signer and every drawn Panel member must hold an ACTIVE bond whose
    // operator is still its own key's (the bond registration is the actor's signed standing consent; the producer's pledge
    // signature was re-verified by the catalog above). Never assumed for an absent or exiting actor.
    let signers = frame.custody_v1().liable_signers_v1();
    for actor in signers.iter().chain(frame.custody_v1().drawn_panel_v1().iter()) {
        let bond = b.state.bond(actor).ok_or(Invalid("a responsible actor has no bond"))?;
        if !matches!(bond.status, PalwBondStatusV2::Active) || bond.operator_id != palw_operator_id_v2(&bond.pubkey) {
            return Err(Invalid("a responsible actor has no current authority"));
        }
    }
    let graph = requirements.source_graph_v1().map_err(requirements_refusal)?;
    let deposit = plan.terms.plan.deposit_sompi;
    let material_until = plan.terms.material_until_daa;
    // Every legal branch (liable signer × turn, every native source-graph node, retention), its resources and capital; then a
    // dry-run reservation of ALL of them in a clone of the rooted book (all-or-nothing). The install re-derives against the book
    // as it stands at the install point.
    let (derivation, _retention_lease) = if !pairing.suppliers_v2().is_empty()
        && b.params.family_supplier_active_at(ctx.daa_score)
        && b.extras.family_supplier_active
    {
        crate::palw_funded_original_admission_v1::derive_funded_admission_family_v2(
            claim,
            plan.allocation_id,
            plan.source_id,
            plan.terms.accepted_daa,
            &signers,
            graph,
            &admission,
        )
        .map_err(|e| match e {
            crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(c) => {
                FundedOriginalRefusalV1::State(PalwStateV2Error::NativeResource(c))
            }
            crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(e) => admission_refusal(e),
        })?
    } else {
        (
            crate::palw_funded_original_admission_v1::derive_funded_admission_v1(
                claim,
                plan.allocation_id,
                plan.source_id,
                plan.terms.accepted_daa,
                &signers,
                graph,
                &admission,
            )
            .map_err(admission_refusal)?,
            None,
        )
    };
    let mut family = if pairing.suppliers_v2().is_empty() {
        Vec::new()
    } else {
        if !b.params.family_supplier_active_at(ctx.daa_score)
            || !b.extras.family_supplier_active
            || !b.params.settlement_once_active_at(ctx.daa_score)
            || !b.extras.staged_tir_route_active
        {
            return Err(Invalid("TFO2 family supplier settlement/material gates are dormant"));
        }
        requirements
            .prepare_family_plans_v1(b, ctx, pairing.suppliers_v2(), pairing.semantic_table_v2())
            .map_err(FundedOriginalRefusalV1::State)?
    };
    let (_, mut complete) = funded_book_candidate_v1(b, &requirements, &derivation, &admission, &signers)?;
    super::palw_court_family_supplier_fold_v1::reserve_family_plans_v1(&mut complete, claim, &mut family, graph)
        .map_err(FundedOriginalRefusalV1::State)?;
    if let Some(consume) = quote {
        let view = archive_service_view_v11::PreparedOriginalArchiveServiceViewV11::from_native_calendar_v11(
            &requirements,
            b,
            ctx,
            &family,
            &derivation,
            &complete,
        )?;
        // Normal quote-only return. No fresh escrow/funding/paid plan is made.
        // All native local requirements/complete/family/derivation leases drop
        // after the callback-owned result has been constructed.
        return Ok(std::ops::ControlFlow::Break(consume(&view)?));
    }
    let archive = if original_archive_reservation_proposal_v1::original_archive_terms_selected_v2(&pairing.envelope().funding.terms) {
        Some(original_archive_reservation_proposal_v1::prepare_archive_finance_at_native_v1(
            b,
            ctx,
            &requirements,
            &family,
            &derivation,
            &complete,
        )?)
    } else {
        None
    };
    let _archive_lease = archive
        .as_ref()
        .map(|q| original_archive_reservation_proposal_v1::reserve_original_archive_capacity_v1(&mut complete, q))
        .transpose()?
        .flatten();
    let batch = batch_schedule.map(|schedule| {
        let (book, quote, graph, capital) = prepare_funded_batch_book_v1(b, ctx, requirements.geometry_v1(), &admission, schedule)?;
        Ok::<_, FundedOriginalRefusalV1>((PreparedBatchFundingV1 {
            service_before: b.state.staged_court_row_v1(&Key::CourtService).cloned(),
            book,
            quote,
        }, graph, capital))
    }).transpose()?;
    let capital = batch.as_ref().map_or(derivation.capital_sompi, |(_, _, capital)| *capital);
    // Capital (RFC-0035 §7.1): Σ branch capital (≥ the schedule floor), less the producer deposit the native work room already
    // holds, plus the fee once.
    let required_amount = crate::palw_funded_original_admission_v1::funded_required_amount_v1(
        schedule.fee_sompi(),
        schedule.capital_floor_sompi(),
        capital,
        deposit,
    )
    .ok_or(Invalid("required capital overflow"))?;
    let required_amount = if let Some(q) = archive.as_ref() {
        required_amount.checked_add(q.capital_v1()).ok_or(Invalid("archive additional capital overflow"))?
    } else {
        required_amount
    };
    if required_amount == 0 {
        return Err(M(FundedMissingV1::Schedule));
    }
    let hard = if family.is_empty() && batch_schedule.is_none() {
        // Preserve the original TFO1/X3 contract. TFO2 has its own signed
        // native service graph and fixed original Court hard, also used by
        // the public pre-signing quote and every supplier response horizon.
        crate::palw_funded_escrow_v1::palw_funded_escrow_hard_deadline_v1(b.params.claim_hard_deadline_v1(), ctx.daa_score)
            .map_err(|_| M(FundedMissingV1::HardDeadline))?
    } else {
        ctx.daa_score
            .checked_add(crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA)
            .ok_or(Invalid("native family original hard overflow"))?
    };
    let mut expected = crate::palw_court_original_escrow_v1::OriginalFundingScopeV1 {
        claim,
        job_identity: pledge.job_identity,
        execution_root: frame.original_v1().execution_root,
        source_id: plan.source_id,
        relative_graph_id: batch.as_ref().map_or_else(|| graph.scope_v3(), |(_, graph, _)| *graph),
        fee_profile_id: archive.as_ref().map(|q| q.profiles_v1()[0]).unwrap_or_else(|| schedule.fee_profile_id()),
        capital_profile_id: archive.as_ref().map(|q| q.profiles_v1()[1]).unwrap_or_else(|| schedule.capital_profile_id()),
        retention_profile_id: archive.as_ref().map(|q| q.profiles_v1()[2]).unwrap_or_else(|| schedule.retention_profile_id()),
        settlement_policy_id: archive.as_ref().map(|q| q.profiles_v1()[3]).unwrap_or_else(|| schedule.settlement_policy_id()),
        required_amount_sompi: required_amount,
        required_material_until_daa: material_until.max(hard),
    };
    if let Some((PreparedBatchFundingV1 { quote, .. }, _, _)) = &batch {
        expected = expected.with_native_batch_v1(quote).map_err(|_| Invalid("native Batch funding scope"))?;
    }
    // Existing TFO1 inspection of the ACTUAL carrier (literal payload, index, amount, payer P2PKH), then the payer's signature over
    // exactly this native scope, network and acceptance interval.
    let envelope = pairing.envelope();
    let actual = envelope.inspect_carrier_v1(pairing.carrier()).map_err(|_| Invalid("actual carrier funding output"))?;
    if actual.outpoint() != pairing.outpoint() {
        return Err(Invalid("pairing outpoint differs from the actual carrier"));
    }
    let funding = crate::palw_court_original_escrow_v1::prepare_funding_v1(
        actual,
        &envelope.funding,
        &envelope.payer_pubkey,
        &expected,
        network,
        ctx.daa_score,
    )
    .map_err(|_| Invalid("payer consent does not cover the native scope, amount, horizon or interval"))?;
    let terms = &funding.consent().terms;
    let mut row = crate::palw_funded_escrow_v1::PalwFundedEscrowRowV1::fresh_v1(
        terms.id_v1(),
        claim,
        funding.actual().outpoint(),
        funding.actual().output().value,
        terms.payer_key_id,
        terms.refund_payload,
        ctx.daa_score,
        hard,
        // Lane r5: the liability opens at admission and closes only when `funded_liability_resolved_v1` holds
        // (`close_resolved_funded_liabilities_v1`, once per block). Payee paths resolve only once this row records the fee and
        // the seat and relay payouts, which happens only while the conviction fence is active.
        true,
    )
    .map_err(|_| Invalid("escrow row shape"))?;
    if archive.is_some() {
        row.version = crate::palw_funded_escrow_v1::PALW_FUNDED_ESCROW_ORIGINAL_ARCHIVE_ROW_VERSION_V1;
    }
    // The escrow settlement resolver has no fence argument of its own. Record the admitting schedule's fee and the rooted
    // payouts only while the same conviction fence that resolved the schedule is active, and only when the fee fits the escrow.
    if b.extras.tir_public_conviction_active {
        let (seat_payee, relay_payee) =
            funded_settlement_payees_v1(b, frame).ok_or(Invalid("funded original lacks native settlement payees"))?;
        let fee = schedule.fee_sompi();
        if fee == 0 || fee > row.amount_sompi {
            return Err(Invalid("funded original fee does not fit actual escrow"));
        }
        row.fee_sompi = fee;
        row.seat_payee = seat_payee;
        row.relay_payee = relay_payee;
        row.check_v1().map_err(|_| Invalid("funded original settlement policy"))?;
    }
    // Capacity: one row per contract and per outpoint (also re-checked by the writer).
    if b.state.funded_escrows_v1().contains_key(&row.contract_id)
        || b.state.funded_escrows_v1().values().any(|other| other.funding_outpoint == row.funding_outpoint)
    {
        return Err(Invalid("contract or funding outpoint already escrowed"));
    }
    // New contracts discharge the placeholder catalog only through checked native
    // evidence. The old TFO1 path keeps every existing missing-role refusal.
    let family_evidence = if family.is_empty() {
        None
    } else {
        Some(PreparedFamilyActorEvidenceV1::derive_v1(&requirements, &family, &derivation, &complete, &row, required_amount)?)
    };
    #[allow(clippy::drop_non_drop)]
    drop(funding);
    let semantic_table = pairing
        .semantic_table_owned_v2()
        .map(crate::palw_native_resource_v1::NativeRetainedV1::try_from_clone_v1)
        .transpose()
        .map_err(|e| FundedOriginalRefusalV1::State(PalwStateV2Error::NativeResource(e)))?;
    Ok(std::ops::ControlFlow::Continue(FundedOriginalPlanV1 {
        requirements,
        row,
        derivation,
        admission,
        signers,
        family,
        semantic_table,
        family_evidence,
        _retention_lease,
        archive,
        _archive_lease,
        batch: batch.map(|(prepared, _, _)| prepared),
    }))
}

/// **Install** at the existing original install point (after `reserve_for_claim` / `write_claim`), inside the same checkpoint.
pub(super) fn install_original_admission_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    admission: OriginalAdmissionV1,
    prompt: &PalwFpClaimPromptV1,
) -> Result<(), PalwStateV2Error> {
    match admission {
        OriginalAdmissionV1::Plain(prepared) => install_prepared_original_v1(b, ctx, prepared, prompt),
        OriginalAdmissionV1::Protected(plan) => {
            use crate::palw_court_clock_v2::{
                palw_claim_route_hard_v2, palw_court_clock_v2_governs, palw_court_clock_v2_reorg_retention,
                palw_court_clock_v2_required_retention_with,
            };

            let PreparedProtectedOriginalInstallationV1 { frame, history, service, escrow, prefix } = plan;
            let claim_id = frame.claim_v1();
            if b.paired_original_checkpoint_claim != Some(claim_id)
                || b.tfp1_original_prefix != Some(prefix)
                || prefix.point_v1() != *ctx
            {
                return Err(refused("protected original must install in its accepting paired checkpoint"));
            }
            let claim = b.state.claim(&claim_id).ok_or(PalwStateV2Error::MissingClaim(claim_id))?;
            if palw_claim_route_hard_v2(b.params, claim).is_some_and(|hard| hard != escrow.hard_deadline_daa) {
                return Err(refused("protected original changes the native dispute clock"));
            }
            if palw_court_clock_v2_governs(b.params, claim) {
                let required = palw_court_clock_v2_required_retention_with(
                    claim.accepted_daa,
                    palw_da_disclose_window_daa_v1(b.params),
                    palw_court_clock_v2_reorg_retention(b.extras.staged_public_court.as_ref()),
                )
                .map_err(|_| PalwStateV2Error::Overflow("protected original material horizon"))?;
                if frame.material_until_daa_v1() < required {
                    return Err(refused("protected original material expires before its native court obligation"));
                }
            }
            // The new frame has no legacy custody/PanelDa path. Its literal
            // carrier and authenticated manifest bind the retained input.
            // The ordinary FP prompt row remains owned by the FP arm.
            b.install_funded_escrow_v1(escrow)?;
            b.install_public_tfp1_original_frame_v1(ctx, frame, history)?;
            b.install_public_tfp1_service_booking_v1(ctx, service)?;
            b.check_funded_original_court_admission_native_v2(claim_id, ctx.daa_score)
        }
        OriginalAdmissionV1::Funded(plan) => {
            let FundedOriginalPlanV1 {
                requirements,
                row,
                derivation,
                admission,
                signers,
                mut family,
                semantic_table,
                family_evidence,
                _retention_lease,
                archive,
                _archive_lease,
                batch,
            } = *plan;
            if row.accepted_daa != ctx.daa_score || b.state.claim(&row.claim).is_none() || !row.liability_open {
                return Err(refused("RFC-0035 A funded install lacks this block's native FP acceptance"));
            }
            // Lane r5: re-derive the all-branch book candidate against the book as it stands NOW (before the frame install).
            if let Some(PreparedBatchFundingV1 { service_before, book, quote }) = batch {
                if !family.is_empty() || archive.is_some() || semantic_table.is_some() {
                    return Err(refused("Batch funding cannot carry legacy archive or supplier reservations"));
                }
                if b.state.staged_court_row_v1(&Key::CourtService) != service_before.as_ref() {
                    return Err(refused("Batch funding service row changed after payer precheck"));
                }
                let claim = b.state.claim(&row.claim).ok_or(PalwStateV2Error::MissingClaim(row.claim))?;
                if crate::palw_court_clock_v2::palw_claim_route_hard_v2(b.params, claim) != Some(row.hard_deadline_daa) {
                    return Err(refused("Batch Court requires the original's fixed dispute clock"));
                }
                requirements.into_geometry_v1().install_batch_frame_v1(b, ctx, prompt)?;
                super::palw_court_service_lifecycle_fold_v1::install_prepared_batch_frontier_v1(
                    b, ctx, service_before.as_ref(), book, &quote,
                )?;
                return b.install_funded_escrow_v1(row);
            }
            let before = b.state.court_service_state_v1().cloned();
            let (after_signer, mut full) = funded_book_candidate_v1(b, &requirements, &derivation, &admission, &signers)
                .map_err(FundedOriginalRefusalV1::into_state_v1)?;
            super::palw_court_family_supplier_fold_v1::reserve_family_plans_v1(
                &mut full,
                row.claim,
                &mut family,
                requirements.source_graph_v1().map_err(requirements_refusal).map_err(FundedOriginalRefusalV1::into_state_v1)?,
            )?;
            let _archive_install_lease = archive
                .as_ref()
                .map(|q| original_archive_reservation_proposal_v1::reserve_original_archive_capacity_v1(&mut full, q))
                .transpose()
                .map_err(FundedOriginalRefusalV1::into_state_v1)?
                .flatten();
            if let Some(evidence) = family_evidence.as_ref() {
                let checked =
                    PreparedFamilyActorEvidenceV1::derive_v1(&requirements, &family, &derivation, &full, &row, row.amount_sompi)
                        .map_err(FundedOriginalRefusalV1::into_state_v1)?;
                if !evidence.matches_v1(&checked) {
                    return Err(refused("family actor catalog changed before install"));
                }
            } else if !family.is_empty() {
                return Err(refused("family actor catalog evidence missing"));
            }
            // G14 lane r6-d1-release: the source branches' causal edges, recorded with the book (recovery keeps causal order).
            let edges = crate::palw_funded_original_admission_v1::funded_admission_causal_edges_v1(
                requirements.source_graph_v1().map_err(requirements_refusal).map_err(FundedOriginalRefusalV1::into_state_v1)?,
                &derivation,
            )
            .map_err(admission_refusal)
            .map_err(FundedOriginalRefusalV1::into_state_v1)?;
            requirements.into_geometry_v1().install_native_v1(b, ctx, prompt)?;
            // The ordinary install may have written court-f02's per-signer binding (only with a pinned F02 profile): it must be
            // exactly the signer prefix of this candidate; anything else is refused (the checkpoint restores every write).
            let now = b.state.court_service_state_v1();
            if now != before.as_ref() && now != Some(&after_signer) {
                return Err(refused("RFC-0035 A funded install: the court service book moved under the all-branch reservation"));
            }
            // Packets, court capacity, resources (in the retention packet) — all branches, one rooted write. G14 lane r6-d1-release:
            // with causal source edges the book is written in its V3 form carrying them (past the conviction fence, where every
            // funded admission is); without edges, in the form it already has.
            if edges.is_empty() {
                b.write_court_service_state_v1(full)?;
            } else {
                let mut next = match b.state.court_service_row_v3_v1() {
                    Some(released) => released.clone(),
                    None => crate::palw_court_service_release_v1::CourtServiceRowV3::from_v2(&full),
                };
                next.state = full;
                next.state.version = crate::palw_court_service_release_v1::COURT_SERVICE_ROW_VERSION_V3;
                next.causal.extend(edges);
                b.write_staged_row_v1(Key::CourtService, Some(Row::CourtServiceV3(Box::new(next).into())))?;
            }
            let claim_id = row.claim;
            // G14 lane F03b (ADR-0192 A/A): the shared frontier rides the same all-branch book (idempotent with the plain hook).
            super::palw_court_service_lifecycle_fold_v1::install_shared_frontier_context_v1(b, ctx, claim_id)?;
            let original_funding_outpoint = row.funding_outpoint;
            let original_archive_contract = row.contract_id;
            b.install_funded_escrow_v1(row)?;
            if let Some(table) = semantic_table.as_ref() {
                super::palw_court_family_material_fold_v1::install_family_table_inventory_v1(
                    b,
                    claim_id,
                    table.as_slice(),
                    original_funding_outpoint,
                )?;
            }
            super::palw_court_family_supplier_fold_v1::install_family_plans_v1(b, ctx, family)?;
            if semantic_table.is_some() {
                super::palw_court_family_material_fold_v1::complete_family_table_inventory_v1(b, ctx, claim_id)?;
            }
            // The paired object's checkpoint restores the complete install if
            // either reserved capacity or the distinct-claim cap refuses it.
            b.check_funded_original_court_admission_native_v2(claim_id, ctx.daa_score)?;
            if let Some(q) = archive.as_ref() {
                original_archive_reservation_proposal_v1::install_original_archive_receipt_v1(b, ctx, q, original_archive_contract)?;
            }
            if let Some(receipt) =
                original_archive_reservation_proposal_v1::verified_original_archive_reservation_v1(&b.state, claim_id, *ctx)?
            {
                let contract = receipt.contract_v1();
                super::palw_claim_local_born_tir_source_v16::capture_original_install_v16(b, ctx, claim_id, contract)?;
            } else {
                #[cfg(test)]
                if b.extras.original_archive_data_observer_v16 {
                    let contract = b
                        .extras
                        .tir_funded_originals
                        .get(&claim_id)
                        .ok_or_else(|| refused("test data observer lost actual original pairing"))?
                        .envelope()
                        .funding
                        .terms
                        .id_v1();
                    super::palw_claim_local_born_tir_source_v16::capture_original_install_v16(b, ctx, claim_id, contract)?;
                }
            }
            Ok(())
        }
    }
}

/// Native pre-signing quote at one explicit prospective original admission point.
/// Opaque inventories retain their native owners; this result is never authority
/// to accept a later carrier. Admission repeats scope, actors, funding and clocks.
pub struct PalwFamilyOriginalNativeQuoteV2 {
    requirements: PreparedOriginalRequirementsV1,
    inventories: Vec<crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_court_family_material_v1::NativeFamilyInventoryV1>>,
    required_funding: u64,
    hard: u64,
    material: u64,
    _inventory_lease: Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>,
    _retention_lease: Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>,
    _derivation: crate::palw_funded_original_admission_v1::FundedAdmissionDerivationV1,
}
impl PalwFamilyOriginalNativeQuoteV2 {
    pub fn inventories_v2(
        &self,
    ) -> &[crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_court_family_material_v1::NativeFamilyInventoryV1>] {
        &self.inventories
    }
    pub fn required_original_funding_v2(&self) -> u64 {
        self.required_funding
    }
    pub fn hard_until_v2(&self) -> u64 {
        self.hard
    }
    pub fn material_until_v2(&self) -> u64 {
        self.material
    }
    pub fn accepted_daa_v2(&self) -> u64 {
        self.requirements.frame_v1().plan_v1().terms.accepted_daa
    }
    pub fn original_plan_v2(&self) -> &crate::palw_tir_public_allocation_v1::PalwTirPublicAllocationPlanV1 {
        self.requirements.frame_v1().plan_v1()
    }
    pub fn relative_graph_v2(&self) -> Hash64 {
        self.inventories[0].relative_graph_v1()
    }
}
/// `extras.tir_public_originals` must carry the exact already-signed TFP1 and
/// native node-resolved rules. Supplier consent/funding is intentionally not
/// required yet: this is the bounded data needed before those parties sign.
pub fn palw_family_original_native_quote_v2(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    extras: &PalwTransitionExtrasV1,
    object: &PalwConsensusObjectV2,
    semantic_table: Option<&[u8]>,
) -> Result<PalwFamilyOriginalNativeQuoteV2, PalwStateV2Error> {
    palw_family_original_native_quote_with_purpose_v3(state, params, ctx, extras, object, semantic_table, None, false)
}
/// Explicit active generated-semantic quote. V2 remains the historical complete
/// supplier inventory preview; neither quote grants admission or funds.
pub type PalwFamilyOriginalNativeQuoteV3 = PalwFamilyOriginalNativeQuoteV2;
pub fn palw_family_original_native_quote_v3(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    extras: &PalwTransitionExtrasV1,
    object: &PalwConsensusObjectV2,
    semantic_table: Option<&[u8]>,
    carrier_inputs: &[crate::tx::TransactionInput],
) -> Result<PalwFamilyOriginalNativeQuoteV3, PalwStateV2Error> {
    palw_family_original_native_quote_with_purpose_v3(state, params, ctx, extras, object, semantic_table, Some(carrier_inputs), true)
}
fn palw_family_original_native_quote_with_purpose_v3(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    extras: &PalwTransitionExtrasV1,
    object: &PalwConsensusObjectV2,
    semantic_table: Option<&[u8]>,
    carrier_inputs: Option<&[crate::tx::TransactionInput]>,
    generated_semantic_v3: bool,
) -> Result<PalwFamilyOriginalNativeQuoteV2, PalwStateV2Error> {
    if !params.family_supplier_active_at(ctx.daa_score) || !extras.family_supplier_active || !extras.tir_public_conviction_active {
        return Err(refused("family native quote owning fence is dormant"));
    }
    let PalwConsensusObjectV2::FreePromptCommitted { public_job: Some(job), .. } = object else {
        return Err(refused("family native quote requires actual public original"));
    };
    if generated_semantic_v3 {
        let claim = crate::palw_freeprompt_v3::fp_claim_id_v3(job);
        if let Some(provenance) = extras.prefix_dependencies_v1.get(&claim) {
            crate::palw_claim_local_v1::prefix_v1::check_admission_v1(state, params, ctx, object, Some(provenance))?;
        }
    }
    let quoted =
        crate::palw_native_resource_v1::NativeRetainedV1::try_from_clone_v1(extras).map_err(PalwStateV2Error::NativeResource)?;
    let (mut quote_extras, _quote_ticket) = quoted.into_parts_v1();
    if let Some(bytes) = semantic_table {
        let sem = state.class_semantics.get(&job.job.class_id).ok_or_else(|| refused("family quote table semantics absent"))?;
        let record = state.tir_class_v1(&job.job.class_id).ok_or_else(|| refused("family quote table class absent"))?;
        let table =
            crate::palw_court_family_material_v1::prepare_family_table_inventory_v1(bytes, sem, record.logits_vocab).map_err(|e| {
                match e {
                    crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(e) => PalwStateV2Error::NativeResource(e),
                    crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(e) => refused(e),
                }
            })?;
        quote_extras.family_original_table_inventories_v2.insert(crate::palw_freeprompt_v3::fp_claim_id_v3(job), table);
    }
    let extras = &quote_extras;
    let b = TransitionBuilder::try_new(state, params, false, false, false, false, extras)?.with_native_original_parent_v19(state);
    let prepared = prepare_paired_original_v1(&b, ctx, object, Some(job))?
        .ok_or_else(|| refused("family native quote lacks signed paired original"))?;
    let schedule = palw_funded_original_schedule_v1(extras).map_err(|e| refused(format!("family native quote schedule: {e:?}")))?;
    let mut requirements = prepared
        .enrich_native_geometry_v1(&b, ctx)
        .map_err(geometry_refusal)
        .map_err(FundedOriginalRefusalV1::into_state_v1)?
        .prepare_requirements_v1(
            &b,
            ctx,
            PreparedRequirementBudgetsV1 {
                numerical: schedule.numerical_budget(),
                input: schedule.input_budget(),
                source: schedule.source_budget(),
            },
        )
        .map_err(requirements_refusal)
        .map_err(FundedOriginalRefusalV1::into_state_v1)?;
    if generated_semantic_v3 {
        requirements.bind_carrier_inputs_v3(
            carrier_inputs.ok_or_else(|| refused("profile3 native quote missing candidate carrier inputs"))?,
            &b,
        )?;
    }
    let (inventories, inventory_lease) = if generated_semantic_v3 {
        requirements.family_inventories_v3(&b, ctx, semantic_table)?
    } else {
        requirements.family_inventories_v1(&b, ctx, semantic_table)?
    };
    let frame = requirements.frame_v1();
    let p = frame.plan_v1();
    let admission = schedule.admission_v1().ok_or_else(|| refused("family native quote all-branch schedule absent"))?;
    let (derivation, retention_lease) = crate::palw_funded_original_admission_v1::derive_funded_admission_family_v2(
        p.original_claim,
        p.allocation_id,
        p.source_id,
        p.terms.accepted_daa,
        &frame.custody_v1().liable_signers_v1(),
        requirements.source_graph_v1().map_err(requirements_refusal).map_err(FundedOriginalRefusalV1::into_state_v1)?,
        &admission,
    )
    .map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => PalwStateV2Error::NativeResource(r),
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(e) => admission_refusal(e).into_state_v1(),
    })?;
    let required_funding = crate::palw_funded_original_admission_v1::funded_required_amount_v1(
        schedule.fee_sompi(),
        schedule.capital_floor_sompi(),
        derivation.capital_sompi,
        p.terms.plan.deposit_sompi,
    )
    .ok_or_else(|| refused("family native quote funding arithmetic"))?;
    let hard = ctx
        .daa_score
        .checked_add(crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA)
        .ok_or_else(|| refused("family native quote hard overflow"))?;
    let material = crate::palw_court_service_state_v2::tir_service_required_material_until_v1(frame, hard).map_err(refused)?;
    Ok(PalwFamilyOriginalNativeQuoteV2 {
        requirements,
        inventories,
        required_funding,
        hard,
        material,
        _inventory_lease: inventory_lease,
        _retention_lease: retention_lease,
        _derivation: derivation,
    })
}

/// Read-only verified identity/readiness. No constructor, funding authority or
/// detached actor claim is carried out of the native state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwOriginalArchiveStatusV1 {
    contract: Hash64,
    source: Hash64,
    constructed: bool,
    material_ready: bool,
}
impl PalwOriginalArchiveStatusV1 {
    pub fn contract_v1(&self) -> Hash64 {
        self.contract
    }
    pub fn source_v1(&self) -> Hash64 {
        self.source
    }
    pub fn constructed_v1(&self) -> bool {
        self.constructed
    }
    pub fn material_ready_v1(&self) -> bool {
        self.material_ready
    }
}
pub fn palw_original_archive_status_v1(
    state: &PalwChainStateV2,
    claim: Hash64,
    point: PalwBlockContextV2,
) -> Result<Option<PalwOriginalArchiveStatusV1>, PalwStateV2Error> {
    let Some(r) = original_archive_reservation_proposal_v1::verified_original_archive_reservation_v1(state, claim, point)? else {
        return Ok(None);
    };
    let mut material_ready = false;
    if r.constructed_v1() && !r.released_v1() {
        super::palw_claim_local_source_admission_v8::for_accepting_point_v8(state, claim, point)?
            .ok_or_else(|| refused("verified archive lacks exact Source8 birth"))?;
        super::palw_claim_local_born_tir_source_v16::verify_original_archive_body_bounds_v1(state, claim, point, r.body_bounds_v1())?;
        material_ready = true;
    }
    Ok(Some(PalwOriginalArchiveStatusV1 {
        contract: r.contract_v1(),
        source: r.source_v1(),
        constructed: r.constructed_v1(),
        material_ready,
    }))
}
