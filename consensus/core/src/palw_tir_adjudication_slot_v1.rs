//! **ADR-0191 admission (DORMANT, pure rules): one adjudication in flight per claim.**
//!
//! A claim has one adjudication slot. An owner asks for it; if the slot is held, the owner queues in arrival order. The holder must
//! finish within `budget_daa` (the adjudication's own span, [`crate::palw_tir_frame_cost_v1`] prices it); at that deadline the slot is
//! reclaimed at the deadline itself (not at the observation time), so the state is a pure function of the events and `now`.
//! A reclaimed holder re-queues at the tail, at most `PALW_TIR_ADJUDICATION_MAX_REOCCUPATIONS_V1` times; then it is refused the slot
//! (its own request is answered by the designated-answer rules, not by this slot).
//!
//! Worst wait for the owner at queue position `p` (1-based) is `(p - 1) · budget_daa`, because every earlier holder can take at most one
//! budget. With `K` owners per claim that is `(K - 1) · budget_daa`.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Slots per claim. Fixed at one: the door is priced for one adjudication carrier at a time (ADR-0191 M = 1).
pub const PALW_TIR_ADJUDICATION_SLOTS_PER_CLAIM_V1: usize = 1;
/// How many times one owner may be reclaimed from the slot and re-queued before it is refused.
pub const PALW_TIR_ADJUDICATION_MAX_REOCCUPATIONS_V1: u32 = 1;

/// Maximum wait for a position counting the holder as position 1.
/// The quote and the scheduler share this definition.
pub fn palw_tir_adjudication_wait_v1(position: u64, budget_daa: u64) -> Option<u64> {
    position.checked_sub(1)?.checked_mul(budget_daa)
}

/// The result of asking for the slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwTirSlotGrantV1 {
    /// The owner holds the slot from `now`.
    Granted,
    /// The slot is held; the owner waits at this 1-based position.
    Queued { position: usize },
    /// The owner is already queued or holding; nothing changes (no double entry).
    AlreadyPresent,
    /// The owner already completed, exhausted its reoccupations, or the live roster is full.
    Refused,
}

/// A claim's adjudication slot: holder, FIFO queue, reclaim counts. All transitions take the DAA time they happen at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwTirAdjudicationSlotV1<O: Ord + Clone> {
    budget_daa: u64,
    maximum_owners: usize,
    holder: Option<(O, u64)>,
    queue: VecDeque<O>,
    reclaims: BTreeMap<O, u32>,
    retired: BTreeSet<O>,
}

impl<O: Ord + Clone> PalwTirAdjudicationSlotV1<O> {
    /// The cap covers the holder and queued owners together. A caller must
    /// still admit each owner through the claim's own roster and funding rules.
    pub fn new(budget_daa: u64, maximum_owners: usize) -> Option<Self> {
        if budget_daa == 0 || maximum_owners == 0 {
            return None;
        }
        Some(Self {
            budget_daa,
            maximum_owners,
            holder: None,
            queue: VecDeque::new(),
            reclaims: BTreeMap::new(),
            retired: BTreeSet::new(),
        })
    }

    /// Apply every reclaim whose deadline is at or before `now`. Deterministic: the reclaim is dated at the holder's deadline.
    fn advance(&mut self, now: u64) {
        while let Some((holder, start)) = self.holder.clone() {
            let deadline = start.saturating_add(self.budget_daa);
            if now < deadline {
                break;
            }
            let count = self.reclaims.entry(holder.clone()).or_insert(0);
            *count += 1;
            if *count > PALW_TIR_ADJUDICATION_MAX_REOCCUPATIONS_V1 {
                self.retired.insert(holder);
            } else {
                self.queue.push_back(holder);
            }
            self.holder = None;
            if let Some(next) = self.queue.pop_front() {
                self.holder = Some((next, deadline));
            }
        }
    }

    /// The owner asks for the slot at `now`.
    pub fn request(&mut self, owner: O, now: u64) -> PalwTirSlotGrantV1 {
        self.advance(now);
        if self.retired.contains(&owner) {
            return PalwTirSlotGrantV1::Refused;
        }
        if self.holder.as_ref().is_some_and(|(h, _)| *h == owner) || self.queue.contains(&owner) {
            return PalwTirSlotGrantV1::AlreadyPresent;
        }
        if self.holder.is_none() && PALW_TIR_ADJUDICATION_SLOTS_PER_CLAIM_V1 == 1 {
            self.holder = Some((owner, now));
            return PalwTirSlotGrantV1::Granted;
        }
        if self.queue.len() >= self.maximum_owners - 1 {
            return PalwTirSlotGrantV1::Refused;
        }
        self.queue.push_back(owner);
        PalwTirSlotGrantV1::Queued { position: self.queue.len() + 1 }
    }

    /// Complete the exact grant at `now`. The next queued owner takes the slot at `now`.
    /// Binding the start prevents an expired carrier from completing a later reoccupation.
    pub fn complete(&mut self, owner: &O, granted_daa: u64, now: u64) -> Option<O> {
        self.advance(now);
        if self.holder.as_ref().is_some_and(|(h, start)| h == owner && *start == granted_daa && now >= *start) {
            self.retired.insert(owner.clone());
            self.holder = None;
            if let Some(next) = self.queue.pop_front() {
                self.holder = Some((next.clone(), now));
                return Some(next);
            }
        }
        None
    }

    /// The current holder and grant start, after applying reclaims up to `now`.
    pub fn holder_at(&mut self, now: u64) -> Option<(O, u64)> {
        self.advance(now);
        self.holder.as_ref().filter(|(_, start)| now >= *start).cloned()
    }

