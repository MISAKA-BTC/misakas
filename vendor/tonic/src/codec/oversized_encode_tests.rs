//! Same tests can be injected into pristine tonic 0.14.5 as a negative control.
use super::*;
use bytes::BufMut;
use std::{
    io::Read,
    sync::{Arc, Mutex},
    task::Waker,
};

#[derive(Default)]
struct RawEncoder {
    fail_after_copy: bool,
    queued: Arc<Mutex<Option<Vec<u8>>>>,
    resources: RetainedResources,
}
impl Encoder for RawEncoder {
    type Item = Vec<u8>;
    type Error = Status;
    fn take_resources(&mut self) -> RetainedResources {
        std::mem::take(&mut self.resources)
    }
    fn encode(&mut self, item: Vec<u8>, buf: &mut EncodeBuf<'_>) -> Result<(), Status> {
        buf.put_slice(&item);
        if self.fail_after_copy {
            Err(Status::internal("intentional encoder failure"))
        } else {
            Ok(())
        }
    }
}
struct OneThenPending {
    item: Arc<Mutex<Option<Vec<u8>>>>,
}
impl Stream for OneThenPending {
    type Item = Result<Vec<u8>, Status>;
    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.item
            .lock()
            .unwrap()
            .take()
            .map(|v| Poll::Ready(Some(Ok(v))))
            .unwrap_or(Poll::Pending)
    }
}
fn stream(payload: Vec<u8>, gzip: bool) -> EncodedBytes<RawEncoder, OneThenPending> {
    let queued = Arc::new(Mutex::new(Some(payload)));
    EncodedBytes::new(
        RawEncoder {
            queued: Arc::clone(&queued),
            ..Default::default()
        },
        OneThenPending { item: queued },
        gzip.then_some(CompressionEncoding::Gzip),
        SingleMessageCompressionOverride::default(),
        None,
    )
}
fn next(s: &mut EncodedBytes<RawEncoder, OneThenPending>) -> Bytes {
    match Pin::new(s).poll_next(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(Some(Ok(bytes))) => bytes,
        other => panic!("expected encoded frame, got {other:?}"),
    }
}
fn reference_frame(payload: &[u8], gzip: bool) -> Vec<u8> {
    // Pristine 0.14.5 framing and GzEncoder configuration, independent of scratch ownership.
    let body = if gzip {
        let mut encoder = flate2::read::GzEncoder::new(payload, flate2::Compression::new(6));
        let mut bytes = Vec::new();
        encoder.read_to_end(&mut bytes).unwrap();
        bytes
    } else {
        payload.to_vec()
    };
    let mut frame = vec![u8::from(gzip)];
    frame.extend_from_slice(&(body.len() as u32).to_be_bytes());
    frame.extend(body);
    frame
}
fn assert_no_large_backing(s: &mut EncodedBytes<RawEncoder, OneThenPending>, prior_len: usize) {
    // try_reclaim never allocates: catches an empty BytesMut view which still owns
    // a large allocation even when its reported capacity became zero via advance().
    assert!(
        !s.uncompression_buf.try_reclaim(prior_len),
        "raw scratch retained prior allocation"
    );
    assert!(
        !s.buf.try_reclaim(prior_len),
        "output scratch retained prior allocation"
    );
}

