//! Stage D, client side: believe a bond only if it is proven against a block the miner pinned.
//!
//! The consensus-core module `palw_state_proof_v1` does the cryptography (header → state root → collection → row). This is the policy a miner
//! applies on top of it: the proven bond is the one it holds a key for, and it may produce (not retiring). A node supplies the header bytes and the
//! proof; it cannot make either say something the pinned block does not.

use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::palw_state_proof_v1::{PalwFactProofV1, PalwProofErrorV1, state_root_of_pinned_header_v1, verify_bond_v1};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwBondStateV2, PalwBondStatusV2};
use kaspa_hashes::Hash64;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BondProofRefusal {
    #[error("the proof does not hold: {0}")]
    Proof(#[from] PalwProofErrorV1),
    #[error("the proven bond's registered key is not the key this miner holds")]
    NotOurKey,
    #[error("the proven bond is retiring and may take no new claims")]
    Retiring,
}

/// The bond as the pinned block's committed state holds it — or a named refusal. No node's word is read.
pub fn verify_bond_against_pin(
    header: &Header,
    pinned_block_hash: Hash64,
    proof: &PalwFactProofV1,
    bond: &PalwBondKeyV2,
    held_pubkey: &[u8],
) -> Result<PalwBondStateV2, BondProofRefusal> {
    let root = state_root_of_pinned_header_v1(header, pinned_block_hash)?;
    let state = verify_bond_v1(proof, root, bond)?;
    if state.pubkey != held_pubkey {
        return Err(BondProofRefusal::NotOurKey);
    }
    if matches!(state.status, PalwBondStatusV2::Retiring { .. }) {
        return Err(BondProofRefusal::Retiring);
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_state_proof_v1::prove_bonds_v1;
    use kaspa_consensus_core::palw_state_v2::PalwChainStateV2;
    use kaspa_consensus_core::tx::TransactionOutpoint;

    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }

    fn pinned(state: &PalwChainStateV2) -> Header {
        let mut header = Header::new_finalized(
            1,
            vec![vec![h(1)]].try_into().unwrap(),
            h(2),
            h(3),
            h(4),
            1,
            0x1d00ffff,
            0,
            kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2,
            5,
            0u64.into(),
            0,
            h(5),
        )
        .with_palw_state_root(state.state_root());
        header.finalize();
        header
    }

    #[test]
    fn a_bond_the_pinned_state_does_not_hold_is_a_proven_absence_not_a_node_s_say_so() {
        let state = PalwChainStateV2::genesis();
        let header = pinned(&state);
        let bond = PalwBondKeyV2(TransactionOutpoint::new(h(7), 0));
        let proof = prove_bonds_v1(&state);
        assert_eq!(
            verify_bond_against_pin(&header, header.hash, &proof, &bond, &[1]),
            Err(BondProofRefusal::Proof(PalwProofErrorV1::Absent))
        );
        // The header must be the block the miner pinned.
        assert!(matches!(
            verify_bond_against_pin(&header, h(0x99), &proof, &bond, &[1]),
            Err(BondProofRefusal::Proof(PalwProofErrorV1::HeaderIsNotThePinnedBlock))
        ));
        // A proof built over a different state does not match this header's committed root.
        let mut other_header = header.clone();
        other_header.palw_state_root = h(0x42);
        other_header.finalize();
        assert!(matches!(
            verify_bond_against_pin(&other_header, other_header.hash, &proof, &bond, &[1]),
            Err(BondProofRefusal::Proof(PalwProofErrorV1::OpeningDoesNotMatchRoot(_)))
        ));
    }
}
