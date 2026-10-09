//! **ADR-0175 × RFC-0009 L2: the rule-E part of the fork-choice leaf v2** — what a header-verified client needs to evaluate rule E
//! exactly as a node does (dormant: it rides `palw_fork_choice_rule_e_v1`, per lane L2FC's contract that a comparator fence reading
//! more ships its leaf version under the same fence). INTERNAL until the full-activation release.
//!
//! Lane L2FC owns the envelope (`palw_fork_choice_commitment_v1`: a header commits `H(borsh(leaf) ‖ state_root())` of its selected
//! parent's post-state) and the leaf's v1 fields. This module defines what leaf v2 appends — [`PalwRuleELeafV2Ext`], derived from
//! the state (no new state field; `state_root()` unchanged) — and both halves of its use:
//!
//! * **the window**: one [`PalwRuleEClaimRecordV1`] per claim accepted within the finality depth of the state's block, sorted by
//!   `(accepted_blue_score, claim_id)`, under a binary Merkle root. A client opens the SUFFIX above a fork `F`'s blue score on each
//!   tip ([`palw_rule_e_open_window_above_v1`] / [`palw_rule_e_verify_window_above_v1`]) — complete whenever `F` lies above the
//!   window's floor (below it, `F` is past any finality point: the client STOPs);
//! * **the registry**: the state's bond keys, sorted, under a binary Merkle root; a client proves each participating bond's
//!   membership in `F`'s registry ([`palw_rule_e_open_registry_v1`] / [`palw_rule_e_verify_registry_v1`]), and reads the even
//!   split's `n` as `F`'s `registry_len`;
//! * **the evaluation**: [`palw_rule_e_pair_from_openings_v1`] — the claim-id difference of the two suffixes, then
//!   [`palw_rule_e_side_from_records_v1`], the function the node computes its own sides with. So a client that verified the
//!   openings holds the node's `PalwRuleEPairV1`, and calls `palw_rule_e_decide_v1` / `palw_rule_e_order_v1` itself (the tie
//!   questions — GHOSTDAG order, the shallow window, which tip is the incumbent — taken both ways, as L2FC §5.2 does).
//!
//! Trees: RFC 6962's shape (split at the largest power of two below `n`) over keyed BLAKE2b-512, a domain per tree and per level
//! kind. A range opening carries the records of one contiguous range and the roots of the subtrees disjoint from it.

use crate::config::params::Params;
use crate::palw_fork_choice_rule_e_v1::{
    PALW_RULE_E_MAX_EXCLUSIVE_CLAIMS_V1, PalwRuleEClaimRecordV1, PalwRuleEErrorV1, PalwRuleEPairV1, palw_rule_e_even_split_min_v1,
    palw_rule_e_side_from_records_v1,
};
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwStateParamsV2};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::Hash64;
use std::collections::BTreeSet;

/// The leaf version a ruleset with rule E in force commits.
pub const PALW_RULE_E_LEAF_VERSION_V2: u16 = 2;

const WINDOW_LEAF_DOMAIN: &[u8] = b"misaka-palw/rule-e/leaf-v2/window/leaf";
const WINDOW_NODE_DOMAIN: &[u8] = b"misaka-palw/rule-e/leaf-v2/window/node";
const WINDOW_EMPTY_DOMAIN: &[u8] = b"misaka-palw/rule-e/leaf-v2/window/empty";
const REGISTRY_LEAF_DOMAIN: &[u8] = b"misaka-palw/rule-e/leaf-v2/registry/leaf";
const REGISTRY_NODE_DOMAIN: &[u8] = b"misaka-palw/rule-e/leaf-v2/registry/node";
const REGISTRY_EMPTY_DOMAIN: &[u8] = b"misaka-palw/rule-e/leaf-v2/registry/empty";

/// **What leaf v2 appends to the v1 fields** (fixed-size borsh, 156 bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwRuleELeafV2Ext {
    /// The window holds every claim with `accepted_blue_score > window_floor_blue_score` (the block's blue score less the finality
    /// depth, saturating).
    pub window_floor_blue_score: u64,
    pub window_len: u32,
    pub window_root: Hash64,
    /// The bond registry's size — the even split's `n` when this state is a fork's.
    pub registry_len: u64,
    pub registry_root: Hash64,
}

/// The encoded length of [`PalwForkChoiceLeafV2`]: the v1 leaf's 194 bytes and the 140 rule E appends.
pub const PALW_FORK_CHOICE_LEAF_V2_LEN: usize = 334;

