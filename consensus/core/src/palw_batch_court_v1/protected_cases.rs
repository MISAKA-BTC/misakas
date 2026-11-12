//! Physical reservation of independently protected native case opportunities.
//! Authentication, normalized slot assignment and complete fee/DA policy are
//! the original compiler's responsibility. This module cannot create a right.
use super::{BatchCourtErrorV1 as Error, BatchCourtWindowV1};
use crate::{
    Hash64,
    palw_court_service_v1::{ServicePacket, ServiceRefusal, ServiceReservations, causal::CausalServicePacketV1},
    palw_native_resource_v1::{
        NativeCoverageV1, NativeRetainedV1, TirNativeRequestsV1, btree_node_envelope_bytes_v1, reserve_native_pending_requests_v1,
    },
};
use std::collections::BTreeMap;

/// A prebooked opportunity, whose identity does not depend on a later notice.
/// A bound CaseId may authorize use of its packets; it cannot rename them.
pub(crate) struct BatchCaseGraphV2<'a> {
    pub opportunity: Hash64,
    pub graph: &'a [CausalServicePacketV1],
    pub terminal: [u8; 64],
}

fn exact_vec<T>(count: usize) -> Result<Vec<T>, Error> {
    let error = || Error::LocalResource(NativeCoverageV1::unsupported::<Vec<T>>("protected Batch graph", "exact allocation refused"));
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|_| error())?;
    if values.capacity() != count {
        return Err(error());
    }
    Ok(values)
}

