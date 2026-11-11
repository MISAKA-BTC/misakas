//! Assemble the protected original's rooted service row after booking and
//! funding preparation. This is a storage candidate, without admission or
//! settlement authority for a complete profile.

use std::collections::{BTreeMap, BTreeSet};

use super::{native_frame::PublicTfp1FrameV1, preclaim::ProtectedBatchBookingV1};
use crate::{
    Hash64,
    palw_batch_court_v1::BatchCourtErrorV1,
    palw_court_service_release_v1::{COURT_SERVICE_ROW_VERSION_V3, CourtServiceRowV3},
    palw_court_service_state_v2::{CourtServiceStateV2, ServiceClaimBinding},
    palw_court_service_v1::{ServiceCost, ServiceReservations},
    palw_native_resource_v1::{
        NativeCoverageV1, NativeDemandLeaseV1, NativeHeapV1, NativeRetainedV1, TirNativeRequestsV1, btree_node_envelope_bytes_v1,
        native_typed_value_requests_v1, reserve_native_pending_requests_v1,
    },
    palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row},
    palw_state_v2::PalwChainStateV2,
};
use misaka_palw_positive_contract::{ContractError, public_one_honest::OriginalClock};

type PacketId = [u8; 64];

#[derive(Debug)]
pub(crate) enum ProtectedServiceBookingErrorV1 {
    MissingServiceBook,
    Contract(ContractError),
    Batch(BatchCourtErrorV1),
    InvalidServiceRow(String),
    LocalResource(NativeCoverageV1),
}

/// Holds the old shared row, the new row's metadata lease, and the moved
/// scheduler lease. The writer checks the old snapshot before publication
/// and keeps both new-row leases through its native state/journal handoff.
pub(crate) struct PreparedProtectedServiceBookingV1 {
    claim: Hash64,
    before: Row,
    row: NativeRetainedV1<CourtServiceRowV3>,
    book_lease: Option<NativeDemandLeaseV1>,
}

