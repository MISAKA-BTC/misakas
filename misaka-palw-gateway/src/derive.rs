//! **ADR-0078 Decision 6: the derivation step, and one-response delivery.**
//!
//! After the worker's result frame, and only when the request asked for a kind, the answer is
//! canonicalized under the kind's grammar and run through its transformer (`misaka-palw-derive`),
//! and the response carries, in one body: the answer (the canonical DSL as text), the artifact
//! (inline base64 under a size the gateway states, else a fetch handle), and the
//! `DerivedArtifactV1` the executor submits or would submit — signed here when the gateway holds
//! the bond key's seed (the rail's local-seed form), unsigned otherwise, for the rail to sign.
//!
//! What a parse failure does: nothing to the claim (X4). The inference still certifies and still
//! mines; the response says the derivation was refused and why. What the chain gets: the object
//! and only the object (Decision 1) — the DSL and the artifact go to the outbox and to the user.

use std::path::{Path, PathBuf};

use kaspa_consensus_core::palw_derived_v1::{PALW_DERIVED_V1_MLDSA87_CONTEXT, PalwDerivedArtifactV1, kind, palw_derived_message_v1};
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use kaspa_hashes::Hash64;
use kaspa_pq_validator_core::{VALIDATOR_SEED_LEN, ValidatorKey};
use misaka_palw_derive::{ClaimBinding, derive_named, registry};

/// The gateway's derivation settings.
pub struct DeriveConfig {
    /// The bond key's raw seed (the rail's local-seed form). `None` leaves the object unsigned in
    /// the outbox for `misaka-palw-fp-rail --derive-artifact` or the signer sidecar.
    pub seed: Option<[u8; VALIDATOR_SEED_LEN]>,
    /// ADR-0078 Decision 6: whether this claim's DSL is put under the data-availability
    /// obligation. Off by default; when on, the outbox gains the `FPD1` payload that
    /// `misaka palw fp-submit --dsl-payload` stages into the node's retention.
    pub serve_dsl: bool,
}

pub struct Derived {
    pub transformer: &'static str,
    pub grammar: &'static str,
    pub kind: u16,
    pub derived_id: Hash64,
    pub object: PalwDerivedArtifactV1,
    pub signature: Option<Vec<u8>>,
    pub canonical_dsl: Vec<u8>,
    pub artifact: Vec<u8>,
    pub media_type: &'static str,
    pub extension: &'static str,
    pub dsl_path: PathBuf,
    pub artifact_path: PathBuf,
    pub object_path: PathBuf,
}

pub enum Outcome {
    Derived(Box<Derived>),
    /// ADR-0078 X4: the answer did not parse under the grammar — no object, the claim untouched.
    Refused {
        transformer: &'static str,
        reason: String,
    },
}

/// A transformer name (`scene/glb/v1`) or a kind name (`scene`, which picks the kind's first
/// registered transformer).
pub fn resolve_transformer(spec: &str) -> Result<&'static str, String> {
    if let Some(t) = registry::transformer_by_name(spec) {
        return Ok(t.manifest().name);
    }
    if let Some(id) = kind::id(spec)
        && let Some((name, _, _)) = registry::transformer_names().into_iter().find(|(_, k, _)| *k == id)
    {
        return Ok(name);
    }
    let available: Vec<String> =
        registry::transformer_names().iter().map(|(n, k, _)| format!("{n} ({})", kind::name(*k).unwrap_or("?"))).collect();
    Err(format!("no transformer or kind named {spec:?}; this build has: {}", available.join(", ")))
}

/// Read the seed file the gateway signs derivations with (the rail's own format: 32 raw bytes).
pub fn read_seed(path: &Path) -> Result<[u8; VALIDATOR_SEED_LEN], String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read the derive seed {}: {e}", path.display()))?;
    if bytes.len() != VALIDATOR_SEED_LEN {
        return Err(format!("the derive seed is {} bytes, not {VALIDATOR_SEED_LEN}", bytes.len()));
    }
    let mut seed = [0u8; VALIDATOR_SEED_LEN];
    seed.copy_from_slice(&bytes);
    Ok(seed)
}

