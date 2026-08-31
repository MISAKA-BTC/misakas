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
//! # Why raw syscalls and not a wrapper
//!
//! A handful of calls per platform: `mmap`/`munmap`/`madvise` on POSIX,
//! `CreateFileMappingW`/`MapViewOfFile`/`UnmapViewOfFile`/`PrefetchVirtualMemory` on Windows. A
//! wrapper crate would add a dependency to a workspace that audits them, in exchange for wrapping
//! fifty lines.
//!
//! # The Windows arm (issue #89)
//!
//! Windows maps the same way and reads the same bytes; what it cannot express are two of the three
//! POSIX hints. `PrefetchVirtualMemory` is a faithful `MADV_WILLNEED` (asynchronous, advisory, and
//! the reason the hint exists — overlap the eighth expert's read with the first one's arithmetic).
//! `MADV_RANDOM` and `MADV_DONTNEED` have no counterpart: the first tunes fault readahead the
//! Windows memory manager does not expose, and the second's eviction role is left to the standby
//! list. Both degrade to no-ops, which is the posture every `advise` call here already declares —
//! "a failure is not an error, because the mapping is correct either way". A Windows producer may
//! therefore hold a larger resident set under memory pressure than a Linux one; it computes the
//! same blocks. ADR-0055's measured throughput numbers are the fleet's (Linux) and are not claimed
//! for this arm.

use std::fs::File;
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;

// ---------------------------------------------------------------------------------------------
// The Win32 surface, declared here for the same reason `libc` is used directly on POSIX: four
// functions and a struct, from kernel32, stable since long before anything this workspace targets.
// ---------------------------------------------------------------------------------------------
#[cfg(windows)]
mod win32 {
    use core::ffi::c_void;

    pub const PAGE_READONLY: u32 = 0x02;
    pub const FILE_MAP_READ: u32 = 0x0004;

    #[repr(C)]
    pub struct Win32MemoryRangeEntry {
        pub virtual_address: *mut c_void,
        pub number_of_bytes: usize,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub fn CreateFileMappingW(
            file: *mut c_void,
            attributes: *mut c_void,
            protect: u32,
            maximum_size_high: u32,
            maximum_size_low: u32,
            name: *const u16,
        ) -> *mut c_void;
        pub fn MapViewOfFile(
            mapping: *mut c_void,
            desired_access: u32,
            offset_high: u32,
            offset_low: u32,
            number_of_bytes: usize,
        ) -> *mut c_void;
        pub fn UnmapViewOfFile(address: *const c_void) -> i32;
        pub fn CloseHandle(handle: *mut c_void) -> i32;
        pub fn GetCurrentProcess() -> *mut c_void;
        pub fn PrefetchVirtualMemory(
            process: *mut c_void,
            number_of_entries: usize,
            entries: *mut Win32MemoryRangeEntry,
            flags: u32,
        ) -> i32;
    }
}

/// A file mapped read-only into the address space, unmapped on drop.
pub struct ReadOnlyMap {
    ptr: *const u8,
    len: usize,
    /// Kept open so [`Self::read_exact_at`] exists. The map does not need the descriptor after it
    /// is made; the streaming path does, and one open fd is the whole cost.
    file: File,
    /// The `CreateFileMappingW` handle, which Windows requires alive alongside the view and closed
    /// after it. Null for an empty file, exactly as `ptr` is dangling for one.
    #[cfg(windows)]
    mapping: *mut core::ffi::c_void,
}

// SAFETY: the mapping is read-only and immutable for its whole life, and the pointer is valid
// until `Drop` unmaps it. Nothing hands out a `&mut`.
unsafe impl Send for ReadOnlyMap {}
unsafe impl Sync for ReadOnlyMap {}

impl ReadOnlyMap {
    /// Map the whole file. An empty file maps to an empty slice rather than failing: it is a
    /// legitimate artifact with no tensor data, and both `mmap` and `CreateFileMappingW` refuse a
    /// zero length.
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

