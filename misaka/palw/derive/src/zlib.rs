//! A zlib stream made of STORED deflate blocks (RFC 1950 + RFC 1951 BTYPE=00). No compression,
//! no Huffman tables, no matcher — and therefore exactly one possible output for a given input,
//! which is what a canonical PNG writer needs (ADR-0078 Decision 8: "fixed zlib level"; the
//! level fixed here is *stored*, the one level whose output does not depend on the encoder's
//! search). Larger files than compression would give, by design: a chart's PNG is kilobytes, and
//! bytes that are the same on every host are worth more than bytes that are few.

/// zlib-wrap `data` as stored blocks of at most 65,535 bytes each.
pub fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 6 + 5 * data.len().div_ceil(65_535).max(1));
    // CMF: CM=8 (deflate), CINFO=7 (32K window). FLG: FLEVEL=0, FDICT=0, FCHECK so that
    // (CMF*256 + FLG) % 31 == 0 → 0x78 0x01.
    out.push(0x78);
    out.push(0x01);
    if data.is_empty() {
        // one empty final stored block
        out.extend_from_slice(&[0x01, 0x00, 0x00, 0xFF, 0xFF]);
    } else {
        let chunks: Vec<&[u8]> = data.chunks(65_535).collect();
        for (i, chunk) in chunks.iter().enumerate() {
            let bfinal = if i + 1 == chunks.len() { 1u8 } else { 0u8 };
            out.push(bfinal); // BTYPE=00 in bits 1-2, BFINAL in bit 0
            let len = chunk.len() as u16;
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&(!len).to_le_bytes());
            out.extend_from_slice(chunk);
        }
    }
    out.extend_from_slice(&crate::checksum::adler32(data).to_be_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_small_streams_have_the_documented_shape() {
        assert_eq!(zlib_stored(b""), vec![0x78, 0x01, 0x01, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x01]);
        let s = zlib_stored(b"abc");
        assert_eq!(&s[..2], &[0x78, 0x01]);
        assert_eq!(&s[2..7], &[0x01, 0x03, 0x00, 0xFC, 0xFF]);
        assert_eq!(&s[7..10], b"abc");
        assert_eq!(&s[10..], &crate::checksum::adler32(b"abc").to_be_bytes());
    }

    #[test]
    fn splits_at_65535() {
        let data = vec![7u8; 65_535 + 10];
        let s = zlib_stored(&data);
        assert_eq!(s[2], 0x00); // first block not final
        let second = 2 + 5 + 65_535;
        assert_eq!(s[second], 0x01); // second block final
        assert_eq!(s.len(), 2 + (5 + 65_535) + (5 + 10) + 4);
    }
}
