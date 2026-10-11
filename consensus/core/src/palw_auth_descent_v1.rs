//! **Lane G14-V: authenticated subtree descent over committed step-discipline trees** (node policy; consensus-inert — nothing here
//! is read by the fold, no wire type is defined, no rule changes).
//!
//! **What it closes.** The legacy public filer's codec-independent localizer read consecutive `StepRange` units (1,024 leaves each),
//! because a matching midpoint leaf authenticates nothing about the prefix it skipped (a producer can change one tile and leave every
//! later tile honest — codex's `a4a62ec21`). That scan is linear: `⌈n / 1024⌉ + 2` reserved sessions, so every claim past 32,768
//! leaves was `Unjudged` under the 34-session reservation. LG14-B's tag-157/158 descent localizes in `⌈height / 9⌉` rounds, but
//! only where its fence is armed AND the verifier's replay is a base0-codec fold; the step tree was also the only tree any localizer
//! walked, so a claim whose step tree agrees and whose logits trace or checkpoint leg lies was a dead end ("checkpoint/trace
//! localization is required").
//!
//! **The rule here.** Every authenticated opening of a committed tree — a `StepRange` answer (leaf hashes + range siblings), a
//! single-leaf opening (a tiled logits row in its rows tree, a tile in its row, a checkpoint anchor) — determines, besides its own
//! leaves, every node on its path to the root and every SIBLING it carries, each at a known `(level, index)`. All of them are
//! authenticated by the fold to the committed root (the opening is refused unless it does). The descent compares only such
//! authenticated SUBTREE ROOTS with the verifier's own nodes, from the root down, always to the leftmost child that differs:
//!
//! * a matching subtree root proves (collision resistance) every leaf under it is the verifier's own — the prefix is covered by the
//!   left siblings' roots, never by a lone leaf;
//! * the first divergent leaf is therefore the first leaf at which the claim's tree leaves the verifier's honest run;
//! * where the focus node's children are not yet known, the next read is the range at the focus subtree's LEFT EDGE: its path runs
//!   down the focus's left spine and carries every right sibling, so after it both children of every spine node are known.
//!
//! **Bound.** With reads of at most `2^r` leaves (`r = 10` for `StepRange`, `r = 0` for single-leaf openings) and a tree of height
//! `h`, the focus level strictly drops between reads: at most `h − r + 1` reads (`r > 0`, `h > r`), `h` reads (`r = 0`), 1 read
//! (`h ≤ r`) — [`palw_auth_descent_max_reads_v1`]. For the step tree's `StepRange` reads at the 2^40 ladder top that is 31 reads,
//! plus the binding read and the terminal: 33 of the reservation's 34 sessions.
//!
//! **Liveness.** An own node that is unavailable, or an own tree whose children both agree under a node that does not
//! ([`PalwAuthDescentV1::OwnInconsistent`]), yields no filing: the verifier's replay is at fault or a hash collided, never the
//! producer by assumption.

use crate::palw_step_leg::{PalwStepOpeningV1, PalwStepRangeOpeningV1, step_merkle_leaf_v1, step_merkle_node_v1};
use kaspa_hashes::Hash64;
use std::collections::BTreeMap;

/// The tree's height: the number of folds from `leaf_count` leaves to one (the step discipline's promotion keeps an odd tail).
pub fn palw_auth_tree_height_v1(leaf_count: u64) -> u8 {
    let mut level = 0u8;
    let mut width = leaf_count.max(1);
    while width > 1 {
        width = width.div_ceil(2);
        level += 1;
    }
    level
}

/// The number of nodes at `level` of a tree over `leaf_count` leaves.
pub fn palw_auth_tree_width_v1(leaf_count: u64, level: u8) -> u64 {
    if level >= 64 {
        return u64::from(leaf_count > 0);
    }
    leaf_count.div_ceil(1u64 << level)
}

/// The leaves `[first, end)` node `(level, index)` covers.
pub fn palw_auth_tree_span_v1(leaf_count: u64, level: u8, index: u64) -> Option<(u64, u64)> {
    let first = index.checked_shl(u32::from(level)).filter(|f| *f < leaf_count || (leaf_count == 0 && *f == 0))?;
    let end = index.checked_add(1).and_then(|i| i.checked_shl(u32::from(level))).unwrap_or(u64::MAX).min(leaf_count);
    (first < end).then_some((first, end))
}

/// **The authenticated nodes of one committed tree**: the root, and every node an accepted opening determined.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwAuthTreeV1 {
    pub leaf_count: u64,
    pub root: Hash64,
    nodes: BTreeMap<(u8, u64), Hash64>,
}

impl PalwAuthTreeV1 {
    /// A tree of `leaf_count ≥ 1` leaves committed to `root`.
    pub fn new(leaf_count: u64, root: Hash64) -> Option<Self> {
        if leaf_count == 0 {
            return None;
        }
        let mut nodes = BTreeMap::new();
        nodes.insert((palw_auth_tree_height_v1(leaf_count), 0), root);
        Some(Self { leaf_count, root, nodes })
    }

    pub fn height(&self) -> u8 {
        palw_auth_tree_height_v1(self.leaf_count)
    }

    /// The claim's node `(level, index)`, where an authenticated opening reached it.
    pub fn node(&self, level: u8, index: u64) -> Option<Hash64> {
        self.nodes.get(&(level, index)).copied()
    }

