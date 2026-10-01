//! **Quantisation formats are data** (RFC-0002 Part II, the quant registry).
//!
//! A format — how a quantised weight's bytes become integers — is a *descriptor*
//! ([`desc::QuantFormatDesc`], JSON `misaka.palw.quant-format.v1`), and ONE interpreter decodes any
//! descriptor ([`blocks`]), so a model whose weights use a format this build has never seen is
//! admitted by supplying a file, not by waiting for a release. The decoded stored integers feed ONE
//! lowering recipe (`crate::lower::qlinear`), which does not know any format either.
//!
//! * [`expr`] — the closed, total, deterministic expression language descriptors are written in.
//! * [`desc`] — the schema, its canonical text and digest (what a runtime pack pins).
//! * [`blocks`] — the interpreter for tensors stored as rows of fixed-size blocks (ggml's types).
//! * [`tensors`] — the interpreter for a weight stored as several named checkpoint tensors (GPTQ,
//!   AWQ, FP8 with block scales, compressed-tensors): the descriptor says which tensors (`roles`),
//!   how a `quantization_config` is read (`config`: the keys it knows, the conditions under which the
//!   tensors mean what the descriptor says, the modules left in float), and how each weight decodes.
//! * [`QuantRegistry`] — the built-in descriptors (`quant-formats/*.json`, embedded) plus any a
//!   caller supplies; a GGUF tensor type or a `quantization_config` is looked up in it, and a type
//!   it does not hold is a named refusal that says what to supply ([`NeedsDescriptor`]).
//!
//! A descriptor carries its own test vectors — block bytes, or the role tensors of a weight and
//! its configuration, and the `f32` values an independent implementation decodes them to — and one
//! that fails them is refused when loaded. The built-in vectors come from gguf-py (ggml types) and
//! from torch re-implementations of each library's own dequantiser (`tools/gen_quant_formats.py`).

pub mod blocks;
pub mod desc;
pub mod expr;
pub mod tensors;

use crate::error::{LowerError, Result};
use blocks::BlocksFormat;
use desc::{FormatId, LayoutDesc, QuantFormatDesc, unhex};
use tensors::{RoleTensor, TensorsFormat};
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

/// A compiled, self-tested descriptor.
#[derive(Clone, Debug)]
pub struct QuantFormat {
    pub desc: QuantFormatDesc,
    digest: [u8; 32],
    blocks: Option<BlocksFormat>,
    tensors: Option<TensorsFormat>,
}

impl QuantFormat {
    /// Parse, compile and test a descriptor.
    pub fn from_json(text: &str) -> Result<QuantFormat> {
        let bad = |e: expr::DslError| LowerError::bad(format!("quant-format descriptor: {e}"));
        let desc = QuantFormatDesc::from_json(text).map_err(bad)?;
        let (blocks, tensors) = match &desc.layout {
            LayoutDesc::Blocks { .. } => {
                (Some(BlocksFormat::compile(&desc).map_err(|e| LowerError::bad(format!("quant format `{}`: {e}", desc.name)))?), None)
            }
            LayoutDesc::Tensors { .. } => {
                (None, Some(TensorsFormat::compile(&desc).map_err(|e| LowerError::bad(format!("quant format `{}`: {e}", desc.name)))?))
            }
        };
        let f = QuantFormat { digest: desc.digest(), desc, blocks, tensors };
        f.run_tests()?;
        Ok(f)
    }

    pub fn name(&self) -> &str {
        &self.desc.name
    }
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
    pub fn digest_hex(&self) -> String {
        self.digest.iter().map(|b| format!("{b:02x}")).collect()
    }
    pub fn ids(&self) -> &[FormatId] {
        &self.desc.ids
    }
    /// The GGUF (ggml) tensor type id this format is, if it is one.
    pub fn ggml_id(&self) -> Option<u32> {
        self.desc.ids.iter().find(|i| i.scheme == "ggml").and_then(|i| i.id)
    }
    pub fn as_blocks(&self) -> Option<&BlocksFormat> {
        self.blocks.as_ref()
    }
    pub fn as_tensors(&self) -> Option<&TensorsFormat> {
        self.tensors.as_ref()
    }

