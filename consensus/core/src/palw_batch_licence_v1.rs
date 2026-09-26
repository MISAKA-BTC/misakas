//! **ADR-0160 lane verify V1 — the batch licence** (`Params::palw_capacity_batch_licence`, F-B;
//! testnet-12 only, dormant on every shipped preset).
//!
//! # Why
//!
//! A coverage licence (`ReceiptLicensedV2`) carries one ML-DSA-87 signature per seat per claim: five
//! 4,627-byte signatures, a 125,768 transient-mass carrier, so a 500,000-mass block holds three and
//! the network licenses about 13 claims a DAA however much seat capital it has (the capacity map
//! §6.4). The signature is the cost, and it is paid per (seat, claim).
//!
//! # What
//!
//! A seat signs ONE window root per window (one DAA on the node) over every receipt it signed in it:
//! the root of a Merkle tree whose leaves are its V3 receipt messages, each bound to the claim's panel
//! anchor ([`palw_receipt_window_leaf_v1`]). A batch carries each root once
//! ([`PalwSeatWindowRootV1`], its signature over [`palw_receipt_window_message_v1`]) and, per claim,
//! each seat's receipt as its fields and a path ([`PalwBatchLicenceEntryV1`]). The per-claim cost
//! falls from five signatures to five paths (≈ 10.7k mass at the node's window sizes), so a block
//! holds ≈ 30 licences beside the roots (ADR-0160 V-T2).
//!
//! # The licensing rule is not restated here
//!
//! An entry is expanded into exactly the `Vec<PalwSeatReceiptV3>` a `ReceiptLicensedV2` of the same
//! receipts would carry ([`palw_batch_entry_receipts_v1`], signatures empty), and:
//!
//! * the acceptance layer runs the SAME validator on it — `validate_receipt_coverage_v2` for a
//!   `PanelBound` claim, `validate_supplementary_receipts_v3` for a licensed one past R-core+ — with a
//!   verifier that answers "signed" exactly for the (seat key, V3 message) pairs whose leaf the entry
//!   proved into a root whose signature verified ([`palw_validate_batch_licence_v1`]);
//! * the fold feeds the same receipts to the same arm the single object folds through
//!   (`apply_receipt_licensed_v2` in `palw_state_v2`), entry by entry, in order.
//!
//! So a batch entry licenses exactly as the single object would — ADR-0160 V-I1, tested as identical
//! state roots — and the carrier's mass is the unchanged block mass rule's (V-I2).
//!
//! # Cross-fork import is closed
//!
//! `palw_receipt_message_v3` binds no anchor (the judge's J-finding): a seat's `Valid` on claim `c` is
//! valid on every fork that drew the same seat for `c`. Each leaf here binds the panel anchor
//! (`anchor_hash`), and an entry whose anchor is not the claim's bound panel's is inert, so a window
//! root signed on one fork licenses nothing on another.
//!
//! # Robustness
//!
//! An entry whose claim is no longer licensable — voided, already licensed below R-core+, redrawn on
//! another anchor, missing — is INERT: skipped by acceptance and fold alike
//! ([`palw_batch_entry_route_v1`], one predicate on the same state), so a competing single licence of
//! one claim does not drop the other twenty-nine. Any other fault — a bad path, a mask the panel did
//! not assign, a root that does not verify, a live entry that does not license — refuses the whole
//! object, which is dropped with the block standing, exactly as a bad single licence is.
//!
//! # Attribution survives batching
//!
//! A seat that signed a window root holding a false `Valid` is convicted by a kind-3 filing carrying
//! the leaf, its path and the root's signature ([`PalwWindowedReceiptV1`], the
//! `PalwFalseValidReceiptV1::Windowed` form), accepted past the same fence.
//!
//! # Signing context
//!
//! [`PALW_RECEIPT_WINDOW_V1_MLDSA87_CONTEXT`] is not in testnet-12's committed context set (V5): that
//! set is inside the genesis ruleset id, and adding to it re-mints a live chain. Like the market's
//! contexts, it is covered instead by the Some-only fence that gates every object signed under it
//! (`palw_capacity_batch_licence` is in `consensus_params_id`), and it lives in its own registry
//! ([`PALW_BATCH_LICENCE_V1_ALL_DOMAINS`]) outside the acceptance families.

use kaspa_hashes::Hash64;

use crate::palw_panel_v2::{
    PALW_RECEIPT_V3_MLDSA87_CONTEXT, PalwPanelParamsV2, PalwReceiptQuorumV2, PalwReceiptVerdictV2, PalwSeatReceiptV2,
    PalwSeatReceiptV3, palw_receipt_message_v3,
};
use crate::palw_state_v2::{PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwStateParamsV2};
use crate::palw_verification_v2::PalwSegmentMaskV2;

/// Keyed-BLAKE2b-512 domain of a window leaf.
pub const PALW_RECEIPT_WINDOW_LEAF_DOMAIN_V1: &[u8] = b"misaka-palw/receipt-window/leaf/v1";
/// Keyed-BLAKE2b-512 domain of an interior node of a window tree.
pub const PALW_RECEIPT_WINDOW_NODE_DOMAIN_V1: &[u8] = b"misaka-palw/receipt-window/node/v1";
/// Keyed-BLAKE2b-512 domain of the message a seat signs over a window root.
pub const PALW_RECEIPT_WINDOW_MESSAGE_DOMAIN_V1: &[u8] = b"misaka-palw/receipt-window/message/v1";
/// ML-DSA-87 signing context of a window root.
pub const PALW_RECEIPT_WINDOW_V1_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/receipt-window/mldsa87/v1";

/// This family's domains (every one distinct from every other family's; see the module doc for why
/// it is not an acceptance family of the committed context set).
pub const PALW_BATCH_LICENCE_V1_ALL_DOMAINS: &[&[u8]] = &[
    PALW_RECEIPT_WINDOW_LEAF_DOMAIN_V1,
    PALW_RECEIPT_WINDOW_NODE_DOMAIN_V1,
    PALW_RECEIPT_WINDOW_MESSAGE_DOMAIN_V1,
    PALW_RECEIPT_WINDOW_V1_MLDSA87_CONTEXT,
];

