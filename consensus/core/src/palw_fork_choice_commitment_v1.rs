//! **RFC-0009 L2 — the fork-choice commitment: what a header commits so a light client can read the comparator's inputs.**
//!
//! `palw_fork_choice::compare_palw_candidates_v1` orders candidates by `(safe frontier, safe weight, live total, hash)`. All of it is
//! already inside the ADR-0043 state root, but not *addressably*: the root is a flat preimage, `safe_weight` sits mid-preimage behind
//! Some-only blocks whose presence depends on the state, so no opening short of the whole state can say which bytes are the weight.
//! A client handed a "correct" root still could not read the order out of it.
//!
//! Past the dormant fence `Params::palw_fork_choice_commitment_v1` a header commits instead
//!
//! ```text
//!   palw_state_root = BLAKE2b-512_keyed("misaka-palw/state-root/fork-choice/v1", borsh(leaf) ‖ state.state_root())
//! ```
//!
//! where the leaf ([`PalwForkChoiceLeafV1`], 292 bytes) holds exactly what the fork-choice rules read — the state's point, the frontier,
//! the two weights — plus `bonds_len`, which bounds ADR-0065 D2's "bonds minted after the fork" (the registry is append-only), and the
//! **weight-allocation slot** ([`PalwWeightAllocationSlotV1`], ADR-0176 D3). An opening ([`PalwForkChoiceOpeningV1`]) is the leaf and
//! the ADR-0043 root: 356 bytes, O(1), and the ADR-0043 root it opens is the root an op-202 collection proof is checked against.
//!
//! **The weight-allocation slot (ADR-0176 D3).** Once `palw_bond_budget_v1` is armed, Final weight is bounded per bond, and every
//! reader — the node's fork choice, the RPC, a remote client — must read the SAME versioned allocation. The slot is where a leaf
//! commits which allocation its weights were read through (`version`, 0 = none in force: the fold's own weights), the budget-capped
//! comparator weights under it, and the root of the per-bond allocation. It is reserved now, fixed-size and inside the v1 leaf, so a
//! budget-capped weight is committed later WITHOUT a new envelope (state-root) version: only the slot's `version` moves. Every later
//! leaf version keeps it at its v1 offset. This build reads version 0 only; a leaf naming another version is refused, never guessed.
//!
//! **No model-availability condition (ADR-0177).** Nothing in the leaf, the slot or the opening says whether a registered model can
//! be fetched, served or seeded; the chain does not interfere with model acquisition, and no reader may weigh a candidate by it.
//!
//! **What moves and what does not.** Only what a header commits. `PalwChainStateV2::state_root()`, the carriage, the deltas and the
//! stores keep their bytes; no state field, delta, tail or preimage block is added. Every site that produces or checks a header's root
//! calls [`palw_committed_state_root_v1`], and below the fence it returns `state.state_root()` — byte-identical to every shipped chain.
//!
//! **The fence is read at the committed state's own point** (`last_point.daa_score`), not at the committing header, so the form is a
//! property of the state: every chain child of a block commits that block's post-state the same way.
//!
//! **What an opening proves, and what it does not.** That these values are the ones a root commits. Not that the root is the fold of the
//! branch's history (transition validity — the light client gets that from a trusted attestation or from re-execution), and not that
//! the branch wins (that is the comparator, run on verified inputs over every candidate the network shows).

use crate::config::params::{ForkActivation, Params};
use crate::palw_fork_choice::PalwCandidateOrderV1;
use crate::palw_mode_v2::PalwModeV2Error;
use crate::palw_state_v2::{PalwBlockContextV2, PalwChainStateV2, PalwDeltaEntryV2, PalwStateDeltaV2};
use crate::{BlockHash, Hash64};

/// The leaf layout version the client parses. A comparator that reads more ships a new version under its own fence.
pub const PALW_FORK_CHOICE_LEAF_VERSION_V1: u16 = 1;
/// The key of the envelope hash (BLAKE2b-512, keyed).
pub const PALW_FORK_CHOICE_DOMAIN_V1: &[u8] = b"misaka-palw/state-root/fork-choice/v1";
/// The borsh size of [`PalwWeightAllocationSlotV1`]: fixed.
pub const PALW_WEIGHT_ALLOCATION_SLOT_BYTES_V1: usize = 2 + 64 + 16 + 16;
/// The borsh size of [`PalwForkChoiceLeafV1`]: fixed, so a reader can refuse any other length.
pub const PALW_FORK_CHOICE_LEAF_BYTES_V1: usize = 2 + 64 + 8 + 8 + 8 + 64 + 16 + 16 + 8 + PALW_WEIGHT_ALLOCATION_SLOT_BYTES_V1;
/// The slot version that means "no bond-budget allocation is in force at this state's point": the comparator reads the fold's weights.
pub const PALW_WEIGHT_ALLOCATION_NONE_V1: u16 = 0;
/// The highest slot version this build reads. `palw_bond_budget_v1` (lane BUDGET, dormant) defines version 1 and raises this with it.
pub const PALW_WEIGHT_ALLOCATION_READ_MAX_V1: u16 = PALW_WEIGHT_ALLOCATION_NONE_V1;

