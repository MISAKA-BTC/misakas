//! **G14 lane r6-d1-release — user decision D1 (2026-10-22): the resolved-binding release rule.** Written, not compiled.
//!
//! A claim bound to the rooted court service book goes Final on its normal schedule (Final is never delayed by the binding). After
//! Final the per-block supply recording still writes the book (`CourtServiceStateV2::check_context` no longer refuses a Final bound
//! claim). Once EVERY packet of a binding is decided by the accepting chain (`OpportunityProvided` or `MissingSupply`), its hard
//! deadline has passed, no court session of the claim is open, every duty clock's response deadline has passed and no row that
//! reads the binding/frame remains, the binding is RELEASED:
//!
//! * the binding leaves `claims` (so the collateral / material / retirement holds of `court_service_holds_*` end);
//! * a compact [`ResolvedServiceBindingV1`] record (scope, decided counts, release DAA) stays in the rooted row;
//! * its packets stay in the book only while they share a DAA slot with a still-live packet ([`ResolvedServiceBindingV1::
//!   retained_packet_ids`]); a slot whose packets are all retired is compacted away together with its residual quota and its supply
//!   record, so `ServiceReservations::check`'s supply invariant (`supply.reserved == budget − remaining`) holds unchanged.
//!
//! The released form needs data the legacy row cannot carry, so it rides a NEW appended row variant
//! (`PalwStagedCourtRowV1::CourtServiceV3`, same key `CourtService`) holding [`CourtServiceRowV3`]. The legacy variant
//! (`CourtService`, version 2) keeps its encoding byte for byte and stays readable; the V3 variant is written only past
//! `palw_tir_public_conviction_v1` (the writer refuses it below the fence), at the first release or at the first funded admission
//! that records causal edges. Inside a V3 row the embedded state carries `version = 3`, which is what lets
//! `CourtServiceStateV2::check` accept retired packets that no live binding owns — the strict ownership check is
//! [`CourtServiceRowV3::check`].
//!
//! The row also carries the per-packet causal predecessors of every live binding ([`CourtServiceRowV3::causal`]), so the canonical
//! recovery sweep keeps causal order (a source packet is never rescheduled at or after a packet that depends on it): risk (a) of
//! court-f02 / r5-original-admission.
use crate::{Hash64, palw_court_service_state_v2::CourtServiceStateV2, palw_court_service_v1::ServiceAssessment};
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::{BTreeMap, BTreeSet};

pub type ServicePacketIdV1 = [u8; 64];

/// The row version of [`CourtServiceRowV3`] (and of its embedded state).
pub const COURT_SERVICE_ROW_VERSION_V3: u16 = 3;
/// At most this many bindings are released per accepting block (bounded work; the rest wait one block).
pub const COURT_SERVICE_RELEASES_PER_BLOCK_V1: usize = 64;

/// **A released binding's compact record** — its original scope, how its packets were decided, and when it was released.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ResolvedServiceBindingV1 {
    pub claim: Hash64,
    pub allocation_id: Hash64,
    pub source_id: Hash64,
    pub original_accepted_daa: u64,
    pub hard_deadline_daa: u64,
    /// The accepting DAA of the release (the book's observed clock at that block).
    pub released_daa: u64,
    /// Packets the binding owned, and how the accepting chain decided them (served + missing == packets).
    pub packets: u32,
    pub served: u32,
    pub missing: u32,
    /// Retired packets still in the book because their DAA slot is shared with a live packet (compaction removes them later).
    pub retained_packet_ids: BTreeSet<ServicePacketIdV1>,
}

/// **The court service book row past the release rule** (`PalwStagedCourtRowV1::CourtServiceV3`).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CourtServiceRowV3 {
    pub version: u16,
    /// The live book and live bindings; `state.version == 3`.
    pub state: CourtServiceStateV2,
    /// Released bindings, by claim.
    pub resolved: BTreeMap<Hash64, ResolvedServiceBindingV1>,
    /// Causal predecessors of live packets (child -> strictly increasing predecessor ids, all of the same live binding).
    pub causal: BTreeMap<ServicePacketIdV1, Vec<ServicePacketIdV1>>,
}

/// The binding scope context checks compare against — live or released.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServiceBindingScopeV1 {
    pub claim: Hash64,
    pub allocation_id: Hash64,
    pub source_id: Hash64,
    pub original_accepted_daa: u64,
    pub hard_deadline_daa: u64,
}

