//! Initial consent25/Fund26 authorize Final27 only after all numerical,
//! resource and measured public-profile conditions. Old18/23 never acquire it.
use super::*;
use crate::palw_flat_execution_pursuit_v19::NativePublicPursuitV19;
use crate::palw_native_final_v27::*;
use misaka_palw_kernel::flat_resources10::ResourceAxisV10 as A;
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(super) struct NativeFinalConsentV27 {
    pub version: u16,
    pub preparation: Digest,
    pub original_consent23: Digest,
    pub policy: Digest,
    pub basis: NativeFinalClassBasisV27,
    pub original_pursuit: Digest,
    pub queue_policy: Digest,
    pub queue_plan: Digest,
}
impl NativeFinalConsentV27 {
    fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-final-consent/v27", self)
    }
}
fn policy(builder: &TransitionBuilder<'_>, now: u64) -> Result<NativeFinalPolicyV27, PalwStateV2Error> {
    let e = builder.extras.kernel_route.as_ref().ok_or_else(|| refused("native Final27 network absent"))?;
    let f = e
        .native_final
        .as_ref()
        .filter(|f| f.activation.is_active(now))
        .ok_or_else(|| refused("native Final27 own trusted fence absent"))?;
    let p = f.policy.clone();
    p.check(e.network_domain.as_bytes()).map_err(refused)?;
    Ok(p)
}
fn basis(
    builder: &TransitionBuilder<'_>,
    class: &CheckedNativeExecutionClassV11,
    job: &CheckedJobQuotaPlanV11,
    p: &NativeExecutionPoliciesV11,
) -> Result<(NativeFinalClassBasisV27, CheckedNativeFinalConditionsV27, NativePublicPursuitV19), PalwStateV2Error> {
    let (pursuit, physical, service, _) = capacity::quote(builder, class, job, p)?;
    let c = class.record();
    let view = class.view();
    let candidate = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .flat_candidate_record_v9(&Hash64::from_bytes(c.candidate9_id))
        .ok_or_else(|| refused("native Final27 original candidate absent"))?;
    let resources = misaka_palw_kernel::flat_resources10::derive_flat_resources_v10(
        &view.program,
        &candidate.param_commitments,
        c.max_positions,
        view.encoder.as_ref(),
    )
    .map_err(refused)?;
    if resources.vector.root() != c.resource10_root {
        return Err(refused("native Final27 original full resource root differs"));
    }
    let source =
        misaka_palw_kernel::flat_quota11::source16::derive_source_plan_v16(class, job, &p.quota, &p.discovery).map_err(refused)?;
    let trace = derive_trace_plan_v17(class, job, &p.quota).map_err(refused)?;
    let q = &resources.vector.quantities;
    let work = [
        A::Macs,
        A::ScalarSteps,
        A::IndexSteps,
        A::MetadataWork,
        A::CourtWork,
        A::ParameterHashBytes,
        A::TraceHashBytes,
        A::CopiedReferenceBytes,
    ]
    .into_iter()
    .try_fold(0u128, |n, a| n.checked_add(q.get(a)))
    .ok_or_else(|| refused("native Final27 full reference tariff overflow"))?;
    let work = u64::try_from(work).map_err(|_| refused("native Final27 full reference tariff exceeds u64"))?;
    let ledger = super::super::palw_kernel_route_fold_v1::route_ledger_policy_v1(builder)?;
    let acquisition = ledger
        .dismissal_fee_v1(work)
        .checked_add(
            (pursuit.requests as u64)
                .checked_mul(ledger.dismissal_fee_v1(work))
                .ok_or_else(|| refused("native Final27 full sequential acquisition tariff overflow"))?,
        )
        .ok_or_else(|| refused("native Final27 complete outsider cost overflow"))?;
    let b = NativeFinalClassBasisV27 {
        class: c.id(),
        program: c.program_root,
        inventory: c.inventory_root,
        real_parameter: c.parameter_root,
        original_max_scope: c.class_scope9_root,
        original_max_resource: c.resource10_root,
        execution_policy: p.execution.root(),
        quota_policy: p.quota.root(),
        discovery_policy: p.discovery.root(),
        source_protocol: object_id(b"misaka-palw/native/explicit-source-trace-protocol/v27", &(source.root(), trace.root())),
        verifier_profile: misaka_palw_kernel::flat_scope9::flat_court_set_id_v9(),
        physical_service: service.root(),
        complete_reference_cost_sompi: acquisition,
        maximum_producer_gain_sompi: p
            .execution
            .reward_cap
            .checked_add(p.execution.external_gain_cap)
            .ok_or_else(|| refused("native Final27 maximum producer gain overflow"))?,
        original_producer_reserve_sompi: c.required_producer_reserve,
        original_finder_award_sompi: ((c.required_producer_reserve as u128 * c.finder_permille as u128) / 1000) as u64,
        original_default_obligation_sompi: p.execution.default_obligation,
        maximum_reference_bytes: u64::try_from(q.get(A::OwnedPeakBytes))
            .map_err(|_| refused("native Final27 original reference RAM overflow"))?,
        maximum_court_bytes: c.source_court_bound.max_proof_bytes.max(view.scope.worst_court_bytes),
        maximum_court_work: c
            .source_court_bound
            .max_work
            .max(u64::try_from(q.get(A::CourtWork)).map_err(|_| refused("native Final27 court work overflow"))?),
        maximum_court_scratch: c
            .source_court_bound
            .max_scratch_bytes
            .max(u64::try_from(q.get(A::CourtScratchBytes)).map_err(|_| refused("native Final27 court RAM overflow"))?),
    };
    let policy = policy(builder, builder.state.kernel_route.as_ref().unwrap().header.scalars.daa)?;
    let checked = policy
        .check_class(p.execution.network_domain, &b, pursuit.complete_path, !physical.requires_source_parts, service.implemented)
        .map_err(refused)?;
    Ok((b, checked, pursuit))
}
pub(super) fn accept(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    bytes: &[u8],
) -> Result<(), PalwStateV2Error> {
    policy(builder, ctx.daa_score)?;
    let id = fixed_id(bytes)?;
    let prep: NativeExecutionPreparationV11 = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .aux_row(EXECUTION_PREPARATION_TABLE_V11, &key(&id))
        .ok_or_else(|| refused("native Final27 original preparation absent"))?;
    if prep.record.producer != palw_kernel_bond_id_v1(signer) {
        return Err(refused("native Final27 only original producer can consent"));
    }
    if !prepay(builder, ctx, signer, class_work(builder, prep.record.class_id, bytes.len())?)? {
        return Ok(());
    }
    let class = checked_class(builder, ctx, p, prep.record.class_id, true)?;
    let job = prep.checked_plan(class.record()).map_err(refused)?;
    let (basis, conditions, pursuit) = basis(builder, &class, &job, p)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    if r.aux.contains_key(&(EXECUTION_PREPARATION_TABLE_V11, key(&(27u8, id)))) {
        return Err(refused("native Final27 original consent exists"));
    }
    if !r.aux.contains_key(&(EXECUTION_PREPARATION_TABLE_V11, key(&(23u8, id)))) {
        capacity::accept(builder, ctx, signer, p, bytes)?;
    }
    let (root, c): (Digest, crate::palw_flat_execution_capacity_v22::NativeExecutionConsentV23) = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .aux_row(EXECUTION_PREPARATION_TABLE_V11, &key(&(23u8, id)))
        .ok_or_else(|| refused("native Final27 original23 consent absent"))?;
    if c.root() != root {
        return Err(refused("native Final27 original23 root differs"));
    }
    let (_, _, service, mass) = capacity::quote(builder, &class, &job, p)?;
    let queue_policy = policy(builder, ctx.daa_score)?.queue_policy;
    let queue =
        crate::palw_native_queue_v30::CheckedNativeQueuePlanV30::derive_fixed(&queue_policy, &class, &job, &pursuit, &service, &mass)
            .map_err(refused)?;
    let c = NativeFinalConsentV27 {
        version: 27,
        preparation: id,
        original_consent23: root,
        policy: conditions.policy_root(),
        basis,
        original_pursuit: pursuit.root(),
        queue_policy: queue_policy.root(),
        queue_plan: queue.record().root(),
    };
    builder.write_kernel_row(EXECUTION_PREPARATION_TABLE_V11, key(&(27u8, id)), Some(key(&(c.root(), c))));
    Ok(())
}
pub(super) fn plan_fund(
    builder: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    p: &NativeExecutionPoliciesV11,
    request: &FlatExecutionFundV11,
    class: &CheckedNativeExecutionClassV11,
    job: &CheckedJobQuotaPlanV11,
) -> Result<NativeFinalConsentV27, PalwStateV2Error> {
    policy(builder, ctx.daa_score)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let raw = r
        .aux
        .get(&(EXECUTION_PREPARATION_TABLE_V11, key(&(27u8, request.preparation))))
        .ok_or_else(|| refused("Fund26 requires original producer consent25 before all resources/coins"))?;
    if raw.len() > 4096 {
        return Err(refused("native Final27 fixed initial consent cap"));
    }
    let (root, c): (Digest, NativeFinalConsentV27) =
        borsh::from_slice(raw).map_err(|_| refused("native Final27 initial consent grammar"))?;
    let (b, conditions, pursuit) = basis(builder, class, job, p)?;
    let (_, _, service, mass) = capacity::quote(builder, class, job, p)?;
    let qp = policy(builder, ctx.daa_score)?.queue_policy;
    let queue = crate::palw_native_queue_v30::CheckedNativeQueuePlanV30::derive_fixed(&qp, class, job, &pursuit, &service, &mass)
        .map_err(refused)?;
    let (root23, c23): (Digest, crate::palw_flat_execution_capacity_v22::NativeExecutionConsentV23) = r
        .aux_row(EXECUTION_PREPARATION_TABLE_V11, &key(&(23u8, request.preparation)))
        .ok_or_else(|| refused("native Final27 original paid23 consent absent"))?;
    if c.version != 27
        || c.root() != root
        || c.preparation != request.preparation
        || c.original_consent23 != root23
        || c23.root() != root23
        || c.policy != conditions.policy_root()
        || c.basis != b
        || c.original_pursuit != pursuit.root()
        || c.queue_policy != qp.root()
        || c.queue_plan != queue.record().root()
    {
        return Err(refused("Fund26 changes initial Final27 original basis"));
    }
    Ok(c)
}
pub(super) fn write_fund(builder: &mut TransitionBuilder<'_>, job: Digest, c: NativeFinalConsentV27) {
    builder.write_kernel_row(EXECUTION_PREPARATION_TABLE_V11, key(&(27u8, c.preparation)), None);
    builder.write_kernel_row(EXECUTION_POLICY_TABLE_V11, key(&(27u8, job)), Some(key(&(c.root(), c))));
}
pub(super) fn close(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    mut row: NativeExecutionJobRowV11,
    class: CheckedNativeExecutionClassHeaderV11,
    mut book: ExecutionJobStateV11,
    plan: CheckedJobQuotaPlanV11,
) -> Result<(), PalwStateV2Error> {
    let id = book.job().id();
    let r = builder.state.kernel_route.as_ref().unwrap();
    let raw = r
        .aux
        .get(&(EXECUTION_POLICY_TABLE_V11, key(&(27u8, id))))
        .ok_or_else(|| refused("old18/23 job has no INITIAL Final27 right"))?;
    if raw.len() > 4096 {
        return Err(refused("native Final27 original journal cap"));
    }
    let (root, c): (Digest, NativeFinalConsentV27) =
        borsh::from_slice(raw).map_err(|_| refused("native Final27 original journal grammar"))?;
    let policy = policy(builder, ctx.daa_score)?;
    let service = builder
        .extras
        .kernel_route
        .as_ref()
        .unwrap()
        .native_carriage_service
        .as_ref()
        .ok_or_else(|| refused("native Final27 actual delivery service absent"))?;
    let pursuit: NativePublicPursuitV19 =
        r.aux_row(EXECUTION_POLICY_TABLE_V11, &key(&(19u8, id))).ok_or_else(|| refused("native Final27 original pursuit absent"))?;
    if c.root() != root
        || c.version != 27
        || c.basis.class != row.header.class_id
        || c.basis.program != class.record().program_root
        || c.basis.real_parameter != class.record().parameter_root
        || c.basis.inventory != class.record().inventory_root
        || c.basis.physical_service != service.root()
        || c.original_pursuit != pursuit.root()
        || c.policy != policy.root()
    {
        return Err(refused("native Final27 original class/policy/pursuit/service rebound"));
    }
    let (qr, q): (Digest, crate::palw_native_queue_v30::NativeQueueLeaseV30) = r
        .aux_row(EXECUTION_QUOTA_TABLE_V11, &key(&(30u8, id, 1u8)))
        .ok_or_else(|| refused("native Final27 original queue30 lease absent"))?;
    if q.root() != qr
        || q.retired
        || q.job != id
        || q.original_funding != book.job().root()
        || q.original_policy != c.queue_policy
        || q.original_plan != c.queue_plan
        || c.queue_policy != policy.queue_policy.root()
        || q.not_before < book.job().deadline_daa
    {
        return Err(refused("native Final27 original burst residency contract differs"));
    }
    policy.check_class(p.execution.network_domain, &c.basis, pursuit.complete_path, true, service.implemented).map_err(refused)?;
    let original_source = source(r, class.record().binding_id)?;
    if original_source.refuted {
        return Err(refused("native Final27 original bound source was convicted"));
    }
    let state = pursuit_state(r, id)?;
    let mut pending = 0u32;
    for seal in &state.retained_seal_keys {
        let s = court::seal_row(r, seal)?;
        if s.job_id != id {
            return Err(refused("native Final27 paid seal belongs to another job"));
        }
        if !s.spent && ctx.daa_score <= s.horizon_daa {
            pending += 1;
        }
    }
    for ticket in &state.retained_discovery_keys {
        let t = discovery::native_row(r, ticket)?;
        if t.job_id != id || t.converted_to.is_some() {
            return Err(refused("native Final27 original discovery lineage differs"));
        }
        if ctx.daa_score <= t.horizon_daa {
            pending += 1;
        }
    }
    let close = NativeCloseSnapshotV11 {
        current_daa: ctx.daa_score,
        pending_own_proofs: pending,
        open_da: state.open_demands,
        latest_answer_proof_until: state.latest_answer_proof_until,
        required_retained_until: state.required_retained_until,
    };
    let settlement = book.finalize(class.record(), &close).map_err(refused)?;
    close_adjudicated_quota(builder, ctx, p, id, &class, &book, &plan, *signer)?;
    credit(builder, ctx, &mut row, &book, &settlement)?;
    Ok(())
}
fn pursuit_state(r: &PalwKernelRouteStateV1, id: Digest) -> Result<NativeExecutionPursuitV11, PalwStateV2Error> {
    super::pursuit(r, &id)
}

