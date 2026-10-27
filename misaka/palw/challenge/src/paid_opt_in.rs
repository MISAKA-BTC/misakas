//! Mandatory prepaid source fees use a distinct policy and accumulator.
//! Native/SDK adapters authenticate both complete source and fee inventories;
//! these pure facts alone are not proof of funding, net profit or source independence.
use std::collections::BTreeMap;

use crate::beacon::{VerifiedWorkBeaconV1, WorkBeaconV1};
use crate::hash::{Digest, object_id};
use crate::opt_in::opt_in_source_facts;
use crate::policy::PolicyRefusalV1;
use crate::sealed::collect_sealed_inner;
use crate::{BeaconContextV1, OptInContractV4, OptInSourceV4, SealedBeaconStateV3};
use borsh::{BorshDeserialize, BorshSerialize};

const INITIAL: &[u8] = b"MISAKA/PALW/WORK-BEACON/PAID-OPT-IN/V5";
const ITEM: &[u8] = b"MISAKA/PALW/WORK-BEACON/PAID-OPT-IN/ITEM/V5";
const MIX: &[u8] = b"MISAKA/PALW/WORK-BEACON/PAID-OPT-IN/MIX/V5";

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PaidOptInContractV5 {
    pub source: OptInContractV4,
    pub fee_terms_id: Digest,
    pub minimum_fee: u64,
    pub fee_per_source: u64,
    pub max_sources: u32,
    pub funded_position: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PaidSourcePromiseV5 {
    pub context_id: Digest,
    pub producer: Digest,
    pub fee_terms_id: Digest,
    pub accepted_position: u64,
    pub fee: u64,
}

pub fn collect_paid_opt_in_work_beacon_v5(
    ctx: &BeaconContextV1,
    contract: &PaidOptInContractV5,
    sources: &[OptInSourceV4],
    promises: &[PaidSourcePromiseV5],
    tip: u64,
) -> Result<SealedBeaconStateV3, PolicyRefusalV1> {
    ctx.policy.validate()?;
    if !ctx.policy.is_paid_opt_in_source() {
        return Err(PolicyRefusalV1::WrongCollector("paid opt-in v5 requires its own policy"));
    }
    let seal_end = ctx
        .commitment_position
        .checked_add(ctx.policy.anchor_delay_slots)
        .and_then(|p| p.checked_add(ctx.policy.beacon_window_slots))
        .ok_or(PolicyRefusalV1::Missing("representable funded seal window"))?;
    if contract.minimum_fee == 0
        || contract.fee_per_source < contract.minimum_fee
        || contract.max_sources < ctx.policy.work_count_k
        || contract.max_sources > 65_536
        || contract.fee_per_source.checked_mul(u64::from(contract.max_sources)).is_none()
        || contract.fee_terms_id == [0; 64]
        || contract.funded_position < ctx.commitment_position
        || contract.funded_position >= seal_end
    {
        return Err(PolicyRefusalV1::Missing("finite prepaid source fee floor/capacity/window"));
    }
    let mut fees = BTreeMap::new();
    for promise in promises.iter().filter(|p| p.context_id == contract.source.context_id && p.accepted_position <= tip) {
        if promise.fee_terms_id != contract.fee_terms_id
            || promise.fee != contract.fee_per_source
            || promise.accepted_position < contract.funded_position
            || promise.accepted_position >= seal_end
            || fees.insert(promise.producer, promise).is_some()
        {
            return Err(PolicyRefusalV1::Missing("unique signed prepaid source fee promise"));
        }
    }
    let mut admitted = 0u32;
    for source in sources.iter().filter(|s| s.context_id == contract.source.context_id && s.opted_in_position <= tip) {
        let producer = source.source.attribution.producer_id;
        let promise = fees.remove(&producer).ok_or(PolicyRefusalV1::Missing("mandatory prepaid source fee promise"))?;
        if promise.accepted_position != source.opted_in_position {
            return Err(PolicyRefusalV1::Missing("source and fee admitted in one transition"));
        }
        admitted = admitted.checked_add(1).ok_or(PolicyRefusalV1::Missing("representable paid source count"))?;
    }
    if !fees.is_empty() || admitted > contract.max_sources {
        return Err(PolicyRefusalV1::Missing("complete bounded paid source inventory"));
    }
    let facts = opt_in_source_facts(ctx, &contract.source, sources, tip)?;
    let initial = object_id(INITIAL, &(ctx.id(), contract));
    collect_sealed_inner(ctx, &facts, tip, ITEM, MIX, initial)
}

const SEAL_SEED: &[u8] = b"MISAKA/PALW/PAID-CLAIM/SEAL-REVEAL-SEED/V6";

/// **The paid lock's seal-reveal seed** (user decision 2026-10-17 on F1; dormant behind the paid-claim fence): the checker draw of a
/// paid target is seeded by the commitments of claims SEALED AFTER the target's commitment — each source's content is fixed at its
/// seal and opened only after the seal window closes — and the sources need NOT be Final (an approved Final is required only on the
/// reward/credit side). `Ok(Some(seed))` once the reveal window closed with at least `work_count_k` opted-in, prepaid sources all
/// revealed in time; `Ok(None)` before that, or for good when a mixed source withheld its reveal or forfeited (a veto: the target
/// takes no lock and times out — it never gets a redraw).
///
/// **No grinding.** (1) The target commits first: every source seal is accepted at or after `commitment + anchor_delay`, so nothing
/// the target fixed can depend on them, and the seed mixes the target's context. (2) A seal binds a salted claim commitment (content
/// and salt fixed at seal); the salt is public only from the reveal, after the seal window closes, so no sealer knows the mix when it
/// seals. (3) The source set is closed at the seal window's end (the prepaid inventory is checked complete, and the route refuses the
/// target's own producer, its own base job, and mutual producer pairs as sources). (4) The only lever left is the last revealer's
/// abort: withholding vetoes the whole attempt (deposit forfeited), it never selects among outcomes, so a coalition can deny a lock but
/// cannot choose the checkers. Honest-fraction premise: at least one source in the mix is honest (unknown to the others at seal).
pub fn collect_paid_seal_reveal_seed_v6(
    ctx: &BeaconContextV1,
    contract: &PaidOptInContractV5,
    sources: &[OptInSourceV4],
    promises: &[PaidSourcePromiseV5],
    tip: u64,
) -> Result<Option<Digest>, PolicyRefusalV1> {
    // The same complete prepaid inventory as v5 (this call also validates the policy and the fee bindings); its Final-based verdict
    // is not read.
    collect_paid_opt_in_work_beacon_v5(ctx, contract, sources, promises, tip)?;
    let seal_end = crate::sealed::seal_window_end_v3(ctx);
    let reveal_end = crate::sealed::reveal_window_end_v3(ctx);
    if tip < reveal_end {
        return Ok(None);
    }
    let mut mixed: Vec<(Digest, u64, Digest)> = Vec::new();
    for s in sources.iter().filter(|s| s.context_id == contract.source.context_id && s.opted_in_position <= tip) {
        if s.source.seal_position < ctx.commitment_position.saturating_add(ctx.policy.anchor_delay_slots)
            || s.source.seal_position >= seal_end
        {
            continue;
        }
        if s.forfeited_position.is_some_and(|at| at <= tip) {
            return Ok(None);
        }
        let Some(reveal) = s.source.reveal.as_ref().filter(|r| r.reveal_position < reveal_end) else {
            return Ok(None);
        };
        mixed.push((s.source.seal, s.source.seal_position, reveal.salt));
    }
    if (mixed.len() as u32) < ctx.policy.work_count_k {
        return Ok(None);
    }
    mixed.sort();
    Ok(Some(object_id(SEAL_SEED, &(ctx.id(), contract, &mixed))))
}

pub fn verify_paid_opt_in_work_beacon_v5(
    ctx: &BeaconContextV1,
    contract: &PaidOptInContractV5,
    sources: &[OptInSourceV4],
    promises: &[PaidSourcePromiseV5],
    tip: u64,
    claimed: &WorkBeaconV1,
) -> Result<VerifiedWorkBeaconV1, PolicyRefusalV1> {
    match collect_paid_opt_in_work_beacon_v5(ctx, contract, sources, promises, tip)? {
        SealedBeaconStateV3::Locked(beacon) if beacon.beacon() == claimed => Ok(beacon),
        _ => Err(PolicyRefusalV1::Missing("re-derived locked paid source beacon")),
    }
}

const OPT_IN_SEAL_INITIAL: &[u8] = b"MISAKA/PALW/WORK-BEACON/OPT-IN-SEAL-REVEAL/V6";
const OPT_IN_SEAL_MIX: &[u8] = b"MISAKA/PALW/WORK-BEACON/OPT-IN-SEAL-REVEAL/MIX/V6";

/// **The v4 / v5 opt-in beacon read by seal-reveal** (user decision 2026-10-18, ADR-0185 note; dormant with the paid-claim bootstrap
/// policy): the sources' content is the salted commitments of opted-in claims SEALED AFTER the target's commitment and opened after
/// the seal window — the same method as the paid lock's F1 seed (`collect_paid_seal_reveal_seed_v6`). No source need be Final: an
/// approved paid source's own lifetime outlasts any `final_by` of the same policy by construction. `paid` = the v5 fee inventory
/// (validated complete exactly as v5 does); `None` = v4. A withheld or forfeited mixed source vetoes; fewer than `k` sealed sources is
/// UNAVAILABLE. Honest-fraction premise as F1: at least one mixed source is honest (unknown to the others at seal).
pub fn collect_opt_in_seal_reveal_beacon_v6(
    ctx: &BeaconContextV1,
    contract: &OptInContractV4,
    sources: &[OptInSourceV4],
    paid: Option<(&PaidOptInContractV5, &[PaidSourcePromiseV5])>,
    tip: u64,
) -> Result<SealedBeaconStateV3, PolicyRefusalV1> {
    ctx.policy.validate()?;
    if let Some((paid_contract, promises)) = paid {
        // The complete prepaid inventory (policy, fee bindings); its Final-based verdict is not read.
        collect_paid_opt_in_work_beacon_v5(ctx, paid_contract, sources, promises, tip)?;
    } else if !ctx.policy.is_opt_in_source() || ctx.policy.is_paid_opt_in_source() {
        return Err(PolicyRefusalV1::WrongCollector("opt-in v4 requires its own policy"));
    }
    let facts = opt_in_source_facts(ctx, contract, sources, tip)?;
    let need = ctx.policy.work_count_k;
    let start = ctx.commitment_position.saturating_add(ctx.policy.anchor_delay_slots);
    let seal_end = crate::sealed::seal_window_end_v3(ctx);
    let reveal_end = crate::sealed::reveal_window_end_v3(ctx);
    let mixed: Vec<&crate::SealedSourceV3> = facts.iter().filter(|s| s.seal_position >= start && s.seal_position < seal_end).collect();
    let n = mixed.len() as u32;
    if tip < seal_end {
        return Ok(SealedBeaconStateV3::Sealing { sealed: n, need });
    }
    if n < need {
        return Ok(SealedBeaconStateV3::Unavailable { mixed: n, need });
    }
    let in_time =
        |s: &crate::SealedSourceV3| s.reveal.as_ref().filter(|r| r.reveal_position < reveal_end && r.reveal_position <= tip).cloned();
    if tip < reveal_end {
        let revealed = mixed.iter().filter(|s| in_time(s).is_some()).count() as u32;
        return Ok(SealedBeaconStateV3::Revealing { mixed: n, revealed });
    }
    let forfeited =
        sources.iter().filter(|s| s.context_id == contract.context_id && s.forfeited_position.is_some_and(|at| at <= tip)).count()
            as u32;
    let withheld = mixed.iter().filter(|s| in_time(s).is_none()).count() as u32;
    if withheld > 0 || forfeited > 0 {
        return Ok(SealedBeaconStateV3::Vetoed { withheld, failed: forfeited });
    }
    let mut items: Vec<(Digest, u64, Digest, Digest, u64)> = mixed
        .iter()
        .map(|s| {
            let r = in_time(s).expect("checked");
            (s.seal, s.seal_position, r.salt, s.source_profile_id, r.reveal_position)
        })
        .collect();
    items.sort();
    let initial = object_id(OPT_IN_SEAL_INITIAL, &(ctx.id(), contract));
    let output = object_id(OPT_IN_SEAL_MIX, &(initial, &items));
    let beacon_sources: Vec<crate::beacon::BeaconSourceV1> = items
        .iter()
        .enumerate()
        .map(|(i, (seal, _, _, profile, revealed))| crate::beacon::BeaconSourceV1 {
            source_profile_id: *profile,
            canonical_work_id: *seal,
            execution_commitment: *seal,
            accepted_position: *revealed,
            settlement_position: *revealed,
            occurrence_index: i as u32,
        })
        .collect();
    let challenge_anchor = crate::beacon::challenge_anchor_v1(ctx, &beacon_sources);
    Ok(SealedBeaconStateV3::Locked(VerifiedWorkBeaconV1::derived(
        WorkBeaconV1 {
            sources: beacon_sources,
            accumulators: vec![initial, output],
            output,
            challenge_anchor,
            lock_position: reveal_end,
        },
        ctx.id(),
    )))
}
