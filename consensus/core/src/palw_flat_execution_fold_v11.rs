//! Independently fenced native funded execution. All public actions are signed
//! outer-110 objects. Native selects original rooted rows, derives clocks and
//! prices, and writes nonce/coins/quota/book/credit in one journal transaction.
use super::*;
use crate::palw_flat_execution_v11::*;
use crate::palw_kernel_route_v1::{PalwKernelRouteStateV1, palw_kernel_bond_id_v1};
use misaka_palw_kernel::flat_quota11::source16::{SOURCE_ROW_BYTES_V16, SourceObligationV16, SourcePlanV16, SourceShardV16};
use misaka_palw_kernel::flat_quota11::trace17::*;
use misaka_palw_kernel::{
    flat_execution11::*,
    flat_quota11::*,
    hash::{Digest, object_id},
};

#[path = "palw_flat_execution_capacity_fold_v23.rs"]
mod capacity;
#[path = "palw_flat_execution_court_fold_v11.rs"]
mod court;
#[path = "palw_flat_execution_da_fold_v11.rs"]
mod da;
#[path = "palw_flat_execution_discovery_fold_v13.rs"]
mod discovery;
#[path = "palw_flat_execution_final_fold_v27.rs"]
mod final27;
#[path = "palw_flat_execution_gc_fold_v24.rs"]
mod gc;
#[path = "palw_flat_execution_source_fold_v16.rs"]
pub(super) mod source;

