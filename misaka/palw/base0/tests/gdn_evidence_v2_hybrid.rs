//! **RFC-0006 consumer v2 source control (written, not run here; not an acceptance result): the hybrid accuser's GDN evidence from
//! a real base0 capture.**
//!
//! The genesis hybrid shape — graph-v8: profile version 3 on the held composition whose recurrence half spells the key-head count
//! (v5), at the dev fixture's 2 key / 4 value heads — produces an honest capture; for EVERY recurrence step leaf the hybrid backend
//! assembles the anchored step evidence from that capture (`Qwen36Backend::gdn_step_evidence_v2`: the served tiles, the leg rebuilt
//! from the served leaves, the predecessor checkpoint recomputed by the accuser and authenticated against the committed leaf), and
//! the consensus court (`memory_v2::check_gdn_step_refutation_v2`) answers `NoFaultFound` on it — never an evidence refusal.
mod common;

use common::*;
use kaspa_consensus_core::palw_attempt_rules_v1::PalwAttemptRulesV1;
use kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1;
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_qwen36_profile::{qwen36_held_canonical_v1, qwen36_profile_v8};
use kaspa_consensus_core::palw_step::{canonical_step_coordinates, kernel_semantics_id_v1};
use kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES;
use kaspa_consensus_core::palw_step_refute::memory_v2::{PalwGdnCourtErrorV2, check_gdn_step_refutation_v2, gdn_replay_start_v2};
use kaspa_consensus_core::palw_step_refute::{KDESC_Q36_GDN_STEP, PalwStepRefuteError};
use kaspa_hashes::Hash64;
use misaka_palw_base0::gdn_evidence_v2::Base0GdnEvidenceV2;

#[test]
fn every_recurrence_leaf_of_an_honest_held_v8_capture_yields_evidence_the_court_acquits() {
    let (artifact, mut geometry) = qwen36_fixture();
    geometry.n_ctx = 32;
    let profile = qwen36_profile_v8(geometry).expect("graph-v8 (version 3, held v5) projects at the fixture geometry");
    let backend = qwen36_backend(&artifact, &profile, qwen36_held_canonical_v1(32)).with_attempt_rules(PalwAttemptRulesV1::CoreV1);
    let (canonical, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x6D_6E01)).expect("the anchor implies a job");
    let out = backend.execute(&palw_attempt_job_v1(canonical, true), &prompt).expect("the honest attempt runs");
    let retention = misaka_palw_base0::produce::base0_material_decode_any_v1(&out.material).expect("decodes");
    let binding = retention.binding().clone();

    // The production oracle: every inventory row proven against its own root.
    let inventory = misaka_palw_base0::inventory::qwen36_inventory_v1(&artifact, &profile).expect("the class yields an inventory");
    let openings: Vec<_> = (0..inventory.operands().len())
        .map(|i| kaspa_consensus_core::palw_artifact::open_artifact_leaf_v1(inventory.operands(), i as u32).unwrap())
        .collect();
    let oracle = kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&openings, inventory.root())
        .expect("every inventory row proves against its own root");

    let gdn_step = kernel_semantics_id_v1(KDESC_Q36_GDN_STEP);
    let (mut judged, mut anchored) = (0u32, 0u32);
    for index in 0..binding.step_leaf_count {
        let coord = canonical_step_coordinates(&binding.shape_profile, &binding.job_context, index).expect("a main coordinate");
        let Some((node, Some(_))) = binding.shape_profile.resolve_node_slot(coord.node_slot) else { continue };
        if node.kernel_semantics_id != gdn_step {
            continue;
        }
        let start = gdn_replay_start_v2(&binding, &coord).expect("a held v5 recurrence leaf has a replay start");
        let evidence = backend.gdn_step_evidence_v2(&out.material, None, index).unwrap_or_else(|e| panic!("leaf {index}: {e}"));
        let Base0GdnEvidenceV2::Step(refutation) = evidence else {
            panic!("leaf {index}: an honest capture has no forged checkpoint");
        };
        assert_eq!(refutation.checkpoint.is_some(), start.checkpoint_index.is_some(), "leaf {index}: the anchor the court demands");
        if let (Some(witness), Some(at)) = (&refutation.checkpoint, start.checkpoint_index) {
            assert_eq!(witness.leaf.checkpoint_index, at, "leaf {index}: the exact predecessor checkpoint");
            anchored += 1;
        }
        let verdict = check_gdn_step_refutation_v2(
            &refutation,
            binding.committed_execution_root,
            profile.shape_profile_id(),
            &oracle,
            PALW_STEP_LEG_MAX_LEAVES,
        );
        assert!(
            matches!(verdict, Err(PalwGdnCourtErrorV2::Evidence(PalwStepRefuteError::NoFaultFound))),
            "leaf {index}: an honest step is acquitted on its own evidence, got {verdict:?}"
        );
        judged += 1;
    }
    assert!(judged > 0, "the hybrid has recurrence step leaves");
    // Whether any leaf stands on a checkpoint depends on the derived recurrence spacing at this context; recorded, not assumed.
    let _ = anchored;
}
