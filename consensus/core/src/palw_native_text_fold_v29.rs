//! Native original-maximum text29 journal. Every field below is selected from
//! actual authenticated rows; an ordinary carrier cannot supply a Final grant.
use super::*;
use crate::{
    palw_native_final_v27::*, palw_native_paged_carriage_v29::*, palw_native_paged_proof_v29::*, palw_native_queue_v30::*,
    palw_native_text_stop_v29::*,
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{
    flat_quota11::text29::*, flat_resources10::ResourceAxisV10 as Axis, flat_resources29::*, flat_scope25::*,
    trace::ParamCommitmentsV1,
};
use misaka_palw_tir::program::TirProgramV1;
#[path = "palw_native_text_da_fold_v29.rs"]
mod public;
fn k<T: BorshSerialize>(kind: u8, id: &T) -> Vec<u8> {
    key(&(29u8, kind, id))
}
fn read<T: BorshDeserialize>(r: &PalwKernelRouteStateV1, table: u8, kind: u8, id: &impl BorshSerialize, cap: usize) -> Option<T> {
    let raw = r.aux.get(&(table, k(kind, id)))?;
    (raw.len() <= cap).then(|| borsh::from_slice(raw).ok()).flatten()
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct ClassHeader {
    class: NativeTextClassV29,
    owner: PalwBondKeyV2,
    wire_bytes: u32,
    registered: u64,
    original_source:NativeSourceBindingV11,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct Prepared {
    wire: NativeTextPrepareV29,
    maximum: NativeTextMaximumV29,
    basis: NativeFinalClassBasisV27,
    owner: PalwBondKeyV2,
    prepared: u64,
}
impl Prepared {
    fn id(&self) -> Digest {
        object_id(b"misaka-palw/native/exact-text-preparation/v29", &(&self.wire, self.maximum.root()))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct JobHeader {
    job: Digest,
    class: Digest,
    funding: Digest,
    book: Digest,
    maximum: Digest,
    preparation: Digest,
    requester: PalwBondKeyV2,
    producer: PalwBondKeyV2,
    requester_payout: Hash64,
    producer_payout: Hash64,
    book_bytes: u32,
    post_bytes: u32,
    terminal: NativeTextTerminalV29,
    reserved: u64,
    quota_record: QuotaPlanRecordV11,
    quota_root: Digest,
}
fn original_header(r: &PalwKernelRouteStateV1, id: Digest) -> Result<JobHeader, PalwStateV2Error> {
    read(r, EXECUTION_JOB_TABLE_V11, 0, &id, 8192).ok_or_else(|| refused("text29 original funded header absent"))
}
fn original_book(r: &PalwKernelRouteStateV1, h: &JobHeader) -> Result<NativeTextBookV29, PalwStateV2Error> {
    let raw = r.aux.get(&(EXECUTION_JOB_TABLE_V11, k(1, &h.job))).ok_or_else(|| refused("text29 original book absent"))?;
    if raw.len() != h.book_bytes as usize {
        return Err(refused("text29 fixed book extent differs"));
    }
    let b = NativeTextBookV29::restore(raw, h.funding, h.book).map_err(refused)?;
    if b.funding().id() != h.job
        || b.funding().request.class != h.class
        || b.funding().maximum.root() != h.maximum
        || b.terminal() != h.terminal
    {
        return Err(refused("text29 original book namespace differs"));
    }
    Ok(b)
}
fn write_book(builder: &mut TransitionBuilder<'_>, h: &mut JobHeader, b: &NativeTextBookV29) -> Result<(), PalwStateV2Error> {
    let raw = key(b);
    h.book = b.root();
    h.book_bytes = raw.len() as u32;
    h.terminal = b.terminal();
    h.reserved = if matches!(b.terminal(), NativeTextTerminalV29::Funded | NativeTextTerminalV29::Challengeable) {
        b.funding().maximum.producer_reserve
    } else {
        0
    };
    builder.write_kernel_row(EXECUTION_JOB_TABLE_V11, k(1, &h.job), Some(raw))?;
    builder.write_kernel_row(EXECUTION_JOB_TABLE_V11, k(0, &h.job), Some(key(h)))?;
    Ok(())
}
pub(super) fn reserved(r: &PalwKernelRouteStateV1, bond: &PalwBondKeyV2) -> u128 {
    let Some(ids) = read::<Vec<Digest>>(r, EXECUTION_JOB_OWNER_TABLE_V11, 1, bond, 4 + 8 * 64) else { return 0 };
    ids.iter().take(8).filter_map(|id| original_header(r, *id).ok()).filter(|h| h.producer == *bond).map(|h| h.reserved as u128).sum()
}
fn class(builder: &TransitionBuilder<'_>, id: Digest) -> Result<(ClassHeader, NativeTextRegisterV29, TirProgramV1), PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let h: ClassHeader = read(r, EXECUTION_CLASS_TABLE_V11, 0, &id, 2048).ok_or_else(|| refused("text29 original class absent"))?;
    let raw = r.aux.get(&(EXECUTION_CLASS_TABLE_V11, k(1, &id))).ok_or_else(|| refused("text29 original class bytes absent"))?;
    if raw.len() != h.wire_bytes as usize || raw.len() > 1 << 20 {
        return Err(refused("text29 original class byte extent differs"));
    }
    let wire = decode_register(raw)?;
    let program = TirProgramV1::decode_canonical(&wire.program_bytes).map_err(|e| refused(e.to_string()))?;
    let s = h.original_source.clone();
    if h.class.id() != id
        || s.refuted
        || s.program_root != h.class.original_program
        || s.parameter_root != h.class.real_parameter
        || s.inventory_root != h.class.inventory
        || wire.real_parameters.root() != h.class.real_parameter
        || wire.maximum_positions != h.class.maximum_positions
        || wire.original_source_components != h.class.original_source_components
    {
        return Err(refused("text29 original source/program/real-PC/all-component identity differs"));
    }
    Ok((h, wire, program))
}
fn paid_restore(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    class_id: Option<Digest>,
    body: usize,
) -> Result<bool, PalwStateV2Error> {
    let wire = if let Some(id) = class_id {
        read::<ClassHeader>(builder.state.kernel_route.as_ref().unwrap(), EXECUTION_CLASS_TABLE_V11, 0, &id, 2048)
            .ok_or_else(|| refused("text29 fixed class header absent"))?
            .wire_bytes as u64
    } else {
        0
    };
    let bytes =
        wire.checked_add(body as u64).and_then(|n| n.checked_add(32768)).ok_or_else(|| refused("text29 restore extent overflow"))?;
    let work = bytes
        .checked_mul(bytes)
        .and_then(|n| n.checked_mul(64))
        .and_then(|n| n.checked_add(bytes * 1024))
        .ok_or_else(|| refused("text29 original restore tariff overflow"))?;
    if !prepay(builder, ctx, signer, work)? {
        return Ok(false);
    }
    let cost = misaka_palw_kernel::flat_quota11::stage14::bounded28::derive_stage_restore_metadata_v28(
        0,
        u32::try_from(wire).map_err(|_| refused("text29 class restore range"))?,
        8192,
        0,
        u32::try_from(body).map_err(|_| refused("text29 current restore body range"))?,
    )
    .map_err(refused)?;
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let mut q = quota_shard(builder, p, &epoch, palw_kernel_bond_id_v1(signer), [0; 64])?;
    q.debit_ordinary(&p.quota, &epoch, &cost).map_err(refused)?;
    write_shard(builder, &q, [0; 64])?;
    Ok(true)
}
fn decode_register(raw: &[u8]) -> Result<NativeTextRegisterV29, PalwStateV2Error> {
    if raw.len() < 138 || raw.len() > 1 << 20 || raw[..2] != 29u16.to_le_bytes() {
        return Err(refused("text29 bounded registration grammar"));
    }
    let n = u32::from_le_bytes(raw[134..138].try_into().unwrap()) as usize;
    if n > (1 << 20) || n > raw.len().saturating_sub(142) {
        return Err(refused("text29 original canonical program length cap"));
    }
    let at = 138 + n;
    let count = u32::from_le_bytes(raw.get(at..at + 4).ok_or_else(|| refused("text29 PC count absent"))?.try_into().unwrap()) as usize;
    if count > 65536 || count.checked_mul(66).is_none_or(|n| n > raw.len() - at - 4) {
        return Err(refused("text29 PC instance map exceeds available bounded body"));
    }
    borsh::from_slice(raw).map_err(|_| refused("text29 exact registration bytes"))
}
fn register(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    if !paid_restore(builder, ctx, signer, p, None, body.len())? {
        return Ok(());
    }
    let wire = decode_register(body)?;
    let program = TirProgramV1::decode_canonical(&wire.program_bytes).map_err(|e| refused(e.to_string()))?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let s = source(r, wire.source_binding)?;
    if s.refuted
        || ctx.daa_score < s.matures_daa
        || s.program_root != misaka_palw_kernel::public::program_root_v1(&program.encode())
        || s.parameter_root != wire.real_parameters.root()
        || wire.original_source_components
            != native_text_source_components_v29(wire.source_binding, s.program_root, s.parameter_root, s.inventory_root)
        || wire.maximum_positions == 0
        || wire.maximum_positions > program.history_bound
    {
        return Err(refused("text29 class differs from mature exact source/component/context contract"));
    }
    let resources = derive_flat_resources_v29(&program, &wire.real_parameters, wire.maximum_positions, None).map_err(refused)?;
    let record = NativeTextClassV29 {
        version: 29,
        source_binding: wire.source_binding,
        original_program: s.program_root,
        real_parameter: s.parameter_root,
        inventory: s.inventory_root,
        original_source_components: wire.original_source_components,
        maximum_positions: wire.maximum_positions,
        court_profile25: resources.court.court_set,
        maximum_scope25: resources.court.root(),
        maximum_resource29: resources.vector.root(),
    };
    let id = record.id();
    let mut ids: Vec<Digest> = read(r, EXECUTION_CLASS_OWNER_TABLE_V11, 0, signer, 4 + 8 * 64).unwrap_or_default();
    if ids.len() >= 8 || r.aux.contains_key(&(EXECUTION_CLASS_TABLE_V11, k(0, &id))) {
        return Err(refused("text29 original class or finite owner catalog exhausted"));
    }
    ids.push(id);
    let h = ClassHeader { class: record, owner: *signer, wire_bytes: body.len() as u32, registered: ctx.daa_score,original_source:s };
    builder.write_kernel_row(EXECUTION_CLASS_TABLE_V11, k(0, &id), Some(key(&h)))?;
    builder.write_kernel_row(EXECUTION_CLASS_TABLE_V11, k(1, &id), Some(body.to_vec()))?;
    builder.write_kernel_row(EXECUTION_CLASS_OWNER_TABLE_V11, k(0, signer), Some(key(&ids)))?;
    builder.note_kernel_bond(signer)?;
    Ok(())
}
struct Quote {
    original_history_bound:u32,
    maximum: CheckedNativeTextMaximumV29,
    basis: NativeFinalClassBasisV27,
    quota: CheckedTextQuotaV29,
    source: SourcePlanV16,
    paged: CheckedNativePagedProofPlanV29,
    queue: CheckedNativeQueuePlanV30,
    conditions: CheckedNativeFinalConditionsV27,
    caller: misaka_palw_kernel::flat_quota11::stage14::source26::CheckedStageSourcePlanV26,
    complete_path: u64,
    proof_window: u64,
}
fn quote(
    builder: &TransitionBuilder<'_>,
    p: &NativeExecutionPoliciesV11,
    wire: &NativeTextPrepareV29,
) -> Result<Quote, PalwStateV2Error> {
    let (h, registered, program) = class(builder, wire.request.class)?;
    if builder.state.kernel_route.as_ref().unwrap().aux.contains_key(&(EXECUTION_SOURCE_FACT_TABLE_V11,k(0,&source_relation(&h.class)))){return Err(refused("text29 exact original source namespace was independently convicted"));}
    wire.request.check_prompt(&program, &wire.prompt).map_err(refused)?;
    let e = builder.extras.kernel_route.as_ref().unwrap();
    let final_policy = &e.native_final.as_ref().ok_or_else(|| refused("text29 trusted Final27 policy absent"))?.policy;
    let service = e
        .native_carriage_service
        .as_ref()
        .filter(|s| s.implemented)
        .ok_or_else(|| refused("text29 actual reserved carriage service absent"))?;
    let mass = e.native_carriage_mass.as_ref().ok_or_else(|| refused("text29 actual physical mass absent"))?;
    let opv = &e.opv.as_ref().ok_or_else(|| refused("text29 original reference/delivery clock assumptions absent"))?.policy;
    let ledger = super::super::palw_kernel_route_fold_v1::route_ledger_policy_v1(builder)?;
    let full = derive_flat_resources_v29(&program, &registered.real_parameters, h.class.maximum_positions, None).map_err(refused)?;
    let job =
        derive_flat_resources_v29(&program, &registered.real_parameters, wire.request.maximum_positions().map_err(refused)?, None)
            .map_err(refused)?;
    let full_scope = check_flat_resource_scope_v25(&program, h.class.maximum_positions, None, &full.court)
        .map_err(|e| refused(format!("{e:?}")))?;
    let job_scope = check_flat_resource_scope_v25(&program, wire.request.maximum_positions().map_err(refused)?, None, &job.court)
        .map_err(|e| refused(format!("{e:?}")))?;
    let restore = (h.wire_bytes as u64)
        .checked_add(wire.prompt.len() as u64 * 4)
        .and_then(|n| n.checked_add(32768))
        .ok_or_else(|| refused("text29 original restore extent overflow"))?;
    let quota = derive_text_quota_v29(
        &program,
        &registered.real_parameters,
        &full_scope,
        &full.vector,
        &job_scope,
        &job.vector,
        h.class.id(),
        h.class.inventory,
        p.root(),
        restore,
    )
    .map_err(refused)?;
    let source = quota.source_plan(&program, &registered.real_parameters, &p.quota, &p.discovery).map_err(refused)?;
    let caller = misaka_palw_kernel::flat_quota11::stage14::source26::derive_text_source_caller_plan_v29(
        &program,
        &registered.real_parameters,
        &quota,
        &p.quota,
        &p.execution,
        &p.discovery,
    )
    .map_err(refused)?;
    let source_bound =
        crate::palw_native_source_proof_v25::derive_native_source_proof_bound_v25(&program, &registered.real_parameters)
            .map_err(refused)?;
    let paged = CheckedNativePagedProofPlanV29::derive(
        &full_scope,
        &full.vector,
        &p.quota,
        source_bound.maximum_bytes as u64,
        source_bound.work,
        source_bound.scratch,
        restore,
    )
    .map_err(refused)?;
    let b = &opv.budgets;
    let horizons = &p.execution.horizons;
    let turnaround = b
        .localize_daa
        .checked_add(b.carrier_daa)
        .and_then(|n| n.checked_add(b.reorg_slack_daa))
        .ok_or_else(|| refused("text29 turnaround overflow"))?;
    let sequential = (quota.requests_per_owner() as u64)
        .checked_mul(horizons.da_answer.checked_add(turnaround).ok_or_else(|| refused("text29 response clock overflow"))?)
        .ok_or_else(|| refused("text29 full acquisition clock overflow"))?;
    let proof_window = horizons.seal_ttl.max(
        (paged.record().maximum_parts as u64)
            .checked_mul(service.period_daa)
            .and_then(|n| n.checked_mul(8))
            .and_then(|n| n.checked_add(horizons.seal_maturity))
            .and_then(|n| n.checked_add(b.carrier_daa))
            .and_then(|n| n.checked_add(b.reorg_slack_daa))
            .ok_or_else(|| refused("text29 full proof transport clock overflow"))?,
    );
    let complete = [b.cold_material_daa, b.check_daa, sequential, b.localize_daa, b.court_daa, proof_window, b.reorg_slack_daa]
        .into_iter()
        .try_fold(0u64, |n, v| n.checked_add(v).ok_or_else(|| refused("text29 original complete path overflow")))?;
    if horizons.challenge_window <= complete
        || horizons.da_answer
            < b.disclose_daa.checked_add(b.carrier_daa).ok_or_else(|| refused("text29 original answer clock overflow"))?
        || p.discovery.requests_per_session < quota.requests_per_owner()
    {
        return Err(refused("text29 original public unknown-fault cold reference/acquisition/prove/carriage guarantee insufficient"));
    }
    let tail = proof_window
        .checked_add(horizons.retention)
        .and_then(|n| n.checked_add(horizons.da_answer))
        .and_then(|n| n.checked_add(horizons.proof_grace))
        .ok_or_else(|| refused("text29 original retention horizon overflow"))?;
    let horizon = horizons
        .commit_window
        .checked_add(horizons.challenge_window)
        .and_then(|n| n.checked_add(tail))
        .and_then(|n| n.checked_add(1))
        .filter(|n| *n < p.quota.interval_daa)
        .ok_or_else(|| refused("text29 entire original maximum liability exceeds one interval"))?;
    let protocol = object_id(
        b"misaka-palw/native/text-original-public-pursuit/v29",
        &(
            29u16,
            h.class.id(),
            full_scope.root(),
            job_scope.root(),
            quota.root(),
            source.root(),
            caller.root(),
            paged.record().root(),
            opv,
            complete,
            proof_window,
            tail,
        ),
    );
    let carriage = CheckedNativePagedCarriagePlanV29::derive(
        &program,
        &registered.real_parameters,
        program.params.len(),
        &paged,
        h.class.id(),
        quota.plan().record().root(),
        protocol,
        horizons.da_answer,
        proof_window - horizons.seal_maturity,
        service,
        mass,
    )
    .map_err(refused)?;
    let queue = CheckedNativeQueuePlanV30::derive_paged(&final_policy.queue_policy, mass, carriage).map_err(refused)?;
    let q = &full.vector.quantities;
    let reference_work = [
        Axis::Macs,
        Axis::ScalarSteps,
        Axis::IndexSteps,
        Axis::MetadataWork,
        Axis::ParameterHashBytes,
        Axis::TraceHashBytes,
        Axis::CopiedReferenceBytes,
        Axis::CourtWork,
    ]
    .into_iter()
    .try_fold(0u128, |n, a| n.checked_add(q.get(a)).ok_or_else(|| refused("text29 complete cold work overflow")))?;
    let acquisition = caller
        .request_cost()
        .get(Axis::MetadataWork)
        .checked_mul(caller.attempts() as u128)
        .and_then(|n| n.checked_add(paged.record().guarantee.get(Axis::MetadataWork)))
        .and_then(|n| n.checked_add(reference_work))
        .ok_or_else(|| refused("text29 complete independent monetary work overflow"))?;
    let cost = ledger
        .dismissal_fee_v1(u64::try_from(acquisition).map_err(|_| refused("text29 complete monetary work exceeds range"))?)
        .checked_add(
            ledger.demand_bond.checked_mul(caller.requests() as u64).ok_or_else(|| refused("text29 source bond cost overflow"))?,
        )
        .ok_or_else(|| refused("text29 full independent pursuit cost overflow"))?;
    let finder = u64::try_from(p.execution.required_producer_reserve as u128 * p.execution.finder_permille as u128 / 1000)
        .map_err(|_| refused("text29 finder extent overflow"))?;
    let basis = NativeFinalClassBasisV27 {
        class: h.class.id(),
        program: h.class.original_program,
        inventory: h.class.inventory,
        real_parameter: h.class.real_parameter,
        original_max_scope: full_scope.root(),
        original_max_resource: full.vector.root(),
        execution_policy: p.execution.root(),
        quota_policy: p.quota.root(),
        discovery_policy: p.discovery.root(),
        source_protocol: protocol,
        verifier_profile: full_scope.court_set(),
        physical_service: service.root(),
        complete_reference_cost_sompi: cost,
        maximum_producer_gain_sompi: p
            .execution
            .reward_cap
            .checked_add(p.execution.external_gain_cap)
            .ok_or_else(|| refused("text29 gain overflow"))?,
        original_producer_reserve_sompi: p.execution.required_producer_reserve,
        original_finder_award_sompi: finder,
        original_default_obligation_sompi: p.execution.default_obligation,
        maximum_reference_bytes: u64::try_from(q.get(Axis::OwnedPeakBytes)).map_err(|_| refused("text29 complete RAM overflow"))?,
        maximum_court_bytes: paged.record().maximum_bytes as u64,
        maximum_court_work: paged.record().court_work,
        maximum_court_scratch: paged.record().court_scratch,
    };
    let conditions =
        final_policy.check_class(p.execution.network_domain, &basis, complete, true, service.implemented).map_err(refused)?;
    let maximum = CheckedNativeTextMaximumV29::derive(
        &program,
        &registered.real_parameters,
        &wire.request,
        &wire.prompt,
        NativeTextMaximumV29 {
            version: 29,
            request: wire.request.root(),
            program: h.class.original_program,
            parameter: h.class.real_parameter,
            inventory: h.class.inventory,
            class_maximum_positions: h.class.maximum_positions,
            class_scope25: full_scope.root(),
            class_resource29: full.vector.root(),
            maximum_positions: wire.request.maximum_positions().map_err(refused)?,
            maximum_scope25: job_scope.root(),
            maximum_resource29: job.vector.root(),
            full_court_profile25: full_scope.court_set(),
            final_policy27: final_policy.root(),
            final_basis27: basis.root(),
            source_acquisition: source.root(),
            public_pursuit: protocol,
            physical_lease_plan: queue.record().original_carriage,
            original_shared_quota_plan: quota.root(),
            original_fee_plan: object_id(
                b"misaka-palw/native/text-all-source-request-fees-burned/v29",
                &(29u16, source.root(), caller.root(), ledger),
            ),
            paged_proof_plan: paged.record().root(),
            source_caller_plan: caller.root(),
            queue_policy: final_policy.queue_policy.root(),
            queue_plan: queue.record().root(),
            challenge_window: horizons.challenge_window,
            retention_tail: tail,
            commit_window: horizons.commit_window,
            complete_job_horizon: horizon,
            producer_reserve: p.execution.required_producer_reserve,
            finder_award: finder,
        },
        &basis,
        &conditions,
    )
    .map_err(refused)?;
    Ok(Quote { original_history_bound:program.history_bound,maximum, basis, quota, source, paged, queue, conditions, caller, complete_path: complete, proof_window })
}
fn prepare(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    let id: Digest = body.get(4..68).ok_or_else(|| refused("text29 preparation class prefix absent"))?.try_into().unwrap();
    if body.len() > 1 << 20 || !paid_restore(builder, ctx, signer, p, Some(id), body.len())? {
        return Ok(());
    }
    let mut cursor = std::io::Cursor::new(body);
    let version = u16::deserialize_reader(&mut cursor).map_err(|_| refused("text29 preparation version"))?;
    let request = NativeTextRequestV29::deserialize_reader(&mut cursor).map_err(|_| refused("text29 fixed request grammar"))?;
    let count = u32::deserialize_reader(&mut cursor).map_err(|_| refused("text29 prompt count absent"))? as usize;
    if count != request.prompt_len as usize || count * 4 != body.len().saturating_sub(cursor.position() as usize) {
        return Err(refused("text29 bounded original prompt extent differs"));
    }
    let wire: NativeTextPrepareV29 = borsh::from_slice(body).map_err(|_| refused("text29 exact preparation grammar"))?;
    let requester = builder.state.bonds.get(signer).ok_or_else(|| refused("text29 requester absent"))?;
    if version != 29
        || wire.request.class != id
        || wire.request.requester != palw_kernel_bond_id_v1(signer)
        || wire.request.requester_operator != requester.operator_id.as_bytes()
        || !matches!(requester.status, PalwBondStatusV2::Active)
        || wire.request.reward > p.execution.reward_cap
    {
        return Err(refused("text29 preparation signer/requester/principal differs"));
    }
    let producer_key = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .bond_key_of(&wire.request.producer)
        .ok_or_else(|| refused("text29 original producer absent"))?;
    let producer = builder.state.bonds.get(&producer_key).ok_or_else(|| refused("text29 producer absent"))?;
    if !matches!(producer.status, PalwBondStatusV2::Active) || wire.request.producer_operator != producer.operator_id.as_bytes() {
        return Err(refused("text29 original live producer operator differs"));
    }
    let q = quote(builder, p, &wire)?;
    let prepared = Prepared { wire, maximum: q.maximum.record().clone(), basis: q.basis, owner: *signer, prepared: ctx.daa_score };
    let id = prepared.id();
    let r = builder.state.kernel_route.as_ref().unwrap();
    let mut ids: Vec<Digest> = read(r, EXECUTION_PREPARATION_TABLE_V11, 0, signer, 4 + 8 * 64).unwrap_or_default();
    if ids.len() >= 8 || r.aux.contains_key(&(EXECUTION_PREPARATION_TABLE_V11, k(1, &id))) {
        return Err(refused("text29 finite own preparation catalog or exact preparation exists"));
    }
    ids.push(id);
    builder.write_kernel_row(EXECUTION_PREPARATION_TABLE_V11, k(1, &id), Some(key(&prepared)))?;
    builder.write_kernel_row(EXECUTION_PREPARATION_TABLE_V11, k(0, signer), Some(key(&ids)))?;
    Ok(())
}
fn accept(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    if body.len() > 4096 {
        return Err(refused("text29 fixed original consent cap"));
    }
    let a: NativeTextConsentV29 = borsh::from_slice(body).map_err(|_| refused("text29 fixed original consent grammar"))?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let prep: Prepared = read(r, EXECUTION_PREPARATION_TABLE_V11, 1, &a.preparation, (1 << 20) + 8192)
        .ok_or_else(|| refused("text29 original preparation absent"))?;
    if !paid_restore(builder, ctx, signer, p, Some(prep.wire.request.class), body.len() + prep.wire.prompt.len() * 4)? {
        return Ok(());
    }
    let q = quote(builder, p, &prep.wire)?;
    let request = &prep.wire.request;
    let producer = builder.state.bonds.get(signer).ok_or_else(|| refused("text29 original producer absent"))?;
    if a.version != 29
        || a.preparation != prep.id()
        || a.request != request.root()
        || a.maximum != *q.maximum.record()
        || a.producer != palw_kernel_bond_id_v1(signer)
        || a.producer != request.producer
        || a.producer_operator != producer.operator_id.as_bytes()
        || a.producer_operator != request.producer_operator
        || a.requester != request.requester
        || a.requester_operator != request.requester_operator
        || a.nonce != request.nonce
        || a.accepted_daa != ctx.daa_score
        || a.expires_daa < ctx.daa_score
        || a.expires_daa
            > ctx.daa_score.checked_add(a.maximum.commit_window).ok_or_else(|| refused("text29 consent expiry overflow"))?
        || a.admitted_interval != ctx.daa_score / p.quota.interval_daa
        || a.expires_daa / p.quota.interval_daa != a.admitted_interval
        || a.public_protocol != a.maximum.public_pursuit
        || a.queue_policy != a.maximum.queue_policy
        || a.queue_plan != a.maximum.queue_plan
        || !matches!(producer.status, PalwBondStatusV2::Active)
    {
        return Err(refused(
            "text29 original producer consent changes exact input/max/source/quota/fee/temporal/physical/Final roots",
        ));
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let mut ids: Vec<Digest> = read(r, EXECUTION_PREPARATION_TABLE_V11, 2, signer, 4 + 8 * 64).unwrap_or_default();
    ids.retain(|id| {
        read::<NativeTextConsentV29>(r, EXECUTION_PREPARATION_TABLE_V11, 3, id, 4096).is_some_and(|a| a.expires_daa >= ctx.daa_score)
    });
    if ids.len() >= 8 || r.aux.contains_key(&(EXECUTION_PREPARATION_TABLE_V11, k(3, &a.root()))) {
        return Err(refused("text29 producer-only finite consent catalog exhausted"));
    }
    ids.push(a.root());
    builder.write_kernel_row(EXECUTION_PREPARATION_TABLE_V11, k(3, &a.root()), Some(key(&a)))?;
    builder.write_kernel_row(EXECUTION_PREPARATION_TABLE_V11, k(2, signer), Some(key(&ids)))?;
    Ok(())
}
fn fund(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    if body.len() != 130 {
        return Err(refused("text29 exact Fund grammar"));
    }
    let request: NativeTextFundV29 = borsh::from_slice(body).map_err(|_| refused("text29 Fund metadata"))?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let prep: Prepared = read(r, EXECUTION_PREPARATION_TABLE_V11, 1, &request.preparation, (1 << 20) + 8192)
        .ok_or_else(|| refused("text29 exact original preparation absent"))?;
    if !paid_restore(builder, ctx, signer, p, Some(prep.wire.request.class), body.len() + prep.wire.prompt.len() * 4)? {
        return Ok(());
    }
    let q = quote(builder, p, &prep.wire)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let consent: NativeTextConsentV29 = read(r, EXECUTION_PREPARATION_TABLE_V11, 3, &request.initial_consent, 4096)
        .ok_or_else(|| refused("text29 original producer signature before any reservation absent"))?;
    let w = &prep.wire.request;
    let requester = builder.state.bonds.get(signer).ok_or_else(|| refused("text29 requester absent"))?;
    let producer_key = r.bond_key_of(&w.producer).ok_or_else(|| refused("text29 original producer absent"))?;
    let producer = builder.state.bonds.get(&producer_key).ok_or_else(|| refused("text29 producer disappeared"))?;
    if request.version != 29
        || request.preparation != prep.id()
        || consent.root() != request.initial_consent
        || consent.preparation != prep.id()
        || consent.maximum != *q.maximum.record()
        || consent.request != w.root()
        || consent.nonce != w.nonce
        || consent.producer != w.producer
        || consent.producer_operator != producer.operator_id.as_bytes()
        || consent.producer_operator != w.producer_operator
        || consent.requester != palw_kernel_bond_id_v1(signer)
        || consent.requester != w.requester
        || consent.requester_operator != requester.operator_id.as_bytes()
        || consent.requester_operator != w.requester_operator
        || ctx.daa_score < consent.accepted_daa
        || ctx.daa_score > consent.expires_daa
        || consent.admitted_interval != ctx.daa_score / p.quota.interval_daa
        || !matches!(producer.status, PalwBondStatusV2::Active)
        || !matches!(requester.status, PalwBondStatusV2::Active)
        || r.aux.contains_key(&(EXECUTION_CREDIT_TABLE_V11, k(3, &request.initial_consent)))
    {
        return Err(refused(
            "text29 Fund changes original signed source/max/economics/principal/operator/nonce/time or consumed consent",
        ));
    }
    let next = r.execution_nonce_counter_v11(&w.requester_operator).map_err(refused)?.advance(&w.nonce).map_err(refused)?;
    let requester_payout = requester.payout_payload;
    let producer_payout = producer.payout_payload;
    if (requester.collateral as u128).saturating_sub(builder.committed_at(signer, ctx.daa_score)) < w.reward as u128
        || (producer.collateral as u128).saturating_sub(builder.committed_at(&producer_key, ctx.daa_score))
            < q.maximum.record().producer_reserve as u128
    {
        return Err(refused("text29 original actual escrow or collectible producer reserve unavailable"));
    }
    let deadline = ctx
        .daa_score
        .checked_add(q.maximum.record().complete_job_horizon)
        .filter(|d| *d < (ctx.daa_score / p.quota.interval_daa + 1) * p.quota.interval_daa)
        .ok_or_else(|| refused("text29 complete original liability crosses interval"))?;
    let escrow = object_id(
        b"misaka-palw/native/text-actual-original-escrow/v29",
        &(ctx.block.as_bytes(), *signer, w.root(), consent.root(), w.reward),
    );
    let funded = NativeTextFundingV29 {
        request: w.clone(),
        maximum: q.maximum.record().clone(),
        consent: consent.root(),
        escrow_debit: escrow,
        maximum_reservation: object_id(
            b"misaka-palw/native/text-original-maximum-reservation/v29",
            &(q.quota.root(), ctx.block.as_bytes(), w.root()),
        ),
        original_shared_subject: object_id(b"misaka-palw/native/text-original-shared-subject/v29", &(w.root(), q.quota.root())),
        admitted_daa: ctx.daa_score,
        admitted_interval: ctx.daa_score / p.quota.interval_daa,
        commit_deadline: ctx
            .daa_score
            .checked_add(q.maximum.record().commit_window)
            .ok_or_else(|| refused("text29 Commit clock overflow"))?,
        deadline,
    };
    let book = NativeTextBookV29::from_native_funding(&q.maximum, funded.clone()).map_err(refused)?;
    let id = funded.id();
    let mut requester_ids: Vec<Digest> = read(r, EXECUTION_JOB_OWNER_TABLE_V11, 0, signer, 4 + 8 * 64).unwrap_or_default();
    let mut producer_ids: Vec<Digest> = read(r, EXECUTION_JOB_OWNER_TABLE_V11, 1, &producer_key, 4 + 8 * 64).unwrap_or_default();
    let mut consents: Vec<Digest> = read(r, EXECUTION_PREPARATION_TABLE_V11, 2, &producer_key, 4 + 8 * 64)
        .ok_or_else(|| refused("text29 original consent owner catalog absent"))?;
    let mut preparations: Vec<Digest> = read(r, EXECUTION_PREPARATION_TABLE_V11, 0, signer, 4 + 8 * 64)
        .ok_or_else(|| refused("text29 original preparation owner catalog absent"))?;
    if requester_ids.len() >= 8
        || producer_ids.len() >= 8
        || !consents.contains(&consent.root())
        || !preparations.contains(&prep.id())
        || r.aux.contains_key(&(EXECUTION_JOB_TABLE_V11, k(0, &id)))
    {
        return Err(refused("text29 finite original job slots/consent/preparation membership differs"));
    }
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let nonce = nonce_witness(w.requester, &next);
    let anchor = TextNativeFundingAnchorV29 {
        class: w.class,
        source: q.quota.source(),
        maximum: funded.maximum.maximum_resource29,
        request: w.root(),
        job: id,
        funding: funded.root(),
        consent: consent.root(),
        requester: w.requester,
        producer: w.producer,
        requester_operator: w.requester_operator,
        producer_operator: w.producer_operator,
        nonce: w.nonce,
        admitted: ctx.daa_score,
        deadline,
        reward: w.reward,
        debit: w.reward,
        receipt: escrow,
    };
    let grant = check_text_quota_admission_v29(&q.quota, &anchor, &p.quota, &nonce, &epoch).map_err(refused)?;
    let mut baseline = quota_shard(builder, p, &epoch, w.producer, id)?;
    baseline.admit_baseline(&p.quota, &epoch, &grant, q.quota.plan()).map_err(refused)?;
    let source_grant = check_text_source_admission_v29(&q.quota, &anchor, &q.source, &p.quota, &nonce, &epoch).map_err(refused)?;
    let mut source =
        source_shard_from_rows(p, &q.source, &baseline.global_row(), &baseline.actor_row(), baseline.subject_row().unwrap(), None)?;
    source.admit(&p.quota, &epoch, &q.source, &source_grant).map_err(refused)?;
    let e = builder.extras.kernel_route.as_ref().unwrap();
    let service = e.native_carriage_service.as_ref().unwrap();
    let mass = e.native_carriage_mass.as_ref().unwrap();
    let qp = &e.native_final.as_ref().unwrap().policy.queue_policy;
    let mut physical = match r.aux_row::<(Digest, crate::palw_flat_execution_capacity_v22::NativeCarriageGlobalV22)>(
        EXECUTION_QUOTA_TABLE_V11,
        &key(&(22u8, 0u8)),
    ) {
        Some((root, g)) if root == g.root() => g,
        Some(_) => return Err(refused("text29 physical global root differs")),
        None => crate::palw_flat_execution_capacity_v22::NativeCarriageGlobalV22::empty(service),
    };
    let mut queue = match r.aux.get(&(EXECUTION_QUOTA_TABLE_V11, key(&(30u8, 0u8)))) {
        None => NativeQueueGlobalV30::empty(qp, service).map_err(refused)?,
        Some(raw) => {
            if raw.len() > NATIVE_QUEUE_ROW_LIMIT_V30 {
                return Err(refused("text29 queue global cap"));
            }
            let (root, g): (Digest, NativeQueueGlobalV30) = borsh::from_slice(raw).map_err(|_| refused("text29 queue grammar"))?;
            if root != g.root() {
                return Err(refused("text29 queue global root differs"));
            }
            g
        }
    };
    let (queue_lease, lease) = queue
        .admit(&mut physical, qp, service, mass, &q.queue, id, funded.root(), consent.root(), ctx.daa_score, deadline)
        .map_err(refused)?;
    let lease_journal = NativePagedLeaseJournalV29 {
        version: 29,
        namespace: 29,
        job: id,
        funding: funded.root(),
        maximum: funded.maximum.root(),
        service: service.root(),
        lease_root: lease.root(),
        lease: lease.clone(),
        proof_envelope: q.queue.paged_carriage().unwrap().record().proof_envelope.clone(),
    };
    let token = CheckedNativeTextAdmissionV29::from_current_native(
        ctx.block.as_bytes(),
        ctx.daa_score,
        &q.maximum,
        &funded,
        next.root(),
        object_id(b"misaka-palw/native/text-current-paid-restore/v29", &(ctx.block.as_bytes(), *signer, prep.id(), body)),
    )
    .map_err(refused)?;
    let budget = builder.plan_native_text_budget_v29(ctx, producer_key, &q.maximum, &funded, &token)?;
    builder.hold_native_recipient_v1(Hash64::from_bytes(id), producer_key, 11, ctx.daa_score, deadline, w.reward, &[])?;
    if let Some(b) = budget {
        builder.commit_bond_budget_v1(ctx, super::super::palw_bond_budget_fold_v1::native_text_budget_id_v29(&id), b)?;
    }
    if builder.slash_bond(*signer, w.reward as u128)? != w.reward {
        return Err(PalwStateV2Error::BondBudgetRefused("text29 original authorized escrow changed after full preflight".into()));
    }
    write_shard(builder, &baseline, id)?;
    write_source_shard(builder, &source, id)?;
    for (kk, raw) in [
        (key(&(16u8, id, 1u8)), key(&q.source.record())),
        (key(&(26u8, id, 0u8)), key(q.caller.record())),
        (key(&(29u8, id, 23u8)), {
            let c = q.queue.paged_carriage().unwrap().record();
            key(&(c.root(), c))
        }),
        (key(&(29u8, id, 1u8)), key(q.paged.record())),
        (key(&(29u8, id, 3u8)), key(&p.quota)),
        (key(&(29u8, id, 22u8)), key(&(lease_journal.root(), lease_journal))),
        (key(&(30u8, id, 1u8)), key(&(queue_lease.root(), queue_lease))),
        (key(&(30u8, 0u8)), key(&(queue.root(), queue))),
        (key(&(22u8, 0u8)), key(&(physical.root(), physical))),
        (key(&(22u8, id)), key(&(lease.root(), lease))),
        (key(&(22u8, id, 2u8)), {
            let slots = crate::palw_flat_execution_capacity_v22::NativePhysicalSlotsV22::empty(id, consent.root());
            key(&(slots.root(), slots))
        }),
    ] {
        builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, kk, Some(raw))?;
    }
    let mut h = JobHeader {
        job: id,
        class: w.class,
        funding: funded.root(),
        book: book.root(),
        maximum: funded.maximum.root(),
        preparation: prep.id(),
        requester: *signer,
        producer: producer_key,
        requester_payout,
        producer_payout,
        book_bytes: 0,
        post_bytes: 0,
        terminal: NativeTextTerminalV29::Funded,
        reserved: funded.maximum.producer_reserve,
        quota_record: q.quota.plan().record(),
        quota_root: q.quota.plan().record().root(),
    };
    write_book(builder, &mut h, &book)?;
    requester_ids.push(id);
    producer_ids.push(id);
    consents.retain(|v| *v != consent.root());
    preparations.retain(|v| *v != prep.id());
    builder.write_kernel_row(EXECUTION_REQUEST_INDEX_TABLE_V11, key(&w.requester_operator), Some(key(&next)))?;
    builder.write_kernel_row(EXECUTION_JOB_OWNER_TABLE_V11, k(0, signer), Some(key(&requester_ids)))?;
    builder.write_kernel_row(EXECUTION_JOB_OWNER_TABLE_V11, k(1, &producer_key), Some(key(&producer_ids)))?;
    builder.write_kernel_row(EXECUTION_PREPARATION_TABLE_V11, k(2, &producer_key), Some(key(&consents)))?;
    builder.write_kernel_row(EXECUTION_PREPARATION_TABLE_V11, k(0, signer), Some(key(&preparations)))?;
    builder.write_kernel_row(EXECUTION_PREPARATION_TABLE_V11, k(3, &consent.root()), None)?;
    builder.write_kernel_row(EXECUTION_CREDIT_TABLE_V11, k(3, &consent.root()), Some(key(&(id, funded.root(), consent.root()))))?;
    builder.write_kernel_row(EXECUTION_POLICY_TABLE_V11, k(0, &id), Some(key(&prep)))?;
    builder.note_kernel_bond(&producer_key)?;
    Ok(())
}
fn cached_subject(
    r: &PalwKernelRouteStateV1,
    h: &JobHeader,
    p: &NativeExecutionPoliciesV11,
) -> Result<(CheckedJobQuotaPlanV11, SubjectBindingV11), PalwStateV2Error> {
    let plan = CheckedJobQuotaPlanV11::restore(&h.quota_record, &h.quota_root).map_err(refused)?;
    let (root, s): (Digest, QuotaSubjectSummaryV11) =
        r.aux_row(EXECUTION_QUOTA_TABLE_V11, &quota_key(2, &h.job)).ok_or_else(|| refused("text29 original shared subject absent"))?;
    if s.row_root() != root
        || s.binding.policy_root != p.quota.root()
        || s.binding.job_id != h.job
        || s.binding.quota_plan_root != plan.record().root()
        || s.binding.class_id != h.class
        || s.binding.producer != palw_kernel_bond_id_v1(&h.producer)
    {
        return Err(refused("text29 original checked shared subject/cache differs"));
    }
    Ok((plan, s.binding))
}
fn decode_post(raw: &[u8], max_gen: u32, max_positions: u32) -> Result<NativeTextPostV29, PalwStateV2Error> {
    if raw.len() < 266 || raw.len() > 1 << 20 || raw[..2] != 29u16.to_le_bytes() {
        return Err(refused("text29 stopped post fixed grammar"));
    }
    let count = u32::from_le_bytes(raw[258..262].try_into().unwrap());
    if count == 0 || count > max_gen || count as usize * 4 > raw.len() - 262 {
        return Err(refused("text29 stopped post bounded generated count"));
    }
    let mut c = std::io::Cursor::new(raw);
    c.set_position((262 + count as usize * 4 + 128) as u64);
    let _: misaka_palw_kernel::seg::SegmentedEvidenceV2 =
        BorshDeserialize::deserialize_reader(&mut c).map_err(|_| refused("text29 fixed evidence grammar"))?;
    let segments = u32::deserialize_reader(&mut c).map_err(|_| refused("text29 segment count absent"))?;
    if segments > max_positions.div_ceil(misaka_palw_kernel::seg::SEG_LEN_V4)
        || raw.len() != c.position() as usize + segments as usize * 64 + 64
    {
        return Err(refused("text29 stopped post exact bounded directory extent"));
    }
    borsh::from_slice(raw).map_err(|_| refused("text29 exact stopped post bytes"))
}
fn commit(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    let id: Digest = body.get(2..66).ok_or_else(|| refused("text29 Commit original job prefix"))?.try_into().unwrap();
    let r = builder.state.kernel_route.as_ref().unwrap();
    let mut h = original_header(r, id)?;
    if h.producer != *signer || h.terminal != NativeTextTerminalV29::Funded {
        return Err(refused("text29 Commit does not own original funded producer"));
    }
    let (plan, binding) = cached_subject(r, &h, p)?;
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let mut shard = quota_shard(builder, p, &epoch, palw_kernel_bond_id_v1(signer), id)?;
    let action = plan.action(QuotaActionKindV11::Metadata, body).map_err(refused)?;
    shard.debit_reserved(&p.quota, &epoch, &binding, &plan, &action).map_err(refused)?;
    write_shard(builder, &shard, id)?; // BEFORE every body/class/max/actual scope decode and restore.
    let r = builder.state.kernel_route.as_ref().unwrap();
    let mut book = original_book(r, &h)?;
    let prep: Prepared = read(r, EXECUTION_POLICY_TABLE_V11, 0, &id, (1 << 20) + 8192)
        .ok_or_else(|| refused("text29 original maximum preparation absent"))?;
    let q = quote(builder, p, &prep.wire)?;
    let (_, registered, program) = class(builder, h.class)?;
    let post = decode_post(body, book.funding().request.maximum_new_tokens, book.funding().maximum.maximum_positions)?;
    let b = book.funding().clone();
    if q.maximum.record() != &b.maximum
        || q.quota.plan().record().root() != h.quota_root
        || post.job != id
        || post.original_funding != b.root()
        || post.maximum_contract != b.maximum.root()
        || post.requester_nonce != b.request.nonce
        || builder.state.bonds.get(signer).is_none_or(|v| v.operator_id.as_bytes() != b.request.producer_operator)
        || post.evidence.header.network_domain != p.execution.network_domain
        || post.evidence.header.program_root != h_original_program(&registered)
        || post.evidence.header.artifact_root != b.maximum.parameter
        || post.evidence.header.class_binding_id != h.class
        || post.evidence.header.plan_root != b.maximum.maximum_resource29
        || post.evidence.header.ruleset_digest != b.maximum.full_court_profile25
    {
        return Err(refused("text29 stopped post rebinds original producer/nonce/max/source/evidence identity"));
    }
    let actual = q.maximum.check_actual(&program, &registered.real_parameters, &b.request, &post.generated).map_err(refused)?;
    check_bound_window_lengths(r,&h,program.history_bound,actual.positions())?;
    if actual.scope_root() != post.actual_scope25
        || actual.resource_root() != post.actual_resource29
        || actual.positions() != post.evidence.positions
        || post.evidence.job_input_root
            != misaka_palw_kernel::seg::job_input_root_v2(
                b.request.prompt_len,
                &misaka_palw_kernel::seg::prompt_root_of_ids_v1(&prep.wire.prompt),
                &post.generated[..post.generated.len() - 1],
            )
    {
        return Err(refused("text29 actual EOS/count/position/input/resources differ"));
    }
    post.evidence.check_structure(&post.segment_roots).map_err(refused)?;
    book.commit(
        &actual,
        palw_kernel_bond_id_v1(signer),
        post.vector(),
        post.trace_root(),
        post.final_state,
        ctx.daa_score,
        &post.segment_roots,
        post.evidence.claim_root,
    )
    .map_err(refused)?;
    h.post_bytes = body.len() as u32;
    write_book(builder, &mut h, &book)?;
    builder.write_kernel_row(EXECUTION_PROTECTED_TABLE_V11, k(0, &id), Some(body.to_vec()))?;
    let claim = book.claim().unwrap();
    let status = NativePagedJobStatusV29 {
        version: 29,
        namespace: 29,
        job: id,
        funding: h.funding,
        claim: claim.root(),
        maximum: h.maximum,
        paid_plan: b.maximum.paged_proof_plan,
        parameter: b.maximum.parameter,
        inventory: b.maximum.inventory,
        active: true,
        paid_owners: [[0; 64]; 4],
        owners_admitted: 0,
        latest_paid_horizon: 0,
        admitted_interval: b.admitted_interval,
        deadline: b.deadline,
        challenge_end: claim.challenge_end,
    };
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, key(&(29u8, id, 0u8)), Some(key(&(status.root(), status))))?;
    Ok(())
}
fn h_original_program(w: &NativeTextRegisterV29) -> Digest {
    misaka_palw_kernel::public::program_root_v1(&w.program_bytes)
}
fn funded_anchor(b: &NativeTextBookV29, q: &CheckedTextQuotaV29) -> TextNativeFundingAnchorV29 {
    let f = b.funding();
    TextNativeFundingAnchorV29 {
        class: f.request.class,
        source: q.source(),
        maximum: f.maximum.maximum_resource29,
        request: f.request.root(),
        job: f.id(),
        funding: f.root(),
        consent: f.consent,
        requester: f.request.requester,
        producer: f.request.producer,
        requester_operator: f.request.requester_operator,
        producer_operator: f.request.producer_operator,
        nonce: f.request.nonce,
        admitted: f.admitted_daa,
        deadline: f.deadline,
        reward: f.request.reward,
        debit: f.request.reward,
        receipt: f.escrow_debit,
    }
}
fn paid_status(r: &PalwKernelRouteStateV1, id: Digest) -> Result<NativePagedJobStatusV29, PalwStateV2Error> {
    let raw =
        r.aux.get(&(EXECUTION_QUOTA_TABLE_V11, key(&(29u8, id, 0u8)))).ok_or_else(|| refused("text29 original paid status absent"))?;
    if raw.len() > 2048 {
        return Err(refused("text29 paid status fixed extent"));
    }
    let (root, s): (Digest, NativePagedJobStatusV29) = borsh::from_slice(raw).map_err(|_| refused("text29 paid status grammar"))?;
    if s.root() != root || s.namespace != 29 || s.job != id || s.owners_admitted > 4 {
        return Err(refused("text29 original paid status root/namespace differs"));
    }
    Ok(s)
}
fn write_caller(
    builder: &mut TransitionBuilder<'_>,
    s: &misaka_palw_kernel::flat_quota11::stage14::source26::StageSourceCallerShardV26,
) -> Result<(), PalwStateV2Error> {
    let g = s.global_row();
    let a = s.actor_row();
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, vec![0], Some(key(&(g.root(), g))))?;
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, quota_key(1, &a.actor), Some(key(&(a.root(), a))))?;
    if let Some(row) = s.caller_row() {
        builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, key(&(26u8, row.job, row.owner)), Some(key(&(row.root(), row))))?;
    }
    Ok(())
}
fn own(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    raw: &[u8],
) -> Result<(), PalwStateV2Error> {
    use crate::palw_native_paged_quota_v29::*;
    use misaka_palw_kernel::flat_quota11::stage14::source26::*;
    if raw.len() != 263 {
        return Err(refused("text29 exact paid owner grammar"));
    }
    let id: Digest = raw[2..66].try_into().unwrap();
    let r = builder.state.kernel_route.as_ref().unwrap();
    let h = original_header(r, id)?;
    if !paid_restore(builder, ctx, signer, p, Some(h.class), raw.len())? {
        return Ok(());
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let b = original_book(r, &h)?;
    let f = b.funding();
    let claim = b.claim().ok_or_else(|| refused("text29 paid owner requires committed claim"))?;
    let prep: Prepared =
        read(r, EXECUTION_POLICY_TABLE_V11, 0, &id, (1 << 20) + 8192).ok_or_else(|| refused("text29 original preparation absent"))?;
    let q = quote(builder, p, &prep.wire)?;
    let (_, registered, program) = class(builder, h.class)?;
    let mut wire: NativeTextOwnV29 = borsh::from_slice(raw).map_err(|_| refused("text29 original owner bytes"))?;
    let actor = palw_kernel_bond_id_v1(signer);
    let bond = builder.state.bonds.get(signer).ok_or_else(|| refused("text29 paid owner bond absent"))?;
    let operator = bond.operator_id.as_bytes();
    let nonce_key = key(&(29u8, operator));
    let next: u64 = r.aux_row(EXECUTION_REQUEST_INDEX_TABLE_V11, &nonce_key).unwrap_or(0);
    let offered = canonical_nonce_v11(&wire.nonce).map_err(refused)?;
    if wire.version != 29
        || wire.job != id
        || b.terminal() != NativeTextTerminalV29::Challengeable
        || q.maximum.record() != &f.maximum
        || offered != next
        || next == u64::MAX
        || actor == f.request.producer
        || operator == f.request.producer_operator
        || !matches!(bond.status, PalwBondStatusV2::Active)
        || ctx.daa_score.checked_add(q.complete_path).is_none_or(|at| at >= claim.challenge_end)
    {
        return Err(refused(
            "text29 paid owner independent operator/principal nonce/original full discovery admission cutoff differs",
        ));
    }
    let paid = q.paged.record();
    if wire.discovery {
        if wire.proof_root != [0; 64] || wire.seal_root != [0; 64] || wire.filing_bytes != paid.maximum_bytes {
            return Err(refused("text29 discovery has no preexisting proof and owns exact original maximum"));
        }
    } else if wire.proof_root == [0; 64]
        || wire.seal_root == [0; 64]
        || wire.filing_bytes < 68
        || wire.filing_bytes > paid.maximum_bytes
    {
        return Err(refused("text29 original proof bound or hashes absent"));
    }
    let ticket = if wire.discovery {
        object_id(
            b"misaka-palw/native/text-original-discovery/v29",
            &(id, claim.root(), actor, operator, wire.nonce, ctx.block.as_bytes(), h.maximum),
        )
    } else {
        [0; 64]
    };
    if wire.discovery {
        wire.proof_root = object_id(b"misaka-palw/native/text-undiscovered-proof-marker/v29", &ticket);
        wire.seal_root = object_id(b"misaka-palw/native/text-unsealed-proof-marker/v29", &ticket);
    }
    let mature =
        ctx.daa_score.checked_add(p.execution.horizons.seal_maturity).ok_or_else(|| refused("text29 owner maturity overflow"))?;
    let reveal_until = ctx.daa_score.checked_add(q.proof_window).ok_or_else(|| refused("text29 paid proof horizon overflow"))?;
    let horizon = reveal_until
        .checked_add(p.execution.horizons.da_answer)
        .and_then(|n| n.checked_add(p.execution.horizons.proof_grace))
        .ok_or_else(|| refused("text29 owner full liability overflow"))?;
    if horizon >= f.deadline || ctx.daa_score.checked_add(q.caller.required_horizon()).is_none_or(|d| d > reveal_until) {
        return Err(refused("text29 owner full original acquisition/proof/delivery exceeds funded horizon"));
    }
    let ledger = super::super::palw_kernel_route_fold_v1::route_ledger_policy_v1(builder)?;
    let fee = ledger.job_fee.max(1);
    if (bond.collateral as u128).saturating_sub(builder.committed_at(signer, ctx.daa_score)) < fee as u128 {
        return Err(refused("text29 original paid discovery/seal fee unavailable"));
    }
    let receipt = object_id(
        b"misaka-palw/native/text-original-paid-owner-debit/v29",
        &(id, claim.root(), actor, operator, wire.nonce, ctx.block.as_bytes(), h.maximum, fee, wire.proof_root, wire.seal_root),
    );
    let lease: NativePagedLeaseJournalV29 = {
        let (root, l): (Digest, NativePagedLeaseJournalV29) =
            r.aux_row(EXECUTION_QUOTA_TABLE_V11, &key(&(29u8, id, 22u8))).ok_or_else(|| refused("text29 original lease absent"))?;
        if root != l.root() || l.job != id || l.namespace != 29 || l.funding != h.funding {
            return Err(refused("text29 original physical lease differs"));
        }
        l
    };
    let own = NativePagedSealV29 {
        version: 29,
        namespace: 29,
        job: id,
        claim: claim.root(),
        original_funding: h.funding,
        maximum_contract: h.maximum,
        original_plan: paid.root(),
        original_service: lease.service,
        original_lease: lease.lease_root,
        maximum_scope: f.maximum.class_scope25,
        real_parameter: f.maximum.parameter,
        inventory: f.maximum.inventory,
        bond: *signer,
        actor,
        operator,
        producer_operator: f.request.producer_operator,
        proof_root: wire.proof_root,
        seal_root: wire.seal_root,
        original_paid_receipt: receipt,
        count: wire.filing_bytes.div_ceil(32 << 10) as u16,
        total_bytes: wire.filing_bytes,
        posted: ctx.daa_score,
        matures: mature,
        reveal_until,
        horizon,
        admitted_interval: f.admitted_interval,
        admitted_block: ctx.block.as_bytes(),
        part_attempts: [0; 512],
        accepted: [0; 64],
        assembly_attempts: 0,
        court_attempts: 0,
        discovery: wire.discovery,
        original_ticket: ticket,
        conversion_attempts: 0,
        retired: false,
        spent: false,
        committed: paid.admission.checked_add(&paid.guarantee).map_err(refused)?,
        consumed: paid.admission.clone(),
        remaining: paid.guarantee.clone(),
    };
    let mut owner_ids: Vec<Digest> = read(r, EXECUTION_SEAL_ACTOR_TABLE_V11, 0, &id, 4 + 4 * 64).unwrap_or_default();
    if owner_ids.len() >= 4 || owner_ids.contains(&own.id()) {
        return Err(refused("text29 original lifetime owner catalogue exhausted"));
    }
    owner_ids.push(own.id());
    let status = paid_status(r, id)?;
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let shard = quota_shard(builder, p, &epoch, actor, id)?;
    let token = CheckedNativePagedAdmissionV29::from_current_native(status.clone(), own.clone(), &q.paged, &p.quota, &epoch)
        .map_err(refused)?;
    let mut projection = NativePagedQuotaProjectionV29::from_native_rows(
        &p.quota,
        &q.paged,
        &shard.global_row(),
        shard.global_row().root(),
        &shard.actor_row(),
        shard.actor_row().root(),
        &status,
        status.root(),
        None,
    )
    .map_err(refused)?;
    projection
        .admit(
            &p.quota,
            &epoch,
            &q.paged,
            &token,
            r.flat_model_seal_count_v6() + r.flat_model_seal_count_v7() + r.flat_model_seal_count_v9(),
            (r.flat_model_seals_of_v6(signer).len() + r.flat_model_seals_of_v7(signer).len() + r.flat_model_seals_of_v9(signer).len())
                as u32,
        )
        .map_err(refused)?;
    let counter = r.execution_nonce_counter_v11(&f.request.requester_operator).map_err(refused)?;
    let source_grant = check_text_source_owner_v29(
        &program,
        &registered.real_parameters,
        &q.quota,
        &funded_anchor(&b, &q.quota),
        &nonce_witness(f.request.requester, &counter),
        &p.quota,
        &p.execution,
        &p.discovery,
        &epoch,
        &q.caller,
        &TextNativeSourceOwnerV29 {
            job: id,
            funding: h.funding,
            consent: f.consent,
            claim: claim.root(),
            original_claim: claim.root(),
            seal: own.id(),
            actor,
            actor_operator: operator,
            producer_operator: f.request.producer_operator,
            original_paid: receipt,
            original_plan: q.quota.root(),
            original_caller_plan: q.caller.root(),
            horizon,
            paid_at: ctx.daa_score,
            actual_debit: fee,
            actual_receipt: receipt,
        },
    )
    .map_err(refused)?;
    let subject = shard.subject_row().ok_or_else(|| refused("text29 original subject absent"))?;
    let mut caller = StageSourceCallerShardV26::from_native_rows(
        &p.quota,
        &q.caller,
        projection.global(),
        &projection.global().root(),
        actor,
        Some((projection.actor(), &projection.actor().root())),
        (subject, &subject.row_root()),
        None,
    )
    .map_err(refused)?;
    caller.admit(&p.quota, &epoch, &q.caller, &source_grant).map_err(refused)?;
    let (slotroot, mut slots): (Digest, crate::palw_flat_execution_capacity_v22::NativePhysicalSlotsV22) =
        r.aux_row(EXECUTION_QUOTA_TABLE_V11, &key(&(22u8, id, 2u8))).ok_or_else(|| refused("text29 original fixed streams absent"))?;
    if slots.root() != slotroot || slots.job != id || slots.original_consent23 != f.consent {
        return Err(refused("text29 original streams lineage differs"));
    }
    slots.reserve_owner(own.id(), horizon).map_err(refused)?;
    if builder.slash_bond(*signer, fee as u128)? != fee {
        return Err(PalwStateV2Error::BondBudgetRefused("text29 original fee changed after complete preflight".into()));
    }
    super::paged::write(builder, &projection)?;
    write_caller(builder, &caller)?;
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, key(&(22u8, id, 2u8)), Some(key(&(slots.root(), slots))))?;
    builder.write_kernel_row(EXECUTION_SEAL_ACTOR_TABLE_V11, k(0, &id), Some(key(&owner_ids)))?;
    builder.write_kernel_row(EXECUTION_REQUEST_INDEX_TABLE_V11, nonce_key, Some(key(&(next + 1))))?;
    Ok(())
}
/// A producer Final credit uses the shared unique original-credit queue identity
/// (job, funding, exact settlement, payout), so the independent audit32 /
/// redemption33 verifiers need no Text29-private row to recompute it. A requester
/// refund keeps its own Text29 key; neither key can collide with a pooled payout.
fn terminal_queue(h: &JobHeader, s: &NativeTextSettlementV29) -> Hash64 {
    if s.producer > 0 {
        return crate::palw_native_audit_v27::native_final_credit_queue_v27(h.job, h.funding, s.root(), h.producer_payout);
    }
    let mut id = object_id(
        b"misaka-palw/native/text-exact-original-credit-queue/v29",
        &(h.job, h.funding, h.maximum, false, h.requester_payout),
    );
    id[0] = crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_PAYOUT_KEY_PREFIX_V1;
    Hash64::from_bytes(id)
}
fn settle(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    p: &NativeExecutionPoliciesV11,
    h: &mut JobHeader,
    b: &NativeTextBookV29,
    s: &NativeTextSettlementV29,
    finder: Option<&PalwBondKeyV2>,
) -> Result<(), PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let queue = terminal_queue(h, s);
    let (root, mut subject): (Digest, QuotaSubjectSummaryV11) = r
        .aux_row(EXECUTION_QUOTA_TABLE_V11, &quota_key(2, &h.job))
        .ok_or_else(|| refused("text29 original settlement subject absent"))?;
    if root != subject.row_root()
        || subject.closed
        || subject.binding.job_id != h.job
        || subject.binding.quota_plan_root != h.quota_root
        || b.escrow() != 0
        || b.settlement() != Some(s)
        || s.job != h.job
        || s.original_funding != h.funding
        || builder.state.pending_payouts.contains_key(&queue)
        || r.aux.contains_key(&(EXECUTION_CREDIT_TABLE_V11, k(0, &h.job)))
    {
        return Err(refused("text29 original terminal escrow/subject/queue or single credit differs"));
    }
    let paid_finder = if let Some(f) = finder {
        let v = builder.state.bonds.get(f).ok_or_else(|| refused("text29 original court finder absent"))?;
        if palw_kernel_bond_id_v1(f) != s.finder || v.operator_id.as_bytes() == b.funding().request.producer_operator {
            return Err(refused("text29 finder is not original independent court owner"));
        }
        Some((*f, v.payout_payload))
    } else {
        None
    };
    if s.actual_collected_slash > 0 {
        if builder.state.bonds.get(&h.producer).is_none_or(|v| v.collateral < s.actual_collected_slash) {
            return Err(refused("text29 actual original producer collateral no longer collectible"));
        }
        if s.finder_credit != 0 && paid_finder.is_none() {
            return Err(refused("text29 collected finder award lacks independent finder"));
        }
    }
    if let Some((_, payout)) = paid_finder {
        if !builder.params.reporter_income_active_at(ctx.daa_score)
            && builder
                .state
                .pending_payouts
                .get(&crate::palw_kernel_route_v1::palw_kernel_payout_key_v1(&payout))
                .is_some_and(|v| v.amount.checked_add(s.finder_credit).is_none())
        {
            return Err(refused("text29 finder actual payout overflow"));
        }
    }
    if s.actual_collected_slash > 0 && builder.slash_bond(h.producer, s.actual_collected_slash as u128)? != s.actual_collected_slash {
        return Err(PalwStateV2Error::BondBudgetRefused("text29 original collected collateral changed after final preflight".into()));
    }
    if s.producer > 0 {
        builder.pay_native_recipient_v1(Hash64::from_bytes(h.job), h.producer, 11, ctx.daa_score, s.producer, queue, true)?;
        builder.write_payout(queue, Some(PalwPayoutV2 { payload: h.producer_payout, amount: s.producer }))?;
    } else {
        builder.cancel_native_recipient_v1(Hash64::from_bytes(h.job), h.producer, 11)?;
        builder.write_payout(queue, Some(PalwPayoutV2 { payload: h.requester_payout, amount: s.requester }))?;
    }
    if s.finder_credit > 0 {
        let (f, payout) = paid_finder.unwrap();
        if builder.params.reporter_income_active_at(ctx.daa_score) {
            let award = Hash64::from_bytes(object_id(
                b"misaka-palw/native/text-original-collected-finder-award/v29",
                &(h.job, h.funding, s.proof, s.finder),
            ));
            builder.stage_reporter_recipient_v1(award, f, ctx.daa_score, s.finder_credit, Hash64::from_bytes(h.job))?;
        } else {
            builder.add_kernel_payout(payout, s.finder_credit)?;
        }
    }
    subject.closed = true;
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, quota_key(2, &h.job), Some(key(&(subject.row_root(), subject))))?;
    write_book(builder, h, b)?;
    builder.write_kernel_row(EXECUTION_CREDIT_TABLE_V11, k(0, &h.job), Some(key(&(s, queue, ctx.daa_score))))?;
    builder.bond_budget_close_v1(&super::super::palw_bond_budget_fold_v1::native_text_budget_id_v29(&h.job), ctx.daa_score)?;
    if let Ok(mut status) = paid_status(builder.state.kernel_route.as_ref().unwrap(), h.job) {
        status.active = false;
        builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, key(&(29u8, h.job, 0u8)), Some(key(&(status.root(), status))))?;
    }
    let _ = p;
    Ok(())
}
/// Called only by original common paid Part/Assembly/Court path. No ordinary
/// reveal action may allocate this file or borrow another actor's prepaid work.
pub(super) fn reveal_paged(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    seal: &NativePagedSealV29,
    file: &[u8],
) -> Result<(), PalwStateV2Error> {
    use crate::palw_native_source_proof_v25::*;
    use misaka_palw_kernel::{
        element::SegClaimContextV1,
        flat_court5::{FlatSegContextV5, flat_trace_profile_id_v5},
        flat_court25::*,
        job::DecodeRuleV1,
    };
    let p = policies(builder, ctx)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let mut h = original_header(r, seal.job)?;
    let mut b = original_book(r, &h)?;
    let f = b.funding().clone();
    let claim = b.claim().ok_or_else(|| refused("text29 court original claim absent"))?.clone();
    if seal.namespace != 29
        || seal.spent
        || seal.retired
        || seal.discovery
        || seal.bond != *signer
        || seal.actor != palw_kernel_bond_id_v1(signer)
        || seal.original_funding != h.funding
        || seal.claim != claim.root()
        || seal.maximum_contract != h.maximum
        || seal.original_plan != f.maximum.paged_proof_plan
        || seal.maximum_scope != f.maximum.class_scope25
        || seal.real_parameter != f.maximum.parameter
        || seal.inventory != f.maximum.inventory
        || seal.producer_operator != f.request.producer_operator
        || ctx.daa_score < seal.matures
        || ctx.daa_score >= seal.reveal_until
        || b.terminal() != NativeTextTerminalV29::Challengeable
        || builder.state.bonds.get(signer).is_none_or(|v| v.operator_id.as_bytes() != seal.operator)
    {
        return Err(refused("text29 original paid court source/scope/operator/claim/time differs"));
    }
    let prep: Prepared = read(r, EXECUTION_POLICY_TABLE_V11, 0, &h.job, (1 << 20) + 8192)
        .ok_or_else(|| refused("text29 original preparation absent"))?;
    let q = quote(builder, &p, &prep.wire)?;
    let (ch, registered, program) = class(builder, h.class)?;
    let raw =
        r.aux.get(&(EXECUTION_PROTECTED_TABLE_V11, k(0, &h.job))).ok_or_else(|| refused("text29 original compact post absent"))?;
    let post = decode_post(raw, f.request.maximum_new_tokens, f.maximum.maximum_positions)?;
    if f.maximum != *q.maximum.record()
        || post.vector() != claim.vector
        || post.trace_root() != claim.trace
        || post.actual_scope25 != claim.actual_scope25
        || post.actual_resource29 != claim.actual_resource29
        || file.len() < 68
        || u32::from_le_bytes(file[64..68].try_into().unwrap()) as usize != file.len() - 68
    {
        return Err(refused("text29 paid court original complete contract/trace/file differs"));
    }
    // Window-bound jobs (§II.14.60) accept their own v37 filing envelope; the frozen
    // v25 envelope and its kinds 0..=2 are unchanged.
    let window_active = builder
        .extras
        .kernel_route
        .as_ref()
        .and_then(|e| e.window_class_v37)
        .is_some_and(|at| ctx.daa_score >= at);
    let mut refutes_source = false;
    if window_active && file.get(68..70) == Some(&37u16.to_le_bytes()[..]) {
        window_court(builder, &h, &b, &post, q.paged.record().maximum_bytes, &file[68..])?;
    } else {
    let filing = borrowed_native_filing_v25(&file[68..], q.paged.record().maximum_bytes)
        .ok_or_else(|| refused("text29 paid source/trace filing25 grammar"))?;
    match filing.kind {
        0 => {
            let actual_scope =
                derive_flat_resources_v29(&program, &registered.real_parameters, claim.actual_positions, None).map_err(refused)?.court;
            let c = FlatContextV25 {
                scope: &actual_scope,
                base: FlatSegContextV5 {
                    profile: flat_trace_profile_id_v5(),
                    claim: SegClaimContextV1 {
                        program: &program,
                        params: &registered.real_parameters,
                        segment_roots: &post.segment_roots,
                        positions: claim.actual_positions,
                        prompt_len: f.request.prompt_len,
                        prompt_root: misaka_palw_kernel::seg::prompt_root_of_ids_v1(&prep.wire.prompt),
                        inline_prompt: Some(&prep.wire.prompt),
                        generated: &post.generated,
                        decode: DecodeRuleV1::Greedy,
                        encoder: None,
                    },
                },
            };
            let proof = FlatFaultV25::from_bytes(filing.body).map_err(|e| refused(e.to_string()))?;
            verify_flat_fault_v25(&c, &proof).map_err(|e| refused(format!("{e:?}")))?;
        }
        1 => {
            let bound = derive_native_source_proof_bound_v25(&program, &registered.real_parameters).map_err(refused)?;
            let proof = decode_native_source_fault_v25(&file[68..], &bound).map_err(refused)?;
            let source = source(r, ch.class.source_binding)?;
            let descriptor =
                [misaka_palw_kernel::descriptor::k2_tir_v4_descriptor(), misaka_palw_kernel::descriptor::k2_tir_v5_descriptor()]
                    .into_iter()
                    .find(|d| d.digest() == source.descriptor)
                    .ok_or_else(|| refused("text29 original real-source inventory profile absent"))?;
            let inventory = crate::palw_tir_artifact_v1::PalwTirModelInventoryV2::new(&descriptor, &program).map_err(refused)?;
            crate::palw_onboarding_v1::verify_artifact_mismatch_v2(
                &inventory,
                Hash64::from_bytes(f.maximum.inventory),
                Hash64::from_bytes(f.maximum.parameter),
                &proof,
            )
            .map_err(refused)?;
            refutes_source = true;
        }
        2 => {
            if filing.body.len() > 512 {
                return Err(refused("text29 paid source header fixedcap"));
            }
            let fault: NativeSourceHeaderFaultV16 =
                borsh::from_slice(filing.body).map_err(|_| refused("text29 original source header fault"))?;
            let layer = if fault.layer_code == 0 {
                None
            } else {
                Some(u16::try_from(fault.layer_code - 1).map_err(|_| refused("text29 source layer code"))?)
            };
            let pc = registered
                .real_parameters
                .by_instance
                .get(&(fault.param, layer))
                .ok_or_else(|| refused("text29 original source instance absent"))?;
            let decl = program.params.get(fault.param as usize).ok_or_else(|| refused("text29 original source declaration absent"))?;
            if !fault.header.authenticates(pc) || fault.header.matches_decl(decl.dtype, &decl.shape) {
                return Err(refused("text29 header court establishes no original bound declaration mismatch"));
            }
            refutes_source = true;
        }
        _ => return Err(refused("text29 paid court unrecognized kind")),
    };
    }
    // G14 lane closure G-14: the conviction collects what the producer's bond still holds of the reserve, never refused for less.
    let collectible = crate::palw_g14_lane_closure_v1::palw_native_producer_collectible_v1(
        builder.state.bonds.get(&h.producer).map(|v| v.collateral),
        f.maximum.producer_reserve,
    );
    let settlement = b.convict(seal.proof_root, seal.actor, seal.operator, collectible, ctx.daa_score).map_err(refused)?;
    settle(builder, ctx, &p, &mut h, &b, &settlement, Some(signer))?;
    let mut paid = seal.clone();
    paid.spent = true;
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, key(&(29u8, h.job, paid.id())), Some(key(&(paid.root(), paid))))?;
    if refutes_source {
        builder.write_kernel_row(
            EXECUTION_SOURCE_FACT_TABLE_V11,
            k(0, &source_relation(&ch.class)),
            Some(key(&(h.job, seal.proof_root, ctx.daa_score))),
        )?;
    }
    Ok(())
}
fn close(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    raw: &[u8],
) -> Result<(), PalwStateV2Error> {
    let id = fixed_id(raw)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let mut h = original_header(r, id)?;
    if !paid_restore(builder, ctx, signer, p, Some(h.class), raw.len() + h.book_bytes as usize)? {
        return Ok(());
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let mut b = original_book(r, &h)?;
    if b.terminal() == NativeTextTerminalV29::Funded {
        let s = b.cancel(ctx.daa_score).map_err(refused)?;
        return settle(builder, ctx, p, &mut h, &b, &s, None);
    }
    if b.terminal() != NativeTextTerminalV29::Challengeable {
        return Err(refused("text29 original principal already settled"));
    }
    let prep: Prepared = read(r, EXECUTION_POLICY_TABLE_V11, 0, &id, (1 << 20) + 8192)
        .ok_or_else(|| refused("text29 original initial contract absent"))?;
    let q = quote(builder, p, &prep.wire)?;
    let f = b.funding().clone();
    let claim = b.claim().unwrap();
    check_bound_window_lengths(r,&h,q.original_history_bound,claim.actual_positions)?;
    let status = paid_status(r, id)?;
    let (open, latest) = public::open(r, id)?;
    if q.maximum.record() != &f.maximum
        || status.namespace != 29
        || status.job != id
        || status.funding != f.root()
        || status.claim != claim.root()
        || status.maximum != f.maximum.root()
        || !status.active
        || status.paid_owners.iter().any(|v| *v != [0; 64])
        || open != 0
        || ctx.daa_score <= latest
        || ctx.daa_score <= status.latest_paid_horizon
        || ctx.daa_score <= claim.retained_until
    {
        return Err(refused("text29 original source/trace/proof/discovery/public default liabilities remain"));
    }
    let (qr, queue): (Digest, NativeQueueLeaseV30) = r
        .aux_row(EXECUTION_QUOTA_TABLE_V11, &key(&(30u8, id, 1u8)))
        .ok_or_else(|| refused("text29 original residency lease absent"))?;
    let (lr, lease): (Digest, NativePagedLeaseJournalV29) = r
        .aux_row(EXECUTION_QUOTA_TABLE_V11, &key(&(29u8, id, 22u8)))
        .ok_or_else(|| refused("text29 original physical lease absent"))?;
    let (source_root, source): (Digest, SourceObligationV16) = r
        .aux_row(EXECUTION_QUOTA_TABLE_V11, &key(&(16u8, id)))
        .ok_or_else(|| refused("text29 original bound-PC response obligation absent"))?;
    if queue.root() != qr
        || queue.retired
        || queue.original_funding != f.root()
        || queue.original_plan != f.maximum.queue_plan
        || queue.original_policy != f.maximum.queue_policy
        || lease.root() != lr
        || lease.lease_root != queue.original_physical_lease
        || lease.lease.retired
        || source.root() != source_root
        || source.plan_root != f.maximum.source_acquisition
        || source.original_consent_root != f.consent
    {
        return Err(refused("text29 original funded queue/source/physical terms rebound"));
    }
    // This private close token certifies original paid liability closure only.
    // Correctness/detection/source availability remains the initial trusted27
    // numerical profile, rechecked above; carriers supply no boolean grants.
    let closed = CheckedNativeTextCloseV29 {
        job: id,
        claim: claim.root(),
        book: b.root(),
        original_funding: f.root(),
        now: ctx.daa_score,
        final_policy: f.maximum.final_policy27,
        final_basis: f.maximum.final_basis27,
    };
    let s = b.finalize(&q.conditions, &closed).map_err(refused)?;
    settle(builder, ctx, p, &mut h, &b, &s, None)
}
fn projection_for(
    builder: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    p: &NativeExecutionPoliciesV11,
    status: &NativePagedJobStatusV29,
    own: &NativePagedSealV29,
    plan: &CheckedNativePagedProofPlanV29,
) -> Result<crate::palw_native_paged_quota_v29::NativePagedQuotaProjectionV29, PalwStateV2Error> {
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let base = quota_shard(builder, p, &epoch, own.actor, own.job)?;
    crate::palw_native_paged_quota_v29::NativePagedQuotaProjectionV29::from_native_rows(
        &p.quota,
        plan,
        &base.global_row(),
        base.global_row().root(),
        &base.actor_row(),
        base.actor_row().root(),
        status,
        status.root(),
        Some((own, own.root())),
    )
    .map_err(refused)
}
fn expire_owner(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    raw: &[u8],
) -> Result<(), PalwStateV2Error> {
    use crate::palw_native_paged_quota_v29::*;
    use misaka_palw_kernel::flat_quota11::stage14::source26::*;
    if raw.len() != 129 || raw[0] != 0 {
        return Err(refused("text29 owner expiry exact kind/job/owner grammar"));
    }
    let job = raw[1..65].try_into().unwrap();
    let id = raw[65..129].try_into().unwrap();
    let r = builder.state.kernel_route.as_ref().unwrap();
    let h = original_header(r, job)?;
    let own = r.native_paged_seal_original_v29(job, id).ok_or_else(|| refused("text29 archival original paid owner absent"))?;
    if own.namespace != 29 || own.retired || ctx.daa_score <= own.horizon {
        return Err(refused("text29 original paid owner immature/already retired"));
    }
    let mut removed = 129usize;
    for index in 0..own.count {
        if let Some(bytes) = r.aux.get(&(EXECUTION_SEAL_TABLE_V11, key(&(29u8, job, id, index)))) {
            if bytes.len() > 32 << 10 {
                return Err(refused("text29 original paid cache extent differs"));
            }
            removed = removed.checked_add(bytes.len()).ok_or_else(|| refused("text29 paid cache journal extent overflow"))?;
        }
    }
    if !paid_restore(builder, ctx, signer, p, Some(h.class), removed)? {
        return Ok(());
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let book = original_book(r, &h)?;
    let status = paid_status(r, job)?;
    if own.original_funding != h.funding
        || own.maximum_contract != h.maximum
        || book.claim().map(|c| c.root()) != Some(own.claim)
        || !matches!(
            book.terminal(),
            NativeTextTerminalV29::Challengeable | NativeTextTerminalV29::Final | NativeTextTerminalV29::Convicted
        )
    {
        return Err(refused("text29 owner expiry does not authenticate original live/terminal book"));
    }
    let pending = public::owner_open(r, job, id)?;
    let token =
        CheckedNativePagedExpiryV29::from_current_native(&status, &own, book.root(), ctx.daa_score, pending).map_err(refused)?;
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let plan = CheckedNativePagedProofPlanV29::restore(
        r.aux.get(&(EXECUTION_QUOTA_TABLE_V11, key(&(29u8, job, 1u8)))).ok_or_else(|| refused("text29 original paid quote absent"))?,
        own.original_plan,
        &p.quota,
    )
    .map_err(refused)?;
    let mut projection = projection_for(builder, ctx, p, &status, &own, &plan)?;
    projection.expire_owner(&p.quota, &epoch, &plan, &token).map_err(refused)?;
    let caller_plan = public::caller_plan(r, job, &book, p)?;
    let base = public::caller(r, p, &caller_plan, &own, projection.global(), projection.actor())?;
    let (_, subject): (Digest, QuotaSubjectSummaryV11) =
        r.aux_row(EXECUTION_QUOTA_TABLE_V11, &quota_key(2, &job)).ok_or_else(|| refused("text29 original caller subject absent"))?;
    let row = base.caller_row().ok_or_else(|| refused("text29 original caller retirement row absent"))?;
    let row_root = row.root();
    let mut caller = StageSourceCallerShardV26::from_native_rows(
        &p.quota,
        &caller_plan,
        projection.global(),
        &projection.global().root(),
        own.actor,
        Some((projection.actor(), &projection.actor().root())),
        (&subject, &subject.row_root()),
        Some((row, &row_root)),
    )
    .map_err(refused)?;
    caller.expire_owner(&p.quota, &epoch, &caller_plan, id, own.original_paid_receipt).map_err(refused)?;
    let (root, mut slots): (Digest, crate::palw_flat_execution_capacity_v22::NativePhysicalSlotsV22) = r
        .aux_row(EXECUTION_QUOTA_TABLE_V11, &key(&(22u8, job, 2u8)))
        .ok_or_else(|| refused("text29 original expiry streams absent"))?;
    if slots.root() != root || slots.job != job {
        return Err(refused("text29 original expiry stream root differs"));
    }
    slots.retire_owner(id, ctx.daa_score).map_err(refused)?;
    super::paged::write(builder, &projection)?;
    write_caller(builder, &caller)?;
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, key(&(22u8, job, 2u8)), Some(key(&(slots.root(), slots))))?;
    for index in 0..own.count {
        builder.write_kernel_row(EXECUTION_SEAL_TABLE_V11, key(&(29u8, job, id, index)), None)?;
    }
    Ok(())
}
fn convert(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    raw: &[u8],
) -> Result<(), PalwStateV2Error> {
    use misaka_palw_kernel::flat_quota11::stage14::source26::*;
    if raw.len() != 262 {
        return Err(refused("text29 exact discovery conversion grammar"));
    }
    let job = raw[..64].try_into().unwrap();
    let old = raw[64..128].try_into().unwrap();
    let proof = raw[128..192].try_into().unwrap();
    let seal = raw[192..256].try_into().unwrap();
    let total = u32::from_le_bytes(raw[256..260].try_into().unwrap());
    let count = u16::from_le_bytes(raw[260..262].try_into().unwrap());
    let r = builder.state.kernel_route.as_ref().unwrap();
    let h = original_header(r, job)?;
    let own = r.native_paged_seal_v29(job, old).ok_or_else(|| refused("text29 original active discovery ticket absent"))?;
    if own.namespace != 29
        || own.bond != *signer
        || own.operator
            != builder.state.bonds.get(signer).ok_or_else(|| refused("text29 discovery actor absent"))?.operator_id.as_bytes()
        || !own.discovery
        || own.spent
        || own.retired
    {
        return Err(refused("text29 conversion does not own original paid independent discovery"));
    }
    let book = original_book(r, &h)?;
    if book.terminal() != NativeTextTerminalV29::Challengeable
        || book.claim().map(|c| c.root()) != Some(own.claim)
        || public::owner_open(r, job, old)? != 0
    {
        return Err(refused("text29 original discovery still has pending public units or closed book"));
    }
    let status = paid_status(r, job)?;
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let plan = CheckedNativePagedProofPlanV29::restore(
        r.aux
            .get(&(EXECUTION_QUOTA_TABLE_V11, key(&(29u8, job, 1u8))))
            .ok_or_else(|| refused("text29 original paid conversion quote absent"))?,
        own.original_plan,
        &p.quota,
    )
    .map_err(refused)?;
    let mut projection = projection_for(builder, ctx, p, &status, &own, &plan)?;
    projection
        .convert_discovery(
            &p.quota,
            &epoch,
            &plan,
            own.actor,
            own.operator,
            proof,
            seal,
            count,
            total,
            ctx.daa_score
                .checked_add(p.execution.horizons.seal_maturity)
                .ok_or_else(|| refused("text29 fresh proof maturity overflow"))?,
        )
        .map_err(refused)?;
    // The original conversion price owns fixed caller/stream journals. It is
    // persisted before subsequent restoration; malformed attempts cannot reset.
    let new = projection.seal().unwrap().id();
    let r = builder.state.kernel_route.as_ref().unwrap();
    let quote = public::caller_plan(r, job, &book, p)?;
    let base = public::caller(r, p, &quote, &own, projection.global(), projection.actor())?;
    let (_, subject): (Digest, QuotaSubjectSummaryV11) = r
        .aux_row(EXECUTION_QUOTA_TABLE_V11, &quota_key(2, &job))
        .ok_or_else(|| refused("text29 original conversion subject absent"))?;
    let row = base.caller_row().ok_or_else(|| refused("text29 original discovery caller absent"))?;
    let root = row.root();
    let mut caller = StageSourceCallerShardV26::from_native_rows(
        &p.quota,
        &quote,
        projection.global(),
        &projection.global().root(),
        own.actor,
        Some((projection.actor(), &projection.actor().root())),
        (&subject, &subject.row_root()),
        Some((row, &root)),
    )
    .map_err(refused)?;
    caller.transfer_owner(&p.quota, &epoch, &quote, old, new, own.original_paid_receipt).map_err(refused)?;
    let (root, mut slots): (Digest, crate::palw_flat_execution_capacity_v22::NativePhysicalSlotsV22) = r
        .aux_row(EXECUTION_QUOTA_TABLE_V11, &key(&(22u8, job, 2u8)))
        .ok_or_else(|| refused("text29 original discovery stream book absent"))?;
    if root != slots.root() || slots.job != job || slots.original_consent23 != book.funding().consent {
        return Err(refused("text29 original discovery stream identity differs"));
    }
    slots.transfer_owner(old, new, own.horizon).map_err(refused)?;
    let mut owners: Vec<Digest> = read(r, EXECUTION_SEAL_ACTOR_TABLE_V11, 0, &job, 4 + 4 * 64)
        .ok_or_else(|| refused("text29 original discovery lifetime catalogue absent"))?;
    *owners.iter_mut().find(|v| **v == old).ok_or_else(|| refused("text29 original discovery lifetime member absent"))? = new;
    builder.write_kernel_row(EXECUTION_SEAL_ACTOR_TABLE_V11, k(0, &job), Some(key(&owners)))?;
    super::paged::write(builder, &projection)?;
    write_caller(builder, &caller)?;
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, key(&(29u8, job, old)), None)?;
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, key(&(26u8, job, old)), None)?;
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, key(&(22u8, job, 2u8)), Some(key(&(slots.root(), slots))))?;
    builder.write_kernel_row(
        EXECUTION_CREDIT_TABLE_V11,
        k(4, &old),
        Some(key(&(job, new, own.original_paid_receipt, own.original_ticket, own.horizon))),
    )?;
    Ok(())
}

