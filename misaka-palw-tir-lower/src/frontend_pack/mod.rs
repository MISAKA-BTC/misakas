//! An untrusted, content-addressed frontend emits the frozen TIR grammar directly.
//! No ModelSpec, feature registry, model name, executable plugin or network access is involved.
//! Source equivalence and full-task support remain separate evidence, not claims made by compilation.

pub mod program;
mod stream;
pub use stream::{BuildRecord, Conversion, SourceRecord};

use crate::adapter::{canonical_json, expr::Env};
use crate::cfg::Cfg;
use crate::hf_schema::TensorIndex;
use crate::weights::{TensorMeta, TensorSource};
use crate::{LowerError, Result};
use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir::admit::{TirAdmissionV1, TirAdmitInputsV1, tir_admit_program_v1};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

pub const FORMAT: &str = "misaka.palw.tir-frontend-pack.v1";
pub const MAX_PACK_BYTES: usize = 2 << 20;
pub const MAX_SOURCE_TENSORS: usize = 65_536;
pub const HASH_KEY: &[u8] = b"MISAKA/PALW/TIR/FRONTEND/PACK/V1";

/// Output files must not replace public inputs or another output with a different format.
pub fn distinct_output(path: &Path, inputs: &[std::path::PathBuf]) -> Result<()> {
    fn resolve(path: &Path) -> std::io::Result<std::path::PathBuf> {
        if path.exists() {
            std::fs::canonicalize(path)
        } else {
            let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
            Ok(std::fs::canonicalize(parent)?.join(path.file_name().ok_or(std::io::ErrorKind::InvalidInput)?))
        }
    }
    let out = resolve(path).map_err(|e| LowerError::Io(e.to_string()))?;
    if inputs.iter().any(|input| resolve(input).is_ok_and(|input| input == out)) {
        return Err(bad("FRONTEND_OUTPUT_CONFLICT: output aliases an input or another output"));
    }
    Ok(())
}

fn bad(msg: impl Into<String>) -> LowerError {
    LowerError::bad(msg)
}
fn input_bound(v: &Value, depth: usize, left: &mut usize) -> Result<()> {
    if depth > crate::adapter::expr::MAX_DEPTH {
        return Err(bad("FRONTEND_EXPANSION_LIMIT: input depth"));
    }
    let bytes = 16 + v.as_str().map_or(0, str::len);
    *left = left.checked_sub(bytes).ok_or_else(|| bad("FRONTEND_EXPANSION_LIMIT: input allocation"))?;
    match v {
        Value::Array(a) => {
            for child in a {
                input_bound(child, depth + 1, left)?;
            }
        }
        Value::Object(o) => {
            for (key, child) in o {
                *left = left.checked_sub(key.len()).ok_or_else(|| bad("FRONTEND_EXPANSION_LIMIT: input allocation"))?;
                input_bound(child, depth + 1, left)?;
            }
        }
        _ => {}
    }
    Ok(())
}
pub(crate) fn digest(key: &[u8], bytes: &[u8]) -> String {
    program::hex(blake2b_simd::Params::new().hash_length(64).key(key).hash(bytes).as_bytes())
}

/// Pin the compiler and reader sources plus dependency lock, independently of a machine path.
pub fn compiler_digest() -> String {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(b"MISAKA/PALW/TIR/FRONTEND/COMPILER").to_state();
    for source in [
        include_str!("mod.rs"),
        include_str!("program.rs"),
        include_str!("stream.rs"),
        include_str!("../adapter/expr.rs"),
        include_str!("../adapter/mod.rs"),
        include_str!("../cfg.rs"),
        include_str!("../weights/mod.rs"),
        include_str!("../../../Cargo.lock"),
        include_str!("../../../misaka-palw-tir/src/validate.rs"),
        include_str!("../../../misaka-palw-tir/src/admit.rs"),
        include_str!("../../../misaka-palw-tir/src/program.rs"),
        include_str!("../../../misaka-palw-tir/src/types.rs"),
        include_str!("../../../misaka-palw-tir/src/prim.rs"),
        include_str!("../../../misaka-palw-tir-artifact/src/lib.rs"),
    ] {
        h.update(&(source.len() as u64).to_le_bytes());
        h.update(source.as_bytes());
    }
    program::hex(h.finalize().as_bytes())
}

