//! # misaka-palw-sdk — the one interface every PALW model class passes through
//!
//! Adding an LLM to a MISAKA network is four agreements that must hold at once: a **graph** the
//! court can walk (whose id IS the class id), an **artifact** whose root the chain pins, a
//! **canonical job** the class is paid per, and an **engine** that executes what the graph
//! describes. Before this crate those agreements were kept per lineage, per consumer: two class
//! tables of different types, three per-lineage arms in the backend dispatch, two loops in the
//! panel's registration builder, a magic switch in the artifact loader — and every new model
//! family meant finding all of them.
//!
//! This crate is the seam. One trait — [`PalwModelLineageV1`] — says what a lineage must supply;
//! one registry — [`PalwClassSdk`] — is the only door node code and tooling go through; one
//! battery — [`conformance`] — enforces at `cargo test` the invariants the admission gate enforces
//! on chain.
//!
//! ## Adding a new LLM
//!
//! **A new checkpoint of a known lineage** (the common case) is a data change and no SDK code:
//! add the frozen geometry beside its family's profile module in `kaspa-consensus-core`, add the
//! table row in `misaka_palw_base0::classes`, convert the weights with the family's converter, and
//! the whole path — `palw-class inspect`/`preflight`, the node's `--palw-register-class`, backend
//! resolution — serves it immediately. The SDK's conformance tests cover the new row the moment it
//! exists.
//!
//! **A new model family** implements [`PalwModelLineageV1`] (container sniff/load, class table,
//! artifact↔class pairing, backend construction), registers it in
//! [`sdk::builtin_lineages_v1`] (or composes it via [`PalwClassSdk::with_lineage`]), and makes
//! [`conformance::check_lineage_v1`] its first test. Nothing else in the system learns the family
//! exists — that is the point.
//!
//! What stays OUTSIDE the SDK, deliberately: converting weights (each family's converter binary
//! owns its container), funding and signing (key custody is the node's), and fleet distribution.
//! The SDK is where a model becomes a class; it is not a wallet and not a deployment tool.

pub mod conformance;
pub mod lineage;
pub mod lineages {
    pub mod dense;
    pub mod qwen36;
}
pub mod sdk;

