//! **The prompt's token ids, as a tiled Merkle commitment — ADR-0081 Decision 3.**
//!
//! # Why this module exists
//!
//! [`crate::palw_v2::prompt_token_ids_hash_v2`] is `put_u32_seq(ids); keyed64(DOMAIN)` — a FLAT
//! digest. A flat digest cannot be opened, so proving *one* id means carrying *all* of them, and
//! two consumers pay that for the whole life of a claim:
//!
//! * [`crate::palw_step_refute::check_execution_step_refutation_v1`] needs the ids to adjudicate an
//!   embedding gather, and the ordering discipline ("the carried ids are checked BEFORE any of them
//!   is read", G5d) forces it to recompute the whole flat hash first.
//! * `derive_court_cost_v1` therefore charges `n_ctx × 4` bytes on EVERY node of the graph. At
//!   `n_ctx` 512 that is 2 KiB per node against an 80 KiB carrier
//!   ([`crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES`]); at 32,768 it is 128 KiB and nothing fits.
//!
//! The gather reads exactly ONE id — `prompt_ids[coordinate.position]` — so the whole list is
//! carried to authenticate a single `u32`. A Merkle root over the ids replaces that with one tile
//! plus `⌈log₂ tiles⌉` path elements: `prompt_ids_close_bytes_v1` measures the difference at
//! 2,048 → 472 bytes at `n_ctx` 512 and 131,072 → 856 at 32,768.
//!
//! # What this does NOT buy, measured
//!
//! It does not make any context admissible. The prompt-id term is a stable **~0.1% of the close it
//! sits in** at every context measured — the floor's court closes at 52,704 bytes at `n_ctx` 12,
//! passes the 81,920-byte carrier at `n_ctx` 20, and reaches 2,105,024 at `n_ctx` 512, of which
//! the whole flat id term is 2,048. Arming this fence moves no class across the ceiling at any
//! context, and at `n_ctx` 30 it makes the close 88 bytes *worse*. What Decision 3 buys is the
//! term's SHAPE: `⌈log₂⌉` instead of linear, which is a property the flat form can never have and
//! which a long context needs from EVERY term. The ones that dominate the close — the history runs
//! and their Merkle paths — are ADR-0077 Decision 11's business and are not touched here.
//! `palw_class_admission_v2::tests::the_prompt_id_term_is_the_openings_size_past_the_fence`
//! asserts all of that, so the claim cannot drift from the derivation.
//!
//! # ADR-0081's OTHER decisions do not live here
//!
//! ADR-0080 and ADR-0081 are **REFUTED IN PART**: the segment chain they proposed is not
//! checkable, because `PalwDerivedArtifactV1` carries a singular `claim_id`/`output_root`,
//! `fp_work_id_v1` makes a second segment `DuplicateWork`, and `checkpoint_genesis_prev_v2`
//! derives its genesis link from `job_context_hash` alone. Decision 3 survives that refutation
//! **because it needs no cross-claim anything**: it is one job's prompt, committed openably, and
//! it lowers the close cost of every long-context design — including a competing one that chunks
//! the REDUCTIONS in step space rather than the close.
//!
//! # One tiling idiom, not two
//!
//! This follows [`crate::palw_step_refute::tiled_logits_scheme_id_v1`] /
//! [`crate::palw_step_refute::PALW_LOGITS_TILE_LANES`] deliberately and in every detail: leaves are
//! fixed-width tiles with a ragged tail, the tree is
//! [`crate::palw_step_leg::step_merkle_root_v1`]'s promote-odd tree (ONE Merkle implementation in
//! this codebase), the opening is a [`crate::palw_step_leg::PalwStepOpeningV1`] walked by
//! [`crate::palw_step_leg::step_opening_root_v1`], and the committed value is an OUTER keyed hash
//! over the tree's root rather than the bare root. A second tiling idiom in one codebase is how two
//! readers come to disagree about what a tile is.
//!
//! # Nothing here is armed
//!
//! [`PalwPromptIdsFormV1::Flat`] is what every shipped preset runs and what
//! [`crate::palw_v2::PalwJobContextV2::from_envelope`] writes. The Merkle form is selected by
//! `Params::palw_prompt_ids_merkle`, `None` on every preset, so a build carrying this module
//! commits byte-identically to a build without it.

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::Hash64;

use crate::palw_step_leg::{PalwStepOpeningV1, step_merkle_root_v1, step_opening_root_v1};

// ---------------------------------------------------------------------------------------------
// The scheme's constants
// ---------------------------------------------------------------------------------------------

/// **Ids per tile: 32, which is 128 bytes of `u32`.**
///
/// Derived rather than picked. One opening costs `4·T` bytes of tile plus
/// `64·⌈log₂(N/T)⌉` bytes of path (`PATH_ELEMENT_BYTES` is 64 in the cost bound), so the total is
/// `f(T) = 4T + 64·log₂(N/T)`, minimised at `T = 64/(4·ln 2) ≈ 23`. Measured on the two contexts
/// that decide anything — `f(T)` at `N = 512` is 384 / **384** / 448 for `T` = 16 / 32 / 64, and at
/// `N = 32,768` it is 768 / **768** / 832 — so 16 and 32 tie for the minimum at both and 32 is the
/// one that builds half as many leaves. Ten percent either side of this changes no admission
/// decision; a `T` of 512 would, which is why the number is derived at all.
pub const PALW_PROMPT_IDS_TILE_LEN: u32 = 32;

