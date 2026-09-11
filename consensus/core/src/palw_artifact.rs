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
    artifact_leaf_parts_v1(&operand.tensor_name, operand.layer, operand.row_start, &operand.bytes)
}

/// **The same leaf over BORROWED parts** (ADR-0106) — so a row can be hashed where it is read and
/// discarded, instead of being copied into an owned operand first. [`artifact_leaf_v1`] is this.
pub fn artifact_leaf_parts_v1(tensor_name: &str, layer: Option<u16>, row_start: u32, bytes: &[u8]) -> Hash64 {
    let mut hasher = PalwArtifactLeafHasherV1::new(tensor_name, layer, row_start, bytes.len() as u32);
    hasher.update(bytes);
    hasher.finish().expect("the length was the slice's own")
}

/// **A leaf hashed as a STREAM** (ADR-0106): the position and the length first — the preimage
/// carries the length before the bytes — then the bytes in any chunking, so a row larger than any
/// buffer is still one leaf. [`Self::finish`] refuses a stream whose absorbed length is not the
/// declared one, which is the only way a streamed leaf could differ from [`artifact_leaf_parts_v1`].
pub struct PalwArtifactLeafHasherV1 {
    state: blake2b_simd::State,
    declared: u32,
    absorbed: u64,
}

impl PalwArtifactLeafHasherV1 {
    pub fn new(tensor_name: &str, layer: Option<u16>, row_start: u32, byte_len: u32) -> Self {
        let mut state = Params::new().hash_length(64).key(PALW_ARTIFACT_DOMAIN_LEAF).to_state();
        state.update(&(tensor_name.len() as u32).to_le_bytes());
        state.update(tensor_name.as_bytes());
        // `u32::MAX` is the "no layer" marker rather than an absent field, so a graph-level tensor
        // and layer `u32::MAX` cannot collide by one being shorter than the other.
        state.update(&layer.map_or(u32::MAX, u32::from).to_le_bytes());
        state.update(&row_start.to_le_bytes());
        state.update(&byte_len.to_le_bytes());
        Self { state, declared: byte_len, absorbed: 0 }
    }

    pub fn update(&mut self, bytes: &[u8]) {
        self.state.update(bytes);
        self.absorbed += bytes.len() as u64;
    }

    /// The leaf, or the refusal naming both lengths.
    pub fn finish(self) -> Result<Hash64, (u32, u64)> {
        if self.absorbed != u64::from(self.declared) {
            return Err((self.declared, self.absorbed));
        }
        let mut out = [0u8; 64];
        out.copy_from_slice(self.state.finalize().as_bytes());
        Ok(Hash64::from_bytes(out))
    }
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

/// **The same root as a STREAM** (ADR-0106): one peak per level, a leaf pushed as a binary carry,
/// and the peaks folded from the LOWEST level up at the end — which is exactly what promotion does,
/// because an unpaired node is carried up untouched until something above pairs with it. No leaf
/// vector and no level copy; pinned against [`artifact_root_v1`] for every size.
#[derive(Clone, Debug, Default)]
pub struct PalwArtifactMerkleFrontierV1 {
    peaks: Vec<Option<Hash64>>,
    count: u64,
}

impl PalwArtifactMerkleFrontierV1 {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, leaf: Hash64) {
        let mut carry = leaf;
        let mut level = 0;
        loop {
            if level == self.peaks.len() {
                self.peaks.push(None);
            }
            match self.peaks[level].take() {
                Some(left) => {
                    carry = node(&left, &carry);
                    level += 1;
                }
                None => {
                    self.peaks[level] = Some(carry);
                    break;
                }
            }
        }
        self.count += 1;
    }

    pub fn leaf_count(&self) -> u64 {
        self.count
    }

    /// `None` for an empty stream, as [`artifact_root_v1`] answers an empty inventory.
    pub fn root(&self) -> Option<Hash64> {
        let mut acc: Option<Hash64> = None;
        for peak in self.peaks.iter().flatten() {
            acc = Some(match acc {
                None => *peak,
                Some(right) => node(peak, &right),
            });
        }
        acc
    }
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
            path.push(if at.is_multiple_of(2) { level[at + 1] } else { level[at - 1] });
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
            acc = if index.is_multiple_of(2) { node(&acc, sibling) } else { node(sibling, &acc) };
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
        find_operand_v1(&self.operands, tensor_name, layer, byte_offset, byte_len).map(|(_, bytes)| bytes)
    }
}