/// Derive, write the outbox files, and (with a seed) sign. `Err` is an operational failure (a
/// disk, a key that is not the executor's); a grammar refusal is `Ok(Outcome::Refused)`.
pub fn run(
    spec: &str,
    cfg: &DeriveConfig,
    binding: &ClaimBinding,
    answer: &[u8],
    outbox: &Path,
    stem: &str,
) -> Result<Outcome, String> {
    let transformer = resolve_transformer(spec)?;
    let derivation = match derive_named(transformer, binding, answer) {
        Ok(d) => d,
        // **A refusal is the derive core's to define, not this caller's** (ADR-0078 X4). The three
        // arms this used to spell out have since become five — `Bound` and `UnpublishedManifest`
        // joined them — and a caller that enumerates them turns each new one into a 500 for the
        // person who asked, instead of "your answer did not parse under this grammar; the claim is
        // untouched". `Display` already prefixes each arm with its own name, so the message loses
        // nothing by not being re-prefixed here.
        Err(e) if e.is_refusal() => return Ok(Outcome::Refused { transformer, reason: e.to_string() }),
        Err(e) => return Err(format!("derivation failed: {e}")),
    };
    let manifest = registry::transformer_by_name(transformer).expect("resolved above").manifest();
    let derived_id = derivation.derived_id();
    let signature = match cfg.seed {
        Some(seed) => {
            let key = ValidatorKey::from_seed(seed);
            if key.public_key() != binding.executor_pubkey.as_slice() {
                return Err(
                    "the derive seed is not the executor key in the identity file — this key cannot sign this derivation".into()
                );
            }
            let message = palw_derived_message_v1(&derivation.object);
            Some(key.sign_with_context(message.as_byte_slice(), PALW_DERIVED_V1_MLDSA87_CONTEXT).to_vec())
        }
        None => None,
    };

    // The outbox: the DSL and the artifact for the user, the object for the chain. The artifact
    // is also filed under its derived id so `GET /v1/artifacts/<id>` can serve it.
    let dsl_path = outbox.join(format!("{stem}.dsl"));
    std::fs::write(&dsl_path, &derivation.canonical_dsl).map_err(|e| format!("cannot write {}: {e}", dsl_path.display()))?;
    let artifact_path = outbox.join(format!("{stem}.artifact.{}", derivation.artifact.extension));
    std::fs::write(&artifact_path, &derivation.artifact.bytes)
        .map_err(|e| format!("cannot write {}: {e}", artifact_path.display()))?;
    let by_id_dir = outbox.join("artifacts");
    std::fs::create_dir_all(&by_id_dir).map_err(|e| format!("cannot create {}: {e}", by_id_dir.display()))?;
    let by_id = by_id_dir.join(format!("{}.{}", hex(derived_id), derivation.artifact.extension));
    std::fs::write(&by_id, &derivation.artifact.bytes).map_err(|e| format!("cannot write {}: {e}", by_id.display()))?;
    let unsigned_path = outbox.join(format!("{stem}.derived-unsigned.borsh"));
    std::fs::write(&unsigned_path, borsh::to_vec(&derivation.object).map_err(|e| e.to_string())?)
        .map_err(|e| format!("cannot write {}: {e}", unsigned_path.display()))?;
    let object_path = match &signature {
        Some(sig) => {
            let object =
                PalwConsensusObjectV2::DerivedArtifactV1 { object: Box::new(derivation.object.clone()), signature: sig.clone() };
            let path = outbox.join(format!("{stem}.derived-object.borsh"));
            std::fs::write(&path, borsh::to_vec(&object).map_err(|e| e.to_string())?)
                .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
            path
        }
        None => unsigned_path.clone(),
    };
    // Decision 6's election: the DSL payload is written only when asked for, so that a claim
    // whose executor did not elect it has nothing on disk to serve.
    let dsl_payload_path = if cfg.serve_dsl {
        let payload = kaspa_consensus_core::palw_derived_v1::palw_fp_dsl_encode_v1(
            binding.claim_id,
            derived_id,
            derivation.object.grammar_id,
            &derivation.canonical_dsl,
        );
        let path = outbox.join(format!("{stem}.dsl-payload.fpd1"));
        std::fs::write(&path, payload).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        Some(path)
    } else {
        None
    };
    let summary = serde_json::json!({
        "schema": "misaka.palw.derived-artifact.v1",
        "derived_id": hex(derived_id),
        "claim_id": hex(binding.claim_id),
        "transformer": transformer,
        "grammar": manifest.grammar,
        "kind": manifest.kind,
        "kind_name": kind::name(manifest.kind),
        "dsl_hash": hex(derivation.dsl_hash),
        "artifact_hash": hex(derivation.artifact_hash),
        "artifact_bytes": derivation.artifact.bytes.len(),
        "signed": signature.is_some(),
        "dsl_da_elected": cfg.serve_dsl,
        "files": {
            "dsl": dsl_path.display().to_string(),
            "artifact": artifact_path.display().to_string(),
            "object": object_path.display().to_string(),
            "dsl_payload": dsl_payload_path.as_ref().map(|p| p.display().to_string()),
        },
    });
    let summary_path = outbox.join(format!("{stem}.derived.json"));
    std::fs::write(&summary_path, serde_json::to_vec_pretty(&summary).unwrap())
        .map_err(|e| format!("cannot write {}: {e}", summary_path.display()))?;

    Ok(Outcome::Derived(Box::new(Derived {
        transformer,
        grammar: manifest.grammar,
        kind: manifest.kind,
        derived_id,
        object: derivation.object,
        signature,
        canonical_dsl: derivation.canonical_dsl,
        artifact: derivation.artifact.bytes,
        media_type: derivation.artifact.media_type,
        extension: derivation.artifact.extension,
        dsl_path,
        artifact_path,
        object_path,
    })))
}

