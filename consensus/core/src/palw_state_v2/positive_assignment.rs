//! RFC-0045 auxiliary state mutations inside the canonical PALW writer.
//!
//! Plans below are private native-fold data. They are not admission capabilities:
//! the native ingress must derive costs, current UTXOs, accepted-prefix origin,
//! original rule scope and actual physical/service reservations before calling.
//! No independent store or success flag is maintained here. Every row uses the
//! existing KernelRouteRow delta, and rollback restores nonce and locks together.
use super::*;
use crate::{palw_positive_assignment_v2::AssignmentJobBindingV2, palw_positive_rows_v2::PositiveRowsV2};
use misaka_palw_positive_contract::{
    assignment::{
        AssignmentId, AssignmentNonceV2, AssignmentRoleV2, journal::AssignmentJournalEventKindV2,
        role_actions::AssignmentCancellationV2, rows::*,
    },
    identity::{PUBKEY_BYTES, SIGNATURE_BYTES},
    native::{KERNEL_AUX_TABLE, NativeRowKey},
    rows::{
        AcceptedOrigin, CaseRow, ClaimIndexRow, FundingRow, NativeOutpoint, NativeRowBody, NativeRowValue, ResultRow, validate_bundle,
    },
};

fn refusal(detail: impl std::fmt::Debug) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(format!("Positive Assignment transition: {detail:?}"))
}
fn native_bond(outpoint: NativeOutpoint) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_bytes(outpoint.transaction.0), outpoint.index))
}

pub(super) struct AssignmentInstallV2 {
    pub assignment: AssignmentRow,
    pub funding: AssignmentFundingRow,
    pub roles: [AssignmentRoleRow; 3],
    pub frame: AssignmentJournalFrameRow,
    pub retention_until_daa: u64,
}

pub(super) struct AssignedCaseInstallV2 {
    pub expected_assignment: AssignmentRow,
    pub binding: AssignmentJobBindingV2,
    pub index: ClaimIndexRow,
    pub result: ResultRow,
    pub case: CaseRow,
    pub funding: FundingRow,
    pub use_row: AssignmentUseRow,
    pub frame: AssignmentJournalFrameRow,
}

