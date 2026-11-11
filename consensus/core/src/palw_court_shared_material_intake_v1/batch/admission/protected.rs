//! Reserve already compiled native cases together, including their real draft
//! expiry and retained-control checks. The caller still authenticates the
//! complete selected/outsider/defense opportunity set before compilation.
use super::*;
use crate::palw_batch_court_v1::protected_cases::{BatchCaseGraphV2, reserve_independent_batch_cases_v2};
use crate::palw_native_resource_v1::reserve_native_pending_requests_v1;

impl CompiledBatchAdmissionV1 {
    #[allow(dead_code)]
    pub(crate) fn reserve_protected_v2(
        cases: &[(Hash64, &Self)],
        book: &mut ServiceReservations,
        maximum_packets: usize,
    ) -> Result<(NativeRetainedV1<Vec<BatchAdmissionQuoteV1>>, NativeRetainedV1<Vec<CausalServicePacketV1>>), BatchCourtErrorV1> {
        Self::reserve_protected_checked_v2(cases, book, maximum_packets, |_, _| Ok(()))
    }

    /// The native owner's signed allowance is checked against every assigned
    /// quote before the shared candidate book is committed.
    pub(crate) fn reserve_protected_checked_v2(
        cases: &[(Hash64, &Self)],
        book: &mut ServiceReservations,
        maximum_packets: usize,
        check: impl FnOnce(&[BatchAdmissionQuoteV1], &ServiceReservations) -> Result<(), ServiceRefusal>,
    ) -> Result<(NativeRetainedV1<Vec<BatchAdmissionQuoteV1>>, NativeRetainedV1<Vec<CausalServicePacketV1>>), BatchCourtErrorV1> {
        use BatchCourtErrorV1::{CaseGraph, Clock, LocalResource};
        let first = cases.first().ok_or(CaseGraph)?.1;
        if cases.len() > maximum_packets
            || cases
                .iter()
                .any(|(_, case)| case.claim != first.claim || case.window != first.window || case.recovery_daa != first.recovery_daa)
        {
            return Err(CaseGraph);
        }
        let hard = first.window.claim_accepted_daa.checked_add(DISPUTE_WINDOW_DAA).ok_or(Clock)?;
        let mut requests = TirNativeRequestsV1::new();
        requests.exact_v1::<Vec<BatchAdmissionQuoteV1>>(1).map_err(LocalResource)?;
        requests.vec_v1::<BatchAdmissionQuoteV1>(cases.len() as u64).map_err(LocalResource)?;
        let owner = reserve_native_pending_requests_v1(requests.bytes_v1()).map_err(LocalResource)?;
        let mut scratch = TirNativeRequestsV1::new();
        scratch.vec_v1::<BatchCaseGraphV2<'_>>(cases.len() as u64).map_err(LocalResource)?;
        let _scratch_owner = reserve_native_pending_requests_v1(scratch.bytes_v1()).map_err(LocalResource)?;
        let graphs = cases
            .iter()
            .map(|(opportunity, case)| BatchCaseGraphV2 { opportunity: *opportunity, graph: &case.graph, terminal: case.terminal })
            .collect::<Vec<_>>();
        let mut quotes = Vec::new();
        // These are aggregate economic quantities, not a new service capacity.
        // Overflow must refuse the entire candidate before it commits.
        let mut totals = [0u64; 10];
        let (_, graph) =
            reserve_independent_batch_cases_v2(first.window, book, &graphs, first.recovery_daa, maximum_packets, |candidate| {
                for (_, case) in cases {
                    let mut retained = 0u64;
                    let mut account = |start, bytes: u64| {
                        let duration = case.window.material_until_daa.checked_sub(start).ok_or(ServiceRefusal::DeadlineExhausted)?;
                        retained =
                            bytes.checked_mul(duration).and_then(|n| retained.checked_add(n)).ok_or(ServiceRefusal::InvalidBudget)?;
                        Ok(())
                    };
                    for &(begin, finish, bytes) in &case.spans {
                        let begin = candidate.assigned_daa(&begin).ok_or(ServiceRefusal::InvalidState)?;
                        let finish = candidate.assigned_daa(&finish).ok_or(ServiceRefusal::InvalidState)?;
                        let expires = case
                            .storage
                            .draft_expires_v1(begin, hard, case.window.material_until_daa)
                            .map_err(|_| ServiceRefusal::DeadlineExhausted)?;
                        if finish >= expires {
                            return Err(ServiceRefusal::DeadlineExhausted);
                        }
                        account(begin, bytes)?;
                    }
                    for control in &case.controls {
                        account(candidate.assigned_daa(&control.packet).ok_or(ServiceRefusal::InvalidState)?, control.payload_bytes)?;
                    }
                    let at = candidate.assigned_daa(&case.terminal).ok_or(ServiceRefusal::InvalidState)?;
                    let required = at
                        .checked_sub(case.window.claim_accepted_daa)
                        .and_then(|n| n.checked_add(case.recovery_daa))
                        .and_then(|n| n.checked_add(1))
                        .ok_or(ServiceRefusal::InvalidInterval)?;
                    let quote = BatchAdmissionQuoteV1 {
                        claim: case.claim,
                        scope: case.scope,
                        relative_graph_id: case.relative_graph_id,
                        required_daa: case.required_daa.unwrap_or(required),
                        material_until_daa: case.window.material_until_daa,
                        service: case.service,
                        retained_payload_bytes: case.retained_payload_bytes,
                        retained_payload_byte_daa: retained,
                        carrier_storage_rent_sompi: case.carrier_storage_rent_sompi,
                        retained_numerical_fault_bytes: case.retained_numerical_fault_bytes,
                        retained_numerical_fault_byte_daa: case.retained_numerical_fault_byte_daa,
                        numerical_fault_storage_rent_sompi: case.numerical_fault_storage_rent_sompi,
                    };
                    for (total, value) in totals.iter_mut().zip([
                        quote.service.bytes,
                        quote.service.work,
                        quote.service.io,
                        quote.service.controls,
                        quote.retained_payload_bytes,
                        quote.retained_payload_byte_daa,
                        quote.carrier_storage_rent_sompi,
                        quote.retained_numerical_fault_bytes,
                        quote.retained_numerical_fault_byte_daa,
                        quote.numerical_fault_storage_rent_sompi,
                    ]) {
                        *total = total.checked_add(value).ok_or(ServiceRefusal::InvalidBudget)?;
                    }
                    quotes.push(quote);
                }
                // Shared funding/retention must fit even when each category
                // separately fits. Do this before the candidate commits.
                for (payload, fault) in [(4, 7), (5, 8), (6, 9)] {
                    totals[payload].checked_add(totals[fault]).ok_or(ServiceRefusal::InvalidBudget)?;
                }
                check(&quotes, candidate)
            })?;
        Ok((NativeRetainedV1::from_owner_reserved_v1(quotes, owner), graph))
    }

