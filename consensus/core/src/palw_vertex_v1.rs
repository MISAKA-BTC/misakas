//! **RFC-0007 Part I and Part III — verification vertices, licence by tally, equivocation, `Held` leaves, and the
//! security parameters** (`Params::palw_verification_vertex_v1`; dormant on every shipped preset). Spec
//! `docs/spec/palw/18-verification-certificates.md` is normative.
//!
//! # What a vertex is
//!
//! A seat signs **one vertex per round**: the Merkle root of every verdict it reached that round, with the verdicts
//! themselves carried as leaves ([`PalwVertexLeafV1`]). The chain carries a vertex once, as a consensus object
//! (`PalwConsensusObjectV2::VerificationVertexV1`, tag 91). The fold **tallies** its `Verdict` leaves: a leaf counts
//! toward its claim under exactly the conditions `validate_receipt_coverage_v2` applies to a receipt today
//! ([`palw_vertex_leaf_fate_v1`]), and when a claim's counted `Valid` leaves reach the panel's quorum the fold applies
//! the licence itself — **the licence is the tally**, and no licence object is carried. The licensing rule is not
//! restated: the counted leaves are expanded into the very `Vec<PalwSeatReceiptV3>` a `ReceiptLicensedV2` of the same
//! seats would carry ([`palw_vertex_receipts_of_v1`]) and fed to the same arm.
//!
//! # Signing
//!
//! [`palw_vertex_message_v1`] binds the network, the seat bond, the round, the signing DAA, the leaf count and the leaf
//! root, so a relayer can neither truncate a vertex nor move it to another round. The leaves are hashed under their own
//! domain ([`palw_vertex_leaf_hash_v1`]) and folded by [`palw_vertex_root_v1`] (odd nodes promoted).
//!
//! # Equivocation
//!
//! Two validly signed vertices of one `(seat_bond, round)` with different roots are an equivocation
//! ([`PalwVertexEquivocationV1`], object tag 92). No court is needed: two signatures over one round are the whole proof.
//! The fold slashes [`PALW_VERTEX_EQUIVOCATION_PENALTY_PERMILLE_V1`] ‰ of the bond, forfeits every lock the seat holds
//! on a claim either vertex names, and ejects the bond (a forced retirement).
//!
//! # Part III
//!
//! [`PalwCheckModeV1`], [`palw_vertex_m_v1`], [`palw_vertex_escape_ppm_v1`], [`palw_vertex_required_slash_multiple_ppm_v1`]
//! and [`palw_vertex_escalation_v1`] state the security model in numbers: `m` independent checkers per interval, the
//! escape probability of a one-point lie, the multiple of the gain a slash must be, and what a seat does when its own
//! check fails. The consensus fold reads none of them to decide a block; they are the one place the producer and the
//! verifier of a rule read the same constant, and the RFC's tables are tests over them.

use crate::Hash64;
use crate::config::params::{ForkActivation, PalwPostLaunchFenceV1, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
use crate::palw_panel_v2::PalwReceiptVerdictV2;
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwStateParamsV2};
use crate::palw_verification_v2::PalwSegmentMaskV2;

// ---------------------------------------------------------------------------------------------
// Domains and constants
// ---------------------------------------------------------------------------------------------

/// Keyed-BLAKE2b-512 domain of the message a seat signs over a vertex.
pub const PALW_VERTEX_MESSAGE_DOMAIN_V1: &[u8] = b"misaka-palw/verification-vertex-message/v1";
/// Keyed-BLAKE2b-512 domain of a leaf hash.
pub const PALW_VERTEX_LEAF_DOMAIN_V1: &[u8] = b"misaka-palw/verification-vertex-leaf/v1";
/// Keyed-BLAKE2b-512 domain of an interior node of the leaf tree.
pub const PALW_VERTEX_NODE_DOMAIN_V1: &[u8] = b"misaka-palw/verification-vertex-node/v1";
/// ML-DSA-87 signing context of a vertex.
pub const PALW_VERTEX_MLDSA87_CONTEXT_V1: &[u8] = b"misaka-palw/verification-vertex/mldsa87/v1";
/// Keyed-BLAKE2b-512 domain of an equivocation offence key (one conviction per `(seat, round)`).
pub const PALW_VERTEX_EQUIVOCATION_KEY_DOMAIN_V1: &[u8] = b"misaka-palw/vertex-equivocation-key/v1";

/// This family's domains. Like the batch licence's, they are covered by the Some-only fence that gates every object signed
/// under them (`palw_verification_vertex_v1` is in `consensus_params_id`), so they are not in testnet-12's committed context
/// set (which sits inside the genesis ruleset id); the uniqueness test runs this registry against every other family.
pub const PALW_VERTEX_V1_ALL_DOMAINS: &[&[u8]] = &[
    PALW_VERTEX_MESSAGE_DOMAIN_V1,
    PALW_VERTEX_LEAF_DOMAIN_V1,
    PALW_VERTEX_NODE_DOMAIN_V1,
    PALW_VERTEX_MLDSA87_CONTEXT_V1,
    PALW_VERTEX_EQUIVOCATION_KEY_DOMAIN_V1,
];

/// The vertex wire version.
pub const PALW_VERTEX_VERSION_V1: u16 = 1;
/// **`round_daa`: one DAA** (the lead's decision of 2026-10-03; RFC-0007 open question 1). A vertex's round is
/// `signed_daa / round_daa`; one DAA keeps licence latency where receipts have it, and a seat signs no vertex mid-round.
pub const PALW_VERTEX_ROUND_DAA_V1: u64 = 1;
/// **The per-vertex leaf cap** (RFC-0007 §I.2): a vertex stays inside one carrier.
pub const PALW_VERTEX_MAX_LEAVES_V1: usize = 1_024;
/// The most bytes of leaves a vertex carries (RFC-0007 §I.2).
pub const PALW_VERTEX_MAX_LEAF_BYTES_V1: usize = 80_000;
/// The longest a signed vertex may wait to land: `ctx.daa_score − signed_daa ≤` this. It bounds how long the chain must
/// remember a `(seat, round)` so that "the first accepted vertex of a round" is a question the state can answer.
pub const PALW_VERTEX_MAX_CARRY_DAA_V1: u64 = 240;
/// How long past its round an equivocation stays provable: the rows of a round are kept this long, and evidence about an older
/// round is refused by name (it could not be told apart from a row that was pruned).
pub const PALW_VERTEX_EVIDENCE_WINDOW_DAA_V1: u64 = 1_200;
/// The most `(round, seat)` rows one block's sweep removes.
pub const PALW_VERTEX_SWEEP_PER_BLOCK_V1: usize = 256;
/// **The equivocation penalty: 100 ‰ of the bond** (the lead's decision of 2026-10-03; RFC-0007 open question 3) — a new act in
/// the ADR-0152 per-act table, beside the locks it forfeits and the ejection it brings.
pub const PALW_VERTEX_EQUIVOCATION_PENALTY_PERMILLE_V1: u16 = 100;
/// **A DA certificate's quorum `q`** (RFC-0007 §I.7): distinct seats of the claim's panel with equal `Held` leaves.
pub const PALW_VERTEX_DA_QUORUM_V1: u16 = 3;
/// The most `Held` rows one claim keeps (three objects times a panel's seats, with room).
pub const PALW_VERTEX_HELD_MAX_PER_CLAIM_V1: usize = 64;
/// **The `Held` exposure** an attester loses, per claim, when the chain concludes the claim's data was not served: this many
/// ‰ of its collateral (RFC-0007 §I.7, "slashed its `Held` exposure").
pub const PALW_VERTEX_HELD_EXPOSURE_PERMILLE_V1: u16 = 5;

/// `Held`'s `object`: the capture (the committed rows, `TirCaptureV1`).
pub const PALW_VERTEX_HELD_OBJECT_CAPTURE_V1: u8 = 0;
/// `Held`'s `object`: the witness (Part II).
pub const PALW_VERTEX_HELD_OBJECT_WITNESS_V1: u8 = 1;
/// `Held`'s `object`: a trace-manifest chunk.
pub const PALW_VERTEX_HELD_OBJECT_TRACE_MANIFEST_V1: u8 = 2;

// ---------------------------------------------------------------------------------------------
// The objects
// ---------------------------------------------------------------------------------------------

/// **How a leaf names its claim** (RFC-0007 §I.3): the whole id (64 bytes), or the DAA its panel bound at and a 16-byte prefix
/// of the id (20). A prefix is unique among the claims bound at one DAA unless someone ground a 2^64 birthday collision; an
/// ambiguous or unknown reference is ignored by the fold, so a seat can only lose by using one wrongly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwClaimRefV1 {
    Full(Hash64) = 0,
    Compact { bound_daa: u32, id_prefix: [u8; 16] } = 1,
}

