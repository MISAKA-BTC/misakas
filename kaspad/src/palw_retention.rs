//! **The retention janitor: a node's `palw-retention/` is pruned whatever the node was started as.**
//!
//! A producer writes an attempt capture for every block it publishes (~0.8 GB on a graph-v5 class)
//! and a free-prompt capture for every claim it executes; a panel keeps the materials it verified
//! under `foreign/`. The prune used to run only inside the producer's draw loop, so a node started
//! without `--palw-produce` — a panel seat, a producer restarted as one, a node still syncing —
//! kept every file it had, and once a Qwen3.6 draw took a minute instead of eighteen a producer wrote
//! ~30 GB an hour (2026-09-12: C's 387 GB filled in under two hours). A node an operator joins with
//! has a small SSD. So this runs on every ConsensusV2 node, once a minute from startup, with no flag:
//!
//! * **The time rule** ([`crate::palw_producer::retained_capture_prune_due_v1`]): an attempt capture
//!   goes at `--palw-attempt-retention-minutes` (60 by default), a free-prompt capture at 48 h and
//!   never while the chain can still ask about it.
//! * **The space rule**: while the volume holding the directory has less free than
//!   [`retention_reserve_bytes_v1`] — `max(8 GiB, 5 % of the volume)` — attempt captures go oldest
//!   first whatever their age, then the panel's foreign copies. A court or an accusation that asks
//!   later is answered from a replay of the block's job (`remade_attempt_capture_v1`), and a seat
//!   re-verifies a foreign claim by replaying it. A free-prompt capture is never taken by this rule:
//!   it is the one copy anywhere. What could not be freed is said, once a minute, by name.
//!
//! A capture is classified by its own bytes — `FPC1` opens the free-prompt envelope, anything else is
//! the attempt lane's bare capture — so a capture whose block the chain has not accepted yet, or never
//! will, is classified the same way as one it holds.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use kaspa_consensus_core::palw_state_v2::{PalwClaimPhaseV2, PalwClaimSourceV2};
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::{
    info,
    task::service::{AsyncService, AsyncServiceFuture},
    trace, warn,
};
use kaspa_hashes::Hash64;

const PALW_RETENTION: &str = "palw-retention";

/// The floor of the space rule: never let the retention volume's free space fall under 8 GiB …
pub(crate) const PALW_RETENTION_MIN_FREE_BYTES: u64 = 8 << 30;
/// … or under this share of the volume, in permille, whichever is larger.
pub(crate) const PALW_RETENTION_MIN_FREE_PERMILLE: u64 = 50;
/// A `.partial` file is a write in progress; one older than this was abandoned (a crash, an ENOSPC)
/// and is removed.
const PALW_RETENTION_PARTIAL_HORIZON: Duration = Duration::from_secs(3600);
/// How often the janitor looks.
const PALW_RETENTION_PERIOD: Duration = Duration::from_secs(60);
/// The free-prompt capture's envelope magic (`palw_fp_capture_encode_v1`).
const FP_CAPTURE_MAGIC: &[u8; 4] = b"FPC1";

/// **The free space the space rule keeps**: `max(8 GiB, 5 % of the volume)`.
pub(crate) fn retention_reserve_bytes_v1(total_bytes: u64) -> u64 {
    PALW_RETENTION_MIN_FREE_BYTES.max(total_bytes / 1000 * PALW_RETENTION_MIN_FREE_PERMILLE)
}

/// What one retained claim — its `.material` and `.answer`, or one foreign copy — costs, and what it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedClaimV1 {
    pub claim: Hash64,
    pub paths: Vec<PathBuf>,
    pub bytes: u64,
    /// The newest of its files' ages (`None` = unreadable, treated as old).
    pub age: Option<Duration>,
    pub kind: RetainedKindV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetainedKindV1 {
    /// This node's own attempt capture: re-made by replaying the block's job when a court asks.
    Attempt,
    /// This node's own free-prompt capture (`FPC1`): the one copy anywhere.
    FreePrompt,
    /// A material this seat verified for another producer (`foreign/`): a copy, re-verified by replay.
    Foreign,
}

