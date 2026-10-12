//! C-01 state-backend (design v0.1 `docs/misaka-evm-state-backend-design-v0.1.md`,
//! Stage 1) slice **S3** — the flat-state-backed revm adapter.
//!
//! [`FlatStateBackend`] is a revm [`DatabaseRef`] that lazily point-looks-up
//! accounts/storage from the C-01 flat latest-canonical state (consensus prefix
//! 234 [`FlatAccount`] rows + prefix 222 content-addressed code), through a
//! [`FlatStateReader`] seam. Wrapped in revm's `CacheDB`
//! ([`flat_backed_cachedb`]) it becomes the executor's fast **head** seed path:
//! instead of materializing the full parent snapshot into a `CacheDB<EmptyDB>`
//! (`snapshot::seed_cachedb`), revm pulls only the accounts/slots a block
//! actually touches. A non-head / historical parent still seeds via
//! `reconstruct_evm_state(parent)` → `seed_cachedb` (the §12 path), unchanged —
//! choosing between the two is the caller's (executor) job at the seed switch
//! (slice S6); this slice only supplies the head adapter.
//!
//! ## Consensus-neutrality (design §7, risk R1 — leaf-extraction / root drift)
//!
//! For every account and every slot this backend returns **byte-identical reads**
//! to `seed_cachedb(materialize(flat))`, so revm makes the identical sequence of
//! reads and `transact()` yields a byte-identical [`ResultAndState`]:
//!
//! * a present account loads (via `CacheDB::basic`) with `AccountState::None` and
//!   the same [`AccountInfo`] — code is attached **eagerly**, exactly as
//!   `seed_cachedb` does, so revm never needs the `code_by_hash` fallback;
//! * an absent account becomes `AccountState::NotExisting` (revm `basic_ref` →
//!   `None`), exactly as the eager seed's untouched-address path;
//! * a missing storage slot reads `0` (the flat row holds only non-zero slots,
//!   like the canonical snapshot);
//! * `block_hash` delegates to revm's own `EmptyDB` (`keccak256(number)`), so the
//!   BLOCKHASH opcode is byte-identical to today's `CacheDB<EmptyDB>` executor.
//!
//! The lazy/eager difference — untouched accounts are absent from the lazy cache
//! — affects only the post-state `state_root` / `snapshot_from_cachedb` walk,
//! which still run over a `CacheDB<EmptyDB>` exactly as today and are out of this
//! slice's scope (they move at S6). This slice changes only **how the parent seed
//! is obtained**, never any committed bytes.
//!
//! **INERT.** Nothing in `kaspa-consensus` or the executor references this module
//! yet; it is exercised offline against a synthetic [`FlatStateReader`]. Fail-
//! closed posture mirrors `seed_cachedb` (audit EVM-01): a non-empty `code_hash`
//! whose code is absent from the content-addressed store is a hard error, never a
//! silently-empty (uncallable) contract.

use kaspa_consensus_core::evm::{EvmAddress, EvmU256, FlatAccount};
use kaspa_hashes::EvmH256;
use revm::DatabaseRef;
use revm::db::{CacheDB, EmptyDB};
use revm::primitives::{AccountInfo, Address, B256, Bytecode, Bytes, KECCAK_EMPTY, U256};

