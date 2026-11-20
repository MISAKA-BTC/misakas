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
    #[cfg(test)]
    MissingServiceBook,
    Contract(ContractError),
    Batch(BatchCourtErrorV1),
    InvalidServiceRow(String),
    LocalResource(NativeCoverageV1),
}

/// Read-only service namespace at the exact planning prefix. A fresh book is
/// permitted under the selected protected declaration or the legacy family
/// gates. Callers cannot replace the selected budget or install this empty base.
pub(crate) struct ProtectedServiceBaseV1<'a> {
    before: Option<&'a Row>,
    at_daa: u64,
    fresh: Option<CourtServiceStateV2>,
}

impl<'a> ProtectedServiceBaseV1<'a> {
    pub(crate) fn resolve_v1(
        prefix: &'a PalwChainStateV2,
        params: &crate::palw_state_v2::PalwStateParamsV2,
        extras: &crate::palw_state_v2::PalwTransitionExtrasV1,
        daa: u64,
        profile: &crate::palw_court_service_binding_v1::CourtServiceProfileV1,
    ) -> Result<Self, crate::palw_native_resource_v1::NativeDecodeErrorV1<String>> {
        use crate::palw_native_resource_v1::NativeDecodeErrorV1 as E;
        if profile.version != 1
            || profile.packets_per_signer == 0
            || profile.service_span_daa == 0
            || profile.packet_cost.controls != 1
            || [profile.budget.bytes, profile.budget.work, profile.budget.io, profile.budget.controls].contains(&0)
        {
            return Err(E::Decode("protected service profile has invalid capacity or packet terms".into()));
        }
        let declared_profile =
            extras.protected_planning_v3.map(|declaration| declaration.service_book_profile_v3()).transpose().map_err(E::Decode)?;
        if declared_profile.is_some_and(|declared| declared != *profile) {
            return Err(E::Decode("protected service bootstrap differs from the selected declaration".into()));
        }
        // Hashing the finite profile has a bounded writer, even when reusing a
        // stored book. Hold that scratch until the identity checks finish.
        let mut requests = TirNativeRequestsV1::new();
        requests.vec_v1::<u8>(1024).map_err(E::Resource)?;
        let _scratch = reserve_native_pending_requests_v1(requests.bytes_v1()).map_err(E::Resource)?;
        let source_profile_id = profile.id();
        let book_profile_id = crate::palw_court_service_state_v2::court_service_profile_id_v2(source_profile_id);
        let before = prefix.staged_court_row_v1(&Key::CourtService);
        if let Some(row) = before {
            let (state, expected_version): (&CourtServiceStateV2, u16) = match row {
                Row::CourtService(state) => (state.as_ref(), 2),
                Row::CourtServiceV3(row) if row.version == COURT_SERVICE_ROW_VERSION_V3 => (&row.state, COURT_SERVICE_ROW_VERSION_V3),
                _ => return Err(E::Decode("protected service namespace has another row type or version".into())),
            };
            if state.version != expected_version
                || state.source_profile_id != source_profile_id
                || state.profile_id != book_profile_id
                || state.book.budget != profile.budget
            {
                return Err(E::Decode("protected service book differs from the selected profile or capacity".into()));
            }
            return Ok(Self { before, at_daa: daa, fresh: None });
        }
        let legacy_owner = params.family_supplier_active_at(daa) && extras.family_supplier_active;
        if !extras.tir_public_conviction_active || (declared_profile.is_none() && !legacy_owner) {
            return Err(E::Decode("protected original has no active native service namespace".into()));
        }
        let fresh = CourtServiceStateV2 {
            version: 2,
            profile_id: book_profile_id,
            source_profile_id,
            book: ServiceReservations::new(profile.budget).map_err(|error| E::Decode(format!("pinned service budget: {error:?}")))?,
            claims: BTreeMap::new(),
        };
        Ok(Self { before: None, at_daa: daa, fresh: Some(fresh) })
    }

    fn prior_v1(&self) -> (&CourtServiceStateV2, Option<&CourtServiceRowV3>) {
        match self.before {
            Some(Row::CourtService(prior)) => (prior.as_ref(), None),
            Some(Row::CourtServiceV3(row)) => (&row.state, Some(row.as_ref())),
            None => (self.fresh.as_ref().expect("resolved fresh service namespace"), None),
            _ => unreachable!("resolved service namespace type"),
        }
    }

    pub(crate) fn book_v1(&self) -> &ServiceReservations {
        &self.prior_v1().0.book
    }
}