impl PalwClaimRefV1 {
    /// The compact reference of a claim bound at `bound_daa`; `None` where the DAA does not fit 32 bits.
    pub fn compact_of(claim: &Hash64, bound_daa: u64) -> Option<Self> {
        let bound_daa = u32::try_from(bound_daa).ok()?;
        let mut id_prefix = [0u8; 16];
        id_prefix.copy_from_slice(&claim.as_bytes()[..16]);
        Some(Self::Compact { bound_daa, id_prefix })
    }

    /// Does this reference name `claim`, whose panel bound at `bound_daa`?
    pub fn names(&self, claim: &Hash64, bound_daa: u64) -> bool {
        match self {
            Self::Full(id) => id == claim,
            Self::Compact { bound_daa: at, id_prefix } => u64::from(*at) == bound_daa && claim.as_bytes()[..16] == id_prefix[..],
        }
    }
}

/// **One thing a seat says in a round** (RFC-0007 §I.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwVertexLeafV1 {
    /// Today's receipt, unsigned: the verdict the seat reached on a claim. The segments it attests are not carried — a `Valid`
    /// attests exactly the seat's assigned mask, as `validate_receipt_coverage_v2` requires of a receipt.
    Verdict { claim: PalwClaimRefV1, verdict: PalwReceiptVerdictV2 } = 0,
    /// A DA attestation (§I.7): "I hold chunks `first..=last` of `object` of this claim, whose digest is `digest`, and I will
    /// serve them until the claim's challenge window closes."
    Held { claim: PalwClaimRefV1, object: u8, first: u32, last: u32, digest: Hash64 } = 1,
    /// A mesh audit (Part IV.1). Not armed by this fence: a vertex carrying one is refused by name until the audit mesh's own
    /// fence lands.
    Audited { claim: PalwClaimRefV1, leaf: u64, result: u8 } = 2,
}

impl PalwVertexLeafV1 {
    /// The leaf's kind tag (its borsh discriminant).
    pub fn tag(&self) -> u8 {
        match self {
            Self::Verdict { .. } => 0,
            Self::Held { .. } => 1,
            Self::Audited { .. } => 2,
        }
    }

    /// The claim the leaf names.
    pub fn claim_ref(&self) -> &PalwClaimRefV1 {
        match self {
            Self::Verdict { claim, .. } | Self::Held { claim, .. } | Self::Audited { claim, .. } => claim,
        }
    }

    /// **The leaf's sort key** (`kind ‖ claim ref ‖ the fields that tell two leaves of one kind and claim apart`): a vertex's
    /// leaves are strictly ascending by it, so a verdict is named once per claim and a leaf has one place.
    pub fn sort_key(&self) -> Vec<u8> {
        let mut key = vec![self.tag()];
        key.extend(borsh::to_vec(self.claim_ref()).expect("a claim reference serializes"));
        match self {
            Self::Verdict { .. } => {}
            Self::Held { object, first, last, .. } => {
                key.push(*object);
                key.extend(first.to_be_bytes());
                key.extend(last.to_be_bytes());
            }
            Self::Audited { leaf, .. } => key.extend(leaf.to_be_bytes()),
        }
        key
    }

    /// The leaf's encoded size.
    pub fn encoded_len(&self) -> usize {
        borsh::to_vec(self).expect("a leaf serializes").len()
    }
}

/// **A verification vertex** (RFC-0007 §I.2): one seat's signed statement of everything it decided in one round.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwVerificationVertexV1 {
    /// [`PALW_VERTEX_VERSION_V1`].
    pub version: u16,
    /// The signer; its registered ML-DSA-87 key verifies.
    pub seat_bond: PalwBondKeyV2,
    /// `signed_daa / PALW_VERTEX_ROUND_DAA_V1`.
    pub round: u64,
    pub signed_daa: u64,
    /// Strictly ascending by [`PalwVertexLeafV1::sort_key`]; at most [`PALW_VERTEX_MAX_LEAVES_V1`] leaves and
    /// [`PALW_VERTEX_MAX_LEAF_BYTES_V1`] bytes.
    pub leaves: Vec<PalwVertexLeafV1>,
    /// The binary Merkle root over [`palw_vertex_leaf_hash_v1`] of each leaf in order ([`palw_vertex_root_v1`]).
    pub leaves_root: Hash64,
    /// ML-DSA-87 over [`palw_vertex_message_v1`] under [`PALW_VERTEX_MLDSA87_CONTEXT_V1`].
    pub signature: Vec<u8>,
}

/// **A vertex without its leaves** — what the signature covers beside the count, and what equivocation evidence needs of a
/// vertex (≈ 4.9 KB against up to 80 KB).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwVertexHeaderV1 {
    pub version: u16,
    pub seat_bond: PalwBondKeyV2,
    pub round: u64,
    pub signed_daa: u64,
    pub leaf_count: u32,
    pub leaves_root: Hash64,
    pub signature: Vec<u8>,
}

impl PalwVerificationVertexV1 {
    /// The vertex's header.
    pub fn header(&self) -> PalwVertexHeaderV1 {
        PalwVertexHeaderV1 {
            version: self.version,
            seat_bond: self.seat_bond,
            round: self.round,
            signed_daa: self.signed_daa,
            leaf_count: u32::try_from(self.leaves.len()).unwrap_or(u32::MAX),
            leaves_root: self.leaves_root,
            signature: self.signature.clone(),
        }
    }

    /// **Build and sign a vertex** from a seat's leaves of one round: sorted into the strict order, the root computed, the
    /// message signed by `sign(message, context)`. `None` for no leaf, a leaf set that breaks a shape rule, or a failed signer.
    /// The seat's node and the tests both build here, so the object the node carries is the one the fold's shape check reads.
    pub fn sign_v1(
        network_domain: Hash64,
        seat_bond: PalwBondKeyV2,
        signed_daa: u64,
        mut leaves: Vec<PalwVertexLeafV1>,
        sign: impl FnOnce(&[u8], &[u8]) -> Option<Vec<u8>>,
    ) -> Option<Self> {
        leaves.sort_by_key(|leaf| leaf.sort_key());
        leaves.dedup_by(|a, b| a.sort_key() == b.sort_key());
        let round = signed_daa / PALW_VERTEX_ROUND_DAA_V1;
        let leaves_root = palw_vertex_root_of_leaves_v1(&leaves)?;
        let message = palw_vertex_message_v1(network_domain, &seat_bond, round, signed_daa, leaves.len(), leaves_root);
        let signature = sign(message.as_byte_slice(), PALW_VERTEX_MLDSA87_CONTEXT_V1)?;
        let vertex = Self { version: PALW_VERTEX_VERSION_V1, seat_bond, round, signed_daa, leaves, leaves_root, signature };
        palw_vertex_shape_v1(&vertex).ok()?;
        Some(vertex)
    }
}

/// **Two vertices of one `(seat, round)` with different roots** (RFC-0007 §I.6; object tag 92). Headers carry the signatures;
/// the leaves of either side ride too, when the filer has them, so the fold can forfeit the seat's locks on the claims they
/// name (a side with no leaves names none).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwVertexEquivocationV1 {
    pub a: PalwVertexHeaderV1,
    pub b: PalwVertexHeaderV1,
    pub a_leaves: Vec<PalwVertexLeafV1>,
    pub b_leaves: Vec<PalwVertexLeafV1>,
}

// ---------------------------------------------------------------------------------------------
// Hashing and signing
// ---------------------------------------------------------------------------------------------

fn keyed(domain: &[u8]) -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(64).key(domain).to_state()
}

fn finish(state: blake2b_simd::State) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// `H(leaf-domain ‖ borsh(leaf))`.
pub fn palw_vertex_leaf_hash_v1(leaf: &PalwVertexLeafV1) -> Hash64 {
    let mut state = keyed(PALW_VERTEX_LEAF_DOMAIN_V1);
    state.update(&borsh::to_vec(leaf).expect("a leaf serializes"));
    finish(state)
}

fn node(left: &Hash64, right: &Hash64) -> Hash64 {
    let mut state = keyed(PALW_VERTEX_NODE_DOMAIN_V1);
    state.update(left.as_byte_slice());
    state.update(right.as_byte_slice());
    finish(state)
}

/// **The root of a vertex's leaf hashes**: pairs hashed left to right, the odd last node of a level promoted unchanged. `None`
/// for an empty list (a seat with nothing to say signs no vertex). The leaf count is signed beside the root, so a promoted
/// node cannot be passed off as a shorter list's.
pub fn palw_vertex_root_v1(leaf_hashes: &[Hash64]) -> Option<Hash64> {
    if leaf_hashes.is_empty() {
        return None;
    }
    let mut level: Vec<Hash64> = leaf_hashes.to_vec();
    while level.len() > 1 {
        level = level.chunks(2).map(|pair| if pair.len() == 2 { node(&pair[0], &pair[1]) } else { pair[0] }).collect();
    }
    Some(level[0])
}

