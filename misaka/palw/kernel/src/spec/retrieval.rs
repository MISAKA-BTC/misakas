//! **RFC-0004 Part II `Retrieval`: a public snapshot, a deterministic index, an exact integer rule, and courts per item.**
//!
//! ```text
//! item           = { key: [i32; D], payload: [u32; ≤ P] }
//! key_digest     = H(retrieval-key; D, key LE)          payload_digest = H(retrieval-payload; len, payload LE)
//! leaf(id)       = H(retrieval-leaf; id u64, key_digest, payload_digest)
//! merkle         = binary over the leaves in id order, an unpaired last node carried up
//! snapshot root  = H(retrieval-snapshot; borsh(SnapshotV1 { N, D, P, B, merkle root }))
//! rule           = TopKCountingV1 { k, SB }:  s = clamp(Σ q·key, ±(2^SB − 1)),  κ = (s + 2^SB)·2^b + (2^b − 1 − id),  b = ⌈log2 N⌉
//!                  the min(k, N) items of largest κ, in descending κ            (RFC-0002 FR-09's counting key, `lower/dsa.rs`)
//! ```
//!
//! `κ` is distinct per item (ties go to the lowest id), so the rule is a total order: no float, no approximate index, no randomness.
//! A claim states, per retrieved entry, the id, the score and the two digests; every disagreement with the snapshot is a public fault:
//! [`RetrievalFaultV1::WrongItem`] (the entry is not the snapshot's item at its id, or its score is not the item's) and
//! [`RetrievalFaultV1::MissedBetter`] (an item outside the result beats the last entry). Each court opens ONE item against the snapshot
//! root (`O(D + P + log N)` bytes). The snapshot is DA of every claim over it: slices of `B` items are demandable by index
//! ([`classify_slice_response_v1`]), and a producer who withholds one defaults.

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::{DType, Tensor};

use crate::hash::{Digest, finish, keyed, object_id};
use crate::public::{ServedPositionV1, TensorWireV1};

pub const RETRIEVAL_KEY_DOMAIN_V1: &[u8] = b"misaka-palw/spec/retrieval-key/v1";
pub const RETRIEVAL_PAYLOAD_DOMAIN_V1: &[u8] = b"misaka-palw/spec/retrieval-payload/v1";
pub const RETRIEVAL_LEAF_DOMAIN_V1: &[u8] = b"misaka-palw/spec/retrieval-leaf/v1";
pub const RETRIEVAL_NODE_DOMAIN_V1: &[u8] = b"misaka-palw/spec/retrieval-node/v1";
pub const RETRIEVAL_SNAPSHOT_DOMAIN_V1: &[u8] = b"misaka-palw/spec/retrieval-snapshot/v1";
pub const RETRIEVAL_INDEX_DOMAIN_V1: &[u8] = b"misaka-palw/spec/retrieval-index/v1";

/// Structural ceilings of a snapshot (parse / DoS bounds; the carrier and the session cap bind earlier in practice).
pub const MAX_RETRIEVAL_ITEMS_V1: u64 = 1 << 40;
pub const MAX_RETRIEVAL_DIM_V1: u16 = 4096;
pub const MAX_RETRIEVAL_PAYLOAD_V1: u16 = 4096;
pub const MAX_RETRIEVAL_K_V1: u16 = 64;
pub const MAX_SLICE_ITEMS_V1: u32 = 1 << 16;

/// One snapshot item.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RetrievalItemV1 {
    pub key: Vec<i32>,
    pub payload: Vec<u32>,
}

impl RetrievalItemV1 {
    /// The item's leaf at `id`.
    pub fn leaf(&self, id: u64) -> Digest {
        leaf_v1(id, &key_digest_v1(&self.key), &payload_digest_v1(&self.payload))
    }

    /// Its encoded size, an upper bound for the bounds arithmetic: `8 + 4·|key| + 4·|payload| + 2·4`.
    pub fn bytes(dim: u16, payload: u16) -> u64 {
        16 + 4 * dim as u64 + 4 * payload as u64
    }
}

pub fn key_digest_v1(key: &[i32]) -> Digest {
    let mut s = keyed(RETRIEVAL_KEY_DOMAIN_V1);
    s.update(&(key.len() as u64).to_le_bytes());
    for v in key {
        s.update(&v.to_le_bytes());
    }
    finish(s)
}

pub fn payload_digest_v1(payload: &[u32]) -> Digest {
    let mut s = keyed(RETRIEVAL_PAYLOAD_DOMAIN_V1);
    s.update(&(payload.len() as u64).to_le_bytes());
    for v in payload {
        s.update(&v.to_le_bytes());
    }
    finish(s)
}

