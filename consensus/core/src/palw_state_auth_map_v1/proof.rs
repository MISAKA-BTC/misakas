//! Proofs over one table: a pruned trie (preorder), its verification against a table root, the rows it proves, and the
//! verifier-side update (old root and proven old values → new root) and stream (complete table in bounded chunks).

use super::AuthMapErrorV1;
use super::bits::{BitPathV1, KeyBoundV1, KeyRangeV1, subtree_may_intersect_any_v1};
use super::trie::{AuthTableSpecV1, auth_table_root_v1, branch_hash, empty_hash, leaf_hash};
use crate::Hash64;
use std::collections::BTreeMap;

/// One node of a pruned trie, in preorder (a branch is followed by its 0-subtree, then its 1-subtree).
#[derive(Clone, PartialEq, Eq, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum ProofNodeV1 {
    /// A subtree left closed: only its hash. Allowed only where no proven range can reach.
    Opaque(Hash64),
    /// An opened branch: its absolute prefix (packed, zero-padded).
    Branch { prefix_len: u32, prefix: Vec<u8> },
    /// An opened row.
    Leaf { key: Vec<u8>, value: Vec<u8> },
}

/// **A proof over one table**: the row count and the pruned trie. An empty table is `count == 0` and no nodes.
#[derive(Clone, PartialEq, Eq, Debug, Default, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct AuthMapProofV1 {
    pub count: u64,
    pub nodes: Vec<ProofNodeV1>,
}

impl AuthMapProofV1 {
    /// Wire bytes (borsh).
    pub fn to_bytes(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("a proof is borsh-serializable")
    }

    /// Decode wire bytes no longer than `max_bytes` (checked before any allocation the bytes could size), with nothing trailing.
    pub fn from_bytes(bytes: &[u8], max_bytes: usize) -> Result<Self, AuthMapErrorV1> {
        if bytes.len() > max_bytes {
            return Err(AuthMapErrorV1::ProofTooLarge);
        }
        let mut slice = bytes;
        let proof = <Self as borsh::BorshDeserialize>::deserialize(&mut slice).map_err(|_| AuthMapErrorV1::Malformed)?;
        slice.is_empty().then_some(proof).ok_or(AuthMapErrorV1::Malformed)
    }
}

/// The verifier's copy of a pruned trie.
#[derive(Clone, Debug)]
enum PNode {
    Opaque(Hash64),
    Leaf { key: Vec<u8>, path: BitPathV1, value: Vec<u8> },
    Branch { prefix: BitPathV1, children: Box<[PNode; 2]> },
}

impl PNode {
    fn hash(&self, spec: &AuthTableSpecV1) -> Hash64 {
        match self {
            Self::Opaque(h) => *h,
            Self::Leaf { key, value, .. } => leaf_hash(spec, key, value),
            Self::Branch { prefix, children } => branch_hash(spec, prefix, &children[0].hash(spec), &children[1].hash(spec)),
        }
    }

    fn collect_leaves(&self, out: &mut BTreeMap<Vec<u8>, Vec<u8>>) {
        match self {
            Self::Opaque(_) => {}
            Self::Leaf { key, value, .. } => {
                out.insert(key.clone(), value.clone());
            }
            Self::Branch { children, .. } => {
                children[0].collect_leaves(out);
                children[1].collect_leaves(out);
            }
        }
    }
}

struct Parser<'a> {
    spec: &'a AuthTableSpecV1,
    nodes: &'a [ProofNodeV1],
    at: usize,
    ranges: &'a [KeyRangeV1],
}

