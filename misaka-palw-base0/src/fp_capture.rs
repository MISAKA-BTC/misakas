//! ADR-0077 Decision 8 / Decision 13, producer side: the free-prompt capture retained SPARSELY —
//! the step leg's leaf hashes folded as they are produced, the tree kept only above a fixed
//! level, every tile re-derived by replay from the checkpoint chunks when an opening is asked for.
//!
//! # What was measured, and why a dense capture cannot reach the ladder
//!
//! A capture today holds every tile of every node of every position, in memory and then on disk:
//! roughly 50 MB per position for the A16 tier, ~25 GB at 512 positions and simply impossible at
//! the 8,192 ADR-0077 Decision 13 plans. Even the LEAF HASHES alone are 64 bytes each, so a
//! `PALW_STEP_LEG_MAX_LEAVES`-sized row is 256 MB of hashes and the `2^32` ladder Decision 12
//! moves to is 256 GB. Neither the tiles nor the whole leaf vector can be the thing an executor
//! keeps.
//!
//! What it CAN keep is the Merkle tree above a fixed level. [`Base0SparseStepAccumulatorV1`] folds
//! each leaf hash the moment the capture produces it, retains the nodes at
//! [`PALW_BASE0_SPARSE_RETAIN_LEVEL_V1`], and throws every leaf away. At retain level 12 that is
//! one 64-byte node per 4,096 leaves — 64 MiB at the `2^32` ladder's top, and 64 KiB at today's
//! `2^22` cap. Everything below the retained level is RE-DERIVED by replay from the checkpoint
//! chunks when an opening is asked for (`fp_interval`), which is the whole point of the checkpoint
//! leg existing.
//!
//! # The one property this module owes
//!
//! **The accumulator's root is `step_merkle_root_v1`'s root, for every leaf count.** The step tree
//! is a promote-odd tree — an odd tail at a level is carried up unchanged rather than paired with
//! itself — and a fold that got that wrong would produce a root the court recomputes differently:
//! an honest producer, unconvictable and unpayable. So the equality is pinned by a test over many
//! leaf counts including odd ones and every retained level, and the same test covers the sibling
//! derivation against `step_merkle_range_siblings_v1`.
//!
//! # Why the two primitives are CALLED here, and were once spelled again
//!
//! A streaming fold cannot be expressed through the public whole-vector functions —
//! `step_merkle_root_v1` re-indexes its leaves from zero, so it cannot value a subtree that starts
//! anywhere else — and `step_merkle_leaf` and the node hash were private to `palw_step_leg`. So
//! this module restated them against the public DOMAIN constants, with the equality test above as
//! the thing that kept the two spellings one rule.
//!
//! That is a test standing in for a definition. The leg now exports
//! [`kaspa_consensus_core::palw_step_leg::step_merkle_leaf_v1`] and
//! [`kaspa_consensus_core::palw_step_leg::step_merkle_node_v1`], this module calls them, and there
//! is one spelling of each rule in the tree. The equality test stays, because it now checks the
//! FOLD (promote-odd, block boundaries) rather than a re-derivation of two hashes.

use kaspa_consensus_core::palw_step_leg::{
    PALW_STEP_LEG_MAX_LEAVES, PALW_STEP_LEG_MAX_OPENING_SIBLINGS, PalwStepRangeOpeningV1, step_merkle_leaf_v1, step_merkle_node_v1,
};
use kaspa_hashes::Hash64;

/// **The level the tree is kept above** (ADR-0077 Decision 8). One retained node per `2^12`
/// leaves: 64 KiB of retained nodes at today's `PALW_STEP_MAX_LEAVES = 2^22`, 64 MiB at the `2^32`
/// Decision 12 moves to. It is also the width of the replay span an opening forces (below), and
/// 4,096 leaves is a small fraction of ONE position of either model tier (~99 k leaves for the
/// dense tier, ~298 k for the hybrid) — so re-deriving a boundary subtree never costs more than
/// the calls the opening was already going to replay.
pub const PALW_BASE0_SPARSE_RETAIN_LEVEL_V1: u32 = 12;

/// The deepest level this module will retain at. A level above the tree's own height is not an
/// error (the retained vector is then a single node — the root), but a level past this is a
/// request to buffer `2^32` leaf hashes, which is the thing the module exists to avoid.
pub const PALW_BASE0_SPARSE_MAX_RETAIN_LEVEL_V1: u32 = 24;

/// Why a sparse fold, or an opening served from one, is refused.
///
/// A plain enum with a hand-written `Display`, which is this crate's idiom (`LegError`,
/// `ProduceError`, `ArtifactError`) and not an accident: `misaka-palw-base0` carries no
/// `thiserror` dependency, and adding one for two error types would put a proc-macro crate into
/// the build of every consumer of the execution family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Base0SparseCaptureError {
    LeafCountOutOfRange {
        got: u64,
        max: u64,
    },
    RetainLevelOutOfRange {
        got: u32,
        max: u32,
    },
    CaptureIncomplete {
        pushed: u64,
        expected: u64,
    },
    CaptureOverrun {
        got: u64,
        expected: u64,
    },
    RangeOutOfLeafSpace {
        first: u64,
        count: u64,
        leaf_count: u64,
    },
    SpanDoesNotCoverTheRange {
        index: u64,
        span_first: u64,
        span_end: u64,
    },
    SpanNotAligned {
        span_first: u64,
        span_end: u64,
    },
    OpeningTooDeep {
        got: usize,
        max: usize,
    },
    /// A deserialized tree whose retained vector is not the width its own `(leaf_count,
    /// retain_level)` implies. It describes no tree, so it has no root — and saying so is the
    /// difference between a refusal and an index past the end of a vector.
    TreeIsNotItsOwnShape {
        retained: usize,
        expected: u64,
    },
}

impl std::fmt::Display for Base0SparseCaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LeafCountOutOfRange { got, max } => write!(f, "a step tree of {got} leaves is outside the leg's range (max {max})"),
            Self::RetainLevelOutOfRange { got, max } => write!(f, "retain level {got} is past the sparse cap of {max}"),
            Self::CaptureIncomplete { pushed, expected } => {
                write!(f, "the capture folded {pushed} leaves and the step space has {expected}")
            }
            Self::CaptureOverrun { got, expected } => {
                write!(f, "the capture pushed a {got}th leaf into a step space of {expected}")
            }
            Self::RangeOutOfLeafSpace { first, count, leaf_count } => {
                write!(f, "the range [{first}, {first}+{count}) is not inside a step space of {leaf_count} leaves")
            }
            Self::SpanDoesNotCoverTheRange { index, span_first, span_end } => {
                write!(f, "the opening needs leaf {index}, which the replayed span [{span_first}, {span_end}) does not cover")
            }
            Self::SpanNotAligned { span_first, span_end } => {
                write!(f, "the span [{span_first}, {span_end}) is not aligned to the retained level, so its nodes are not the tree's")
            }
            Self::OpeningTooDeep { got, max } => {
                write!(f, "the range opening would carry {got} siblings, past the leg's cap of {max}")
            }
            Self::TreeIsNotItsOwnShape { retained, expected } => {
                write!(f, "the retained vector holds {retained} nodes and this tree's shape implies {expected}")
            }
        }
    }
}

impl std::error::Error for Base0SparseCaptureError {}

// **The step tree's leaf and node rules are `palw_step_leg`'s, called rather than restated.**
//
// They used to be restated here — a `merkle_leaf_v1`, a `merkle_node_v1` and a `keyed64` that
// reproduced the leg's private ones — and a restatement of a consensus hash is a second spelling
// of it: the two agreed only because `the_sparse_accumulator_is_the_step_tree` compared their
// roots, and a root the court recomputes differently is an honest producer who can neither be
// convicted nor paid. `step_merkle_leaf_v1` / `step_merkle_node_v1` are the leg's own, exported
// for exactly this caller.

/// The promote-odd fold of one level's nodes into one — the shape `step_merkle_root_v1` walks,
/// applied to a block whose left edge is even at every level it folds through (which is what makes
/// the local pairing the global pairing; see [`Base0SparseStepAccumulatorV1`]).
fn fold_block_v1(mut level: Vec<Hash64>) -> Option<Hash64> {
    if level.is_empty() {
        return None;
    }
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut chunks = level.chunks_exact(2);
        for pair in &mut chunks {
            next.push(step_merkle_node_v1(&pair[0], &pair[1]));
        }
        if let [odd] = chunks.remainder() {
            next.push(*odd);
        }
        level = next;
    }
    Some(level[0])
}

/// **The capture's fold: leaf hashes in, one retained node per `2^retain_level` of them.**
///
/// Fed in canonical leaf order, exactly once per leaf, by the family's capture loop. It holds at
/// most `2^retain_level` leaf hashes at a time (the block being folded) plus the retained vector,
/// and it refuses a short capture the way [`crate::legs::Base0StepCaptureV1::finish`] does: a
/// commitment over a partial space says "computed zero" about every leaf nobody filled, and an
/// executor must never be the one that emits that object.
///
/// # Why a block fold rather than a per-level carry
///
/// The node at `(level, p)` of the step tree is the promote-odd fold of the leaves it covers,
/// `[p·2^level, min((p+1)·2^level, n))`, because pairing at every level is local and the block's
/// left edge `p·2^(level−j)` is even for every `j < level`. The only place promotion can occur
/// inside a block is the rightmost one, where "the block's local width is odd" and "the global
/// level width is odd, and this is its last node" are the same statement — again because the left
/// edge is even. So folding whole blocks is not an approximation of the tree; it IS the tree, and
/// the test says so over odd counts at every level.
pub struct Base0SparseStepAccumulatorV1 {
    leaf_count: u64,
    retain_level: u32,
    block: Vec<Hash64>,
    retained: Vec<Hash64>,
    pushed: u64,
}