impl CourtServiceRowV3 {
    /// The V3 form of a legacy row: same book and bindings, nothing released, no causal edges.
    pub fn from_v2(state: &CourtServiceStateV2) -> Self {
        let mut state = state.clone();
        state.version = COURT_SERVICE_ROW_VERSION_V3;
        Self { version: COURT_SERVICE_ROW_VERSION_V3, state, resolved: BTreeMap::new(), causal: BTreeMap::new() }
    }

    fn live_owner(&self) -> BTreeMap<ServicePacketIdV1, Hash64> {
        let mut owner = BTreeMap::new();
        for (claim, binding) in &self.state.claims {
            for id in &binding.packet_ids {
                owner.insert(*id, *claim);
            }
        }
        owner
    }

    /// **The strict row check**: every book packet is owned by exactly one live binding or retained by exactly one released record,
    /// every released record is internally consistent, and every causal edge joins two packets of the same live binding in order.
    pub fn check(&self) -> Result<(), String> {
        if self.version != COURT_SERVICE_ROW_VERSION_V3 || self.state.version != COURT_SERVICE_ROW_VERSION_V3 {
            return Err("unsupported court service row version".into());
        }
        self.state.check_live_v1()?;
        let owner = self.live_owner();
        let mut retired = BTreeSet::new();
        for (claim, record) in &self.resolved {
            if claim != &record.claim || self.state.claims.contains_key(claim) {
                return Err("released service record names a live or another claim".into());
            }
            if record.original_accepted_daa.checked_add(crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA)
                != Some(record.hard_deadline_daa)
                || record.released_daa < record.hard_deadline_daa
                || record.served.checked_add(record.missing) != Some(record.packets)
                || record.packets == 0
                || record.retained_packet_ids.len() as u64 > u64::from(record.packets)
            {
                return Err("invalid released service record".into());
            }
            for id in &record.retained_packet_ids {
                if owner.contains_key(id) || !retired.insert(*id) || self.state.book.packet(id).is_none() {
                    return Err("retained released packet is live, duplicated or absent".into());
                }
                if !matches!(self.state.book.assess(id), ServiceAssessment::OpportunityProvided | ServiceAssessment::MissingSupply) {
                    return Err("retained released packet is undecided".into());
                }
            }
        }
        if owner.len().checked_add(retired.len()) != Some(self.state.book.packets.len()) {
            return Err("service packet is neither live nor retained by a released record".into());
        }
        for (child, predecessors) in &self.causal {
            let claim = owner.get(child).ok_or("causal edge names a packet no live binding owns")?;
            if predecessors.is_empty() || predecessors.windows(2).any(|w| w[0] >= w[1]) {
                return Err("causal predecessors must be non-empty and strictly increasing".into());
            }
            let at = self.state.book.assigned_daa(child).ok_or("causal child has no assignment")?;
            for predecessor in predecessors {
                if owner.get(predecessor) != Some(claim) || predecessor == child {
                    return Err("causal predecessor is not a packet of the same live binding".into());
                }
                if self.state.book.assigned_daa(predecessor).is_none_or(|p| p >= at) {
                    return Err("causal child is not scheduled strictly after its predecessor".into());
                }
            }
        }
        Ok(())
    }

    /// The binding scope of `claim`, live or released.
    pub fn scope_v1(&self, claim: &Hash64) -> Option<ServiceBindingScopeV1> {
        if let Some(b) = self.state.claims.get(claim) {
            return Some(ServiceBindingScopeV1 {
                claim: b.claim,
                allocation_id: b.allocation_id,
                source_id: b.source_id,
                original_accepted_daa: b.original_accepted_daa,
                hard_deadline_daa: b.hard_deadline_daa,
            });
        }
        self.resolved.get(claim).map(|r| ServiceBindingScopeV1 {
            claim: r.claim,
            allocation_id: r.allocation_id,
            source_id: r.source_id,
            original_accepted_daa: r.original_accepted_daa,
            hard_deadline_daa: r.hard_deadline_daa,
        })
    }

    /// Is every packet of the live binding of `claim` decided at the book's clock, with the clock at or past its hard deadline?
    /// (The book-local half of release eligibility; the fold adds courts, duty clocks and dependent rows.)
    pub fn binding_decided_v1(state: &CourtServiceStateV2, claim: &Hash64) -> bool {
        let Some(binding) = state.claims.get(claim) else { return false };
        let Some(now) = state.book.last_observed_daa else { return false };
        now >= binding.hard_deadline_daa
            && binding
                .packet_ids
                .iter()
                .all(|id| matches!(state.book.assess(id), ServiceAssessment::OpportunityProvided | ServiceAssessment::MissingSupply))
    }

