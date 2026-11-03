//! Candidate batch Court transport contract. A range identifies material in an
//! accepted source; the native caller must independently verify its opening
//! and the responder's authority. Transport completeness grants no credit.
use crate::Hash64;
use crate::palw_court_policy_v1::{DISPUTE_WINDOW_DAA, NOTICE_WINDOW_DAA, shared_initial_notice_daa_in_window_v1};
use crate::palw_court_service_v1::{ServiceCost, ServicePacket, ServiceRefusal, ServiceReservations, causal::CausalServicePacketV1};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::blake2b_512_keyed;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read};

pub mod carriage;
pub mod fold_order;
pub(crate) mod rooted;

pub const MAX_BATCH_RANGES_V1: usize = 64;
pub const MAX_BATCH_PARTS_V1: usize = 256;
pub const MAX_BATCH_PART_BYTES_V1: u32 = 16_384;

/// Prospective clocks for an admitted dispute. The accepting fold must check
/// the opening's standing, bond, and actionable evidence before constructing
/// this record. Clock validation alone does not admit a dispute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
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

    /// Reserve a complete native case graph at the last timely notice. The
    /// coordinator derives the graph from all admitted branches and pays for
    /// the graph/index/slot-search work. Every reserved packet must precede
    /// the terminal filing, whose remaining interval includes a separate
    /// recovery allowance and the T06 exclusive-boundary margin.
    pub fn reserve_latest_notice_case_v1(
        self,
        book: &mut ServiceReservations,
        graph: &[CausalServicePacketV1],
        terminal: [u8; 64],
        recovery_daa: u64,
    ) -> Result<u64, BatchCourtErrorV1> {
        self.reserve_latest_notice_checked_v1(book, graph, terminal, recovery_daa, |_, _| Ok(()))
    }

    pub(crate) fn reserve_latest_notice_checked_v1(
        self,
        book: &mut ServiceReservations,
        graph: &[CausalServicePacketV1],
        terminal: [u8; 64],
        recovery_daa: u64,
        check_assignments: impl FnOnce(&ServiceReservations, &BTreeMap<[u8; 64], &CausalServicePacketV1>) -> Result<(), ServiceRefusal>,
    ) -> Result<u64, BatchCourtErrorV1> {
        use BatchCourtErrorV1::{CaseGraph, Clock, Reservation, Window};
        self.check()?;
        let latest_notice = self.claim_accepted_daa.checked_add(NOTICE_WINDOW_DAA).ok_or(Clock)?;
        let first_service = latest_notice.checked_add(1).ok_or(Clock)?;
        let hard = self.claim_accepted_daa.checked_add(DISPUTE_WINDOW_DAA).ok_or(Clock)?;
        let case_end = hard.min(self.collect_until_daa);
        let service_before = case_end.checked_sub(recovery_daa).ok_or(Window)?;
        if self.challenge_opened_daa != latest_notice || recovery_daa == 0 || first_service >= service_before {
            return Err(Window);
        }

        let mut nodes = BTreeMap::new();
        for node in graph {
            if node.packet.release_daa < first_service || node.packet.service_before > service_before {
                return Err(Window);
            }
            if nodes.insert(node.packet.id, node).is_some() {
                return Err(CaseGraph);
            }
        }
        let mut visited = BTreeSet::from([terminal]);
        let mut pending = vec![terminal];
        while let Some(id) = pending.pop() {
            let node = nodes.get(&id).ok_or(CaseGraph)?;
            for predecessor in &node.predecessors {
                if visited.insert(*predecessor) {
                    pending.push(*predecessor);
                }
            }
        }
        if visited.len() != nodes.len() {
            return Err(CaseGraph);
        }

        book.reserve_causal_checked_v1(graph, service_before, |candidate| check_assignments(candidate, &nodes))
            .map_err(Reservation)?;
        // The assigned slot is below service_before; the checked window
        // proves these additions fit and cannot exceed the original hard end.
        let terminal_daa = book.assigned_daa(&terminal).expect("successful case reservation assigns its terminal");
        Ok(terminal_daa - self.claim_accepted_daa + recovery_daa + 1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum BatchRangeAuthenticationV1 {
    IndependentParts,
    WholeRange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BatchRangeV1 {
    /// Canonical material identity, including its source root and coordinates.
    pub id: Hash64,
    /// Exact bytes after the source-specific encoding is fixed.
    pub bytes: u32,
    /// The native caller derives this from the accepted source encoding.
    pub authentication: BatchRangeAuthenticationV1,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize)]
pub struct BatchResponsePartV1<'a> {
    pub case_id: Hash64,
    pub request_root: Hash64,
    pub range_id: Hash64,
    pub part_index: u32,
    pub bytes: &'a [u8],
}

impl BorshDeserialize for BatchDemandV1 {
    fn deserialize_reader<R: Read>(reader: &mut R) -> io::Result<Self> {
        let case_id = Hash64::deserialize_reader(reader)?;
        let case_window = BatchCourtWindowV1::deserialize_reader(reader)?;
        let opened_daa = u64::deserialize_reader(reader)?;
        let response_deadline_daa = u64::deserialize_reader(reader)?;
        let maximum_bytes = u64::deserialize_reader(reader)?;
        let part_bytes = u32::deserialize_reader(reader)?;
        let count = u32::deserialize_reader(reader)? as usize;
        if !(1..=MAX_BATCH_RANGES_V1).contains(&count) {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "batch Court demand range count"));
        }
        let mut ranges = Vec::new();
        ranges.try_reserve_exact(count).map_err(|_| io::Error::new(io::ErrorKind::OutOfMemory, "batch Court demand ranges"))?;
        for _ in 0..count {
            ranges.push(BatchRangeV1::deserialize_reader(reader)?);
        }
        let demand = Self { case_id, case_window, opened_daa, response_deadline_daa, maximum_bytes, part_bytes, ranges };
        demand.check().map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "batch Court demand shape"))?;
        Ok(demand)
    }
}

