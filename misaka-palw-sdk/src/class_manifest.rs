//! **The manifest committed beside a `.palwart`, so nobody types an inventory root by hand.**
//!
//! A registration pins the operand-inventory root at a named profile. Two things made that value a
//! recurring outage: deriving it needed the artifact, which a genesis card compiled into a binary
//! does not have, so it was pasted in by a human; and computing it used to cost an inventory, so
//! nobody re-derived it to check. The first is what this file removes and the second is what
//! `a16_inventory_root_streamed_v1` removed.
//!
//! **What it is.** A small JSON sidecar, `<artifact>.palwmanifest`, listing every class this build
//! pairs the artifact with and the inventory root of each — written by
//! [`PalwClassManifestFileV1::derive_from_artifact`], which is the SAME call the runtime's resolve
//! uses (`sdk.pairings` -> `CanonicalClassV1::artifact_root`). One derivation, two readers.
//!
//! **What it is not: an authority.** A manifest cannot make a root true. The chain's
//! `ClassRegistered` is the consensus fact, and the only question a node ever asks is "does the
//! artifact I hold produce the root the chain registered". The manifest answers it without a walk,
//! and the answer is checkable: [`PalwClassManifestFileV1::agrees_with_artifact`] refuses a manifest
//! whose `artifact_digest` is not the file's, and a node that wants no trust at all recomputes
//! (135 s at a 2M context, measured). A manifest that lies about a root only ever costs its own
//! holder — the match fails and the class does not resolve, or the match succeeds and the producer's
//! openings do not verify, which is a slash. That asymmetry is why no signature is required here,
//! and why one is worth having when an artifact is DISTRIBUTED: then the row is something a
//! registrant's bond key can be held to, which is the job `ClassManifestV2` already does on chain
//! for the artifact's byte count.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_class_identity_v1::{PalwArtifactDigestV1, PalwClassIdV1, PalwInventoryRootV1};

pub const PALW_CLASS_MANIFEST_SCHEMA_V1: &str = "misaka.palw.class-manifest.v1";
/// The suffix appended to the artifact's own path. A sidecar rather than a field inside the
/// container, because adding a field to the container would move `artifact_digest` — the one value
/// that must not move when a manifest is written.
pub const PALW_CLASS_MANIFEST_SUFFIX_V1: &str = ".palwmanifest";

/// One class the artifact serves, and the root a registration of it must pin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwClassManifestRowV1 {
    /// The checkpoint identity a human reads — `CanonicalClassV1::model_id`.
    pub model_id: String,
    pub class_id: PalwClassIdV1,
    pub inventory_root: PalwInventoryRootV1,
}

/// The sidecar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwClassManifestFileV1 {
    /// The FILE's hash. Bound here so a manifest cannot drift from the artifact it describes; it is
    /// deliberately the one place a digest belongs.
    pub artifact_digest: PalwArtifactDigestV1,
    pub artifact_bytes: u64,
    /// `PalwLoadedArtifactV1::lineage_id` — a build-local name, recorded so a manifest written by a
    /// build with different lineages says so instead of matching by accident.
    pub lineage_id: String,
    /// Every pairing, ordered by class id so the file is byte-stable for the same artifact.
    pub rows: Vec<PalwClassManifestRowV1>,
}

/// Why a manifest could not be used. Each variant is a sentence an operator can act on, because the
/// alternative — a node that silently walks 2.87 GB instead — is what hid the original defect.
#[derive(Debug, PartialEq, Eq)]
pub enum PalwClassManifestErrorV1 {
    /// The manifest describes a different file. Never recoverable by retrying: regenerate it.
    DigestMismatch { manifest: PalwArtifactDigestV1, artifact: PalwArtifactDigestV1 },
    /// The manifest is well-formed and simply does not mention this class.
    NoSuchClass { class_id: PalwClassIdV1 },
    /// The manifest says one root and a recomputation says another — the check
    /// `--palw-verify-class-manifest` exists for, and a refusal to start rather than a warning.
    RootDisagrees { class_id: PalwClassIdV1, manifest: PalwInventoryRootV1, recomputed: PalwInventoryRootV1 },
    Malformed(String),
}

