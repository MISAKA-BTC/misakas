//! **Segmented claims (K2-TIR-v4): position, segment and claim roots, the segmented evidence object, and prompt tiles**
//! (`docs/design/palw/k2-real-scale.md` §1–§2, §5).
//!
//! ```text
//! node leaf     = H(POS_LEAF; occurrence u16, node u16, C_v3(value))       canonical (occurrence, node) order, derived values included
//! position root = H(POS_ROOT; p u32, N u64, merkle(node leaves))
//! segment leaf  = H(SEG_LEAF; p u32, position root)
//! segment root  = H(SEG_ROOT; index u32, first u32, end u32, merkle(segment leaves))      segments of SEG_LEN_V4 positions
//! claim root    = H(CLAIM_ROOT; positions u32, segment len u32, count u32, merkle(segment roots))
//! ```
//!
//! A claim carries its **segment roots** on chain (`O(P / 1,024)` digests); position roots and node commitments are material a
//! producer serves (off-chain, or on chain in answer to a demand). A [`NodeOpeningV1`] authenticates one node commitment against
//! the on-chain segment roots with `⌈log2 N⌉ + 10` siblings.
//!
//! The job's input is bound by [`job_input_root_v2`] over the prompt's **tile root** ([`prompt_root_of_ids_v1`]), never the prompt's
//! ids, so a 262,144- or 2,097,152-id prompt costs a claim nothing: the prompt is posted on chain in tiles of
//! [`PROMPT_TILE_IDS_V1`] ids ([`TiledJobV1`], [`PromptTileOpeningV1`]) and a court opens the one tile it reads.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::evidence::{EvidenceHeaderV1, SuiteParamsV1};
use crate::hash::{Digest, finish, keyed, object_id};
use crate::job::DecodeRuleV1;
use crate::merkle3::{tree_path, tree_root, tree_root_from_path};

pub const SEG_POS_LEAF_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/seg/position-leaf/v1";
pub const SEG_POS_NODE_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/seg/position-node/v1";
pub const SEG_POS_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/seg/position-root/v1";
pub const SEG_SEG_LEAF_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/seg/segment-leaf/v1";
pub const SEG_SEG_NODE_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/seg/segment-node/v1";
pub const SEG_SEG_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/seg/segment-root/v1";
pub const SEG_CLAIM_NODE_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/seg/claim-node/v1";
pub const SEG_CLAIM_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/seg/claim-root/v1";
pub const SEG_EVIDENCE_DOMAIN_V2: &[u8] = b"misaka-palw/kernel/seg/verification-evidence/v2";
pub const JOB_INPUT_DOMAIN_V2: &[u8] = b"misaka-palw/kernel/job-input/v2";
pub const PROMPT_TILE_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/prompt-tile/v1";
pub const PROMPT_NODE_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/prompt-node/v1";
pub const PROMPT_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/prompt-root/v1";
pub const TILED_JOB_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/tiled-job/v1";

/// The evidence object's version for a segmented claim.
pub const SEG_EVIDENCE_VERSION_V2: u16 = 2;
/// Positions per segment (the last segment may be shorter).
pub const SEG_LEN_V4: u32 = 1024;
/// Ids per prompt tile (16 KiB of `u32`).
pub const PROMPT_TILE_IDS_V1: usize = 4096;
/// The most segments a segmented claim may carry (2^21 positions): a parse bound.
pub const SEG_MAX_SEGMENTS_V4: usize = (1 << 21) / SEG_LEN_V4 as usize;

fn node(domain: &[u8], l: &Digest, r: &Digest) -> Digest {
    let mut s = keyed(domain);
    s.update(l).update(r);
    finish(s)
}

fn pos_node(l: &Digest, r: &Digest) -> Digest {
    node(SEG_POS_NODE_DOMAIN_V1, l, r)
}

pub(crate) fn seg_node(l: &Digest, r: &Digest) -> Digest {
    node(SEG_SEG_NODE_DOMAIN_V1, l, r)
}

fn claim_node(l: &Digest, r: &Digest) -> Digest {
    node(SEG_CLAIM_NODE_DOMAIN_V1, l, r)
}

fn prompt_node(l: &Digest, r: &Digest) -> Digest {
    node(PROMPT_NODE_DOMAIN_V1, l, r)
}

