use super::compression::{compress, CompressionEncoding, CompressionSettings, SingleMessageCompressionOverride};
use super::{BufferSettings, EncodeBuf, Encoder, RetainedResources, DEFAULT_MAX_SEND_MESSAGE_SIZE, HEADER_SIZE};
use crate::Status;
use bytes::{BufMut, Bytes, BytesMut};
use http::HeaderMap;
use http_body::{Body, Frame};
use pin_project::pin_project;
use std::{
    pin::Pin,
    task::{ready, Context, Poll},
};
use tokio_stream::{adapters::Fuse, Stream, StreamExt};

// Large gRPC messages must not leave their backing allocations attached to an
// otherwise idle, long-lived stream. This is a reuse ceiling, not a wire limit.
const MAX_RETAINED_ENCODER_BUFFER_SIZE: usize = 1024 * 1024;

fn take_encoded_buffer(buf: &mut BytesMut, settings: BufferSettings) -> Bytes {
    let reuse_limit = MAX_RETAINED_ENCODER_BUFFER_SIZE.max(settings.buffer_size);
    if buf.len() > reuse_limit || buf.capacity() > reuse_limit {
        // gzip reserves output by the uncompressed length. Avoid making a
        // slow reader own that large backing when its actual wire frame is
        // tiny. The ratio gate bounds this extra copy to <=1/4 of that backing;
        // large plain frames still move without a second full-message copy.
        if !buf.is_empty() && buf.capacity() / buf.len() >= 4 && buf.capacity() - buf.len() > reuse_limit {
            let frame = Bytes::copy_from_slice(buf);
            *buf = BytesMut::with_capacity(settings.buffer_size);
            return frame;
        }
        // Move the COMPLETE backing allocation into the returned frame. split_to
        // would leave an empty tail sharing that allocation with this encoder.
        std::mem::replace(buf, BytesMut::with_capacity(settings.buffer_size)).freeze()
    } else {
        buf.split_to(buf.len()).freeze()
    }
}

#[derive(Debug)]
struct RetainedFrame {
    bytes: Bytes,
    _resources: RetainedResources,
}
impl AsRef<[u8]> for RetainedFrame {
    fn as_ref(&self) -> &[u8] {
        self.bytes.as_ref()
    }
}
fn take_retained_frame(buf: &mut BytesMut, settings: BufferSettings, resources: &mut RetainedResources) -> Bytes {
    let bytes = take_encoded_buffer(buf, settings);
    if resources.is_empty() {
        bytes
    } else {
        // A small split_to can leave an empty encoder tail sharing this frame's
        // backing. Budgeted streams must drop that alias before its ticket returns.
        *buf = BytesMut::with_capacity(settings.buffer_size);
        Bytes::from_owner(RetainedFrame { bytes, _resources: std::mem::take(resources) })
    }
}

fn release_oversized_scratch(buf: &mut BytesMut, encoded_len: usize, settings: BufferSettings) {
    let reuse_limit = MAX_RETAINED_ENCODER_BUFFER_SIZE.max(settings.buffer_size);
    // compress() advances the view to zero length; capacity alone therefore
    // cannot detect the large original allocation after a successful encode.
    if encoded_len > reuse_limit || buf.capacity() > reuse_limit {
        *buf = BytesMut::with_capacity(settings.buffer_size);
    }
}

/// Combinator for efficient encoding of messages into reasonably sized buffers.
/// EncodedBytes encodes ready messages from its delegate stream into a BytesMut,
/// splitting off and yielding a buffer when either:
///  * The delegate stream polls as not ready, or
///  * The encoded buffer surpasses YIELD_THRESHOLD.
#[pin_project(project = EncodedBytesProj)]
#[derive(Debug)]
struct EncodedBytes<T, U> {
    #[pin]
    source: Fuse<U>,
    compression_encoding: Option<CompressionEncoding>,
    max_message_size: Option<usize>,
    buf: BytesMut,
    uncompression_buf: BytesMut,
    error: Option<Status>,
    resources: RetainedResources,
    // Drop buffers before encoder-held tickets, including panic unwinding from
    // a custom encoder which has not yet transferred its resources to a frame.
    encoder: T,
}

impl<T: Encoder, U: Stream> EncodedBytes<T, U> {
    fn new(
        encoder: T,
        source: U,
        compression_encoding: Option<CompressionEncoding>,
        compression_override: SingleMessageCompressionOverride,
        max_message_size: Option<usize>,
    ) -> Self {
        let buffer_settings = encoder.buffer_settings();
        let buf = BytesMut::with_capacity(buffer_settings.buffer_size);

        let compression_encoding =
            if compression_override == SingleMessageCompressionOverride::Disable { None } else { compression_encoding };

        let uncompression_buf =
            if compression_encoding.is_some() { BytesMut::with_capacity(buffer_settings.buffer_size) } else { BytesMut::new() };

        Self {
            source: source.fuse(),
            encoder,
            compression_encoding,
            max_message_size,
            buf,
            uncompression_buf,
            error: None,
            resources: Vec::new(),
        }
    }
}

