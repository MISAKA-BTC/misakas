//! **RFC-0009 stage D — a light client's proof of one fact in the PALW state, checked against a pinned block.**
//!
//! A remote miner must not start an inference on a bond it believes in because a node said so. What it can check with nothing but a block hash it
//! pinned (a signed checkpoint, `misaka-palw-remote`) is this chain of equalities:
//!
//! ```text
//!   pinned block hash  ◀── recompute ── header bytes            (the header commits `palw_state_root`)
//!   palw_state_root    ◀── hash ─────── state-root preimage     (every collection appears as its 64-byte root; `PalwStateOpeningV1`)
//!   collection root    ◀── hash ─────── the collection's rows   (`PalwCollectionOpeningV1`: ALL rows, so presence AND absence are proven)
//!   the fact           =    the row     (`verify_bond` / `verify_class` / `verify_claim`: decoded from the proven row)
//! ```
//!
//! **What it proves.** That a bond (its key, its registered ML-DSA-87 key, status, collateral, payout), a class (its artifact root, status) or a claim
//! (its roots, phase, executor bond) is — or is not — in the state a header commits. It needs no node's word for any of it.
//!
//! **What it costs and what it does not prove.** The state root is a flat commitment (ADR-0043 §2), not a Merkle tree, so opening a collection
//! means shipping every row of it: O(rows), not O(log n). A bond table with its 2.6 KB keys is megabytes; this is a *minimal* proof that is correct,
//! not a cheap one — a tree-shaped state commitment is a consensus change (a new state-root version) and is the design recorded in the spec's
//! section 5, not done here. It also proves nothing about the header itself beyond "this is the block you pinned": that the pinned block is on the
//! heaviest chain is the checkpoint's trust (a signed, fresh checkpoint — `misaka-palw-remote::checkpoint`), and PoW verification of the header
//! chain is not implemented. **Fences are not state**: they are consensus parameters committed through the params/schedule id, so there is no
//! state proof for one — a client that runs the same ruleset id (pinned with the network) holds them.

use crate::Hash64;
use crate::header::Header;
use crate::palw_state_v2::{
    PalwBondKeyV2, PalwBondStateV2, PalwChainStateV2, PalwClaimStateV2, PalwClassStateV2, palw_collection_root_of_entries_v1,
    palw_state_root_of_preimage_v1,
};

pub const PALW_PROOF_LABEL_BONDS: &[u8] = b"bonds";
pub const PALW_PROOF_LABEL_CLASSES: &[u8] = b"classes";
pub const PALW_PROOF_LABEL_CLAIMS: &[u8] = b"claims";

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwProofErrorV1 {
    #[error("the header does not hash to the pinned block hash")]
    HeaderIsNotThePinnedBlock,
    #[error("the header commits no PALW state root")]
    HeaderCommitsNoState,
    #[error("the opening does not hash to the state root {0}")]
    OpeningDoesNotMatchRoot(Hash64),
    #[error("the collection's root is not part of the committed state")]
    CollectionNotInState,
    #[error("the proof opens collection {got:?}, not {want:?}")]
    WrongCollection { got: Vec<u8>, want: Vec<u8> },
    #[error("the fact is not in the committed state")]
    Absent,
    #[error("the proven row does not decode as the expected record")]
    Undecodable,
}

/// The state-root preimage: every collection root (and the small scalar blocks) in ADR-0043 order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwStateOpeningV1 {
    pub preimage: Vec<u8>,
}

impl PalwStateOpeningV1 {
    pub fn of(state: &PalwChainStateV2) -> Self {
        Self { preimage: state.state_root_preimage() }
    }

    pub fn root(&self) -> Hash64 {
        palw_state_root_of_preimage_v1(&self.preimage)
    }

    /// Does the committed preimage contain this 64-byte collection root? A collection root commits its own label and length, so a
    /// match is a statement about that collection and no other (a different label hashes to a different root).
    pub fn contains_collection_root(&self, root: &Hash64) -> bool {
        self.preimage.windows(64).any(|w| w == root.as_bytes().as_slice())
    }
}

/// Every row of one collection, `(borsh key, borsh value)` in key order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwCollectionOpeningV1 {
    pub label: Vec<u8>,
    pub rows: Vec<(Vec<u8>, Vec<u8>)>,
}

