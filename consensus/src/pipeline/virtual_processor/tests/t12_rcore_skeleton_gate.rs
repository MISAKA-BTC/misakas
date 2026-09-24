//! **ADR-0152 v3.1's v22 skeleton at the processor, on testnet-12** (S-SPEC §6 / T24's object half):
//! every object the v22 layout declares (tags 53–56) and every `ObjectiveOffence` of a declared kind
//! (4, 5, 6) is refused by name at the processor's own gate (`palw_v2_validate_objects`), dropped by
//! its acceptance walk with the block standing, and refused by the fold — on the shipped ruleset,
//! which arms `palw_rcore_plus`, and on its fence-off twin alike. No writer of R-core+ exists yet,
//! so the fence decides nothing here: that sameness is the dormancy this file pins.
//!
//! No block is mined: the gate, the walk and the fold are asked on the genesis state the processor
//! stored at construction, with the harness cards of `t12_round_lane_e2e::t12_with_harness_cards`.
use super::TestContext;
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::config::{Config, ConfigBuilder};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2;
use kaspa_consensus_core::palw_offence_v1::{PalwOffenceKindV1, palw_offence_evidence_digest_v1};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2 as Obj, PalwStateV2Error,
};
use kaspa_hashes::Hash64;

struct Skeleton {
    ctx: TestContext,
    bundle: PalwConsensusParamsV2,
    state: PalwChainStateV2,
    point: PalwBlockContextV2,
    cards: Vec<PalwBondKeyV2>,
}

/// testnet-12 with harness cards at genesis; `armed = false` is the fence-off twin (the fence and
/// C7 unset, the bundle's mirrors re-synced to the dormant values).
fn skeleton(armed: bool) -> Skeleton {
    let (config, bundle, _premine, _floats) = super::t12_round_lane_e2e::t12_with_harness_cards();
    assert!(config.params.palw_rcore_plus.is_some_and(|f| f.is_active(0)), "testnet-12 arms R-core+ from genesis");
    let (config, bundle): (Config, PalwConsensusParamsV2) = if armed {
        (config, bundle)
    } else {
        let mut params = config.params.clone();
        params.palw_rcore_plus = None;
        params.palw_rcore_conservative_classes = &[];
        params.sync_palw_rcore_plus();
        let config = ConfigBuilder::new(params).skip_proof_of_work().build();
        // The twin's own bundle (its mirrors re-synced), so the fold this harness drives reads the
        // ruleset the processor reads — M3's tag 55 folds past the fence and is refused below it.
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(twin) = &config.params.palw_consensus_mode else {
            unreachable!("testnet-12 is ConsensusV2")
        };
        let twin = twin.clone();
        (config, twin)
    };
    config.params.validate_palw_v2().expect("the fixture is a runnable ruleset");
    let ctx = TestContext::new(TestConsensus::new(&config));
    let (_, state) =
        ctx.consensus.virtual_processor().palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("the tip loads");
    let cards: Vec<PalwBondKeyV2> = bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            Obj::BondRegistered { bond, .. } => Some(*bond),
            _ => None,
        })
        .collect();
    let point = PalwBlockContextV2 {
        block: ctx.consensus.get_sink(),
        daa_score: ctx.consensus.get_virtual_daa_score() + 10,
        blue_score: 5,
        subsidy: 0,
    };
    Skeleton { ctx, bundle, state, point, cards }
}

impl Skeleton {
    fn validate(&self, object: &Obj) -> Result<(), String> {
        self.ctx.consensus.virtual_processor().palw_v2_validate_objects(
            &self.state,
            &self.bundle.state,
            &self.point,
            std::slice::from_ref(object),
        )
    }

    fn accepted(&self, object: &Obj) -> Vec<Obj> {
        self.ctx.consensus.virtual_processor().palw_v2_accepted_objects_for_tests(
            &self.state,
            &self.bundle.state,
            &self.point,
            vec![object.clone()],
            self.point.block,
        )
    }

    fn fold(&self, object: &Obj) -> Result<PalwChainStateV2, PalwStateV2Error> {
        self.ctx.consensus.virtual_processor().palw_v2_fold_accepted_for_tests(
            &self.state,
            &self.bundle.state,
            &self.point,
            std::slice::from_ref(object),
        )
    }