/// **The one lookup an oracle performs**, shared by the verifier's proven set and the prover's
/// recorder so the two cannot answer the same question differently.
///
/// The proof binds the bytes that were committed; it says nothing about how many the caller wants.
/// A short opening is a missing operand, not a truncated answer — and a LONG one is refused too,
/// because an opening that proves more than the step reads is an opening whose extra bytes nothing
/// checked.
pub fn find_operand_v1(
    operands: &[PalwArtifactOperandV1],
    tensor_name: &str,
    layer: Option<u16>,
    byte_offset: u32,
    byte_len: u32,
) -> Option<(usize, Vec<u8>)> {
    let (index, operand) =
        operands.iter().enumerate().find(|(_, o)| o.tensor_name == tensor_name && o.layer == layer && o.row_start == byte_offset)?;
    (operand.bytes.len() == byte_len as usize).then(|| (index, operand.bytes.clone()))
}

/// **An oracle over a FULL inventory that remembers which operands were asked for.**
///
/// The prover's problem is not "what does this step read" — it is "what will the ADJUDICATOR ask
/// for", and those are the same question only if two pieces of code agree. Writing the enumeration
/// a second time on the prover side is exactly the correspondence defect this tree keeps finding,
/// so the prover does not write it: it runs the adjudicator against the whole inventory, records
/// every row the adjudicator actually resolved, and opens those.
///
/// The recorded set is therefore correct BY CONSTRUCTION for any op kind, present or future — a
/// new kernel that reads a new tensor needs no change here, because nothing here knows what a
/// kernel is.
pub struct PalwRecordingOracleV1<'a> {
    inventory: &'a [PalwArtifactOperandV1],
    used: std::cell::RefCell<std::collections::BTreeSet<usize>>,
}

impl<'a> PalwRecordingOracleV1<'a> {
    pub fn new(inventory: &'a [PalwArtifactOperandV1]) -> Self {
        Self { inventory, used: Default::default() }
    }

    /// The openings for every operand the adjudicator resolved, in inventory order.
    ///
    /// `None` when any recorded index cannot be opened, which cannot happen for an inventory this
    /// oracle answered from and is therefore a corrupt inventory rather than a missing operand.
    pub fn openings(&self) -> Option<Vec<PalwArtifactOpeningV1>> {
        self.used.borrow().iter().map(|i| open_artifact_leaf_v1(self.inventory, *i as u32)).collect()
    }

    /// How many rows were resolved — the cost half of the same answer, for a caller sizing a close
    /// against the court's opening ceiling before it builds one.
    pub fn recorded(&self) -> usize {
        self.used.borrow().len()
    }
}

impl crate::palw_step_refute::PalwWeightOracleV1 for PalwRecordingOracleV1<'_> {
    fn operand_bytes(&self, tensor_name: &str, layer: Option<u16>, byte_offset: u32, byte_len: u32) -> Option<Vec<u8>> {
        let (index, bytes) = find_operand_v1(self.inventory, tensor_name, layer, byte_offset, byte_len)?;
        self.used.borrow_mut().insert(index);
        Some(bytes)
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

    /// **What the prover opens is what the verifier resolves, because the prover asked the
    /// verifier.**
    ///
    /// The production close carried `operand_openings: Vec::new()`, so no step that reads a weight
    /// could adjudicate in either direction — the court could not convict a liar and could not
    /// acquit an honest producer. Building the set by hand on the prover side would mean writing
    /// the adjudicator's operand enumeration twice, and every correspondence defect in this tree
    /// has come from exactly that. So the prover runs the adjudicator against the whole inventory
    /// and opens what it recorded.
    ///
    /// This pins the round trip: what the recorder collects verifies against the registered root,
    /// and a `PalwProvenOperandsV1` rebuilt from those openings answers the SAME queries with the
    /// SAME bytes. Anything the recorder missed shows up here as a `None` from the proven set.
    #[test]
    fn a_recorded_opening_set_answers_exactly_what_the_verifier_asks() {
        let inv = inventory();
        let leaves: Vec<Hash64> = inv.iter().map(artifact_leaf_v1).collect();
        let root = artifact_root_v1(&leaves).unwrap();

        // The queries a step makes. The recorder does not know they are coming; it learns them.
        let queries: [(&str, Option<u16>, u32, u32); 2] =
            [("blk.{layer}.attn_q.weight", Some(0), 0, 4), ("blk.{layer}.ffn_up.weight", Some(1), 0, 2)];

        let recorder = PalwRecordingOracleV1::new(&inv);
        let mut expected: Vec<Vec<u8>> = Vec::new();
        for (name, layer, off, len) in queries {
            expected.push(recorder.operand_bytes(name, layer, off, len).expect("the full inventory answers"));
        }
        assert_eq!(recorder.recorded(), 2, "two rows resolved, and the untouched third is not opened");

        let openings = recorder.openings().expect("every recorded row opens");
        for opening in &openings {
            verify_artifact_opening_v1(opening, root).expect("a recorded opening verifies against the registered root");
        }

        // The verifier's side, built from exactly those openings.
        let proven = PalwProvenOperandsV1::from_openings_v1(&openings, root).expect("the openings compose");
        for ((name, layer, off, len), want) in queries.into_iter().zip(expected) {
            assert_eq!(
                proven.operand_bytes(name, layer, off, len),
                Some(want),
                "the proven set must answer what the recorder answered, byte for byte"
            );
        }

        // And nothing else: a row the step never read is not carried, so the close does not pay for
        // bytes no refutation reads.
        assert_eq!(
            proven.operand_bytes("blk.{layer}.attn_k.weight", Some(0), 0, 4),
            None,
            "an operand the adjudicator never asked for must not be in the close"
        );
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
        (PalwArtifactOpeningV1 { operand: inv[index].clone(), leaf_index: index as u32, leaf_count: 3, path }, root)
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
        assert_eq!(PalwProvenOperandsV1::from_openings_v1(&[good, bad], root).err(), Some(PalwArtifactError::RootMismatch));
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
    #[error(
        "'{tensor}' is out of canonical order at index {index}: (name, layer, offset) ascending is what makes the order one nobody chooses"
    )]
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

