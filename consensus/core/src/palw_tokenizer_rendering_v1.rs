//! Explicit prospective token-id rendering rules. These do not tokenize prompt text.
//! JSON parsing and byte decoding are shared with the host to avoid a second interpretation.
//! Stop/EOG ids are independently signed configuration, not derived from tokenizer.json.

use crate::Hash64;
use std::collections::{BTreeMap, BTreeSet};

pub const PALW_TOKENIZER_RAW_DOMAIN_V1: &[u8] = b"MISAKA/PALW/BASE0/TOKENIZER/V1\0\0";

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwTokenizerRenderingRuleV1 {
    /// Unique ordinary and added-token ids; added tokens override the ordinary spelling.
    /// Holes below the largest id render empty; ids above it have no rendering.
    QwenByteLevelJsonV1 = 1,
}

pub fn palw_tokenizer_raw_commitment_v1(bytes: &[u8]) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_TOKENIZER_RAW_DOMAIN_V1).to_state();
    h.update(&(bytes.len() as u64).to_le_bytes());
    h.update(bytes);
    Hash64::from_bytes(h.finalize().as_bytes().try_into().expect("a 64-byte digest"))
}

pub fn palw_qwen_byte_alphabet_v1() -> [char; 256] {
    let mut table = ['\0'; 256];
    let mut assigned = [false; 256];
    for b in (b'!'..=b'~').chain(0xA1..=0xAC).chain(0xAE..=0xFF) {
        table[b as usize] = char::from_u32(u32::from(b)).expect("a byte is a codepoint");
        assigned[b as usize] = true;
    }
    let mut next = 256;
    for (b, slot) in table.iter_mut().enumerate() {
        if !assigned[b] {
            *slot = char::from_u32(next).expect("a byte alphabet codepoint");
            next += 1;
        }
    }
    table
}

pub fn palw_qwen_token_rendering_v1(token: &str, added: bool) -> Option<Vec<u8>> {
    if added {
        return Some(token.as_bytes().to_vec());
    }
    let alphabet = palw_qwen_byte_alphabet_v1();
    token.chars().map(|c| alphabet.iter().position(|symbol| *symbol == c).map(|b| b as u8)).collect()
}

/// Sparse id map: a tiny JSON file naming a huge id never allocates an id-sized array here.
#[derive(Clone, Debug)]
pub struct PalwQwenRenderingMapV1 {
    tokens: BTreeMap<u32, (String, bool)>,
    highest: u32,
}

impl PalwQwenRenderingMapV1 {
    pub fn parse(bytes: &[u8]) -> Result<Self, &'static str> {
        let root = palw_unique_json_v1(bytes)?;
        let model = root.get("model").ok_or("no model")?;
        let vocab = model.get("vocab").and_then(|v| v.as_object()).ok_or("no model.vocab")?;
        let mut tokens = BTreeMap::new();
        let mut highest = 0;
        for (token, id) in vocab {
            let id = id.as_u64().and_then(|id| u32::try_from(id).ok()).ok_or("a vocabulary id is not u32")?;
            highest = highest.max(id);
            if tokens.insert(id, (token.clone(), false)).is_some() {
                return Err("duplicate vocabulary id");
            }
        }
        // Validate the same accepted JSON schema as the runtime even though merges do not
        // affect id rendering. The rule has no regex/normalization implementation dependency.
        for entry in model.get("merges").and_then(|v| v.as_array()).ok_or("no model.merges")? {
            match entry {
                serde_json::Value::String(s) if s.split_once(' ').is_some() => {}
                serde_json::Value::Array(pair) if pair.len() == 2 && pair.iter().all(|v| v.is_string()) => {}
                _ => return Err("a merge is neither two strings nor a spaced string"),
            }
        }
        let mut added_ids = BTreeSet::new();
        if let Some(list) = root.get("added_tokens") {
            for entry in list.as_array().ok_or("added_tokens is not an array")? {
                let id = entry
                    .get("id")
                    .and_then(|v| v.as_u64())
                    .and_then(|id| u32::try_from(id).ok())
                    .ok_or("an added token id is not u32")?;
                let token = entry.get("content").and_then(|v| v.as_str()).ok_or("added token content is not a string")?;
                if !added_ids.insert(id) {
                    return Err("duplicate added token id");
                }
                highest = highest.max(id);
                tokens.insert(id, (token.to_owned(), true));
            }
        }
        Ok(Self { tokens, highest })
    }
    pub fn rendering(&self, id: u32) -> Option<Vec<u8>> {
        if id > self.highest {
            return None;
        }
        match self.tokens.get(&id) {
            Some((token, added)) => palw_qwen_token_rendering_v1(token, *added),
            None => Some(Vec::new()),
        }
    }
}

