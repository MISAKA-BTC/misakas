use super::*;
use http_body_util::BodyExt;
use prost::Message;
use tonic::codec::{Codec, EncodeBody};

#[tokio::test]
async fn snapshot_ticket_keeps_its_class_without_reviving_pending_bytes() {
    let snapshot_bank = memory_budget::MemoryBudget::new(4 << 20, [4 << 20; 4]);
    let snapshot = snapshot_bank.try_reserve(memory_budget::MemoryClass::Snapshot, 2 << 20).unwrap();
    let message = p2p::KaspadMessage {
        payload: Some(p2p::kaspad_message::Payload::PruningPointPalwState(p2p::PruningPointPalwStateMessage {
            found: true,
            palw_state: vec![19; 1 << 20],
            ..Default::default()
        })),
        ..Default::default()
    };
    snapshot.materialized();
    let local = transport::connection_budget();
    let queued = transport::Outbound::with_preallocated(message, &local, snapshot, Vec::new()).unwrap();
    let expected = snapshot_bank.snapshot().reserved_bytes;
    assert!(expected < 2 << 20);
    assert_eq!(snapshot_bank.snapshot().pending_bytes, 0);
    assert_eq!(snapshot_bank.snapshot().class_bytes[memory_budget::MemoryClass::Snapshot as usize], expected);
    let mut codec = transport::BudgetedProstCodec::<p2p::KaspadMessage, p2p::KaspadMessage>::default();
    let mut body = EncodeBody::new_client(
        codec.encoder(),
        tokio_stream::iter([Ok(queued)]),
        Some(tonic::codec::CompressionEncoding::Gzip),
        Some(320 << 20),
    );
    let bytes = body.frame().await.unwrap().unwrap().into_data().unwrap();
    let last_owner = bytes.slice(1..);
    drop(body);
    drop(bytes);
    assert_eq!(snapshot_bank.snapshot().reserved_bytes, expected);
    assert_eq!(snapshot_bank.snapshot().pending_bytes, 0);
    drop(last_owner);
    assert_eq!(snapshot_bank.snapshot().reserved_bytes, 0);
    assert_eq!(local.snapshot().reserved_bytes, 0);
}

#[tokio::test]
#[ignore = "real 256/320 MiB frames; run alone with --ignored --test-threads=1"]
async fn legacy_evm_256mib_and_p2p_320mib_envelopes_remain_usable() {
    for payload_bytes in [256 << 20, (320 << 20) - 1024] {
        for gzip in [false, true] {
            let message = p2p::KaspadMessage {
                payload: Some(p2p::kaspad_message::Payload::PruningPointEvmState(p2p::PruningPointEvmStateMessage {
                    found: true,
                    evm_state_snapshot: vec![43; payload_bytes],
                    ..Default::default()
                })),
                ..Default::default()
            };
            let wire_size = message.encoded_len();
            assert!(wire_size <= 320 << 20);
            let local = transport::connection_budget();
            let queued = transport::Outbound::try_new(message, &local).unwrap();
            let admitted = local.snapshot().reserved_bytes;
            assert!(local.snapshot().pending_bytes > 0);
            let mut codec = transport::BudgetedProstCodec::<p2p::KaspadMessage, p2p::KaspadMessage>::default();
            let mut body = EncodeBody::new_client(
                codec.encoder(),
                tokio_stream::iter([Ok(queued)]),
                gzip.then_some(tonic::codec::CompressionEncoding::Gzip),
                Some(320 << 20),
            );
            let frame = body.frame().await.unwrap().unwrap().into_data().unwrap();
            let retained_peak = local.snapshot().reserved_bytes;
            assert_eq!(retained_peak, admitted);
            assert_eq!(local.snapshot().pending_bytes, 0);
            assert!(retained_peak < 1536 << 20);
            assert_eq!(frame[0], u8::from(gzip));
            let frame_bytes = frame.len();
            drop(body);
            let mut receive_codec = transport::BudgetedProstCodec::<p2p::KaspadMessage, p2p::KaspadMessage>::default();
            let mut receive = tonic::Streaming::new_request(
                receive_codec.decoder(),
                http_body_util::Full::new(frame),
                gzip.then_some(tonic::codec::CompressionEncoding::Gzip),
                Some(320 << 20),
            );
            let decoded = receive.message().await.unwrap().unwrap();
            let Some(p2p::kaspad_message::Payload::PruningPointEvmState(snapshot)) = decoded.payload else {
                panic!("wrong decoded payload")
            };
            assert_eq!(snapshot.evm_state_snapshot.len(), payload_bytes);
            assert!(snapshot.evm_state_snapshot.iter().all(|v| *v == 43));
            drop(snapshot);
            drop(receive);
            assert_eq!(local.snapshot().reserved_bytes, 0);
            eprintln!(
                "payload_bytes={payload_bytes} gzip={gzip} wire_bytes={frame_bytes} accounted_simultaneous_peak={retained_peak} final_reserved=0"
            );
        }
    }
}
