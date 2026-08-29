//! The PALW execution classes — the list a person consults before mining.
//!
//! On the MISAKA network a block is won by verified LLM inference, and *which* model you run is a
//! chain-registered **class**: an execution graph the whole network can re-derive, with a share of
//! the emission and an artifact every panel seat checks byte-for-byte. "Can I mine, and with
//! what?" therefore has a precise answer per class, and this module is that answer as data — the
//! UX equivalent of the model list, but for participation.
//!
//! Three classes ship in testnet-11's genesis (`docs/testnet11-join-mining.md` §5–6c and
//! `docs/palw-public-testnet-classes-runbook.md` in the misakas repository, plus the pinned
//! constants in its `consensus/core/src/config/params.rs`):
//!
//! | class | artifact | share |
//! |---|---|---|
//! | `PALW-BASE-0` | none — derived from a seed on every node | 600‰ |
//! | `PALW-QWEN25-A16` | `.palwart`, 1.7 GiB, converted locally | 200‰ |
//! | `QWEN36` | `qwen36.palwq36`, 34 GiB, downloadable | 200‰ |
//!
//! # What this table is, and is not
//!
//! It is a **pinned snapshot of the testnet-11 genesis registry**, kept here so the Studio can
//! show the list — with artifact identities a download can be verified against — before any node
//! is running. It is not the source of truth: the chain is, and a node's own startup check
//! (`the node checks this itself and refuses a mismatch`) is what finally gates production. If
//! the registry ever changes, this table is a release-note edit, exactly like the quantization
//! table.
//!
//! Every hash here is copied from the runbooks and the consensus constants verbatim, with its
//! provenance named, so a mismatch is attributable. The one value this table carries only as a
//! prefix is BASE-0's class id: the id is `shape_profile_id()` — a function of the execution
//! graph, computed by the node — and the docs print it truncated. The Studio displays what it can
//! prove and lets the node's own output supply the rest.

use serde::{Deserialize, Serialize};

/// How a class's artifact comes to exist on a machine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PalwArtifactSource {
    /// No file at all: the artifact is derived from a seed by every node. The floor class — the
    /// reason a machine with no GPU and no download can still produce blocks.
    DerivedFromSeed,
    /// A file published for download, verifiable against a pinned digest before use.
    Download {
        filename: &'static str,
        /// SHA-256 of the file itself — what the download manager verifies.
        sha256: &'static str,
        size_bytes: u64,
        /// Hugging Face repository holding it.
        hf_repo: &'static str,
        /// Converting from the source GGUF is the alternative route; named so the UI can offer
        /// both.
        convert_command: &'static str,
    },
    /// Must be converted locally from public weights — no direct download is published.
    ConvertLocally {
        /// Extension the converted artifact carries, e.g. `.palwart`.
        extension: &'static str,
        approx_size_bytes: u64,
        /// The public weights the conversion reads.
        source_repo: &'static str,
        convert_command: &'static str,
    },
}

/// One chain-registered execution class.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PalwClassSpec {
    /// The name operators know it by.
    pub name: &'static str,
    pub description: &'static str,
    /// Share of emission, in permille. The floor's is what remains after the model classes.
    pub share_permille: u16,
    /// The class id (`shape_profile_id()` over the execution graph), 128 hex chars where the
    /// docs publish it in full; a documented prefix otherwise. Display, never verification —
    /// verification is the artifact root, and the node performs it.
    pub class_id_hex: &'static str,
    /// Whether `class_id_hex` is the complete id or a documented prefix.
    pub class_id_complete: bool,
    /// The artifact root the chain registers (`Base0ArtifactV1::artifact_digest()`), 128 hex.
    /// What `--root-only` must print for the artifact to be the registered class.
    pub artifact_root_hex: &'static str,
    pub artifact: PalwArtifactSource,
    /// The floor: default when no class is named, exempt from the per-class epoch budget — the
    /// one class that can always produce.
    pub is_base: bool,
}

