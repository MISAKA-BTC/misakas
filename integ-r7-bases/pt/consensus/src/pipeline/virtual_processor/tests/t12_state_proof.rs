//! **RFC-0009 stage D on a real testnet-12 chain: the node proves the PALW state its header commits, and a client that only pinned the
//! block checks it.**
//!
//! `palw_state_proof_v1` (the consensus API behind `getPalwStateProof`, op 202) hands out the header of a block and the state that header
//! commits — the state as-of the block's SELECTED PARENT. The client's whole trust is the block hash it pinned; this drives the real
//! processor and the client-side verifier together, then breaks each link:
//!
//! * the proof opens against the pinned block's committed root, and a genesis bond is proven present with the key the card registered;
//! * absence of a bond is proven too (the whole table is committed);
//! * a header that is not the pinned block, a tampered preimage, a forged row set and a wrong collection are each refused;
//! * a block the node cannot prove (unknown, or a collection that does not exist) is an error naming why, not a guess.

use super::t12_round_lane_e2e::{t12_genesis_chain, t12_with_harness_cards};
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::palw_state_proof_v1::{PalwProofErrorV1, state_root_of_pinned_header_v1, verify_bond_v1, verify_class_v1};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwBondStatusV2};
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;

#[tokio::test]
async fn the_node_proves_the_state_its_header_commits_to_a_client_that_pinned_only_the_block() {
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    let mut chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let ttpb = config.params.target_time_per_block();
    for _ in 0..4 {
        chain.heartbeat(ttpb, Vec::new()).await;
    }
    let pinned = chain.sink();

    // The node's answer for the block the client pinned.
    let (header, bonds) = chain.ctx.consensus.palw_state_proof_v1(pinned, b"bonds").expect("the node proves its sink");
    // CLIENT SIDE from here: nothing but `pinned`, the header and the proof.
    let root = state_root_of_pinned_header_v1(&header, pinned).expect("the header is the pinned block and commits a state");
    assert_ne!(root, Hash64::default());
    // The eight genesis cards are bonds the proof opens, each with the key it registered.
    let (_, tip_state) = chain.tip_state();
    let expected: Vec<(PalwBondKeyV2, Vec<u8>)> = chain.bonds.iter().map(|b| (*b, Vec::new())).collect();
    assert_eq!(expected.len(), 8);
    for (bond, _) in &expected {
        let proven = verify_bond_v1(&bonds, root, bond).expect("a genesis bond is proven present");
        let held = tip_state.bond(bond).expect("the chain holds the bond");
        assert_eq!(proven.pubkey, held.pubkey, "the proven key is the registered key");
        assert!(!matches!(proven.status, PalwBondStatusV2::Retiring { .. }), "an Active genesis bond");
    }
    // Absence is proven: the whole table is committed.
    let stranger = PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_bytes([0xEE; 64]), 9));
    assert_eq!(verify_bond_v1(&bonds, root, &stranger), Err(PalwProofErrorV1::Absent));
    // The class table, and the base class in it, from the same committed root.
    let (class_header, classes) = chain.ctx.consensus.palw_state_proof_v1(pinned, b"classes").expect("classes");
    assert_eq!(class_header.palw_state_root, header.palw_state_root, "one block, one committed root");
    let base = config.params.palw_consensus_mode.clone();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(params) = base else { panic!("testnet-12 is ConsensusV2") };
    let base_class = verify_class_v1(&classes, root, &params.base_class_id).expect("the base class is proven present");
    assert_eq!(base_class.registrant_bond, None, "a genesis class has no registrant: proven, not reported");
    // The claims table is empty on this chain: provable as such.
    let (_, claims) = chain.ctx.consensus.palw_state_proof_v1(pinned, b"claims").expect("claims");
    assert!(claims.collection.rows.is_empty());
    assert_eq!(
        kaspa_consensus_core::palw_state_proof_v1::verify_claim_v1(&claims, root, &Hash64::from_bytes([1; 64])),
        Err(PalwProofErrorV1::Absent)
    );

    // Each link broken.
    // 1. Not the block the client pinned.
    assert_eq!(state_root_of_pinned_header_v1(&header, Hash64::from_bytes([7; 64])), Err(PalwProofErrorV1::HeaderIsNotThePinnedBlock));
    // 2. A header whose committed root was swapped no longer hashes to the pin.
    let mut swapped = header.clone();
    swapped.palw_state_root = Hash64::from_bytes([8; 64]);
    assert_eq!(state_root_of_pinned_header_v1(&swapped, pinned), Err(PalwProofErrorV1::HeaderIsNotThePinnedBlock));
    // 3. A tampered preimage does not hash to the root.
    let mut tampered = bonds.clone();
    tampered.state.preimage[5] ^= 1;
    assert!(matches!(verify_bond_v1(&tampered, root, &chain.bonds[0]), Err(PalwProofErrorV1::OpeningDoesNotMatchRoot(_))));
    // 4. A forged row (a stranger's bond added) is not part of the committed collection.
    let mut forged = bonds.clone();
    forged.collection.rows.push((borsh::to_vec(&stranger).unwrap(), vec![0; 10]));
    assert_eq!(verify_bond_v1(&forged, root, &stranger), Err(PalwProofErrorV1::CollectionNotInState));
    // 5. A bonds proof is not a classes proof.
    assert!(matches!(verify_class_v1(&bonds, root, &params.base_class_id), Err(PalwProofErrorV1::WrongCollection { .. })));

    // The node says why it cannot, instead of guessing.
    let unknown = chain.ctx.consensus.palw_state_proof_v1(Hash64::from_bytes([0x55; 64]), b"bonds").unwrap_err();
    assert!(unknown.contains("no header"), "{unknown}");
    let wrong = chain.ctx.consensus.palw_state_proof_v1(pinned, b"fences").unwrap_err();
    assert!(wrong.contains("no such collection"), "{wrong}");
}
