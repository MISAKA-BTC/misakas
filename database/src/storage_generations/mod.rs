//! **Generation-numbered node-local storage with an atomic switch journal** (G14 storage, IBD lane;
//! design: `docs/design/palw/storage/ibd-generations.md`).
//!
//! A *generation* is one directory `gen-NNNNNN/` holding a complete, self-sufficient copy of some node-local state (the
//! consensus DB a pruned IBD builds, an imported pruning-point state, …). At most TWO generations exist at any time:
//! the **active** one and either the **previous** one (kept after a switch so a reorg can step back across it) or a
//! **staging** one (being built). Starting a new staging generation first retires the previous one — the losing
//! generation is cleaned up BEFORE new space is reserved, so the peak is two generations, never three.
//!
//! The only durable record of which directory is which is the journal file `GENERATIONS`, replaced atomically:
//! write `GENERATIONS.new`, fsync it, rename it over `GENERATIONS`, fsync the directory. A crash leaves either the old
//! or the new journal, never a mix.
//!
//! A generation is **verified** only when its directory holds a `VERIFIED` marker written (same write/fsync/rename
//! discipline) AFTER the caller's verification finished, and the marker refuses to exist without a whole-state check:
//! receiving a root alone is never "verified". The journal can name a generation `active` only if it is verified, so
//! recovery can always tell which generation is verified:
//!
//! * `GENERATIONS.new` left over → the switch it describes never happened; deleted.
//! * staging in the journal → never promoted (the decision to switch was not durable); deleted.
//! * active without a valid marker (only possible after disk corruption) → previous, if it is verified; otherwise
//!   [`RecoveryReport::active`] is `None` and the caller must resync — it is never handed an unverified state.
//! * previous whose directory is gone (crash inside retirement) → forgotten.
//! * any other `gen-*` directory → a losing generation; deleted.
//!
//! [`CrashPoint`] lets tests stop a step half-way, exactly where a process kill would, and reopen.

pub mod reserve;
pub mod spill;
pub mod undo;

#[cfg(test)]
mod tests;

use reserve::{StoragePurpose, StorageReservation, StorageReserver};
use std::{
    fmt, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

pub const JOURNAL_FILE: &str = "GENERATIONS";
pub const JOURNAL_NEW_FILE: &str = "GENERATIONS.new";
pub const VERIFIED_FILE: &str = "VERIFIED";
const VERIFIED_NEW_FILE: &str = "VERIFIED.new";
const GEN_PREFIX: &str = "gen-";
const JOURNAL_MAGIC: &str = "misaka-generations v1";
const VERIFIED_MAGIC: &str = "misaka-generation-verified v1";

/// Where a test may stop a step, as a process kill would.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CrashPoint {
    /// begin_staging: after the losing generation was removed, before the reservation.
    StagingAfterCleanup,
    /// begin_staging: after `GENERATIONS.new` (naming the staging) was written and fsynced, before the rename.
    StagingAfterJournalTmp,
    /// begin_staging: after the rename, before the directory exists.
    StagingAfterJournal,
    /// begin_staging: after the directory exists (the caller would now fill it).
    StagingAfterDir,
    /// mark_verified: after `VERIFIED.new` was written and fsynced, before the rename.
    VerifyAfterMarkerTmp,
    /// mark_verified: after the marker rename (verified, not yet switched).
    VerifyAfterMarker,
    /// switch: after `GENERATIONS.new` (naming the new active) was written and fsynced, before the rename.
    SwitchAfterJournalTmp,
    /// switch: after the rename, before the directory fsync.
    SwitchAfterJournal,
    /// retire_previous / roll_back: after the losing directory was removed, before the journal forgot it.
    RetireAfterRemove,
    /// roll_back_to_previous: after `GENERATIONS.new` was written, before the rename.
    RollBackAfterJournalTmp,
}

#[derive(Debug, thiserror::Error)]
pub enum GenerationError {
    #[error("generation storage I/O: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Refused(#[from] reserve::ReserveRefused),
    #[error("generation journal is corrupt ({0}); verified generations on disk: {1:?} — refusing to guess which was active")]
    JournalCorrupt(String, Vec<u64>),
    #[error("a root alone is never verified: the marker for generation {0} needs a whole-state check")]
    RootAloneIsNotVerified(u64),
    #[error("generation {0} is not the staging generation")]
    NotStaging(u64),
    #[error("generation {0} is not verified; it cannot become active")]
    NotVerified(u64),
    #[error("no verified previous generation to roll back to")]
    NoPrevious,
    #[error("simulated crash at {0:?}")]
    SimulatedCrash(CrashPoint),
}

