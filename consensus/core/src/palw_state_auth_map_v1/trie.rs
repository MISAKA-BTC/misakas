//! The authenticated ordered map: a binary Patricia (crit-bit) Merkle trie over [`BitPathV1`]-encoded keys.
//!
//! * **Canonical.** The shape is a function of the key set alone (every branch splits at the first bit its keys disagree on), so two
//!   nodes that hold the same rows hold the same root whatever order they wrote them in.
//! * **Absolute prefixes.** A branch commits the whole bit prefix its keys share (not the bits skipped since its parent), and a leaf
//!   its whole key. A subtree's hash therefore never depends on where it hangs, which is what lets a verifier splice a sibling up on
//!   a delete, and lets a proof's opaque subtree be placed in key order by the prefix its parent commits.
//! * **Tagged.** Every node hash is keyed by a node-kind domain and carries the table's [`AuthTableSpecV1`] (table id, schema
//!   version, key and value limits): a subtree of one table, or of one version of a table, hashes differently in any other.

use super::AuthMapErrorV1;
use super::bits::{BitPathV1, KeyRangeV1, encoded_bits_v1, subtree_may_intersect_any_v1};
use super::proof::{AuthMapProofV1, ProofNodeV1};
use crate::Hash64;
use blake2b_simd::Params;

pub(super) const DOMAIN_LEAF: &[u8] = b"misaka-palw/auth-map/v1/leaf";
pub(super) const DOMAIN_BRANCH: &[u8] = b"misaka-palw/auth-map/v1/branch";
pub(super) const DOMAIN_EMPTY: &[u8] = b"misaka-palw/auth-map/v1/empty";
pub(super) const DOMAIN_TABLE: &[u8] = b"misaka-palw/auth-map/v1/table";

/// The hard ceiling on a table's declared key length. With it, a path is at most `9 · 512 + 1` bits, so a point proof is at most
/// that many branches deep whatever keys an adversary grinds.
pub const AUTH_MAP_MAX_KEY_LEN_V1: u32 = 512;
/// The hard ceiling on a table's declared value length (a row larger than this belongs in an artifact store, by digest).
pub const AUTH_MAP_MAX_VALUE_LEN_V1: u32 = 1 << 20;

/// **A table's identity and limits** — part of every node hash, so they are part of the commitment, not a verifier's guess.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct AuthTableSpecV1 {
    pub table_id: u32,
    pub schema_version: u32,
    pub max_key_len: u32,
    pub max_value_len: u32,
}

impl AuthTableSpecV1 {
    pub const fn new(table_id: u32, schema_version: u32, max_key_len: u32, max_value_len: u32) -> Self {
        Self { table_id, schema_version, max_key_len, max_value_len }
    }

    pub fn check(&self) -> Result<(), AuthMapErrorV1> {
        if self.max_key_len > AUTH_MAP_MAX_KEY_LEN_V1 || self.max_value_len > AUTH_MAP_MAX_VALUE_LEN_V1 {
            return Err(AuthMapErrorV1::SpecOutOfBounds);
        }
        Ok(())
    }

    /// The deepest a leaf can sit: one branch per bit of the longest encoded key.
    pub const fn max_depth(&self) -> u32 {
        encoded_bits_v1(self.max_key_len)
    }

    /// The most nodes a proof of ONE key (membership or not) can need: a branch and an opaque sibling per level, and the end node.
    pub const fn point_proof_node_bound(&self) -> u32 {
        2 * self.max_depth() + 1
    }

    fn tag(&self) -> [u8; 16] {
        let mut t = [0u8; 16];
        t[0..4].copy_from_slice(&self.table_id.to_le_bytes());
        t[4..8].copy_from_slice(&self.schema_version.to_le_bytes());
        t[8..12].copy_from_slice(&self.max_key_len.to_le_bytes());
        t[12..16].copy_from_slice(&self.max_value_len.to_le_bytes());
        t
    }
}

fn hasher(domain: &[u8], spec: &AuthTableSpecV1) -> blake2b_simd::State {
    let mut s = Params::new().hash_length(64).key(domain).to_state();
    s.update(&spec.tag());
    s
}

