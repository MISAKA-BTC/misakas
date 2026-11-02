//! One RFC-0037 row-stream generation on the existing IBD import path.
//!
//! The bytes are fed to `palw_accept_row_stream_generation_v1` first. That function returns only
//! when `PalwRowStreamVerifierV1` accepts the stream.
//! The suffix is then the existing snapshot envelope, which the caller installs with
//! `import_pruning_point_palw_state`.
use super::IbdFlow;
use crate::palw_snapshot_chunks::{Snapshot, decode_row_snapshot_with_native_lease_v1};
use kaspa_consensus_core::{
    BlockHash, Hash64,
    api::ConsensusApi,
    palw_row_stream_v1::{self, PalwRowStreamErrorV1},
};
use kaspa_consensusmanager::ConsensusProxy;
use kaspa_p2p_lib::{common::ProtocolError, pb::PruningPointPalwRowStreamMessage};


fn row_stream_error(err: PalwRowStreamErrorV1) -> ProtocolError {
    match err {
        PalwRowStreamErrorV1::NativeResource(unavailable) => ProtocolError::LocalNativeCapacityUnavailable(unavailable),
        PalwRowStreamErrorV1::ByteBudget(_) | PalwRowStreamErrorV1::Sink(_) => {
            ProtocolError::LocalNativeResourceUnavailable(err.to_string())
        }
        other => ProtocolError::OtherOwned(format!("PALW row stream generation refused: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_snapshot_native_refusal_keeps_the_typed_local_capacity_cause() {
        let capacity = kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<Snapshot>(
            "row stream test", "local reservation refused",
        );
        assert!(matches!(row_stream_error(PalwRowStreamErrorV1::NativeResource(capacity)),
            ProtocolError::LocalNativeCapacityUnavailable(_)));
        assert!(matches!(row_stream_error(PalwRowStreamErrorV1::TruncatedGeneration), ProtocolError::OtherOwned(_)));
    }
}

/// The selected-chain child of the pruning point: the same headers-selected tip and selected-parent
/// chain the existing import already uses. This reads that header's committed root.
pub(super) fn witness_root(consensus:&dyn ConsensusApi,pruning_point:BlockHash)->Result<Hash64,String>{
    consensus.pruning_point_palw_row_witness_root_v1(pruning_point)
        .ok_or_else(||"exact pruning-session flat-state witness unavailable".into())
}

// One bounded diagnostic slot, never a state-import authority or storage marker.
// It lets node integration checks distinguish an authenticated codec2 receive
// from a negotiated109 connection which used the explicit107 fallback.
static LAST_AUTHENTICATED_ROW_SNAPSHOT_V1: std::sync::Mutex<Option<(u64, BlockHash, Hash64, u64)>> = std::sync::Mutex::new(None);

impl IbdFlow {
    /// Process-local receive telemetry: sequence, pruning point, authenticated
    /// state root, complete collection count. This records staging validation;
    /// ordinary atomic import and stable-generation publication are separate.
    pub fn last_authenticated_row_snapshot_v1() -> Option<(u64, BlockHash, Hash64, u64)> {
        *LAST_AUTHENTICATED_ROW_SNAPSHOT_V1.lock().unwrap()
    }

    pub(super) fn record_authenticated_row_snapshot_v1(point: BlockHash, root: Hash64, collections: u64) {
        let mut last = LAST_AUTHENTICATED_ROW_SNAPSHOT_V1.lock().unwrap();
        let sequence = last.as_ref().map_or(1, |receipt| receipt.0.saturating_add(1));
        *last = Some((sequence, point, root, collections));
    }

    pub(super) async fn accept_pruning_point_palw_row_stream(
        &mut self,
        consensus: &ConsensusProxy,
        point: BlockHash,
        message: &PruningPointPalwRowStreamMessage,
        already_imported: Option<u64>,
    ) -> Result<Snapshot, ProtocolError> {
        if message.generation != palw_row_stream_v1::PALW_ROW_STREAM_CURRENT_GENERATION_V1 {
            return Err(ProtocolError::Other("PALW row stream generation was not requested"));
        }
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
            .map_err(|why| ProtocolError::LocalNativeResourceUnavailable(format!("PALW row stream has no local witness header: {why}")))?;
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
        let (snapshot, lease) = decode_row_snapshot_with_native_lease_v1(point, accepted.import_suffix())
            .map_err(super::palw_snapshot_chunks::snapshot_failure)?;
        self.native_palw_receive_lease_v1 = lease;
        // This is still staging data. Rechecking the preimage hash is NOT the whole-state
        // import gate and must never mint a PalwWholeStateVerifiedV1 token. The caller
        // installs this only via import_pruning_point_palw_snapshot_for_session.
        Ok(snapshot)
    }
}
