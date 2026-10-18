//! **RFC-0018 GAP-R4 — the rule-E leaf v3: header-committed rule E v2 participation and Gate C certification state.**
//!
//! Leaf v2 (`palw_fork_choice_rule_e_leaf_v2`) commits each exclusive-past record without its certification state and the registry
//! by bond key only, so a header-verified reader could neither apply Gate C nor rank by rule E v2's capital. Leaf v3 commits:
//!
//! * the **window** of records WITH their certification state (`PalwRuleEClaimRecordV1::certification`), under its own leaf domain;
//! * the **registry** as `(bond, collateral)` entries under its own leaf domain, and the registry's **capital total** (the v2 even
//!   split's base) in the leaf.
//!
//! **Which leaf a header commits** ([`palw_rule_e_commit_form_v1`], read at the committed state's own point, like every fence of
//! the envelope): below `palw_fork_choice_commitment_v1` the flat ADR-0043 root; past it the v1 leaf; past rule E the v2 leaf; past rule
//! E AND either `palw_rule_e_participation_v2` or Gate C (`palw_independently_work_certified_v1`) the v3 leaf. Below the v3 fences the
//! v1/v2 envelopes are byte-identical to what they were ([`palw_committed_state_root_v2`] equals `palw_committed_state_root_v1` there,
//! and leaf v2's record leaf drops the certification field). The envelope hash is the same keyed hash for every leaf
//! (`PALW_FORK_CHOICE_DOMAIN_V1`, `leaf ‖ inner root`); the leaf's version and length say which grammar it is.
//!
//! **Node and client agree by construction**: the node commits [`palw_committed_state_root_v2`]; a verifier (the RFC-09 L2 client,
//! the v39 state anchor, the carriage RPC) opens it through [`palw_verify_committed_leaf_v2`], which checks the grammar the reader's own
//! ruleset selects at the point, the point itself and the envelope — and refuses everything else (no raw-root fallback past the fence).
//! Window and registry openings verify against the leaf's roots with the v3 leaf encodings; the pair is the node's own accumulator.
//!
//! No cryptographic computation proof is involved (PRINCIPLES §8): hashes and Merkle openings of committed state only. Dormant: every
//! fence named here is `None` on every shipped preset.

use crate::config::params::{ForkActivation, Params};
use crate::palw_fork_choice::PalwCandidateOrderV1;
use crate::palw_fork_choice_commitment_v1::{
    PALW_FORK_CHOICE_DOMAIN_V1, PalwForkChoiceLeafV1, PalwForkChoicePointV1, palw_fork_choice_committed_at_v1,
};
use crate::palw_fork_choice_rule_e_leaf_v2::{
    PALW_FORK_CHOICE_LEAF_V2_LEN, PalwForkChoiceLeafV2, PalwRuleELeafV2ErrorV1, PalwRuleELeafV2Ext, PalwRuleEWindowOpeningV1, keyed,
    palw_rule_e_open_window_above_with, palw_rule_e_verify_window_above_with, palw_rule_e_window_v1, prove_range, root_from_range,
    tree_root,
};
use crate::palw_fork_choice_rule_e_v1::{
    PALW_RULE_E_MAX_EXCLUSIVE_CLAIMS_V1, PalwRuleEClaimRecordV1, PalwRuleEPairV1, palw_rule_e_even_split_min_v1,
    palw_rule_e_side_from_records_with_capital_v2,
};
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::Hash64;
use std::collections::BTreeSet;

/// The leaf version a state commits where rule E and (rule E v2 or Gate C) are in force.
pub const PALW_RULE_E_LEAF_VERSION_V3: u16 = 3;
/// Leaf v3's encoded length: leaf v2's 334 bytes and the registry's capital total (u128).
pub const PALW_FORK_CHOICE_LEAF_V3_LEN: usize = PALW_FORK_CHOICE_LEAF_V2_LEN + 16;

const WINDOW_LEAF_DOMAIN_V3: &[u8] = b"misaka-palw/rule-e/leaf-v3/window/leaf";
const WINDOW_NODE_DOMAIN_V3: &[u8] = b"misaka-palw/rule-e/leaf-v3/window/node";
const WINDOW_EMPTY_DOMAIN_V3: &[u8] = b"misaka-palw/rule-e/leaf-v3/window/empty";
const REGISTRY_LEAF_DOMAIN_V3: &[u8] = b"misaka-palw/rule-e/leaf-v3/registry/leaf";
const REGISTRY_NODE_DOMAIN_V3: &[u8] = b"misaka-palw/rule-e/leaf-v3/registry/node";
const REGISTRY_EMPTY_DOMAIN_V3: &[u8] = b"misaka-palw/rule-e/leaf-v3/registry/empty";