    /// The descriptor's own vectors: each block's bytes must decode to its `f32` values, bit for bit.
    fn run_tests(&self) -> Result<()> {
        if let Some(t) = &self.tensors {
            return self.run_tensor_tests(t);
        }
        let Some(b) = &self.blocks else { return Ok(()) };
        for (n, t) in self.desc.tests.iter().enumerate() {
            let raw = unhex(&t.block_hex).map_err(|e| LowerError::bad(format!("{} test {n}: {e}", self.name())))?;
            let want = unhex(&t.values_f32_hex).map_err(|e| LowerError::bad(format!("{} test {n}: {e}", self.name())))?;
            if raw.is_empty() || raw.len() % b.bytes != 0 {
                return Err(LowerError::bad(format!("{} test {n}: {} bytes is not whole {}-byte blocks", self.name(), raw.len(), b.bytes)));
            }
            let blocks = raw.len() / b.bytes;
            let got = b
                .decode_floats(&raw, 1, blocks * b.elems)
                .map_err(|e| LowerError::bad(format!("quant format `{}` test {n}: {e}", self.name())))?;
            if want.len() != got.len() * 4 {
                return Err(LowerError::bad(format!("{} test {n}: {} values expected, {} decoded", self.name(), want.len() / 4, got.len())));
            }
            for (i, (g, w)) in got.iter().zip(want.chunks_exact(4)).enumerate() {
                let w = f32::from_le_bytes([w[0], w[1], w[2], w[3]]);
                // Bit for bit, except that zero has two bit patterns and the sign of a zero weight is the
                // sign of an operand that was itself zero (0.0 · −1): numerically one value.
                if g.to_bits() != w.to_bits() && !(*g == 0.0 && w == 0.0) {
                    return Err(LowerError::bad(format!(
                        "quant format `{}` fails its own test {n} at element {i}: decodes to {g:e}, the vector says {w:e}",
                        self.name()
                    )));
                }
            }
        }
        Ok(())
    }
}

impl QuantFormat {
    /// A `tensors` format's vectors: the role tensors of one weight and the configuration, the weight
    /// they must decode to (float32, bit for bit).
    fn run_tensor_tests(&self, t: &TensorsFormat) -> Result<()> {
        let roles: Vec<(String, bool)> = t.roles().map(|(n, _, req)| (n.to_string(), req)).collect();
        for (n, v) in self.desc.tests.iter().enumerate() {
            let bad = |m: String| LowerError::bad(format!("quant format `{}` test {n}: {m}", self.name()));
            if let Some(k) = v.roles.keys().find(|k| !roles.iter().any(|(r, _)| r == *k)) {
                return Err(bad(format!("a tensor for role `{k}`, which the format does not declare")));
            }
            let mut tensors: Vec<Option<RoleTensor>> = Vec::with_capacity(roles.len());
            for (r, required) in &roles {
                match v.roles.get(r) {
                    Some(tr) => tensors.push(Some(RoleTensor { shape: tr.shape.clone(), dtype: tr.dtype.clone(), data: unhex(&tr.hex).map_err(|e| bad(e.to_string()))? })),
                    None if *required => return Err(bad(format!("no tensor for the required role `{r}`"))),
                    None => tensors.push(None),
                }
            }
            let cfg = v.config.clone().unwrap_or_else(|| serde_json::json!({}));
            // A format that says how its configuration is read is tested through that reading.
            let params = if self.desc.config.is_some() { t.read_config(&cfg).map_err(|e| bad(e.to_string()))?.params } else { t.resolve_params(&cfg).map_err(|e| bad(e.to_string()))? };
            let (got, out, inp) = t.decode_floats(&tensors, &params).map_err(|e| bad(e.to_string()))?;
            let want = unhex(&v.values_f32_hex).map_err(|e| bad(e.to_string()))?;
            if want.len() != got.len() * 4 {
                return Err(bad(format!("{} values expected, {} decoded ({out} × {inp})", want.len() / 4, got.len())));
            }
            for (i, (g, w)) in got.iter().zip(want.chunks_exact(4)).enumerate() {
                let w = f32::from_le_bytes([w[0], w[1], w[2], w[3]]);
                if g.to_bits() != w.to_bits() && !(*g == 0.0 && w == 0.0) {
                    return Err(bad(format!("fails at element {i} (row {}, column {}): decodes to {g:e}, the vector says {w:e}", i / inp, i % inp)));
                }
            }
        }
        Ok(())
    }
}

/// What a GGUF file (or a checkpoint) holds that no descriptor in the registry describes: the
/// refusal that says what to supply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NeedsDescriptor {
    pub scheme: String,
    pub id: Option<u32>,
    /// A name the file's metadata gives it, when it gives one.
    pub name: Option<String>,
    pub tensors: Vec<String>,
}