pub fn leaf_v1(id: u64, key_digest: &Digest, payload_digest: &Digest) -> Digest {
    let mut s = keyed(RETRIEVAL_LEAF_DOMAIN_V1);
    s.update(&id.to_le_bytes()).update(key_digest).update(payload_digest);
    finish(s)
}

// ---- the snapshot's Merkle tree ------------------------------------------------------------------------------------------

fn node_hash(l: &Digest, r: &Digest) -> Digest {
    let mut s = keyed(RETRIEVAL_NODE_DOMAIN_V1);
    s.update(l).update(r);
    finish(s)
}

fn next_level(level: &[Digest]) -> Vec<Digest> {
    level.chunks(2).map(|p| if p.len() == 2 { node_hash(&p[0], &p[1]) } else { p[0] }).collect()
}

/// The Merkle root over `leaves` (binary; an unpaired last node is carried up). `None` for no leaf.
pub fn merkle_root_v1(leaves: &[Digest]) -> Option<Digest> {
    if leaves.is_empty() {
        return None;
    }
    let mut level = leaves.to_vec();
    while level.len() > 1 {
        level = next_level(&level);
    }
    Some(level[0])
}

/// The siblings from leaf `index` to the root (a carried-up level contributes none).
pub fn merkle_path_v1(leaves: &[Digest], mut index: usize) -> Vec<Digest> {
    let mut out = Vec::new();
    let mut level = leaves.to_vec();
    while level.len() > 1 {
        let sib = index ^ 1;
        if sib < level.len() {
            out.push(level[sib]);
        }
        level = next_level(&level);
        index /= 2;
    }
    out
}

/// Whether `path` takes leaf `index` of a `count`-leaf tree to `root`. Exactly the siblings the tree has: never more.
pub fn merkle_verify_v1(leaf: Digest, mut index: u64, mut count: u64, path: &[Digest], root: &Digest) -> bool {
    if index >= count {
        return false;
    }
    let mut acc = leaf;
    let mut used = 0usize;
    while count > 1 {
        let sib = index ^ 1;
        if sib < count {
            let Some(s) = path.get(used) else { return false };
            acc = if index % 2 == 0 { node_hash(&acc, s) } else { node_hash(s, &acc) };
            used += 1;
        }
        index /= 2;
        count = count.div_ceil(2);
    }
    used == path.len() && acc == *root
}

/// `⌈log2 n⌉` (0 for `n ≤ 1`): the most siblings a path has.
pub fn depth_v1(n: u64) -> u32 {
    if n <= 1 { 0 } else { 64 - (n - 1).leading_zeros() }
}

// ---- the bound objects ---------------------------------------------------------------------------------------------------

/// The public snapshot a class binds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SnapshotV1 {
    /// `N`, the items.
    pub items: u64,
    /// `D`, every key's length.
    pub dim: u16,
    /// `P`, the longest payload.
    pub max_payload: u16,
    /// `B`, items per DA slice.
    pub slice_items: u32,
    pub merkle_root: Digest,
}

impl SnapshotV1 {
    /// The snapshot root: what the consumer attests public and a class binds.
    pub fn root(&self) -> Digest {
        object_id(RETRIEVAL_SNAPSHOT_DOMAIN_V1, self)
    }

    /// The number of DA slices.
    pub fn slices(&self) -> u64 {
        self.items.div_ceil(self.slice_items.max(1) as u64)
    }

    /// The ids of slice `t`: `[t·B, min((t+1)·B, N))`.
    pub fn slice_range(&self, t: u64) -> Option<std::ops::Range<u64>> {
        let b = self.slice_items as u64;
        let first = t.checked_mul(b)?;
        (first < self.items).then(|| first..(first + b).min(self.items))
    }

    pub fn well_formed(&self) -> Result<(), String> {
        if self.items == 0 || self.items > MAX_RETRIEVAL_ITEMS_V1 {
            return Err(format!("a snapshot holds 1..={MAX_RETRIEVAL_ITEMS_V1} items, not {}", self.items));
        }
        if self.dim == 0 || self.dim > MAX_RETRIEVAL_DIM_V1 {
            return Err(format!("a key has 1..={MAX_RETRIEVAL_DIM_V1} elements, not {}", self.dim));
        }
        if self.max_payload > MAX_RETRIEVAL_PAYLOAD_V1 {
            return Err(format!("a payload holds at most {MAX_RETRIEVAL_PAYLOAD_V1} tokens"));
        }
        if self.slice_items == 0 || self.slice_items > MAX_SLICE_ITEMS_V1 {
            return Err(format!("a slice holds 1..={MAX_SLICE_ITEMS_V1} items"));
        }
        Ok(())
    }
}

/// The deterministic index. `Flat`: the snapshot's own id order, every item scored (the rule is exact, so the index is the order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum IndexV1 {
    Flat = 0,
}

