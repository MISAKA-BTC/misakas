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
pub mod flat_backend;
pub mod mldsa_verify;
pub mod model_market;
pub mod precompiles;
pub mod reconstruct;
pub mod roots;
pub mod sim;
pub mod snapshot;
pub mod state;
pub mod trace;
pub mod tx;
pub mod withdraw;

pub use executor::{AcceptedTxCandidate, EvmBlockInput, EvmMarketInput, execute_block_evm};

use revm::primitives::{AccountInfo, Address, KECCAK_EMPTY, SpecId, TxKind, U256};
use revm::{
    Database, Evm,
    db::{CacheDB, EmptyDB},
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
const _: () = assert!(
    matches!(EVM_SPEC_ID, SpecId::SHANGHAI),
    "EVM spec bump: re-run supply/skip-class suites and re-decide the F002 residual policy (see comment)"
);

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
    /// ADR-0089 Decision 6: the block's `MarketSettle` system ops are not EXACTLY the
    /// settlement list its selected parent's fold decided (a missing, extra, reordered or
    /// altered op). A producer fault: the block is disqualified, as a bad deposit claim
    /// disqualifies it.
    #[display("evm market settlement mismatch: {_0}")]
    MarketSettlementMismatch(String),
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
    fn evm_empty_code_hash_matches_revm() {
        // §12: the secp-free EVM_EMPTY_CODE_HASH the archive diff engine uses to
        // recognize code-less accounts must equal revm's KECCAK_EMPTY, or the code
        // store / reconstruction would mis-classify EOAs vs contracts.
        assert_eq!(kaspa_consensus_core::evm::EVM_EMPTY_CODE_HASH.as_bytes(), KECCAK_EMPTY.0);
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

    /// **O13 benchmark (2026-09-18): what one second of execution buys on this host.** Run with
    /// `cargo test -p kaspa-evm --release -- --ignored --nocapture o13_bench`. Two workloads: plain
    /// transfers (the cheapest gas — the most transactions a second can carry) and a storage-writing
    /// contract (the dearest gas — every SSTORE grows the state). The per-round budget is set from
    /// the SLOWER gas/s with a safety factor, and the bytes/gas of each tells the propagation cost.
    #[test]
    #[ignore]
    fn o13_bench_execution_gas_per_second() {
        use alloy_consensus::{SignableTransaction, TxEip1559, TxEnvelope};
        use alloy_eips::eip2718::Encodable2718;
        use alloy_signer::SignerSync;
        use alloy_signer_local::PrivateKeySigner;
        use revm::primitives::{B256, Bytes};
        let chain_id = kaspa_consensus_core::evm::EVM_CHAIN_ID;
        let signer = PrivateKeySigner::from_bytes(&B256::from([0x11u8; 32])).unwrap();
        let from = signer.address();
        let funded = [(from, AccountInfo { balance: U256::from(u64::MAX), nonce: 0, code_hash: KECCAK_EMPTY, code: None })];
        let sign = |tx: TxEip1559| -> Vec<u8> {
            let sig = signer.sign_hash_sync(&tx.signature_hash()).unwrap();
            TxEnvelope::from(tx.into_signed(sig)).encoded_2718()
        };
        // Workload A: 1,000 transfers.
        let transfers: Vec<Vec<u8>> = (0..1_000u64)
            .map(|n| {
                sign(TxEip1559 {
                    chain_id,
                    nonce: n,
                    gas_limit: 21_000,
                    max_fee_per_gas: 0,
                    max_priority_fee_per_gas: 0,
                    to: TxKind::Call(Address::with_last_byte((n % 200) as u8 + 1)),
                    value: U256::from(1u64),
                    access_list: Default::default(),
                    input: Bytes::new(),
                })
            })
            .collect();
        let bytes_a: usize = transfers.iter().map(|t| t.len()).sum();
        let t0 = std::time::Instant::now();
        let (_, gas_a, _) = execute_block_simple(&funded, &transfers, chain_id, 30_000_000, 0).unwrap();
        let dt_a = t0.elapsed().as_secs_f64();
        // Workload B: a contract whose code loops SSTORE over fresh slots — init code that, on every
        // call, writes `n` slots: PUSH loop. Deploy once, then 200 calls of ~40 SSTOREs each.
        // Runtime: for i in 0..40 { sstore(i + calldata_word, 1) }  — assembled by hand below.
        let runtime: Vec<u8> = {
            let mut c = Vec::new();
            // counter := 0 (kept on stack)
            c.extend([0x60, 0x00]); // PUSH1 0
            // loop: JUMPDEST
            let loop_start = c.len() as u8;
            c.push(0x5b);
            // dup counter; calldataload(0) add -> slot ; push 1 ; swap ; sstore
            c.extend([0x80, 0x60, 0x00, 0x35, 0x01, 0x60, 0x01, 0x90, 0x55]);
            // counter += 1 ; dup ; push 40 ; gt (40 > counter) ; push loop ; jumpi
            c.extend([0x60, 0x01, 0x01, 0x80, 0x60, 0x28, 0x11, 0x60, loop_start, 0x57]);
            c.push(0x00); // STOP
            c
        };
        let init: Vec<u8> = {
            // CODECOPY the runtime to memory and RETURN it.
            let len = runtime.len() as u8;
            let mut i = vec![0x60, len, 0x60, 0x0c, 0x60, 0x00, 0x39, 0x60, len, 0x60, 0x00, 0xf3];
            i.extend(&runtime);
            i
        };
        let deploy = sign(TxEip1559 {
            chain_id,
            nonce: 1_000,
            gas_limit: 300_000,
            max_fee_per_gas: 0,
            max_priority_fee_per_gas: 0,
            to: TxKind::Create,
            value: U256::ZERO,
            access_list: Default::default(),
            input: Bytes::from(init),
        });
        let contract = from.create(1_000);
        let calls: Vec<Vec<u8>> = (0..200u64)
            .map(|n| {
                let mut data = [0u8; 32];
                data[24..].copy_from_slice(&(n * 64).to_be_bytes());
                sign(TxEip1559 {
                    chain_id,
                    nonce: 1_001 + n,
                    gas_limit: 1_500_000,
                    max_fee_per_gas: 0,
                    max_priority_fee_per_gas: 0,
                    to: TxKind::Call(contract),
                    value: U256::ZERO,
                    access_list: Default::default(),
                    input: Bytes::from(data.to_vec()),
                })
            })
            .collect();
        let mut txs_b = vec![deploy];
        txs_b.extend(calls);
        let bytes_b: usize = txs_b.iter().map(|t| t.len()).sum();
        let funded_b = [(from, AccountInfo { balance: U256::from(u64::MAX), nonce: 1_000, code_hash: KECCAK_EMPTY, code: None })];
        let t1 = std::time::Instant::now();
        let (_, gas_b, _) = execute_block_simple(&funded_b, &txs_b, chain_id, 300_000_000, 0).unwrap();
        let dt_b = t1.elapsed().as_secs_f64();
        println!(
            "O13 BENCH transfers: {} txs, {} gas, {:.3} s -> {:.0} gas/s, {} bytes -> {:.4} bytes/gas",
            transfers.len(),
            gas_a,
            dt_a,
            gas_a as f64 / dt_a,
            bytes_a,
            bytes_a as f64 / gas_a as f64
        );
        println!(
            "O13 BENCH sstore-heavy: {} txs, {} gas, {:.3} s -> {:.0} gas/s, {} bytes -> {:.4} bytes/gas",
            txs_b.len(),
            gas_b,
            dt_b,
            gas_b as f64 / dt_b,
            bytes_b,
            bytes_b as f64 / gas_b as f64
        );
        assert!(gas_b > 1_000_000, "the storage workload executed ({gas_b} gas): the hand-assembled loop ran");
    }

    /// **O13 burst gate (2026-09-18): one chain block at the 390 M ceiling.** ADR-0139 lets a chain
    /// block that merges 120 permitted rounds accept `EVM_CHAIN_BLOCK_GAS_CEILING_V1` of user gas.
    /// The unit tests pin the RULE (one budget a distinct round, duplicates and gaps bought once,
    /// saturating at the ceiling); this measures what the rule COSTS when a block actually fills it.
    ///
    /// Three phases are timed separately because a reorg pays different ones than an apply:
    ///   * `seed`    — building the executor's parent CacheDB from the snapshot (paid by both),
    ///   * `execute` — the 390 M gas itself (paid by both; a reorg re-executes the replacement),
    ///   * `commit`  — `state_root` + `snapshot_from_cachedb`, the two O(state) passes.
    /// An apply is seed+execute+commit. A reorg is the same work again for the replacement block,
    /// plus re-seeding from the pre-block snapshot, which `seed` measures.
    ///
    /// The workload is transfers to FRESH addresses: the cheapest gas per transaction, so the most
    /// transactions a ceiling block can carry, and the worst case for state growth (a new account
    /// per transaction). Run with
    /// `cargo test -p kaspa-evm --release -- --ignored --nocapture o13_bench_ceiling_block`.
    #[test]
    #[ignore]
    fn o13_bench_ceiling_block_apply_and_reorg() {
        use alloy_consensus::{SignableTransaction, TxEip1559, TxEnvelope};
        use alloy_eips::eip2718::Encodable2718;
        use alloy_signer::SignerSync;
        use alloy_signer_local::PrivateKeySigner;
        use revm::primitives::{B256, Bytes};

        const RUNS: usize = 30;
        let ceiling = kaspa_consensus_core::evm::EVM_CHAIN_BLOCK_GAS_CEILING_V1;
        let chain_id = kaspa_consensus_core::evm::EVM_CHAIN_ID;
        let signer = PrivateKeySigner::from_bytes(&B256::from([0x11u8; 32])).unwrap();
        let from = signer.address();
        let sign = |tx: TxEip1559| -> Vec<u8> {
            let sig = signer.sign_hash_sync(&tx.signature_hash()).unwrap();
            TxEnvelope::from(tx.into_signed(sig)).encoded_2718()
        };

        // Exactly as many 21,000-gas transfers as the ceiling admits, each to an address no block
        // before it touched: the state-growth worst case ADR-0139 §3 names as the binding cost.
        let count = (ceiling / 21_000) as usize;
        let txs: Vec<Vec<u8>> = (0..count as u64)
            .map(|n| {
                // Offset clear of the zero address and the precompiles at 0x01..0x09: those are not
                // fresh accounts, and an empty one is dropped from the snapshot, which would make
                // the state-growth count report less than the block actually created.
                let mut to = [0u8; 20];
                to[12..20].copy_from_slice(&(n + 0x1_0000).to_be_bytes());
                sign(TxEip1559 {
                    chain_id,
                    nonce: n,
                    gas_limit: 21_000,
                    max_fee_per_gas: 0,
                    max_priority_fee_per_gas: 0,
                    to: TxKind::Call(Address::from(to)),
                    value: U256::from(1u64),
                    access_list: Default::default(),
                    input: Bytes::new(),
                })
            })
            .collect();
        let block_bytes: usize = txs.iter().map(|t| t.len()).sum();

        // The parent state this block is applied on top of: the funded sender, nothing else. Built
        // through the same extraction the chain uses, so the seed under test is a real snapshot.
        let parent = {
            let mut pre = CacheDB::new(EmptyDB::default());
            pre.insert_account_info(
                from,
                AccountInfo { balance: U256::from(u64::MAX), nonce: 0, code_hash: KECCAK_EMPTY, code: None },
            );
            crate::snapshot::snapshot_from_cachedb(&pre)
        };

        let mut seed_ms = Vec::with_capacity(RUNS);
        let mut exec_ms = Vec::with_capacity(RUNS);
        let mut commit_ms = Vec::with_capacity(RUNS);
        let mut apply_ms = Vec::with_capacity(RUNS);
        let mut reseed_ms = Vec::with_capacity(RUNS);
        let (mut gas_total, mut accounts_after, mut slots_after, mut snap_bytes) = (0u64, 0usize, 0usize, 0usize);

        for _ in 0..RUNS {
            let t_apply = std::time::Instant::now();

            let t0 = std::time::Instant::now();
            let mut db = crate::snapshot::seed_cachedb(&parent).expect("the parent seed is well formed");
            seed_ms.push(t0.elapsed().as_secs_f64() * 1000.0);

            let t1 = std::time::Instant::now();
            let mut gas = 0u64;
            for raw in &txs {
                let txenv = crate::tx::decode_tx_to_env(raw).expect("the bench signs its own transactions");
                let mut evm = Evm::builder()
                    .with_db(&mut db)
                    .with_spec_id(EVM_SPEC_ID)
                    .modify_cfg_env(|c| c.chain_id = chain_id)
                    .modify_block_env(|b| {
                        b.number = U256::from(1u64);
                        b.gas_limit = U256::from(ceiling);
                        b.basefee = U256::from(0u64);
                    })
                    .modify_tx_env(move |t| *t = txenv)
                    .build();
                gas += evm.transact_commit().expect("a funded transfer executes").gas_used();
            }
            exec_ms.push(t1.elapsed().as_secs_f64() * 1000.0);

            let t2 = std::time::Instant::now();
            let _root = crate::state::state_root(&db);
            let snap = crate::snapshot::snapshot_from_cachedb(&db);
            commit_ms.push(t2.elapsed().as_secs_f64() * 1000.0);

            apply_ms.push(t_apply.elapsed().as_secs_f64() * 1000.0);

            // The seed above came from a one-account parent, which measures nothing. A REORG
            // re-seeds from a snapshot of the whole state, so time that against the state this
            // block just produced — the honest lower bound for a chain that has run a while.
            let t3 = std::time::Instant::now();
            let reseeded = crate::snapshot::seed_cachedb(&snap).expect("the post-state seeds back");
            reseed_ms.push(t3.elapsed().as_secs_f64() * 1000.0);
            debug_assert_eq!(reseeded.accounts.len(), snap.accounts.len());

            gas_total = gas;
            accounts_after = snap.accounts.len();
            slots_after = snap.accounts.iter().map(|a| a.storage.len()).sum();
            snap_bytes = snap.accounts.iter().map(|a| 20 + 8 + 32 + 32 + a.code.len() + a.storage.len() * 64).sum();
        }

        // Percentiles over a DETERMINISTIC workload measure this host's noise, not tail behaviour of
        // the rule. p50 and p95 are reported because they were asked for; `max` is the number that
        // actually bounds a slot, and at n=30 it is also the only honest answer above p95.
        let pct = |v: &mut Vec<f64>, p: f64| -> f64 {
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let i = (((v.len() - 1) as f64) * p).round() as usize;
            v[i]
        };
        let line = |name: &str, v: &mut Vec<f64>| {
            let (lo, p50, p95, hi) = (pct(v, 0.0), pct(v, 0.50), pct(v, 0.95), pct(v, 1.0));
            println!("O13 CEILING {name:8} min {lo:8.1} ms · p50 {p50:8.1} ms · p95 {p95:8.1} ms · max {hi:8.1} ms  (n={RUNS})");
        };

        println!(
            "O13 CEILING block: {} transfers, {} gas of a {} ceiling, {} bytes -> {:.4} bytes/gas",
            txs.len(),
            gas_total,
            ceiling,
            block_bytes,
            block_bytes as f64 / gas_total as f64
        );
        line("seed", &mut seed_ms);
        line("execute", &mut exec_ms);
        line("commit", &mut commit_ms);
        line("APPLY", &mut apply_ms);
        line("reseed", &mut reseed_ms);
        println!(
            "O13 CEILING state after one block: {accounts_after} accounts, {slots_after} storage slots, \
             {snap_bytes} snapshot bytes -> {:.4} bytes/gas of NEW state",
            snap_bytes as f64 / gas_total.max(1) as f64
        );
        println!(
            "O13 CEILING reorg = reseed from the pre-block snapshot + the replacement block's own apply. \
             The `reseed` line times that seed against {accounts_after} accounts, so a reorg of D ceiling \
             blocks costs about D x (APPLY + reseed)."
        );

        assert!(gas_total > ceiling - 21_000, "the block filled the ceiling ({gas_total} of {ceiling})");
        assert_eq!(accounts_after, txs.len() + 1, "one fresh account a transfer, plus the sender: the state-growth worst case");
    }

    #[test]
    fn execute_signed_1559_transfer() {
        use alloy_consensus::{SignableTransaction, TxEip1559, TxEnvelope};
        use alloy_eips::eip2718::Encodable2718;
        use alloy_signer::SignerSync;
        use alloy_signer_local::PrivateKeySigner;
        use revm::primitives::{B256, Bytes};

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