impl Parser<'_> {
    /// Parse the subtree at the cursor. `known` is what the verifier knows of its keys BEFORE opening it: the parent's prefix and
    /// the direction bit (`None` at the root, where nothing is known). Enforces: an opaque subtree no range can reach; an opened
    /// subtree that really lies under `known`; a branch prefix that ends before the deepest bit a key can have.
    fn subtree(&mut self, known: Option<&BitPathV1>) -> Result<PNode, AuthMapErrorV1> {
        let node = self.nodes.get(self.at).ok_or(AuthMapErrorV1::Malformed)?;
        self.at += 1;
        match node {
            ProofNodeV1::Opaque(h) => {
                let reachable = match known {
                    None => self.ranges.iter().any(|r| !r.is_empty()),
                    Some(q) => subtree_may_intersect_any_v1(q, self.ranges),
                };
                if reachable {
                    return Err(AuthMapErrorV1::IncompleteRange);
                }
                Ok(PNode::Opaque(*h))
            }
            ProofNodeV1::Leaf { key, value } => {
                if key.len() > self.spec.max_key_len as usize {
                    return Err(AuthMapErrorV1::KeyTooLong);
                }
                if value.len() > self.spec.max_value_len as usize {
                    return Err(AuthMapErrorV1::ValueTooLong);
                }
                let path = BitPathV1::encode_key(key);
                if known.is_some_and(|q| !path.starts_with(q)) {
                    return Err(AuthMapErrorV1::Misplaced);
                }
                Ok(PNode::Leaf { key: key.clone(), path, value: value.clone() })
            }
            ProofNodeV1::Branch { prefix_len, prefix } => {
                // a branch splits at bit `prefix_len`, which a key must still have: prefix_len < max encoded bits
                if *prefix_len >= self.spec.max_depth() {
                    return Err(AuthMapErrorV1::Malformed);
                }
                let prefix = BitPathV1::from_parts(prefix.clone(), *prefix_len).ok_or(AuthMapErrorV1::Malformed)?;
                if known.is_some_and(|q| !prefix.starts_with(q)) {
                    return Err(AuthMapErrorV1::Misplaced);
                }
                let left = self.subtree(Some(&prefix.child(0)))?;
                let right = self.subtree(Some(&prefix.child(1)))?;
                Ok(PNode::Branch { prefix, children: Box::new([left, right]) })
            }
        }
    }
}

/// **The rows a verified proof establishes.** For a key inside a proven range the answer is a fact — present with this value, or
/// absent. For a key outside every proven range there is no answer ([`AuthMapErrorV1::NotCovered`]): "not proven" is never
/// "absent".
#[derive(Clone, Debug)]
pub struct VerifiedRowsV1 {
    count: u64,
    ranges: Vec<KeyRangeV1>,
    rows: BTreeMap<Vec<u8>, Vec<u8>>,
    tree: Option<PNode>,
}

impl VerifiedRowsV1 {
    pub fn count(&self) -> u64 {
        self.count
    }

    pub fn ranges(&self) -> &[KeyRangeV1] {
        &self.ranges
    }

    pub fn covers(&self, key: &[u8]) -> bool {
        self.ranges.iter().any(|r| r.contains(key))
    }

    /// The proven answer for `key`: `Ok(Some(value))` present, `Ok(None)` proven absent, `Err(NotCovered)` not asked.
    pub fn get(&self, key: &[u8]) -> Result<Option<&[u8]>, AuthMapErrorV1> {
        if !self.covers(key) {
            return Err(AuthMapErrorV1::NotCovered);
        }
        Ok(self.rows.get(key).map(Vec::as_slice))
    }

    /// Every row of `range` (which must be one of the proven ranges), complete, in key order.
    pub fn range(&self, range: &KeyRangeV1) -> Result<Vec<(&[u8], &[u8])>, AuthMapErrorV1> {
        if !self.ranges.contains(range) {
            return Err(AuthMapErrorV1::NotCovered);
        }
        Ok(self.rows.iter().filter(|(k, _)| range.contains(k)).map(|(k, v)| (k.as_slice(), v.as_slice())).collect())
    }