    /// Match this graph's actual assignments AND legal repair windows to the
    /// companion's promises before committing any of its protected ranks.
    /// This checks only stages present in this graph; it is not a substitute
    /// for the full profile's first-notice, localization or defense graphs.
    pub(crate) fn check_public_original_schedule_v1(
        &self,
        candidate: &ServiceReservations,
        schedule: misaka_palw_positive_contract::public_one_honest::VerificationSchedule,
    ) -> Result<(), ServiceRefusal> {
        let check = |id: &[u8; 64], deadline: u64| {
            let packet = candidate.packet(id).ok_or(ServiceRefusal::InvalidState)?;
            let assigned = candidate.assigned_daa(id).ok_or(ServiceRefusal::InvalidState)?;
            let latest = packet.service_before.checked_sub(1).ok_or(ServiceRefusal::InvalidInterval)?;
            if assigned > deadline || latest > deadline {
                return Err(ServiceRefusal::DeadlineExhausted);
            }
            Ok(())
        };
        let terminal_deadline = schedule.terminal_by().checked_sub(self.recovery_daa).ok_or(ServiceRefusal::DeadlineExhausted)?;
        // Selection Verify carries the signed local evidence itself. Its
        // canonical filing must precede filed_by, even though Terminal can
        // recheck and settle that retained evidence later.
        let selection_filing_deadline = schedule.filed_by().min(terminal_deadline);
        let late_delivery =
            |id: &[u8; 64]| self.late_selection.as_ref().is_some_and(|selection| selection.deliveries.binary_search(id).is_ok());
        let late_verification = |id: &[u8; 64]| self.late_selection.as_ref().is_some_and(|selection| selection.verification == *id);
        if let Some(selection) = &self.late_selection {
            let bind = candidate.packet(&selection.bind).ok_or(ServiceRefusal::InvalidState)?;
            // A material case can discover the row at the latest legal
            // localization boundary. Earlier reservation would erase its
            // continuation right even if an easy fixture already knew it.
            if bind.release_daa <= schedule.localized_by() {
                return Err(ServiceRefusal::DeadlineExhausted);
            }
            check(&selection.bind, schedule.filed_by())?;
            for id in &selection.deliveries {
                check(id, schedule.filed_by())?;
            }
            check(&selection.verification, selection_filing_deadline)?;
        }
        for (_, finish, _) in &self.spans {
            check(finish, if late_delivery(finish) { schedule.filed_by() } else { schedule.material_ready_by() })?;
        }
        // A verification may also retain an archive record. Storage ownership
        // never changes it into a later filing-only stage.
        for id in &self.inspection_packets {
            check(id, if late_verification(id) { selection_filing_deadline } else { schedule.inspection_by() })?;
        }
        for node in &self.graph {
            if node.packet.id != self.terminal && !self.controls.iter().any(|control| control.packet == node.packet.id) {
                let deadline = if late_delivery(&node.packet.id) {
                    schedule.filed_by()
                } else if late_verification(&node.packet.id) {
                    selection_filing_deadline
                } else {
                    schedule.inspection_by()
                };
                check(&node.packet.id, deadline)?;
            }
        }
        for control in &self.controls {
            check(&control.packet, if late_verification(&control.packet) { selection_filing_deadline } else { schedule.filed_by() })?;
        }
        check(&self.terminal, terminal_deadline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compiled(scope: u64) -> NativeRetainedV1<CompiledBatchAdmissionV1> {
        let h = Hash64::from_u64_word;
        let terminal = h(scope * 10).as_bytes();
        let verification = h(scope * 10 + 1).as_bytes();
        let demands = [MaximumBatchDemandV1 { ordinal: 5, range_maximums: &[PART + 1], predecessors: &[], verification }];
        let plan = BatchAdmissionV1 {
            claim: h(1),
            scope: h(scope),
            window: BatchCourtWindowV1 {
                claim_accepted_daa: 2,
                challenge_opened_daa: 3122,
                collect_until_daa: 10002,
                material_until_daa: 11000,
            },
            demands: &demands,
            retained_controls: &[],
            terminal,
            recovery_daa: 1,
            maximum_packets: 7,
        };
        let storage = MaterialIntakeScheduleV1 {
            sompi_per_byte: 2,
            draft_ttl_daa: 128,
            cover: crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalParameterCoverBudgetV1 {
                max_original_leaves: 1,
                max_total_edges: 1,
                max_total_coordinates: 1,
                max_root_memberships: 1,
            },
        };
        let cost = |bytes| ServiceCost { bytes, work: 1, io: 1, controls: 1 };
        let graph = [terminal, verification]
            .into_iter()
            .map(|id| CausalServicePacketV1 {
                packet: ServicePacket { id, release_daa: 3123, service_before: 10001, cost: cost(1) },
                predecessors: vec![],
            })
            .collect();
        plan.compile_with_v1(graph, storage, |_, bytes| Ok(cost(bytes)), reserve_native_pending_requests_v1, None).unwrap()
    }

    #[test]
    fn protected_compiled_cases_check_every_draft_and_fee_before_atomic_commit() {
        let h = Hash64::from_u64_word;
        let a = compiled(2);
        let (mut b, b_owner) = compiled(3).into_parts_v1();
        let initial = ServiceReservations::new(ServiceCost { bytes: 65536, work: 2, io: 2, controls: 2 }).unwrap();
        let mut book = initial.clone();
        let (quotes, graph) = CompiledBatchAdmissionV1::reserve_protected_v2(&[(h(100), &a), (h(101), &b)], &mut book, 14).unwrap();
        assert_eq!(graph.len(), 14);
        assert_eq!(quotes.len(), 2);
        book.check_causal_v1(&graph).unwrap();
        for (quote, case) in quotes.iter().zip([&*a, &b]) {
            assert_eq!(quote.scope, case.scope);
            assert_eq!(quote.relative_graph_id, case.relative_graph_id);
            assert_eq!(quote.retained_payload_bytes, PART + 1);
            assert_eq!(quote.service.controls, 7);
            let (begin, _, bytes) = case.spans[0];
            assert_eq!(quote.retained_payload_byte_daa, bytes * (11000 - book.assigned_daa(&begin).unwrap()));
        }
        let (_, owner) = quotes.into_parts_v1();
        assert!(owner.is_some());
        let mut refused = initial.clone();
        assert!(
            CompiledBatchAdmissionV1::reserve_protected_checked_v2(&[(h(100), &a), (h(101), &b)], &mut refused, 14, |quotes, _| {
                assert_eq!(quotes.len(), 2);
                Err(ServiceRefusal::InvalidBudget)
            },)
            .is_err()
        );
        assert_eq!(refused, initial, "signed allowance refusal must precede every book write");
        let mut reversed = initial.clone();
        CompiledBatchAdmissionV1::reserve_protected_v2(&[(h(101), &b), (h(100), &a)], &mut reversed, 14).unwrap();
        assert_eq!(book, reversed);
        let mut book = initial.clone();
        b.storage.draft_ttl_daa = 1;
        assert!(CompiledBatchAdmissionV1::reserve_protected_v2(&[(h(100), &a), (h(101), &b)], &mut book, 14).is_err());
        assert_eq!(book, initial, "second case expiry must not leave the first reserved");
        b.storage.draft_ttl_daa = 128;
        b.carrier_storage_rent_sompi = u64::MAX;
        assert!(matches!(
            CompiledBatchAdmissionV1::reserve_protected_v2(&[(h(100), &a), (h(101), &b)], &mut book, 14),
            Err(BatchCourtErrorV1::Reservation(ServiceRefusal::InvalidBudget))
        ));
        assert_eq!(book, initial, "aggregate fees overflow before either case is committed");
        b.carrier_storage_rent_sompi = 2 * (PART + 1);
        // Each category fits individually; their shared total does not.
        for field in 0..3 {
            match field {
                0 => b.retained_numerical_fault_bytes = u64::MAX,
                1 => b.retained_numerical_fault_byte_daa = u64::MAX,
                _ => b.numerical_fault_storage_rent_sompi = u64::MAX,
            }
            assert!(matches!(
                CompiledBatchAdmissionV1::reserve_protected_v2(&[(h(100), &a), (h(101), &b)], &mut book, 14),
                Err(BatchCourtErrorV1::Reservation(ServiceRefusal::InvalidBudget))
            ));
            assert_eq!(book, initial);
            b.retained_numerical_fault_bytes = 0;
            b.retained_numerical_fault_byte_daa = 0;
            b.numerical_fault_storage_rent_sompi = 0;
        }
        b.claim = h(9);
        assert!(CompiledBatchAdmissionV1::reserve_protected_v2(&[(h(100), &a), (h(101), &b)], &mut book, 14).is_err());
        assert_eq!(book, initial);
        drop(b_owner);
    }

    #[test]
    fn retained_verification_keeps_inspection_deadline_before_filing() {
        use misaka_palw_positive_contract::public_one_honest::{
            FirstNoticeDemand, MaterialCaseDemand, OriginalClock, VerificationSchedule,
        };
        let (mut case, _owner) = compiled(2).into_parts_v1();
        let verify = case.inspection_packets[0];
        // Give the verification an archive payload too. Its service window
        // could otherwise move beyond inspection, while still fitting filing.
        case.controls.push(BatchRetainedControlV1 { packet: verify, payload_bytes: 1 });
        for node in &mut case.graph {
            node.packet.service_before = if node.packet.id == verify {
                9000
            } else if node.packet.id == case.terminal {
                9800
            } else {
                4000
            };
        }
        let schedule = VerificationSchedule::new(
            OriginalClock::new(2).unwrap(),
            FirstNoticeDemand {
                draw_ready_by: 3,
                material_ready_by: 3,
                cold_acquisition_daa: 1,
                full_inspection_daa: 1,
                notice_inclusion_daa: 1,
                missing_material_detected_by: 3,
                material_request_inclusion_daa: 1,
            },
            MaterialCaseDemand {
                material_delivery_daa: 878,
                full_inspection_daa: 1000,
                localization_daa: 0,
                filing_daa: 4000,
                adjudication_and_recovery_daa: 800,
            },
        )
        .unwrap();
        assert_eq!((schedule.material_ready_by(), schedule.inspection_by(), schedule.filed_by()), (4000, 5000, 9000));
        let initial = ServiceReservations::new(ServiceCost { bytes: 65536, work: 2, io: 2, controls: 2 }).unwrap();
        let mut book = initial.clone();
        let result = CompiledBatchAdmissionV1::reserve_protected_checked_v2(
            &[(Hash64::from_u64_word(2), &case)],
            &mut book,
            7,
            |_, candidate| case.check_public_original_schedule_v1(candidate, schedule),
        );
        assert!(matches!(result, Err(BatchCourtErrorV1::Reservation(ServiceRefusal::DeadlineExhausted))));
        assert_eq!(book, initial);
    }
}
