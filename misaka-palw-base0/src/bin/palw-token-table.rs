//! **`palw-token-table` — the pin of a class's token table, printed from its tokenizer file
//! (ADR-0096 §10 B4).**
//!
//! ```text
//! palw-token-table --tokenizer <tokenizer.json> --vocab-len <N>
//! ```
//!
//! Prints one JSON object: `tokenizer_commitment` (the value an artifact's `tokenizer_commitment`
//! holds for this file — `Base0ArtifactV1::tokenizer_commitment_of`, the one `palw-class
//! bind-tokenizer` writes), `vocab_len`, `root` (the table's root, what `PALW_TOKEN_TABLES_V1` pins),
//! `tokenizer_len` (the ids the file itself names), and the facts the pin rests on —
//! `max_token_bytes` / `longest_token_id`, `empty_ids`, `byte_complete`.
//!
//! `--vocab-len` is the CLASS's vocabulary: the registered profile's `vocab_size`, the width of its
//! logit row — 151936 for Qwen2.5-1.5B, which is wider than the 151,665 ids its tokenizer names.
//!
//! **It refuses to print a pin nothing could use**, by name, with a non-zero exit: a table whose
//! longest rendering is past `PALW_TOKEN_TABLE_MAX_TOKEN_BYTES_V1` (the court would refuse that
//! token's honest opening, so a lie about it could never be tried), and a table that is not
//! byte-complete (§10 B7's court reads "no lane is admitted" as "no byte continues", which is the
//! same question only when every byte is some id's whole rendering).

use kaspa_consensus_core::palw_token_table_v1::PALW_TOKEN_TABLE_MAX_TOKEN_BYTES_V1;
use misaka_palw_base0::artifact::Base0ArtifactV1;
use misaka_palw_base0::token_table::{token_table_facts_v1, token_table_root_for_v1};
use misaka_palw_base0::tokenizer::QwenTokenizer;

fn die(msg: String) -> ! {
    eprintln!("palw-token-table: {msg}");
    std::process::exit(1)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let path = flag("--tokenizer").unwrap_or_else(|| die("--tokenizer <tokenizer.json> is required".into()));
    let vocab_len = flag("--vocab-len").unwrap_or_else(|| {
        die("--vocab-len <N> is required: the class's vocabulary (its profile's vocab_size), not the tokenizer's length".into())
    });
    let vocab_len: u32 = match vocab_len.parse() {
        Ok(n) if n > 0 => n,
        _ => die(format!("--vocab-len {vocab_len:?} is not a positive integer")),
    };

    let bytes = std::fs::read(&path).unwrap_or_else(|e| die(format!("{path}: {e}")));
    let commitment = Base0ArtifactV1::tokenizer_commitment_of(&bytes);
    let tokenizer = QwenTokenizer::from_json(&bytes).unwrap_or_else(|e| die(format!("{path}: {e}")));

    let facts = token_table_facts_v1(&tokenizer, vocab_len);
    if facts.max_token_bytes > PALW_TOKEN_TABLE_MAX_TOKEN_BYTES_V1 {
        die(format!(
            "id {} renders {} bytes, past the {PALW_TOKEN_TABLE_MAX_TOKEN_BYTES_V1}-byte bound a court opening may state. \
             This table cannot be pinned: that token's honest opening would be refused, and a lie about it never tried.",
            facts.longest_token_id, facts.max_token_bytes
        ));
    }
    if !facts.byte_complete {
        die(format!(
            "{path} at {vocab_len} ids is not byte-complete: some byte is no id's whole rendering, so the court's finish \
             rule (no byte continues = no lane is admitted) would not be the selection rule's. This table cannot be pinned."
        ));
    }

    let root = token_table_root_for_v1(&tokenizer, vocab_len);
    let out = serde_json::json!({
        "tokenizer_commitment": commitment.to_string(),
        "vocab_len": vocab_len,
        "root": root.to_string(),
        "tokenizer_len": tokenizer.len(),
        "max_token_bytes": facts.max_token_bytes,
        "longest_token_id": facts.longest_token_id,
        "empty_ids": facts.empty_ids,
        "byte_complete": facts.byte_complete,
    });
    println!("{}", serde_json::to_string_pretty(&out).expect("a JSON value serializes"));
}
