//! **The manifest** (ADR-0108 Decision 1): one JSON document, canonical under RFC 8785, whose id is
//! a keyed hash of its canonical bytes, and whose every path is relative to the directory it sits
//! in (SA-2). Everything here is the boundary: the bounds of SA-1 are enforced before any kind is
//! asked about the document, a key that looks like key material is refused as unknown (SA-6), and
//! `source.digest` is carried and never read (SA-7).

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use kaspa_hashes::Hash64;
use serde::{Deserialize, Serialize};

/// The literal every manifest opens with.
pub const PALW_EXTENSION_MANIFEST_V1: &str = "misaka-palw/extension-manifest/v1";
/// `extension_id = BLAKE2b-512(key = this, len ‖ canonical bytes)` — the same shape
/// `kaspa_consensus_core::palw_derived_v1::transformer_id_v1` has (Decision 1).
pub const PALW_EXTENSION_ID_DOMAIN_V1: &[u8] = b"misaka-palw/extension-manifest/v1";
/// SA-1: a manifest is at most this many canonical bytes.
pub const PALW_EXTENSION_MAX_CANONICAL_BYTES: usize = 1 << 20;
/// SA-1: the raw bytes are bounded BEFORE canonicalization, since the canonicalizer is a parser
/// driven by its input. Four times the canonical bound: canonical form strips whitespace and
/// re-escapes strings, so a document this far over cannot canonicalize under the bound.
pub const PALW_EXTENSION_MAX_RAW_BYTES: usize = PALW_EXTENSION_MAX_CANONICAL_BYTES * 4;
/// SA-1: at most this many verification vectors.
pub const PALW_EXTENSION_MAX_VECTORS: usize = 4_096;
/// SA-1: at most this many fences named.
pub const PALW_EXTENSION_MAX_FENCES: usize = 64;
/// At most this many kernel ids or capabilities listed — the same order as the vector bound.
pub const PALW_EXTENSION_MAX_LIST: usize = 4_096;
/// A name is the person's; it is bounded so a manifest cannot be mostly name.
pub const PALW_EXTENSION_MAX_NAME_BYTES: usize = 256;
/// A relative path inside the manifest's directory, bounded like a name.
pub const PALW_EXTENSION_MAX_PATH_BYTES: usize = 1_024;
/// ADR-0108 §8: reserved as a name, refused as unknown until there is an object for it to name.
pub const PALW_EXTENSION_RESERVED_KINDS: &[&str] = &["service-descriptor"];
/// SA-6: a manifest carries no key material and no signature. A key whose lower-cased name
/// contains one of these is refused as unknown, at any depth, before the document is read.
pub const PALW_EXTENSION_KEY_MATERIAL_TOKENS: &[&str] =
    &["signature", "sig_", "_sig", "seed", "secret", "private", "privkey", "pubkey", "public_key", "mnemonic", "signer", "key_file"];

/// A boundary refusal: the field named, and why. **No report is produced** for one of these — a
/// bound exceeded, a duplicate key, a key that looks like key material, a path that escapes — the
/// document was never read far enough to have a tier (SA-1: "a bound exceeded is no report").
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwExtensionError {
    #[error("{field}: {reason}")]
    Field { field: String, reason: String },
    /// Something this crate could not do — a document that would not serialize, a file the
    /// operating system refused after the path passed every rule. Named so it is never mistaken
    /// for a verdict on the manifest.
    #[error("internal: {0}")]
    Internal(String),
}

impl PalwExtensionError {
    pub fn field(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Field { field: field.into(), reason: reason.into() }
    }
}

/// The kinds a person can bring (Decision 1). `service-descriptor` is deliberately absent — see
/// [`PALW_EXTENSION_RESERVED_KINDS`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PalwExtensionKindV1 {
    ModelClass,
    ContextProfile,
    FamilyCertification,
    LaneCertification,
    DerivedTransformer,
    RulesetCandidate,
}

