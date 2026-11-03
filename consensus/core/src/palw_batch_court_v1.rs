//! Candidate batch Court transport contract. A range identifies material in an
//! accepted source; the native caller must independently verify its opening
//! and the responder's authority. Transport completeness grants no credit.
use crate::Hash64;
use crate::palw_court_policy_v1::{DISPUTE_WINDOW_DAA, NOTICE_WINDOW_DAA, shared_initial_notice_daa_in_window_v1};
use borsh::BorshSerialize;
use kaspa_hashes::blake2b_512_keyed;

pub const MAX_BATCH_RANGES_V1: usize = 64;
pub const MAX_BATCH_PARTS_V1: usize = 256;
pub const MAX_BATCH_PART_BYTES_V1: u32 = 16_384;

/// Prospective clocks for an admitted dispute. The accepting fold must check
/// the opening's standing, bond, and actionable evidence before constructing
/// this record. Clock validation alone does not admit a dispute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize)]
pub struct BatchCourtWindowV1 {
    pub claim_accepted_daa: u64,
    pub challenge_opened_daa: u64,
    /// Exclusive collection boundary, at most one past the inclusive hard deadline.
    pub collect_until_daa: u64,
    pub material_until_daa: u64,
}

impl BatchCourtWindowV1 {
    pub fn check(self) -> Result<(), BatchCourtErrorV1> {
        let timely = shared_initial_notice_daa_in_window_v1(self.claim_accepted_daa, self.challenge_opened_daa)
            .map_err(|_| BatchCourtErrorV1::Clock)?;
        let maximum_collect_until = self
            .claim_accepted_daa
            .checked_add(DISPUTE_WINDOW_DAA)
            .and_then(|hard| hard.checked_add(1))
            .ok_or(BatchCourtErrorV1::Clock)?;
        if !timely
            || self.collect_until_daa <= self.challenge_opened_daa
            || self.collect_until_daa > maximum_collect_until
            || self.material_until_daa < self.collect_until_daa
        {
            return Err(BatchCourtErrorV1::Window);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize)]
pub struct BatchRangeV1 {
    /// Canonical material identity, including its source root and coordinates.
    pub id: Hash64,
    /// Exact bytes after the source-specific encoding is fixed.
    pub bytes: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct BatchDemandV1 {
    pub case_id: Hash64,
    pub case_window: BatchCourtWindowV1,
    pub opened_daa: u64,
    pub response_deadline_daa: u64,
    pub maximum_bytes: u64,
    pub part_bytes: u32,
    /// Strict ascending material identities. Dependencies between rounds are
    /// resolved before building a demand. Ranges within it can arrive independently.
    pub ranges: Vec<BatchRangeV1>,
}

#[derive(Clone, Copy, Debug)]
pub struct BatchResponsePartV1<'a> {
    pub case_id: Hash64,
    pub request_root: Hash64,
    pub range_id: Hash64,
    pub part_index: u32,
    pub bytes: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatchPartOutcomeV1 {
    Applied,
    Duplicate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatchProgressStatusV1 {
    AwaitingMaterial,
    AllMaterialVerified,
    TimedOutUnverified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatchCourtErrorV1 {
    Demand,
    Identity,
    PartShape,
    Conflict,
    Window,
    Opening,
    Checkpoint,
    Clock,
    OverTarget { required_daa: u64 },
}

impl BatchDemandV1 {
    fn check(&self) -> Result<usize, BatchCourtErrorV1> {
        use BatchCourtErrorV1::Demand;
        self.case_window.check()?;
        if self.case_id == Hash64::default()
            || self.opened_daa < self.case_window.challenge_opened_daa
            || self.response_deadline_daa <= self.opened_daa
            || self.response_deadline_daa >= self.case_window.collect_until_daa
            || !(1..=MAX_BATCH_PART_BYTES_V1).contains(&self.part_bytes)
            || !(1..=MAX_BATCH_RANGES_V1).contains(&self.ranges.len())
            || self.ranges.windows(2).any(|pair| pair[0].id >= pair[1].id)
        {
            return Err(Demand);
        }
        let mut bytes = 0u64;
        let mut parts = 0u64;
        for range in &self.ranges {
            if range.bytes == 0 || range.id == Hash64::default() {
                return Err(Demand);
            }
            bytes += u64::from(range.bytes);
            parts += u64::from(range.bytes.div_ceil(self.part_bytes));
        }
        if bytes > self.maximum_bytes || parts > MAX_BATCH_PARTS_V1 as u64 {
            return Err(Demand);
        }
        Ok(parts as usize)
    }

    fn root(&self) -> Result<Hash64, BatchCourtErrorV1> {
        let bytes = borsh::to_vec(self).map_err(|_| BatchCourtErrorV1::Demand)?;
        Ok(blake2b_512_keyed(b"misaka-palw/batch-court/demand/v1", &bytes))
    }
}

/// A bounded bitmap with a content digest per accepted slot. Evidence bytes
/// stay in their independently bounded archive. Callers stage this value with
/// archive writes and accounting in the same commit before acknowledging a part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BatchProgressV1 {
    demand: BatchDemandV1,
    request_root: Hash64,
    accepted: Vec<Option<Hash64>>,
}

impl BatchProgressV1 {
    pub fn new(demand: BatchDemandV1) -> Result<Self, BatchCourtErrorV1> {
        let parts = demand.check()?;
        let request_root = demand.root()?;
        Ok(Self { demand, request_root, accepted: vec![None; parts] })
    }

    pub fn request_root(&self) -> Hash64 {
        self.request_root
    }

    /// Stable, canonical part order for a branch-owned checkpoint.
    pub fn checkpoint(&self) -> &[Option<Hash64>] {
        &self.accepted
    }

    pub fn checkpoint_root(&self) -> Result<Hash64, BatchCourtErrorV1> {
        let bytes = borsh::to_vec(&(self.request_root, &self.accepted)).map_err(|_| BatchCourtErrorV1::Checkpoint)?;
        Ok(blake2b_512_keyed(b"misaka-palw/batch-court/progress/v1", &bytes))
    }

    /// Restore only a checkpoint authenticated by the caller's accepted branch.
    /// The archive must retain each verified part on that same branch.
    pub fn restore(
        demand: BatchDemandV1,
        request_root: Hash64,
        accepted: &[Option<Hash64>],
        expected_checkpoint_root: Hash64,
    ) -> Result<Self, BatchCourtErrorV1> {
        let mut progress = Self::new(demand)?;
        if progress.request_root != request_root || progress.accepted.len() != accepted.len() {
            return Err(BatchCourtErrorV1::Checkpoint);
        }
        progress.accepted.copy_from_slice(accepted);
        if progress.checkpoint_root()? != expected_checkpoint_root {
            return Err(BatchCourtErrorV1::Checkpoint);
        }
        Ok(progress)
    }

    pub fn status(&self, daa: u64) -> BatchProgressStatusV1 {
        if self.accepted.iter().all(Option::is_some) {
            BatchProgressStatusV1::AllMaterialVerified
        } else if daa >= self.demand.response_deadline_daa {
            BatchProgressStatusV1::TimedOutUnverified
        } else {
            BatchProgressStatusV1::AwaitingMaterial
        }
    }

    pub fn remaining_payload_bytes(&self) -> u64 {
        let mut slots = self.accepted.iter();
        let size = self.demand.part_bytes;
        self.demand
            .ranges
            .iter()
            .map(|range| {
                (0..range.bytes.div_ceil(size))
                    .filter_map(|part| slots.next().expect("validated part count").is_none().then_some(part))
                    .map(|part| u64::from((range.bytes - part * size).min(size)))
                    .sum::<u64>()
            })
            .sum()
    }

    /// Parts may arrive in any order. Exact duplicates consume no new slot.
    /// The caller must authenticate the envelope and account bounded ingress
    /// work even for duplicate packets. The verifier authenticates the literal
    /// bytes for this range and offset against the independently accepted source.
    pub fn accept(
        &mut self,
        daa: u64,
        part: BatchResponsePartV1<'_>,
        verify_opening: impl FnOnce(BatchRangeV1, u32, &[u8]) -> bool,
    ) -> Result<BatchPartOutcomeV1, BatchCourtErrorV1> {
        use BatchCourtErrorV1::{Conflict, Identity, Opening, PartShape, Window};
        if part.case_id != self.demand.case_id || part.request_root != self.request_root {
            return Err(Identity);
        }
        let range_index = self.demand.ranges.binary_search_by_key(&part.range_id, |range| range.id).map_err(|_| Identity)?;
        let range = self.demand.ranges[range_index];
        let size = self.demand.part_bytes;
        let offset = part.part_index.checked_mul(size).filter(|offset| *offset < range.bytes).ok_or(PartShape)?;
        if part.bytes.len() != (range.bytes - offset).min(size) as usize {
            return Err(PartShape);
        }
        if daa < self.demand.opened_daa || daa >= self.demand.response_deadline_daa {
            return Err(Window);
        }
        let slot = self.demand.ranges[..range_index].iter().map(|range| range.bytes.div_ceil(size) as usize).sum::<usize>()
            + part.part_index as usize;
        let content = blake2b_512_keyed(b"misaka-palw/batch-court/part/v1", part.bytes);
        match self.accepted[slot] {
            Some(previous) if previous == content => return Ok(BatchPartOutcomeV1::Duplicate),
            Some(_) => return Err(Conflict),
            None => {}
        }
        if !verify_opening(range, offset, part.bytes) {
            return Err(Opening);
        }
        self.accepted[slot] = Some(content);
        Ok(BatchPartOutcomeV1::Applied)
    }
}

/// Separate budgets for actual admitted wire bytes and validated service.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatchCourtClockV1 {
    pub preparation_daa: u64,
    pub owner_wait_daa: u64,
    /// Sum of measured response/inclusion waits along dependent rounds.
    pub serial_round_daa: u64,
    /// Encoded bytes including framing, authentication, retries and all custodians.
    pub wire_bytes: u64,
    pub guaranteed_wire_bytes_per_daa: u64,
    /// Conservative end-to-end Node verification allowance under its own
    /// independently enforced CPU, IO and concurrent work budgets.
    pub node_service_daa: u64,
    pub filing_daa: u64,
    pub recovery_daa: u64,
}

impl BatchCourtClockV1 {
    /// Prospective total since claim acceptance. No overlap is assumed
    /// between these terms. The caller must justify the measured guarantees
    /// and reserve a separate liability/retention tail before activation.
    pub fn door_daa(self, open_elapsed_daa: u64) -> Result<u64, BatchCourtErrorV1> {
        if open_elapsed_daa > NOTICE_WINDOW_DAA {
            return Err(BatchCourtErrorV1::Window);
        }
        if self.wire_bytes == 0 || self.guaranteed_wire_bytes_per_daa == 0 || self.serial_round_daa == 0 || self.node_service_daa == 0
        {
            return Err(BatchCourtErrorV1::Clock);
        }
        let required_daa = [
            open_elapsed_daa,
            self.preparation_daa,
            self.owner_wait_daa,
            self.serial_round_daa,
            self.wire_bytes.div_ceil(self.guaranteed_wire_bytes_per_daa),
            self.node_service_daa,
            self.filing_daa,
            self.recovery_daa,
            1, // T06: reserve the exclusive collection boundary margin.
        ]
        .into_iter()
        .try_fold(0u64, u64::checked_add)
        .ok_or(BatchCourtErrorV1::Clock)?;
        if required_daa > DISPUTE_WINDOW_DAA {
            return Err(BatchCourtErrorV1::OverTarget { required_daa });
        }
        Ok(required_daa)
    }

