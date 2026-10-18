//! Owning39 termination and producer transport (stage actions 53/54/55).
//!
//! * 53 collects a genuinely settled job once its whole original retention has
//!   elapsed: it re-derives the original quota terminal, burns the unused
//!   source39 obligation, retires the original queue30/physical22 leases exactly
//!   once, detaches input material, releases the job's class retention
//!   (HOLD_JOB) and erases bulky rows. The original header (16), book (17),
//!   single credit (23), accepted post (28) and the Master binding (84) stay, so
//!   a later Master, auditor or receipt reader re-uses the original Final and
//!   generated tokens. Collection never returns quota/window allowance, never
//!   re-opens a job and never creates a payment.
//! * 54/55 carry the producer's original compact Commit wire in 32-KiB pages.
//!   The first page of each of at most two attempts pays the SAME original
//!   reserved Metadata envelope a direct Commit45 pays; every page also pays its
//!   own ordinary copy/storage tariff before the bytes are stored. Assembly
//!   decodes only after the complete group hash matches, through the exact same
//!   `commit_after_paid` path as action 45. A malformed group keeps its debit.
use super::*;

pub(super) const COMMIT_JOURNAL: u8 = 29;
pub(super) const COMMIT_PAGE: u8 = 30;
pub(super) const COLLECTED: u8 = 31;
/// Written by `public::request`: every public pending id ever opened for a job.
pub(super) const OPENED: u8 = 33;
pub(super) const OPENED_CAP: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(super) struct CommitJournalV39 {
    pub(super) version: u16,
    pub(super) job: Digest,
    pub(super) funding: Digest,
    pub(super) producer: PalwBondKeyV2,
    pub(super) until: u64,
    pub(super) bound: u32,
    pub(super) attempt: u8,
    pub(super) group: Digest,
    pub(super) count: u16,
    pub(super) total: u32,
    pub(super) next: u16,
    pub(super) burned: bool,
    pub(super) assembled: bool,
}
impl CommitJournalV39 {
    pub(super) fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-producer-commit-transport/v39", self)
    }
}
fn fresh_journal(h: &JobHeader) -> Result<CommitJournalV39, PalwStateV2Error> {
    Ok(CommitJournalV39 {
        version: 39,
        job: h.id,
        funding: h.original.root(),
        producer: h.original.producer,
        until: h.original.commit_until,
        bound: native_commit_bytes_bound_v39(h.original.maximum_positions).map_err(bad)?,
        attempt: 0,
        group: [0; 64],
        count: 0,
        total: 0,
        next: 0,
        burned: false,
        assembled: false,
    })
}
pub(super) fn commit_journal(r: &PalwKernelRouteStateV1, h: &JobHeader) -> Result<CommitJournalV39, PalwStateV2Error> {
    let Some(raw) = r.aux.get(&(TABLE, k(COMMIT_JOURNAL, &h.id))) else { return fresh_journal(h) };
    if raw.len() > 1024 {
        return Err(bad("native39 original commit transport journal extent"));
    }
    let (root, j): (Digest, CommitJournalV39) = borsh::from_slice(raw).map_err(|_| bad("native39 commit journal grammar"))?;
    let expected = fresh_journal(h)?;
    if root != j.root()
        || j.version != 39
        || j.job != h.id
        || j.funding != expected.funding
        || j.producer != expected.producer
        || j.until != expected.until
        || j.bound != expected.bound
        || j.attempt == 0
        || j.attempt > 2
        || j.group == [0; 64]
        || j.count == 0
        || j.count > NATIVE_COMMIT_PARTS_MAX_V39
        || j.total == 0
        || j.total > j.bound
        || j.total.div_ceil(NATIVE_COMMIT_PAGE_BYTES_V39 as u32) != j.count as u32
        || j.next > j.count
    {
        return Err(bad("native39 original commit transport conservation differs"));
    }
    Ok(j)
}
fn write_journal(builder: &mut TransitionBuilder<'_>, j: &CommitJournalV39) -> Result<(), PalwStateV2Error> {
    builder.write_kernel_row(TABLE, k(COMMIT_JOURNAL, &j.job), Some(b(&(j.root(), j))))?;
    Ok(())
}
fn erase_pages(builder: &mut TransitionBuilder<'_>, job: Digest, attempt: u8, count: u16) -> Result<(), PalwStateV2Error> {
    for i in 0..count {
        builder.write_kernel_row(TABLE, k(COMMIT_PAGE, &(job, attempt, i)), None)?;
    }
    Ok(())
}
impl PalwKernelRouteStateV1 {
    /// Fixed native rows only: ownership/priority evidence for one page, never
    /// an execution, court or Final authorization. A new group may start only
    /// at index 0, after a burned or absent attempt, and at most twice.
    pub fn native_commit_owner_v39(
        &self,
        job: Digest,
        group: Digest,
        index: u16,
        count: u16,
        total: u32,
        now: u64,
    ) -> Option<PalwBondKeyV2> {
        let h = header(self, job).ok()?;
        let j = commit_journal(self, &h).ok()?;
        if h.terminal != NativeJobTerminalV39::Funded
            || j.assembled
            || now < h.original.admitted
            || now >= j.until
            || group == [0; 64]
            || count == 0
            || index >= count
            || total == 0
            || total > j.bound
            || total.div_ceil(NATIVE_COMMIT_PAGE_BYTES_V39 as u32) != count as u32
        {
            return None;
        }
        let continuation = j.attempt != 0 && !j.burned && j.group == group && j.count == count && j.total == total && index == j.next;
        let restart = index == 0 && j.attempt < 2 && (j.attempt == 0 || j.burned);
        (continuation || restart).then_some(h.original.producer)
    }
    pub fn native_commit_assemble_owner_v39(&self, job: Digest, group: Digest, now: u64) -> Option<PalwBondKeyV2> {
        let h = header(self, job).ok()?;
        let j = commit_journal(self, &h).ok()?;
        if h.terminal != NativeJobTerminalV39::Funded
            || j.assembled
            || j.burned
            || j.attempt == 0
            || j.group != group
            || j.next != j.count
            || now >= j.until
        {
            return None;
        }
        Some(h.original.producer)
    }
    /// Scheduling evidence for a compact-post page or assembly: the original
    /// paged carriage/queue/physical leases of the funded job, and the proof
    /// envelope that already bounds a 32-KiB page packet.
    pub fn native_paid_commit_eligibility_v39(
        &self,
        job: Digest,
        group: Digest,
        index: Option<(u16, u16, u32)>,
        now: u64,
        mass: &crate::palw_native_carriage_v22::NativeCarriageMassParamsV22,
        service: &NativeCarriageServiceV22,
    ) -> Option<NativePaidCarrierEligibilityV22> {
        let owner = match index {
            Some((index, count, total)) => self.native_commit_owner_v39(job, group, index, count, total, now)?,
            None => self.native_commit_assemble_owner_v39(job, group, now)?,
        };
        let h = header(self, job).ok()?;
        let (c, lease) = original_carriage(self, &h, mass, service)?;
        mass.packet_mass(&c.proof_envelope).ok().filter(|m| *m <= service.slot_mass)?;
        Some(NativePaidCarrierEligibilityV22 {
            original_job: job,
            unit: group,
            owner,
            deadline: h.original.commit_until.saturating_sub(1),
            kind: if index.is_some() { 10 } else { 11 },
            lease_root: lease,
            original_service: service.root(),
            envelope: c.proof_envelope,
        })
    }
}
pub(super) fn commit_part(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    let part = borrowed_native_commit_metadata_v39(body).ok_or_else(|| bad("native39 commit page fixed metadata"))?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let owner = r
        .native_commit_owner_v39(part.job, part.group, part.index, part.count, part.total, ctx.daa_score)
        .ok_or_else(|| bad("native39 commit page lacks own original Funded producer phase/attempt"))?;
    if owner != *signer {
        return Err(bad("native39 commit page signer is not the original producer"));
    }
    let h = header(r, part.job)?;
    let mut j = commit_journal(r, &h)?;
    let restart = part.index == 0 && (j.attempt == 0 || j.burned);
    if restart {
        // One full original reserved Metadata envelope per attempt, exactly
        // the debit a direct Commit45 pays; never more than two attempts.
        predebit_original(builder, ctx, p, &h, QuotaActionKindV11::Metadata, body)?;
        erase_pages(builder, part.job, j.attempt, j.count)?;
        j.attempt += 1;
        j.group = part.group;
        j.count = part.count;
        j.total = part.total;
        j.next = 0;
        j.burned = false;
    }
    // Every page pays its own copy/storage before any byte is stored.
    if !prepay(builder, ctx, signer, p, 0, body.len(), part.body.len() as u64)? {
        if restart {
            write_journal(builder, &j)?;
        }
        return Ok(());
    }
    if borrowed_native_commit_part_v39(body).is_none() {
        // Bad nested extent burns this attempt; its paid debit is retained.
        j.burned = true;
        write_journal(builder, &j)?;
        return Ok(());
    }
    builder.write_kernel_row(TABLE, k(COMMIT_PAGE, &(part.job, j.attempt, part.index)), Some(part.body.to_vec()))?;
    j.next += 1;
    write_journal(builder, &j)?;
    Ok(())
}
pub(super) fn commit_assemble(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    let (job, group) = borrowed_native_commit_assembly_v39(body).ok_or_else(|| bad("native39 commit assembly exact frame"))?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    if r.native_commit_assemble_owner_v39(job, group, ctx.daa_score) != Some(*signer) {
        return Err(bad("native39 commit assembly lacks the original complete paid producer group"));
    }
    let h = header(r, job)?;
    let mut j = commit_journal(r, &h)?;
    if !prepay(builder, ctx, signer, p, j.total as u64, body.len(), 0)? {
        return Ok(());
    }
    // One-time assembly is consumed BEFORE allocation or nested decoding.
    j.burned = true;
    write_journal(builder, &j)?;
    let mut all = Vec::new();
    all.try_reserve_exact(j.total as usize).map_err(|_| bad("native39 commit assembly bounded allocation"))?;
    for index in 0..j.count {
        let raw = builder
            .state
            .kernel_route
            .as_ref()
            .unwrap()
            .aux
            .get(&(TABLE, k(COMMIT_PAGE, &(job, j.attempt, index))))
            .filter(|v| v.len() <= NATIVE_COMMIT_PAGE_BYTES_V39)
            .ok_or_else(|| bad("native39 original paid commit page absent"))?;
        all.extend_from_slice(raw);
    }
    erase_pages(builder, job, j.attempt, j.count)?;
    if all.len() != j.total as usize || native_commit_group_v39(job, &all) != j.group || all.get(2..66) != Some(job.as_slice()) {
        return Ok(());
    }
    let before = h.terminal;
    commit_after_paid(builder, ctx, signer, p, &all, h)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    if header(r, job).is_ok_and(|now| now.terminal != before) {
        j.assembled = true;
        write_journal(builder, &j)?;
    }
    Ok(())
}

