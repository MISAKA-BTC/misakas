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