    /// The Windows arm of [`Self::open`]: a pagefile-backed section over the file, mapped as one
    /// read-only view. The section handle is kept and closed after the view in `Drop` — the
    /// documented order, though Windows tolerates either.
    #[cfg(windows)]
    pub fn open(path: &std::path::Path) -> std::io::Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len() as usize;
        if len == 0 {
            return Ok(Self { ptr: std::ptr::NonNull::<u8>::dangling().as_ptr(), len: 0, file, mapping: std::ptr::null_mut() });
        }
        // SAFETY: the handle is a valid open file. Size zeros mean "the file's current size", and
        // the view below asks for the same `len` the metadata reported.
        let mapping = unsafe {
            win32::CreateFileMappingW(file.as_raw_handle() as *mut _, std::ptr::null_mut(), win32::PAGE_READONLY, 0, 0, std::ptr::null())
        };
        if mapping.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: the section was just created over this file at PAGE_READONLY.
        let ptr = unsafe { win32::MapViewOfFile(mapping, win32::FILE_MAP_READ, 0, 0, len) };
        if ptr.is_null() {
            let err = std::io::Error::last_os_error();
            // SAFETY: closing the handle this function created; the view never existed.
            unsafe { win32::CloseHandle(mapping) };
            return Err(err);
        }
        Ok(Self { ptr: ptr as *const u8, len, file, mapping })
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
    #[cfg(unix)]
    pub fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        use std::os::unix::fs::FileExt;
        self.file.read_exact_at(buf, offset)
    }

    /// The Windows arm of [`Self::read_exact_at`]: `seek_read` is positional like `pread` but
    /// does not promise a full buffer, so it loops — the same contract `read_exact_at` has on
    /// POSIX, restated by hand.
    #[cfg(windows)]
    pub fn read_exact_at(&self, mut offset: u64, mut buf: &mut [u8]) -> std::io::Result<()> {
        use std::os::windows::fs::FileExt;
        while !buf.is_empty() {
            match self.file.seek_read(buf, offset) {
                Ok(0) => return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "failed to fill whole buffer")),
                Ok(n) => {
                    buf = &mut buf[n..];
                    offset += n as u64;
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    /// Tell the kernel the access pattern is random, which is what a router that picks eight of
    /// two hundred and fifty-six experts produces. Advisory: a failure is not an error, because
    /// the mapping is correct either way. Windows exposes no fault-readahead tuning, so there
    /// this is the no-op the sentence above already permits.
    pub fn advise_random(&self) {
        if self.len == 0 {
            return;
        }
        #[cfg(unix)]
        // SAFETY: the mapping is live and the length is its own.
        unsafe {
            libc::madvise(self.ptr as *mut libc::c_void, self.len, libc::MADV_RANDOM);
        }
    }

    /// **Ask the kernel to start reading a range now** (`MADV_WILLNEED` /
    /// `PrefetchVirtualMemory`).
    ///
    /// Advisory and asynchronous: the call returns before the pages arrive, which is exactly what
    /// makes it useful. The mixture knows which eight experts it needs the moment the router
    /// commits, and issuing all of their ranges before computing the first one lets the read of
    /// the eighth overlap the arithmetic of the first.
    ///
    /// Rounded outward to page boundaries, because `madvise` requires an aligned start and a range
    /// that stops mid-page leaves the tail unread. (`PrefetchVirtualMemory` does not require the
    /// alignment; it gets the same rounded range because asking for the whole page is what the
    /// fault will do anyway.)
    pub fn will_need(&self, offset: usize, len: usize) {
        #[cfg(unix)]
        self.advise(offset, len, libc::MADV_WILLNEED);
        #[cfg(windows)]
        {
            let Some((start, end)) = self.rounded_range(offset, len) else { return };
            let mut entry = win32::Win32MemoryRangeEntry {
                // SAFETY: `start` is inside the live mapping (rounded_range checked it).
                virtual_address: unsafe { self.ptr.add(start) as *mut _ },
                number_of_bytes: end - start,
            };
            // SAFETY: the range is inside the live mapping and the call is purely a hint — a
            // failure (or an old kernel32 without the symbol would be a link error, not a runtime
            // one; the function is Win8+, below every supported target) leaves the mapping
            // correct, which is why the result is discarded.
            unsafe {
                win32::PrefetchVirtualMemory(win32::GetCurrentProcess(), 1, &mut entry, 0);
            }
        }
    }

    /// **Give a range back** (`MADV_DONTNEED`).
    ///
    /// On a private read-only mapping this drops the resident pages and the next touch re-reads
    /// them from the file — no data is lost and nothing is written. It is how an expert cache
    /// EVICTS: without it the page cache keeps every expert it has ever seen and pays for that by
    /// evicting the weights every token needs. Windows has no per-range counterpart for a mapped
    /// view; eviction is the standby list's job there, so this is a no-op and a Windows producer
    /// simply carries a larger resident set under pressure.
    pub fn dont_need(&self, offset: usize, len: usize) {
        #[cfg(unix)]
        self.advise(offset, len, libc::MADV_DONTNEED);
        #[cfg(windows)]
        {
            let _ = (offset, len);
        }
    }

    /// The page-rounded intersection of `[offset, offset+len)` with the mapping, or `None` when
    /// nothing of it lands inside. 4,096 on Windows — the page size on every architecture the
    /// msvc targets cover — and `sysconf` on POSIX, where it can genuinely vary.
    fn rounded_range(&self, offset: usize, len: usize) -> Option<(usize, usize)> {
        if self.len == 0 || len == 0 || offset >= self.len {
            return None;
        }
        #[cfg(unix)]
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(1) as usize;
        #[cfg(windows)]
        let page = 4096usize;
        let start = offset - offset % page;
        let end = (offset + len).min(self.len).next_multiple_of(page).min(self.len.next_multiple_of(page));
        (end > start).then_some((start, end))
    }

    #[cfg(unix)]
    fn advise(&self, offset: usize, len: usize, advice: i32) {
        let Some((start, end)) = self.rounded_range(offset, len) else { return };
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
        #[cfg(unix)]
        // SAFETY: unmapping exactly what was mapped, once.
        unsafe {
            libc::munmap(self.ptr as *mut libc::c_void, self.len);
        }
        #[cfg(windows)]
        // SAFETY: unmapping the view this struct mapped, then closing the section handle it
        // created — each exactly once, in the documented order.
        unsafe {
            win32::UnmapViewOfFile(self.ptr as *const _);
            win32::CloseHandle(self.mapping);
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

    /// The hint pair and the streaming read, on whichever platform runs the test — the hints must
    /// be inert (the mapping stays correct) and the positional read must fill the buffer exactly.
    #[test]
    fn hints_are_inert_and_positional_reads_fill_the_buffer() {
        let bytes: Vec<u8> = (0..255u8).cycle().take(10_000).collect();
        let path = temp("hints", &bytes);
        let map = ReadOnlyMap::open(&path).expect("map");
        map.will_need(4_000, 2_000);
        map.dont_need(4_000, 2_000);
        map.will_need(9_999, 500); // clipped at the end, not an error
        let mut buf = [0u8; 100];
        map.read_exact_at(9_900, &mut buf).expect("tail read");
        assert_eq!(&buf[..], &bytes[9_900..10_000]);
        assert_eq!(map.as_bytes()[4_000..4_010], bytes[4_000..4_010], "hints must not perturb the mapping");
        std::fs::remove_file(&path).ok();
    }

    /// An empty file is a legitimate artifact with no tensor data, and both platforms' mapping
    /// calls refuse a zero length — so the empty case is handled rather than propagated.
    #[test]
    fn an_empty_file_maps_to_an_empty_slice() {
        let path = temp("empty", &[]);
        let map = ReadOnlyMap::open(&path).expect("map");
        assert!(map.is_empty());
        assert_eq!(map.as_bytes(), &[] as &[u8]);
        assert_eq!(map.i8_slice(0, 0), Some(&[] as &[i8]));
        assert_eq!(map.i8_slice(0, 1), None);
        map.advise_random();
        map.will_need(0, 1);
        map.dont_need(0, 1);
        std::fs::remove_file(&path).ok();
    }
}
