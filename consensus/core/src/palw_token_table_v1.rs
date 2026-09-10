//! **ADR-0096 §10 B4 — the token tables are the build's.**
//!
//! # Why this module exists
//!
//! A constrained decode (ADR-0096 Decision 7) is tried by walking the automaton over token BYTES,
//! and the court that walks it is run by every node inside the fold. No node holds a tokenizer, and
//! the only tokenizer-derived value the chain can reach is the artifact's `tokenizer_commitment` —
//! a flat hash of `tokenizer.json`, from which no single token's bytes can be proven. A court that
//! needs "the bytes of lane `j`" therefore needs two things the flat hash is not: a commitment one
//! token can be OPENED against, and a place to find it that does not depend on which files a node
//! happens to hold (a verdict that depends on local files is a consensus split).
//!
//! This module is both. The commitment is a Merkle root over one leaf per id of the class's
//! vocabulary ([`token_table_root_v1`]); the place is this build: [`PALW_TOKEN_TABLES_V1`] pins
//! `(tokenizer_commitment, vocab_len, root)` for every tokenizer a class may constrain under, in
//! the shape ADR-0082 Decision 11 gave the Gumbel table — a tool generates the value
//! (`palw-token-table` in `misaka-palw-base0`), a constant pins it, and a test checks the constant
//! against the file. It is ADR-0067's rule applied to tokenizers: classes are chain data, kernels —
//! and now token tables — are the build. A tokenizer with no row cannot carry a version-6 job.
//!
//! # The leaf, and what "the bytes of an id" are
//!
//! `leaf(id) = H(PALW_TOKEN_TABLE_DOMAIN_V1 ‖ id_le32 ‖ len_le32 ‖ bytes)`
//! ([`token_table_leaf_v1`]), a keyed BLAKE2b-512 whose key is the domain. The id is inside the
//! leaf and the length prefixes the bytes, so the encoding is injective: no `(id, bytes)` pair can
//! be read as another.
//!
//! Which bytes an id has is decided by ONE function on the producing side
//! (`misaka_palw_base0::tokenizer::QwenTokenizer::constrained_rendering_v1`, which the table, the
//! engine's mask, the worker's stream and the rendered-segments hash all read), and it has three
//! cases:
//!
//! * an ordinary token: its byte-level rendering — the bytes `QwenTokenizer::token_bytes` gives;
//! * an **added (control) token** — `<|im_end|>`, `<|endoftext|>`, `<tool_call>`, every entry of
//!   the tokenizer's `added_tokens`: the EMPTY string. A control token is never answer content
//!   (ADR-0077 Decision 6 classifies every added token that way and refuses one reached from
//!   text), so its literal text — `<|im_start|>` is twelve perfectly good JSON-string bytes — must
//!   not be something a string-typed schema can admit;
//! * an id the tokenizer cannot render — a PADDED vocabulary id past the tokenizer's own table (the
//!   class's `vocab_size` is wider than the file), or a token holding a non-byte character: the
//!   EMPTY string.
//!
//! **The empty leaf helps nobody, because no constraint admits an empty rendering.** The table is
//! total over `0..vocab_len`, so an opening of such an id proves exactly one fact — the table gives
//! it no bytes — and the constraint module refuses to admit a rendering that advances no byte
//! (`palw_decode_constraint_v1::constraint_admits_lane_v1`, the form the producer's mask and the
//! court both call; a token that advances no byte would otherwise be admitted at EVERY state). So
//! an empty lane cannot beat a committed token (B5's two-disclosure arm: the beating lane must be
//! admitted), cannot be committed while the answer runs (B5's one-disclosure arm convicts it), and
//! is the end-of-generation token only by B7's rule, which reads the id and never its bytes. B6
//! compares a claimant's segment with the table's bytes; an honest worker renders through the same
//! function, so its segment for a control or padded id is empty too.
//! `no_constraint_admits_the_empty_leaf` pins that dependency HERE, where the claim is made.
//!
//! # The tree is the step tree, not a new one
//!
//! The leaves are committed by [`crate::palw_step_leg::step_merkle_root_capped_v1`]'s promote-odd
//! tree and an opening is walked by [`crate::palw_step_leg::step_opening_root_capped_v1`] — the
//! ONE Merkle implementation the newer commitments already share (`palw_prompt_ids_v1`'s tiles,
//! the tiled logits, the step leg). Chosen for four properties, all of which this table needs:
//!
//! * **It takes any leaf count.** 151,936 = 2⁷ · 1,187, and an odd node is PROMOTED, never
//!   duplicated (duplication is the second-preimage hole `palw_artifact` names: a tree over
//!   `[a, b, c]` and one over `[a, b, c, c]` share a root).
//! * **Its leaves are index-bound** (`step_merkle_leaf_v1(index, leaf)`), so even a leaf that did
//!   not name its id could not be moved to another one.
//! * **Its verifier derives the path's shape from `(index, count)`** and refuses a path that is
//!   short or long — here the count comes from the PIN, never from the carrier (the `palw_artifact`
//!   tree's opening states its own `leaf_count`, which is the wrong shape for a pinned table).
//! * **A court already runs it** on every tiled-logits and prompt-ids opening, so this table adds a
//!   leaf format and a wrapper, not an algorithm.
//!
//! The committed value wraps the tree's root with the table's length under its own domain
//! (`H(PALW_TOKEN_TABLE_ROOT_DOMAIN_V1 ‖ vocab_len_le64 ‖ tree_root)`), `palw_prompt_ids_v1`'s
//! outer-root idiom and for its reasons: a bare root is a value some other tree could also produce,
//! an empty table has no tree but does have a commitment, and with the length inside the root the
//! pin's two numbers are one commitment — a pin whose `vocab_len` were mistyped would verify no
//! opening at all rather than some of them.
//!
//! A path has **exactly the tree's own length for that id** ([`token_table_path_len_v1`]): the
//! tree's height `⌈log₂ vocab_len⌉`, less one for every level at which the id's node is the promoted
//! odd tail, which only ever happens along the table's right edge. At the pinned 151,936 ids every
//! id below 131,072 has the full 18 siblings and the rest have 16, 15, 12 or 11 (the added tokens,
//! 151,643–151,664, have 12). The verifier refuses any other count before it hashes anything.
//!
//! # The bound on one token's bytes: 256
//!
//! [`PALW_TOKEN_TABLE_MAX_TOKEN_BYTES_V1`] bounds what a carrier may state as one token's bytes,
//! so the cost of verifying an opening is bounded before the carrier is read. It must be at least
//! the longest rendering of every pinned table — an honest opening past it would be refused, and a
//! court that cannot open a token cannot try a lie about it (B6) or a lane that beats it (B5). It
//! is measured, not guessed: the longest Qwen2.5 token is **128 bytes** (id 56,940, a run of 128
//! spaces — whitespace and comment rules are carried as single tokens), and **121** of its ids are
//! longer than 64 bytes, so the first guess of 64 would have left 121 tokens untriable. The
//! bound is twice the measured longest so a second tokenizer's whitespace run does not force a
//! consensus change; the pin's test and the tool both refuse a table whose longest rendering
//! exceeds it.
//!
//! # Nothing here is armed
//!
//! The table is read only by the version-6 court arms (ADR-0096 §10 B5, B6), which sit behind
//! `Params::palw_fp_decode_constraint` — `None` on every preset. A build carrying this module
//! commits and adjudicates byte-identically to one without it.