/// **The fork-choice leaf v2** — lane L2FC's v1 leaf (`leaf_version`, then the v1 fields in v1 order and offsets, so the
/// selected-chain walk reads `block`/`daa_score`/`blue_score` where it always has) followed by rule E's fields. Fixed-size borsh,
/// [`PALW_FORK_CHOICE_LEAF_V2_LEN`] bytes; committed under L2FC's envelope where rule E is in force at the state's own point
/// (which leaf a state commits, and the envelope, are L2FC's — this module only builds and checks the leaf). `bonds_len` is the
/// registry tree's size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwForkChoiceLeafV2 {
    /// [`PALW_RULE_E_LEAF_VERSION_V2`].
    pub leaf_version: u16,
    pub block: Hash64,
    pub daa_score: u64,
    pub blue_score: u64,
    pub safe_frontier_blue_score: u64,
    pub safe_frontier: Hash64,
    pub safe_weight: u128,
    pub bounded_immature: u128,
    pub bonds_len: u64,
    pub window_floor_blue_score: u64,
    pub window_len: u32,
    pub window_root: Hash64,
    pub registry_root: Hash64,
}

impl PalwForkChoiceLeafV2 {
    /// **The leaf a state's post-state would commit under rule E**: `Some` iff the network is ConsensusV2, rule E is in force at
    /// the state's own point (`last_point.daa_score`), and the state has a point. Prices the window with the weight fences read at
    /// that point — as the node prices a tip.
    pub fn of(state: &PalwChainStateV2, params: &Params) -> Option<Self> {
        let point = *state.last_point()?;
        if !params.palw_fork_choice_rule_e_active_at(point.daa_score) {
            return None;
        }
        let state_params = params.palw_rule_e_state_params_v1()?;
        let weightless = params.palw_uncertified_weightless.is_some_and(|f| f.is_active(point.daa_score));
        let ext = palw_rule_e_leaf_v2_ext(
            state,
            state_params,
            weightless,
            params.palw_canonical_work_daa(),
            point.blue_score,
            params.finality_depth(),
        )
        .ok()?;
        let (safe_frontier_blue_score, safe_frontier) = state.safe_frontier();
        Some(Self {
            leaf_version: PALW_RULE_E_LEAF_VERSION_V2,
            block: point.block,
            daa_score: point.daa_score,
            blue_score: point.blue_score,
            safe_frontier_blue_score,
            safe_frontier,
            safe_weight: state.safe_weight(),
            bounded_immature: state.bounded_immature(),
            bonds_len: ext.registry_len,
            window_floor_blue_score: ext.window_floor_blue_score,
            window_len: ext.window_len,
            window_root: ext.window_root,
            registry_root: ext.registry_root,
        })
    }

    pub fn encode(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("a fixed-size leaf serializes")
    }

    /// Decode a leaf v2; any other length or version is refused.
    pub fn decode(bytes: &[u8]) -> Result<Self, PalwRuleELeafV2ErrorV1> {
        if bytes.len() != PALW_FORK_CHOICE_LEAF_V2_LEN {
            return Err(PalwRuleELeafV2ErrorV1::BadLeaf);
        }
        let leaf: Self = borsh::from_slice(bytes).map_err(|_| PalwRuleELeafV2ErrorV1::BadLeaf)?;
        if leaf.leaf_version != PALW_RULE_E_LEAF_VERSION_V2 {
            return Err(PalwRuleELeafV2ErrorV1::BadLeaf);
        }
        Ok(leaf)
    }

    /// The rule-E fields.
    pub fn ext(&self) -> PalwRuleELeafV2Ext {
        PalwRuleELeafV2Ext {
            window_floor_blue_score: self.window_floor_blue_score,
            window_len: self.window_len,
            window_root: self.window_root,
            registry_len: self.bonds_len,
            registry_root: self.registry_root,
        }
    }
}

