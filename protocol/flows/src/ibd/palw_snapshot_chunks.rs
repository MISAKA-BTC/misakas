//! Protocol-106 requester. Builds a bounded private spill before any local consensus
//! mutation; native import verifies the completed carriage under the pruning header.
use super::IbdFlow;
use crate::palw_snapshot_chunks::{
    NativeSnapshotDecodeErrorV1, SNAPSHOT_CODEC_ROWS_V1, SNAPSHOT_CODEC_ZLIB_V1, Snapshot, SnapshotCompressionV1, SnapshotReceiver,
    SnapshotSpill, park_interrupted_snapshot, take_interrupted_snapshot,
};
use kaspa_consensus_core::{BlockHash, Hash64};
use kaspa_consensusmanager::ConsensusProxy;
use kaspa_p2p_lib::{
    common::ProtocolError,
    make_message,
    pb::{RequestPruningPointPalwStateMessage, kaspad_message::Payload},
};
use std::time::Duration;

fn hash(bytes: &[u8]) -> Result<Hash64, ProtocolError> {
    Ok(Hash64::from_bytes(bytes.try_into().map_err(|_| ProtocolError::Other("invalid PALW snapshot hash width"))?))
}

pub(super) fn snapshot_failure(why: NativeSnapshotDecodeErrorV1) -> ProtocolError {
    match why {
        NativeSnapshotDecodeErrorV1::Capacity(unavailable) => ProtocolError::LocalNativeCapacityUnavailable(unavailable),
        NativeSnapshotDecodeErrorV1::LocalStorage(why) => {
            ProtocolError::LocalNativeResourceUnavailable(format!("PALW snapshot spill: {why}"))
        }
        NativeSnapshotDecodeErrorV1::Encoding(why) => {
            kaspa_core::warn!("[palw-sidecar] structural snapshot decode refused: {why}");
            ProtocolError::Other("PALW snapshot stream hash/Borsh/EOF failed")
        }
    }
}

// An unavailable response has no authenticated stream identity. Require the
// explicit empty chunked sentinel emitted by the server before allowing the
// one-time legacy fallback; malformed stream replies must never downgrade.
fn explicit_chunk_unavailable_v1(msg: &kaspa_p2p_lib::pb::PruningPointPalwStateMessage) -> bool {
    !msg.found
        && msg.chunked
        && msg.palw_state.is_empty()
        && msg.class_carriages.is_empty()
        && msg.pruning_point_hash.is_none()
        && msg.snapshot_root.is_empty()
        && msg.chunk_index == 0
        && msg.chunk_count == 0
        && msg.total_bytes == 0
        && msg.chunk_hash.is_empty()
        && msg.snapshot_codec == 0
        && msg.snapshot_nonce.is_empty()
        && msg.decoded_bytes == 0
}

fn row_fallback_allowed_v1(codec: u32, result: &Result<Option<Snapshot>, ProtocolError>) -> bool {
    codec == SNAPSHOT_CODEC_ROWS_V1 && matches!(result, Err(ProtocolError::PruningSidecarUnavailable("PALW chunked state", _)))
}
fn fallback_nonce_v1(previous: &[u8]) -> Vec<u8> {
    loop {
        let nonce = rand::random::<[u8; 32]>();
        if nonce != [0; 32] && nonce.as_slice() != previous {
            return nonce.to_vec();
        }
    }
}

