//! Bounded native selected-chain traversal across pruned carrier/acceptance holes. A hole
//! never invents an answer and never marks the watermark complete. Older independently
//! authenticated accepted bodies remain reachable in the same bounded page.
use kaspa_consensus_core::{
    Hash64,
    api::ConsensusApi,
    palw_state_v2::{PalwConsensusObjectV2, PalwLifecycleTxPayloadV2},
};

pub(super) fn walk_available_v1(
    consensus: &dyn ConsensusApi,
    start: Hash64,
    floor: u64,
    maximum: usize,
    visit: &mut dyn FnMut(Hash64, PalwConsensusObjectV2),
) -> Result<(Option<Hash64>, bool), String> {
    let mut cursor = start;
    let mut incomplete = false;
    for _ in 0..maximum {
        let header = consensus.get_header(cursor).map_err(|e| format!("history header {cursor}: {e}"))?;
        if header.daa_score < floor {
            return Ok((None, incomplete));
        }
        match consensus.get_block_acceptance_data(cursor) {
            Ok(rows) => {
                for row in rows.iter().rev() {
                    if row.accepted_transactions.is_empty() {
                        continue;
                    }
                    let block = match consensus.get_block(row.block_hash) {
                        Ok(block) => block,
                        Err(_) => {
                            incomplete = true;
                            continue;
                        }
                    };
                    for entry in row.accepted_transactions.iter().rev() {
                        let Some(tx) = block.transactions.get(entry.index_within_block as usize) else {
                            incomplete = true;
                            continue;
                        };
                        if tx.subnetwork_id == kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE
                            && let Ok(payload) = borsh::from_slice::<PalwLifecycleTxPayloadV2>(&tx.payload)
                        {
                            visit(cursor, payload.object);
                        }
                    }
                }
            }
            Err(_) => incomplete = true,
        }
        // The parent/header path is the locally validated native chain. A missing parent is
        // an unavailable traversal, not permission to follow a peer-supplied arbitrary hash.
        let parent =
            consensus.get_ghostdag_data(cursor).map_err(|e| format!("history selected parent {cursor}: {e}"))?.selected_parent;
        if parent == cursor {
            return Ok((None, incomplete));
        }
        cursor = parent;
    }
    Ok((Some(cursor), incomplete))
}
