//! RFC-0008 v2 — **the weightless carriage**: EXEC blocks reach the chain through a committed anchor, never through a parent.
//!
//! The v1 round lane let a chain block merge round blocks as GHOSTDAG parents. That is what coupled the lane's liveness to the
//! chain's mergeset rules (one stale unmergeable round tip, then every later round block refused for merge depth —
//! testnet-12, 2026-10-01..03) and what let a round block sit in a mergeset at all. Past `Params::palw_exec_payload_v2` both
//! v2 subtypes — `EXEC_TX` and `EXEC_SLICE` — are carried the same way:
//!
//! * **A chain block names no EXEC block as a parent.** Lane edges are execution data, not consensus parents: the chain's
//!   GHOSTDAG, mergeset, merge depth, k-cluster, DAA window, blue score, blue work and pruning level never see an EXEC block.
//!   (An EXEC block hangs from the chain through ONE chain parent — its anchor — and any lane parents.)
//! * **A chain block that wants the lane accepted ends its coinbase `extra_data` with an anchor trailer** ([`PalwExecV2AnchorV1`]):
//!   up to [`PALW_EXEC_V2_MAX_HEADS`] lane heads, the number of EXEC blocks they newly cover, and the Merkle root over those
//!   blocks. The coinbase is inside `hash_merkle_root`, which is in the pre-PoW header, so the anchor is committed before the
//!   grind and nobody can re-wrap a solved block. No header field moves.
//! * **What an anchor covers is the closure of its heads through lane-parent edges** ([`palw_exec_v2_closure_v1`]), stopping at
//!   a block the parent state already anchored (covered once, never walked past) and **dropping** a block outside the two-span
//!   window, anchored on another chain, or from the legacy v1 carriage. A dropped head contributes nothing and invalidates
//!   nothing — the lane resumes from its latest anchored checkpoint — so a stale or foreign head can never wedge a template or a
//!   validating node. The covered set is in a canonical order that includes the subtype and respects predecessor dependencies
//!   (`EXEC_TX` by `(round, permit index)`, then `EXEC_SLICE` by `(root, index)`, ties by block hash).
//! * **Accepting it is the fold's, per ledger**: each covered `EXEC_TX` is judged against the parent state's permit schedule and
//!   spends one permit; each covered `EXEC_SLICE` is judged by the six admission rules and credits one canonical range. The two
//!   ledgers never read each other, and a missing or invalid carrier delays or skips that carrier alone — it never stalls the
//!   chain, and heartbeat / BASE-0 never read any of it.
//!
//! Everything here is a pure function over what the producer and the verifier both read, so the template builds the trailer the
//! validating walk recomputes. Integer only; checked arithmetic.

use crate::{BlockHash, Hash64};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// The trailer's magic: the last four bytes of a coinbase `extra_data` that carries a v2 anchor. Not `PXA1` (ADR-0168's
/// unshipped trailer): the two never share bytes.
pub const PALW_EXEC_V2_ANCHOR_MAGIC: [u8; 4] = *b"PXA2";

/// The trailer's wire version.
pub const PALW_EXEC_V2_ANCHOR_VERSION: u8 = 1;

/// The most lane heads one anchor names.
pub const PALW_EXEC_V2_MAX_HEADS: usize = 8;

/// Version byte, head-count byte, `count` (u32) and the 64-byte root: the body without its heads.
const PALW_EXEC_V2_ANCHOR_BODY_FIXED: usize = 1 + 1 + 4 + 64;

/// The bytes a trailer adds to `extra_data` at most: the body plus its length and the magic. Bounds the coinbase's growth.
pub const PALW_EXEC_V2_ANCHOR_MAX_TRAILER_BYTES: usize = PALW_EXEC_V2_ANCHOR_BODY_FIXED + 64 * PALW_EXEC_V2_MAX_HEADS + 2 + 4;

/// The domain of a covered block's leaf.
pub const PALW_EXEC_V2_ANCHOR_LEAF_DOMAIN: &[u8] = b"misaka-palw/exec-v2/anchor/leaf/v1";
/// The domain of an interior node.
pub const PALW_EXEC_V2_ANCHOR_NODE_DOMAIN: &[u8] = b"misaka-palw/exec-v2/anchor/node/v1";
/// The domain of the root over the leaves (it binds the leaf count).
pub const PALW_EXEC_V2_ANCHOR_ROOT_DOMAIN: &[u8] = b"misaka-palw/exec-v2/anchor/root/v1";

/// Every keyed-BLAKE2b domain this module hashes under, for the distinctness test.
pub const PALW_EXEC_V2_ANCHOR_ALL_DOMAINS: &[&[u8]] =
    &[PALW_EXEC_V2_ANCHOR_LEAF_DOMAIN, PALW_EXEC_V2_ANCHOR_NODE_DOMAIN, PALW_EXEC_V2_ANCHOR_ROOT_DOMAIN];

