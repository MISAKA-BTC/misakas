//! Explicit provenance-only parsing. Legacy parsing stays feature-dependent; the
//! opt-in profile rounds JSON decimals with Rust's correctly-rounded f64 parser.
//! No profile changes program/layout/parameter arithmetic or grants acceptance.
use serde_json::{Value, value::RawValue};
use std::collections::BTreeMap;
pub const METADATA_PROFILE_V16: &str = "correctly-rounded-v16";
pub const METADATA_SCHEMA_V16: &str = "misaka.palw.artifact-metadata-parsing.v16";
pub const MAX_METADATA_BYTES_V16: usize = 1 << 20;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetadataParseProfileV16 {
    LegacyBuildV1,
    CorrectlyRoundedV16,
}
impl MetadataParseProfileV16 {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "legacy-build-v1" => Some(Self::LegacyBuildV1),
            METADATA_PROFILE_V16 => Some(Self::CorrectlyRoundedV16),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::LegacyBuildV1 => "legacy-build-v1",
            Self::CorrectlyRoundedV16 => METADATA_PROFILE_V16,
        }
    }
}
fn exact(raw: &RawValue, depth: usize) -> Result<Value, String> {
    if depth > 128 {
        return Err("metadata nesting cap".into());
    }
    let s = raw.get().trim();
    match s.as_bytes().first().copied() {
        Some(b'{') => {
            let fields: BTreeMap<String, Box<RawValue>> = serde_json::from_str(s).map_err(|e| e.to_string())?;
            let mut out = serde_json::Map::new();
            for (k, v) in fields {
                out.insert(k, exact(&v, depth + 1)?);
            }
            Ok(Value::Object(out))
        }
        Some(b'[') => {
            let values: Vec<Box<RawValue>> = serde_json::from_str(s).map_err(|e| e.to_string())?;
            Ok(Value::Array(values.iter().map(|v| exact(v, depth + 1)).collect::<Result<_, _>>()?))
        }
        Some(b'-' | b'0'..=b'9') => {
            if !s.contains(['.', 'e', 'E']) && s != "-0" {
                if let Ok(n) = s.parse::<i64>() {
                    return Ok(Value::from(n));
                }
                if let Ok(n) = s.parse::<u64>() {
                    return Ok(Value::from(n));
                }
                return Err("metadata integer exceeds exact i64/u64 range".into());
            }
            let n = s.parse::<f64>().map_err(|e| e.to_string())?;
            serde_json::Number::from_f64(n).map(Value::Number).ok_or_else(|| "metadata non-finite decimal".into())
        }
        _ => serde_json::from_str(s).map_err(|e| e.to_string()),
    }
}
pub fn parse_metadata_v16(text: &str, profile: MetadataParseProfileV16) -> Result<Value, String> {
    if profile == MetadataParseProfileV16::LegacyBuildV1 {
        return serde_json::from_str(text).map_err(|e| e.to_string());
    }
    if text.len() > MAX_METADATA_BYTES_V16 {
        return Err("metadata byte cap".into());
    }
    let raw: Box<RawValue> = serde_json::from_str(text).map_err(|e| e.to_string())?;
    exact(&raw, 0)
}
/// Marker decoding uses strings only. An unknown or malformed explicit marker is
/// refused; unmarked historical metadata retains the legacy parser unchanged.
pub fn parse_recorded_metadata_v16(text: &str) -> Result<Value, String> {
    let legacy: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    match legacy.get("metadata_parsing") {
        None => Ok(legacy),
        Some(v)
            if v.as_object().is_some_and(|o| o.len() == 2)
                && v["schema"] == METADATA_SCHEMA_V16
                && v["profile"] == METADATA_PROFILE_V16 =>
        {
            parse_metadata_v16(text, MetadataParseProfileV16::CorrectlyRoundedV16)
        }
        Some(_) => Err("unknown explicit metadata parsing profile".into()),
    }
}
pub fn record_metadata_profile_v16(meta: &mut Value, profile: MetadataParseProfileV16) -> Result<(), String> {
    if !meta.is_object() {
        return Err("metadata profile needs an object".into());
    }
    if profile == MetadataParseProfileV16::LegacyBuildV1 && meta.get("metadata_parsing").is_some() {
        return Err("explicit recorded metadata profile cannot be downgraded".into());
    }
    if profile == MetadataParseProfileV16::CorrectlyRoundedV16 {
        meta["metadata_parsing"] = serde_json::json!({"schema":METADATA_SCHEMA_V16,"profile":METADATA_PROFILE_V16})
    }
    Ok(())
}
