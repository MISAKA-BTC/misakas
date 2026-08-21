//! ADR-0038 W1 / external audit P0-8: **operands a full node can check without the model.**
//!
//! Adjudicating a MatMul, a Requantize or a RoPE step needs raw rows of the model artifact. The
//! court reads them through [`crate::palw_step_refute::PalwWeightOracleV1`], and the production
//! implementation for a full node — `PalwNoWeightsV1` — answers `None` to everything, so every step
//! conviction lands `Unadjudicable`. Arithmetic fraud is unconvictable.
//!
//! The tempting repair is worse than the gap: give nodes that happen to hold the artifact a real
//! oracle, and a verdict starts depending on which local files a node has. Two honest nodes then
//! disagree about a conviction, which is consensus splitting on filesystem contents.
//!
//! **So the operand travels with the accusation.** A refutation carries the rows it needs plus a
//! proof against a root the CHAIN registered for the class, and the adjudicator checks the proof
//! instead of reading a file. W1's "a full node never runs the LLM" then extends to "and never
//! needs its weights either".
//!
//! `model_weights_hash` cannot serve as that root: it is an identity digest over the GGUF's sha256,
//! size, filename, repo and revision — flat, and nothing opens against it. This module is the
//! openable commitment it is not.
//!
//! ## What is NOT here
//!
//! The **inventory**: which tensors, in what order, sliced into what rows. That is a property of the
//! model as the pinned runtime lays it out, and it belongs with the shape profile — which has never
//! been built for the pinned model (every profile in this tree is a test fixture). So this module
//! provides the commitment and the proof; a class cannot USE it until its inventory exists. Landing
//! the mechanism first means the remainder is "register a root", not "design a proof system".

use crate::Hash64;
use blake2b_simd::Params;

/// Domain of an artifact leaf: one contiguous row slice of one tensor.
pub const PALW_ARTIFACT_DOMAIN_LEAF: &[u8] = b"misaka-palw/artifact/leaf/v1";
/// Domain of an artifact interior node.
pub const PALW_ARTIFACT_DOMAIN_NODE: &[u8] = b"misaka-palw/artifact/node/v1";

/// One operand a refutation carries, with the position it claims.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwArtifactOperandV1 {
    /// GGUF tensor name, layer-substituted exactly as the step profile names it.
    pub tensor_name: String,
    /// `None` for a graph-level tensor; the layer index otherwise.
    pub layer: Option<u16>,
    /// **A BYTE offset into the tensor** (ADR-0049 Decision A). Every caller already treated it
    /// as one; the name predates the contract being written down.
    pub row_start: u32,
    /// The bytes themselves, in the tensor's own dtype.
    pub bytes: Vec<u8>,
}