/// Every public pending id opened for a job (bounded). `public::request` notes
/// each new id before its row exists, so collection can erase all of them.
pub(super) fn note_opened(builder: &mut TransitionBuilder<'_>, job: Digest, id: Digest) -> Result<(), PalwStateV2Error> {
    let mut ids = opened(builder.state.kernel_route.as_ref().unwrap(), job)?;
    if ids.len() >= OPENED_CAP || ids.contains(&id) {
        return Err(bad("native39 original public pending index full or replayed"));
    }
    ids.push(id);
    builder.write_kernel_row(TABLE, k(OPENED, &job), Some(b(&ids)))?;
    Ok(())
}
pub(super) fn opened(r: &PalwKernelRouteStateV1, job: Digest) -> Result<Vec<Digest>, PalwStateV2Error> {
    let Some(raw) = r.aux.get(&(TABLE, k(OPENED, &job))) else { return Ok(vec![]) };
    if raw.len() > 4 + OPENED_CAP * 64 {
        return Err(bad("native39 opened pending index extent"));
    }
    let ids: Vec<Digest> = borsh::from_slice(raw).map_err(|_| bad("native39 opened pending index grammar"))?;
    if ids.len() > OPENED_CAP || ids.iter().enumerate().any(|(i, x)| *x == [0; 64] || ids[..i].contains(x)) {
        return Err(bad("native39 opened pending index membership"));
    }
    Ok(ids)
}