/// The v3 window leaf: the whole record, certification state included.
fn record_leaf_v3(record: &PalwRuleEClaimRecordV1) -> Hash64 {
    keyed(WINDOW_LEAF_DOMAIN_V3, &[&borsh::to_vec(record).expect("a record serializes")])
}

/// The v3 registry leaf: the bond and its collateral at the state.
fn capital_leaf_v3(bond: &PalwBondKeyV2, collateral: u64) -> Hash64 {
    keyed(REGISTRY_LEAF_DOMAIN_V3, &[&borsh::to_vec(bond).expect("a bond key serializes"), &collateral.to_le_bytes()])
}

/// **Leaf v3** — leaf v2's fields (with `leaf_version = 3` and the v3 roots) and the registry's capital total.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwForkChoiceLeafV3 {
    pub base: PalwForkChoiceLeafV2,
    /// Σ collateral over the registry — the v2 even split's base (a third of it, rounded up).
    pub registry_capital_total: u128,
}

/// A state's registry entries `(bond, collateral)` in key order.
pub fn palw_rule_e_registry_entries_v3(state: &PalwChainStateV2) -> Vec<(PalwBondKeyV2, u64)> {
    let mut entries: Vec<(PalwBondKeyV2, u64)> = state.bonds_iter().map(|(k, b)| (*k, b.collateral)).collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
}

impl PalwForkChoiceLeafV3 {
    /// The leaf v3 a state's post-state commits: `Some` iff the v3 form governs at the state's own point.
    pub fn of(state: &PalwChainStateV2, params: &Params) -> Option<Self> {
        let point = *state.last_point()?;
        if palw_rule_e_commit_form_v1(params, point.daa_score) != PalwCommitFormV1::RuleEV3 {
            return None;
        }
        let state_params = params.palw_rule_e_state_params_v1()?;
        let weightless = params.palw_uncertified_weightless.is_some_and(|f| f.is_active(point.daa_score));
        let (floor, window) = palw_rule_e_window_v1(
            state,
            state_params,
            weightless,
            params.palw_canonical_work_daa(),
            point.blue_score,
            params.finality_depth(),
        )
        .ok()?;
        let entries = palw_rule_e_registry_entries_v3(state);
        let window_leaves: Vec<Hash64> = window.iter().map(record_leaf_v3).collect();
        let registry_leaves: Vec<Hash64> = entries.iter().map(|(b, c)| capital_leaf_v3(b, *c)).collect();
        let (safe_frontier_blue_score, safe_frontier) = state.safe_frontier();
        Some(Self {
            base: PalwForkChoiceLeafV2 {
                leaf_version: PALW_RULE_E_LEAF_VERSION_V3,
                block: point.block,
                daa_score: point.daa_score,
                blue_score: point.blue_score,
                safe_frontier_blue_score,
                safe_frontier,
                safe_weight: state.safe_weight(),
                bounded_immature: state.bounded_immature(),
                bonds_len: entries.len() as u64,
                window_floor_blue_score: floor,
                window_len: window.len() as u32,
                window_root: tree_root(&window_leaves, WINDOW_NODE_DOMAIN_V3, WINDOW_EMPTY_DOMAIN_V3),
                registry_root: tree_root(&registry_leaves, REGISTRY_NODE_DOMAIN_V3, REGISTRY_EMPTY_DOMAIN_V3),
            },
            registry_capital_total: entries.iter().map(|(_, c)| *c as u128).sum(),
        })
    }

    pub fn encode(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("a fixed-size leaf serializes")
    }

    /// Decode a leaf v3; any other length or version is refused.
    pub fn decode(bytes: &[u8]) -> Result<Self, PalwRuleELeafV2ErrorV1> {
        if bytes.len() != PALW_FORK_CHOICE_LEAF_V3_LEN {
            return Err(PalwRuleELeafV2ErrorV1::BadLeaf);
        }
        let leaf: Self = borsh::from_slice(bytes).map_err(|_| PalwRuleELeafV2ErrorV1::BadLeaf)?;
        if leaf.base.leaf_version != PALW_RULE_E_LEAF_VERSION_V3 {
            return Err(PalwRuleELeafV2ErrorV1::BadLeaf);
        }
        Ok(leaf)
    }

