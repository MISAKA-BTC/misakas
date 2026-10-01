//! **The quant-format descriptor** — the JSON schema `misaka.palw.quant-format.v1`, and its
//! canonical text and digest.
//!
//! A descriptor is a file, not code. Whoever adds a model that uses a format this build does not
//! know writes one; a runtime pack pins it by [`digest`](QuantFormatDesc::digest), so a conversion
//! is reproducible by anyone holding the file. See [`super`] for how it decodes.
//!
//! ```json
//! { "schema": "misaka.palw.quant-format.v1",
//!   "name": "Q4_0",
//!   "ids": [ { "scheme": "ggml", "id": 2 } ],
//!   "layout": { "kind": "blocks", "elems": 32, "bytes": 18,
//!               "fields": [ { "name": "d", "at": 0, "type": "f16" },
//!                           { "name": "qs", "at": 2, "type": "u8", "count": 16 } ] },
//!   "decode": { "target": "integers",
//!               "group": { "size": 32 },
//!               "q": "(qs[e % 16] >> (4 * (e / 16))) & 15",
//!               "scale": "d", "zero": "8",
//!               "code": { "min": 0, "max": 15 } },
//!   "tests": [ { "block_hex": "…", "values_f32_hex": "…" } ] }
//! ```
//!
//! **What a `blocks` format is**: a tensor stored as rows of fixed-size blocks, `elems` weights in
//! `bytes` bytes, the way ggml stores quantised weights. The fields are typed reads at byte offsets
//! of one block; the decode expressions (see [`super::expr`]) say, per element `e` of the block, its
//! integer code `q`, and per group `j` of `group.size` consecutive elements, the exact `scale`, the
//! integer `zero` point and the optional float offset `min`, so that
//! `W = scale · (q − zero) − min`. `target: "floats"` says instead `value` per element, for formats
//! whose elements are themselves floats.
//!
//! **What a `tensors` format is**: a weight stored as several named checkpoint tensors (GPTQ's
//! `qweight`, `qzeros`, `scales`, `g_idx`; an FP8 weight and its block scales). Each role is a tensor
//! `<module><suffix>`; expressions index them by element (`qweight[i / 8, o]`) and may read the
//! format's parameters (`bits`, `group_size`: from the model's `quantization_config`).

