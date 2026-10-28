//! **Kimi K3 tokenizer / input canonicalization.**
//!
//! Same UTF-8 prompt on every node → same token id sequence. The protocol tokenizer is
//! **byte-fallback**: each byte becomes `BYTE_BASE + byte` with `BYTE_BASE = vocab − 256`.
//! There is no locale, no NFC, no UTF-16. Invalid UTF-8 is refused rather than repaired, so two
//! hosts cannot disagree about a replacement character.
//!
//! When a real 163,840-entry vocab root is bound, it becomes part of [`KimiK3TokenizerSpecV1`]
//! and the class id moves. Until then this is the only tokenizer a Kimi class may name.

use crate::Hash64;

/// Domain for [`kimi_k3_tokenizer_id_v1`].
pub const KIMI_K3_TOKENIZER_DOMAIN: &[u8] = b"misaka-palw/kimi-k3-tokenizer/v1";

/// Card vocabulary (ADR-0097 §1.3).
pub const KIMI_K3_VOCAB_SIZE: u32 = 163_840;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KimiK3TokenizerSpecV1 {
    pub vocab_size: u32,
    pub pad_id: u32,
    pub bos_id: u32,
    pub eos_id: u32,
}

impl KimiK3TokenizerSpecV1 {
    /// The card's sizes with reserved specials in the first three ids.
    pub const CARD: Self = Self { vocab_size: KIMI_K3_VOCAB_SIZE, pad_id: 0, bos_id: 1, eos_id: 2 };

    pub fn byte_base(self) -> u32 {
        self.vocab_size.saturating_sub(256)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KimiK3TokenizerError {
    Empty,
    InvalidUtf8,
    VocabTooSmall,
    SpecialIdOutOfRange,
}

/// Identity of the tokenizer a class commits. Carried on CanonicalWork's `tokenizer_id`.
pub fn kimi_k3_tokenizer_id_v1(spec: &KimiK3TokenizerSpecV1) -> Result<Hash64, KimiK3TokenizerError> {
    if spec.vocab_size < 256 {
        return Err(KimiK3TokenizerError::VocabTooSmall);
    }
    for id in [spec.pad_id, spec.bos_id, spec.eos_id] {
        if id >= spec.vocab_size {
            return Err(KimiK3TokenizerError::SpecialIdOutOfRange);
        }
    }
    let mut h = blake2b_simd::Params::new().hash_length(64).key(KIMI_K3_TOKENIZER_DOMAIN).to_state();
    h.update(&spec.vocab_size.to_le_bytes());
    h.update(&spec.pad_id.to_le_bytes());
    h.update(&spec.bos_id.to_le_bytes());
    h.update(&spec.eos_id.to_le_bytes());
    h.update(b"byte-fallback/v1");
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Ok(Hash64::from_bytes(out))
}

/// UTF-8 bytes → token ids. `wrap_bos_eos` prepends BOS and appends EOS.
pub fn kimi_k3_tokenize_v1(spec: &KimiK3TokenizerSpecV1, utf8: &[u8], wrap_bos_eos: bool) -> Result<Vec<u32>, KimiK3TokenizerError> {
    if spec.vocab_size < 256 {
        return Err(KimiK3TokenizerError::VocabTooSmall);
    }
    if !utf8.is_empty() && std::str::from_utf8(utf8).is_err() {
        return Err(KimiK3TokenizerError::InvalidUtf8);
    }
    let base = spec.byte_base();
    let mut ids = Vec::with_capacity(utf8.len() + 2);
    if wrap_bos_eos {
        ids.push(spec.bos_id);
    }
    for &b in utf8 {
        ids.push(base + u32::from(b));
    }
    if wrap_bos_eos {
        ids.push(spec.eos_id);
    }
    if ids.is_empty() {
        return Err(KimiK3TokenizerError::Empty);
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_prompt_is_the_same_ids_on_every_host() {
        let spec = KimiK3TokenizerSpecV1::CARD;
        let a = kimi_k3_tokenize_v1(&spec, b"hello", true).unwrap();
        let b = kimi_k3_tokenize_v1(&spec, b"hello", true).unwrap();
        assert_eq!(a, b);
        assert_eq!(a[0], spec.bos_id);
        assert_eq!(*a.last().unwrap(), spec.eos_id);
        assert_eq!(a[1], spec.byte_base() + u32::from(b'h'));
    }

    #[test]
    fn invalid_utf8_is_refused_not_repaired() {
        let spec = KimiK3TokenizerSpecV1::CARD;
        assert_eq!(kimi_k3_tokenize_v1(&spec, &[0xff], false), Err(KimiK3TokenizerError::InvalidUtf8));
    }

    #[test]
    fn tokenizer_id_is_stable_and_moves_with_the_spec() {
        let a = kimi_k3_tokenizer_id_v1(&KimiK3TokenizerSpecV1::CARD).unwrap();
        let b = kimi_k3_tokenizer_id_v1(&KimiK3TokenizerSpecV1::CARD).unwrap();
        assert_eq!(a, b);
        let mut other = KimiK3TokenizerSpecV1::CARD;
        other.eos_id = 3;
        assert_ne!(a, kimi_k3_tokenizer_id_v1(&other).unwrap());
    }
}
