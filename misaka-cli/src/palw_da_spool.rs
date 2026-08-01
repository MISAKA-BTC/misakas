//! Convert the Qwen lifecycle `export --node-context` artifact into kaspad's local-only spool pair.

use crate::{CliError, CliResult, OutputFormat};
use kaspa_consensus_core::palw::da::{PALW_DA_MAX_OBJECT_BYTES, PALW_RECEIPT_DA_OBJECT_VERSION_V2, palw_receipt_da_commitment};
use serde::Serialize;
use serde_json::Value;
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::{
    ffi::CString,
    fs::OpenOptions,
    io::{Read, Write},
};

#[cfg(unix)]
use std::os::unix::{
    ffi::OsStrExt,
    fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
};

const MAX_ARTIFACT_BYTES: u64 = 2 * 1024 * 1024;
const STALE_TEMP_MIN_AGE_NANOS: u128 = 10 * 60 * 1_000_000_000;
const MAX_STALE_TEMP_SCAN_ENTRIES: usize = 4_096;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize)]
struct SpoolEntry<'a> {
    schema: &'static str,
    batch_id: &'a str,
    leaf_index: u32,
    object_root: String,
    object_len: u32,
}

fn error(message: impl Into<String>) -> CliError {
    CliError::generic(message)
}

#[cfg(unix)]
fn ensure_secure_dir(path: &Path) -> Result<(), CliError> {
    if !path.exists() {
        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700).recursive(false);
        builder.create(path).map_err(|e| error(format!("cannot create {}: {e}", path.display())))?;
    }
    let metadata = fs::symlink_metadata(path).map_err(|e| error(format!("cannot stat {}: {e}", path.display())))?;
    // SAFETY: geteuid has no preconditions and retains no pointer.
    let uid = unsafe { libc::geteuid() };
    if !metadata.is_dir() || metadata.file_type().is_symlink() || metadata.uid() != uid || metadata.mode() & 0o077 != 0 {
        return Err(error(format!("spool directory must be a real owner-only directory (0700): {}", path.display())));
    }
    Ok(())
}

#[cfg(not(unix))]
fn ensure_secure_dir(_path: &Path) -> Result<(), CliError> {
    Err(error("PALW DA spool enqueue requires Unix owner/mode security checks"))
}

#[cfg(unix)]
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), CliError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|e| error(format!("refusing/cannot create {}: {e}", path.display())))?;
    file.write_all(bytes).and_then(|()| file.sync_all()).map_err(|e| error(format!("cannot durably write {}: {e}", path.display())))
}

#[cfg(not(unix))]
fn write_new(_path: &Path, _bytes: &[u8]) -> Result<(), CliError> {
    Err(error("PALW DA spool enqueue requires Unix owner/mode security checks"))
}

#[cfg(unix)]
fn path_cstring(path: &Path) -> io::Result<CString> {
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, format!("path contains NUL: {}", path.display())))
}

#[cfg(target_os = "linux")]
fn atomic_rename_noreplace(source: &Path, destination: &Path) -> io::Result<()> {
    let source = path_cstring(source)?;
    let destination = path_cstring(destination)?;
    // Invoked as a raw syscall, not via `libc::renameat2`: that binding exists only for
    // `target_env = "gnu"` in our pinned libc, while this `cfg` also covers the musl release
    // target (the musl build failed with `E0425: cannot find function renameat2`). A libc bump
    // would only turn that into a link error, since musl gained the wrapper in 1.2.6 and this
    // repository's toolchain predates it. `SYS_renameat2`/`RENAME_NOREPLACE` exist for both
    // envs, so this keeps the atomic no-replace semantics identical on gnu and musl.
    // SAFETY: both C strings remain live for the syscall and AT_FDCWD borrows no descriptor.
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    // -1 on failure (errno set); ENOSYS/EINVAL on an unsupported kernel/fs keeps the
    // documented fail-closed behaviour.
    if result == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}