    /// **Release `release` at the book's current clock `now`** (all-or-nothing). Each binding must be decided
    /// ([`Self::binding_decided_v1`]); its edges leave, its record is written, then fully retired slots are compacted.
    pub fn release_v1(&mut self, release: &BTreeSet<Hash64>, now: u64) -> Result<(), String> {
        if release.is_empty() {
            return Err("an empty service release".into());
        }
        if self.state.book.last_observed_daa != Some(now) {
            return Err("a service release runs only at the book's current observed clock".into());
        }
        let mut next = self.clone();
        for claim in release {
            if !Self::binding_decided_v1(&next.state, claim) {
                return Err("a service binding is released only when every packet is decided past its hard deadline".into());
            }
            let binding = next.state.claims.remove(claim).ok_or("service release names no live binding")?;
            let (mut served, mut missing) = (0u32, 0u32);
            for id in &binding.packet_ids {
                match next.state.book.assess(id) {
                    ServiceAssessment::OpportunityProvided => served = served.checked_add(1).ok_or("service count overflow")?,
                    ServiceAssessment::MissingSupply => missing = missing.checked_add(1).ok_or("service count overflow")?,
                    _ => return Err("undecided service packet at release".into()),
                }
            }
            next.causal.retain(|child, _| !binding.packet_ids.contains(child));
            let record = ResolvedServiceBindingV1 {
                claim: binding.claim,
                allocation_id: binding.allocation_id,
                source_id: binding.source_id,
                original_accepted_daa: binding.original_accepted_daa,
                hard_deadline_daa: binding.hard_deadline_daa,
                released_daa: now,
                packets: u32::try_from(binding.packet_ids.len()).map_err(|_| "service packet count overflow")?,
                served,
                missing,
                retained_packet_ids: binding.packet_ids,
            };
            if next.resolved.insert(*claim, record).is_some() {
                return Err("service binding already released".into());
            }
        }
        next.compact_v1();
        next.check()?;
        *self = next;
        Ok(())
    }

    /// Remove every DAA slot whose packets are all retired: its packets, its assignments, its residual quota and its supply record
    /// leave together (the supply invariant is per slot, so the rest of the book is untouched).
    fn compact_v1(&mut self) {
        let owner = self.live_owner();
        let mut slot_has_live: BTreeMap<u64, bool> = BTreeMap::new();
        for (id, slot) in &self.state.book.assignments {
            let live = slot_has_live.entry(*slot).or_insert(false);
            *live |= owner.contains_key(id);
        }
        let dead: BTreeSet<u64> = slot_has_live.into_iter().filter(|(_, live)| !*live).map(|(slot, _)| slot).collect();
        if dead.is_empty() {
            return;
        }
        let gone: Vec<ServicePacketIdV1> =
            self.state.book.assignments.iter().filter(|(_, slot)| dead.contains(slot)).map(|(id, _)| *id).collect();
        for id in &gone {
            self.state.book.assignments.remove(id);
            self.state.book.packets.remove(id);
        }
        for slot in &dead {
            self.state.book.remaining.remove(slot);
            self.state.book.supply.remove(slot);
        }
        for record in self.resolved.values_mut() {
            record.retained_packet_ids.retain(|id| self.state.book.packets.contains_key(id));
        }
    }