/// A leaf digest binds the POSITION as well as the bytes.
///
/// Without the position an opening proves only that these bytes are somewhere in the artifact, and
/// an accuser could open a genuine row of some other tensor and claim it as the operand of the step
/// under dispute — a proof that verifies while proving the wrong thing.
pub fn artifact_leaf_v1(operand: &PalwArtifactOperandV1) -> Hash64 {
    let mut state = Params::new().hash_length(64).key(PALW_ARTIFACT_DOMAIN_LEAF).to_state();
    state.update(&(operand.tensor_name.len() as u32).to_le_bytes());
    state.update(operand.tensor_name.as_bytes());
    // `u32::MAX` is the "no layer" marker rather than an absent field, so a graph-level tensor and
    // layer `u32::MAX` cannot collide by one being shorter than the other.
    state.update(&operand.layer.map_or(u32::MAX, u32::from).to_le_bytes());
    state.update(&operand.row_start.to_le_bytes());
    state.update(&(operand.bytes.len() as u32).to_le_bytes());
    state.update(&operand.bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

fn node(left: &Hash64, right: &Hash64) -> Hash64 {
    let mut state = Params::new().hash_length(64).key(PALW_ARTIFACT_DOMAIN_NODE).to_state();
    state.update(left.as_byte_slice());
    state.update(right.as_byte_slice());
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// The class's artifact root over an ordered leaf inventory.
///
/// An odd node is **promoted, not duplicated**. Duplicating the last leaf is the classic
/// second-preimage hole — a tree over `[a, b, c]` and one over `[a, b, c, c]` produce the same root,
/// so a proof for the fourth position verifies against an inventory that has three. Promotion is
/// what `palw_step_leg` already does, and the two must agree in spirit or a reader will assume one
/// while auditing the other.
pub fn artifact_root_v1(leaves: &[Hash64]) -> Option<Hash64> {
    if leaves.is_empty() {
        return None; // an empty inventory has no root, and a zero root would verify nothing forever
    }
    let mut level: Vec<Hash64> = leaves.to_vec();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut i = 0;
        while i + 1 < level.len() {
            next.push(node(&level[i], &level[i + 1]));
            i += 2;
        }
        if i < level.len() {
            next.push(level[i]); // promote
        }
        level = next;
    }
    Some(level[0])
}

/// An opening: the operand, its index, the inventory size, and the sibling path.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwArtifactOpeningV1 {
    pub operand: PalwArtifactOperandV1,
    pub leaf_index: u32,
    pub leaf_count: u32,
    pub path: Vec<Hash64>,
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwArtifactError {
    #[error("leaf index {index} is outside an inventory of {count}")]
    IndexOutOfRange { index: u32, count: u32 },
    #[error("the opening does not reconstruct the registered artifact root")]
    RootMismatch,
    #[error("an inventory of {0} leaves has no root")]
    EmptyInventory(u32),
}

/// Recompute the root an opening implies, and compare it with the class's registered one.
///
/// Promote levels consume no path element, exactly as the tree builds them — a path whose length is
/// "log2 of the count" would be wrong for any inventory that is not a power of two, and wrong in the
/// direction that accepts a forged sibling.
pub fn verify_artifact_opening_v1(opening: &PalwArtifactOpeningV1, registered_root: Hash64) -> Result<(), PalwArtifactError> {
    if opening.leaf_count == 0 {
        return Err(PalwArtifactError::EmptyInventory(0));
    }
    if opening.leaf_index >= opening.leaf_count {
        return Err(PalwArtifactError::IndexOutOfRange { index: opening.leaf_index, count: opening.leaf_count });
    }
    let mut acc = artifact_leaf_v1(&opening.operand);
    let mut index = opening.leaf_index as u64;
    let mut width = opening.leaf_count as u64;
    let mut supplied = opening.path.iter();
    while width > 1 {
        let promoted = index == width - 1 && width % 2 == 1;
        if !promoted {
            let sibling = supplied.next().ok_or(PalwArtifactError::RootMismatch)?;
            acc = if index % 2 == 0 { node(&acc, sibling) } else { node(sibling, &acc) };
        }
        index /= 2;
        width = width.div_ceil(2);
    }
    // A path with elements left over is a different tree that happened to reach the same root by
    // accident of length; refuse rather than ignore the tail.
    if supplied.next().is_some() || acc != registered_root {
        return Err(PalwArtifactError::RootMismatch);
    }
    Ok(())
}

/// A [`crate::palw_step_refute::PalwWeightOracleV1`] backed by PROVEN operands.
///
/// This is the point of the module: the court's arithmetic does not change at all. It keeps asking
/// an oracle for rows; the oracle is now satisfied by evidence the accusation carried and this node
/// checked against a chain-registered root, instead of by a file this node may or may not hold.
pub struct PalwProvenOperandsV1 {
    operands: Vec<PalwArtifactOperandV1>,
}

impl PalwProvenOperandsV1 {
    /// Verify every opening against `registered_root`, then expose the operands.
    ///
    /// All-or-nothing: one bad opening rejects the whole set rather than dropping that operand,
    /// because a dropped operand becomes `None` downstream, which the court reads as
    /// `Unadjudicable` — an accusation with one forged row would look like a coverage gap and
    /// freeze the class (I10) instead of failing as the forgery it is.
    pub fn from_openings_v1(openings: &[PalwArtifactOpeningV1], registered_root: Hash64) -> Result<Self, PalwArtifactError> {
        for opening in openings {
            verify_artifact_opening_v1(opening, registered_root)?;
        }
        Ok(Self { operands: openings.iter().map(|o| o.operand.clone()).collect() })
    }
}

impl crate::palw_step_refute::PalwWeightOracleV1 for PalwProvenOperandsV1 {
    /// **Exactly `byte_len` bytes, or nothing** (ADR-0049 Decision A).
    ///
    /// This returned `byte_len` bytes while the trait asked for `elements` VALUES, and the two
    /// coincide only at a one-byte dtype. `PALW-BASE-0` is `int8` throughout, so the only class
    /// that exists could not expose it — and `Rescale`, which asked for one value and required
    /// five bytes, could never adjudicate through a real opening. The contract is bytes on both
    /// sides now; the mismatch had no way to announce itself before.
    fn operand_bytes(&self, tensor_name: &str, layer: Option<u16>, byte_offset: u32, byte_len: u32) -> Option<Vec<u8>> {
        let operand =
            self.operands.iter().find(|o| o.tensor_name == tensor_name && o.layer == layer && o.row_start == byte_offset)?;
        // The proof binds the bytes that were committed; it says nothing about how many the caller
        // wants. A short opening is a missing operand, not a truncated answer — and a LONG one is
        // refused too, because an opening that proves more than the step reads is an opening whose
        // extra bytes nothing checked.
        (operand.bytes.len() == byte_len as usize).then(|| operand.bytes.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_step_refute::PalwWeightOracleV1;

    fn operand(name: &str, layer: Option<u16>, row_start: u32, bytes: &[u8]) -> PalwArtifactOperandV1 {
        PalwArtifactOperandV1 { tensor_name: name.to_string(), layer, row_start, bytes: bytes.to_vec() }
    }

    fn inventory() -> Vec<PalwArtifactOperandV1> {
        vec![
            operand("blk.{layer}.attn_q.weight", Some(0), 0, &[1, 2, 3, 4]),
            operand("blk.{layer}.attn_k.weight", Some(0), 0, &[5, 6, 7, 8]),
            operand("blk.{layer}.ffn_up.weight", Some(1), 0, &[9, 10]),
        ]
    }

    fn open(index: usize) -> (PalwArtifactOpeningV1, Hash64) {
        let inv = inventory();
        let leaves: Vec<Hash64> = inv.iter().map(artifact_leaf_v1).collect();
        let root = artifact_root_v1(&leaves).unwrap();
        // Three leaves: [h0 h1 h2] -> [n(h0,h1), h2(promoted)] -> root. Index 2 promotes once.
        let path = match index {
            0 => vec![leaves[1]],
            1 => vec![leaves[0]],
            2 => vec![],
            _ => unreachable!(),
        };
        let mut path = path;
        if index < 2 {
            path.push(leaves[2]);
        } else {
            path.push(node(&leaves[0], &leaves[1]));
        }
        (
            PalwArtifactOpeningV1 { operand: inv[index].clone(), leaf_index: index as u32, leaf_count: 3, path },
            root,
        )
    }

    /// The court adjudicates from carried evidence, with no local model.
    #[test]
    fn a_proven_operand_answers_the_oracle() {
        let (opening, root) = open(0);
        let oracle = PalwProvenOperandsV1::from_openings_v1(&[opening], root).expect("honest opening");
        assert_eq!(oracle.operand_bytes("blk.{layer}.attn_q.weight", Some(0), 0, 4), Some(vec![1, 2, 3, 4]));
        // A row nobody proved is absent — which the court reads as Unadjudicable, the safe
        // direction: no proof, no conviction.
        assert_eq!(oracle.operand_bytes("blk.{layer}.attn_q.weight", Some(0), 0, 5), None, "more elements than were proven");
        assert_eq!(oracle.operand_bytes("blk.{layer}.ffn_down.weight", Some(0), 0, 1), None, "never opened");
    }

    /// The leaf binds the POSITION, so a genuine row of another tensor cannot stand in.
    ///
    /// This is the attack a bytes-only leaf allows: open a real row, claim it as the operand of the
    /// step under dispute, and the proof verifies while proving the wrong thing.
    #[test]
    fn an_operand_cannot_be_moved_to_another_position() {
        let (mut opening, root) = open(0);
        opening.operand.tensor_name = "blk.{layer}.attn_k.weight".into();
        assert_eq!(verify_artifact_opening_v1(&opening, root), Err(PalwArtifactError::RootMismatch));

        let (mut opening, root) = open(0);
        opening.operand.layer = Some(1);
        assert_eq!(verify_artifact_opening_v1(&opening, root), Err(PalwArtifactError::RootMismatch));

        let (mut opening, root) = open(0);
        opening.operand.row_start = 4;
        assert_eq!(verify_artifact_opening_v1(&opening, root), Err(PalwArtifactError::RootMismatch));
    }

    /// A promoted odd node consumes no path element, and the tree does not duplicate the last leaf.
    ///
    /// Duplication is the classic second-preimage hole: `[a, b, c]` and `[a, b, c, c]` would share a
    /// root, so a proof for a fourth position verifies against an inventory of three.
    #[test]
    fn the_odd_leaf_is_promoted_not_duplicated() {
        let inv = inventory();
        let leaves: Vec<Hash64> = inv.iter().map(artifact_leaf_v1).collect();
        let three = artifact_root_v1(&leaves).unwrap();
        let four = artifact_root_v1(&[leaves[0], leaves[1], leaves[2], leaves[2]]).unwrap();
        assert_ne!(three, four, "duplicating the odd leaf would make two inventories share a root");

        let (opening, root) = open(2);
        assert!(verify_artifact_opening_v1(&opening, root).is_ok(), "the promoted leaf opens");
    }

    /// A forged operand rejects the whole set rather than being dropped.
    ///
    /// Dropping it would make it `None` downstream, and the court reads `None` as `Unadjudicable` —
    /// so an accusation carrying one forged row would look like a coverage gap and freeze the class
    /// (I10) instead of failing as the forgery it is.
    #[test]
    fn one_forged_opening_rejects_the_whole_accusation() {
        let (good, root) = open(0);
        let (mut bad, _) = open(1);
        bad.operand.bytes = vec![0xFF; 4];
        assert_eq!(
            PalwProvenOperandsV1::from_openings_v1(&[good, bad], root).err(),
            Some(PalwArtifactError::RootMismatch)
        );
    }

    /// Out-of-range and empty inventories are refused rather than reaching the hash.
    #[test]
    fn an_impossible_position_is_refused() {
        let (mut opening, root) = open(0);
        opening.leaf_index = 3;
        assert_eq!(verify_artifact_opening_v1(&opening, root), Err(PalwArtifactError::IndexOutOfRange { index: 3, count: 3 }));
        opening.leaf_count = 0;
        assert_eq!(verify_artifact_opening_v1(&opening, root), Err(PalwArtifactError::EmptyInventory(0)));
        assert_eq!(artifact_root_v1(&[]), None);
    }

    /// A path with elements left over is refused, not ignored.
    #[test]
    fn a_path_with_a_tail_is_refused() {
        let (mut opening, root) = open(0);
        opening.path.push(Hash64::from_u64_word(0xDEAD));
        assert_eq!(verify_artifact_opening_v1(&opening, root), Err(PalwArtifactError::RootMismatch));
    }
}
