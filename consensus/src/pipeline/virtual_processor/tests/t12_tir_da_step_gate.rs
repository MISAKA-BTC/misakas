//! **The second IR fence's DA demand at the processor's gate** (`DefaultAccusedTirStep`, evidence
//! transport C), on testnet-12 as launched with harness keys on the eight cards: refused by name
//! below `palw_tir_fence2`; past it, admitted only when the accuser's registered key signed the demand
//! over the network, the claim, the unit and the accuser, and when the unit is a step leaf, an
//! interior step node or a rows-tree node (a row at level 0) inside the widest execution a binding may
//! commit. The demand carries no
//! binding: the claim and its class are the fold's, and a unit past the claim's own execution is the
//! accused's to prove (`palw_tir_da_step_fold`).
use super::t12_round_lane_e2e::{t12_genesis_chain, t12_with_harness_cards};
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::config::ConfigBuilder;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_da_rcore_v1::{PalwDaUnitV1, PalwTirStepAccusationV1, palw_tir_step_accusation_object_v1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_state_v2::{PalwBlockContextV2, PalwConsensusObjectV2 as Obj};
use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;
use kaspa_hashes::Hash64;

#[tokio::test]
async fn t12_an_ir_step_demand_is_signed_by_its_accuser_and_refused_below_the_fence() {
    let (config, _, premine, floats) = t12_with_harness_cards();
    let armed = |fence2: bool| {
        let mut params = config.params.clone();
        params.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(0)));
        params.sync_palw_tir_v1();
        if fence2 {
            params.palw_tir_fence2 = Some(ForkActivation::new(0));
            params.sync_palw_tir_fence2();
        }
        let c = ConfigBuilder::new(params).skip_proof_of_work().build();
        c.params.validate_palw_v2().expect("a runnable ruleset");
        c
    };
    for fence2 in [false, true] {
        let config = armed(fence2);
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
            PalwDaUnitV1::TirStepLeaf { index: 7 },
            PalwDaUnitV1::TirStepNode { level: 22, index: 0 },
            PalwDaUnitV1::TirRowNode { level: 0, index: 7 },
            PalwDaUnitV1::TirRowNode { level: 22, index: 0 },
        ] {
            let signed = palw_tir_step_accusation_object_v1(&domain, claim, unit, chain.bonds[card], sign).expect("the builder");
            if !fence2 {
                let refused = gate(&signed).expect_err("below the fence");
                assert!(refused.contains("palw_tir_fence2 is not in force"), "{refused}");
                assert!(
                    vp.palw_v2_accepted_objects_for_tests(&state, &bundle.state, &point, vec![signed], block).is_empty(),
                    "dropped"
                );
                continue;
            }
            gate(&signed).expect("signed by its accuser, past the fence");
            let Obj::DefaultAccusedTirStep { accusation } = &signed else { unreachable!() };
            let edit = |f: &dyn Fn(&mut PalwTirStepAccusationV1)| {
                let mut a = accusation.as_ref().clone();
                f(&mut a);
                Obj::DefaultAccusedTirStep { accusation: Box::new(a) }
            };
            for (what, object) in [
                ("another unit", edit(&|a| a.unit = PalwDaUnitV1::TirStepLeaf { index: 8 })),
                ("another claim", edit(&|a| a.claim = Hash64::from_bytes([0x5D; 64]))),
                ("another accuser", edit(&|a| a.accuser = chain.bonds[3])),
                ("no signature", edit(&|a| a.signature.clear())),
                ("an event unit", edit(&|a| a.unit = PalwDaUnitV1::Event { row: 0, tile: 0 })),
                ("a leaf past every execution", edit(&|a| a.unit = PalwDaUnitV1::TirStepLeaf { index: 1 << 22 })),
                ("a node above every tree", edit(&|a| a.unit = PalwDaUnitV1::TirStepNode { level: 23, index: 0 })),
                ("a node past its level", edit(&|a| a.unit = PalwDaUnitV1::TirStepNode { level: 22, index: 1 })),
                ("a leaf named as a node", edit(&|a| a.unit = PalwDaUnitV1::TirStepNode { level: 0, index: 7 })),
                ("a row past every trace", edit(&|a| a.unit = PalwDaUnitV1::TirRowNode { level: 0, index: 1 << 22 })),
                ("a rows node above every tree", edit(&|a| a.unit = PalwDaUnitV1::TirRowNode { level: 23, index: 0 })),
                ("a rows node past its level", edit(&|a| a.unit = PalwDaUnitV1::TirRowNode { level: 22, index: 1 })),
            ] {
                assert!(gate(&object).is_err(), "{what} is refused at the gate");
            }
        }
        if fence2 {
            // The builder refuses what the gate would.
            for unit in [
                PalwDaUnitV1::TirStepNode { level: 0, index: 0 },
                PalwDaUnitV1::TirStepLeaf { index: 1 << 22 },
                PalwDaUnitV1::TirRowNode { level: 23, index: 0 },
                PalwDaUnitV1::TirRowNode { level: 0, index: 1 << 22 },
            ] {
                assert!(palw_tir_step_accusation_object_v1(&domain, claim, unit, chain.bonds[card], sign).is_err(), "{unit:?}");
            }
        }
    }
}