    /// Every proven row of every proven range, in key order.
    pub fn proven_rows(&self) -> impl Iterator<Item = (&[u8], &[u8])> {
        self.rows.iter().filter(|(k, _)| self.covers(k)).map(|(k, v)| (k.as_slice(), v.as_slice()))
    }
}

/// **Verify `proof` of `ranges` against `table_root`.** Checks, in this order: the proof's size (before any hashing), its shape
/// (preorder, nothing trailing, canonical prefixes, every opened node under the prefix its parent fixes), completeness (no opaque
/// subtree a range can reach), and the root. On success, every key of every range is answered.
pub fn verify_rows_v1(
    spec: &AuthTableSpecV1,
    table_root: &Hash64,
    proof: &AuthMapProofV1,
    ranges: &[KeyRangeV1],
    max_nodes: u32,
) -> Result<VerifiedRowsV1, AuthMapErrorV1> {
    spec.check()?;
    if proof.nodes.len() > max_nodes as usize {
        return Err(AuthMapErrorV1::ProofTooLarge);
    }
    if ranges.iter().any(|r| r.max_bound_len() > spec.max_key_len as usize) {
        return Err(AuthMapErrorV1::KeyTooLong);
    }
    let (tree, trie_root) = if proof.nodes.is_empty() {
        if proof.count != 0 {
            return Err(AuthMapErrorV1::RootMismatch);
        }
        (None, empty_hash(spec))
    } else {
        if proof.count == 0 {
            return Err(AuthMapErrorV1::RootMismatch);
        }
        let mut parser = Parser { spec, nodes: &proof.nodes, at: 0, ranges };
        let tree = parser.subtree(None)?;
        if parser.at != proof.nodes.len() {
            return Err(AuthMapErrorV1::Malformed);
        }
        let h = tree.hash(spec);
        (Some(tree), h)
    };
    if auth_table_root_v1(spec, proof.count, &trie_root) != *table_root {
        return Err(AuthMapErrorV1::RootMismatch);
    }
    let mut rows = BTreeMap::new();
    if let Some(t) = &tree {
        t.collect_leaves(&mut rows);
    }
    Ok(VerifiedRowsV1 { count: proof.count, ranges: ranges.to_vec(), rows, tree })
}

/// One row write an update proves: the value before (as the writer read it) and after; `None` is "no row".
#[derive(Clone, PartialEq, Eq, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct RowUpdateV1 {
    pub key: Vec<u8>,
    pub old: Option<Vec<u8>>,
    pub new: Option<Vec<u8>>,
}

fn p_join(a: PNode, a_prefix: &BitPathV1, b: PNode, b_prefix: &BitPathV1) -> PNode {
    let common = a_prefix.common_prefix_len(b_prefix);
    let prefix = a_prefix.prefix(common);
    if a_prefix.bit(common) == 0 {
        PNode::Branch { prefix, children: Box::new([a, b]) }
    } else {
        PNode::Branch { prefix, children: Box::new([b, a]) }
    }
}

fn p_insert(node: PNode, path: &BitPathV1, key: Vec<u8>, value: Vec<u8>) -> Result<PNode, AuthMapErrorV1> {
    match node {
        PNode::Opaque(_) => Err(AuthMapErrorV1::NotCovered),
        PNode::Leaf { key: lk, path: lp, value: lv } => {
            if lp == *path {
                Ok(PNode::Leaf { key, path: lp, value })
            } else {
                let new = PNode::Leaf { key, path: path.clone(), value };
                let old_prefix = lp.clone();
                Ok(p_join(PNode::Leaf { key: lk, path: lp, value: lv }, &old_prefix, new, path))
            }
        }
        PNode::Branch { prefix, children } => {
            if path.starts_with(&prefix) {
                let dir = path.bit(prefix.len());
                let [c0, c1] = *children;
                let kids = if dir == 0 { [p_insert(c0, path, key, value)?, c1] } else { [c0, p_insert(c1, path, key, value)?] };
                Ok(PNode::Branch { prefix, children: Box::new(kids) })
            } else {
                let new = PNode::Leaf { key, path: path.clone(), value };
                let old_prefix = prefix.clone();
                Ok(p_join(PNode::Branch { prefix, children }, &old_prefix, new, path))
            }
        }
    }
}