#[cfg(target_os = "macos")]
fn atomic_rename_noreplace(source: &Path, destination: &Path) -> io::Result<()> {
    let source = path_cstring(source)?;
    let destination = path_cstring(destination)?;
    // SAFETY: both C strings remain live for the call; RENAME_EXCL is atomic no-replace.
    let result = unsafe { libc::renamex_np(source.as_ptr(), destination.as_ptr(), libc::RENAME_EXCL) };
    if result == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn atomic_rename_noreplace(_source: &Path, _destination: &Path) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "atomic no-replace rename is unavailable on this platform"))
}

#[cfg(unix)]
fn read_secure_existing(path: &Path, cap: usize) -> Result<Vec<u8>, CliError> {
    let before = fs::symlink_metadata(path).map_err(|e| error(format!("cannot securely stat existing {}: {e}", path.display())))?;
    // SAFETY: geteuid has no preconditions and retains no pointer.
    let uid = unsafe { libc::geteuid() };
    if !before.file_type().is_file()
        || before.file_type().is_symlink()
        || before.uid() != uid
        || before.mode() & 0o077 != 0
        || before.nlink() != 1
        || before.len() > cap as u64
    {
        return Err(error(format!("existing spool path is not a secure owner-only single-link file: {}", path.display())));
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|e| error(format!("cannot securely open existing {}: {e}", path.display())))?;
    let after = file.metadata().map_err(|e| error(format!("cannot stat opened existing {}: {e}", path.display())))?;
    if before.dev() != after.dev() || before.ino() != after.ino() {
        return Err(error(format!("existing spool path changed while opening: {}", path.display())));
    }
    let mut bytes = Vec::with_capacity(after.len() as usize);
    Read::by_ref(&mut file)
        .take((cap + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| error(format!("cannot read existing {}: {e}", path.display())))?;
    if bytes.len() > cap {
        return Err(error(format!("existing spool path exceeds {cap} bytes: {}", path.display())));
    }
    Ok(bytes)
}

#[cfg(not(unix))]
fn read_secure_existing(_path: &Path, _cap: usize) -> Result<Vec<u8>, CliError> {
    Err(error("PALW DA spool enqueue requires Unix owner/mode security checks"))
}

fn publish_or_verify_existing(tmp: &Path, final_path: &Path, expected: &[u8], label: &str) -> Result<(), CliError> {
    match atomic_rename_noreplace(tmp, final_path) {
        Ok(()) => Ok(()),
        Err(rename_error) if rename_error.kind() == io::ErrorKind::AlreadyExists => {
            let _ = fs::remove_file(tmp);
            let existing = read_secure_existing(final_path, expected.len())?;
            if existing != expected {
                return Err(error(format!("refusing mismatched existing spool {label}: {}", final_path.display())));
            }
            Ok(())
        }
        Err(rename_error) => {
            let _ = fs::remove_file(tmp);
            Err(error(format!("cannot publish spool {label} without overwrite: {rename_error}")))
        }
    }
}

fn now_nanos() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_nanos()).unwrap_or_default()
}

fn spool_temp_timestamp(name: &str) -> Option<u128> {
    let body = name.strip_prefix('.')?;
    let body = body.strip_suffix(".palwda.tmp").or_else(|| body.strip_suffix(".json.tmp"))?;
    let (root_hex, token) = body.split_once('.')?;
    if root_hex.len() != 128 || !root_hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut fields = token.split('-');
    fields.next()?.parse::<u32>().ok()?;
    let timestamp = fields.next()?.parse::<u128>().ok()?;
    fields.next()?.parse::<u64>().ok()?;
    (fields.next().is_none()).then_some(timestamp)
}

