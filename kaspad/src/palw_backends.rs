//! **One place a node turns "the chain says class X" into "run it"** (ADR-0053).
//!
//! The producer and the panel both need a backend for a class the chain named, and both used to
//! construct `Base0Backend` directly. The dispatch lives here, once, keyed on the **class id the
//! chain named** — not on what the node happens to hold, not on a flag, and not on a guess from
//! the graph's shape.
//!
//! # What this used to be, and what removing a family removed
//!
//! Under ADR-0051 this was a family dispatch: a `match terms.family` sending a Metal/GGUF claim to
//! a black-box worker and everything else to the integer floor, because resolving one family's
//! claim through the other's backend would judge material by rules it was never committed under.
//! ADR-0053 withdrew that family, so the match is gone and with it the failure mode: there is one
//! way to execute a registered class, and a node either holds that class's artifact or it does
//! not. `resolve` still refuses rather than substitutes — the floor is DERIVED, so a node with
//! nothing installed can always serve it, and a converted class that this node lacks the artifact
//! for is an error and never a fallback to some class it does have.

use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_hashes::Hash64;

/// What a node holds that lets it act for some class.
pub struct PalwBackendRegistry {
    court: PalwCourtParamsV2,
    /// Converted artifacts for classes whose weights are not derivable. The floor needs none.
    class_artifacts: Vec<misaka_palw_base0::artifact::Base0ArtifactV1>,
    /// **Memory-mapped Qwen3.6 artifacts: the root this node computed, and the mapping.**
    ///
    /// `Arc`, because the registry is rebuilt per block while the artifact is a 33 GiB mapping
    /// whose root took a pass over the file to compute — both happen once, at load, and the
    /// per-block resolve is a pointer clone. The ROOT is this node's own computation over the
    /// bytes it holds (`Qwen36ArtifactV1::artifact_root`), matched below against what the CHAIN
    /// registered: derive, never declare, the same rule `resolve_class_v1` applies to the dense
    /// tier.
    qwen36_artifacts: Vec<(Hash64, std::sync::Arc<misaka_palw_base0::qwen36::Qwen36ArtifactV1>)>,
    /// The network this node runs, for the job contexts the Qwen3.6 backend builds.
    network_id: Vec<u8>,
}

impl PalwBackendRegistry {
    pub fn new(
        court: PalwCourtParamsV2,
        class_artifacts: Vec<misaka_palw_base0::artifact::Base0ArtifactV1>,
        qwen36_artifacts: Vec<(Hash64, std::sync::Arc<misaka_palw_base0::qwen36::Qwen36ArtifactV1>)>,
        network_id: Vec<u8>,
    ) -> Self {
        Self { court, class_artifacts, qwen36_artifacts, network_id }
    }

    /// **Resolve the class the chain named into something that can run it.**
    ///
    /// `class_id` and `artifact_root` come off the class record, so they are the chain's answer.
    /// A node that cannot serve that class says so — it does not fall back to one it can, because
    /// producing or judging under a class the chain did not name is worse than not participating.
    pub fn resolve(&self, class_id: Hash64, artifact_root: Hash64) -> Result<Box<dyn PalwExecutionBackendV1>, String> {
        if let Ok(resolved) = misaka_palw_base0::classes::resolve_class_v1(&self.court, class_id, artifact_root, &self.class_artifacts)
        {
            return Ok(Box::new(misaka_palw_base0::backend::Base0Backend::new(resolved)));
        }
        // The hybrid class. Its id is the court profile's — the same derivation the registration
        // used — so a chain that named it and a node that holds its artifact meet on two facts,
        // and a mismatch on either is a refusal that says which.
        let qwen36_id = qwen36_class_id_v1();
        if class_id == qwen36_id {
            if let Some((_, artifact)) = self.qwen36_artifacts.iter().find(|(root, _)| *root == artifact_root) {
                return Ok(Box::new(misaka_palw_base0::qwen36_backend::Qwen36Backend::new(
                    artifact.clone(),
                    "Qwen3.6-35B-A3B",
                    kaspa_consensus_core::palw_qwen36_profile::QWEN36_RC_CANONICAL,
                    qwen36_id,
                    self.network_id.clone(),
                )));
            }
            return Err(format!(
                "the chain names the Qwen3.6 class and this node holds no artifact whose computed root is {artifact_root}                  (pass the converted .palwq36 with --palw-class-artifact)"
            ));
        }
        Err(format!("this node cannot serve the registered class {class_id} (artifact root {artifact_root})"))
    }
}