/// The retrieval rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum RetrievalRuleV1 {
    /// RFC-0002 FR-09's counting threshold: the `min(k, N)` items of largest `κ`, in descending `κ`.
    TopKCountingV1 { k: u16, score_bits: u8 } = 0,
}

/// **The `Retrieval` root kind** (version 1).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RetrievalRootV1 {
    /// The typed-roots extension descriptor's digest (`K2-TR-v1`).
    pub extension: Digest,
    pub snapshot: SnapshotV1,
    pub index: IndexV1,
    pub rule: RetrievalRuleV1,
}

impl RetrievalRootV1 {
    pub fn well_formed(&self) -> Result<(), String> {
        self.snapshot.well_formed()?;
        let RetrievalRuleV1::TopKCountingV1 { k, score_bits } = self.rule;
        if k == 0 || k > MAX_RETRIEVAL_K_V1 {
            return Err(format!("k is 1..={MAX_RETRIEVAL_K_V1}, not {k}"));
        }
        if !(8..=62).contains(&score_bits) {
            return Err(format!("the score is narrowed to 8..=62 bits, not {score_bits}"));
        }
        Ok(())
    }

    /// **The deterministic index commitment**: the index kind, the snapshot root and the rule.
    pub fn index_commitment(&self) -> Digest {
        object_id(RETRIEVAL_INDEX_DOMAIN_V1, &(self.index, self.snapshot.root(), self.rule))
    }

    /// The entries a result has: `min(k, N)`.
    pub fn result_len(&self) -> usize {
        let RetrievalRuleV1::TopKCountingV1 { k, .. } = self.rule;
        (k as u64).min(self.snapshot.items) as usize
    }

    fn score_bits(&self) -> u32 {
        let RetrievalRuleV1::TopKCountingV1 { score_bits, .. } = self.rule;
        score_bits as u32
    }

    /// The narrowed score of `key` for `query`: the exact dot product, saturated into `±(2^SB − 1)`. `Err` for a misshapen operand.
    pub fn score(&self, query: &[i32], key: &[i32]) -> Result<i64, String> {
        let d = self.snapshot.dim as usize;
        if query.len() != d || key.len() != d {
            return Err(format!("a query and a key have {d} elements"));
        }
        let dot: i128 = query.iter().zip(key).map(|(q, k)| *q as i128 * *k as i128).sum();
        let lim = (1i128 << self.score_bits()) - 1;
        Ok(dot.clamp(-lim, lim) as i64)
    }

    /// Whether a stated score is inside the narrowed range.
    pub fn score_in_range(&self, score: i64) -> bool {
        (score as i128).abs() < (1i128 << self.score_bits())
    }

    /// `κ(id) = (s + 2^SB)·2^b + (2^b − 1 − id)`: distinct per item, ties to the lowest id.
    pub fn kappa(&self, score: i64, id: u64) -> i128 {
        let b = depth_v1(self.snapshot.items).max(1);
        let sb = self.score_bits();
        ((score as i128 + (1i128 << sb)) << b) + ((1i128 << b) - 1 - id as i128)
    }
}

/// A retrieval job: the query.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RetrievalJobV1 {
    pub class: Digest,
    pub query: Vec<i32>,
    pub nonce: Digest,
}

/// One retrieved entry, as a claim states it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RetrievedV1 {
    pub id: u64,
    pub score: i64,
    pub key_digest: Digest,
    pub payload_digest: Digest,
}

/// A retrieval claim (signed by `producer_bond`).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RetrievalClaimV1 {
    pub job_id: Digest,
    pub producer_bond: Digest,
    pub result: Vec<RetrievedV1>,
}

/// **The inclusion checks of a result** (no opening needed): the length, the ids, the scores' range and the strict κ order.
pub fn check_result_v1(root: &RetrievalRootV1, result: &[RetrievedV1]) -> Result<(), String> {
    if result.len() != root.result_len() {
        return Err(format!("{} entries, the rule returns {}", result.len(), root.result_len()));
    }
    let mut last: Option<i128> = None;
    for (i, e) in result.iter().enumerate() {
        if e.id >= root.snapshot.items {
            return Err(format!("entry {i} names item {} of {}", e.id, root.snapshot.items));
        }
        if !root.score_in_range(e.score) {
            return Err(format!("entry {i}'s score is outside the rule's range"));
        }
        let k = root.kappa(e.score, e.id);
        if last.is_some_and(|l| k >= l) {
            return Err(format!("entry {i} is not below entry {} in the rule's order (κ)", i - 1));
        }
        last = Some(k);
    }
    Ok(())
}

/// A query the class can score.
pub fn check_query_v1(root: &RetrievalRootV1, query: &[i32]) -> Result<(), String> {
    if query.len() != root.snapshot.dim as usize {
        return Err(format!("a query has {} elements, the snapshot's keys {}", query.len(), root.snapshot.dim));
    }
    Ok(())
}