pub type GenerationResult<T> = Result<T, GenerationError>;

/// What the verifier attests. `whole_state_checked` must be `true`: the IBD path sets it only after the full-row
/// stream verification AND the whole-state consistency checks (or a replay from a verified base) passed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedMarker {
    /// Free text naming what was verified, e.g. `palw-pruning-point <hash>`. Single line.
    pub subject: String,
    /// Hex of the root the content was checked against (the header-committed one, never a peer's word).
    pub root_hex: String,
    pub whole_state_checked: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Generation {
    pub number: u64,
    pub path: PathBuf,
}

/// A staging generation being built. Holds its space reservation until it is switched in or abandoned.
#[derive(Debug)]
pub struct StagingGeneration {
    pub number: u64,
    pub path: PathBuf,
    reservation: Option<StorageReservation>,
}

impl StagingGeneration {
    pub fn reserved_bytes(&self) -> u64 {
        self.reservation.as_ref().map_or(0, StorageReservation::bytes)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Journal {
    seq: u64,
    next: u64,
    active: Option<u64>,
    previous: Option<u64>,
    staging: Option<u64>,
}

/// What [`GenerationStore::open`] found and did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecoveryReport {
    /// The verified generation now active, if any. `None` = nothing verified survives: resync.
    pub active: Option<u64>,
    pub previous: Option<u64>,
    /// Generation directories deleted as losers (stale staging, strays, an unverified active).
    pub removed: Vec<u64>,
    /// A `GENERATIONS.new` was found and discarded.
    pub discarded_uncommitted_journal: bool,
    /// The journal's active generation failed its marker check and recovery fell back to the previous one.
    pub fell_back_to_previous: bool,
}

pub struct GenerationStore {
    root: PathBuf,
    journal: Journal,
    reserver: Arc<dyn StorageReserver>,
    crash_at: Option<CrashPoint>,
}

impl fmt::Debug for GenerationStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GenerationStore").field("root", &self.root).field("journal", &self.journal).finish()
    }
}

fn gen_dir_name(n: u64) -> String {
    format!("{GEN_PREFIX}{n:06}")
}

fn parse_gen_dir(name: &str) -> Option<u64> {
    name.strip_prefix(GEN_PREFIX)?.parse().ok()
}

fn fsync_dir(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        fs::File::open(dir)?.sync_all()?;
    }
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}

fn opt(v: Option<u64>) -> String {
    v.map_or_else(|| "-".to_string(), |n| n.to_string())
}

fn parse_opt(v: &str) -> Option<Option<u64>> {
    if v == "-" { Some(None) } else { v.parse().ok().map(Some) }
}

/// `key=value` lines with a trailing `checksum=` over everything above it.
fn render_checked(magic: &str, fields: &[(&str, String)]) -> String {
    let mut body = format!("{magic}\n");
    for (k, v) in fields {
        body.push_str(&format!("{k}={v}\n"));
    }
    let sum = undo::fnv1a64(body.as_bytes());
    format!("{body}checksum={sum:016x}\n")
}

fn parse_checked(magic: &str, text: &str) -> Result<Vec<(String, String)>, String> {
    let Some(idx) = text.rfind("checksum=") else { return Err("no checksum".into()) };
    let (body, tail) = text.split_at(idx);
    let sum = tail.trim_end().strip_prefix("checksum=").ok_or("bad checksum line")?;
    if u64::from_str_radix(sum, 16).ok() != Some(undo::fnv1a64(body.as_bytes())) {
        return Err("checksum mismatch".into());
    }
    let mut lines = body.lines();
    if lines.next() != Some(magic) {
        return Err("bad magic".into());
    }
    lines.map(|l| l.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())).ok_or_else(|| format!("bad line {l:?}"))).collect()
}

fn field<'a>(fields: &'a [(String, String)], key: &str) -> Result<&'a str, String> {
    fields.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str()).ok_or_else(|| format!("missing {key}"))
}