impl PalwExtensionKindV1 {
    pub const ALL: [Self; 6] = [
        Self::ModelClass,
        Self::ContextProfile,
        Self::FamilyCertification,
        Self::LaneCertification,
        Self::DerivedTransformer,
        Self::RulesetCandidate,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ModelClass => "model-class",
            Self::ContextProfile => "context-profile",
            Self::FamilyCertification => "family-certification",
            Self::LaneCertification => "lane-certification",
            Self::DerivedTransformer => "derived-transformer",
            Self::RulesetCandidate => "ruleset-candidate",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == s)
    }

    /// **The object the chain has for this kind** (Decision 5's table) — what `admission.object`
    /// must say. `none` for the two kinds nothing rides for: a transformer (each derivation over it
    /// rides per claim as a `DerivedArtifactV1`) and a ruleset candidate (a fence is a release).
    pub fn admission_object(self) -> &'static str {
        match self {
            Self::ModelClass | Self::ContextProfile => "ClassRegistered",
            Self::FamilyCertification => "FamilyCertified",
            Self::LaneCertification => "ClassLaneCertified",
            Self::DerivedTransformer | Self::RulesetCandidate => "none",
        }
    }
}

impl std::fmt::Display for PalwExtensionKindV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `source`: the person's own record of what they started from. **Inert** (SA-7): carried, echoed
/// by `inspect`, and an input to nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PalwExtensionSourceV1 {
    /// 64 hex, or absent.
    pub digest: Option<String>,
    pub note: Option<String>,
}

/// `artifact`: the bytes the identity was made from, where the kind has any.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PalwExtensionArtifactV1 {
    /// 128 hex — the root the kind recomputes (a class's registered root, a transformer's id …).
    pub root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    /// Relative to the manifest's directory (SA-2). Absent where the machine holds no file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// A fence, as a manifest asks about it: a height, or one of the words `genesis` (active from block
/// 1), `active` (in force at the height the verifier judges at) and `dormant` (not in force).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PalwFenceRequestV1 {
    Height(u64),
    Named(String),
}

impl std::fmt::Display for PalwFenceRequestV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Height(h) => write!(f, "{h}"),
            Self::Named(s) => f.write_str(s),
        }
    }
}

/// `requires`: what the manifest was written against.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PalwExtensionRequiresV1 {
    /// 64 hex — the `consensus_params_id` the manifest was written against. A verifier on another
    /// ruleset reports the mismatch and verifies anyway (Decision 1).
    pub ruleset_id: Option<String>,
    /// Fence name → what the manifest assumes (or, for a `ruleset-candidate`, what it proposes).
    pub fences: BTreeMap<String, PalwFenceRequestV1>,
    /// 128 hex each — the kernels the object reaches, if the person wants that checked.
    pub kernel_ids: Vec<String>,
}

/// `declares`: what the person claims. `object_id` is recomputed by the kind and a mismatch is
/// refused by that field's name (Decision 1: derived, never declared).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PalwExtensionDeclaresV1 {
    /// 128 hex.
    pub object_id: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub resource_bounds: BTreeMap<String, u64>,
}

/// One `derived-transformer` vector: a DSL file and what running the transformer over it must give.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PalwTransformerVectorV1 {
    /// Relative to the manifest's directory (SA-2).
    pub dsl_path: String,
    /// 128 hex.
    pub expected_dsl_hash: String,
    /// 128 hex.
    pub expected_artifact_hash: String,
    pub expected_artifact_bytes: u64,
}

/// `verification`: kind-specific inputs. Every field is optional at the type level; each kind says
/// which it needs and refuses by name when one is missing or contradicts another.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PalwExtensionVerificationV1 {
    /// `model-class` / `context-profile`: a class this build's SDK ledger names.
    pub model_id: Option<String>,
    /// `model-class` / `context-profile`: the profile itself, as the borsh of `PalwShapeProfileV3`
    /// in hex — the form for a class no ledger of this build carries.
    pub profile_borsh_hex: Option<String>,
    /// The same, as a borsh file beside the manifest.
    pub profile_path: Option<String>,
    /// `[prefill, decode]` — the job the class is paid per; required beside an inline profile.
    pub canonical_job: Option<(u32, u32)>,
    /// `context-profile`: the width. With `project`, the width to project the family's row at;
    /// with `model_id`, the ladder row at that width; with an inline profile, what its `n_ctx`
    /// must be.
    pub n_ctx: Option<u32>,
    /// `context-profile`: `a16-context-row` projects the dense A16 family's row at `n_ctx` with the
    /// shipped ladder function (`palw_a16_context_row_profile_v1`).
    pub project: Option<String>,
    /// `family-certification` / `lane-certification`: the borsh `PalwConsensusObjectV2` file
    /// `palw-certify drill|bind --out` wrote, relative to the manifest's directory.
    pub object_path: Option<String>,
    /// `attempt` | `fp` — checked against the object's own lane when given.
    pub lane: Option<String>,
    /// `derived-transformer`: the vectors.
    pub vectors: Vec<PalwTransformerVectorV1>,
    /// Cross-checks by name (`artifact_root`, `family_id`, `class_id`, `transformer_id`), 128 hex
    /// each: each is compared with the value the kind recomputes and a mismatch is refused by its
    /// own field name.
    pub expected: BTreeMap<String, String>,
}

