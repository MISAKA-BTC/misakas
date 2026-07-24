//! Mark-and-sweep for the content-addressed code store (prefix 222).
//!
//! 222 is the one EVM store that cannot be reclaimed by block range. It is keyed
//! by `code_hash`, and every account, state diff and anchor that references that
//! hash shares the single entry. Deleting it when one referencing block is pruned
//! would corrupt every other holder — and unlike an index, bytecode is not
//! rebuildable from anything the node still has.
//!
//! So reachability decides, and reachability is computed rather than counted.
//! Reference counting was rejected deliberately: a count has to be correct across
//! reorgs, migrations, crashes mid-batch and rows written by older formats, and a
//! count that drifts low deletes live code. A mark pass is stateless — it asks
//! the current stores what they reference, so it cannot inherit a past mistake.
//!
//! Sweeping goes through a QUARANTINE. One pass finding a hash unreachable is an
//! opinion formed at one instant, while commits are happening; requiring several
//! consecutive passes to agree converts a transient mark miss from data loss into
//! a delayed deletion. The asymmetry is the point: keeping dead code costs disk,
//! deleting live code costs the state.

use crate::model::stores::evm::{
    DbEvmCodeQuarantineStore, DbEvmCodeStore, DbEvmFlatAccountStore, DbEvmStateCheckpointStore, DbEvmStateCheckpointV2Store,
    DbEvmStateDiffStore, DbEvmStateStore,
};
use kaspa_consensus_core::evm::EVM_EMPTY_CODE_HASH;
use kaspa_database::prelude::{DB, StoreError};
use kaspa_hashes::EvmH256;
use rocksdb::WriteBatch;
use std::collections::HashSet;
use std::sync::Arc;

/// How many consecutive passes must agree before a hash is deleted.
///
/// Two, not one: a single pass runs concurrently with block commits, so a hash
/// deployed moments before the mark could be missed. Two passes with a full
/// interval between them cannot both miss a hash that is genuinely referenced.
pub const DEFAULT_QUARANTINE_EPOCHS: u64 = 2;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CodeGcReport {
    pub live: u64,
    pub quarantined: u64,
    pub released: u64,
    pub deleted: u64,
    /// Set when the pass refused to sweep because the mark could not be trusted.
    /// Distinguishes "nothing to collect" from "did not look", which are the same
    /// number and opposite situations.
    pub aborted: bool,
}

pub struct CodeGcStores {
    pub db: Arc<DB>,
    pub code: Arc<DbEvmCodeStore>,
    pub quarantine: Arc<DbEvmCodeQuarantineStore>,
    pub flat: Arc<DbEvmFlatAccountStore>,
    pub diffs: Arc<DbEvmStateDiffStore>,
    pub anchors_v1: Arc<DbEvmStateCheckpointStore>,
    pub anchors_v2: Arc<DbEvmStateCheckpointV2Store>,
    /// The legacy per-block snapshot store. Still a mark root while a migrating
    /// database has rows in it: those snapshots inline their own bytecode, but a
    /// node mid-migration may be reading code through 222 for them.
    pub legacy_snapshots: Arc<DbEvmStateStore>,
}

