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
}

impl PalwBackendRegistry {
    pub fn new(court: PalwCourtParamsV2, class_artifacts: Vec<misaka_palw_base0::artifact::Base0ArtifactV1>) -> Self {
        Self { court, class_artifacts }
    }

    /// **Resolve the class the chain named into something that can run it.**
    ///
    /// `class_id` and `artifact_root` come off the class record, so they are the chain's answer.
    /// A node that cannot serve that class says so — it does not fall back to one it can, because
    /// producing or judging under a class the chain did not name is worse than not participating.
    pub fn resolve(&self, class_id: Hash64, artifact_root: Hash64) -> Result<Box<dyn PalwExecutionBackendV1>, String> {
        let resolved = misaka_palw_base0::classes::resolve_class_v1(&self.court, class_id, artifact_root, &self.class_artifacts)
            .map_err(|e| format!("this node cannot serve the registered class: {e}"))?;
        Ok(Box::new(misaka_palw_base0::backend::Base0Backend::new(resolved)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn court() -> PalwCourtParamsV2 {
        PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2).expect("shipped court")
    }

    fn registry() -> PalwBackendRegistry {
        PalwBackendRegistry::new(court(), Vec::new())
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
