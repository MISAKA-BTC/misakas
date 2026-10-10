//! Independent review regressions for integration 34b6c3f5e.
mod common;

use common::chain::T;
use common::ledger_world::{World, OUTSIDER, PRODUCER, position, PROSECUTION};
use common::{active_for, root_of, MAX_POSITIONS};
use misaka_palw_kernel::descriptor::k2_tir_v2_descriptor;
use misaka_palw_kernel::gate::public_prosecution_complete_v1;
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
    let nodes = fx.program.occurrences().iter().map(|(b, _)| fx.program.blocks[*b as usize].nodes.len() as u64).sum();
    let b = public_prosecution_complete_v1(&d, &plan, nodes, &ProfileMaterialV1::kernel_route(true), &PROSECUTION).unwrap();
    let live_artifact = fx.params.tensors.values().map(|t| t.data.capacity() as u128 * std::mem::size_of::<i128>() as u128).sum::<u128>();
    eprintln!("RAM: artifact wire={} evidence/position={} claimed RAM={} live artifact data alone={}", plan.budgets.artifact_bytes, plan.budgets.evidence_bytes_per_position, b.max_verifier_ram, live_artifact);
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
    let responses = (0..positions).map(|p| T::Respond { claim: id, stage: 0, position: p, bytes: position(&h.trace, p, |_| {}) }).collect();
    w.block(10, vec![h.tx, T::PanelCovered { claim: id }]);
    w.block(11, (0..positions).map(|p| T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: p }).collect());
    w.block(12, responses);
    assert_eq!(w.l.served.len(), positions as usize);
    let served_bytes = w.l.to_rows().iter().filter(|((table,_),_)| *table == TABLE_SERVED_V1).map(|((_,k),v)| (k.len()+v.len()) as u128).sum::<u128>();
    eprintln!("retained state: positions={positions}, claimed bound={bound}, served rows alone={served_bytes}");
    assert!(served_bytes <= bound, "the per-claim retained bound must cover its persisted served bytes");
    assert!(w.l.bonds[&PRODUCER].reserved > 0);
}