/// Bytes of an opening that are not tile ids and not path elements: `prompt_token_count` (4),
/// `tile_index` (4), the opening's `leaf_index` (8) and `leaf_hash` (64), and borsh's two `u32`
/// vector length prefixes (8). Charged so the cost bound prices what the carrier actually relays
/// rather than what the scheme would cost if openings were free.
pub const PALW_PROMPT_IDS_OPENING_HEADER_BYTES: u64 = 4 + 4 + 8 + 64 + 4 + 4;

pub const PALW_PROMPT_IDS_DOMAIN_TILE: &[u8] = b"misaka-palw/prompt-token-ids/tile/v1";
pub const PALW_PROMPT_IDS_DOMAIN_ROOT: &[u8] = b"misaka-palw/prompt-token-ids/root/v1";
pub const PALW_PROMPT_IDS_DOMAIN_FORM: &[u8] = b"misaka-palw/prompt-token-ids/form/v1";

fn prompt_ids_keyed(domain: &'static [u8], parts: &[&[u8]]) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(domain).to_state();
    for p in parts {
        h.update(&(p.len() as u64).to_le_bytes());
        h.update(p);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

// ---------------------------------------------------------------------------------------------
// Which form a network's `prompt_token_ids_hash` is
// ---------------------------------------------------------------------------------------------

/// **Which commitment a job's `prompt_token_ids_hash` slot holds.**
///
/// One slot, two occupants — the [`crate::palw_step_refute::PalwDecodeTokenPinV1`] shape, for its
/// reason: a checker that assumed the occupant would authenticate material the network never
/// committed. The form is a property of the RULESET (`Params::palw_prompt_ids_merkle`), never of
/// the carrier, so a challenger picks what to carry and never which rules apply (ADR-0046).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PalwPromptIdsFormV1 {
    /// [`crate::palw_v2::prompt_token_ids_hash_v2`] — the flat digest every shipped preset runs.
    Flat,
    /// [`prompt_token_ids_root_v1`] — the tiled Merkle root ADR-0081 Decision 3 installs.
    MerkleV1,
}

/// **The form's identity, so it can be bound rather than assumed.**
///
/// The `flat_logits_scheme_id_v1` precedent exactly: the flat prompt hash predates any name for
/// itself, so nothing could pin a network to it and a network admitted under one form could be
/// prosecuted under the other. A constant of the DOMAIN string plus the form's own byte string,
/// never of a version number, so two forms cannot collide by both being "1".
pub fn prompt_ids_form_id_v1(form: PalwPromptIdsFormV1) -> Hash64 {
    let name: &[u8] = match form {
        PalwPromptIdsFormV1::Flat => b"flat-prompt-token-ids-v2",
        PalwPromptIdsFormV1::MerkleV1 => b"tiled-merkle-prompt-token-ids-v1",
    };
    prompt_ids_keyed(PALW_PROMPT_IDS_DOMAIN_FORM, &[name])
}

/// The commitment a job carries in `prompt_token_ids_hash`, under the form the ruleset selects.
///
/// The ONE place the two forms meet. A caller that switched on the form itself would be a second
/// spelling of a consensus hash, which is the defect this codebase names "a root the court
/// recomputes differently is an honest producer who can neither be convicted nor paid".
pub fn prompt_token_ids_commitment_v1(form: PalwPromptIdsFormV1, ids: &[u32]) -> Result<Hash64, PalwPromptIdsError> {
    match form {
        PalwPromptIdsFormV1::Flat => Ok(crate::palw_v2::prompt_token_ids_hash_v2(ids)),
        PalwPromptIdsFormV1::MerkleV1 => prompt_token_ids_root_v1(ids),
    }
}

// ---------------------------------------------------------------------------------------------
// The commitment
// ---------------------------------------------------------------------------------------------

/// How many tiles a prompt of `count` ids occupies, or `None` when the count is past what the
/// step tree can hold. Derived in one place because the builder, the verifier and the cost bound
/// all need it and a bound that guessed would drift from the walk.
pub fn prompt_ids_tile_count_v1(count: u64) -> Option<u64> {
    let tiles = count.div_ceil(u64::from(PALW_PROMPT_IDS_TILE_LEN));
    (tiles <= crate::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES).then_some(tiles)
}

/// The canonical width of tile `tile_index` of a `count`-id prompt: the scheme width, except for
/// a ragged last tile. `None` when the tile is past the tree.
pub fn prompt_ids_tile_len_v1(count: u64, tile_index: u64) -> Option<usize> {
    let tiles = prompt_ids_tile_count_v1(count)?;
    if tile_index >= tiles {
        return None;
    }
    let tile = u64::from(PALW_PROMPT_IDS_TILE_LEN);
    let remainder = count % tile;
    Some(if tile_index + 1 == tiles && remainder != 0 { remainder as usize } else { tile as usize })
}

/// **One tile's leaf hash: the prompt's total count, the tile index and the ids.**
///
/// `count` is inside every leaf for the reason
/// [`crate::palw_step_refute::tiled_logits_tile_leaf_v1`] puts the job context inside its own — a
/// leaf that verifies under one commitment must not verify under another. Here that is
/// specifically a truncation guard: without the count, tile 0 of a 64-id prompt and tile 0 of the
/// 32-id prompt that is its prefix are the same bytes, and a challenger could open the short
/// prompt's tile against the long prompt's root.
///
/// The job context is deliberately NOT bound in: this root IS a field of
/// [`crate::palw_v2::PalwJobContextV2`], so binding the context would be circular. The ids and
/// their count are what the commitment is over, and two jobs with the same prompt and the same
/// length have the same prompt — there is nothing to separate.
pub fn prompt_ids_tile_leaf_v1(count: u64, tile_index: u64, ids: &[u32]) -> Hash64 {
    let mut bytes = Vec::with_capacity(ids.len() * 4);
    for id in ids {
        bytes.extend_from_slice(&id.to_le_bytes());
    }
    prompt_ids_keyed(PALW_PROMPT_IDS_DOMAIN_TILE, &[&count.to_le_bytes(), &tile_index.to_le_bytes(), &bytes])
}

/// **The commitment's OUTER hash** — the count and the tile tree's root under this module's own
/// domain, so the committing side and every verifier share one derivation
/// ([`crate::palw_step_refute::tiled_logits_outer_root_v1`]'s reason: "the ids are inside the
/// trace root" must be one implementation rather than an agreement between two).
///
/// A bare Merkle root would also have been a valid commitment; it is wrapped because a bare root
/// is a value some other tree could also produce, and because an EMPTY prompt has no tree at all
/// (`step_merkle_root_v1` refuses zero leaves) while it does have a commitment. Zero ids commit to
/// this hash over a zero `Hash64`, which no non-empty prompt can alias — the count is in the
/// preimage.
pub fn prompt_ids_outer_root_v1(count: u64, tile_tree_root: &Hash64) -> Hash64 {
    prompt_ids_keyed(PALW_PROMPT_IDS_DOMAIN_ROOT, &[&count.to_le_bytes(), tile_tree_root.as_byte_slice()])
}

/// **The tiled Merkle root over a prompt's canonical token ids** — ADR-0081 Decision 3's
/// commitment, and what `prompt_token_ids_hash` holds past `Params::palw_prompt_ids_merkle`.
pub fn prompt_token_ids_root_v1(ids: &[u32]) -> Result<Hash64, PalwPromptIdsError> {
    let count = ids.len() as u64;
    let tiles = prompt_ids_tile_count_v1(count).ok_or(PalwPromptIdsError::PromptLongerThanTheStepTree {
        count,
        max: crate::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES * u64::from(PALW_PROMPT_IDS_TILE_LEN),
    })?;
    if tiles == 0 {
        return Ok(prompt_ids_outer_root_v1(0, &Hash64::default()));
    }
    let root = step_merkle_root_v1(&prompt_ids_tile_leaves_v1(ids)).map_err(|_| PalwPromptIdsError::PromptLongerThanTheStepTree {
        count,
        max: crate::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES * u64::from(PALW_PROMPT_IDS_TILE_LEN),
    })?;
    Ok(prompt_ids_outer_root_v1(count, &root))
}

fn prompt_ids_tile_leaves_v1(ids: &[u32]) -> Vec<Hash64> {
    let count = ids.len() as u64;
    ids.chunks(PALW_PROMPT_IDS_TILE_LEN as usize)
        .enumerate()
        .map(|(t, chunk)| prompt_ids_tile_leaf_v1(count, t as u64, chunk))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// The opening
// ---------------------------------------------------------------------------------------------

/// **What a challenger carries instead of the whole prompt: one tile and its path.**
///
/// `prompt_token_count` is carried rather than read off the job context on purpose. It is inside
/// every leaf preimage, so a carrier that lies about it produces leaves that do not hash; carrying
/// it makes the verifier's FIRST check "is this the count the job declared", which is a comparison
/// of two integers ahead of any hashing — the same structural-bounds-before-hashing order
/// `check_base0_decode_pin` documents.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPromptIdsOpeningV1 {
    /// The prompt length the tree was built over — `job_context.declared_prefill_tokens`.
    pub prompt_token_count: u32,
    /// Which tile this opens (0-based; `position / PALW_PROMPT_IDS_TILE_LEN`).
    pub tile_index: u32,
    /// The tile's ids verbatim, in canonical order.
    pub tile_ids: Vec<u32>,
    /// The tile leaf's membership proof in the commitment's tree.
    pub opening: PalwStepOpeningV1,
}

/// **A verified view of some of a prompt's ids, addressed by ABSOLUTE position.**
///
/// The only thing an adjudicating kernel is ever handed. A window can be produced two ways — from
/// a whole carried id list, or from a verified opening — and in both cases it exists only after the
/// material behind it has been matched against `job_context.prompt_token_ids_hash`. The type is
/// what makes the G5d ordering discipline structural rather than a comment: there is no
/// constructor that skips the check, so an unverified opening cannot be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPromptIdWindowV1<'a> {
    base: u32,
    ids: &'a [u32],
}

