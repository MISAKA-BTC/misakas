//! Durable responder acceleration. Neither a manifest nor a local tree is a
//! verifier credential: reopened IDs and every answer reach ORIGINAL roots.
use super::*;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::palw_backend::{
    PalwTirPublicPreparedDescriptorV1 as Descriptor, PalwTirPublicPreparedNeedV1 as Need, PalwTirPublicPreparedWriterV1,
};
use std::fs::OpenOptions;

const MANIFEST_MAX: u64 = 1024;
const FILES: [&str; 6] = ["capture.bin", "steps.bin", "offsets.bin", "rows.bin", "tiles.bin", "spool.bin"];
const MANIFEST_DOMAIN: &[u8] = b"misaka-palw/tir-prepared-manifest/v1";

// One node owns a cache root. This guard prevents in-process duplicate writers;
// cross-process sharing is NOT supported or claimed to have a resource bound.
type WriterKey = (std::path::PathBuf, Hash64, u64);
static WRITERS: std::sync::OnceLock<Mutex<std::collections::BTreeSet<WriterKey>>> = std::sync::OnceLock::new();
struct ActiveWriter(WriterKey);
impl ActiveWriter {
    fn acquire(root: &Path, source: Hash64, until: u64) -> Result<Self, String> {
        let key = (root.canonicalize().map_err(|e| e.to_string())?, source, until);
        if !WRITERS
            .get_or_init(|| Mutex::new(Default::default()))
            .lock()
            .map_err(|_| "prepared writer registry poisoned")?
            .insert(key.clone())
        {
            return Err("prepared source/horizon already has a live publisher".into());
        }
        Ok(Self(key))
    }
}
impl Drop for ActiveWriter {
    fn drop(&mut self) {
        if let Some(writers) = WRITERS.get()
            && let Ok(mut writers) = writers.lock()
        {
            writers.remove(&self.0);
        }
    }
}
struct Staging {
    path: std::path::PathBuf,
    _active: ActiveWriter,
}
impl Staging {
    fn path(&self) -> &Path {
        &self.path
    }
}
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = cleanup_partial(&self.path);
    }
}
fn cleanup_partial(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_dir() => {}
        Ok(_) => return Err("prepared partial namespace is not a directory".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.to_string()),
    }
    let entries = std::fs::read_dir(path)
        .map_err(|e| e.to_string())?
        .take(8)
        .map(|e| e.map(|e| e.file_name()).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    if entries.len() > 7
        || entries.iter().any(|name| {
            name.as_os_str() != std::ffi::OsStr::new("manifest.bin")
                && !FILES.iter().any(|known| name.as_os_str() == std::ffi::OsStr::new(known))
        })
    {
        return Err("prepared partial contains unrelated material".into());
    }
    for name in entries {
        std::fs::remove_file(path.join(name)).map_err(|e| e.to_string())?;
    }
    std::fs::remove_dir(path).map_err(|e| e.to_string())
}

#[derive(Clone, Copy, Default, BorshSerialize, BorshDeserialize)]
struct FileInfo {
    length: u64,
    digest: Hash64,
}
#[derive(BorshSerialize, BorshDeserialize)]
struct Manifest {
    version: u16,
    source_id: Hash64,
    material_until_daa: u64,
    rows_start: u64,
    rows_root: Option<Hash64>,
    files: [FileInfo; 6],
}

/// Position-independent access: exporting a Ready reader cannot race its seek
/// cursor while the original reader answers another request.
struct RangeFile {
    file: File,
    base: u64,
    length: u64,
}
impl PalwOriginalCaptureReaderV1 for RangeFile {
    fn len_v1(&self) -> u64 {
        self.length
    }
    fn read_v1(&self, at: u64, length: u32) -> Result<Vec<u8>, String> {
        if length > READ || at.checked_add(u64::from(length)).is_none_or(|end| end > self.length) {
            return Err("prepared original read exceeds its fixed bounded range".into());
        }
        let mut bytes = vec![0; length as usize];
        let mut read = 0;
        while read < bytes.len() {
            let offset = self.base.checked_add(at).and_then(|n| n.checked_add(read as u64)).ok_or("prepared read offset overflow")?;
            #[cfg(unix)]
            let n = std::os::unix::fs::FileExt::read_at(&self.file, &mut bytes[read..], offset);
            #[cfg(windows)]
            let n = std::os::windows::fs::FileExt::seek_read(&self.file, &mut bytes[read..], offset);
            let n = n.map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("prepared original file was truncated".into());
            }
            read += n;
        }
        Ok(bytes)
    }
}
fn range(file: File, base: u64, length: u64) -> Arc<dyn PalwOriginalCaptureReaderV1> {
    Arc::new(RangeFile { file, base, length })
}
fn error() -> String {
    "prepared TIR resource bound overflow".into()
}

