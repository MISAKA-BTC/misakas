//! **A weight-block request rides the interval lane under bit 26** (RFC-0007 Part II, §II.8; node-local, not consensus).
//!
//! A seat whose sketch check failed names the failing free-axis block of one weight product and asks the producer or any holder of the class for
//! exactly that block's weight bytes, opened against the class's `artifact_root`. The request is an interval-opening-request-shaped ask on
//! the lane the other kinds already use, so it inherits their signature, freshness, throttle, byte-allowance and size-cap rules unchanged:
//!
//! * `intervalIndex` = [`PALW_WEIGHT_BLOCK_REQUEST_BIT_V1`] ‖ site ordinal (14 bits) ‖ block (12 bits). Bits 31 to 27 belong to the other kinds
//!   (block-leaves, resume, leaf-evidence / segment, shard inventory rows at 28, step runs at 27), and bit 25 to the witness-chunk request, so the
//!   kinds cannot collide, and a plain interval never reaches bit 25;
//! * `leafIndex` = the first eight bytes (little endian) of the class's `artifact_root`: it names which held class the site ordinal is read in, and
//!   the signature binds it (the lane's leaf-request tag). The answer is checked against the whole root by the seat, so a prefix collision only costs
//!   a refusal.
//!
//! The answer is one borsh multiproof (`PalwArtifactMultiproofV1`) of the inventory leaves that cover the block's byte ranges, at most `F` = 2 MiB of
//! weight plus its siblings: inside the lane's 4 MiB cap.
//!

use crate::Hash64;

/// Bit 26 of the interval index (bits 31 to 27 and 25 clear): a weight-block request.
pub const PALW_WEIGHT_BLOCK_REQUEST_BIT_V1: u32 = 1 << 26;
/// Site ordinals are 14 bits (bits 12 to 25), blocks 12.
pub const PALW_WEIGHT_BLOCK_MAX_SITES_V1: u32 = 1 << 14;
pub const PALW_WEIGHT_BLOCK_MAX_BLOCKS_V1: u32 = 1 << 12;

/// The request index for block `block` of weight site `site`; `None` past the field widths.
pub fn palw_weight_block_request_index_v1(site: u32, block: u32) -> Option<u32> {
    (site < PALW_WEIGHT_BLOCK_MAX_SITES_V1 && block < PALW_WEIGHT_BLOCK_MAX_BLOCKS_V1)
        .then_some(PALW_WEIGHT_BLOCK_REQUEST_BIT_V1 | (site << 12) | block)
}

/// `Some((site, block))` for a weight-block request index, `None` for any other kind (any of bits 31 to 27 set, or bit 26 clear).
pub fn palw_weight_block_request_decode_v1(index: u32) -> Option<(u32, u32)> {
    (index >> 26 == 1).then_some(((index >> 12) & 0x3FFF, index & 0xFFF))
}

