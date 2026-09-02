//! **ADR-0079 S7 / SA-6 (unit R-05) — the special-token corpus.**
//!
//! S7: *"Untrusted prompt text never tokenizes to a control token (`parse_special = false`),
//! pinned by the existing template test, with a corpus that includes every special-token
//! literal."* SA-6 says the same rule reaches the gateway: *"the security property is the same in
//! both forms — untrusted text never yields a control id — and S7's corpus pins it."*
//!
//! The corpus is `tests/corpus/special-tokens.txt`: every special-token literal of the two
//! shipped tokenizer families, crossed with every shape a stranger can put one in (bare, doubled,
//! tripled, in the middle of a word, inside JSON, inside the gateway's own rendered template,
//! around a zero-width space, in Japanese, next to an emoji), plus the unicode look-alikes.
//!
//! # Why the mechanism arm exists
//!
//! An empty added-token table passes S7 vacuously, and one of the three shipped classes has
//! exactly that: PALW-BASE-0 is a derived fixture whose `tokenizer_commitment` is
//! `Hash64::default()` and whose 1,024 ids have no names. So a test that only asserted "no
//! control id came out" would be green on a tokenizer that cannot produce one at all. Every arm
//! below therefore proves, on the same tokenizer, that the literal IS a live control id when it
//! is presented as one — [`special_literals_are_live_control_ids`].
//!
//! # What is green here, and what is not
//!
//! * **Green, unconditionally:** the mechanism arm, the corpus's own coverage checks, and
//!   [`unicode_lookalikes_never_become_a_control_id`] — the look-alike half of S7 holds today,
//!   on the fixture and on a real Qwen table.
//! * **Not green:** the literal half. `QwenTokenizer` has ONE encoder, [`QwenTokenizer::encode`],
//!   and it matches added tokens on the raw text before anything else (`leftmost_added`), so a
//!   user string containing `<|im_start|>` becomes the control id. There is no
//!   `parse_special = false` entry point anywhere in the tree — the comments that claim one
//!   (`misaka-palw-gateway/src/main.rs` above `TEMPLATE_ID_V1`,
//!   `misaka-palw-base0/src/bin/palw-a16-fp-worker.rs` above the `Text` arm) describe the
//!   *template's plain-text markers*, not the model's control tokens, and the gateway hands the
//!   worker `PalwFpWorkerInputV3::Text(rendered_prompt)` with the user's bytes verbatim.
//!   [`s7_untrusted_text_never_yields_a_control_id`] is that pin, ARMED: the worker's Text arm
//!   goes through `QwenTokenizer::encode_without_specials` (stream W landed it), and this test
//!   encodes user text the same way;
//!   [`the_user_text_encoder_is_still_missing`] is the tripwire that goes red the day it lands.
//!
//! `tokenizer.rs` is owned by the worker workstream; this file changes nothing in it.

use misaka_palw_base0::tokenizer::QwenTokenizer;

// ---------------------------------------------------------------------------------------------
// The corpus file
// ---------------------------------------------------------------------------------------------

const CORPUS: &str = include_str!("corpus/special-tokens.txt");

/// The env var that points at a real family `tokenizer.json` (e.g. a Qwen2.5 or Qwen3.x
/// checkpoint's). Absent ⇒ the real-table arms print why they are skipping and return.
const QWEN_TOKENIZER_ENV: &str = "MISAKA_QWEN_TOKENIZER_JSON";

/// One `[literals]` row: the exact string, and whether the shipped tables mark it `special`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Literal {
    content: String,
    /// `special: true` in the added-token table — a CONTROL token, which is what S7 names.
    special: bool,
}

struct Corpus {
    literals: Vec<Literal>,
    shapes: Vec<String>,
    lookalikes: Vec<String>,
}

/// `\n`, `\t`, `\\` and `\u{XXXX}`. Written out rather than pulled in as a dependency: the corpus
/// needs invisible characters spelled visibly, and nothing more.
fn unescape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some('u') => {
                assert_eq!(chars.next(), Some('{'), "a \\u escape is written \\u{{XXXX}}");
                let mut hex = String::new();
                for c in chars.by_ref() {
                    if c == '}' {
                        break;
                    }
                    hex.push(c);
                }
                let code = u32::from_str_radix(&hex, 16).unwrap_or_else(|_| panic!("\\u{{{hex}}} is not hex"));
                out.push(char::from_u32(code).unwrap_or_else(|| panic!("\\u{{{hex}}} is not a code point")));
            }
            other => panic!("unknown escape \\{other:?} in the corpus"),
        }
    }
    out
}

