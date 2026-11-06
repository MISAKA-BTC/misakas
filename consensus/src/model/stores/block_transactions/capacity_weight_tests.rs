//! The real tracked-bytes cache must evict payloads according to retained RAM,
//! including capacity that does not appear in a serialized transaction.
use super::*;
use kaspa_consensus_core::{
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{ScriptPublicKey, scriptvec},
};
use kaspa_database::{create_temp_db, prelude::ConnBuilder};
use kaspa_utils::mem_size::MemMode;

fn transaction(payload: Vec<u8>) -> Transaction {
    Transaction::new(
        0,
        vec![TransactionInput::new(Default::default(), vec![1; 10], 0, 0)],
        vec![TransactionOutput::new(1, ScriptPublicKey::new(0, scriptvec![0x51]))],
        0,
        SUBNETWORK_ID_NATIVE,
        0,
        payload,
    )
}

#[test]
fn standard_pq_witness_and_spilled_output_are_charged() {
    let mut builder = kaspa_txscript::script_builder::ScriptBuilder::new();
    builder.add_data(&vec![0; kaspa_txscript::MLDSA87_SIG_LEN + 1]).unwrap();
    builder.add_data(&vec![0; kaspa_txscript::MLDSA87_PK_LEN]).unwrap();
    let witness = builder.drain();
    assert_eq!(witness.len(), 7_226, "standard ML-DSA87 P2PKH push layout");
    let output = kaspa_txscript::pay_to_address_script(&kaspa_addresses::Address::new(
        kaspa_addresses::Prefix::Mainnet,
        kaspa_addresses::Version::PubKeyHashMlDsa87,
        &[2; 64],
    ));
    assert_eq!(output.script().len(), 69);
    assert!(output.heap_capacity_bytes() >= 69, "standard PQ output exceeds inline capacity");
    let minimum_heap = witness.capacity() + output.heap_capacity_bytes();
    let tx = Transaction::new(
        0,
        vec![TransactionInput::new(Default::default(), witness, 0, 1)],
        vec![TransactionOutput::new(1, output)],
        0,
        SUBNETWORK_ID_NATIVE,
        0,
        vec![],
    );
    let body = BlockBody(vec![tx].into());
    assert!(body.estimate_mem_bytes() >= minimum_heap, "66-byte signature estimate cannot account for a normal PQ input");
    assert_eq!(
        ScriptPublicKey::new(0, scriptvec![0x51]).heap_capacity_bytes(),
        0,
        "inline script counted only in its containing struct"
    );
}

#[test]
fn payload_script_and_vector_spare_capacity_change_weight_without_changing_wire_bytes() {
    let compact = transaction(vec![3; 4]);
    let mut spare = compact.clone();
    spare.payload.reserve_exact(16 * 1024);
    spare.inputs.reserve_exact(7);
    spare.outputs.reserve_exact(11);
    spare.inputs[0].signature_script.reserve_exact(4096);
    let mut script = Vec::with_capacity(2048);
    script.push(0x51);
    spare.outputs[0].script_public_key = ScriptPublicKey::from_vec(0, script);
    assert!(spare.outputs[0].script_public_key.heap_capacity_bytes() >= 2048);
    let extra = spare.payload.capacity() - compact.payload.capacity()
        + (spare.inputs.capacity() - compact.inputs.capacity()) * size_of::<TransactionInput>()
        + (spare.outputs.capacity() - compact.outputs.capacity()) * size_of::<TransactionOutput>()
        + spare.inputs[0].signature_script.capacity()
        - compact.inputs[0].signature_script.capacity()
        + spare.outputs[0].script_public_key.heap_capacity_bytes();
    let compact = BlockBody(vec![compact].into());
    let mut spare_vec = Vec::with_capacity(17);
    spare_vec.push(spare);
    let extra = extra + (spare_vec.capacity() - compact.0.capacity()) * size_of::<Transaction>();
    let spare = BlockBody(spare_vec.into());
    assert_eq!(bincode::serialize(&compact).unwrap(), bincode::serialize(&spare).unwrap(), "capacity is local only");
    assert!(spare.estimate_mem_bytes() - compact.estimate_mem_bytes() >= extra, "all spare capacity must affect the byte policy");
}

#[test]
fn tracked_byte_cache_releases_oversized_body_and_db_reads_remain_valid() {
    let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let store = DbBlockTransactionsStore::new(db, CachePolicy::Tracked { max_size: 1024, min_items: 0, mem_mode: MemMode::Bytes });
    let mut payload = Vec::with_capacity(128 * 1024);
    payload.resize(64 * 1024, 4);
    let data = Arc::new(vec![transaction(payload)]);
    let weak = Arc::downgrade(&data);
    let hash = BlockHash::from_u64_word(901);
    store.insert(hash, data.into()).unwrap();
    assert!(store.access.read_from_cache(hash).is_none(), "a large payload must not remain under a tiny nominal charge");
    assert!(weak.upgrade().is_none(), "cache eviction releases the last in-memory body owner");
    let loaded = store.get(hash).unwrap();
    assert_eq!(loaded[0].payload, vec![4; 64 * 1024], "cache admission cannot discard a persisted valid body");
    assert!(store.access.read_from_cache(hash).is_none());
}

#[test]
fn byte_cache_retains_only_the_budgeted_number_of_large_bodies() {
    let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let weight = BlockBody(vec![transaction(vec![5; 64 * 1024])].into()).estimate_mem_bytes();
    let store =
        DbBlockTransactionsStore::new(db, CachePolicy::Tracked { max_size: 2 * weight, min_items: 0, mem_mode: MemMode::Bytes });
    let mut witnesses = Vec::new();
    for i in 0..8 {
        let body = Arc::new(vec![transaction(vec![5; 64 * 1024])]);
        witnesses.push(Arc::downgrade(&body));
        store.insert(BlockHash::from_u64_word(1000 + i), body.into()).unwrap();
    }
    assert_eq!(
        witnesses.iter().filter(|weak| weak.upgrade().is_some()).count(),
        2,
        "eviction must follow actual retained capacities rather than item counts or missing payload bytes"
    );
    for i in 0..8 {
        assert!(store.has(BlockHash::from_u64_word(1000 + i)).unwrap());
    }
    drop(store);
    assert!(witnesses.iter().all(|weak| weak.upgrade().is_none()));
}
