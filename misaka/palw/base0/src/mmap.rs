//! **A read-only memory map, and why the runtime needs one.**
//!
//! Qwen3.6 is 35.95 B parameters. At one byte each that is 33.5 GiB of weights, which is more than
//! this machine has of RAM and more than most machines that would produce a block have. Reading
//! the artifact into a `Vec` is not a slow way to do it; it is not a way to do it.
//!
//! A memory map is not a workaround for that — it is the right shape for the access pattern. The
//! mixture reads eight of two hundred and fifty-six experts per token, so 97 % of the weights are
//! untouched on any given step and the resident set is a fraction of the file. The kernel's page
//! cache already implements exactly that policy, and it implements it better than a runtime that
//! guessed which experts to keep.
//!
//! # Why `libc` and not a wrapper
//!
//! Three calls: `mmap`, `munmap`, `madvise`. A wrapper crate would add a dependency to a workspace
//! that audits them, in exchange for wrapping thirty lines.

//! # Portability
//!
//! The three calls are POSIX and there is no Windows equivalent in this file. Rather than let an
//! ungated `std::os::unix` break the whole workspace on `x86_64-pc-windows-msvc` — which is what it
//! did, and which is a defect this repository has recorded before — the mapping is `cfg`-gated and
//! [`ReadOnlyMap::open`] refuses on any other platform. A node there still validates the chain and
//! still produces for the liveness floor, which needs no mapped artifact; it cannot produce for a
//! class whose weights only a mapping can reach, and it is told so at the moment it tries.

use std::fs::File;
#[cfg(unix)]
use std::os::unix::io::AsRawFd;

/// A file mapped read-only into the address space, unmapped on drop.
pub struct ReadOnlyMap {
    ptr: *const u8,
    len: usize,
    /// Kept open so [`Self::read_at`] exists. `mmap` does not need the descriptor after the map
    /// is made; the streaming path does, and one open fd is the whole cost.
    ///
    /// Off POSIX nothing reads it — `open` refuses before one exists — and an unused field is a
    /// warning, which a `-D warnings` check turns into the same red build this gating was written
    /// to fix. Allowed there and only there, so the field stays live where it is used.
    #[cfg_attr(not(unix), allow(dead_code))]
    file: File,
}

// SAFETY: the mapping is read-only and immutable for its whole life, and the pointer is valid
// until `Drop` unmaps it. Nothing hands out a `&mut`.
unsafe impl Send for ReadOnlyMap {}
unsafe impl Sync for ReadOnlyMap {}

