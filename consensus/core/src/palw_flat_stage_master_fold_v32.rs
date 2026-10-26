//! Native completion capability32. Kernel output projections prove mathematics;
//! only this private adapter can authenticate original chain/coin provenance.
use super::*;
use misaka_palw_kernel::flat_inputs12::composition32::{
    CheckedComponentOutputV32, CheckedMasterRequestV32, CheckedStageInvocationV32, ComponentOutputContextV32, MasterRequestV32,
    check_component_output_v32, check_master_final_v32, check_stage_invocation_v32,
};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[allow(dead_code)] // capability32 composition is not wired into the stage fold yet
struct NativeInvocationBindingV32 {
    version: u16,
    master: Digest,
    stage: u16,
    job: Digest,
    original_funding: Digest,
    invocation: Digest,
    input_binding: Digest,
    class: Digest,
    source: Digest,
    maximum_contract: Digest,
    original_shared_nonce: Digest,
}
/// This type is not exported and has no decoder/public raw constructor. Its
/// kernel projection cannot be substituted for an authenticated native receipt.
#[allow(dead_code)] // capability32 composition is not wired into the stage fold yet
struct NativeCheckedComponentFinalV32 {
    binding: NativeInvocationBindingV32,
    output: CheckedComponentOutputV32,
    terminal: Digest,
    credit: crate::Hash64,
}
#[expect(dead_code, reason = "capability32 composition is not wired into the stage fold yet")]
fn component_final(
    builder: &TransitionBuilder<'_>,
    master: &CheckedMasterRequestV32,
    invocation: &CheckedStageInvocationV32<'_>,
    job: Digest,
    values: &[misaka_palw_tir::Tensor],
    p: &NativeStagePoliciesV28,
) -> Result<NativeCheckedComponentFinalV32, PalwStateV2Error> {
    let h = original_header(builder, job)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let binding: NativeInvocationBindingV32 =
        read(r, 32, &job, 1024).ok_or_else(|| refused("Master32 original invocation has no authenticated native funding binding"))?;
    if binding.version != 32
        || binding.master != master.root()
        || binding.job != job
        || binding.original_funding != h.original
        || binding.invocation != invocation.invocation_root()
        || binding.input_binding != invocation.input_binding_root()
        || binding.class != h.class
        || binding.source != invocation.source().id()
        || binding.maximum_contract != h.contract.root()
        || binding.original_shared_nonce == [0; 64]
        || master.wire().classes.get(binding.stage as usize) != Some(&h.class)
    {
        return Err(refused("Master32 component native original funding/invocation/class/source/master binding differs"));
    }
    let (_class, _actual, plan, book, data) = restore_job(builder, p, &h)?;
    if book.terminal() != StageTerminalV28::Final || book.settlement().is_none() {
        return Err(refused("Master32 component has no genuine native Final"));
    }
    let settlement = book.settlement().unwrap();
    let queue = settlement_queue(&h, true);
    let (rooted, stored_queue, payload): (StageSettlementV28, crate::Hash64, crate::Hash64) =
        read(r, 14, &job, 1024).ok_or_else(|| refused("Master32 original actual terminal monetary receipt absent"))?;
    if rooted != *settlement
        || stored_queue != queue
        || payload != h.producer_payout
        || settlement.producer_credit != book.reward()
        || settlement.escrow_debit != book.reward()
        || settlement.requester_credit != 0
        || settlement.actual_slash != 0
        || settlement.original != h.original
    {
        return Err(refused("Master32 component actual once-credit receipt does not match genuine original settlement"));
    }
    // A redeemed queue need not remain pending; the immutable original credit
    // receipt survives redemption/GC and fixes its exact amount and recipient.
    if builder
        .state
        .pending_payouts
        .get(&queue)
        .is_some_and(|paid| paid.payload != payload || paid.amount != settlement.producer_credit)
    {
        return Err(refused("Master32 pending producer redemption disagrees with original income"));
    }
    let nonce = r.execution_nonce_counter_v11(&book.requester_operator()).map_err(refused)?;
    if nonce.next_nonce <= misaka_palw_kernel::flat_execution11::canonical_nonce_v11(&book.job().nonce).map_err(refused)?
        || nonce.root()
            != misaka_palw_kernel::flat_execution11::native_requester_nonce_counter_root_v11(
                &book.requester_operator(),
                nonce.next_nonce,
            )
    {
        return Err(refused("Master32 component original native requester nonce is not spent"));
    }
    let trace = StageTraceV14::from_bytes(&data.trace, h.positions).map_err(refused)?;
    let projection = check_component_output_v32(
        invocation,
        &book,
        &plan,
        &trace,
        values,
        &ComponentOutputContextV32 {
            master: master.root(),
            invocation: binding.invocation,
            inputs: binding.input_binding,
            job,
            funding: h.original,
            terminal: book.root(),
            source: binding.source,
            class: h.class,
            claim: book.claim().unwrap(),
            credit_queue: queue.as_bytes(),
            actual_credit: settlement.producer_credit,
            actual_escrow: settlement.escrow_debit,
            producer_operator: book.producer_operator(),
            established: settlement.established,
        },
    )
    .map_err(refused)?;
    Ok(NativeCheckedComponentFinalV32 { binding, output: projection, terminal: book.root(), credit: queue })
}
#[expect(dead_code, reason = "capability32 composition is not wired into the stage fold yet")]
fn compose_final(
    master: &CheckedMasterRequestV32,
    components: Vec<NativeCheckedComponentFinalV32>,
) -> Result<Digest, PalwStateV2Error> {
    if components.len() != master.wire().classes.len()
        || components
            .iter()
            .enumerate()
            .any(|(i, c)| c.binding.master != master.root() || c.binding.stage as usize != i || c.terminal == [0; 64])
        || components.iter().map(|c| c.credit).collect::<std::collections::BTreeSet<_>>().len() != components.len()
    {
        return Err(refused("Master32 native closure omits/reorders a genuine component or duplicates actual credit"));
    }
    let outputs = components.into_iter().map(|c| c.output).collect::<Vec<_>>();
    check_master_final_v32(master, &outputs).map_err(refused)
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct NativeMasterHeaderV32 {
    version: u16,
    id: Digest,
    requester: PalwBondKeyV2,
    requester_operator: Digest,
    source: Digest,
    manifest: Digest,
    nonce: Digest,
    policy: Digest,
    registered: u64,
    body_bytes: u32,
    classes: Vec<Digest>,
    maximum_courts: Vec<Digest>,
    maximum_resources: Vec<Digest>,
    next_stage: u16,
    terminal: Option<Digest>,
}
fn master_key<T: BorshSerialize>(kind: u8, id: &T) -> Vec<u8> {
    bytes(&(32u8, kind, id))
}
fn master_read<T: BorshDeserialize>(r: &PalwKernelRouteStateV1, kind: u8, id: &impl BorshSerialize, cap: usize) -> Option<T> {
    let raw = r.aux.get(&(STAGE_TABLE_V14, master_key(kind, id)))?;
    if raw.len() > cap {
        return None;
    }
    borsh::from_slice(raw).ok()
}
#[expect(dead_code, reason = "capability32 composition is not wired into the stage fold yet")]
fn restore_master(
    builder: &TransitionBuilder<'_>,
    p: &NativeStagePoliciesV28,
    id: Digest,
) -> Result<(CheckedMasterRequestV32, Vec<CheckedStageClassV28>, NativeMasterHeaderV32), PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let h: NativeMasterHeaderV32 = master_read(r, 0, &id, 16384).ok_or_else(|| refused("Master32 original native header absent"))?;
    let raw: Vec<u8> = master_read(r, 1, &id, 2 << 20).ok_or_else(|| refused("Master32 original request absent"))?;
    if h.version != 32 || h.id != id || h.body_bytes as usize != raw.len() || h.policy != p.root() {
        return Err(refused("Master32 original header/policy/extent differs"));
    }
    let wire = MasterRequestV32::from_bytes(&raw).map_err(refused)?;
    let classes = h.classes.iter().map(|id| restore_class(builder, p, *id)).collect::<Result<Vec<_>, _>>()?;
    let bundle_raw: Vec<u8> = read(r, 1, &h.source, 16 << 20).ok_or_else(|| refused("Master32 original complete source absent"))?;
    let (bundle, _) = NativeStageSourceBundleV28::from_bytes(&bundle_raw).map_err(refused)?.check().map_err(refused)?;
    let master = wire.check(&bundle, &classes).map_err(refused)?;
    if master.root() != id
        || wire.complete_source != h.source
        || wire.manifest != h.manifest
        || wire.requester != palw_kernel_bond_id_v1(&h.requester)
        || wire.nonce != h.nonce
        || wire.classes != h.classes
        || h.maximum_courts.len() != classes.len()
        || h.maximum_resources.len() != classes.len()
    {
        return Err(refused("Master32 original full source/classes/requester/nonce roots differ"));
    }
    for (i, class) in classes.iter().enumerate() {
        let scope = misaka_palw_kernel::flat_inputs12::court32::derive_stage_court_scope_v32(class).map_err(refused)?;
        if scope.root() != h.maximum_courts[i] || class.maximum_resources().root() != h.maximum_resources[i] {
            return Err(refused("Master32 independently derived original maximum component courts/resources differ"));
        }
    }
    Ok((master, classes, h))
}
/// Original master declaration pays its own complete restoration and finite
/// metadata catalogue. Producer resources are reserved only by later exact
/// producer-consented child funding; this registration grants none.
pub(super) fn register(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeStagePoliciesV28,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    if body.len() < 130 || body.len() > 2 << 20 || body[..2] != 32u16.to_le_bytes() {
        return Err(refused("Master32 initial bounded request grammar"));
    }
    let source: Digest = body[2..66].try_into().unwrap();
    let r = builder.state.kernel_route.as_ref().unwrap();
    let bh: BundleHeader = read(r, 0, &source, 8192).ok_or_else(|| refused("Master32 original all-component source header absent"))?;
    if !pay_restore(builder, ctx, signer, p, bh.bytes, 1 << 20, 1 << 20, body.len() as u32)? {
        return Ok(());
    }
    let wire = MasterRequestV32::from_bytes(body).map_err(refused)?;
    if wire.requester != palw_kernel_bond_id_v1(signer) {
        return Err(refused("Master32 original requester signature differs"));
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let actor = builder
        .state
        .bonds
        .get(signer)
        .filter(|b| matches!(b.status, PalwBondStatusV2::Active))
        .ok_or_else(|| refused("Master32 original requester is not active"))?;
    let operator = actor.operator_id.as_bytes();
    let nonce = r.execution_nonce_counter_v11(&operator).map_err(refused)?;
    nonce.advance(&wire.nonce).map_err(refused)?;
    let mut catalog: Vec<Digest> = master_read(r, 2, signer, 4 + 8 * 64).unwrap_or_default();
    if catalog.len() >= 8 {
        return Err(refused("Master32 original requester finite metadata catalogue full"));
    }
    let bundle_raw: Vec<u8> = read(r, 1, &source, 16 << 20).ok_or_else(|| refused("Master32 original complete bundle body absent"))?;
    let (bundle, _) = NativeStageSourceBundleV28::from_bytes(&bundle_raw).map_err(refused)?.check().map_err(refused)?;
    let classes = wire.classes.iter().map(|id| restore_class(builder, p, *id)).collect::<Result<Vec<_>, _>>()?;
    let master = wire.check(&bundle, &classes).map_err(refused)?;
    let id = master.root();
    if master_read::<NativeMasterHeaderV32>(builder.state.kernel_route.as_ref().unwrap(), 0, &id, 16384).is_some() {
        return Err(refused("Master32 original requester declaration already registered"));
    }
    let courts = classes
        .iter()
        .map(misaka_palw_kernel::flat_inputs12::court32::derive_stage_court_scope_v32)
        .collect::<Result<Vec<_>, _>>()
        .map_err(refused)?;
    let h = NativeMasterHeaderV32 {
        version: 32,
        id,
        requester: *signer,
        requester_operator: operator,
        source,
        manifest: bundle.manifest().root(),
        nonce: wire.nonce,
        policy: p.root(),
        registered: ctx.daa_score,
        body_bytes: body.len() as u32,
        classes: wire.classes.clone(),
        maximum_courts: courts.iter().map(|c| c.root()).collect(),
        maximum_resources: classes.iter().map(|c| c.maximum_resources().root()).collect(),
        next_stage: 0,
        terminal: None,
    };
    catalog.push(id);
    builder.write_kernel_row(STAGE_TABLE_V14, master_key(0, &id), Some(bytes(&h)));
    builder.write_kernel_row(STAGE_TABLE_V14, master_key(1, &id), Some(bytes(&body.to_vec())));
    builder.write_kernel_row(STAGE_TABLE_V14, master_key(2, signer), Some(bytes(&catalog)));
    builder.note_kernel_bond(signer);
    Ok(())
}

// ---- Audit-time composite Final (lead default 2026-10-15, pending user review) ----
//
// The Master32 composite is NOT a consensus object: no carrier, row or credit is
// created. It is a deterministic function of public material only — the
// authenticated original aux rows of one pinned state (audit32's full opening),
// the owning ruleset's immutable Stage28 policy, the public master decode tokens
// and each component's public output tensors, which must authenticate against
// that component's own committed trace openings. Every component must already be
// an individually genuine, settled Stage28 Final; nothing here pays or finalizes.
pub(super) type AuditRows = std::collections::BTreeMap<(u8, Vec<u8>), Vec<u8>>;
fn aux_read<T: BorshDeserialize>(rows: &AuditRows, raw_key: Vec<u8>, cap: usize) -> Result<T, String> {
    let raw = rows.get(&(STAGE_TABLE_V14, raw_key)).ok_or("master32 audit original row absent")?;
    if raw.len() > cap {
        return Err("master32 audit original row cap".into());
    }
    borsh::from_slice(raw).map_err(|_| "master32 audit original row grammar".into())
}
pub(super) fn audit_class(rows: &AuditRows, p: &NativeStagePoliciesV28, id: Digest) -> Result<CheckedStageClassV28, String> {
    let h: ClassHeader = aux_read(rows, key(2, &id), 1024)?;
    let bh: BundleHeader = aux_read(rows, key(0, &h.bundle), 8192)?;
    let raw: Vec<u8> = aux_read(rows, key(1, &h.bundle), (16 << 20) + 8)?;
    if raw.len() != bh.bytes as usize {
        return Err("master32 audit original source byte count".into());
    }
    let (manifest, sources) = NativeStageSourceBundleV28::from_bytes(&raw)?.check()?;
    let wire_raw: Vec<u8> = aux_read(rows, key(3, &id), (1 << 20) + 8)?;
    let wire = FlatStageClassWireV28::from_bytes(&wire_raw)?;
    let source = sources.get(wire.stage as usize).ok_or("master32 audit source stage absent")?;
    let mut class = wire.check(source)?;
    class.bind_complete_bundle(&manifest)?;
    let refuted = rows.contains_key(&(STAGE_TABLE_V14, crate::palw_flat_stage_v14::stage_key_v14(14, &source.relation_root())));
    if h.version != 28
        || h.id != id
        || h.bundle != manifest.root()
        || h.source != source.id()
        || h.policy != p.root()
        || wire.policy != p.root()
        || class.root() != id
        || h.scope != class.maximum_scope().root()
        || h.resource != class.maximum_resources().root()
        || h.bytes as usize != wire_raw.len()
        || refuted
    {
        return Err("master32 audit original source/class/manifest/maxima differ or refuted".into());
    }
    Ok(class)
}
/// Returns the composite digest of `check_master_final_v32` and every component's
/// genuine-final root, in stage order. `jobs[i]`/`values[i]` belong to stage i.
pub(crate) fn audit_master_composite_from_rows_v32(
    rows: &AuditRows,
    p: &NativeStagePoliciesV28,
    master_id: Digest,
    jobs: &[Digest],
    values: &[Vec<misaka_palw_tir::Tensor>],
    generated: &[u32],
) -> Result<(Digest, Vec<Digest>), String> {
    let h: NativeMasterHeaderV32 = aux_read(rows, master_key(0, &master_id), 16384)?;
    let raw: Vec<u8> = aux_read(rows, master_key(1, &master_id), (2 << 20) + 8)?;
    if h.version != 32 || h.id != master_id || h.body_bytes as usize != raw.len() || h.policy != p.root() {
        return Err("master32 audit original header/policy/extent differs".into());
    }
    let wire = MasterRequestV32::from_bytes(&raw)?;
    let classes = h.classes.iter().map(|id| audit_class(rows, p, *id)).collect::<Result<Vec<_>, _>>()?;
    let bh: BundleHeader = aux_read(rows, key(0, &h.source), 8192)?;
    let bundle_raw: Vec<u8> = aux_read(rows, key(1, &h.source), (16 << 20) + 8)?;
    if bundle_raw.len() != bh.bytes as usize {
        return Err("master32 audit original complete source byte count".into());
    }
    let (bundle, _) = NativeStageSourceBundleV28::from_bytes(&bundle_raw)?.check()?;
    let master = wire.check(&bundle, &classes)?;
    if master.root() != master_id
        || wire.classes != h.classes
        || jobs.len() != classes.len()
        || values.len() != classes.len()
        || generated.len() > 1 << 20
    {
        return Err("master32 audit request/classes/component count differs".into());
    }
    let mut finals = Vec::with_capacity(classes.len());
    for (stage, class) in classes.iter().enumerate() {
        let job = jobs[stage];
        let jh: JobHeader = aux_read(rows, key(5, &job), 8192)?;
        let data: JobData = aux_read(rows, key(6, &job), (1 << 20) + (2 << 20) + 16384)?;
        if jh.id != job
            || jh.class != class.root()
            || data.book.len() != jh.book_bytes as usize
            || data.trace.len() != jh.trace_bytes as usize
        {
            return Err("master32 audit component header/class/extent differs".into());
        }
        let book: StageJobBookV28 = borsh::from_slice(&data.book).map_err(|_| "master32 audit component book grammar")?;
        let actual = class.check_job(book.job())?;
        let plan = derive_stage_quota_v28(class, &actual)?;
        book.check(class, &actual, &plan)?;
        let s = book.settlement().ok_or("master32 audit component settlement absent")?;
        let (receipt, queue, payload): (StageSettlementV28, crate::Hash64, crate::Hash64) = aux_read(rows, key(14, &job), 1024)?;
        if book.id() != job
            || book.root() != jh.book
            || book.terminal() != StageTerminalV28::Final
            || receipt != *s
            || queue != settlement_queue(&jh, true)
            || payload != jh.producer_payout
        {
            return Err("master32 audit component is not an individually genuine settled Final".into());
        }
        let invocation = check_stage_invocation_v32(&master, class, book.job(), generated, &finals)?;
        let trace = StageTraceV14::from_bytes(&data.trace, jh.positions)?;
        let projection = check_component_output_v32(
            &invocation,
            &book,
            &plan,
            &trace,
            &values[stage],
            &ComponentOutputContextV32 {
                master: master.root(),
                invocation: invocation.invocation_root(),
                inputs: invocation.input_binding_root(),
                job,
                funding: jh.original,
                terminal: book.root(),
                source: invocation.source().id(),
                class: jh.class,
                claim: book.claim().ok_or("master32 audit component claim absent")?,
                credit_queue: queue.as_bytes(),
                actual_credit: s.producer_credit,
                actual_escrow: s.escrow_debit,
                producer_operator: book.producer_operator(),
                established: s.established,
            },
        )?;
        finals.push(projection);
    }
    let roots = finals.iter().map(|f| f.root()).collect();
    Ok((check_master_final_v32(&master, &finals)?, roots))
}

impl crate::palw_state_v2::PalwChainStateV2 {
    /// Stateless audit entry (no `self`): only `palw_native_audit_proof_v32::derive_native_master_composite_v32`
    /// calls this, with rows from a verified full aux opening. Not a consensus transition.
    pub(crate) fn native_master_composite_from_rows_v32(
        rows: &AuditRows,
        p: &NativeStagePoliciesV28,
        master: Digest,
        jobs: &[Digest],
        values: &[Vec<misaka_palw_tir::Tensor>],
        generated: &[u32],
    ) -> Result<(Digest, Vec<Digest>), String> {
        audit_master_composite_from_rows_v32(rows, p, master, jobs, values, generated)
    }
}