impl PalwChainStateV2 {
    /// Extracts internally authentic original Final rows. This does not certify
    /// header provenance, independent replay, source retrieval or redemption.
    pub fn native_execution_audit_v27(
        &self,
        id: Digest,
    ) -> Result<crate::palw_native_audit_v27::CheckedNativeExecutionAuditViewV27, String> {
        let r = self.kernel_route().ok_or("native audit route absent")?;
        Self::native_execution_audit_from_authenticated_rows_v27(
            &r.aux,
            r.header.policy.network_domain,
            self.last_point().map(|p| p.daa_score).unwrap_or(r.header.scalars.daa),
            self.state_root().as_bytes(),
            r.ledger_root().as_bytes(),
            r.aux_root().as_bytes(),
            &self.pending_payouts,
            id,
        )
    }
    /// Only Core's private proof32 verifier/context may call this stateless
    /// constructor. Public callers provide verified proof tokens, never row
    /// DTOs, roots or claimed payout booleans.
    pub(crate) fn native_execution_audit_from_authenticated_rows_v27(
        rows: &std::collections::BTreeMap<(u8, Vec<u8>), Vec<u8>>,
        network_domain: Digest,
        daa: u64,
        state_root: Digest,
        ledger_root: Digest,
        aux_root: Digest,
        payouts: &std::collections::BTreeMap<Hash64, PalwPayoutV2>,
        id: Digest,
    ) -> Result<crate::palw_native_audit_v27::CheckedNativeExecutionAuditViewV27, String> {
        use crate::palw_native_audit_v27::*;
        // Text29 Finals live under their own namespaced keys in the same tables;
        // they are extracted by the Text29 fold's own independent row reader.
        if !rows.contains_key(&(EXECUTION_JOB_TABLE_V11, key(&id))) && rows.contains_key(&(EXECUTION_JOB_TABLE_V11, key(&(29u8, 0u8, id))))
        {
            let record = super::text29::audit_record_from_rows_v29(
                rows,
                network_domain,
                daa,
                (state_root, ledger_root, aux_root),
                payouts,
                id,
            )?;
            return CheckedNativeExecutionAuditViewV27::from_native_rows(record);
        }
        let raw = rows.get(&(EXECUTION_JOB_TABLE_V11, key(&id))).ok_or("native audit original job absent")?;
        let row = decode_native_execution_job_v11(raw)?;
        if row.header.job_id != id {
            return Err("native audit original job namespace differs".into());
        }
        let raw = rows.get(&(EXECUTION_CLASS_TABLE_V11, key(&row.header.class_id))).ok_or("native audit original class absent")?;
        let (class, _) = native_execution_class_header_v11(raw)?;
        if class.record.id() != row.header.class_id {
            return Err("native audit original class namespace differs".into());
        }
        let checked = class.checked_fixed_header()?;
        let book = row.restore_book(&checked)?;
        let j = book.job();
        let claim = book.claim().ok_or("native audit Final lacks claim")?;
        let s = book.settlement().ok_or("native audit Final lacks settlement")?;
        s.check()?;
        let credit: NativeExecutionCreditV11 = rows
            .get(&(EXECUTION_CREDIT_TABLE_V11, key(&id)))
            .and_then(|raw| (raw.len() <= 1024).then(|| borsh::from_slice(raw).ok()).flatten())
            .ok_or("native audit single credit absent")?;
        let raw = rows.get(&(EXECUTION_POLICY_TABLE_V11, key(&(27u8, id)))).ok_or("native audit original Final27 consent absent")?;
        if raw.len() > 4096 {
            return Err("native audit original consent fixed cap".into());
        }
        let (consent_root, c): (Digest, NativeFinalConsentV27) = borsh::from_slice(raw).map_err(|_| "native audit consent grammar")?;
        let raw = rows
            .get(&(EXECUTION_REQUEST_INDEX_TABLE_V11, key(&row.header.requester_operator)))
            .ok_or("native audit permanent operator nonce row absent")?;
        if raw.len() != 72 {
            return Err("native audit fixed nonce row cap".into());
        }
        let nonce: NativeRequesterNonceRowV11 = borsh::from_slice(raw).map_err(|_| "native audit nonce grammar")?;
        if nonce.operator != row.header.requester_operator {
            return Err("native audit nonce operator differs".into());
        }
        let consumed_nonce = canonical_nonce_v11(&j.nonce)?;
        if book.terminal() != ExecutionTerminalV11::Final
            || book.escrow_remaining() != 0
            || claim.terminal != ExecutionTerminalV11::Final
            || !claim.credit_paid
            || s.terminal != ExecutionTerminalV11::Final
            || s.claim_id != Some(claim.id())
            || s.escrow_debit != j.escrow_original_debit
            || s.producer_credit != j.reward
            || s.requester_refund != 0
            || s.collected_original_slash != 0
            || s.finder_credit != 0
            || s.burned != 0
            || credit.job_id != id
            || credit.original_funded_job_root != j.root()
            || credit.terminal_book_root != book.root()
            || credit.settlement_root != s.root()
            || credit.producer_credit != s.producer_credit
            || credit.requester_refund != 0
            || credit.collected_original_slash != 0
            || credit.finder_credit != 0
            || credit.burned != 0
            || credit.established_daa < claim.challenge_end
            || c.version != 27
            || c.root() != consent_root
            || c.basis.class != class.record.id()
            || c.basis.program != class.record.program_root
            || c.basis.inventory != class.record.inventory_root
            || c.basis.real_parameter != class.record.parameter_root
            || c.policy == [0; 64]
            || nonce.next_nonce <= consumed_nonce
        {
            return Err("native audit Final/nonce/original debit/single credit agreement differs".into());
        }
        let mut audit_rows = Vec::new();
        for (table, k) in [
            (EXECUTION_CLASS_TABLE_V11, key(&row.header.class_id)),
            (EXECUTION_JOB_TABLE_V11, key(&id)),
            (EXECUTION_CREDIT_TABLE_V11, key(&id)),
            (EXECUTION_POLICY_TABLE_V11, key(&(27u8, id))),
            (EXECUTION_REQUEST_INDEX_TABLE_V11, key(&row.header.requester_operator)),
            (EXECUTION_POLICY_TABLE_V11, key(&(19u8, id))),
        ] {
            let bytes = rows.get(&(table, k.clone())).ok_or("native audit original row missing")?;
            audit_rows.push(NativeAuditRowV27 { table, key: k, bytes: bytes.clone() });
        }
        let payout_queue =
            crate::palw_native_audit_v27::native_final_credit_queue_v27(id, j.root(), s.root(), row.header.producer_payout);
        CheckedNativeExecutionAuditViewV27::from_native_rows(NativeExecutionAuditRecordV27 {
            version: 27,
            namespace: 11,
            network_domain,
            daa,
            state_root,
            ledger_root,
            aux_root,
            class: row.header.class_id,
            source: class.record.binding_id,
            program: class.record.program_root,
            real_parameter: class.record.parameter_root,
            inventory: class.record.inventory_root,
            job: id,
            funding: j.root(),
            original_escrow_debit: j.escrow_original_debit,
            original_escrow_receipt: j.escrow_debit_receipt,
            original_consent: consent_root,
            original_final_policy: c.policy,
            original_final_basis: c.basis.root(),
            maximum_scope: class.record.class_scope9_root,
            maximum_resource: class.record.resource10_root,
            actual_scope: j.actual_scope9_root,
            actual_resource: j.actual_resource10_root,
            claim: claim.id(),
            vector: claim.vector,
            trace: claim.trace_root,
            output: claim.output_root,
            terminal_book: book.root(),
            settlement: s.root(),
            credit: object_id(b"misaka-palw/native/audit-single-credit/v27", &credit),
            producer_credit: credit.producer_credit,
            requester_operator: row.header.requester_operator,
            producer_operator: row.header.producer_operator,
            consumed_nonce: j.nonce,
            next_nonce: nonce.next_nonce,
            established_daa: credit.established_daa,
            payout_payload: row.header.producer_payout.as_bytes(),
            payout_queue: payout_queue.as_bytes(),
            observed_queue_amount: payouts.get(&payout_queue).filter(|p| p.payload == row.header.producer_payout).map(|p| p.amount),
            rows: audit_rows,
        })
    }
}