impl TransitionBuilder<'_> {
    fn assignment_row_v2(&self, id: AssignmentId) -> Result<Option<AssignmentRow>, PalwStateV2Error> {
        match PositiveRowsV2::new(&self.state).read(NativeRowKey::Assignment(id)).map_err(refusal)? {
            None => Ok(None),
            Some(value) => match value.body {
                NativeRowBody::Assignment(row) => Ok(Some(row)),
                _ => Err(refusal("wrong Assignment row kind")),
            },
        }
    }
    fn require_absent_assignment_key_v2(&self, key: NativeRowKey) -> Result<(), PalwStateV2Error> {
        if PositiveRowsV2::new(&self.state).read(key).map_err(refusal)?.is_some() {
            return Err(refusal("an existing accepted row is never overwritten by intake"));
        }
        Ok(())
    }
    /// Current registry ownership and ordinary work eligibility, distinct from
    /// an old signed duty which survives retirement. This is intake only.
    fn current_assignment_role_v2(&self, role: &AssignmentRoleRow) -> Result<(), PalwStateV2Error> {
        let key = native_bond(role.bond);
        let bond = self.state.bond(&key).ok_or_else(|| refusal("missing current native role bond"))?;
        crate::palw_positive_metadata_v2::positive_registered_principal_v2(&self.state, &key).map_err(refusal)?;
        if !palw_bond_may_take_work_v2(bond, self.params.min_collateral_sompi) || bond.pubkey.as_slice() != role.key.as_slice() {
            return Err(refusal("role key or current work eligibility changed"));
        }
        Ok(())
    }
    fn assignment_origin_v2(&self, origin: AcceptedOrigin, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
        if origin.accepting_block.0 != ctx.block.as_bytes()
            || origin.accepted_daa != ctx.daa_score
            || origin.accepted_blue_score != ctx.blue_score
        {
            return Err(refusal("event does not belong to this native accepting point"));
        }
        Ok(())
    }
    /// Admits the canonical key before encoding, then the value/map/journal
    /// copies through the existing native typed writer. Never silently drops a
    /// write when the kernel header is absent.
    pub(super) fn write_assignment_aux_v2(&mut self, body: NativeRowBody) -> Result<(), PalwStateV2Error> {
        if self.state.kernel_route.is_none() {
            return Err(refusal("missing actual kernel route header"));
        }
        let key = body.key();
        if self.native_writers_tracking_v1() {
            let bytes = borsh::object_length(&(2u16, 1u16, &key)).map_err(refusal)? as u64;
            let mut quote = native_writers::NativeWriteQuoteV1::new("Positive Assignment aux151 key");
            quote.exact::<u8>(bytes)?;
            self.admit_native_write_v1(quote)?;
        }
        let key = key.encode();
        let value = NativeRowValue::new(body).map_err(refusal)?;
        self.write_kernel_row_encoded_v1(KERNEL_AUX_TABLE, &key, Some(&value))
    }
    pub(super) fn append_assignment_frame_v2(
        &mut self,
        frame: AssignmentJournalFrameRow,
        retention: u64,
    ) -> Result<(), PalwStateV2Error> {
        let key = NativeRowKey::AssignmentJournalHead(frame.header.assignment);
        let previous = match PositiveRowsV2::new(&self.state).read(key).map_err(refusal)? {
            None => None,
            Some(value) => match value.body {
                NativeRowBody::AssignmentJournalHead(head) => Some(head),
                _ => return Err(refusal("wrong Assignment journal head kind")),
            },
        };
        self.require_absent_assignment_key_v2(NativeRowKey::AssignmentJournalFrame {
            assignment: frame.header.assignment,
            ordinal: frame.header.ordinal,
        })?;
        let head = frame.append_to(previous.as_ref(), retention).map_err(refusal)?;
        self.write_assignment_aux_v2(NativeRowBody::AssignmentJournalFrame(frame))?;
        self.write_assignment_aux_v2(NativeRowBody::AssignmentJournalHead(head))
    }
    pub(super) fn assignment_retention_v2(&self, id: AssignmentId) -> Result<u64, PalwStateV2Error> {
        match PositiveRowsV2::new(&self.state).read(NativeRowKey::AssignmentJournalHead(id)).map_err(refusal)? {
            Some(value) => match value.body {
                NativeRowBody::AssignmentJournalHead(head) => Ok(head.retention_until_daa),
                _ => Err(refusal("wrong Assignment journal head kind")),
            },
            None => Err(refusal("missing accepted Assignment journal head")),
        }
    }
    /// All purpose accounts share one ordinary native UTXO namespace. An
    /// output cannot simultaneously back a bond, legacy escrow, A or C.
    pub(super) fn require_assignment_output_unlocked_v2(&self, point: NativeOutpoint) -> Result<(), PalwStateV2Error> {
        let native = native_bond(point).0;
        if self.state.bonds.contains_key(&PalwBondKeyV2(native))
            || self.state.funded_escrows.values().any(|row| row.settled_by.is_none() && row.funding_outpoint == native)
        {
            return Err(refusal("funding collides with another native lock"));
        }
        let route = self.state.kernel_route.as_ref().ok_or_else(|| refusal("missing kernel route"))?;
        for ((table, key), bytes) in &route.aux {
            if *table != KERNEL_AUX_TABLE || !matches!(key.get(4), Some(7 | 13 | 18)) {
                continue;
            }
            let value = NativeRowValue::decode(key, bytes).map_err(refusal)?;
            let collision = match value.body {
                NativeRowBody::AssignmentFunding(row) => row.original_outpoint == point || row.current_outpoint == Some(point),
                NativeRowBody::Funding(row) => row.original_outpoint == point || row.current_outpoint == Some(point),
                NativeRowBody::AssignmentProverFunding(row) => row.original_outpoint == point || row.current_outpoint == Some(point),
                _ => false,
            };
            if collision {
                return Err(refusal("funding output belongs to another Positive purpose account"));
            }
        }
        Ok(())
    }
    /// One admission checkpoint owns the permanent poster counter, all three
    /// role signatures, initial A lock, reservation/liability references and
    /// journal. A later error undoes every one of them.
    pub(super) fn install_positive_assignment_v2(
        &mut self,
        ctx: &PalwBlockContextV2,
        plan: AssignmentInstallV2,
    ) -> Result<(), PalwStateV2Error> {
        let row = &plan.assignment;
        row.validate_shape().map_err(refusal)?;
        let id = row.id();
        self.assignment_origin_v2(row.origin, ctx)?;
        if row.status != AssignmentStatusV2::Open
            || row.last_event != row.origin
            || plan.frame.header.assignment != id
            || plan.frame.header.origin != row.origin
            || plan.frame.header.kind != AssignmentJournalEventKindV2::Open
            || plan.funding.assignment != id
            || plan.funding.authorization != row.bundle.funding
            || plan.funding.funding_origin != row.origin
            || plan.funding.current_outpoint != Some(plan.funding.original_outpoint)
            || plan.funding.last_settlement.is_some()
        {
            return Err(refusal("initial Assignment/funding/journal mismatch"));
        }
        self.require_absent_assignment_key_v2(NativeRowKey::Assignment(id))?;
        self.require_absent_assignment_key_v2(NativeRowKey::AssignmentFunding(id))?;
        self.require_absent_assignment_key_v2(NativeRowKey::AssignmentUse(id))?;
        plan.funding.validate_shape().map_err(refusal)?;
        for ((role, outpoint), role_row) in [AssignmentRoleV2::Poster, AssignmentRoleV2::Worker, AssignmentRoleV2::Prover]
            .into_iter()
            .zip([row.bundle.terms.poster, row.bundle.terms.worker, row.bundle.terms.prover])
            .zip(&plan.roles)
        {
            if role_row.role != role
                || role_row.bond != outpoint
                || role_row.assignment != id
                || role_row.authorization != AssignmentRoleAuthorizationV2::Initial
            {
                return Err(refusal("initial role set differs from the three signed roles"));
            }
            role_row.authenticate(row).map_err(refusal)?;
            self.current_assignment_role_v2(role_row)?;
            self.require_absent_assignment_key_v2(NativeRowKey::AssignmentRole { assignment: id, role, bond: outpoint })?;
        }
        if plan.funding.payer_key != plan.roles[0].key || plan.funding.payer_signature != plan.roles[0].signature {
            return Err(refusal("A funding does not use the literal poster-purpose consent"));
        }
        self.require_assignment_output_unlocked_v2(plan.funding.original_outpoint)?;
        let nonce_key = NativeRowKey::AssignmentNonce(row.bundle.terms.poster);
        let previous = match PositiveRowsV2::new(&self.state).read(nonce_key).map_err(refusal)? {
            None => AssignmentNonceV2::default(),
            Some(value) => match value.body {
                NativeRowBody::AssignmentNonce(nonce) if nonce.poster == row.bundle.terms.poster => nonce.nonce,
                _ => return Err(refusal("wrong permanent poster nonce row")),
            },
        };
        let nonce = previous.after_accepting(row.bundle.terms.poster_nonce).map_err(refusal)?;
        let poster = row.bundle.terms.poster;
        let checkpoint = self.try_checkpoint()?;
        let result = (|| {
            self.write_assignment_aux_v2(NativeRowBody::AssignmentNonce(AssignmentNonceRow { poster, nonce }))?;
            self.write_assignment_aux_v2(NativeRowBody::Assignment(plan.assignment))?;
            self.write_assignment_aux_v2(NativeRowBody::AssignmentFunding(plan.funding))?;
            for role in plan.roles {
                self.write_assignment_aux_v2(NativeRowBody::AssignmentRole(role))?;
            }
            self.append_assignment_frame_v2(plan.frame, plan.retention_until_daa)
        })();
        if result.is_err() {
            self.restore(checkpoint);
        }
        result
    }

    /// A binds exactly one literal r1 Result and C. No old op0 row is attached
    /// retroactively and no candidate output is copied into expected metadata.
    pub(super) fn consume_positive_assignment_v2(
        &mut self,
        ctx: &PalwBlockContextV2,
        plan: AssignedCaseInstallV2,
    ) -> Result<(), PalwStateV2Error> {
        let expected = &plan.expected_assignment;
        let id = expected.id();
        if self.assignment_row_v2(id)?.as_ref() != Some(expected)
            || expected.status != AssignmentStatusV2::Open
            || ctx.daa_score >= expected.bundle.terms.result_before_daa
            || plan.use_row.assignment != id
            || plan.use_row.result != plan.result.result.id()
            || plan.use_row.case != plan.case.case
            || plan.use_row.origin != plan.result.origin
            || plan.frame.header.origin != plan.result.origin
            || plan.frame.header.assignment != id
            || plan.frame.header.kind != AssignmentJournalEventKindV2::BindResult
            || plan.result.policy != expected.positive_policy
            || plan.result.fired_fork != expected.fired_rules
            || plan.result.dependencies != expected.dependencies
            || plan.result.miner_key != expected.worker_key
            || plan.result.role_assignment
                != (misaka_palw_positive_contract::rows::AcceptedMaterial { origin: expected.origin, digest: id.0 })
            || plan.binding.terms() != &expected.bundle.terms
        {
            return Err(refusal("Assignment is stale, expired, consumed or differently bound"));
        }
        self.assignment_origin_v2(plan.result.origin, ctx)?;
        plan.binding.check_result(&plan.result.result).map_err(refusal)?;
        validate_bundle(&plan.index, &plan.result, &plan.case, &plan.funding).map_err(refusal)?;
        let future = expected.bundle.future_case;
        if plan.funding.consent.payer != future.payer
            || plan.funding.consent.refund_payload != future.refund_payload
            || plan.funding.basis.required_fee > future.max_case_fee
            || plan.funding.basis.required_capital > future.max_case_capital
            || plan.funding.initial_locked > future.max_case_locked
            || ctx.daa_score >= future.case_admission_before_daa
        {
            return Err(refusal("C exceeds original signed future-Case bounds"));
        }
        for key in [
            NativeRowKey::AssignmentUse(id),
            NativeRowKey::Result(plan.result.result.id()),
            NativeRowKey::ClaimIndex(plan.index.claim),
            NativeRowKey::Case(plan.case.case),
            NativeRowKey::Funding(plan.case.case),
        ] {
            self.require_absent_assignment_key_v2(key)?;
        }
        let a_funding = PositiveRowsV2::new(&self.state)
            .read(NativeRowKey::AssignmentFunding(id))
            .map_err(refusal)?
            .ok_or_else(|| refusal("missing original A funding"))?;
        if let NativeRowBody::AssignmentFunding(a) = a_funding.body {
            if a.current_outpoint == plan.funding.current_outpoint {
                return Err(refusal("A and C cannot double-lock one output"));
            }
        } else {
            return Err(refusal("wrong A funding row kind"));
        }
        self.require_assignment_output_unlocked_v2(plan.funding.original_outpoint)?;
        let retention = self.assignment_retention_v2(id)?;
        let checkpoint = self.try_checkpoint()?;
        let result = (|| {
            let mut row = plan.expected_assignment;
            row.status = AssignmentStatusV2::Consumed { case: plan.case.case, result: plan.result.result.id() };
            row.last_event = plan.result.origin;
            self.write_assignment_aux_v2(NativeRowBody::Assignment(row))?;
            self.write_assignment_aux_v2(NativeRowBody::AssignmentUse(plan.use_row))?;
            self.write_assignment_aux_v2(NativeRowBody::Result(plan.result))?;
            self.write_assignment_aux_v2(NativeRowBody::ClaimIndex(plan.index))?;
            self.write_assignment_aux_v2(NativeRowBody::Case(plan.case))?;
            self.write_assignment_aux_v2(NativeRowBody::Funding(plan.funding))?;
            self.append_assignment_frame_v2(plan.frame, retention)
        })();
        if result.is_err() {
            self.restore(checkpoint);
        }
        result
    }
}

