//! **Pipeline-claim data availability at the processor's gate** (`DefaultAccusedPipelineStep`, object tag 83;
//! spec 17 §17.14), on testnet-12 as launched with harness keys on the eight cards and the RFC-0002/0003/0004
//! fences armed from genesis: refused by name — and dropped by the acceptance walk — below
//! `palw_improvement_v1`; past it, admitted only when the accuser's registered key signed the demand over the
//! network, the claim, the unit and the accuser, and when the unit could be in some pipeline execution (a
//! stage below sixteen, a leaf below the widest ladder, an interior node inside the widest tree). The demand
//! carries no binding: the claim and its kind are the fold's (`palw_pipeline_da_fold_v1`'s tests), and a unit
//! past the claim's own execution is the accused's to prove.
use super::t12_round_lane_e2e::{t12_genesis_chain, t12_with_harness_cards};
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::config::ConfigBuilder;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1;
use kaspa_consensus_core::palw_gen_v1::PalwGenFenceV1;
use kaspa_consensus_core::palw_improve_v1::PalwImprovementFenceV1;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_pipeline_da_v1::{PalwPipelineStepAccusationV1, palw_pipeline_step_accusation_object_v1};
use kaspa_consensus_core::palw_state_v2::{PalwBlockContextV2, PalwConsensusObjectV2 as Obj};
use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;
use kaspa_hashes::Hash64;

#[tokio::test]
async fn t12_a_pipeline_step_demand_is_signed_by_its_accuser_and_refused_below_the_fence() {
    let (config, _, premine, floats) = t12_with_harness_cards();
    let armed = |improvement: bool| {
        let mut params = config.params.clone();
        params.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(0)));
        params.sync_palw_tir_v1();
        params.palw_tir_fence2 = Some(ForkActivation::new(0));
        params.sync_palw_tir_fence2();
        params.palw_gen_v1 = Some(PalwGenFenceV1::drill_v1(ForkActivation::new(0)));
        params.sync_palw_gen_v1();
        params.palw_fp_decode_rules = Some(ForkActivation::new(0));
        params.sync_palw_fp_decode_rules();
        if improvement {
            params.palw_improvement_v1 = Some(PalwImprovementFenceV1::drill_v1(ForkActivation::new(0)));
            params.sync_palw_improvement_v1();
        }
        let c = ConfigBuilder::new(params).skip_proof_of_work().build();
        c.params.validate_palw_v2().expect("a runnable ruleset");
        c
    };
    for improvement in [false, true] {
        let config = armed(improvement);
        let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!() };
        let chain = t12_genesis_chain(&config, bundle, &premine, &floats);
        let vp = chain.vp();
        let (block, state) = chain.tip_state();
        let point = PalwBlockContextV2 { block, daa_score: chain.daa_of(block), blue_score: 1, subsidy: 0 };
        let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            config.params.net.to_string().as_bytes(),
            Some(config.params.genesis.hash),
        );
        let card = 2usize;
        let key = TestConsensus::palw_v2_registry_keypair(card as u64);
        let sign = |message: &[u8], context: &[u8]| {
            libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, message, context, [0x62u8; 32]).ok().map(|s| s.as_ref().to_vec())
        };
        let claim = Hash64::from_bytes([0x5C; 64]);
        let gate = |object: &Obj| vp.palw_v2_validate_objects(&state, &bundle.state, &point, std::slice::from_ref(object));
        for unit in [
            PalwDaUnitV1::PipelineStepLeaf { stage: 0, index: 7 },
            PalwDaUnitV1::PipelineStepLeaf { stage: 15, index: (1 << 40) - 1 },
            PalwDaUnitV1::PipelineStepNode { stage: 1, level: 1, index: 3 },
            PalwDaUnitV1::PipelineStepNode { stage: 2, level: 40, index: 0 },
        ] {
            let signed = palw_pipeline_step_accusation_object_v1(&domain, claim, unit, chain.bonds[card], sign).expect("the builder");
            if !improvement {
                let refused = gate(&signed).expect_err("below the fence");
                assert!(refused.contains("palw_improvement_v1 is not in force"), "{refused}");
                assert!(
                    vp.palw_v2_accepted_objects_for_tests(&state, &bundle.state, &point, vec![signed], block).is_empty(),
                    "dropped by the acceptance walk, charged nothing"
                );
                continue;
            }
            gate(&signed).expect("signed by its accuser, past the fence");
            let Obj::DefaultAccusedPipelineStep { accusation } = &signed else { unreachable!() };
            let edit = |f: &dyn Fn(&mut PalwPipelineStepAccusationV1)| {
                let mut a = accusation.as_ref().clone();
                f(&mut a);
                Obj::DefaultAccusedPipelineStep { accusation: Box::new(a) }
            };
            for (what, object) in [
                ("another unit", edit(&|a| a.unit = PalwDaUnitV1::PipelineStepLeaf { stage: 3, index: 8 })),
                ("another claim", edit(&|a| a.claim = Hash64::from_bytes([0x5D; 64]))),
                ("another accuser", edit(&|a| a.accuser = chain.bonds[3])),
                ("no signature", edit(&|a| a.signature.clear())),
                ("an IR leaf unit", edit(&|a| a.unit = PalwDaUnitV1::TirStepLeaf { index: 7 })),
                ("an event unit", edit(&|a| a.unit = PalwDaUnitV1::Event { row: 0, tile: 0 })),
                ("a stage past sixteen", edit(&|a| a.unit = PalwDaUnitV1::PipelineStepLeaf { stage: 16, index: 0 })),
                ("a leaf past every execution", edit(&|a| a.unit = PalwDaUnitV1::PipelineStepLeaf { stage: 0, index: 1 << 40 })),
                ("a leaf named as a node", edit(&|a| a.unit = PalwDaUnitV1::PipelineStepNode { stage: 0, level: 0, index: 7 })),
                ("a node above every tree", edit(&|a| a.unit = PalwDaUnitV1::PipelineStepNode { stage: 0, level: 41, index: 0 })),
                ("a node past its level", edit(&|a| a.unit = PalwDaUnitV1::PipelineStepNode { stage: 0, level: 40, index: 1 })),
            ] {
                assert!(gate(&object).is_err(), "{what} is refused at the gate");
            }
        }
        if improvement {
            // The builder refuses what the gate would.
            for unit in [
                PalwDaUnitV1::PipelineStepNode { stage: 0, level: 0, index: 0 },
                PalwDaUnitV1::PipelineStepLeaf { stage: 16, index: 0 },
                PalwDaUnitV1::PipelineStepLeaf { stage: 0, index: 1 << 40 },
                PalwDaUnitV1::TirStepLeaf { index: 0 },
            ] {
                assert!(palw_pipeline_step_accusation_object_v1(&domain, claim, unit, chain.bonds[card], sign).is_err(), "{unit:?}");
            }
        }
    }
}
