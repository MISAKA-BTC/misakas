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
    /// **The fold was handed a call it was not expecting.** A cursor is only the canonical index
    /// while the terms arrive in the canonical order; the dense capture could take them in any
    /// order because it addressed every leaf, and this cannot.
    CallOutOfOrder {
        expected: (u32, u32),
        got: (u32, u32),
    },
    /// A captured row whose `(table, layer, index)` is not a slot of the class's graph.
    RowIsNotThisGraphs {
        layer: u16,
        index: usize,
    },
    /// Two rows for one global slot — one execution, two answers about the same node.
    TwoRowsForOneSlot,
    /// The position's enumeration reaches a slot the caller supplied no row for. The dense
    /// capture's own refusal is the short capture its `finish` rejects, one call later.
    MissingRowForSlot {
        slot: u32,
    },
    /// A row for a slot this position does not have — a post row where no token is selected, or a
    /// slot past the graph. `NotACanonicalCoordinate` on the dense path.
    RowForASlotThePositionDoesNotHave {
        slot: u32,
    },
    /// A row whose width is not the one its node's `out_len` implies at this position's `kv_len`.
    /// The dense capture places such a row's overflow tiles at coordinates
    /// `canonical_step_leaf_index` refuses; the fold has no coordinate to refuse, so it compares
    /// the width itself.
    RowIsNotTheGraphsWidth {
        slot: u32,
        got: u64,
        tiles: u64,
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
            Self::CallOutOfOrder { expected, got } => write!(
                f,
                "the fold expected call {} position {} and was handed call {} position {}",
                expected.0, expected.1, got.0, got.1
            ),
            Self::RowIsNotThisGraphs { layer, index } => {
                write!(f, "a captured row at layer {layer} index {index} is not a slot of this class's graph")
            }
            Self::TwoRowsForOneSlot => write!(f, "two captured rows name one global node slot"),
            Self::MissingRowForSlot { slot } => write!(f, "this position's enumeration reaches slot {slot} and no row was captured for it"),
            Self::RowForASlotThePositionDoesNotHave { slot } => {
                write!(f, "a row was captured for slot {slot}, which this position's enumeration does not reach")
            }
            Self::RowIsNotTheGraphsWidth { slot, got, tiles } => {
                write!(f, "slot {slot} committed {got} values, which is not the {tiles} tile(s) its declared width implies")
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
        Self::new_capped_v1(leaf_count, retain_level, PALW_STEP_LEG_MAX_LEAVES)
    }

    /// **The same fold against the ladder top the RULESET froze** — W1b's threading
    /// (`bb4f145b`) applied to the retention side. The leg's own constant is a DEFAULT, and a job
    /// priced against a deeper ruleset must be foldable or the executor commits a leg it cannot
    /// retain. A caller with no ruleset in scope passes the default and nothing moves.
    pub fn new_capped_v1(leaf_count: u64, retain_level: u32, max_step_leaf_count: u64) -> Result<Self, Base0SparseCaptureError> {
        if leaf_count == 0 || leaf_count > max_step_leaf_count {
            return Err(Base0SparseCaptureError::LeafCountOutOfRange { got: leaf_count, max: max_step_leaf_count });
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
        Self::from_leaves_capped_v1(leaves, retain_level, PALW_STEP_LEG_MAX_LEAVES)
    }

    /// [`Self::from_leaves_v1`] against the ruleset's ladder — the same threading, for the same
    /// reason ([`Base0SparseStepAccumulatorV1::new_capped_v1`]).
    pub fn from_leaves_capped_v1(
        leaves: &[Hash64],
        retain_level: u32,
        max_step_leaf_count: u64,
    ) -> Result<Self, Base0SparseCaptureError> {
        let mut acc = Base0SparseStepAccumulatorV1::new_capped_v1(leaves.len() as u64, retain_level, max_step_leaf_count)?;
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
        // **No ladder bound here, deliberately.** The bound on the accumulator's `leaf_count` is
        // an ALLOCATION guard — the dense forms size a `Hash64` vector from the field, and a
        // stranger's blob asking for `2^48` leaves is a process abort. Nothing in this type is
        // sized from `leaf_count`: the retained vector's width is checked against it below, the
        // level walks are `O(64)`, and every leaf-hash slice an opening reads is the caller's own.
        // Bounding it here by the leg's DEFAULT ladder would refuse a tree a deeper ruleset made
        // legal, which is the w1b defect in the retention path.
        if self.leaf_count == 0 {
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
// The retained level, derived from the ruleset's ladder
// =============================================================================================

/// **What the retained set is allowed to weigh: 64 MiB, at any ladder** (ADR-0082 Decision 7).
///
/// The budget is the quantity the level is derived FROM; the level is not a number anyone types.
pub const PALW_BASE0_SPARSE_RETAIN_BUDGET_BYTES: u64 = 64 << 20;

/// `⌈log₂(budget / node bytes)⌉` — how many retained nodes the budget buys, as a level. A
/// `Hash64` is 64 bytes, so 64 MiB is `2^20` of them.
const fn sparse_retained_nodes_log2_v1() -> u32 {
    (PALW_BASE0_SPARSE_RETAIN_BUDGET_BYTES / 64).trailing_zeros()
}

/// **The retained level a ladder of `max_step_leaf_count` leaves implies**:
/// `retain_level = ⌈log₂ leaves⌉ − log₂(budget / 64)` — ADR-0082 Decision 7's own rule, which is
/// the smallest level whose retained vector fits [`PALW_BASE0_SPARSE_RETAIN_BUDGET_BYTES`].
///
/// The argument is the RULESET's `PalwCourtParamsV2::max_step_leaf_count`, which is what the
/// executor has priced its job against since W1b (`bb4f145b`) — never a constant here. At the
/// `2^32` ladder ADR-0077 Decision 12 moves to it returns 12, which is what
/// [`PALW_BASE0_SPARSE_RETAIN_LEVEL_V1`] is; at any deeper ladder it returns more, and the
/// retained set does not grow past the budget.
pub fn palw_base0_sparse_retain_level_for_ladder_v1(max_step_leaf_count: u64) -> u32 {
    let ladder_level = max_step_leaf_count.next_power_of_two().trailing_zeros();
    ladder_level.saturating_sub(sparse_retained_nodes_log2_v1())
}

/// **The level a capture folds at, for a ruleset whose ladder is `max_step_leaf_count`.**
///
/// The ladder's own derivation, floored at [`PALW_BASE0_SPARSE_RETAIN_LEVEL_V1`]. The floor is not
/// a second rule about bytes — a level BELOW it retains more nodes, and it buys nothing, because
/// the thing an opening re-derives is a whole CALL of the enumeration (~99 k leaves on the dense
/// tier, ~298 k on the hybrid) and every level below `2^12` is already far inside one call. So
/// under the shipped `2^22` ladder a claim's retained tree is 64 KiB rather than the 64 MiB the
/// budget would allow, and at `2^32` the two rules agree on 12.
pub fn palw_base0_sparse_retain_level_v1(max_step_leaf_count: u64) -> u32 {
    palw_base0_sparse_retain_level_for_ladder_v1(max_step_leaf_count).max(PALW_BASE0_SPARSE_RETAIN_LEVEL_V1)
}

// =============================================================================================
// The capture: a job's rows folded as they are produced
// =============================================================================================

/// The number of leaves one node of the graph commits at a position whose cache holds `kv_len`
/// rows — `tiles_for(node_out_len(node, kv_len), node.tile_len)`, which `palw_step` keeps private
/// to its own enumeration.
///
/// Restating it here is a second spelling of a consensus rule, and the way it is kept honest is
/// the way the fold itself is: `the_fold_places_every_leaf_where_the_canonical_enumeration_does`
/// compares this capture's whole index→hash map against the dense one's, which is built out of
/// `canonical_step_leaf_index` and nothing else. A drift in either direction fails there rather
/// than at a producer nobody can pay.
fn node_leaf_count_v1(node: &kaspa_consensus_core::palw_step::PalwStepNodeV1, kv_len: u64) -> u64 {
    use kaspa_consensus_core::palw_step::PalwStepOutLenV1;
    let elements = match node.out_len {
        PalwStepOutLenV1::Fixed { elements } => elements as u64,
        PalwStepOutLenV1::KvScaled { multiplier } => multiplier as u64 * kv_len,
    };
    if node.tile_len == 0 { 0 } else { elements.div_ceil(node.tile_len as u64) }
}

/// **The free-prompt capture: every leaf hashed the moment the engine produces it, folded, and
/// thrown away** (ADR-0082 Decision 7).
///
/// The dense twin ([`crate::legs::Base0StepCaptureV1`]) holds a `Hash64` for every leaf of the
/// step space AND a `PalwStepTileLeafV1` — the tile's own bytes — for every one of them: ~50 MB a
/// position on the dense tier, and it asks `canonical_step_leaf_index` where each of the ~110 k
/// tiles of a position goes, a walk that is itself linear in the calls already captured. This
/// holds one retained node per `2^retain_level` leaves and walks the enumeration ONCE per
/// position, as a cursor: the canonical order is call-major, position-major, slot-major,
/// tile-major, so the leaf a row's tile lands on is the one after the leaf before it.
///
/// **What it therefore requires of its caller, and refuses by name when it does not get it:**
/// calls in ascending order from 0, positions in ascending order from 0 inside the prefill call,
/// and every row of the position the class's graph declares — no more, no fewer, each the width
/// its node's `out_len` implies at that position's `kv_len`. The dense capture could take rows in
/// any order because it addressed every leaf; this one is a fold, and a fold cannot be given its
/// terms out of order. Every one of those refusals is a case the dense capture also refused (as
/// `NotACanonicalCoordinate`, or as the short capture its `finish` rejects) — the fold names them
/// where they happen instead.
pub struct Base0SparseStepCaptureV1 {
    ctx_hash: Hash64,
    profile_hash: Hash64,
    prefill: u32,
    decode_calls: u32,
    acc: Base0SparseStepAccumulatorV1,
    /// The next canonical leaf index the fold expects — the cursor that replaces a lookup per tile.
    cursor: u64,
    next_call: u32,
    next_position: u32,
}

impl Base0SparseStepCaptureV1 {
    /// `leaf_count` is the class's own step-leaf count for this job, priced against the ruleset's
    /// ladder (`step_leaf_count_capped_v1`); `retain_level` is
    /// [`palw_base0_sparse_retain_level_v1`] of that same ladder.
    pub fn new(
        profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
        ctx: &kaspa_consensus_core::palw_v2::PalwJobContextV2,
        leaf_count: u64,
        retain_level: u32,
    ) -> Result<Self, Base0SparseCaptureError> {
        Self::new_capped_v1(profile, ctx, leaf_count, retain_level, PALW_STEP_LEG_MAX_LEAVES)
    }

    /// The same capture against the ruleset's ladder.
    pub fn new_capped_v1(
        profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
        ctx: &kaspa_consensus_core::palw_v2::PalwJobContextV2,
        leaf_count: u64,
        retain_level: u32,
        max_step_leaf_count: u64,
    ) -> Result<Self, Base0SparseCaptureError> {
        Ok(Self {
            ctx_hash: ctx.context_hash(),
            profile_hash: profile.shape_profile_id(),
            prefill: ctx.declared_prefill_tokens,
            decode_calls: ctx.exact_decode_tokens.saturating_sub(1),
            acc: Base0SparseStepAccumulatorV1::new_capped_v1(leaf_count, retain_level, max_step_leaf_count)?,
            cursor: 0,
            next_call: 0,
            next_position: 0,
        })
    }

    /// How much of the step space this capture has folded — the sparse twin of
    /// [`crate::legs::Base0StepCaptureV1::progress`].
    pub fn progress(&self) -> (u64, u64) {
        self.acc.progress()
    }

    /// **One forward call's committed rows, folded in canonical order.**
    ///
    /// `rows` are the family's own flattening ([`crate::legs::a16_captured_rows_v1`] and its two
    /// siblings); they may arrive in any order within the call, because the slot walk below places
    /// them, but the SET must be the position's own.
    pub fn push_call(
        &mut self,
        profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
        call_index: u32,
        position: u32,
        rows: &[crate::legs::Base0CapturedRowV1],
    ) -> Result<(), Base0SparseCaptureError> {
        use kaspa_consensus_core::palw_step::PalwStepCoordinateV1;
        use kaspa_consensus_core::palw_step_leg::{PALW_STEP_LEG_OBJECT_VERSION_V1, PalwStepTileLeafV1};
        if call_index != self.next_call || position != self.next_position {
            return Err(Base0SparseCaptureError::CallOutOfOrder {
                expected: (self.next_call, self.next_position),
                got: (call_index, position),
            });
        }
        // The enumeration's own two facts about a position (`canonical_step_leaf_index`): what the
        // cache holds when it runs, and whether the logits table exists at it.
        let (kv_len, with_logits) = if call_index == 0 {
            (position as u64 + 1, position + 1 == self.prefill)
        } else {
            (self.prefill as u64 + call_index as u64, true)
        };

        // Rows by global slot: one `global_node_slot` per ROW, never one per tile.
        let slot_count = profile.global_node_count();
        let first_post_slot = slot_count.saturating_sub(profile.post_nodes.len() as u32);
        let mut by_slot: Vec<(u32, usize)> = Vec::with_capacity(rows.len());
        for (at, row) in rows.iter().enumerate() {
            let slot = profile
                .global_node_slot(row.table, row.layer, row.index)
                .ok_or(Base0SparseCaptureError::RowIsNotThisGraphs { layer: row.layer, index: row.index })?;
            by_slot.push((slot, at));
        }
        by_slot.sort_unstable_by_key(|(slot, _)| *slot);
        if by_slot.windows(2).any(|w| w[0].0 == w[1].0) {
            return Err(Base0SparseCaptureError::TwoRowsForOneSlot);
        }

        // The position's slots, in the order the enumeration walks them, against the rows in hand.
        let mut supplied = by_slot.iter().peekable();
        for slot in 0..slot_count {
            if slot >= first_post_slot && !with_logits {
                continue; // post nodes do not exist at a position that selects no token
            }
            let (node, _) = profile.resolve_node_slot(slot).ok_or(Base0SparseCaptureError::MissingRowForSlot { slot })?;
            let expected = node_leaf_count_v1(node, kv_len);
            let Some((_, at)) = supplied.next_if(|(s, _)| *s == slot).copied() else {
                return Err(Base0SparseCaptureError::MissingRowForSlot { slot });
            };
            let row = &rows[at];
            let tile_len = node.tile_len as usize;
            if tile_len == 0 || row.row.len().div_ceil(tile_len) as u64 != expected {
                return Err(Base0SparseCaptureError::RowIsNotTheGraphsWidth { slot, got: row.row.len() as u64, tiles: expected });
            }
            for (tile_index, chunk) in row.row.chunks(tile_len).enumerate() {
                let leaf = PalwStepTileLeafV1 {
                    version: PALW_STEP_LEG_OBJECT_VERSION_V1,
                    coord: PalwStepCoordinateV1 {
                        call_index,
                        node_slot: slot,
                        position,
                        tile_index: tile_index as u32,
                    },
                    value_count: chunk.len() as u32,
                    values_le: chunk.iter().flat_map(|v| v.to_le_bytes()).collect(),
                };
                self.acc.push(kaspa_consensus_core::palw_step_leg::step_tile_leaf_hash_v1(&self.ctx_hash, &self.profile_hash, &leaf))?;
                self.cursor += 1;
            }
        }
        // A row for a slot the walk did not reach — a post row at a non-selecting position, or a
        // slot past the graph — is a row about a different execution.
        if let Some((slot, _)) = supplied.next() {
            return Err(Base0SparseCaptureError::RowForASlotThePositionDoesNotHave { slot: *slot });
        }

        if call_index == 0 && position + 1 < self.prefill {
            self.next_position = position + 1;
        } else {
            self.next_call = call_index + 1;
            self.next_position = 0;
        }
        Ok(())
    }

    /// Seal the fold: the retained tree, and nothing else. A short capture is refused for the
    /// reason [`crate::legs::Base0StepCaptureV1::finish`] refuses one — a commitment over a
    /// partial space says "computed zero" about every leaf nobody filled.
    pub fn finish(self) -> Result<Base0SparseStepTreeV1, Base0SparseCaptureError> {
        if self.next_call <= self.decode_calls {
            return Err(Base0SparseCaptureError::CaptureIncomplete { pushed: self.cursor, expected: self.acc.progress().1 });
        }
        self.acc.finish()
    }
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
        // **The CARRIER, not the close budget** (ADR-0080 design A). A close is a chunk group of
        // up to `DEFAULT_MAX_CLOSE_CHUNKS` parts now, so `DEFAULT_MAX_CLOSE_BYTES` is 2,250,000
        // and comparing one opening against it would make this assertion true of both maps. The
        // fact v2 exists for is about ONE transaction: v1's opening does not fit one and v2's
        // does, so a v1 court pays extra carriers for thirty-one heads it will not read.
        let carrier = kaspa_consensus_core::palw_state_v2::PALW_OBJECT_CHUNK_MAX_BYTES as u64;
        let budget = kaspa_consensus_core::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES;

        let delta = v as u64 * v1.delta_row_bytes as u64;
        assert_eq!(delta, 65_536);
        assert_eq!(kernel as u64 * v1.conv_row_bytes as u64, 196_608, "v1's window spans every head");
        assert_eq!(kernel as u64 * v2.conv_head_row_bytes as u64, 6_144, "v2's window is one head's");
        assert!(delta + kernel as u64 * v1.conv_row_bytes as u64 > carrier, "v1 fits one carrier after all");
        assert!(delta + kernel as u64 * v2.conv_head_row_bytes as u64 <= carrier, "v2 does not fit one carrier");
        // And both are inside the GROUP, which is what says the comparison above is about the part.
        assert!(delta + kernel as u64 * v1.conv_row_bytes as u64 <= budget, "even v1's opening is inside the close budget now");
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

    /// **The retained level is the ladder's, and the budget is what derives it** (ADR-0082
    /// Decision 7's `retain_level = ⌈log₂ leaves⌉ − 20`).
    ///
    /// Two things are pinned, and the shipped constant is the SECOND of them rather than an
    /// independent number: that the level the derivation returns is the smallest one whose
    /// retained vector fits [`PALW_BASE0_SPARSE_RETAIN_BUDGET_BYTES`] — one lower would exceed it
    /// — and that at the `2^32` ladder ADR-0077 Decision 12 moves to, that level is
    /// [`PALW_BASE0_SPARSE_RETAIN_LEVEL_V1`].
    #[test]
    fn the_retained_level_is_the_ladders_and_the_budget_is_what_bounds_it() {
        let node_bytes = 64u64;
        for ladder_log2 in 10..=48u32 {
            let ladder = 1u64 << ladder_log2;
            let level = palw_base0_sparse_retain_level_for_ladder_v1(ladder);
            let retained = |level: u32| -> u64 { ladder.div_ceil(1u64 << level) * node_bytes };
            assert!(
                retained(level) <= PALW_BASE0_SPARSE_RETAIN_BUDGET_BYTES,
                "a 2^{ladder_log2} ladder retains {} bytes at level {level}, past the budget",
                retained(level)
            );
            if level > 0 {
                assert!(
                    retained(level - 1) > PALW_BASE0_SPARSE_RETAIN_BUDGET_BYTES,
                    "level {level} is not the smallest that fits at 2^{ladder_log2}"
                );
            }
        }
        assert_eq!(
            palw_base0_sparse_retain_level_for_ladder_v1(1 << 32),
            PALW_BASE0_SPARSE_RETAIN_LEVEL_V1,
            "the shipped constant is what the 2^32 ladder derives to — 2^20 retained nodes, 64 MiB"
        );
        // And the level a capture actually folds at never goes below it, because an opening
        // re-derives a whole CALL and every level under 2^12 is already inside one.
        assert_eq!(palw_base0_sparse_retain_level_v1(PALW_STEP_LEG_MAX_LEAVES), PALW_BASE0_SPARSE_RETAIN_LEVEL_V1);
        assert_eq!(palw_base0_sparse_retain_level_v1(1 << 40), palw_base0_sparse_retain_level_for_ladder_v1(1 << 40));
    }

    // =========================================================================================
    // U-01 — the capture, measured against the forward (ADR-0082 Decision 7)
    // =========================================================================================

    /// The largest `exact_decode_tokens` this profile's job of `prefill` tokens fits under `cap`.
    /// The width the registered row ALLOWS, derived rather than typed: the same
    /// `step_leaf_count_capped_v1` the executor prices with.
    fn u01_decode_budget(profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3, prefill: u32, cap: u64) -> (u32, u64) {
        let price = |decode: u32| -> Option<u64> {
            let ctx = kaspa_consensus_core::palw_base0_profile::rc_job_context(profile, prefill, decode);
            kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(profile, &ctx, cap).ok()
        };
        let mut best = (0u32, 0u64);
        let mut decode = 1u32;
        while let Some(leaves) = price(decode) {
            best = (decode, leaves);
            decode += 1;
            if decode > 100_000 {
                break;
            }
        }
        best
    }

    /// Peak RSS of this process while a phase runs, in bytes — sampled with `ps`, which is the
    /// one number available on this host without a new dependency. `0` when `ps` cannot answer.
    struct U01Rss {
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
        peak: std::sync::Arc<std::sync::atomic::AtomicU64>,
        handle: Option<std::thread::JoinHandle<()>>,
    }

    impl U01Rss {
        fn start() -> Self {
            let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let peak = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
            let (s, p) = (stop.clone(), peak.clone());
            let pid = std::process::id().to_string();
            let handle = std::thread::spawn(move || {
                while !s.load(std::sync::atomic::Ordering::Relaxed) {
                    if let Ok(out) = std::process::Command::new("ps").args(["-o", "rss=", "-p", &pid]).output() {
                        if let Ok(kb) = String::from_utf8_lossy(&out.stdout).trim().parse::<u64>() {
                            p.fetch_max(kb * 1024, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            });
            Self { stop, peak, handle: Some(handle) }
        }

        fn finish(mut self) -> u64 {
            self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }
            self.peak.load(std::sync::atomic::Ordering::Relaxed)
        }
    }

    /// **U-01: what the capture costs, against what the forward costs** (ADR-0082 §1.5, Decision 7).
    ///
    /// Off unless `MISAKA_PALW_U01_ARTIFACT` names a converted dense artifact: 1.7 GiB of weights
    /// and a minute of arithmetic are not a unit test's inputs, and a measurement that ran on
    /// derived toy weights would be a number about nothing. Run it as
    ///
    /// ```text
    /// MISAKA_PALW_U01_ARTIFACT=/path/qwen25-1.5b-a16.palwart \
    ///   cargo test --release -p misaka-palw-base0 --lib -- u01_the_capture --nocapture
    /// ```
    ///
    /// The job is the §1.5 one: the registered dense row, a 26-token prefill and the decode the
    /// ruleset's ladder allows. Three phases, one process, one artifact: the engine's own forward
    /// with nothing captured, then the shipped capture path, then whatever the executor's capture
    /// costs after Decision 7.
    #[test]
    fn u01_the_capture_is_priced_against_the_forward() {
        use kaspa_consensus_core::palw_freeprompt_v3::{
            PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION, PalwFreePromptJobV3,
        };
        use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
        use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

        let Ok(path) = std::env::var("MISAKA_PALW_U01_ARTIFACT") else {
            eprintln!("U-01: skipped — set MISAKA_PALW_U01_ARTIFACT to the dense .palwart to measure");
            return;
        };
        let prefill: u32 = std::env::var("MISAKA_PALW_U01_PREFILL").ok().and_then(|v| v.parse().ok()).unwrap_or(26);

        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let artifact = crate::artifact::decode_artifact_file_v1(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
        drop(bytes);
        let court = PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2).expect("shipped court");
        let entry = crate::classes::canonical_class_by_model_id_v1(&court, "Qwen/Qwen2.5-1.5B/graph-v2")
            .expect("this build's catalog has the dense row");
        let profile = entry.profile.clone();
        let (decode, leaves) = u01_decode_budget(&profile, prefill, court.max_step_leaf_count());
        assert!(decode >= 2, "a {prefill}-token prefill leaves no decode budget under this ladder");
        let positions = prefill as u64 + decode as u64 - 1;
        eprintln!(
            "U-01 job: prefill {prefill}, decode {decode}, {leaves} leaves of {} ladder, {positions} forward calls",
            court.max_step_leaf_count()
        );

        // Deterministic in-vocabulary ids. The measurement is of arithmetic whose cost does not
        // depend on WHICH ids they are, and a tokenizer here would only add a file to the recipe.
        let vocab = artifact.shape.vocab;
        let prompt: Vec<usize> = (0..prefill as usize).map(|i| (i * 7919 + 1013) % vocab).collect();

        // ---- phase A: the engine's forward, nothing captured -------------------------------
        let rss = U01Rss::start();
        let started = std::time::Instant::now();
        {
            let engine = crate::engine_a16::A16Engine::new(&artifact).expect("the artifact is an A16 class");
            let mut cache = crate::engine_a16::A16Cache::new(artifact.shape.n_layers);
            let mut last = Vec::new();
            for (position, token) in prompt.iter().enumerate() {
                last = engine.forward_token(&mut cache, *token, position).expect("a prefill position runs");
            }
            let mut next = kaspa_consensus_core::palw_step_refute::base0_decode_token_select_v1(&last);
            for call in 1..decode as usize {
                let logits = engine.forward_token(&mut cache, next, prefill as usize + call - 1).expect("a decode call runs");
                next = kaspa_consensus_core::palw_step_refute::base0_decode_token_select_v1(&logits);
            }
        }
        let forward = started.elapsed();
        let forward_rss = rss.finish();

        // ---- phase B: the DENSE capture — every tile of every node of every position ---------
        //
        // The shipped path until ADR-0082 Decision 7, and the thing §1.5's 94x was measured on.
        // Skippable (`MISAKA_PALW_U01_SKIP_DENSE=1`) because it is also the thing that does not
        // fit in memory at a real context, which is the other half of what is being measured.
        let ctx = {
            let mut ctx = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, prefill, decode);
            ctx.job_id = Hash64::from_u64_word(0x0082_0001);
            ctx.prompt_token_ids_hash =
                kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(&prompt.iter().map(|t| *t as u32).collect::<Vec<_>>());
            ctx
        };
        let cap = court.max_step_leaf_count();
        let dense = if std::env::var("MISAKA_PALW_U01_SKIP_DENSE").is_ok() {
            None
        } else {
            let rss = U01Rss::start();
            let started = std::time::Instant::now();
            let run = crate::qwen25_a16_backend::a16_execute_for_attempt_streaming_capped_v1(
                &artifact, &profile, None, &ctx, &prompt, cap, &mut |_| {},
            )
            .expect("the dense capture runs this job");
            let took = started.elapsed();
            let bytes = crate::produce::base0_material_encode_v1(&run).expect("the dense retention encodes").len();
            Some((took, rss.finish(), bytes))
        };

        // ---- phase C: the FOLD — Decision 7's capture -----------------------------------------
        let rss = U01Rss::start();
        let started = std::time::Instant::now();
        let folded = crate::qwen25_a16_backend::a16_execute_free_prompt_streaming_v1(
            &artifact, &profile, None, &ctx, &prompt, cap, &mut |_| {},
        )
        .expect("the folded capture runs this job");
        let fold_took = started.elapsed();
        let fold_rss = rss.finish();
        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        let fold_bytes = crate::produce::base0_fp_material_encode_v2(&folded, &ids).expect("the fold retains").len();
        let tree = folded.step_tree.as_ref().expect("a folded run keeps its tree");
        if let Some((_, _, _)) = &dense {
            // The measurement is only a measurement if both phases committed the same thing.
            assert_eq!(tree.root().expect("the tree is its own shape"), folded.binding.step_merkle_root);
        }

        let per = |d: std::time::Duration| d.as_secs_f64() / decode as f64;
        let per_position = |d: std::time::Duration| d.as_secs_f64() / positions as f64;
        let line = |what: &str, took: std::time::Duration, rss: u64, bytes: Option<usize>| {
            eprintln!(
                "U-01 {what:<9}: {:>8.3} s total, {:>7.4} s/token, {:>7.4} s/position, peak RSS {:>5.2} GiB{}",
                took.as_secs_f64(),
                per(took),
                per_position(took),
                rss as f64 / (1 << 30) as f64,
                match bytes {
                    Some(b) => format!(", retention {b} bytes ({:.2} MB/position)", b as f64 / positions as f64 / 1e6),
                    None => String::new(),
                }
            );
        };
        line("forward", forward, forward_rss, None);
        if let Some((took, rss, bytes)) = &dense {
            line("captured", *took, *rss, Some(*bytes));
            eprintln!("U-01 captured/forward : {:.1}x", took.as_secs_f64() / forward.as_secs_f64().max(f64::MIN_POSITIVE));
        }
        line("folded", fold_took, fold_rss, Some(fold_bytes));
        eprintln!("U-01 folded/forward   : {:.1}x", fold_took.as_secs_f64() / forward.as_secs_f64().max(f64::MIN_POSITIVE));
        if let Some((took, _, bytes)) = &dense {
            eprintln!(
                "U-01 fold vs dense    : {:.1}x faster, {:.1}x smaller retention",
                took.as_secs_f64() / fold_took.as_secs_f64().max(f64::MIN_POSITIVE),
                *bytes as f64 / fold_bytes.max(1) as f64
            );
        }
        eprintln!(
            "U-01 retained tree    : {} nodes at level {} ({} bytes)",
            tree.retained_nodes().len(),
            tree.retain_level(),
            tree.retained_nodes().len() * 64
        );
        eprintln!("U-01 step leaves: {leaves}, {} per position", leaves / positions.max(1));
    }
}
