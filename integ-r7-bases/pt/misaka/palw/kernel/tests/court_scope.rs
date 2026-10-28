//! **The court scope on F-C4R4-17's own class** (`dense_moe_v1(7)`, the C4R4 PoC's ledger world): the predicate leaves no clear
//! model-dependent value for the ledger to serve, so no union of demands rebuilds a weight; model-independent values stay owed.
//! (G14R's ledger hook applies `court_withheld_mask_v1`; this pins what it applies.)

use misaka_palw_kernel::scope::*;
use misaka_palw_tir::program::Ref;
use misaka_palw_tir_sketch::fixture::dense_moe_v1;

mod common;

use common::chain::T;
use common::ledger_world::{Da, OUTSIDER, PRODUCER, World, convicted, position};
use misaka_palw_kernel::ledger::{KernelLedgerV1, LedgerEventV1, OutsiderFindingV1, OutsiderV1};
use misaka_palw_kernel::public::{MaterialResponseV1, PositionResponseV1};

#[test]
fn combined_scope_keeps_requester_quotas_and_public_conviction_without_clear_model_values() {
    use common::ledger_world::{SPAM1, refused};
    use misaka_palw_kernel::court_scope::CourtScopePolicyV1;
    let mut w = World::new_opv();
    for ledger in [&mut w.genesis, &mut w.l] {
        ledger.commitment_scope_from_daa = Some(0);
        ledger.court_scope_policy = Some(CourtScopePolicyV1 { activation_daa: 0 });
    }
    let prompt: Vec<u32> = (0..20).map(|i| (i * 7 + 3) % 32).collect();
    let job = w.post_job(2, &prompt, 1, 17);
    let (_, lie) = w.lying(&job, 1);
    let id = lie.claim.id();
    let positions = lie.trace.values.len() as u32;
    assert!(positions > COURT_SCOPE_MAX_UNITS_PER_REQUESTER_V1 as u32);
    let events = w.block(10, vec![lie.tx, T::PanelCovered { claim: id }]);
    assert!(events.iter().any(|e| matches!(e, LedgerEventV1::ClaimCommitted { claim } if *claim == id)), "{events:?}");
    let events = w.block(
        11,
        (0..=COURT_SCOPE_MAX_UNITS_PER_REQUESTER_V1 as u32)
            .map(|position| T::FileDemand { demander: SPAM1, claim: id, stage: 0, position })
            .collect(),
    );
    assert_eq!(
        events.iter().filter(|e| matches!(e, LedgerEventV1::DemandOpened { .. })).count(),
        COURT_SCOPE_MAX_UNITS_PER_REQUESTER_V1
    );
    assert!(refused(&events).unwrap().contains("COURT_SCOPE_UNITS"), "{events:?}");
    let events = w.block(
        12,
        (COURT_SCOPE_MAX_UNITS_PER_REQUESTER_V1 as u32..positions)
            .map(|position| T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position })
            .collect(),
    );
    assert!(refused(&events).is_none(), "another requester retains its independent allowance: {events:?}");
    let mut responses = Vec::new();
    for position in 0..positions {
        let bytes = scoped_response(&w, &id, &lie.trace, position);
        assert!(w.l.classify_served_position_v1(&id, 0, position, &bytes).is_ok());
        responses.push(T::Respond { claim: id, stage: 0, position, bytes });
    }
    let events = w.block(13, responses);
    assert_eq!(events.iter().filter(|e| matches!(e, LedgerEventV1::Served { .. })).count(), positions as usize);
    drop(lie.trace);
    let empty = Da(Default::default());
    let OutsiderFindingV1::Prosecute(proof) =
        (OutsiderV1 { ledger: &w.l, claim: id, material: &empty, artifact: &w.params, salt: [0x5A; 64] }).check().unwrap()
    else {
        panic!("the outsider must convict from authenticated public rows")
    };
    let events = w.block(14, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert!(convicted(&events).is_some(), "{events:?}");
    assert_eq!(KernelLedgerV1::replay(&w.genesis, &w.blocks).root(), w.l.root());
}

