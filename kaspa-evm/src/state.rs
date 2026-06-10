//! Ethereum keccak256 Merkle-Patricia state/storage roots over the executor's
//! in-memory state (design §3.2 `state_root`). P2 computes the root over the
//! full in-memory account set held in the revm `CacheDB`; the persistent,
//! incremental state-trie backend is P3. The empty state yields the canonical
//! empty-trie root (= `EVM_GENESIS_STATE_ROOT`), so a no-op block reproduces
//! genesis.

use alloy_rlp::Encodable;
use alloy_trie::{HashBuilder, Nibbles, EMPTY_ROOT_HASH};
use revm::db::{CacheDB, EmptyDB};
use revm::primitives::{keccak256, HashMap, B256, U256};

/// Ethereum account leaf RLP = `rlp([nonce, balance, storage_root, code_hash])`.
#[derive(alloy_rlp::RlpEncodable)]
struct TrieAccount {
    nonce: u64,
    balance: U256,
    storage_root: B256,
    code_hash: B256,
}

/// Build a secure-trie root from `(unhashed_key, value_rlp)` entries: keys are
/// keccak256-hashed then unpacked to nibbles, entries sorted by hashed key, and
/// fed to the `HashBuilder`. Empty ⇒ the canonical empty-trie root.
fn secure_root(entries: impl Iterator<Item = (B256, Vec<u8>)>) -> B256 {
    let mut leaves: Vec<(B256, Vec<u8>)> = entries.collect();
    if leaves.is_empty() {
        return EMPTY_ROOT_HASH;
    }
    leaves.sort_unstable_by_key(|(k, _)| *k);
    let mut hb = HashBuilder::default();
    for (k, v) in &leaves {
        hb.add_leaf(Nibbles::unpack(k), v);
    }
    hb.root()
}

/// keccak256 MPT storage root for one account: `keccak256(slot) -> rlp(value)`
/// over the non-zero slots (a zero slot is absent from the trie).
fn storage_root(storage: &HashMap<U256, U256>) -> B256 {
    secure_root(storage.iter().filter(|(_, v)| !v.is_zero()).map(|(slot, val)| {
        let key = keccak256(slot.to_be_bytes::<32>());
        let mut value_rlp = Vec::new();
        val.encode(&mut value_rlp);
        (key, value_rlp)
    }))
}

/// keccak256 MPT state root over every non-empty account in the executor's
/// `CacheDB` (EIP-161 empty accounts are excluded from the trie).
pub fn state_root(db: &CacheDB<EmptyDB>) -> B256 {
    secure_root(db.accounts.iter().filter(|(_, acc)| !acc.info.is_empty()).map(|(addr, acc)| {
        let ta = TrieAccount {
            nonce: acc.info.nonce,
            balance: acc.info.balance,
            storage_root: storage_root(&acc.storage),
            code_hash: acc.info.code_hash,
        };
        let mut account_rlp = Vec::new();
        ta.encode(&mut account_rlp);
        (keccak256(addr), account_rlp)
    }))
}
