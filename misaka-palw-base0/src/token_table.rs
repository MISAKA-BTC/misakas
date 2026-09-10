//! **ADR-0096 §10 B4, the producing side: a class's token table, built from its tokenizer.**
//!
//! consensus-core pins each table's root and verifies openings against it
//! (`kaspa_consensus_core::palw_token_table_v1`); it holds no tokenizer and never will. This is the
//! other half — the only place a tokenizer becomes table leaves — and it is used three ways:
//! `palw-token-table` prints a pin from a tokenizer file, the pin's test recomputes it from the
//! file, and a court close builder opens one id's bytes against it.
//!
//! **What an id's bytes are is decided once, and not here**: `QwenTokenizer::constrained_rendering_v1`
//! — an ordinary token's byte-level rendering; nothing for an added (control) token; nothing for an
//! id the tokenizer cannot render — is the one spelling the table, the engine's mask, the worker's
//! stream and the rendered-segments hash all read, and [`token_table_bytes_v1`] is the table's name
//! for it, not a copy. A copy is how two sides come to disagree about an honest answer: §10 B6
//! convicts a segment that is not the table's bytes, so a worker that rendered `<|endoftext|>` as
//! its thirteen characters where the table says "nothing" would be convicted for an honest run.
//! Because the table READS the rule, the pin's test guards the rule too — a change to it moves the
//! root the file builds, and the pinned constant then fails by name.

use kaspa_consensus_core::palw_token_table_v1::{
    PalwTokenTableError, PalwTokenTableOpeningV1, token_table_leaf_v1, token_table_opening_from_leaves_v1, token_table_root_v1,
};
use kaspa_hashes::Hash64;

use crate::tokenizer::QwenTokenizer;

/// **One id's bytes, as the table defines them.**
///
/// * an added token (`QwenTokenizer::is_added_id`) — every `<|…|>` control marker and every other
///   entry of `added_tokens` — has NONE: a control token is never answer content, and its literal
///   text is exactly what a string-typed schema would otherwise admit;
/// * an ordinary token has its byte-level rendering (`QwenTokenizer::token_bytes`);
/// * an id the tokenizer cannot render — past its table (a padded vocabulary id), or holding a
///   character outside the byte alphabet — has none.
///
/// "None" is the empty string, and no constraint admits an empty rendering, which is what makes
/// the empty leaf harmless (the consensus module's header gives the argument).
///
/// This IS `QwenTokenizer::constrained_rendering_v1`, called rather than restated.
pub fn token_table_bytes_v1(tokenizer: &QwenTokenizer, id: u32) -> Vec<u8> {
    tokenizer.constrained_rendering_v1(id)
}

/// **The table's leaves: one per id in `0..vocab_len`, in id order.** `vocab_len` is the CLASS's
/// vocabulary (its logit row), not the tokenizer's length — the ids past the tokenizer's table are
/// part of the lane space a court opens, and they get the empty leaf.
pub fn token_table_leaves_v1(tokenizer: &QwenTokenizer, vocab_len: u32) -> Vec<Hash64> {
    (0..vocab_len).map(|id| token_table_leaf_v1(id, &token_table_bytes_v1(tokenizer, id))).collect()
}

/// The table's root — what `PALW_TOKEN_TABLES_V1` pins for this tokenizer at this width.
pub fn token_table_root_for_v1(tokenizer: &QwenTokenizer, vocab_len: u32) -> Hash64 {
    token_table_root_v1(&token_table_leaves_v1(tokenizer, vocab_len))
}

/// **The opening of `id`'s bytes against the table's root** — what a court close carries (§10 B5's
/// beating lane, B6's committed token). Builds the whole table; a caller opening many ids builds
/// [`token_table_leaves_v1`] once and calls `token_table_opening_from_leaves_v1` per id.
///
/// Refuses by name rather than inventing an opening: an id past the table has none.
pub fn token_table_opening_v1(
    tokenizer: &QwenTokenizer,
    vocab_len: u32,
    id: u32,
) -> Result<PalwTokenTableOpeningV1, PalwTokenTableError> {
    let leaves = token_table_leaves_v1(tokenizer, vocab_len);
    token_table_opening_from_leaves_v1(&leaves, id, token_table_bytes_v1(tokenizer, id))
}