/// **Load one `--palw-class-artifact` path, dispatched by the file's own magic.**
///
/// One flag serves both artifact kinds: the dense tier's file decodes whole (it is a few GiB at
/// most and digest-checked by its decoder), while a `.palwq36` is memory-mapped and its root is
/// COMPUTED — one pass over the file, ~2 minutes cold for the 33 GiB class — because the root is
/// this node's proof that it holds what the chain registered, and a root read from a sidecar
/// would be a declaration. Returns whichever side matched, or the error from the side the magic
/// named (never both errors: a file IS one kind, and the other side's complaint is noise).
pub enum LoadedClassArtifact {
    Dense(Box<misaka_palw_base0::artifact::Base0ArtifactV1>),
    Qwen36 { computed_root: Hash64, artifact: std::sync::Arc<misaka_palw_base0::qwen36::Qwen36ArtifactV1> },
}

pub fn load_class_artifact(path: &std::path::Path) -> Result<LoadedClassArtifact, String> {
    // The magic decides, from the first bytes, without reading the body.
    let mut head = [0u8; 8];
    {
        use std::io::Read;
        let mut f = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let n = f.read(&mut head).map_err(|e| format!("{}: {e}", path.display()))?;
        if n < 8 {
            return Err(format!("{}: shorter than any artifact magic", path.display()));
        }
    }
    if head == *misaka_palw_base0::qwen36::QWEN36_FILE_MAGIC {
        let artifact = misaka_palw_base0::qwen36::open_artifact(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let computed_root = artifact.artifact_root();
        return Ok(LoadedClassArtifact::Qwen36 { computed_root, artifact: std::sync::Arc::new(artifact) });
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    misaka_palw_base0::artifact::decode_artifact_file_v1(&bytes)
        .map(|a| LoadedClassArtifact::Dense(Box::new(a)))
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// The hybrid class's chain id, derived once per call from the registered geometry — the same
/// `shape_profile_id` the registration and the admission gate compute, because a second spelling
/// of the class id would be a second thing to drift.
pub fn qwen36_class_id_v1() -> Hash64 {
    kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v1(kaspa_consensus_core::palw_qwen36_profile::QWEN36_35B_A3B)
        .expect("the pinned geometry projects")
        .shape_profile_id()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn court() -> PalwCourtParamsV2 {
        PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2).expect("shipped court")
    }

    fn registry() -> PalwBackendRegistry {
        PalwBackendRegistry::new(court(), Vec::new(), Vec::new(), b"misaka-palw-rc".to_vec())
    }

    /// **The floor resolves on a node with nothing installed.** It is derived, so it needs no
    /// artifact and no worker — the property that keeps a plain Linux node the liveness anchor,
    /// and the property the withdrawn family could not have (its seats had to hold particular
    /// hardware before one claim could license).
    #[test]
    fn the_floor_resolves_with_no_files_at_all() {
        let entry = misaka_palw_base0::classes::canonical_class_by_model_id_v1(&court(), "PALW-BASE-0/rc").expect("the floor");
        let root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().expect("pinned");
        let backend = registry().resolve(entry.class_id(), root).expect("the floor resolves");
        assert_eq!(backend.model_id(), "PALW-BASE-0/rc");
    }

    /// **A class this node does not hold is an error, not a substitution.** The old dispatch could
    /// answer "wrong family" here; what is left is the honest question — does this node have the
    /// artifact the chain's `(class_id, artifact_root)` names — and the honest refusal.
    #[test]
    fn a_class_this_node_does_not_hold_is_refused_by_name() {
        let err = match registry().resolve(Hash64::from_u64_word(0x99), Hash64::from_u64_word(0xA1)) {
            Err(e) => e,
            Ok(b) => panic!("a node with no artifacts resolved an unknown class to {}", b.model_id()),
        };
        assert!(err.contains("cannot serve the registered class"), "{err}");
    }
}