/// The root over a vertex's leaves.
pub fn palw_vertex_root_of_leaves_v1(leaves: &[PalwVertexLeafV1]) -> Option<Hash64> {
    let hashes: Vec<Hash64> = leaves.iter().map(palw_vertex_leaf_hash_v1).collect();
    palw_vertex_root_v1(&hashes)
}

/// **What a seat signs over a vertex**: `H(message-domain ‖ network ‖ borsh(seat) ‖ le64(round) ‖ le64(signed_daa) ‖
/// le32(|leaves|) ‖ root)`.
pub fn palw_vertex_message_v1(
    network_domain: Hash64,
    seat_bond: &PalwBondKeyV2,
    round: u64,
    signed_daa: u64,
    leaf_count: usize,
    leaves_root: Hash64,
) -> Hash64 {
    let mut state = keyed(PALW_VERTEX_MESSAGE_DOMAIN_V1);
    state.update(network_domain.as_byte_slice());
    state.update(&borsh::to_vec(seat_bond).expect("a bond key serializes"));
    state.update(&round.to_le_bytes());
    state.update(&signed_daa.to_le_bytes());
    state.update(&u32::try_from(leaf_count).unwrap_or(u32::MAX).to_le_bytes());
    state.update(leaves_root.as_byte_slice());
    finish(state)
}

/// The message of a header (its count and root are the header's own).
pub fn palw_vertex_header_message_v1(network_domain: Hash64, header: &PalwVertexHeaderV1) -> Hash64 {
    palw_vertex_message_v1(
        network_domain,
        &header.seat_bond,
        header.round,
        header.signed_daa,
        header.leaf_count as usize,
        header.leaves_root,
    )
}

/// **The key of one equivocation offence**: one per `(seat, round)`.
pub fn palw_vertex_equivocation_key_v1(seat: &PalwBondKeyV2, round: u64) -> Hash64 {
    let mut state = keyed(PALW_VERTEX_EQUIVOCATION_KEY_DOMAIN_V1);
    state.update(&borsh::to_vec(seat).expect("a bond key serializes"));
    state.update(&round.to_le_bytes());
    finish(state)
}

// ---------------------------------------------------------------------------------------------
// Refusals, by name
// ---------------------------------------------------------------------------------------------

/// **Why a vertex or an equivocation is refused.** Every hostile shape has a name; nothing here panics.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwVertexErrorV1 {
    #[error("a verification vertex below palw_verification_vertex_v1 (RFC-0007 Part I)")]
    Dormant,
    #[error("a vertex of version {got}; this build reads version {PALW_VERTEX_VERSION_V1}")]
    Version { got: u16 },
    #[error("a vertex of round {got} signed at DAA {signed_daa}: its round is {expected}")]
    RoundMismatch { got: u64, signed_daa: u64, expected: u64 },
    #[error("a vertex with no leaf (a seat with nothing to say signs none)")]
    NoLeaves,
    #[error("a vertex of {got} leaves, past the per-vertex cap of {PALW_VERTEX_MAX_LEAVES_V1}")]
    TooManyLeaves { got: usize },
    #[error("a vertex whose leaves take {got} bytes, past the cap of {PALW_VERTEX_MAX_LEAF_BYTES_V1}")]
    LeavesTooLarge { got: usize },
    #[error("a vertex whose leaves are not strictly ascending (leaf {index})")]
    LeavesNotAscending { index: usize },
    #[error("a vertex whose leaf {index} is an audit leaf: the audit mesh is not armed by this fence")]
    AuditedLeafNotArmed { index: usize },
    #[error("a vertex whose Held leaf {index} names object {object}, which is not 0 (capture), 1 (witness) or 2 (trace manifest)")]
    HeldObjectUnknown { index: usize, object: u8 },
    #[error("a vertex whose Held leaf {index} names the chunks {first}..={last}, an inverted range")]
    HeldRangeInverted { index: usize, first: u32, last: u32 },
    #[error("a vertex whose leaf root does not recompute from its leaves")]
    RootMismatch,
    #[error("a vertex whose signature is {got} bytes, not the ML-DSA-87 {expected}")]
    SignatureLength { got: usize, expected: usize },
    #[error("a vertex signed at DAA {signed_daa} in a block at DAA {at}: it is signed after the block carrying it")]
    SignedInTheFuture { signed_daa: u64, at: u64 },
    #[error("a vertex signed at DAA {signed_daa} carried at DAA {at}: older than the {PALW_VERTEX_MAX_CARRY_DAA_V1} DAA a vertex may wait")]
    TooOld { signed_daa: u64, at: u64 },
    #[error("bond {0:?} is not registered on this chain")]
    UnknownBond(PalwBondKeyV2),
    #[error("the vertex of seat {seat:?} for round {round} is already on the chain: one vertex per seat per round")]
    SecondInRound { seat: PalwBondKeyV2, round: u64 },
    #[error("the vertex's signature does not verify under bond {0:?}'s registered key")]
    BadSignature(PalwBondKeyV2),
    #[error("an equivocation whose two vertices name different seats")]
    EquivocationSeatsDiffer,
    #[error("an equivocation whose two vertices name different rounds")]
    EquivocationRoundsDiffer,
    #[error("an equivocation whose two vertices carry the same leaf root: it is one vertex")]
    EquivocationRootsEqual,
    #[error("an equivocation side whose carried leaves do not match its header's root and count")]
    EquivocationLeavesMismatch,
    #[error("an equivocation about a round signed at DAA {signed_daa}, past the {PALW_VERTEX_EVIDENCE_WINDOW_DAA_V1} DAA it stays provable (block at DAA {at})")]
    EvidenceTooOld { signed_daa: u64, at: u64 },
    #[error("the equivocation of seat {seat:?} in round {round} was already convicted")]
    AlreadyConvicted { seat: PalwBondKeyV2, round: u64 },
}

// ---------------------------------------------------------------------------------------------
// Stateless checks
// ---------------------------------------------------------------------------------------------

/// **A vertex's shape** (PALW-VC-1): version, round, leaf count and size, strict order, root, signature length. Stateless.
pub fn palw_vertex_shape_v1(vertex: &PalwVerificationVertexV1) -> Result<(), PalwVertexErrorV1> {
    use PalwVertexErrorV1 as E;
    if vertex.version != PALW_VERTEX_VERSION_V1 {
        return Err(E::Version { got: vertex.version });
    }
    let expected = vertex.signed_daa / PALW_VERTEX_ROUND_DAA_V1;
    if vertex.round != expected {
        return Err(E::RoundMismatch { got: vertex.round, signed_daa: vertex.signed_daa, expected });
    }
    if vertex.leaves.is_empty() {
        return Err(E::NoLeaves);
    }
    if vertex.leaves.len() > PALW_VERTEX_MAX_LEAVES_V1 {
        return Err(E::TooManyLeaves { got: vertex.leaves.len() });
    }
    palw_vertex_leaves_shape_v1(&vertex.leaves)?;
    let root = palw_vertex_root_of_leaves_v1(&vertex.leaves).ok_or(E::NoLeaves)?;
    if root != vertex.leaves_root {
        return Err(E::RootMismatch);
    }
    let expected_len = crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
    if vertex.signature.len() != expected_len {
        return Err(E::SignatureLength { got: vertex.signature.len(), expected: expected_len });
    }
    Ok(())
}

/// The shape of a leaf list alone: size, strict order, no audit leaf, `Held` ranges and objects. (A vertex's count cap is
/// [`palw_vertex_shape_v1`]'s; equivocation evidence checks the list against its header's count.)
pub fn palw_vertex_leaves_shape_v1(leaves: &[PalwVertexLeafV1]) -> Result<(), PalwVertexErrorV1> {
    use PalwVertexErrorV1 as E;
    let mut bytes = 0usize;
    let mut previous: Option<Vec<u8>> = None;
    for (index, leaf) in leaves.iter().enumerate() {
        bytes = bytes.saturating_add(leaf.encoded_len());
        if bytes > PALW_VERTEX_MAX_LEAF_BYTES_V1 {
            return Err(E::LeavesTooLarge { got: bytes });
        }
        let key = leaf.sort_key();
        if previous.as_ref().is_some_and(|before| *before >= key) {
            return Err(E::LeavesNotAscending { index });
        }
        previous = Some(key);
        match leaf {
            PalwVertexLeafV1::Audited { .. } => return Err(E::AuditedLeafNotArmed { index }),
            PalwVertexLeafV1::Held { object, first, last, .. } => {
                if *object > PALW_VERTEX_HELD_OBJECT_TRACE_MANIFEST_V1 {
                    return Err(E::HeldObjectUnknown { index, object: *object });
                }
                if first > last {
                    return Err(E::HeldRangeInverted { index, first: *first, last: *last });
                }
            }
            PalwVertexLeafV1::Verdict { .. } => {}
        }
    }
    Ok(())
}

