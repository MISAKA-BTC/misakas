//! Small native caller controls; enlarged geometry below is not a backend run.
use super::super::geometry_owner::{self, GeometryPlanV4};
use super::*;
use kaspa_consensus_core::palw_native_resource_v1::{
    NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, reserve_native_requests_v1,
};

fn opened(w: &World) -> PalwGdnHeldCaseV1 {
    palw_gdn_held_open_retained_v3(claim_of(&w.forged, w), &w.forged.view(), || w.backend.gdn_recompute_kernels_v2().unwrap())
        .unwrap()
        .unwrap()
}
fn native_demands(w: &World, c: Base0GdnCheckpointCourtV2) -> Vec<PalwHeldMissingV1> {
    match c {
        Base0GdnCheckpointCourtV2::Transition { node_slot, head } => {
            gdn_transition_demands_v2(&w.forged.binding, node_slot, head, w.index)
        }
        Base0GdnCheckpointCourtV2::ConvWindow { conv_slot, head } => {
            gdn_conv_window_demands_v2(&w.forged.binding, conv_slot, head, w.index)
        }
    }
    .unwrap()
}

#[test]
fn actual_multihead_transition_and_conv_loads_match_native_order_and_demands() {
    let w = world(0);
    let mut case = opened(&w);
    let plan = GeometryPlanV4::of(&w.forged.binding).unwrap();
    assert_eq!(plan.transition_heads, 4);
    assert_eq!(plan.conv_heads, 4);
    let mut expected = Vec::new();
    for conv in [false, true] {
        let mut slot = 0;
        while let Some((node, layer)) = w.profile.resolve_node_slot(slot) {
            if layer.is_some()
                && node.kernel_semantics_id == kernel_semantics_id_v1(if conv { KDESC_Q36_SSM_CONV } else { KDESC_Q36_GDN_STEP })
            {
                for head in 0..4 {
                    let c = if conv {
                        Base0GdnCheckpointCourtV2::ConvWindow { conv_slot: slot, head }
                    } else {
                        Base0GdnCheckpointCourtV2::Transition { node_slot: slot, head }
                    };
                    assert!(!native_demands(&w, c).is_empty());
                    expected.push(c);
                }
            }
            slot += 1;
        }
    }
    assert_eq!(case.candidates, expected);
    assert_eq!(case.candidates.len() as u64, plan.candidates);
    // Drive both actual Base0 paths, including the last head; the same forged
    // checkpoint need not implicate every candidate for its demands to exist.
    let conv = case.candidates.iter().position(|c| matches!(c, Base0GdnCheckpointCourtV2::ConvWindow { head: 3, .. })).unwrap();
    for at in [3, conv] {
        load_candidate_v1(&mut case, at, &w.forged.view(), &mut || w.backend.gdn_recompute_kernels_v2().unwrap()).unwrap();
        assert_eq!(case.at, at);
        assert_eq!(case.demands, native_demands(&w, case.candidates[at]));
        assert_eq!(case.answers.len(), case.demands.len());
        assert!(case.answers.iter().all(Option::is_none));
    }
}