    pub fn ext(&self) -> PalwRuleELeafV2Ext {
        self.base.ext()
    }
}

// -------------------------------------------------------------------------------------------------------------------------------
// The form and the committed root
// -------------------------------------------------------------------------------------------------------------------------------

/// Which grammar a post-state at a point is committed in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwCommitFormV1 {
    /// Below `palw_fork_choice_commitment_v1`: the ADR-0043 root itself.
    Flat,
    /// The fork-choice leaf v1 envelope.
    V1,
    /// Rule E in force: the rule-E leaf v2 envelope.
    RuleEV2,
    /// Rule E and (rule E v2 participation or Gate C) in force: the rule-E leaf v3 envelope.
    RuleEV3,
}

fn active(fence: Option<ForkActivation>, daa: u64) -> bool {
    fence.is_some_and(|f| f != ForkActivation::never() && f.is_active(daa))
}

/// **The form at `point_daa`** — the reader's own ruleset decides, never the opening.
pub fn palw_rule_e_commit_form_v1(params: &Params, point_daa: u64) -> PalwCommitFormV1 {
    if !palw_fork_choice_committed_at_v1(params.palw_fork_choice_commitment_v1, Some(point_daa)) {
        return PalwCommitFormV1::Flat;
    }
    if !params.palw_fork_choice_rule_e_active_at(point_daa) {
        return PalwCommitFormV1::V1;
    }
    if active(params.palw_rule_e_participation_v2, point_daa) || active(params.palw_independently_work_certified_v1, point_daa) {
        PalwCommitFormV1::RuleEV3
    } else {
        PalwCommitFormV1::RuleEV2
    }
}

fn envelope(leaf: &[u8], inner: &Hash64) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_FORK_CHOICE_DOMAIN_V1).to_state();
    state.update(leaf);
    state.update(inner.as_bytes().as_slice());
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// The committed leaf of a post-state in its form (`None`: flat, or no leaf can be built — the genesis state).
pub fn palw_committed_leaf_bytes_v2(state: &PalwChainStateV2, params: &Params) -> Option<Vec<u8>> {
    let point = state.last_point()?.daa_score;
    match palw_rule_e_commit_form_v1(params, point) {
        PalwCommitFormV1::Flat => None,
        PalwCommitFormV1::V1 => PalwForkChoiceLeafV1::of(state).map(|l| l.encode()),
        PalwCommitFormV1::RuleEV2 => PalwForkChoiceLeafV2::of(state, params).map(|l| l.encode()),
        PalwCommitFormV1::RuleEV3 => PalwForkChoiceLeafV3::of(state, params).map(|l| l.encode()),
    }
}

/// **THE root a header commits for `state` under `params`** — every producing and checking site with the ruleset at hand calls this.
/// Flat below the commitment fence; past it the envelope of the leaf of the form at the state's point. Where only the commitment
/// fence is armed it equals `palw_committed_state_root_v1` byte for byte; a leaf that cannot be built commits the flat root (and so
/// fails every past-fence check rather than being assumed).
pub fn palw_committed_state_root_v2(state: &PalwChainStateV2, params: &Params) -> Hash64 {
    let inner = state.state_root();
    match palw_committed_leaf_bytes_v2(state, params) {
        Some(leaf) => envelope(&leaf, &inner),
        None => inner,
    }
}

/// An opening of the post-state in the grammar selected by its point's ruleset.
/// The bytes retain their version; readers must verify them against a trusted root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwCommittedOpeningV2 {
    pub leaf: Vec<u8>,
    pub inner_root: Hash64,
}

impl PalwCommittedOpeningV2 {
    /// Flat states carry no leaf. Past the fence, failure to construct a leaf is an error.
    pub fn of(state: &PalwChainStateV2, params: &Params) -> Option<Self> {
        let point = state.last_point()?;
        let leaf = match palw_rule_e_commit_form_v1(params, point.daa_score) {
            PalwCommitFormV1::Flat => Vec::new(),
            _ => palw_committed_leaf_bytes_v2(state, params)?,
        };
        Some(Self { leaf, inner_root: state.state_root() })
    }