    /// How many authenticated nodes the case holds (the resource report; bounded by `reads · (2^r + 2h)`).
    pub fn known(&self) -> usize {
        self.nodes.len()
    }

    /// **Learn one authenticated range opening** (`StepRange`'s answer, or one leaf as a range of one): fold it exactly as
    /// `step_range_opening_root_capped_v1` does, recording every node it forms and every sibling it consumes at its position.
    /// Nothing is recorded unless the fold reaches this tree's root — an opening of another commitment teaches nothing.
    pub fn learn_range(&mut self, opening: &PalwStepRangeOpeningV1) -> Result<usize, String> {
        let n = self.leaf_count;
        let k = opening.leaf_hashes.len() as u64;
        let first = opening.first_leaf_index;
        if k == 0 || first.checked_add(k).is_none_or(|end| end > n) {
            return Err(format!("a range of {k} leaves at {first} is not inside {n} leaves"));
        }
        let mut learned: Vec<((u8, u64), Hash64)> = Vec::with_capacity(2 * opening.leaf_hashes.len() + 2 * 64);
        let mut nodes: Vec<Hash64> =
            opening.leaf_hashes.iter().enumerate().map(|(i, leaf)| step_merkle_leaf_v1(first + i as u64, leaf)).collect();
        for (i, node) in nodes.iter().enumerate() {
            learned.push(((0, first + i as u64), *node));
        }
        let (mut a, mut b, mut width, mut level) = (first, first + k, n, 0u8);
        let mut siblings = opening.siblings.iter();
        while width > 1 {
            let mut next = Vec::with_capacity(nodes.len() / 2 + 2);
            let mut i = 0usize;
            if a % 2 == 1 {
                let left = *siblings.next().ok_or("the range opening's path is too short")?;
                learned.push(((level, a - 1), left));
                next.push(step_merkle_node_v1(&left, &nodes[0]));
                i = 1;
            }
            while i + 1 < nodes.len() {
                next.push(step_merkle_node_v1(&nodes[i], &nodes[i + 1]));
                i += 2;
            }
            if i < nodes.len() {
                if width % 2 == 1 && b == width {
                    next.push(nodes[i]);
                } else {
                    let right = *siblings.next().ok_or("the range opening's path is too short")?;
                    learned.push(((level, b), right));
                    next.push(step_merkle_node_v1(&nodes[i], &right));
                }
            }
            a /= 2;
            b = b.div_ceil(2);
            width = width.div_ceil(2);
            level += 1;
            for (j, node) in next.iter().enumerate() {
                learned.push(((level, a + j as u64), *node));
            }
            nodes = next;
        }
        if siblings.next().is_some() {
            return Err("the range opening's path is too long".into());
        }
        if nodes.len() != 1 || nodes[0] != self.root {
            return Err("the range opening does not reach the committed root".into());
        }
        let before = self.nodes.len();
        for (at, node) in learned {
            self.nodes.insert(at, node);
        }
        Ok(self.nodes.len() - before)
    }

    /// **Learn one authenticated single-leaf opening** (a tiled logits row in its rows tree, a tile in its row, a checkpoint leaf):
    /// a range of one, whose siblings come in the same order.
    pub fn learn_leaf(&mut self, opening: &PalwStepOpeningV1) -> Result<usize, String> {
        self.learn_range(&PalwStepRangeOpeningV1 {
            first_leaf_index: opening.leaf_index,
            leaf_hashes: vec![opening.leaf_hash],
            siblings: opening.siblings.clone(),
        })
    }
}

/// **What the descent asks next.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwAuthDescentV1 {
    /// The committed root is the verifier's own: nothing to localize in this tree.
    Agrees,
    /// Read (demand) the authenticated opening of leaves `[first, first + count)` next.
    Read { first: u64, count: u64 },
    /// The first leaf (left to right) whose committed node is not the verifier's own; every leaf before it is the verifier's own.
    FirstDivergentLeaf(u64),
    /// The committed node differs and both its authenticated children are the verifier's own: the verifier's own tree is
    /// inconsistent (or a hash collided) — file nothing.
    OwnInconsistent { level: u8, index: u64 },
    /// The verifier cannot produce its own node here — file nothing.
    OwnUnavailable { level: u8, index: u64 },
}