/// What a bond files against a retrieval result.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum RetrievalFaultV1 {
    /// Entry `index` is not the snapshot's item at its id (a wrong opening), or not that item's score.
    WrongItem { index: u16, item: RetrievalItemV1, path: Vec<Digest> } = 0,
    /// An item outside the result whose κ is above the last entry's.
    MissedBetter { id: u64, item: RetrievalItemV1, path: Vec<Digest> } = 1,
}

impl RetrievalFaultV1 {
    /// The filing's size bound (an item and a path).
    pub fn bytes(&self) -> usize {
        borsh::to_vec(self).map(|v| v.len()).unwrap_or(usize::MAX)
    }
}

/// An item opened at `id` against the snapshot: authenticated, and of the snapshot's shape.
fn authentic(root: &RetrievalRootV1, id: u64, item: &RetrievalItemV1, path: &[Digest]) -> Result<(), String> {
    if !well_shaped(root, item) {
        return Err("the opened item is not of the snapshot's shape".into());
    }
    authenticated(root, id, item, path)
}

/// Of the snapshot's shape: a key of length `D`, a payload of at most `P`.
fn well_shaped(root: &RetrievalRootV1, item: &RetrievalItemV1) -> bool {
    item.key.len() == root.snapshot.dim as usize && item.payload.len() <= root.snapshot.max_payload as usize
}

/// **The leaf at `id` is this item's**, whatever its shape (GAP-52): a registrant can commit a leaf that is no well-formed item, and
/// the courts must still judge a claim that names it.
fn authenticated(root: &RetrievalRootV1, id: u64, item: &RetrievalItemV1, path: &[Digest]) -> Result<(), String> {
    let s = &root.snapshot;
    if path.len() > depth_v1(s.items) as usize || !merkle_verify_v1(item.leaf(id), id, s.items, path, &s.merkle_root) {
        return Err("the opening does not authenticate against the snapshot root".into());
    }
    Ok(())
}

/// **The retrieval court.** `Ok(())`: convicted; `Err(why)`: dismissed (not authentic, or no fault). Reads the class root, the job's
/// query and the claimed result (all on chain) and the filing — nothing else. **Total over the snapshot (GAP-52):** the item at an id
/// is opened by its leaf alone; one that is not of the snapshot's shape is no item the rule can retrieve, so a claim that names it is
/// wrong (`WrongItem` convicts) and it beats nothing (`MissedBetter` dismisses).
pub fn judge_retrieval_fault_v1(
    root: &RetrievalRootV1,
    query: &[i32],
    result: &[RetrievedV1],
    fault: &RetrievalFaultV1,
) -> Result<(), String> {
    match fault {
        RetrievalFaultV1::WrongItem { index, item, path } => {
            let e = result.get(*index as usize).ok_or("no such entry")?;
            authenticated(root, e.id, item, path)?;
            if !well_shaped(root, item) {
                return Ok(());
            }
            let opened_wrong = key_digest_v1(&item.key) != e.key_digest || payload_digest_v1(&item.payload) != e.payload_digest;
            let score_wrong = root.score(query, &item.key)? != e.score;
            if opened_wrong || score_wrong { Ok(()) } else { Err("the entry is the snapshot's item with its score: no fault".into()) }
        }
        RetrievalFaultV1::MissedBetter { id, item, path } => {
            if result.iter().any(|e| e.id == *id) {
                return Err("the item is in the result (a wrong entry is a WrongItem filing)".into());
            }
            authenticated(root, *id, item, path)?;
            if !well_shaped(root, item) {
                return Err("a malformed item is no item the rule retrieves: it beats nothing".into());
            }
            let last = result.last().ok_or("an empty result")?;
            let better = root.kappa(root.score(query, &item.key)?, *id) > root.kappa(last.score, last.id);
            if better { Ok(()) } else { Err("the item does not beat the last entry: no fault".into()) }
        }
    }
}

// ---- the DA path: snapshot slices ----------------------------------------------------------------------------------------

/// The answer to a slice demand: every item of the slice, in id order, each with its path.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SliceResponseV1 {
    pub items: Vec<RetrievalItemV1>,
    pub paths: Vec<Vec<Digest>>,
}

fn key_tensor(key: &[i32]) -> Tensor {
    Tensor::new(DType::I32, vec![key.len()], key.iter().map(|v| *v as i128).collect()).expect("an i32 is an I32")
}

fn payload_tensor(payload: &[u32]) -> Tensor {
    Tensor::new(DType::Idx, vec![payload.len()], payload.iter().map(|v| *v as i128).collect()).expect("a u32 is an Idx")
}

