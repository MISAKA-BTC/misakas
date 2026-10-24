//! Actual immutable source, shared row visitor, native quote and held consumer.
use super::*;
use kaspa_consensus_core::palw_native_resource_v1::{
    NativeHeapV1, NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, reserve_native_requests_v1,
};
use kaspa_consensus_core::palw_step::PalwCanonicalStepRankV3;
use kaspa_consensus_core::palw_step_refute::memory_v2::{
    PalwGdnCaptureV2, gdn_conv_refutation_requests_v3, gdn_conv_window_required_v2, visit_gdn_conv_required_rows_v3,
};

fn conv_case(w: &World) -> PalwGdnHeldCaseV1 {
    let mut case =
        palw_gdn_held_open_retained_v3(claim_of(&w.forged, w), &w.forged.view(), || w.backend.gdn_recompute_kernels_v2().unwrap())
            .unwrap()
            .unwrap();
    let at = case.candidates.iter().position(|c| matches!(c, Base0GdnCheckpointCourtV2::ConvWindow { head: 0, .. })).unwrap();
    load_candidate_v1(&mut case, at, &w.forged.view(), &mut || w.backend.gdn_recompute_kernels_v2().unwrap()).unwrap();
    let chain: Vec<_> = case.demands.iter().map(|unit| producer_answer(w, &case.claim, *unit)).collect();
    note_answers_v1(&mut case, &w.forged.view(), &chain).unwrap();
    case
}
fn capture<'a>(w: &'a World, leg: &'a misaka_palw_base0::legs::Base0CheckpointsV1) -> PalwGdnCaptureV2<'a> {
    PalwGdnCaptureV2 {
        binding: &w.forged.binding,
        step_leaf_hashes: &w.forged.tiles.leaves,
        checkpoint_leaf_hashes: &leg.leaf_hashes,
        max_leaves: w.forged.max_leaves,
    }
}

#[test]
fn actual_conv_shared_visitor_and_cached_rank_match_native_rows_for_first_and_last_head() {
    let w = world_with_fault(0, true);
    let case = conv_case(&w);
    let Base0GdnCheckpointCourtV2::ConvWindow { conv_slot, .. } = case.candidates[case.at] else { panic!("conv") };
    let leg = misaka_palw_base0::gdn_evidence_v2::base0_gdn_leg_v2(&w.forged.view()).unwrap();
    let rank = PalwCanonicalStepRankV3::new_v3(&w.profile, &w.forged.binding.job_context).unwrap();
    for head in [0, u32::from(w.profile.gdn_heads) - 1] {
        let native = gdn_conv_window_required_v2(&w.forged.binding, conv_slot, head, case.index).unwrap();
        let mut borrowed = Vec::new();
        visit_gdn_conv_required_rows_v3(&w.forged.binding, conv_slot, head, case.index, |source| {
            let mut row = Vec::new();
            source.visit_tiles_v3(|index, coordinate| {
                assert_eq!(Some(index), rank.index_v3(&coordinate));
                assert_eq!(
                    Some(index),
                    kaspa_consensus_core::palw_step::canonical_step_leaf_index(&w.profile, &w.forged.binding.job_context, &coordinate)
                );
                row.push((index, coordinate));
                Ok(())
            })?;
            assert_eq!(source.tile_count_v3(), row.len() as u64);
            borrowed.push(row);
            Ok(())
        })
        .unwrap();
        assert_eq!(borrowed, native);
        let q = gdn_conv_refutation_requests_v3(&capture(&w, &leg), conv_slot, head, case.index, |at, _| {
            w.forged.tiles.tiles.iter().find(|(i, _)| *i == at).map(|(_, leaf)| leaf)
        })
        .unwrap();
        assert_eq!(q.required_rows, 3 * u64::from(w.positions.min(u32::from(w.profile.gdn_conv_kernel))));
        assert_eq!(q.required_tiles, native.iter().map(|r| r.len() as u64).sum::<u64>());
        let runs = native
            .iter()
            .map(|row| 1 + row.windows(2).filter(|pair| pair[0].0.checked_add(1) != Some(pair[1].0)).count() as u64)
            .sum::<u64>();
        assert_eq!(q.required_runs, runs);
        assert!(q.persistent_bytes > w.forged.binding.native_heap_v1("conv source").bytes().unwrap());
        assert!(
            q.workspace_bytes
                >= ((w.forged.tiles.leaves.len() + w.forged.tiles.leaves.len().div_ceil(2)) * std::mem::size_of::<Hash64>()) as u64
        );
    }
    assert!(matches!(
        gdn_conv_refutation_requests_v3(&capture(&w, &leg), conv_slot, u32::from(w.profile.gdn_heads), case.index, |_, _| None),
        Err(PalwGdnCourtErrorV2::Evidence(_))
    ));
}

