//! Public page recovery follows the court-authenticated history head. Records survive
//! body/delta pruning; missing records use the bounded public peer lane. No disk cursor,
//! peer fork flag or producer snapshot chooses the replay branch.
use super::*;
use kaspa_consensus_core::{
    api::ConsensusApi,
    palw_prefix_family_court_v1::PalwPrefixFamilyCourtV1,
    palw_prefix_history_v1::{PALW_PREFIX_HISTORY_MAX_RECORD_BYTES_V1, PalwPrefixHistoryRecordV1, PalwPrefixHistoryRequestV1},
    palw_prefix_public_source_v1::{PalwPrefixPublicPendingV1, PalwPrefixPublicSourceV1},
};
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

struct Replay {
    directory: PathBuf,
    cursor_id: Hash64,
    sequence: u64,
    head: Hash64,
    last_daa: u64,
    leaf: u64,
    leaves: u64,
    frontier: Vec<(u64, Hash64)>,
    index_root: Option<Hash64>,
    family: Option<PalwPrefixFamilyCourtV1>,
    records: Records,
    pending: Option<PalwPrefixPublicPendingV1>,
    touched: Instant,
    _ticket: crate::palw_memory_ledger::PalwMemoryReservationV1,
}
static REPLAYS: OnceLock<Mutex<Vec<Replay>>> = OnceLock::new();
fn parent(left: Hash64, right: Hash64) -> Hash64 {
    let mut hash = blake2b_simd::Params::new().hash_length(64).key(b"PALW/node/public-history-index/v1").to_state();
    hash.update(left.as_byte_slice());
    hash.update(right.as_byte_slice());
    Hash64::from_bytes(hash.finalize().as_bytes().try_into().expect("64"))
}
fn write_hash(path: &Path, index: u64, hash: Hash64) -> Result<(), String> {
    let mut file = fs::OpenOptions::new().write(true).open(path).map_err(|e| e.to_string())?;
    file.seek(SeekFrom::Start(index.checked_mul(64).ok_or("history index offset overflow")?)).map_err(|e| e.to_string())?;
    // Disposable index: restart truncates/reverifies it, and every semantic read
    // proves membership under the trusted runtime root. Only raw CAS is durable.
    file.write_all(hash.as_byte_slice()).map_err(|e| e.to_string())
}
fn read_hash(file: &mut fs::File, index: u64) -> Result<Hash64, String> {
    file.seek(SeekFrom::Start(index.checked_mul(64).ok_or("history index offset overflow")?)).map_err(|e| e.to_string())?;
    let mut hash = [0u8; 64];
    file.read_exact(&mut hash).map_err(|e| e.to_string())?;
    Ok(Hash64::from_bytes(hash))
}
fn reference(path: &Path, sequence: u64, leaves: u64, root: Hash64) -> Result<Hash64, String> {
    if sequence == 0 || sequence > leaves || !leaves.is_power_of_two() {
        return Err("noncanonical history reference index".into());
    }
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() != leaves.checked_mul(128).ok_or("history index length overflow")? {
        return Err("history reference index changed geometry".into());
    }
    let mut index = leaves + sequence - 1;
    let head = read_hash(&mut file, index)?;
    let mut hash = head;
    while index > 1 {
        let sibling = read_hash(&mut file, index ^ 1)?;
        hash = if index & 1 == 0 { parent(hash, sibling) } else { parent(sibling, hash) };
        index /= 2;
    }
    if hash != root {
        return Err("history sequence reference does not authenticate against verified branch".into());
    }
    Ok(head)
}
struct Records {
    directory: PathBuf,
    held: u64,
    maximum: u64,
    count: u64,
    maximum_count: u64,
    scan: Option<fs::ReadDir>,
}
impl Records {
    fn open(directory: PathBuf, maximum_moves: u64) -> Result<Self, String> {
        fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let maximum =
            maximum_moves.checked_mul(PALW_PREFIX_HISTORY_MAX_RECORD_BYTES_V1 as u64).ok_or("public record disk budget overflow")?;
        let scan = Some(fs::read_dir(&directory).map_err(|e| e.to_string())?);
        Ok(Self { directory, held: 0, maximum, count: 0, maximum_count: maximum_moves, scan })
    }
    fn inventory(&mut self) -> Result<bool, String> {
        for _ in 0..64 {
            let Some(scan) = self.scan.as_mut() else {
                return Ok(true);
            };
            let Some(entry) = scan.next() else {
                self.scan = None;
                return Ok(true);
            };
            let entry = entry.map_err(|e| e.to_string())?;
            self.count += 1;
            if self.count > self.maximum_count {
                return Err("public record cache exceeds admitted file budget".into());
            }
            self.held =
                self.held.checked_add(entry.metadata().map_err(|e| e.to_string())?.len()).ok_or("record cache accounting overflow")?;
            if self.held > self.maximum {
                return Err("public record cache exceeds admitted byte budget".into());
            }
        }
        Ok(false)
    }
    fn path(&self, head: Hash64) -> PathBuf {
        self.directory.join(format!("record-{head}"))
    }
    fn read(&self, wanted: &PalwPrefixHistoryRequestV1) -> Option<PalwPrefixHistoryRecordV1> {
        bounded(&self.path(wanted.head), PALW_PREFIX_HISTORY_MAX_RECORD_BYTES_V1)
            .ok()
            .and_then(|raw| borsh::from_slice::<PalwPrefixHistoryRecordV1>(&raw).ok())
            .filter(|r| r.authenticates_request_v1(wanted).is_ok())
    }
    fn save(&mut self, wanted: &PalwPrefixHistoryRequestV1, row: &PalwPrefixHistoryRecordV1) -> Result<(), String> {
        if self.scan.is_some() {
            return Err("public record inventory pending".into());
        }
        row.authenticates_request_v1(wanted)?;
        if self.read(wanted).is_some() {
            return Ok(());
        }
        let raw = borsh::to_vec(row).map_err(|e| e.to_string())?;
        let path = self.path(wanted.head);
        let old = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        // This is acceleration only: a full cache does not block a public fetch/replay,
        // and still-paid previous branch records are never evicted to admit this one.
        if (old == 0 && self.count >= self.maximum_count)
            || self.held.checked_add(raw.len() as u64).ok_or("record cache peak overflow")? > self.maximum
        {
            return Ok(());
        }
        put(&path, &raw)?;
        if old == 0 {
            self.count += 1;
        }
        self.held =
            self.held.checked_sub(old).and_then(|n| n.checked_add(raw.len() as u64)).ok_or("record cache accounting overflow")?;
        Ok(())
    }
}
fn put(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = path.with_extension("tmp");
    let mut file = fs::File::create(&temp).map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    fs::rename(temp, path).map_err(|e| e.to_string())?;
    fs::File::open(path.parent().ok_or("recovery path lacks parent")?).and_then(|f| f.sync_all()).map_err(|e| e.to_string())
}
fn bounded(path: &Path, maximum: usize) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > maximum as u64 {
        return Err("public recovery record exceeds bound".into());
    }
    let mut raw = Vec::new();
    file.take(maximum as u64 + 1).read_to_end(&mut raw).map_err(|e| e.to_string())?;
    if raw.len() > maximum {
        return Err("public recovery record changed during read".into());
    }
    Ok(raw)
}
fn request(progress: &PalwStagedOwnedProgressV1, seed: Hash64, sequence: u64, head: Hash64) -> PalwPrefixHistoryRequestV1 {
    PalwPrefixHistoryRequestV1 {
        claim: progress.claim,
        owner: progress.reserver,
        reserved_daa: progress.reserved_daa,
        seed_id: seed,
        head,
        sequence,
        material_until_daa: progress.terms.material_until_daa,
    }
}