/// Collect every code hash the node still depends on.
///
/// Returns `None` if any root could not be read completely. A PARTIAL mark set is
/// worse than no mark set: it looks like a valid answer and every hash it failed
/// to see becomes a deletion candidate. Fail closed.
pub fn mark_live_code(stores: &CodeGcStores) -> Option<HashSet<EvmH256>> {
    let mut live = HashSet::new();
    // The empty-code hash is never stored, but treat it as live so a bug that did
    // store it cannot cascade.
    live.insert(EVM_EMPTY_CODE_HASH);

    // Root 1: the current flat state. Every live account's code.
    for row in stores.flat.iter() {
        let (_, account) = row.ok()?;
        live.insert(account.core.code_hash);
    }

    // Root 2: retained forward diffs. A reorg replays these, so the code they
    // reference on BOTH sides has to survive — `before` matters as much as
    // `after`, because an inverse application restores the pre-state.
    for row in stores.diffs.iter() {
        let (_, diff) = row.ok()?;
        for change in &diff.account_changes {
            if let Some(before) = &change.before {
                live.insert(before.code_hash);
            }
            if let Some(after) = &change.after {
                live.insert(after.code_hash);
            }
        }
    }

    // Root 3: retained anchors. A v2 anchor holds only code hashes, so it is
    // MEANINGLESS without the code store — these are exactly the references that
    // would silently break.
    for row in stores.anchors_v2.iter() {
        let (_, anchor) = row.ok()?;
        for hash in anchor.referenced_code_hashes().ok()? {
            live.insert(hash);
        }
    }
    // Legacy v1 anchors inline their code, so they do not strictly need 222.
    // Marked anyway: a mixed database may already be serving them through it.
    for row in stores.anchors_v1.iter() {
        let (_, anchor) = row.ok()?;
        for account in anchor.decode_snapshot().ok()?.accounts {
            live.insert(account.code_hash);
        }
    }

    // Root 4: surviving legacy 206 snapshots, for the same reason.
    for row in stores.legacy_snapshots.iter() {
        let (_, snapshot) = row.ok()?;
        for account in snapshot.accounts {
            live.insert(account.code_hash);
        }
    }

    Some(live)
}

