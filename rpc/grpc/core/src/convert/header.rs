use crate::protowire;
use crate::{from, try_from};
use kaspa_consensus_core::header::Header;
use kaspa_rpc_core::{FromRpcHex, RpcError, RpcHash, RpcResult, ToRpcHex};
use std::str::FromStr;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &kaspa_rpc_core::RpcHeader, protowire::RpcBlockHeader, {
    Self {
        version: item.version.into(),
        parents: item.parents_by_level.iter().map(|x| x.as_slice().into()).collect(),
        hash_merkle_root: item.hash_merkle_root.to_string(),
        accepted_id_merkle_root: item.accepted_id_merkle_root.to_string(),
        utxo_commitment: item.utxo_commitment.to_string(),
        timestamp: item.timestamp.try_into().expect("timestamp is always convertible to i64"),
        bits: item.bits,
        nonce: item.nonce,
        daa_score: item.daa_score,
        blue_work: item.blue_work.to_rpc_hex(),
        blue_score: item.blue_score,
        pruning_point: item.pruning_point.to_string(),
        pow_algo_id: item.pow_algo_id as u32,
        hash: item.hash.to_string(),
        // kaspa-pq EVM Lane v0.4: both EVM commitments (v2+ hash preimage).
        evm_payload_hash: item.evm_payload_hash.to_string(),
        evm_commitment_root: item.evm_commitment_root.to_string(),
        // kaspa-pq ADR-0022: the overlay-state commitment (every-version preimage).
        overlay_commitment_root: item.overlay_commitment_root.to_string(),
    }
});

from!(item: &kaspa_rpc_core::RpcRawHeader, protowire::RpcBlockHeader, {
    Self {
        hash: Default::default(), // We don't include the hash for the raw header
        version: item.version.into(),
        parents: item.parents_by_level.iter().map(|x| x.as_slice().into()).collect(),
        hash_merkle_root: item.hash_merkle_root.to_string(),
        accepted_id_merkle_root: item.accepted_id_merkle_root.to_string(),
        utxo_commitment: item.utxo_commitment.to_string(),
        timestamp: item.timestamp.try_into().expect("timestamp is always convertible to i64"),
        bits: item.bits,
        nonce: item.nonce,
        daa_score: item.daa_score,
        blue_work: item.blue_work.to_rpc_hex(),
        blue_score: item.blue_score,
        pruning_point: item.pruning_point.to_string(),
        pow_algo_id: item.pow_algo_id as u32,
        // kaspa-pq EVM Lane v0.4: both EVM commitments (v2+ hash preimage).
        evm_payload_hash: item.evm_payload_hash.to_string(),
        evm_commitment_root: item.evm_commitment_root.to_string(),
        // kaspa-pq ADR-0022: the overlay-state commitment (every-version preimage).
        overlay_commitment_root: item.overlay_commitment_root.to_string(),
    }
});

/// Parse a 128-char hex Hash64, treating absent/empty (an old peer) as zero —
/// matching every v0/v1 header where the EVM commitments are hash-invisible.
fn hash64_or_zero(s: &str) -> Result<kaspa_consensus_core::Hash64, faster_hex::Error> {
    if s.is_empty() { Ok(Default::default()) } else { kaspa_consensus_core::Hash64::from_str(s) }
}

from!(item: &[RpcHash], protowire::RpcBlockLevelParents, { Self { parent_hashes: item.iter().map(|x| x.to_string()).collect() } });

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: &protowire::RpcBlockHeader, kaspa_rpc_core::RpcHeader, {
    // We re-hash the block to remain as most trustless as possible.
    // PR-9.5c/f: hash_merkle_root / accepted_id_merkle_root are now
    // Hash64 (128-char hex on the wire); the rest stay 32-byte RpcHash.
    let header = Header::new_finalized(
        item.version.try_into()?,
        item.parents.iter().map(Vec::<RpcHash>::try_from).collect::<RpcResult<Vec<Vec<RpcHash>>>>()?.try_into()?,
        kaspa_consensus_core::Hash64::from_str(&item.hash_merkle_root)?,
        kaspa_consensus_core::Hash64::from_str(&item.accepted_id_merkle_root)?,
        kaspa_consensus_core::Hash64::from_str(&item.utxo_commitment)? /* kaspa-pq (ADR-0004 / design §12): utxo_commitment is 64-byte Hash64 */,
        item.timestamp.try_into()?,
        item.bits,
        item.nonce,
        // kaspa-pq Phase 2 (ADR-0007): carry the declared Layer-1 algo id through the proto.
        item.pow_algo_id as u8,
        item.daa_score,
        kaspa_rpc_core::RpcBlueWorkType::from_rpc_hex(&item.blue_work)?,
        item.blue_score,
        RpcHash::from_str(&item.pruning_point)?,
    )
    // kaspa-pq EVM Lane v0.4: restore the EVM commitments BEFORE the trustless
    // re-hash — on a v2 header they are part of the preimage.
    .with_evm_payload_hash(hash64_or_zero(&item.evm_payload_hash)?)
    .with_evm_commitment(hash64_or_zero(&item.evm_commitment_root)?)
    // kaspa-pq ADR-0022: include the overlay commitment in the trustless re-hash.
    .with_overlay_commitment(hash64_or_zero(&item.overlay_commitment_root)?);

    header.into()
});