fn corpus() -> Corpus {
    let mut literals = Vec::new();
    let mut shapes = Vec::new();
    let mut lookalikes = Vec::new();
    let mut section = "";
    for line in CORPUS.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        if line.starts_with('[') {
            section = line.trim();
            continue;
        }
        match section {
            "[literals]" => {
                let (content, kind) =
                    line.split_once('\t').unwrap_or_else(|| panic!("a literal row needs a TAB and a kind: {line:?}"));
                let special = match kind.trim() {
                    "special" => true,
                    "added" => false,
                    other => panic!("a literal's kind is `special` or `added`, not {other:?}"),
                };
                literals.push(Literal { content: content.to_string(), special });
            }
            "[shapes]" => shapes.push(unescape(line)),
            "[lookalikes]" => lookalikes.push(unescape(line)),
            other => panic!("unknown corpus section {other:?}"),
        }
    }
    Corpus { literals, shapes, lookalikes }
}

/// Every shape, filled with every literal — the "and embedded inside ordinary text, in the middle
/// of words, doubled" half of S7's sentence, as strings.
fn user_text_cases(c: &Corpus) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for literal in &c.literals {
        for shape in &c.shapes {
            out.push((literal.content.clone(), shape.replace("%S%", &literal.content)));
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// The fixture tokenizer: a real added-token table over a byte-level vocabulary
// ---------------------------------------------------------------------------------------------

/// GPT-2's byte↔char table, as `tokenizer.rs` builds it. Reproduced here because the fixture has
/// to WRITE a vocabulary the loader will accept, and the loader's alphabet is that table. It is a
/// published constant, not the logic under test.
fn byte_to_char_table() -> [char; 256] {
    let mut assigned = [false; 256];
    let mut table = ['\0'; 256];
    for b in (b'!'..=b'~').chain(0xA1..=0xAC).chain(0xAE..=0xFF) {
        table[b as usize] = char::from_u32(u32::from(b)).expect("a byte is a code point");
        assigned[b as usize] = true;
    }
    let mut next = 0u32;
    for (b, slot) in table.iter_mut().enumerate() {
        if !assigned[b] {
            *slot = char::from_u32(256 + next).expect("in range");
            next += 1;
        }
    }
    table
}

/// A `tokenizer.json` whose vocabulary is the 256 byte characters and whose `added_tokens` are
/// exactly the corpus literals, with the corpus's own `special` flags and no merges.
///
/// No merges is deliberate: every ordinary piece then encodes to one id per byte, so the round
/// trip is total and a look-alike that survived as text can be PROVEN to have survived (its ids
/// decode back to it) rather than merely proven not to be a control id.
fn fixture_tokenizer(c: &Corpus) -> (QwenTokenizer, Vec<(Literal, u32)>) {
    let table = byte_to_char_table();
    let mut vocab = serde_json::Map::new();
    for (b, ch) in table.iter().enumerate() {
        vocab.insert(ch.to_string(), serde_json::json!(b));
    }
    let mut added = Vec::new();
    let mut ids = Vec::new();
    for (n, literal) in c.literals.iter().enumerate() {
        let id = 256 + n as u32;
        added.push(serde_json::json!({ "id": id, "content": literal.content, "special": literal.special }));
        ids.push((literal.clone(), id));
    }
    let doc = serde_json::json!({
        "model": { "vocab": vocab, "merges": [] },
        "added_tokens": added,
    });
    let bytes = serde_json::to_vec(&doc).expect("the fixture serializes");
    (QwenTokenizer::from_json(&bytes).expect("the fixture loads"), ids)
}

/// The added-token table of a real family `tokenizer.json`, read off the FILE — the enumeration
/// S7 asks for ("every special-token literal"), never a list typed from memory.
fn real_added_tokens(bytes: &[u8]) -> Vec<(String, u32, bool)> {
    let doc: serde_json::Value = serde_json::from_slice(bytes).expect("the tokenizer file is JSON");
    doc.get("added_tokens")
        .and_then(|v| v.as_array())
        .map(|list| {
            list.iter()
                .map(|e| {
                    (
                        e.get("content").and_then(|v| v.as_str()).expect("an added token has content").to_string(),
                        e.get("id").and_then(|v| v.as_u64()).expect("an added token has an id") as u32,
                        e.get("special").and_then(|v| v.as_bool()).unwrap_or(false),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `Some((bytes, path))` when the operator pointed the env var at a real family tokenizer.
fn real_tokenizer_bytes() -> Option<(Vec<u8>, String)> {
    let path = std::env::var(QWEN_TOKENIZER_ENV).ok().filter(|p| !p.trim().is_empty())?;
    match std::fs::read(&path) {
        Ok(bytes) => Some((bytes, path)),
        Err(e) => panic!("{QWEN_TOKENIZER_ENV}={path}: {e} — point it at a real tokenizer.json or unset it"),
    }
}

fn skip(what: &str) {
    eprintln!("SKIPPED: {what} — set {QWEN_TOKENIZER_ENV}=/path/to/tokenizer.json (a Qwen2.5 or Qwen3.x checkpoint's) to run it");
}

/// **The one line that changes when the encoder lands.** Today the crate exposes exactly one
/// encoder and it parses specials; see [`the_user_text_encoder_is_still_missing`].
fn encode_as_user_text(tokenizer: &QwenTokenizer, text: &str) -> Vec<u32> {
    // ADR-0079 R-05 armed: the untrusted-text encoder is `encode_without_specials` (stream W),
    // and the worker's Text arm goes through it.
    tokenizer.encode_without_specials(text).expect("a byte-level tokenizer represents every input")
}

// ---------------------------------------------------------------------------------------------
// The corpus's own coverage — a corpus that does not cover is a corpus that cannot pin
// ---------------------------------------------------------------------------------------------

#[test]
fn the_corpus_is_well_formed_and_large() {
    let c = corpus();
    assert!(c.literals.len() >= 33, "the two shipped families declare 33 added tokens between them, got {}", c.literals.len());
    assert!(c.literals.iter().filter(|l| l.special).count() >= 21, "Qwen3.x alone marks 21 of its added tokens special");
    assert!(c.shapes.len() >= 20, "S7 wants embedded, mid-word, doubled — {} shapes is not a corpus", c.shapes.len());
    assert!(c.lookalikes.len() >= 25, "only {} look-alikes", c.lookalikes.len());

    let mut seen = std::collections::HashSet::new();
    for l in &c.literals {
        assert!(seen.insert(l.content.clone()), "the corpus repeats the literal {:?}", l.content);
        assert!(!l.content.is_empty(), "an empty literal matches everywhere");
    }
    for shape in &c.shapes {
        assert!(shape.contains("%S%"), "a shape with no %S% is not a shape: {shape:?}");
    }

    let cases = user_text_cases(&c);
    assert_eq!(cases.len(), c.literals.len() * c.shapes.len());
    eprintln!(
        "corpus: {} literals ({} special) x {} shapes = {} user-text cases, + {} look-alikes = {} total",
        c.literals.len(),
        c.literals.iter().filter(|l| l.special).count(),
        c.shapes.len(),
        cases.len(),
        c.lookalikes.len(),
        cases.len() + c.lookalikes.len()
    );
}

/// A look-alike that contains the real literal tests the opposite of what it claims: it would be
/// EXPECTED to produce a control id, and its passing would mean the assertion had been inverted.
#[test]
fn no_lookalike_contains_a_real_literal() {
    let c = corpus();
    for line in &c.lookalikes {
        for literal in &c.literals {
            assert!(
                !line.contains(&literal.content),
                "the look-alike {line:?} contains the real literal {:?} — it belongs in [shapes], not [lookalikes]",
                literal.content
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The mechanism — this is not an empty vocabulary
// ---------------------------------------------------------------------------------------------

/// **The mechanism arm.** Each literal, presented AS a special (the template's own act), yields
/// exactly that id. Without this the S7 assertions below would be satisfied by a tokenizer that
/// has no control tokens at all — which is the shipped floor class, PALW-BASE-0.
#[test]
fn special_literals_are_live_control_ids() {
    let c = corpus();
    let (tokenizer, ids) = fixture_tokenizer(&c);
    for (literal, id) in &ids {
        assert_eq!(
            tokenizer.added_id(&literal.content),
            Some(*id),
            "{:?} is not in the added-token table the tokenizer loaded",
            literal.content
        );
        let encoded = tokenizer.encode(&literal.content).expect("the fixture represents every input");
        assert_eq!(encoded, vec![*id], "{:?} presented as a special must be exactly its own id", literal.content);
        assert_eq!(
            tokenizer.decode(&encoded).expect("an added token decodes to its content"),
            literal.content,
            "{:?} must round-trip",
            literal.content
        );
    }
    assert!(ids.iter().any(|(l, _)| l.special), "the fixture must carry at least one CONTROL token or S7 is vacuous here");
}

/// The same mechanism on a real family table, and the enumeration check S7 asks for: **every**
/// `special: true` entry of the loaded table is in the corpus. A family that ships one this
/// corpus does not name is a corpus that no longer includes every special-token literal.
#[test]
fn the_corpus_covers_every_special_of_a_real_family_table() {
    let Some((bytes, path)) = real_tokenizer_bytes() else {
        skip("the real-family coverage check");
        return;
    };
    let c = corpus();
    let table = real_added_tokens(&bytes);
    assert!(!table.is_empty(), "{path} declares no added_tokens — that is not a family tokenizer");
    let known: std::collections::HashSet<&str> = c.literals.iter().map(|l| l.content.as_str()).collect();
    let special_known: std::collections::HashSet<&str> = c.literals.iter().filter(|l| l.special).map(|l| l.content.as_str()).collect();

    let mut missing = Vec::new();
    let mut misclassified = Vec::new();
    for (content, _, special) in &table {
        if !known.contains(content.as_str()) {
            missing.push(content.clone());
        } else if *special && !special_known.contains(content.as_str()) {
            misclassified.push(content.clone());
        }
    }
    assert!(
        missing.is_empty(),
        "{path} declares added tokens the corpus does not name: {missing:?} — add them to tests/corpus/special-tokens.txt"
    );
    assert!(misclassified.is_empty(), "{path} marks these `special: true` but the corpus files them as `added`: {misclassified:?}");

    let tokenizer = QwenTokenizer::from_json(&bytes).expect("a real tokenizer.json loads");
    for (content, id, _) in &table {
        assert_eq!(tokenizer.added_id(content), Some(*id), "{path}: {content:?} lost its id on load");
        assert_eq!(
            tokenizer.encode(content).expect("a real vocabulary represents its own added tokens"),
            vec![*id],
            "{path}: {content:?} presented as a special must be exactly its own id"
        );
    }
    eprintln!(
        "real family {path}: {} added tokens, {} special — all named by the corpus",
        table.len(),
        table.iter().filter(|t| t.2).count()
    );
}

// ---------------------------------------------------------------------------------------------
// S7, the look-alike half — GREEN
// ---------------------------------------------------------------------------------------------

/// **S7, the half that holds today.** A fullwidth `＜`, a `｜` where a `|` was, a zero-width space
/// inside the name, a Cyrillic `і`, a combining acute, a case change, a truncation — none of them
/// is the literal, so none of them may become a control id.
///
/// The decoded form is checked too, for a hazard that is not obvious: the tokenizer NFC-normalizes
/// ordinary text, and a *compatibility* normalization (NFKC) maps `＜` to `<` and `｜` to `|` —
/// which would MANUFACTURE `<|im_start|>` out of a look-alike after the added-token match had
/// already been decided. NFC does not, and this is the assertion that says so.
#[test]
fn unicode_lookalikes_never_become_a_control_id() {
    let c = corpus();
    let (tokenizer, ids) = fixture_tokenizer(&c);
    let control: std::collections::HashSet<u32> = ids.iter().filter(|(l, _)| l.special).map(|(_, id)| *id).collect();
    let any_added: std::collections::HashSet<u32> = ids.iter().map(|(_, id)| *id).collect();

    for line in &c.lookalikes {
        let out = tokenizer.encode(line).expect("the fixture represents every input");
        for id in &out {
            assert!(!control.contains(id), "the look-alike {line:?} produced control id {id}");
            assert!(!any_added.contains(id), "the look-alike {line:?} produced added-token id {id}");
        }
        // It went through as bytes, and normalization did not turn it into the real thing.
        let decoded = tokenizer.decode(&out).expect("byte ids decode");
        for literal in &c.literals {
            assert!(
                !decoded.contains(&literal.content),
                "normalizing the look-alike {line:?} manufactured the literal {:?} ({decoded:?}) — the pre-tokenizer must \
                 normalize with NFC, never a compatibility form",
                literal.content
            );
        }
        assert_eq!(
            tokenizer.encode(&decoded).expect("the fixture represents every input"),
            out,
            "encoding is not idempotent on the look-alike {line:?}"
        );
    }
}

/// The same, on a real family table — where the vocabulary is 150k+ pieces and the assertion is
/// not arithmetic on a fixture's id range.
#[test]
fn unicode_lookalikes_never_become_a_control_id_on_a_real_family() {
    let Some((bytes, path)) = real_tokenizer_bytes() else {
        skip("the real-family look-alike arm");
        return;
    };
    let c = corpus();
    let tokenizer = QwenTokenizer::from_json(&bytes).expect("a real tokenizer.json loads");
    let table = real_added_tokens(&bytes);
    let control: std::collections::HashSet<u32> = table.iter().filter(|t| t.2).map(|t| t.1).collect();
    assert!(!control.is_empty(), "{path} marks nothing special — S7 would be vacuous against it");

    let literals: Vec<&str> = c.literals.iter().map(|l| l.content.as_str()).collect();
    for line in &c.lookalikes {
        let out = tokenizer.encode(line).expect("a byte-level BPE represents every input");
        for id in &out {
            assert!(!control.contains(id), "{path}: the look-alike {line:?} produced control id {id}");
        }
        let decoded = tokenizer.decode(&out).expect("a real vocabulary decodes what it encoded");
        for literal in &literals {
            assert!(!decoded.contains(literal), "{path}: normalizing {line:?} manufactured {literal:?} ({decoded:?})");
        }
    }
    eprintln!("real family {path}: {} look-alikes produced no control id", c.lookalikes.len());
}

// ---------------------------------------------------------------------------------------------
// S7, the literal half — the pin (armed: the worker encodes user text without specials)
// ---------------------------------------------------------------------------------------------

/// **S7's pin, written and not yet armed.**
///
/// This is the assertion S7 asks for: every corpus case, presented as USER TEXT, yields no id the
/// added-token table marks `special`. User text is encoded with [`QwenTokenizer::encode_without_specials`]
/// — the encoder the worker's Text arm uses (`fp_worker.rs`) — while [`QwenTokenizer::encode`], which
/// matches added tokens on the raw string first, remains the TEMPLATE's encoder: a template that means
/// to emit a control token says so through the `Segments` arm's explicit `Special(id)`. The coverage
/// arm above proves every literal is a live control id, so this cannot pass vacuously.
#[test]
fn s7_untrusted_text_never_yields_a_control_id() {
    let c = corpus();
    let (tokenizer, ids) = fixture_tokenizer(&c);
    let control: std::collections::HashSet<u32> = ids.iter().filter(|(l, _)| l.special).map(|(_, id)| *id).collect();

    let mut leaked = Vec::new();
    for (literal, case) in user_text_cases(&c) {
        for id in encode_as_user_text(&tokenizer, &case) {
            if control.contains(&id) {
                leaked.push(format!("{case:?} smuggled {literal:?} as control id {id}"));
                break;
            }
        }
    }
    for line in &c.lookalikes {
        for id in encode_as_user_text(&tokenizer, line) {
            assert!(!control.contains(&id), "the look-alike {line:?} produced control id {id}");
        }
    }
    // The same assertion against a real family table, where the ids are the ones the worker
    // manifest publishes and the class actually executes.
    if let Some((bytes, path)) = real_tokenizer_bytes() {
        let real = QwenTokenizer::from_json(&bytes).expect("a real tokenizer.json loads");
        let real_control: std::collections::HashSet<u32> =
            real_added_tokens(&bytes).into_iter().filter(|t| t.2).map(|t| t.1).collect();
        for (literal, case) in user_text_cases(&c) {
            for id in encode_as_user_text(&real, &case) {
                if real_control.contains(&id) {
                    leaked.push(format!("{path}: {case:?} smuggled {literal:?} as control id {id}"));
                    break;
                }
            }
        }
    } else {
        skip("the real-family half of the S7 pin");
    }

    assert!(
        leaked.is_empty(),
        "ADR-0079 S7: untrusted prompt text never tokenizes to a control token. {} cases did (of {} per tokenizer):\n  {}",
        leaked.len(),
        c.literals.len() * c.shapes.len(),
        leaked.iter().take(10).cloned().collect::<Vec<_>>().join("\n  ")
    );
}