impl PalwCollectionOpeningV1 {
    pub fn root(&self) -> Hash64 {
        palw_collection_root_of_entries_v1(&self.label, self.rows.len(), self.rows.iter().cloned())
    }
}

/// A proof of one collection's rows against a state: the state opening and the collection opening.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwFactProofV1 {
    pub state: PalwStateOpeningV1,
    pub collection: PalwCollectionOpeningV1,
}

fn rows<K: borsh::BorshSerialize, V: borsh::BorshSerialize>(it: impl Iterator<Item = (K, V)>) -> Vec<(Vec<u8>, Vec<u8>)> {
    it.map(|(k, v)| (borsh::to_vec(&k).expect("borsh-serializable"), borsh::to_vec(&v).expect("borsh-serializable"))).collect()
}

/// Prove the bond table (presence or absence of any bond in it).
pub fn prove_bonds_v1(state: &PalwChainStateV2) -> PalwFactProofV1 {
    PalwFactProofV1 {
        state: PalwStateOpeningV1::of(state),
        collection: PalwCollectionOpeningV1 {
            label: PALW_PROOF_LABEL_BONDS.to_vec(),
            rows: rows(state.bonds_iter().map(|(k, v)| (*k, v.clone()))),
        },
    }
}

pub fn prove_classes_v1(state: &PalwChainStateV2) -> PalwFactProofV1 {
    PalwFactProofV1 {
        state: PalwStateOpeningV1::of(state),
        collection: PalwCollectionOpeningV1 {
            label: PALW_PROOF_LABEL_CLASSES.to_vec(),
            rows: rows(state.classes_iter().map(|(k, v)| (*k, v.clone()))),
        },
    }
}

pub fn prove_claims_v1(state: &PalwChainStateV2) -> PalwFactProofV1 {
    PalwFactProofV1 {
        state: PalwStateOpeningV1::of(state),
        collection: PalwCollectionOpeningV1 {
            label: PALW_PROOF_LABEL_CLAIMS.to_vec(),
            rows: rows(state.claims_iter().map(|(k, v)| (*k, v.clone()))),
        },
    }
}

/// **The state root a pinned block commits**, from the header bytes the miner was handed: the header must hash to the pinned block hash
/// (recomputed here, never trusted from the header's own `hash` field) and must commit a PALW state root.
pub fn state_root_of_pinned_header_v1(header: &Header, pinned_block_hash: Hash64) -> Result<Hash64, PalwProofErrorV1> {
    if crate::hashing::header::hash(header) != pinned_block_hash {
        return Err(PalwProofErrorV1::HeaderIsNotThePinnedBlock);
    }
    if header.palw_state_root == Hash64::default() {
        return Err(PalwProofErrorV1::HeaderCommitsNoState);
    }
    Ok(header.palw_state_root)
}

/// The shared half of every verifier: the opening hashes to the root and the collection hashes to a root the opening contains. The rows are then
/// exactly the committed ones in the committed order — the collection root commits their order and count, so a duplicated, reordered or
/// extra row changes it and is refused. Returns the rows (now proven).
fn verified_rows<'a>(
    proof: &'a PalwFactProofV1,
    state_root: Hash64,
    label: &[u8],
) -> Result<&'a [(Vec<u8>, Vec<u8>)], PalwProofErrorV1> {
    if proof.state.root() != state_root {
        return Err(PalwProofErrorV1::OpeningDoesNotMatchRoot(state_root));
    }
    if proof.collection.label != label {
        return Err(PalwProofErrorV1::WrongCollection { got: proof.collection.label.clone(), want: label.to_vec() });
    }
    if !proof.state.contains_collection_root(&proof.collection.root()) {
        return Err(PalwProofErrorV1::CollectionNotInState);
    }
    Ok(&proof.collection.rows)
}

fn find<'a>(rows: &'a [(Vec<u8>, Vec<u8>)], key: &[u8]) -> Option<&'a [u8]> {
    rows.iter().find(|(k, _)| k.as_slice() == key).map(|(_, v)| v.as_slice())
}

fn decode<T: borsh::BorshDeserialize>(bytes: &[u8]) -> Result<T, PalwProofErrorV1> {
    let mut slice = bytes;
    let v = T::deserialize(&mut slice).map_err(|_| PalwProofErrorV1::Undecodable)?;
    slice.is_empty().then_some(v).ok_or(PalwProofErrorV1::Undecodable)
}

