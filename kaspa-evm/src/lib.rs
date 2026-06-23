//! kaspa-pq Selected-Parent EVM execution lane (ADR-0020) — revm-backed executor.
//!
//! v0.4 mergeset delayed acceptance (design §3): the EVM parent of a DAG block
//! `B` is its GHOSTDAG `selected_parent(B)`, and `EvmResult(B)` executes the
//! MERGESET's payload txs (`AcceptedEvmTxs(B)`) — never B's own payload, which
//! is data accepted by B's selected child. The result is a pure function of
//! B's parents + B's system ops: computed once when the block becomes a
//! selected-chain candidate, stored by block hash, and never re-executed on a
//! virtual reorg (design §2.2/§10).
//!
//! This crate is the only place revm (and an EVM secp256k1/k256 ecrecover stack)
//! enters the tree. It is an **optional** dependency of `kaspa-consensus`, gated
//! behind the non-default `evm` cargo feature, so the default node stays
//! secp-free (scripts/pq-ci-guard.sh). The consensus types it operates on
//! ([`kaspa_consensus_core::evm`]) are always compiled and secp-free.

pub mod env;
pub mod executor;
pub mod mldsa_verify;
pub mod precompiles;
pub mod roots;
pub mod sim;
pub mod snapshot;
pub mod state;
pub mod tx;
pub mod withdraw;

pub use executor::{execute_block_evm, AcceptedTxCandidate, EvmBlockInput};

use revm::primitives::{AccountInfo, Address, SpecId, TxKind, U256, KECCAK_EMPTY};
use revm::{
    db::{CacheDB, EmptyDB},
    Database, Evm,
};

/// The pinned initial MISAKA EVM fork (design §19.2: London+ baseline that runs
/// Uniswap v2/v3 and current-solc contracts; Cancun/EIP-1153 for v4 is a later
/// fork). Frozen at activation — a bump is a hard fork.
pub const EVM_SPEC_ID: SpecId = SpecId::SHANGHAI;

// Audit C1 — spec-bump guard. EVM_SPEC_ID is load-bearing BEYOND opcode gating:
// the F002 SELFDESTRUCT force-send analysis (pre-EIP-6780 — see executor.rs
// module docs + `selfdestruct_to_f002_strands_value_supply_neutrally`) and the
// class-4 revert/class-2 skip boundary were audited AT SHANGHAI. Bumping the
// spec is a hard fork AND requires re-running the supply-conservation and
// skip-class suites (kaspa-evm executor tests + consensus `--features evm`
// integration tests) and re-deciding the F002 residual policy before the new
// id is frozen. This assert (and pq-ci-guard) makes a silent bump impossible.
const _: () = assert!(matches!(EVM_SPEC_ID, SpecId::SHANGHAI), "EVM spec bump: re-run supply/skip-class suites and re-decide the F002 residual policy (see comment)");

/// The Ethereum empty-trie root `keccak256(rlp(()))` — the EVM genesis state root
/// (no predeploys). Must equal `kaspa_consensus_core::evm::EVM_GENESIS_STATE_ROOT`.
pub fn empty_state_root() -> [u8; 32] {
    alloy_trie::EMPTY_ROOT_HASH.0
}

/// Increment-1 smoke (replaced by the block executor as P2 fills in): fund a
/// sender, run a single value transfer through revm at the pinned spec, and
/// return the recipient's post-execution wei balance. Proves the revm execution
/// path links and runs under this crate's secp-isolated feature set.
pub fn smoke_transfer(value_wei: u128) -> u128 {
    let from = Address::with_last_byte(0x11);
    let to = Address::with_last_byte(0x22);

    let mut db = CacheDB::new(EmptyDB::default());
    db.insert_account_info(
        from,
        AccountInfo { balance: U256::from(value_wei) + U256::from(1_000_000_000u64), nonce: 0, code_hash: KECCAK_EMPTY, code: None },
    );

    let mut evm = Evm::builder()
        .with_db(&mut db)
        .with_spec_id(EVM_SPEC_ID)
        .modify_cfg_env(|c| c.chain_id = kaspa_consensus_core::evm::EVM_CHAIN_ID)
        .modify_block_env(|b| {
            b.gas_limit = U256::from(30_000_000u64);
            b.basefee = U256::ZERO;
        })
        .modify_tx_env(|t| {
            t.caller = from;
            t.transact_to = TxKind::Call(to);
            t.value = U256::from(value_wei);
            t.gas_limit = 21_000;
            t.gas_price = U256::ZERO;
        })
        .build();
    evm.transact_commit().expect("transfer executes");
    drop(evm);

    u128::try_from(db.basic(to).unwrap().map(|a| a.balance).unwrap_or_default()).unwrap_or(0)
}

/// Errors from running a block's EVM lane.
#[derive(Debug, derive_more::Display)]
pub enum EvmExecError {
    /// A payload tx could not be decoded / its signer recovered.
    #[display("evm payload tx: {_0}")]
    TxDecode(tx::TxDecodeError),
    /// revm reported a transaction invalid for inclusion (nonce / funds / basefee).
    /// The full executor maps this to a status-0 receipt (design §6.3); this P2
    /// helper surfaces it directly.
    #[display("evm tx invalid for inclusion: {_0}")]
    InvalidTx(String),
    /// A consensus arithmetic invariant was violated (balance/supply over- or
    /// underflow). Spec-impossible on a correct chain, so it signals store
    /// corruption or a bug — fail closed (deterministic error) rather than
    /// silently saturate and hide the broken invariant (audit #5).
    #[display("evm consensus invariant violated: {_0}")]
    InvariantViolation(String),
}