try_from!(item: &protowire::RpcBlockHeader, kaspa_rpc_core::RpcRawHeader, {
    Self {
        version: item.version.try_into()?,
        parents_by_level: item.parents.iter().map(Vec::<RpcHash>::try_from).collect::<RpcResult<Vec<Vec<RpcHash>>>>()?,
        // PR-9.5c/f: merkle roots widened to Hash64.
        hash_merkle_root: kaspa_consensus_core::Hash64::from_str(&item.hash_merkle_root)?,
        accepted_id_merkle_root: kaspa_consensus_core::Hash64::from_str(&item.accepted_id_merkle_root)?,
        utxo_commitment: kaspa_consensus_core::Hash64::from_str(&item.utxo_commitment)? /* kaspa-pq (ADR-0004 / design §12): utxo_commitment is 64-byte Hash64 */,
        timestamp: item.timestamp.try_into()?,
        bits: item.bits,
        nonce: item.nonce,
        daa_score: item.daa_score,
        blue_work: kaspa_rpc_core::RpcBlueWorkType::from_rpc_hex(&item.blue_work)?,
        blue_score: item.blue_score,
        pruning_point: RpcHash::from_str(&item.pruning_point)?,
        pow_algo_id: item.pow_algo_id as u8,
        evm_payload_hash: hash64_or_zero(&item.evm_payload_hash)?,
        evm_commitment_root: hash64_or_zero(&item.evm_commitment_root)?,
        // kaspa-pq ADR-0022: the overlay-state commitment (every-version preimage).
        overlay_commitment_root: hash64_or_zero(&item.overlay_commitment_root)?,
    }
});

try_from!(item: &protowire::RpcBlockHeader, kaspa_rpc_core::RpcOptionalHeader, {
    // We re-hash the block to remain as most trustless as possible.
    // PR-9.5c/f: merkle roots widened to Hash64.
    let header = Header::new_finalized(
        item.version.try_into()?,
        item.parents.iter().map(Vec::<RpcHash>::try_from).collect::<RpcResult<Vec<Vec<RpcHash>>>>()?.try_into()?,
        kaspa_consensus_core::Hash64::from_str(&item.hash_merkle_root)?,
        kaspa_consensus_core::Hash64::from_str(&item.accepted_id_merkle_root)?,
        kaspa_consensus_core::Hash64::from_str(&item.utxo_commitment)? /* kaspa-pq (ADR-0004 / design §12): utxo_commitment is 64-byte Hash64 */,
        item.timestamp.try_into()?,
        item.bits,
        item.nonce,
        // kaspa-pq Phase 2 (ADR-0007): carry the declared Layer-1 algo id through the proto.
        item.pow_algo_id as u8,
        item.daa_score,
        kaspa_rpc_core::RpcBlueWorkType::from_rpc_hex(&item.blue_work)?,
        item.blue_score,
        RpcHash::from_str(&item.pruning_point)?,
    )
    // kaspa-pq EVM Lane v0.4: include the commitments in the trustless re-hash.
    .with_evm_payload_hash(hash64_or_zero(&item.evm_payload_hash)?)
    .with_evm_commitment(hash64_or_zero(&item.evm_commitment_root)?)
    // kaspa-pq ADR-0022: include the overlay commitment in the trustless re-hash.
    .with_overlay_commitment(hash64_or_zero(&item.overlay_commitment_root)?);

    kaspa_rpc_core::RpcOptionalHeader::from(header)
});

try_from!(item: &protowire::RpcBlockLevelParents, Vec<RpcHash>, {
    item.parent_hashes.iter().map(|x| RpcHash::from_str(x)).collect::<Result<Vec<_>, _>>()?
});