impl ReadOnlyMap {
    /// Map the whole file. An empty file maps to an empty slice rather than failing: it is a
    /// legitimate artifact with no tensor data, and `mmap` refuses a zero length.
    #[cfg(unix)]
    pub fn open(path: &std::path::Path) -> std::io::Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len() as usize;
        if len == 0 {
            return Ok(Self { ptr: std::ptr::NonNull::<u8>::dangling().as_ptr(), len: 0, file });
        }
        // SAFETY: `fd` is a valid open descriptor for the length reported by its own metadata.
        let ptr = unsafe { libc::mmap(std::ptr::null_mut(), len, libc::PROT_READ, libc::MAP_PRIVATE, file.as_raw_fd(), 0) };
        if ptr == libc::MAP_FAILED {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self { ptr: ptr as *const u8, len, file })
    }

    /// The non-POSIX arm: refuse, with the reason, rather than pretend.
    ///
    /// Reading the artifact into memory instead is not a fallback — Qwen3.6 is 33.5 GiB and the
    /// whole point of the mapping is that 97 % of it is untouched per token. An `Unsupported`
    /// error at the one call that needs the mapping keeps the crate building everywhere and keeps
    /// the failure at the place a person can act on it.
    #[cfg(not(unix))]
    pub fn open(path: &std::path::Path) -> std::io::Result<Self> {
        let _ = path;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "memory-mapped artifacts are implemented for POSIX only; this platform can validate the chain and \
             produce for the liveness floor, but not for a class whose weights are read through a mapping",
        ))
    }

    /// Read a range through the file descriptor rather than the mapping — the same inode, the
    /// same page cache, the same bytes; only the syscall that touches a cold page differs.
    ///
    /// It differs enormously. A page-cache miss through the mapping is a synchronous 4 KiB
    /// fault, and on the fleet's own virtio disks fault readahead never engages — not under
    /// `MADV_SEQUENTIAL`, not under `MADV_WILLNEED`, not with the block device's readahead
    /// window raised. Measured on the same device, same file, same day: 6 MB/s through the map,
    /// 68 MB/s through a default `read()`, 1.3 GB/s through reads this size. A whole-file pass
    /// belongs on this path — and so, since ADR-0112, does every weight a forward pass reads:
    /// per-token expert access stayed on the map for a while on the theory that the page cache's
    /// resident-set behaviour was the reason the map existed, and the fleet's draws measured what
    /// that theory cost (12.8 GiB through three million faults a draw, twenty minutes). The map's
    /// reason now is the header and the embedding row; the weights go through `read_i8_at`.
    #[cfg(unix)]
    pub fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        use std::os::unix::fs::FileExt;
        self.file.read_exact_at(buf, offset)
    }

    /// **Unreachable off POSIX, and deliberately not implemented there.**
    ///
    /// `open` refuses on any non-POSIX platform, so no `ReadOnlyMap` exists for this to be called
    /// on. A Windows positional read (`seek_read`) would be a few lines — and they would be lines
    /// no CI on this workspace compiles, because the cfg-inversion check that guards this file
    /// exercises the `not(windows)` arm on a POSIX host. Shipping an unreachable branch that
    /// nothing builds is how the ungated `std::os::unix` got here in the first place, so this
    /// stays a refusal until a platform that can compile it needs it.
    #[cfg(not(unix))]
    pub fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        let _ = (offset, buf);
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "positional reads need the POSIX mapping path"))
    }

    /// `len` `i8` values at `offset`, read through the file descriptor into memory this process
    /// owns — the residency loader's read (ADR-0112 Decision 1). The bytes are the mapping's
    /// bytes; what differs is that they arrive through one read sized to the tensor rather than
    /// through page faults sized to a page, which on the fleet's disks is the difference between
    /// 845 MB/s and 11 MB/s. A range that leaves the file is an error with the numbers in it,
    /// for the reason `i8_slice` says `None`: the offsets are an artifact's directory, which is
    /// data.
    pub fn read_i8_at(&self, offset: usize, len: usize) -> std::io::Result<Vec<i8>> {
        if offset.checked_add(len).is_none_or(|end| end > self.len) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                format!("{len} bytes at {offset} leave a {}-byte file", self.len),
            ));
        }
        let mut out = vec![0i8; len];
        // SAFETY: `i8` and `u8` share a layout, and the slice covers exactly the vector's `len`.
        let bytes: &mut [u8] = unsafe { std::slice::from_raw_parts_mut(out.as_mut_ptr() as *mut u8, len) };
        self.read_exact_at(offset as u64, bytes)?;
        Ok(out)
    }

    /// [`Self::read_i8_at`] for bytes that are bytes — a parameter row, a header.
    pub fn read_u8_at(&self, offset: usize, len: usize) -> std::io::Result<Vec<u8>> {
        if offset.checked_add(len).is_none_or(|end| end > self.len) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                format!("{len} bytes at {offset} leave a {}-byte file", self.len),
            ));
        }
        let mut out = vec![0u8; len];
        self.read_exact_at(offset as u64, &mut out)?;
        Ok(out)
    }

    /// Tell the kernel the access pattern is random, which is what a router that picks eight of
    /// two hundred and fifty-six experts produces. Advisory: a failure is not an error, because
    /// the mapping is correct either way.
    /// The whole mapping as a slice. Empty for an empty file. The pages behind it are the kernel's
    /// page cache: every process that maps the same file shares them, which is the point of
    /// mapping an artifact instead of reading it (an artifact read into a `Vec` is a private copy
    /// per process, and seven seats on one host were seven copies).
    pub fn as_slice(&self) -> &[u8] {
        if self.len == 0 { &[] } else { unsafe { std::slice::from_raw_parts(self.ptr, self.len) } }
    }

    pub fn advise_random(&self) {
        if self.len == 0 {
            return;
        }
        #[cfg(not(unix))]
        return;
        #[cfg(unix)]
        // SAFETY: the mapping is live and the length is its own.
        unsafe {
            libc::madvise(self.ptr as *mut libc::c_void, self.len, libc::MADV_RANDOM);
        }
    }

    /// **Ask the kernel to start reading a range now** (`MADV_WILLNEED`).
    ///
    /// Advisory and asynchronous: the call returns before the pages arrive, which is exactly what
    /// makes it useful. The mixture knows which eight experts it needs the moment the router
    /// commits, and issuing all of their ranges before computing the first one lets the read of
    /// the eighth overlap the arithmetic of the first.
    ///
    /// Rounded outward to page boundaries, because `madvise` requires an aligned start and a range
    /// that stops mid-page leaves the tail unread.
    pub fn will_need(&self, offset: usize, len: usize) {
        #[cfg(unix)]
        self.advise(offset, len, libc::MADV_WILLNEED);
        #[cfg(not(unix))]
        self.advise(offset, len, 0);
    }

    /// **Give a range back** (`MADV_DONTNEED`).
    ///
    /// On a private read-only mapping this drops the resident pages and the next touch re-reads
    /// them from the file — no data is lost and nothing is written. It is how an expert cache
    /// EVICTS: without it the page cache keeps every expert it has ever seen and pays for that by
    /// evicting the weights every token needs.
    pub fn dont_need(&self, offset: usize, len: usize) {
        #[cfg(unix)]
        self.advise(offset, len, libc::MADV_DONTNEED);
        #[cfg(not(unix))]
        self.advise(offset, len, 0);
    }

    #[cfg(not(unix))]
    fn advise(&self, offset: usize, len: usize, advice: i32) {
        // Advice is a hint the mapping is correct without; off POSIX there is no mapping at all.
        let _ = (offset, len, advice);
    }

    #[cfg(unix)]
    fn advise(&self, offset: usize, len: usize, advice: i32) {
        if self.len == 0 || len == 0 || offset >= self.len {
            return;
        }
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(1) as usize;
        let start = offset - offset % page;
        let end = (offset + len).min(self.len).next_multiple_of(page).min(self.len.next_multiple_of(page));
        if end <= start {
            return;
        }
        // SAFETY: the range is inside the live mapping and the advice is purely a hint — a failure
        // leaves the mapping correct, which is why the result is discarded.
        unsafe {
            libc::madvise(self.ptr.add(start) as *mut libc::c_void, end - start, advice);
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The whole mapping as bytes.
    pub fn as_bytes(&self) -> &[u8] {
        if self.len == 0 {
            return &[];
        }
        // SAFETY: the mapping covers `len` readable bytes and outlives the borrow.
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }

    /// `len` `i8` values at `offset`, or `None` if the range leaves the mapping.
    ///
    /// Returns `None` rather than panicking: the offsets come from an artifact file's directory,
    /// which is data a producer was handed, and a truncated file must be a refusal rather than a
    /// segmentation fault.
    pub fn i8_slice(&self, offset: usize, len: usize) -> Option<&[i8]> {
        let end = offset.checked_add(len)?;
        if end > self.len {
            return None;
        }
        // SAFETY: the range is inside the mapping, and `i8` has the same layout as `u8`.
        Some(unsafe { std::slice::from_raw_parts(self.ptr.add(offset) as *const i8, len) })
    }
}

impl Drop for ReadOnlyMap {
    fn drop(&mut self) {
        if self.len == 0 {
            return;
        }
        // Off POSIX nothing was ever mapped: `open` refuses, so `len` is only non-zero here on unix.
        #[cfg(not(unix))]
        return;
        #[cfg(unix)]
        // SAFETY: unmapping exactly what was mapped, once.
        unsafe {
            libc::munmap(self.ptr as *mut libc::c_void, self.len);
        }
    }
}

/// **What identifies a file to a host**: the device and inode it lives at, and the size and
/// modification time it had when it was opened. Two processes that mapped the same artifact agree
/// on it whatever path each was given, and a file rewritten under the same name does not — which
/// is what lets a host count one pinned copy once (kaspad's host ledger) and lets a pin refuse a
/// file that is no longer the one a holding decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FileIdentityV1 {
    pub dev: u64,
    pub ino: u64,
    pub len: u64,
    /// Seconds and nanoseconds since the epoch, as the filesystem keeps them.
    pub mtime_sec: i64,
    pub mtime_nsec: i64,
}