/// **The node's prover for op 203**: `state`'s window opened above `from_blue_score` (exclusive). `None` where the state carries
/// no leaf v2 ([`PalwForkChoiceLeafV2::of`]).
pub fn prove_window_suffix_v1(state: &PalwChainStateV2, params: &Params, from_blue_score: u64) -> Option<PalwRuleEWindowOpeningV1> {
    let point = *state.last_point()?;
    if !params.palw_fork_choice_rule_e_active_at(point.daa_score) {
        return None;
    }
    let weightless = params.palw_uncertified_weightless.is_some_and(|f| f.is_active(point.daa_score));
    let (_, window) = palw_rule_e_window_v1(
        state,
        params.palw_rule_e_state_params_v1()?,
        weightless,
        params.palw_canonical_work_daa(),
        point.blue_score,
        params.finality_depth(),
    )
    .ok()?;
    Some(palw_rule_e_open_window_above_v1(&window, from_blue_score))
}

/// **The node's prover for op 203**: `bond`'s membership (or absence, by adjacency) in `state`'s registry.
pub fn prove_registry_membership_v1(state: &PalwChainStateV2, bond: &PalwBondKeyV2) -> PalwRuleERegistryOpeningV1 {
    palw_rule_e_open_registry_v1(&palw_rule_e_registry_keys_v1(state), bond)
}

/// **The client's verifier**: every record of the tip whose `leaf` it is, accepted above `from_blue_score` (exclusive) — complete:
/// the opening runs to `window_len` and starts at index 0 or at a record at or below `from`. Refuses `from` below the window's
/// floor (the fork is past finality: STOP).
pub fn verify_window_suffix_v1(
    leaf: &PalwForkChoiceLeafV2,
    from_blue_score: u64,
    proof: &PalwRuleEWindowOpeningV1,
) -> Result<Vec<PalwRuleEClaimRecordV1>, PalwRuleELeafV2ErrorV1> {
    palw_rule_e_verify_window_above_v1(&leaf.ext(), from_blue_score, proof)
}

/// **The client's verifier**: whether `bond` is in the registry `leaf` commits (non-membership by adjacency).
pub fn verify_registry_membership_v1(
    leaf: &PalwForkChoiceLeafV2,
    bond: &PalwBondKeyV2,
    proof: &PalwRuleERegistryOpeningV1,
) -> Result<bool, PalwRuleELeafV2ErrorV1> {
    palw_rule_e_verify_registry_v1(&leaf.ext(), bond, proof)
}

/// Why an opening does not verify.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwRuleELeafV2ErrorV1 {
    /// The fork lies at or below the window's floor: the suffix above it would not be complete (the fork is past finality).
    ForkBelowWindow,
    /// The range is not inside the tree, is empty where it must not be, or is not where the opening says.
    BadRange,
    /// The records are not in the window's order, or the boundary record lies above the fork.
    BadOrder,
    /// The recomputed root differs.
    RootMismatch,
    /// More exclusive records than a node weighs.
    TooMany,
    /// A side could not be computed.
    Side(PalwRuleEErrorV1),
    /// Not a leaf v2 (length or version).
    BadLeaf,
}

