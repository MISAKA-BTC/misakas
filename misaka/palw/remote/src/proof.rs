//! Stage D, client side: believe a bond only if it is proven against a block the miner pinned.
//!
//! The consensus-core module `palw_state_proof_v1` does the cryptography (header → state root → collection → row). This is the policy a miner
//! applies on top of it: the proven bond is the one it holds a key for, and it may produce (not retiring). A node supplies the header bytes and the
//! proof; it cannot make either say something the pinned block does not.

use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::palw_state_proof_v1::{
    PalwCollectionOpeningV1, PalwFactProofV1, PalwProofErrorV1, PalwStateOpeningV1, state_root_of_pinned_header_v1, verify_bond_v1,
    verify_claim_v1, verify_class_v1,
};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwBondStateV2, PalwBondStatusV2, PalwClaimStateV2, PalwClassStateV2};
use kaspa_hashes::Hash64;

use crate::trust::{Labelled, Provenance};

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

/// **A proof as the wire carries it** (`getPalwStateProof`, op 202): the state-root preimage, the collection's label and every row. Nothing in
/// it is trusted until one of the `*_at_pin_v1` functions below has checked it against the header of a block the client pinned.
pub fn proof_from_parts_v1(state_preimage: Vec<u8>, label: &str, rows: Vec<(Vec<u8>, Vec<u8>)>) -> PalwFactProofV1 {
    PalwFactProofV1 {
        state: PalwStateOpeningV1 { preimage: state_preimage },
        collection: PalwCollectionOpeningV1 { label: label.as_bytes().to_vec(), rows },
    }
}

/// **A state proof as a file carries it** (hex): the committing block, the header's borsh bytes, the state-root preimage and every row of one
/// collection. A bundle embeds one so an OFFLINE signer can check the owner bond's key against a block it pinned.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StateProofV1 {
    /// The block whose header commits the state (the one the client pinned).
    pub block: Hash64,
    pub header: String,
    pub state_preimage: String,
    pub collection: String,
    pub rows: Vec<(String, String)>,
}

impl StateProofV1 {
    pub fn new(block: Hash64, header: &Header, proof: &PalwFactProofV1) -> Self {
        Self {
            block,
            header: faster_hex::hex_string(&borsh::to_vec(header).expect("a header serializes")),
            state_preimage: faster_hex::hex_string(&proof.state.preimage),
            collection: String::from_utf8_lossy(&proof.collection.label).into_owned(),
            rows: proof.collection.rows.iter().map(|(k, v)| (faster_hex::hex_string(k), faster_hex::hex_string(v))).collect(),
        }
    }

    /// Decode the header and the proof. Decoding proves nothing; the `*_at_pin_v1` functions do.
    pub fn open(&self) -> Result<(Header, PalwFactProofV1), String> {
        fn unhex(t: &str) -> Result<Vec<u8>, String> {
            let mut out = vec![0u8; t.len() / 2];
            if t.len() % 2 != 0 || faster_hex::hex_decode(t.as_bytes(), &mut out).is_err() {
                return Err("a hex field is not hex".into());
            }
            Ok(out)
        }
        let header: Header = borsh::from_slice(&unhex(&self.header)?).map_err(|e| format!("the header does not decode: {e}"))?;
        let rows = self.rows.iter().map(|(k, v)| Ok((unhex(k)?, unhex(v)?))).collect::<Result<Vec<_>, String>>()?;
        Ok((header, proof_from_parts_v1(unhex(&self.state_preimage)?, &self.collection, rows)))
    }
}

fn pinned_root(header: &Header, pinned: Hash64) -> Result<(Hash64, u64), PalwProofErrorV1> {
    Ok((state_root_of_pinned_header_v1(header, pinned)?, header.daa_score))
}

/// The bond in the state `pinned`'s header commits: present (with its record), or PROVEN absent. Any other failure of the proof is an error —
/// a node that cannot prove is not evidence of absence.
pub fn bond_at_pin_v1(
    header: &Header,
    pinned: Hash64,
    proof: &PalwFactProofV1,
    bond: &PalwBondKeyV2,
) -> Result<Labelled<Option<PalwBondStateV2>>, PalwProofErrorV1> {
    let (root, header_daa) = pinned_root(header, pinned)?;
    bond_under_root_v1(root, pinned, header_daa, proof, bond)
}

/// [`bond_at_pin_v1`] under an ADR-0043 root the caller already established for `pinned`'s header — past
/// `palw_fork_choice_commitment_v1` the header commits an envelope, whose inner root an opening unwraps (`crate::l2`,
/// `crate::verify::l3_root_at_header_v1`).
pub fn bond_under_root_v1(
    root: Hash64,
    pinned: Hash64,
    header_daa: u64,
    proof: &PalwFactProofV1,
    bond: &PalwBondKeyV2,
) -> Result<Labelled<Option<PalwBondStateV2>>, PalwProofErrorV1> {
    match verify_bond_v1(proof, root, bond) {
        Ok(state) => Ok(Labelled { value: Some(state), provenance: Provenance::ProvenAtPin { pinned_block: pinned, header_daa } }),
        Err(PalwProofErrorV1::Absent) => {
            Ok(Labelled { value: None, provenance: Provenance::ProvenAbsentAtPin { pinned_block: pinned, header_daa } })
        }
        Err(e) => Err(e),
    }
}

