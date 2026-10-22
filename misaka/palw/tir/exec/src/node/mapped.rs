//! A read-only file mapping: the container's tensors are served in place (the PALWTIR1 format
//! aligns every tensor at 64 bytes, so `i16`/`i32`/`i64` params are reinterpreted, not copied).
//!
//! **In-place rewrites are refused, not computed on** (audit C-07). A `MAP_PRIVATE` mapping of a
//! file another process rewrites in place may show the new bytes (POSIX leaves it unspecified),
//! so weights verified at load could silently change under a claim. The mapping keeps its file
//! descriptor and the file's identity at open (device, inode, length and modification time, from
//! `fstat` on that descriptor); [`MappedFile::check_unmodified`] re-reads it and
//! refuses any difference. Replacing the file by rename (the supported way to update an artifact)
//! leaves this inode untouched, so a mapping keeps serving the bytes it verified.

use std::fs::File;
use std::path::Path;

/// What `fstat` says about the mapped inode: equal stamps before and after mean no in-place write
/// (a write moves the modification time; a truncation also moves the length). The change time is
/// not compared: replacing the file by rename unlinks this inode, which moves only its change time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FileStampV1 {
    dev: u64,
    ino: u64,
    len: u64,
    mtime: (i64, i64),
}

impl FileStampV1 {
    #[cfg(unix)]
    pub(crate) fn of(file: &File) -> std::io::Result<Self> {
        use std::os::unix::fs::MetadataExt;
        let m = file.metadata()?;
        Ok(Self { dev: m.dev(), ino: m.ino(), len: m.len(), mtime: (m.mtime(), m.mtime_nsec()) })
    }
    #[cfg(not(unix))]
    pub(crate) fn of(file: &File) -> std::io::Result<Self> {
        Ok(Self { dev: 0, ino: 0, len: file.metadata()?.len(), mtime: (0, 0) })
    }
}

/// A whole file mapped read-only, unmapped on drop.
pub struct MappedFile {
    ptr: *const u8,
    len: usize,
    /// The descriptor the mapping was made from (kept so the SAME inode is re-checked) and its stamp at open.
    file: File,
    stamp: FileStampV1,
}

// SAFETY: the mapping is read-only and never mutated; the pointer is valid for `len` bytes until
// `drop` unmaps it, and shared `&[u8]` access from several threads is sound.
unsafe impl Send for MappedFile {}
unsafe impl Sync for MappedFile {}

impl MappedFile {
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let file = File::open(path)?;
        let stamp = FileStampV1::of(&file)?;
        let len = stamp.len as usize;
        if len == 0 {
            return Ok(MappedFile { ptr: std::ptr::NonNull::<u8>::dangling().as_ptr(), len: 0, file, stamp });
        }
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            // SAFETY: a fresh private read-only mapping of an open file descriptor; the result is
            // checked against MAP_FAILED before use (the mapping holds its own reference; the
            // descriptor is kept only to re-check the inode's stamp).
            let ptr = unsafe { libc::mmap(std::ptr::null_mut(), len, libc::PROT_READ, libc::MAP_PRIVATE, file.as_raw_fd(), 0) };
            if ptr == libc::MAP_FAILED {
                return Err(std::io::Error::last_os_error());
            }
            Ok(MappedFile { ptr: ptr as *const u8, len, file, stamp })
        }
        #[cfg(not(unix))]
        {
            let _ = (file, stamp);
            Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "file mapping needs a unix target"))
        }
    }

    pub fn bytes(&self) -> &[u8] {
        // SAFETY: `ptr` is valid for `len` bytes for the life of `self` (see `open`).
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }

    /// **The mapped inode has not been written since it was opened** (audit C-07): its stamp is the
    /// one taken at open. An in-place writer moves the modification time (and a truncation
    /// the length), so computing on these bytes is refused rather than done on bytes nobody verified.
    pub fn check_unmodified(&self) -> std::io::Result<()> {
        self.stamp.check(&self.file)
    }
}

impl FileStampV1 {
    pub(crate) fn len_v1(&self) -> u64 {
        self.len
    }

    /// The descriptor's inode still carries this stamp.
    pub(crate) fn check(&self, file: &File) -> std::io::Result<()> {
        if FileStampV1::of(file)? != *self {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "the artifact was modified in place after it was verified; replace artifacts by rename and reload",
            ));
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek, SeekFrom, Write};

    /// Audit C-07: an in-place rewrite of a mapped artifact is detected (and refused by the caller); a rename-replacement
    /// leaves the mapping's inode, its bytes and its check untouched.
    #[test]
    fn audit_fixcd_c07_an_in_place_rewrite_is_detected_and_a_rename_is_not() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("weights.bin");
        std::fs::write(&path, vec![1u8; 4096]).unwrap();
        let map = MappedFile::open(&path).unwrap();
        map.check_unmodified().expect("fresh");
        // Rename-replacement: the mapping still serves the verified bytes and still checks clean.
        let staged = dir.path().join("weights.new");
        std::fs::write(&staged, vec![2u8; 4096]).unwrap();
        std::fs::rename(&staged, &path).unwrap();
        map.check_unmodified().expect("a rename does not touch the mapped inode");
        assert!(map.bytes().iter().all(|b| *b == 1));
        // In-place rewrite of the mapped inode itself: refused.
        let map = MappedFile::open(&path).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let mut f = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        f.seek(SeekFrom::Start(0)).unwrap();
        f.write_all(&[9u8; 16]).unwrap();
        f.sync_all().unwrap();
        let err = map.check_unmodified().unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData, "{err}");
    }
}