/// **The signature check** (PALW-VC-2): the seat's registered key, over [`palw_vertex_message_v1`], under
/// [`PALW_VERTEX_MLDSA87_CONTEXT_V1`]. `verify_mldsa87(pubkey, message, signature, context)` is the caller's verifier.
pub fn palw_vertex_verify_signature_v1<V>(
    state: &PalwChainStateV2,
    network_domain: Hash64,
    seat_bond: &PalwBondKeyV2,
    round: u64,
    signed_daa: u64,
    leaf_count: usize,
    leaves_root: Hash64,
    signature: &[u8],
    verify_mldsa87: V,
) -> Result<(), PalwVertexErrorV1>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    let bond = state.bond(seat_bond).ok_or(PalwVertexErrorV1::UnknownBond(*seat_bond))?;
    let message = palw_vertex_message_v1(network_domain, seat_bond, round, signed_daa, leaf_count, leaves_root);
    if verify_mldsa87(&bond.pubkey, message.as_byte_slice(), signature, PALW_VERTEX_MLDSA87_CONTEXT_V1) {
        Ok(())
    } else {
        Err(PalwVertexErrorV1::BadSignature(*seat_bond))
    }
}

/// **A vertex's admissibility at a block** (the part of PALW-VC-1 that reads a clock and the registry): not signed after the
/// block that carries it, not older than [`PALW_VERTEX_MAX_CARRY_DAA_V1`], from a registered bond, and the first of its
/// `(seat, round)` on this chain. Pure over the state; the fold and the acceptance walk both call it.
pub fn palw_vertex_admissible_v1(
    state: &PalwChainStateV2,
    vertex: &PalwVerificationVertexV1,
    daa_score: u64,
) -> Result<(), PalwVertexErrorV1> {
    use PalwVertexErrorV1 as E;
    palw_vertex_shape_v1(vertex)?;
    if vertex.signed_daa > daa_score {
        return Err(E::SignedInTheFuture { signed_daa: vertex.signed_daa, at: daa_score });
    }
    if daa_score - vertex.signed_daa > PALW_VERTEX_MAX_CARRY_DAA_V1 {
        return Err(E::TooOld { signed_daa: vertex.signed_daa, at: daa_score });
    }
    if state.bond(&vertex.seat_bond).is_none() {
        return Err(E::UnknownBond(vertex.seat_bond));
    }
    if state.vertex_round_row_v1(vertex.round, &vertex.seat_bond).is_some() {
        return Err(E::SecondInRound { seat: vertex.seat_bond, round: vertex.round });
    }
    Ok(())
}

/// **Equivocation evidence's shape** (PALW-VC-5, stateless): one seat, one round, two different roots, each header well-formed and
/// each side's leaves (if carried) matching its header. Signatures are the caller's ([`palw_vertex_verify_signature_v1`] on each
/// header).
pub fn palw_vertex_equivocation_shape_v1(evidence: &PalwVertexEquivocationV1) -> Result<(), PalwVertexErrorV1> {
    use PalwVertexErrorV1 as E;
    let (a, b) = (&evidence.a, &evidence.b);
    for header in [a, b] {
        if header.version != PALW_VERTEX_VERSION_V1 {
            return Err(E::Version { got: header.version });
        }
        let expected = header.signed_daa / PALW_VERTEX_ROUND_DAA_V1;
        if header.round != expected {
            return Err(E::RoundMismatch { got: header.round, signed_daa: header.signed_daa, expected });
        }
        let expected_len = crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
        if header.signature.len() != expected_len {
            return Err(E::SignatureLength { got: header.signature.len(), expected: expected_len });
        }
    }
    if a.seat_bond != b.seat_bond {
        return Err(E::EquivocationSeatsDiffer);
    }
    if a.round != b.round {
        return Err(E::EquivocationRoundsDiffer);
    }
    if a.leaves_root == b.leaves_root {
        return Err(E::EquivocationRootsEqual);
    }
    for (header, leaves) in [(a, &evidence.a_leaves), (b, &evidence.b_leaves)] {
        if leaves.is_empty() {
            continue;
        }
        if leaves.len() != header.leaf_count as usize || leaves.len() > PALW_VERTEX_MAX_LEAVES_V1 {
            return Err(E::EquivocationLeavesMismatch);
        }
        palw_vertex_leaves_shape_v1(leaves)?;
        if palw_vertex_root_of_leaves_v1(leaves) != Some(header.leaves_root) {
            return Err(E::EquivocationLeavesMismatch);
        }
    }
    Ok(())
}

/// **Equivocation evidence's admissibility at a block** (PALW-VC-5): the shape, the round still provable
/// ([`PALW_VERTEX_EVIDENCE_WINDOW_DAA_V1`]), the bond registered, the pair not yet convicted.
pub fn palw_vertex_equivocation_admissible_v1(
    state: &PalwChainStateV2,
    evidence: &PalwVertexEquivocationV1,
    daa_score: u64,
) -> Result<(), PalwVertexErrorV1> {
    use PalwVertexErrorV1 as E;
    palw_vertex_equivocation_shape_v1(evidence)?;
    let (a, b) = (&evidence.a, &evidence.b);
    for header in [a, b] {
        if daa_score.saturating_sub(header.signed_daa) > PALW_VERTEX_EVIDENCE_WINDOW_DAA_V1 {
            return Err(E::EvidenceTooOld { signed_daa: header.signed_daa, at: daa_score });
        }
    }
    if state.bond(&a.seat_bond).is_none() {
        return Err(E::UnknownBond(a.seat_bond));
    }
    if state.vertex_round_row_v1(a.round, &a.seat_bond).is_some_and(|row| row.convicted) {
        return Err(E::AlreadyConvicted { seat: a.seat_bond, round: a.round });
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The rows the fold keeps
// ---------------------------------------------------------------------------------------------

/// **What the chain remembers of one `(round, seat)`**: the root of the first vertex it accepted, when, and whether the
/// pair was convicted of equivocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwVertexRoundRowV1 {
    pub leaves_root: Hash64,
    pub accepted_daa: u64,
    pub convicted: bool,
}

/// One counted `Verdict` leaf: the seat, what it said, and the DAA it said it at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwVertexCountedV1 {
    pub seat: PalwBondKeyV2,
    pub verdict: PalwReceiptVerdictV2,
    pub signed_daa: u64,
}

/// **A claim's tally**: the leaves counted toward it while it is `PanelBound`, in the order the chain counted them. Dropped
/// the moment the claim leaves that phase.
#[derive(Clone, Debug, Default, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwVertexTallyV1 {
    pub counted: Vec<PalwVertexCountedV1>,
}

impl PalwVertexTallyV1 {
    pub fn answered(&self, seat: &PalwBondKeyV2) -> bool {
        self.counted.iter().any(|row| row.seat == *seat)
    }

    /// Counted `Valid` leaves.
    pub fn valid(&self) -> usize {
        self.counted.iter().filter(|row| matches!(row.verdict, PalwReceiptVerdictV2::Valid)).count()
    }
}

/// One attestation of a `Held` leaf, kept while its claim is live.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwVertexHeldRowV1 {
    pub seat: PalwBondKeyV2,
    pub object: u8,
    pub first: u32,
    pub last: u32,
    pub digest: Hash64,
    pub signed_daa: u64,
    /// Whether the attester has already been charged its `Held` exposure on this claim.
    pub charged: bool,
}

/// **The vertex's tables as the state holds them** (RFC-0007): the `(round, seat)` rows, each `PanelBound` claim's tally, and each
/// live claim's `Held` attestations. Empty — and then neither rooted nor carried — on every chain below
/// `Params::palw_verification_vertex_v1`.
#[derive(Clone, Debug, Default, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwVertexStateV1 {
    /// Keyed `(round, seat)`: round-first, so the sweep removes the oldest rounds first.
    pub rounds: std::collections::BTreeMap<(u64, PalwBondKeyV2), PalwVertexRoundRowV1>,
    pub tallies: std::collections::BTreeMap<Hash64, PalwVertexTallyV1>,
    pub held: std::collections::BTreeMap<Hash64, Vec<PalwVertexHeldRowV1>>,
}