/// Reject repeated object keys at any depth instead of silently accepting last-value wins.
/// The incremental court must implement this exact rule before publishing any derived result.
pub fn palw_unique_json_v1(bytes: &[u8]) -> Result<serde_json::Value, &'static str> {
    use serde::Deserialize;
    let mut d = serde_json::Deserializer::from_slice(bytes);
    let value = UniqueJson::deserialize(&mut d).map_err(|_| "malformed JSON or duplicate object key")?.0;
    d.end().map_err(|_| "trailing JSON bytes")?;
    Ok(value)
}

struct UniqueJson(serde_json::Value);
impl<'de> serde::Deserialize<'de> for UniqueJson {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = UniqueJson;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("JSON with unique object keys")
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
                serde_json::Number::from_f64(v).map(|v| UniqueJson(v.into())).ok_or_else(|| E::custom("nonfinite number"))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueJson(serde_json::Value::Null))
            }
            fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                self.visit_unit()
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(v) = a.next_element::<UniqueJson>()? {
                    values.push(v.0);
                }
                Ok(UniqueJson(serde_json::Value::Array(values)))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = a.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom("duplicate object key"));
                    }
                    values.insert(key, a.next_value::<UniqueJson>()?.0);
                }
                Ok(UniqueJson(serde_json::Value::Object(values)))
            }
        }
        d.deserialize_any(Visitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alphabet_added_holes_and_duplicates_are_unambiguous() {
        let alphabet = palw_qwen_byte_alphabet_v1();
        let token: String = alphabet.iter().collect();
        assert_eq!(palw_qwen_token_rendering_v1(&token, false), Some((0..=255).collect()));
        let raw = r#"{"model":{"vocab":{"a":0,"b":2},"merges":[]},"added_tokens":[{"id":2,"content":"漢"}]}"#.as_bytes();
        let map = PalwQwenRenderingMapV1::parse(raw).unwrap();
        assert_eq!(map.rendering(0), Some(b"a".to_vec()));
        assert_eq!(map.rendering(1), Some(vec![]));
        assert_eq!(map.rendering(2), Some("漢".as_bytes().to_vec()));
        assert_eq!(map.rendering(3), None);
        for raw in [
            br#"{"model":{"vocab":{"a":0,"b":0},"merges":[]}}"#.as_slice(),
            br#"{"model":{"vocab":{"a":4294967296},"merges":[]}}"#,
            br#"{"model":{"vocab":{"a":0,"a":1},"merges":[]}}"#,
            br#"{"model":{"vocab":{},"merges":[]},"added_tokens":[{"id":0,"content":"a"},{"id":0,"content":"b"}]}"#,
        ] {
            assert!(PalwQwenRenderingMapV1::parse(raw).is_err());
        }
        let sparse = PalwQwenRenderingMapV1::parse(br#"{"model":{"vocab":{"a":4294967295},"merges":[]}}"#).unwrap();
        assert_eq!(sparse.rendering(u32::MAX), Some(b"a".to_vec()));
    }
}