pub fn class_at_pin_v1(
    header: &Header,
    pinned: Hash64,
    proof: &PalwFactProofV1,
    class_id: &Hash64,
) -> Result<Labelled<Option<PalwClassStateV2>>, PalwProofErrorV1> {
    let (root, header_daa) = pinned_root(header, pinned)?;
    class_under_root_v1(root, pinned, header_daa, proof, class_id)
}

/// [`class_at_pin_v1`] under an ADR-0043 root the caller already established for `pinned`'s header (see [`bond_under_root_v1`]).
pub fn class_under_root_v1(
    root: Hash64,
    pinned: Hash64,
    header_daa: u64,
    proof: &PalwFactProofV1,
    class_id: &Hash64,
) -> Result<Labelled<Option<PalwClassStateV2>>, PalwProofErrorV1> {
    match verify_class_v1(proof, root, class_id) {
        Ok(state) => Ok(Labelled { value: Some(state), provenance: Provenance::ProvenAtPin { pinned_block: pinned, header_daa } }),
        Err(PalwProofErrorV1::Absent) => {
            Ok(Labelled { value: None, provenance: Provenance::ProvenAbsentAtPin { pinned_block: pinned, header_daa } })
        }
        Err(e) => Err(e),
    }
}

pub fn claim_at_pin_v1(
    header: &Header,
    pinned: Hash64,
    proof: &PalwFactProofV1,
    claim_id: &Hash64,
) -> Result<Labelled<Option<PalwClaimStateV2>>, PalwProofErrorV1> {
    let (root, header_daa) = pinned_root(header, pinned)?;
    match verify_claim_v1(proof, root, claim_id) {
        Ok(state) => Ok(Labelled { value: Some(state), provenance: Provenance::ProvenAtPin { pinned_block: pinned, header_daa } }),
        Err(PalwProofErrorV1::Absent) => {
            Ok(Labelled { value: None, provenance: Provenance::ProvenAbsentAtPin { pinned_block: pinned, header_daa } })
        }
        Err(e) => Err(e),
    }
}

/// What a class proof says about a registration the client signed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistrationAtPinV1 {
    /// The class is in the committed state under THIS root and THIS registrant bond: proven.
    Registered(Provenance),
    /// Proven absent as of the pinned block — a registration after it is not covered.
    NotYetRegistered(Provenance),
    /// The committed state holds the class under another root or another registrant: proven misattribution, not a rumour.
    Misattributed { why: String, provenance: Provenance },
}

