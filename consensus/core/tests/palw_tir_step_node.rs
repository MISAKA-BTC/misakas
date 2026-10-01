//! **The second IR fence's descent unit, by hash arithmetic alone: `TirStepNode { level, index }`**
//! (`Params::palw_tir_fence2`; evidence transport C).
//!
//! A node's answer is its frontier — the nodes eight levels below it, or the leaf nodes when nearer —
//! and its opening; the chain folds the frontier by the step tree's own rule (pairs, an odd last node
//! promoted) and walks the node to the committed step root. Here, over synthetic trees:
//!
//! * **every node of every small tree** — every count to 70 and the ragged counts around powers of
//!   two — is answered by the prover (`palw_tir_step_node_parts_v1`) and reaches its root, and
//!   nothing past the tree is;
//! * **a tampered answer never reaches the root**: any frontier node or sibling flipped, dropped or
//!   added, another node's answer, another level's;
//! * **one seat finds the first leaf it disputes in a D-F1-sized tree** — 2^22 leaves, the most a
//!   binding commits, twenty-two levels — **in three node sessions and one leaf session**, inside its
//!   four, against a liar that answers every demand and garbles the leaves after its lie too; and in
//!   a ragged 2^16 + 3 tree, whose promotions the descent crosses.
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_tir_step_node`

use std::collections::{BTreeMap, BTreeSet};

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_step_leg::{
    PALW_STEP_LEG_MAX_LEAVES, PalwStepOpeningV1, step_merkle_leaf_v1, step_merkle_node_v1, step_merkle_root_v1,
    step_opening_root_capped_v1, step_opening_v1,
};
use kaspa_consensus_core::palw_tir_court_v1::{
    PALW_TIR_STEP_NODE_DEPTH_V1, palw_tir_step_node_frontier_v1, palw_tir_step_node_parts_v1, palw_tir_step_node_reaches_v1,
    palw_tir_step_tree_height_v1, palw_tir_step_tree_width_v1,
};

fn leaf_hash(i: u64) -> Hash64 {
    Hash64::from_u64_word(0x5EED_0000_0000 ^ i)
}

/// A step tree level by level: `levels[0]` the index-bound leaf nodes, the last the root.
struct Tree {
    levels: Vec<Vec<Hash64>>,
}

impl Tree {
    fn of(hashes: &[Hash64]) -> Self {
        let mut levels = vec![hashes.iter().enumerate().map(|(i, h)| step_merkle_leaf_v1(i as u64, h)).collect::<Vec<_>>()];
        while levels.last().unwrap().len() > 1 {
            let last = levels.last().unwrap();
            let mut next: Vec<Hash64> = last.chunks_exact(2).map(|pair| step_merkle_node_v1(&pair[0], &pair[1])).collect();
            if last.len() % 2 == 1 {
                next.push(*last.last().unwrap());
            }
            levels.push(next);
        }
        Self { levels }
    }

    fn count(&self) -> u64 {
        self.levels[0].len() as u64
    }

    fn root(&self) -> Hash64 {
        *self.levels.last().unwrap().last().unwrap()
    }
}

/// **A liar's tree**: an honest tree with some leaves replaced — the replaced nodes kept aside, so a
/// 2^22-leaf liar costs one tree, not two.
struct Liar<'a> {
    honest: &'a Tree,
    leaves: BTreeMap<u64, Hash64>,
    changed: BTreeMap<(u8, u64), Hash64>,
}

impl<'a> Liar<'a> {
    fn new(honest: &'a Tree, leaves: &[(u64, Hash64)]) -> Self {
        let mut liar = Self { honest, leaves: leaves.iter().copied().collect(), changed: BTreeMap::new() };
        let mut positions: BTreeSet<u64> = BTreeSet::new();
        for (i, h) in leaves {
            liar.changed.insert((0, *i), step_merkle_leaf_v1(*i, h));
            positions.insert(*i);
        }
        for level in 0..(honest.levels.len() - 1) as u8 {
            let width = honest.levels[level as usize].len() as u64;
            let mut parents = BTreeSet::new();
            for p in positions {
                let parent = if width % 2 == 1 && p == width - 1 {
                    liar.node(level, p)
                } else {
                    let left = p & !1;
                    step_merkle_node_v1(&liar.node(level, left), &liar.node(level, left + 1))
                };
                liar.changed.insert((level + 1, p / 2), parent);
                parents.insert(p / 2);
            }
            positions = parents;
        }
        liar
    }

    fn node(&self, level: u8, position: u64) -> Hash64 {
        self.changed.get(&(level, position)).copied().unwrap_or(self.honest.levels[level as usize][position as usize])
    }

    fn root(&self) -> Hash64 {
        self.node((self.honest.levels.len() - 1) as u8, 0)
    }

    /// Leaf `index`'s opening in the liar's tree.
    fn opening(&self, index: u64) -> PalwStepOpeningV1 {
        let leaf_hash = self.leaves.get(&index).copied().unwrap_or_else(|| leaf_hash(index));
        PalwStepOpeningV1 { leaf_index: index, leaf_hash, siblings: self.siblings(0, index) }
    }

    /// Node `(level, index)`'s answer: its frontier and its opening.
    fn answer(&self, level: u8, index: u64) -> (Vec<Hash64>, Vec<Hash64>) {
        let (below, first, end) = palw_tir_step_node_frontier_v1(self.honest.count(), level, index).expect("in the tree");
        let frontier = (first..end).map(|p| self.node(below, p)).collect();
        (frontier, self.siblings(level, index))
    }

    fn siblings(&self, level: u8, index: u64) -> Vec<Hash64> {
        let mut siblings = Vec::new();
        let (mut l, mut position) = (level, index);
        while let Some(width) = palw_tir_step_tree_width_v1(self.honest.count(), l).filter(|w| *w > 1) {
            if !(width % 2 == 1 && position == width - 1) {
                siblings.push(self.node(l, position ^ 1));
            }
            position /= 2;
            l += 1;
        }
        siblings
    }
}

#[test]
fn the_tree_shape_is_the_step_trees() {
    assert_eq!(palw_tir_step_tree_height_v1(1), 0);
    assert_eq!(palw_tir_step_tree_height_v1(2), 1);
    assert_eq!(palw_tir_step_tree_height_v1(3), 2);
    assert_eq!(palw_tir_step_tree_height_v1(256), 8);
    assert_eq!(palw_tir_step_tree_height_v1(257), 9);
    assert_eq!(palw_tir_step_tree_height_v1(PALW_STEP_LEG_MAX_LEAVES), 22, "D-F1 size: twenty-two levels");
    assert_eq!(palw_tir_step_tree_width_v1(PALW_STEP_LEG_MAX_LEAVES, 22), Some(1));
    assert_eq!(palw_tir_step_tree_width_v1(PALW_STEP_LEG_MAX_LEAVES, 23), None);
    assert_eq!(palw_tir_step_tree_width_v1(0, 0), None);
    assert_eq!(palw_tir_step_tree_width_v1(5, 1), Some(3));
    assert_eq!(palw_tir_step_tree_width_v1(5, 3), Some(1));
    assert_eq!(palw_tir_step_tree_width_v1(5, 4), None);
    // The frontiers of a D-F1 descent: 22 → 14 → 6 → the leaf nodes.
    assert_eq!(palw_tir_step_node_frontier_v1(PALW_STEP_LEG_MAX_LEAVES, 22, 0), Some((14, 0, 256)));
    assert_eq!(palw_tir_step_node_frontier_v1(PALW_STEP_LEG_MAX_LEAVES, 14, 3), Some((6, 768, 1024)));
    assert_eq!(palw_tir_step_node_frontier_v1(PALW_STEP_LEG_MAX_LEAVES, 6, 5), Some((0, 320, 384)));
    assert_eq!(palw_tir_step_node_frontier_v1(PALW_STEP_LEG_MAX_LEAVES, 0, 5), None, "a leaf has no frontier");
    // A ragged tail: the last node covers what is left.
    assert_eq!(palw_tir_step_node_frontier_v1(1000, 9, 1), Some((1, 256, 500)));
    assert_eq!(PALW_TIR_STEP_NODE_DEPTH_V1, 8);
}

#[test]
fn every_node_of_every_small_tree_is_answered_and_reaches_its_root() {
    let counts: Vec<u64> = (1..=70).chain([127, 128, 129, 255, 256, 257, 511, 513, 1000, 1001]).collect();
    for count in counts {
        let hashes: Vec<Hash64> = (0..count).map(leaf_hash).collect();
        let tree = Tree::of(&hashes);
        assert_eq!(tree.root(), step_merkle_root_v1(&hashes).unwrap(), "{count}: the test's tree is the step tree");
        let height = palw_tir_step_tree_height_v1(count);
        assert_eq!(height as usize, tree.levels.len() - 1);
        for level in 1..=height {
            let width = palw_tir_step_tree_width_v1(count, level).unwrap();
            assert_eq!(width, tree.levels[level as usize].len() as u64, "{count}: level {level}'s width");
            let indices: Vec<u64> =
                if count <= 70 { (0..width).collect() } else { vec![0, 1u64.min(width - 1), width / 2, width - 1] };
            for index in indices {
                let (frontier, siblings) = palw_tir_step_node_parts_v1(&hashes, level, index).expect("in the tree");
                let (below, first, end) = palw_tir_step_node_frontier_v1(count, level, index).unwrap();
                assert_eq!(below, level.saturating_sub(8));
                assert_eq!(
                    frontier,
                    tree.levels[below as usize][first as usize..end as usize],
                    "{count}: ({level}, {index})'s frontier"
                );
                palw_tir_step_node_reaches_v1(count, &tree.root(), level, index, &frontier, &siblings)
                    .unwrap_or_else(|e| panic!("{count}: ({level}, {index}): {e}"));
            }
            assert!(palw_tir_step_node_parts_v1(&hashes, level, width).is_none(), "{count}: past level {level}");
            assert!(palw_tir_step_node_frontier_v1(count, level, width).is_none());
        }
        assert!(palw_tir_step_node_parts_v1(&hashes, height + 1, 0).is_none(), "{count}: above the root");
        assert!(palw_tir_step_node_parts_v1(&hashes, 0, 0).is_none(), "{count}: a leaf is not a node");
        assert!(palw_tir_step_node_reaches_v1(count, &tree.root(), 0, 0, &[tree.levels[0][0]], &[]).is_err());
    }
}

#[test]
fn a_tampered_answer_never_reaches_the_root() {
    for count in [1000u64, 1001, 4099] {
        let hashes: Vec<Hash64> = (0..count).map(leaf_hash).collect();
        let tree = Tree::of(&hashes);
        let root = tree.root();
        let height = palw_tir_step_tree_height_v1(count);
        for (level, index) in [(1u8, 0u64), (3, 7), (9, 1), (height - 1, 1), (height, 0)] {
            let Some((frontier, siblings)) = palw_tir_step_node_parts_v1(&hashes, level, index) else { continue };
            let reaches = |f: &[Hash64], s: &[Hash64]| palw_tir_step_node_reaches_v1(count, &root, level, index, f, s).is_ok();
            assert!(reaches(&frontier, &siblings), "{count}: ({level}, {index})");
            let flip = Hash64::from_u64_word(0xF11F);
            for k in 0..frontier.len() {
                let mut f = frontier.clone();
                f[k] = flip;
                assert!(!reaches(&f, &siblings), "{count}: ({level}, {index}): frontier node {k} flipped");
            }
            for k in 0..siblings.len() {
                let mut s = siblings.clone();
                s[k] = flip;
                assert!(!reaches(&frontier, &s), "{count}: ({level}, {index}): sibling {k} flipped");
            }
            assert!(!reaches(&frontier[1..], &siblings), "a frontier node dropped");
            assert!(!reaches(&[frontier.clone(), vec![flip]].concat(), &siblings), "a frontier node added");
            if !siblings.is_empty() {
                assert!(!reaches(&frontier, &siblings[1..]), "a sibling dropped");
            }
            assert!(!reaches(&frontier, &[siblings.clone(), vec![flip]].concat()), "a sibling added");
            // Another node's answer, for this node; this answer at another level.
            if let Some((f, s)) = palw_tir_step_node_parts_v1(&hashes, level, index + 1) {
                assert!(!reaches(&f, &s), "{count}: ({level}, {}) answering ({level}, {index})", index + 1);
            }
            for other in [level - 1, level + 1] {
                assert!(
                    palw_tir_step_node_reaches_v1(count, &root, other, index, &frontier, &siblings).is_err(),
                    "{count}: ({level}, {index})'s answer at level {other}"
                );
            }
        }
    }
}

/// One seat against a liar that answers every demand: the root, then the first node of each frontier
/// its own tree disagrees with, then that leaf. Returns the leaf and the sessions it took.
fn descend(mine: &Tree, liar: &Liar<'_>) -> (u64, u32) {
    let count = mine.count();
    let committed = liar.root();
    let (mut level, mut index) = (palw_tir_step_tree_height_v1(count), 0u64);
    let mut sessions = 0u32;
    loop {
        sessions += 1;
        let (frontier, siblings) = liar.answer(level, index);
        palw_tir_step_node_reaches_v1(count, &committed, level, index, &frontier, &siblings)
            .unwrap_or_else(|e| panic!("({level}, {index}): the liar's answer is its tree's: {e}"));
        let (below, first, end) = palw_tir_step_node_frontier_v1(count, level, index).unwrap();
        let own = &mine.levels[below as usize][first as usize..end as usize];
        let k = frontier.iter().zip(own).position(|(theirs, ours)| theirs != ours).expect("a disputed node's frontier differs") as u64;
        (level, index) = (below, first + k);
        if level == 0 {
            // The leaf session: the leaf and its opening under the committed root.
            sessions += 1;
            let opening = liar.opening(index);
            assert_eq!(
                step_opening_root_capped_v1(count, &opening, PALW_STEP_LEG_MAX_LEAVES).expect("the opening walks"),
                committed,
                "leaf {index}'s opening reaches the committed root"
            );
            assert_ne!(step_merkle_leaf_v1(index, &opening.leaf_hash), mine.levels[0][index as usize], "and it is the disputed leaf");
            return (index, sessions);
        }
    }
}

fn honest_tree(count: u64) -> Tree {
    Tree::of(&(0..count).map(leaf_hash).collect::<Vec<_>>())
}

/// The lie at `lie`, and later leaves garbled too — the last, and the one right after the lie.
fn lies(count: u64, lie: u64) -> Vec<(u64, Hash64)> {
    let mut changed = vec![(lie, Hash64::from_u64_word(0x11E))];
    for later in [lie + 1, count - 1] {
        if later > lie && later < count {
            changed.push((later, Hash64::from_u64_word(0x6A2B ^ later)));
        }
    }
    changed
}

#[test]
fn one_seat_finds_the_first_disputed_leaf_of_a_d_f1_sized_tree_in_four_sessions() {
    let count = PALW_STEP_LEG_MAX_LEAVES;
    let mine = honest_tree(count);
    for lie in [count - 2, count / 3 + 7, 0] {
        let changed = lies(count, lie);
        let liar = Liar::new(&mine, &changed);
        assert_ne!(liar.root(), mine.root());
        let (found, sessions) = descend(&mine, &liar);
        assert_eq!(found, lie, "the first leaf the seat's tree disputes");
        assert_eq!(sessions, 4, "22 levels: three node sessions and the leaf session");
    }
}

#[test]
fn one_seat_crosses_the_promotions_of_a_ragged_tree() {
    let count = (1u64 << 16) + 3;
    assert_eq!(palw_tir_step_tree_height_v1(count), 17);
    let mine = honest_tree(count);
    for lie in [count - 1, count - 3, (1 << 16) - 1, 12_345] {
        let changed = lies(count, lie);
        let liar = Liar::new(&mine, &changed);
        // The overlay is the liar's whole tree: every answer equals the prover's over its leaves.
        let mut hashes: Vec<Hash64> = (0..count).map(leaf_hash).collect();
        for (i, h) in &changed {
            hashes[*i as usize] = *h;
        }
        assert_eq!(liar.root(), step_merkle_root_v1(&hashes).unwrap());
        for (level, index) in [(17u8, 0u64), (9, 0), (9, 128), (1, lie / 2), (1, 32_769)] {
            if let Some(parts) = palw_tir_step_node_parts_v1(&hashes, level, index) {
                assert_eq!(liar.answer(level, index), parts, "({level}, {index})");
            }
        }
        assert_eq!(liar.opening(lie), step_opening_v1(&hashes, lie).unwrap());
        let (found, sessions) = descend(&mine, &liar);
        assert_eq!(found, lie);
        assert_eq!(sessions, 4, "17 levels: three node sessions and the leaf session");
    }
    // A tree of at most 2^16 leaves is two node sessions and the leaf.
    let mine = honest_tree(1 << 16);
    assert_eq!(descend(&mine, &Liar::new(&mine, &lies(1 << 16, 40_000))), (40_000, 3));
}
