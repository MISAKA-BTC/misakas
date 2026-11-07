//! Owning39 immutable body archive for evidence after collection/retention
//! release (RFC-0002 §II.14, GC evidence). An archive is plain data: a source
//! body, a class body or a collected job's exact consent row, each stored under
//! its original identity. Nothing here is authority. The Core auditor
//! authenticates every returned byte against the original immutable header in
//! the SAME independently anchored state (source body hash, class derivation,
//! collection record's consent root/length/stored-body digest) and refuses a
//! mismatch; an archive can only make a genuine audit possible, never pass one.
//!
//! Layout under the archive root: `source/<hex id>`, `class/<hex id>`,
//! `consent/<hex job>`. Writers create files exclusively (no overwrite).
use kaspa_consensus_core::{
    palw_native_audit_proof_v39::{NativeAuditRowReaderV39, VerifiedNativeAuxRowsV39},
    palw_native_execution_v39::{NativeOriginalSourceArchiveV39, SOURCE_UPLOAD_MAX_BYTES_V39},
    palw_native_job_v39::{NativeConsentV39, NativeConsentWireV39},
};
use misaka_palw_kernel::hash::{Digest, object_id};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

const TABLE: u8 = 112;
fn hex(d: &Digest) -> String {
    crate::runtime_pack::commit::hex(d)
}
fn key(kind: u8, id: &Digest) -> Vec<u8> {
    borsh::to_vec(&(39u8, kind, id)).expect("bounded owning39 key")
}

pub struct NativeArchiveDirV39 {
    root: PathBuf,
}
impl NativeArchiveDirV39 {
    pub fn open(root: &Path) -> Result<Self, String> {
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        if !root.is_dir() {
            return Err("archive39 root must be an existing directory".into());
        }
        Ok(Self { root })
    }
    pub fn create(root: &Path) -> Result<Self, String> {
        for d in ["source", "class", "consent"] {
            std::fs::create_dir_all(root.join(d)).map_err(|e| e.to_string())?;
        }
        Self::open(root)
    }
    fn path(&self, family: &str, id: &Digest) -> Result<PathBuf, String> {
        let path = self.root.join(family).join(hex(id));
        if !path.starts_with(&self.root) {
            return Err("archive39 path escapes its root".into());
        }
        Ok(path)
    }
    fn load(&self, family: &str, id: &Digest, maximum: usize) -> Result<Option<Vec<u8>>, String> {
        let path = self.path(family, id)?;
        let Ok(mut f) = std::fs::File::open(&path) else { return Ok(None) };
        let m = f.metadata().map_err(|e| e.to_string())?;
        if !m.is_file() || m.len() > maximum as u64 {
            return Err("archive39 body exceeds the caller's original typed bound".into());
        }
        let mut raw = Vec::with_capacity(m.len() as usize);
        (&mut f).take(maximum as u64 + 1).read_to_end(&mut raw).map_err(|e| e.to_string())?;
        if raw.len() as u64 != m.len() {
            return Err("archive39 body changed while reading".into());
        }
        Ok(Some(raw))
    }
    fn store(&self, family: &str, id: &Digest, raw: &[u8]) -> Result<bool, String> {
        let path = self.path(family, id)?;
        if path.exists() {
            if std::fs::read(&path).map_err(|e| e.to_string())? != raw {
                return Err("archive39 refuses to replace an existing body with different bytes".into());
            }
            return Ok(false);
        }
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e| e.to_string())?;
        f.write_all(raw).and_then(|_| f.sync_all()).map_err(|e| e.to_string())?;
        Ok(true)
    }
}
impl NativeOriginalSourceArchiveV39 for NativeArchiveDirV39 {
    fn source(&self, id: Digest, maximum: usize) -> Result<Option<Vec<u8>>, String> {
        self.load("source", &id, maximum)
    }
    fn class(&self, id: Digest, maximum: usize) -> Result<Option<Vec<u8>>, String> {
        self.load("class", &id, maximum)
    }
    fn consent(&self, job: Digest, maximum: usize) -> Result<Option<Vec<u8>>, String> {
        self.load("consent", &job, maximum)
    }
}