#[test]
fn actual_zero_cache_borrowed_conv_scan_and_full_role_admission_survive_court_output() {
    const CHILD: &str = "MISAKA_GDN_CONV_INPUT_OWNER_CHILD";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let name = format!("{}::actual_zero_cache_borrowed_conv_scan_and_full_role_admission_survive_court_output", module_path!());
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
    let w = world_with_fault(0, true);
    let mut case = conv_case(&w);
    let before = (case.at, case.demands.clone(), borsh::to_vec(&case.answers).unwrap(), case.demanded);
    let Base0GdnCheckpointCourtV2::ConvWindow { conv_slot, head } = case.candidates[case.at] else { panic!("conv") };
    let leg = misaka_palw_base0::gdn_evidence_v2::base0_gdn_leg_v2(&w.forged.view()).unwrap();
    let source = |at, _| w.forged.tiles.tiles.iter().find(|(i, _)| *i == at).map(|(_, leaf)| leaf);
    let q = gdn_conv_refutation_requests_v3(&capture(&w, &leg), conv_slot, head, case.index, source).unwrap();
    let answers: Vec<_> = case.answers.iter().map(|a| (**a.as_ref().unwrap()).clone()).collect();
    let ceiling = 256 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: ceiling, database_bytes: 0 }));
    let blocker = reserve_native_requests_v1(ceiling).unwrap();
    let reads = std::cell::Cell::new(0);
    assert!(
        w.profile
            .pre_nodes
            .iter()
            .chain(&w.profile.gdn_nodes)
            .chain(&w.profile.attn_nodes)
            .chain(&w.profile.post_nodes)
            .all(|node| matches!(node.out_len, kaspa_consensus_core::palw_step::PalwStepOutLenV1::Fixed { .. })),
        "the actual fused-attention fixture has no KvScaled rank terms"
    );
    let zero_cache = PalwCanonicalStepRankV3::new_v3(&w.profile, &w.forged.binding.job_context)
        .expect("zero-sized native rank arrays need no artificial positive reservation");
    let scanned = gdn_conv_refutation_requests_v3(&capture(&w, &leg), conv_slot, head, case.index, |at, coordinate| {
        reads.set(reads.get() + 1);
        source(at, coordinate)
    })
    .expect("borrowed all-Fixed scan legitimately succeeds with no rank allocation");
    assert_eq!(scanned, q);
    assert!(scanned.required_tiles > 0);
    assert_eq!(reads.get() as u64, scanned.required_tiles);
    drop(zero_cache);
    assert!(
        matches!(q.reserve_v3(), Err(PalwGdnCourtErrorV2::Resource(_))),
        "complete role admission precedes native input/tree construction"
    );
    assert_eq!((case.at, case.demands.clone(), borsh::to_vec(&case.answers).unwrap(), case.demanded), before);
    drop(blocker);
    assert!(
        matches!(
            gdn_conv_refutation_requests_v3(&capture(&w, &leg), conv_slot, head, case.index, |_, _| None),
            Err(PalwGdnCourtErrorV2::Evidence(_))
        ),
        "source absence stays an evidence refusal"
    );
    let built = base0_gdn_conv_window_evidence_from_disclosures_retained_v4(
        &w.forged.view(),
        conv_slot,
        head,
        case.index,
        &case.demands,
        &answers,
    )
    .unwrap();
    assert!(reserve_native_requests_v1(ceiling).is_err(), "persistent full input roles and separate witness survive builder return");
    assert!(
        terminal_found_fault_v1(check_gdn_conv_window_refutation_v2(
            &built,
            case.claim.execution_root,
            case.claim.shape_profile_id,
            w.forged.max_leaves
        ))
        .unwrap()
    );
    assert!(reserve_native_requests_v1(ceiling).is_err(), "court scratch retirement does not retire input tickets");
    let signs = std::cell::Cell::new(0);
    let signer = |m: &[u8], c: &[u8]| {
        assert!(reserve_native_requests_v1(ceiling).is_err());
        signs.set(signs.get() + 1);
        sign(m, c)
    };
    let accuser = PalwGdnHeldAccuserV1 {
        bond: bond(2),
        network_domain: Hash64::from_u64_word(0xD0),
        openings: &w.openings,
        oracle: &w.oracle,
        sign: &signer,
    };
    let object = built
        .try_handoff_v4(|refutation| {
            super::super::action_owner::accusation(&case.claim, PalwMemoryRoutingEvidenceV2::GdnConvWindow { refutation }, &accuser)
        })
        .unwrap();
    assert!(reserve_native_requests_v1(ceiling).is_err(), "signed output independently owns its copied inputs");
    drop(object);
    assert!(reserve_native_requests_v1(ceiling).is_ok());
    let action = palw_gdn_held_step_retained_v2(
        &mut case,
        &w.forged.view(),
        || w.backend.gdn_recompute_kernels_v2().unwrap(),
        &[],
        100,
        20,
        &accuser,
    )
    .unwrap();
    assert!(matches!(action, PalwGdnHeldNextRetainedV2::Accuse { .. }));
    assert_eq!(signs.get(), 2);
    drop(action);
    assert!(reserve_native_requests_v1(ceiling).is_ok());
}
