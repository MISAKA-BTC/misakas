//! **Bounded IBD spill files inside the managed storage area.**
//!
//! G14 storage budget: "IBD spill 384 MiB × 2". A [`SpillArea`] owns one directory inside the managed area, hands out
//! at most `max_files` files at once, caps each at `per_file_cap` bytes (a write past the cap FAILS — it is never
//! truncated or wrapped), reserves each file's full cap through the [`StorageReserver`] before the file exists, and
//! removes every file on drop. Leftovers from a crashed process are swept when the area is opened.
//!
//! Default figures are the budget's: 384 MiB per file, two files (the same two-session policy the protocol-106/107 PALW
//! snapshot receiver already enforces with its global semaphore). Tests run the identical logic under a scaled-down cap.

use super::reserve::{StoragePurpose, StorageReservation, StorageReserver};
use parking_lot::Mutex;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

/// 384 MiB — the per-file IBD spill cap of the G14 budget (equal to `PALW_PRUNING_STATE_MAX_BYTES`).
pub const DEFAULT_SPILL_FILE_CAP: u64 = 384 << 20;
/// Two concurrent spill files (receive + serve, or two receives).
pub const DEFAULT_SPILL_FILES: usize = 2;
const SPILL_PREFIX: &str = "spill-";
const SPILL_SUFFIX: &str = ".tmp";

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SpillAccounting {
    pub files: usize,
    pub bytes: u64,
    /// The largest `bytes` ever observed across all live files of this area — what the peak-bytes tests assert.
    pub high_water: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum SpillError {
    #[error("all {0} spill files are in use")]
    Busy(usize),
    #[error(transparent)]
    Refused(#[from] super::reserve::ReserveRefused),
    #[error("spill I/O: {0}")]
    Io(#[from] io::Error),
}

/// The managed spill directory.
#[derive(Clone)]
pub struct SpillArea {
    dir: PathBuf,
    per_file_cap: u64,
    max_files: usize,
    accounting: Arc<Mutex<SpillAccounting>>,
    reserver: Arc<dyn StorageReserver>,
}

impl SpillArea {
    /// Create (if needed) and sweep `dir`. Any `spill-*.tmp` found here belongs to a process that died; it is never
    /// state, so it is deleted before a byte of new spill is reserved.
    pub fn open(dir: impl AsRef<Path>, per_file_cap: u64, max_files: usize, reserver: Arc<dyn StorageReserver>) -> io::Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir)?;
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(SPILL_PREFIX) && name.ends_with(SPILL_SUFFIX) {
                fs::remove_file(entry.path())?;
            }
        }
        Ok(Self { dir, per_file_cap, max_files, accounting: Default::default(), reserver })
    }

    pub fn with_defaults(dir: impl AsRef<Path>, reserver: Arc<dyn StorageReserver>) -> io::Result<Self> {
        Self::open(dir, DEFAULT_SPILL_FILE_CAP, DEFAULT_SPILL_FILES, reserver)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
    pub fn per_file_cap(&self) -> u64 {
        self.per_file_cap
    }
    pub fn max_files(&self) -> usize {
        self.max_files
    }
    /// The area's worst case: `max_files × per_file_cap`.
    pub fn max_bytes(&self) -> u64 {
        self.per_file_cap.saturating_mul(self.max_files as u64)
    }
    pub fn accounting(&self) -> SpillAccounting {
        *self.accounting.lock()
    }

    /// One spill file. Reserves its full cap first; refuses when `max_files` are live.
    pub fn acquire(&self) -> Result<SpillFile, SpillError> {
        {
            let mut acc = self.accounting.lock();
            if acc.files >= self.max_files {
                return Err(SpillError::Busy(self.max_files));
            }
            acc.files += 1; // claim the slot under the lock; undone below on any failure
        }
        let release_slot = |acc: &Mutex<SpillAccounting>| acc.lock().files -= 1;
        let reservation = match StorageReservation::acquire(&self.reserver, StoragePurpose::IbdSpill, self.per_file_cap) {
            Ok(r) => r,
            Err(e) => {
                release_slot(&self.accounting);
                return Err(e.into());
            }
        };
        let path = self.dir.join(format!("{SPILL_PREFIX}{:032x}{SPILL_SUFFIX}", rand::random::<u128>()));
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = match options.open(&path) {
            Ok(f) => f,
            Err(e) => {
                release_slot(&self.accounting);
                return Err(e.into());
            }
        };
        Ok(SpillFile { file, path, cap: self.per_file_cap, written: 0, accounting: self.accounting.clone(), _reservation: reservation })
    }
}

/// A capped, private spill file. Writing past the cap is an error; dropping it deletes it and returns its bytes.
pub struct SpillFile {
    file: File,
    path: PathBuf,
    cap: u64,
    written: u64,
    accounting: Arc<Mutex<SpillAccounting>>,
    _reservation: StorageReservation,
}

impl SpillFile {
    pub fn len(&self) -> u64 {
        self.written
    }
    pub fn is_empty(&self) -> bool {
        self.written == 0
    }
    pub fn cap(&self) -> u64 {
        self.cap
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Flush, then read the whole spill from the start. Writes after this continue at the end.
    pub fn reader(&mut self) -> io::Result<impl Read + '_> {
        self.file.flush()?;
        self.file.seek(SeekFrom::Start(0))?;
        Ok(SpillReader { file: &mut self.file, remaining: self.written })
    }
}

struct SpillReader<'a> {
    file: &'a mut File,
    remaining: u64,
}

impl Read for SpillReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Ok(0);
        }
        let want = buf.len().min(self.remaining.min(usize::MAX as u64) as usize);
        let n = self.file.read(&mut buf[..want])?;
        self.remaining -= n as u64;
        Ok(n)
    }
}

impl Write for SpillFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let next = self.written.checked_add(buf.len() as u64).ok_or_else(|| io::Error::other("spill length overflow"))?;
        if next > self.cap {
            return Err(io::Error::new(io::ErrorKind::StorageFull, format!("IBD spill cap of {} bytes reached", self.cap)));
        }
        self.file.seek(SeekFrom::End(0))?;
        let n = self.file.write(buf)?;
        self.written += n as u64;
        let mut acc = self.accounting.lock();
        acc.bytes += n as u64;
        acc.high_water = acc.high_water.max(acc.bytes);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

impl Drop for SpillFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        let mut acc = self.accounting.lock();
        acc.bytes = acc.bytes.saturating_sub(self.written);
        acc.files = acc.files.saturating_sub(1);
    }
}