/// A position tree's leaf.
pub fn node_leaf_v1(occurrence: u16, node: u16, commitment: &Digest) -> Digest {
    let mut s = keyed(SEG_POS_LEAF_DOMAIN_V1);
    s.update(&occurrence.to_le_bytes()).update(&node.to_le_bytes()).update(commitment);
    finish(s)
}

fn position_root_of_tree(p: u32, count: u64, tree: &Digest) -> Digest {
    let mut s = keyed(SEG_POS_ROOT_DOMAIN_V1);
    s.update(&p.to_le_bytes()).update(&count.to_le_bytes()).update(tree);
    finish(s)
}

pub(crate) fn segment_leaf_v1(p: u32, position_root: &Digest) -> Digest {
    let mut s = keyed(SEG_SEG_LEAF_DOMAIN_V1);
    s.update(&p.to_le_bytes()).update(position_root);
    finish(s)
}

fn segment_root_of_tree(index: u32, first: u32, end: u32, tree: &Digest) -> Digest {
    let mut s = keyed(SEG_SEG_ROOT_DOMAIN_V1);
    s.update(&index.to_le_bytes()).update(&first.to_le_bytes()).update(&end.to_le_bytes()).update(tree);
    finish(s)
}

/// `[first, end)` of segment `index` of a claim of `positions`.
pub fn segment_bounds_v1(positions: u32, index: u32) -> Option<(u32, u32)> {
    let first = index.checked_mul(SEG_LEN_V4)?;
    (first < positions).then(|| (first, first.saturating_add(SEG_LEN_V4).min(positions)))
}

/// How many segments a claim of `positions` has.
pub fn segment_count_v1(positions: u32) -> u32 {
    positions.div_ceil(SEG_LEN_V4)
}

/// **The claim root** over the segment roots.
pub fn claim_root_v1(positions: u32, segment_roots: &[Digest]) -> Digest {
    let leaves: Vec<Digest> = segment_roots.to_vec();
    let tree = tree_root(leaves, claim_node, SEG_CLAIM_NODE_DOMAIN_V1);
    let mut s = keyed(SEG_CLAIM_ROOT_DOMAIN_V1);
    s.update(&positions.to_le_bytes())
        .update(&SEG_LEN_V4.to_le_bytes())
        .update(&(segment_roots.len() as u32).to_le_bytes())
        .update(&tree);
    finish(s)
}

/// **A claim's segment roots from its position roots alone** — what a verifier that re-executed the claim compares with the on-chain
/// roots (`crate::seg_detect`): it needs no node commitment of anyone's.
pub fn segment_roots_of_position_roots_v1(position_roots: &[Digest]) -> Vec<Digest> {
    let positions = position_roots.len() as u32;
    (0..segment_count_v1(positions))
        .map(|index| {
            let (first, end) = segment_bounds_v1(positions, index).expect("a segment of the claim");
            let leaves = (first..end).map(|q| segment_leaf_v1(q, &position_roots[q as usize])).collect();
            segment_root_of_tree(index, first, end, &tree_root(leaves, seg_node, SEG_SEG_NODE_DOMAIN_V1))
        })
        .collect()
}

/// **A producer's segmented commitments**: every node commitment of every position, `commitments[p][occurrence][node]`
/// (v3 tensor commitments), and the trees over them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SegmentedCommitmentsV1 {
    pub commitments: Vec<Vec<Vec<Digest>>>,
    /// Every position's root, in order.
    pub position_roots: Vec<Digest>,
}

impl SegmentedCommitmentsV1 {
    pub fn new(commitments: Vec<Vec<Vec<Digest>>>) -> Self {
        let position_roots = commitments.iter().enumerate().map(|(p, c)| position_root_of_v1(p as u32, c)).collect();
        Self { commitments, position_roots }
    }

    pub fn positions(&self) -> u32 {
        self.commitments.len() as u32
    }

    pub fn position_root(&self, p: u32) -> Digest {
        self.position_roots[p as usize]
    }

    fn segment_leaves(&self, index: u32) -> Vec<Digest> {
        let (first, end) = segment_bounds_v1(self.positions(), index).expect("a segment of the claim");
        (first..end).map(|q| segment_leaf_v1(q, &self.position_root(q))).collect()
    }

    pub fn segment_root(&self, index: u32) -> Digest {
        let (first, end) = segment_bounds_v1(self.positions(), index).expect("a segment of the claim");
        segment_root_of_tree(index, first, end, &tree_root(self.segment_leaves(index), seg_node, SEG_SEG_NODE_DOMAIN_V1))
    }

