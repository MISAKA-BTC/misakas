//! **G14 lane closure G-N2: the post-Final public accusation of an owning39 mining claim** (stage action 80; a child module of the
//! native39 job fold, dormant behind `palw_native_mining_v39` with every prerequisite of the mining fold).
//!
//! A mining claim (action 75) credits the stages its producer computed in one complete-Master escrow Final. The component jobs'
//! own courts close at their Final, so before this lane nothing could convict a mining claim after it: `accusation_pending` was
//! fixed `false` and a claim certified by colluding attestors earned its receipt untouchable (audit G-N2). Here:
//!
//! ```text
//! link       mining claim → (Master, stage) → component job (header, its producer bond == the claim's producer) → committed post,
//!            consent, class and material — every row authenticated through the job fold's own restore (`restore_job`)
//! retention  commit 75 writes a hold per credited job; collection 53 is refused while the claim is accusable (its liability
//!            horizon, `palw_native_mining_liability_until_v39`), so the material outlives the job's quota interval
//! kind 0     a self-contained filing (the job court's own `check_flat_filing_v39` against the committed post) → conviction
//! kind 1     a post-Final demand on the component (the public lane's own `check_flat_demand_v39`); the accuser's bond is debited,
//!            the producer must answer by `now + da_answer`; ≤ 8 per claim, ≤ 2 open; while one is open Gate C waits
//!            (`accusation_pending`) and no receipt quantum is spent
//! kind 2     the producer's answer in ≤ 64 parts of 32 KiB, checked by `check_flat_reply_v39`; the reply is public material,
//!            the accuser's bond pays the producer
//! kind 3     the default of an unanswered demand → conviction (the accuser is refunded its bond)
//! verdict    the producer's reserved exposure is slashed (what the bond still holds, G-14), the finder paid ≤ ADR-0032's 490‰,
//!            the rest burned; the claim is closed without reward, its receipt revoked and every quantum's safe weight taken back,
//!            its certification row withdrawn (no Gate C credit), every Reproduced attestor's lock charged
//! ```
//!
//! **Veto.** RFC-0018 U-6 is not adopted: the certification policy's `veto` stays the design value 0
//! (`PALW_CERT_VETO_DESIGN_V1`), parameterised and not used here. A conviction is objective (a verified filing or a default), never a
//! count of negative attestations. No cryptographic computation proof is used (PRINCIPLES §8).
use super::*;
use crate::palw_native_mining_v39::*;
use crate::palw_native_receipt_v39::{native_mining_budget_id_v39, native_mining_convicted_v39};
use misaka_palw_kernel::flat_public39::{FlatDemandV39, FlatReplyV39, check_flat_demand_v39, check_flat_reply_v39};

const MINING: u8 = MINING_TABLE_V39;
fn mk<T: BorshSerialize>(kind: u8, id: &T) -> Vec<u8> {
    crate::palw_native_receipt_v39::native_mining_row_key_v39(kind, id)
}
fn mread<T: BorshDeserialize>(r: &PalwKernelRouteStateV1, kind: u8, id: &impl BorshSerialize, cap: usize) -> Option<T> {
    let raw = r.aux.get(&(MINING, mk(kind, id))).filter(|raw| raw.len() <= cap)?;
    borsh::from_slice(raw).ok()
}

/// The component job of `stage` in the claim's Master Final, produced by the claim's own producer (the authenticated link).
fn component(r: &PalwKernelRouteStateV1, c: &NativeMiningClaimRecordV39, stage: u16) -> Result<JobHeader, PalwStateV2Error> {
    if !c.stages.contains(&stage) {
        return Err(bad("mining39 accusation names a stage the claim does not credit"));
    }
    let fin = super::super::palw_native_mining_fold_v39::master_final(r, &c.master)
        .filter(|f| f.root() == c.master_final)
        .ok_or_else(|| bad("mining39 accusation: the claim's Master Final is gone"))?;
    let job = fin
        .components
        .iter()
        .find(|x| x.original.stage == stage)
        .map(|x| x.original.job)
        .ok_or_else(|| bad("mining39 accusation stage absent from the Master Final"))?;
    let h = header(r, job)?;
    if h.original.producer != c.producer || h.terminal != NativeJobTerminalV39::Final {
        return Err(bad("mining39 accusation: the component is not the claim producer's Final job"));
    }
    Ok(h)
}

/// The component's committed point context (consent, class, maximum, post, inputs from retained material) — the same restoration
/// the job's own court and public lane run.
fn with_context<R>(
    builder: &TransitionBuilder<'_>,
    p: &NativeExecutionPoliciesV39,
    h: &JobHeader,
    f: impl FnOnce(&misaka_palw_kernel::flat_point39::CheckedPointContextV39) -> R,
) -> Result<R, PalwStateV2Error> {
    let (book, _, class, max, _, _) = restore_job(builder, p, h)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let raw = r
        .aux
        .get(&(TABLE, k(28, &h.id)))
        .filter(|raw| raw.len() == h.post_bytes as usize)
        .ok_or_else(|| bad("mining39 accusation: the component's committed post is not retained"))?;
    let post = NativeCommitWireV39::from_bytes(raw, h.original.maximum_positions).map_err(bad)?;
    let reader = material::NativeMaterialReader { route: r };
    let inputs = ActualInputResolverV39::derive(class.source(), &max, &post.generated, &reader).map_err(bad)?;
    let claim = book.claim().ok_or_else(|| bad("mining39 accusation: the component has no accepted claim"))?.claim;
    let context = misaka_palw_kernel::flat_point39::CheckedPointContextV39::check(class.source(), &max, claim, &post.claim, &inputs)
        .map_err(bad)?;
    Ok(f(&context))
}