/// **Which retained claims go now**, as indexes into `claims`, and how many bytes the space rule still
/// could not free. Pure: `chain` is the claim's source and phase at the tip (`None` = unknown), and
/// `free`/`reserve` are the volume's free bytes and the floor the space rule keeps.
pub(crate) fn retention_prune_plan_v1(
    claims: &[RetainedClaimV1],
    chain: impl Fn(&Hash64) -> Option<(PalwClaimSourceV2, PalwClaimPhaseV2)>,
    attempt_horizon: Duration,
    free: u64,
    reserve: u64,
) -> (Vec<usize>, u64) {
    let mut doomed: Vec<usize> = Vec::new();
    let mut freed = 0u64;
    for (i, c) in claims.iter().enumerate() {
        let due = match c.kind {
            // The attempt lane's bytes say what the claim is, whatever the chain knows yet.
            RetainedKindV1::Attempt => crate::palw_producer::retained_capture_prune_due_v1(
                c.age,
                Some((&PalwClaimSourceV2::Attempt, &PalwClaimPhaseV2::Provisional)),
                attempt_horizon,
            ),
            RetainedKindV1::FreePrompt => {
                let view = chain(&c.claim);
                crate::palw_producer::retained_capture_prune_due_v1(c.age, view.as_ref().map(|(s, p)| (s, p)), attempt_horizon)
            }
            // The panel prunes its foreign copies by age as it writes them; here they only yield space.
            RetainedKindV1::Foreign => false,
        };
        if due {
            doomed.push(i);
            freed = freed.saturating_add(c.bytes);
        }
    }
    // **The space rule**: the oldest re-makeable bytes first — own attempt captures, then foreign copies.
    let mut short = reserve.saturating_sub(free.saturating_add(freed));
    if short > 0 {
        let age = |c: &RetainedClaimV1| c.age.unwrap_or(Duration::MAX);
        for kind in [RetainedKindV1::Attempt, RetainedKindV1::Foreign] {
            let mut order: Vec<usize> = (0..claims.len()).filter(|i| claims[*i].kind == kind && !doomed.contains(i)).collect();
            order.sort_by(|a, b| age(&claims[*b]).cmp(&age(&claims[*a])));
            for i in order {
                if short == 0 {
                    break;
                }
                doomed.push(i);
                short = short.saturating_sub(claims[i].bytes);
            }
        }
    }
    (doomed, short)
}

/// Read `dir` (and `dir/foreign`) into retained claims; `.partial` files older than an hour are
/// returned separately, to be removed.
pub(crate) fn scan_retention_v1(dir: &Path, now: std::time::SystemTime) -> (Vec<RetainedClaimV1>, Vec<PathBuf>) {
    let mut own: std::collections::BTreeMap<Hash64, RetainedClaimV1> = std::collections::BTreeMap::new();
    let mut claims = Vec::new();
    let mut abandoned = Vec::new();
    let age_of = |meta: &std::fs::Metadata| meta.modified().ok().and_then(|t| now.duration_since(t).ok());
    for (sub, foreign) in [(dir.to_path_buf(), false), (dir.join("foreign"), true)] {
        let Ok(entries) = std::fs::read_dir(&sub) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_owned) else { continue };
            let age = age_of(&meta);
            if name.ends_with(".partial") {
                if age.is_none_or(|a| a >= PALW_RETENTION_PARTIAL_HORIZON) {
                    abandoned.push(path);
                }
                continue;
            }
            let (stem, material) = match (name.strip_suffix(".material"), name.strip_suffix(".answer")) {
                (Some(stem), _) => (stem.to_owned(), true),
                (None, Some(stem)) => (stem.to_owned(), false),
                _ => continue,
            };
            let Ok(claim) = stem.parse::<Hash64>() else { continue };
            if foreign {
                claims.push(RetainedClaimV1 { claim, paths: vec![path], bytes: meta.len(), age, kind: RetainedKindV1::Foreign });
                continue;
            }
            let kind = if material && starts_with_fp_magic(&path) { RetainedKindV1::FreePrompt } else { RetainedKindV1::Attempt };
            let row = own.entry(claim).or_insert(RetainedClaimV1 {
                claim,
                paths: Vec::new(),
                bytes: 0,
                age: None,
                kind: RetainedKindV1::Attempt,
            });
            // The claim is as young as its youngest file; an unreadable age reads as old.
            row.age = match (row.paths.is_empty(), row.age, age) {
                (true, _, age) => age,
                (false, Some(a), Some(b)) => Some(a.min(b)),
                (false, Some(a), None) => Some(a),
                (false, None, b) => b,
            };
            row.paths.push(path);
            row.bytes = row.bytes.saturating_add(meta.len());
            if material {
                row.kind = kind;
            }
        }
    }
    // An `.answer` alone is classified by the chain view later only if it matters; its bytes are
    // small, and it goes with the attempt rule by default — as it always went with its material.
    claims.extend(own.into_values());
    (claims, abandoned)
}

fn starts_with_fp_magic(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else { return false };
    let mut head = [0u8; 4];
    file.read_exact(&mut head).is_ok() && &head == FP_CAPTURE_MAGIC
}