fn scoped_response(w: &World, claim: &[u8; 64], trace: &misaka_palw_kernel::trace::TraceV1, p: u32) -> Vec<u8> {
    let mask = w.l.derived_mask(&w.l.claims[claim], 0);
    position(trace, p, |values| {
        for (occ, omitted) in values.iter_mut().zip(&mask) {
            for (value, hidden) in occ.iter_mut().zip(omitted) {
                if *hidden {
                    *value = MaterialResponseV1::Omitted;
                }
            }
        }
    })
}

#[test]
fn scoped_ledger_serves_every_position_without_model_values_and_replay_convicts_the_hidden_lie() {
    let mut w = World::new();
    w.l.commitment_scope_from_daa = Some(0);
    let job = w.post_job(2, &[3, 17, 9], 2, 1);
    let (_, lie) = w.lying(&job, 2);
    let id = lie.claim.id();
    let positions = lie.trace.values.len() as u32;
    w.block(10, vec![lie.tx, T::PanelCovered { claim: id }]);
    let mask = w.l.derived_mask(&w.l.claims[&id], 0);
    assert!(mask.iter().flatten().any(|hidden| *hidden));
    let producer_collateral = w.l.bonds[&PRODUCER].collateral;
    let demands = (0..positions).map(|p| T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: p }).collect();
    let events = w.block(11, demands);
    assert_eq!(events.iter().filter(|event| matches!(event, LedgerEventV1::DemandOpened { .. })).count(), positions as usize);
    let mut responses = Vec::new();
    for p in 0..positions {
        let bytes = scoped_response(&w, &id, &lie.trace, p);
        assert!(w.l.classify_served_position_v1(&id, 0, p, &bytes).is_ok());
        let mut leaking: PositionResponseV1 = borsh::from_slice(&bytes).unwrap();
        let (s, n) = mask.iter().enumerate().find_map(|(s, nodes)| nodes.iter().position(|v| *v).map(|n| (s, n))).unwrap();
        leaking.values[s][n] =
            MaterialResponseV1::Whole(misaka_palw_kernel::public::TensorWireV1::of(&lie.trace.values[p as usize][s][n]));
        assert!(
            w.l.classify_served_position_v1(&id, 0, p, &borsh::to_vec(&leaking).unwrap()).is_err(),
            "scope is enforced, not advisory"
        );
        responses.push(T::Respond { claim: id, stage: 0, position: p, bytes });
    }
    let events = w.block(12, responses);
    assert_eq!(events.iter().filter(|event| matches!(event, LedgerEventV1::Served { .. })).count(), positions as usize);
    // Drop the only producer trace before constructing the outsider from authenticated rows.
    drop(lie.trace);
    let rows = w.l.to_rows();
    let mut restored = KernelLedgerV1::from_rows(&w.genesis, w.l.scalars(), &rows).unwrap();
    restored.commitment_scope_from_daa = Some(0);
    let empty = Da(Default::default());
    let finding =
        OutsiderV1 { ledger: &restored, claim: id, material: &empty, artifact: &w.params, salt: [0x5A; 64] }.check().unwrap();
    let OutsiderFindingV1::Prosecute(proof) = finding else { panic!("the hidden lie must be objectively refuted") };
    assert_eq!(restored.root(), w.l.root());
    let events = w.block(13, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert!(convicted(&events).is_some(), "{events:?}");
    assert!(w.l.bonds[&PRODUCER].collateral < producer_collateral);
}

#[test]
fn commitment_scope_preserves_old_claim_contracts_and_clears_consumer_policy_on_cache_rebuild() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 1, 1);
    let honest = w.honest(&job, 1);
    let id = honest.claim.id();
    w.block(10, vec![honest.tx, T::PanelCovered { claim: id }]);
    let historical = w.l.derived_mask(&w.l.claims[&id], 0);
    w.l.commitment_scope_from_daa = Some(11);
    assert_eq!(w.l.derived_mask(&w.l.claims[&id], 0), historical);
    w.l.commitment_scope_from_daa = Some(10);
    assert_ne!(w.l.derived_mask(&w.l.claims[&id], 0), historical);
    let rows = w.l.to_rows();
    let reconstructed = KernelLedgerV1::from_rows(&w.genesis, w.l.scalars(), &rows).unwrap();
    let cached = w.l.clone().as_rebuilt_from_v1(&rows);
    assert_eq!(reconstructed.commitment_scope_from_daa, None);
    assert_eq!(cached.commitment_scope_from_daa, None);
    assert_eq!(reconstructed.root(), cached.root());
}