    /// **The forward writer's transition rule for a V3 row.** Either a canonical release of exactly the bindings that left (recomputed
    /// from `old` at its clock), or an ordinary retaining update: the legacy retention rules on the embedded state, released records
    /// unchanged, accepted causal edges unchanged, new edges only for newly bound packets.
    pub fn check_transition_from_v1(&self, old: &Self) -> Result<(), String> {
        let removed: BTreeSet<Hash64> = old.state.claims.keys().filter(|c| !self.state.claims.contains_key(*c)).copied().collect();
        if !removed.is_empty() {
            let now = old.state.book.last_observed_daa.ok_or("service release needs an observed clock")?;
            let mut expect = old.clone();
            expect.release_v1(&removed, now)?;
            if &expect != self {
                return Err("service release differs from the canonical release".into());
            }
            return Ok(());
        }
        self.state.check_retains_unresolved(&old.state)?;
        if self.resolved != old.resolved {
            return Err("released service records are immutable".into());
        }
        let old_owner = old.live_owner();
        for (child, predecessors) in &old.causal {
            if self.causal.get(child) != Some(predecessors) {
                return Err("accepted causal service edges are immutable".into());
            }
        }
        for child in self.causal.keys() {
            if !old.causal.contains_key(child) && old_owner.contains_key(child) {
                return Err("causal edges cannot be added to an accepted binding".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_court_service_state_v2::{ServiceClaimBinding, court_service_profile_id_v2};
    use crate::palw_court_service_v1::{ServiceCost, ServicePacket, ServiceRefusal, ServiceReservations};

    const W: u64 = crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA;

    fn cost() -> ServiceCost {
        ServiceCost { bytes: 1, work: 1, io: 1, controls: 1 }
    }
    fn packet(id: u8, start: u64, end: u64) -> ServicePacket {
        ServicePacket { id: [id; 64], release_daa: start, service_before: end, cost: cost() }
    }
    fn binding(claim: u64, accepted: u64, ids: &[u8]) -> ServiceClaimBinding {
        ServiceClaimBinding {
            claim: Hash64::from_u64_word(claim),
            allocation_id: Hash64::from_u64_word(claim + 100),
            source_id: Hash64::from_u64_word(claim + 200),
            original_accepted_daa: accepted,
            hard_deadline_daa: accepted + W,
            packet_ids: ids.iter().map(|i| [*i; 64]).collect(),
        }
    }
    /// Two claims sharing slot 2 (packets 1 of A and 3 of B), A also owns 2 at slot 3; budget 2 controls per DAA.
    fn two_claims() -> CourtServiceRowV3 {
        let budget = ServiceCost { bytes: 2, work: 2, io: 2, controls: 2 };
        let mut book = ServiceReservations::new(budget).unwrap();
        book.reserve(&[packet(1, 2, 3), packet(2, 3, 4)], W).unwrap();
        book.reserve(&[packet(3, 2, 3)], W + 1).unwrap();
        let a = binding(1, 0, &[1, 2]);
        let b = binding(2, 1, &[3]);
        let src = Hash64::from_u64_word(9);
        let state = CourtServiceStateV2 {
            version: 2,
            profile_id: court_service_profile_id_v2(src),
            source_profile_id: src,
            book,
            claims: BTreeMap::from([(a.claim, a), (b.claim, b)]),
        };
        state.check().unwrap();
        CourtServiceRowV3::from_v2(&state)
    }

    #[test]
    fn r6_d1_release_needs_every_packet_decided_past_hard() {
        let mut row = two_claims();
        let a = Hash64::from_u64_word(1);
        row.state.book.observe_block(2, [7; 64], ServiceCost::default()).unwrap();
        assert!(row.clone().release_v1(&BTreeSet::from([a]), 2).is_err(), "before hard and a packet pending");
        row.state.book.observe_block(W, [8; 64], ServiceCost::default()).unwrap();
        assert!(row.clone().release_v1(&BTreeSet::from([a]), 2).is_err(), "a stale clock is refused");
        let before = row.clone();
        row.release_v1(&BTreeSet::from([a]), W).unwrap();
        row.check().unwrap();
        let record = &row.resolved[&a];
        assert_eq!((record.packets, record.served, record.missing), (2, 1, 1), "slot 2 supplied, slot 3 skipped");
        assert!(!row.state.claims.contains_key(&a));
        // Slot 2 is shared with B's live packet: A's packet 1 stays retained; slot 3 (only A's packet 2) is compacted away.
        assert_eq!(record.retained_packet_ids, BTreeSet::from([[1; 64]]));
        assert!(row.state.book.packet(&[2; 64]).is_none() && row.state.book.assigned_daa(&[2; 64]).is_none());
        assert!(row.state.book.supply_at(2).is_some(), "the shared supplied slot keeps its supply record");
        // The canonical transition rule accepts exactly this release and nothing else.
        row.check_transition_from_v1(&before).unwrap();
        let mut forged = row.clone();
        forged.resolved.get_mut(&a).unwrap().served = 2;
        forged.resolved.get_mut(&a).unwrap().missing = 0;
        assert!(forged.check_transition_from_v1(&before).is_err());
        // Releasing B then compacts slot 2 entirely: supply history and residual quota leave with the last retired packet.
        row.state.book.observe_block(W + 1, [9; 64], ServiceCost::default()).unwrap();
        let b = Hash64::from_u64_word(2);
        row.release_v1(&BTreeSet::from([b]), W + 1).unwrap();
        assert!(row.state.book.packets.is_empty() && row.state.book.supply.is_empty() && row.state.book.remaining.is_empty());
        assert!(row.resolved.values().all(|r| r.retained_packet_ids.is_empty()));
        row.check().unwrap();
        // Borsh round trip of the released form (the new row is rooted as written).
        let bytes = borsh::to_vec(&row).unwrap();
        assert_eq!(borsh::from_slice::<CourtServiceRowV3>(&bytes).unwrap(), row);
    }

    #[test]
    fn r6_d1_legacy_state_encoding_is_unchanged_and_v3_is_refused_in_the_legacy_check() {
        let row = two_claims();
        let mut legacy = row.state.clone();
        legacy.version = 2;
        // The legacy struct is unchanged: its bytes start with version 2 and decode back to it.
        let bytes = borsh::to_vec(&legacy).unwrap();
        assert_eq!(&bytes[..2], &2u16.to_le_bytes());
        assert_eq!(borsh::from_slice::<CourtServiceStateV2>(&bytes).unwrap(), legacy);
        // An unowned packet is refused by the legacy (version 2) check and by the V3 row check.
        let mut orphan = legacy.clone();
        orphan.claims.remove(&Hash64::from_u64_word(2));
        assert!(orphan.check().is_err());
        let mut v3 = CourtServiceRowV3::from_v2(&orphan);
        assert!(v3.check().is_err(), "a V3 row needs a released record for every unowned packet");
        v3.version = 2;
        assert!(v3.check().is_err());
    }

    #[test]
    fn r6_d1_recovery_keeps_causal_order() {
        let edges = BTreeMap::from([([2; 64], vec![[1; 64]])]);
        let ids = BTreeSet::from([[1; 64], [2; 64]]);
        // (1) The missed source (slot 2) can land before its dependent (slot 6): recovered, order kept.
        let mut book = ServiceReservations::new(cost()).unwrap();
        book.reserve(&[packet(1, 2, 20), packet(2, 6, 20)], W).unwrap();
        book.observe_block(4, [1; 64], ServiceCost::default()).unwrap();
        assert_eq!(book.repair_missing_ordered_v1(&ids, W, &edges), Ok(1));
        assert_eq!(book.assigned_daa(&[1; 64]), Some(5));
        assert_eq!(book.assigned_daa(&[2; 64]), Some(6));
        // (2) Source and dependent both missed: the source is placed first, the dependent strictly after it.
        let mut both = ServiceReservations::new(cost()).unwrap();
        both.reserve(&[packet(1, 2, 20), packet(2, 3, 20)], W).unwrap();
        both.observe_block(4, [1; 64], ServiceCost::default()).unwrap();
        assert_eq!(both.repair_missing_ordered_v1(&ids, W, &edges), Ok(2));
        assert!(both.assigned_daa(&[1; 64]).unwrap() < both.assigned_daa(&[2; 64]).unwrap());
        // (3) Every free slot before the dependent is taken: the source could only land after it. Refused, nothing moves (the
        // packet stays MissingSupply -> a no-slash hold), while the old order-blind sweep would invert the order.
        let mut tight = ServiceReservations::new(cost()).unwrap();
        tight.reserve(&[packet(1, 2, 20), packet(3, 5, 6), packet(2, 6, 20)], W).unwrap();
        tight.observe_block(4, [1; 64], ServiceCost::default()).unwrap();
        let before = tight.clone();
        assert_eq!(tight.repair_missing_ordered_v1(&ids, W, &edges), Err(ServiceRefusal::CausalOrder));
        assert_eq!(tight, before, "a refused recovery changes nothing");
        let mut blind = before.clone();
        assert_eq!(blind.repair_missing(&ids, W), Ok(1));
        assert!(blind.assigned_daa(&[1; 64]).unwrap() > blind.assigned_daa(&[2; 64]).unwrap(), "the defect being fixed");
        // (4) Without edges the ordered sweep is exactly the legacy sweep.
        let mut legacy = before.clone();
        assert_eq!(legacy.repair_missing_ordered_v1(&ids, W, &BTreeMap::new()), Ok(1));
        assert_eq!(legacy, blind);
    }

    #[test]
    fn r6_d1_causal_edges_are_checked_and_immutable() {
        let mut row = two_claims();
        // A's packet 2 (slot 3) depends on packet 1 (slot 2): in order.
        row.causal.insert([2; 64], vec![[1; 64]]);
        row.check().unwrap();
        let old = row.clone();
        let mut reversed = row.clone();
        reversed.causal.clear();
        reversed.causal.insert([1; 64], vec![[2; 64]]);
        assert!(reversed.check().is_err(), "child before predecessor");
        let mut cross = row.clone();
        cross.causal.insert([3; 64], vec![[1; 64]]);
        assert!(cross.check().is_err(), "edge across bindings");
        let mut dropped = row.clone();
        dropped.causal.clear();
        assert!(dropped.check_transition_from_v1(&old).is_err());
    }
}