impl<'a> PalwPromptIdWindowV1<'a> {
    /// The window a refutation that addresses no gather carries. Every read of it is `None`, which
    /// every gather turns into `Unadjudicable` — the same answer an out-of-range index gave before
    /// windows existed.
    pub const EMPTY: PalwPromptIdWindowV1<'static> = PalwPromptIdWindowV1 { base: 0, ids: &[] };

    /// The whole prompt, already checked against the job's FLAT commitment by the caller.
    ///
    /// **`pub(crate)`, which is what makes the type an invariant rather than a convention.** The
    /// only PUBLIC way to obtain a window is [`verify_prompt_ids_opening_v1`], so no code outside
    /// this crate can hand a gather ids that nothing authenticated. Inside the crate there is
    /// exactly one caller — `check_execution_step_refutation_v1`, on the line after it compares
    /// [`crate::palw_v2::prompt_token_ids_hash_v2`] of those ids against
    /// `job_context.prompt_token_ids_hash` — and that comparison is the check this constructor's
    /// name asserts has happened.
    pub(crate) fn whole_checked(ids: &'a [u32]) -> Self {
        Self { base: 0, ids }
    }

    /// The id at an absolute prompt position, or `None` when this window does not cover it.
    ///
    /// `None` is not "0" and not "the first id": a gather that asks for a position outside the
    /// carried tile has been handed evidence for a different coordinate, and the caller answers
    /// `Unadjudicable` rather than convicting on an id nobody proved.
    pub fn at(&self, position: u32) -> Option<u32> {
        let offset = position.checked_sub(self.base)? as usize;
        self.ids.get(offset).copied()
    }