/// **The descent's next move**, from the authenticated nodes alone: `own(level, index)` is the verifier's own node at the same
/// position (leaf nodes are `step_merkle_leaf_v1(index, own leaf hash)`), `max_read` the widest read one session discloses (a power
/// of two: 1,024 for `StepRange`, 1 for single-leaf openings).
pub fn palw_auth_descent_next_v1(
    tree: &PalwAuthTreeV1,
    own: &dyn Fn(u8, u64) -> Option<Hash64>,
    max_read: u64,
) -> PalwAuthDescentV1 {
    debug_assert!(max_read.is_power_of_two());
    let n = tree.leaf_count;
    let height = tree.height();
    let Some(own_root) = own(height, 0) else { return PalwAuthDescentV1::OwnUnavailable { level: height, index: 0 } };
    if own_root == tree.root {
        return PalwAuthDescentV1::Agrees;
    }
    let (mut level, mut index) = (height, 0u64);
    loop {
        if level == 0 {
            return PalwAuthDescentV1::FirstDivergentLeaf(index);
        }
        let child_width = palw_auth_tree_width_v1(n, level - 1);
        let (left, right) = (2 * index, 2 * index + 1);
        if right >= child_width {
            // The level's odd tail is promoted: one child, the same node.
            (level, index) = (level - 1, left);
            continue;
        }
        match (tree.node(level - 1, left), tree.node(level - 1, right)) {
            (Some(committed_left), Some(committed_right)) => {
                let Some(own_left) = own(level - 1, left) else {
                    return PalwAuthDescentV1::OwnUnavailable { level: level - 1, index: left };
                };
                if own_left != committed_left {
                    (level, index) = (level - 1, left);
                    continue;
                }
                let Some(own_right) = own(level - 1, right) else {
                    return PalwAuthDescentV1::OwnUnavailable { level: level - 1, index: right };
                };
                if own_right != committed_right {
                    (level, index) = (level - 1, right);
                    continue;
                }
                return PalwAuthDescentV1::OwnInconsistent { level, index };
            }
            _ => {
                let (first, end) = palw_auth_tree_span_v1(n, level, index).expect("a node on the descent covers leaves");
                return PalwAuthDescentV1::Read { first, count: (end - first).min(max_read) };
            }
        }
    }
}

/// **The worst-case reads of one descent** over `leaf_count` leaves with reads of `max_read` leaves (module doc's bound).
pub fn palw_auth_descent_max_reads_v1(leaf_count: u64, max_read: u64) -> u64 {
    let h = u64::from(palw_auth_tree_height_v1(leaf_count));
    let r = u64::from(max_read.max(1).trailing_zeros());
    if h == 0 {
        0
    } else if h <= r {
        1
    } else if r == 0 {
        h
    } else {
        h - r + 1
    }
}

/// **The verifier's own tree over its own leaf hashes**, without holding every leaf: the nodes at `retain` and above are kept
/// (`⌈n / 2^retain⌉` of them); a node below is folded from `own_range` over its ≤ `2^retain` leaves when the descent asks for it.
/// `own_range(first, count)` returns the verifier's OWN leaf hashes (its replay), never the producer's.
pub struct PalwOwnLeafTreeV1 {
    pub leaf_count: u64,
    pub retain: u8,
    /// Levels `retain ..= height`, index 0 the retained level.
    upper: Vec<Vec<Hash64>>,
}

/// The widest `own_range` one read of a node below the retained level asks for.
pub const PALW_OWN_LEAF_TREE_CHUNK_V1: u32 = 1_024;
/// The leaves one streaming read of the builder covers (aligned to the retained level).
pub const PALW_OWN_LEAF_TREE_STREAM_V1: u64 = 1 << 20;

impl PalwOwnLeafTreeV1 {
    /// The retained level for `leaf_count` leaves: 1,024-leaf blocks, raised so at most 2^20 nodes are retained.
    pub fn retain_level_v1(leaf_count: u64) -> u8 {
        palw_auth_tree_height_v1(leaf_count).saturating_sub(20).max(10)
    }

    /// Build by streaming `own_range` over every leaf once.
    pub fn build(
        leaf_count: u64,
        retain: u8,
        own_range: &dyn Fn(u64, u32) -> Result<Vec<Hash64>, String>,
    ) -> Result<Self, String> {
        if leaf_count == 0 {
            return Err("an empty tree".into());
        }
        let retain = retain.min(palw_auth_tree_height_v1(leaf_count));
        let width = palw_auth_tree_width_v1(leaf_count, retain);
        let mut retained = Vec::with_capacity(usize::try_from(width).map_err(|_| "too many retained nodes")?);
        // Stream the leaves in chunks aligned to the retained level (one own read a chunk, never one a retained node).
        let chunk = (1u64 << retain).max(PALW_OWN_LEAF_TREE_STREAM_V1);
        let mut at = 0u64;
        while at < leaf_count {
            let end = at.saturating_add(chunk).min(leaf_count);
            retained.extend(Self::fold_chunk(leaf_count, retain, at, end, own_range)?);
            at = end;
        }
        if retained.len() as u64 != width {
            return Err("the retained level is not the tree's width".into());
        }
        let mut upper = vec![retained];
        while upper.last().is_some_and(|l| l.len() > 1) {
            let level = upper.last().expect("a level");
            let mut next = Vec::with_capacity(level.len().div_ceil(2));
            let mut pairs = level.chunks_exact(2);
            for pair in &mut pairs {
                next.push(step_merkle_node_v1(&pair[0], &pair[1]));
            }
            if let [odd] = pairs.remainder() {
                next.push(*odd);
            }
            upper.push(next);
        }
        Ok(Self { leaf_count, retain, upper })
    }

    /// Node `(level, index)` of a span folded from the leaves.
    fn fold_span(
        leaf_count: u64,
        level: u8,
        index: u64,
        own_range: &dyn Fn(u64, u32) -> Result<Vec<Hash64>, String>,
    ) -> Result<Hash64, String> {
        let (first, end) = palw_auth_tree_span_v1(leaf_count, level, index).ok_or("no such node")?;
        let nodes = Self::fold_chunk(leaf_count, level, first, end, own_range)?;
        (nodes.len() == 1).then(|| nodes[0]).ok_or_else(|| "the span did not fold to one node".into())
    }

