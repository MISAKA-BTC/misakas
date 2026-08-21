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
//! ## The inventory (ADR-0049 Decision G)
//!
//! This module used to say the inventory was "NOT here" — which tensors, in what order, sliced into
//! what rows — on the grounds that no shape profile existed for a real class. Both classes have one
//! now, so [`PalwArtifactInventoryV1`] is that missing half: the canonical layout an
//! `artifact_root` commits to, with the rules that make an opening's ABSENCE mean something.
//!
//! An opening proves "these bytes are at this position under this root". It proves nothing about
//! what is NOT opened unless the layout is pinned too — without that, an artifact can carry a row
//! twice at different offsets, leave a gap no entry covers, or append bytes nothing describes, and
//! every individual opening still verifies. So the constructor refuses a duplicate, an overlap, a
//! gap, a zero-length row and a non-canonical order, and "every byte is covered exactly once, in
//! one order" becomes a property of the type rather than a hope about the producer.

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

/// **The prover side of [`verify_artifact_opening_v1`]** — build the opening for one leaf.
///
/// The verifier existed and the prover did not, so every opening in the tree was hand-assembled by
/// a test that knew its own three-leaf shape. That is the half of a proof system where a mistake
/// is invisible: a hand-built path that happens to verify proves the test, not the code, and a
/// producer with no way to MAKE an opening cannot carry one in a real close (audit C-06).
///
/// Promotion is mirrored exactly: a node with no sibling at its level consumes no path element, so
/// the path this emits is the path the verifier consumes, for any inventory size rather than for
/// powers of two.
pub fn open_artifact_leaf_v1(operands: &[PalwArtifactOperandV1], index: u32) -> Option<PalwArtifactOpeningV1> {
    if operands.is_empty() || index as usize >= operands.len() {
        return None;
    }
    let mut level: Vec<Hash64> = operands.iter().map(artifact_leaf_v1).collect();
    let leaf_count = level.len() as u32;
    let mut at = index as usize;
    let mut path = Vec::new();
    while level.len() > 1 {
        let promoted = at == level.len() - 1 && level.len() % 2 == 1;
        if !promoted {
            path.push(if at % 2 == 0 { level[at + 1] } else { level[at - 1] });
        }
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut i = 0;
        while i + 1 < level.len() {
            next.push(node(&level[i], &level[i + 1]));
            i += 2;
        }
        if i < level.len() {
            next.push(level[i]);
        }
        level = next;
        at /= 2;
    }
    Some(PalwArtifactOpeningV1 { operand: operands[index as usize].clone(), leaf_index: index, leaf_count, path })
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

    /// **The prover and the verifier agree, at every inventory size** — including the odd ones,
    /// where promotion decides whether a level consumes a path element.
    ///
    /// The hand-built openings this module tested with knew their own three-leaf shape, so they
    /// proved the fixture rather than the code, and no producer could build one at all.
    #[test]
    fn every_leaf_opens_and_every_opening_verifies() {
        for count in 1usize..=17 {
            let inv: Vec<PalwArtifactOperandV1> =
                (0..count).map(|i| operand("t", Some(i as u16), 0, &[i as u8, 0xAA, 0xBB])).collect();
            let root = artifact_root_v1(&inv.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).unwrap();
            for i in 0..count {
                let opening = open_artifact_leaf_v1(&inv, i as u32).expect("every leaf opens");
                assert_eq!(verify_artifact_opening_v1(&opening, root), Ok(()), "count {count}, leaf {i}");
                // …and it proves THAT leaf: the same path with another operand does not verify.
                let mut forged = opening.clone();
                forged.operand.bytes[0] ^= 0xFF;
                assert_eq!(verify_artifact_opening_v1(&forged, root), Err(PalwArtifactError::RootMismatch));
            }
            assert!(open_artifact_leaf_v1(&inv, count as u32).is_none(), "an index past the end opens nothing");
        }
        assert!(open_artifact_leaf_v1(&[], 0).is_none());
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


// ---------------------------------------------------------------------------------------------
// The canonical inventory (ADR-0049 Decision G)
// ---------------------------------------------------------------------------------------------

/// Why an inventory is not canonical.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwInventoryError {
    #[error("an empty inventory has no root, and a zero root would verify nothing forever")]
    Empty,
    #[error("row {index} of '{tensor}' is zero-length — a row that proves no bytes is a leaf that binds nothing")]
    ZeroLengthRow { tensor: String, index: usize },
    #[error("'{tensor}' appears twice at byte {offset}: two leaves for one position let a producer choose which one an opening meets")]
    DuplicateRow { tensor: String, offset: u32 },
    #[error("'{tensor}' is out of canonical order at index {index}: (name, layer, offset) ascending is what makes the order one nobody chooses")]
    NotCanonicalOrder { tensor: String, index: usize },
    #[error("'{tensor}' does not start at byte 0 — a tensor whose first row is not its first byte has a prefix nothing covers")]
    DoesNotStartAtZero { tensor: String },
    #[error("'{tensor}' has a gap or an overlap at byte {at}: expected the previous row to end there")]
    GapOrOverlap { tensor: String, at: u32 },
    #[error("the profile names '{tensor}' and the inventory does not carry it")]
    ProfileTensorMissing { tensor: String },
}

/// **The canonical layout an `artifact_root` commits to.**
///
/// One entry per contiguous row slice, ordered by `(tensor_name, layer, byte_offset)` ascending,
/// with every tensor tiled from byte 0 with no gap and no overlap. Constructible only through
/// [`PalwArtifactInventoryV1::new`], so an inventory value IS a checked one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwArtifactInventoryV1 {
    operands: Vec<PalwArtifactOperandV1>,
}