impl<'a> BatchResponsePartV1<'a> {
    /// Borrow the payload directly from an already-admitted outer carrier.
    /// The Node still authenticates the envelope and the range opening.
    pub fn decode_v1(mut carrier: &'a [u8]) -> Result<Self, BatchCourtErrorV1> {
        use BatchCourtErrorV1::PartShape;
        let case_id = Hash64::deserialize(&mut carrier).map_err(|_| PartShape)?;
        let request_root = Hash64::deserialize(&mut carrier).map_err(|_| PartShape)?;
        let range_id = Hash64::deserialize(&mut carrier).map_err(|_| PartShape)?;
        let part_index = u32::deserialize(&mut carrier).map_err(|_| PartShape)?;
        let count = u32::deserialize(&mut carrier).map_err(|_| PartShape)?;
        if !(1..=MAX_BATCH_PART_BYTES_V1).contains(&count) || carrier.len() != count as usize {
            return Err(PartShape);
        }
        Ok(Self { case_id, request_root, range_id, part_index, bytes: carrier })
    }
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
    AuthenticationMode,
    DeliveryCost,
    CaseGraph,
    Reservation(ServiceRefusal),
    VerificationUnavailable,
    Checkpoint,
    Clock,
    OverTarget { required_daa: u64 },
}