/// The most window roots one batch carries. A root is ≈ 19k transient mass, so a block's 500,000
/// holds at most 26 anyway; the bound keeps a malformed object from asking for more verifies.
pub const PALW_BATCH_LICENCE_MAX_ROOTS_V1: usize = 64;
/// The most entries one batch carries (a block's mass holds ≈ 30 at Merkle-path sizes, ≈ 700 in the
/// ×100 design's full-list form).
pub const PALW_BATCH_LICENCE_MAX_ENTRIES_V1: usize = 1_024;
/// The most receipts one entry carries (a panel's seats; testnet-12 draws five).
pub const PALW_BATCH_LICENCE_MAX_SEATS_V1: usize = 32;
/// The most leaves a window tree holds, so a path is at most 20 hashes.
pub const PALW_RECEIPT_WINDOW_MAX_LEAVES_V1: u32 = 1 << 20;
/// The most DAA one window spans (`to_daa − from_daa`). The node signs one DAA a window.
pub const PALW_RECEIPT_WINDOW_MAX_SPAN_DAA_V1: u64 = 64;

fn keyed(domain: &[u8]) -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(64).key(domain).to_state()
}

fn finish(state: blake2b_simd::State) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **One seat's signed window root.** `root` is the Merkle root of `count` leaves
/// ([`palw_receipt_window_root_v1`]); `signature` is the seat's ML-DSA-87 signature, under its
/// registered key and [`PALW_RECEIPT_WINDOW_V1_MLDSA87_CONTEXT`], over
/// [`palw_receipt_window_message_v1`]. Every leaf proved into it must carry a `signed_daa` inside
/// `[from_daa, to_daa]`.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwSeatWindowRootV1 {
    pub seat_bond: PalwBondKeyV2,
    pub from_daa: u64,
    pub to_daa: u64,
    pub root: Hash64,
    pub count: u32,
    pub signature: Vec<u8>,
}

/// **One seat's receipt inside a batch entry**: the receipt's fields (the seat is the panel's seat
/// `seat_index`; the claim and anchor are the entry's), the root it is proved into (`root_index` into
/// the batch's roots, which must be that seat's), and its place and path in that root's tree.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwBatchSeatReceiptV1 {
    pub seat_index: u8,
    pub root_index: u16,
    pub verdict: PalwReceiptVerdictV2,
    pub mask: PalwSegmentMaskV2,
    pub signed_daa: u64,
    pub leaf_index: u32,
    pub path: Vec<Hash64>,
}

/// **One claim's licence (or SR-10 supplementary set) inside a batch.** `anchor_hash` must be the
/// claim's bound panel's anchor, or the entry is inert ([`palw_batch_entry_route_v1`]).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwBatchLicenceEntryV1 {
    pub claim: Hash64,
    pub anchor_hash: Hash64,
    pub seats: Vec<PalwBatchSeatReceiptV1>,
}

/// **A leaf**: `H(leaf-domain ‖ palw_receipt_message_v3(…) ‖ anchor)`. The V3 message already binds
/// the network domain, the claim, the verdict with its obligation, the DAA and the mask; the anchor is
/// what it lacked.
pub fn palw_receipt_window_leaf_v1(v3_message: Hash64, anchor_hash: Hash64) -> Hash64 {
    let mut state = keyed(PALW_RECEIPT_WINDOW_LEAF_DOMAIN_V1);
    state.update(v3_message.as_byte_slice());
    state.update(anchor_hash.as_byte_slice());
    finish(state)
}

/// The leaf of one receipt, from its fields.
pub fn palw_receipt_window_leaf_of_v1(
    network_domain: Hash64,
    claim: Hash64,
    verdict: PalwReceiptVerdictV2,
    signed_daa: u64,
    mask: PalwSegmentMaskV2,
    anchor_hash: Hash64,
) -> Hash64 {
    palw_receipt_window_leaf_v1(palw_receipt_message_v3(network_domain, claim, verdict, signed_daa, mask), anchor_hash)
}

fn node(left: &Hash64, right: &Hash64) -> Hash64 {
    let mut state = keyed(PALW_RECEIPT_WINDOW_NODE_DOMAIN_V1);
    state.update(left.as_byte_slice());
    state.update(right.as_byte_slice());
    finish(state)
}

/// **The root of a window tree.** Pairs are hashed left to right; the odd last node of a level is
/// promoted unchanged (no duplication, so no second preimage of a shorter list: `count` is signed and
/// every path is checked against it). `None` for an empty list.
pub fn palw_receipt_window_root_v1(leaves: &[Hash64]) -> Option<Hash64> {
    if leaves.is_empty() {
        return None;
    }
    let mut level: Vec<Hash64> = leaves.to_vec();
    while level.len() > 1 {
        level = level.chunks(2).map(|pair| if pair.len() == 2 { node(&pair[0], &pair[1]) } else { pair[0] }).collect();
    }
    Some(level[0])
}

/// **The path of leaf `index`**: its sibling at each level where it has one, bottom-up.
pub fn palw_receipt_window_path_v1(leaves: &[Hash64], index: usize) -> Option<Vec<Hash64>> {
    if index >= leaves.len() {
        return None;
    }
    let mut path = Vec::new();
    let mut level: Vec<Hash64> = leaves.to_vec();
    let mut idx = index;
    while level.len() > 1 {
        let sibling = idx ^ 1;
        if sibling < level.len() {
            path.push(level[sibling]);
        }
        level = level.chunks(2).map(|pair| if pair.len() == 2 { node(&pair[0], &pair[1]) } else { pair[0] }).collect();
        idx /= 2;
    }
    Some(path)
}