/// The open accusation window of a claim and the policies it reads.
fn open_claim(
    builder: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    id: &Digest,
) -> Result<(NativeMiningClaimRecordV39, NativeMiningPolicyV39, u64), PalwStateV2Error> {
    let policy = super::super::palw_native_mining_fold_v39::policy(builder, ctx)?;
    let cert = builder.params.rfc0018_v1().ok_or_else(|| bad("mining39 accusation requires RFC-0018"))?.certification;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let c = super::super::palw_native_mining_fold_v39::claim(r, id)?;
    let until = palw_native_mining_liability_until_v39(c.admitted_daa, &policy, &cert);
    if native_mining_convicted_v39(&builder.state, &c.claim_hash())
        || c.closed.is_some_and(|(_, issued)| !issued)
        || ctx.daa_score > until
    {
        return Err(bad("mining39 accusation: the claim is convicted, closed without reward, or past its liability"));
    }
    Ok((c, policy, until))
}

fn independent(
    builder: &TransitionBuilder<'_>,
    signer: &PalwBondKeyV2,
    c: &NativeMiningClaimRecordV39,
) -> Result<Digest, PalwStateV2Error> {
    let bond = builder.state.bonds.get(signer).ok_or_else(|| bad("mining39 accuser bond absent"))?;
    let operator = bond.operator_id.as_bytes();
    if !matches!(bond.status, PalwBondStatusV2::Active) || *signer == c.producer || operator == c.operator {
        return Err(bad("mining39 accuser is not an Active bond independent of the producer"));
    }
    Ok(operator)
}

fn counts(r: &PalwKernelRouteStateV1, claim: &Digest) -> NativeMiningAccusationCountsV39 {
    mread(r, MINING_ACCUSATION_COUNT_KIND_V39, claim, 64).unwrap_or_default()
}

pub(super) fn apply(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    let w = NativeMiningAccusationWireV39::from_bytes(body).map_err(bad)?;
    match w.kind {
        0 => file(builder, ctx, signer, p, &w),
        1 => demand(builder, ctx, signer, p, &w),
        2 => answer(builder, ctx, signer, p, &w),
        _ => default(builder, ctx, signer, p, &w),
    }
}

fn file(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    w: &NativeMiningAccusationWireV39,
) -> Result<(), PalwStateV2Error> {
    let (c, policy, _) = open_claim(builder, ctx, &w.claim)?;
    let operator = independent(builder, signer, &c)?;
    let h = component(builder.state.kernel_route.as_ref().unwrap(), &c, w.stage)?;
    if !prepay(builder, ctx, signer, p, h.source_bytes as u64 + h.consent_bytes as u64 + h.post_bytes as u64, w.body.len(), 65536)? {
        return Ok(());
    }
    let verified = with_context(builder, p, &h, |cx| misaka_palw_kernel::flat_point39::check_flat_filing_v39(cx, &w.body).is_ok())?;
    if !verified {
        // The fee is spent; nothing else changes (a false accusation never touches the producer).
        return Ok(());
    }
    let proof = object_id(b"misaka-palw/native/g14-mining-post-final-filing/v39", &(c.id, w.stage, h.id, &w.body));
    convict(builder, ctx, &policy, &c, w.stage, proof, Some((*signer, operator)))
}

fn demand(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    w: &NativeMiningAccusationWireV39,
) -> Result<(), PalwStateV2Error> {
    let (c, _, until) = open_claim(builder, ctx, &w.claim)?;
    let operator = independent(builder, signer, &c)?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let h = component(r, &c, w.stage)?;
    let mut n = counts(r, &c.id);
    let deadline = ctx.daa_score.saturating_add(p.execution.execution.horizons.da_answer.max(1));
    if n.demands >= MINING_ACCUSATION_DEMANDS_PER_CLAIM_V39 || n.open >= MINING_ACCUSATION_OPEN_PER_CLAIM_V39 || deadline > until {
        return Err(bad("mining39 accusation demand capacity of the claim is exhausted or its answer would outlive the liability"));
    }
    let id = object_id(b"misaka-palw/native/g14-mining-post-final-demand/v39", &(c.id, w.stage, &w.body, n.demands));
    if r.aux.contains_key(&(MINING, mk(MINING_ACCUSATION_DEMAND_KIND_V39, &id))) {
        return Err(bad("mining39 accusation demand duplicate"));
    }
    let bond = r.header.policy.demand_bond.max(1);
    let record = builder.state.bonds.get(signer).unwrap();
    let payout = record.payout_payload;
    if (record.collateral as u128).saturating_sub(builder.committed_at(signer, ctx.daa_score)) < bond as u128 {
        return Err(bad("mining39 accusation demand bond unavailable"));
    }
    if !prepay(builder, ctx, signer, p, h.source_bytes as u64 + h.consent_bytes as u64 + h.post_bytes as u64, w.body.len(), 65536)? {
        return Ok(());
    }
    let Ok(d) = FlatDemandV39::from_bytes(&w.body) else { return Ok(()) };
    if !with_context(builder, p, &h, |cx| check_flat_demand_v39(cx, &d).is_ok())? {
        return Ok(());
    }
    if builder.slash_bond(*signer, bond as u128)? != bond {
        return Err(PalwStateV2Error::BondBudgetRefused("mining39 accusation demand bond changed after preflight".into()));
    }
    let row = NativeMiningDemandV39 {
        version: 39,
        id,
        claim: c.id,
        stage: w.stage,
        job: h.id,
        demand: w.body.clone(),
        accuser: *signer,
        accuser_operator: operator,
        accuser_payout: payout,
        bond,
        opened: ctx.daa_score,
        deadline,
        settled: false,
    };
    n.demands += 1;
    n.open += 1;
    builder.write_kernel_row(MINING, mk(MINING_ACCUSATION_DEMAND_KIND_V39, &id), Some(b(&row)))?;
    builder.write_kernel_row(MINING, mk(MINING_ACCUSATION_COUNT_KIND_V39, &c.id), Some(b(&n)))?;
    builder.note_kernel_bond(signer)?;
    Ok(())
}