impl PalwVertexStateV1 {
    pub fn is_empty(&self) -> bool {
        self.rounds.is_empty() && self.tallies.is_empty() && self.held.is_empty()
    }
}

/// **Why a leaf did not count** (informational: the fold ignores it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwVertexLeafFateV1 {
    /// Counted toward a `PanelBound` claim's tally.
    Counted,
    /// Counted as a supplementary receipt of a claim already licensed (the seat's `Valid` or `Sampled`, credited by the
    /// licensing arm's supplementary door).
    Supplementary,
    /// Ignored, for this reason.
    Ignored(&'static str),
}

/// **The claim a leaf names, on a state**: `Full` is the id when the claim has a panel; `Compact` is the one claim whose
/// panel bound at the stated DAA and whose id starts with the prefix — `None` when no claim or more than one does (an ambiguous
/// reference is ignored). `by_bound` is the bound-DAA index ([`PalwChainStateV2::claims_bound_at_v1`] on a state, the fold's
/// lazily built copy while it writes).
pub fn palw_vertex_resolve_claim_v1(
    state: &PalwChainStateV2,
    claim: &PalwClaimRefV1,
    claims_bound_at: impl Fn(u64) -> Vec<Hash64>,
) -> Option<Hash64> {
    match claim {
        PalwClaimRefV1::Full(id) => state.panel(id).is_some().then_some(*id),
        PalwClaimRefV1::Compact { bound_daa, id_prefix } => {
            let mut hits = claims_bound_at(u64::from(*bound_daa)).into_iter().filter(|id| id.as_bytes()[..16] == id_prefix[..]);
            let first = hits.next()?;
            hits.next().is_none().then_some(first)
        }
    }
}

/// **The conditions under which a `Verdict` leaf counts** (PALW-VC-3) — the conditions `validate_receipt_coverage_v2` applies
/// to a receipt, on the state the fold writes to:
///
/// * the claim is `PanelBound` on a panel that bound at or after the fence (a claim bound before it licenses on the receipt
///   path, so the two paths never count one seat twice), is not licensed by shard parts, and the vertex's seat holds a seat on
///   its panel;
/// * `signed_daa` is inside the claim's receipt window (at or after the bind, at or before the deadline, not after the
///   carrying block);
/// * it is the seat's first counted verdict for the claim (PALW-VC-4: a verdict stands);
/// * `Incapable` is not pleaded on the liveness floor; `Sampled` needs R-core+.
///
/// A claim already licensed takes the seat's `Valid` or `Sampled` as a supplementary receipt on the licensing arm's own
/// conditions (a seat on duty the chain has not credited or locked, inside the window). Anything else is ignored.
pub fn palw_vertex_leaf_fate_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    daa_score: u64,
    seat: &PalwBondKeyV2,
    signed_daa: u64,
    claim_id: &Hash64,
    verdict: &PalwReceiptVerdictV2,
) -> PalwVertexLeafFateV1 {
    use PalwVertexLeafFateV1::{Counted, Ignored, Supplementary};
    let (Some(claim), Some(panel)) = (state.claim(claim_id), state.panel(claim_id)) else { return Ignored("no such claim or panel") };
    let Some(fence) = params.vertex_from_daa() else { return Ignored("the fence is not armed") };
    if panel.bound_daa < fence {
        return Ignored("the claim's panel bound before the fence: it licenses on the receipt path");
    }
    if !panel.seats.iter().any(|s| s.bond == *seat) {
        return Ignored("the vertex's seat holds no seat on the claim's panel");
    }
    if state.class_shard_plan(&claim.class_id).is_some() {
        return Ignored("the claim's class licenses by shard parts");
    }
    let Ok(Some(deadline)) = crate::palw_state_v2::palw_claim_receipt_deadline_v1(state, params, claim_id, claim) else {
        return Ignored("the claim has no receipt deadline");
    };
    if signed_daa < panel.bound_daa || signed_daa > deadline || signed_daa > daa_score {
        return Ignored("signed outside the claim's receipt window");
    }
    match verdict {
        PalwReceiptVerdictV2::Incapable if !crate::palw_state_v2::palw_seat_may_plead_incapable_v2(claim.class_id, params.base_class_id()) => {
            return Ignored("Incapable is not pleaded on the liveness floor");
        }
        PalwReceiptVerdictV2::Sampled if !params.rcore_plus_active_at(daa_score) => return Ignored("Sampled needs palw_rcore_plus"),
        _ => {}
    }
    match claim.phase {
        PalwClaimPhaseV2::PanelBound { .. } => {
            if state.vertex_tally_of_v1(claim_id).is_some_and(|tally| tally.answered(seat)) {
                return Ignored("the seat's first counted verdict stands");
            }
            Counted
        }
        PalwClaimPhaseV2::ReceiptLicensed { .. } if params.rcore_plus_active_at(daa_score) => {
            if !matches!(verdict, PalwReceiptVerdictV2::Valid | PalwReceiptVerdictV2::Sampled) {
                return Ignored("a licensed claim takes only a Valid or Sampled leaf, as a supplementary receipt");
            }
            if daa_score > deadline {
                return Ignored("the receipt window of the licensed claim has closed");
            }
            match state.panel_duties_of(claim_id).and_then(|duties| duties.get(seat)) {
                Some(0) => {}
                Some(_) => return Ignored("the seat is already credited on the claim"),
                None => return Ignored("the seat is not on duty for the claim"),
            }
            if state.slashable_lock(*seat, *claim_id).is_some() {
                return Ignored("the seat is already counted on the claim");
            }
            Supplementary
        }
        _ => Ignored("the claim is not in a phase that takes a verdict"),
    }
}

/// **The receipts a tally stands for**, in the panel's seat order, exactly as a `ReceiptLicensedV2` of the same seats would
/// carry them — with empty signatures (what vouches for each is the vertex's signature, verified once at acceptance) and, for a
/// `Valid`, the mask the seat is assigned (the only mask `validate_receipt_coverage_v2` takes).
pub fn palw_vertex_receipts_of_v1(
    state: &PalwChainStateV2,
    claim_id: &Hash64,
    counted: &[PalwVertexCountedV1],
) -> Option<Vec<crate::palw_panel_v2::PalwSeatReceiptV3>> {
    let panel = state.panel(claim_id)?;
    let seat_count = u16::try_from(panel.seats.len()).ok()?;
    let assignment = crate::palw_verification_v2::palw_segment_assignment_v2(panel.anchor, *claim_id, seat_count);
    let mut out = Vec::with_capacity(counted.len());
    for (index, seat) in panel.seats.iter().enumerate() {
        let Some(row) = counted.iter().find(|row| row.seat == seat.bond) else { continue };
        let segments = match row.verdict {
            PalwReceiptVerdictV2::Valid => assignment.mask_of(u16::try_from(index).ok()?),
            _ => PalwSegmentMaskV2::NONE,
        };
        out.push(crate::palw_panel_v2::PalwSeatReceiptV3 {
            receipt: crate::palw_panel_v2::PalwSeatReceiptV2 {
                claim: *claim_id,
                verdict: row.verdict,
                seat_bond: seat.bond,
                signed_daa: row.signed_daa,
                signature: Vec::new(),
            },
            segments,
        });
    }
    Some(out)
}

/// **A DA certificate** (RFC-0007 §I.7): `q` distinct seats of the claim's panel with equal `Held` leaves for the same
/// object, range and digest. Returns the attesting seats, or `None` below the quorum.
pub fn palw_vertex_da_certificate_v1(
    state: &PalwChainStateV2,
    claim_id: &Hash64,
    object: u8,
    first: u32,
    last: u32,
    digest: &Hash64,
) -> Option<Vec<PalwBondKeyV2>> {
    let rows = state.vertex_held_of_v1(claim_id)?;
    let mut seats: Vec<PalwBondKeyV2> = rows
        .iter()
        .filter(|row| row.object == object && row.first == first && row.last == last && row.digest == *digest)
        .map(|row| row.seat)
        .collect();
    seats.sort();
    seats.dedup();
    (seats.len() >= usize::from(PALW_VERTEX_DA_QUORUM_V1)).then_some(seats)
}

/// **The path rule** (RFC-0007 migration; the lead's decision of 2026-10-03, open question 4): a claim whose panel bound at or after
/// the fence licenses **by tally**; a claim whose panel bound before it licenses on the receipt path until it is licensed or void —
/// receipts are accepted past the fence for exactly those claims, so nothing is stranded. The one predicate the acceptance walk, the
/// fold and the node read.
pub fn palw_vertex_claim_licenses_by_tally_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    daa_score: u64,
    claim_id: &Hash64,
) -> bool {
    params.vertex_active_at(daa_score)
        && params.vertex_from_daa().is_some_and(|fence| state.panel(claim_id).is_some_and(|panel| panel.bound_daa >= fence))
}

