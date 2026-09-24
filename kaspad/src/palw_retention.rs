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
//!   never while the chain can still ask about it — past `palw_rcore_plus` that includes a `Final`
//!   claim a data-availability session can still open on (ADR-0152 DA-8; P2-7), until its
//!   `trace_retention_daa`.
//! * **The space rule**: while the volume holding the directory has less free than
//!   [`retention_reserve_bytes_v1`] — `max(8 GiB, 5 % of the volume)` — attempt captures go oldest
//!   first whatever their age, then the panel's foreign copies — those an R-core session can still
//!   demand a unit of last, and of those the free-prompt ones last of all (a covering signer answers
//!   from its copy, X7, and a free-prompt job is not chain data: [`retained_foreign_rank_v1`]); one of
//!   those taken is said by name, as a liability. A court or an accusation
//!   that asks later is answered from a replay of the block's job (`remade_attempt_capture_v1`), and a seat
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

/// **The chain's view of a retained claim at the tip**, as the prune reads it: its source and phase,
/// and whether R-core's data-availability court can still demand a unit of it — `da_owed`, past
/// `palw_rcore_plus` only: [`kaspa_consensus_core::palw_da_rcore_v1::palw_da_material_owed_v1`]
/// (ADR-0152 DA-8; Phase 2, P2-7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedChainViewV1 {
    pub source: PalwClaimSourceV2,
    pub phase: PalwClaimPhaseV2,
    pub da_owed: bool,
}

