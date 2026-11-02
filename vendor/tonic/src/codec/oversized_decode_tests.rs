use super::*;
use bytes::Bytes;
use http_body_util::Full;
use std::task::Waker;

struct RawDecoder;
impl Decoder for RawDecoder {
    type Item = Bytes;
    type Error = Status;
    fn decode(&mut self, src: &mut DecodeBuf<'_>) -> Result<Option<Bytes>, Status> {
        Ok(Some(src.copy_to_bytes(src.remaining())))
    }
}
fn frame(data: &[u8], gzip: bool) -> Vec<u8> {
    use std::io::Read;
    let payload = if gzip {
        let mut gz = flate2::read::GzEncoder::new(data, flate2::Compression::new(6));
        let mut encoded = Vec::new();
        gz.read_to_end(&mut encoded).unwrap();
        encoded
    } else {
        data.to_vec()
    };
    let mut result = vec![u8::from(gzip)];
    result.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    result.extend_from_slice(&payload);
    result
}
fn stream(frames: Vec<u8>, gzip: bool) -> Streaming<Bytes> {
    Streaming::new_request(RawDecoder, Full::new(Bytes::from(frames)), gzip.then_some(CompressionEncoding::Gzip), Some(320 << 20))
}
fn next(s: &mut Streaming<Bytes>) -> Bytes {
    match Pin::new(s).poll_next(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(Some(Ok(msg))) => msg,
        other => panic!("expected decoded message, got {other:?}"),
    }
}
fn assert_no_large_scratch(s: &mut Streaming<Bytes>, len: usize) {
    assert!(!s.inner.buf.try_reclaim(len), "read buffer retains completed backing");
    assert!(!s.inner.decompress_buf.try_reclaim(len), "decompression retains completed backing");
}

#[test]
fn plain_large_then_small_preserves_unread_next_frame_and_shared_output() {
    let data = vec![13; 4 << 20];
    let mut both = frame(&data, false);
    both.extend(frame(b"next", false));
    let mut s = stream(both, false);
    let held = next(&mut s); // Shares the old BytesMut backing.
    assert_eq!(&held[..], data);
    assert_eq!(&next(&mut s)[..], b"next");
    assert_no_large_scratch(&mut s, data.len());
    assert_eq!(&held[..], data); // Shrinking scratch did not invalidate shared output.
    drop(held);
    assert_no_large_scratch(&mut s, data.len());
}

#[test]
fn gzip_large_then_small_releases_decompressed_capacity_on_idle() {
    let data = vec![27; 4 << 20];
    let mut both = frame(&data, true);
    both.extend(frame(b"next", true));
    let mut s = stream(both, true);
    assert_eq!(&next(&mut s)[..], data);
    assert_eq!(&next(&mut s)[..], b"next");
    assert_no_large_scratch(&mut s, data.len());
}

#[test]
fn two_large_unread_frames_are_not_truncated_by_release() {
    let data = vec![42; 2 << 20];
    let mut both = frame(&data, false);
    both.extend(frame(&data, false));
    let mut s = stream(both, false);
    assert_eq!(&next(&mut s)[..], data);
    assert_eq!(&next(&mut s)[..], data);
    assert_no_large_scratch(&mut s, data.len());
}

#[test]
fn truncated_gzip_error_returns_large_scratch_and_terminates() {
    let data = vec![17; 4 << 20];
    let mut malformed = frame(&data, true);
    malformed.truncate(malformed.len() - 5);
    let len = malformed.len() - 5;
    malformed[1..5].copy_from_slice(&(len as u32).to_be_bytes());
    let mut s = stream(malformed, true);
    assert!(matches!(Pin::new(&mut s).poll_next(&mut Context::from_waker(Waker::noop())), Poll::Ready(Some(Err(_)))));
    assert_no_large_scratch(&mut s, data.len());
    assert!(matches!(Pin::new(&mut s).poll_next(&mut Context::from_waker(Waker::noop())), Poll::Ready(None)));
}

