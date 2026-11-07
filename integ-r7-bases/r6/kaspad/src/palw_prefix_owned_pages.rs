//! Publicly reconstructed owner pages. Content addresses authenticate every disk read;
//! the accepted root is the only branch selector. Genesis is deterministic ZERO, never a
//! producer snapshot. Only one page and a logarithmic Merkle path are resident at a time.
use kaspa_consensus_core::{
    Hash64,
    palw_prefix_gdn_paged_v1::{
        PALW_GDN_PAGE_WORDS_V1, PalwGdnOwnedPageOpeningV1, palw_gdn_owned_node_v1, palw_gdn_owned_page_hash_v1,
    },
};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};
struct Inventory {
    disk: Arc<Mutex<(u64, u64, u64, u64)>>,
    scan: Option<fs::ReadDir>,
    touched: Instant,
}
static DISK: OnceLock<Mutex<BTreeMap<PathBuf, Inventory>>> = OnceLock::new();
const INVENTORY_PENDING: &str = "public owner page inventory pending";

pub(super) struct OwnerPagesV1 {
    directory: PathBuf,
    zero: Vec<Hash64>,
    depth: usize,
    disk: Arc<Mutex<(u64, u64, u64, u64)>>,
}
impl OwnerPagesV1 {
    pub(super) fn open_for_seed(directory: &Path, seed: Hash64, d_k: u32, d_v: u32, maximum_moves: u64) -> Result<Self, String> {
        Self::open_for_plan(&directory.join(format!("pages-{seed}")).join("canonical"), d_k, d_v, maximum_moves)
    }
    pub(super) fn open_replay_for_seed(
        directory: &Path,
        seed: Hash64,
        d_k: u32,
        d_v: u32,
        maximum_moves: u64,
    ) -> Result<Self, String> {
        Self::open_for_plan(&directory.join(format!("pages-{seed}")).join("replay"), d_k, d_v, maximum_moves)
    }
    pub(super) fn inventory_pending_v1(error: &str) -> bool {
        error == INVENTORY_PENDING
    }
    fn invalidate(directory: &Path) -> Result<(), String> {
        let Some(registry) = DISK.get() else {
            return Ok(());
        };
        let mut registry = registry.lock().map_err(|_| "owner disk registry poisoned")?;
        if let Ok(path) = fs::canonicalize(directory) {
            registry.remove(&path);
        }
        Ok(())
    }
    fn clear_files(directory: &Path, maximum: usize) -> Result<(usize, bool), String> {
        if !directory.exists() {
            return Ok((0, true));
        }
        Self::invalidate(directory)?;
        let mut removed = 0;
        let mut entries = fs::read_dir(directory).map_err(|e| e.to_string())?;
        for entry in entries.by_ref().take(maximum) {
            let entry = entry.map_err(|e| e.to_string())?;
            if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
                return Err("non-file in derived owner page cache".into());
            }
            fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
            removed += 1;
        }
        Ok((removed, entries.next().is_none()))
    }
    /// Only incomplete derived replay pages are discarded. Canonical paid-root
    /// acceleration and authenticated original records are not touched by reset.
    pub(super) fn reset_replay_v1(directory: &Path, seed: Hash64) -> Result<bool, String> {
        Self::clear_files(&directory.join(format!("pages-{seed}")).join("replay"), 64).map(|(_, done)| done)
    }
    /// Called only AFTER the worker reproduces the exact rooted history/cursor.
    /// Fixed namespaces bound reorg storage; garbage contains derived pages only.
    pub(super) fn promote_replay_v1(directory: &Path, seed: Hash64) -> Result<bool, String> {
        let scope = directory.join(format!("pages-{seed}"));
        let canonical = scope.join("canonical");
        let replay = scope.join("replay");
        let garbage = scope.join("garbage");
        if !Self::clear_files(&garbage, 64)?.1 {
            return Ok(false);
        }
        if garbage.exists() {
            fs::remove_dir(&garbage).map_err(|e| e.to_string())?;
        }
        Self::invalidate(&canonical)?;
        Self::invalidate(&replay)?;
        if canonical.exists() {
            fs::rename(&canonical, &garbage).map_err(|e| e.to_string())?;
        }
        if let Err(error) = fs::rename(&replay, &canonical) {
            if garbage.exists() && !canonical.exists() {
                let _ = fs::rename(&garbage, &canonical);
            }
            return Err(error.to_string());
        }
        fs::File::open(&scope).and_then(|f| f.sync_all()).map_err(|e| e.to_string())?;
        Ok(true)
    }
    /// Both clocks must come from authenticated native state, never a cache manifest.
    /// Expiration at a reversible tip is insufficient to erase still-paid reorg roots.
    pub(super) fn gc_expired_seed_v1(
        directory: &Path,
        seed: Hash64,
        material_until: u64,
        irreversible_daa: u64,
    ) -> Result<usize, String> {
        if irreversible_daa <= material_until {
            return Ok(0);
        }
        let scope = directory.join(format!("pages-{seed}"));
        if !scope.exists() {
            return Ok(0);
        }
        let mut removed = 0;
        for name in ["canonical", "replay", "garbage"] {
            let directory = scope.join(name);
            let (n, done) = Self::clear_files(&directory, 64 - removed)?;
            removed += n;
            if done && directory.exists() {
                fs::remove_dir(directory).map_err(|e| e.to_string())?;
            }
            if removed == 64 {
                break;
            }
        }
        if fs::read_dir(&scope).map_err(|e| e.to_string())?.next().is_none() {
            fs::remove_dir(&scope).map_err(|e| e.to_string())?;
        }
        Ok(removed)
    }
    #[cfg(test)]
    pub(super) fn open(directory: &Path, d_k: u32, d_v: u32) -> Result<Self, String> {
        Self::open_for_plan(directory, d_k, d_v, 1_000_000)
    }
    pub(super) fn open_for_plan(directory: &Path, d_k: u32, d_v: u32, maximum_moves: u64) -> Result<Self, String> {
        if maximum_moves == 0 {
            return Err("owner page plan has no admitted movements".into());
        }
        if d_k == 0 || d_v == 0 || d_k > 1 << 17 || d_v > 1 << 17 {
            return Err("public owner pages exceed registered recurrence geometry".into());
        }
        let leaves = u64::from(d_k.div_ceil(PALW_GDN_PAGE_WORDS_V1 as u32))
            .checked_mul(u64::from(d_v))
            .ok_or("owner page count overflow")?
            .checked_next_power_of_two()
            .ok_or("owner page padding overflow")?;
        let depth = leaves.trailing_zeros() as usize;
        let mut zero = vec![palw_gdn_owned_page_hash_v1(&vec![0; PALW_GDN_PAGE_WORDS_V1])?];
        for level in 0..depth {
            zero.push(palw_gdn_owned_node_v1(&zero[level], &zero[level]));
        }
        fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        // One admitted movement writes at most one page and its logarithmic path.
        // Never delete a paid branch to make an unrelated speculative write fit.
        let maximum = maximum_moves
            .checked_mul((PALW_GDN_PAGE_WORDS_V1 * 4 + depth * 128) as u64)
            .and_then(|n| n.checked_add((PALW_GDN_PAGE_WORDS_V1 * 4) as u64))
            .ok_or("owner page disk budget overflow")?;
        let mut registry = DISK.get_or_init(|| Mutex::new(BTreeMap::new())).lock().map_err(|_| "owner disk registry poisoned")?;
        registry.retain(|_, v| Arc::strong_count(&v.disk) > 1 || v.touched.elapsed() < Duration::from_secs(300));
        let directory = fs::canonicalize(directory).map_err(|e| e.to_string())?;
        let maximum_files = maximum_moves.checked_mul(depth as u64 + 1).ok_or("owner page file budget overflow")?;
        if !registry.contains_key(&directory) {
            if registry.len() >= 128 {
                let oldest = registry
                    .iter()
                    .filter(|(_, v)| Arc::strong_count(&v.disk) == 1)
                    .min_by_key(|(_, v)| v.touched)
                    .map(|(k, _)| k.clone());
                if let Some(key) = oldest {
                    registry.remove(&key);
                } else {
                    return Err(INVENTORY_PENDING.into());
                }
            }
            registry.insert(
                directory.clone(),
                Inventory {
                    disk: Arc::new(Mutex::new((0, maximum, 0, maximum_files.saturating_add(1)))),
                    scan: Some(fs::read_dir(&directory).map_err(|e| e.to_string())?),
                    touched: Instant::now(),
                },
            );
        }
        let inventory = registry.get_mut(&directory).expect("inserted inventory");
        inventory.touched = Instant::now();
        let mut budget = inventory.disk.lock().map_err(|_| "owner page accounting poisoned")?;
        budget.1 = budget.1.min(maximum);
        budget.3 = budget.3.min(maximum_files.saturating_add(1));
        for _ in 0..64 {
            let Some(scan) = inventory.scan.as_mut() else {
                break;
            };
            let Some(entry) = scan.next() else {
                inventory.scan = None;
                break;
            };
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !(name.starts_with("page-") || name.starts_with("node-") || name.starts_with(".page-") || name.starts_with(".node-")) {
                continue;
            }
            budget.2 += 1;
            if budget.2 > budget.3 {
                return Err("public owner page cache exceeds admitted file budget".into());
            }
            budget.0 =
                budget.0.checked_add(entry.metadata().map_err(|e| e.to_string())?.len()).ok_or("owner page disk usage overflow")?;
            if budget.0 > budget.1 {
                return Err("public owner page cache exceeds admitted byte budget".into());
            }
        }
        if inventory.scan.is_some() {
            return Err(INVENTORY_PENDING.into());
        }
        if budget.0 > budget.1 {
            return Err("owner page cache exceeds requested immutable plan".into());
        }
        let disk = inventory.disk.clone();
        drop(budget);
        Ok(Self { directory, zero, depth, disk })
    }
    fn path(&self, kind: &str, hash: Hash64) -> PathBuf {
        self.directory.join(format!("{kind}-{hash}"))
    }
    fn read(&self, kind: &str, hash: Hash64, expected: usize) -> Result<Vec<u8>, String> {
        let file = fs::File::open(self.path(kind, hash)).map_err(|e| format!("accepted public owner page not held: {e}"))?;
        if file.metadata().map_err(|e| e.to_string())?.len() != expected as u64 {
            return Err("public owner page has noncanonical on-disk length".into());
        }
        let mut bytes = Vec::with_capacity(expected);
        file.take(expected as u64 + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        if bytes.len() != expected {
            return Err("public owner page changed during read".into());
        }
        Ok(bytes)
    }
    fn put(&self, kind: &str, hash: Hash64, bytes: &[u8]) -> Result<(), String> {
        let mut disk = self.disk.lock().map_err(|_| "owner page accounting poisoned")?;
        let path = self.path(kind, hash);
        if path.exists() {
            if self.read(kind, hash, bytes.len()).is_ok_and(|existing| existing == bytes) {
                return Ok(());
            }
            // A corrupt acceleration file is replaced only by freshly kernel-derived
            // bytes under their computed content address, never trusted backup content.
        }
        let temp = self.directory.join(format!(".{kind}-{hash}.tmp"));
        if let Ok(metadata) = fs::metadata(&temp) {
            fs::remove_file(&temp).map_err(|e| e.to_string())?;
            disk.0 = disk.0.checked_sub(metadata.len()).ok_or("owner page stale temporary accounting mismatch")?;
            disk.2 = disk.2.checked_sub(1).ok_or("owner page stale temporary file count mismatch")?;
        }
        let old_metadata = fs::metadata(&path).ok();
        let old_length = old_metadata.as_ref().map(|m| m.len()).unwrap_or(0);
        let next = disk
            .0
            .checked_sub(old_length)
            .and_then(|n| n.checked_add(bytes.len() as u64))
            .ok_or("owner page disk accounting overflow")?;
        // Include the atomic replacement's temporary copy in the peak bound.
        if disk.0.checked_add(bytes.len() as u64).ok_or("owner page temporary budget overflow")? > disk.1
            || disk.2.checked_add(1).ok_or("owner page temporary file count overflow")? > disk.3
        {
            return Err("public owner page cache has no admitted disk capacity; paid branches retained".into());
        }
        // A crash can leave an unreferenced partial temp, never an accepted-root page.
        disk.0 += bytes.len() as u64;
        disk.2 += 1;
        let written = (|| -> Result<(), String> {
            let mut file = fs::OpenOptions::new().write(true).create(true).truncate(true).open(&temp).map_err(|e| e.to_string())?;
            file.write_all(bytes).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            fs::rename(&temp, &path).map_err(|e| e.to_string())
        })();
        if let Err(error) = written {
            if !temp.exists() || fs::remove_file(&temp).is_ok() {
                disk.0 -= bytes.len() as u64;
                disk.2 -= 1;
            }
            return Err(error);
        }
        disk.0 = next;
        if old_metadata.is_some() {
            disk.2 -= 1;
        }
        fs::File::open(&self.directory).and_then(|f| f.sync_all()).map_err(|e| e.to_string())?;
        Ok(())
    }
    pub(super) fn opening(&self, root: Hash64, index: u64) -> Result<PalwGdnOwnedPageOpeningV1, String> {
        if index >= (1u64 << self.depth) {
            return Err("owner page index outside original geometry".into());
        }
        let mut node = root;
        let mut siblings = Vec::with_capacity(self.depth);
        for level in (1..=self.depth).rev() {
            let (left, right) = if node == self.zero[level] {
                (self.zero[level - 1], self.zero[level - 1])
            } else {
                let raw = self.read("node", node, 128)?;
                let left = Hash64::from_bytes(raw[..64].try_into().expect("64"));
                let right = Hash64::from_bytes(raw[64..].try_into().expect("64"));
                if palw_gdn_owned_node_v1(&left, &right) != node {
                    return Err("owner Merkle node content address does not authenticate".into());
                }
                (left, right)
            };
            if (index >> (level - 1)) & 1 == 0 {
                siblings.push(right);
                node = left;
            } else {
                siblings.push(left);
                node = right;
            }
        }
        let values = if node == self.zero[0] {
            vec![0; PALW_GDN_PAGE_WORDS_V1]
        } else {
            let raw = self.read("page", node, PALW_GDN_PAGE_WORDS_V1 * 4)?;
            let values: Vec<i32> = raw.chunks_exact(4).map(|b| i32::from_le_bytes(b.try_into().expect("4"))).collect();
            if palw_gdn_owned_page_hash_v1(&values)? != node {
                return Err("owner page content address does not authenticate".into());
            }
            values
        };
        siblings.reverse();
        Ok(PalwGdnOwnedPageOpeningV1 { values, siblings })
    }
    /// Persist the computed successor before a write carrier is sent. Both branches stay
    /// available, so an unaccepted send, restart or reorg cannot rename the accepted root.
    pub(super) fn save_successor(&self, index: u64, prior: &PalwGdnOwnedPageOpeningV1, values: &[i32]) -> Result<Hash64, String> {
        if prior.siblings.len() != self.depth || values.len() != PALW_GDN_PAGE_WORDS_V1 || index >= (1u64 << self.depth) {
            return Err("owner successor has noncanonical page geometry".into());
        }
        let mut hash = palw_gdn_owned_page_hash_v1(values)?;
        self.put("page", hash, &values.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>())?;
        for (level, sibling) in prior.siblings.iter().enumerate() {
            let (left, right) = if (index >> level) & 1 == 0 { (hash, *sibling) } else { (*sibling, hash) };
            hash = palw_gdn_owned_node_v1(&left, &right);
            let mut raw = Vec::with_capacity(128);
            raw.extend_from_slice(left.as_byte_slice());
            raw.extend_from_slice(right.as_byte_slice());
            self.put("node", hash, &raw)?;
        }
        Ok(hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_owner_pages_select_only_accepted_roots_across_restart_and_reorg() {
        let dir = tempfile::tempdir().unwrap();
        let pages = OwnerPagesV1::open(dir.path(), 4097, 3).unwrap();
        let genesis = pages.zero[pages.depth];
        let zero = pages.opening(genesis, 1).unwrap();
        let mut first = zero.values.clone();
        first[0] = 17;
        let root1 = pages.save_successor(1, &zero, &first).unwrap();
        assert_eq!(pages.opening(genesis, 1).unwrap(), zero, "sending did not acknowledge successor");
        drop(pages);
        let restarted = OwnerPagesV1::open(dir.path(), 4097, 3).unwrap();
        assert_eq!(restarted.opening(root1, 1).unwrap().values, first);
        let prior = restarted.opening(root1, 4).unwrap();
        let mut second = prior.values.clone();
        second[1] = -19;
        let root2 = restarted.save_successor(4, &prior, &second).unwrap();
        assert_eq!(restarted.opening(root2, 1).unwrap().values, first);
        assert_eq!(restarted.opening(root2, 4).unwrap().values, second);
        assert_eq!(restarted.opening(root1, 4).unwrap().values, zero.values, "reorg did not import newer speculative page");
        assert!(restarted.opening(root2, 8).is_err());
        let page_hash = palw_gdn_owned_page_hash_v1(&first).unwrap();
        fs::write(restarted.path("page", page_hash), vec![0u8; PALW_GDN_PAGE_WORDS_V1 * 4]).unwrap();
        assert!(restarted.opening(root2, 1).is_err(), "untrusted disk cannot replace authenticated public computation");
    }
    #[test]
    fn lost_page_acceleration_is_rebuilt_from_public_transition_outputs_not_backup_state() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("owned");
        let pages = OwnerPagesV1::open(&path, 8193, 2).unwrap();
        let genesis = pages.zero[pages.depth];
        let first_opening = pages.opening(genesis, 0).unwrap();
        let mut first_output = first_opening.values.clone();
        first_output[0] = 31;
        let root1 = pages.save_successor(0, &first_opening, &first_output).unwrap();
        let second_opening = pages.opening(root1, 5).unwrap();
        let mut second_output = second_opening.values.clone();
        second_output[7] = -43;
        let root2 = pages.save_successor(5, &second_opening, &second_output).unwrap();
        drop(pages);
        fs::remove_dir_all(&path).unwrap();
        let rebuilt = OwnerPagesV1::open(&path, 8193, 2).unwrap();
        assert!(rebuilt.opening(root2, 5).is_err(), "missing public operands remain pending before replay");
        // This tests the storage primitive. The recovery worker separately derives these
        // outputs with the actual family kernel from authenticated public source history.
        assert_eq!(rebuilt.save_successor(0, &first_opening, &first_output).unwrap(), root1);
        assert_eq!(rebuilt.save_successor(5, &second_opening, &second_output).unwrap(), root2);
        assert_eq!(rebuilt.opening(root2, 0).unwrap().values, first_output);
        assert_eq!(rebuilt.opening(root2, 5).unwrap().values, second_output);
    }
    #[test]
    fn admitted_page_cap_is_shared_and_never_evicts_a_paid_root() {
        let dir = tempfile::tempdir().unwrap();
        let pages = OwnerPagesV1::open_for_plan(dir.path(), 4097, 3, 1).unwrap();
        let genesis = pages.zero[pages.depth];
        let opening = pages.opening(genesis, 0).unwrap();
        let mut values = opening.values.clone();
        values[0] = 21;
        let root = pages.save_successor(0, &opening, &values).unwrap();
        let second = OwnerPagesV1::open_for_plan(dir.path(), 4097, 3, 1).unwrap();
        assert!(Arc::ptr_eq(&pages.disk, &second.disk));
        let another = second.opening(root, 1).unwrap();
        let mut speculative = another.values.clone();
        speculative[0] = -23;
        assert!(second.save_successor(1, &another, &speculative).is_err());
        assert_eq!(pages.opening(root, 0).unwrap().values, values);
        let budget = pages.disk.lock().unwrap();
        assert!(budget.0 <= budget.1);
        assert!(OwnerPagesV1::open_for_plan(dir.path(), 4097, 3, u64::MAX).is_err());
    }
    #[test]
    fn seed_namespaces_and_irreversible_expiry_keep_paid_branches_isolated() {
        let dir = tempfile::tempdir().unwrap();
        let a = Hash64::from_u64_word(41);
        let b = Hash64::from_u64_word(42);
        let pages = OwnerPagesV1::open_for_seed(dir.path(), a, 4096, 1, 2).unwrap();
        let other = OwnerPagesV1::open_for_seed(dir.path(), b, 4096, 1, 2).unwrap();
        let opening = pages.opening(pages.zero[pages.depth], 0).unwrap();
        let mut values = opening.values.clone();
        values[0] = 47;
        let root = pages.save_successor(0, &opening, &values).unwrap();
        let other_root = other.save_successor(0, &opening, &values).unwrap();
        assert_eq!(OwnerPagesV1::gc_expired_seed_v1(dir.path(), a, 200, 200).unwrap(), 0);
        assert_eq!(pages.opening(root, 0).unwrap().values, values);
        assert!(OwnerPagesV1::gc_expired_seed_v1(dir.path(), a, 200, 201).unwrap() > 0);
        assert!(pages.opening(root, 0).is_err());
        assert_eq!(other.opening(other_root, 0).unwrap().values, values);
    }
    #[test]
    fn repeated_reorg_reconstruction_promotes_only_a_complete_new_root_with_fixed_storage_namespaces() {
        let dir = tempfile::tempdir().unwrap();
        let seed = Hash64::from_u64_word(61);
        let canonical = OwnerPagesV1::open_for_seed(dir.path(), seed, 4097, 3, 2).unwrap();
        let genesis = canonical.zero[canonical.depth];
        let first = canonical.opening(genesis, 0).unwrap();
        let mut original = first.values.clone();
        original[0] = 63;
        let accepted = canonical.save_successor(0, &first, &original).unwrap();
        for branch in 0..12 {
            assert!(OwnerPagesV1::reset_replay_v1(dir.path(), seed).unwrap());
            let replay = OwnerPagesV1::open_replay_for_seed(dir.path(), seed, 4097, 3, 2).unwrap();
            let opening = replay.opening(genesis, 0).unwrap();
            let mut derived = opening.values.clone();
            derived[0] = 71 + branch;
            let root = replay.save_successor(0, &opening, &derived).unwrap();
            // This storage primitive is called by recovery only after full signed
            // history verification and exact native cursor reproduction, not by a
            // disk branch flag or this locally computed page hash alone.
            if branch == 0 {
                assert_eq!(canonical.opening(accepted, 0).unwrap().values, original);
            }
            assert_eq!(replay.opening(root, 0).unwrap().values, derived);
            assert!(OwnerPagesV1::promote_replay_v1(dir.path(), seed).unwrap());
            let current = OwnerPagesV1::open_for_seed(dir.path(), seed, 4097, 3, 2).unwrap();
            assert_eq!(current.opening(root, 0).unwrap().values, derived);
            let scope = dir.path().join(format!("pages-{seed}"));
            assert!(fs::read_dir(scope).unwrap().count() <= 3, "no directory per reorg or accepted cursor");
        }
    }
    #[test]
    fn restart_inventory_and_abandoned_replay_cleanup_are_bounded_and_preserve_canonical_pages() {
        let dir = tempfile::tempdir().unwrap();
        let inventory = dir.path().join("inventory");
        fs::create_dir(&inventory).unwrap();
        for i in 0..70 {
            fs::write(inventory.join(format!("node-{i}")), [0u8; 128]).unwrap();
        }
        assert!(OwnerPagesV1::open_for_plan(&inventory, 4097, 3, 100).err().is_some_and(|e| OwnerPagesV1::inventory_pending_v1(&e)));
        assert!(OwnerPagesV1::open_for_plan(&inventory, 4097, 3, 100).is_ok());
        let seed = Hash64::from_u64_word(81);
        let pages = OwnerPagesV1::open_for_seed(dir.path(), seed, 4096, 1, 100).unwrap();
        let prior = pages.opening(pages.zero[pages.depth], 0).unwrap();
        let mut values = prior.values.clone();
        values[0] = 83;
        let root = pages.save_successor(0, &prior, &values).unwrap();
        let replay = dir.path().join(format!("pages-{seed}")).join("replay");
        fs::create_dir(&replay).unwrap();
        for i in 0..70 {
            fs::write(replay.join(format!("node-{i}")), [0u8; 128]).unwrap();
        }
        assert!(!OwnerPagesV1::reset_replay_v1(dir.path(), seed).unwrap());
        assert_eq!(fs::read_dir(&replay).unwrap().count(), 6);
        assert_eq!(pages.opening(root, 0).unwrap().values, values);
        assert!(OwnerPagesV1::reset_replay_v1(dir.path(), seed).unwrap());
    }
}