impl Base0SparseStepAccumulatorV1 {
    pub fn new(leaf_count: u64, retain_level: u32) -> Result<Self, Base0SparseCaptureError> {
        if leaf_count == 0 || leaf_count > PALW_STEP_LEG_MAX_LEAVES {
            return Err(Base0SparseCaptureError::LeafCountOutOfRange { got: leaf_count, max: PALW_STEP_LEG_MAX_LEAVES });
        }
        if retain_level > PALW_BASE0_SPARSE_MAX_RETAIN_LEVEL_V1 {
            return Err(Base0SparseCaptureError::RetainLevelOutOfRange {
                got: retain_level,
                max: PALW_BASE0_SPARSE_MAX_RETAIN_LEVEL_V1,
            });
        }
        let block_leaves = 1usize << retain_level;
        Ok(Self {
            leaf_count,
            retain_level,
            block: Vec::with_capacity(block_leaves.min(leaf_count as usize)),
            retained: Vec::with_capacity(((leaf_count as usize).div_ceil(block_leaves)).min(1 << 20)),
            pushed: 0,
        })
    }

    /// Fold one leaf hash. Order is the caller's obligation and the leaf index is derived from the
    /// count so far — a caller that pushed out of order would be committing a different tree, and
    /// the family's capture loops walk the canonical enumeration by construction.
    pub fn push(&mut self, leaf_hash: Hash64) -> Result<(), Base0SparseCaptureError> {
        if self.pushed >= self.leaf_count {
            return Err(Base0SparseCaptureError::CaptureOverrun { got: self.pushed + 1, expected: self.leaf_count });
        }
        self.block.push(step_merkle_leaf_v1(self.pushed, &leaf_hash));
        self.pushed += 1;
        if self.block.len() == 1usize << self.retain_level {
            let block = std::mem::take(&mut self.block);
            self.retained.push(fold_block_v1(block).expect("a full block is not empty"));
        }
        Ok(())
    }

    /// How much of the step space this fold has seen — the sparse twin of
    /// [`crate::legs::Base0StepCaptureV1::progress`].
    pub fn progress(&self) -> (u64, u64) {
        (self.pushed, self.leaf_count)
    }

    /// Seal the fold. A short capture is refused, for the reason `Base0StepCaptureV1` refuses one.
    pub fn finish(mut self) -> Result<Base0SparseStepTreeV1, Base0SparseCaptureError> {
        if self.pushed != self.leaf_count {
            return Err(Base0SparseCaptureError::CaptureIncomplete { pushed: self.pushed, expected: self.leaf_count });
        }
        if !self.block.is_empty() {
            let block = std::mem::take(&mut self.block);
            self.retained.push(fold_block_v1(block).expect("a non-empty block"));
        }
        Ok(Base0SparseStepTreeV1 { leaf_count: self.leaf_count, retain_level: self.retain_level, retained: self.retained })
    }
}

/// **What the executor keeps of the step tree**: the nodes at the retained level, in order.
///
/// Everything at or above the retained level is a pure function of this vector; everything below
/// is re-derived by replay. So an opening is served from (this ‖ a replayed span), and the capture
/// itself never has to be held whole — R1's retention half.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Base0SparseStepTreeV1 {
    leaf_count: u64,
    retain_level: u32,
    retained: Vec<Hash64>,
}

impl Base0SparseStepTreeV1 {
    pub fn leaf_count(&self) -> u64 {
        self.leaf_count
    }
    pub fn retain_level(&self) -> u32 {
        self.retain_level
    }
    /// The retained nodes — what an executor writes to disk instead of the capture.
    pub fn retained_nodes(&self) -> &[Hash64] {
        &self.retained
    }

    /// Build the sparse tree from a DENSE leaf vector. The producer's own path folds as it
    /// captures ([`Base0SparseStepAccumulatorV1`]); this is for the callers that already hold the
    /// leaves — the retention file as it is written today, and the tests — and it goes through the
    /// same accumulator so the two cannot be two rules.
    pub fn from_leaves_v1(leaves: &[Hash64], retain_level: u32) -> Result<Self, Base0SparseCaptureError> {
        let mut acc = Base0SparseStepAccumulatorV1::new(leaves.len() as u64, retain_level)?;
        for leaf in leaves {
            acc.push(*leaf)?;
        }
        acc.finish()
    }

    /// **This tree is the shape its own fields describe.**
    ///
    /// The three fields are private and the accumulator only ever produces a consistent triple —
    /// but the type is `BorshDeserialize`, so a retention file (or anything else that hands over
    /// bytes) can produce one that is not. Every derivation below indexes the retained vector at
    /// positions `(leaf_count, retain_level)` implies, so an inconsistent triple is not a wrong
    /// answer, it is an index past the end. Checked once, by name, and every derivation goes
    /// through it.
    pub fn validate_v1(&self) -> Result<(), Base0SparseCaptureError> {
        if self.leaf_count == 0 || self.leaf_count > PALW_STEP_LEG_MAX_LEAVES {
            return Err(Base0SparseCaptureError::LeafCountOutOfRange { got: self.leaf_count, max: PALW_STEP_LEG_MAX_LEAVES });
        }
        if self.retain_level > PALW_BASE0_SPARSE_MAX_RETAIN_LEVEL_V1 {
            return Err(Base0SparseCaptureError::RetainLevelOutOfRange {
                got: self.retain_level,
                max: PALW_BASE0_SPARSE_MAX_RETAIN_LEVEL_V1,
            });
        }
        let expected = level_width(self.leaf_count, self.retain_level);
        if self.retained.len() as u64 != expected {
            return Err(Base0SparseCaptureError::TreeIsNotItsOwnShape { retained: self.retained.len(), expected });
        }
        Ok(())
    }

    /// The step leg's Merkle root — byte-identical to `step_merkle_root_v1` over the same leaves.
    ///
    /// `Err` only for a tree that is not its own shape ([`Self::validate_v1`]); one this crate
    /// folded is always its own shape, which is why the accumulator's callers can treat this as
    /// total.
    pub fn root(&self) -> Result<Hash64, Base0SparseCaptureError> {
        self.validate_v1()?;
        let mut level = self.retained.clone();
        let mut width = level_width(self.leaf_count, self.retain_level);
        while width > 1 {
            let mut next = Vec::with_capacity(level.len().div_ceil(2));
            let mut chunks = level.chunks_exact(2);
            for pair in &mut chunks {
                next.push(step_merkle_node_v1(&pair[0], &pair[1]));
            }
            if let [odd] = chunks.remainder() {
                next.push(*odd);
            }
            level = next;
            width = width.div_ceil(2);
        }
        level.first().copied().ok_or(Base0SparseCaptureError::TreeIsNotItsOwnShape { retained: 0, expected: 1 })
    }

    /// **The leaves an opening of `[first, first + count)` forces the executor to re-derive.**
    ///
    /// A sibling below the retained level is always inside the SAME retained-level subtree as the
    /// range edge it hangs off — its parent is at a level no deeper than the retained one — so
    /// re-deriving the two boundary subtrees whole is exactly enough, and re-deriving anything
    /// wider would be replaying calls no opening reads. The span is therefore the range rounded
    /// out to retained-level boundaries, clamped to the step space.
    pub fn span_for_range(&self, first: u64, count: u64) -> Result<(u64, u64), Base0SparseCaptureError> {
        self.validate_v1()?;
        if count == 0 || first.checked_add(count).is_none_or(|end| end > self.leaf_count) {
            return Err(Base0SparseCaptureError::RangeOutOfLeafSpace { first, count, leaf_count: self.leaf_count });
        }
        let block = 1u64 << self.retain_level;
        let span_first = (first / block) * block;
        let span_end = (((first + count - 1) / block) + 1).saturating_mul(block).min(self.leaf_count);
        Ok((span_first, span_end))
    }

    /// **The sibling set `step_range_opening_root_v1` consumes, served from the sparse tree.**
    ///
    /// `span_leaves` are the LEAF HASHES of `[span_first, span_first + span_leaves.len())` — what
    /// a replay re-derived — and must be the span [`Self::span_for_range`] named. Levels below the
    /// retained one are folded out of those; the retained level is read from the kept vector; the
    /// levels above are folded from it. The walk mirrors `step_merkle_range_siblings_v1` exactly,
    /// including the promote rule and the left-then-right ordering within a level: a sibling set in
    /// any other order is an opening nothing verifies.
    pub fn range_siblings_v1(
        &self,
        span_first: u64,
        span_leaves: &[Hash64],
        first: u64,
        count: u64,
    ) -> Result<Vec<Hash64>, Base0SparseCaptureError> {
        let (want_first, want_end) = self.span_for_range(first, count)?;
        let span_end = span_first + span_leaves.len() as u64;
        if span_first != want_first || span_end != want_end {
            return Err(Base0SparseCaptureError::SpanNotAligned { span_first, span_end });
        }
        // Levels 0..=retain_level inside the span, and the retained level and above from the kept
        // vector. Two towers, meeting at the retained level, which is what "kept only above a
        // fixed level" means in code.
        let lower = self.span_levels(span_first, span_leaves);
        let upper = self.upper_levels();

        let mut out = Vec::new();
        let (mut a, mut b) = (first, first + count);
        let mut level = 0u32;
        let mut width = self.leaf_count;
        while width > 1 {
            if !a.is_multiple_of(2) {
                out.push(self.node_at(&lower, &upper, span_first, level, a - 1)?);
            }
            if !b.is_multiple_of(2) && (b != width || width.is_multiple_of(2)) {
                out.push(self.node_at(&lower, &upper, span_first, level, b)?);
            }
            a /= 2;
            b = b.div_ceil(2);
            width = width.div_ceil(2);
            level += 1;
        }
        if out.len() > 2 * PALW_STEP_LEG_MAX_OPENING_SIBLINGS {
            return Err(Base0SparseCaptureError::OpeningTooDeep { got: out.len(), max: 2 * PALW_STEP_LEG_MAX_OPENING_SIBLINGS });
        }
        Ok(out)
    }