pub(super) enum AssignmentCloseV2 {
    Cancel { consent: AssignmentCancellationV2, poster_signature: [u8; SIGNATURE_BYTES], worker_signature: [u8; SIGNATURE_BYTES] },
    Expire,
}
impl TransitionBuilder<'_> {
    /// Stops new tasks only. Existing service claims, capital, funds, role
    /// preimages and liabilities remain in place for their independent clocks.
    /// Cancellation/expiry never resets the permanent nonce or deletes a join.
    pub(super) fn close_positive_assignment_v2(
        &mut self,
        ctx: &PalwBlockContextV2,
        expected: &AssignmentRow,
        event: AssignmentCloseV2,
        frame: AssignmentJournalFrameRow,
        max_roles: u32,
    ) -> Result<(), PalwStateV2Error> {
        let id = expected.id();
        if self.assignment_row_v2(id)?.as_ref() != Some(expected)
            || expected.status != AssignmentStatusV2::Open
            || frame.header.assignment != id
            || max_roles < 3
            || max_roles == u32::MAX
        {
            return Err(refusal("only the current unconsumed Assignment can close"));
        }
        self.assignment_origin_v2(frame.header.origin, ctx)?;
        let mut roles = 0u32;
        let route = self.state.kernel_route.as_ref().ok_or_else(|| refusal("missing kernel route"))?;
        for ((table, key), bytes) in &route.aux {
            if *table != KERNEL_AUX_TABLE || key.get(4) != Some(&15) {
                continue;
            }
            let value = NativeRowValue::decode(key, bytes).map_err(refusal)?;
            let NativeRowBody::AssignmentRole(role) = value.body else {
                return Err(refusal("wrong role row kind"));
            };
            if role.assignment != id {
                continue;
            }
            roles = roles.checked_add(1).ok_or_else(|| refusal("role count overflow"))?;
            if roles > max_roles {
                return Err(refusal("all-role close scan exceeds original finite role policy"));
            }
            role.authenticate(expected).map_err(refusal)?;
            if let AssignmentRoleAuthorizationV2::Joined(join) = role.authorization
                && join.cancellation_policy != expected.bundle.cancellation_policy
            {
                return Err(refusal("joined prover did not consent to the original cancellation policy"));
            }
        }
        if roles < 3 {
            return Err(refusal("missing original role obligations"));
        }
        let status = match event {
            AssignmentCloseV2::Cancel { consent, poster_signature, worker_signature } => {
                if frame.header.kind != AssignmentJournalEventKindV2::Cancel {
                    return Err(refusal("wrong cancellation journal kind"));
                }
                let poster =
                    self.state.bond(&native_bond(expected.bundle.terms.poster)).ok_or_else(|| refusal("missing registered poster"))?;
                let poster_key: &[u8; PUBKEY_BYTES] = poster.pubkey.as_slice().try_into().map_err(refusal)?;
                let worker =
                    self.state.bond(&native_bond(expected.bundle.terms.worker)).ok_or_else(|| refusal("missing registered worker"))?;
                let worker_key: &[u8; PUBKEY_BYTES] = worker.pubkey.as_slice().try_into().map_err(refusal)?;
                if worker_key != &expected.worker_key {
                    return Err(refusal("worker changed since original admission"));
                }
                consent.authenticate_poster(&expected.bundle, poster_key, &poster_signature, ctx.daa_score).map_err(refusal)?;
                consent.authenticate_worker(&expected.bundle, worker_key, &worker_signature, ctx.daa_score).map_err(refusal)?;
                AssignmentStatusV2::Cancelled
            }
            AssignmentCloseV2::Expire => {
                if frame.header.kind != AssignmentJournalEventKindV2::Expire || ctx.daa_score < expected.bundle.terms.result_before_daa
                {
                    return Err(refusal("expiry needs the original result cutoff"));
                }
                AssignmentStatusV2::ExpiredUnconsumed
            }
        };
        let retention = self.assignment_retention_v2(id)?;
        let checkpoint = self.try_checkpoint()?;
        let result = (|| {
            let mut next = expected.clone();
            next.status = status;
            next.last_event = frame.header.origin;
            self.write_assignment_aux_v2(NativeRowBody::Assignment(next))?;
            self.append_assignment_frame_v2(frame, retention)
        })();
        if result.is_err() {
            self.restore(checkpoint);
        }
        result
    }
}