fn source_relation(class:&NativeTextClassV29)->Digest{object_id(b"misaka-palw/native/exact-real-source-relation/v29",&(29u16,class.original_program,class.real_parameter,class.inventory))}
/// Text29's own action grammar inside the funded namespace (actions 27..=40). The
/// caller (`palw_flat_execution_fold_v11::apply`) runs this only past BOTH the
/// funded fence and the trusted Final27 fence; below them every Text29 body rides
/// unjudged. A refusal is a dismissed body: any persisted predebit stays paid.
/// Action 34 has no body of its own — a paid paged proof reveals only at its
/// complete Part/Assembly/Court-debited assembly through action 32.
pub(super) fn apply(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    action: u8,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    match action {
        TEXT_REGISTER_ACTION_V29 => register(builder, ctx, signer, p, body),
        TEXT_PREPARE_ACTION_V29 => prepare(builder, ctx, signer, p, body),
        TEXT_ACCEPT_ACTION_V29 => accept(builder, ctx, signer, p, body),
        TEXT_FUND_ACTION_V29 => fund(builder, ctx, signer, p, body),
        TEXT_COMMIT_ACTION_V29 => commit(builder, ctx, signer, p, body),
        TEXT_PAGE_ACTION_V29 => super::paged::part(builder, ctx, signer, body),
        TEXT_OWN_ACTION_V29 => own(builder, ctx, signer, p, body),
        TEXT_REVEAL_ACTION_V29 => Err(refused("text29 paged proof reveals only at its paid assembly (action 32)")),
        TEXT_CLOSE_ACTION_V29 => close(builder, ctx, signer, p, body),
        TEXT_REQUEST_ACTION_V29 => public::request(builder, ctx, signer, p, body),
        TEXT_ANSWER_ACTION_V29 => public::answer(builder, ctx, signer, p, body),
        TEXT_DEFAULT_ACTION_V29 => public::default(builder, ctx, signer, p, body),
        TEXT_DISCOVERY_ACTION_V29 => convert(builder, ctx, signer, p, body),
        TEXT_COLLECT_ACTION_V29 => match body.first() {
            // kind 0: retire one expired paid paged owner (exact 129 bytes).
            Some(0) => expire_owner(builder, ctx, signer, p, body),
            // kind 1: collect the next indexed public answer/default cache of one job.
            Some(1) if body.len() == 65 => public::collect_step(builder, ctx, signer, p, body[1..].try_into().unwrap()),
            // kind 2: retire one terminal job's own rows once every liability is gone.
            Some(2) if body.len() == 65 => collect_terminal(builder, ctx, signer, p, body[1..].try_into().unwrap()),
            _ => Err(refused("text29 collection exact kind grammar")),
        },
        _ => Err(refused("unknown text29 action")),
    }
}
/// Requester DA bonds still open on Text29 public requests (part of the bond's
/// unavailable collateral beside the original producer reserve).
pub(super) fn demand_reserved(r: &PalwKernelRouteStateV1, bond: &PalwBondKeyV2) -> u128 {
    public::reserved(r, bond)
}
/// Public collection of one genuinely terminal Text29 job (collection kind 2).
/// Mirrors the funded v24 / Stage28 collectors: every paid owner, public answer
/// and default cache must already be retired; the source guarantee, the shared
/// quota subject and the joint queue30/physical22 leases are retired ONCE under
/// a kernel-checked original terminal grant; then the finite owner catalogues
/// regain their slots. The immutable original credit receipt (credit kind 0),
/// consumed-consent anchor (kind 3) and the permanent nonce counter survive, so
/// redemption audit and replay protection never depend on retained job rows.
fn collect_terminal(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    id: Digest,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_flat_execution_capacity_v22::NativeCarriageGlobalV22;
    use misaka_palw_kernel::flat_quota11::{source16::retire_expired_source_v16, stage14::source26::StageSourceCallerRowV26};
    let r = builder.state.kernel_route.as_ref().unwrap();
    let h = original_header(r, id)?;
    let post_bytes = r.aux.get(&(EXECUTION_PROTECTED_TABLE_V11, k(0, &id))).map_or(0, Vec::len);
    if !paid_restore(builder, ctx, signer, p, Some(h.class), 65 + h.book_bytes as usize + post_bytes)? {
        return Ok(());
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let b = original_book(r, &h)?;
    let f = b.funding().clone();
    let s = b.settlement().ok_or_else(|| refused("text29 collection genuine original settlement absent"))?.clone();
    let (credited, queue, established): (NativeTextSettlementV29, Hash64, u64) = r
        .aux_row(EXECUTION_CREDIT_TABLE_V11, &k(0, &id))
        .ok_or_else(|| refused("text29 collection immutable original credit receipt absent"))?;
    if !matches!(b.terminal(), NativeTextTerminalV29::Final | NativeTextTerminalV29::Convicted | NativeTextTerminalV29::Refunded)
        || credited != s
        || queue != terminal_queue(&h, &s)
        || b.escrow() != 0
        || ctx.daa_score <= f.deadline
        || b.claim().is_some_and(|c| ctx.daa_score <= c.retained_until)
    {
        return Err(refused("text29 collection lacks genuine expired original terminal and single credit"));
    }
    if !public::all_collected(r, id)? {
        return Err(refused("text29 collection original public answer/default caches remain"));
    }
    if let Ok(status) = paid_status(r, id) {
        if status.active || status.paid_owners.iter().any(|v| *v != [0; 64]) || ctx.daa_score <= status.latest_paid_horizon {
            return Err(refused("text29 collection original paid owner liability remains"));
        }
    }
    let owners: Vec<Digest> = read(r, EXECUTION_SEAL_ACTOR_TABLE_V11, 0, &id, 4 + 4 * 64).unwrap_or_default();
    if owners.len() > 4 {
        return Err(refused("text29 collection original owner lifetime catalogue differs"));
    }
    for owner in &owners {
        let own = r.native_paged_seal_original_v29(id, *owner).ok_or_else(|| refused("text29 collection archived owner absent"))?;
        let (root, caller): (Digest, StageSourceCallerRowV26) = r
            .aux_row(EXECUTION_QUOTA_TABLE_V11, &key(&(26u8, id, *owner)))
            .ok_or_else(|| refused("text29 collection original caller retirement absent"))?;
        if !own.retired
            || own.original_funding != h.funding
            || ctx.daa_score <= own.horizon
            || caller.root() != root
            || !caller.retired
            || caller.owner != *owner
            || caller.original_paid != own.original_paid_receipt
            || caller.remaining != Default::default()
        {
            return Err(refused("text29 collection original owner/source/cache liability remains"));
        }
    }
    let prep: Prepared = read(r, EXECUTION_POLICY_TABLE_V11, 0, &id, (1 << 20) + 8192)
        .ok_or_else(|| refused("text29 collection original initial contract absent"))?;
    let q = quote(builder, p, &prep.wire)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    if q.maximum.record() != &f.maximum || q.quota.plan().record().root() != h.quota_root {
        return Err(refused("text29 collection original quote/maximum rebound"));
    }
    let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
    let counter = r.execution_nonce_counter_v11(&f.request.requester_operator).map_err(refused)?;
    let terminal = check_text_terminal_v29(
        &q.quota,
        &funded_anchor(&b, &q.quota),
        &p.quota,
        &TextNativeTerminalAnchorV29 {
            job: id,
            funding: h.funding,
            terminal_book: b.root(),
            request_key: f.request.root(),
            nonce_witness: nonce_witness(f.request.requester, &counter),
            producer_credit: s.producer,
            requester_credit: s.requester,
            actual_slash: s.actual_collected_slash,
            finder_credit: s.finder_credit,
            escrow_debit: s.escrow_debit,
            proof: s.proof,
            established,
            closed_escrow: b.escrow() == 0,
        },
        &epoch,
    )
    .map_err(refused)?;
    // Source guarantee: same interval retires on the producer's own shard; a later
    // interval charges only the current caller's ordinary fixed-row collection.
    let (source_root, source_row) = read_rooted::<SourceObligationV16>(r, key(&(16u8, id)), SOURCE_ROW_BYTES_V16 as usize)?
        .ok_or_else(|| refused("text29 collection original source obligation absent"))?;
    if source_row.plan_root != q.source.root() || source_row.original_consent_root != f.consent {
        return Err(refused("text29 collection original source consent/quote differs"));
    }
    // The producer's same-interval source retirement and the caller's collection
    // shard must share ONE global row lineage: the caller shard is rebuilt from the
    // retired source shard's rows, so neither write can overwrite the other.
    let same_interval = epoch.network_interval == f.admitted_interval;
    let mut producer_source = None;
    let mut source_retired = source_row.root();
    if !source_row.retired && same_interval {
        let base = quota_shard(builder, p, &epoch, f.request.producer, id)?;
        let mut src = source_shard_from_rows(
            p,
            &q.source,
            &base.global_row(),
            &base.actor_row(),
            base.subject_row().ok_or_else(|| refused("text29 collection original subject absent"))?,
            Some((&source_row, &source_root)),
        )?;
        src.retire(&p.quota, &epoch, &q.source, &terminal).map_err(refused)?;
        source_retired = src.source_row().ok_or_else(|| refused("text29 collection retired source row absent"))?.root();
        producer_source = Some(src);
    }
    let actor = palw_kernel_bond_id_v1(signer);
    let mut shard = match &producer_source {
        Some(src) => {
            let g = src.global_row().clone();
            let producer_row = src.producer_row().clone();
            let state_actor =
                read_rooted::<QuotaActorRowV11>(r, quota_key(1, &actor), QUOTA_SELECTED_ACTOR_ROW_BYTES_V11 as usize)?;
            let a = if producer_row.actor == actor {
                Some((producer_row.root(), producer_row))
            } else {
                state_actor
            };
            let subject =
                read_rooted::<QuotaSubjectSummaryV11>(r, quota_key(2, &id), QUOTA_SELECTED_SUBJECT_ROW_BYTES_V11 as usize)?;
            CheckedQuotaShardV11::from_native_rows(
                &p.quota,
                &g,
                &g.root(),
                actor,
                id,
                None,
                a.as_ref().map(|(root, row)| (row, root)),
                subject.as_ref().map(|(root, row)| (row, root)),
                None,
            )
            .map_err(refused)?
        }
        None => quota_shard(builder, p, &epoch, actor, id)?,
    };
    if !source_row.retired && !same_interval {
        source_retired =
            retire_expired_source_v16(&mut shard, &p.quota, &epoch, &q.source, &source_row, &source_root, &terminal)
                .map_err(refused)?
                .root();
    }
    // Joint original queue30 + physical22 retirement, exactly once.
    let (qlease_root, mut qlease): (Digest, NativeQueueLeaseV30) = r
        .aux_row(EXECUTION_QUOTA_TABLE_V11, &key(&(30u8, id, 1u8)))
        .ok_or_else(|| refused("text29 collection original queue lease absent"))?;
    let (jroot, mut journal): (Digest, NativePagedLeaseJournalV29) = r
        .aux_row(EXECUTION_QUOTA_TABLE_V11, &key(&(29u8, id, 22u8)))
        .ok_or_else(|| refused("text29 collection original physical lease absent"))?;
    let (pgroot, mut physical): (Digest, NativeCarriageGlobalV22) = r
        .aux_row(EXECUTION_QUOTA_TABLE_V11, &key(&(22u8, 0u8)))
        .ok_or_else(|| refused("text29 collection original physical global absent"))?;
    let qraw = r
        .aux
        .get(&(EXECUTION_QUOTA_TABLE_V11, key(&(30u8, 0u8))))
        .ok_or_else(|| refused("text29 collection original queue global absent"))?;
    if qraw.len() > NATIVE_QUEUE_ROW_LIMIT_V30 {
        return Err(refused("text29 collection queue global cap"));
    }
    let (qgroot, mut global): (Digest, NativeQueueGlobalV30) =
        borsh::from_slice(qraw).map_err(|_| refused("text29 collection queue global grammar"))?;
    let (slotroot, slots): (Digest, crate::palw_flat_execution_capacity_v22::NativePhysicalSlotsV22) = r
        .aux_row(EXECUTION_QUOTA_TABLE_V11, &key(&(22u8, id, 2u8)))
        .ok_or_else(|| refused("text29 collection original streams absent"))?;
    if qlease.root() != qlease_root
        || qlease.job != id
        || qlease.original_funding != h.funding
        || qlease.original_plan != f.maximum.queue_plan
        || journal.root() != jroot
        || journal.namespace != 29
        || journal.job != id
        || journal.funding != h.funding
        || journal.lease_root != journal.lease.root()
        || physical.root() != pgroot
        || global.root() != qgroot
        || slots.root() != slotroot
        || slots.job != id
        || slots.owners.iter().any(|v| *v != [0; 64])
        || slots.not_before.iter().any(|d| ctx.daa_score <= *d)
    {
        return Err(refused("text29 collection original queue/physical/stream terms differ or remain owned"));
    }
    let closure = CheckedNativeQueueClosureV30::from_current_native(
        id,
        h.funding,
        qlease_root,
        journal.lease_root,
        b.root(),
        ctx.daa_score,
        false,
    )
    .map_err(refused)?;
    let qp = &builder.extras.kernel_route.as_ref().unwrap().native_final.as_ref().unwrap().policy.queue_policy;
    global.retire(&mut physical, qp, &mut qlease, &mut journal.lease, &closure).map_err(refused)?;
    shard.collect_closed_expired(&p.quota, &epoch, &terminal).map_err(refused)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let mut requester_ids: Vec<Digest> = read(r, EXECUTION_JOB_OWNER_TABLE_V11, 0, &h.requester, 4 + 8 * 64).unwrap_or_default();
    let mut producer_ids: Vec<Digest> = read(r, EXECUTION_JOB_OWNER_TABLE_V11, 1, &h.producer, 4 + 8 * 64).unwrap_or_default();
    if !requester_ids.contains(&id) || !producer_ids.contains(&id) {
        return Err(refused("text29 collection original catalogue membership absent"));
    }
    requester_ids.retain(|v| *v != id);
    producer_ids.retain(|v| *v != id);
    // ---- every fallible check is above; journal writes only below ----
    if let Some(src) = &producer_source {
        write_source_shard(builder, src, id)?;
    }
    write_shard(builder, &shard, id)?;
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, key(&(30u8, 0u8)), Some(key(&(global.root(), global))))?;
    builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, key(&(22u8, 0u8)), Some(key(&(physical.root(), physical))))?;
    builder.write_kernel_row(EXECUTION_JOB_OWNER_TABLE_V11, k(0, &h.requester), Some(key(&requester_ids)))?;
    builder.write_kernel_row(EXECUTION_JOB_OWNER_TABLE_V11, k(1, &h.producer), Some(key(&producer_ids)))?;
    builder.write_kernel_row(
        EXECUTION_CREDIT_TABLE_V11,
        k(4, &id),
        Some(key(&(
            b.root(),
            object_id(b"misaka-palw/native/text-collected-credit/v29", &(&s, queue, established)),
            source_retired,
            ctx.daa_score,
        ))),
    )?;
    for owner in &owners {
        builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, key(&(29u8, id, *owner)), None)?;
        builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, key(&(26u8, id, *owner)), None)?;
    }
    for kk in [
        quota_key(2, &id),
        key(&(16u8, id)),
        key(&(16u8, id, 1u8)),
        key(&(26u8, id, 0u8)),
        key(&(29u8, id, 0u8)),
        key(&(29u8, id, 1u8)),
        key(&(29u8, id, 3u8)),
        key(&(29u8, id, 22u8)),
        key(&(29u8, id, 23u8)),
        key(&(30u8, id, 1u8)),
        key(&(22u8, id)),
        key(&(22u8, id, 2u8)),
    ] {
        builder.write_kernel_row(EXECUTION_QUOTA_TABLE_V11, kk, None)?;
    }
    for (table, kk) in [
        (EXECUTION_JOB_TABLE_V11, k(0, &id)),
        (EXECUTION_JOB_TABLE_V11, k(1, &id)),
        // A window-bound job's binding (§II.14.60) leaves with the job.
        (EXECUTION_JOB_TABLE_V11, key(&(37u8, 3u8, id))),
        (EXECUTION_POLICY_TABLE_V11, k(0, &id)),
        (EXECUTION_PROTECTED_TABLE_V11, k(0, &id)),
        (EXECUTION_SEAL_ACTOR_TABLE_V11, k(0, &id)),
        (EXECUTION_DEMAND_COUNT_TABLE_V11, k(0, &id)),
    ] {
        builder.write_kernel_row(table, kk, None)?;
    }
    builder.note_kernel_bond(&h.requester)?;
    builder.note_kernel_bond(&h.producer)?;
    Ok(())
}
/// Independent audit extraction of one genuine Text29 producer Final from
/// authenticated original rows only (the audit32 full aux opening or the live
/// store). The result uses the shared v27 audit record with `namespace = 29`;
/// its payout queue is the shared unique credit identity, so redemption33
/// verifies delivery with no producer/Panel private state and no Text29 row DTO.
pub(super) fn audit_record_from_rows_v29(
    rows: &std::collections::BTreeMap<(u8, Vec<u8>), Vec<u8>>,
    network_domain: Digest,
    daa: u64,
    roots: (Digest, Digest, Digest),
    payouts: &std::collections::BTreeMap<Hash64, PalwPayoutV2>,
    id: Digest,
) -> Result<crate::palw_native_audit_v27::NativeExecutionAuditRecordV27, String> {
    use crate::palw_native_audit_v27::{NativeAuditRowV27, NativeExecutionAuditRecordV27};
    fn row<'a>(
        rows: &'a std::collections::BTreeMap<(u8, Vec<u8>), Vec<u8>>,
        table: u8,
        k: &[u8],
        cap: usize,
    ) -> Result<&'a Vec<u8>, String> {
        let raw = rows.get(&(table, k.to_vec())).ok_or("text29 audit original row absent")?;
        if raw.len() > cap {
            return Err("text29 audit original row cap".into());
        }
        Ok(raw)
    }
    let job_key = k(0, &id);
    let h: JobHeader = borsh::from_slice(row(rows, EXECUTION_JOB_TABLE_V11, &job_key, 8192)?)
        .map_err(|_| "text29 audit original header grammar")?;
    let book_key = k(1, &id);
    let raw = row(rows, EXECUTION_JOB_TABLE_V11, &book_key, TEXT_STOP_BOOK_BYTES_V29)?;
    if h.job != id || raw.len() != h.book_bytes as usize {
        return Err("text29 audit original header/book namespace differs".into());
    }
    let b = NativeTextBookV29::restore(raw, h.funding, h.book)?;
    let class_key = k(0, &h.class);
    let c: ClassHeader = borsh::from_slice(row(rows, EXECUTION_CLASS_TABLE_V11, &class_key, 2048)?)
        .map_err(|_| "text29 audit original class grammar")?;
    let f = b.funding();
    let claim = b.claim().ok_or("text29 audit Final lacks claim")?;
    check_bound_window_lengths_from_rows(rows, &h, &b, None, claim.actual_positions)?;
    let s = b.settlement().ok_or("text29 audit Final lacks settlement")?;
    let credit_key = k(0, &id);
    let (credited, queue, established): (NativeTextSettlementV29, Hash64, u64) =
        borsh::from_slice(row(rows, EXECUTION_CREDIT_TABLE_V11, &credit_key, 1024)?).map_err(|_| "text29 audit credit grammar")?;
    let nonce_key = key(&f.request.requester_operator);
    let raw = row(rows, EXECUTION_REQUEST_INDEX_TABLE_V11, &nonce_key, 72)?;
    if raw.len() != 72 {
        return Err("text29 audit fixed nonce row".into());
    }
    let nonce: NativeRequesterNonceRowV11 = borsh::from_slice(raw).map_err(|_| "text29 audit nonce grammar")?;
    if c.class.id() != h.class
        || f.id() != id
        || f.request.class != h.class
        || b.terminal() != NativeTextTerminalV29::Final
        || h.terminal != NativeTextTerminalV29::Final
        || b.escrow() != 0
        || credited != *s
        || s.terminal != NativeTextTerminalV29::Final
        || s.claim != Some(claim.root())
        || s.producer == 0
        || s.producer != s.escrow_debit
        || s.requester != 0
        || s.actual_collected_slash != 0
        || s.finder_credit != 0
        || s.burned != 0
        || queue != terminal_queue(&h, s)
        || established < claim.challenge_end
        || nonce.operator != f.request.requester_operator
        || nonce.next_nonce <= canonical_nonce_v11(&f.request.nonce)?
        || f.request.requester_operator == f.request.producer_operator
        || f.maximum.final_policy27 == [0; 64]
    {
        return Err("text29 audit Final/nonce/original debit/single credit agreement differs".into());
    }
    let mut audit_rows = Vec::new();
    for (table, kk) in [
        (EXECUTION_CLASS_TABLE_V11, class_key),
        (EXECUTION_JOB_TABLE_V11, job_key),
        (EXECUTION_JOB_TABLE_V11, book_key),
        (EXECUTION_CREDIT_TABLE_V11, credit_key),
        (EXECUTION_REQUEST_INDEX_TABLE_V11, nonce_key),
    ] {
        let bytes = rows.get(&(table, kk.clone())).ok_or("text29 audit original row missing")?.clone();
        audit_rows.push(NativeAuditRowV27 { table, key: kk, bytes });
    }
    let (state_root, ledger_root, aux_root) = roots;
    Ok(NativeExecutionAuditRecordV27 {
        version: 27,
        namespace: 29,
        network_domain,
        daa,
        state_root,
        ledger_root,
        aux_root,
        class: h.class,
        source: c.class.source_binding,
        program: c.class.original_program,
        real_parameter: c.class.real_parameter,
        inventory: c.class.inventory,
        job: id,
        funding: f.root(),
        original_escrow_debit: s.escrow_debit,
        original_escrow_receipt: f.escrow_debit,
        original_consent: f.consent,
        original_final_policy: f.maximum.final_policy27,
        original_final_basis: f.maximum.final_basis27,
        maximum_scope: f.maximum.maximum_scope25,
        maximum_resource: f.maximum.maximum_resource29,
        actual_scope: claim.actual_scope25,
        actual_resource: claim.actual_resource29,
        claim: claim.root(),
        vector: claim.vector,
        trace: claim.trace,
        output: claim.generated_root,
        terminal_book: b.root(),
        settlement: s.root(),
        credit: object_id(b"misaka-palw/native/audit-single-credit/v29", &(s, queue, established)),
        producer_credit: s.producer,
        requester_operator: f.request.requester_operator,
        producer_operator: f.request.producer_operator,
        consumed_nonce: f.request.nonce,
        next_nonce: nonce.next_nonce,
        established_daa: established,
        payout_payload: h.producer_payout.as_bytes(),
        payout_queue: queue.as_bytes(),
        observed_queue_amount: payouts.get(&queue).filter(|p| p.payload == h.producer_payout).map(|p| p.amount),
        rows: audit_rows,
    })
}
impl crate::palw_state_v2::PalwChainStateV2 {
    /// Stateless lifecycle extraction for the independent register/seat auditor
    /// (`derive_native_lifecycle_anchor_v32`); rows come from a verified full opening.
    pub(crate) fn native_text_lifecycle_from_rows_v29(
        rows: &std::collections::BTreeMap<(u8, Vec<u8>), Vec<u8>>,
        stage: crate::palw_native_audit_proof_v32::NativeLifecycleStageV32,
        id: Digest,
    ) -> Result<crate::palw_native_audit_proof_v32::NativeLifecycleAnchorV32, String> {
        use crate::palw_native_audit_proof_v32::{NativeLifecycleAnchorV32, NativeLifecycleStageV32 as S};
        let row = |table: u8, k: Vec<u8>, cap: usize| -> Result<Vec<u8>, String> {
            let raw = rows.get(&(table, k)).ok_or("text29 lifecycle original row absent")?;
            if raw.len() > cap {
                return Err("text29 lifecycle original row cap".into());
            }
            Ok(raw.clone())
        };
        let class_of = |class: Digest| -> Result<ClassHeader, String> {
            let h: ClassHeader =
                borsh::from_slice(&row(EXECUTION_CLASS_TABLE_V11, k(0, &class), 2048)?).map_err(|_| "text29 lifecycle class grammar")?;
            if h.class.id() != class
                || h.original_source.refuted
                || h.original_source.program_root != h.class.original_program
                || h.original_source.parameter_root != h.class.real_parameter
                || h.original_source.inventory_root != h.class.inventory
                || rows.contains_key(&(EXECUTION_SOURCE_FACT_TABLE_V11, k(0, &source_relation(&h.class))))
            {
                return Err("text29 lifecycle class is not bound to a mature unrefuted original source".into());
            }
            Ok(h)
        };
        match stage {
            S::Register => {
                let h = class_of(id)?;
                Ok(NativeLifecycleAnchorV32 {
                    namespace: 29,
                    stage,
                    class: id,
                    source: h.class.source_binding,
                    program: h.class.original_program,
                    maximum_scope: h.class.maximum_scope25,
                    maximum_resource: h.class.maximum_resource29,
                    job: [0; 64],
                    funding: [0; 64],
                    consent: [0; 64],
                    requester_operator: [0; 64],
                    producer_operator: [0; 64],
                    terminal: "registered".into(),
                    anchored_daa: h.registered.max(h.original_source.matures_daa),
                })
            }
            S::Seat => {
                let jh: JobHeader =
                    borsh::from_slice(&row(EXECUTION_JOB_TABLE_V11, k(0, &id), 8192)?).map_err(|_| "text29 lifecycle header grammar")?;
                let raw = row(EXECUTION_JOB_TABLE_V11, k(1, &id), TEXT_STOP_BOOK_BYTES_V29)?;
                if jh.job != id || raw.len() != jh.book_bytes as usize {
                    return Err("text29 lifecycle header/book extent differs".into());
                }
                let b = NativeTextBookV29::restore(&raw, jh.funding, jh.book)?;
                let f = b.funding();
                let h = class_of(jh.class)?;
                if f.id() != id || f.request.class != jh.class || f.consent == [0; 64] || f.maximum.root() != jh.maximum {
                    return Err("text29 lifecycle funded original consent/maximum differs".into());
                }
                Ok(NativeLifecycleAnchorV32 {
                    namespace: 29,
                    stage,
                    class: jh.class,
                    source: h.class.source_binding,
                    program: h.class.original_program,
                    maximum_scope: f.maximum.maximum_scope25,
                    maximum_resource: f.maximum.maximum_resource29,
                    job: id,
                    funding: f.root(),
                    consent: f.consent,
                    requester_operator: f.request.requester_operator,
                    producer_operator: f.request.producer_operator,
                    terminal: format!("{:?}", b.terminal()),
                    anchored_daa: f.admitted_daa,
                })
            }
        }
    }
}
/// Funded action 45 (`palw_window_class_v37`): register a typed window class that
/// adopts `LengthContractV36` with a complete `WindowGraphV37`, sourced from one
/// registered Text29 class. Registration only — no job, reward or Final right.
pub(super) fn register_window37(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    use crate::palw_window_class_v37::*;
    if builder.extras.kernel_route.as_ref().and_then(|e| e.window_class_v37).is_none_or(|at| ctx.daa_score < at) {
        return Ok(()); // below its own fence: unjudged, no state effect
    }
    let text_class: Digest =
        body.get(2..66).ok_or_else(|| refused("window37 class Text29 source prefix absent"))?.try_into().unwrap();
    if body.len() > WINDOW_CLASS_MAX_BODY_BYTES_V37 || !paid_restore(builder, ctx, signer, p, Some(text_class), body.len())? {
        return Ok(());
    }
    let wire = NativeWindowClassRegisterV37::from_bytes(body).map_err(refused)?;
    let (h, _, program) = class(builder, text_class)?;
    misaka_palw_kernel::flat_window37::check_window_graph_program_v37(&wire.graph, &program).map_err(refused)?;
    let record = wire.derive(&h.class, program.history_bound).map_err(refused)?;
    let id = record.id();
    let r = builder.state.kernel_route.as_ref().unwrap();
    let catalog_key = key(&(37u8, 2u8, *signer));
    let mut ids: Vec<Digest> = r
        .aux
        .get(&(EXECUTION_CLASS_TABLE_V11, catalog_key.clone()))
        .filter(|raw| raw.len() <= 4 + 8 * 64)
        .and_then(|raw| borsh::from_slice(raw).ok())
        .unwrap_or_default();
    if ids.len() >= 8 || r.aux.contains_key(&(EXECUTION_CLASS_TABLE_V11, key(&(37u8, 0u8, id)))) {
        return Err(refused("window37 class already registered or finite owner catalogue full"));
    }
    ids.push(id);
    builder.write_kernel_row(EXECUTION_CLASS_TABLE_V11, key(&(37u8, 0u8, id)), Some(key(&(record, *signer, ctx.daa_score))))?;
    builder.write_kernel_row(EXECUTION_CLASS_TABLE_V11, key(&(37u8, 1u8, id)), Some(body.to_vec()))?;
    builder.write_kernel_row(EXECUTION_CLASS_TABLE_V11, catalog_key, Some(key(&ids)))?;
    builder.note_kernel_bond(signer)?;
    Ok(())
}
/// Funded action 46 (`palw_window_class_v37`): the job's PRODUCER opts its Funded,
/// not-yet-committed Text29 job into a registered window class. Producer consent is
/// required because the window courts add liability beyond the original Text29
/// consent; nothing about money, maxima or clocks changes. Fund/Commit/Final/
/// redemption stay exactly Text29's; only courts 3/4 become admissible.
pub(super) fn attach_window37(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV11,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    use crate::palw_window_class_v37::*;
    if builder.extras.kernel_route.as_ref().and_then(|e| e.window_class_v37).is_none_or(|at| ctx.daa_score < at) {
        return Ok(());
    }
    if body.len() != WINDOW_ATTACH_BYTES_V37 || body[..2] != 37u16.to_le_bytes() {
        return Err(refused("window37 attach exact grammar"));
    }
    let job: Digest = body[2..66].try_into().unwrap();
    let window: Digest = body[66..130].try_into().unwrap();
    let r = builder.state.kernel_route.as_ref().unwrap();
    let h = original_header(r, job)?;
    if !paid_restore(builder, ctx, signer, p, Some(h.class), body.len() + h.book_bytes as usize + WINDOW_CLASS_MAX_BODY_BYTES_V37)? {
        return Ok(());
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let b = original_book(r, &h)?;
    let (record, _, _): (NativeWindowClassV37, PalwBondKeyV2, u64) = r
        .aux_row(EXECUTION_CLASS_TABLE_V11, &key(&(37u8, 0u8, window)))
        .ok_or_else(|| refused("window37 attach original window class absent"))?;
    let raw = r.aux.get(&(EXECUTION_CLASS_TABLE_V11, key(&(37u8, 1u8, window)))).ok_or_else(|| refused("window37 class body absent"))?;
    let wire = NativeWindowClassRegisterV37::from_bytes(raw).map_err(refused)?;
    let (ch, _, program) = class(builder, h.class)?;
    if h.producer != *signer
        || b.terminal() != NativeTextTerminalV29::Funded
        || b.claim().is_some()
        || record.id() != window
        || record.text_class != h.class
        || wire.derive(&ch.class, program.history_bound).map_err(refused)? != record
        || b.funding().maximum.maximum_positions > record.maximum_positions
        || builder.state.kernel_route.as_ref().unwrap().aux.contains_key(&(EXECUTION_JOB_TABLE_V11, key(&(37u8, 3u8, job))))
    {
        return Err(refused("window37 attach needs the producer's own Funded uncommitted job of the window's Text29 class"));
    }
    misaka_palw_kernel::flat_window37::check_window_graph_program_v37(&wire.graph, &program).map_err(refused)?;
    wire.check_text_extent(program.history_bound,b.funding().request.maximum_positions().map_err(refused)?)
        .map_err(refused)?;
    // Both window courts must fit the job's ORIGINAL paid filing bound.
    let prep: Prepared = read(builder.state.kernel_route.as_ref().unwrap(), EXECUTION_POLICY_TABLE_V11, 0, &job, (1 << 20) + 8192)
        .ok_or_else(|| refused("window37 attach original preparation absent"))?;
    let q = quote(builder, p, &prep.wire)?;
    let cap = q.paged.record().maximum_bytes as u64;
    let gs = wire.graph.scope().map_err(refused)?;
    let ss = wire.graph.selection.scope().map_err(refused)?;
    let paid = q.paged.record();
    let extra_restore = raw.len() as u64 + 512;
    let extra_work = extra_restore.checked_mul(extra_restore).and_then(|n| n.checked_mul(64))
        .ok_or_else(|| refused("window37 court restore work overflow"))?;
    if gs.filing_bytes + 7 > cap || ss.filing_bytes + 7 > cap
        || gs.court_work.max(ss.court_work).checked_add(extra_work).is_none_or(|n| n > paid.court_work)
        || gs.court_scratch_bytes.max(ss.court_scratch_bytes).checked_add(extra_restore)
            .is_none_or(|n| n > paid.court_scratch)
    {
        return Err(refused("window37 full court/restore envelope exceeds the job's original paid maximum"));
    }
    let binding = NativeWindowJobBindingV37 { version: 37, job, window_class: window, graph: wire.graph.root(), attached_daa: ctx.daa_score };
    builder.write_kernel_row(EXECUTION_JOB_TABLE_V11, key(&(37u8, 3u8, job)), Some(key(&binding)))?;
    builder.note_kernel_bond(signer)?;
    Ok(())
}
/// Re-derive both maximum and actual length commitments from the original
/// registered window class. The job header/book already commits the request
/// and stopped positions; no caller-supplied actual length or quote is used.
fn check_bound_window_lengths(
    r:&PalwKernelRouteStateV1,h:&JobHeader,program_history_bound:u32,actual_positions:u32,
)->Result<(),PalwStateV2Error>{
    if !r.aux.contains_key(&(EXECUTION_JOB_TABLE_V11,key(&(37u8,3u8,h.job)))){return Ok(());}
    let book=original_book(r,h)?;
    check_bound_window_lengths_from_rows(&r.aux,h,&book,Some(program_history_bound),actual_positions)
        .map_err(refused)
}
/// Shared with independent Final/receipt extraction. When a window is present,
/// the history bound comes from the original canonical class, never a report.
fn check_bound_window_lengths_from_rows(
    rows:&std::collections::BTreeMap<(u8,Vec<u8>),Vec<u8>>,h:&JobHeader,book:&NativeTextBookV29,
    program_history_bound:Option<u32>,actual_positions:u32,
)->Result<(),String>{
    use crate::palw_window_class_v37::*;
    let original_key=key(&(37u8,3u8,h.job));
    let Some(raw_binding)=rows.get(&(EXECUTION_JOB_TABLE_V11,original_key)) else{return Ok(());};
    if raw_binding.len()>512{return Err("window37 original binding byte cap".into());}
    let binding:NativeWindowJobBindingV37=borsh::from_slice(raw_binding)
        .map_err(|_|"window37 original binding grammar")?;
    let raw=rows.get(&(EXECUTION_CLASS_TABLE_V11,key(&(37u8,1u8,binding.window_class))))
        .ok_or("window37 original length contract absent")?;
    let wire=NativeWindowClassRegisterV37::from_bytes(raw)?;
    if binding.version!=37||binding.job!=h.job||binding.graph!=wire.graph.root()||wire.text_class!=h.class{
        return Err("window37 original length/graph/job binding differs".into());
    }
    let history=match program_history_bound{
        Some(n)=>n,
        None=>{
            let raw=rows.get(&(EXECUTION_CLASS_TABLE_V11,k(1,&h.class)))
                .ok_or("window37 original canonical text registration absent")?;
            let original=decode_register(raw).map_err(|e|format!("{e:?}"))?;
            let program=TirProgramV1::decode_canonical(&original.program_bytes).map_err(|e|e.to_string())?;
            if misaka_palw_kernel::public::program_root_v1(&program.encode())!=wire.graph.program_root{
                return Err("window37 original canonical program root differs".into());
            }
            misaka_palw_kernel::flat_window37::check_window_graph_program_v37(&wire.graph,&program)?;
            program.history_bound
        }
    };
    wire.check_text_extent(history,book.funding().request.maximum_positions()?)?;
    wire.check_text_extent(history,actual_positions)?;
    Ok(())
}

/// Window court verification for a paid paged filing of a window-bound job. Returns
/// Ok(()) only on an objective continuity/append (kind 3) or v8 selection (kind 4) fault.
fn window_court(
    builder: &TransitionBuilder<'_>,
    h: &JobHeader,
    b: &NativeTextBookV29,
    post: &NativeTextPostV29,
    filing_cap: u32,
    raw: &[u8],
) -> Result<(), PalwStateV2Error> {
    use crate::palw_window_class_v37::*;
    use misaka_palw_kernel::{flat_history8::*, flat_window37::*};
    let r = builder.state.kernel_route.as_ref().unwrap();
    let binding: NativeWindowJobBindingV37 = r
        .aux_row(EXECUTION_JOB_TABLE_V11, &key(&(37u8, 3u8, h.job)))
        .ok_or_else(|| refused("window37 court: job is not window-bound"))?;
    let body = r.aux.get(&(EXECUTION_CLASS_TABLE_V11, key(&(37u8, 1u8, binding.window_class)))).ok_or_else(|| refused("window37 class body absent"))?;
    let graph = NativeWindowClassRegisterV37::from_bytes(body).map_err(refused)?.graph;
    let claim = b.claim().ok_or_else(|| refused("window37 court claim absent"))?;
    if graph.root() != binding.graph || binding.job != h.job {
        return Err(refused("window37 court binding differs"));
    }
    let (kind, filing) = borrowed_window_filing_v37(raw, filing_cap).ok_or_else(|| refused("window37 filing envelope"))?;
    match kind {
        3 => {
            let scope = graph.scope().map_err(refused)?;
            let fault = WindowFaultV37::from_bytes(filing).map_err(|e| refused(e.to_string()))?;
            let c = WindowContextV37 {
                graph: &graph,
                scope: &scope,
                job_root: h.job,
                positions: claim.actual_positions,
                segment_roots: &post.segment_roots,
            };
            verify_window_fault_v37(&c, &fault).map_err(|e| refused(format!("{e:?}")))
        }
        4 => {
            let scope = graph.selection.scope().map_err(refused)?;
            let job = window_selection_job_v37(&graph, h.job, claim.actual_positions, b.funding().request.nonce);
            let fault = HistorySelectionFaultV8::from_bytes(filing).map_err(|e| refused(e.to_string()))?;
            let c = HistorySelectionContextV8 { plan: &graph.selection, job: &job, scope: &scope, segment_roots: &post.segment_roots };
            verify_history_selection_fault_v8(&c, &fault).map_err(|e| refused(format!("{e:?}")))
        }
        _ => Err(refused("window37 unknown court")),
    }
}

// Readonly receipt-window projection. Only audit32-authenticated original rows
// enter this adapter; no returned observation is a consensus authority.
impl crate::palw_state_v2::PalwChainStateV2 {
    pub(crate) fn native_text_receipt_from_rows_v1(
        rows:&std::collections::BTreeMap<(u8,Vec<u8>),Vec<u8>>,id:Digest,
    )->Result<crate::palw_native_receipt_windows_v1::NativeReceiptJobV1,String>{
        use crate::palw_native_audit_proof_v32::NativeLifecycleStageV32;
        let a=Self::native_text_lifecycle_from_rows_v29(rows,NativeLifecycleStageV32::Seat,id)?;
        let header=rows.get(&(EXECUTION_JOB_TABLE_V11,k(0,&id))).ok_or("receipt original text header absent")?;
        if header.len()>8192{return Err("receipt original text header cap".into());}
        let h:JobHeader=borsh::from_slice(header).map_err(|_|"receipt original text header grammar")?;
        let raw=rows.get(&(EXECUTION_JOB_TABLE_V11,k(1,&id))).ok_or("receipt original text book absent")?;
        if raw.len()!=h.book_bytes as usize||raw.len()>TEXT_STOP_BOOK_BYTES_V29{return Err("receipt original text book cap".into());}
        let b=NativeTextBookV29::restore(raw,h.funding,h.book)?;let f=b.funding();
        let actual_positions=match b.claim(){Some(c)=>c.actual_positions,None=>f.request.maximum_positions()?};
        check_bound_window_lengths_from_rows(rows,&h,&b,None,actual_positions)?;
        if b.terminal()!=h.terminal{return Err("receipt text original terminal mismatch".into());}
        Ok(crate::palw_native_receipt_windows_v1::NativeReceiptJobV1{
            namespace:29,complete_model:true,job:id,class:a.class,source:a.source,inventory:f.maximum.inventory,real_parameter:f.maximum.parameter,
            funding:a.funding,consent:a.consent,maximum_contract:f.maximum.root(),maximum_scope:a.maximum_scope,maximum_resource:a.maximum_resource,
            queue_plan:f.maximum.queue_plan,queue_policy:f.maximum.queue_policy,source_plan:f.maximum.source_acquisition,
            physical_plan:f.maximum.physical_lease_plan,carriage_version:29,
            physical_service:{let raw=rows.get(&(EXECUTION_QUOTA_TABLE_V11,key(&(29u8,id,22u8)))).ok_or("receipt text original physical journal absent")?;
                if raw.len()>2048{return Err("receipt physical journal cap".into());}
                let(root,j):(Digest,NativePagedLeaseJournalV29)=borsh::from_slice(raw).map_err(|_|"receipt physical journal grammar")?;
                if j.root()!=root||j.job!=id||j.funding!=f.root(){return Err("receipt physical journal original differs".into());}j.service},
            requester_operator:a.requester_operator,producer_operator:a.producer_operator,nonce:f.request.nonce,
            admitted:f.admitted_daa,commit_deadline:f.commit_deadline,deadline:f.deadline,
            claim:b.claim().map(|c|c.root()),committed:b.claim().map(|c|c.committed_daa),challenge_end:b.claim().map(|c|c.challenge_end),terminal:a.terminal,
        })
    }
}