/// **Fold a leaf up its path** in a tree of `count` leaves; `None` for an index out of range, a
/// count past [`PALW_RECEIPT_WINDOW_MAX_LEAVES_V1`], or a path of the wrong length.
pub fn palw_receipt_window_fold_v1(leaf: Hash64, index: u32, count: u32, path: &[Hash64]) -> Option<Hash64> {
    if count == 0 || index >= count || count > PALW_RECEIPT_WINDOW_MAX_LEAVES_V1 {
        return None;
    }
    let mut acc = leaf;
    let (mut idx, mut width) = (index as u64, count as u64);
    let mut siblings = path.iter();
    while width > 1 {
        if idx % 2 == 1 {
            acc = node(siblings.next()?, &acc);
        } else if idx + 1 < width {
            acc = node(&acc, siblings.next()?);
        }
        idx /= 2;
        width = width.div_ceil(2);
    }
    siblings.next().is_none().then_some(acc)
}

/// **What a seat signs over a window**: `H(message-domain ‖ network ‖ seat ‖ from ‖ to ‖ root ‖
/// count)`. The seat is inside, so a root cannot be re-attributed to another bond's key; the count is
/// inside, so a path cannot claim a shorter or longer tree.
pub fn palw_receipt_window_message_v1(
    network_domain: Hash64,
    seat_bond: &PalwBondKeyV2,
    from_daa: u64,
    to_daa: u64,
    root: Hash64,
    count: u32,
) -> Hash64 {
    let mut state = keyed(PALW_RECEIPT_WINDOW_MESSAGE_DOMAIN_V1);
    state.update(network_domain.as_byte_slice());
    state.update(&borsh::to_vec(seat_bond).expect("a bond key is borsh-serializable"));
    state.update(&from_daa.to_le_bytes());
    state.update(&to_daa.to_le_bytes());
    state.update(root.as_byte_slice());
    state.update(&count.to_le_bytes());
    finish(state)
}

/// **How an entry routes on a state** — the one predicate acceptance and the fold share.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwBatchEntryRouteV1 {
    /// Skipped: the claim is gone, is not at a phase this batch can move, or its bound panel is not on
    /// the entry's anchor (a redraw, another fork).
    Inert,
    /// A `PanelBound` claim: the entry is a coverage licence.
    Licence,
    /// A licensed claim past `palw_rcore_plus`: the entry is an SR-10 supplementary set.
    Supplementary,
}

/// **The entry's route** at `daa_score` on `state`: [`PalwBatchEntryRouteV1`].
pub fn palw_batch_entry_route_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    daa_score: u64,
    entry: &PalwBatchLicenceEntryV1,
) -> PalwBatchEntryRouteV1 {
    let (Some(claim), Some(panel)) = (state.claim(&entry.claim), state.panel(&entry.claim)) else {
        return PalwBatchEntryRouteV1::Inert;
    };
    if panel.anchor != entry.anchor_hash {
        return PalwBatchEntryRouteV1::Inert;
    }
    match claim.phase {
        PalwClaimPhaseV2::PanelBound { .. } => PalwBatchEntryRouteV1::Licence,
        PalwClaimPhaseV2::ReceiptLicensed { .. } if params.rcore_plus_active_at(daa_score) => PalwBatchEntryRouteV1::Supplementary,
        _ => PalwBatchEntryRouteV1::Inert,
    }
}

/// **The receipts an entry stands for**, in its order, exactly as a `ReceiptLicensedV2` of the same
/// receipts would carry them — with EMPTY signatures: what vouches for each is its path into a
/// verified root, and nothing downstream of acceptance reads a receipt's signature bytes. `None` if a
/// seat index is not one of `seats`.
pub fn palw_batch_entry_receipts_v1(entry: &PalwBatchLicenceEntryV1, seats: &[PalwBondKeyV2]) -> Option<Vec<PalwSeatReceiptV3>> {
    entry
        .seats
        .iter()
        .map(|seat| {
            let bond = *seats.get(seat.seat_index as usize)?;
            Some(PalwSeatReceiptV3 {
                receipt: PalwSeatReceiptV2 {
                    claim: entry.claim,
                    verdict: seat.verdict,
                    seat_bond: bond,
                    signed_daa: seat.signed_daa,
                    signature: Vec::new(),
                },
                segments: seat.mask,
            })
        })
        .collect()
}

/// Why a batch is refused. The object is dropped with the block standing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwBatchLicenceErrorV1 {
    /// Below `palw_capacity_batch_licence` (or Verification V2).
    Dormant,
    /// A shape bound: empty, over a limit, a duplicate root or claim.
    Shape(&'static str),
    /// A root names a bond this chain does not hold.
    RootBondMissing(PalwBondKeyV2),
    /// A root's signature does not verify under its seat's registered key.
    RootSignatureInvalid(PalwBondKeyV2),
    /// A live entry's receipt does not prove into its root.
    Proof { claim: Hash64, why: &'static str },
    /// A live entry does not license (or credit) as the single object would not.
    Entry { claim: Hash64, why: String },
}

impl std::fmt::Display for PalwBatchLicenceErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dormant => write!(f, "a batch licence below palw_capacity_batch_licence (ADR-0160 F-B)"),
            Self::Shape(why) => write!(f, "a batch licence's shape is refused: {why}"),
            Self::RootBondMissing(bond) => write!(f, "a window root names bond {bond:?} this chain does not hold"),
            Self::RootSignatureInvalid(bond) => write!(f, "a window root is not signed by the bond it names ({bond:?})"),
            Self::Proof { claim, why } => write!(f, "claim {claim}: a batched receipt does not prove into its root: {why}"),
            Self::Entry { claim, why } => write!(f, "claim {claim}: the batched receipts do not license: {why}"),
        }
    }
}

/// What an accepted batch will do, entry by entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwBatchLicenceSummaryV1 {
    /// Entries that license a `PanelBound` claim.
    pub licences: Vec<Hash64>,
    /// Entries that are SR-10 supplementary sets.
    pub supplementary: Vec<Hash64>,
    /// Entries skipped as inert.
    pub inert: Vec<Hash64>,
}