/// Collection53: `39u16 ‖ job` (66 bytes). Any Active bond may collect, paying
/// the restoration it causes; the outcome is a pure function of original rows.
pub(super) fn collect(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    raw: &[u8],
) -> Result<(), PalwStateV2Error> {
    if raw.len() != 66 || raw[..2] != 39u16.to_le_bytes() {
        return Err(bad("native39 collection exact original job grammar"));
    }
    let id: Digest = raw[2..66].try_into().unwrap();
    let r = builder.state.kernel_route.as_ref().unwrap();
    if r.aux.contains_key(&(TABLE, k(COLLECTED, &id))) {
        return Err(bad("native39 original job already collected"));
    }
    if super::palw_native_liability_hold_fold_v1::held_native_fee_v1(
        &builder.state,
        crate::palw_native_liability_hold_v1::PALW_NATIVE_FAMILY_JOB_V39,
        &id,
    ).is_some() {
        return Err(bad("native39 collection cannot erase a job with an unpaid held fee"));
    }
    let h = header(r, id)?;
    let terminal_book = book(r, &h)?;
    let (croot, credit): (Digest, NativeSingleCreditV39) = read(r, 23, &id, 1024)?;
    let s = terminal_book.settlement().cloned().ok_or_else(|| bad("native39 collection requires the original settlement"))?;
    let horizon = s
        .established
        .checked_add(p.execution.execution.horizons.retention)
        .ok_or_else(|| bad("native39 collection retention overflow"))?
        .max(h.original.deadline);
    if !matches!(h.terminal, NativeJobTerminalV39::Final | NativeJobTerminalV39::Refunded | NativeJobTerminalV39::Convicted)
        || credit.root() != croot
        || credit.job != id
        || credit.funding != terminal_book.funding()
        || credit.terminal != terminal_book.root()
        || credit.settlement != s.root()
        || ctx.daa_score <= horizon
    {
        return Err(bad("native39 collection requires a genuine settled single credit past the complete original retention"));
    }
    // G14 lane closure G-N2: a component a live mining claim credits keeps its material through that claim's liability.
    if super::mining_court::job_held(r, &id, ctx.daa_score) {
        return Err(bad("native39 collection: a mining claim crediting this job is still accusable (material held)"));
    }
    let st = protected::status(r, &h)?;
    if !st.pending.is_empty() || st.owners.iter().any(|o| *o != [0; 64]) {
        return Err(bad("native39 collection: original paid streams or open public units remain"));
    }
    let opened_ids = opened(r, id)?;
    let restore = (h.source_bytes as u64)
        .checked_add(h.consent_bytes as u64)
        .and_then(|n| n.checked_add(h.post_bytes as u64))
        .and_then(|n| n.checked_add(opened_ids.len() as u64 * 4096))
        .ok_or_else(|| bad("native39 collection restore extent overflow"))?;
    if !prepay(builder, ctx, signer, p, restore, raw.len(), 16384)? {
        return Ok(());
    }
    // Original consent, class, maximum and quota re-derived as at Fund.
    let (old, c, _class, max, q, _contract) = restore_job_for_terminal(builder, p, &h, &terminal_book)?;
    if old.root() != terminal_book.root() {
        return Err(bad("native39 collection terminal book was replaced"));
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    let consent_raw = r
        .aux
        .get(&(TABLE, k(27, &id)))
        .filter(|v| v.len() <= NativeConsentWireV39::MAX_BYTES + 16384)
        .ok_or_else(|| bad("native39 collection original consent row absent"))?;
    let consent_bytes = consent_raw.len() as u32;
    let consent_body = native_consent_body_digest_v39(consent_raw);
    let counter = r.execution_nonce_counter_v11(&h.original.requester_operator).map_err(bad)?;
    let e = epoch(p, ctx)?;
    let anchor = NativeTerminalAnchorV39 {
        job: id,
        funding: terminal_book.funding(),
        terminal_book: terminal_book.root(),
        established: s.established,
        producer_credit: s.producer_credit,
        requester_credit: s.requester_credit,
        actual_slash: s.actual_slash,
        finder_credit: s.finder_credit,
        escrow_closed: terminal_book.escrow() == 0,
        proof: s.proof,
    };
    let terminal = check_terminal_v39(
        &q,
        &h.original.funding_anchor(),
        &p.execution.quota,
        &nonce(palw_kernel_bond_id_v1(&h.original.requester), &counter),
        &anchor,
        &e,
    )
    .map_err(bad)?;
    // Unused source39 obligation is burned once; consumed interval room is
    // never returned.
    let shard = super::super::palw_flat_execution_fold_v11::quota_shard(
        builder,
        &p.execution,
        &e,
        palw_kernel_bond_id_v1(&h.original.producer),
        id,
    )?;
    let g = shard.global_row();
    let a = shard.actor_row();
    let subject = shard.subject_row().ok_or_else(|| bad("native39 collection original subject absent"))?.clone();
    let source_raw = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .aux
        .get(&(QUOTA, b(&(39u8, 0u8, id))))
        .filter(|raw| raw.len() < 4096)
        .ok_or_else(|| bad("native39 collection original source obligation absent"))?;
    let (source_root, source_row): (Digest, SourceObligationV39) =
        borsh::from_slice(source_raw).map_err(|_| bad("native39 collection source row grammar"))?;
    let mut source = SourceShardV39::from_native_rows(
        &p.execution.quota,
        &q,
        &g,
        &g.root(),
        (&a, &a.root()),
        (&subject, &subject.row_root()),
        Some((&source_row, &source_root)),
    )
    .map_err(bad)?;
    let source_retired = if source_row.retired {
        false
    } else {
        source.burn_closed(&p.execution.quota, &e, &terminal).map_err(bad)?;
        true
    };
    // Joint original queue30 + physical22 retirement, exactly once.
    let r = builder.state.kernel_route.as_ref().unwrap();
    let (qlease_root, mut qlease): (Digest, NativeQueueLeaseV30) =
        r.aux_row(QUOTA, &b(&(30u8, id, 1u8))).ok_or_else(|| bad("native39 collection original queue lease absent"))?;
    let (plroot, mut plin): (Digest, NativeCarriageLeaseV22) =
        r.aux_row(QUOTA, &b(&(22u8, id))).ok_or_else(|| bad("native39 collection original physical lease absent"))?;
    let (pgroot, mut physical): (Digest, NativeCarriageGlobalV22) =
        r.aux_row(QUOTA, &b(&(22u8, 0u8))).ok_or_else(|| bad("native39 collection physical global absent"))?;
    let qraw = r.aux.get(&(QUOTA, b(&(30u8, 0u8)))).ok_or_else(|| bad("native39 collection queue global absent"))?;
    if qraw.len() > NATIVE_QUEUE_ROW_LIMIT_V30 {
        return Err(bad("native39 collection queue global cap"));
    }
    let (qgroot, mut global): (Digest, NativeQueueGlobalV30) =
        borsh::from_slice(qraw).map_err(|_| bad("native39 collection queue global grammar"))?;
    if qlease.root() != qlease_root
        || qlease.job != id
        || qlease.original_funding != h.original.root()
        || qlease.original_consent != h.original.consent
        || qlease.original_plan != h.original.queue
        || plin.root() != plroot
        || plin.original_job != id
        || physical.root() != pgroot
        || global.root() != qgroot
    {
        return Err(bad("native39 collection original queue/physical terms differ"));
    }
    let closure = CheckedNativeQueueClosureV30::from_current_native(
        id,
        h.original.root(),
        qlease_root,
        plroot,
        terminal_book.root(),
        ctx.daa_score,
        false,
    )
    .map_err(bad)?;
    let qp = &builder.extras.kernel_route.as_ref().unwrap().native_final.as_ref().unwrap().policy.queue_policy;
    global.retire(&mut physical, qp, &mut qlease, &mut plin, &closure).map_err(bad)?;
    // The v11 subject leaves the shared counters exactly once.
    let mut shard = super::super::palw_flat_execution_fold_v11::quota_shard(
        builder,
        &p.execution,
        &e,
        palw_kernel_bond_id_v1(&h.original.producer),
        id,
    )?;
    if source_retired {
        // Re-read the producer counters the burn just updated before the
        // subject collection consumes the same rows.
        let g2 = source.global_row().clone();
        let a2 = source.producer_row().clone();
        shard = misaka_palw_kernel::flat_quota11::CheckedQuotaShardV11::from_native_rows(
            &p.execution.quota,
            &g2,
            &g2.root(),
            a2.actor,
            id,
            None,
            Some((&a2, &a2.root())),
            Some((&subject, &subject.row_root())),
            None,
        )
        .map_err(bad)?;
    }
    shard.collect_closed_expired(&p.execution.quota, &e, &terminal).map_err(bad)?;
    let collected = NativeCollectedJobV39 {
        version: 39,
        job: id,
        funding: h.original.root(),
        terminal_book: terminal_book.root(),
        credit: croot,
        consent: c.root(),
        consent_bytes,
        consent_body,
        class: h.original.class,
        queue_lease: qlease_root,
        physical_lease: plroot,
        source_retired,
        collected: ctx.daa_score,
    };
    // ---- every fallible check is above; writes only below ----
    material::detach(builder, max.input_manifest(), id)?;
    super::super::palw_native_source_fold_v39::release_class(
        builder,
        h.original.class,
        super::super::palw_native_source_fold_v39::HOLD_JOB,
        id,
    )?;
    super::super::palw_flat_execution_fold_v11::write_shard(builder, &shard, id)?;
    builder.write_kernel_row(QUOTA, b(&(30u8, 0u8)), Some(b(&(global.root(), global))))?;
    builder.write_kernel_row(QUOTA, b(&(22u8, 0u8)), Some(b(&(physical.root(), physical))))?;
    builder.write_kernel_row(QUOTA, b(&(2u8, id)), None)?;
    builder.write_kernel_row(QUOTA, b(&(39u8, 0u8, id)), None)?;
    builder.write_kernel_row(QUOTA, b(&(30u8, id, 1u8)), None)?;
    builder.write_kernel_row(QUOTA, b(&(22u8, id)), None)?;
    for pid in &opened_ids {
        let r = builder.state.kernel_route.as_ref().unwrap();
        if let Ok(v) = public::pending(r, *pid) {
            if v.job != id || !v.settled {
                return Err(bad("native39 collection opened public unit belongs elsewhere or is live"));
            }
            for attempt in 1..=v.attempts {
                for i in 0..4u16 {
                    builder.write_kernel_row(TABLE, k(43, &(v.id, attempt, i)), None)?;
                }
            }
            builder.write_kernel_row(TABLE, k(41, &v.unit), None)?;
            builder.write_kernel_row(TABLE, k(42, &v.unit), None)?;
        }
        builder.write_kernel_row(TABLE, k(40, pid), None)?;
    }
    let r = builder.state.kernel_route.as_ref().unwrap();
    if let Ok(j) = commit_journal(r, &h) {
        if j.attempt != 0 {
            erase_pages(builder, id, j.attempt, j.count)?;
        }
    }
    for kind in [COMMIT_JOURNAL, OPENED, 36] {
        builder.write_kernel_row(TABLE, k(kind, &id), None)?;
    }
    builder.write_kernel_row(TABLE, k(27, &id), None)?;
    builder.write_kernel_row(TABLE, k(COLLECTED, &id), Some(b(&(collected.root(), &collected))))?;
    builder.note_kernel_bond(&h.original.requester)?;
    builder.note_kernel_bond(&h.original.producer)?;
    Ok(())
}
/// Readonly: the immutable collection record, if this job was collected.
pub(super) fn collected(r: &PalwKernelRouteStateV1, job: Digest) -> Result<Option<NativeCollectedJobV39>, PalwStateV2Error> {
    let Some(raw) = r.aux.get(&(TABLE, k(COLLECTED, &job))) else { return Ok(None) };
    if raw.len() > 2048 {
        return Err(bad("native39 collected record extent"));
    }
    let (root, c): (Digest, NativeCollectedJobV39) = borsh::from_slice(raw).map_err(|_| bad("native39 collected record grammar"))?;
    if root != c.root() || c.version != 39 || c.job != job {
        return Err(bad("native39 collected record identity differs"));
    }
    Ok(Some(c))
}
