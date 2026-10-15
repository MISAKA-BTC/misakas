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
    io::{Read, Write},
    path::{Path, PathBuf},
};

/// Distinct `(consent, tail block)` observations kept per contract (newest last).
pub const MONETARY_GC_MAX_OBSERVATIONS_V1: usize = 8;
/// Selected-chain blocks after the last present point considered per contract.
pub const MONETARY_GC_SCAN_LIMIT_V1: usize = 100_000;
/// Discovery witness calls per tick. Each positive discovery adds at most one
/// final revalidation at atomic deletion, so total calls are bounded by twice this value.
pub const MONETARY_GC_WITNESS_BUDGET_V1: usize = 256;
const JOURNAL_VERSION_V1: u16 = 1;
const JOURNAL_VERSION_V2: u16 = 2;
const MONETARY_GC_MAX_JOURNAL_BYTES_V2: u64 = 16 << 20;
const MONETARY_GC_MAX_WATCHES_V2: usize = 4_096;

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
    /// V2 authority is receipt PRESENCE in current native-finalized state,
    /// including the complete signed generation history. Legacy implementations
    /// return no authority, preserving their original witness semantics.
    fn terminal_receipt(
        &mut self,
        _root: Hash64,
        _stored_consent: Hash64,
    ) -> Result<Option<(BlockHash, kaspa_consensus_core::palw_material_release_recovery_v2::PalwMaterialTerminalReceiptV2)>, String>
    {
        Ok(None)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MonetaryGcWatchV1 {
    pub stored_consent: Hash64,
    pub root: Hash64,
    /// `(row consent, row tail block)` seen while present, newest last.
    pub observed: Vec<(Hash64, Hash64)>,
    pub last_present: Option<BlockHash>,
    /// Next flattened (block, observed tuple) candidate in the current scan page.
    /// V1 journals stored a block index; opening them resets the cursor safely.
    pub scan_cursor: u64,
    /// Why this contract is held at MAX right now (informational; never authorizes anything).
    pub pinned: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct MonetaryGcJournalV1 {
    version: u16,
    watches: Vec<MonetaryGcWatchV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct MonetaryGcJournalV2 {
    version: u16,
    watches: Vec<MonetaryGcWatchV1>,
    /// Pages begin after these selected-chain blocks; last_present remains the
    /// stable rescan origin so finality arriving later is always reconsidered.
    scan_anchors: BTreeMap<Hash64, BlockHash>,
    next_contract: Option<Hash64>,
    completed: BTreeMap<Hash64, MonetaryGcCompletedV2>,
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

/// Durable metadata-cleanup receipt. Its intent was native-witness checked
/// before atomically deleting exactly this consent. An application acknowledges
/// it only after its authority-index cleanup is fsynced. Bounded like live rows.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MonetaryGcCompletedV2 {
    pub version: u16,
    pub intent: MonetaryGcIntentV1,
    pub native_claims: Vec<Hash64>,
}
const MONETARY_GC_MAX_INTENT_BYTES_V2: u64 = 2 + 322 + 4 + 64 * 64;

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
    scan_anchors: BTreeMap<Hash64, BlockHash>,
    next_contract: Option<Hash64>,
    completed: BTreeMap<Hash64, MonetaryGcCompletedV2>,
}

fn read_capped(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let source = File::open(path).map_err(|e| e.to_string())?;
    if source.metadata().map_err(|e| e.to_string())?.len() > limit {
        return Err("provider GC file exceeds its recovery cap".into());
    }
    let mut bytes = Vec::new();
    source.take(limit + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("provider GC file grew beyond its recovery cap".into());
    }
    Ok(bytes)
}
/// Bounds every nested allocation before Borsh sees local persisted bytes.
fn preflight_journal(bytes: &[u8]) -> Result<u16, String> {
    struct Cursor<'a>(&'a [u8]);
    impl<'a> Cursor<'a> {
        fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
            let head = self.0.get(..n).ok_or("provider GC journal truncated")?;
            self.0 = &self.0[n..];
            Ok(head)
        }
        fn count(&mut self, cap: usize) -> Result<usize, String> {
            let n = u32::from_le_bytes(self.take(4)?.try_into().unwrap()) as usize;
            if n > cap {
                return Err("provider GC journal nested count exceeds cap".into());
            }
            Ok(n)
        }
        fn optional(&mut self, n: usize) -> Result<(), String> {
            match self.take(1)?[0] {
                0 => Ok(()),
                1 => {
                    self.take(n)?;
                    Ok(())
                }
                _ => Err("provider GC journal option tag differs".into()),
            }
        }
    }
    let mut c = Cursor(bytes);
    let version = u16::from_le_bytes(c.take(2)?.try_into().unwrap());
    if version != JOURNAL_VERSION_V1 && version != JOURNAL_VERSION_V2 {
        return Err("provider GC journal version differs".into());
    }
    let rows = c.count(MONETARY_GC_MAX_WATCHES_V2)?;
    for _ in 0..rows {
        c.take(128)?;
        let observed = c.count(MONETARY_GC_MAX_OBSERVATIONS_V1)?;
        c.take(observed * 128)?;
        c.optional(64)?;
        c.take(8)?;
        match c.take(1)?[0] {
            0 => {}
            1 => {
                let n = c.count(4096)?;
                c.take(n)?;
            }
            _ => return Err("provider GC journal pinned option differs".into()),
        }
    }
    if version == JOURNAL_VERSION_V2 {
        let anchors = c.count(rows)?;
        c.take(anchors * 128)?;
        c.optional(64)?;
        let completed = c.count(MONETARY_GC_MAX_WATCHES_V2)?;
        for _ in 0..completed {
            c.take(64 + 2 + 322)?;
            let claims = c.count(64)?;
            c.take(claims * 64)?;
        }
    }
    if !c.0.is_empty() {
        return Err("provider GC journal trailing bytes".into());
    }
    Ok(version)
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
        let (watches, scan_anchors, next_contract, completed) = match fs::metadata(&journal) {
            Ok(meta) => {
                if meta.len() > MONETARY_GC_MAX_JOURNAL_BYTES_V2 {
                    return Err("provider GC journal exceeds its recovery cap".into());
                }
                let bytes = read_capped(&journal, MONETARY_GC_MAX_JOURNAL_BYTES_V2)?;
                preflight_journal(&bytes)?;
                let version = u16::from_le_bytes(bytes.get(..2).ok_or("provider GC journal truncated")?.try_into().unwrap());
                let (rows, anchors, next, completed) = match version {
                    JOURNAL_VERSION_V1 => {
                        let old =
                            MonetaryGcJournalV1::try_from_slice(&bytes).map_err(|e| format!("provider GC legacy journal: {e}"))?;
                        (
                            old.watches
                                .into_iter()
                                .map(|mut w| {
                                    w.scan_cursor = 0;
                                    w
                                })
                                .collect::<Vec<_>>(),
                            BTreeMap::new(),
                            None,
                            BTreeMap::new(),
                        )
                    }
                    JOURNAL_VERSION_V2 => {
                        let row = MonetaryGcJournalV2::try_from_slice(&bytes).map_err(|e| format!("provider GC journal: {e}"))?;
                        (row.watches, row.scan_anchors, row.next_contract, row.completed)
                    }
                    _ => return Err("provider GC journal has an unknown version".into()),
                };
                if rows.len() > MONETARY_GC_MAX_WATCHES_V2
                    || anchors.len() > rows.len()
                    || rows.iter().any(|w| {
                        w.observed.len() > MONETARY_GC_MAX_OBSERVATIONS_V1
                            || w.root == Hash64::default()
                            || w.stored_consent == Hash64::default()
                    })
                {
                    return Err("provider GC journal exceeds its contract/observation bounds".into());
                }
                let count = rows.len();
                let watches: BTreeMap<_, _> = rows.into_iter().map(|w| (w.stored_consent, w)).collect();
                if watches.len() != count || anchors.keys().any(|key| !watches.contains_key(key)) {
                    return Err("provider GC journal duplicates or substitutes a stored consent".into());
                }
                if completed.len() > MONETARY_GC_MAX_WATCHES_V2
                    || completed.iter().any(|(id, r)| {
                        *id != r.intent.stored_consent
                            || r.version != 2
                            || r.intent.version != 1
                            || r.native_claims.len() > 64
                            || r.native_claims.windows(2).any(|v| v[0] >= v[1])
                    })
                {
                    return Err("provider GC completed receipt substituted or oversized".into());
                }
                (watches, anchors, next, completed)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (BTreeMap::new(), BTreeMap::new(), None, BTreeMap::new()),
            Err(error) => return Err(error.to_string()),
        };
        Ok(Self {
            journal,
            intent,
            watches,
            scan_limit: MONETARY_GC_SCAN_LIMIT_V1,
            witness_budget: MONETARY_GC_WITNESS_BUDGET_V1,
            scan_anchors,
            next_contract,
            completed,
        })
    }

    pub fn with_limits(mut self, scan_limit: usize, witness_budget: usize) -> Self {
        self.scan_limit = scan_limit.max(1);
        self.witness_budget = witness_budget.max(1);
        self
    }

    pub fn watches(&self) -> impl Iterator<Item = &MonetaryGcWatchV1> {
        self.watches.values()
    }

    pub fn completed_receipts(&self) -> impl Iterator<Item = &MonetaryGcCompletedV2> {
        self.completed.values()
    }
    pub fn acknowledge_metadata_cleanup(&mut self, consent: Hash64) -> Result<(), String> {
        let old = self.completed.remove(&consent);
        if old.is_none() {
            return Ok(());
        }
        if let Err(error) = self.persist() {
            self.completed.insert(consent, old.unwrap());
            return Err(error);
        }
        Ok(())
    }

    fn persist(&self) -> Result<(), String> {
        let journal = MonetaryGcJournalV2 {
            version: JOURNAL_VERSION_V2,
            watches: self.watches.values().cloned().collect(),
            scan_anchors: self.scan_anchors.clone(),
            next_contract: self.next_contract,
            completed: self.completed.clone(),
        };
        let bytes = borsh::to_vec(&journal).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MONETARY_GC_MAX_JOURNAL_BYTES_V2 {
            return Err("provider GC journal publication exceeds cap".into());
        }
        write_atomic(&self.journal, &bytes)
    }

    /// Delete under the write-ahead marker. `Ok(true)`: the contract is gone from the store (now or by an earlier run).
    fn delete(
        &mut self,
        store: &mut DurableMonetaryMaterialProviderV1,
        oracle: &mut impl MonetaryMaterialGcOracleV1,
        intent: &MonetaryGcIntentV1,
        native_claims: Vec<Hash64>,
    ) -> Result<bool, String> {
        if intent.version != JOURNAL_VERSION_V1 {
            return Err("provider GC intent version differs".into());
        }
        if native_claims.len() > 64 || native_claims.windows(2).any(|v| v[0] >= v[1]) {
            return Err("provider GC intent native claims differ".into());
        }
        if self.completed.len() >= MONETARY_GC_MAX_WATCHES_V2 && !self.completed.contains_key(&intent.stored_consent) {
            return Err("provider GC metadata receipt capacity requires acknowledgement".into());
        }
        let receipt = MonetaryGcCompletedV2 { version: 2, intent: intent.clone(), native_claims };
        write_atomic(&self.intent, &borsh::to_vec(&receipt).map_err(|e| e.to_string())?)?;
        if store.held_contracts().iter().any(|(consent, _)| *consent == intent.stored_consent) {
            store.gc_finalized(
                intent.root,
                intent.stored_consent,
                intent.release_consent,
                intent.tail,
                intent.release_block,
                |root, consent, tail, block| oracle.release_finalized(root, consent, tail, block),
            )?;
        } else if !oracle.release_finalized(intent.root, intent.release_consent, intent.tail, intent.release_block)? {
            return Ok(false);
        }
        let gone = !store.held_contracts().iter().any(|(consent, _)| *consent == intent.stored_consent);
        if gone {
            self.completed.insert(intent.stored_consent, receipt);
            self.persist()?;
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
        match fs::metadata(&self.intent) {
            Ok(_) => {
                let bytes = read_capped(&self.intent, MONETARY_GC_MAX_INTENT_BYTES_V2)?;
                let version = u16::from_le_bytes(bytes.get(..2).ok_or("provider GC intent truncated")?.try_into().unwrap());
                let (intent, claims) = match version {
                    1 => {
                        let intent =
                            MonetaryGcIntentV1::try_from_slice(&bytes).map_err(|e| format!("provider GC legacy intent: {e}"))?;
                        let claims = store
                            .held_native_authorities()
                            .get(&intent.stored_consent)
                            .map(|s| s.iter().copied().collect())
                            .unwrap_or_default();
                        (intent, claims)
                    }
                    2 => {
                        if bytes.len() < 328 {
                            return Err("provider GC intent truncated".into());
                        }
                        let count = u32::from_le_bytes(bytes[324..328].try_into().unwrap()) as usize;
                        if count > 64 || bytes.len() != 328 + 64 * count {
                            return Err("provider GC intent claim extent differs".into());
                        }
                        let receipt = MonetaryGcCompletedV2::try_from_slice(&bytes).map_err(|e| e.to_string())?;
                        (receipt.intent, receipt.native_claims)
                    }
                    _ => return Err("provider GC intent unknown version".into()),
                };
                match self.delete(store, oracle, &intent, claims) {
                    Ok(true) => {
                        self.watches.remove(&intent.stored_consent);
                        report.deleted.push(intent.stored_consent);
                    }
                    Ok(false) => {
                        // The store still contains every byte. A reorg can revoke
                        // this candidate; clear the unexecuted intent and resume
                        // discovery rather than pinning forever to a stale tuple.
                        remove_durably(&self.intent)?;
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
        self.scan_anchors.retain(|consent, _| self.watches.contains_key(consent));
        if held.len() > MONETARY_GC_MAX_WATCHES_V2 {
            return Err("provider GC retained contract count exceeds journal capacity".into());
        }
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
        let mut recovery_budget = self.witness_budget;
        let mut consents: Vec<Hash64> = self.watches.keys().copied().collect();
        if let Some(next) = self.next_contract {
            let start = consents.partition_point(|key| *key < next);
            let count = consents.len();
            if count > 0 {
                consents.rotate_left(start % count)
            }
        }
        for consent in consents {
            // Rotate after every inspected contract. A large/unfinalized first
            // contract may exhaust the budget but cannot starve later ones.
            if budget == 0 {
                self.next_contract = Some(consent);
                break;
            }
            self.next_contract = self
                .watches
                .range((std::ops::Bound::Excluded(consent), std::ops::Bound::Unbounded))
                .next()
                .map(|(id, _)| *id)
                .or_else(|| self.watches.keys().next().copied());
            let root = self.watches[&consent].root;
            let recovery = if recovery_budget > 0 {
                recovery_budget -= 1;
                oracle.terminal_receipt(root, consent)
            } else {
                Ok(None)
            };
            match recovery {
                Ok(Some((pin, receipt))) if receipt.covers(root, consent) => {
                    let intent = MonetaryGcIntentV1 {
                        version: 1,
                        stored_consent: consent,
                        root,
                        release_consent: consent,
                        tail: receipt.tail_block,
                        release_block: pin,
                    };
                    let claims =
                        store.held_native_authorities().get(&consent).map(|s| s.iter().copied().collect()).unwrap_or_default();
                    match self.delete(store, oracle, &intent, claims) {
                        Ok(true) => {
                            self.watches.remove(&consent);
                            self.scan_anchors.remove(&consent);
                            report.deleted.push(consent);
                        }
                        Ok(false) => {
                            self.watches.get_mut(&consent).unwrap().pinned = Some("terminal receipt changed before deletion".into());
                        }
                        Err(e) => {
                            self.watches.get_mut(&consent).unwrap().pinned = Some(format!("terminal receipt deletion failed: {e}"));
                        }
                    }
                    continue;
                }
                Ok(Some(_)) => {
                    self.watches.get_mut(&consent).unwrap().pinned = Some("terminal receipt substituted stored generation".into());
                    continue;
                }
                Err(e) => {
                    self.watches.get_mut(&consent).unwrap().pinned = Some(format!("terminal receipt read failed: {e}"));
                    continue;
                }
                Ok(None) => {}
            }
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
                self.scan_anchors.remove(&consent);
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
            let anchor = self.scan_anchors.get(&consent).copied().unwrap_or(low);
            let blocks = match oracle.chain_after(anchor, self.scan_limit) {
                Ok(blocks) => blocks,
                Err(error) => {
                    watch.pinned = Some(format!("selected-chain read failed: {error}"));
                    // An advanced page anchor may have left the selected chain.
                    // Restart from the last authenticated present point.
                    self.scan_anchors.remove(&consent);
                    watch.scan_cursor = 0;
                    continue;
                }
            };
            let observations = watch.observed.len();
            let candidates = blocks.len().checked_mul(observations).ok_or("provider GC scan candidate count overflow")?;
            let mut found = None;
            let mut cursor = usize::try_from(watch.scan_cursor).unwrap_or(usize::MAX);
            if cursor > candidates {
                cursor = 0;
            }
            while cursor < candidates && budget > 0 {
                let block = blocks[cursor / observations];
                let (release_consent, tail) = watch.observed[observations - 1 - cursor % observations];
                budget -= 1;
                match oracle.release_finalized(watch.root, release_consent, tail, block) {
                    Ok(true) => {
                        found = Some((release_consent, tail, block));
                        break;
                    }
                    Ok(false) => {
                        cursor += 1;
                        watch.pinned = None;
                    }
                    Err(error) => {
                        watch.pinned = Some(format!("native GC witness failed: {error}"));
                        break;
                    }
                }
            }
            if cursor >= candidates {
                watch.scan_cursor = 0;
                if blocks.len() == self.scan_limit && !blocks.is_empty() {
                    self.scan_anchors.insert(consent, *blocks.last().unwrap());
                } else {
                    self.scan_anchors.remove(&consent);
                }
            } else {
                watch.scan_cursor = cursor as u64;
            }
            let Some((release_consent, tail, release_block)) = found else { continue };
            let intent = MonetaryGcIntentV1 {
                version: JOURNAL_VERSION_V1,
                stored_consent: consent,
                root: watch.root,
                release_consent,
                tail,
                release_block,
            };
            let claims = store.held_native_authorities().get(&consent).map(|s| s.iter().copied().collect()).unwrap_or_default();
            match self.delete(store, oracle, &intent, claims) {
                Ok(true) => {
                    self.watches.remove(&consent);
                    self.scan_anchors.remove(&consent);
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
/// parent-state committed by `release_block`, its absence in the parent-state committed by the selected-chain CHILD
/// of `release_block` and by the finalized pin, and (per the authority) `tail ≼ release_block ≼ pin` on the selected chain. Its soundness reduces to the
/// configured finality authority; with none, nothing is deleted.
pub struct RemoteProofGcOracleV1<A, F> {
    pub authority: A,
    /// Parent-state rows committed by the pinned header (all op-242 pages,
    /// then `finish`). This is BEFORE the pinned block's own transition. An Err pins.
    pub proven_rows: F,
}

impl<A: RemoteFinalityAuthorityV1, F: FnMut(BlockHash) -> Result<Vec<PalwMonetaryMaterialRowV1>, String>> MonetaryMaterialGcOracleV1
    for RemoteProofGcOracleV1<A, F>
{
    fn covenant_rows(&mut self) -> Result<(BlockHash, Vec<PalwMonetaryMaterialRowV1>), String> {
        let pin = self.authority.finalized_pin()?;
        // op242 authenticates the state AFTER the selected parent, BEFORE pin.
        // Recording pin as last_present would skip a release performed by pin
        // itself when chain_after(last_present) discovers candidates.
        let state_point = self.authority.selected_parent(pin)?;
        Ok((state_point, (self.proven_rows)(pin)?))
    }
    fn chain_after(&mut self, low: BlockHash, limit: usize) -> Result<Vec<BlockHash>, String> {
        self.authority.chain_after(low, limit)
    }
    fn release_finalized(&mut self, root: Hash64, consent: Hash64, tail: Hash64, release_block: BlockHash) -> Result<bool, String> {
        let pin = self.authority.finalized_pin()?;
        if !self.authority.is_chain_ancestor_of(release_block, pin)? || !self.authority.is_chain_ancestor_of(tail, release_block)? {
            return Ok(false);
        }
        // Header R commits state BEFORE applying R. Its selected-chain child
        // commits state AFTER R. Reading R's parent and R here would detect a
        // removal by the parent and misattribute it to R (or miss real releases).
        let before = (self.proven_rows)(release_block)?;
        if !before.iter().any(|row| row.terms.root == root && row.consent == consent && row.tail_block == Some(tail)) {
            return Ok(false);
        }
        let Some(child) = self.authority.chain_after(release_block, 1)?.first().copied() else { return Ok(false) };
        if self.authority.selected_parent(child)? != release_block || !self.authority.is_chain_ancestor_of(child, pin)? {
            return Ok(false);
        }
        if (self.proven_rows)(child)?.iter().any(|row| row.terms.root == root) {
            return Ok(false);
        }
        Ok(!(self.proven_rows)(pin)?.iter().any(|row| row.terms.root == root))
    }
}

/// V2 remote oracle. The native-finality authority supplies a current finalized
/// pin and op242 authenticates its entire parent-state. Historical Release and
/// payment-tail headers may be long pruned; receipt presence replaces them.
pub struct RemoteProofGcOracleV2<A, F> {
    pub authority: A,
    pub proven_snapshot: F,
}
impl<A: RemoteFinalityAuthorityV1, F: FnMut(BlockHash) -> Result<crate::monetary_material_rpc::ProvenMonetarySnapshotV2, String>>
    MonetaryMaterialGcOracleV1 for RemoteProofGcOracleV2<A, F>
{
    fn covenant_rows(&mut self) -> Result<(BlockHash, Vec<PalwMonetaryMaterialRowV1>), String> {
        let pin = self.authority.finalized_pin()?;
        let parent = self.authority.selected_parent(pin)?;
        Ok((parent, (self.proven_snapshot)(pin)?.rows().to_vec()))
    }
    fn chain_after(&mut self, low: BlockHash, limit: usize) -> Result<Vec<BlockHash>, String> {
        self.authority.chain_after(low, limit)
    }
    fn terminal_receipt(
        &mut self,
        root: Hash64,
        stored: Hash64,
    ) -> Result<Option<(BlockHash, kaspa_consensus_core::palw_material_release_recovery_v2::PalwMaterialTerminalReceiptV2)>, String>
    {
        use kaspa_consensus_core::palw_material_release_recovery_v2::PalwMaterialRecoveryRowV2;
        let pin = self.authority.finalized_pin()?;
        let snapshot = (self.proven_snapshot)(pin)?;
        Ok(snapshot.recoveries().iter().find_map(|row| match row {
            PalwMaterialRecoveryRowV2::Terminal(r) if r.covers(root, stored) => Some((pin, r.clone())),
            _ => None,
        }))
    }
    fn release_finalized(&mut self, root: Hash64, consent: Hash64, tail: Hash64, _release: BlockHash) -> Result<bool, String> {
        Ok(self.terminal_receipt(root, consent)?.is_some_and(|(_, r)| r.tail_block == tail))
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
            let start = self.chain.iter().position(|b| *b == _low).map_or(0, |i| i + 1);
            Ok(self.chain.iter().skip(start).copied().take(limit).collect())
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
    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn typed_store(
        file: &Path,
    ) -> (DurableMonetaryMaterialProviderV1, PalwMonetaryMaterialRowV1, misaka_palw_kernel::ledger::KernelLedgerV1) {
        use crate::monetary_gc_kernel_fixture::ledger_world::{World, position};
        use kaspa_consensus_core::{
            palw_material_capacity_v1::{PalwKernelMaterialBundleV1, PalwMaterialManifestV1},
            palw_monetary_material_v1::*,
        };
        let mut world = World::new();
        let job = world.post_job(2, &[3, 17], 1, 0x71);
        let made = world.honest(&job, 1);
        let claim = made.claim.id();
        world.block(4, vec![made.tx]);
        assert!(world.l.claims.contains_key(&claim));
        let bundle = PalwKernelMaterialBundleV1 {
            claim: Hash64::from_bytes(claim),
            positions: (0..made.trace.values.len()).map(|p| (0, p as u32, position(&made.trace, p as u32, |_| {}))).collect(),
        };
        let scope = bundle.verify(&world.l, 1 << 24).unwrap();
        let root = bundle.claim;
        let manifest = PalwMaterialManifestV1 { root, scopes: vec![scope] };
        let provider = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(h(8), 0));
        let enrollment = PalwMonetaryMaterialEnrollmentV1 {
            terms: PalwMonetaryMaterialTermsV1 {
                version: 1,
                root,
                dependencies: vec![],
                exec_plan_root: None,
                sponsor: provider,
                nonce: h(90),
                policy: h(91),
                max_material_bytes: 1 << 24,
                material_manifest: Some(manifest),
                native_session: None,
                window_daa: 436,
                enrollment_until_daa: 100,
                payment_relative_until_finality: true,
            },
            offers: vec![PalwMonetaryMaterialOfferV1 { provider, collateral_sompi: 1, prepaid_fee_sompi: 1, signature: vec![] }],
        };
        let mut store = DurableMonetaryMaterialProviderV1::open(file, 1 << 24, 1 << 26, Some(&world.l), |_| None).unwrap();
        store.prepare_material(provider, &enrollment, &[], &[bundle], &[], Some(&world.l), |_| None).unwrap();
        let consent = enrollment.id();
        let row = PalwMonetaryMaterialRowV1 {
            terms: enrollment.terms,
            consent,
            sequence: 0,
            accepted_daa: 10,
            root_seen: true,
            native_binding: None,
            providers: vec![],
            challenges: vec![],
            last_tail_until: Some(446),
            tail_block: Some(h(20)),
            tail_blue_score: Some(20),
            last_payment: None,
            payment_history_root: Hash64::default(),
            service_fee_count: 0,
            service_fee_total_sompi: 0,
            service_fee_paid_sompi: 0,
            service_fee_history_root: Hash64::default(),
            queued_entitlements: BTreeMap::new(),
        };
        (store, row, world.l)
    }
    #[test]
    fn typed_provider_reopen_scans_every_generation_with_budget_one_and_requires_true_gc_witness() {
        // Actual typed dense/MoE kernel material, real fsync/reopen and janitor.
        // The injected finality oracle tests plumbing; it is not native-chain evidence.
        let dir = std::env::temp_dir().join(format!("misaka-gc-positive-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let file = dir.join("store");
        let (mut store, row, ledger) = typed_store(&file);
        let mut janitor = MonetaryMaterialGcJanitorV1::open(&file).unwrap().with_limits(2, 1);
        let mut oracle = Fake { rows: Ok(vec![row.clone()]), chain: vec![h(501), h(502), h(503)], finalized: None, calls: 0 };
        janitor.tick(&mut store, &mut oracle).unwrap();
        // A newer observed generation is a false tuple. With one call per tick,
        // the original tuple must still be examined rather than starved forever.
        let mut newer = row.clone();
        newer.consent = h(77);
        newer.tail_block = Some(h(78));
        oracle.rows = Ok(vec![newer]);
        janitor.tick(&mut store, &mut oracle).unwrap();
        oracle.rows = Ok(vec![]);
        oracle.finalized = Some((row.terms.root, row.consent, h(20), h(501)));
        assert!(janitor.tick(&mut store, &mut oracle).unwrap().deleted.is_empty());
        drop(janitor);
        drop(store);
        let mut store = DurableMonetaryMaterialProviderV1::open(&file, 1 << 24, 1 << 26, Some(&ledger), |_| None).unwrap();
        let mut janitor = MonetaryMaterialGcJanitorV1::open(&file).unwrap().with_limits(2, 1);
        assert_eq!(janitor.tick(&mut store, &mut oracle).unwrap().deleted, vec![row.consent]);
        assert!(store.held_contracts().is_empty());
        drop(store);
        let store = DurableMonetaryMaterialProviderV1::open(&file, 1 << 24, 1 << 26, Some(&ledger), |_| None).unwrap();
        assert!(store.held_contracts().is_empty());
        drop(store);
        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn v1_journal_migrates_cursor_without_skipping_candidates_and_preflight_refuses_nested_bombs() {
        let dir = std::env::temp_dir().join(format!("misaka-gc-migrate-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("store");
        let watch = MonetaryGcWatchV1 {
            stored_consent: h(1),
            root: h(2),
            observed: vec![(h(3), h(4))],
            last_present: Some(h(5)),
            scan_cursor: 999,
            pinned: None,
        };
        let bytes = borsh::to_vec(&MonetaryGcJournalV1 { version: 1, watches: vec![watch] }).unwrap();
        fs::write(dir.join("store.gc-journal"), &bytes).unwrap();
        let recovered = MonetaryMaterialGcJanitorV1::open(&file).unwrap();
        assert_eq!(recovered.watches().next().unwrap().scan_cursor, 0);
        recovered.persist().unwrap();
        assert_eq!(preflight_journal(&fs::read(dir.join("store.gc-journal")).unwrap()), Ok(2));
        let mut bomb = vec![2, 0];
        bomb.extend(u32::MAX.to_le_bytes());
        assert!(preflight_journal(&bomb).is_err());
        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn selected_chain_scan_advances_beyond_its_first_page_and_recovers_after_reopen() {
        let dir = std::env::temp_dir().join(format!("misaka-gc-paging-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let file = dir.join("store");
        let (mut store, row, ledger) = typed_store(&file);
        let mut janitor = MonetaryMaterialGcJanitorV1::open(&file).unwrap().with_limits(1, 1);
        let mut oracle = Fake { rows: Ok(vec![row.clone()]), chain: vec![h(501), h(502), h(503)], finalized: None, calls: 0 };
        janitor.tick(&mut store, &mut oracle).unwrap();
        oracle.rows = Ok(vec![]);
        oracle.finalized = Some((row.terms.root, row.consent, h(20), h(503)));
        assert!(janitor.tick(&mut store, &mut oracle).unwrap().deleted.is_empty());
        drop(janitor);
        drop(store);
        let mut store = DurableMonetaryMaterialProviderV1::open(&file, 1 << 24, 1 << 26, Some(&ledger), |_| None).unwrap();
        let mut janitor = MonetaryMaterialGcJanitorV1::open(&file).unwrap().with_limits(1, 1);
        assert!(janitor.tick(&mut store, &mut oracle).unwrap().deleted.is_empty());
        assert_eq!(janitor.tick(&mut store, &mut oracle).unwrap().deleted, vec![row.consent]);
        drop(store);
        let _ = fs::remove_dir_all(dir);
    }

    struct Finality {
        chain: Vec<BlockHash>,
    }
    impl RemoteFinalityAuthorityV1 for Finality {
        fn finalized_pin(&mut self) -> Result<BlockHash, String> {
            Ok(*self.chain.last().unwrap())
        }
        fn selected_parent(&mut self, b: BlockHash) -> Result<BlockHash, String> {
            let i = self.chain.iter().position(|h| *h == b).ok_or("unknown block")?;
            i.checked_sub(1).map(|i| self.chain[i]).ok_or("genesis has no parent".into())
        }
        fn is_chain_ancestor_of(&mut self, a: BlockHash, b: BlockHash) -> Result<bool, String> {
            Ok(self.chain.iter().position(|h| *h == a).zip(self.chain.iter().position(|h| *h == b)).is_some_and(|(a, b)| a <= b))
        }
        fn chain_after(&mut self, b: BlockHash, limit: usize) -> Result<Vec<BlockHash>, String> {
            let i = self.chain.iter().position(|h| *h == b).ok_or("unknown chain point")?;
            Ok(self.chain.iter().skip(i + 1).take(limit).copied().collect())
        }
    }
    #[test]
    fn remote_gc_reads_release_header_before_state_and_its_child_after_state() {
        let dir = std::env::temp_dir().join(format!("misaka-gc-proof-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let file = dir.join("store");
        let (store, row, _) = typed_store(&file);
        let asked = std::cell::RefCell::new(Vec::new());
        let mut oracle = RemoteProofGcOracleV1 {
            authority: Finality { chain: vec![h(20), h(500), h(501), h(502), h(503)] },
            proven_rows: |block| {
                asked.borrow_mut().push(block);
                Ok(if block == h(501) { vec![row.clone()] } else { vec![] })
            },
        };
        assert!(oracle.release_finalized(row.terms.root, row.consent, h(20), h(501)).unwrap());
        assert_eq!(
            *asked.borrow(),
            vec![h(501), h(502), h(503)],
            "op242 commits parent-state; the release's parent is never its before-state proof"
        );
        assert_eq!(
            oracle.covenant_rows().unwrap().0,
            h(502),
            "the authenticated state point is the finalized header's selected parent"
        );
        oracle.authority.chain = vec![h(20), h(500), h(501)];
        assert!(
            !oracle.release_finalized(row.terms.root, row.consent, h(20), h(501)).unwrap(),
            "a finalized release with no finalized child proof remains retained"
        );
        drop(store);
        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn native_metadata_cleanup_receipt_replays_after_the_store_delete_and_ack_is_durable() {
        // WAL/controller seam: native witness is injected explicitly, with no
        // claim that these identifiers authenticate model material.
        let dir = std::env::temp_dir().join(format!("misaka-gc-receipt-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let file = dir.join("store");
        let mut store = DurableMonetaryMaterialProviderV1::open(&file, 1 << 20, 1 << 24, None, |_| None).unwrap();
        let mut janitor = MonetaryMaterialGcJanitorV1::open(&file).unwrap();
        let intent = MonetaryGcIntentV1 {
            version: 1,
            stored_consent: h(10),
            root: h(11),
            release_consent: h(12),
            tail: h(13),
            release_block: h(14),
        };
        let pending = MonetaryGcCompletedV2 { version: 2, intent: intent.clone(), native_claims: vec![h(15)] };
        write_atomic(&janitor.intent, &borsh::to_vec(&pending).unwrap()).unwrap();
        let mut oracle = Fake { rows: Ok(vec![]), chain: vec![], finalized: Some((h(11), h(12), h(13), h(14))), calls: 0 };
        janitor.tick(&mut store, &mut oracle).unwrap();
        assert!(oracle.calls > 0, "store absence alone cannot authorize metadata cleanup");
        drop(janitor);
        let mut reopened = MonetaryMaterialGcJanitorV1::open(&file).unwrap();
        assert_eq!(reopened.completed_receipts().cloned().collect::<Vec<_>>(), vec![pending]);
        reopened.acknowledge_metadata_cleanup(h(10)).unwrap();
        drop(reopened);
        assert!(MonetaryMaterialGcJanitorV1::open(&file).unwrap().completed_receipts().next().is_none());
        drop(store);
        let _ = fs::remove_dir_all(dir);
    }
}