impl NeedsDescriptor {
    pub fn message(&self, known: &[String]) -> String {
        let what = match (&self.id, &self.name) {
            (_, Some(n)) if self.scheme == "config" => format!("quantization_config quant_method={n}"),
            (Some(id), Some(n)) => format!("{} type {id} (`{n}`)", self.scheme),
            (Some(id), None) => format!("{} type {id}", self.scheme),
            (None, Some(n)) => format!("{} format `{n}`", self.scheme),
            (None, None) => format!("a {} format", self.scheme),
        };
        let some: Vec<&String> = self.tensors.iter().take(3).collect();
        let known_note = match (self.scheme.as_str(), &self.name) {
            ("config", Some(n)) => known_undescribed_method(n.split('/').next().unwrap_or(n))
                .map(|k| format!(" `{}` is known and not yet described ({}): {}.", k.method, k.status, k.note))
                .unwrap_or_default(),
            _ => String::new(),
        };
        let which = if self.tensors.is_empty() {
            String::new()
        } else {
            format!(": {} tensor(s) need one ({}{})", self.tensors.len(), some.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "), if self.tensors.len() > 3 { ", …" } else { "" })
        };
        format!(
            "{what} has no quant-format descriptor{which}.{known_note} \
             A descriptor is a file (misaka.palw.quant-format.v1) — supply it with --quant-format <file.json> or a pack's quant.descriptors; \
             no code change is needed. Without one: use the model's original safetensors, or re-quantise it to a described type ({})",
            known.join(", ")
        )
    }
}

/// A quantisation method a checkpoint can announce that no descriptor reads yet.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct KnownUndescribed {
    pub method: String,
    /// `queued` (next to be described) or `known`.
    pub status: String,
    pub title: String,
    /// What writing its descriptor takes (or why it cannot be one yet).
    pub note: String,
}

/// The methods that are known and not yet described (`quant-formats/known-undescribed.json`).
pub fn known_undescribed() -> &'static [KnownUndescribed] {
    static K: OnceLock<Vec<KnownUndescribed>> = OnceLock::new();
    K.get_or_init(|| {
        let v: serde_json::Value = serde_json::from_str(include_str!("../../quant-formats/known-undescribed.json")).expect("the known-undescribed list is JSON");
        assert_eq!(v["schema"], "misaka.palw.quant-known-undescribed.v1");
        v["methods"]
            .as_array()
            .expect("methods")
            .iter()
            .map(|m| KnownUndescribed {
                method: m["method"].as_str().unwrap_or_default().to_string(),
                status: m["status"].as_str().unwrap_or("known").to_string(),
                title: m["title"].as_str().unwrap_or_default().to_string(),
                note: m["note"].as_str().unwrap_or_default().to_string(),
            })
            .collect()
    })
}

/// The entry for a `quant_method` that is known and not described, if it is one (a described method is not).
pub fn known_undescribed_method(method: &str) -> Option<&'static KnownUndescribed> {
    let m = method.to_ascii_lowercase();
    known_undescribed().iter().find(|k| k.method == m)
}

/// Descriptors by name and by the ids a checkpoint announces them with.
#[derive(Clone, Debug, Default)]
pub struct QuantRegistry {
    formats: Vec<Arc<QuantFormat>>,
    by_ggml: BTreeMap<u32, usize>,
    by_name: BTreeMap<String, usize>,
    /// A `quantization_config`'s `quant_method` (`gptq`, `compressed-tensors/pack-quantized`).
    by_config: BTreeMap<String, usize>,
}

macro_rules! builtin {
    ($($f:literal),* $(,)?) => {
        &[$(include_str!(concat!("../../quant-formats/", $f, ".json"))),*]
    };
}

/// The built-in descriptors, embedded.
const BUILTIN: &[&str] = builtin!(
    "f32", "f16", "bf16", "f64", "q4_0", "q4_1", "q5_0", "q5_1", "q8_0", "q2_k", "q3_k", "q4_k", "q5_k", "q6_k", "iq4_nl", "iq4_xs",
    "tq1_0", "tq2_0", "mxfp4", "nvfp4", "q1_0", "q2_0", "iq2_xxs", "iq2_xs", "iq2_s", "iq3_xxs", "iq3_s", "iq1_s", "iq1_m", "gptq", "awq",
    "fp8_block", "ct_pack", "ct_fp8", "ct_int8",
);

