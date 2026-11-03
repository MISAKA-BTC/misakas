use super::*;
use http_body_util::BodyExt;
use prost::Message;
use tonic::codec::{Codec, EncodeBody};

#[test]
fn utxo_conversion_quote_covers_actual_generated_protobuf_capacities() {
    use transport::RetainedSize;
    for (entries, address_payload, script_bytes) in [(0usize, 0usize, 0usize), (1, 64, 74), (4097, 64, 74), (3, 256, 8192)] {
        // Match the production shape: owned address, Hash64 hex, script hex,
        // nested optional outpoint/utxo/script structs, and no verbose entry.
        let mut rows = Vec::with_capacity(if entries == 0 { 0 } else { entries.next_power_of_two() });
        for index in 0..entries {
            rows.push(rpc::RpcUtxosByAddressesEntry {
                address: "a".repeat(2 * address_payload + 64),
                outpoint: Some(rpc::RpcOutpoint { transaction_id: "b".repeat(128), index: index as u32 }),
                utxo_entry: Some(rpc::RpcUtxoEntry {
                    amount: 1,
                    script_public_key: Some(rpc::RpcScriptPublicKey { version: 0, script_public_key: "c".repeat(2 * script_bytes) }),
                    ..Default::default()
                }),
            });
        }
        let message = rpc::KaspadResponse {
            id: 7,
            payload: Some(rpc::kaspad_response::Payload::GetUtxosByAddressesResponse(rpc::GetUtxosByAddressesResponseMessage {
                entries: rows,
                error: None,
            })),
        };
        let actual_owned = (std::mem::size_of::<rpc::KaspadResponse>() as u64) + message.retained_heap_bytes() + 128;
        let quote = utxo_memory::utxo_proto_reservation_bytes(entries as u64, address_payload as u64, script_bytes as u64).unwrap();
        assert!(
            actual_owned <= quote,
            "entries={entries} address={address_payload} script={script_bytes} quote={quote} actual={actual_owned}"
        );
    }
}

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
            if !gzip {
                use crate::wire_preflight::WirePreflight;
                let schema = p2p::KaspadMessage::wire_heap_bound(&frame[5..], 0).unwrap();
                assert_eq!(schema.retained_bytes, payload_bytes as u64);
                assert_eq!(schema.peak_bytes, payload_bytes as u64);
            }
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

#[tokio::test]
async fn evm_and_overlay_sidecar_borsh_admission_and_transport_owner() {
    use memory_budget::{MemoryBudget, MemoryClass};
    use p2p::kaspad_message::Payload;
    use snapshot_memory::{
        MAX_EVM_HEADER_BYTES, MAX_EVM_STATE_SNAPSHOT_BYTES, MAX_OVERLAY_SNAPSHOT_BYTES, encode_exact, encode_sidecar, encoded_size,
    };
    use std::mem::size_of;

    let bank = MemoryBudget::new(4 << 20, [4 << 20; 4]);
    let header = [7u64, 18];
    let state = vec![23u8; 1 << 17];
    let expected_header = borsh::to_vec(&header).unwrap();
    let expected_state = borsh::to_vec(&state).unwrap();
    let header_len = encoded_size(&header, MAX_EVM_HEADER_BYTES as u64).unwrap();
    let state_len = encoded_size(&state, MAX_EVM_STATE_SNAPSHOT_BYTES as u64).unwrap();
    assert_eq!(encoded_size(&state, MAX_OVERLAY_SNAPSHOT_BYTES as u64).unwrap(), state_len);
    assert!(encoded_size(&state, state_len - 1).is_err());
    let inline = size_of::<p2p::KaspadMessage>();

    for evm in [true, false] {
        let lengths = if evm { vec![header_len, state_len] } else { vec![state_len] };
        let charged = inline as u64 + 128 + lengths.iter().sum::<u64>();
        let blocked = bank.try_reserve_retained(MemoryClass::Snapshot, (4 << 20) - charged + 1).unwrap();
        assert_eq!(
            encode_sidecar::<()>(&bank, inline, &lengths, || panic!("denied admission ran the encoder")).unwrap_err().kind(),
            std::io::ErrorKind::OutOfMemory
        );
        drop(blocked);

        let (message, lease) = encode_sidecar(&bank, inline, &lengths, || {
            assert_eq!(bank.snapshot().pending_bytes, charged);
            let payload = if evm {
                let evm_header = encode_exact(&header, header_len)?;
                assert_eq!(evm_header, expected_header);
                let evm_state_snapshot = encode_exact(&state, state_len)?;
                assert_eq!(evm_state_snapshot, expected_state);
                Payload::PruningPointEvmState(p2p::PruningPointEvmStateMessage { found: true, evm_header, evm_state_snapshot })
            } else {
                let overlay_snapshot = encode_exact(&state, state_len)?;
                assert_eq!(overlay_snapshot, expected_state);
                Payload::PruningPointOverlaySnapshot(p2p::PruningPointOverlaySnapshotMessage { found: true, overlay_snapshot })
            };
            Ok(p2p::KaspadMessage { payload: Some(payload), ..Default::default() })
        })
        .unwrap();
        assert_eq!(lease.bytes(), charged);
        assert_eq!(bank.snapshot().pending_bytes, 0);

        let local = transport::connection_budget();
        let queued = transport::Outbound::with_preallocated(message, &local, lease, Vec::new()).unwrap();
        let mut codec = transport::BudgetedProstCodec::<p2p::KaspadMessage, p2p::KaspadMessage>::default();
        let mut body = EncodeBody::new_client(codec.encoder(), tokio_stream::iter([Ok(queued)]), None, Some(320 << 20));
        let frame = body.frame().await.unwrap().unwrap().into_data().unwrap();
        let alias = frame.slice(5..);
        let decoded = p2p::KaspadMessage::decode(alias.as_ref()).unwrap();
        match decoded.payload.unwrap() {
            Payload::PruningPointEvmState(reply) if evm => {
                assert!(reply.found);
                assert_eq!(reply.evm_header, expected_header);
                assert_eq!(reply.evm_state_snapshot, expected_state);
            }
            Payload::PruningPointOverlaySnapshot(reply) if !evm => {
                assert!(reply.found);
                assert_eq!(reply.overlay_snapshot, expected_state);
            }
            _ => panic!("wrong sidecar reply"),
        }
        drop(body);
        drop(frame);
        assert_eq!(bank.snapshot().class_bytes[MemoryClass::Snapshot as usize], charged);
        drop(alias);
        assert_eq!(bank.snapshot().reserved_bytes, 0);
        assert_eq!(local.snapshot().reserved_bytes, 0);
    }
}