    /// The whole range opening — leaf hashes and siblings — ready for
    /// `step_range_opening_root_v1`.
    pub fn range_opening_v1(
        &self,
        span_first: u64,
        span_leaves: &[Hash64],
        first: u64,
        count: u64,
    ) -> Result<PalwStepRangeOpeningV1, Base0SparseCaptureError> {
        let siblings = self.range_siblings_v1(span_first, span_leaves, first, count)?;
        let start = (first - span_first) as usize;
        let end = start + count as usize;
        let leaf_hashes = span_leaves
            .get(start..end)
            .ok_or(Base0SparseCaptureError::SpanDoesNotCoverTheRange {
                index: first + count - 1,
                span_first,
                span_end: span_first + span_leaves.len() as u64,
            })?
            .to_vec();
        Ok(PalwStepRangeOpeningV1 { first_leaf_index: first, leaf_hashes, siblings })
    }

    /// Levels `0..=retain_level` of the span, each as (nodes, first global position).
    fn span_levels(&self, span_first: u64, span_leaves: &[Hash64]) -> Vec<Vec<Hash64>> {
        let mut levels: Vec<Vec<Hash64>> = Vec::with_capacity(self.retain_level as usize + 1);
        levels.push(span_leaves.iter().enumerate().map(|(i, h)| step_merkle_leaf_v1(span_first + i as u64, h)).collect());
        for level in 0..self.retain_level {
            let width = level_width(self.leaf_count, level);
            let start = span_first >> level;
            let current = levels.last().expect("seeded");
            let mut next = Vec::with_capacity(current.len().div_ceil(2));
            let mut i = 0usize;
            while i + 1 < current.len() {
                next.push(step_merkle_node_v1(&current[i], &current[i + 1]));
                i += 2;
            }
            if i < current.len() {
                // **A lone tail inside the span can only be the level's global last node**, so the
                // promote rule applies verbatim and the node is carried up unchanged.
                //
                // Why that holds: the span's left edge is a multiple of `2^retain_level`, so at
                // every level `l < retain_level` its first node sits at an EVEN global position and
                // the span's local pairing is the global pairing. A span that is a whole block has
                // `2^(retain_level − l)` nodes at level `l` — even — so an odd remainder can only
                // occur in the last, partial block, whose right edge is the leaf space's own. The
                // debug assertion states it where it would fail rather than in prose.
                debug_assert!(
                    !width.is_multiple_of(2) && start + i as u64 == width - 1,
                    "an aligned span promotes only at the level's global tail"
                );
                next.push(current[i]);
            }
            levels.push(next);
        }
        levels
    }

    /// Levels `retain_level..` folded from the kept vector; `[0]` is the retained level itself.
    fn upper_levels(&self) -> Vec<Vec<Hash64>> {
        let mut levels = vec![self.retained.clone()];
        let mut width = level_width(self.leaf_count, self.retain_level);
        while width > 1 {
            let current = levels.last().expect("seeded");
            let mut next = Vec::with_capacity(current.len().div_ceil(2));
            let mut chunks = current.chunks_exact(2);
            for pair in &mut chunks {
                next.push(step_merkle_node_v1(&pair[0], &pair[1]));
            }
            if let [odd] = chunks.remainder() {
                next.push(*odd);
            }
            levels.push(next);
            width = width.div_ceil(2);
        }
        levels
    }

    fn node_at(
        &self,
        lower: &[Vec<Hash64>],
        upper: &[Vec<Hash64>],
        span_first: u64,
        level: u32,
        position: u64,
    ) -> Result<Hash64, Base0SparseCaptureError> {
        if level >= self.retain_level {
            let tower = (level - self.retain_level) as usize;
            return upper.get(tower).and_then(|nodes| nodes.get(position as usize)).copied().ok_or(
                Base0SparseCaptureError::SpanDoesNotCoverTheRange { index: position << level, span_first, span_end: span_first },
            );
        }
        let start = span_first >> level;
        let nodes = &lower[level as usize];
        position.checked_sub(start).and_then(|offset| nodes.get(offset as usize)).copied().ok_or(
            Base0SparseCaptureError::SpanDoesNotCoverTheRange {
                index: position << level,
                span_first,
                span_end: span_first + (nodes.len() as u64) * (1u64 << level),
            },
        )
    }
}

/// `ceil(leaf_count / 2^level)` — the promote-odd tree's width at `level`, which is what decides
/// both pairing and promotion. Derived rather than tracked: a width carried alongside the fold
/// would be a second source for a number the leaf count already fixes.
fn level_width(leaf_count: u64, level: u32) -> u64 {
    let mut width = leaf_count;
    for _ in 0..level {
        width = width.div_ceil(2);
        if width <= 1 {
            return width.max(1);
        }
    }
    width.max(1)
}

// =============================================================================================
// ADR-0077 Decision 10 — the recurrence's replay state, chunked
// =============================================================================================

/// **The state a GatedDeltaNet layer carries between positions, as a chunk map** (ADR-0077
/// Decision 10, executor half).
///
/// # Why this exists, stated as the cost it removes
///
/// The attention layers of every class in this family have a committed, resumable history: the KV
/// cache under `integer_kv_state_chunk_map_id_v1`, opened by `KvCache::from_state_chunks` and
/// walked by `base0_replay_from_checkpoint_v1`. The RECURRENCE has none. The hybrid class
/// registers the checkpoint sentinel (`state_chunk_map_id == Hash64::default()`), so
/// `gdn_core_genesis_replay` walks EVERY prior position for every disputed step, `positions =
/// n_ctx` enters `derive_court_cost_v1` linearly, and that term is one of the three that hold the
/// class at eight tokens (ADR-0077 §1). A recurrence whose state cannot be committed is a
/// recurrence whose history cannot be skipped, at any context length.
///
/// # What one row is, and why the conv window is in it
///
/// A GatedDeltaNet head's state is `Qwen36GdnStateV1`: `d_v` rows of `d_k` `i32` values, row-major
/// (`q36_gdn_step` walks it as `s.chunks_exact(d_k)`). That is the whole recurrent state of the
/// delta rule — but it is NOT the whole state of the layer: the short causal convolution in front
/// of it holds the last `conv_kernel` rows of the concatenated q/k/v channels, and a replay that
/// restored the delta state and not the convolution would compute the first `conv_kernel − 1`
/// positions after the anchor from zeros. So the map covers both kinds, and the enumeration says
/// which is which by name rather than by position arithmetic a reader has to reconstruct.
///
/// # Not registered, and superseded for anything that wants to be
///
/// Registering it moves `state_chunk_map_id`, which is inside the checkpoint profile, which is
/// inside the class id — a re-mint, and a consensus move this workstream does not make. It ships
/// as the executor's machinery with its layout string frozen and its round trip and its anchored
/// equivalence pinned, the way ADR-0078's hermetic runner shipped unregistered.
///
/// **And a class that wants a context registers [`PALW_GDN_STATE_CHUNK_MAP_NAME_V2`] instead.**
/// The conv row above spans every head, so a court opening one head's window pays for all of them
/// — 196,608 bytes of an 81,920-byte close carrier on Qwen3.6's geometry. v2 is the same bytes,
/// keyed by head. v1 stays byte-identical for anything that already named it.
///
/// **A RE-EXPORT, not a copy.** The spelling lives in `kaspa_consensus_core::palw_state_chunk_map`
/// — the lower crate, the one the court reads — and this crate names it so an executor and an
/// adjudicator cannot come to hold two strings. They did: the same descriptor was written out
/// twice, here and there, and two spellings of a map name is two ids, which is a class whose
/// capture and whose court disagree about their map and therefore a class no dispute can open.
pub use kaspa_consensus_core::palw_state_chunk_map::PALW_GDN_STATE_CHUNK_MAP_NAME_V1;
/// The head-sliced enumeration of the same state — see
/// [`kaspa_consensus_core::palw_state_chunk_map::PALW_GDN_STATE_CHUNK_MAP_NAME_V2`]. Re-exported
/// under the same rule.
pub use kaspa_consensus_core::palw_state_chunk_map::PALW_GDN_STATE_CHUNK_MAP_NAME_V2;

/// `state_chunk_map_id` for the recurrence layout — the value a class that registers it declares.
///
/// The consensus crate's function, called: the id is `H(name)` and both halves of that must come
/// from one place, or a respelling on either side mints a second id in silence.
pub fn base0_gdn_state_chunk_map_id_v1() -> Hash64 {
    kaspa_consensus_core::palw_state_chunk_map::gdn_state_chunk_map_id_v1()
}

/// `state_chunk_map_id` for the head-sliced recurrence layout (gdn v2).
pub fn base0_gdn_state_chunk_map_id_v2() -> Hash64 {
    kaspa_consensus_core::palw_state_chunk_map::gdn_state_chunk_map_id_v2()
}

/// **The map id a class of THIS shape registers** — the composition when it has attention layers,
/// the recurrence's own when it does not.
///
/// A hybrid holds both kinds of state, so registering the recurrence map alone would leave its
/// attention anchors with no geometry a court can read — `Unadjudicable` on honest material — and
/// registering the cache map alone would leave the recurrence at its genesis-anchored replay. The
/// executor is the side that files the profile, so the choice is derived here from the graph
/// rather than passed in: a caller that reached for `base0_gdn_state_chunk_map_id_v2()` on a
/// Qwen3.6-shaped class would register half a map.
pub fn base0_class_state_chunk_map_id_v2(profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3) -> Hash64 {
    if profile.full_attention_interval == 0 {
        kaspa_consensus_core::palw_state_chunk_map::gdn_state_chunk_map_id_v2()
    } else {
        kaspa_consensus_core::palw_state_chunk_map::hybrid_state_chunk_map_id_v2()
    }
}

/// [`base0_class_state_chunk_map_id_v2`] on the row-major recurrence enumeration (gdn v1).
pub fn base0_class_state_chunk_map_id_v1(profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3) -> Hash64 {
    if profile.full_attention_interval == 0 {
        kaspa_consensus_core::palw_state_chunk_map::gdn_state_chunk_map_id_v1()
    } else {
        kaspa_consensus_core::palw_state_chunk_map::hybrid_state_chunk_map_id_v1()
    }
}

/// Which half of the recurrence state a chunk belongs to. The discriminants ARE the enumeration
/// order, exactly as `PalwStateChunkKindV1` does it for the KV map.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Base0GdnChunkKindV1 {
    /// One block of rows of one head's `d_v × d_k` delta state.
    Delta = 0,
    /// One block of rows of one layer's convolution window.
    Conv = 1,
}