/// The bond, as the committed state holds it. `Err(Absent)` is a PROOF of absence (the whole table is committed).
pub fn verify_bond_v1(proof: &PalwFactProofV1, state_root: Hash64, bond: &PalwBondKeyV2) -> Result<PalwBondStateV2, PalwProofErrorV1> {
    let rows = verified_rows(proof, state_root, PALW_PROOF_LABEL_BONDS)?;
    decode(find(rows, &borsh::to_vec(bond).expect("borsh-serializable")).ok_or(PalwProofErrorV1::Absent)?)
}

pub fn verify_class_v1(proof: &PalwFactProofV1, state_root: Hash64, class_id: &Hash64) -> Result<PalwClassStateV2, PalwProofErrorV1> {
    let rows = verified_rows(proof, state_root, PALW_PROOF_LABEL_CLASSES)?;
    decode(find(rows, &borsh::to_vec(class_id).expect("borsh-serializable")).ok_or(PalwProofErrorV1::Absent)?)
}

pub fn verify_claim_v1(proof: &PalwFactProofV1, state_root: Hash64, claim_id: &Hash64) -> Result<PalwClaimStateV2, PalwProofErrorV1> {
    let rows = verified_rows(proof, state_root, PALW_PROOF_LABEL_CLAIMS)?;
    decode(find(rows, &borsh::to_vec(claim_id).expect("borsh-serializable")).ok_or(PalwProofErrorV1::Absent)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_state_v2::PalwChainStateV2;

    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }

    #[test]
    fn the_split_preimage_hashes_to_the_state_root_on_genesis() {
        let s = PalwChainStateV2::genesis();
        assert_eq!(palw_state_root_of_preimage_v1(&s.state_root_preimage()), s.state_root());
        assert_eq!(PalwStateOpeningV1::of(&s).root(), s.state_root());
    }

    #[test]
    fn an_empty_collection_proves_absence_and_a_forged_opening_proves_nothing() {
        let s = PalwChainStateV2::genesis();
        let root = s.state_root();
        let bond = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(7), 0));
        let proof = prove_bonds_v1(&s);
        assert_eq!(verify_bond_v1(&proof, root, &bond), Err(PalwProofErrorV1::Absent), "absence is proven: the whole table is committed");
        // A different state root is refused.
        assert_eq!(verify_bond_v1(&proof, h(1), &bond), Err(PalwProofErrorV1::OpeningDoesNotMatchRoot(h(1))));
        // A collection opened under another label is refused.
        let mut wrong = proof.clone();
        wrong.collection.label = b"classes".to_vec();
        assert!(matches!(verify_bond_v1(&wrong, root, &bond), Err(PalwProofErrorV1::WrongCollection { .. })));
        // A forged row set whose root the state does not contain is refused.
        let mut forged = proof.clone();
        forged.collection.rows.push((vec![9; 68], vec![1, 2, 3]));
        assert_eq!(verify_bond_v1(&forged, root, &bond), Err(PalwProofErrorV1::CollectionNotInState));
        // A tampered preimage does not hash to the root.
        let mut tampered = proof.clone();
        tampered.state.preimage[10] ^= 1;
        assert!(matches!(verify_bond_v1(&tampered, root, &bond), Err(PalwProofErrorV1::OpeningDoesNotMatchRoot(_))));
    }

    #[test]
    fn a_header_is_the_pinned_block_only_if_it_hashes_to_it_and_it_must_commit_a_state() {
        let state_root = PalwChainStateV2::genesis().state_root();
        let header = Header::new_finalized(
            1,
            vec![vec![h(1)]].try_into().unwrap(),
            h(2),
            h(3),
            h(4),
            1,
            0x1d00ffff,
            0,
            crate::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2,
            5,
            0u64.into(),
            0,
            h(5),
        )
        .with_palw_state_root(state_root);
        let mut header = header;
        header.finalize();
        assert_eq!(state_root_of_pinned_header_v1(&header, header.hash), Ok(state_root));
        assert_eq!(state_root_of_pinned_header_v1(&header, h(9)), Err(PalwProofErrorV1::HeaderIsNotThePinnedBlock));
        // A header whose claimed root was swapped no longer hashes to the pin: the root cannot be changed without the block.
        let mut swapped = header.clone();
        swapped.palw_state_root = h(8);
        assert_eq!(state_root_of_pinned_header_v1(&swapped, header.hash), Err(PalwProofErrorV1::HeaderIsNotThePinnedBlock));
    }
}