fn p_remove(node: PNode, path: &BitPathV1) -> Result<Option<PNode>, AuthMapErrorV1> {
    match node {
        PNode::Opaque(_) => Err(AuthMapErrorV1::NotCovered),
        PNode::Leaf { path: lp, .. } if lp == *path => Ok(None),
        PNode::Leaf { .. } => Err(AuthMapErrorV1::NotCovered),
        PNode::Branch { prefix, children } => {
            if !path.starts_with(&prefix) {
                return Err(AuthMapErrorV1::NotCovered);
            }
            let dir = path.bit(prefix.len());
            let [c0, c1] = *children;
            let (taken, other) = if dir == 0 { (c0, c1) } else { (c1, c0) };
            Ok(Some(match p_remove(taken, path)? {
                None => other,
                Some(child) => {
                    let kids = if dir == 0 { [child, other] } else { [other, child] };
                    PNode::Branch { prefix, children: Box::new(kids) }
                }
            }))
        }
    }
}

/// **Verify an update and compute the new table root.** `proof` must prove the point range of every key in `updates` against
/// `old_root`; every `old` must equal the proven value (a writer that read a different row, or a row from another root, is
/// refused); the writes are then applied to the pruned trie — which, by completeness, holds every node they touch — and the new
/// root is returned. Duplicate keys are refused (one write per key per change set).
pub fn verify_update_v1(
    spec: &AuthTableSpecV1,
    old_root: &Hash64,
    proof: &AuthMapProofV1,
    updates: &[RowUpdateV1],
    max_nodes: u32,
) -> Result<Hash64, AuthMapErrorV1> {
    let ranges: Vec<_> = updates.iter().map(|u| KeyRangeV1::point(&u.key)).collect();
    let mut seen = std::collections::BTreeSet::new();
    for u in updates {
        if !seen.insert(u.key.as_slice()) {
            return Err(AuthMapErrorV1::DuplicateWrite);
        }
        if u.new.as_ref().is_some_and(|v| v.len() > spec.max_value_len as usize) {
            return Err(AuthMapErrorV1::ValueTooLong);
        }
    }
    let verified = verify_rows_v1(spec, old_root, proof, &ranges, max_nodes)?;
    for u in updates {
        if verified.get(&u.key)? != u.old.as_deref() {
            return Err(AuthMapErrorV1::StaleOldValue);
        }
    }
    let mut count = verified.count;
    let mut tree = verified.tree;
    // Inserts and overwrites first, deletes last. The trie is canonical, so the order does not change the root; it changes which
    // nodes the walk needs. A delete splices its (possibly opaque) sibling into the parent's place, and a later insert reaching that
    // place would need the sibling's prefix, which a proof does not reveal. Run first, an insert only walks opened nodes (coverage)
    // or nodes it created; a delete only walks the path of a proven-present key, every node of which is opened.
    let (writes, deletes): (Vec<&RowUpdateV1>, Vec<&RowUpdateV1>) = updates.iter().partition(|u| u.new.is_some());
    for u in writes.into_iter().chain(deletes) {
        let path = BitPathV1::encode_key(&u.key);
        match (&u.old, &u.new) {
            (_, Some(v)) => {
                tree = Some(match tree {
                    None => PNode::Leaf { key: u.key.clone(), path, value: v.clone() },
                    Some(t) => p_insert(t, &path, u.key.clone(), v.clone())?,
                });
                if u.old.is_none() {
                    count += 1;
                }
            }
            (Some(_), None) => {
                tree = p_remove(tree.ok_or(AuthMapErrorV1::NotCovered)?, &path)?;
                count -= 1;
            }
            (None, None) => {}
        }
    }
    let trie_root = tree.map_or_else(|| empty_hash(spec), |t| t.hash(spec));
    Ok(auth_table_root_v1(spec, count, &trie_root))
}

