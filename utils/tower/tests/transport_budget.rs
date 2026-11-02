use http_body_util::BodyExt;
use kaspa_core::memory_budget::{MemoryBudget, MemoryClass};
use kaspa_utils_tower::transport::{BudgetedProstCodec, Outbound, RetainedSize};
use prost::Message;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tonic::codec::{Codec, EncodeBody};

#[derive(Clone, PartialEq, Message)]
struct Blob {
    #[prost(bytes = "vec", tag = "1")]
    value: Vec<u8>,
}
impl RetainedSize for Blob {
    fn retained_heap_bytes(&self) -> u64 {
        self.value.capacity() as u64
    }
}
struct Session(Arc<AtomicUsize>);
impl Drop for Session {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
fn bank(bytes: u64) -> Arc<MemoryBudget> {
    MemoryBudget::new(bytes, [bytes; 4])
}

#[tokio::test]
async fn slow_transport_slices_hold_snapshot_session_after_dequeue_and_codec_drop() {
    let local = bank(64 << 20);
    let dropped = Arc::new(AtomicUsize::new(0));
    let blob = Blob { value: vec![7; 4 << 20] };
    let expected = blob.encode_to_vec();
    let queued = Outbound::with_resources(blob, &local, vec![Box::new(Session(Arc::clone(&dropped)))]).unwrap();
    let mut codec = BudgetedProstCodec::<Blob, Blob>::default();
    let mut body = EncodeBody::new_client(codec.encoder(), tokio_stream::iter([Ok(queued)]), None, None);
    let bytes = body.frame().await.unwrap().unwrap().into_data().unwrap();
    assert_eq!(&bytes[5..], expected);
    let shared_transport_slice = bytes.slice(5..);
    drop(bytes);
    drop(body);
    assert_eq!(dropped.load(Ordering::SeqCst), 0);
    assert!(local.snapshot().reserved_bytes > 4 << 20);
    drop(shared_transport_slice);
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    assert_eq!(local.snapshot().reserved_bytes, 0);
}

#[tokio::test]
async fn cancelled_queue_returns_session_and_unused_capacity_tickets() {
    let local = bank(1 << 20);
    let dropped = Arc::new(AtomicUsize::new(0));
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    let value = Vec::with_capacity(512 << 10); // zero wire payload, large retained allocation
    let queued = Outbound::with_resources(Blob { value }, &local, vec![Box::new(Session(Arc::clone(&dropped)))]).unwrap();
    sender.try_send(queued).unwrap();
    assert!(local.snapshot().reserved_bytes >= 512 << 10);
    assert!(Outbound::try_new(Blob { value: Vec::with_capacity(512 << 10) }, &local).is_err());
    drop(receiver);
    assert_eq!(local.snapshot().reserved_bytes, 0);
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    // Capacity is usable again immediately; no wait/deadlock to wake.
    assert!(Outbound::try_new(Blob { value: vec![1; 512 << 10] }, &local).is_ok());
}

#[tokio::test]
async fn codec_admission_failure_releases_original_without_waiting() {
    let local = bank(512 << 10);
    let queued = Outbound::try_new(Blob { value: vec![1; 128 << 10] }, &local).unwrap();
    let mut codec = BudgetedProstCodec::<Blob, Blob>::default();
    let mut body = EncodeBody::new_client(codec.encoder(), tokio_stream::iter([Ok(queued)]), None, None);
    let status = body.frame().await.unwrap().unwrap_err();
    assert_eq!(status.code(), tonic::Code::ResourceExhausted);
    assert_eq!(local.snapshot().reserved_bytes, 0);
}

#[test]
fn many_connections_share_one_process_bank_and_return_all_leases() {
    let shared = bank(1024);
    let mut seats = Vec::new();
    for _ in 0..8 {
        seats.push(shared.try_reserve(MemoryClass::Transport, 128).unwrap());
    }
    assert!(shared.try_reserve(MemoryClass::Transport, 1).is_err());
    drop(seats.pop());
    assert!(shared.try_reserve(MemoryClass::Snapshot, 128).is_ok());
    drop(seats);
    assert_eq!(shared.snapshot().reserved_bytes, 0);
}