/// Why a recurrence state cannot be chunked, or restored from chunks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Base0GdnStateError {
    ZeroGeometry { heads: u32, k_dim: u32, v_dim: u32 },
    RowExceedsChunk { row_bytes: u64, max: usize },
    NoRecurrenceLayers,
    TooManyChunks { got: u64, max: usize },
    ChunkCountIsNotTheMaps { got: usize, want: u64 },
    ChunkIsNotItsOwnLength { index: u64, got: usize, want: u64 },
    StateIsNotTheGeometrys { layer: u16, head: u32 },
    ConvIsNotTheGeometrys { layer: u16 },
}

impl std::fmt::Display for Base0GdnStateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroGeometry { heads, k_dim, v_dim } => {
                write!(f, "a recurrence of {heads} heads at {k_dim}×{v_dim} has no state to chunk")
            }
            Self::RowExceedsChunk { row_bytes, max } => {
                write!(f, "one row is {row_bytes} bytes and a state chunk holds at most {max}")
            }
            Self::NoRecurrenceLayers => write!(f, "the profile declares no recurrence layer, so it has no recurrence state"),
            Self::TooManyChunks { got, max } => write!(f, "the layout needs {got} chunks and the leg admits at most {max}"),
            Self::ChunkCountIsNotTheMaps { got, want } => write!(f, "{got} chunks were served and the map has {want}"),
            Self::ChunkIsNotItsOwnLength { index, got, want } => {
                write!(f, "chunk {index} is {got} bytes and its entry declares {want}")
            }
            Self::StateIsNotTheGeometrys { layer, head } => {
                write!(f, "layer {layer} head {head} holds a state of a shape this geometry does not describe")
            }
            Self::ConvIsNotTheGeometrys { layer } => {
                write!(f, "layer {layer} holds a convolution window this geometry does not describe")
            }
        }
    }
}

impl std::error::Error for Base0GdnStateError {}

/// The derived layout of one job's recurrence state. Every field is a function of the class's
/// declared geometry; none is a choice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Base0GdnStateGeometryV1 {
    /// The recurrence layers, ascending — in the PROFILE's numbering, because a court reading an
    /// entry is about to index a layer table.
    pub layers: Vec<u16>,
    pub heads: u32,
    pub k_dim: u32,
    pub v_dim: u32,
    pub conv_kernel: u32,
    /// `(2 · k_dim + v_dim) · heads` — the concatenated q/k/v channels the convolution runs over.
    pub conv_width: u32,
    /// `k_dim × 4`: one row of one head's delta state.
    pub delta_row_bytes: u32,
    /// `conv_width × 4`: one row of the convolution window.
    pub conv_row_bytes: u32,
    pub delta_rows_per_chunk: u32,
    pub conv_rows_per_chunk: u32,
}

impl Base0GdnStateGeometryV1 {
    pub fn delta_chunks_per_head(&self) -> u32 {
        self.v_dim.div_ceil(self.delta_rows_per_chunk)
    }
    pub fn conv_chunks_per_layer(&self) -> u32 {
        self.conv_kernel.div_ceil(self.conv_rows_per_chunk)
    }
    /// Every chunk in the map: the delta half, then the conv half.
    pub fn chunk_count(&self) -> u64 {
        let layers = self.layers.len() as u64;
        layers * self.heads as u64 * self.delta_chunks_per_head() as u64 + layers * self.conv_chunks_per_layer() as u64
    }
    pub fn total_bytes(&self) -> u64 {
        let layers = self.layers.len() as u64;
        layers * self.heads as u64 * self.v_dim as u64 * self.delta_row_bytes as u64
            + layers * self.conv_kernel as u64 * self.conv_row_bytes as u64
    }
}

/// One chunk of the recurrence map: a run of rows of one head's delta state, or of one layer's
/// convolution window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Base0GdnChunkEntryV1 {
    pub kind: Base0GdnChunkKindV1,
    pub layer: u16,
    /// Meaningless for `Conv`, which is per layer.
    pub head: u32,
    pub row_start: u32,
    pub row_count: u32,
    pub row_bytes: u32,
}

impl Base0GdnChunkEntryV1 {
    pub fn byte_len(&self) -> u64 {
        self.row_count as u64 * self.row_bytes as u64
    }
}

/// **The layout at this geometry.** Derived, never chosen: the rows-per-chunk figure is the widest
/// run that fits the leg's own per-chunk cap, which is the same rule
/// `integer_kv_state_geometry_v1` applies to the KV map.
pub fn base0_gdn_state_geometry_v1(
    layers: &[u16],
    heads: u32,
    k_dim: u32,
    v_dim: u32,
    conv_kernel: u32,
) -> Result<Base0GdnStateGeometryV1, Base0GdnStateError> {
    use kaspa_consensus_core::palw_step_leg::{PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES, PALW_STEP_LEG_MAX_STATE_CHUNKS};
    if heads == 0 || k_dim == 0 || v_dim == 0 || conv_kernel == 0 {
        return Err(Base0GdnStateError::ZeroGeometry { heads, k_dim, v_dim });
    }
    if layers.is_empty() {
        return Err(Base0GdnStateError::NoRecurrenceLayers);
    }
    let conv_width = (2 * k_dim as u64 + v_dim as u64) * heads as u64;
    let delta_row_bytes = k_dim as u64 * 4;
    let conv_row_bytes = conv_width * 4;
    for row in [delta_row_bytes, conv_row_bytes] {
        if row > PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES as u64 {
            return Err(Base0GdnStateError::RowExceedsChunk { row_bytes: row, max: PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES });
        }
    }
    let geometry = Base0GdnStateGeometryV1 {
        layers: layers.to_vec(),
        heads,
        k_dim,
        v_dim,
        conv_kernel,
        conv_width: conv_width as u32,
        delta_row_bytes: delta_row_bytes as u32,
        conv_row_bytes: conv_row_bytes as u32,
        delta_rows_per_chunk: ((PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES as u64 / delta_row_bytes).min(v_dim as u64)) as u32,
        conv_rows_per_chunk: ((PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES as u64 / conv_row_bytes).min(conv_kernel as u64)) as u32,
    };
    let count = geometry.chunk_count();
    if count > PALW_STEP_LEG_MAX_STATE_CHUNKS as u64 {
        return Err(Base0GdnStateError::TooManyChunks { got: count, max: PALW_STEP_LEG_MAX_STATE_CHUNKS });
    }
    Ok(geometry)
}

/// The entry at `chunk_index`, or `None` past the end of the map — the enumeration itself.
pub fn base0_gdn_chunk_entry_v1(geometry: &Base0GdnStateGeometryV1, chunk_index: u64) -> Option<Base0GdnChunkEntryV1> {
    if chunk_index >= geometry.chunk_count() {
        return None;
    }
    let per_head = geometry.delta_chunks_per_head() as u64;
    let delta_total = geometry.layers.len() as u64 * geometry.heads as u64 * per_head;
    if chunk_index < delta_total {
        let layer_ordinal = (chunk_index / (geometry.heads as u64 * per_head)) as usize;
        let within_layer = chunk_index % (geometry.heads as u64 * per_head);
        let head = (within_layer / per_head) as u32;
        let block = (within_layer % per_head) as u32;
        let row_start = block * geometry.delta_rows_per_chunk;
        return Some(Base0GdnChunkEntryV1 {
            kind: Base0GdnChunkKindV1::Delta,
            layer: geometry.layers[layer_ordinal],
            head,
            row_start,
            row_count: (geometry.v_dim - row_start).min(geometry.delta_rows_per_chunk),
            row_bytes: geometry.delta_row_bytes,
        });
    }
    let within_conv = chunk_index - delta_total;
    let per_layer = geometry.conv_chunks_per_layer() as u64;
    let layer_ordinal = (within_conv / per_layer) as usize;
    let block = (within_conv % per_layer) as u32;
    let row_start = block * geometry.conv_rows_per_chunk;
    Some(Base0GdnChunkEntryV1 {
        kind: Base0GdnChunkKindV1::Conv,
        layer: geometry.layers[layer_ordinal],
        head: 0,
        row_start,
        row_count: (geometry.conv_kernel - row_start).min(geometry.conv_rows_per_chunk),
        row_bytes: geometry.conv_row_bytes,
    })
}

/// **One recurrence layer's live state**, in the shape the engine holds it: one delta state per
/// head, plus the convolution's `conv_kernel` most recent rows, oldest first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Base0GdnLayerStateV1 {
    pub heads: Vec<kaspa_consensus_core::palw_qwen36_ops::Qwen36GdnStateV1>,
    pub conv: Vec<Vec<i32>>,
}

/// **Chunk a live recurrence state, in the map's own order** — the capture side of Decision 10.
///
/// `states` is one entry per layer of `geometry.layers`, in that order. A state whose shape is not
/// the geometry's is refused by NAME rather than padded: a checkpoint that opens to a state the
/// producer never held is worse than a missing one, and the producer has signed for it — the rule
/// `A16Cache::state_chunk_bytes_v1` states.
pub fn base0_gdn_state_chunks_v1(
    geometry: &Base0GdnStateGeometryV1,
    states: &[Base0GdnLayerStateV1],
) -> Result<Vec<Vec<u8>>, Base0GdnStateError> {
    if states.len() != geometry.layers.len() {
        return Err(Base0GdnStateError::NoRecurrenceLayers);
    }
    for (ordinal, state) in states.iter().enumerate() {
        let layer = geometry.layers[ordinal];
        if state.heads.len() != geometry.heads as usize {
            return Err(Base0GdnStateError::StateIsNotTheGeometrys { layer, head: state.heads.len() as u32 });
        }
        for (head, s) in state.heads.iter().enumerate() {
            if s.d_k != geometry.k_dim as usize || s.d_v != geometry.v_dim as usize || s.s.len() != s.d_k * s.d_v {
                return Err(Base0GdnStateError::StateIsNotTheGeometrys { layer, head: head as u32 });
            }
        }
        if state.conv.len() != geometry.conv_kernel as usize || state.conv.iter().any(|r| r.len() != geometry.conv_width as usize) {
            return Err(Base0GdnStateError::ConvIsNotTheGeometrys { layer });
        }
    }
    let mut out = Vec::with_capacity(geometry.chunk_count() as usize);
    for index in 0..geometry.chunk_count() {
        let entry = base0_gdn_chunk_entry_v1(geometry, index)
            .ok_or(Base0GdnStateError::ChunkCountIsNotTheMaps { got: index as usize, want: geometry.chunk_count() })?;
        let ordinal = geometry.layers.iter().position(|l| *l == entry.layer).ok_or(Base0GdnStateError::NoRecurrenceLayers)?;
        let state = &states[ordinal];
        let mut bytes = Vec::with_capacity(entry.byte_len() as usize);
        match entry.kind {
            Base0GdnChunkKindV1::Delta => {
                let head = &state.heads[entry.head as usize];
                for row in entry.row_start..entry.row_start + entry.row_count {
                    let start = row as usize * geometry.k_dim as usize;
                    for value in &head.s[start..start + geometry.k_dim as usize] {
                        bytes.extend_from_slice(&value.to_le_bytes());
                    }
                }
            }
            Base0GdnChunkKindV1::Conv => {
                for row in entry.row_start..entry.row_start + entry.row_count {
                    for value in &state.conv[row as usize] {
                        bytes.extend_from_slice(&value.to_le_bytes());
                    }
                }
            }
        }
        out.push(bytes);
    }
    Ok(out)
}

