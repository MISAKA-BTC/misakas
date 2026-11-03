//! Pending legacy settlement sources survive claim retirement. Capturing a
//! priced candidate never grants work: promotion separately checks the real
//! approval, maturity, revocations and complete claim-local dependency closure.
use super::*;
use crate::palw_native_resource_v1::{NativeDecodeErrorV1, NativeHeapV1, NativeRetainedV1};
use crate::palw_native_settlement_v1::{NativeFactRulesV1, NativeLegacyPricedWorkV1, native_legacy_priced_source_v1};
use borsh::{BorshDeserialize, BorshSerialize};
pub const LEGACY_PENDING_FACT_TABLE_V1: u8 = 196;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct LegacyPendingFactV1 {
    pub version: u16,
    pub ruleset: crate::Hash,
    pub claim_id: Hash64,
    pub claim: PalwClaimStateV2,
    pub quantum: Option<u32>,
    pub fact: NativeLegacyPricedWorkV1,
    pub approval: Vec<u8>,
    pub source_point: PalwBlockContextV2,
    pub canonical_work_daa: Option<u64>,
    pub quantum_maturity_daa: u64,
    pub material_until_daa: Option<u64>,
    pub retired_at: Option<PalwBlockContextV2>,
    /// The actual preceding committed snapshot; the event's final state root
    /// cannot include its own archival row without circularity.
    pub source_parent_root: Hash64,
}
crate::palw_native_resource_v1::native_struct!(LegacyPendingFactV1;version,ruleset,claim_id,claim,quantum,fact,approval,source_point,canonical_work_daa,quantum_maturity_daa,material_until_daa,retired_at,source_parent_root);
crate::palw_native_resource_v1::wire::wire_struct!(LegacyPendingFactV1;u16,crate::Hash,Hash64,PalwClaimStateV2,Option<u32>,NativeLegacyPricedWorkV1,Vec<u8>,PalwBlockContextV2,Option<u64>,u64,Option<u64>,Option<PalwBlockContextV2>,Hash64);
crate::palw_native_resource_v1::native_struct!(NativeLegacyPricedWorkV1;identity,anchor,operator,class,anchor_blue,accepted_blue,anchor_daa,accepted_daa,work);
crate::palw_native_resource_v1::wire::wire_struct!(NativeLegacyPricedWorkV1;Hash64,Hash64,Hash64,Hash64,u64,u64,u64,u64,u128);
fn key(id: Hash64, claim: Hash64) -> [u8; 128] {
    let mut out = [0; 128];
    out[..64].copy_from_slice(claim.as_byte_slice());
    out[64..].copy_from_slice(id.as_byte_slice());
    out
}
fn bad(s: impl ToString) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(s.to_string())
}
pub(super) fn decode(bytes: &[u8]) -> Result<NativeRetainedV1<LegacyPendingFactV1>, PalwStateV2Error> {
    crate::palw_native_resource_v1::retain_native_value_decode_classified_v1::<LegacyPendingFactV1, std::io::Error>(bytes, || {
        borsh::from_slice(bytes)
    })
    .map_err(|e| match e {
        NativeDecodeErrorV1::Resource(r) => PalwStateV2Error::NativeResource(r),
        NativeDecodeErrorV1::Decode(e) => bad(e),
    })
}
fn enabled(b: &TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> bool {
    b.extras.native39_settlement_checkpoint.is_some_and(|c| c.retirement.activation.is_active(ctx.daa_score))
}
/// One borrowed real journal record, before it or its producer/certification
/// registry can disappear. Uncertified and dependency-held candidates remain
/// pending; absence of approval is never treated as rejection or success.
fn capture(
    b: &TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
    id: Hash64,
    claim: &PalwClaimStateV2,
    quantum: Option<u32>,
) -> Result<Option<NativeRetainedV1<LegacyPendingFactV1>>, PalwStateV2Error> {
    if !matches!(claim.phase, PalwClaimPhaseV2::Final { .. }) {
        return Ok(None);
    }
    let (complete, _exec_lease) = super::palw_claim_local_rewards_v1::prepare_exec_reader(&b.state, id)?;
    let empty_set = BTreeSet::new();
    let maturity = b.extras.native_fact_quantum_maturity_daa.unwrap_or_else(|| {
        b.extras.economic_safety.map_or_else(
            || crate::palw_economic_safety_v1::palw_exec_quantum_maturity_daa_v1(b.params.window_challenge(), b.params.window_court()),
            |e| e.maturity_daa,
        )
    });
    let rules = NativeFactRulesV1 {
        state: &b.state,
        params: b.params,
        canonical_work_daa: b.extras.canonical_work_daa,
        quantum_maturity_daa: maturity,
        claims_with_open_disputes: &empty_set,
    };
    let Some(fact) = native_legacy_priced_source_v1(&rules, claim, quantum, (ctx.daa_score, ctx.blue_score)) else { return Ok(None) };
    let material_until_daa =
        if complete { crate::palw_native_settlement_v1::native_effective_retention_v1(&b.state, b.params, &id, claim) } else { None };
    let _lookup = crate::palw_native_resource_v1::reserve_native_requests_v1(128).map_err(PalwStateV2Error::NativeResource)?;
    if b.state
        .kernel_route
        .as_ref()
        .is_some_and(|r| r.aux.contains_key(&(LEGACY_PENDING_FACT_TABLE_V1, key(fact.identity, id).to_vec())))
    {
        return Ok(None);
    }
    let approval = super::palw_claim_local_rewards_v1::cert_bytes(b.state.kernel_route.as_ref(), id).unwrap_or(&[]);
    let bytes = claim
        .native_heap_v1("legacy pending claim")
        .bytes()
        .ok_or_else(|| bad("legacy pending claim resource quote"))?
        .checked_add(approval.len() as u64)
        .and_then(|n| {
            n.checked_add((std::mem::size_of::<NativeRetainedV1<LegacyPendingFactV1>>() + std::mem::size_of::<[usize; 3]>()) as u64)
        })
        .ok_or_else(|| bad("legacy pending resource overflow"))?;
    let lease = crate::palw_native_resource_v1::reserve_native_requests_v1(bytes).map_err(PalwStateV2Error::NativeResource)?;
    Ok(Some(NativeRetainedV1::from_owner_reserved_v1(
        LegacyPendingFactV1 {
            version: 1,
            ruleset: b.extras.native_fact_ruleset.ok_or_else(|| bad("legacy capture actual network ruleset absent"))?,
            claim_id: id,
            claim: claim.clone(),
            quantum,
            fact,
            approval: approval.to_vec(),
            source_point: *ctx,
            canonical_work_daa: b.extras.canonical_work_daa,
            quantum_maturity_daa: if quantum.is_some() { maturity } else { 0 },
            material_until_daa,
            retired_at: None,
            source_parent_root: parent.state_root(),
        },
        lease,
    )))
}

pub(super) fn capture_legacy_sources_v1(
    b: &mut TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    if !enabled(b, ctx) {
        return Ok(());
    }
    let count = b.entries.len();
    for index in 0..count {
        // All captured claims are borrowed directly from the actual executed
        // journal. No NativeDeltaEvidence clone or local published DTO is used.
        let Some(PalwDeltaEntryV2::Claim { key: id, new: Some(claim), .. }) = b.entries.get(index) else { continue };
        let id = *id;
        match &claim.source {
            PalwClaimSourceV2::Attempt => {
                if let Some(row) = capture(b, parent, ctx, id, claim, None)? {
                    b.write_kernel_row_encoded_v1(LEGACY_PENDING_FACT_TABLE_V1, &key(row.fact.identity, id), Some(&*row))?;
                }
            }
            PalwClaimSourceV2::FreePrompt { spent, .. } => {
                // Reborrow after each write; journal additions never change the
                // original entry at `index` or its ordered spent-set inventory.
                let _ = spent;
                let mut cursor = None;
                loop {
                    let PalwDeltaEntryV2::Claim { old, new: Some(claim), .. } = &b.entries[index] else { unreachable!() };
                    let PalwClaimSourceV2::FreePrompt { spent, .. } = &claim.source else { unreachable!() };
                    use std::ops::Bound::{Excluded, Unbounded};
                    let next = match cursor {
                        None => spent.iter().next(),
                        Some(previous) => spent.range((Excluded(previous), Unbounded)).next(),
                    };
                    let Some(&quantum) = next else { break };
                    cursor = Some(quantum);
                    if old
                        .as_ref()
                        .is_some_and(|c| matches!(&c.source,PalwClaimSourceV2::FreePrompt{spent,..} if spent.contains(&quantum)))
                    {
                        continue;
                    }
                    if let Some(row) = capture(b, parent, ctx, id, claim, Some(quantum))? {
                        b.write_kernel_row_encoded_v1(LEGACY_PENDING_FACT_TABLE_V1, &key(row.fact.identity, id), Some(&*row))?;
                    }
                }
            }
        }
    }
    Ok(())
}

pub(super) fn promote_legacy_sources_v1(b: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    if !enabled(b, ctx) {
        return Ok(());
    }
    // The actual fold has written this point's frame/checkpoint but finalizes
    // LastPoint only once at its end. Historical frame validation must see the
    // current executed point while this read runs, including on the first block
    // that closes a dependency. Restore the parent marker on every outcome.
    let saved = b.state.last_point;
    b.state.last_point = Some(*ctx);
    let result = promote_legacy_sources_inner_v1(b, ctx);
    b.state.last_point = saved;
    result
}
fn promote_legacy_sources_inner_v1(b: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    // Two bounded search keys plus the decoded source are reserved before the
    // BTree range search. No source inventory or spent-set deep clone is built.
    let _keys = crate::palw_native_resource_v1::reserve_native_requests_v1(256).map_err(PalwStateV2Error::NativeResource)?;
    let mut cursor = None::<[u8; 128]>;
    loop {
        let next = {
            let Some(route) = &b.state.kernel_route else { break };
            use std::ops::Bound::{Excluded, Included};
            let lower = (LEGACY_PENDING_FACT_TABLE_V1, cursor.map_or_else(Vec::new, |k| k.to_vec()));
            let upper = (LEGACY_PENDING_FACT_TABLE_V1 + 1, Vec::new());
            let bound = if cursor.is_some() { Excluded(lower) } else { Included(lower) };
            match route.aux.range((bound, Excluded(upper))).next() {
                None => None,
                Some(((_, k), raw)) => {
                    let k: [u8; 128] = k.as_slice().try_into().map_err(|_| bad("legacy pending fact key width"))?;
                    Some((k, decode(raw)?))
                }
            }
        };
        let Some((k, mut row)) = next else { break };
        cursor = Some(k);
        if row.version != 1
            || k != key(row.fact.identity, row.claim_id)
            || row.fact.work == 0
            || row.source_point.daa_score > ctx.daa_score
            || row.source_point.blue_score > ctx.blue_score
            || row.claim.work_id.is_none()
            || !matches!(row.claim.phase, PalwClaimPhaseV2::Final { .. })
            || row.fact.class != row.claim.class_id
            || row.fact.anchor != row.claim.accepted_block
        {
            return Err(bad("legacy pending fact source scope"));
        }
        // Preserve the actual selected approval before the claim/certification
        // retires. Its bytes stay bound to the source manifest after retirement.
        if let Some(live) = b.state.claims.get(&row.claim_id).filter(|c| c.accepted_block == row.claim.accepted_block) {
            if matches!(live.phase, PalwClaimPhaseV2::Voided { .. }) {
                b.write_kernel_row_encoded_v1::<LegacyPendingFactV1>(LEGACY_PENDING_FACT_TABLE_V1, &k, None)?;
                continue;
            }
            let (complete, _exec_lease) = super::palw_claim_local_rewards_v1::prepare_exec_reader(&b.state, row.claim_id)?;
            if !complete {
                continue;
            }
            let current_material =
                crate::palw_native_settlement_v1::native_effective_retention_v1(&b.state, b.params, &row.claim_id, live)
                    .ok_or_else(|| bad("legacy pending current material horizon unavailable"))?;
            if row.material_until_daa.is_none_or(|old| current_material > old) {
                let (mut value, lease) = row.into_parts_v1();
                value.material_until_daa = Some(current_material);
                row = NativeRetainedV1::from_owner_reserved_v1(value, lease);
                b.write_kernel_row_encoded_v1(LEGACY_PENDING_FACT_TABLE_V1, &k, Some(&*row))?;
            }
            // Retain the actual terminal negative verdict too: it discards a
            // pending candidate rather than leaving failed approval forever.
            // Recording these bytes never authorizes positive credit.
            if let Some(approval) = super::palw_claim_local_rewards_v1::cert_bytes(b.state.kernel_route.as_ref(), row.claim_id)
                && row.approval.as_slice() != approval
            {
                let requests = crate::palw_native_resource_v1::native_value_requests_v1(&*row)
                    .map_err(PalwStateV2Error::NativeResource)?
                    .checked_add(approval.len() as u64)
                    .ok_or_else(|| bad("legacy approval owner resource overflow"))?;
                let lease =
                    crate::palw_native_resource_v1::reserve_native_requests_v1(requests).map_err(PalwStateV2Error::NativeResource)?;
                let mut replacement = (*row).clone();
                replacement.approval = approval.to_vec();
                let replacement = NativeRetainedV1::from_owner_reserved_v1(replacement, lease);
                b.write_kernel_row_encoded_v1(LEGACY_PENDING_FACT_TABLE_V1, &k, Some(&*replacement))?;
                row = replacement;
            }
        }
        let Some(work) = row.claim.work_id else { return Err(bad("legacy pending canonical work absent")) };
        let expected_identity = match (&row.claim.source, row.quantum) {
            (PalwClaimSourceV2::Attempt, None) => work,
            (PalwClaimSourceV2::FreePrompt { spent, .. }, Some(q)) if spent.contains(&q) => {
                crate::palw_native_settlement_v1::fp_slice_identity(&work, q)
            }
            _ => return Err(bad("legacy pending quantum does not name a real spent source")),
        };
        if expected_identity != row.fact.identity {
            return Err(bad("legacy pending canonical work identity"));
        }
        let Some(material_until) = row.material_until_daa else { continue };
        let final_daa = match row.claim.phase {
            PalwClaimPhaseV2::Final { final_daa } => final_daa,
            _ => return Err(bad("pending source lost Final")),
        };
        let matured_daa = material_until
            .max(final_daa.saturating_add(b.params.claim_retirement_daa()))
            .max(if row.quantum.is_some() { row.fact.accepted_daa.saturating_add(row.quantum_maturity_daa) } else { 0 });
        if ctx.daa_score < matured_daa || row.approval.is_empty() {
            continue;
        }
        match super::palw_claim_local_rewards_v1::approval(b.params, row.claim_id, &row.claim, Some(&row.approval))? {
            super::palw_claim_local_rewards_v1::RewardReadinessV1::Ready => {}
            super::palw_claim_local_rewards_v1::RewardReadinessV1::Denied => {
                b.write_kernel_row_encoded_v1::<LegacyPendingFactV1>(LEGACY_PENDING_FACT_TABLE_V1, &k, None)?;
                continue;
            }
            _ => continue,
        }
        let decision = if b.params.claim_local_settlement_active_at(ctx.daa_score) {
            super::palw_claim_local_rewards_v1::palw_claim_local_recorded_settlement_v1(
                &b.state,
                b.params,
                row.claim_id,
                &row.claim,
                ctx.daa_score,
            )?
        } else {
            // Preserve the original global lifecycle boundary below stage 2.
            let open_from = crate::palw_native_settlement_v1::native_open_from_v1(
                b.state.claims.iter().map(|(id, c)| {
                    (
                        c.accepted_blue_score,
                        c.phase.clone(),
                        crate::palw_native_settlement_v1::native_effective_retention_v1(&b.state, b.params, id, c).unwrap_or(u64::MAX),
                    )
                }),
                b.state
                    .da_sessions
                    .keys()
                    .map(|(id, _)| b.state.claims.get(id).map(|c| c.accepted_blue_score))
                    .chain(b.state.court_sessions.values().map(|s| b.state.claims.get(&s.claim).map(|c| c.accepted_blue_score))),
                ctx.daa_score,
            );
            if row.fact.accepted_blue < open_from
                && (row.retired_at.is_some()
                    || b.state.claims.get(&row.claim_id).is_some_and(|c| {
                        c.accepted_block == row.claim.accepted_block && matches!(c.phase, PalwClaimPhaseV2::Final { .. })
                    }))
            {
                PalwClaimLocalSettlementV1::Settled
            } else {
                PalwClaimLocalSettlementV1::Held { until_daa: u64::MAX }
            }
        };
        match decision {
            PalwClaimLocalSettlementV1::Denied => {
                b.write_kernel_row_encoded_v1::<LegacyPendingFactV1>(LEGACY_PENDING_FACT_TABLE_V1, &k, None)?;
            }
            PalwClaimLocalSettlementV1::Settled => {
                use crate::palw_native_closed_facts_v1::{ClosedFactFamilyV1, NativeClosedFactOriginV1};
                // Serializing the source manifest is admitted before allocating
                // its bytes, and binds the exact source/approval/spend clocks.
                let bytes_len = borsh::object_length(&source_manifest_v1(&row)).map_err(bad)? as u64;
                let _manifest =
                    crate::palw_native_resource_v1::reserve_native_requests_v1(bytes_len).map_err(PalwStateV2Error::NativeResource)?;
                let manifest = borsh::to_vec(&source_manifest_v1(&row)).map_err(bad)?;
                let origin = NativeClosedFactOriginV1 {
                    family: if row.quantum.is_some() { ClosedFactFamilyV1::LegacyFp } else { ClosedFactFamilyV1::LegacyAttempt },
                    subject: row.claim_id,
                    work,
                    ruleset: row.ruleset,
                    canonical_work_daa: row.canonical_work_daa,
                    quantum: row.quantum,
                    approval: Some(crate::palw_claim_local_v1::palw_liability_material_id_v1(&row.approval)),
                    source_manifest: crate::palw_claim_local_v1::palw_liability_material_id_v1(&manifest),
                    once_obligation: None,
                    source_point: row.source_point,
                    source_state_root: row.source_parent_root,
                    terminal_daa: match row.claim.phase {
                        PalwClaimPhaseV2::Final { final_daa } => final_daa,
                        _ => unreachable!(),
                    },
                    material_until_daa: material_until,
                    quantum_maturity_daa: row.quantum_maturity_daa,
                    retirement_daa: b.params.claim_retirement_daa(),
                };
                super::palw_native_closed_facts_fold_v1::append_native_closed_fact_v1(
                    b,
                    ctx,
                    row.fact.mature_v1(matured_daa),
                    origin,
                )?;
                // The owning closed decision is now in canonical195 and all
                // actual source duties have ended. Release this heavy source;
                // historical correctness is not inferred from a producer DTO.
                b.write_kernel_row_encoded_v1::<LegacyPendingFactV1>(LEGACY_PENDING_FACT_TABLE_V1, &k, None)?;
            }
            _ => {} // Held dependency can become ready later; retain its source.
        }
    }
    Ok(())
}

fn source_manifest_v1(row: &LegacyPendingFactV1) -> impl BorshSerialize + '_ {
    (
        row.version,
        row.ruleset,
        row.claim_id,
        &row.claim,
        row.quantum,
        &row.fact,
        &row.approval,
        row.source_point,
        row.canonical_work_daa,
        row.quantum_maturity_daa,
        row.material_until_daa,
        row.retired_at,
        row.source_parent_root,
    )
}

