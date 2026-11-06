//! Bounded, explicit loading of existing G14 source material into a process cache.
//!
//! Files contain the canonical Borsh bytes of an existing G14 opening (or the
//! existing whole flat-prompt Vec<u32>), without another network envelope. These
//! filenames are local storage addresses, not allocated native wire identifiers.
//! Loading authenticates DATA IDENTITY only. It does not admit a duty or check
//! model computation. No method rebuilds a trace, opening tree, or witness.
use super::{Digest, SourceKind, StoredProviderV2, StoredUnitV2};
use kaspa_consensus_core::{
    Hash64,
    palw_artifact::{PalwArtifactOpeningV1, PalwArtifactOperandV1, verify_artifact_opening_v1},
    palw_prompt_ids_v1::{
        PALW_PROMPT_IDS_TILE_LEN, PalwPromptIdsFormV1, PalwPromptIdsOpeningV1, prompt_token_ids_match_v1, verify_prompt_ids_opening_v1,
    },
    palw_step_leg::PalwStepOpeningV1,
};
use std::{collections::BTreeMap, fs::File, io::Read, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoredEncodingV2 {
    Artifact,
    PromptTile,
    FlatPrompt,
}
impl StoredEncodingV2 {
    fn kind(self) -> SourceKind {
        match self {
            Self::Artifact => SourceKind::ArtifactInventory,
            Self::PromptTile | Self::FlatPrompt => SourceKind::Input,
        }
    }
    fn stem(self) -> &'static str {
        match self {
            Self::Artifact => "artifact",
            Self::PromptTile => "prompt-tile",
            Self::FlatPrompt => "prompt-flat",
        }
    }
}