    pub fn committed_root(&self) -> Hash64 {
        if self.leaf.is_empty() { self.inner_root } else { envelope(&self.leaf, &self.inner_root) }
    }

    pub fn verify(
        &self,
        params: &Params,
        root: &Hash64,
        point: &PalwForkChoicePointV1,
    ) -> Result<PalwCommittedLeafViewV2, PalwCommittedLeafErrorV2> {
        palw_verify_committed_leaf_v2(params, root, point, &self.leaf, &self.inner_root)
    }
}

/// What a verified opening says: the comparator's order and, past rule E, the rule-E fields (and v3's capital total).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwCommittedLeafViewV2 {
    pub form: PalwCommitFormV1,
    pub order: PalwCandidateOrderV1,
    pub rule_e: Option<PalwRuleELeafV2Ext>,
    pub registry_capital_total: Option<u128>,
}

/// Why a committed opening does not verify.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwCommittedLeafErrorV2 {
    /// The reader's ruleset says the root is flat here, but a leaf was offered (or none where one is required).
    WrongForm,
    /// The leaf does not parse in the grammar the reader's ruleset selects.
    BadLeaf,
    /// The leaf names another point.
    PointMismatch,
    /// The envelope does not hash to the committed root.
    RootMismatch,
}

/// **The verifier every reader shares** (the v39 anchor, the L2 client, a light client): open `leaf` and `inner_root` against
/// `committed_root` for `point`, in the grammar `params` selects at `point.daa_score`.
pub fn palw_verify_committed_leaf_v2(
    params: &Params,
    committed_root: &Hash64,
    point: &PalwForkChoicePointV1,
    leaf: &[u8],
    inner_root: &Hash64,
) -> Result<PalwCommittedLeafViewV2, PalwCommittedLeafErrorV2> {
    use PalwCommittedLeafErrorV2 as E;
    let form = palw_rule_e_commit_form_v1(params, point.daa_score);
    let (block, daa, blue, order, rule_e, capital) = match form {
        PalwCommitFormV1::Flat => {
            // Below the commitment fence the header commits the ADR-0043 root itself: no leaf, and the roots are one.
            if !leaf.is_empty() || inner_root != committed_root {
                return Err(E::WrongForm);
            }
            return Ok(PalwCommittedLeafViewV2 {
                form,
                order: PalwCandidateOrderV1::new(0, 0, 0, point.block),
                rule_e: None,
                registry_capital_total: None,
            });
        }
        PalwCommitFormV1::V1 => {
            // The v1 grammar keeps its own verifier (the weight-allocation slot's version check included).
            let l = PalwForkChoiceLeafV1::decode(leaf).ok_or(E::BadLeaf)?;
            let order = crate::palw_fork_choice_commitment_v1::PalwForkChoiceOpeningV1 { leaf: l, inner_root: *inner_root }
                .verify(committed_root, point, params.palw_fork_choice_commitment_v1)
                .map_err(|_| E::RootMismatch)?;
            return Ok(PalwCommittedLeafViewV2 { form, order, rule_e: None, registry_capital_total: None });
        }
        PalwCommitFormV1::RuleEV2 => {
            let l = PalwForkChoiceLeafV2::decode(leaf).map_err(|_| E::BadLeaf)?;
            (
                l.block,
                l.daa_score,
                l.blue_score,
                PalwCandidateOrderV1::new(l.safe_frontier_blue_score, l.safe_weight, l.bounded_immature, l.block),
                Some(l.ext()),
                None,
            )
        }
        PalwCommitFormV1::RuleEV3 => {
            let l = PalwForkChoiceLeafV3::decode(leaf).map_err(|_| E::BadLeaf)?;
            let b = l.base;
            (
                b.block,
                b.daa_score,
                b.blue_score,
                PalwCandidateOrderV1::new(b.safe_frontier_blue_score, b.safe_weight, b.bounded_immature, b.block),
                Some(l.ext()),
                Some(l.registry_capital_total),
            )
        }
    };
    if (block, daa, blue) != (point.block, point.daa_score, point.blue_score) {
        return Err(E::PointMismatch);
    }
    if envelope(leaf, inner_root) != *committed_root {
        return Err(E::RootMismatch);
    }
    Ok(PalwCommittedLeafViewV2 { form, order, rule_e, registry_capital_total: capital })
}

