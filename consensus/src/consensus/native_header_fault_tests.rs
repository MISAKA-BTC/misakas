//! Production get_header versus actual cold RocksDB bytes. Reader fixtures,
//! not accepted blocks, EVM execution or historical signature replay.
use super::*;
use crate::{consensus::test_consensus::TestConsensus, model::stores::headers::HeaderWithBlockLevel};
use kaspa_consensus_core::{
    config::{ConfigBuilder, params::MAINNET_PARAMS},
    header::Header,
};
use kaspa_database::{
    create_temp_db,
    prelude::{ConnBuilder, DbKey},
    registry::DatabaseStorePrefixes,
};
use serde::Serialize;

struct Native {
    node: TestConsensus,
    db: Arc<DB>,
    handles: Vec<std::thread::JoinHandle<()>>,
    _life: kaspa_database::utils::DbLifetime,
}
impl Native {
    fn new() -> Self {
        let (life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let (tx, _rx) = async_channel::unbounded();
        let node = TestConsensus::with_db(db.clone(), &ConfigBuilder::new(MAINNET_PARAMS).build(), tx);
        let handles = node.init();
        Self { node, db, handles, _life: life }
    }
    fn put(&self, prefix: DatabaseStorePrefixes, hash: BlockHash, raw: &[u8]) {
        self.db.put(DbKey::new(&Vec::<u8>::from(prefix), hash), raw).unwrap();
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        self.node.shutdown(std::mem::take(&mut self.handles));
    }
}
fn header(id: u64, daa: u64) -> Header {
    let mut header: Header = (&MAINNET_PARAMS.genesis).into();
    header.hash = BlockHash::from_u64_word(id);
    header.daa_score = daa;
    header
}
// This test-only Serialize layout is the existing fallback HeaderWithBlockLevel2
// (the stored legacy struct is private). It chooses legal empty parents and
// exercises the production decoder; it installs no accepted-chain authority.
#[derive(Serialize)]
struct LegacyHeader<'a> {
    hash: BlockHash,
    version: u16,
    parents_by_level: Vec<Vec<BlockHash>>,
    hash_merkle_root: kaspa_consensus_core::MerkleRoot,
    accepted_id_merkle_root: kaspa_consensus_core::AcceptedIdMerkleRoot,
    utxo_commitment: kaspa_hashes::Hash64,
    timestamp: u64,
    bits: u32,
    nonce: u64,
    daa_score: u64,
    blue_work: &'a kaspa_consensus_core::BlueWorkType,
    blue_score: u64,
    pruning_point: BlockHash,
}
fn legacy(header: &Header) -> Vec<u8> {
    legacy_with_parents(header, vec![])
}
fn legacy_with_parents(header: &Header, parents_by_level: Vec<Vec<BlockHash>>) -> Vec<u8> {
    bincode::serialize(&(
        LegacyHeader {
            hash: header.hash,
            version: header.version,
            parents_by_level,
            hash_merkle_root: header.hash_merkle_root,
            accepted_id_merkle_root: header.accepted_id_merkle_root,
            utxo_commitment: header.utxo_commitment,
            timestamp: header.timestamp,
            bits: header.bits,
            nonce: header.nonce,
            daa_score: header.daa_score,
            blue_work: &header.blue_work,
            blue_score: header.blue_score,
            pruning_point: header.pruning_point,
        },
        0u8,
    ))
    .unwrap()
}

#[test]
fn native_header_cold_missing_codec_fault_and_exact_positive_are_distinct() {
    let n = Native::new();
    let missing = BlockHash::from_u64_word(0x1200);
    assert!(matches!(n.node.get_header(missing), Err(ConsensusError::HeaderNotFound(h)) if h == missing));
    let broken = BlockHash::from_u64_word(0x1201);
    n.put(DatabaseStorePrefixes::CompressedHeaders, broken, &[0xff]);
    assert!(matches!(n.node.get_header(broken), Err(ConsensusError::GeneralOwned(_))));
    for (id, daa) in [(0x1202, 0), (0x1203, 10_001)] {
        let expected = header(id, daa);
        n.put(
            DatabaseStorePrefixes::CompressedHeaders,
            expected.hash,
            &bincode::serialize(&HeaderWithBlockLevel { header: Arc::new(expected.clone()), block_level: 0 }).unwrap(),
        );
        let actual = n.node.get_header(expected.hash).unwrap();
        assert_eq!((actual.hash, actual.daa_score), (expected.hash, daa));
        assert_eq!(
            bincode::serialize(actual.as_ref()).unwrap(),
            bincode::serialize(&expected).unwrap(),
            "the complete header is preserved, with no zero substitute"
        );
    }
}

#[test]
fn native_header_malformed_primary_never_uses_a_positive_legacy_fallback() {
    let n = Native::new();
    let fallback = header(0x1210, 73);
    let wire = legacy(&fallback);
    n.put(DatabaseStorePrefixes::Headers, fallback.hash, &wire);
    let read = n.node.get_header(fallback.hash).unwrap();
    assert_eq!((read.hash, read.daa_score), (fallback.hash, 73), "the old fallback really decodes positively");
    // Another fresh key avoids the successful reader's warm cache. The same
    // independently positive legacy bytes sit behind an authoritative codec fault.
    let shadowed = header(0x1211, 74);
    n.put(DatabaseStorePrefixes::Headers, shadowed.hash, &legacy(&shadowed));
    n.put(DatabaseStorePrefixes::CompressedHeaders, shadowed.hash, &[0xff]);
    assert!(matches!(n.node.get_header(shadowed.hash), Err(ConsensusError::GeneralOwned(_))));
}

#[test]
fn native_header_legacy_parent_levels_reject_overflow_and_preserve_valid_wire() {
    let n = Native::new();
    let parent = BlockHash::from_u64_word(0x1220);
    // The maximum representable legacy vector still reads through the actual
    // fallback. Its expanded levels must match without changing the old bytes.
    let valid = header(0x1221, 255);
    let levels = vec![vec![parent]; usize::from(u8::MAX)];
    let raw = legacy_with_parents(&valid, levels.clone());
    n.put(DatabaseStorePrefixes::Headers, valid.hash, &raw);
    let read = n.node.get_header(valid.hash).unwrap();
    assert_eq!((read.hash, read.daa_score), (valid.hash, 255));
    assert_eq!(read.parents_by_level.expanded_iter().map(<[BlockHash]>::to_vec).collect::<Vec<_>>(), levels);
    assert_eq!(
        n.db.get(DbKey::new(&Vec::<u8>::from(DatabaseStorePrefixes::Headers), valid.hash)).unwrap(),
        Some(raw),
        "a successful legacy read does not rewrite its stored wire representation"
    );

    // This is valid bincode with an unrepresentable level count. Previously
    // decoding succeeded and the conversion's unwrap panicked in get_header.
    let invalid = header(0x1222, 256);
    n.put(DatabaseStorePrefixes::Headers, invalid.hash, &legacy_with_parents(&invalid, vec![vec![parent]; 256]));
    assert!(matches!(n.node.get_header(invalid.hash), Err(ConsensusError::GeneralOwned(_))));
    // Failure must not publish a truncated or empty replacement into its cache.
    assert!(matches!(n.node.get_header(invalid.hash), Err(ConsensusError::GeneralOwned(_))));
}
