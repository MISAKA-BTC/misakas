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
    /// `Arc`, for the same reason the mapped tier is: the registry is rebuilt per duty and per
    /// pooled payload, and a dense artifact is ~1.65 GiB. Cloning them per call copied gigabytes
    /// inside the panel's async tick (audit M2-14).
    class_artifacts: Vec<std::sync::Arc<misaka_palw_base0::artifact::Base0ArtifactV1>>,
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
        class_artifacts: Vec<std::sync::Arc<misaka_palw_base0::artifact::Base0ArtifactV1>>,
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
        let dense: Vec<misaka_palw_base0::artifact::Base0ArtifactV1> = self.class_artifacts.iter().map(|a| (**a).clone()).collect();
        if let Ok(resolved) = misaka_palw_base0::classes::resolve_class_v1(&self.court, class_id, artifact_root, &dense) {
            return Ok(Box::new(misaka_palw_base0::backend::Base0Backend::new(resolved)));
        }
        // The hybrid class. Its id is the court profile's — the same derivation the registration
        // used — so a chain that named it and a node that holds its artifact meet on two facts,
        // and a mismatch on either is a refusal that says which.
        // **The A16 dense class.** Its artifact rides the same container as the floor's, so it is
        // found in the same list — by its DIGEST, which is what the chain registered. Tried before
        // the hybrid because both are dense-file classes and only the id separates them.
        if let Some(entry) = misaka_palw_base0::classes::canonical_classes_v1(&self.court)
            .into_iter()
            .filter(|c| matches!(c.source, misaka_palw_base0::classes::ArtifactSourceV1::ConvertedA16))
            .find(|c| c.class_id() == class_id)
        {
            if let Some(artifact) = self.class_artifacts.iter().find(|a| a.artifact_digest() == artifact_root) {
                return Ok(Box::new(misaka_palw_base0::qwen25_a16_backend::Qwen25A16Backend::new(
                    artifact.clone(),
                    self.network_id.clone(),
                    class_id,
                    entry.canonical_job,
                )));
            }
            return Err(format!(
                "the chain names the {} class and this node holds no artifact whose digest is {artifact_root} \
                 (pass the converted .palwart with --palw-class-artifact)",
                entry.model_id
            ));
        }
        if let Some(entry) =
            misaka_palw_base0::classes::qwen36_canonical_classes_v1().into_iter().find(|c| c.class_id() == Some(class_id))
        {
            if let Some((_, artifact)) = self.qwen36_artifacts.iter().find(|(root, _)| *root == artifact_root) {
                return Ok(Box::new(misaka_palw_base0::qwen36_backend::Qwen36Backend::new(
                    artifact.clone(),
                    entry.model_id,
                    entry.canonical_job,
                    class_id,
                    self.network_id.clone(),
                )));
            }
            return Err(format!(
                "the chain names the {} class and this node holds no artifact whose computed root is {artifact_root} \
                 (pass the converted .palwq36 with --palw-class-artifact)",
                entry.model_id
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

    /// **The PUBLIC network's own class set, resolved against a node's holdings.**
    ///
    /// Not a fixture: this reads `Params::from(testnet-11)` — the ruleset a real node boots with —
    /// walks the classes its genesis registers, and asks the registry for each. The floor must
    /// resolve on a node holding nothing (it is derived); Qwen3.6 must be refused BY ROOT with the
    /// message that names the flag, because a node without the weights still validates the chain
    /// and simply cannot produce for that class.
    ///
    /// This is the test that would have caught a two-class ruleset whose second class no node
    /// could ever name — the class id the params register and the class id the registry dispatches
    /// on are derived in different modules, and nothing else compares them.
    #[test]
    fn the_public_networks_classes_resolve_the_way_a_node_would_ask() {
        use kaspa_consensus_core::config::params::palw_rc_qwen36_is_registered;
        let params: kaspa_consensus_core::config::params::Params =
            kaspa_consensus_core::network::NetworkId::with_suffix(kaspa_consensus_core::network::NetworkType::Testnet, 11).into();
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
            panic!("testnet-11 ships a ConsensusV2 bundle");
        };
        let classes: Vec<(Hash64, Hash64)> = bundle
            .genesis_objects
            .iter()
            .filter_map(|o| match o {
                kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, .. } => {
                    Some((*class_id, *artifact_root))
                }
                _ => None,
            })
            .collect();
        let expected = 1
            + usize::from(palw_rc_qwen36_is_registered())
            + usize::from(kaspa_consensus_core::config::params::palw_rc_qwen25_a16_is_registered());
        assert_eq!(classes.len(), expected, "the shipped network registers exactly the classes its pins describe");

        let bare = PalwBackendRegistry::new(bundle.court, Vec::new(), Vec::new(), params.net.to_string().into_bytes());
        let (floor_id, floor_root) = classes[0];
        assert_eq!(floor_id, bundle.base_class_id, "the floor is registered first");
        let floor = bare.resolve(floor_id, floor_root).expect("the derived floor resolves on a node holding nothing");
        assert_eq!(floor.model_id(), "PALW-BASE-0/rc");

        // Every non-floor class must be one this build can NAME (its id derives from a pinned
        // geometry here) and REFUSE BY ROOT on a node holding nothing. The floor is index 0; the
        // rest are checked by membership rather than by position, because the registration list's
        // order is the genesis gate's business and not this test's.
        let known: Vec<(Hash64, Hash64)> = [
            palw_rc_qwen36_is_registered()
                .then(|| (qwen36_class_id_v1(), kaspa_consensus_core::config::params::PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT)),
            kaspa_consensus_core::config::params::palw_rc_qwen25_a16_is_registered().then(|| {
                (
                    kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_class_id_v1(),
                    kaspa_consensus_core::config::params::PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT,
                )
            }),
        ]
        .into_iter()
        .flatten()
        .collect();
        for (id, root) in &known {
            let (_, registered_root) = classes
                .iter()
                .find(|(c, _)| c == id)
                .unwrap_or_else(|| panic!("the network registers a class this build dispatches on: {id}"));
            assert_eq!(registered_root, root, "the network's artifact root for {id} is the pinned one");
            let err = match bare.resolve(*id, *root) {
                Err(e) => e,
                Ok(b) => panic!("a node holding no weights resolved {id} to {}", b.model_id()),
            };
            assert!(err.contains("--palw-class-artifact"), "the refusal names the flag that fixes it: {err}");
        }
        assert_eq!(known.len() + 1, classes.len(), "every registered class is one this build can name");

        if let Some(&(qwen_id, qwen_root)) = classes.iter().find(|(c, _)| *c == qwen36_class_id_v1()) {
            assert_eq!(qwen_id, qwen36_class_id_v1(), "the registered second class is the one this build dispatches on");
            assert_eq!(
                qwen_root,
                kaspa_consensus_core::config::params::PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT,
                "the network's artifact root is the pinned one"
            );
            // And an artifact whose computed root is not the chain's is refused too — the file's
            // NAME is never the answer.
            let alien = PalwBackendRegistry::new(
                bundle.court,
                Vec::new(),
                vec![(Hash64::from_u64_word(0xA11E), std::sync::Arc::new(misaka_palw_base0::qwen36::qwen36_dev_fixture(1, 8)))],
                params.net.to_string().into_bytes(),
            );
            assert!(matches!(alien.resolve(qwen_id, qwen_root), Err(_)), "a file with the wrong root is not this class");
        }
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
