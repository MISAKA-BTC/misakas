//! §12 historical state reconstruction (design §12.4) — the verify core.
//!
//! Given a SEED full snapshot (the nearest checkpoint at-or-before the target, or
//! the empty genesis when the target precedes the first checkpoint), the ordered
//! forward diffs from the seed's block to the target, and a resolver for the
//! content-addressed code store, this replays the diffs through the pure
//! consensus-core engine, rebuilds the full snapshot, and VERIFIES it reproduces
//! the target block's committed EVM state root via the real keccak-MPT
//! ([`crate::state::state_root`]).
//!
//! A root mismatch — or any diff-chain inconsistency / missing code surfaced by
//! the engine — fails the reconstruction. There is no empty / partial fallback
//! (design §12.4: a corrupt history must fail the query, not silently answer with
//! genesis state). The consensus driver gathers the seed + diffs by walking the
//! canonical number map and calls this; everything here is offline-testable.

use kaspa_consensus_core::evm::{apply_state_diff, recon_from_snapshot, recon_to_snapshot, EvmStateDiffV2, EvmStateSnapshot, StateDiffError};
use kaspa_hashes::EvmH256;

/// A historical-reconstruction failure (design §12.4) — fail closed.
#[derive(Debug)]
pub enum ReconstructError {
    /// A diff-chain inconsistency or missing bytecode, from the pure engine
    /// ([`StateDiffError`]): a forward diff's `before` view disagreed with the
    /// accumulated state, a checkpoint checksum was bad, or a code hash the
    /// reconstructed state references is absent from the code store.
    Engine(StateDiffError),
    /// The reconstructed snapshot failed revm's canonical-form / code-consistency
    /// check while seeding ([`crate::snapshot::seed_cachedb`]).
    Seed(crate::EvmExecError),
    /// The reconstructed state's keccak-MPT root does not equal the target block's
    /// committed EVM state root: the diff/checkpoint chain is corrupt. The query
    /// fails (design §12.4) — never answered from a partial state.
    RootMismatch { expected: EvmH256, computed: EvmH256 },
}

impl std::fmt::Display for ReconstructError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReconstructError::Engine(e) => write!(f, "EVM reconstruction engine: {e}"),
            ReconstructError::Seed(e) => write!(f, "EVM reconstruction seed: {e}"),
            ReconstructError::RootMismatch { expected, computed } => {
                write!(f, "EVM reconstruction state-root mismatch: committed {expected}, reconstructed {computed}")
            }
        }
    }
}

impl std::error::Error for ReconstructError {}