impl PalwArtifactInventoryV1 {
    /// Check the layout, then keep it. Every rule refuses a way an opening's absence could be made
    /// to mean nothing.
    pub fn new(operands: Vec<PalwArtifactOperandV1>) -> Result<Self, PalwInventoryError> {
        if operands.is_empty() {
            return Err(PalwInventoryError::Empty);
        }
        let key = |o: &PalwArtifactOperandV1| (o.tensor_name.clone(), o.layer, o.row_start);
        for (i, o) in operands.iter().enumerate() {
            if o.bytes.is_empty() {
                return Err(PalwInventoryError::ZeroLengthRow { tensor: o.tensor_name.clone(), index: i });
            }
            if i > 0 {
                let prev = &operands[i - 1];
                let (pk, ok) = (key(prev), key(o));
                if pk == ok {
                    return Err(PalwInventoryError::DuplicateRow { tensor: o.tensor_name.clone(), offset: o.row_start });
                }
                if pk > ok {
                    return Err(PalwInventoryError::NotCanonicalOrder { tensor: o.tensor_name.clone(), index: i });
                }
                // Within one tensor the rows must tile it: the next row starts exactly where the
                // previous ended. Across tensors the previous end says nothing.
                if (prev.tensor_name.as_str(), prev.layer) == (o.tensor_name.as_str(), o.layer) {
                    let end = prev.row_start.checked_add(prev.bytes.len() as u32).ok_or(PalwInventoryError::GapOrOverlap {
                        tensor: o.tensor_name.clone(),
                        at: prev.row_start,
                    })?;
                    if end != o.row_start {
                        return Err(PalwInventoryError::GapOrOverlap { tensor: o.tensor_name.clone(), at: end });
                    }
                    continue;
                }
            }
            if o.row_start != 0 {
                return Err(PalwInventoryError::DoesNotStartAtZero { tensor: o.tensor_name.clone() });
            }
        }
        Ok(Self { operands })
    }

    pub fn operands(&self) -> &[PalwArtifactOperandV1] {
        &self.operands
    }