    /// Whether the window carries nothing — the "this refutation addresses no gather" case.
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
}

/// Why an opening is not evidence.
///
/// Every variant is a REFUSAL, never a verdict either way: malformed evidence gets the material
/// thrown out, exactly as `check_tiled_decode_token_refutation_v1` treats a pin that fails any
/// recomputation.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwPromptIdsError {
    #[error("the prompt is longer than the step tree can commit ({count} ids, max {max})")]
    PromptLongerThanTheStepTree { count: u64, max: u64 },
    #[error("the opening's prompt length is not the one the job context declares (opening {opening}, job {job})")]
    CountIsNotTheJobs { opening: u32, job: u32 },
    #[error("an empty prompt has no tile to open")]
    EmptyPromptHasNoOpening,
    #[error("the opened tile index {tile_index} is past the commitment's {tiles} tiles")]
    TileIndexPastTheTree { tile_index: u64, tiles: u64 },
    #[error("the opened tile carries {got} ids but the scheme's width here is {expected}")]
    TileIsNotTheSchemeWidth { got: usize, expected: usize },
    #[error("the opening names leaf {leaf_index}, not the tile {tile_index} it claims to open")]
    OpeningDoesNotNameItsOwnTile { leaf_index: u64, tile_index: u64 },
    #[error("the opened tile's ids do not hash to its leaf")]
    TileIdsDoNotHashToItsLeaf,
    #[error("the opening's path does not walk")]
    OpeningDoesNotWalk,
    #[error("the opening does not bind the job's committed prompt root")]
    OpeningDoesNotBindTheJobsRoot,
}

impl PalwPromptIdsError {
    /// The refusal's name as a `&'static str`, for the callers whose own error type carries a
    /// static reason (`PalwStepRefuteError::InputSetNotCanonical`). Naming the refusal is the
    /// point: a court that refused evidence without saying which rule refused it is how a
    /// challenger learns nothing and an operator learns less.
    pub fn refusal(&self) -> &'static str {
        match self {
            Self::PromptLongerThanTheStepTree { .. } => "the prompt is longer than the step tree can commit",
            Self::CountIsNotTheJobs { .. } => "the prompt-ids opening's length is not the one the job context declares",
            Self::EmptyPromptHasNoOpening => "an empty prompt has no tile to open",
            Self::TileIndexPastTheTree { .. } => "the opened prompt-ids tile is past the commitment's tiles",
            Self::TileIsNotTheSchemeWidth { .. } => "an opened prompt-ids tile is not the scheme's width",
            Self::OpeningDoesNotNameItsOwnTile { .. } => "the prompt-ids opening does not name its own tile",
            Self::TileIdsDoNotHashToItsLeaf => "the opened prompt ids do not hash to their leaf",
            Self::OpeningDoesNotWalk => "the prompt-ids opening does not walk",
            Self::OpeningDoesNotBindTheJobsRoot => "the prompt-ids opening does not bind the job's committed prompt root",
        }
    }
}

