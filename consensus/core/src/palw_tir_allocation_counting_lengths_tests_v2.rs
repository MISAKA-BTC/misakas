// Exact serializer-length compatibility on existing native allocation fixtures.
#[test]
fn allocation_counting_lengths_match_real_native_plain_flat_and_constraint_plans() {
    fn same<T: borsh::BorshSerialize>(name: &str, value: &T) -> usize {
        let encoded = borsh::to_vec(value).unwrap();
        let counted = borsh::object_length(value).unwrap();
        assert_eq!(counted, encoded.len(), "same actual serializer length: {name}");
        counted
    }
    for (flat, constrained) in [(false, false), (true, false), (false, true), (true, true)] {
        let f = fixture(flat, constrained);
        let plan = complete(&f, constrained);
        let components = (
            plan.semantic.as_ref(),
            &plan.acquisition,
            plan.flat_trace,
            &plan.preparation_work,
            &plan.owner_work,
            &plan.preparation,
            &plan.executor,
            &plan.terminal,
        );
        assert!(same("all actual plan components", &components) > 0);
        assert!(same("actual original job/claim", &f.original) > 0);
        assert!(same("actual original header", &f.header) > 0);
        assert!(same("actual native config", &f.rules.config) > 0);
        assert!(same("actual owner work rules", &plan.owner_work.rules) > 0);
        let (controls, validation, footprint) = original_controls(
            &f.original,
            &f.header,
            plan.semantic.as_ref(),
            &plan.acquisition,
            plan.flat_trace,
            &plan.preparation_work,
            &plan.owner_work,
            &plan.preparation,
            &plan.executor,
            &plan.terminal,
            f.rules,
        )
        .unwrap();
        assert_eq!(controls, plan.controls);
        assert_eq!(validation, plan.owner_validation);
        assert_eq!(footprint, plan.control_footprint);
        assert_eq!(plan.resource_id_v1().unwrap(), plan.resource_plan_id);
        assert_eq!(plan.accepted_id_v1().unwrap(), plan.allocation_id);
    }
}