use super::expr::DslError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const QUANT_FORMAT_SCHEMA_V1: &str = "misaka.palw.quant-format.v1";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuantFormatDesc {
    pub schema: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub doc: String,
    /// How a checkpoint announces this format.
    #[serde(default)]
    pub ids: Vec<FormatId>,
    pub layout: LayoutDesc,
    /// How a `quantization_config` that announces this format is read (`ids` of scheme `config`): the
    /// keys it may carry, the modules it leaves in float, and the conditions under which the stored
    /// tensors mean what this descriptor says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<ConfigDesc>,
    /// Parameters of the format (`bits`, `group_size`), read from the model's configuration.
    #[serde(default)]
    pub params: BTreeMap<String, ParamDesc>,
    #[serde(default)]
    pub tables: BTreeMap<String, TableDesc>,
    pub decode: DecodeDesc,
    /// Block bytes and the values they must decode to: a descriptor that fails its own vectors is
    /// refused when it is loaded.
    #[serde(default)]
    pub tests: Vec<TestVector>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormatId {
    /// `ggml` (a GGUF tensor type id) or `config` (a `quantization_config.quant_method`).
    pub scheme: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum LayoutDesc {
    #[serde(rename = "blocks")]
    Blocks {
        elems: usize,
        bytes: usize,
        fields: Vec<FieldDesc>,
    },
    #[serde(rename = "tensors")]
    Tensors {
        roles: Vec<RoleDesc>,
        /// The weight's `[out, in]` shape, as expressions over the roles' shapes (`dim_scales[1]`)
        /// and the parameters.
        dims: DimsDesc,
        /// Conditions over the shapes and parameters that must all hold (non-zero): the format's own
        /// consistency rules, so a checkpoint whose tensors disagree is refused by name.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        checks: Vec<CheckDesc>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDesc {
    pub name: String,
    /// Byte offset in the block.
    pub at: usize,
    /// `u8 i8 u16 i16 u32 i32 u64 i64` (integers), `f16 bf16 f32 f64` (read as floats).
    #[serde(rename = "type")]
    pub ty: String,
    /// An array of this many (absent: a scalar).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleDesc {
    pub name: String,
    /// The tensor is `<module><suffix>`.
    pub suffix: String,
    /// Stored dtypes accepted (`I32`, `U8`, `F16`, `BF16`, `F32`, `F8_E4M3`, …).
    pub dtypes: Vec<String>,
    #[serde(default = "yes")]
    pub required: bool,
    /// Number of axes.
    pub rank: usize,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckDesc {
    pub expr: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DimsDesc {
    pub out: String,
    pub inp: String,
}

/// How a model's `quantization_config` is read for a format.
///
/// Every top-level key of the configuration is either read (a parameter's `config` path starts with
/// it), declared `inert`, named by `skip`, named by a check, or refused by name — a key the descriptor
/// does not know may change what the stored tensors mean.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigDesc {
    /// Keys that never change what the stored tensors mean (tooling, calibration, kernel choices).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inert: Vec<String>,
    /// The key that lists the modules kept in float (`ignore`, `modules_to_not_convert`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip: Option<String>,
    /// How a `skip` entry names a module: `exact` (the full module name; an entry `re:<pattern>` is a
    /// regular expression of the subset `^ $ . .* \x` and literals) or `contains` (a substring of it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_match: Option<String>,
    /// `never`: the language-model head is not stored in this format; `unless_skipped`: it is, unless
    /// `skip` names it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lm_head: Option<String>,
    /// Conditions on the configuration; all must hold.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<ConfigCheck>,
}

/// The value at a dotted `path` of the configuration (an absent key is `null`) must be one of
/// `one_of`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigCheck {
    pub path: String,
    pub one_of: Vec<serde_json::Value>,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParamDesc {
    /// The `quantization_config` key it is read from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<i64>,
    /// Values the format is defined for (others are refused).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed: Vec<i64>,
    /// A string-valued configuration key read through this table (`"gptq_v2": 1`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub map: BTreeMap<String, i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableDesc {
    /// Entry width in bits (`8 16 32 64`), and whether entries are signed.
    pub bits: u32,
    #[serde(default)]
    pub signed: bool,
    /// The entries, little-endian, hex.
    pub hex: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecodeDesc {
    /// `integers`: `W = scale·(q − zero) − min` over stored integers (the exact lowering reads
    /// them); `floats`: `value` per element.
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<GroupDesc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub q: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zero: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// The range `q` lies in: each decoded code is checked against it, and the lowering reads the
    /// code width from it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<CodeDesc>,
    /// `tensors` layouts: whether the lowering must carry a per-group offset term (codes minus the zero
    /// point do not fit `i8`), as an expression over the parameters. Absent: the conservative rule (a
    /// float offset, or a data-dependent zero over a code range that does not fit).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset_term: Option<String>,
    /// `tensors` layouts: whether the lowering gathers the input through the group index first, as an
    /// expression over the parameters. Absent: whether `group.index` is declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<String>,
}

/// A number a descriptor gives literally, or as an expression over the format's parameters
/// (`group_size`, resolved from the model's configuration).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SizeDesc {
    Fixed(usize),
    Expr(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupDesc {
    /// Columns per group (every group has exactly this many).
    pub size: SizeDesc,
    /// `tensors` layouts: the group of column `i` (default `i / size`); a column order the lowering
    /// gathers through (GPTQ's `g_idx`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<String>,
}

/// An integer a descriptor gives literally or as an expression over the format's parameters
/// (`(1 << bits) - 1`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ValDesc {
    Int(i64),
    Expr(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeDesc {
    pub min: ValDesc,
    pub max: ValDesc,
}

/// A resolved code range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodeRange {
    pub min: i64,
    pub max: i64,
}

impl CodeDesc {
    /// The literal range (a blocks format's; an expression is an error there).
    pub fn literal(&self) -> Result<CodeRange, DslError> {
        match (&self.min, &self.max) {
            (ValDesc::Int(a), ValDesc::Int(b)) => Ok(CodeRange { min: *a, max: *b }),
            _ => Err(DslError("a blocks format's decode.code is a pair of numbers".into())),
        }
    }
}

impl CodeRange {
    /// Checked against what an `i16` code holds.
    pub fn check(self) -> Result<Self, DslError> {
        if self.min > self.max || self.min < i16::MIN as i64 || self.max > i16::MAX as i64 {
            return Err(DslError(format!("decode.code {}..={} is not within i16", self.min, self.max)));
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestVector {
    /// `blocks` formats: one or more blocks' bytes.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub block_hex: String,
    /// `tensors` formats: the role tensors of one weight, by role name (an optional role may be absent).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub roles: BTreeMap<String, TestRole>,
    /// `tensors` formats: the `quantization_config` the parameters are read from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,
    /// The `f32` values the dequantised weight must be, little-endian, hex — from an independent
    /// implementation.
    pub values_f32_hex: String,
}

/// One role tensor of a `tensors` test vector.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestRole {
    pub dtype: String,
    pub shape: Vec<usize>,
    /// Little-endian, row-major, hex.
    pub hex: String,
}

impl QuantFormatDesc {
    pub fn from_json(text: &str) -> Result<Self, DslError> {
        let d: QuantFormatDesc = serde_json::from_str(text).map_err(|e| DslError(format!("quant-format descriptor: {e}")))?;
        if d.schema != QUANT_FORMAT_SCHEMA_V1 {
            return Err(DslError(format!("quant-format descriptor: schema `{}`, not {QUANT_FORMAT_SCHEMA_V1}", d.schema)));
        }
        if d.name.is_empty() || d.name.len() > 64 || !d.name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
            return Err(DslError(format!("quant-format descriptor: `{}` is not a name (≤ 64 of A-Z a-z 0-9 _ -)", d.name)));
        }
        Ok(d)
    }

    /// The canonical text: every object's keys sorted, no whitespace — the bytes a digest is over.
    pub fn canonical_json(&self) -> String {
        let v = serde_json::to_value(self).expect("a descriptor serialises");
        let mut out = String::new();
        canon(&v, &mut out);
        out
    }

    /// `BLAKE2b-256` over the canonical text, keyed `misaka.palw.quant-format.v1`: what a runtime
    /// pack pins.
    pub fn digest(&self) -> [u8; 32] {
        let h = blake2b_simd::Params::new().hash_length(32).key(QUANT_FORMAT_SCHEMA_V1.as_bytes()).hash(self.canonical_json().as_bytes());
        let mut out = [0u8; 32];
        out.copy_from_slice(h.as_bytes());
        out
    }

    pub fn digest_hex(&self) -> String {
        self.digest().iter().map(|b| format!("{b:02x}")).collect()
    }
}

fn canon(v: &serde_json::Value, out: &mut String) {
    use serde_json::Value;
    match v {
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(k).expect("string"));
                out.push(':');
                canon(&m[*k], out);
            }
            out.push('}');
        }
        Value::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                canon(x, out);
            }
            out.push(']');
        }
        other => out.push_str(&serde_json::to_string(other).expect("scalar")),
    }
}

pub(crate) fn unhex(s: &str) -> Result<Vec<u8>, DslError> {
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return Err(DslError("a hex string of odd length".into()));
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| DslError(format!("hex: {e}")))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const Q4_0: &str = r#"{ "schema": "misaka.palw.quant-format.v1", "name": "Q4_0",
        "ids": [ { "scheme": "ggml", "id": 2 } ],
        "layout": { "kind": "blocks", "elems": 32, "bytes": 18,
                    "fields": [ { "name": "d", "at": 0, "type": "f16" }, { "name": "qs", "at": 2, "type": "u8", "count": 16 } ] },
        "decode": { "target": "integers", "group": { "size": 32 }, "q": "(qs[e % 16] >> (4 * (e / 16))) & 15",
                    "scale": "d", "zero": "8", "code": { "min": 0, "max": 15 } } }"#;

    #[test]
    fn a_descriptor_parses_and_its_digest_ignores_layout_of_the_text() {
        let a = QuantFormatDesc::from_json(Q4_0).expect("parses");
        // The same descriptor with its keys in another order and other whitespace.
        let v: serde_json::Value = serde_json::from_str(Q4_0).expect("json");
        let shuffled = serde_json::to_string(&v).expect("compact");
        let b = QuantFormatDesc::from_json(&shuffled).expect("parses");
        assert_eq!(a, b);
        assert_eq!(a.digest(), b.digest());
        assert_eq!(a.canonical_json(), b.canonical_json());
        assert!(a.canonical_json().starts_with("{\"decode\":"), "keys sorted");
        // One character of the decode changes the identity.
        let c = QuantFormatDesc::from_json(&Q4_0.replace("\"zero\": \"8\"", "\"zero\": \"7\"")).expect("parses");
        assert_ne!(a.digest(), c.digest());
    }

    #[test]
    fn unknown_keys_a_foreign_schema_and_bad_names_are_refused() {
        assert!(QuantFormatDesc::from_json(&Q4_0.replace("\"doc\"", "\"x\"").replace("\"ids\"", "\"idz\"")).is_err());
        assert!(QuantFormatDesc::from_json(&Q4_0.replace("quant-format.v1", "quant-format.v2")).is_err());
        assert!(QuantFormatDesc::from_json(&Q4_0.replace("\"Q4_0\"", "\"Q4 0\"")).is_err());
    }
}