/// **ADR-0176 D3: the weight-allocation slot of a fork-choice leaf** — which versioned bond-budget allocation the comparator's weights
/// were read through, and those weights.
///
/// * `version == 0` ([`PALW_WEIGHT_ALLOCATION_NONE_V1`], every state today): no allocation is in force at the state's point; every
///   other field is zero, and the comparator reads the leaf's own `safe_weight` / `bounded_immature` (the fold's).
/// * `version == n ≥ 1`: version `n` of `palw_bond_budget_v1`'s allocation is in force; `capped_safe_weight` /
///   `capped_bounded_immature` are the comparator's key 2 and key 3's addend as that allocation bounds them (the per-bond Final and
///   provisional weight ceilings applied), and `allocation_root` commits the per-bond allocation they were computed from, so a reader
///   can open one bond's allocation under the same header root. The node's fork choice, the RPC and every remote client read these —
///   one allocation, one version. What version `n` contains beyond these fields, and how `allocation_root` is built, is the budget
///   engine's; the leaf layout and the envelope do not change for it.
///
/// The slot carries bond-budget weights only — never a model-availability condition (ADR-0177).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwWeightAllocationSlotV1 {
    pub version: u16,
    pub allocation_root: Hash64,
    pub capped_safe_weight: u128,
    pub capped_bounded_immature: u128,
}

impl PalwWeightAllocationSlotV1 {
    /// No allocation in force: version 0, every field zero.
    pub const NONE: Self = Self {
        version: PALW_WEIGHT_ALLOCATION_NONE_V1,
        allocation_root: Hash64::from_bytes([0; 64]),
        capped_safe_weight: 0,
        capped_bounded_immature: 0,
    };

    /// Version 0 has exactly one encoding (every field zero), so two leaves of one state cannot differ in an unread field.
    pub fn is_canonical(&self) -> bool {
        self.version != PALW_WEIGHT_ALLOCATION_NONE_V1 || *self == Self::NONE
    }
}

/// **The fork-choice leaf of one post-state** — the comparator's inputs and the point they belong to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwForkChoiceLeafV1 {
    /// [`PALW_FORK_CHOICE_LEAF_VERSION_V1`].
    pub leaf_version: u16,
    /// The chain block whose post-state this is (`last_point.block`) — the candidate a client weighs.
    pub block: BlockHash,
    /// Its DAA score and blue score (`last_point`), so the client checks the leaf against the header it verified.
    pub daa_score: u64,
    pub blue_score: u64,
    /// Comparator key 1 and the frontier block.
    pub safe_frontier_blue_score: u64,
    pub safe_frontier: BlockHash,
    /// Comparator key 2.
    pub safe_weight: u128,
    /// Comparator key 3's addend (`live_total` is constructed by [`PalwCandidateOrderV1::new`], never carried).
    pub bounded_immature: u128,
    /// The bond registry's size. Append-only (ADR-0065 D5), so bonds minted on a branch after a fork are at most the difference of two
    /// leaves' counts — the bound a client uses to show ADR-0065 D2 cannot veto.
    pub bonds_len: u64,
    /// **ADR-0176 D3's slot** — the versioned bond-budget allocation the comparator's weights are read through
    /// ([`PalwWeightAllocationSlotV1::NONE`] until `palw_bond_budget_v1` is armed). Part of the v1 fields: every later leaf version keeps
    /// it at this offset.
    pub weight_allocation: PalwWeightAllocationSlotV1,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwForkChoiceErrorV1 {
    #[error("the state has no point (the genesis state): nothing is committed in the fork-choice form")]
    NoPoint,
    #[error("leaf version {0} is not {PALW_FORK_CHOICE_LEAF_VERSION_V1}")]
    LeafVersion(u16),
    #[error("the block's state predates the fork-choice commitment (DAA {daa} is below the fence): no opening exists")]
    BelowFence { daa: u64 },
    #[error("the opening is for block {got}, not {expected}")]
    WrongBlock { expected: BlockHash, got: BlockHash },
    #[error("the opening's point (DAA {got_daa}, blue {got_blue}) is not the header's (DAA {daa}, blue {blue})")]
    WrongPoint { daa: u64, blue: u64, got_daa: u64, got_blue: u64 },
    #[error("the opening does not hash to the committed root {0}")]
    RootMismatch(Hash64),
    #[error("the delta is not this leaf's block's delta: {0}")]
    DeltaMismatch(&'static str),
    #[error(
        "the leaf's weights are read through bond-budget allocation version {0}, which this build does not read (it reads up to \
         {PALW_WEIGHT_ALLOCATION_READ_MAX_V1}), or a version-0 slot is not all zero"
    )]
    WeightAllocation(u16),
}