/// Why a lane block was left out of an anchor's coverage — what the lane's health reports as the reason a block was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PalwExecV2RejectReasonV1 {
    /// Its anchor's span is outside the two-span window of the anchoring block.
    OutsideWindow,
    /// Its anchor does not lie on the anchoring block's selected chain.
    ForeignAnchor,
    /// It carries the legacy v1 envelope (`PXR1`): v1 blocks are accepted through the v1 mergeset path only, never through an
    /// anchor — one carrier is never accepted through both paths.
    LegacyCarriage,
}

impl PalwExecV2RejectReasonV1 {
    pub fn as_str(self) -> &'static str {
        match self {
            PalwExecV2RejectReasonV1::OutsideWindow => "outside-window",
            PalwExecV2RejectReasonV1::ForeignAnchor => "foreign-anchor",
            PalwExecV2RejectReasonV1::LegacyCarriage => "legacy-carriage",
        }
    }
}

/// Every named refusal of the anchor carriage. A hostile trailer or head list is refused by one of these, never by a panic.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PalwExecV2AnchorErrorV1 {
    #[error("the anchor trailer is malformed: {0}")]
    Malformed(&'static str),
    #[error("the anchor trailer's version {0} is not {PALW_EXEC_V2_ANCHOR_VERSION}")]
    Version(u8),
    #[error("an anchor names {0} heads: it must name between 1 and {PALW_EXEC_V2_MAX_HEADS}")]
    HeadCount(usize),
    #[error("the anchor's heads are not strictly ascending: a head list has one spelling")]
    HeadsNotCanonical,
    #[error("the anchor's head {0} is not an EXEC block of the lane")]
    HeadNotLane(BlockHash),
    #[error("the anchor reaches {0} EXEC blocks, more than the {1} one anchor may cover")]
    TooManyLeaves(usize, usize),
    #[error("the anchor's count {declared} is not the {actual} EXEC blocks its heads cover")]
    CountMismatch { declared: u32, actual: usize },
    #[error("the anchor's root is not the root over the EXEC blocks its heads cover")]
    RootMismatch,
    #[error("the anchor names {0}, which this node does not hold yet")]
    MissingBlock(BlockHash),
    #[error("an EXEC block's coinbase carries an anchor: only a chain block anchors the lane")]
    LaneBlockAnchors,
    #[error("an anchor rides a block before the EXEC payload is armed")]
    BeforeFence,
}

/// **One anchor**: what a chain block commits about the lane. Carried as a trailer of the coinbase's `extra_data`
/// ([`palw_exec_v2_anchor_split`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwExecV2AnchorV1 {
    /// The lane heads the block anchors, strictly ascending.
    pub heads: Vec<BlockHash>,
    /// How many EXEC blocks the heads newly cover.
    pub count: u32,
    /// The Merkle root over those blocks ([`palw_exec_v2_root`]).
    pub root: Hash64,
}

impl PalwExecV2AnchorV1 {
    /// The anchor of a closure the node computed: its heads (sorted, de-duplicated), its size and root.
    pub fn of_closure(heads: &[BlockHash], closure: &PalwExecV2ClosureV1) -> Result<Self, PalwExecV2AnchorErrorV1> {
        let heads: Vec<BlockHash> = heads.iter().copied().collect::<BTreeSet<_>>().into_iter().collect();
        let anchor = Self { heads, count: closure.members.len() as u32, root: closure.root() };
        anchor.validate_shape()?;
        Ok(anchor)
    }

    /// Shape only: what the trailer decides from its own bytes.
    pub fn validate_shape(&self) -> Result<(), PalwExecV2AnchorErrorV1> {
        if self.heads.is_empty() || self.heads.len() > PALW_EXEC_V2_MAX_HEADS {
            return Err(PalwExecV2AnchorErrorV1::HeadCount(self.heads.len()));
        }
        if self.heads.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(PalwExecV2AnchorErrorV1::HeadsNotCanonical);
        }
        Ok(())
    }

    fn body(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(PALW_EXEC_V2_ANCHOR_BODY_FIXED + 64 * self.heads.len());
        out.push(PALW_EXEC_V2_ANCHOR_VERSION);
        out.push(self.heads.len() as u8);
        for head in &self.heads {
            out.extend_from_slice(head.as_byte_slice());
        }
        out.extend_from_slice(&self.count.to_le_bytes());
        out.extend_from_slice(self.root.as_byte_slice());
        out
    }

    /// The bytes a trailer appends: body, its length (u16 LE), the magic.
    pub fn trailer(&self) -> Vec<u8> {
        let mut out = self.body();
        let body_len = out.len() as u16;
        out.extend_from_slice(&body_len.to_le_bytes());
        out.extend_from_slice(&PALW_EXEC_V2_ANCHOR_MAGIC);
        out
    }

    fn decode_body(body: &[u8]) -> Result<Self, PalwExecV2AnchorErrorV1> {
        use PalwExecV2AnchorErrorV1::Malformed;
        if body.len() < PALW_EXEC_V2_ANCHOR_BODY_FIXED {
            return Err(Malformed("shorter than the fixed part"));
        }
        if body[0] != PALW_EXEC_V2_ANCHOR_VERSION {
            return Err(PalwExecV2AnchorErrorV1::Version(body[0]));
        }
        let heads_len = body[1] as usize;
        if heads_len == 0 || heads_len > PALW_EXEC_V2_MAX_HEADS {
            return Err(PalwExecV2AnchorErrorV1::HeadCount(heads_len));
        }
        if body.len() != PALW_EXEC_V2_ANCHOR_BODY_FIXED + 64 * heads_len {
            return Err(Malformed("length does not match the head count"));
        }
        let hash_at = |at: usize| {
            let mut bytes = [0u8; 64];
            bytes.copy_from_slice(&body[at..at + 64]);
            Hash64::from_bytes(bytes)
        };
        let heads: Vec<BlockHash> = (0..heads_len).map(|i| hash_at(2 + 64 * i)).collect();
        let tail = 2 + 64 * heads_len;
        let mut count = [0u8; 4];
        count.copy_from_slice(&body[tail..tail + 4]);
        let anchor = Self { heads, count: u32::from_le_bytes(count), root: hash_at(tail + 4) };
        anchor.validate_shape()?;
        Ok(anchor)
    }
}