/// Called only after the ordinary deadline path actually removed this exact
/// terminal claim. This records the event, never infers retirement from absence.
pub(super) fn note_legacy_retirement_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    id: Hash64,
    claim: &PalwClaimStateV2,
    material_horizon: Option<u64>,
) -> Result<(), PalwStateV2Error> {
    if !enabled(b, ctx) || b.state.claims.contains_key(&id) {
        return Ok(());
    }
    let _range = crate::palw_native_resource_v1::reserve_native_requests_v1(512).map_err(PalwStateV2Error::NativeResource)?;
    let lower = (LEGACY_PENDING_FACT_TABLE_V1, key(Hash64::default(), id).to_vec());
    let upper = (LEGACY_PENDING_FACT_TABLE_V1, key(Hash64::from_bytes([255; 64]), id).to_vec());
    let count = b.state.kernel_route.as_ref().map_or(0, |r| r.aux.range(lower.clone()..=upper.clone()).count());
    let _outer = crate::palw_native_resource_v1::reserve_native_requests_v1(
        (std::mem::size_of::<NativeRetainedV1<LegacyPendingFactV1>>() as u64)
            .checked_mul(count as u64)
            .ok_or_else(|| bad("legacy retirement source inventory overflow"))?,
    )
    .map_err(PalwStateV2Error::NativeResource)?;
    let mut rows = Vec::with_capacity(count);
    if let Some(route) = &b.state.kernel_route {
        for ((_, k), raw) in route.aux.range(lower..=upper) {
            if k.len() != 128 {
                return Err(bad("legacy retirement source key width"));
            }
            rows.push(decode(raw)?);
        }
    }
    for row in rows {
        if row.claim.accepted_block != claim.accepted_block {
            continue;
        }
        let copied = NativeRetainedV1::try_from_clone_v1(claim).map_err(PalwStateV2Error::NativeResource)?;
        let (claim, _claim_lease) = copied.into_parts_v1();
        let (mut row, _row_lease) = row.into_parts_v1();
        row.claim = claim;
        row.retired_at = Some(*ctx);
        row.material_until_daa = match (row.material_until_daa, material_horizon) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
        if !matches!(row.claim.phase, PalwClaimPhaseV2::Final { .. }) {
            b.write_kernel_row_encoded_v1::<LegacyPendingFactV1>(LEGACY_PENDING_FACT_TABLE_V1, &key(row.fact.identity, id), None)?;
            continue;
        }
        b.write_kernel_row_encoded_v1(LEGACY_PENDING_FACT_TABLE_V1, &key(row.fact.identity, id), Some(&row))?;
    }
    Ok(())
}