/// `transformer` (`derived-transformer` only): the name alone — the manifest this build publishes
/// under that name — or every field of the preimage, for a transformer this build may not ship.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PalwExtensionTransformerV1 {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grammar: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discipline: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writer: Option<String>,
    /// 64 hex.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_tree_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_dsl_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_artifact_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_steps: Option<u64>,
}

impl PalwExtensionTransformerV1 {
    /// Whether every preimage field is given (the full form), none (the name form), or some
    /// (refused: a half manifest hashes to nothing anyone published).
    pub fn form(&self) -> Result<bool, PalwExtensionError> {
        let given = [
            self.grammar.is_some(),
            self.discipline.is_some(),
            self.writer.is_some(),
            self.source_tree_sha256.is_some(),
            self.kind.is_some(),
            self.max_dsl_bytes.is_some(),
            self.max_artifact_bytes.is_some(),
            self.max_steps.is_some(),
        ];
        let n = given.iter().filter(|g| **g).count();
        match n {
            0 => Ok(false),
            8 => Ok(true),
            _ => Err(PalwExtensionError::field(
                "transformer",
                format!(
                    "{n} of the 8 preimage fields are given — name a transformer this build publishes (`name` alone) or give every \
                     field (grammar, discipline, writer, source_tree_sha256, kind, max_dsl_bytes, max_artifact_bytes, max_steps)"
                ),
            )),
        }
    }
}

/// `admission`: which existing object the kind rides as (Decision 5), or `none`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PalwExtensionAdmissionV1 {
    pub object: String,
}

/// The manifest (Decision 1). Parsed only through [`PalwParsedManifestV1::parse`], which is where
/// the bounds live; the struct itself is serde and nothing more.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PalwExtensionManifestV1 {
    pub manifest: String,
    pub kind: PalwExtensionKindV1,
    /// A name the person chose — never an identity.
    pub name: String,
    /// e.g. `testnet-11`.
    pub network: String,
    #[serde(default)]
    pub source: PalwExtensionSourceV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<PalwExtensionArtifactV1>,
    #[serde(default)]
    pub requires: PalwExtensionRequiresV1,
    pub declares: PalwExtensionDeclaresV1,
    #[serde(default)]
    pub verification: PalwExtensionVerificationV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transformer: Option<PalwExtensionTransformerV1>,
    pub admission: PalwExtensionAdmissionV1,
}

/// A manifest that passed the boundary: the document, its canonical bytes, and its id. The id is
/// a function of the bytes the person wrote (canonicalized), never of a re-serialization of the
/// struct — so a field this crate defaults is not in the preimage unless the person wrote it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwParsedManifestV1 {
    pub manifest: PalwExtensionManifestV1,
    pub canonical_bytes: Vec<u8>,
    pub extension_id: Hash64,
}

/// `extension_id_v1(canonical bytes)`: keyed BLAKE2b-512 under [`PALW_EXTENSION_ID_DOMAIN_V1`]
/// over `len_le64 ‖ bytes` — the shape `palw_derived_v1::canonical_id` gives `transformer_id_v1`,
/// restated here because that helper is private to consensus-core and this id is not consensus.
pub fn extension_id_v1(canonical_bytes: &[u8]) -> Hash64 {
    keyed_id_v1(PALW_EXTENSION_ID_DOMAIN_V1, canonical_bytes)
}