    /// The worst wait for a 1-based queue position: `(position - 1) · budget`.
    pub fn worst_wait_daa(&self, position: usize) -> Option<u64> {
        if position > self.maximum_owners {
            return None;
        }
        palw_tir_adjudication_wait_v1(u64::try_from(position).ok()?, self.budget_daa)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_owner_is_queued_while_the_slot_is_held_and_m_is_one() {
        let mut s = PalwTirAdjudicationSlotV1::new(347, 8).unwrap();
        assert_eq!(s.request(1u8, 0), PalwTirSlotGrantV1::Granted);
        assert_eq!(s.request(2u8, 10), PalwTirSlotGrantV1::Queued { position: 2 });
        assert_eq!(s.request(3u8, 11), PalwTirSlotGrantV1::Queued { position: 3 });
        assert_eq!(s.holder_at(12), Some((1, 0)), "M = 1: one holder at a time");
    }

    #[test]
    fn completion_grants_the_queue_in_arrival_order() {
        let mut s = PalwTirAdjudicationSlotV1::new(347, 8).unwrap();
        s.request(1u8, 0);
        s.request(2u8, 1);
        s.request(3u8, 2);
        assert_eq!(s.complete(&1, 0, 100), Some(2));
        assert_eq!(s.holder_at(101), Some((2, 100)));
        assert_eq!(s.complete(&2, 100, 150), Some(3));
    }

    #[test]
    fn a_duplicate_request_is_not_a_second_entry() {
        let mut s = PalwTirAdjudicationSlotV1::new(347, 8).unwrap();
        s.request(1u8, 0);
        s.request(2u8, 1);
        assert_eq!(s.request(2u8, 2), PalwTirSlotGrantV1::AlreadyPresent);
        assert_eq!(s.request(1u8, 3), PalwTirSlotGrantV1::AlreadyPresent);
    }

    #[test]
    fn a_silent_holder_is_reclaimed_at_its_deadline_and_the_next_owner_takes_over_there() {
        let mut s = PalwTirAdjudicationSlotV1::new(347, 8).unwrap();
        s.request(1u8, 0);
        s.request(2u8, 1);
        assert_eq!(s.holder_at(346), Some((1, 0)));
        assert_eq!(s.holder_at(347), Some((2, 347)), "the next owner receives the full budget at the deadline");
        assert_eq!(s.holder_at(693), Some((2, 347)));
        assert_eq!(s.holder_at(694), Some((1, 694)));
        assert_eq!(s.worst_wait_daa(2), Some(347));
    }

    #[test]
    fn malicious_reoccupation_is_capped_and_then_refused() {
        let mut s = PalwTirAdjudicationSlotV1::new(100, 8).unwrap();
        s.request(1u8, 0);
        s.request(2u8, 1);
        // 1 is reclaimed once (back to the tail), then holds again only by taking the slot from 2 at its deadline.
        assert_eq!(s.holder_at(101), Some((2, 100)));
        assert_eq!(s.holder_at(201), Some((1, 200)), "re-occupation 1 is allowed once");
        // 1 now holds again and goes silent: its second reclaim refuses it (no unbounded re-occupation).
        s.holder_at(10_000);
        assert_eq!(s.request(1u8, 10_000), PalwTirSlotGrantV1::Refused, "a second reclaim refuses the occupier");
    }

    #[test]
    fn a_stale_completion_cannot_release_the_same_owners_new_grant() {
        let mut s = PalwTirAdjudicationSlotV1::new(100, 2).unwrap();
        s.request(1u8, 0);
        assert_eq!(s.holder_at(100), Some((1, 100)));
        s.request(2u8, 100);
        assert_eq!(s.complete(&1, 0, 101), None);
        assert_eq!(s.holder_at(101), Some((1, 100)));
        assert_eq!(s.complete(&1, 100, 102), Some(2));
        assert_eq!(s.request(1, 102), PalwTirSlotGrantV1::Refused, "one completion ends this owner's adjudication");
    }

    #[test]
    fn the_worst_wait_for_k_owners_is_k_minus_one_budgets() {
        let s = PalwTirAdjudicationSlotV1::<u8>::new(347, 8).unwrap();
        assert_eq!(s.worst_wait_daa(8), Some(7 * 347), "K = 8: (K-1)·budget");
    }

    #[test]
    fn the_live_roster_is_capped_and_the_last_admitted_owner_gets_the_quoted_wait() {
        let mut slot = PalwTirAdjudicationSlotV1::new(100, 8).unwrap();
        assert_eq!(slot.request(1, 0), PalwTirSlotGrantV1::Granted);
        for owner in 2..=8 {
            assert_eq!(slot.request(owner, 0), PalwTirSlotGrantV1::Queued { position: owner });
        }
        assert_eq!(slot.request(9, 0), PalwTirSlotGrantV1::Refused);
        assert_eq!(slot.worst_wait_daa(9), None);
        assert_eq!(slot.holder_at(slot.worst_wait_daa(8).unwrap()), Some((8, 700)));
        assert_eq!(slot.complete(&8, 700, 700), Some(1));
        assert_eq!(slot.request(9, 700), PalwTirSlotGrantV1::Queued { position: 8 });
        assert!(PalwTirAdjudicationSlotV1::<u8>::new(0, 8).is_none());
        assert!(PalwTirAdjudicationSlotV1::<u8>::new(100, 0).is_none());
    }

    #[test]
    fn the_slot_state_is_a_pure_function_of_the_events_and_time() {
        let mut a = PalwTirAdjudicationSlotV1::new(347, 8).unwrap();
        let mut b = a.clone();
        for s in [&mut a, &mut b] {
            s.request(1u8, 0);
            s.request(2u8, 5);
            s.request(3u8, 6);
        }
        for now in [10, 347, 348, 700, 2_000] {
            assert_eq!(a.clone().holder_at(now), b.clone().holder_at(now));
        }
    }
}