/// Bit 25 of the interval index, bits 31 to 26 and 16 to 24 clear: **a witness-chunk request** (§II.7). The index is this bit and the chunk number
/// (16 bits, 0-based within the witness; the manifest's chunk `1 + index`); the request names no class — the claim in the message names the witness. The
/// answer is the chunk's bytes.
pub const PALW_WITNESS_CHUNK_REQUEST_BIT_V1: u32 = 1 << 25;

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
    fn the_index_round_trips() {
        for (site, block) in [(0, 0), (1, 0), (16_383, 4_095), (300, 17)] {
            let index = palw_weight_block_request_index_v1(site, block).unwrap();
            assert_eq!(palw_weight_block_request_decode_v1(index), Some((site, block)));
            assert_eq!(index & (0x1F << 27), 0, "bits 31 to 27 stay clear");
        }
        assert_eq!(palw_weight_block_request_index_v1(16_384, 0), None);
        assert_eq!(palw_weight_block_request_index_v1(0, 4_096), None);
    }

    #[test]
    fn a_witness_chunk_index_round_trips() {
        for chunk in [0u32, 1, 255, 0xFFFF] {
            let index = palw_witness_chunk_request_index_v1(chunk).unwrap();
            assert_eq!(palw_witness_chunk_request_decode_v1(index), Some(chunk));
        }
        assert_eq!(palw_witness_chunk_request_index_v1(0x1_0000), None);
    }

    /// **Every request kind of the interval lane is disjoint from every other**: RFC-0007's two (bits 26 and 25), lane S's shard inventory rows (bit 28)
    /// and step runs (bit 27), the leaf-evidence and segment kinds (bit 29), block-leaves and resume (bits 31, 30) and the plain interval. Each decoder
    /// refuses the others' indices.
    #[test]
    fn the_request_kinds_are_mutually_disjoint_and_each_decoder_refuses_the_others() {
        use crate::palw_leaf_evidence_v1::{palw_leaf_evidence_request_decode_v1, palw_leaf_evidence_request_index_v1};
        use crate::palw_segment_resume_v1::palw_segment_opening_request_decode_v1;
        let weight = palw_weight_block_request_index_v1(0x3FFF, 0xFFF).unwrap();
        let weight_low = palw_weight_block_request_index_v1(0, 0).unwrap();
        let witness = palw_witness_chunk_request_index_v1(0xFFFF).unwrap();
        let witness_low = palw_witness_chunk_request_index_v1(0).unwrap();
        let leaf = palw_leaf_evidence_request_index_v1(5).unwrap();
        let shard_rows = 1u32 << 28; // lane S: shard inventory rows
        let step_runs = 1u32 << 27; // lane S: TirStepRun runs
        let block_leaves = 1u32 << 31;
        let resume = 1u32 << 30;
        let plain = 17u32;
        // The bit each kind owns, and that no kind owns another's.
        for (name, index, owns) in [
            ("weight", weight, 26u32),
            ("weight", weight_low, 26),
            ("witness", witness, 25),
            ("witness", witness_low, 25),
        ] {
            assert_ne!(index & (1 << owns), 0, "{name} owns bit {owns}");
            for foreign in [27u32, 28, 29, 30, 31] {
                assert_eq!(index & (1 << foreign), 0, "{name} never sets bit {foreign}");
            }
        }
        assert_eq!(witness & (1 << 26), 0, "a witness index never sets the weight bit");
        assert_ne!(weight & (1 << 25), 0, "a high-site weight index does set bit 25, and that is why the witness decoder also requires bit 26 clear");
        // Each decoder refuses every other kind's index.
        for index in [witness, witness_low, leaf, shard_rows, step_runs, block_leaves, resume, plain, shard_rows | 3, step_runs | 3] {
            assert_eq!(palw_weight_block_request_decode_v1(index), None, "weight refuses {index:#x}");
        }
        for index in [weight, weight_low, leaf, shard_rows, step_runs, block_leaves, resume, plain, shard_rows | 3, step_runs | 3] {
            assert_eq!(palw_witness_chunk_request_decode_v1(index), None, "witness refuses {index:#x}");
        }
        for index in [weight, weight_low, witness, witness_low, shard_rows, step_runs, block_leaves, resume, plain] {
            assert_eq!(palw_leaf_evidence_request_decode_v1(index), None, "leaf evidence refuses {index:#x}");
            assert_eq!(palw_segment_opening_request_decode_v1(index), None, "a segment opening refuses {index:#x}");
        }
        assert_eq!(palw_leaf_evidence_request_decode_v1(leaf), Some(5), "and the plain leaf request still decodes");
    }

    #[test]
    fn the_root_tag_is_the_roots_first_eight_bytes() {
        let root = Hash64::from_bytes([7; 64]);
        assert_eq!(palw_weight_block_root_tag_v1(&root), u64::from_le_bytes([7; 8]));
        assert_ne!(palw_weight_block_root_tag_v1(&root), palw_weight_block_root_tag_v1(&Hash64::from_bytes([8; 64])));
    }
}