impl PalwForkChoiceLeafV1 {
    /// The leaf of a post-state. `None` for a state with no point (the genesis state), which is never committed in this form.
    pub fn of(state: &PalwChainStateV2) -> Option<Self> {
        let point = state.last_point()?;
        let (safe_frontier_blue_score, safe_frontier) = state.safe_frontier();
        Some(Self {
            leaf_version: PALW_FORK_CHOICE_LEAF_VERSION_V1,
            block: point.block,
            daa_score: point.daa_score,
            blue_score: point.blue_score,
            safe_frontier_blue_score,
            safe_frontier,
            safe_weight: state.safe_weight(),
            bounded_immature: state.bounded_immature(),
            bonds_len: state.bonds_iter().count() as u64,
            // No bond-budget allocation exists in this tree. The merge that brings `palw_bond_budget_v1` fills the slot here (the one
            // place every producing and checking site builds a leaf through) for a state whose point stands past that fence.
            weight_allocation: PalwWeightAllocationSlotV1::NONE,
        })
    }

    /// The candidate order this leaf feeds the ONE comparator — built by the same constructor the node uses
    /// (`PalwChainStateV2::candidate_order`), so `live_total` is never read from anywhere. Under a slot version ≥ 1 the weights are the
    /// allocation's budget-capped ones (ADR-0176 D3); a reader refuses a version it does not read before it orders anything
    /// ([`PalwForkChoiceOpeningV1::verify`]).
    pub fn order(&self) -> PalwCandidateOrderV1 {
        let (safe, immature) = self.comparator_weights();
        PalwCandidateOrderV1::new(self.safe_frontier_blue_score, safe, immature, self.block)
    }

    /// The comparator's key 2 and key 3's addend: the fold's below any allocation, the slot's budget-capped ones under one.
    pub fn comparator_weights(&self) -> (u128, u128) {
        if self.weight_allocation.version == PALW_WEIGHT_ALLOCATION_NONE_V1 {
            (self.safe_weight, self.bounded_immature)
        } else {
            (self.weight_allocation.capped_safe_weight, self.weight_allocation.capped_bounded_immature)
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("a fork-choice leaf is borsh-serializable")
    }

    /// Decode exactly [`PALW_FORK_CHOICE_LEAF_BYTES_V1`] bytes of a version-1 leaf; anything else is refused.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != PALW_FORK_CHOICE_LEAF_BYTES_V1 {
            return None;
        }
        let leaf: Self = borsh::from_slice(bytes).ok()?;
        (leaf.leaf_version == PALW_FORK_CHOICE_LEAF_VERSION_V1 && leaf.weight_allocation.is_canonical()).then_some(leaf)
    }

    /// **The leaf of the PARENT's post-state, from this block's delta** — what a walk that holds only delta roots (RFC-0012's native
    /// settlement) needs to check a header past the fence. See [`PalwForkChoiceDeltaV1`].
    pub fn parent_by_delta(&self, delta: &PalwStateDeltaV2) -> Result<Self, PalwForkChoiceErrorV1> {
        self.parent_by(&PalwForkChoiceDeltaV1::of(delta))
    }

    /// The parent's leaf from the leaf-relevant part of this block's delta. Every `new` value the delta records must be this leaf's
    /// (a delta of another state is refused, never half-applied). A leaf whose weights are read through a bond-budget allocation
    /// (slot version ≥ 1) is refused: this build cannot rebuild an allocation from a delta, and a walk that cannot rebuild a leaf
    /// compares the flat form and so breaks rather than assumes.
    pub fn parent_by(&self, delta: &PalwForkChoiceDeltaV1) -> Result<Self, PalwForkChoiceErrorV1> {
        if self.weight_allocation != PalwWeightAllocationSlotV1::NONE {
            return Err(PalwForkChoiceErrorV1::WeightAllocation(self.weight_allocation.version));
        }
        if delta.block != self.block {
            return Err(PalwForkChoiceErrorV1::DeltaMismatch("the delta's point is another block"));
        }
        let Some((old_point, new_point)) = delta.last_point else {
            return Err(PalwForkChoiceErrorV1::DeltaMismatch("the delta carries no last point"));
        };
        if new_point.map(|p| p.block) != Some(self.block) {
            return Err(PalwForkChoiceErrorV1::DeltaMismatch("the delta's last point is not the leaf's block"));
        }
        let old_point = old_point.ok_or(PalwForkChoiceErrorV1::NoPoint)?;
        let mut parent = *self;
        parent.block = old_point.block;
        parent.daa_score = old_point.daa_score;
        parent.blue_score = old_point.blue_score;
        if let Some((old, new)) = delta.weights {
            if new != (self.safe_weight, self.bounded_immature) {
                return Err(PalwForkChoiceErrorV1::DeltaMismatch("the delta's weights are not the leaf's"));
            }
            (parent.safe_weight, parent.bounded_immature) = old;
        }
        if let Some((old, new)) = delta.frontier {
            if new != (self.safe_frontier_blue_score, self.safe_frontier) {
                return Err(PalwForkChoiceErrorV1::DeltaMismatch("the delta's frontier is not the leaf's"));
            }
            (parent.safe_frontier_blue_score, parent.safe_frontier) = old;
        }
        let parent_bonds = self.bonds_len as i128 - delta.bonds_added as i128;
        if parent_bonds < 0 {
            return Err(PalwForkChoiceErrorV1::DeltaMismatch("the delta adds more bonds than the leaf holds"));
        }
        parent.bonds_len = parent_bonds as u64;
        Ok(parent)
    }
}

