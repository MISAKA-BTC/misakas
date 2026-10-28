//! Independent regressions from the G14 review: resource bounds and shared progress under metadata saturation.
mod common;

use common::chain::T;
use common::ledger_world::{Da, outsider};
use common::ledger_world::{OUTSIDER, PRODUCER, PROSECUTION, World, position};
use common::{MAX_POSITIONS, active_for, root_of};
use misaka_palw_kernel::descriptor::k2_tir_v2_descriptor;
use misaka_palw_kernel::gate::public_prosecution_complete_v1;
use misaka_palw_kernel::gate::{MAX_DEMANDERS_PER_SESSION_V1, MAX_PROOF_SEALS_PER_CLAIM_V1};
use misaka_palw_kernel::ledger::{LedgerEventV1 as E, OutsiderFindingV1};
use misaka_palw_kernel::plan::plan_for_tir_program_v1;
use misaka_palw_kernel::public::ProfileMaterialV1;
use misaka_palw_kernel::rows::TABLE_SERVED_V1;
use misaka_palw_tir_sketch::fixture::wide128_v1;

#[test]
fn review_ram_bound_covers_the_artifacts_existing_tensor_storage() {
    let fx = wide128_v1(7);
    let d = k2_tir_v2_descriptor();
    let plan = plan_for_tir_program_v1(&d, &fx.program, root_of(&fx.program), MAX_POSITIONS).unwrap();
    misaka_palw_kernel::check::check_plan_v1(&active_for(&d), &d, &fx.program, root_of(&fx.program), &plan, 0).unwrap();
    let b = public_prosecution_complete_v1(&d, &fx.program, &plan, &ProfileMaterialV1::kernel_route(true), &PROSECUTION).unwrap();
    let live_artifact =
        fx.params.tensors.values().map(|t| t.data.capacity() as u128 * std::mem::size_of::<i128>() as u128).sum::<u128>();
    eprintln!(
        "RAM: artifact wire={} evidence/position={} claimed RAM={} live artifact data alone={}",
        plan.budgets.artifact_bytes, plan.budgets.evidence_bytes_per_position, b.max_verifier_ram, live_artifact
    );
    assert!(live_artifact <= b.max_verifier_ram, "the bound must cover even the artifact's live data, before caches and temporaries");
}

#[test]
fn review_retained_state_bound_covers_all_authenticated_served_positions() {
    let mut w = World::new();
    let bound = w.l.classes[&w.class].bounds.max_retained_state;
    let prompt = (0..63).map(|i| ((i * 7 + 3) % 32) as u32).collect::<Vec<_>>();
    let job = w.post_job(2, &prompt, 1, 41);
    let h = w.honest(&job, 1);
    let id = h.claim.id();
    let positions = h.trace.values.len() as u32;
    let responses =
        (0..positions).map(|p| T::Respond { claim: id, stage: 0, position: p, bytes: position(&h.trace, p, |_| {}) }).collect();
    w.block(10, vec![h.tx, T::PanelCovered { claim: id }]);
    w.block(11, (0..positions).map(|p| T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: p }).collect());
    w.block(12, responses);
    assert_eq!(w.l.served.len(), positions as usize);
    let served_bytes =
        w.l.to_rows()
            .iter()
            .filter(|((table, _), _)| *table == TABLE_SERVED_V1)
            .map(|((_, k), v)| (k.len() + v.len()) as u128)
            .sum::<u128>();
    eprintln!("retained state: positions={positions}, claimed bound={bound}, served rows alone={served_bytes}");
    assert!(served_bytes <= bound, "the per-claim retained bound must cover its persisted served bytes");
    let claim_rows: u128 =
        w.l.to_rows()
            .iter()
            .filter(|((table, key), _)| [6, 7, 8, 15, 17].contains(table) && key.starts_with(&id))
            .map(|((_, k), v)| (k.len() + v.len()) as u128)
            .sum();
    assert!(claim_rows <= bound, "all kernel claim, demand, served and proof-seal rows must fit");
    let resident = h.trace.values.iter().flatten().flatten().map(|t| t.data.capacity() as u128 * 16).sum::<u128>()
        + w.params.tensors.values().map(|t| t.data.capacity() as u128 * 16).sum::<u128>();
    assert!(resident <= w.l.classes[&w.class].bounds.max_verifier_ram, "whole-context derived history must be counted");
    assert!(w.l.bonds[&PRODUCER].reserved > 0);
}

