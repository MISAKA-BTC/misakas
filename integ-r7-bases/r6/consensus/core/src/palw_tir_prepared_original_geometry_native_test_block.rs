        // Real native transaction-paired preparation BEFORE any FP reservation
        // or rooted Claim/TirClaim installation. No decoder can make this type.
        {
            let mut paired_extras=extras.clone();paired_extras.tir_public_originals.insert(claim,envelope.clone());
            let paired=TransitionBuilder::try_new(&state,&params,false,false,false,false,&paired_extras).unwrap();
            let before=paired.state.state_root();let entries=paired.entries.len();
            let make=||prepare_paired_original_v1(&paired,&ctx,&object,Some(&fp.commitment)).unwrap().unwrap();
            let prepared=make();
            assert_eq!(prepared.frame_v1().original_v1(),&fp.commitment);
            assert_eq!(prepared.frame_v1().plan_v1(),&plan);
            assert!(paired.state.claim(&claim).is_none());assert!(palw_tir_public_claim_v1(&paired.state,claim).is_none());
            let enriched=prepared.enrich_native_geometry_v1(&paired,&ctx).unwrap();
            assert_eq!(enriched.source_v1().source_id_v1(),plan.source_id);
            let registered=paired.state.class(&class).unwrap().registered_daa;
            assert_eq!(enriched.demand_v1(),crate::palw_tir_fence2_v1::palw_tir_demand_rules_at_v1(&params,registered));
            let budget=crate::palw_court_numerical_geometry_v3::NumericalGeometryBudgetV3{max_original_leaves:1_000_000,max_roots:1_000_000,max_paths:10_000_000,max_edges:10_000_000,max_path_bytes:10_000_000};
            let numerical=enriched.numerical_v3(&paired,&ctx,budget).unwrap();
            let analytical=enriched.source_v1().numerical_tree_upper_v1(&fx.record,&fx.layout,&paired_extras.tir_public_allocation_rules_v1.unwrap().court,enriched.demand_v1()).unwrap();
            assert_eq!(numerical.roots_v3().len() as u64,analytical.eligible_roots);
            let limits=crate::palw_court_source_material_graph_v3::SourceGraphBudgetV3{max_queries:100_000,max_roles:8,max_vertices:2_000_000,max_edges:8_000_000};
            assert!(matches!(enriched.source_graph_v3(&paired,&ctx,limits),Err(PreparedGeometryErrorV1::MissingSharedServiceNamespace)));
            let mut other=ctx;other.blue_score+=1;
            assert!(matches!(make().enrich_native_geometry_v1(&paired,&other),Err(PreparedGeometryErrorV1::State(_))));
            assert!(matches!(enriched.numerical_v3(&paired,&ctx,crate::palw_court_numerical_geometry_v3::NumericalGeometryBudgetV3{max_original_leaves:0,..budget}),Err(PreparedGeometryErrorV1::Native(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_)))));
            assert_eq!(paired.state.state_root(),before);assert_eq!(paired.entries.len(),entries);
            assert!(paired.state.claim(&claim).is_none());assert!(palw_tir_public_claim_v1(&paired.state,claim).is_none());
        }