/// **Restore a recurrence state from committed chunks** — the replay side of Decision 10, and the
/// inverse [`base0_gdn_state_chunks_v1`]'s round trip pins.
///
/// Every refusal is a refusal to replay, never a partial state: a state assembled from material
/// that does not cover it would fold from zeros, and zeros are indistinguishable from a computed
/// state once they are in a commitment.
pub fn base0_gdn_state_from_chunks_v1(
    geometry: &Base0GdnStateGeometryV1,
    chunks: &[Vec<u8>],
) -> Result<Vec<Base0GdnLayerStateV1>, Base0GdnStateError> {
    use kaspa_consensus_core::palw_qwen36_ops::Qwen36GdnStateV1;
    if chunks.len() as u64 != geometry.chunk_count() {
        return Err(Base0GdnStateError::ChunkCountIsNotTheMaps { got: chunks.len(), want: geometry.chunk_count() });
    }
    let mut states: Vec<Base0GdnLayerStateV1> = geometry
        .layers
        .iter()
        .map(|_| Base0GdnLayerStateV1 {
            heads: (0..geometry.heads).map(|_| Qwen36GdnStateV1::zeros(geometry.v_dim as usize, geometry.k_dim as usize)).collect(),
            conv: vec![vec![0i32; geometry.conv_width as usize]; geometry.conv_kernel as usize],
        })
        .collect();
    for (index, bytes) in chunks.iter().enumerate() {
        let entry = base0_gdn_chunk_entry_v1(geometry, index as u64)
            .ok_or(Base0GdnStateError::ChunkCountIsNotTheMaps { got: chunks.len(), want: geometry.chunk_count() })?;
        if bytes.len() as u64 != entry.byte_len() {
            return Err(Base0GdnStateError::ChunkIsNotItsOwnLength { index: index as u64, got: bytes.len(), want: entry.byte_len() });
        }
        let ordinal = geometry.layers.iter().position(|l| *l == entry.layer).ok_or(Base0GdnStateError::NoRecurrenceLayers)?;
        let values: Vec<i32> = bytes.chunks_exact(4).map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
        let width = entry.row_bytes as usize / 4;
        match entry.kind {
            Base0GdnChunkKindV1::Delta => {
                let head = &mut states[ordinal].heads[entry.head as usize];
                for row in 0..entry.row_count as usize {
                    let dst = (entry.row_start as usize + row) * width;
                    head.s[dst..dst + width].copy_from_slice(&values[row * width..(row + 1) * width]);
                }
            }
            Base0GdnChunkKindV1::Conv => {
                for row in 0..entry.row_count as usize {
                    states[ordinal].conv[entry.row_start as usize + row].copy_from_slice(&values[row * width..(row + 1) * width]);
                }
            }
        }
    }
    Ok(states)
}

// =============================================================================================
// gdn v2 — the same state, enumerated so ONE HEAD's window is one opening
// =============================================================================================

/// **The layout under [`PALW_GDN_STATE_CHUNK_MAP_NAME_V2`]**: the delta half exactly as v1 has it,
/// and the convolution half keyed by HEAD instead of by window row.
///
/// # Why the conv half is re-keyed and the delta half is not
///
/// A court replaying `KDESC_Q36_GDN_STEP` replays one HEAD. v1's delta chunks are already per
/// head, so opening one costs `v_dim × k_dim × 4` — 65,536 bytes on Qwen3.6's geometry, and that
/// is the state the replay genuinely needs. v1's convolution chunks are per LAYER: one row spans
/// every head's `2·k + v` channels, so a court that needed one head's four taps opened the layer's
/// window and paid for thirty-one heads it would not read — 196,608 bytes against an 81,920-byte
/// carrier, a term constant in the context and therefore payable at no context at all.
///
/// v2 covers the SAME bytes. `total_bytes` is v1's, chunk for chunk in the delta half and
/// head-by-head in the conv half, and `the_two_recurrence_maps_cover_the_same_state` is what makes
/// that a property rather than a claim.
///
/// # The gather, because head `h`'s conv channels are not contiguous
///
/// A window row is `[q | k | v]`, region-major, head-major inside each region — the engine's own
/// `current.extend(q); extend(k); extend(v)` in `Qwen36Engine::linear_attention`. So head `h`'s
/// channels are three disjoint ranges, and the map's name spells all three: a gather a reader has
/// to reconstruct is a gather two readers reconstruct differently.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Base0GdnStateGeometryV2 {
    pub layers: Vec<u16>,
    pub heads: u32,
    pub k_dim: u32,
    pub v_dim: u32,
    pub conv_kernel: u32,
    /// `(2 · k_dim + v_dim) · heads` — the full window row the engine holds, which the per-head
    /// rows are gathered OUT of. Kept so a restore can size the row it scatters back into.
    pub conv_width: u32,
    /// `k_dim × 4`: one row of one head's delta state. v1's, unchanged.
    pub delta_row_bytes: u32,
    /// `(2 · k_dim + v_dim) × 4`: one head's slice of one window row — the whole of what v2 moves.
    pub conv_head_row_bytes: u32,
    pub delta_rows_per_chunk: u32,
    pub conv_rows_per_chunk: u32,
}

impl Base0GdnStateGeometryV2 {
    pub fn delta_chunks_per_head(&self) -> u32 {
        self.v_dim.div_ceil(self.delta_rows_per_chunk)
    }
    pub fn conv_chunks_per_head(&self) -> u32 {
        self.conv_kernel.div_ceil(self.conv_rows_per_chunk)
    }
    /// Every chunk in the map: the delta half, then the conv half, both `(layer, head)`-keyed.
    pub fn chunk_count(&self) -> u64 {
        let per_head = self.delta_chunks_per_head() as u64 + self.conv_chunks_per_head() as u64;
        self.layers.len() as u64 * self.heads as u64 * per_head
    }
    pub fn total_bytes(&self) -> u64 {
        let heads = self.layers.len() as u64 * self.heads as u64;
        heads * self.v_dim as u64 * self.delta_row_bytes as u64 + heads * self.conv_kernel as u64 * self.conv_head_row_bytes as u64
    }
    /// **Head `h`'s channel indices inside one full window row**, in the map's declared order
    /// `[q, k, v]`. The one spelling of the gather: the chunker and the restorer both walk it, so
    /// they cannot disagree about which channels are whose.
    pub fn conv_head_channels(&self, head: u32) -> impl Iterator<Item = usize> + '_ {
        let (k, v, heads) = (self.k_dim as usize, self.v_dim as usize, self.heads as usize);
        let h = head as usize;
        (h * k..h * k + k).chain(heads * k + h * k..heads * k + h * k + k).chain(2 * heads * k + h * v..2 * heads * k + h * v + v)
    }
}

/// One chunk of the v2 map: a run of rows of one head's delta state, or of one head's slice of the
/// convolution window. `head` is meaningful for BOTH kinds, which is the difference from v1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Base0GdnChunkEntryV2 {
    pub kind: Base0GdnChunkKindV1,
    pub layer: u16,
    pub head: u32,
    pub row_start: u32,
    pub row_count: u32,
    pub row_bytes: u32,
}

impl Base0GdnChunkEntryV2 {
    pub fn byte_len(&self) -> u64 {
        self.row_count as u64 * self.row_bytes as u64
    }
}

/// **The v2 layout at this geometry.** Derived, never chosen — the same rule v1 applies, over the
/// narrower conv row.
pub fn base0_gdn_state_geometry_v2(
    layers: &[u16],
    heads: u32,
    k_dim: u32,
    v_dim: u32,
    conv_kernel: u32,
) -> Result<Base0GdnStateGeometryV2, Base0GdnStateError> {
    use kaspa_consensus_core::palw_step_leg::{PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES, PALW_STEP_LEG_MAX_STATE_CHUNKS};
    if heads == 0 || k_dim == 0 || v_dim == 0 || conv_kernel == 0 {
        return Err(Base0GdnStateError::ZeroGeometry { heads, k_dim, v_dim });
    }
    if layers.is_empty() {
        return Err(Base0GdnStateError::NoRecurrenceLayers);
    }
    let conv_width = (2 * k_dim as u64 + v_dim as u64) * heads as u64;
    let delta_row_bytes = k_dim as u64 * 4;
    let conv_head_row_bytes = (2 * k_dim as u64 + v_dim as u64) * 4;
    for row in [delta_row_bytes, conv_head_row_bytes] {
        if row > PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES as u64 {
            return Err(Base0GdnStateError::RowExceedsChunk { row_bytes: row, max: PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES });
        }
    }
    if conv_width > u32::MAX as u64 {
        return Err(Base0GdnStateError::RowExceedsChunk { row_bytes: conv_width * 4, max: PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES });
    }
    let geometry = Base0GdnStateGeometryV2 {
        layers: layers.to_vec(),
        heads,
        k_dim,
        v_dim,
        conv_kernel,
        conv_width: conv_width as u32,
        delta_row_bytes: delta_row_bytes as u32,
        conv_head_row_bytes: conv_head_row_bytes as u32,
        delta_rows_per_chunk: ((PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES as u64 / delta_row_bytes).min(v_dim as u64)) as u32,
        conv_rows_per_chunk: ((PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES as u64 / conv_head_row_bytes).min(conv_kernel as u64)) as u32,
    };
    let count = geometry.chunk_count();
    if count > PALW_STEP_LEG_MAX_STATE_CHUNKS as u64 {
        return Err(Base0GdnStateError::TooManyChunks { got: count, max: PALW_STEP_LEG_MAX_STATE_CHUNKS });
    }
    Ok(geometry)
}

