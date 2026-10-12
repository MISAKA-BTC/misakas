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