/// **Split a coinbase `extra_data` (or payload) into the miner's own bytes and an anchor trailer, if it ends in one.** A tail that is
/// not the magic is the miner's and is returned whole. A tail that IS the magic is an anchor, and a malformed one is refused — a
/// block that says it anchors and cannot is not a block that anchors nothing.
pub fn palw_exec_v2_anchor_split(extra_data: &[u8]) -> Result<(&[u8], Option<PalwExecV2AnchorV1>), PalwExecV2AnchorErrorV1> {
    use PalwExecV2AnchorErrorV1::Malformed;
    let Some(without_magic) = extra_data.strip_suffix(&PALW_EXEC_V2_ANCHOR_MAGIC) else {
        return Ok((extra_data, None));
    };
    if without_magic.len() < 2 {
        return Err(Malformed("magic without a length"));
    }
    let (rest, len_bytes) = without_magic.split_at(without_magic.len() - 2);
    let body_len = u16::from_le_bytes([len_bytes[0], len_bytes[1]]) as usize;
    if body_len > rest.len() {
        return Err(Malformed("length exceeds the extra data"));
    }
    let (prefix, body) = rest.split_at(rest.len() - body_len);
    Ok((prefix, Some(PalwExecV2AnchorV1::decode_body(body)?)))
}

/// **The lane heads a block's coinbase anchors**, for a caller that only needs its dependencies. A block with no (or a malformed)
/// trailer names none: the shape check is the body stage's, not this reader's.
pub fn palw_exec_v2_anchor_heads_of_block(block: &crate::block::Block) -> Vec<BlockHash> {
    block
        .transactions
        .first()
        .and_then(|coinbase| palw_exec_v2_anchor_split(&coinbase.payload).ok())
        .and_then(|(_, anchor)| anchor)
        .map(|anchor| anchor.heads)
        .unwrap_or_default()
}

/// `extra_data` with `anchor` as its trailer, replacing any trailer already there.
pub fn palw_exec_v2_anchor_append(extra_data: &[u8], anchor: &PalwExecV2AnchorV1) -> Result<Vec<u8>, PalwExecV2AnchorErrorV1> {
    let (prefix, _) = palw_exec_v2_anchor_split(extra_data)?;
    let mut out = prefix.to_vec();
    out.extend_from_slice(&anchor.trailer());
    Ok(out)
}

fn keyed64(domain: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(domain).to_state();
    for part in parts {
        state.update(part);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **Which payload a covered block carries, with its ordering coordinates.** The derived order is the canonical covered-set
/// order: every `EXEC_TX` (by round, then permit index) before every `EXEC_SLICE` (by root, then index), so a slice
/// is always processed after its predecessor index of the same root, and the two ledgers see their own order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PalwExecV2MemberKeyV1 {
    Tx { round: u64, permit_index: u16 },
    Slice { root_claim_id: Hash64, slice_index: u32 },
}

/// One covered EXEC block: what its leaf commits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PalwExecV2MemberV1 {
    pub key: PalwExecV2MemberKeyV1,
    pub hash: BlockHash,
}

impl PalwExecV2MemberV1 {
    pub fn leaf(&self) -> Hash64 {
        match self.key {
            PalwExecV2MemberKeyV1::Tx { round, permit_index } => keyed64(
                PALW_EXEC_V2_ANCHOR_LEAF_DOMAIN,
                &[self.hash.as_byte_slice(), &[1u8], &round.to_le_bytes(), &permit_index.to_le_bytes()],
            ),
            PalwExecV2MemberKeyV1::Slice { root_claim_id, slice_index } => keyed64(
                PALW_EXEC_V2_ANCHOR_LEAF_DOMAIN,
                &[self.hash.as_byte_slice(), &[2u8], root_claim_id.as_byte_slice(), &slice_index.to_le_bytes()],
            ),
        }
    }
}