impl Outcome {
    /// The response block (Decision 6): the DSL as text, the artifact inline or by handle, and
    /// the object as the chain will see it.
    pub fn to_json(&self, inline_max: usize) -> serde_json::Value {
        match self {
            Outcome::Refused { transformer, reason } => serde_json::json!({
                "status": "refused",
                "transformer": transformer,
                "reason": reason,
                "note": "the answer did not derive under this grammar; the inference still certifies and mines (ADR-0078 X4)",
            }),
            Outcome::Derived(d) => {
                let object_bytes = borsh::to_vec(&d.object).expect("borsh");
                let artifact = if d.artifact.len() <= inline_max {
                    serde_json::json!({ "inline_base64": base64(&d.artifact) })
                } else {
                    serde_json::json!({ "url": format!("/v1/artifacts/{}", hex(d.derived_id)) })
                };
                serde_json::json!({
                    "status": "derived",
                    "transformer": d.transformer,
                    "grammar": d.grammar,
                    "kind": d.kind,
                    "kind_name": kind::name(d.kind),
                    "derived_id": hex(d.derived_id),
                    "grammar_id": hex(d.object.grammar_id),
                    "transformer_id": hex(d.object.transformer_id),
                    "dsl_hash": hex(d.object.dsl_hash),
                    "artifact_hash": hex(d.object.artifact_hash),
                    "artifact_bytes": d.object.artifact_bytes,
                    "media_type": d.media_type,
                    "extension": d.extension,
                    "dsl": String::from_utf8_lossy(&d.canonical_dsl),
                    "artifact": artifact,
                    "object_borsh_hex": faster_hex::hex_string(&object_bytes),
                    "signature_hex": d.signature.as_ref().map(|s| faster_hex::hex_string(s)),
                    "signed": d.signature.is_some(),
                    "files": { "dsl": d.dsl_path.display().to_string(), "artifact": d.artifact_path.display().to_string(), "object": d.object_path.display().to_string() },
                    "verify": "recompute dsl_hash = H(grammar_id ‖ canonical dsl) and artifact_hash = H(transformer(dsl)) with `palw-derive verify`; output_root = output_commitment_v2(job_context_hash, output_token_ids, family rendered hash)",
                })
            }
        }
    }
}

/// The worker's retention manifest beside the trace: the job context hash and family name that
/// let a consumer recompute `output_root` (ADR-0078 X6).
pub fn read_worker_manifest(job_dir: &Path) -> (Option<String>, Option<String>) {
    let Ok(bytes) = std::fs::read(job_dir.join("manifest.json")) else { return (None, None) };
    let Ok(doc) = serde_json::from_slice::<serde_json::Value>(&bytes) else { return (None, None) };
    let get = |k: &str| doc.get(k).and_then(|v| v.as_str()).map(|s| s.to_string());
    (get("job_context_hash"), get("family"))
}

/// Serve `GET /v1/artifacts/<derived-id-hex>`: the bytes filed under the id, or nothing.
/// **Every extension the shipped kinds file an artifact under, with the type it is served as.**
///
/// A closed list rather than "whatever the directory holds" — see [`artifact_by_id`]. The test
/// `the_served_extension_table_covers_every_shipped_kind` holds it against the derive crate's own
/// `EXTENSION` constants, so a kind that arrives with an eighth extension fails there rather than
/// becoming a silent 404 in production.
pub const SERVED_ARTIFACT_TYPES: &[(&str, &str)] = &[
    ("glb", "model/gltf-binary"),
    ("mid", "audio/midi"),
    ("png", "image/png"),
    ("stl", "model/stl"),
    ("mmap", "application/octet-stream"),
    ("mcod", "application/octet-stream"),
    ("msim", "application/octet-stream"),
];

