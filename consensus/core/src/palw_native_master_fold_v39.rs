//! Complete Master39 authority over original registered source/classes and
//! genuinely funded component books. Pure manifests never mint native rights.
use super::*;
use crate::{
    palw_kernel_route_v1::{PalwKernelRouteStateV1, palw_kernel_bond_id_v1},
    palw_native_execution_v39::*,
    palw_native_job_v39::*,
    palw_native_master_v39::*,
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{
    flat_component_source39::*,
    flat_invocation39::*,
    flat_job39::CheckedJobMaximumV39,
    flat_material39::{MaterialDescriptorV39, MaterialOriginV39},
    flat_pipeline39::*,
    flat_quota11::*,
    flat_resources10::{ResourceAxisV10 as A, ResourceQuantitiesV10},
    hash::{Digest, object_id},
};
const TABLE: u8 = 112;
const MASTER_RESTORE_MAX: u64 = 128 << 20;
fn bad(s: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(s.into())
}
fn b<T: BorshSerialize>(v: &T) -> Vec<u8> {
    borsh::to_vec(v).expect("bounded Master39 value")
}
fn k<T: BorshSerialize>(kind: u8, id: &T) -> Vec<u8> {
    b(&(39u8, kind, id))
}
fn row<T: BorshDeserialize>(
    r: &PalwKernelRouteStateV1,
    kind: u8,
    id: &impl BorshSerialize,
    cap: usize,
) -> Result<T, PalwStateV2Error> {
    let raw =
        r.aux.get(&(TABLE, k(kind, id))).filter(|r| r.len() <= cap).ok_or_else(|| bad("Master39 original bounded row absent"))?;
    borsh::from_slice(raw).map_err(|_| bad("Master39 original row grammar"))
}
pub(super) fn record(r: &PalwKernelRouteStateV1, id: Digest) -> Result<NativeMasterRecordV39, PalwStateV2Error> {
    let (root, m): (Digest, NativeMasterRecordV39) = row(r, 80, &id, 64 << 10)?;
    if m.version != 39
        || m.id != id
        || m.root() != root
        || m.components.is_empty()
        || m.components.len() > 64
        || m.components.iter().enumerate().any(|(i, c)| c.stage as usize != i)
        || m.registration_bytes as usize > MASTER_REGISTER_MAX_BYTES_V39
        || m.restore_bytes > MASTER_RESTORE_MAX
    {
        return Err(bad("Master39 original rooted header differs"));
    }
    Ok(m)
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct Progress {
    version: u16,
    master: Digest,
    original: Digest,
    components: Vec<Option<NativeMasterFundedComponentV39>>,
}
impl Progress {
    fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-Master-component-progress/v39", self)
    }
}
fn progress(r: &PalwKernelRouteStateV1, m: &NativeMasterRecordV39) -> Result<Progress, PalwStateV2Error> {
    let (root, p): (Digest, Progress) = row(r, 82, &m.id, 64 << 10)?;
    if p.version != 39
        || p.master != m.id
        || p.original != m.root()
        || root != p.root()
        || p.components.len() != m.components.len()
        || p.components.iter().enumerate().any(|(i, c)| c.as_ref().is_some_and(|c| c.stage as usize != i || c.job == [0; 64]))
    {
        return Err(bad("Master39 original component progress differs"));
    }
    Ok(p)
}
fn catalog(r: &PalwKernelRouteStateV1, owner: &PalwBondKeyV2) -> Result<Vec<Digest>, PalwStateV2Error> {
    let Some(raw) = r.aux.get(&(TABLE, k(85, owner))) else { return Ok(vec![]) };
    if raw.len() < 4 || raw.len() > 4 + 8 * 64 || u32::from_le_bytes(raw[..4].try_into().unwrap()) > 8 {
        return Err(bad("Master39 original owner catalog bound"));
    }
    let ids: Vec<Digest> = borsh::from_slice(raw).map_err(|_| bad("Master39 owner catalog"))?;
    if ids.iter().enumerate().any(|(i, v)| *v == [0; 64] || ids[..i].contains(v)) {
        return Err(bad("Master39 owner catalog duplicate"));
    }
    Ok(ids)
}
fn prepay(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    bytes: u64,
    held: u64,
) -> Result<bool, PalwStateV2Error> {
    if bytes > MASTER_RESTORE_MAX || held > 1 << 30 {
        return Err(bad("Master39 original paid metadata profile"));
    }
    let work = bytes
        .checked_mul(16384)
        .and_then(|n| n.checked_add(1 << 20))
        .ok_or_else(|| bad("Master39 original metadata work overflow"))?;
    let Some(fee) = super::palw_flat_model_vector_fold_v3::charge(builder, ctx, signer, work, false)? else { return Ok(false) };
    builder.slash_bond(*signer, fee)?;
    let mut q = ResourceQuantitiesV10::default();
    for (axis, v) in [
        (A::MetadataWork, work as u128),
        (A::ScalarSteps, work as u128),
        (A::IndexSteps, bytes as u128 * 128),
        (A::ValidatedElements, bytes as u128),
        (A::CopiedReferenceBytes, bytes as u128 * 16),
        (A::OwnedPeakBytes, bytes as u128 * 16 + held as u128 + 65536),
        (A::CourtScratchBytes, bytes as u128 * 16 + 65536),
        (A::StoredEvidenceBytes, held as u128),
        (A::ContainerAllocatorAllowance, bytes as u128 * 8 + 65536),
        (A::Attempts, 1),
    ] {
        q.set(axis, v);
    }
    let epoch = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(bad)?;
    let actor = palw_kernel_bond_id_v1(signer);
    let mut shard = super::palw_flat_execution_fold_v11::quota_shard(builder, &p.execution, &epoch, actor, [0; 64])?;
    shard.debit_ordinary(&p.execution.quota, &epoch, &DerivedOrdinaryMetadataV11::from_native_envelope(q)).map_err(bad)?;
    super::palw_flat_execution_fold_v11::write_shard(builder, &shard, [0; 64]);
    Ok(true)
}
fn registration_extent(r: &PalwKernelRouteStateV1, raw: &[u8]) -> Result<u64, PalwStateV2Error> {
    if raw.len() < 22 || raw.len() > MASTER_REGISTER_MAX_BYTES_V39 || raw[..2] != 39u16.to_le_bytes() {
        return Err(bad("Master39 original register fixed prefix"));
    }
    let n = u32::from_le_bytes(raw[18..22].try_into().unwrap()) as usize;
    if n == 0 || n > 64 || raw.len() < 22 + n * 64 {
        return Err(bad("Master39 original class count"));
    }
    let mut extent = raw.len() as u64 + 65536;
    let mut source_bytes = 0u64;
    for c in raw[22..22 + n * 64].chunks_exact(64) {
        let ch = super::palw_native_source_fold_v39::class_header(r, c.try_into().unwrap())?;
        source_bytes =
            source_bytes.checked_add(ch.source_bytes as u64).ok_or_else(|| bad("Master39 original source extent overflow"))?;
        extent = extent.checked_add(ch.source_bytes as u64 + 4096).ok_or_else(|| bad("Master39 original restore extent overflow"))?;
    }
    if source_bytes > MAX_COMPLETE_SOURCE_BYTES_V39 as u64 || extent > MASTER_RESTORE_MAX {
        return Err(bad("Master39 complete original source/restore profile exceeded"));
    }
    Ok(extent)
}
struct Restored {
    wire: NativeMasterRegisterV39,
    classes: Vec<CheckedNativeClassV39>,
    pipeline: CheckedPipelineV39,
    request: MasterPublicRequestV39,
    manifest: CompleteComponentManifestV39,
    complete_source: Digest,
}
fn restore_wire(r: &PalwKernelRouteStateV1, p: &NativeExecutionPoliciesV39, raw: &[u8]) -> Result<Restored, PalwStateV2Error> {
    let wire = NativeMasterRegisterV39::from_bytes(raw).map_err(bad)?;
    let mut classes = Vec::with_capacity(wire.classes.len());
    for id in &wire.classes {
        classes.push(super::palw_native_source_fold_v39::restore_class(r, p, *id)?);
    }
    let sources = classes.iter().map(|c| c.source().clone()).collect::<Vec<_>>();
    let programs = sources.iter().map(|s| s.program().clone()).collect::<Vec<_>>();
    let pipeline =
        PipelineV39::from_bytes(&wire.pipeline).map_err(|e| bad(e.to_string()))?.check(&programs).map_err(|e| bad(e.to_string()))?;
    let manifest = CompleteComponentManifestV39::from_bytes(&wire.manifest).map_err(|e| bad(e.to_string()))?;
    let checked = check_component_manifest_v39(
        &manifest,
        &sources,
        &pipeline,
        &manifest.original_required_components,
        manifest.original_task_metadata,
        &manifest.inputs,
    )
    .map_err(|e| bad(format!("{e:?}")))?;
    let encoded = 2u64
        + 4
        + wire.manifest.len() as u64
        + 4
        + wire.pipeline.len() as u64
        + 4
        + classes.iter().map(|c| 4 + c.source().wire().to_bytes().len() as u64).sum::<u64>();
    if encoded > MAX_COMPLETE_SOURCE_BYTES_V39 as u64 {
        return Err(bad("Master39 original complete bundle extent"));
    }
    let complete_source = object_id(
        b"misaka-palw/kernel/complete-original-source-bundle/v39",
        &(39u16, checked.root(), pipeline.root(), sources.iter().map(|s| s.root()).collect::<Vec<_>>(), encoded),
    );
    let request = MasterPublicRequestV39::from_bytes(&wire.request).map_err(bad)?;
    request.check(&pipeline).map_err(bad)?;
    if request.complete_source != complete_source {
        return Err(bad("Master39 request changes original complete source"));
    }
    for (i, c) in classes.iter().enumerate() {
        if c.wire().max_positions != c.source().original_context_max()
            || pipeline.wire().stages[i].max_trip < c.source().original_context_max()
        {
            return Err(bad("Master39 component lacks complete original context rather than a short class"));
        }
    }
    Ok(Restored { wire, classes, pipeline, request, manifest, complete_source })
}
fn stored_raw(r: &PalwKernelRouteStateV1, m: &NativeMasterRecordV39) -> Result<Vec<u8>, PalwStateV2Error> {
    let bytes = r
        .aux
        .get(&(TABLE, k(81, &m.id)))
        .filter(|v| v.len() == 4 + m.registration_bytes as usize)
        .ok_or_else(|| bad("Master39 original register body extent differs"))?;
    if u32::from_le_bytes(bytes[..4].try_into().unwrap()) != m.registration_bytes {
        return Err(bad("Master39 original register body prefix differs"));
    }
    Ok(bytes[4..].to_vec())
}
fn check_restored(m: &NativeMasterRecordV39, s: &Restored) -> Result<(), PalwStateV2Error> {
    if s.wire.root(m.owner) != m.id
        || s.complete_source != m.complete_source
        || s.request.root() != m.request
        || s.pipeline.root() != m.pipeline
        || s.manifest.root() != m.manifest
        || s.wire.classes != m.components.iter().map(|c| c.class).collect::<Vec<_>>()
    {
        return Err(bad("Master39 original source/request/header was rebound"));
    }
    Ok(())
}
fn original_final(
    r: &PalwKernelRouteStateV1,
    c: &NativeMasterFundedComponentV39,
) -> Result<(NativeJobBookV39, NativeSingleCreditV39), PalwStateV2Error> {
    let h = super::palw_native_job_fold_v39::header(r, c.job)?;
    let book = super::palw_native_job_fold_v39::book(r, &h)?;
    let (root, credit): (Digest, NativeSingleCreditV39) = row(r, 23, &c.job, 1024)?;
    if h.terminal != NativeJobTerminalV39::Final
        || book.funding() != c.funding
        || h.original.maximum_contract != c.maximum_contract
        || h.original.maximum != c.maximum
        || h.original.input_manifest != c.input_binding
        || h.original.requester_operator != c.requester_operator
        || h.original.producer_operator != c.producer_operator
        || credit.root() != root
        || credit.job != c.job
        || credit.funding != c.funding
        || credit.terminal != book.root()
        || credit.claim != book.claim().map(|c| c.claim)
        || credit.recipient != h.original.producer
        || credit.amount != h.original.reward
        || credit.amount == 0
        || book.settlement().is_none_or(|s| {
            s.root() != credit.settlement || s.producer_credit != credit.amount || s.requester_credit != 0 || s.actual_slash != 0
        })
    {
        return Err(bad("Master39 component lacks genuine original Final/actual single credit"));
    }
    Ok((book, credit))
}
fn register(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    raw: &[u8],
) -> Result<(), PalwStateV2Error> {
    let extent = registration_extent(builder.state.kernel_route.as_ref().unwrap(), raw)?;
    let mut ids = catalog(builder.state.kernel_route.as_ref().unwrap(), signer)?;
    if ids.len() >= 8 {
        return Err(bad("Master39 original owner has eight retained Masters"));
    }
    if !prepay(builder, ctx, signer, p, extent, raw.len() as u64 * 8 + 131072)? {
        return Ok(());
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let s = restore_wire(r, p, raw)?;
    let owner = builder.state.bonds.get(signer).ok_or_else(|| bad("Master39 original owner bond absent"))?;
    let operator = owner.operator_id.as_bytes();
    let nonce = r.execution_nonce_counter_v11(&operator).map_err(bad)?;
    let sequence = r
        .aux
        .get(&(TABLE, k(86, signer)))
        .map(|v| borsh::from_slice::<u64>(v).map_err(|_| bad("Master39 original owner sequence")))
        .transpose()?
        .unwrap_or(0);
    if owner.status != PalwBondStatusV2::Active
        || ctx.daa_score.saturating_sub(owner.registered_daa) < p.execution.execution.horizons.seal_maturity
        || s.request.requester != palw_kernel_bond_id_v1(signer)
        || s.request.requester_operator != operator
        || misaka_palw_kernel::flat_execution11::canonical_nonce_v11(&s.request.first_nonce).map_err(bad)? != nonce.next_nonce
        || s.wire.sequence != sequence
        || s.wire.deadline <= ctx.daa_score
        || s.wire.deadline.checked_sub(ctx.daa_score).is_none_or(|n| n > p.maximum_horizon.saturating_mul(s.classes.len() as u64))
    {
        return Err(bad("Master39 original owner/operator/nonce/sequence/deadline differs"));
    }
    for (key, producer) in s.wire.producer_bonds.iter().zip(&s.request.producers) {
        let bond = builder.state.bonds.get(key).ok_or_else(|| bad("Master39 original component producer absent"))?;
        if bond.status != PalwBondStatusV2::Active
            || bond.operator_id.as_bytes() != producer.operator
            || palw_kernel_bond_id_v1(key) != producer.producer
            || producer.reward > p.execution.execution.reward_cap
        {
            return Err(bad("Master39 actual original component producer/operator/reward differs"));
        }
    }
    // Existing finalized token slots carry authority only from genuine native
    // Final books and the exact accepted generated ids, never from this DTO.
    let finalized_extent = s.request.finalized.iter().try_fold(0u64, |sum, f| {
        let h = super::palw_native_job_fold_v39::header(r, f.job)?;
        sum.checked_add(h.post_bytes as u64 + h.book_bytes as u64 + 32768)
            .ok_or_else(|| bad("Master39 original finalized metadata extent overflow"))
    })?;
    if finalized_extent > 0 && !prepay(builder, ctx, signer, p, finalized_extent, 0)? {
        return Err(bad("Master39 original finalized metadata tariff unavailable"));
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    for f in &s.request.finalized {
        let h = super::palw_native_job_fold_v39::header(r, f.job)?;
        let book = super::palw_native_job_fold_v39::book(r, &h)?;
        let (root, credit): (Digest, NativeSingleCreditV39) = row(r, 23, &f.job, 1024)?;
        let post_raw = r
            .aux
            .get(&(TABLE, k(28, &f.job)))
            .filter(|v| v.len() == h.post_bytes as usize)
            .ok_or_else(|| bad("Master39 finalized token original post absent"))?;
        let post = NativeCommitWireV39::from_bytes(post_raw, h.original.maximum_positions).map_err(bad)?;
        if h.terminal != NativeJobTerminalV39::Final
            || post.job != f.job
            || post.generated != f.generated
            || post.claim.root() != f.claim
            || book.claim().map(|c| c.claim) != Some(f.claim)
            || credit.root() != root
            || root != f.credit
            || credit.terminal != book.root()
            || credit.claim != Some(f.claim)
            || credit.funding != book.funding()
            || credit.amount != h.original.reward
            || book.settlement().is_none_or(|s| s.root() != credit.settlement || s.producer_credit != credit.amount)
        {
            return Err(bad("Master39 finalized tokens lack genuine original Final/once-credit/generation"));
        }
    }
    let id = s.wire.root(*signer);
    if r.aux.contains_key(&(TABLE, k(80, &id))) || r.aux.contains_key(&(TABLE, k(83, &id))) {
        return Err(bad("Master39 original identity replay"));
    }
    let components = s
        .classes
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let source = c.source();
            let d = &s.manifest.stages[i];
            NativeMasterComponentV39 {
                stage: i as u16,
                class: c.root(),
                source: source.root(),
                profile: source.profile_root(),
                program: source.program_root(),
                inventory: source.inventory_root(),
                parameter: source.parameters().root(),
                input: d.input_declarations,
                output: d.output_declaration,
                original_context: source.original_context_max(),
                maximum_positions: c.wire().max_positions,
                maximum_scope: c.scope().root(),
                maximum_resources: c.resources().root(),
            }
        })
        .collect::<Vec<_>>();
    let set = |domain: &[u8], values: Vec<Digest>| object_id(domain, &(39u16, &values));
    let m = NativeMasterRecordV39 {
        version: 39,
        id,
        owner: *signer,
        operator,
        sequence,
        admitted: ctx.daa_score,
        deadline: s.wire.deadline,
        policy: p.root(),
        task: s.manifest.original_task_metadata,
        complete_source: s.complete_source,
        manifest: s.manifest.root(),
        pipeline: s.pipeline.root(),
        request: s.request.root(),
        class_set: set(b"misaka-palw/native/Master-classes/v39", components.iter().map(|c| c.class).collect()),
        profile_set: set(b"misaka-palw/native/Master-profiles/v39", components.iter().map(|c| c.profile).collect()),
        program_set: set(b"misaka-palw/native/Master-programs/v39", components.iter().map(|c| c.program).collect()),
        inventory_set: set(b"misaka-palw/native/Master-inventories/v39", components.iter().map(|c| c.inventory).collect()),
        parameter_set: set(b"misaka-palw/native/Master-real-parameters/v39", components.iter().map(|c| c.parameter).collect()),
        maximum_scope_set: set(b"misaka-palw/native/Master-max-scopes/v39", components.iter().map(|c| c.maximum_scope).collect()),
        maximum_resource_set: set(
            b"misaka-palw/native/Master-max-resources/v39",
            components.iter().map(|c| c.maximum_resources).collect(),
        ),
        components,
        registration_bytes: raw.len() as u32,
        restore_bytes: extent,
    };
    let progress = Progress { version: 39, master: id, original: m.root(), components: vec![None; m.components.len()] };
    ids.push(id);
    for edge in &s.manifest.inputs {
        if let ComponentInputSourceV39::Upstream { stage, output_declaration } = &edge.source {
            let target = m.components.get(edge.stage as usize).ok_or_else(|| bad("Master39 original input target absent"))?;
            builder.write_kernel_row(
                TABLE,
                k(92, &(id, edge.stage, edge.input)),
                Some(b(&(m.root(), *stage, *output_declaration, target.source))),
            );
        }
    }
    builder.write_kernel_row(TABLE, k(80, &id), Some(b(&(m.root(), &m))));
    builder.write_kernel_row(TABLE, k(81, &id), Some(b(&raw.to_vec())));
    builder.write_kernel_row(TABLE, k(82, &id), Some(b(&(progress.root(), progress))));
    builder.write_kernel_row(TABLE, k(85, signer), Some(b(&ids)));
    builder.write_kernel_row(
        TABLE,
        k(86, signer),
        Some(b(&sequence.checked_add(1).ok_or_else(|| bad("Master39 original sequence overflow"))?)),
    );
    Ok(())
}

/// Expensive Master restore is paid by the original current action before its
/// source/program/request clones. None is only the explicit standalone paid
/// work case (master zero), never a missing or unqualified Master fallback.
pub(super) fn authenticate_invocation(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    class: &CheckedNativeClassV39,
    maximum: &CheckedJobMaximumV39,
    reward: u64,
) -> Result<Option<CheckedComponentInvocationV39>, PalwStateV2Error> {
    let id = maximum.input_manifest().master;
    if id == [0; 64] {
        return Ok(None);
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let m = record(r, id)?;
    let pr = progress(r, &m)?;
    let stage =
        pr.components.iter().position(Option::is_none).ok_or_else(|| bad("Master39 already funded every original component"))?;
    if ctx.daa_score > m.deadline
        || m.policy != p.root()
        || m.components[stage].class != class.root()
        || m.components[stage].source != class.source().root()
        || maximum.request().requester != palw_kernel_bond_id_v1(&m.owner)
    {
        return Err(bad("Master39 original next component/source/operator horizon differs"));
    }
    let extra = pr.components[..stage].iter().flatten().try_fold(65536u64, |n, c| {
        let h = super::palw_native_job_fold_v39::header(r, c.job)?;
        n.checked_add(h.book_bytes as u64 + 16384).ok_or_else(|| bad("Master39 original prior header extent overflow"))
    })?;
    if !prepay(
        builder,
        ctx,
        signer,
        p,
        m.restore_bytes.checked_add(extra).ok_or_else(|| bad("Master39 complete restore overflow"))?,
        0,
    )? {
        return Err(bad("Master39 original metadata tariff unavailable"));
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let raw = stored_raw(r, &m)?;
    let s = restore_wire(r, p, &raw)?;
    check_restored(&m, &s)?;
    if s.request.producers[stage].reward != reward || s.request.requester_operator != m.operator {
        return Err(bad("Master39 original component reward/requester operator differs"));
    }
    let decode = s.pipeline.wire().stages.iter().position(|stage| matches!(stage.trip, misaka_palw_tir::pipeline::TripRule::Decode));
    let mut originals = Vec::with_capacity(stage);
    for c in pr.components[..stage].iter().flatten() {
        let (book, credit) = original_final(r, c)?;
        let claim = book.claim().ok_or_else(|| bad("Master39 original prior claim absent"))?;
        originals.push((c.clone(), book.root(), credit.root(), claim.claim, claim.positions));
    }
    // At most the actual decoder's generated list is retained; other upstream
    // tensor values remain in their separately paid/certified material pages.
    let mut generated = vec![];
    if let Some(d) = decode.filter(|d| *d < stage) {
        let c = pr.components[d].as_ref().ok_or_else(|| bad("Master39 original decoder not funded"))?;
        let h = super::palw_native_job_fold_v39::header(r, c.job)?;
        if !prepay(builder, ctx, signer, p, h.post_bytes as u64, 0)? {
            return Err(bad("Master39 original generation restore tariff unavailable"));
        }
        let r = builder.state.kernel_route.as_ref().unwrap();
        let post = r
            .aux
            .get(&(TABLE, k(28, &c.job)))
            .filter(|v| v.len() == h.post_bytes as usize)
            .ok_or_else(|| bad("Master39 original decoder accepted post absent"))?;
        let post = NativeCommitWireV39::from_bytes(post, h.original.maximum_positions).map_err(bad)?;
        if post.job != c.job
            || post.claim.root() != originals[d].3
            || object_id(b"misaka-palw/native/original-stopped-generation/v39", &post.generated)
                != super::palw_native_job_fold_v39::book(r, &h)?.claim().unwrap().generated
        {
            return Err(bad("Master39 original decoder generation/claim differs"));
        }
        generated = post.generated;
    }
    let prior = originals
        .iter()
        .enumerate()
        .map(|(i, (c, book, credit, claim, positions))| PriorComponentV39 {
            stage: c.stage,
            job: c.job,
            class: m.components[i].class,
            source: m.components[i].source,
            claim: *claim,
            final_book: *book,
            single_credit: *credit,
            positions: *positions,
            generated: if decode == Some(i) { &generated } else { &[] },
        })
        .collect::<Vec<_>>();
    check_component_invocation_v39(id, &s.request, &s.pipeline, class.source(), stage as u16, maximum, &prior).map(Some).map_err(bad)
}
pub(super) fn record_funded(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    checked: &CheckedComponentInvocationV39,
    job: Digest,
) -> Result<(), PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let m = record(r, checked.master())?;
    let mut progress = progress(r, &m)?;
    let stage = checked.stage() as usize;
    if progress.components.iter().position(Option::is_none) != Some(stage)
        || stage >= m.components.len()
        || r.aux.contains_key(&(TABLE, k(84, &job)))
    {
        return Err(bad("Master39 component job already consumed or original order changed"));
    }
    let h = super::palw_native_job_fold_v39::header(r, job)?;
    let original = &h.original;
    let c = &m.components[stage];
    if original.class != c.class
        || original.source != c.source
        || original.maximum != checked.maximum()
        || original.input_manifest != checked.input_manifest()
        || original.requester != m.owner
        || original.requester_operator != m.operator
        || original.admitted != ctx.daa_score
        || original.admitted_block != ctx.block.as_bytes()
        || original.deadline > m.deadline
        || h.terminal != NativeJobTerminalV39::Funded
    {
        return Err(bad("Master39 funded binding is not exact actual original current job"));
    }
    let component = NativeMasterFundedComponentV39 {
        stage: stage as u16,
        job,
        funding: original.root(),
        maximum_contract: original.maximum_contract,
        maximum: original.maximum,
        invocation: checked.root(),
        input_binding: original.input_manifest,
        requester_operator: original.requester_operator,
        producer_operator: original.producer_operator,
        admitted: ctx.daa_score,
    };
    progress.components[stage] = Some(component.clone());
    builder.write_kernel_row(TABLE, k(82, &m.id), Some(b(&(progress.root(), progress))));
    builder.write_kernel_row(TABLE, k(84, &job), Some(b(&(m.id, component))));
    Ok(())
}
/// Material preparation may borrow an output only from its own earlier
/// genuinely Final component, through the exact original declared input edge.
pub(super) fn authenticate_upstream(
    r: &PalwKernelRouteStateV1,
    id: Digest,
    d: &MaterialDescriptorV39,
    job: Digest,
) -> Result<(), PalwStateV2Error> {
    let m = record(r, id)?;
    let pr = progress(r, &m)?;
    let MaterialOriginV39::Upstream { master, stage, class, source, claim, final_book, single_credit, output: _, .. } = &d.origin
    else {
        return Err(bad("Master39 original upstream origin absent"));
    };
    let i = *stage as usize;
    let c = m.components.get(i).ok_or_else(|| bad("Master39 original upstream component index"))?;
    let funded = pr.components.get(i).and_then(Option::as_ref).ok_or_else(|| bad("Master39 upstream not actually funded"))?;
    if *master != id
        || d.master != id
        || funded.job != job
        || c.class != *class
        || c.source != *source
        || d.requester != palw_kernel_bond_id_v1(&m.owner)
        || d.requester_operator != m.operator
    {
        return Err(bad("Master39 original upstream owner/component/output declaration differs"));
    }
    let (book, credit) = original_final(r, funded)?;
    if book.root() != *final_book || book.claim().map(|c| c.claim) != Some(*claim) || credit.root() != *single_credit {
        return Err(bad("Master39 upstream Final/once-credit identity differs"));
    }
    let target =
        pr.components.iter().position(Option::is_none).ok_or_else(|| bad("Master39 no original downstream component remains"))?;
    if target <= i || m.components[target].source != d.source {
        return Err(bad("Master39 upstream material is not for original next component"));
    }
    let (original, prior, output, target_source): (Digest, u16, Digest, Digest) =
        row(r, 92, &(id, target as u16, d.input as u16), 256)?;
    if original != m.root() || prior as usize != i || output != c.output || target_source != d.source {
        return Err(bad("Master39 material does not match exact immutable declared input edge"));
    }
    Ok(())
}
fn close(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    raw: &[u8],
) -> Result<(), PalwStateV2Error> {
    let id: Digest = raw.try_into().map_err(|_| bad("Master39 exact original close identity"))?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let m = record(r, id)?;
    let progress = progress(r, &m)?;
    let extent = progress.components.iter().flatten().try_fold(131072u64, |n, c| {
        let h = super::palw_native_job_fold_v39::header(r, c.job)?;
        n.checked_add(h.book_bytes as u64 + 32768).ok_or_else(|| bad("Master39 original close extent"))
    })?;
    if r.aux.contains_key(&(TABLE, k(83, &id))) || r.aux.contains_key(&(TABLE, k(87, &id))) {
        return Err(bad("Master39 terminal already recorded"));
    }
    if !prepay(builder, ctx, signer, p, extent, 131072)? {
        return Ok(());
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let mut components = Vec::with_capacity(m.components.len());
    let mut all_final = true;
    for c in &progress.components {
        if let Some(c) = c {
            let h = super::palw_native_job_fold_v39::header(r, c.job)?;
            if matches!(h.terminal, NativeJobTerminalV39::Funded | NativeJobTerminalV39::Challengeable) {
                return Err(bad("Master39 cannot collect live original child obligations"));
            }
            if h.terminal == NativeJobTerminalV39::Final {
                let (book, credit) = original_final(r, c)?;
                components.push(NativeMasterComponentFinalV39 {
                    original: c.clone(),
                    claim: book.claim().unwrap().claim,
                    final_book: book.root(),
                    settlement: credit.settlement,
                    single_credit: credit.root(),
                    payout: credit.payout,
                    queue: credit.queue,
                    amount: credit.amount,
                    finalized: credit.established,
                });
            } else {
                all_final = false
            }
        } else {
            all_final = false
        }
    }
    if all_final {
        let final_record = NativeMasterFinalRecordV39 {
            version: 39,
            master: id,
            original_record: m.root(),
            complete_source: m.complete_source,
            request: m.request,
            single_credit_set: object_id(
                b"misaka-palw/native/original-Master-single-credit-set/v39",
                &(id, components.iter().map(|c| (c.original.job, c.single_credit, c.payout, c.queue, c.amount)).collect::<Vec<_>>()),
            ),
            components,
            established: ctx.daa_score,
        };
        builder.write_kernel_row(TABLE, k(83, &id), Some(b(&(final_record.root(), final_record))));
    } else {
        if ctx.daa_score <= m.deadline {
            return Err(bad("Master39 original incomplete deadline has not expired"));
        }
        builder.write_kernel_row(TABLE, k(87, &id), Some(b(&(m.root(), ctx.daa_score))));
    }
    // Header, funded bindings and immutable terminal credit roots remain. A
    // terminal Master never creates another payment, receipt or quota refund.
    let mut ids = catalog(builder.state.kernel_route.as_ref().unwrap(), &m.owner)?;
    ids.retain(|v| *v != id);
    builder.write_kernel_row(TABLE, k(85, &m.owner), Some(b(&ids)));
    Ok(())
}
pub(super) fn apply(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    action: u8,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    let p = super::palw_native_source_fold_v39::policy(builder, ctx)?;
    match action {
        65 => register(builder, ctx, signer, &p, body),
        66 => close(builder, ctx, signer, &p, body),
        _ => Err(bad("Master39 paged registration is not yet connected")),
    }
}