impl FileIdentityV1 {
    /// The identity of an open file, from its own descriptor (never a second `stat` of a path that
    /// may have moved). `Unsupported` off POSIX.
    pub fn of_file(file: &File) -> std::io::Result<Self> {
        let meta = file.metadata()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Ok(Self { dev: meta.dev(), ino: meta.ino(), len: meta.len(), mtime_sec: meta.mtime(), mtime_nsec: meta.mtime_nsec() })
        }
        #[cfg(not(unix))]
        {
            let _ = meta;
            Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "file identities are read on POSIX only"))
        }
    }
}

/// **A whole file mapped `PROT_READ | MAP_SHARED`, which can be locked into RAM** — the pin kaspad
/// takes on a class artifact whose replay reads it in place (`palw_artifact_pin`, int-10.2;
/// `docs/design/palw/t12-replay-memory-1001.md`).
///
/// # Why its own mapping, and why `MAP_SHARED`
///
/// The holding's own mapping ([`ReadOnlyMap`], and the IR container's twin) is `MAP_PRIVATE`. For a
/// mapping nobody writes, private and shared map the very same page-cache pages — but only a shared
/// mapping SAYS so: a private page that is ever written, or locked while writable, becomes this
/// process's anonymous copy, which is exactly the per-seat copy a pin exists to avoid. A shared,
/// read-only mapping of an `O_RDONLY` descriptor has no such path, so the pages `mlock` makes
/// resident are, provably, the one copy every process on the host maps. And because the pin is a
/// mapping of its own, its life is the node's policy — a flag, a cap, an eviction — and not the
/// lineage's: the lineages' mappings and decoders do not change at all. A page is unevictable while
/// ANY mapping of it is locked, so locking this one keeps the holding's own view of the same pages
/// resident too.
///
/// # What it costs
///
/// Address space (the file's length again, no memory), one descriptor, and — once locked — the
/// file's pages, which are then neither evicted nor counted in `MemAvailable`: a host pays for one
/// pinned copy once, however many processes lock it. Each locking process is charged the whole
/// mapping against its own `RLIMIT_MEMLOCK` (the kernel counts locked VMAs per process), which is
/// why a process without `CAP_IPC_LOCK` needs the limit raised (`LimitMEMLOCK=` in a unit).
pub struct PinnedFileMapV1 {
    ptr: *mut u8,
    len: usize,
    locked: bool,
    identity: FileIdentityV1,
    /// Kept open: the populate pass reads through it, and the identity is its own.
    #[cfg_attr(not(unix), allow(dead_code))]
    file: File,
}

