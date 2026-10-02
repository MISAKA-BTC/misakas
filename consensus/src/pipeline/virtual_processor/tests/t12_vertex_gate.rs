//! **The verification vertex at the processor's gate** (RFC-0007 Part I, `VerificationVertexV1` and `VertexEquivocationV1`, tags 91
//! and 92), on testnet-12 as launched with harness keys on the eight cards: dropped by name below `palw_verification_vertex_v1` (a
//! payload an older build cannot decode, so the acceptance walk drops it and the block stands); past it, admitted only when its shape
//! is the vertex's (strictly ascending leaves, the caps, a root that recomputes), its clock is one a vertex may have, its seat is
//! registered, it is the first of its `(seat, round)` on the chain, and **its one ML-DSA-87 signature verifies under the seat bond's
//! registered key**. An equivocation is admitted only with BOTH signatures valid under the seat's key. What a leaf does is the fold's
//! (`palw_vertex_fold_v1`), where the state is in hand — and a leaf that does not count is ignored, never refused.
use super::t12_round_lane_e2e::{t12_genesis_chain, t12_with_harness_cards};
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::config::ConfigBuilder;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_panel_v2::PalwReceiptVerdictV2;
use kaspa_consensus_core::palw_state_v2::{PalwBlockContextV2, PalwConsensusObjectV2 as Obj};
use kaspa_consensus_core::palw_vertex_v1::{
    PALW_VERTEX_MLDSA87_CONTEXT_V1, PalwClaimRefV1, PalwVerificationVertexV1, PalwVertexEquivocationV1, PalwVertexLeafV1,
};
use kaspa_hashes::Hash64;