/// **One chunk of an authenticated stream**: every row from the stream's position up to `end`, proven complete.
#[derive(Clone, PartialEq, Eq, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct StreamChunkV1 {
    pub end: KeyBoundV1,
    pub proof: AuthMapProofV1,
}

/// **The verifier of a whole-table stream** (the "authenticated stream" access class: a draw or a census that must see every row,
/// read in bounded pieces against one pinned root). Each chunk proves `[position, end)` complete with at most `max_rows` rows and
/// at least one unless it is the last; the stream is done at an unbounded end, and the rows seen then number the committed count.
#[derive(Clone, Debug)]
pub struct AuthStreamVerifierV1 {
    spec: AuthTableSpecV1,
    root: Hash64,
    position: KeyBoundV1,
    max_rows: usize,
    max_nodes: u32,
    seen: u64,
    count: Option<u64>,
    done: bool,
}

impl AuthStreamVerifierV1 {
    pub fn new(spec: AuthTableSpecV1, root: Hash64, max_rows: usize, max_nodes: u32) -> Self {
        Self { spec, root, position: KeyBoundV1::Unbounded, max_rows, max_nodes, seen: 0, count: None, done: false }
    }

    pub fn is_done(&self) -> bool {
        self.done
    }

    /// Accept the next chunk; returns its rows, in key order.
    pub fn accept(&mut self, chunk: &StreamChunkV1) -> Result<Vec<(Vec<u8>, Vec<u8>)>, AuthMapErrorV1> {
        if self.done {
            return Err(AuthMapErrorV1::StreamFinished);
        }
        if matches!(chunk.end, KeyBoundV1::Included(_)) {
            return Err(AuthMapErrorV1::Malformed);
        }
        let range = KeyRangeV1 { start: self.position.clone(), end: chunk.end.clone() };
        let verified = verify_rows_v1(&self.spec, &self.root, &chunk.proof, std::slice::from_ref(&range), self.max_nodes)?;
        if self.count.is_some_and(|c| c != verified.count) {
            return Err(AuthMapErrorV1::RootMismatch);
        }
        self.count = Some(verified.count);
        let rows: Vec<_> = verified.range(&range)?.into_iter().map(|(k, v)| (k.to_vec(), v.to_vec())).collect();
        if rows.len() > self.max_rows {
            return Err(AuthMapErrorV1::ProofTooLarge);
        }
        let last = matches!(chunk.end, KeyBoundV1::Unbounded);
        if rows.is_empty() && !last {
            return Err(AuthMapErrorV1::NoProgress);
        }
        self.seen += rows.len() as u64;
        match &chunk.end {
            KeyBoundV1::Excluded(k) => self.position = KeyBoundV1::Included(k.clone()),
            _ => {
                self.done = true;
                if self.seen != verified.count {
                    return Err(AuthMapErrorV1::IncompleteRange);
                }
            }
        }
        Ok(rows)
    }
}

impl super::trie::AuthMapV1 {
    /// The stream chunk starting at `position`: at most `max_rows` rows, ending just before the next row (or unbounded).
    pub fn stream_chunk(&self, position: &KeyBoundV1, max_rows: usize) -> StreamChunkV1 {
        let open = KeyRangeV1 { start: position.clone(), end: KeyBoundV1::Unbounded };
        let rows = self.range_rows(&open, max_rows.saturating_add(1));
        let end = match rows.get(max_rows) {
            Some((k, _)) => KeyBoundV1::Excluded(k.clone()),
            None => KeyBoundV1::Unbounded,
        };
        let range = KeyRangeV1 { start: position.clone(), end: end.clone() };
        StreamChunkV1 { end, proof: self.prove(std::slice::from_ref(&range)) }
    }
}