/// **The prover's side: the opening that proves the id at `position`.**
///
/// Exported beside the root builder rather than reimplemented by each challenger, for
/// [`crate::palw_step_leg::step_merkle_path_v1`]'s stated reason — a third copy of a promote-odd
/// walk is the "second name-to-bytes mapping" defect, and it fails as an opening nobody can verify.
pub fn prompt_ids_opening_v1(ids: &[u32], position: u32) -> Result<PalwPromptIdsOpeningV1, PalwPromptIdsError> {
    let count = ids.len() as u64;
    let tiles = prompt_ids_tile_count_v1(count).ok_or(PalwPromptIdsError::PromptLongerThanTheStepTree {
        count,
        max: crate::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES * u64::from(PALW_PROMPT_IDS_TILE_LEN),
    })?;
    if tiles == 0 {
        return Err(PalwPromptIdsError::EmptyPromptHasNoOpening);
    }
    let tile_index = u64::from(position) / u64::from(PALW_PROMPT_IDS_TILE_LEN);
    if tile_index >= tiles {
        return Err(PalwPromptIdsError::TileIndexPastTheTree { tile_index, tiles });
    }
    let leaves = prompt_ids_tile_leaves_v1(ids);
    let opening = crate::palw_step_leg::step_opening_v1(&leaves, tile_index).map_err(|_| PalwPromptIdsError::OpeningDoesNotWalk)?;
    let start = (tile_index as usize) * PALW_PROMPT_IDS_TILE_LEN as usize;
    let end = (start + PALW_PROMPT_IDS_TILE_LEN as usize).min(ids.len());
    Ok(PalwPromptIdsOpeningV1 {
        prompt_token_count: ids.len() as u32,
        tile_index: tile_index as u32,
        tile_ids: ids[start..end].to_vec(),
        opening,
    })
}

/// **The verifier: an opening becomes a readable window only by binding the job's own root.**
///
/// Checked in this order, and the order is the rule:
///
/// 1. structural bounds — the declared count, the tile index, the tile's canonical width — which
///    cost a few integer comparisons, so oversized junk is refused before any hashing;
/// 2. the tile's ids against the leaf the opening names;
/// 3. the walked tree root, wrapped by [`prompt_ids_outer_root_v1`], against the job's committed
///    `prompt_token_ids_hash`;
/// 4. only then does the caller get a window it can read an id out of.
///
/// Step 3 before step 4 is the whole of the G5d discipline: unchecked, a challenger names whatever
/// ids convict an honest producer, because the ids ARE the basis on which a gather's correct output
/// is decided. The window type is what makes that structural — there is no other constructor
/// reachable from an opening.
pub fn verify_prompt_ids_opening_v1<'a>(
    committed_root: &Hash64,
    declared_prefill_tokens: u32,
    opening: &'a PalwPromptIdsOpeningV1,
) -> Result<PalwPromptIdWindowV1<'a>, PalwPromptIdsError> {
    if opening.prompt_token_count != declared_prefill_tokens {
        return Err(PalwPromptIdsError::CountIsNotTheJobs { opening: opening.prompt_token_count, job: declared_prefill_tokens });
    }
    let count = u64::from(opening.prompt_token_count);
    let tiles = prompt_ids_tile_count_v1(count).ok_or(PalwPromptIdsError::PromptLongerThanTheStepTree {
        count,
        max: crate::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES * u64::from(PALW_PROMPT_IDS_TILE_LEN),
    })?;
    if tiles == 0 {
        return Err(PalwPromptIdsError::EmptyPromptHasNoOpening);
    }
    let tile_index = u64::from(opening.tile_index);
    let expected = prompt_ids_tile_len_v1(count, tile_index).ok_or(PalwPromptIdsError::TileIndexPastTheTree { tile_index, tiles })?;
    if opening.tile_ids.len() != expected {
        return Err(PalwPromptIdsError::TileIsNotTheSchemeWidth { got: opening.tile_ids.len(), expected });
    }
    if opening.opening.leaf_index != tile_index {
        return Err(PalwPromptIdsError::OpeningDoesNotNameItsOwnTile { leaf_index: opening.opening.leaf_index, tile_index });
    }
    if opening.opening.leaf_hash != prompt_ids_tile_leaf_v1(count, tile_index, &opening.tile_ids) {
        return Err(PalwPromptIdsError::TileIdsDoNotHashToItsLeaf);
    }
    let tree_root = step_opening_root_v1(tiles, &opening.opening).map_err(|_| PalwPromptIdsError::OpeningDoesNotWalk)?;
    if prompt_ids_outer_root_v1(count, &tree_root) != *committed_root {
        return Err(PalwPromptIdsError::OpeningDoesNotBindTheJobsRoot);
    }
    let base = u32::try_from(tile_index * u64::from(PALW_PROMPT_IDS_TILE_LEN))
        .map_err(|_| PalwPromptIdsError::TileIndexPastTheTree { tile_index, tiles })?;
    Ok(PalwPromptIdWindowV1 { base, ids: &opening.tile_ids })
}