/// GiB, binary.
const GIB: u64 = 1 << 30;

/// The testnet-11 genesis classes.
///
/// Order is the order a newcomer should read them in: the one that needs nothing first.
pub const TESTNET11_CLASSES: &[PalwClassSpec] = &[
    PalwClassSpec {
        name: "PALW-BASE-0",
        description: "The deterministic integer floor. Its artifact is derived from a seed on every node — no GGUF, \
                      no download, no GPU — and it is exempt from the per-class epoch budget, so it can always \
                      produce. The default class when none is named.",
        share_permille: 600,
        // docs/palw-rc-testnet11-launch-runbook.md prints the first half; the id is computed by
        // the node (`shape_profile_id()`), and the Studio shows the node's own value once one is
        // connected.
        class_id_hex: "c185df95388739dc549777a9ca43866ddf773f1c84df77479a9eb59ba8d1d2b2",
        class_id_complete: false,
        // PALW_RC_GENESIS_ARTIFACT_ROOT is likewise derived per-network from the seed; the node
        // reports it. Empty here rather than a value this table cannot source.
        artifact_root_hex: "",
        artifact: PalwArtifactSource::DerivedFromSeed,
        is_base: true,
    },
    PalwClassSpec {
        name: "PALW-QWEN25-A16",
        description: "Qwen2.5-1.5B-Instruct, W8A16 static-PTQ — the dense tier. Converted locally from the public \
                      weights (~3 s, 2.9 GiB read); no download of the artifact itself is published.",
        share_permille: 200,
        class_id_hex: "",
        class_id_complete: false,
        // PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT, consensus/core/src/config/params.rs.
        artifact_root_hex: "c00faa480f2344d4a737e5b2e87ab6064d8d6e42c1ffeb6aa0a14ed62134299a\
                            7c9dc08f15342cefca1e29390810e6d2c5879f4c3853ebe43a9e2d47ed57ba17",
        artifact: PalwArtifactSource::ConvertLocally {
            extension: ".palwart",
            approx_size_bytes: 1_825_361_100, // ~1.7 GiB, per the classes runbook
            source_repo: "Qwen/Qwen2.5-1.5B-Instruct",
            convert_command: "qwen25-convert /path/to/Qwen2.5-1.5B-Instruct --a16 --out qwen25-1.5b-a16.palwart",
        },
        is_base: false,
    },
    PalwClassSpec {
        name: "QWEN36",
        description: "Qwen3.6-abliterated-35B-A3B under the hybrid integer runtime. The artifact is a 34 GiB \
                      conversion of the Q4_K_M GGUF — downloadable, or reproducible from the source GGUF; every \
                      route lands on the same registered root or the node refuses it.",
        share_permille: 200,
        // Printed in full in docs/testnet11-join-mining.md §6c.
        class_id_hex: "ec7bbcbffe13f36f1c2c418c65bdab840dd40b2bc22b217522dae836153078dd\
                       b77a92fb0645d34f98e9e3a1302e4543448a3924b3cd152fc74774ad3f02fb3f",
        class_id_complete: true,
        // PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT — what `qwen36-run --root-only` must print.
        artifact_root_hex: "f4aad4fd543928eb2d3a737555b09da9bf685fc515c0f8d4520988efcffacf08\
                            13d1b727537f0d03d349253aa11ef427e4047c2166b69fd7edb46a4a9984b368",
        artifact: PalwArtifactSource::Download {
            filename: "qwen36.palwq36",
            sha256: "7a944595a4256ab0aa4ca8b59f39fea268654b3630e54fb354cf1fa7658cf08c",
            size_bytes: 36_492_831_232,
            hf_repo: "Misakachain/Qwen3.6-35B-A3B-PALW-runtime",
            convert_command: "qwen36-convert --url <gguf url> --header header.bin --out qwen36.palwq36 --context 512",
        },
        is_base: false,
    },
];