impl PreparedProtectedServiceBookingV1 {
    pub(crate) fn prepare_v1(
        prefix: &PalwChainStateV2,
        frame: &PublicTfp1FrameV1,
        booking: &ProtectedBatchBookingV1,
        book: NativeRetainedV1<ServiceReservations>,
    ) -> Result<Self, ProtectedServiceBookingErrorV1> {
        use ProtectedServiceBookingErrorV1::{Batch, Contract, InvalidServiceRow, LocalResource, MissingServiceBook};
        booking.check_book_v1(&book).map_err(|error| match error {
            BatchCourtErrorV1::LocalResource(coverage) => LocalResource(coverage),
            error => Batch(error),
        })?;
        let before = prefix.staged_court_row_v1(&Key::CourtService).ok_or(MissingServiceBook)?;
        let (prior, released) = match before {
            Row::CourtService(prior) => (prior.as_ref(), None),
            Row::CourtServiceV3(row) => (&row.state, Some(row.as_ref())),
            _ => return Err(Contract(ContractError::Scope)),
        };
        let claim = frame.claim_v1();
        let origin = frame.origin_v1();
        let obligation = booking.obligation_v1();
        let graph = booking.graph_v1();
        let hard = OriginalClock::new(origin.accepted_daa).map_err(Contract)?.hard();
        if booking.original_v1() != frame.original_id_v1()
            || !booking.matches_acceptance_v1(origin.accepted_daa, origin.accepted_blue_score)
            || frame.native_facts_v1().map_err(Contract)? != *obligation.facts_v1()
            || frame.obligation_v1() != obligation.obligation_v1()
            || (frame.profile_v1(), frame.resource_v1()) != (obligation.profile, obligation.resource)
            || prior.claims.contains_key(&claim)
            || released.is_some_and(|row| row.resolved.contains_key(&claim))
            || booking.quotes_v1().is_empty()
            || booking.quotes_v1().iter().any(|quote| {
                quote.claim != claim || quote.material_until_daa < hard || quote.material_until_daa > frame.material_until_daa_v1()
            })
        {
            return Err(Contract(ContractError::Scope));
        }
        if graph.is_empty()
            || graph.windows(2).any(|pair| pair[0].packet.id >= pair[1].packet.id)
            || !book_extends_v1(&prior.book, &book, graph.len())
        {
            return Err(Contract(ContractError::Scope));
        }
        for node in graph {
            let id = &node.packet.id;
            let assigned = book.assigned_daa(id).ok_or(Contract(ContractError::Scope))?;
            if prior.book.packet(id).is_some()
                || book.packet(id) != Some(&node.packet)
                || node.packet.release_daa <= origin.accepted_daa
                || node.packet.service_before > hard
                || !(node.packet.release_daa..node.packet.service_before).contains(&assigned)
                || node.predecessors.windows(2).any(|ids| ids[0] >= ids[1])
                || node.predecessors.iter().any(|parent| {
                    graph.binary_search_by_key(parent, |packet| packet.packet.id).is_err()
                        || book.assigned_daa(parent).is_none_or(|at| at >= assigned)
                })
            {
                return Err(Contract(ContractError::Scope));
            }
        }

        if cfg!(not(target_pointer_width = "64")) {
            return Err(LocalResource(NativeCoverageV1::unsupported::<CourtServiceRowV3>(
                "protected TFP1 service metadata",
                "pinned BTree allocation layout only covers 64-bit targets",
            )));
        }
        let requests = metadata_requests_v1(prior, released, graph).map_err(LocalResource)?;
        let lease = reserve_native_pending_requests_v1(requests).map_err(LocalResource)?;
        let scratch = validation_requests_v1(book.packets.len()).map_err(LocalResource)?;
        let _validation = reserve_native_pending_requests_v1(scratch).map_err(LocalResource)?;
        let book_requests = native_typed_value_requests_v1(&*book).map_err(LocalResource)?;
        let mut claims = prior.claims.clone();
        let binding = ServiceClaimBinding {
            claim,
            allocation_id: frame.identity_v1(),
            source_id: frame.source_v1(),
            original_accepted_daa: origin.accepted_daa,
            hard_deadline_daa: hard,
            packet_ids: graph.iter().map(|node| node.packet.id).collect::<BTreeSet<_>>(),
        };
        frame.check_service_scope_v1(&binding).map_err(Contract)?;
        claims.insert(claim, binding);
        let mut causal = released.map(|row| row.causal.clone()).unwrap_or_default();
        for node in graph.iter().filter(|node| !node.predecessors.is_empty()) {
            if causal.insert(node.packet.id, node.predecessors.clone()).is_some() {
                return Err(Contract(ContractError::Scope));
            }
        }
        let (book, book_lease) = book.into_parts_v1();
        if book_lease.as_ref().is_none_or(|lease| lease.memory_v1() < book_requests) {
            drop(book);
            return Err(LocalResource(NativeCoverageV1::unsupported::<ServiceReservations>(
                "protected TFP1 service book",
                "book is missing its scheduler allocation owner",
            )));
        }
        let row = CourtServiceRowV3 {
            version: COURT_SERVICE_ROW_VERSION_V3,
            state: CourtServiceStateV2 {
                version: COURT_SERVICE_ROW_VERSION_V3,
                profile_id: prior.profile_id,
                source_profile_id: prior.source_profile_id,
                book,
                claims,
            },
            resolved: released.map(|row| row.resolved.clone()).unwrap_or_default(),
            causal,
        };
        row.check().map_err(InvalidServiceRow)?;
        super::service_graph::check_frame_service_graph_v2(frame, &row, &row.state.claims[&claim]).map_err(Contract)?;
        Ok(Self { claim, before: before.clone(), row: NativeRetainedV1::from_owner_reserved_v1(row, lease), book_lease })
    }

    pub(crate) fn claim_v1(&self) -> Hash64 {
        self.claim
    }

    pub(crate) fn before_v1(&self) -> Option<&Row> {
        Some(&self.before)
    }

    pub(crate) fn row_v1(&self) -> &CourtServiceRowV3 {
        &self.row
    }

