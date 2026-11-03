//! Prospective, per-frame TIR owner scheduler. Callers supply only already
//! authenticated owner keys and demand transitions from the current frame.
//! This component changes scheduling metadata only. Court state retains the
//! demand, accepted answers, turn balance, deposit and evidence horizon.
//! No existing admission or quote uses this candidate.

use borsh::{BorshDeserialize, BorshSerialize};
use std::{collections::VecDeque, io};

/// Implementation ceiling for the candidate roster, including parked and closed rows.
pub const MAX_ROSTER_V2: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnerScheduleErrorV2 {
    InvalidLimits,
    InvalidClock,
    DuplicateOwner,
    RosterFull,
    ResponseCapacityFull,
    UnknownOwner,
    WrongPhase,
    WrongDemand,
    InvalidWindow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum OwnerPhaseV2<D = u64> {
    Ready,
    Active { until: u64 },
    Waiting { turn: u64, demand: D, respond_until: u64, collect_until: u64 },
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct OwnerRow<K, D> {
    key: K,
    phase: OwnerPhaseV2<D>,
    last_demand: Option<u64>,
}

/// Ready owners and re-entrants are appended to the same FIFO. A key is
/// reserved for the frame's lifetime, including after execution closes.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct OwnerScheduleV2<K, D = u64> {
    roster_limit: usize,
    active_limit: usize,
    response_limit: usize,
    lease_daa: u64,
    now: u64,
    rows: VecDeque<OwnerRow<K, D>>,
}

impl<K: Copy + Eq, D: Copy + Eq> OwnerScheduleV2<K, D> {
    pub fn new(
        roster_limit: usize,
        active_limit: usize,
        response_limit: usize,
        lease_daa: u64,
        now: u64,
    ) -> Result<Self, OwnerScheduleErrorV2> {
        if roster_limit == 0
            || roster_limit > MAX_ROSTER_V2
            || !(1..=roster_limit).contains(&active_limit)
            || !(1..=roster_limit).contains(&response_limit)
            || lease_daa == 0
        {
            return Err(OwnerScheduleErrorV2::InvalidLimits);
        }
        now.checked_add(lease_daa).ok_or(OwnerScheduleErrorV2::InvalidClock)?;
        Ok(Self { roster_limit, active_limit, response_limit, lease_daa, now, rows: VecDeque::with_capacity(roster_limit) })
    }

    pub fn phase(&self, key: K) -> Option<OwnerPhaseV2<D>> {
        self.rows.iter().find(|row| row.key == key).map(|row| row.phase)
    }

    fn index(&self, key: K) -> Result<usize, OwnerScheduleErrorV2> {
        self.rows.iter().position(|row| row.key == key).ok_or(OwnerScheduleErrorV2::UnknownOwner)
    }

    fn fill_active(&mut self) {
        let active = self.rows.iter().filter(|row| matches!(row.phase, OwnerPhaseV2::Active { .. })).count();
        // Constructor and advance validate the addition before any change.
        let until = self.now + self.lease_daa;
        for row in self.rows.iter_mut().filter(|row| row.phase == OwnerPhaseV2::Ready).take(self.active_limit - active) {
            row.phase = OwnerPhaseV2::Active { until };
        }
    }

    /// Update the frame clock before applying that clock's owner actions.
    /// Expired leases rejoin the tail with all owner work intact. A closed
    /// collection window frees the response reservation only. The scheduler
    /// does not decide fault or authorize release of evidence or funds.
    pub fn advance(&mut self, now: u64) -> Result<(), OwnerScheduleErrorV2> {
        if now < self.now || now.checked_add(self.lease_daa).is_none() {
            return Err(OwnerScheduleErrorV2::InvalidClock);
        }
        self.now = now;
        let mut index = 0;
        for _ in 0..self.rows.len() {
            match self.rows[index].phase {
                OwnerPhaseV2::Active { until } if now >= until => {
                    let mut row = self.rows.remove(index).expect("existing row");
                    row.phase = OwnerPhaseV2::Ready;
                    self.rows.push_back(row);
                }
                OwnerPhaseV2::Waiting { collect_until, .. } if now >= collect_until => {
                    self.rows[index].phase = OwnerPhaseV2::Closed;
                    index += 1;
                }
                _ => index += 1,
            }
        }
        self.fill_active();
        Ok(())
    }

    pub fn admit(&mut self, key: K) -> Result<(), OwnerScheduleErrorV2> {
        if self.phase(key).is_some() {
            return Err(OwnerScheduleErrorV2::DuplicateOwner);
        }
        if self.rows.len() == self.roster_limit {
            return Err(OwnerScheduleErrorV2::RosterFull);
        }
        self.rows.push_back(OwnerRow { key, phase: OwnerPhaseV2::Ready, last_demand: None });
        self.fill_active();
        Ok(())
    }

    /// Apply atomically with the verified Court demand. A parked owner
    /// releases execution capacity and keeps one response reservation
    /// through answer or default collection, each with its own window.
    pub fn wait_for_response(
        &mut self,
        key: K,
        turn: u64,
        demand: D,
        respond_until: u64,
        collect_until: u64,
    ) -> Result<(), OwnerScheduleErrorV2> {
        let index = self.index(key)?;
        let row = self.rows[index];
        if !matches!(row.phase, OwnerPhaseV2::Active { .. }) {
            return Err(OwnerScheduleErrorV2::WrongPhase);
        }
        if row.last_demand.is_some_and(|previous| turn <= previous) {
            return Err(OwnerScheduleErrorV2::WrongDemand);
        }
        if self.now >= respond_until || respond_until >= collect_until {
            return Err(OwnerScheduleErrorV2::InvalidWindow);
        }
        if self.rows.iter().filter(|row| matches!(row.phase, OwnerPhaseV2::Waiting { .. })).count() == self.response_limit {
            return Err(OwnerScheduleErrorV2::ResponseCapacityFull);
        }
        self.rows[index].last_demand = Some(turn);
        self.rows[index].phase = OwnerPhaseV2::Waiting { turn, demand, respond_until, collect_until };
        self.fill_active();
        Ok(())
    }

    /// Called after a verified final response part (with `collected=false`)
    /// or a verified default collection (with `collected=true`). Partial
    /// parts keep the same reservation and do not requeue the owner.
    pub fn resume(&mut self, key: K, turn: u64, demand: D, collected: bool) -> Result<(), OwnerScheduleErrorV2> {
        let index = self.index(key)?;
        let OwnerPhaseV2::Waiting { turn: pending, demand: pending_id, respond_until, collect_until } = self.rows[index].phase else {
            return Err(OwnerScheduleErrorV2::WrongPhase);
        };
        if turn != pending || demand != pending_id {
            return Err(OwnerScheduleErrorV2::WrongDemand);
        }
        if self.now >= collect_until || collected != (self.now >= respond_until) {
            return Err(OwnerScheduleErrorV2::InvalidWindow);
        }
        let mut row = self.rows.remove(index).expect("existing row");
        row.phase = OwnerPhaseV2::Ready;
        self.rows.push_back(row);
        self.fill_active();
        Ok(())
    }

    /// Finish a verified owner action that has no outstanding response.
    pub fn finish(&mut self, key: K) -> Result<(), OwnerScheduleErrorV2> {
        let index = self.index(key)?;
        if !matches!(self.rows[index].phase, OwnerPhaseV2::Active { .. }) {
            return Err(OwnerScheduleErrorV2::WrongPhase);
        }
        self.rows[index].phase = OwnerPhaseV2::Closed;
        self.fill_active();
        Ok(())
    }

    /// Number of execution-slot releases sufficient for an already-ready
    /// owner to run. Existing ready owners are the only ones ahead of it.
    /// This excludes response work and makes no assumption about clock cadence.
    pub fn ready_wait_slot_releases(&self, key: K) -> Option<usize> {
        let index = self.index(key).ok()?;
        match self.rows[index].phase {
            OwnerPhaseV2::Active { .. } => Some(0),
            OwnerPhaseV2::Ready => {
                let ahead = self.rows.iter().take(index).filter(|row| row.phase == OwnerPhaseV2::Ready).count();
                ahead.checked_add(1)
            }
            _ => None,
        }
    }
}

fn invalid_snapshot(reason: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, reason)
}