use std::str::FromStr;

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::Hash64;

use crate::palw_step_leg::{
    PalwStepLegError, PalwStepOpeningV1, step_leg_max_opening_siblings_v1, step_merkle_path_capped_v1, step_merkle_root_capped_v1,
    step_opening_root_capped_v1, step_range_sibling_count_v1,
};

// ---------------------------------------------------------------------------------------------
// Domains and bounds
// ---------------------------------------------------------------------------------------------

/// The key every table LEAF is hashed under (ADR-0096 §10 B4).
pub const PALW_TOKEN_TABLE_DOMAIN_V1: &[u8] = b"misaka-palw/token-table/v1";

/// The key the table's ROOT — the tree root wrapped with the table's length — is hashed under. Its
/// own string, so a leaf can never be read as a root or a root as a leaf.
pub const PALW_TOKEN_TABLE_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/token-table/root/v1";

/// **The most bytes one token's rendering may have in an opening: 256** — twice the longest token
/// of the one pinned table (Qwen2.5's id 56,940, 128 spaces). The module header gives the
/// measurement; `the_bytes_bound_admits_the_longest_rendering_and_refuses_one_past_it` holds the
/// boundary, and the pin's test holds every pinned table under it.
pub const PALW_TOKEN_TABLE_MAX_TOKEN_BYTES_V1: usize = 256;

/// Bytes of an opening that are neither the token's bytes nor path elements: the id (4) and
/// borsh's two `u32` vector-length prefixes (4 + 4). Charged so the cost gate prices what the
/// carrier actually relays — [`token_table_opening_bytes_v1`] is exactly the borsh length.
pub const PALW_TOKEN_TABLE_OPENING_HEADER_BYTES_V1: u64 = 4 + 4 + 4;

/// One path element: a [`Hash64`].
const PATH_ELEMENT_BYTES: u64 = 64;

