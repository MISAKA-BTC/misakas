//! **`PALWTIRS`: a composite candidate's adapter section** (RFC-0004 §6.7, A10) — what a seat that
//! holds the parent fetches of a composite candidate: the candidate's program, layout and tokenizer,
//! and the tensors of its params `p..` alone. Its params `0..p` are the parent's, served from the
//! parent's own container, byte for byte (the composite rule guarantees the declarations and instances
//! are the parent's).
//!
//! ```text
//! file   := "PALWTIRS" · version u16 LE · header_len u32 LE · header (borsh) · zeros to 64 · tensors
//! header := PalwTirContainerHeaderV1 — PALWTIR1's header, with `tensors` restricted to params j ≥ p
//! ```
//!
//! * **The table** is exactly [`param_instances_v1`] of the program restricted to params `j ≥ p`, in
//!   inventory order, each entry keeping its global param index; offsets 64-byte aligned, in order,
//!   inside the file — so a mapped section serves its params in place, as a PALWTIR1 file does.
//! * **`p`** is the composite's (the chain's `PalwTirArtifactRefV1::Composite { p, .. }`, which a node
//!   reads off the candidate's registration); the reader is handed it and holds the table to it. The
//!   file's `meta` records it too (`composite.p`, with the roots, for humans and tools: it enters no
//!   identity).
//! * **Verification** is the node's, against the chain: the section's leaves root to `adapter_root`
//!   (`palw_tir_inventory_section_root_v1` over params `p..`), the parent's container to
//!   `parent_root`, and the candidate's class id recomputes over the composite artifact root.

use borsh::BorshDeserialize;
use misaka_palw_tir::TirProgramV1;
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::{
    DigestWriter, PALW_TIR_MAX_HEADER_BYTES_V1, PALW_TIR_TENSOR_ALIGN_V1, PREFIX, PalwTirContainerError, PalwTirContainerHeaderV1,
    PalwTirTensorEntryV1, Result, align, check_values, digest_state, param_instances_v1, tensor_bytes_v1,
};

pub const PALW_TIR_SECTION_MAGIC_V1: &[u8; 8] = b"PALWTIRS";
pub const PALW_TIR_SECTION_VERSION_V1: u16 = 1;

/// The instances a section of `program` from param `p` on holds, in inventory order.
pub fn section_instances_v1(program: &TirProgramV1, p: u32) -> Vec<(u16, Option<u16>)> {
    param_instances_v1(program)
        .into_iter()
        .enumerate()
        .filter(|(j, _)| *j as u32 >= p)
        .flat_map(|(j, inst)| inst.into_iter().map(move |l| (j as u16, l)))
        .collect()
}

fn check_p(program: &TirProgramV1, p: u32) -> Result<()> {
    if p as usize >= program.params.len() {
        return Err(PalwTirContainerError::Table(format!(
            "p = {p}: a section holds params p.. of a program of {} params, and must hold at least one",
            program.params.len()
        )));
    }
    Ok(())
}

