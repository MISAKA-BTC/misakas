//! Real loopback P2P routing over tonic, using the production snapshot exporter,
//! packet encoder, incremental receiver and final native import. The queued-row
//! population is a codec fixture, not a live chain/model or deployment campaign.
use super::*;
use kaspa_consensus_core::{
    api::ConsensusApi,
    palw_state_v2::{PalwChainStateV2, PalwPayoutV2, PalwStateParamsV2},
};
use kaspa_p2p_lib::{
    Adaptor, ConnectionInitializer, Hub, IncomingRoute, KaspadHandshake, KaspadMessagePayloadType, Router,
    common::ProtocolError,
    make_message,
    pb::{RequestPruningPointPalwStateMessage, VersionMessage, kaspad_message::Payload},
};
use std::sync::{Arc, Mutex};

struct Source {
    state: PalwChainStateV2,
}
impl ConsensusApi for Source {
    fn write_pruning_point_palw_row_generation_v1(
        &self,
        _point: Hash64,
        suffix: &mut dyn Read,
        suffix_len: u64,
        writer: &mut dyn Write,
    ) -> io::Result<Option<u64>> {
        kaspa_consensus_core::palw_direct_row_export_v1::write_row_generation_stream_v1(&self.state, suffix, suffix_len, writer)
            .map(Some)
    }
}
struct Initializer {
    sender: Option<Arc<Mutex<SnapshotSender>>>,
    client: Option<tokio::sync::mpsc::UnboundedSender<(Arc<Router>, IncomingRoute)>>,
}
#[async_trait::async_trait]
impl ConnectionInitializer for Initializer {
    async fn initialize_connection(&self, router: Arc<Router>) -> Result<(), ProtocolError> {
        let mut handshake = KaspadHandshake::new(&router);
        router.start();
        let version = handshake
            .handshake(VersionMessage {
                protocol_version: 109,
                id: uuid::Uuid::new_v4().as_bytes().to_vec(),
                network: "rfc37-loopback".into(),
                ..Default::default()
            })
            .await?;
        router.set_identity(kaspa_utils::networking::PeerId::from_slice(&version.id).unwrap());
        if !crate::flow_context::palw_row_stream_negotiated_v1(version.protocol_version) {
            return Err(ProtocolError::Other("row dialect not negotiated"));
        }
        let mut incoming = router.subscribe(vec![if self.sender.is_some() {
            KaspadMessagePayloadType::RequestPruningPointPalwState
        } else {
            KaspadMessagePayloadType::PruningPointPalwState
        }]);
        handshake.exchange_ready_messages().await?;
        if let Some(sender) = self.sender.clone() {
            tokio::spawn(async move {
                while let Some(message) = incoming.recv().await {
                    let Some(Payload::RequestPruningPointPalwState(request)) = message.payload else { break };
                    assert_eq!((request.snapshot_codec, request.row_stream_generation), (SNAPSHOT_CODEC_ROWS_V1, 1));
                    let point = Hash64::try_from(request.pruning_point_hash.unwrap()).unwrap();
                    let root = if request.snapshot_root.is_empty() {
                        None
                    } else {
                        Some(Hash64::from_bytes(request.snapshot_root.try_into().unwrap()))
                    };
                    let (reply, _chunk_lease) = {
                        let mut sender = sender.lock().unwrap();
                        assert_eq!(sender.profile.unwrap().nonce.as_slice(), request.snapshot_nonce.as_slice());
                        sender.reply_message(point, root, request.chunk_index).unwrap()
                    };
                    router.enqueue(make_message!(Payload::PruningPointPalwState, reply)).await.unwrap();
                }
            });
        } else {
            self.client.as_ref().unwrap().send((router, incoming)).map_err(|_| ProtocolError::Other("test receiver closed"))?;
        }
        Ok(())
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rfc37_real_peer_serves_bounded_chunks_and_imports_the_same_native_state() {
    let params = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, Hash64::from_u64_word(1), 4, 1000, 100, 1000, 0).unwrap();
    let mut carriage = PalwStateCarriageV2::from_state(&PalwChainStateV2::genesis());
    for i in 1..=35_000u64 {
        carriage
            .pending_payouts
            .insert(Hash64::from_u64_word(i), PalwPayoutV2 { payload: Hash64::from_u64_word(i + 100_000), amount: 1 });
    }
    let state = carriage.clone().into_state(&params, None).unwrap();
    let expected_root = state.state_root();
    let source = Source { state };
    let point = Hash64::from_u64_word(900);
    let nonce = [31; 32];
    let sender = SnapshotSender::encode_row_snapshot(SnapshotSpill::new().unwrap(), point, &carriage, &[], nonce, &source).unwrap();
    assert!(sender.count >= 2, "fixture must cross real4MiB packet boundaries");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let server = Adaptor::bidirectional(
        address.to_string().parse().unwrap(),
        Hub::new(),
        Arc::new(Initializer { sender: Some(Arc::new(Mutex::new(sender))), client: None }),
        Default::default(),
    )
    .unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let client = Adaptor::client_only(Hub::new(), Arc::new(Initializer { sender: None, client: Some(tx) }), Default::default());
    client.connect_peer_with_retries(address.to_string(), 10, Duration::from_millis(20)).await.unwrap();
    let (router, mut incoming) = tokio::time::timeout(Duration::from_secs(10), rx.recv()).await.unwrap().unwrap();
    let mut receiver: Option<SnapshotReceiver> = None;
    let mut root = None;
    for index in 0..96u32 {
        router
            .enqueue(make_message!(
                Payload::RequestPruningPointPalwState,
                RequestPruningPointPalwStateMessage {
                    pruning_point_hash: Some(point.into()),
                    chunked: true,
                    snapshot_root: root.map_or_else(Vec::new, |h: Hash64| h.as_byte_slice().to_vec()),
                    chunk_index: index,
                    snapshot_codec: SNAPSHOT_CODEC_ROWS_V1,
                    snapshot_nonce: nonce.to_vec(),
                    row_stream_generation: 1,
                }
            ))
            .await
            .unwrap();
        let message = tokio::time::timeout(Duration::from_secs(20), incoming.recv()).await.unwrap().unwrap();
        let Some(Payload::PruningPointPalwState(reply)) = message.payload else { panic!("wrong reply") };
        assert!(reply.palw_state.len() <= CHUNK_BYTES);
        let next_root = Hash64::from_bytes(reply.snapshot_root.try_into().unwrap());
        let profile =
            SnapshotCompressionV1::from_wire(reply.snapshot_codec, &reply.snapshot_nonce, reply.decoded_bytes).unwrap().unwrap();
        if receiver.is_none() {
            let mut r = SnapshotReceiver::new_compressed(
                SnapshotSpill::new().unwrap(),
                point,
                next_root,
                reply.total_bytes,
                reply.chunk_count,
                profile,
            )
            .unwrap();
            r.enable_row_verification(expected_root).unwrap();
            receiver = Some(r);
            root = Some(next_root);
        }
        let r = receiver.as_mut().unwrap();
        r.accept(
            point,
            next_root,
            reply.total_bytes,
            reply.chunk_count,
            index,
            Hash64::from_bytes(reply.chunk_hash.try_into().unwrap()),
            &reply.palw_state,
        )
        .unwrap();
        if r.complete() {
            break;
        }
    }
    let receiver = receiver.unwrap();
    assert!(receiver.complete());
    let ((restored, declarations), _lease, inventory) = receiver.decode_row_generation_with_native_lease(expected_root).unwrap();
    assert!(inventory.matches_state(&source.state).unwrap());
    assert!(declarations.is_empty());
    assert_eq!(restored.into_state(&params, Some(expected_root)).unwrap(), source.state);
    client.close().await;
    server.close().await;
}