/// **The part of a block's delta the fork-choice leaf reads** — small enough to keep beside a delta root in a memory-only row.
/// `LastPoint` is written for every block (the fold's last entry), `Weights` / `Frontier` only when they moved, and the bond registry
/// only grows (`bonds_added` counts insertions minus removals).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwForkChoiceDeltaV1 {
    pub block: BlockHash,
    pub last_point: Option<(Option<PalwBlockContextV2>, Option<PalwBlockContextV2>)>,
    pub weights: Option<((u128, u128), (u128, u128))>,
    pub frontier: Option<((u64, BlockHash), (u64, BlockHash))>,
    pub bonds_added: i64,
}

impl PalwForkChoiceDeltaV1 {
    pub fn of(delta: &PalwStateDeltaV2) -> Self {
        let mut out = Self { block: delta.point.block, last_point: None, weights: None, frontier: None, bonds_added: 0 };
        for entry in &delta.entries {
            match entry {
                PalwDeltaEntryV2::LastPoint { old, new } => out.last_point = Some((*old, *new)),
                PalwDeltaEntryV2::Weights { old, new } => out.weights = Some((*old, *new)),
                PalwDeltaEntryV2::Frontier { old, new } => out.frontier = Some((*old, *new)),
                PalwDeltaEntryV2::Bond { old: None, new: Some(_), .. } => out.bonds_added += 1,
                PalwDeltaEntryV2::Bond { old: Some(_), new: None, .. } => out.bonds_added -= 1,
                _ => {}
            }
        }
        out
    }
}

/// The envelope root: what a header commits for a post-state past the fence.
pub fn palw_fork_choice_envelope_root_v1(leaf: &PalwForkChoiceLeafV1, inner_root: &Hash64) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_FORK_CHOICE_DOMAIN_V1).to_state();
    state.update(&leaf.encode());
    state.update(inner_root.as_bytes().as_slice());
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// Is a post-state whose point stands at `point_daa` committed in the fork-choice form under `fence`?
/// `never()` and `None` are dormant; a state with no point never is.
pub fn palw_fork_choice_committed_at_v1(fence: Option<ForkActivation>, point_daa: Option<u64>) -> bool {
    match (fence, point_daa) {
        (Some(f), Some(daa)) => f != ForkActivation::never() && f.is_active(daa),
        _ => false,
    }
}

/// **The root a header commits for `state`** — THE helper every producing and checking site calls. Below the fence (and on every
/// shipped network, where it is `None`) this is `state.state_root()`, byte for byte.
pub fn palw_committed_state_root_v1(state: &PalwChainStateV2, fence: Option<ForkActivation>) -> Hash64 {
    let inner = state.state_root();
    if !palw_fork_choice_committed_at_v1(fence, state.last_point().map(|p| p.daa_score)) {
        return inner;
    }
    match PalwForkChoiceLeafV1::of(state) {
        Some(leaf) => palw_fork_choice_envelope_root_v1(&leaf, &inner),
        None => inner,
    }
}

/// [`palw_committed_state_root_v1`] when the caller already holds the leaf and the ADR-0043 root (a walk over delta roots).
pub fn palw_committed_root_of_parts_v1(leaf: &PalwForkChoiceLeafV1, inner_root: &Hash64, fence: Option<ForkActivation>) -> Hash64 {
    if palw_fork_choice_committed_at_v1(fence, Some(leaf.daa_score)) {
        palw_fork_choice_envelope_root_v1(leaf, inner_root)
    } else {
        *inner_root
    }
}

/// **An opening of the fork-choice commitment**: the leaf, and the ADR-0043 root it is enveloped with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwForkChoiceOpeningV1 {
    pub leaf: PalwForkChoiceLeafV1,
    pub inner_root: Hash64,
}

/// The header facts an opening is checked against — taken from a header the client verified, never from the opening.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwForkChoicePointV1 {
    pub block: BlockHash,
    pub daa_score: u64,
    pub blue_score: u64,
}