fn keyed(domain: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(domain).to_state();
    for part in parts {
        h.update(part);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

fn record_leaf(record: &PalwRuleEClaimRecordV1) -> Hash64 {
    keyed(WINDOW_LEAF_DOMAIN, &[&borsh::to_vec(record).expect("a record serializes")])
}

fn bond_leaf(bond: &PalwBondKeyV2) -> Hash64 {
    keyed(REGISTRY_LEAF_DOMAIN, &[&borsh::to_vec(bond).expect("a bond key serializes")])
}

/// The largest power of two strictly below `n` (`n ≥ 2`).
fn split(n: usize) -> usize {
    let mut k = 1usize;
    while k << 1 < n {
        k <<= 1;
    }
    k
}

/// RFC 6962's tree hash over `leaves`.
fn tree_root(leaves: &[Hash64], node: &[u8], empty: &[u8]) -> Hash64 {
    match leaves.len() {
        0 => keyed(empty, &[]),
        1 => leaves[0],
        n => {
            let k = split(n);
            keyed(node, &[tree_root(&leaves[..k], node, empty).as_byte_slice(), tree_root(&leaves[k..], node, empty).as_byte_slice()])
        }
    }
}

/// The roots of the subtrees of `leaves` disjoint from `[lo, hi)`, in the order [`root_from_range`] consumes them.
fn prove_range(leaves: &[Hash64], lo: usize, hi: usize, node: &[u8], empty: &[u8], out: &mut Vec<Hash64>) {
    let n = leaves.len();
    if hi == 0 || lo >= n {
        out.push(tree_root(leaves, node, empty));
        return;
    }
    if lo == 0 && hi >= n {
        return;
    }
    let k = split(n);
    prove_range(&leaves[..k], lo, hi.min(k), node, empty, out);
    prove_range(&leaves[k..], lo.saturating_sub(k), hi.saturating_sub(k), node, empty, out);
}

/// The root of a tree of `n` leaves from the leaves of `[lo, hi)` (`range`, in order) and the disjoint subtrees' roots (`proof`,
/// consumed in order). `None` when either runs short.
fn root_from_range(
    n: usize,
    lo: usize,
    hi: usize,
    range: &mut impl Iterator<Item = Hash64>,
    proof: &mut impl Iterator<Item = Hash64>,
    node: &[u8],
    empty: &[u8],
) -> Option<Hash64> {
    if n == 0 {
        return Some(keyed(empty, &[]));
    }
    if hi == 0 || lo >= n {
        return proof.next();
    }
    if lo == 0 && hi >= n {
        let leaves: Vec<Hash64> = (0..n).map(|_| range.next()).collect::<Option<_>>()?;
        return Some(tree_root(&leaves, node, empty));
    }
    let k = split(n);
    let left = root_from_range(k, lo, hi.min(k), range, proof, node, empty)?;
    let right = root_from_range(n - k, lo.saturating_sub(k), hi.saturating_sub(k), range, proof, node, empty)?;
    Some(keyed(node, &[left.as_byte_slice(), right.as_byte_slice()]))
}

/// **A state's window**: the record of every claim accepted above `blue_score − finality_depth`, in `(accepted_blue_score,
/// claim_id)` order, priced at this state ([`PalwChainStateV2::palw_rule_e_records_v1`]).
pub fn palw_rule_e_window_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    uncertified_weightless: bool,
    canonical_work_daa: Option<u64>,
    blue_score: u64,
    finality_depth: u64,
) -> Result<(u64, Vec<PalwRuleEClaimRecordV1>), PalwRuleEErrorV1> {
    let floor = blue_score.saturating_sub(finality_depth);
    let mut records = state
        .palw_rule_e_records_v1(params, uncertified_weightless, canonical_work_daa, |_, claim| claim.accepted_blue_score > floor)?;
    records.sort_by(|a, b| (a.accepted_blue_score, a.claim_id).cmp(&(b.accepted_blue_score, b.claim_id)));
    Ok((floor, records))
}

/// A state's bond keys in key order.
pub fn palw_rule_e_registry_keys_v1(state: &PalwChainStateV2) -> Vec<PalwBondKeyV2> {
    let mut keys: Vec<PalwBondKeyV2> = state.bonds_iter().map(|(k, _)| *k).collect();
    keys.sort();
    keys
}

/// **Leaf v2's rule-E fields for a state** at its block's `blue_score`.
pub fn palw_rule_e_leaf_v2_ext(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    uncertified_weightless: bool,
    canonical_work_daa: Option<u64>,
    blue_score: u64,
    finality_depth: u64,
) -> Result<PalwRuleELeafV2Ext, PalwRuleEErrorV1> {
    let (floor, window) =
        palw_rule_e_window_v1(state, params, uncertified_weightless, canonical_work_daa, blue_score, finality_depth)?;
    let keys = palw_rule_e_registry_keys_v1(state);
    Ok(palw_rule_e_leaf_v2_ext_of(floor, &window, &keys))
}

/// Leaf v2's rule-E fields from a window and a registry already built.
pub fn palw_rule_e_leaf_v2_ext_of(floor: u64, window: &[PalwRuleEClaimRecordV1], keys: &[PalwBondKeyV2]) -> PalwRuleELeafV2Ext {
    let window_leaves: Vec<Hash64> = window.iter().map(record_leaf).collect();
    let registry_leaves: Vec<Hash64> = keys.iter().map(bond_leaf).collect();
    PalwRuleELeafV2Ext {
        window_floor_blue_score: floor,
        window_len: window.len() as u32,
        window_root: tree_root(&window_leaves, WINDOW_NODE_DOMAIN, WINDOW_EMPTY_DOMAIN),
        registry_len: keys.len() as u64,
        registry_root: tree_root(&registry_leaves, REGISTRY_NODE_DOMAIN, REGISTRY_EMPTY_DOMAIN),
    }
}

/// **A tip's window opened above a fork**: the records `[lo, window_len)` — the suffix above the fork's blue score, preceded by the
/// last record at or below it when there is one (the boundary that proves the suffix starts where it says) — and the proof.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwRuleEWindowOpeningV1 {
    pub lo: u32,
    pub records: Vec<PalwRuleEClaimRecordV1>,
    pub proof: Vec<Hash64>,
}

