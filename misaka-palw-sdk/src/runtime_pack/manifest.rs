//! **The runtime pack manifest** (`misaka.palw.runtime-pack.v1`, RFC-0002 Part II, requirement 2).
//!
//! One document that says everything needed to rebuild a class's artifact from public source and to
//! judge the result: the source files by hash, the frontend that read them (adapter by hash, the spec's
//! digest, the features and what is left out), the quantisation descriptors by digest, the profile
//! (quantisation policy, calibration statistics by digest, window), the converter and its math, the
//! executors that must agree, the logit convention, the result (artifact digest, inventory root, graph
//! root, tokenizer), the conformance vectors, and the Hugging Face reference the integer program is held
//! to. A pack is a directory: `pack.json` and the sidecars it names (the statistics, the reference
//! logits, a user-supplied adapter, supplied descriptors), each pinned by hash.
//!
//! **Identity**: [`pack_digest`] — BLAKE2b-256, keyed, over the canonical JSON (the adapter files'
//! form: keys sorted, compact, integral floats as integers) of `pack.json`. The file may be pretty
//! printed; the digest does not depend on it. Nothing in the manifest depends on the machine, the
//! time or the thread count: building a pack twice from the same inputs gives the same digest.
//!
//! **Numbers.** Every number that is not an integer (the quantisation headroom, the logit scale, the
//! tolerances, the measured fit) is stored as the 16 hex digits of its IEEE-754 binary64 bits: a decimal
//! text does not round-trip through every JSON reader (serde_json's default parser is off by an ulp on
//! some values), and a digest must not depend on the reader.

use misaka_palw_tir_lower::adapter::canonical_json;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `f64` as the hex of its bits (see the module docs).
pub(crate) mod f64_bits {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format!("{:016x}", v.to_bits()))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
        let t = String::deserialize(d)?;
        if t.len() != 16 || !t.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) {
            return Err(serde::de::Error::custom(format!("`{t}` is not the 16 lowercase hex digits of an f64's bits")));
        }
        u64::from_str_radix(&t, 16).map(f64::from_bits).map_err(serde::de::Error::custom)
    }
}

pub const PACK_SCHEMA_V1: &str = "misaka.palw.runtime-pack.v1";
/// The manifest's file name inside a pack directory.
pub const PACK_FILE: &str = "pack.json";
/// The domain key of the pack digest.
pub const PACK_DIGEST_KEY_V1: &[u8] = b"misaka.palw.runtime-pack.v1";
/// The version of the lowering this build performs: bumped when the same inputs would lower to other
/// bytes (the golden gate `tests/golden_lowering.rs` is what notices).
pub const LOWERING_VERSION_V1: &str = "misaka.palw.lowering.v1";

/// A file of the model's public source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceFile {
    /// Relative to the model directory (the file name for a lone GGUF).
    pub path: String,
    pub bytes: u64,
    /// SHA-256, hex: the git-lfs object id the hub publishes for a weight file.
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelSection {
    /// `safetensors` or `gguf`.
    pub format: String,
    /// What the source is called (a repository id, a directory name): informational.
    pub label: String,
    /// The hub revision, when known: informational (the file hashes are the identity).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    pub architectures: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_type: Option<String>,
    /// BLAKE2b-256 over the canonical JSON of the configuration the frontend read.
    pub config_digest: String,
    pub files: Vec<SourceFile>,
}