/// **The shape bounds**, stateless: non-empty, within the limits, no root twice, no claim twice, every
/// window well-formed.
pub fn palw_batch_licence_shape_v1(roots: &[PalwSeatWindowRootV1], entries: &[PalwBatchLicenceEntryV1]) -> Result<(), PalwBatchLicenceErrorV1> {
    use PalwBatchLicenceErrorV1::Shape;
    if roots.is_empty() || entries.is_empty() {
        return Err(Shape("no root or no entry"));
    }
    if roots.len() > PALW_BATCH_LICENCE_MAX_ROOTS_V1 {
        return Err(Shape("more roots than PALW_BATCH_LICENCE_MAX_ROOTS_V1"));
    }
    if entries.len() > PALW_BATCH_LICENCE_MAX_ENTRIES_V1 {
        return Err(Shape("more entries than PALW_BATCH_LICENCE_MAX_ENTRIES_V1"));
    }
    for (i, root) in roots.iter().enumerate() {
        if root.count == 0 || root.count > PALW_RECEIPT_WINDOW_MAX_LEAVES_V1 {
            return Err(Shape("a window's leaf count is zero or past PALW_RECEIPT_WINDOW_MAX_LEAVES_V1"));
        }
        if root.from_daa > root.to_daa || root.to_daa - root.from_daa > PALW_RECEIPT_WINDOW_MAX_SPAN_DAA_V1 {
            return Err(Shape("a window is inverted or spans more than PALW_RECEIPT_WINDOW_MAX_SPAN_DAA_V1"));
        }
        if roots[..i].iter().any(|other| other.seat_bond == root.seat_bond && other.root == root.root) {
            return Err(Shape("a window root twice"));
        }
    }
    let mut claims = std::collections::BTreeSet::new();
    for entry in entries {
        if !claims.insert(entry.claim) {
            return Err(Shape("a claim twice"));
        }
        if entry.seats.is_empty() || entry.seats.len() > PALW_BATCH_LICENCE_MAX_SEATS_V1 {
            return Err(Shape("an entry with no receipt or more than PALW_BATCH_LICENCE_MAX_SEATS_V1"));
        }
        for seat in &entry.seats {
            if seat.root_index as usize >= roots.len() {
                return Err(Shape("a receipt names a root the batch does not carry"));
            }
            if seat.path.len() > 20 {
                return Err(Shape("a path longer than PALW_RECEIPT_WINDOW_MAX_LEAVES_V1's depth"));
            }
        }
    }
    Ok(())
}

/// **Verify every root's signature, once** — the one ML-DSA-87 check per (seat, window) the batch
/// exists for.
pub fn palw_batch_licence_verify_roots_v1<V>(
    state: &PalwChainStateV2,
    network_domain: Hash64,
    roots: &[PalwSeatWindowRootV1],
    verify_mldsa87: V,
) -> Result<(), PalwBatchLicenceErrorV1>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    for root in roots {
        let bond = state.bond(&root.seat_bond).ok_or(PalwBatchLicenceErrorV1::RootBondMissing(root.seat_bond))?;
        let message = palw_receipt_window_message_v1(network_domain, &root.seat_bond, root.from_daa, root.to_daa, root.root, root.count);
        if !verify_mldsa87(&bond.pubkey, message.as_byte_slice(), &root.signature, PALW_RECEIPT_WINDOW_V1_MLDSA87_CONTEXT) {
            return Err(PalwBatchLicenceErrorV1::RootSignatureInvalid(root.seat_bond));
        }
    }
    Ok(())
}

/// **The (seat key, V3 message) pairs a live entry proves**, each receipt's leaf folded up its path to
/// its root: the root must be the receipt's seat's, the receipt's DAA inside the root's window.
fn palw_batch_entry_proofs_v1<'s>(
    state: &'s PalwChainStateV2,
    network_domain: Hash64,
    roots: &[PalwSeatWindowRootV1],
    entry: &PalwBatchLicenceEntryV1,
    seats: &[PalwBondKeyV2],
) -> Result<Vec<(&'s [u8], Hash64)>, PalwBatchLicenceErrorV1> {
    let refused = |why| PalwBatchLicenceErrorV1::Proof { claim: entry.claim, why };
    let mut proven = Vec::with_capacity(entry.seats.len());
    for seat in &entry.seats {
        let bond = seats.get(seat.seat_index as usize).ok_or_else(|| refused("a seat index the panel does not have"))?;
        let root = roots.get(seat.root_index as usize).ok_or_else(|| refused("a root the batch does not carry"))?;
        if root.seat_bond != *bond {
            return Err(refused("the receipt's root is another seat's"));
        }
        if seat.signed_daa < root.from_daa || seat.signed_daa > root.to_daa {
            return Err(refused("the receipt's DAA is outside its root's window"));
        }
        let message = palw_receipt_message_v3(network_domain, entry.claim, seat.verdict, seat.signed_daa, seat.mask);
        let leaf = palw_receipt_window_leaf_v1(message, entry.anchor_hash);
        if palw_receipt_window_fold_v1(leaf, seat.leaf_index, root.count, &seat.path) != Some(root.root) {
            return Err(refused("the path does not fold to the root"));
        }
        let key = state.bond(bond).ok_or_else(|| refused("the seat's bond is gone"))?;
        proven.push((key.pubkey.as_slice(), message));
    }
    Ok(proven)
}

