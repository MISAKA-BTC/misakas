//! **The lowerer's artifact output: a `PALWTIR1` container** (RFC-0002 Phase F, F3;
//! `misaka_palw_tir_artifact`). Gate 2a's crate-local `PALWTIRA` format grew into it: the same
//! typed little-endian tensors, now in inventory order, 64-byte aligned, beside the program they
//! belong to, so the node's evaluator loads what the lowerer writes and the consensus inventory
//! hashes it front to back.
//!
//! What identifies the artifact is not this file but the inventory root
//! (`kaspa_consensus_core::palw_tir_artifact_v1`) over the program's declarations; the file digest
//! returned here is what a `.palwmanifest` binds to the file.

use crate::error::{LowerError, Result};
use crate::lower::{IntParams, IntTensor};
use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir_artifact::{PalwTirContainerV1, write_container_v1};
use std::path::Path;

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// BLAKE2b-512 of the program's canonical encoding (plain, for logs; the class's `graph_ir_root` is
/// the keyed form in consensus).
pub fn program_digest(p: &TirProgramV1) -> String {
    hex(blake2b_simd::Params::new().hash_length(64).hash(&p.encode()).as_bytes())
}

/// The key of the tokenizer commitment — `misaka_palw_base0::artifact::PALW_BASE0_TOKENIZER_DOMAIN`,
/// spelled again because this crate does not depend on base0; `misaka-palw-sdk` holds the two
/// equal (`tir_manifest` tests).
pub const TOKENIZER_COMMITMENT_DOMAIN_V1: &[u8] = b"MISAKA/PALW/BASE0/TOKENIZER/V1\0\0";

/// **The tokenizer id a class binds**: the legacy tokenizer commitment of a `tokenizer.json`'s
/// bytes, `BLAKE2b-512(key, len_le64 ‖ bytes)` — the id an A16 artifact converted to `PALWTIR1`
/// carries, so an HF-lowered artifact of the same tokenizer carries the same one.
pub fn tokenizer_id_of(tokenizer_bytes: &[u8]) -> [u8; 64] {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(TOKENIZER_COMMITMENT_DOMAIN_V1).to_state();
    state.update(&(tokenizer_bytes.len() as u64).to_le_bytes());
    state.update(tokenizer_bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    out
}

/// **The files a tokenizer is read from**, in the order the converter binds the first one present when no `--tokenizer <file>` is
/// given. The class commits to the BYTES of one file ([`tokenizer_id_of`]); which tokenizer algorithm they describe is the file's
/// own business (a fast-tokenizers JSON, a SentencePiece model, a tiktoken ranks file, WordPiece's vocabulary), so the names are a
/// vocabulary of *data*, not of models: `tokenizer.json` first (every model that has it binds it, as before).
pub const TOKENIZER_FILES_V1: &[&str] = &[
    "tokenizer.json",
    "tokenizer.model",
    "spiece.model",
    "sentencepiece.bpe.model",
    "sentencepiece.model",
    "tekken.json",
    "vocab.json",
    "vocab.txt",
];

/// A file name a tokenizer is read from: one of [`TOKENIZER_FILES_V1`], or a tiktoken ranks file (`*.tiktoken`).
pub fn is_tokenizer_file(name: &str) -> bool {
    TOKENIZER_FILES_V1.contains(&name) || name.ends_with(".tiktoken")
}

/// The tokenizer file among `names` (file names of one directory) the converter binds: the first of [`TOKENIZER_FILES_V1`] present,
/// else the first `*.tiktoken` by name.
pub fn tokenizer_file_among<'a>(names: impl IntoIterator<Item = &'a str> + Clone) -> Option<String> {
    for want in TOKENIZER_FILES_V1 {
        if names.clone().into_iter().any(|n| n == *want) {
            return Some((*want).to_string());
        }
    }
    let mut tik: Vec<&str> = names.into_iter().filter(|n| n.ends_with(".tiktoken")).collect();
    tik.sort_unstable();
    tik.first().map(|n| (*n).to_string())
}

/// The tokenizer file of a model directory, if it has one ([`tokenizer_file_among`]).
pub fn tokenizer_path_in(dir: &Path) -> Option<std::path::PathBuf> {
    let names: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    tokenizer_file_among(names.iter().map(String::as_str)).map(|n| dir.join(n))
}

/// Write `params` for `program` as a `PALWTIR1` container; returns the file digest (hex).
/// `meta` is provenance (scales, calibration set) and enters no identity.
pub fn write(
    path: &Path,
    program: &TirProgramV1,
    params: &IntParams,
    tokenizer_id: [u8; 64],
    meta: serde_json::Value,
) -> Result<String> {
    let d = write_container_v1(path, program, Vec::new(), tokenizer_id, meta.to_string(), &mut |j, l| {
        params.tensors.get(&(j, l)).map(IntTensor::le_bytes).ok_or_else(|| format!("no tensor for param {j} layer {l:?}"))
    })
    .map_err(|e| LowerError::Io(e.to_string()))?;
    Ok(hex(&d))
}

/// Read a container back as integer params, checking it carries `program`.
pub fn read(path: &Path, program: &TirProgramV1) -> Result<(PalwTirContainerV1, IntParams)> {
    let c = PalwTirContainerV1::open(path).map_err(|e| LowerError::bad(e.to_string()))?;
    if &c.program != program {
        return Err(LowerError::bad("the container carries a different program"));
    }
    let mut params = IntParams::default();
    for e in &c.header.tensors {
        let d = &program.params[e.param as usize];
        let bytes = c.read_tensor_bytes(e.param, e.layer).map_err(|x| LowerError::bad(x.to_string()))?;
        let t = IntTensor::from_le_bytes(d.dtype, d.shape.iter().map(|x| *x as usize).collect(), &bytes)?;
        params.tensors.insert((e.param, e.layer), t);
    }
    Ok((c, params))
}

#[cfg(test)]
mod tokenizer_file_tests {
    use super::*;

    #[test]
    fn the_converter_binds_the_first_tokenizer_file_in_the_tables_order_and_tokenizer_json_wins() {
        let names = ["config.json", "vocab.txt", "spiece.model", "tokenizer.json", "merges.txt"];
        assert_eq!(tokenizer_file_among(names).as_deref(), Some("tokenizer.json"));
        assert_eq!(tokenizer_file_among(["config.json", "vocab.txt", "spiece.model"]).as_deref(), Some("spiece.model"));
        assert_eq!(tokenizer_file_among(["b.tiktoken", "a.tiktoken"]).as_deref(), Some("a.tiktoken"));
        assert_eq!(tokenizer_file_among(["vocab.json", "merges.txt", "a.tiktoken"]).as_deref(), Some("vocab.json"));
        assert_eq!(tokenizer_file_among(["config.json", "merges.txt", "special_tokens_map.json"]), None);
        assert!(is_tokenizer_file("x.tiktoken") && is_tokenizer_file("spiece.model") && !is_tokenizer_file("merges.txt"));
    }
}
