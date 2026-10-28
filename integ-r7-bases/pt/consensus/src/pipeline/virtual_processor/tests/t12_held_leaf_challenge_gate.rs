//! **The held leaf challenge at the processor's gate** (RFC-0003 decision 22, `HeldLeafChallengeDeclared`, tag 90), on
//! testnet-12 as launched with harness keys on the eight cards: dropped by name below `palw_held_close_chunks_v1`
//! (the object is a payload an older build cannot decode, so the acceptance walk drops it and the block stands);
//! past it, admitted only when the accuser's registered key signed the challenge's digest over the network, every
//! field of it, and when its shape is the court's (one to 32 chunks, one digest per chunk, an executor that is not
//! the accuser). The claim, its class, the leaf's bound, the open sessions and the accuser's standing are the
//! fold's (`palw_held_close_chunks`, `palw_held_close_fold_v1`), where the state is in hand.
use super::t12_round_lane_e2e::{t12_genesis_chain, t12_with_harness_cards};
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::config::ConfigBuilder;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_held_close_v1::{
    PALW_HELD_LEAF_CHALLENGE_MLDSA87_CONTEXT_V1, PALW_HELD_LEAF_CHALLENGE_VERSION_V1, PalwHeldLeafChallengeV1,
    palw_held_leaf_challenge_digest_v1,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_producer_v2::PalwObjectRehearsalV1 as Rehearsal;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwConsensusObjectV2 as Obj, PalwStateV2Error, palw_court_close_chunk_digest_v1,
};
use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;
use kaspa_hashes::Hash64;

#[tokio::test]
async fn t12_a_held_leaf_challenge_is_signed_by_its_accuser_and_dropped_below_its_fence() {
    let (config, _, premine, floats) = t12_with_harness_cards();
    let armed = |fence: bool| {
        let mut params = config.params.clone();
        params.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(0)));
        params.sync_palw_tir_v1();
        if fence {
            params.palw_held_close_chunks_v1 = Some(ForkActivation::new(0));
            params.sync_palw_held_close_chunks_v1();
        }
        let c = ConfigBuilder::new(params).skip_proof_of_work().build();
        c.params.validate_palw_v2().expect("a runnable ruleset");
        c
    };
    for fence in [false, true] {
        let config = armed(fence);
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
        let sign_as = |card: usize, message: &[u8]| {
            let key = TestConsensus::palw_v2_registry_keypair(card as u64);
            libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, message, PALW_HELD_LEAF_CHALLENGE_MLDSA87_CONTEXT_V1, [0x62u8; 32])
                .ok()
                .map(|s| s.as_ref().to_vec())
                .expect("a signature")
        };
        // A challenge of two chunks, signed by `signer` over every field of it.
        let make = |f: &dyn Fn(&mut PalwHeldLeafChallengeV1), signer: usize| {
            let chunks = [vec![1u8; 10], vec![2u8; 10]];
            let mut c = PalwHeldLeafChallengeV1 {
                version: PALW_HELD_LEAF_CHALLENGE_VERSION_V1,
                claim: Hash64::from_bytes([0x5C; 64]),
                execution_root: Hash64::from_bytes([0x5D; 64]),
                trace_root: Hash64::from_bytes([0x5E; 64]),
                executor_bond: chain.bonds[card + 1],
                accuser_bond: chain.bonds[card],
                leaf_index: 7,
                count: 2,
                chunk_digests: chunks.iter().map(|b| palw_court_close_chunk_digest_v1(b)).collect(),
                close_digest: palw_court_close_chunk_digest_v1(&[chunks[0].clone(), chunks[1].clone()].concat()),
                signature: Vec::new(),
            };
            f(&mut c);
            c.signature = sign_as(signer, palw_held_leaf_challenge_digest_v1(domain.as_byte_slice(), &c).as_byte_slice());
            Obj::HeldLeafChallengeDeclared { challenge: Box::new(c) }
        };
        let gate = |object: &Obj| vp.palw_v2_validate_objects(&state, &bundle.state, &point, std::slice::from_ref(object));
        let signed = make(&|_| {}, card);
        if !fence {
            let refused = gate(&signed).expect_err("below the fence");
            assert!(refused.contains("palw_held_close_chunks_v1 is not in force"), "{refused}");
            assert!(vp.palw_v2_accepted_objects_for_tests(&state, &bundle.state, &point, vec![signed], block).is_empty(), "dropped");
            continue;
        }
        gate(&signed).expect("signed by its accuser, past the fence");
        // The gate takes it. What the walk then does is the FOLD's: the challenge names a claim this chain does not
        // have, so the rehearsal reaches the fold's own arm and is refused there, typed `MissingClaim` — not at the
        // gate, not for a fence, not for a signature — and the walk, which drops what the fold refuses, drops it.
        match vp.palw_object_rehearsal_v1_on(&state, &bundle.state, &point, &signed) {
            Rehearsal::Refused(PalwStateV2Error::MissingClaim(claim)) => assert_eq!(claim, Hash64::from_bytes([0x5C; 64])),
            other => panic!("the fold's refusal of an absent claim, not {other:?}"),
        }
        assert!(
            vp.palw_v2_accepted_objects_for_tests(&state, &bundle.state, &point, vec![signed.clone()], block).is_empty(),
            "the walk drops what the fold refuses"
        );
        for (what, object) in [
            // A field the signature covers, moved after signing: the digest no longer verifies.
            ("another leaf", {
                let Obj::HeldLeafChallengeDeclared { challenge } = &signed else { unreachable!() };
                let mut c = challenge.as_ref().clone();
                c.leaf_index += 1;
                Obj::HeldLeafChallengeDeclared { challenge: Box::new(c) }
            }),
            // Signed by a bond that is not the one it names.
            ("another signer", make(&|_| {}, 3)),
            (
                "an accuser the chain does not have",
                make(
                    &|c| {
                        c.accuser_bond =
                            kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
                                kaspa_consensus_core::tx::TransactionId::from_bytes([0xEE; 64]),
                                9,
                            ))
                    },
                    card,
                ),
            ),
            ("no signature", {
                let Obj::HeldLeafChallengeDeclared { challenge } = &signed else { unreachable!() };
                let mut c = challenge.as_ref().clone();
                c.signature.clear();
                Obj::HeldLeafChallengeDeclared { challenge: Box::new(c) }
            }),
            ("no chunk", make(&|c| c.count = 0, card)),
            ("past the court's chunks", make(&|c| c.count = 33, card)),
            (
                "one digest short",
                make(
                    &|c| {
                        c.chunk_digests.pop();
                    },
                    card,
                ),
            ),
            ("an executor that is its own accuser", make(&|c| c.executor_bond = c.accuser_bond, card)),
        ] {
            assert!(gate(&object).is_err(), "{what} is refused at the gate");
        }
    }
}