    pub fn segment_roots(&self) -> Vec<Digest> {
        (0..segment_count_v1(self.positions())).map(|i| self.segment_root(i)).collect()
    }

    pub fn claim_root(&self) -> Digest {
        claim_root_v1(self.positions(), &self.segment_roots())
    }

    /// Position `p`'s root and its path to its segment root.
    pub fn position_path(&self, p: u32) -> (Digest, Vec<Digest>) {
        let index = p / SEG_LEN_V4;
        let (first, _) = segment_bounds_v1(self.positions(), index).expect("a position of the claim");
        (self.position_root(p), tree_path(self.segment_leaves(index), (p - first) as usize, seg_node))
    }

    /// **The opening of node `(p, s, n)`'s commitment** against the claim's segment roots.
    pub fn node_opening(&self, p: u32, s: u16, n: u16) -> Option<NodeOpeningV1> {
        let (_, siblings) = self.position_path(p);
        position_node_opening_v1(p, self.commitments.get(p as usize)?, siblings, s, n)
    }
}

fn node_leaves_of(commitments: &[Vec<Digest>]) -> Vec<Digest> {
    let mut out = Vec::new();
    for (s, occ) in commitments.iter().enumerate() {
        for (n, c) in occ.iter().enumerate() {
            out.push(node_leaf_v1(s as u16, n as u16, c));
        }
    }
    out
}

/// **A position's root** from its node commitments `[occurrence][node]`.
pub fn position_root_of_v1(p: u32, commitments: &[Vec<Digest>]) -> Digest {
    let leaves = node_leaves_of(commitments);
    let count = leaves.len() as u64;
    position_root_of_tree(p, count, &tree_root(leaves, pos_node, SEG_POS_NODE_DOMAIN_V1))
}

/// **The opening of `(s, n)` at position `p`** from the position's node commitments and the position root's path to its segment root.
pub fn position_node_opening_v1(
    p: u32,
    commitments: &[Vec<Digest>],
    position_siblings: Vec<Digest>,
    s: u16,
    n: u16,
) -> Option<NodeOpeningV1> {
    let commitment = *commitments.get(s as usize)?.get(n as usize)?;
    let index = node_index(commitments, s, n)?;
    Some(NodeOpeningV1 {
        position: p,
        occurrence: s,
        node: n,
        commitment,
        node_siblings: tree_path(node_leaves_of(commitments), index as usize, pos_node),
        position_root: position_root_of_v1(p, commitments),
        position_siblings,
    })
}

/// The flat index of `(s, n)` among a position's committed values.
fn node_index(occ: &[Vec<Digest>], s: u16, n: u16) -> Option<u64> {
    let row = occ.get(s as usize)?;
    if n as usize >= row.len() {
        return None;
    }
    let before: usize = occ[..s as usize].iter().map(Vec::len).sum();
    Some((before + n as usize) as u64)
}

/// **One node commitment, authenticated against a claim's segment roots** (the shape of a position and the program's node count are
/// the verifier's: `node_index` and `node_count` come from the program, never from the opening).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NodeOpeningV1 {
    pub position: u32,
    pub occurrence: u16,
    pub node: u16,
    pub commitment: Digest,
    pub node_siblings: Vec<Digest>,
    pub position_root: Digest,
    pub position_siblings: Vec<Digest>,
}

impl NodeOpeningV1 {
    /// Does this opening put `commitment` at `(position, occurrence, node)` of the claim? `node_index` is the flat index of
    /// `(occurrence, node)` among the program's `node_count` committed values a position.
    pub fn authenticates(&self, segment_roots: &[Digest], positions: u32, node_index: u64, node_count: u64) -> bool {
        self.position_root_ok(node_index, node_count)
            && position_in_segment(self.position, &self.position_root, &self.position_siblings, segment_roots, positions)
    }

    fn position_root_ok(&self, node_index: u64, node_count: u64) -> bool {
        let leaf = node_leaf_v1(self.occurrence, self.node, &self.commitment);
        tree_root_from_path(leaf, node_index, node_count, &self.node_siblings, pos_node)
            .is_some_and(|tree| position_root_of_tree(self.position, node_count, &tree) == self.position_root)
    }

    pub fn byte_len(&self) -> u64 {
        8 + 64 * (2 + self.node_siblings.len() as u64 + self.position_siblings.len() as u64) + 16
    }
}