fn refused(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(why.into())
}
fn key<T: borsh::BorshSerialize>(value: &T) -> Vec<u8> {
    borsh::to_vec(value).expect("fixed native key")
}
fn fixed_id(bytes: &[u8]) -> Result<Digest, PalwStateV2Error> {
    bytes.try_into().map_err(|_| refused("funded action requires one exact digest"))
}
fn policies(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<NativeExecutionPoliciesV11, PalwStateV2Error> {
    super::palw_flat_model_vector_fold_v3::enabled(builder, ctx)?;
    let fence = builder
        .extras
        .kernel_route
        .as_ref()
        .and_then(|e| e.funded_execution.as_ref())
        .ok_or_else(|| refused("palw_funded_execution_v11 is not in force"))?;
    if !fence.activation.is_active(ctx.daa_score) {
        return Err(refused("funded execution is below its independent fence"));
    }
    let p = fence.policies.clone();
    p.check(builder.state.kernel_route.as_ref().unwrap().header.policy.network_domain).map_err(refused)?;
    let stored = builder.state.kernel_route.as_ref().unwrap().aux.get(&(EXECUTION_POLICY_TABLE_V11, Vec::new()));
    let bytes = key(&p);
    if stored.is_some_and(|old| old != &bytes) {
        return Err(refused("funded execution immutable policy changed"));
    }
    if stored.is_none() {
        builder.write_kernel_row(EXECUTION_POLICY_TABLE_V11, Vec::new(), Some(bytes));
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
fn source(route: &PalwKernelRouteStateV1, binding: Digest) -> Result<NativeSourceBindingV11, PalwStateV2Error> {
    let h = route
        .model_artifact_binding_header_v2(&Hash64::from_bytes(binding))
        .ok_or_else(|| refused("native original source binding absent"))?;
    Ok(NativeSourceBindingV11 {
        descriptor: h.descriptor,
        program_root: h.program_root,
        parameter_root: h.kernel_param_root.as_bytes(),
        inventory_root: h.model_inventory_root.as_bytes(),
        bound_daa: h.bound_daa,
        matures_daa: h.matures_daa,
        refuted: h.refuted,
    })
}
fn checked_class(
    builder: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    p: &NativeExecutionPoliciesV11,
    id: Digest,
    admit: bool,
) -> Result<CheckedNativeExecutionClassV11, PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let header = r.execution_class_header_v11(&id).ok_or_else(|| refused("unknown native execution class"))?;
    header.checked_fixed_header().map_err(refused)?;
    let c = &header.record;
    let s = source(r, c.binding_id)?;
    let candidate =
        r.flat_candidate_record_v9(&Hash64::from_bytes(c.candidate9_id)).ok_or_else(|| refused("original candidate9 absent"))?;
    let class =
        restore_native_execution_class_header_v11(&candidate, &s, Some(c), &p.execution, &p.quota, ctx.daa_score).map_err(refused)?;
    if !admit {
        return Ok(class);
    }
    if r.aux.contains_key(&(EXECUTION_SOURCE_FACT_TABLE_V11, key(&c.binding_id)))
        || r.aux.contains_key(&(EXECUTION_SOURCE_FACT_TABLE_V11, key(&(16u8, c.binding_id))))
    {
        return Err(refused("native verified original source theorem blocks fresh admission"));
    }
    if r.template().schedule.standing_at(&c.descriptor, ctx.daa_score) != misaka_palw_kernel::descriptor::KernelStandingV1::Active {
        return Err(refused("native funded descriptor not active"));
    }
    authorize_native_execution_class_header_v11(
        class,
        &s,
        &NativeClassEligibilityV11 {
            class_id: id,
            active_descriptor: c.descriptor,
            active_binding: c.binding_id,
            execution_policy_root: p.execution.root(),
            quota_policy_root: p.quota.root(),
            block_hash: ctx.block.as_bytes(),
            current_daa: ctx.daa_score,
        },
    )
    .map_err(refused)
}
fn class_work(builder: &TransitionBuilder<'_>, id: Digest, bytes: usize) -> Result<u64, PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let c = r.execution_class_header_v11(&id).ok_or_else(|| refused("unknown funded class"))?;
    crate::palw_flat_model_vector_v9::flat_model_vector_work_v9(
        r,
        &Hash64::from_bytes(c.record.candidate9_id),
        bytes.checked_add(16 << 10).ok_or_else(|| refused("native row work overflow"))?,
    )
    .ok_or_else(|| refused("original native class restore tariff absent"))
}
fn quota_key(tag: u8, id: &Digest) -> Vec<u8> {
    let mut k = vec![tag];
    k.extend_from_slice(id);
    k
}
fn read_rooted<T: borsh::BorshDeserialize + borsh::BorshSerialize>(
    r: &PalwKernelRouteStateV1,
    k: Vec<u8>,
    cap: usize,
) -> Result<Option<(Digest, T)>, PalwStateV2Error> {
    let Some(raw) = r.aux.get(&(EXECUTION_QUOTA_TABLE_V11, k)) else {
        return Ok(None);
    };
    if raw.len() != 64 + cap {
        return Err(refused("native quota row fixed framing differs"));
    }
    borsh::from_slice(raw).map(Some).map_err(|_| refused("invalid native rooted quota row"))
}
pub(super) fn quota_shard(
    builder: &TransitionBuilder<'_>,
    p: &NativeExecutionPoliciesV11,
    epoch: &TrustedEpochV11,
    actor: Digest,
    job: Digest,
) -> Result<CheckedQuotaShardV11, PalwStateV2Error> {
    quota_shard_seal(builder, p, epoch, actor, job, None)
}
pub(super) fn quota_shard_seal(
    builder: &TransitionBuilder<'_>,
    p: &NativeExecutionPoliciesV11,
    epoch: &TrustedEpochV11,
    actor: Digest,
    job: Digest,
    seal: Option<Digest>,
) -> Result<CheckedQuotaShardV11, PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let global = read_rooted::<QuotaGlobalRowV11>(r, vec![0], QUOTA_GLOBAL_ROW_BYTES_V11 as usize)?;
    let (gr, g) = match global {
        Some(v) => v,
        None => {
            let g = QuotaStateV11::new(&p.quota, epoch.clone()).map_err(refused)?.global_row();
            (g.root(), g)
        }
    };
    let a = read_rooted::<QuotaActorRowV11>(r, quota_key(1, &actor), QUOTA_SELECTED_ACTOR_ROW_BYTES_V11 as usize)?;
    let s = read_rooted::<QuotaSubjectSummaryV11>(r, quota_key(2, &job), QUOTA_SELECTED_SUBJECT_ROW_BYTES_V11 as usize)?;
    let z = match seal {
        Some(id) => read_rooted::<ProtectedSealSummaryV11>(r, quota_key(3, &id), QUOTA_SELECTED_SEAL_ROW_BYTES_V11 as usize)?,
        None => None,
    };
    CheckedQuotaShardV11::from_native_rows(
        &p.quota,
        &g,
        &gr,
        actor,
        job,
        seal,
        a.as_ref().map(|(root, row)| (row, root)),
        s.as_ref().map(|(root, row)| (row, root)),
        z.as_ref().map(|(root, row)| (row, root)),
    )
    .map_err(refused)
}
pub(super) fn write_shard(builder: &mut TransitionBuilder<'_>, shard: &CheckedQuotaShardV11, job: Digest) {
    let g = shard.global_row();
    let a = shard.actor_row();
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, vec![0], Some(key(&(g.root(), g))));
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, quota_key(1, &a.actor), Some(key(&(a.root(), a))));
    if let Some(s) = shard.subject_row() {
        builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, quota_key(2, &job), Some(key(&(s.row_root(), s))));
    }
    if let Some(s) = shard.seal_row() {
        builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, quota_key(3, &s.key.root()), Some(key(&(s.row_root(), s))));
    }
}
fn trace_row_key(role: u8, entitlement: Digest) -> Vec<u8> {
    key(&(17u8, role, entitlement))
}
fn trace_plan(builder: &TransitionBuilder<'_>, p: &NativeExecutionPoliciesV11, id: Digest) -> Result<TracePlanV17, PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let raw = r.aux.get(&(EXECUTION_JOB_TABLE_V11, key(&id))).ok_or_else(|| refused("native trace original job absent"))?;
    let (h, plan) = native_execution_job_plan_v11(raw).map_err(refused)?;
    if h.job_id != id {
        return Err(refused("native trace original job namespace"));
    }
    trace_plan_from_native_record_v17(h.class_id, &plan, &h.quota_plan_root, &p.quota).map_err(refused)
}
pub(super) fn trace_shard_from_rows(
    p: &NativeExecutionPoliciesV11,
    plan: &TracePlanV17,
    g: &QuotaGlobalRowV11,
    a: &QuotaActorRowV11,
    s: &QuotaSubjectSummaryV11,
    row: Option<(&TraceSupplementV17, &Digest)>,
) -> Result<TraceShardV17, PalwStateV2Error> {
    TraceShardV17::from_native_rows(&p.quota, plan, g, &g.root(), a.actor, Some((a, &a.root())), (s, &s.row_root()), row)
        .map_err(refused)
}
pub(super) fn trace_shard(
    builder: &TransitionBuilder<'_>,
    p: &NativeExecutionPoliciesV11,
    plan: &TracePlanV17,
    actor: Digest,
    job: Digest,
    role: u8,
    entitlement: Digest,
) -> Result<TraceShardV17, PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let g = read_rooted::<QuotaGlobalRowV11>(r, vec![0], QUOTA_GLOBAL_ROW_BYTES_V11 as usize)?
        .ok_or_else(|| refused("native trace global absent"))?;
    let a = read_rooted::<QuotaActorRowV11>(r, quota_key(1, &actor), QUOTA_SELECTED_ACTOR_ROW_BYTES_V11 as usize)?;
    let s = read_rooted::<QuotaSubjectSummaryV11>(r, quota_key(2, &job), QUOTA_SELECTED_SUBJECT_ROW_BYTES_V11 as usize)?
        .ok_or_else(|| refused("native trace subject absent"))?;
    let z = read_rooted::<TraceSupplementV17>(r, trace_row_key(role, entitlement), TRACE_ROW_BYTES_V17 as usize)?;
    TraceShardV17::from_native_rows(
        &p.quota,
        plan,
        &g.1,
        &g.0,
        actor,
        a.as_ref().map(|(root, row)| (row, root)),
        (&s.1, &s.0),
        z.as_ref().map(|(root, row)| (row, root)),
    )
    .map_err(refused)
}
pub(super) fn write_trace(builder: &mut TransitionBuilder<'_>, shard: &TraceShardV17, role: u8, entitlement: Digest) {
    let g = shard.global_row();
    let a = shard.actor_row();
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, vec![0], Some(key(&(g.root(), g))));
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, quota_key(1, &a.actor), Some(key(&(a.root(), a))));
    builder.write_kernel_row(
        EXECUTION_QUOTA_TABLE_V11,
        trace_row_key(role, entitlement),
        shard.trace_row().map(|r| key(&(r.root(), r))),
    );
}
pub(super) fn source_shard_from_rows(
    p: &NativeExecutionPoliciesV11,
    plan: &SourcePlanV16,
    g: &QuotaGlobalRowV11,
    a: &QuotaActorRowV11,
    s: &QuotaSubjectSummaryV11,
    row: Option<(&SourceObligationV16, &Digest)>,
) -> Result<SourceShardV16, PalwStateV2Error> {
    SourceShardV16::from_native_rows(&p.quota, plan, g, &g.root(), (a, &a.root()), (s, &s.row_root()), row).map_err(refused)
}
pub(super) fn write_source_shard(builder: &mut TransitionBuilder<'_>, shard: &SourceShardV16, job: Digest) {
    let g = shard.global_row();
    let a = shard.producer_row();
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, vec![0], Some(key(&(g.root(), g))));
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, quota_key(1, &a.actor), Some(key(&(a.root(), a))));
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, key(&(16u8, job)), shard.source_row().map(|r| key(&(r.root(), r))));
}
fn debit_trace_request(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    p: &NativeExecutionPoliciesV11,
    actor: Digest,
    id: Digest,
    lane: u8,
    entitlement: Digest,
) -> Result<(), PalwStateV2Error> {
    let plan = trace_plan(builder, p, id)?;
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let mut shard = trace_shard(builder, p, &plan, actor, id, lane, entitlement)?;
    if lane == 0 {
        shard.debit_ordinary(&p.quota, &epoch, &plan).map_err(refused)?;
    } else {
        shard.debit_reserved(&p.quota, &epoch, &plan, &entitlement, lane).map_err(refused)?;
    }
    write_trace(builder, &shard, lane, entitlement);
    Ok(())
}
fn bounded_ids(route: &PalwKernelRouteStateV1, table: u8, owner: &PalwBondKeyV2, max: usize) -> Result<Vec<Digest>, PalwStateV2Error> {
    bounded_ids_key(route, table, key(owner), max)
}
fn bounded_ids_key(
    route: &PalwKernelRouteStateV1,
    table: u8,
    owner_key: Vec<u8>,
    max: usize,
) -> Result<Vec<Digest>, PalwStateV2Error> {
    let Some(raw) = route.aux.get(&(table, owner_key)) else { return Ok(vec![]) };
    if raw.len() < 4 || raw.len() > 4 + max * 64 {
        return Err(refused("native owner catalog extent"));
    }
    let n = u32::from_le_bytes(raw[..4].try_into().unwrap()) as usize;
    if n > max || raw.len() != 4 + n * 64 {
        return Err(refused("native owner catalog count"));
    }
    borsh::from_slice(raw).map_err(|_| refused("native owner catalog decode"))
}
fn register(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    bytes: &[u8],
) -> Result<(), PalwStateV2Error> {
    let candidate_id = fixed_id(bytes)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let plan = r.flat_candidate_plan_v9(&Hash64::from_bytes(candidate_id)).ok_or_else(|| refused("native candidate9 unknown"))?;
    let work = plan.native_restore_work.checked_add(16 << 10).ok_or_else(|| refused("native admission work overflow"))?;
    let mut owners = bounded_ids(r, EXECUTION_CLASS_OWNER_TABLE_V11, signer, EXECUTION_CLASSES_PER_ACTOR_V11)?;
    if owners.len() >= EXECUTION_CLASSES_PER_ACTOR_V11 {
        return Err(refused("native class owner cap"));
    }
    if !prepay(builder, ctx, signer, work)? {
        return Ok(());
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let candidate = r.flat_candidate_record_v9(&Hash64::from_bytes(candidate_id)).ok_or_else(|| refused("native candidate absent"))?;
    let s = source(r, candidate.binding)?;
    let restored =
        restore_native_execution_class_header_v11(&candidate, &s, None, &p.execution, &p.quota, ctx.daa_score).map_err(refused)?;
    let c = restored.record().clone();
    if r.template().schedule.standing_at(&c.descriptor, ctx.daa_score) != misaka_palw_kernel::descriptor::KernelStandingV1::Active {
        return Err(refused("native descriptor dormant"));
    }
    if r.execution_class_header_v11(&c.id()).is_some() {
        return Err(refused("native class already registered"));
    }
    let row = NativeExecutionClassRowV11 {
        header: NativeExecutionClassJournalV11 {
            record: c.clone(),
            original_header_root: restored.header().root(),
            encoder_input_limit: restored.view().encoder.map(|e| e.l),
            owner: *signer,
            admitted_daa: ctx.daa_score,
            primitive_count: u32::try_from(restored.view().scope.relations.len()).map_err(|_| refused("native primitive count"))?,
        },
    };
    owners.push(c.id());
    builder.write_kernel_row(EXECUTION_CLASS_TABLE_V11, key(&c.id()), Some(key(&row)));
    builder.write_kernel_row(EXECUTION_CLASS_OWNER_TABLE_V11, key(signer), Some(key(&owners)));
    builder.note_kernel_bond(signer);
    Ok(())
}
fn request_class(bytes: &[u8]) -> Result<Digest, PalwStateV2Error> {
    bytes.get(2..66).ok_or_else(|| refused("truncated native request class")).and_then(fixed_id)
}
fn prepare(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    bytes: &[u8],
) -> Result<(), PalwStateV2Error> {
    let id = request_class(bytes)?;
    let work = class_work(builder, id, bytes.len())?;
    if !prepay(builder, ctx, signer, work)? {
        return Ok(());
    }
    let class = checked_class(builder, ctx, p, id, true)?;
    let request = decode_flat_execution_fund_v11(bytes, class.record().max_positions).map_err(|e| refused(e.to_string()))?;
    if request.preparation != [0; 64] {
        return Err(refused("Prepare must not name another preparation"));
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let producer = r.bond_key_of(&request.producer).ok_or_else(|| refused("native producer has never signed a route object"))?;
    if !builder.state.bonds.get(&producer).is_some_and(|b| matches!(b.status, PalwBondStatusV2::Active)) {
        return Err(refused("native producer inactive"));
    }
    let mut owners = bounded_ids(r, EXECUTION_PREPARATION_TABLE_V11, signer, EXECUTION_PREPARATIONS_PER_ACTOR_V11)?;
    if owners.len() >= EXECUTION_PREPARATIONS_PER_ACTOR_V11 {
        return Err(refused("native preparation owner cap"));
    }
    let requester = palw_kernel_bond_id_v1(signer);
    let operator = builder.state.bonds.get(signer).unwrap().operator_id.as_bytes();
    r.execution_nonce_counter_v11(&operator).map_err(refused)?.advance(&request.nonce).map_err(refused)?;
    let input = request.input(class.record().encoder).map_err(refused)?;
    let plan = derive_job_quota_plan_from_checked_native_class_v11(&class, input.positions).map_err(refused)?;
    misaka_palw_kernel::flat_quota11::discovery13::derive_discovery_plan_v13(&p.discovery, &p.quota, &plan).map_err(refused)?;
    let checked = bind_native_execution_preparation_v11(
        &class,
        &p.quota,
        input,
        plan,
        requester,
        request.producer,
        request.nonce,
        request.reward,
    )
    .map_err(refused)?;
    let row = NativeExecutionPreparationV11 {
        record: checked.record().clone(),
        plan: checked.quota_plan().record(),
        owner: *signer,
        prepared_daa: ctx.daa_score,
    };
    let prep = row.id();
    if r.aux.contains_key(&(EXECUTION_PREPARATION_TABLE_V11, key(&prep))) {
        return Err(refused("native preparation already present"));
    }
    owners.push(prep);
    builder.write_kernel_row(EXECUTION_PREPARATION_TABLE_V11, key(&prep), Some(key(&row)));
    builder.write_kernel_row(EXECUTION_PREPARATION_TABLE_V11, key(signer), Some(key(&owners)));
    builder.note_kernel_bond(signer);
    Ok(())
}
fn nonce_witness(requester: Digest, next: &NativeRequesterNonceRowV11) -> NativeRequesterNonceWitnessV11 {
    NativeRequesterNonceWitnessV11 {
        requester,
        requester_operator_id: next.operator,
        counter_root: next.root(),
        next_nonce: next.next_nonce,
    }
}
fn fund(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    bytes: &[u8],
    complete_pursuit23: bool,
    final_authority27: bool,
) -> Result<(), PalwStateV2Error> {
    let id = request_class(bytes)?;
    if !prepay(builder, ctx, signer, class_work(builder, id, bytes.len())?)? {
        return Ok(());
    }
    let class = checked_class(builder, ctx, p, id, true)?;
    let request = decode_flat_execution_fund_v11(bytes, class.record().max_positions).map_err(|e| refused(e.to_string()))?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let prep: NativeExecutionPreparationV11 = r
        .aux_row(EXECUTION_PREPARATION_TABLE_V11, &key(&request.preparation))
        .ok_or_else(|| refused("native exact preparation absent"))?;
    let requester = palw_kernel_bond_id_v1(signer);
    let producer = r.bond_key_of(&request.producer).ok_or_else(|| refused("native producer absent"))?;
    let requester_bond = builder.state.bonds.get(signer).unwrap();
    let requester_operator = requester_bond.operator_id.as_bytes();
    let requester_payout = requester_bond.payout_payload;
    let producer_bond = builder.state.bonds.get(&producer).ok_or_else(|| refused("native producer disappeared"))?;
    if !matches!(producer_bond.status, PalwBondStatusV2::Active) {
        return Err(refused("native producer is inactive"));
    }
    let producer_operator = producer_bond.operator_id.as_bytes();
    let producer_payout = producer_bond.payout_payload;
    let consent_key = key(&(13u8, request.preparation));
    let consent_bytes = r
        .aux
        .get(&(EXECUTION_PREPARATION_TABLE_V11, consent_key.clone()))
        .ok_or_else(|| refused("Fund lacks original producer-signed consent before reserving its resources"))?;
    if consent_bytes.len() > 2048 {
        return Err(refused("native producer consent fixed extent"));
    }
    let (consent_root, consent): (Digest, NativeExecutionConsentV13) =
        borsh::from_slice(consent_bytes).map_err(|_| refused("native producer consent framing"))?;
    if consent.version != 13
        || consent.root() != consent_root
        || consent.preparation != request.preparation
        || consent.class_id != id
        || consent.original_class_header_root != class.header().root()
        || consent.policies_root != p.root()
        || consent.quota_plan_root != prep.plan.root()
        || consent.requester != *signer
        || consent.requester_operator != requester_operator
        || consent.producer != producer
        || consent.producer_operator != producer_operator
        || consent.input_root != prep.record.input.root()
        || consent.nonce != request.nonce
        || consent.reward != request.reward
        || consent.accepted_daa > ctx.daa_score
        || ctx.daa_score > consent.expires_daa
        || consent.network_interval != ctx.daa_score / p.quota.interval_daa
    {
        return Err(refused("Fund differs from exact finite original producer authorization"));
    }
    let contract_raw = r
        .aux
        .get(&(EXECUTION_PREPARATION_TABLE_V11, key(&(18u8, request.preparation))))
        .ok_or_else(|| refused("Fund requires explicit original v18 producer authorization for source/trace guarantees"))?;
    if contract_raw.len() != 64 + 2 + 5 * 64 {
        return Err(refused("native v18 consent fixed framing"));
    }
    let (contract_root, contract): (Digest, NativeExecutionConsentV18) =
        borsh::from_slice(contract_raw).map_err(|_| refused("native v18 consent decode"))?;
    if !complete_pursuit23 && r.aux.contains_key(&(EXECUTION_PREPARATION_TABLE_V11, key(&(23u8, request.preparation)))) {
        return Err(refused("original producer consent23 requires Fund24; mechanics Fund2 cannot downgrade it"));
    }
    if contract.version != 18
        || contract.root() != contract_root
        || contract.original_consent13_root != consent_root
        || contract.availability_terms_root
            != availability_terms(&consent, contract.source_plan16_root, contract.trace_plan17_root, &class.record().horizons)
    {
        return Err(refused("Fund changes explicitly signed original source/trace availability contract"));
    }
    let mut consents =
        bounded_ids_key(r, EXECUTION_PREPARATION_TABLE_V11, key(&(13u8, producer)), EXECUTION_PREPARATIONS_PER_ACTOR_V11)?;
    if !consents.contains(&request.preparation) {
        return Err(refused("native producer consent owner index differs"));
    }
    consents.retain(|id| *id != request.preparation);

    let next = r.execution_nonce_counter_v11(&requester_operator).map_err(refused)?.advance(&request.nonce).map_err(refused)?;
    let input = request.input(class.record().encoder).map_err(refused)?;
    if prep.owner != *signer
        || prep.id() != request.preparation
        || prep.record.class_id != id
        || prep.record.input != input
        || prep.record.requester != requester
        || prep.record.producer != request.producer
        || prep.record.nonce != request.nonce
        || prep.record.reward != request.reward
    {
        return Err(refused("Fund differs from exact independently prepared native request"));
    }
    let mut owners = bounded_ids(r, EXECUTION_JOB_OWNER_TABLE_V11, signer, EXECUTION_JOBS_PER_REQUESTER_V11)?;
    let mut producer_jobs =
        bounded_ids_key(r, EXECUTION_JOB_OWNER_TABLE_V11, key(&(1u8, producer)), EXECUTION_JOBS_PER_REQUESTER_V11)?;
    if producer_jobs.len() >= EXECUTION_JOBS_PER_REQUESTER_V11 {
        return Err(refused("native producer job catalog full"));
    }
    let count: u32 = r.aux_row(EXECUTION_JOB_COUNT_TABLE_V11, &[]).unwrap_or(0);
    if owners.len() >= EXECUTION_JOBS_PER_REQUESTER_V11 || count >= EXECUTION_LIVE_JOBS_MAX_V11 {
        return Err(refused("native funded job catalog full"));
    }
    if requester_bond.collateral as u128 - builder.committed_at(signer, ctx.daa_score).min(requester_bond.collateral as u128)
        < request.reward as u128
    {
        return Err(refused("native original requester escrow unavailable"));
    }
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let receipt = object_id(
        b"misaka-palw/native/original-execution-escrow-debit/v11",
        &(ctx.block.as_bytes(), requester, request.preparation, request.nonce, request.reward),
    );
    let checked = CheckedExecutionPreparationV11::restore(&prep.record, &prep.plan, request.preparation).map_err(refused)?;
    let prepared =
        prepare_execution_job_from_checked_plan_v11(&class, &p.quota, &epoch, checked, request.reward, receipt).map_err(refused)?;
    let book = ExecutionJobStateV11::from_native_funded_job(prepared.job).map_err(refused)?;
    let job = book.job();
    let job_id = job.id();
    if r.execution_job_header_v11(&job_id).is_some() {
        return Err(refused("native job duplicate"));
    }
    let admission = check_native_quota_admission_v11(
        &class,
        &book,
        &NativeQuotaAdmissionAnchorV11 {
            job_id,
            funded_job_root: job.root(),
            book_root: book.root(),
            request_key: job.request_key(),
            nonce_witness: nonce_witness(requester, &next),
            terminal_book_root: None,
        },
    )
    .map_err(refused)?;
    let mut shard = quota_shard(builder, p, &epoch, request.producer, job_id)?;
    shard.admit_baseline(&p.quota, &epoch, &admission, &prepared.quota_plan).map_err(refused)?;
    let trace_plan = derive_trace_plan_v17(&class, &prepared.quota_plan, &p.quota).map_err(refused)?;
    let original_consent = misaka_palw_kernel::flat_quota11::source16::NativeSourceConsentV16 {
        job_id,
        funded_job_root: job.root(),
        book_root: book.root(),
        producer: job.producer,
        producer_operator,
        original_consent_root: contract_root,
        original_source_plan_root: contract.source_plan16_root,
        original_trace_plan_root: contract.trace_plan17_root,
        accepted_daa: consent.accepted_daa,
        expires_daa: consent.expires_daa,
    };
    let trace_admission = check_trace_producer_v17(&class, &book, &epoch, &trace_plan, &original_consent).map_err(refused)?;
    let mut trace =
        trace_shard_from_rows(p, &trace_plan, &shard.global_row(), &shard.actor_row(), shard.subject_row().unwrap(), None)?;
    trace.admit(&p.quota, &epoch, &trace_plan, &trace_admission).map_err(refused)?;
    let source_plan =
        misaka_palw_kernel::flat_quota11::source16::derive_source_plan_v16(&class, &prepared.quota_plan, &p.quota, &p.discovery)
            .map_err(refused)?;
    if source_plan.root() != contract.source_plan16_root || trace_plan.root() != contract.trace_plan17_root {
        return Err(refused("Fund supplemental quantities differ from producer-signed original v18 quote"));
    }
    let fees = delivery_fee_plan(builder, p, class.record(), &consent, source_plan.requests())?;
    if fees.root() != contract.fee_plan19_root {
        return Err(refused("Fund delivery income cap/rates differ from explicitly signed original contract"));
    }
    let capacity = if complete_pursuit23 {
        Some(capacity::plan_fund(
            builder,
            p,
            &request,
            &class,
            &prepared.quota_plan,
            job_id,
            consent_root,
            contract_root,
            final_authority27,
            job.root(),
            job.deadline_daa,
            ctx.daa_score,
        )?)
    } else {
        None
    };
    let final_admission = if final_authority27 {
        Some(final27::plan_fund(builder, ctx, p, &request, &class, &prepared.quota_plan)?)
    } else {
        if r.aux.contains_key(&(EXECUTION_PREPARATION_TABLE_V11, key(&(27u8, request.preparation)))) {
            return Err(refused("initial Final27 producer consent requires Fund26; old Fund cannot downgrade"));
        }
        None
    };
    let source_admission = misaka_palw_kernel::flat_quota11::source16::check_native_source_admission_v16(
        &class,
        &book,
        &epoch,
        &source_plan,
        &original_consent,
    )
    .map_err(refused)?;
    let mut source =
        source_shard_from_rows(p, &source_plan, trace.global_row(), trace.actor_row(), shard.subject_row().unwrap(), None)?;
    source.admit(&p.quota, &epoch, &source_plan, &source_admission).map_err(refused)?;
    let budget_plan = builder.plan_funded_execution_budget_v13(ctx, producer, &class, job)?;
    let book_bytes = key(&book);
    let row = NativeExecutionJobRowV11 {
        header: NativeExecutionJobHeaderV11 {
            version: 11,
            job_id,
            class_id: id,
            original_class_header_root: class.header().root(),
            funded_job_root: job.root(),
            book_root: book.root(),
            quota_plan_root: job.quota_plan_root,
            requester: *signer,
            requester_operator,
            requester_payout,
            producer,
            producer_operator,
            producer_payout,
            book_bytes_len: book_bytes.len() as u32,
            prompt_len: job.input.prompt_len,
            stored_post_cap: prepared.quota_plan.native_post_carrier_cap(),
            deadline_daa: job.deadline_daa,
            terminal: ExecutionTerminalV11::Funded,
            producer_reserved: 0,
        },
        book_bytes,
        quota_plan: prepared.quota_plan.record(),
        prompt: request.prompt,
    };
    let mut preparations = bounded_ids(r, EXECUTION_PREPARATION_TABLE_V11, signer, EXECUTION_PREPARATIONS_PER_ACTOR_V11)?;
    preparations.retain(|v| *v != request.preparation);
    // Every fallible subject/resource/nonce check precedes the actual original
    // debit. All writes and payouts remain one native journal transaction.
    builder.hold_native_recipient_v1(
        Hash64::from_bytes(job_id),
        producer,
        11,
        ctx.daa_score,
        row.header.deadline_daa,
        request.reward,
        &[],
    )?;
    builder.hold_native_recipient_v1(
        Hash64::from_bytes(job_id),
        producer,
        19,
        ctx.daa_score,
        row.header.deadline_daa,
        fees.maximum,
        &[],
    )?;
    if let Some(plan) = budget_plan {
        builder.commit_bond_budget_v1(ctx, super::palw_bond_budget_fold_v1::funded_execution_budget_id_v13(&job_id), plan)?;
    }
    let debit = builder.slash_bond(*signer, request.reward as u128)?;
    if debit != request.reward {
        return Err(refused("native escrow debit differs from original authorized coins"));
    }
    write_shard(builder, &shard, job_id);
    write_trace(builder, &trace, 0, job_id);
    write_source_shard(builder, &source, job_id);
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, key(&(16u8, job_id, 1u8)), Some(key(&source_plan.record())));
    if let Some(c) = final_admission {
        final27::write_fund(builder, job_id, c);
    }
    owners.push(job_id);
    producer_jobs.push(job_id);
    builder.write_kernel_row(EXECUTION_JOB_OWNER_TABLE_V11, key(&(1u8, producer)), Some(key(&producer_jobs)));
    builder.write_kernel_row(EXECUTION_REQUEST_INDEX_TABLE_V11, key(&requester_operator), Some(key(&next)));
    builder.write_kernel_row(EXECUTION_JOB_TABLE_V11, key(&job_id), Some(key(&row)));
    if let Some(admission) = capacity {
        capacity::write(builder, admission, job_id);
        builder.write_kernel_row(EXECUTION_PREPARATION_TABLE_V11, key(&(23u8, request.preparation)), None);
    }
    builder.write_kernel_row(
        EXECUTION_CREDIT_TABLE_V11,
        key(&(19u8, job_id)),
        Some(key(&NativeExecutionFeesV19 {
            version: 19,
            job: job_id,
            funded_job_root: job.root(),
            original_consent18: contract_root,
            plan: fees,
            earned: 0,
        })),
    );
    builder.write_kernel_row(EXECUTION_JOB_OWNER_TABLE_V11, key(signer), Some(key(&owners)));
    builder.write_kernel_row(EXECUTION_JOB_COUNT_TABLE_V11, vec![], Some(key(&(count + 1))));
    builder.write_kernel_row(EXECUTION_PREPARATION_TABLE_V11, key(&request.preparation), None);
    builder.write_kernel_row(EXECUTION_PREPARATION_TABLE_V11, consent_key, None);
    builder.write_kernel_row(EXECUTION_PREPARATION_TABLE_V11, key(&(18u8, request.preparation)), None);
    builder.write_kernel_row(EXECUTION_PREPARATION_TABLE_V11, key(&(13u8, producer)), Some(key(&consents)));
    builder.write_kernel_row(EXECUTION_PREPARATION_TABLE_V11, key(signer), Some(key(&preparations)));
    Ok(())
}
pub(super) fn apply(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    action: u8,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    if action >= 25
        && !builder
            .extras
            .kernel_route
            .as_ref()
            .and_then(|e| e.native_final.as_ref())
            .is_some_and(|f| f.activation.is_active(ctx.daa_score))
    {
        return Ok(());
    }
    let p = policies(builder, ctx)?;
    let result = match action {
        EXECUTION_ACTION_REGISTER_V11 => register(builder, ctx, signer, &p, body),
        EXECUTION_ACTION_PREPARE_V11 => prepare(builder, ctx, signer, &p, body),
        EXECUTION_ACTION_FUND_V11 => fund(builder, ctx, signer, &p, body, false, false),
        EXECUTION_ACTION_FUND_V23 => fund(builder, ctx, signer, &p, body, true, false),
        25 => final27::accept(builder, ctx, signer, &p, body),
        26 => fund(builder, ctx, signer, &p, body, true, true),
        EXECUTION_ACTION_COMMIT_V11 => commit(builder, ctx, signer, &p, body),
        EXECUTION_ACTION_CLOSE_V11 => close(builder, ctx, signer, &p, body),
        EXECUTION_ACTION_SEAL_V11 => court::seal(builder, ctx, signer, &p, body),
        EXECUTION_ACTION_REVEAL_V11 => court::reveal(builder, ctx, signer, &p, body),
        EXECUTION_ACTION_DEMAND_V11 => da::demand(builder, ctx, signer, &p, body),
        EXECUTION_ACTION_ANSWER_V11 => da::answer(builder, ctx, signer, &p, body),
        EXECUTION_ACTION_SETTLE_DEMAND_V11 => da::settle(builder, ctx, signer, &p, body),
        EXECUTION_ACTION_DISCOVER_V11 => discovery::admit(builder, ctx, signer, &p, body),
        EXECUTION_ACTION_CONVERT_V11 => discovery::convert(builder, ctx, signer, &p, body),
        EXECUTION_ACTION_ACCEPT_V13 => accept(builder, ctx, signer, &p, body, false),
        EXECUTION_ACTION_ACCEPT_V18 => accept(builder, ctx, signer, &p, body, true),
        EXECUTION_ACTION_ACCEPT_V23 => capacity::accept(builder, ctx, signer, &p, body),
        EXECUTION_ACTION_SOURCE_REQUEST_V16 => source::request(builder, ctx, signer, &p, body),
        EXECUTION_ACTION_SOURCE_ANSWER_V16 => source::answer(builder, ctx, signer, &p, body),
        EXECUTION_ACTION_SOURCE_SETTLE_V16 => source::settle(builder, ctx, signer, &p, body),
        EXECUTION_ACTION_EXPIRE_V11 => gc::apply(builder, ctx, signer, &p, body),
        // Unconnected protected action grammars remain closed.
        _ => Err(refused("funded execution action is not connected to native court/DA/terminal guarantees")),
    };
    // A dismissed expensive body preserves its prior native predebit. Every
    // business transition preflights all fallible checks before money/writes.
    match result {
        Err(PalwStateV2Error::KernelRouteRefused(_)) => Ok(()),
        other => other,
    }
}

fn original_job(
    builder: &TransitionBuilder<'_>,
    id: Digest,
) -> Result<
    (NativeExecutionJobRowV11, CheckedNativeExecutionClassHeaderV11, ExecutionJobStateV11, CheckedJobQuotaPlanV11),
    PalwStateV2Error,
> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let row = r.execution_job_row_v11(&id).map_err(refused)?;
    let header = r.execution_class_header_v11(&row.header.class_id).ok_or_else(|| refused("original funded class absent"))?;
    if row.header.original_class_header_root != header.original_header_root {
        return Err(refused("native job changed original class header"));
    }
    let class = header.checked_fixed_header().map_err(refused)?;
    let book = row.restore_book(&class).map_err(refused)?;
    let plan = CheckedJobQuotaPlanV11::restore(&row.quota_plan, &row.header.quota_plan_root).map_err(refused)?;
    Ok((row, class, book, plan))
}
fn pursuit(route: &PalwKernelRouteStateV1, id: &Digest) -> Result<NativeExecutionPursuitV11, PalwStateV2Error> {
    match route.aux.get(&(EXECUTION_DEMAND_COUNT_TABLE_V11, key(id))) {
        Some(raw) => NativeExecutionPursuitV11::decode(raw).map_err(refused),
        None => Ok(NativeExecutionPursuitV11::default()),
    }
}
fn commit(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    bytes: &[u8],
) -> Result<(), PalwStateV2Error> {
    let id = bytes.get(66..130).ok_or_else(|| refused("truncated native commit job")).and_then(fixed_id)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let header = r.execution_job_header_v11(&id).ok_or_else(|| refused("native commit has no funded job"))?;
    if *signer != header.producer || bytes.len() as u64 > header.stored_post_cap {
        return Err(refused("native Commit signer/carrier differs from original job"));
    }
    if !prepay(builder, ctx, signer, class_work(builder, header.class_id, bytes.len())?)? {
        return Ok(());
    }
    let (mut row, _, mut book, plan) = original_job(builder, id)?;
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let binding = book
        .job()
        .quota_subject(&builder.state.kernel_route.as_ref().unwrap().execution_class_header_v11(&header.class_id).unwrap().record)
        .map_err(refused)?;
    let mut shard = quota_shard(builder, p, &epoch, binding.producer, id)?;
    let action = plan.action(QuotaActionKindV11::Metadata, bytes).map_err(refused)?;
    shard.debit_reserved(&p.quota, &epoch, &binding, &plan, &action).map_err(refused)?;
    write_shard(builder, &shard, id); // invalid expensive post retains its attempt
    let class = checked_class(builder, ctx, p, header.class_id, false)?;
    let post = decode_flat_execution_post_v11(bytes).map_err(|e| refused(e.to_string()))?;
    checked_flat_execution_post_v11(builder.state.kernel_route.as_ref().unwrap(), &class, &book, &post).map_err(refused)?;
    let producer = builder.state.bonds.get(signer).ok_or_else(|| refused("native producer absent"))?;
    let free = (producer.collateral as u128).saturating_sub(builder.committed_at(signer, ctx.daa_score));
    if free < class.record().required_producer_reserve as u128 {
        return Err(refused("native original producer reserve unavailable"));
    }
    let trace = post.native_trace(class.record());
    let reserve_receipt = object_id(
        b"misaka-palw/native/original-funded-producer-reserve/v11",
        &(ctx.block.as_bytes(), id, trace.vector, class.record().required_producer_reserve),
    );
    book.commit(
        class.record(),
        palw_kernel_bond_id_v1(signer),
        &trace,
        ctx.daa_score,
        class.record().required_producer_reserve,
        reserve_receipt,
    )
    .map_err(refused)?;
    let claim = book.claim().unwrap();
    let state = NativeExecutionPursuitV11 { required_retained_until: claim.retention_until, ..Default::default() };
    row.replace_book(&book).map_err(refused)?;
    if builder.bond_budget_mirror_at(ctx.daa_score).is_some() {
        builder.bind_material_income_root_v1(
            ctx.daa_score,
            Hash64::from_bytes(trace.vector),
            super::palw_bond_budget_fold_v1::native_income_id_v1(Hash64::from_bytes(id), signer, 11),
        )?;
        builder.bind_material_income_root_v1(
            ctx.daa_score,
            Hash64::from_bytes(trace.vector),
            super::palw_bond_budget_fold_v1::native_income_id_v1(Hash64::from_bytes(id), signer, 19),
        )?;
    }
    builder.write_kernel_row(EXECUTION_JOB_TABLE_V11, key(&id), Some(key(&row)));
    builder.write_kernel_row(EXECUTION_PROTECTED_TABLE_V11, key(&id), Some(bytes.to_vec()));
    builder.write_kernel_row(EXECUTION_VECTOR_JOB_TABLE_V11, key(&trace.vector), Some(key(&id)));
    builder.write_kernel_row(EXECUTION_DEMAND_COUNT_TABLE_V11, key(&id), Some(key(&state)));
    Ok(())
}
fn credit(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    row: &mut NativeExecutionJobRowV11,
    book: &ExecutionJobStateV11,
    s: &ExecutionSettlementV11,
) -> Result<(), PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    if r.aux.contains_key(&(EXECUTION_CREDIT_TABLE_V11, key(&row.header.job_id)))
        || row.header.terminal != ExecutionTerminalV11::Funded && row.header.terminal != ExecutionTerminalV11::Challengeable
    {
        return Err(refused("native original escrow already credited"));
    }
    s.check().map_err(refused)?;
    if s.escrow_debit != book.job().escrow_original_debit || book.escrow_remaining() != 0 {
        return Err(refused("native terminal has no exact original escrow debit"));
    }
    row.replace_book(book).map_err(refused)?;
    let credit = NativeExecutionCreditV11 {
        job_id: row.header.job_id,
        original_funded_job_root: row.header.funded_job_root,
        terminal_book_root: book.root(),
        settlement_root: s.root(),
        requester_refund: s.requester_refund,
        producer_credit: s.producer_credit,
        collected_original_slash: s.collected_original_slash,
        finder: s.finder,
        finder_credit: s.finder_credit,
        burned: s.burned,
        established_daa: ctx.daa_score,
    };
    let original = Hash64::from_bytes(row.header.job_id);
    builder.cancel_native_recipient_v1(original, row.header.producer, 19);
    builder.bond_budget_close_v1(&super::palw_bond_budget_fold_v1::funded_execution_budget_id_v13(&row.header.job_id), ctx.daa_score);
    if s.producer_credit > 0 {
        let unique27 = s.terminal == ExecutionTerminalV11::Final
            && builder
                .state
                .kernel_route
                .as_ref()
                .unwrap()
                .aux
                .contains_key(&(EXECUTION_POLICY_TABLE_V11, key(&(27u8, row.header.job_id))));
        let queue = if unique27 {
            crate::palw_native_audit_v27::native_final_credit_queue_v27(
                row.header.job_id,
                row.header.funded_job_root,
                s.root(),
                row.header.producer_payout,
            )
        } else {
            crate::palw_kernel_route_v1::palw_kernel_payout_key_v1(&row.header.producer_payout)
        };
        if unique27 && builder.state.pending_payouts.contains_key(&queue) {
            return Err(PalwStateV2Error::BondBudgetRefused("native original unique Final27 queue already exists".into()));
        }
        builder.pay_native_recipient_v1(original, row.header.producer, 11, ctx.daa_score, s.producer_credit, queue, true)?;
        if unique27 {
            builder.write_payout(queue, Some(PalwPayoutV2 { payload: row.header.producer_payout, amount: s.producer_credit }));
        } else {
            builder.add_kernel_payout(row.header.producer_payout, s.producer_credit)?;
        }
    } else {
        builder.cancel_native_recipient_v1(original, row.header.producer, 11);
    }
    // This is original requester principal, not producer/reporter income.
    // Keep its per-job queue independent of pooled fee-income queue identities.
    if s.requester_refund > 0 {
        let mut queue = object_id(
            b"misaka-palw/native/original-principal-refund-queue/v13",
            &(row.header.job_id, row.header.funded_job_root, row.header.requester_payout),
        );
        queue[0] = crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_PAYOUT_KEY_PREFIX_V1;
        let queue = Hash64::from_bytes(queue);
        if builder.state.pending_payouts.contains_key(&queue) {
            return Err(refused("native original principal queue already occupied"));
        }
        builder.write_payout(queue, Some(PalwPayoutV2 { payload: row.header.requester_payout, amount: s.requester_refund }));
    }
    if s.finder_credit != 0 {
        let finder = s.finder.ok_or_else(|| refused("native finder credit lacks original finder"))?;
        let bond = builder
            .state
            .kernel_route
            .as_ref()
            .unwrap()
            .bond_key_of(&finder)
            .ok_or_else(|| refused("native original finder bond absent"))?;
        let payout = builder.state.bonds.get(&bond).ok_or_else(|| refused("native original finder absent"))?.payout_payload;
        if builder.params.reporter_income_active_at(ctx.daa_score) {
            let award = Hash64::from_bytes(object_id(
                b"misaka-palw/native/original-funded-court-award/v13",
                &(row.header.job_id, s.root(), finder),
            ));
            builder.stage_reporter_recipient_v1(award, bond, ctx.daa_score, s.finder_credit, original)?;
        } else {
            builder.add_kernel_payout(payout, s.finder_credit)?;
        }
    }
    builder.write_kernel_row(EXECUTION_JOB_TABLE_V11, key(&row.header.job_id), Some(key(row)));
    builder.write_kernel_row(EXECUTION_CREDIT_TABLE_V11, key(&row.header.job_id), Some(key(&credit)));
    Ok(())
}
fn close_adjudicated_quota(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    p: &NativeExecutionPoliciesV11,
    id: Digest,
    class: &CheckedNativeExecutionClassHeaderV11,
    book: &ExecutionJobStateV11,
    plan: &CheckedJobQuotaPlanV11,
    caller: PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let binding = book.job().quota_subject(class.record()).map_err(refused)?;
    let actor = if ctx.daa_score <= binding.deadline_daa { binding.producer } else { palw_kernel_bond_id_v1(&caller) };
    let mut shard = quota_shard(builder, p, &epoch, actor, id)?;
    if shard.subject_row().is_some_and(|s| s.closed) {
        return Err(refused("native original adjudicated subject already closed"));
    }
    if ctx.daa_score <= binding.deadline_daa {
        let paid = shard.debit_terminal(&p.quota, &epoch, &binding, plan, &key(&id)).map_err(refused)?;
        let row = builder
            .state
            .kernel_route
            .as_ref()
            .unwrap()
            .execution_job_header_v11(&id)
            .ok_or_else(|| refused("native terminal original job absent"))?;
        let nonce =
            builder.state.kernel_route.as_ref().unwrap().execution_nonce_counter_v11(&row.requester_operator).map_err(refused)?;
        let terminal = check_native_quota_terminal_v11(
            class,
            book,
            &NativeQuotaTerminalAnchorV11 {
                job_id: id,
                funded_job_root: row.funded_job_root,
                terminal_book_root: book.root(),
                request_key: book.job().request_key(),
                nonce_witness: nonce_witness(book.job().requester, &nonce),
            },
        )
        .map_err(refused)?;
        shard.mark_terminal_closed(&p.quota, &epoch, &terminal, &paid).map_err(refused)?;
    } else {
        let paid = shard.debit_expired_terminal(&p.quota, &epoch, &binding, plan).map_err(refused)?;
        shard.mark_expired_terminal_closed(&p.quota, &epoch, &binding, class, book, &paid).map_err(refused)?;
    }
    write_shard(builder, &shard, id);
    Ok(())
}
fn close(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    bytes: &[u8],
) -> Result<(), PalwStateV2Error> {
    let id = fixed_id(bytes)?;
    let header = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .execution_job_header_v11(&id)
        .ok_or_else(|| refused("native Close job absent"))?;
    if !prepay(builder, ctx, signer, class_work(builder, header.class_id, bytes.len())?)? {
        return Ok(());
    }
    let (mut row, class, mut book, plan) = original_job(builder, id)?;
    if book.terminal() == ExecutionTerminalV11::Challengeable {
        return final27::close(builder, ctx, signer, p, row, class, book, plan);
    }
    if book.terminal() != ExecutionTerminalV11::Funded {
        return Err(refused("native original execution is already terminal"));
    }
    if *signer != header.requester || ctx.daa_score <= book.job().commit_deadline {
        return Err(refused("native unclaimed cancellation is unauthorized or immature"));
    }
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let binding = book.job().quota_subject(class.record()).map_err(refused)?;
    let mut shard = quota_shard(builder, p, &epoch, binding.producer, id)?;
    let paid = shard.debit_terminal(&p.quota, &epoch, &binding, &plan, bytes).map_err(refused)?;
    write_shard(builder, &shard, id);
    let s = book.cancel_unclaimed(class.record(), ctx.daa_score).map_err(refused)?;
    let next =
        builder.state.kernel_route.as_ref().unwrap().execution_nonce_counter_v11(&header.requester_operator).map_err(refused)?;
    let terminal = check_native_quota_terminal_v11(
        &class,
        &book,
        &NativeQuotaTerminalAnchorV11 {
            job_id: id,
            funded_job_root: header.funded_job_root,
            terminal_book_root: book.root(),
            request_key: book.job().request_key(),
            nonce_witness: nonce_witness(book.job().requester, &next),
        },
    )
    .map_err(refused)?;
    shard.mark_terminal_closed(&p.quota, &epoch, &terminal, &paid).map_err(refused)?;
    credit(builder, ctx, &mut row, &book, &s)?;
    write_shard(builder, &shard, id);
    Ok(())
}
impl PalwChainStateV2 {
    pub fn native_execution_reserved_v11(&self, bond: &PalwBondKeyV2) -> u128 {
        let Some(r) = self.kernel_route.as_ref() else { return 0 };
        // Fixed original owner catalog, never a scan of all funded books.
        // Jobs are indexed by producer as well as requester under different
        // prefixes in the same bounded native owner table.
        let ids: Vec<Digest> = r.aux_row(EXECUTION_JOB_OWNER_TABLE_V11, &key(&(1u8, *bond))).unwrap_or_default();
        ids.into_iter()
            .take(EXECUTION_JOBS_PER_REQUESTER_V11)
            .filter_map(|id| r.execution_job_header_v11(&id))
            .filter(|h| h.producer == *bond)
            .fold(0u128, |sum, h| sum.saturating_add(h.producer_reserved as u128))
    }
}

