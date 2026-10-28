//! Original stage28 native journal. Every new action has an independent28 fence
//! and immutable before-Fund temporal/source/court/physical/numerical contract.
use super::{
    PalwBlockContextV2, PalwBondKeyV2, PalwBondStatusV2, PalwPayoutV2, PalwStateV2Error, TransitionBuilder, bytes, prepay, refused,
    tariff,
};
use crate::{
    palw_flat_stage_v14::{NativeStageAcceptanceV14, STAGE_TABLE_V14, StageAcceptRequestV14, StageFundRequestV14},
    palw_flat_stage_v28::*,
    palw_kernel_route_v1::{PalwKernelRouteStateV1, palw_kernel_bond_id_v1},
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{
    flat_inputs12::{RawExternalInputV12, RawExternalInputsV12, bounded14::FlatStageJobV14, bounded28::*},
    flat_quota11::{NativeRequesterNonceWitnessV11, TrustedEpochV11, stage14::StageTraceV14, stage14::bounded28::*},
    hash::{Digest, object_id},
};
#[path = "palw_flat_stage_master_fold_v32.rs"]
mod master32;
#[path = "palw_flat_stage_source_fold_v28.rs"]
mod source28;
fn key<T: BorshSerialize>(kind: u8, id: &T) -> Vec<u8> {
    bytes(&(28u8, kind, id))
}
fn read<T: BorshDeserialize>(r: &PalwKernelRouteStateV1, kind: u8, id: &impl BorshSerialize, cap: usize) -> Option<T> {
    let raw = r.aux.get(&(STAGE_TABLE_V14, key(kind, id)))?;
    (raw.len() <= cap).then(|| borsh::from_slice(raw).ok()).flatten()
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct BundleHeader {
    version: u16,
    id: Digest,
    bytes: u32,
    owner: PalwBondKeyV2,
    registered: u64,
    source_ids: Vec<Digest>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct ClassHeader {
    version: u16,
    id: Digest,
    bundle: Digest,
    source: Digest,
    policy: Digest,
    scope: Digest,
    resource: Digest,
    bytes: u32,
    owner: PalwBondKeyV2,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct JobHeader {
    version: u16,
    id: Digest,
    class: Digest,
    consent: Digest,
    original: Digest,
    book: Digest,
    contract: NativeStageMaximumV28,
    requester: PalwBondKeyV2,
    producer: PalwBondKeyV2,
    requester_payout: crate::Hash64,
    producer_payout: crate::Hash64,
    book_bytes: u32,
    input_bytes: u32,
    trace_bytes: u32,
    positions: u32,
    quota_record: misaka_palw_kernel::flat_quota11::QuotaPlanRecordV11,
    source_requests_per_owner: u32,
    restore_bytes: u64,
    terminal: StageTerminalV28,
    producer_reserved: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct JobData {
    book: Vec<u8>,
    inputs: Vec<Vec<u8>>,
    trace: Vec<u8>,
}
fn policy(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<NativeStagePoliciesV28, PalwStateV2Error> {
    super::super::palw_flat_model_vector_fold_v3::enabled(builder, ctx)?;
    let r = builder.extras.kernel_route.as_ref().ok_or_else(|| refused("stage28 route extras absent"))?;
    let fence = r
        .stage_execution_v28
        .as_ref()
        .filter(|f| f.activation.is_active(ctx.daa_score))
        .ok_or_else(|| refused("stage28 independent fence inactive"))?;
    let final_fence = r
        .native_final
        .as_ref()
        .filter(|f| f.activation.is_active(ctx.daa_score))
        .ok_or_else(|| refused("stage28 Final27 independent fence inactive"))?;
    let p = fence.policies.clone();
    p.check(builder.state.kernel_route.as_ref().unwrap().header.policy.network_domain).map_err(refused)?;
    if p.final_policy != final_fence.policy.root() {
        return Err(refused("stage28 original Final authority differs from trusted Params"));
    }
    let service = r
        .native_carriage_service
        .as_ref()
        .filter(|s| s.implemented && s.root() == p.physical_service)
        .ok_or_else(|| refused("stage28 actual reserved physical service absent"))?;
    service.check(r.native_carriage_mass.as_ref().ok_or_else(|| refused("stage28 physical mass absent"))?).map_err(refused)?;
    for (table, k, raw) in [
        (STAGE_TABLE_V14, key(4, &()), bytes(&p)),
        (crate::palw_flat_execution_v11::EXECUTION_POLICY_TABLE_V11, Vec::new(), bytes(&p.execution)),
    ] {
        let old = builder.state.kernel_route.as_ref().unwrap().aux.get(&(table, k.clone()));
        if old.is_some_and(|v| v != &raw) {
            return Err(refused("stage28 immutable/shared native policy changed"));
        }
        if old.is_none() {
            builder.write_kernel_row(table, k, Some(raw))?;
        }
    }
    Ok(p)
}
fn pay_restore(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV28,
    source: u32,
    class: u32,
    inputs: u32,
    body: u32,
) -> Result<bool, PalwStateV2Error> {
    let q = derive_stage_restore_metadata_v28(source, class, 8192, inputs, body).map_err(refused)?;
    let work = tariff(source as usize)?
        .checked_add(tariff(class as usize)?)
        .and_then(|n| n.checked_add(tariff(inputs as usize).ok()?))
        .and_then(|n| n.checked_add(body as u64 * 1024))
        .ok_or_else(|| refused("stage28 paid restore tariff overflow"))?;
    if !prepay(builder, ctx, signer, work)? {
        return Ok(false);
    }
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let mut shard = super::super::palw_flat_execution_fold_v11::quota_shard(
        builder,
        &p.execution,
        &epoch,
        palw_kernel_bond_id_v1(signer),
        [0; 64],
    )?;
    shard.debit_ordinary(&p.execution.quota, &epoch, &q).map_err(refused)?;
    super::super::palw_flat_execution_fold_v11::write_shard(builder, &shard, [0; 64])?;
    Ok(true)
}
fn restore_class_impl(
    builder: &TransitionBuilder<'_>,
    p: &NativeStagePoliciesV28,
    id: Digest,
    allow_refuted: bool,
) -> Result<CheckedStageClassV28, PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let h: ClassHeader = read(r, 2, &id, 1024).ok_or_else(|| refused("stage28 class header absent"))?;
    let bh: BundleHeader = read(r, 0, &h.bundle, 8192).ok_or_else(|| refused("stage28 original pipeline header absent"))?;
    let raw: Vec<u8> = read(r, 1, &h.bundle, 16 << 20).ok_or_else(|| refused("stage28 original complete source bundle absent"))?;
    if raw.len() != bh.bytes as usize {
        return Err(refused("stage28 original source byte count changed"));
    }
    let bundle = NativeStageSourceBundleV28::from_bytes(&raw).map_err(refused)?;
    let (manifest, sources) = bundle.check().map_err(refused)?;
    let raw: Vec<u8> = read(r, 3, &id, 1 << 20).ok_or_else(|| refused("stage28 exact class wire absent"))?;
    let wire = FlatStageClassWireV28::from_bytes(&raw).map_err(refused)?;
    let source = sources.get(wire.stage as usize).ok_or_else(|| refused("stage28 source stage absent"))?;
    let mut class = wire.check(source).map_err(refused)?;
    class.bind_complete_bundle(&manifest).map_err(refused)?;
    if h.version != 28
        || h.id != id
        || h.bundle != manifest.root()
        || h.source != source.id()
        || h.policy != p.root()
        || wire.policy != p.root()
        || class.root() != id
        || h.scope != class.maximum_scope().root()
        || h.resource != class.maximum_resources().root()
        || h.bytes as usize != raw.len()
        || bh.source_ids != sources.iter().map(|s| s.id()).collect::<Vec<_>>()
        || !allow_refuted && r.stage_source_refuted_relation_v14(&source.relation_root())
    {
        return Err(refused("stage28 original source/class/manifest/maxima independently differ or refuted"));
    }
    Ok(class)
}
fn restore_class(
    builder: &TransitionBuilder<'_>,
    p: &NativeStagePoliciesV28,
    id: Digest,
) -> Result<CheckedStageClassV28, PalwStateV2Error> {
    restore_class_impl(builder, p, id, false)
}
fn original_header(builder: &TransitionBuilder<'_>, id: Digest) -> Result<JobHeader, PalwStateV2Error> {
    read(builder.state.kernel_route.as_ref().unwrap(), 5, &id, 8192).ok_or_else(|| refused("stage28 original funded header absent"))
}
pub(super) fn reserved(r: &PalwKernelRouteStateV1, bond: &PalwBondKeyV2, _daa: u64) -> u128 {
    let producer = read::<Vec<Digest>>(r, 13, bond, 4 + 8 * 64)
        .unwrap_or_default()
        .iter()
        .take(8)
        .filter_map(|id| read::<JobHeader>(r, 5, id, 8192))
        .filter(|h| h.producer == *bond && matches!(h.terminal, StageTerminalV28::Funded | StageTerminalV28::Challengeable))
        .map(|h| h.producer_reserved as u128)
        .sum::<u128>();
    producer.saturating_add(source28::reserved(r, bond))
}

fn restore_job(
    builder: &TransitionBuilder<'_>,
    p: &NativeStagePoliciesV28,
    h: &JobHeader,
) -> Result<(CheckedStageClassV28, CheckedStageJobV28, CheckedStageQuotaV28, StageJobBookV28, JobData), PalwStateV2Error> {
    let class = restore_class_impl(builder, p, h.class, true)?;
    let data: JobData = read(builder.state.kernel_route.as_ref().unwrap(), 6, &h.id, (1 << 20) + (2 << 20) + 16384)
        .ok_or_else(|| refused("stage28 bounded original job body absent"))?;
    if data.book.len() != h.book_bytes as usize
        || data.trace.len() != h.trace_bytes as usize
        || data.inputs.iter().map(|v| v.len()).sum::<usize>() != h.input_bytes as usize
    {
        return Err(refused("stage28 fixed original body extents differ"));
    }
    let book: StageJobBookV28 = borsh::from_slice(&data.book).map_err(|_| refused("stage28 original fixed book grammar"))?;
    let actual = class.check_job(book.job()).map_err(refused)?;
    let plan = if master32::binding(builder.state.kernel_route.as_ref().unwrap(), h.id)?.is_some() {
        derive_stage_quota_v32(&class, &actual)
    } else {
        derive_stage_quota_v28(&class, &actual)
    }
    .map_err(refused)?;
    book.check(&class, &actual, &plan).map_err(refused)?;
    if book.id() != h.id
        || book.root() != h.book
        || book.original_root() != h.original
        || book.class() != h.class
        || book.consent() != h.consent
        || book.contract() != h.contract.root()
        || book.quota_root() != plan.root()
        || book.terminal() != h.terminal
    {
        return Err(refused("stage28 original native book/max contract differ"));
    }
    Ok((class, actual, plan, book, data))
}
fn contract(
    builder: &TransitionBuilder<'_>,
    p: &NativeStagePoliciesV28,
    class: &CheckedStageClassV28,
    job: &CheckedStageJobV28,
    quota: &CheckedStageQuotaV28,
) -> Result<CheckedNativeStageMaximumV28, PalwStateV2Error> {
    let r = builder.extras.kernel_route.as_ref().unwrap();
    let ledger = super::super::palw_kernel_route_fold_v1::route_ledger_policy_v1(builder)?;
    CheckedNativeStageMaximumV28::derive(
        p,
        class,
        job,
        quota,
        &r.native_final.as_ref().unwrap().policy,
        &ledger,
        r.native_carriage_service.as_ref().unwrap(),
        r.native_carriage_mass.as_ref().unwrap(),
    )
    .map_err(refused)
}
fn original_extents(builder: &TransitionBuilder<'_>, class: Digest) -> Result<(u32, u32), PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let ch: ClassHeader = read(r, 2, &class, 1024).ok_or_else(|| refused("stage28 class fixed header absent"))?;
    let bh: BundleHeader = read(r, 0, &ch.bundle, 8192).ok_or_else(|| refused("stage28 source fixed header absent"))?;
    Ok((bh.bytes, ch.bytes))
}
fn accept(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV28,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    accept_impl(builder, ctx, signer, p, body, false)
}
fn accept_impl(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV28,
    body: &[u8],
    invocation32: bool,
) -> Result<(), PalwStateV2Error> {
    let id: Digest = body.get(8..72).ok_or_else(|| refused("stage28 acceptance class fixed prefix absent"))?.try_into().unwrap();
    let (source_bytes, class_bytes) = original_extents(builder, id)?;
    if !pay_restore(builder, ctx, signer, p, source_bytes, class_bytes, 0, body.len() as u32)? {
        return Ok(());
    }
    let request = StageAcceptSourceV28::from_bytes(body).map_err(refused)?;
    let prep = StageAcceptRequestV14::from_bytes(&request.preparation14).map_err(refused)?;
    let job = FlatStageJobV14::from_bytes(&prep.job_bytes).map_err(refused)?;
    let class = restore_class(builder, p, id)?;
    let actual = class.check_job(&job).map_err(refused)?;
    let quota = if invocation32 { derive_stage_quota_v32(&class, &actual) } else { derive_stage_quota_v28(&class, &actual) }
        .map_err(refused)?;
    let maximum = contract(builder, p, &class, &actual, &quota)?;
    let m = maximum.record();
    let r = builder.state.kernel_route.as_ref().unwrap();
    let requester = builder.state.bonds.get(&prep.requester).ok_or_else(|| refused("stage28 original requester absent"))?;
    let producer = builder.state.bonds.get(signer).ok_or_else(|| refused("stage28 original producer absent"))?;
    if !matches!(requester.status, PalwBondStatusV2::Active)
        || !matches!(producer.status, PalwBondStatusV2::Active)
        || job.requester != palw_kernel_bond_id_v1(&prep.requester)
        || job.producer != palw_kernel_bond_id_v1(signer)
        || requester.operator_id == producer.operator_id
        || prep.class != class.root()
        || prep.source != class.source().id()
        || prep.policy != p.root()
        || prep.admission != actual.admission().root()
        || prep.quota != quota.root()
        || prep.reward == 0
        || prep.reward > p.execution.execution.reward_cap
        || prep.expires_daa < ctx.daa_score
        || prep.expires_daa / p.execution.quota.interval_daa != ctx.daa_score / p.execution.quota.interval_daa
        || prep.expires_daa
            > ctx
                .daa_score
                .checked_add(p.execution.execution.horizons.commit_window)
                .ok_or_else(|| refused("stage28 consent expiry overflow"))?
        || request.source_plan != m.source_plan
        || request.trace_plan != m.paged_plan
        || request.caller_source26 != m.caller_source
        || request.temporal28 != m.temporal.root()
        || request.checker25 != class.maximum_scope().court_set()
        || request.whole_claim != m.final_basis.root()
        || request.final_policy27 != p.final_policy
        || request.physical_service != p.physical_service
        || request.fee_plan != m.delivery_fee_terms
        || request.policy28 != p.root()
        || request.physical_plan != m.physical_plan
        || request.maximum_contract != m.root()
        || request.queue_plan != m.queue_plan
        || request.queue_policy != m.queue_policy
    {
        return Err(refused(
            "stage28 acceptance differs from exact independent source/input/maxima/timing/physical/Final/fee contract",
        ));
    }
    m.temporal.deadline(ctx.daa_score, p.execution.quota.interval_daa).map_err(refused)?;
    let nonce = r.execution_nonce_counter_v11(&requester.operator_id.as_bytes()).map_err(refused)?;
    nonce.advance(&job.nonce).map_err(refused)?;
    let mut ids: Vec<Digest> = read(r, 9, signer, 4 + 8 * 64).unwrap_or_default();
    // Only this producer can create its consent rows; terminal/spent anchors
    // remain retained. Expired metadata releases its finite own catalog slot.
    ids.retain(|id| read::<NativeStageConsentV28>(r, 10, id, 4096).is_some_and(|v| v.original14.expires_daa >= ctx.daa_score));
    if ids.len() >= p.catalog_cap as usize {
        return Err(refused("stage28 finite own consent catalog full"));
    }
    let a = NativeStageAcceptanceV14 {
        version: 14,
        id: [0; 64],
        class: id,
        source: class.source().id(),
        policy: p.root(),
        job: job.root(),
        admission: actual.admission().root(),
        quota: quota.root(),
        requester: prep.requester,
        producer: *signer,
        requester_operator: requester.operator_id.as_bytes(),
        producer_operator: producer.operator_id.as_bytes(),
        nonce: job.nonce,
        reward: prep.reward,
        accepted_daa: ctx.daa_score,
        expires_daa: prep.expires_daa,
    };
    let mut consent = NativeStageConsentV28 {
        version: 28,
        id: [0; 64],
        original14: a,
        source_plan: m.source_plan,
        trace_plan: m.paged_plan,
        caller_source26: m.caller_source,
        temporal28: m.temporal.root(),
        checker25: class.maximum_scope().court_set(),
        whole_claim: m.final_basis.root(),
        final_policy27: p.final_policy,
        physical_service: p.physical_service,
        fee_plan: m.delivery_fee_terms,
        policy28: p.root(),
        physical_plan: m.physical_plan,
        maximum_contract: m.root(),
        queue_plan: m.queue_plan,
        queue_policy: m.queue_policy,
        accepted_block: ctx.block,
    };
    consent.id = consent.root();
    if read::<NativeStageConsentV28>(r, 10, &consent.id, 4096).is_some() {
        return Err(refused("stage28 original producer acceptance already exists"));
    }
    ids.push(consent.id);
    builder.write_kernel_row(STAGE_TABLE_V14, key(10, &consent.id), Some(bytes(&consent)))?;
    builder.write_kernel_row(STAGE_TABLE_V14, key(9, signer), Some(bytes(&ids)))?;
    builder.note_kernel_bond(signer)?;
    Ok(())
}
fn fund(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV28,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    fund_impl(builder, ctx, signer, p, body, false)
}
fn fund_impl(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV28,
    body: &[u8],
    invocation32: bool,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_flat_execution_capacity_v22::NativeCarriageGlobalV22;
    let class_id: Digest = body.get(2..66).ok_or_else(|| refused("stage28 Fund fixed original class absent"))?.try_into().unwrap();
    let (s, c) = original_extents(builder, class_id)?;
    if body.len() > (1 << 20) + 2112 || !pay_restore(builder, ctx, signer, p, s, c, p.max_input_bytes, body.len() as u32)? {
        return Ok(());
    }
    let request = StageFundRequestV14::from_bytes(body).map_err(refused)?;
    let job = FlatStageJobV14::from_bytes(&request.job_bytes).map_err(refused)?;
    let class = restore_class(builder, p, class_id)?;
    let actual = class.check_job(&job).map_err(refused)?;
    let plan = if invocation32 { derive_stage_quota_v32(&class, &actual) } else { derive_stage_quota_v28(&class, &actual) }
        .map_err(refused)?;
    let maximum = contract(builder, p, &class, &actual, &plan)?;
    let m = maximum.record();
    if !invocation32
        && (!matches!(
            actual.program().output,
            misaka_palw_tir::program_v2::OutputDecl::Rows { .. } | misaka_palw_tir::program_v2::OutputDecl::Final { .. }
        ) || !actual.program().post_writes().is_empty()
            || actual.program().inputs.iter().any(|d| matches!(d.source, misaka_palw_tir::program_v2::InputSource::Random { .. }))
            || actual
                .program()
                .blocks
                .iter()
                .flat_map(|b| &b.nodes)
                .flat_map(|n| &n.inputs)
                .any(|r| *r == misaka_palw_tir::Ref::Input(0)))
    {
        return Err(refused(
            "stage28 effects/token stage requires explicit original invocation32 master/state/random/terminal contract",
        ));
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let consent: NativeStageConsentV28 = read(r, 10, &request.acceptance, 4096)
        .ok_or_else(|| refused("stage28 requires original producer signed independent28 acceptance"))?;
    let a = &consent.original14;
    let requester = builder.state.bonds.get(signer).ok_or_else(|| refused("stage28 requester absent"))?;
    let producer = builder.state.bonds.get(&request.producer).ok_or_else(|| refused("stage28 producer absent"))?;
    if consent.version != 28
        || consent.id != consent.root()
        || requester.operator_id.as_bytes() != a.requester_operator
        || producer.operator_id.as_bytes() != a.producer_operator
        || !matches!(requester.status, PalwBondStatusV2::Active)
        || !matches!(producer.status, PalwBondStatusV2::Active)
        || a.requester != *signer
        || a.producer != request.producer
        || a.class != class_id
        || a.source != class.source().id()
        || a.policy != p.root()
        || a.job != job.root()
        || a.admission != actual.admission().root()
        || a.quota != plan.root()
        || a.nonce != job.nonce
        || a.reward != request.reward
        || ctx.daa_score < a.accepted_daa
        || ctx.daa_score > a.expires_daa
        || consent.maximum_contract != m.root()
        || consent.physical_plan != m.physical_plan
        || consent.queue_plan != m.queue_plan
        || consent.queue_policy != m.queue_policy
        || consent.source_plan != m.source_plan
        || consent.trace_plan != m.paged_plan
        || consent.caller_source26 != m.caller_source
        || consent.temporal28 != m.temporal.root()
        || consent.whole_claim != m.final_basis.root()
        || consent.fee_plan != m.delivery_fee_terms
        || consent.final_policy27 != p.final_policy
        || consent.physical_service != p.physical_service
        || r.aux.contains_key(&(STAGE_TABLE_V14, key(11, &consent.id)))
    {
        return Err(refused(
            "stage28 Fund changes original before-Fund consent/operators/source/input/maxima/economics/deadlines or is already consumed",
        ));
    }
    let mut consent_ids: Vec<Digest> =
        read(r, 9, &request.producer, 4 + 8 * 64).ok_or_else(|| refused("stage28 original acceptance catalog absent"))?;
    if !consent_ids.contains(&consent.id) {
        return Err(refused("stage28 original acceptance catalog membership differs"));
    }
    consent_ids.retain(|v| *v != consent.id);
    let old_nonce = r.execution_nonce_counter_v11(&a.requester_operator).map_err(refused)?;
    let next_nonce = old_nonce.advance(&job.nonce).map_err(refused)?;
    let mut requester_ids: Vec<Digest> = read(r, 12, signer, 4 + 8 * 64).unwrap_or_default();
    let mut producer_ids: Vec<Digest> = read(r, 13, &request.producer, 4 + 8 * 64).unwrap_or_default();
    if requester_ids.len() >= p.catalog_cap as usize || producer_ids.len() >= p.catalog_cap as usize {
        return Err(refused("stage28 finite original requester/producer job slots exhausted"));
    }
    let requester_payout = requester.payout_payload;
    let producer_payout = producer.payout_payload;
    if (requester.collateral as u128).saturating_sub(builder.committed_at(signer, ctx.daa_score)) < request.reward as u128
        || (producer.collateral as u128).saturating_sub(builder.committed_at(&request.producer, ctx.daa_score))
            < m.final_basis.original_producer_reserve_sompi as u128
    {
        return Err(refused("stage28 actual original requester escrow or whole-claim collectible producer reserve unavailable"));
    }
    struct Images<'a> {
        program: &'a misaka_palw_tir::program_v2::TirProgramV2,
        images: &'a [Vec<u8>],
    }
    impl RawExternalInputsV12 for Images<'_> {
        fn input(&self, k: u16) -> Option<RawExternalInputV12<'_>> {
            let d = self.program.inputs.get(k as usize)?;
            Some(RawExternalInputV12 { dtype: d.dtype, shape: &d.shape, bytes: self.images.get(k as usize)? })
        }
    }
    if request.input_images.len() != actual.program().inputs.len() {
        return Err(refused("stage28 actual image count differs"));
    }
    actual.acquire(&Images { program: actual.program(), images: &request.input_images }).map_err(refused)?;
    let deadline = m.temporal.deadline(ctx.daa_score, p.execution.quota.interval_daa).map_err(refused)?;
    let commit_until =
        ctx.daa_score.checked_add(m.temporal.commit_window).ok_or_else(|| refused("stage28 original Commit deadline overflow"))?;
    let witness = NativeRequesterNonceWitnessV11 {
        requester: job.requester,
        requester_operator_id: a.requester_operator,
        counter_root: next_nonce.root(),
        next_nonce: next_nonce.next_nonce,
    };
    let receipt = object_id(
        b"misaka-palw/native/stage-actual-original-escrow-debit/v28",
        &(ctx.block.as_bytes(), *signer, a.requester_operator, job.nonce, m.root(), request.reward),
    );
    let book = StageJobBookV28::fund(
        &class,
        &actual,
        &plan,
        &p.execution.quota,
        &NativeStageFundingV28 {
            class: class_id,
            source: class.source().id(),
            policy: p.root(),
            consent: consent.id,
            contract: m.root(),
            temporal: m.temporal.root(),
            source_plan: m.source_plan,
            caller_source: m.caller_source,
            paged_plan: m.paged_plan,
            final_policy: p.final_policy,
            final_basis: m.final_basis.root(),
            physical: p.physical_service,
            requester_operator: a.requester_operator,
            producer_operator: a.producer_operator,
            reward: request.reward,
            actual_debit: request.reward,
            debit_receipt: receipt,
            reserve: m.final_basis.original_producer_reserve_sompi,
            finder: m.final_basis.original_finder_award_sompi,
            nonce: witness.clone(),
            admitted: ctx.daa_score,
            commit_until,
            challenge_window: m.temporal.challenge_window,
            final_wait: m
                .temporal
                .tail
                .checked_sub(p.execution.execution.horizons.retention)
                .ok_or_else(|| refused("stage28 original final tail overflow"))?,
            deadline,
        },
    )
    .map_err(refused)?;
    let id = book.id();
    if read::<JobHeader>(builder.state.kernel_route.as_ref().unwrap(), 5, &id, 8192).is_some() {
        return Err(refused("stage28 original funded job exists"));
    }
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let grant = check_stage_quota_admission_v28(&book, &plan, &p.execution.quota, book.root(), &witness, &epoch).map_err(refused)?;
    let mut shard = super::super::palw_flat_execution_fold_v11::quota_shard(builder, &p.execution, &epoch, job.producer, id)?;
    shard.admit_baseline(&p.execution.quota, &epoch, &grant, plan.plan()).map_err(refused)?;
    let source_grant = check_stage_source_admission_v28(
        &class,
        &actual,
        &book,
        &plan,
        &p.execution.quota,
        &p.execution.discovery,
        maximum.source(),
        book.root(),
        &witness,
        &epoch,
    )
    .map_err(refused)?;
    let mut source_shard = super::super::palw_flat_execution_fold_v11::source_shard_from_rows(
        &p.execution,
        maximum.source(),
        &shard.global_row(),
        &shard.actor_row(),
        shard.subject_row().unwrap(),
        None,
    )?;
    source_shard.admit(&p.execution.quota, &epoch, maximum.source(), &source_grant).map_err(refused)?;
    if invocation32 {
        for _ in 0..(invocation_commit_parts_bound_v32(job.positions).map_err(refused)? as u32 + 1) {
            source_shard.admit_valid_request(&p.execution.quota, &epoch, maximum.source(), &id).map_err(refused)?;
        }
    }
    let extras = builder.extras.kernel_route.as_ref().unwrap();
    let service = extras.native_carriage_service.as_ref().unwrap();
    let mass = extras.native_carriage_mass.as_ref().unwrap();
    let global_key = bytes(&(22u8, 0u8));
    let mut global = match builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .aux
        .get(&(crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11, global_key.clone()))
    {
        None => NativeCarriageGlobalV22::empty(service),
        Some(raw) => {
            let (root, g): (Digest, NativeCarriageGlobalV22) =
                borsh::from_slice(raw).map_err(|_| refused("stage28 shared physical global grammar"))?;
            if g.root() != root {
                return Err(refused("stage28 shared physical global root differs"));
            }
            g
        }
    };
    let queue_policy = &extras.native_final.as_ref().unwrap().policy.queue_policy;
    let queue_key = bytes(&(30u8, 0u8));
    let mut queue_global = match builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .aux
        .get(&(crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11, queue_key.clone()))
    {
        None => crate::palw_native_queue_v30::NativeQueueGlobalV30::empty(queue_policy, service).map_err(refused)?,
        Some(raw) => {
            if raw.len() > crate::palw_native_queue_v30::NATIVE_QUEUE_ROW_LIMIT_V30 {
                return Err(refused("stage28 original shared queue cap"));
            }
            let (root, q): (Digest, crate::palw_native_queue_v30::NativeQueueGlobalV30) =
                borsh::from_slice(raw).map_err(|_| refused("stage28 original shared queue grammar"))?;
            if q.root() != root {
                return Err(refused("stage28 original shared queue root differs"));
            }
            q
        }
    };
    let (queue_lease, lease) = queue_global
        .admit(&mut global, queue_policy, service, mass, maximum.queue(), id, book.root(), consent.id, ctx.daa_score, deadline)
        .map_err(refused)?;
    if builder.bond_budget_mirror_at(ctx.daa_score).is_some_and(|mirror| mirror.allocation_at(ctx.daa_score).is_some()) {
        return Err(refused("stage28 budget lacks independently authenticated native model-line allocation"));
    }
    // RFC-0015 §8.3 (lead default 2026-10-15): Panel-free stage28 work is refused unbudgeted where the OPV policy requires a budget.
    if builder.bond_budget_mirror_at(ctx.daa_score).is_none() {
        builder.unbudgeted_panel_free_v1()?;
    }
    // ADR-0176 amendment (ユーザー判断 2026-10-15): the producer's liability past `palw_bond_formula_v2` is at least the formula's.
    let liability = builder.bond_formula_producer_liability_v2(
        ctx,
        crate::palw_bond_formula_v2::PalwClaimTypeV2::NativeFunded,
        request.reward,
        book.reserve() as u128,
    );
    let budget = builder.plan_bond_budget_v1(
        ctx,
        request.producer,
        None,
        crate::palw_bond_budget_v1::PalwBudgetAskV1 { block_units: 0, reward_sompi: 0, final_weight: 0, liability_sompi: liability },
        crate::palw_bond_budget_v1::PalwBudgetOriginV1::KernelRoute,
        0,
    )?;
    // Every fallible identity/quote/collateral/quota/lease preflight completed.
    // Native money failures abort the complete transition, not a caught refusal.
    builder.hold_native_recipient_v1(
        crate::Hash64::from_bytes(id),
        request.producer,
        14,
        ctx.daa_score,
        deadline,
        request.reward,
        &[],
    )?;
    if let Some(budget) = budget {
        builder.commit_bond_budget_v1(
            ctx,
            crate::Hash64::from_bytes(object_id(b"misaka-palw/native/stage-original-fixed-budget/v28", &id)),
            budget,
        )?;
    }
    if builder.slash_bond(*signer, request.reward as u128)? != request.reward {
        return Err(PalwStateV2Error::BondBudgetRefused("stage28 original escrow collection changed after complete preflight".into()));
    }
    super::super::palw_flat_execution_fold_v11::write_shard(builder, &shard, id)?;
    super::super::palw_flat_execution_fold_v11::write_source_shard(builder, &source_shard, id)?;
    let table = crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11;
    builder.write_kernel_row(table, bytes(&(16u8, id, 1u8)), Some(bytes(&maximum.source().record())))?;
    builder.write_kernel_row(table, bytes(&(26u8, id, 0u8)), Some(bytes(maximum.caller().record())))?;
    builder.write_kernel_row(table, queue_key, Some(bytes(&(queue_global.root(), queue_global))))?;
    builder.write_kernel_row(table, bytes(&(30u8, id, 1u8)), Some(bytes(&(queue_lease.root(), queue_lease))))?;
    builder.write_kernel_row(table, bytes(&(30u8, id, 2u8)), Some(bytes(maximum.queue().record())))?;
    builder.write_kernel_row(table, bytes(&(29u8, id, 23u8)), Some(bytes(maximum.carriage().record())))?;
    builder.write_kernel_row(table, bytes(&(29u8, id, 3u8)), Some(bytes(&p.execution.quota)))?;
    builder.write_kernel_row(table, bytes(&(29u8, id, 1u8)), Some(bytes(maximum.paged().record())))?;
    builder.write_kernel_row(table, global_key, Some(bytes(&(global.root(), global))))?;
    let lease_journal = crate::palw_native_paged_proof_v29::NativePagedLeaseJournalV29 {
        version: 29,
        namespace: 28,
        job: id,
        funding: book.root(),
        maximum: m.root(),
        service: p.physical_service,
        lease_root: lease.root(),
        lease: lease.clone(),
        proof_envelope: maximum.carriage().record().proof_envelope.clone(),
    };
    builder.write_kernel_row(table, bytes(&(29u8, id, 22u8)), Some(bytes(&(lease_journal.root(), lease_journal))))?;
    let slots = crate::palw_flat_execution_capacity_v22::NativePhysicalSlotsV22::empty(id, consent.id);
    builder.write_kernel_row(table, bytes(&(22u8, id, 2u8)), Some(bytes(&(slots.root(), slots))))?;
    builder.write_kernel_row(
        crate::palw_flat_execution_v11::EXECUTION_REQUEST_INDEX_TABLE_V11,
        bytes(&a.requester_operator),
        Some(bytes(&next_nonce)),
    )?;
    let data = JobData { book: bytes(&book), inputs: request.input_images, trace: vec![] };
    let h = JobHeader {
        version: 28,
        id,
        class: class_id,
        consent: consent.id,
        original: book.root(),
        book: book.root(),
        contract: m.clone(),
        requester: *signer,
        producer: request.producer,
        requester_payout,
        producer_payout,
        book_bytes: data.book.len() as u32,
        input_bytes: u32::try_from(actual.input_raw_bytes()).map_err(|_| refused("stage28 input extent overflow"))?,
        trace_bytes: 0,
        positions: job.positions,
        quota_record: plan.plan().record(),
        source_requests_per_owner: plan.requests_per_owner(),
        restore_bytes: plan.restore_bytes(),
        terminal: book.terminal(),
        producer_reserved: book.reserve(),
    };
    requester_ids.push(id);
    producer_ids.push(id);
    builder.write_kernel_row(STAGE_TABLE_V14, key(5, &id), Some(bytes(&h)))?;
    builder.write_kernel_row(STAGE_TABLE_V14, key(6, &id), Some(bytes(&data)))?;
    builder.write_kernel_row(STAGE_TABLE_V14, key(11, &consent.id), Some(bytes(&(id, book.root(), receipt))))?;
    builder.write_kernel_row(STAGE_TABLE_V14, key(9, &request.producer), Some(bytes(&consent_ids)))?;
    builder.write_kernel_row(STAGE_TABLE_V14, key(12, signer), Some(bytes(&requester_ids)))?;
    builder.write_kernel_row(STAGE_TABLE_V14, key(13, &request.producer), Some(bytes(&producer_ids)))?;
    builder.note_kernel_bond(signer)?;
    builder.note_kernel_bond(&request.producer)?;
    Ok(())
}
fn cached_plan(h: &JobHeader, p: &NativeStagePoliciesV28) -> Result<CheckedStageQuotaV28, PalwStateV2Error> {
    let restore =
        if h.contract.final_basis.verifier_profile == misaka_palw_kernel::flat_inputs12::court32::ordinary_stage_court_id_v32() {
            CheckedStageQuotaV28::restore_native_record_v32
        } else {
            CheckedStageQuotaV28::restore_native_record
        };
    restore(
        h.class,
        h.contract.source,
        h.contract.admission,
        p.root(),
        h.source_requests_per_owner,
        h.restore_bytes,
        &h.quota_record,
        h.contract.quota,
    )
    .map_err(refused)
}
fn original_binding(
    builder: &TransitionBuilder<'_>,
    p: &NativeStagePoliciesV28,
    h: &JobHeader,
    plan: &CheckedStageQuotaV28,
) -> Result<misaka_palw_kernel::flat_quota11::SubjectBindingV11, PalwStateV2Error> {
    use misaka_palw_kernel::flat_quota11::SubjectBindingV11;
    let raw = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .aux
        .get(&(crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11, bytes(&(2u8, h.id))))
        .ok_or_else(|| refused("stage28 original shared subject absent"))?;
    let (root, s): (Digest, misaka_palw_kernel::flat_quota11::QuotaSubjectSummaryV11) =
        borsh::from_slice(raw).map_err(|_| refused("stage28 original subject grammar"))?;
    if s.row_root() != root {
        return Err(refused("stage28 original subject root differs"));
    }
    let b: SubjectBindingV11 = s.binding;
    if b.job_id != h.id
        || b.class_id != h.class
        || b.class_scope9_root != h.contract.maximum_scope
        || b.quota_plan_root != plan.plan().record().root()
        || b.policy_root != p.execution.quota.root()
        || b.producer != palw_kernel_bond_id_v1(&h.producer)
        || b.requester != palw_kernel_bond_id_v1(&h.requester)
        || b.resource10_root != h.contract.actual_resources
        || b.parameter_root != h.quota_record.parameter_root
        || b.actual_positions != h.positions
    {
        return Err(refused("stage28 original native shared binding differs"));
    }
    Ok(b)
}
fn write_job(
    builder: &mut TransitionBuilder<'_>,
    h: &mut JobHeader,
    data: &mut JobData,
    book: &StageJobBookV28,
) -> Result<(), PalwStateV2Error> {
    h.book = book.root();
    h.book_bytes = data.book.len() as u32;
    h.trace_bytes = data.trace.len() as u32;
    h.terminal = book.terminal();
    builder.write_kernel_row(STAGE_TABLE_V14, key(5, &h.id), Some(bytes(h)))?;
    builder.write_kernel_row(STAGE_TABLE_V14, key(6, &h.id), Some(bytes(data)))?;
    Ok(())
}
fn commit(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV28,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    let id: Digest = body.get(66..130).ok_or_else(|| refused("stage28 Commit original job fixed prefix absent"))?.try_into().unwrap();
    let mut h = original_header(builder, id)?;
    if master32::binding(builder.state.kernel_route.as_ref().unwrap(), id)?.is_some() {
        return Err(refused("invocation32 must use its original Commit36 contract"));
    }
    if h.terminal != StageTerminalV28::Funded
        || h.producer != *signer
        || builder.state.bonds.get(signer).is_none_or(|b| {
            b.operator_id.as_bytes()
                != read::<NativeStageConsentV28>(builder.state.kernel_route.as_ref().unwrap(), 10, &h.consent, 4096)
                    .map(|v| v.original14.producer_operator)
                    .unwrap_or([0; 64])
        })
    {
        return Err(refused("stage28 Commit original producer/finite funded state differs"));
    }
    let cached = cached_plan(&h, p)?;
    let binding = original_binding(builder, p, &h, &cached)?;
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let mut shard =
        super::super::palw_flat_execution_fold_v11::quota_shard(builder, &p.execution, &epoch, palw_kernel_bond_id_v1(signer), id)?;
    let action = cached.plan().action(misaka_palw_kernel::flat_quota11::QuotaActionKindV11::Metadata, body).map_err(refused)?;
    shard.debit_reserved(&p.execution.quota, &epoch, &binding, cached.plan(), &action).map_err(refused)?;
    super::super::palw_flat_execution_fold_v11::write_shard(builder, &shard, id)?;
    let (class, actual, plan, mut book, mut data) = restore_job(builder, p, &h)?;
    let maximum = contract(builder, p, &class, &actual, &plan)?;
    if maximum.record() != &h.contract || cached.root() != plan.root() {
        return Err(refused("stage28 Commit cache differs from original independent before-Fund contract"));
    }
    let trace = StageTraceV14::from_bytes(body, h.positions).map_err(refused)?;
    book.commit(&class, &actual, &plan, palw_kernel_bond_id_v1(signer), ctx.daa_score, &trace, h.producer_reserved)
        .map_err(refused)?;
    data.book = bytes(&book);
    data.trace = body.to_vec();
    write_job(builder, &mut h, &mut data, &book)?;
    let status = crate::palw_native_paged_proof_v29::NativePagedJobStatusV29 {
        version: 29,
        namespace: 28,
        job: id,
        funding: h.original,
        claim: book.claim().unwrap(),
        maximum: h.contract.root(),
        paid_plan: h.contract.paged_plan,
        parameter: class.source().parameters().root(),
        inventory: class.source().inventory_root(),
        active: true,
        admitted_interval: book.admitted_interval(),
        deadline: book.deadline(),
        challenge_end: book.challenge_end().unwrap(),
        paid_owners: [[0; 64]; 4],
        owners_admitted: 0,
        latest_paid_horizon: 0,
    };
    builder.write_kernel_row(
        crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11,
        bytes(&(29u8, id, 0u8)),
        Some(bytes(&(status.root(), status))),
    )?;
    Ok(())
}
fn settlement_queue(h: &JobHeader, producer: bool) -> crate::Hash64 {
    let mut root = object_id(
        b"misaka-palw/native/stage-original-terminal-payout/v28",
        &(h.id, h.original, h.contract.root(), producer, if producer { h.producer_payout } else { h.requester_payout }),
    );
    root[0] = crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_PAYOUT_KEY_PREFIX_V1;
    crate::Hash64::from_bytes(root)
}
fn terminal_grant(
    builder: &TransitionBuilder<'_>,
    p: &NativeStagePoliciesV28,
    h: &JobHeader,
    book: &StageJobBookV28,
    plan: &CheckedStageQuotaV28,
    s: &StageSettlementV28,
    queue: crate::Hash64,
) -> Result<misaka_palw_kernel::flat_quota11::CheckedQuotaTerminalV11, PalwStateV2Error> {
    let nonce =
        builder.state.kernel_route.as_ref().unwrap().execution_nonce_counter_v11(&book.requester_operator()).map_err(refused)?;
    check_stage_terminal_v28(
        book,
        plan,
        &p.execution.quota,
        &NativeStageTerminalAnchorV28 {
            job: h.id,
            original: h.original,
            terminal: book.root(),
            nonce: NativeRequesterNonceWitnessV11 {
                requester: book.job().requester,
                requester_operator_id: book.requester_operator(),
                counter_root: nonce.root(),
                next_nonce: nonce.next_nonce,
            },
            producer_credit: s.producer_credit,
            requester_credit: s.requester_credit,
            actual_slash: s.actual_slash,
            finder_credit: s.finder_credit,
            queue: queue.as_bytes(),
            proof: s.proof,
            established: s.established,
        },
    )
    .map_err(refused)
}
fn closed_subject(
    builder: &TransitionBuilder<'_>,
    p: &NativeStagePoliciesV28,
    h: &JobHeader,
    book: &StageJobBookV28,
    plan: &CheckedStageQuotaV28,
    terminal: &misaka_palw_kernel::flat_quota11::CheckedQuotaTerminalV11,
) -> Result<misaka_palw_kernel::flat_quota11::QuotaSubjectSummaryV11, PalwStateV2Error> {
    // Only this private native function can journal the public row's closure:
    // its caller has a kernel-private checked genuine settlement token. Quota
    // consumption and all original promises remain held until paid retirement.
    let _ = terminal;
    let raw = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .aux
        .get(&(crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11, bytes(&(2u8, h.id))))
        .ok_or_else(|| refused("stage28 original quota subject absent"))?;
    let (root, mut row): (Digest, misaka_palw_kernel::flat_quota11::QuotaSubjectSummaryV11) =
        borsh::from_slice(raw).map_err(|_| refused("stage28 exact original subject grammar"))?;
    if row.row_root() != root
        || row.binding != book.binding(plan, &p.execution.quota).map_err(refused)?
        || row.closed
        || !matches!(book.terminal(), StageTerminalV28::Final | StageTerminalV28::Refunded | StageTerminalV28::Convicted)
    {
        return Err(refused("stage28 closure lacks exact genuine original terminal subject"));
    }
    row.closed = true;
    Ok(row)
}
fn deactivate_paid_status(builder: &mut TransitionBuilder<'_>, id: Digest) -> Result<(), PalwStateV2Error> {
    let table = crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11;
    let k = bytes(&(29u8, id, 0u8));
    let Some(raw) = builder.state.kernel_route.as_ref().unwrap().aux.get(&(table, k.clone())) else {
        return Ok(());
    };
    let (root, mut status): (Digest, crate::palw_native_paged_proof_v29::NativePagedJobStatusV29) =
        borsh::from_slice(raw).map_err(|_| refused("stage28 original paid status grammar"))?;
    if status.root() != root || status.namespace != 28 || status.job != id {
        return Err(refused("stage28 original paid status binding differs"));
    }
    status.active = false;
    builder.write_kernel_row(table, k, Some(bytes(&(status.root(), status))))?;
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn settle_conviction(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV28,
    class: &CheckedStageClassV28,
    plan: &CheckedStageQuotaV28,
    h: &mut JobHeader,
    data: &mut JobData,
    book: &StageJobBookV28,
    s: &StageSettlementV28,
    refutes_source: bool,
) -> Result<(), PalwStateV2Error> {
    let queue = settlement_queue(h, false);
    let r = builder.state.kernel_route.as_ref().unwrap();
    if builder.state.pending_payouts.contains_key(&queue) || r.aux.contains_key(&(STAGE_TABLE_V14, key(14, &h.id))) {
        return Err(refused("stage28 original terminal credit already exists"));
    }
    let terminal = terminal_grant(builder, p, h, book, plan, s, queue)?;
    let subject = closed_subject(builder, p, h, book, plan, &terminal)?;
    let payout = builder.state.bonds.get(signer).ok_or_else(|| refused("stage28 original finder absent"))?.payout_payload;
    if !builder.params.reporter_income_active_at(ctx.daa_score)
        && builder
            .state
            .pending_payouts
            .get(&crate::palw_kernel_route_v1::palw_kernel_payout_key_v1(&payout))
            .is_some_and(|v| v.amount.checked_add(s.finder_credit).is_none())
    {
        return Err(refused("stage28 actual finder payout overflow"));
    }
    if builder.state.bonds.get(&h.producer).is_none_or(|b| b.collateral < s.actual_slash) {
        return Err(refused("stage28 original producer collection preflight changed"));
    }
    // Fixed paid status validation precedes money, and every later financial
    // error aborts the complete native delta rather than a caught refusal.
    deactivate_paid_status(builder, h.id)?;
    if builder.slash_bond(h.producer, s.actual_slash as u128)? != s.actual_slash {
        return Err(PalwStateV2Error::BondBudgetRefused("stage28 actual collateral collection changed after preflight".into()));
    }
    builder.write_payout(queue, Some(PalwPayoutV2 { payload: h.requester_payout, amount: s.requester_credit }))?;
    if s.finder_credit != 0 {
        if builder.params.reporter_income_active_at(ctx.daa_score) {
            let award = crate::Hash64::from_bytes(object_id(
                b"misaka-palw/native/stage-original-court-award/v28",
                &(h.id, book.root(), palw_kernel_bond_id_v1(signer)),
            ));
            builder.stage_reporter_recipient_v1(award, *signer, ctx.daa_score, s.finder_credit, crate::Hash64::from_bytes(h.id))?;
        } else {
            builder.add_kernel_payout(payout, s.finder_credit)?;
        }
    }
    builder.cancel_native_recipient_v1(crate::Hash64::from_bytes(h.id), h.producer, 14)?;
    builder.bond_budget_close_v1(
        &crate::Hash64::from_bytes(object_id(b"misaka-palw/native/stage-original-fixed-budget/v28", &h.id)),
        ctx.daa_score,
    )?;
    data.book = bytes(book);
    h.producer_reserved = 0;
    write_job(builder, h, data, book)?;
    builder.write_kernel_row(STAGE_TABLE_V14, key(14, &h.id), Some(bytes(&(s, queue, h.requester_payout))))?;
    builder.write_kernel_row(
        crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11,
        bytes(&(2u8, h.id)),
        Some(bytes(&(subject.row_root(), subject))),
    )?;
    if refutes_source {
        let source = class.source();
        let fact = crate::palw_flat_stage_v14::NativeStageNegativeSourceFactV14 {
            version: 14,
            relation: source.relation_root(),
            source: source.id(),
            source_program: source.program_root(),
            parameters: source.parameters().root(),
            inventory: source.inventory_root(),
            proof: s.proof,
            established_daa: ctx.daa_score,
            reporter: *signer,
        };
        builder.write_kernel_row(
            STAGE_TABLE_V14,
            crate::palw_flat_stage_v14::stage_key_v14(14, &fact.relation),
            Some(bytes(&fact)),
        )?;
    }
    master32::note_abort(builder, h, book)?;
    Ok(())
}
fn paid_status(
    builder: &TransitionBuilder<'_>,
    id: Digest,
) -> Result<crate::palw_native_paged_proof_v29::NativePagedJobStatusV29, PalwStateV2Error> {
    let raw = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .aux
        .get(&(crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11, bytes(&(29u8, id, 0u8))))
        .ok_or_else(|| refused("stage28 original paid status absent"))?;
    if raw.len() > 2048 {
        return Err(refused("stage28 original paid status cap"));
    }
    let (root, status): (Digest, crate::palw_native_paged_proof_v29::NativePagedJobStatusV29) =
        borsh::from_slice(raw).map_err(|_| refused("stage28 paid status grammar"))?;
    if status.root() != root || status.namespace != 28 || status.job != id {
        return Err(refused("stage28 original paid status root differs"));
    }
    Ok(status)
}
fn write_paged(
    builder: &mut TransitionBuilder<'_>,
    p: &crate::palw_native_paged_quota_v29::NativePagedQuotaProjectionV29,
) -> Result<(), PalwStateV2Error> {
    let table = crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11;
    let g = p.global();
    let a = p.actor();
    let mut actor = vec![1];
    actor.extend_from_slice(&a.actor);
    builder.write_kernel_row(table, vec![0], Some(bytes(&(g.root(), g))))?;
    builder.write_kernel_row(table, actor, Some(bytes(&(a.root(), a))))?;
    builder.write_kernel_row(table, bytes(&(29u8, p.status().job, 0u8)), Some(bytes(&(p.status().root(), p.status()))))?;
    if let Some(s) = p.seal() {
        builder.write_kernel_row(table, bytes(&(29u8, s.job, s.id())), Some(bytes(&(s.root(), s))))?;
    }
    Ok(())
}
fn write_caller(
    builder: &mut TransitionBuilder<'_>,
    s: &misaka_palw_kernel::flat_quota11::stage14::source26::StageSourceCallerShardV26,
) -> Result<(), PalwStateV2Error> {
    let table = crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11;
    let g = s.global_row();
    let a = s.actor_row();
    let mut k = vec![1];
    k.extend_from_slice(&a.actor);
    builder.write_kernel_row(table, vec![0], Some(bytes(&(g.root(), g))))?;
    builder.write_kernel_row(table, k, Some(bytes(&(a.root(), a))))?;
    if let Some(row) = s.caller_row() {
        builder.write_kernel_row(table, bytes(&(26u8, row.job, row.owner)), Some(bytes(&(row.root(), row))))?;
    }
    Ok(())
}
fn owner(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV28,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    use crate::{
        palw_native_paged_proof_v29::NativePagedSealV29,
        palw_native_paged_quota_v29::{CheckedNativePagedAdmissionV29, NativePagedQuotaProjectionV29},
    };
    use misaka_palw_kernel::flat_quota11::stage14::source26::*;
    if body.len() != 201 || body[..2] != 28u16.to_le_bytes() {
        return Err(refused("stage28 paid owner exact metadata grammar"));
    }
    let id: Digest = body[2..66].try_into().unwrap();
    let h = original_header(builder, id)?;
    let (s, c) = original_extents(builder, h.class)?;
    if !pay_restore(builder, ctx, signer, p, s, c, h.input_bytes, body.len() as u32)? {
        return Ok(());
    }
    let (class, actual, plan, book, _data) = restore_job(builder, p, &h)?;
    let maximum = contract(builder, p, &class, &actual, &plan)?;
    if h.terminal != StageTerminalV28::Challengeable
        || maximum.record() != &h.contract
        || book.challenge_end().is_none_or(|end| ctx.daa_score > end)
    {
        return Err(refused("stage28 paid owner outside original whole-claim admission interval"));
    }
    let mut status = paid_status(builder, id)?;
    let bond = builder.state.bonds.get(signer).ok_or_else(|| refused("stage28 owner absent"))?;
    let actor = palw_kernel_bond_id_v1(signer);
    let operator = bond.operator_id.as_bytes();
    let producer_operator = book.producer_operator();
    if !matches!(bond.status, PalwBondStatusV2::Active) || actor == book.job().producer || operator == producer_operator {
        return Err(refused("stage28 paid owner is not independent from original producer"));
    }
    let discovery = body[200];
    if discovery > 1 {
        return Err(refused("stage28 paid owner mode exceeded"));
    }
    let mut proof: Digest = body[66..130].try_into().unwrap();
    let mut seal_root: Digest = body[130..194].try_into().unwrap();
    let total = u32::from_le_bytes(body[194..198].try_into().unwrap());
    let count = u16::from_le_bytes(body[198..200].try_into().unwrap());
    if discovery == 0 && (proof == [0; 64] || seal_root == [0; 64])
        || discovery == 1
            && (proof != [0; 64]
                || seal_root != [0; 64]
                || total != maximum.paged().record().maximum_bytes
                || count != maximum.paged().record().maximum_parts)
    {
        return Err(refused("stage28 original proof/discovery explicit metadata differs"));
    }
    let ticket = if discovery == 1 {
        object_id(
            b"misaka-palw/native/stage-original-discovery-ticket/v28",
            &(id, status.claim, actor, operator, ctx.block.as_bytes(), h.contract.root()),
        )
    } else {
        [0; 64]
    };
    if discovery == 1 {
        proof = object_id(b"misaka-palw/native/stage-unproved-discovery-marker/v28", &ticket);
        seal_root = object_id(b"misaka-palw/native/stage-unsealed-discovery-marker/v28", &ticket);
    }
    let matures =
        ctx.daa_score.checked_add(h.contract.temporal.seal_maturity).ok_or_else(|| refused("stage28 owner maturity overflow"))?;
    let reveal_until =
        ctx.daa_score.checked_add(h.contract.temporal.proof_window).ok_or_else(|| refused("stage28 owner reveal overflow"))?;
    let horizon = reveal_until
        .checked_add(p.execution.execution.horizons.da_answer)
        .and_then(|x| x.checked_add(p.execution.execution.horizons.proof_grace))
        .ok_or_else(|| refused("stage28 owner horizon overflow"))?;
    if horizon >= book.deadline() || ctx.daa_score.checked_add(maximum.caller().required_horizon()).is_none_or(|at| at > reveal_until)
    {
        return Err(refused("stage28 original paid owner adaptive source/proof/delivery horizon does not fit before-Fund deadline"));
    }
    let ledger = super::super::palw_kernel_route_fold_v1::route_ledger_policy_v1(builder)?;
    let fee = ledger.job_fee.max(1);
    let free = (bond.collateral as u128).saturating_sub(builder.committed_at(signer, ctx.daa_score));
    if free < fee as u128 {
        return Err(refused("stage28 actual paid owner fee unavailable"));
    }
    let receipt = object_id(
        b"misaka-palw/native/stage-original-paid-owner-debit/v28",
        &(id, actor, operator, ctx.block.as_bytes(), fee, h.contract.root(), proof, seal_root),
    );
    let paid = maximum.paged().record();
    let own = NativePagedSealV29 {
        version: 29,
        namespace: 28,
        job: id,
        claim: status.claim,
        original_funding: h.original,
        maximum_contract: h.contract.root(),
        original_plan: h.contract.paged_plan,
        original_service: p.physical_service,
        original_lease: {
            let raw = builder
                .state
                .kernel_route
                .as_ref()
                .unwrap()
                .aux
                .get(&(crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11, bytes(&(29u8, id, 22u8))))
                .ok_or_else(|| refused("stage28 original physical lease journal absent"))?;
            let (root, l): (Digest, crate::palw_native_paged_proof_v29::NativePagedLeaseJournalV29) =
                borsh::from_slice(raw).map_err(|_| refused("stage28 original lease grammar"))?;
            if l.root() != root || l.job != id || l.namespace != 28 || l.funding != h.original {
                return Err(refused("stage28 original lease root differs"));
            }
            l.lease_root
        },
        maximum_scope: h.contract.maximum_scope,
        real_parameter: class.source().parameters().root(),
        inventory: class.source().inventory_root(),
        bond: *signer,
        actor,
        operator,
        producer_operator,
        proof_root: proof,
        seal_root,
        original_paid_receipt: receipt,
        count,
        total_bytes: total,
        posted: ctx.daa_score,
        matures,
        reveal_until,
        horizon,
        admitted_interval: book.admitted_interval(),
        admitted_block: ctx.block.as_bytes(),
        part_attempts: [0; 512],
        accepted: [0; 64],
        assembly_attempts: 0,
        court_attempts: 0,
        discovery: discovery == 1,
        original_ticket: ticket,
        conversion_attempts: 0,
        retired: false,
        spent: false,
        committed: paid.admission.checked_add(&paid.guarantee).map_err(refused)?,
        consumed: paid.admission.clone(),
        remaining: paid.guarantee.clone(),
    };
    // G14 item 4 (lane r5-f03-ingress): the lifetime catalogue (cap 4) is checked BEFORE the admission token, as text29's. Past
    // `palw_court_outsider_lane_v1` a full catalogue may reclaim the entry of an owner that expired WITHOUT progress (the court-owner
    // round-2 text29 rule, G-N1); below the fence the closure is never consulted and the refusal is byte for byte the old one.
    let mut owner_ids: Vec<Digest> = read(builder.state.kernel_route.as_ref().unwrap(), 19, &id, 4 + 4 * 64).unwrap_or_default();
    let evicted = {
        let ro: &TransitionBuilder<'_> = &*builder;
        let reclaim_active = ro.params.court_outsider_lane_active_v1(ctx.daa_score);
        stage28_owner_catalogue_admit_v28(&owner_ids, own.id(), reclaim_active, |member| {
            stage28_owner_reclaimable_v28(ro, &h, id, *member, ctx.daa_score)
        })
        .map_err(refused)?
    };
    if let Some(gone) = evicted {
        owner_ids.retain(|v| *v != gone);
        // The evicted owner holds no paid slot (`expire_owner` ran), so the LIFETIME count the token refuses at 4 becomes the
        // live-plus-progressed count (v39's `admitted`).
        status.owners_admitted =
            status.owners_admitted.checked_sub(1).ok_or_else(|| refused("stage28 reclaimed owner count absent"))?;
    }
    owner_ids.push(own.id());
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let shard = super::super::palw_flat_execution_fold_v11::quota_shard(builder, &p.execution, &epoch, actor, id)?;
    let token =
        CheckedNativePagedAdmissionV29::from_current_native(status.clone(), own.clone(), maximum.paged(), &p.execution.quota, &epoch)
            .map_err(refused)?;
    let mut projection = NativePagedQuotaProjectionV29::from_native_rows(
        &p.execution.quota,
        maximum.paged(),
        &shard.global_row(),
        shard.global_row().root(),
        &shard.actor_row(),
        shard.actor_row().root(),
        &status,
        status.root(),
        None,
    )
    .map_err(refused)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let legacy_actor = (r.flat_model_seals_of_v6(signer).len()
        + r.flat_model_seals_of_v7(signer).len()
        + r.flat_model_seals_of_v9(signer).len()) as u32;
    projection
        .admit(
            &p.execution.quota,
            &epoch,
            maximum.paged(),
            &token,
            r.flat_model_seal_count_v6() + r.flat_model_seal_count_v7() + r.flat_model_seal_count_v9(),
            legacy_actor,
        )
        .map_err(refused)?;
    let source_quote = source28::source_quote(builder, p, &h)?;
    let producer_source = source28::producer_shard(builder, p, &epoch, &h, &source_quote)?;
    let producer_row = producer_source.source_row().unwrap();
    if producer_row.retired
        || producer_row.requests_cap.checked_sub(producer_row.requests_admitted).is_none_or(|n| n < plan.requests_per_owner())
    {
        return Err(refused("stage28 original producer lifetime answer pool cannot promise this additional owner"));
    }
    let native = NativeStageSourceOwnerV28 {
        job: id,
        original_book: h.original,
        consent: h.consent,
        claim: status.claim,
        seal: own.id(),
        actor,
        actor_operator: operator,
        producer_operator,
        original_paid: receipt,
        original_plan: h.contract.caller_source,
        horizon,
        paid_at: ctx.daa_score,
        actual_debit: fee,
        actual_receipt: receipt,
    };
    let source_grant = check_stage_source_owner_v28(
        &class,
        &actual,
        &plan,
        &book,
        &p.execution.quota,
        &p.execution.execution,
        &p.execution.discovery,
        &epoch,
        maximum.caller(),
        &native,
    )
    .map_err(refused)?;
    let subject = shard.subject_row().ok_or_else(|| refused("stage28 original owner subject absent"))?;
    let mut caller = StageSourceCallerShardV26::from_native_rows(
        &p.execution.quota,
        maximum.caller(),
        projection.global(),
        &projection.global().root(),
        actor,
        Some((projection.actor(), &projection.actor().root())),
        (subject, &subject.row_root()),
        None,
    )
    .map_err(refused)?;
    caller.admit(&p.execution.quota, &epoch, maximum.caller(), &source_grant).map_err(refused)?;
    let slotkey = bytes(&(22u8, id, 2u8));
    let raw = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .aux
        .get(&(crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11, slotkey.clone()))
        .ok_or_else(|| refused("stage28 original physical slots absent"))?;
    let (root, mut slots): (Digest, crate::palw_flat_execution_capacity_v22::NativePhysicalSlotsV22) =
        borsh::from_slice(raw).map_err(|_| refused("stage28 original physical streams grammar"))?;
    if slots.root() != root || slots.job != id || slots.original_consent23 != h.consent {
        return Err(refused("stage28 original physical stream lineage differs"));
    }
    slots.reserve_owner(own.id(), horizon).map_err(refused)?;
    if builder.slash_bond(*signer, fee as u128)? != fee {
        return Err(PalwStateV2Error::BondBudgetRefused("stage28 actual own paid fee changed after complete preflight".into()));
    }
    builder.write_kernel_row(STAGE_TABLE_V14, key(19, &id), Some(bytes(&owner_ids)))?;
    write_paged(builder, &projection)?;
    write_caller(builder, &caller)?;
    builder.write_kernel_row(
        crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11,
        slotkey,
        Some(bytes(&(slots.root(), slots))),
    )?;
    if let Some(gone) = evicted {
        // The evicted owner leaves the lifetime catalogue, so collection no longer iterates it: retire now exactly the two archival
        // rows collection checks per catalogued owner — nothing is orphaned.
        builder.write_kernel_row(crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11, bytes(&(29u8, id, gone)), None)?;
        builder.write_kernel_row(crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11, bytes(&(26u8, id, gone)), None)?;
    }
    builder.write_kernel_row(STAGE_TABLE_V14, key(15, &own.id()), Some(bytes(&(id, receipt, maximum.caller().root(), own.id()))))?;
    builder.note_kernel_bond(signer)?;
    Ok(())
}

/// **G14 item 4 (lane r5-f03-ingress): the stage28 lifetime owner catalogue's admission (cap 4)** — the mirror of text29's
/// `text29_owner_catalogue_admit_v29` (court-owner round 2). `Ok(None)`: the newcomer takes a free entry. `Ok(Some(member))`: the
/// catalogue is full, the reclamation is in force, and `member` — the FIRST catalogued owner (catalogue order, a rooted fact) that
/// `reclaimable` accepts — gives up its entry. Otherwise the old refusal, with its old text. Below the fence `reclaimable` is never
/// called; an owner that made progress is never reclaimable.
pub(crate) fn stage28_owner_catalogue_admit_v28(
    catalogue: &[Digest],
    newcomer: Digest,
    reclaim_active: bool,
    reclaimable: impl Fn(&Digest) -> bool,
) -> Result<Option<Digest>, &'static str> {
    const EXHAUSTED: &str = "stage28 original four-owner lifetime catalogue exhausted";
    if catalogue.contains(&newcomer) {
        return Err(EXHAUSTED);
    }
    if catalogue.len() < 4 {
        return Ok(None);
    }
    if !reclaim_active || catalogue.len() > 4 {
        return Err(EXHAUSTED);
    }
    catalogue.iter().find(|member| reclaimable(member)).copied().map(Some).ok_or(EXHAUSTED)
}

/// **A stage28 owner that made no progress before its deadline** (pure on its archival seal): namespace 28, expired and retired
/// (`expire_owner` ran, past its horizon), no part attempt, no accepted part, no assembly, no court move, no discovery conversion.
pub(crate) fn stage28_owner_made_no_progress_v28(seal: &crate::palw_native_paged_proof_v29::NativePagedSealV29, now: u64) -> bool {
    seal.namespace == 28
        && seal.retired
        && now > seal.horizon
        && seal.part_attempts.iter().all(|n| *n == 0)
        && seal.accepted == [0; 64]
        && seal.assembly_attempts == 0
        && seal.court_attempts == 0
        && seal.conversion_attempts == 0
}

/// [`stage28_owner_made_no_progress_v28`] on the chain rows, plus every liability collection would demand of it already retired: its
/// caller row (26) retired with nothing remaining and bound to its paid receipt, and no pending owned source request.
fn stage28_owner_reclaimable_v28(builder: &TransitionBuilder<'_>, h: &JobHeader, job: Digest, member: Digest, now: u64) -> bool {
    use misaka_palw_kernel::flat_quota11::stage14::source26::StageSourceCallerRowV26;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let Some(seal) = r.native_paged_seal_original_v29(job, member) else { return false };
    if !stage28_owner_made_no_progress_v28(&seal, now) {
        return false;
    }
    let Some(raw) = r.aux.get(&(crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11, bytes(&(26u8, job, member)))) else {
        return false;
    };
    let Ok((root, caller)) = borsh::from_slice::<(Digest, StageSourceCallerRowV26)>(raw) else { return false };
    caller.root() == root
        && caller.retired
        && caller.owner == member
        && caller.original_paid == seal.original_paid_receipt
        && caller.remaining == Default::default()
        && source28::pending_owned(builder, h, member).is_ok_and(|n| n == 0)
}

fn paged_projection(
    builder: &TransitionBuilder<'_>,
    p: &NativeStagePoliciesV28,
    epoch: &TrustedEpochV11,
    h: &JobHeader,
    maximum: &CheckedNativeStageMaximumV28,
    status: &crate::palw_native_paged_proof_v29::NativePagedJobStatusV29,
    own: &crate::palw_native_paged_proof_v29::NativePagedSealV29,
) -> Result<crate::palw_native_paged_quota_v29::NativePagedQuotaProjectionV29, PalwStateV2Error> {
    let q = super::super::palw_flat_execution_fold_v11::quota_shard(builder, &p.execution, epoch, own.actor, h.id)?;
    crate::palw_native_paged_quota_v29::NativePagedQuotaProjectionV29::from_native_rows(
        &p.execution.quota,
        maximum.paged(),
        &q.global_row(),
        q.global_row().root(),
        &q.actor_row(),
        q.actor_row().root(),
        status,
        status.root(),
        Some((own, own.root())),
    )
    .map_err(refused)
}
fn discovery_convert(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV28,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    if body.len() != 264 || body[..2] != 28u16.to_le_bytes() {
        return Err(refused("stage28 discovery conversion exact metadata"));
    }
    let id: Digest = body[2..66].try_into().unwrap();
    let old: Digest = body[66..130].try_into().unwrap();
    let proof = body[130..194].try_into().unwrap();
    let seal_root = body[194..258].try_into().unwrap();
    let total = u32::from_le_bytes(body[258..262].try_into().unwrap());
    let count = u16::from_le_bytes(body[262..264].try_into().unwrap());
    let h = original_header(builder, id)?;
    let (s, c) = original_extents(builder, h.class)?;
    if !pay_restore(builder, ctx, signer, p, s, c, h.input_bytes, 264)? {
        return Ok(());
    }
    let (class, actual, plan, book, _data) = restore_job(builder, p, &h)?;
    let maximum = contract(builder, p, &class, &actual, &plan)?;
    let own = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .native_paged_seal_v29(id, old)
        .ok_or_else(|| refused("stage28 original discovery owner absent"))?;
    if h.terminal != StageTerminalV28::Challengeable
        || maximum.record() != &h.contract
        || own.namespace != 28
        || own.bond != *signer
        || own.claim != book.claim().unwrap()
        || own.original_funding != h.original
        || own.maximum_contract != h.contract.root()
        || builder.state.bonds.get(signer).is_none_or(|b| b.operator_id.as_bytes() != own.operator)
        || source28::pending_owned(builder, &h, old)? != 0
    {
        return Err(refused("stage28 discovery conversion lacks exact original independent settled own ticket"));
    }
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let status = paid_status(builder, id)?;
    let mut projection = paged_projection(builder, p, &epoch, &h, &maximum, &status, &own)?;
    let matures =
        ctx.daa_score.checked_add(h.contract.temporal.seal_maturity).ok_or_else(|| refused("stage28 converted maturity overflow"))?;
    projection
        .convert_discovery(
            &p.execution.quota,
            &epoch,
            maximum.paged(),
            own.actor,
            own.operator,
            proof,
            seal_root,
            count,
            total,
            matures,
        )
        .map_err(refused)?;
    let new = projection.seal().unwrap().id();
    let quote = source28::caller_quote(builder, p, &h)?;
    let base = source28::caller_shard(builder, p, &epoch, &h, own.actor, old, &quote)?;
    let subject = super::super::palw_flat_execution_fold_v11::quota_shard(builder, &p.execution, &epoch, own.actor, id)?
        .subject_row()
        .unwrap()
        .clone();
    let row = base.caller_row().unwrap();
    let mut caller = misaka_palw_kernel::flat_quota11::stage14::source26::StageSourceCallerShardV26::from_native_rows(
        &p.execution.quota,
        &quote,
        projection.global(),
        &projection.global().root(),
        own.actor,
        Some((projection.actor(), &projection.actor().root())),
        (&subject, &subject.row_root()),
        Some((row, &row.root())),
    )
    .map_err(refused)?;
    caller.transfer_owner(&p.execution.quota, &epoch, &quote, old, new, own.original_paid_receipt).map_err(refused)?;
    let mut slots = source28::slots(builder, &h)?;
    slots.transfer_owner(old, new, own.horizon).map_err(refused)?;
    let mut ids: Vec<Digest> = read(builder.state.kernel_route.as_ref().unwrap(), 19, &id, 4 + 4 * 64)
        .ok_or_else(|| refused("stage28 original discovery lifetime catalogue absent"))?;
    let slot = ids.iter_mut().find(|v| **v == old).ok_or_else(|| refused("stage28 original discovery lifetime membership absent"))?;
    *slot = new;
    // Permanent conversion anchor preserves the spent discovery identity; only
    // its active catalogue/key moves. No new source allowance or TTL is minted.
    builder.write_kernel_row(STAGE_TABLE_V14, key(19, &id), Some(bytes(&ids)))?;
    write_paged(builder, &projection)?;
    write_caller(builder, &caller)?;
    let table = crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11;
    builder.write_kernel_row(table, bytes(&(29u8, id, old)), None)?;
    builder.write_kernel_row(table, bytes(&(26u8, id, old)), None)?;
    builder.write_kernel_row(table, bytes(&(22u8, id, 2u8)), Some(bytes(&(slots.root(), slots))))?;
    builder.write_kernel_row(STAGE_TABLE_V14, key(15, &old), Some(bytes(&(id, own.original_paid_receipt, quote.root(), new))))?;
    builder.write_kernel_row(STAGE_TABLE_V14, key(15, &new), Some(bytes(&(id, own.original_paid_receipt, quote.root(), new))))?;
    Ok(())
}
fn expire_owner(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV28,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    if body.len() != 128 {
        return Err(refused("stage28 expiry exact original job/owner"));
    }
    let id = body[..64].try_into().unwrap();
    let key_owner = body[64..].try_into().unwrap();
    let h = original_header(builder, id)?;
    let (s, c) = original_extents(builder, h.class)?;
    if !pay_restore(builder, ctx, signer, p, s, c, h.input_bytes, 128)? {
        return Ok(());
    }
    let (class, actual, plan, book, _data) = restore_job(builder, p, &h)?;
    let maximum = contract(builder, p, &class, &actual, &plan)?;
    let status = paid_status(builder, id)?;
    let own = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .native_paged_seal_original_v29(id, key_owner)
        .ok_or_else(|| refused("stage28 original expiry owner absent"))?;
    if maximum.record() != &h.contract
        || own.namespace != 28
        || own.original_funding != h.original
        || own.claim != book.claim().ok_or_else(|| refused("stage28 original committed claim absent"))?
        || own.maximum_contract != h.contract.root()
        || !matches!(book.terminal(), StageTerminalV28::Challengeable | StageTerminalV28::Final | StageTerminalV28::Convicted)
    {
        return Err(refused("stage28 expiry lacks genuine original live/terminal book"));
    }
    let pending = source28::pending_owned(builder, &h, key_owner)?;
    let token = crate::palw_native_paged_quota_v29::CheckedNativePagedExpiryV29::from_current_native(
        &status,
        &own,
        book.root(),
        ctx.daa_score,
        pending,
    )
    .map_err(refused)?;
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let mut projection = paged_projection(builder, p, &epoch, &h, &maximum, &status, &own)?;
    projection.expire_owner(&p.execution.quota, &epoch, maximum.paged(), &token).map_err(refused)?;
    let quote = source28::caller_quote(builder, p, &h)?;
    let base = source28::caller_shard(builder, p, &epoch, &h, own.actor, key_owner, &quote)?;
    let subject = super::super::palw_flat_execution_fold_v11::quota_shard(builder, &p.execution, &epoch, own.actor, id)?
        .subject_row()
        .unwrap()
        .clone();
    let row = base.caller_row().unwrap();
    let mut caller = misaka_palw_kernel::flat_quota11::stage14::source26::StageSourceCallerShardV26::from_native_rows(
        &p.execution.quota,
        &quote,
        projection.global(),
        &projection.global().root(),
        own.actor,
        Some((projection.actor(), &projection.actor().root())),
        (&subject, &subject.row_root()),
        Some((row, &row.root())),
    )
    .map_err(refused)?;
    caller.expire_owner(&p.execution.quota, &epoch, &quote, key_owner, own.original_paid_receipt).map_err(refused)?;
    let mut slots = source28::slots(builder, &h)?;
    slots.retire_owner(key_owner, ctx.daa_score).map_err(refused)?;
    write_paged(builder, &projection)?;
    write_caller(builder, &caller)?;
    let table = crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11;
    builder.write_kernel_row(table, bytes(&(22u8, id, 2u8)), Some(bytes(&(slots.root(), slots))))?;
    for part in 0..own.count {
        builder.write_kernel_row(
            crate::palw_flat_execution_v11::EXECUTION_SEAL_TABLE_V11,
            bytes(&(29u8, id, key_owner, part)),
            None,
        )?;
    }
    Ok(())
}

fn settle_plain(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    p: &NativeStagePoliciesV28,
    h: &mut JobHeader,
    data: &mut JobData,
    book: &StageJobBookV28,
    plan: &CheckedStageQuotaV28,
    s: &StageSettlementV28,
) -> Result<(), PalwStateV2Error> {
    let producer = s.terminal == StageTerminalV28::Final;
    let queue = settlement_queue(h, producer);
    if builder.state.pending_payouts.contains_key(&queue)
        || builder.state.kernel_route.as_ref().unwrap().aux.contains_key(&(STAGE_TABLE_V14, key(14, &h.id)))
    {
        return Err(refused("stage28 actual original terminal income already paid"));
    }
    let terminal = terminal_grant(builder, p, h, book, plan, s, queue)?;
    let subject = closed_subject(builder, p, h, book, plan, &terminal)?;
    deactivate_paid_status(builder, h.id)?;
    if producer {
        builder.pay_native_recipient_v1(
            crate::Hash64::from_bytes(h.id),
            h.producer,
            14,
            ctx.daa_score,
            s.producer_credit,
            queue,
            false,
        )?;
    } else {
        builder.cancel_native_recipient_v1(crate::Hash64::from_bytes(h.id), h.producer, 14)?;
    }
    builder.write_payout(
        queue,
        Some(PalwPayoutV2 {
            payload: if producer { h.producer_payout } else { h.requester_payout },
            amount: if producer { s.producer_credit } else { s.requester_credit },
        }),
    )?;
    builder.bond_budget_close_v1(
        &crate::Hash64::from_bytes(object_id(b"misaka-palw/native/stage-original-fixed-budget/v28", &h.id)),
        ctx.daa_score,
    )?;
    data.book = bytes(book);
    h.producer_reserved = 0;
    write_job(builder, h, data, book)?;
    builder.write_kernel_row(
        STAGE_TABLE_V14,
        key(14, &h.id),
        Some(bytes(&(s, queue, if producer { h.producer_payout } else { h.requester_payout }))),
    )?;
    builder.write_kernel_row(
        crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11,
        bytes(&(2u8, h.id)),
        Some(bytes(&(subject.row_root(), subject))),
    )?;
    master32::note_abort(builder, h, book)?;
    Ok(())
}
fn finalize(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV28,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    let id = body.try_into().map_err(|_| refused("stage28 terminal exact original job"))?;
    let mut h = original_header(builder, id)?;
    let (s, c) = original_extents(builder, h.class)?;
    if !pay_restore(builder, ctx, signer, p, s, c, h.input_bytes, 64)? {
        return Ok(());
    }
    let (class, actual, plan, mut book, mut data) = restore_job(builder, p, &h)?;
    let maximum = contract(builder, p, &class, &actual, &plan)?;
    if maximum.record() != &h.contract
        || builder.state.bonds.get(&h.producer).is_none_or(|b| b.operator_id.as_bytes() != book.producer_operator())
    {
        return Err(refused("stage28 terminal original before-Fund conditions or producer changed"));
    }
    if book.terminal() == StageTerminalV28::Funded {
        let outcome = book.refund_unclaimed(ctx.daa_score).map_err(refused)?;
        return settle_plain(builder, ctx, p, &mut h, &mut data, &book, &plan, &outcome);
    }
    if book.terminal() != StageTerminalV28::Challengeable || book.final_after().is_none_or(|at| ctx.daa_score <= at) {
        return Err(refused("stage28 original complete pursuit/proof/source terminal horizon remains open"));
    }
    if builder.state.kernel_route.as_ref().unwrap().stage_source_refuted_relation_v14(&class.source().relation_root()) {
        return Err(refused("stage28 Final cannot reuse an independently refuted original real R/PC relation"));
    }
    let status = paid_status(builder, id)?;
    if !status.active
        || status.funding != h.original
        || status.claim != book.claim().unwrap()
        || status.maximum != h.contract.root()
        || status.paid_owners.iter().any(|id| *id != [0; 64])
        || ctx.daa_score <= status.latest_paid_horizon
    {
        return Err(refused("stage28 original own independent protected owners still retained"));
    }
    source28::quiescent(builder, &h, ctx.daa_score)?;
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let quote = source28::source_quote(builder, p, &h)?;
    source28::producer_shard(builder, p, &epoch, &h, &quote)?;
    // A component stage's Final proves only that component. Complete-pipeline
    // composition consumes every genuine stage terminal under Master32.
    master32::check_final_binding(builder, p, &h, &book, &class, &data)?;
    let conditions = maximum.conditions();
    if conditions.policy_root() != book.final_policy_root() || conditions.basis_root() != book.final_basis_root() {
        return Err(refused("stage28 trusted original numerical/detection/source/delivery conditions differ"));
    }
    let queue = settlement_queue(&h, true);
    let conditions_root = object_id(
        b"misaka-palw/native/stage-original-independent-Final-conditions/v28",
        &(
            conditions.policy_root(),
            conditions.basis_root(),
            conditions.complete_path_daa(),
            h.original,
            book.claim(),
            h.contract.root(),
        ),
    );
    let outcome = book
        .finalize(&NativeStageFinalV28 {
            job: id,
            original: h.original,
            claim: book.claim().unwrap(),
            consent: h.consent,
            contract: h.contract.root(),
            final_policy: conditions.policy_root(),
            final_basis: conditions.basis_root(),
            physical: p.physical_service,
            producer_operator: book.producer_operator(),
            established: ctx.daa_score,
            pending_units: 0,
            retained_owners: 0,
            source_original: quote.root(),
            original_source_closed: true,
            original_source_equivalent: true,
            original_trace_closed: true,
            actual_escrow: book.reward(),
            actual_producer_credit: book.reward(),
            producer_queue: queue.as_bytes(),
            original_native_conditions: conditions_root,
        })
        .map_err(refused)?;
    settle_plain(builder, ctx, p, &mut h, &mut data, &book, &plan, &outcome)?;
    master32::note_final(builder, &h, &book)
}

fn collect_terminal(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV28,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    use crate::{
        palw_flat_execution_capacity_v22::NativeCarriageGlobalV22, palw_native_paged_proof_v29::NativePagedLeaseJournalV29,
        palw_native_queue_v30::*,
    };
    use misaka_palw_kernel::flat_quota11::{source16::*, stage14::source26::StageSourceCallerRowV26};
    let id = body.try_into().map_err(|_| refused("stage28 public terminal GC exact original job"))?;
    let h = original_header(builder, id)?;
    let (s, c) = original_extents(builder, h.class)?;
    if !pay_restore(builder, ctx, signer, p, s, c, h.input_bytes, 64)? {
        return Ok(());
    }
    let (class, actual, plan, book, _data) = restore_job(builder, p, &h)?;
    let _ = class;
    let _ = actual;
    if !matches!(book.terminal(), StageTerminalV28::Final | StageTerminalV28::Refunded | StageTerminalV28::Convicted)
        || ctx.daa_score <= book.deadline()
    {
        return Err(refused("stage28 collection lacks genuine original expired terminal"));
    }
    source28::quiescent(builder, &h, ctx.daa_score)?;
    master32::check_collect(builder, &h, &book)?;
    master32::collect_commit_fragments(builder, id, ctx.daa_score)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let table = crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11;
    let owners: Vec<Digest> = read(r, 19, &id, 4 + 4 * 64).unwrap_or_default();
    if owners.len() > 4 {
        return Err(refused("stage28 original owner lifetime catalogue differs"));
    }
    for owner in &owners {
        let own = r.native_paged_seal_original_v29(id, *owner).ok_or_else(|| refused("stage28 archived owner absent"))?;
        let raw =
            r.aux.get(&(table, bytes(&(26u8, id, *owner)))).ok_or_else(|| refused("stage28 original caller retirement absent"))?;
        let (root, caller): (Digest, StageSourceCallerRowV26) =
            borsh::from_slice(raw).map_err(|_| refused("stage28 caller retirement grammar"))?;
        if !own.retired
            || own.original_funding != h.original
            || ctx.daa_score <= own.horizon
            || caller.root() != root
            || !caller.retired
            || caller.owner != *owner
            || caller.original_paid != own.original_paid_receipt
            || caller.remaining != Default::default()
        {
            return Err(refused("stage28 original owner/source/cache liability remains retained"));
        }
    }
    if !source28::collect_units(builder, &h)? {
        return Ok(());
    }
    let settlement = book.settlement().ok_or_else(|| refused("stage28 genuine terminal settlement absent"))?;
    let queue = settlement_queue(&h, book.terminal() == StageTerminalV28::Final);
    let terminal = terminal_grant(builder, p, &h, &book, &plan, settlement, queue)?;
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let quote = source28::source_quote(builder, p, &h)?;
    let mut source = source28::producer_shard(builder, p, &epoch, &h, &quote)?;
    if !source.source_row().unwrap().retired {
        if epoch.network_interval == book.admitted_interval() {
            source.retire(&p.execution.quota, &epoch, &quote, &terminal).map_err(refused)?;
            super::super::palw_flat_execution_fold_v11::write_source_shard(builder, &source, id)?;
        } else {
            let mut caller = super::super::palw_flat_execution_fold_v11::quota_shard(
                builder,
                &p.execution,
                &epoch,
                palw_kernel_bond_id_v1(signer),
                id,
            )?;
            let row = source.source_row().unwrap();
            let retired = retire_expired_source_v16(&mut caller, &p.execution.quota, &epoch, &quote, row, &row.root(), &terminal)
                .map_err(refused)?;
            super::super::palw_flat_execution_fold_v11::write_shard(builder, &caller, id)?;
            builder.write_kernel_row(table, bytes(&(16u8, id)), Some(bytes(&(retired.root(), retired))))?;
        }
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let (mut qlease_root, mut qlease): (Digest, NativeQueueLeaseV30) =
        borsh::from_slice(r.aux.get(&(table, bytes(&(30u8, id, 1u8)))).ok_or_else(|| refused("stage28 original queue lease absent"))?)
            .map_err(|_| refused("stage28 original queue lease grammar"))?;
    let (jroot, mut journal): (Digest, NativePagedLeaseJournalV29) = borsh::from_slice(
        r.aux.get(&(table, bytes(&(29u8, id, 22u8)))).ok_or_else(|| refused("stage28 original physical lease absent"))?,
    )
    .map_err(|_| refused("stage28 original physical lease grammar"))?;
    let (pgroot, mut physical): (Digest, NativeCarriageGlobalV22) =
        borsh::from_slice(r.aux.get(&(table, bytes(&(22u8, 0u8)))).ok_or_else(|| refused("stage28 original physical global absent"))?)
            .map_err(|_| refused("stage28 original physical global grammar"))?;
    let (qgroot, mut global): (Digest, NativeQueueGlobalV30) =
        borsh::from_slice(r.aux.get(&(table, bytes(&(30u8, 0u8)))).ok_or_else(|| refused("stage28 original queue global absent"))?)
            .map_err(|_| refused("stage28 original queue global grammar"))?;
    if qlease.root() != qlease_root
        || qlease.job != id
        || qlease.original_funding != h.original
        || qlease.original_consent != h.consent
        || qlease.original_plan != h.contract.queue_plan
        || journal.root() != jroot
        || journal.namespace != 28
        || journal.job != id
        || journal.funding != h.original
        || journal.lease_root != journal.lease.root()
        || physical.root() != pgroot
        || global.root() != qgroot
    {
        return Err(refused("stage28 joint original queue/physical roots differ"));
    }
    let closure = CheckedNativeQueueClosureV30::from_current_native(
        id,
        h.original,
        qlease_root,
        journal.lease_root,
        book.root(),
        ctx.daa_score,
        false,
    )
    .map_err(refused)?;
    let policy = &builder.extras.kernel_route.as_ref().unwrap().native_final.as_ref().unwrap().policy.queue_policy;
    global.retire(&mut physical, policy, &mut qlease, &mut journal.lease, &closure).map_err(refused)?;
    qlease_root = qlease.root();
    let _ = qlease_root;
    let mut shard =
        super::super::palw_flat_execution_fold_v11::quota_shard(builder, &p.execution, &epoch, palw_kernel_bond_id_v1(signer), id)?;
    shard.collect_closed_expired(&p.execution.quota, &epoch, &terminal).map_err(refused)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let mut requester_ids: Vec<Digest> =
        read(r, 12, &h.requester, 4 + 8 * 64).ok_or_else(|| refused("stage28 original requester catalogue absent"))?;
    let mut producer_ids: Vec<Digest> =
        read(r, 13, &h.producer, 4 + 8 * 64).ok_or_else(|| refused("stage28 original producer catalogue absent"))?;
    if !requester_ids.contains(&id) || !producer_ids.contains(&id) {
        return Err(refused("stage28 original terminal catalogue membership differs"));
    }
    requester_ids.retain(|n| *n != id);
    producer_ids.retain(|n| *n != id);
    // Preserve immutable original consent/nonce/actual-credit/terminal anchors.
    // Only genuine expired quota, cached payloads and catalogue slots disappear.
    super::super::palw_flat_execution_fold_v11::write_shard(builder, &shard, id)?;
    builder.write_kernel_row(table, bytes(&(2u8, id)), None)?;
    builder.write_kernel_row(table, bytes(&(22u8, 0u8)), Some(bytes(&(physical.root(), physical))))?;
    builder.write_kernel_row(table, bytes(&(30u8, 0u8)), Some(bytes(&(global.root(), global))))?;
    builder.write_kernel_row(STAGE_TABLE_V14, key(12, &h.requester), Some(bytes(&requester_ids)))?;
    builder.write_kernel_row(STAGE_TABLE_V14, key(13, &h.producer), Some(bytes(&producer_ids)))?;
    builder.write_kernel_row(
        STAGE_TABLE_V14,
        key(20, &id),
        Some(bytes(&(h.original, book.root(), book.class(), book.source(), book.claim(), settlement, h.contract.root()))),
    )?;
    for owner in owners {
        builder.write_kernel_row(table, bytes(&(29u8, id, owner)), None)?;
        builder.write_kernel_row(table, bytes(&(26u8, id, owner)), None)?;
    }
    for k in [
        bytes(&(16u8, id)),
        bytes(&(16u8, id, 1u8)),
        bytes(&(26u8, id, 0u8)),
        bytes(&(29u8, id, 0u8)),
        bytes(&(29u8, id, 1u8)),
        bytes(&(29u8, id, 3u8)),
        bytes(&(29u8, id, 22u8)),
        bytes(&(29u8, id, 23u8)),
        bytes(&(22u8, id, 2u8)),
        bytes(&(30u8, id, 1u8)),
        bytes(&(30u8, id, 2u8)),
    ] {
        builder.write_kernel_row(table, k, None)?;
    }
    for kind in [5, 6, 16, 18, 19] {
        builder.write_kernel_row(STAGE_TABLE_V14, key(kind, &id), None)?;
    }
    master32::collect_material(builder, id)?;
    builder.note_kernel_bond(&h.requester)?;
    builder.note_kernel_bond(&h.producer)?;
    Ok(())
}
/// Called only by the common new paged handler AFTER persisted original Part,
/// Assembly and Court debits and the exact paid proof/salt hash verification.
pub(super) fn reveal_paged_v28(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    seal: &crate::palw_native_paged_proof_v29::NativePagedSealV29,
    file: &[u8],
) -> Result<(), PalwStateV2Error> {
    use crate::palw_native_source_proof_v25::*;
    use misaka_palw_kernel::{
        element::SegClaimContextV1,
        flat_court5::{FlatSegContextV5, flat_trace_profile_id_v5},
        job::DecodeRuleV1,
        seg::prompt_root_of_ids_v1,
    };
    let p = policy(builder, ctx)?;
    let mut h = original_header(builder, seal.job)?;
    if seal.namespace != 28
        || seal.bond != *signer
        || seal.actor != palw_kernel_bond_id_v1(signer)
        || seal.spent
        || h.terminal != StageTerminalV28::Challengeable
        || seal.original_funding != h.original
        || seal.maximum_contract != h.contract.root()
        || seal.original_plan != h.contract.paged_plan
        || seal.original_service != p.physical_service
        || ctx.daa_score < seal.matures
        || ctx.daa_score >= seal.reveal_until
        || builder.state.bonds.get(signer).is_none_or(|b| b.operator_id.as_bytes() != seal.operator)
    {
        return Err(refused("stage28 paged court original own paid source/claim/maximum/operator/time differs"));
    }
    let (class, actual, plan, mut book, mut data) = restore_job(builder, &p, &h)?;
    let maximum = contract(builder, &p, &class, &actual, &plan)?;
    if maximum.record() != &h.contract
        || Some(seal.claim) != book.claim()
        || seal.maximum_scope != class.maximum_scope().root()
        || seal.real_parameter != class.source().parameters().root()
        || seal.inventory != class.source().inventory_root()
        || seal.producer_operator != book.producer_operator()
        || builder.state.bonds.get(&h.producer).is_none_or(|b| b.operator_id.as_bytes() != book.producer_operator())
    {
        return Err(refused("stage28 paged court independently restored original producer/source/input/scope/basis differs"));
    }
    let n = u32::from_le_bytes(file.get(64..68).ok_or_else(|| refused("stage28 paid filing fixed length absent"))?.try_into().unwrap())
        as usize;
    if n != file.len().saturating_sub(68) {
        return Err(refused("stage28 paid filing exact whole length differs"));
    }
    let filing = borrowed_native_filing_v25(&file[68..], maximum.paged().record().maximum_bytes)
        .ok_or_else(|| refused("stage28 paid filing independently bounded version25 differs"))?;
    let mut refutes_source = false;
    match filing.kind {
        0 => {
            let trace = StageTraceV14::from_invocation_bytes_v32(
                &data.trace,
                master32::actual_positions(builder.state.kernel_route.as_ref().unwrap(), h.id, h.positions)?,
            )
            .map_err(refused)?;
            if master32::binding(builder.state.kernel_route.as_ref().unwrap(), h.id)?.is_some() {
                master32::verify_computation(builder, &p, &h, &book, &class, &data, filing.body)?;
            } else {
                let program = actual.program().v1_view();
                let prompt = vec![0; h.positions as usize];
                let base = FlatSegContextV5 {
                    profile: flat_trace_profile_id_v5(),
                    claim: SegClaimContextV1 {
                        program: &program,
                        params: actual.combined_parameters(),
                        segment_roots: &trace.segment_roots,
                        positions: h.positions,
                        prompt_len: h.positions,
                        prompt_root: prompt_root_of_ids_v1(&prompt),
                        inline_prompt: Some(&prompt),
                        generated: &[0],
                        decode: DecodeRuleV1::Greedy,
                        encoder: None,
                    },
                };
                let version = filing
                    .body
                    .get(..2)
                    .map(|v| u16::from_le_bytes(v.try_into().unwrap()))
                    .ok_or_else(|| refused("stage28 computation court version absent"))?;
                match version {
                    25 => {
                        let f = misaka_palw_kernel::flat_point25::parse_flat_point_fault_v25(filing.body)
                            .map_err(|e| refused(e.to_string()))?;
                        misaka_palw_kernel::flat_point25::verify_flat_point_fault_v25(
                            &misaka_palw_kernel::flat_point25::PointContextV25 {
                                court_id: misaka_palw_kernel::flat_point25::ordinary_point_court_id_v25(),
                                base,
                            },
                            &f,
                        )
                        .map_err(|e| refused(format!("{e:?}")))?;
                    }
                    7 => {
                        let f = misaka_palw_kernel::flat_topk7::TopKFaultV7::from_bytes(filing.body)
                            .map_err(|e| refused(e.to_string()))?;
                        misaka_palw_kernel::flat_topk7::verify_topk_fault_v7(
                            &misaka_palw_kernel::flat_topk7::TopKContextV7 {
                                court_id: misaka_palw_kernel::flat_topk7::topk_court_id_v7(),
                                base,
                            },
                            &f,
                        )
                        .map_err(|e| refused(format!("{e:?}")))?;
                    }
                    5 => {
                        let f = misaka_palw_kernel::flat_court5::FlatFaultV5::from_bytes(filing.body)
                            .map_err(|e| refused(e.to_string()))?;
                        if matches!(f, misaka_palw_kernel::flat_court5::FlatFaultV5::Decode(_)) {
                            return Err(refused("stage28 Rows/Final never accepts decoder court"));
                        }
                        misaka_palw_kernel::flat_court5::verify_flat_fault_v5(&base, &f).map_err(|e| refused(format!("{e:?}")))?;
                    }
                    _ => return Err(refused("stage28 computation filing differs from explicitly priced checker25/TopK7/legacy5 set")),
                }
            }
        }
        1 => {
            let mut real = class.source().program().v1_view();
            real.params.truncate(class.source().program().params.len());
            let bound = derive_native_source_proof_bound_v25(&real, class.source().parameters()).map_err(refused)?;
            if bound.root() != h.contract.source_proof25 {
                return Err(refused("stage28 original real source proof bound differs"));
            }
            let proof = decode_native_source_fault_v25(&file[68..], &bound).map_err(refused)?;
            let view = class.source().program().v1_view();
            let inventory =
                crate::palw_tir_artifact_v1::PalwTirModelInventoryV2::new_stage14(&view, class.source().program()).map_err(refused)?;
            crate::palw_onboarding_v1::verify_artifact_mismatch_v2(
                &inventory,
                crate::Hash64::from_bytes(class.source().inventory_root()),
                crate::Hash64::from_bytes(class.source().parameters().root()),
                &proof,
            )
            .map_err(refused)?;
            refutes_source = true;
        }
        2 => {
            if filing.body.len() > 512 {
                return Err(refused("stage28 real source header court fixed bound"));
            }
            let f: crate::palw_flat_execution_v11::NativeSourceHeaderFaultV16 =
                borsh::from_slice(filing.body).map_err(|_| refused("stage28 source header original grammar"))?;
            if f.layer_code > 65536 {
                return Err(refused("stage28 original source header layer code exceeded"));
            }
            let layer = if f.layer_code == 0 { None } else { Some((f.layer_code - 1) as u16) };
            let pc = class
                .source()
                .parameters()
                .by_instance
                .get(&(f.param, layer))
                .ok_or_else(|| refused("stage28 source header uses actual input PC as source"))?;
            let decl = class
                .source()
                .program()
                .params
                .get(f.param as usize)
                .ok_or_else(|| refused("stage28 real source declaration absent"))?;
            if !f.header.authenticates(pc) || f.header.matches_decl(decl.dtype, &decl.shape) {
                return Err(refused("stage28 source header does not establish bound original declaration mismatch"));
            }
            refutes_source = true;
        }
        _ => return Err(refused("unknown stage28 source/computation court")),
    }
    let reserve = book.reserve();
    let collected = builder.state.bonds.get(&h.producer).map(|b| b.collateral.min(reserve)).unwrap_or(0);
    if collected != reserve {
        return Err(refused("stage28 original whole-claim producer collateral is no longer actually collectible"));
    }
    let proof = object_id(
        b"misaka-palw/native/stage-original-priced-court-result/v28",
        &(seal.id(), seal.original_paid_receipt, h.original, h.contract.root(), &file[68..]),
    );
    let outcome = book
        .convict(&NativeStageConvictionV28 {
            job: h.id,
            original: h.original,
            claim: seal.claim,
            producer_operator: book.producer_operator(),
            finder: seal.actor,
            finder_operator: seal.operator,
            proof,
            actual_collected: reserve,
            established: ctx.daa_score,
        })
        .map_err(refused)?;
    settle_conviction(builder, ctx, signer, &p, &class, &plan, &mut h, &mut data, &book, &outcome, refutes_source)?;
    Ok(())
}
fn bind(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV28,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    if body.len() > 16 << 20 {
        return Err(refused("stage28 complete source carrier cap"));
    }
    if !pay_restore(builder, ctx, signer, p, 0, 0, 0, body.len() as u32)? {
        return Ok(());
    }
    let bundle = NativeStageSourceBundleV28::from_bytes(body).map_err(refused)?;
    let (manifest, sources) = bundle.check().map_err(refused)?;
    let id = manifest.root();
    let r = builder.state.kernel_route.as_ref().unwrap();
    let mut catalog: Vec<Digest> = read(r, 7, signer, 4 + 8 * 64).unwrap_or_default();
    if catalog.len() >= p.catalog_cap as usize && !catalog.contains(&id) {
        return Err(refused("stage28 own source catalog full"));
    }
    if sources.iter().any(|s| r.stage_source_refuted_relation_v14(&s.relation_root())) {
        return Err(refused("stage28 bundle aliases refuted original R/PC relation"));
    }
    if read::<BundleHeader>(r, 0, &id, 8192).is_some() {
        return Ok(());
    }
    catalog.push(id);
    let h = BundleHeader {
        version: 28,
        id,
        bytes: body.len() as u32,
        owner: *signer,
        registered: ctx.daa_score,
        source_ids: sources.iter().map(|s| s.id()).collect(),
    };
    builder.write_kernel_row(STAGE_TABLE_V14, key(0, &id), Some(bytes(&h)))?;
    builder.write_kernel_row(STAGE_TABLE_V14, key(1, &id), Some(bytes(&body.to_vec())))?;
    builder.write_kernel_row(STAGE_TABLE_V14, key(7, signer), Some(bytes(&catalog)))?;
    builder.note_kernel_bond(signer)?;
    Ok(())
}
fn register(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV28,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    let wire = FlatStageClassWireV28::from_bytes(body).map_err(refused)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let h: BundleHeader = read(r, 0, &wire.complete_source, 8192).ok_or_else(|| refused("stage28 original pipeline bundle absent"))?;
    if !pay_restore(builder, ctx, signer, p, h.bytes, 0, 0, body.len() as u32)? {
        return Ok(());
    }
    let raw: Vec<u8> = read(builder.state.kernel_route.as_ref().unwrap(), 1, &h.id, 16 << 20)
        .ok_or_else(|| refused("stage28 original pipeline body absent"))?;
    let bundle = NativeStageSourceBundleV28::from_bytes(&raw).map_err(refused)?;
    let (manifest, sources) = bundle.check().map_err(refused)?;
    let source = sources.get(wire.stage as usize).ok_or_else(|| refused("stage28 original component absent"))?;
    let mut class = wire.check(source).map_err(refused)?;
    class.bind_complete_bundle(&manifest).map_err(refused)?;
    if wire.policy != p.root() || class.maximum_resources().quantities.0.iter().zip(&p.execution.quota.plan.0).any(|(n, c)| n > c) {
        return Err(refused("stage28 independent maximum exceeds immutable full21 policy"));
    }
    let id = class.root();
    let mut ids: Vec<Digest> = read(builder.state.kernel_route.as_ref().unwrap(), 8, signer, 4 + 8 * 64).unwrap_or_default();
    if ids.len() >= p.catalog_cap as usize && !ids.contains(&id) {
        return Err(refused("stage28 own class catalog full"));
    }
    if read::<ClassHeader>(builder.state.kernel_route.as_ref().unwrap(), 2, &id, 1024).is_some() {
        return Ok(());
    }
    ids.push(id);
    let h = ClassHeader {
        version: 28,
        id,
        bundle: manifest.root(),
        source: source.id(),
        policy: p.root(),
        scope: class.maximum_scope().root(),
        resource: class.maximum_resources().root(),
        bytes: body.len() as u32,
        owner: *signer,
    };
    builder.write_kernel_row(STAGE_TABLE_V14, key(2, &id), Some(bytes(&h)))?;
    builder.write_kernel_row(STAGE_TABLE_V14, key(3, &id), Some(bytes(&body.to_vec())))?;
    builder.write_kernel_row(STAGE_TABLE_V14, key(8, signer), Some(bytes(&ids)))?;
    builder.note_kernel_bond(signer)?;
    Ok(())
}
pub(super) fn apply(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    action: u8,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    if !(19..=40).contains(&action) {
        return Err(refused("unknown stage28 action before bounded body decode"));
    }
    let p = policy(builder, ctx)?;
    let result = match action {
        19 => bind(builder, ctx, signer, &p, body),
        20 => register(builder, ctx, signer, &p, body),
        21 => accept(builder, ctx, signer, &p, body),
        22 => fund(builder, ctx, signer, &p, body),
        23 => commit(builder, ctx, signer, &p, body),
        24 => owner(builder, ctx, signer, &p, body),
        25 => source28::request(builder, ctx, signer, &p, body),
        26 => source28::answer(builder, ctx, signer, &p, body),
        27 => source28::settle(builder, ctx, signer, &p, body),
        28 => discovery_convert(builder, ctx, signer, &p, body),
        29 => finalize(builder, ctx, signer, &p, body),
        30 => expire_owner(builder, ctx, signer, &p, body),
        31 => collect_terminal(builder, ctx, signer, &p, body),
        32 => source28::reply_part(builder, ctx, signer, &p, body),
        33 => master32::register(builder, ctx, signer, &p, body),
        34 => master32::accept_invocation(builder, ctx, signer, &p, body),
        35 => master32::fund_invocation(builder, ctx, signer, &p, body),
        36 => master32::commit_invocation(builder, ctx, signer, &p, body),
        37 => master32::collect_master(builder, ctx, signer, &p, body),
        38 => master32::commit_compact(builder, ctx, signer, &p, body),
        39 => master32::commit_part(builder, ctx, signer, &p, body),
        40 => master32::assemble_commit(builder, ctx, signer, &p, body),
        _ => Err(refused("stage28 native action implementation pending")),
    };
    match result {
        Err(PalwStateV2Error::KernelRouteRefused(_)) => Ok(()),
        other => other,
    }
}

/// Independent audit extraction of one genuine Stage28 component producer Final from
/// authenticated original rows only (audit32's full aux opening or the live store).
/// Stage28 keeps its own credit identifier (`settlement_queue`, fixed before the
/// settlement exists) — carried verbatim as the record's `payout_queue` and
/// authenticated here against the original header and the immutable kind-14
/// receipt; there is no consensus change. Namespace-28 field mapping: `program` is
/// the complete source-bundle root, `real_parameter`/`inventory` the component
/// source id (which binds its R/PC/inventory), `trace` the stored trace image root.
pub(super) fn audit_record_from_rows_v28(
    rows: &std::collections::BTreeMap<(u8, Vec<u8>), Vec<u8>>,
    network_domain: Digest,
    daa: u64,
    roots: (Digest, Digest, Digest),
    payouts: &std::collections::BTreeMap<crate::Hash64, PalwPayoutV2>,
    id: Digest,
) -> Result<crate::palw_native_audit_v27::NativeExecutionAuditRecordV27, String> {
    use crate::palw_native_audit_v27::{NativeAuditRowV27, NativeExecutionAuditRecordV27};
    fn get<'a>(
        rows: &'a std::collections::BTreeMap<(u8, Vec<u8>), Vec<u8>>,
        kind: u8,
        id: &Digest,
        cap: usize,
    ) -> Result<(Vec<u8>, &'a Vec<u8>), String> {
        let k = key(kind, id);
        let raw = rows.get(&(STAGE_TABLE_V14, k.clone())).ok_or("stage28 audit original row absent")?;
        if raw.len() > cap {
            return Err("stage28 audit original row cap".into());
        }
        Ok((k, raw))
    }
    let get = |kind: u8, id: &Digest, cap: usize| get(rows, kind, id, cap);
    let (hk, raw) = get(5, &id, 8192)?;
    let h: JobHeader = borsh::from_slice(raw).map_err(|_| "stage28 audit header grammar")?;
    let (_, raw) = get(6, &id, (1 << 20) + (2 << 20) + 16384)?;
    let data: JobData = borsh::from_slice(raw).map_err(|_| "stage28 audit body grammar")?;
    if h.id != id || data.book.len() != h.book_bytes as usize || data.trace.len() != h.trace_bytes as usize {
        return Err("stage28 audit original header/body extents differ".into());
    }
    let book: StageJobBookV28 = borsh::from_slice(&data.book).map_err(|_| "stage28 audit book grammar")?;
    let (ck, raw) = get(2, &h.class, 1024)?;
    let ch: ClassHeader = borsh::from_slice(raw).map_err(|_| "stage28 audit class grammar")?;
    let (rk, raw) = get(14, &id, 1024)?;
    let (credited, queue, payload): (StageSettlementV28, crate::Hash64, crate::Hash64) =
        borsh::from_slice(raw).map_err(|_| "stage28 audit credit grammar")?;
    let s = book.settlement().ok_or("stage28 audit Final lacks settlement")?;
    let nk = borsh::to_vec(&book.requester_operator()).map_err(|e| e.to_string())?;
    let raw = rows
        .get(&(crate::palw_flat_execution_v11::EXECUTION_REQUEST_INDEX_TABLE_V11, nk.clone()))
        .ok_or("stage28 audit permanent nonce row absent")?;
    if raw.len() != 72 {
        return Err("stage28 audit fixed nonce row".into());
    }
    let nonce: crate::palw_flat_execution_v11::NativeRequesterNonceRowV11 =
        borsh::from_slice(raw).map_err(|_| "stage28 audit nonce grammar")?;
    if book.id() != id
        || book.root() != h.book
        || book.original_root() != h.original
        || book.class() != h.class
        || ch.id != h.class
        || book.contract() != h.contract.root()
        || book.terminal() != StageTerminalV28::Final
        || h.terminal != StageTerminalV28::Final
        || credited != *s
        || queue != settlement_queue(&h, true)
        || payload != h.producer_payout
        || s.producer_credit == 0
        || s.producer_credit != book.reward()
        || s.escrow_debit != book.reward()
        || s.requester_credit != 0
        || s.actual_slash != 0
        || s.finder_credit != 0
        || s.burned != 0
        || book.final_after().is_none_or(|at| s.established <= at)
        || book.claim().is_none()
        || s.claim != book.claim()
        || nonce.operator != book.requester_operator()
        || nonce.next_nonce <= misaka_palw_kernel::flat_execution11::canonical_nonce_v11(&book.job().nonce)?
        || book.requester_operator() == book.producer_operator()
        || book.final_policy_root() == [0; 64]
    {
        return Err("stage28 audit Final/nonce/original debit/single Stage28 credit agreement differs".into());
    }
    let mut audit_rows = Vec::new();
    for (table, k) in [
        (STAGE_TABLE_V14, ck),
        (STAGE_TABLE_V14, hk),
        (STAGE_TABLE_V14, rk),
        (crate::palw_flat_execution_v11::EXECUTION_REQUEST_INDEX_TABLE_V11, nk),
    ] {
        let bytes = rows.get(&(table, k.clone())).ok_or("stage28 audit original row missing")?.clone();
        audit_rows.push(NativeAuditRowV27 { table, key: k, bytes });
    }
    let (state_root, ledger_root, aux_root) = roots;
    Ok(NativeExecutionAuditRecordV27 {
        version: 27,
        namespace: 28,
        network_domain,
        daa,
        state_root,
        ledger_root,
        aux_root,
        class: h.class,
        source: ch.source,
        program: ch.bundle,
        real_parameter: ch.source,
        inventory: ch.source,
        job: id,
        funding: h.original,
        original_escrow_debit: s.escrow_debit,
        original_escrow_receipt: h.original,
        original_consent: book.consent(),
        original_final_policy: book.final_policy_root(),
        original_final_basis: book.final_basis_root(),
        maximum_scope: book.maximum_scope_root(),
        maximum_resource: book.maximum_resources_root(),
        actual_scope: book.actual_scope_root(),
        actual_resource: book.actual_resources_root(),
        claim: book.claim().unwrap(),
        vector: book.claim().unwrap(),
        trace: object_id(b"misaka-palw/native/audit-stage-trace-image/v28", &data.trace),
        output: book.claim().unwrap(),
        terminal_book: book.root(),
        settlement: object_id(b"misaka-palw/native/audit-stage-settlement/v28", s),
        credit: object_id(b"misaka-palw/native/audit-single-credit/v28", &(s, queue, payload)),
        producer_credit: s.producer_credit,
        requester_operator: book.requester_operator(),
        producer_operator: book.producer_operator(),
        consumed_nonce: book.job().nonce,
        next_nonce: nonce.next_nonce,
        established_daa: s.established,
        payout_payload: h.producer_payout.as_bytes(),
        payout_queue: queue.as_bytes(),
        observed_queue_amount: payouts.get(&queue).filter(|p| p.payload == h.producer_payout).map(|p| p.amount),
        rows: audit_rows,
    })
}

impl crate::palw_state_v2::PalwChainStateV2 {
    /// Stateless Stage28 lifecycle extraction for the independent register/seat auditor.
    pub(crate) fn native_stage_lifecycle_from_rows_v28(
        rows: &std::collections::BTreeMap<(u8, Vec<u8>), Vec<u8>>,
        p: &NativeStagePoliciesV28,
        stage: crate::palw_native_audit_proof_v32::NativeLifecycleStageV32,
        id: Digest,
    ) -> Result<crate::palw_native_audit_proof_v32::NativeLifecycleAnchorV32, String> {
        use crate::palw_native_audit_proof_v32::{NativeLifecycleAnchorV32, NativeLifecycleStageV32 as S};
        let header = |class: Digest| -> Result<ClassHeader, String> {
            let raw = rows.get(&(STAGE_TABLE_V14, key(2, &class))).ok_or("stage28 lifecycle class header absent")?;
            if raw.len() > 1024 {
                return Err("stage28 lifecycle class cap".into());
            }
            borsh::from_slice(raw).map_err(|_| "stage28 lifecycle class grammar".into())
        };
        match stage {
            S::Register => {
                let class = master32::audit_class(rows, p, id)?;
                let h = header(id)?;
                Ok(NativeLifecycleAnchorV32 {
                    namespace: 28,
                    stage,
                    class: id,
                    source: h.source,
                    program: h.bundle,
                    maximum_scope: class.maximum_scope().root(),
                    maximum_resource: class.maximum_resources().root(),
                    job: [0; 64],
                    funding: [0; 64],
                    consent: [0; 64],
                    requester_operator: [0; 64],
                    producer_operator: [0; 64],
                    terminal: "registered".into(),
                    anchored_daa: 0,
                })
            }
            S::Seat => {
                let raw = rows.get(&(STAGE_TABLE_V14, key(5, &id))).ok_or("stage28 lifecycle header absent")?;
                if raw.len() > 8192 {
                    return Err("stage28 lifecycle header cap".into());
                }
                let jh: JobHeader = borsh::from_slice(raw).map_err(|_| "stage28 lifecycle header grammar")?;
                let raw = rows.get(&(STAGE_TABLE_V14, key(6, &id))).ok_or("stage28 lifecycle body absent")?;
                if raw.len() > (1 << 20) + (2 << 20) + 16384 {
                    return Err("stage28 lifecycle body cap".into());
                }
                let data: JobData = borsh::from_slice(raw).map_err(|_| "stage28 lifecycle body grammar")?;
                if jh.id != id || data.book.len() != jh.book_bytes as usize {
                    return Err("stage28 lifecycle header/body extent differs".into());
                }
                let book: StageJobBookV28 = borsh::from_slice(&data.book).map_err(|_| "stage28 lifecycle book grammar")?;
                let class = master32::audit_class(rows, p, jh.class)?;
                let actual = class.check_job(book.job())?;
                let plan = if rows.contains_key(&(STAGE_TABLE_V14, key(32, &id))) {
                    derive_stage_quota_v32(&class, &actual)?
                } else {
                    derive_stage_quota_v28(&class, &actual)?
                };
                book.check(&class, &actual, &plan)?;
                let h = header(jh.class)?;
                if book.id() != id
                    || book.root() != jh.book
                    || book.original_root() != jh.original
                    || book.consent() != jh.consent
                    || jh.consent == [0; 64]
                    || book.contract() != jh.contract.root()
                {
                    return Err("stage28 lifecycle funded original consent/contract differs".into());
                }
                Ok(NativeLifecycleAnchorV32 {
                    namespace: 28,
                    stage,
                    class: jh.class,
                    source: h.source,
                    program: h.bundle,
                    maximum_scope: book.maximum_scope_root(),
                    maximum_resource: book.maximum_resources_root(),
                    job: id,
                    funding: jh.original,
                    consent: jh.consent,
                    requester_operator: book.requester_operator(),
                    producer_operator: book.producer_operator(),
                    terminal: format!("{:?}", book.terminal()),
                    anchored_daa: 0,
                })
            }
        }
    }
}

// Readonly original receipt observation; cannot mint an admission or Final grant.
impl crate::palw_state_v2::PalwChainStateV2 {
    pub(crate) fn native_stage_receipt_from_rows_v1(
        rows: &std::collections::BTreeMap<(u8, Vec<u8>), Vec<u8>>,
        p: &NativeStagePoliciesV28,
        id: Digest,
    ) -> Result<crate::palw_native_receipt_windows_v1::NativeReceiptJobV1, String> {
        use crate::palw_native_audit_proof_v32::NativeLifecycleStageV32;
        let a = Self::native_stage_lifecycle_from_rows_v28(rows, p, NativeLifecycleStageV32::Seat, id)?;
        let raw = rows.get(&(STAGE_TABLE_V14, key(5, &id))).ok_or("receipt original stage header absent")?;
        if raw.len() > 8192 {
            return Err("receipt original stage header cap".into());
        }
        let h: JobHeader = borsh::from_slice(raw).map_err(|_| "receipt original stage header grammar")?;
        let raw = rows.get(&(STAGE_TABLE_V14, key(6, &id))).ok_or("receipt original stage data absent")?;
        if raw.len() > (1 << 20) + (2 << 20) + 16384 {
            return Err("receipt original stage data cap".into());
        }
        let data: JobData = borsh::from_slice(raw).map_err(|_| "receipt original stage data grammar")?;
        let book: StageJobBookV28 = borsh::from_slice(&data.book).map_err(|_| "receipt original stage book grammar")?;
        if book.root() != h.book || book.terminal() != h.terminal {
            return Err("receipt original stage terminal differs".into());
        }
        let raw = rows.get(&(STAGE_TABLE_V14, key(2, &h.class))).ok_or("receipt original stage class header absent")?;
        if raw.len() > 1024 {
            return Err("receipt original stage class header cap".into());
        }
        let ch: ClassHeader = borsh::from_slice(raw).map_err(|_| "receipt original stage class header grammar")?;
        let raw = rows.get(&(STAGE_TABLE_V14, key(0, &ch.bundle))).ok_or("receipt original complete bundle header absent")?;
        if raw.len() > 8192 {
            return Err("receipt original complete bundle header cap".into());
        }
        let bundle: BundleHeader = borsh::from_slice(raw).map_err(|_| "receipt original complete bundle header grammar")?;
        let class = master32::audit_class(rows, p, h.class)?;
        let (admitted, commit_deadline, committed) = book.receipt_clocks_v1();
        let invocation = rows.contains_key(&(STAGE_TABLE_V14, key(32, &id)));
        let cached = cached_plan(&h, p).map_err(|e| e.to_string())?;
        if cached.is_invocation_v32() != invocation
            || (h.contract.final_basis.verifier_profile == misaka_palw_kernel::flat_inputs12::court32::ordinary_stage_court_id_v32())
                != invocation
        {
            return Err("receipt original invocation32 quota/court contract differs".into());
        }

        Ok(crate::palw_native_receipt_windows_v1::NativeReceiptJobV1 {
            namespace: 28,
            complete_model: bundle.source_ids.len() == 1 && !rows.contains_key(&(STAGE_TABLE_V14, key(32, &id))),
            job: id,
            class: a.class,
            source: a.source,
            inventory: class.source().inventory_root(),
            real_parameter: class.source().parameters().root(),
            funding: a.funding,
            consent: a.consent,
            maximum_contract: h.contract.root(),
            maximum_scope: a.maximum_scope,
            maximum_resource: a.maximum_resource,
            queue_plan: h.contract.queue_plan,
            queue_policy: h.contract.queue_policy,
            source_plan: h.contract.source_plan,
            physical_service: book.physical_root(),
            physical_plan: h.contract.physical_plan,
            carriage_version: if invocation { 32 } else { 29 },
            requester_operator: a.requester_operator,
            producer_operator: a.producer_operator,
            nonce: book.job().nonce,
            admitted,
            commit_deadline,
            deadline: book.deadline(),
            claim: book.claim(),
            committed,
            challenge_end: book.challenge_end(),
            terminal: a.terminal,
        })
    }
}

/// G14 item 4 (lane r5-f03-ingress): the stage28 lifetime owner catalogue's reclamation (cap 4), on the pure admission and the
/// progress predicate — the text29 rule of court-owner round 2, applied to namespace 28.
#[cfg(test)]
mod g14_r5_f03_stage28_reclaim_tests {
    use super::*;
    use crate::palw_native_paged_proof_v29::NativePagedSealV29;

    fn d(n: u8) -> Digest {
        [n; 64]
    }

    fn probe_seal() -> NativePagedSealV29 {
        NativePagedSealV29 {
            version: 29,
            namespace: 28,
            job: d(0x11),
            claim: d(0x12),
            original_funding: d(0x13),
            maximum_contract: d(0x14),
            original_plan: d(0x15),
            original_service: d(0x16),
            original_lease: d(0x17),
            maximum_scope: d(0x18),
            real_parameter: d(0x19),
            inventory: d(0x1A),
            bond: PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Default::default(), 0)),
            actor: d(0x1B),
            operator: d(0x1C),
            producer_operator: d(0x1D),
            proof_root: d(0x1E),
            seal_root: d(0x1F),
            original_paid_receipt: d(0x20),
            count: 1,
            total_bytes: 68,
            posted: 10,
            matures: 20,
            reveal_until: 30,
            horizon: 40,
            admitted_interval: 0,
            admitted_block: d(0x21),
            part_attempts: [0; 512],
            accepted: [0; 64],
            assembly_attempts: 0,
            court_attempts: 0,
            discovery: false,
            original_ticket: d(0x22),
            conversion_attempts: 0,
            retired: false,
            spent: false,
            committed: Default::default(),
            consumed: Default::default(),
            remaining: Default::default(),
        }
    }

    /// Four stalled owners (expired, no progress) and an outsider after their deadline: past the fence the outsider takes the first
    /// stalled owner's entry; the cap stays 4; a duplicate is still refused.
    #[test]
    fn r5_f03_stage28_four_stalled_owners_yield_a_slot_to_an_outsider_after_the_deadline() {
        let catalogue = [d(1), d(2), d(3), d(4)];
        let got = stage28_owner_catalogue_admit_v28(&catalogue, d(9), true, |_| true).expect("a stalled owner yields");
        assert_eq!(got, Some(d(1)), "catalogue order picks the first stalled owner");
        let mut after: Vec<Digest> = catalogue.to_vec();
        after.retain(|v| Some(*v) != got);
        after.push(d(9));
        assert_eq!(after.len(), 4, "the cap stays 4");
        assert_eq!(stage28_owner_catalogue_admit_v28(&after, d(10), true, |m| *m != d(9)), Ok(Some(d(2))));
        assert!(stage28_owner_catalogue_admit_v28(&after, d(9), true, |_| true).is_err());
        assert_eq!(stage28_owner_catalogue_admit_v28(&[d(1)], d(9), false, |_| unreachable!()), Ok(None), "room: no reclamation");
    }

    /// Owners that made progress are never evicted; the predicate reads namespace 28 only.
    #[test]
    fn r5_f03_stage28_progressing_owners_are_never_evicted() {
        let catalogue = [d(1), d(2), d(3), d(4)];
        assert!(stage28_owner_catalogue_admit_v28(&catalogue, d(9), true, |_| false).is_err());
        assert_eq!(stage28_owner_catalogue_admit_v28(&catalogue, d(9), true, |m| *m == d(3)), Ok(Some(d(3))));
        let mut seal = probe_seal();
        seal.retired = true;
        assert!(stage28_owner_made_no_progress_v28(&seal, 41));
        assert!(!stage28_owner_made_no_progress_v28(&seal, 40), "not past its deadline");
        for touch in 0..5 {
            let mut s = seal.clone();
            match touch {
                0 => s.part_attempts[7] = 1,
                1 => s.accepted[0] = 1,
                2 => s.assembly_attempts = 1,
                3 => s.court_attempts = 1,
                _ => s.conversion_attempts = 1,
            }
            assert!(!stage28_owner_made_no_progress_v28(&s, 41), "progress {touch} is never evicted");
        }
        let mut other = seal.clone();
        other.namespace = 29;
        assert!(!stage28_owner_made_no_progress_v28(&other, 41), "a text29 seal is not a stage28 owner");
        seal.retired = false;
        assert!(!stage28_owner_made_no_progress_v28(&seal, 41), "a live owner is never evicted");
    }

    /// Below the fence: a full catalogue refuses with the OLD text and the predicate is never consulted.
    #[test]
    fn r5_f03_stage28_below_the_fence_the_catalogue_is_unchanged() {
        let catalogue = [d(1), d(2), d(3), d(4)];
        assert_eq!(
            stage28_owner_catalogue_admit_v28(&catalogue, d(9), false, |_| panic!("never consulted below the fence")),
            Err("stage28 original four-owner lifetime catalogue exhausted")
        );
    }
}