    /// `artifact_root` — the Merkle root over this layout's leaves, in this order.
    pub fn root(&self) -> Hash64 {
        let leaves: Vec<Hash64> = self.operands.iter().map(artifact_leaf_v1).collect();
        artifact_root_v1(&leaves).expect("a non-empty inventory has a root")
    }

    /// **Every tensor the class's graph reads is carried.**
    ///
    /// The layout rules make an inventory internally consistent; this is what ties it to the class.
    /// A registration whose artifact omits a tensor its own profile names is a class whose steps
    /// adjudicate `Unadjudicable` at exactly the nodes that read it — coverage-clean and
    /// unprosecutable, which is the shape ADR-0049 exists to refuse.
    pub fn verify_covers_profile(&self, profile: &crate::palw_step::PalwShapeProfileV3) -> Result<(), PalwInventoryError> {
        for table in [&profile.pre_nodes, &profile.gdn_nodes, &profile.attn_nodes, &profile.post_nodes] {
            for node in table.iter() {
                if node.weight_name.is_empty() {
                    continue;
                }
                let carried = self.operands.iter().any(|o| {
                    // `{layer}` is substituted at interpretation time, so a template matches any
                    // entry whose name agrees outside the placeholder.
                    o.tensor_name == node.weight_name
                        || (node.weight_name.contains("{layer}")
                            && layer_template_matches(node.weight_name.as_str(), o.tensor_name.as_str()))
                });
                if !carried {
                    return Err(PalwInventoryError::ProfileTensorMissing { tensor: node.weight_name.clone() });
                }
            }
        }
        Ok(())
    }
}