/// The node's half: open `window` (a state's [`palw_rule_e_window_v1`]) above `fork_blue_score`.
pub fn palw_rule_e_open_window_above_v1(window: &[PalwRuleEClaimRecordV1], fork_blue_score: u64) -> PalwRuleEWindowOpeningV1 {
    let first_above = window.partition_point(|r| r.accepted_blue_score <= fork_blue_score);
    let lo = first_above.saturating_sub(1);
    let leaves: Vec<Hash64> = window.iter().map(record_leaf).collect();
    let mut proof = Vec::new();
    prove_range(&leaves, lo, window.len(), WINDOW_NODE_DOMAIN, WINDOW_EMPTY_DOMAIN, &mut proof);
    PalwRuleEWindowOpeningV1 { lo: lo as u32, records: window[lo..].to_vec(), proof }
}

/// **The client's half**: verify an opening of a tip whose leaf v2 carries `ext`, and return the tip's records above
/// `fork_blue_score` — every claim that tip's chain accepted above the fork.
pub fn palw_rule_e_verify_window_above_v1(
    ext: &PalwRuleELeafV2Ext,
    fork_blue_score: u64,
    opening: &PalwRuleEWindowOpeningV1,
) -> Result<Vec<PalwRuleEClaimRecordV1>, PalwRuleELeafV2ErrorV1> {
    if fork_blue_score < ext.window_floor_blue_score {
        return Err(PalwRuleELeafV2ErrorV1::ForkBelowWindow);
    }
    let (n, lo) = (ext.window_len as usize, opening.lo as usize);
    if lo > n || lo + opening.records.len() != n {
        return Err(PalwRuleELeafV2ErrorV1::BadRange);
    }
    let key = |r: &PalwRuleEClaimRecordV1| (r.accepted_blue_score, r.claim_id);
    if opening.records.windows(2).any(|w| key(&w[0]) >= key(&w[1])) {
        return Err(PalwRuleELeafV2ErrorV1::BadOrder);
    }
    // The boundary: the first opened record when it lies at or below the fork — the last such record. It may sit at index 0 (one
    // record of the window at or below the fork); with records before the range it is required, so the suffix starts where it says.
    let boundary = opening.records.first().is_some_and(|r| r.accepted_blue_score <= fork_blue_score);
    if lo > 0 && !boundary {
        return Err(PalwRuleELeafV2ErrorV1::BadOrder);
    }
    // …and only it: a second record at or below the fork means the range starts too early (harmless, but not the canonical opening).
    if opening.records.iter().skip(usize::from(boundary)).any(|r| r.accepted_blue_score <= fork_blue_score) {
        return Err(PalwRuleELeafV2ErrorV1::BadRange);
    }
    let root = root_from_range(
        n,
        lo,
        n,
        &mut opening.records.iter().map(record_leaf),
        &mut opening.proof.iter().copied(),
        WINDOW_NODE_DOMAIN,
        WINDOW_EMPTY_DOMAIN,
    )
    .ok_or(PalwRuleELeafV2ErrorV1::BadRange)?;
    if root != ext.window_root {
        return Err(PalwRuleELeafV2ErrorV1::RootMismatch);
    }
    let above: Vec<PalwRuleEClaimRecordV1> =
        opening.records.iter().filter(|r| r.accepted_blue_score > fork_blue_score).copied().collect();
    if above.len() > PALW_RULE_E_MAX_EXCLUSIVE_CLAIMS_V1 {
        return Err(PalwRuleELeafV2ErrorV1::TooMany);
    }
    Ok(above)
}

/// **A registry opening for one bond**: the keys `[lo, lo + keys.len())` and the proof — the bond itself (membership), or the two
/// keys around where it would sit, or the one key at the edge it would precede or follow (non-membership).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwRuleERegistryOpeningV1 {
    pub lo: u64,
    pub keys: Vec<PalwBondKeyV2>,
    pub proof: Vec<Hash64>,
}

