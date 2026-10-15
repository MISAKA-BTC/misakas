//! Rooted public39 acquisition, producer-owned answers, and objective DA default.
//! Invalid/duplicate requests never consume another producer's guarantee. Whole
//! answer attempts are debited before the first fragment, restore or decoder.
use super::*;
use misaka_palw_kernel::flat_public39::*;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(super) struct Pending {
    pub(super) version: u16,
    pub(super) id: Digest,
    pub(super) job: Digest,
    pub(super) funding: Digest,
    pub(super) claim: Digest,
    pub(super) unit: Digest,
    pub(super) requester: PalwBondKeyV2,
    pub(super) operator: Digest,
    pub(super) owner: Digest,
    pub(super) source_plan: Digest,
    pub(super) opened: u64,
    pub(super) until: u64,
    pub(super) bond: u64,
    pub(super) settled: bool,
    pub(super) delivered: bool,
    pub(super) demand: Vec<u8>,
    pub(super) attempts: u8,
    pub(super) group: Digest,
    pub(super) total: u32,
    pub(super) count: u16,
    pub(super) accepted: u8,
}
impl Pending {
    fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-rooted-public-pending/v39", self)
    }
}
pub(super) fn pending(r: &PalwKernelRouteStateV1, id: Digest) -> Result<Pending, PalwStateV2Error> {
    let (root, p): (Digest, Pending) = read(r, 40, &id, 32768)?;
    if p.root() != root
        || p.version != 39
        || p.id != id
        || p.demand.len() > REQUEST_MAX_BYTES_V39
        || p.attempts > 2
        || p.count > 4
        || p.total > REPLY_MAX_BYTES_V39 as u32
        || p.until <= p.opened
        || p.bond == 0
    {
        return Err(bad("native39 original public pending differs"));
    }
    Ok(p)
}
fn write_pending(builder: &mut TransitionBuilder<'_>, p: &Pending) {
    builder.write_kernel_row(TABLE, k(40, &p.id), Some(b(&(p.root(), p))));
}
fn source(
    builder: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    p: &NativeExecutionPoliciesV39,
    h: &JobHeader,
    q: &CheckedQuotaV39,
) -> Result<SourceShardV39, PalwStateV2Error> {
    let e = epoch(p, ctx)?;
    let s = super::super::palw_flat_execution_fold_v11::quota_shard(
        builder,
        &p.execution,
        &e,
        palw_kernel_bond_id_v1(&h.original.producer),
        h.id,
    )?;
    let g = s.global_row();
    let a = s.actor_row();
    let subject = s.subject_row().ok_or_else(|| bad("native39 original public subject absent"))?;
    let raw = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .aux
        .get(&(QUOTA, b(&(39u8, 0u8, h.id))))
        .filter(|raw| raw.len() < 4096)
        .ok_or_else(|| bad("native39 original source obligation absent"))?;
    let (root, row): (Digest, SourceObligationV39) =
        borsh::from_slice(raw).map_err(|_| bad("native39 original source row grammar"))?;
    SourceShardV39::from_native_rows(
        &p.execution.quota,
        q,
        &g,
        &g.root(),
        (&a, &a.root()),
        (subject, &subject.row_root()),
        Some((&row, &root)),
    )
    .map_err(bad)
}
fn check_demand(
    builder: &TransitionBuilder<'_>,
    p: &NativeExecutionPoliciesV39,
    h: &JobHeader,
    d: &FlatDemandV39,
    reply: Option<&FlatReplyV39>,
) -> Result<(), PalwStateV2Error> {
    let (book, _, class, max, _, _) = restore_job(builder, p, h)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let raw = r
        .aux
        .get(&(TABLE, k(28, &h.id)))
        .filter(|raw| raw.len() == h.post_bytes as usize)
        .ok_or_else(|| bad("native39 accepted original post absent"))?;
    let post = NativeCommitWireV39::from_bytes(raw, h.original.maximum_positions).map_err(bad)?;
    let reader = NativeMaterialReader { route: r };
    let inputs = ActualInputResolverV39::derive(class.source(), &max, &post.generated, &reader).map_err(bad)?;
    let c = misaka_palw_kernel::flat_point39::CheckedPointContextV39::check(
        class.source(),
        &max,
        book.claim().unwrap().claim,
        &post.claim,
        &inputs,
    )
    .map_err(bad)?;
    check_flat_demand_v39(&c, d).map_err(bad)?;
    if let Some(v) = reply {
        check_flat_reply_v39(&c, d, v).map_err(bad)?;
    }
    Ok(())
}
// Request=39:u16,job64,original paid owner64 (zero selects ordinary),len:u32,body.
fn request(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    raw: &[u8],
) -> Result<(), PalwStateV2Error> {
    if raw.len() < 134 || raw.len() > 134 + REQUEST_MAX_BYTES_V39 || raw[..2] != 39u16.to_le_bytes() {
        return Err(bad("native39 public request bounded prefix"));
    }
    let job = raw[2..66].try_into().unwrap();
    let paid = raw[66..130].try_into().unwrap();
    let n = u32::from_le_bytes(raw[130..134].try_into().unwrap()) as usize;
    if n != raw.len() - 134 {
        return Err(bad("native39 public request nested extent"));
    }
    let h = header(builder.state.kernel_route.as_ref().unwrap(), job)?;
    if h.terminal != NativeJobTerminalV39::Challengeable {
        return Err(bad("native39 public request original claim not open"));
    }
    let actor = builder.state.bonds.get(signer).ok_or_else(|| bad("native39 public requester absent"))?;
    let operator = actor.operator_id.as_bytes();
    if *signer == h.original.producer || operator == h.original.producer_operator {
        return Err(bad("native39 public requester not independent"));
    }
    let mut owner = if paid == [0; 64] {
        if !prepay(builder, ctx, signer, p, h.source_bytes as u64 + h.consent_bytes as u64 + h.post_bytes as u64, raw.len(), 32768)? {
            return Ok(());
        }
        None
    } else {
        let o = protected::owner(builder.state.kernel_route.as_ref().unwrap(), paid)?;
        if o.actor != *signer || o.operator != operator {
            return Err(bad("native39 public request cannot spend another owner's ticket"));
        }
        Some(protected::query_debit(builder, ctx, p, &h, paid, raw)?)
    };
    // Paid caller restore/request credit has already been consumed. Bad typed
    // coordinates are dropped, preserving that original debit and attempt.
    let Ok(d) = FlatDemandV39::from_bytes(&raw[134..]) else { return Ok(()) };
    if check_demand(builder, p, &h, &d, None).is_err() {
        return Ok(());
    }
    let (book, _, _, _, q, max) = restore_job(builder, p, &h)?;
    let claim = book.claim().unwrap();
    let mut st = protected::status(builder.state.kernel_route.as_ref().unwrap(), &h)?;
    let unit = object_id(b"misaka-palw/native/original-public-unit/v39", &(job, claim.claim, d.root()));
    let r = builder.state.kernel_route.as_ref().unwrap();
    if r.aux.contains_key(&(TABLE, k(42, &unit))) || r.aux.contains_key(&(TABLE, k(41, &unit))) {
        return Ok(());
    }
    let ordinary_pending = st.pending.iter().filter_map(|id| pending(r, *id).ok()).filter(|p| p.owner == [0; 64]).count();
    let latest = claim
        .challenge_end
        .checked_sub(max.record().temporal.acquisition)
        .ok_or_else(|| bad("native39 complete public cutoff absent"))?;
    if st.pending.len() >= 11
        || owner.is_none() && (st.ordinary >= 32 || ordinary_pending >= 3 || ctx.daa_score > latest)
        || owner.as_ref().is_some_and(|o| o.pending >= 2)
    {
        return Err(bad("native39 original independently guaranteed open slots/full request lifetime exhausted"));
    }
    let mut actor_ids = list(r, 44, signer)?;
    if actor_ids.len() >= 8 {
        return Err(bad("native39 original public requester pending catalog full"));
    }
    let bond = r.header.policy.demand_bond;
    let actor = builder.state.bonds.get(signer).unwrap();
    if (actor.collateral as u128).saturating_sub(builder.committed_at(signer, ctx.daa_score)) < bond as u128 {
        return Err(bad("native39 original actual demand bond unavailable"));
    }
    let until =
        ctx.daa_score.checked_add(max.record().temporal.answer_window).ok_or_else(|| bad("native39 public answer clock overflow"))?;
    if until >= h.original.deadline || owner.as_ref().is_some_and(|o| until >= o.until) {
        return Err(bad("native39 original answer availability horizon exhausted"));
    }
    let id =
        object_id(b"misaka-palw/native/original-public-demand-id/v39", &(job, unit, *signer, operator, paid, ctx.block.as_bytes()));
    let e = epoch(p, ctx)?;
    let mut source = source(builder, ctx, p, &h, &q)?;
    source.admit_valid_unit(&p.execution.quota, &e, &q, job).map_err(bad)?;
    write_source(builder, &source, job);
    let v = Pending {
        version: 39,
        id,
        job,
        funding: book.funding(),
        claim: claim.claim,
        unit,
        requester: *signer,
        operator,
        owner: paid,
        source_plan: q.source().root(),
        opened: ctx.daa_score,
        until,
        bond,
        settled: false,
        delivered: false,
        demand: d.to_bytes(),
        attempts: 0,
        group: [0; 64],
        total: 0,
        count: 0,
        accepted: 0,
    };
    st.pending.push(id);
    if let Some(o) = &mut owner {
        o.pending += 1;
        protected::set_owner(builder, o);
    } else {
        st.ordinary += 1;
    }
    actor_ids.push(id);
    // Collection53 erases every unit ever opened for this job from this index.
    super::collect::note_opened(builder, job, id)?;
    write_pending(builder, &v);
    protected::write_status(builder, &st);
    builder.write_kernel_row(TABLE, k(41, &unit), Some(b(&id)));
    builder.write_kernel_row(TABLE, k(44, signer), Some(b(&actor_ids)));
    builder.note_kernel_bond(signer);
    Ok(())
}
fn settle_pending(
    builder: &mut TransitionBuilder<'_>,
    h: &JobHeader,
    v: &mut Pending,
    delivered: bool,
) -> Result<(), PalwStateV2Error> {
    let mut st = protected::status(builder.state.kernel_route.as_ref().unwrap(), h)?;
    if v.settled || !st.pending.contains(&v.id) {
        return Err(bad("native39 original pending was already settled"));
    }
    if delivered && builder.slash_bond(v.requester, v.bond as u128)? != v.bond {
        return Err(bad("native39 original collected request payment changed"));
    }
    if v.owner != [0; 64] {
        let mut o = protected::owner(builder.state.kernel_route.as_ref().unwrap(), v.owner)?;
        if o.job != h.id || o.pending == 0 {
            return Err(bad("native39 original owner pending count differs"));
        }
        o.pending -= 1;
        protected::set_owner(builder, &o);
    }
    st.pending.retain(|id| *id != v.id);
    let mut ids = list(builder.state.kernel_route.as_ref().unwrap(), 44, &v.requester)?;
    ids.retain(|id| *id != v.id);
    v.settled = true;
    v.delivered = delivered;
    write_pending(builder, v);
    protected::write_status(builder, &st);
    builder.write_kernel_row(TABLE, k(44, &v.requester), Some(b(&ids)));
    builder.write_kernel_row(TABLE, k(41, &v.unit), None);
    Ok(())
}
// Answer=39:u16,kind0,pending64,group64,indexu16,countu16,totalu32,len:u32,bytes.
fn answer(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    raw: &[u8],
) -> Result<(), PalwStateV2Error> {
    if raw.len() < 143 || raw.len() > 143 + 32768 || raw[..2] != 39u16.to_le_bytes() || raw[2] != 0 {
        return Err(bad("native39 producer answer bounded prefix"));
    }
    let id = raw[3..67].try_into().unwrap();
    let group = raw[67..131].try_into().unwrap();
    let index = u16::from_le_bytes(raw[131..133].try_into().unwrap());
    let count = u16::from_le_bytes(raw[133..135].try_into().unwrap());
    let total = u32::from_le_bytes(raw[135..139].try_into().unwrap());
    let n = u32::from_le_bytes(raw[139..143].try_into().unwrap()) as usize;
    let mut v = pending(builder.state.kernel_route.as_ref().unwrap(), id)?;
    let h = header(builder.state.kernel_route.as_ref().unwrap(), v.job)?;
    if h.terminal != NativeJobTerminalV39::Challengeable
        || h.original.producer != *signer
        || v.settled
        || ctx.daa_score > v.until
        || group == [0; 64]
        || count == 0
        || count > 4
        || total == 0
        || total > REPLY_MAX_BYTES_V39 as u32
        || count as u64 != (total as u64).div_ceil(32768)
        || index >= count
    {
        return Err(bad("native39 answer original live producer/unit/geometry differs"));
    }
    if v.group == [0; 64] {
        if v.attempts >= 2 {
            return Err(bad("native39 original producer answer attempts exhausted"));
        }
        // Exact cached quota restoration follows a fixed original Metadata
        // producer debit. The full answer guarantee is then consumed once for
        // ALL four fragments and assembly before the first byte is copied.
        debit_cached_answer(builder, ctx, p, &h, v.source_plan, total as u64)?;
        v.attempts += 1;
        v.group = group;
        v.total = total;
        v.count = count;
        v.accepted = 0;
        write_pending(builder, &v);
    } else if v.group != group || v.total != total || v.count != count {
        return Err(bad("native39 answer cannot rebind original partial group"));
    }
    if v.accepted & (1 << index) != 0 {
        return Err(bad("native39 original answer fragment already accepted"));
    }
    let expected = (total as u64 - index as u64 * 32768).min(32768) as usize;
    if n != raw.len() - 143 || n != expected {
        return Ok(());
    }
    builder.write_kernel_row(TABLE, k(43, &(v.id, v.attempts, index)), Some(raw[143..].to_vec()));
    v.accepted |= 1 << index;
    write_pending(builder, &v);
    if (0..count).any(|i| v.accepted & (1 << i) == 0) {
        return Ok(());
    }
    let mut reply = Vec::with_capacity(total as usize);
    for i in 0..count {
        let Some(part) = builder.state.kernel_route.as_ref().unwrap().aux.get(&(TABLE, k(43, &(v.id, v.attempts, i)))) else {
            return Ok(());
        };
        reply.extend_from_slice(part);
    }
    let valid = object_id(b"misaka-palw/native/original-public-reply-file/v39", &reply) == group && reply.len() == total as usize;
    let result = if valid {
        match (FlatDemandV39::from_bytes(&v.demand), FlatReplyV39::from_bytes(&reply)) {
            (Ok(d), Ok(a)) => check_demand(builder, p, &h, &d, Some(&a)).is_ok(),
            _ => false,
        }
    } else {
        false
    };
    if !result {
        v.group = [0; 64];
        v.accepted = 0;
        write_pending(builder, &v);
        return Ok(());
    }
    settle_pending(builder, &h, &mut v, true)?;
    builder.write_kernel_row(TABLE, k(42, &v.unit), Some(b(&(v.root(), &v.demand, &reply))));
    Ok(())
}
fn default(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    raw: &[u8],
) -> Result<(), PalwStateV2Error> {
    if raw.len() != 66 || raw[..2] != 39u16.to_le_bytes() {
        return Err(bad("native39 DAdefault exact fixed proof"));
    }
    let id = raw[2..66].try_into().unwrap();
    let mut v = pending(builder.state.kernel_route.as_ref().unwrap(), id)?;
    let mut h = header(builder.state.kernel_route.as_ref().unwrap(), v.job)?;
    let caller = builder.state.bonds.get(signer).ok_or_else(|| bad("native39 public default caller absent"))?;
    if v.settled
        || h.terminal != NativeJobTerminalV39::Challengeable
        || ctx.daa_score <= v.until
        || ctx.daa_score > h.original.deadline
        || caller.operator_id.as_bytes() == h.original.producer_operator
    {
        return Err(bad("native39 DAdefault lacks original unanswered unit/deadline/independence"));
    }
    if !prepay(builder, ctx, signer, p, h.source_bytes as u64 + h.consent_bytes as u64 + h.post_bytes as u64, raw.len(), 16384)? {
        return Ok(());
    }
    let (mut book, _, _, _, _, _) = restore_job(builder, p, &h)?;
    let proof = object_id(
        b"misaka-palw/native/original-objective-DA-default/v39",
        &(h.id, h.original.root(), v.id, v.unit, v.until, v.root()),
    );
    let s = book.convict(ctx.daa_score, proof, h.original.reserve, v.operator).map_err(bad)?;
    settle_pending(builder, &h, &mut v, false)?;
    super::credit(builder, ctx, p, &mut h, &book, &s, Some(v.requester))
}
pub(super) fn reserved(r: &PalwKernelRouteStateV1, bond: &PalwBondKeyV2) -> u128 {
    list(r, 44, bond)
        .unwrap_or_default()
        .iter()
        .filter_map(|id| pending(r, *id).ok())
        .filter(|p| !p.settled && p.requester == *bond)
        .fold(0u128, |n, p| n.saturating_add(p.bond as u128))
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
        47 => request(builder, ctx, signer, p, raw),
        48 => answer(builder, ctx, signer, p, raw),
        49 => default(builder, ctx, signer, p, raw),
        _ => Err(bad("native39 public action")),
    }
}