// -------------------------------------------------------------------------------------------------------------------------------
// v3 openings: window (with certification) and registry (with capital)
// -------------------------------------------------------------------------------------------------------------------------------

/// The node's prover: `state`'s v3 window opened above `from_blue_score` (exclusive). `None` where the v3 form does not govern.
pub fn prove_window_suffix_v3(state: &PalwChainStateV2, params: &Params, from_blue_score: u64) -> Option<PalwRuleEWindowOpeningV1> {
    let point = *state.last_point()?;
    if palw_rule_e_commit_form_v1(params, point.daa_score) != PalwCommitFormV1::RuleEV3 {
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
    Some(palw_rule_e_open_window_above_with(&window, from_blue_score, record_leaf_v3, WINDOW_NODE_DOMAIN_V3, WINDOW_EMPTY_DOMAIN_V3))
}

/// The client's verifier of a v3 window opening — the records come back with their certification state.
pub fn verify_window_suffix_v3(
    leaf: &PalwForkChoiceLeafV3,
    from_blue_score: u64,
    proof: &PalwRuleEWindowOpeningV1,
) -> Result<Vec<PalwRuleEClaimRecordV1>, PalwRuleELeafV2ErrorV1> {
    palw_rule_e_verify_window_above_with(
        &leaf.ext(),
        from_blue_score,
        proof,
        record_leaf_v3,
        WINDOW_NODE_DOMAIN_V3,
        WINDOW_EMPTY_DOMAIN_V3,
    )
}

/// **A v3 registry opening for one bond** — entries `(bond, collateral)` `[lo, lo + entries.len())` and the proof.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwRuleERegistryOpeningV3 {
    pub lo: u64,
    pub entries: Vec<(PalwBondKeyV2, u64)>,
    pub proof: Vec<Hash64>,
}

/// The node's half: open `entries` (a state's [`palw_rule_e_registry_entries_v3`]) for `bond`.
pub fn palw_rule_e_open_registry_v3(entries: &[(PalwBondKeyV2, u64)], bond: &PalwBondKeyV2) -> PalwRuleERegistryOpeningV3 {
    let at = entries.partition_point(|(k, _)| k < bond);
    let (lo, hi) = if entries.get(at).is_some_and(|(k, _)| k == bond) {
        (at, at + 1)
    } else if entries.is_empty() {
        (0, 0)
    } else if at == 0 {
        (0, 1)
    } else if at == entries.len() {
        (at - 1, at)
    } else {
        (at - 1, at + 1)
    };
    let leaves: Vec<Hash64> = entries.iter().map(|(b, c)| capital_leaf_v3(b, *c)).collect();
    let mut proof = Vec::new();
    if lo < hi {
        prove_range(&leaves, lo, hi, REGISTRY_NODE_DOMAIN_V3, REGISTRY_EMPTY_DOMAIN_V3, &mut proof);
    }
    PalwRuleERegistryOpeningV3 { lo: lo as u64, entries: entries[lo..hi].to_vec(), proof }
}

/// **The client's half**: `Some(collateral)` if `bond` is in the registry leaf v3 commits, `None` if it is proven absent.
pub fn palw_rule_e_verify_registry_v3(
    leaf: &PalwForkChoiceLeafV3,
    bond: &PalwBondKeyV2,
    opening: &PalwRuleERegistryOpeningV3,
) -> Result<Option<u64>, PalwRuleELeafV2ErrorV1> {
    let ext = leaf.ext();
    let (n, lo, len) = (ext.registry_len as usize, opening.lo as usize, opening.entries.len());
    if n == 0 {
        let empty = keyed(REGISTRY_EMPTY_DOMAIN_V3, &[]);
        return if len == 0 && opening.proof.is_empty() && ext.registry_root == empty {
            Ok(None)
        } else {
            Err(PalwRuleELeafV2ErrorV1::RootMismatch)
        };
    }
    if len == 0 || len > 2 || lo + len > n {
        return Err(PalwRuleELeafV2ErrorV1::BadRange);
    }
    if opening.entries.windows(2).any(|w| w[0].0 >= w[1].0) {
        return Err(PalwRuleELeafV2ErrorV1::BadOrder);
    }
    let root = root_from_range(
        n,
        lo,
        lo + len,
        &mut opening.entries.iter().map(|(b, c)| capital_leaf_v3(b, *c)),
        &mut opening.proof.iter().copied(),
        REGISTRY_NODE_DOMAIN_V3,
        REGISTRY_EMPTY_DOMAIN_V3,
    )
    .ok_or(PalwRuleELeafV2ErrorV1::BadRange)?;
    if root != ext.registry_root {
        return Err(PalwRuleELeafV2ErrorV1::RootMismatch);
    }
    if let Some((_, capital)) = opening.entries.iter().find(|(k, _)| k == bond) {
        return if len == 1 { Ok(Some(*capital)) } else { Err(PalwRuleELeafV2ErrorV1::BadRange) };
    }
    let absent = match opening.entries.as_slice() {
        [(a, _), (b, _)] => a < bond && bond < b,
        [(only, _)] => (lo == 0 && bond < only) || (lo + 1 == n && only < bond),
        _ => false,
    };
    if absent { Ok(None) } else { Err(PalwRuleELeafV2ErrorV1::BadRange) }
}