impl Journal {
    fn render(&self) -> String {
        render_checked(
            JOURNAL_MAGIC,
            &[
                ("seq", self.seq.to_string()),
                ("next", self.next.to_string()),
                ("active", opt(self.active)),
                ("previous", opt(self.previous)),
                ("staging", opt(self.staging)),
            ],
        )
    }
    fn parse(text: &str) -> Result<Self, String> {
        let f = parse_checked(JOURNAL_MAGIC, text)?;
        let num = |k| field(&f, k)?.parse::<u64>().map_err(|e| format!("{k}: {e}"));
        let o = |k| parse_opt(field(&f, k)?).ok_or_else(|| format!("bad {k}"));
        Ok(Self { seq: num("seq")?, next: num("next")?, active: o("active")?, previous: o("previous")?, staging: o("staging")? })
    }
}

/// Read and check a generation's marker. `Ok(None)` = not verified (absent, torn, or naming another generation).
pub fn read_verified_marker(gen_dir: &Path, number: u64) -> io::Result<Option<VerifiedMarker>> {
    let text = match fs::read_to_string(gen_dir.join(VERIFIED_FILE)) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let Ok(f) = parse_checked(VERIFIED_MAGIC, &text) else { return Ok(None) };
    let get = |k| field(&f, k).ok().map(str::to_string);
    if get("gen").and_then(|g| g.parse::<u64>().ok()) != Some(number) || get("whole_state_checked").as_deref() != Some("1") {
        return Ok(None);
    }
    Ok(Some(VerifiedMarker {
        subject: get("subject").unwrap_or_default(),
        root_hex: get("root").unwrap_or_default(),
        whole_state_checked: true,
    }))
}