#[test]
fn borrowed_geometry_collapses_full_window_and_does_not_truncate_large_head_inventory() {
    let w = world(0);
    let mut binding = w.forged.binding.clone();
    binding.shape_profile.state_chunk_map_id = kaspa_consensus_core::palw_state_chunk_map::hybrid_composed_state_chunk_map_id_v1();
    let full = GeometryPlanV4::of(&binding).unwrap();
    let (candidates, _owner) = geometry_owner::candidates(&binding).unwrap();
    assert_eq!(full.conv_heads, 1);
    assert!(
        candidates
            .iter()
            .filter(|c| matches!(c, Base0GdnCheckpointCourtV2::ConvWindow { .. }))
            .all(|c| matches!(c, Base0GdnCheckpointCourtV2::ConvWindow { head: 0, .. }))
    );
    let transitions = candidates.iter().filter(|c| matches!(c, Base0GdnCheckpointCourtV2::Transition { .. })).count();
    assert_eq!(transitions, 4 * candidates.iter().filter(|c| matches!(c, Base0GdnCheckpointCourtV2::ConvWindow { .. })).count());
    // Only geometry/inventory construction: no fabricated native acceptance or
    // large backend execution is claimed for this modified signed profile.
    binding = w.forged.binding.clone();
    binding.shape_profile.state_chunk_map_id = kaspa_consensus_core::palw_state_chunk_map::hybrid_state_chunk_map_id_v4();
    binding.shape_profile.gdn_heads = 4098;
    let large = GeometryPlanV4::of(&binding).unwrap();
    let (candidates, _owner) = geometry_owner::candidates(&binding).unwrap();
    assert_eq!(large.transition_heads, 4098);
    assert_eq!(large.conv_heads, 4098);
    assert_eq!(candidates.len() as u64, large.candidates);
    assert_eq!(large.candidates, 3 * 2 * 4098);
    assert!(candidates.iter().any(|c| matches!(c, Base0GdnCheckpointCourtV2::Transition { head: 4097, .. })));
    assert!(candidates.iter().any(|c| matches!(c, Base0GdnCheckpointCourtV2::ConvWindow { head: 4097, .. })));
}

#[test]
fn candidate_and_actual_demand_capacity_refusal_preserve_case_for_retry_and_retain_owners() {
    const CHILD: &str = "MISAKA_HELD_GDN_GEOMETRY_CHILD";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let name =
            format!("{}::candidate_and_actual_demand_capacity_refusal_preserve_case_for_retry_and_retain_owners", module_path!());
        let name = name.split_once("::").unwrap().1;
        assert!(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", name, "--nocapture", "--test-threads=1"])
                .env(CHILD, "1")
                .status()
                .unwrap()
                .success()
        );
        return;
    }
    let w = world(0);
    let mut case = opened(&w);
    case.demanded = Some((0, 110));
    let objects: Vec<_> = case.demands.iter().map(|unit| producer_answer(&w, &case.claim, *unit)).collect();
    note_answers_v1(&mut case, &w.forged.view(), &objects).unwrap();
    let before = (case.at, case.demands.clone(), borsh::to_vec(&case.answers).unwrap(), case.demanded);
    let ceiling = 256 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: ceiling, database_bytes: 0 }));
    let blocker = reserve_native_requests_v1(ceiling).unwrap();
    assert!(matches!(geometry_owner::candidates(&w.forged.binding), Err(PalwGdnHeldErrorV2::Resource(_))));
    let calls = std::cell::Cell::new(0);
    let mut kernels = || {
        calls.set(calls.get() + 1);
        w.backend.gdn_recompute_kernels_v2().unwrap()
    };
    assert!(matches!(load_candidate_v1(&mut case, 1, &w.forged.view(), &mut kernels), Err(PalwGdnHeldErrorV2::Resource(_))));
    assert_eq!(calls.get(), 0, "demand admission precedes even Base0 leg/prefix work at this boundary");
    assert_eq!((case.at, case.demands.clone(), borsh::to_vec(&case.answers).unwrap(), case.demanded), before);
    drop(blocker);
    let (candidates, candidate_owner) = geometry_owner::candidates(&w.forged.binding).unwrap();
    assert!(reserve_native_requests_v1(ceiling).is_err());
    drop(candidates);
    drop(candidate_owner);
    assert!(reserve_native_requests_v1(ceiling).is_ok());
    load_candidate_v1(&mut case, 1, &w.forged.view(), &mut kernels).unwrap();
    assert_eq!(case.at, 1);
    assert!(case.demanded.is_none());
    assert!(case.answers.iter().all(Option::is_none));
    assert!(reserve_native_requests_v1(ceiling).is_err(), "case keeps demand and answer-slot tickets beyond helper return");
    drop(case);
    assert!(reserve_native_requests_v1(ceiling).is_ok(), "retiring the case releases all newly admitted arrays");
}