impl PalwForkChoiceOpeningV1 {
    /// The opening of a post-state (what a node serves). `None` for the genesis state.
    pub fn of(state: &PalwChainStateV2) -> Option<Self> {
        Some(Self { leaf: PalwForkChoiceLeafV1::of(state)?, inner_root: state.state_root() })
    }

    /// The root this opening commits to in the fork-choice form.
    pub fn committed_root(&self) -> Hash64 {
        palw_fork_choice_envelope_root_v1(&self.leaf, &self.inner_root)
    }

    /// **Verify an opening** against a root committed for `point`'s post-state (by an attestation, or by a chain child's header):
    /// the leaf's weight-allocation slot must be one this build reads, the state must be past the fence (by the client's own
    /// ruleset), the leaf must name `point` exactly, and the envelope must hash to `committed_root`. Returns the candidate order — the
    /// comparator's input, built the node's way. (A reader whose ruleset arms `palw_bond_budget_v1` must also refuse a version-0 slot at
    /// a point past that fence: the remote client does, `misaka-palw-remote::l2`.)
    pub fn verify(
        &self,
        committed_root: &Hash64,
        point: &PalwForkChoicePointV1,
        fence: Option<ForkActivation>,
    ) -> Result<PalwCandidateOrderV1, PalwForkChoiceErrorV1> {
        if self.leaf.leaf_version != PALW_FORK_CHOICE_LEAF_VERSION_V1 {
            return Err(PalwForkChoiceErrorV1::LeafVersion(self.leaf.leaf_version));
        }
        let slot = &self.leaf.weight_allocation;
        if slot.version > PALW_WEIGHT_ALLOCATION_READ_MAX_V1 || !slot.is_canonical() {
            return Err(PalwForkChoiceErrorV1::WeightAllocation(slot.version));
        }
        if !palw_fork_choice_committed_at_v1(fence, Some(point.daa_score)) {
            return Err(PalwForkChoiceErrorV1::BelowFence { daa: point.daa_score });
        }
        if self.leaf.block != point.block {
            return Err(PalwForkChoiceErrorV1::WrongBlock { expected: point.block, got: self.leaf.block });
        }
        if (self.leaf.daa_score, self.leaf.blue_score) != (point.daa_score, point.blue_score) {
            return Err(PalwForkChoiceErrorV1::WrongPoint {
                daa: point.daa_score,
                blue: point.blue_score,
                got_daa: self.leaf.daa_score,
                got_blue: self.leaf.blue_score,
            });
        }
        if self.committed_root() != *committed_root {
            return Err(PalwForkChoiceErrorV1::RootMismatch(*committed_root));
        }
        Ok(self.leaf.order())
    }
}

/// The most blocks one request may name (op 203): each is a walk from the node's PALW tip.
pub const PALW_FORK_CHOICE_MAX_BLOCKS_PER_REQUEST_V1: usize = 16;

/// **What a node serves for one block** (op 203): the block's header, the opening of its post-state, and the root that post-state is
/// committed as (in the form its point's fence says). Nothing here is trusted by a client until checked (`PalwForkChoiceOpeningV1::verify`
/// against an attested root, a chain child's header, or both).
#[derive(Clone, Debug)]
pub struct PalwForkChoiceServedEntryV1 {
    pub header: crate::header::Header,
    pub opening: PalwForkChoiceOpeningV1,
    pub committed_root: Hash64,
    /// True when the committed root is the envelope (the fence is in force at the post-state's point); false: the flat ADR-0043 root,
    /// and the opening proves nothing about it.
    pub committed_form: bool,
}

/// **A node's fork-choice view** (op 203): its sink and DAG tips — a peer that hides a tip is caught when another peer shows it — and the
/// openings of the blocks asked for (or of the sink and the tips).
#[derive(Clone, Debug)]
pub struct PalwForkChoiceServedV1 {
    pub sink: BlockHash,
    pub tips: Vec<BlockHash>,
    pub entries: Vec<(BlockHash, Result<PalwForkChoiceServedEntryV1, String>)>,
    /// This node's DNS BFT gate facts (ADR-0128 Decision 5) — what its deep-reorg gate would refuse on, ahead of the comparator.
    /// `None` on a network without the overlay. An issuer copies them into its attestation; a client never reads them from a peer.
    pub dns_gate: Option<PalwDnsGateFactV1>,
}

/// **The node-local fact the DNS BFT gate refuses on**: whether the overlay is in its `Active` stage and the DNS-final anchor it has
/// confirmed. The gate refuses only a candidate that abandons a confirmed anchor in the `Active` stage; in `Bootstrap`, or with nothing
/// confirmed, it never refuses and the comparator decides.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwDnsGateFactV1 {
    pub stage_active: bool,
    /// `(anchor block, its DAA)`; `None` with nothing confirmed.
    pub confirmed_anchor: Option<(BlockHash, u64)>,
}

