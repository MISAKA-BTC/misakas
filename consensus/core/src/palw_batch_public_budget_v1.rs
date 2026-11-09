//! Ordered check of a reserved public graph against the DAA left after the
//! latest legal first notice.
//!
//! That remainder is [`PALW_PUBLIC_SHARED_BUDGET_AFTER_NOTICE_DAA_V1`] =
//! `DISPUTE_WINDOW_DAA − NOTICE_WINDOW_DAA` (6,880). The slowest legal first
//! notice is the one this budget uses. An earlier notice does not widen it,
//! and the retired localization span (64 retries × acquisition 4,096, admitted
//! as 306,108 DAA) is not a fallback.
//!
//! Slot capacities are summed only as a lower bound the tests can name. Admission
//! uses the dependency order: a slot starts at its material-ready DAA or when
//! every predecessor slot has finished, whichever is later, and recovery is
//! added after the last slot. This is not a measured execution model, it does
//! not read G14 profile estimates, and it does not establish that a public T12
//! claim is ready.

use crate::Hash64;
use crate::palw_court_policy_v1::{DISPUTE_WINDOW_DAA, NOTICE_WINDOW_DAA};
use crate::palw_court_service_v1::{ServiceCost, ServicePacket, ServiceRefusal, ServiceReservations, causal::CausalServicePacketV1};
use crate::palw_court_shared_material_intake_v1::MaterialLedgerV1;
use crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1;
use crate::palw_state_v2::PalwBondKeyV2;
use std::collections::{BTreeMap, BTreeSet};

/// DAA remaining after the latest legal first notice (`a + 3,120`) until the
/// hard deadline (`a + 10,000`).
pub const PALW_PUBLIC_SHARED_BUDGET_AFTER_NOTICE_DAA_V1: u64 = DISPUTE_WINDOW_DAA - NOTICE_WINDOW_DAA;

/// Restart count of the retired public localization contract. Not a budget.
pub const PALW_RETIRED_LOCALIZATION_RETRIES_V1: u64 = 64;
/// Acquisition width of that retired contract. Not a budget.
pub const PALW_RETIRED_LOCALIZATION_ACQUISITION_DAA_V1: u64 = 4_096;
/// Admitted localization span of that contract (the retries times the
/// acquisition width, plus the rest of the old path). Not a budget, and not
/// a fallback when 6,880 is too small.
pub const PALW_RETIRED_LOCALIZATION_PATH_DAA_V1: u64 = 306_108;

/// One submission slot in the reserved public graph, timed from the DAA after
/// the latest legal first notice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicSharedSlotV1 {
    pub id: [u8; 64],
    /// DAA this slot occupies once it is allowed to start.
    pub slot_daa: u64,
    /// Earliest start, in DAA after the latest legal first notice, when this
    /// slot's material is ready. Predecessors can only push the start later.
    pub material_ready_daa: u64,
    pub predecessors: Vec<[u8; 64]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicSharedBudgetRefusalV1 {
    /// The first notice is after `a + NOTICE_WINDOW_DAA`.
    Notice,
    /// Empty, cyclic, duplicated, zero-length, or otherwise not a public graph.
    Graph,
    /// The ordered span, including recovery, does not fit in 6,880 DAA.
    Exceeds { required: u64, allowed: u64 },
}

/// Refuse `required` when it is past the post-notice budget. The retired
/// 306,108 DAA localization span is not consulted.
pub fn check_scheduled_public_span_v1(required_after_notice_daa: u64) -> Result<u64, PublicSharedBudgetRefusalV1> {
    let allowed = PALW_PUBLIC_SHARED_BUDGET_AFTER_NOTICE_DAA_V1;
    if required_after_notice_daa > allowed {
        Err(PublicSharedBudgetRefusalV1::Exceeds { required: required_after_notice_daa, allowed })
    } else {
        Ok(required_after_notice_daa)
    }
}

/// Worst-case check of a reserved public graph.
///
/// `notice_offset_daa` is the first notice's offset from acceptance. Offsets
/// above [`NOTICE_WINDOW_DAA`] are refused. Every legal offset, including an
/// early notice, is checked against the same 6,880 DAA that remain after the
/// slowest legal first notice.
pub fn check_reserved_public_graph_v1(
    notice_offset_daa: u64,
    slots: &[PublicSharedSlotV1],
    recovery_daa: u64,
) -> Result<u64, PublicSharedBudgetRefusalV1> {
    if notice_offset_daa > NOTICE_WINDOW_DAA {
        return Err(PublicSharedBudgetRefusalV1::Notice);
    }
    check_scheduled_public_span_v1(ordered_required_daa(slots, recovery_daa)?)
}