/// **Does `position_root` sit at `p` of the claim** (its path to the on-chain segment root)?
pub fn position_in_segment(p: u32, position_root: &Digest, siblings: &[Digest], segment_roots: &[Digest], positions: u32) -> bool {
    let index = p / SEG_LEN_V4;
    let Some((first, end)) = segment_bounds_v1(positions, index) else { return false };
    let Some(segment_root) = segment_roots.get(index as usize) else { return false };
    tree_root_from_path(segment_leaf_v1(p, position_root), (p - first) as u64, (end - first) as u64, siblings, seg_node)
        .is_some_and(|tree| segment_root_of_tree(index, first, end, &tree) == *segment_root)
}

/// The position tree's depth for `node_count` committed values (an opening's sibling count).
pub fn node_path_len_v1(node_count: u64) -> u64 {
    crate::merkle::depth(node_count)
}

/// **The evidence object of a segmented claim** (`docs/design/palw/k2-real-scale.md` §1.3): the header, the job's input, the positions,
/// the claim root over the segment roots and the suite. No state or output root: continuity is by wiring, the output by the decode court.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SegmentedEvidenceV2 {
    pub version: u16,
    pub header: EvidenceHeaderV1,
    pub job_input_root: Digest,
    pub positions: u32,
    pub segment_len: u32,
    pub claim_root: Digest,
    pub suite: SuiteParamsV1,
}

impl SegmentedEvidenceV2 {
    pub fn root(&self) -> Digest {
        object_id(SEG_EVIDENCE_DOMAIN_V2, self)
    }

    /// **The structural check at inclusion**, O(segments): version, segment length, the segment count and the claim root.
    pub fn check_structure(&self, segment_roots: &[Digest]) -> Result<(), String> {
        if self.version != SEG_EVIDENCE_VERSION_V2 {
            return Err(format!("segmented evidence version {} is not supported", self.version));
        }
        if self.segment_len != SEG_LEN_V4 || self.positions == 0 {
            return Err(format!(
                "{} positions in segments of {}: a claim has positions, in segments of {SEG_LEN_V4}",
                self.positions, self.segment_len
            ));
        }
        if segment_roots.len() != segment_count_v1(self.positions) as usize || segment_roots.len() > SEG_MAX_SEGMENTS_V4 {
            return Err(format!("{} segment roots for {} positions", segment_roots.len(), self.positions));
        }
        if claim_root_v1(self.positions, segment_roots) != self.claim_root {
            return Err("the segment roots are not the evidence's claim root".into());
        }
        Ok(())
    }
}

/// **The job's input binding** for a segmented claim: the prompt by its length and tile root, then the fed generated ids.
pub fn job_input_root_v2(prompt_len: u32, prompt_root: &Digest, fed: &[u32]) -> Digest {
    let mut s = keyed(JOB_INPUT_DOMAIN_V2);
    s.update(&prompt_len.to_le_bytes()).update(prompt_root).update(&(fed.len() as u64).to_le_bytes());
    for t in fed {
        s.update(&t.to_le_bytes());
    }
    finish(s)
}

/// A prompt tile's leaf.
pub fn prompt_tile_leaf_v1(index: u32, ids: &[u32]) -> Digest {
    let mut s = keyed(PROMPT_TILE_DOMAIN_V1);
    s.update(&index.to_le_bytes()).update(&(ids.len() as u32).to_le_bytes());
    for t in ids {
        s.update(&t.to_le_bytes());
    }
    finish(s)
}

/// Tiles of a prompt of `len` ids.
pub fn prompt_tiles_v1(len: u32) -> u32 {
    (len as usize).div_ceil(PROMPT_TILE_IDS_V1) as u32
}

fn prompt_root_of_tree(len: u32, tree: &Digest) -> Digest {
    let mut s = keyed(PROMPT_ROOT_DOMAIN_V1);
    s.update(&len.to_le_bytes()).update(tree);
    finish(s)
}

fn prompt_leaves(ids: &[u32]) -> Vec<Digest> {
    ids.chunks(PROMPT_TILE_IDS_V1).enumerate().map(|(i, c)| prompt_tile_leaf_v1(i as u32, c)).collect()
}

/// **The prompt root** of a prompt's ids (what an inline job and a tiled job both bind).
pub fn prompt_root_of_ids_v1(ids: &[u32]) -> Digest {
    prompt_root_of_tree(ids.len() as u32, &tree_root(prompt_leaves(ids), prompt_node, PROMPT_NODE_DOMAIN_V1))
}