#[inline]
fn to_u256(v: EvmU256) -> U256 {
    U256::from_be_bytes(v.to_be_bytes())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Error from a flat-state point lookup or a flat/code-store inconsistency.
#[derive(Debug, derive_more::Display)]
pub enum FlatBackendError {
    /// The underlying store read failed (RocksDB / borsh decode); `_0` is the source.
    #[display("flat-state store read: {_0}")]
    Store(String),
    /// `code_hash != KECCAK_EMPTY` but the content-addressed code store has no
    /// entry — the account would be an uncallable contract (store corruption).
    /// Fail closed rather than seed empty code (audit EVM-01).
    #[display("flat-state: account 0x{address} code_hash {code_hash:?} has no code in the content-addressed store")]
    MissingCode { address: String, code_hash: EvmH256 },
}

/// O(1) point-lookup seam over the C-01 flat latest-canonical state. Implemented
/// by the consensus stores (`DbEvmFlatAccountStore` + `DbEvmCodeStore`) at the
/// live seed switch (slice S6) and by an in-memory fake in tests. NEVER
/// enumerates — the full-state walk (root recompute / IBD snapshot) uses the
/// store's own `iter()` directly, not this trait.
pub trait FlatStateReader {
    /// The account at `address` in the current canonical state (`None` = absent).
    fn flat_account(&self, address: EvmAddress) -> Result<Option<FlatAccount>, FlatBackendError>;
    /// Code bytes for `code_hash` from the content-addressed code store
    /// (`None` = not present). Never queried for `KECCAK_EMPTY`.
    fn flat_code(&self, code_hash: EvmH256) -> Result<Option<Vec<u8>>, FlatBackendError>;
}

/// A revm [`DatabaseRef`] over a [`FlatStateReader`] (plus an `EmptyDB` for
/// `block_hash` parity). Wrap in `CacheDB` to obtain a `Database`
/// (`CacheDB<ExtDB: DatabaseRef>` auto-implements `Database`) — see
/// [`flat_backed_cachedb`].
#[derive(Clone)]
pub struct FlatStateBackend<R: FlatStateReader> {
    reader: R,
    empty: EmptyDB,
}

impl<R: FlatStateReader> FlatStateBackend<R> {
    pub fn new(reader: R) -> Self {
        Self { reader, empty: EmptyDB::default() }
    }
}

/// Build the executor's **head** seed: a `CacheDB` lazily backed by the flat
/// store. revm reads accounts/slots on demand and caches them; nothing is
/// enumerated up front (the O(state) materialization the snapshot path does).
pub fn flat_backed_cachedb<R: FlatStateReader>(reader: R) -> CacheDB<FlatStateBackend<R>> {
    CacheDB::new(FlatStateBackend::new(reader))
}

impl<R: FlatStateReader> DatabaseRef for FlatStateBackend<R> {
    type Error = FlatBackendError;

    fn basic_ref(&self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        let Some(flat) = self.reader.flat_account(EvmAddress::from_bytes(address.into_array()))? else {
            return Ok(None);
        };
        let code_hash = B256::from(flat.core.code_hash.as_bytes());
        // Attach code eagerly, exactly as `seed_cachedb`, so the produced
        // `AccountInfo` is byte-identical and revm never needs `code_by_hash`.
        let code = if code_hash == KECCAK_EMPTY {
            None
        } else {
            let bytes = self.reader.flat_code(flat.core.code_hash)?.ok_or_else(|| FlatBackendError::MissingCode {
                address: hex(&address.into_array()),
                code_hash: flat.core.code_hash,
            })?;
            Some(Bytecode::new_raw(Bytes::from(bytes)))
        };
        Ok(Some(AccountInfo { balance: to_u256(flat.core.balance), nonce: flat.core.nonce, code_hash, code }))
    }

    fn code_by_hash_ref(&self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        // Fallback only — `basic_ref` already attaches code. Matches `EmptyDB`
        // for the empty hash; fail-closed for a non-empty hash missing its code.
        if code_hash == KECCAK_EMPTY {
            return Ok(Bytecode::default());
        }
        let h = EvmH256::from_bytes(code_hash.0);
        let bytes =
            self.reader.flat_code(h)?.ok_or_else(|| FlatBackendError::MissingCode { address: "<by-hash>".into(), code_hash: h })?;
        Ok(Bytecode::new_raw(Bytes::from(bytes)))
    }

    fn storage_ref(&self, address: Address, index: U256) -> Result<U256, Self::Error> {
        let Some(flat) = self.reader.flat_account(EvmAddress::from_bytes(address.into_array()))? else {
            return Ok(U256::ZERO);
        };
        // The flat row holds only non-zero slots (canonical form); a slot not
        // listed reads zero — exactly the snapshot-seeded path.
        let target = EvmU256::from_be_bytes(index.to_be_bytes::<32>());
        Ok(flat.storage.iter().find(|(slot, _)| *slot == target).map(|(_, v)| to_u256(*v)).unwrap_or(U256::ZERO))
    }

    fn block_hash_ref(&self, number: u64) -> Result<B256, Self::Error> {
        // Byte-identical to the current `CacheDB<EmptyDB>` executor path.
        DatabaseRef::block_hash_ref(&self.empty, number).map_err(|never: std::convert::Infallible| match never {})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EVM_SPEC_ID;
    use crate::snapshot::seed_cachedb;
    use alloy_consensus::{SignableTransaction, TxEip1559, TxEnvelope};
    use alloy_eips::eip2718::Encodable2718;
    use alloy_signer::SignerSync;
    use alloy_signer_local::PrivateKeySigner;
    use kaspa_consensus_core::evm::{AccountCore, EVM_CHAIN_ID, EVM_INITIAL_BASE_FEE, EvmAccountSnapshot, EvmStateSnapshot};
    use revm::primitives::{ResultAndState, TxKind, keccak256};
    use revm::{Database, Evm};
    use std::collections::HashMap;

    /// In-memory [`FlatStateReader`] — the synthetic store for the offline proof.
    #[derive(Clone, Default)]
    struct FakeFlatReader {
        accounts: HashMap<[u8; 20], FlatAccount>,
        code: HashMap<[u8; 32], Vec<u8>>,
    }

    impl FlatStateReader for FakeFlatReader {
        fn flat_account(&self, address: EvmAddress) -> Result<Option<FlatAccount>, FlatBackendError> {
            Ok(self.accounts.get(&address.as_bytes()).cloned())
        }
        fn flat_code(&self, code_hash: EvmH256) -> Result<Option<Vec<u8>>, FlatBackendError> {
            Ok(self.code.get(&code_hash.as_bytes()).cloned())
        }
    }

    fn empty_hash() -> EvmH256 {
        EvmH256::from_bytes(KECCAK_EMPTY.0)
    }

    /// Build a canonical snapshot AND the equivalent flat reader from one source
    /// of accounts. The snapshot is sorted (accounts by address, slots by slot)
    /// so `seed_cachedb` accepts it; the reader is keyed by address.
    fn build(accounts: Vec<EvmAccountSnapshot>) -> (EvmStateSnapshot, FakeFlatReader) {
        let mut reader = FakeFlatReader::default();
        let mut accs = accounts;
        for a in &mut accs {
            a.storage.sort_unstable_by(|x, y| x.0.to_be_bytes().cmp(&y.0.to_be_bytes()));
            if !a.code.is_empty() {
                reader.code.insert(a.code_hash.as_bytes(), a.code.clone());
            }
            reader.accounts.insert(
                a.address.as_bytes(),
                FlatAccount {
                    core: AccountCore { nonce: a.nonce, balance: a.balance, code_hash: a.code_hash },
                    storage: a.storage.clone(),
                },
            );
        }
        accs.sort_unstable_by(|x, y| x.address.as_bytes().cmp(&y.address.as_bytes()));
        (EvmStateSnapshot { accounts: accs }, reader)
    }

    fn eoa(addr: [u8; 20], nonce: u64, balance: u128) -> EvmAccountSnapshot {
        EvmAccountSnapshot {
            address: EvmAddress::from_bytes(addr),
            nonce,
            balance: EvmU256::from(balance),
            code_hash: empty_hash(),
            code: vec![],
            storage: vec![],
        }
    }

    fn contract(addr: [u8; 20], nonce: u64, balance: u128, code: Vec<u8>, storage: Vec<(u128, u128)>) -> EvmAccountSnapshot {
        let code_hash = EvmH256::from_bytes(keccak256(&code).0);
        let storage = storage.into_iter().map(|(s, v)| (EvmU256::from(s), EvmU256::from(v))).collect();
        EvmAccountSnapshot { address: EvmAddress::from_bytes(addr), nonce, balance: EvmU256::from(balance), code_hash, code, storage }
    }

    /// The lazy flat-backed `DatabaseRef` returns byte-identical reads to the
    /// eager `seed_cachedb(snapshot)` for present accounts, absent accounts,
    /// present/absent storage slots, code, and block hashes.
    #[test]
    fn flat_backend_reads_match_seeded_cachedb() {
        // A non-trivial canonical state: two EOAs and a contract with storage.
        let contract_code = vec![0x60u8, 0x00, 0x60, 0x00, 0xf3];
        let (snapshot, reader) = build(vec![
            eoa([0xAA; 20], 5, 1_000),
            eoa([0xBB; 20], 0, 42),
            contract([0xCC; 20], 1, 7, contract_code, vec![(1, 11), (5, 55)]),
        ]);

        let eager = seed_cachedb(&snapshot).expect("canonical snapshot seeds");
        let lazy = FlatStateBackend::new(reader);

        // basic_ref: present accounts (incl. the contract's attached code) + an absent one.
        for addr in [[0xAA; 20], [0xBB; 20], [0xCC; 20], [0xDD; 20]] {
            let a = Address::from(addr);
            assert_eq!(
                lazy.basic_ref(a).unwrap(),
                DatabaseRef::basic_ref(&eager, a).unwrap(),
                "basic_ref divergence at 0x{}",
                hex(&addr)
            );
        }

        // storage_ref on the contract: present non-zero slots + an absent (zero) slot,
        // and storage on an absent account (must be zero).
        let cc = Address::from([0xCC; 20]);
        for slot in [1u64, 5, 9] {
            let idx = U256::from(slot);
            assert_eq!(
                lazy.storage_ref(cc, idx).unwrap(),
                DatabaseRef::storage_ref(&eager, cc, idx).unwrap(),
                "storage_ref divergence at slot {slot}"
            );
        }
        let dd = Address::from([0xDD; 20]);
        assert_eq!(lazy.storage_ref(dd, U256::from(1u64)).unwrap(), U256::ZERO);

        // code_by_hash_ref: empty hash → default; the contract's hash → its code.
        assert_eq!(lazy.code_by_hash_ref(KECCAK_EMPTY).unwrap(), Bytecode::default());
        let cc_hash = DatabaseRef::basic_ref(&eager, cc).unwrap().unwrap().code_hash;
        assert_eq!(lazy.code_by_hash_ref(cc_hash).unwrap(), DatabaseRef::code_by_hash_ref(&eager, cc_hash).unwrap());

        // block_hash_ref parity with EmptyDB.
        for n in [0u64, 1, 12_345] {
            assert_eq!(
                lazy.block_hash_ref(n).unwrap(),
                DatabaseRef::block_hash_ref(&eager, n).unwrap(),
                "block_hash_ref divergence at {n}"
            );
        }
    }

    fn signed_transfer(signer: &PrivateKeySigner, nonce: u64, to: Address, value: u128, max_fee: u128) -> Vec<u8> {
        let tx = TxEip1559 {
            chain_id: EVM_CHAIN_ID,
            nonce,
            gas_limit: 21_000,
            max_fee_per_gas: max_fee,
            max_priority_fee_per_gas: 0,
            to: TxKind::Call(to),
            value: U256::from(value),
            access_list: Default::default(),
            input: Default::default(),
        };
        let sig = signer.sign_hash_sync(&tx.signature_hash()).unwrap();
        TxEnvelope::from(tx.into_signed(sig)).encoded_2718()
    }

    fn run_one<DB: Database>(db: DB, raw: &[u8]) -> ResultAndState
    where
        DB::Error: std::fmt::Debug,
    {
        let txenv = crate::tx::decode_tx_to_env(raw).expect("decode tx");
        let mut evm = Evm::builder()
            .with_db(db)
            .with_spec_id(EVM_SPEC_ID)
            .modify_cfg_env(|c| c.chain_id = EVM_CHAIN_ID)
            .modify_block_env(|b| {
                b.number = U256::from(1u64);
                b.gas_limit = U256::from(30_000_000u64);
                b.basefee = U256::from(EVM_INITIAL_BASE_FEE as u128);
            })
            .modify_tx_env(move |t| *t = txenv)
            .build();
        evm.transact().expect("transact")
    }

    /// The strong consensus-neutrality proof: running the identical signed tx
    /// against the LAZY flat-backed seed and the EAGER `seed_cachedb` seed yields
    /// a byte-identical `ResultAndState` (same execution result + same state diff).
    /// Untouched accounts (the contract here) never enter the result, so the
    /// lazy/eager cache difference is provably invisible to execution.
    #[test]
    fn flat_backed_execution_matches_seeded() {
        let signer = PrivateKeySigner::from_bytes(&B256::from([0x11u8; 32])).unwrap();
        let from = signer.address();
        let to = Address::with_last_byte(0x55);

        // Fund the signer; include an untouched contract so the state is non-trivial.
        let contract_code = vec![0x60u8, 0x00, 0x60, 0x00, 0xf3];
        let (snapshot, reader) = build(vec![
            EvmAccountSnapshot {
                address: EvmAddress::from_bytes(from.into_array()),
                nonce: 0,
                balance: EvmU256::from(1_000_000_000_000_000_000u128),
                code_hash: empty_hash(),
                code: vec![],
                storage: vec![],
            },
            contract([0xCC; 20], 1, 7, contract_code, vec![(1, 11)]),
        ]);

        let raw = signed_transfer(&signer, 0, to, 500, EVM_INITIAL_BASE_FEE as u128);

        let res_lazy = run_one(flat_backed_cachedb(reader.clone()), &raw);
        let res_eager = run_one(seed_cachedb(&snapshot).unwrap(), &raw);

        assert_eq!(res_lazy, res_eager, "lazy flat-backed seed must produce a byte-identical ResultAndState");
        assert!(res_lazy.result.is_success(), "the funded transfer succeeds");
        // The untouched contract is absent from the (touched-only) result state.
        assert!(!res_lazy.state.contains_key(&Address::from([0xCC; 20])), "untouched account stays out of the result");
    }

    /// Fail-closed: a contract whose `code_hash` has no code in the store is a
    /// hard error (audit EVM-01), never a silently-empty contract.
    #[test]
    fn missing_code_fails_closed() {
        let code = vec![0x60u8, 0x00];
        let code_hash = EvmH256::from_bytes(keccak256(&code).0);
        let mut reader = FakeFlatReader::default();
        // Account references a code_hash, but the code store is empty.
        reader.accounts.insert(
            [0xCC; 20],
            FlatAccount { core: AccountCore { nonce: 1, balance: EvmU256::from(1u128), code_hash }, storage: vec![] },
        );
        let lazy = FlatStateBackend::new(reader);
        assert!(matches!(lazy.basic_ref(Address::from([0xCC; 20])), Err(FlatBackendError::MissingCode { .. })));
    }
}
