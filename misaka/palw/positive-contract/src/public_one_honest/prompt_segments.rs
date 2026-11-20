//! Draft prompt material preserving the worker's text/control boundaries.
//! A distinct codec keeps existing literal-text originals unchanged. This
//! encodes inputs only; native profile selection and tokenization are separate.
use super::{CheckedManifestV1, ManifestEntryV1, ManifestScopeRules, MaterialScope};
use crate::{
    ContractError,
    identity::{Digest, hash},
};
use std::fmt::Write;

const MAGIC: &str = "PPS1\n";

/// Selected by the native carrier version, never inferred from prompt bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptMaterialEncodingV1 {
    LiteralUtf8,
    SegmentsV1,
}
impl PromptMaterialEncodingV1 {
    pub fn codec_v1(self) -> Digest {
        match self {
            Self::LiteralUtf8 => super::prompt_text_material_codec_v1(),
            Self::SegmentsV1 => prompt_segments_material_codec_v1(),
        }
    }
    pub fn from_codec_v1(codec: Digest) -> Result<Self, ContractError> {
        if codec == super::prompt_text_material_codec_v1() {
            Ok(Self::LiteralUtf8)
        } else if codec == prompt_segments_material_codec_v1() {
            Ok(Self::SegmentsV1)
        } else {
            Err(ContractError::UnsupportedProfile)
        }
    }
    /// Keep every legacy scope unchanged; segmented inputs have a distinct scope.
    pub fn ruleset_v1(self, legacy: Digest) -> Digest {
        match self {
            Self::LiteralUtf8 => legacy,
            Self::SegmentsV1 => hash(b"misaka/palw/segmented-prompt-ruleset/v1", &legacy),
        }
    }
    pub fn material_v1(self, ruleset: Digest, body: &str, rules: ManifestScopeRules) -> Result<ManifestEntryV1, ContractError> {
        if rules.codec != self.codec_v1() {
            return Err(ContractError::UnsupportedProfile);
        }
        match self {
            Self::LiteralUtf8 => super::prompt_text_material_v1(ruleset, body, rules),
            Self::SegmentsV1 => prompt_segments_material_v1(ruleset, body, rules),
        }
    }
    /// The caller authenticates raw tokenizer bytes first. Callbacks use the
    /// worker's literal Text semantics and its explicit added-token table.
    pub fn encode_ids_v1<E>(
        self,
        body: &str,
        maximum_ids: u32,
        mut text: impl FnMut(&str) -> Result<Vec<u32>, E>,
        mut control: impl FnMut(u32) -> bool,
    ) -> Result<Vec<u32>, ContractError> {
        let mut out = Vec::new();
        let mut append = |ids: &[u32]| -> Result<(), ContractError> {
            let total = out.len().checked_add(ids.len()).ok_or(ContractError::Budget)?;
            if total > maximum_ids as usize {
                return Err(ContractError::Budget);
            }
            out.try_reserve(ids.len()).map_err(|_| ContractError::Budget)?;
            out.extend_from_slice(ids);
            Ok(())
        };
        match self {
            Self::LiteralUtf8 => append(&text(body).map_err(|_| ContractError::Identity)?)?,
            Self::SegmentsV1 => {
                let segments = PromptSegmentsV1::parse_v1(body, body.len() as u64, u32::MAX)?;
                for segment in segments.iter_v1() {
                    match segment {
                        PromptSegmentRefV1::Text(value) => append(&text(value).map_err(|_| ContractError::Identity)?)?,
                        PromptSegmentRefV1::Special(id) if control(id) => append(&[id])?,
                        PromptSegmentRefV1::Special(_) => return Err(ContractError::Identity),
                    }
                }
            }
        }
        if out.is_empty() {
            return Err(ContractError::NonCanonical);
        }
        Ok(out)
    }

    pub fn check_body_v1(self, body: &[u8], max_bytes: u64) -> Result<(), ContractError> {
        if body.len() as u64 > max_bytes {
            return Err(ContractError::Budget);
        }
        let text = std::str::from_utf8(body).map_err(|_| ContractError::NonCanonical)?;
        if text.is_empty() {
            return Err(ContractError::NonCanonical);
        }
        if self == Self::SegmentsV1 {
            PromptSegmentsV1::parse_v1(text, max_bytes, u32::MAX)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptSegmentRefV1<'a> {
    Text(&'a str),
    Special(u32),
}

/// A checked, allocation-free view. Text lengths count UTF-8 bytes, not
/// characters. Adjacent Text segments remain separate tokenizer calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PromptSegmentsV1<'a> {
    body: &'a str,
    count: u32,
}

fn number(input: &mut &str, delimiter: char) -> Result<u32, ContractError> {
    let (digits, rest) = input.split_once(delimiter).ok_or(ContractError::NonCanonical)?;
    if digits.is_empty()
        || digits.len() > 10
        || (digits.len() > 1 && digits.starts_with('0'))
        || !digits.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(ContractError::NonCanonical);
    }
    let value = digits.parse().map_err(|_| ContractError::Range)?;
    *input = rest;
    Ok(value)
}

fn next<'a>(input: &mut &'a str) -> Result<Option<PromptSegmentRefV1<'a>>, ContractError> {
    if input.is_empty() {
        return Ok(None);
    }
    if let Some(rest) = input.strip_prefix("t:") {
        *input = rest;
        let length = number(input, ':')? as usize;
        if length == 0 {
            return Err(ContractError::NonCanonical);
        }
        let text = input.get(..length).ok_or(ContractError::Range)?;
        *input = input.get(length..).ok_or(ContractError::Range)?;
        Ok(Some(PromptSegmentRefV1::Text(text)))
    } else if let Some(rest) = input.strip_prefix("s:") {
        *input = rest;
        Ok(Some(PromptSegmentRefV1::Special(number(input, '\n')?)))
    } else {
        Err(ContractError::NonCanonical)
    }
}