// SAFETY: the mapping is read-only and never handed out; the pointer is valid until `Drop`.
unsafe impl Send for PinnedFileMapV1 {}
unsafe impl Sync for PinnedFileMapV1 {}

impl PinnedFileMapV1 {
    /// Open `path` read-only and map the whole of it shared. Nothing is resident or locked yet.
    #[cfg(unix)]
    pub fn map_shared(path: &std::path::Path) -> std::io::Result<Self> {
        let file = File::open(path)?;
        let identity = FileIdentityV1::of_file(&file)?;
        let len = usize::try_from(identity.len).map_err(|_| std::io::Error::other("the file is larger than the address space"))?;
        if len == 0 {
            return Ok(Self { ptr: std::ptr::NonNull::<u8>::dangling().as_ptr(), len: 0, locked: false, identity, file });
        }
        // SAFETY: `fd` is a valid open descriptor for the length its own metadata reports.
        let ptr = unsafe { libc::mmap(std::ptr::null_mut(), len, libc::PROT_READ, libc::MAP_SHARED, file.as_raw_fd(), 0) };
        if ptr == libc::MAP_FAILED {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self { ptr: ptr as *mut u8, len, locked: false, identity, file })
    }

    /// The non-POSIX arm: there is no mapping to pin.
    #[cfg(not(unix))]
    pub fn map_shared(path: &std::path::Path) -> std::io::Result<Self> {
        let _ = path;
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "pinning a mapped artifact needs a POSIX mapping"))
    }

    pub fn identity(&self) -> FileIdentityV1 {
        self.identity
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn is_locked(&self) -> bool {
        self.locked
    }

    /// **How many of the file's bytes are in RAM right now** (`mincore`, rounded to whole pages and
    /// clamped to the file): what locking it will NOT have to read. A page another process faulted in
    /// counts — the page cache is the host's, not this process's.
    #[cfg(unix)]
    pub fn resident_bytes(&self) -> std::io::Result<u64> {
        if self.len == 0 {
            return Ok(0);
        }
        let page = page_size();
        let pages = self.len.div_ceil(page);
        let mut vec = vec![0u8; pages];
        // SAFETY: the range is this live mapping; `vec` has one byte per page of it. The pointer
        // casts adapt to the platform's spelling of the same call (`*mut`/`*const`, `u8`/`c_char`).
        let rc = unsafe { libc::mincore(self.ptr as _, self.len, vec.as_mut_ptr() as _) };
        if rc != 0 {
            return Err(std::io::Error::last_os_error());
        }
        let resident = vec.iter().filter(|v| **v & 1 == 1).count() as u64;
        Ok(resident.saturating_mul(page as u64).min(self.len as u64))
    }

    #[cfg(not(unix))]
    pub fn resident_bytes(&self) -> std::io::Result<u64> {
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "residency is read on POSIX only"))
    }

    /// **Bring the file into the page cache by large reads through the descriptor** — before the
    /// lock, so the lock maps resident pages instead of faulting them in 4 KiB at a time (6–11 MB/s
    /// on the fleet's virtio disks against 845 MB/s for reads this size: [`ReadOnlyMap::read_exact_at`]'s
    /// measurement). Pages already resident cost a copy each and no I/O.
    pub fn populate_by_reads(&self, chunk: usize) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileExt;
            let mut buf = vec![0u8; chunk.max(1 << 16)];
            let mut at = 0u64;
            while at < self.len as u64 {
                let want = (self.len as u64 - at).min(buf.len() as u64) as usize;
                self.file.read_exact_at(&mut buf[..want], at)?;
                at += want as u64;
            }
            Ok(())
        }
        #[cfg(not(unix))]
        {
            let _ = chunk;
            Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "reads through a mapping's descriptor need POSIX"))
        }
    }

    /// **Lock the whole mapping into RAM** (`mlock`): every page resident, none evictable while this
    /// lives. `Err` is the kernel's refusal as it gave it (`ENOMEM`/`EAGAIN` past `RLIMIT_MEMLOCK` or
    /// the platform's wire limit, `EPERM` where locking needs a privilege) and leaves nothing locked.
    #[cfg(unix)]
    pub fn lock(&mut self) -> std::io::Result<()> {
        if self.len == 0 || self.locked {
            self.locked = true;
            return Ok(());
        }
        // SAFETY: the range is exactly this live mapping.
        let rc = unsafe { libc::mlock(self.ptr as *const libc::c_void, self.len) };
        if rc != 0 {
            return Err(std::io::Error::last_os_error());
        }
        self.locked = true;
        Ok(())
    }

    #[cfg(not(unix))]
    pub fn lock(&mut self) -> std::io::Result<()> {
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "locking a mapping needs POSIX"))
    }
}