/// **The facts a pin rests on**, measured from the table rather than remembered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwTokenTableFactsV1 {
    /// The longest rendering in the table, and the lowest id that has it. Must be within
    /// `PALW_TOKEN_TABLE_MAX_TOKEN_BYTES_V1`, or an honest opening of that id is refused.
    pub max_token_bytes: usize,
    pub longest_token_id: u32,
    /// Ids with the empty leaf: the added tokens and the ids the tokenizer cannot render.
    pub empty_ids: u32,
    /// Every one of the 256 bytes is some id's WHOLE rendering. §10 B7's court decides "no lane is
    /// admitted" as "no byte continues", which is only the same question over a byte-complete
    /// table.
    pub byte_complete: bool,
}

/// Measure [`PalwTokenTableFactsV1`] over `0..vocab_len`.
pub fn token_table_facts_v1(tokenizer: &QwenTokenizer, vocab_len: u32) -> PalwTokenTableFactsV1 {
    let (mut max_token_bytes, mut longest_token_id, mut empty_ids) = (0usize, 0u32, 0u32);
    let mut single = [false; 256];
    for id in 0..vocab_len {
        let bytes = token_table_bytes_v1(tokenizer, id);
        if bytes.len() > max_token_bytes {
            max_token_bytes = bytes.len();
            longest_token_id = id;
        }
        match bytes.as_slice() {
            [] => empty_ids += 1,
            [b] => single[*b as usize] = true,
            _ => {}
        }
    }
    PalwTokenTableFactsV1 { max_token_bytes, longest_token_id, empty_ids, byte_complete: single.iter().all(|s| *s) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::{FIXTURE_VOCAB, byte_level_fixture_v1};
    use kaspa_consensus_core::palw_token_table_v1::{
        PALW_TOKEN_TABLE_MAX_TOKEN_BYTES_V1, PALW_TOKEN_TABLE_QWEN25_V1, token_table_pin_for_v1, token_table_proven_bytes_v1,
        verify_token_table_opening_v1,
    };

    /// The fixture's class is wider than its tokenizer, as a real class is: ten padded ids.
    const FIXTURE_TABLE: u32 = FIXTURE_VOCAB + 10;

    /// **An opening built here verifies under consensus-core's verifier, for every id** — the byte
    /// pieces with their one byte, the control and user-defined tokens with none, the padded ids
    /// with none — through both builders.
    #[test]
    fn every_fixture_token_table_opening_verifies_under_the_consensus_verifier() {
        let (tokenizer, im_start, im_end) = byte_level_fixture_v1();
        let leaves = token_table_leaves_v1(&tokenizer, FIXTURE_TABLE);
        let root = token_table_root_for_v1(&tokenizer, FIXTURE_TABLE);
        assert_eq!(root, token_table_root_v1(&leaves));
        for id in 0..FIXTURE_TABLE {
            let expected: Vec<u8> = match id {
                0..=255 => tokenizer.token_bytes(id).expect("a byte piece renders"),
                _ => Vec::new(),
            };
            assert_eq!(token_table_bytes_v1(&tokenizer, id), expected, "id {id}");
            let opening = token_table_opening_from_leaves_v1(&leaves, id, expected.clone()).expect("opens");
            assert_eq!(verify_token_table_opening_v1(&root, FIXTURE_TABLE, &opening), Ok(()), "id {id}");
            assert_eq!(token_table_proven_bytes_v1(&root, FIXTURE_TABLE, id, &opening), Ok(&expected[..]));
        }
        // The one-call builder is the same opening.
        for id in [0, 97, 255, im_start, im_end, FIXTURE_VOCAB - 1, FIXTURE_TABLE - 1] {
            let opening = token_table_opening_v1(&tokenizer, FIXTURE_TABLE, id).expect("opens");
            assert_eq!(opening, token_table_opening_from_leaves_v1(&leaves, id, token_table_bytes_v1(&tokenizer, id)).unwrap());
            assert_eq!(verify_token_table_opening_v1(&root, FIXTURE_TABLE, &opening), Ok(()), "id {id}");
        }
        assert_eq!(
            token_table_opening_v1(&tokenizer, FIXTURE_TABLE, FIXTURE_TABLE),
            Err(PalwTokenTableError::IdOutOfRange { id: FIXTURE_TABLE, vocab_len: FIXTURE_TABLE }),
            "an id past the class has no opening"
        );
    }

    /// **A control token is not its spelling in the table**, though the tokenizer spells it: the
    /// literal `<|im_start|>` is twelve bytes a string schema would admit, and the table refuses to
    /// be the thing that lets it. An opening that claims the spelling does not verify.
    #[test]
    fn a_control_token_has_the_empty_leaf_and_its_spelling_does_not_open() {
        let (tokenizer, im_start, _) = byte_level_fixture_v1();
        assert_eq!(tokenizer.token_bytes(im_start), Some(b"<|im_start|>".to_vec()), "the tokenizer spells it");
        assert!(token_table_bytes_v1(&tokenizer, im_start).is_empty(), "the table does not");
        let leaves = token_table_leaves_v1(&tokenizer, FIXTURE_TABLE);
        let root = token_table_root_v1(&leaves);
        assert_eq!(
            token_table_opening_from_leaves_v1(&leaves, im_start, b"<|im_start|>".to_vec()),
            Err(PalwTokenTableError::BytesAreNotTheLeaf { id: im_start })
        );
        let mut forged = token_table_opening_from_leaves_v1(&leaves, im_start, Vec::new()).expect("opens");
        forged.bytes = b"<|im_start|>".to_vec();
        assert_eq!(verify_token_table_opening_v1(&root, FIXTURE_TABLE, &forged), Err(PalwTokenTableError::RootMismatch));
    }

    /// The width is the class's: the same tokenizer at another `vocab_len` is another table, and
    /// the facts count the empty ids the class's padding adds.
    #[test]
    fn the_token_table_width_is_the_classes_and_the_facts_are_measured() {
        let (tokenizer, _, _) = byte_level_fixture_v1();
        assert_ne!(token_table_root_for_v1(&tokenizer, FIXTURE_VOCAB), token_table_root_for_v1(&tokenizer, FIXTURE_TABLE));
        assert_ne!(token_table_root_for_v1(&tokenizer, 256), token_table_root_for_v1(&tokenizer, FIXTURE_VOCAB));

        let facts = token_table_facts_v1(&tokenizer, FIXTURE_TABLE);
        assert_eq!(facts.max_token_bytes, 1, "every byte piece is one byte and every added token is empty");
        assert_eq!(facts.longest_token_id, 0);
        assert_eq!(facts.empty_ids, (FIXTURE_VOCAB - 256) + (FIXTURE_TABLE - FIXTURE_VOCAB), "four added, ten padded");
        assert!(facts.byte_complete);
        assert!(!token_table_facts_v1(&tokenizer, 255).byte_complete, "a table missing one byte piece is not byte-complete");
    }

    // -----------------------------------------------------------------------------------------
    // The pin, against the file
    // -----------------------------------------------------------------------------------------

    /// Where the dense lane's `tokenizer.json` is looked for when `MISAKA_FLOOR_TOKENIZER_DENSE` is
    /// unset — `misaka-palw-derive/tests/corpus_width.rs`'s list and rule, plus the runtime's own
    /// copy of the same file.
    const DENSE_TOKENIZER_FIXTURES: &[&str] = &[
        "/Users/wata/Downloads/qwen25-tokenizer.json",
        "/Users/wata/Downloads/misaka-palw-runtime/models/qwen2.5-1.5b/tokenizer.json",
    ];

    /// The declaration a host with no checkpoint makes (CI sets it for the whole workspace). It
    /// turns the refusal below into a named SKIPPED line; without it, absence FAILS.
    const NO_CHECKPOINT: &str = "MISAKA_PALW_WIDTH_NO_TOKENIZER";

    /// `corpus_width.rs`'s `dense_tokenizer_path`, for the pin: `Ok(path)` when the file is found;
    /// `Err(line)` only when it is not AND the host declared it cannot check; a panic otherwise.
    fn dense_tokenizer_path(test: &str) -> Result<std::path::PathBuf, String> {
        if let Ok(p) = std::env::var("MISAKA_FLOOR_TOKENIZER_DENSE") {
            let p = std::path::PathBuf::from(p);
            assert!(p.exists(), "MISAKA_FLOOR_TOKENIZER_DENSE={} does not exist", p.display());
            return Ok(p);
        }
        if let Some(p) = DENSE_TOKENIZER_FIXTURES.iter().map(std::path::PathBuf::from).find(|p| p.exists()) {
            return Ok(p);
        }
        let looked = DENSE_TOKENIZER_FIXTURES.join(", ");
        if std::env::var_os(NO_CHECKPOINT).is_some() {
            return Err(format!(
                "SKIPPED {test}: no dense tokenizer on this host and {NO_CHECKPOINT} is set. \
                 MISAKA_FLOOR_TOKENIZER_DENSE is unset and none of [{looked}] exists. \
                 THE PIN WAS NOT CHECKED BY THIS RUN."
            ));
        }
        panic!(
            "{test} has no dense tokenizer: MISAKA_FLOOR_TOKENIZER_DENSE is unset and none of [{looked}] exists. \
             PALW_TOKEN_TABLE_QWEN25_V1 is a consensus constant the court proves token bytes against, and a run \
             that checks nothing must not read as one that checked. Set MISAKA_FLOOR_TOKENIZER_DENSE to the dense \
             checkpoint's tokenizer.json, or set {NO_CHECKPOINT} to declare that this host cannot check it."
        );
    }

    /// **The pinned Qwen2.5 table is the one the tokenizer file builds** — ADR-0082 Decision 11's
    /// shape: `palw-token-table` generated the row, and this recomputes it from the file.
    ///
    /// Checks, in order: the file's commitment is the row's; the root the file builds at the row's
    /// width is the row's root, and `token_table_pin_for_v1` returns it; the facts the module docs
    /// state (longest rendering 128 bytes at id 56,940, 121 ids past 64 bytes, byte-complete with
    /// ids 0–255 the 256 single bytes, the empty leaves exactly the 22 added tokens and the 271
    /// padded ids); and openings of the ids that stress the tree — the full-height path, the longest
    /// token, an added token, the last padded id — verify against the PINNED root.
    #[test]
    fn the_pinned_qwen25_token_table_is_the_one_the_tokenizer_file_builds() {
        let test = "the_pinned_qwen25_token_table_is_the_one_the_tokenizer_file_builds";
        let path = match dense_tokenizer_path(test) {
            Ok(p) => p,
            Err(skipped) => return println!("{skipped}"),
        };
        let row = PALW_TOKEN_TABLE_QWEN25_V1;
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let commitment = crate::artifact::Base0ArtifactV1::tokenizer_commitment_of(&bytes);
        assert_eq!(
            commitment.to_string(),
            row.tokenizer_commitment_hex,
            "{} is not the tokenizer the row pins — a different file at this path, not a moved pin",
            path.display()
        );

        let tokenizer = QwenTokenizer::from_json(&bytes).expect("the pinned tokenizer parses");
        let leaves = token_table_leaves_v1(&tokenizer, row.vocab_len);
        let root = token_table_root_v1(&leaves);
        assert_eq!(
            root.to_string(),
            row.root_hex,
            "the table {} builds at {} ids is not the pinned root — re-run palw-token-table and re-pin only if the \
             table's definition changed on purpose",
            path.display(),
            row.vocab_len
        );
        assert_eq!(token_table_pin_for_v1(&commitment), Some((row.vocab_len, root)));

        // The facts the constant's doc states, measured.
        assert_eq!(tokenizer.len(), 151_665, "151,643 BPE tokens and 22 added tokens");
        let facts = token_table_facts_v1(&tokenizer, row.vocab_len);
        assert_eq!((facts.max_token_bytes, facts.longest_token_id), (128, 56_940));
        assert!(facts.max_token_bytes <= PALW_TOKEN_TABLE_MAX_TOKEN_BYTES_V1);
        assert!(facts.byte_complete);
        let singles: std::collections::BTreeSet<u8> = (0..256u32)
            .map(|id| match token_table_bytes_v1(&tokenizer, id).as_slice() {
                [b] => *b,
                other => panic!("id {id} renders {} bytes, not one", other.len()),
            })
            .collect();
        assert_eq!(singles.len(), 256, "ids 0-255 are the 256 single bytes");
        let added = tokenizer.added_tokens().len() as u32;
        assert_eq!(added, 22);
        assert_eq!(facts.empty_ids, added + (row.vocab_len - tokenizer.len() as u32), "no ordinary token renders empty");
        assert_eq!((0..row.vocab_len).filter(|id| token_table_bytes_v1(&tokenizer, *id).len() > 64).count(), 121);

        let pinned: Hash64 = row.root_hex.parse().expect("the pinned root is hex");
        for id in [0, 56_940, 131_071, 131_072, 151_643, 151_664, 151_665, row.vocab_len - 1] {
            let opening = token_table_opening_from_leaves_v1(&leaves, id, token_table_bytes_v1(&tokenizer, id)).expect("opens");
            assert_eq!(verify_token_table_opening_v1(&pinned, row.vocab_len, &opening), Ok(()), "id {id}");
        }
        println!(
            "token table: {} checked against the pin — {} ids, root {}…, {} empty leaves, longest rendering {} bytes (id {})",
            path.display(),
            row.vocab_len,
            &row.root_hex[..16],
            facts.empty_ids,
            facts.max_token_bytes,
            facts.longest_token_id
        );
    }
}