/// **The acceptance layer's whole check of a batch** — the processor calls it past F-B with the
/// chain's ML-DSA verifier; the fold never needs it (it re-derives every structural fact, and routes
/// by [`palw_batch_entry_route_v1`] on the state it folds).
///
/// 1. the fence (the state params' mirror) and Verification V2;
/// 2. the shape ([`palw_batch_licence_shape_v1`]);
/// 3. every root's signature, once ([`palw_batch_licence_verify_roots_v1`]);
/// 4. per entry, its route; for a live one, every receipt proved into its seat's root, then the SAME
///    validator the single object meets — `validate_receipt_coverage_v2` (it must answer `Licensed`)
///    or `validate_supplementary_receipts_v3` (`Supplementary`) — whose signature question is answered
///    by the proofs of step 4.
#[allow(clippy::too_many_arguments)]
pub fn palw_validate_batch_licence_v1<V>(
    state: &PalwChainStateV2,
    panel_params: &PalwPanelParamsV2,
    state_params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    network_domain: Hash64,
    verification_v2_active: bool,
    roots: &[PalwSeatWindowRootV1],
    entries: &[PalwBatchLicenceEntryV1],
    verify_mldsa87: V,
    unavailable_abstains: bool,
    independence_daa: Option<u64>,
) -> Result<PalwBatchLicenceSummaryV1, PalwBatchLicenceErrorV1>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    if !state_params.capacity_batch_active_at(ctx.daa_score) || !verification_v2_active {
        return Err(PalwBatchLicenceErrorV1::Dormant);
    }
    palw_batch_licence_shape_v1(roots, entries)?;
    palw_batch_licence_verify_roots_v1(state, network_domain, roots, &verify_mldsa87)?;
    palw_validate_batch_entries_v1(state, panel_params, state_params, ctx, network_domain, roots, entries, unavailable_abstains, independence_daa)
}

/// **Step 4 of [`palw_validate_batch_licence_v1`] alone, over roots whose signatures the caller
/// already verified** — what a node's assembler asks of each candidate entry
/// ([`palw_assemble_batch_licence_v1`]), so it never offers an entry acceptance would refuse.
#[allow(clippy::too_many_arguments)]
pub fn palw_validate_batch_entries_v1(
    state: &PalwChainStateV2,
    panel_params: &PalwPanelParamsV2,
    state_params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    network_domain: Hash64,
    roots: &[PalwSeatWindowRootV1],
    entries: &[PalwBatchLicenceEntryV1],
    unavailable_abstains: bool,
    independence_daa: Option<u64>,
) -> Result<PalwBatchLicenceSummaryV1, PalwBatchLicenceErrorV1> {
    let mut summary = PalwBatchLicenceSummaryV1::default();
    for entry in entries {
        let route = palw_batch_entry_route_v1(state, state_params, ctx.daa_score, entry);
        if route == PalwBatchEntryRouteV1::Inert {
            summary.inert.push(entry.claim);
            continue;
        }
        let seats: Vec<PalwBondKeyV2> =
            state.panel(&entry.claim).map(|panel| panel.seats.iter().map(|seat| seat.bond).collect()).unwrap_or_default();
        let proven = palw_batch_entry_proofs_v1(state, network_domain, roots, entry, &seats)?;
        let receipts = palw_batch_entry_receipts_v1(entry, &seats)
            .ok_or(PalwBatchLicenceErrorV1::Proof { claim: entry.claim, why: "a seat index the panel does not have" })?;
        // The single object's signature question, answered by the proofs: "signed" exactly for a
        // (key, V3 message) pair this entry proved into a verified root, and only under the V3 context.
        let proved = |pk: &[u8], msg: &[u8], _signature: &[u8], context: &[u8]| {
            context == PALW_RECEIPT_V3_MLDSA87_CONTEXT && proven.iter().any(|(key, message)| *key == pk && message.as_byte_slice() == msg)
        };
        let entry_refused = |why: String| PalwBatchLicenceErrorV1::Entry { claim: entry.claim, why };
        match route {
            PalwBatchEntryRouteV1::Licence => {
                match crate::palw_panel_v2::validate_receipt_coverage_v2(
                    state,
                    panel_params,
                    state_params,
                    ctx,
                    network_domain,
                    &entry.claim,
                    &receipts,
                    proved,
                    unavailable_abstains,
                    independence_daa,
                ) {
                    Ok(PalwReceiptQuorumV2::Licensed { .. }) => summary.licences.push(entry.claim),
                    Ok(other) => return Err(entry_refused(format!("the receipts answer {other:?}, not a licence"))),
                    Err(e) => return Err(entry_refused(e.to_string())),
                }
            }
            PalwBatchEntryRouteV1::Supplementary => {
                match crate::palw_panel_v2::validate_supplementary_receipts_v3(
                    state,
                    state_params,
                    ctx,
                    network_domain,
                    &entry.claim,
                    &receipts,
                    proved,
                ) {
                    Ok(PalwReceiptQuorumV2::Supplementary { .. }) => summary.supplementary.push(entry.claim),
                    Ok(other) => return Err(entry_refused(format!("the receipts answer {other:?}, not a supplementary set"))),
                    Err(e) => return Err(entry_refused(e.to_string())),
                }
            }
            PalwBatchEntryRouteV1::Inert => unreachable!("inert entries were skipped above"),
        }
    }
    Ok(summary)
}

// ---------------------------------------------------------------------------------------------------
// The node's side: a seat's window, and the assembler (node policy — never validity).
// ---------------------------------------------------------------------------------------------------

/// **One receipt as a seat's window holds it** — the fields its leaf commits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwWindowLeafV1 {
    pub claim: Hash64,
    pub anchor_hash: Hash64,
    pub verdict: PalwReceiptVerdictV2,
    pub signed_daa: u64,
    pub mask: PalwSegmentMaskV2,
}

impl PalwWindowLeafV1 {
    pub fn leaf(&self, network_domain: Hash64) -> Hash64 {
        palw_receipt_window_leaf_of_v1(network_domain, self.claim, self.verdict, self.signed_daa, self.mask, self.anchor_hash)
    }
}

/// **A seat's signed window** — its root and the leaves under it, as the seat publishes them and a
/// collector pools them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwSeatWindowV1 {
    pub root: PalwSeatWindowRootV1,
    pub leaves: Vec<PalwWindowLeafV1>,
}