/// Holds the old shared row, the new row's metadata lease, and the moved
/// scheduler lease. The writer checks the old snapshot before publication
/// and keeps both new-row leases through its native state/journal handoff.
pub(crate) struct PreparedProtectedServiceBookingV1 {
    claim: Hash64,
    before: Option<Row>,
    row: NativeRetainedV1<CourtServiceRowV3>,
    book_lease: Option<NativeDemandLeaseV1>,
}

impl PreparedProtectedServiceBookingV1 {
    #[cfg(test)]
    pub(crate) fn prepare_v1(
        prefix: &PalwChainStateV2,
        frame: &PublicTfp1FrameV1,
        booking: &ProtectedBatchBookingV1,
        book: NativeRetainedV1<ServiceReservations>,
    ) -> Result<Self, ProtectedServiceBookingErrorV1> {
        let before = prefix.staged_court_row_v1(&Key::CourtService).ok_or(ProtectedServiceBookingErrorV1::MissingServiceBook)?;
        if !matches!(before, Row::CourtService(_) | Row::CourtServiceV3(_)) {
            return Err(ProtectedServiceBookingErrorV1::Contract(ContractError::Scope));
        }
        Self::prepare_from_base_v1(
            ProtectedServiceBaseV1 { before: Some(before), at_daa: frame.origin_v1().accepted_daa, fresh: None },
            frame,
            booking,
            book,
        )
    }

    pub(crate) fn prepare_from_base_v1(
        base: ProtectedServiceBaseV1<'_>,
        frame: &PublicTfp1FrameV1,
        booking: &ProtectedBatchBookingV1,
        book: NativeRetainedV1<ServiceReservations>,
    ) -> Result<Self, ProtectedServiceBookingErrorV1> {
        use ProtectedServiceBookingErrorV1::{Batch, Contract, InvalidServiceRow, LocalResource};
        booking.check_book_v1(&book).map_err(|error| match error {
            BatchCourtErrorV1::LocalResource(coverage) => LocalResource(coverage),
            error => Batch(error),
        })?;
        let (prior, released) = base.prior_v1();
        let claim = frame.claim_v1();
        let origin = frame.origin_v1();
        let obligation = booking.obligation_v1();
        let graph = booking.graph_v1();
        let hard = OriginalClock::new(origin.accepted_daa).map_err(Contract)?.hard();
        if base.at_daa != origin.accepted_daa
            || booking.original_v1() != frame.original_id_v1()
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
        Ok(Self { claim, before: base.before.cloned(), row: NativeRetainedV1::from_owner_reserved_v1(row, lease), book_lease })
    }

    pub(crate) fn claim_v1(&self) -> Hash64 {
        self.claim
    }

    pub(crate) fn before_v1(&self) -> Option<&Row> {
        self.before.as_ref()
    }

    pub(crate) fn row_v1(&self) -> &CourtServiceRowV3 {
        &self.row
    }

