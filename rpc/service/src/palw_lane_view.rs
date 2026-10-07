//! **Lane SCAN: the explorer-facing shape of the execution lane's node-only reads.** Pure conversions
//! from `kaspa_consensus_core::palw_exec_view_v1` to the RPC rows, so they can be tested without a node.

use kaspa_consensus_core::palw_exec_view_v1::{
    PalwBlockLaneV1, PalwExecutionRowV1, PalwRoundBlockRecordV1, PalwRoundLaneHealthV1, PalwRoundOutcomeV1,
    palw_round_lane_telemetry_v1,
};
use kaspa_rpc_core::{RpcPalwExecBlock, RpcPalwExecutionRow, RpcPalwRefusalCount, RpcPalwRoundLaneHealth, RpcPalwRoundMark};

/// The most executions one `getPalwRoundLane` answers.
pub const MAX_EXECUTIONS_V1: usize = 256;

/// ADR-0125 semantic amendment: a complete header-derived view, never a guess from a missing header or a verdict.
pub fn palw_merge_view_v1(
    consensus: &dyn kaspa_consensus_core::api::ConsensusApi,
    ghostdag: &kaspa_consensus_core::trusted::ExternalGhostdagData,
) -> Option<kaspa_rpc_core::RpcPalwMergeView> {
    let classified = ghostdag
        .try_classify_palw_mergeset_v1(|member| {
            consensus
                .get_header(member)
                .map(|header| header.pow_algo_id == kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_ROUND_V1)
        })
        .ok()?;
    Some(kaspa_rpc_core::RpcPalwMergeView {
        round_blocks: classified.rounds().to_vec(),
        genuine_red_blocks: classified.genuine_reds().to_vec(),
    })
}

fn bond_string(bond: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2) -> String {
    format!("{}:{}", bond.0.transaction_id, bond.0.index)
}

fn mark_of(record: &PalwRoundBlockRecordV1) -> RpcPalwRoundMark {
    let (claim_id, verdict) = match record.outcome {
        PalwRoundOutcomeV1::Exec(lineage) => (lineage.claim_id.map(|c| c.to_string()), "granted"),
        PalwRoundOutcomeV1::Refused(_) => (None, "refused"),
    };
    RpcPalwRoundMark {
        hash: record.hash.to_string(),
        round: record.round,
        daa_score: record.daa_score,
        timestamp_ms: record.timestamp_ms,
        bond: record.bond.as_ref().map(bond_string).unwrap_or_default(),
        claim_id,
        verdict: verdict.to_string(),
    }
}

/// The health as the RPC row, from a ledger's snapshot.
pub fn health_to_rpc_v1(health: &PalwRoundLaneHealthV1, now_ms: u64, ledger_since_ms: u64) -> RpcPalwRoundLaneHealth {
    RpcPalwRoundLaneHealth {
        accepted_total: health.accepted_total,
        refused_total: health.refused_total,
        refused_since_last_accepted: health.refused_since_last_accepted,
        last_accepted: health.last_accepted.as_ref().map(mark_of),
        latest: health.latest.as_ref().map(mark_of),
        top_refusals: health
            .top_refusals
            .iter()
            .map(|(reason, count)| RpcPalwRefusalCount { reason: reason.to_string(), count: *count })
            .collect(),
        stale: health.stale,
        stale_after_ms: health.stale_after_ms,
        now_ms,
        ledger_since_ms,
    }
}

/// This node's ledger as the RPC health row, at `now_ms`.
pub fn round_lane_health_v1(now_ms: u64) -> RpcPalwRoundLaneHealth {
    let ledger = palw_round_lane_telemetry_v1();
    health_to_rpc_v1(&ledger.health(now_ms, 8), now_ms, ledger.started_ms())
}

pub fn execution_row_v1(row: &PalwExecutionRowV1) -> RpcPalwExecutionRow {
    RpcPalwExecutionRow {
        claim_id: row.claim_id.to_string(),
        class_id: row.class_id.map(|c| c.to_string()).unwrap_or_default(),
        executor_bond: bond_string(&row.executor_bond),
        domain: row.domain.to_string(),
        stage: row.stage.to_string(),
        status: row.status().to_string(),
        credit: row.credit,
        span: row.span,
        tickets: row.tickets,
        tickets_spent: row.tickets_spent,
        first_round: row.first_round,
        last_round: row.last_round,
        accepted_daa: row.accepted_daa,
        accepted_block: row.accepted_block.map(|b| b.to_string()),
        phase: row.phase.to_string(),
        final_daa: row.final_daa,
    }
}

