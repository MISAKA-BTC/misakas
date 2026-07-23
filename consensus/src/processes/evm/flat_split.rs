//! C-01 Stage 2: the split flat state, and reading across the migration.
//!
//! Stage 1 stores an account's entire storage vector in one row (prefix 234).
//! Storage is O(live state), which is right, but a single-slot write has to
//! decode, mutate, re-encode and rewrite ALL of it. A contract with 100k slots
//! pays megabytes of write amplification to change one word — and the temporary
//! space RocksDB needs to compact that churn is disk a node does not have while
//! it is already short of it, which is the situation this whole change exists
//! for.
//!
//! Stage 2 splits the row: `address → AccountCore` (230) plus one row per
//! non-zero slot (233). A one-slot write becomes one row; zeroing becomes a
//! delete; an account's slots share an address prefix, so materializing it is
//! still one range scan.
//!
//! Migration is dual-read, not a stop-the-world rewrite. V2 is authoritative
//! where it exists and V1 answers otherwise, so a node can migrate in the
//! background while serving. The verification that matters is not "did every
//! account move" but "does the state root still match" — the split changes the
//! layout, and any layout that produces a different root is wrong regardless of
//! how many rows it converted.

use crate::model::stores::evm::{DbEvmFlatAccountCoreStore, DbEvmFlatAccountStore, DbEvmFlatStorageStore};
use kaspa_consensus_core::evm::{AccountChange, EVM_EMPTY_CODE_HASH, EvmAddress, EvmStateDiffV2, EvmU256, FlatAccount};
use kaspa_database::prelude::{DB, StoreError};
use rocksdb::WriteBatch;
use std::sync::Arc;

/// How many accounts one background migration step converts.
///
/// Bounded for the same reason prune passes are: a migration that stalls block
/// processing has swapped one operational problem for another.
pub const MIGRATION_BATCH_ACCOUNTS: usize = 512;

pub struct FlatSplitStores {
    pub db: Arc<DB>,
    /// Stage 1, whole-account rows. Read until migrated, then reclaimed.
    pub v1: Arc<DbEvmFlatAccountStore>,
    pub cores: Arc<DbEvmFlatAccountCoreStore>,
    pub slots: Arc<DbEvmFlatStorageStore>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MigrationStep {
    pub accounts_migrated: u64,
    pub slots_written: u64,
    /// No v1 rows left — the split is complete and v1 can be retired.
    pub complete: bool,
}

impl FlatSplitStores {
    /// Read one account, v2 first.
    ///
    /// V2 is authoritative WHEREVER IT EXISTS, including for an account whose
    /// core row says it has no slots. Falling back to v1 on an empty slot scan
    /// would resurrect storage a v2 write had just cleared — the classic
    /// dual-read bug, where "absent" and "empty" are conflated in the direction
    /// that loses the newer write.
    pub fn get(&self, address: EvmAddress) -> Result<Option<FlatAccount>, StoreError> {
        if let Some(core) = self.cores.get(address)? {
            return Ok(Some(FlatAccount { core, storage: self.slots.account_slots(address)? }));
        }
        self.v1.get(address)
    }

    /// Write one account into the split layout.
    ///
    /// Takes the PREVIOUS storage so slots that disappeared are deleted rather
    /// than left behind: without that, a shrinking account keeps rows nothing
    /// references, and the store stops tracking live state.
    pub fn write(
        &self,
        batch: &mut WriteBatch,
        address: EvmAddress,
        account: &FlatAccount,
        previous_slots: &[(EvmU256, EvmU256)],
    ) -> Result<u64, StoreError> {
        self.cores.write_batch(batch, address, account.core.clone())?;
        let mut writes = 0;
        for (slot, value) in &account.storage {
            self.slots.set_batch(batch, address, *slot, *value)?;
            writes += 1;
        }
        for (slot, _) in previous_slots {
            if !account.storage.iter().any(|(s, _)| s == slot) {
                self.slots.set_batch(batch, address, *slot, EvmU256::ZERO)?;
                writes += 1;
            }
        }
        Ok(writes)
    }

    /// Delete an account entirely (SELFDESTRUCT / EIP-161 empty).
    pub fn delete(&self, batch: &mut WriteBatch, address: EvmAddress) -> Result<u64, StoreError> {
        self.cores.delete_batch(batch, address)?;
        // v1's row goes too, or a dual read would resurrect the account from a
        // layout the node has stopped writing.
        self.v1.delete_batch(batch, address)?;
        self.slots.delete_account_batch(batch, address)
    }