impl PalwSeatWindowV1 {
    /// **Build and sign a window** over `leaves` (a seat's receipts of `[from_daa, to_daa]`, in the
    /// order it chooses). `sign` is the seat's ML-DSA-87 signer over the window message under
    /// [`PALW_RECEIPT_WINDOW_V1_MLDSA87_CONTEXT`]. `None` for an empty window.
    pub fn sign(
        network_domain: Hash64,
        seat_bond: PalwBondKeyV2,
        from_daa: u64,
        to_daa: u64,
        leaves: Vec<PalwWindowLeafV1>,
        sign: impl FnOnce(&[u8]) -> Vec<u8>,
    ) -> Option<Self> {
        let hashes: Vec<Hash64> = leaves.iter().map(|leaf| leaf.leaf(network_domain)).collect();
        let root = palw_receipt_window_root_v1(&hashes)?;
        let count = u32::try_from(hashes.len()).ok()?;
        let message = palw_receipt_window_message_v1(network_domain, &seat_bond, from_daa, to_daa, root, count);
        let signature = sign(message.as_byte_slice());
        Some(Self { root: PalwSeatWindowRootV1 { seat_bond, from_daa, to_daa, root, count, signature }, leaves })
    }

    /// The batched receipt of leaf `index` (its path computed here), for seat `seat_index` of the
    /// claim's panel, proved into root `root_index` of the batch.
    fn batched(&self, network_domain: Hash64, index: usize, seat_index: u8, root_index: u16) -> Option<PalwBatchSeatReceiptV1> {
        let hashes: Vec<Hash64> = self.leaves.iter().map(|leaf| leaf.leaf(network_domain)).collect();
        let leaf = self.leaves.get(index)?;
        Some(PalwBatchSeatReceiptV1 {
            seat_index,
            root_index,
            verdict: leaf.verdict,
            mask: leaf.mask,
            signed_daa: leaf.signed_daa,
            leaf_index: u32::try_from(index).ok()?,
            path: palw_receipt_window_path_v1(&hashes, index)?,
        })
    }
}

/// The borsh size of a batch object as it would ride (roots and entries).
pub fn palw_batch_licence_bytes_v1(roots: &[PalwSeatWindowRootV1], entries: &[PalwBatchLicenceEntryV1]) -> usize {
    borsh::object_length(&(roots, entries)).unwrap_or(usize::MAX)
}

/// **The collector's batch** (node policy; ADR-0160 V1): the claims of `due`, in the order given (the
/// V04 order, oldest bind first — [`crate::palw_state_v2`]'s readers and `kaspad`'s
/// `palw_licence_claim_order_v1` produce it), each with every seat's `Valid` leaf the pooled
/// `windows` hold for it on its bound panel's anchor, kept only where acceptance would take the entry
/// ([`palw_validate_batch_entries_v1`] on this one entry), where `licenses` says the fold would
/// license it (the processor asks the fold of the equivalent single object, so an inert — unbacked —
/// set is never re-offered), and while the object stays within `max_bytes`. Windows a kept entry needs
/// are added as roots, once. `None` when nothing is kept.
///
/// The windows' signatures are the caller's to have verified (a node checks a seat's window when it
/// pools it); a root that does not verify drops the whole object at acceptance.
#[allow(clippy::too_many_arguments)]
pub fn palw_assemble_batch_licence_v1(
    state: &PalwChainStateV2,
    panel_params: &PalwPanelParamsV2,
    state_params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    network_domain: Hash64,
    windows: &[PalwSeatWindowV1],
    due: &[Hash64],
    max_bytes: usize,
    unavailable_abstains: bool,
    independence_daa: Option<u64>,
    licenses: impl Fn(&Hash64, &[PalwSeatReceiptV3]) -> bool,
) -> Option<(Vec<PalwSeatWindowRootV1>, Vec<PalwBatchLicenceEntryV1>)> {
    // (seat, claim, anchor) → (window index, leaf index) of that seat's Valid leaf, first window first.
    let mut index: std::collections::HashMap<(PalwBondKeyV2, Hash64), (usize, usize)> = std::collections::HashMap::new();
    for (w, window) in windows.iter().enumerate() {
        for (l, leaf) in window.leaves.iter().enumerate() {
            if leaf.verdict == PalwReceiptVerdictV2::Valid {
                index.entry((window.root.seat_bond, leaf.claim)).or_insert((w, l));
            }
        }
    }
    let mut roots: Vec<PalwSeatWindowRootV1> = Vec::new();
    let mut root_of_window: std::collections::HashMap<usize, u16> = std::collections::HashMap::new();
    let mut entries: Vec<PalwBatchLicenceEntryV1> = Vec::new();
    for claim in due {
        let Some(panel) = state.panel(claim) else { continue };
        let probe_entry = |roots: &[PalwSeatWindowRootV1], root_of_window: &std::collections::HashMap<usize, u16>| {
            let mut new_roots: Vec<(usize, PalwSeatWindowRootV1)> = Vec::new();
            let mut seats = Vec::new();
            for (seat_index, seat) in panel.seats.iter().enumerate() {
                let Some(&(w, l)) = index.get(&(seat.bond, *claim)) else { continue };
                let window = &windows[w];
                if window.leaves[l].anchor_hash != panel.anchor {
                    continue;
                }
                let root_index = match root_of_window.get(&w) {
                    Some(r) => *r,
                    None => match new_roots.iter().position(|(nw, _)| *nw == w) {
                        Some(p) => (roots.len() + p) as u16,
                        None => {
                            new_roots.push((w, window.root.clone()));
                            (roots.len() + new_roots.len() - 1) as u16
                        }
                    },
                };
                seats.push(window.batched(network_domain, l, u8::try_from(seat_index).ok()?, root_index)?);
            }
            (!seats.is_empty()).then(|| (PalwBatchLicenceEntryV1 { claim: *claim, anchor_hash: panel.anchor, seats }, new_roots))
        };
        let Some((entry, new_roots)) = probe_entry(&roots, &root_of_window) else { continue };
        let mut trial_roots = roots.clone();
        trial_roots.extend(new_roots.iter().map(|(_, root)| root.clone()));
        let live = palw_validate_batch_entries_v1(
            state,
            panel_params,
            state_params,
            ctx,
            network_domain,
            &trial_roots,
            std::slice::from_ref(&entry),
            unavailable_abstains,
            independence_daa,
        );
        if !live.is_ok_and(|summary| summary.inert.is_empty()) {
            continue;
        }
        let seats: Vec<PalwBondKeyV2> = panel.seats.iter().map(|seat| seat.bond).collect();
        if !palw_batch_entry_receipts_v1(&entry, &seats).is_some_and(|receipts| licenses(claim, &receipts)) {
            continue;
        }
        let mut trial_entries = entries.clone();
        trial_entries.push(entry);
        if palw_batch_licence_bytes_v1(&trial_roots, &trial_entries) > max_bytes {
            // Oldest first: a later claim may still fit where this one did not (fewer new roots), so
            // keep walking, but never reorder what was taken.
            continue;
        }
        for (w, _) in &new_roots {
            root_of_window.insert(*w, roots.len() as u16);
            roots.push(windows[*w].root.clone());
        }
        entries = trial_entries;
    }
    (!entries.is_empty()).then_some((roots, entries))
}

