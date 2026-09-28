//! **The second IR fence's DA demand at the processor's gate** (`DefaultAccusedTirLeaf`, evidence
//! transport C), on testnet-12 as launched with harness keys on the eight cards: refused by name
//! below `palw_tir_fence2`; past it, admitted only when the accuser's registered key signed the demand
//! over the network, the claim, the leaf and the accuser, and when the binding rides without its
//! program. The claim, its class and the leaf's bound are the fold's (`palw_tir_da_leaf_fold`).
use super::t12_round_lane_e2e::{t12_genesis_chain, t12_with_harness_cards};
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::config::ConfigBuilder;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_da_rcore_v1::{palw_tir_leaf_accusation_object_v1, PalwTirLeafAccusationV1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_state_v2::{PalwBlockContextV2, PalwConsensusObjectV2 as Obj};
use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;
use kaspa_hashes::Hash64;

fn binding() -> kaspa_consensus_core::palw_tir_step_v1::PalwTirStepBindingV1 {
    let z = Hash64::from_bytes([0; 64]);
    kaspa_consensus_core::palw_tir_step_v1::PalwTirStepBindingV1 {
        version: kaspa_consensus_core::palw_tir_step_v1::PALW_TIR_STEP_BINDING_VERSION_V1,
        job_context: kaspa_consensus_core::palw_v2::PalwJobContextV2 {
            version: 2,
            network_id: b"misaka-palw-rc".to_vec(),
            job_id: z,
            job_nullifier: z,
            assignment_id: z,
            execution_seed: [0; 32],
            model_profile_id: z,
            runtime_manifest_hash: z,
            runtime_class_id: z,
            shape_profile_id: z,
            trace_scheme_id: z,
            cu_ruleset_id: z,
            tokenizer_id: z,
            prompt_token_ids_hash: z,
            declared_prefill_tokens: 1,
            exact_decode_tokens: 1,
            max_context_tokens: 2,
        },
        class: kaspa_consensus_core::palw_tir_class_v1::PalwTirClassV1 {
            version: kaspa_consensus_core::palw_tir_class_v1::PALW_TIR_CLASS_VERSION_V1,
            program: Vec::new(),
            layout: kaspa_consensus_core::palw_tir_class_v1::PalwTirLayoutV1 {
                version: kaspa_consensus_core::palw_tir_class_v1::PALW_TIR_LAYOUT_VERSION_V1,
                max_context: 1,
                checkpoint_interval: 1,
                h_tile: 1,
                commit_tiles: Vec::new(),
                state_tiles: Vec::new(),
            },
            tokenizer_id: z,
        },
        artifact_root: z,
        full_logits_trace_root: z,
        step_leaf_count: 1,
        step_merkle_root: z,
        committed_execution_root: z,
    }
}

#[tokio::test]
async fn t12_an_ir_leaf_demand_is_signed_by_its_accuser_and_refused_below_the_fence() {
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
        let signed = palw_tir_leaf_accusation_object_v1(&domain, claim, 7, &binding(), chain.bonds[card], sign).expect("the builder");
        let gate = |object: &Obj| vp.palw_v2_validate_objects(&state, &bundle.state, &point, std::slice::from_ref(object));
        if !fence2 {
            let refused = gate(&signed).expect_err("below the fence");
            assert!(refused.contains("palw_tir_fence2 is not in force"), "{refused}");
            assert!(vp.palw_v2_accepted_objects_for_tests(&state, &bundle.state, &point, vec![signed], block).is_empty(), "dropped");
            continue;
        }
        gate(&signed).expect("signed by its accuser, past the fence");
        let Obj::DefaultAccusedTirLeaf { accusation } = &signed else { unreachable!() };
        let edit = |f: &dyn Fn(&mut PalwTirLeafAccusationV1)| {
            let mut a = accusation.as_ref().clone();
            f(&mut a);
            Obj::DefaultAccusedTirLeaf { accusation: Box::new(a) }
        };
        for (what, object) in [
            ("another leaf", edit(&|a| a.index += 1)),
            ("another claim", edit(&|a| a.claim = Hash64::from_bytes([0x5D; 64]))),
            ("another accuser", edit(&|a| a.accuser = chain.bonds[3])),
            ("no signature", edit(&|a| a.signature.clear())),
            ("the program carried", edit(&|a| a.binding.class.program = vec![1, 2, 3])),
        ] {
            assert!(gate(&object).is_err(), "{what} is refused at the gate");
        }
    }
}