/// Reserve all selected/outsider/defense graphs in ONE candidate of the common
/// book. A predecessor can only be a packet of its own opportunity. Reusing
/// authenticated material bytes does not add a cross-case service dependency.
/// There is no synthetic terminal whose completion could veto another case.
#[allow(dead_code)]
pub(crate) fn reserve_independent_batch_cases_v2(
    window: BatchCourtWindowV1,
    book: &mut ServiceReservations,
    cases: &[BatchCaseGraphV2<'_>],
    recovery_daa: u64,
    maximum_packets: usize,
    check_assignments: impl FnOnce(&ServiceReservations) -> Result<(), ServiceRefusal>,
) -> Result<(u64, NativeRetainedV1<Vec<CausalServicePacketV1>>), Error> {
    reserve_independent_batch_cases_with_requests_v3(window, book, cases, None, recovery_daa, maximum_packets, check_assignments)
}

/// Exactly one pre-notice request per protected continuation. The native
/// owner derives the request identity, timing, price and authenticated rank.
pub(crate) fn reserve_independent_batch_cases_with_requests_v3(
    window: BatchCourtWindowV1,
    book: &mut ServiceReservations,
    cases: &[BatchCaseGraphV2<'_>],
    requests: Option<&[ServicePacket]>,
    recovery_daa: u64,
    maximum_packets: usize,
    check_assignments: impl FnOnce(&ServiceReservations) -> Result<(), ServiceRefusal>,
) -> Result<(u64, NativeRetainedV1<Vec<CausalServicePacketV1>>), Error> {
    reserve_batch_and_early_duties_v1(window, book, cases, requests, &[], recovery_daa, maximum_packets, check_assignments)
}

pub(crate) fn reserve_batch_and_early_duties_v1(
    window: BatchCourtWindowV1,
    book: &mut ServiceReservations,
    cases: &[BatchCaseGraphV2<'_>],
    requests: Option<&[ServicePacket]>,
    early: &[CausalServicePacketV1],
    recovery_daa: u64,
    maximum_packets: usize,
    check_assignments: impl FnOnce(&ServiceReservations) -> Result<(), ServiceRefusal>,
) -> Result<(u64, NativeRetainedV1<Vec<CausalServicePacketV1>>), Error> {
    reserve_batch_and_early_duties_with_request_series_v2(
        window,
        book,
        cases,
        requests.map(|requests| (requests, 1)),
        early,
        recovery_daa,
        maximum_packets,
        check_assignments,
    )
}

/// The flat request list is ordered by case and then by opportunity. Every
/// request in a case precedes its post-notice work for capacity planning.
/// These edges do not require every opportunity to carry an actual request.
#[allow(clippy::too_many_arguments)]
pub(crate) fn reserve_batch_and_early_duties_with_request_series_v2(
    window: BatchCourtWindowV1,
    book: &mut ServiceReservations,
    cases: &[BatchCaseGraphV2<'_>],
    requests: Option<(&[ServicePacket], usize)>,
    early: &[CausalServicePacketV1],
    recovery_daa: u64,
    maximum_packets: usize,
    check_assignments: impl FnOnce(&ServiceReservations) -> Result<(), ServiceRefusal>,
) -> Result<(u64, NativeRetainedV1<Vec<CausalServicePacketV1>>), Error> {
    if cases.is_empty() || cases.len() > maximum_packets {
        return Err(Error::CaseGraph);
    }
    if requests.is_some_and(|(requests, per_case)| per_case == 0 || cases.len().checked_mul(per_case) != Some(requests.len())) {
        return Err(Error::CaseGraph);
    }
    let requests_for = |index: usize| requests.map_or(&[][..], |(requests, per_case)| &requests[index * per_case..][..per_case]);
    let initial_count = early.len().checked_add(requests.map_or(0, |(requests, _)| requests.len())).ok_or(Error::CaseGraph)?;
    if initial_count > maximum_packets {
        return Err(Error::CaseGraph);
    }
    let nodes = cases.iter().try_fold(initial_count, |sum, case| {
        if case.opportunity == Hash64::default() || case.graph.is_empty() {
            return Err(Error::CaseGraph);
        }
        sum.checked_add(case.graph.len()).filter(|sum| *sum <= maximum_packets).ok_or(Error::CaseGraph)
    })?;
    let early_edges =
        early.iter().try_fold(0u64, |sum, node| sum.checked_add(node.predecessors.len() as u64)).ok_or(Error::CaseGraph)?;
    let edges = cases.iter().flat_map(|case| case.graph).try_fold(early_edges, |sum, node| {
        let count = if node.predecessors.is_empty() { requests.map_or(0, |(_, per_case)| per_case) } else { node.predecessors.len() };
        sum.checked_add(count as u64).ok_or(Error::CaseGraph)
    })?;
    let mut owned = TirNativeRequestsV1::new();
    owned.exact_v1::<Vec<CausalServicePacketV1>>(1).map_err(Error::LocalResource)?;
    owned.exact_v1::<CausalServicePacketV1>(nodes as u64).map_err(Error::LocalResource)?;
    owned.exact_v1::<[u8; 64]>(edges).map_err(Error::LocalResource)?;
    let owner = reserve_native_pending_requests_v1(owned.bytes_v1()).map_err(Error::LocalResource)?;
    let mut scratch = TirNativeRequestsV1::new();
    scratch.vec_v1::<[u8; 64]>(cases.len() as u64).map_err(Error::LocalResource)?;
    scratch.vec_v1::<[u8; 64]>(initial_count as u64).map_err(Error::LocalResource)?;
    scratch.vec_v1::<[u8; 64]>(early.len() as u64).map_err(Error::LocalResource)?;
    scratch
        .exact_v1::<u8>(
            (nodes as u64)
                .checked_mul(btree_node_envelope_bytes_v1::<[u8; 64], usize>() as u64)
                .and_then(|n| n.checked_add((cases.len() as u64).checked_mul(btree_node_envelope_bytes_v1::<Hash64, ()>() as u64)?))
                .ok_or(Error::CaseGraph)?,
        )
        .map_err(Error::LocalResource)?;
    let _scratch_owner = reserve_native_pending_requests_v1(scratch.bytes_v1()).map_err(Error::LocalResource)?;
    let mut opportunities = BTreeMap::new();
    let mut membership = BTreeMap::new();
    let mut terminals = exact_vec(cases.len())?;
    let mut initial_ids = exact_vec(initial_count)?;
    let mut early_ids = exact_vec(early.len())?;
    for node in early {
        if membership.insert(node.packet.id, cases.len()).is_some() {
            return Err(Error::CaseGraph);
        }
        initial_ids.push(node.packet.id);
        early_ids.push(node.packet.id);
    }
    for (index, case) in cases.iter().enumerate() {
        if opportunities.insert(case.opportunity, ()).is_some() {
            return Err(Error::CaseGraph);
        }
        for request in requests_for(index) {
            let id = request.id;
            if membership.insert(id, index).is_some() {
                return Err(Error::CaseGraph);
            }
            initial_ids.push(id);
        }
        for node in case.graph {
            if membership.insert(node.packet.id, index).is_some() {
                return Err(Error::CaseGraph);
            }
        }
        terminals.push(case.terminal);
    }
    for (index, case) in cases.iter().enumerate() {
        if membership.get(&case.terminal) != Some(&index)
            || case.graph.iter().any(|node| node.predecessors.iter().any(|parent| membership.get(parent) != Some(&index)))
        {
            return Err(Error::CaseGraph);
        }
    }
    let mut graph = exact_vec(nodes)?;
    for node in early {
        let mut predecessors = exact_vec(node.predecessors.len())?;
        predecessors.extend_from_slice(&node.predecessors);
        graph.push(CausalServicePacketV1 { packet: node.packet.clone(), predecessors });
    }
    for (index, case) in cases.iter().enumerate() {
        let requests = requests_for(index);
        for request in requests {
            graph.push(CausalServicePacketV1 { packet: request.clone(), predecessors: Vec::new() });
        }
        for node in case.graph {
            let initial = if node.predecessors.is_empty() { requests } else { &[] };
            let mut predecessors = exact_vec(node.predecessors.len().checked_add(initial.len()).ok_or(Error::CaseGraph)?)?;
            predecessors.extend_from_slice(&node.predecessors);
            predecessors.extend(initial.iter().map(|request| request.id));
            graph.push(CausalServicePacketV1 { packet: node.packet.clone(), predecessors });
        }
    }
    graph.sort_unstable_by_key(|node| node.packet.id);
    for node in &mut graph {
        node.predecessors.sort_unstable();
    }
    initial_ids.sort_unstable();
    early_ids.sort_unstable();
    let required = window.reserve_initial_request_many_with_v3(
        book,
        &graph,
        &terminals,
        &initial_ids,
        &early_ids,
        recovery_daa,
        |candidate, _| check_assignments(candidate),
        reserve_native_pending_requests_v1,
    )?;
    Ok((required, NativeRetainedV1::from_owner_reserved_v1(graph, owner)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_court_policy_v1::{DISPUTE_WINDOW_DAA, NOTICE_WINDOW_DAA};
    use crate::palw_court_service_v1::{ServiceCost, ServicePacket};

    const UNIT: ServiceCost = ServiceCost { bytes: 17, work: 5, io: 3, controls: 1 };

    fn window() -> BatchCourtWindowV1 {
        BatchCourtWindowV1 {
            claim_accepted_daa: 10,
            challenge_opened_daa: 10 + NOTICE_WINDOW_DAA,
            collect_until_daa: 10 + DISPUTE_WINDOW_DAA,
            material_until_daa: 11 + DISPUTE_WINDOW_DAA,
        }
    }

    fn graph(first_id: u8) -> Vec<CausalServicePacketV1> {
        let first = window().challenge_opened_daa + 1;
        (0..3)
            .map(|offset| CausalServicePacketV1 {
                packet: ServicePacket { id: [first_id + offset; 64], release_daa: first, service_before: first + 3, cost: UNIT },
                predecessors: if offset == 0 { vec![] } else { vec![[first_id + offset - 1; 64]] },
            })
            .collect()
    }

    fn case<'a>(id: u64, graph: &'a [CausalServicePacketV1]) -> BatchCaseGraphV2<'a> {
        BatchCaseGraphV2 { opportunity: Hash64::from_u64_word(id), graph, terminal: graph[2].packet.id }
    }

    fn book() -> ServiceReservations {
        ServiceReservations::new(ServiceCost { bytes: UNIT.bytes * 2, work: UNIT.work * 2, io: UNIT.io * 2, controls: 2 }).unwrap()
    }

    #[test]
    fn protected_batch_cases_reserve_sum_capacity_atomically_without_a_common_terminal() {
        let (a, b) = (graph(1), graph(11));
        // Each case alone fits the one-unit pool; both together do not.
        for nodes in [&a, &b] {
            let mut one = ServiceReservations::new(UNIT).unwrap();
            reserve_independent_batch_cases_v2(window(), &mut one, &[case(1, nodes)], 1, 6, |_| Ok(())).unwrap();
        }
        let mut too_small = ServiceReservations::new(UNIT).unwrap();
        let saved = too_small.clone();
        let failure = reserve_independent_batch_cases_v2(window(), &mut too_small, &[case(1, &a), case(2, &b)], 1, 6, |_| Ok(()));
        assert!(matches!(failure, Err(Error::Reservation(ServiceRefusal::NoCapacity))));
        assert_eq!(too_small, saved);

        let mut together = book();
        let (required, combined) =
            reserve_independent_batch_cases_v2(window(), &mut together, &[case(1, &a), case(2, &b)], 1, 6, |candidate| {
                assert!(candidate.assigned_daa(&[3; 64]).is_some());
                assert!(candidate.assigned_daa(&[13; 64]).is_some());
                Ok(())
            })
            .unwrap();
        assert_eq!(required, NOTICE_WINDOW_DAA + 5);
        assert_eq!(combined.len(), 6, "no synthetic common terminal is appended");
        let first = window().challenge_opened_daa + 1;
        for offset in 0..3 {
            assert_eq!(together.assigned_daa(&[1 + offset; 64]), Some(first + u64::from(offset)));
            assert_eq!(together.assigned_daa(&[11 + offset; 64]), Some(first + u64::from(offset)));
        }
        together.check_causal_v1(&combined).unwrap();
        let mut reversed = book();
        let (_, reversed_graph) =
            reserve_independent_batch_cases_v2(window(), &mut reversed, &[case(2, &b), case(1, &a)], 1, 6, |_| Ok(())).unwrap();
        assert_eq!(together, reversed, "caller ordering cannot steal a sibling's reservation");
        assert_eq!(*combined, *reversed_graph);
        let (_, owner) = combined.into_parts_v1();
        assert!(owner.is_some(), "ordinary nodes retain the actual merged graph owner");

        // An existing promise consumes the same physical pool.
        let mut occupied = book();
        let prior = CausalServicePacketV1 {
            packet: ServicePacket { id: [99; 64], release_daa: first, service_before: first + 1, cost: UNIT },
            predecessors: vec![],
        };
        occupied.reserve_causal_v1(&[prior], window().collect_until_daa).unwrap();
        let saved = occupied.clone();
        assert!(matches!(
            reserve_independent_batch_cases_v2(window(), &mut occupied, &[case(1, &a), case(2, &b)], 1, 6, |_| Ok(())),
            Err(Error::Reservation(ServiceRefusal::DeadlineExhausted))
        ));
        assert_eq!(occupied, saved);

        let mut refused = book();
        let saved = refused.clone();
        assert!(matches!(
            reserve_independent_batch_cases_v2(window(), &mut refused, &[case(1, &a), case(2, &b)], 1, 6, |_| Err(
                ServiceRefusal::InvalidBudget
            )),
            Err(Error::Reservation(ServiceRefusal::InvalidBudget))
        ));
        assert_eq!(refused, saved, "a late native assignment check commits neither case");
    }

    #[test]
    fn initial_requests_and_late_cases_share_one_atomic_pool() {
        let (a, b) = (graph(1), graph(11));
        let request = |id| ServicePacket { id: [id; 64], release_daa: 20, service_before: 21, cost: UNIT };
        let requests = [request(101), request(102)];
        let cases = [case(1, &a), case(2, &b)];
        let mut together = book();
        let (required, combined) =
            reserve_independent_batch_cases_with_requests_v3(window(), &mut together, &cases, Some(&requests), 1, 8, |_| Ok(()))
                .unwrap();
        assert_eq!(required, NOTICE_WINDOW_DAA + 5, "early DAA is not charged again after notice");
        assert_eq!(combined.len(), 8);
        for (index, nodes) in [&a, &b].iter().enumerate() {
            assert_eq!(together.assigned_daa(&requests[index].id), Some(20));
            let root = combined.iter().find(|node| node.packet.id == nodes[0].packet.id).unwrap();
            assert_eq!(root.predecessors, vec![requests[index].id]);
        }
        together.check_causal_v1(&combined).unwrap();
        let mut reversed = book();
        reserve_independent_batch_cases_with_requests_v3(
            window(),
            &mut reversed,
            &[case(2, &b), case(1, &a)],
            Some(&[requests[1].clone(), requests[0].clone()]),
            1,
            8,
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(together, reversed);

        // An existing early obligation competes in the same physical pool.
        let mut occupied = book();
        occupied.reserve_causal_v1(&[CausalServicePacketV1 { packet: request(103), predecessors: vec![] }], 21).unwrap();
        let saved = occupied.clone();
        assert!(
            reserve_independent_batch_cases_with_requests_v3(window(), &mut occupied, &cases, Some(&requests), 1, 8, |_| Ok(()))
                .is_err()
        );
        assert_eq!(occupied, saved);

        let mut candidate = book();
        let saved = candidate.clone();
        assert!(
            reserve_independent_batch_cases_with_requests_v3(window(), &mut candidate, &cases, Some(&requests), 1, 8, |_| Err(
                ServiceRefusal::InvalidBudget
            ))
            .is_err()
        );
        assert_eq!(candidate, saved, "late funding refusal must undo both early requests too");
        let mut late = requests.clone();
        late[0].release_daa = window().challenge_opened_daa + 1;
        late[0].service_before = late[0].release_daa + 1;
        for (prefix, cap) in [(&late[..], 8), (&requests[..1], 8), (&requests[..], 7)] {
            assert!(
                reserve_independent_batch_cases_with_requests_v3(window(), &mut candidate, &cases, Some(prefix), 1, cap, |_| Ok(()))
                    .is_err()
            );
            assert_eq!(candidate, saved);
        }
    }

    #[test]
    fn protected_batch_cases_reject_foreign_edges_missing_terminals_and_omitted_nodes() {
        let (a, mut b) = (graph(1), graph(11));
        let mut book = book();
        let saved = book.clone();
        let mut refuse = |cases: &[BatchCaseGraphV2<'_>], max| {
            assert!(reserve_independent_batch_cases_v2(window(), &mut book, cases, 1, max, |_| Ok(())).is_err());
            assert_eq!(book, saved);
        };
        refuse(&[], 6);
        refuse(&[case(1, &a), case(2, &b)], 5);
        refuse(&[case(1, &a), case(1, &b)], 6);
        refuse(&[case(1, &a), case(2, &a)], 6);
        b[0].predecessors.push(a[2].packet.id);
        refuse(&[case(1, &a), case(2, &b)], 6);
        b[0].predecessors.clear();
        b[2].predecessors.clear();
        refuse(&[case(1, &a), case(2, &b)], 6);
        let prior = b[1].packet.id;
        b[2].predecessors.push(prior);
        let mut wrong = case(2, &b);
        wrong.terminal = a[2].packet.id;
        refuse(&[case(1, &a), wrong], 6);
        b[2].packet.service_before = window().collect_until_daa;
        refuse(&[case(1, &a), case(2, &b)], 6);
    }
}
