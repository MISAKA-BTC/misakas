//! **LG14-B: the legacy held DA objects (tags 157–159) at the processor's gate and walk**, on testnet-12 as launched with harness
//! keys on the eight cards.
//!
//! * **Below `palw_legacy_held_da_v2`** (every shipped network): each object — and an int-12 answer (tag 55) carrying the appended
//!   unit — is dropped by name by the acceptance walk (the block stands, nothing is charged, A-2), and the gate refuses it by name.
//!   A block that carries them walks to exactly the objects a block without them walks to (byte identity of the walk).
//! * **Past the fence** (test-armed WITHOUT its validation, which refuses every height): admitted only when the bond the object names
//!   signed it over the network, the claim, the unit and that bond under the object's own context; another claim, another bond, no
//!   signature or a signature under another card's key is refused.
use super::t12_round_lane_e2e::{t12_genesis_chain, t12_with_harness_cards};
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::config::Config;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_da_rcore_v1::{PalwDaAnswerV1, PalwDaUnitV1};
use kaspa_consensus_core::palw_legacy_held_da_v2::{
    PALW_LEGACY_HELD_ANSWER_MLDSA87_CONTEXT_V2, PALW_LEGACY_HELD_DEMAND_MLDSA87_CONTEXT_V2, PALW_LEGACY_HELD_VERSION_V2,
    PALW_LEGACY_LEAF_RECOMPUTE_MLDSA87_CONTEXT_V2, PalwLegacyHeldAnswerCarriageV2, PalwLegacyHeldAnswerV2, PalwLegacyHeldDemandV2,
    PalwLegacyHeldUnitV2, PalwLegacyLeafRecomputeV2, palw_legacy_held_answer_message_v2, palw_legacy_held_demand_message_v2,
    palw_legacy_leaf_recompute_message_v2, palw_legacy_recompute_placeholder_v2,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_state_v2::{PalwBlockContextV2, PalwConsensusObjectV2 as Obj};
use kaspa_consensus_core::palw_step_leg::{PalwStepBindingV2, PalwStepOpeningV1};
use kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_hashes::Hash64;

/// A binding of the right SHAPE (the gate prices and signs over it; the fold, never this test, authenticates it).
fn a_binding() -> PalwStepBindingV2 {
    let profile =
        kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .expect("the RC BASE-0 profile");
    let h = Hash64::from_u64_word;
    let job_context = PalwJobContextV2 {
        version: kaspa_consensus_core::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V2,
        network_id: b"t12-gate".to_vec(),
        job_id: h(1),
        job_nullifier: h(2),
        assignment_id: h(3),
        execution_seed: [7; 32],
        model_profile_id: h(4),
        runtime_manifest_hash: h(5),
        runtime_class_id: h(6),
        shape_profile_id: profile.shape_profile_id(),
        trace_scheme_id: h(8),
        cu_ruleset_id: h(9),
        tokenizer_id: h(10),
        prompt_token_ids_hash: h(11),
        declared_prefill_tokens: 4,
        exact_decode_tokens: 1,
        max_context_tokens: 8,
    };
    PalwStepBindingV2 {
        version: kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_OBJECT_VERSION_V1,
        job_context,
        state_chunk_map_id: profile.state_chunk_map_id,
        shape_profile: profile,
        checkpoint_profile: kaspa_consensus_core::palw_legs::PalwCheckpointProfileV1 {
            version: kaspa_consensus_core::palw_legs::PALW_LEGS_OBJECT_VERSION_V1,
            checkpoint_interval: 1,
            state_layout_id: kaspa_consensus_core::palw_state_chunk_map::integer_kv_state_layout_id_v1(),
        },
        full_logits_trace_root: h(12),
        activation_leg_root: h(13),
        step_leaf_count: 64,
        step_merkle_root: h(14),
        checkpoint_count: 0,
        checkpoint_merkle_root: h(15),
        committed_execution_root: h(16),
    }
}

#[tokio::test]
async fn t12_legacy_held_objects_are_dropped_below_the_fence_and_signed_by_their_bond_past_it() {
    let (config, _, premine, floats) = t12_with_harness_cards();
    for (armed, merkle) in [(false, true), (true, true), (true, false)] {
        let mut params = config.params.clone();
        if !merkle {
            params.palw_prompt_ids_merkle = None;
            let PalwConsensusMode::ConsensusV2(bundle) = &mut params.palw_consensus_mode else { unreachable!() };
            bundle.trace_format_version = 3; // coherent Flat genesis form, solely for this gate fixture
        }
        if armed {
            params.palw_legacy_held_da_v2 = Some(ForkActivation::new(0));
            assert!(params.validate_palw_v2().is_err(), "the real validation still refuses the fence; only this harness bypasses it");
        }
        params.skip_proof_of_work = true;
        let config = Config::new(params);
        let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!() };
        let chain = t12_genesis_chain(&config, bundle, &premine, &floats);
        let vp = chain.vp();
        let (block, state) = chain.tip_state();
        let point = PalwBlockContextV2 { block, daa_score: chain.daa_of(block), blue_score: 1, subsidy: 0 };
        let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            config.params.net.to_string().as_bytes(),
            Some(config.params.genesis.hash),
        );
        let signer = |card: usize| {
            let key = TestConsensus::palw_v2_registry_keypair(card as u64);
            move |message: &[u8], context: &[u8]| {
                libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, message, context, [0x62u8; 32]).expect("signs").as_ref().to_vec()
            }
        };
        let claim = Hash64::from_bytes([0x5C; 64]);
        let binding = a_binding();
        let unit = PalwLegacyHeldUnitV2::StepNode { level: 3, index: 0 };
        let demand = |claim: Hash64, accuser: usize, by: usize| {
            let mut d = PalwLegacyHeldDemandV2 {
                version: PALW_LEGACY_HELD_VERSION_V2,
                claim,
                unit,
                accuser: chain.bonds[accuser],
                binding: binding.clone(),
                signature: Vec::new(),
            };
            let message = palw_legacy_held_demand_message_v2(domain.as_byte_slice(), &d);
            d.signature = signer(by)(message.as_byte_slice(), PALW_LEGACY_HELD_DEMAND_MLDSA87_CONTEXT_V2);
            Obj::LegacyHeldDemandedV2 { demand: Box::new(d) }
        };
        let answer = |claim: Hash64, discloser: usize, by: usize| {
            let mut a = PalwLegacyHeldAnswerCarriageV2 {
                version: PALW_LEGACY_HELD_VERSION_V2,
                claim,
                unit,
                binding: binding.clone(),
                answer: PalwLegacyHeldAnswerV2::Node { frontier: vec![Hash64::from_u64_word(1)], siblings: Vec::new() },
                discloser: chain.bonds[discloser],
                signature: Vec::new(),
            };
            let message = palw_legacy_held_answer_message_v2(domain.as_byte_slice(), &a);
            a.signature = signer(by)(message.as_byte_slice(), PALW_LEGACY_HELD_ANSWER_MLDSA87_CONTEXT_V2);
            Obj::LegacyHeldAnsweredV2 { answer: Box::new(a) }
        };
        let recompute = |claim: Hash64, accuser: usize, by: usize| {
            let mut r = PalwLegacyLeafRecomputeV2 {
                version: PALW_LEGACY_HELD_VERSION_V2,
                claim,
                execution_root: binding.committed_execution_root,
                trace_root: binding.full_logits_trace_root,
                executor_bond: chain.bonds[1],
                accuser_bond: chain.bonds[accuser],
                leaf_index: 0,
                refutation: PalwExecutionStepRefutationV1 {
                    binding: binding.clone(),
                    output_opening: PalwStepOpeningV1 { leaf_index: 0, leaf_hash: Hash64::from_u64_word(9), siblings: Vec::new() },
                    output_preimage: palw_legacy_recompute_placeholder_v2(),
                    inputs: Vec::new(),
                    prompt_token_ids: Vec::new(),
                    decode_tokens: None,
                    kv_checkpoint: None,
                },
                artifact_openings: Vec::new(),
                prompt_ids_opening: None,
                signature: Vec::new(),
            };
            let message = palw_legacy_leaf_recompute_message_v2(domain.as_byte_slice(), &r);
            r.signature = signer(by)(message.as_byte_slice(), PALW_LEGACY_LEAF_RECOMPUTE_MLDSA87_CONTEXT_V2);
            Obj::LegacyLeafRecomputedV2 { accusation: Box::new(r) }
        };
        let gate = |object: &Obj| vp.palw_v2_validate_objects(&state, &bundle.state, &point, std::slice::from_ref(object));
        let walk = |objects: Vec<Obj>| vp.palw_v2_accepted_objects_for_tests(&state, &bundle.state, &point, objects, block);
        let input_demand = |tokens: u32| {
            let Obj::LegacyHeldDemandedV2 { mut demand } = demand(claim, 2, 2) else { unreachable!() };
            demand.unit = PalwLegacyHeldUnitV2::PromptIds { chunk: 0 };
            demand.binding.job_context.declared_prefill_tokens = tokens;
            let message = palw_legacy_held_demand_message_v2(domain.as_byte_slice(), &demand);
            demand.signature = signer(2)(message.as_byte_slice(), PALW_LEGACY_HELD_DEMAND_MLDSA87_CONTEXT_V2);
            Obj::LegacyHeldDemandedV2 { demand }
        };
        let Obj::LegacyHeldAnsweredV2 { answer: mut input_answer } = answer(claim, 3, 3) else { unreachable!() };
        input_answer.unit = PalwLegacyHeldUnitV2::PromptIds { chunk: 0 };
        input_answer.answer = PalwLegacyHeldAnswerV2::PromptIds { ids: vec![0; 4], tree_root: None, siblings: vec![] };
        let message = palw_legacy_held_answer_message_v2(domain.as_byte_slice(), &input_answer);
        input_answer.signature = signer(3)(message.as_byte_slice(), PALW_LEGACY_HELD_ANSWER_MLDSA87_CONTEXT_V2);
        let signed = [
            demand(claim, 2, 2),
            answer(claim, 3, 3),
            recompute(claim, 2, 2),
            input_demand(4),
            Obj::LegacyHeldAnsweredV2 { answer: input_answer },
        ];
        // An int-12 answer (tag 55) that names the appended unit rides the same rule.
        let carrying = Obj::MaterialDisclosedV2 {
            claim,
            unit: PalwDaUnitV1::LegacyHeldV2(unit),
            answer: PalwDaAnswerV1::Event(kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1::OutOfRange {
                binding: Box::new(binding.clone()),
            }),
            discloser: chain.bonds[3],
            signature: vec![1; 8],
        };
        if !armed {
            for object in signed.iter().chain(std::iter::once(&carrying)) {
                assert!(walk(vec![object.clone()]).is_empty(), "dropped by name below the fence: {object:?}");
            }
            for object in &signed {
                let refused = gate(object).expect_err("below the fence");
                assert!(refused.contains("palw_legacy_held_da_v2 is not in force"), "{refused}");
            }
            // Byte identity of the walk: a block carrying them walks to what a block without them walks to.
            assert_eq!(walk(signed.to_vec()), walk(Vec::new()));
            continue;
        }
        for object in &signed {
            gate(object).unwrap_or_else(|e| panic!("signed by the bond it names, past the fence: {e}"));
        }
        if !merkle {
            for tokens in [100_000, u32::MAX] {
                assert!(
                    gate(&input_demand(tokens)).unwrap_err().contains("input response and binding cannot fit"),
                    "a Flat unit beyond the single-carrier or court ceiling is refused before opening a DA session"
                );
            }
        } else {
            gate(&input_demand(100_000)).expect("Merkle answers stay bounded regardless of the total input count");
        }
        let other = Hash64::from_bytes([0x5D; 64]);
        let tamper = |object: &Obj, f: &dyn Fn(&mut Obj)| {
            let mut o = object.clone();
            f(&mut o);
            o
        };
        for (what, object) in [
            (
                "the demand: another claim",
                tamper(&signed[0], &|o| {
                    if let Obj::LegacyHeldDemandedV2 { demand } = o {
                        demand.claim = other
                    }
                }),
            ),
            (
                "the demand: another accuser",
                tamper(&signed[0], &|o| {
                    if let Obj::LegacyHeldDemandedV2 { demand } = o {
                        demand.accuser = chain.bonds[4]
                    }
                }),
            ),
            (
                "the demand: no signature",
                tamper(&signed[0], &|o| {
                    if let Obj::LegacyHeldDemandedV2 { demand } = o {
                        demand.signature.clear()
                    }
                }),
            ),
            ("the demand: another card's key", demand(claim, 2, 5)),
            (
                "the answer: another unit",
                tamper(&signed[1], &|o| {
                    if let Obj::LegacyHeldAnsweredV2 { answer } = o {
                        answer.unit = PalwLegacyHeldUnitV2::StepNode { level: 3, index: 1 }
                    }
                }),
            ),
            ("the answer: another discloser", answer(claim, 4, 3)),
            (
                "the recompute: another leaf",
                tamper(&signed[2], &|o| {
                    if let Obj::LegacyLeafRecomputedV2 { accusation } = o {
                        accusation.leaf_index = 1
                    }
                }),
            ),
            ("the recompute: another card's key", recompute(claim, 2, 6)),
        ] {
            assert!(gate(&object).is_err(), "{what} is refused at the gate");
        }
    }
}
