//! A separate source contract: ordinary race seals are not inputs. Native/SDK callers derive these facts from verified rows.
//! Algorithm support is not approval or a participation/soundness/fee guarantee.
use crate::beacon::{VerifiedWorkBeaconV1, WorkBeaconV1};
use crate::hash::{Digest, object_id};
use crate::policy::PolicyRefusalV1;
use crate::sealed::{collect_sealed_inner, seal_window_end_v3};
use crate::{BeaconContextV1, SealRevealV3, SealedBeaconStateV3, SealedSourceV3, SourceFateV3};
use borsh::{BorshDeserialize, BorshSerialize};

const INITIAL: &[u8] = b"MISAKA/PALW/WORK-BEACON/OPT-IN/V4";
const ITEM: &[u8] = b"MISAKA/PALW/WORK-BEACON/OPT-IN/ITEM/V4";
const MIX: &[u8] = b"MISAKA/PALW/WORK-BEACON/OPT-IN/MIX/V4";

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct OptInContractV4 {
    pub context_id: Digest,
    pub terms_id: Digest,
    pub source_final_by: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct OptInSourceV4 {
    pub context_id: Digest,
    pub terms_id: Digest,
    pub opted_in_position: u64,
    pub forfeited_position: Option<u64>,
    pub source: SealedSourceV3,
}

pub fn collect_opt_in_work_beacon_v4(
    ctx: &BeaconContextV1,
    contract: &OptInContractV4,
    sources: &[OptInSourceV4],
    tip: u64,
) -> Result<SealedBeaconStateV3, PolicyRefusalV1> {
    ctx.policy.validate()?;
    if !ctx.policy.is_opt_in_source() || ctx.policy.is_paid_opt_in_source() {
        return Err(PolicyRefusalV1::WrongCollector("opt-in v4 requires its own policy"));
    }
    let facts = opt_in_source_facts(ctx, contract, sources, tip)?;
    let initial = object_id(INITIAL, &(ctx.id(), contract));
    collect_sealed_inner(ctx, &facts, tip, ITEM, MIX, initial)
}

pub(crate) fn opt_in_source_facts(
    ctx: &BeaconContextV1,
    contract: &OptInContractV4,
    sources: &[OptInSourceV4],
    tip: u64,
) -> Result<Vec<SealedSourceV3>, PolicyRefusalV1> {
    let start = ctx
        .commitment_position
        .checked_add(ctx.policy.anchor_delay_slots)
        .ok_or(PolicyRefusalV1::Missing("representable opt-in start"))?;
    let seal_end =
        start.checked_add(ctx.policy.beacon_window_slots).ok_or(PolicyRefusalV1::Missing("representable opt-in seal window"))?;
    let reveal_end =
        seal_end.checked_add(ctx.policy.beacon_window_slots).ok_or(PolicyRefusalV1::Missing("representable opt-in reveal window"))?;
    if contract.context_id != ctx.id()
        || contract.source_final_by < reveal_end
        || contract.source_final_by == u64::MAX
        || contract.source_final_by.checked_add(ctx.policy.settlement_depth_d).is_none()
    {
        return Err(PolicyRefusalV1::Missing("bound finite opt-in contract"));
    }
    let mut facts = Vec::new();
    for s in sources.iter().filter(|s| s.context_id == contract.context_id && s.opted_in_position <= tip) {
        if s.terms_id != contract.terms_id
            || s.opted_in_position < s.source.seal_position
            || s.opted_in_position < start
            || s.opted_in_position >= seal_end
            || s.source.seal_position < start
            || s.source.seal_position >= seal_end
        {
            return Err(PolicyRefusalV1::Missing("opt-in source contract binding"));
        }
        let mut fact = s.source.clone();
        let late_final = fact.reveal.as_ref().is_some_and(|r| {
            matches!(&r.fate,
            SourceFateV3::Final(ev) if ev.settlement_position > contract.source_final_by)
        });
        let no_timely_final = tip > contract.source_final_by
            && !fact
                .reveal
                .as_ref()
                .is_some_and(|r| matches!(&r.fate, SourceFateV3::Final(ev) if ev.settlement_position <= contract.source_final_by));
        if s.forfeited_position.is_some_and(|at| at <= tip) || late_final || no_timely_final {
            // Even an eventual ordinary Final cannot resurrect an opted-in contribution that forfeited its contract.
            fact.reveal = Some(SealRevealV3 {
                reveal_position: fact.reveal.as_ref().map_or(seal_window_end_v3(ctx), |r| r.reveal_position),
                salt: fact.reveal.as_ref().map_or([0; 64], |r| r.salt),
                fate: SourceFateV3::Failed,
            });
        }
        facts.push(fact);
    }
    Ok(facts)
}

pub fn verify_opt_in_work_beacon_v4(
    ctx: &BeaconContextV1,
    contract: &OptInContractV4,
    sources: &[OptInSourceV4],
    tip: u64,
    claimed: &WorkBeaconV1,
) -> Result<VerifiedWorkBeaconV1, PolicyRefusalV1> {
    match collect_opt_in_work_beacon_v4(ctx, contract, sources, tip)? {
        SealedBeaconStateV3::Locked(beacon) if beacon.beacon() == claimed => Ok(beacon),
        _ => Err(PolicyRefusalV1::Missing("re-derived locked opt-in beacon")),
    }
}