/// **The layout rules, one row at a time — their one spelling** (ADR-0106). A row is its
/// `(tensor, layer, byte offset, byte length)`; the held inventory, the digest one and a stream are
/// all checked by this walk, so a streamed inventory is refused by the same rule, at the same row,
/// with the same error as the held one — "the stream is the same inventory" cannot drift into "the
/// stream is a similar inventory".
#[derive(Clone, Debug, Default)]
pub struct PalwInventoryLayoutCheckerV1 {
    prev: Option<(String, Option<u16>, u32, u32)>,
    index: usize,
}

impl PalwInventoryLayoutCheckerV1 {
    pub fn new() -> Self {
        Self::default()
    }

    /// The next row, refused by name if it breaks a rule given the rows before it.
    pub fn push(&mut self, name: &str, layer: Option<u16>, start: u32, len: u32) -> Result<(), PalwInventoryError> {
        let index = self.index;
        if len == 0 {
            return Err(PalwInventoryError::ZeroLengthRow { tensor: name.to_string(), index });
        }
        let mut continues_its_tensor = false;
        if let Some((p_name, p_layer, p_start, p_len)) = &self.prev {
            let (pk, ok) = ((p_name.as_str(), *p_layer, *p_start), (name, layer, start));
            if pk == ok {
                return Err(PalwInventoryError::DuplicateRow { tensor: name.to_string(), offset: start });
            }
            if pk > ok {
                return Err(PalwInventoryError::NotCanonicalOrder { tensor: name.to_string(), index });
            }
            // Within one tensor the rows must tile it: the next row starts exactly where the
            // previous ended. Across tensors the previous end says nothing.
            if (p_name.as_str(), *p_layer) == (name, layer) {
                let end =
                    p_start.checked_add(*p_len).ok_or(PalwInventoryError::GapOrOverlap { tensor: name.to_string(), at: *p_start })?;
                if end != start {
                    return Err(PalwInventoryError::GapOrOverlap { tensor: name.to_string(), at: end });
                }
                continues_its_tensor = true;
            }
        }
        if !continues_its_tensor && start != 0 {
            return Err(PalwInventoryError::DoesNotStartAtZero { tensor: name.to_string() });
        }
        match &mut self.prev {
            Some((p_name, p_layer, p_start, p_len)) if p_name.as_str() == name => (*p_layer, *p_start, *p_len) = (layer, start, len),
            _ => self.prev = Some((name.to_string(), layer, start, len)),
        }
        self.index += 1;
        Ok(())
    }

    /// The rows checked so far, or `Empty` when there were none — an inventory of nothing opens
    /// nothing.
    pub fn finish(&self) -> Result<usize, PalwInventoryError> {
        if self.index == 0 { Err(PalwInventoryError::Empty) } else { Ok(self.index) }
    }
}