    /// The nodes at `level` over leaves `[first, end)`, `first` aligned to `2^level` (the global odd tail promoted).
    fn fold_chunk(
        leaf_count: u64,
        level: u8,
        first: u64,
        end: u64,
        own_range: &dyn Fn(u64, u32) -> Result<Vec<Hash64>, String>,
    ) -> Result<Vec<Hash64>, String> {
        let mut nodes = Vec::with_capacity(usize::try_from(end - first).unwrap_or(0));
        let mut at = first;
        while at < end {
            let count = (end - at).min(PALW_OWN_LEAF_TREE_STREAM_V1) as u32;
            let hashes = own_range(at, count)?;
            if hashes.len() != count as usize {
                return Err(format!("the own run returned {} of {count} leaf hashes", hashes.len()));
            }
            nodes.extend(hashes.iter().enumerate().map(|(i, h)| step_merkle_leaf_v1(at + i as u64, h)));
            at += u64::from(count);
        }
        // Fold within the span, promoting an odd tail only where the GLOBAL level's width is odd at its end.
        let mut start = first;
        for l in 0..level {
            let width = palw_auth_tree_width_v1(leaf_count, l);
            let mut next = Vec::with_capacity(nodes.len().div_ceil(2));
            let mut k = 0usize;
            while k < nodes.len() {
                let position = start + k as u64;
                if position + 1 < width && k + 1 < nodes.len() {
                    next.push(step_merkle_node_v1(&nodes[k], &nodes[k + 1]));
                    k += 2;
                } else {
                    next.push(nodes[k]);
                    k += 1;
                }
            }
            nodes = next;
            start /= 2;
        }
        Ok(nodes)
    }

    /// The verifier's own node `(level, index)`.
    pub fn node(&self, level: u8, index: u64, own_range: &dyn Fn(u64, u32) -> Result<Vec<Hash64>, String>) -> Option<Hash64> {
        if index >= palw_auth_tree_width_v1(self.leaf_count, level) || level > palw_auth_tree_height_v1(self.leaf_count) {
            return None;
        }
        if level >= self.retain {
            return self.upper.get(usize::from(level - self.retain))?.get(usize::try_from(index).ok()?).copied();
        }
        Self::fold_span(self.leaf_count, level, index, own_range).ok()
    }

    pub fn root(&self) -> Hash64 {
        *self.upper.last().and_then(|l| l.first()).expect("a built tree has a root")
    }
}

// ---- past an agreeing step tree: the logits trace (lane G14-V) ---------------------------------------------------------------

/// **What the public trace localization does next** once the claim's step tree is the verifier's own and its committed logits
/// trace is not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwTraceLocalizeV1 {
    /// Read (demand through a reserved DA `Event` session) the tiled event `(row, tile)` next.
    Read { row: u32, tile: u8 },
    /// A complete kind-4 contradiction from public, authenticated material and the verifier's own run, already held to the
    /// fold's own predicate: `LogitsNotStepOutput` (12) for a committed logits lane the (agreeing) step tree's head did not
    /// output, `ForgedOutput` (8) for a flat class's committed token that is not its (agreeing) row's selection.
    Proof(crate::palw_offence_v1::PalwPanelContradictionV1),
}

/// **The authenticated tiled events a trace localization has read** — each checked against the claim's trace and execution roots
/// before it is kept; the rows tree and a row's tile tree are rebuilt from their openings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwTraceDescentV1 {
    pub events: BTreeMap<(u32, u8), crate::palw_step_refute::PalwTraceEventDisclosureV1>,
}

/// The widest tiled descent a verifier reads past the binding read: the rows tree, one row's tile tree (≤ 256 tiles), and the
/// located tile's own event.
pub fn palw_trace_descent_max_reads_v1(decode_rows: u64, tiles: u64) -> u64 {
    palw_auth_descent_max_reads_v1(decode_rows, 1) + palw_auth_descent_max_reads_v1(tiles, 1) + 2
}

impl PalwTraceDescentV1 {
    /// Learn one public event, authenticated against the claim's own roots (the fold's disclosure check) — or nothing.
    pub fn learn(
        &mut self,
        claim_trace_root: Hash64,
        claim_execution_root: Hash64,
        row: u32,
        tile: u8,
        event: &crate::palw_step_refute::PalwTraceEventDisclosureV1,
        ladder: u64,
    ) -> Result<(), String> {
        crate::palw_step_refute::check_trace_event_disclosure_v1(claim_trace_root, claim_execution_root, row, tile, event, ladder)
            .map_err(|e| format!("the public event does not authenticate: {e}"))?;
        if !matches!(event, crate::palw_step_refute::PalwTraceEventDisclosureV1::Tiled { .. }) {
            return Err("only a tiled event is read past the binding".into());
        }
        self.events.insert((row, tile), event.clone());
        Ok(())
    }
}