fn keyed64(key: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for part in parts {
        state.update(part);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

// ---------------------------------------------------------------------------------------------
// The commitment
// ---------------------------------------------------------------------------------------------

/// **One id's leaf: `H(domain ‖ id_le32 ‖ len_le32 ‖ bytes)`.**
///
/// `bytes` is the id's rendering as the table defines it — empty for an added (control) token and
/// for an id the tokenizer cannot render (see the module header). The empty leaf is one ordinary
/// value per id; what keeps it harmless is that no constraint admits an empty rendering, not
/// anything about the hash.
///
/// Total over any byte string; the verifier refuses bytes past
/// [`PALW_TOKEN_TABLE_MAX_TOKEN_BYTES_V1`] before it ever calls this.
pub fn token_table_leaf_v1(id: u32, bytes: &[u8]) -> Hash64 {
    keyed64(PALW_TOKEN_TABLE_DOMAIN_V1, &[&id.to_le_bytes(), &(bytes.len() as u32).to_le_bytes(), bytes])
}

/// The committed value: the table's length and its tree's root, under the root domain.
fn token_table_outer_root_v1(vocab_len: u64, tree_root: &Hash64) -> Hash64 {
    keyed64(PALW_TOKEN_TABLE_ROOT_DOMAIN_V1, &[&vocab_len.to_le_bytes(), tree_root.as_byte_slice()])
}

/// **The table's root over `leaves`, in id order** — `leaves[i]` is [`token_table_leaf_v1`] of id
/// `i`, and `leaves.len()` is the class's `vocab_len`.
///
/// The step tree's root, wrapped with the length. An empty table has no tree and commits to the
/// wrapper over a zero root; no opening verifies against it, because every id is out of range.
pub fn token_table_root_v1(leaves: &[Hash64]) -> Hash64 {
    let count = leaves.len() as u64;
    // The step tree refuses exactly two leaf sets: an empty one, answered here, and one past its
    // cap, which cannot happen when the cap passed is the count itself. The `unwrap_or_default` is
    // therefore unreachable, and fail-closed if it were not: a zero tree root under a non-zero
    // count is a root no opening reaches.
    let tree = if leaves.is_empty() { Hash64::default() } else { step_merkle_root_capped_v1(leaves, count).unwrap_or_default() };
    token_table_outer_root_v1(count, &tree)
}

/// **How many siblings an opening of `id` in a `vocab_len`-id table carries** — the step tree's own
/// path length for that leaf, [`step_range_sibling_count_v1`] of a one-leaf range, so the count the
/// verifier demands is computed by the function the tree's cost bounds already use rather than
/// restated. Meaningful only for `id < vocab_len`, which the verifier checks first.
pub fn token_table_path_len_v1(vocab_len: u32, id: u32) -> usize {
    step_range_sibling_count_v1(u64::from(vocab_len), u64::from(id), 1) as usize
}

// ---------------------------------------------------------------------------------------------
// The opening
// ---------------------------------------------------------------------------------------------

/// **One token's bytes and their path to the pinned root** — what a court close carries to prove
/// "id `id` renders as `bytes` in this class's table" (ADR-0096 §10 B5's beating lane, B6's
/// committed token).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTokenTableOpeningV1 {
    pub id: u32,
    /// The id's rendering as the table defines it; empty for a control or unrenderable id.
    pub bytes: Vec<u8>,
    /// The step-tree path, leaf to root, exactly [`token_table_path_len_v1`] long.
    pub siblings: Vec<Hash64>,
}

/// Why an opening is not evidence, or why a builder would not make one. Every variant is a
/// REFUSAL of the material, never a verdict about a claim.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwTokenTableError {
    #[error("token id {id} is not below the table's {vocab_len} ids")]
    IdOutOfRange { id: u32, vocab_len: u32 },
    #[error("the opening states {got} bytes for one token, past the {max}-byte bound")]
    TokenBytesTooLong { got: usize, max: usize },
    #[error("the opening carries {got} siblings, but id {id} of a {vocab_len}-id table has a path of exactly {expected}")]
    SiblingCount { id: u32, vocab_len: u32, got: usize, expected: usize },
    #[error("the opening does not reconstruct the pinned table root")]
    RootMismatch,
    #[error("the opening is for id {got}, not the id {expected} it was offered for")]
    AnotherId { expected: u32, got: u32 },
    #[error("the bytes are not the table's leaf for id {id}")]
    BytesAreNotTheLeaf { id: u32 },
    #[error("the step tree refused the path: {0}")]
    Tree(PalwStepLegError),
}

impl PalwTokenTableError {
    /// The refusal's name as a `&'static str`, for a caller whose own error carries a static reason
    /// (`PalwStepRefuteError::InputSetNotCanonical`) — `PalwPromptIdsError::refusal`'s shape.
    pub fn refusal(&self) -> &'static str {
        match self {
            Self::IdOutOfRange { .. } => "the token-table opening names an id past the class vocabulary",
            Self::TokenBytesTooLong { .. } => "the token-table opening states more bytes than any token may have",
            Self::SiblingCount { .. } => "the token-table opening's path is not the tree's own length for its id",
            Self::RootMismatch => "the token-table opening does not reconstruct the pinned table root",
            Self::AnotherId { .. } => "the token-table opening is for another id",
            Self::BytesAreNotTheLeaf { .. } => "the bytes are not the token table's leaf for that id",
            Self::Tree(_) => "the step tree refused the token-table path",
        }
    }
}

/// **The verifier: does `opening` prove its id's bytes under the table `(root, vocab_len)`?**
///
/// Both numbers come from the PIN ([`token_table_pin_for_v1`]), never from the carrier. Checked in
/// this order, and the order is the rule — every structural bound is integer arithmetic ahead of
/// any hashing, so an oversized carrier costs nothing to refuse:
///
/// 1. the id is below `vocab_len`;
/// 2. the bytes are within [`PALW_TOKEN_TABLE_MAX_TOKEN_BYTES_V1`];
/// 3. the path is exactly [`token_table_path_len_v1`] long — neither the promote levels (which
///    consume no sibling) nor a padding sibling can be smuggled in;
/// 4. the leaf, walked by [`step_opening_root_capped_v1`] and wrapped with `vocab_len`, is `root`.
///
/// Proving an id's bytes is all this does. That the id is the lane in dispute is the caller's to
/// check — [`token_table_proven_bytes_v1`] does both.
pub fn verify_token_table_opening_v1(
    root: &Hash64,
    vocab_len: u32,
    opening: &PalwTokenTableOpeningV1,
) -> Result<(), PalwTokenTableError> {
    if opening.id >= vocab_len {
        return Err(PalwTokenTableError::IdOutOfRange { id: opening.id, vocab_len });
    }
    if opening.bytes.len() > PALW_TOKEN_TABLE_MAX_TOKEN_BYTES_V1 {
        return Err(PalwTokenTableError::TokenBytesTooLong { got: opening.bytes.len(), max: PALW_TOKEN_TABLE_MAX_TOKEN_BYTES_V1 });
    }
    let expected = token_table_path_len_v1(vocab_len, opening.id);
    if opening.siblings.len() != expected {
        return Err(PalwTokenTableError::SiblingCount { id: opening.id, vocab_len, got: opening.siblings.len(), expected });
    }
    let width = u64::from(vocab_len);
    let step = PalwStepOpeningV1 {
        leaf_index: u64::from(opening.id),
        leaf_hash: token_table_leaf_v1(opening.id, &opening.bytes),
        siblings: opening.siblings.clone(),
    };
    let tree = step_opening_root_capped_v1(width, &step, width).map_err(PalwTokenTableError::Tree)?;
    if token_table_outer_root_v1(width, &tree) != *root {
        return Err(PalwTokenTableError::RootMismatch);
    }
    Ok(())
}