// ---------------------------------------------------------------------------------------------------
// Attribution: a false `Valid` inside a window root.
// ---------------------------------------------------------------------------------------------------

/// **A batched receipt as a kind-3 filing carries it** (`PalwFalseValidReceiptV1::Windowed`): the V3
/// receipt it stands for — `receipt.signature` is the window ROOT's signature — with the anchor its
/// leaf bound, the window it was signed in and its path. What the seat signed is
/// [`Self::signed_message_v1`]: the window message over the root the leaf folds to, so a path that
/// does not fold to the signed root yields a message the signature does not verify under.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwWindowedReceiptV1 {
    pub receipt: PalwSeatReceiptV3,
    pub anchor_hash: Hash64,
    pub from_daa: u64,
    pub to_daa: u64,
    pub count: u32,
    pub leaf_index: u32,
    pub path: Vec<Hash64>,
}

impl PalwWindowedReceiptV1 {
    /// The message the seat signed (under [`PALW_RECEIPT_WINDOW_V1_MLDSA87_CONTEXT`]): the window
    /// message over the root this receipt's leaf folds to. A leaf outside the window, or a path that
    /// does not fold, answers the all-zero hash, which no signature verifies under.
    pub fn signed_message_v1(&self, chain_domain: Hash64) -> Hash64 {
        let inner = &self.receipt.receipt;
        if inner.signed_daa < self.from_daa || inner.signed_daa > self.to_daa || self.to_daa - self.from_daa > PALW_RECEIPT_WINDOW_MAX_SPAN_DAA_V1
        {
            return Hash64::default();
        }
        let leaf = palw_receipt_window_leaf_of_v1(chain_domain, inner.claim, inner.verdict, inner.signed_daa, self.receipt.segments, self.anchor_hash);
        match palw_receipt_window_fold_v1(leaf, self.leaf_index, self.count, &self.path) {
            Some(root) => palw_receipt_window_message_v1(chain_domain, &inner.seat_bond, self.from_daa, self.to_daa, root, self.count),
            None => Hash64::default(),
        }
    }

