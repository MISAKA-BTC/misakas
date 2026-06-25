//! C-01 R4 measurement (Stage-2 gate). Benchmarks the three per-block O(state) passes the
//! flat-state backend performs on a NON-EMPTY block, as a function of the canonical account
//! count N:
//!
//!   * `state_root`           — the committed keccak-MPT root recompute (kaspa-evm/src/state.rs).
//!                              This is the R4 cost: a full rebuild over ALL accounts every block.
//!                              An incremental persistent MPT (Stage 2) would make it O(changed).
//!   * `seed_cachedb`         — building the executor's parent seed CacheDB from the snapshot (the
//!                              CPU half of the EAGER seed; the disk read of the 234 rows is
//!                              consensus-side and not measured here). The lazy S9c seam would make
//!                              this O(touched).
//!   * `snapshot_from_cachedb`— extracting the post-state (the 206 / flat write input), the third
//!                              O(state) pass.
//!
//! Decision use: compare `state_root` at realistic N against the ~100 ms per-slot budget (10 BPS).
//! If it is a binding fraction only at N far above the live state, R4 is not yet triggered and the
//! incremental MPT stays deferred. NOTE: empty-mergeset blocks (the common case on a quiet chain)
//! skip ALL three passes via the executor's O(1) fast path, so this measures the per-NON-EMPTY-block
//! cost only. Memory (eager seed peak = O(N), lazy = O(touched)) is structural and reported in the
//! runbook/analysis, not here (a precise RSS harness is best run on the Linux build host).
//!
//! Run: `cargo bench -p kaspa-evm --bench state_cost`

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use kaspa_consensus_core::evm::{EvmAccountSnapshot, EvmAddress, EvmStateSnapshot, EvmU256, EVM_EMPTY_CODE_HASH};
use kaspa_evm::snapshot::{seed_cachedb, snapshot_from_cachedb};
use kaspa_evm::state::state_root;
use std::time::Duration;

/// Synthetic canonical state: `n` accounts, addresses sequential in the low 8 bytes (already sorted;
/// `secure_root` keccak-hashes the address into the trie key, so the trie is well-distributed
/// regardless). Every `contract_every`-th account carries `slots` non-zero storage entries (no code —
/// only the storage sub-trie cost is exercised), so the bench covers the account trie (N leaves) plus
/// a realistic fraction of storage tries. All accounts are non-empty (nonce=1, balance>0) so none is
/// EIP-161-excluded from the root.
fn make_snapshot(n: usize, contract_every: usize, slots: usize) -> EvmStateSnapshot {
    let mut accounts = Vec::with_capacity(n);
    for i in 0..n {
        let mut addr = [0u8; 20];
        addr[12..].copy_from_slice(&(i as u64).to_be_bytes());
        let storage = if contract_every != 0 && i % contract_every == 0 {
            (0..slots as u128).map(|j| (EvmU256::from_u128(j + 1), EvmU256::from_u128(i as u128 + j + 1))).collect()
        } else {
            Vec::new()
        };
        accounts.push(EvmAccountSnapshot {
            address: EvmAddress::from_bytes(addr),
            nonce: 1,
            balance: EvmU256::from_u128(i as u128 + 1),
            code_hash: EVM_EMPTY_CODE_HASH,
            code: Vec::new(),
            storage,
        });
    }
    EvmStateSnapshot { accounts }
}

fn sample_size_for(n: usize) -> usize {
    match n {
        x if x >= 1_000_000 => 10,
        x if x >= 100_000 => 15,
        x if x >= 10_000 => 30,
        _ => 60,
    }
}

fn bench_state_cost(c: &mut Criterion) {
    // ~5% of accounts are contracts carrying 16 non-zero storage slots each (a modest load).
    const CONTRACT_EVERY: usize = 20;
    const SLOTS: usize = 16;
    let sizes = [1_000usize, 10_000, 100_000, 1_000_000];

    let mut group = c.benchmark_group("evm_state_cost");
    // Bound the wall-clock for the large-N samples; criterion still honors the sample-size floor.
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(6));

    for &n in &sizes {
        let snap = make_snapshot(n, CONTRACT_EVERY, SLOTS);
        let db = seed_cachedb(&snap).expect("synthetic snapshot seeds");
        group.throughput(Throughput::Elements(n as u64));
        group.sample_size(sample_size_for(n));

        // (R4) committed-root recompute: full keccak-MPT over ALL N accounts every non-empty block.
        group.bench_with_input(BenchmarkId::new("state_root", n), &db, |b, db| {
            b.iter(|| black_box(state_root(black_box(db))));
        });
        // Eager parent-seed CacheDB build (CPU half of the eager seed).
        group.bench_with_input(BenchmarkId::new("seed_cachedb", n), &snap, |b, snap| {
            b.iter(|| black_box(seed_cachedb(black_box(snap)).unwrap()));
        });
        // Post-state extraction (the third O(state) pass).
        group.bench_with_input(BenchmarkId::new("snapshot_from_cachedb", n), &db, |b, db| {
            b.iter(|| black_box(snapshot_from_cachedb(black_box(db))));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_state_cost);
criterion_main!(benches);
