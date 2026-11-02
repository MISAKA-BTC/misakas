//! Intentionally failing opt-in counterexamples, not production protection.
//! Run alone: the first case pins/fills the process bank from its environment.
use super::*;
use http_body_util::BodyExt;
use prost::Message;
use tonic::codec::{Codec, EncodeBody};
use transport::RetainedSize;

fn headers(count: usize) -> p2p::KaspadMessage {
    p2p::KaspadMessage {
        payload: Some(p2p::kaspad_message::Payload::BlockHeaders(p2p::BlockHeadersMessage {
            block_headers: vec![p2p::BlockHeader::default(); count],
        })),
        ..Default::default()
    }
}

#[tokio::test]
#[ignore = "known failing inbound contract; run isolated with MISAKA_MEMORY_BUDGET_BYTES=1048576"]
async fn exhausted_process_budget_must_refuse_plain_and_gzip_before_decoded_heap() {
    use memory_budget::{MemoryClass, global_memory_budget};
    use wire_preflight::WirePreflight;
    // Ordinary canonical protobuf. No unsupported duplicate singular/oneof
    // encoding is used. No consensus block is submitted or judged here.
    let mut frames = Vec::new();
    for gzip in [false, true] {
        let message = headers(4097);
        let wire = message.encode_to_vec();
        let schema = p2p::KaspadMessage::wire_heap_bound(&wire, 0).unwrap();
        let mut encoder = tonic_prost::ProstCodec::<p2p::KaspadMessage, p2p::KaspadMessage>::default();
        let mut body = EncodeBody::new_client(
            encoder.encoder(),
            futures::stream::iter([Ok(message)]),
            gzip.then_some(tonic::codec::CompressionEncoding::Gzip),
            Some(320 << 20),
        );
        let bytes = body.frame().await.unwrap().unwrap().into_data().unwrap();
        frames.push((gzip, bytes, schema));
    }
    let bank = global_memory_budget();
    assert_eq!(bank.snapshot().limit_bytes, 1 << 20, "run this case alone with the documented environment");
    let resident = vec![0u8; 1 << 20];
    let held = bank.try_reserve_retained(MemoryClass::Transport, resident.len() as u64).unwrap();
    assert!(bank.try_reserve(MemoryClass::Transport, 1).is_err());
    let full = bank.snapshot().reserved_bytes;
    let mut accumulator = Vec::new();
    let mut accepted = 0;
    for (gzip, bytes, schema) in frames {
        let wire_bytes = bytes.len();
        let mut codec = transport::BudgetedProstCodec::<p2p::KaspadMessage, p2p::KaspadMessage>::default();
        let mut receive = tonic::Streaming::new_request(
            codec.decoder(),
            http_body_util::Full::new(bytes),
            gzip.then_some(tonic::codec::CompressionEncoding::Gzip),
            Some(320 << 20),
        );
        match receive.message().await {
            Err(status) => assert_eq!(status.code(), tonic::Code::ResourceExhausted, "local pressure must be typed local failure"),
            Ok(Some(message)) => {
                accepted += 1;
                let heap = message.retained_heap_bytes();
                // Mirror the actual plain incoming route and successful payload
                // move. The accumulated message data survives the decoder/queue.
                let (tx, mut rx) = tokio::sync::mpsc::channel::<p2p::KaspadMessage>(1);
                tx.send(message).await.unwrap();
                let message = rx.recv().await.unwrap();
                let Some(p2p::kaspad_message::Payload::BlockHeaders(payload)) = message.payload else { panic!("headers") };
                accumulator.push(payload);
                drop(receive);
                println!(
                    "INBOUND_GAP gzip={gzip} wire={wire_bytes} schema_peak={} decoded_heap={heap} bank_full={full} bank_after_dequeue={} accumulated_headers={}",
                    schema.peak_bytes,
                    bank.snapshot().reserved_bytes,
                    accumulator.iter().map(|message| message.block_headers.len()).sum::<usize>()
                );
                assert_eq!(bank.snapshot().reserved_bytes, full, "characterization: no inbound ticket was created");
            }
            Ok(None) => panic!("fixture must decode one message or fail admission"),
        }
    }
    assert_eq!(resident.len(), held.bytes() as usize);
    assert_eq!(accepted, 0, "KNOWN GAP: a full process bank admitted and retained decoded protobuf allocations");
}

struct QueueOnlyOwner {
    message: p2p::KaspadMessage,
    _lease: memory_budget::MemoryLease,
}
fn extract_with_production_unwrap(queued: QueueOnlyOwner) -> p2p::BlockHeadersMessage {
    // This is the actual production macro extracted by build.rs. A proposed
    // outer-only wrapper does not fix the lifetime: its other fields drop at
    // this function's return while the extracted payload escapes.
    unwrap_message!(Some(queued.message), p2p::kaspad_message::Payload::BlockHeaders).unwrap()
}

#[test]
#[ignore = "known failing proposed queue-only wrapper; actual macro returns a plain payload"]
fn queue_only_ticket_must_outlive_production_payload_extraction() {
    use memory_budget::{MemoryBudget, MemoryClass};
    let message = headers(257);
    let owned = std::mem::size_of::<p2p::KaspadMessage>() as u64 + message.retained_heap_bytes();
    let bank = MemoryBudget::new(owned, [owned; 4]);
    let lease = bank.try_reserve_retained(MemoryClass::Transport, owned).unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    tx.try_send(QueueOnlyOwner { message, _lease: lease }).unwrap_or_else(|_| panic!("empty queue"));
    assert_eq!(bank.snapshot().reserved_bytes, owned);
    let payload = extract_with_production_unwrap(rx.try_recv().unwrap());
    let live = payload.retained_heap_bytes();
    println!("OWNER_GAP charged_in_queue={owned} live_payload_heap={live} bank_after_extraction={}", bank.snapshot().reserved_bytes);
    assert_eq!(payload.block_headers.len(), 257);
    assert!(live > 0);
    assert!(bank.snapshot().reserved_bytes >= live, "KNOWN GAP: outer-only lease returned while extracted payload remains alive");
}