impl IbdFlow {
    pub(super) async fn request_pruning_point_palw_chunks(
        &mut self,
        consensus: &ConsensusProxy,
        point: BlockHash,
    ) -> Result<Option<Snapshot>, ProtocolError> {
        if self.router.properties().protocol_version < crate::flow_context::PROTOCOL_VERSION_PALW_SNAPSHOT_CHUNKS {
            return Err(ProtocolError::Other("PALW snapshot chunks were not negotiated"));
        }
        let mut receiver = take_interrupted_snapshot(point);
        let compressed = self.router.properties().protocol_version >= crate::flow_context::PROTOCOL_VERSION_PALW_SNAPSHOT_COMPRESSED;
        let expected_row_root = if crate::flow_context::palw_row_stream_negotiated_v1(self.router.properties().protocol_version) {
            consensus.clone().spawn_blocking(move |api| api.pruning_point_palw_row_witness_root_v1(point)).await
        } else {
            None
        };
        // A negotiated109 peer may still have a non-flat witness mode. Preserve
        // the existing107 import rather than interpreting another header root.
        let codec = if expected_row_root.is_some() {
            SNAPSHOT_CODEC_ROWS_V1
        } else if compressed {
            SNAPSHOT_CODEC_ZLIB_V1
        } else {
            0
        };
        // Negotiated peer changes may require a fresh transfer. A parked stream's
        // raw/compressed identity is never reinterpreted under another version.
        if receiver
            .as_ref()
            .is_some_and(|state| state.profile().map_or(0, |p| p.codec) != codec || state.row_expected_root() != expected_row_root)
        {
            receiver = None;
        }
        let nonce = if compressed {
            receiver.as_ref().and_then(SnapshotReceiver::profile).map_or_else(rand::random::<[u8; 32]>, |p| p.nonce).to_vec()
        } else {
            Vec::new()
        };
        // A parked receiver already owns its permit. Otherwise reserve before wire
        // work; cancellation and malformed replies immediately release this local lease.
        let mut spill = if receiver.is_none() {
            Some(
                SnapshotSpill::new()
                    .map_err(|why| ProtocolError::LocalNativeResourceUnavailable(format!("PALW snapshot spill/session: {why}")))?,
            )
        } else {
            None
        };
        let mut result = self
            .receive_pruning_point_palw_chunks(consensus, point, &mut receiver, &mut spill, &nonce, codec, expected_row_root)
            .await;
        if row_fallback_allowed_v1(codec, &result) {
            // The row generation may exceed its unchanged384MiB cap while
            // the existing compressed carriage still fits. Start one fresh107
            // transfer on this peer; never reinterpret a partial row stream.
            drop(receiver.take());
            drop(spill.take());
            spill = Some(
                SnapshotSpill::new()
                    .map_err(|why| ProtocolError::LocalNativeResourceUnavailable(format!("PALW fallback spill/session: {why}")))?,
            );
            let fresh = fallback_nonce_v1(&nonce);
            result = self
                .receive_pruning_point_palw_chunks(consensus, point, &mut receiver, &mut spill, &fresh, SNAPSHOT_CODEC_ZLIB_V1, None)
                .await;
        }
        if matches!(&result, Err(ProtocolError::Timeout(_)) | Err(ProtocolError::ConnectionClosed))
            && let Some(receiver) = receiver.take()
        {
            park_interrupted_snapshot(receiver);
        }
        result
    }