/// **Classify a slice response** against the snapshot (`malformed`, `fake_opening`), or the served form: one `[key, payload]`
/// pair of tensors per item, in id order.
pub fn classify_slice_response_v1(root: &RetrievalRootV1, t: u64, bytes: &[u8]) -> Result<ServedPositionV1, &'static str> {
    let range = root.snapshot.slice_range(t).ok_or("malformed")?;
    let r: SliceResponseV1 = borsh::from_slice(bytes).map_err(|_| "malformed")?;
    if r.items.len() as u64 != range.end - range.start || r.paths.len() != r.items.len() {
        return Err("malformed");
    }
    let mut values = Vec::with_capacity(r.items.len());
    for ((id, item), path) in range.zip(&r.items).zip(&r.paths) {
        if item.key.len() != root.snapshot.dim as usize || item.payload.len() > root.snapshot.max_payload as usize {
            return Err("malformed");
        }
        if authentic(root, id, item, path).is_err() {
            return Err("fake_opening");
        }
        values.push(vec![Some(TensorWireV1::of(&key_tensor(&item.key))), Some(TensorWireV1::of(&payload_tensor(&item.payload)))]);
    }
    Ok(ServedPositionV1 { values, inputs: Vec::new() })
}

/// **The answer to an entry demand** (stage `0xC0 + s`, GAP-52): the item the claim's entry names, and its path.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct EntryResponseV1 {
    pub item: RetrievalItemV1,
    pub path: Vec<Digest>,
}

/// **Classify an entry demand's response**: the item at the entry's id, authenticated by its leaf against the snapshot root — of any
/// shape (a malformed one is served too: then `WrongItem` convicts from it, the producer could not have retrieved it). Served as one
/// value pair `[key, payload]` like a slice item. `malformed` for undecodable bytes or an item past the parse bounds, `fake_opening`
/// for one that does not reach the root at the entry's id.
pub fn classify_entry_response_v1(
    root: &RetrievalRootV1,
    entry: &RetrievedV1,
    bytes: &[u8],
) -> Result<ServedPositionV1, &'static str> {
    let r: EntryResponseV1 = borsh::from_slice(bytes).map_err(|_| "malformed")?;
    if r.item.key.len() > MAX_RETRIEVAL_DIM_V1 as usize || r.item.payload.len() > MAX_RETRIEVAL_PAYLOAD_V1 as usize {
        return Err("malformed");
    }
    if authenticated(root, entry.id, &r.item, &r.path).is_err() {
        return Err("fake_opening");
    }
    Ok(ServedPositionV1 {
        values: vec![vec![Some(TensorWireV1::of(&key_tensor(&r.item.key))), Some(TensorWireV1::of(&payload_tensor(&r.item.payload)))]],
        inputs: Vec::new(),
    })
}

/// The items of a served slice (what an outsider reads back from the ledger).
pub fn items_of_served_v1(sp: &ServedPositionV1) -> Option<Vec<RetrievalItemV1>> {
    sp.values
        .iter()
        .map(|pair| {
            let key = pair.first()?.as_ref()?.decode().ok()?;
            let payload = pair.get(1)?.as_ref()?.decode().ok()?;
            Some(RetrievalItemV1 {
                key: key.data.iter().map(|v| i32::try_from(*v).ok()).collect::<Option<_>>()?,
                payload: payload.data.iter().map(|v| u32::try_from(*v).ok()).collect::<Option<_>>()?,
            })
        })
        .collect()
}

// ---- the snapshot as a producer or an outsider holds it -------------------------------------------------------------------

/// A whole snapshot in hand: its items and leaves (a producer's, a DA provider's, or an outsider's after a full fetch).
#[derive(Clone, Debug)]
pub struct SnapshotDataV1 {
    pub items: Vec<RetrievalItemV1>,
    pub leaves: Vec<Digest>,
    pub snapshot: SnapshotV1,
}

impl SnapshotDataV1 {
    /// Build from items (every key of length `dim`, every payload at most `max_payload`).
    pub fn new(items: Vec<RetrievalItemV1>, dim: u16, max_payload: u16, slice_items: u32) -> Result<Self, String> {
        if items.iter().any(|i| i.key.len() != dim as usize || i.payload.len() > max_payload as usize) {
            return Err("an item is not of the snapshot's shape".into());
        }
        let leaves: Vec<Digest> = items.iter().enumerate().map(|(id, it)| it.leaf(id as u64)).collect();
        let merkle_root = merkle_root_v1(&leaves).ok_or("an empty snapshot")?;
        let snapshot = SnapshotV1 { items: items.len() as u64, dim, max_payload, slice_items, merkle_root };
        snapshot.well_formed()?;
        Ok(Self { items, leaves, snapshot })
    }

    /// Item `id` and its path.
    pub fn open(&self, id: u64) -> Option<(RetrievalItemV1, Vec<Digest>)> {
        let item = self.items.get(id as usize)?.clone();
        Some((item, merkle_path_v1(&self.leaves, id as usize)))
    }

