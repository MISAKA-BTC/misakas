//! Read-only existing filesystem quota evidence. Never creates a project,
//! changes quotas, or accepts configured bytes as kernel enforcement.
//! Linux ABI references: v6.12 include/uapi/linux/{fs.h,dqblk_xfs.h,quota.h}.
use super::NativeCoverageV1;
use std::path::Path;

#[derive(Debug)]
pub struct NativeHardDatabaseQuotaV1 {
    limit: u64,
    used: u64,
    project: u32,
    path: std::path::PathBuf,
}
impl NativeHardDatabaseQuotaV1 {
    pub fn limit_bytes(&self) -> u64 {
        self.limit
    }
    pub fn headroom_bytes(&self) -> u64 {
        self.limit.saturating_sub(self.used)
    }
    pub fn project(&self) -> u32 {
        self.project
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(all(target_os = "linux", target_endian = "little", any(target_arch = "x86_64", target_arch = "aarch64")))]
pub fn actual_hard_database_quota_v1(path: &Path) -> Result<NativeHardDatabaseQuotaV1, NativeCoverageV1> {
    use std::{
        ffi::CString,
        fs::{self, File},
        io::Read,
        os::{
            fd::AsRawFd,
            unix::{ffi::OsStrExt, fs::MetadataExt},
        },
    };
    let refusal =
        |why: &str| NativeCoverageV1::unsupported::<NativeHardDatabaseQuotaV1>("RocksDB history/WAL/compaction project quota", why);
    // Only read operations are declared. No ioctl or quotactl setter exists in
    // this module, so discovery cannot alter host filesystem configuration.
    unsafe extern "C" {
        fn ioctl(fd: i32, request: usize, ...) -> i32;
        fn quotactl(command: i32, special: *const std::ffi::c_char, id: i32, address: *mut std::ffi::c_char) -> i32;
    }
    #[repr(C)]
    #[derive(Default, PartialEq, Eq)]
    struct InodeProject {
        flags: u32,
        extent: u32,
        extents: u32,
        project: u32,
        cow_extent: u32,
        padding: [u8; 8],
    }
    #[repr(C, align(8))]
    struct QuotaBytes([u8; 112]);
    #[repr(C, align(8))]
    struct StatusBytes([u8; 160]);
    const GET_ATTRIBUTES: usize = 0x801c_581f;
    const GET_PROJECT_QUOTA: i32 = ((('X' as i32) << 8 | 3) << 8) | 2;
    const GET_PROJECT_STATUS: i32 = ((('X' as i32) << 8 | 8) << 8) | 2;
    fn project(file: &File) -> std::io::Result<InodeProject> {
        let mut value = InodeProject::default();
        // SAFETY: a live descriptor and correctly sized repr(C) output buffer;
        // FSGETXATTR is a read-only ioctl on the pinned Linux UAPI profile.
        if unsafe { ioctl(file.as_raw_fd(), GET_ATTRIBUTES, &mut value as *mut InodeProject) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(value)
    }
    fn bounded_text(path: &Path) -> std::io::Result<String> {
        let mut bytes = Vec::new();
        File::open(path)?.take((256 << 10) + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 256 << 10 {
            return Err(std::io::Error::other("quota mount metadata exceeds 256 KiB"));
        }
        String::from_utf8(bytes).map_err(std::io::Error::other)
    }
    fn unescape(value: &str) -> String {
        value.replace("\\040", " ").replace("\\011", "\t").replace("\\012", "\n").replace("\\134", "\\")
    }
    let canonical = path.canonicalize().map_err(|_| refusal("database directory is unavailable"))?;
    let directory = File::open(&canonical).map_err(|_| refusal("cannot open actual database directory"))?;
    let before = directory.metadata().map_err(|_| refusal("database inode metadata unavailable"))?;
    if !before.is_dir() {
        return Err(refusal("database path is not a directory"));
    }
    let attributes = project(&directory).map_err(|_| refusal("actual inode has no supported project-id evidence"))?;
    if attributes.project == 0 || attributes.project > i32::MAX as u32 || attributes.flags & 0x200 == 0 {
        return Err(refusal("database must belong to a nonzero project with inherited project assignment"));
    }
    if attributes.flags & (0x1 | 0x100) != 0 {
        return Err(refusal("realtime allocation requires an unaudited separate quota domain"));
    }
    let mountinfo = bounded_text(Path::new("/proc/self/mountinfo")).map_err(|_| refusal("kernel mount metadata unavailable"))?;
    let mut selected = None;
    for line in mountinfo.lines() {
        let Some((left, right)) = line.split_once(" - ") else { continue };
        let left: Vec<_> = left.split_whitespace().collect();
        let right: Vec<_> = right.split_whitespace().collect();
        if left.len() < 6 || right.len() < 3 {
            continue;
        }
        let mount = std::path::PathBuf::from(unescape(left[4]));
        if canonical.starts_with(&mount)
            && selected
                .as_ref()
                .is_none_or(|(old, _, _): &(std::path::PathBuf, String, String)| mount.components().count() > old.components().count())
        {
            selected = Some((mount, right[0].to_owned(), unescape(right[1])));
        }
    }
    let (_, filesystem, device) = selected.ok_or_else(|| refusal("database is outside an observed kernel mount"))?;
    if filesystem != "xfs" {
        return Err(refusal("only the audited XFS single-directory project-quota backend is implemented"));
    }
    let device = CString::new(device).map_err(|_| refusal("quota device contains NUL"))?;
    let mut status = StatusBytes([0; 160]);
    status.0[0] = 1;
    // SAFETY: correctly aligned/sized version-1 output buffer, valid device
    // CString; Q_XGETQSTATV reads accounting/enforcement flags only.
    if unsafe { quotactl(GET_PROJECT_STATUS, device.as_ptr(), 0, status.0.as_mut_ptr().cast()) } != 0 {
        return Err(refusal("kernel project quota status is unavailable"));
    }
    let flags = u16::from_le_bytes([status.0[2], status.0[3]]);
    if status.0[0] != 1 || flags & 0x30 != 0x30 {
        return Err(refusal("project quota accounting AND enforcement were not observed"));
    }
    let mut quota = QuotaBytes([0; 112]);
    // SAFETY: live device CString, finite project id and correctly aligned/sized
    // fs_disk_quota v1 output buffer. Q_XGETQUOTA has no host mutation.
    if unsafe { quotactl(GET_PROJECT_QUOTA, device.as_ptr(), attributes.project as i32, quota.0.as_mut_ptr().cast()) } != 0 {
        return Err(refusal("actual project quota limits are unavailable"));
    }
    let word = |offset: usize| u64::from_le_bytes(quota.0[offset..offset + 8].try_into().unwrap());
    if quota.0[0] != 1 || quota.0[1] & 2 == 0 || u32::from_le_bytes(quota.0[4..8].try_into().unwrap()) != attributes.project {
        return Err(refusal("quota version/type/project identity is inconsistent"));
    }
    let limit = word(8)
        .checked_mul(512)
        .filter(|n| *n > 0 && *n < u64::MAX)
        .ok_or_else(|| refusal("project has no finite nonsaturated hard block limit"))?;
    let used =
        word(40).checked_mul(512).ok_or_else(|| NativeCoverageV1::overflow::<NativeHardDatabaseQuotaV1>("project block usage"))?;
    // Whole DB coverage requires existing files to share the quota. A default
    // RocksDB single-directory layout is flat; symlinks/subdirectories/external
    // WAL/table paths do not silently inherit this evidence.
    let mut entries = 0usize;
    for item in fs::read_dir(&canonical).map_err(|_| refusal("database directory cannot be inventoried"))? {
        entries = entries.checked_add(1).ok_or_else(|| refusal("database file count overflow"))?;
        if entries > 100_000 {
            return Err(refusal("database file inventory exceeds bounded profile"));
        }
        let item = item.map_err(|_| refusal("database file inventory changed/unavailable"))?;
        if !item.file_type().map_err(|_| refusal("database file type unavailable"))?.is_file() {
            return Err(refusal("nonregular/symlink/nested database layout has no whole-project coverage"));
        }
        let file = File::open(item.path()).map_err(|_| refusal("database file changed during inventory"))?;
        let metadata = file.metadata().map_err(|_| refusal("database file metadata unavailable"))?;
        let row = project(&file).map_err(|_| refusal("existing database file project id unavailable"))?;
        if metadata.dev() != before.dev() || row.project != attributes.project || row.flags & 1 != 0 {
            return Err(refusal("existing database files do not share the enforced normal-data project"));
        }
    }
    let after = fs::metadata(&canonical).map_err(|_| refusal("database directory disappeared"))?;
    if after.dev() != before.dev()
        || after.ino() != before.ino()
        || project(&directory).map_err(|_| refusal("database project changed"))? != attributes
    {
        return Err(refusal("database inode/project changed during quota inventory"));
    }
    Ok(NativeHardDatabaseQuotaV1 { limit, used, project: attributes.project, path: canonical })
}

#[cfg(not(all(target_os = "linux", target_endian = "little", any(target_arch = "x86_64", target_arch = "aarch64"))))]
pub fn actual_hard_database_quota_v1(_: &Path) -> Result<NativeHardDatabaseQuotaV1, NativeCoverageV1> {
    Err(NativeCoverageV1::unsupported::<NativeHardDatabaseQuotaV1>(
        "RocksDB history/WAL/compaction project quota",
        "no audited enforced filesystem-quota backend on this target",
    ))
}