/// **One prompt tile** with its path to the prompt root.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PromptTileOpeningV1 {
    pub index: u32,
    pub ids: Vec<u32>,
    pub siblings: Vec<Digest>,
}

impl PromptTileOpeningV1 {
    pub fn of(ids: &[u32], index: u32) -> Option<Self> {
        let tile = ids.chunks(PROMPT_TILE_IDS_V1).nth(index as usize)?.to_vec();
        Some(Self { index, ids: tile, siblings: tree_path(prompt_leaves(ids), index as usize, prompt_node) })
    }

    /// Is this tile `index` of the prompt of `len` ids whose root is `root`? Every tile is full but the last.
    pub fn authenticates(&self, len: u32, root: &Digest) -> bool {
        let tiles = prompt_tiles_v1(len);
        if self.index >= tiles {
            return false;
        }
        let want = if self.index + 1 == tiles { len as usize - self.index as usize * PROMPT_TILE_IDS_V1 } else { PROMPT_TILE_IDS_V1 };
        if self.ids.len() != want {
            return false;
        }
        tree_root_from_path(prompt_tile_leaf_v1(self.index, &self.ids), self.index as u64, tiles as u64, &self.siblings, prompt_node)
            .is_some_and(|tree| prompt_root_of_tree(len, &tree) == *root)
    }

    /// The id at prompt position `p`, if this tile holds it.
    pub fn id_at(&self, p: u32) -> Option<u32> {
        let first = self.index as usize * PROMPT_TILE_IDS_V1;
        (p as usize).checked_sub(first).and_then(|i| self.ids.get(i).copied())
    }
}

/// **A job whose prompt is posted in tiles**: the class, the prompt's length and tile root, the generation budget and decode rule.
/// Claims on it commit only once every tile is posted (`PostPromptTile`), so the input is public on chain before any claim exists.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TiledJobV1 {
    pub class_binding_id: Digest,
    pub prompt_len: u32,
    pub prompt_root: Digest,
    pub max_new_tokens: u32,
    pub decode: DecodeRuleV1,
    pub nonce: Digest,
}

impl TiledJobV1 {
    pub fn id(&self) -> Digest {
        object_id(TILED_JOB_DOMAIN_V1, self)
    }

    pub fn well_formed(&self, max_positions: u32) -> Result<(), String> {
        if self.prompt_len == 0 || self.max_new_tokens == 0 {
            return Err("a tiled job has a prompt and generates at least one token".into());
        }
        let positions = self.prompt_len as u64 + self.max_new_tokens as u64 - 1;
        if positions > max_positions as u64 {
            return Err(format!("{positions} positions exceed the class's {max_positions}"));
        }
        Ok(())
    }
}

/// A tiled job as the ledger keeps it: the job and which tiles are posted (a bitmap; the ids stay in the blocks that carried them).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TiledJobRowV1 {
    pub job: TiledJobV1,
    pub posted: Vec<u8>,
}

impl TiledJobRowV1 {
    pub fn new(job: TiledJobV1) -> Self {
        let tiles = prompt_tiles_v1(job.prompt_len) as usize;
        Self { job, posted: vec![0; tiles.div_ceil(8)] }
    }

    pub fn is_posted(&self, index: u32) -> bool {
        self.posted.get(index as usize / 8).is_some_and(|b| b & (1 << (index % 8)) != 0)
    }

    pub fn mark(&mut self, index: u32) {
        if let Some(b) = self.posted.get_mut(index as usize / 8) {
            *b |= 1 << (index % 8);
        }
    }

    pub fn complete(&self) -> bool {
        (0..prompt_tiles_v1(self.job.prompt_len)).all(|i| self.is_posted(i))
    }
}

/// **A producer's segmented commitments of its trace**: the v3 commitment of every committed value of every position.
pub fn seg_commitments_of_trace_v1(trace: &crate::trace::TraceV1) -> SegmentedCommitmentsV1 {
    SegmentedCommitmentsV1::new(
        trace
            .values
            .iter()
            .map(|p| p.iter().map(|o| o.iter().map(crate::merkle3::tensor_commitment_v3).collect()).collect())
            .collect(),
    )
}

