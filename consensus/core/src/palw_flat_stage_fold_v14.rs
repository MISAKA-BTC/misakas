//! Native stage14 admission under its own dormant fence. Typed source weights
//! and external inputs never share an artifact namespace or collateral grant.
use super::*;
use crate::palw_flat_stage_v14::*;
use crate::palw_kernel_route_v1::palw_kernel_bond_id_v1;
use misaka_palw_kernel::{flat_inputs12::source14::*, flat_resources10::ResourceAxisV10, hash::Digest};
use misaka_palw_kernel::{
    flat_inputs12::{FlatInputBudgetV12, RawExternalInputV12, RawExternalInputsV12, bounded14::FlatStageJobV14},
    flat_quota11::{NativeRequesterNonceWitnessV11, QuotaActionKindV11, TrustedEpochV11, stage14::*},
};
#[path = "palw_flat_stage_court_fold_v14.rs"]
mod court;

pub(super) fn apply_chunk(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    chunk: &crate::palw_kernel_route_v1::PalwKernelChunkV1,
) -> Result<(), PalwStateV2Error> {
    match court::apply_chunk(builder, ctx, chunk) {
        Err(PalwStateV2Error::KernelRouteRefused(_)) => Ok(()),
        other => other,
    }
}
#[path = "palw_flat_stage_source_fold_v14.rs"]
mod source;
#[path="palw_flat_stage_fold_v28.rs"]
mod next28;
/// Stage28 component Final audit extraction (see `next28::audit_record_from_rows_v28`).
pub(super) fn audit_record_from_rows_v28(
    rows: &std::collections::BTreeMap<(u8, Vec<u8>), Vec<u8>>,
    network_domain: misaka_palw_kernel::hash::Digest,
    daa: u64,
    roots: (misaka_palw_kernel::hash::Digest, misaka_palw_kernel::hash::Digest, misaka_palw_kernel::hash::Digest),
    payouts: &std::collections::BTreeMap<crate::Hash64, PalwPayoutV2>,
    id: misaka_palw_kernel::hash::Digest,
) -> Result<crate::palw_native_audit_v27::NativeExecutionAuditRecordV27, String> {
    next28::audit_record_from_rows_v28(rows, network_domain, daa, roots, payouts, id)
}

