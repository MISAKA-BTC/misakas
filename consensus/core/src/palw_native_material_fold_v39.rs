//! Requester-owned39 material bytes. A Start prepays ALL original pages,
//! failed attempts, whole streamed validation and retention before accepting data.
//! Parts never consume another producer, challenger or ordinary pool reservation.
use super::*;
use misaka_palw_kernel::flat_material_output39::*;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct MaterialHeader {
    version: u16,
    id: Digest,
    class: Digest,
    source_bytes: u32,
    descriptor: MaterialDescriptorV39,
    plan: Digest,
    owner: PalwBondKeyV2,
    operator: Digest,
    admitted: u64,
    deadline: u64,
    progress: Digest,
    receipt: Digest,
    certificate: Option<OutputCertificatePlanV39>,
    attached: Option<Digest>,
}
impl MaterialHeader {
    fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-requester-material-header/v39", self)
    }
}
fn header(r: &PalwKernelRouteStateV1, id: Digest) -> Result<MaterialHeader, PalwStateV2Error> {
    let (root, h): (Digest, MaterialHeader) = read(r, 60, &id, 4096)?;
    if h.version != 39
        || h.id != id
        || h.descriptor.root() != id
        || h.root() != root
        || h.receipt == [0; 64]
        || h.operator != h.descriptor.requester_operator
        || palw_kernel_bond_id_v1(&h.owner) != h.descriptor.requester
    {
        return Err(bad("native39 original material header relation differs"));
    }
    Ok(h)
}
fn plan(h: &MaterialHeader) -> Result<CheckedMaterialPlanV39, PalwStateV2Error> {
    let p = CheckedMaterialPlanV39::derive(&h.descriptor, h.admitted, h.deadline).map_err(bad)?;
    if p.root() != h.plan {
        return Err(bad("native39 original material plan differs"));
    }
    Ok(p)
}
fn progress(
    r: &PalwKernelRouteStateV1,
    h: &MaterialHeader,
    p: &CheckedMaterialPlanV39,
) -> Result<MaterialProgressV39, PalwStateV2Error> {
    let raw = r.aux.get(&(TABLE, k(61, &h.id))).ok_or_else(|| bad("native39 paid original material progress absent"))?;
    MaterialProgressV39::restore(raw, p, h.progress).map_err(bad)
}
fn write(builder: &mut TransitionBuilder<'_>, h: &mut MaterialHeader, s: &MaterialProgressV39) -> Result<(), PalwStateV2Error> {
    h.progress = s.root();
    builder.write_kernel_row(TABLE, k(61, &h.id), Some(b(s)))?;
    builder.write_kernel_row(TABLE, k(60, &h.id), Some(b(&(h.root(), h))))?;
    Ok(())
}
pub(super) struct NativeMaterialReader<'a> {
    pub route: &'a PalwKernelRouteStateV1,
}
impl MaterialPageReaderV39 for NativeMaterialReader<'_> {
    fn page(&self, descriptor: Digest, index: u16) -> Result<MaterialPageV39, String> {
        let raw = self.route.aux.get(&(TABLE, k(62, &(descriptor, index)))).ok_or("native39 original paid material page absent")?;
        MaterialPageV39::from_bytes(raw)
    }
}
fn pay_complete(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    m: &CheckedMaterialPlanV39,
) -> Result<bool, PalwStateV2Error> {
    let r = m.record();
    let total = r
        .hashing_bytes
        .checked_add(r.read_calls.checked_mul(4096).ok_or_else(|| bad("native39 material fullread tariff overflow"))?)
        .and_then(|n| n.checked_add(r.expanded_bytes as u128 * 4))
        .ok_or_else(|| bad("native39 material complete tariff overflow"))?;
    let work = u64::try_from(total).map_err(|_| bad("native39 material complete tariff outside monetary range"))?;
    let Some(fee) = super::super::palw_flat_model_vector_fold_v3::charge(builder, ctx, signer, work, false)? else { return Ok(false) };
    if builder.slash_bond(*signer, fee)? != fee as u64 {
        return Err(bad("native39 material original current payment changed"));
    }
    let mut q = ResourceQuantitiesV10::default();
    for (a, v) in [
        (A::MetadataWork, total),
        (A::ScalarSteps, total),
        (A::IndexSteps, r.read_calls),
        (A::ValidatedElements, r.read_calls),
        (A::CopiedReferenceBytes, r.delivery_bytes as u128 + r.expanded_bytes as u128 * 4),
        (A::OwnedPeakBytes, r.owned_bytes as u128),
        (A::CourtScratchBytes, r.scratch_bytes as u128),
        (A::StoredEvidenceBytes, r.stored_bytes as u128 + 131072),
        (A::DeliveryBytes, r.delivery_bytes as u128),
        (A::TraceHashBytes, r.hashing_bytes),
        (A::ContainerAllocatorAllowance, r.owned_bytes as u128),
        (A::Attempts, r.pages as u128 * 2 + 2),
    ] {
        q.set(a, v);
    }
    let e = epoch(p, ctx)?;
    let mut shard =
        super::super::palw_flat_execution_fold_v11::quota_shard(builder, &p.execution, &e, palw_kernel_bond_id_v1(signer), [0; 64])?;
    shard.debit_ordinary(&p.execution.quota, &e, &DerivedOrdinaryMetadataV11::from_native_envelope(q)).map_err(bad)?;
    super::super::palw_flat_execution_fold_v11::write_shard(builder, &shard, [0; 64])?;
    Ok(true)
}
// Start =39:u16,class:64,deadline:u64,descriptor_len:u32,canonicaldescriptor.
fn start(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    raw: &[u8],
) -> Result<(), PalwStateV2Error> {
    if raw.len() < 79 || raw.len() > 79 + 2048 + 516 || raw[..2] != 39u16.to_le_bytes() {
        return Err(bad("native39 materialStart bounded prefix"));
    }
    let class_id = raw[2..66].try_into().unwrap();
    let until = u64::from_le_bytes(raw[66..74].try_into().unwrap());
    let n = u32::from_le_bytes(raw[74..78].try_into().unwrap()) as usize;
    if n > 2048 || 78 + n >= raw.len() {
        return Err(bad("native39 material descriptor exact extent"));
    }
    let tail = &raw[78 + n..];
    let certificate = match tail.first() {
        Some(0) if tail.len() == 1 => None,
        Some(1) if tail.len() >= 5 => {
            let m = u32::from_le_bytes(tail[1..5].try_into().unwrap()) as usize;
            if m > 512 || tail.len() != 5 + m {
                return Err(bad("native39 original output certificate framing"));
            }
            Some(OutputCertificatePlanV39::from_bytes(&tail[5..]).map_err(bad)?)
        }
        _ => return Err(bad("native39 original output certificate option")),
    };
    let ch = super::super::palw_native_source_fold_v39::class_header(builder.state.kernel_route.as_ref().unwrap(), class_id)?;
    let mut ids = list(builder.state.kernel_route.as_ref().unwrap(), 63, signer)?;
    if ids.len() >= 8 {
        return Err(bad("native39 requester-owned material slots full"));
    }
    if !prepay(builder, ctx, signer, p, ch.source_bytes as u64, raw.len(), 16384)? {
        return Ok(());
    }
    let d = MaterialDescriptorV39::from_bytes(&raw[78..78 + n]).map_err(bad)?;
    let bond = builder.state.bonds.get(signer).ok_or_else(|| bad("native39 material owner absent"))?;
    if !matches!(bond.status, PalwBondStatusV2::Active)
        || d.requester != palw_kernel_bond_id_v1(signer)
        || d.requester_operator != bond.operator_id.as_bytes()
        || until <= ctx.daa_score
        || until >= epoch(p, ctx)?.interval_end(&p.execution.quota).map_err(bad)?
        || until - ctx.daa_score > p.maximum_horizon
    {
        return Err(bad("native39 material original principal/interval/deadline differs"));
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    r.execution_nonce_counter_v11(&d.requester_operator).map_err(bad)?.advance(&d.nonce).map_err(bad)?;
    if r.aux.contains_key(&(TABLE, k(60, &d.root()))) {
        return Err(bad("native39 original material replay"));
    }
    let owner_operator = bond.operator_id.as_bytes();
    let class = super::super::palw_native_source_fold_v39::restore_class(r, p, class_id)?;
    if class.source().root() != d.source {
        return Err(bad("native39 material original source differs"));
    }
    // Native upstream certification is performed at Seal after all original
    // bytes exist, and is rechecked at Fund. External task identity is immutable.
    if let MaterialOriginV39::External { original_task, .. } = &d.origin {
        if *original_task != class.source().original_task_metadata() {
            return Err(bad("native39 material external original task differs"));
        }
    }
    let m = CheckedMaterialPlanV39::derive(&d, ctx.daa_score, until).map_err(bad)?;
    let checked_certificate = check_certificate_plan(builder, ctx, signer, p, &d, certificate.as_ref())?;
    if let Some(cp) = &checked_certificate {
        if !pay_certificate(builder, ctx, signer, p, &m, cp)? {
            return Ok(());
        }
    }
    if !pay_complete(builder, ctx, signer, p, &m)? {
        return Ok(());
    }
    let receipt =
        object_id(b"misaka-palw/native/actual-requester-material-prepayment/v39", &(ctx.block.as_bytes(), *signer, m.root(), d.nonce));
    let s = MaterialProgressV39::new(&m, receipt).map_err(bad)?;
    let mut h = MaterialHeader {
        version: 39,
        id: d.root(),
        class: class_id,
        source_bytes: ch.source_bytes,
        descriptor: d,
        plan: m.root(),
        owner: *signer,
        operator: owner_operator,
        admitted: ctx.daa_score,
        deadline: until,
        progress: s.root(),
        receipt,
        certificate,
        attached: None,
    };
    ids.push(h.id);
    if let Some(cp) = &checked_certificate {
        let cs = CertificateProgress {
            version: 39,
            plan: cp.root(),
            attempts: vec![0; cp.pages() as usize],
            accepted: vec![0; (cp.pages() as usize).div_ceil(8)],
        };
        builder.write_kernel_row(TABLE, k(65, &h.id), Some(b(&(cs.root(), cs))))?;
    }
    write(builder, &mut h, &s)?;
    builder.write_kernel_row(TABLE, k(63, signer), Some(b(&ids)))?;
    Ok(())
}
// Part =39:u16,descriptor:64,index:u16 followed by bounded MaterialPage39.
fn part(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    _p: &NativeExecutionPoliciesV39,
    raw: &[u8],
) -> Result<(), PalwStateV2Error> {
    if raw.len() < 69 || raw.len() > 69 + 32768 + 1024 || raw[..2] != 39u16.to_le_bytes() {
        return Err(bad("native39 materialPart fixed extent"));
    }
    let lane = raw[2];
    if lane > 1 {
        return Err(bad("native39 material page lane"));
    }
    let id = raw[3..67].try_into().unwrap();
    let index = u16::from_le_bytes(raw[67..69].try_into().unwrap());
    if lane == 1 {
        return certificate_part(builder, ctx, signer, id, index, &raw[69..]);
    }
    let mut h = header(builder.state.kernel_route.as_ref().unwrap(), id)?;
    if h.owner != *signer {
        return Err(bad("native39 materialPart original requester required"));
    }
    let m = plan(&h)?;
    let mut s = progress(builder.state.kernel_route.as_ref().unwrap(), &h, &m)?;
    s.debit_part(&m, palw_kernel_bond_id_v1(signer), index, ctx.daa_score).map_err(bad)?;
    write(builder, &mut h, &s)?;
    let Ok(page) = MaterialPageV39::from_bytes(&raw[69..]) else { return Ok(()) };
    if page.index != index || s.accept_part(&m, index, &page.body, &page.siblings).is_err() {
        return Ok(());
    }
    builder.write_kernel_row(TABLE, k(62, &(id, index)), Some(raw[69..].to_vec()))?;
    write(builder, &mut h, &s)?;
    Ok(())
}
fn seal(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    raw: &[u8],
) -> Result<(), PalwStateV2Error> {
    if raw.len() != 66 || raw[..2] != 39u16.to_le_bytes() {
        return Err(bad("native39 materialSeal exact extent"));
    }
    let id = raw[2..66].try_into().unwrap();
    let mut h = header(builder.state.kernel_route.as_ref().unwrap(), id)?;
    if h.owner != *signer {
        return Err(bad("native39 materialSeal original requester required"));
    }
    let m = plan(&h)?;
    let mut s = progress(builder.state.kernel_route.as_ref().unwrap(), &h, &m)?;
    s.debit_seal(&m, palw_kernel_bond_id_v1(signer), ctx.daa_score).map_err(bad)?;
    write(builder, &mut h, &s)?;
    if !prepay(builder, ctx, signer, p, h.source_bytes as u64, raw.len(), 0)? {
        return Ok(());
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let class = super::super::palw_native_source_fold_v39::restore_class(r, p, h.class)?;
    let reader = NativeMaterialReader { route: r };
    let checked = match check_material_image_for_source_v39(class.source(), &m, |offset, count| {
        read_material_range_v39(&h.descriptor, &reader, offset, count)
    }) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    certify_upstream(r, p, &h.descriptor, h.certificate.as_ref())?;
    s.accept_seal(&m, &checked).map_err(bad)?;
    if let Some(cp) = &h.certificate {
        builder.write_kernel_row(TABLE, k(67, &h.id), Some(b(&cp.root())))?;
    }
    write(builder, &mut h, &s)?;
    Ok(())
}
fn collect(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    raw: &[u8],
) -> Result<(), PalwStateV2Error> {
    if raw.len() != 66 || raw[..2] != 39u16.to_le_bytes() {
        return Err(bad("native39 materialCollect exact extent"));
    }
    let id = raw[2..66].try_into().unwrap();
    let h = header(builder.state.kernel_route.as_ref().unwrap(), id)?;
    if ctx.daa_score <= h.deadline || h.attached.is_some() {
        return Err(bad("native39 original material still retained by original contract"));
    }
    let m = plan(&h)?;
    if !prepay(builder, ctx, signer, p, 16384, raw.len(), 0)? {
        return Ok(());
    }
    for i in 0..m.record().pages {
        builder.write_kernel_row(TABLE, k(62, &(id, i)), None)?;
    }
    if let Some(cp) = &h.certificate {
        for i in 0..cp.bytes.div_ceil(32768) as u16 {
            builder.write_kernel_row(TABLE, k(66, &(cp.root(), i)), None)?;
        }
        builder.write_kernel_row(TABLE, k(65, &id), None)?;
        builder.write_kernel_row(TABLE, k(67, &id), None)?;
    }
    builder.write_kernel_row(TABLE, k(60, &id), None)?;
    builder.write_kernel_row(TABLE, k(61, &id), None)?;
    let mut ids = list(builder.state.kernel_route.as_ref().unwrap(), 63, &h.owner)?;
    ids.retain(|x| *x != id);
    builder.write_kernel_row(TABLE, k(63, &h.owner), Some(b(&ids)))?;
    builder.write_kernel_row(TABLE, k(64, &id), Some(b(&(h.plan, h.receipt, h.deadline))))?;
    Ok(())
}
pub(super) fn authenticate_material(
    r: &PalwKernelRouteStateV1,
    m: &MaterialManifestV39,
    owner: &PalwBondKeyV2,
    operator: Digest,
    nonce: Digest,
    now: u64,
) -> Result<(), PalwStateV2Error> {
    for input in &m.bindings {
        if let InputBindingV39::Material { descriptor, .. } = input {
            let h = header(r, descriptor.root())?;
            let p = plan(&h)?;
            let s = progress(r, &h, &p)?;
            if h.owner != *owner
                || h.operator != operator
                || h.descriptor != *descriptor
                || descriptor.nonce != nonce
                || h.deadline < now
                || !s.sealed
                || h.attached.is_some()
            {
                return Err(bad("native39 input material lacks exact original requester paid seal/nonce/availability"));
            }
            if let Some(cp) = &h.certificate {
                let proved: Digest = read(r, 67, &h.id, 64)?;
                if proved != cp.root() {
                    return Err(bad("native39 original upstream certificate was not sealed"));
                }
            }
        }
    }
    Ok(())
}
pub(super) fn apply(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    action: u8,
    raw: &[u8],
) -> Result<(), PalwStateV2Error> {
    match action {
        61 => start(builder, ctx, signer, p, raw),
        62 => part(builder, ctx, signer, p, raw),
        63 => seal(builder, ctx, signer, p, raw),
        64 => collect(builder, ctx, signer, p, raw),
        _ => Err(bad("native39 material action")),
    }
}

pub(super) fn attach(
    builder: &mut TransitionBuilder<'_>,
    m: &MaterialManifestV39,
    job: Digest,
    until: u64,
) -> Result<(), PalwStateV2Error> {
    let mut updates = Vec::new();
    for binding in &m.bindings {
        if let InputBindingV39::Material { descriptor, .. } = binding {
            let mut h = header(builder.state.kernel_route.as_ref().unwrap(), descriptor.root())?;
            if h.attached.is_some() || h.deadline < until {
                return Err(bad("native39 original material does not cover complete funded availability horizon"));
            }
            h.attached = Some(job);
            updates.push(h);
        }
    }
    for h in updates {
        builder.write_kernel_row(TABLE, k(60, &h.id), Some(b(&(h.root(), h))))?;
    }
    Ok(())
}
pub(super) fn detach(builder: &mut TransitionBuilder<'_>, m: &MaterialManifestV39, job: Digest) -> Result<(), PalwStateV2Error> {
    for binding in &m.bindings {
        if let InputBindingV39::Material { descriptor, .. } = binding {
            let mut h = header(builder.state.kernel_route.as_ref().unwrap(), descriptor.root())?;
            if h.attached != Some(job) {
                return Err(bad("native39 original material attachment differs"));
            }
            h.attached = None;
            builder.write_kernel_row(TABLE, k(60, &h.id), Some(b(&(h.root(), h))))?;
        }
    }
    Ok(())
}

pub(super) fn preflight_attach(r: &PalwKernelRouteStateV1, m: &MaterialManifestV39, until: u64) -> Result<(), PalwStateV2Error> {
    for binding in &m.bindings {
        if let InputBindingV39::Material { descriptor, .. } = binding {
            let h = header(r, descriptor.root())?;
            if h.attached.is_some() || h.deadline < until {
                return Err(bad("native39 original material cannot cover entire original funded horizon"));
            }
        }
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct CertificateProgress {
    version: u16,
    plan: Digest,
    attempts: Vec<u8>,
    accepted: Vec<u8>,
}
impl CertificateProgress {
    fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-output-certificate-progress/v39", self)
    }
    fn has(&self, index: u16) -> bool {
        self.accepted[index as usize / 8] & (1 << (index % 8)) != 0
    }
}
fn certificate_progress(r: &PalwKernelRouteStateV1, h: &MaterialHeader) -> Result<CertificateProgress, PalwStateV2Error> {
    let (root, s): (Digest, CertificateProgress) = read(r, 65, &h.id, 12288)?;
    let p = h.certificate.as_ref().ok_or_else(|| bad("native39 original output certificate absent"))?;
    let n = p.bytes.div_ceil(32768) as usize;
    if s.version != 39
        || s.plan != p.root()
        || root != s.root()
        || n == 0
        || n > 8192
        || s.attempts.len() != n
        || s.accepted.len() != n.div_ceil(8)
        || s.attempts.iter().any(|n| *n > 2)
    {
        return Err(bad("native39 original certificate attempts/geometry differs"));
    }
    Ok(s)
}
impl OutputCertificatePageReaderV39 for NativeMaterialReader<'_> {
    fn page(&self, certificate: Digest, index: u16) -> Result<MaterialPageV39, String> {
        let raw =
            self.route.aux.get(&(TABLE, k(66, &(certificate, index)))).ok_or("native39 original output certificate page absent")?;
        MaterialPageV39::from_bytes(raw)
    }
}
fn upstream_final(
    r: &PalwKernelRouteStateV1,
    p: &NativeExecutionPoliciesV39,
    d: &MaterialDescriptorV39,
) -> Result<(CheckedNativeClassV39, misaka_palw_kernel::flat_point39::FlatClaimV39), PalwStateV2Error> {
    let MaterialOriginV39::Upstream { master, job, class, source, claim, final_book, single_credit, .. } = &d.origin else {
        return Err(bad("native39 upstream material origin absent"));
    };
    let h = super::header(r, *job)?;
    let book = super::book(r, &h)?;
    let (root, credit): (Digest, NativeSingleCreditV39) = read(r, 23, job, 1024)?;
    if *master != d.master
        || h.terminal != NativeJobTerminalV39::Final
        || book.root() != *final_book
        || book.claim().map(|c| c.claim) != Some(*claim)
        || h.original.class != *class
        || h.original.source != *source
        || credit.root() != root
        || root != *single_credit
        || credit.job != *job
        || credit.funding != book.funding()
        || credit.terminal != book.root()
        || credit.claim != Some(*claim)
        || credit.amount != h.original.reward
        || credit.recipient != h.original.producer
        || credit.amount == 0
        || book.settlement().is_none_or(|s| s.root() != credit.settlement || s.producer_credit != credit.amount)
    {
        return Err(bad("native39 upstream material lacks genuine original Final/actual once credit"));
    }
    // The registered Master39 sidecar authenticates component order/task/input
    // provenance; a caller cannot borrow an unrelated independently Final job.
    super::super::palw_native_master_fold_v39::authenticate_upstream(r, *master, d, *job)?;
    let c = super::super::palw_native_source_fold_v39::restore_class(r, p, *class)?;
    let raw = r
        .aux
        .get(&(TABLE, k(28, job)))
        .filter(|raw| raw.len() == h.post_bytes as usize)
        .ok_or_else(|| bad("native39 original upstream compact post absent"))?;
    let post = NativeCommitWireV39::from_bytes(raw, h.original.maximum_positions).map_err(bad)?;
    if post.job != *job || post.claim.root() != *claim {
        return Err(bad("native39 original upstream accepted claim differs"));
    }
    Ok((c, post.claim))
}
fn check_certificate_plan(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    d: &MaterialDescriptorV39,
    record: Option<&OutputCertificatePlanV39>,
) -> Result<Option<CheckedOutputCertificatePlanV39>, PalwStateV2Error> {
    match (&d.origin, record) {
        (MaterialOriginV39::External { .. }, None) => Ok(None),
        (MaterialOriginV39::Upstream { job, .. }, Some(record)) => {
            let h = super::header(builder.state.kernel_route.as_ref().unwrap(), *job)?;
            if !prepay(builder, ctx, signer, p, h.source_bytes as u64 + h.consent_bytes as u64 + h.post_bytes as u64, 512, 0)? {
                return Err(bad("native39 original upstream restore tariff unavailable"));
            }
            let (c, claim) = upstream_final(builder.state.kernel_route.as_ref().unwrap(), p, d)?;
            CheckedOutputCertificatePlanV39::check(c.source(), &claim, d, record).map(Some).map_err(bad)
        }
        _ => Err(bad("native39 upstream certificate required exactly for upstream material")),
    }
}
fn pay_certificate(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    m: &CheckedMaterialPlanV39,
    c: &CheckedOutputCertificatePlanV39,
) -> Result<bool, PalwStateV2Error> {
    let cells = m.record().raw_bytes / m.descriptor().dtype.width() as u64;
    let work = c
        .hashing_bytes(m.record().raw_bytes)
        .checked_add(c.read_calls(cells).checked_mul(4096).ok_or_else(|| bad("native39 certificate cost overflow"))?)
        .ok_or_else(|| bad("native39 certificate cost overflow"))?;
    let Some(fee) = super::super::palw_flat_model_vector_fold_v3::charge(
        builder,
        ctx,
        signer,
        u64::try_from(work).map_err(|_| bad("native39 certificate monetary range"))?,
        false,
    )?
    else {
        return Ok(false);
    };
    builder.slash_bond(*signer, fee)?;
    let mut q = ResourceQuantitiesV10::default();
    for (a, v) in [
        (A::MetadataWork, work),
        (A::ScalarSteps, work),
        (A::IndexSteps, c.read_calls(cells)),
        (A::TraceHashBytes, c.hashing_bytes(m.record().raw_bytes)),
        (A::CopiedReferenceBytes, c.delivery_bytes() as u128 * 2),
        (A::OwnedPeakBytes, c.scratch_bytes() as u128 + 65536),
        (A::CourtScratchBytes, c.scratch_bytes() as u128),
        (A::StoredEvidenceBytes, c.stored_bytes() as u128 + 65536),
        (A::DeliveryBytes, c.delivery_bytes() as u128),
        (A::ContainerAllocatorAllowance, c.scratch_bytes() as u128 + 65536),
        (A::Attempts, c.pages() as u128 * 2 + 2),
    ] {
        q.set(a, v);
    }
    let e = epoch(p, ctx)?;
    let mut s =
        super::super::palw_flat_execution_fold_v11::quota_shard(builder, &p.execution, &e, palw_kernel_bond_id_v1(signer), [0; 64])?;
    s.debit_ordinary(&p.execution.quota, &e, &DerivedOrdinaryMetadataV11::from_native_envelope(q)).map_err(bad)?;
    super::super::palw_flat_execution_fold_v11::write_shard(builder, &s, [0; 64])?;
    Ok(true)
}
fn certificate_part(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    id: Digest,
    index: u16,
    raw: &[u8],
) -> Result<(), PalwStateV2Error> {
    let h = header(builder.state.kernel_route.as_ref().unwrap(), id)?;
    let p = h.certificate.as_ref().ok_or_else(|| bad("native39 original certificate plan absent"))?;
    let m = plan(&h)?;
    let s = progress(builder.state.kernel_route.as_ref().unwrap(), &h, &m)?;
    let mut cs = certificate_progress(builder.state.kernel_route.as_ref().unwrap(), &h)?;
    if h.owner != *signer
        || ctx.daa_score < h.admitted
        || ctx.daa_score > h.deadline
        || s.sealed
        || s.consumed
        || index as usize >= cs.attempts.len()
        || cs.attempts[index as usize] >= 2
        || cs.has(index)
    {
        return Err(bad("native39 original requester certificate page unavailable"));
    }
    cs.attempts[index as usize] += 1;
    builder.write_kernel_row(TABLE, k(65, &id), Some(b(&(cs.root(), &cs))))?;
    let Ok(page) = MaterialPageV39::from_bytes(raw) else { return Ok(()) };
    let expected = (p.bytes - index as u64 * 32768).min(32768) as usize;
    if page.index != index
        || page.body.len() != expected
        || misaka_palw_kernel::merkle3::tree_root_from_path(
            output_certificate_page_leaf_v39(p, index, &page.body),
            index as u64,
            cs.attempts.len() as u64,
            &page.siblings,
            output_certificate_page_node_v39,
        ) != Some(p.page_root)
    {
        return Ok(());
    }
    cs.accepted[index as usize / 8] |= 1 << (index % 8);
    builder.write_kernel_row(TABLE, k(66, &(p.root(), index)), Some(raw.to_vec()))?;
    builder.write_kernel_row(TABLE, k(65, &id), Some(b(&(cs.root(), &cs))))?;
    Ok(())
}
fn certify_upstream(
    r: &PalwKernelRouteStateV1,
    p: &NativeExecutionPoliciesV39,
    d: &MaterialDescriptorV39,
    record: Option<&OutputCertificatePlanV39>,
) -> Result<(), PalwStateV2Error> {
    match (&d.origin, record) {
        (MaterialOriginV39::External { .. }, None) => Ok(()),
        (MaterialOriginV39::Upstream { .. }, Some(record)) => {
            let (c, claim) = upstream_final(r, p, d)?;
            let cp = CheckedOutputCertificatePlanV39::check(c.source(), &claim, d, record).map_err(bad)?;
            let reader = NativeMaterialReader { route: r };
            check_upstream_material_output_v39(c.source(), claim.root(), &claim, d, &cp, &reader, &reader).map_err(bad)?;
            Ok(())
        }
        _ => Err(bad("native39 original output certificate origin mismatch")),
    }
}

pub(super) fn authenticate_projections(
    r: &PalwKernelRouteStateV1,
    manifest: &MaterialManifestV39,
    expected: &[(u8, OutputProjectionV39)],
) -> Result<(), PalwStateV2Error> {
    for (input, projection) in expected {
        let binding = manifest.bindings.get(*input as usize).ok_or_else(|| bad("native39 original projected input absent"))?;
        let InputBindingV39::Material { descriptor, .. } = binding else {
            return Err(bad("native39 projected input has no original material"));
        };
        let h = header(r, descriptor.root())?;
        let original = h.certificate.as_ref().ok_or_else(|| bad("native39 original upstream output certificate absent"))?;
        if original.projection != *projection {
            return Err(bad("native39 upstream certificate changes original pipeline rows_from/drop/padding"));
        }
    }
    Ok(())
}