fn cleanup_stale_temp_prefixes(incoming: &Path, now: u128) -> Result<usize, CliError> {
    let entries = fs::read_dir(incoming).map_err(|e| error(format!("cannot scan spool temp prefixes {}: {e}", incoming.display())))?;
    let mut removed = 0;
    for (index, entry) in entries.enumerate() {
        if index >= MAX_STALE_TEMP_SCAN_ENTRIES {
            return Err(error(format!(
                "refusing unbounded spool temp-prefix scan in {} (>{MAX_STALE_TEMP_SCAN_ENTRIES} entries)",
                incoming.display()
            )));
        }
        let entry = entry.map_err(|e| error(format!("cannot scan spool temp prefix in {}: {e}", incoming.display())))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(timestamp) = spool_temp_timestamp(name) else { continue };
        if now.saturating_sub(timestamp) < STALE_TEMP_MIN_AGE_NANOS {
            continue;
        }
        let path = entry.path();
        // Reuse the same no-symlink/uid/mode/single-link/open-inode checks as crash-prefix resume.
        // A malformed or insecure matching prefix fails the enqueue instead of being followed or
        // silently left as attacker-controlled state.
        let _ = read_secure_existing(&path, PALW_DA_MAX_OBJECT_BYTES)?;
        fs::remove_file(&path).map_err(|e| error(format!("cannot remove securely checked stale temp {}: {e}", path.display())))?;
        removed += 1;
    }
    if removed != 0 {
        fs::File::open(incoming)
            .and_then(|directory| directory.sync_all())
            .map_err(|e| error(format!("cannot sync stale temp cleanup in {}: {e}", incoming.display())))?;
    }
    Ok(removed)
}

fn temp_token() -> String {
    let nanos = now_nanos();
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{}-{nanos}-{sequence}", std::process::id())
}

fn string_at<'a>(value: &'a Value, path: &[&str]) -> Result<&'a str, CliError> {
    let mut cursor = value;
    for field in path {
        cursor = cursor.get(*field).ok_or_else(|| error(format!("Qwen artifact is missing {}", path.join("."))))?;
    }
    cursor.as_str().ok_or_else(|| error(format!("Qwen artifact field {} must be a string", path.join("."))))
}

fn u64_at(value: &Value, path: &[&str]) -> Result<u64, CliError> {
    let mut cursor = value;
    for field in path {
        cursor = cursor.get(*field).ok_or_else(|| error(format!("Qwen artifact is missing {}", path.join("."))))?;
    }
    cursor.as_u64().ok_or_else(|| error(format!("Qwen artifact field {} must be an unsigned integer", path.join("."))))
}

fn decode_hex(value: &str, field: &str, max_bytes: usize) -> Result<Vec<u8>, CliError> {
    if !value.len().is_multiple_of(2) || value.len() / 2 > max_bytes {
        return Err(error(format!("{field} has an invalid/oversize hex length")));
    }
    let mut bytes = vec![0u8; value.len() / 2];
    faster_hex::hex_decode(value.as_bytes(), &mut bytes).map_err(|_| error(format!("{field} is not hexadecimal")))?;
    Ok(bytes)
}

