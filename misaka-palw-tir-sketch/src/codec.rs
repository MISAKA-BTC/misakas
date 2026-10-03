//! **The witness on the wire** (RFC-0007 Part II, §II.2 and §II.7): a canonical byte image of a [`TirWitnessV1`], cut into chunks a
//! producer serves and a seat names (`Unavailable { chunk_index }` names chunk `1 + i` of the claim's trace manifest under
//! `palw_witness_manifest_v1`), each chunk with a digest the seat checks what it was served against.
//!
//! The image is little-endian and strictly canonical: a version, the job's tokens, every position's served `MatMul` outputs and committed
//! rows (each tensor as its dtype tag, rank, dims and element bytes at its declared width), and the committed rows' root. [`decode`] is
//! total: a truncated, over-long, mis-tagged or absurdly sized image is an error naming what is wrong, never a panic and never an
//! allocation the image has not paid for.
//!
//! The witness carries no commitment on chain (a free field in the priced bytes would be a free draw, ADR-0072 Decision 8): its chunk
//! digests are the producer's served manifest, and what a seat trusts is the algebra, not the digests (they only name which chunk failed).

use blake2b_simd::Params;
use misaka_palw_tir::{DType, Tensor};

use crate::witness::{TirCommitRowV1, TirWitnessStepV1, TirWitnessV1, TirWitnessValueV1};

/// The image's version.
pub const TIR_WITNESS_CODEC_VERSION_V1: u16 = 1;
/// Key of a chunk digest.
pub const TIR_WITNESS_CHUNK_DOMAIN_V1: &[u8] = b"misaka-palw/tir/sketch/witness-chunk/v1";
/// The most elements one decoded tensor may declare (the IR's own ceiling is far below this; a hostile image cannot ask for more).
const MAX_TENSOR_ELEMENTS: usize = 1 << 26;

fn dtype_of(tag: u8) -> Result<DType, String> {
    DType::ALL.into_iter().find(|d| d.tag() == tag).ok_or_else(|| format!("dtype tag {tag} is not a dtype"))
}

fn put_tensor(out: &mut Vec<u8>, t: &Tensor) {
    out.push(t.dtype.tag());
    out.push(t.shape.len() as u8);
    for d in &t.shape {
        out.extend_from_slice(&(*d as u32).to_le_bytes());
    }
    out.extend_from_slice(&t.to_le_bytes());
}

/// **Encode** a witness canonically.
pub fn encode(w: &TirWitnessV1) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&TIR_WITNESS_CODEC_VERSION_V1.to_le_bytes());
    out.extend_from_slice(&w.prompt_len.to_le_bytes());
    out.extend_from_slice(&(w.tokens.len() as u32).to_le_bytes());
    for t in &w.tokens {
        out.extend_from_slice(&t.to_le_bytes());
    }
    out.extend_from_slice(&(w.generated.len() as u32).to_le_bytes());
    for t in &w.generated {
        out.extend_from_slice(&t.to_le_bytes());
    }
    out.extend_from_slice(&(w.steps.len() as u32).to_le_bytes());
    for s in &w.steps {
        out.extend_from_slice(&s.pos.to_le_bytes());
        out.extend_from_slice(&(s.values.len() as u32).to_le_bytes());
        for v in &s.values {
            out.extend_from_slice(&v.occurrence.to_le_bytes());
            out.extend_from_slice(&v.node.to_le_bytes());
            put_tensor(&mut out, &v.value);
        }
        out.extend_from_slice(&(s.commits.len() as u32).to_le_bytes());
        for c in &s.commits {
            out.extend_from_slice(&c.slot.to_le_bytes());
            put_tensor(&mut out, &c.value);
        }
    }
    out.extend_from_slice(&w.commit_root);
    out
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).filter(|end| *end <= self.bytes.len()).ok_or_else(|| format!("the image is truncated at byte {}", self.at))?;
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().expect("two bytes")))
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("four bytes")))
    }

    /// A count whose items each need at least `min_item` bytes: refused unless the rest of the image could hold them, so no
    /// allocation is made on a count the image cannot pay for.
    fn count(&mut self, min_item: usize) -> Result<usize, String> {
        let n = self.u32()? as usize;
        if n.checked_mul(min_item).is_none_or(|need| need > self.bytes.len() - self.at) {
            return Err(format!("a count of {n} items cannot fit in the {} bytes that remain", self.bytes.len() - self.at));
        }
        Ok(n)
    }

    fn tensor(&mut self) -> Result<Tensor, String> {
        let dtype = dtype_of(self.u8()?)?;
        let rank = self.u8()? as usize;
        if rank > misaka_palw_tir::types::MAX_RANK {
            return Err(format!("a tensor of rank {rank}"));
        }
        let mut shape = Vec::with_capacity(rank);
        let mut elements = 1usize;
        for _ in 0..rank {
            let d = self.u32()? as usize;
            elements = elements.checked_mul(d).filter(|e| *e <= MAX_TENSOR_ELEMENTS).ok_or("a tensor too large to be a witness value")?;
            shape.push(d);
        }
        let bytes = self.take(elements.checked_mul(dtype.width()).ok_or("a tensor too large")?)?;
        Tensor::from_le_bytes(dtype, &shape, bytes).map_err(|e| e.to_string())
    }
}