    /// Keep the separate scheduler lease alive until the writer has taken
    /// ownership of the row and its journal. This method does not publish it.
    pub(crate) fn into_parts_v1(self) -> (Row, NativeRetainedV1<CourtServiceRowV3>, Option<NativeDemandLeaseV1>) {
        (self.before, self.row, self.book_lease)
    }
}

pub(super) fn book_extends_v1(before: &ServiceReservations, next: &ServiceReservations, added: usize) -> bool {
    before.budget == next.budget
        && before.last_observed_daa == next.last_observed_daa
        && before.supply == next.supply
        && before.packets.len().checked_add(added) == Some(next.packets.len())
        && before.assignments.len().checked_add(added) == Some(next.assignments.len())
        && before.packets.iter().all(|(id, packet)| next.packet(id) == Some(packet))
        && before.assignments.iter().all(|(id, at)| next.assigned_daa(id) == Some(*at))
}

fn metadata_requests_v1(
    prior: &CourtServiceStateV2,
    released: Option<&CourtServiceRowV3>,
    graph: &[crate::palw_court_service_v1::causal::CausalServicePacketV1],
) -> Result<u64, NativeCoverageV1> {
    const PATH: &str = "protected TFP1 service metadata";
    let mut requests = TirNativeRequestsV1::new();
    let mut reserve = |coverage: NativeCoverageV1| requests.add_v1(coverage.bytes().ok_or(coverage)?);
    reserve(prior.claims.native_heap_v1(PATH))?;
    if let Some(row) = released {
        reserve(row.resolved.native_heap_v1(PATH))?;
        reserve(row.causal.native_heap_v1(PATH))?;
    }
    // The writer may materialize a Box and an Arc simultaneously while the
    // book's allocations move intact from the scheduler owner into the row.
    requests.exact_v1::<CourtServiceRowV3>(2)?;
    requests.exact_v1::<NativeRetainedV1<CourtServiceRowV3>>(1)?;
    requests.exact_v1::<usize>(4)?;
    requests.add_v1(btree_node_envelope_bytes_v1::<Hash64, ServiceClaimBinding>() as u64)?;
    requests.add_v1(TirNativeRequestsV1::product_v1(graph.len() as u64, btree_node_envelope_bytes_v1::<PacketId, ()>() as u64)?)?;
    for node in graph.iter().filter(|node| !node.predecessors.is_empty()) {
        requests.add_v1(btree_node_envelope_bytes_v1::<PacketId, Vec<PacketId>>() as u64)?;
        requests.exact_v1::<PacketId>(node.predecessors.len() as u64)?;
    }
    Ok(requests.bytes_v1())
}

pub(super) fn validation_requests_v1(packets: usize) -> Result<u64, NativeCoverageV1> {
    // Row::check rebuilds the residual slot map, live membership/owner
    // maps and retained released membership. Each is bounded by all packets.
    let bytes_per_packet = (btree_node_envelope_bytes_v1::<u64, ServiceCost>()
        + btree_node_envelope_bytes_v1::<PacketId, Hash64>()
        + 2 * btree_node_envelope_bytes_v1::<PacketId, ()>()) as u64;
    let mut requests = TirNativeRequestsV1::new();
    requests.add_v1(TirNativeRequestsV1::product_v1(packets as u64, bytes_per_packet)?)?;
    requests.vec_v1::<u8>(1024)?; // fixed profile hash's Borsh Vec
    Ok(requests.bytes_v1())
}