pub fn enqueue(output: OutputFormat, artifact_path: &str, spool_root: &str) -> CliResult {
    let artifact_path = PathBuf::from(artifact_path);
    let artifact_metadata = fs::symlink_metadata(&artifact_path)
        .map_err(|e| error(format!("cannot stat Qwen artifact {}: {e}", artifact_path.display())))?;
    if !artifact_metadata.is_file() || artifact_metadata.file_type().is_symlink() || artifact_metadata.len() > MAX_ARTIFACT_BYTES {
        return Err(error(format!("Qwen artifact must be a regular non-symlink file <= {MAX_ARTIFACT_BYTES} bytes")));
    }
    let artifact: Value = serde_json::from_slice(
        &fs::read(&artifact_path).map_err(|e| error(format!("cannot read Qwen artifact {}: {e}", artifact_path.display())))?,
    )
    .map_err(|e| error(format!("invalid Qwen lifecycle JSON: {e}")))?;
    if string_at(&artifact, &["schema"])? != "misaka.palw.lifecycle-receipt-v3-node-da-bridge.v2"
        || string_at(&artifact, &["bridge_object", "schema"])? != "misaka.palw.receipt-da-object.v2"
        || u64_at(&artifact, &["da", "object_version"])? != u64::from(PALW_RECEIPT_DA_OBJECT_VERSION_V2)
        || artifact.pointer("/da/node_da_object_compatible").and_then(Value::as_bool) != Some(true)
        || artifact.pointer("/enforcement/node_admission_required").and_then(Value::as_bool) != Some(true)
    {
        return Err(error("artifact is not an exact Qwen --node-context Object-v2 export requiring node admission"));
    }

    let batch_id = string_at(&artifact, &["selected_chain_context", "batch_id"])?;
    if batch_id.len() != 128 || decode_hex(batch_id, "selected_chain_context.batch_id", 64)?.len() != 64 {
        return Err(error("selected_chain_context.batch_id must be 64 bytes"));
    }
    let leaf_index = u32::try_from(u64_at(&artifact, &["selected_chain_context", "leaf_index"])?)
        .map_err(|_| error("selected_chain_context.leaf_index exceeds u32"))?;
    let object =
        decode_hex(string_at(&artifact, &["bridge_object", "bytes_hex"])?, "bridge_object.bytes_hex", PALW_DA_MAX_OBJECT_BYTES)?;
    let version = object
        .get(..2)
        .and_then(|prefix| prefix.try_into().ok())
        .map(u16::from_le_bytes)
        .ok_or_else(|| error("bridge_object.bytes_hex has no Object-v2 version prefix"))?;
    if version != PALW_RECEIPT_DA_OBJECT_VERSION_V2 {
        return Err(error("bridge_object.bytes_hex is not Object-v2"));
    }
    let commitment = palw_receipt_da_commitment(version, &object).map_err(|e| error(format!("invalid Object-v2 commitment: {e}")))?;
    let root_hex = faster_hex::hex_string(commitment.root.as_byte_slice());
    if string_at(&artifact, &["da", "root"])? != root_hex
        || u64_at(&artifact, &["da", "object_len"])? != u64::from(commitment.object_len)
        || u64_at(&artifact, &["da", "chunk_count"])? != u64::from(commitment.chunk_count)
    {
        return Err(error("artifact DA metadata does not match bridge_object.bytes_hex"));
    }

    let root = PathBuf::from(spool_root);
    if !root.is_absolute() {
        return Err(error("--spool-dir must be an absolute path matching kaspad --palw-da-import-dir"));
    }
    ensure_secure_dir(&root)?;
    let incoming = root.join("incoming");
    ensure_secure_dir(&incoming)?;
    cleanup_stale_temp_prefixes(&incoming, now_nanos())?;

    let metadata = SpoolEntry {
        schema: "misaka.palw.da-spool-entry.v1",
        batch_id,
        leaf_index,
        object_root: root_hex.clone(),
        object_len: commitment.object_len,
    };
    let metadata_bytes = serde_json::to_vec_pretty(&metadata).map_err(|e| error(format!("cannot encode spool metadata: {e}")))?;
    let temp_token = temp_token();
    let object_tmp = incoming.join(format!(".{root_hex}.{temp_token}.palwda.tmp"));
    let metadata_tmp = incoming.join(format!(".{root_hex}.{temp_token}.json.tmp"));
    let object_final = incoming.join(format!("{root_hex}.palwda"));
    let metadata_final = incoming.join(format!("{root_hex}.json"));
    if let Err(e) = write_new(&object_tmp, &object) {
        let _ = fs::remove_file(&object_tmp);
        return Err(e);
    }
    publish_or_verify_existing(&object_tmp, &object_final, &object, "object")?;
    fs::File::open(&incoming)
        .and_then(|directory| directory.sync_all())
        .map_err(|e| error(format!("cannot sync published spool object in {}: {e}", incoming.display())))?;

    // Metadata is the daemon's ready marker. Create and publish it only after the object rename is
    // durable, and never delete the final object on a later failure: a same-UID racer could have
    // replaced that path, while an object-only crash prefix is safely ignored by kaspad.
    if let Err(e) = write_new(&metadata_tmp, &metadata_bytes) {
        let _ = fs::remove_file(&metadata_tmp);
        return Err(e);
    }
    publish_or_verify_existing(&metadata_tmp, &metadata_final, &metadata_bytes, "metadata ready marker")?;
    fs::File::open(&incoming)
        .and_then(|directory| directory.sync_all())
        .map_err(|e| error(format!("cannot sync spool directory {}: {e}", incoming.display())))?;

    if output == OutputFormat::Json {
        println!(
            "{}",
            serde_json::json!({
                "ok": true,
                "schema": "misaka.palw.da-spool-enqueue-result.v1",
                "job": root_hex,
                "batchId": batch_id,
                "leafIndex": leaf_index,
                "objectLen": commitment.object_len,
                "incoming": metadata_final,
            })
        );
    } else {
        println!("Queued PALW Object-v2 {root_hex} for local node admission ({batch_id}:{leaf_index})");
        println!("Metadata: {}", metadata_final.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[cfg(unix)]
    fn qwen_fixture(temp: &TempDir) -> (PathBuf, PathBuf, String, String, Vec<u8>) {
        let spool = temp.path().join("spool");
        let artifact_path = temp.path().join("qwen.json");
        let mut object = vec![0x42; 20_000];
        object[..2].copy_from_slice(&PALW_RECEIPT_DA_OBJECT_VERSION_V2.to_le_bytes());
        let commitment = palw_receipt_da_commitment(PALW_RECEIPT_DA_OBJECT_VERSION_V2, &object).unwrap();
        let root = faster_hex::hex_string(commitment.root.as_byte_slice());
        let batch = "02".repeat(64);
        let artifact = serde_json::json!({
            "schema": "misaka.palw.lifecycle-receipt-v3-node-da-bridge.v2",
            "selected_chain_context": { "batch_id": batch, "leaf_index": 7 },
            "bridge_object": { "schema": "misaka.palw.receipt-da-object.v2", "bytes_hex": faster_hex::hex_string(&object) },
            "da": {
                "object_version": 2,
                "object_len": commitment.object_len,
                "chunk_count": commitment.chunk_count,
                "root": root,
                "node_da_object_compatible": true
            },
            "enforcement": { "node_admission_required": true }
        });
        fs::write(&artifact_path, serde_json::to_vec(&artifact).unwrap()).unwrap();
        (spool, artifact_path, root, batch, object)
    }

    #[cfg(unix)]
    fn assert_no_temp_prefixes(incoming: &Path) {
        let leftovers = fs::read_dir(incoming)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .filter(|name| name.to_string_lossy().ends_with(".tmp"))
            .collect::<Vec<_>>();
        assert!(leftovers.is_empty(), "enqueue left temporary prefixes behind: {leftovers:?}");
    }

    #[cfg(unix)]
    #[test]
    fn qwen_node_context_artifact_enqueues_atomic_owner_only_pair() {
        let temp = TempDir::new().unwrap();
        let (spool, artifact_path, root, batch, object) = qwen_fixture(&temp);

        enqueue(OutputFormat::Json, artifact_path.to_str().unwrap(), spool.to_str().unwrap()).unwrap();
        let object_path = spool.join("incoming").join(format!("{root}.palwda"));
        let metadata_path = spool.join("incoming").join(format!("{root}.json"));
        assert_eq!(fs::read(&object_path).unwrap(), object);
        assert_eq!(fs::metadata(&object_path).unwrap().mode() & 0o777, 0o600);
        let metadata: Value = serde_json::from_slice(&fs::read(metadata_path).unwrap()).unwrap();
        assert_eq!(metadata["batch_id"], batch);
        assert_eq!(metadata["leaf_index"], 7);
        assert_eq!(metadata["object_root"], root);
    }

    #[cfg(unix)]
    #[test]
    fn producer_never_overwrites_existing_regular_final_path() {
        let temp = TempDir::new().unwrap();
        let (spool, artifact_path, root, _, _) = qwen_fixture(&temp);
        ensure_secure_dir(&spool).unwrap();
        let incoming = spool.join("incoming");
        ensure_secure_dir(&incoming).unwrap();
        let object_final = incoming.join(format!("{root}.palwda"));
        write_new(&object_final, b"preexisting").unwrap();

        assert!(enqueue(OutputFormat::Json, artifact_path.to_str().unwrap(), spool.to_str().unwrap()).is_err());
        assert_eq!(fs::read(object_final).unwrap(), b"preexisting");
        assert!(!incoming.join(format!("{root}.json")).exists());
        assert_no_temp_prefixes(&incoming);
    }

    #[cfg(unix)]
    #[test]
    fn producer_resumes_an_identical_object_only_crash_prefix() {
        let temp = TempDir::new().unwrap();
        let (spool, artifact_path, root, _, object) = qwen_fixture(&temp);
        ensure_secure_dir(&spool).unwrap();
        let incoming = spool.join("incoming");
        ensure_secure_dir(&incoming).unwrap();
        let object_final = incoming.join(format!("{root}.palwda"));
        write_new(&object_final, &object).unwrap();
        let stale_object_tmp = incoming.join(format!(".{root}.1-0-0.palwda.tmp"));
        let stale_metadata_tmp = incoming.join(format!(".{root}.1-0-0.json.tmp"));
        let other_root = "ab".repeat(64);
        let other_root_stale_tmp = incoming.join(format!(".{other_root}.2-0-9.palwda.tmp"));
        write_new(&stale_object_tmp, b"crash-left object temp").unwrap();
        write_new(&stale_metadata_tmp, b"crash-left metadata temp").unwrap();
        write_new(&other_root_stale_tmp, b"other root crash-left temp").unwrap();
        let before = fs::metadata(&object_final).unwrap();

        enqueue(OutputFormat::Json, artifact_path.to_str().unwrap(), spool.to_str().unwrap()).unwrap();
        let after = fs::metadata(&object_final).unwrap();
        assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()), "identical crash prefix must be reused, not replaced");
        assert_eq!(fs::read(&object_final).unwrap(), object);
        assert!(incoming.join(format!("{root}.json")).is_file());
        assert!(!stale_object_tmp.exists());
        assert!(!stale_metadata_tmp.exists());
        assert!(!other_root_stale_tmp.exists(), "enqueue must reclaim stale valid prefixes globally, not only for its root");
        assert_no_temp_prefixes(&incoming);
    }

    #[cfg(unix)]
    #[test]
    fn producer_never_overwrites_dangling_metadata_final_path() {
        let temp = TempDir::new().unwrap();
        let (spool, artifact_path, root, _, object) = qwen_fixture(&temp);
        ensure_secure_dir(&spool).unwrap();
        let incoming = spool.join("incoming");
        ensure_secure_dir(&incoming).unwrap();
        let metadata_final = incoming.join(format!("{root}.json"));
        let missing_target = incoming.join("missing-metadata-target");
        std::os::unix::fs::symlink(&missing_target, &metadata_final).unwrap();
        assert!(!metadata_final.exists(), "the test destination must be a dangling symlink");

        assert!(enqueue(OutputFormat::Json, artifact_path.to_str().unwrap(), spool.to_str().unwrap()).is_err());
        assert_eq!(fs::read_link(&metadata_final).unwrap(), missing_target);
        assert_eq!(fs::read(incoming.join(format!("{root}.palwda"))).unwrap(), object);
        assert_no_temp_prefixes(&incoming);
    }
}
