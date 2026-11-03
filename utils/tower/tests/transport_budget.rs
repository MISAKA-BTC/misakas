use http_body_util::BodyExt;
use kaspa_core::memory_budget::{MemoryBudget, MemoryClass};
use kaspa_utils_tower::transport::{BudgetedProstCodec, Outbound, RetainedSize, convert_preallocated};
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

#[derive(Debug)]
struct DropCheckedMessage {
    blob: Blob,
    bank: Arc<MemoryBudget>,
    observed: Arc<AtomicUsize>,
}
impl Message for DropCheckedMessage {
    fn encode_raw(&self, dst: &mut impl prost::bytes::BufMut) {
        self.blob.encode_raw(dst);
    }
    fn merge_field(
        &mut self,
        tag: u32,
        wire: prost::encoding::WireType,
        src: &mut impl prost::bytes::Buf,
        ctx: prost::encoding::DecodeContext,
    ) -> Result<(), prost::DecodeError> {
        self.blob.merge_field(tag, wire, src, ctx)
    }
    fn encoded_len(&self) -> usize {
        self.blob.encoded_len()
    }
    fn clear(&mut self) {
        self.blob.clear();
    }
}
impl RetainedSize for DropCheckedMessage {
    fn retained_heap_bytes(&self) -> u64 {
        self.blob.retained_heap_bytes()
    }
}
impl Drop for DropCheckedMessage {
    fn drop(&mut self) {
        self.observed.store(self.bank.snapshot().reserved_bytes as usize, Ordering::SeqCst);
    }
}

#[test]
fn failed_admission_drops_backing_before_releasing_generation_ticket() {
    let shared = bank(8192);
    let lease = shared.try_reserve(MemoryClass::Snapshot, 4096).unwrap();
    lease.materialized();
    let observed = Arc::new(AtomicUsize::new(0));
    let message =
        DropCheckedMessage { blob: Blob { value: vec![7; 128] }, bank: Arc::clone(&shared), observed: Arc::clone(&observed) };
    assert!(Outbound::with_preallocated(message, &bank(0), lease, Vec::new()).is_err());
    assert!(observed.load(Ordering::SeqCst) > 0, "ticket returned before backing destructor");
    assert_eq!(shared.snapshot().reserved_bytes, 0);
}

#[test]
fn conversion_unwind_drops_source_before_its_retained_ticket() {
    let shared = bank(8192);
    let retained = shared.try_reserve(MemoryClass::Native, 4096).unwrap();
    retained.materialized();
    let observed = Arc::new(AtomicUsize::new(0));
    let source = DropCheckedMessage { blob: Blob { value: vec![1; 128] }, bank: Arc::clone(&shared), observed: Arc::clone(&observed) };
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        convert_preallocated(source, Some(retained), None, |_| -> () { panic!("intentional conversion failure") });
    }));
    assert!(failure.is_err());
    assert!(observed.load(Ordering::SeqCst) > 0);
    assert_eq!(shared.snapshot().reserved_bytes, 0);
}

#[tokio::test]
async fn slow_transport_slices_hold_snapshot_session_after_dequeue_and_codec_drop() {
    let local = bank(64 << 20);
    let dropped = Arc::new(AtomicUsize::new(0));
    let blob = Blob { value: vec![7; 4 << 20] };
    let expected = blob.encode_to_vec();
    let queued = Outbound::with_resources(blob, &local, vec![Box::new(Session(Arc::clone(&dropped)))]).unwrap();
    let mut codec = BudgetedProstCodec::<Blob, Blob>::default();
    let mut body = EncodeBody::new_client(codec.encoder(), futures::stream::iter([Ok(queued)]), None, None);
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
    assert!(Outbound::try_new(Blob { value: Vec::with_capacity(512 << 10) }, &local).is_ok());
}

#[tokio::test]
async fn codec_peak_is_admitted_before_queue_acceptance_without_waiting() {
    let local = bank(512 << 10);
    assert!(Outbound::try_new(Blob { value: vec![1; 128 << 10] }, &local).is_err());
    assert_eq!(local.snapshot().reserved_bytes, 0);
    assert_eq!(local.snapshot().pending_bytes, 0);
}

#[tokio::test]
async fn all_accepted_large_items_encode_without_needing_more_admission() {
    let local = bank(34 << 20);
    let mut items = Vec::new();
    while let Ok(item) = Outbound::try_new(Blob { value: vec![1; 4 << 20] }, &local) {
        items.push(Ok(item));
    }
    assert_eq!(items.len(), 2);
    let mut codec = BudgetedProstCodec::<Blob, Blob>::default();
    let mut body = EncodeBody::new_client(codec.encoder(), futures::stream::iter(items), None, None);
    for _ in 0..2 {
        drop(body.frame().await.unwrap().unwrap());
    }
    drop(body);
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

#[tokio::test]
async fn reply_conversion_keeps_both_owners_then_transfers_to_http_frame() {
    let shared = bank(8 << 20);
    let source = shared.try_reserve(MemoryClass::Native, 128 << 10).unwrap();
    let original = vec![27u8; 128 << 10];
    source.materialized();
    let conversion = shared.try_reserve(MemoryClass::Transport, 256 << 10).unwrap();
    let (message, conversion) = convert_preallocated(original, Some(source), Some(conversion), |original| {
        let usage = shared.snapshot();
        assert_eq!(usage.class_bytes[MemoryClass::Native as usize], 128 << 10);
        assert_eq!(usage.pending_bytes, 256 << 10);
        Blob { value: original.clone() }
    });
    assert_eq!(shared.snapshot().class_bytes[MemoryClass::Native as usize], 0);
    assert_eq!(shared.snapshot().pending_bytes, 0);
    let local = bank(4 << 20);
    let queued = Outbound::with_preallocated(message, &local, conversion.unwrap(), Vec::new()).unwrap();
    let actual = shared.snapshot().reserved_bytes;
    assert!(actual < 256 << 10);
    let mut codec = BudgetedProstCodec::<Blob, Blob>::default();
    let mut body = EncodeBody::new_client(codec.encoder(), futures::stream::iter([Ok(queued)]), None, None);
    let frame = body.frame().await.unwrap().unwrap().into_data().unwrap();
    drop(body);
    assert_eq!(shared.snapshot().reserved_bytes, actual);
    drop(frame);
    assert_eq!(shared.snapshot().reserved_bytes, 0);
    assert_eq!(local.snapshot().reserved_bytes, 0);
}