fn finish(s: blake2b_simd::State) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(s.finalize().as_bytes());
    Hash64::from_bytes(out)
}

pub(super) fn leaf_hash(spec: &AuthTableSpecV1, key: &[u8], value: &[u8]) -> Hash64 {
    let mut s = hasher(DOMAIN_LEAF, spec);
    s.update(&(key.len() as u64).to_le_bytes());
    s.update(key);
    s.update(&(value.len() as u64).to_le_bytes());
    s.update(value);
    finish(s)
}

pub(super) fn branch_hash(spec: &AuthTableSpecV1, prefix: &BitPathV1, left: &Hash64, right: &Hash64) -> Hash64 {
    let mut s = hasher(DOMAIN_BRANCH, spec);
    s.update(&prefix.len().to_le_bytes());
    s.update(prefix.packed());
    s.update(left.as_byte_slice());
    s.update(right.as_byte_slice());
    finish(s)
}

pub(super) fn empty_hash(spec: &AuthTableSpecV1) -> Hash64 {
    finish(hasher(DOMAIN_EMPTY, spec))
}

/// **The table root**: the trie root with the row count beside it (a count lets a verifier size a stream before reading it).
pub fn auth_table_root_v1(spec: &AuthTableSpecV1, count: u64, trie_root: &Hash64) -> Hash64 {
    let mut s = hasher(DOMAIN_TABLE, spec);
    s.update(&count.to_le_bytes());
    s.update(trie_root.as_byte_slice());
    finish(s)
}

#[derive(Clone, Debug)]
pub(super) enum NodeV1 {
    Leaf { key: Vec<u8>, path: BitPathV1, value: Vec<u8>, hash: Hash64 },
    Branch { prefix: BitPathV1, children: Box<[NodeV1; 2]>, hash: Hash64 },
}

impl NodeV1 {
    pub(super) fn hash(&self) -> Hash64 {
        match self {
            Self::Leaf { hash, .. } | Self::Branch { hash, .. } => *hash,
        }
    }

    /// The prefix every key below shares: a leaf's whole path, a branch's committed prefix.
    pub(super) fn prefix(&self) -> &BitPathV1 {
        match self {
            Self::Leaf { path, .. } => path,
            Self::Branch { prefix, .. } => prefix,
        }
    }

    fn leaf(spec: &AuthTableSpecV1, key: Vec<u8>, value: Vec<u8>) -> Self {
        let hash = leaf_hash(spec, &key, &value);
        Self::Leaf { path: BitPathV1::encode_key(&key), key, value, hash }
    }

    fn branch(spec: &AuthTableSpecV1, prefix: BitPathV1, children: [NodeV1; 2]) -> Self {
        let hash = branch_hash(spec, &prefix, &children[0].hash(), &children[1].hash());
        Self::Branch { prefix, children: Box::new(children), hash }
    }
}

/// Join two subtrees whose prefixes diverge: a new branch at their common prefix, ordered by the bit they split on.
fn join(spec: &AuthTableSpecV1, a: NodeV1, b: NodeV1) -> NodeV1 {
    let common = a.prefix().common_prefix_len(b.prefix());
    let prefix = a.prefix().prefix(common);
    if a.prefix().bit(common) == 0 { NodeV1::branch(spec, prefix, [a, b]) } else { NodeV1::branch(spec, prefix, [b, a]) }
}

fn insert(spec: &AuthTableSpecV1, node: NodeV1, path: &BitPathV1, key: Vec<u8>, value: Vec<u8>) -> (NodeV1, Option<Vec<u8>>) {
    match node {
        NodeV1::Leaf { key: lk, path: lp, value: lv, hash } => {
            if lp == *path {
                (NodeV1::leaf(spec, key, value), Some(lv))
            } else {
                (join(spec, NodeV1::Leaf { key: lk, path: lp, value: lv, hash }, NodeV1::leaf(spec, key, value)), None)
            }
        }
        NodeV1::Branch { prefix, children, hash } => {
            if path.starts_with(&prefix) {
                let dir = path.bit(prefix.len()) as usize;
                let [c0, c1] = *children;
                let (kids, old) = if dir == 0 {
                    let (n, old) = insert(spec, c0, path, key, value);
                    ([n, c1], old)
                } else {
                    let (n, old) = insert(spec, c1, path, key, value);
                    ([c0, n], old)
                };
                (NodeV1::branch(spec, prefix, kids), old)
            } else {
                (join(spec, NodeV1::Branch { prefix, children, hash }, NodeV1::leaf(spec, key, value)), None)
            }
        }
    }
}

