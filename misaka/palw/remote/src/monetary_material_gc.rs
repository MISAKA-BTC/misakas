//! **Provider-side GC janitor for retained monetary material** (RFC-0008 covenant, 2026-10-15 round 3; unverified source —
//! nothing here has been compiled or run).
//!
//! The covenant's deletion rule is unchanged: a retained bundle leaves the store only through
//! [`DurableMonetaryMaterialProviderV1::gc_finalized`], whose witness must say that the accepted native release of exactly
//! `(root, consent, tail)` was carried by `release_block` and that both the release block and the tail are finalized on the
//! current selected chain. This module only FINDS the candidate `(consent, tail, release_block)` and asks that witness:
//!
//! * While the row is present in a coherent covenant read, the janitor records the `(consent, tail_block)` pairs it saw and the
//!   selected-chain point it saw them at. A row it never saw present is pinned: absence alone never authorizes deletion.
//! * Once absent, it walks the selected-chain blocks after the last point the row was present, a bounded number of witness
//!   calls per tick, asking the native witness for each `(block, consent, tail)` candidate. A `false` (not the release, or
//!   not final yet) retains; the walk restarts next round until finality arrives.
//! * Every failed or unknown read pins (MAX retention) and deletes nothing this tick.
//!
//! **Crash safety.** Before deleting, the janitor writes an intent marker (write, fsync, rename, directory fsync) naming the
//! exact witness tuple. The store's own publication is atomic, so a crash leaves either the old snapshot (the marker replays
//! the same witness-checked deletion) or the new one (the replay finds nothing to delete). The marker is removed only after
//! the contract is observed gone from the reopened in-memory store; the journal is rewritten the same atomic way.
use crate::monetary_material::DurableMonetaryMaterialProviderV1;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{BlockHash, Hash64, palw_monetary_material_v1::PalwMonetaryMaterialRowV1};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

/// Distinct `(consent, tail block)` observations kept per contract (newest last).
pub const MONETARY_GC_MAX_OBSERVATIONS_V1: usize = 8;
/// Selected-chain blocks after the last present point considered per contract.
pub const MONETARY_GC_SCAN_LIMIT_V1: usize = 100_000;
/// Native witness calls per tick, across all contracts (each one is a coherent consensus read).
pub const MONETARY_GC_WITNESS_BUDGET_V1: usize = 256;
const JOURNAL_VERSION_V1: u16 = 1;