    /// Keep the separate scheduler lease alive until the writer has taken
    /// ownership of the row and its journal. This method does not publish it.
    pub(crate) fn into_parts_v1(self) -> (Option<Row>, NativeRetainedV1<CourtServiceRowV3>, Option<NativeDemandLeaseV1>) {
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
pub(crate) fn writer_scratch_requests_v1(before: Option<&Row>, next: &CourtServiceRowV3) -> Result<u64, NativeCoverageV1> {
    let old = match before {
        Some(Row::CourtService(row)) => Some(row.as_ref()),
        Some(Row::CourtServiceV3(row)) => Some(&row.state),
        None => None,
        _ => {
            return Err(NativeCoverageV1::unsupported::<Row>("protected TFP1 service writer", "prior row is not a service book"));
        }
    };
    let mut requests = TirNativeRequestsV1::new();
    // `court_service_as_v3_v1` deep-clones the old heap. The typed old
    // shared row already counts its full payload, including a legacy book.
    if let Some(before) = before {
        requests.add_v1(native_typed_value_requests_v1(before)?)?;
        requests.exact_v1::<CourtServiceRowV3>(1)?; // lifted V2 inline metadata
    }
    requests.add_v1(validation_requests_v1(next.state.book.packets.len())?)?;
    // `check_transition_from_v1` allocates the old live-owner map; the
    // removed-bindings set stays empty for this verified append.
    requests.add_v1(TirNativeRequestsV1::product_v1(
        old.map_or(0, |old| old.book.packets.len()) as u64,
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

    #[test]
    fn declared_protected_book_owns_bootstrap_without_legacy_family_activation() {
        use crate::palw_state_v2::{PalwStateParamsV2, PalwTransitionExtrasV1};
        let prefix = PalwChainStateV2::genesis();
        let params = PalwStateParamsV2::new(500, 10, 10, 10, 10, 100, Hash64::from_u64_word(1), 2, 1000, 1, 1000, 1).unwrap();
        let declaration = crate::palw_positive_contract_v2::ProtectedPlanningDeclarationV3::decode_json_v3(include_bytes!(
            "../../../tests/fixtures/protected-planning-schema-v3.json"
        ))
        .unwrap();
        let profile = declaration.service_book_profile_v3().unwrap();
        let mut extras = PalwTransitionExtrasV1 { protected_planning_v3: Some(declaration), ..Default::default() };
        assert!(!params.family_supplier_active_at(100));
        assert!(!extras.family_supplier_active);
        assert!(ProtectedServiceBaseV1::resolve_v1(&prefix, &params, &extras, 100, &profile).is_err());
        extras.tir_public_conviction_active = true;
        let resolved = ProtectedServiceBaseV1::resolve_v1(&prefix, &params, &extras, 100, &profile).unwrap();
        assert_eq!(resolved.book_v1().budget, profile.budget);
        assert_eq!(resolved.prior_v1().0.source_profile_id, profile.id());
        assert!(prefix.court_service_state_v1().is_none(), "planning still writes no book");

        let mut changed = profile;
        changed.budget.work += 1;
        assert!(ProtectedServiceBaseV1::resolve_v1(&prefix, &params, &extras, 100, &changed).is_err());
        extras.protected_planning_v3 = None;
        assert!(ProtectedServiceBaseV1::resolve_v1(&prefix, &params, &extras, 100, &profile).is_err());
        let mut invalid = serde_json::to_value(declaration).unwrap();
        invalid["version"] = serde_json::json!(u16::MAX);
        extras.protected_planning_v3 = Some(serde_json::from_value(invalid).unwrap());
        extras.family_supplier_active = true;
        let legacy = params.with_family_supplier_from(Some(100));
        assert!(
            ProtectedServiceBaseV1::resolve_v1(&prefix, &legacy, &extras, 100, &profile).is_err(),
            "invalid declaration cannot fall back to legacy capacity"
        );
    }

    #[test]
    fn protected_service_bootstrap_requires_all_gates_and_keeps_pinned_capacity() {
        use crate::palw_court_service_v1::{ServicePacket, ServiceRefusal};
        use crate::palw_state_v2::{PalwStateParamsV2, PalwTransitionExtrasV1};
        let prefix = PalwChainStateV2::genesis();
        let params = PalwStateParamsV2::new(500, 10, 10, 10, 10, 100, Hash64::from_u64_word(1), 2, 1000, 1, 1000, 1).unwrap();
        let at = 100;
        let profile = crate::palw_court_service_binding_v1::PALW_COURT_SERVICE_PROFILE_V1.unwrap();
        for flags in 0..8 {
            let params = params.clone().with_family_supplier_from((flags & 1 != 0).then_some(at));
            let mut extras = PalwTransitionExtrasV1::default();
            extras.family_supplier_active = flags & 2 != 0;
            extras.tir_public_conviction_active = flags & 4 != 0;
            assert!(ProtectedServiceBaseV1::resolve_v1(&prefix, &params, &extras, at - 1, &profile).is_err());
            let resolved = ProtectedServiceBaseV1::resolve_v1(&prefix, &params, &extras, at, &profile);
            if flags != 7 {
                assert!(resolved.is_err(), "a missing native gate cannot bootstrap a namespace");
                continue;
            }
            let base = resolved.unwrap();
            let profile = crate::palw_court_service_binding_v1::PALW_COURT_SERVICE_PROFILE_V1.unwrap();
            let (state, released) = base.prior_v1();
            assert!(base.before.is_none() && released.is_none());
            assert_eq!(state.source_profile_id, profile.id());
            assert_eq!(state.profile_id, crate::palw_court_service_state_v2::court_service_profile_id_v2(profile.id()));
            assert_eq!(base.book_v1().budget, profile.budget);
            assert!(state.claims.is_empty());
            assert!(prefix.court_service_state_v1().is_none(), "quote resolution must not install its base");
            let mut book = base.book_v1().clone();
            let mut cost = profile.packet_cost;
            cost.work = profile.budget.work.checked_add(1).unwrap();
            let packet = ServicePacket { id: [1; 64], release_daa: at + 1, service_before: at + 2, cost };
            assert_eq!(book.reserve(&[packet], at + 10), Err(ServiceRefusal::NoCapacity));
            assert_eq!(&book, base.book_v1(), "bootstrap must not create extra capacity on refusal");
        }
    }

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
        assert_eq!(saved.as_ref(), Some(before));
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
