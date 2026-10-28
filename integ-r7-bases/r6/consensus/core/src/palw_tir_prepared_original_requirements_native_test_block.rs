        {
            let mut paired_extras=extras.clone();paired_extras.tir_public_originals.insert(claim,envelope.clone());
            let paired=TransitionBuilder::try_new(&state,&params,false,false,false,false,&paired_extras).unwrap();
            let before=paired.state.state_root();let entries=paired.entries.len();
            let make=||prepare_paired_original_v1(&paired,&ctx,&object,Some(&fp.commitment)).unwrap().unwrap();
            let budgets=PreparedRequirementBudgetsV1{
                numerical:crate::palw_court_numerical_geometry_v3::NumericalGeometryBudgetV3{max_original_leaves:1_000_000,max_roots:1_000_000,max_paths:10_000_000,max_edges:10_000_000,max_path_bytes:10_000_000},
                input:crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalParameterCoverBudgetV1{max_original_leaves:1_000_000,max_total_edges:10_000_000,max_total_coordinates:1_000_000,max_root_memberships:1_000_000},
                source:crate::palw_court_source_material_graph_v3::SourceGraphBudgetV3{max_queries:100_000,max_roles:8,max_vertices:2_000_000,max_edges:8_000_000},
            };
            let prepared=make().enrich_native_geometry_v1(&paired,&ctx).unwrap();
            let requirements=prepared.prepare_requirements_v1(&paired,&ctx,budgets).unwrap();
            assert_eq!(requirements.frame_v1().original_v1(),&fp.commitment);
            assert_eq!(requirements.shape_v1().grammar_v1(),crate::palw_court_shared_tir_builder_store_v1::original_aggregate::OriginalGrammarV1::Ordinary);
            assert_eq!(requirements.numerical_v1().source_id_v3(),plan.source_id);
            assert_eq!(requirements.inputs_v1().parameters_v1().source_id,plan.source_id);
            assert_eq!(requirements.numerical_v1().roots_v3().len(),requirements.inputs_v1().roots_v1().len());
            assert!(matches!(requirements.source_graph_v1(),Err(PreparedRequirementsErrorV1::MissingSharedServiceNamespace)));
            assert!(matches!(requirements.numerical_material_join_v1(&paired,&ctx,100000),Err(PreparedRequirementsErrorV1::MissingSharedServiceNamespace)));
            // Requirements succeeded and retain real native owners even though
            // absent service namespace explicitly blocks their provider join.
            // Real signed native original, before Claim/FP reservation. Only
            // its existing source producer responsibility is a named actor.
            let actors=requirements.actor_catalog_v1(&paired,&ctx).unwrap();
            let actor=actors.source_actor_v1();
            assert_eq!(actor.bond,producer);assert_eq!(actor.operator,palw_operator_id_v2(&key));
            assert_eq!(actor.key_id,crate::mldsa87_primitives::mldsa87_key_id(&key));
            assert_eq!(actor.pledge_id,requirements.frame_v1().custody_v1().pledge_v1().id_v1());
            assert_eq!(actors.potential_source_slots_v1(),plan.acquisition.maximum_custodians);
            let missing=actors.missing_contracts_v1().collect::<Vec<_>>();
            assert!(missing.contains(&PreparedRoleContractMissingV1::CanonicalWholeBranchActorAssignment));
            assert!(missing.contains(&PreparedRoleContractMissingV1::SharedServiceOperatorConsent));
            assert!(missing.contains(&PreparedRoleContractMissingV1::FeeCapitalRetentionRecoveryBodies));
            assert_eq!(missing.contains(&PreparedRoleContractMissingV1::SemanticMaterialProviderConsent),plan.semantic.is_some());
            // Actual state rotation/status/operator changes after preparation
            // cannot reuse the old authenticated source responsibility.
            for mode in 0..3 {
                let mut changed=paired.state.clone();let bond=changed.bonds.get_mut(&producer).unwrap();
                match mode {0=>bond.pubkey[0]^=1,1=>bond.operator_id=h(0xAC701),
                    _=>bond.status=PalwBondStatusV2::Retiring{since_daa:ctx.daa_score,settled_at_since:0}}
                let changed=TransitionBuilder::try_new(&changed,&params,false,false,false,false,&paired_extras).unwrap();
                assert!(matches!(requirements.actor_catalog_v1(&changed,&ctx),Err(PreparedGeometryErrorV1::State(_))));
            }
            let mut short=budgets;short.input.max_original_leaves=0;
            assert!(matches!(make().enrich_native_geometry_v1(&paired,&ctx).unwrap().prepare_requirements_v1(&paired,&ctx,short),Err(PreparedRequirementsErrorV1::Native(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_)))));
            // An actual snapshot captured by preparation cannot be silently
            // replaced between native prepare and optional requirement derive.
            let origin=make();let mut changed=paired.state.clone();
            let replacement=crate::palw_class_semantics_v1::PalwClassSemanticsV1::of_registration(class,requirements.frame_v1().header_v1().artifact_root,&requirements.frame_v1().header_v1().job_context,0);
            if changed.class_semantics_v1(&class)==Some(&replacement){changed.class_semantics.get_mut(&class).unwrap().bound_from_daa=1;}else{changed.class_semantics.insert(class,replacement);}
            let changed=TransitionBuilder::try_new(&changed,&params,false,false,false,false,&paired_extras).unwrap();
            assert!(matches!(origin.enrich_native_geometry_v1(&changed,&ctx),Err(PreparedGeometryErrorV1::State(_))));
            assert_eq!(paired.state.state_root(),before);assert_eq!(paired.entries.len(),entries);
            assert!(paired.state.claim(&claim).is_none());assert!(palw_tir_public_claim_v1(&paired.state,claim).is_none());
        }