impl<K: BorshDeserialize + Copy + Eq, D: BorshDeserialize + Copy + Eq> BorshDeserialize for OwnerScheduleV2<K, D> {
    fn deserialize_reader<R: io::Read>(reader: &mut R) -> io::Result<Self> {
        let roster_limit = usize::deserialize_reader(reader)?;
        let active_limit = usize::deserialize_reader(reader)?;
        let response_limit = usize::deserialize_reader(reader)?;
        let lease_daa = u64::deserialize_reader(reader)?;
        let now = u64::deserialize_reader(reader)?;
        let count = u32::deserialize_reader(reader)? as usize;
        if count > roster_limit || roster_limit > MAX_ROSTER_V2 {
            return Err(invalid_snapshot("TIR owner snapshot exceeds the roster ceiling"));
        }
        let mut result = Self::new(roster_limit, active_limit, response_limit, lease_daa, now)
            .map_err(|_| invalid_snapshot("invalid TIR owner snapshot limits or clock"))?;
        let mut active = 0;
        let mut ready = 0;
        let mut waiting = 0;
        for _ in 0..count {
            let row = OwnerRow::deserialize_reader(reader)?;
            if result.phase(row.key).is_some() {
                return Err(invalid_snapshot("duplicate TIR owner snapshot key"));
            }
            match row.phase {
                OwnerPhaseV2::Active { until } if now < until && until <= now + lease_daa => active += 1,
                OwnerPhaseV2::Ready => ready += 1,
                OwnerPhaseV2::Waiting { turn, respond_until, collect_until, .. }
                    if row.last_demand == Some(turn) && respond_until < collect_until && now < collect_until =>
                {
                    waiting += 1;
                }
                OwnerPhaseV2::Closed => {}
                _ => return Err(invalid_snapshot("TIR owner snapshot has an invalid phase or deadline")),
            }
            result.rows.push_back(row);
        }
        if active > active_limit || waiting > response_limit || (ready > 0 && active < active_limit) {
            return Err(invalid_snapshot("TIR owner snapshot exceeds its phase capacities"));
        }
        Ok(result)
    }
}