#[derive(Debug, Default, serde::Serialize)]
pub struct ArchiveSaveReportV39 {
    pub sources: usize,
    pub classes: usize,
    pub consents: usize,
    pub already_present: usize,
}

/// Copy live original bodies out of ONE fully authenticated stream before a
/// later collection/release removes them. Each copied source body is checked
/// against its fixed header hash and each consent row against the consent
/// identity its funded header names; class bodies are copied verbatim (the
/// auditor re-derives the class from the source). Rows already released in
/// this state are skipped, not invented.
pub fn save_native_archive_from_stream_v39<R: NativeAuditRowReaderV39>(
    rows: &VerifiedNativeAuxRowsV39<R>,
    archive: &NativeArchiveDirV39,
) -> Result<ArchiveSaveReportV39, String> {
    let mut report = ArchiveSaveReportV39::default();
    let ids = rows
        .keys()
        .filter(|(table, k, _)| *table == TABLE && k.len() == 66 && k[0] == 39 && matches!(k[1], 1 | 3 | 27))
        .map(|(_, k, _)| (k[1], <Digest>::try_from(&k[2..66]).expect("fixed 64-byte id")))
        .collect::<Vec<_>>();
    for (kind, id) in ids {
        match kind {
            1 => {
                let (root, header): (Digest, (u16, Digest, u32, Digest)) = {
                    let raw = rows.row(TABLE, &key(0, &id), 1024)?.ok_or("archive39 live source without its fixed header")?;
                    // SourceHeaderV39 = { version, id, bytes, body, owner, registered }: only its
                    // leading fixed fields are read here; Core re-authenticates the whole row.
                    let mut r = raw.as_slice();
                    let root: Digest = borsh::BorshDeserialize::deserialize(&mut r).map_err(|_| "archive39 source header grammar")?;
                    let fixed: (u16, Digest, u32, Digest) =
                        borsh::BorshDeserialize::deserialize(&mut r).map_err(|_| "archive39 source header fields")?;
                    (root, fixed)
                };
                let _ = root;
                let raw = rows.row(TABLE, &key(1, &id), SOURCE_UPLOAD_MAX_BYTES_V39 + 4)?.ok_or("archive39 source body vanished")?;
                let body: Vec<u8> = borsh::from_slice(&raw).map_err(|_| "archive39 source body grammar")?;
                if header.0 != 39
                    || header.1 != id
                    || body.len() != header.2 as usize
                    || object_id(b"misaka-palw/native/source-body/v39", &body) != header.3
                {
                    return Err("archive39 live source body differs from its original header".into());
                }
                if archive.store("source", &id, &body)? {
                    report.sources += 1;
                } else {
                    report.already_present += 1;
                }
            }
            3 => {
                let raw = rows.row(TABLE, &key(3, &id), 266)?.ok_or("archive39 class body vanished")?;
                let body: Vec<u8> = borsh::from_slice(&raw).map_err(|_| "archive39 class body grammar")?;
                if body.len() != 262 {
                    return Err("archive39 class body exact extent".into());
                }
                if archive.store("class", &id, &body)? {
                    report.classes += 1;
                } else {
                    report.already_present += 1;
                }
            }
            _ => {
                let raw =
                    rows.row(TABLE, &key(27, &id), NativeConsentWireV39::MAX_BYTES + 16384)?.ok_or("archive39 consent vanished")?;
                let consent: NativeConsentV39 = borsh::from_slice(&raw).map_err(|_| "archive39 consent grammar")?;
                // The funded header's `original.consent` is checked by Core at
                // audit time; here the row's own identity must at least be its
                // consumed tombstone's (row 20 binds job ↔ consent once).
                let tomb = rows.row(TABLE, &key(20, &consent.root()), 256)?.ok_or("archive39 consent without consumed tombstone")?;
                let (job, _, c): (Digest, Digest, Digest) = borsh::from_slice(&tomb).map_err(|_| "archive39 tombstone grammar")?;
                if job != id || c != consent.root() {
                    return Err("archive39 consent row is not this job's consumed original consent".into());
                }
                if archive.store("consent", &id, &raw)? {
                    report.consents += 1;
                } else {
                    report.already_present += 1;
                }
            }
        }
    }
    Ok(report)
}
