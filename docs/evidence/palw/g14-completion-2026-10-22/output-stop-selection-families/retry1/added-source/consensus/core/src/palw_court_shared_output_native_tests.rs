// Actual native prepared original/shared state. Production pledge preparation
// keeps the existing verified-half fixture assumptions; not mined admission.
#[test]
fn shared_original_output_duty_derives_complete_fullids_dependencies_and_rechecks() {
    use crate::palw_court_shared_tir_builder_store_v1::{
        SharedTirBuilderStoreV1 as Store,
        output_terminal::{
            NativeOutputDutyV1 as Duty, OUTPUT_INTENT_CONTEXT_V1 as CONTEXT,
            OutputDecisionV1 as Decision, OutputDutyScopeV1 as Coverage,
            OutputRuntimeGuardV1 as Guard, SignedOutputTerminalIntentV1 as Intent,
            ids_material_id_v1,
        },
    };
    use crate::palw_court_shared_tir_state_v1 as shared;
    use crate::palw_court_source_material_graph_v3::{
        NativeSourceMaterialGraphV3 as Graph, SourceGraphBudgetV3 as Budget,
        SourceMaterialNodeKindV3 as Node, SourceMaterialTargetV3 as Target,
    };
    for fault in [false, true] {
        let fx = if fault {
            crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_lying_output_v1(
                2,
                h(0xFA01),
            )
        } else {
            crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_v1(None, 2)
        };
        let frame = &fx.frame;
        let p = frame.plan_v1();
        let claim = p.original_claim;
        assert!(
            p.semantic.is_none(),
            "this fixture isolates Output, not another semantic predicate"
        );
        let (mut state, params, extras) = world(&fx, frame);
        let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([111; 32]);
        state.bonds.get_mut(&outsider()).unwrap().pubkey = key.verification_key.as_ref().to_vec();
        let state = bind_clock_service(&state, frame, &params, &extras);
        let source = crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::new_native_v1(
            &fx.record,
            &fx.layout,
            frame.header_v1(),
            frame.original_v1().execution_root,
            frame.original_v1().trace_root,
            fx.rules.maximum_leaves,
        )
        .unwrap();
        let profile =
            shared::shared_native_profile_id_v1(state.court_service_state_v1().unwrap().profile_id);
        let set =
            crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame, &source, profile)
                .unwrap();
        let scope = set.scope_id().unwrap();
        let context = shared::SharedTirContextV1 {
            version: 1,
            claim,
            allocation_id: p.allocation_id,
            source_id: p.source_id,
            rules_id: p.rules_id,
            profile_id: profile,
            scope_id: scope,
            queries: set.len(),
            original_accepted_daa: p.terms.accepted_daa,
            hard_daa: p.terms.accepted_daa + 10_000,
            material_until_daa: p.terms.material_until_daa,
        };
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        b.write_staged_row_v1(
            Key::CourtSharedContext(claim),
            Some(Row::CourtSharedContext(Box::new(context))),
        )
        .unwrap();
        let budget = Budget {
            max_queries: 10_000,
            max_roles: 8,
            max_vertices: 1_000_000,
            max_edges: 10_000_000,
        };
        let graph = Graph::derive_native_v3(frame, &source, profile, budget).unwrap();
        let duty = Duty::derive_native_v1(frame, &source, &graph, profile).unwrap();
        assert_eq!(
            duty.scope_v1(),
            Coverage::OriginalOutputAndNativeRegularStopOnly
        );
        assert_eq!(
            duty.runtime_guard_v1(),
            Guard::AnyActualAuthenticatedFullIdsForCurrentSource
        );
        let finish = graph
            .nodes_v3()
            .filter_map(|n| match n.kind_v3() {
                Node::Finish {
                    target: Target::FullIds,
                    role,
                } => Some((*role, n.id_v3())),
                _ => None,
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        assert!(!finish.is_empty());
        assert_eq!(
            duty.allocation_dependencies_v1().len() as u64,
            graph.role_slots_v3()
        );
        for dependency in duty.allocation_dependencies_v1() {
            assert_eq!(
                finish.get(&dependency.role),
                Some(&dependency.logical_finish)
            );
            assert_eq!(
                dependency.absolute_finish,
                graph.absolute_packet_id_v3(dependency.logical_finish)
            );
        }
        assert_ne!(duty.logical_id_v1(), Hash64::default());
        assert_ne!(duty.absolute_id_v1(), Hash64::default());
        assert!(Duty::derive_native_v1(frame, &source, &graph, h(0xFA02)).is_err());
        let other = if fault {
            crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_v1(None, 2)
        } else {
            crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_lying_output_v1(
                2,
                h(0xFA01),
            )
        };
        let other_source = crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::new_native_v1(
            &other.record,
            &other.layout,
            other.frame.header_v1(),
            other.frame.original_v1().execution_root,
            other.frame.original_v1().trace_root,
            other.rules.maximum_leaves,
        )
        .unwrap();
        assert_ne!(other.frame.plan_v1().original_claim, claim);
        let other_graph =
            Graph::derive_native_v3(&other.frame, &other_source, profile, budget).unwrap();
        assert!(
            Duty::derive_native_v1(frame, &source, &other_graph, profile).is_err(),
            "another original cannot lend its dependency scope"
        );

        assert!(matches!(
            Graph::derive_native_v3(
                frame,
                &source,
                profile,
                Budget {
                    max_roles: graph.role_slots_v3() - 1,
                    ..budget
                }
            ),
            Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_))
        ));
        let rows = crate::palw_step_refute::tiled_logits_rows_root_v1(
            &fx.execution.binding.job_context,
            &fx.execution.rows,
        )
        .unwrap();
        let ids = source
            .retain_complete_ids_v1(rows, &fx.execution.generated)
            .unwrap();
        let store = Store::new(&b.state, claim, scope, &[], &[], Some(&ids)).unwrap();
        let rules = crate::palw_court_close_receipt_v1::NativeCourtRulesV1 {
            network_domain: extras.tir_public_network_domain_v1.unwrap(),
            at: at(frame, 1),
            court: fx.rules.court,
            step_ladder: fx.rules.maximum_leaves,
            prompt_ids_merkle: false,
            held_regime: false,
            decode_close_convicts_only: false,
            fp_decode_from_daa: None,
            cell_closure_from_daa: None,
            memory_routing_from_daa: None,
            tir_active: true,
            gen_active: false,
            task_heads_active: false,
            improvement_active: false,
            constraint_active: false,
        };
        let mut intent = Intent {
            version: 1,
            claim,
            scope_id: scope,
            policy_id: crate::palw_court_shared_tir_root_v1::shared_tir_root_policy_id_v1(&rules)
                .unwrap(),
            duty_id: duty.absolute_id_v1(),
            material_id: ids_material_id_v1(&ids).unwrap(),
            filer: outsider(),
            signature: vec![],
        };
        let sign = |i: &mut Intent| {
            i.signature = libcrux_ml_dsa::ml_dsa_87::sign(
                &key.signing_key,
                i.signing_message(rules.network_domain)
                    .unwrap()
                    .as_byte_slice(),
                CONTEXT,
                [112; 32],
            )
            .unwrap()
            .as_ref()
            .to_vec();
        };
        sign(&mut intent);
        let before = b.state.state_root();
        let evidence = store
            .build_output_terminal_v1(&b.state, &params, &rules, &intent, &duty)
            .unwrap();
        let result = evidence.recheck(&store, &b.state, &params, &rules).unwrap();
        if fault {
            let Decision::Fault {
                kind,
                claim: found,
                execution,
                computed,
            } = result
            else {
                panic!("actual false output commitment must fail")
            };
            assert_eq!(
                kind,
                crate::palw_tir_objective_v1::PalwTirOutputFaultV1::OutputRootMismatch
            );
            assert_eq!(found, claim);
            assert_eq!(execution, frame.original_v1().execution_root);
            assert_eq!(
                computed,
                crate::palw_attempt_rules_v1::palw_attempt_output_root_v1(
                    &source.header_v1().job_context,
                    &fx.execution.generated
                )
            );
            assert_ne!(computed, frame.original_v1().output_root);
        } else {
            assert_eq!(result, Decision::OutputHolds);
        }
        use crate::palw_court_shared_tir_builder_store_v1::output_terminal::{
            OriginalTerminalFamilyV1, REGULAR_STOP_CONTEXT_V1, RegularStopDecisionV1,
            RegularStopRefusalV1, SignedRegularStopIntentV1 as StopIntent,
            TerminalAllocationParentsV1,
        };
        let nodes = duty.family_nodes_v1().collect::<Vec<_>>();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].family, OriginalTerminalFamilyV1::OutputIdentity);
        assert_eq!(nodes[1].family, OriginalTerminalFamilyV1::NativeRegularStop);
        assert_eq!(
            nodes[0].parents,
            TerminalAllocationParentsV1::AllFullIdsFinishes(duty.allocation_dependencies_v1())
        );
        assert_eq!(
            nodes[1].parents,
            TerminalAllocationParentsV1::OutputIdentity {
                logical: duty.logical_id_v1(),
                absolute: duty.absolute_id_v1()
            }
        );
        let mut si = StopIntent {
            version: 1,
            claim,
            scope_id: scope,
            policy_id: intent.policy_id,
            duty_id: duty.native_regular_stop_id_v1().unwrap(),
            material_id: intent.material_id,
            decode_count: frame.original_v1().decode_tokens_executed,
            stop_reason: frame.original_v1().stop_reason,
            filer: outsider(),
            signature: vec![],
        };
        let stop_sign = |s: &mut StopIntent| {
            s.signature = libcrux_ml_dsa::ml_dsa_87::sign(
                &key.signing_key,
                s.signing_message(rules.network_domain)
                    .unwrap()
                    .as_byte_slice(),
                REGULAR_STOP_CONTEXT_V1,
                [114; 32],
            )
            .unwrap()
            .as_ref()
            .to_vec();
        };
        stop_sign(&mut si);
        if fault {
            assert!(matches!(
                store.build_regular_stop_terminal_v1(&b.state, &params, &rules, &si, &duty),
                Err(RegularStopRefusalV1::OutputPrerequisite(
                    crate::palw_tir_objective_v1::PalwTirOutputFaultV1::OutputRootMismatch
                ))
            ));
        } else {
            let e = store
                .build_regular_stop_terminal_v1(&b.state, &params, &rules, &si, &duty)
                .unwrap();
            assert_eq!(
                e.recheck(&store, &b.state, &params, &rules).unwrap(),
                RegularStopDecisionV1::StopRuleHolds
            );
        }
        for which in 0..5 {
            let mut bad = si.clone();
            match which {
                0 => bad.decode_count += 1,
                1 => bad.scope_id = h(0xFA11),
                2 => bad.duty_id = h(0xFA12),
                3 => bad.material_id = h(0xFA13),
                _ => {
                    bad.stop_reason = match si.stop_reason {
                        crate::palw_freeprompt_v3::PalwFpStopReasonV3::ExactBudgetReached => {
                            crate::palw_freeprompt_v3::PalwFpStopReasonV3::EndOfGeneration
                        }
                        _ => crate::palw_freeprompt_v3::PalwFpStopReasonV3::ExactBudgetReached,
                    }
                }
            }
            stop_sign(&mut bad);
            assert!(
                store
                    .build_regular_stop_terminal_v1(&b.state, &params, &rules, &bad, &duty)
                    .is_err()
            );
        }
        let mut corrupt_stop = si.clone();
        corrupt_stop.signature[0] ^= 1;
        assert!(
            store
                .build_regular_stop_terminal_v1(&b.state, &params, &rules, &corrupt_stop, &duty)
                .is_err()
        );
        let no_stop_ids = Store::new(&b.state, claim, scope, &[], &[], None).unwrap();
        assert!(
            no_stop_ids
                .build_regular_stop_terminal_v1(&b.state, &params, &rules, &si, &duty)
                .is_err()
        );
        let mut late_stop = rules.clone();
        late_stop.at.daa_score = p.terms.accepted_daa + 10_000;
        assert!(
            store
                .build_regular_stop_terminal_v1(&b.state, &params, &late_stop, &si, &duty)
                .is_err()
        );
        let missing = Store::new(&b.state, claim, scope, &[], &[], None).unwrap();
        assert!(
            missing
                .build_output_terminal_v1(&b.state, &params, &rules, &intent, &duty)
                .is_err(),
            "potential finishes cannot replace actual FullIds"
        );
        for which in 0..3 {
            let mut wrong = intent.clone();
            match which {
                0 => wrong.duty_id = h(0xFA03),
                1 => wrong.scope_id = h(0xFA04),
                _ => wrong.material_id = h(0xFA05),
            }
            sign(&mut wrong);
            assert!(
                store
                    .build_output_terminal_v1(&b.state, &params, &rules, &wrong, &duty)
                    .is_err()
            );
        }
        let mut wrong = intent.clone();
        wrong.signature[0] ^= 1;
        assert!(
            store
                .build_output_terminal_v1(&b.state, &params, &rules, &wrong, &duty)
                .is_err()
        );
        let mut late = rules.clone();
        late.at.daa_score = p.terms.accepted_daa + 10_000;
        assert!(
            store
                .build_output_terminal_v1(&b.state, &params, &late, &intent, &duty)
                .is_err()
        );
        let mut replaced = b.state.clone();
        replaced.bonds.get_mut(&outsider()).unwrap().pubkey =
            libcrux_ml_dsa::ml_dsa_87::generate_key_pair([113; 32])
                .verification_key
                .as_ref()
                .to_vec();
        assert!(
            evidence
                .recheck(&store, &replaced, &params, &rules)
                .is_err()
        );
        assert!(
            store
                .build_regular_stop_terminal_v1(&replaced, &params, &rules, &si, &duty)
                .is_err()
        );
        assert_eq!(b.state.state_root(), before);
    }
}