impl<T, U> Stream for EncodedBytes<T, U>
where
    T: Encoder<Error = Status>,
    U: Stream<Item = Result<T::Item, Status>>,
{
    type Item = Result<Bytes, Status>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let EncodedBytesProj { mut source, encoder, compression_encoding, max_message_size, buf, uncompression_buf, error, resources } =
            self.project();
        let buffer_settings = encoder.buffer_settings();

        if let Some(status) = error.take() {
            return Poll::Ready(Some(Err(status)));
        }

        loop {
            match source.as_mut().poll_next(cx) {
                Poll::Pending if buf.is_empty() => {
                    return Poll::Pending;
                }
                Poll::Ready(None) if buf.is_empty() => {
                    return Poll::Ready(None);
                }
                Poll::Pending | Poll::Ready(None) => {
                    return Poll::Ready(Some(Ok(take_retained_frame(buf, buffer_settings, resources))));
                }
                Poll::Ready(Some(Ok(item))) => {
                    if let Err(status) =
                        encode_item(encoder, buf, uncompression_buf, *compression_encoding, *max_message_size, buffer_settings, item)
                    {
                        // An error terminates this body; free partial buffers and tickets now.
                        *buf = BytesMut::new();
                        *uncompression_buf = BytesMut::new();
                        resources.clear();
                        drop(encoder.take_resources());
                        return Poll::Ready(Some(Err(status)));
                    }
                    resources.append(&mut encoder.take_resources());
                    if !resources.is_empty() && compression_encoding.is_some() {
                        // This peak ticket travels with the HTTP frame; its old
                        // compression backing must not remain on an idle stream.
                        *uncompression_buf = BytesMut::with_capacity(buffer_settings.buffer_size);
                    }

                    if buf.len() >= buffer_settings.yield_threshold {
                        return Poll::Ready(Some(Ok(take_retained_frame(buf, buffer_settings, resources))));
                    }
                }
                Poll::Ready(Some(Err(status))) => {
                    if buf.is_empty() {
                        return Poll::Ready(Some(Err(status)));
                    }
                    *error = Some(status);
                    return Poll::Ready(Some(Ok(take_retained_frame(buf, buffer_settings, resources))));
                }
            }
        }
    }
}

fn encode_item<T>(
    encoder: &mut T,
    buf: &mut BytesMut,
    uncompression_buf: &mut BytesMut,
    compression_encoding: Option<CompressionEncoding>,
    max_message_size: Option<usize>,
    buffer_settings: BufferSettings,
    item: T::Item,
) -> Result<(), Status>
where
    T: Encoder<Error = Status>,
{
    encoder.prepare_encode(&item).map_err(|err| Status::resource_exhausted(format!("Encoder admission: {err}")))?;
    if let Some(size) = encoder.encoded_len(&item) {
        // The admission above includes both simultaneous allocations. Fresh exact
        // large buffers avoid doubling an old scratch allocation during growth.
        let limit = max_message_size.unwrap_or(DEFAULT_MAX_SEND_MESSAGE_SIZE);
        if size > limit {
            return Err(Status::out_of_range("message exceeds configured encoding limit"));
        }
        if size > MAX_RETAINED_ENCODER_BUFFER_SIZE {
            if compression_encoding.is_some() {
                *uncompression_buf = BytesMut::with_capacity(size);
                // gzip expansion is bounded by framing; the extra 1/8 plus 64KiB
                // also covers the generic compressor's growth interval rounding.
                let mut next =
                    BytesMut::with_capacity(buf.len().saturating_add(size).saturating_add(size / 8).saturating_add(64 * 1024));
                next.extend_from_slice(buf);
                *buf = next;
            } else {
                let mut next = BytesMut::with_capacity(buf.len().saturating_add(size).saturating_add(HEADER_SIZE));
                next.extend_from_slice(buf);
                *buf = next;
            }
        }
    }
    let offset = buf.len();

    buf.reserve(HEADER_SIZE);
    unsafe {
        buf.advance_mut(HEADER_SIZE);
    }

    if let Some(encoding) = compression_encoding {
        uncompression_buf.clear();

        if let Err(err) = encoder.encode(item, &mut EncodeBuf::new(uncompression_buf)) {
            let partial_len = uncompression_buf.len();
            release_oversized_scratch(uncompression_buf, partial_len, buffer_settings);
            return Err(Status::internal(format!("Error encoding: {err}")));
        }

        let uncompressed_len = uncompression_buf.len();

        let compressed = compress(
            CompressionSettings { encoding, buffer_growth_interval: buffer_settings.buffer_size },
            uncompression_buf,
            buf,
            uncompressed_len,
        );
        release_oversized_scratch(uncompression_buf, uncompressed_len, buffer_settings);
        compressed.map_err(|err| Status::internal(format!("Error compressing: {err}")))?;
    } else {
        encoder.encode(item, &mut EncodeBuf::new(buf)).map_err(|err| Status::internal(format!("Error encoding: {err}")))?;
    }

    // now that we know length, we can write the header
    finish_encoding(compression_encoding, max_message_size, &mut buf[offset..])
}