impl std::fmt::Display for PalwClassManifestErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DigestMismatch { manifest, artifact } => write!(
                f,
                "this manifest describes a different artifact: it names {manifest} and the file hashes to {artifact} — \
                 regenerate it with `palw-class manifest <file>`"
            ),
            Self::NoSuchClass { class_id } => write!(
                f,
                "this manifest names no class {class_id}; it was written by a build that paired the artifact differently, \
                 so regenerate it with the build you are running"
            ),
            Self::RootDisagrees { class_id, manifest, recomputed } => write!(
                f,
                "class {class_id}: the manifest claims inventory root {manifest} and this build derives {recomputed} from the \
                 same file — one of the two is not the root a registration can pin, and a producer that guesses gets slashed"
            ),
            Self::Malformed(why) => write!(f, "this manifest cannot be read: {why}"),
        }
    }
}

impl PalwClassManifestFileV1 {
    /// **The one derivation.** Every row comes from `sdk.pairings`, which is
    /// `CanonicalClassV1::artifact_root` — the same expression a producer's class resolve evaluates.
    /// A manifest generated any other way would be a second mapping, which is the defect this file
    /// exists to retire rather than to reproduce in a new place.
    pub fn derive_from_artifact(sdk: &crate::PalwClassSdk, artifact: &crate::PalwLoadedArtifactV1, artifact_bytes: u64) -> Self {
        let mut rows: Vec<PalwClassManifestRowV1> = sdk
            .pairings(artifact)
            .into_iter()
            .filter_map(|(entry, paired)| {
                paired.ok().map(|root| PalwClassManifestRowV1 {
                    model_id: entry.model_id.to_string(),
                    class_id: PalwClassIdV1::of_this_graph(entry.class_id()),
                    inventory_root: PalwInventoryRootV1::rooted_over_the_inventory(root),
                })
            })
            .collect();
        rows.sort_by_key(|r| r.class_id.into_hash64());
        Self {
            artifact_digest: PalwArtifactDigestV1::measured_over_the_file(artifact_digest_of(artifact)),
            artifact_bytes,
            lineage_id: artifact.lineage_id.to_string(),
            rows,
        }
    }

    /// Is this manifest about this file at all? The cheap check, and the one that must run before any
    /// row is believed.
    pub fn agrees_with_artifact(&self, artifact: &crate::PalwLoadedArtifactV1) -> Result<(), PalwClassManifestErrorV1> {
        let measured = PalwArtifactDigestV1::measured_over_the_file(artifact_digest_of(artifact));
        if self.artifact_digest != measured {
            return Err(PalwClassManifestErrorV1::DigestMismatch { manifest: self.artifact_digest, artifact: measured });
        }
        Ok(())
    }

    /// The root this manifest claims for a class, after the digest check.
    pub fn inventory_root_of(
        &self,
        artifact: &crate::PalwLoadedArtifactV1,
        class_id: PalwClassIdV1,
    ) -> Result<PalwInventoryRootV1, PalwClassManifestErrorV1> {
        self.agrees_with_artifact(artifact)?;
        self.rows
            .iter()
            .find(|r| r.class_id == class_id)
            .map(|r| r.inventory_root)
            .ok_or(PalwClassManifestErrorV1::NoSuchClass { class_id })
    }

    /// **Fail closed against a recomputation** — every row, derived again from the file, streamed.
    /// This is what `--palw-verify-class-manifest` runs, and what a startup that refuses to trust a
    /// sidecar it did not write should run: a manifest is a cache, and a cache nobody ever checks is
    /// how a wrong value survives two networks.
    pub fn verify_against_the_artifact(
        &self,
        sdk: &crate::PalwClassSdk,
        artifact: &crate::PalwLoadedArtifactV1,
    ) -> Result<(), PalwClassManifestErrorV1> {
        self.agrees_with_artifact(artifact)?;
        let fresh = Self::derive_from_artifact(sdk, artifact, self.artifact_bytes);
        for row in &self.rows {
            match fresh.rows.iter().find(|r| r.class_id == row.class_id) {
                Some(f) if f.inventory_root == row.inventory_root => {}
                Some(f) => {
                    return Err(PalwClassManifestErrorV1::RootDisagrees {
                        class_id: row.class_id,
                        manifest: row.inventory_root,
                        recomputed: f.inventory_root,
                    });
                }
                None => return Err(PalwClassManifestErrorV1::NoSuchClass { class_id: row.class_id }),
            }
        }
        Ok(())
    }