/// **Is the registration the client signed in the state the pinned block commits?** The registrant bond is read out of the proven class record
/// (`PalwClassStateV2::registrant_bond`) — the class-row RPC does not report it, and does not need to.
pub fn registration_at_pin_v1(
    header: &Header,
    pinned: Hash64,
    classes_proof: &PalwFactProofV1,
    class_id: &Hash64,
    artifact_root: Hash64,
    owner_bond: &PalwBondKeyV2,
) -> Result<RegistrationAtPinV1, PalwProofErrorV1> {
    let proven = class_at_pin_v1(header, pinned, classes_proof, class_id)?;
    Ok(match proven.value {
        None => RegistrationAtPinV1::NotYetRegistered(proven.provenance),
        Some(record) if record.artifact_root != artifact_root => RegistrationAtPinV1::Misattributed {
            why: format!(
                "the committed state holds class {class_id} over root {}, this registration signed root {artifact_root}",
                record.artifact_root
            ),
            provenance: proven.provenance,
        },
        Some(record) if record.registrant_bond != Some(*owner_bond) => RegistrationAtPinV1::Misattributed {
            why: format!(
                "the committed state holds class {class_id} under registrant {:?}, this registration signed bond {}:{}",
                record.registrant_bond.map(|b| format!("{}:{}", b.0.transaction_id, b.0.index)),
                owner_bond.0.transaction_id,
                owner_bond.0.index
            ),
            provenance: proven.provenance,
        },
        Some(_) => RegistrationAtPinV1::Registered(proven.provenance),
    })
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

    // ---- the registration standing, from a hand-committed state (a presence case needs a record; the real-chain case is the consensus test
    // `t12_state_proof`) ----

    use kaspa_consensus_core::palw_state_proof_v1::PalwCollectionOpeningV1 as Coll;
    use kaspa_consensus_core::palw_state_v2::{
        PalwClassStatusV2, PalwPwuRuleV2, palw_collection_root_of_entries_v1, palw_state_root_of_preimage_v1,
    };

    fn class_record(root: Hash64, registrant: Option<PalwBondKeyV2>) -> PalwClassStateV2 {
        PalwClassStateV2 {
            artifact_root: root,
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(7),
            status: PalwClassStatusV2::Active,
            registered_daa: 10,
            registrant_bond: registrant,
            fused_attention: false,
        }
    }

    /// A state whose class table holds `record` under `class_id`, committed by a header; returns (header, classes proof).
    fn committed_classes(class_id: Hash64, record: Option<PalwClassStateV2>) -> (Header, PalwFactProofV1) {
        let rows: Vec<(Vec<u8>, Vec<u8>)> =
            record.iter().map(|r| (borsh::to_vec(&class_id).unwrap(), borsh::to_vec(r).unwrap())).collect();
        let root = palw_collection_root_of_entries_v1(b"classes", rows.len(), rows.iter().cloned());
        let mut preimage = vec![0xAA; 13];
        preimage.extend_from_slice(root.as_bytes().as_slice());
        preimage.extend_from_slice(&[0xBB; 9]);
        let state_root = palw_state_root_of_preimage_v1(&preimage);
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
            55,
            0u64.into(),
            0,
            h(5),
        )
        .with_palw_state_root(state_root);
        header.finalize();
        let proof = PalwFactProofV1 {
            state: kaspa_consensus_core::palw_state_proof_v1::PalwStateOpeningV1 { preimage },
            collection: Coll { label: b"classes".to_vec(), rows },
        };
        (header, proof)
    }

    #[test]
    fn a_registration_is_proven_present_absent_or_misattributed_against_the_pin_and_never_by_a_nodes_word() {
        let (class, root, ours) = (h(0x10), h(0x11), PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(h(7), 1)));
        // Present under our root and our bond.
        let (header, proof) = committed_classes(class, Some(class_record(root, Some(ours))));
        match registration_at_pin_v1(&header, header.hash, &proof, &class, root, &ours).unwrap() {
            RegistrationAtPinV1::Registered(p) => assert!(p.is_proven() && p.label().contains("PROVEN against pinned block")),
            other => panic!("{other:?}"),
        }
        // Another registrant bond: PROVEN misattribution (the class-row RPC cannot say this; the proof can).
        let thief = PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(h(8), 0));
        let (header2, proof2) = committed_classes(class, Some(class_record(root, Some(thief))));
        assert!(matches!(
            registration_at_pin_v1(&header2, header2.hash, &proof2, &class, root, &ours).unwrap(),
            RegistrationAtPinV1::Misattributed { .. }
        ));
        // Another root under our class id.
        let (header3, proof3) = committed_classes(class, Some(class_record(h(0x99), Some(ours))));
        assert!(matches!(
            registration_at_pin_v1(&header3, header3.hash, &proof3, &class, root, &ours).unwrap(),
            RegistrationAtPinV1::Misattributed { .. }
        ));
        // A genesis class (no registrant) is not ours either.
        let (header4, proof4) = committed_classes(class, Some(class_record(root, None)));
        assert!(matches!(
            registration_at_pin_v1(&header4, header4.hash, &proof4, &class, root, &ours).unwrap(),
            RegistrationAtPinV1::Misattributed { .. }
        ));
        // Absent as of the pin: proven absent — and silent about what came after.
        let (header5, proof5) = committed_classes(class, None);
        match registration_at_pin_v1(&header5, header5.hash, &proof5, &class, root, &ours).unwrap() {
            RegistrationAtPinV1::NotYetRegistered(p) => assert!(p.label().contains("later blocks")),
            other => panic!("{other:?}"),
        }
        // A proof that does not open against the pinned header is an ERROR, never "absent".
        assert!(registration_at_pin_v1(&header, h(0x42), &proof, &class, root, &ours).is_err(), "not the pinned block");
        let mut forged = proof.clone();
        forged.collection.rows.clear();
        assert!(
            registration_at_pin_v1(&header, header.hash, &forged, &class, root, &ours).is_err(),
            "a node that hides the row cannot make it absent"
        );
        // …and the same through the wire-parts constructor.
        let wire = proof_from_parts_v1(proof.state.preimage.clone(), "classes", proof.collection.rows.clone());
        assert_eq!(wire, proof);
    }

    #[test]
    fn bonds_and_claims_have_the_same_present_absent_standing() {
        let state = PalwChainStateV2::genesis();
        let header = pinned(&state);
        let bond = PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(h(7), 0));
        let got = bond_at_pin_v1(&header, header.hash, &prove_bonds_v1(&state), &bond).unwrap();
        assert_eq!(got.value, None);
        assert!(matches!(got.provenance, Provenance::ProvenAbsentAtPin { .. }));
        let got =
            claim_at_pin_v1(&header, header.hash, &kaspa_consensus_core::palw_state_proof_v1::prove_claims_v1(&state), &h(3)).unwrap();
        assert_eq!(got.value, None);
        // A claims proof offered for a bond question is refused, not read as absence.
        assert!(
            bond_at_pin_v1(&header, header.hash, &kaspa_consensus_core::palw_state_proof_v1::prove_claims_v1(&state), &bond).is_err()
        );
    }
}