fn demand_row(r: &PalwKernelRouteStateV1, raw: &[u8]) -> Result<NativeMiningDemandV39, PalwStateV2Error> {
    let id: Digest = raw.get(..64).and_then(|v| v.try_into().ok()).ok_or_else(|| bad("mining39 accusation names its demand"))?;
    let d: NativeMiningDemandV39 =
        mread(r, MINING_ACCUSATION_DEMAND_KIND_V39, &id, 1 << 20).ok_or_else(|| bad("mining39 accusation demand absent"))?;
    if d.id != id || d.version != 39 {
        return Err(bad("mining39 accusation demand identity differs"));
    }
    Ok(d)
}

fn close_demand(builder: &mut TransitionBuilder<'_>, mut d: NativeMiningDemandV39) -> Result<(), PalwStateV2Error> {
    let mut n = counts(builder.state.kernel_route.as_ref().unwrap(), &d.claim);
    n.open = n.open.saturating_sub(1);
    d.settled = true;
    builder.write_kernel_row(MINING, mk(MINING_ACCUSATION_DEMAND_KIND_V39, &d.id), Some(b(&d)))?;
    builder.write_kernel_row(MINING, mk(MINING_ACCUSATION_COUNT_KIND_V39, &d.claim), Some(b(&n)))?;
    Ok(())
}

/// Kind 2: `demand ‖ total u32 ‖ index u16 ‖ part` from the producer. The assembled reply must pass `check_flat_reply_v39`; a reply
/// that does not keeps the demand open (the producer may retry until the deadline).
fn answer(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    w: &NativeMiningAccusationWireV39,
) -> Result<(), PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let d = demand_row(r, &w.body)?;
    if w.body.len() < 70 {
        return Err(bad("mining39 accusation answer part header"));
    }
    let total = u32::from_le_bytes(w.body[64..68].try_into().unwrap());
    let index = u16::from_le_bytes(w.body[68..70].try_into().unwrap());
    let part = &w.body[70..];
    let parts = total.div_ceil(MINING_ACCUSATION_PART_BYTES_V39 as u32);
    let expected = (total as usize - (index as usize * MINING_ACCUSATION_PART_BYTES_V39).min(total as usize))
        .min(MINING_ACCUSATION_PART_BYTES_V39);
    let c = super::super::palw_native_mining_fold_v39::claim(r, &d.claim)?;
    if d.settled
        // audit A-3: a convicted claim's open demand is refunded to its accuser (`default`), never paid to the convicted producer.
        || native_mining_convicted_v39(&builder.state, &c.claim_hash())
        || *signer != c.producer
        || ctx.daa_score > d.deadline
        || total == 0
        || parts > MINING_ACCUSATION_PARTS_MAX_V39
        || index as u32 >= parts
        || part.len() != expected
    {
        return Err(bad("mining39 accusation answer is not the producer's in-time bounded part"));
    }
    let h = header(r, d.job)?;
    if !prepay(builder, ctx, signer, p, h.source_bytes as u64 + h.consent_bytes as u64 + h.post_bytes as u64, w.body.len(), 65536)? {
        return Ok(());
    }
    builder.write_kernel_row(MINING, mk(MINING_ACCUSATION_REPLY_KIND_V39, &(d.id, index)), Some(part.to_vec()))?;
    let r = builder.state.kernel_route.as_ref().unwrap();
    let mut reply = Vec::with_capacity(total as usize);
    for i in 0..parts as u16 {
        let Some(v) = r.aux.get(&(MINING, mk(MINING_ACCUSATION_REPLY_KIND_V39, &(d.id, i)))) else { return Ok(()) };
        reply.extend_from_slice(v);
    }
    if reply.len() != total as usize {
        return Ok(());
    }
    let (Ok(demand), Ok(a)) = (FlatDemandV39::from_bytes(&d.demand), FlatReplyV39::from_bytes(&reply)) else {
        return Ok(());
    };
    if !with_context(builder, p, &h, |cx| check_flat_reply_v39(cx, &demand, &a).is_ok())? {
        return Ok(());
    }
    // The reply is public material for every outsider's filing; the accuser's bond pays the producer for it.
    for i in 0..parts as u16 {
        builder.write_kernel_row(MINING, mk(MINING_ACCUSATION_REPLY_KIND_V39, &(d.id, i)), None)?;
    }
    builder.write_kernel_row(MINING, mk(MINING_ACCUSATION_REPLY_KIND_V39, &(d.id, u16::MAX)), Some(reply))?;
    let key = Hash64::from_bytes(object_id(b"misaka-palw/native/g14-mining-demand-fee/v39", &d.id));
    builder.write_payout(key, Some(PalwPayoutV2 { payload: h.producer_payout, amount: d.bond }))?;
    close_demand(builder, d)
}