    /// **The rule's result** for `query` (the honest producer's, and the outsider's reference).
    pub fn retrieve(&self, root: &RetrievalRootV1, query: &[i32]) -> Result<Vec<RetrievedV1>, String> {
        check_query_v1(root, query)?;
        let mut scored: Vec<(i128, u64, i64)> = Vec::with_capacity(self.items.len());
        for (id, it) in self.items.iter().enumerate() {
            let s = root.score(query, &it.key)?;
            scored.push((root.kappa(s, id as u64), id as u64, s));
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        Ok(scored
            .into_iter()
            .take(root.result_len())
            .map(|(_, id, score)| {
                let it = &self.items[id as usize];
                RetrievedV1 { id, score, key_digest: key_digest_v1(&it.key), payload_digest: payload_digest_v1(&it.payload) }
            })
            .collect())
    }

    /// **The first fault of a result**, or `None` (the outsider's check: every entry re-opened, then a scan for a better item).
    pub fn find_fault(&self, root: &RetrievalRootV1, query: &[i32], result: &[RetrievedV1]) -> Option<RetrievalFaultV1> {
        for (i, e) in result.iter().enumerate() {
            let (item, path) = self.open(e.id)?;
            let fault = RetrievalFaultV1::WrongItem { index: i as u16, item, path };
            if judge_retrieval_fault_v1(root, query, result, &fault).is_ok() {
                return Some(fault);
            }
        }
        let last = result.last()?;
        let floor = root.kappa(last.score, last.id);
        let mut best: Option<(i128, u64)> = None;
        for (id, it) in self.items.iter().enumerate() {
            let id = id as u64;
            if result.iter().any(|e| e.id == id) {
                continue;
            }
            let k = root.kappa(root.score(query, &it.key).ok()?, id);
            if k > floor && best.is_none_or(|(b, _)| k > b) {
                best = Some((k, id));
            }
        }
        let (_, id) = best?;
        let (item, path) = self.open(id)?;
        Some(RetrievalFaultV1::MissedBetter { id, item, path })
    }

    /// The honest answer to a demand for slice `t`.
    pub fn slice_response(&self, t: u64) -> Option<Vec<u8>> {
        let range = self.snapshot.slice_range(t)?;
        let (items, paths) = range.map(|id| self.open(id)).collect::<Option<Vec<_>>>()?.into_iter().unzip();
        Some(borsh::to_vec(&SliceResponseV1 { items, paths }).expect("in-memory borsh"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(n: usize, d: usize) -> Vec<RetrievalItemV1> {
        (0..n)
            .map(|i| RetrievalItemV1 {
                key: (0..d).map(|j| ((i * 7 + j * 3) % 11) as i32 - 5).collect(),
                payload: vec![(i % 30) as u32, ((i + 1) % 30) as u32],
            })
            .collect()
    }

    fn root_of(s: &SnapshotDataV1, k: u16) -> RetrievalRootV1 {
        RetrievalRootV1 {
            extension: [0; 64],
            snapshot: s.snapshot,
            index: IndexV1::Flat,
            rule: RetrievalRuleV1::TopKCountingV1 { k, score_bits: 20 },
        }
    }

    #[test]
    fn every_path_verifies_and_nothing_else_does() {
        for n in [1usize, 2, 3, 5, 8, 13] {
            let leaves: Vec<Digest> = (0..n).map(|i| [i as u8; 64]).collect();
            let root = merkle_root_v1(&leaves).unwrap();
            for i in 0..n {
                let p = merkle_path_v1(&leaves, i);
                assert!(p.len() as u32 <= depth_v1(n as u64));
                assert!(merkle_verify_v1(leaves[i], i as u64, n as u64, &p, &root), "n={n} i={i}");
                assert!(!merkle_verify_v1(leaves[i], (i as u64 + 1) % n as u64, n as u64, &p, &root) || n == 1);
                let mut longer = p.clone();
                longer.push([9; 64]);
                assert!(!merkle_verify_v1(leaves[i], i as u64, n as u64, &longer, &root), "an extra sibling is refused");
            }
            assert!(!merkle_verify_v1(leaves[0], n as u64, n as u64, &[], &root), "an index past the tree is refused");
        }
    }

    #[test]
    fn the_rule_is_the_sort_with_ties_to_the_lowest_id() {
        // Many ties: keys from a tiny alphabet.
        let s = SnapshotDataV1::new(items(37, 4), 4, 2, 8).unwrap();
        let root = root_of(&s, 5);
        let q = vec![1, -2, 3, 0];
        let got = s.retrieve(&root, &q).unwrap();
        let mut sorted: Vec<(i64, u64)> =
            s.items.iter().enumerate().map(|(i, it)| (root.score(&q, &it.key).unwrap(), i as u64)).collect();
        sorted.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        assert_eq!(got.iter().map(|e| (e.score, e.id)).collect::<Vec<_>>(), sorted[..5].to_vec());
        check_result_v1(&root, &got).unwrap();
        assert_eq!(s.find_fault(&root, &q, &got), None, "an honest result has no fault");
    }

    #[test]
    fn a_wrong_item_and_a_missed_better_item_are_convicted_and_honest_entries_are_not() {
        let s = SnapshotDataV1::new(items(40, 4), 4, 2, 8).unwrap();
        let root = root_of(&s, 3);
        let q = vec![2, 1, -1, 3];
        let honest = s.retrieve(&root, &q).unwrap();
        // A wrong payload digest at entry 1.
        let mut wrong = honest.clone();
        wrong[1].payload_digest = payload_digest_v1(&[9, 9]);
        let f = s.find_fault(&root, &q, &wrong).unwrap();
        assert!(matches!(f, RetrievalFaultV1::WrongItem { index: 1, .. }));
        assert!(judge_retrieval_fault_v1(&root, &q, &wrong, &f).is_ok());
        assert!(judge_retrieval_fault_v1(&root, &q, &honest, &f).is_err(), "the same opening against the honest result: no fault");
        // A wrong score.
        let mut wrong = honest.clone();
        wrong[2].score -= 1;
        if check_result_v1(&root, &wrong).is_ok() {
            assert!(matches!(s.find_fault(&root, &q, &wrong), Some(RetrievalFaultV1::WrongItem { index: 2, .. })));
        }
        // The best item dropped: the rest shifted up and the 4th appended.
        let all = {
            let mut r = root.clone();
            r.rule = RetrievalRuleV1::TopKCountingV1 { k: 4, score_bits: 20 };
            s.retrieve(&r, &q).unwrap()
        };
        let missed = vec![all[1], all[2], all[3]];
        check_result_v1(&root, &missed).unwrap();
        let f = s.find_fault(&root, &q, &missed).unwrap();
        let RetrievalFaultV1::MissedBetter { id, .. } = &f else { panic!("{f:?}") };
        assert_eq!(*id, all[0].id);
        assert!(judge_retrieval_fault_v1(&root, &q, &missed, &f).is_ok());
        assert!(judge_retrieval_fault_v1(&root, &q, &honest, &f).is_err(), "it is in the honest result");
        // A forged opening is not authentic.
        let RetrievalFaultV1::MissedBetter { id, mut item, path } = f else { unreachable!() };
        item.key[0] += 100;
        assert!(
            judge_retrieval_fault_v1(&root, &q, &missed, &RetrievalFaultV1::MissedBetter { id, item, path })
                .unwrap_err()
                .contains("authenticate")
        );
    }

    #[test]
    fn inclusion_refuses_a_misordered_duplicate_or_out_of_range_result() {
        let s = SnapshotDataV1::new(items(20, 4), 4, 2, 8).unwrap();
        let root = root_of(&s, 3);
        let q = vec![1, 1, 1, 1];
        let honest = s.retrieve(&root, &q).unwrap();
        let mut swapped = honest.clone();
        swapped.swap(0, 1);
        assert!(check_result_v1(&root, &swapped).is_err());
        let mut dup = honest.clone();
        dup[2] = dup[1];
        assert!(check_result_v1(&root, &dup).is_err());
        let mut far = honest.clone();
        far[2].id = 20;
        assert!(check_result_v1(&root, &far).is_err());
        assert!(check_result_v1(&root, &honest[..2]).is_err(), "the rule returns min(k, N)");
    }

    #[test]
    fn a_slice_is_served_whole_and_a_forged_one_is_a_fake_opening() {
        let s = SnapshotDataV1::new(items(21, 4), 4, 2, 8).unwrap();
        let root = root_of(&s, 3);
        assert_eq!(s.snapshot.slices(), 3);
        let bytes = s.slice_response(2).unwrap();
        let served = classify_slice_response_v1(&root, 2, &bytes).unwrap();
        assert_eq!(items_of_served_v1(&served).unwrap(), s.items[16..21].to_vec());
        let mut r: SliceResponseV1 = borsh::from_slice(&bytes).unwrap();
        r.items[0].payload[0] += 1;
        assert_eq!(classify_slice_response_v1(&root, 2, &borsh::to_vec(&r).unwrap()), Err("fake_opening"));
        r.items.pop();
        assert_eq!(classify_slice_response_v1(&root, 2, &borsh::to_vec(&r).unwrap()), Err("malformed"));
        assert_eq!(classify_slice_response_v1(&root, 3, &bytes), Err("malformed"), "no slice 3");
        assert_eq!(classify_slice_response_v1(&root, 2, &[1, 2, 3]), Err("malformed"));
    }

    /// **GAP-52**: a registrant commits a leaf that is no well-formed item (a key of another length). A claim that names it is
    /// convicted from the opened item alone (`WrongItem`), the item never "beats" an entry (`MissedBetter` dismissed), and an entry
    /// demand serves it (authenticated by its leaf) so any bond can file that conviction; a forged path is a fake opening.
    #[test]
    fn a_malformed_snapshot_leaf_is_judged_and_an_entry_demand_serves_the_claims_own_item() {
        let mut all = items(9, 4);
        all[5] = RetrievalItemV1 { key: vec![100, 100, 100], payload: vec![1] }; // D = 4: malformed, and a huge score if it scored
        let leaves: Vec<Digest> = all.iter().enumerate().map(|(i, it)| it.leaf(i as u64)).collect();
        let snapshot = SnapshotV1 { items: 9, dim: 4, max_payload: 2, slice_items: 4, merkle_root: merkle_root_v1(&leaves).unwrap() };
        let root = RetrievalRootV1 {
            extension: [0; 64],
            snapshot,
            index: IndexV1::Flat,
            rule: RetrievalRuleV1::TopKCountingV1 { k: 2, score_bits: 20 },
        };
        let q = vec![1, 1, 1, 1];
        let path = |id: usize| merkle_path_v1(&leaves, id);
        // The honest result over the well-formed items.
        let mut scored: Vec<(i128, u64, i64)> = all
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != 5)
            .map(|(i, it)| {
                let sc = root.score(&q, &it.key).unwrap();
                (root.kappa(sc, i as u64), i as u64, sc)
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        let entry = |id: u64, score: i64| RetrievedV1 {
            id,
            score,
            key_digest: key_digest_v1(&all[id as usize].key),
            payload_digest: payload_digest_v1(&all[id as usize].payload),
        };
        let honest: Vec<RetrievedV1> = scored[..2].iter().map(|(_, id, sc)| entry(*id, *sc)).collect();
        // A lying claim puts the malformed item first with a top score (its digests are the leaf's own).
        let top = (1i64 << 20) - 1;
        let lie = vec![entry(5, top), honest[0]];
        check_result_v1(&root, &lie).unwrap();
        let wrong = RetrievalFaultV1::WrongItem { index: 0, item: all[5].clone(), path: path(5) };
        judge_retrieval_fault_v1(&root, &q, &lie, &wrong).expect("a claim naming a malformed item is convicted");
        // Against the honest result the malformed item is no better entry, and an honest entry is no wrong one.
        let missed = RetrievalFaultV1::MissedBetter { id: 5, item: all[5].clone(), path: path(5) };
        assert!(judge_retrieval_fault_v1(&root, &q, &honest, &missed).is_err(), "a malformed item beats nothing");
        let fine =
            RetrievalFaultV1::WrongItem { index: 0, item: all[honest[0].id as usize].clone(), path: path(honest[0].id as usize) };
        assert!(judge_retrieval_fault_v1(&root, &q, &honest, &fine).is_err());
        // The entry demand: the claim's own item, served whatever its shape; a forged path is not.
        let bytes = borsh::to_vec(&EntryResponseV1 { item: all[5].clone(), path: path(5) }).unwrap();
        let served = classify_entry_response_v1(&root, &lie[0], &bytes).expect("served: the leaf is its");
        assert_eq!(items_of_served_v1(&served).unwrap(), vec![all[5].clone()], "public from now on");
        let forged = borsh::to_vec(&EntryResponseV1 { item: all[5].clone(), path: path(4) }).unwrap();
        assert_eq!(classify_entry_response_v1(&root, &lie[0], &forged), Err("fake_opening"));
        assert_eq!(classify_entry_response_v1(&root, &lie[0], &[1, 2]), Err("malformed"));
        // A leaf with NO item behind it (a digest nobody can open): no response classifies — withheld by construction, the
        // producer's default at the deadline; and no court can open it either way, so only the entry demand reaches a terminal.
        let mut junk_leaves = leaves.clone();
        junk_leaves[5] = [7; 64];
        let junk_root = RetrievalRootV1 {
            snapshot: SnapshotV1 { merkle_root: merkle_root_v1(&junk_leaves).unwrap(), ..root.snapshot },
            ..root.clone()
        };
        let try_open = borsh::to_vec(&EntryResponseV1 { item: all[5].clone(), path: merkle_path_v1(&junk_leaves, 5) }).unwrap();
        assert_eq!(classify_entry_response_v1(&junk_root, &lie[0], &try_open), Err("fake_opening"));
        let wrong = RetrievalFaultV1::WrongItem { index: 0, item: all[5].clone(), path: merkle_path_v1(&junk_leaves, 5) };
        assert!(judge_retrieval_fault_v1(&junk_root, &q, &lie, &wrong).is_err(), "no court opens a junk leaf: the DA path decides");
    }
}