// ---------------------------------------------------------------------------------------------
// What it costs — the term `derive_court_cost_v1` charges on every node
// ---------------------------------------------------------------------------------------------

/// **What the prompt-id term costs on one close, per form.**
///
/// The court's own side of the scheme: `derive_court_cost_v1` charges exactly this on every node
/// (`shape.count_ids`), so the price a class is admitted at is the price its challengers pay. One
/// derivation, because a bound that guessed would drift from the carrier.
///
/// * [`PalwPromptIdsFormV1::Flat`] — `count × 4`, the whole list, which is what ADR-0077 §4 budgets
///   ("PublicDa carries `n_ctx × 4` bytes of ids — 2 KiB at 512").
/// * [`PalwPromptIdsFormV1::MerkleV1`] — one tile (`min(count, 32) × 4`), its path
///   (`64 × ⌈log₂ tiles⌉`, `PATH_ELEMENT_BYTES` matching the cost bound's own constant), and
///   [`PALW_PROMPT_IDS_OPENING_HEADER_BYTES`].
///
/// The Merkle form is not cheaper at every context and is not claimed to be: below ~50 ids the
/// header outweighs the list it replaces (208 bytes against 120 at `n_ctx` 30). It is
/// `⌈log₂⌉`-shaped, which is the property a long context needs and the flat form can never have.
pub fn prompt_ids_close_bytes_v1(form: PalwPromptIdsFormV1, prompt_token_count: u64) -> Option<u64> {
    match form {
        PalwPromptIdsFormV1::Flat => prompt_token_count.checked_mul(4),
        PalwPromptIdsFormV1::MerkleV1 => {
            let tiles = prompt_ids_tile_count_v1(prompt_token_count)?;
            if tiles == 0 {
                return Some(PALW_PROMPT_IDS_OPENING_HEADER_BYTES);
            }
            // `⌈log₂ tiles⌉`, spelled the way the cost bound spells every other path depth.
            let depth = u64::from(tiles.next_power_of_two().trailing_zeros());
            let tile_bytes = prompt_token_count.min(u64::from(PALW_PROMPT_IDS_TILE_LEN)).checked_mul(4)?;
            tile_bytes.checked_add(depth.checked_mul(64)?)?.checked_add(PALW_PROMPT_IDS_OPENING_HEADER_BYTES)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(n: usize) -> Vec<u32> {
        (0..n as u32).map(|i| i.wrapping_mul(2_654_435_761) % 151_936).collect()
    }

    /// **The commitment opens, at every tile of a ragged tree.** Commit / open / verify are held
    /// together here rather than by agreement between three call sites.
    #[test]
    fn every_tile_of_a_ragged_prompt_opens_against_its_root() {
        // 100 ids is 3 full tiles and a 4-id tail — the ragged case, which is the one a
        // fixed-width scheme gets wrong.
        let prompt = ids(100);
        let root = prompt_token_ids_root_v1(&prompt).expect("100 ids fit the tree");
        for position in 0..100u32 {
            let opening = prompt_ids_opening_v1(&prompt, position).expect("every position has a tile");
            let window = verify_prompt_ids_opening_v1(&root, 100, &opening).expect("an honest opening verifies");
            assert_eq!(window.at(position), Some(prompt[position as usize]), "position {position}");
        }
        let last = prompt_ids_opening_v1(&prompt, 99).expect("the ragged tile opens");
        assert_eq!(last.tile_ids.len(), 4, "100 % 32 == 4");
    }

    /// A window answers `None` outside its own tile — never 0, never the neighbouring id. A gather
    /// handed evidence for another coordinate must reach `Unadjudicable`, not a conviction.
    #[test]
    fn a_window_reads_nothing_outside_its_own_tile() {
        let prompt = ids(100);
        let root = prompt_token_ids_root_v1(&prompt).unwrap();
        let opening = prompt_ids_opening_v1(&prompt, 40).unwrap();
        let window = verify_prompt_ids_opening_v1(&root, 100, &opening).unwrap();
        assert_eq!(window.at(32), Some(prompt[32]));
        assert_eq!(window.at(63), Some(prompt[63]));
        assert_eq!(window.at(31), None, "the tile below");
        assert_eq!(window.at(64), None, "the tile above");
        assert_eq!(window.at(0), None);
        assert_eq!(PalwPromptIdWindowV1::EMPTY.at(0), None);
    }

    /// **An opening that does not bind the job's root is refused BY NAME**, in every way a
    /// challenger can fail to bind it: a foreign root, altered ids, a relabelled tile, a stolen
    /// path, and a count that is not the job's.
    #[test]
    fn an_opening_that_does_not_bind_the_jobs_root_is_refused_by_name() {
        let prompt = ids(100);
        let root = prompt_token_ids_root_v1(&prompt).unwrap();
        let honest = prompt_ids_opening_v1(&prompt, 40).unwrap();
        assert!(verify_prompt_ids_opening_v1(&root, 100, &honest).is_ok());

        // A different prompt of the SAME length: every structural gate passes and the root does
        // not, which is the only gate that can catch it.
        let foreign = prompt_token_ids_root_v1(&ids(100).iter().map(|i| i ^ 1).collect::<Vec<_>>()).unwrap();
        assert_eq!(verify_prompt_ids_opening_v1(&foreign, 100, &honest), Err(PalwPromptIdsError::OpeningDoesNotBindTheJobsRoot));

        // One id changed: the tile no longer hashes to the leaf the opening names.
        let mut lying = honest.clone();
        lying.tile_ids[3] ^= 1;
        assert_eq!(verify_prompt_ids_opening_v1(&root, 100, &lying), Err(PalwPromptIdsError::TileIdsDoNotHashToItsLeaf));

        // The tile relabelled: the leaf hash binds the index, so the leaf no longer matches.
        let mut moved = honest.clone();
        moved.tile_index = 0;
        assert_eq!(
            verify_prompt_ids_opening_v1(&root, 100, &moved),
            Err(PalwPromptIdsError::OpeningDoesNotNameItsOwnTile { leaf_index: 1, tile_index: 0 })
        );

        // Another tile's path under this tile's leaf: the walk reaches a root that is not the job's.
        let mut swapped = honest.clone();
        swapped.opening.siblings = prompt_ids_opening_v1(&prompt, 8).unwrap().opening.siblings;
        assert_eq!(verify_prompt_ids_opening_v1(&root, 100, &swapped), Err(PalwPromptIdsError::OpeningDoesNotBindTheJobsRoot));

        // A count that is not the job's is refused before any hashing happens.
        assert_eq!(
            verify_prompt_ids_opening_v1(&root, 99, &honest),
            Err(PalwPromptIdsError::CountIsNotTheJobs { opening: 100, job: 99 })
        );

        // A ragged tile padded to full width is not the scheme's width here.
        let mut padded = prompt_ids_opening_v1(&prompt, 99).unwrap();
        padded.tile_ids.push(7);
        assert_eq!(
            verify_prompt_ids_opening_v1(&root, 100, &padded),
            Err(PalwPromptIdsError::TileIsNotTheSchemeWidth { got: 5, expected: 4 })
        );

        // A tile past the tree.
        let mut past = honest.clone();
        past.tile_index = 4;
        assert_eq!(
            verify_prompt_ids_opening_v1(&root, 100, &past),
            Err(PalwPromptIdsError::TileIndexPastTheTree { tile_index: 4, tiles: 4 })
        );
    }

    /// A prefix of a prompt must not open against the longer prompt's root. The count is in every
    /// leaf preimage exactly to close this: tile 0 of a 32-id prompt and tile 0 of the 100-id
    /// prompt that starts with it carry identical ids.
    #[test]
    fn a_prefixs_tile_does_not_open_against_the_longer_prompts_root() {
        let long = ids(100);
        let short = long[..32].to_vec();
        let long_root = prompt_token_ids_root_v1(&long).unwrap();
        let short_opening = prompt_ids_opening_v1(&short, 0).unwrap();
        assert_eq!(short_opening.tile_ids, long[..32].to_vec(), "the two tiles are the same ids");
        assert_eq!(
            verify_prompt_ids_opening_v1(&long_root, 100, &short_opening),
            Err(PalwPromptIdsError::CountIsNotTheJobs { opening: 32, job: 100 })
        );
        // And with the count forced past the first gate, the leaf itself refuses.
        let mut relabelled = short_opening;
        relabelled.prompt_token_count = 100;
        assert_eq!(verify_prompt_ids_opening_v1(&long_root, 100, &relabelled), Err(PalwPromptIdsError::TileIdsDoNotHashToItsLeaf));
    }

    /// An empty prompt has a commitment and no opening. Both halves matter: the flat form hashes
    /// the empty list to a real value, so the Merkle form must too, and a "verified" opening of
    /// nothing would be a window a gather could read out of.
    #[test]
    fn an_empty_prompt_commits_and_cannot_be_opened() {
        let root = prompt_token_ids_root_v1(&[]).expect("the empty prompt commits");
        assert_eq!(root, prompt_ids_outer_root_v1(0, &Hash64::default()));
        assert_ne!(root, prompt_token_ids_root_v1(&[0]).unwrap());
        assert_eq!(prompt_ids_opening_v1(&[], 0), Err(PalwPromptIdsError::EmptyPromptHasNoOpening));
    }

    /// **The two forms are different commitments and are named as such.** A network cannot be
    /// admitted under one and prosecuted under the other by accident, because the ids differ and
    /// the roots differ.
    #[test]
    fn the_two_prompt_id_forms_are_distinct_and_named() {
        let prompt = ids(100);
        let flat = prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, &prompt).unwrap();
        let merkle = prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::MerkleV1, &prompt).unwrap();
        assert_eq!(flat, crate::palw_v2::prompt_token_ids_hash_v2(&prompt));
        assert_eq!(merkle, prompt_token_ids_root_v1(&prompt).unwrap());
        assert_ne!(flat, merkle);
        assert_ne!(prompt_ids_form_id_v1(PalwPromptIdsFormV1::Flat), prompt_ids_form_id_v1(PalwPromptIdsFormV1::MerkleV1));
    }

    /// **The id term's growth is logarithmic — the four numbers, printed.**
    ///
    /// ADR-0081 Decision 3's whole claim. The flat term is `n_ctx × 4` and passes the 80 KiB
    /// carrier between 4,096 and 32,768; the opening's is `4·min(n,32) + 64·⌈log₂⌈n/32⌉⌉ + 88` and
    /// every DOUBLING of the context adds exactly one 64-byte path element.
    #[test]
    fn the_prompt_id_close_term_grows_logarithmically() {
        let carrier = crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES;
        let mut measured = Vec::new();
        for n_ctx in [30u64, 512, 4_096, 32_768] {
            let flat = prompt_ids_close_bytes_v1(PalwPromptIdsFormV1::Flat, n_ctx).unwrap();
            let merkle = prompt_ids_close_bytes_v1(PalwPromptIdsFormV1::MerkleV1, n_ctx).unwrap();
            println!("n_ctx {n_ctx:>6}: flat {flat:>7} bytes, opening {merkle:>4} bytes (carrier {carrier})");
            measured.push((n_ctx, flat, merkle));
        }
        assert_eq!(
            measured,
            vec![(30u64, 120u64, 208u64), (512, 2_048, 472), (4_096, 16_384, 664), (32_768, 131_072, 856)],
            "the four numbers this decision is worth",
        );
        // Eight times the context is three doublings, and each doubling adds exactly one path
        // element (64 bytes) and nothing else.
        assert_eq!(measured[2].2 - measured[1].2, 3 * 64, "512 -> 4,096 is 3 doublings");
        assert_eq!(measured[3].2 - measured[2].2, 3 * 64, "4,096 -> 32,768 is 3 doublings");
        // And the shape holds across a whole sweep, not just at four points.
        let mut n = 32u64;
        while n < 1 << 24 {
            let here = prompt_ids_close_bytes_v1(PalwPromptIdsFormV1::MerkleV1, n).unwrap();
            let doubled = prompt_ids_close_bytes_v1(PalwPromptIdsFormV1::MerkleV1, n * 2).unwrap();
            assert_eq!(doubled - here, 64, "one path element per doubling, at n_ctx {n}");
            n *= 2;
        }
        // The reason the decision exists: the flat term alone passes the carrier, the opening's
        // does not come close.
        assert!(measured[3].1 > carrier, "the flat term at 32,768 is {} against an {carrier}-byte carrier", measured[3].1);
        assert!(measured[3].2 * 90 < carrier, "the opening's term is under a ninetieth of the carrier");
    }

    /// The prompt the scheme cannot commit is refused rather than silently truncated — the count
    /// past which `tiles` would overflow the step tree.
    #[test]
    fn a_prompt_past_the_step_tree_is_refused() {
        let max = crate::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES * u64::from(PALW_PROMPT_IDS_TILE_LEN);
        assert_eq!(prompt_ids_tile_count_v1(max), Some(crate::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES));
        assert_eq!(prompt_ids_tile_count_v1(max + 1), None);
        assert_eq!(prompt_ids_close_bytes_v1(PalwPromptIdsFormV1::MerkleV1, max + 1), None);
    }

    /// The scheme's identities are consensus-visible: a silent edit to a domain string or the tile
    /// width is a network that commits differently while claiming the same rules.
    #[test]
    fn the_prompt_ids_scheme_is_golden_pinned() {
        assert_eq!(PALW_PROMPT_IDS_TILE_LEN, 32);
        assert_eq!(PALW_PROMPT_IDS_OPENING_HEADER_BYTES, 88);
        let pin = |h: Hash64| faster_hex::hex_string(h.as_byte_slice())[..32].to_string();
        assert_eq!(pin(prompt_ids_form_id_v1(PalwPromptIdsFormV1::Flat)), "2e5a66ae84365966eb3068d6f7be5c9d");
        assert_eq!(pin(prompt_ids_form_id_v1(PalwPromptIdsFormV1::MerkleV1)), "f047fdc31c693d2032912871aef09933");
        assert_eq!(pin(prompt_token_ids_root_v1(&[1, 2, 3, 4, 5]).unwrap()), "02bca4cb945994ab06703e7df49bf17b");
        assert_eq!(pin(prompt_token_ids_root_v1(&ids(100)).unwrap()), "e4cc01395aebc5f7b021f1b1247c25b9");
        assert_eq!(pin(prompt_token_ids_root_v1(&[]).unwrap()), "cb6b8efaa8127bc6bdd93caf61a39e39");
    }
}
