//! Native39 original maximum/actual-EOS journal. All expensive restoration is
//! preceded by a current ordinary or original producer reservation debit.
use super::*;
use crate::{
    palw_flat_execution_capacity_v22::*,
    palw_kernel_route_v1::{PalwKernelRouteStateV1, palw_kernel_bond_id_v1},
    palw_native_execution_v39::*,
    palw_native_job_v39::*,
    palw_native_queue_v30::*,
};
use crate::palw_native_liability_hold_v1::{
    PALW_NATIVE_FAMILY_JOB_V39, PALW_NATIVE_LIABILITY_HOLD_ROW_VERSION_V1, PALW_NATIVE_LIABILITY_HOLD_TABLE_V1,
    PalwNativeHeldFeeRowV1, palw_native_held_fee_key_v1,
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{
    flat_job39::*,
    flat_material39::*,
    flat_quota11::{
        owning39::{source::*, *},
        *,
    },
    flat_resources10::{ResourceAxisV10 as A, ResourceQuantitiesV10},
    hash::{Digest, object_id},
};
const TABLE: u8 = 112;
const QUOTA: u8 = 92;
const NONCE: u8 = 89;
fn bad(s: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(s.into())
}
fn b<T: BorshSerialize>(v: &T) -> Vec<u8> {
    borsh::to_vec(v).expect("native39 bounded canonical row")
}
fn k<T: BorshSerialize>(kind: u8, id: &T) -> Vec<u8> {
    b(&(39u8, kind, id))
}
fn read<T: BorshDeserialize>(
    r: &PalwKernelRouteStateV1,
    kind: u8,
    id: &impl BorshSerialize,
    cap: usize,
) -> Result<T, PalwStateV2Error> {
    let raw =
        r.aux.get(&(TABLE, k(kind, id))).filter(|raw| raw.len() <= cap).ok_or_else(|| bad("native39 original bounded row absent"))?;
    borsh::from_slice(raw).map_err(|_| bad("native39 original row grammar"))
}
fn list(r: &PalwKernelRouteStateV1, kind: u8, id: &PalwBondKeyV2) -> Result<Vec<Digest>, PalwStateV2Error> {
    match r.aux.get(&(TABLE, k(kind, id))) {
        None => Ok(vec![]),
        Some(raw) => {
            if raw.len() > 4 + 8 * 64 {
                return Err(bad("native39 catalog extent"));
            }
            let ids: Vec<Digest> = borsh::from_slice(raw).map_err(|_| bad("native39 catalog grammar"))?;
            if ids.len() > 8 || ids.iter().enumerate().any(|(i, x)| *x == [0; 64] || ids[..i].contains(x)) {
                return Err(bad("native39 catalog membership"));
            }
            Ok(ids)
        }
    }
}
fn epoch(p: &NativeExecutionPoliciesV39, ctx: &PalwBlockContextV2) -> Result<TrustedEpochV11, PalwStateV2Error> {
    TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(bad)
}
fn nonce(requester: Digest, row: &crate::palw_flat_execution_v11::NativeRequesterNonceRowV11) -> NativeRequesterNonceWitnessV11 {
    NativeRequesterNonceWitnessV11 {
        requester,
        requester_operator_id: row.operator,
        counter_root: row.root(),
        next_nonce: row.next_nonce,
    }
}
fn prepay(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    restore: u64,
    body: usize,
    held: u64,
) -> Result<bool, PalwStateV2Error> {
    let bytes = restore.checked_add(body as u64).ok_or_else(|| bad("native39 original restore extent overflow"))?;
    if bytes > 128 << 20 || held > 64 << 20 {
        return Err(bad("native39 original metadata envelope exceeded"));
    }
    let work = bytes.checked_mul(8192).and_then(|n| n.checked_add(65536)).ok_or_else(|| bad("native39 restore work overflow"))?;
    let Some(fee) = super::palw_flat_model_vector_fold_v3::charge(builder, ctx, signer, work, false)? else { return Ok(false) };
    builder.slash_bond(*signer, fee)?;
    let mut q = ResourceQuantitiesV10::default();
    for (a, v) in [
        (A::MetadataWork, work as u128),
        (A::ScalarSteps, work as u128),
        (A::IndexSteps, bytes as u128 * 64),
        (A::ValidatedElements, bytes as u128),
        (A::CopiedReferenceBytes, bytes as u128 * 8),
        (A::OwnedPeakBytes, bytes as u128 * 12 + held as u128 + 65536),
        (A::CourtScratchBytes, bytes as u128 * 12 + 65536),
        (A::StoredEvidenceBytes, held as u128),
        (A::ContainerAllocatorAllowance, bytes as u128 * 4 + 65536),
        (A::Attempts, 1),
    ] {
        q.set(a, v);
    }
    let e = epoch(p, ctx)?;
    let mut s = super::palw_flat_execution_fold_v11::quota_shard(builder, &p.execution, &e, palw_kernel_bond_id_v1(signer), [0; 64])?;
    s.debit_ordinary(&p.execution.quota, &e, &DerivedOrdinaryMetadataV11::from_native_envelope((&q).into())).map_err(bad)?;
    super::palw_flat_execution_fold_v11::write_shard(builder, &s, [0; 64])?;
    Ok(true)
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct ConsentHeader {
    version: u16,
    id: Digest,
    class: Digest,
    source_bytes: u32,
    body_bytes: u32,
    body: Digest,
    wire: Digest,
    producer: PalwBondKeyV2,
    requester: PalwBondKeyV2,
    accepted: u64,
    expires: u64,
}
impl ConsentHeader {
    fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/producer-consent-header/v39", self)
    }
}
fn consent_header(r: &PalwKernelRouteStateV1, id: Digest) -> Result<ConsentHeader, PalwStateV2Error> {
    let (root, h): (Digest, ConsentHeader) = read(r, 18, &id, 1024)?;
    if h.version != 39
        || h.id != id
        || h.root() != root
        || h.body_bytes as usize > NativeConsentWireV39::MAX_BYTES + 16384
        || h.body_bytes == 0
    {
        return Err(bad("native39 original consent fixed header differs"));
    }
    Ok(h)
}
fn original_consent(r: &PalwKernelRouteStateV1, h: &ConsentHeader) -> Result<NativeConsentV39, PalwStateV2Error> {
    let raw: Vec<u8> = read(r, 26, &h.id, NativeConsentWireV39::MAX_BYTES + 16388)?;
    if raw.len() != h.body_bytes as usize || object_id(b"misaka-palw/native/original-consent-body/v39", &raw) != h.body {
        return Err(bad("native39 original consent body differs"));
    }
    let c: NativeConsentV39 = borsh::from_slice(&raw).map_err(|_| bad("native39 original consent grammar"))?;
    if c.root() != h.id
        || c.producer != h.producer
        || c.wire.requester != h.requester
        || c.wire.class != h.class
        || c.wire.expires != h.expires
    {
        return Err(bad("native39 original consent header relation differs"));
    }
    Ok(c)
}
fn quote(
    builder: &TransitionBuilder<'_>,
    p: &NativeExecutionPoliciesV39,
    w: &NativeConsentWireV39,
) -> Result<(CheckedNativeClassV39, CheckedJobMaximumV39, CheckedQuotaV39, CheckedNativeMaximumV39), PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let class = super::palw_native_source_fold_v39::restore_class(r, p, w.class)?;
    let maximum = JobMaximumV39::from_bytes(&w.maximum).map_err(bad)?;
    let material = MaterialManifestV39::from_bytes(&w.material).map_err(bad)?;
    let maximum = CheckedJobMaximumV39::check(class.source(), &maximum, &material).map_err(bad)?;
    if maximum.request().requester != palw_kernel_bond_id_v1(&w.requester)
        || w.reward == 0
        || w.reward > p.execution.execution.reward_cap
    {
        return Err(bad("native39 original requester/max/reward differs"));
    }
    let quota = derive_quota_v39(class.source(), &maximum, class.root(), p.root(), class.wire().max_positions, 32).map_err(bad)?;
    let e = builder.extras.kernel_route.as_ref().unwrap();
    let max = CheckedNativeMaximumV39::derive(
        &class,
        &maximum,
        &quota,
        p,
        &e.native_final.as_ref().unwrap().policy,
        &r.header.policy,
        e.native_carriage_mass.as_ref().ok_or_else(|| bad("native39 original mass absent"))?,
        e.native_carriage_service.as_ref().ok_or_else(|| bad("native39 original service absent"))?,
    )
    .map_err(bad)?;
    if max.record().root() != w.maximum_contract {
        return Err(bad("native39 producer did not sign complete original resource/temporal/economic/queue contract"));
    }
    Ok((class, maximum, quota, max))
}
fn accept(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    if body.len() < 66 || body.len() > NativeConsentWireV39::MAX_BYTES {
        return Err(bad("native39 original Accept extent"));
    }
    let class_id: Digest = body[2..66].try_into().unwrap();
    let ch = super::palw_native_source_fold_v39::class_header(builder.state.kernel_route.as_ref().unwrap(), class_id)?;
    let mut ids = list(builder.state.kernel_route.as_ref().unwrap(), 19, signer)?;
    ids.retain(|id| consent_header(builder.state.kernel_route.as_ref().unwrap(), *id).is_ok_and(|h| h.expires >= ctx.daa_score));
    if ids.len() >= p.catalog_cap.min(8) as usize {
        return Err(bad("native39 producer-owned consent catalog full"));
    }
    if !prepay(builder, ctx, signer, p, ch.source_bytes as u64, body.len(), body.len() as u64 + 16384)? {
        return Ok(());
    }
    let w = NativeConsentWireV39::from_bytes(body).map_err(bad)?;
    let (class, maximum, _, contract) = quote(builder, p, &w)?;
    let invocation = super::palw_native_master_fold_v39::authenticate_invocation(builder, ctx, signer, p, &class, &maximum, w.reward)?;
    if let Some(invocation) = &invocation {
        material::authenticate_projections(
            builder.state.kernel_route.as_ref().unwrap(),
            maximum.input_manifest(),
            invocation.projections(),
        )?;
    }
    let producer = builder.state.bonds.get(signer).ok_or_else(|| bad("native39 original producer absent"))?;
    let requester = builder.state.bonds.get(&w.requester).ok_or_else(|| bad("native39 original requester absent"))?;
    if !matches!(producer.status, PalwBondStatusV2::Active)
        || !matches!(requester.status, PalwBondStatusV2::Active)
        || maximum.request().producer != palw_kernel_bond_id_v1(signer)
        || w.producer_operator != producer.operator_id.as_bytes()
        || w.requester_operator != requester.operator_id.as_bytes()
        || w.producer_operator == w.requester_operator
        || w.expires < ctx.daa_score
        || w.expires
            > ctx
                .daa_score
                .checked_add(contract.record().temporal.commit_window)
                .ok_or_else(|| bad("native39 consent clock overflow"))?
        || w.expires / p.execution.quota.interval_daa != ctx.daa_score / p.execution.quota.interval_daa
    {
        return Err(bad("native39 producer-only consent changes original operators/epoch/max source"));
    }
    builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .execution_nonce_counter_v11(&w.requester_operator)
        .map_err(bad)?
        .advance(&maximum.request().nonce)
        .map_err(bad)?;
    authenticate_material(
        builder.state.kernel_route.as_ref().unwrap(),
        maximum.input_manifest(),
        &w.requester,
        w.requester_operator,
        maximum.request().nonce,
        ctx.daa_score,
    )?;
    let c = NativeConsentV39 {
        wire: w,
        producer: *signer,
        accepted: ctx.daa_score,
        interval: ctx.daa_score / p.execution.quota.interval_daa,
        checked_maximum: contract.record().clone(),
    };
    let id = c.root();
    if ids.contains(&id) {
        return Err(bad("native39 consent replay"));
    }
    ids.push(id);
    let raw = b(&c);
    let h = ConsentHeader {
        version: 39,
        id,
        class: class_id,
        source_bytes: ch.source_bytes,
        body_bytes: raw.len() as u32,
        body: object_id(b"misaka-palw/native/original-consent-body/v39", &raw),
        wire: c.wire.root(),
        producer: *signer,
        requester: c.wire.requester,
        accepted: c.accepted,
        expires: c.wire.expires,
    };
    builder.write_kernel_row(TABLE, k(18, &id), Some(b(&(h.root(), h))))?;
    builder.write_kernel_row(TABLE, k(26, &id), Some(b(&raw)))?;
    builder.write_kernel_row(TABLE, k(19, signer), Some(b(&ids)))?;
    super::palw_native_source_fold_v39::retain_class(
        builder,
        class_id,
        super::palw_native_source_fold_v39::HOLD_CONSENT,
        id,
        c.wire.expires,
    )?;
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(super) struct JobHeader {
    pub(super) version: u16,
    pub(super) id: Digest,
    pub(super) original: NativeJobOriginalV39,
    pub(super) book: Digest,
    pub(super) book_bytes: u32,
    pub(super) consent_bytes: u32,
    pub(super) class_bytes: u32,
    pub(super) source_bytes: u32,
    pub(super) post_bytes: u32,
    pub(super) plan: QuotaPlanRecordV11,
    pub(super) plan_root: Digest,
    pub(super) terminal: NativeJobTerminalV39,
    pub(super) requester_payout: Hash64,
    pub(super) producer_payout: Hash64,
}
impl JobHeader {
    fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-funded-job-header/v39", self)
    }
}
pub(super) fn header(r: &PalwKernelRouteStateV1, id: Digest) -> Result<JobHeader, PalwStateV2Error> {
    let (root, h): (Digest, JobHeader) = read(r, 16, &id, 16384)?;
    if h.version != 39
        || h.id != id
        || h.original.job() != id
        || h.root() != root
        || h.book_bytes > 16384
        || h.plan.root() != h.plan_root
        || h.plan_root != h.original.shared_plan
    {
        return Err(bad("native39 original funded fixed header differs"));
    }
    Ok(h)
}
pub(super) fn book(r: &PalwKernelRouteStateV1, h: &JobHeader) -> Result<NativeJobBookV39, PalwStateV2Error> {
    let raw = r
        .aux
        .get(&(TABLE, k(17, &h.id)))
        .filter(|raw| raw.len() == h.book_bytes as usize)
        .ok_or_else(|| bad("native39 original book extent absent"))?;
    let book: NativeJobBookV39 = borsh::from_slice(raw).map_err(|_| bad("native39 original book grammar"))?;
    if book.root() != h.book || book.id() != h.id || book.original() != &h.original || book.terminal() != h.terminal {
        return Err(bad("native39 original book relation differs"));
    }
    Ok(book)
}
fn write_book(builder: &mut TransitionBuilder<'_>, h: &mut JobHeader, book: &NativeJobBookV39) -> Result<(), PalwStateV2Error> {
    let raw = b(book);
    h.book = book.root();
    h.book_bytes = raw.len() as u32;
    h.terminal = book.terminal();
    builder.write_kernel_row(TABLE, k(17, &h.id), Some(raw))?;
    builder.write_kernel_row(TABLE, k(16, &h.id), Some(b(&(h.root(), &*h))))?;
    Ok(())
}
pub(super) fn reserved(r: &PalwKernelRouteStateV1, bond: &PalwBondKeyV2) -> u128 {
    list(r, 22, bond)
        .unwrap_or_default()
        .iter()
        .filter_map(|id| header(r, *id).ok())
        .filter(|h| {
            h.original.producer == *bond
                && (matches!(h.terminal, NativeJobTerminalV39::Funded | NativeJobTerminalV39::Challengeable)
                    || (h.terminal == NativeJobTerminalV39::Final
                        && r.aux_row::<PalwNativeHeldFeeRowV1>(
                            PALW_NATIVE_LIABILITY_HOLD_TABLE_V1,
                            &palw_native_held_fee_key_v1(PALW_NATIVE_FAMILY_JOB_V39, &h.id),
                        ).is_some_and(|row| row.version == PALW_NATIVE_LIABILITY_HOLD_ROW_VERSION_V1 && row.family == PALW_NATIVE_FAMILY_JOB_V39 && row.job == h.id && row.producer == *bond)))
        })
        .fold(0u128, |n, h| n.saturating_add(h.original.reserve as u128))
        .saturating_add(public::reserved(r, bond))
}
fn write_source(builder: &mut TransitionBuilder<'_>, s: &SourceShardV39, id: Digest) -> Result<(), PalwStateV2Error> {
    let g = s.global_row();
    let a = s.producer_row();
    builder.write_kernel_row(QUOTA, vec![0], Some(b(&(g.root(), g))))?;
    builder.write_kernel_row(QUOTA, b(&(1u8, a.actor)), Some(b(&(a.root(), a))))?;
    if let Some(r) = s.source_row() {
        builder.write_kernel_row(QUOTA, b(&(39u8, 0u8, id)), Some(b(&(r.root(), r))))?;
    }
    Ok(())
}
fn fund(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    let f = NativeFundWireV39::from_bytes(body).map_err(bad)?;
    let h = consent_header(builder.state.kernel_route.as_ref().unwrap(), f.consent)?;
    if h.requester != *signer || ctx.daa_score < h.accepted || ctx.daa_score > h.expires {
        return Err(bad("native39 original signed requester/consent clock differs"));
    }
    if !prepay(builder, ctx, signer, p, h.source_bytes as u64 + h.body_bytes as u64, body.len(), 32768)? {
        return Ok(());
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let c = original_consent(r, &h)?;
    let (class, maximum, quota, contract) = quote(builder, p, &c.wire)?;
    let invocation =
        super::palw_native_master_fold_v39::authenticate_invocation(builder, ctx, signer, p, &class, &maximum, c.wire.reward)?;
    if let Some(invocation) = &invocation {
        material::authenticate_projections(
            builder.state.kernel_route.as_ref().unwrap(),
            maximum.input_manifest(),
            invocation.projections(),
        )?;
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let requester = builder.state.bonds.get(signer).ok_or_else(|| bad("native39 requester absent"))?;
    let producer = builder.state.bonds.get(&c.producer).ok_or_else(|| bad("native39 producer absent"))?;
    if !matches!(requester.status, PalwBondStatusV2::Active)
        || !matches!(producer.status, PalwBondStatusV2::Active)
        || c.interval != ctx.daa_score / p.execution.quota.interval_daa
        || c.wire.requester_operator != requester.operator_id.as_bytes()
        || c.wire.producer_operator != producer.operator_id.as_bytes()
        || c.checked_maximum != *contract.record()
        || c.root() != f.consent
        || r.aux.contains_key(&(TABLE, k(20, &f.consent)))
    {
        return Err(bad("native39 Fund changes original producer consent/principal/operator/contract or replays consent"));
    }
    authenticate_material(
        r,
        maximum.input_manifest(),
        signer,
        requester.operator_id.as_bytes(),
        maximum.request().nonce,
        ctx.daa_score,
    )?;
    let next =
        r.execution_nonce_counter_v11(&c.wire.requester_operator).map_err(bad)?.advance(&maximum.request().nonce).map_err(bad)?;
    let reward = c.wire.reward;
    let reserve = p.execution.execution.required_producer_reserve;
    if (requester.collateral as u128).saturating_sub(builder.committed_at(signer, ctx.daa_score)) < reward as u128
        || (producer.collateral as u128).saturating_sub(builder.committed_at(&c.producer, ctx.daa_score)) < reserve as u128
    {
        return Err(bad("native39 actual escrow or collectible original producer reserve unavailable"));
    }
    let mut requesters = list(r, 21, signer)?;
    let mut producers = list(r, 22, &c.producer)?;
    let mut consents = list(r, 19, &c.producer)?;
    if requesters.len() >= p.catalog_cap.min(8) as usize
        || producers.len() >= p.catalog_cap.min(8) as usize
        || !consents.contains(&f.consent)
    {
        return Err(bad("native39 original finite funded slots or consent membership unavailable"));
    }
    let temporal = &contract.record().temporal;
    let deadline = temporal.deadline(ctx.daa_score, p.execution.quota.interval_daa).map_err(bad)?;
    material::preflight_attach(r, maximum.input_manifest(), deadline)?;
    if maximum.input_manifest().master != [0; 64]
        && deadline > super::palw_native_master_fold_v39::record(r, maximum.input_manifest().master)?.deadline
    {
        return Err(bad("native39 complete original component opportunity exceeds its original Master horizon"));
    }
    let receipt = object_id(
        b"misaka-palw/native/original-collected-escrow/v39",
        &(ctx.block.as_bytes(), *signer, c.root(), maximum.root(), reward),
    );
    let original = NativeJobOriginalV39 {
        version: 39,
        class: class.root(),
        source: class.source().root(),
        policy: p.root(),
        maximum: maximum.root(),
        class_scope: class.scope().root(),
        class_resources: class.resources().root(),
        maximum_scope: maximum.resources().scope().root(),
        maximum_resources: maximum.resources().root(),
        maximum_positions: maximum.maximum_positions(),
        input_manifest: maximum.input_manifest().root(),
        quota: quota.root(),
        shared_plan: quota.plan().record().root(),
        source_plan: quota.source().root(),
        paged_plan: quota.paged().root(),
        consent: c.root(),
        maximum_contract: contract.record().root(),
        transport: contract.record().transport,
        queue: contract.record().queue,
        physical: p.physical_service,
        final_policy: p.final_policy,
        final_authority: contract.conditions().authority_root(),
        final_basis: contract.conditions().basis_root(),
        requester: *signer,
        producer: c.producer,
        requester_operator: c.wire.requester_operator,
        producer_operator: c.wire.producer_operator,
        nonce: maximum.request().nonce,
        reward,
        reserve,
        finder: u64::try_from(reserve as u128 * p.execution.execution.finder_permille as u128 / 1000)
            .map_err(|_| bad("native39 finder range"))?,
        actual_debit: reward,
        debit_receipt: receipt,
        admitted_block: ctx.block.as_bytes(),
        admitted: ctx.daa_score,
        interval: ctx.daa_score / p.execution.quota.interval_daa,
        commit_until: ctx.daa_score.checked_add(temporal.commit_window).ok_or_else(|| bad("native39 commit clock overflow"))?,
        challenge_window: temporal.challenge_window,
        final_tail: temporal.final_tail,
        deadline,
    };
    let id = original.job();
    if r.aux.contains_key(&(TABLE, k(16, &id))) || r.aux.contains_key(&(TABLE, k(24, &id))) {
        return Err(bad("native39 original job replay"));
    }
    let e = epoch(p, ctx)?;
    let witness = nonce(palw_kernel_bond_id_v1(signer), &next);
    let grant = check_quota_admission_v39(&quota, &original.funding_anchor(), &p.execution.quota, &witness, &e).map_err(bad)?;
    let mut baseline =
        super::palw_flat_execution_fold_v11::quota_shard(builder, &p.execution, &e, palw_kernel_bond_id_v1(&c.producer), id)?;
    baseline.admit_baseline(&p.execution.quota, &e, &grant, quota.plan()).map_err(bad)?;
    let sg = check_source_admission_v39(&quota, &original.funding_anchor(), &p.execution.quota, &witness, &e).map_err(bad)?;
    let g = baseline.global_row();
    let a = baseline.actor_row();
    let s = baseline.subject_row().unwrap();
    let mut source =
        SourceShardV39::from_native_rows(&p.execution.quota, &quota, &g, &g.root(), (&a, &a.root()), (s, &s.row_root()), None)
            .map_err(bad)?;
    source.admit(&p.execution.quota, &e, &quota, &sg).map_err(bad)?;
    for _ in 0..quota.source().preclaim_units {
        source.admit_valid_unit(&p.execution.quota, &e, &quota, id).map_err(bad)?;
    }
    let ext = builder.extras.kernel_route.as_ref().unwrap();
    let service = ext.native_carriage_service.as_ref().unwrap();
    let mass = ext.native_carriage_mass.as_ref().unwrap();
    let queue_policy = &ext.native_final.as_ref().unwrap().policy.queue_policy;
    let mut physical = match r.aux_row::<(Digest, NativeCarriageGlobalV22)>(QUOTA, &b(&(22u8, 0u8))) {
        None => NativeCarriageGlobalV22::empty(service),
        Some((root, g)) if root == g.root() => g,
        _ => return Err(bad("native39 physical global original root differs")),
    };
    let mut queue = match r.aux.get(&(QUOTA, b(&(30u8, 0u8)))) {
        None => NativeQueueGlobalV30::empty(queue_policy, service).map_err(bad)?,
        Some(raw) => {
            if raw.len() > NATIVE_QUEUE_ROW_LIMIT_V30 {
                return Err(bad("native39 queue global extent"));
            }
            let (root, g): (Digest, NativeQueueGlobalV30) =
                borsh::from_slice(raw).map_err(|_| bad("native39 queue global grammar"))?;
            if root != g.root() {
                return Err(bad("native39 queue global original root differs"));
            }
            g
        }
    };
    let (ql, pl) = queue
        .admit(&mut physical, queue_policy, service, mass, contract.queue(), id, original.root(), c.root(), ctx.daa_score, deadline)
        .map_err(bad)?;
    let token =
        CheckedNativeFundingV39::from_current_native(original.clone(), &class, &maximum, &quota, p, &witness, &e).map_err(bad)?;
    let budget = builder.plan_native_job_budget_v39(ctx, c.producer, &class, &maximum, &quota, &token)?;
    let requester_payout = requester.payout_payload;
    let producer_payout = producer.payout_payload;
    builder.hold_native_recipient_v1(Hash64::from_bytes(id), c.producer, 11, ctx.daa_score, deadline, reward, &[])?;
    if let Some(plan) = budget {
        builder.commit_bond_budget_v1(ctx, super::palw_bond_budget_fold_v1::native_job_budget_id_v39(&id), plan)?;
    }
    if builder.slash_bond(*signer, reward as u128)? != reward {
        return Err(bad("native39 original escrow changed after all preflights"));
    }
    material::attach(builder, maximum.input_manifest(), id, deadline)?;
    let book = NativeJobBookV39::fund(token);
    let ch = super::palw_native_source_fold_v39::class_header(builder.state.kernel_route.as_ref().unwrap(), class.root())?;
    super::palw_flat_execution_fold_v11::write_shard(builder, &baseline, id)?;
    write_source(builder, &source, id)?;
    for (key, raw) in [
        (b(&(30u8, 0u8)), b(&(queue.root(), queue))),
        (b(&(22u8, 0u8)), b(&(physical.root(), physical))),
        (b(&(30u8, id, 1u8)), b(&(ql.root(), ql))),
        (b(&(22u8, id)), b(&(pl.root(), pl))),
        (b(&(39u8, id, 0u8)), b(contract.record())),
        (b(&(39u8, id, 1u8)), b(quota.source())),
        (b(&(39u8, id, 2u8)), b(quota.paged())),
        (b(&(39u8, id, 3u8)), b(&p.execution.quota)),
        (b(&(39u8, id, 4u8)), b(&(contract.carriage().record().root(), contract.carriage().record()))),
    ] {
        builder.write_kernel_row(QUOTA, key, Some(raw))?;
    }
    let mut jh = JobHeader {
        version: 39,
        id,
        original,
        book: book.root(),
        book_bytes: 0,
        consent_bytes: h.body_bytes,
        class_bytes: ch.class_bytes,
        source_bytes: ch.source_bytes,
        post_bytes: 0,
        plan: quota.plan().record(),
        plan_root: quota.plan().record().root(),
        terminal: NativeJobTerminalV39::Funded,
        requester_payout,
        producer_payout,
    };
    write_book(builder, &mut jh, &book)?;
    protected::initialize(builder, &jh)?;
    requesters.push(id);
    producers.push(id);
    consents.retain(|x| *x != c.root());
    builder.write_kernel_row(TABLE, k(21, signer), Some(b(&requesters)))?;
    builder.write_kernel_row(TABLE, k(22, &c.producer), Some(b(&producers)))?;
    builder.write_kernel_row(TABLE, k(19, &c.producer), Some(b(&consents)))?;
    builder.write_kernel_row(TABLE, k(20, &c.root()), Some(b(&(id, book.funding(), c.root()))))?;
    builder.write_kernel_row(TABLE, k(25, &id), Some(b(&(next.root(), receipt, ctx.block.as_bytes(), ctx.daa_score))))?;
    // Preserve the ORIGINAL producer wire and contract for all later replay,
    // audit and GC. The consumed tombstone prevents a second Fund forever.
    builder.write_kernel_row(TABLE, k(27, &id), Some(b(&c)))?;
    builder.write_kernel_row(TABLE, k(18, &c.root()), None)?;
    builder.write_kernel_row(TABLE, k(26, &c.root()), None)?;
    builder.write_kernel_row(NONCE, b(&c.wire.requester_operator), Some(b(&next)))?;
    super::palw_native_source_fold_v39::retain_class(
        builder,
        jh.original.class,
        super::palw_native_source_fold_v39::HOLD_JOB,
        id,
        jh.original.deadline,
    )?;
    super::palw_native_source_fold_v39::release_class(
        builder,
        jh.original.class,
        super::palw_native_source_fold_v39::HOLD_CONSENT,
        c.root(),
    )?;
    if let Some(invocation) = &invocation {
        super::palw_native_master_fold_v39::record_funded(builder, ctx, invocation, id)?;
    }
    builder.note_kernel_bond(&c.producer)?;
    Ok(())
}
fn original_binding(h: &JobHeader, plan: &CheckedJobQuotaPlanV11) -> SubjectBindingV11 {
    SubjectBindingV11 {
        version: 11,
        policy_root: [0; 64],
        class_id: h.original.class,
        class_scope9_root: h.original.class_scope,
        program_root: plan.record().program_root,
        parameter_root: plan.record().parameter_root,
        job_id: h.id,
        job_input_root: h.original.maximum,
        resource10_root: h.original.maximum_resources,
        quota_plan_root: h.plan_root,
        actual_positions: h.original.maximum_positions,
        requester: palw_kernel_bond_id_v1(&h.original.requester),
        producer: palw_kernel_bond_id_v1(&h.original.producer),
        admitted_interval: h.original.interval,
        deadline_daa: h.original.deadline,
    }
}
fn predebit_original(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    p: &NativeExecutionPoliciesV39,
    h: &JobHeader,
    kind: QuotaActionKindV11,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    let plan = CheckedJobQuotaPlanV11::restore(&h.plan, &h.plan_root).map_err(bad)?;
    let mut binding = original_binding(h, &plan);
    binding.policy_root = p.execution.quota.root();
    let e = epoch(p, ctx)?;
    let mut s = super::palw_flat_execution_fold_v11::quota_shard(
        builder,
        &p.execution,
        &e,
        palw_kernel_bond_id_v1(&h.original.producer),
        h.id,
    )?;
    let action = plan.action(kind, body).map_err(bad)?;
    s.debit_reserved(&p.execution.quota, &e, &binding, &plan, &action).map_err(bad)?;
    super::palw_flat_execution_fold_v11::write_shard(builder, &s, h.id)?;
    Ok(())
}
fn restore_job(
    builder: &TransitionBuilder<'_>,
    p: &NativeExecutionPoliciesV39,
    h: &JobHeader,
) -> Result<
    (NativeJobBookV39, NativeConsentV39, CheckedNativeClassV39, CheckedJobMaximumV39, CheckedQuotaV39, CheckedNativeMaximumV39),
    PalwStateV2Error,
> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let c: NativeConsentV39 = read(r, 27, &h.id, NativeConsentWireV39::MAX_BYTES + 16384)?;
    let (class, max, quota, contract) = quote(builder, p, &c.wire)?;
    let book = book(r, h)?;
    book.check(&class, &max, &quota, p).map_err(bad)?;
    if c.root() != h.original.consent
        || c.producer != h.original.producer
        || c.wire.requester != h.original.requester
        || c.checked_maximum != *contract.record()
        || contract.record().root() != h.original.maximum_contract
        || quota.root() != h.original.quota
        || h.class_bytes != super::palw_native_source_fold_v39::class_header(r, h.original.class)?.class_bytes
        || h.source_bytes != super::palw_native_source_fold_v39::class_header(r, h.original.class)?.source_bytes
    {
        return Err(bad("native39 current original source/class/consent/max contract differs"));
    }
    let c0 = r.aux.get(&(TABLE, k(20, &c.root()))).ok_or_else(|| bad("native39 once consumed consent tombstone absent"))?;
    if c0 != &b(&(h.id, book.funding(), c.root())) {
        return Err(bad("native39 consumed original consent relation differs"));
    }
    let counter = r.execution_nonce_counter_v11(&h.original.requester_operator).map_err(bad)?;
    let used = misaka_palw_kernel::flat_execution11::canonical_nonce_v11(&h.original.nonce)
        .map_err(bad)?
        .checked_add(1)
        .ok_or_else(|| bad("native39 spent nonce overflow"))?;
    if counter.next_nonce < used {
        return Err(bad("native39 original requester spent nonce absent"));
    }
    Ok((book, c, class, max, quota, contract))
}
fn commit(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    if body.len() < 66 || body.len() > NativeCommitWireV39::MAX_BYTES {
        return Err(bad("native39 compactCommit body extent"));
    }
    let id = body[2..66].try_into().unwrap();
    let h = header(builder.state.kernel_route.as_ref().unwrap(), id)?;
    if *signer != h.original.producer || h.terminal != NativeJobTerminalV39::Funded || ctx.daa_score >= h.original.commit_until {
        return Err(bad("native39 compactCommit requires original Funded producer phase"));
    }
    predebit_original(builder, ctx, p, &h, QuotaActionKindV11::Metadata, body)?;
    commit_after_paid(builder, ctx, signer, p, body, h)
}
fn commit_after_paid(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    body: &[u8],
    mut h: JobHeader,
) -> Result<(), PalwStateV2Error> {
    let id = h.id;
    // Malformed attempts retain the preceding original paid debit. The object
    // is dropped successfully, so a nested decoder cannot roll back the tariff.
    let Ok(w) = NativeCommitWireV39::from_bytes(body, h.original.maximum_positions) else { return Ok(()) };
    let (mut book, _, class, max, quota, _) = restore_job(builder, p, &h)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let reader = NativeMaterialReader { route: r };
    let inputs = match ActualInputResolverV39::derive(class.source(), &max, &w.generated, &reader) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    if book.commit(&class, &max, &quota, p, signer, ctx.daa_score, &w.claim, &w.generated, &inputs, h.original.reserve).is_err() {
        return Ok(());
    }
    let raw = b(&w);
    h.post_bytes = raw.len() as u32;
    builder.write_kernel_row(TABLE, k(28, &id), Some(raw))?;
    write_book(builder, &mut h, &book)?;
    protected::set_claim(builder, &h, w.claim.root())?;
    Ok(())
}
#[path = "palw_native_material_fold_v39.rs"]
mod material;
use material::{NativeMaterialReader, authenticate_material};
pub(super) fn apply(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    action: u8,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    if matches!(action, 41 | 42 | 57..=60 | 71 | 72) {
        return super::palw_native_source_fold_v39::apply(builder, ctx, signer, action, body);
    }
    let p = super::palw_native_source_fold_v39::policy(builder, ctx)?;
    match action {
        43 => accept(builder, ctx, signer, &p, body),
        44 => fund(builder, ctx, signer, &p, body),
        45 => commit(builder, ctx, signer, &p, body),
        47..=49 => public::apply(builder, ctx, signer, &p, action, body),
        46 | 50 | 52 | 56 => protected::apply(builder, ctx, signer, &p, action, body),
        51 => finalize(builder, ctx, signer, &p, body),
        NATIVE_COLLECT_ACTION_V39 => collect::collect(builder, ctx, signer, &p, body),
        NATIVE_COMMIT_PART_ACTION_V39 => collect::commit_part(builder, ctx, signer, &p, body),
        NATIVE_COMMIT_ASSEMBLE_ACTION_V39 => collect::commit_assemble(builder, ctx, signer, &p, body),
        65..=70 => super::palw_native_master_fold_v39::apply(builder, ctx, signer, action, body),
        61..=64 => material::apply(builder, ctx, signer, &p, action, body),
        73..=79 => super::palw_native_mining_fold_v39::apply(builder, ctx, signer, &p, action, body),
        _ => Err(bad("native39 action has no implemented current native transition")),
    }
}
#[path = "palw_native_job_audit_fold_v39.rs"]
mod audit;
#[path = "palw_native_collect_fold_v39.rs"]
pub(super) mod collect;
pub(super) use audit::audit_policies_v39;

#[path = "palw_native_protected_fold_v39.rs"]
mod protected;
fn credit(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    p: &NativeExecutionPoliciesV39,
    h: &mut JobHeader,
    book: &NativeJobBookV39,
    s: &NativeSettlementV39,
    finder: Option<PalwBondKeyV2>,
) -> Result<(), PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    if r.aux.contains_key(&(TABLE, k(23, &h.id)))
        || book.settlement() != Some(s)
        || s.original_funding != h.original.root()
        || s.job != h.id
        || s.original_escrow != h.original.reward
        || s.producer_credit.checked_add(s.requester_credit) != Some(h.original.reward)
    {
        return Err(bad("native39 original escrow/once-credit settlement differs"));
    }
    let recipient = if s.producer_credit != 0 { h.original.producer } else { h.original.requester };
    let payout = if s.producer_credit != 0 { h.producer_payout } else { h.requester_payout };
    let amount = s.producer_credit.checked_add(s.requester_credit).ok_or_else(|| bad("native39 actual queue overflow"))?;
    let queue = Hash64::from_bytes(object_id(
        b"misaka-palw/native/original-single-credit-queue/v39",
        &(h.id, h.original.root(), s.root(), recipient, payout),
    ));
    if builder.state.pending_payouts.contains_key(&queue) {
        return Err(bad("native39 original job-specific actual queue already exists"));
    }
    // G14 lane closure G-14: a conviction of a drained bond may collect nothing for its finder; nothing is then paid.
    let finder_payout = match finder.filter(|_| s.finder_credit != 0) {
        Some(f) => {
            let bond = builder.state.bonds.get(&f).ok_or_else(|| bad("native39 actual finder absent"))?;
            if bond.operator_id.as_bytes() == h.original.producer_operator || s.finder_credit == 0 {
                return Err(bad("native39 actual independent finder differs"));
            }
            Some((f, bond.payout_payload))
        }
        None => {
            if s.finder_credit != 0 {
                return Err(bad("native39 actual finder credit has no independent owner"));
            }
            None
        }
    };
    if s.actual_slash != 0 && builder.state.bonds.get(&h.original.producer).is_none_or(|b| b.collateral < s.actual_slash) {
        return Err(bad("native39 original producer reserve no longer collectible"));
    }
    if s.actual_slash != 0 && builder.slash_bond(h.original.producer, s.actual_slash as u128)? != s.actual_slash {
        return Err(bad("native39 original actual collected producer reserve changed"));
    }
    // RFC08 08-B (ADR-0184 §1, dormant `palw_native_liability_horizon_v1`): a Final of a claim committed past the fence does not
    // queue the producer's fee — it is held to the claim's liability horizon (signed deadline + window_court), accusable through
    // it, and released by the closing tick or refunded to the requester on a post-Final conviction.
    let held = s.producer_credit != 0
        && s.terminal == NativeJobTerminalV39::Final
        && book.claim().is_some_and(|c| {
            super::palw_native_liability_hold_fold_v1::native_fee_needs_hold_v1(
                builder.params,
                c.committed,
                ctx.daa_score,
                h.original.deadline,
            )
        });
    if held {
        let horizon = super::palw_native_liability_hold_fold_v1::native_horizon_v1(builder.params, h.original.deadline);
        super::palw_native_liability_hold_fold_v1::hold_native_fee_v1(
            builder,
            &crate::palw_native_liability_hold_v1::PalwNativeHeldFeeRowV1 {
                version: crate::palw_native_liability_hold_v1::PALW_NATIVE_LIABILITY_HOLD_ROW_VERSION_V1,
                family: crate::palw_native_liability_hold_v1::PALW_NATIVE_FAMILY_JOB_V39,
                job: h.id,
                claim: s.claim.unwrap_or([0; 64]),
                producer: h.original.producer,
                queue: queue.as_bytes(),
                payload: payout.as_bytes(),
                amount,
                income_root: h.id,
                income_kind: 11,
                budget: super::palw_bond_budget_fold_v1::native_job_budget_id_v39(&h.id).as_bytes(),
                refund_payload: h.requester_payout.as_bytes(),
                final_daa: ctx.daa_score,
                horizon_daa: horizon,
            },
        )?;
    } else {
        if s.producer_credit != 0 {
            builder.pay_native_recipient_v1(Hash64::from_bytes(h.id), h.original.producer, 11, ctx.daa_score, amount, queue, true)?;
        } else {
            builder.cancel_native_recipient_v1(Hash64::from_bytes(h.id), h.original.producer, 11)?;
        }
        builder.write_payout(queue, Some(PalwPayoutV2 { payload: payout, amount }))?;
    }
    if let Some((f, payout)) = finder_payout {
        if builder.params.reporter_income_active_at(ctx.daa_score) {
            let award = Hash64::from_bytes(object_id(
                b"misaka-palw/native/original-actual-collected-finder/v39",
                &(h.id, h.original.root(), s.proof, f),
            ));
            builder.stage_reporter_recipient_v1(award, f, ctx.daa_score, s.finder_credit, Hash64::from_bytes(h.id))?;
        } else {
            builder.add_kernel_payout(payout, s.finder_credit)?;
        }
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let raw =
        r.aux.get(&(QUOTA, b(&(2u8, h.id)))).filter(|raw| raw.len() < 8192).ok_or_else(|| bad("native39 original subject absent"))?;
    let (root, mut subject): (Digest, QuotaSubjectSummaryV11) = borsh::from_slice(raw).map_err(|_| bad("native39 subject grammar"))?;
    if root != subject.row_root() || subject.binding.job_id != h.id || subject.closed {
        return Err(bad("native39 original live subject terminal differs"));
    }
    let counter = r.execution_nonce_counter_v11(&h.original.requester_operator).map_err(bad)?;
    let e = epoch(p, ctx)?;
    let terminal = NativeTerminalAnchorV39 {
        job: h.id,
        funding: book.funding(),
        terminal_book: book.root(),
        established: s.established,
        producer_credit: s.producer_credit,
        requester_credit: s.requester_credit,
        actual_slash: s.actual_slash,
        finder_credit: s.finder_credit,
        escrow_closed: book.escrow() == 0,
        proof: s.proof,
    };
    // This private checked token authenticates actual monetary conservation and
    // the original spent nonce; it grants no caller-supplied Final authority.
    let (_, _, _, _, q, _) = restore_job_for_terminal(builder, p, h, book)?;
    check_terminal_v39(
        &q,
        &h.original.funding_anchor(),
        &p.execution.quota,
        &nonce(palw_kernel_bond_id_v1(&h.original.requester), &counter),
        &terminal,
        &e,
    )
    .map_err(bad)?;
    public::cancel_remaining(builder, h)?;
    // A held Final still admits independently paid accusations. Closing the
    // subject here would make protected::owner reject every new owner despite
    // accusable_v39 accepting the same claim. Keep its prepaid capacity intact
    // until the held fee actually leaves; never return consumed allowance.
    subject.closed = !held;
    builder.write_kernel_row(QUOTA, b(&(2u8, h.id)), Some(b(&(subject.row_root(), subject))))?;
    write_book(builder, h, book)?;
    let c = NativeSingleCreditV39 {
        version: 39,
        job: h.id,
        funding: book.funding(),
        terminal: book.root(),
        settlement: s.root(),
        claim: s.claim,
        recipient,
        payout,
        queue,
        amount,
        established: s.established,
    };
    builder.write_kernel_row(TABLE, k(23, &h.id), Some(b(&(c.root(), &c))))?;
    // A held fee keeps the producer's reservation open to the horizon (the release or the refund closes it).
    if !held {
        builder.bond_budget_close_v1(&super::palw_bond_budget_fold_v1::native_job_budget_id_v39(&h.id), ctx.daa_score)?;
    }
    // Keep the original books/posts/credit for Master and independent audit,
    // A held Final retains the producer slot as its collateral index; other
    // terminal slots can leave without returning any prepaid allowance.
    for (kind, who) in [(21, h.original.requester), (22, h.original.producer)] {
        // The producer catalogue is also the bounded collateral-reservation index.
        // Keep this slot while the Final fee is held, including an awaited verdict.
        if held && kind == 22 {
            continue;
        }
        let mut ids = list(builder.state.kernel_route.as_ref().unwrap(), kind, &who)?;
        ids.retain(|id| *id != h.id);
        builder.write_kernel_row(TABLE, k(kind, &who), Some(b(&ids)))?;
    }
    Ok(())
}
/// Release the bounded producer reservation slot only when its held fee has been
/// paid or refunded. The original job/book/credit remain available for audit.
pub(super) fn clear_held_producer_slot_v39(
    builder: &mut TransitionBuilder<'_>,
    row: &crate::palw_native_liability_hold_v1::PalwNativeHeldFeeRowV1,
) -> Result<(), PalwStateV2Error> {
    if row.family != crate::palw_native_liability_hold_v1::PALW_NATIVE_FAMILY_JOB_V39 {
        return Ok(());
    }
    let route = builder.state.kernel_route.as_ref().ok_or_else(|| bad("native39 held producer catalogue absent"))?;
    let mut ids = list(route, 22, &row.producer)?;
    // The native release guard authenticates the original subject. Close its
    // retained capacity with the actual fee transition, preserving all debits.
    // Authenticate any present row before changing it or the producer index.
    let subject_key = b(&(2u8, row.job));
    let subject = route.aux.get(&(QUOTA, subject_key.clone())).map(|raw| {
        if raw.len() >= 8192 {
            return Err(bad("native39 held subject extent"));
        }
        let (root, mut subject): (Digest, QuotaSubjectSummaryV11) =
            borsh::from_slice(raw).map_err(|_| bad("native39 held subject grammar"))?;
        if root != subject.row_root() || subject.binding.job_id != row.job {
            return Err(bad("native39 held subject root/job differs"));
        }
        subject.closed = true;
        Ok(subject)
    }).transpose()?;
    if let Some(subject) = subject {
        builder.write_kernel_row(QUOTA, subject_key, Some(b(&(subject.row_root(), subject))))?;
    }
    if ids.contains(&row.job) {
        ids.retain(|id| *id != row.job);
        builder.write_kernel_row(TABLE, k(22, &row.producer), Some(b(&ids)))?;
    }
    Ok(())
}

/// **RFC08 08-B (ADR-0184 §1): the credit of a conviction of a Final claim before its liability horizon.** The Final's fee was
/// held (aux 144), never queued: it is refunded to the requester whole, the producer's reserve is collected as a pre-Final
/// conviction collects it (`s.actual_slash`), the finder is paid as [`credit`] pays it, and the job's single-credit record is
/// replaced by the conviction's. Refused without a held fee inside its horizon.
pub(super) fn credit_after_final_conviction(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    h: &mut JobHeader,
    book: &NativeJobBookV39,
    s: &NativeSettlementV39,
    finder: PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_native_liability_hold_v1::PALW_NATIVE_FAMILY_JOB_V39;
    let r = builder.state.kernel_route.as_ref().unwrap();
    if !r.aux.contains_key(&(TABLE, k(23, &h.id)))
        || book.settlement() != Some(s)
        || s.terminal != NativeJobTerminalV39::Convicted
        || s.job != h.id
        || s.producer_credit != 0
        || s.requester_credit != h.original.reward
    {
        return Err(bad("native39 post-Final conviction settlement differs"));
    }
    let row =
        super::palw_native_liability_hold_fold_v1::refund_native_fee_v1(builder, PALW_NATIVE_FAMILY_JOB_V39, &h.id, ctx.daa_score)?;
    if row.amount != h.original.reward {
        return Err(bad("native39 held fee differs from the original escrow"));
    }
    if s.actual_slash != 0 && builder.slash_bond(h.original.producer, s.actual_slash as u128)? != s.actual_slash {
        return Err(bad("native39 post-Final collected producer reserve changed"));
    }
    if s.finder_credit != 0 {
        let bond = builder.state.bonds.get(&finder).ok_or_else(|| bad("native39 post-Final finder absent"))?;
        if bond.operator_id.as_bytes() == h.original.producer_operator {
            return Err(bad("native39 post-Final finder is not independent"));
        }
        let payout = bond.payout_payload;
        if builder.params.reporter_income_active_at(ctx.daa_score) {
            let award = Hash64::from_bytes(object_id(
                b"misaka-palw/native/post-final-collected-finder/v39",
                &(h.id, h.original.root(), s.proof, finder),
            ));
            builder.stage_reporter_recipient_v1(award, finder, ctx.daa_score, s.finder_credit, Hash64::from_bytes(h.id))?;
        } else {
            builder.add_kernel_payout(payout, s.finder_credit)?;
        }
    }
    write_book(builder, h, book)?;
    let c = NativeSingleCreditV39 {
        version: 39,
        job: h.id,
        funding: book.funding(),
        terminal: book.root(),
        settlement: s.root(),
        claim: s.claim,
        recipient: h.original.requester,
        payout: h.requester_payout,
        queue: super::palw_native_liability_hold_fold_v1::native_refund_queue_v1(&row),
        amount: row.amount,
        established: s.established,
    };
    builder.write_kernel_row(TABLE, k(23, &h.id), Some(b(&(c.root(), &c))))?;
    Ok(())
}

/// **Does a held fee's job still have an open dispute?** (the release waits for its verdict): a native39 job's pending demands or
/// live proof owners. Unknown families or unavailable job/status rows cannot authorize a payment.
pub(super) fn native_dispute_open_v39(
    builder: &TransitionBuilder<'_>,
    row: &crate::palw_native_liability_hold_v1::PalwNativeHeldFeeRowV1,
) -> bool {
    if row.family != crate::palw_native_liability_hold_v1::PALW_NATIVE_FAMILY_JOB_V39 {
        return true;
    }
    let Some(r) = builder.state.kernel_route.as_ref() else { return true };
    let Ok(h) = header(r, row.job) else { return true };
    let Some(raw) = r.aux.get(&(QUOTA, b(&(2u8, row.job)))).filter(|raw| raw.len() < 8192) else { return true };
    let Ok((root, subject)) = borsh::from_slice::<(Digest, QuotaSubjectSummaryV11)>(raw) else { return true };
    if root != subject.row_root() || subject.binding.job_id != row.job {
        return true;
    }
    protected::status(r, &h).map_or(true, |st| !st.pending.is_empty() || st.owners.iter().any(|id| *id != [0; 64]))
}

/// **Is the job still accusable at `now`?** Challengeable, or (RFC08 08-B) Final with its fee held inside the liability horizon.
pub(super) fn accusable_v39(builder: &TransitionBuilder<'_>, h: &JobHeader, now: u64) -> bool {
    h.terminal == NativeJobTerminalV39::Challengeable
        || (h.terminal == NativeJobTerminalV39::Final
            && super::palw_native_liability_hold_fold_v1::native_accusable_after_final_v1(
                &builder.state,
                crate::palw_native_liability_hold_v1::PALW_NATIVE_FAMILY_JOB_V39,
                &h.id,
                now,
            )
            .is_some())
}

/// The last DAA an accusation's window may reach: the signed deadline, or past a held Final its liability horizon.
pub(super) fn accusation_horizon_v39(builder: &TransitionBuilder<'_>, h: &JobHeader) -> u64 {
    if h.terminal == NativeJobTerminalV39::Final
        && let Some(row) = super::palw_native_liability_hold_fold_v1::held_native_fee_v1(
            &builder.state,
            crate::palw_native_liability_hold_v1::PALW_NATIVE_FAMILY_JOB_V39,
            &h.id,
        )
    {
        return row.horizon_daa;
    }
    h.original.deadline
}

fn restore_job_for_terminal(
    builder: &TransitionBuilder<'_>,
    p: &NativeExecutionPoliciesV39,
    h: &JobHeader,
    terminal: &NativeJobBookV39,
) -> Result<
    (NativeJobBookV39, NativeConsentV39, CheckedNativeClassV39, CheckedJobMaximumV39, CheckedQuotaV39, CheckedNativeMaximumV39),
    PalwStateV2Error,
> {
    let (old, c, class, max, q, contract) = restore_job(builder, p, h)?;
    if old.funding() != terminal.funding() || old.id() != terminal.id() {
        return Err(bad("native39 original terminal replaced another book"));
    }
    terminal.check(&class, &max, &q, p).map_err(bad)?;
    Ok((old, c, class, max, q, contract))
}
fn finalize(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    raw: &[u8],
) -> Result<(), PalwStateV2Error> {
    if raw.len() != 67 || raw[..2] != 39u16.to_le_bytes() || raw[2] > 1 {
        return Err(bad("native39 Final/refund exact grammar"));
    }
    let mode = raw[2];
    let id = raw[3..67].try_into().unwrap();
    let mut h = header(builder.state.kernel_route.as_ref().unwrap(), id)?;
    let actor = builder.state.bonds.get(signer).ok_or_else(|| bad("native39 independent finalizer absent"))?;
    if mode == 0 && (*signer == h.original.producer || actor.operator_id.as_bytes() == h.original.producer_operator) {
        return Err(bad("native39 Final must be independently submitted"));
    }
    if !prepay(builder, ctx, signer, p, h.source_bytes as u64 + h.consent_bytes as u64 + h.post_bytes as u64, raw.len(), 16384)? {
        return Ok(());
    }
    let (mut book, _, _, _, _, max) = restore_job(builder, p, &h)?;
    let s = if mode == 1 {
        book.refund_unclaimed(ctx.daa_score).map_err(bad)?
    } else {
        let status = protected::status(builder.state.kernel_route.as_ref().unwrap(), &h)?;
        let claim = book.claim().ok_or_else(|| bad("native39 actual original claim absent"))?;
        if !status.pending.is_empty() || status.owners.iter().any(|id| *id != [0; 64]) || ctx.daa_score <= status.latest {
            return Err(bad("native39 Final still has original paid streams or unanswered units"));
        }
        let closure = NativeFinalClosureV39 {
            job: id,
            funding: book.funding(),
            claim: claim.claim,
            maximum: h.original.maximum_contract,
            source: h.original.source_plan,
            physical: h.original.physical,
            queue: h.original.queue,
            now: ctx.daa_score,
            pending: 0,
            owners: 0,
            source_obligations: 0,
            trace_obligations: 0,
            original_closure: object_id(
                b"misaka-palw/native/original-complete-public-closure/v39",
                &(status.root(), h.book, h.original.source_plan, h.original.queue),
            ),
        };
        book.finalize(max.conditions(), &closure).map_err(bad)?
    };
    credit(builder, ctx, p, &mut h, &book, &s, None)
}

#[path = "palw_native_public_fold_v39.rs"]
mod public;
fn original_carriage(
    r: &PalwKernelRouteStateV1,
    h: &JobHeader,
    mass: &crate::palw_native_carriage_v22::NativeCarriageMassParamsV22,
    service: &NativeCarriageServiceV22,
) -> Option<(crate::palw_native_paged_carriage_v29::NativePagedCarriageRecordV29, Digest)> {
    if !service.implemented || service.check(mass).is_err() || service.root() != h.original.physical {
        return None;
    }
    let bounded = |key: Vec<u8>, cap: usize| r.aux.get(&(QUOTA, key)).filter(|raw| raw.len() <= cap);
    let (root, c): (Digest, crate::palw_native_paged_carriage_v29::NativePagedCarriageRecordV29) =
        borsh::from_slice(bounded(b(&(39u8, h.id, 4u8)), 4096)?).ok()?;
    let (qroot, q): (Digest, NativeQueueLeaseV30) = borsh::from_slice(bounded(b(&(30u8, h.id, 1u8)), 1024)?).ok()?;
    let (proot, physical): (Digest, NativeCarriageLeaseV22) = borsh::from_slice(bounded(b(&(22u8, h.id)), 512)?).ok()?;
    if c.version != 39
        || root != c.root()
        || root != h.original.transport
        || c.class != h.original.class
        || c.original_job_plan != h.original.shared_plan
        || c.paid_paged_plan != h.original.paged_plan
        || c.service != service.root()
        || qroot != q.root()
        || q.retired
        || q.job != h.id
        || q.original_funding != h.original.root()
        || q.original_consent != h.original.consent
        || q.original_plan != h.original.queue
        || q.original_physical_lease != proot
        || physical.root() != proot
        || physical.retired
        || physical.original_job != h.id
        || physical.original_consent != h.original.consent
        || physical.plan != c.root()
        || physical.density != c.density
    {
        return None;
    }
    Some((c, proot))
}
impl PalwKernelRouteStateV1 {
    /// Constant bounded native metadata only, before signature/proof allocation.
    pub fn native_proof_owner_v39(
        &self,
        job: Digest,
        seal: Digest,
        index: u16,
        count: u16,
        total: u32,
        mode: u8,
        now: u64,
    ) -> Option<PalwBondKeyV2> {
        let h = header(self, job).ok()?;
        let o = protected::owner(self, seal).ok()?;
        let s = protected::status(self, &h).ok()?;
        if h.terminal != NativeJobTerminalV39::Challengeable
            || o.job != job
            || o.funding != h.original.root()
            || !s.owners.contains(&seal)
            || o.claim != s.claim
            || o.retired
            || o.spent
            || now < o.matures
            || now >= o.until
        {
            return None;
        }
        match mode {
            0 if !o.discovery
                && o.count == count
                && o.total == total
                && index < count
                && o.attempts[index as usize] < 2
                && !o.has(index) =>
            {
                Some(o.actor)
            }
            1 if !o.discovery
                && o.count == count
                && o.total == total
                && index == 0
                && o.assemblies < 2
                && (0..count).all(|i| o.has(i)) =>
            {
                Some(o.actor)
            }
            2 if o.discovery
                && o.conversion == 0
                && index == 0
                && total > 0
                && total <= o.total
                && count as u64 == (total as u64).div_ceil(32768) =>
            {
                Some(o.actor)
            }
            _ => None,
        }
    }
    pub fn native_paid_proof_eligibility_v39(
        &self,
        job: Digest,
        seal: Digest,
        index: u16,
        count: u16,
        total: u32,
        mode: u8,
        now: u64,
        mass: &crate::palw_native_carriage_v22::NativeCarriageMassParamsV22,
        service: &NativeCarriageServiceV22,
    ) -> Option<NativePaidCarrierEligibilityV22> {
        let actor = self.native_proof_owner_v39(job, seal, index, count, total, mode, now)?;
        let h = header(self, job).ok()?;
        let o = protected::owner(self, seal).ok()?;
        let (c, lease) = original_carriage(self, &h, mass, service)?;
        mass.packet_mass(&c.proof_envelope).ok().filter(|m| *m <= service.slot_mass)?;
        Some(NativePaidCarrierEligibilityV22 {
            original_job: job,
            unit: seal,
            owner: actor,
            deadline: o.until,
            kind: if mode == 0 {
                7
            } else if mode == 1 {
                8
            } else {
                12
            },
            lease_root: lease,
            original_service: service.root(),
            envelope: c.proof_envelope,
        })
    }
    pub fn native_answer_owner_v39(
        &self,
        id: Digest,
        group: Digest,
        index: u16,
        count: u16,
        total: u32,
        now: u64,
    ) -> Option<PalwBondKeyV2> {
        let p = public::pending(self, id).ok()?;
        let h = header(self, p.job).ok()?;
        if h.terminal != NativeJobTerminalV39::Challengeable
            || p.funding != h.original.root()
            || p.source_plan != h.original.source_plan
            || p.settled
            || now > p.until
            || group == [0; 64]
            || count == 0
            || count > 4
            || index >= count
            || total == 0
            || total > 128 << 10
            || count as u64 != (total as u64).div_ceil(32768)
            || p.group == [0; 64] && p.attempts >= 2
            || p.group != [0; 64] && (p.group != group || p.total != total || p.count != count)
            || p.accepted & (1 << index) != 0
        {
            return None;
        }
        Some(h.original.producer)
    }
    pub fn native_paid_answer_eligibility_v39(
        &self,
        id: Digest,
        group: Digest,
        index: u16,
        count: u16,
        total: u32,
        now: u64,
        mass: &crate::palw_native_carriage_v22::NativeCarriageMassParamsV22,
        service: &NativeCarriageServiceV22,
    ) -> Option<NativePaidCarrierEligibilityV22> {
        let actor = self.native_answer_owner_v39(id, group, index, count, total, now)?;
        let p = public::pending(self, id).ok()?;
        let h = header(self, p.job).ok()?;
        let (c, lease) = original_carriage(self, &h, mass, service)?;
        mass.packet_mass(&c.answer_envelope).ok().filter(|m| *m <= service.slot_mass)?;
        Some(NativePaidCarrierEligibilityV22 {
            original_job: h.id,
            unit: id,
            owner: actor,
            deadline: p.until,
            kind: 9,
            lease_root: lease,
            original_service: service.root(),
            envelope: c.answer_envelope,
        })
    }
}

pub(super) fn accepted_consent_wire(r: &PalwKernelRouteStateV1, producer: &PalwBondKeyV2, wire_root: Digest, now: u64) -> bool {
    list(r, 19, producer).is_ok_and(|ids| {
        ids.iter().any(|id| {
            consent_header(r, *id)
                .is_ok_and(|h| h.producer == *producer && h.accepted == now && h.wire == wire_root && h.expires >= now)
        })
    })
}

#[cfg(test)]
#[path = "palw_native_job_fold_v39_liability_tests.rs"]
mod liability_tests;