/// The walk over a held slice — the checker, row by row.
fn check_inventory_layout_v1<T>(rows: &[T], at: impl Fn(&T) -> (&str, Option<u16>, u32, u32)) -> Result<(), PalwInventoryError> {
    if rows.is_empty() {
        return Err(PalwInventoryError::Empty);
    }
    let mut checker = PalwInventoryLayoutCheckerV1::new();
    for row in rows {
        let (name, layer, start, len) = at(row);
        checker.push(name, layer, start, len)?;
    }
    Ok(())
}

impl PalwArtifactInventoryV1 {
    /// Check the layout, then keep it. Every rule refuses a way an opening's absence could be made
    /// to mean nothing.
    pub fn new(operands: Vec<PalwArtifactOperandV1>) -> Result<Self, PalwInventoryError> {
        check_inventory_layout_v1(&operands, |o| (o.tensor_name.as_str(), o.layer, o.row_start, o.bytes.len() as u32))?;
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

    /// The three numbers a streamed build is held to (ADR-0106), computed the materialized way.
    pub fn summary(&self) -> PalwArtifactInventorySummaryV1 {
        PalwArtifactInventorySummaryV1 {
            root: self.root(),
            leaf_count: self.operands.len() as u32,
            artifact_bytes: self.operands.iter().map(|o| o.bytes.len() as u64).sum(),
        }
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

/// **One inventory row without its bytes** (ADR-0106): the coordinate, the length and the leaf the
/// bytes hashed to — what a measurement, a root and a placement need, and all a stream keeps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwArtifactRowDigestV1 {
    pub tensor_name: String,
    pub layer: Option<u16>,
    pub row_start: u32,
    pub byte_len: u32,
    pub leaf_hash: Hash64,
}

impl PalwArtifactRowDigestV1 {
    /// The row a materialized inventory holds, digested — the equality the stream is held to.
    pub fn of(operand: &PalwArtifactOperandV1) -> Self {
        Self {
            tensor_name: operand.tensor_name.clone(),
            layer: operand.layer,
            row_start: operand.row_start,
            byte_len: operand.bytes.len() as u32,
            leaf_hash: artifact_leaf_v1(operand),
        }
    }
}

/// **What a measurement and a registration read of an inventory** (ADR-0106): its root, its leaf
/// count and the bytes its rows cover — the three numbers the materialized and the streamed build
/// must agree on byte for byte (a root that moves between them is a defect, never a migration).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwArtifactInventorySummaryV1 {
    pub root: Hash64,
    pub leaf_count: u32,
    pub artifact_bytes: u64,
}

/// **An inventory consumed as a stream** (ADR-0106): rows arriving in canonical order, each checked
/// by the one layout rule, hashed into the frontier and counted — the summary, with nothing kept
/// per row. This is what makes a whole-artifact measurement hold one read, not the artifact.
#[derive(Clone, Debug, Default)]
pub struct PalwArtifactInventoryStreamV1 {
    checker: PalwInventoryLayoutCheckerV1,
    frontier: PalwArtifactMerkleFrontierV1,
    bytes: u64,
}

impl PalwArtifactInventoryStreamV1 {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, tensor_name: &str, layer: Option<u16>, row_start: u32, bytes: &[u8]) -> Result<(), PalwInventoryError> {
        self.checker.push(tensor_name, layer, row_start, bytes.len() as u32)?;
        self.frontier.push(artifact_leaf_parts_v1(tensor_name, layer, row_start, bytes));
        self.bytes += bytes.len() as u64;
        Ok(())
    }

    pub fn finish(self) -> Result<PalwArtifactInventorySummaryV1, PalwInventoryError> {
        let leaf_count = self.checker.finish()? as u32;
        Ok(PalwArtifactInventorySummaryV1 {
            root: self.frontier.root().expect("a non-empty stream has a root"),
            leaf_count,
            artifact_bytes: self.bytes,
        })
    }
}

/// **The canonical layout, as leaves** (ADR-0106): the same rows [`PalwArtifactInventoryV1`] holds,
/// in the same order, under the same checks — with each row's bytes hashed where they were read and
/// never kept. Its root IS the materialized inventory's; an opening is built from it by handing back
/// the one row's bytes ([`Self::opening_v1`]), which are checked against the leaf, so the court's
/// wire evidence is unchanged and produced on demand rather than retained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwArtifactInventoryDigestV1 {
    rows: Vec<PalwArtifactRowDigestV1>,
}

