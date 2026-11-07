//! A read-only file mapping: the container's tensors are served in place (the PALWTIR1 format
//! aligns every tensor at 64 bytes, so `i16`/`i32`/`i64` params are reinterpreted, not copied).

use std::fs::File;
use std::path::Path;

/// A whole file mapped read-only, unmapped on drop.
pub struct MappedFile {
    ptr: *const u8,
    len: usize,
}

// SAFETY: the mapping is read-only and never mutated; the pointer is valid for `len` bytes until
// `drop` unmaps it, and shared `&[u8]` access from several threads is sound.
unsafe impl Send for MappedFile {}
unsafe impl Sync for MappedFile {}

impl MappedFile {
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len() as usize;
        if len == 0 {
            return Ok(MappedFile { ptr: std::ptr::NonNull::<u8>::dangling().as_ptr(), len: 0 });
        }
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            // SAFETY: a fresh private read-only mapping of an open file descriptor; the result is
            // checked against MAP_FAILED before use, and the descriptor may close afterwards (the
            // mapping holds its own reference).
            let ptr = unsafe { libc::mmap(std::ptr::null_mut(), len, libc::PROT_READ, libc::MAP_PRIVATE, file.as_raw_fd(), 0) };
            if ptr == libc::MAP_FAILED {
                return Err(std::io::Error::last_os_error());
            }
            Ok(MappedFile { ptr: ptr as *const u8, len })
        }
        #[cfg(not(unix))]
        {
            let _ = file;
            Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "file mapping needs a unix target"))
        }
    }

    pub fn bytes(&self) -> &[u8] {
        // SAFETY: `ptr` is valid for `len` bytes for the life of `self` (see `open`).
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl Drop for MappedFile {
    fn drop(&mut self) {
        #[cfg(unix)]
        if self.len > 0 {
            // SAFETY: `ptr`/`len` are exactly what `mmap` returned; nothing borrows the mapping
            // past `self` (every borrow is tied to `&self`).
            unsafe {
                libc::munmap(self.ptr as *mut libc::c_void, self.len);
            }
        }
    }
}