    /// Profile gate including the last allowed opening. A timely challenger
    /// must have the same reserved service opportunity as an early one.
    pub fn worst_case_door_daa(self) -> Result<u64, BatchCourtErrorV1> {
        self.door_daa(NOTICE_WINDOW_DAA)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demand() -> BatchDemandV1 {
        let mut ranges =
            vec![BatchRangeV1 { id: Hash64::from_u64_word(1), bytes: 6 }, BatchRangeV1 { id: Hash64::from_u64_word(2), bytes: 3 }];
        ranges.sort_by_key(|range| range.id);
        BatchDemandV1 {
            case_id: Hash64::from_u64_word(9),
            case_window: BatchCourtWindowV1 {
                claim_accepted_daa: 1,
                challenge_opened_daa: 8,
                collect_until_daa: 25,
                material_until_daa: 30,
            },
            opened_daa: 10,
            response_deadline_daa: 20,
            maximum_bytes: 9,
            part_bytes: 4,
            ranges,
        }
    }

    fn part(progress: &BatchProgressV1, range: u64, part_index: u32, bytes: &'static [u8]) -> BatchResponsePartV1<'static> {
        BatchResponsePartV1 {
            case_id: progress.demand.case_id,
            request_root: progress.request_root(),
            range_id: Hash64::from_u64_word(range),
            part_index,
            bytes,
        }
    }

    #[test]
    fn out_of_order_and_restart_have_same_verified_checkpoint() {
        let mut progress = BatchProgressV1::new(demand()).unwrap();
        let a = part(&progress, 1, 0, b"abcd");
        let b = part(&progress, 1, 1, b"ef");
        let c = part(&progress, 2, 0, b"xyz");
        assert_eq!(progress.accept(11, b, |_, offset, bytes| offset == 4 && bytes == b"ef"), Ok(BatchPartOutcomeV1::Applied));
        assert_eq!(progress.remaining_payload_bytes(), 7);
        let checkpoint = progress.checkpoint_root().unwrap();
        let mut resumed = BatchProgressV1::restore(demand(), progress.request_root(), progress.checkpoint(), checkpoint).unwrap();
        assert_eq!(resumed.accept(12, b, |_, _, _| panic!("duplicate verification")), Ok(BatchPartOutcomeV1::Duplicate));
        assert_eq!(resumed.checkpoint_root().unwrap(), checkpoint);
        assert_eq!(
            BatchProgressV1::restore(demand(), resumed.request_root(), &[None; 3], checkpoint),
            Err(BatchCourtErrorV1::Checkpoint)
        );
        for response in [c, a] {
            resumed.accept(19, response, |_, _, _| true).unwrap();
        }
        let mut ordered = BatchProgressV1::new(demand()).unwrap();
        for response in [a, b, c] {
            ordered.accept(19, response, |_, _, _| true).unwrap();
        }
        assert_eq!(resumed.checkpoint_root(), ordered.checkpoint_root());
        assert_eq!(resumed.remaining_payload_bytes(), 0);
        assert_eq!(resumed.status(20), BatchProgressStatusV1::AllMaterialVerified);
        assert_eq!(progress.status(20), BatchProgressStatusV1::TimedOutUnverified);
    }

    #[test]
    fn refuses_overbudget_identity_invalid_openings_conflicts_and_late_parts() {
        let mut too_large = demand();
        too_large.maximum_bytes -= 1;
        assert_eq!(BatchProgressV1::new(too_large), Err(BatchCourtErrorV1::Demand));
        let mut too_many = demand();
        too_many.ranges[0].bytes = 2_000;
        too_many.maximum_bytes = 3_000;
        assert_eq!(BatchProgressV1::new(too_many), Err(BatchCourtErrorV1::Demand));
        let mut progress = BatchProgressV1::new(demand()).unwrap();
        let a = part(&progress, 1, 0, b"abcd");
        let bad = BatchResponsePartV1 { request_root: Hash64::default(), ..a };
        assert_eq!(progress.accept(10, bad, |_, _, _| true), Err(BatchCourtErrorV1::Identity));
        let bad = BatchResponsePartV1 { bytes: b"abcde", ..a };
        assert_eq!(progress.accept(10, bad, |_, _, _| true), Err(BatchCourtErrorV1::PartShape));
        assert_eq!(progress.accept(10, a, |_, _, _| false), Err(BatchCourtErrorV1::Opening));
        assert_eq!(progress.remaining_payload_bytes(), 9);
        assert_eq!(progress.accept(9, a, |_, _, _| true), Err(BatchCourtErrorV1::Window));
        assert_eq!(progress.accept(20, a, |_, _, _| true), Err(BatchCourtErrorV1::Window));
        progress.accept(10, a, |_, _, _| true).unwrap();
        let other = BatchResponsePartV1 { bytes: b"pqrs", ..a };
        assert_eq!(progress.accept(11, other, |_, _, _| true), Err(BatchCourtErrorV1::Conflict));
        assert_eq!(progress.remaining_payload_bytes(), 5);
    }

    #[test]
    fn a_timely_open_can_use_the_extended_window_without_renewing_its_end() {
        let case = BatchCourtWindowV1 {
            claim_accepted_daa: 100,
            challenge_opened_daa: 3_220,
            collect_until_daa: 10_101,
            material_until_daa: 10_101,
        };
        assert_eq!(case.check(), Ok(()));
        assert_eq!(BatchCourtWindowV1 { challenge_opened_daa: 3_221, ..case }.check(), Err(BatchCourtErrorV1::Window));
        assert_eq!(
            BatchCourtWindowV1 { collect_until_daa: 10_102, material_until_daa: 20_000, ..case }.check(),
            Err(BatchCourtErrorV1::Window)
        );
        assert_eq!(BatchCourtWindowV1 { material_until_daa: 10_100, ..case }.check(), Err(BatchCourtErrorV1::Window));
        let mut late = demand();
        late.response_deadline_daa = late.case_window.collect_until_daa;
        assert_eq!(BatchProgressV1::new(late), Err(BatchCourtErrorV1::Demand));
    }

    #[test]
    fn the_prospective_clock_preserves_over_target_and_overflow() {
        let clock = BatchCourtClockV1 {
            preparation_daa: 500,
            owner_wait_daa: 800,
            serial_round_daa: 1_400,
            wire_bytes: 2_400_000,
            guaranteed_wire_bytes_per_daa: 1_000,
            node_service_daa: 1_600,
            filing_daa: 1_200,
            recovery_daa: 1_100,
        };
        assert_eq!(clock.door_daa(0), Ok(9_001));
        assert_eq!(clock.door_daa(999), Ok(10_000));
        assert_eq!(clock.door_daa(1_000), Err(BatchCourtErrorV1::OverTarget { required_daa: 10_001 }));
        assert_eq!(clock.door_daa(3_121), Err(BatchCourtErrorV1::Window));
        assert_eq!(clock.worst_case_door_daa(), Err(BatchCourtErrorV1::OverTarget { required_daa: 12_121 }));
        assert_eq!(BatchCourtClockV1 { preparation_daa: u64::MAX, ..clock }.door_daa(0), Err(BatchCourtErrorV1::Clock));
    }
}