/// Cold-load/IBD validation uses the real global network parameters, including
/// the immutable canonical pricing fence and quantum clock. A stored timestamp
/// or work quantity cannot choose a cheaper network policy.
pub fn palw_validate_legacy_pending_fact_context_v1(
    state: &PalwChainStateV2,
    global: &crate::config::params::Params,
) -> Result<(), PalwStateV2Error> {
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &global.palw_consensus_mode else {
        if state.kernel_route.as_ref().is_some_and(|r| r.aux.keys().any(|(t, _)| *t == LEGACY_PENDING_FACT_TABLE_V1)) {
            return Err(bad("legacy source archive lacks actual state rules"));
        }
        return Ok(());
    };
    let params = &bundle.state;
    let Some(route) = &state.kernel_route else { return Ok(()) };
    for ((table, k), raw) in &route.aux {
        if *table != LEGACY_PENDING_FACT_TABLE_V1 {
            continue;
        }
        let row = decode(raw)?;
        let last = state.last_point.ok_or_else(|| bad("legacy source archive lacks executed point"))?;
        if row.version != 1
            || k.as_slice() != key(row.fact.identity, row.claim_id)
            || row.ruleset != global.consensus_params_id()
            || row.canonical_work_daa != global.palw_canonical_work_daa()
            || row.fact.class == params.base_class_id()
            || row.fact.work == 0
            || row.source_parent_root == Hash64::default()
            || row.source_point.blue_score > last.blue_score
            || row.source_point.daa_score > last.daa_score
            || global.palw_dns_retirement.is_none_or(|r| !r.activation.is_active(row.source_point.daa_score))
        {
            return Err(bad("legacy source archive differs from actual network/parent/clock context"));
        }
        let Some(work) = row.claim.work_id else { return Err(bad("legacy source lacks actual work identity")) };
        let PalwClaimPhaseV2::Final { .. } = row.claim.phase else { return Err(bad("legacy source was not Final")) };
        let (identity, weight, maturity) = match (&row.claim.source, row.quantum) {
            (PalwClaimSourceV2::Attempt, None) => {
                (work, state.palw_claim_canonical_weight_v1(&row.claim, global.palw_canonical_work_daa()).unwrap_or(0), 0)
            }
            (PalwClaimSourceV2::FreePrompt { quanta, spent }, Some(q)) if spent.contains(&q) => {
                let pricing = PalwFpPricingV1::of(params, global.palw_canonical_work_daa());
                if !pricing.prices_in_compute(&row.claim) {
                    return Err(bad("legacy source FP pricing fence"));
                }
                (
                    crate::palw_native_settlement_v1::fp_slice_identity(&work, q),
                    palw_fp_spend_weight_v1(state, &row.claim, *quanta, &pricing),
                    global.palw_exec_quantum_maturity_v1(),
                )
            }
            _ => return Err(bad("legacy source actual spent quantum")),
        };
        if row.fact.identity != identity
            || row.fact.work != weight
            || row.quantum_maturity_daa != maturity
            || row.material_until_daa.is_some_and(|until| until < row.claim.trace_retention_daa)
            || row.fact.anchor != row.claim.accepted_block
            || row.fact.anchor_blue != row.claim.accepted_blue_score
            || row.fact.anchor_daa != row.claim.accepted_daa
            || row.fact.class != row.claim.class_id
        {
            return Err(bad("legacy source canonical pricing/maturity decomposition"));
        }
        if !row.approval.is_empty() {
            let cert = crate::palw_native_resource_v1::retain_native_value_decode_classified_v1::<
                crate::palw_security_certification_v1::PalwClaimCertificationRowV1,
                std::io::Error,
            >(&row.approval, || borsh::from_slice(&row.approval))
            .map_err(|e| match e {
                NativeDecodeErrorV1::Resource(r) => PalwStateV2Error::NativeResource(r),
                NativeDecodeErrorV1::Decode(e) => bad(e),
            })?;
            if cert.claim != row.claim_id.as_bytes()
                || cert.class_id != row.claim.class_id.as_bytes()
                || cert.producer_principal != row.fact.operator.as_bytes()
            {
                return Err(bad("legacy source selected approval identity"));
            }
        }
    }
    Ok(())
}

pub(super) fn legacy_retiring_horizon_v1(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    id: Hash64,
    claim: &PalwClaimStateV2,
) -> Result<Option<u64>, PalwStateV2Error> {
    if !enabled(b, ctx) {
        return Ok(None);
    }
    let (complete, _lease) = super::palw_claim_local_rewards_v1::prepare_exec_reader(&b.state, id)?;
    Ok(if complete { crate::palw_native_settlement_v1::native_effective_retention_v1(&b.state, b.params, &id, claim) } else { None })
}