/// The free and total bytes of the volume holding `path` — the deepest mount point that contains it.
pub(crate) fn volume_space_v1(path: &Path) -> Option<(u64, u64)> {
    let mut probe = path.to_path_buf();
    while !probe.exists() {
        match probe.parent() {
            Some(parent) if parent != probe => probe = parent.to_path_buf(),
            _ => break,
        }
    }
    let probe = probe.canonicalize().unwrap_or(probe);
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mut best: Option<(usize, u64, u64)> = None;
    for disk in disks.list() {
        let mount = disk.mount_point();
        if probe.starts_with(mount) {
            let len = mount.as_os_str().len();
            if best.is_none_or(|(best_len, _, _)| len > best_len) {
                best = Some((len, disk.available_space(), disk.total_space()));
            }
        }
    }
    best.map(|(_, available, total)| (available, total))
}

/// **The service.** Registered by the daemon on every ConsensusV2 node, whatever else it runs.
pub struct PalwRetentionJanitor {
    consensus_manager: Arc<ConsensusManager>,
    dir: PathBuf,
    attempt_horizon: Duration,
    shutdown: kaspa_utils::triggers::SingleTrigger,
}

impl PalwRetentionJanitor {
    pub fn new(consensus_manager: Arc<ConsensusManager>, dir: PathBuf, attempt_horizon: Duration) -> Self {
        Self { consensus_manager, dir, attempt_horizon, shutdown: kaspa_utils::triggers::SingleTrigger::default() }
    }

    /// One pass: scan, plan, remove. Returns what it removed and what the space rule could not free.
    fn pass(&self) -> (usize, u64, u64) {
        let now = std::time::SystemTime::now();
        let (claims, abandoned) = scan_retention_v1(&self.dir, now);
        for path in &abandoned {
            if let Err(e) = std::fs::remove_file(path) {
                trace!("[{PALW_RETENTION}] cannot remove the abandoned write {}: {e}", path.display());
            }
        }
        if claims.is_empty() {
            return (abandoned.len(), 0, 0);
        }
        let (free, total) = volume_space_v1(&self.dir).unwrap_or((u64::MAX, 0));
        let reserve = if total == 0 { 0 } else { retention_reserve_bytes_v1(total) };
        let session = self.consensus_manager.consensus().unguarded_session();
        let chain = |claim: &Hash64| session.palw_derived_artifacts_v1(*claim).map(|(state, _, _)| (state.source, state.phase));
        let (doomed, short) = retention_prune_plan_v1(&claims, chain, self.attempt_horizon, free, reserve);
        let mut removed = 0usize;
        let mut bytes = 0u64;
        for i in doomed {
            let c = &claims[i];
            for path in &c.paths {
                match std::fs::remove_file(path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => trace!("[{PALW_RETENTION}] cannot prune {}: {e}", path.display()),
                }
            }
            removed += 1;
            bytes = bytes.saturating_add(c.bytes);
        }
        (removed + abandoned.len(), bytes, short)
    }

    async fn worker(self: &Arc<Self>) {
        info!(
            "[{PALW_RETENTION}] pruning {} every {} s: attempt captures after {} min, free-prompt captures after 48 h once \
             the chain is done with them, and oldest re-makeable captures first while the volume has under max(8 GiB, 5 %) free",
            self.dir.display(),
            PALW_RETENTION_PERIOD.as_secs(),
            self.attempt_horizon.as_secs() / 60
        );
        loop {
            let (removed, bytes, short) = self.pass();
            if removed > 0 {
                info!("[{PALW_RETENTION}] pruned {removed} retained file group(s), {} MB", bytes / 1_000_000);
            }
            if short > 0 {
                warn!(
                    "[{PALW_RETENTION}] the retention volume is still {} MB under its free-space floor: what is left is \
                     free-prompt captures the chain can still ask about, or other data on the volume",
                    short / 1_000_000
                );
            }
            tokio::select! {
                _ = tokio::time::sleep(PALW_RETENTION_PERIOD) => {}
                _ = self.shutdown.listener.clone() => break,
            }
        }
    }
}

impl AsyncService for PalwRetentionJanitor {
    fn ident(self: Arc<Self>) -> &'static str {
        PALW_RETENTION
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            self.worker().await;
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", PALW_RETENTION);
        self.shutdown.trigger.trigger();
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            trace!("{} stopped", PALW_RETENTION);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_state_v2::PalwVoidReasonV2 as R;

    const GIB: u64 = 1 << 30;
    const MIN: Duration = Duration::from_secs(60);

    fn claim(n: u64, kind: RetainedKindV1, bytes: u64, age_min: u64) -> RetainedClaimV1 {
        RetainedClaimV1 {
            claim: Hash64::from_u64_word(n),
            paths: vec![PathBuf::from(format!("{n}.material"))],
            bytes,
            age: Some(MIN * age_min as u32),
            kind,
        }
    }

    /// **The floor is 8 GiB, or 5 % of a volume big enough for that to be more.**
    #[test]
    fn the_reserve_is_eight_gib_or_five_percent() {
        assert_eq!(retention_reserve_bytes_v1(0), 8 * GIB);
        assert_eq!(retention_reserve_bytes_v1(100 * GIB), 8 * GIB, "5 % of 100 GiB is under the floor");
        assert_eq!(retention_reserve_bytes_v1(1000 * GIB), 50 * GIB, "5 % of a 1 TiB-ish volume");
    }

