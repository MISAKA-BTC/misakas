//! Exact cold original-commitment ownership in the two legacy Source readers.
use crate::{
    Hash64,
    palw_freeprompt_v3::{PALW_FP_V3_DOMAIN_CLAIM_ID, PalwFreePromptCommitmentV3},
    palw_native_resource_v1::{
        NativeCoverageV1, NativeDecodeErrorV1, NativeRetainedV1, preflight_native_value_v1, retain_native_value_decode_classified_v1,
    },
};
use borsh::BorshSerialize;
use std::io::{self, Write};
type Error = NativeDecodeErrorV1<String>;

pub(crate) fn read_v4(raw: Option<&[u8]>) -> Result<Option<NativeRetainedV1<PalwFreePromptCommitmentV3>>, Error> {
    let Some(raw) = raw else { return Ok(None) };
    // Malformed persisted bytes are a decode fault. The second scanner in the
    // retained decoder derives the actual type's complete allocation request;
    // capacity refusal stays a local Resource, never an absent original.
    preflight_native_value_v1::<PalwFreePromptCommitmentV3>(raw, raw.len() as u64).map_err(|why| Error::Decode(why.to_string()))?;
    retain_native_value_decode_classified_v1(raw, || borsh::from_slice(raw).map_err(|why| why.to_string())).map(Some)
}
fn preflight_error_v4(why: io::Error) -> Error {
    match why.kind() {
        io::ErrorKind::InvalidData | io::ErrorKind::UnexpectedEof => Error::Decode(why.to_string()),
        _ => Error::Resource(NativeCoverageV1::unsupported::<PalwFreePromptCommitmentV3>(
            "legacy original native preflight",
            &why.to_string(),
        )),
    }
}
struct HashWriter(blake2b_simd::State);
impl Write for HashWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
/// Identical domain, byte-length prefix and canonical Borsh bytes to
/// fp_claim_id_v3, streamed without a second variable-sized identity buffer.
pub(crate) fn claim_id_v4(original: &PalwFreePromptCommitmentV3) -> Result<Hash64, String> {
    let span = borsh::object_length(original).map_err(|why| why.to_string())?;
    let mut hash = HashWriter(blake2b_simd::Params::new().hash_length(64).key(PALW_FP_V3_DOMAIN_CLAIM_ID).to_state());
    hash.write_all(&(span as u64).to_le_bytes()).map_err(|why| why.to_string())?;
    original.serialize(&mut hash).map_err(|why| why.to_string())?;
    Ok(Hash64::from_bytes(hash.0.finalize().as_bytes().try_into().expect("64 bytes")))
}

#[cfg(test)]
#[test]
fn legacy_original_owner_scanner_local_capacity_and_corruption_remain_distinct() {
    for kind in [io::ErrorKind::OutOfMemory, io::ErrorKind::TimedOut, io::ErrorKind::Unsupported] {
        assert!(matches!(preflight_error_v4(io::Error::new(kind, "local scanner control")), Error::Resource(_)));
    }
    for kind in [io::ErrorKind::InvalidData, io::ErrorKind::UnexpectedEof] {
        assert!(matches!(preflight_error_v4(io::Error::new(kind, "persisted row control")), Error::Decode(_)));
    }
}