/// Finite process-load bounds, separate from a claim's canonical service meter.
/// Cache payload bytes exclude allocator/map overhead; max_cached_units also
/// bounds that metadata. A load needs at most one file buffer and decoded unit
/// in addition to the previously retained cache. This is not an RSS measurement.
#[derive(Clone, Copy, Debug)]
pub struct StoreLimitsV2 {
    pub max_unit_bytes: u64,
    pub max_cached_bytes: u64,
    pub max_cached_units: u64,
    pub max_file_reads: u64,
    pub max_reserved_read_bytes: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StoreMeterV2 {
    pub file_reads: u64,
    /// Declared file length plus one byte reserved before I/O, including errors.
    pub reserved_read_bytes: u64,
    pub cached_bytes: u64,
    pub cached_units: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreErrorV2 {
    Budget,
    Io(String),
    NotRegular,
    NonCanonical,
    Authentication,
    Address,
    EncodingConflict,
    UnsupportedPlatform,
}
fn io(error: std::io::Error) -> StoreErrorV2 {
    StoreErrorV2::Io(error.to_string())
}

/// No caller-supplied path components: the complete file is directly beneath
/// the opened root FD. There are no intermediate directories to traverse.
pub fn source_file_name_v2(source: Digest, index: u64, encoding: StoredEncodingV2) -> String {
    format!("{}-{}-{index}.borsh", encoding.stem(), faster_hex::hex_string(&source.0))
}
struct Entry {
    encoding: StoredEncodingV2,
    unit: StoredUnitV2,
}

/// Unix directory-FD loader. `unit()` performs no I/O and returns only immutable,
/// already loaded data. Preloading is explicit and spends this store's own
/// finite read/cache budgets; it is not hidden inside a Miner response callback.
/// Native response acceptance/fees still belong to A4's actual intake.
pub struct FileStoredProviderV2 {
    directory: File,
    limits: StoreLimitsV2,
    meter: StoreMeterV2,
    cache: BTreeMap<(SourceKind, Digest, u64), Entry>,
}
impl FileStoredProviderV2 {
    pub fn open(root: &Path, limits: StoreLimitsV2) -> Result<Self, StoreErrorV2> {
        if limits.max_unit_bytes == 0
            || limits.max_unit_bytes > usize::MAX as u64
            || limits.max_cached_bytes == 0
            || limits.max_cached_units == 0
            || limits.max_file_reads == 0
            || limits.max_reserved_read_bytes == 0
        {
            return Err(StoreErrorV2::Budget);
        }
        let directory = open_directory(root)?;
        Ok(Self { directory, limits, meter: StoreMeterV2::default(), cache: BTreeMap::new() })
    }
    pub fn meter(&self) -> StoreMeterV2 {
        self.meter
    }

    /// Load one already stored opening. Exact duplicate loads are idempotent.
    /// Errors preserve consumed read quotas. A corrupt/missing provider can be
    /// skipped by the caller in favor of another configured provider.
    pub fn load(&mut self, source: Digest, index: u64, encoding: StoredEncodingV2) -> Result<(), StoreErrorV2> {
        if (encoding == StoredEncodingV2::FlatPrompt && index != 0)
            || (encoding != StoredEncodingV2::FlatPrompt && index > u32::MAX as u64)
        {
            return Err(StoreErrorV2::Address);
        }
        let key = (encoding.kind(), source, index);
        if let Some(entry) = self.cache.get(&key) {
            return if entry.encoding == encoding { Ok(()) } else { Err(StoreErrorV2::EncodingConflict) };
        }
        if self.meter.cached_units >= self.limits.max_cached_units || self.meter.file_reads >= self.limits.max_file_reads {
            return Err(StoreErrorV2::Budget);
        }
        self.meter.file_reads += 1;
        let mut file = open_file(&self.directory, &source_file_name_v2(source, index, encoding))?;
        let metadata = file.metadata().map_err(io)?;
        if !metadata.is_file() {
            return Err(StoreErrorV2::NotRegular);
        }
        let bytes = metadata.len();
        let read_reservation = bytes.checked_add(1).ok_or(StoreErrorV2::Budget)?;
        let total_read = self.meter.reserved_read_bytes.checked_add(read_reservation).ok_or(StoreErrorV2::Budget)?;
        let total_cache = self.meter.cached_bytes.checked_add(bytes).ok_or(StoreErrorV2::Budget)?;
        if bytes == 0
            || bytes > self.limits.max_unit_bytes
            || total_cache > self.limits.max_cached_bytes
            || total_read > self.limits.max_reserved_read_bytes
        {
            return Err(StoreErrorV2::Budget);
        }
        // Reserve before reading, including truncation/corruption and a possible
        // one-byte growth probe. Nothing can reset this quota by failing a load.
        self.meter.reserved_read_bytes = total_read;
        let mut body = Vec::new();
        body.try_reserve_exact(bytes as usize).map_err(|_| StoreErrorV2::Budget)?;
        body.resize(bytes as usize, 0);
        file.read_exact(&mut body).map_err(io)?;
        let mut extra = [0u8; 1];
        if file.read(&mut extra).map_err(io)? != 0 {
            return Err(StoreErrorV2::NonCanonical);
        }
        let unit = decode(&body, encoding)?;
        if !authenticates(source, index, &unit) {
            return Err(StoreErrorV2::Authentication);
        }
        self.cache.insert(key, Entry { encoding, unit });
        self.meter.cached_bytes = total_cache;
        self.meter.cached_units += 1;
        Ok(())
    }
}
impl StoredProviderV2 for FileStoredProviderV2 {
    fn unit(&self, kind: SourceKind, source: Digest, index: u64) -> Result<&StoredUnitV2, String> {
        self.cache
            .get(&(kind, source, index))
            .map(|entry| &entry.unit)
            .ok_or_else(|| "source unit is not in the bounded prepared cache".into())
    }
}

#[cfg(unix)]
fn open_directory(root: &Path) -> Result<File, StoreErrorV2> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(root)
        .map_err(io)?;
    if !file.metadata().map_err(io)?.is_dir() {
        return Err(StoreErrorV2::NotRegular);
    }
    Ok(file)
}
#[cfg(not(unix))]
fn open_directory(_: &Path) -> Result<File, StoreErrorV2> {
    Err(StoreErrorV2::UnsupportedPlatform)
}
#[cfg(unix)]
fn open_file(directory: &File, name: &str) -> Result<File, StoreErrorV2> {
    use std::os::fd::{AsRawFd, FromRawFd};
    let name = std::ffi::CString::new(name).map_err(|_| StoreErrorV2::Address)?;
    // SAFETY: generated `name` has one path component and stays alive across
    // openat; the directory FD is owned here. Only a successful fresh FD is
    // converted into File. NOFOLLOW prevents a final symlink; NONBLOCK prevents
    // a malicious FIFO from blocking before its non-regular type is rejected.
    let fd = unsafe {
        libc::openat(directory.as_raw_fd(), name.as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK)
    };
    if fd < 0 {
        return Err(io(std::io::Error::last_os_error()));
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}
#[cfg(not(unix))]
fn open_file(_: &File, _: &str) -> Result<File, StoreErrorV2> {
    Err(StoreErrorV2::UnsupportedPlatform)
}

fn authenticates(source: Digest, index: u64, unit: &StoredUnitV2) -> bool {
    let root = Hash64::from_bytes(source.0);
    match unit {
        StoredUnitV2::Artifact(opening) => u64::from(opening.leaf_index) == index && verify_artifact_opening_v1(opening, root).is_ok(),
        StoredUnitV2::Input(opening) => {
            u64::from(opening.tile_index) == index && verify_prompt_ids_opening_v1(&root, opening.prompt_token_count, opening).is_ok()
        }
        StoredUnitV2::FlatInput(ids) => index == 0 && prompt_token_ids_match_v1(PalwPromptIdsFormV1::Flat, ids, &root),
    }
}

/// This decoder is deliberately explicit: an untrusted Borsh vector length is
/// compared with both the protocol bound and the remaining file before allocation.
struct Reader<'a> {
    rest: &'a [u8],
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], StoreErrorV2> {
        if n > self.rest.len() {
            return Err(StoreErrorV2::NonCanonical);
        }
        let (bytes, rest) = self.rest.split_at(n);
        self.rest = rest;
        Ok(bytes)
    }
    fn u8(&mut self) -> Result<u8, StoreErrorV2> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, StoreErrorV2> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, StoreErrorV2> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, StoreErrorV2> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn hash(&mut self) -> Result<Hash64, StoreErrorV2> {
        Ok(Hash64::from_bytes(self.take(64)?.try_into().unwrap()))
    }
    fn bytes(&mut self) -> Result<Vec<u8>, StoreErrorV2> {
        let n = self.u32()? as usize;
        let bytes = self.take(n)?;
        let mut out = Vec::new();
        out.try_reserve_exact(n).map_err(|_| StoreErrorV2::Budget)?;
        out.extend_from_slice(bytes);
        Ok(out)
    }
    fn hashes(&mut self) -> Result<Vec<Hash64>, StoreErrorV2> {
        let n = self.u32()? as usize;
        if n > 32 || n > self.rest.len() / 64 {
            return Err(StoreErrorV2::NonCanonical);
        }
        (0..n).map(|_| self.hash()).collect()
    }
    fn ids(&mut self, max: usize) -> Result<Vec<u32>, StoreErrorV2> {
        let n = self.u32()? as usize;
        if n > max || n > self.rest.len() / 4 {
            return Err(StoreErrorV2::NonCanonical);
        }
        let mut out = Vec::new();
        out.try_reserve_exact(n).map_err(|_| StoreErrorV2::Budget)?;
        for _ in 0..n {
            out.push(self.u32()?);
        }
        Ok(out)
    }
    fn opening(&mut self) -> Result<PalwStepOpeningV1, StoreErrorV2> {
        Ok(PalwStepOpeningV1 { leaf_index: self.u64()?, leaf_hash: self.hash()?, siblings: self.hashes()? })
    }
}
fn decode(bytes: &[u8], encoding: StoredEncodingV2) -> Result<StoredUnitV2, StoreErrorV2> {
    let mut r = Reader { rest: bytes };
    let unit = match encoding {
        StoredEncodingV2::Artifact => {
            let tensor_name = String::from_utf8(r.bytes()?).map_err(|_| StoreErrorV2::NonCanonical)?;
            let layer = match r.u8()? {
                0 => None,
                1 => Some(r.u16()?),
                _ => return Err(StoreErrorV2::NonCanonical),
            };
            let row_start = r.u32()?;
            let operand = PalwArtifactOperandV1 { tensor_name, layer, row_start, bytes: r.bytes()? };
            StoredUnitV2::Artifact(PalwArtifactOpeningV1 { operand, leaf_index: r.u32()?, leaf_count: r.u32()?, path: r.hashes()? })
        }
        StoredEncodingV2::PromptTile => StoredUnitV2::Input(PalwPromptIdsOpeningV1 {
            prompt_token_count: r.u32()?,
            tile_index: r.u32()?,
            tile_ids: r.ids(PALW_PROMPT_IDS_TILE_LEN as usize)?,
            opening: r.opening()?,
        }),
        StoredEncodingV2::FlatPrompt => StoredUnitV2::FlatInput(r.ids(bytes.len() / 4)?),
    };
    if !r.rest.is_empty() {
        return Err(StoreErrorV2::NonCanonical);
    }
    Ok(unit)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        palw_artifact::{artifact_leaf_v1, artifact_root_v1, open_artifact_leaf_v1},
        palw_prompt_ids_v1::{prompt_ids_opening_v1, prompt_token_ids_commitment_v1},
    };
    use std::{
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };
    struct Dir(PathBuf);
    impl Dir {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path =
                std::env::temp_dir().join(format!("misaka-a2-store-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn put(&self, root: Digest, index: u64, encoding: StoredEncodingV2, bytes: &[u8]) {
            std::fs::write(self.0.join(source_file_name_v2(root, index, encoding)), bytes).unwrap();
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn limits() -> StoreLimitsV2 {
        StoreLimitsV2 {
            max_unit_bytes: 4096,
            max_cached_bytes: 16384,
            max_cached_units: 8,
            max_file_reads: 16,
            max_reserved_read_bytes: 32768,
        }
    }
    fn artifact() -> (Digest, Vec<PalwArtifactOperandV1>) {
        let operands: Vec<_> = (0..3)
            .map(|n| PalwArtifactOperandV1 {
                tensor_name: "embedding".into(),
                layer: None,
                row_start: n * 16,
                bytes: vec![n as u8; 16],
            })
            .collect();
        let root = artifact_root_v1(&operands.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).unwrap();
        (Digest(root.as_bytes()), operands)
    }
    #[test]
    fn real_g14_artifact_and_prompt_files_roundtrip_into_immutable_bounded_cache() {
        let dir = Dir::new();
        let (root, operands) = artifact();
        let opening = open_artifact_leaf_v1(&operands, 1).unwrap();
        dir.put(root, 1, StoredEncodingV2::Artifact, &borsh::to_vec(&opening).unwrap());
        let prompt: Vec<u32> = (0..65).collect();
        let tiled = Digest(prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::MerkleV1, &prompt).unwrap().as_bytes());
        let tile = prompt_ids_opening_v1(&prompt, 64).unwrap();
        dir.put(tiled, 2, StoredEncodingV2::PromptTile, &borsh::to_vec(&tile).unwrap());
        let flat = Digest(prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, &prompt).unwrap().as_bytes());
        dir.put(flat, 0, StoredEncodingV2::FlatPrompt, &borsh::to_vec(&prompt).unwrap());
        let mut provider = FileStoredProviderV2::open(&dir.0, limits()).unwrap();
        provider.load(root, 1, StoredEncodingV2::Artifact).unwrap();
        provider.load(tiled, 2, StoredEncodingV2::PromptTile).unwrap();
        provider.load(flat, 0, StoredEncodingV2::FlatPrompt).unwrap();
        assert_eq!(provider.unit(SourceKind::ArtifactInventory, root, 1).unwrap(), &StoredUnitV2::Artifact(opening));
        assert_eq!(provider.unit(SourceKind::Input, tiled, 2).unwrap(), &StoredUnitV2::Input(tile));
        assert_eq!(provider.unit(SourceKind::Input, flat, 0).unwrap(), &StoredUnitV2::FlatInput(prompt));
        let before = provider.meter();
        provider.load(root, 1, StoredEncodingV2::Artifact).unwrap();
        assert_eq!(provider.meter(), before);
        // A subsequent on-disk mutation cannot mutate an already borrowed cache.
        dir.put(root, 1, StoredEncodingV2::Artifact, b"bad");
        assert!(provider.unit(SourceKind::ArtifactInventory, root, 1).is_ok());
    }
    #[test]
    fn oversized_malformed_trailing_and_wrong_identity_files_are_refused() {
        let dir = Dir::new();
        let (root, operands) = artifact();
        let mut provider = FileStoredProviderV2::open(&dir.0, limits()).unwrap();
        dir.put(root, 0, StoredEncodingV2::Artifact, &vec![0; 4097]);
        assert_eq!(provider.load(root, 0, StoredEncodingV2::Artifact), Err(StoreErrorV2::Budget));
        assert_eq!(provider.meter().reserved_read_bytes, 0);
        dir.put(root, 0, StoredEncodingV2::Artifact, &u32::MAX.to_le_bytes());
        assert_eq!(provider.load(root, 0, StoredEncodingV2::Artifact), Err(StoreErrorV2::NonCanonical));
        let opening = open_artifact_leaf_v1(&operands, 0).unwrap();
        let mut bytes = borsh::to_vec(&opening).unwrap();
        bytes.push(0);
        dir.put(root, 0, StoredEncodingV2::Artifact, &bytes);
        assert_eq!(provider.load(root, 0, StoredEncodingV2::Artifact), Err(StoreErrorV2::NonCanonical));
        dir.put(root, 1, StoredEncodingV2::Artifact, &borsh::to_vec(&opening).unwrap());
        assert_eq!(provider.load(root, 1, StoredEncodingV2::Artifact), Err(StoreErrorV2::Authentication));
        assert_eq!(provider.meter().cached_units, 0);
    }
    #[test]
    fn failed_loads_and_cache_caps_cannot_reset_read_budgets() {
        let dir = Dir::new();
        let root = Digest([1; 64]);
        let mut cap = limits();
        cap.max_file_reads = 2;
        let mut provider = FileStoredProviderV2::open(&dir.0, cap).unwrap();
        assert!(matches!(provider.load(root, 0, StoredEncodingV2::Artifact), Err(StoreErrorV2::Io(_))));
        assert!(matches!(provider.load(root, 0, StoredEncodingV2::Artifact), Err(StoreErrorV2::Io(_))));
        assert_eq!(provider.load(root, 0, StoredEncodingV2::Artifact), Err(StoreErrorV2::Budget));
        let (root, operands) = artifact();
        for index in 0..2 {
            dir.put(
                root,
                index,
                StoredEncodingV2::Artifact,
                &borsh::to_vec(&open_artifact_leaf_v1(&operands, index as u32).unwrap()).unwrap(),
            );
        }
        let mut cap = limits();
        cap.max_cached_units = 1;
        let mut provider = FileStoredProviderV2::open(&dir.0, cap).unwrap();
        provider.load(root, 0, StoredEncodingV2::Artifact).unwrap();
        let before = provider.meter();
        assert_eq!(provider.load(root, 1, StoredEncodingV2::Artifact), Err(StoreErrorV2::Budget));
        assert_eq!(provider.meter(), before);
    }
    #[test]
    fn cache_body_and_read_reservation_limits_precede_reading_or_decoding() {
        let dir = Dir::new();
        let (root, operands) = artifact();
        let bytes = borsh::to_vec(&open_artifact_leaf_v1(&operands, 0).unwrap()).unwrap();
        dir.put(root, 0, StoredEncodingV2::Artifact, &bytes);
        for read_limited in [false, true] {
            let mut cap = limits();
            if read_limited {
                cap.max_reserved_read_bytes = bytes.len() as u64;
            } else {
                cap.max_cached_bytes = bytes.len() as u64 - 1;
            }
            let mut provider = FileStoredProviderV2::open(&dir.0, cap).unwrap();
            assert_eq!(provider.load(root, 0, StoredEncodingV2::Artifact), Err(StoreErrorV2::Budget));
            assert_eq!(provider.meter().file_reads, 1);
            assert_eq!(provider.meter().reserved_read_bytes, 0);
            assert_eq!(provider.meter().cached_units, 0);
        }
        let mut cap = limits();
        cap.max_file_reads = 0;
        assert!(matches!(FileStoredProviderV2::open(&dir.0, cap), Err(StoreErrorV2::Budget)));
    }

    #[test]
    fn symlinks_and_fifos_are_refused_without_following_or_blocking() {
        use std::os::unix::fs::symlink;
        let dir = Dir::new();
        let target = Dir::new();
        let root = Digest([2; 64]);
        let name = source_file_name_v2(root, 0, StoredEncodingV2::Artifact);
        std::fs::write(target.0.join("outside"), b"secret").unwrap();
        symlink(target.0.join("outside"), dir.0.join(&name)).unwrap();
        let mut provider = FileStoredProviderV2::open(&dir.0, limits()).unwrap();
        assert!(matches!(provider.load(root, 0, StoredEncodingV2::Artifact), Err(StoreErrorV2::Io(_))));
        std::fs::remove_file(dir.0.join(&name)).unwrap();
        use std::os::unix::ffi::OsStrExt;
        let name = std::ffi::CString::new(dir.0.join(name).as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        assert_eq!(provider.load(root, 0, StoredEncodingV2::Artifact), Err(StoreErrorV2::NotRegular));
    }
    #[test]
    fn malformed_lengths_are_rejected_before_protocol_sized_allocations() {
        for encoding in [StoredEncodingV2::Artifact, StoredEncodingV2::PromptTile, StoredEncodingV2::FlatPrompt] {
            for len in 0..16 {
                let mut bytes = vec![0xFF; len];
                assert!(decode(&bytes, encoding).is_err());
                bytes.push(0);
                assert!(decode(&bytes, encoding).is_err());
            }
        }
        let mut bytes = vec![0; 8];
        bytes.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(decode(&bytes, StoredEncodingV2::PromptTile), Err(StoreErrorV2::NonCanonical)));
    }
}