pub use lineage::{PalwClassEntryV1, PalwLoadedArtifactV1, PalwModelLineageV1};
pub use sdk::{PalwCandidateError, PalwClassSdk, PalwRegistrationCandidateV1, builtin_lineages_v1};

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
    use kaspa_consensus_core::palw_state_v2::PalwRegistrationTermsV2;
    use kaspa_hashes::Hash64;

    use super::*;

    /// **The RULESET's ladder, not the executor's constant** (audit D H-5). This was
    /// `PALW_STEP_MAX_LEAVES` (2^22) — the executor's own default — while the networks this SDK
    /// serves froze `PALW_RC_COURT_MAX_STEP_LEAF_COUNT` (2^26) for exactly the rows the ledger now
    /// carries. A battery run at 2^22 reports "the step space does not enumerate" for the graph-v5
    /// 512 row, which is a statement about a constant somewhere else and not about the class.
    fn court() -> PalwCourtParamsV2 {
        PalwCourtParamsV2::new(kaspa_consensus_core::palw_class_admission_v2::PALW_RC_COURT_MAX_STEP_LEAF_COUNT, 4, 2)
            .expect("shipped court")
    }

    fn sdk() -> PalwClassSdk {
        PalwClassSdk::builtin_v1(court(), b"misaka-palw-rc".to_vec())
    }

    fn empty_terms() -> PalwRegistrationTermsV2 {
        PalwRegistrationTermsV2 {
            min_grantable_share_permille: 1,
            slash_value_per_pwu: 1,
            initial_target: 1,
            registered_class_ids: Vec::new(),
            registered_artifact_roots: Vec::new(),
            chain_certified_families: Vec::new(),
        }
    }

    /// **The battery, over everything this build ships.** A new table row anywhere is covered by
    /// this test the moment it exists; a new lineage is covered the moment it joins
    /// `builtin_lineages_v1`. This is the call the trait's documentation promises.
    #[test]
    fn every_builtin_lineage_conforms() {
        conformance::check_sdk_v1(&sdk()).expect("every built-in class satisfies the adjudicability battery");
    }

    /// The ledger is the union of the lineage tables, and it names the classes the public
    /// networks actually run — the floor, both A16 rungs, and both qwen36 members.
    #[test]
    fn the_ledger_names_the_shipped_classes() {
        let ledger = sdk().ledger();
        for expected in [
            "PALW-BASE-0/rc",
            "Qwen/Qwen2.5-1.5B",
            "Qwen/Qwen2.5-Coder-1.5B-Instruct",
            "Qwen3.6-35B-A3B",
            "huihui-ai/Huihui-Qwen3-Coder-30B-A3B-Instruct-abliterated",
        ] {
            assert!(ledger.iter().any(|e| e.model_id == expected), "the ledger lost {expected}");
        }
        let floor = ledger.iter().find(|e| e.model_id == "PALW-BASE-0/rc").expect("floor");
        assert!(!floor.needs_artifact_file, "the floor is derived — it must never demand a file");
    }

    /// **The floor resolves on a node holding nothing** — the property that keeps a plain Linux
    /// node the liveness anchor — and an unknown class is refused with the sentence operators
    /// grep for.
    #[test]
    fn the_floor_resolves_from_nothing_and_unknown_classes_are_refused() {
        let s = sdk();
        let floor_id = misaka_palw_base0::classes::canonical_class_by_model_id_v1(&court(), "PALW-BASE-0/rc")
            .expect("the floor is in the registry")
            .class_id();
        let root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().expect("pinned");
        let backend = s.resolve(floor_id, root, &[]).expect("the derived floor resolves with no holdings");
        assert_eq!(backend.model_id(), "PALW-BASE-0/rc");

        let err = match s.resolve(Hash64::from_u64_word(0x99), Hash64::from_u64_word(0xA1), &[]) {
            Err(e) => e,
            Ok(b) => panic!("a node with no artifacts resolved an unknown class to {}", b.model_id()),
        };
        assert!(err.contains("cannot serve the registered class"), "{err}");
    }

    /// A synthetic lineage exercising the registry rules end to end — and, deliberately, the
    /// worked example of what adding a new family costs: this impl and one `with_lineage` call.
    struct TestLineage {
        entry_model_id: &'static str,
        weight_key: Hash64,
        root: Hash64,
    }

    impl PalwModelLineageV1 for TestLineage {
        fn lineage_id(&self) -> &'static str {
            "test-lineage"
        }
        fn classes(&self, court: &PalwCourtParamsV2) -> Vec<PalwClassEntryV1> {
            // The floor's real graph under a synthetic model id: valid, adjudicable, and cheap.
            let floor = misaka_palw_base0::classes::canonical_class_by_model_id_v1(court, "PALW-BASE-0/rc").expect("floor");
            vec![PalwClassEntryV1 {
                model_id: self.entry_model_id,
                lineage_id: "test-lineage",
                profile: floor.profile,
                canonical_job: floor.canonical_job,
                needs_artifact_file: true,
            }]
        }
        fn sniffs(&self, head: &[u8; 8]) -> bool {
            head == b"TESTLIN1"
        }
        fn load(&self, _path: &std::path::Path) -> Result<PalwLoadedArtifactV1, String> {
            Err("the test lineage loads nothing".into())
        }
        fn registered_weight_keys(&self, _artifact: &PalwLoadedArtifactV1) -> Vec<Hash64> {
            vec![self.weight_key]
        }
        fn pair(
            &self,
            _court: &PalwCourtParamsV2,
            _entry: &PalwClassEntryV1,
            _artifact: &PalwLoadedArtifactV1,
        ) -> Result<Hash64, String> {
            Ok(self.root)
        }
        fn resolve(
            &self,
            _court: &PalwCourtParamsV2,
            _class_id: Hash64,
            _artifact_root: Hash64,
            _holdings: &[PalwLoadedArtifactV1],
            _network_id: &[u8],
        ) -> Option<Result<Box<dyn kaspa_consensus_core::palw_backend::PalwExecutionBackendV1>, String>> {
            None
        }
    }

    fn test_holding() -> PalwLoadedArtifactV1 {
        PalwLoadedArtifactV1::from_parts("test-lineage", None, "a test holding".into(), Arc::new(()))
    }

    /// **The two candidate rules that exist because seats burned.** Known weights (a registered
    /// weight key) never candidate for a new class; an already-registered class id is dropped
    /// after matching, so "nothing matches" and "everything is registered" stay different
    /// refusals.
    #[test]
    fn candidates_refuse_known_weights_and_registered_classes() {
        let lineage =
            TestLineage { entry_model_id: "test/model", weight_key: Hash64::from_u64_word(7), root: Hash64::from_u64_word(9) };
        let class_id = lineage.classes(&court())[0].class_id();
        let s = PalwClassSdk::with_lineages(vec![Arc::new(lineage)], court(), b"misaka-palw-rc".to_vec());
        let holding = test_holding();

        // Clean terms: the pairing stands, and it is the one candidate.
        let picked = s.registration_candidate(std::slice::from_ref(&holding), &empty_terms(), None).expect("one candidate stands");
        assert_eq!(picked.entry.model_id, "test/model");
        assert_eq!(picked.artifact_root, Hash64::from_u64_word(9));

        // The weights are already on chain: the artifact is not looking for a class.
        let mut known_weights = empty_terms();
        known_weights.registered_artifact_roots = vec![Hash64::from_u64_word(7)];
        match s.registration_candidate(std::slice::from_ref(&holding), &known_weights, None) {
            Err(PalwCandidateError::NoMatch) => {}
            other => panic!("known weights must not candidate, got {other:?}"),
        }

        // The class is already on chain: matched, then dropped — the OTHER sentence.
        let mut registered = empty_terms();
        registered.registered_class_ids = vec![class_id];
        match s.registration_candidate(std::slice::from_ref(&holding), &registered, None) {
            Err(PalwCandidateError::AllRegistered) => {}
            other => panic!("a registered class is AllRegistered, got {other:?}"),
        }

        // The operator names something the matches do not contain.
        match s.registration_candidate(&[holding], &empty_terms(), Some("not/this")) {
            Err(PalwCandidateError::FilterMatchesNothing { wanted }) => assert_eq!(wanted, "not/this"),
            other => panic!("a wrong filter is FilterMatchesNothing, got {other:?}"),
        }
    }

    /// Two lineages may not share an id, and only one may claim the container-fallback slot —
    /// the properties every dispatch above silently relies on.
    #[test]
    #[should_panic(expected = "share a lineage id")]
    fn two_lineages_may_not_share_an_id() {
        let a = TestLineage { entry_model_id: "a", weight_key: Hash64::default(), root: Hash64::default() };
        let b = TestLineage { entry_model_id: "b", weight_key: Hash64::default(), root: Hash64::default() };
        let _ = PalwClassSdk::with_lineages(vec![Arc::new(a), Arc::new(b)], court(), Vec::new());
    }

    /// **Preflight refuses before anything is signed or funded.** The floor's real profile passes
    /// against a court provisioned for it; the SAME entry against a root is still just the gate's
    /// answer — so the cheap way to see a refusal is a bundle whose ladder cannot hold the class.
    #[test]
    fn preflight_admission_answers_before_submission() {
        use kaspa_consensus_core::network::{NetworkId, NetworkType};
        let params: kaspa_consensus_core::config::params::Params = NetworkId::with_suffix(NetworkType::Testnet, 11).into();
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
            panic!("testnet-11 ships a ConsensusV2 bundle");
        };
        let s = PalwClassSdk::builtin_v1(bundle.court, params.net.to_string().into_bytes());
        let floor = s.ledger().into_iter().find(|e| e.model_id == "PALW-BASE-0/rc").expect("floor");
        let root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().expect("pinned");
        let shape = kaspa_consensus_core::palw_class_admission_v2::palw_admission_shape_at_v1(&params, bundle, &floor.profile, 0)
            .expect("the network's court has a shape");
        s.preflight_admission(bundle, &floor, root, &shape).expect("the floor admits under the network that runs it");

        // A court too shallow for the class: the gate refuses, and the refusal says so BEFORE any
        // signature or carrier exists. (Ladder depth is a bundle property; 2^4 leaves holds no
        // real class.)
        let mut shallow = bundle.clone();
        shallow.court = PalwCourtParamsV2::new(16, 4, 2).expect("a legal, tiny court");
        let err = s.preflight_admission(&shallow, &floor, root, &shape).expect_err("a class deeper than the ladder is refused");
        assert!(err.contains("nothing was signed or funded"), "{err}");
    }

    /// **The ADR-0082 devnet drill's stage-2 failure, as a unit test.** The graph-v5 row
    /// preflights under the court devnet arms at genesis (the shape the acceptance path judges
    /// by), and is refused BY NAME under no court — so a preflight that asked a court-less gate
    /// reported "would be refused" for a row the chain admits, and the panel never signed.
    #[test]
    fn a_fused_row_preflights_under_the_court_the_ruleset_arms_and_not_without_it() {
        use kaspa_consensus_core::palw_class_admission_v2::{PalwAdmissionShapeV1, palw_admission_shape_at_v1};
        let params = kaspa_consensus_core::config::params::devnet_shipped_params();
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
            panic!("devnet ships a ConsensusV2 bundle");
        };
        let s = PalwClassSdk::builtin_v1(bundle.court, params.net.to_string().into_bytes());
        let row = s
            .ledger()
            .into_iter()
            .find(|e| e.model_id == misaka_palw_base0::classes::A16_GRAPH_V5_MODEL_ID)
            .expect("this build's SDK knows the graph-v5 row");
        let root = kaspa_hashes::Hash64::from_u64_word(0xA16);
        let armed = palw_admission_shape_at_v1(&params, bundle, &row.profile, 0).expect("devnet's court has a shape");
        assert!(armed.court.is_some(), "devnet arms palw_kary_court at genesis");
        s.preflight_admission(bundle, &row, root, &armed).expect("the fused row is admissible under the court the ruleset arms");

        let dormant = PalwAdmissionShapeV1 { court: None, ladder: None };
        let err = s.preflight_admission(bundle, &row, root, &dormant).expect_err("no court, no fused row");
        assert!(err.contains("has no dissection to try it with"), "{err}");
    }

    /// **The chain we cut is not the chain the drill runs.** Devnet arms `palw_context_ladder`;
    /// testnet-11 (`palw_rc_shipped_params`) leaves it dormant, so the panel there asks the gate
    /// with the court armed and `ladder: None` — the bundle's own 2^26 count, no ladder rules.
    /// The fused row must be admissible through that exact shape too, or a green devnet drill
    /// would be a network the cut chain is not (8e's question at the freeze).
    #[test]
    fn a_fused_row_preflights_under_t11s_court_with_the_ladder_dormant() {
        use kaspa_consensus_core::palw_class_admission_v2::{PalwAdmissionShapeV1, palw_admission_shape_at_v1};
        let params = kaspa_consensus_core::config::params::palw_rc_shipped_params();
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
            panic!("testnet-11 ships a ConsensusV2 bundle");
        };
        assert!(params.palw_context_ladder.is_none(), "t11 leaves palw_context_ladder dormant (the 5f card §1)");
        let s = PalwClassSdk::builtin_v1(bundle.court, params.net.to_string().into_bytes());
        let row = s
            .ledger()
            .into_iter()
            .find(|e| e.model_id == misaka_palw_base0::classes::A16_GRAPH_V5_MODEL_ID)
            .expect("this build's SDK knows the graph-v5 row");
        let shape = palw_admission_shape_at_v1(&params, bundle, &row.profile, 0).expect("t11's court has a shape");
        assert!(shape.court.is_some(), "t11 arms palw_kary_court at genesis");
        assert!(shape.ladder.is_none(), "and the dormant ladder fence yields no rules — the gate counts at the bundle's ladder");
        let root = kaspa_hashes::Hash64::from_u64_word(0xA16);
        let admitted = s
            .preflight_admission(bundle, &row, root, &shape)
            .expect("the fused row is admissible under t11's armed court with the ladder dormant");
        // Counted at the RULESET's ladder (2^26), the number the gate recounts — above the
        // executor's 2^22, which is why the probe must be counted against the bundle (audit D H-5b).
        let counted = kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(
            &row.profile,
            &row.canonical_context(),
            bundle.court.max_step_leaf_count(),
        )
        .expect("the canonical job counts against the ruleset's ladder");
        assert_eq!(admitted.canonical_step_leaf_count, counted, "the entry carries the count the gate recomputed");
        assert!(counted > 1 << 22, "the 512 row does not fit the executor's 2^22 — the bundle's ladder is what admits it");

        // **And the gate prices the row identically whether the ladder fence is armed or dormant**:
        // the entry with the court's rules passed explicitly is the entry with none passed, byte for
        // byte — the court, not the context-ladder fence, is what the dissection price follows.
        let court = shape.court.expect("t11 arms the court");
        let rules = kaspa_consensus_core::palw_context_ladder::palw_class_ladder_rules_for_court_v1(
            &row.profile,
            Some(court),
            bundle.court.max_step_leaf_count(),
        )
        .expect("the graph-v5 row has ladder rules under the court");
        let with_rules = PalwAdmissionShapeV1 { court: Some(court), ladder: Some(rules) };
        let priced = s.preflight_admission(bundle, &row, root, &with_rules).expect("the same row, the rules stated");
        assert_eq!(format!("{priced:?}"), format!("{admitted:?}"), "one price for the fused row under one court");
    }
}