impl Drop for PinnedFileMapV1 {
    fn drop(&mut self) {
        if self.len == 0 {
            return;
        }
        #[cfg(not(unix))]
        return;
        // SAFETY: unlocking and unmapping exactly what was mapped, once. `munmap` alone would drop
        // the lock with the mapping; the explicit `munlock` says so.
        #[cfg(unix)]
        unsafe {
            if self.locked {
                libc::munlock(self.ptr as *const libc::c_void, self.len);
            }
            libc::munmap(self.ptr as *mut libc::c_void, self.len);
        }
    }
}

/// This host's page size, in bytes.
#[cfg(unix)]
fn page_size() -> usize {
    // SAFETY: `sysconf` reads a constant of the running system.
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if page > 0 { page as usize } else { 4096 }
}

/// **This process's `RLIMIT_MEMLOCK`, soft and hard, in bytes** (`u64::MAX` for unlimited) — what a
/// refused lock is measured against and what its warning names, so an operator without root knows
/// which limit to raise. `None` where the platform cannot say.
#[allow(clippy::unnecessary_cast)] // `rlim_t` is `u64` on the platforms this builds for, and a width elsewhere
pub fn memlock_rlimit_v1() -> Option<(u64, u64)> {
    #[cfg(unix)]
    {
        let mut limit = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        // SAFETY: `limit` is a valid out-parameter for the call.
        let rc = unsafe { libc::getrlimit(libc::RLIMIT_MEMLOCK, &mut limit) };
        if rc != 0 {
            return None;
        }
        let bytes = |v: libc::rlim_t| if v == libc::RLIM_INFINITY { u64::MAX } else { v as u64 };
        Some((bytes(limit.rlim_cur), bytes(limit.rlim_max)))
    }
    #[cfg(not(unix))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("misaka-mmap-{name}-{}", std::process::id()));
        let mut f = File::create(&path).expect("create");
        f.write_all(bytes).expect("write");
        path
    }

    #[test]
    fn a_mapped_file_reads_back_and_refuses_what_is_past_it() {
        let bytes: Vec<u8> = (0..=255u8).collect();
        let path = temp("basic", &bytes);
        let map = ReadOnlyMap::open(&path).expect("map");
        map.advise_random();
        assert_eq!(map.len(), 256);
        assert_eq!(map.as_bytes(), &bytes[..]);
        assert_eq!(map.i8_slice(0, 4), Some(&[0i8, 1, 2, 3][..]));
        // The wrap-around byte: 128 as u8 is -128 as i8, which is what a weight code is.
        assert_eq!(map.i8_slice(128, 1), Some(&[-128i8][..]));
        // A range past the end is a refusal, not a fault.
        assert_eq!(map.i8_slice(250, 10), None);
        assert_eq!(map.i8_slice(usize::MAX, 1), None);
        assert_eq!(map.i8_slice(0, 257), None);
        std::fs::remove_file(&path).ok();
    }

    /// **A pinned file is the file, resident, and locked; a rewrite is another identity.** A 3 MiB
    /// temp file mapped shared: its identity is its own metadata's, the populate pass leaves every
    /// page resident (`mincore`), the lock takes, and dropping the pin unlocks and unmaps. Writing the
    /// file again under the same name moves its identity — what a pin compares before it trusts that
    /// the file is still the one a holding decoded.
    #[cfg(unix)]
    #[test]
    fn a_pinned_file_is_resident_locked_and_known_by_its_identity() {
        use std::os::unix::fs::MetadataExt;
        let bytes: Vec<u8> = (0..3u32 << 20).map(|i| (i % 251) as u8).collect();
        let path = temp("pin", &bytes);
        let mut pin = PinnedFileMapV1::map_shared(&path).expect("maps shared");
        let meta = std::fs::metadata(&path).expect("stat");
        assert_eq!(
            pin.identity(),
            FileIdentityV1 {
                dev: meta.dev(),
                ino: meta.ino(),
                len: meta.len(),
                mtime_sec: meta.mtime(),
                mtime_nsec: meta.mtime_nsec()
            }
        );
        assert_eq!(pin.len(), bytes.len());
        pin.populate_by_reads(1 << 20).expect("reads through the descriptor");
        assert_eq!(pin.resident_bytes().expect("mincore"), bytes.len() as u64, "every page is in the page cache after the pass");
        assert!(!pin.is_locked());
        pin.lock().expect("a 3 MiB lock is within any default limit this test runs under");
        assert!(pin.is_locked());
        assert!(memlock_rlimit_v1().is_some(), "the limit a refusal would name is readable");
        drop(pin);
        // The same name, other bytes: another identity.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let mut f = File::create(&path).expect("rewrite");
        f.write_all(&bytes[..1 << 20]).expect("write");
        drop(f);
        let again = PinnedFileMapV1::map_shared(&path).expect("maps again");
        assert_ne!(again.identity().len, bytes.len() as u64);
        std::fs::remove_file(&path).ok();
        // An empty file pins to nothing, and that is not an error.
        let empty = temp("pin-empty", &[]);
        let mut nothing = PinnedFileMapV1::map_shared(&empty).expect("an empty map");
        assert!(nothing.is_empty());
        assert_eq!(nothing.resident_bytes().expect("nothing to ask"), 0);
        nothing.lock().expect("nothing to lock");
        std::fs::remove_file(&empty).ok();
    }

    /// An empty file is a legitimate artifact with no tensor data, and `mmap` refuses a zero
    /// length — so the empty case is handled rather than propagated as an error.
    #[test]
    fn an_empty_file_maps_to_an_empty_slice() {
        let path = temp("empty", &[]);
        let map = ReadOnlyMap::open(&path).expect("map");
        assert!(map.is_empty());
        assert_eq!(map.as_bytes(), &[] as &[u8]);
        assert_eq!(map.i8_slice(0, 0), Some(&[] as &[i8]));
        assert_eq!(map.i8_slice(0, 1), None);
        map.advise_random();
        std::fs::remove_file(&path).ok();
    }
}