/// New funded jobs cannot lose original candidate/program/PC through a legacy
/// owner withdrawal. A bounded fixed-prefix walk is paid before any header read.
pub(super) fn candidate_withdraw_held(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    ledger: &misaka_palw_kernel::ledger::KernelLedgerV1,
    owner: &Digest,
) -> Result<bool, PalwStateV2Error> {
    let Some(route) = builder.state.kernel_route.as_ref() else { return Ok(false) };
    let count = route
        .aux
        .range((EXECUTION_JOB_TABLE_V11, Vec::new())..(EXECUTION_JOB_TABLE_V11 + 1, Vec::new()))
        .take(EXECUTION_LIVE_JOBS_MAX_V11 as usize + 1)
        .count();
    if count == 0 {
        return Ok(false);
    }
    if count > EXECUTION_LIVE_JOBS_MAX_V11 as usize {
        return Ok(true);
    }
    let work = (count as u64).checked_mul(131072).ok_or_else(|| refused("native candidate retention tariff overflow"))?;
    if !super::palw_kernel_route_fold_v1::charge_route_budget_v1(builder, ctx, work, false)? {
        return Ok(true);
    }
    let route = builder.state.kernel_route.as_ref().unwrap();
    for ((_, job_key), bytes) in route.aux.range((EXECUTION_JOB_TABLE_V11, Vec::new())..(EXECUTION_JOB_TABLE_V11 + 1, Vec::new())) {
        let job: Digest = borsh::from_slice(job_key).map_err(|_| refused("native original job retention key"))?;
        let h = native_execution_job_header_v11(bytes).map_err(refused)?;
        if h.job_id != job {
            return Err(refused("native original job retention namespace"));
        }
        if !matches!(h.terminal, ExecutionTerminalV11::Funded | ExecutionTerminalV11::Challengeable) {
            continue;
        }
        let c = route.execution_class_header_v11(&h.class_id).ok_or_else(|| refused("native retained original class absent"))?;
        if ledger.native_candidate_owned_v9(owner, &c.record.candidate9_id) {
            return Ok(true);
        }
    }
    Ok(false)
}
impl PalwChainStateV2 {
    pub fn native_execution_demand_reserved_v11(&self, bond: &PalwBondKeyV2) -> u128 {
        let Some(r) = self.kernel_route.as_ref() else { return 0 };
        let trace = (0u8..2)
            .filter_map(|slot| r.aux_row::<Hash64>(EXECUTION_DEMAND_SLOT_TABLE_V11, &key(&(*bond, slot))))
            .filter_map(|id| r.execution_demand_v11(&id))
            .filter(|d| d.body.requester == *bond && !d.body.settled)
            .fold(0u128, |sum, d| sum.saturating_add(d.body.bond as u128));
        trace.saturating_add(source::reserved(self, bond))
    }
}