    /// **With room to spare only the clock prunes; out of room, the oldest attempt captures go first,
    /// then foreign copies, and a free-prompt capture never goes.**
    #[test]
    fn under_its_floor_the_volume_gives_up_attempt_captures_oldest_first_and_never_a_free_prompt_one() {
        let claims = vec![
            claim(1, RetainedKindV1::Attempt, 10 * GIB, 30), // young
            claim(2, RetainedKindV1::Attempt, 10 * GIB, 50), // older, still young
            claim(3, RetainedKindV1::Attempt, 10 * GIB, 90), // past the 60-minute horizon
            claim(4, RetainedKindV1::FreePrompt, 10 * GIB, 5000),
            claim(5, RetainedKindV1::Foreign, 10 * GIB, 100),
        ];
        let live =
            |_: &Hash64| Some((PalwClaimSourceV2::FreePrompt { quanta: 1, spent: Default::default() }, PalwClaimPhaseV2::Provisional));
        // Plenty of room: the clock alone — only the attempt capture past its hour.
        let (doomed, short) = retention_prune_plan_v1(&claims, live, 60 * MIN, 100 * GIB, 8 * GIB);
        assert_eq!((doomed, short), (vec![2], 0));
        // 5 GiB free against an 8 GiB floor: the clock frees 10, which is enough.
        let (doomed, short) = retention_prune_plan_v1(&claims, live, 60 * MIN, 5 * GIB, 8 * GIB);
        assert_eq!((doomed, short), (vec![2], 0));
        // Nothing free against a 25 GiB floor: the expired one, then the oldest attempt, then the young one.
        let (doomed, short) = retention_prune_plan_v1(&claims, live, 60 * MIN, 0, 25 * GIB);
        assert_eq!((doomed, short), (vec![2, 1, 0], 0));
        // Against 45 GiB: every attempt capture and the foreign copy, and 5 GiB still short — the live
        // free-prompt capture is never taken, however full the disk.
        let (doomed, short) = retention_prune_plan_v1(&claims, live, 60 * MIN, 0, 45 * GIB);
        assert_eq!((doomed, short), (vec![2, 1, 0, 4], 5 * GIB));
        // A free-prompt claim the chain is done with goes on the clock (48 h), not on space.
        let done = |_: &Hash64| {
            Some((
                PalwClaimSourceV2::FreePrompt { quanta: 1, spent: Default::default() },
                PalwClaimPhaseV2::Voided { voided_daa: 1, reason: R::ReceiptTimeout },
            ))
        };
        let (doomed, _) = retention_prune_plan_v1(&claims, done, 60 * MIN, 100 * GIB, 8 * GIB);
        assert_eq!(doomed, vec![2, 3]);
    }

    /// **The directory is read by its bytes and names**: an `FPC1` material is a free-prompt capture, a
    /// bare one an attempt capture with its answer beside it, `foreign/` holds copies, and a `.partial`
    /// older than an hour was abandoned.
    #[test]
    fn the_scan_classifies_by_bytes_and_pairs_material_with_answer() {
        let dir = std::env::temp_dir().join(format!("palw-retention-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("foreign")).unwrap();
        let a = Hash64::from_u64_word(1);
        let f = Hash64::from_u64_word(2);
        let g = Hash64::from_u64_word(3);
        std::fs::write(dir.join(format!("{a}.material")), [1u8, 2, 0, 0, 9]).unwrap();
        std::fs::write(dir.join(format!("{a}.answer")), [7u8; 3]).unwrap();
        std::fs::write(dir.join(format!("{f}.material")), b"FPC1rest").unwrap();
        std::fs::write(dir.join("foreign").join(format!("{g}.material")), [5u8; 6]).unwrap();
        let partial = dir.join(format!("{a}.material.partial"));
        std::fs::write(&partial, [0u8; 2]).unwrap();
        let later = std::time::SystemTime::now() + Duration::from_secs(2 * 3600);
        let (claims, abandoned) = scan_retention_v1(&dir, later);
        assert_eq!(abandoned, vec![partial]);
        let by = |c: &Hash64| claims.iter().find(|x| &x.claim == c).cloned().unwrap();
        assert_eq!((by(&a).kind, by(&a).bytes, by(&a).paths.len()), (RetainedKindV1::Attempt, 8, 2));
        assert_eq!(by(&f).kind, RetainedKindV1::FreePrompt);
        assert_eq!(by(&g).kind, RetainedKindV1::Foreign);
        assert!(by(&a).age.is_some_and(|age| age >= Duration::from_secs(7000)), "two hours after the writes");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