fn finish_encoding(
    compression_encoding: Option<CompressionEncoding>,
    max_message_size: Option<usize>,
    buf: &mut [u8],
) -> Result<(), Status> {
    let len = buf.len() - HEADER_SIZE;
    let limit = max_message_size.unwrap_or(DEFAULT_MAX_SEND_MESSAGE_SIZE);
    if len > limit {
        return Err(Status::out_of_range(format!(
            "Error, encoded message length too large: found {len} bytes, the limit is: {limit} bytes"
        )));
    }

    if len > u32::MAX as usize {
        return Err(Status::resource_exhausted(format!("Cannot return body with more than 4GB of data but got {len} bytes")));
    }
    {
        let mut buf = &mut buf[..HEADER_SIZE];
        buf.put_u8(compression_encoding.is_some() as u8);
        buf.put_u32(len as u32);
    }

    Ok(())
}

#[derive(Debug)]
enum Role {
    Client,
    Server,
}

/// A specialized implementation of [Body] for encoding [Result<Bytes, Status>].
#[pin_project]
#[derive(Debug)]
pub struct EncodeBody<T, U> {
    #[pin]
    inner: EncodedBytes<T, U>,
    state: EncodeState,
}

#[derive(Debug)]
struct EncodeState {
    error: Option<Status>,
    role: Role,
    is_end_stream: bool,
}

impl<T: Encoder, U: Stream> EncodeBody<T, U> {
    /// Turns a stream of grpc messages into [EncodeBody] which is used by grpc clients for
    /// turning the messages into http frames for sending over the network.
    pub fn new_client(
        encoder: T,
        source: U,
        compression_encoding: Option<CompressionEncoding>,
        max_message_size: Option<usize>,
    ) -> Self {
        Self {
            inner: EncodedBytes::new(
                encoder,
                source,
                compression_encoding,
                SingleMessageCompressionOverride::default(),
                max_message_size,
            ),
            state: EncodeState { error: None, role: Role::Client, is_end_stream: false },
        }
    }

    /// Turns a stream of grpc results (message or error status) into [EncodeBody] which is used by grpc
    /// servers for turning the messages into http frames for sending over the network.
    pub fn new_server(
        encoder: T,
        source: U,
        compression_encoding: Option<CompressionEncoding>,
        compression_override: SingleMessageCompressionOverride,
        max_message_size: Option<usize>,
    ) -> Self {
        Self {
            inner: EncodedBytes::new(encoder, source, compression_encoding, compression_override, max_message_size),
            state: EncodeState { error: None, role: Role::Server, is_end_stream: false },
        }
    }
}

impl EncodeState {
    fn trailers(&mut self) -> Option<Result<HeaderMap, Status>> {
        match self.role {
            Role::Client => None,
            Role::Server => {
                if self.is_end_stream {
                    return None;
                }

                self.is_end_stream = true;
                let status = if let Some(status) = self.error.take() { status } else { Status::ok("") };
                Some(status.to_header_map())
            }
        }
    }
}

impl<T, U> Body for EncodeBody<T, U>
where
    T: Encoder<Error = Status>,
    U: Stream<Item = Result<T::Item, Status>>,
{
    type Data = Bytes;
    type Error = Status;

    fn is_end_stream(&self) -> bool {
        self.state.is_end_stream
    }

    fn poll_frame(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let self_proj = self.project();
        match ready!(self_proj.inner.poll_next(cx)) {
            Some(Ok(d)) => Some(Ok(Frame::data(d))).into(),
            Some(Err(status)) => match self_proj.state.role {
                Role::Client => Some(Err(status)).into(),
                Role::Server => {
                    self_proj.state.is_end_stream = true;
                    Some(Ok(Frame::trailers(status.to_header_map()?))).into()
                }
            },
            None => self_proj.state.trailers().map(|t| t.map(Frame::trailers)).into(),
        }
    }
}

#[cfg(all(test, feature = "gzip"))]
#[path = "oversized_encode_tests.rs"]
mod oversized_encode_tests;