/// The v2 entry at `chunk_index`, or `None` past the end of the map — the enumeration itself.
pub fn base0_gdn_chunk_entry_v2(geometry: &Base0GdnStateGeometryV2, chunk_index: u64) -> Option<Base0GdnChunkEntryV2> {
    if chunk_index >= geometry.chunk_count() {
        return None;
    }
    let delta_per_head = geometry.delta_chunks_per_head() as u64;
    let conv_per_head = geometry.conv_chunks_per_head() as u64;
    let delta_total = geometry.layers.len() as u64 * geometry.heads as u64 * delta_per_head;
    if chunk_index < delta_total {
        let layer_ordinal = (chunk_index / (geometry.heads as u64 * delta_per_head)) as usize;
        let within_layer = chunk_index % (geometry.heads as u64 * delta_per_head);
        let head = (within_layer / delta_per_head) as u32;
        let block = (within_layer % delta_per_head) as u32;
        let row_start = block * geometry.delta_rows_per_chunk;
        return Some(Base0GdnChunkEntryV2 {
            kind: Base0GdnChunkKindV1::Delta,
            layer: geometry.layers[layer_ordinal],
            head,
            row_start,
            row_count: (geometry.v_dim - row_start).min(geometry.delta_rows_per_chunk),
            row_bytes: geometry.delta_row_bytes,
        });
    }
    let within_conv = chunk_index - delta_total;
    let layer_ordinal = (within_conv / (geometry.heads as u64 * conv_per_head)) as usize;
    let within_layer = within_conv % (geometry.heads as u64 * conv_per_head);
    let head = (within_layer / conv_per_head) as u32;
    let block = (within_layer % conv_per_head) as u32;
    let row_start = block * geometry.conv_rows_per_chunk;
    Some(Base0GdnChunkEntryV2 {
        kind: Base0GdnChunkKindV1::Conv,
        layer: geometry.layers[layer_ordinal],
        head,
        row_start,
        row_count: (geometry.conv_kernel - row_start).min(geometry.conv_rows_per_chunk),
        row_bytes: geometry.conv_head_row_bytes,
    })
}

/// **Chunk a live recurrence state under v2** — the capture side, gathering each head's conv
/// channels out of the full window rows the engine holds.
pub fn base0_gdn_state_chunks_v2(
    geometry: &Base0GdnStateGeometryV2,
    states: &[Base0GdnLayerStateV1],
) -> Result<Vec<Vec<u8>>, Base0GdnStateError> {
    if states.len() != geometry.layers.len() {
        return Err(Base0GdnStateError::NoRecurrenceLayers);
    }
    for (ordinal, state) in states.iter().enumerate() {
        let layer = geometry.layers[ordinal];
        if state.heads.len() != geometry.heads as usize {
            return Err(Base0GdnStateError::StateIsNotTheGeometrys { layer, head: state.heads.len() as u32 });
        }
        for (head, s) in state.heads.iter().enumerate() {
            if s.d_k != geometry.k_dim as usize || s.d_v != geometry.v_dim as usize || s.s.len() != s.d_k * s.d_v {
                return Err(Base0GdnStateError::StateIsNotTheGeometrys { layer, head: head as u32 });
            }
        }
        if state.conv.len() != geometry.conv_kernel as usize || state.conv.iter().any(|r| r.len() != geometry.conv_width as usize) {
            return Err(Base0GdnStateError::ConvIsNotTheGeometrys { layer });
        }
    }
    let mut out = Vec::with_capacity(geometry.chunk_count() as usize);
    for index in 0..geometry.chunk_count() {
        let entry = base0_gdn_chunk_entry_v2(geometry, index)
            .ok_or(Base0GdnStateError::ChunkCountIsNotTheMaps { got: index as usize, want: geometry.chunk_count() })?;
        let ordinal = geometry.layers.iter().position(|l| *l == entry.layer).ok_or(Base0GdnStateError::NoRecurrenceLayers)?;
        let state = &states[ordinal];
        let mut bytes = Vec::with_capacity(entry.byte_len() as usize);
        match entry.kind {
            Base0GdnChunkKindV1::Delta => {
                let head = &state.heads[entry.head as usize];
                for row in entry.row_start..entry.row_start + entry.row_count {
                    let start = row as usize * geometry.k_dim as usize;
                    for value in &head.s[start..start + geometry.k_dim as usize] {
                        bytes.extend_from_slice(&value.to_le_bytes());
                    }
                }
            }
            Base0GdnChunkKindV1::Conv => {
                for row in entry.row_start..entry.row_start + entry.row_count {
                    let window = &state.conv[row as usize];
                    for channel in geometry.conv_head_channels(entry.head) {
                        let value = window.get(channel).ok_or(Base0GdnStateError::ConvIsNotTheGeometrys { layer: entry.layer })?;
                        bytes.extend_from_slice(&value.to_le_bytes());
                    }
                }
            }
        }
        out.push(bytes);
    }
    Ok(out)
}

