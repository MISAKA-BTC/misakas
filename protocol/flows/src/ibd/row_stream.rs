//! One RFC-0037 row-stream generation on the existing IBD import path.
//!
//! The bytes are fed to `palw_accept_row_stream_generation_v1` first. That function returns only
//! when `PalwRowStreamVerifierV1` accepts the stream.
//! The suffix is then the existing snapshot envelope, which the caller installs with
//! `import_pruning_point_palw_state`.
use super::IbdFlow;
use crate::palw_snapshot_chunks::{Snapshot, decode_snapshot_envelope};
use kaspa_consensus_core::{
    BlockHash, Hash64,
    api::ConsensusApi,
    blockstatus::BlockStatus,
    palw_row_stream_v1::{self, PalwRowStreamErrorV1},
    palw_state_v2::palw_state_root_of_preimage_v1,
};
use kaspa_consensusmanager::ConsensusProxy;
use kaspa_p2p_lib::{common::ProtocolError, pb::PruningPointPalwRowStreamMessage};

const WITNESS_WALK_LIMIT: usize = 1_048_576;

fn row_stream_error(err: PalwRowStreamErrorV1) -> ProtocolError {
    match err {
        PalwRowStreamErrorV1::ByteBudget(_) | PalwRowStreamErrorV1::Sink(_) => {
            ProtocolError::LocalNativeResourceUnavailable(err.to_string())
        }
        other => ProtocolError::OtherOwned(format!("PALW row stream generation refused: {other}")),
    }
}

/// The selected-chain child of the pruning point: the same headers-selected tip and selected-parent
/// chain the existing import already uses. This reads that header's committed root.
fn witness_root(consensus: &dyn ConsensusApi, pruning_point: BlockHash) -> Result<Hash64, String> {
    let mut cursor = consensus.get_header_download_hint();
    let mut previous = None;
    for _ in 0..WITNESS_WALK_LIMIT {
        if cursor == pruning_point {
            return Err("pruning point has no selected-chain child to witness the row stream".into());
        }
        let header = consensus.get_header(cursor).map_err(|err| err.to_string())?;
        let parent = consensus.get_ghostdag_data(cursor).map_err(|err| err.to_string())?.selected_parent;
        if parent == pruning_point {
            if consensus.get_block_status(cursor) == Some(BlockStatus::StatusDisqualifiedFromChain) {
                return Err("selected-chain child is disqualified and witnesses nothing".into());
            }
            return Ok(header.palw_state_root);
        }
        if parent == cursor || previous == Some(parent) {
            return Err("selected-parent walk cycled before the pruning point".into());
        }
        previous = Some(cursor);
        cursor = parent;
    }
    Err("selected-parent walk exceeded the existing header bound".into())
}

impl IbdFlow {
    pub(super) async fn accept_pruning_point_palw_row_stream(
        &self,
        consensus: &ConsensusProxy,
        point: BlockHash,
        message: &PruningPointPalwRowStreamMessage,
        already_imported: Option<u64>,
    ) -> Result<Snapshot, ProtocolError> {
        if message.byte_len != message.generation_bytes.len() as u64 {
            return Err(ProtocolError::Other("truncated PALW row stream generation"));
        }
        let reply_point = BlockHash::try_from(
            message.pruning_point_hash.clone().ok_or(ProtocolError::Other("PALW row stream lacks a pruning point"))?,
        )
        .map_err(|_| ProtocolError::Other("invalid PALW row stream pruning point"))?;
        if reply_point != point {
            return Err(ProtocolError::Other("PALW row stream changed pruning point"));
        }
        let consensus = consensus.clone();
        let expected_root = consensus
            .spawn_blocking(move |api| witness_root(api, point))
            .await
            .map_err(|why| ProtocolError::OtherOwned(format!("PALW row stream has no witness header: {why}")))?;
        let accepted = palw_row_stream_v1::palw_accept_row_stream_generation_v1(
            expected_root,
            &message.generation_bytes,
            std::io::sink(),
            palw_row_stream_v1::PALW_ROW_STREAM_MAX_GENERATION_BYTES_V1 as u64,
            already_imported,
        )
        .map_err(row_stream_error)?;
        if accepted.import_suffix().is_empty() {
            return Err(ProtocolError::Other("truncated PALW row stream generation"));
        }
        let snapshot = decode_snapshot_envelope(accepted.import_suffix())
            .map_err(|_| ProtocolError::Other("PALW row stream import suffix is not the existing snapshot envelope"))?;
        accepted
            .into_whole_state(|_, preimage, root| {
                if palw_state_root_of_preimage_v1(preimage) == root { Ok(()) } else { Err("preimage".into()) }
            })
            .map_err(row_stream_error)?;
        Ok(snapshot)
    }
}
