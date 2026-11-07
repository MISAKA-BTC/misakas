// Included in the private native original fold (`palw_tir_public_fold_v1`). No wire, DTO or caller constructor.
//
// **G14 item 1 / RFC-0035 A §3, §7, §9.1: the funded-original coordinator.**
//
// Seam: `apply_object_with_pair_v1`'s paired checkpoint → `apply_object_inner` → the FreePromptCommitted arm, right after
// `prepare_paired_original_v1` and BEFORE the first `reserve_for_claim`. Everything is derived from the native preparation that
// already succeeded there (`PreparedOriginalFrameV1` → geometry → requirements → actor catalog → source graph), from the rooted
// state at THIS point of the block (not a parent snapshot), and from the actual carrier transaction (`PalwFundedOriginalPairingV1`).
// No caller flag, profile hash or self-signed manifest is policy: the payer's signed profile ids must EQUAL the schedule's, and the
// schedule has no production value (typed-missing), so in every production build this refuses with the typed reason and the
// checkpoint leaves FP / frame / escrow deltas empty. The carrier remains an ordinary transaction; its funding output is the payer's
// own P2PKH and is spendable by the payer after the mergeset (the provisional lock is same-mergeset only).
//
// Install happens after the ordinary FP writes, inside the same checkpoint: the frame/preparation (`install_native_v1`, the
// existing writer), then the rooted escrow row (`install_funded_escrow_v1`). Any error restores the checkpoint, so FP, frame,
// preparation, rooted graph identity, consents and the escrow row commit together or not at all (RFC-0035 §3 "一括更新").
use crate::palw_funded_original_coordinator_v1::{
    PALW_FUNDED_ROLE_FEE_CAPITAL_BODIES_V1, PALW_FUNDED_ROLE_SEMANTIC_PROVIDER_V1, PALW_FUNDED_ROLE_SERVICE_OPERATOR_V1,
    PALW_FUNDED_ROLE_WHOLE_BRANCH_V1, PalwFundedOriginalMissingV1 as FundedMissingV1, palw_funded_original_schedule_v1,
};

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
        PreparedGeometryErrorV1::Native(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(c)) => FundedOriginalRefusalV1::Resource(c),
        PreparedGeometryErrorV1::Native(crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(_)) => {
            FundedOriginalRefusalV1::Invalid("native original geometry")
        }
        PreparedGeometryErrorV1::MissingSharedServiceNamespace => FundedOriginalRefusalV1::Missing(FundedMissingV1::SharedServiceNamespace),
        PreparedGeometryErrorV1::MissingNativeRegistrationPolicy => FundedOriginalRefusalV1::Missing(FundedMissingV1::NativeRegistrationPolicy),
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
        PreparedRequirementsErrorV1::MissingSharedServiceNamespace => FundedOriginalRefusalV1::Missing(FundedMissingV1::SharedServiceNamespace),
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
) -> Result<(crate::palw_court_service_state_v2::CourtServiceStateV2, crate::palw_court_service_state_v2::CourtServiceStateV2), FundedOriginalRefusalV1> {
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

/// What the FP arm carries from the seam to the install: the ordinary TFP1 preparation, or the coordinated funded original.
pub(super) enum OriginalAdmissionV1 {
    Plain(PreparedOriginalFrameV1),
    Funded(Box<FundedOriginalPlanV1>),
}
impl OriginalAdmissionV1 {
    pub(super) fn frame_v1(&self) -> &Frame {
        match self {
            Self::Plain(p) => p.frame_v1(),
            Self::Funded(f) => f.requirements.frame_v1(),
        }
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
    let funded = b.extras.tir_funded_originals.contains_key(&claim);
    match (prepared, funded) {
        (None, false) => Ok(None),
        (Some(p), false) => Ok(Some(OriginalAdmissionV1::Plain(p))),
        (None, true) => Err(refused("RFC-0035 A funded original has no native paired preparation")),
        (Some(p), true) => coordinate_funded_original_v1(b, ctx, claim, p)
            .map(|plan| Some(OriginalAdmissionV1::Funded(Box::new(plan))))
            .map_err(FundedOriginalRefusalV1::into_state_v1),
    }
}

/// **The coordinator's checks — reads only; nothing is written.** Order: pairing/literal → schedule (typed-missing in production)
/// → native geometry and requirements (budgets from the schedule) → actor catalog (current authority, re-signed source pledge) and
/// each missing role contract → source graph (service namespace) → native expected scope → existing TFO1 inspection of the actual
/// carrier + payer signature (`prepare_funding_v1`) → derived hard deadline → fresh row and its uniqueness.
pub(super) fn coordinate_funded_original_v1(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim: Hash64,
    prepared: PreparedOriginalFrameV1,
) -> Result<FundedOriginalPlanV1, FundedOriginalRefusalV1> {
    use FundedOriginalRefusalV1::{Invalid, Missing as M};
    let pairing = b.extras.tir_funded_originals.get(&claim).ok_or(Invalid("no transaction-local funded pairing"))?;
    let literal = b.extras.tir_public_originals.get(&claim).ok_or(Invalid("funded pairing without its literal TFP1"))?;
    let inner = pairing.envelope().original_v1().map_err(|_| Invalid("TFO1 literal original"))?;
    if pairing.claim() != claim || inner != *literal || prepared.frame_v1().plan_v1().original_claim != claim {
        return Err(Invalid("funded pairing, literal TFP1 and prepared original differ"));
    }
    let schedule = palw_funded_original_schedule_v1(b.extras).map_err(M)?;
    let required_amount = schedule.required_amount_sompi().ok_or(Invalid("schedule amount overflow"))?;
    if required_amount == 0 {
        return Err(M(FundedMissingV1::Schedule));
    }
    let network = b.extras.tir_public_network_domain_v1.ok_or(M(FundedMissingV1::NetworkDomain))?;
    // Native geometry and requirements, all from the successful preparation; the budgets are part of the schedule.
    let geometry = prepared.enrich_native_geometry_v1(b, ctx).map_err(geometry_refusal)?;
    let requirements = geometry
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
        if !schedule.adopts_role(bit) {
            return Err(M(typed));
        }
    }
    let source_actor = actors.source_actor_v1();
    // The complete native source graph (all query families, scoped to the CURRENT service namespace). Its relative scope is the
    // graph identity the payer must have signed; the graph itself is re-derivable from the rooted frame (RFC-0035 §3 last bullet).
    let graph = requirements.source_graph_v1().map_err(requirements_refusal)?;
    graph.check_v3().map_err(|_| Invalid("native source graph"))?;
    let frame = requirements.frame_v1();
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
    // Every legal branch (liable signer × turn, every native source-graph node, retention), its resources and capital; then a
    // dry-run reservation of ALL of them in a clone of the rooted book (all-or-nothing). The install re-derives against the book
    // as it stands at the install point.
    let derivation = crate::palw_funded_original_admission_v1::derive_funded_admission_v1(
        claim,
        plan.allocation_id,
        plan.source_id,
        plan.terms.accepted_daa,
        &signers,
        graph,
        &admission,
    )
    .map_err(admission_refusal)?;
    funded_book_candidate_v1(b, &requirements, &derivation, &admission, &signers)?;
    // Capital (RFC-0035 §7.1): Σ branch capital (≥ the schedule floor), less the producer deposit the native work room already
    // holds, plus the fee once.
    let required_amount = crate::palw_funded_original_admission_v1::funded_required_amount_v1(
        schedule.fee_sompi(),
        schedule.capital_floor_sompi(),
        derivation.capital_sompi,
        plan.terms.plan.deposit_sompi,
    )
    .ok_or(Invalid("required capital overflow"))?;
    if required_amount == 0 {
        return Err(M(FundedMissingV1::Schedule));
    }
    let hard = crate::palw_funded_escrow_v1::palw_funded_escrow_hard_deadline_v1(b.params.claim_hard_deadline_v1(), ctx.daa_score)
        .map_err(|_| M(FundedMissingV1::HardDeadline))?;
    let expected = crate::palw_court_original_escrow_v1::OriginalFundingScopeV1 {
        claim,
        job_identity: pledge.job_identity,
        execution_root: frame.original_v1().execution_root,
        source_id: plan.source_id,
        relative_graph_id: graph.scope_v3(),
        fee_profile_id: schedule.fee_profile_id(),
        capital_profile_id: schedule.capital_profile_id(),
        retention_profile_id: schedule.retention_profile_id(),
        settlement_policy_id: schedule.settlement_policy_id(),
        required_amount_sompi: required_amount,
        required_material_until_daa: plan.terms.material_until_daa.max(hard),
    };
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
    let row = crate::palw_funded_escrow_v1::PalwFundedEscrowRowV1::fresh_v1(
        terms.id_v1(),
        claim,
        funding.actual().outpoint(),
        funding.actual().output().value,
        terms.payer_key_id,
        terms.refund_payload,
        ctx.daa_score,
        hard,
        // Lane r5: the liability opens at admission and closes only when `funded_liability_resolved_v1` holds
        // (`close_resolved_funded_liabilities_v1`, once per block). Payee paths stay refused (`SettlementScheduleMissing`).
        true,
    )
    .map_err(|_| Invalid("escrow row shape"))?;
    // Capacity: one row per contract and per outpoint (also re-checked by the writer).
    if b.state.funded_escrows_v1().contains_key(&row.contract_id)
        || b.state.funded_escrows_v1().values().any(|other| other.funding_outpoint == row.funding_outpoint)
    {
        return Err(Invalid("contract or funding outpoint already escrowed"));
    }
    drop(funding);
    Ok(FundedOriginalPlanV1 { requirements, row, derivation, admission, signers })
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
        OriginalAdmissionV1::Funded(plan) => {
            let FundedOriginalPlanV1 { requirements, row, derivation, admission, signers } = *plan;
            if row.accepted_daa != ctx.daa_score || b.state.claim(&row.claim).is_none() || !row.liability_open {
                return Err(refused("RFC-0035 A funded install lacks this block's native FP acceptance"));
            }
            // Lane r5: re-derive the all-branch book candidate against the book as it stands NOW (before the frame install).
            let before = b.state.court_service_state_v1().cloned();
            let (after_signer, full) =
                funded_book_candidate_v1(b, &requirements, &derivation, &admission, &signers).map_err(FundedOriginalRefusalV1::into_state_v1)?;
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
            b.install_funded_escrow_v1(row)?;
            // G14 lane r6-d1-release (r5 open 4): reserve the claim's court capacity under `palw_court_capacity_v1` (dormant below it).
            super::palw_court_capacity_fold_v1::check_funded_court_reserve_v1(&b.state, b.params, claim_id, ctx.daa_score)
        }
    }
}