/// **Write a section** of `program` from param `p` on: `tensor(param, layer)` is asked for each
/// instance of a param `j ≥ p` in inventory order and must return exactly its declared bytes. Returns
/// the file digest ([`crate::file_digest_v1`]) of what was written.
pub fn write_section_v1(
    path: &Path,
    program: &TirProgramV1,
    p: u32,
    layout: Vec<u8>,
    tokenizer_id: [u8; 64],
    meta: String,
    tensor: &mut dyn FnMut(u16, Option<u16>) -> std::result::Result<Vec<u8>, String>,
) -> Result<[u8; 64]> {
    misaka_palw_tir::validate::validate(program).map_err(|e| PalwTirContainerError::Program(e.to_string()))?;
    check_p(program, p)?;
    let entries: Vec<PalwTirTensorEntryV1> = section_instances_v1(program, p)
        .into_iter()
        .map(|(j, layer)| PalwTirTensorEntryV1 { param: j, layer, offset: 0, bytes: tensor_bytes_v1(program, j) })
        .collect();
    let mut header = PalwTirContainerHeaderV1 {
        version: PALW_TIR_SECTION_VERSION_V1,
        program: program.encode(),
        layout,
        tokenizer_id,
        tensors: entries,
        meta,
    };
    let header_len = borsh::to_vec(&header).map_err(|e| PalwTirContainerError::Header(e.to_string()))?.len() as u64;
    let mut at = align(PREFIX + header_len);
    for e in header.tensors.iter_mut() {
        e.offset = at;
        at = align(at + e.bytes);
    }
    let hb = borsh::to_vec(&header).map_err(|e| PalwTirContainerError::Header(e.to_string()))?;
    let f = std::fs::File::create(path)?;
    let mut w = DigestWriter { inner: std::io::BufWriter::with_capacity(1 << 20, f), state: digest_state(), written: 0 };
    w.write_all(PALW_TIR_SECTION_MAGIC_V1)?;
    w.write_all(&PALW_TIR_SECTION_VERSION_V1.to_le_bytes())?;
    w.write_all(&(hb.len() as u32).to_le_bytes())?;
    w.write_all(&hb)?;
    for e in &header.tensors {
        let pad = e.offset - w.written;
        w.write_all(&vec![0u8; pad as usize])?;
        let bytes = tensor(e.param, e.layer).map_err(PalwTirContainerError::Bytes)?;
        let name = &program.params[e.param as usize].name;
        if bytes.len() as u64 != e.bytes {
            return Err(PalwTirContainerError::Bytes(format!(
                "`{name}` (layer {:?}): {} bytes, declared {}",
                e.layer,
                bytes.len(),
                e.bytes
            )));
        }
        check_values(program.params[e.param as usize].dtype, &bytes)
            .map_err(|m| PalwTirContainerError::Bytes(format!("`{name}`: {m}")))?;
        w.write_all(&bytes)?;
    }
    w.inner.flush()?;
    let mut out = [0u8; 64];
    out.copy_from_slice(w.state.finalize().as_bytes());
    Ok(out)
}

/// An opened section: the checked header and program, `p`, and where each of its tensors is.
#[derive(Debug)]
pub struct PalwTirSectionV1 {
    pub header: PalwTirContainerHeaderV1,
    pub program: TirProgramV1,
    pub p: u32,
    pub path: PathBuf,
    pub file_len: u64,
    index: BTreeMap<(u16, Option<u16>), (u64, u64)>,
}

/// **A section's header, unchecked beyond its framing** — the magic, the version and the header's
/// bytes decoding — for a reader that learns `p` from it (the provenance's `composite.p`) before it
/// opens the section at that `p` ([`PalwTirSectionV1::open`] checks the rest).
pub fn peek_section_header_v1(path: &Path) -> Result<PalwTirContainerHeaderV1> {
    let mut f = std::fs::File::open(path)?;
    let file_len = f.metadata()?.len();
    let mut prefix = [0u8; PREFIX as usize];
    f.read_exact(&mut prefix).map_err(|_| PalwTirContainerError::Magic)?;
    if &prefix[0..8] != PALW_TIR_SECTION_MAGIC_V1 || u16::from_le_bytes([prefix[8], prefix[9]]) != PALW_TIR_SECTION_VERSION_V1 {
        return Err(PalwTirContainerError::Magic);
    }
    let hl = u32::from_le_bytes(prefix[10..14].try_into().expect("4 bytes"));
    if hl > PALW_TIR_MAX_HEADER_BYTES_V1 || PREFIX + hl as u64 > file_len {
        return Err(PalwTirContainerError::Header(format!("a header of {hl} bytes in a file of {file_len}")));
    }
    let mut hb = vec![0u8; hl as usize];
    f.read_exact(&mut hb)?;
    PalwTirContainerHeaderV1::try_from_slice(&hb).map_err(|e| PalwTirContainerError::Header(e.to_string()))
}