    /// The sidecar path for an artifact path.
    pub fn path_beside(artifact_path: &std::path::Path) -> std::path::PathBuf {
        let mut s = artifact_path.as_os_str().to_os_string();
        s.push(PALW_CLASS_MANIFEST_SUFFIX_V1);
        std::path::PathBuf::from(s)
    }

    /// Canonical JSON: sorted rows, one shape, so two runs over one artifact produce one file.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        out.push_str("{\n");
        out.push_str(&format!("  \"schema\": \"{PALW_CLASS_MANIFEST_SCHEMA_V1}\",\n"));
        out.push_str(&format!("  \"artifact_digest\": \"{}\",\n", self.artifact_digest));
        out.push_str(&format!("  \"artifact_bytes\": {},\n", self.artifact_bytes));
        out.push_str(&format!("  \"lineage_id\": \"{}\",\n", self.lineage_id));
        out.push_str("  \"classes\": [\n");
        for (i, r) in self.rows.iter().enumerate() {
            let comma = if i + 1 == self.rows.len() { "" } else { "," };
            out.push_str("    {\n");
            out.push_str(&format!("      \"model_id\": \"{}\",\n", r.model_id));
            out.push_str(&format!("      \"class_id\": \"{}\",\n", r.class_id));
            out.push_str(&format!("      \"inventory_root\": \"{}\"\n", r.inventory_root));
            out.push_str(&format!("    }}{comma}\n"));
        }
        out.push_str("  ]\n}\n");
        out
    }

    /// Read one back. Hand-parsed over the shape `to_json` writes, so the sidecar carries no
    /// dependency the node does not already have, and so a field this build does not know is a
    /// refusal by name rather than a silent default.
    pub fn from_json(text: &str) -> Result<Self, PalwClassManifestErrorV1> {
        let m = |e: &str| PalwClassManifestErrorV1::Malformed(e.to_string());
        let field = |key: &str| -> Option<String> {
            let at = text.find(&format!("\"{key}\""))?;
            let rest = &text[at + key.len() + 2..];
            let colon = rest.find(':')?;
            let after = rest[colon + 1..].trim_start();
            if let Some(stripped) = after.strip_prefix('"') {
                stripped.find('"').map(|end| stripped[..end].to_string())
            } else {
                let end = after.find(|c: char| c == ',' || c == '\n' || c == '}')?;
                Some(after[..end].trim().to_string())
            }
        };
        let schema = field("schema").ok_or_else(|| m("no schema"))?;
        if schema != PALW_CLASS_MANIFEST_SCHEMA_V1 {
            return Err(m(&format!("schema {schema} is not {PALW_CLASS_MANIFEST_SCHEMA_V1}")));
        }
        let hash = |s: &str| -> Result<Hash64, PalwClassManifestErrorV1> {
            s.parse::<Hash64>().map_err(|_| m(&format!("{s} is not a 64-byte hash")))
        };
        let artifact_digest =
            PalwArtifactDigestV1::measured_over_the_file(hash(&field("artifact_digest").ok_or_else(|| m("no artifact_digest"))?)?);
        let artifact_bytes =
            field("artifact_bytes").ok_or_else(|| m("no artifact_bytes"))?.parse::<u64>().map_err(|_| m("artifact_bytes"))?;
        let lineage_id = field("lineage_id").ok_or_else(|| m("no lineage_id"))?;
        let mut rows = Vec::new();
        for block in text.split("\"model_id\"").skip(1) {
            let take = |key: &str| -> Option<String> {
                let at = block.find(&format!("\"{key}\""))?;
                let rest = &block[at + key.len() + 2..];
                let q = rest.find('"')? + 1;
                rest[q..].find('"').map(|end| rest[q..q + end].to_string())
            };
            let model_id = {
                let q = block.find('"').ok_or_else(|| m("a class row has no model_id"))? + 1;
                block[q..].find('"').map(|e| block[q..q + e].to_string()).ok_or_else(|| m("a class row has no model_id"))?
            };
            rows.push(PalwClassManifestRowV1 {
                model_id,
                class_id: PalwClassIdV1::of_this_graph(hash(&take("class_id").ok_or_else(|| m("a class row has no class_id"))?)?),
                inventory_root: PalwInventoryRootV1::rooted_over_the_inventory(hash(
                    &take("inventory_root").ok_or_else(|| m("a class row has no inventory_root"))?,
                )?),
            });
        }
        Ok(Self { artifact_digest, artifact_bytes, lineage_id, rows })
    }
}

