//! **One place a node turns "the chain says class X" into "run it"** (ADR-0051 step 6).
//!
//! The producer and the panel both need a backend for a class the chain named, and both used to
//! construct `Base0Backend` directly — which was correct while one family existed and is a silent
//! wrong answer the moment two do: a Family-M claim resolved through the integer backend would be
//! judged by rules its material was never committed under.
//!
//! So the dispatch lives here, once, keyed on the **class's registered terms**. Not on what the
//! node happens to hold, not on a flag, and not on a guess from the graph's shape: the family is
//! a consensus fact carried in `PalwClassTermsV2`, and reading it anywhere else would be a second
//! opinion about it.

use kaspa_consensus_core::palw_backend::{PalwExecutionBackendV1, PalwExecutionFamilyV1};
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_state_v2::PalwClassTermsV2;
use kaspa_hashes::Hash64;
use std::path::PathBuf;

/// What a node holds that lets it act for some class.
pub struct PalwBackendRegistry {
    court: PalwCourtParamsV2,
    /// Converted artifacts for deterministic classes whose weights are not derivable.
    class_artifacts: Vec<misaka_palw_base0::artifact::Base0ArtifactV1>,
    /// The pinned worker, if this node has one. `None` on every host without a GPU toolchain,
    /// which is most of them and must stay supported: the deterministic floor is the liveness
    /// anchor and may never require a runtime a Linux server cannot build.
    metal_worker: Option<PathBuf>,
    network_id: Vec<u8>,
}

impl PalwBackendRegistry {
    pub fn new(
        court: PalwCourtParamsV2,
        class_artifacts: Vec<misaka_palw_base0::artifact::Base0ArtifactV1>,
        metal_worker: Option<PathBuf>,
        network_id: Vec<u8>,
    ) -> Self {
        Self { court, class_artifacts, metal_worker, network_id }
    }

    /// **Resolve the class the chain named into something that can run it.**
    ///
    /// `terms` comes off the class record, so the family is the chain's answer. A node that cannot
    /// serve that family says so — it does not fall back to one it can, because producing or
    /// judging under the wrong family's rules is worse than not participating.
    pub fn resolve(
        &self,
        terms: PalwClassTermsV2,
        class_id: Hash64,
        artifact_root: Hash64,
    ) -> Result<Box<dyn PalwExecutionBackendV1>, String> {
        match terms.family {
            PalwExecutionFamilyV1::DeterministicInteger => {
                let resolved =
                    misaka_palw_base0::classes::resolve_class_v1(&self.court, class_id, artifact_root, &self.class_artifacts)
                        .map_err(|e| format!("this node cannot serve the registered deterministic class: {e}"))?;
                Ok(Box::new(misaka_palw_base0::backend::Base0Backend::new(resolved)))
            }
            PalwExecutionFamilyV1::MetalGguf => {
                let worker = self
                    .metal_worker
                    .clone()
                    .ok_or("this class is Metal/GGUF and this node has no worker (--palw-metal-worker)")?;
                // **The pins come off the CHAIN**, and the worker is then held to them. A node
                // that took its identity from its own binary would agree with itself; this is what
                // makes `check_runtime_identity` a check rather than a tautology.
                let on_chain = terms
                    .runtime_pins
                    .ok_or("a Metal/GGUF class with no registered runtime pins cannot be served — admission should have refused it")?;
                let pins = misaka_palw_metal::catalog::pins_from_chain(worker, self.network_id.clone(), &on_chain);
                if pins.shape_profile_id != class_id {
                    return Err(format!(
                        "this node builds class {} for a registration that names {class_id} — it holds a different model",
                        pins.shape_profile_id
                    ));
                }
                let backend = misaka_palw_metal::MetalBackend::new(pins);
                backend
                    .check_runtime_identity()
                    .map_err(|e| format!("this node's worker is not the runtime class {class_id} pins: {e}"))?;
                let _ = artifact_root;
                Ok(Box::new(backend))
            }
        }
    }