impl PalwTirSectionV1 {
    /// **Read and check the header** of a section of params `p..` (no tensor is read): the magic and
    /// version, the program decoding canonically and validating, `p` inside it, the table exactly
    /// [`section_instances_v1`] with the declared byte lengths, and the offsets aligned, in order and
    /// inside the file.
    pub fn open(path: &Path, p: u32) -> Result<Self> {
        let mut f = std::fs::File::open(path)?;
        let file_len = f.metadata()?.len();
        let mut prefix = [0u8; PREFIX as usize];
        f.read_exact(&mut prefix).map_err(|_| PalwTirContainerError::Magic)?;
        if &prefix[0..8] != PALW_TIR_SECTION_MAGIC_V1 || u16::from_le_bytes([prefix[8], prefix[9]]) != PALW_TIR_SECTION_VERSION_V1 {
            return Err(PalwTirContainerError::Magic);
        }
        let hl = u32::from_le_bytes(prefix[10..14].try_into().expect("4 bytes"));
        if hl > PALW_TIR_MAX_HEADER_BYTES_V1 || PREFIX + hl as u64 > file_len {
            return Err(PalwTirContainerError::Header(format!("a header of {hl} bytes in a file of {file_len}")));
        }
        let mut hb = vec![0u8; hl as usize];
        f.read_exact(&mut hb)?;
        let header = PalwTirContainerHeaderV1::try_from_slice(&hb).map_err(|e| PalwTirContainerError::Header(e.to_string()))?;
        if header.version != PALW_TIR_SECTION_VERSION_V1 {
            return Err(PalwTirContainerError::Header(format!("header version {}", header.version)));
        }
        let program = TirProgramV1::decode_canonical(&header.program).map_err(|e| PalwTirContainerError::Program(e.to_string()))?;
        misaka_palw_tir::validate::validate(&program).map_err(|e| PalwTirContainerError::Program(e.to_string()))?;
        check_p(&program, p)?;
        let want = section_instances_v1(&program, p);
        let got: Vec<(u16, Option<u16>)> = header.tensors.iter().map(|e| (e.param, e.layer)).collect();
        if got != want {
            return Err(PalwTirContainerError::Table(format!(
                "the tensors are not the program's param instances from param {p} on, in inventory order"
            )));
        }
        let mut index = BTreeMap::new();
        let mut end = PREFIX + hl as u64;
        for e in &header.tensors {
            let name = &program.params[e.param as usize].name;
            let decl = tensor_bytes_v1(&program, e.param);
            if e.bytes != decl {
                return Err(PalwTirContainerError::Table(format!(
                    "`{name}` (layer {:?}): {} bytes, declared {decl}",
                    e.layer, e.bytes
                )));
            }
            if !e.offset.is_multiple_of(PALW_TIR_TENSOR_ALIGN_V1) || e.offset < end {
                return Err(PalwTirContainerError::Table(format!(
                    "`{name}` (layer {:?}): offset {} misaligned or overlapping",
                    e.layer, e.offset
                )));
            }
            end = e.offset.checked_add(e.bytes).filter(|x| *x <= file_len).ok_or_else(|| {
                PalwTirContainerError::Table(format!("`{name}` (layer {:?}) runs past the end of the file", e.layer))
            })?;
            index.insert((e.param, e.layer), (e.offset, e.bytes));
        }
        Ok(Self { header, program, p, path: path.to_path_buf(), file_len, index })
    }

    /// Where an instance's bytes are: `(offset, bytes)` — only for a param `j ≥ p`.
    pub fn locate(&self, param: u16, layer: Option<u16>) -> Option<(u64, u64)> {
        self.index.get(&(param, layer)).copied()
    }

    /// One instance's bytes, read from the file.
    pub fn read_tensor_bytes(&self, param: u16, layer: Option<u16>) -> Result<Vec<u8>> {
        let (off, len) = self.locate(param, layer).ok_or(PalwTirContainerError::NoSuchTensor { param, layer })?;
        let mut f = std::fs::File::open(&self.path)?;
        f.seek(SeekFrom::Start(off))?;
        let mut v = vec![0u8; len as usize];
        f.read_exact(&mut v)?;
        Ok(v)
    }
}