/// Reconstruct + verify the EVM state at a target block (design §12.4).
///
/// `seed` is the full state at some canonical ancestor (a checkpoint snapshot, or
/// the empty default for a target before the first checkpoint); `forward_diffs`
/// are that ancestor's children up to and including the target, in canonical
/// order; `code_resolver` resolves a `code_hash` to its bytes (prefix 222);
/// `expected_root` is the target block's committed `EvmExecutionHeader.state_root`.
/// On success the returned snapshot is the exact canonical state at the target,
/// proven to reproduce `expected_root`.
pub fn reconstruct_evm_state(
    seed: &EvmStateSnapshot,
    forward_diffs: &[EvmStateDiffV2],
    code_resolver: impl FnMut(&EvmH256) -> Option<Vec<u8>>,
    expected_root: EvmH256,
) -> Result<EvmStateSnapshot, ReconstructError> {
    let mut state = recon_from_snapshot(seed);
    for diff in forward_diffs {
        apply_state_diff(&mut state, diff).map_err(ReconstructError::Engine)?;
    }
    let snapshot = recon_to_snapshot(&state, code_resolver).map_err(ReconstructError::Engine)?;

    // Verify the keccak-MPT state root (real revm trie) against the committed root.
    // seed_cachedb additionally re-checks the snapshot is in canonical form.
    let db = crate::snapshot::seed_cachedb(&snapshot).map_err(ReconstructError::Seed)?;
    let computed = EvmH256::from_bytes(crate::state::state_root(&db).0);
    if computed != expected_root {
        return Err(ReconstructError::RootMismatch { expected: expected_root, computed });
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::evm::{compute_state_diff, EVM_EMPTY_CODE_HASH};
    use revm::primitives::{Address, KECCAK_EMPTY, U256};
    use std::collections::BTreeMap;

    /// Build a CacheDB-backed snapshot the canonical way (so its root is the real
    /// committed root) from (addr, nonce, balance, code, [(slot, val)]).
    fn build_snapshot(accounts: &[(u8, u64, u64, &[u8], &[(u64, u64)])]) -> (EvmStateSnapshot, EvmH256) {
        use revm::db::{CacheDB, EmptyDB};
        use revm::primitives::{AccountInfo, Bytecode};
        let mut db = CacheDB::new(EmptyDB::default());
        for (a, nonce, bal, code, storage) in accounts {
            let addr = Address::with_last_byte(*a);
            let (code_hash, bytecode) = if code.is_empty() {
                (KECCAK_EMPTY, None)
            } else {
                let bc = Bytecode::new_raw(code.to_vec().into());
                (bc.hash_slow(), Some(bc))
            };
            db.insert_account_info(addr, AccountInfo { balance: U256::from(*bal), nonce: *nonce, code_hash, code: bytecode });
            for (slot, val) in *storage {
                db.insert_account_storage(addr, U256::from(*slot), U256::from(*val)).unwrap();
            }
        }
        let snap = crate::snapshot::snapshot_from_cachedb(&db);
        let root = EvmH256::from_bytes(crate::state::state_root(&db).0);
        (snap, root)
    }

    /// A code store filled from a chain of snapshots' diffs.
    fn code_store_of(snaps: &[EvmStateSnapshot]) -> BTreeMap<[u8; 32], Vec<u8>> {
        let mut store = BTreeMap::new();
        for s in snaps {
            for acc in &s.accounts {
                if acc.code_hash != EVM_EMPTY_CODE_HASH {
                    store.insert(acc.code_hash.as_bytes(), acc.code.clone());
                }
            }
        }
        store
    }

    fn hash64(b: u8) -> kaspa_hashes::Hash64 {
        kaspa_hashes::Hash64::from_bytes([b; 64])
    }

    /// Reconstruct the target from genesis + the full forward diff chain, and
    /// verify its committed root.
    #[test]
    fn reconstructs_and_verifies_root_from_genesis() {
        let code: &[u8] = &[0x60, 0x01, 0x60, 0x02, 0x01]; // PUSH1 1 PUSH1 2 ADD
        let chain_specs: Vec<Vec<(u8, u64, u64, &[u8], &[(u64, u64)])>> = vec![
            vec![(0x01, 1, 1000, &[], &[])],
            vec![(0x01, 2, 800, &[], &[]), (0x02, 1, 0, code, &[(1, 7)])],
            vec![(0x01, 2, 800, &[], &[]), (0x02, 1, 0, code, &[(1, 7), (5, 9)])],
        ];
        let mut snaps = vec![EvmStateSnapshot::default()];
        let mut roots = vec![EvmH256::from_bytes(kaspa_consensus_core::evm::EVM_GENESIS_STATE_ROOT.as_bytes())];
        for spec in &chain_specs {
            let (s, r) = build_snapshot(spec);
            snaps.push(s);
            roots.push(r);
        }
        let code_store = code_store_of(&snaps);

        // Forward diffs S0->S1, S1->S2, S2->S3.
        let diffs: Vec<EvmStateDiffV2> =
            (1..snaps.len()).map(|i| compute_state_diff(&snaps[i - 1], &snaps[i], hash64(i as u8), hash64((i - 1) as u8))).collect();

        // Reconstruct the last block from the genesis seed + all diffs.
        let target = snaps.len() - 1;
        let got = reconstruct_evm_state(&EvmStateSnapshot::default(), &diffs, |h| code_store.get(&h.as_bytes()).cloned(), roots[target])
            .expect("reconstruction verifies");
        assert_eq!(got, snaps[target]);
    }

    /// Seeding from a mid-chain checkpoint + only the remaining diffs yields the
    /// same verified state (checkpoint anchor path).
    #[test]
    fn reconstructs_from_midchain_checkpoint() {
        let specs: Vec<Vec<(u8, u64, u64, &[u8], &[(u64, u64)])>> = vec![
            vec![(0x0A, 1, 500, &[], &[])],
            vec![(0x0A, 2, 400, &[], &[(2, 2)])],
            vec![(0x0A, 3, 300, &[], &[(2, 2), (3, 3)])],
        ];
        let mut snaps = vec![EvmStateSnapshot::default()];
        let mut roots = vec![EvmH256::from_bytes(kaspa_consensus_core::evm::EVM_GENESIS_STATE_ROOT.as_bytes())];
        for s in &specs {
            let (snap, r) = build_snapshot(s);
            snaps.push(snap);
            roots.push(r);
        }
        // Use snaps[1] as the checkpoint seed; apply diffs 2,3.
        let d2 = compute_state_diff(&snaps[1], &snaps[2], hash64(2), hash64(1));
        let d3 = compute_state_diff(&snaps[2], &snaps[3], hash64(3), hash64(2));
        let got = reconstruct_evm_state(&snaps[1], &[d2, d3], |_| None, roots[3]).expect("midchain reconstruction verifies");
        assert_eq!(got, snaps[3]);
    }

    /// A wrong committed root is rejected (corruption), not silently accepted.
    #[test]
    fn wrong_root_fails_closed() {
        let (s1, _r1) = build_snapshot(&[(0x01, 1, 100, &[], &[])]);
        let d1 = compute_state_diff(&EvmStateSnapshot::default(), &s1, hash64(1), hash64(0));
        let bogus = EvmH256::from_bytes([0xAB; 32]);
        let err = reconstruct_evm_state(&EvmStateSnapshot::default(), &[d1], |_| None, bogus).unwrap_err();
        assert!(matches!(err, ReconstructError::RootMismatch { .. }));
    }

    /// Missing bytecode fails closed (no empty fallback).
    #[test]
    fn missing_code_fails_closed() {
        let code: &[u8] = &[0xfe];
        let (s1, r1) = build_snapshot(&[(0x07, 1, 0, code, &[])]);
        let d1 = compute_state_diff(&EvmStateSnapshot::default(), &s1, hash64(1), hash64(0));
        // Empty resolver → the contract's code can't be resolved.
        let err = reconstruct_evm_state(&EvmStateSnapshot::default(), &[d1], |_| None, r1).unwrap_err();
        assert!(matches!(err, ReconstructError::Engine(StateDiffError::MissingCode(_))));
    }
}