    /// The four v22 objects, each well-formed enough to reach the gate.
    fn objects(&self) -> Vec<Obj> {
        let card = self.cards[1];
        // Any well-formed binding: nothing past the gate's first arm reads it.
        let zeros = vec![0u8; 1 << 16];
        let binding: kaspa_consensus_core::palw_step_leg::PalwStepBindingV2 =
            borsh::BorshDeserialize::deserialize(&mut zeros.as_slice()).expect("an all-zero binding decodes");
        vec![
            Obj::ReporterCommitted { commitment: Hash64::from_u64_word(0x53), reporter: card, signature: vec![1; 8] },
            Obj::ReporterRevealed { offence_key: Hash64::from_u64_word(0x54), reporter: card, salt: [0x54; 32] },
            Obj::MaterialDisclosedV2 {
                claim: Hash64::from_u64_word(0x55),
                unit: kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1::Held(
                    kaspa_consensus_core::palw_held_da_v1::PalwHeldMissingV1::StepLeaf { leaf: 1 },
                ),
                answer: kaspa_consensus_core::palw_da_rcore_v1::PalwDaAnswerV1::Held(Box::new(
                    kaspa_consensus_core::palw_held_da_v1::PalwHeldDisclosureCarriageV1 {
                        version: 1,
                        claim: Hash64::from_u64_word(0x55),
                        missing: kaspa_consensus_core::palw_held_da_v1::PalwHeldMissingV1::StepLeaf { leaf: 1 },
                        binding,
                        disclosure: kaspa_consensus_core::palw_held_da_v1::PalwHeldDisclosureV1::StepRange {
                            opening: kaspa_consensus_core::palw_step_leg::PalwStepRangeOpeningV1 {
                                first_leaf_index: 0,
                                leaf_hashes: vec![],
                                siblings: vec![],
                            },
                        },
                        signature: Vec::new(),
                    },
                )),
                discloser: card,
                signature: vec![1; 8],
            },
            Obj::PanelUnavailableQuorum { claim: Hash64::from_u64_word(0x56), receipts: Vec::new() },
        ]
    }
}

/// **Tags 53, 54 and 56 are dropped by the gate and the walk and refused by the fold, fence on or
/// off; tag 55 is M3's past `palw_rcore_plus`.** The stateless ride table admits each (their shape
/// rules are their owners'), so what keeps the declared ones inert is the acceptance layer and the
/// fold, by name. Tag 55 past the fence is judged: this one's signature is junk, so the gate refuses
/// it for that, the walk drops it, and the fold — which never reads a signature — refuses it for the
/// claim it names, which this chain does not hold. Below the fence it is declared-not-landed.
#[tokio::test]
async fn rcore_v22_objects_are_dropped_by_the_gate_and_the_walk_and_refused_by_the_fold() {
    for armed in [true, false] {
        let s = skeleton(armed);
        for (tag, object) in (53u8..).zip(s.objects()) {
            assert_eq!(borsh::to_vec(&object).unwrap()[0], tag, "{object:?}");
            kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object)
                .expect("the ride table admits the shape; acceptance drops it");
            if armed && tag == 55 {
                let why = s.validate(&object).expect_err("the gate refuses an unsigned answer");
                assert!(why.contains("is not signed by the bond it names"), "tag 55: {why}");
                assert!(s.accepted(&object).is_empty(), "tag 55: the walk drops it with the block standing");
                let err = s.fold(&object).expect_err("the fold refuses it too");
                assert!(matches!(err, PalwStateV2Error::MissingClaim(_)), "tag 55: {err}");
                continue;
            }
            let why = s.validate(&object).expect_err("the gate drops a declared-not-landed object");
            assert!(why.contains("declared by the v22 layout"), "armed {armed}, tag {tag}: {why}");
            assert!(s.accepted(&object).is_empty(), "armed {armed}, tag {tag}: the walk drops it with the block standing");
            let err = s.fold(&object).expect_err("the fold refuses it too");
            assert!(matches!(err, PalwStateV2Error::RcoreObjectNotLanded(_)), "armed {armed}, tag {tag}: {err}");
        }
    }
}

/// **An `ObjectiveOffence` of kind 5 or 6 is dropped by the gate and the walk and refused by the
/// fold, fence on or off, by name and for good** (the skeleton review's F1: both are records only the
/// fold writes), whatever its evidence — before any key, slash or forfeiture is reached. Kind 4 is
/// M2's and is judged (`palw_offence_attribution` is armed in both twins): refused by its adjudicator
/// when its evidence does not decode. Nothing is written either way.
#[tokio::test]
async fn rcore_v22_offence_kinds_are_dropped_by_the_gate_and_the_walk_and_refused_by_the_fold() {
    use kaspa_consensus_core::palw_offence_v1::PalwOffenceVerifyError as E;
    for armed in [true, false] {
        let s = skeleton(armed);
        for kind in [PalwOffenceKindV1::ExecutorRefuted, PalwOffenceKindV1::DaDefault, PalwOffenceKindV1::CourtConviction] {
            let evidence = vec![kind as u8; 16];
            let object =
                Obj::ObjectiveOffence { kind, accused: s.cards[0], evidence_id: palw_offence_evidence_digest_v1(&evidence), evidence };
            let want = match kind {
                PalwOffenceKindV1::ExecutorRefuted => E::PanelFalseValidNeedsContradiction.to_string(),
                _ => E::KindNotFileable(kind.not_fileable_reason_v1()).to_string(),
            };
            let why = s.validate(&object).expect_err("the gate refuses it");
            assert_eq!(why, want, "armed {armed}, {kind:?}");
            assert!(s.accepted(&object).is_empty(), "armed {armed}, {kind:?}: the walk drops it");
            let err = s.fold(&object).expect_err("the fold refuses it too");
            assert!(
                matches!(&err, PalwStateV2Error::ObjectiveOffenceRefused(_, why) if *why == want),
                "armed {armed}, {kind:?}: {err}"
            );
        }
    }
}