impl<'a> PromptSegmentsV1<'a> {
    pub fn parse_v1(body: &'a str, max_bytes: u64, max_segments: u32) -> Result<Self, ContractError> {
        if body.len() as u64 > max_bytes {
            return Err(ContractError::Budget);
        }
        let mut input = body.strip_prefix(MAGIC).ok_or(ContractError::Version)?;
        let mut count = 0u32;
        while next(&mut input)?.is_some() {
            count = count.checked_add(1).filter(|n| *n <= max_segments).ok_or(ContractError::Budget)?;
        }
        if count == 0 {
            return Err(ContractError::NonCanonical);
        }
        Ok(Self { body, count })
    }
    pub fn body_v1(&self) -> &'a str {
        self.body
    }
    pub fn count_v1(&self) -> u32 {
        self.count
    }
    pub fn iter_v1(&self) -> impl Iterator<Item = PromptSegmentRefV1<'a>> + 'a {
        let mut input = &self.body[MAGIC.len()..];
        std::iter::from_fn(move || next(&mut input).expect("immutable validated prompt"))
    }
}

pub fn encode_prompt_segments_v1(segments: &[PromptSegmentRefV1<'_>], max_bytes: u64) -> Result<String, ContractError> {
    // Size first: no output allocation precedes the caller's byte bound.
    let mut size = MAGIC.len() as u64;
    for segment in segments {
        let added = match segment {
            PromptSegmentRefV1::Text(text) => {
                let n = u32::try_from(text.len()).map_err(|_| ContractError::Budget)?;
                if n == 0 {
                    return Err(ContractError::NonCanonical);
                }
                3 + u64::from(n.ilog10() + 1) + u64::from(n)
            }
            PromptSegmentRefV1::Special(id) => 3 + u64::from(id.checked_ilog10().unwrap_or(0) + 1),
        };
        size = size.checked_add(added).filter(|n| *n <= max_bytes).ok_or(ContractError::Budget)?;
    }
    if segments.is_empty() || size > max_bytes {
        return Err(ContractError::NonCanonical);
    }
    let count = u32::try_from(segments.len()).map_err(|_| ContractError::Budget)?;
    let mut out = String::new();
    out.try_reserve_exact(usize::try_from(size).map_err(|_| ContractError::Budget)?).map_err(|_| ContractError::Budget)?;
    out.push_str(MAGIC);
    for segment in segments {
        match segment {
            PromptSegmentRefV1::Text(text) => write!(&mut out, "t:{}:{text}", text.len()),
            PromptSegmentRefV1::Special(id) => writeln!(&mut out, "s:{id}"),
        }
        .map_err(|_| ContractError::Budget)?;
    }
    PromptSegmentsV1::parse_v1(&out, max_bytes, count)?;
    Ok(out)
}

pub fn prompt_segments_material_codec_v1() -> Digest {
    hash(b"misaka/palw/material-codec/v1", &b"prompt-segments/utf8-text-and-explicit-controls/v1/whole")
}

pub fn prompt_segments_material_v1(ruleset: Digest, body: &str, rules: ManifestScopeRules) -> Result<ManifestEntryV1, ContractError> {
    if rules.codec != prompt_segments_material_codec_v1() {
        return Err(ContractError::UnsupportedProfile);
    }
    PromptSegmentsV1::parse_v1(body, rules.max_bytes.min(u64::from(rules.chunk_bytes)), u32::MAX)?;
    let subject = hash(b"misaka/palw/public-prompt-segments/v1", &(ruleset, body.as_bytes()));
    Ok(ManifestEntryV1 {
        subject,
        content_root: subject,
        codec: rules.codec,
        total_bytes: body.len() as u64,
        chunk_bytes: rules.chunk_bytes,
    })
}

impl CheckedManifestV1 {
    pub fn open_prompt_material_v2<'a>(&self, bytes: &'a [u8]) -> Result<(PromptMaterialEncodingV1, &'a str), ContractError> {
        let encoding = PromptMaterialEncodingV1::from_codec_v1(self.entry(MaterialScope::Prompt).codec)?;
        let text = match encoding {
            PromptMaterialEncodingV1::LiteralUtf8 => self.open_prompt_text(bytes)?,
            PromptMaterialEncodingV1::SegmentsV1 => self.open_prompt_segments_v1(bytes, u32::MAX)?.body_v1(),
        };
        Ok((encoding, text))
    }

    pub fn open_prompt_segments_v1<'a>(&self, bytes: &'a [u8], max_segments: u32) -> Result<PromptSegmentsV1<'a>, ContractError> {
        let entry = self.entry(MaterialScope::Prompt);
        if bytes.len() as u64 != entry.total_bytes {
            return Err(ContractError::Range);
        }
        let body = std::str::from_utf8(bytes).map_err(|_| ContractError::NonCanonical)?;
        let rules = ManifestScopeRules { codec: entry.codec, max_bytes: entry.total_bytes, chunk_bytes: entry.chunk_bytes };
        if prompt_segments_material_v1(self.ruleset, body, rules)? != *entry {
            return Err(ContractError::Identity);
        }
        PromptSegmentsV1::parse_v1(body, entry.total_bytes, max_segments)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prompt_segments_encoding_retains_boundaries_and_refuses_control_substitution() {
        let encoding = PromptMaterialEncodingV1::SegmentsV1;
        let text = |value: &str| Ok::<Vec<u32>, ()>(if value == "ab" { vec![99] } else { value.bytes().map(u32::from).collect() });
        let body = encode_prompt_segments_v1(
            &[PromptSegmentRefV1::Special(128), PromptSegmentRefV1::Text("a"), PromptSegmentRefV1::Text("b")],
            100,
        )
        .unwrap();
        assert_eq!(encoding.encode_ids_v1(&body, 3, text, |id| id == 128).unwrap(), vec![128, 97, 98]);
        assert!(encoding.encode_ids_v1(&body, 2, text, |id| id == 128).is_err());
        assert!(encoding.encode_ids_v1(&body, 3, text, |_| false).is_err());
        assert_eq!(PromptMaterialEncodingV1::LiteralUtf8.encode_ids_v1("ab", 1, text, |_| false).unwrap(), vec![99]);
        assert_ne!(encoding.ruleset_v1(Digest([1; 64])), PromptMaterialEncodingV1::LiteralUtf8.ruleset_v1(Digest([1; 64])));
    }

    #[test]
    fn prompt_segments_preserve_literal_controls_unicode_and_tokenization_boundaries() {
        let pieces = [
            PromptSegmentRefV1::Special(128),
            PromptSegmentRefV1::Text("<|im_start|>日本語\ns:7\n"),
            PromptSegmentRefV1::Text("a"),
            PromptSegmentRefV1::Text("b"),
            PromptSegmentRefV1::Special(u32::MAX),
        ];
        let body = encode_prompt_segments_v1(&pieces, 4096).unwrap();
        let view = PromptSegmentsV1::parse_v1(&body, body.len() as u64, 5).unwrap();
        assert_eq!(view.iter_v1().collect::<Vec<_>>(), pieces);
        assert_eq!(view.count_v1(), 5);
        assert!(PromptSegmentsV1::parse_v1(&body, body.len() as u64 - 1, 5).is_err());
        assert!(PromptSegmentsV1::parse_v1(&body, body.len() as u64, 4).is_err());
        assert!(encode_prompt_segments_v1(&pieces, body.len() as u64 - 1).is_err());
        let rules = ManifestScopeRules { codec: prompt_segments_material_codec_v1(), max_bytes: 4096, chunk_bytes: 4096 };
        let tag = Digest([9; 64]);
        let special = encode_prompt_segments_v1(&[PromptSegmentRefV1::Special(128)], 4096).unwrap();
        let literal = encode_prompt_segments_v1(&[PromptSegmentRefV1::Text("<|im_start|>")], 4096).unwrap();
        assert_ne!(
            prompt_segments_material_v1(tag, &special, rules).unwrap().subject,
            prompt_segments_material_v1(tag, &literal, rules).unwrap().subject
        );
        assert!(super::super::prompt_text_material_v1(tag, &body, rules).is_err());
        assert!(
            prompt_segments_material_v1(
                tag,
                &body,
                ManifestScopeRules { codec: super::super::prompt_text_material_codec_v1(), ..rules }
            )
            .is_err()
        );
    }
    #[test]
    fn prompt_segments_reject_noncanonical_or_unbounded_inputs() {
        for body in [
            "",
            "PPS2\ns:1\n",
            "PPS1\n",
            "PPS1\nt:0:",
            "PPS1\ns:01\n",
            "PPS1\ns:+1\n",
            "PPS1\ns:4294967296\n",
            "PPS1\nt:1:日",
            "PPS1\nt:99:a",
            "PPS1\ns:1\ntrailing",
            "PPS1\nt:01:a",
        ] {
            assert!(PromptSegmentsV1::parse_v1(body, 4096, 10).is_err(), "{body:?}");
        }
        assert!(encode_prompt_segments_v1(&[], 4096).is_err());
        assert!(encode_prompt_segments_v1(&[PromptSegmentRefV1::Text("")], 4096).is_err());
    }
}