/// P2 block-execution helper: seed a fresh in-memory state, run the raw EIP-2718
/// txs in order through revm at the pinned spec, and return the post-state
/// keccak MPT root, total gas used, and the resulting state. The full
/// `execute_block_evm` (env derivation, deposit credit, F002 withdraw, MISAKA
/// roots, commitment) builds on this.
pub fn execute_block_simple(
    initial: &[(Address, AccountInfo)],
    raw_txs: &[Vec<u8>],
    chain_id: u64,
    gas_limit: u64,
    basefee: u128,
) -> Result<(revm::primitives::B256, u64, CacheDB<EmptyDB>), EvmExecError> {
    let mut db = CacheDB::new(EmptyDB::default());
    for (addr, info) in initial {
        db.insert_account_info(*addr, info.clone());
    }
    let mut total_gas = 0u64;
    for raw in raw_txs {
        let txenv = tx::decode_tx_to_env(raw).map_err(EvmExecError::TxDecode)?;
        let mut evm = Evm::builder()
            .with_db(&mut db)
            .with_spec_id(EVM_SPEC_ID)
            .modify_cfg_env(|c| c.chain_id = chain_id)
            .modify_block_env(|b| {
                b.number = U256::from(1u64);
                b.gas_limit = U256::from(gas_limit);
                b.basefee = U256::from(basefee);
            })
            .modify_tx_env(move |t| *t = txenv)
            .build();
        let result = evm.transact_commit().map_err(|e| EvmExecError::InvalidTx(e.to_string()))?;
        total_gas += result.gas_used();
        drop(evm);
    }
    let root = state::state_root(&db);
    Ok((root, total_gas, db))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_is_shanghai() {
        assert_eq!(EVM_SPEC_ID, SpecId::SHANGHAI);
    }

    #[test]
    fn empty_state_root_matches_genesis() {
        // The pinned EVM genesis state root is the canonical empty-trie root.
        assert_eq!(empty_state_root(), kaspa_consensus_core::evm::EVM_GENESIS_STATE_ROOT.as_bytes());
    }

    #[test]
    fn smoke_transfer_credits_recipient() {
        assert_eq!(smoke_transfer(1_000), 1_000);
    }

    #[test]
    fn empty_cachedb_state_root_is_genesis() {
        let db = CacheDB::new(EmptyDB::default());
        assert_eq!(state::state_root(&db).0, kaspa_consensus_core::evm::EVM_GENESIS_STATE_ROOT.as_bytes());
    }

    #[test]
    fn funded_account_state_root_is_stable_and_nonempty() {
        let mut db = CacheDB::new(EmptyDB::default());
        db.insert_account_info(
            Address::with_last_byte(0xAB),
            AccountInfo { balance: U256::from(123u64), nonce: 1, code_hash: KECCAK_EMPTY, code: None },
        );
        let r1 = state::state_root(&db);
        assert_ne!(r1, alloy_trie::EMPTY_ROOT_HASH);
        assert_eq!(r1, state::state_root(&db), "state root is deterministic");
    }

    #[test]
    fn execute_signed_1559_transfer() {
        use alloy_consensus::{SignableTransaction, TxEip1559, TxEnvelope};
        use alloy_eips::eip2718::Encodable2718;
        use alloy_signer::SignerSync;
        use alloy_signer_local::PrivateKeySigner;
        use revm::primitives::{Bytes, B256};

        let chain_id = kaspa_consensus_core::evm::EVM_CHAIN_ID;
        let signer = PrivateKeySigner::from_bytes(&B256::from([0x11u8; 32])).unwrap();
        let from = signer.address();
        let to = Address::with_last_byte(0x22);

        let tx = TxEip1559 {
            chain_id,
            nonce: 0,
            gas_limit: 21_000,
            max_fee_per_gas: 0,
            max_priority_fee_per_gas: 0,
            to: TxKind::Call(to),
            value: U256::from(500u64),
            access_list: Default::default(),
            input: Bytes::new(),
        };
        let sig = signer.sign_hash_sync(&tx.signature_hash()).unwrap();
        let raw = TxEnvelope::from(tx.into_signed(sig)).encoded_2718();

        let initial = [(from, AccountInfo { balance: U256::from(1_000_000u64), nonce: 0, code_hash: KECCAK_EMPTY, code: None })];
        let (root, gas, mut db) = execute_block_simple(&initial, &[raw], chain_id, 30_000_000, 0).unwrap();

        assert_eq!(gas, 21_000, "a plain transfer costs the intrinsic 21k gas");
        assert_eq!(db.basic(to).unwrap().unwrap().balance, U256::from(500u64), "recipient credited");
        assert_ne!(root, alloy_trie::EMPTY_ROOT_HASH, "post-state root is non-empty");
    }
}
