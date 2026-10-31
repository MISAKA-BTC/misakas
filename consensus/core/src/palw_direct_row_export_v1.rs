//! Dormant RFC0037 collection exporter. The canonical root writer supplies
//! collection offsets and borrowed row emitters, so this does not duplicate the
//! fence-dependent collection inventory. Nested engines use their existing root grammars.
use crate::palw_row_stream_v1::*;
use crate::palw_state_v2::PalwChainStateV2;
use std::io::{self, Read, Write};

#[derive(Default)]
struct Count(u64);
impl Write for Count {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self.0.checked_add(bytes.len() as u64).ok_or_else(|| io::Error::other("row generation length overflow"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Write a generation containing every direct and supported nested collection.
/// No whole table or generation is buffered. The caller owns a staging sink and
/// must discard partial output on error; this function does not promote storage
/// or bypass the receiver's whole-state consistency/import checks.
///
/// Kernel ledger rows, native auxiliary rows, panel serialization chunks and
/// nested bond-budget income tables retain their frozen commitment formats.
/// Unknown kernel table numbers are refused before writing any bytes.
pub fn write_direct_row_generation_v1(state: &PalwChainStateV2, suffix: &[u8], sink: &mut impl Write) -> io::Result<u64> {
    write_row_generation_stream_v1(state, &mut std::io::Cursor::new(suffix), suffix.len() as u64, sink)
}

/// File-backed suffix variant used by the negotiated P2P serving path. No
/// whole encoded native snapshot is copied into RAM to append the envelope.
pub fn write_row_generation_stream_v1(
    state: &PalwChainStateV2,
    mut suffix: &mut dyn std::io::Read,
    suffix_len: u64,
    sink: &mut dyn Write,
) -> io::Result<u64> {
    state.require_direct_row_export_v1()?;
    let mut count = Count::default();
    let mut collections = 0u32;
    let (preimage, _) = state.visit_direct_state_root_collections_v1(|label, _offset, _rows, emit| {
        collections = collections.checked_add(1).ok_or_else(|| io::Error::other("row collection count overflow"))?;
        count.0 =
            count.0.checked_add(4 + label.len() as u64 + 16).ok_or_else(|| io::Error::other("row generation length overflow"))?;
        emit(&mut count)
    })?;
    if preimage.len() > PALW_ROW_STREAM_MAX_PREIMAGE_BYTES_V1 {
        return Err(io::Error::other("row preimage exceeds native bound"));
    }
    let stream_len =
        count.0.checked_add(4 + preimage.len() as u64 + 4 + 1).ok_or_else(|| io::Error::other("row generation length overflow"))?;
    let total = stream_len
        .checked_add(4 + 2 + 8 + 8 + 8)
        .and_then(|n| n.checked_add(suffix_len))
        .filter(|n| *n <= PALW_ROW_STREAM_MAX_GENERATION_BYTES_V1 as u64)
        .ok_or_else(|| io::Error::other("row generation exceeds native bound"))?;
    sink.write_all(&PALW_ROW_STREAM_GENERATION_MAGIC_V1.to_le_bytes())?;
    sink.write_all(&PALW_ROW_STREAM_GENERATION_VERSION_V1.to_le_bytes())?;
    sink.write_all(&PALW_ROW_STREAM_CURRENT_GENERATION_V1.to_le_bytes())?;
    sink.write_all(&stream_len.to_le_bytes())?;
    sink.write_all(&(preimage.len() as u32).to_le_bytes())?;
    sink.write_all(&preimage)?;
    sink.write_all(&collections.to_le_bytes())?;
    state.visit_direct_state_root_collections_v1(|label, offset, rows, emit| {
        sink.write_all(&(label.len() as u32).to_le_bytes())?;
        sink.write_all(label)?;
        sink.write_all(&rows.to_le_bytes())?;
        sink.write_all(&(offset as u64).to_le_bytes())?;
        emit(sink)
    })?;
    sink.write_all(&[PALW_ROW_SPILL_END_V1])?;
    sink.write_all(&suffix_len.to_le_bytes())?;
    if io::copy(&mut (&mut suffix).take(suffix_len), sink)? != suffix_len {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "native suffix truncated"));
    }
    let mut extra = [0];
    if suffix.read(&mut extra)? != 0 {
        return Err(io::Error::other("native suffix has trailing bytes"));
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rfc37_a_valid_native_suffix_cannot_hide_an_omitted_collection_inventory() {
        let state = PalwChainStateV2::genesis();
        let preimage = state.state_root_preimage();
        let wire = encode_row_stream_generation_v1(&preimage, &[], b"complete-native-envelope").unwrap();
        let suffix =
            palw_verify_row_generation_reader_v1(state.state_root(), &mut std::io::Cursor::new(&wire), wire.len() as u64).unwrap();
        assert_eq!(suffix.inventory.state_root(), state.state_root(), "the preimage remains valid");
        assert!(!suffix.inventory.matches_state(&state).unwrap(), "the suffix never substitutes for all observed collection roots");
    }
    #[test]
    fn direct_row_export_offsets_and_generation_match_the_canonical_root_writer() {
        let state = PalwChainStateV2::genesis();
        let expected = state.state_root_preimage_located_v1();
        let mut seen = 0;
        let visited = state
            .visit_direct_state_root_collections_v1(|label, offset, rows, emit| {
                let mut wire = Vec::new();
                emit(&mut wire)?;
                assert_eq!(rows, 0);
                let root = PalwCollectionRootStreamV1::new(label, rows).finish().unwrap();
                assert_eq!(&expected.0[offset..offset + 64], root.as_byte_slice());
                seen += 1;
                Ok(())
            })
            .unwrap();
        assert!(seen > 10);
        assert_eq!(visited, expected, "the visitor must be the same consensus preimage writer");
        let mut bytes = Vec::new();
        let len = write_direct_row_generation_v1(&state, b"native-envelope", &mut bytes).unwrap();
        assert_eq!(len as usize, bytes.len());
        let accepted = palw_accept_row_stream_generation_v1(state.state_root(), &bytes, io::sink(), len, None).unwrap();
        assert_eq!(accepted.import_suffix(), b"native-envelope");
    }
}

/// Preferred name after nested-engine coverage was added.
pub use write_direct_row_generation_v1 as write_row_generation_v1;