#[test]
fn per_layer_artifact_instances_are_counted_and_low_ram_is_refused() {
    let mut fx = wide128_v1(7);
    assert!(fx.program.params.iter().any(|p| p.per_layer));
    let layer = fx.program.schedule.layers[0];
    fx.program.schedule.layers = vec![layer; 3];
    let d = k2_tir_v2_descriptor();
    let plan = plan_for_tir_program_v1(&d, &fx.program, root_of(&fx.program), MAX_POSITIONS).unwrap();
    misaka_palw_kernel::check::check_plan_v1(&active_for(&d), &d, &fx.program, root_of(&fx.program), &plan, 0).unwrap();
    let encoded: u128 = fx
        .program
        .params
        .iter()
        .map(|p| p.shape.iter().map(|d| *d as u128).product::<u128>() * p.dtype.width() as u128 * if p.per_layer { 3 } else { 1 })
        .sum();
    assert_eq!(plan.budgets.artifact_bytes, encoded);
    let ordinary =
        public_prosecution_complete_v1(&d, &fx.program, &plan, &ProfileMaterialV1::kernel_route(true), &PROSECUTION).unwrap();
    let mut repeated = d.clone();
    repeated.soundness.repetitions += 2;
    let repeated_plan = plan_for_tir_program_v1(&repeated, &fx.program, root_of(&fx.program), MAX_POSITIONS).unwrap();
    let repeated_bounds =
        public_prosecution_complete_v1(&repeated, &fx.program, &repeated_plan, &ProfileMaterialV1::kernel_route(true), &PROSECUTION)
            .unwrap();
    assert!(repeated_bounds.max_verifier_ram > ordinary.max_verifier_ram, "cached projections grow with repetitions");
    let policy = misaka_palw_kernel::gate::ProsecutionPolicyV1 { max_verifier_ram: 4176, ..PROSECUTION };
    let gaps = public_prosecution_complete_v1(&d, &fx.program, &plan, &ProfileMaterialV1::kernel_route(true), &policy).unwrap_err();
    assert!(gaps.iter().any(|g| matches!(g, misaka_palw_kernel::gate::ProsecutionGapV1::Unbounded { what: "verifier RAM", .. })));
}

fn spam_bonds() -> Vec<[u8; 64]> {
    (0..MAX_DEMANDERS_PER_SESSION_V1.max(MAX_PROOF_SEALS_PER_CLAIM_V1))
        .map(|i| {
            let mut id = [0x77; 64];
            id[..8].copy_from_slice(&(i as u64).to_le_bytes());
            id
        })
        .collect()
}

#[test]
fn full_join_and_seal_metadata_does_not_preempt_a_new_outsiders_conviction() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 42);
    let (at, lie) = w.lying(&job, 3);
    let claim = lie.claim.id();
    let bonds = spam_bonds();
    w.block(3, bonds.iter().map(|b| T::RegisterBond { bond: *b, collateral: 1000 }).collect());
    w.block(10, vec![lie.tx, T::PanelCovered { claim }]);
    let mut spam = bonds
        .iter()
        .take(MAX_DEMANDERS_PER_SESSION_V1)
        .map(|b| T::FileDemand { demander: *b, claim, stage: 0, position: at.0 })
        .collect::<Vec<_>>();
    spam.extend(bonds.iter().take(MAX_PROOF_SEALS_PER_CLAIM_V1).map(|b| T::SealProof { accuser: *b, claim, seal: [0x99; 64] }));
    w.block(11, spam);
    let key = (claim, 0, at.0);
    let before = w.l.to_rows();
    let events = w.block(
        12,
        vec![
            T::FileDemand { demander: OUTSIDER, claim, stage: 0, position: at.0 },
            T::SealProof { accuser: OUTSIDER, claim, seal: [0x88; 64] },
        ],
    );
    assert_eq!(events.iter().filter(|e| matches!(e, E::Refused { .. })).count(), 2);
    assert_eq!(w.l.to_rows(), before, "refused metadata joins leave committed rows unchanged");
    assert_eq!(w.l.demands[&key].demanders.len(), MAX_DEMANDERS_PER_SESSION_V1);
    assert_eq!(w.l.proof_seals.len(), MAX_PROOF_SEALS_PER_CLAIM_V1);
    assert_eq!(w.l.demands[&key].deadline_daa, 31);
    assert_eq!(w.l.bonds[&OUTSIDER].reserved, 0);
    w.block(13, vec![T::Respond { claim, stage: 0, position: at.0, bytes: position(&lie.trace, at.0, |_| {}) }]);
    let da = Da::publishing(&lie.trace, &[at]);
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, claim, &da) else { panic!("public served rows expose the lie") };
    let events = w.block(14, vec![T::FileProof { accuser: OUTSIDER, claim, proof }]);
    assert!(events.iter().any(|e| matches!(e,E::Convicted { claim:c,accuser,.. } if *c==claim && *accuser==OUTSIDER)));
    assert!(w.l.claims[&claim].convicted);
}

#[test]
fn full_join_metadata_does_not_cancel_or_delay_the_public_default() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 43);
    let honest = w.honest(&job, 3);
    let claim = honest.claim.id();
    let bonds = spam_bonds();
    w.block(3, bonds.iter().map(|b| T::RegisterBond { bond: *b, collateral: 1000 }).collect());
    w.block(10, vec![honest.tx, T::PanelCovered { claim }]);
    w.block(
        11,
        bonds
            .iter()
            .take(MAX_DEMANDERS_PER_SESSION_V1)
            .map(|b| T::FileDemand { demander: *b, claim, stage: 0, position: 1 })
            .collect(),
    );
    w.block(12, vec![T::FileDemand { demander: OUTSIDER, claim, stage: 0, position: 1 }]);
    let events = w.block(32, vec![]);
    assert!(events.iter().any(|e| matches!(e,E::ProducerDefault { claim:c,.. } if *c==claim)));
    assert!(!w.l.claims[&claim].convicted);
    assert!(w.l.demands.is_empty());
}