/// **The root over the covered blocks**, in canonical order. The leaf count is bound into the root, so the empty set has a root
/// of its own (never a default hash) and an odd leaf cannot be promoted into a different tree.
pub fn palw_exec_v2_root(members: &[PalwExecV2MemberV1]) -> Hash64 {
    let mut ordered = members.to_vec();
    ordered.sort();
    let mut level: Vec<Hash64> = ordered.iter().map(PalwExecV2MemberV1::leaf).collect();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut chunks = level.chunks_exact(2);
        for pair in &mut chunks {
            next.push(keyed64(PALW_EXEC_V2_ANCHOR_NODE_DOMAIN, &[pair[0].as_byte_slice(), pair[1].as_byte_slice()]));
        }
        if let [odd] = chunks.remainder() {
            next.push(*odd);
        }
        level = next;
    }
    let top = level.first().map(|hash| hash.as_byte_slice().to_vec()).unwrap_or_default();
    keyed64(PALW_EXEC_V2_ANCHOR_ROOT_DOMAIN, &[&(ordered.len() as u64).to_le_bytes(), &top])
}

/// **The two-span window**: an EXEC block anchored in span `anchor_span` is coverable by a block in span `span_now` when the
/// anchor's span is this one or the one before — the two spans the fold keeps schedules and permit ledgers for.
pub fn palw_exec_v2_window_ok(anchor_span: u64, span_now: u64) -> bool {
    anchor_span <= span_now && span_now - anchor_span <= 1
}

/// What the closure needs to know about a block. `None` from [`PalwExecV2DagV1::node`] means the header is not held — a
/// dependency the node must fetch, never a verdict.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwExecV2NodeV1 {
    /// An algo-10 block of an open lane. A chain block (or the origin) is not.
    pub is_lane: bool,
    /// The lane block carries the legacy v1 envelope.
    pub legacy: bool,
    /// The coordinates of a v2 lane block.
    pub key: Option<PalwExecV2MemberKeyV1>,
    /// The span of the block's anchor (its selected parent).
    pub anchor_span: u64,
    /// The block's anchor lies on the anchoring block's selected chain.
    pub anchor_on_chain: bool,
    /// The block's direct parents; the closure follows the lane ones.
    pub parents: Vec<BlockHash>,
}

/// The DAG a closure walks. The consensus implementation reads the header and GHOSTDAG stores and the reachability service; a test
/// implements it over a map.
pub trait PalwExecV2DagV1 {
    fn node(&self, hash: &BlockHash) -> Option<PalwExecV2NodeV1>;
}

/// **The EXEC blocks an anchor covers.**
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PalwExecV2ClosureV1 {
    /// Canonical order.
    pub members: Vec<PalwExecV2MemberV1>,
    /// Blocks the walk reached and left out, with the reason — the lane's health reads them.
    pub dropped: BTreeMap<BlockHash, PalwExecV2RejectReasonV1>,
}

impl PalwExecV2ClosureV1 {
    pub fn root(&self) -> Hash64 {
        palw_exec_v2_root(&self.members)
    }
}