/// Scratch for the existing staged writer's strict append of a prepared row.
/// The candidate and its unchanged prior are borrowed here. The caller keeps
/// their owners and this scratch through the writer. A protected append has
/// no release or legacy draft-normalization fallback to reserve.
pub(crate) fn writer_scratch_requests_v1(before: &Row, next: &CourtServiceRowV3) -> Result<u64, NativeCoverageV1> {
    let old = match before {
        Row::CourtService(row) => row.as_ref(),
        Row::CourtServiceV3(row) => &row.state,
        _ => {
            return Err(NativeCoverageV1::unsupported::<Row>("protected TFP1 service writer", "prior row is not a service book"));
        }
    };
    let mut requests = TirNativeRequestsV1::new();
    // `court_service_as_v3_v1` deep-clones the old heap. The typed old
    // shared row already counts its full payload, including a legacy book.
    requests.add_v1(native_typed_value_requests_v1(before)?)?;
    requests.exact_v1::<CourtServiceRowV3>(1)?; // lifted V2 inline metadata
    requests.add_v1(validation_requests_v1(next.state.book.packets.len())?)?;
    // `check_transition_from_v1` allocates the old live-owner map; the
    // removed-bindings set stays empty for this verified append.
    requests.add_v1(TirNativeRequestsV1::product_v1(
        old.book.packets.len() as u64,
        btree_node_envelope_bytes_v1::<PacketId, Hash64>() as u64,
    )?)?;
    // `check_context` still visits legacy bindings and can compute a legacy
    // semantic job pin with a bounded serialization, one binding at a time.
    requests.vec_v1::<u8>(crate::palw_freeprompt_v3::PALW_FP_COMMITMENT_TX_MAX_BYTES as u64)?;
    requests.vec_v1::<Hash64>(next.state.claims.len() as u64)?; // held_terminal
    Ok(requests.bytes_v1())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Requires an actual prepared frame and a book from the same successful
    /// signed booking. The fixture does not install or activate a profile.
    pub(crate) fn exercise_protected_service_booking_v1(
        prefix: &PalwChainStateV2,
        frame: &PublicTfp1FrameV1,
        booking: &ProtectedBatchBookingV1,
        book: &NativeRetainedV1<ServiceReservations>,
    ) {
        let before = prefix.staged_court_row_v1(&Key::CourtService).unwrap();
        let old = prefix.court_service_state_v1().unwrap();
        let prepared = PreparedProtectedServiceBookingV1::prepare_v1(prefix, frame, booking, book.try_clone_v1().unwrap()).unwrap();
        assert_eq!(prepared.claim_v1(), frame.claim_v1());
        assert_eq!(prepared.before_v1(), Some(before));
        let (saved, row, _book_lease) = prepared.into_parts_v1();
        assert_eq!(&saved, before);
        assert_eq!(&row.state.book, &**book);
        assert_eq!((row.state.profile_id, row.state.source_profile_id), (old.profile_id, old.source_profile_id));
        assert_eq!(row.state.claims.len(), old.claims.len() + 1);
        assert!(old.claims.iter().all(|(claim, binding)| row.state.claims.get(claim) == Some(binding)));
        if let Some(old_v3) = prefix.court_service_row_v3_v1() {
            assert_eq!(row.resolved, old_v3.resolved);
            assert!(old_v3.causal.iter().all(|(id, parents)| row.causal.get(id) == Some(parents)));
        } else {
            assert!(row.resolved.is_empty());
        }
        let binding = &row.state.claims[&frame.claim_v1()];
        frame.check_service_scope_v1(binding).unwrap();
        assert_eq!(binding.packet_ids.len(), booking.graph_v1().len());
        for node in booking.graph_v1() {
            assert!(binding.packet_ids.contains(&node.packet.id));
            assert_eq!(row.causal.get(&node.packet.id), (!node.predecessors.is_empty()).then_some(&node.predecessors));
        }

        let mut substituted = (**book).clone();
        let id = booking.graph_v1()[0].packet.id;
        *substituted.assignments.get_mut(&id).unwrap() += 1;
        assert!(matches!(
            PreparedProtectedServiceBookingV1::prepare_v1(
                prefix,
                frame,
                booking,
                NativeRetainedV1::retain_existing_v1(substituted).unwrap(),
            ),
            Err(ProtectedServiceBookingErrorV1::Batch(BatchCourtErrorV1::CaseGraph))
        ));
        assert!(matches!(
            PreparedProtectedServiceBookingV1::prepare_v1(&PalwChainStateV2::genesis(), frame, booking, book.try_clone_v1().unwrap(),),
            Err(ProtectedServiceBookingErrorV1::MissingServiceBook)
        ));
        assert_eq!(prefix.staged_court_row_v1(&Key::CourtService), Some(before));
    }
}