#[test]
fn oversized_gzip_wire_bytes_unchanged_and_raw_backing_released() {
    let payload = vec![b'x'; 4 * 1024 * 1024];
    let expected = reference_frame(&payload, true);
    let mut s = stream(payload.clone(), true);
    let frame = next(&mut s);
    assert_eq!(&frame[..], expected);
    assert!(
        frame.is_unique(),
        "encoder must not retain an empty tail of the emitted allocation"
    );
    drop(frame);
    assert_no_large_backing(&mut s, payload.len());
}
#[test]
fn oversized_uncompressed_wire_bytes_unchanged_and_output_backing_released() {
    let payload = vec![b'y'; 4 * 1024 * 1024];
    let expected = reference_frame(&payload, false);
    let mut s = stream(payload.clone(), false);
    let frame = next(&mut s);
    assert_eq!(&frame[..], expected);
    assert!(frame.is_unique());
    drop(frame);
    assert_no_large_backing(&mut s, payload.len());
}
#[test]
fn large_then_small_gzip_does_not_restore_large_reuse_capacity() {
    let large = 4 * 1024 * 1024;
    let mut s = stream(vec![9; large], true);
    drop(next(&mut s));
    *s.encoder.queued.lock().unwrap() = Some(vec![7; 31]);
    let frame = next(&mut s);
    assert_eq!(&frame[..], reference_frame(&[7; 31], true));
    drop(frame);
    assert_no_large_backing(&mut s, large);
}
#[test]
fn slow_reader_owns_large_frame_without_encoder_alias() {
    let payload = vec![0; 4 * 1024 * 1024];
    let expected = reference_frame(&payload, false);
    let mut s = stream(payload, false);
    let held_frame = next(&mut s);
    assert!(held_frame.is_unique());
    assert!(Pin::new(&mut s)
        .poll_next(&mut Context::from_waker(Waker::noop()))
        .is_pending());
    assert!(s.buf.capacity() <= 1024 * 1024);
    assert_eq!(&held_frame[..], expected); // Frame stays valid for an arbitrarily slow body consumer.
}
#[test]
fn dropping_encoder_does_not_invalidate_inflight_frame() {
    let payload = vec![6; 2 * 1024 * 1024];
    let expected = reference_frame(&payload, true);
    let mut s = stream(payload, true);
    let held_frame = next(&mut s);
    drop(s);
    assert!(held_frame.is_unique());
    assert_eq!(&held_frame[..], expected);
}
#[test]
fn small_messages_keep_existing_reuse_and_wire_behavior() {
    let mut s = stream(vec![2; 23], true);
    drop(next(&mut s));
    assert!(s.uncompression_buf.try_reclaim(1024));
    *s.encoder.queued.lock().unwrap() = Some(vec![3; 29]);
    assert_eq!(&next(&mut s)[..], reference_frame(&[3; 29], true));
}
#[test]
fn oversized_partial_encode_failure_releases_scratch() {
    let mut raw = BytesMut::new();
    let mut output = BytesMut::new();
    let error = encode_item(
        &mut RawEncoder {
            fail_after_copy: true,
            ..Default::default()
        },
        &mut output,
        &mut raw,
        Some(CompressionEncoding::Gzip),
        None,
        BufferSettings::default(),
        vec![1; 4 * 1024 * 1024],
    );
    assert!(error.is_err());
    assert!(raw.is_empty(), "partial encoder scratch retained bytes");
    assert!(!raw.try_reclaim(4 * 1024 * 1024));
}
#[test]
fn multiple_large_frames_do_not_accumulate_encoder_backings() {
    let large = 2 * 1024 * 1024;
    let mut s = stream(vec![0; large], true);
    let mut held = Vec::new();
    held.push(next(&mut s));
    for n in 1..4 {
        *s.encoder.queued.lock().unwrap() = Some(vec![n; large]);
        held.push(next(&mut s));
    }
    assert!(held.iter().all(Bytes::is_unique));
    // Each body consumer owns only its actual frame; the encoder owns small scratch.
    assert!(s.buf.capacity() <= 1024 * 1024);
    assert!(!s.uncompression_buf.try_reclaim(large));
    drop(held);
    assert_no_large_backing(&mut s, large);
}

#[test]
fn highly_compressed_frame_does_not_retain_uncompressed_sized_backing() {
    let payload = vec![0; 4 * 1024 * 1024];
    let expected = reference_frame(&payload, true);
    let mut s = stream(payload, true);
    let frame = next(&mut s);
    assert_eq!(&frame[..], expected);
    let wire = frame.len();
    let backing = frame
        .try_into_mut()
        .expect("frame must have sole ownership");
    assert!(
        backing.capacity() <= wire * 2,
        "wire frame retained sparse large allocation"
    );
    assert!(s.buf.capacity() <= 1024 * 1024);
}

#[test]
fn budgeted_sub_ceiling_frame_has_no_idle_scratch_alias_after_ticket_return() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Ticket(Arc<AtomicUsize>);
    impl Drop for Ticket {
        fn drop(&mut self) { self.0.fetch_add(1, Ordering::SeqCst); }
    }
    for gzip in [false, true] {
        let data = vec![29; 512 << 10]; // Below the ordinary 1 MiB reuse ceiling.
        let released = Arc::new(AtomicUsize::new(0));
        let mut s = stream(data.clone(), gzip);
        s.encoder.resources.push(Box::new(Ticket(Arc::clone(&released))));
        let frame = next(&mut s);
        assert_eq!(&frame[..], reference_frame(&data, gzip));
        assert_no_large_backing(&mut s, data.len());
        let held_slice = frame.slice(1..);
        drop(frame);
        assert_eq!(released.load(Ordering::SeqCst), 0);
        drop(held_slice);
        assert_eq!(released.load(Ordering::SeqCst), 1);
        assert_no_large_backing(&mut s, data.len());
        assert!(Pin::new(&mut s)
            .poll_next(&mut Context::from_waker(Waker::noop()))
            .is_pending());
    }
}
