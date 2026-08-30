//! **The heartbeat rules' evidence source** (ADR-0060 Decisions 1–2).
//!
//! The slot rule and the lane retarget need the POV's youngest heartbeat, its youngest
//! bonded-lane block, and a short run of recent heartbeats — in CHAIN order. The difficulty
//! window cannot serve this: it is SAMPLED (`difficulty_sample_rate`), so the newest blocks —
//! precisely the ones the slot rule is about — may not be in it at all, and the integration
//! test caught the rule waving a second heartbeat through on exactly that gap.
//!
//! So the evidence is a bounded selected-parent-chain walk from the validated header's own POV:
//! deterministic per POV, complete over the span that matters, and run only for heartbeat
//! (algo-3) headers — which the slot rule itself keeps rare.

use crate::model::stores::{ghostdag::GhostdagStoreReader, headers::HeaderStoreReader};
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::palw_heartbeat_v1::{
    HEARTBEAT_EVIDENCE_MAX_BLOCKS, HEARTBEAT_RETARGET_ROWS, HeartbeatWindowBlock, PALW_HEARTBEAT_ALGO_ID,
};
use kaspa_consensus_core::pow_layer0::is_palw_v2_algo_id;

/// Walk the selected-parent chain from `selected_parent` toward genesis, collecting
/// [`HeartbeatWindowBlock`] rows until the evidence is sufficient: at least one bonded-lane
/// block (the ramp's silence baseline) AND [`HEARTBEAT_RETARGET_ROWS`] heartbeats (the
/// retarget's span), or genesis, or the [`HEARTBEAT_EVIDENCE_MAX_BLOCKS`] safety cap.
///
/// Chain order, not timestamp order: the youngest heartbeat "in the window" is the nearest one
/// by chain distance, whatever timestamp it stamped — so back-dating a heartbeat cannot free
/// the next slot, it only compresses the measured span and makes the retarget price the lane
/// harder.
pub fn collect_heartbeat_evidence(
    ghostdag_store: &impl GhostdagStoreReader,
    headers_store: &impl HeaderStoreReader,
    selected_parent: BlockHash,
    genesis_hash: BlockHash,
) -> Vec<HeartbeatWindowBlock> {
    let mut rows = Vec::new();
    let (mut heartbeats, mut bonded) = (0usize, 0usize);
    let mut cur = selected_parent;
    for _ in 0..HEARTBEAT_EVIDENCE_MAX_BLOCKS {
        let Ok(header) = headers_store.get_header(cur) else { break };
        let algo = header.pow_algo_id;
        rows.push(HeartbeatWindowBlock { timestamp: header.timestamp, bits: header.bits, algo_id: algo });
        if algo == PALW_HEARTBEAT_ALGO_ID {
            heartbeats += 1;
        } else if is_palw_v2_algo_id(algo) {
            bonded += 1;
        }
        if bonded >= 1 && heartbeats >= HEARTBEAT_RETARGET_ROWS {
            break;
        }
        if cur == genesis_hash {
            break;
        }
        let Ok(parent) = ghostdag_store.get_selected_parent(cur) else { break };
        cur = parent;
    }
    rows
}