#[cfg(test)]
mod tests {
    use crate::protowire;
    use itertools::Itertools;
    use kaspa_consensus_core::{block::Block, header::Header};
    use kaspa_rpc_core::{RpcBlock, RpcHash, RpcHeader};

    fn new_unique() -> RpcHash {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        let c = COUNTER.fetch_add(1, Ordering::Relaxed);
        RpcHash::from_u64_word(c)
    }

    // PR-9.5c/f: Hash64 unique generator for the merkle-root header
    // positions (hash_merkle_root / accepted_id_merkle_root), which
    // widened to Hash64. The low 8 bytes carry the counter so values
    // stay distinct across calls.
    fn new_unique_hash64() -> kaspa_consensus_core::Hash64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        let c = COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut bytes = [0u8; 64];
        bytes[..8].copy_from_slice(&c.to_le_bytes());
        kaspa_consensus_core::Hash64::from_bytes(bytes)
    }

    // 32-byte unique generator (`Hash`). Retained as a helper; the
    // utxo_commitment header position is now 64-byte `Hash64` (see
    // `new_unique_hash64`), as are the block ids / merkle roots.
    #[allow(dead_code)]
    fn new_unique_hash32() -> kaspa_consensus_core::Hash {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        let c = COUNTER.fetch_add(1, Ordering::Relaxed);
        kaspa_consensus_core::Hash::from_u64_word(c)
    }

    fn test_parents_by_level_rxr(rpc_parents_1: &[Vec<RpcHash>], rpc_parents_2: &[Vec<RpcHash>]) {
        assert_eq!(rpc_parents_1, rpc_parents_2);
    }
    fn test_parents_by_level_rxp(rpc_parents: &[Vec<RpcHash>], proto_parents: &[protowire::RpcBlockLevelParents]) {
        for (r_level_parents, proto_level_parents) in rpc_parents.iter().zip_eq(proto_parents.iter()) {
            for (r_parent, proto_parent) in r_level_parents.iter().zip_eq(proto_level_parents.parent_hashes.iter()) {
                assert_eq!(r_parent.to_string(), *proto_parent);
            }
        }
    }

    #[test]
    fn test_rpc_block_level_parents() {
        let proto_block_level_parents = protowire::RpcBlockLevelParents {
            parent_hashes: vec![new_unique().to_string(), new_unique().to_string(), new_unique().to_string()],
        };
        let rpc_block_level_parents: Vec<RpcHash> = (&proto_block_level_parents).try_into().unwrap();
        let proto_block_level_parents_reconverted: protowire::RpcBlockLevelParents = rpc_block_level_parents.as_slice().into();
        for (i, _) in rpc_block_level_parents.iter().enumerate() {
            assert_eq!(proto_block_level_parents.parent_hashes[i], rpc_block_level_parents[i].to_string());
            assert_eq!(proto_block_level_parents_reconverted.parent_hashes[i], rpc_block_level_parents[i].to_string());
            assert_eq!(proto_block_level_parents.parent_hashes[i], proto_block_level_parents_reconverted.parent_hashes[i]);
        }
        assert_eq!(proto_block_level_parents, proto_block_level_parents_reconverted);

        let rpc_block_level_parents: Vec<RpcHash> = vec![new_unique(), new_unique()];
        let proto_block_level_parents: protowire::RpcBlockLevelParents = rpc_block_level_parents.as_slice().into();
        let rpc_block_level_parents_reconverted: Vec<RpcHash> = (&proto_block_level_parents).try_into().unwrap();

        assert_eq!(rpc_block_level_parents, rpc_block_level_parents_reconverted);
        for ((p_hash, r1_hash), r2_hash) in
            proto_block_level_parents.parent_hashes.iter().zip_eq(rpc_block_level_parents).zip_eq(rpc_block_level_parents_reconverted)
        {
            assert_eq!(p_hash, &r1_hash.to_string());
            assert_eq!(p_hash, &r2_hash.to_string());
            assert_eq!(r1_hash, r2_hash);
        }
    }

    #[test]
    fn test_rpc_header() {
        let header = Header::new_finalized(
            0,
            vec![vec![new_unique(), new_unique(), new_unique()], vec![new_unique()], vec![new_unique(), new_unique()]]
                .try_into()
                .unwrap(),
            // PR-9.5c/f: hash_merkle_root + accepted_id_merkle_root are Hash64.
            // kaspa-pq (ADR-0004 / design §12): utxo_commitment (3rd) is also Hash64.
            new_unique_hash64(),
            new_unique_hash64(),
            new_unique_hash64(),
            123,
            12345,
            98765,
            // PR-9.5d: pow_algo_id (Phase 1 kHeavyHash = 1).
            kaspa_consensus_core::pow_layer0::POW_ALGO_ID_KHEAVYHASH,
            120055,
            459912.into(),
            1928374,
            new_unique(),
        );
        let rpc_header = RpcHeader::from(header);
        let proto_header: protowire::RpcBlockHeader = (&rpc_header).into();
        let reconverted_rpc_header: RpcHeader = (&proto_header).try_into().unwrap();
        let reconverted_proto_header: protowire::RpcBlockHeader = (&reconverted_rpc_header).into();

        assert_eq!(rpc_header.parents_by_level, reconverted_rpc_header.parents_by_level);
        assert_eq!(proto_header.parents, reconverted_proto_header.parents.to_vec());
        test_parents_by_level_rxr(&rpc_header.parents_by_level, &reconverted_rpc_header.parents_by_level);
        test_parents_by_level_rxp(&rpc_header.parents_by_level, &proto_header.parents);
        test_parents_by_level_rxp(&rpc_header.parents_by_level, &reconverted_proto_header.parents);
        test_parents_by_level_rxp(&reconverted_rpc_header.parents_by_level, &reconverted_proto_header.parents);

        assert_eq!(rpc_header.hash, reconverted_rpc_header.hash);
        assert_eq!(proto_header, reconverted_proto_header);
    }

    #[test]
    fn test_rpc_block() {
        let header = Header::new_finalized(
            0,
            vec![vec![new_unique(), new_unique(), new_unique()], vec![new_unique()], vec![new_unique(), new_unique()]]
                .try_into()
                .unwrap(),
            // PR-9.5c/f: hash_merkle_root + accepted_id_merkle_root are Hash64.
            // kaspa-pq (ADR-0004 / design §12): utxo_commitment (3rd) is also Hash64.
            new_unique_hash64(),
            new_unique_hash64(),
            new_unique_hash64(),
            123,
            12345,
            98765,
            // PR-9.5d: pow_algo_id (Phase 1 kHeavyHash = 1).
            kaspa_consensus_core::pow_layer0::POW_ALGO_ID_KHEAVYHASH,
            120055,
            459912.into(),
            1928374,
            new_unique(),
        );
        let consensus_block = Block::from_header(header);
        let rpc_block: RpcBlock = (&consensus_block).into();
        let proto_block: protowire::RpcBlock = (&rpc_block).into();
        let rpc_block_converted_from_proto: RpcBlock = (&proto_block).try_into().unwrap();
        let consensus_block_reconverted: Block = rpc_block_converted_from_proto.clone().try_into().unwrap();
        let rpc_block_reconverted_from_consensus: RpcBlock = (&consensus_block_reconverted).into();
        let proto_block_reconverted: protowire::RpcBlock = (&rpc_block_reconverted_from_consensus).into();
        let consensus_parents = Vec::from(&consensus_block.header.parents_by_level);
        let consensus_reconverted_parents = Vec::from(&consensus_block_reconverted.header.parents_by_level);

        assert_eq!(rpc_block.header.parents_by_level, rpc_block_converted_from_proto.header.parents_by_level);
        assert_eq!(proto_block.header.as_ref().unwrap().parents, proto_block_reconverted.header.as_ref().unwrap().parents);
        test_parents_by_level_rxr(&rpc_block.header.parents_by_level, &rpc_block_converted_from_proto.header.parents_by_level);
        test_parents_by_level_rxr(&rpc_block.header.parents_by_level, &rpc_block_reconverted_from_consensus.header.parents_by_level);
        test_parents_by_level_rxr(&consensus_parents, &rpc_block_converted_from_proto.header.parents_by_level);
        test_parents_by_level_rxr(&consensus_parents, &consensus_reconverted_parents);
        test_parents_by_level_rxp(&rpc_block.header.parents_by_level, &proto_block.header.as_ref().unwrap().parents);
        test_parents_by_level_rxp(&rpc_block.header.parents_by_level, &proto_block_reconverted.header.as_ref().unwrap().parents);
        test_parents_by_level_rxp(
            &rpc_block_converted_from_proto.header.parents_by_level,
            &proto_block_reconverted.header.as_ref().unwrap().parents,
        );

        assert_eq!(consensus_block.hash(), consensus_block_reconverted.hash());
        assert_eq!(proto_block, proto_block_reconverted);
    }
}