/// The claim a receipt-path licensing object names (`ReceiptLicensed`, `ReceiptLicensedV2`, `OptimisticLicensed`,
/// `ProducerDefaulted`), if `object` is one. A batch licence routes its entries one by one
/// (`palw_batch_entry_route_v1`), so it is not named here.
pub fn palw_receipt_object_claim_v1(object: &crate::palw_state_v2::PalwConsensusObjectV2) -> Option<&Hash64> {
    use crate::palw_state_v2::PalwConsensusObjectV2 as O;
    match object {
        O::ReceiptLicensed { claim, .. }
        | O::ReceiptLicensedV2 { claim, .. }
        | O::OptimisticLicensed { claim, .. }
        | O::ProducerDefaulted { claim, .. } => Some(claim),
        _ => None,
    }
}

/// The sompi `permille`‰ of `amount` (floored, checked: `None` on overflow).
pub fn palw_vertex_permille_of_v1(amount: u128, permille: u16) -> Option<u128> {
    amount.checked_mul(u128::from(permille))?.checked_div(1_000)
}

// ---------------------------------------------------------------------------------------------
// Part III — the security model in numbers
// ---------------------------------------------------------------------------------------------

/// **How a class's seats check an interval** (RFC-0007 §III.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwCheckModeV1 {
    /// Full replay or algebraic, every seat of the panel, duty running to `Final`: `m = seats`.
    FullToFinal,
    /// Full, licensed at quorum, the rest stop: `m = quorum`.
    FullAtQuorum,
    /// `k` of `n` intervals sampled by each of the panel's seats (ADR-0098): the seats cover an interval with probability
    /// `k / n` each.
    Sampled { k: u32, n: u32 },
}

/// **`m`: the number of independent checkers of one interval.** For a sampled mode it is the *expected* number, in parts per
/// million of a checker (`seats · k / n`), so it is returned as ppm of one checker.
pub fn palw_vertex_m_v1(mode: PalwCheckModeV1, seats: u32, quorum: u32) -> u64 {
    match mode {
        PalwCheckModeV1::FullToFinal => u64::from(seats) * 1_000_000,
        PalwCheckModeV1::FullAtQuorum => u64::from(quorum) * 1_000_000,
        PalwCheckModeV1::Sampled { k, n } => {
            if n == 0 {
                0
            } else {
                u64::from(seats) * u64::from(k.min(n)) * 1_000_000 / u64::from(n)
            }
        }
    }
}

/// **The escape probability of a one-point lie, in parts per million**, when the adversary holds `f_ppm` parts per million of the
/// eligible seat weight (RFC-0007 §III.2): `f^m` for the full modes, `(f + (1 − f)(1 − k/n))^seats` for the sampled one. Saturating
/// integer arithmetic over 64-bit ppm; `1_000_000` is certain escape.
pub fn palw_vertex_escape_ppm_v1(mode: PalwCheckModeV1, seats: u32, quorum: u32, f_ppm: u64) -> u64 {
    let one: u128 = 1_000_000;
    let f = u128::from(f_ppm.min(1_000_000));
    let pow = |base: u128, exp: u32| -> u128 {
        let mut acc = one;
        for _ in 0..exp {
            acc = acc * base / one;
        }
        acc
    };
    let per_seat = match mode {
        PalwCheckModeV1::FullToFinal | PalwCheckModeV1::FullAtQuorum => f,
        PalwCheckModeV1::Sampled { k, n } => {
            if n == 0 {
                one
            } else {
                let covered = one * u128::from(k.min(n)) / u128::from(n);
                // f + (1 - f) * (1 - k/n)
                f + (one - f) * (one - covered) / one
            }
        }
    };
    let m = match mode {
        PalwCheckModeV1::FullAtQuorum => quorum,
        _ => seats,
    };
    u64::try_from(pow(per_seat.min(one), m)).unwrap_or(1_000_000)
}

/// **How many times the gain a slash must be** for a rational producer not to lie (RFC-0007 §III.4): a lie pays when
/// `gain > P(detect) · slash`, so the slash must be at least `gain / P(detect)`. `p_detect_ppm` is `P(detect)`; the multiple is
/// returned in ppm of the gain (`1_000_000` = once the gain), rounded up. `None` for a probability of zero.
pub fn palw_vertex_required_slash_multiple_ppm_v1(p_detect_ppm: u64) -> Option<u64> {
    if p_detect_ppm == 0 {
        return None;
    }
    let p = u128::from(p_detect_ppm.min(1_000_000));
    let multiple = (1_000_000u128 * 1_000_000).div_ceil(p);
    u64::try_from(multiple).ok()
}

/// Does a slash deter a lie worth `gain` that is caught with probability `p_detect_ppm`? (`slash · P(detect) ≥ gain`.)
pub fn palw_vertex_slash_deters_v1(gain: u128, slash: u128, p_detect_ppm: u64) -> bool {
    slash.saturating_mul(u128::from(p_detect_ppm.min(1_000_000))) >= gain.saturating_mul(1_000_000)
}

/// **What a seat does when its own check of an interval fails** (RFC-0007 §II.8, III.3): a failed algebraic check is not a
/// verdict (the witness may simply be bad), so the seat files no `Valid`, falls back to the exact replay or court path, and may
/// open the court through the challenger's half. A passed check files `Valid` into the round's vertex.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwEscalationV1 {
    /// The check passed: the verdict `Valid` joins this round's vertex.
    FileValid,
    /// The check failed: no `Valid`; re-derive by the exact reference evaluator, and let the court (unchanged) decide.
    EscalateToExactCourt,
    /// The check could not run (no witness, no sketch): abstain with the verdict the seat's situation supports, never `Valid`.
    Abstain,
}

/// The escalation for a check's outcome.
pub fn palw_vertex_escalation_v1(check_ran: bool, check_passed: bool) -> PalwEscalationV1 {
    match (check_ran, check_passed) {
        (false, _) => PalwEscalationV1::Abstain,
        (true, true) => PalwEscalationV1::FileValid,
        (true, false) => PalwEscalationV1::EscalateToExactCourt,
    }
}

/// **Does a seat's duty run past the licence?** (RFC-0007 §III.2: "duty runs to `Final`, not to the licence".) The seat keeps
/// checking after quorum so that `m` stays the panel's size; a later finding opens the court inside the licence-to-`Final` window.
pub const PALW_VERTEX_DUTY_RUNS_TO_FINAL_V1: bool = true;

// ---------------------------------------------------------------------------------------------
// The fence
// ---------------------------------------------------------------------------------------------

/// **The entry a flag day (or a drill) arms the verification vertex with**, through its own `set`, which writes the bundle's mirror.
/// In NO testnet-12 flag-day list today: the fence is dormant on every network and arms with a later flag day.
pub const PALW_VERTEX_ENTRY_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_verification_vertex_v1",
    set: |params, at| {
        params.palw_verification_vertex_v1 = at;
        params.sync_palw_verification_vertex_v1();
    },
};

/// The drill's one-entry list (`--palw-drill-vertex-at`, [`crate::config::drill::palw_drill_vertex_at_v1`]).
pub const PALW_DRILL_VERTEX_FENCES_V1: &[PalwPostLaunchFenceV1] = &[PALW_VERTEX_ENTRY_V1];