impl Params {
    /// Whether a post-state whose point stands at `daa_score` is committed in the fork-choice form — never, in this binary's presets.
    pub fn palw_fork_choice_commitment_active_at(&self, daa_score: u64) -> bool {
        palw_fork_choice_committed_at_v1(self.palw_fork_choice_commitment_v1, Some(daa_score))
    }

    /// **The fence's refusal**: no release assigns it a height, so any armed height is refused at configuration. (A test arms it on a
    /// `Params` it builds directly; `validate_palw_v2` is the configuration path's gate.)
    pub fn validate_palw_fork_choice_commitment_v1(&self) -> Result<(), PalwModeV2Error> {
        match self.palw_fork_choice_commitment_v1 {
            Some(f) if f != ForkActivation::never() => Err(PalwModeV2Error::Invalid(
                "palw_fork_choice_commitment_v1 cannot be armed: RFC-0009 L2's commitment has no release height (it ships with the \
                 full-activation release, at a fresh height)",
            )),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::params::{MAINNET_PARAMS, TESTNET_PARAMS};
    use crate::palw_state_v2::{PalwConsensusObjectV2 as Obj, PalwPwuRuleV2, PalwStateParamsV2, apply_palw_transition_v2};
    use crate::tx::{TransactionId, TransactionOutpoint};

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }

    fn params() -> PalwStateParamsV2 {
        PalwStateParamsV2::new(100, 1, 1, 1, 500, 1_000, h(1), 4, 1_000, 100, 1_000, 0).unwrap()
    }

    /// A bond with its own key (the registry is one key, one bond).
    fn bond(n: u64) -> Obj {
        let mut pubkey = vec![7; 2592];
        pubkey[..8].copy_from_slice(&n.to_le_bytes());
        Obj::BondRegistered {
            bond: crate::palw_state_v2::PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(0xB0 + n), 0)),
            pubkey,
            operator_pubkey: n.to_le_bytes().to_vec(),
            collateral: 1 << 40,
            payout_payload: h(0x9A),
            capable_classes: Default::default(),
            signature: Vec::new(),
        }
    }

    /// Two chain blocks: genesis → B1 (a class and a bond) → B2 (another bond).
    fn two_blocks() -> (PalwChainStateV2, PalwChainStateV2, PalwStateDeltaV2, PalwChainStateV2, PalwStateDeltaV2) {
        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let class = Obj::ClassRegistered {
            class_id: h(1),
            artifact_root: h(0xA1),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(160),
            initial_target: u128::MAX / 2,
            share_permille: 1000,
            activation_daa: 0,
            admission: None,
        };
        let c1 = PalwBlockContextV2 { block: h(10), daa_score: 5, blue_score: 7, subsidy: 0 };
        let (s1, d1) = apply_palw_transition_v2(&genesis, &p, &c1, &[class, bond(0)], None).unwrap();
        let c2 = PalwBlockContextV2 { block: h(11), daa_score: 6, blue_score: 9, subsidy: 0 };
        let (s2, d2) = apply_palw_transition_v2(&s1, &p, &c2, &[bond(1)], None).unwrap();
        (genesis, s1, d1, s2, d2)
    }

    #[test]
    fn the_leaf_is_its_fixed_size_and_round_trips() {
        let (_, _, _, s2, _) = two_blocks();
        let leaf = PalwForkChoiceLeafV1::of(&s2).unwrap();
        assert_eq!(leaf.encode().len(), PALW_FORK_CHOICE_LEAF_BYTES_V1);
        assert_eq!(PALW_FORK_CHOICE_LEAF_BYTES_V1, 292, "194 bytes of keys and point, 98 of the ADR-0176 D3 slot");
        assert_eq!(borsh::to_vec(&PalwWeightAllocationSlotV1::NONE).unwrap().len(), PALW_WEIGHT_ALLOCATION_SLOT_BYTES_V1);
        assert_eq!(borsh::to_vec(&PalwForkChoiceOpeningV1::of(&s2).unwrap()).unwrap().len(), 356, "an opening: the leaf and 64 bytes");
        assert_eq!(leaf.weight_allocation, PalwWeightAllocationSlotV1::NONE, "no bond-budget allocation exists in this tree");
        assert_eq!(PalwForkChoiceLeafV1::decode(&leaf.encode()), Some(leaf));
        assert_eq!(PalwForkChoiceLeafV1::decode(&leaf.encode()[1..]), None, "a short leaf is refused");
        let mut v2 = leaf;
        v2.leaf_version = 2;
        assert_eq!(PalwForkChoiceLeafV1::decode(&v2.encode()), None, "another version is not parsed as this one");
        assert_eq!((leaf.block, leaf.daa_score, leaf.blue_score), (h(11), 6, 9));
        assert_eq!(leaf.bonds_len, 2);
        assert_eq!(leaf.order(), s2.candidate_order(h(11)), "the node's own constructor, the same order");
        assert_eq!(PalwForkChoiceLeafV1::of(&PalwChainStateV2::genesis()), None, "the genesis state has no point");
    }

    /// **Dormant is byte-identical**: with the fence `None`, `never()` or not yet reached, the header commits `state.state_root()`.
    #[test]
    fn a_dormant_fence_commits_exactly_the_adr_0043_root() {
        let (genesis, s1, _, s2, _) = two_blocks();
        for s in [&genesis, &s1, &s2] {
            assert_eq!(palw_committed_state_root_v1(s, None), s.state_root());
            assert_eq!(palw_committed_state_root_v1(s, Some(ForkActivation::never())), s.state_root());
            assert_eq!(palw_committed_state_root_v1(s, Some(ForkActivation::new(1_000))), s.state_root());
        }
        // Keyed on the state's own point: s1 stands at DAA 5, s2 at 6 — a fence at 6 commits s2 in the new form and s1 in the old.
        let f = Some(ForkActivation::new(6));
        assert_eq!(palw_committed_state_root_v1(&s1, f), s1.state_root());
        assert_ne!(palw_committed_state_root_v1(&s2, f), s2.state_root());
        assert_eq!(palw_committed_state_root_v1(&genesis, Some(ForkActivation::always())), genesis.state_root(), "no point, no leaf");
        for p in [MAINNET_PARAMS, TESTNET_PARAMS] {
            assert_eq!(p.palw_fork_choice_commitment_v1, None);
            assert!(!p.palw_fork_choice_commitment_active_at(u64::MAX));
            p.validate_palw_fork_choice_commitment_v1().unwrap();
        }
        let mut armed = TESTNET_PARAMS;
        armed.palw_fork_choice_commitment_v1 = Some(ForkActivation::new(10));
        assert!(armed.validate_palw_fork_choice_commitment_v1().is_err(), "refused when armed");
        armed.palw_fork_choice_commitment_v1 = Some(ForkActivation::never());
        armed.validate_palw_fork_choice_commitment_v1().unwrap();
    }

    /// **The envelope binds every key and the ADR-0043 root**, and an opening is checked against the header it claims to describe.
    #[test]
    fn an_opening_binds_every_key_to_its_block_and_its_root() {
        let (_, _, _, s2, _) = two_blocks();
        let f = Some(ForkActivation::new(1));
        let root = palw_committed_state_root_v1(&s2, f);
        let opening = PalwForkChoiceOpeningV1::of(&s2).unwrap();
        assert_eq!(opening.committed_root(), root);
        let point = PalwForkChoicePointV1 { block: h(11), daa_score: 6, blue_score: 9 };
        assert_eq!(opening.verify(&root, &point, f), Ok(s2.candidate_order(h(11))));
        // Each key moved alone breaks the root — the weight-allocation slot's fields included.
        let mutations: [fn(&mut PalwForkChoiceLeafV1); 12] = [
            |l| l.weight_allocation.version = 1,
            |l| l.weight_allocation.allocation_root = h(0xA0),
            |l| l.weight_allocation.capped_safe_weight = 1,
            |l| l.weight_allocation.capped_bounded_immature = 1,
            |l| l.safe_frontier_blue_score += 1,
            |l| l.safe_frontier = h(0xF),
            |l| l.safe_weight += 1,
            |l| l.bounded_immature += 1,
            |l| l.bonds_len += 1,
            |l| l.daa_score += 1,
            |l| l.blue_score += 1,
            |l| l.block = h(0xBB),
        ];
        for (i, m) in mutations.iter().enumerate() {
            let mut forged = opening;
            m(&mut forged.leaf);
            assert_ne!(forged.committed_root(), root, "mutation {i} must move the root");
            assert!(forged.verify(&root, &point, f).is_err(), "mutation {i} is refused");
        }
        let mut other_inner = opening;
        other_inner.inner_root = h(0x1234);
        assert_eq!(other_inner.verify(&root, &point, f), Err(PalwForkChoiceErrorV1::RootMismatch(root)));
        // The right opening for the wrong header: another block, or this block's hash at another point.
        let elsewhere = PalwForkChoicePointV1 { block: h(10), ..point };
        assert!(matches!(opening.verify(&root, &elsewhere, f), Err(PalwForkChoiceErrorV1::WrongBlock { .. })));
        let moved = PalwForkChoicePointV1 { blue_score: 10, ..point };
        assert!(matches!(opening.verify(&root, &moved, f), Err(PalwForkChoiceErrorV1::WrongPoint { .. })));
        // Below the fence no opening exists — the client must not accept one (the header commits the flat root there).
        assert!(matches!(opening.verify(&root, &point, Some(ForkActivation::new(7))), Err(PalwForkChoiceErrorV1::BelowFence { .. })));
        assert!(matches!(opening.verify(&root, &point, None), Err(PalwForkChoiceErrorV1::BelowFence { .. })));
        let mut wrong_version = opening;
        wrong_version.leaf.leaf_version = 9;
        assert_eq!(wrong_version.verify(&root, &point, f), Err(PalwForkChoiceErrorV1::LeafVersion(9)));
        // ADR-0176 D3: a slot version this build does not read is refused even when the envelope holds (an issuer could sign such a
        // root); so is a version-0 slot with a non-zero field (two encodings of "no allocation").
        let mut budgeted = opening;
        budgeted.leaf.weight_allocation =
            PalwWeightAllocationSlotV1 { version: 1, allocation_root: h(0xA0), capped_safe_weight: 1, capped_bounded_immature: 0 };
        assert_eq!(budgeted.verify(&budgeted.committed_root(), &point, f), Err(PalwForkChoiceErrorV1::WeightAllocation(1)));
        let mut unclean = opening;
        unclean.leaf.weight_allocation.capped_safe_weight = 7;
        assert_eq!(unclean.verify(&unclean.committed_root(), &point, f), Err(PalwForkChoiceErrorV1::WeightAllocation(0)));
        assert_eq!(PalwForkChoiceLeafV1::decode(&unclean.leaf.encode()), None, "a non-canonical slot does not decode");
        assert!(PalwForkChoiceLeafV1::decode(&budgeted.leaf.encode()).is_some(), "a versioned slot decodes; reading it is refused");
        // The parts form agrees with the state form on both sides of the fence.
        assert_eq!(palw_committed_root_of_parts_v1(&opening.leaf, &opening.inner_root, f), root);
        assert_eq!(palw_committed_root_of_parts_v1(&opening.leaf, &opening.inner_root, None), s2.state_root());
    }

    /// **ADR-0176 D3: the comparator reads the weights of the allocation the slot names** — the fold's at version 0, the budget-capped
    /// ones at a version ≥ 1 — through the leaf's one `order()`, so the node, the RPC and a client cannot read different weights.
    #[test]
    fn the_comparator_reads_the_weights_of_the_allocation_the_slot_names() {
        let (_, _, _, s2, _) = two_blocks();
        let mut leaf = PalwForkChoiceLeafV1::of(&s2).unwrap();
        leaf.safe_weight = 500;
        leaf.bounded_immature = 40;
        assert_eq!(leaf.comparator_weights(), (500, 40));
        assert_eq!(leaf.order(), PalwCandidateOrderV1::new(leaf.safe_frontier_blue_score, 500, 40, leaf.block));
        leaf.weight_allocation =
            PalwWeightAllocationSlotV1 { version: 1, allocation_root: h(0xA0), capped_safe_weight: 120, capped_bounded_immature: 9 };
        assert_eq!(leaf.comparator_weights(), (120, 9), "under an allocation the capped weights, never the fold's");
        assert_eq!(leaf.order(), PalwCandidateOrderV1::new(leaf.safe_frontier_blue_score, 120, 9, leaf.block));
        assert!(PalwWeightAllocationSlotV1::NONE.is_canonical());
        assert!(!PalwWeightAllocationSlotV1 { allocation_root: h(1), ..PalwWeightAllocationSlotV1::NONE }.is_canonical());
    }

    /// **The parent's leaf from the child's delta** — what the delta-root walk uses — equals the parent state's own leaf.
    #[test]
    fn the_parent_leaf_is_rebuilt_from_the_childs_delta() {
        let (_, s1, d1, s2, d2) = two_blocks();
        let l1 = PalwForkChoiceLeafV1::of(&s1).unwrap();
        let l2 = PalwForkChoiceLeafV1::of(&s2).unwrap();
        assert_eq!(l2.parent_by_delta(&d2), Ok(l1));
        // s1's parent is the genesis state, which has no point: named, not guessed.
        assert_eq!(l1.parent_by_delta(&d1), Err(PalwForkChoiceErrorV1::NoPoint));
        // Another block's delta is refused.
        assert!(matches!(l2.parent_by_delta(&d1), Err(PalwForkChoiceErrorV1::DeltaMismatch(_))));
        // A leaf whose weights the delta did not produce is refused.
        let mut forged = l2;
        forged.safe_weight += 1;
        let mut d2w = d2.clone();
        d2w.entries.push(PalwDeltaEntryV2::Weights { old: (0, 0), new: (5, 5) });
        assert!(matches!(forged.parent_by_delta(&d2w), Err(PalwForkChoiceErrorV1::DeltaMismatch(_))));
        // A leaf read through a bond-budget allocation is not rebuilt from a delta in this build: named, never half-rebuilt.
        let mut budgeted = l2;
        budgeted.weight_allocation.version = 1;
        assert_eq!(budgeted.parent_by_delta(&d2), Err(PalwForkChoiceErrorV1::WeightAllocation(1)));
    }
}
