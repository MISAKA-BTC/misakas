//! Protocol-106 requester. Builds a bounded private spill before any local consensus
//! mutation; native import verifies the completed carriage under the pruning header.
use super::IbdFlow;
use crate::palw_snapshot_chunks::{Snapshot, SnapshotReceiver, SnapshotSpill};
use kaspa_consensus_core::{BlockHash, Hash64};
use kaspa_p2p_lib::{
    common::ProtocolError,
    dequeue_with_timeout, make_message,
    pb::{RequestPruningPointPalwStateMessage, kaspad_message::Payload},
};
use std::time::Duration;

fn hash(bytes: &[u8]) -> Result<Hash64, ProtocolError> {
    Ok(Hash64::from_bytes(bytes.try_into().map_err(|_| ProtocolError::Other("invalid PALW snapshot hash width"))?))
}

impl IbdFlow {
    pub(super) async fn request_pruning_point_palw_chunks(&mut self, point: BlockHash) -> Result<Option<Snapshot>, ProtocolError> {
        if self.router.properties().protocol_version < crate::flow_context::PROTOCOL_VERSION_PALW_SNAPSHOT_CHUNKS {
            return Err(ProtocolError::Other("PALW snapshot chunks were not negotiated"));
        }
        // The permit is acquired BEFORE requesting bytes. Cancellation or any refusal drops
        // the file and returns the shared session slot; no stale file is trusted on a retry.
        let mut spill = Some(
            SnapshotSpill::new()
                .map_err(|_| ProtocolError::PruningSidecarUnavailable("PALW snapshot session budget", point.to_string()))?,
        );
        let mut receiver: Option<SnapshotReceiver> = None;
        let mut root = None;
        let max_chunks = crate::palw_gossip::PALW_PRUNING_STATE_MAX_BYTES.div_ceil(crate::palw_snapshot_chunks::CHUNK_BYTES);
        for index in 0..max_chunks as u32 {
            self.router
                .enqueue(make_message!(
                    Payload::RequestPruningPointPalwState,
                    RequestPruningPointPalwStateMessage {
                        pruning_point_hash: Some(point.into()),
                        chunked: true,
                        snapshot_root: root.map(|h: Hash64| h.as_byte_slice().to_vec()).unwrap_or_default(),
                        chunk_index: index,
                    }
                ))
                .await?;
            let msg = dequeue_with_timeout!(self.incoming_route, Payload::PruningPointPalwState, Duration::from_secs(600))?;
            if !msg.found {
                return Err(ProtocolError::PruningSidecarUnavailable("PALW chunked state", point.to_string()));
            }
            if !msg.chunked || !msg.class_carriages.is_empty() || msg.chunk_index != index {
                return Err(ProtocolError::Other("invalid PALW chunked reply mode/order"));
            }
            let reply_point =
                BlockHash::try_from(msg.pruning_point_hash.ok_or(ProtocolError::Other("PALW chunk lacks pruning point"))?)
                    .map_err(|_| ProtocolError::Other("invalid PALW chunk pruning point"))?;
            if reply_point != point {
                return Err(ProtocolError::Other("PALW chunk changed pruning point"));
            }
            let reply_root = hash(&msg.snapshot_root)?;
            let chunk_hash = hash(&msg.chunk_hash)?;
            if root.is_none() {
                let state = SnapshotReceiver::new(spill.take().unwrap(), point, reply_root, msg.total_bytes, msg.chunk_count)
                    .map_err(|_| ProtocolError::Other("PALW snapshot total/count exceeds existing policy"))?;
                receiver = Some(state);
                root = Some(reply_root);
            }
            let state = receiver.as_mut().unwrap();
            state
                .accept(point, reply_root, msg.total_bytes, msg.chunk_count, index, chunk_hash, &msg.palw_state)
                .map_err(|_| ProtocolError::Other("PALW snapshot chunk identity/order/size/hash failed"))?;
            if state.complete() {
                break;
            }
        }
        let receiver = receiver.ok_or(ProtocolError::Other("empty PALW chunked snapshot"))?;
        if !receiver.complete() {
            return Err(ProtocolError::Other("PALW chunked snapshot ended early"));
        }
        let snapshot: Snapshot = kaspa_consensusmanager::spawn_blocking(move || receiver.decode_snapshot())
            .await
            .map_err(|_| ProtocolError::Other("PALW snapshot decoding task failed"))?
            .map_err(|_| ProtocolError::Other("PALW snapshot stream hash/Borsh/EOF failed"))?;
        // Declarations were length checked and filtered during streaming decode. Native
        // import/adoption still validates the consensus state and each declaration's profile.
        Ok(Some(snapshot))
    }
}