/// **Which retained claims go now**, as indexes into `claims`, and how many bytes the space rule still
/// could not free. Pure: `chain` is the claim's view at the tip (`None` = unknown), and
/// `free`/`reserve` are the volume's free bytes and the floor the space rule keeps.
///
/// **P2-7 (ADR-0152 DA-4, DA-7, X7):** past `palw_rcore_plus` a session opens on a `Final` claim too
/// (`FinalRow`: a default is S3, the row burned and `min(25% · C, 3 G)`), so a free-prompt capture —
/// the one copy anywhere — is never due while `da_owed`, whatever its phase; and under the space
/// rule the foreign copies a session can still demand go after every other foreign copy, the
/// free-prompt ones last of all ([`retained_foreign_rank_v1`]), because a covering signer answers from
/// its copy and a free-prompt job is not chain data.
pub(crate) fn retention_prune_plan_v1(
    claims: &[RetainedClaimV1],
    chain: impl Fn(&Hash64) -> Option<RetainedChainViewV1>,
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
                !view.as_ref().is_some_and(|view| view.da_owed)
                    && crate::palw_producer::retained_capture_prune_due_v1(
                        c.age,
                        view.as_ref().map(|view| (&view.source, &view.phase)),
                        attempt_horizon,
                    )
            }
            // The panel prunes its foreign copies by age as it writes them; here they only yield space.
            RetainedKindV1::Foreign => false,
        };
        if due {
            doomed.push(i);
            freed = freed.saturating_add(c.bytes);
        }
    }
    // **The space rule**: the oldest re-makeable bytes first — own attempt captures, then foreign copies,
    // those an R-core session can still demand last, the free-prompt ones last of all (P2-7).
    let mut short = reserve.saturating_sub(free.saturating_add(freed));
    if short > 0 {
        let age = |c: &RetainedClaimV1| c.age.unwrap_or(Duration::MAX);
        let rank: Vec<u8> = claims
            .iter()
            .map(|c| if c.kind == RetainedKindV1::Foreign { retained_foreign_rank_v1(chain(&c.claim).as_ref()) } else { 0 })
            .collect();
        for kind in [RetainedKindV1::Attempt, RetainedKindV1::Foreign] {
            let mut order: Vec<usize> = (0..claims.len()).filter(|i| claims[*i].kind == kind && !doomed.contains(i)).collect();
            order.sort_by(|a, b| (rank[*a], age(&claims[*b])).cmp(&(rank[*b], age(&claims[*a]))));
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

/// **How late the space rule takes a foreign copy** (P2-7; lower goes first): `0`, nothing an R-core
/// session can still demand of it; `1`, a session can (`da_owed`) and it is an attempt claim's, which
/// any node re-makes from its block; `2`, a session can and it is a free-prompt claim's, whose job is
/// not chain data — for a full-mask signer the one copy it answers from, so taking it is an S4 the
/// signer could not avoid if the producer withholds (the P2-7 review's LOW).
pub(crate) fn retained_foreign_rank_v1(view: Option<&RetainedChainViewV1>) -> u8 {
    match view {
        Some(view) if view.da_owed && matches!(view.source, PalwClaimSourceV2::FreePrompt { .. }) => 2,
        Some(view) if view.da_owed => 1,
        _ => 0,
    }
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
    /// `Params::palw_rcore_plus` (`palw_rcore_plus_fence`): past it the DA court's window is read
    /// off the claim's retention, not its phase (P2-7).
    rcore_plus: Option<kaspa_consensus_core::config::params::ForkActivation>,
    shutdown: kaspa_utils::triggers::SingleTrigger,
}

impl PalwRetentionJanitor {
    pub fn new(
        consensus_manager: Arc<ConsensusManager>,
        dir: PathBuf,
        attempt_horizon: Duration,
        rcore_plus: Option<kaspa_consensus_core::config::params::ForkActivation>,
    ) -> Self {
        Self { consensus_manager, dir, attempt_horizon, rcore_plus, shutdown: kaspa_utils::triggers::SingleTrigger::default() }
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
        let now_daa = session.get_virtual_daa_score();
        let rcore = self.rcore_plus.is_some_and(|fence| fence.is_active(now_daa));
        let chain = |claim: &Hash64| {
            session.palw_derived_artifacts_v1(*claim).map(|(state, _, _)| RetainedChainViewV1 {
                da_owed: rcore && kaspa_consensus_core::palw_da_rcore_v1::palw_da_material_owed_v1(&state, now_daa),
                source: state.source,
                phase: state.phase,
            })
        };
        let (doomed, short) = retention_prune_plan_v1(&claims, &chain, self.attempt_horizon, free, reserve);
        for i in &doomed {
            let c = &claims[*i];
            if c.kind == RetainedKindV1::Foreign && retained_foreign_rank_v1(chain(&c.claim).as_ref()) == 2 {
                warn!(
                    "[{PALW_RETENTION}] claim {}: the volume is under its free-space floor, so its free-prompt copy goes \
                     while a data-availability session can still demand a unit of it — if this node holds a full-mask Valid \
                     lock on it and the producer withholds, it cannot answer and is charged S4 (ADR-0152 X7)",
                    c.claim
                );
            }
        }
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
        let live = |_: &Hash64| {
            Some(RetainedChainViewV1 {
                source: PalwClaimSourceV2::FreePrompt { quanta: 1, spent: Default::default() },
                phase: PalwClaimPhaseV2::Provisional,
                da_owed: false,
            })
        };
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
            Some(RetainedChainViewV1 {
                source: PalwClaimSourceV2::FreePrompt { quanta: 1, spent: Default::default() },
                phase: PalwClaimPhaseV2::Voided { voided_daa: 1, reason: R::ReceiptTimeout },
                da_owed: false,
            })
        };
        let (doomed, _) = retention_prune_plan_v1(&claims, done, 60 * MIN, 100 * GIB, 8 * GIB);
        assert_eq!(doomed, vec![2, 3]);
    }

    /// **P2-7 (ADR-0152 DA-8): what R-core's court can still ask for is kept.** Past `palw_rcore_plus`
    /// a `Final` free-prompt claim is still accused (`FinalRow`, whose default is S3), so its capture —
    /// past the 48 h clock and `Final`, which below the fence releases it — stays while `da_owed`, and
    /// goes on the clock once the claim's retention has passed. Under the space rule the foreign copy
    /// a session can still demand goes after the other foreign copies, even when it is the older.
    #[test]
    fn p2_7_a_capture_the_da_court_can_still_ask_for_is_kept() {
        let fp = PalwClaimSourceV2::FreePrompt { quanta: 1, spent: Default::default() };
        let view =
            |da_owed: bool| RetainedChainViewV1 { source: fp.clone(), phase: PalwClaimPhaseV2::Final { final_daa: 9 }, da_owed };
        let claims = vec![claim(1, RetainedKindV1::FreePrompt, GIB, 5000)];
        let (doomed, _) = retention_prune_plan_v1(&claims, |_: &Hash64| Some(view(true)), 60 * MIN, 100 * GIB, 8 * GIB);
        assert!(doomed.is_empty(), "a Final claim a session can still open on keeps its capture");
        let (doomed, _) = retention_prune_plan_v1(&claims, |_: &Hash64| Some(view(false)), 60 * MIN, 100 * GIB, 8 * GIB);
        assert_eq!(doomed, vec![0], "past its retention (or below the fence) Final releases it at 48 h, as before");
        // The space rule: the owed foreign copy (claim 3, the older) goes after the other one.
        let foreign = vec![claim(2, RetainedKindV1::Foreign, 10 * GIB, 100), claim(3, RetainedKindV1::Foreign, 10 * GIB, 900)];
        let owed = |claim: &Hash64| Some(view(*claim == Hash64::from_u64_word(3)));
        let (doomed, short) = retention_prune_plan_v1(&foreign, owed, 60 * MIN, 0, 5 * GIB);
        assert_eq!((doomed, short), (vec![0], 0), "the unowed copy frees the floor alone");
        let (doomed, short) = retention_prune_plan_v1(&foreign, owed, 60 * MIN, 0, 15 * GIB);
        assert_eq!((doomed, short), (vec![0, 1], 0), "and the owed copy only when that is not enough");
        let unowed = |_: &Hash64| Some(view(false));
        let (doomed, _) = retention_prune_plan_v1(&foreign, unowed, 60 * MIN, 0, 5 * GIB);
        assert_eq!(doomed, vec![1], "with nothing owed, oldest first as before");
        // Among the owed copies, an attempt claim's (re-made from its block by any node) goes before a
        // free-prompt claim's (its job is not chain data: a full-mask signer's one copy), even when the
        // free-prompt copy is the older (the P2-7 review's LOW).
        let attempt = |da_owed: bool| RetainedChainViewV1 {
            source: PalwClaimSourceV2::Attempt,
            phase: PalwClaimPhaseV2::Final { final_daa: 9 },
            da_owed,
        };
        let mixed = vec![claim(4, RetainedKindV1::Foreign, 10 * GIB, 900), claim(5, RetainedKindV1::Foreign, 10 * GIB, 100)];
        let owed_both = |claim: &Hash64| Some(if *claim == Hash64::from_u64_word(4) { view(true) } else { attempt(true) });
        let (doomed, short) = retention_prune_plan_v1(&mixed, owed_both, 60 * MIN, 0, 5 * GIB);
        assert_eq!((doomed, short), (vec![1], 0), "the owed attempt copy goes first, though it is the younger");
        let (doomed, _) = retention_prune_plan_v1(&mixed, owed_both, 60 * MIN, 0, 15 * GIB);
        assert_eq!(doomed, vec![1, 0], "and the owed free-prompt copy only when nothing else frees the floor");
        assert_eq!(
            [
                retained_foreign_rank_v1(None),
                retained_foreign_rank_v1(Some(&view(false))),
                retained_foreign_rank_v1(Some(&attempt(false))),
                retained_foreign_rank_v1(Some(&attempt(true))),
                retained_foreign_rank_v1(Some(&view(true))),
            ],
            [0, 0, 0, 1, 2]
        );
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