fn refused(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(why.into())
}
fn bytes<T: borsh::BorshSerialize>(value: &T) -> Vec<u8> {
    borsh::to_vec(value).expect("bounded stage native row")
}
fn policies(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<NativeStagePoliciesV14, PalwStateV2Error> {
    super::palw_flat_model_vector_fold_v3::enabled(builder, ctx)?;
    let f = builder
        .extras
        .kernel_route
        .as_ref()
        .and_then(|r| r.stage_execution.as_ref())
        .ok_or_else(|| refused("palw_stage_execution_v14 is not in force"))?;
    if !f.activation.is_active(ctx.daa_score) {
        return Err(refused("typed stage14 is below its independent fence"));
    }
    let p = f.policies.clone();
    p.check(builder.state.kernel_route.as_ref().unwrap().header.policy.network_domain).map_err(refused)?;
    let key = stage_key_v14(4, &());
    let raw = bytes(&p);
    let stored = builder.state.kernel_route.as_ref().unwrap().aux.get(&(STAGE_TABLE_V14, key.clone()));
    if stored.is_some_and(|old| old != &raw) {
        return Err(refused("typed stage14 immutable native policy changed"));
    }
    if stored.is_none() {
        builder.write_kernel_row(STAGE_TABLE_V14, key, Some(raw));
    }
    // Text11 and stage14 use one global quota/actor/session pool. A different
    // configured tuple cannot fork those limits under another feature label.
    let shared = bytes(&p.execution);
    let table = crate::palw_flat_execution_v11::EXECUTION_POLICY_TABLE_V11;
    let stored = builder.state.kernel_route.as_ref().unwrap().aux.get(&(table, Vec::new()));
    if stored.is_some_and(|old| old != &shared) {
        return Err(refused("stage14/text11 shared native policies differ"));
    }
    if stored.is_none() {
        builder.write_kernel_row(table, Vec::new(), Some(shared));
    }
    Ok(p)
}
fn prepay(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    work: u64,
) -> Result<bool, PalwStateV2Error> {
    let Some(fee) = super::palw_flat_model_vector_fold_v3::charge(builder, ctx, signer, work, false)? else {
        return Ok(false);
    };
    builder.slash_bond(*signer, fee)?;
    Ok(true)
}
fn tariff(n: usize) -> Result<u64, PalwStateV2Error> {
    let n = u64::try_from(n).map_err(|_| refused("stage byte tariff overflow"))?;
    n.checked_mul(n)
        .and_then(|w| w.checked_mul(64))
        .and_then(|w| w.checked_add(n.checked_mul(1024)?))
        .and_then(|w| w.checked_add(65536))
        .ok_or_else(|| refused("stage metadata tariff overflow"))
}
fn prepay_restore(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV14,
    source_bytes: u32,
    class_bytes: u32,
    carrier_bytes: u32,
    fee_work: u64,
) -> Result<bool, PalwStateV2Error> {
    let quote = derive_stage_restore_metadata_v14(source_bytes, class_bytes, 0, 0, carrier_bytes).map_err(refused)?;
    if !prepay(builder, ctx, signer, fee_work)? {
        return Ok(false);
    }
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let mut shard =
        super::palw_flat_execution_fold_v11::quota_shard(builder, &p.execution, &epoch, palw_kernel_bond_id_v1(signer), [0; 64])?;
    shard.debit_ordinary(&p.execution.quota, &epoch, &quote).map_err(refused)?;
    super::palw_flat_execution_fold_v11::write_shard(builder, &shard, [0; 64]);
    Ok(true)
}
impl PalwChainStateV2 {
    pub fn native_stage_reserved_v14(&self, bond: &PalwBondKeyV2, daa: u64) -> u128 {
        self.kernel_route.as_ref().map(|r| r.stage_source_reserved_v14(bond, daa) + r.stage_producer_reserved_v14(bond)+next28::reserved(r,bond,daa)).unwrap_or(0)
    }
}
pub(super) fn reveal_paged_v28(builder:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,signer:&PalwBondKeyV2,seal:&crate::palw_native_paged_proof_v29::NativePagedSealV29,file:&[u8])->Result<(),PalwStateV2Error>{next28::reveal_paged_v28(builder,ctx,signer,seal,file)}

fn checked_class(
    builder: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    p: &NativeStagePoliciesV14,
    id: Digest,
    admit: bool,
) -> Result<CheckedStageClassV14, PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let h = r.stage_class_header_v14(&id).ok_or_else(|| refused("original stage class absent"))?;
    let s = r.stage_source_header_v14(&h.source_binding).ok_or_else(|| refused("original stage class source absent"))?;
    if admit && (s.refuted || r.stage_source_refuted_relation_v14(&s.relation_root) || s.matures_daa > ctx.daa_score) {
        return Err(refused("stage source currently unavailable for admission"));
    }
    let wire = r.stage_class_wire_v14(&id).ok_or_else(|| refused("stage class bounded wire corrupt"))?;
    let source = r
        .stage_source_row_v14(&h.source_binding)
        .ok_or_else(|| refused("stage source bounded row corrupt"))?
        .checked_source()
        .map_err(refused)?;
    let class = wire.check(&source).map_err(refused)?;
    if class.root() != id
        || h.policy_root != p.root()
        || class.policy_root() != p.root()
        || class.source().id() != h.source_binding
        || class.template().root().map_err(refused)? != h.template_root
        || class.maximum_resources().court_scope != h.max_scope_root
        || class.maximum_resources().root() != h.max_resource_root
    {
        return Err(refused("stage class original native roots independently differ"));
    }
    Ok(class)
}
fn restore_tariff(builder: &TransitionBuilder<'_>, id: Digest, body: usize) -> Result<u64, PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let c = r.stage_class_header_v14(&id).ok_or_else(|| refused("stage class absent"))?;
    let s = r.stage_source_header_v14(&c.source_binding).ok_or_else(|| refused("stage class source absent"))?;
    tariff(c.wire_bytes_len as usize)?
        .checked_add(tariff(s.source_bytes_len as usize)?)
        .and_then(|n| n.checked_add(u64::try_from(body).ok()?.checked_mul(1024)?))
        .ok_or_else(|| refused("stage restore tariff overflow"))
}
fn plan_stage_budget(
    builder: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    p: &NativeStagePoliciesV14,
    class: &CheckedStageClassV14,
    book: &StageJobBookV14,
    plan: &CheckedStageQuotaV14,
    anchor: &NativeStageQuotaAnchorV14,
    producer: PalwBondKeyV2,
) -> Result<Option<super::palw_bond_budget_fold_v1::PalwBudgetPlanV1>, PalwStateV2Error> {
    let granted = check_stage_quota_admission_v14(class, book, plan, &p.execution.quota, anchor).map_err(refused)?;
    if book.execution_policy_root() != p.execution.execution.root()
        || class.policy_root() != p.root()
        || granted.binding().producer != palw_kernel_bond_id_v1(&producer)
        || anchor.block != ctx.block.as_bytes()
        || anchor.daa != ctx.daa_score
        || book.original_debit() == 0
        || book.original_debit() != book.escrow_remaining()
    {
        return Err(refused("stage fixed budget lacks exact original18 checked native epoch/producer/coins"));
    }
    let Some(mirror) = builder.bond_budget_mirror_at(ctx.daa_score) else {
        // RFC-0015 §8.3: Panel-free stage work is unbudgeted only where no OPV policy requires a budget.
        return builder.unbudgeted_panel_free_v1();
    };
    if mirror.allocation_at(ctx.daa_score).is_some() {
        return Err(refused("stage budget has no independently authenticated native model-line allocation"));
    }
    builder.plan_bond_budget_v1(
        ctx,
        producer,
        None,
        crate::palw_bond_budget_v1::PalwBudgetAskV1 {
            block_units: 0,
            reward_sompi: 0,
            final_weight: 0,
            // ADR-0176 amendment (ユーザー判断 2026-10-15): past `palw_bond_formula_v2` at least the formula's NativeFunded total
            // (collectable-loss floor + court/DA/retention), the class's own reserve otherwise.
            liability_sompi: builder.bond_formula_producer_liability_v2(
                ctx,
                crate::palw_bond_formula_v2::PalwClaimTypeV2::NativeFunded,
                0,
                p.execution.execution.required_producer_reserve as u128,
            ),
        },
        crate::palw_bond_budget_v1::PalwBudgetOriginV1::KernelRoute,
        0,
    )
}
fn fund(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV14,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    let id: Digest = body
        .get(2..66)
        .ok_or_else(|| refused("truncated stage Fund class"))?
        .try_into()
        .map_err(|_| refused("stage Fund class digest"))?;
    if body.len() > STAGE_NATIVE_INPUT_BYTES_MAX_V14 + 2112 {
        return Err(refused("stage Fund carrier cap"));
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let ch = r.stage_class_header_v14(&id).ok_or_else(|| refused("stage Fund original class absent"))?;
    let sh = r.stage_source_header_v14(&ch.source_binding).ok_or_else(|| refused("stage Fund original source absent"))?;
    if !prepay_restore(
        builder,
        ctx,
        signer,
        p,
        sh.source_bytes_len,
        ch.wire_bytes_len,
        body.len() as u32,
        restore_tariff(builder, id, body.len())?,
    )? {
        return Ok(());
    }
    let class = checked_class(builder, ctx, p, id, true)?;
    let request = StageFundRequestV14::from_bytes(body).map_err(refused)?;
    let job = FlatStageJobV14::from_bytes(&request.job_bytes).map_err(refused)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let requester = palw_kernel_bond_id_v1(signer);
    let producer = palw_kernel_bond_id_v1(&request.producer);
    if job.requester != requester || job.producer != producer {
        return Err(refused("stage Fund signer and producer differ from committed job"));
    }
    let requester_bond = builder.state.bonds.get(signer).ok_or_else(|| refused("stage requester absent"))?;
    let requester_operator = requester_bond.operator_id.as_bytes();
    let requester_payout = requester_bond.payout_payload;
    let producer_bond = builder.state.bonds.get(&request.producer).ok_or_else(|| refused("stage producer absent"))?;
    if !matches!(producer_bond.status, PalwBondStatusV2::Active) {
        return Err(refused("stage producer inactive"));
    }
    let producer_operator = producer_bond.operator_id.as_bytes();
    let producer_payout = producer_bond.payout_payload;
    let contract = r
        .stage_acceptance_v18(&request.acceptance)
        .ok_or_else(|| refused("stage Fund requires explicit original source16/trace17 consent18"))?;
    let accepted = &contract.preparation;
    if accepted.requester != *signer
        || accepted.producer != request.producer
        || accepted.class != id
        || accepted.source != class.source().id()
        || accepted.policy != p.root()
        || accepted.job != job.root()
        || accepted.requester_operator != requester_operator
        || accepted.producer_operator != producer_operator
        || accepted.nonce != job.nonce
        || accepted.reward != request.reward
        || ctx.daa_score > accepted.expires_daa
        || ctx.daa_score < accepted.accepted_daa
    {
        return Err(refused("stage Fund differs from exact finite original producer consent"));
    }
    let old_nonce = r.execution_nonce_counter_v11(&requester_operator).map_err(refused)?;
    let next_nonce = old_nonce.advance(&job.nonce).map_err(refused)?;
    let mut owners = r.stage_owner_ids_v14(6, signer).ok_or_else(|| refused("stage funded owner catalog corrupt"))?;
    let mut producer_jobs = r.stage_owner_ids_v14(7, &request.producer).ok_or_else(|| refused("stage producer catalog corrupt"))?;
    if owners.len() >= p.catalog_cap as usize || producer_jobs.len() >= p.catalog_cap as usize {
        return Err(refused("stage funded owner/producer catalog full"));
    }
    let actual = class.check_job(&job).map_err(refused)?;
    let plan = derive_stage_quota_v14(&class, &actual).map_err(refused)?;
    let source_plan = derive_stage_source_plan_v14(&class, &plan, &p.execution.quota, &p.execution.discovery).map_err(refused)?;
    let trace_plan = derive_stage_trace_plan_v14(&class, &plan, &p.execution.quota).map_err(refused)?;
    if contract.source_plan != source_plan.root() || contract.trace_plan != trace_plan.root() {
        return Err(refused("stage Fund source/trace delivery differs from explicit producer consent18"));
    }
    let preparation = StageAcceptRequestV14 {
        version: 14,
        class: id,
        requester: *signer,
        reward: request.reward,
        expires_daa: accepted.expires_daa,
        source: accepted.source,
        policy: accepted.policy,
        admission: accepted.admission,
        quota: accepted.quota,
        job_bytes: request.job_bytes.clone(),
    };
    let ledger = super::palw_kernel_route_fold_v1::route_ledger_policy_v1(builder)?;
    let fees = stage_delivery_fee_plan_v20(&ledger, p, &preparation, source_plan.requests()).map_err(refused)?;
    let availability = stage_availability_terms_v18(p, &preparation, source_plan.root(), trace_plan.root(), fees.root());
    if contract.fee_plan != fees.root() || contract.availability_terms != availability {
        return Err(refused("stage Fund fees/availability differ from explicitly signed original consent18"));
    }
    if accepted.admission != actual.admission().root() || accepted.quota != plan.root() {
        return Err(refused("stage Fund actual resources/source differ from producer's signed preparation"));
    }
    if request.input_images.len() != actual.program().inputs.len() {
        return Err(refused("stage Fund input count differs"));
    }
    let prospective_job =
        misaka_palw_kernel::hash::object_id(b"misaka-palw/kernel/funded-stage-job/v14", &(14u16, id, job.root(), request.reward));
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let mut input_shard = super::palw_flat_execution_fold_v11::quota_shard(builder, &p.execution, &epoch, requester, prospective_job)?;
    let input_work = derive_stage_input_metadata_v14(&actual).map_err(refused)?;
    input_shard.debit_ordinary(&p.execution.quota, &epoch, &input_work).map_err(refused)?;
    super::palw_flat_execution_fold_v11::write_shard(builder, &input_shard, prospective_job);
    struct Images<'a> {
        program: &'a misaka_palw_tir::program_v2::TirProgramV2,
        bytes: &'a [Vec<u8>],
    }
    impl RawExternalInputsV12 for Images<'_> {
        fn input(&self, k: u16) -> Option<RawExternalInputV12<'_>> {
            let d = self.program.inputs.get(k as usize)?;
            Some(RawExternalInputV12 { dtype: d.dtype, shape: &d.shape, bytes: self.bytes.get(k as usize)? })
        }
    }
    // Exact images, input intervals and public roots are authenticated before
    // a producer's baseline and the requester's original escrow are committed.
    actual
        .acquire(
            class.template(),
            class.source().parameters(),
            &Images { program: actual.program(), bytes: &request.input_images },
            &mut FlatInputBudgetV12 {
                raw_bytes: actual
                    .program()
                    .inputs
                    .iter()
                    .try_fold(0u128, |n, d| {
                        d.shape
                            .iter()
                            .try_fold(d.dtype.width() as u128, |n, x| n.checked_mul(*x as u128))
                            .and_then(|b| n.checked_add(b))
                    })
                    .ok_or_else(|| refused("stage input byte bound overflow"))?,
                reference_bytes: 32 << 20,
                attempts: 1,
            },
        )
        .map_err(refused)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let source = r.stage_source_header_v14(&class.source().id()).ok_or_else(|| refused("stage original source disappeared"))?;
    let book = StageJobBookV14::fund(
        &class,
        &actual,
        &plan,
        &p.execution.execution,
        &p.execution.quota,
        &NativeStageFundingV14 {
            class: id,
            source: source.id,
            policy: p.root(),
            acceptance: request.acceptance,
            consent_version: 18,
            source_plan: source_plan.root(),
            trace_plan: trace_plan.root(),
            fee_plan: fees.root(),
            availability_terms: availability,
            source_matures_daa: source.matures_daa,
            source_refuted: source.refuted,
            requester,
            requester_operator,
            producer,
            producer_operator,
            block: ctx.block.as_bytes(),
            daa: ctx.daa_score,
            reward: request.reward,
            original_debit: request.reward,
            next_nonce: old_nonce.next_nonce,
            nonce_counter_root: old_nonce.root(),
        },
    )
    .map_err(refused)?;
    let job_id = book.id();
    if r.stage_job_header_v14(&job_id).is_some() {
        return Err(refused("stage original funded job already exists"));
    }
    let anchor = NativeStageQuotaAnchorV14 {
        class: id,
        source: source.id,
        policy: p.root(),
        acceptance: request.acceptance,
        source_plan: source_plan.root(),
        trace_plan: trace_plan.root(),
        fee_plan: fees.root(),
        availability_terms: availability,
        funded_book_root: book.root(),
        job_id,
        original_debit_receipt: book.debit_receipt(),
        original_debit: request.reward,
        nonce: NativeRequesterNonceWitnessV11 {
            requester,
            requester_operator_id: requester_operator,
            counter_root: next_nonce.root(),
            next_nonce: next_nonce.next_nonce,
        },
        block: ctx.block.as_bytes(),
        daa: ctx.daa_score,
        source_refuted: source.refuted,
        source_matures_daa: source.matures_daa,
    };
    let grant = check_stage_quota_admission_v14(&class, &book, &plan, &p.execution.quota, &anchor).map_err(refused)?;
    let budget_plan = plan_stage_budget(builder, ctx, p, &class, &book, &plan, &anchor, request.producer)?;
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let mut shard = super::palw_flat_execution_fold_v11::quota_shard(builder, &p.execution, &epoch, producer, job_id)?;
    shard.admit_baseline(&p.execution.quota, &epoch, &grant, plan.plan()).map_err(refused)?;
    let source_grant = check_stage_source_admission_v14(
        &class,
        &book,
        &plan,
        &p.execution.quota,
        &p.execution.discovery,
        &epoch,
        &source_plan,
        &anchor,
    )
    .map_err(refused)?;
    let mut source_shard = super::palw_flat_execution_fold_v11::source_shard_from_rows(
        &p.execution,
        &source_plan,
        &shard.global_row(),
        &shard.actor_row(),
        shard.subject_row().ok_or_else(|| refused("stage baseline absent"))?,
        None,
    )?;
    source_shard.admit(&p.execution.quota, &epoch, &source_plan, &source_grant).map_err(refused)?;
    let trace_grant =
        check_stage_trace_admission_v14(&class, &book, &plan, &p.execution.quota, &epoch, &trace_plan, &anchor).map_err(refused)?;
    let mut trace_shard = misaka_palw_kernel::flat_quota11::trace17::TraceShardV17::from_native_rows(
        &p.execution.quota,
        &trace_plan,
        source_shard.global_row(),
        &source_shard.global_row().root(),
        producer,
        Some((source_shard.producer_row(), &source_shard.producer_row().root())),
        (shard.subject_row().unwrap(), &shard.subject_row().unwrap().row_root()),
        None,
    )
    .map_err(refused)?;
    trace_shard.admit(&p.execution.quota, &epoch, &trace_plan, &trace_grant).map_err(refused)?;
    let free =
        (builder.state.bonds.get(signer).unwrap().collateral as u128).saturating_sub(builder.committed_at(signer, ctx.daa_score));
    if free < request.reward as u128 {
        return Err(refused("stage original requester escrow unavailable"));
    }
    let book_bytes = bytes(&book);
    let input_raw_bytes = request
        .input_images
        .iter()
        .try_fold(0u64, |n, b| n.checked_add(b.len() as u64))
        .ok_or_else(|| refused("stage input retained bytes overflow"))?;
    let header = NativeStageJobHeaderV14 {
        version: 14,
        id: job_id,
        class: id,
        acceptance: request.acceptance,
        source_binding: class.source().id(),
        policy_root: p.root(),
        admission_root: actual.admission().root(),
        stage_quota_root: plan.root(),
        quota_record: plan.plan().record(),
        original_book_root: book.root(),
        book_root: book.root(),
        book_bytes_len: book_bytes.len() as u32,
        requester: *signer,
        producer: request.producer,
        requester_operator,
        producer_operator,
        requester_payout,
        producer_payout,
        positions: job.positions,
        input_raw_bytes,
        input_count: request.input_images.len() as u16,
        deadline_daa: book.deadline_daa(),
        producer_reserved: 0,
        terminal: book.terminal(),
    };
    let row = NativeStageJobRowV14 { header, book_bytes, input_images: request.input_images, trace_bytes: vec![] };
    let mut acceptances = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .stage_owner_ids_v14(8, &request.producer)
        .ok_or_else(|| refused("stage producer acceptance catalog corrupt"))?;
    if !acceptances.contains(&request.acceptance) {
        return Err(refused("stage original acceptance owner catalog differs"));
    }
    acceptances.retain(|v| *v != request.acceptance);
    builder.hold_native_recipient_v1(
        Hash64::from_bytes(job_id),
        request.producer,
        14,
        ctx.daa_score,
        book.deadline_daa(),
        request.reward,
        &[],
    )?;
    if let Some(budget_plan) = budget_plan {
        let budget_id =
            Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"misaka-palw/native/stage-original-fixed-budget/v20", &job_id));
        builder.commit_bond_budget_v1(ctx, budget_id, budget_plan)?;
    }
    builder.hold_native_recipient_v1(
        Hash64::from_bytes(job_id),
        request.producer,
        19,
        ctx.daa_score,
        book.deadline_daa(),
        fees.maximum,
        &[],
    )?;
    let debit = builder.slash_bond(*signer, request.reward as u128)?;
    if debit != request.reward {
        return Err(refused("stage actual original coin debit differs"));
    }
    super::palw_flat_execution_fold_v11::write_shard(builder, &shard, job_id);
    super::palw_flat_execution_fold_v11::write_source_shard(builder, &source_shard, job_id);
    builder.write_kernel_row(
        crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11,
        bytes(&(16u8, job_id, 1u8)),
        Some(bytes(&source_plan.record())),
    );
    super::palw_flat_execution_fold_v11::write_trace(builder, &trace_shard, 0, job_id);
    owners.push(job_id);
    producer_jobs.push(job_id);
    builder.write_kernel_row(
        crate::palw_flat_execution_v11::EXECUTION_REQUEST_INDEX_TABLE_V11,
        bytes(&requester_operator),
        Some(bytes(&next_nonce)),
    );
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(5, &job_id), Some(bytes(&row)));
    builder.write_kernel_row(
        STAGE_TABLE_V14,
        stage_key_v14(16, &job_id),
        Some(bytes(&NativeStagePursuitV14 {
            version: 14,
            job: job_id,
            class: id,
            original_book: book.original_funded_root(),
            consent18: request.acceptance,
            open_demands: 0,
            retained_seals: 0,
            retained_discovery: 0,
            seal_keys: [[0; 64]; 4],
            discovery_keys: [[0; 64]; 4],
            source_requests: [[0; 64]; 32],
            source_units: [[0; 64]; 32],
            requests_admitted: 0,
            required_until: book.deadline_daa(),
        })),
    );
    builder.write_kernel_row(
        STAGE_TABLE_V14,
        stage_key_v14(12, &job_id),
        Some(bytes(&NativeStageFeesV20 {
            version: 20,
            job: job_id,
            original_book: book.original_funded_root(),
            consent18: request.acceptance,
            plan: fees,
            earned: 0,
        })),
    );
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(6, signer), Some(bytes(&owners)));
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(7, &request.producer), Some(bytes(&producer_jobs)));
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(10, &request.acceptance), None);
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(8, &request.producer), Some(bytes(&acceptances)));
    Ok(())
}

