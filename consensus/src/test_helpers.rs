use kaspa_consensus_core::{
    BlockHash,
    block::Block,
    header::Header,
    subnets::SubnetworkId,
    tx::{ScriptPublicKey, ScriptVec, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry},
    utxo::utxo_collection::UtxoCollection,
};
use kaspa_hashes::{HASH_SIZE, Hash};
use rand::{Rng, rngs::SmallRng, seq::SliceRandom};

pub fn header_from_precomputed_hash(hash: BlockHash, parents: Vec<BlockHash>) -> Header {
    Header::from_precomputed_hash(hash, parents)
}

pub fn block_from_precomputed_hash(hash: BlockHash, parents: Vec<BlockHash>) -> Block {
    Block::from_precomputed_hash(hash, parents)
}

pub fn generate_random_utxos_from_script_public_key_pool(
    rng: &mut SmallRng,
    amount: usize,
    script_public_key_pool: &[ScriptPublicKey],
) -> UtxoCollection {
    let mut i = 0;
    let mut collection = UtxoCollection::with_capacity(amount);
    while i < amount {
        collection
            .insert(generate_random_outpoint(rng), generate_random_utxo_from_script_public_key_pool(rng, script_public_key_pool));
        i += 1;
    }
    collection
}

pub fn generate_random_hash(rng: &mut SmallRng) -> Hash {
    let random_bytes = rng.r#gen::<[u8; HASH_SIZE]>();
    Hash::from_bytes(random_bytes)
}