// A real HTTP body that can go idle between fragments without reporting EOF.
#[derive(Clone)]
struct PausableBody {
    frames: std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<Result<http_body::Frame<Bytes>, Status>>>>,
}
impl PausableBody {
    fn new(data: Vec<u8>) -> Self {
        let body = Self { frames: Default::default() };
        body.push(Ok(http_body::Frame::data(Bytes::from(data))));
        body
    }
    fn push(&self, frame: Result<http_body::Frame<Bytes>, Status>) {
        self.frames.lock().unwrap().push_back(frame);
    }
}
impl HttpBody for PausableBody {
    type Data = Bytes;
    type Error = Status;
    fn poll_frame(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Result<http_body::Frame<Bytes>, Status>>> {
        match self.frames.lock().unwrap().pop_front() {
            Some(frame) => Poll::Ready(Some(frame)),
            None => Poll::Pending,
        }
    }
}
fn paused(body: PausableBody, gzip: bool) -> Streaming<Bytes> {
    Streaming::new_request(RawDecoder, body, gzip.then_some(CompressionEncoding::Gzip), Some(320 << 20))
}
fn poll(s: &mut Streaming<Bytes>) -> Poll<Option<Result<Bytes, Status>>> {
    Pin::new(s).poll_next(&mut Context::from_waker(Waker::noop()))
}
fn assert_error_once_and_empty(s: &mut Streaming<Bytes>, code: Code) {
    match poll(s) {
        Poll::Ready(Some(Err(error))) => assert_eq!(error.code(), code),
        result => panic!("expected terminal {code:?}, got {result:?}"),
    }
    assert_eq!(s.inner.buf.capacity(), 0);
    assert_eq!(s.inner.decompress_buf.capacity(), 0);
    assert!(matches!(poll(s), Poll::Ready(None)));
}

#[test]
fn plain_large_small_idle_keeps_split_next_header_and_all_shared_aliases() {
    let large = vec![71; 4 << 20];
    let small = frame(b"small after idle", false);
    let mut initial = frame(&large, false);
    initial.extend_from_slice(&small[..3]); // Incomplete next gRPC header.
    let body = PausableBody::new(initial);
    let mut s = paused(body.clone(), false);
    let held = next(&mut s);
    let cloned = held.clone();
    let slice = held.slice(17..4097);
    assert_eq!(s.inner.buf.as_ref(), &small[..3]);
    assert!(s.inner.buf.capacity() <= MAX_RETAINED_DECODER_BUFFER_SIZE);
    assert!(matches!(poll(&mut s), Poll::Pending));
    assert_eq!(s.inner.buf.as_ref(), &small[..3]);
    body.push(Ok(http_body::Frame::data(Bytes::copy_from_slice(&small[3..]))));
    assert_eq!(&next(&mut s)[..], b"small after idle");
    assert!(matches!(poll(&mut s), Poll::Pending));
    assert_eq!(&held[..], large);
    assert_eq!(&cloned[..], large);
    assert!(slice.iter().all(|byte| *byte == 71));
    drop((held, cloned, slice));
    assert_no_large_scratch(&mut s, large.len());
    assert_eq!(s.inner.max_message_size, Some(320 << 20));
}

#[test]
fn gzip_large_small_idle_preserves_partial_compressed_next_payload_and_aliases() {
    // Incompressible data exercises both the wire and decompression buffers.
    let mut word = 0x12345678u32;
    let large: Vec<u8> = (0..(4 << 20))
        .map(|_| {
            word ^= word << 13;
            word ^= word >> 17;
            word ^= word << 5;
            word as u8
        })
        .collect();
    let small = frame(b"gzip after idle", true);
    let split = HEADER_SIZE + 3;
    let mut initial = frame(&large, true);
    initial.extend_from_slice(&small[..split]);
    let body = PausableBody::new(initial);
    let mut s = paused(body.clone(), true);
    let held = next(&mut s);
    let slice = held.slice(11..8193);
    assert_eq!(s.inner.decompress_buf.capacity(), 0);
    assert!(matches!(poll(&mut s), Poll::Pending));
    assert_eq!(s.inner.buf.as_ref(), &small[HEADER_SIZE..split]);
    body.push(Ok(http_body::Frame::data(Bytes::copy_from_slice(&small[split..]))));
    assert_eq!(&next(&mut s)[..], b"gzip after idle");
    assert!(matches!(poll(&mut s), Poll::Pending));
    assert_eq!(&held[..], large);
    assert_eq!(&slice[..], &large[11..8193]);
    drop((held, slice));
    assert_no_large_scratch(&mut s, large.len());
}

#[test]
fn body_error_after_large_frame_and_partial_next_frame_frees_scratch_once() {
    let large = vec![31; 2 << 20];
    let mut initial = frame(&large, false);
    initial.extend_from_slice(&[0, 0, 64, 0, 0]); // Announce a 4 MiB message.
    initial.extend_from_slice(b"partial");
    let body = PausableBody::new(initial);
    body.push(Err(Status::unavailable("test body failure")));
    let mut s = paused(body, false);
    let held = next(&mut s);
    assert_error_once_and_empty(&mut s, Code::Unavailable);
    assert_eq!(&held[..], large);
}

#[test]
fn eof_after_only_a_large_declared_header_is_truncated_not_success() {
    let mut s = stream(vec![0, 0, 64, 0, 0], false);
    assert_error_once_and_empty(&mut s, Code::Internal);
}

#[test]
fn trailers_cannot_hide_a_truncated_message_or_keep_its_scratch() {
    let body = PausableBody::new(vec![0, 0, 64, 0, 0, 17]);
    body.push(Ok(http_body::Frame::trailers(HeaderMap::new())));
    let mut s = paused(body, false);
    assert_error_once_and_empty(&mut s, Code::Internal);
}

#[test]
fn successful_trailers_release_scratch_and_preserve_metadata_and_aliases() {
    let large = vec![91; 2 << 20];
    let body = PausableBody::new(frame(&large, false));
    let mut trailers = HeaderMap::new();
    trailers.insert("test-trailer", "kept".parse().unwrap());
    body.push(Ok(http_body::Frame::trailers(trailers)));
    let mut s = paused(body, false);
    let held = next(&mut s);
    assert!(matches!(poll(&mut s), Poll::Ready(None)));
    assert_eq!(s.inner.buf.capacity(), 0);
    assert_eq!(s.inner.decompress_buf.capacity(), 0);
    assert_eq!(s.inner.trailers.as_ref().unwrap()["test-trailer"], "kept");
    assert_eq!(&held[..], large);
    assert!(matches!(poll(&mut s), Poll::Ready(None)));
}

#[test]
fn several_unread_large_frames_survive_until_each_completed_boundary() {
    let first = vec![7; 2 << 20];
    let second = vec![8; 3 << 20];
    let mut input = frame(&first, false);
    let unread = frame(&second, false);
    input.extend_from_slice(&unread);
    input.extend(frame(b"tail", false));
    let mut s = stream(input, false);
    let held = next(&mut s);
    assert!(s.inner.buf.starts_with(&unread));
    assert!(s.inner.buf.len() > MAX_RETAINED_DECODER_BUFFER_SIZE);
    assert_eq!(&next(&mut s)[..], second);
    assert_eq!(&next(&mut s)[..], b"tail");
    assert!(matches!(poll(&mut s), Poll::Ready(None)));
    assert_eq!(&held[..], first);
    drop(held);
    assert_no_large_scratch(&mut s, second.len());
}

#[test]
fn completed_gzip_prefix_decoder_discards_unused_scratch_but_retains_output_alias() {
    struct PrefixDecoder;
    impl Decoder for PrefixDecoder {
        type Item = Bytes;
        type Error = Status;
        fn decode(&mut self, src: &mut DecodeBuf<'_>) -> Result<Option<Bytes>, Status> {
            // A completed decoded item can intentionally retain only a prefix.
            // The rest of this decompressed message is not a future wire frame.
            Ok(Some(src.copy_to_bytes(1)))
        }
    }
    let mut frames = frame(&vec![19; 4 << 20], true);
    frames.extend(frame(b"second", true));
    let mut s =
        Streaming::new_request(PrefixDecoder, Full::new(Bytes::from(frames)), Some(CompressionEncoding::Gzip), Some(256 << 20));
    let held = next(&mut s);
    assert_eq!(&held[..], &[19]);
    assert_eq!(s.inner.decompress_buf.len(), 0);
    assert_eq!(s.inner.decompress_buf.capacity(), 0);
    assert_eq!(&next(&mut s)[..], b"s");
    assert!(matches!(poll(&mut s), Poll::Ready(None)));
    assert_eq!(&held[..], &[19]);
    drop(held);
    assert_no_large_scratch(&mut s, 4 << 20);
    assert_eq!(s.inner.max_message_size, Some(256 << 20));
}

#[test]
fn negotiated_gzip_stream_preserves_mixed_plain_and_compressed_frames() {
    let large = vec![53; 2 << 20];
    let mut frames = frame(&large, true);
    frames.extend(frame(b"plain flag under negotiated gzip", false));
    frames.extend(frame(b"gzip again", true));
    let mut s = stream(frames, true);
    let held = next(&mut s);
    assert_eq!(&next(&mut s)[..], b"plain flag under negotiated gzip");
    assert_eq!(&next(&mut s)[..], b"gzip again");
    assert!(matches!(poll(&mut s), Poll::Ready(None)));
    assert_eq!(&held[..], large);
    drop(held);
    assert_no_large_scratch(&mut s, large.len());
}