fn remove(spec: &AuthTableSpecV1, node: NodeV1, path: &BitPathV1) -> (Option<NodeV1>, Option<Vec<u8>>) {
    match node {
        NodeV1::Leaf { path: ref lp, .. } if lp != path => (Some(node), None),
        NodeV1::Leaf { value, .. } => (None, Some(value)),
        NodeV1::Branch { ref prefix, .. } if !path.starts_with(prefix) => (Some(node), None),
        NodeV1::Branch { prefix, children, .. } => {
            let dir = path.bit(prefix.len()) as usize;
            let [c0, c1] = *children;
            let (taken, other) = if dir == 0 { (c0, c1) } else { (c1, c0) };
            match remove(spec, taken, path) {
                (None, old) => (Some(other), old), // the sibling (absolute prefix, unchanged hash) takes the branch's place
                (Some(child), old) => {
                    let kids = if dir == 0 { [child, other] } else { [other, child] };
                    (Some(NodeV1::branch(spec, prefix, kids)), old)
                }
            }
        }
    }
}

/// **The authenticated ordered map of one table.** Rows are raw `(key, value)` byte strings ordered bytewise; the table's schema
/// (not this module) owns the order-preserving key encoding of its typed keys.
#[derive(Clone, Debug)]
pub struct AuthMapV1 {
    spec: AuthTableSpecV1,
    root: Option<NodeV1>,
    len: u64,
}

impl AuthMapV1 {
    pub fn new(spec: AuthTableSpecV1) -> Result<Self, AuthMapErrorV1> {
        spec.check()?;
        Ok(Self { spec, root: None, len: 0 })
    }