impl CodeGcStores {
    /// One pass: mark, then sweep through the quarantine.
    ///
    /// `epoch` is a monotonically increasing pass counter. `quarantine_epochs` is
    /// how many passes a hash must stay unreachable before deletion.
    pub fn run(&self, epoch: u64, quarantine_epochs: u64) -> Result<CodeGcReport, StoreError> {
        let Some(live) = mark_live_code(self) else {
            // A root could not be fully read. Sweeping on a partial mark would
            // delete live bytecode, so do not sweep at all.
            return Ok(CodeGcReport { aborted: true, ..Default::default() });
        };

        let mut report = CodeGcReport { live: live.len() as u64, ..Default::default() };
        let mut batch = WriteBatch::default();

        for row in self.code.iter_hashes() {
            let hash = row?;
            if live.contains(&hash) {
                // Reachable again: release it. A hash can re-enter the live set
                // when a contract is redeployed or a reorg restores a diff, and a
                // release must be able to undo a quarantine.
                if self.quarantine.get(hash)?.is_some() {
                    self.quarantine.delete_batch(&mut batch, hash)?;
                    report.released += 1;
                }
                continue;
            }
            match self.quarantine.get(hash)? {
                None => {
                    self.quarantine.set_batch(&mut batch, hash, epoch)?;
                    report.quarantined += 1;
                }
                Some(since) if epoch.saturating_sub(since) >= quarantine_epochs => {
                    self.code.delete_batch(&mut batch, hash)?;
                    self.quarantine.delete_batch(&mut batch, hash)?;
                    report.deleted += 1;
                }
                // Still serving its sentence.
                Some(_) => {}
            }
        }

        self.db.write(batch).map_err(StoreError::DbError)?;
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::stores::evm::{EvmCodeStoreReader, EvmStateStore};
    use kaspa_consensus_core::evm::{
        AccountChange, AccountCore, EvmAccountSnapshot, EvmAddress, EvmStateDiffV2, EvmStateSnapshot, EvmU256, FlatAccount,
    };
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::{CachePolicy, ConnBuilder};

    fn h256(b: u8) -> EvmH256 {
        EvmH256::from_bytes([b; 32])
    }

    fn addr(b: u8) -> EvmAddress {
        EvmAddress::from_bytes([b; 20])
    }

    fn core(code_hash: EvmH256) -> AccountCore {
        AccountCore { nonce: 1, balance: EvmU256::from_u128(1), code_hash }
    }

    fn stores(db: Arc<DB>) -> CodeGcStores {
        CodeGcStores {
            db: db.clone(),
            code: Arc::new(DbEvmCodeStore::new(db.clone(), CachePolicy::Empty)),
            quarantine: Arc::new(DbEvmCodeQuarantineStore::new(db.clone())),
            flat: Arc::new(DbEvmFlatAccountStore::new(db.clone(), CachePolicy::Empty)),
            diffs: Arc::new(DbEvmStateDiffStore::new(db.clone(), CachePolicy::Empty)),
            anchors_v1: Arc::new(DbEvmStateCheckpointStore::new(db.clone(), CachePolicy::Empty)),
            anchors_v2: Arc::new(DbEvmStateCheckpointV2Store::new(db.clone(), CachePolicy::Empty)),
            legacy_snapshots: Arc::new(DbEvmStateStore::new(db, CachePolicy::Empty)),
        }
    }

    fn put_code(s: &CodeGcStores, hash: EvmH256) {
        let mut b = WriteBatch::default();
        s.code.write_batch(&mut b, hash, vec![1, 2, 3]).unwrap();
        s.db.write(b).unwrap();
    }

    #[test]
    fn unreachable_code_survives_the_first_pass_and_dies_on_the_second() {
        // The whole reason for the quarantine: one pass is an opinion formed while
        // commits are in flight.
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let s = stores(db);
        put_code(&s, h256(0xAA));

        let r = s.run(1, DEFAULT_QUARANTINE_EPOCHS).unwrap();
        assert_eq!((r.quarantined, r.deleted), (1, 0), "a first sighting must not delete");
        assert!(s.code.get(h256(0xAA)).unwrap().is_some(), "the bytes must still be there after one pass");

        let r = s.run(1 + DEFAULT_QUARANTINE_EPOCHS, DEFAULT_QUARANTINE_EPOCHS).unwrap();
        assert_eq!(r.deleted, 1);
        assert!(s.code.get(h256(0xAA)).unwrap().is_none());
    }

    #[test]
    fn code_referenced_by_the_live_flat_state_is_never_collected() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let s = stores(db);
        put_code(&s, h256(0xAA));
        let mut b = WriteBatch::default();
        s.flat.write_batch(&mut b, addr(1), FlatAccount { core: core(h256(0xAA)), storage: vec![] }).unwrap();
        s.db.write(b).unwrap();

        for epoch in 1..8 {
            s.run(epoch, DEFAULT_QUARANTINE_EPOCHS).unwrap();
        }
        assert!(s.code.get(h256(0xAA)).unwrap().is_some(), "live account code must never be swept");
    }

    #[test]
    fn a_retained_diff_keeps_both_sides_of_a_code_change_alive() {
        // `before` matters as much as `after`: applying a diff inversely on a reorg
        // restores the pre-state, which needs the pre-state's code.
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let s = stores(db);
        put_code(&s, h256(0xB1));
        put_code(&s, h256(0xB2));

        let diff = EvmStateDiffV2 {
            account_changes: vec![AccountChange {
                address: addr(2),
                before: Some(core(h256(0xB1))),
                after: Some(core(h256(0xB2))),
                storage_changes: vec![],
            }],
            ..Default::default()
        };
        let mut b = WriteBatch::default();
        s.diffs.insert_batch(&mut b, kaspa_consensus_core::BlockHash::from_bytes([7; 64]), diff).unwrap();
        s.db.write(b).unwrap();

        for epoch in 1..8 {
            s.run(epoch, DEFAULT_QUARANTINE_EPOCHS).unwrap();
        }
        assert!(s.code.get(h256(0xB1)).unwrap().is_some(), "the pre-state code of a retained diff must survive");
        assert!(s.code.get(h256(0xB2)).unwrap().is_some());
    }