/// The one keyed-digest shape this crate uses (the manifest id and the receipt id share it).
pub(crate) fn keyed_id_v1(domain: &[u8], bytes: &[u8]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(domain).to_state();
    state.update(&(bytes.len() as u64).to_le_bytes());
    state.update(bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// RFC 8785 canonical bytes of a JSON document — the ONE spelling, `misaka_palw_derive`'s.
pub fn canonical_json(input: &[u8]) -> Result<Vec<u8>, PalwExtensionError> {
    misaka_palw_derive::canon_json::canonicalize_json(input).map_err(|e| PalwExtensionError::field("manifest", e.to_string()))
}

impl PalwParsedManifestV1 {
    /// **The boundary.** In order: the raw bound, the canonical form (duplicate keys and floats are
    /// refused by the canonicalizer), the canonical bound, the key-material scan (SA-6), the
    /// reserved kind, the shape, and every field bound of SA-1 — all before any kind is asked.
    pub fn parse(input: &[u8]) -> Result<Self, PalwExtensionError> {
        if input.len() > PALW_EXTENSION_MAX_RAW_BYTES {
            return Err(PalwExtensionError::field(
                "manifest",
                format!("{} raw bytes, over the {PALW_EXTENSION_MAX_RAW_BYTES}-byte bound (SA-1)", input.len()),
            ));
        }
        let canonical_bytes = canonical_json(input)?;
        if canonical_bytes.len() > PALW_EXTENSION_MAX_CANONICAL_BYTES {
            return Err(PalwExtensionError::field(
                "manifest",
                format!("{} canonical bytes, over the {PALW_EXTENSION_MAX_CANONICAL_BYTES}-byte bound (SA-1)", canonical_bytes.len()),
            ));
        }
        let value: serde_json::Value = serde_json::from_slice(&canonical_bytes)
            .map_err(|e| PalwExtensionError::field("manifest", format!("not a JSON document: {e}")))?;
        let object = value.as_object().ok_or_else(|| PalwExtensionError::field("manifest", "the document is not a JSON object"))?;
        refuse_key_material(&value, "")?;
        match object.get("manifest").and_then(|v| v.as_str()) {
            Some(PALW_EXTENSION_MANIFEST_V1) => {}
            Some(other) => {
                return Err(PalwExtensionError::field("manifest", format!("`{other}` is not `{PALW_EXTENSION_MANIFEST_V1}`")));
            }
            None => {
                return Err(PalwExtensionError::field(
                    "manifest",
                    format!("missing; a manifest opens with `{PALW_EXTENSION_MANIFEST_V1}`"),
                ));
            }
        }
        match object.get("kind").and_then(|v| v.as_str()) {
            Some(kind) if PALW_EXTENSION_RESERVED_KINDS.contains(&kind) => {
                return Err(PalwExtensionError::field(
                    "kind",
                    format!(
                        "`{kind}` is reserved as a name and refused as unknown until there is an object for it to name (ADR-0108 §8)"
                    ),
                ));
            }
            Some(kind) if PalwExtensionKindV1::parse(kind).is_none() => {
                return Err(PalwExtensionError::field(
                    "kind",
                    format!(
                        "`{kind}` is not a kind this build knows ({})",
                        PalwExtensionKindV1::ALL.iter().map(|k| k.as_str()).collect::<Vec<_>>().join(", ")
                    ),
                ));
            }
            Some(_) => {}
            None => return Err(PalwExtensionError::field("kind", "missing")),
        }
        let manifest: PalwExtensionManifestV1 =
            serde_json::from_slice(&canonical_bytes).map_err(|e| PalwExtensionError::field("manifest", e.to_string()))?;
        manifest.check_bounds()?;
        let extension_id = extension_id_v1(&canonical_bytes);
        Ok(Self { manifest, canonical_bytes, extension_id })
    }

    /// The canonical bytes as a string, for printing.
    pub fn canonical_str(&self) -> &str {
        // Canonical JSON is ASCII-escaped where it needs to be and UTF-8 elsewhere; the
        // canonicalizer only ever emits valid UTF-8.
        std::str::from_utf8(&self.canonical_bytes).unwrap_or("<non-utf8 canonical bytes>")
    }
}

/// SA-6, at every depth: a key that looks like key material or a signature is refused as unknown
/// before the shape is read, naming the key's path.
fn refuse_key_material(value: &serde_json::Value, path: &str) -> Result<(), PalwExtensionError> {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                let lower = key.to_ascii_lowercase();
                let child_path = if path.is_empty() { key.clone() } else { format!("{path}.{key}") };
                if PALW_EXTENSION_KEY_MATERIAL_TOKENS.iter().any(|token| lower.contains(token)) {
                    return Err(PalwExtensionError::field(
                        child_path,
                        "a manifest carries no key material and no signature; a field that looks like one is refused as unknown (SA-6) — \
                         `submit` signs with the operator's own key",
                    ));
                }
                refuse_key_material(child, &child_path)?;
            }
            Ok(())
        }
        serde_json::Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                refuse_key_material(item, &format!("{path}[{i}]"))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// A hex field of exactly `chars` characters (SA-1: "every hex field exactly 64 or 128 characters").
pub fn check_hex(field: &str, value: &str, chars: usize) -> Result<(), PalwExtensionError> {
    if value.len() != chars || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(PalwExtensionError::field(field, format!("must be exactly {chars} hex characters, got {} (SA-1)", value.len())));
    }
    Ok(())
}