impl PalwArtifactInventoryDigestV1 {
    /// The rows must already be in canonical order — the materialized constructor's rule.
    pub fn new(rows: Vec<PalwArtifactRowDigestV1>) -> Result<Self, PalwInventoryError> {
        check_inventory_layout_v1(&rows, |r| (r.tensor_name.as_str(), r.layer, r.row_start, r.byte_len))?;
        Ok(Self { rows })
    }

    pub fn rows(&self) -> &[PalwArtifactRowDigestV1] {
        &self.rows
    }

    pub fn leaf_count(&self) -> u32 {
        self.rows.len() as u32
    }

    pub fn summary(&self) -> PalwArtifactInventorySummaryV1 {
        PalwArtifactInventorySummaryV1 {
            root: self.root(),
            leaf_count: self.leaf_count(),
            artifact_bytes: self.rows.iter().map(|r| u64::from(r.byte_len)).sum(),
        }
    }

    /// `artifact_root`, through the streaming frontier: no leaf vector, no level copy.
    pub fn root(&self) -> Hash64 {
        let mut frontier = PalwArtifactMerkleFrontierV1::new();
        for row in &self.rows {
            frontier.push(row.leaf_hash);
        }
        frontier.root().expect("a non-empty inventory has a root")
    }

    /// The index of the row at `(tensor, layer, byte offset)`, by the canonical order.
    pub fn index_of(&self, tensor_name: &str, layer: Option<u16>, row_start: u32) -> Option<u32> {
        self.rows
            .binary_search_by(|r| (r.tensor_name.as_str(), r.layer, r.row_start).cmp(&(tensor_name, layer, row_start)))
            .ok()
            .map(|i| i as u32)
    }