#[test]
fn scoped_honest_replay_refuses_a_wrong_verifier_model_and_does_not_excuse_missing_claim_material() {
    let mut w = World::new();
    w.l.commitment_scope_from_daa = Some(0);
    let job = w.post_job(2, &[3, 17, 9], 1, 1);
    let honest = w.honest(&job, 1);
    let id = honest.claim.id();
    w.block(10, vec![honest.tx, T::PanelCovered { claim: id }]);
    let empty = Da(Default::default());
    let verifier = OutsiderV1 { ledger: &w.l, claim: id, material: &empty, artifact: &w.params, salt: [0x5A; 64] };
    assert!(matches!(verifier.check_computation().unwrap(), OutsiderFindingV1::Clean));
    assert!(matches!(verifier.check().unwrap(), OutsiderFindingV1::Demand(_)), "public claim-specific DA duties remain owed");
    let mut wrong = w.params.clone();
    for tensor in wrong.tensors.values_mut() {
        if let Some(element) = tensor.data.first_mut() {
            *element += 1;
        }
    }
    let verifier = OutsiderV1 { ledger: &w.l, claim: id, material: &empty, artifact: &wrong, salt: [0x5A; 64] };
    assert!(verifier.check_computation().is_err(), "incorrect verifier acquisition is never a producer conviction");
    let producer_collateral = w.l.bonds[&PRODUCER].collateral;
    w.block(11, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: 0 }]);
    let events = w.block(11 + w.l.policy.court_deadline_daa + 1, vec![]);
    assert!(events.iter().any(|event| matches!(event, LedgerEventV1::ProducerDefault { .. })), "{events:?}");
    assert!(!w.l.claims[&id].convicted, "DA nonresponse has its own terminal, not an arithmetic verdict");
    assert!(w.l.bonds[&PRODUCER].collateral < producer_collateral);
}

#[test]
fn f_c4r4_17_class_serves_no_clear_activation_of_a_weight_relation() {
    let p = dense_moe_v1(7).program;
    let m = p.params.len();
    let e = court_model_exposure_v1(&p, m);
    assert!(e.min_linear_inner_dim.is_some(), "the class has model-linear relations: {e:?}");
    let budget = court_clear_position_budget_v1(&e);
    assert!(
        budget < e.min_linear_inner_dim.unwrap(),
        "the model's lifetime clear budget ({budget}) is below every relation's rank: no weight is ever determined"
    );
    // Past the budget (here: from the first demand when an elementwise relation exists) every position is CommitmentOnly.
    let d = court_position_disclosure_v1(budget, &e);
    assert_eq!(d, DisclosureV1::CommitmentOnly);
    let withheld = court_withheld_mask_v1(&p, m, d);
    let materials = court_node_materials_v1(&p, m);
    let mut relations = 0;
    for (o, (b, _)) in p.occurrences().iter().enumerate() {
        for (n, node) in p.blocks[*b as usize].nodes.iter().enumerate() {
            let reads_weight = node.inputs.iter().any(|r| matches!(r, Ref::Param(j) if (*j as usize) < m));
            if reads_weight && matches!(node.prim, misaka_palw_tir::Prim::MatMul) {
                relations += 1;
                assert!(withheld[o][n], "a weight relation's output is never served in the clear (occ {o}, node {n})");
                for r in &node.inputs {
                    if let Ref::Node(i) = r {
                        assert!(
                            withheld[o][*i as usize] || !materials[o][*i as usize].is_model_dependent(),
                            "its activation input is withheld unless it is model-independent"
                        );
                    }
                }
            }
        }
    }
    assert!(relations > 0, "F-C4R4-17 rebuilt such relations");
    // Model-independent values (none depends on a weight) stay owed in the clear: the claim-specific DA is intact.
    assert!(materials.iter().flatten().zip(withheld.iter().flatten()).all(|(mat, w)| *w == mat.is_model_dependent()));
}