/// PR-9.5c: `TransactionOutpoint::new` now takes `TransactionId`
/// (Hash64); generate a random 64-byte hash inline. `rand`'s
/// `Standard` distribution only impls `Distribution<[u8; N]>`
/// for `N ≤ 32`, so the 64-byte fill is via two 32-byte gens.
pub fn generate_random_outpoint(rng: &mut SmallRng) -> TransactionOutpoint {
    TransactionOutpoint::new(generate_random_hash64(rng), rng.r#gen::<u32>())
}

/// Internal: random 64-byte `Hash64` for PR-9.5c+ test helpers.
fn generate_random_hash64(rng: &mut SmallRng) -> kaspa_hashes::Hash64 {
    let mut bytes64 = [0u8; kaspa_hashes::HASH64_SIZE];
    let first: [u8; 32] = rng.r#gen();
    let second: [u8; 32] = rng.r#gen();
    bytes64[..32].copy_from_slice(&first);
    bytes64[32..].copy_from_slice(&second);
    kaspa_hashes::Hash64::from_bytes(bytes64)
}

pub fn generate_random_utxo_from_script_public_key_pool(rng: &mut SmallRng, script_public_key_pool: &[ScriptPublicKey]) -> UtxoEntry {
    UtxoEntry::new(
        rng.gen_range(1..100_000), //we choose small amounts as to not overflow with large utxosets.
        script_public_key_pool.choose(rng).expect("expected_script_public key").clone(),
        rng.r#gen(),
        rng.gen_bool(0.5),
    )
}

pub fn generate_random_utxo(rng: &mut SmallRng) -> UtxoEntry {
    UtxoEntry::new(
        rng.gen_range(1..100_000), //we choose small amounts as to not overflow with large utxosets.
        generate_random_p2pk_script_public_key(rng),
        rng.r#gen(),
        rng.gen_bool(0.5),
    )
}

///Note: this generates schnorr p2pk script public keys.
pub fn generate_random_p2pk_script_public_key(rng: &mut SmallRng) -> ScriptPublicKey {
    let mut script: ScriptVec = (0..32).map(|_| rng.r#gen()).collect();
    script.insert(0, 0x20);
    script.push(0xac);
    ScriptPublicKey::new(0_u16, script)
}

// PR-9.5e: block hashes are now `BlockHash` (Hash64); used for `parents_by_level` below.
pub fn generate_random_hashes(rng: &mut SmallRng, amount: usize) -> Vec<BlockHash> {
    let mut hashes = Vec::with_capacity(amount);
    let mut i = 0;
    while i < amount {
        hashes.push(generate_random_hash64(rng));
        i += 1;
    }
    hashes
}

///Note: generate_random_block is filled with random data, it does not represent a consensus-valid block!
pub fn generate_random_block(
    rng: &mut SmallRng,
    parent_amount: usize,
    number_of_transactions: usize,
    input_amount: usize,
    output_amount: usize,
) -> Block {
    Block::new(
        generate_random_header(rng, parent_amount),
        generate_random_transactions(rng, number_of_transactions, input_amount, output_amount),
    )
}

///Note: generate_random_header is filled with random data, it does not represent a consensus-valid header!
pub fn generate_random_header(rng: &mut SmallRng, parent_amount: usize) -> Header {
    // PR-9.5c/d: positions 3 and 4 (`hash_merkle_root`,
    // `accepted_id_merkle_root`) and position 5 (`utxo_commitment`) are
    // all now 64-byte `Hash64`.
    Header::new_finalized(
        rng.r#gen(),
        vec![generate_random_hashes(rng, parent_amount)].try_into().unwrap(),
        generate_random_hash64(rng),
        generate_random_hash64(rng),
        // kaspa-pq (ADR-0004 / design §12): utxo_commitment is Hash64.
        generate_random_hash64(rng),
        rng.r#gen(), // timestamp
        rng.r#gen(), // bits
        rng.r#gen(), // nonce
        // PR-9.5d / audit L-02: Phase-1 consensus admits only kHeavyHash, so use the
        // canonical algo id (a random byte would self-disqualify under check_algo_id_phase1).
        kaspa_consensus_core::pow_layer0::POW_ALGO_ID_KHEAVYHASH, // pow_algo_id
        rng.r#gen(),                 // daa_score
        rng.r#gen::<u64>().into(),   // blue_work
        rng.r#gen(),                 // blue_score
        generate_random_hash64(rng), // PR-9.5e: pruning_point is a BlockHash (Hash64)
    )
}

///Note: generate_random_transaction is filled with random data, it does not represent a consensus-valid transaction!
pub fn generate_random_transaction(rng: &mut SmallRng, input_amount: usize, output_amount: usize) -> Transaction {
    Transaction::new(
        rng.r#gen(),
        generate_random_transaction_inputs(rng, input_amount),
        generate_random_transaction_outputs(rng, output_amount),
        rng.r#gen(),
        SubnetworkId::from_byte(rng.r#gen()),
        rng.r#gen(),
        (0..20).map(|_| rng.r#gen::<u8>()).collect(),
    )
}

///Note: generate_random_transactions is filled with random data, it does not represent consensus-valid  transactions!
pub fn generate_random_transactions(rng: &mut SmallRng, amount: usize, input_amount: usize, output_amount: usize) -> Vec<Transaction> {
    Vec::from_iter((0..amount).map(move |_| generate_random_transaction(rng, input_amount, output_amount)))
}

///Note: generate_random_transactions is filled with random data, it does not represent consensus-valid  transaction input!
pub fn generate_random_transaction_input(rng: &mut SmallRng) -> TransactionInput {
    TransactionInput::new(
        generate_random_transaction_outpoint(rng),
        (0..32).map(|_| rng.r#gen::<u8>()).collect(),
        rng.r#gen(),
        rng.r#gen(),
    )
}

///Note: generate_random_transactions is filled with random data, it does not represent consensus-valid  transaction output!
pub fn generate_random_transaction_inputs(rng: &mut SmallRng, amount: usize) -> Vec<TransactionInput> {
    Vec::from_iter((0..amount).map(|_| generate_random_transaction_input(rng)))
}

///Note: generate_random_transactions is filled with random data, it does not represent consensus-valid  transaction output!
pub fn generate_random_transaction_output(rng: &mut SmallRng) -> TransactionOutput {
    TransactionOutput::new(
        rng.gen_range(1..100_000), //we choose small amounts as to not overflow with large utxosets.
        generate_random_p2pk_script_public_key(rng),
    )
}

///Note: generate_random_transactions is filled with random data, it does not represent consensus-valid  transaction output!
pub fn generate_random_transaction_outputs(rng: &mut SmallRng, amount: usize) -> Vec<TransactionOutput> {
    Vec::from_iter((0..amount).map(|_| generate_random_transaction_output(rng)))
}

///Note: generate_random_transactions is filled with random data, it does not represent consensus-valid  transaction output!
///
/// PR-9.5c: `TransactionId` widened to `Hash64`.
pub fn generate_random_transaction_outpoint(rng: &mut SmallRng) -> TransactionOutpoint {
    TransactionOutpoint::new(generate_random_hash64(rng), rng.r#gen())
}

//TODO: create `assert_eq_<kaspa-sturct>!()` helper macros in `consensus::test_helpers`