/// Kind 3: `demand` past its deadline, unanswered → the producer defaulted on its committed answer: conviction. Of a claim already
/// convicted by another path: the demand's bond is refunded to its accuser and the demand closes (audit A-3).
fn default(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    w: &NativeMiningAccusationWireV39,
) -> Result<(), PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let d = demand_row(r, &w.body)?;
    let c = super::super::palw_native_mining_fold_v39::claim(r, &d.claim)?;
    // audit A-3: a claim convicted by ANOTHER path (a verified filing, or another demand's default) while this demand is open owes
    // no answer any more (`answer` refuses it), so the honest accuser's burned demand bond is refunded once and the demand closes —
    // without a second conviction and without waiting for a deadline nobody can meet. Unconvicted: the default rule as before.
    if native_mining_convicted_v39(&builder.state, &c.claim_hash()) {
        if d.settled {
            return Err(bad("mining39 accusation refund of a settled demand"));
        }
        // No court-work prepay: the refund decodes one fixed demand row and writes two rows (no material, no adjudication), and
        // succeeds once per demand (a settled demand is refused above before any work).
        let _ = (signer, p);
        let refund = Hash64::from_bytes(object_id(b"misaka-palw/native/g14-mining-demand-refund/v39", &d.id));
        builder.write_payout(refund, Some(PalwPayoutV2 { payload: d.accuser_payout, amount: d.bond }))?;
        return close_demand(builder, d);
    }
    if d.settled || ctx.daa_score <= d.deadline {
        return Err(bad("mining39 accusation default before the deadline or of a settled demand"));
    }
    let policy = super::super::palw_native_mining_fold_v39::policy(builder, ctx)?;
    if !prepay(builder, ctx, signer, p, 0, w.body.len(), 0)? {
        return Ok(());
    }
    let proof = object_id(b"misaka-palw/native/g14-mining-post-final-default/v39", &(d.id, d.deadline));
    let refund = Hash64::from_bytes(object_id(b"misaka-palw/native/g14-mining-demand-refund/v39", &d.id));
    builder.write_payout(refund, Some(PalwPayoutV2 { payload: d.accuser_payout, amount: d.bond }))?;
    let finder = (d.accuser, d.accuser_operator);
    let stage = d.stage;
    close_demand(builder, d)?;
    convict(builder, ctx, &policy, &c, stage, proof, Some(finder))
}

/// **The verdict on a mining claim** (a verified filing or a default). Never refused for short collateral (G-14).
pub(super) fn convict(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    _policy: &NativeMiningPolicyV39,
    c: &NativeMiningClaimRecordV39,
    stage: u16,
    proof: Digest,
    finder: Option<(PalwBondKeyV2, Digest)>,
) -> Result<(), PalwStateV2Error> {
    let now = ctx.daa_score;
    let hash = c.claim_hash();
    // 1. The producer's exposure: what its bond still holds of the reserved price.
    let collectible = crate::palw_g14_lane_closure_v1::palw_native_producer_collectible_v1(
        builder.state.bonds.get(&c.producer).map(|b| b.collateral),
        u64::try_from(c.price.reserved_exposure).unwrap_or(u64::MAX),
    );
    let collected = if collectible == 0 { 0 } else { builder.slash_bond(c.producer, collectible as u128)? };
    let finder_credit = match finder {
        Some(_) => {
            (u128::from(collected) * u128::from(crate::palw_g14_lane_closure_v1::PALW_NATIVE_FINDER_SHARE_CAP_PERMILLE_V1) / 1000)
                as u64
        }
        None => 0,
    };
    if let (Some((bond, _)), true) = (finder, finder_credit > 0) {
        let payload = builder.state.bonds.get(&bond).map(|b| b.payout_payload).unwrap_or_default();
        let key = Hash64::from_bytes(object_id(b"misaka-palw/native/g14-mining-finder/v39", &c.id));
        builder.write_payout(key, Some(PalwPayoutV2 { payload, amount: finder_credit }))?;
    }
    // 2. The receipt and the weight it granted.
    let r = builder.state.kernel_route.as_ref().unwrap();
    let receipt_id = native_mining_receipt_id_v39(c.id);
    let had_receipt = r.aux.contains_key(&(MINING, mk(MINING_RECEIPT_KIND_V39, &receipt_id)));
    let revoked: u128 = mread(r, MINING_RECEIPT_GRANTED_KIND_V39, &receipt_id, 32).unwrap_or(0);
    builder.state.safe_weight = builder.state.safe_weight.saturating_sub(revoked);
    let mut m = super::super::palw_native_mining_fold_v39::model(builder.state.kernel_route.as_ref().unwrap(), &c.model)?;
    let mut seat =
        super::super::palw_native_mining_fold_v39::seat(builder.state.kernel_route.as_ref().unwrap(), &c.model, &c.producer);
    let mut closed = c.clone();
    if had_receipt {
        builder.write_kernel_row(MINING, mk(MINING_RECEIPT_KIND_V39, &receipt_id), None)?;
        m.live_receipts = m.live_receipts.saturating_sub(1);
    }
    if closed.closed.is_none() {
        m.live_claims = m.live_claims.saturating_sub(1);
        if let Some(s) = seat.as_mut() {
            s.live_claims = s.live_claims.saturating_sub(1);
        }
    }
    // Closed without reward; the ADR-0176 reservation closes (nothing returns).
    closed.closed = Some((now, false));
    builder.bond_budget_close_v1(&native_mining_budget_id_v39(&c.id), now)?;
    super::super::palw_native_mining_fold_v39::write_claim(builder, &closed)?;
    super::super::palw_native_mining_fold_v39::write_model(builder, &m)?;
    if let Some(s) = seat {
        super::super::palw_native_mining_fold_v39::write_seat(builder, &s)?;
    }
    // 3. Gate C: the certification row is withdrawn — no SecurityCertified credit, decided or not.
    super::super::palw_security_certification_fold_v1::withdraw_native_mining_v39(builder, &hash, now)?;
    // 4. Every Reproduced attestor's lock is charged (burned).
    let attestors_charged = super::super::palw_security_certification_fold_v1::charge_native_attestor_locks_v39(builder, &hash)?;
    // 5. The material holds end with the claim's liability.
    release_holds(builder, c)?;
    let tomb = NativeMiningConvictionV39 {
        version: 39,
        claim: c.id,
        stage,
        proof,
        established: now,
        collected,
        finder: finder.map(|(b, _)| b),
        finder_credit,
        revoked_weight: revoked,
        attestors_charged,
    };
    builder.write_kernel_row(MINING, mk(MINING_CONVICTED_KIND_V39, &c.id), Some(b(&tomb)))?;
    Ok(())
}