/// **The client's evaluation of one pair under v3**: the two tips' verified v3 records above `F`, `F`'s proven capital per bond
/// (`fork_capital`; a bond not proven is not counted), `F`'s `registry_len` and capital total — through the node's own accumulator,
/// with Gate C applied from each record's certification state. Equal to the node's `PalwRuleEPairV1` (capital fields included).
pub fn palw_rule_e_pair_from_openings_v3(
    a: &[PalwRuleEClaimRecordV1],
    b: &[PalwRuleEClaimRecordV1],
    fork_capital: impl Fn(&PalwBondKeyV2) -> Option<u64>,
    fork: &PalwForkChoiceLeafV3,
) -> Result<PalwRuleEPairV1, PalwRuleELeafV2ErrorV1> {
    if a.len() > PALW_RULE_E_MAX_EXCLUSIVE_CLAIMS_V1 || b.len() > PALW_RULE_E_MAX_EXCLUSIVE_CLAIMS_V1 {
        return Err(PalwRuleELeafV2ErrorV1::TooMany);
    }
    let ids_a: BTreeSet<Hash64> = a.iter().map(|r| r.claim_id).collect();
    let ids_b: BTreeSet<Hash64> = b.iter().map(|r| r.claim_id).collect();
    let side = |records: &[PalwRuleEClaimRecordV1], other: &BTreeSet<Hash64>| {
        palw_rule_e_side_from_records_with_capital_v2(records.iter().filter(|r| !other.contains(&r.claim_id)), &fork_capital)
            .map_err(PalwRuleELeafV2ErrorV1::Side)
    };
    Ok(PalwRuleEPairV1 {
        a: side(a, &ids_b)?,
        b: side(b, &ids_a)?,
        even_split_min: palw_rule_e_even_split_min_v1(usize::try_from(fork.base.bonds_len).unwrap_or(usize::MAX)),
        even_split_min_capital_v2: crate::palw_rfc0018_v1::palw_rule_e_even_split_min_capital_v2(fork.registry_capital_total),
    })
}

#[cfg(test)]
mod tests;

/// Optional op-203 sub-openings. The caller chooses the fork's blue score and
/// the executor bonds whose membership/capital it needs at the fork.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwForkChoiceProofQueryV2 {
    pub from_blue_score: Option<u64>,
    pub bonds: Vec<PalwBondKeyV2>,
}

pub const PALW_FORK_CHOICE_MAX_PROOF_BONDS_V2: usize = 256;

impl PalwForkChoiceProofQueryV2 {
    pub fn validate(&self) -> Result<(), String> {
        if self.bonds.len() > PALW_FORK_CHOICE_MAX_PROOF_BONDS_V2 {
            return Err(format!("at most {PALW_FORK_CHOICE_MAX_PROOF_BONDS_V2} registry bonds per request"));
        }
        if self.bonds.iter().collect::<BTreeSet<_>>().len() != self.bonds.len() {
            return Err("duplicate registry bond in fork-choice request".into());
        }
        Ok(())
    }
}

/// Versioned data openings. Hashes authenticate data identity only; the client
/// still needs a trusted post-state root and applies the shared decision rule.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwForkChoiceProofV2 {
    RuleEV2 {
        window: Option<PalwRuleEWindowOpeningV1>,
        registry: Vec<(PalwBondKeyV2, crate::palw_fork_choice_rule_e_leaf_v2::PalwRuleERegistryOpeningV1)>,
    },
    RuleEV3 {
        window: Option<PalwRuleEWindowOpeningV1>,
        registry: Vec<(PalwBondKeyV2, PalwRuleERegistryOpeningV3)>,
    },
}