/// Labels the author's intended task. It is provenance, never a support verdict.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub task: String,
    pub completeness: Completeness,
    pub components: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Completeness {
    Full,
    Partial,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Import {
    /// Preserve the stored integer exactly; narrowing outside the target type refuses.
    Integer,
    /// IEEE finite value × 2^shift, rounded by the named rule; no platform float arithmetic.
    FixedPoint { shift: i16, round: program::Round, overflow: Overflow },
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Overflow {
    Reject,
    Saturate,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub param: u16,
    pub layer: Option<u16>,
    pub source: String,
    pub import: Import,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    format: String,
    id: String,
    scope: Scope,
    #[serde(default)]
    defaults: Map<String, Value>,
    #[serde(default)]
    inert: Vec<String>,
    #[serde(default)]
    vars: BTreeMap<String, Value>,
    program: Value,
    bindings: Value,
}

/// Parsed data, identified by the canonical effective JSON rather than a local path or author.
#[derive(Clone, Debug)]
pub struct FrontendPack {
    definition: Definition,
    hash: String,
}

impl FrontendPack {
    pub fn parse(text: &str) -> Result<Self> {
        if text.len() > MAX_PACK_BYTES {
            return Err(bad("FRONTEND_EXPANSION_LIMIT: pack bytes"));
        }
        let v: Value = serde_json::from_str(text).map_err(|e| bad(format!("FRONTEND_ENCODING: {e}")))?;
        let mut remaining = crate::adapter::expr::MAX_EXPANSION_BYTES;
        input_bound(&v, 0, &mut remaining)?;
        let hash = digest(HASH_KEY, canonical_json(&v).as_bytes());
        let definition: Definition = serde_json::from_value(v).map_err(|e| bad(format!("FRONTEND_ENCODING: {e}")))?;
        if definition.format != FORMAT {
            return Err(bad("KERNEL_EXTENSION_REQUIRED: frontend format"));
        }
        if definition.id.is_empty()
            || definition.id.len() > 128
            || definition.scope.task.is_empty()
            || definition.scope.task.len() > 128
            || definition.scope.components.len() > 64
            || definition.scope.components.iter().any(|s| s.is_empty() || s.len() > 128)
            || definition.vars.len() > 4096
            || definition.vars.keys().any(|s| s.is_empty() || s.len() > 128)
        {
            return Err(bad("FRONTEND_ENCODING: invalid id, scope or variables"));
        }
        Ok(Self { definition, hash })
    }

    pub fn read(path: &Path) -> Result<Self> {
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|e| LowerError::Io(e.to_string()))?
            .take((MAX_PACK_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| LowerError::Io(e.to_string()))?;
        Self::parse(std::str::from_utf8(&bytes).map_err(|e| bad(format!("FRONTEND_ENCODING: {e}")))?)
    }
    pub fn hash(&self) -> &str {
        &self.hash
    }
    pub fn id(&self) -> &str {
        &self.definition.id
    }

    /// Only source headers are read here. All bindings and TIR admission precede any weight read.
    /// `inputs` comes from the caller's network profile; successful compilation does not activate it.
    pub fn compile(&self, config: &Value, source: &dyn TensorSource, inputs: &TirAdmitInputsV1) -> Result<Compiled> {
        let d = &self.definition;
        let map = config.as_object().ok_or_else(|| bad("FRONTEND_ENCODING: config is an object"))?;
        let mut remaining = MAX_PACK_BYTES;
        input_bound(config, 0, &mut remaining)?;
        let names = source.names();
        if names.len() > MAX_SOURCE_TENSORS {
            return Err(bad("FRONTEND_EXPANSION_LIMIT: source inventory"));
        }
        let available: BTreeSet<String> = names.iter().cloned().collect();
        if available.len() != names.len() {
            return Err(bad("FRONTEND_BINDING: duplicate source name"));
        }
        let index = TensorIndex::from_source(source);
        let cfg = Cfg::new_strict(self.id(), map, "");
        cfg.inert(&d.inert.iter().map(String::as_str).collect::<Vec<_>>());
        let env = Env::new(&cfg, None, &d.defaults, Some(&index), self.id());
        for (name, expr) in &d.vars {
            env.define_global(name, expr.clone());
        }
        env.force_globals(&d.vars.keys().cloned().collect::<Vec<_>>())?;
        let program: program::Program = serde_json::from_value(env.eval(&d.program)?)
            .map_err(|e| LowerError::not_lowerable(format!("FRONTEND_ENCODING_OR_KERNEL_EXTENSION_REQUIRED: {e}")))?;
        let program = program.compile()?;
        let admission =
            tir_admit_program_v1(&program, inputs).map_err(|e| LowerError::not_lowerable(format!("TIR_ADMISSION: {e}")))?;
        let bindings: Vec<Binding> =
            serde_json::from_value(env.eval(&d.bindings)?).map_err(|e| bad(format!("FRONTEND_BINDING: {e}")))?;
        cfg.finish()?;
        let assumed_defaults = env.assumed.borrow().iter().cloned().collect();
        let expected: BTreeSet<_> = misaka_palw_tir_artifact::param_instances_v1(&program)
            .into_iter()
            .enumerate()
            .flat_map(|(j, layers)| layers.into_iter().map(move |l| (j as u16, l)))
            .collect();
        let mut resolved = BTreeMap::new();
        let mut used = BTreeSet::new();
        for b in bindings {
            if !expected.contains(&(b.param, b.layer)) || resolved.contains_key(&(b.param, b.layer)) {
                return Err(bad("FRONTEND_BINDING: unexpected or repeated parameter instance"));
            }
            let meta = source.metadata(&b.source).ok_or_else(|| bad(format!("FRONTEND_BINDING: missing tensor {}", b.source)))?;
            let param = &program.params[b.param as usize];
            if meta.shape != param.shape.iter().map(|n| *n as usize).collect::<Vec<_>>() {
                return Err(bad(format!("FRONTEND_BINDING: shape of {}", b.source)));
            }
            let width = stored_width(&meta.dtype)
                .ok_or_else(|| LowerError::not_lowerable(format!("SOURCE_FORMAT_UNSUPPORTED: {}", meta.dtype)))?;
            let count = meta
                .shape
                .iter()
                .try_fold(1u64, |n, d| n.checked_mul(*d as u64))
                .ok_or_else(|| bad("FRONTEND_BINDING: shape overflow"))?;
            if count.checked_mul(width as u64) != Some(meta.bytes) {
                return Err(bad("FRONTEND_BINDING: source byte count"));
            }
            match b.import {
                Import::Integer if !is_integer(&meta.dtype) => {
                    return Err(bad("FRONTEND_BINDING: integer import requires integer storage"));
                }
                Import::FixedPoint { shift, .. } if is_integer(&meta.dtype) || !(-64..=64).contains(&shift) => {
                    return Err(bad("FRONTEND_BINDING: fixed point requires IEEE storage and shift -64..=64"));
                }
                _ => {}
            }
            used.insert(b.source.clone());
            resolved.insert((b.param, b.layer), Resolved { binding: b, meta });
        }
        if resolved.len() != expected.len() {
            return Err(bad("FRONTEND_BINDING: missing parameter instance"));
        }
        if used != available {
            return Err(LowerError::not_lowerable(format!("TENSOR_UNREAD: {:?}", available.difference(&used).collect::<Vec<_>>())));
        }
        Ok(Compiled {
            program,
            admission,
            bindings: resolved,
            pack_hash: self.hash.clone(),
            pack_id: d.id.clone(),
            scope: d.scope.clone(),
            config_hash: digest(b"MISAKA/PALW/TIR/FRONTEND/CONFIG/V1", canonical_json(config).as_bytes()),
            assumed_defaults,
        })
    }
}

pub(crate) fn is_integer(dtype: &str) -> bool {
    matches!(dtype, "I8" | "I16" | "I32" | "I64" | "U8" | "U16" | "U32" | "U64")
}
pub(crate) fn stored_width(dtype: &str) -> Option<usize> {
    match dtype {
        "I8" | "U8" => Some(1),
        "I16" | "U16" | "BF16" | "F16" => Some(2),
        "I32" | "U32" | "F32" => Some(4),
        "I64" | "U64" | "F64" => Some(8),
        _ => None,
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Resolved {
    pub binding: Binding,
    pub meta: TensorMeta,
}

pub struct Compiled {
    program: TirProgramV1,
    admission: TirAdmissionV1,
    bindings: BTreeMap<(u16, Option<u16>), Resolved>,
    pack_hash: String,
    pack_id: String,
    scope: Scope,
    config_hash: String,
    assumed_defaults: Vec<String>,
}
impl Compiled {
    pub fn program(&self) -> &TirProgramV1 {
        &self.program
    }
    pub fn admission(&self) -> &TirAdmissionV1 {
        &self.admission
    }
}