fn ordered_required_daa(slots: &[PublicSharedSlotV1], recovery_daa: u64) -> Result<u64, PublicSharedBudgetRefusalV1> {
    use PublicSharedBudgetRefusalV1::Graph;
    if slots.is_empty() {
        return Err(Graph);
    }
    let mut index = BTreeMap::new();
    for (i, slot) in slots.iter().enumerate() {
        if slot.slot_daa == 0 || index.insert(slot.id, i).is_some() {
            return Err(Graph);
        }
    }
    let n = slots.len();
    let mut indegree = vec![0usize; n];
    let mut children = vec![Vec::<usize>::new(); n];
    for (i, slot) in slots.iter().enumerate() {
        let mut seen = BTreeSet::new();
        for pred in &slot.predecessors {
            let p = *index.get(pred).ok_or(Graph)?;
            if p == i || !seen.insert(p) {
                return Err(Graph);
            }
            indegree[i] += 1;
            children[p].push(i);
        }
    }
    let mut ready = Vec::new();
    for (i, degree) in indegree.iter().enumerate() {
        if *degree == 0 {
            ready.push(i);
        }
    }
    let mut start = vec![0u64; n];
    let mut seen_n = 0usize;
    while let Some(i) = ready.pop() {
        seen_n += 1;
        let mut ready_at = slots[i].material_ready_daa;
        for pred in &slots[i].predecessors {
            let p = index[pred];
            let end = start[p].checked_add(slots[p].slot_daa).ok_or(Graph)?;
            ready_at = ready_at.max(end);
        }
        start[i] = ready_at;
        for child in &children[i] {
            indegree[*child] -= 1;
            if indegree[*child] == 0 {
                ready.push(*child);
            }
        }
    }
    if seen_n != n {
        return Err(Graph);
    }
    let mut end = 0u64;
    for (i, slot) in slots.iter().enumerate() {
        end = end.max(start[i].checked_add(slot.slot_daa).ok_or(Graph)?);
    }
    end.checked_add(recovery_daa).ok_or(Graph)
}

/// Supply failure on the public path, with no inclusion premise adopted.
///
/// Missing supply stays [`crate::palw_court_service_binding_v1::CourtServiceAttributionV1::NoSlashHold`].
/// Batch clock version 5 ends that hold through [`crate::palw_tir_frame_cost_v1::batch_public_hold_end_v5`]:
/// abort, and settle reserved fees minus spent fees. That end is not Final.
/// Older clock versions stay a hold. Their rows are not given a version-5 end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublicSupplyFailureOutcomeV1 {
    pub attribution: crate::palw_court_service_binding_v1::CourtServiceAttributionV1,
    /// `Some` only for Batch clock version 5. Empty for every older clock.
    pub batch_v5_end: Option<crate::palw_tir_frame_cost_v1::PalwBatchPublicHoldEndV5>,
}

pub fn public_supply_failure_outcome_v1(
    service: &crate::palw_court_service_state_v2::CourtServiceStateV2,
    claim: crate::Hash64,
    clocks: &[&crate::palw_court_duty_clock_v1::CourtDutyClockV1],
    unpaid: &BTreeSet<crate::palw_state_v2::PalwBondKeyV2>,
    clock_version: u16,
    reserved_fee_sompi: u64,
    spent_fee_sompi: u64,
) -> Result<PublicSupplyFailureOutcomeV1, &'static str> {
    use crate::palw_court_service_binding_v1::{CourtServiceAttributionV1, attribute_service_default_v1};
    use crate::palw_tir_frame_cost_v1::{
        batch_public_hold_end_v5, batch_public_hold_reported_as_within_deadline_final_v5, is_batch_public_clock_v5,
    };
    let attribution = match attribute_service_default_v1(service, claim, clocks, unpaid, None)? {
        CourtServiceAttributionV1::PartyDefault => {
            return Err("public supply failure has no inclusion premise; no party default");
        }
        hold @ CourtServiceAttributionV1::NoSlashHold(_) => hold,
    };
    if is_batch_public_clock_v5(clock_version) {
        let end = batch_public_hold_end_v5(clock_version, reserved_fee_sompi, spent_fee_sompi)?;
        if batch_public_hold_reported_as_within_deadline_final_v5(&end) {
            return Err("batch v5 supply hold was reported as Final");
        }
        Ok(PublicSupplyFailureOutcomeV1 { attribution, batch_v5_end: Some(end) })
    } else if batch_public_hold_end_v5(clock_version, reserved_fee_sompi, spent_fee_sompi).is_ok() {
        Err("old claim row was reclassified as a batch v5 hold end")
    } else {
        Ok(PublicSupplyFailureOutcomeV1 { attribution, batch_v5_end: None })
    }
}

/// Canonical shared-request identity. The owner, the queue, and the relayer are not inputs.
pub fn shared_canonical_request_id_v1(claim: Hash64, allocation: Hash64, source_id: Hash64, ordinal: u64) -> Hash64 {
    let bytes = borsh::to_vec(&(1u16, claim, allocation, source_id, ordinal)).expect("canonical shared request id");
    let digest = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/public-shared-reservation/request/v1").hash(&bytes);
    Hash64::from_bytes(digest.as_bytes().try_into().expect("64-byte digest"))
}

/// The party's own submission packet. Two owners of one claim receive two ids.
pub fn shared_party_opportunity_id_v1(claim: Hash64, allocation: Hash64, source_id: Hash64, owner: &PalwBondKeyV2) -> [u8; 64] {
    crate::palw_court_service_binding_v1::court_service_packet_id_v1(claim, allocation, source_id, owner, 0)
}