impl PalwForkChoiceProofV2 {
    pub fn of(state: &PalwChainStateV2, params: &Params, query: &PalwForkChoiceProofQueryV2) -> Result<Option<Self>, String> {
        use crate::palw_fork_choice_rule_e_leaf_v2::{prove_registry_membership_v1, prove_window_suffix_v1};
        query.validate()?;
        let point = state.last_point().ok_or("the genesis state has no fork-choice point")?;
        Ok(match palw_rule_e_commit_form_v1(params, point.daa_score) {
            PalwCommitFormV1::Flat | PalwCommitFormV1::V1 => None,
            PalwCommitFormV1::RuleEV2 => Some(Self::RuleEV2 {
                window: query
                    .from_blue_score
                    .map(|from| prove_window_suffix_v1(state, params, from).ok_or("window unavailable"))
                    .transpose()?,
                registry: query.bonds.iter().map(|b| (*b, prove_registry_membership_v1(state, b))).collect(),
            }),
            PalwCommitFormV1::RuleEV3 => {
                let entries = palw_rule_e_registry_entries_v3(state);
                Some(Self::RuleEV3 {
                    window: query
                        .from_blue_score
                        .map(|from| prove_window_suffix_v3(state, params, from).ok_or("window unavailable"))
                        .transpose()?,
                    registry: query.bonds.iter().map(|b| (*b, palw_rule_e_open_registry_v3(&entries, b))).collect(),
                })
            }
        })
    }
}

/// Authenticated Rule E inputs; `None` capital is a proven absent bond, while
/// an unrequested bond has no entry and must never be assumed absent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwForkChoiceProofViewV2 {
    pub records: Vec<PalwRuleEClaimRecordV1>,
    pub capital: std::collections::BTreeMap<PalwBondKeyV2, Option<u64>>,
}

impl PalwForkChoiceProofV2 {
    /// Call after authenticating `leaf` against a trusted root with the shared
    /// committed-leaf reader. Refuses missing, duplicate and unsolicited data.
    pub fn verify(&self, leaf: &[u8], query: &PalwForkChoiceProofQueryV2) -> Result<PalwForkChoiceProofViewV2, String> {
        use crate::palw_fork_choice_rule_e_leaf_v2::{verify_registry_membership_v1, verify_window_suffix_v1};
        use std::collections::BTreeMap;
        query.validate()?;
        let bad = |e| format!("fork-choice sub-opening does not verify: {e:?}");
        let mut capital = BTreeMap::new();
        let records = match self {
            Self::RuleEV2 { window, registry } => {
                let leaf = PalwForkChoiceLeafV2::decode(leaf).map_err(bad)?;
                let records = match (query.from_blue_score, window) {
                    (None, None) => Vec::new(),
                    (Some(from), Some(proof)) => verify_window_suffix_v1(&leaf, from, proof).map_err(bad)?,
                    _ => return Err("missing or unsolicited fork-choice window".into()),
                };
                for (bond, proof) in registry {
                    let present = verify_registry_membership_v1(&leaf, bond, proof).map_err(bad)?;
                    if capital.insert(*bond, present.then_some(0)).is_some() {
                        return Err("duplicate registry opening".into());
                    }
                }
                records
            }
            Self::RuleEV3 { window, registry } => {
                let leaf = PalwForkChoiceLeafV3::decode(leaf).map_err(bad)?;
                let records = match (query.from_blue_score, window) {
                    (None, None) => Vec::new(),
                    (Some(from), Some(proof)) => verify_window_suffix_v3(&leaf, from, proof).map_err(bad)?,
                    _ => return Err("missing or unsolicited fork-choice window".into()),
                };
                for (bond, proof) in registry {
                    let value = palw_rule_e_verify_registry_v3(&leaf, bond, proof).map_err(bad)?;
                    if capital.insert(*bond, value).is_some() {
                        return Err("duplicate registry opening".into());
                    }
                }
                records
            }
        };
        if capital.keys().copied().collect::<BTreeSet<_>>() != query.bonds.iter().copied().collect() {
            return Err("missing or unsolicited registry opening".into());
        }
        Ok(PalwForkChoiceProofViewV2 { records, capital })
    }
}
