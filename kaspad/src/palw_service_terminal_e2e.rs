//! Actual service boundary case: ordinary registered tiled class and legal ~110KiB FP job.
//! Included manifest/parts are independently reconstructed without producer capture or state.
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn registered_tiled_large_job_convicts_producer_and_entire_panel_via_terminal_carriage() {
    fresh_public_service(PublicCase::TiledConstraintLargeJob).await;
}

pub(super) fn enlarge_selection_job_v1(job: &mut kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3) {
    use kaspa_consensus_core::{palw_decode_constraint_v1::*, palw_fp_constraint_v2::palw_constraint_of_bytes_v1};
    let Some(kaspa_consensus_core::palw_freeprompt_v3::PalwFpJobTailV1::Constraint(tail)) = job.tail.as_mut() else {
        panic!("constrained job")
    };
    let mut c = palw_constraint_of_bytes_v1(&tail.constraint).unwrap();
    c.version = 2;
    // Preserve the reachable selector exactly, adding a legal unreachable public frame.
    // The original constraint/table remains the only authority on all generated tokens.
    let edges = (0..128).map(|byte| PalwConstraintEdgeV1 { lo: byte, hi: byte, action: PalwConstraintActionV1::Goto(0) }).collect();
    c.frames.push(PalwConstraintFrameV1 { start: 0, nodes: vec![PalwConstraintNodeV1 { accepting: true, edges }; 169] });
    tail.constraint = c.to_bytes();
    assert!(tail.constraint.len() > 100_000);
    assert!(tail.constraint.len() < 120_000);
    kaspa_consensus_core::palw_fp_constraint_v2::palw_constraint_admitted_v1(&tail.constraint, true).unwrap();
}

pub(super) fn included_executor_evidence_v1(
    carriers: &[PalwConsensusObjectV2],
    claim: Hash64,
    producer: PalwBondKeyV2,
) -> Option<kaspa_consensus_core::palw_offence_attribution_v1::PalwExecutorRefutedEvidenceV1> {
    use kaspa_consensus_core::palw_terminal_carriage_v1 as t;
    for object in carriers {
        let PalwConsensusObjectV2::ObjectiveEvidenceDeclaredV1 { declaration: d } = object else { continue };
        if d.claim != claim
            || d.accused != producer
            || d.kind != kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::ExecutorRefuted
        {
            continue;
        }
        d.check(t::PALW_TERMINAL_MAX_BYTES_V1).unwrap();
        assert!(d.total_bytes > kaspa_consensus_core::palw_offence_attribution_v1::PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES);
        let mut parts = std::collections::BTreeMap::new();
        for object in carriers {
            let PalwConsensusObjectV2::ObjectiveEvidenceChunkV1 { part } = object else { continue };
            if (part.claim, part.reserver, part.evidence_id) != (d.claim, d.reserver, d.evidence_id) {
                continue;
            }
            let mut row =
                t::PalwPendingTerminalV1 { declaration: (**d).clone(), deadline_daa: 0, parts: std::collections::BTreeMap::new() };
            if row.check_part(part).is_ok() {
                row.parts.insert(part.index, part.bytes.clone());
                parts.insert(part.index, part.bytes.clone());
            }
        }
        if parts.len() != d.part_digests.len() {
            continue;
        }
        let bytes: Vec<u8> = parts.values().flatten().copied().collect();
        assert_eq!(bytes.len() as u64, d.total_bytes);
        assert_eq!(kaspa_consensus_core::palw_offence_v1::palw_offence_evidence_digest_v1(&bytes), d.evidence_id);
        let offence = kaspa_consensus_core::palw_offence_attribution_v1::palw_executor_refuted_offence_id_v1(&producer.0, &claim);
        let reveal = carriers
            .iter()
            .find_map(|o| match o {
                PalwConsensusObjectV2::ReporterRevealed { offence_key, reporter, salt }
                    if *offence_key == offence && *reporter == d.reserver =>
                {
                    Some(*salt)
                }
                _ => None,
            })
            .expect("the actual service reveals this terminal conviction");
        let commitment =
            kaspa_consensus_core::palw_state_v2::palw_reporter_commitment_v1(&offence, &d.evidence_id, &d.reserver, &reveal);
        assert!(carriers.iter().any(|o|matches!(o,PalwConsensusObjectV2::ReporterCommitted {commitment:c,reporter,..} if *c==commitment && *reporter==d.reserver)),"carriage preserves the reporter's original evidence identity");
        return borsh::from_slice(&bytes).ok();
    }
    None
}