fn part_digest(bytes: &[u8]) -> Hash64 {
    blake2b_512_keyed(b"misaka-palw/batch-court/part/v1", bytes)
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

    /// Canonical request identity used by signed native material delivery.
    pub fn checked_request_root_v1(&self) -> Result<Hash64, BatchCourtErrorV1> {
        self.check()?;
        self.root()
    }

    fn root(&self) -> Result<Hash64, BatchCourtErrorV1> {
        let bytes = borsh::to_vec(self).map_err(|_| BatchCourtErrorV1::Demand)?;
        Ok(blake2b_512_keyed(b"misaka-palw/batch-court/demand/v1", &bytes))
    }

    /// Project one exact demand after its opening, with independent part
    /// slots and one whole-body verification control per whole range. Costs
    /// come from the native profile in canonical range/part order, including
    /// outer framing, authentication and storage. The caller must reserve
    /// later dependent rounds, terminal filing and recovery separately.
    pub fn delivery_service_graph_v1(
        &self,
        part_costs: &[ServiceCost],
        whole_range_costs: &[ServiceCost],
    ) -> Result<Vec<CausalServicePacketV1>, BatchCourtErrorV1> {
        const PART_HEADER_BYTES: u64 = 3 * 64 + 2 * 4;
        use BatchCourtErrorV1::DeliveryCost;
        let count = self.check()?;
        let whole_count = self.ranges.iter().filter(|range| range.authentication == BatchRangeAuthenticationV1::WholeRange).count();
        if part_costs.len() != count || whole_range_costs.len() != whole_count {
            return Err(DeliveryCost);
        }
        let request_root = self.root()?;
        let release_daa = self.opened_daa.checked_add(1).ok_or(BatchCourtErrorV1::Clock)?;
        let service_before = self.response_deadline_daa;
        let mut part_costs = part_costs.iter().copied();
        let mut whole_range_costs = whole_range_costs.iter().copied();
        let mut graph = Vec::with_capacity(count + whole_count);
        for range in &self.ranges {
            let part_count = range.bytes.div_ceil(self.part_bytes);
            let first_part = graph.len();
            for index in 0..part_count {
                let payload_bytes = u64::from((range.bytes - index * self.part_bytes).min(self.part_bytes));
                let cost = part_costs.next().expect("validated canonical part count");
                if cost.bytes < PART_HEADER_BYTES + payload_bytes {
                    return Err(DeliveryCost);
                }
                graph.push(CausalServicePacketV1 {
                    packet: ServicePacket {
                        id: delivery_packet_id(request_root, range.id, Some(index)),
                        release_daa,
                        service_before,
                        cost,
                    },
                    predecessors: Vec::new(),
                });
            }
            if range.authentication == BatchRangeAuthenticationV1::WholeRange {
                let predecessors = graph[first_part..].iter().map(|part| part.packet.id).collect();
                let cost = whole_range_costs.next().expect("validated canonical whole range count");
                graph.push(CausalServicePacketV1 {
                    packet: ServicePacket { id: delivery_packet_id(request_root, range.id, None), release_daa, service_before, cost },
                    predecessors,
                });
            }
        }
        Ok(graph)
    }
}