impl QuantRegistry {
    /// The built-in registry (every descriptor in `quant-formats/`, tested when first used).
    pub fn builtin() -> &'static QuantRegistry {
        static R: OnceLock<QuantRegistry> = OnceLock::new();
        R.get_or_init(|| {
            let mut r = QuantRegistry::default();
            for text in BUILTIN {
                let f = QuantFormat::from_json(text).unwrap_or_else(|e| panic!("a built-in quant format is broken: {e}"));
                r.add(f).unwrap_or_else(|e| panic!("a built-in quant format clashes: {e}"));
            }
            r
        })
    }

    /// Add a descriptor. A name or id another descriptor holds is refused unless it is the very same
    /// descriptor (equal digest): a supplied file cannot redefine a type the registry already knows.
    pub fn add(&mut self, f: QuantFormat) -> Result<()> {
        if let Some(&i) = self.by_name.get(f.name()) {
            return if self.formats[i].digest == f.digest {
                Ok(())
            } else {
                Err(LowerError::bad(format!("a quant format named `{}` is already registered, with another descriptor", f.name())))
            };
        }
        if let Some(id) = f.ggml_id()
            && let Some(&i) = self.by_ggml.get(&id)
        {
            return Err(LowerError::bad(format!("ggml type {id} is already `{}`; `{}` cannot redefine it", self.formats[i].name(), f.name())));
        }
        let methods: Vec<String> = f.ids().iter().filter(|i| i.scheme == "config").filter_map(|i| i.method.clone()).collect();
        for m in &methods {
            if let Some(&i) = self.by_config.get(m) {
                return Err(LowerError::bad(format!("quant_method `{m}` is already read by `{}`; `{}` cannot redefine it", self.formats[i].name(), f.name())));
            }
        }
        let i = self.formats.len();
        for m in methods {
            self.by_config.insert(m, i);
        }
        if let Some(id) = f.ggml_id() {
            self.by_ggml.insert(id, i);
        }
        self.by_name.insert(f.name().to_string(), i);
        self.formats.push(Arc::new(f));
        Ok(())
    }

    /// The built-in registry extended with the descriptor files at `paths` (`--quant-format`). A file
    /// that is malformed, fails its own test vectors or redefines a type the registry holds is
    /// refused by its path.
    pub fn with_files(paths: &[std::path::PathBuf]) -> Result<QuantRegistry> {
        let mut extra = Vec::new();
        for p in paths {
            let text = std::fs::read_to_string(p).map_err(|e| LowerError::Io(format!("{}: {e}", p.display())))?;
            extra.push(QuantFormat::from_json(&text).map_err(|e| LowerError::bad(format!("{}: {e}", p.display())))?);
        }
        QuantRegistry::builtin().with(extra)
    }

    /// This registry and `extra` (parsed descriptors the caller supplies).
    pub fn with(&self, extra: Vec<QuantFormat>) -> Result<QuantRegistry> {
        let mut r = self.clone();
        for f in extra {
            r.add(f)?;
        }
        Ok(r)
    }

    pub fn ggml(&self, id: u32) -> Option<&Arc<QuantFormat>> {
        self.by_ggml.get(&id).map(|i| &self.formats[*i])
    }
    pub fn named(&self, name: &str) -> Option<&Arc<QuantFormat>> {
        self.by_name.get(name).map(|i| &self.formats[*i])
    }
    /// The format a `quantization_config` announces: `quant_method` with its `format` first
    /// (`compressed-tensors/pack-quantized`), then `quant_method` alone.
    pub fn config(&self, method: &str, format: Option<&str>) -> Option<&Arc<QuantFormat>> {
        let method = method.to_ascii_lowercase();
        format
            .and_then(|f| self.by_config.get(&format!("{method}/{}", f.to_ascii_lowercase())))
            .or_else(|| self.by_config.get(&method))
            .map(|i| &self.formats[*i])
    }
    /// The `quant_method` ids this registry reads, for a refusal that lists them.
    pub fn config_methods(&self) -> Vec<String> {
        self.by_config.keys().cloned().collect()
    }
    pub fn all(&self) -> &[Arc<QuantFormat>] {
        &self.formats
    }
    pub fn names(&self) -> Vec<String> {
        self.formats.iter().map(|f| f.name().to_string()).collect()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A 2-bit format nobody has registered: 16 weights in 6 bytes, a binary16 scale and four bytes of
    /// codes, `W = d·(q − 1)`. Written the way a model's author would write it.
    pub(crate) const CUSTOM: &str = r#"{ "schema": "misaka.palw.quant-format.v1", "name": "PQ2_TEST",
        "ids": [ { "scheme": "ggml", "id": 200 } ],
        "layout": { "kind": "blocks", "elems": 16, "bytes": 6,
                    "fields": [ { "name": "d", "at": 0, "type": "f16" }, { "name": "qs", "at": 2, "type": "u8", "count": 4 } ] },
        "decode": { "target": "integers", "group": { "size": 16 }, "q": "(qs[e / 4] >> (2 * (e % 4))) & 3",
                    "scale": "d", "zero": "1", "code": { "min": 0, "max": 3 } } }"#;

    #[test]
    fn the_builtin_registry_loads_and_every_descriptor_passes_its_vectors() {
        let r = QuantRegistry::builtin();
        // Loading ran every descriptor's own vectors; they are not empty.
        for f in r.all() {
            assert!(f.as_tensors().is_some() || !f.desc.tests.is_empty(), "{} carries no test vector", f.name());
            assert_eq!(f.digest(), f.desc.digest());
        }
        let ids: Vec<u32> = r.all().iter().filter_map(|f| f.ggml_id()).collect();
        for id in [0u32, 1, 2, 3, 6, 7, 8, 10, 11, 12, 13, 14, 16, 17, 18, 19, 20, 21, 22, 23, 28, 29, 30, 34, 35, 39, 40, 41, 42] {
            assert!(ids.contains(&id), "ggml type {id} has no built-in descriptor");
        }
        assert!(r.ggml(200).is_none() && r.ggml(43).is_none(), "an unknown id is unknown");
        eprintln!("{} built-in formats: {}", r.all().len(), r.names().join(" "));
    }

    #[test]
    fn a_descriptor_that_fails_its_own_vector_is_refused() {
        let q8 = include_str!("../../quant-formats/q8_0.json");
        assert!(QuantFormat::from_json(q8).is_ok());
        // Change the scale to `d * 2`: the independent implementation's values no longer come out.
        let wrong = q8.replace("\"scale\": \"d\"", "\"scale\": \"d * 2\"");
        let e = QuantFormat::from_json(&wrong).expect_err("must fail its vector");
        assert!(e.to_string().contains("fails its own test"), "{e}");
        // A code outside the declared range is an error at decode time, not a wrapped value.
        let narrow = q8.replace("\"max\": 127", "\"max\": 100");
        assert!(QuantFormat::from_json(&narrow).is_err());
    }

    #[test]
    fn a_registered_type_cannot_be_redefined_by_a_supplied_file() {
        let reg = QuantRegistry::builtin();
        let q4 = include_str!("../../quant-formats/q4_0.json");
        // The same descriptor again is fine (it is the same one); another one under the id is not.
        assert!(reg.with(vec![QuantFormat::from_json(q4).unwrap()]).is_ok());
        // (Its own vectors would refuse the changed decode, so the imposter drops them.)
        let mut v: serde_json::Value = serde_json::from_str(q4).unwrap();
        v["name"] = "Q4_0_EVIL".into();
        v["decode"]["zero"] = "7".into();
        v.as_object_mut().unwrap().remove("tests");
        let e = reg.with(vec![QuantFormat::from_json(&v.to_string()).unwrap()]).expect_err("redefines ggml type 2");
        assert!(e.to_string().contains("cannot redefine"), "{e}");
        // A new type is added and found by id and by name.
        let ext = reg.with(vec![QuantFormat::from_json(CUSTOM).unwrap()]).unwrap();
        assert_eq!(ext.ggml(200).map(|f| f.name()), Some("PQ2_TEST"));
        assert!(ext.named("PQ2_TEST").is_some() && reg.named("PQ2_TEST").is_none());
    }

    #[test]
    fn a_custom_format_decodes_with_no_code_added() {
        let f = QuantFormat::from_json(CUSTOM).unwrap();
        let b = f.as_blocks().unwrap();
        // d = 0.5 (0x3800), codes 0,1,2,3 repeating: bytes 0b11100100 = 0xE4 four times.
        let raw = [0x00u8, 0x38, 0xE4, 0xE4, 0xE4, 0xE4];
        let q = b.decode_integers(&raw, 1, 16).unwrap();
        assert_eq!(&q.q[..8], &[0, 1, 2, 3, 0, 1, 2, 3]);
        assert_eq!((q.scale.clone(), q.zero.clone(), q.group, q.bits, q.signed), (vec![0.5], vec![1], 16, 2, false));
        assert_eq!(b.decode_floats(&raw, 1, 16).unwrap()[..4], [-0.5, 0.0, 0.5, 1.0]);
        // Rows of another width, or bytes of another length, are refused.
        assert!(b.decode_integers(&raw, 1, 20).is_err());
        assert!(b.decode_integers(&raw[..5], 1, 16).is_err());
        // An offset term is needed only for a float offset or a code range i8 cannot hold.
        assert!(!b.offset_term());
    }
}
