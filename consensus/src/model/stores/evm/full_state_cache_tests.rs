use super::*;
use kaspa_consensus_core::evm::EvmAccountSnapshot;
use kaspa_database::{create_temp_db, prelude::ConnBuilder};
use kaspa_utils::mem_size::{MemMode, MemSizeEstimator};

fn policy(bytes: usize) -> CachePolicy {
    CachePolicy::Tracked { max_size: bytes, min_items: 0, mem_mode: MemMode::Bytes }
}

fn snapshot(tag: u8, code_bytes: usize) -> EvmStateSnapshot {
    EvmStateSnapshot {
        accounts: vec![EvmAccountSnapshot {
            address: EvmAddress::from_bytes([tag; 20]),
            code: vec![tag; code_bytes],
            storage: vec![(Default::default(), Default::default()); 8],
            ..Default::default()
        }],
    }
}

fn commit(store: &DbEvmStateStore, db: &DB, hash: BlockHash, snapshot: EvmStateSnapshot) {
    let mut batch = WriteBatch::default();
    store.insert_batch(&mut batch, hash, snapshot).unwrap();
    db.write(batch).unwrap();
}

#[test]
fn oversized_snapshot_is_not_cached_and_reloads_byte_identically() {
    let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let state = snapshot(7, 8192);
    let canonical = borsh::to_vec(&state).unwrap();
    for budget in [0, 4096] {
        let store = DbEvmStateStore::new(db.clone(), policy(budget));
        let hash = BlockHash::from(100 + budget as u64);
        assert!(state.estimate_mem_bytes() > budget);
        commit(&store, &db, hash, state.clone());
        assert!(store.access.read_from_cache(hash).is_none(), "no minimum entry can pin an oversized state");
        for _ in 0..2 {
            let loaded = store.get(hash).unwrap();
            assert_eq!(loaded, state);
            assert_eq!(borsh::to_vec(&loaded).unwrap(), canonical);
            assert!(store.access.read_from_cache(hash).is_none(), "cold reload must also obey the cache ceiling");
        }
        let restarted = DbEvmStateStore::new(db.clone(), policy(budget));
        assert_eq!(borsh::to_vec(&restarted.get(hash).unwrap()).unwrap(), canonical);
    }
}

#[test]
fn many_full_snapshots_evict_to_byte_budget_without_changing_db_answers() {
    let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let budget = snapshot(1, 2048).estimate_mem_bytes() * 2;
    let store = DbEvmStateStore::new(db.clone(), policy(budget));
    let states = (1..=8u8).map(|tag| (BlockHash::from(tag as u64), snapshot(tag, 2048))).collect::<Vec<_>>();
    let assert_cache_bound = || {
        // Fixtures have exact-length Vec capacities, so cloned cache probes
        // have the same capacity-based weight as these cached allocations.
        let cached = states.iter().filter_map(|(hash, _)| store.access.read_from_cache(*hash)).collect::<Vec<_>>();
        assert!(cached.len() <= 2, "64 full generations must not survive merely because their item count fits");
        assert!(cached.iter().map(MemSizeEstimator::estimate_mem_bytes).sum::<usize>() <= budget);
    };
    for (hash, state) in &states {
        commit(&store, &db, *hash, state.clone());
        assert_cache_bound();
    }
    for (hash, state) in &states {
        assert_eq!(borsh::to_vec(&store.get(*hash).unwrap()).unwrap(), borsh::to_vec(state).unwrap());
        assert_cache_bound();
    }
    let uncached = DbEvmStateStore::new(db, CachePolicy::Empty);
    for (hash, state) in &states {
        assert_eq!(uncached.get(*hash).unwrap(), *state, "eviction never deletes or rewrites persisted state");
    }
}