/// The seed and original horizon must be obtained from authenticated paid authority.
/// A cached manifest, tip DAA, peer clock, or stale response is not GC authority.
pub(super) fn gc_expired_seed_v1(directory: &Path, seed: Hash64, material_until: u64, irreversible_daa: u64) -> Result<usize, String> {
    if irreversible_daa <= material_until {
        return Ok(0);
    }
    let scope = directory.join(format!("history-{seed}"));
    if let Some(workers) = REPLAYS.get() {
        let mut workers = workers.lock().map_err(|_| "public recovery registry poisoned")?;
        if let Ok(scope) = fs::canonicalize(&scope) {
            workers.retain(|w| w.directory != scope);
        }
    }
    let records = scope.join("records");
    let mut removed = 0;
    if records.exists() {
        for entry in fs::read_dir(&records).map_err(|e| e.to_string())?.take(64) {
            let entry = entry.map_err(|e| e.to_string())?;
            if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
                return Err("non-file inside expired public history scope".into());
            }
            fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
            removed += 1;
        }
    }
    if scope.join("references").exists() {
        fs::remove_file(scope.join("references")).map_err(|e| e.to_string())?;
    }
    if records.exists() && fs::read_dir(&records).map_err(|e| e.to_string())?.next().is_none() {
        fs::remove_dir(&records).map_err(|e| e.to_string())?;
    }
    if scope.exists() && fs::read_dir(&scope).map_err(|e| e.to_string())?.next().is_none() {
        fs::remove_dir(&scope).map_err(|e| e.to_string())?;
    }
    Ok(removed)
}