/// The node's half: open `keys` (a state's [`palw_rule_e_registry_keys_v1`]) for `bond`.
pub fn palw_rule_e_open_registry_v1(keys: &[PalwBondKeyV2], bond: &PalwBondKeyV2) -> PalwRuleERegistryOpeningV1 {
    let at = keys.partition_point(|k| k < bond);
    let (lo, hi) = if keys.get(at) == Some(bond) {
        (at, at + 1)
    } else if keys.is_empty() {
        (0, 0)
    } else if at == 0 {
        (0, 1)
    } else if at == keys.len() {
        (at - 1, at)
    } else {
        (at - 1, at + 1)
    };
    let leaves: Vec<Hash64> = keys.iter().map(bond_leaf).collect();
    let mut proof = Vec::new();
    if lo < hi {
        prove_range(&leaves, lo, hi, REGISTRY_NODE_DOMAIN, REGISTRY_EMPTY_DOMAIN, &mut proof);
    }
    PalwRuleERegistryOpeningV1 { lo: lo as u64, keys: keys[lo..hi].to_vec(), proof }
}

/// **The client's half**: whether `bond` is in the registry `ext` commits.
pub fn palw_rule_e_verify_registry_v1(
    ext: &PalwRuleELeafV2Ext,
    bond: &PalwBondKeyV2,
    opening: &PalwRuleERegistryOpeningV1,
) -> Result<bool, PalwRuleELeafV2ErrorV1> {
    let (n, lo, len) = (ext.registry_len as usize, opening.lo as usize, opening.keys.len());
    if n == 0 {
        return if len == 0 && opening.proof.is_empty() && ext.registry_root == keyed(REGISTRY_EMPTY_DOMAIN, &[]) {
            Ok(false)
        } else {
            Err(PalwRuleELeafV2ErrorV1::RootMismatch)
        };
    }
    if len == 0 || len > 2 || lo + len > n {
        return Err(PalwRuleELeafV2ErrorV1::BadRange);
    }
    if opening.keys.windows(2).any(|w| w[0] >= w[1]) {
        return Err(PalwRuleELeafV2ErrorV1::BadOrder);
    }
    let root = root_from_range(
        n,
        lo,
        lo + len,
        &mut opening.keys.iter().map(bond_leaf),
        &mut opening.proof.iter().copied(),
        REGISTRY_NODE_DOMAIN,
        REGISTRY_EMPTY_DOMAIN,
    )
    .ok_or(PalwRuleELeafV2ErrorV1::BadRange)?;
    if root != ext.registry_root {
        return Err(PalwRuleELeafV2ErrorV1::RootMismatch);
    }
    if opening.keys.contains(bond) {
        return if len == 1 { Ok(true) } else { Err(PalwRuleELeafV2ErrorV1::BadRange) };
    }
    // Non-membership: the bond sits strictly between the two keys, or beyond the one edge key.
    let proves_absence = match opening.keys.as_slice() {
        [a, b] => a < bond && bond < b,
        [only] => (lo == 0 && bond < only) || (lo + 1 == n && only < bond),
        _ => false,
    };
    if proves_absence { Ok(false) } else { Err(PalwRuleELeafV2ErrorV1::BadRange) }
}

