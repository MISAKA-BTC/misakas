// Included after genuine ordinary/Prefix TFO2 acceptance. Settlement uses the
// production schedule stored by that acceptance, then crosses the block fold.
{
    use crate::palw_funded_escrow_v1 as escrow;
    let admitted = *accepted.funded_escrows_v1().values().find(|row| row.claim == claim).unwrap();
    assert_eq!(admitted.funding_outpoint, actual.outpoint());
    assert_eq!(admitted.hard_deadline_daa, public_quote.hard_until_v2(), "admission and pre-signing quote share the original native hard");
    assert!(admitted.fee_sompi > 0 && admitted.amount_sompi > admitted.fee_sompi);
    let [work, relay, burn] = crate::palw_g14_values_v1::palw_g14_escrow_split_v1(admitted.fee_sompi);
    let remaining = admitted.amount_sompi.checked_sub(admitted.fee_sompi).unwrap();
    let output = |amount, payee: Hash64| TransactionOutput::new(amount, crate::mldsa87_primitives::p2pkh_mldsa87_spk(&payee.as_bytes()));
    let mut outputs = Vec::new();
    if work > 0 { outputs.push(output(work, admitted.seat_payee)); }
    if relay > 0 { outputs.push(output(relay, admitted.relay_payee)); }
    outputs.push(output(remaining, admitted.refund_payload));
    let spend = Transaction::new(0, vec![TransactionInput::new(admitted.funding_outpoint, vec![], 0, 0)],
        outputs, 0, crate::subnets::SUBNETWORK_ID_NATIVE, 0, vec![]);
    let plan = escrow::palw_escrow_qualified_spend_v1(&admitted, &spend, &[admitted.amount_sompi], ctx.daa_score + 2,
        escrow::palw_escrow_fee_schedule_v1(&admitted).as_ref()).unwrap();
    assert_eq!((plan.paid_now, plan.burned_now, plan.continued_now), (work + relay, burn, remaining));
    assert_eq!(spend.outputs.iter().map(|o| o.value).sum::<u64>() + burn, admitted.amount_sompi);
    let fact = escrow::PalwEscrowSettlementFactV1 { contract_id: admitted.contract_id, old: plan.old, new: plan.new };
    let mut settlement_extras = active.clone();
    settlement_extras.funded_escrow_settlements = vec![fact];
    let settlement_point = PalwBlockContextV2 { block: h(0x42e5c), daa_score: ctx.daa_score + 2, blue_score: ctx.blue_score + 2, subsidy: 0 };
    let (settled, delta) = apply_palw_transition_v2_with_extras(&accepted, &family_params, &settlement_point,
        &[], None, false, false, false, false, &settlement_extras).unwrap();
    assert_eq!(settled.funded_escrows_v1()[&admitted.contract_id], plan.new);
    let encoded = borsh::to_vec(&PalwStateCarriageV2::from_state(&settled)).unwrap();
    let restored = borsh::from_slice::<PalwStateCarriageV2>(&encoded).unwrap().into_state(&family_params, Some(settled.state_root())).unwrap();
    assert_eq!(restored.funded_escrows_v1()[&admitted.contract_id], plan.new);
    let duplicate_point = PalwBlockContextV2 { block: h(0x42e5d), daa_score: ctx.daa_score + 3, blue_score: ctx.blue_score + 3, subsidy: 0 };
    let duplicate = apply_palw_transition_v2_with_extras(&restored, &family_params, &duplicate_point,
        &[], None, false, false, false, false, &settlement_extras).unwrap_err();
    assert!(duplicate.to_string().contains("settlement fact whose row is not the rooted row"),
        "a later block must refuse the already consumed actual spend for its once-only row, not a duplicate point: {duplicate}");
    let mut undone = settled.clone();
    for entry in delta.entries.iter().rev() { apply_delta_entry(&mut undone, entry, true).unwrap(); }
    undone.set_last_point_for_tests(accepted.last_point().cloned());
    assert_eq!(undone.state_root(), accepted.state_root(), "escrow continuation and its lock revert with the original state");
    let mut corrupt = PalwStateCarriageV2::from_state(&settled);
    let row = corrupt.funded_escrows.get_mut(&admitted.contract_id).unwrap();
    row.paid_sompi -= 1;
    row.burned_sompi += 1;
    assert!(borsh::from_slice::<PalwStateCarriageV2>(&borsh::to_vec(&corrupt).unwrap()).is_err(),
        "carriage cannot relabel a real payee transfer as burn while keeping the same total");

    let mut no_payees = family_state.clone();
    for bond in no_payees.bonds.values_mut() { bond.payout_payload = Hash64::default(); }
    let read = TransitionBuilder::try_new(&no_payees, &family_params, false, false, false, false, &active).unwrap();
    let prepared = prepare_paired_original_v1(&read, &ctx, &object, Some(&fp.commitment)).unwrap().unwrap();
    assert!(matches!(coordinate_funded_original_v1(&read, &ctx, claim, prepared),
        Err(FundedOriginalRefusalV1::Invalid("funded original lacks native settlement payees"))),
        "paid admission cannot silently install an unresolvable payout schedule");
}