fn accept(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV14,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    let id: Digest = body
        .get(2..66)
        .ok_or_else(|| refused("truncated stage Accept class"))?
        .try_into()
        .map_err(|_| refused("stage Accept class differs"))?;
    if body.len() > 2048 {
        return Err(refused("stage Accept carrier cap"));
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let ch = r.stage_class_header_v14(&id).ok_or_else(|| refused("stage acceptance original class absent"))?;
    let sh = r.stage_source_header_v14(&ch.source_binding).ok_or_else(|| refused("stage acceptance original source absent"))?;
    if !prepay_restore(
        builder,
        ctx,
        signer,
        p,
        sh.source_bytes_len,
        ch.wire_bytes_len,
        body.len() as u32,
        restore_tariff(builder, id, body.len())?,
    )? {
        return Ok(());
    }
    let class = checked_class(builder, ctx, p, id, true)?;
    let request = StageAcceptRequestV14::from_bytes(body).map_err(refused)?;
    let job = FlatStageJobV14::from_bytes(&request.job_bytes).map_err(refused)?;
    if job.producer != palw_kernel_bond_id_v1(signer)
        || job.requester != palw_kernel_bond_id_v1(&request.requester)
        || request.class != id
        || request.source != class.source().id()
        || request.policy != p.root()
        || request.reward == 0
        || request.reward > p.execution.execution.reward_cap
        || request.expires_daa <= ctx.daa_score
        || request.expires_daa
            > ctx
                .daa_score
                .checked_add(p.execution.execution.horizons.commit_window)
                .ok_or_else(|| refused("stage acceptance expiry overflow"))?
        || request.expires_daa / p.execution.quota.interval_daa != ctx.daa_score / p.execution.quota.interval_daa
    {
        return Err(refused("stage acceptance lacks own producer/exact original principals/source/policy/finite expiry"));
    }
    let requester = builder.state.bonds.get(&request.requester).ok_or_else(|| refused("stage acceptance requester absent"))?;
    let requester_operator = requester.operator_id.as_bytes();
    let producer = builder.state.bonds.get(signer).ok_or_else(|| refused("stage acceptance producer absent"))?;
    if !matches!(producer.status, PalwBondStatusV2::Active) {
        return Err(refused("stage acceptance producer inactive"));
    }
    let producer_operator = producer.operator_id.as_bytes();
    let r = builder.state.kernel_route.as_ref().unwrap();
    r.execution_nonce_counter_v11(&requester_operator).map_err(refused)?.advance(&job.nonce).map_err(refused)?;
    let actual = class.check_job(&job).map_err(refused)?;
    let plan = derive_stage_quota_v14(&class, &actual).map_err(refused)?;
    if request.admission != actual.admission().root() || request.quota != plan.root() {
        return Err(refused("stage acceptance resource preparation independently differs"));
    }
    let mut owners = r.stage_owner_ids_v14(8, signer).ok_or_else(|| refused("stage acceptance owner catalog corrupt"))?;
    let expired = owners
        .iter()
        .filter_map(|id| {
            r.stage_acceptance_v14(id)
                .map(|v| (*id, v.producer, v.expires_daa))
                .or_else(|| r.stage_acceptance_v18(id).map(|v| (*id, v.preparation.producer, v.preparation.expires_daa)))
        })
        .filter(|(_, producer, expires)| *producer == *signer && *expires < ctx.daa_score)
        .map(|(id, _, _)| id)
        .collect::<Vec<_>>();
    owners.retain(|id| !expired.contains(id));
    if owners.len() >= p.catalog_cap as usize {
        return Err(refused("stage own producer consent catalog full"));
    }
    let mut consent = NativeStageAcceptanceV14 {
        version: 14,
        id: [0; 64],
        class: id,
        source: class.source().id(),
        policy: p.root(),
        job: job.root(),
        admission: actual.admission().root(),
        quota: plan.root(),
        requester: request.requester,
        producer: *signer,
        requester_operator,
        producer_operator,
        nonce: job.nonce,
        reward: request.reward,
        accepted_daa: ctx.daa_score,
        expires_daa: request.expires_daa,
    };
    consent.id = consent.content_id();
    if r.stage_acceptance_v14(&consent.id).is_some() {
        return Err(refused("stage producer consent already exists"));
    }
    for id in expired {
        builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(9, &id), None);
        builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(10, &id), None);
    }
    owners.push(consent.id);
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(9, &consent.id), Some(bytes(&consent)));
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(8, signer), Some(bytes(&owners)));
    builder.note_kernel_bond(signer);
    Ok(())
}