/// **The client's evaluation of one pair**: `a` and `b` — the two tips' verified records above their fork `F` — the bonds of `F`'s
/// registry the client proved (`fork_registry`; a bond it did not prove absent or present is not counted, so the client must
/// prove every attempt record's bond it needs), and `F`'s `registry_len`. Each side: its records whose id the other side does not
/// hold, through the node's own side function. Equal to the node's `PalwRuleEPairV1` for the same tips.
pub fn palw_rule_e_pair_from_openings_v1(
    a: &[PalwRuleEClaimRecordV1],
    b: &[PalwRuleEClaimRecordV1],
    fork_registry: impl Fn(&PalwBondKeyV2) -> bool,
    fork_registry_len: u64,
) -> Result<PalwRuleEPairV1, PalwRuleELeafV2ErrorV1> {
    let ids_a: BTreeSet<Hash64> = a.iter().map(|r| r.claim_id).collect();
    let ids_b: BTreeSet<Hash64> = b.iter().map(|r| r.claim_id).collect();
    let side = |records: &[PalwRuleEClaimRecordV1], other: &BTreeSet<Hash64>| {
        palw_rule_e_side_from_records_v1(records.iter().filter(|r| !other.contains(&r.claim_id)), &fork_registry)
            .map_err(PalwRuleELeafV2ErrorV1::Side)
    };
    Ok(PalwRuleEPairV1 {
        a: side(a, &ids_b)?,
        b: side(b, &ids_a)?,
        even_split_min: palw_rule_e_even_split_min_v1(usize::try_from(fork_registry_len).unwrap_or(usize::MAX)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_fork_choice_rule_e_v1::PalwRuleEClaimStatusV1;
    use crate::tx::TransactionOutpoint;

    fn bond(i: u32) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(7), i))
    }

    fn record(id: u64, bs: u64, bond_i: u32, status: PalwRuleEClaimStatusV1) -> PalwRuleEClaimRecordV1 {
        PalwRuleEClaimRecordV1 {
            claim_id: Hash64::from_u64_word(id),
            accepted_blue_score: bs,
            bond: bond(bond_i),
            attempt: true,
            status,
            safe_weight: if status == PalwRuleEClaimStatusV1::Final { 1_000 } else { 0 },
            live_weight: if status == PalwRuleEClaimStatusV1::Live { 10 } else { 0 },
            capped: false,
            bond_collateral: None,
        }
    }

    #[test]
    fn the_leaf_is_334_bytes_v1_fields_first_and_refuses_any_other_shape() {
        let leaf = PalwForkChoiceLeafV2 {
            leaf_version: PALW_RULE_E_LEAF_VERSION_V2,
            block: Hash64::from_u64_word(1),
            daa_score: 2,
            blue_score: 3,
            safe_frontier_blue_score: 4,
            safe_frontier: Hash64::from_u64_word(5),
            safe_weight: 6,
            bounded_immature: 7,
            bonds_len: 8,
            window_floor_blue_score: 9,
            window_len: 10,
            window_root: Hash64::from_u64_word(11),
            registry_root: Hash64::from_u64_word(12),
        };
        let bytes = leaf.encode();
        assert_eq!(bytes.len(), PALW_FORK_CHOICE_LEAF_V2_LEN);
        assert_eq!(&bytes[..2], &2u16.to_le_bytes(), "the version first");
        assert_eq!(&bytes[2..66], leaf.block.as_byte_slice(), "then the v1 fields at their v1 offsets");
        assert_eq!(&bytes[66..74], &2u64.to_le_bytes());
        assert_eq!(&bytes[186..194], &8u64.to_le_bytes(), "bonds_len ends the v1 part at 194");
        assert_eq!(PalwForkChoiceLeafV2::decode(&bytes), Ok(leaf));
        assert_eq!(PalwForkChoiceLeafV2::decode(&bytes[..194]), Err(PalwRuleELeafV2ErrorV1::BadLeaf), "a v1-length leaf is not v2");
        let mut v1 = bytes.clone();
        v1[0] = 1;
        assert_eq!(PalwForkChoiceLeafV2::decode(&v1), Err(PalwRuleELeafV2ErrorV1::BadLeaf));
    }

    #[test]
    fn every_range_of_every_tree_up_to_nine_leaves_opens_and_verifies() {
        for n in 0..=9usize {
            let leaves: Vec<Hash64> = (0..n as u64).map(|i| keyed(b"t", &[&i.to_le_bytes()])).collect();
            let root = tree_root(&leaves, b"node", b"empty");
            for lo in 0..=n {
                for hi in lo..=n {
                    let mut proof = Vec::new();
                    prove_range(&leaves, lo, hi, b"node", b"empty", &mut proof);
                    let got =
                        root_from_range(n, lo, hi, &mut leaves[lo..hi].iter().copied(), &mut proof.iter().copied(), b"node", b"empty");
                    assert_eq!(got, Some(root), "n {n} [{lo}, {hi})");
                }
            }
        }
    }

    #[test]
    fn a_window_opens_above_a_fork_and_any_tampering_is_refused() {
        let window: Vec<_> = (0..7u64).map(|i| record(100 + i, 10 + 2 * i, i as u32, PalwRuleEClaimStatusV1::Live)).collect();
        let ext = palw_rule_e_leaf_v2_ext_of(5, &window, &[bond(1), bond(2)]);
        // 10 and 11: exactly one record at or below the fork — the boundary at index 0, which the opening starts with.
        for fork_bs in [5u64, 9, 10, 11, 15, 22, 30] {
            let opening = palw_rule_e_open_window_above_v1(&window, fork_bs);
            let above = palw_rule_e_verify_window_above_v1(&ext, fork_bs, &opening).unwrap_or_else(|e| panic!("{fork_bs}: {e:?}"));
            let expected: Vec<_> = window.iter().filter(|r| r.accepted_blue_score > fork_bs).copied().collect();
            assert_eq!(above, expected, "fork at {fork_bs}");
        }
        assert_eq!(
            palw_rule_e_verify_window_above_v1(&ext, 4, &palw_rule_e_open_window_above_v1(&window, 4)),
            Err(PalwRuleELeafV2ErrorV1::ForkBelowWindow),
            "a fork below the floor cannot be shown complete"
        );
        // Dropping the newest record, altering a weight, or hiding the boundary is refused.
        let good = palw_rule_e_open_window_above_v1(&window, 15);
        let mut dropped = good.clone();
        dropped.records.pop();
        assert!(palw_rule_e_verify_window_above_v1(&ext, 15, &dropped).is_err());
        let mut altered = good.clone();
        altered.records[2].live_weight += 1;
        assert_eq!(palw_rule_e_verify_window_above_v1(&ext, 15, &altered), Err(PalwRuleELeafV2ErrorV1::RootMismatch));
        let mut hidden = good.clone();
        hidden.lo += 1;
        hidden.records.remove(0);
        assert!(palw_rule_e_verify_window_above_v1(&ext, 15, &hidden).is_err(), "a suffix must start at its boundary");
        // The same with the boundary at index 0.
        let mut hidden0 = palw_rule_e_open_window_above_v1(&window, 10);
        assert_eq!(hidden0.lo, 0);
        hidden0.lo += 1;
        hidden0.records.remove(0);
        assert_eq!(palw_rule_e_verify_window_above_v1(&ext, 10, &hidden0), Err(PalwRuleELeafV2ErrorV1::BadOrder));
    }

    #[test]
    fn registry_membership_and_absence_are_proven_and_lies_are_refused() {
        let keys: Vec<_> = [2u32, 4, 6, 8, 10].map(bond).to_vec();
        let ext = palw_rule_e_leaf_v2_ext_of(0, &[], &keys);
        for i in 0..=11u32 {
            let opening = palw_rule_e_open_registry_v1(&keys, &bond(i));
            assert_eq!(palw_rule_e_verify_registry_v1(&ext, &bond(i), &opening), Ok(keys.contains(&bond(i))), "bond {i}");
        }
        // A membership opening of 4 does not prove 5 present, and an absence opening of 5 cannot be passed off for 4.
        let of_four = palw_rule_e_open_registry_v1(&keys, &bond(4));
        assert!(palw_rule_e_verify_registry_v1(&ext, &bond(5), &of_four).is_err());
        let of_five = palw_rule_e_open_registry_v1(&keys, &bond(5));
        assert!(palw_rule_e_verify_registry_v1(&ext, &bond(4), &of_five).is_err());
        let empty = palw_rule_e_leaf_v2_ext_of(0, &[], &[]);
        assert_eq!(palw_rule_e_verify_registry_v1(&empty, &bond(1), &palw_rule_e_open_registry_v1(&[], &bond(1))), Ok(false));
    }

    /// The client's pair: a claim both tips hold counts for neither; participation counts only bonds of the fork's registry.
    #[test]
    fn the_pair_from_openings_cancels_shared_claims_and_counts_only_the_forks_bonds() {
        use PalwRuleEClaimStatusV1::{Final, Live};
        let shared = record(1, 20, 1, Final);
        let a = vec![shared, record(2, 21, 2, Live), record(3, 22, 3, Live), record(9, 23, 9, Live)];
        let mut shared_on_b = shared;
        shared_on_b.status = Live;
        shared_on_b.safe_weight = 0;
        let b = vec![shared_on_b, record(4, 21, 4, Live)];
        let fork_bonds: BTreeSet<_> = [1u32, 2, 3, 4].map(bond).into_iter().collect();
        let pair = palw_rule_e_pair_from_openings_v1(&a, &b, |k| fork_bonds.contains(k), 4).unwrap();
        assert_eq!(pair.a.participation, 2, "bonds 2 and 3 — bond 9 is not the fork's, the shared claim is neither side's");
        assert_eq!(pair.b.participation, 1);
        assert_eq!(pair.a.safe_weight, 0, "the shared claim's Final decides nothing");
        assert_eq!((pair.a.exclusive_claims, pair.b.exclusive_claims), (3, 1));
        assert_eq!(pair.even_split_min, 2);
    }
}