include!("palw_tir_owner_schedule_rooted_v2.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use OwnerPhaseV2::*;
    use OwnerScheduleErrorV2::*;

    #[test]
    fn parked_owner_yields_and_rejoins_behind_existing_ready_owners() {
        let mut schedule = OwnerScheduleV2::new(4, 1, 1, 3, 100).unwrap();
        for key in 1..=4 {
            schedule.admit(key).unwrap();
        }
        assert_eq!(schedule.ready_wait_slot_releases(4), Some(3));
        schedule.wait_for_response(1, 7, 107u64, 108, 112).unwrap();
        assert_eq!(schedule.phase(1), Some(Waiting { turn: 7, demand: 107, respond_until: 108, collect_until: 112 }));
        assert_eq!(schedule.phase(2), Some(Active { until: 103 }));
        schedule.advance(101).unwrap();
        assert_eq!(schedule.resume(1, 6, 106u64, false), Err(WrongDemand));
        assert_eq!(schedule.resume(1, 7, 199u64, false), Err(WrongDemand));
        assert_eq!(schedule.resume(1, 7, 107u64, true), Err(InvalidWindow));
        schedule.resume(1, 7, 107u64, false).unwrap();
        assert_eq!(schedule.ready_wait_slot_releases(1), Some(3));
        schedule.advance(103).unwrap();
        assert_eq!(schedule.phase(3), Some(Active { until: 106 }));
        schedule.finish(3).unwrap();
        assert_eq!(schedule.phase(4), Some(Active { until: 106 }));
        schedule.finish(4).unwrap();
        assert_eq!(schedule.phase(1), Some(Active { until: 106 }));
        assert_eq!(schedule.wait_for_response(1, 7, 107u64, 108, 112), Err(WrongDemand));
        schedule.wait_for_response(1, 8, 108u64, 108, 112).unwrap();
    }

    #[test]
    fn waiting_response_has_separate_cap_and_nonoverlapping_windows() {
        let mut schedule = OwnerScheduleV2::new(3, 2, 1, 2, 10).unwrap();
        for key in 1..=3 {
            schedule.admit(key).unwrap();
        }
        schedule.wait_for_response(1, 0, 100u64, 20, 25).unwrap();
        let parked = schedule.clone();
        assert_eq!(schedule.wait_for_response(2, 0, 100u64, 20, 25), Err(ResponseCapacityFull));
        assert_eq!(schedule.finish(1), Err(WrongPhase));
        assert_eq!(schedule, parked);

        schedule.advance(20).unwrap();
        assert_eq!(schedule.resume(1, 0, 100u64, false), Err(InvalidWindow));
        schedule.resume(1, 0, 100u64, true).unwrap();
        assert_eq!(schedule.phase(1), Some(Ready));
        schedule.wait_for_response(2, 0, 100u64, 22, 25).unwrap();
        schedule.advance(25).unwrap();
        assert_eq!(schedule.phase(2), Some(Closed));
        assert_eq!(schedule.resume(2, 0, 100u64, true), Err(WrongPhase));
        assert_eq!(schedule.admit(2), Err(DuplicateOwner));
        assert_eq!(schedule.admit(4), Err(RosterFull));
        schedule.wait_for_response(1, 1, 101u64, 27, 30).unwrap();
    }

    #[test]
    fn restoring_prior_generation_replays_slots_and_waits_deterministically() {
        let mut schedule = OwnerScheduleV2::new(6, 2, 2, 2, 30).unwrap();
        for key in 1..=6 {
            schedule.admit(key).unwrap();
        }
        schedule.wait_for_response(1, 4, 104u64, 33, 37).unwrap();
        let checkpoint = schedule.clone();

        schedule.advance(32).unwrap();
        assert_eq!(schedule.phase(4), Some(Active { until: 34 }));
        assert_eq!(schedule.phase(5), Some(Active { until: 34 }));
        schedule.resume(1, 4, 104u64, false).unwrap();

        let mut replay = checkpoint.clone();
        replay.advance(32).unwrap();
        replay.resume(1, 4, 104u64, false).unwrap();
        assert_eq!(replay, schedule);

        let mut alternate = checkpoint;
        alternate.advance(34).unwrap();
        assert_eq!(alternate.resume(1, 4, 104u64, false), Err(InvalidWindow));
        alternate.resume(1, 4, 104u64, true).unwrap();
        assert_ne!(alternate, schedule);
        assert_eq!(alternate.phase(1), Some(Ready));
    }

    #[test]
    fn checkpoint_preserves_response_identity_and_bounds_the_roster_before_reading_rows() {
        let mut schedule = OwnerScheduleV2::new(3, 1, 2, 4, 20).unwrap();
        schedule.admit(7u64).unwrap();
        schedule.admit(9).unwrap();
        schedule.wait_for_response(7, 1, 151u64, 25, 30).unwrap();
        let mut bytes = borsh::to_vec(&schedule).unwrap();
        let restored: OwnerScheduleV2<u64> = borsh::from_slice(&bytes).unwrap();
        assert_eq!(restored, schedule);

        // Exactly five u64 header fields precede the Borsh count.
        bytes[40..44].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(borsh::from_slice::<OwnerScheduleV2<u64>>(&bytes).unwrap_err().to_string().contains("roster ceiling"));
    }

    #[test]
    fn roster_and_clock_boundaries_fail_without_mutating_state() {
        assert_eq!(OwnerScheduleV2::<u8>::new(MAX_ROSTER_V2 + 1, 1, 1, 1, 0), Err(InvalidLimits));
        assert_eq!(OwnerScheduleV2::<u8>::new(1, 1, 1, 2, u64::MAX - 1), Err(InvalidClock));
        let mut schedule = OwnerScheduleV2::new(MAX_ROSTER_V2, 8, 8, 2, 12).unwrap();
        for key in 0..MAX_ROSTER_V2 {
            schedule.admit(key).unwrap();
        }
        let initial = schedule.clone();
        assert_eq!(schedule.advance(11), Err(InvalidClock));
        assert_eq!(schedule.advance(u64::MAX), Err(InvalidClock));
        assert_eq!(schedule.admit(MAX_ROSTER_V2), Err(RosterFull));
        assert_eq!(schedule.wait_for_response(0, 0, 100u64, 12, 13), Err(InvalidWindow));
        assert_eq!(schedule, initial);

        let last = MAX_ROSTER_V2 - 1;
        assert_eq!(schedule.ready_wait_slot_releases(last), Some(MAX_ROSTER_V2 - 8));
        for daa in (14..=26).step_by(2) {
            schedule.advance(daa).unwrap();
        }
        assert_eq!(schedule.phase(last), Some(Active { until: 28 }));
    }
}
