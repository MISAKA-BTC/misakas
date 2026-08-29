//! **One place a node turns "the chain says class X" into "run it"** (ADR-0053) — now the SDK's
//! door inside kaspad.
//!
//! The dispatch itself lives in `misaka_palw_sdk`: the SDK holds one lineage list (the dense
//! container and the Qwen3.6 mmap tier today), and resolving a chain-named `(class_id,
//! artifact_root)` walks it — each lineage serves its class, refuses it by name, or passes. This
//! module keeps the node-side shape both services construct per duty, and nothing else: a new
//! lineage lands in the SDK and this file does not move, which is the property the old
//! three-armed dispatch could not have.
//!
//! `resolve` still refuses rather than substitutes — the floor is DERIVED, so a node with nothing
//! installed can always serve it, and a converted class this node lacks the artifact for is an
//! error and never a fallback to some class it does have.

use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_hashes::Hash64;
use misaka_palw_sdk::{PalwClassSdk, PalwLoadedArtifactV1};

/// What a node holds that lets it act for some class: the SDK (which classes exist, how they
/// load, pair, and execute) plus this node's loaded holdings. Rebuilt per duty and per pooled
/// payload; the holdings are `Arc`-backed inside, so the rebuild is pointer clones, not gigabytes
/// (audit M2-14).
pub struct PalwBackendRegistry {
    sdk: PalwClassSdk,
    holdings: Vec<PalwLoadedArtifactV1>,
}

impl PalwBackendRegistry {
    pub fn new(court: PalwCourtParamsV2, holdings: Vec<PalwLoadedArtifactV1>, network_id: Vec<u8>) -> Self {
        Self { sdk: PalwClassSdk::builtin_v1(court, network_id), holdings }
    }

    /// The SDK this registry dispatches through — the panel's registration builder asks it for
    /// candidates and admission preflight, against the same holdings `resolve` serves.
    pub fn sdk(&self) -> &PalwClassSdk {
        &self.sdk
    }

    pub fn holdings(&self) -> &[PalwLoadedArtifactV1] {
        &self.holdings
    }

    /// **Resolve the class the chain named into something that can run it.**
    ///
    /// `class_id` and `artifact_root` come off the class record, so they are the chain's answer.
    /// A node that cannot serve that class says so — it does not fall back to one it can, because
    /// producing or judging under a class the chain did not name is worse than not participating.
    pub fn resolve(&self, class_id: Hash64, artifact_root: Hash64) -> Result<Box<dyn PalwExecutionBackendV1>, String> {
        self.sdk.resolve(class_id, artifact_root, &self.holdings)
    }
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
        PalwBackendRegistry::new(court(), Vec::new(), b"misaka-palw-rc".to_vec())
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

        let bare = PalwBackendRegistry::new(bundle.court, Vec::new(), params.net.to_string().into_bytes());
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
            // And an artifact whose COMPUTED root is not the chain's is refused too — the file's
            // name is never the answer, and neither is a declared root: the holding derives its
            // root from the fixture's own bytes, and that root is not the registered class's.
            let alien = PalwBackendRegistry::new(
                bundle.court,
                vec![misaka_palw_sdk::lineages::qwen36::holding_from_artifact(
                    std::sync::Arc::new(misaka_palw_base0::qwen36::qwen36_dev_fixture(1, 8)),
                    None,
                )],
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