/// Coalescing a second owner onto a reserved request. A different request id is
/// refused, and two owners are refused when they would share one opportunity.
pub fn shared_request_coalesce_keeps_opportunities_v1(
    reserved_request: Hash64,
    incoming_request: Hash64,
    opener: &PalwBondKeyV2,
    incoming: &PalwBondKeyV2,
    allocation: Hash64,
    source_id: Hash64,
    claim: Hash64,
) -> Result<(), &'static str> {
    if reserved_request == Hash64::default() || reserved_request != incoming_request {
        return Err("shared request would replace its canonical row");
    }
    let opener_seat = shared_party_opportunity_id_v1(claim, allocation, source_id, opener);
    let incoming_seat = shared_party_opportunity_id_v1(claim, allocation, source_id, incoming);
    if opener != incoming && opener_seat == incoming_seat {
        return Err("shared request collapsed two submission opportunities");
    }
    Ok(())
}

/// One party's inclusion outcome. A missing premise stays [`crate::palw_court_service_binding_v1::CourtServiceAttributionV1::NoSlashHold`].
/// When a premise is present, attribution still holds a party whose own supplied unit is outside it.
pub fn public_party_inclusion_outcome_v1(
    service: &crate::palw_court_service_state_v2::CourtServiceStateV2,
    claim: Hash64,
    clocks: &[&crate::palw_court_duty_clock_v1::CourtDutyClockV1],
    party: &PalwBondKeyV2,
    premise: Option<&crate::palw_court_service_binding_v1::CourtServiceInclusionPremiseV1>,
) -> Result<crate::palw_court_service_binding_v1::CourtServiceAttributionV1, &'static str> {
    use crate::palw_court_service_binding_v1::{CourtServiceAttributionV1, attribute_service_default_v1};
    let Some(premise) = premise else {
        return Ok(CourtServiceAttributionV1::NoSlashHold("party inclusion premise is missing; no party default"));
    };
    let unpaid = BTreeSet::from([*party]);
    attribute_service_default_v1(service, claim, clocks, &unpaid, Some(premise))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedReservationEffectV1 {
    Opened,
    Coalesced,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedReservationRefusalV1 {
    Budget(PublicSharedBudgetRefusalV1),
    ForeignRequest,
    DoubleBooked,
    Intake,
    Owner,
    Dependency,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SharedOwnerSeatV1 {
    opportunity: [u8; 64],
    ledger: MaterialLedgerV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ReservedGraphV1 {
    slots: BTreeMap<[u8; 64], PublicSharedSlotV1>,
    recovery_daa: u64,
    required_daa: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SharedClaimHoldV1 {
    request_id: Hash64,
    owners: BTreeMap<PalwBondKeyV2, SharedOwnerSeatV1>,
    graph: Option<ReservedGraphV1>,
}

/// Shared requests, per-party intake, and dependency slots under the 6,880 DAA budget.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedPublicReservationBookV1 {
    claims: BTreeMap<Hash64, SharedClaimHoldV1>,
    graph_slots: BTreeMap<[u8; 64], Hash64>,
    opportunities: ServiceReservations,
}

impl SharedPublicReservationBookV1 {
    pub fn new() -> Self {
        let budget = ServiceCost { bytes: 1, work: 1, io: 1, controls: 1 };
        Self {
            claims: BTreeMap::new(),
            graph_slots: BTreeMap::new(),
            opportunities: ServiceReservations::new(budget).expect("unit service budget"),
        }
    }

    /// Admit `owner` to `claim`'s one canonical request and reserve that owner's submission packet.
    pub fn admit_owner_v1(
        &mut self,
        claim: Hash64,
        request_id: Hash64,
        owner: PalwBondKeyV2,
        allocation: Hash64,
        source_id: Hash64,
        intake_budget_bytes: u64,
    ) -> Result<SharedReservationEffectV1, SharedReservationRefusalV1> {
        use SharedReservationRefusalV1::{DoubleBooked, ForeignRequest, Owner};
        if claim == Hash64::default() || request_id == Hash64::default() || intake_budget_bytes == 0 {
            return Err(Owner);
        }
        if let Some(held) = self.claims.get(&claim) {
            if held.request_id != request_id {
                return Err(ForeignRequest);
            }
            if held.owners.contains_key(&owner) {
                shared_request_coalesce_keeps_opportunities_v1(
                    held.request_id,
                    request_id,
                    &owner,
                    &owner,
                    allocation,
                    source_id,
                    claim,
                )
                .map_err(|_| ForeignRequest)?;
                return Ok(SharedReservationEffectV1::Coalesced);
            }
        }
        let opportunity = shared_party_opportunity_id_v1(claim, allocation, source_id, &owner);
        if self.opportunities.packet(&opportunity).is_some() || self.graph_slots.contains_key(&opportunity) {
            return Err(DoubleBooked);
        }
        let ledger = MaterialLedgerV1 {
            version: 1,
            claim,
            submitter: owner,
            budget_bytes: intake_budget_bytes,
            reserved_bytes: 0,
            charged_sompi: 0,
            material_until_daa: DISPUTE_WINDOW_DAA,
        };
        ledger.check(&PalwStagedCourtKeyV1::CourtMaterialLedger(claim, owner)).map_err(|_| Owner)?;
        let packet = opportunity_packet(opportunity);
        self.opportunities.reserve(std::slice::from_ref(&packet), DISPUTE_WINDOW_DAA).map_err(reservation_refusal)?;
        let fresh = !self.claims.contains_key(&claim);
        self.claims
            .entry(claim)
            .or_insert_with(|| SharedClaimHoldV1 { request_id, owners: BTreeMap::new(), graph: None })
            .owners
            .insert(owner, SharedOwnerSeatV1 { opportunity, ledger });
        Ok(if fresh { SharedReservationEffectV1::Opened } else { SharedReservationEffectV1::Coalesced })
    }

    /// Reserve bytes on this owner's ledger. Another owner's budget is left unchanged.
    pub fn reserve_intake_v1(&mut self, claim: Hash64, owner: &PalwBondKeyV2, bytes: u64) -> Result<(), SharedReservationRefusalV1> {
        use SharedReservationRefusalV1::{Intake, Owner};
        if bytes == 0 {
            return Err(Intake);
        }
        let seat = self.claims.get_mut(&claim).and_then(|held| held.owners.get_mut(owner)).ok_or(Owner)?;
        let next =
            seat.ledger.reserved_bytes.checked_add(bytes).filter(|reserved| *reserved <= seat.ledger.budget_bytes).ok_or(Intake)?;
        let mut ledger = seat.ledger.clone();
        ledger.reserved_bytes = next;
        ledger.check(&PalwStagedCourtKeyV1::CourtMaterialLedger(claim, *owner)).map_err(|_| Intake)?;
        seat.ledger = ledger;
        Ok(())
    }

    /// Reserve `slots` for an admitted claim. The ordered span uses the 6,880 DAA check.
    /// The same graph again is a duplicate. A different graph, or a slot another claim holds, is refused.
    pub fn reserve_dependency_graph_v1(
        &mut self,
        claim: Hash64,
        notice_offset_daa: u64,
        slots: &[PublicSharedSlotV1],
        recovery_daa: u64,
    ) -> Result<u64, SharedReservationRefusalV1> {
        use SharedReservationRefusalV1::{Budget, Dependency, DoubleBooked, ForeignRequest, Owner};
        if !self.claims.get(&claim).is_some_and(|held| !held.owners.is_empty()) {
            return Err(Owner);
        }
        let required = check_reserved_public_graph_v1(notice_offset_daa, slots, recovery_daa).map_err(Budget)?;
        if required > PALW_PUBLIC_SHARED_BUDGET_AFTER_NOTICE_DAA_V1 {
            return Err(Budget(PublicSharedBudgetRefusalV1::Exceeds {
                required,
                allowed: PALW_PUBLIC_SHARED_BUDGET_AFTER_NOTICE_DAA_V1,
            }));
        }
        let normalized = normalize_slots(slots);
        if let Some(graph) = &self.claims[&claim].graph {
            if graph.slots == normalized && graph.recovery_daa == recovery_daa && graph.required_daa == required {
                return Ok(graph.required_daa);
            }
            return Err(ForeignRequest);
        }
        for id in normalized.keys() {
            match self.graph_slots.get(id) {
                Some(holder) if *holder != claim => return Err(DoubleBooked),
                Some(_) => return Err(DoubleBooked),
                None if self.opportunities.packet(id).is_some() => return Err(DoubleBooked),
                None => {}
            }
        }
        let packets = normalized
            .values()
            .map(|slot| CausalServicePacketV1 { packet: opportunity_packet(slot.id), predecessors: slot.predecessors.clone() })
            .collect::<Vec<_>>();
        self.opportunities.reserve_causal_v1(&packets, DISPUTE_WINDOW_DAA).map_err(|refusal| match refusal {
            ServiceRefusal::DuplicateIdentity => DoubleBooked,
            _ => Dependency,
        })?;
        for id in normalized.keys() {
            self.graph_slots.insert(*id, claim);
        }
        self.claims.get_mut(&claim).ok_or(Owner)?.graph =
            Some(ReservedGraphV1 { slots: normalized, recovery_daa, required_daa: required });
        Ok(required)
    }
}

fn opportunity_packet(id: [u8; 64]) -> ServicePacket {
    ServicePacket {
        id,
        release_daa: 1,
        service_before: DISPUTE_WINDOW_DAA,
        cost: ServiceCost { bytes: 1, work: 1, io: 1, controls: 1 },
    }
}

fn normalize_slots(slots: &[PublicSharedSlotV1]) -> BTreeMap<[u8; 64], PublicSharedSlotV1> {
    let mut map = BTreeMap::new();
    for slot in slots {
        let mut slot = slot.clone();
        slot.predecessors.sort();
        map.insert(slot.id, slot);
    }
    map
}

fn reservation_refusal(refusal: ServiceRefusal) -> SharedReservationRefusalV1 {
    match refusal {
        ServiceRefusal::DuplicateIdentity => SharedReservationRefusalV1::DoubleBooked,
        _ => SharedReservationRefusalV1::Owner,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Hash64;
    use crate::palw_court_service_binding_v1::{
        CourtServiceAttributionV1, CourtServiceInclusionPremiseV1, CourtServiceProfileV1, court_service_binding_candidate_v1,
    };
    use crate::palw_court_service_v1::{ServiceCost, ServiceReservations};
    use crate::palw_state_v2::PalwBondKeyV2;
    use crate::tx::TransactionOutpoint;
    use std::collections::BTreeSet;

    fn id(n: u8) -> [u8; 64] {
        [n; 64]
    }

    fn slot(n: u8, slot_daa: u64, material_ready_daa: u64, preds: &[u8]) -> PublicSharedSlotV1 {
        PublicSharedSlotV1 { id: id(n), slot_daa, material_ready_daa, predecessors: preds.iter().copied().map(id).collect() }
    }

    #[test]
    fn public_shared_budget_accepts_a_graph_that_fits_in_6880() {
        assert_eq!(PALW_PUBLIC_SHARED_BUDGET_AFTER_NOTICE_DAA_V1, 6_880);
        assert_eq!(PALW_PUBLIC_SHARED_BUDGET_AFTER_NOTICE_DAA_V1, DISPUTE_WINDOW_DAA - NOTICE_WINDOW_DAA);
        // Two roots run together. The dependent slot waits for both, then recovery.
        // The sum of the slot capacities is 10,000, which is past 6,880; the
        // ordered span is 4,000 + 2,000 + 880 = 6,880.
        let slots = vec![slot(1, 4_000, 0, &[]), slot(2, 4_000, 0, &[]), slot(3, 2_000, 0, &[1, 2])];
        let unordered: u64 = slots.iter().map(|slot| slot.slot_daa).sum();
        assert!(unordered > PALW_PUBLIC_SHARED_BUDGET_AFTER_NOTICE_DAA_V1);
        let recovery = 880;
        assert_eq!(check_reserved_public_graph_v1(NOTICE_WINDOW_DAA, &slots, recovery), Ok(6_880));
        // An earlier notice does not receive a larger budget, and does not change a span that already fits.
        assert_eq!(check_reserved_public_graph_v1(0, &slots, recovery), Ok(6_880));
    }

    #[test]
    fn public_shared_budget_refuses_a_graph_past_6880() {
        let slots = vec![slot(1, 6_881, 0, &[])];
        let refusal = Err(PublicSharedBudgetRefusalV1::Exceeds { required: 6_881, allowed: 6_880 });
        assert_eq!(check_reserved_public_graph_v1(NOTICE_WINDOW_DAA, &slots, 0), refusal);
        // Notice at acceptance would leave 10,000 DAA if the budget followed the actual notice.
        // The checker still uses the slowest legal first notice.
        assert_eq!(check_reserved_public_graph_v1(0, &slots, 0), refusal);
        assert_eq!(check_reserved_public_graph_v1(NOTICE_WINDOW_DAA + 1, &slots, 0), Err(PublicSharedBudgetRefusalV1::Notice));
    }

    #[test]
    fn public_shared_budget_refuses_a_graph_that_only_fits_the_retired_localization_span() {
        assert_eq!(PALW_RETIRED_LOCALIZATION_RETRIES_V1 * PALW_RETIRED_LOCALIZATION_ACQUISITION_DAA_V1, 262_144);
        assert!(PALW_RETIRED_LOCALIZATION_PATH_DAA_V1 > 262_144);
        assert!(PALW_RETIRED_LOCALIZATION_PATH_DAA_V1 > PALW_PUBLIC_SHARED_BUDGET_AFTER_NOTICE_DAA_V1);
        let slots = vec![slot(1, PALW_RETIRED_LOCALIZATION_PATH_DAA_V1, 0, &[])];
        let required = PALW_RETIRED_LOCALIZATION_PATH_DAA_V1;
        assert_eq!(
            check_reserved_public_graph_v1(NOTICE_WINDOW_DAA, &slots, 0),
            Err(PublicSharedBudgetRefusalV1::Exceeds { required, allowed: 6_880 })
        );
        assert_eq!(check_scheduled_public_span_v1(required), Err(PublicSharedBudgetRefusalV1::Exceeds { required, allowed: 6_880 }));
    }

    #[test]
    fn public_shared_budget_refuses_when_order_exceeds_6880_and_the_unordered_sum_fits() {
        // Capacities sum to 6,880 and recovery is zero. The second slot's material
        // is ready one DAA after the first slot ends, so the ordered span is 6,881.
        let slots = vec![slot(1, 3_440, 0, &[]), slot(2, 3_440, 3_441, &[1])];
        let unordered: u64 = slots.iter().map(|slot| slot.slot_daa).sum();
        let recovery = 0;
        assert_eq!(unordered + recovery, 6_880);
        assert_eq!(
            check_reserved_public_graph_v1(NOTICE_WINDOW_DAA, &slots, recovery),
            Err(PublicSharedBudgetRefusalV1::Exceeds { required: 6_881, allowed: 6_880 })
        );
    }

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }

    fn bond(n: u32) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(h(0xB0), n))
    }

    fn profile() -> CourtServiceProfileV1 {
        let unit = ServiceCost { bytes: 10, work: 10, io: 1, controls: 1 };
        CourtServiceProfileV1 {
            version: 1,
            budget: ServiceCost { bytes: 100, work: 100, io: 10, controls: 10 },
            packet_cost: unit,
            packets_per_signer: 2,
            service_span_daa: 50,
        }
    }

    fn clock(actor: PalwBondKeyV2) -> crate::palw_court_duty_clock_v1::CourtDutyClockV1 {
        crate::palw_court_duty_clock_v1::CourtDutyClockV1 {
            version: 1,
            profile_id: h(9),
            duty_id: h(10),
            claim: h(1),
            allocation_id: h(2),
            source_id: h(3),
            acquisition: crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1::TirAcquisition(h(1), actor, h(11)),
            answerer: actor,
            demand_id: h(12),
            turn: 0,
            predecessor_block: h(13),
            predecessor_daa: 1_000,
            first_eligible_daa: 1_001,
            response_deadline_daa: 1_200,
            original_hard_daa: 1_000 + DISPUTE_WINDOW_DAA,
            material_until_daa: 1_000 + DISPUTE_WINDOW_DAA,
        }
    }

    fn observe_all(book: &mut ServiceReservations, from: u64, to: u64, skip: Option<u64>) {
        for daa in from..=to {
            if Some(daa) == skip {
                continue;
            }
            book.observe_block(daa, [daa as u8; 64], ServiceCost::default()).unwrap();
        }
    }

    #[test]
    fn public_shared_budget_supply_failure_without_premise_is_no_slash_hold() {
        let signers: BTreeSet<_> = [bond(1), bond(2)].into_iter().collect();
        let mut service = court_service_binding_candidate_v1(None, h(1), h(2), h(3), 1_000, &signers, &profile()).unwrap();
        let skipped = *service.book.assignments.values().next().unwrap();
        observe_all(&mut service.book, 1_001, 1_101, Some(skipped));
        let unpaid: BTreeSet<_> = [bond(1)].into_iter().collect();
        let duty = clock(bond(1));
        let missing = public_supply_failure_outcome_v1(&service, h(1), &[&duty], &unpaid, clock_v1(), 0, 0).unwrap();
        assert!(matches!(missing.attribution, CourtServiceAttributionV1::NoSlashHold(_)));
        assert!(missing.batch_v5_end.is_none());

        let mut supplied = court_service_binding_candidate_v1(None, h(1), h(2), h(3), 1_000, &signers, &profile()).unwrap();
        observe_all(&mut supplied.book, 1_001, 1_101, None);
        let no_premise = public_supply_failure_outcome_v1(&supplied, h(1), &[&duty], &unpaid, clock_v1(), 0, 0);
        assert!(no_premise.is_err());
        assert!(!matches!(no_premise, Ok(PublicSupplyFailureOutcomeV1 { attribution: CourtServiceAttributionV1::PartyDefault, .. })));
    }

    fn claim_request(n: u64, ordinal: u64) -> (Hash64, Hash64, Hash64, Hash64) {
        let claim = h(n);
        let allocation = h(0xA0 + n);
        let source = h(0xC0 + n);
        (claim, allocation, source, shared_canonical_request_id_v1(claim, allocation, source, ordinal))
    }

    #[test]
    fn shared_reservation_holds_one_request_for_two_owners() {
        let (claim, allocation, source, request) = claim_request(1, 0);
        assert_eq!(request, shared_canonical_request_id_v1(claim, allocation, source, 0));
        assert_ne!(request, shared_canonical_request_id_v1(claim, allocation, source, 1));
        let owner_a = bond(1);
        let owner_b = bond(2);
        let seat_a = shared_party_opportunity_id_v1(claim, allocation, source, &owner_a);
        let seat_b = shared_party_opportunity_id_v1(claim, allocation, source, &owner_b);
        assert_ne!(seat_a, seat_b);
        assert!(
            shared_request_coalesce_keeps_opportunities_v1(request, request, &owner_a, &owner_b, allocation, source, claim).is_ok()
        );

        let mut book = SharedPublicReservationBookV1::new();
        let budget = 32;
        assert_eq!(book.admit_owner_v1(claim, request, owner_a, allocation, source, budget), Ok(SharedReservationEffectV1::Opened));
        assert_eq!(book.admit_owner_v1(claim, request, owner_b, allocation, source, budget), Ok(SharedReservationEffectV1::Coalesced));
        assert_eq!(book.admit_owner_v1(claim, request, owner_a, allocation, source, budget), Ok(SharedReservationEffectV1::Coalesced));
        let held = &book.claims[&claim];
        assert_eq!(held.request_id, request);
        assert_eq!(held.owners.len(), 2);
        assert_eq!(held.owners[&owner_a].opportunity, seat_a);
        assert_eq!(held.owners[&owner_b].opportunity, seat_b);
        let daa_a = book.opportunities.assigned_daa(&seat_a);
        let daa_b = book.opportunities.assigned_daa(&seat_b);
        assert!(daa_a.is_some() && daa_b.is_some() && daa_a != daa_b);

        book.reserve_intake_v1(claim, &owner_a, budget).unwrap();
        book.reserve_intake_v1(claim, &owner_b, budget).unwrap();
        assert_eq!(book.claims[&claim].owners[&owner_a].ledger.reserved_bytes, budget);
        assert_eq!(book.claims[&claim].owners[&owner_b].ledger.reserved_bytes, budget);
        assert_eq!(book.claims[&claim].owners[&owner_b].ledger.budget_bytes, budget);
        let reserved = book.clone();
        assert_eq!(book.reserve_intake_v1(claim, &owner_a, 1), Err(SharedReservationRefusalV1::Intake));
        assert_eq!(book, reserved);
        assert_eq!(book.claims[&claim].owners[&owner_b].ledger.reserved_bytes, budget);
    }

    #[test]
    fn shared_reservation_holds_two_claims_do_not_share_a_slot() {
        let (claim_a, allocation_a, source_a, request_a) = claim_request(1, 0);
        let (claim_b, allocation_b, source_b, request_b) = claim_request(2, 0);
        assert_ne!(request_a, request_b);
        let mut book = SharedPublicReservationBookV1::new();
        book.admit_owner_v1(claim_a, request_a, bond(1), allocation_a, source_a, 8).unwrap();
        book.admit_owner_v1(claim_b, request_b, bond(2), allocation_b, source_b, 8).unwrap();
        let parent = slot(1, 10, 0, &[]);
        let child = slot(5, 10, 0, &[1]);
        assert_eq!(book.reserve_dependency_graph_v1(claim_a, NOTICE_WINDOW_DAA, &[parent.clone(), child.clone()], 0), Ok(20));
        let parent_daa = book.opportunities.assigned_daa(&parent.id).unwrap();
        let child_daa = book.opportunities.assigned_daa(&child.id).unwrap();
        assert!(child_daa > parent_daa);
        assert_eq!(book.graph_slots[&parent.id], claim_a);
        let held = book.clone();
        assert_eq!(
            book.reserve_dependency_graph_v1(claim_b, NOTICE_WINDOW_DAA, &[parent.clone()], 0),
            Err(SharedReservationRefusalV1::DoubleBooked)
        );
        assert_eq!(book, held);
        let other = slot(2, 10, 0, &[]);
        assert_eq!(book.reserve_dependency_graph_v1(claim_b, NOTICE_WINDOW_DAA, &[other.clone()], 0), Ok(10));
        assert_eq!(book.graph_slots[&parent.id], claim_a);
        assert_eq!(book.graph_slots[&other.id], claim_b);
        assert_ne!(book.opportunities.assigned_daa(&other.id), Some(parent_daa));
        assert_eq!(book.opportunities.assigned_daa(&parent.id), Some(parent_daa));
        assert_eq!(book.claims[&claim_a].request_id, request_a);
        assert_eq!(book.claims[&claim_b].request_id, request_b);
    }

    #[test]
    fn shared_reservation_holds_malicious_request_does_not_evict_or_widen() {
        let (claim, allocation, source, request) = claim_request(1, 0);
        let owner = bond(1);
        let mut book = SharedPublicReservationBookV1::new();
        book.admit_owner_v1(claim, request, owner, allocation, source, 8).unwrap();
        let reserved = slot(7, 100, 0, &[]);
        assert_eq!(book.reserve_dependency_graph_v1(claim, NOTICE_WINDOW_DAA, &[reserved.clone()], 0), Ok(100));
        let seat = book.opportunities.assigned_daa(&reserved.id);
        let held = book.clone();
        let other = shared_canonical_request_id_v1(claim, allocation, source, 1);
        assert!(shared_request_coalesce_keeps_opportunities_v1(request, other, &owner, &bond(2), allocation, source, claim).is_err());
        assert_eq!(book.admit_owner_v1(claim, other, bond(2), allocation, source, 8), Err(SharedReservationRefusalV1::ForeignRequest));
        let oversize = slot(8, 6_881, 0, &[]);
        let exceeds =
            Err(SharedReservationRefusalV1::Budget(PublicSharedBudgetRefusalV1::Exceeds { required: 6_881, allowed: 6_880 }));
        assert_eq!(book.reserve_dependency_graph_v1(claim, NOTICE_WINDOW_DAA, &[oversize.clone()], 0), exceeds);
        assert_eq!(book.reserve_dependency_graph_v1(claim, 0, &[oversize], 0), exceeds);
        let ordered = vec![slot(3, 3_440, 0, &[]), slot(4, 3_440, 3_441, &[3])];
        assert_eq!(ordered.iter().map(|slot| slot.slot_daa).sum::<u64>(), 6_880);
        assert_eq!(book.reserve_dependency_graph_v1(claim, NOTICE_WINDOW_DAA, &ordered, 0), exceeds);
        assert_eq!(
            book.reserve_dependency_graph_v1(claim, NOTICE_WINDOW_DAA + 1, &[reserved.clone()], 0),
            Err(SharedReservationRefusalV1::Budget(PublicSharedBudgetRefusalV1::Notice))
        );
        assert_eq!(book.reserve_dependency_graph_v1(claim, NOTICE_WINDOW_DAA, &[reserved], 0), Ok(100));
        assert_eq!(book, held);
        assert_eq!(book.opportunities.assigned_daa(&id(7)), seat);
        assert_eq!(PALW_PUBLIC_SHARED_BUDGET_AFTER_NOTICE_DAA_V1, 6_880);
        assert_eq!(PALW_PUBLIC_SHARED_BUDGET_AFTER_NOTICE_DAA_V1, DISPUTE_WINDOW_DAA - NOTICE_WINDOW_DAA);
    }

    #[test]
    fn shared_reservation_holds_missing_own_inclusion_is_no_slash_hold() {
        let signers: BTreeSet<_> = [bond(1), bond(2)].into_iter().collect();
        let mut service = court_service_binding_candidate_v1(None, h(1), h(2), h(3), 1_000, &signers, &profile()).unwrap();
        observe_all(&mut service.book, 1_001, 1_101, None);
        let party = bond(2);
        let own = clock(party);
        let missing = public_party_inclusion_outcome_v1(&service, h(1), &[&own], &party, None);
        assert!(matches!(missing, Ok(CourtServiceAttributionV1::NoSlashHold(_))));
        assert!(!matches!(missing, Ok(CourtServiceAttributionV1::PartyDefault)));

        let mut tight = clock(party);
        tight.response_deadline_daa = tight.first_eligible_daa;
        let premise = CourtServiceInclusionPremiseV1 { version: 1, inclusion_delay_daa: 0 };
        let outside = public_party_inclusion_outcome_v1(&service, h(1), &[&tight], &party, Some(&premise));
        assert!(matches!(outside, Ok(CourtServiceAttributionV1::NoSlashHold(_))));

        let admitted = public_party_inclusion_outcome_v1(&service, h(1), &[&clock(bond(1))], &bond(1), Some(&premise));
        assert_eq!(admitted, Ok(CourtServiceAttributionV1::PartyDefault));
        let unpaid: BTreeSet<_> = [bond(1)].into_iter().collect();
        let still_undecided = public_supply_failure_outcome_v1(&service, h(1), &[&clock(bond(1))], &unpaid, clock_v1(), 0, 0);
        assert!(still_undecided.is_err());
        assert!(!matches!(
            still_undecided,
            Ok(PublicSupplyFailureOutcomeV1 { attribution: CourtServiceAttributionV1::PartyDefault, .. })
        ));
    }

    fn clock_v1() -> u16 {
        crate::palw_tir_frame_cost_v1::PALW_TIR_FRAME_CLOCK_VERSION_V1
    }

    #[test]
    fn batch_v5_hold_end_applies_on_supply_failure_and_is_not_final() {
        use crate::palw_tir_frame_cost_v1::{
            PALW_TIR_FRAME_CLOCK_BATCH_PUBLIC_V5, PALW_TIR_FRAME_CLOCK_FAMILY_MATERIAL_V4, PalwBatchPublicHoldEndV5,
            batch_public_hold_reported_as_within_deadline_final_v5,
        };
        let signers: BTreeSet<_> = [bond(1), bond(2)].into_iter().collect();
        let mut service = court_service_binding_candidate_v1(None, h(1), h(2), h(3), 1_000, &signers, &profile()).unwrap();
        let skipped = *service.book.assignments.values().next().unwrap();
        observe_all(&mut service.book, 1_001, 1_101, Some(skipped));
        let unpaid: BTreeSet<_> = [bond(1)].into_iter().collect();
        let duty = clock(bond(1));
        let before = service.clone();
        let outcome =
            public_supply_failure_outcome_v1(&service, h(1), &[&duty], &unpaid, PALW_TIR_FRAME_CLOCK_BATCH_PUBLIC_V5, 1_000, 250)
                .unwrap();
        assert!(matches!(outcome.attribution, CourtServiceAttributionV1::NoSlashHold(_)));
        let end = outcome.batch_v5_end.expect("version 5 supply failure ends the hold");
        assert_eq!(end, PalwBatchPublicHoldEndV5::AbortSettleUnused { unused_reserved_fee_sompi: 750 });
        assert!(!batch_public_hold_reported_as_within_deadline_final_v5(&end));
        assert_eq!(service, before);

        let old = public_supply_failure_outcome_v1(&service, h(1), &[&duty], &unpaid, clock_v1(), 1_000, 250).unwrap();
        assert!(matches!(old.attribution, CourtServiceAttributionV1::NoSlashHold(_)));
        assert!(old.batch_v5_end.is_none());
        let family =
            public_supply_failure_outcome_v1(&service, h(1), &[&duty], &unpaid, PALW_TIR_FRAME_CLOCK_FAMILY_MATERIAL_V4, 1_000, 0)
                .unwrap();
        assert!(matches!(family.attribution, CourtServiceAttributionV1::NoSlashHold(_)));
        assert!(family.batch_v5_end.is_none());
        assert_eq!(service, before);
        assert!(
            public_supply_failure_outcome_v1(&service, h(1), &[&duty], &unpaid, PALW_TIR_FRAME_CLOCK_BATCH_PUBLIC_V5, 10, 11).is_err()
        );

        let mut supplied = court_service_binding_candidate_v1(None, h(1), h(2), h(3), 1_000, &signers, &profile()).unwrap();
        observe_all(&mut supplied.book, 1_001, 1_101, None);
        let supplied_before = supplied.clone();
        assert!(
            public_supply_failure_outcome_v1(&supplied, h(1), &[&duty], &unpaid, PALW_TIR_FRAME_CLOCK_BATCH_PUBLIC_V5, 1_000, 250)
                .is_err()
        );
        assert_eq!(supplied, supplied_before);
    }
}