/// **The bytes of `id`, and only once they are proven** — [`verify_token_table_opening_v1`] plus
/// the check that the opening is for the id the caller is asking about. The form a court arm wants:
/// "the beating lane's rendering" is `token_table_proven_bytes_v1(root, vocab_len, beat_lane, …)`,
/// and a valid opening of some OTHER id is refused rather than read.
pub fn token_table_proven_bytes_v1<'a>(
    root: &Hash64,
    vocab_len: u32,
    id: u32,
    opening: &'a PalwTokenTableOpeningV1,
) -> Result<&'a [u8], PalwTokenTableError> {
    if opening.id != id {
        return Err(PalwTokenTableError::AnotherId { expected: id, got: opening.id });
    }
    verify_token_table_opening_v1(root, vocab_len, opening)?;
    Ok(&opening.bytes)
}

/// **The prover's side: the opening of `id`, from the whole table's leaves and that id's bytes.**
///
/// Fail-closed like every producer in this codebase: it refuses to build an opening the verifier
/// would refuse — an id past the table, bytes past the bound, or bytes that are not the leaf — so a
/// close builder that gets one back holds evidence, not a guess. The path is
/// [`step_merkle_path_capped_v1`]'s, the tree's own prover.
pub fn token_table_opening_from_leaves_v1(
    leaves: &[Hash64],
    id: u32,
    bytes: Vec<u8>,
) -> Result<PalwTokenTableOpeningV1, PalwTokenTableError> {
    let vocab_len = u32::try_from(leaves.len()).unwrap_or(u32::MAX);
    let Some(leaf) = leaves.get(id as usize) else {
        return Err(PalwTokenTableError::IdOutOfRange { id, vocab_len });
    };
    if bytes.len() > PALW_TOKEN_TABLE_MAX_TOKEN_BYTES_V1 {
        return Err(PalwTokenTableError::TokenBytesTooLong { got: bytes.len(), max: PALW_TOKEN_TABLE_MAX_TOKEN_BYTES_V1 });
    }
    if token_table_leaf_v1(id, &bytes) != *leaf {
        return Err(PalwTokenTableError::BytesAreNotTheLeaf { id });
    }
    let siblings = step_merkle_path_capped_v1(leaves, id as usize, leaves.len() as u64).map_err(PalwTokenTableError::Tree)?;
    Ok(PalwTokenTableOpeningV1 { id, bytes, siblings })
}

/// **The bytes an opening rides at** — its token bytes, its path and the header; exactly the
/// opening's borsh length, which is what a court close's cost gate charges for it.
pub fn token_table_opening_bytes_v1(opening: &PalwTokenTableOpeningV1) -> u64 {
    (opening.bytes.len() as u64)
        .saturating_add((opening.siblings.len() as u64).saturating_mul(PATH_ELEMENT_BYTES))
        .saturating_add(PALW_TOKEN_TABLE_OPENING_HEADER_BYTES_V1)
}

/// **The most an honest opening in a `vocab_len`-id table can cost**, priced before any opening
/// exists: the header, the byte bound and a full-height path (id 0's path is never shortened by a
/// promotion). 1,420 bytes for the pinned Qwen2.5 table — a rounding error beside an 80 KiB
/// carrier.
pub fn token_table_max_opening_bytes_v1(vocab_len: u32) -> u64 {
    PALW_TOKEN_TABLE_OPENING_HEADER_BYTES_V1
        + PALW_TOKEN_TABLE_MAX_TOKEN_BYTES_V1 as u64
        + PATH_ELEMENT_BYTES * step_leg_max_opening_siblings_v1(u64::from(vocab_len)) as u64
}

// ---------------------------------------------------------------------------------------------
// The pinned tables
// ---------------------------------------------------------------------------------------------

/// **One pinned table: which tokenizer, how many ids, and the root.**
///
/// Keyed by the artifact's `tokenizer_commitment` (`Base0ArtifactV1::tokenizer_commitment_of` over
/// the `tokenizer.json` bytes) because that is the tokenizer-derived value the chain can reach;
/// `vocab_len` is the CLASS's vocabulary — the registered profile's `vocab_size`, which is wider
/// than the tokenizer's own table (the padded ids get the empty leaf).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwTokenTablePinV1 {
    /// 128 lowercase hex characters.
    pub tokenizer_commitment_hex: &'static str,
    pub vocab_len: u32,
    /// [`token_table_root_v1`] of the table, 128 lowercase hex characters.
    pub root_hex: &'static str,
    /// Where the value came from, in words a person can re-run.
    pub source: &'static str,
}