impl Params {
    /// `palw_verification_vertex_v1`, resolved: `Some` only on a `ConsensusV2` network that armed it with a real height (a `never()`
    /// value is dormant).
    pub fn palw_verification_vertex_fence(&self) -> Option<ForkActivation> {
        match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(_) => self.palw_verification_vertex_v1.filter(|f| *f != ForkActivation::never()),
            _ => None,
        }
    }

    /// **Is the verification vertex in force at `daa_score`?** `false` on every shipped preset.
    pub fn palw_verification_vertex_active_at(&self, daa_score: u64) -> bool {
        self.palw_verification_vertex_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// **The fence's mirror** on the V2 bundle's state params (`vertex_from_daa`), which the fold reads. Written here and nothing else;
    /// `None` where the fence is not armed (or is `never()`). Call it wherever the fence is set on an assembled ruleset;
    /// [`Self::validate_palw_verification_vertex_v1`] refuses a ruleset whose copy disagrees.
    pub fn sync_palw_verification_vertex_v1(&mut self) {
        let from_daa = self.palw_verification_vertex_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_vertex_from_daa(from_daa);
        }
    }

    /// **The verification vertex's own refusals**, asked by [`Params::validate_palw_v2`]:
    ///
    /// * a V2 bundle whose mirror of the height is not the fence's;
    /// * arming on a ruleset that is not `ConsensusV2`;
    /// * arming without, at or below its height, the fences the tally leans on — `palw_verification_v2` (the coverage doors the tally
    ///   feeds), `palw_rcore_plus` (the licence is the backed subset's; `Sampled` is a verdict), `palw_unavailable_abstains` (an
    ///   `Unavailable` leaf abstains), `palw_panel_economy` (the duty rows the supplementary door reads) and `palw_objective_offence`
    ///   (the lock ledger the equivocation forfeits from) — each named in the refusal.
    ///
    /// A `Some(never())` value is dormant and passes (it collapses out of the identity).
    pub fn validate_palw_verification_vertex_v1(&self) -> Result<(), PalwModeV2Error> {
        let mirror = match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => bundle.state.vertex_from_daa(),
            _ => None,
        };
        let armed = self.palw_verification_vertex_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if mirror != armed {
            return Err(PalwModeV2Error::Invalid(
                "palw_verification_vertex_v1 disagrees with the V2 bundle's mirror: mirror it with \
                 Params::sync_palw_verification_vertex_v1 after the bundle is assembled",
            ));
        }
        let Some(at) = armed else { return Ok(()) };
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_verification_vertex_v1 is a ConsensusV2 rule: this ruleset has no V2 bundle"));
        }
        let in_force = |fence: Option<ForkActivation>| fence.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= at);
        if !in_force(self.palw_verification_v2) {
            return Err(PalwModeV2Error::Invalid(
                "palw_verification_vertex_v1 needs palw_verification_v2 in force at or below it: the tally feeds the coverage licence",
            ));
        }
        if !in_force(self.palw_rcore_plus) {
            return Err(PalwModeV2Error::Invalid(
                "palw_verification_vertex_v1 needs palw_rcore_plus in force at or below it: the licence is the backed subset's",
            ));
        }
        if !in_force(self.palw_unavailable_abstains) {
            return Err(PalwModeV2Error::Invalid(
                "palw_verification_vertex_v1 needs palw_unavailable_abstains in force at or below it: an Unavailable leaf abstains",
            ));
        }
        if !in_force(self.palw_panel_economy) {
            return Err(PalwModeV2Error::Invalid(
                "palw_verification_vertex_v1 needs palw_panel_economy in force at or below it: the supplementary door reads the duty rows",
            ));
        }
        if !in_force(self.palw_objective_offence) {
            return Err(PalwModeV2Error::Invalid(
                "palw_verification_vertex_v1 needs palw_objective_offence in force at or below it: an equivocation forfeits from the lock ledger",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::{TransactionId, TransactionOutpoint};

    fn bond(i: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_bytes([i; 64]), u32::from(i)))
    }

    fn h(i: u8) -> Hash64 {
        Hash64::from_bytes([i; 64])
    }

    fn verdict_leaf(i: u8, verdict: PalwReceiptVerdictV2) -> PalwVertexLeafV1 {
        PalwVertexLeafV1::Verdict { claim: PalwClaimRefV1::Full(h(i)), verdict }
    }

    fn vertex(leaves: Vec<PalwVertexLeafV1>) -> PalwVerificationVertexV1 {
        let domain = h(0xD0);
        PalwVerificationVertexV1::sign_v1(domain, bond(1), 100, leaves, |message, context| {
            assert_eq!(context, PALW_VERTEX_MLDSA87_CONTEXT_V1);
            let mut signature = message.to_vec();
            signature.resize(crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN, 0);
            Some(signature)
        })
        .expect("a well-formed vertex signs")
    }

    #[test]
    fn domains_are_unique_across_all_palw_families() {
        let mut all: Vec<&[u8]> = Vec::new();
        all.extend(PALW_VERTEX_V1_ALL_DOMAINS);
        all.extend(crate::palw_receipt::PALW_RECEIPT_ALL_DOMAINS);
        all.push(crate::palw_receipt::PALW_RECEIPT_MLDSA87_CONTEXT);
        all.extend(crate::palw_batch_licence_v1::PALW_BATCH_LICENCE_V1_ALL_DOMAINS);
        all.extend(crate::palw_job_identity::PALW_JOB_ALL_DOMAINS);
        all.extend(crate::palw_schedule::PALW_SCHEDULE_ALL_DOMAINS);
        all.extend(crate::palw_slash::PALW_S_ALL_DOMAINS);
        all.extend(crate::palw_routing::PALW_ROUTING_ALL_DOMAINS);
        all.extend(crate::palw_registry::PALW_REGISTRY_ALL_DOMAINS);
        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                assert_ne!(a, b, "domain collision: {:?}", String::from_utf8_lossy(a));
            }
        }
        // …and none is a receipt's own signing domain, so a vertex signature is neither a receipt's nor another court move's.
        assert!(![
            crate::palw_panel_v2::PALW_RECEIPT_V2_MLDSA87_CONTEXT,
            crate::palw_panel_v2::PALW_RECEIPT_V3_MLDSA87_CONTEXT,
        ]
        .contains(&PALW_VERTEX_MLDSA87_CONTEXT_V1));
    }

    /// The honest vertex passes the shape check; the leaves are sorted into the strict order and a repeated leaf is one.
    #[test]
    fn an_honest_vertex_is_well_formed_and_its_leaves_are_in_the_strict_order() {
        let v = vertex(vec![
            verdict_leaf(9, PalwReceiptVerdictV2::Valid),
            verdict_leaf(2, PalwReceiptVerdictV2::Valid),
            verdict_leaf(9, PalwReceiptVerdictV2::Valid),
            PalwVertexLeafV1::Held { claim: PalwClaimRefV1::Full(h(2)), object: 1, first: 0, last: 7, digest: h(0xAA) },
        ]);
        assert_eq!(palw_vertex_shape_v1(&v), Ok(()));
        assert_eq!(v.leaves.len(), 3, "a repeated leaf is one");
        assert!(v.leaves.windows(2).all(|pair| pair[0].sort_key() < pair[1].sort_key()));
        assert_eq!(v.round, 100 / PALW_VERTEX_ROUND_DAA_V1);
        assert_eq!(v.header().leaf_count, 3);
    }

    /// **Malformed and hostile vertices are refused by name**, rule by rule.
    #[test]
    fn a_malformed_vertex_is_refused_by_name() {
        use PalwVertexErrorV1 as E;
        let base = vertex(vec![verdict_leaf(1, PalwReceiptVerdictV2::Valid), verdict_leaf(2, PalwReceiptVerdictV2::Valid)]);
        assert_eq!(palw_vertex_shape_v1(&base), Ok(()));
        let mut bad = base.clone();
        bad.version = 2;
        assert_eq!(palw_vertex_shape_v1(&bad), Err(E::Version { got: 2 }));
        let mut bad = base.clone();
        bad.round += 1;
        assert!(matches!(palw_vertex_shape_v1(&bad), Err(E::RoundMismatch { .. })), "a vertex moved to another round");
        let mut bad = base.clone();
        bad.leaves.clear();
        assert_eq!(palw_vertex_shape_v1(&bad), Err(E::NoLeaves));
        let mut bad = base.clone();
        bad.leaves.swap(0, 1);
        assert_eq!(palw_vertex_shape_v1(&bad), Err(E::LeavesNotAscending { index: 1 }));
        let mut bad = base.clone();
        bad.leaves.push(bad.leaves[1]);
        assert_eq!(palw_vertex_shape_v1(&bad), Err(E::LeavesNotAscending { index: 2 }), "a leaf twice");
        let mut bad = base.clone();
        bad.leaves_root = h(0xEE);
        assert_eq!(palw_vertex_shape_v1(&bad), Err(E::RootMismatch));
        let mut bad = base.clone();
        bad.leaves.pop();
        assert_eq!(palw_vertex_shape_v1(&bad), Err(E::RootMismatch), "a truncated vertex");
        let mut bad = base.clone();
        bad.signature.pop();
        assert!(matches!(palw_vertex_shape_v1(&bad), Err(E::SignatureLength { .. })));
        // The leaf cap: one more than the cap.
        let many: Vec<PalwVertexLeafV1> = (0..=PALW_VERTEX_MAX_LEAVES_V1 as u32)
            .map(|i| PalwVertexLeafV1::Verdict {
                claim: PalwClaimRefV1::Compact { bound_daa: i, id_prefix: [0; 16] },
                verdict: PalwReceiptVerdictV2::Valid,
            })
            .collect();
        let mut bad = base.clone();
        bad.leaves_root = palw_vertex_root_of_leaves_v1(&many).unwrap();
        bad.leaves = many;
        assert!(matches!(palw_vertex_shape_v1(&bad), Err(E::TooManyLeaves { .. })));
        // An audit leaf, an unknown Held object and an inverted Held range.
        for (leaf, want) in [
            (
                PalwVertexLeafV1::Audited { claim: PalwClaimRefV1::Full(h(5)), leaf: 1, result: 0 },
                E::AuditedLeafNotArmed { index: 0 },
            ),
            (
                PalwVertexLeafV1::Held { claim: PalwClaimRefV1::Full(h(5)), object: 9, first: 0, last: 1, digest: h(1) },
                E::HeldObjectUnknown { index: 0, object: 9 },
            ),
            (
                PalwVertexLeafV1::Held { claim: PalwClaimRefV1::Full(h(5)), object: 0, first: 4, last: 1, digest: h(1) },
                E::HeldRangeInverted { index: 0, first: 4, last: 1 },
            ),
        ] {
            let mut bad = base.clone();
            bad.leaves = vec![leaf];
            bad.leaves_root = palw_vertex_root_of_leaves_v1(&bad.leaves).unwrap();
            assert_eq!(palw_vertex_shape_v1(&bad), Err(want));
        }
    }

    /// The message binds every field: the network, the seat, the round, the DAA, the count and the root.
    #[test]
    fn the_signed_message_binds_every_field() {
        let base = palw_vertex_message_v1(h(1), &bond(1), 5, 5, 3, h(9));
        assert_ne!(base, palw_vertex_message_v1(h(2), &bond(1), 5, 5, 3, h(9)), "network");
        assert_ne!(base, palw_vertex_message_v1(h(1), &bond(2), 5, 5, 3, h(9)), "seat");
        assert_ne!(base, palw_vertex_message_v1(h(1), &bond(1), 6, 5, 3, h(9)), "round");
        assert_ne!(base, palw_vertex_message_v1(h(1), &bond(1), 5, 6, 3, h(9)), "signed daa");
        assert_ne!(base, palw_vertex_message_v1(h(1), &bond(1), 5, 5, 4, h(9)), "leaf count");
        assert_ne!(base, palw_vertex_message_v1(h(1), &bond(1), 5, 5, 3, h(8)), "root");
        let v = vertex(vec![verdict_leaf(1, PalwReceiptVerdictV2::Valid)]);
        assert_eq!(
            palw_vertex_header_message_v1(h(0xD0), &v.header()),
            palw_vertex_message_v1(h(0xD0), &v.seat_bond, v.round, v.signed_daa, 1, v.leaves_root),
            "a header's message is the vertex's"
        );
    }

    /// The root is a Merkle root with the odd node promoted: a leaf set is not another's, whatever its length.
    #[test]
    fn the_root_is_order_and_length_sensitive() {
        let leaves: Vec<PalwVertexLeafV1> = (1..=5).map(|i| verdict_leaf(i, PalwReceiptVerdictV2::Valid)).collect();
        let root = palw_vertex_root_of_leaves_v1(&leaves).unwrap();
        assert_ne!(root, palw_vertex_root_of_leaves_v1(&leaves[..4]).unwrap());
        let mut swapped = leaves.clone();
        swapped.swap(0, 4);
        assert_ne!(root, palw_vertex_root_of_leaves_v1(&swapped).unwrap());
        assert_eq!(palw_vertex_root_of_leaves_v1(&[]), None);
        let one = palw_vertex_leaf_hash_v1(&leaves[0]);
        assert_eq!(palw_vertex_root_v1(&[one]), Some(one), "one leaf is its own root");
    }

    /// A compact reference names a claim by its bound DAA and a 16-byte prefix; an ambiguous one names none.
    #[test]
    fn a_compact_reference_names_one_claim_or_none() {
        let claim = h(7);
        let compact = PalwClaimRefV1::compact_of(&claim, 123).unwrap();
        assert!(compact.names(&claim, 123));
        assert!(!compact.names(&claim, 124), "another bound DAA");
        assert!(!compact.names(&h(8), 123), "another prefix");
        assert!(PalwClaimRefV1::compact_of(&claim, u64::from(u32::MAX) + 1).is_none(), "a DAA past 32 bits cannot be compacted");
        assert!(PalwClaimRefV1::Full(claim).names(&claim, 0));
        // Leaf sizes: the RFC's 66 / 22 bytes plus the verdict tag (Valid is one byte).
        let full = verdict_leaf(1, PalwReceiptVerdictV2::Valid);
        let compact_leaf = PalwVertexLeafV1::Verdict { claim: compact, verdict: PalwReceiptVerdictV2::Valid };
        assert_eq!(full.encoded_len(), 1 + 1 + 64 + 1);
        assert_eq!(compact_leaf.encoded_len(), 1 + 1 + 4 + 16 + 1);
    }

    /// Part III's tables: the RFC's numbers.
    #[test]
    fn part_iii_reproduces_the_rfc_tables() {
        use PalwCheckModeV1::*;
        // m: five seats full; three at quorum; k = 4 of N = 299 sampled by five seats.
        assert_eq!(palw_vertex_m_v1(FullToFinal, 5, 3), 5_000_000);
        assert_eq!(palw_vertex_m_v1(FullAtQuorum, 5, 3), 3_000_000);
        assert_eq!(palw_vertex_m_v1(Sampled { k: 4, n: 299 }, 5, 3), 66_889);
        // Escape of a one-point lie at f = 0.1 / 0.2 / 0.33 (ppm): f^5 = 10 / 320 / 3,913; f^3 = 1,000 / 8,000 / 35,937.
        for (f, five, three) in [(100_000u64, 10u64, 1_000u64), (200_000, 320, 8_000), (330_000, 3_913, 35_937)] {
            let got5 = palw_vertex_escape_ppm_v1(FullToFinal, 5, 3, f);
            let got3 = palw_vertex_escape_ppm_v1(FullAtQuorum, 5, 3, f);
            assert!(got5.abs_diff(five) <= 2, "f = {f}: {got5} vs {five}");
            assert!(got3.abs_diff(three) <= 2, "f = {f}: {got3} vs {three}");
        }
        // Sampled k = 4 of N = 299: 94.1 % / 94.8 % / 95.6 % escape.
        for (f, want) in [(100_000u64, 941_000u64), (200_000, 948_000), (330_000, 956_000)] {
            let got = palw_vertex_escape_ppm_v1(Sampled { k: 4, n: 299 }, 5, 3, f);
            assert!(got.abs_diff(want) <= 2_000, "f = {f}: {got} vs {want}");
        }
        // ADR-0098's measured catch of a one-token lie by five seats at k = 4 of N = 299 is 6.51 %: the escape at f = 0.
        let catch = 1_000_000 - palw_vertex_escape_ppm_v1(Sampled { k: 4, n: 299 }, 5, 3, 0);
        assert!(catch.abs_diff(65_100) <= 1_000, "{catch}");
        // The slash multiple at a 6 % catch is about 17x the gain; at certainty, once.
        let multiple = palw_vertex_required_slash_multiple_ppm_v1(60_000).unwrap();
        assert!((16_000_000..=17_000_000).contains(&multiple), "{multiple}");
        assert_eq!(palw_vertex_required_slash_multiple_ppm_v1(1_000_000), Some(1_000_000));
        assert_eq!(palw_vertex_required_slash_multiple_ppm_v1(0), None);
        assert!(palw_vertex_slash_deters_v1(100, 1_700, 60_000));
        assert!(!palw_vertex_slash_deters_v1(100, 1_000, 60_000));
        // Escalation.
        assert_eq!(palw_vertex_escalation_v1(true, true), PalwEscalationV1::FileValid);
        assert_eq!(palw_vertex_escalation_v1(true, false), PalwEscalationV1::EscalateToExactCourt);
        assert_eq!(palw_vertex_escalation_v1(false, true), PalwEscalationV1::Abstain);
        assert_eq!(palw_vertex_permille_of_v1(1_000_000, PALW_VERTEX_EQUIVOCATION_PENALTY_PERMILLE_V1), Some(100_000));
        assert_eq!(palw_vertex_permille_of_v1(u128::MAX, 100), None, "checked, never wrapped");
    }

    /// A tally counts a seat once and sums its `Valid`s.
    #[test]
    fn a_tally_knows_who_answered() {
        let mut tally = PalwVertexTallyV1::default();
        tally.counted.push(PalwVertexCountedV1 { seat: bond(1), verdict: PalwReceiptVerdictV2::Valid, signed_daa: 5 });
        tally.counted.push(PalwVertexCountedV1 { seat: bond(2), verdict: PalwReceiptVerdictV2::Incapable, signed_daa: 5 });
        assert!(tally.answered(&bond(1)) && tally.answered(&bond(2)) && !tally.answered(&bond(3)));
        assert_eq!(tally.valid(), 1);
    }
}