/// **The evidence object a producer commits** for a segmented claim of `prompt_len` prompt ids (root `prompt_root`) and the fed ids.
pub fn build_segmented_evidence_v1(
    header: EvidenceHeaderV1,
    descriptor: &crate::descriptor::KernelDescriptorV1,
    prompt_len: u32,
    prompt_root: &Digest,
    fed: &[u32],
    commitments: &SegmentedCommitmentsV1,
) -> SegmentedEvidenceV2 {
    SegmentedEvidenceV2 {
        version: SEG_EVIDENCE_VERSION_V2,
        header,
        job_input_root: job_input_root_v2(prompt_len, prompt_root, fed),
        positions: commitments.positions(),
        segment_len: SEG_LEN_V4,
        claim_root: commitments.claim_root(),
        suite: SuiteParamsV1::of(descriptor),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commitments(positions: u32, per: &[usize]) -> SegmentedCommitmentsV1 {
        SegmentedCommitmentsV1::new(
            (0..positions)
                .map(|p| {
                    per.iter()
                        .enumerate()
                        .map(|(s, n)| (0..*n).map(|i| crate::hash::id(b"t", &[p as u8, (p >> 8) as u8, s as u8, i as u8])).collect())
                        .collect()
                })
                .collect(),
        )
    }

    #[test]
    fn every_node_opens_against_the_segment_roots_and_nothing_else_does() {
        for positions in [1u32, 5, 1024, 1025, 2050] {
            let c = commitments(positions, &[2, 3, 1]);
            let roots = c.segment_roots();
            assert_eq!(roots.len() as u32, segment_count_v1(positions));
            for p in [0, positions / 2, positions - 1] {
                for (s, n, index) in [(0u16, 0u16, 0u64), (0, 1, 1), (1, 2, 4), (2, 0, 5)] {
                    let o = c.node_opening(p, s, n).unwrap();
                    assert!(o.authenticates(&roots, positions, index, 6), "{positions} {p} ({s}, {n})");
                    assert!(!o.authenticates(&roots, positions, (index + 1) % 6, 6), "another node index");
                    let mut moved = o.clone();
                    moved.position = (p + 1) % positions;
                    if positions > 1 {
                        assert!(!moved.authenticates(&roots, positions, index, 6), "another position");
                    }
                    let mut bad = o.clone();
                    bad.commitment[0] ^= 1;
                    assert!(!bad.authenticates(&roots, positions, index, 6));
                }
            }
            let ev = SegmentedEvidenceV2 {
                version: SEG_EVIDENCE_VERSION_V2,
                header: EvidenceHeaderV1 {
                    network_domain: [0; 64],
                    ruleset_digest: [0; 64],
                    class_binding_id: [0; 64],
                    program_root: [0; 64],
                    artifact_root: [0; 64],
                    plan_root: [0; 64],
                },
                job_input_root: [0; 64],
                positions,
                segment_len: SEG_LEN_V4,
                claim_root: c.claim_root(),
                suite: SuiteParamsV1::of(&crate::descriptor::k2_tir_v4_descriptor()),
            };
            ev.check_structure(&roots).unwrap();
            let mut fewer = roots.clone();
            fewer.pop();
            assert!(ev.check_structure(&fewer).is_err());
            let mut swapped = roots.clone();
            swapped[0][0] ^= 1;
            assert!(ev.check_structure(&swapped).is_err());
        }
    }

    #[test]
    fn segment_roots_from_position_roots_alone_are_the_commitments_own() {
        for positions in [1u32, 1023, 1024, 1025, 3000] {
            let c = commitments(positions, &[2, 1]);
            assert_eq!(segment_roots_of_position_roots_v1(&c.position_roots), c.segment_roots(), "{positions}");
        }
    }

    #[test]
    fn prompt_tiles_open_against_the_prompt_root() {
        for len in [1usize, 4095, 4096, 4097, 9000] {
            let ids: Vec<u32> = (0..len as u32).map(|i| i % 97).collect();
            let root = prompt_root_of_ids_v1(&ids);
            for t in 0..prompt_tiles_v1(len as u32) {
                let o = PromptTileOpeningV1::of(&ids, t).unwrap();
                assert!(o.authenticates(len as u32, &root));
                for p in [t as usize * PROMPT_TILE_IDS_V1, (len - 1).min((t as usize + 1) * PROMPT_TILE_IDS_V1 - 1)] {
                    assert_eq!(o.id_at(p as u32), Some(ids[p]));
                }
                let mut bad = o.clone();
                bad.ids[0] += 1;
                assert!(!bad.authenticates(len as u32, &root));
                assert!(!o.authenticates(len as u32 + 1, &root), "the length is bound");
            }
        }
    }
}