/// **Restore a recurrence state from v2 chunks** — the replay side, scattering each head's conv
/// channels back where the gather took them from.
pub fn base0_gdn_state_from_chunks_v2(
    geometry: &Base0GdnStateGeometryV2,
    chunks: &[Vec<u8>],
) -> Result<Vec<Base0GdnLayerStateV1>, Base0GdnStateError> {
    use kaspa_consensus_core::palw_qwen36_ops::Qwen36GdnStateV1;
    if chunks.len() as u64 != geometry.chunk_count() {
        return Err(Base0GdnStateError::ChunkCountIsNotTheMaps { got: chunks.len(), want: geometry.chunk_count() });
    }
    let mut states: Vec<Base0GdnLayerStateV1> = geometry
        .layers
        .iter()
        .map(|_| Base0GdnLayerStateV1 {
            heads: (0..geometry.heads).map(|_| Qwen36GdnStateV1::zeros(geometry.v_dim as usize, geometry.k_dim as usize)).collect(),
            conv: vec![vec![0i32; geometry.conv_width as usize]; geometry.conv_kernel as usize],
        })
        .collect();
    for (index, bytes) in chunks.iter().enumerate() {
        let entry = base0_gdn_chunk_entry_v2(geometry, index as u64)
            .ok_or(Base0GdnStateError::ChunkCountIsNotTheMaps { got: chunks.len(), want: geometry.chunk_count() })?;
        if bytes.len() as u64 != entry.byte_len() {
            return Err(Base0GdnStateError::ChunkIsNotItsOwnLength { index: index as u64, got: bytes.len(), want: entry.byte_len() });
        }
        let ordinal = geometry.layers.iter().position(|l| *l == entry.layer).ok_or(Base0GdnStateError::NoRecurrenceLayers)?;
        let values: Vec<i32> = bytes.chunks_exact(4).map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
        let width = entry.row_bytes as usize / 4;
        match entry.kind {
            Base0GdnChunkKindV1::Delta => {
                let head = &mut states[ordinal].heads[entry.head as usize];
                for row in 0..entry.row_count as usize {
                    let dst = (entry.row_start as usize + row) * width;
                    head.s[dst..dst + width].copy_from_slice(&values[row * width..(row + 1) * width]);
                }
            }
            Base0GdnChunkKindV1::Conv => {
                for row in 0..entry.row_count as usize {
                    let window = &mut states[ordinal].conv[entry.row_start as usize + row];
                    for (slot, channel) in geometry.conv_head_channels(entry.head).enumerate() {
                        window[channel] = values[row * width + slot];
                    }
                }
            }
        }
    }
    Ok(states)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_step_leg::{step_merkle_range_siblings_v1, step_merkle_root_v1, step_range_opening_root_v1};

    fn leaves(n: usize) -> Vec<Hash64> {
        (0..n as u64).map(|i| Hash64::from_u64_word(i.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(7))).collect()
    }

    /// **The accumulator IS the step tree.** The one property this module owes: fold the same
    /// leaves and get the byte-identical root `step_merkle_root_v1` produces, at every retained
    /// level and at every leaf count — the odd ones especially, because promote-odd is the rule a
    /// second fold gets wrong, and a root the court recomputes differently is an honest producer
    /// who can neither be convicted nor paid.
    #[test]
    fn the_sparse_accumulator_is_the_step_tree() {
        let mut counts: Vec<usize> = (1..=80).collect();
        counts.extend([127, 128, 129, 255, 256, 257, 511, 513, 1_000, 1_023, 1_024, 1_025, 4_095, 4_096, 4_097, 8_191]);
        for n in counts {
            let ls = leaves(n);
            let dense = step_merkle_root_v1(&ls).expect("a dense root");
            for retain_level in [0u32, 1, 2, 3, 5, 8, 12, 13] {
                let sparse = Base0SparseStepTreeV1::from_leaves_v1(&ls, retain_level).expect("folds");
                assert_eq!(sparse.root().expect("a shaped tree"), dense, "leaf count {n}, retain level {retain_level}");
                assert_eq!(sparse.retained_nodes().len() as u64, level_width(n as u64, retain_level));
            }
        }
    }

    /// The sparse sibling walk is `step_merkle_range_siblings_v1`'s walk — same set, same ORDER.
    /// Order is part of the form: `step_range_opening_root_v1` consumes left-then-right per level,
    /// bottom-up, and a correctly-valued set in the wrong order verifies against nothing.
    #[test]
    fn the_sparse_range_siblings_are_the_dense_ones() {
        for n in [1usize, 2, 3, 5, 8, 9, 16, 17, 31, 33, 64, 100, 129, 260] {
            let ls = leaves(n);
            for retain_level in [0u32, 1, 2, 3, 5] {
                let tree = Base0SparseStepTreeV1::from_leaves_v1(&ls, retain_level).expect("folds");
                for first in 0..n as u64 {
                    for count in 1..=(n as u64 - first).min(9) {
                        let (span_first, span_end) = tree.span_for_range(first, count).expect("a span");
                        let span = &ls[span_first as usize..span_end as usize];
                        let sparse = tree.range_siblings_v1(span_first, span, first, count).expect("siblings");
                        let dense = step_merkle_range_siblings_v1(&ls, first as usize, count as usize).expect("dense siblings");
                        assert_eq!(sparse, dense, "n={n} level={retain_level} first={first} count={count}");
                    }
                }
            }
        }
    }

    /// The opening the sparse tree assembles verifies against the tree's own root, which is the
    /// step leg's root — end to end, without the dense leaf vector ever being consulted for
    /// anything but the replayed span.
    #[test]
    fn a_sparse_range_opening_verifies_against_the_step_root() {
        for n in [1usize, 7, 16, 33, 100, 257] {
            let ls = leaves(n);
            let tree = Base0SparseStepTreeV1::from_leaves_v1(&ls, 3).expect("folds");
            let root = tree.root().expect("a shaped tree");
            for first in 0..n as u64 {
                for count in 1..=(n as u64 - first).min(5) {
                    let (span_first, span_end) = tree.span_for_range(first, count).expect("a span");
                    let opening = tree
                        .range_opening_v1(span_first, &ls[span_first as usize..span_end as usize], first, count)
                        .expect("an opening");
                    assert_eq!(step_range_opening_root_v1(n as u64, &opening).expect("recomputes"), root);
                }
            }
        }
    }

    /// A short fold is refused, for the reason `Base0StepCaptureV1::finish` refuses a short
    /// capture: the root over an unfilled space says "computed zero" about every missing leaf.
    #[test]
    fn a_short_fold_is_refused_by_name() {
        let mut acc = Base0SparseStepAccumulatorV1::new(4, 1).expect("new");
        acc.push(Hash64::from_u64_word(1)).expect("push");
        assert_eq!(acc.progress(), (1, 4));
        assert_eq!(acc.finish().unwrap_err(), Base0SparseCaptureError::CaptureIncomplete { pushed: 1, expected: 4 });

        let mut acc = Base0SparseStepAccumulatorV1::new(1, 1).expect("new");
        acc.push(Hash64::from_u64_word(1)).expect("push");
        assert_eq!(acc.push(Hash64::from_u64_word(2)).unwrap_err(), Base0SparseCaptureError::CaptureOverrun { got: 2, expected: 1 });
    }

    /// The retention bound the module exists for, stated as arithmetic: what an executor keeps is
    /// the leaf count divided by the block, never the leaf count.
    #[test]
    fn the_retained_vector_is_the_leaf_count_over_the_block() {
        let tree = Base0SparseStepTreeV1::from_leaves_v1(&leaves(4_097), PALW_BASE0_SPARSE_RETAIN_LEVEL_V1).expect("folds");
        assert_eq!(tree.retained_nodes().len(), 2, "4,097 leaves ride two retained nodes at level 12");
        // The whole tree at the ladder's current cap, priced: 2^22 leaves is 1,024 retained nodes.
        assert_eq!(level_width(1 << 22, PALW_BASE0_SPARSE_RETAIN_LEVEL_V1), 1_024);
    }

    // ------------------------------------------------------------------------------------------
    // ADR-0077 Decision 10, the recurrence half — W2 on a synthetic GatedDeltaNet state map
    // ------------------------------------------------------------------------------------------

    use kaspa_consensus_core::palw_base0_a16::A16QuantParams;
    use kaspa_consensus_core::palw_qwen36_ops::{Qwen36GdnParamsV1, Qwen36GdnStateV1, q36_gdn_step};

    /// A deterministic little stream of A16 codes — the same shape a projection would hand the
    /// recurrence, without a 33 GiB artifact to produce it.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> i32 {
            self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            ((self.0.rotate_right(24) % 4_001) as i64 - 2_000) as i32
        }
        fn row(&mut self, n: usize) -> Vec<i32> {
            (0..n).map(|_| self.next()).collect()
        }
    }

    /// One head's inputs at one position: `(k, v, q, decay, beta)` — what the projections in front
    /// of the recurrence hand it.
    type GdnStep = (Vec<i32>, Vec<i32>, Vec<i32>, i64, i64);

    fn gdn_params() -> Qwen36GdnParamsV1 {
        let p = |m: i64, sh: u8| A16QuantParams { multiplier: m, shift: sh, zero: 0 };
        Qwen36GdnParamsV1 { read: p(1 << 20, 20), write_shift: -6, delta: p(1 << 21, 20), out: p(1 << 20, 21) }
    }

    /// The synthetic class: two recurrence layers, two heads, `k=4`, `v=3`, a 3-row conv window.
    fn gdn_geometry() -> Base0GdnStateGeometryV1 {
        base0_gdn_state_geometry_v1(&[1, 3], 2, 4, 3, 3).expect("a geometry")
    }

    fn gdn_live_state(seed: u64, geometry: &Base0GdnStateGeometryV1) -> Vec<Base0GdnLayerStateV1> {
        let mut rng = Lcg(seed);
        geometry
            .layers
            .iter()
            .map(|_| Base0GdnLayerStateV1 {
                heads: (0..geometry.heads)
                    .map(|_| {
                        let mut st = Qwen36GdnStateV1::zeros(geometry.v_dim as usize, geometry.k_dim as usize);
                        st.s = rng.row(st.s.len());
                        st
                    })
                    .collect(),
                conv: (0..geometry.conv_kernel).map(|_| rng.row(geometry.conv_width as usize)).collect(),
            })
            .collect()
    }

    /// **The map is a bijection on the state it describes.** Chunk a live recurrence state and
    /// restore it: byte for byte, value for value. A map that lost the convolution window, or
    /// mis-ordered the heads, would restore a state the replay folds forward — and the divergence
    /// would surface as a fault against an honest producer, at a position nobody could point at.
    #[test]
    fn the_recurrence_state_survives_its_own_map() {
        let geometry = gdn_geometry();
        let live = gdn_live_state(0xC0FFEE, &geometry);
        let chunks = base0_gdn_state_chunks_v1(&geometry, &live).expect("chunks");
        assert_eq!(chunks.len() as u64, geometry.chunk_count());
        assert_eq!(chunks.iter().map(|c| c.len() as u64).sum::<u64>(), geometry.total_bytes());
        for (index, bytes) in chunks.iter().enumerate() {
            let entry = base0_gdn_chunk_entry_v1(&geometry, index as u64).expect("an entry");
            assert_eq!(bytes.len() as u64, entry.byte_len(), "chunk {index} is its entry's length");
        }
        assert_eq!(base0_gdn_state_from_chunks_v1(&geometry, &chunks).expect("restores"), live);

        // A chunk that is not its own declared length is a refusal, never a partial state.
        let mut short = chunks.clone();
        short[0].pop();
        assert!(matches!(
            base0_gdn_state_from_chunks_v1(&geometry, &short),
            Err(Base0GdnStateError::ChunkIsNotItsOwnLength { index: 0, .. })
        ));
        assert!(matches!(
            base0_gdn_state_from_chunks_v1(&geometry, &chunks[1..]),
            Err(Base0GdnStateError::ChunkCountIsNotTheMaps { .. })
        ));
        // And the layout string is frozen: the id a class would register is a function of it.
        assert_eq!(base0_gdn_state_chunk_map_id_v1(), base0_gdn_state_chunk_map_id_v1());
        assert_ne!(
            base0_gdn_state_chunk_map_id_v1(),
            kaspa_consensus_core::palw_state_chunk_map::integer_kv_state_chunk_map_id_v1(),
            "the recurrence layout is not the KV one"
        );
    }

    /// **The executor's map ids ARE the court's** — one spelling, in the lower crate, called from
    /// here.
    ///
    /// This crate held its own copy of the layout string for a round. Two spellings of a map name
    /// is two ids, and a class whose capture and whose adjudicator disagree about their map id is a
    /// class no dispute can open: the producer commits chunks under one identity and the court
    /// hashes them under another, so every honest refutation reads as malformed evidence. The
    /// identity is checked here rather than assumed because the copy compiled fine.
    #[test]
    fn the_executors_recurrence_map_ids_are_the_courts() {
        use kaspa_consensus_core::palw_state_chunk_map as map;
        assert_eq!(base0_gdn_state_chunk_map_id_v1(), map::gdn_state_chunk_map_id_v1());
        assert_eq!(base0_gdn_state_chunk_map_id_v2(), map::gdn_state_chunk_map_id_v2());
        assert_ne!(base0_gdn_state_chunk_map_id_v1(), base0_gdn_state_chunk_map_id_v2(), "two enumerations, two ids");
        assert_eq!(PALW_GDN_STATE_CHUNK_MAP_NAME_V1, map::PALW_GDN_STATE_CHUNK_MAP_NAME_V1);
        assert_eq!(PALW_GDN_STATE_CHUNK_MAP_NAME_V2, map::PALW_GDN_STATE_CHUNK_MAP_NAME_V2);

        // **A class with attention layers registers the COMPOSITION, not the recurrence half.**
        // Registering the recurrence map alone leaves its attention anchors with no geometry the
        // court can read, which is `Unadjudicable` on honest material.
        let hybrid = kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v2(
            kaspa_consensus_core::palw_qwen36_profile::qwen36_geometry_artifact_eps(
                kaspa_consensus_core::palw_qwen36_profile::QWEN36_35B_A3B,
            ),
        )
        .expect("the pinned hybrid geometry projects");
        assert_ne!(hybrid.full_attention_interval, 0, "the hybrid stopped having attention layers");
        assert_eq!(base0_class_state_chunk_map_id_v2(&hybrid), map::hybrid_state_chunk_map_id_v2());
        assert_eq!(base0_class_state_chunk_map_id_v1(&hybrid), map::hybrid_state_chunk_map_id_v1());
        let mut recurrent = hybrid.clone();
        recurrent.full_attention_interval = 0;
        assert_eq!(base0_class_state_chunk_map_id_v2(&recurrent), map::gdn_state_chunk_map_id_v2());
        assert_eq!(base0_class_state_chunk_map_id_v1(&recurrent), map::gdn_state_chunk_map_id_v1());
    }

    /// **v2 is a RE-ORDERING, not a narrowing: the two maps cover the same state.**
    ///
    /// Same total bytes, same restored value. A v2 map that dropped a channel would restore a
    /// state the producer never held and fold forward from it — the divergence surfacing as a
    /// fault against an honest producer, at a position nobody could point at. The head-slice is
    /// only sound because the bytes are all still there, so that is what is checked.
    #[test]
    fn the_two_recurrence_maps_cover_the_same_state() {
        let v1 = gdn_geometry();
        let v2 = base0_gdn_state_geometry_v2(&[1, 3], 2, 4, 3, 3).expect("a v2 geometry");
        assert_eq!(v2.total_bytes(), v1.total_bytes(), "v2 covers different bytes than v1");
        assert_eq!(
            v2.conv_head_row_bytes as u64 * v2.heads as u64,
            v1.conv_row_bytes as u64,
            "a head row is the layer row over heads"
        );
        // Every channel of a window row belongs to exactly one head, and to one slot of it.
        let mut seen = std::collections::HashSet::new();
        for head in 0..v2.heads {
            for channel in v2.conv_head_channels(head) {
                assert!(seen.insert(channel), "channel {channel} is in two heads' slices");
                assert!(channel < v2.conv_width as usize, "channel {channel} is past the window row");
            }
        }
        assert_eq!(seen.len(), v2.conv_width as usize, "the head slices do not cover the window row");

        let live = gdn_live_state(0xC0FFEE, &v1);
        let chunks = base0_gdn_state_chunks_v2(&v2, &live).expect("v2 chunks");
        assert_eq!(chunks.len() as u64, v2.chunk_count());
        assert_eq!(chunks.iter().map(|c| c.len() as u64).sum::<u64>(), v2.total_bytes());
        for (index, bytes) in chunks.iter().enumerate() {
            let entry = base0_gdn_chunk_entry_v2(&v2, index as u64).expect("an entry");
            assert_eq!(bytes.len() as u64, entry.byte_len(), "chunk {index} is its entry's length");
        }
        assert_eq!(base0_gdn_state_from_chunks_v2(&v2, &chunks).expect("restores"), live, "the v2 map is not a bijection");

        // The refusals are v1's, for v1's reasons: a partial state is never assembled.
        let mut short = chunks.clone();
        short[0].pop();
        assert!(matches!(
            base0_gdn_state_from_chunks_v2(&v2, &short),
            Err(Base0GdnStateError::ChunkIsNotItsOwnLength { index: 0, .. })
        ));
        assert!(matches!(base0_gdn_state_from_chunks_v2(&v2, &chunks[1..]), Err(Base0GdnStateError::ChunkCountIsNotTheMaps { .. })));
        // And the two maps' chunk streams are genuinely different objects — a v1 capture opened
        // under v2 restores a state nobody folded, which is why they are different classes.
        assert_ne!(chunks, base0_gdn_state_chunks_v1(&v1, &live).expect("v1 chunks"));
    }

    /// **The v2 map's cost, on the geometry that decided it.** Qwen3.6: 32 heads of 128, a
    /// four-tap window. The figures are the court's
    /// (`palw_state_chunk_map::gdn_state_row_bytes_v*`) read back through the executor's own
    /// geometry, because a capture whose rows are not the size the court priced is a capture the
    /// court cannot afford to open.
    #[test]
    fn one_heads_opening_is_the_carriers_size_under_v2_and_not_under_v1() {
        let (heads, k, v, kernel) = (32u32, 128u32, 128u32, 4u32);
        let layers: Vec<u16> = (0..30).collect();
        let v1 = base0_gdn_state_geometry_v1(&layers, heads, k, v, kernel).expect("v1");
        let v2 = base0_gdn_state_geometry_v2(&layers, heads, k, v, kernel).expect("v2");
        let budget = kaspa_consensus_core::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES;

        let delta = v as u64 * v1.delta_row_bytes as u64;
        assert_eq!(delta, 65_536);
        assert_eq!(kernel as u64 * v1.conv_row_bytes as u64, 196_608, "v1's window spans every head");
        assert_eq!(kernel as u64 * v2.conv_head_row_bytes as u64, 6_144, "v2's window is one head's");
        assert!(delta + kernel as u64 * v1.conv_row_bytes as u64 > budget, "v1 fits the carrier after all");
        assert!(delta + kernel as u64 * v2.conv_head_row_bytes as u64 <= budget, "v2 does not fit the carrier");
        assert_eq!(v1.total_bytes(), v2.total_bytes(), "the re-ordering moved bytes");
    }

    /// **W2 for the recurrence: the anchored replay and the long form reach the same state.**
    ///
    /// The genesis form folds every position from zero — what `gdn_core_genesis_replay` does, and
    /// what holds `positions = n_ctx` in `derive_court_cost_v1`. The anchored form takes the state
    /// the producer committed at position `c`, restores it THROUGH THE MAP (not from the live
    /// object — that would prove nothing about the chunking), and folds only the positions after
    /// it. Every output row of every position from `c` on must be identical, and so must the final
    /// state: an integer recurrence has no tolerance, and "close" is not a verdict.
    ///
    /// This is the whole claim Decision 10 makes about the recurrence, on the shipped arithmetic
    /// (`q36_gdn_step`) rather than a restatement of it.
    #[test]
    fn an_anchored_recurrence_replay_is_the_genesis_one() {
        let geometry = gdn_geometry();
        let params = gdn_params();
        let (kd, vd) = (geometry.k_dim as usize, geometry.v_dim as usize);
        let positions = 12usize;
        let mut rng = Lcg(0x5EED);
        // One (k, v, q, decay, beta) per position per head — the projections' output, synthesised.
        let inputs: Vec<Vec<GdnStep>> = (0..positions)
            .map(|_| {
                (0..geometry.heads)
                    .map(|_| {
                        let k = rng.row(kd);
                        let v = rng.row(vd);
                        let q = rng.row(kd);
                        // Both gates live in [0, ONE] on the Q-rail the op declares.
                        let decay = (rng.next().unsigned_abs() as i64) % (1 << 24);
                        let beta = (rng.next().unsigned_abs() as i64) % (1 << 24);
                        (k, v, q, decay, beta)
                    })
                    .collect()
            })
            .collect();

        let fold = |state: &mut Base0GdnLayerStateV1, from: usize| -> Vec<Vec<i32>> {
            let mut out = Vec::new();
            for step in inputs.iter().skip(from) {
                let mut row = Vec::new();
                for (head, (k, v, q, decay, beta)) in step.iter().enumerate() {
                    row.extend(q36_gdn_step(&mut state.heads[head], k, v, q, *decay, *beta, params).expect("the step runs"));
                }
                out.push(row);
            }
            out
        };

        // The long form: fold all twelve positions from a zero state, keeping the state at the
        // checkpoint on the way past it.
        let zero = || Base0GdnLayerStateV1 {
            heads: (0..geometry.heads).map(|_| Qwen36GdnStateV1::zeros(vd, kd)).collect(),
            conv: vec![vec![0i32; geometry.conv_width as usize]; geometry.conv_kernel as usize],
        };
        let anchor_at = 5usize;
        let mut long = zero();
        let long_head = fold(&mut long, 0);
        // The state the producer would have committed: exactly `anchor_at` positions folded.
        let mut at_anchor = zero();
        for step in inputs.iter().take(anchor_at) {
            for (head, (k, v, q, decay, beta)) in step.iter().enumerate() {
                q36_gdn_step(&mut at_anchor.heads[head], k, v, q, *decay, *beta, params).expect("the step runs");
            }
        }

        // Commit it: chunk it under the map, and restore from the CHUNKS.
        //
        // **Both enumerations, in one sweep.** v2 re-keys the convolution half by head, and a
        // re-keying is exactly the kind of change that round-trips its own test and still restores
        // a state the replay folds forward differently. So the equivalence is asserted for each map
        // against the SAME long form: a v1-named class still reaches the genesis verdict, and so
        // does a v2-named one.
        let v2 = base0_gdn_state_geometry_v2(&geometry.layers, geometry.heads, geometry.k_dim, geometry.v_dim, geometry.conv_kernel)
            .expect("a v2 geometry");
        let restored_v1 = base0_gdn_state_from_chunks_v1(
            &geometry,
            &base0_gdn_state_chunks_v1(&geometry, &[at_anchor.clone(), at_anchor.clone()]).expect("v1 chunks"),
        )
        .expect("restores");
        let restored_v2 = base0_gdn_state_from_chunks_v2(
            &v2,
            &base0_gdn_state_chunks_v2(&v2, &[at_anchor.clone(), at_anchor.clone()]).expect("v2 chunks"),
        )
        .expect("restores");
        assert_eq!(restored_v1[0], at_anchor, "the committed state is the state that was folded");
        assert_eq!(restored_v2[0], at_anchor, "the head-sliced map restores a different state than it was given");

        for (map, restored) in [("gdn v1", &restored_v1), ("gdn v2", &restored_v2)] {
            let mut anchored = restored[0].clone();
            let anchored_tail = fold(&mut anchored, anchor_at);

            assert_eq!(
                anchored_tail,
                long_head[anchor_at..].to_vec(),
                "{map}: every row after the anchor must be identical — an integer recurrence has no tolerance"
            );
            assert_eq!(anchored.heads, long.heads, "{map}: and the states they end in are the same state");
            assert_eq!(
                anchored_tail.len(),
                positions - anchor_at,
                "{map}: the anchored form costs the positions since the checkpoint"
            );
        }
    }
}