    /// **The windowed form of seat `seat_bond`'s receipt in a batch entry** — what a filer reads off a
    /// carried batch for the claim's seat at `seat_index` (the panel's), or `None` when the entry
    /// carries no receipt of that seat.
    pub fn of_batch_entry(
        roots: &[PalwSeatWindowRootV1],
        entry: &PalwBatchLicenceEntryV1,
        seat_index: u8,
        seat_bond: PalwBondKeyV2,
    ) -> Option<Self> {
        let seat = entry.seats.iter().find(|seat| seat.seat_index == seat_index)?;
        let root = roots.get(seat.root_index as usize)?;
        (root.seat_bond == seat_bond).then(|| Self {
            receipt: PalwSeatReceiptV3 {
                receipt: PalwSeatReceiptV2 {
                    claim: entry.claim,
                    verdict: seat.verdict,
                    seat_bond,
                    signed_daa: seat.signed_daa,
                    signature: root.signature.clone(),
                },
                segments: seat.mask,
            },
            anchor_hash: entry.anchor_hash,
            from_daa: root.from_daa,
            to_daa: root.to_daa,
            count: root.count,
            leaf_index: seat.leaf_index,
            path: seat.path.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    #[test]
    fn every_path_folds_to_the_root_and_nothing_else_does() {
        for count in 1usize..=33 {
            let leaves: Vec<Hash64> = (0..count as u64).map(|i| h(0x1EAF_0000 + i)).collect();
            let root = palw_receipt_window_root_v1(&leaves).unwrap();
            for index in 0..count {
                let path = palw_receipt_window_path_v1(&leaves, index).unwrap();
                assert!(path.len() <= 6, "count {count}: a path of at most ⌈log2 count⌉ hashes");
                assert_eq!(palw_receipt_window_fold_v1(leaves[index], index as u32, count as u32, &path), Some(root), "{count}/{index}");
                // Another leaf, another index, another count, a longer or shorter path: never the root.
                assert_ne!(palw_receipt_window_fold_v1(h(0xBAD), index as u32, count as u32, &path), Some(root));
                if count > 1 {
                    let other = (index + 1) % count;
                    assert_ne!(palw_receipt_window_fold_v1(leaves[index], other as u32, count as u32, &path), Some(root));
                }
                assert_eq!(palw_receipt_window_fold_v1(leaves[index], count as u32, count as u32, &path), None, "index out of range");
                let mut long = path.clone();
                long.push(h(1));
                assert_eq!(palw_receipt_window_fold_v1(leaves[index], index as u32, count as u32, &long), None);
                if !path.is_empty() {
                    assert_eq!(palw_receipt_window_fold_v1(leaves[index], index as u32, count as u32, &path[1..]), None);
                }
            }
        }
        assert_eq!(palw_receipt_window_root_v1(&[]), None);
        assert_eq!(palw_receipt_window_fold_v1(h(1), 0, 0, &[]), None);
        assert_eq!(palw_receipt_window_fold_v1(h(1), 0, PALW_RECEIPT_WINDOW_MAX_LEAVES_V1 + 1, &[]), None);
        // A one-leaf tree's root is its leaf, and its path is empty.
        assert_eq!(palw_receipt_window_root_v1(&[h(7)]), Some(h(7)));
    }

    /// The leaf binds the anchor and every field of the V3 message; the window message binds the
    /// seat, the window, the root and the count.
    #[test]
    fn the_leaf_binds_the_anchor_and_the_message_binds_the_seat_and_count() {
        let (d, c) = (h(0xD0), h(0xC1A1));
        let base = palw_receipt_window_leaf_of_v1(d, c, PalwReceiptVerdictV2::Valid, 10, PalwSegmentMaskV2(0b11), h(0xA1));
        for other in [
            palw_receipt_window_leaf_of_v1(d, c, PalwReceiptVerdictV2::Valid, 10, PalwSegmentMaskV2(0b11), h(0xA2)),
            palw_receipt_window_leaf_of_v1(d, c, PalwReceiptVerdictV2::Valid, 11, PalwSegmentMaskV2(0b11), h(0xA1)),
            palw_receipt_window_leaf_of_v1(d, c, PalwReceiptVerdictV2::Valid, 10, PalwSegmentMaskV2(0b01), h(0xA1)),
            palw_receipt_window_leaf_of_v1(d, c, PalwReceiptVerdictV2::Incapable, 10, PalwSegmentMaskV2(0b11), h(0xA1)),
            palw_receipt_window_leaf_of_v1(d, h(0xC1A2), PalwReceiptVerdictV2::Valid, 10, PalwSegmentMaskV2(0b11), h(0xA1)),
            palw_receipt_window_leaf_of_v1(h(0xD1), c, PalwReceiptVerdictV2::Valid, 10, PalwSegmentMaskV2(0b11), h(0xA1)),
        ] {
            assert_ne!(other, base);
        }
        let seat = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(0x5EA7), 0));
        let other_seat = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(0x5EA7), 1));
        let m = palw_receipt_window_message_v1(d, &seat, 5, 5, h(0x200), 3);
        for other in [
            palw_receipt_window_message_v1(d, &other_seat, 5, 5, h(0x200), 3),
            palw_receipt_window_message_v1(d, &seat, 4, 5, h(0x200), 3),
            palw_receipt_window_message_v1(d, &seat, 5, 6, h(0x200), 3),
            palw_receipt_window_message_v1(d, &seat, 5, 5, h(0x201), 3),
            palw_receipt_window_message_v1(d, &seat, 5, 5, h(0x200), 4),
            palw_receipt_window_message_v1(h(0xD1), &seat, 5, 5, h(0x200), 3),
        ] {
            assert_ne!(other, m);
        }
        // The family's domains are distinct from each other and from the receipt families'.
        let mut all: Vec<&[u8]> = PALW_BATCH_LICENCE_V1_ALL_DOMAINS.to_vec();
        all.extend(crate::palw_panel_v2::PALW_PANEL_V2_ALL_DOMAINS);
        all.push(crate::palw_panel_v2::PALW_RECEIPT_V3_DOMAIN_MESSAGE);
        all.push(crate::palw_panel_v2::PALW_RECEIPT_V3_MLDSA87_CONTEXT);
        let distinct: std::collections::BTreeSet<&[u8]> = all.iter().copied().collect();
        assert_eq!(distinct.len(), all.len());
        assert!(!crate::palw_mode_v2::PALW_V2_SIGNATURE_CONTEXTS_COMPLETE_V5.contains(&PALW_RECEIPT_WINDOW_V1_MLDSA87_CONTEXT), "not in t12's committed set: the Some-only fence covers it");
    }

    #[test]
    fn a_windowed_receipt_signs_the_root_its_leaf_folds_to() {
        let d = h(0xD0);
        let seat = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(0x5EA7), 2));
        let leaves: Vec<PalwWindowLeafV1> = (0..5u64)
            .map(|i| PalwWindowLeafV1 {
                claim: h(0xC000 + i),
                anchor_hash: h(0xA000 + i),
                verdict: PalwReceiptVerdictV2::Valid,
                signed_daa: 40,
                mask: PalwSegmentMaskV2(1 << i),
            })
            .collect();
        let window = PalwSeatWindowV1::sign(d, seat, 40, 40, leaves.clone(), |m| m.to_vec()).unwrap();
        // The "signature" is the message itself here: what the seat signed is recomputed exactly.
        for (i, leaf) in leaves.iter().enumerate() {
            let batched = window.batched(d, i, 3, 0).unwrap();
            let entry = PalwBatchLicenceEntryV1 { claim: leaf.claim, anchor_hash: leaf.anchor_hash, seats: vec![batched] };
            let windowed = PalwWindowedReceiptV1::of_batch_entry(std::slice::from_ref(&window.root), &entry, 3, seat).unwrap();
            assert_eq!(windowed.signed_message_v1(d).as_byte_slice(), window.root.signature.as_slice());
            assert_eq!(windowed.receipt.receipt.seat_bond, seat);
            // Another seat's root is not this seat's receipt.
            assert!(PalwWindowedReceiptV1::of_batch_entry(std::slice::from_ref(&window.root), &entry, 3, PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(1), 0))).is_none());
            // A lie about the anchor, the mask or the DAA folds elsewhere.
            for edit in 0..3 {
                let mut forged = windowed.clone();
                match edit {
                    0 => forged.anchor_hash = h(0xFFFF),
                    1 => forged.receipt.segments = PalwSegmentMaskV2(0xF),
                    _ => forged.receipt.receipt.signed_daa = 41,
                }
                assert_ne!(forged.signed_message_v1(d).as_byte_slice(), window.root.signature.as_slice(), "edit {edit}");
            }
        }
    }
}