    /// Apply a forward diff to the split layout — the Stage 2 analogue of
    /// `apply_diff_to_flat`.
    ///
    /// The saving is visible here: a diff touching three slots of a large
    /// contract writes three rows, where Stage 1 rewrote the account's entire
    /// storage vector.
    pub fn apply_diff(&self, batch: &mut WriteBatch, diff: &EvmStateDiffV2) -> Result<u64, StoreError> {
        let mut writes = 0;
        for change in &diff.account_changes {
            writes += self.apply_account_change(batch, change)?;
        }
        Ok(writes)
    }

    fn apply_account_change(&self, batch: &mut WriteBatch, change: &AccountChange) -> Result<u64, StoreError> {
        let address = change.address;
        let Some(after) = &change.after else {
            return self.delete(batch, address);
        };
        // An EIP-161-empty account is not stored: nonce 0, balance 0, no code.
        if after.nonce == 0 && after.balance == EvmU256::ZERO && after.code_hash == EVM_EMPTY_CODE_HASH {
            return self.delete(batch, address);
        }
        self.cores.write_batch(batch, address, after.clone())?;
        let mut writes = 1;
        for sc in &change.storage_changes {
            self.slots.set_batch(batch, address, sc.slot, sc.after)?;
            writes += 1;
        }
        Ok(writes)
    }

    /// Convert one bounded batch of v1 accounts. Idempotent, so a crash mid-step
    /// costs at most a repeated batch.
    pub fn migrate_step(&self, batch_accounts: usize) -> Result<MigrationStep, StoreError> {
        let mut batch = WriteBatch::default();
        let mut step = MigrationStep::default();

        for row in self.v1.iter().take(batch_accounts) {
            let (address, account) = row?;
            // The previous slot set is empty: a v2 row for this account cannot
            // exist yet, because v1 is only read for accounts v2 does not have.
            step.slots_written += self.write(&mut batch, address, &account, &[])?;
            self.v1.delete_batch(&mut batch, address)?;
            step.accounts_migrated += 1;
        }
        step.complete = step.accounts_migrated == 0;
        self.db.write(batch).map_err(StoreError::DbError)?;
        Ok(step)
    }