/// **Decode** a witness image: total, strict, and canonical (re-encoding the result gives the input back).
pub fn decode(bytes: &[u8]) -> Result<TirWitnessV1, String> {
    let mut r = Reader { bytes, at: 0 };
    let version = r.u16()?;
    if version != TIR_WITNESS_CODEC_VERSION_V1 {
        return Err(format!("witness image version {version}; this build reads {TIR_WITNESS_CODEC_VERSION_V1}"));
    }
    let prompt_len = r.u32()?;
    let n = r.count(4)?;
    let tokens = (0..n).map(|_| r.u32()).collect::<Result<Vec<_>, _>>()?;
    let n = r.count(4)?;
    let generated = (0..n).map(|_| r.u32()).collect::<Result<Vec<_>, _>>()?;
    let steps_n = r.count(8)?;
    let mut steps = Vec::with_capacity(steps_n);
    for _ in 0..steps_n {
        let pos = r.u32()?;
        let values_n = r.count(8)?;
        let mut values = Vec::with_capacity(values_n);
        for _ in 0..values_n {
            let (occurrence, node) = (r.u16()?, r.u16()?);
            values.push(TirWitnessValueV1 { occurrence, node, value: r.tensor()? });
        }
        let commits_n = r.count(6)?;
        let mut commits = Vec::with_capacity(commits_n);
        for _ in 0..commits_n {
            let slot = r.u32()?;
            commits.push(TirCommitRowV1 { slot, value: r.tensor()? });
        }
        steps.push(TirWitnessStepV1 { pos, values, commits });
    }
    let commit_root: [u8; 32] = r.take(32)?.try_into().expect("thirty-two bytes");
    if r.at != bytes.len() {
        return Err(format!("{} bytes follow the witness image", bytes.len() - r.at));
    }
    Ok(TirWitnessV1 { prompt_len, tokens, generated, steps, commit_root })
}

/// **Cut an image into chunks of at most `chunk_bytes`** (the last may be shorter; an empty image is no chunk).
pub fn chunks(image: &[u8], chunk_bytes: usize) -> Vec<&[u8]> {
    if chunk_bytes == 0 {
        return Vec::new();
    }
    image.chunks(chunk_bytes).collect()
}

/// **A chunk's digest**, bound to its index: a chunk cannot be passed off as another's.
pub fn chunk_digest(index: u32, chunk: &[u8]) -> [u8; 32] {
    let mut h = Params::new().hash_length(32).key(TIR_WITNESS_CHUNK_DOMAIN_V1).to_state();
    h.update(&index.to_le_bytes());
    h.update(&(chunk.len() as u64).to_le_bytes());
    h.update(chunk);
    let mut out = [0u8; 32];
    out.copy_from_slice(h.finalize().as_bytes());
    out
}

/// **Reassemble** chunks named by index (a seat that fetched them in any order) into the image — `None` unless every index
/// `0..digests.len()` is present, each chunk matches its digest, and none is extra.
pub fn assemble(mut served: Vec<(u32, Vec<u8>)>, digests: &[[u8; 32]]) -> Option<Vec<u8>> {
    served.sort_by_key(|(i, _)| *i);
    served.dedup_by_key(|(i, _)| *i);
    if served.len() != digests.len() {
        return None;
    }
    let mut out = Vec::new();
    for (expected, (index, chunk)) in served.iter().enumerate() {
        if *index as usize != expected || chunk_digest(*index, chunk) != digests[expected] {
            return None;
        }
        out.extend_from_slice(chunk);
    }
    Some(out)
}