/// The holds `commit` wrote for the claim's credited components.
pub(super) fn release_holds(builder: &mut TransitionBuilder<'_>, c: &NativeMiningClaimRecordV39) -> Result<(), PalwStateV2Error> {
    let r = builder.state.kernel_route.as_ref().unwrap();
    let Some(fin) = super::super::palw_native_mining_fold_v39::master_final(r, &c.master) else { return Ok(()) };
    let jobs: Vec<Digest> = fin.components.iter().filter(|x| c.stages.contains(&x.original.stage)).map(|x| x.original.job).collect();
    for job in jobs {
        let held: Option<(Digest, u64)> = mread(builder.state.kernel_route.as_ref().unwrap(), MINING_JOB_HOLD_KIND_V39, &job, 128);
        if held.is_some_and(|(claim, _)| claim == c.id) {
            builder.write_kernel_row(MINING, mk(MINING_JOB_HOLD_KIND_V39, &job), None)?;
        }
    }
    Ok(())
}

/// **Collection 53's guard:** a component whose material a live, accusable mining claim relies on is not collected.
pub(super) fn job_held(r: &PalwKernelRouteStateV1, job: &Digest, now: u64) -> bool {
    mread::<(Digest, u64)>(r, MINING_JOB_HOLD_KIND_V39, job, 128)
        .is_some_and(|(claim, until)| now <= until && r.aux.contains_key(&(MINING, mk(MINING_CLAIM_KIND_V39, &claim))))
}