/// **The closure of `heads` through lane-parent edges** — the blocks one anchor covers.
///
/// * A head that is not a lane block is refused (`HeadNotLane`); a block the DAG does not hold is a dependency (`MissingBlock`).
/// * A block `already_anchored` by the parent state is a boundary: not covered again, not walked past.
/// * A block outside the window, anchored on another chain, or from the legacy carriage is **dropped**: a boundary that is not
///   walked past either, and recorded with its reason. A stale head therefore costs one probe and invalidates nothing — the
///   property that keeps a wedged lane from wedging the chain.
/// * More than `max_leaves` covered blocks is refused (`TooManyLeaves`); the walk stops as soon as it knows, so a hostile lane
///   costs at most `max_leaves + 1` visits plus the boundary probes of the blocks it reached.
pub fn palw_exec_v2_closure_v1(
    dag: &impl PalwExecV2DagV1,
    heads: &[BlockHash],
    already_anchored: impl Fn(&BlockHash) -> bool,
    span_now: u64,
    max_leaves: usize,
) -> Result<PalwExecV2ClosureV1, PalwExecV2AnchorErrorV1> {
    if heads.is_empty() || heads.len() > PALW_EXEC_V2_MAX_HEADS {
        return Err(PalwExecV2AnchorErrorV1::HeadCount(heads.len()));
    }
    let mut closure = PalwExecV2ClosureV1::default();
    let mut seen: BTreeSet<BlockHash> = BTreeSet::new();
    let mut stack: Vec<(BlockHash, bool)> = heads.iter().rev().map(|head| (*head, true)).collect();
    while let Some((hash, is_head)) = stack.pop() {
        if !seen.insert(hash) {
            continue;
        }
        let node = dag.node(&hash).ok_or(PalwExecV2AnchorErrorV1::MissingBlock(hash))?;
        if !node.is_lane {
            if is_head {
                return Err(PalwExecV2AnchorErrorV1::HeadNotLane(hash));
            }
            // A chain parent (the block's anchor): where the lane hangs from the chain, not part of the lane.
            continue;
        }
        if already_anchored(&hash) {
            continue;
        }
        if node.legacy {
            closure.dropped.insert(hash, PalwExecV2RejectReasonV1::LegacyCarriage);
            continue;
        }
        if !node.anchor_on_chain {
            closure.dropped.insert(hash, PalwExecV2RejectReasonV1::ForeignAnchor);
            continue;
        }
        if !palw_exec_v2_window_ok(node.anchor_span, span_now) {
            closure.dropped.insert(hash, PalwExecV2RejectReasonV1::OutsideWindow);
            continue;
        }
        let Some(key) = node.key else {
            // A lane block whose coordinates cannot be read is not coverable: dropped like a legacy one.
            closure.dropped.insert(hash, PalwExecV2RejectReasonV1::LegacyCarriage);
            continue;
        };
        if closure.members.len() >= max_leaves {
            return Err(PalwExecV2AnchorErrorV1::TooManyLeaves(closure.members.len() + 1, max_leaves));
        }
        closure.members.push(PalwExecV2MemberV1 { key, hash });
        for parent in node.parents.iter().rev() {
            if !seen.contains(parent) {
                stack.push((*parent, false));
            }
        }
    }
    closure.members.sort();
    Ok(closure)
}

/// **Verify a trailer against the closure the verifier computed.** The producer builds the trailer with
/// [`PalwExecV2AnchorV1::of_closure`] over the same closure, so this is the same function read the other way.
pub fn palw_exec_v2_anchor_verify(anchor: &PalwExecV2AnchorV1, closure: &PalwExecV2ClosureV1) -> Result<(), PalwExecV2AnchorErrorV1> {
    if anchor.count as usize != closure.members.len() {
        return Err(PalwExecV2AnchorErrorV1::CountMismatch { declared: anchor.count, actual: closure.members.len() });
    }
    if anchor.root != closure.root() {
        return Err(PalwExecV2AnchorErrorV1::RootMismatch);
    }
    Ok(())
}

/// **What the fold needs of one anchoring block**: the EXEC blocks it covers (with the span each is anchored in) and the clock the
/// window is read at. Built by the virtual processor from the closure; `None` in the fold's extras where the block anchors nothing.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PalwExecV2AnchorFoldV1 {
    /// `(block, anchor span)` of every covered block.
    pub members: Vec<(BlockHash, u64)>,
    /// The span of the anchoring block: entries older than the window are dropped from the anchored set.
    pub span_now: u64,
}