    #[test]
    fn a_v2_anchor_keeps_the_code_it_can_no_longer_carry_itself() {
        // A v2 anchor stores only code hashes, so it is meaningless without 222 —
        // exactly the reference that would silently break.
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let s = stores(db);
        put_code(&s, h256(0xC1));

        let snapshot = EvmStateSnapshot {
            accounts: vec![EvmAccountSnapshot {
                address: addr(3),
                nonce: 1,
                balance: EvmU256::from_u128(1),
                code_hash: h256(0xC1),
                code: vec![1, 2, 3],
                storage: vec![],
            }],
        };
        let anchor = kaspa_consensus_core::evm::EvmStateCheckpointV2::build(
            kaspa_consensus_core::BlockHash::from_bytes([8; 64]),
            5,
            h256(0x55),
            &snapshot,
            kaspa_consensus_core::evm::CheckpointCodec::Deflate,
        );
        let mut b = WriteBatch::default();
        s.anchors_v2.insert_batch(&mut b, kaspa_consensus_core::BlockHash::from_bytes([8; 64]), anchor).unwrap();
        s.db.write(b).unwrap();

        for epoch in 1..8 {
            s.run(epoch, DEFAULT_QUARANTINE_EPOCHS).unwrap();
        }
        assert!(s.code.get(h256(0xC1)).unwrap().is_some(), "anchor-referenced code must survive");
    }

    #[test]
    fn becoming_reachable_again_releases_a_quarantined_hash() {
        // Redeployment and reorgs both put a hash back into the live set; a
        // quarantine that could not be undone would delete it anyway.
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let s = stores(db);
        put_code(&s, h256(0xDD));

        assert_eq!(s.run(1, DEFAULT_QUARANTINE_EPOCHS).unwrap().quarantined, 1);
        let mut b = WriteBatch::default();
        s.flat.write_batch(&mut b, addr(4), FlatAccount { core: core(h256(0xDD)), storage: vec![] }).unwrap();
        s.db.write(b).unwrap();

        let r = s.run(2, DEFAULT_QUARANTINE_EPOCHS).unwrap();
        assert_eq!(r.released, 1, "a hash back in the live set must leave quarantine");

        // And it stays: the release must have cleared the record, not just skipped
        // one pass.
        for epoch in 3..10 {
            s.run(epoch, DEFAULT_QUARANTINE_EPOCHS).unwrap();
        }
        assert!(s.code.get(h256(0xDD)).unwrap().is_some());
    }

    #[test]
    fn nothing_is_swept_when_the_mark_set_is_empty_of_roots_but_code_is_live_in_206() {
        // A database still holding legacy 206 snapshots is mid-migration; its
        // accounts are roots too.
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let s = stores(db);
        put_code(&s, h256(0xEE));

        let snapshot = EvmStateSnapshot {
            accounts: vec![EvmAccountSnapshot {
                address: addr(5),
                nonce: 1,
                balance: EvmU256::from_u128(1),
                code_hash: h256(0xEE),
                code: vec![1, 2, 3],
                storage: vec![],
            }],
        };
        let mut b = WriteBatch::default();
        s.legacy_snapshots.insert_batch(&mut b, kaspa_consensus_core::BlockHash::from_bytes([9; 64]), snapshot).unwrap();
        s.db.write(b).unwrap();

        for epoch in 1..8 {
            s.run(epoch, DEFAULT_QUARANTINE_EPOCHS).unwrap();
        }
        assert!(s.code.get(h256(0xEE)).unwrap().is_some(), "a migrating database's 206 accounts are mark roots");
    }

    #[test]
    fn the_empty_code_hash_is_always_live() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let s = stores(db);
        put_code(&s, EVM_EMPTY_CODE_HASH);
        for epoch in 1..8 {
            s.run(epoch, DEFAULT_QUARANTINE_EPOCHS).unwrap();
        }
        assert!(s.code.get(EVM_EMPTY_CODE_HASH).unwrap().is_some());
    }
}