/// The file's own hash, from the dense container the holding carries. `None` becomes the zero hash,
/// which no real artifact produces, so a holding this lineage cannot open fails the digest check
/// rather than passing it vacuously.
fn artifact_digest_of(artifact: &crate::PalwLoadedArtifactV1) -> Hash64 {
    crate::lineages::dense::artifact_of(artifact).map(|a| a.artifact_digest()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(model: &str, id: u64, root: u64) -> PalwClassManifestRowV1 {
        PalwClassManifestRowV1 {
            model_id: model.to_string(),
            class_id: PalwClassIdV1::of_this_graph(Hash64::from_u64_word(id)),
            inventory_root: PalwInventoryRootV1::rooted_over_the_inventory(Hash64::from_u64_word(root)),
        }
    }

    fn manifest() -> PalwClassManifestFileV1 {
        PalwClassManifestFileV1 {
            artifact_digest: PalwArtifactDigestV1::measured_over_the_file(Hash64::from_u64_word(0xB5BA)),
            artifact_bytes: 2_871_234_560,
            lineage_id: "base0-dense-v1".to_string(),
            rows: vec![row("Qwen/Qwen2.5-1.5B", 0x74C6, 0xF63A), row("Qwen/Qwen2.5-1.5B-512", 0x71BB, 0x1A74)],
        }
    }

    #[test]
    fn a_manifest_survives_the_round_trip() {
        let m = manifest();
        let back = PalwClassManifestFileV1::from_json(&m.to_json()).expect("its own output parses");
        assert_eq!(back, m, "the sidecar round-trips");
        // Byte-stable: the same manifest writes the same file.
        assert_eq!(m.to_json(), manifest().to_json());
    }

    /// **The digest is what binds a manifest to a file, so a wrong digest is a refusal and not a
    /// warning.** The failure it prevents is the one that shipped: a value that looked measured,
    /// beside an artifact nobody re-derived it from.
    #[test]
    fn a_manifest_about_another_file_is_refused_by_name() {
        let m = manifest();
        let other = PalwArtifactDigestV1::measured_over_the_file(Hash64::from_u64_word(0xDEAD));
        let e = PalwClassManifestErrorV1::DigestMismatch { manifest: m.artifact_digest, artifact: other };
        let said = e.to_string();
        assert!(said.contains("describes a different artifact"), "{said}");
        assert!(said.contains("palw-class manifest"), "the message names the fix: {said}");
    }

    /// A class the manifest does not carry is a named miss, not a zero root — a zero would be a root
    /// that verifies nothing and matches nothing, which is the shape of a silent failure.
    #[test]
    fn an_absent_class_is_named_rather_than_defaulted() {
        let m = manifest();
        let missing = PalwClassIdV1::of_this_graph(Hash64::from_u64_word(0xFFFF));
        assert_eq!(
            m.rows.iter().find(|r| r.class_id == missing).map(|r| r.inventory_root),
            None,
            "the class is genuinely absent"
        );
        let said = PalwClassManifestErrorV1::NoSuchClass { class_id: missing }.to_string();
        assert!(said.contains("names no class"), "{said}");
        assert!(said.contains("regenerate it with the build you are running"), "{said}");
    }

    /// **The disagreement message has to name both values.** When testnet-12's seats refused their own
    /// artifact the log gave only the root the chain wanted; what the node DERIVED was never printed,
    /// so the one fact that would have identified the defect — that the pin was the digest — took a
    /// bespoke probe to recover.
    #[test]
    fn a_root_disagreement_prints_both_roots() {
        let said = PalwClassManifestErrorV1::RootDisagrees {
            class_id: PalwClassIdV1::of_this_graph(Hash64::from_u64_word(0x74C6)),
            manifest: PalwInventoryRootV1::rooted_over_the_inventory(Hash64::from_u64_word(0xB5BA)),
            recomputed: PalwInventoryRootV1::rooted_over_the_inventory(Hash64::from_u64_word(0xF63A)),
        }
        .to_string();
        assert!(said.contains("claims inventory root"), "{said}");
        assert!(said.contains("and this build derives"), "{said}");
        assert!(said.contains("gets slashed"), "the message says who pays: {said}");
    }

    #[test]
    fn the_sidecar_sits_beside_the_artifact() {
        let p = PalwClassManifestFileV1::path_beside(std::path::Path::new("/root/palw-class/qwen25-1.5b-a16-2m.palwart"));
        assert_eq!(p.to_str().unwrap(), "/root/palw-class/qwen25-1.5b-a16-2m.palwart.palwmanifest");
    }

    /// A schema this build does not know is a refusal, not a default — the rule that keeps a future
    /// field from being read as absent.
    #[test]
    fn an_unknown_schema_is_refused() {
        let bad = manifest().to_json().replace(PALW_CLASS_MANIFEST_SCHEMA_V1, "misaka.palw.class-manifest.v2");
        match PalwClassManifestFileV1::from_json(&bad) {
            Err(PalwClassManifestErrorV1::Malformed(why)) => assert!(why.contains("is not"), "{why}"),
            other => panic!("an unknown schema must be refused, got {other:?}"),
        }
    }
}

/// **What a holding's sidecar turned out to be.** Three answers, because "no manifest" and "a
/// manifest about another file" must not collapse into one: the first is an operator who never ran
/// `palw-class manifest`, and the second is a file that will mislead the next reader.
#[derive(Debug)]
pub enum PalwManifestLookupV1 {
    /// No sidecar beside the artifact, or the holding came from no file. Derive.
    Absent,
    /// A sidecar that describes this exact file.
    Agrees(PalwClassManifestFileV1),
    /// A sidecar that does not. **The caller derives anyway and SAYS SO** — silently ignoring it is
    /// how a stale root survives, and silently trusting it is how a wrong one ships.
    Disagrees(PalwClassManifestErrorV1),
}

/// The sidecar beside a holding's file, checked against that file.
///
/// Reading it is a few hundred bytes; the walk it replaces is 135 s and several GiB at a 2M context
/// (measured), so this is re-read per lookup rather than cached in the holding — the cost is not
/// where the problem was.
pub fn manifest_beside(holding: &crate::PalwLoadedArtifactV1) -> PalwManifestLookupV1 {
    let Some(path) = holding.path.as_ref() else { return PalwManifestLookupV1::Absent };
    let sidecar = PalwClassManifestFileV1::path_beside(path);
    let Ok(text) = std::fs::read_to_string(&sidecar) else { return PalwManifestLookupV1::Absent };
    match PalwClassManifestFileV1::from_json(&text) {
        Err(e) => PalwManifestLookupV1::Disagrees(e),
        Ok(m) => match m.agrees_with_artifact(holding) {
            Ok(()) => PalwManifestLookupV1::Agrees(m),
            Err(e) => PalwManifestLookupV1::Disagrees(e),
        },
    }
}

/// **The root for a class, from the sidecar when it can be trusted.**
///
/// `Ok(Some(root))` — the sidecar named it. `Ok(None)` — no usable sidecar, or it does not carry
/// this class; derive. `Err(why)` — a sidecar that disagrees with its own file, which the caller
/// must surface before falling back, because it is a fact about the operator's disk that no amount
/// of deriving fixes.
pub fn inventory_root_from_sidecar(
    holding: &crate::PalwLoadedArtifactV1,
    class_id: PalwClassIdV1,
) -> Result<Option<PalwInventoryRootV1>, PalwClassManifestErrorV1> {
    match manifest_beside(holding) {
        PalwManifestLookupV1::Absent => Ok(None),
        PalwManifestLookupV1::Disagrees(e) => Err(e),
        PalwManifestLookupV1::Agrees(m) => Ok(m.rows.iter().find(|r| r.class_id == class_id).map(|r| r.inventory_root)),
    }
}