/// **The trace localization's next move** (lane G14-V). Preconditions the caller established: `binding` is the claim's
/// (authenticated by the binding read), and the verifier's OWN run commits the same step tree (so its head openings authenticate
/// against the claim's step root) and another logits trace root.
///
/// * **flat** — the binding read's event already carries every committed row and id: the first lane that is not the verifier's
///   own is located with no further session, then `LogitsNotStepOutput`; rows that all agree with ids that do not are
///   `ForgedOutput`.
/// * **tiled** — a descent over the rows tree (single-row openings; a row's root is authenticated by its opening) to the first
///   divergent row, then over that row's tile tree to the first divergent tile; each read is one `Event` session, at most
///   [`palw_trace_descent_max_reads_v1`]. The bent lane's event is the proof's.
///
/// Nothing is returned that the fold's own predicate does not convict.
#[allow(clippy::too_many_arguments)]
pub fn palw_trace_localize_next_v1(
    target: &crate::palw_offence_attribution_v1::PalwOffenceTargetV1,
    binding: &crate::palw_step_leg::PalwStepBindingV2,
    claim_event0: &crate::palw_step_refute::PalwTraceEventDisclosureV1,
    read: &PalwTraceDescentV1,
    own_event: &dyn Fn(u32, u8) -> Result<crate::palw_step_refute::PalwTraceEventDisclosureV1, String>,
    own_head_opening: &dyn Fn(u64) -> Result<PalwStepOpeningV1, String>,
    ladder: u64,
) -> Result<PalwTraceLocalizeV1, String> {
    use crate::palw_offence_v1::PalwPanelContradictionV1 as C;
    use crate::palw_step_refute::{PALW_LOGITS_TILE_LANES, PalwTraceEventDisclosureV1 as Ev};
    if claim_event0.binding() != binding || binding.committed_execution_root != target.execution_root {
        return Err("the binding read is not this claim's".into());
    }
    let (profile, ctx) = (&binding.shape_profile, &binding.job_context);
    let rows = u64::from(ctx.exact_decode_tokens);
    let vocab = profile.vocab_size as usize;
    // The bent (row, lane) and the public event that carries it.
    let (row, lane, event) = match claim_event0 {
        Ev::Flat { pin, .. } => {
            let Ev::Flat { pin: own_pin, .. } = own_event(0, 0)? else { return Err("the own run is not a flat class".into()) };
            if pin.logits_rows.len() != own_pin.logits_rows.len() {
                return Err("the flat pin's row count is not the verifier's own".into());
            }
            let bent = pin.logits_rows.iter().zip(own_pin.logits_rows.iter()).enumerate().find_map(|(r, (claim_row, own_row))| {
                claim_row.iter().zip(own_row.iter()).position(|(a, b)| a != b).map(|lane| (r as u32, lane))
            });
            let Some((row, lane)) = bent else {
                let position = pin
                    .generated_token_ids
                    .iter()
                    .zip(own_pin.generated_token_ids.iter())
                    .position(|(a, b)| a != b)
                    .ok_or("the flat rows and ids are the verifier's own: the trace root differs elsewhere")?;
                let contradiction = C::ForgedOutput { binding: binding.clone(), pin: pin.clone(), position: position as u32 };
                crate::palw_offence_attribution_v1::palw_false_valid_convicts_execution_v2(
                    &contradiction,
                    None,
                    target.execution_root,
                    target.artifact_root,
                    ladder,
                )
                .map_err(|e| format!("the forged token does not convict: {e:?}"))?;
                return Ok(PalwTraceLocalizeV1::Proof(contradiction));
            };
            (row, lane, claim_event0.clone())
        }
        Ev::Tiled { .. } => {
            let tiles = vocab.div_ceil(PALW_LOGITS_TILE_LANES).max(1) as u64;
            type Own = (PalwStepOpeningV1, PalwStepOpeningV1, Vec<i32>);
            let tiled = |r: u32, t: u8| -> Result<Own, String> {
                match own_event(r, t)? {
                    Ev::Tiled { row_opening, tile_opening, tile_lanes, .. } => Ok((row_opening, tile_opening, tile_lanes)),
                    _ => Err("the own run is not a tiled class".into()),
                }
            };
            let Ev::Tiled { row_opening: first_row, .. } = claim_event0 else { unreachable!("tiled above") };
            let rows_root = crate::palw_step_leg::step_opening_root_capped_v1(rows, first_row, ladder)
                .map_err(|e| format!("the binding read's row opening: {e:?}"))?;
            let public = || std::iter::once(((0u32, 0u8), claim_event0)).chain(read.events.iter().map(|(k, e)| (*k, e)));
            // The rows tree: every authenticated row opening read so far.
            let mut rows_tree = PalwAuthTreeV1::new(rows, rows_root).ok_or("a run with no rows")?;
            for (_, event) in public() {
                if let Ev::Tiled { row_opening, .. } = event {
                    rows_tree.learn_leaf(row_opening)?;
                }
            }
            let own_rows: Vec<Hash64> =
                (0..rows as u32).map(|r| tiled(r, 0).map(|(o, ..)| o.leaf_hash)).collect::<Result<_, String>>()?;
            let rows_range = |first: u64, count: u32| Ok(own_rows[first as usize..(first + u64::from(count)) as usize].to_vec());
            let own_rows_tree = PalwOwnLeafTreeV1::build(rows, 0, &rows_range)?;
            let row = match palw_auth_descent_next_v1(&rows_tree, &|l, i| own_rows_tree.node(l, i, &rows_range), 1) {
                PalwAuthDescentV1::Read { first, .. } => return Ok(PalwTraceLocalizeV1::Read { row: first as u32, tile: 0 }),
                PalwAuthDescentV1::FirstDivergentLeaf(row) => row as u32,
                PalwAuthDescentV1::Agrees => {
                    return Err("every committed row is the verifier's own: the ids differ (ForgedOutputTiled is not wired)".into());
                }
                other => return Err(format!("the own rows tree cannot judge: {other:?}")),
            };
            // The row's committed root comes from an opening OF that row; a sibling node is not its root's preimage.
            let row_events: BTreeMap<u8, &Ev> = public().filter(|((r, _), _)| *r == row).map(|((_, t), e)| (t, e)).collect();
            let Some(Ev::Tiled { row_opening, .. }) = row_events.values().next().copied() else {
                return Ok(PalwTraceLocalizeV1::Read { row, tile: 0 });
            };
            let mut tile_tree = PalwAuthTreeV1::new(tiles, row_opening.leaf_hash).ok_or("a row with no tiles")?;
            for event in row_events.values() {
                if let Ev::Tiled { tile_opening, .. } = event {
                    tile_tree.learn_leaf(tile_opening)?;
                }
            }
            let own_tiles: Vec<Hash64> =
                (0..tiles).map(|t| tiled(row, t as u8).map(|(_, o, _)| o.leaf_hash)).collect::<Result<_, String>>()?;
            let tiles_range = |first: u64, count: u32| Ok(own_tiles[first as usize..(first + u64::from(count)) as usize].to_vec());
            let own_tile_tree = PalwOwnLeafTreeV1::build(tiles, 0, &tiles_range)?;
            let tile = match palw_auth_descent_next_v1(&tile_tree, &|l, i| own_tile_tree.node(l, i, &tiles_range), 1) {
                PalwAuthDescentV1::Read { first, .. } => return Ok(PalwTraceLocalizeV1::Read { row, tile: first as u8 }),
                PalwAuthDescentV1::FirstDivergentLeaf(t) => t as u8,
                other => return Err(format!("the divergent row's tiles cannot be judged: {other:?}")),
            };
            let Some(event) = row_events.get(&tile).map(|e| (*e).clone()) else {
                return Ok(PalwTraceLocalizeV1::Read { row, tile });
            };
            let Ev::Tiled { tile_lanes, .. } = &event else { unreachable!("tiled above") };
            let (_, _, own_lanes) = tiled(row, tile)?;
            let at = tile_lanes.iter().zip(own_lanes.iter()).position(|(a, b)| a != b).ok_or("the divergent tile's lanes agree")?;
            (row, usize::from(tile) * PALW_LOGITS_TILE_LANES + at, event)
        }
        Ev::OutOfRange { .. } => return Err("the binding read opened no row".into()),
    };
    let head = crate::palw_step::palw_logits_head_v1(profile).ok_or("the class's head is not provable (HeadUnproven)")?;
    let head_tile = (lane / head.tile_len as usize) as u32;
    let coord = crate::palw_step::palw_logits_head_coordinate_v1(&head, ctx, row, head_tile).ok_or("no head coordinate")?;
    let index = crate::palw_step::canonical_step_leaf_index(profile, ctx, &coord).ok_or("no head leaf")?;
    let head_opening = own_head_opening(index)?;
    crate::palw_offence_attribution_v1::palw_logits_not_step_output_fault_v1(target, &event, row, head_tile, &head_opening, ladder)
        .map_err(|e| format!("the bent lane does not convict: {e:?}"))?;
    Ok(PalwTraceLocalizeV1::Proof(C::LogitsNotStepOutput { event, row, head_tile, head_opening }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_step_leg::{step_merkle_range_siblings_v1, step_merkle_root_v1};

    fn leaves(n: u64, salt: u64) -> Vec<Hash64> {
        (0..n).map(|i| Hash64::from_u64_word(i.wrapping_mul(0x9E37_79B9) ^ salt)).collect()
    }

    /// A whole-tree oracle over explicit leaf hashes (tests only; the node uses [`PalwOwnLeafTreeV1`]).
    fn whole(leaf_hashes: &[Hash64]) -> Vec<Vec<Hash64>> {
        let mut levels = vec![leaf_hashes.iter().enumerate().map(|(i, h)| step_merkle_leaf_v1(i as u64, h)).collect::<Vec<_>>()];
        while levels.last().unwrap().len() > 1 {
            let level = levels.last().unwrap();
            let mut next = Vec::new();
            let mut pairs = level.chunks_exact(2);
            for p in &mut pairs {
                next.push(step_merkle_node_v1(&p[0], &p[1]));
            }
            if let [odd] = pairs.remainder() {
                next.push(*odd);
            }
            levels.push(next);
        }
        levels
    }

    /// Run a whole descent against a producer that answers every read from its own (lying) leaves. Returns the located leaf and
    /// the reads spent.
    fn descend(claim: &[Hash64], own: &[Hash64], max_read: u64) -> (PalwAuthDescentV1, u64) {
        let n = claim.len() as u64;
        let claim_levels = whole(claim);
        let own_levels = whole(own);
        let mut tree = PalwAuthTreeV1::new(n, *claim_levels.last().unwrap().first().unwrap()).unwrap();
        assert_eq!(tree.root, step_merkle_root_v1(claim).unwrap());
        let own_node = |l: u8, i: u64| own_levels.get(usize::from(l))?.get(i as usize).copied();
        let mut reads = 0;
        loop {
            match palw_auth_descent_next_v1(&tree, &own_node, max_read) {
                PalwAuthDescentV1::Read { first, count } => {
                    reads += 1;
                    assert!(count >= 1 && count <= max_read);
                    let (a, b) = (first as usize, (first + count) as usize);
                    let opening = PalwStepRangeOpeningV1 {
                        first_leaf_index: first,
                        leaf_hashes: claim[a..b].to_vec(),
                        siblings: step_merkle_range_siblings_v1(claim, a, b - a).unwrap(),
                    };
                    assert!(tree.learn_range(&opening).unwrap() > 0, "every read teaches a node");
                    assert!(reads <= palw_auth_descent_max_reads_v1(n, max_read), "n {n}: over the bound");
                }
                done => return (done, reads),
            }
        }
    }

    /// **Any single lie, isolated or followed by honest values, at any position, is located exactly, within the bound** — the
    /// exhaustive sweep over every position for every small size, and both read widths.
    #[test]
    fn every_position_of_a_lone_lie_is_located_within_the_read_bound() {
        for max_read in [1u64, 4, 1024] {
            for n in 1u64..=70 {
                let honest = leaves(n, 1);
                for lie in 0..n {
                    let mut claim = honest.clone();
                    claim[lie as usize] = Hash64::from_u64_word(0xBAD0 + lie);
                    let (done, reads) = descend(&claim, &honest, max_read);
                    assert_eq!(done, PalwAuthDescentV1::FirstDivergentLeaf(lie), "n {n} lie {lie} r {max_read}");
                    assert!(reads <= palw_auth_descent_max_reads_v1(n, max_read));
                }
                assert_eq!(descend(&honest, &honest, max_read).0, PalwAuthDescentV1::Agrees);
            }
        }
    }

    /// Several disjoint lies, the first anywhere, later ones anywhere after it: the FIRST is located (its inputs are honest).
    /// A matching leaf between two lies, or a matching midpoint, never moves the descent past an earlier lie.
    #[test]
    fn the_first_of_several_disjoint_lies_is_located_and_a_matching_midpoint_proves_nothing() {
        for n in [5u64, 33, 1_025, 4_097, 40_000] {
            let honest = leaves(n, 2);
            for first in [0, 1, n / 3, n / 2 - 1, n / 2, n - 2] {
                for later in [first + 1, (first + n) / 2, n - 1] {
                    if later <= first || later >= n {
                        continue;
                    }
                    let mut claim = honest.clone();
                    claim[first as usize] = Hash64::from_u64_word(0xF00);
                    claim[later as usize] = Hash64::from_u64_word(0xF01);
                    for max_read in [1u64, 1024] {
                        let (done, _) = descend(&claim, &honest, max_read);
                        assert_eq!(done, PalwAuthDescentV1::FirstDivergentLeaf(first), "n {n} first {first} later {later}");
                    }
                }
            }
        }
    }

    /// **The session bound at real scale**: a lie at the last leaf (the worst case of left-edge reads) and at assorted positions of
    /// a 2^26-leaf claim, against an own tree retained at 2^16 blocks and folded below from the own run, never the producer's
    /// material. Reads + binding + terminal stay within the reservation's 34 sessions up to the 2^40 ladder top.
    #[test]
    fn a_large_claims_lie_anywhere_is_located_within_thirty_four_sessions() {
        assert_eq!(palw_auth_descent_max_reads_v1(1 << 40, 1024), 31);
        assert!(palw_auth_descent_max_reads_v1(1 << 40, 1024) + 2 <= 34);
        assert_eq!(palw_auth_descent_max_reads_v1(1 << 22, 1024), 13);
        assert_eq!(palw_auth_descent_max_reads_v1(1024, 1024), 1);
        assert_eq!(palw_auth_descent_max_reads_v1(1025, 1024), 2);
        assert_eq!(palw_auth_descent_max_reads_v1(256, 1), 8);
        // A synthetic 2^20-leaf claim, leaf hashes generated on demand (the own run), one lie or two.
        let n: u64 = (1 << 20) + 3;
        let honest = |i: u64| Hash64::from_u64_word(i.wrapping_mul(0x51_7CC1_B727_220A) ^ 7);
        for lies in [vec![n - 1], vec![0], vec![n / 2], vec![1023, n - 5], vec![(1 << 19) + 1024, (1 << 19) + 2048]] {
            let claim_leaf = |i: u64| if lies.contains(&i) { Hash64::from_u64_word(0xBAD ^ i) } else { honest(i) };
            let claim_range = |first: u64, count: u32| -> Result<Vec<Hash64>, String> {
                Ok((first..first + u64::from(count)).map(claim_leaf).collect())
            };
            let own_range = |first: u64, count: u32| -> Result<Vec<Hash64>, String> {
                Ok((first..first + u64::from(count)).map(honest).collect())
            };
            let retain = PalwOwnLeafTreeV1::retain_level_v1(n);
            let own_tree = PalwOwnLeafTreeV1::build(n, retain, &own_range).unwrap();
            let claim_tree = PalwOwnLeafTreeV1::build(n, retain, &claim_range).unwrap();
            let mut tree = PalwAuthTreeV1::new(n, claim_tree.root()).unwrap();
            let own_node = |l: u8, i: u64| own_tree.node(l, i, &own_range);
            let mut reads = 0;
            let located = loop {
                match palw_auth_descent_next_v1(&tree, &own_node, 1024) {
                    PalwAuthDescentV1::Read { first, count } => {
                        reads += 1;
                        // The producer's answer: its leaves and the siblings of ITS tree (built from its retained level).
                        let siblings = range_siblings(n, first, count, &|l, i| claim_tree.node(l, i, &claim_range).unwrap());
                        let opening = PalwStepRangeOpeningV1 {
                            first_leaf_index: first,
                            leaf_hashes: claim_range(first, count as u32).unwrap(),
                            siblings,
                        };
                        tree.learn_range(&opening).unwrap();
                    }
                    PalwAuthDescentV1::FirstDivergentLeaf(leaf) => break leaf,
                    other => panic!("{other:?}"),
                }
            };
            assert_eq!(located, *lies.iter().min().unwrap());
            assert!(reads <= palw_auth_descent_max_reads_v1(n, 1024), "{reads} reads");
            assert!(reads + 2 <= 34);
        }
    }

    /// The producer's range siblings from a node oracle (`step_merkle_range_siblings_v1`'s order).
    fn range_siblings(n: u64, first: u64, count: u64, node: &dyn Fn(u8, u64) -> Hash64) -> Vec<Hash64> {
        let (mut a, mut b, mut width, mut level) = (first, first + count, n, 0u8);
        let mut out = Vec::new();
        while width > 1 {
            if a % 2 == 1 {
                out.push(node(level, a - 1));
            }
            if b % 2 == 1 && !(width % 2 == 1 && b == width) {
                out.push(node(level, b));
            }
            a /= 2;
            b = b.div_ceil(2);
            width = width.div_ceil(2);
            level += 1;
        }
        out
    }

    /// An opening of another commitment, a shifted range, a forged sibling or a short path teaches nothing; a later genuine
    /// opening still does. The own tree's retained fold equals the whole tree at every node.
    #[test]
    fn unauthenticated_openings_teach_nothing_and_the_own_fold_is_the_whole_tree() {
        let n = 3_001u64;
        let claim = leaves(n, 3);
        let root = step_merkle_root_v1(&claim).unwrap();
        let mut tree = PalwAuthTreeV1::new(n, root).unwrap();
        let good = PalwStepRangeOpeningV1 {
            first_leaf_index: 1_000,
            leaf_hashes: claim[1_000..1_100].to_vec(),
            siblings: step_merkle_range_siblings_v1(&claim, 1_000, 100).unwrap(),
        };
        let before = tree.clone();
        for what in 0..4 {
            let mut bad = good.clone();
            match what {
                0 => bad.leaf_hashes[7] = Hash64::from_u64_word(1),
                1 => bad.first_leaf_index = 1_001,
                2 => bad.siblings[0] = Hash64::from_u64_word(2),
                _ => {
                    bad.siblings.pop();
                }
            }
            assert!(tree.learn_range(&bad).is_err());
            assert_eq!(tree, before);
        }
        assert!(tree.learn_range(&good).unwrap() > 100);
        let levels = whole(&claim);
        for ((l, i), node) in &tree.nodes {
            assert_eq!(levels[usize::from(*l)][*i as usize], *node, "a learned node is the tree's own");
        }
        let range = |first: u64, count: u32| Ok(claim[first as usize..(first + u64::from(count)) as usize].to_vec());
        for retain in [0u8, 3, 10, 12] {
            let own = PalwOwnLeafTreeV1::build(n, retain, &range).unwrap();
            assert_eq!(own.root(), root);
            for (l, level) in levels.iter().enumerate() {
                for i in [0, level.len() as u64 / 2, level.len() as u64 - 1] {
                    assert_eq!(own.node(l as u8, i, &range), Some(level[i as usize]), "retain {retain} node ({l}, {i})");
                }
            }
        }
        // One leaf opening is a range of one.
        let mut single = PalwAuthTreeV1::new(n, root).unwrap();
        let opening = crate::palw_step_leg::PalwStepOpeningV1 {
            leaf_index: n - 1,
            leaf_hash: claim[(n - 1) as usize],
            siblings: step_merkle_range_siblings_v1(&claim, (n - 1) as usize, 1).unwrap(),
        };
        assert_eq!(crate::palw_step_leg::step_opening_root_capped_v1(n, &opening, n).unwrap(), root);
        assert!(single.learn_leaf(&opening).unwrap() > 0);
    }

    /// An own tree that disagrees at a node but agrees with both its authenticated children is the verifier's own fault: nothing
    /// is located.
    #[test]
    fn an_inconsistent_own_tree_locates_nothing() {
        let n = 8u64;
        let claim = leaves(n, 4);
        let levels = whole(&claim);
        let mut tree = PalwAuthTreeV1::new(n, *levels.last().unwrap().first().unwrap()).unwrap();
        tree.learn_range(&PalwStepRangeOpeningV1 { first_leaf_index: 0, leaf_hashes: claim.clone(), siblings: vec![] }).unwrap();
        let own = |l: u8, i: u64| if l == 3 { Some(Hash64::from_u64_word(9)) } else { Some(levels[usize::from(l)][i as usize]) };
        assert_eq!(palw_auth_descent_next_v1(&tree, &own, 1024), PalwAuthDescentV1::OwnInconsistent { level: 3, index: 0 });
        let none = |_: u8, _: u64| None;
        assert_eq!(palw_auth_descent_next_v1(&tree, &none, 1024), PalwAuthDescentV1::OwnUnavailable { level: 3, index: 0 });
    }
}