fn accept_delivery_v18(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV14,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    if body.len() > 2310 {
        return Err(refused("stage consent18 bounded carrier"));
    }
    let class_id: Digest = body
        .get(8..72)
        .ok_or_else(|| refused("stage consent18 class prefix"))?
        .try_into()
        .map_err(|_| refused("stage consent18 class id"))?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let ch = r.stage_class_header_v14(&class_id).ok_or_else(|| refused("stage consent18 original class absent"))?;
    let sh = r.stage_source_header_v14(&ch.source_binding).ok_or_else(|| refused("stage consent18 original source absent"))?;
    let restore =
        derive_stage_restore_metadata_v14(sh.source_bytes_len, ch.wire_bytes_len, 0, 0, body.len() as u32).map_err(refused)?;
    if !prepay(builder, ctx, signer, restore_tariff(builder, class_id, body.len())?)? {
        return Ok(());
    }
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let mut shard =
        super::palw_flat_execution_fold_v11::quota_shard(builder, &p.execution, &epoch, palw_kernel_bond_id_v1(signer), [0; 64])?;
    shard.debit_ordinary(&p.execution.quota, &epoch, &restore).map_err(refused)?;
    super::palw_flat_execution_fold_v11::write_shard(builder, &shard, [0; 64]);
    let request = StageAcceptRequestV18::from_bytes(body).map_err(refused)?;
    let preparation = StageAcceptRequestV14::from_bytes(&request.preparation_bytes).map_err(refused)?;
    let job = FlatStageJobV14::from_bytes(&preparation.job_bytes).map_err(refused)?;
    let class = checked_class(builder, ctx, p, class_id, true)?;
    let actual = class.check_job(&job).map_err(refused)?;
    let plan = derive_stage_quota_v14(&class, &actual).map_err(refused)?;
    let source = derive_stage_source_plan_v14(&class, &plan, &p.execution.quota, &p.execution.discovery).map_err(refused)?;
    let trace = derive_stage_trace_plan_v14(&class, &plan, &p.execution.quota).map_err(refused)?;
    if request.source_plan != source.root() || request.trace_plan != trace.root() {
        return Err(refused("stage consent18 source/trace quotes differ from independent original derivation"));
    }
    let ledger = super::palw_kernel_route_fold_v1::route_ledger_policy_v1(builder)?;
    let fees = stage_delivery_fee_plan_v20(&ledger, p, &preparation, source.requests()).map_err(refused)?;
    let availability = stage_availability_terms_v18(p, &preparation, source.root(), trace.root(), fees.root());
    if request.fee_plan != fees.root() || request.availability_terms != availability {
        return Err(refused("stage consent18 governing fees/availability bounds differ from original policy"));
    }
    // Reuse the unchanged action13 preparation checks, but the signature of
    // THIS new action also authenticates the explicit additional liabilities.
    accept(builder, ctx, signer, p, &request.preparation_bytes)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let mut owners = r.stage_owner_ids_v14(8, signer).ok_or_else(|| refused("stage consent18 owner catalog corrupt"))?;
    let consent = owners
        .iter()
        .filter_map(|id| r.stage_acceptance_v14(id))
        .find(|v| {
            v.accepted_daa == ctx.daa_score
                && v.producer == *signer
                && v.requester == preparation.requester
                && v.class == class_id
                && v.job == job.root()
                && v.admission == preparation.admission
                && v.quota == preparation.quota
                && v.source == preparation.source
                && v.policy == preparation.policy
                && v.nonce == job.nonce
                && v.reward == preparation.reward
                && v.expires_daa == preparation.expires_daa
        })
        .ok_or_else(|| refused("stage consent18 lacks original own accepted preparation"))?;
    let old_id = consent.id;
    let mut contract = NativeStageAcceptanceV18 {
        version: 18,
        id: [0; 64],
        preparation: consent,
        source_plan: source.root(),
        trace_plan: trace.root(),
        fee_plan: fees.root(),
        availability_terms: availability,
    };
    contract.id = contract.content_id();
    if r.stage_acceptance_v18(&contract.id).is_some() {
        return Err(refused("stage consent18 already present"));
    }
    for id in &mut owners {
        if *id == old_id {
            *id = contract.id;
        }
    }
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(9, &old_id), None);
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(10, &contract.id), Some(bytes(&contract)));
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(8, signer), Some(bytes(&owners)));
    Ok(())
}