/// **Qwen2.5's tokenizer, at the A16 dense class's vocabulary.**
///
/// The `tokenizer.json` `palw-class bind-tokenizer` bound into the testnet-11 dense artifact
/// (commitment `fa9a4352…a649bb`; 7,031,645 bytes, sha256 `c0382117…e87539`). It names 151,665
/// ids — 151,643 byte-level BPE tokens and 22 added tokens — and the class's logit row is 151,936
/// wide, so 271 padded ids and the 22 added tokens have the empty leaf (293 in all). The longest
/// rendering is 128 bytes (id 56,940), and every one of the 256 bytes is some id's whole rendering
/// (ids 0–255), which is what lets B7's finish rule read "no lane is admitted" as "no byte
/// continues".
///
/// Printed by `palw-token-table --tokenizer tokenizer.json --vocab-len 151936`; checked against
/// the file by `misaka_palw_base0::token_table::tests::the_pinned_qwen25_token_table_is_the_one_the_tokenizer_file_builds`,
/// and against the class profile by `the_pin_is_one_row_the_qwen25_tokenizer_at_the_a16_class_vocabulary`.
pub const PALW_TOKEN_TABLE_QWEN25_V1: PalwTokenTablePinV1 = PalwTokenTablePinV1 {
    tokenizer_commitment_hex: "fa9a43521e324f8482d88a2f4147ae2321202db8806b7c039322fc8a3d265ab482c35e0ab1bd2819b86d50449b0270db435823b002aef001c07a8c7c10a649bb",
    vocab_len: crate::palw_qwen25_profile::QWEN25_1_5B_A16.vocab_size,
    root_hex: "01e9c31cabc5b9cae91bfea41bb3268e620ed266cc2218337d813fb3133889d4e97291cd2df463ea85c9bb609bb823e61172d5d7c39fe47c9eb8e43dd9cd9a1f",
    source: "Qwen2.5-1.5B tokenizer.json (7,031,645 bytes, sha256 c0382117ea329cdf097041132f6d735924b697924d6f6fc3945713e96ce87539), \
             bound into the testnet-11 dense artifact by `palw-class bind-tokenizer`; root printed by \
             `palw-token-table --tokenizer tokenizer.json --vocab-len 151936`",
};

/// **Every tokenizer a class may constrain under.** One row today. A tokenizer with no row cannot
/// carry a version-6 job: the court could not prove a single token's bytes for it.
pub const PALW_TOKEN_TABLES_V1: &[PalwTokenTablePinV1] = &[PALW_TOKEN_TABLE_QWEN25_V1];