/// Resolve one artifact by its derived id (ADR-0078 Decision 6's fetch handle).
///
/// **ADR-0078 SA-4: a direct probe, never a directory walk.** This used to `read_dir` the whole
/// artifact directory on every request and compare stems. That made a MISS — which is what a
/// stranger sends, since a hit needs a `derived_id` — the most expensive answer the route has, and
/// it grew with every artifact the gateway had ever produced: an unauthenticated read route whose
/// cost is linear in the operator's own history is the amplifier SA-4 names, and it does not need
/// the DSL election to be turned on to exist. A miss now costs at most
/// `SERVED_ARTIFACT_TYPES.len()` failed `open`s and reads nothing.
///
/// The 128-hex check stays FIRST and is what keeps this out of path-traversal territory: the
/// only strings that reach the join are 128 ASCII hex digits, so no `..`, no separator, and no
/// absolute path can be spelled.
pub fn artifact_by_id(outbox: &Path, id_hex: &str) -> Option<(Vec<u8>, &'static str)> {
    if id_hex.len() != 128 || !id_hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let dir = outbox.join("artifacts");
    for (ext, content_type) in SERVED_ARTIFACT_TYPES {
        if let Ok(bytes) = std::fs::read(dir.join(format!("{id_hex}.{ext}"))) {
            return Some((bytes, content_type));
        }
    }
    None
}

pub fn hex(h: Hash64) -> String {
    faster_hex::hex_string(h.as_byte_slice())
}

/// Standard base64 (RFC 4648, with padding) — small enough to hold here rather than add a crate
/// to the gateway for one field.
pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc4648_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn a_kind_name_resolves_to_a_transformer_and_nonsense_names_the_build() {
        for (name, k, _) in registry::transformer_names() {
            assert_eq!(resolve_transformer(name).unwrap(), name);
            let by_kind = resolve_transformer(kind::name(k).unwrap()).unwrap();
            assert_eq!(registry::transformer_by_name(by_kind).unwrap().manifest().kind, k);
        }
        assert!(resolve_transformer("no-such-kind").unwrap_err().contains("this build has"));
    }

    /// **ADR-0078 SA-4: the closed extension table is closed over the kinds that ship.**
    ///
    /// [`artifact_by_id`] no longer walks the artifact directory, so an extension missing from
    /// [`SERVED_ARTIFACT_TYPES`] is a kind whose artifact silently 404s. The derive crate states
    /// each one as a public constant of its kind module, and this reads them back — `image` is the
    /// one kind that spells its extension inline at the `Artifact` literal instead of as a
    /// constant, so `png` is named here directly and that is why.
    #[test]
    fn the_served_extension_table_covers_every_shipped_kind() {
        use misaka_palw_derive::kinds;
        let served: std::collections::BTreeSet<&str> = SERVED_ARTIFACT_TYPES.iter().map(|(e, _)| *e).collect();
        for ext in [
            kinds::scene::EXTENSION,
            kinds::music::EXTENSION,
            kinds::cad::EXTENSION,
            kinds::map::EXTENSION,
            kinds::code::EXTENSION,
            kinds::simulation::EXTENSION,
            "png",
        ] {
            assert!(served.contains(ext), "a shipped kind files artifacts under {ext:?} and the fetch handle would 404 them");
        }
        assert_eq!(served.len(), SERVED_ARTIFACT_TYPES.len(), "an extension is listed twice");
        // One transformer per kind row, and seven rows: if the crate grows an eighth kind this
        // count moves and the list above has to be revisited rather than silently under-covering.
        assert_eq!(registry::transformer_names().len(), 8, "the kind table changed; re-check the served extension list");
    }

    /// **ADR-0078 SA-4, and the reason the probe is safe.** Only 128 ASCII hex digits reach the
    /// path join, so nothing a stranger can spell escapes the artifact directory — and a miss
    /// reads no directory at all.
    #[test]
    fn the_fetch_handle_resolves_only_a_well_formed_id_and_never_escapes_the_outbox() {
        let dir = std::env::temp_dir().join(format!("palw-sa4-{}", std::process::id()));
        let artifacts = dir.join("artifacts");
        std::fs::create_dir_all(&artifacts).unwrap();
        let id = "ab".repeat(64);
        std::fs::write(artifacts.join(format!("{id}.glb")), b"GLB-BYTES").unwrap();

        assert_eq!(artifact_by_id(&dir, &id), Some((b"GLB-BYTES".to_vec(), "model/gltf-binary")));
        // A stem that is not a derived id is refused before any filesystem call.
        for bad in ["", "../../etc/passwd", "zz", &"ab".repeat(63), &format!("{id}a"), &format!("{}g", "ab".repeat(63) + "a")] {
            assert_eq!(artifact_by_id(&dir, bad), None, "{bad:?} must not resolve");
        }
        // A well-formed id this gateway never built is a miss, not a walk.
        assert_eq!(artifact_by_id(&dir, &"cd".repeat(64)), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