impl GenerationStore {
    /// Open `root`, recover from any interrupted step, and delete every losing generation.
    pub fn open(root: impl AsRef<Path>, reserver: Arc<dyn StorageReserver>) -> GenerationResult<(Self, RecoveryReport)> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)?;
        let mut report = RecoveryReport::default();
        let new = root.join(JOURNAL_NEW_FILE);
        if new.exists() {
            fs::remove_file(&new)?;
            report.discarded_uncommitted_journal = true;
        }
        let on_disk: Vec<u64> = Self::scan(&root)?;
        let mut journal = match fs::read_to_string(root.join(JOURNAL_FILE)) {
            Ok(text) => Journal::parse(&text).map_err(|why| {
                let verified = on_disk
                    .iter()
                    .copied()
                    .filter(|n| read_verified_marker(&root.join(gen_dir_name(*n)), *n).ok().flatten().is_some());
                GenerationError::JournalCorrupt(why, verified.collect())
            })?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                Journal { next: on_disk.iter().max().map_or(1, |m| m + 1), ..Default::default() }
            }
            Err(e) => return Err(e.into()),
        };
        let mut store = Self { root, journal: journal.clone(), reserver, crash_at: None };
        let verified = |n: u64| read_verified_marker(&store.gen_path(n), n).ok().flatten().is_some();
        // A staging generation was never switched in: the decision to adopt it was not durable.
        journal.staging = None;
        if let Some(p) = journal.previous
            && (!store.gen_path(p).exists() || !verified(p))
        {
            journal.previous = None;
        }
        if let Some(a) = journal.active
            && !verified(a)
        {
            journal.active = journal.previous.take();
            report.fell_back_to_previous = journal.active.is_some();
        }
        journal.next = journal.next.max(on_disk.iter().max().map_or(1, |m| m + 1));
        if journal != store.journal {
            journal.seq += 1;
            store.write_journal(&journal, None)?;
        }
        store.journal = journal;
        for n in on_disk {
            if Some(n) != store.journal.active && Some(n) != store.journal.previous {
                fs::remove_dir_all(store.gen_path(n))?;
                report.removed.push(n);
            }
        }
        fsync_dir(&store.root)?;
        report.active = store.journal.active;
        report.previous = store.journal.previous;
        Ok((store, report))
    }

    fn scan(root: &Path) -> io::Result<Vec<u64>> {
        let mut out = Vec::new();
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            if entry.file_type()?.is_dir()
                && let Some(n) = parse_gen_dir(&entry.file_name().to_string_lossy())
            {
                out.push(n);
            }
        }
        out.sort_unstable();
        Ok(out)
    }

    /// Stop the next matching step half-way, as a process kill would. Test hook; never set in production.
    #[doc(hidden)]
    pub fn arm_crash_point(&mut self, point: CrashPoint) {
        self.crash_at = Some(point);
    }

    fn crash(&self, point: CrashPoint) -> GenerationResult<()> {
        if self.crash_at == Some(point) { Err(GenerationError::SimulatedCrash(point)) } else { Ok(()) }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    fn gen_path(&self, n: u64) -> PathBuf {
        self.root.join(gen_dir_name(n))
    }
    fn generation(&self, n: Option<u64>) -> Option<Generation> {
        n.map(|number| Generation { number, path: self.gen_path(number) })
    }
    pub fn active(&self) -> Option<Generation> {
        self.generation(self.journal.active)
    }
    pub fn previous(&self) -> Option<Generation> {
        self.generation(self.journal.previous)
    }
    pub fn staging_number(&self) -> Option<u64> {
        self.journal.staging
    }
    pub fn is_verified(&self, n: u64) -> bool {
        read_verified_marker(&self.gen_path(n), n).ok().flatten().is_some()
    }
    /// Generation directories currently on disk.
    pub fn generations_on_disk(&self) -> io::Result<Vec<u64>> {
        Self::scan(&self.root)
    }

    /// The write-ahead discipline every journal change goes through.
    fn write_journal(&self, next: &Journal, crash_tmp: Option<CrashPoint>) -> GenerationResult<()> {
        let new = self.root.join(JOURNAL_NEW_FILE);
        {
            let mut f = fs::OpenOptions::new().write(true).create(true).truncate(true).open(&new)?;
            f.write_all(next.render().as_bytes())?;
            f.sync_all()?;
        }
        if let Some(p) = crash_tmp {
            self.crash(p)?;
        }
        fs::rename(&new, self.root.join(JOURNAL_FILE))?;
        Ok(())
    }

    fn commit(&mut self, next: Journal, crash_tmp: CrashPoint, crash_after_rename: Option<CrashPoint>) -> GenerationResult<()> {
        let mut next = next;
        next.seq = self.journal.seq + 1;
        self.write_journal(&next, Some(crash_tmp))?;
        self.journal = next;
        if let Some(p) = crash_after_rename {
            self.crash(p)?;
        }
        fsync_dir(&self.root)?;
        Ok(())
    }

    fn commit_quiet(&mut self, next: Journal) -> GenerationResult<()> {
        let mut next = next;
        next.seq = self.journal.seq + 1;
        self.write_journal(&next, None)?;
        self.journal = next;
        fsync_dir(&self.root)?;
        Ok(())
    }

    /// Forget and delete the previous generation (the losing side of the last switch). Journal first, then the
    /// directory: a crash in between leaves an unreferenced directory that recovery deletes.
    pub fn retire_previous(&mut self) -> GenerationResult<()> {
        let Some(p) = self.journal.previous else { return Ok(()) };
        let mut next = self.journal.clone();
        next.previous = None;
        self.commit_quiet(next)?;
        let dir = self.gen_path(p);
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        self.crash(CrashPoint::RetireAfterRemove)?;
        fsync_dir(&self.root)?;
        Ok(())
    }

    /// Drop an unfinished staging generation (verification failed, the switch was refused, the IBD was cancelled).
    pub fn abandon_staging(&mut self, staging: StagingGeneration) -> GenerationResult<()> {
        if self.journal.staging != Some(staging.number) {
            return Err(GenerationError::NotStaging(staging.number));
        }
        let mut next = self.journal.clone();
        next.staging = None;
        self.commit_quiet(next)?;
        if staging.path.exists() {
            fs::remove_dir_all(&staging.path)?;
        }
        fsync_dir(&self.root)?;
        Ok(())
    }

    /// Start building a new generation. Order: retire the losing generations (stale staging, previous), THEN reserve
    /// `estimate_bytes`, THEN write the journal naming the staging generation, THEN create its directory.
    pub fn begin_staging(&mut self, estimate_bytes: u64) -> GenerationResult<StagingGeneration> {
        if let Some(s) = self.journal.staging {
            let mut next = self.journal.clone();
            next.staging = None;
            self.commit_quiet(next)?;
            let dir = self.gen_path(s);
            if dir.exists() {
                fs::remove_dir_all(dir)?;
            }
        }
        self.retire_previous()?;
        for n in self.generations_on_disk()? {
            if Some(n) != self.journal.active {
                fs::remove_dir_all(self.gen_path(n))?;
            }
        }
        self.crash(CrashPoint::StagingAfterCleanup)?;
        let reservation = StorageReservation::acquire(&self.reserver, StoragePurpose::StagingGeneration, estimate_bytes)?;
        let number = self.journal.next;
        let mut next = self.journal.clone();
        next.staging = Some(number);
        next.next = number + 1;
        self.commit(next, CrashPoint::StagingAfterJournalTmp, Some(CrashPoint::StagingAfterJournal))?;
        let path = self.gen_path(number);
        fs::create_dir(&path)?;
        fsync_dir(&self.root)?;
        self.crash(CrashPoint::StagingAfterDir)?;
        Ok(StagingGeneration { number, path, reservation: Some(reservation) })
    }

    /// Record that the staging generation's content was verified. Refuses a marker without a whole-state check.
    pub fn mark_verified(&mut self, staging: &StagingGeneration, marker: &VerifiedMarker) -> GenerationResult<()> {
        if self.journal.staging != Some(staging.number) {
            return Err(GenerationError::NotStaging(staging.number));
        }
        if !marker.whole_state_checked {
            return Err(GenerationError::RootAloneIsNotVerified(staging.number));
        }
        let one_line = |s: &str| s.replace(['\n', '\r'], " ");
        let text = render_checked(
            VERIFIED_MAGIC,
            &[
                ("gen", staging.number.to_string()),
                ("subject", one_line(&marker.subject)),
                ("root", one_line(&marker.root_hex)),
                ("whole_state_checked", "1".to_string()),
            ],
        );
        let tmp = staging.path.join(VERIFIED_NEW_FILE);
        {
            let mut f = fs::OpenOptions::new().write(true).create(true).truncate(true).open(&tmp)?;
            f.write_all(text.as_bytes())?;
            f.sync_all()?;
        }
        self.crash(CrashPoint::VerifyAfterMarkerTmp)?;
        fs::rename(&tmp, staging.path.join(VERIFIED_FILE))?;
        fsync_dir(&staging.path)?;
        self.crash(CrashPoint::VerifyAfterMarker)?;
        Ok(())
    }

    /// The atomic switch: the verified staging generation becomes active and the old active becomes previous. Its space
    /// reservation is released — the bytes are now real usage the capacity manager measures.
    pub fn switch_to(&mut self, mut staging: StagingGeneration) -> GenerationResult<Generation> {
        if self.journal.staging != Some(staging.number) {
            return Err(GenerationError::NotStaging(staging.number));
        }
        if !self.is_verified(staging.number) {
            return Err(GenerationError::NotVerified(staging.number));
        }
        let mut next = self.journal.clone();
        next.previous = self.journal.active;
        next.active = Some(staging.number);
        next.staging = None;
        self.commit(next, CrashPoint::SwitchAfterJournalTmp, Some(CrashPoint::SwitchAfterJournal))?;
        drop(staging.reservation.take());
        Ok(Generation { number: staging.number, path: staging.path.clone() })
    }

    /// Reorg across a switch: make the retained, verified previous generation active again and delete the one that
    /// lost. Refused (`NoPrevious`) once the previous generation was retired — the caller must then resync; it never
    /// treats the missing generation as a verdict on any block.
    pub fn roll_back_to_previous(&mut self) -> GenerationResult<Generation> {
        let Some(p) = self.journal.previous else { return Err(GenerationError::NoPrevious) };
        if !self.is_verified(p) {
            return Err(GenerationError::NotVerified(p));
        }
        let loser = self.journal.active;
        let mut next = self.journal.clone();
        next.active = Some(p);
        next.previous = None;
        self.commit(next, CrashPoint::RollBackAfterJournalTmp, None)?;
        if let Some(l) = loser {
            let dir = self.gen_path(l);
            if dir.exists() {
                fs::remove_dir_all(dir)?;
            }
            self.crash(CrashPoint::RetireAfterRemove)?;
            fsync_dir(&self.root)?;
        }
        Ok(Generation { number: p, path: self.gen_path(p) })
    }
}