// Actual native prepared original/shared state. Production pledge preparation
// keeps the existing verified-half fixture assumptions; not mined admission.
#[test]
fn shared_regular_stop_v4_uses_actual_output_dependency_and_native_predicate() {
    use crate::palw_court_shared_tir_builder_store_v1::{
        SharedTirBuilderStoreV1 as Store,
        output_terminal::{
            NativeOutputDutyV1 as Duty, OUTPUT_INTENT_CONTEXT_V1 as CONTEXT,
            OutputDecisionV1 as Decision, OutputDutyScopeV1 as Coverage,
            OutputRuntimeGuardV1 as Guard, SignedOutputTerminalIntentV1 as Intent,
            ids_material_id_v1,
        },
    };
    use crate::palw_court_shared_tir_state_v1 as shared;
    use crate::palw_court_source_material_graph_v3::{
        NativeSourceMaterialGraphV3 as Graph, SourceGraphBudgetV3 as Budget,
        SourceMaterialNodeKindV3 as Node, SourceMaterialTargetV3 as Target,
    };
    for stop_fault in [false, true] {
        let fault = false;
        let first = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_ruled_v1(
            4,
            |_| {},
            None,
        )
        .execution
        .generated[0];
        let config = crate::palw_decode_pipeline_v4::DecodeConfigV4 {
            repeat_penalty_q: 65_536,
            penalty_window: 0,
            frequency_penalty_q: 0,
            presence_penalty_q: 0,
            logit_bias: vec![],
            stop_sequences: if stop_fault {
                vec![vec![first]]
            } else {
                vec![]
            },
        };
        let fx = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_ruled_v1(
            4,
            move |job| {
                job.version = crate::palw_freeprompt_v3::PALW_FP_V4_VERSION;
                job.decode = Some(config.clone());
            },
            None,
        );
        let frame = &fx.frame;
        let p = frame.plan_v1();
        let claim = p.original_claim;
        assert!(
            p.semantic.is_none(),
            "this fixture isolates Output, not another semantic predicate"
        );
        let (mut state, params, extras) = world(&fx, frame);
        let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([111; 32]);
        state.bonds.get_mut(&outsider()).unwrap().pubkey = key.verification_key.as_ref().to_vec();
        let state = bind_clock_service(&state, frame, &params, &extras);
        let source = crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::new_native_v1(
            &fx.record,
            &fx.layout,
            frame.header_v1(),
            frame.original_v1().execution_root,
            frame.original_v1().trace_root,
            fx.rules.maximum_leaves,
        )
        .unwrap();
        let profile =
            shared::shared_native_profile_id_v1(state.court_service_state_v1().unwrap().profile_id);
        let set =
            crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame, &source, profile)
                .unwrap();
        let scope = set.scope_id().unwrap();
        let context = shared::SharedTirContextV1 {
            version: 1,
            claim,
            allocation_id: p.allocation_id,
            source_id: p.source_id,
            rules_id: p.rules_id,
            profile_id: profile,
            scope_id: scope,
            queries: set.len(),
            original_accepted_daa: p.terms.accepted_daa,
            hard_daa: p.terms.accepted_daa + 10_000,
            material_until_daa: p.terms.material_until_daa,
        };
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        b.write_staged_row_v1(
            Key::CourtSharedContext(claim),
            Some(Row::CourtSharedContext(Box::new(context))),
        )
        .unwrap();
        let budget = Budget {
            max_queries: 10_000,
            max_roles: 8,
            max_vertices: 1_000_000,
            max_edges: 10_000_000,
        };
        let graph = Graph::derive_native_v3(frame, &source, profile, budget).unwrap();
        let duty = Duty::derive_native_v1(frame, &source, &graph, profile).unwrap();
        assert_eq!(
            duty.scope_v1(),
            Coverage::OriginalOutputAndNativeRegularStopOnly
        );
        assert_eq!(
            duty.runtime_guard_v1(),
            Guard::AnyActualAuthenticatedFullIdsForCurrentSource
        );
        let finish = graph
            .nodes_v3()
            .filter_map(|n| match n.kind_v3() {
                Node::Finish {
                    target: Target::FullIds,
                    role,
                } => Some((*role, n.id_v3())),
                _ => None,
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        assert!(!finish.is_empty());
        assert_eq!(
            duty.allocation_dependencies_v1().len() as u64,
            graph.role_slots_v3()
        );
        for dependency in duty.allocation_dependencies_v1() {
            assert_eq!(
                finish.get(&dependency.role),
                Some(&dependency.logical_finish)
            );
            assert_eq!(
                dependency.absolute_finish,
                graph.absolute_packet_id_v3(dependency.logical_finish)
            );
        }
        assert_ne!(duty.logical_id_v1(), Hash64::default());
        assert_ne!(duty.absolute_id_v1(), Hash64::default());
        assert!(Duty::derive_native_v1(frame, &source, &graph, h(0xFA02)).is_err());
        let other = if fault {
            crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_v1(None, 2)
        } else {
            crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_lying_output_v1(
                2,
                h(0xFA01),
            )
        };
        let other_source = crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::new_native_v1(
            &other.record,
            &other.layout,
            other.frame.header_v1(),
            other.frame.original_v1().execution_root,
            other.frame.original_v1().trace_root,
            other.rules.maximum_leaves,
        )
        .unwrap();
        assert_ne!(other.frame.plan_v1().original_claim, claim);
        let other_graph =
            Graph::derive_native_v3(&other.frame, &other_source, profile, budget).unwrap();
        assert!(
            Duty::derive_native_v1(frame, &source, &other_graph, profile).is_err(),
            "another original cannot lend its dependency scope"
        );

        assert!(matches!(
            Graph::derive_native_v3(
                frame,
                &source,
                profile,
                Budget {
                    max_roles: graph.role_slots_v3() - 1,
                    ..budget
                }
            ),
            Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_))
        ));
        let rows = crate::palw_step_refute::tiled_logits_rows_root_v1(
            &fx.execution.binding.job_context,
            &fx.execution.rows,
        )
        .unwrap();
        let ids = source
            .retain_complete_ids_v1(rows, &fx.execution.generated)
            .unwrap();
        let store = Store::new(&b.state, claim, scope, &[], &[], Some(&ids)).unwrap();
        let rules = crate::palw_court_close_receipt_v1::NativeCourtRulesV1 {
            network_domain: extras.tir_public_network_domain_v1.unwrap(),
            at: at(frame, 1),
            court: fx.rules.court,
            step_ladder: fx.rules.maximum_leaves,
            prompt_ids_merkle: false,
            held_regime: false,
            decode_close_convicts_only: false,
            fp_decode_from_daa: None,
            cell_closure_from_daa: None,
            memory_routing_from_daa: None,
            tir_active: true,
            gen_active: false,
            task_heads_active: false,
            improvement_active: false,
            constraint_active: false,
        };
        let mut intent = Intent {
            version: 1,
            claim,
            scope_id: scope,
            policy_id: crate::palw_court_shared_tir_root_v1::shared_tir_root_policy_id_v1(&rules)
                .unwrap(),
            duty_id: duty.absolute_id_v1(),
            material_id: ids_material_id_v1(&ids).unwrap(),
            filer: outsider(),
            signature: vec![],
        };
        let sign = |i: &mut Intent| {
            i.signature = libcrux_ml_dsa::ml_dsa_87::sign(
                &key.signing_key,
                i.signing_message(rules.network_domain)
                    .unwrap()
                    .as_byte_slice(),
                CONTEXT,
                [112; 32],
            )
            .unwrap()
            .as_ref()
            .to_vec();
        };
        sign(&mut intent);
        let before = b.state.state_root();
        let evidence = store
            .build_output_terminal_v1(&b.state, &params, &rules, &intent, &duty)
            .unwrap();
        let result = evidence.recheck(&store, &b.state, &params, &rules).unwrap();
        if fault {
            let Decision::Fault {
                kind,
                claim: found,
                execution,
                computed,
            } = result
            else {
                panic!("actual false output commitment must fail")
            };
            assert_eq!(
                kind,
                crate::palw_tir_objective_v1::PalwTirOutputFaultV1::OutputRootMismatch
            );
            assert_eq!(found, claim);
            assert_eq!(execution, frame.original_v1().execution_root);
            assert_eq!(
                computed,
                crate::palw_attempt_rules_v1::palw_attempt_output_root_v1(
                    &source.header_v1().job_context,
                    &fx.execution.generated
                )
            );
            assert_ne!(computed, frame.original_v1().output_root);
        } else {
            assert_eq!(result, Decision::OutputHolds);
        }
        use crate::palw_court_shared_tir_builder_store_v1::output_terminal::{
            OriginalTerminalFamilyV1, REGULAR_STOP_CONTEXT_V1, RegularStopDecisionV1,
            RegularStopRefusalV1, SignedRegularStopIntentV1 as StopIntent,
            TerminalAllocationParentsV1,
        };
        let nodes = duty.family_nodes_v1().collect::<Vec<_>>();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].family, OriginalTerminalFamilyV1::OutputIdentity);
        assert_eq!(nodes[1].family, OriginalTerminalFamilyV1::NativeRegularStop);
        assert_eq!(
            nodes[0].parents,
            TerminalAllocationParentsV1::AllFullIdsFinishes(duty.allocation_dependencies_v1())
        );
        assert_eq!(
            nodes[1].parents,
            TerminalAllocationParentsV1::OutputIdentity {
                logical: duty.logical_id_v1(),
                absolute: duty.absolute_id_v1()
            }
        );
        let mut si = StopIntent {
            version: 1,
            claim,
            scope_id: scope,
            policy_id: intent.policy_id,
            duty_id: duty.native_regular_stop_id_v1().unwrap(),
            material_id: intent.material_id,
            decode_count: frame.original_v1().decode_tokens_executed,
            stop_reason: frame.original_v1().stop_reason,
            filer: outsider(),
            signature: vec![],
        };
        let stop_sign = |s: &mut StopIntent| {
            s.signature = libcrux_ml_dsa::ml_dsa_87::sign(
                &key.signing_key,
                s.signing_message(rules.network_domain)
                    .unwrap()
                    .as_byte_slice(),
                REGULAR_STOP_CONTEXT_V1,
                [114; 32],
            )
            .unwrap()
            .as_ref()
            .to_vec();
        };
        stop_sign(&mut si);
        if fault {
            assert!(matches!(
                store.build_regular_stop_terminal_v1(&b.state, &params, &rules, &si, &duty),
                Err(RegularStopRefusalV1::OutputPrerequisite(
                    crate::palw_tir_objective_v1::PalwTirOutputFaultV1::OutputRootMismatch
                ))
            ));
        } else {
            let e = store
                .build_regular_stop_terminal_v1(&b.state, &params, &rules, &si, &duty)
                .unwrap();
            let decision = e.recheck(&store, &b.state, &params, &rules).unwrap();
            if stop_fault {
                assert!(matches!(decision, RegularStopDecisionV1::Fault(_)));
            } else {
                assert_eq!(decision, RegularStopDecisionV1::StopRuleHolds);
            }
        }
        for which in 0..5 {
            let mut bad = si.clone();
            match which {
                0 => bad.decode_count += 1,
                1 => bad.scope_id = h(0xFA11),
                2 => bad.duty_id = h(0xFA12),
                3 => bad.material_id = h(0xFA13),
                _ => {
                    bad.stop_reason = match si.stop_reason {
                        crate::palw_freeprompt_v3::PalwFpStopReasonV3::ExactBudgetReached => {
                            crate::palw_freeprompt_v3::PalwFpStopReasonV3::EndOfGeneration
                        }
                        _ => crate::palw_freeprompt_v3::PalwFpStopReasonV3::ExactBudgetReached,
                    }
                }
            }
            stop_sign(&mut bad);
            assert!(
                store
                    .build_regular_stop_terminal_v1(&b.state, &params, &rules, &bad, &duty)
                    .is_err()
            );
        }
        let mut corrupt_stop = si.clone();
        corrupt_stop.signature[0] ^= 1;
        assert!(
            store
                .build_regular_stop_terminal_v1(&b.state, &params, &rules, &corrupt_stop, &duty)
                .is_err()
        );
        let no_stop_ids = Store::new(&b.state, claim, scope, &[], &[], None).unwrap();
        assert!(
            no_stop_ids
                .build_regular_stop_terminal_v1(&b.state, &params, &rules, &si, &duty)
                .is_err()
        );
        let mut late_stop = rules.clone();
        late_stop.at.daa_score = p.terms.accepted_daa + 10_000;
        assert!(
            store
                .build_regular_stop_terminal_v1(&b.state, &params, &late_stop, &si, &duty)
                .is_err()
        );
        let missing = Store::new(&b.state, claim, scope, &[], &[], None).unwrap();
        assert!(
            missing
                .build_output_terminal_v1(&b.state, &params, &rules, &intent, &duty)
                .is_err(),
            "potential finishes cannot replace actual FullIds"
        );
        for which in 0..3 {
            let mut wrong = intent.clone();
            match which {
                0 => wrong.duty_id = h(0xFA03),
                1 => wrong.scope_id = h(0xFA04),
                _ => wrong.material_id = h(0xFA05),
            }
            sign(&mut wrong);
            assert!(
                store
                    .build_output_terminal_v1(&b.state, &params, &rules, &wrong, &duty)
                    .is_err()
            );
        }
        let mut wrong = intent.clone();
        wrong.signature[0] ^= 1;
        assert!(
            store
                .build_output_terminal_v1(&b.state, &params, &rules, &wrong, &duty)
                .is_err()
        );
        let mut late = rules.clone();
        late.at.daa_score = p.terms.accepted_daa + 10_000;
        assert!(
            store
                .build_output_terminal_v1(&b.state, &params, &late, &intent, &duty)
                .is_err()
        );
        let mut replaced = b.state.clone();
        replaced.bonds.get_mut(&outsider()).unwrap().pubkey =
            libcrux_ml_dsa::ml_dsa_87::generate_key_pair([113; 32])
                .verification_key
                .as_ref()
                .to_vec();
        assert!(
            evidence
                .recheck(&store, &replaced, &params, &rules)
                .is_err()
        );
        assert!(
            store
                .build_regular_stop_terminal_v1(&replaced, &params, &rules, &si, &duty)
                .is_err()
        );
        assert_eq!(b.state.state_root(), before);
    }
}
