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

use std::fs::File;
use std::os::unix::io::AsRawFd;

/// A file mapped read-only into the address space, unmapped on drop.
pub struct ReadOnlyMap {
    ptr: *const u8,
    len: usize,
    /// Kept open so [`Self::read_at`] exists. `mmap` does not need the descriptor after the map
    /// is made; the streaming path does, and one open fd is the whole cost.
    file: File,
}

// SAFETY: the mapping is read-only and immutable for its whole life, and the pointer is valid
// until `Drop` unmaps it. Nothing hands out a `&mut`.
unsafe impl Send for ReadOnlyMap {}
unsafe impl Sync for ReadOnlyMap {}

impl ReadOnlyMap {
    /// Map the whole file. An empty file maps to an empty slice rather than failing: it is a
    /// legitimate artifact with no tensor data, and `mmap` refuses a zero length.
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

    /// Read a range through the file descriptor rather than the mapping — the same inode, the
    /// same page cache, the same bytes; only the syscall that touches a cold page differs.
    ///
    /// It differs enormously. A page-cache miss through the mapping is a synchronous 4 KiB
    /// fault, and on the fleet's own virtio disks fault readahead never engages — not under
    /// `MADV_SEQUENTIAL`, not under `MADV_WILLNEED`, not with the block device's readahead
    /// window raised. Measured on the same device, same file, same day: 6 MB/s through the map,
    /// 68 MB/s through a default `read()`, 1.3 GB/s through reads this size. A whole-file pass
    /// belongs on this path; per-token expert access stays on the map, whose resident-set
    /// behavior is the reason the map exists.
    pub fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        use std::os::unix::fs::FileExt;
        self.file.read_exact_at(buf, offset)
    }

    /// Tell the kernel the access pattern is random, which is what a router that picks eight of
    /// two hundred and fifty-six experts produces. Advisory: a failure is not an error, because
    /// the mapping is correct either way.
    pub fn advise_random(&self) {
        if self.len == 0 {
            return;
        }
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
        self.advise(offset, len, libc::MADV_WILLNEED);
    }

    /// **Give a range back** (`MADV_DONTNEED`).
    ///
    /// On a private read-only mapping this drops the resident pages and the next touch re-reads
    /// them from the file — no data is lost and nothing is written. It is how an expert cache
    /// EVICTS: without it the page cache keeps every expert it has ever seen and pays for that by
    /// evicting the weights every token needs.
    pub fn dont_need(&self, offset: usize, len: usize) {
        self.advise(offset, len, libc::MADV_DONTNEED);
    }

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
        // SAFETY: unmapping exactly what was mapped, once.
        unsafe {
            libc::munmap(self.ptr as *mut libc::c_void, self.len);
        }
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