    /// **An opening built from the digest and the row's own bytes** — the bytes must hash to the
    /// recorded leaf, so the opening is the materialized inventory's
    /// [`open_artifact_leaf_v1`] output exactly. `None` for an index outside the inventory or
    /// bytes that are not that row's.
    pub fn opening_v1(&self, index: u32, bytes: Vec<u8>) -> Option<PalwArtifactOpeningV1> {
        let row = self.rows.get(index as usize)?;
        if row.byte_len as usize != bytes.len()
            || artifact_leaf_parts_v1(&row.tensor_name, row.layer, row.row_start, &bytes) != row.leaf_hash
        {
            return None;
        }
        let mut level: Vec<Hash64> = self.rows.iter().map(|r| r.leaf_hash).collect();
        let leaf_count = level.len() as u32;
        let mut at = index as usize;
        let mut path = Vec::new();
        while level.len() > 1 {
            let promoted = at == level.len() - 1 && level.len() % 2 == 1;
            if !promoted {
                path.push(if at.is_multiple_of(2) { level[at + 1] } else { level[at - 1] });
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
        Some(PalwArtifactOpeningV1 {
            operand: PalwArtifactOperandV1 { tensor_name: row.tensor_name.clone(), layer: row.layer, row_start: row.row_start, bytes },
            leaf_index: index,
            leaf_count,
            path,
        })
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
mod streaming_tests {
    use super::*;

    fn op(name: &str, layer: Option<u16>, start: u32, bytes: &[u8]) -> PalwArtifactOperandV1 {
        PalwArtifactOperandV1 { tensor_name: name.to_string(), layer, row_start: start, bytes: bytes.to_vec() }
    }

    /// **ADR-0106 W1: the parts leaf and the streamed leaf are the operand leaf**, and a stream
    /// that absorbed a different length than it declared is refused rather than hashed.
    #[test]
    fn the_parts_leaf_and_the_streamed_leaf_are_the_operand_leaf() {
        let bytes: Vec<u8> = (0..1000u32).map(|i| (i * 7 + 3) as u8).collect();
        for (name, layer, start) in [("blk.{layer}.attn_q.weight", Some(3u16), 4096u32), ("token_embd.weight", None, 0)] {
            let whole = artifact_leaf_v1(&op(name, layer, start, &bytes));
            assert_eq!(artifact_leaf_parts_v1(name, layer, start, &bytes), whole);
            let mut streamed = PalwArtifactLeafHasherV1::new(name, layer, start, bytes.len() as u32);
            for chunk in bytes.chunks(97) {
                streamed.update(chunk);
            }
            assert_eq!(streamed.finish(), Ok(whole));
        }
        let mut short = PalwArtifactLeafHasherV1::new("t", None, 0, 10);
        short.update(&[1, 2, 3]);
        assert_eq!(short.finish(), Err((10, 3)));
    }

    /// **ADR-0106 W7: the frontier's root is `artifact_root_v1`'s, at every size** — promotion at
    /// every level is exercised by the sizes below 300 and a few past powers of two.
    #[test]
    fn the_frontier_root_is_the_promoting_root_at_every_size() {
        let leaves: Vec<Hash64> = (0..1100u64).map(Hash64::from_u64_word).collect();
        let mut frontier = PalwArtifactMerkleFrontierV1::new();
        assert_eq!(frontier.root(), None);
        for n in 1..=leaves.len() {
            frontier.push(leaves[n - 1]);
            if n <= 300 || n.is_power_of_two() || (n + 1).is_power_of_two() || n == leaves.len() {
                assert_eq!(frontier.root(), artifact_root_v1(&leaves[..n]), "n = {n}");
            }
        }
        assert_eq!(frontier.leaf_count(), leaves.len() as u64);
    }

    /// **ADR-0106 W2: the digest inventory is the materialized one** — the same checks refuse the
    /// same layouts, the root is the same root, and an opening from the digest plus the row's bytes
    /// is the materialized opening byte for byte (and bytes that are not the row's open nothing).
    #[test]
    fn the_digest_inventory_roots_checks_and_opens_as_the_materialized_one() {
        let rows = vec![
            op("a.weight", Some(0), 0, &[1, 2, 3]),
            op("a.weight", Some(0), 3, &[4, 5]),
            op("a.weight", Some(1), 0, &[6]),
            op("b.a16", None, 0, &[7, 8, 9, 10]),
            op("z", None, 0, &[11]),
        ];
        let full = PalwArtifactInventoryV1::new(rows.clone()).expect("a canonical layout");
        let digest =
            PalwArtifactInventoryDigestV1::new(rows.iter().map(PalwArtifactRowDigestV1::of).collect()).expect("the same layout");
        assert_eq!(digest.root(), full.root());
        assert_eq!(digest.leaf_count() as usize, full.operands().len());
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(digest.index_of(&row.tensor_name, row.layer, row.row_start), Some(i as u32));
            assert_eq!(digest.opening_v1(i as u32, row.bytes.clone()), open_artifact_leaf_v1(full.operands(), i as u32), "row {i}");
        }
        assert_eq!(digest.opening_v1(0, vec![9, 9, 9]), None, "bytes that are not the row's open nothing");
        // The same refusals, from the one walk.
        let mut gap = rows.clone();
        gap[1].row_start = 4;
        assert_eq!(
            PalwArtifactInventoryDigestV1::new(gap.iter().map(PalwArtifactRowDigestV1::of).collect()),
            Err(PalwArtifactInventoryV1::new(gap).expect_err("a gap"))
        );
        let mut disorder = rows.clone();
        disorder.swap(3, 4);
        assert_eq!(
            PalwArtifactInventoryDigestV1::new(disorder.iter().map(PalwArtifactRowDigestV1::of).collect()),
            Err(PalwArtifactInventoryV1::new(disorder.clone()).expect_err("out of order"))
        );
        // And a STREAM of the same rows: the same summary when canonical, the same refusal when not.
        let stream = |rows: &[PalwArtifactOperandV1]| -> Result<PalwArtifactInventorySummaryV1, PalwInventoryError> {
            let mut s = PalwArtifactInventoryStreamV1::new();
            for o in rows {
                s.push(&o.tensor_name, o.layer, o.row_start, &o.bytes)?;
            }
            s.finish()
        };
        assert_eq!(stream(&rows), Ok(full.summary()));
        assert_eq!(stream(&disorder), Err(PalwArtifactInventoryV1::new(disorder).expect_err("out of order")));
        assert_eq!(stream(&[]), Err(PalwInventoryError::Empty));
        let mut zero = rows.clone();
        zero[2].bytes.clear();
        assert_eq!(stream(&zero), Err(PalwArtifactInventoryV1::new(zero).expect_err("a zero-length row")));
    }
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
        let without: Vec<PalwArtifactOperandV1> = operands.into_iter().filter(|o| !o.tensor_name.contains("attn_residual")).collect();
        let err = PalwArtifactInventoryV1::new(without)
            .expect("still a legal layout")
            .verify_covers_profile(&profile)
            .expect_err("a graph reading a tensor nobody carries is unprosecutable at that node");
        assert!(
            matches!(err, PalwInventoryError::ProfileTensorMissing { ref tensor } if tensor.contains("attn_residual")),
            "got {err:?}"
        );
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