/// Whether this machine holds a class's artifact, and whether it plausibly can run it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PalwClassReadiness {
    /// Nothing to obtain: the node derives the artifact itself.
    ReadyBuiltIn,
    /// The artifact file is present. `verified` is true only when its SHA-256 has been computed
    /// and matches the pin — presence alone is a filename, not an identity, and the field says
    /// which of the two the UI is showing.
    ArtifactPresent { path: String, size_bytes: u64, verified: bool },
    /// Not on disk. `downloadable` distinguishes "click to download" from "convert locally".
    ArtifactMissing { downloadable: bool },
    /// On disk but the wrong size — a truncated download or a different conversion. Named
    /// separately from Missing because the remedy differs: delete or re-verify, don't re-download
    /// beside it.
    ArtifactMismatch { path: String, size_bytes: u64, expected_bytes: u64 },
}

/// One class, assessed for this machine.
#[derive(Clone, Debug, Serialize)]
pub struct PalwClassStatus {
    pub spec: PalwClassSpec,
    pub readiness: PalwClassReadiness,
    /// A one-line memory note when the artifact is bigger than this machine's RAM — honest
    /// arithmetic (the hybrid runtime maps the artifact), not a benchmark.
    pub memory_note: Option<String>,
}

/// Assess every testnet-11 class against a directory scan and the machine.
///
/// `artifact_files` is (path, file name, size) for candidate artifact files — the caller scans
/// its models directory (and the node's app dir if it knows one); this stays pure so it is
/// testable without a filesystem.
pub fn assess_classes(artifact_files: &[(String, String, u64)], total_memory: u64) -> Vec<PalwClassStatus> {
    TESTNET11_CLASSES
        .iter()
        .map(|spec| {
            let readiness = match &spec.artifact {
                PalwArtifactSource::DerivedFromSeed => PalwClassReadiness::ReadyBuiltIn,
                PalwArtifactSource::Download { filename, size_bytes, .. } => {
                    match artifact_files.iter().find(|(_, name, _)| name == filename) {
                        Some((path, _, size)) if size == size_bytes => {
                            PalwClassReadiness::ArtifactPresent { path: path.clone(), size_bytes: *size, verified: false }
                        }
                        Some((path, _, size)) => {
                            PalwClassReadiness::ArtifactMismatch { path: path.clone(), size_bytes: *size, expected_bytes: *size_bytes }
                        }
                        None => PalwClassReadiness::ArtifactMissing { downloadable: true },
                    }
                }
                PalwArtifactSource::ConvertLocally { extension, .. } => {
                    match artifact_files.iter().find(|(_, name, _)| name.ends_with(extension)) {
                        // A conversion's byte size varies with its input, so presence is judged
                        // by extension and the root check is the node's.
                        Some((path, _, size)) => {
                            PalwClassReadiness::ArtifactPresent { path: path.clone(), size_bytes: *size, verified: false }
                        }
                        None => PalwClassReadiness::ArtifactMissing { downloadable: false },
                    }
                }
            };

            let artifact_bytes = match &spec.artifact {
                PalwArtifactSource::DerivedFromSeed => 0,
                PalwArtifactSource::Download { size_bytes, .. } => *size_bytes,
                PalwArtifactSource::ConvertLocally { approx_size_bytes, .. } => *approx_size_bytes,
            };
            let memory_note = (artifact_bytes > total_memory).then(|| {
                format!(
                    "the artifact is {:.1} GiB against {:.1} GiB of RAM — this machine cannot run this class",
                    artifact_bytes as f64 / GIB as f64,
                    total_memory as f64 / GIB as f64
                )
            });

            PalwClassStatus { spec: spec.clone(), readiness, memory_note }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_registry_snapshot_is_internally_consistent() {
        assert_eq!(TESTNET11_CLASSES.len(), 3);
        let total: u16 = TESTNET11_CLASSES.iter().map(|c| c.share_permille).sum();
        assert_eq!(total, 1000, "shares are permille of the whole emission");

        let base: Vec<_> = TESTNET11_CLASSES.iter().filter(|c| c.is_base).collect();
        assert_eq!(base.len(), 1, "exactly one floor");
        assert_eq!(base[0].name, "PALW-BASE-0");
        assert!(matches!(base[0].artifact, PalwArtifactSource::DerivedFromSeed));

        for class in TESTNET11_CLASSES {
            // A complete Hash64 is 128 hex chars; anything else must say it is a prefix.
            if class.class_id_complete {
                assert_eq!(class.class_id_hex.len(), 128, "{}", class.name);
            }
            if !class.artifact_root_hex.is_empty() {
                assert_eq!(class.artifact_root_hex.len(), 128, "{}", class.name);
            }
        }
    }

    #[test]
    fn the_floor_is_always_ready_even_on_an_empty_machine() {
        let statuses = assess_classes(&[], 8 << 30);
        let base = statuses.iter().find(|s| s.spec.is_base).expect("floor");
        assert_eq!(base.readiness, PalwClassReadiness::ReadyBuiltIn);
        assert!(base.memory_note.is_none());
    }

    #[test]
    fn a_present_artifact_is_reported_with_its_path_and_not_called_verified() {
        let files = vec![("/m/qwen36.palwq36".to_string(), "qwen36.palwq36".to_string(), 36_492_831_232u64)];
        let statuses = assess_classes(&files, 64 << 30);
        let qwen36 = statuses.iter().find(|s| s.spec.name == "QWEN36").expect("class");
        match &qwen36.readiness {
            PalwClassReadiness::ArtifactPresent { path, verified, .. } => {
                assert_eq!(path, "/m/qwen36.palwq36");
                assert!(!verified, "presence is a filename, not an identity");
            }
            other => panic!("expected present, got {other:?}"),
        }
    }

    /// A truncated 34 GiB download must not be shown as ready — the node would refuse it at
    /// startup, and the UI saying "present" until then wastes the operator's session.
    #[test]
    fn a_wrong_sized_artifact_is_a_mismatch_not_a_presence() {
        let files = vec![("/m/qwen36.palwq36".to_string(), "qwen36.palwq36".to_string(), 1_000_000u64)];
        let statuses = assess_classes(&files, 64 << 30);
        let qwen36 = statuses.iter().find(|s| s.spec.name == "QWEN36").expect("class");
        assert!(matches!(qwen36.readiness, PalwClassReadiness::ArtifactMismatch { expected_bytes: 36_492_831_232, .. }));
    }

    #[test]
    fn missing_artifacts_say_whether_a_download_exists() {
        let statuses = assess_classes(&[], 64 << 30);
        let qwen36 = statuses.iter().find(|s| s.spec.name == "QWEN36").expect("class");
        assert_eq!(qwen36.readiness, PalwClassReadiness::ArtifactMissing { downloadable: true });
        let qwen25 = statuses.iter().find(|s| s.spec.name == "PALW-QWEN25-A16").expect("class");
        assert_eq!(qwen25.readiness, PalwClassReadiness::ArtifactMissing { downloadable: false });
    }

    /// A 34 GiB class on a 16 GiB laptop: listed, and honest about why it will not run — not
    /// hidden, because seeing what stronger hardware could mine is part of the point of a list.
    #[test]
    fn an_oversized_class_carries_a_memory_note() {
        let statuses = assess_classes(&[], 16 << 30);
        let qwen36 = statuses.iter().find(|s| s.spec.name == "QWEN36").expect("class");
        let note = qwen36.memory_note.as_ref().expect("a note");
        assert!(note.contains("cannot run"), "{note}");
        let qwen25 = statuses.iter().find(|s| s.spec.name == "PALW-QWEN25-A16").expect("class");
        assert!(qwen25.memory_note.is_none(), "1.7 GiB fits a 16 GiB machine");
    }
}
