//! **The audit mesh's objects at the processor's gate** (RFC-0007 Part IV.1, `TrapCommittedV1` and `TrapRevealedV1`, tags 93 and 94; and an
//! `Audited` leaf of a vertex), on testnet-12 as launched with harness keys on the eight cards and the vertex and the mesh armed:
//!
//! * **dropped by name below `palw_audit_mesh_v1`** (a payload an older build cannot decode: the acceptance walk drops it and the block stands;
//!   the gate refuses it as its second lock), and an audit leaf is refused by name below the fence even when the vertex fence is armed;
//! * **past it**, a commitment is admitted only from a registered Active bond **drawn by the slot lottery** with the deposit free and no trap
//!   open, **and its one ML-DSA-87 signature verifies under the setter bond's registered key**; a reveal opens a commitment of its setter
//!   about its own audited claim inside the reveal window, and verifies the same way.
//!
//! What a leaf or a trap does once admitted is the fold's (`palw_state_v2::tests::mesh_fold_v1`); this file is the gate's.
use super::t12_round_lane_e2e::{t12_genesis_chain, t12_with_harness_cards};
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::config::ConfigBuilder;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_mesh_v1::{
    PALW_MESH_TRAP_COMMITTED_MLDSA87_CONTEXT_V1, PALW_MESH_TRAP_REVEALED_MLDSA87_CONTEXT_V1, PALW_TRAP_SLOT_DAA_V1, PalwTrapCommittedV1,
    PalwTrapRevealedV1, palw_trap_slot_drawn_v1,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_panel_v2::PalwReceiptVerdictV2;
use kaspa_consensus_core::palw_state_v2::{PalwBlockContextV2, PalwConsensusObjectV2 as Obj, palw_object_is_mesh_v1};
use kaspa_consensus_core::palw_vertex_v1::{PalwClaimRefV1, PalwVerificationVertexV1, PalwVertexLeafV1};
use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;
use kaspa_hashes::Hash64;

#[tokio::test]
async fn t12_a_trap_is_signed_by_its_setter_drawn_by_the_lottery_and_dropped_below_its_fence() {
    let (config, _, premine, floats) = t12_with_harness_cards();
    let armed = |mesh: bool| {
        let mut params = config.params.clone();
        params.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(0)));
        params.sync_palw_tir_v1();
        params.palw_verification_vertex_v1 = Some(ForkActivation::new(0));
        params.sync_palw_verification_vertex_v1();
        if mesh {
            params.palw_audit_mesh_v1 = Some(ForkActivation::new(0));
            params.sync_palw_audit_mesh_v1();
        }
        let c = ConfigBuilder::new(params).skip_proof_of_work().build();
        c.params.validate_palw_v2().expect("a runnable ruleset");
        c
    };
    for mesh in [false, true] {
        let config = armed(mesh);
        let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!() };
        let chain = t12_genesis_chain(&config, bundle, &premine, &floats);
        let vp = chain.vp();
        let (block, state) = chain.tip_state();
        let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            config.params.net.to_string().as_bytes(),
            Some(config.params.genesis.hash),
        );
        let card = 2usize;
        let setter = chain.bonds[card];
        // A DAA inside a slot in which this setter is drawn, and one inside a slot where it is not.
        let drawn = (1..).map(|slot| slot * PALW_TRAP_SLOT_DAA_V1 + 1).find(|daa| palw_trap_slot_drawn_v1(&setter, *daa)).unwrap();
        let undrawn = (1..).map(|slot| slot * PALW_TRAP_SLOT_DAA_V1 + 1).find(|daa| !palw_trap_slot_drawn_v1(&setter, *daa)).unwrap();
        let at = |daa: u64| PalwBlockContextV2 { block, daa_score: daa, blue_score: 1, subsidy: 0 };
        let sign_as = |card: usize, message: &[u8], context: &[u8]| {
            let key = TestConsensus::palw_v2_registry_keypair(card as u64);
            libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, message, context, [0x71u8; 32]).ok().map(|s| s.as_ref().to_vec())
        };
        let claim = Hash64::from_bytes([0xC1; 64]);
        let salt = [9u8; 32];
        let commit = |signer: usize, bond| {
            Obj::TrapCommittedV1 {
                trap: Box::new(
                    PalwTrapCommittedV1::sign_v1(domain, bond, &claim, 0, 4, &salt, |m, c| {
                        assert_eq!(c, PALW_MESH_TRAP_COMMITTED_MLDSA87_CONTEXT_V1);
                        sign_as(signer, m, c)
                    })
                    .unwrap(),
                ),
            }
        };
        let gate = |object: &Obj, daa: u64| vp.palw_v2_validate_objects(&state, &bundle.state, &at(daa), std::slice::from_ref(object));
        let good = commit(card, setter);
        assert!(palw_object_is_mesh_v1(&good));
        if !mesh {
            let refused = gate(&good, drawn).expect_err("below the fence");
            assert!(refused.contains("palw_audit_mesh_v1"), "{refused}");
            assert!(
                vp.palw_v2_accepted_objects_for_tests(&state, &bundle.state, &at(drawn), vec![good], block).is_empty(),
                "dropped, and the block stands"
            );
            // An audit leaf of a vertex is refused by name below the mesh fence, though the vertex fence is armed.
            let leaf = PalwVertexLeafV1::Audited { claim: PalwClaimRefV1::Full(claim), leaf: 1, result: 0 };
            let vertex = PalwVerificationVertexV1::sign_v1(domain, chain.bonds[card], 0, vec![leaf], |m, c| sign_as(card, m, c)).unwrap();
            let refused = gate(&Obj::VerificationVertexV1 { vertex: Box::new(vertex) }, 5).expect_err("an audit leaf below the mesh fence");
            assert!(refused.contains("audit mesh") && refused.contains("not armed"), "{refused}");
            continue;
        }
        // Past the fence: signed by its setter, drawn, deposit free: the gate takes it, and the walk accepts it.
        gate(&good, drawn).unwrap_or_else(|e| panic!("a drawn setter's signed commitment: {e}"));
        assert_eq!(vp.palw_v2_accepted_objects_for_tests(&state, &bundle.state, &at(drawn), vec![good.clone()], block).len(), 1);
        // Hostile commitments, each refused at the gate by its own reason.
        let refused = |object: Obj, daa: u64, why: &str| {
            let e = gate(&object, daa).expect_err(why);
            assert!(e.contains(why), "{why}: {e}");
        };
        refused(good.clone(), undrawn, "not drawn to set a trap");
        refused(commit(card + 1, setter), drawn, "does not verify"); // another key's signature
        refused(commit(card, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
            kaspa_consensus_core::tx::TransactionId::from_bytes([0xEE; 64]),
            9,
        ))), drawn, "not registered");
        let mut no_signature = commit(card, setter);
        if let Obj::TrapCommittedV1 { trap } = &mut no_signature {
            trap.signature.clear();
        }
        refused(no_signature, drawn, "signature is 0 bytes");
        // A reveal opens nothing the chain holds; and a reveal with an impossible tile is malformed.
        let reveal = |fault: u64, tiles: u64| Obj::TrapRevealedV1 {
            reveal: Box::new(
                PalwTrapRevealedV1::sign_v1(domain, setter, claim, fault, tiles, salt, |m, c| {
                    assert_eq!(c, PALW_MESH_TRAP_REVEALED_MLDSA87_CONTEXT_V1);
                    sign_as(card, m, c)
                })
                .unwrap(),
            ),
        };
        refused(reveal(0, 4), drawn + 300, "opens no commitment");
        refused(reveal(9, 4), drawn + 300, "tiles");
        // A vertex carrying an audit leaf past the fence passes the gate (what the leaf does is the fold's: no row, no effect).
        let leaf = PalwVertexLeafV1::Audited { claim: PalwClaimRefV1::Full(claim), leaf: 1, result: 0 };
        let vertex = PalwVerificationVertexV1::sign_v1(domain, chain.bonds[card], 0, vec![leaf], |m, c| sign_as(card, m, c)).unwrap();
        gate(&Obj::VerificationVertexV1 { vertex: Box::new(vertex) }, 5).expect("an audit leaf rides a vertex past the fence");
        let _ = PalwReceiptVerdictV2::Valid;
    }
}