/// An adapter (or the standard template) pinned by id and hash.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterPin {
    /// `none` (Level A), `built-in` or `user-file`.
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// BLAKE2b-512 hex of the effective adapter (`misaka.palw.model-adapter.v1` identity).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    /// A user-supplied adapter's file inside the pack.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrontendSection {
    pub adapter: AdapterPin,
    /// Level A reads the standard decoder template; its hash is pinned here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<AdapterPin>,
    /// The hash of the whole built-in adapter pack of the build that wrote this.
    pub builtin_pack_hash: String,
    /// `misaka.palw.model-spec.v1` digest of the spec the lowering started from.
    pub spec_digest: String,
    /// `A` or `B`.
    pub level: String,
    /// Configuration keys the class defaults supplied.
    pub assumed_defaults: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeaturesSection {
    /// A digest of the feature vocabulary of the build (ids, status, protocol requirement).
    pub registry_digest: String,
    /// The features the spec uses: `{id, layers, detail}`.
    pub used: Vec<Value>,
    /// `misaka.palw.feature-scope.v1`: what the class computes and what it leaves out.
    pub scope: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DescriptorPin {
    pub name: String,
    /// The descriptor's digest (`misaka.palw.quant-format.v1` identity).
    pub digest: String,
    /// `built-in` or `file` (then `file` names it inside the pack).
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuantSection {
    pub descriptors: Vec<DescriptorPin>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyRec {
    #[serde(with = "f64_bits")]
    pub headroom16: f64,
    #[serde(with = "f64_bits")]
    pub headroom32: f64,
    #[serde(with = "f64_bits")]
    pub headroom_resid: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalibrationRec {
    pub schema: String,
    /// Digest of the statistics (`misaka.palw.calib-stats.v1`, bit-exact): the pinned input.
    pub stats_digest: String,
    pub stats_file: String,
    pub sites: usize,
    /// Where the statistics were measured from (a token file's record, or the statistics' own path).
    pub source: Value,
    /// The calibration sequences, when the pack carries them (so the statistics can be re-measured).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequences_file: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileSection {
    pub policy: PolicyRec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_window: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<usize>,
    pub calibration: CalibrationRec,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MathRec {
    /// `libm-v1` (the same bytes on every platform) or `std` (the platform's libm).
    pub mode: String,
    /// For `std`: the platform that built the artifact (`macos-aarch64`): the only one that rebuilds it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConverterSection {
    pub name: String,
    pub crate_version: String,
    /// [`super::manifest::LOWERING_VERSION_V1`].
    pub lowering: String,
    pub math: MathRec,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImplRec {
    pub name: String,
    pub crate_name: String,
    pub crate_version: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutorSection {
    pub program_format: String,
    /// `prim_set_id` (BLAKE2b-512 hex) of the primitive set the program is written in.
    pub prim_set_id: String,
    /// The implementations that must agree byte for byte on every conformance vector.
    pub implementations: Vec<ImplRec>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToleranceRec {
    /// Least-squares slope of the HF logits on the program's value (`code × scale`) must lie in
    /// `[slope_min, slope_max]`: the unit check — a program whose logits are in other units than
    /// natural-log logits has a slope far from 1.
    #[serde(with = "f64_bits")]
    pub slope_min: f64,
    #[serde(with = "f64_bits")]
    pub slope_max: f64,
    #[serde(with = "f64_bits")]
    pub corr_min: f64,
    #[serde(with = "f64_bits")]
    pub top1_min: f64,
    #[serde(with = "f64_bits")]
    pub kl_max: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogitsSection {
    /// `legacy-greedy-only`: the chain treats the logit codes as opaque beyond their order (the pack
    /// records the scale the tools read them with); `q24-natural-v1`: code / 2^24 are natural-log
    /// logits, the scale is exactly 2^-24.
    pub convention: String,
    /// The scale the codes are read with: `code × scale` is a logit.
    #[serde(rename = "scale_bits", with = "f64_bits")]
    pub scale: f64,
    pub tolerance: ToleranceRec,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultSection {
    /// The container's file digest (`misaka_palw_tir_artifact::file_digest_v1`), hex.
    pub artifact_digest: String,
    pub artifact_bytes: u64,
    pub graph_ir_root: String,
    pub inventory_root: String,
    pub leaf_count: u32,
    pub tokenizer_id: String,
    pub program_bytes: u64,
}

/// One conformance vector: a prompt, the tokens decoded greedily after it, and the digests of what the
/// program computed — the logits of every position and every commit point — which every
/// implementation must reproduce.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConformanceVector {
    pub label: String,
    pub prompt: Vec<usize>,
    pub decode: usize,
    /// The tokens decoded after the prompt (arg-max of the logits, the lowest index on a tie).
    pub tokens: Vec<usize>,
    pub positions: usize,
    pub logits_digest: String,
    pub commits_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConformanceSection {
    pub vectors: Vec<ConformanceVector>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FitRec {
    #[serde(with = "f64_bits")]
    pub slope: f64,
    #[serde(with = "f64_bits")]
    pub intercept: f64,
    #[serde(with = "f64_bits")]
    pub corr: f64,
    #[serde(with = "f64_bits")]
    pub max_abs: f64,
    #[serde(with = "f64_bits")]
    pub rmse: f64,
    #[serde(with = "f64_bits")]
    pub top1: f64,
    #[serde(with = "f64_bits")]
    pub kl_mean: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HfReferenceSection {
    pub file: String,
    pub digest: String,
    /// What produced the reference (`transformers`, `torch`, dtype, device): informational.
    pub producer: Value,
    pub sequences: usize,
    pub positions: usize,
    pub vocab: usize,
    /// What the integer program measured against it when the pack was built.
    pub measured: FitRec,
}

/// A class declared from the artifact on a network: the layout and the class id it gives.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeclaredClass {
    pub network: String,
    pub layout_digest: String,
    pub class_id: String,
    pub max_context: u32,
    pub checkpoint_interval: u32,
    pub h_tile: u32,
    /// The container written with the layout: its file digest.
    pub file_digest: String,
    /// Full effective layout; old packs remain readable but cannot reproduce it from a base artifact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exact_layout: Option<ExactLayoutRec>,
}

/// Actual class inputs, not the search options that happened to produce them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExactLayoutRec {
    pub version: u16,
    pub commit_tiles: Vec<u32>,
    pub state_tiles: Vec<u32>,
    pub logits_scheme_id: String,
}

impl DeclaredClass {
    pub fn from_class(
        network: String,
        class: &kaspa_consensus_core::palw_tir_class_v1::PalwTirClassV1,
        root: &kaspa_hashes::Hash64,
        file_digest: String,
    ) -> Result<Self, String> {
        let program = class.decode_program().map_err(|e| e.to_string())?;
        Ok(Self {
            network,
            layout_digest: class.layout_digest().to_string(),
            class_id: class.class_id(root).to_string(),
            max_context: class.layout.max_context,
            checkpoint_interval: class.layout.checkpoint_interval,
            h_tile: class.layout.h_tile,
            file_digest,
            exact_layout: Some(ExactLayoutRec {
                version: class.layout.version,
                commit_tiles: class.layout.commit_tiles.clone(),
                state_tiles: class.layout.state_tiles.clone(),
                logits_scheme_id: hex(&program.logits_scheme_id),
            }),
        })
    }

    /// Reconstruct exactly; never search under this binary's current admission rules.
    pub fn class_from_program(
        &self,
        program: &misaka_palw_tir::TirProgramV1,
        tokenizer_id: kaspa_hashes::Hash64,
    ) -> Result<kaspa_consensus_core::palw_tir_class_v1::PalwTirClassV1, String> {
        use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1, PalwTirLayoutV1};
        let r = self.exact_layout.as_ref().ok_or("legacy pack has no exact layout; bind its declared artifact into a new pack")?;
        let scheme: kaspa_hashes::Hash64 = r.logits_scheme_id.parse().map_err(|e| format!("logits scheme: {e:?}"))?;
        let mut p = program.clone();
        p.logits_scheme_id.copy_from_slice(scheme.as_byte_slice());
        let p = misaka_palw_tir::TirProgramV1::decode_canonical(&p.encode()).map_err(|e| e.to_string())?;
        Ok(PalwTirClassV1 {
            version: PALW_TIR_CLASS_VERSION_V1,
            program: p.encode(),
            tokenizer_id,
            layout: PalwTirLayoutV1 {
                version: r.version,
                max_context: self.max_context,
                checkpoint_interval: self.checkpoint_interval,
                h_tile: self.h_tile,
                commit_tiles: r.commit_tiles.clone(),
                state_tiles: r.state_tiles.clone(),
            },
        })
    }
}

/// A sidecar file of the pack directory, pinned by hash.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackFile {
    pub path: String,
    pub bytes: u64,
    /// BLAKE2b-256, hex.
    pub blake2b256: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimePackV1 {
    pub schema: String,
    pub name: String,
    pub model: ModelSection,
    pub frontend: FrontendSection,
    pub features: FeaturesSection,
    pub quant: QuantSection,
    pub profile: ProfileSection,
    pub converter: ConverterSection,
    pub executor: ExecutorSection,
    pub logits: LogitsSection,
    pub result: ResultSection,
    pub conformance: ConformanceSection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hf_reference: Option<HfReferenceSection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub declared: Vec<DeclaredClass>,
    pub files: Vec<PackFile>,
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// BLAKE2b-256 of `bytes`, hex (the pack's file hash).
pub fn blake2b256_hex(bytes: &[u8]) -> String {
    hex(blake2b_simd::Params::new().hash_length(32).hash(bytes).as_bytes())
}

impl RuntimePackV1 {
    /// The canonical JSON text the digest is over.
    pub fn canonical(&self) -> String {
        canonical_json(&serde_json::to_value(self).unwrap_or(Value::Null))
    }

    /// **The pack's identity.**
    pub fn digest(&self) -> String {
        let h = blake2b_simd::Params::new().hash_length(32).key(PACK_DIGEST_KEY_V1).hash(self.canonical().as_bytes());
        hex(h.as_bytes())
    }

    /// Parse a manifest's text: the schema must be this one, a field this version does not know is an error.
    pub fn parse(text: &str) -> Result<RuntimePackV1, String> {
        let v: Value = serde_json::from_str(text).map_err(|e| format!("the pack manifest is not JSON: {e}"))?;
        if v.get("schema").and_then(Value::as_str) != Some(PACK_SCHEMA_V1) {
            return Err(format!("the manifest's schema is not {PACK_SCHEMA_V1}"));
        }
        let p: RuntimePackV1 = serde_json::from_value(v).map_err(|e| format!("the pack manifest: {e}"))?;
        p.check_shape()?;
        Ok(p)
    }

    /// The manifest as pretty-printed text (keys in the canonical order).
    pub fn to_pretty(&self) -> String {
        // Canonical order, human spacing: sort by round-tripping through a BTreeMap-based Value.
        let canon: Value = serde_json::from_str(&self.canonical()).unwrap_or(Value::Null);
        serde_json::to_string_pretty(&canon).unwrap_or_default() + "\n"
    }

    /// The structural rules a manifest must satisfy whoever wrote it (hex lengths, file names).
    pub fn check_shape(&self) -> Result<(), String> {
        let hexn = |what: &str, s: &str, n: usize| -> Result<(), String> {
            if s.len() == n && s.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) {
                Ok(())
            } else {
                Err(format!("{what} is not {n} lowercase hex characters"))
            }
        };
        hexn("model.config_digest", &self.model.config_digest, 64)?;
        for f in &self.model.files {
            hexn(&format!("the sha256 of model.files[{}]", f.path), &f.sha256, 64)?;
            safe_name(&f.path)?;
        }
        hexn("frontend.spec_digest", &self.frontend.spec_digest, 64)?;
        hexn("frontend.builtin_pack_hash", &self.frontend.builtin_pack_hash, 128)?;
        for a in std::iter::once(&self.frontend.adapter).chain(self.frontend.template.iter()) {
            if let Some(h) = &a.hash {
                hexn("an adapter hash", h, 128)?;
            }
            if let Some(f) = &a.file {
                safe_name(f)?;
            }
        }
        for d in &self.quant.descriptors {
            hexn(&format!("quant descriptor {} digest", d.name), &d.digest, 64)?;
            if let Some(f) = &d.file {
                safe_name(f)?;
            }
        }
        hexn("profile.calibration.stats_digest", &self.profile.calibration.stats_digest, 64)?;
        safe_name(&self.profile.calibration.stats_file)?;
        hexn("executor.prim_set_id", &self.executor.prim_set_id, 128)?;
        hexn("result.artifact_digest", &self.result.artifact_digest, 128)?;
        hexn("result.graph_ir_root", &self.result.graph_ir_root, 128)?;
        hexn("result.inventory_root", &self.result.inventory_root, 128)?;
        hexn("result.tokenizer_id", &self.result.tokenizer_id, 128)?;
        if !matches!(self.logits.convention.as_str(), "legacy-greedy-only" | "q24-natural-v1") {
            return Err(format!("logits.convention `{}` (legacy-greedy-only or q24-natural-v1)", self.logits.convention));
        }
        if self.logits.convention == "q24-natural-v1" && self.logits.scale.to_bits() != 2f64.powi(-24).to_bits() {
            return Err("logits.convention q24-natural-v1 says the scale is exactly 2^-24".into());
        }
        if !matches!(self.converter.math.mode.as_str(), "libm-v1" | "std") {
            return Err(format!("converter.math.mode `{}` (libm-v1 or std)", self.converter.math.mode));
        }
        if self.converter.math.mode == "std" && self.converter.math.platform.is_none() {
            return Err("math `std` records the platform that built the artifact".into());
        }
        for v in &self.conformance.vectors {
            hexn(&format!("conformance `{}` logits_digest", v.label), &v.logits_digest, 64)?;
            hexn(&format!("conformance `{}` commits_digest", v.label), &v.commits_digest, 64)?;
        }
        for f in &self.files {
            safe_name(&f.path)?;
            hexn(&format!("files[{}].blake2b256", f.path), &f.blake2b256, 64)?;
        }
        for d in &self.declared {
            hexn("declared.layout_digest", &d.layout_digest, 128)?;
            hexn("declared.class_id", &d.class_id, 128)?;
            hexn("declared.file_digest", &d.file_digest, 128)?;
            if let Some(l) = &d.exact_layout {
                hexn("declared.exact_layout.logits_scheme_id", &l.logits_scheme_id, 128)?;
                if l.version != 1
                    || d.max_context == 0
                    || d.checkpoint_interval == 0
                    || d.h_tile == 0
                    || l.commit_tiles.contains(&0)
                    || l.state_tiles.contains(&0)
                {
                    return Err("declared exact layout has an unsupported version or a zero dimension".into());
                }
            }
        }
        Ok(())
    }
}

/// A relative file name inside the pack or the model directory: no `..`, no absolute path.
pub fn safe_name(p: &str) -> Result<(), String> {
    let path = std::path::Path::new(p);
    if p.is_empty() || path.is_absolute() || path.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
        return Err(format!("`{p}` is not a relative file name inside the directory"));
    }
    Ok(())
}