/// `blk.{layer}.x` against `blk.7.x` — the placeholder matches one path segment and nothing else,
/// so `blk.{layer}.w` cannot be satisfied by `blk.7.other.w`.
fn layer_template_matches(template: &str, name: &str) -> bool {
    let Some((head, tail)) = template.split_once("{layer}") else { return template == name };
    let Some(rest) = name.strip_prefix(head) else { return false };
    let Some(middle) = rest.strip_suffix(tail) else { return false };
    !middle.is_empty() && middle.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod inventory_tests {
    use super::*;

    fn row(tensor: &str, layer: Option<u16>, row_start: u32, len: usize) -> PalwArtifactOperandV1 {
        PalwArtifactOperandV1 { tensor_name: tensor.to_string(), layer, row_start, bytes: vec![7u8; len] }
    }

    fn good() -> Vec<PalwArtifactOperandV1> {
        vec![
            row("blk.0.w", Some(0), 0, 4),
            row("blk.0.w", Some(0), 4, 4),
            row("blk.1.w", Some(1), 0, 8),
            row("token_embd.weight", None, 0, 16),
        ]
    }

    /// **The rules are what make an opening's ABSENCE mean something.**
    ///
    /// A Merkle opening proves "these bytes are at this position under this root" and says nothing
    /// about what is not opened. Without a pinned layout an artifact can carry one row twice at
    /// different offsets, leave a gap no entry covers, or append bytes nothing describes — and every
    /// individual opening still verifies, so the court sees a consistent artifact that is not the
    /// one the class registered.
    #[test]
    fn a_canonical_inventory_is_the_only_constructible_one() {
        let inv = PalwArtifactInventoryV1::new(good()).expect("a tiled, ordered, gapless layout");
        assert_eq!(inv.operands().len(), 4);
        assert_ne!(inv.root(), Hash64::default(), "and it has a root");

        // Empty: a zero root would verify nothing, forever.
        assert_eq!(PalwArtifactInventoryV1::new(vec![]).unwrap_err(), PalwInventoryError::Empty);

        // A zero-length row is a leaf that binds no bytes.
        let mut z = good();
        z[0].bytes.clear();
        assert!(matches!(PalwArtifactInventoryV1::new(z).unwrap_err(), PalwInventoryError::ZeroLengthRow { .. }));

        // Two leaves for one position let a producer choose which one an opening meets.
        let mut dup = good();
        dup.insert(1, row("blk.0.w", Some(0), 0, 4));
        assert!(matches!(PalwArtifactInventoryV1::new(dup).unwrap_err(), PalwInventoryError::DuplicateRow { .. }));

        // Order is ascending on (name, layer, offset) so that it is an order nobody chooses.
        let mut unordered = good();
        unordered.swap(0, 2);
        assert!(matches!(PalwArtifactInventoryV1::new(unordered).unwrap_err(), PalwInventoryError::NotCanonicalOrder { .. }));

        // A tensor whose first row is not its first byte has a prefix nothing covers.
        let mut late = good();
        late[0].row_start = 4;
        late[1].row_start = 8;
        assert!(matches!(PalwArtifactInventoryV1::new(late).unwrap_err(), PalwInventoryError::DoesNotStartAtZero { .. }));

        // A gap: byte 4..8 of `blk.0.w` is described by nothing.
        let mut gap = good();
        gap[1].row_start = 8;
        assert!(matches!(PalwArtifactInventoryV1::new(gap).unwrap_err(), PalwInventoryError::GapOrOverlap { at: 4, .. }));

        // An overlap: byte 2..4 belongs to two rows, so an opening can prove either.
        let mut over = good();
        over[1].row_start = 2;
        assert!(matches!(PalwArtifactInventoryV1::new(over).unwrap_err(), PalwInventoryError::GapOrOverlap { .. }));
    }

    /// A registration whose artifact omits a tensor its own profile names is coverage-clean and
    /// unprosecutable: every step reading that tensor adjudicates `Unadjudicable`.
    #[test]
    fn an_inventory_must_carry_every_tensor_the_graph_reads() {
        let profile = crate::palw_base0_profile::base0_profile_v1(crate::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .expect("the floor's geometry is expressible");

        // One row per tensor the graph names, layer 0 substituted — enough to satisfy coverage.
        let mut operands: Vec<PalwArtifactOperandV1> = crate::palw_base0_profile::base0_tensor_names_v1()
            .into_iter()
            .map(|t| row(&t.replace("{layer}", "0"), None, 0, 8))
            .collect();
        operands.sort_by(|a, b| (a.tensor_name.as_str(), a.layer, a.row_start).cmp(&(b.tensor_name.as_str(), b.layer, b.row_start)));
        let inv = PalwArtifactInventoryV1::new(operands.clone()).expect("one row per tensor is a legal layout");
        inv.verify_covers_profile(&profile).expect("every tensor the graph reads is carried");

        // Drop the one the residual narrowing reads — the node ADR-0050 A added — and the gate says
        // which tensor is missing rather than leaving it to be found by a dispute.
        let without: Vec<PalwArtifactOperandV1> =
            operands.into_iter().filter(|o| !o.tensor_name.contains("attn_residual")).collect();
        let err = PalwArtifactInventoryV1::new(without)
            .expect("still a legal layout")
            .verify_covers_profile(&profile)
            .expect_err("a graph reading a tensor nobody carries is unprosecutable at that node");
        assert!(matches!(err, PalwInventoryError::ProfileTensorMissing { ref tensor } if tensor.contains("attn_residual")), "got {err:?}");
    }

    /// `{layer}` matches one numeric segment and nothing else, so a template cannot be satisfied by
    /// a tensor that merely starts and ends the same way.
    #[test]
    fn the_layer_placeholder_matches_a_number_and_not_a_path() {
        assert!(layer_template_matches("blk.{layer}.w", "blk.7.w"));
        assert!(layer_template_matches("blk.{layer}.w", "blk.13.w"));
        assert!(!layer_template_matches("blk.{layer}.w", "blk.7.other.w"), "a dot is not a digit");
        assert!(!layer_template_matches("blk.{layer}.w", "blk..w"), "an empty layer is not a layer");
        assert!(!layer_template_matches("blk.{layer}.w", "blk.7.x"));
    }
}