/// Service-wide GC takes a native lease token, not a private cached expiry. Returning
/// `false` keeps this lease in the caller's fair queue for another bounded tick.
pub(super) fn gc_prefix_cache_v1(directory: &Path, seed: Hash64, material_until: u64, irreversible_daa: u64) -> Result<bool, String> {
    if irreversible_daa <= material_until {
        return Ok(false);
    }
    gc_expired_seed_v1(directory, seed, material_until, irreversible_daa)?;
    owned_pages::OwnerPagesV1::gc_expired_seed_v1(directory, seed, material_until, irreversible_daa)?;
    Ok(!directory.join(format!("history-{seed}")).exists() && !directory.join(format!("pages-{seed}")).exists())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn recover_v1(
    consensus: &dyn ConsensusApi,
    target: &PalwOffenceTargetV1,
    binding: &PalwStepBindingV2,
    job: &PalwFreePromptJobV3,
    progress: &PalwStagedOwnedProgressV1,
    maximum_leaves: u64,
    directory: &Path,
    mut pull: impl FnMut(&PalwPrefixHistoryRequestV1) -> Result<Option<PalwPrefixHistoryRecordV1>, String>,
) -> Result<bool, String> {
    palw_constrained_job_target_v1(target, binding, job).map_err(|e| e.to_string())?;
    let Some(PalwStagedOwnedCursorV1::PublicPrefix {
        cursor: expected,
        history_count,
        history_head,
        pending: None,
        completed: None,
        ..
    }) = &progress.cursor
    else {
        return Err("page recovery lacks accepted public semantic cursor".into());
    };
    if *history_count == 0
        || *history_count > progress.terms.plan.maximum_moves
        || *history_head == Hash64::default()
        || progress.now_daa >= progress.terms.material_until_daa
    {
        return Err("page recovery lacks funded rooted history or original material clock".into());
    }
    let (accepted_root, page_count, depth) = progress.prefix_page_root.ok_or("page recovery lacks native active owner root")?;
    let geometry = u64::from(binding.shape_profile.gdn_head_k_dim.div_ceil(4096))
        .checked_mul(u64::from(binding.shape_profile.gdn_head_v_dim))
        .ok_or("page recovery geometry overflow")?;
    if geometry == 0
        || page_count != geometry
        || page_count.checked_next_power_of_two().map(|n| n.trailing_zeros() as u8) != Some(depth)
        || !matches!(expected, PalwPrefixFamilyCursorV1::HeadPage { root, .. } | PalwPrefixFamilyCursorV1::HeadPageDigest { root, .. } if *root == accepted_root)
    {
        return Err("page recovery active root/geometry differs from native continuation".into());
    }
    let seed = progress.seed_id.ok_or("page recovery lacks immutable seed")?;
    let cursor_raw = borsh::to_vec(&(seed, expected, history_count, history_head)).map_err(|e| e.to_string())?;
    let cursor_id = Hash64::from_bytes(
        blake2b_simd::Params::new()
            .hash_length(64)
            .key(b"PALW/node/public-page-recovery/v2")
            .hash(&cursor_raw)
            .as_bytes()
            .try_into()
            .expect("64"),
    );
    // The caller's immutable owner/claim directory contains one seed CAS and one
    // replaceable branch index, not a complete archive for every accepted cursor.
    let recovery = directory.join(format!("history-{seed}"));
    fs::create_dir_all(&recovery).map_err(|e| e.to_string())?;
    let recovery = fs::canonicalize(recovery).map_err(|e| e.to_string())?;
    let index_path = recovery.join("references");
    let Ok(mut replays) = REPLAYS.get_or_init(|| Mutex::new(Vec::new())).try_lock() else {
        return Ok(false);
    };
    // RAII reservations remain live across bounded polls; abandoned workers do not
    // retain matrix/source buffers indefinitely. A restart reauthenticates genesis.
    replays.retain(|r| r.touched.elapsed() < Duration::from_secs(30));
    if replays.iter().any(|r| r.directory == recovery && r.cursor_id != cursor_id) {
        replays.retain(|r| r.directory != recovery);
    }
    if !replays.iter().any(|r| r.directory == recovery) {
        if replays.len() >= 16 {
            return Ok(false);
        }
        if !owned_pages::OwnerPagesV1::reset_replay_v1(directory, seed)? {
            return Ok(false);
        }
        let ticket = crate::palw_memory_ledger::host_ledger_v1()
            .reserve(
                crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                    role: "public-prefix-page-recovery",
                    class_id: target.class_id,
                    job: progress.claim,
                },
                progress.terms.plan.resident_ram_bytes.checked_add(1_048_576).ok_or("recovery memory overflow")?,
            )
            .map_err(|e| e.to_string())?;
        let leaves = history_count.checked_next_power_of_two().ok_or("history index padding overflow")?;
        let records = Records::open(recovery.join("records"), progress.terms.plan.maximum_moves)?;
        let file = fs::OpenOptions::new().write(true).create(true).truncate(true).open(&index_path).map_err(|e| e.to_string())?;
        file.set_len(leaves.checked_mul(128).ok_or("history reference budget overflow")?).map_err(|e| e.to_string())?;
        replays.push(Replay {
            directory: recovery.clone(),
            cursor_id,
            sequence: *history_count,
            head: *history_head,
            last_daa: progress.now_daa,
            leaf: leaves - 1,
            leaves,
            frontier: Vec::with_capacity(64),
            index_root: None,
            family: None,
            records,
            pending: None,
            touched: Instant::now(),
            _ticket: ticket,
        });
    }
    let position = replays.iter().position(|r| r.directory == recovery).expect("inserted replay");
    let mut replay = replays.remove(position);
    replay.touched = Instant::now();
    let result = (|| -> Result<bool, String> {
        if !replay.records.inventory()? {
            return Ok(false);
        }
        if replay.index_root.is_none() {
            for _ in 0..64 {
                let leaf_hash = if replay.leaf >= *history_count {
                    Hash64::default()
                } else {
                    let wanted = request(progress, seed, replay.sequence, replay.head);
                    let row = if let Some(row) = replay.records.read(&wanted) {
                        Some(row)
                    } else {
                        match consensus.palw_prefix_history_record_v1(progress.snapshot_block, wanted.clone()) {
                            Ok(Some(row)) => Some(row),
                            // Public bytes can come from a peer; the pinned accepted head, never
                            // the peer's clock/fork or a local watermark, authenticates each link.
                            Ok(None) | Err(_) => pull(&wanted)?,
                        }
                    };
                    let Some(row) = row else {
                        return Ok(false);
                    };
                    row.authenticates_request_v1(&wanted)?;
                    if row.accepted_daa > replay.last_daa {
                        return Err("public history changes accepted clock ordering".into());
                    }
                    replay.last_daa = row.accepted_daa;
                    replay.records.save(&wanted, &row)?;
                    replay.sequence -= 1;
                    replay.head = row.previous_head;
                    wanted.head
                };
                let mut node = replay.leaves + replay.leaf;
                let mut hash = leaf_hash;
                write_hash(&index_path, node, hash)?;
                while node & 1 == 0 && replay.frontier.last().is_some_and(|(s, _)| *s == node + 1) {
                    let (_, right) = replay.frontier.pop().expect("right sibling");
                    hash = parent(hash, right);
                    node /= 2;
                    write_hash(&index_path, node, hash)?;
                }
                replay.frontier.push((node, hash));
                if replay.leaf == 0 {
                    if replay.head != Hash64::default() || replay.sequence != 0 || replay.frontier.len() != 1 || node != 1 {
                        return Err("public history does not terminate at authenticated genesis".into());
                    }
                    replay.index_root = Some(hash);
                    replay.family = Some(PalwPrefixFamilyCourtV1::new(
                        seed,
                        target,
                        binding,
                        job,
                        maximum_leaves,
                        progress.terms.plan.resident_ram_bytes,
                    )?);
                    replay.sequence = 1;
                    replay.last_daa = progress.reserved_daa;
                    return Ok(false);
                }
                replay.leaf -= 1;
            }
            return Ok(false);
        }
        let pages = match owned_pages::OwnerPagesV1::open_replay_for_seed(
            directory,
            seed,
            binding.shape_profile.gdn_head_k_dim,
            binding.shape_profile.gdn_head_v_dim,
            progress.terms.plan.maximum_moves,
        ) {
            Ok(pages) => pages,
            Err(error) if owned_pages::OwnerPagesV1::inventory_pending_v1(&error) => return Ok(false),
            Err(error) => return Err(error),
        };
        for _ in 0..64 {
            if replay.sequence > *history_count {
                break;
            }
            let head = reference(&index_path, replay.sequence, replay.leaves, replay.index_root.expect("verified index"))?;
            let wanted = request(progress, seed, replay.sequence, head);
            let record = match replay.records.read(&wanted) {
                Some(row) => Some(row),
                None => match consensus.palw_prefix_history_record_v1(progress.snapshot_block, wanted.clone()) {
                    Ok(Some(row)) => Some(row),
                    Ok(None) | Err(_) => pull(&wanted)?,
                },
            };
            let Some(record) = record else {
                return Ok(false);
            };
            record.authenticates_request_v1(&wanted)?;
            if record.previous_head != replay.head || record.accepted_daa < replay.last_daa {
                return Err("semantic replay changed authenticated link order".into());
            }
            replay.head = head;
            replay.last_daa = record.accepted_daa;
            let now = record.accepted_daa;
            let movement = record.movement;
            movement.check()?;
            if (movement.claim, movement.reserver, movement.reserved_daa, movement.seed_id)
                != (progress.claim, progress.reserver, progress.reserved_daa, seed)
            {
                return Err("public replay changes immutable ownership".into());
            }
            let family = replay.family.as_mut().expect("initialized public family");
            let pending = &mut replay.pending;
            match &movement.action {
                PalwConstraintStreamActionV1::PrefixPublicDemand(demand) => {
                    if pending.is_some() {
                        return Err("public replay overlaps demanded sources".into());
                    }
                    let source = family.next_public_source_v1()?.ok_or("public replay demands unrelated source")?;
                    if source.coordinate != demand.coordinate || !source.needs_opening || source.offset != 0 {
                        return Err("public replay demand changes canonical continuation".into());
                    }
                    let source = PalwPrefixPublicSourceV1::derive(binding, source.coordinate, maximum_leaves)?;
                    let demand_id = Hash64::from_bytes(movement.signing_message().try_into().map_err(|_| "demand width")?);
                    *pending = Some(PalwPrefixPublicPendingV1::new(
                        progress.claim,
                        progress.reserver,
                        progress.reserved_daa,
                        seed,
                        demand.cursor_id,
                        demand_id,
                        now,
                        demand.deadline_daa,
                        source,
                        progress.prefix_custodians.as_ref().ok_or("public replay lacks frozen custody")?.clone(),
                    )?);
                }
                PalwConstraintStreamActionV1::PrefixPublicAnswer(answer) => {
                    let saved = pending.as_mut().ok_or("public answer lacks exact demand")?;
                    if !saved.matches(
                        progress.claim,
                        progress.reserver,
                        progress.reserved_daa,
                        seed,
                        answer.cursor_id,
                        answer.demand_id,
                    ) {
                        return Err("public answer changes literal pending identity".into());
                    }
                    saved.packet(answer.answerer, now, &answer.action)?;
                    if let Some((opening, bytes)) = saved.authenticated_answer() {
                        for action in family.authenticated_public_source_actions_v1(seed, opening, bytes)? {
                            family.movement(seed, &action)?;
                        }
                        *pending = None;
                    }
                }
                PalwConstraintStreamActionV1::Prefix(action) => replay_owner(family, &pages, seed, action)?,
                PalwConstraintStreamActionV1::PrefixBatch(actions) => {
                    for action in actions {
                        replay_owner(family, &pages, seed, action)?;
                    }
                }
                _ => return Err("public page replay changes typed continuation".into()),
            }
            replay.sequence += 1;
        }
        if replay.sequence <= *history_count {
            return Ok(false);
        }
        if replay.head != *history_head
            || replay.pending.is_some()
            || replay.family.as_ref().expect("public family").cursor_v1()? != *expected
            || replay.family.as_ref().expect("public family").page_root_v1() != progress.prefix_page_root
        {
            return Err("public replay does not reproduce accepted history head and cursor/root".into());
        }
        match expected {
            PalwPrefixFamilyCursorV1::HeadPage { root, index, .. } | PalwPrefixFamilyCursorV1::HeadPageDigest { root, index, .. } => {
                pages.opening(*root, *index)?;
            }
            _ => return Err("public page recovery has no rooted active page to promote".into()),
        }
        owned_pages::OwnerPagesV1::promote_replay_v1(directory, seed)
    })();
    if matches!(result, Ok(false)) {
        replays.push(replay);
    }
    result
}
fn replay_owner(
    family: &mut PalwPrefixFamilyCourtV1,
    pages: &owned_pages::OwnerPagesV1,
    seed: Hash64,
    action: &PalwPrefixCourtActionV1,
) -> Result<(), String> {
    if let PalwPrefixCourtActionV1::HeadPageWrite { opening } = action {
        let PalwPrefixFamilyCursorV1::HeadPage { index, write: true, .. } = family.cursor_v1()? else {
            return Err("public page write changes deterministic cursor".into());
        };
        let values = family.preview_page_write_v1(opening)?;
        pages.save_successor(index, opening, &values)?;
    }
    family.movement(seed, action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        BlockHash,
        tx::{TransactionId, TransactionOutpoint},
    };
    fn record(block: u64) -> PalwPrefixHistoryRecordV1 {
        PalwPrefixHistoryRecordV1::new(
            Hash64::default(),
            1,
            BlockHash::from_u64_word(block),
            100,
            200,
            PalwConstraintStreamMoveV1 {
                version: 1,
                claim: Hash64::from_u64_word(1),
                reserver: PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(2), index: 0 }),
                reserved_daa: 90,
                seed_id: Hash64::from_u64_word(3),
                phase: PalwConstraintStreamPhaseV1::PrefixCourt,
                action: PalwConstraintStreamActionV1::Prefix(PalwPrefixCourtActionV1::FinishTile),
                signature: vec![7; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
            },
        )
        .unwrap()
    }
    fn wanted(row: &PalwPrefixHistoryRecordV1) -> PalwPrefixHistoryRequestV1 {
        PalwPrefixHistoryRequestV1 {
            claim: row.movement.claim,
            owner: row.movement.reserver,
            reserved_daa: row.movement.reserved_daa,
            seed_id: row.movement.seed_id,
            head: row.head_v1().unwrap(),
            sequence: row.sequence,
            material_until_daa: row.material_until_daa,
        }
    }
    #[test]
    fn seed_cas_deduplicates_and_full_cache_does_not_drop_paid_reorg_material() {
        let dir = tempfile::tempdir().unwrap();
        let mut records = Records::open(dir.path().join("records"), 1).unwrap();
        assert!(records.inventory().unwrap());
        let first = record(8);
        let req = wanted(&first);
        records.save(&req, &first).unwrap();
        records.save(&req, &first).unwrap();
        assert_eq!(fs::read_dir(&records.directory).unwrap().count(), 1);
        let fork = record(9);
        records.save(&wanted(&fork), &fork).unwrap();
        assert_eq!(records.read(&req), Some(first.clone()));
        assert!(records.read(&wanted(&fork)).is_none(), "full acceleration cache leaves new public row usable without saving it");
        let mut tampered = first.clone();
        tampered.movement.signature[0] ^= 1;
        assert!(records.save(&req, &tampered).is_err());
        fs::write(records.path(req.head), borsh::to_vec(&tampered).unwrap()).unwrap();
        assert!(records.read(&req).is_none(), "disk bytes never select an authenticated branch");
        drop(records);
        fs::remove_dir_all(dir.path().join("records")).unwrap();
        let mut rebuilt = Records::open(dir.path().join("records"), 1).unwrap();
        assert!(rebuilt.inventory().unwrap());
        rebuilt.save(&req, &first).unwrap();
        assert_eq!(rebuilt.read(&req), Some(first));
    }
    #[test]
    fn bounded_references_authenticate_every_sequence_and_reject_reorg_or_disk_substitution() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("references");
        let file = fs::File::create(&path).unwrap();
        file.set_len(4 * 128).unwrap();
        let heads = [Hash64::from_u64_word(11), Hash64::from_u64_word(12), Hash64::from_u64_word(13), Hash64::default()];
        let mut frontier: Vec<(u64, Hash64)> = Vec::new();
        for leaf in (0..4).rev() {
            let mut node = 4 + leaf;
            let mut hash = heads[leaf as usize];
            write_hash(&path, node, hash).unwrap();
            while node & 1 == 0 && frontier.last().is_some_and(|(s, _)| *s == node + 1) {
                hash = parent(hash, frontier.pop().unwrap().1);
                node /= 2;
                write_hash(&path, node, hash).unwrap();
            }
            frontier.push((node, hash));
        }
        assert_eq!(frontier.len(), 1);
        let root = frontier[0].1;
        for seq in 1..=3 {
            assert_eq!(reference(&path, seq, 4, root).unwrap(), heads[seq as usize - 1]);
        }
        write_hash(&path, 5, Hash64::from_u64_word(99)).unwrap();
        assert!(reference(&path, 2, 4, root).is_err());
        assert!(reference(&path, 1, 4, Hash64::from_u64_word(77)).is_err());
        assert!(reference(&path, 0, 4, root).is_err());
        assert_eq!(fs::metadata(path).unwrap().len(), 512, "index holds hashes, never duplicated signed movement bytes");
    }
    #[test]
    fn public_record_gc_requires_irreversible_original_horizon_and_is_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let seed = Hash64::from_u64_word(51);
        let scope = dir.path().join(format!("history-{seed}"));
        let records = scope.join("records");
        fs::create_dir_all(&records).unwrap();
        for i in 0..70 {
            fs::write(records.join(format!("record-{i}")), [0u8; 64]).unwrap();
        }
        fs::write(scope.join("references"), [0u8; 128]).unwrap();
        assert_eq!(gc_expired_seed_v1(dir.path(), seed, 200, 200).unwrap(), 0);
        assert!(scope.join("references").exists());
        assert_eq!(gc_expired_seed_v1(dir.path(), seed, 200, 201).unwrap(), 64);
        assert_eq!(fs::read_dir(&records).unwrap().count(), 6);
        assert_eq!(gc_expired_seed_v1(dir.path(), seed, 200, 201).unwrap(), 6);
    }
}
