//! **A weight-block request rides the interval lane under bit 28** (RFC-0007 Part II, §II.8; node-local, not consensus).
//!
//! A seat whose sketch check failed names the failing free-axis block of one weight product and asks the producer or any holder of the class for
//! exactly that block's weight bytes, opened against the class's `artifact_root`. The request is an interval-opening-request-shaped ask on
//! the lane the other kinds already use, so it inherits their signature, freshness, throttle, byte-allowance and size-cap rules unchanged:
//!
//! * `intervalIndex` = [`PALW_WEIGHT_BLOCK_REQUEST_BIT_V1`] ‖ site ordinal (16 bits) ‖ block (12 bits). Bits 31, 30 and 29 are the block-leaves,
//!   resume and leaf-evidence / segment kinds', so the five kinds cannot collide, and a plain interval never reaches bit 28;
//! * `leafIndex` = the first eight bytes (little endian) of the class's `artifact_root`: it names which held class the site ordinal is read in, and
//!   the signature binds it (the lane's leaf-request tag). The answer is checked against the whole root by the seat, so a prefix collision only costs
//!   a refusal.
//!
//! The answer is one borsh multiproof (`PalwArtifactMultiproofV1`) of the inventory leaves that cover the block's byte ranges, at most `F` = 2 MiB of
//! weight plus its siblings: inside the lane's 4 MiB cap.
//!

use crate::Hash64;

/// Bit 28 of the interval index: a weight-block request.
pub const PALW_WEIGHT_BLOCK_REQUEST_BIT_V1: u32 = 1 << 28;
/// Site ordinals are 16 bits, blocks 12.
pub const PALW_WEIGHT_BLOCK_MAX_SITES_V1: u32 = 1 << 16;
pub const PALW_WEIGHT_BLOCK_MAX_BLOCKS_V1: u32 = 1 << 12;

/// The request index for block `block` of weight site `site`; `None` past the field widths.
pub fn palw_weight_block_request_index_v1(site: u32, block: u32) -> Option<u32> {
    (site < PALW_WEIGHT_BLOCK_MAX_SITES_V1 && block < PALW_WEIGHT_BLOCK_MAX_BLOCKS_V1)
        .then_some(PALW_WEIGHT_BLOCK_REQUEST_BIT_V1 | (site << 12) | block)
}

/// `Some((site, block))` for a weight-block request index, `None` for any other kind (bits 31 to 29 set, or bit 28 clear).
pub fn palw_weight_block_request_decode_v1(index: u32) -> Option<(u32, u32)> {
    (index >> 28 == 1).then_some(((index >> 12) & 0xFFFF, index & 0xFFF))
}

/// Bit 27 of the interval index, bits 31 to 28 clear: **a witness-chunk request** (§II.7). The index is this bit and the chunk number (16 bits, 0-based
/// within the witness; the manifest's chunk `1 + index`); the request names no class — the claim in the message names the witness. The answer is the
/// chunk's bytes. A weight-block request has bit 28 set, so `index >> 27` is 1 only here.
pub const PALW_WITNESS_CHUNK_REQUEST_BIT_V1: u32 = 1 << 27;

/// The request index for witness chunk `chunk`; `None` past 16 bits.
pub fn palw_witness_chunk_request_index_v1(chunk: u32) -> Option<u32> {
    (chunk <= 0xFFFF).then_some(PALW_WITNESS_CHUNK_REQUEST_BIT_V1 | chunk)
}

/// `Some(chunk)` for a witness-chunk request index, `None` for any other kind.
pub fn palw_witness_chunk_request_decode_v1(index: u32) -> Option<u32> {
    (index & !(PALW_WITNESS_CHUNK_REQUEST_BIT_V1 | 0xFFFF) == 0 && index & PALW_WITNESS_CHUNK_REQUEST_BIT_V1 != 0).then_some(index & 0xFFFF)
}

/// The `leafIndex` that names a class by its artifact root.
pub fn palw_weight_block_root_tag_v1(artifact_root: &Hash64) -> u64 {
    let mut b = [0u8; 8];
    b.copy_from_slice(&artifact_root.as_byte_slice()[..8]);
    u64::from_le_bytes(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_index_round_trips_and_collides_with_no_other_kind() {
        for (site, block) in [(0, 0), (1, 0), (65_535, 4_095), (300, 17)] {
            let index = palw_weight_block_request_index_v1(site, block).unwrap();
            assert_eq!(palw_weight_block_request_decode_v1(index), Some((site, block)));
            assert!(crate::palw_leaf_evidence_v1::palw_leaf_evidence_request_decode_v1(index).is_none(), "not leaf evidence");
            assert!(crate::palw_segment_resume_v1::palw_segment_opening_request_decode_v1(index).is_none(), "not a segment opening");
            assert_eq!(index & (7 << 29), 0, "bits 31 to 29 stay clear: not a block-leaves, resume or leaf-evidence request");
        }
        assert_eq!(palw_weight_block_request_index_v1(65_536, 0), None);
        assert_eq!(palw_weight_block_request_index_v1(0, 4_096), None);
        for other in [0u32, 7, 1 << 29, 1 << 30, 1 << 31, (1 << 29) | 5, (1 << 28) | (1 << 29)] {
            assert_eq!(palw_weight_block_request_decode_v1(other), None, "{other:#x}");
        }
    }

    #[test]
    fn a_witness_chunk_index_round_trips_and_is_no_other_kind() {
        for chunk in [0u32, 1, 255, 0xFFFF] {
            let index = palw_witness_chunk_request_index_v1(chunk).unwrap();
            assert_eq!(palw_witness_chunk_request_decode_v1(index), Some(chunk));
            assert!(palw_weight_block_request_decode_v1(index).is_none(), "not a weight block");
            assert!(crate::palw_leaf_evidence_v1::palw_leaf_evidence_request_decode_v1(index).is_none());
            assert!(crate::palw_segment_resume_v1::palw_segment_opening_request_decode_v1(index).is_none());
        }
        assert_eq!(palw_witness_chunk_request_index_v1(0x1_0000), None);
        for other in [0u32, 5, 1 << 28, (1 << 27) | (1 << 28), (1 << 27) | (1 << 16), 1 << 29, 1 << 30, 1 << 31] {
            assert_eq!(palw_witness_chunk_request_decode_v1(other), None, "{other:#x}");
        }
        let w = palw_weight_block_request_index_v1(0x8000, 3).unwrap();
        assert!(palw_witness_chunk_request_decode_v1(w).is_none(), "a high-site weight request is not a witness chunk");
    }

    #[test]
    fn the_root_tag_is_the_roots_first_eight_bytes() {
        let root = Hash64::from_bytes([7; 64]);
        assert_eq!(palw_weight_block_root_tag_v1(&root), u64::from_le_bytes([7; 8]));
        assert_ne!(palw_weight_block_root_tag_v1(&root), palw_weight_block_root_tag_v1(&Hash64::from_bytes([8; 64])));
    }
}