/// A block's `laneClass` and, for a round block, its lineage.
pub fn block_lane_to_rpc_v1(lane: &PalwBlockLaneV1) -> (String, Option<RpcPalwExecBlock>) {
    let exec = lane.envelope.map(|(round, permit_index, bond)| {
        let mut block =
            RpcPalwExecBlock { round, permit_index, bond: bond_string(&bond), verdict: "unknown".to_string(), ..Default::default() };
        match lane.record.map(|r| r.outcome) {
            Some(PalwRoundOutcomeV1::Exec(lineage)) => {
                block.verdict = "granted".to_string();
                block.claim_id = lineage.claim_id.map(|c| c.to_string());
                block.class_id = lineage.class_id.map(|c| c.to_string());
                block.quantum_index = lineage.quantum_index;
                block.quantum_id = (lineage.quantum_id != kaspa_hashes::Hash64::default()).then(|| lineage.quantum_id.to_string());
            }
            Some(PalwRoundOutcomeV1::Refused(reason)) => {
                block.verdict = "refused".to_string();
                block.refusal = Some(reason.name().to_string());
            }
            None => {}
        }
        block
    });
    (lane.class.name().to_string(), exec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_exec_view_v1::{
        PalwBlockClassV1, PalwRoundLaneTelemetryV1, PalwRoundLineageV1, PalwRoundRefusalV1,
    };
    use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
    use kaspa_hashes::Hash64;

    fn h(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    #[test]
    fn adr0125_semantic_merge_view_uses_headers_and_missing_headers_make_it_unavailable() {
        use kaspa_consensus_core::api::ConsensusApi;
        use kaspa_consensus_core::errors::consensus::{ConsensusError, ConsensusResult};
        use kaspa_consensus_core::{BlockHash, header::Header, trusted::ExternalGhostdagData};
        use std::sync::Arc;
        struct Headers {
            missing: bool,
        }
        impl ConsensusApi for Headers {
            fn get_header(&self, hash: BlockHash) -> ConsensusResult<Arc<Header>> {
                if self.missing && hash == h(3) {
                    return Err(ConsensusError::HeaderNotFound(hash));
                }
                let mut header = Header::from_precomputed_hash(hash, vec![]);
                header.pow_algo_id = if hash == h(3) { 10 } else { 6 };
                Ok(Arc::new(header))
            }
        }
        let raw = ExternalGhostdagData {
            blue_score: 7,
            blue_work: Default::default(),
            selected_parent: h(1),
            mergeset_blues: vec![h(1)],
            mergeset_reds: vec![h(2), h(3), h(4)],
            blues_anticone_sizes: Default::default(),
        };
        let view = palw_merge_view_v1(&Headers { missing: false }, &raw).unwrap();
        assert_eq!(view.round_blocks, [h(3)]);
        assert_eq!(view.genuine_red_blocks, [h(2), h(4)]);
        assert_eq!(raw.mergeset_reds, [h(2), h(3), h(4)]);
        assert!(palw_merge_view_v1(&Headers { missing: true }, &raw).is_none());
    }
    fn bond(v: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 2 })
    }
    fn granted(hash: u64, ts: u64) -> PalwRoundBlockRecordV1 {
        PalwRoundBlockRecordV1 {
            hash: h(hash),
            daa_score: 50,
            timestamp_ms: ts,
            round: 9,
            permit_index: 0,
            bond: Some(bond(1)),
            outcome: PalwRoundOutcomeV1::Exec(PalwRoundLineageV1 {
                quantum_id: h(30),
                claim_id: Some(h(31)),
                quantum_index: Some(4),
                class_id: Some(h(32)),
            }),
        }
    }

    #[test]
    fn the_health_row_carries_the_counts_marks_and_the_stale_flag() {
        let ledger = PalwRoundLaneTelemetryV1::default();
        ledger.record(granted(1, 1_000));
        ledger.record_header_refusal(h(2), 2_000, 51, PalwRoundRefusalV1::HeaderSignature);
        let rpc = health_to_rpc_v1(&ledger.health(2_500, 4), 2_500, 77);
        assert_eq!((rpc.accepted_total, rpc.refused_total, rpc.refused_since_last_accepted), (1, 1, 1));
        assert_eq!(rpc.last_accepted.as_ref().unwrap().claim_id, Some(h(31).to_string()));
        assert_eq!(rpc.last_accepted.as_ref().unwrap().bond, format!("{}:2", TransactionId::from_u64_word(1)));
        assert_eq!(rpc.latest.as_ref().unwrap().verdict, "refused", "the newest judged block is the refused one");
        assert_eq!(rpc.top_refusals, vec![RpcPalwRefusalCount { reason: "header_signature_invalid".into(), count: 1 }]);
        assert!(!rpc.stale);
        assert_eq!((rpc.now_ms, rpc.ledger_since_ms), (2_500, 77));
    }

    #[test]
    fn a_granted_round_block_reports_its_claim_class_and_ticket() {
        let record = granted(1, 1_000);
        let lane = PalwBlockLaneV1 { class: PalwBlockClassV1::Exec, envelope: Some((9, 0, bond(1))), record: Some(record) };
        let (class, exec) = block_lane_to_rpc_v1(&lane);
        let exec = exec.unwrap();
        assert_eq!(class, "EXEC");
        assert_eq!((exec.round, exec.permit_index, exec.verdict.as_str()), (9, 0, "granted"));
        assert_eq!(exec.claim_id, Some(h(31).to_string()));
        assert_eq!(exec.class_id, Some(h(32).to_string()));
        assert_eq!((exec.quantum_index, exec.quantum_id), (Some(4), Some(h(30).to_string())));
        assert!(exec.refusal.is_none());
    }

    #[test]
    fn a_refused_round_block_is_round_and_names_its_reason() {
        let mut record = granted(1, 1_000);
        record.outcome = PalwRoundOutcomeV1::Refused(PalwRoundRefusalV1::PermitAlreadyUsed);
        let lane = PalwBlockLaneV1 { class: PalwBlockClassV1::Round, envelope: Some((9, 0, bond(1))), record: Some(record) };
        let (class, exec) = block_lane_to_rpc_v1(&lane);
        let exec = exec.unwrap();
        assert_eq!(class, "ROUND");
        assert_eq!((exec.verdict.as_str(), exec.refusal.as_deref()), ("refused", Some("permit_already_used")));
        assert!(exec.claim_id.is_none());
    }

    #[test]
    fn a_round_block_the_node_cannot_recall_is_round_and_unknown_never_exec() {
        let lane = PalwBlockLaneV1 { class: PalwBlockClassV1::Round, envelope: Some((9, 0, bond(1))), record: None };
        let (class, exec) = block_lane_to_rpc_v1(&lane);
        assert_eq!(class, "ROUND");
        assert_eq!(exec.unwrap().verdict, "unknown");
    }

    #[test]
    fn an_ordinary_block_has_a_class_and_no_lineage() {
        for (class, name) in [(PalwBlockClassV1::Blue, "BLUE"), (PalwBlockClassV1::Red, "RED"), (PalwBlockClassV1::Unmerged, "")] {
            let (got, exec) = block_lane_to_rpc_v1(&PalwBlockLaneV1 { class, envelope: None, record: None });
            assert_eq!(got, name);
            assert!(exec.is_none());
        }
    }

    #[test]
    fn a_lottery_permit_reports_no_ticket_id() {
        let mut record = granted(1, 1_000);
        record.outcome = PalwRoundOutcomeV1::Exec(PalwRoundLineageV1 {
            quantum_id: Hash64::default(),
            claim_id: None,
            quantum_index: None,
            class_id: None,
        });
        let lane = PalwBlockLaneV1 { class: PalwBlockClassV1::Exec, envelope: Some((9, 0, bond(1))), record: Some(record) };
        let exec = block_lane_to_rpc_v1(&lane).1.unwrap();
        assert_eq!(exec.verdict, "granted");
        assert!(exec.quantum_id.is_none() && exec.claim_id.is_none());
    }

    #[test]
    fn an_execution_row_converts_with_its_status_and_empty_strings_for_a_retired_claim() {
        let row = PalwExecutionRowV1 {
            claim_id: h(1),
            class_id: None,
            executor_bond: bond(5),
            domain: h(2),
            stage: "scheduled",
            credit: 1_000,
            span: 3,
            tickets: 120,
            tickets_spent: 120,
            first_round: Some(10),
            last_round: Some(900),
            accepted_daa: None,
            accepted_block: None,
            phase: "",
            final_daa: None,
        };
        let rpc = execution_row_v1(&row);
        assert_eq!((rpc.status.as_str(), rpc.tickets, rpc.tickets_spent), ("complete", 120, 120));
        assert_eq!(rpc.class_id, "");
        assert_eq!(rpc.first_round, Some(10));
    }
}