/// A 128-hex field as a `Hash64`.
pub fn parse_hash64(field: &str, value: &str) -> Result<Hash64, PalwExtensionError> {
    check_hex(field, value, 128)?;
    value.parse::<Hash64>().map_err(|e| PalwExtensionError::field(field, format!("{e}")))
}

/// SA-2's static half: relative, no `..`, no root, no prefix, bounded, non-empty.
pub fn check_relative_path(field: &str, value: &str) -> Result<(), PalwExtensionError> {
    if value.is_empty() {
        return Err(PalwExtensionError::field(field, "an empty path names nothing"));
    }
    if value.len() > PALW_EXTENSION_MAX_PATH_BYTES {
        return Err(PalwExtensionError::field(
            field,
            format!("{} bytes, over the {PALW_EXTENSION_MAX_PATH_BYTES}-byte path bound", value.len()),
        ));
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return Err(PalwExtensionError::field(field, "must be relative to the manifest's directory, not absolute (SA-2)"));
    }
    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir => {
                return Err(PalwExtensionError::field(field, "`..` is refused: a path never leaves the manifest's directory (SA-2)"));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(PalwExtensionError::field(field, "must be relative to the manifest's directory (SA-2)"));
            }
        }
    }
    Ok(())
}

/// SA-2's dynamic half: the path resolved against the manifest's directory, and refused if what it
/// resolves to — symlinks followed — is outside that directory. `Ok(None)` is "not there / not
/// readable", which is a depth stop and not a refusal (Decision 3); `Err` is a path that escapes.
pub fn resolve_manifest_path(field: &str, manifest_dir: &Path, value: &str) -> Result<Option<PathBuf>, PalwExtensionError> {
    check_relative_path(field, value)?;
    let Ok(base) = manifest_dir.canonicalize() else {
        return Ok(None);
    };
    let joined = base.join(value);
    let Ok(resolved) = joined.canonicalize() else {
        return Ok(None);
    };
    if !resolved.starts_with(&base) {
        return Err(PalwExtensionError::field(
            field,
            format!("resolves to {}, outside the manifest's directory {} (SA-2)", resolved.display(), base.display()),
        ));
    }
    Ok(Some(resolved))
}