pub(super) fn debit_cached_answer(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    p: &NativeExecutionPoliciesV39,
    h: &JobHeader,
    original_source_plan: Digest,
    bytes: u64,
) -> Result<(), PalwStateV2Error> {
    let e = epoch(p, ctx)?;
    let s = super::super::palw_flat_execution_fold_v11::quota_shard(
        builder,
        &p.execution,
        &e,
        palw_kernel_bond_id_v1(&h.original.producer),
        h.id,
    )?;
    let g = s.global_row();
    let a = s.actor_row();
    let subject = s.subject_row().ok_or_else(|| bad("native39 cached original subject absent"))?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let raw = r
        .aux
        .get(&(QUOTA, b(&(39u8, h.id, 1u8))))
        .filter(|raw| raw.len() < 4096)
        .ok_or_else(|| bad("native39 cached original source plan absent"))?;
    let plan: SourcePlanRecordV39 = borsh::from_slice(raw).map_err(|_| bad("native39 fixed source plan grammar"))?;
    if plan.root() != h.original.source_plan || original_source_plan != plan.root() {
        return Err(bad("native39 original pending/source plan mismatch"));
    }
    let raw = r
        .aux
        .get(&(QUOTA, b(&(39u8, 0u8, h.id))))
        .filter(|raw| raw.len() < 4096)
        .ok_or_else(|| bad("native39 original producer obligation absent"))?;
    let (root, row): (Digest, SourceObligationV39) = borsh::from_slice(raw).map_err(|_| bad("native39 source obligation grammar"))?;
    let updated = debit_native_cached_answer_v39(
        &p.execution.quota,
        &e,
        &plan,
        &h.original.source_plan,
        &g,
        &g.root(),
        (&a, &a.root()),
        (subject, &subject.row_root()),
        (&row, &root),
        bytes,
    )
    .map_err(bad)?;
    write_source(builder, &updated, h.id);
    Ok(())
}
/// Genuine terminal closure returns original outstanding demand holds without
/// charging innocent requesters for a response that was never delivered.
pub(super) fn cancel_remaining(builder: &mut TransitionBuilder<'_>, h: &JobHeader) -> Result<(), PalwStateV2Error> {
    let ids = protected::status(builder.state.kernel_route.as_ref().unwrap(), h)?.pending;
    for id in ids {
        let mut v = pending(builder.state.kernel_route.as_ref().unwrap(), id)?;
        if !v.settled {
            settle_pending(builder, h, &mut v, false)?;
        }
    }
    Ok(())
}
