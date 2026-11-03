{
    let born = accepted.claim(&claim).unwrap();
    assert!(matches!(born.phase, PalwClaimPhaseV2::Provisional), "this branch must use the actual unbound original");
    assert!(accepted.court_service_holds_claim_v1(&claim));
    let initial_frame = palw_tir_public_claim_v1(&accepted, claim).unwrap().clone();
    let bind = accepted.deadlines.iter().find_map(|(at, id)| (*id == claim).then_some(*at))
        .expect("actual unbound original has its native bind deadline");
    let tick = |parent: &PalwChainStateV2, daa: u64, nonce: u64| {
        let old = parent.last_point().unwrap();
        let point = PalwBlockContextV2 { block: Hash64::from_u64_word(nonce), daa_score: daa,
            blue_score: old.blue_score.checked_add(1).unwrap(), subsidy: ctx.subsidy };
        assert!(point.daa_score >= old.daa_score && point.block != old.block);
        apply_palw_transition_v2_with_extras(parent, &family_params, &point, &[], None,
            false, false, false, false, &active).unwrap()
    };
    let (voided, void_delta) = tick(&accepted, bind.checked_add(1).unwrap(), 0xF043_0001);
    let terminal = voided.claim(&claim).unwrap();
    assert!(matches!(terminal.phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::BindTimeout, .. }));
    assert!(voided.court_service_holds_claim_v1(&claim));
    let release = super::super::terminal_operational_release_at_v1(terminal, &family_params, None).unwrap()
        .expect("actual abandon hold has an operational release");
    let held = super::super::palw_claim_held_commitment_v1(&family_params, terminal).unwrap();
    assert!(held > 0);
    let before_exposure = voided.reserved_exposure(&producer);
    let expected = before_exposure.checked_sub(held).unwrap();
    let hard = voided.court_service_state_v1().unwrap().claims[&claim].hard_deadline_daa;
    assert!(release.checked_add(2).unwrap() < hard, "unchanged native book must still hold retirement after release");
    let cold = |s: &PalwChainStateV2| {
        let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(s)).unwrap();
        let restored = borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap()
            .into_state(&family_params, Some(s.state_root())).unwrap();
        assert_eq!(restored.state_root(), s.state_root());
        assert_eq!(restored.reserved_exposure(&producer), s.reserved_exposure(&producer));
        assert!(restored.court_service_holds_claim_v1(&claim));
        restored
    };
    assert_eq!(super::super::revert_delta_v2(&voided, &void_delta, &family_params).unwrap().state_root(), accepted.state_root());
    let voided = cold(&voided);
    let (boundary, _) = tick(&voided, release, 0xF043_0002);
    assert_eq!(boundary.reserved_exposure(&producer), before_exposure);
    let boundary = cold(&boundary);
    let (released, release_delta) = tick(&boundary, release + 1, 0xF043_0003);
    assert_eq!(released.reserved_exposure(&producer), expected, "only the exact operational commitment releases");
    assert!(released.claim(&claim).is_some());
    assert!(released.court_service_holds_claim_v1(&claim));
    assert!(released.staged_bond_exit_held_v1(&producer, release + 1));
    assert_eq!(palw_tir_public_claim_v1(&released, claim).unwrap(), &initial_frame);
    for (id, _) in voided.funded_escrows_v1().iter().filter(|(_, row)| row.claim == claim && row.liability_open) {
        let actual = released.funded_escrows_v1().get(id).unwrap();
        assert!(actual.liability_open && actual.settled_by.is_none(), "operational release does not refund its own unresolved escrow");
    }
    let released = cold(&released);
    let undone = super::super::revert_delta_v2(&released, &release_delta, &family_params).unwrap();
    assert_eq!(undone.state_root(), boundary.state_root());
    let replayed = super::super::apply_delta_v2(&undone, &release_delta, &family_params).unwrap();
    assert_eq!(replayed.state_root(), released.state_root());
    let (again, _) = tick(&released, release + 2, 0xF043_0004);
    assert_eq!(again.reserved_exposure(&producer), expected, "subsequent blocks do not release the commitment twice");
    assert!(again.claim(&claim).is_some() && again.court_service_holds_claim_v1(&claim));
}