impl PalwExtensionManifestV1 {
    /// SA-1's field bounds, every one naming its field.
    fn check_bounds(&self) -> Result<(), PalwExtensionError> {
        if self.name.is_empty() || self.name.len() > PALW_EXTENSION_MAX_NAME_BYTES {
            return Err(PalwExtensionError::field(
                "name",
                format!("must be 1..={PALW_EXTENSION_MAX_NAME_BYTES} bytes, got {}", self.name.len()),
            ));
        }
        if self.network.is_empty() || self.network.len() > PALW_EXTENSION_MAX_NAME_BYTES {
            return Err(PalwExtensionError::field("network", "must be a network id such as `testnet-11`"));
        }
        if let Some(digest) = &self.source.digest {
            check_hex("source.digest", digest, 64)?;
        }
        if let Some(artifact) = &self.artifact {
            check_hex("artifact.root", &artifact.root, 128)?;
            if let Some(path) = &artifact.path {
                check_relative_path("artifact.path", path)?;
            }
        }
        if let Some(id) = &self.requires.ruleset_id {
            check_hex("requires.ruleset_id", id, 64)?;
        }
        if self.requires.fences.len() > PALW_EXTENSION_MAX_FENCES {
            return Err(PalwExtensionError::field(
                "requires.fences",
                format!("{} fences, over the {PALW_EXTENSION_MAX_FENCES} bound (SA-1)", self.requires.fences.len()),
            ));
        }
        for (name, request) in &self.requires.fences {
            if name.is_empty() || name.len() > PALW_EXTENSION_MAX_NAME_BYTES {
                return Err(PalwExtensionError::field("requires.fences", "a fence name must be 1..=256 bytes"));
            }
            if let PalwFenceRequestV1::Named(word) = request
                && !matches!(word.as_str(), "genesis" | "active" | "dormant")
            {
                return Err(PalwExtensionError::field(
                    format!("requires.fences.{name}"),
                    format!("`{word}` is not a height, `genesis`, `active` or `dormant`"),
                ));
            }
        }
        if self.requires.kernel_ids.len() > PALW_EXTENSION_MAX_LIST {
            return Err(PalwExtensionError::field("requires.kernel_ids", format!("over the {PALW_EXTENSION_MAX_LIST} bound (SA-1)")));
        }
        for (i, id) in self.requires.kernel_ids.iter().enumerate() {
            check_hex(&format!("requires.kernel_ids[{i}]"), id, 128)?;
        }
        check_hex("declares.object_id", &self.declares.object_id, 128)?;
        if self.declares.capabilities.len() > PALW_EXTENSION_MAX_LIST {
            return Err(PalwExtensionError::field(
                "declares.capabilities",
                format!("over the {PALW_EXTENSION_MAX_LIST} bound (SA-1)"),
            ));
        }
        if self.declares.resource_bounds.len() > PALW_EXTENSION_MAX_LIST {
            return Err(PalwExtensionError::field(
                "declares.resource_bounds",
                format!("over the {PALW_EXTENSION_MAX_LIST} bound (SA-1)"),
            ));
        }
        let v = &self.verification;
        if v.vectors.len() > PALW_EXTENSION_MAX_VECTORS {
            return Err(PalwExtensionError::field(
                "verification.vectors",
                format!("{} vectors, over the {PALW_EXTENSION_MAX_VECTORS} bound (SA-1)", v.vectors.len()),
            ));
        }
        for (i, vector) in v.vectors.iter().enumerate() {
            check_relative_path(&format!("verification.vectors[{i}].dsl_path"), &vector.dsl_path)?;
            check_hex(&format!("verification.vectors[{i}].expected_dsl_hash"), &vector.expected_dsl_hash, 128)?;
            check_hex(&format!("verification.vectors[{i}].expected_artifact_hash"), &vector.expected_artifact_hash, 128)?;
        }
        if let Some(path) = &v.profile_path {
            check_relative_path("verification.profile_path", path)?;
        }
        if let Some(path) = &v.object_path {
            check_relative_path("verification.object_path", path)?;
        }
        if let Some(hex) = &v.profile_borsh_hex
            && (hex.is_empty() || hex.len() % 2 != 0 || !hex.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(PalwExtensionError::field("verification.profile_borsh_hex", "must be an even number of hex characters"));
        }
        for (key, value) in &v.expected {
            check_hex(&format!("verification.expected.{key}"), value, 128)?;
        }
        if let Some(t) = &self.transformer {
            if t.name.is_empty() || t.name.len() > PALW_EXTENSION_MAX_NAME_BYTES {
                return Err(PalwExtensionError::field("transformer.name", "must be 1..=256 bytes"));
            }
            if let Some(tree) = &t.source_tree_sha256 {
                check_hex("transformer.source_tree_sha256", tree, 64)?;
            }
            t.form()?;
        }
        if self.admission.object.is_empty() || self.admission.object.len() > PALW_EXTENSION_MAX_NAME_BYTES {
            return Err(PalwExtensionError::field("admission.object", "must name an object, or `none`"));
        }
        Ok(())
    }
}