    pub fn spec(&self) -> &AuthTableSpecV1 {
        &self.spec
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The trie root alone (the empty-table hash for no rows).
    pub fn trie_root(&self) -> Hash64 {
        self.root.as_ref().map_or_else(|| empty_hash(&self.spec), NodeV1::hash)
    }

    /// The table root a directory (or a state) commits: [`auth_table_root_v1`] of the count and the trie root.
    pub fn root(&self) -> Hash64 {
        auth_table_root_v1(&self.spec, self.len, &self.trie_root())
    }

    fn check_row(&self, key: &[u8], value: Option<&[u8]>) -> Result<(), AuthMapErrorV1> {
        if key.len() > self.spec.max_key_len as usize {
            return Err(AuthMapErrorV1::KeyTooLong);
        }
        if value.is_some_and(|v| v.len() > self.spec.max_value_len as usize) {
            return Err(AuthMapErrorV1::ValueTooLong);
        }
        Ok(())
    }

    /// Insert or overwrite; returns the previous value. O(depth) hashes.
    pub fn insert(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<Option<Vec<u8>>, AuthMapErrorV1> {
        self.check_row(&key, Some(&value))?;
        let path = BitPathV1::encode_key(&key);
        let (root, old) = match self.root.take() {
            None => (NodeV1::leaf(&self.spec, key, value), None),
            Some(node) => insert(&self.spec, node, &path, key, value),
        };
        self.root = Some(root);
        if old.is_none() {
            self.len += 1;
        }
        Ok(old)
    }

    /// Remove; returns the removed value. O(depth) hashes.
    pub fn remove(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>, AuthMapErrorV1> {
        self.check_row(key, None)?;
        let path = BitPathV1::encode_key(key);
        let Some(node) = self.root.take() else { return Ok(None) };
        let (root, old) = remove(&self.spec, node, &path);
        self.root = root;
        if old.is_some() {
            self.len -= 1;
        }
        Ok(old)
    }

    /// Apply `Some` as insert, `None` as remove.
    pub fn write(&mut self, key: Vec<u8>, value: Option<Vec<u8>>) -> Result<Option<Vec<u8>>, AuthMapErrorV1> {
        match value {
            Some(v) => self.insert(key, v),
            None => self.remove(&key),
        }
    }

    pub fn get(&self, key: &[u8]) -> Option<&[u8]> {
        let path = BitPathV1::encode_key(key);
        let mut node = self.root.as_ref()?;
        loop {
            match node {
                NodeV1::Leaf { path: lp, value, .. } => return (*lp == path).then_some(value.as_slice()),
                NodeV1::Branch { prefix, children, .. } => {
                    if !path.starts_with(prefix) {
                        return None;
                    }
                    node = &children[path.bit(prefix.len()) as usize];
                }
            }
        }
    }

    /// Every row in key order.
    pub fn rows(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
        let mut out = Vec::with_capacity(self.len as usize);
        fn walk(node: &NodeV1, out: &mut Vec<(Vec<u8>, Vec<u8>)>) {
            match node {
                NodeV1::Leaf { key, value, .. } => out.push((key.clone(), value.clone())),
                NodeV1::Branch { children, .. } => {
                    walk(&children[0], out);
                    walk(&children[1], out);
                }
            }
        }
        if let Some(root) = &self.root {
            walk(root, &mut out);
        }
        out
    }

    /// The rows of `range`, in key order, at most `limit` of them.
    pub fn range_rows(&self, range: &KeyRangeV1, limit: usize) -> Vec<(Vec<u8>, Vec<u8>)> {
        let mut out = Vec::new();
        fn walk(node: &NodeV1, range: &KeyRangeV1, limit: usize, out: &mut Vec<(Vec<u8>, Vec<u8>)>) {
            if out.len() >= limit {
                return;
            }
            match node {
                NodeV1::Leaf { key, value, .. } => {
                    if range.contains(key) {
                        out.push((key.clone(), value.clone()))
                    }
                }
                NodeV1::Branch { prefix, children, .. } => {
                    for (bit, child) in children.iter().enumerate() {
                        if subtree_may_intersect_any_v1(&prefix.child(bit as u8), std::slice::from_ref(range)) {
                            walk(child, range, limit, out);
                        }
                    }
                }
            }
        }
        if let Some(root) = &self.root
            && !range.is_empty()
        {
            walk(root, range, limit, &mut out);
        }
        out
    }

    /// **Prove `ranges`.** The proof opens exactly the subtrees [`subtree_may_intersect_any_v1`] says may hold a key of a range,
    /// and leaves every other subtree as its hash. One proof answers membership (`KeyRangeV1::point` of a present key),
    /// non-membership (of an absent one), range completeness, and — over the point ranges of the keys an update writes — the
    /// update itself ([`super::proof::verify_update_v1`]).
    pub fn prove(&self, ranges: &[KeyRangeV1]) -> AuthMapProofV1 {
        let mut nodes = Vec::new();
        fn open(node: &NodeV1, ranges: &[KeyRangeV1], nodes: &mut Vec<ProofNodeV1>) {
            match node {
                NodeV1::Leaf { key, value, .. } => nodes.push(ProofNodeV1::Leaf { key: key.clone(), value: value.clone() }),
                NodeV1::Branch { prefix, children, .. } => {
                    nodes.push(ProofNodeV1::Branch { prefix_len: prefix.len(), prefix: prefix.packed().to_vec() });
                    for (bit, child) in children.iter().enumerate() {
                        if subtree_may_intersect_any_v1(&prefix.child(bit as u8), ranges) {
                            open(child, ranges, nodes);
                        } else {
                            nodes.push(ProofNodeV1::Opaque(child.hash()));
                        }
                    }
                }
            }
        }
        if let Some(root) = &self.root {
            if ranges.iter().any(|r| !r.is_empty()) {
                open(root, ranges, &mut nodes);
            } else {
                nodes.push(ProofNodeV1::Opaque(root.hash()));
            }
        }
        AuthMapProofV1 { count: self.len, nodes }
    }

    /// Prove the point ranges of `keys` (the proof an update of those keys carries).
    pub fn prove_keys<K: AsRef<[u8]>>(&self, keys: &[K]) -> AuthMapProofV1 {
        let ranges: Vec<_> = keys.iter().map(|k| KeyRangeV1::point(k.as_ref())).collect();
        self.prove(&ranges)
    }
}