/// An original paid seal owns one group. Fixed headers and individual raw parts
/// avoid copying an expanding group under the smaller fragment tariff.
pub(super) fn apply_chunk(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    chunk: &crate::palw_kernel_route_v1::PalwKernelChunkV1,
) -> Result<(), PalwStateV2Error> {
    let result = apply_chunk_inner(builder, ctx, chunk);
    match result {
        Err(PalwStateV2Error::KernelRouteRefused(_)) => Ok(()),
        other => other,
    }
}
fn apply_chunk_inner(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    chunk: &crate::palw_kernel_route_v1::PalwKernelChunkV1,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_kernel_route_v1::{PalwKernelChunkTargetV1, palw_kernel_chunk_inner_matches_target_v1};
    let PalwKernelChunkTargetV1::FundedProofV11 { seal, actor } = chunk.target else { return Err(refused("native chunk target")) };
    let p = policies(builder, ctx)?;
    if actor != palw_kernel_bond_id_v1(&chunk.opener)
        || !builder.state.bonds.get(&chunk.opener).is_some_and(|b| matches!(b.status, PalwBondStatusV2::Active))
        || chunk.count == 0
        || chunk.count > 64
        || chunk.index >= chunk.count
        || chunk.bytes.is_empty()
        || chunk.bytes.len() > PROTECTED_FRAGMENT_BYTES_V11 as usize
    {
        return Err(refused("native proof fragment fixed actor/count/bytes"));
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let original = r.execution_seal_v11(&seal).ok_or_else(|| refused("native proof fragment no original paid seal"))?;
    if original.actor != actor
        || original.bond != chunk.opener
        || original.spent
        || ctx.daa_score < original.mature_daa
        || ctx.daa_score >= original.reveal_until
    {
        return Err(refused("native proof fragment unowned/immature/spent/late"));
    }
    let id = original.job_id;
    let (_, _, book, plan) = original_job(builder, id)?;
    if book.root() != original.original_book_root || book.terminal() != ExecutionTerminalV11::Challengeable {
        return Err(refused("native fragment original claim closed"));
    }
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let mut shard = quota_shard_seal(builder, &p, &epoch, actor, id, Some(seal))?;
    let paid = shard
        .debit_protected_fragment(&p.quota, &epoch, &plan, chunk.index as u32, chunk.count as u32, chunk.bytes.len() as u64, 512)
        .map_err(refused)?;
    write_shard(builder, &shard, id); // retained even if subsequent body fails
    let r = builder.state.kernel_route.as_ref().unwrap();
    let group_key = key(&(12u8, seal));
    let part_key = key(&(15u8, seal, chunk.index));
    let header = if let Some(g) = r.funded_proof_group_v11(&seal) {
        if g.group != chunk.group.as_bytes() || g.opener != chunk.opener || g.count != chunk.count || ctx.daa_score > g.expires_daa {
            return Err(refused("native fixed own group differs"));
        }
        g
    } else {
        if r.aux.contains_key(&(EXECUTION_SEAL_TABLE_V11, group_key.clone())) {
            return Err(refused("native fixed original group malformed"));
        }
        NativeExecutionProofGroupV11 {
            group: chunk.group.as_bytes(),
            seal,
            opener: chunk.opener,
            count: chunk.count,
            opened_daa: ctx.daa_score,
            expires_daa: original.reveal_until.saturating_sub(1),
            parts: Default::default(),
        }
    };
    if r.aux.contains_key(&(EXECUTION_SEAL_TABLE_V11, part_key.clone())) {
        return Err(refused("native original own part duplicate"));
    }
    // Each part's outer signature authenticates its exact bytes and fixed owner.
    // Only the final prepaid assembly establishes the whole original group hash.
    shard.accept_protected_fragment(&p.quota, &epoch, &plan, &paid).map_err(refused)?;
    let complete = shard.seal_row().is_some_and(|s| s.fragments.complete());
    builder.write_kernel_row(EXECUTION_SEAL_TABLE_V11, group_key.clone(), Some(key(&header)));
    builder.write_kernel_row(EXECUTION_SEAL_TABLE_V11, part_key, Some(chunk.bytes.clone()));
    write_shard(builder, &shard, id);
    if !complete {
        return Ok(());
    }
    shard.debit_protected(&p.quota, &epoch, &plan, ProtectedActionKindV11::WholeChunk, &[]).map_err(refused)?;
    write_shard(builder, &shard, id);
    let cap = derive_protected_seal_plan_v11(&plan).map_err(refused)?.assembly_cap as usize;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let mut total = 0usize;
    for i in 0..header.count {
        let raw = r
            .aux
            .get(&(EXECUTION_SEAL_TABLE_V11, key(&(15u8, seal, i))))
            .ok_or_else(|| refused("native complete group missing an original part"))?;
        if raw.len() > PROTECTED_FRAGMENT_BYTES_V11 as usize {
            return Err(refused("native stored fragment byte bound"));
        }
        total = total.checked_add(raw.len()).filter(|n| *n <= cap).ok_or_else(|| refused("native whole original group aggregate"))?;
    }
    let mut whole = Vec::with_capacity(total);
    for i in 0..header.count {
        whole.extend_from_slice(&r.aux[&(EXECUTION_SEAL_TABLE_V11, key(&(15u8, seal, i)))]);
    }
    if palw_object_chunk_group_id_v1(&whole) != chunk.group {
        return Err(refused("native prepaid group hash differs"));
    }
    let inner: PalwConsensusObjectV2 = borsh::from_slice(&whole).map_err(|_| refused("native prepaid assembled object malformed"))?;
    if !palw_kernel_chunk_inner_matches_target_v1(&inner, &chunk.target)
        || !matches!(&inner,PalwConsensusObjectV2::KernelRouteV1{signer,..} if *signer==chunk.opener)
    {
        return Err(refused("native assembled own proof actor/target differs"));
    }
    builder.write_kernel_row(EXECUTION_SEAL_TABLE_V11, group_key, None);
    for i in 0..header.count {
        builder.write_kernel_row(EXECUTION_SEAL_TABLE_V11, key(&(15u8, seal, i)), None);
    }
    // All original parts are signed by this same actual native bond. Its actor
    // is not taken from an alleged balance, proof metadata or inner signature.
    super::apply_object(builder, ctx, &inner)
}

fn accept(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    bytes: &[u8],
    contract18: bool,
) -> Result<(), PalwStateV2Error> {
    let id = fixed_id(bytes)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let raw = r.aux.get(&(EXECUTION_PREPARATION_TABLE_V11, key(&id))).ok_or_else(|| refused("native accepted preparation absent"))?;
    if raw.len() > 8192 {
        return Err(refused("native accepted preparation fixed extent"));
    }
    let prep: NativeExecutionPreparationV11 = borsh::from_slice(raw).map_err(|_| refused("native accepted preparation decode"))?;
    if prep.id() != id || prep.record.producer != palw_kernel_bond_id_v1(signer) {
        return Err(refused("only the exact original producer may consent to prepared work"));
    }
    let work = class_work(builder, prep.record.class_id, bytes.len())?;
    if !prepay(builder, ctx, signer, work)? {
        return Ok(());
    }
    let class = checked_class(builder, ctx, p, prep.record.class_id, true)?;
    let job_plan = prep.checked_plan(class.record()).map_err(refused)?;
    let contract = if contract18 {
        let source = misaka_palw_kernel::flat_quota11::source16::derive_source_plan_v16(&class, &job_plan, &p.quota, &p.discovery)
            .map_err(refused)?;
        let trace = derive_trace_plan_v17(&class, &job_plan, &p.quota).map_err(refused)?;
        Some((source.root(), trace.root()))
    } else {
        None
    };
    let r = builder.state.kernel_route.as_ref().unwrap();
    let requester = builder.state.bonds.get(&prep.owner).ok_or_else(|| refused("native consenting original requester absent"))?;
    let producer = builder.state.bonds.get(signer).unwrap();
    let requester_operator = requester.operator_id.as_bytes();
    if prep.record.requester != palw_kernel_bond_id_v1(&prep.owner)
        || !matches!(requester.status, PalwBondStatusV2::Active)
        || prep.record.class_id != class.record().id()
        || prep.prepared_daa > ctx.daa_score
    {
        return Err(refused("native consent exact original actors/class/clock"));
    }
    r.execution_nonce_counter_v11(&requester_operator).map_err(refused)?.advance(&prep.record.nonce).map_err(refused)?;
    let mut ids = bounded_ids_key(r, EXECUTION_PREPARATION_TABLE_V11, key(&(13u8, *signer)), EXECUTION_PREPARATIONS_PER_ACTOR_V11)?;
    if ids.len() >= EXECUTION_PREPARATIONS_PER_ACTOR_V11
        || ids.contains(&id)
        || r.aux.contains_key(&(EXECUTION_PREPARATION_TABLE_V11, key(&(13u8, id))))
    {
        return Err(refused("native original producer consent duplicate/catalog full"));
    }
    let expires = ctx
        .daa_score
        .checked_add(class.record().horizons.commit_window)
        .ok_or_else(|| refused("native finite consent expiry overflow"))?;
    let end = (ctx.daa_score / p.quota.interval_daa)
        .checked_add(1)
        .and_then(|n| n.checked_mul(p.quota.interval_daa))
        .ok_or_else(|| refused("native consent interval overflow"))?;
    if expires >= end {
        return Err(refused("native producer consent crosses original quota interval"));
    }
    let consent = NativeExecutionConsentV13 {
        version: 13,
        preparation: id,
        class_id: class.record().id(),
        original_class_header_root: class.header().root(),
        policies_root: p.root(),
        input_root: prep.record.input.root(),
        quota_plan_root: prep.plan.root(),
        requester: prep.owner,
        requester_operator,
        producer: *signer,
        producer_operator: producer.operator_id.as_bytes(),
        nonce: prep.record.nonce,
        reward: prep.record.reward,
        accepted_daa: ctx.daa_score,
        expires_daa: expires,
        network_interval: ctx.daa_score / p.quota.interval_daa,
    };
    ids.push(id);
    if let Some((source, trace)) = contract {
        let terms = availability_terms(&consent, source, trace, &class.record().horizons);
        let extra = NativeExecutionConsentV18 {
            version: 18,
            original_consent13_root: consent.root(),
            source_plan16_root: source,
            trace_plan17_root: trace,
            availability_terms_root: terms,
            fee_plan19_root: delivery_fee_plan(
                builder,
                p,
                class.record(),
                &consent,
                misaka_palw_kernel::flat_quota11::source16::derive_source_plan_v16(&class, &job_plan, &p.quota, &p.discovery)
                    .map_err(refused)?
                    .requests(),
            )?
            .root(),
        };
        builder.write_kernel_row(EXECUTION_PREPARATION_TABLE_V11, key(&(18u8, id)), Some(key(&(extra.root(), extra))));
    }
    builder.write_kernel_row(EXECUTION_PREPARATION_TABLE_V11, key(&(13u8, id)), Some(key(&(consent.root(), consent))));
    builder.write_kernel_row(EXECUTION_PREPARATION_TABLE_V11, key(&(13u8, *signer)), Some(key(&ids)));
    Ok(())
}
fn availability_terms(consent: &NativeExecutionConsentV13, source: Digest, trace: Digest, horizons: &ExecutionHorizonsV11) -> Digest {
    object_id(b"misaka-palw/native/original-availability-contract/v18",&(18u16,consent.root(),source,trace,horizons,
        misaka_palw_kernel::flat_quota11::source16::SOURCE_ANSWER_MAX_BYTES_V16,32u32<<10,
        b"original-PC-v3-row-column-header-hash-leaf;original-trace-v9-node-header-hash-value;producer-bound-PC-only-default;external-R-absence-no-default;fixed-owned-source-transport;caller-paid-request-restore;no-window-reset".to_vec()))
}
fn delivery_fee_plan(
    builder: &TransitionBuilder<'_>,
    p: &NativeExecutionPoliciesV11,
    class: &ExecutionClassRecordV11,
    consent: &NativeExecutionConsentV13,
    source_units: u32,
) -> Result<NativeExecutionFeePlanV19, PalwStateV2Error> {
    let policy = super::palw_kernel_route_fold_v1::route_ledger_policy_v1(builder)?;
    let trace_unit_fee =
        policy.demand_bond.max(policy.dismissal_fee_v1(crate::palw_model_artifact_v2::model_artifact_work_v2(0, 16 << 10).unwrap()));
    let source_unit_fee = policy.demand_bond.max(
        policy.dismissal_fee_v1(
            crate::palw_model_artifact_v2::model_artifact_work_v2(
                0,
                misaka_palw_kernel::flat_quota11::source16::SOURCE_ANSWER_MAX_BYTES_V16 as usize,
            )
            .unwrap(),
        ),
    );
    let trace_units = p.quota.future_attempts[2];
    let available = class
        .reward_cap
        .checked_sub(consent.reward)
        .ok_or_else(|| refused("native original delivered fee cap exceeds reward envelope"))?;
    let possible = (trace_unit_fee as u128)
        .checked_mul(trace_units as u128)
        .and_then(|n| n.checked_add((source_unit_fee as u128) * (source_units as u128)))
        .ok_or_else(|| refused("native delivery fee gain overflow"))?;
    let maximum = available.min(possible.min(u64::MAX as u128) as u64);
    let gain = (consent.reward as u128)
        + (maximum as u128)
        + (p.execution.external_gain_cap as u128)
        + (p.execution.default_obligation as u128);
    if gain > class.required_producer_reserve as u128 {
        return Err(refused("native original reserve does not cover all reward/DAfee/external/default gain"));
    }
    Ok(NativeExecutionFeePlanV19 {
        version: 19,
        class: class.id(),
        quota_plan: consent.quota_plan_root,
        original_consent13: consent.root(),
        reward: consent.reward,
        maximum,
        trace_units,
        source_units,
        trace_unit_fee,
        source_unit_fee,
    })
}
fn pay_delivery_fee(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    id: Digest,
    producer: PalwBondKeyV2,
    payout: Hash64,
    collected: u64,
    source: bool,
) -> Result<(), PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let raw = r
        .aux
        .get(&(EXECUTION_CREDIT_TABLE_V11, key(&(19u8, id))))
        .ok_or_else(|| refused("native original delivered income cap absent"))?;
    if raw.len() > 1024 {
        return Err(refused("native delivered income cap fixed extent"));
    }
    let mut row: NativeExecutionFeesV19 = borsh::from_slice(raw).map_err(|_| refused("native delivered income cap framing"))?;
    let header = r.execution_job_header_v11(&id).ok_or_else(|| refused("native fee original job absent"))?;
    if row.version != 19
        || row.job != id
        || row.funded_job_root != header.funded_job_root
        || header.producer != producer
        || header.producer_payout != payout
        || header.terminal != ExecutionTerminalV11::Challengeable
        || row.earned > row.plan.maximum
        || collected != if source { row.plan.source_unit_fee } else { row.plan.trace_unit_fee }
    {
        return Err(refused("native collected fee differs from signed original income envelope/producer"));
    }
    let credit = collected.min(row.plan.maximum - row.earned);
    if credit > 0 {
        let queue = crate::palw_kernel_route_v1::palw_kernel_payout_key_v1(&payout);
        builder.pay_native_recipient_v1(Hash64::from_bytes(id), producer, 19, ctx.daa_score, credit, queue, false)?;
        builder.add_kernel_payout(payout, credit)?;
        row.earned += credit;
    }
    // Original requester coins above the signed producer cap are burned.
    // Valid original data is accepted when the remaining fee cap is zero.
    builder.write_kernel_row(EXECUTION_CREDIT_TABLE_V11, key(&(19u8, id)), Some(key(&row)));
    Ok(())
}