    /// Materialize the whole split state as `(address, FlatAccount)` pairs, in
    /// address order — the input to the state-root recompute.
    ///
    /// Includes any not-yet-migrated v1 accounts, so the root is correct at every
    /// point DURING the migration and not only at its end. A migration that only
    /// verifies at completion cannot be run incrementally on a live node.
    pub fn materialize(&self) -> Result<Vec<(EvmAddress, FlatAccount)>, StoreError> {
        let mut out: Vec<(EvmAddress, FlatAccount)> = Vec::new();
        for row in self.cores.iter() {
            let (address, core) = row?;
            out.push((address, FlatAccount { core, storage: self.slots.account_slots(address)? }));
        }
        for row in self.v1.iter() {
            let (address, account) = row?;
            if !out.iter().any(|(a, _)| *a == address) {
                out.push((address, account));
            }
        }
        out.sort_by(|(a, _), (b, _)| a.as_bytes().cmp(&b.as_bytes()));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::evm::{AccountCore, StorageChange};
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::{CachePolicy, ConnBuilder};

    fn addr(b: u8) -> EvmAddress {
        EvmAddress::from_bytes([b; 20])
    }

    fn u(v: u128) -> EvmU256 {
        EvmU256::from_u128(v)
    }

    fn core(nonce: u64) -> AccountCore {
        AccountCore { nonce, balance: u(1_000), code_hash: EVM_EMPTY_CODE_HASH }
    }

    fn stores() -> (kaspa_database::utils::DbLifetime, FlatSplitStores) {
        let (lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let s = FlatSplitStores {
            db: db.clone(),
            v1: Arc::new(DbEvmFlatAccountStore::new(db.clone(), CachePolicy::Empty)),
            cores: Arc::new(DbEvmFlatAccountCoreStore::new(db.clone(), CachePolicy::Empty)),
            slots: Arc::new(DbEvmFlatStorageStore::new(db, CachePolicy::Empty)),
        };
        (lt, s)
    }

    fn commit(s: &FlatSplitStores, f: impl FnOnce(&mut WriteBatch)) {
        let mut b = WriteBatch::default();
        f(&mut b);
        s.db.write(b).unwrap();
    }

    #[test]
    fn slots_round_trip_and_zero_means_absent() {
        let (_lt, s) = stores();
        let account = FlatAccount { core: core(1), storage: vec![(u(1), u(11)), (u(2), u(22))] };
        commit(&s, |b| {
            s.write(b, addr(1), &account, &[]).unwrap();
        });
        assert_eq!(s.get(addr(1)).unwrap(), Some(account));

        // Writing zero DELETES: the EVM makes no distinction between a slot set to
        // zero and a slot never set, and storing zeros would grow the store with
        // writes instead of with live state.
        commit(&s, |b| {
            s.slots.set_batch(b, addr(1), u(1), EvmU256::ZERO).unwrap();
        });
        assert_eq!(s.get(addr(1)).unwrap().unwrap().storage, vec![(u(2), u(22))]);
    }

    #[test]
    fn a_one_slot_update_touches_exactly_one_slot_row() {
        // The entire point of the split. Stage 1 rewrote the account's whole
        // storage vector for this.
        let (_lt, s) = stores();
        let big: Vec<_> = (0..1000u128).map(|i| (u(i), u(i + 1))).collect();
        commit(&s, |b| {
            s.write(b, addr(2), &FlatAccount { core: core(1), storage: big.clone() }, &[]).unwrap();
        });

        let diff = EvmStateDiffV2 {
            account_changes: vec![AccountChange {
                address: addr(2),
                before: Some(core(1)),
                after: Some(core(2)),
                storage_changes: vec![StorageChange { slot: u(500), before: u(501), after: u(999) }],
            }],
            ..Default::default()
        };
        let writes = {
            let mut b = WriteBatch::default();
            let w = s.apply_diff(&mut b, &diff).unwrap();
            s.db.write(b).unwrap();
            w
        };
        // One core row + one slot row. Not 1000.
        assert_eq!(writes, 2, "a one-slot change must not rewrite the account's storage");

        let after = s.get(addr(2)).unwrap().unwrap();
        assert_eq!(after.core.nonce, 2);
        assert_eq!(after.storage.len(), 1000, "the untouched slots are still there");
        assert_eq!(after.storage.iter().find(|(k, _)| *k == u(500)).unwrap().1, u(999));
    }

    #[test]
    fn a_shrinking_account_does_not_leave_orphan_slots() {
        let (_lt, s) = stores();
        let before = vec![(u(1), u(1)), (u(2), u(2)), (u(3), u(3))];
        commit(&s, |b| {
            s.write(b, addr(3), &FlatAccount { core: core(1), storage: before.clone() }, &[]).unwrap();
        });
        commit(&s, |b| {
            s.write(b, addr(3), &FlatAccount { core: core(2), storage: vec![(u(2), u(22))] }, &before).unwrap();
        });
        assert_eq!(s.get(addr(3)).unwrap().unwrap().storage, vec![(u(2), u(22))], "slots that vanished must be deleted");
    }

    #[test]
    fn dual_read_prefers_v2_even_when_the_account_has_no_slots() {
        // The dual-read bug this guards: treating an empty slot scan as "not
        // migrated" would fall through to v1 and resurrect storage that v2 had
        // just cleared.
        let (_lt, s) = stores();
        commit(&s, |b| {
            s.v1.write_batch(b, addr(4), FlatAccount { core: core(1), storage: vec![(u(9), u(9))] }).unwrap();
        });
        assert_eq!(s.get(addr(4)).unwrap().unwrap().storage, vec![(u(9), u(9))], "v1 answers before migration");

        commit(&s, |b| {
            s.cores.write_batch(b, addr(4), core(2)).unwrap();
        });
        let read = s.get(addr(4)).unwrap().unwrap();
        assert_eq!(read.core.nonce, 2);
        assert!(read.storage.is_empty(), "a v2 core with no slots means NO storage, not 'ask v1'");
    }

    #[test]
    fn migration_is_incremental_bounded_and_idempotent() {
        let (_lt, s) = stores();
        for i in 0..10u8 {
            commit(&s, |b| {
                s.v1.write_batch(b, addr(i), FlatAccount { core: core(i as u64 + 1), storage: vec![(u(1), u(i as u128 + 1))] })
                    .unwrap();
            });
        }
        let before = s.materialize().unwrap();

        // Bounded steps, and the state is unchanged at every point in between —
        // which is what lets the migration run on a live node.
        let mut steps = 0;
        loop {
            let step = s.migrate_step(3).unwrap();
            assert!(step.accounts_migrated <= 3, "a step must respect its bound");
            assert_eq!(s.materialize().unwrap(), before, "the state must be identical mid-migration");
            if step.complete {
                break;
            }
            steps += 1;
            assert!(steps < 100, "migration must terminate");
        }
        assert!(steps >= 3, "10 accounts in batches of 3 must take several steps");

        // Idempotent once done.
        assert!(s.migrate_step(3).unwrap().complete);
        assert_eq!(s.materialize().unwrap(), before);
    }

    #[test]
    fn destroying_an_account_removes_its_core_slots_and_v1_row() {
        // A leftover v1 row would let a dual read resurrect a destroyed account.
        let (_lt, s) = stores();
        commit(&s, |b| {
            s.v1.write_batch(b, addr(5), FlatAccount { core: core(1), storage: vec![(u(1), u(1))] }).unwrap();
            s.write(b, addr(5), &FlatAccount { core: core(1), storage: vec![(u(1), u(1)), (u(2), u(2))] }, &[]).unwrap();
        });
        commit(&s, |b| {
            s.delete(b, addr(5)).unwrap();
        });
        assert_eq!(s.get(addr(5)).unwrap(), None);
        assert!(s.slots.account_slots(addr(5)).unwrap().is_empty());
    }

    #[test]
    fn an_eip161_empty_account_is_removed_rather_than_stored() {
        let (_lt, s) = stores();
        commit(&s, |b| {
            s.write(b, addr(6), &FlatAccount { core: core(1), storage: vec![(u(1), u(1))] }, &[]).unwrap();
        });
        let diff = EvmStateDiffV2 {
            account_changes: vec![AccountChange {
                address: addr(6),
                before: Some(core(1)),
                after: Some(AccountCore { nonce: 0, balance: EvmU256::ZERO, code_hash: EVM_EMPTY_CODE_HASH }),
                storage_changes: vec![],
            }],
            ..Default::default()
        };
        commit(&s, |b| {
            s.apply_diff(b, &diff).unwrap();
        });
        assert_eq!(s.get(addr(6)).unwrap(), None, "an EIP-161-empty account must not occupy a row");
    }

    /// The property that actually decides whether the split is correct.
    ///
    /// A layout change is a bug the moment it produces a different state root,
    /// however many rows it converted; and the root is what the shadow check and
    /// the anchors compare against. Recomputed here through the real keccak-MPT,
    /// not asserted structurally.
    #[test]
    #[cfg(feature = "evm")]
    fn the_split_layout_produces_a_byte_identical_state_root() {
        use crate::model::stores::evm::DbEvmCodeStore;

        let (_lt, s) = stores();
        let code = DbEvmCodeStore::new(s.db.clone(), CachePolicy::Empty);

        // A state with a contract, an EOA, and an account whose storage is large
        // enough that the layouts differ in every way except the root.
        let accounts: Vec<(EvmAddress, FlatAccount)> = vec![
            (addr(0x11), FlatAccount { core: core(3), storage: vec![] }),
            (addr(0x22), FlatAccount { core: core(1), storage: (0..64u128).map(|i| (u(i), u(i * 7 + 1))).collect() }),
            (addr(0x33), FlatAccount { core: core(9), storage: vec![(u(1), u(1))] }),
        ];
        commit(&s, |b| {
            for (address, account) in &accounts {
                // v1 layout AND split layout, side by side.
                s.v1.write_batch(b, *address, account.clone()).unwrap();
                s.write(b, *address, account, &[]).unwrap();
            }
        });

        let root_of = |snapshot: &kaspa_consensus_core::evm::EvmStateSnapshot| {
            let cdb = kaspa_evm::snapshot::seed_cachedb(snapshot).unwrap();
            kaspa_hashes::EvmH256::from_bytes(kaspa_evm::state::state_root(&cdb).0)
        };

        let v1_snapshot = super::super::materialize_snapshot(&s.v1, &code).unwrap();
        let split_snapshot = kaspa_consensus_core::evm::EvmStateSnapshot {
            accounts: s.materialize().unwrap().into_iter().map(|(a, fa)| fa.to_snapshot(a, Vec::new())).collect(),
        };

        assert_eq!(root_of(&v1_snapshot), root_of(&split_snapshot), "the split layout must reproduce the v1 state root exactly");
    }

    #[test]
    fn account_slots_do_not_leak_across_addresses() {
        // Address-first keys make an account's slots contiguous; a prefix bug
        // would silently mix two contracts' storage.
        let (_lt, s) = stores();
        commit(&s, |b| {
            s.write(b, addr(7), &FlatAccount { core: core(1), storage: vec![(u(1), u(70))] }, &[]).unwrap();
            s.write(b, addr(8), &FlatAccount { core: core(1), storage: vec![(u(1), u(80)), (u(2), u(81))] }, &[]).unwrap();
        });
        assert_eq!(s.slots.account_slots(addr(7)).unwrap(), vec![(u(1), u(70))]);
        assert_eq!(s.slots.account_slots(addr(8)).unwrap(), vec![(u(1), u(80)), (u(2), u(81))]);
    }
}