#[cfg(test)]
mod tests {
    //! G-N2 verdict over the fold's own rows (a TransitionBuilder on a state carrying a certified, receipted mining claim). The
    //! filing / demand / answer verification itself is the job court's and public lane's (`check_flat_filing_v39`,
    //! `check_flat_demand_v39`, `check_flat_reply_v39`); a v39 job fixture to drive it end to end does not exist in the repository.
    use super::*;
    use crate::palw_model_eligibility_v1::PalwEligibilityAttestationRowV1;
    use crate::palw_security_certification_v1::*;
    use std::collections::BTreeSet;

    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }
    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_u64_word(n), index: 0 })
    }
    fn state() -> PalwChainStateV2 {
        let mut state = PalwChainStateV2::genesis();
        for n in 1..=4u64 {
            state.bonds.insert(
                bond(n),
                PalwBondStateV2 {
                    pubkey: vec![n as u8],
                    operator_id: h(n as u8),
                    collateral: 10_000,
                    slashed: 0,
                    status: PalwBondStatusV2::Active,
                    registered_daa: 0,
                    payout_payload: h(n as u8 + 100),
                    capable_classes: BTreeSet::new(),
                },
            );
        }
        state
    }
    fn ctx(daa: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: h(200), daa_score: daa, blue_score: daa, subsidy: 0 }
    }
    fn claim(exposure: u128) -> NativeMiningClaimRecordV39 {
        NativeMiningClaimRecordV39 {
            version: 39,
            id: [7; 64],
            model: [8; 64],
            master: [9; 64],
            master_final: [10; 64],
            master_established: 5,
            complete_source: [11; 64],
            stages: vec![0],
            producer: bond(1),
            operator: h(1).as_bytes(),
            admitted_daa: 10,
            epoch: 0,
            policy: [12; 64],
            work: NativeInferenceWorkV39 {
                version: 39,
                source: [13; 64],
                first_position: 0,
                end_position: 4,
                mac_equivalents: 400,
                bytes_read: 0,
                bytes_written: 0,
            },
            price: NativeMiningPriceV39 {
                version: 39,
                model: [8; 64],
                master: [9; 64],
                work: [14; 64],
                credited_mac: 400,
                quantum_mac: 100,
                quanta: 4,
                reserved_exposure: exposure,
            },
            closed: Some((20, true)),
        }
    }

    /// Writes a certified, receipted claim with two Reproduced attestors (bonds 3 and 4, each locked 600) and 3 spent quanta.
    fn world(tb: &mut TransitionBuilder<'_>, c: &NativeMiningClaimRecordV39) {
        super::super::super::palw_kernel_route_fold_v1::ensure_route_header(tb, &ctx(1)).unwrap();
        let m = NativeMiningModelRecordV39 {
            version: 39,
            model: c.model,
            economic_class: [1; 64],
            template_master: [2; 64],
            complete_source: c.complete_source,
            manifest: [3; 64],
            pipeline: [4; 64],
            class_set: [5; 64],
            source_classes: vec![],
            original_task: [6; 64],
            original_contexts: vec![],
            registrant: bond(2),
            registrant_operator: h(2).as_bytes(),
            share_permille: 0,
            admitted_daa: 1,
            policy: [12; 64],
            canonical_cost_table: inference_cost_table_v39(),
            seats: 1,
            live_claims: 0,
            live_receipts: 1,
            retired: None,
        };
        super::super::super::palw_native_mining_fold_v39::write_model(tb, &m).unwrap();
        super::super::super::palw_native_mining_fold_v39::write_claim(tb, c).unwrap();
        let receipt = NativeMiningReceiptRecordV39 {
            version: 39,
            id: native_mining_receipt_id_v39(c.id),
            claim: c.id,
            model: c.model,
            master: c.master,
            producer: c.producer,
            operator: c.operator,
            approval: [15; 64],
            price: c.price.clone(),
            established_daa: 20,
            expires: 1_000,
            spent_quanta: 3,
            spent: vec![0b0111],
        };
        tb.write_kernel_row(
            MINING,
            mk(MINING_RECEIPT_KIND_V39, &receipt.id),
            Some(borsh::to_vec(&(receipt.root(), &receipt)).unwrap()),
        )
        .unwrap();
        tb.write_kernel_row(MINING, mk(MINING_RECEIPT_GRANTED_KIND_V39, &receipt.id), Some(super::b(&300u128))).unwrap();
        tb.state.safe_weight = 1_000;
        let attest = |n: u64| PalwEligibilityAttestationRowV1 {
            verifier: palw_kernel_bond_id_v1(&bond(n)),
            verdict: crate::palw_model_eligibility_v1::PalwEligibilityVerdictV1::Reproduced,
            transcript: [n as u8; 64],
            attested_daa: 15,
        };
        let row = PalwClaimCertificationRowV1 {
            claim: c.id,
            class_id: c.model,
            producer_bond: palw_kernel_bond_id_v1(&c.producer),
            producer_principal: c.operator,
            attempt: false,
            final_daa: 5,
            committed_daa: 10,
            committed_span: 0,
            anchor: Some((1, [0; 64])),
            draws: vec![palw_kernel_bond_id_v1(&bond(3)), palw_kernel_bond_id_v1(&bond(4))],
            drawn_daa: 12,
            deadline_daa: 30,
            attestations: vec![attest(3), attest(4)],
            held_reward: 0,
            payout_payload: [0; 64],
            outcome: Some((PalwCertificationOutcomeV1::Certified, 18)),
        };
        let key = certification_native_key_v39(&palw_native_certification_id_v39(&c.claim_hash()));
        tb.write_kernel_row(PALW_SECURITY_CERTIFICATION_TABLE_V1, key, Some(borsh::to_vec(&row).unwrap())).unwrap();
        for n in [3u64, 4] {
            use crate::palw_g14_lane_closure_v1::*;
            let lock = PalwKernelSignerLockRowV1 { version: 1, claim: c.id, seat: bond(n), amount: 600, locked_daa: 15 };
            tb.write_kernel_row(
                PALW_G14_LANE_CLOSURE_TABLE_V1,
                palw_native_attestor_lock_key_v1(&palw_kernel_bond_id_v1(&bond(n)), &c.id),
                Some(borsh::to_vec(&lock).unwrap()),
            )
            .unwrap();
        }
    }

    fn params() -> PalwStateParamsV2 {
        PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h(1), 4, 1000, 100, 1000, 0).unwrap()
    }
    fn extras() -> PalwTransitionExtrasV1 {
        PalwTransitionExtrasV1 {
            kernel_route: Some(crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1 {
                network_domain: h(1),
                ruleset_digest: h(2),
                ..Default::default()
            }),
            ..Default::default()
        }
    }
    fn policy() -> NativeMiningPolicyV39 {
        PalwNativeMiningFenceV39::unapproved_probe_at(crate::config::params::ForkActivation::always(), [1; 64]).policy
    }

    #[test]
    fn g14n2_a_post_final_conviction_slashes_revokes_the_receipt_weight_and_certification_and_charges_attestors() {
        let (state, params, extras) = (state(), params(), extras());
        let mut tb = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        let c = claim(4_000);
        world(&mut tb, &c);
        assert_eq!(super::super::super::palw_native_mining_fold_v39::live_safe_weight_v39(&tb.state).unwrap(), 300);
        let hash = c.claim_hash();
        assert!(crate::palw_security_certification_v1::palw_native_mining_security_certified_v39(&tb.state, &hash), "the premise");
        assert_eq!(tb.state.kernel_route.as_ref().unwrap().signer_locks_of_v1(&bond(3)), 600, "the lock is in the committed ledger");
        convict(&mut tb, &ctx(40), &policy(), &c, 0, [0x99; 64], Some((bond(2), h(2).as_bytes()))).unwrap();
        // The producer's reserved exposure is collected; the finder is paid 490‰ of it; the rest is burned.
        assert_eq!(tb.state.bonds[&bond(1)].collateral, 6_000);
        let finder =
            tb.state.pending_payouts.get(&Hash64::from_bytes(object_id(b"misaka-palw/native/g14-mining-finder/v39", &c.id))).unwrap();
        assert_eq!((finder.amount, finder.payload), (1_960, h(102)));
        // Receipt gone, its granted weight taken back, the claim closed without reward, the certification withdrawn.
        let r = tb.state.kernel_route.as_ref().unwrap();
        assert!(!r.aux.contains_key(&(MINING, mk(MINING_RECEIPT_KIND_V39, &native_mining_receipt_id_v39(c.id)))));
        assert_eq!(tb.state.safe_weight, 700);
        assert_eq!(super::super::super::palw_native_mining_fold_v39::live_safe_weight_v39(&tb.state).unwrap(), 0);
        let closed = super::super::super::palw_native_mining_fold_v39::claim(r, &c.id).unwrap();
        assert_eq!(closed.closed, Some((40, false)));
        assert!(!crate::palw_security_certification_v1::palw_native_mining_security_certified_v39(&tb.state, &hash));
        assert!(native_mining_convicted_v39(&tb.state, &hash));
        // Each Reproduced attestor's lock is charged and leaves the ledger.
        assert_eq!(tb.state.bonds[&bond(3)].collateral, 9_400);
        assert_eq!(tb.state.bonds[&bond(4)].collateral, 9_400);
        assert_eq!(tb.state.kernel_route.as_ref().unwrap().signer_locks_of_v1(&bond(3)), 0);
        let tomb: NativeMiningConvictionV39 =
            mread(tb.state.kernel_route.as_ref().unwrap(), MINING_CONVICTED_KIND_V39, &c.id, 4096).unwrap();
        assert_eq!((tomb.collected, tomb.finder_credit, tomb.revoked_weight, tomb.attestors_charged), (4_000, 1_960, 300, 1_200));
    }

    /// T-N13 for the mining court: a producer drained below its exposure is still convicted; only what it holds is collected.
    #[test]
    fn native39_receipt_retirement_preserves_the_restart_weight_identity() {
        use super::super::super::palw_native_mining_fold_v39::{live_safe_weight_v39, retire_receipt_weight_v39};
        let (state, params, extras) = (state(), params(), extras());
        let mut tb = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        let c = claim(4_000);
        world(&mut tb, &c);
        let before = live_safe_weight_v39(&tb.state).unwrap() + tb.state.retired_safe_weight;
        assert_eq!(before, 300);
        let safe = tb.state.safe_weight;
        retire_receipt_weight_v39(&mut tb, &native_mining_receipt_id_v39(c.id)).unwrap();
        assert_eq!(live_safe_weight_v39(&tb.state).unwrap(), 0);
        assert_eq!(tb.state.retired_safe_weight, before);
        assert_eq!(tb.state.safe_weight, safe, "collection never removes already earned safe weight");
        retire_receipt_weight_v39(&mut tb, &native_mining_receipt_id_v39(c.id)).unwrap();
        assert_eq!(tb.state.retired_safe_weight, before, "a collected grant is never counted twice");
    }

    #[test]
    fn g14n2_a_mining_conviction_is_not_refused_for_short_collateral() {
        let (mut state, params, extras) = (state(), params(), extras());
        state.bonds.get_mut(&bond(1)).unwrap().collateral = 1_000;
        let mut tb = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        let c = claim(4_000);
        world(&mut tb, &c);
        convict(&mut tb, &ctx(40), &policy(), &c, 0, [0x99; 64], Some((bond(2), h(2).as_bytes()))).unwrap();
        assert_eq!(tb.state.bonds[&bond(1)].collateral, 0);
        let tomb: NativeMiningConvictionV39 =
            mread(tb.state.kernel_route.as_ref().unwrap(), MINING_CONVICTED_KIND_V39, &c.id, 4096).unwrap();
        assert_eq!((tomb.collected, tomb.finder_credit), (1_000, 490));
        assert!(!crate::palw_security_certification_v1::palw_native_mining_security_certified_v39(&tb.state, &c.claim_hash()));
    }

    #[test]
    fn g14n2_an_open_demand_is_gate_c_pending_and_a_hold_keeps_the_component_material() {
        let (state, params, extras) = (state(), params(), extras());
        let mut tb = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        let c = claim(4_000);
        world(&mut tb, &c);
        let hash = c.claim_hash();
        assert!(!crate::palw_native_receipt_v39::native_mining_accusation_pending_v39(&tb.state, &hash));
        tb.write_kernel_row(
            MINING,
            mk(MINING_ACCUSATION_COUNT_KIND_V39, &c.id),
            Some(super::b(&NativeMiningAccusationCountsV39 { demands: 1, open: 1 })),
        )
        .unwrap();
        assert!(crate::palw_native_receipt_v39::native_mining_accusation_pending_v39(&tb.state, &hash));
        // The certification verdict waits on a pending accusation, k positives in hand (RFC-0018's own verdict, veto = 0).
        let policy = PalwSecurityCertificationPolicyV1::design_unapproved_v1(100);
        assert_eq!(policy.veto, PALW_CERT_VETO_DESIGN_V1);
        assert_eq!(PALW_CERT_VETO_DESIGN_V1, 0, "RFC-0018 U-6 is not adopted");
        // A hold keeps collection away while the claim is live and inside its liability.
        let job = [21u8; 64];
        tb.write_kernel_row(MINING, mk(MINING_JOB_HOLD_KIND_V39, &job), Some(super::b(&(c.id, 500u64)))).unwrap();
        let r = tb.state.kernel_route.as_ref().unwrap();
        assert!(job_held(r, &job, 500));
        assert!(!job_held(r, &job, 501), "past the liability horizon the material may be collected");
    }

    #[test]
    fn g14n2_the_liability_horizon_spans_certification_receipt_and_one_prosecution_window() {
        let p = policy();
        let cert = PalwSecurityCertificationPolicyV1::design_unapproved_v1(100);
        let until = palw_native_mining_liability_until_v39(1_000, &p, &cert);
        assert_eq!(until, 1_000 + cert.lifetime_daa() + p.receipt_lifetime_daa + cert.prosecution_window_daa);
        assert!(until > 1_000 + p.receipt_lifetime_daa, "outlives every receipt quantum");
        let wire = NativeMiningAccusationWireV39 { version: 39, kind: 4, claim: [1; 64], stage: 0, body: vec![1] };
        assert!(NativeMiningAccusationWireV39::from_bytes(&borsh::to_vec(&wire).unwrap()).is_err(), "no fifth kind");
        let big = NativeMiningAccusationWireV39 { kind: 0, body: vec![0; MINING_ACCUSATION_MAX_BYTES_V39], ..wire };
        assert!(NativeMiningAccusationWireV39::from_bytes(&borsh::to_vec(&big).unwrap()).is_err(), "bounded extent");
    }

    /// An honest certified claim no one accuses keeps its certification, receipt, weight, producer collateral and attestor locks;
    /// Gate C sees no pending accusation and no conviction.
    #[test]
    fn g14n2_an_unaccused_honest_claim_is_untouched() {
        let (state, params, extras) = (state(), params(), extras());
        let mut tb = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        let c = claim(4_000);
        world(&mut tb, &c);
        let hash = c.claim_hash();
        assert!(!crate::palw_native_receipt_v39::native_mining_accusation_pending_v39(&tb.state, &hash));
        assert!(!native_mining_convicted_v39(&tb.state, &hash));
        assert!(crate::palw_security_certification_v1::palw_native_mining_security_certified_v39(&tb.state, &hash));
        let r = tb.state.kernel_route.as_ref().unwrap();
        assert!(r.aux.contains_key(&(MINING, mk(MINING_RECEIPT_KIND_V39, &native_mining_receipt_id_v39(c.id)))));
        assert_eq!(tb.state.safe_weight, 1_000);
        assert_eq!(tb.state.bonds[&bond(1)].collateral, 10_000);
        assert_eq!(r.signer_locks_of_v1(&bond(3)), 600);
        assert_eq!(super::super::super::palw_native_mining_fold_v39::claim(r, &c.id).unwrap().closed, Some((20, true)));
    }

    /// Below the owning39 mining fence (no `native_mining39` in the route extras) action 80 is refused and changes nothing.
    #[test]
    fn g14n2_below_the_mining_fence_action_80_is_refused_without_effect() {
        let (state, params, extras) = (state(), params(), extras());
        assert!(extras.kernel_route.as_ref().unwrap().native_mining39.is_none());
        let mut tb = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        let c = claim(4_000);
        world(&mut tb, &c);
        let before = (tb.state.kernel_route.as_ref().unwrap().aux.clone(), tb.state.safe_weight, tb.state.bonds.clone());
        let wire = NativeMiningAccusationWireV39 { version: 39, kind: 0, claim: c.id, stage: 0, body: vec![1] };
        let body = borsh::to_vec(&wire).unwrap();
        assert!(super::super::apply(&mut tb, &ctx(40), &bond(2), ACCUSE_MINING_ACTION_V39, &body).is_err());
        assert!(super::super::super::palw_native_mining_fold_v39::policy(&tb, &ctx(40)).is_err());
        let after = (tb.state.kernel_route.as_ref().unwrap().aux.clone(), tb.state.safe_weight, tb.state.bonds.clone());
        assert!(before == after, "a refused accusation below the fence leaves the state byte-identical");
        assert!(!native_mining_convicted_v39(&tb.state, &c.claim_hash()));
    }

    /// **audit(verify-1) A-3**: an honest accuser's demand is open (bond 50 already burned from its bond at `demand`) when ANOTHER
    /// filing convicts the claim. `convict` settles no open demand, and the demand's own `default` is refused for a convicted claim,
    /// so the burned demand bond is never refunded (the only refund leg lives in `default`). Expected: refunded once; actual: never.
    #[test]
    fn audit_verify1_a3_open_demand_bond_is_stranded_by_a_conviction_from_another_path() {
        let (mut state, params, extras) = (state(), params(), extras());
        state.bonds.get_mut(&bond(3)).unwrap().collateral -= 50; // what `demand` burned (`slash_bond(signer, bond)`)
        let mut tb = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        let c = claim(4_000);
        world(&mut tb, &c);
        let id = [0x5d; 64];
        let d = NativeMiningDemandV39 {
            version: 39,
            id,
            claim: c.id,
            stage: 0,
            job: [21; 64],
            demand: vec![],
            accuser: bond(3),
            accuser_operator: h(3).as_bytes(),
            accuser_payout: h(103),
            bond: 50,
            opened: 25,
            deadline: 30,
            settled: false,
        };
        tb.write_kernel_row(MINING, mk(MINING_ACCUSATION_DEMAND_KIND_V39, &id), Some(super::b(&d))).unwrap();
        tb.write_kernel_row(
            MINING,
            mk(MINING_ACCUSATION_COUNT_KIND_V39, &c.id),
            Some(super::b(&NativeMiningAccusationCountsV39 { demands: 1, open: 1 })),
        )
        .unwrap();
        // Another finder's verified filing convicts the claim first (inside the demand's answer window).
        convict(&mut tb, &ctx(28), &policy(), &c, 0, [0x99; 64], Some((bond(2), h(2).as_bytes()))).unwrap();
        // The accuser then files its default once the deadline has passed.
        let p = crate::palw_native_execution_v39::PalwNativeExecutionFenceV39::unapproved_probe_at(
            crate::config::params::ForkActivation::always(),
            [1; 64],
        )
        .policies;
        let wire = NativeMiningAccusationWireV39 { version: 39, kind: 3, claim: c.id, stage: 0, body: id.to_vec() };
        let defaulted = default(&mut tb, &ctx(31), &bond(3), &p, &wire);
        let refund_key = Hash64::from_bytes(object_id(b"misaka-palw/native/g14-mining-demand-refund/v39", &id));
        let refunded = tb.state.pending_payouts.get(&refund_key).map(|v| v.amount);
        let row: NativeMiningDemandV39 =
            mread(tb.state.kernel_route.as_ref().unwrap(), MINING_ACCUSATION_DEMAND_KIND_V39, &id, 1 << 20).unwrap();
        eprintln!(
            "[verify-1 A-3] default after conviction: {defaulted:?}; demand settled {}; refund payout {refunded:?}",
            row.settled
        );
        assert_eq!(refunded, Some(50), "A-3: the honest accuser's burned demand bond is never refunded after another path convicts");
        // Regression (fix): refunded once — the demand is settled and its open count released; a second default and a late answer
        // of the convicted claim are refused (the producer is never paid the accuser's bond).
        assert!(defaulted.is_ok() && row.settled);
        let n: NativeMiningAccusationCountsV39 =
            mread(tb.state.kernel_route.as_ref().unwrap(), MINING_ACCUSATION_COUNT_KIND_V39, &c.id, 64).unwrap();
        assert_eq!((n.demands, n.open), (1, 0));
        assert!(default(&mut tb, &ctx(32), &bond(3), &p, &wire).is_err(), "refunded once");
        let mut late = id.to_vec();
        late.extend_from_slice(&1u32.to_le_bytes());
        late.extend_from_slice(&0u16.to_le_bytes());
        late.push(0);
        let answer_wire = NativeMiningAccusationWireV39 { version: 39, kind: 2, claim: c.id, stage: 0, body: late };
        assert!(
            answer(&mut tb, &ctx(29), &c.producer, &p, &answer_wire).is_err(),
            "a convicted claim's demand is never answered for pay"
        );
    }
}