    /// Does this node have any way to serve this family at all? Cheap, and the honest thing to log
    /// at startup rather than discovering it per block.
    pub fn can_serve(&self, family: PalwExecutionFamilyV1) -> bool {
        match family {
            PalwExecutionFamilyV1::DeterministicInteger => true,
            PalwExecutionFamilyV1::MetalGguf => self.metal_worker.is_some(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_state_v2::PalwRuntimePinsV2;

    fn court() -> PalwCourtParamsV2 {
        PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2).expect("shipped court")
    }

    fn registry(worker: Option<PathBuf>) -> PalwBackendRegistry {
        PalwBackendRegistry::new(court(), Vec::new(), worker, b"misaka-palw-rc".to_vec())
    }

    /// **The floor still resolves, through the dispatch.** It is derived, so it needs no artifact
    /// and no worker — which is the property that keeps a Linux node the liveness anchor.
    #[test]
    fn the_deterministic_floor_resolves_with_no_worker_and_no_files() {
        let entry = misaka_palw_base0::classes::canonical_class_by_model_id_v1(&court(), "PALW-BASE-0/rc").expect("the floor");
        let root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().expect("pinned");
        let backend = registry(None)
            .resolve(PalwClassTermsV2::deterministic_default(), entry.class_id(), root)
            .expect("the floor resolves on a node with nothing installed");
        assert_eq!(backend.family(), PalwExecutionFamilyV1::DeterministicInteger);
        assert!(backend.family().is_court_adjudicable());
    }

    /// **A node without a worker refuses a Metal class rather than falling back.** Producing or
    /// judging under the wrong family's rules is worse than not participating, so the failure is
    /// explicit and names the missing flag.
    #[test]
    fn a_node_with_no_worker_refuses_a_metal_class_by_name() {
        let terms = PalwClassTermsV2 {
            family: PalwExecutionFamilyV1::MetalGguf,
            runtime_pins: Some(PalwRuntimePinsV2 {
                runtime_manifest_hash: Hash64::from_u64_word(1),
                runtime_class_id: Hash64::from_u64_word(2),
                model_profile_id: Hash64::from_u64_word(3),
                trace_scheme_id: Hash64::from_u64_word(4),
                cu_ruleset_id: Hash64::from_u64_word(5),
                tokenizer_id: Hash64::from_u64_word(6),
                prefill_tokens: 8,
                exact_decode_tokens: 4,
                max_context_tokens: 4096,
                vocab_size: 248_320,
            }),
            panel_seats: Some(2),
            panel_quorum: Some(2),
        };
        let err = match registry(None).resolve(terms, Hash64::from_u64_word(0x99), Hash64::from_u64_word(0xA1)) {
            Err(e) => e,
            Ok(b) => panic!("a node with no worker resolved a Metal class to {}", b.model_id()),
        };
        assert!(err.contains("no worker"), "{err}");
        assert!(!registry(None).can_serve(PalwExecutionFamilyV1::MetalGguf));
        assert!(registry(None).can_serve(PalwExecutionFamilyV1::DeterministicInteger), "the floor is always servable");
    }

    /// **A worker that is not the registered runtime is refused, not used.** The pins come off the
    /// chain and the worker is held to them — the check that stops a node agreeing with itself.
    #[test]
    fn a_worker_that_is_not_the_registered_runtime_is_refused() {
        let pins = PalwRuntimePinsV2 {
            runtime_manifest_hash: Hash64::from_u64_word(0xDEAD),
            runtime_class_id: Hash64::from_u64_word(2),
            model_profile_id: Hash64::from_u64_word(3),
            trace_scheme_id: Hash64::from_u64_word(4),
            cu_ruleset_id: Hash64::from_u64_word(5),
            tokenizer_id: Hash64::from_u64_word(6),
            prefill_tokens: 8,
            exact_decode_tokens: 4,
            max_context_tokens: 4096,
            vocab_size: 248_320,
        };
        let terms = PalwClassTermsV2 {
            family: PalwExecutionFamilyV1::MetalGguf,
            runtime_pins: Some(pins),
            panel_seats: Some(2),
            panel_quorum: Some(2),
        };
        let class_id = misaka_palw_metal::catalog::cat_m_0001_profile().shape_profile_id();
        let err = match registry(Some(PathBuf::from("/nonexistent/worker"))).resolve(terms, class_id, Hash64::from_u64_word(0xA1)) {
            Err(e) => e,
            Ok(b) => panic!("an absent worker was accepted as the registered runtime for {}", b.model_id()),
        };
        assert!(err.contains("is not the runtime"), "{err}");
    }
}