#[test]
fn sidecar_partial_serialization_failure_releases_its_ticket() {
    use memory_budget::MemoryBudget;
    use snapshot_memory::{encode_exact, encode_sidecar};
    let bank = MemoryBudget::new(1024, [1024; 4]);
    let result = encode_sidecar(&bank, 200, &[8, 1], || {
        let first = encode_exact(&8u64, 8)?;
        let second = encode_exact(&9u64, 1)?;
        Ok((first, second))
    });
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidData);
    assert_eq!(bank.snapshot().reserved_bytes, 0);
    assert_eq!(bank.snapshot().pending_bytes, 0);

    assert!(encode_sidecar::<()>(&bank, 200, &[u64::MAX], || panic!("overflow reached encoder")).is_err());
    if usize::BITS == 64 {
        assert_eq!(encode_exact(&0u8, u64::MAX).unwrap_err().kind(), std::io::ErrorKind::OutOfMemory);
    }
}

#[tokio::test]
async fn abandoned_sidecar_blocking_result_drops_data_before_ticket() {
    use memory_budget::MemoryBudget;
    use snapshot_memory::{encode_exact, encode_sidecar};
    use std::{sync::Arc, time::Duration};

    struct Reply {
        data: Vec<u8>,
        bank: Arc<MemoryBudget>,
    }
    impl Drop for Reply {
        fn drop(&mut self) {
            assert_eq!(self.data.len(), 8);
            assert_eq!(self.bank.snapshot().reserved_bytes, 336);
        }
    }

    struct Finished(Option<tokio::sync::oneshot::Sender<()>>);
    impl Drop for Finished {
        fn drop(&mut self) {
            // Drop runs after the returned (reply, lease).
            let _ = self.0.take().unwrap().send(());
        }
    }

    let bank = MemoryBudget::new(1024, [1024; 4]);
    let worker_bank = bank.clone();
    let (started, running) = tokio::sync::oneshot::channel();
    let (release, released) = std::sync::mpsc::channel();
    let (finished, done) = tokio::sync::oneshot::channel();
    let worker = tokio::task::spawn_blocking(move || {
        let reply = encode_sidecar(&worker_bank, 200, &[8], || {
            started.send(()).unwrap();
            released.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok(Reply { data: encode_exact(&21u64, 8)?, bank: worker_bank.clone() })
        })
        .unwrap();
        (reply, Finished(Some(finished)))
    });
    running.await.unwrap();
    assert_eq!(bank.snapshot().pending_bytes, 336);
    drop(worker);
    release.send(()).unwrap();
    done.await.unwrap();
    assert_eq!(bank.snapshot().reserved_bytes, 0);
}