/// Replays raw inputs at the same accepted-object boundary in the final fold.
/// Semantic refusals affect only the Assignment; local capacity cannot be
/// swallowed as though it were a peer-invalid operation.
pub(super) fn apply_raw_before_object_v2(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    index: usize,
    cursor: &mut usize,
    previous_ordinal: &mut Option<u32>,
) -> Result<(), PalwStateV2Error> {
    while let Some(scheduled) = builder.extras.positive_assignment_actions.get(*cursor) {
        let slot = scheduled.before_object as usize;
        if slot > index {
            break;
        }
        if slot < index {
            return Err(refusal("raw action schedule moved backward"));
        }
        let origin = scheduled.action.origin();
        if previous_ordinal.is_some_and(|previous| origin.accepted_tx_ordinal <= previous) {
            return Err(refusal("raw native accepted ordinals are not strictly increasing"));
        }
        // Arc clone retains the actual immutable transport allocation. It is
        // not a decoded/saved admission capability and cannot bypass this hook.
        let action = scheduled.action.clone();
        match builder.apply_positive_assignment_raw_v2(ctx, &action) {
            Ok(()) => {}
            Err(local @ (PalwStateV2Error::NativeResource(_) | PalwStateV2Error::NativeContextUnavailable(_))) => return Err(local),
            Err(_semantic) => {}
        }
        *previous_ordinal = Some(origin.accepted_tx_ordinal);
        *cursor += 1;
    }
    Ok(())
}
impl TransitionBuilder<'_> {
    /// The raw executor is shared by rehearsal and final native fold. Every
    /// operation enters one canonical checkpoint and decodes afresh. No saved
    /// prepared rows, public boolean or generic authority trait is consumed.
    pub(super) fn apply_positive_assignment_raw_v2(
        &mut self,
        ctx: &PalwBlockContextV2,
        action: &PositiveAssignmentRawActionV2,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_positive_court_v2::coordinator::assignment_body::{
            PositiveAssignmentNativeOperationV2 as O, decode_assignment_body_v2,
        };
        // Capture before ANY event write, including resource/fee/P rows. This
        // exact delta position is persisted as DATA and re-executed by archive
        // loading; neither the digest nor a recorded origin grants authority.
        let boundary =
            (self.native_reservation.state_root_v2(&self.state).map_err(PalwStateV2Error::NativeResource)?, self.entries.len());
        let checkpoint = self.try_checkpoint()?;
        let previous_boundary = self.assignment_raw_boundary.replace(boundary);
        let previous_active = self.assignment_active_raw.replace(action.clone());
        let result = (|| {
            self.assignment_origin_v2(action.origin(), ctx)?;
            let tx = action.transaction();
            if tx.id().as_bytes() != action.origin().carrier_transaction.0
                || tx.inputs.len() != action.entries().len()
                || tx.subnetwork_id != crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE
                || tx.payload.get(..3) != Some(&[1, 0, 176])
            {
                return Err(refusal("raw action lost its actual native transaction/origin binding"));
            }
            let rules = action.rules();
            if matches!(tx.payload.get(7), Some(0..=7)) {
                let old_limits = crate::palw_positive_court_v2::coordinator::native_body::NativeBodyLimitsV2 {
                    max_body_bytes: crate::palw_positive_court_v2::coordinator::native_body::MAX_BODY_BYTES_V2,
                    max_opaque_bytes: crate::palw_positive_court_v2::coordinator::native_body::MAX_OPAQUE_BYTES_V2,
                    max_exact_relations: crate::palw_positive_court_v2::coordinator::native_body::MAX_EXACT_RELATIONS_V2,
                    max_signatures: crate::palw_positive_court_v2::coordinator::native_body::MAX_SIGNATURES_V2,
                };
                let old = crate::palw_positive_court_v2::coordinator::native_body::decode_native_body_v2(&tx.payload[3..], old_limits)
                    .map_err(refusal)?;
                return match &old.operation {
                    crate::palw_positive_court_v2::coordinator::native_body::PositiveNativeOperationV2::ResultAdmission { .. } => {
                        Err(refusal("assigned mode requires atomic op9, never old op0 then attach"))
                    }
                    crate::palw_positive_court_v2::coordinator::native_body::PositiveNativeOperationV2::PositiveCourtAttestation {
                        ..
                    } => self.apply_positive_assignment_attestation_raw_v2(ctx, action, &old),
                    _ => self.apply_positive_assignment_case_raw_v2(ctx, action, &old),
                };
            }
            let limits = rules.body_limits().map_err(refusal)?;
            let body = decode_assignment_body_v2(&tx.payload[3..], limits).map_err(refusal)?;
            match &body.operation {
                O::AcceptCanonicalAssignment { bundle, source, prover, signatures, layout } => {
                    let scope = rules.rule_scope();
                    if bundle.terms.network != rules.network() || bundle.terms.ruleset.0 != scope.ruleset().as_bytes() {
                        return Err(refusal("Assignment does not bind the actual original native scope"));
                    }
                    rules.assignment_policy().check_terms(&bundle.terms, ctx.daa_score).map_err(refusal)?;
                    let preimages =
                        rules.native_preimages().ok_or_else(|| refusal("actual native fee/physical preimages are missing"))?;
                    preimages.validate(rules.assignment_policy()).map_err(refusal)?;
                    source.check(bundle).map_err(refusal)?;
                    prover.check(bundle, rules.assignment_policy().positive_policy).map_err(refusal)?;
                    // Literal layout workspace was independently reserved by
                    // the raw scanner. The heavier registry/program/prompt/
                    // inventory/CoveragePlan now has its own real typed owner.
                    // This reads the CURRENT prefix; later registration/nonce
                    // changes cannot authorize the earlier carrier.
                    self.require_absent_assignment_key_v2(NativeRowKey::Assignment(bundle.terms.id()))?;
                    let class = Hash64::from_bytes(bundle.terms.registered_class.0);
                    if self.state.class(&class).is_none() || !self.state.tir_classes.contains_key(&class) {
                        return Err(refusal("actual registered TIR class missing at this accepted prefix"));
                    }
                    let nonce = match PositiveRowsV2::new(&self.state)
                        .read(NativeRowKey::AssignmentNonce(bundle.terms.poster))
                        .map_err(refusal)?
                    {
                        None => AssignmentNonceV2::default(),
                        Some(value) => match value.body {
                            NativeRowBody::AssignmentNonce(row) => row.nonce,
                            _ => return Err(refusal("wrong nonce row kind")),
                        },
                    };
                    nonce.after_accepting(bundle.terms.poster_nonce).map_err(refusal)?;
                    for point in [bundle.terms.poster, bundle.terms.worker, bundle.terms.prover] {
                        let bond = self.state.bond(&native_bond(point)).ok_or_else(|| refusal("missing current role bond"))?;
                        if !palw_bond_may_take_work_v2(bond, self.params.min_collateral_sompi) {
                            return Err(refusal("current role cannot accept work"));
                        }
                    }
                    let layout: crate::palw_tir_class_v1::PalwTirLayoutV1 = borsh::from_slice(layout).map_err(refusal)?;
                    let reserved = crate::palw_positive_assignment_resources_v2::ReservedAssignmentDerivedResourcesV2::derive(
                        &self.state,
                        bundle.terms,
                        &layout,
                        rules.assignment_policy(),
                        rules.positive_policy(),
                        &preimages.source_policy,
                        rules.prompt_form(),
                        preimages.physical.max_heap_bytes.min(rules.assignment_policy().max_heap_bytes),
                    )
                    .map_err(|error| match error {
                        crate::palw_positive_assignment_resources_v2::AssignmentResourceErrorV2::Native(why) => {
                            PalwStateV2Error::NativeResource(why)
                        }
                        crate::palw_positive_assignment_resources_v2::AssignmentResourceErrorV2::MissingNativeLease => {
                            PalwStateV2Error::NativeResource(crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<Self>(
                                "Assignment derived construction",
                                "actual native allocation lease is not armed",
                            ))
                        }
                        other => refusal(other),
                    })?;
                    let resources = reserved.resources();
                    if resources.source_quote().digest != bundle.source_catalog
                        || resources.binding().job().context_hash().as_bytes() != bundle.job_context.0
                    {
                        return Err(refusal("independently derived native job/source catalog differs from signed bundle"));
                    }
                    resources.binding().authenticate_bundle_roles(&self.state, bundle, signatures).map_err(refusal)?;
                    let clock = rules.gate_c_clock(bundle.terms.result_before_daa).map_err(refusal)?;
                    match clock.require_complete_native_bound() {
                        Err(why) => Err(refusal(why)),
                        Ok(impossible) => match impossible {},
                    }
                }
                O::AdmitAssignedResult { assignment, .. } => {
                    let row = self
                        .assignment_row_v2(*assignment)?
                        .ok_or_else(|| refusal("missing actual accepted Assignment at this prefix"))?;
                    if row.status != AssignmentStatusV2::Open || ctx.daa_score >= row.bundle.terms.result_before_daa {
                        return Err(refusal("Assignment unavailable for single Result consumption"));
                    }
                    Err(refusal("native original material/Case reservation owner is not yet resolved"))
                }
                O::CancelAssignment { cancellation, .. } => {
                    let row =
                        self.assignment_row_v2(cancellation.assignment)?.ok_or_else(|| refusal("missing accepted Assignment"))?;
                    cancellation.check(&row.bundle, ctx.daa_score).map_err(refusal)?;
                    if row.status != AssignmentStatusV2::Open {
                        return Err(refusal("consumed or terminal Assignment cannot cancel"));
                    }
                    Err(refusal("native joined-rights/liability/material close owner is not yet resolved"))
                }
                O::ExpireAssignment { assignment } => {
                    let row = self.assignment_row_v2(*assignment)?.ok_or_else(|| refusal("missing accepted Assignment"))?;
                    if row.status != AssignmentStatusV2::Open || ctx.daa_score < row.bundle.terms.result_before_daa {
                        return Err(refusal("Assignment cannot expire at this clock"));
                    }
                    Err(refusal("native joined-rights/liability/material expiry owner is not yet resolved"))
                }
                O::JoinAssignmentProver { consent, signature } => {
                    let row = self.assignment_row_v2(consent.assignment)?.ok_or_else(|| refusal("missing accepted Assignment"))?;
                    let bond = self.state.bond(&native_bond(consent.prover)).ok_or_else(|| refusal("missing current prover bond"))?;
                    if !palw_bond_may_take_work_v2(bond, self.params.min_collateral_sompi) {
                        return Err(refusal("current prover cannot accept work"));
                    }
                    consent.check(&row.bundle, ctx.daa_score).map_err(refusal)?;
                    // The actual priced construction/join reservation owner
                    // must precede role-purpose crypto and fund mutation.
                    let _untrusted_signature = signature;
                    self.require_absent_assignment_key_v2(NativeRowKey::AssignmentProverFunding {
                        assignment: consent.assignment,
                        prover: consent.prover,
                    })?;
                    Err(refusal("native cumulative join capacity/funds/material owner is not yet resolved"))
                }
                O::SettleAssignment { .. } => self.settle_positive_assignment_raw_funds_v2(ctx, action, &body),
            }
        })();
        self.assignment_active_raw = previous_active;
        self.assignment_raw_boundary = previous_boundary;
        if result.is_err() {
            self.restore(checkpoint);
        }
        result
    }
}