pub(crate) fn prepared_need_v1(binding: &PalwTirStepBindingV1, source: &PalwTirPublicSourceV1, length: u64) -> Result<Need, String> {
    let base = need_v1(binding, source, length, 0)?;
    let metadata = borsh::object_length(binding).map_err(|e| e.to_string())? as u64;
    let rows = u64::from(source.decode_count_v1())
        .checked_mul(u64::from(source.vocab_v1()).checked_mul(4).and_then(|n| n.checked_add(8)).ok_or_else(error)?)
        .ok_or_else(error)?;
    let maximum_capture = metadata
        .checked_add(32)
        .and_then(|n| n.checked_add(u64::from(binding.job_context.declared_prefill_tokens).checked_mul(4)?))
        .and_then(|n| n.checked_add(rows))
        .and_then(|n| n.checked_add(source.leaf_count_v1().checked_mul(MAX_LEAF + LEAF_HEAD)?))
        .ok_or_else(error)?;
    if length == 0 || length > maximum_capture {
        return Err("prepared original range exceeds its native format geometry".into());
    }
    let disk = base.disk_bytes.checked_add(length).and_then(|n| n.checked_add(MANIFEST_MAX)).ok_or_else(error)?;
    let slices = disk.div_ceil(u64::from(READ)).checked_add(8).ok_or_else(error)?;
    Ok(Need {
        additional_ram_bytes: 131_072,
        additional_disk_bytes: disk.checked_mul(2).ok_or_else(error)?,
        maximum_retained_bytes: disk,
        maximum_file_operations: slices.checked_mul(4).and_then(|n| n.checked_add(64)).ok_or_else(error)?,
        maximum_publish_io_bytes: disk.checked_mul(2).and_then(|n| n.checked_add(4 * MANIFEST_MAX)).ok_or_else(error)?,
        maximum_publish_hash_bytes: disk.checked_add(slices.checked_mul(MANIFEST_MAX).ok_or_else(error)?).ok_or_else(error)?,
        maximum_publish_slices: slices,
        reopen_ram_bytes: base.resident_ram_bytes,
    })
}
pub(super) fn reader_need_v1(reader: &CaptureReader) -> Result<Need, String> {
    if !matches!(reader.phase, Phase::Ready) {
        return Err("only an original-root Ready reader can retain prepared custody".into());
    }
    prepared_need_v1(&reader.binding, &reader.public, reader.source.len_v1())
}
fn directory(root: &Path, descriptor: &Descriptor) -> std::path::PathBuf {
    root.join(format!("prepared-{}", descriptor.manifest_digest))
}
fn regular(path: &Path, length: u64) -> Result<File, String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.file_type().is_file() || metadata.len() != length {
        return Err("prepared file type or exact length differs".into());
    }
    File::open(path).map_err(|e| e.to_string())
}
fn manifest(root: &Path, descriptor: &Descriptor) -> Result<Manifest, String> {
    let path = directory(root, descriptor);
    if !std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?.file_type().is_dir() {
        return Err("prepared source is not a sealed directory".into());
    }
    let file = File::open(path.join("manifest.bin")).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(MANIFEST_MAX + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MANIFEST_MAX || hash_scope(MANIFEST_DOMAIN, &bytes)? != descriptor.manifest_digest {
        return Err("prepared manifest does not match the exact Ready descriptor".into());
    }
    let m: Manifest = borsh::from_slice(&bytes).map_err(|e| e.to_string())?;
    let total = m.files.iter().try_fold(bytes.len() as u64, |n, f| n.checked_add(f.length).ok_or_else(error))?;
    if m.version != 1
        || m.source_id != descriptor.source_id
        || m.material_until_daa != descriptor.material_until_daa
        || total != descriptor.retained_bytes
        || m.files.iter().any(|f| (f.length == 0) != (f.digest == Hash64::default()))
    {
        return Err("prepared manifest changes its source, horizon or exact byte geometry".into());
    }
    Ok(m)
}

pub(crate) fn retire_prepared_v1(descriptor: &Descriptor, root: &Path, native_until: u64, irreversible: u64) -> Result<(), String> {
    if descriptor.material_until_daa != native_until || irreversible <= native_until {
        return Err("prepared retirement has no exact irreversible native expiry authority".into());
    }
    let path = directory(root, descriptor);
    match std::fs::symlink_metadata(&path) {
        Ok(m) if m.file_type().is_dir() => {}
        Ok(_) => return Err("prepared retirement refuses a non-directory namespace".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.to_string()),
    }
    let entries = match std::fs::read_dir(&path) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.to_string()),
    };
    // Fixed six payloads and one manifest; never recurse into an arbitrary tree.
    let names = entries.take(8).map(|entry| entry.map(|e| e.file_name()).map_err(|e| e.to_string())).collect::<Result<Vec<_>, _>>()?;
    if names.is_empty() {
        return std::fs::remove_dir(&path).map_err(|e| e.to_string());
    }
    if names.len() > 7
        || names.iter().any(|name| {
            name.as_os_str() != std::ffi::OsStr::new("manifest.bin")
                && !FILES.iter().any(|known| name.as_os_str() == std::ffi::OsStr::new(known))
        })
    {
        return Err("prepared retirement encountered unrelated files".into());
    }
    // Native exact descriptor + irreversible expiry authorizes this namespace;
    // even a corrupt manifest must not veto retirement or become clock authority.
    for name in FILES {
        match std::fs::remove_file(path.join(name)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    std::fs::remove_file(path.join("manifest.bin")).map_err(|e| e.to_string())?;
    std::fs::remove_dir(&path).map_err(|e| e.to_string())?;
    File::open(root).and_then(|f| f.sync_all()).map_err(|e| e.to_string())
}

struct Writer {
    staging: Option<Staging>,
    root: std::path::PathBuf,
    descriptor: Descriptor,
    manifest: Manifest,
    inputs: Vec<Option<Arc<dyn PalwOriginalCaptureReaderV1>>>,
    outputs: Vec<Option<File>>,
    hashes: Vec<blake2b_simd::State>,
    file: usize,
    offset: u64,
    progress: PalwTirPublicCaptureProgressV1,
    completed: bool,
    failed: bool,
}
pub(super) fn begin_v1(
    reader: &CaptureReader,
    root: &Path,
    until: u64,
    now: u64,
) -> Result<Box<dyn PalwTirPublicPreparedWriterV1>, String> {
    let need = reader_need_v1(reader)?;
    if now >= until {
        return Err("prepared original custody is outside its native retention window".into());
    }
    let mut inputs = vec![None; FILES.len()];
    inputs[0] = Some(reader.source.clone());
    let (file, base, length) = reader.steps.file_snapshot_v1()?;
    inputs[1] = Some(range(file, base, length));
    inputs[2] = Some(range(
        reader.offsets.try_clone().map_err(|e| e.to_string())?,
        0,
        reader.public.leaf_count_v1().checked_mul(8).ok_or_else(error)?,
    ));
    if let Some(rows) = &reader.rows {
        let (file, base, length) = rows.file_snapshot_v1()?;
        inputs[3] = Some(range(file, base, length));
    }
    if let Some(file) = &reader.forest {
        inputs[4] = Some(range(file.try_clone().map_err(|e| e.to_string())?, 0, file.metadata().map_err(|e| e.to_string())?.len()));
    }
    if let Some(spool) = &reader.spool {
        let held = spool.0.lock().map_err(|_| "prepared spool poisoned")?;
        inputs[5] = Some(range(held.0.try_clone().map_err(|e| e.to_string())?, 0, held.1));
    }
    let files = std::array::from_fn(|i| FileInfo { length: inputs[i].as_ref().map_or(0, |r| r.len_v1()), digest: Hash64::default() });
    if files.iter().try_fold(MANIFEST_MAX, |n, f| n.checked_add(f.length).ok_or_else(error))? > need.maximum_retained_bytes {
        return Err("prepared export exceeds its reserved native format bound".into());
    }
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let active = ActiveWriter::acquire(root, reader.public.source_id_v1(), until)?;
    let partial = root.join(format!("preparing-{}-{until}", reader.public.source_id_v1()));
    // This unpublished acceleration can be reconstructed from the original
    // source. At most one partial survives restart per native source/horizon.
    cleanup_partial(&partial)?;
    std::fs::create_dir(&partial).map_err(|e| e.to_string())?;
    let staging = Staging { path: partial, _active: active };
    let outputs = FILES
        .iter()
        .enumerate()
        .map(|(i, name)| {
            if files[i].length == 0 {
                Ok(None)
            } else {
                OpenOptions::new().write(true).create_new(true).open(staging.path().join(name)).map(Some).map_err(|e| e.to_string())
            }
        })
        .collect::<Result<Vec<_>, String>>()?;
    let header = reader.public.header_v1();
    Ok(Box::new(Writer {
        staging: Some(staging),
        root: root.to_path_buf(),
        descriptor: Descriptor {
            version: 1,
            source_id: reader.public.source_id_v1(),
            class_id: header.job_context.shape_profile_id,
            artifact_root: header.artifact_root,
            execution_root: header.committed_execution_root,
            trace_root: header.full_logits_trace_root,
            step_root: header.step_merkle_root,
            step_leaf_count: header.step_leaf_count,
            decode_count: reader.public.decode_count_v1(),
            vocab: reader.public.vocab_v1(),
            material_until_daa: until,
            manifest_digest: Hash64::default(),
            retained_bytes: 0,
        },
        manifest: Manifest {
            version: 1,
            source_id: reader.public.source_id_v1(),
            material_until_daa: until,
            rows_start: reader.rows_start,
            rows_root: reader.rows_root,
            files,
        },
        inputs,
        outputs,
        hashes: (0..FILES.len())
            .map(|_| blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/tir-prepared-file/v1").to_state())
            .collect(),
        file: 0,
        offset: 0,
        progress: PalwTirPublicCaptureProgressV1::default(),
        completed: false,
        failed: false,
    }))
}
impl Writer {
    fn next(&self) -> usize {
        (self.file..FILES.len()).find(|i| self.manifest.files[*i].length > 0).unwrap_or(FILES.len())
    }
    fn poll(&mut self) -> Result<Option<Descriptor>, String> {
        if self.failed {
            return Err("prepared publisher previously failed; start a newly paid export".into());
        }
        if self.completed {
            return Ok(Some(self.descriptor));
        }
        let i = self.next();
        if i < FILES.len() {
            self.file = i;
            let n = (self.manifest.files[i].length - self.offset).min(u64::from(READ)) as u32;
            let bytes = self.inputs[i].as_ref().ok_or("prepared input missing")?.read_v1(self.offset, n)?;
            if bytes.len() != n as usize {
                return Err("prepared original input returned a short slice".into());
            }
            let output = self.outputs[i].as_mut().ok_or("prepared output missing")?;
            output.write_all(&bytes).map_err(|e| e.to_string())?;
            self.hashes[i].update(&bytes);
            self.offset += u64::from(n);
            if self.offset == self.manifest.files[i].length {
                output.sync_all().map_err(|e| e.to_string())?;
                self.manifest.files[i].digest = Hash64::from_bytes(self.hashes[i].finalize().as_bytes().try_into().expect("64"));
                self.file += 1;
                self.offset = 0;
            }
            return Ok(None);
        }
        let bytes = borsh::to_vec(&self.manifest).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MANIFEST_MAX {
            return Err("prepared fixed manifest exceeds its codec bound".into());
        }
        self.descriptor.manifest_digest = hash_scope(MANIFEST_DOMAIN, &bytes)?;
        self.descriptor.retained_bytes =
            self.manifest.files.iter().try_fold(bytes.len() as u64, |n, f| n.checked_add(f.length).ok_or_else(error))?;
        let staging = self.staging.as_ref().ok_or("prepared staging missing")?;
        let mut manifest_file =
            OpenOptions::new().write(true).create_new(true).open(staging.path().join("manifest.bin")).map_err(|e| e.to_string())?;
        manifest_file.write_all(&bytes).and_then(|_| manifest_file.sync_all()).map_err(|e| e.to_string())?;
        File::open(staging.path()).and_then(|f| f.sync_all()).map_err(|e| e.to_string())?;
        let target = directory(&self.root, &self.descriptor);
        if target.try_exists().map_err(|e| e.to_string())? {
            // A retry can repair only the SAME immutable manifest. Atomic file
            // replacements carry identical promised hashes, never another paid
            // source or horizon. Concurrent readers still authenticate each use.
            let old = manifest(&self.root, &self.descriptor)?;
            if borsh::to_vec(&old).map_err(|e| e.to_string())? != bytes {
                return Err("prepared retry changes sealed manifest".into());
            }
            for (i, name) in FILES.iter().enumerate() {
                if self.manifest.files[i].length > 0 {
                    std::fs::rename(staging.path().join(name), target.join(name)).map_err(|e| e.to_string())?;
                }
            }
            File::open(&target).and_then(|f| f.sync_all()).map_err(|e| e.to_string())?;
        } else {
            std::fs::rename(staging.path(), &target).map_err(|e| e.to_string())?;
            File::open(&self.root).and_then(|f| f.sync_all()).map_err(|e| e.to_string())?;
        }
        self.staging.take();
        self.completed = true;
        Ok(Some(self.descriptor))
    }
}
impl PalwTirPublicPreparedWriterV1 for Writer {
    fn slice_quote_v1(&self) -> Result<PalwTirPublicCaptureSliceV1, String> {
        let i = self.next();
        let n = if !self.failed && !self.completed && i < FILES.len() {
            (self.manifest.files[i].length - self.offset).min(u64::from(READ))
        } else {
            MANIFEST_MAX * 2
        };
        let cursor_id = hash_scope(
            b"misaka-palw/tir-prepared-write/v1",
            &(
                self.descriptor.source_id,
                self.descriptor.material_until_daa,
                self.progress.sequence,
                i as u8,
                self.offset,
                self.failed,
                self.completed,
            ),
        )?;
        Ok(PalwTirPublicCaptureSliceV1 {
            source_id: self.descriptor.source_id,
            request_id: Hash64::default(),
            sequence: self.progress.sequence,
            phase: 12,
            cursor_id,
            work: PalwTirPublicCaptureWorkV1 { io_bytes: n * 2, hash_bytes: n + MANIFEST_MAX, ..Default::default() },
            resident_ram_bytes: 131_072,
        })
    }
    fn progress_v1(&self) -> PalwTirPublicCaptureProgressV1 {
        self.progress
    }
    fn poll_quoted_v1(&mut self, quote: &PalwTirPublicCaptureSliceV1) -> Result<Option<Descriptor>, String> {
        if *quote != self.slice_quote_v1()? {
            return Err("prepared publisher permit names a stale or different slice".into());
        }
        self.progress = PalwTirPublicCaptureProgressV1 {
            sequence: self.progress.sequence.checked_add(1).ok_or_else(error)?,
            charged: self.progress.charged.checked_add_v1(quote.work)?,
            last_slice: Some(*quote),
        };
        let result = self.poll();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
}

pub(crate) fn reopen_prepared_v1(
    binding: PalwTirStepBindingV1,
    public: PalwTirPublicSourceV1,
    descriptor: &Descriptor,
    root: &Path,
    native_until: u64,
    native_now: u64,
    form: PalwPromptIdsFormV1,
) -> Result<CaptureReader, String> {
    let header = public.header_v1();
    if descriptor.version != 1
        || descriptor.source_id != public.source_id_v1()
        || descriptor.class_id != header.job_context.shape_profile_id
        || descriptor.artifact_root != header.artifact_root
        || descriptor.execution_root != header.committed_execution_root
        || descriptor.trace_root != header.full_logits_trace_root
        || descriptor.step_root != header.step_merkle_root
        || descriptor.step_leaf_count != header.step_leaf_count
        || descriptor.decode_count != public.decode_count_v1()
        || descriptor.vocab != public.vocab_v1()
        || descriptor.material_until_daa != native_until
        || native_now >= native_until
    {
        return Err("prepared Ready descriptor changes original native source/geometry/horizon or has expired".into());
    }
    let m = manifest(root, descriptor)?;
    let prepared_need = prepared_need_v1(&binding, &public, m.files[0].length)?;
    if descriptor.retained_bytes > prepared_need.maximum_retained_bytes {
        return Err("prepared retained bytes exceed native source bound".into());
    }
    let path = directory(root, descriptor);
    let files = m
        .files
        .iter()
        .enumerate()
        .map(|(i, info)| if info.length == 0 { Ok(None) } else { regular(&path.join(FILES[i]), info.length).map(Some) })
        .collect::<Result<Vec<_>, String>>()?;
    let mut files = files.into_iter();
    let capture = files.next().flatten().ok_or("prepared original capture absent")?;
    let steps_file = files.next().flatten().ok_or("prepared step index absent")?;
    let offsets = files.next().flatten().ok_or("prepared offset index absent")?;
    let rows_file = files.next().flatten();
    let forest = files.next().flatten();
    let spool_file = files.next().flatten();
    let n = public.leaf_count_v1();
    let d = u64::from(public.decode_count_v1());
    let tile_stride = tree_disk_bytes_v1(u64::from(public.vocab_v1()).div_ceil(PALW_LOGITS_TILE_LANES as u64))?;
    if m.files[1].length != tree_disk_bytes_v1(n)?
        || m.files[2].length != n.checked_mul(8).ok_or_else(error)?
        || m.files[5].length > n.checked_mul(MAX_LEAF + LEAF_HEAD).ok_or_else(error)?
    {
        return Err("prepared step/spool index changes original geometry".into());
    }
    let tiled = public.supports_tiled_trace_v1();
    let expected_rows_start = (super::super::backend::TIR_CAPTURE_MAGIC_V1.len() as u64)
        .checked_add(borsh::object_length(&binding).map_err(|e| e.to_string())? as u64)
        .and_then(|n| n.checked_add(8))
        .and_then(|n| n.checked_add(u64::from(binding.job_context.declared_prefill_tokens).checked_mul(4)?))
        .ok_or_else(error)?;
    if m.rows_start != expected_rows_start {
        return Err("prepared rows do not begin at the original capture geometry".into());
    }
    if tiled
        && (m.files[3].length != tree_disk_bytes_v1(d)?
            || m.files[4].length != tile_stride.checked_mul(d).ok_or_else(error)?
            || m.rows_root.is_none())
        || !tiled && (m.files[3].length != 0 || m.files[4].length != 0 || m.rows_root.is_some())
    {
        return Err("prepared row/tile index changes original codec geometry".into());
    }
    let id_offset = m
        .rows_start
        .checked_add(
            d.checked_mul(u64::from(public.vocab_v1()).checked_mul(4).and_then(|n| n.checked_add(4)).ok_or_else(error)?)
                .ok_or_else(error)?,
        )
        .and_then(|n| n.checked_add(4))
        .ok_or_else(error)?;
    if tiled && id_offset.checked_add(d.checked_mul(4).ok_or_else(error)?).is_none_or(|end| end > m.files[0].length) {
        return Err("prepared ID range exceeds original exact capture".into());
    }
    let source = range(capture, 0, m.files[0].length);
    let need = need_v1(&binding, &public, source.len_v1(), 0)?;
    let metadata_bytes = borsh::object_length(&binding).map_err(|e| e.to_string())? as u64;
    let steps = DiskTree::slice(&steps_file, 0, n, Some(binding.step_merkle_root))?;
    let rows = rows_file.map(|f| DiskTree::slice(&f, 0, d, m.rows_root)).transpose()?;
    let spool = spool_file.map(|f| Arc::new(Spool(Mutex::new((f, m.files[5].length)))));
    Ok(CaptureReader {
        source,
        binding,
        public,
        expected: Vec::new(),
        form,
        at: id_offset,
        phase: if tiled { Phase::Ids } else { Phase::Ready },
        prompt: Vec::new(),
        ids: Vec::new(),
        row: descriptor.decode_count,
        tile: 0,
        rows_start: m.rows_start,
        row_started: false,
        rows_root: m.rows_root,
        flat: None,
        steps,
        rows,
        forest,
        tile_stride,
        active_tiles: None,
        offsets,
        leaf: n,
        replay: None,
        replay_point: None,
        replay_ids: None,
        replay_trace: None,
        spool,
        directory: root.to_path_buf(),
        need,
        metadata_bytes,
        replay_work: None,
        progress: PalwTirPublicCaptureProgressV1::default(),
        prepared_reopen: true,
        prepared_descriptor: Some(*descriptor),
    })
}