/// **Where the janitor's facts come from.** The local implementation is the node's Consensus API
/// (`palw_monetary_material_covenants_v1`, `get_virtual_chain_from_block`, `palw_monetary_material_release_finalized_v1`);
/// a remote provider uses [`RemoteProofGcOracleV1`]. Every method returns `Err` rather than a guess.
pub trait MonetaryMaterialGcOracleV1 {
    /// One coherent read: the selected-chain point it stands at and every accepted covenant row there.
    fn covenant_rows(&mut self) -> Result<(BlockHash, Vec<PalwMonetaryMaterialRowV1>), String>;
    /// Selected-chain blocks after `low` toward the read point, oldest first, at most `limit`.
    fn chain_after(&mut self, low: BlockHash, limit: usize) -> Result<Vec<BlockHash>, String>;
    /// The physical-GC witness (`ConsensusApi::palw_monetary_material_release_finalized_v1` semantics).
    fn release_finalized(&mut self, root: Hash64, consent: Hash64, tail: Hash64, release_block: BlockHash) -> Result<bool, String>;
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MonetaryGcWatchV1 {
    pub stored_consent: Hash64,
    pub root: Hash64,
    /// `(row consent, row tail block)` seen while present, newest last.
    pub observed: Vec<(Hash64, Hash64)>,
    pub last_present: Option<BlockHash>,
    /// Next index into `chain_after(last_present)` to ask about.
    pub scan_cursor: u64,
    /// Why this contract is held at MAX right now (informational; never authorizes anything).
    pub pinned: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct MonetaryGcJournalV1 {
    version: u16,
    watches: Vec<MonetaryGcWatchV1>,
}

/// The write-ahead deletion marker: exactly the witness tuple the deletion was checked under.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MonetaryGcIntentV1 {
    pub version: u16,
    pub stored_consent: Hash64,
    pub root: Hash64,
    pub release_consent: Hash64,
    pub tail: Hash64,
    pub release_block: BlockHash,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MonetaryGcTickReportV1 {
    /// Stored consents deleted this tick (each under a true native witness).
    pub deleted: Vec<Hash64>,
    /// Contracts still retained after this tick.
    pub retained: usize,
    /// `(stored consent, reason)` held at MAX.
    pub pinned: Vec<(Hash64, String)>,
    /// The whole tick was refused (a failed read); nothing was deleted.
    pub refused: Option<String>,
}

pub struct MonetaryMaterialGcJanitorV1 {
    journal: PathBuf,
    intent: PathBuf,
    watches: BTreeMap<Hash64, MonetaryGcWatchV1>,
    scan_limit: usize,
    witness_budget: usize,
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("janitor file has no parent directory")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let name = path.file_name().and_then(|n| n.to_str()).ok_or("janitor file name is not UTF-8")?;
    let temporary = parent.join(format!("{name}.partial-{}", std::process::id()));
    let result = (|| {
        let mut out = OpenOptions::new().write(true).create(true).truncate(true).open(&temporary)?;
        out.write_all(bytes)?;
        out.sync_all()?;
        fs::rename(&temporary, path)?;
        File::open(parent)?.sync_all()
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(error.to_string());
    }
    Ok(())
}

fn remove_durably(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    }
    let parent = path.parent().ok_or("janitor file has no parent directory")?;
    File::open(parent).and_then(|d| d.sync_all()).map_err(|e| e.to_string())
}

impl MonetaryMaterialGcJanitorV1 {
    /// Opens the journal beside `store_file` (`<store>.gc-journal`, `<store>.gc-intent`). A corrupt journal refuses: the
    /// operator inspects it; the janitor never starts over from an empty view of what it observed.
    pub fn open(store_file: &Path) -> Result<Self, String> {
        let name = store_file.file_name().and_then(|n| n.to_str()).ok_or("provider store file name is not UTF-8")?;
        let parent = store_file.parent().ok_or("provider store file has no parent directory")?;
        let journal = parent.join(format!("{name}.gc-journal"));
        let intent = parent.join(format!("{name}.gc-intent"));
        let watches = match fs::read(&journal) {
            Ok(bytes) => {
                let decoded = MonetaryGcJournalV1::try_from_slice(&bytes).map_err(|e| format!("provider GC journal: {e}"))?;
                if decoded.version != JOURNAL_VERSION_V1 {
                    return Err("provider GC journal has an unknown version".into());
                }
                decoded.watches.into_iter().map(|w| (w.stored_consent, w)).collect()
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(error) => return Err(error.to_string()),
        };
        Ok(Self { journal, intent, watches, scan_limit: MONETARY_GC_SCAN_LIMIT_V1, witness_budget: MONETARY_GC_WITNESS_BUDGET_V1 })
    }

    pub fn with_limits(mut self, scan_limit: usize, witness_budget: usize) -> Self {
        self.scan_limit = scan_limit.max(1);
        self.witness_budget = witness_budget.max(1);
        self
    }

    pub fn watches(&self) -> impl Iterator<Item = &MonetaryGcWatchV1> {
        self.watches.values()
    }

    fn persist(&self) -> Result<(), String> {
        let journal = MonetaryGcJournalV1 { version: JOURNAL_VERSION_V1, watches: self.watches.values().cloned().collect() };
        write_atomic(&self.journal, &borsh::to_vec(&journal).map_err(|e| e.to_string())?)
    }

    /// Delete under the write-ahead marker. `Ok(true)`: the contract is gone from the store (now or by an earlier run).
    fn delete(
        &self,
        store: &mut DurableMonetaryMaterialProviderV1,
        oracle: &mut impl MonetaryMaterialGcOracleV1,
        intent: &MonetaryGcIntentV1,
    ) -> Result<bool, String> {
        write_atomic(&self.intent, &borsh::to_vec(intent).map_err(|e| e.to_string())?)?;
        if store.held_contracts().iter().any(|(consent, _)| *consent == intent.stored_consent) {
            store.gc_finalized(
                intent.root,
                intent.stored_consent,
                intent.release_consent,
                intent.tail,
                intent.release_block,
                |root, consent, tail, block| oracle.release_finalized(root, consent, tail, block),
            )?;
        }
        let gone = !store.held_contracts().iter().any(|(consent, _)| *consent == intent.stored_consent);
        if gone {
            remove_durably(&self.intent)?;
        }
        Ok(gone)
    }

    /// One janitor round. Never deletes on a failed read; see the module comment.
    pub fn tick(
        &mut self,
        store: &mut DurableMonetaryMaterialProviderV1,
        oracle: &mut impl MonetaryMaterialGcOracleV1,
    ) -> Result<MonetaryGcTickReportV1, String> {
        let mut report = MonetaryGcTickReportV1::default();
        // 1. Replay an interrupted deletion first, under the same witness.
        match fs::read(&self.intent) {
            Ok(bytes) => {
                let intent = MonetaryGcIntentV1::try_from_slice(&bytes).map_err(|e| format!("provider GC intent marker: {e}"))?;
                match self.delete(store, oracle, &intent) {
                    Ok(true) => {
                        self.watches.remove(&intent.stored_consent);
                        report.deleted.push(intent.stored_consent);
                    }
                    Ok(false) => {
                        report.refused = Some("an interrupted deletion's witness no longer holds; retained and pinned".into());
                        report.retained = store.held_contracts().len();
                        return Ok(report);
                    }
                    Err(error) => {
                        report.refused = Some(format!("an interrupted deletion could not be replayed: {error}"));
                        report.retained = store.held_contracts().len();
                        return Ok(report);
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        // 2. One coherent covenant read; a failure pins everything for this tick.
        let held = store.held_contracts();
        self.watches.retain(|consent, _| held.iter().any(|(c, _)| c == consent));
        for (consent, root) in &held {
            self.watches.entry(*consent).or_insert_with(|| MonetaryGcWatchV1 {
                stored_consent: *consent,
                root: *root,
                observed: Vec::new(),
                last_present: None,
                scan_cursor: 0,
                pinned: None,
            });
        }
        let (point, rows) = match oracle.covenant_rows() {
            Ok(read) => read,
            Err(error) => {
                report.refused = Some(format!("covenant read failed; every contract retained: {error}"));
                report.retained = held.len();
                self.persist()?;
                return Ok(report);
            }
        };
        let mut budget = self.witness_budget;
        let consents: Vec<Hash64> = self.watches.keys().copied().collect();
        for consent in consents {
            let watch = self.watches.get_mut(&consent).expect("listed");
            if let Some(row) = rows.iter().find(|row| row.terms.root == watch.root) {
                if let Some(tail) = row.tail_block {
                    let seen = (row.consent, tail);
                    watch.observed.retain(|o| *o != seen);
                    watch.observed.push(seen);
                    if watch.observed.len() > MONETARY_GC_MAX_OBSERVATIONS_V1 {
                        watch.observed.remove(0);
                    }
                }
                watch.last_present = Some(point);
                watch.scan_cursor = 0;
                watch.pinned = None;
                continue;
            }
            let Some(low) = watch.last_present else {
                watch.pinned = Some("never observed accepted; absence cannot authorize deletion".into());
                continue;
            };
            if watch.observed.is_empty() {
                watch.pinned = Some("released without an observed payment tail; no witness tuple to ask".into());
                continue;
            }
            if budget == 0 {
                continue;
            }
            let blocks = match oracle.chain_after(low, self.scan_limit) {
                Ok(blocks) => blocks,
                Err(error) => {
                    watch.pinned = Some(format!("selected-chain read failed: {error}"));
                    continue;
                }
            };
            let mut found = None;
            let mut cursor = watch.scan_cursor as usize;
            'scan: while cursor < blocks.len() && budget > 0 {
                for (release_consent, tail) in watch.observed.iter().rev() {
                    if budget == 0 {
                        break 'scan;
                    }
                    budget -= 1;
                    match oracle.release_finalized(watch.root, *release_consent, *tail, blocks[cursor]) {
                        Ok(true) => {
                            found = Some((*release_consent, *tail, blocks[cursor]));
                            break 'scan;
                        }
                        Ok(false) => {}
                        Err(error) => {
                            watch.pinned = Some(format!("native GC witness failed: {error}"));
                            break 'scan;
                        }
                    }
                }
                cursor += 1;
            }
            watch.scan_cursor = if cursor >= blocks.len() { 0 } else { cursor as u64 };
            let Some((release_consent, tail, release_block)) = found else { continue };
            let intent = MonetaryGcIntentV1 {
                version: JOURNAL_VERSION_V1,
                stored_consent: consent,
                root: watch.root,
                release_consent,
                tail,
                release_block,
            };
            match self.delete(store, oracle, &intent) {
                Ok(true) => {
                    self.watches.remove(&consent);
                    report.deleted.push(consent);
                }
                Ok(false) => {
                    if let Some(watch) = self.watches.get_mut(&consent) {
                        watch.pinned = Some("the witness did not hold at deletion; retained".into());
                    }
                }
                Err(error) => {
                    if let Some(watch) = self.watches.get_mut(&consent) {
                        watch.pinned = Some(format!("deletion failed; retained: {error}"));
                    }
                }
            }
        }
        self.persist()?;
        report.retained = store.held_contracts().len();
        report.pinned = self.watches.values().filter_map(|w| w.pinned.clone().map(|why| (w.stored_consent, why))).collect();
        Ok(report)
    }
}

/// **A remote provider's facts**: the finality authority the operator configured, and covenant rows authenticated at a
/// pinned block by op 242 (`MonetaryMaterialProofPagerV1::finish`). Unknown authority: every call errs, so the janitor pins.
pub trait RemoteFinalityAuthorityV1 {
    /// A block the authority asserts is finalized on the selected chain.
    fn finalized_pin(&mut self) -> Result<BlockHash, String>;
    fn selected_parent(&mut self, block: BlockHash) -> Result<BlockHash, String>;
    fn is_chain_ancestor_of(&mut self, low: BlockHash, high: BlockHash) -> Result<bool, String>;
    fn chain_after(&mut self, low: BlockHash, limit: usize) -> Result<Vec<BlockHash>, String>;
}

/// The remote oracle. Its witness is WEAKER than the local delta witness and says so: op 242 proves which rows a block's
/// committed state holds, not which delta removed one. It requires the exact `(root, consent, tail)` row in the
/// authenticated state of `release_block`'s selected parent, its absence in the state of `release_block` and of the
/// finalized pin, and (per the authority) `tail ≼ release_block ≼ pin` on the selected chain. Its soundness reduces to the
/// configured finality authority; with none, nothing is deleted.
pub struct RemoteProofGcOracleV1<A, F> {
    pub authority: A,
    /// Rows authenticated at the pinned block (all op-242 pages, then `finish`). An `Err` pins.
    pub proven_rows: F,
}

impl<A: RemoteFinalityAuthorityV1, F: FnMut(BlockHash) -> Result<Vec<PalwMonetaryMaterialRowV1>, String>> MonetaryMaterialGcOracleV1
    for RemoteProofGcOracleV1<A, F>
{
    fn covenant_rows(&mut self) -> Result<(BlockHash, Vec<PalwMonetaryMaterialRowV1>), String> {
        let pin = self.authority.finalized_pin()?;
        Ok((pin, (self.proven_rows)(pin)?))
    }
    fn chain_after(&mut self, low: BlockHash, limit: usize) -> Result<Vec<BlockHash>, String> {
        self.authority.chain_after(low, limit)
    }
    fn release_finalized(&mut self, root: Hash64, consent: Hash64, tail: Hash64, release_block: BlockHash) -> Result<bool, String> {
        let pin = self.authority.finalized_pin()?;
        if !self.authority.is_chain_ancestor_of(release_block, pin)? || !self.authority.is_chain_ancestor_of(tail, release_block)? {
            return Ok(false);
        }
        let parent = self.authority.selected_parent(release_block)?;
        let before = (self.proven_rows)(parent)?;
        if !before.iter().any(|row| row.terms.root == root && row.consent == consent && row.tail_block == Some(tail)) {
            return Ok(false);
        }
        if (self.proven_rows)(release_block)?.iter().any(|row| row.terms.root == root) {
            return Ok(false);
        }
        Ok(!(self.proven_rows)(pin)?.iter().any(|row| row.terms.root == root))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        rows: Result<Vec<PalwMonetaryMaterialRowV1>, String>,
        chain: Vec<BlockHash>,
        finalized: Option<(Hash64, Hash64, Hash64, BlockHash)>,
        calls: usize,
    }
    impl MonetaryMaterialGcOracleV1 for Fake {
        fn covenant_rows(&mut self) -> Result<(BlockHash, Vec<PalwMonetaryMaterialRowV1>), String> {
            Ok((Hash64::from_u64_word(500), self.rows.clone()?))
        }
        fn chain_after(&mut self, _low: BlockHash, limit: usize) -> Result<Vec<BlockHash>, String> {
            Ok(self.chain.iter().copied().take(limit).collect())
        }
        fn release_finalized(&mut self, root: Hash64, consent: Hash64, tail: Hash64, block: BlockHash) -> Result<bool, String> {
            self.calls += 1;
            Ok(self.finalized == Some((root, consent, tail, block)))
        }
    }

    #[test]
    fn an_unobserved_or_unreadable_contract_is_retained_and_the_journal_survives_reopen() {
        let dir = std::env::temp_dir().join(format!("misaka-gc-janitor-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let store_file = dir.join("p.store");
        let mut store = DurableMonetaryMaterialProviderV1::open(&store_file, 1 << 20, 1 << 24, None, |_| None).unwrap();
        let mut janitor = MonetaryMaterialGcJanitorV1::open(&store_file).unwrap();
        let mut oracle = Fake { rows: Err("import incomplete".into()), chain: vec![], finalized: None, calls: 0 };
        let report = janitor.tick(&mut store, &mut oracle).unwrap();
        assert!(report.refused.is_some() && report.deleted.is_empty());
        oracle.rows = Ok(vec![]);
        let report = janitor.tick(&mut store, &mut oracle).unwrap();
        assert!(report.deleted.is_empty() && report.refused.is_none());
        assert_eq!(oracle.calls, 0, "an empty store asks no witness");
        assert!(MonetaryMaterialGcJanitorV1::open(&store_file).is_ok(), "the journal reopens");
        fs::write(dir.join("p.store.gc-journal"), b"corrupt").unwrap();
        assert!(MonetaryMaterialGcJanitorV1::open(&store_file).is_err(), "a corrupt journal refuses instead of forgetting");
        let _ = fs::remove_dir_all(&dir);
    }
}