fn delivery_packet_id(request_root: Hash64, range: Hash64, index: Option<u32>) -> [u8; 64] {
    let bytes = borsh::to_vec(&(request_root, range, index)).expect("fixed delivery identity encodes");
    blake2b_512_keyed(b"misaka-palw/batch-court/delivery/v1", &bytes).as_bytes()
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
        progress.check_checkpoint(expected_checkpoint_root)?;
        Ok(progress)
    }

    /// Encode only canonical slot digests. Evidence and its accounting are
    /// separate branch-owned rows. Callers publish them atomically.
    pub fn encode_checkpoint_v1(&self) -> Result<Vec<u8>, BatchCourtErrorV1> {
        borsh::to_vec(&self.accepted).map_err(|_| BatchCourtErrorV1::Checkpoint)
    }

    /// Decode against the demand's slot count, so untrusted bytes cannot increase allocation.
    /// A snapshot on its own cannot supply its expected branch commitment.
    pub fn decode_checkpoint_v1(
        demand: BatchDemandV1,
        request_root: Hash64,
        mut bytes: &[u8],
        expected_checkpoint_root: Hash64,
    ) -> Result<Self, BatchCourtErrorV1> {
        use BatchCourtErrorV1::Checkpoint;
        let mut progress = Self::new(demand)?;
        let count = u32::deserialize(&mut bytes).map_err(|_| Checkpoint)? as usize;
        if request_root != progress.request_root || count != progress.accepted.len() {
            return Err(Checkpoint);
        }
        for slot in &mut progress.accepted {
            *slot = Option::<Hash64>::deserialize(&mut bytes).map_err(|_| Checkpoint)?;
        }
        if !bytes.is_empty() {
            return Err(Checkpoint);
        }
        progress.check_checkpoint(expected_checkpoint_root)?;
        Ok(progress)
    }

    fn check_checkpoint(&self, expected_checkpoint_root: Hash64) -> Result<(), BatchCourtErrorV1> {
        let mut slots = self.accepted.as_slice();
        for range in &self.demand.ranges {
            let part_count = range.bytes.div_ceil(self.demand.part_bytes) as usize;
            let (range_slots, rest) = slots.split_at(part_count);
            slots = rest;
            if range.authentication == BatchRangeAuthenticationV1::WholeRange
                && range_slots.iter().any(Option::is_some)
                && range_slots.iter().any(Option::is_none)
            {
                return Err(BatchCourtErrorV1::Checkpoint);
            }
        }
        if self.checkpoint_root()? != expected_checkpoint_root {
            return Err(BatchCourtErrorV1::Checkpoint);
        }
        Ok(())
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

    fn range_at(&self, id: Hash64) -> Result<(BatchRangeV1, usize), BatchCourtErrorV1> {
        let index = self.demand.ranges.binary_search_by_key(&id, |range| range.id).map_err(|_| BatchCourtErrorV1::Identity)?;
        let first_slot = self.demand.ranges[..index].iter().map(|range| range.bytes.div_ceil(self.demand.part_bytes) as usize).sum();
        Ok((self.demand.ranges[index], first_slot))
    }

    fn check_response(&self, daa: u64, case_id: Hash64, request_root: Hash64) -> Result<(), BatchCourtErrorV1> {
        if case_id != self.demand.case_id || request_root != self.request_root {
            return Err(BatchCourtErrorV1::Identity);
        }
        if daa < self.demand.opened_daa || daa >= self.demand.response_deadline_daa {
            return Err(BatchCourtErrorV1::Window);
        }
        Ok(())
    }

    /// Use the native full-body verifier for a range whose encoding has no
    /// independent per-part openings. The caller owns and bounds assembly of
    /// the bytes. No slot advances until the whole range verifies.
    pub fn accept_whole_range(
        &mut self,
        daa: u64,
        case_id: Hash64,
        request_root: Hash64,
        range_id: Hash64,
        bytes: &[u8],
        verify_opening: impl FnOnce(BatchRangeV1, &[u8]) -> Result<(), BatchCourtErrorV1>,
    ) -> Result<BatchPartOutcomeV1, BatchCourtErrorV1> {
        use BatchCourtErrorV1::{Conflict, PartShape};
        self.check_response(daa, case_id, request_root)?;
        let (range, first) = self.range_at(range_id)?;
        if range.authentication != BatchRangeAuthenticationV1::WholeRange {
            return Err(BatchCourtErrorV1::AuthenticationMode);
        }
        if bytes.len() != range.bytes as usize {
            return Err(PartShape);
        }
        let chunks = bytes.chunks(self.demand.part_bytes as usize);
        let slots = &mut self.accepted[first..first + chunks.len()];
        for (stored, bytes) in slots.iter().zip(chunks.clone()) {
            if stored.is_some_and(|hash| hash != part_digest(bytes)) {
                return Err(Conflict);
            }
        }
        // Use the same native validation for duplicate whole bodies.
        verify_opening(range, bytes)?;
        if slots.iter().all(Option::is_some) {
            return Ok(BatchPartOutcomeV1::Duplicate);
        }
        for (slot, bytes) in slots.iter_mut().zip(chunks) {
            if slot.is_none() {
                *slot = Some(part_digest(bytes));
            }
        }
        Ok(BatchPartOutcomeV1::Applied)
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
        use BatchCourtErrorV1::{Conflict, Opening, PartShape};
        self.check_response(daa, part.case_id, part.request_root)?;
        let (range, first) = self.range_at(part.range_id)?;
        if range.authentication != BatchRangeAuthenticationV1::IndependentParts {
            return Err(BatchCourtErrorV1::AuthenticationMode);
        }
        let size = self.demand.part_bytes;
        let offset = part.part_index.checked_mul(size).filter(|offset| *offset < range.bytes).ok_or(PartShape)?;
        if part.bytes.len() != (range.bytes - offset).min(size) as usize {
            return Err(PartShape);
        }
        let slot = first + part.part_index as usize;
        let content = part_digest(part.bytes);
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
        let authentication = BatchRangeAuthenticationV1::IndependentParts;
        let mut ranges = vec![
            BatchRangeV1 { id: Hash64::from_u64_word(1), bytes: 6, authentication },
            BatchRangeV1 { id: Hash64::from_u64_word(2), bytes: 3, authentication },
        ];
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
    fn reserves_parallel_parts_and_a_causal_whole_body_check() {
        use crate::palw_court_service_v1::{ServiceRefusal, ServiceReservations};
        let mut request = demand();
        let range = request.ranges.iter_mut().find(|range| range.id == Hash64::from_u64_word(1)).unwrap();
        range.authentication = BatchRangeAuthenticationV1::WholeRange;
        let range = range.id;
        let capacity = ServiceCost { bytes: 610, work: 6, io: 3, controls: 3 };
        let part = ServiceCost { bytes: 204, work: 2, io: 1, controls: 1 };
        let finish = ServiceCost { bytes: 20, work: 6, io: 2, controls: 1 };
        let mut book = ServiceReservations::new(capacity).unwrap();
        let initial = book.clone();
        // The first two parts use 204 and 202 bytes and the third 203.
        let parts = request
            .ranges
            .iter()
            .flat_map(|range| {
                (0..range.bytes.div_ceil(request.part_bytes)).map(move |index| ServiceCost {
                    bytes: 200 + u64::from((range.bytes - index * request.part_bytes).min(request.part_bytes)),
                    ..part
                })
            })
            .collect::<Vec<_>>();
        let graph = request.delivery_service_graph_v1(&parts, &[finish]).unwrap();
        book.reserve_causal_v1(&graph, request.response_deadline_daa).unwrap();
        book.check_causal_v1(&graph).unwrap();
        let root = request.root().unwrap();
        assert_eq!(book.assigned_daa(&delivery_packet_id(root, range, Some(0))), Some(11));
        assert_eq!(book.assigned_daa(&delivery_packet_id(root, range, Some(1))), Some(11));
        assert_eq!(book.assigned_daa(&delivery_packet_id(root, range, None)), Some(12));

        let mut refused = initial.clone();
        request.response_deadline_daa = 12;
        let graph = request.delivery_service_graph_v1(&parts, &[finish]).unwrap();
        assert_eq!(book.check_causal_v1(&graph), Err(ServiceRefusal::InvalidState));
        assert_eq!(refused.reserve_causal_v1(&graph, request.response_deadline_daa), Err(ServiceRefusal::DeadlineExhausted));
        assert_eq!(refused, initial);
        let mut underpriced = parts;
        underpriced[0].bytes -= 1;
        assert_eq!(request.delivery_service_graph_v1(&underpriced, &[finish]), Err(BatchCourtErrorV1::DeliveryCost));
    }

    #[test]
    fn latest_notice_reserves_terminal_and_recovery_without_crossing_the_boundary() {
        let accepted = 100;
        let notice = accepted + NOTICE_WINDOW_DAA;
        let hard = accepted + DISPUTE_WINDOW_DAA;
        let case = BatchCourtWindowV1 {
            claim_accepted_daa: accepted,
            challenge_opened_daa: notice,
            collect_until_daa: hard + 1,
            material_until_daa: hard + 1,
        };
        let cost = ServiceCost { bytes: 1, work: 1, io: 1, controls: 1 };
        let node = |id, release_daa, service_before, predecessors| CausalServicePacketV1 {
            packet: ServicePacket { id: [id; 64], release_daa, service_before, cost },
            predecessors,
        };
        let terminal = [3; 64];
        let recovery = 4;
        let before = hard - recovery;
        let mut graph = vec![
            node(1, notice + 1, before, vec![]),
            node(2, notice + 1, before, vec![]),
            node(3, before - 1, before, vec![[1; 64], [2; 64]]),
        ];
        let initial = ServiceReservations::new(cost).unwrap();
        let mut book = initial.clone();
        assert_eq!(case.reserve_latest_notice_case_v1(&mut book, &graph, terminal, recovery), Ok(DISPUTE_WINDOW_DAA));
        assert_eq!(book.assigned_daa(&terminal), Some(before - 1));
        let mut refused = initial.clone();
        let early = BatchCourtWindowV1 { challenge_opened_daa: notice - 1, ..case };
        assert_eq!(early.reserve_latest_notice_case_v1(&mut refused, &graph, terminal, recovery), Err(BatchCourtErrorV1::Window));
        assert_eq!(case.reserve_latest_notice_case_v1(&mut refused, &graph, terminal, recovery + 1), Err(BatchCourtErrorV1::Window));
        graph[2].predecessors.pop();
        assert_eq!(case.reserve_latest_notice_case_v1(&mut refused, &graph, terminal, recovery), Err(BatchCourtErrorV1::CaseGraph));
        graph[2].predecessors.push([2; 64]);
        graph[0].packet.release_daa = before - 1;
        assert_eq!(
            case.reserve_latest_notice_case_v1(&mut refused, &graph, terminal, recovery),
            Err(BatchCourtErrorV1::Reservation(ServiceRefusal::DeadlineExhausted))
        );
        assert_eq!(refused, initial);
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
    fn whole_range_requires_its_native_verifier_even_across_checkpoint_restore() {
        let mut whole_demand = demand();
        whole_demand.ranges.iter_mut().find(|range| range.id == Hash64::from_u64_word(1)).unwrap().authentication =
            BatchRangeAuthenticationV1::WholeRange;
        let mut mixed = BatchProgressV1::new(whole_demand.clone()).unwrap();
        let a = part(&mixed, 1, 0, b"abcd");
        let b = part(&mixed, 1, 1, b"ef");
        let c = part(&mixed, 2, 0, b"xyz");
        let opening_is_unused = |_, _, _: &[u8]| panic!("whole range rejected before opening");
        assert_eq!(mixed.accept(10, a, opening_is_unused), Err(BatchCourtErrorV1::AuthenticationMode));
        assert_eq!(mixed.accept(10, b, opening_is_unused), Err(BatchCourtErrorV1::AuthenticationMode));
        mixed.accept(10, c, |_, _, _| true).unwrap();
        assert_eq!(mixed.status(20), BatchProgressStatusV1::TimedOutUnverified);
        let before = mixed.checkpoint_root().unwrap();
        let whole = |state: &mut BatchProgressV1, bytes: &[u8], verdict: Result<(), BatchCourtErrorV1>| {
            state.accept_whole_range(10, a.case_id, a.request_root, a.range_id, bytes, |range, body| {
                assert_eq!(range.bytes, body.len() as u32);
                verdict
            })
        };
        let invalid = Err(BatchCourtErrorV1::Opening);
        let unavailable = Err(BatchCourtErrorV1::VerificationUnavailable);
        assert_eq!(whole(&mut mixed, b"abcdef", invalid), Err(BatchCourtErrorV1::Opening));
        assert_eq!(whole(&mut mixed, b"abcdef", unavailable), Err(BatchCourtErrorV1::VerificationUnavailable));
        assert_eq!(mixed.checkpoint_root().unwrap(), before);
        assert_eq!(whole(&mut mixed, b"abcdef", Ok(())), Ok(BatchPartOutcomeV1::Applied));
        assert_eq!(whole(&mut mixed, b"abcdXY", Ok(())), Err(BatchCourtErrorV1::Conflict));
        assert_eq!(whole(&mut mixed, b"abcdef", invalid), Err(BatchCourtErrorV1::Opening));
        assert_eq!(whole(&mut mixed, b"abcdef", Ok(())), Ok(BatchPartOutcomeV1::Duplicate));

        assert_eq!(mixed.status(20), BatchProgressStatusV1::AllMaterialVerified);
        assert_eq!(mixed.remaining_payload_bytes(), 0);
        let checkpoint = mixed.encode_checkpoint_v1().unwrap();
        let complete_root = mixed.checkpoint_root().unwrap();
        assert_eq!(
            BatchProgressV1::decode_checkpoint_v1(whole_demand.clone(), a.request_root, &checkpoint, complete_root),
            Ok(mixed.clone())
        );
        assert_eq!(
            BatchProgressV1::decode_checkpoint_v1(demand(), a.request_root, &checkpoint, complete_root),
            Err(BatchCourtErrorV1::Checkpoint)
        );
        assert_eq!(
            mixed.accept_whole_range(10, c.case_id, c.request_root, c.range_id, c.bytes, |_, _| panic!("independent range")),
            Err(BatchCourtErrorV1::AuthenticationMode)
        );

        let (_, first) = mixed.range_at(a.range_id).unwrap();
        mixed.accepted[first] = None;
        let partial = mixed.encode_checkpoint_v1().unwrap();
        let partial_root = mixed.checkpoint_root().unwrap();
        assert_eq!(
            BatchProgressV1::restore(whole_demand.clone(), a.request_root, mixed.checkpoint(), partial_root),
            Err(BatchCourtErrorV1::Checkpoint)
        );
        assert_eq!(
            BatchProgressV1::decode_checkpoint_v1(whole_demand, a.request_root, &partial, partial_root),
            Err(BatchCourtErrorV1::Checkpoint)
        );
    }

    #[test]
    fn bounded_codec_reopens_the_exact_branch_checkpoint() {
        let original = demand();
        let encoded = borsh::to_vec(&original).unwrap();
        assert_eq!(BatchDemandV1::try_from_slice(&encoded).unwrap(), original);
        let mut oversized = encoded.clone();
        let count_offset = encoded.len() - original.ranges.len() * (64 + 4 + 1) - 4;
        oversized[count_offset..count_offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(BatchDemandV1::try_from_slice(&oversized).is_err());
        let mut unknown_authentication = encoded;
        unknown_authentication[count_offset + 4 + 64 + 4] = u8::MAX;
        assert!(BatchDemandV1::try_from_slice(&unknown_authentication).is_err());

        let mut progress = BatchProgressV1::new(original.clone()).unwrap();
        let response = part(&progress, 1, 1, b"ef");
        let mut carried = borsh::to_vec(&response).unwrap();
        assert_eq!(BatchResponsePartV1::decode_v1(&carried), Ok(response));
        carried.push(0);
        assert_eq!(BatchResponsePartV1::decode_v1(&carried), Err(BatchCourtErrorV1::PartShape));

        progress.accept(10, response, |_, _, _| true).unwrap();
        let root = progress.checkpoint_root().unwrap();
        let request = progress.request_root();
        let mut checkpoint = progress.encode_checkpoint_v1().unwrap();
        assert_eq!(BatchProgressV1::decode_checkpoint_v1(original.clone(), request, &checkpoint, root), Ok(progress));
        checkpoint[0..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(BatchProgressV1::decode_checkpoint_v1(original, request, &checkpoint, root), Err(BatchCourtErrorV1::Checkpoint));
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
