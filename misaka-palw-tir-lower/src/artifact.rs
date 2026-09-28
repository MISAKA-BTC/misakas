//! **The artifact writer** (Gate 2a): a lowered program's integer params in one file, named per
//! `ParamDecl`, typed little-endian, with a digest.
//!
//! This is a CRATE-LOCAL format for the lowerer's own round trips and for review; Phase F binds
//! artifacts to consensus (the artifact root, chapter 03) and may lay them out differently. Nothing
//! here is a consensus object.
//!
//! ```text
//! file   := "PALWTIRA" · version u32 · header_len u64 · header (JSON) · data
//! header := { format, program_digest, digest, meta, tensors: [{ param, name, layer, dtype, shape,
//!             offset, bytes, blake2b256 }] }
//! data   := the tensors, in (param, layer) order, each little-endian two's complement
//! ```
//!
//! `program_digest` is BLAKE2b-512 of the program's canonical encoding (`TirProgramV1::encode`).
//! `digest` is BLAKE2b-512 (personal `palw-tir-art-v1`) over the program digest, then for each
//! tensor in order: `param u16 · layer u32 (u32::MAX for a global) · dtype tag u8 · rank u8 · dims
//! u32… · byte length u64 · bytes` — every integer little-endian. It covers names only through the
//! program digest (a param's name is its declaration's), so the digest identifies exactly the
//! program plus the numbers.

use crate::error::{LowerError, Result};
use crate::lower::{IntParams, IntTensor};
use misaka_palw_tir as tir;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::Path;
use tir::{DType, TirProgramV1};

pub const MAGIC: &[u8; 8] = b"PALWTIRA";
pub const VERSION: u32 = 1;
pub const FORMAT: &str = "palw-tir-artifact/v1 (crate-local; not a consensus format)";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TensorEntry {
    pub param: u16,
    pub name: String,
    pub layer: Option<u16>,
    pub dtype: String,
    pub shape: Vec<usize>,
    pub offset: u64,
    pub bytes: u64,
    pub blake2b256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Header {
    pub format: String,
    pub program_digest: String,
    pub digest: String,
    /// Free-form facts about how the numbers were made (scales, calibration set, policy).
    pub meta: serde_json::Value,
    pub tensors: Vec<TensorEntry>,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn program_digest(p: &TirProgramV1) -> String {
    hex(blake2b_simd::Params::new().hash_length(64).hash(&p.encode()).as_bytes())
}

/// The artifact digest (see the module doc) and the per-tensor entries, data offsets from 0.
fn digest_and_entries(p: &TirProgramV1, params: &IntParams) -> Result<(String, String, Vec<TensorEntry>)> {
    let pd = program_digest(p);
    let mut st = blake2b_simd::Params::new().hash_length(64).personal(b"palw-tir-art-v1").to_state();
    st.update(pd.as_bytes());
    let mut entries = Vec::with_capacity(params.tensors.len());
    let mut offset = 0u64;
    for ((pi, layer), t) in &params.tensors {
        let d =
            p.params.get(*pi as usize).ok_or_else(|| LowerError::bad(format!("tensor for param {pi}, which the program lacks")))?;
        if d.dtype != t.dtype || d.shape.iter().map(|x| *x as usize).collect::<Vec<_>>() != t.shape {
            return Err(LowerError::bad(format!("tensor `{}` does not match its declaration", d.name)));
        }
        let bytes = t.le_bytes();
        st.update(&pi.to_le_bytes());
        st.update(&layer.map(|l| l as u32).unwrap_or(u32::MAX).to_le_bytes());
        st.update(&[t.dtype.tag(), t.shape.len() as u8]);
        for dim in &t.shape {
            st.update(&(*dim as u32).to_le_bytes());
        }
        st.update(&(bytes.len() as u64).to_le_bytes());
        st.update(&bytes);
        entries.push(TensorEntry {
            param: *pi,
            name: d.name.clone(),
            layer: *layer,
            dtype: t.dtype.name().to_string(),
            shape: t.shape.clone(),
            offset,
            bytes: bytes.len() as u64,
            blake2b256: hex(blake2b_simd::Params::new().hash_length(32).hash(&bytes).as_bytes()),
        });
        offset += bytes.len() as u64;
    }
    Ok((pd, hex(st.finalize().as_bytes()), entries))
}

/// Write `params` for `program` to `path`; returns the artifact digest.
pub fn write(path: &Path, program: &TirProgramV1, params: &IntParams, meta: serde_json::Value) -> Result<String> {
    let (program_digest, digest, tensors) = digest_and_entries(program, params)?;
    let header = Header { format: FORMAT.into(), program_digest, digest: digest.clone(), meta, tensors };
    let hj = serde_json::to_vec(&header).map_err(|e| LowerError::Io(e.to_string()))?;
    let f = std::fs::File::create(path).map_err(|e| LowerError::Io(format!("{}: {e}", path.display())))?;
    let mut w = std::io::BufWriter::new(f);
    let io = |e: std::io::Error| LowerError::Io(e.to_string());
    w.write_all(MAGIC).map_err(io)?;
    w.write_all(&VERSION.to_le_bytes()).map_err(io)?;
    w.write_all(&(hj.len() as u64).to_le_bytes()).map_err(io)?;
    w.write_all(&hj).map_err(io)?;
    for t in params.tensors.values() {
        w.write_all(&t.le_bytes()).map_err(io)?;
    }
    w.flush().map_err(io)?;
    Ok(digest)
}

/// Read an artifact back, checking every tensor digest and the artifact digest against `program`.
pub fn read(path: &Path, program: &TirProgramV1) -> Result<(Header, IntParams)> {
    let mut f = std::fs::File::open(path).map_err(|e| LowerError::Io(format!("{}: {e}", path.display())))?;
    let io = |e: std::io::Error| LowerError::Io(e.to_string());
    let mut head = [0u8; 20];
    f.read_exact(&mut head).map_err(io)?;
    if &head[0..8] != MAGIC || u32::from_le_bytes(head[8..12].try_into().expect("4")) != VERSION {
        return Err(LowerError::bad("not a PALW-TIR artifact (v1)"));
    }
    let hl = u64::from_le_bytes(head[12..20].try_into().expect("8")) as usize;
    let mut hj = vec![0u8; hl];
    f.read_exact(&mut hj).map_err(io)?;
    let header: Header = serde_json::from_slice(&hj).map_err(|e| LowerError::bad(format!("artifact header: {e}")))?;
    let mut params = IntParams::default();
    for e in &header.tensors {
        let mut b = vec![0u8; e.bytes as usize];
        f.read_exact(&mut b).map_err(io)?;
        if hex(blake2b_simd::Params::new().hash_length(32).hash(&b).as_bytes()) != e.blake2b256 {
            return Err(LowerError::bad(format!("tensor `{}` does not match its digest", e.name)));
        }
        let dt = DType::from_name(&e.dtype).ok_or_else(|| LowerError::bad(format!("dtype {}", e.dtype)))?;
        params.tensors.insert((e.param, e.layer), IntTensor::from_le_bytes(dt, e.shape.clone(), &b)?);
    }
    let (pd, digest, _) = digest_and_entries(program, &params)?;
    if pd != header.program_digest {
        return Err(LowerError::bad("the artifact was made for a different program"));
    }
    if digest != header.digest {
        return Err(LowerError::bad("artifact digest mismatch"));
    }
    Ok((header, params))
}
