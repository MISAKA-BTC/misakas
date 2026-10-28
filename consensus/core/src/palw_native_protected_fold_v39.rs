//! Native39 paid discovery/proof streams. All original21 reservations share the
//! existing global/actor/subject counters, while u16/2048 progress owns new rows.
use super::*;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(super) struct Status {
    pub(super) version: u16,
    pub(super) job: Digest,
    pub(super) funding: Digest,
    pub(super) claim: Digest,
    pub(super) owners: [Digest; 4],
    pub(super) admitted: u8,
    pub(super) ordinary: u32,
    pub(super) pending: Vec<Digest>,
    pub(super) latest: u64,
}
impl Status {
    pub(super) fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-public-status/v39", self)
    }
}
pub(super) fn status(r: &PalwKernelRouteStateV1, h: &JobHeader) -> Result<Status, PalwStateV2Error> {
    let (root, s): (Digest, Status) = read(r, 36, &h.id, 2048)?;
    if root != s.root()
        || s.version != 39
        || s.job != h.id
        || s.funding != h.original.root()
        || s.claim
            != if matches!(h.terminal, NativeJobTerminalV39::Funded | NativeJobTerminalV39::Refunded) {
                [0; 64]
            } else {
                book(r, h)?.claim().ok_or_else(|| bad("native39 original claim absent"))?.claim
            }
        || s.admitted > 4
        || s.ordinary > 32
        || s.pending.len() > 11
        || s.pending.iter().enumerate().any(|(i, k)| s.pending[..i].contains(k))
    {
        return Err(bad("native39 original public status/count differs"));
    }
    Ok(s)
}
pub(super) fn initialize(builder: &mut TransitionBuilder<'_>, h: &JobHeader) -> Result<(), PalwStateV2Error> {
    let s = Status {
        version: 39,
        job: h.id,
        funding: h.original.root(),
        claim: [0; 64],
        owners: [[0; 64]; 4],
        admitted: 0,
        ordinary: 0,
        pending: vec![],
        latest: 0,
    };
    write_status(builder, &s)
}
pub(super) fn write_status(builder: &mut TransitionBuilder<'_>, s: &Status) -> Result<(), PalwStateV2Error> {
    builder.write_kernel_row(TABLE, k(36, &s.job), Some(b(&(s.root(), s))))?;
    Ok(())
}
pub(super) fn set_claim(builder: &mut TransitionBuilder<'_>, h: &JobHeader, claim: Digest) -> Result<(), PalwStateV2Error> {
    let mut s: (Digest, Status) = read(builder.state.kernel_route.as_ref().unwrap(), 36, &h.id, 2048)?;
    if s.0 != s.1.root() || s.1.claim != [0; 64] || s.1.admitted != 0 || !s.1.pending.is_empty() {
        return Err(bad("native39 public status cannot replace prior claim"));
    }
    s.1.claim = claim;
    write_status(builder, &s.1)?;
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(super) struct Owner {
    pub(super) version: u16,
    pub(super) id: Digest,
    pub(super) job: Digest,
    pub(super) funding: Digest,
    pub(super) claim: Digest,
    pub(super) maximum: Digest,
    pub(super) plan: Digest,
    pub(super) actor: PalwBondKeyV2,
    pub(super) operator: Digest,
    pub(super) paid: Digest,
    pub(super) proof: Digest,
    pub(super) discovery: bool,
    pub(super) total: u32,
    pub(super) count: u16,
    pub(super) posted: u64,
    pub(super) matures: u64,
    pub(super) until: u64,
    pub(super) interval: u64,
    pub(super) attempts: Vec<u8>,
    pub(super) accepted: Vec<u8>,
    pub(super) assemblies: u8,
    pub(super) courts: u8,
    pub(super) conversion: u8,
    pub(super) queries: u32,
    pub(super) pending: u8,
    pub(super) retired: bool,
    pub(super) spent: bool,
    pub(super) committed: QuotaQuantitiesV11,
    pub(super) consumed: QuotaQuantitiesV11,
    pub(super) remaining: QuotaQuantitiesV11,
}
impl Owner {
    pub(super) fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-paid-discovery-proof/v39", self)
    }
    pub(super) fn has(&self, i: u16) -> bool {
        self.accepted[i as usize / 8] & (1 << (i % 8)) != 0
    }
}
pub(super) fn owner(r: &PalwKernelRouteStateV1, id: Digest) -> Result<Owner, PalwStateV2Error> {
    let (root, s): (Digest, Owner) = read(r, 37, &id, 16384)?;
    if root != s.root()
        || s.version != 39
        || s.id != id
        || s.count == 0
        || s.count > 2048
        || s.total == 0
        || s.total > 64 << 20
        || s.count as u64 != (s.total as u64).div_ceil(32768)
        || s.attempts.len() != 2048
        || s.accepted.len() != 256
        || s.attempts.iter().any(|n| *n > 2)
        || s.assemblies > 2
        || s.courts > 2
        || s.conversion > 1
        || s.pending > 2
        || s.consumed.checked_add(&s.remaining).map_err(bad)? != s.committed
    {
        return Err(bad("native39 original paid owner progress/conservation differs"));
    }
    Ok(s)
}
fn write_owner(builder: &mut TransitionBuilder<'_>, o: &Owner) -> Result<(), PalwStateV2Error> {
    builder.write_kernel_row(TABLE, k(37, &o.id), Some(b(&(o.root(), o))))?;
    Ok(())
}
fn plus(
    s: &QuotaIntervalSummaryV11,
    spent: &QuotaQuantitiesV11,
    reserve: &QuotaQuantitiesV11,
    cap: &QuotaQuantitiesV11,
) -> Result<QuotaIntervalSummaryV11, PalwStateV2Error> {
    let n = QuotaIntervalSummaryV11 {
        consumed: s.consumed.checked_add(spent).map_err(bad)?,
        reserved: s.reserved.checked_add(reserve).map_err(bad)?,
    };
    n.consumed.checked_add(&n.reserved).map_err(bad)?.within(cap).map_err(bad)?;
    Ok(n)
}
fn consume(
    s: &QuotaIntervalSummaryV11,
    d: &QuotaQuantitiesV11,
    cap: &QuotaQuantitiesV11,
) -> Result<QuotaIntervalSummaryV11, PalwStateV2Error> {
    let n = QuotaIntervalSummaryV11 {
        consumed: s.consumed.checked_add(d).map_err(bad)?,
        reserved: s.reserved.checked_sub(d).map_err(bad)?,
    };
    n.consumed.checked_add(&n.reserved).map_err(bad)?.within(cap).map_err(bad)?;
    Ok(n)
}
fn put_rows(
    builder: &mut TransitionBuilder<'_>,
    g: &QuotaGlobalRowV11,
    a: &QuotaActorRowV11,
    s: &QuotaSubjectSummaryV11,
) -> Result<(), PalwStateV2Error> {
    builder.write_kernel_row(QUOTA, vec![0], Some(b(&(g.root(), g))))?;
    builder.write_kernel_row(QUOTA, b(&(1u8, a.actor)), Some(b(&(a.root(), a))))?;
    builder.write_kernel_row(QUOTA, b(&(2u8, s.binding.job_id)), Some(b(&(s.row_root(), s))))?;
    Ok(())
}
fn context(
    builder: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    p: &NativeExecutionPoliciesV39,
    h: &JobHeader,
    o: &Owner,
) -> Result<(TrustedEpochV11, QuotaGlobalRowV11, QuotaActorRowV11, QuotaSubjectSummaryV11), PalwStateV2Error> {
    let e = epoch(p, ctx)?;
    let shard =
        super::super::palw_flat_execution_fold_v11::quota_shard(builder, &p.execution, &e, palw_kernel_bond_id_v1(&o.actor), h.id)?;
    let mut g = shard.global_row();
    let a = shard.actor_row();
    let s = shard.subject_row().ok_or_else(|| bad("native39 original protected subject absent"))?.clone();
    if o.job != h.id
        || o.funding != h.original.root()
        || o.claim != book(builder.state.kernel_route.as_ref().unwrap(), h)?.claim().map(|c| c.claim).unwrap_or([0; 64])
        || o.interval != e.network_interval
        || g.epoch.network_interval != e.network_interval
        || a.network_interval != e.network_interval
        || !a.active
    {
        return Err(bad("native39 original protected interval/actor/book differs"));
    }
    if g.epoch.block_hash != e.block_hash {
        g.block.consumed = QuotaQuantitiesV11::default();
    }
    g.epoch = e.clone();
    Ok((e, g, a, s))
}
fn debit(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    p: &NativeExecutionPoliciesV39,
    h: &JobHeader,
    o: &mut Owner,
    d: &QuotaQuantitiesV11,
) -> Result<(), PalwStateV2Error> {
    let (_, mut g, mut a, s) = context(builder, ctx, p, h, o)?;
    g.block = consume(&g.block, d, &p.execution.quota.block)?;
    g.network = consume(&g.network, d, &p.execution.quota.network)?;
    a.interval = consume(&a.interval, d, &p.execution.quota.actor)?;
    o.consumed = o.consumed.checked_add(d).map_err(bad)?;
    o.remaining = o.remaining.checked_sub(d).map_err(bad)?;
    put_rows(builder, &g, &a, &s)?;
    write_owner(builder, o)?;
    Ok(())
}
fn admit(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    raw: &[u8],
) -> Result<(), PalwStateV2Error> {
    if raw.len() != 139 || raw[..2] != 39u16.to_le_bytes() || raw[138] > 1 {
        return Err(bad("native39 paid owner exact grammar"));
    }
    let job = raw[2..66].try_into().unwrap();
    let proof = raw[66..130].try_into().unwrap();
    let total = u64::from_le_bytes(raw[130..138].try_into().unwrap());
    let discovery = raw[138] == 1;
    let h = header(builder.state.kernel_route.as_ref().unwrap(), job)?;
    if !super::accusable_v39(builder, &h, ctx.daa_score) || *signer == h.original.producer {
        return Err(bad("native39 paid owner requires independent original challengeable claim"));
    }
    if !prepay(builder, ctx, signer, p, h.source_bytes as u64 + h.consent_bytes as u64, raw.len(), 16384)? {
        return Ok(());
    }
    let (book, _, _, _, quota, max) = restore_job(builder, p, &h)?;
    let claim = book.claim().unwrap();
    let mut st = status(builder.state.kernel_route.as_ref().unwrap(), &h)?;
    let actor = builder.state.bonds.get(signer).ok_or_else(|| bad("native39 independent actor absent"))?;
    let operator = actor.operator_id.as_bytes();
    let temporal = &max.record().temporal;
    // RFC08 08-B: past a held Final the opportunity runs to the liability horizon (the deadline below it).
    let horizon = super::accusation_horizon_v39(builder, &h);
    let latest = if h.terminal == NativeJobTerminalV39::Final {
        horizon.checked_sub(temporal.acquisition)
    } else {
        claim.challenge_end.checked_sub(temporal.acquisition)
    }
    .ok_or_else(|| bad("native39 full opportunity cutoff absent"))?;
    let until = ctx
        .daa_score
        .checked_add(temporal.acquisition)
        .and_then(|n| n.checked_add(temporal.proof_window))
        .ok_or_else(|| bad("native39 paid full stream horizon overflow"))?;
    if operator == h.original.producer_operator
        // G14 lane closure G-N1: live owners (an expired owner's slot is freed at its expiry).
        || st.admitted >= 4
        || ctx.daa_score > latest
        || until >= horizon
        || total == 0
        || total > quota.paged().maximum_bytes
        || !discovery && proof == [0; 64]
        || discovery && proof != [0; 64]
    {
        return Err(bad("native39 independent original maximum/full opportunity/proof differs"));
    }
    let request = quota.plan().record().action_envelopes[1].checked_mul(quota.source().requests_per_owner).map_err(bad)?;
    let guarantee = quota.paged().guarantee.checked_add(&request).map_err(bad)?;
    guarantee.within(&p.execution.quota.plan).map_err(bad)?;
    let cost = guarantee
        .get(A::MetadataWork)
        .checked_add(guarantee.get(A::CourtWork))
        .ok_or_else(|| bad("native39 paid owner tariff overflow"))?;
    let fee = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .header
        .policy
        .dismissal_fee_v1(u64::try_from(cost).map_err(|_| bad("native39 owner tariff monetary range"))?);
    if (actor.collateral as u128) < builder.committed_at(signer, ctx.daa_score).saturating_add(fee as u128) {
        return Err(bad("native39 independent owner original actual payment unavailable"));
    }
    let paid = object_id(
        b"misaka-palw/native/original-collected-owner-fee/v39",
        &(ctx.block.as_bytes(), *signer, job, proof, total, discovery, fee),
    );
    let id = object_id(b"misaka-palw/native/original-owner-id/v39", &(job, book.funding(), claim.claim, *signer, operator, paid));
    let e = epoch(p, ctx)?;
    let shard =
        super::super::palw_flat_execution_fold_v11::quota_shard(builder, &p.execution, &e, palw_kernel_bond_id_v1(signer), job)?;
    let mut g = shard.global_row();
    let mut a = shard.actor_row();
    let mut s = shard.subject_row().ok_or_else(|| bad("native39 original funded subject absent"))?.clone();
    if s.closed
        || s.retained_protected_seals >= 4
        || g.protected_seal_count >= PROTECTED_SEALS_MAX_V11
        || a.retained_protected_seals >= PROTECTED_SEALS_PER_ACTOR_V11
        || g.epoch.network_interval != e.network_interval
        || a.network_interval != e.network_interval
        || !a.active
    {
        return Err(bad("native39 original shared protected capacity exhausted"));
    }
    if g.epoch.block_hash != e.block_hash {
        g.block.consumed = QuotaQuantitiesV11::default();
    }
    let zero = QuotaQuantitiesV11::default();
    g.block = plus(&g.block, &zero, &guarantee, &p.execution.quota.block)?;
    g.network = plus(&g.network, &zero, &guarantee, &p.execution.quota.network)?;
    a.interval = plus(&a.interval, &zero, &guarantee, &p.execution.quota.actor)?;
    g.epoch = e;
    g.protected_seal_count += 1;
    a.retained_protected_seals += 1;
    s.retained_protected_seals += 1;
    if builder.slash_bond(*signer, fee as u128)? != fee {
        return Err(bad("native39 owner original payment changed"));
    }
    let o = Owner {
        version: 39,
        id,
        job,
        funding: book.funding(),
        claim: claim.claim,
        maximum: h.original.maximum_contract,
        plan: quota.paged().root(),
        actor: *signer,
        operator,
        paid,
        proof,
        discovery,
        total: total as u32,
        count: total.div_ceil(32768) as u16,
        posted: ctx.daa_score,
        matures: ctx.daa_score + temporal.maturity,
        until,
        interval: h.original.interval,
        attempts: vec![0; 2048],
        accepted: vec![0; 256],
        assemblies: 0,
        courts: 0,
        conversion: 0,
        queries: 0,
        pending: 0,
        retired: false,
        spent: false,
        committed: guarantee.clone(),
        consumed: zero,
        remaining: guarantee,
    };
    let slot = st.owners.iter().position(|id| *id == [0; 64]).ok_or_else(|| bad("native39 original paid owner slots exhausted"))?;
    st.owners[slot] = id;
    st.admitted += 1;
    st.latest = st.latest.max(until);
    put_rows(builder, &g, &a, &s)?;
    write_owner(builder, &o)?;
    write_status(builder, &st)?;
    Ok(())
}
fn plan(r: &PalwKernelRouteStateV1, h: &JobHeader) -> Result<PagedProofPlanRecordV39, PalwStateV2Error> {
    let raw = r
        .aux
        .get(&(QUOTA, b(&(39u8, h.id, 2u8))))
        .filter(|b| b.len() <= 8192)
        .ok_or_else(|| bad("native39 original paid proof plan absent"))?;
    let p: PagedProofPlanRecordV39 = borsh::from_slice(raw).map_err(|_| bad("native39 fixed paid proof plan grammar"))?;
    if p.version != 39
        || p.class != h.original.class
        || p.maximum != h.original.maximum
        || p.root() != h.original.paged_plan
        || p.pages == 0
        || p.pages > 2048
        || p.maximum_bytes > 64 << 20
    {
        return Err(bad("native39 original proof plan differs"));
    }
    Ok(p)
}
fn proof_part(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    raw: &[u8],
) -> Result<(), PalwStateV2Error> {
    let part = borrowed_native_proof_part_v39(raw).ok_or_else(|| bad("native39 proofPart original bounded prefix"))?;
    let mut h = header(builder.state.kernel_route.as_ref().unwrap(), part.job)?;
    let mut o = owner(builder.state.kernel_route.as_ref().unwrap(), part.seal)?;
    let paid = plan(builder.state.kernel_route.as_ref().unwrap(), &h)?;
    let st = status(builder.state.kernel_route.as_ref().unwrap(), &h)?;
    if !super::accusable_v39(builder, &h, ctx.daa_score)
        || o.actor != *signer
        || o.job != h.id
        || !st.owners.contains(&o.id)
        || o.discovery
        || o.retired
        || o.spent
        || o.proof == [0; 64]
        || ctx.daa_score < o.matures
        || ctx.daa_score >= o.until
        || o.plan != paid.root()
        || part.count != o.count
        || part.total != o.total
    {
        return Err(bad("native39 proofPart lacks original paid owner/maturity/maximum/claim"));
    }
    if part.mode == 0 {
        let i = part.index;
        if o.attempts[i as usize] >= 2 || o.has(i) {
            return Err(bad("native39 original proof fragment already spent"));
        }
        o.attempts[i as usize] += 1;
        debit(builder, ctx, p, &h, &mut o, &paid.part)?;
        let expected = (o.total as u64 - i as u64 * 32768).min(32768) as usize;
        if part.body.len() != expected {
            return Ok(());
        }
        builder.write_kernel_row(TABLE, k(38, &(o.id, i)), Some(part.body.to_vec()))?;
        o.accepted[i as usize / 8] |= 1 << (i % 8);
        write_owner(builder, &o)?;
        return Ok(());
    }
    if o.assemblies >= 2 || (0..o.count).any(|i| !o.has(i)) {
        return Err(bad("native39 original proof assembly requires all prepaid parts"));
    }
    o.assemblies += 1;
    debit(builder, ctx, p, &h, &mut o, &paid.assembly)?;
    let mut file = Vec::with_capacity(o.total as usize);
    for i in 0..o.count {
        let Some(v) = builder.state.kernel_route.as_ref().unwrap().aux.get(&(TABLE, k(38, &(o.id, i)))) else { return Ok(()) };
        let expected = (o.total as u64 - i as u64 * 32768).min(32768) as usize;
        if v.len() != expected {
            return Ok(());
        }
        file.extend_from_slice(v);
    }
    if object_id(b"misaka-palw/native/original-owned-proof-file/v39", &file) != o.proof || file.len() < 68 {
        return Ok(());
    }
    if o.courts >= 2 {
        return Ok(());
    }
    o.courts += 1;
    debit(builder, ctx, p, &h, &mut o, &paid.court)?;
    // salt64 and nested filing length are authenticated by the original sealed
    // complete-file hash. Decode follows the complete court predebit above.
    let n = u32::from_le_bytes(file[64..68].try_into().unwrap()) as usize;
    if n != file.len() - 68 {
        return Ok(());
    }
    let (mut book, _, class, max, _, _) = restore_job(builder, p, &h)?;
    let raw = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .aux
        .get(&(TABLE, k(28, &h.id)))
        .filter(|raw| raw.len() == h.post_bytes as usize)
        .ok_or_else(|| bad("native39 accepted original compact post absent"))?;
    let post = NativeCommitWireV39::from_bytes(raw, h.original.maximum_positions).map_err(bad)?;
    let reader = NativeMaterialReader { route: builder.state.kernel_route.as_ref().unwrap() };
    let inputs = ActualInputResolverV39::derive(class.source(), &max, &post.generated, &reader).map_err(bad)?;
    let context = misaka_palw_kernel::flat_point39::CheckedPointContextV39::check(
        class.source(),
        &max,
        book.claim().unwrap().claim,
        &post.claim,
        &inputs,
    )
    .map_err(bad)?;
    if misaka_palw_kernel::flat_point39::check_flat_filing_v39(&context, &file[68..]).is_err() {
        return Ok(());
    }
    let proof =
        object_id(b"misaka-palw/native/original-independent-court-verdict/v39", &(h.id, h.original.root(), o.id, o.paid, o.proof));
    // G14 lane closure G-14: the conviction collects what the producer's bond still holds of the reserve, never refused for less.
    let collectible = crate::palw_g14_lane_closure_v1::palw_native_producer_collectible_v1(
        builder.state.bonds.get(&h.original.producer).map(|b| b.collateral),
        h.original.reserve,
    );
    // RFC08 08-B: a held Final is convicted inside its liability horizon (the fee refunded, the reserve collected).
    let after_final = h.terminal == NativeJobTerminalV39::Final;
    let s = if after_final {
        let horizon = super::accusation_horizon_v39(builder, &h);
        book.convict_after_final(ctx.daa_score, horizon, proof, collectible, o.operator).map_err(bad)?
    } else {
        book.convict(ctx.daa_score, proof, collectible, o.operator).map_err(bad)?
    };
    o.spent = true;
    write_owner(builder, &o)?;
    if after_final {
        return super::credit_after_final_conviction(builder, ctx, &mut h, &book, &s, *signer);
    }
    super::credit(builder, ctx, p, &mut h, &book, &s, Some(*signer))
}
fn convert(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    raw: &[u8],
) -> Result<(), PalwStateV2Error> {
    if raw.len() != 202 || raw[..2] != 39u16.to_le_bytes() {
        return Err(bad("native39 discovery conversion exact framing"));
    }
    let job = raw[2..66].try_into().unwrap();
    let id = raw[66..130].try_into().unwrap();
    let proof = raw[130..194].try_into().unwrap();
    let total = u64::from_le_bytes(raw[194..202].try_into().unwrap());
    let h = header(builder.state.kernel_route.as_ref().unwrap(), job)?;
    let mut o = owner(builder.state.kernel_route.as_ref().unwrap(), id)?;
    let paid = plan(builder.state.kernel_route.as_ref().unwrap(), &h)?;
    let st = status(builder.state.kernel_route.as_ref().unwrap(), &h)?;
    if !super::accusable_v39(builder, &h, ctx.daa_score)
        || !st.owners.contains(&id)
        || o.job != job
        || o.actor != *signer
        || !o.discovery
        || o.retired
        || o.spent
        || o.conversion != 0
        || ctx.daa_score < o.matures
        || proof == [0; 64]
        || total == 0
        || total > o.total as u64
        || total > paid.maximum_bytes
    {
        return Err(bad("native39 discovery conversion changes original stream/actor/maximum"));
    }
    // Reuse the original full transport window; fresh proof maturity must fit.
    let raw = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .aux
        .get(&(QUOTA, b(&(39u8, job, 0u8))))
        .filter(|raw| raw.len() < 16384)
        .ok_or_else(|| bad("native39 original complete temporal plan absent"))?;
    let max: NativeMaximumContractV39 = borsh::from_slice(raw).map_err(|_| bad("native39 original temporal grammar"))?;
    if max.root() != h.original.maximum_contract
        || ctx.daa_score.checked_add(max.temporal.proof_window).is_none_or(|until| until > o.until)
    {
        return Err(bad("native39 discovery conversion cannot reset original full opportunity"));
    }
    o.conversion = 1;
    debit(builder, ctx, p, &h, &mut o, &paid.conversion)?;
    o.discovery = false;
    o.proof = proof;
    o.total = total as u32;
    o.count = total.div_ceil(32768) as u16;
    o.matures = ctx.daa_score.checked_add(max.temporal.maturity).ok_or_else(|| bad("native39 proof maturity overflow"))?;
    write_owner(builder, &o)?;
    Ok(())
}
fn expire(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    raw: &[u8],
) -> Result<(), PalwStateV2Error> {
    if raw.len() != 130 || raw[..2] != 39u16.to_le_bytes() {
        return Err(bad("native39 paid owner expiry exact framing"));
    }
    let job = raw[2..66].try_into().unwrap();
    let id = raw[66..130].try_into().unwrap();
    let h = header(builder.state.kernel_route.as_ref().unwrap(), job)?;
    let mut o = owner(builder.state.kernel_route.as_ref().unwrap(), id)?;
    let mut st = status(builder.state.kernel_route.as_ref().unwrap(), &h)?;
    if o.job != job
        || o.funding != h.original.root()
        || !st.owners.contains(&id)
        || o.retired
        || o.pending != 0
        || ctx.daa_score <= o.until
    {
        return Err(bad("native39 original paid stream is not expired/settled"));
    }
    if !prepay(builder, ctx, signer, p, o.total as u64 + 32768, raw.len(), 0)? {
        return Ok(());
    }
    let e = epoch(p, ctx)?;
    let _r = builder.state.kernel_route.as_ref().unwrap();
    let shard =
        super::super::palw_flat_execution_fold_v11::quota_shard(builder, &p.execution, &e, palw_kernel_bond_id_v1(&o.actor), job)?;
    let mut g = shard.global_row();
    let mut a = shard.actor_row();
    let mut s = shard.subject_row().ok_or_else(|| bad("native39 expiry original subject absent"))?.clone();
    if g.protected_seal_count == 0 || a.retained_protected_seals == 0 || s.retained_protected_seals == 0 {
        return Err(bad("native39 original retained counters absent"));
    }
    if o.interval == e.network_interval {
        if g.epoch.block_hash != e.block_hash {
            g.block.consumed = QuotaQuantitiesV11::default();
        }
        g.block = consume(&g.block, &o.remaining, &p.execution.quota.block)?;
        g.network = consume(&g.network, &o.remaining, &p.execution.quota.network)?;
        a.interval = consume(&a.interval, &o.remaining, &p.execution.quota.actor)?;
        g.epoch = e;
    } else if o.interval > e.network_interval {
        return Err(bad("native39 expiry rewinds original interval"));
    }
    g.protected_seal_count -= 1;
    a.retained_protected_seals -= 1;
    s.retained_protected_seals -= 1;
    o.retired = true;
    o.consumed = o.committed.clone();
    o.remaining = QuotaQuantitiesV11::default();
    for i in 0..o.count {
        builder.write_kernel_row(TABLE, k(38, &(id, i)), None)?;
    }
    let slot = st.owners.iter().position(|v| *v == id).unwrap();
    st.owners[slot] = [0; 64];
    // G14 lane closure G-N1: an owner that ran out its window without a conviction frees its prosecution slot — `admitted` counts the
    // live owners, so four idle owners can no longer hold an honest outside verifier off the claim until Final. Each new owner pays its
    // own tariff again; the expired owner's archive row (kind 39) stays.
    st.admitted = st.admitted.checked_sub(1).ok_or_else(|| bad("native39 expiry original owner count absent"))?;
    put_rows(builder, &g, &a, &s)?;
    write_status(builder, &st)?;
    builder.write_kernel_row(TABLE, k(39, &id), Some(b(&(o.root(), o.paid, o.proof, o.job, o.claim, o.funding, o.until))))?;
    builder.write_kernel_row(TABLE, k(37, &id), None)?;
    Ok(())
}
pub(super) fn query_debit(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    p: &NativeExecutionPoliciesV39,
    h: &JobHeader,
    id: Digest,
    raw_request: &[u8],
) -> Result<Owner, PalwStateV2Error> {
    let mut o = owner(builder.state.kernel_route.as_ref().unwrap(), id)?;
    let st = status(builder.state.kernel_route.as_ref().unwrap(), h)?;
    if o.job != h.id
        || o.claim != st.claim
        || !st.owners.contains(&id)
        || o.retired
        || o.spent
        || ctx.daa_score >= o.until
        || o.pending >= 2
    {
        return Err(bad("native39 original paid discovery request unavailable"));
    }
    let plan = CheckedJobQuotaPlanV11::restore(&h.plan, &h.plan_root).map_err(bad)?;
    let raw = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .aux
        .get(&(QUOTA, b(&(39u8, h.id, 1u8))))
        .ok_or_else(|| bad("native39 original public source plan absent"))?;
    let source: SourcePlanRecordV39 = borsh::from_slice(raw).map_err(|_| bad("native39 source plan grammar"))?;
    if source.root() != h.original.source_plan || o.queries >= source.requests_per_owner {
        return Err(bad("native39 original discovery request count exhausted"));
    }
    let action = plan.action(QuotaActionKindV11::DARequest, raw_request).map_err(bad)?;
    o.queries += 1;
    debit(builder, ctx, p, h, &mut o, action.quantities())?;
    Ok(o)
}
pub(super) fn set_owner(builder: &mut TransitionBuilder<'_>, o: &Owner) -> Result<(), PalwStateV2Error> {
    write_owner(builder, o)
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
        46 => admit(builder, ctx, signer, p, raw),
        50 => proof_part(builder, ctx, signer, p, raw),
        52 => expire(builder, ctx, signer, p, raw),
        56 => convert(builder, ctx, signer, p, raw),
        _ => Err(bad("native39 protected action")),
    }
}