/// **The lane's health**, as an RPC reports it: the newest head this node holds and whether the next anchoring block could still
/// cover it. Node-local; no consensus rule reads it.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PalwExecV2LaneHealthV1 {
    pub latest_head: Option<BlockHash>,
    /// The newest head can no longer be anchored (outside the window, on another chain or legacy): the lane must resume from its
    /// checkpoint.
    pub stale: bool,
    pub stale_reason: Option<PalwExecV2RejectReasonV1>,
    /// How many EXEC blocks the next anchoring block would cover now.
    pub pending: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }

    #[derive(Default)]
    struct MapDag(BTreeMap<BlockHash, PalwExecV2NodeV1>);

    impl MapDag {
        fn chain(&mut self, id: u64) {
            self.0.insert(
                h(id),
                PalwExecV2NodeV1 { is_lane: false, legacy: false, key: None, anchor_span: 0, anchor_on_chain: true, parents: vec![] },
            );
        }
        fn lane(&mut self, id: u64, key: PalwExecV2MemberKeyV1, anchor: u64, anchor_span: u64, parents: &[u64]) {
            self.0.insert(
                h(id),
                PalwExecV2NodeV1 {
                    is_lane: true,
                    legacy: false,
                    key: Some(key),
                    anchor_span,
                    anchor_on_chain: true,
                    parents: std::iter::once(anchor).chain(parents.iter().copied()).map(h).collect(),
                },
            );
        }
        fn tx(&mut self, id: u64, round: u64, anchor: u64, anchor_span: u64, parents: &[u64]) {
            self.lane(id, PalwExecV2MemberKeyV1::Tx { round, permit_index: 0 }, anchor, anchor_span, parents);
        }
        fn slice(&mut self, id: u64, root: u64, index: u32, anchor: u64, anchor_span: u64, parents: &[u64]) {
            self.lane(id, PalwExecV2MemberKeyV1::Slice { root_claim_id: h(root), slice_index: index }, anchor, anchor_span, parents);
        }
    }

    impl PalwExecV2DagV1 for MapDag {
        fn node(&self, hash: &BlockHash) -> Option<PalwExecV2NodeV1> {
            self.0.get(hash).cloned()
        }
    }

    /// chain 1; a mixed lane 10 (tx) <- 11 (slice) <- 12 (tx) <- 13 (slice) hanging from it.
    fn lane() -> MapDag {
        let mut dag = MapDag::default();
        dag.chain(1);
        dag.tx(10, 100, 1, 5, &[]);
        dag.slice(11, 7, 0, 1, 5, &[10]);
        dag.tx(12, 102, 1, 5, &[11]);
        dag.slice(13, 7, 1, 1, 5, &[12]);
        dag
    }

    fn hashes(closure: &PalwExecV2ClosureV1) -> Vec<Hash64> {
        closure.members.iter().map(|m| m.hash).collect()
    }

    #[test]
    fn the_closure_of_a_head_is_the_whole_lane_back_to_the_chain_with_every_subtype_in_canonical_order() {
        let closure = palw_exec_v2_closure_v1(&lane(), &[h(13)], |_| false, 5, 100).unwrap();
        // Every EXEC_TX first (by round), then every EXEC_SLICE (by root, index).
        assert_eq!(hashes(&closure), vec![h(10), h(12), h(11), h(13)]);
        assert!(closure.dropped.is_empty());
    }

    #[test]
    fn a_slice_is_always_ordered_after_its_predecessor_index_of_the_same_root() {
        let mut dag = MapDag::default();
        dag.chain(1);
        // Built in the opposite order from how they are named; the canonical order is by (root, index).
        dag.slice(20, 9, 2, 1, 5, &[]);
        dag.slice(21, 9, 0, 1, 5, &[20]);
        dag.slice(22, 9, 1, 1, 5, &[21]);
        dag.slice(23, 3, 5, 1, 5, &[22]);
        let closure = palw_exec_v2_closure_v1(&dag, &[h(23)], |_| false, 5, 100).unwrap();
        let order: Vec<(Hash64, u32)> = closure
            .members
            .iter()
            .map(|m| match m.key {
                PalwExecV2MemberKeyV1::Slice { root_claim_id, slice_index } => (root_claim_id, slice_index),
                _ => unreachable!(),
            })
            .collect();
        let mut sorted = order.clone();
        sorted.sort();
        assert_eq!(order, sorted, "slices of one root are in index order");
        let of_root_9: Vec<u32> = order.iter().filter(|(root, _)| *root == h(9)).map(|(_, i)| *i).collect();
        assert_eq!(of_root_9, vec![0, 1, 2]);
    }

    #[test]
    fn the_closure_stops_at_what_the_parent_state_already_anchored() {
        let anchored: BTreeSet<_> = [h(10), h(11)].into_iter().collect();
        let closure = palw_exec_v2_closure_v1(&lane(), &[h(13)], |b| anchored.contains(b), 5, 100).unwrap();
        assert_eq!(hashes(&closure), vec![h(12), h(13)], "an anchored block is covered once, and the walk does not go past it");
    }

    #[test]
    fn a_head_outside_the_window_is_discarded_and_invalidates_nothing() {
        let closure = palw_exec_v2_closure_v1(&lane(), &[h(13)], |_| false, 8, 100).unwrap();
        assert!(closure.members.is_empty());
        assert_eq!(closure.dropped.get(&h(13)), Some(&PalwExecV2RejectReasonV1::OutsideWindow));
        assert_eq!(closure.dropped.len(), 1, "a stale head costs one probe: the walk does not go past it");
    }

    #[test]
    fn a_stale_ancestor_is_a_boundary_so_a_fresh_block_over_it_is_still_covered() {
        let mut dag = lane();
        dag.chain(2);
        dag.slice(14, 7, 2, 2, 7, &[13]);
        let closure = palw_exec_v2_closure_v1(&dag, &[h(14)], |_| false, 8, 100).unwrap();
        assert_eq!(hashes(&closure), vec![h(14)]);
        assert_eq!(closure.dropped.get(&h(13)), Some(&PalwExecV2RejectReasonV1::OutsideWindow));
    }

    #[test]
    fn a_foreign_anchor_and_a_legacy_carriage_are_dropped_with_their_reasons() {
        let mut dag = lane();
        dag.0.get_mut(&h(12)).unwrap().anchor_on_chain = false;
        let closure = palw_exec_v2_closure_v1(&dag, &[h(13)], |_| false, 5, 100).unwrap();
        assert_eq!(hashes(&closure), vec![h(13)]);
        assert_eq!(closure.dropped.get(&h(12)), Some(&PalwExecV2RejectReasonV1::ForeignAnchor));
        let mut dag = lane();
        dag.0.get_mut(&h(11)).unwrap().legacy = true;
        let closure = palw_exec_v2_closure_v1(&dag, &[h(13)], |_| false, 5, 100).unwrap();
        assert_eq!(hashes(&closure), vec![h(12), h(13)]);
        assert_eq!(closure.dropped.get(&h(11)), Some(&PalwExecV2RejectReasonV1::LegacyCarriage), "a v1 block is never anchored");
    }

    #[test]
    fn a_head_that_is_not_a_lane_block_is_refused_and_a_missing_block_is_a_dependency() {
        assert_eq!(
            palw_exec_v2_closure_v1(&lane(), &[h(1)], |_| false, 5, 100).unwrap_err(),
            PalwExecV2AnchorErrorV1::HeadNotLane(h(1))
        );
        assert_eq!(
            palw_exec_v2_closure_v1(&lane(), &[h(99)], |_| false, 5, 100).unwrap_err(),
            PalwExecV2AnchorErrorV1::MissingBlock(h(99))
        );
    }

    #[test]
    fn a_lane_longer_than_the_bound_is_refused_and_the_walk_stops_at_the_bound() {
        let mut dag = MapDag::default();
        dag.chain(1);
        let mut previous = vec![];
        for id in 10..60 {
            dag.slice(id, 4, id as u32, 1, 5, &previous);
            previous = vec![id];
        }
        let err = palw_exec_v2_closure_v1(&dag, &[h(59)], |_| false, 5, 10).unwrap_err();
        assert_eq!(err, PalwExecV2AnchorErrorV1::TooManyLeaves(11, 10));
        assert!(palw_exec_v2_closure_v1(&dag, &[h(59)], |_| false, 5, 50).is_ok());
    }

    #[test]
    fn heads_are_bounded_and_a_diamond_is_covered_once() {
        let mut dag = MapDag::default();
        dag.chain(1);
        dag.tx(10, 100, 1, 5, &[]);
        dag.tx(11, 101, 1, 5, &[10]);
        dag.tx(12, 101, 1, 5, &[10]);
        let closure = palw_exec_v2_closure_v1(&dag, &[h(11), h(12)], |_| false, 5, 100).unwrap();
        assert_eq!(closure.members.len(), 3, "the shared ancestor counts once");
        let too_many: Vec<_> = (0..9).map(h).collect();
        assert_eq!(palw_exec_v2_closure_v1(&dag, &too_many, |_| false, 5, 100).unwrap_err(), PalwExecV2AnchorErrorV1::HeadCount(9));
        assert_eq!(palw_exec_v2_closure_v1(&dag, &[], |_| false, 5, 100).unwrap_err(), PalwExecV2AnchorErrorV1::HeadCount(0));
    }

    #[test]
    fn the_window_is_this_span_and_the_one_before() {
        assert!(palw_exec_v2_window_ok(5, 5));
        assert!(palw_exec_v2_window_ok(4, 5));
        assert!(!palw_exec_v2_window_ok(3, 5));
        assert!(!palw_exec_v2_window_ok(6, 5));
        assert!(!palw_exec_v2_window_ok(u64::MAX, u64::MAX - 1));
        assert!(palw_exec_v2_window_ok(u64::MAX, u64::MAX));
    }

    #[test]
    fn the_root_binds_the_set_its_subtypes_its_coordinates_and_its_size() {
        let tx = |round, index, id| PalwExecV2MemberV1 { key: PalwExecV2MemberKeyV1::Tx { round, permit_index: index }, hash: h(id) };
        let slice = |root, index, id| PalwExecV2MemberV1 {
            key: PalwExecV2MemberKeyV1::Slice { root_claim_id: h(root), slice_index: index },
            hash: h(id),
        };
        let (a, b, c) = (tx(1, 0, 1), slice(5, 0, 2), slice(5, 1, 3));
        let root = palw_exec_v2_root(&[a, b, c]);
        assert_eq!(root, palw_exec_v2_root(&[c, a, b]), "the root is a function of the set, not of the order it was found in");
        assert_ne!(root, palw_exec_v2_root(&[a, b]));
        assert_ne!(root, palw_exec_v2_root(&[a, b, slice(5, 2, 3)]), "the slice index is committed");
        assert_ne!(root, palw_exec_v2_root(&[a, b, slice(6, 1, 3)]), "the root claim is committed");
        assert_ne!(root, palw_exec_v2_root(&[a, b, tx(3, 1, 3)]), "the subtype is committed");
        // The same hash as a permit and as a slice is two different leaves.
        assert_ne!(tx(1, 0, 1).leaf(), slice(1, 0, 1).leaf());
        assert_ne!(palw_exec_v2_root(&[]), Hash64::default(), "the empty set has a root of its own, never a default hash");
        assert_ne!(palw_exec_v2_root(&[]), palw_exec_v2_root(&[a]));
    }

    #[test]
    fn a_trailer_round_trips_behind_the_miners_own_bytes_and_is_not_the_adr_0168_magic() {
        let closure = palw_exec_v2_closure_v1(&lane(), &[h(13)], |_| false, 5, 100).unwrap();
        let anchor = PalwExecV2AnchorV1::of_closure(&[h(13)], &closure).unwrap();
        let extra = palw_exec_v2_anchor_append(b"my-miner/1.0", &anchor).unwrap();
        assert!(extra.len() <= b"my-miner/1.0".len() + PALW_EXEC_V2_ANCHOR_MAX_TRAILER_BYTES);
        let (prefix, decoded) = palw_exec_v2_anchor_split(&extra).unwrap();
        assert_eq!(prefix, b"my-miner/1.0");
        assert_eq!(decoded, Some(anchor.clone()));
        palw_exec_v2_anchor_verify(&anchor, &closure).unwrap();
        let again = palw_exec_v2_anchor_append(&extra, &anchor).unwrap();
        assert_eq!(again, extra, "appending twice replaces; it never stacks");
        assert_ne!(PALW_EXEC_V2_ANCHOR_MAGIC, *b"PXA1");
        // A v1-magic tail is the miner's bytes, not an anchor.
        let mut v1_tail = b"x".to_vec();
        v1_tail.extend_from_slice(b"PXA1");
        assert_eq!(palw_exec_v2_anchor_split(&v1_tail).unwrap(), (&v1_tail[..], None));
    }

    #[test]
    fn extra_data_without_the_magic_is_the_miners_and_returned_whole() {
        for extra in [&b""[..], b"x", b"PXA", b"PXA2x", &[0u8; 100][..]] {
            assert_eq!(palw_exec_v2_anchor_split(extra).unwrap(), (extra, None));
        }
    }

    #[test]
    fn every_malformed_trailer_is_refused_by_name_and_none_panics() {
        let closure = palw_exec_v2_closure_v1(&lane(), &[h(13)], |_| false, 5, 100).unwrap();
        let good = PalwExecV2AnchorV1::of_closure(&[h(13)], &closure).unwrap().trailer();
        for cut in 0..good.len() {
            let _ = palw_exec_v2_anchor_split(&good[cut..]);
            let _ = palw_exec_v2_anchor_split(&good[..cut]);
        }
        for at in 0..good.len() {
            let mut bad = good.clone();
            bad[at] ^= 0xFF;
            let _ = palw_exec_v2_anchor_split(&bad);
        }
        assert!(palw_exec_v2_anchor_split(&PALW_EXEC_V2_ANCHOR_MAGIC).is_err());
        assert!(palw_exec_v2_anchor_split(&[0xFF, 0xFF, b'P', b'X', b'A', b'2']).is_err());
        let mut wrong_version = good.clone();
        wrong_version[good.len() - 4 - 2 - (PALW_EXEC_V2_ANCHOR_BODY_FIXED + 64)] = 9;
        assert_eq!(palw_exec_v2_anchor_split(&wrong_version).unwrap_err(), PalwExecV2AnchorErrorV1::Version(9));
        let none = PalwExecV2AnchorV1 { heads: vec![], count: 0, root: h(1) };
        assert_eq!(none.validate_shape().unwrap_err(), PalwExecV2AnchorErrorV1::HeadCount(0));
        let unsorted = PalwExecV2AnchorV1 { heads: vec![h(2), h(1)], count: 0, root: h(1) };
        assert_eq!(unsorted.validate_shape().unwrap_err(), PalwExecV2AnchorErrorV1::HeadsNotCanonical);
        let dup = PalwExecV2AnchorV1 { heads: vec![h(1), h(1)], count: 0, root: h(1) };
        assert_eq!(dup.validate_shape().unwrap_err(), PalwExecV2AnchorErrorV1::HeadsNotCanonical);
    }

    #[test]
    fn a_trailer_that_lies_about_its_count_or_its_root_fails_verification() {
        let closure = palw_exec_v2_closure_v1(&lane(), &[h(13)], |_| false, 5, 100).unwrap();
        let mut anchor = PalwExecV2AnchorV1::of_closure(&[h(13)], &closure).unwrap();
        anchor.count += 1;
        assert_eq!(
            palw_exec_v2_anchor_verify(&anchor, &closure).unwrap_err(),
            PalwExecV2AnchorErrorV1::CountMismatch { declared: 5, actual: 4 }
        );
        anchor.count -= 1;
        anchor.root = h(7);
        assert_eq!(palw_exec_v2_anchor_verify(&anchor, &closure).unwrap_err(), PalwExecV2AnchorErrorV1::RootMismatch);
    }

    #[test]
    fn the_domains_are_distinct_from_each_other_and_from_the_wire_layer() {
        let mut all: Vec<&[u8]> = PALW_EXEC_V2_ANCHOR_ALL_DOMAINS.to_vec();
        all.extend(crate::palw_exec_v2::PALW_EXEC_V2_ALL_DOMAINS.iter().copied());
        let unique: BTreeSet<&[u8]> = all.iter().copied().collect();
        assert_eq!(unique.len(), all.len());
    }
}
