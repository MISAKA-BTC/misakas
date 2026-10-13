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
impl PalwChainStateV2 {
    pub fn native_stage_reserved_v14(&self, bond: &PalwBondKeyV2, daa: u64) -> u128 {
        self.kernel_route.as_ref().map(|r| r.stage_source_reserved_v14(bond, daa) + r.stage_producer_reserved_v14(bond)).unwrap_or(0)
    }
}

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
    if admit && (s.refuted || s.matures_daa > ctx.daa_score) {
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
    if body.len() > STAGE_NATIVE_INPUT_BYTES_MAX_V14 + 2048 {
        return Err(refused("stage Fund carrier cap"));
    }
    if !prepay(builder, ctx, signer, restore_tariff(builder, id, body.len())?)? {
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
    let old_nonce = r.execution_nonce_counter_v11(&requester_operator).map_err(refused)?;
    let next_nonce = old_nonce.advance(&job.nonce).map_err(refused)?;
    let mut owners = r.stage_owner_ids_v14(6, signer).ok_or_else(|| refused("stage funded owner catalog corrupt"))?;
    let mut producer_jobs = r.stage_owner_ids_v14(7, &request.producer).ok_or_else(|| refused("stage producer catalog corrupt"))?;
    if owners.len() >= p.catalog_cap as usize || producer_jobs.len() >= p.catalog_cap as usize {
        return Err(refused("stage funded owner/producer catalog full"));
    }
    let actual = class.check_job(&job).map_err(refused)?;
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
    let plan = derive_stage_quota_v14(&class, &actual).map_err(refused)?;
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
    let grant = check_stage_quota_admission_v14(
        &class,
        &book,
        &plan,
        &p.execution.quota,
        &NativeStageQuotaAnchorV14 {
            class: id,
            source: source.id,
            policy: p.root(),
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
        },
    )
    .map_err(refused)?;
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let mut shard = super::palw_flat_execution_fold_v11::quota_shard(builder, &p.execution, &epoch, producer, job_id)?;
    shard.admit_baseline(&p.execution.quota, &epoch, &grant, plan.plan()).map_err(refused)?;
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
    let debit = builder.slash_bond(*signer, request.reward as u128)?;
    if debit != request.reward {
        return Err(refused("stage actual original coin debit differs"));
    }
    super::palw_flat_execution_fold_v11::write_shard(builder, &shard, job_id);
    owners.push(job_id);
    producer_jobs.push(job_id);
    builder.write_kernel_row(
        crate::palw_flat_execution_v11::EXECUTION_REQUEST_INDEX_TABLE_V11,
        bytes(&requester_operator),
        Some(bytes(&next_nonce)),
    );
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(5, &job_id), Some(bytes(&row)));
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(6, signer), Some(bytes(&owners)));
    builder.write_kernel_row(STAGE_TABLE_V14, stage_key_v14(7, &request.producer), Some(bytes(&producer_jobs)));
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
    let class = checked_class(builder, ctx, p, h.class, true)?;
    let mut row =
        builder.state.kernel_route.as_ref().unwrap().stage_job_row_v14(&id).ok_or_else(|| refused("stage funded row corrupt"))?;
    let mut book = StageJobBookV14::from_rooted_bytes(&row.book_bytes, h.book_root).map_err(refused)?;
    let actual = class.check_job(book.job()).map_err(refused)?;
    let plan = derive_stage_quota_v14(&class, &actual).map_err(refused)?;
    book.check_restored(&class, &plan).map_err(refused)?;
    let binding = book.quota_subject(&plan, &p.execution.quota).map_err(refused)?;
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let mut shard = super::palw_flat_execution_fold_v11::quota_shard(builder, &p.execution, &epoch, binding.producer, id)?;
    let action = plan.plan().action(QuotaActionKindV11::Metadata, body).map_err(refused)?;
    shard.debit_reserved(&p.execution.quota, &epoch, &binding, plan.plan(), &action).map_err(refused)?;
    super::palw_flat_execution_fold_v11::write_shard(builder, &shard, id);
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
    if !prepay(builder, ctx, signer, tariff(body.len())?)? {
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
    if h.refuted || h.matures_daa > ctx.daa_score {
        return Err(refused("stage14 source refuted or not mature"));
    }
    let mut owners = r.stage_owner_ids_v14(3, signer).ok_or_else(|| refused("stage class owner catalog corrupt"))?;
    if owners.len() >= p.catalog_cap as usize {
        return Err(refused("stage class catalog full"));
    }
    let work =
        tariff(body.len())?.checked_add(tariff(h.source_bytes_len as usize)?).ok_or_else(|| refused("stage class tariff overflow"))?;
    if !prepay(builder, ctx, signer, work)? {
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
pub(super) fn apply(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    action: u8,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    let p = policies(builder, ctx)?;
    let result = match action {
        STAGE_ACTION_BIND_V14 => bind(builder, ctx, signer, &p, body),
        STAGE_ACTION_REGISTER_V14 => register(builder, ctx, signer, &p, body),
        STAGE_ACTION_FUND_V14 => fund(builder, ctx, signer, &p, body),
        STAGE_ACTION_COMMIT_V14 => commit(builder, ctx, signer, &p, body),
        // No wire boolean can enable an unconnected money, source conviction,
        // protected discovery/DA or tensor-output Final path.
        _ => return Err(refused("stage14 action lacks complete native funding/court/DA/Final guarantees")),
    };
    match result {
        Err(PalwStateV2Error::KernelRouteRefused(_)) => Ok(()),
        other => other,
    }
}