    async fn receive_pruning_point_palw_chunks(
        &mut self,
        consensus: &ConsensusProxy,
        point: BlockHash,
        receiver: &mut Option<SnapshotReceiver>,
        spill: &mut Option<SnapshotSpill>,
        nonce: &[u8],
        codec: u32,
        expected_row_root: Option<Hash64>,
    ) -> Result<Option<Snapshot>, ProtocolError> {
        let mut root = receiver.as_ref().map(SnapshotReceiver::root);
        let start_index = receiver.as_ref().map_or(0, SnapshotReceiver::next_index);
        let max_chunks = crate::palw_gossip::PALW_PRUNING_STATE_MAX_BYTES.div_ceil(crate::palw_snapshot_chunks::CHUNK_BYTES);
        for index in start_index..max_chunks as u32 {
            self.router
                .enqueue(make_message!(
                    Payload::RequestPruningPointPalwState,
                    RequestPruningPointPalwStateMessage {
                        pruning_point_hash: Some(point.into()),
                        chunked: true,
                        snapshot_root: root.map(|h: Hash64| h.as_byte_slice().to_vec()).unwrap_or_default(),
                        chunk_index: index,
                        snapshot_codec: codec,
                        snapshot_nonce: nonce.to_vec(),
                        row_stream_generation: if codec == SNAPSHOT_CODEC_ROWS_V1 { 1 } else { 0 },
                    }
                ))
                .await?;
            // A stalled continuation may be retried on another peer while the original
            // immutable receiver is still within its total 600-second lease.
            let timeout = Duration::from_secs(if index == 0 { 600 } else { 30 });
            let payload = match tokio::time::timeout(timeout, self.incoming_route.recv()).await {
                Ok(Some(message)) => message.payload,
                Ok(None) => return Err(ProtocolError::ConnectionClosed),
                Err(_) => return Err(ProtocolError::Timeout(timeout)),
            };
            let msg = match payload {
                Some(Payload::PruningPointPalwRowStream(_)) => {
                    return Err(ProtocolError::Other("row generations require bounded codec2 chunk replies"));
                }
                Some(Payload::PruningPointPalwState(msg)) => msg,
                other => {
                    return Err(ProtocolError::UnexpectedMessage(
                        "PruningPointPalwState or PruningPointPalwRowStream",
                        other.as_ref().map(Into::into),
                    ));
                }
            };
            if !msg.found {
                if !explicit_chunk_unavailable_v1(&msg) {
                    return Err(ProtocolError::Other("malformed PALW snapshot unavailable reply"));
                }
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
            let profile = SnapshotCompressionV1::from_wire(msg.snapshot_codec, &msg.snapshot_nonce, msg.decoded_bytes)
                .map_err(|_| ProtocolError::Other("invalid PALW snapshot compression profile"))?;
            let expects_compression = !nonce.is_empty();
            if profile.is_some() != expects_compression || profile.is_some_and(|p| p.nonce.as_slice() != nonce || p.codec != codec) {
                return Err(ProtocolError::Other("PALW snapshot mode/nonce changed from negotiated request"));
            }
            if root.is_none() {
                let mut state = match profile {
                    Some(profile) => SnapshotReceiver::new_compressed(
                        spill.take().unwrap(),
                        point,
                        reply_root,
                        msg.total_bytes,
                        msg.chunk_count,
                        profile,
                    ),
                    None => SnapshotReceiver::new(spill.take().unwrap(), point, reply_root, msg.total_bytes, msg.chunk_count),
                }
                .map_err(|_| ProtocolError::Other("PALW snapshot total/count exceeds existing policy"))?;
                if let Some(expected) = expected_row_root {
                    state.enable_row_verification(expected).map_err(snapshot_failure)?;
                }
                *receiver = Some(state);
                root = Some(reply_root);
            }
            if receiver.as_ref().unwrap().profile() != profile {
                return Err(ProtocolError::Other("PALW snapshot compression profile changed during stream"));
            }
            let mut state = receiver.take().unwrap();
            let bytes = msg.palw_state;
            let state = kaspa_consensusmanager::spawn_blocking(move || {
                state.accept(point, reply_root, msg.total_bytes, msg.chunk_count, index, chunk_hash, &bytes)?;
                Ok::<_, NativeSnapshotDecodeErrorV1>(state)
            })
            .await
            .map_err(|why| ProtocolError::LocalNativeResourceUnavailable(format!("PALW chunk verification worker stopped: {why}")))?
            .map_err(snapshot_failure)?;
            let complete = state.complete();
            *receiver = Some(state);
            if complete {
                break;
            }
        }
        let receiver = receiver.take().ok_or(ProtocolError::Other("empty PALW chunked snapshot"))?;
        if !receiver.complete() {
            return Err(ProtocolError::Other("PALW chunked snapshot ended early"));
        }
        let expected_root = expected_row_root;
        let (snapshot, native_lease, inventory) = kaspa_consensusmanager::spawn_blocking(move || match expected_root {
            Some(root) => receiver
                .decode_row_generation_with_native_lease(root)
                .map(|(snapshot, lease, inventory)| (snapshot, lease, Some(inventory))),
            None => receiver.decode_snapshot_with_native_lease().map(|(snapshot, lease)| (snapshot, lease, None)),
        })
        .await
        .map_err(|why| ProtocolError::LocalNativeResourceUnavailable(format!("PALW snapshot decoding worker stopped: {why}")))?
        .map_err(snapshot_failure)?;
        let (snapshot, native_lease) = if let Some(inventory) = inventory {
            let row_root = inventory.state_root();
            let collection_count = inventory.collection_count();
            let checked = consensus
                .clone()
                .spawn_blocking(move |api| {
                    api.palw_validate_row_stream_snapshot_v1(point, &snapshot.0, &inventory)?;
                    Ok::<_, kaspa_consensus_core::palw_native_resource_v1::NativeDecodeErrorV1<String>>((snapshot, native_lease))
                })
                .await
                .map_err(|error| match error {
                    kaspa_consensus_core::palw_native_resource_v1::NativeDecodeErrorV1::Resource(why) => {
                        ProtocolError::LocalNativeCapacityUnavailable(why)
                    }
                    kaspa_consensus_core::palw_native_resource_v1::NativeDecodeErrorV1::Decode(why) => {
                        ProtocolError::OtherOwned(format!("PALW row inventory differs from complete native state: {why}"))
                    }
                })?;
            Self::record_authenticated_row_snapshot_v1(point, row_root, collection_count);
            checked
        } else {
            (snapshot, native_lease)
        };
        self.native_palw_receive_lease_v1 = native_lease;
        // Declarations were length checked and filtered during streaming decode. Native
        // import/adoption still validates the consensus state and each declaration's profile.
        Ok(Some(snapshot))
    }
}

#[cfg(test)]
mod row_fallback_tests {
    use super::*;
    #[test]
    fn rfc37_malformed_unavailable_reply_cannot_trigger_a_codec_downgrade() {
        use kaspa_p2p_lib::pb::PruningPointPalwStateMessage;
        let unavailable = || PruningPointPalwStateMessage { chunked: true, ..Default::default() };
        assert!(explicit_chunk_unavailable_v1(&unavailable()));
        let mut malformed = Vec::new();
        let mut reply = unavailable();
        reply.chunked = false;
        malformed.push(reply);
        let mut reply = unavailable();
        reply.found = true;
        malformed.push(reply);
        let mut reply = unavailable();
        reply.palw_state = vec![1];
        malformed.push(reply);
        let mut reply = unavailable();
        reply.class_carriages.push(kaspa_p2p_lib::pb::PalwClassCarriageEntry::default());
        malformed.push(reply);
        let mut reply = unavailable();
        reply.pruning_point_hash = Some(kaspa_consensus_core::Hash64::from_u64_word(1).into());
        malformed.push(reply);
        let mut reply = unavailable();
        reply.snapshot_root = vec![1; 64];
        malformed.push(reply);
        let mut reply = unavailable();
        reply.chunk_index = 1;
        malformed.push(reply);
        let mut reply = unavailable();
        reply.chunk_count = 1;
        malformed.push(reply);
        let mut reply = unavailable();
        reply.total_bytes = 1;
        malformed.push(reply);
        let mut reply = unavailable();
        reply.chunk_hash = vec![1; 64];
        malformed.push(reply);
        let mut reply = unavailable();
        reply.snapshot_codec = SNAPSHOT_CODEC_ROWS_V1;
        malformed.push(reply);
        let mut reply = unavailable();
        reply.snapshot_nonce = vec![1; 32];
        malformed.push(reply);
        let mut reply = unavailable();
        reply.decoded_bytes = 1;
        malformed.push(reply);
        for reply in malformed {
            assert!(!explicit_chunk_unavailable_v1(&reply), "{reply:?}");
        }
    }

    #[test]
    fn rfc37_fallback_is_once_for_explicit_unavailability_and_never_authentication_failure() {
        let unavailable = Err(ProtocolError::PruningSidecarUnavailable("PALW chunked state", "point".into()));
        assert!(row_fallback_allowed_v1(SNAPSHOT_CODEC_ROWS_V1, &unavailable));
        assert!(!row_fallback_allowed_v1(SNAPSHOT_CODEC_ZLIB_V1, &unavailable));
        for error in [
            ProtocolError::Other("bad row root"),
            ProtocolError::ConnectionClosed,
            ProtocolError::LocalNativeResourceUnavailable("capacity".into()),
        ] {
            assert!(!row_fallback_allowed_v1(SNAPSHOT_CODEC_ROWS_V1, &Err(error)));
        }
        let nonce = fallback_nonce_v1(&[7; 32]);
        assert_eq!(nonce.len(), 32);
        assert_ne!(nonce, [7; 32]);
        assert_ne!(nonce, [0; 32]);
    }
}