fn commit(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV14,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    let id: Digest = body
        .get(66..130)
        .ok_or_else(|| refused("truncated stage Commit job id"))?
        .try_into()
        .map_err(|_| refused("stage Commit job id differs"))?;
    let h =
        builder.state.kernel_route.as_ref().unwrap().stage_job_header_v14(&id).ok_or_else(|| refused("stage funded job absent"))?;
    if *signer != h.producer || h.terminal != StageTerminalV14::Funded || body.len() > (2 << 20) {
        return Err(refused("stage Commit lacks original own producer/job/carrier"));
    }
    let work = restore_tariff(builder, h.class, body.len())?
        .checked_add(h.input_raw_bytes.checked_mul(1024).ok_or_else(|| refused("stage input restore tariff overflow"))?)
        .ok_or_else(|| refused("stage Commit tariff overflow"))?;
    if !prepay(builder, ctx, signer, work)? {
        return Ok(());
    }
    let cached = CheckedStageQuotaV14::restore_native_record(
        h.class,
        h.source_binding,
        h.admission_root,
        h.policy_root,
        &h.quota_record,
        h.stage_quota_root,
    )
    .map_err(refused)?;
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let mut shard =
        super::palw_flat_execution_fold_v11::quota_shard(builder, &p.execution, &epoch, palw_kernel_bond_id_v1(signer), id)?;
    let binding = shard.subject_row().ok_or_else(|| refused("stage Commit original prepaid subject absent"))?.binding.clone();
    if binding.class_id != h.class || binding.job_id != id || binding.producer != palw_kernel_bond_id_v1(signer) {
        return Err(refused("stage Commit fixed quote selects another original subject"));
    }
    let action = cached.plan().action(QuotaActionKindV11::Metadata, body).map_err(refused)?;
    shard.debit_reserved(&p.execution.quota, &epoch, &binding, cached.plan(), &action).map_err(refused)?;
    super::palw_flat_execution_fold_v11::write_shard(builder, &shard, id);
    // Source/class/input restoration and claim decoding occur only after the
    // original persisted metadata attempt has been consumed.
    let class = checked_class(builder, ctx, p, h.class, true)?;
    let mut row =
        builder.state.kernel_route.as_ref().unwrap().stage_job_row_v14(&id).ok_or_else(|| refused("stage funded row corrupt"))?;
    let mut book = StageJobBookV14::from_rooted_bytes(&row.book_bytes, h.book_root).map_err(refused)?;
    let actual = class.check_job(book.job()).map_err(refused)?;
    let plan = derive_stage_quota_v14(&class, &actual).map_err(refused)?;
    book.check_restored(&class, &plan).map_err(refused)?;
    if plan.root() != cached.root() || book.quota_subject(&plan, &p.execution.quota).map_err(refused)? != binding {
        return Err(refused("stage Commit cached quote differs from independent original class/job"));
    }
    let trace = StageTraceV14::from_bytes(body, h.positions).map_err(refused)?;
    let producer = builder.state.bonds.get(signer).ok_or_else(|| refused("original stage producer absent"))?;
    if producer.operator_id.as_bytes() != h.producer_operator {
        return Err(refused("stage producer operator rotated from original job"));
    }
    let free = (producer.collateral as u128).saturating_sub(builder.committed_at(signer, ctx.daa_score));
    let reserve = book
        .commit(
            &class,
            &plan,
            &p.execution.execution,
            palw_kernel_bond_id_v1(signer),
            u64::try_from(free).map_err(|_| refused("stage original producer reserve overflow"))?,
            &trace,
            ctx.daa_score,
        )
        .map_err(refused)?;
    row.header.producer_reserved = reserve;
    row.header.terminal = book.terminal();
    row.header.book_root = book.root();
    row.book_bytes = bytes(&book);
    row.header.book_bytes_len = row.book_bytes.len() as u32;
    row.trace_bytes = body.to_vec();
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(5, &id), Some(bytes(&row)));
    Ok(())
}
fn close_unclaimed(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV14,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    let id: Digest = body.try_into().map_err(|_| refused("stage Close requires exact original job id"))?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let h = r.stage_job_header_v14(&id).ok_or_else(|| refused("original stage Close job absent"))?;
    if h.terminal != StageTerminalV14::Funded || ctx.daa_score <= h.deadline_daa {
        return Err(refused("stage Close requires original expired unclaimed job"));
    }
    let ch = r.stage_class_header_v14(&h.class).ok_or_else(|| refused("stage Close original class absent"))?;
    let sh = r.stage_source_header_v14(&ch.source_binding).ok_or_else(|| refused("stage Close original source absent"))?;
    let fee_work = restore_tariff(builder, h.class, body.len())?
        .checked_add(h.input_raw_bytes.checked_mul(1024).ok_or_else(|| refused("stage Close input tariff overflow"))?)
        .ok_or_else(|| refused("stage Close restore tariff overflow"))?;
    let ordinary = derive_stage_restore_metadata_v14(sh.source_bytes_len, ch.wire_bytes_len, h.book_bytes_len, h.input_raw_bytes, 64)
        .map_err(refused)?;
    let actor = palw_kernel_bond_id_v1(signer);
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    if !prepay(builder, ctx, signer, fee_work)? {
        return Ok(());
    }
    let mut shard = super::palw_flat_execution_fold_v11::quota_shard(builder, &p.execution, &epoch, actor, id)?;
    shard.debit_ordinary(&p.execution.quota, &epoch, &ordinary).map_err(refused)?;
    super::palw_flat_execution_fold_v11::write_shard(builder, &shard, id);
    let class = checked_class(builder, ctx, p, h.class, false)?;
    let mut row = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .stage_job_row_v14(&id)
        .ok_or_else(|| refused("stage Close original bounded row corrupt"))?;
    let mut book = StageJobBookV14::from_rooted_bytes(&row.book_bytes, h.book_root).map_err(refused)?;
    let actual = class.check_job(book.job()).map_err(refused)?;
    let plan = derive_stage_quota_v14(&class, &actual).map_err(refused)?;
    book.check_restored(&class, &plan).map_err(refused)?;
    let counter = builder.state.kernel_route.as_ref().unwrap().execution_nonce_counter_v11(&h.requester_operator).map_err(refused)?;
    if book.original_funded_root() != h.original_book_root
        || book.requester_operator() != h.requester_operator
        || counter.next_nonce <= misaka_palw_kernel::flat_execution11::canonical_nonce_v11(&book.job().nonce).map_err(refused)?
        || row.trace_bytes.len() != 0
    {
        return Err(refused("stage Close lacks original escrow/shared nonce/claim lineage"));
    }
    let binding = book.quota_subject(&plan, &p.execution.quota).map_err(refused)?;
    let paid = shard.debit_expired_terminal(&p.execution.quota, &epoch, &binding, plan.plan()).map_err(refused)?;
    super::palw_flat_execution_fold_v11::write_shard(builder, &shard, id);
    let refund = book.refund_unclaimed(ctx.daa_score).map_err(refused)?;
    let mut queue = misaka_palw_kernel::hash::object_id(
        b"misaka-palw/native/stage-original-principal-refund/v14",
        &(id, h.original_book_root, h.requester_payout),
    );
    queue[0] = crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_PAYOUT_KEY_PREFIX_V1;
    let queue = Hash64::from_bytes(queue);
    if builder.state.pending_payouts.contains_key(&queue) {
        return Err(refused("stage original principal refund queue already occupied"));
    }
    shard
        .mark_expired_stage_terminal_closed_v14(&p.execution.quota, &epoch, &binding, &class, &plan, &book, &paid)
        .map_err(refused)?;
    row.header.book_root = book.root();
    row.header.terminal = book.terminal();
    row.book_bytes = bytes(&book);
    row.header.book_bytes_len = row.book_bytes.len() as u32;
    // Fixed original beneficiary and principal only: public Close signers may
    // not redirect this queue or convert a payment attempt into new income.
    builder.write_payout(queue, Some(PalwPayoutV2 { payload: h.requester_payout, amount: refund }));
    builder.cancel_native_recipient_v1(Hash64::from_bytes(id), h.producer, 14);
    builder.cancel_native_recipient_v1(Hash64::from_bytes(id), h.producer, 19);
    builder.bond_budget_close_v1(
        &Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"misaka-palw/native/stage-original-fixed-budget/v20", &id)),
        ctx.daa_score,
    );
    let credit = NativeStagePrincipalRefundV14 {
        version: 14,
        job: id,
        original_book: h.original_book_root,
        terminal_book: book.root(),
        acceptance18: h.acceptance,
        requester: h.requester,
        payout: h.requester_payout,
        queue,
        amount: refund,
        established_daa: ctx.daa_score,
    };
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(11, &id), Some(bytes(&credit)));
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(5, &id), Some(bytes(&row)));
    super::palw_flat_execution_fold_v11::write_shard(builder, &shard, id);
    Ok(())
}
fn bind(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV14,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    if body.len() > STAGE_SOURCE_MAX_BYTES_V14 {
        return Err(refused("native stage source carrier cap"));
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let mut owners = r.stage_owner_ids_v14(1, signer).ok_or_else(|| refused("stage source owner catalog corrupt"))?;
    if owners.len() >= p.catalog_cap as usize {
        return Err(refused("stage source catalog full"));
    }
    if !prepay_restore(builder, ctx, signer, p, 0, 0, body.len() as u32, tariff(body.len())?)? {
        return Ok(());
    }
    let source = FlatStageSourceWireV14::from_bytes(body).map_err(refused)?;
    let checked = source.check().map_err(refused)?;
    let view = checked.program().v1_view();
    crate::palw_tir_artifact_v1::PalwTirModelInventoryV2::new_stage14(&view, checked.program()).map_err(refused)?;
    let court = misaka_palw_kernel::flat_execution11::native_source_court_bound_v11(source.program_bytes.len(), checked.parameters())
        .map_err(refused)?;
    let cap = &p.execution.quota.plan.0;
    if court.work as u128 > cap[ResourceAxisV10::CourtWork as usize]
        || court.max_proof_bytes as u128 > cap[ResourceAxisV10::CourtBytes as usize]
        || court.scratch_bytes as u128 > cap[ResourceAxisV10::CourtScratchBytes as usize]
    {
        return Err(refused("typed source worst court exceeds immutable native policy"));
    }
    let id = checked.id();
    if builder.state.kernel_route.as_ref().unwrap().stage_source_refuted_relation_v14(&checked.relation_root()) {
        return Err(refused("stage binding attempts a previously refuted original real R/PC relation"));
    }
    if builder.state.kernel_route.as_ref().unwrap().stage_source_header_v14(&id).is_some() {
        return Ok(());
    }
    let free = (builder.state.bonds.get(signer).ok_or_else(|| refused("stage binder absent"))?.collateral as u128)
        .saturating_sub(builder.committed_at(signer, ctx.daa_score));
    if free < p.source_reservation as u128 {
        return Err(refused("typed source original liability unavailable"));
    }
    let header = NativeStageSourceHeaderV14 {
        version: 14,
        id,
        program_root: checked.program_root(),
        input_declarations_root: checked.input_declarations_root(),
        parameters_root: checked.parameters().root(),
        inventory_root: checked.inventory_root(),
        relation_root: checked.relation_root(),
        source_bytes_len: body.len() as u32,
        program_bytes_len: source.program_bytes.len() as u32,
        pc_instances: checked.parameters().by_instance.len() as u32,
        max_proof_bytes: u32::try_from(court.max_proof_bytes).map_err(|_| refused("stage source proof cap overflow"))?,
        binder: *signer,
        bound_daa: ctx.daa_score,
        matures_daa: ctx.daa_score.checked_add(p.source_maturity).ok_or_else(|| refused("stage source maturity overflow"))?,
        final_daa: ctx.daa_score.checked_add(p.source_liability).ok_or_else(|| refused("stage source liability overflow"))?,
        reserved: p.source_reservation,
        refuted: false,
    };
    let row = NativeStageSourceRowV14 { header, source_bytes: body.to_vec() };
    owners.push(id);
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(0, &id), Some(bytes(&row)));
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(1, signer), Some(bytes(&owners)));
    builder.note_kernel_bond(signer);
    Ok(())
}
fn register(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV14,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    let source_id: Digest = body
        .get(2..66)
        .ok_or_else(|| refused("truncated stage class source id"))?
        .try_into()
        .map_err(|_| refused("stage source id differs"))?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let h = r.stage_source_header_v14(&source_id).ok_or_else(|| refused("original stage14 source absent"))?;
    if h.refuted || r.stage_source_refuted_relation_v14(&h.relation_root) || h.matures_daa > ctx.daa_score {
        return Err(refused("stage14 source refuted or not mature"));
    }
    let mut owners = r.stage_owner_ids_v14(3, signer).ok_or_else(|| refused("stage class owner catalog corrupt"))?;
    if owners.len() >= p.catalog_cap as usize {
        return Err(refused("stage class catalog full"));
    }
    let work =
        tariff(body.len())?.checked_add(tariff(h.source_bytes_len as usize)?).ok_or_else(|| refused("stage class tariff overflow"))?;
    if !prepay_restore(builder, ctx, signer, p, h.source_bytes_len, 0, body.len() as u32, work)? {
        return Ok(());
    }
    let wire = FlatStageClassWireV14::from_bytes(body).map_err(refused)?;
    if wire.policy_root != p.root() {
        return Err(refused("stage class lacks exact native policy root"));
    }
    let row = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .stage_source_row_v14(&source_id)
        .ok_or_else(|| refused("stage source bounded row corrupt"))?;
    let source = row.checked_source().map_err(refused)?;
    let class = wire.check(&source).map_err(refused)?;
    if class.maximum_resources().quantities.0.iter().zip(&p.execution.quota.plan.0).any(|(need, cap)| need > cap) {
        return Err(refused("stage maximum exceeds immutable native 21-axis policy"));
    }
    let id = class.root();
    if builder.state.kernel_route.as_ref().unwrap().stage_class_header_v14(&id).is_some() {
        return Ok(());
    }
    let header = NativeStageClassHeaderV14 {
        version: 14,
        id,
        source_binding: source_id,
        template_root: class.template().root().map_err(refused)?,
        policy_root: p.root(),
        max_scope_root: class.maximum_resources().court_scope,
        max_resource_root: class.maximum_resources().root(),
        wire_bytes_len: body.len() as u32,
        owner: *signer,
        admitted_daa: ctx.daa_score,
    };
    let row = NativeStageClassRowV14 { header, class_bytes: body.to_vec() };
    owners.push(id);
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(2, &id), Some(bytes(&row)));
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(3, signer), Some(bytes(&owners)));
    builder.note_kernel_bond(signer);
    Ok(())
}
fn source_fact(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV14,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    if body.len() < 70 || u16::from_le_bytes(body[..2].try_into().unwrap()) != 14 {
        return Err(refused("stage source filing requires exact version14 source/proof frame"));
    }
    let source: Digest = body[2..66].try_into().unwrap();
    let n = u32::from_le_bytes(body[66..70].try_into().unwrap()) as usize;
    let h = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .stage_source_header_v14(&source)
        .ok_or_else(|| refused("stage source filing original source absent"))?;
    if n != body.len() - 70 || n > h.max_proof_bytes as usize {
        return Err(refused("stage source proof exceeds original bounded real-PC frame"));
    }
    if !prepay_restore(
        builder,
        ctx,
        signer,
        p,
        h.source_bytes_len,
        0,
        body.len() as u32,
        tariff(h.source_bytes_len as usize)?
            .checked_add(tariff(body.len())?)
            .ok_or_else(|| refused("stage source filing tariff overflow"))?,
    )? {
        return Ok(());
    }
    let row = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .stage_source_row_v14(&source)
        .ok_or_else(|| refused("stage source original bounded row corrupt"))?;
    let checked = row.checked_source().map_err(refused)?.check().map_err(refused)?;
    let quote = derive_stage_source_filing_v14(&checked, body.len() as u64).map_err(refused)?;
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let actor = palw_kernel_bond_id_v1(signer);
    let mut shard = super::palw_flat_execution_fold_v11::quota_shard(builder, &p.execution, &epoch, actor, [0; 64])?;
    shard.debit_ordinary(&p.execution.quota, &epoch, &quote).map_err(refused)?;
    super::palw_flat_execution_fold_v11::write_shard(builder, &shard, [0; 64]);
    verify_stage_source_proof_v14(&row, &body[70..]).map_err(refused)?;
    let fact = NativeStageNegativeSourceFactV14 {
        version: 14,
        relation: checked.relation_root(),
        source,
        source_program: checked.program_root(),
        parameters: checked.parameters().root(),
        inventory: checked.inventory_root(),
        proof: misaka_palw_kernel::hash::object_id(b"misaka-palw/native/stage-negative-source-proof/v14", &&body[70..]),
        established_daa: ctx.daa_score,
        reporter: *signer,
    };
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(14, &checked.relation_root()), Some(bytes(&fact)));
    // This is an admission-denial fact only. Each existing funded/committed
    // job still needs its own original claim/paid proof/coins court before a
    // finder, binder or producer settlement can be issued.
    Ok(())
}
fn collect_terminal(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV14,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    use misaka_palw_kernel::flat_quota11::{source16::*, trace17::*, *};
    let id: Digest = body.try_into().map_err(|_| refused("stage GC exact original job frame"))?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let h = r.stage_job_header_v14(&id).ok_or_else(|| refused("stage GC original terminal absent"))?;
    if !matches!(h.terminal, StageTerminalV14::Refunded | StageTerminalV14::Convicted) || ctx.daa_score <= h.deadline_daa {
        return Err(refused("stage GC needs original settled job after complete deadline"));
    }
    let pursuit: NativeStagePursuitV14 =
        r.aux_row(STAGE_TABLE_V14, &stage_key_v14(16, &id)).ok_or_else(|| refused("stage GC original pursuit counters absent"))?;
    if pursuit.version != 14
        || pursuit.job != id
        || pursuit.class != h.class
        || pursuit.original_book != h.original_book_root
        || pursuit.consent18 != h.acceptance
        || pursuit.open_demands != 0
        || pursuit.retained_seals != 0
        || pursuit.retained_discovery != 0
        || ctx.daa_score <= pursuit.required_until
        || pursuit.requests_admitted > 32
        || pursuit.source_requests.iter().filter(|k| **k != [0; 64]).count() != pursuit.requests_admitted as usize
        || pursuit.source_units.iter().filter(|k| **k != [0; 64]).count() != pursuit.requests_admitted as usize
    {
        return Err(refused("stage GC original pending/retained counters are live or rebound"));
    }
    let mut cached_bytes = 8192u64 + (pursuit.requests_admitted as u64) * 1536;
    for unit in pursuit.source_units.iter().take(pursuit.requests_admitted as usize) {
        if let Some(raw) = r.aux.get(&(crate::palw_flat_execution_v11::EXECUTION_SERVED_TABLE_V11, bytes(&(16u8, *unit)))) {
            if raw.len() as u64 > misaka_palw_kernel::flat_quota11::source16::SOURCE_ANSWER_MAX_BYTES_V16 {
                return Err(refused("stage GC original source cache exceeds unit envelope"));
            }
            cached_bytes =
                cached_bytes.checked_add(raw.len() as u64).ok_or_else(|| refused("stage GC cache journal extent overflow"))?;
        }
    }
    let ch = r.stage_class_header_v14(&h.class).ok_or_else(|| refused("stage GC original class absent"))?;
    let sh = r.stage_source_header_v14(&ch.source_binding).ok_or_else(|| refused("stage GC original source absent"))?;
    if !prepay_restore(builder, ctx, signer, p, sh.source_bytes_len, ch.wire_bytes_len, 64, restore_tariff(builder, h.class, 64)?)? {
        return Ok(());
    }
    let restore = derive_stage_restore_metadata_v14(
        0,
        0,
        h.book_bytes_len,
        h.input_raw_bytes.checked_add(cached_bytes).ok_or_else(|| refused("stage GC full cache restore overflow"))?,
        0,
    )
    .map_err(refused)?;
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let mut restored =
        super::palw_flat_execution_fold_v11::quota_shard(builder, &p.execution, &epoch, palw_kernel_bond_id_v1(signer), id)?;
    restored.debit_ordinary(&p.execution.quota, &epoch, &restore).map_err(refused)?;
    super::palw_flat_execution_fold_v11::write_shard(builder, &restored, id);
    let r = builder.state.kernel_route.as_ref().unwrap();
    let mut collected_source = Vec::with_capacity(pursuit.requests_admitted as usize);
    for request in pursuit.source_requests.iter().take(pursuit.requests_admitted as usize) {
        let demand = super::palw_flat_execution_fold_v11::source::demand(r, *request)?;
        if !demand.settled || demand.request.job != id {
            return Err(refused("stage GC indexed original source pending unit remains live/rebound"));
        }
        collected_source.push(demand);
    }
    let class = checked_class(builder, ctx, p, h.class, false)?;
    let row = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .stage_job_row_v14(&id)
        .ok_or_else(|| refused("stage GC original bounded job corrupt"))?;
    let book = StageJobBookV14::from_rooted_bytes(&row.book_bytes, h.book_root).map_err(refused)?;
    let actual = class.check_job(book.job()).map_err(refused)?;
    let plan = derive_stage_quota_v14(&class, &actual).map_err(refused)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let credit: NativeStagePrincipalRefundV14 =
        r.aux_row(STAGE_TABLE_V14, &stage_key_v14(11, &id)).ok_or_else(|| refused("stage GC genuine principal credit absent"))?;
    if credit.version != 14
        || credit.job != id
        || credit.original_book != h.original_book_root
        || credit.terminal_book != book.root()
        || credit.acceptance18 != h.acceptance
        || credit.requester != h.requester
        || credit.payout != h.requester_payout
    {
        return Err(refused("stage GC principal credit changes original beneficiary/coins"));
    }
    let counter = r.execution_nonce_counter_v11(&h.requester_operator).map_err(refused)?;
    let native_terminal = NativeStageTerminalAnchorV14 {
        job_id: id,
        original_funded_book_root: h.original_book_root,
        terminal_book_root: book.root(),
        acceptance18: h.acceptance,
        nonce: NativeRequesterNonceWitnessV11 {
            requester: book.job().requester,
            requester_operator_id: h.requester_operator,
            next_nonce: counter.next_nonce,
            counter_root: counter.root(),
        },
        established_daa: credit.established_daa,
        principal_refund: credit.amount,
        principal_queue: credit.queue.as_bytes(),
    };
    let terminal = if book.terminal() == StageTerminalV14::Refunded {
        check_stage_quota_terminal_v14(&class, &book, &plan, &p.execution.quota, &native_terminal)
    } else {
        let outcome: StageSettlementV14 = r
            .aux_row(STAGE_TABLE_V14, &stage_key_v14(13, &id))
            .ok_or_else(|| refused("stage GC original actual settlement absent"))?;
        check_stage_conviction_terminal_v14(
            &class,
            &book,
            &plan,
            &p.execution.quota,
            &native_terminal,
            &outcome,
            outcome.collected_original_slash,
        )
    }
    .map_err(refused)?;
    fn rooted<T: borsh::BorshDeserialize>(
        r: &crate::palw_kernel_route_v1::PalwKernelRouteStateV1,
        key: Vec<u8>,
        cap: u64,
    ) -> Result<(Digest, T), PalwStateV2Error> {
        let raw = r
            .aux
            .get(&(crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11, key))
            .ok_or_else(|| refused("stage GC original quota row absent"))?;
        if raw.len() as u64 != 64 + cap {
            return Err(refused("stage GC quota fixed framing"));
        }
        borsh::from_slice(raw).map_err(|_| refused("stage GC quota fixed row decode"))
    }
    let (source_root, source): (Digest, SourceObligationV16) = rooted(r, bytes(&(16u8, id)), SOURCE_ROW_BYTES_V16)?;
    let (trace_root, trace): (Digest, TraceSupplementV17) = rooted(r, bytes(&(17u8, 0u8, id)), TRACE_ROW_BYTES_V17)?;
    if source.original_consent_root != h.acceptance || trace.original_paid_root != h.acceptance {
        return Err(refused("stage GC source/trace are not original producer consent18"));
    }
    let source_plan = derive_stage_source_plan_v14(&class, &plan, &p.execution.quota, &p.execution.discovery).map_err(refused)?;
    let trace_plan = derive_stage_trace_plan_v14(&class, &plan, &p.execution.quota).map_err(refused)?;
    if source_plan.root() != book.source_plan_root() || trace_plan.root() != book.trace_plan_root() {
        return Err(refused("stage GC changes signed original source/trace quote"));
    }
    let mut requester = r.stage_owner_ids_v14(6, &h.requester).ok_or_else(|| refused("stage GC requester catalog corrupt"))?;
    let mut producer = r.stage_owner_ids_v14(7, &h.producer).ok_or_else(|| refused("stage GC producer catalog corrupt"))?;
    if !requester.contains(&id) || !producer.contains(&id) {
        return Err(refused("stage GC original owner membership absent"));
    }
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let actor = palw_kernel_bond_id_v1(signer);
    let mut shard = super::palw_flat_execution_fold_v11::quota_shard(builder, &p.execution, &epoch, actor, id)?;
    let source_retired =
        retire_expired_source_v16(&mut shard, &p.execution.quota, &epoch, &source_plan, &source, &source_root, &terminal)
            .map_err(refused)?;
    let trace_retired = retire_expired_trace_v17(&mut shard, &p.execution.quota, &epoch, &trace_plan, &trace, &trace_root, &terminal)
        .map_err(refused)?;
    // Current caller pays both original supplemental retirements and complete
    // selected subject GC. No old reservations or nonce counters are refunded.
    shard.collect_closed_expired(&p.execution.quota, &epoch, &terminal).map_err(refused)?;
    requester.retain(|v| *v != id);
    producer.retain(|v| *v != id);
    let tombstone = NativeStageCollectedV14 {
        version: 14,
        job: id,
        class: h.class,
        original_book: h.original_book_root,
        terminal_book: book.root(),
        consent18: h.acceptance,
        requester_operator: h.requester_operator,
        original_nonce: book.job().nonce,
        principal_credit: misaka_palw_kernel::hash::object_id(b"misaka-palw/native/stage-original-principal-credit/v14", &credit),
        source_retired: source_retired.root(),
        trace_retired: trace_retired.root(),
        collected_daa: ctx.daa_score,
    };
    super::palw_flat_execution_fold_v11::write_shard(builder, &shard, id);
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(17, &id), Some(bytes(&tombstone)));
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(6, &h.requester), Some(bytes(&requester)));
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(7, &h.producer), Some(bytes(&producer)));
    for kind in [5u8, 12, 16] {
        builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(kind, &id), None);
    }
    for request in pursuit.source_requests.iter().take(pursuit.requests_admitted as usize) {
        builder.write_kernel_row(crate::palw_flat_execution_v11::EXECUTION_DEMAND_TABLE_V11, bytes(&(16u8, *request)), None);
        builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(18, request), None);
    }
    for demand in collected_source {
        let slot = bytes(&(16u8, demand.requester, demand.request.slot));
        if builder
            .state
            .kernel_route
            .as_ref()
            .unwrap()
            .aux_row::<Digest>(crate::palw_flat_execution_v11::EXECUTION_DEMAND_SLOT_TABLE_V11, &slot)
            == Some(demand.id())
        {
            builder.write_kernel_row(crate::palw_flat_execution_v11::EXECUTION_DEMAND_SLOT_TABLE_V11, slot, None);
        }
    }
    for unit in pursuit.source_units.iter().take(pursuit.requests_admitted as usize) {
        builder.write_kernel_row(crate::palw_flat_execution_v11::EXECUTION_SERVED_TABLE_V11, bytes(&(16u8, *unit)), None);
    }
    for key in [bytes(&(2u8, id)), bytes(&(16u8, id)), bytes(&(16u8, id, 1u8)), bytes(&(17u8, 0u8, id))] {
        builder.write_kernel_row(crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11, key, None);
    }
    Ok(())
}
pub(super) fn apply(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    action: u8,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    if action>=crate::palw_flat_stage_v28::STAGE_ACTION_FIRST_V28 {
        return next28::apply(builder,ctx,signer,action,body);
    }
    let p = policies(builder, ctx)?;
    let result = match action {
        STAGE_ACTION_BIND_V14 => bind(builder, ctx, signer, &p, body),
        STAGE_ACTION_REGISTER_V14 => register(builder, ctx, signer, &p, body),
        STAGE_ACTION_FUND_V14 => fund(builder, ctx, signer, &p, body),
        STAGE_ACTION_COMMIT_V14 => commit(builder, ctx, signer, &p, body),
        STAGE_ACTION_CLOSE_V14 => close_unclaimed(builder, ctx, signer, &p, body),
        STAGE_ACTION_ACCEPT_V14 => accept(builder, ctx, signer, &p, body),
        STAGE_ACTION_ACCEPT_V18 => accept_delivery_v18(builder, ctx, signer, &p, body),
        STAGE_ACTION_SOURCE_REVEAL_V14 => source_fact(builder, ctx, signer, &p, body),
        STAGE_ACTION_COLLECT_V14 => collect_terminal(builder, ctx, signer, &p, body),
        7 => source::request(builder, ctx, signer, &p, body),
        8 => source::answer(builder, ctx, signer, &p, body),
        9 => source::settle(builder, ctx, signer, &p, body),
        16 => court::seal(builder, ctx, signer, &p, body),
        17 => court::reveal(builder, ctx, signer, &p, body),
        18 => court::retire(builder, ctx, signer, &p, body),
        // No wire boolean can enable an unconnected money, source conviction,
        // protected discovery/DA or tensor-output Final path.
        _ => return Err(refused("stage14 action lacks complete native funding/court/DA/Final guarantees")),
    };
    match result {
        Err(PalwStateV2Error::KernelRouteRefused(_)) => Ok(()),
        other => other,
    }
}