/// **The pinned `(vocab_len, root)` for a tokenizer commitment**, or `None` when this build pins no
/// table for it — including the zero commitment an artifact that declares no tokenizer carries.
///
/// A row whose hex does not parse matches nothing; `every_pinned_row_is_well_formed` holds every
/// row to 128 lowercase hex characters, so that case is a build that failed its own tests.
pub fn token_table_pin_for_v1(tokenizer_commitment: &Hash64) -> Option<(u32, Hash64)> {
    PALW_TOKEN_TABLES_V1.iter().find_map(|row| {
        let commitment = Hash64::from_str(row.tokenizer_commitment_hex).ok()?;
        if commitment != *tokenizer_commitment {
            return None;
        }
        Some((row.vocab_len, Hash64::from_str(row.root_hex).ok()?))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_step_leg::{PALW_STEP_LEG_ALL_DOMAINS, step_merkle_leaf_v1, step_merkle_node_v1};

    /// A synthetic table's renderings: id `i` renders as `i % 5` copies of `i as u8`, so every
    /// fifth id is empty and the rest differ in both length and content.
    fn synthetic_bytes(id: u32) -> Vec<u8> {
        vec![id as u8; (id % 5) as usize]
    }

    fn synthetic_leaves(n: u32) -> Vec<Hash64> {
        (0..n).map(|id| token_table_leaf_v1(id, &synthetic_bytes(id))).collect()
    }

    fn flip(h: &Hash64) -> Hash64 {
        let mut b = h.as_bytes();
        b[0] ^= 1;
        Hash64::from_bytes(b)
    }

    /// The leaf is exactly the preimage the ADR names — `id_le32 ‖ len_le32 ‖ bytes` under the
    /// domain key — so a second implementation can be written from the sentence.
    #[test]
    fn the_leaf_is_the_documented_preimage() {
        for (id, bytes) in [(0u32, &b""[..]), (7u32, &b"a"[..]), (151_935u32, &b"hello"[..]), (u32::MAX, &[0u8; 128][..])] {
            let mut state = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/token-table/v1").to_state();
            state.update(&id.to_le_bytes());
            state.update(&(bytes.len() as u32).to_le_bytes());
            state.update(bytes);
            assert_eq!(token_table_leaf_v1(id, bytes).as_byte_slice(), state.finalize().as_bytes(), "id {id}");
        }
        // The id and the length are both inside: the empty leaf is one value PER ID, and a byte
        // string cannot be re-read as a shorter one plus a different id.
        assert_ne!(token_table_leaf_v1(0, b""), token_table_leaf_v1(1, b""));
        assert_ne!(token_table_leaf_v1(1, b""), token_table_leaf_v1(1, b"\0"));
        assert_ne!(token_table_leaf_v1(1, b"ab"), token_table_leaf_v1(1, b"ba"));
    }

    /// **Every id of every table from 1 to 37 ids opens, and every forgery of it is refused** —
    /// exhaustive over the sizes where every promote shape occurs (odd widths at every level, a
    /// promoted tail two levels deep, the one-leaf table with no path at all).
    #[test]
    fn every_id_of_every_table_up_to_37_opens_and_every_forgery_is_refused() {
        for n in 1u32..=37 {
            let leaves = synthetic_leaves(n);
            let root = token_table_root_v1(&leaves);
            for id in 0..n {
                let opening = token_table_opening_from_leaves_v1(&leaves, id, synthetic_bytes(id)).expect("an id of the table opens");
                assert_eq!(opening.siblings.len(), token_table_path_len_v1(n, id), "n {n} id {id}: the builder's path is the tree's");
                assert_eq!(verify_token_table_opening_v1(&root, n, &opening), Ok(()), "n {n} id {id}");
                assert_eq!(token_table_proven_bytes_v1(&root, n, id, &opening), Ok(&synthetic_bytes(id)[..]));

                // A flipped byte — or, for an empty rendering, one byte more — is another leaf.
                let mut forged = opening.clone();
                if forged.bytes.is_empty() {
                    forged.bytes.push(0);
                } else {
                    forged.bytes[0] ^= 1;
                }
                assert_eq!(verify_token_table_opening_v1(&root, n, &forged), Err(PalwTokenTableError::RootMismatch), "n {n} id {id}");

                // The same bytes and path offered as another id: a different path length is
                // refused by count, the same length by the root (the leaf is index-bound twice).
                if n > 1 {
                    let other = (id + 1) % n;
                    let moved = PalwTokenTableOpeningV1 { id: other, ..opening.clone() };
                    let err = verify_token_table_opening_v1(&root, n, &moved).expect_err("a moved leaf is refused");
                    assert!(
                        matches!(err, PalwTokenTableError::RootMismatch | PalwTokenTableError::SiblingCount { .. }),
                        "n {n} id {id} as {other}: {err}"
                    );
                    assert_eq!(
                        token_table_proven_bytes_v1(&root, n, other, &opening),
                        Err(PalwTokenTableError::AnotherId { expected: other, got: id })
                    );
                }
                let past = PalwTokenTableOpeningV1 { id: n, ..opening.clone() };
                assert_eq!(
                    verify_token_table_opening_v1(&root, n, &past),
                    Err(PalwTokenTableError::IdOutOfRange { id: n, vocab_len: n })
                );

                // One sibling more, one fewer: refused by count before any hashing.
                let mut long = opening.clone();
                long.siblings.push(Hash64::default());
                assert!(matches!(verify_token_table_opening_v1(&root, n, &long), Err(PalwTokenTableError::SiblingCount { .. })));
                if !opening.siblings.is_empty() {
                    let mut short = opening.clone();
                    short.siblings.pop();
                    assert!(matches!(verify_token_table_opening_v1(&root, n, &short), Err(PalwTokenTableError::SiblingCount { .. })));
                    // Every sibling is load-bearing.
                    for k in 0..opening.siblings.len() {
                        let mut bent = opening.clone();
                        bent.siblings[k] = flip(&bent.siblings[k]);
                        assert_eq!(
                            verify_token_table_opening_v1(&root, n, &bent),
                            Err(PalwTokenTableError::RootMismatch),
                            "n {n} id {id} k {k}"
                        );
                    }
                }

                // Another table's root, and this table's root under another length.
                let mut other_leaves = leaves.clone();
                other_leaves[((id + 1) % n) as usize] = token_table_leaf_v1((id + 1) % n, b"not the table");
                if n > 1 {
                    assert_eq!(
                        verify_token_table_opening_v1(&token_table_root_v1(&other_leaves), n, &opening),
                        Err(PalwTokenTableError::RootMismatch),
                        "n {n} id {id}"
                    );
                }
                assert_eq!(verify_token_table_opening_v1(&flip(&root), n, &opening), Err(PalwTokenTableError::RootMismatch));
                assert!(verify_token_table_opening_v1(&root, n + 1, &opening).is_err(), "n {n} id {id}: the length is in the root");
                if id < n - 1 {
                    assert!(
                        verify_token_table_opening_v1(&root, n - 1, &opening).is_err(),
                        "n {n} id {id}: the length is in the root"
                    );
                }
            }
        }
    }

    /// The path length the verifier demands is the tree's own: what the builder emits, never more
    /// than the height, the full height at id 0, and shorter only on the promoted right edge.
    #[test]
    fn the_sibling_count_is_the_trees_own_path_for_that_id() {
        for n in 1u32..=70 {
            let leaves = synthetic_leaves(n);
            let height = step_leg_max_opening_siblings_v1(u64::from(n));
            assert_eq!(token_table_path_len_v1(n, 0), height, "n {n}: id 0 is never promoted");
            for id in 0..n {
                let path = step_merkle_path_capped_v1(&leaves, id as usize, u64::from(n)).expect("in range");
                assert_eq!(token_table_path_len_v1(n, id), path.len(), "n {n} id {id}");
                assert!(path.len() <= height);
            }
        }
        // At the pinned width: 18 levels, and the right edge shorter exactly where the module
        // header says it is — every run boundary, on both sides.
        let n = PALW_TOKEN_TABLE_QWEN25_V1.vocab_len;
        assert_eq!(step_leg_max_opening_siblings_v1(u64::from(n)), 18);
        for (first, last, siblings) in [
            (0u32, 131_071u32, 18usize),
            (131_072, 147_455, 16),
            (147_456, 151_551, 15),
            (151_552, 151_807, 12),
            (151_808, 151_935, 11),
        ] {
            assert_eq!(token_table_path_len_v1(n, first), siblings, "id {first}");
            assert_eq!(token_table_path_len_v1(n, last), siblings, "id {last}");
        }
        assert_eq!(token_table_path_len_v1(n, 56_940), 18, "the longest token rides a full path");
        assert_eq!(token_table_path_len_v1(n, 151_643), 12, "and the first added token a promoted one");
    }

    /// **The walk is `palw_step_leg`'s**, restated here once by hand over three leaves so the
    /// sentence "the tree is the step tree" is checked rather than asserted.
    #[test]
    fn the_root_is_the_step_trees_root_wrapped_with_the_length() {
        let leaves = synthetic_leaves(3);
        let l: Vec<Hash64> = leaves.iter().enumerate().map(|(i, h)| step_merkle_leaf_v1(i as u64, h)).collect();
        // Three leaves: the first two pair, the third is promoted, then the two pair.
        let tree = step_merkle_node_v1(&step_merkle_node_v1(&l[0], &l[1]), &l[2]);
        let mut state = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/token-table/root/v1").to_state();
        state.update(&3u64.to_le_bytes());
        state.update(tree.as_byte_slice());
        assert_eq!(token_table_root_v1(&leaves).as_byte_slice(), state.finalize().as_bytes());
    }

    /// The length is inside the root: a table and the same table with one more (empty) id are two
    /// commitments, and the empty table has one that opens nothing.
    #[test]
    fn the_root_binds_the_table_length_and_the_empty_table_opens_nothing() {
        let leaves = synthetic_leaves(10);
        let mut padded = leaves.clone();
        padded.push(token_table_leaf_v1(10, b""));
        assert_ne!(token_table_root_v1(&leaves), token_table_root_v1(&padded), "a padded id is part of the table");

        let empty = token_table_root_v1(&[]);
        assert_ne!(empty, Hash64::default());
        assert_ne!(empty, token_table_root_v1(&[token_table_leaf_v1(0, b"")]));
        let nothing = PalwTokenTableOpeningV1 { id: 0, bytes: Vec::new(), siblings: Vec::new() };
        assert_eq!(verify_token_table_opening_v1(&empty, 0, &nothing), Err(PalwTokenTableError::IdOutOfRange { id: 0, vocab_len: 0 }));
        assert_eq!(
            token_table_opening_from_leaves_v1(&[], 0, Vec::new()),
            Err(PalwTokenTableError::IdOutOfRange { id: 0, vocab_len: 0 })
        );
    }

    /// **The bound's boundary, pinned on both sides.** A rendering of exactly the bound opens; one
    /// byte more is refused by the builder and by the verifier — even under a root that was built
    /// over it, because the refusal is of the carrier's size, ahead of any hashing.
    #[test]
    fn the_bytes_bound_admits_the_longest_rendering_and_refuses_one_past_it() {
        assert_eq!(PALW_TOKEN_TABLE_MAX_TOKEN_BYTES_V1, 256);
        let at_bound = vec![b' '; PALW_TOKEN_TABLE_MAX_TOKEN_BYTES_V1];
        let past_bound = vec![b' '; PALW_TOKEN_TABLE_MAX_TOKEN_BYTES_V1 + 1];
        let leaves = vec![token_table_leaf_v1(0, &at_bound), token_table_leaf_v1(1, &past_bound), token_table_leaf_v1(2, b"x")];
        let root = token_table_root_v1(&leaves);

        let opening = token_table_opening_from_leaves_v1(&leaves, 0, at_bound).expect("the bound itself opens");
        assert_eq!(verify_token_table_opening_v1(&root, 3, &opening), Ok(()));

        let too_long = PalwTokenTableError::TokenBytesTooLong { got: 257, max: 256 };
        assert_eq!(token_table_opening_from_leaves_v1(&leaves, 1, past_bound.clone()), Err(too_long.clone()));
        let siblings = step_merkle_path_capped_v1(&leaves, 1, 3).expect("in range");
        let hand_built = PalwTokenTableOpeningV1 { id: 1, bytes: past_bound, siblings };
        assert_eq!(verify_token_table_opening_v1(&root, 3, &hand_built), Err(too_long), "refused by size, not by root");

        // And a builder handed bytes that are not the leaf refuses rather than emit a dud.
        assert_eq!(
            token_table_opening_from_leaves_v1(&leaves, 2, b"y".to_vec()),
            Err(PalwTokenTableError::BytesAreNotTheLeaf { id: 2 })
        );
    }

    /// The cost gate charges what the carrier relays: the borsh length, byte for byte, and never
    /// more than the pre-opening bound.
    #[test]
    fn the_opening_bytes_are_the_borsh_length_and_the_bound_holds() {
        for n in [1u32, 2, 3, 17, 64, 65, 300] {
            let leaves = synthetic_leaves(n);
            let root = token_table_root_v1(&leaves);
            for id in 0..n {
                let opening = token_table_opening_from_leaves_v1(&leaves, id, synthetic_bytes(id)).expect("opens");
                let wire = borsh::to_vec(&opening).expect("serializes");
                assert_eq!(token_table_opening_bytes_v1(&opening), wire.len() as u64, "n {n} id {id}");
                assert!(token_table_opening_bytes_v1(&opening) <= token_table_max_opening_bytes_v1(n));
                let back: PalwTokenTableOpeningV1 = borsh::from_slice(&wire).expect("round trips");
                assert_eq!(verify_token_table_opening_v1(&root, n, &back), Ok(()));
            }
        }
        assert_eq!(token_table_max_opening_bytes_v1(PALW_TOKEN_TABLE_QWEN25_V1.vocab_len), 12 + 256 + 18 * 64);
        assert_eq!(token_table_max_opening_bytes_v1(PALW_TOKEN_TABLE_QWEN25_V1.vocab_len), 1_420);
    }

    /// **The pinned row is the Qwen2.5 tokenizer, at the vocabulary the A16 dense class registers.**
    /// The root itself is checked against the FILE by the base0 test the row's doc names; this one
    /// runs everywhere, file or not, and holds the row to the class it serves.
    #[test]
    fn the_pin_is_one_row_the_qwen25_tokenizer_at_the_a16_class_vocabulary() {
        use crate::palw_qwen25_profile::{QWEN25_1_5B, QWEN25_1_5B_A16, qwen25_a16_profile_v2};
        assert_eq!(PALW_TOKEN_TABLES_V1, &[PALW_TOKEN_TABLE_QWEN25_V1]);
        let row = PALW_TOKEN_TABLE_QWEN25_V1;
        assert_eq!(
            row.tokenizer_commitment_hex,
            "fa9a43521e324f8482d88a2f4147ae2321202db8806b7c039322fc8a3d265ab482c35e0ab1bd2819b86d50449b0270db435823b002aef001c07a8c7c10a649bb",
            "the commitment `palw-class bind-tokenizer` bound into the testnet-11 dense artifact"
        );
        assert_eq!(row.vocab_len, 151_936);
        assert_eq!(row.vocab_len, QWEN25_1_5B.vocab_size, "the family's vocabulary");
        let profile = qwen25_a16_profile_v2(QWEN25_1_5B_A16).expect("the registered A16 geometry projects");
        assert_eq!(row.vocab_len, profile.vocab_size, "the width of the class's logit row, which is the lane space a court opens");

        let commitment = Hash64::from_str(row.tokenizer_commitment_hex).expect("hex");
        let root = Hash64::from_str(row.root_hex).expect("hex");
        assert_eq!(token_table_pin_for_v1(&commitment), Some((151_936, root)));
        assert_eq!(token_table_pin_for_v1(&Hash64::default()), None, "an artifact that declares no tokenizer has no table");
        assert_eq!(token_table_pin_for_v1(&flip(&commitment)), None);
    }

    #[test]
    fn every_pinned_row_is_well_formed() {
        for row in PALW_TOKEN_TABLES_V1 {
            for hex in [row.tokenizer_commitment_hex, row.root_hex] {
                assert_eq!(hex.len(), 128, "{hex}");
                assert!(hex.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)), "lowercase hex: {hex}");
                let parsed = Hash64::from_str(hex).expect("parses");
                assert_ne!(parsed, Hash64::default(), "a zero value names no tokenizer and no table");
                assert_eq!(parsed.to_string(), hex, "canonical spelling");
            }
            assert!(row.vocab_len > 0);
            assert!(!row.source.is_empty());
        }
        // One row per tokenizer: two rows for one commitment would make the lookup order a rule.
        for (i, a) in PALW_TOKEN_TABLES_V1.iter().enumerate() {
            for b in &PALW_TOKEN_TABLES_V1[i + 1..] {
                assert_ne!(a.tokenizer_commitment_hex, b.tokenizer_commitment_hex);
            }
        }
    }

    /// **A synthetic table's root, pinned** — the construction's golden. The Qwen root is checked
    /// against its file only where the file is; this one is checked on every run, so a change to
    /// the leaf, the tree or the wrapper cannot pass CI quietly. The value was computed by a second,
    /// independent implementation written from this module's header alone (Python's `hashlib`
    /// BLAKE2b, 2026-09-10), which also reproduced the pinned Qwen2.5 root from the file.
    #[test]
    fn the_construction_is_golden_pinned() {
        let root = token_table_root_v1(&synthetic_leaves(37));
        assert_eq!(
            root.to_string(),
            "87e8cc47329c225142d7582c1d472b783fcf0d47cfecd734cc7bbfcac735e82f5b739acaaaaf8d6818a6cb9c5cb978f6c89424c447ecda616a25aa1db43bbd85"
        );
    }

    #[test]
    fn the_domains_are_distinct_and_usable_as_keys() {
        let ours = [PALW_TOKEN_TABLE_DOMAIN_V1, PALW_TOKEN_TABLE_ROOT_DOMAIN_V1];
        assert_ne!(ours[0], ours[1]);
        for d in ours {
            assert!(!d.is_empty() && d.len() <= 64, "a BLAKE2b key is at most 64 bytes");
            assert!(!PALW_STEP_LEG_ALL_DOMAINS.contains(&d), "the tree's own domains are the step leg's; the leaf and root are ours");
            assert_ne!(d, crate::palw_decode_constraint_v1::PALW_CONSTRAINT_DOMAIN_V1);
        }
    }

    /// **The empty leaf is harmless only because no constraint admits an empty rendering** — the
    /// rule lives in the constraint module, and the claim that relies on it lives here, so it is
    /// checked here: even the automaton that admits EVERY byte admits no token that has none.
    #[test]
    fn no_constraint_admits_the_empty_leaf() {
        use crate::palw_decode_constraint_v1::{
            PALW_DECODE_CONSTRAINT_VERSION_V1, PalwConstraintActionV1, PalwConstraintEdgeV1, PalwConstraintFrameV1,
            PalwConstraintNodeV1, PalwDecodeConstraintV1, constraint_admits_lane_v1, constraint_start_state_v1,
        };
        let anything = PalwDecodeConstraintV1 {
            version: PALW_DECODE_CONSTRAINT_VERSION_V1,
            compiler_id: Hash64::default(),
            start_frame: 0,
            frames: vec![PalwConstraintFrameV1 {
                start: 0,
                nodes: vec![PalwConstraintNodeV1 {
                    accepting: true,
                    edges: vec![PalwConstraintEdgeV1 { lo: 0, hi: 255, action: PalwConstraintActionV1::Goto(0) }],
                }],
            }],
        };
        anything.validate().expect("a well-formed automaton");
        let start = constraint_start_state_v1(&anything);
        assert!(constraint_admits_lane_v1(&anything, &start, Some(&b"a"[..])).is_some(), "it admits every non-empty rendering");
        assert!(constraint_admits_lane_v1(&anything, &start, Some(&[0u8; 128][..])).is_some());
        assert!(constraint_admits_lane_v1(&anything, &start, Some(&b""[..])).is_none(), "and never the table's empty leaf");
        assert!(constraint_admits_lane_v1(&anything, &start, None).is_none());
    }
}
