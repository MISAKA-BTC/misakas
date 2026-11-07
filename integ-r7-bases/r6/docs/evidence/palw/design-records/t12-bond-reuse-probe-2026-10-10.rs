#[path = "rcore_common.rs"]
mod common;
use common::*;
use kaspa_consensus_core::config::params::palw_t12_shipped_params;
use kaspa_consensus_core::palw_issuance_slots_v1::PalwIssuanceReadV1;

#[test]
fn current_shipped_parameters_and_reuse_predicates() {
    let p = palw_t12_shipped_params();
    p.validate_palw_v2().unwrap();
    let sp = &bundle(&p).state;
    let state = genesis_state(&p);
    let classes: Vec<_> = genesis_classes(&p).into_iter().map(|(id, leaves, _, _)| {
        serde_json::json!({"id": id.to_string(), "leaves": leaves,
            "n_ctx": state.fp_work_profile_of(&id).map(|v| v.n_ctx),
            "c7": p.palw_rcore_conservative_classes.contains(&id)})
    }).collect();
    let stages: Vec<_> = [0u64,750,1300,1700,2000,3600,10_000].into_iter().map(|daa| {
        let step = sp.capacity_step_at(daa);
        let rho = step.map_or(1, |v| v.rho);
        let r = PalwIssuanceReadV1::of_v1(13_000 * 100_000_000, rho, 0, None, daa);
        serde_json::json!({"daa":daa,"rho":rho,"q_credit_permille":step.map(|v|v.q_credit_permille),
            "issuance_active":sp.capacity_slots_applies_at(daa),"outstanding_cap":r.cap,
            "rate_milli_per_daa":r.rate_milli,"burst_milli":r.burst_milli})
    }).collect();
    let result = serde_json::json!({"scope":"pre compiled shipped t12, not live-node measurement",
        "params_id":p.consensus_params_id().to_string(),"minimum_bond_sompi":sp.min_collateral_sompi(),
        "exposure_permille":sp.fp_max_exposure_ratio_permille(),"window_receipt_daa":sp.window_receipt(),
        "window_challenge_daa":sp.window_challenge(),"anchor_delay_daa":sp.capacity_network_anchor_delay(),
        "rho_schedule":p.palw_capacity_aggregate_liability.as_ref().map(|v|v.steps.iter().map(|s|serde_json::json!({"from_daa":s.from_daa,"rho":s.rho,"q_credit_permille":s.q_credit_permille})).collect::<Vec<_>>()),
        "capacity_slots_fence":format!("{:?}",p.palw_capacity_issuance_slots),
        "capacity_escrow_fence":format!("{:?}",p.palw_capacity_escrow_at_licence),
        "stages":stages,"classes":classes});
    println!("AUDIT_JSON:{}",result);
}

#[test]
fn earlier_counted_licence_releases_escrow_and_slot_earlier() {
    use kaspa_consensus_core::palw_issuance_slots_v1::palw_issuance_holds_slot_v1;
    use kaspa_consensus_core::palw_state_v2::palw_rcore_release_due_v1;
    // Build a known panel record using the historical fold fixture. This tests the CURRENT
    // decision predicates, not full acceptance of a fabricated signed model execution.
    let mut c = Chain::new(t12());
    let id = c.floor_claim(0x49C0);
    let seats = c.floor_seats();
    c.bind(id, &seats);
    let p = palw_t12_shipped_params();
    let sp = &bundle(&p).state;
    let mut waiting = c.claim(&id);
    waiting.class_id = genesis_classes(&p)[1].0; // the non-C7 A16 8k class
    waiting.accepted_daa = 10_000;
    waiting.phase = PalwClaimPhaseV2::PanelBound { bound_daa: 10_020 };
    let mut fast = waiting.clone();
    fast.phase = PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 10_021 };
    fast.rcore.licence_door = Some(PalwLicenceDoorTagV1::Quorum);
    fast.rcore.basis_k = 2;
    fast.rcore.served_mask = (1u32 << seats.len()) - 1;
    assert!(palw_rcore_release_due_v1(sp, &c.s, &id, &fast));
    fast.rcore.escrow_released = true;
    let held_waiting = palw_claim_commitment_v1(sp, &waiting, 10_021).unwrap();
    let held_fast = palw_claim_commitment_v1(sp, &fast, 10_021).unwrap();
    assert!(held_fast < held_waiting);
    assert!(palw_issuance_holds_slot_v1(sp, &waiting, 10_021));
    assert!(!palw_issuance_holds_slot_v1(sp, &fast, 10_021));
    let delta = held_waiting - held_fast;
    println!("REUSE_PREDICATE_JSON:{}",serde_json::json!({"accepted_daa":10000,"same_observation_daa":10021,"fast_license_daa":10021,"waiting_bound_daa":10020,"current_a16_8k_class":fast.class_id.to_string(),"escrow_released_earlier_sompi":delta,"fast_holds_slot":false,"waiting_holds_slot":true,"scope":"current pure decision predicates over a synthetic record; not an end-to-end forgery"}));
}

#[test]
fn quoted_reservation_for_13k_by_class_and_height() {
    use kaspa_consensus_core::palw_producer_v2::palw_producer_facts_v4;
    use kaspa_consensus_core::palw_state_v2::{palw_claim_escrow_v1,palw_work_floor_for_block_v1};
    use kaspa_consensus_core::palw_admission_v2::palw_work_lottery_floor_v1;
    use kaspa_consensus_core::palw_reward_v2::PalwRewardParamsV2;
    let p=palw_t12_shipped_params();let b=bundle(&p);
    let mut fixture=Chain::new(t12());fixture.step(&[bond_obj(90,13_000*100_000_000)]);
    let state=&fixture.s;let bond=bond_key(90);
    let carve=PalwRewardParamsV2::new(p.palw_overlay_carve.unwrap().worker_carve_permille).unwrap();
    let rate=p.palw_economic_payout.unwrap().rate_sompi_per_giga;
    let escrow=palw_claim_escrow_v1(&b.state,T12_BLOCK_SUBSIDY_SOMPI,Some(carve));
    let w0=palw_work_floor_for_block_v1(&b.state,T12_BLOCK_SUBSIDY_SOMPI,Some(carve),rate);
    let mut rows=Vec::new();
    for daa in [2u64,1700,10000] {
        let registry=registry_fold(&p,daa).unwrap();
        for (id,_,_,_) in genesis_classes(&p) {
            let draw=registry.genesis_works.get(&id).map(|w|w.economic_ccu_per_claim);
            let facts=palw_producer_facts_v4(state,&b.state,&b.admission,kaspa_consensus_core::BlockHash::default(),daa,id,Some(&bond),palw_work_lottery_floor_v1(state,Some(w0),true),p.palw_canonical_work_daa(),draw,true,escrow,None).unwrap();
            let f=facts.bond.unwrap();rows.push(serde_json::json!({"daa":daa,"class":id.to_string(),"reservation_sompi":f.claim_exposure,"ceiling_sompi":f.exposure_ceiling,"escrow_sompi":escrow,"scope":"producer quote on empty synthetic 13k bond, not full execution or saturated bond"}));
        }
    }
    println!("RESERVATION_JSON:{}",serde_json::json!(rows));
}