#[tokio::test]
async fn t12_a_vertex_is_signed_by_its_seat_and_dropped_below_its_fence() {
    let (config, _, premine, floats) = t12_with_harness_cards();
    let armed = |fence: bool| {
        let mut params = config.params.clone();
        if fence {
            params.palw_verification_vertex_v1 = Some(ForkActivation::new(0));
            params.sync_palw_verification_vertex_v1();
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
        let daa = chain.daa_of(block);
        let point = PalwBlockContextV2 { block, daa_score: daa, blue_score: 1, subsidy: 0 };
        let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            config.params.net.to_string().as_bytes(),
            Some(config.params.genesis.hash),
        );
        let card = 2usize;
        let sign_as = |card: usize, message: &[u8], context: &[u8]| {
            let key = TestConsensus::palw_v2_registry_keypair(card as u64);
            libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, message, context, [0x71u8; 32]).ok().map(|s| s.as_ref().to_vec())
        };
        let leaves = |claims: &[u8]| -> Vec<PalwVertexLeafV1> {
            claims
                .iter()
                .map(|c| PalwVertexLeafV1::Verdict {
                    claim: PalwClaimRefV1::Full(Hash64::from_bytes([*c; 64])),
                    verdict: PalwReceiptVerdictV2::Valid,
                })
                .collect()
        };
        // A vertex of `card`'s seat at `signed_daa`, signed by `signer`'s key.
        let make = |seat_card: usize, signer: usize, signed_daa: u64, claims: &[u8]| {
            PalwVerificationVertexV1::sign_v1(domain, chain.bonds[seat_card], signed_daa, leaves(claims), |message, context| {
                assert_eq!(context, PALW_VERTEX_MLDSA87_CONTEXT_V1);
                sign_as(signer, message, context)
            })
            .expect("a well-formed vertex signs")
        };
        let object = |v: PalwVerificationVertexV1| Obj::VerificationVertexV1 { vertex: Box::new(v) };
        let gate = |object: &Obj| vp.palw_v2_validate_objects(&state, &bundle.state, &point, std::slice::from_ref(object));
        let good = make(card, card, daa, &[1, 2, 3]);
        if !fence {
            let refused = gate(&object(good.clone())).expect_err("below the fence");
            assert!(refused.contains("below palw_verification_vertex_v1"), "{refused}");
            assert!(
                vp.palw_v2_accepted_objects_for_tests(&state, &bundle.state, &point, vec![object(good)], block).is_empty(),
                "dropped"
            );
            continue;
        }
        // Past the fence: signed by its seat, the gate takes it, and the walk accepts it — its leaves name claims this chain does not
        // have, which the fold ignores: the vertex still stands as the seat's round.
        gate(&object(good.clone())).expect("signed by its seat, past the fence");
        assert_eq!(
            vp.palw_v2_accepted_objects_for_tests(&state, &bundle.state, &point, vec![object(good.clone())], block).len(),
            1,
            "accepted"
        );
        // One vertex per seat per round: the second of the same round is dropped, the first stands.
        let second = make(card, card, daa, &[4, 5]);
        assert_ne!(good.leaves_root, second.leaves_root);
        let walked =
            vp.palw_v2_accepted_objects_for_tests(&state, &bundle.state, &point, vec![object(good.clone()), object(second)], block);
        assert_eq!(walked.len(), 1, "one vertex a round: the second is dropped, the block stands");
        // Hostile vertices, each refused at the gate by its own reason.
        let mut tampered = good.clone();
        tampered.leaves.pop();
        let other_seat = make(card + 1, card, daa, &[1, 2, 3]);
        let mut no_signature = good.clone();
        no_signature.signature.clear();
        let mut bad_root = good.clone();
        bad_root.leaves_root = Hash64::from_bytes([0xEE; 64]);
        let unknown_bond = {
            let mut v = good.clone();
            v.seat_bond = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
                kaspa_consensus_core::tx::TransactionId::from_bytes([0xEE; 64]),
                9,
            ));
            v
        };
        for (what, why, v) in [
            ("a truncated vertex (a leaf removed after signing)", "root does not recompute", tampered),
            ("a vertex of another seat, signed by this seat's key", "does not verify", other_seat),
            ("a vertex with no signature", "signature is 0 bytes", no_signature),
            ("a root that does not recompute", "root does not recompute", bad_root),
            ("a bond the chain does not have", "not registered", unknown_bond),
            ("a vertex signed after the block that carries it", "signed after the block", make(card, card, daa + 1, &[1])),
        ] {
            let refused = gate(&object(v)).expect_err(what);
            assert!(refused.contains(why), "{what}: {refused}");
        }
        // The clock: a vertex older than a vertex may wait is refused (only when the chain's DAA allows one).
        if daa > kaspa_consensus_core::palw_vertex_v1::PALW_VERTEX_MAX_CARRY_DAA_V1 {
            let old = make(card, card, daa - kaspa_consensus_core::palw_vertex_v1::PALW_VERTEX_MAX_CARRY_DAA_V1 - 1, &[1]);
            assert!(gate(&object(old)).expect_err("too old").contains("older than"));
        }

        // **Equivocation**: two vertices of one round with different roots, both signed by the seat. Admitted at the gate with both
        // signatures valid; refused with one of them forged, with one vertex twice, and for another seat's pair signed by this key.
        let a = good.clone();
        let b = make(card, card, daa, &[7, 8]);
        let evidence = |a: &PalwVerificationVertexV1, b: &PalwVerificationVertexV1| Obj::VertexEquivocationV1 {
            evidence: Box::new(PalwVertexEquivocationV1 {
                a: a.header(),
                b: b.header(),
                a_leaves: a.leaves.clone(),
                b_leaves: b.leaves.clone(),
            }),
        };
        gate(&evidence(&a, &b)).expect("two signed vertices of one round are an equivocation");
        let mut forged = b.clone();
        forged.signature = sign_as(card + 2, b"not the vertex message", PALW_VERTEX_MLDSA87_CONTEXT_V1).unwrap();
        // The forged signature is the wrong length for ML-DSA-87? No: it is a real signature over another message, so it parses and fails.
        let refused = gate(&evidence(&a, &forged)).expect_err("one forged signature");
        assert!(refused.contains("does not verify"), "{refused}");
        assert!(gate(&evidence(&a, &a)).expect_err("one vertex twice").contains("same leaf root"));
        // The evidence is walked and accepted; the fold then convicts the seat (the fold's tests hold the slash).
        assert_eq!(vp.palw_v2_accepted_objects_for_tests(&state, &bundle.state, &point, vec![evidence(&a, &b)], block).len(), 1);
    }
}
