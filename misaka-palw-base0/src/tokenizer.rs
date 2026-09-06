//! **The Qwen byte-level BPE tokenizer, as part of the runtime.**
//!
//! # Why this is in the runtime and not in consensus
//!
//! PALW binds `prompt_token_ids_hash` and `tokenizer_id`, never the text (`palw_v2.rs`: the
//! worker "never tokenizes, normalizes or templates text on this path"). That is what keeps a
//! regex-engine difference from being able to fork a chain, and it is a deliberate boundary.
//!
//! But a runtime that cannot turn text into ids is not a runtime — it is a forward pass with a
//! test harness — so the tokenizer belongs here, on the runtime side of that boundary, hashed
//! into the artifact's `tokenizer_commitment` so that "the same Qwen" is not an ambiguous claim.
//!
//! # What is implemented, exactly
//!
//! `tokenizer.json` for Qwen2.5 declares: NFC normalization; a `Split` pre-tokenizer with the
//! GPT-4 pattern under `Isolated` behaviour; ByteLevel with `add_prefix_space: false`; and a BPE
//! model with no unknown token, no byte fallback and no continuing-subword prefix. All of that is
//! reproduced here rather than delegated, because the failure mode of delegating it is a silent
//! version bump that re-tokenizes every prompt.
//!
//! ## The pre-tokenizer pattern, and why it is hand-written
//!
//! ```text
//! (?i:'s|'t|'re|'ve|'m|'ll|'d)|[^\r\n\p{L}\p{N}]?\p{L}+|\p{N}| ?[^\s\p{L}\p{N}]+[\r\n]*|\s*[\r\n]+|\s+(?!\S)|\s+
//! ```
//!
//! Rust's `regex` crate has no lookahead, so `\s+(?!\S)` cannot be expressed and the pattern
//! cannot be compiled as written. It is therefore implemented as an ordered scan: at each
//! position the seven alternatives are tried in the order they appear and the FIRST that matches
//! wins — which is what a backtracking engine does, and is not the same as taking the longest
//! match. `\p{L}` and `\p{N}` come from `unicode-properties`, not from `char::is_alphabetic`:
//! Unicode's `Alphabetic` property includes `Nl` (Roman numerals) and `Other_Alphabetic`
//! (combining marks), so approximating with it puts `Ⅷ` in the letter branch where the pattern
//! puts it in the number branch.

use std::collections::HashMap;
use unicode_normalization::UnicodeNormalization;
use unicode_properties::{GeneralCategoryGroup, UnicodeGeneralCategory};

/// Why a tokenizer could not be built or used.
#[derive(Debug)]
pub enum TokenizerError {
    /// `tokenizer.json` was not the shape this loader reads.
    Malformed(&'static str),
    /// A piece produced by the pre-tokenizer had no id after every merge was applied. With no
    /// unknown token and no byte fallback declared, that is unrepresentable rather than degraded.
    Unrepresentable(String),
    /// Decoding produced bytes that are not UTF-8. Returned rather than replaced: a runtime that
    /// silently substitutes U+FFFD hides a real decoding bug.
    NotUtf8,
    /// [`QwenTokenizer::encode_without_specials`] found an added token's id in its OWN ordinary
    /// output (ADR-0077 Decision 6, ADR-0079 Decision 7).
    ///
    /// Not a theoretical guard. `encode_ordinary` looks its merged pieces up in the same
    /// vocabulary the added tokens live in, so any added token whose content the pre-tokenizer
    /// does NOT split — a `<pad>`-style marker, or any of the plain-looking strings a GGUF is free
    /// to declare `USER_DEFINED` — is reachable from prose, and the whole promise of the segment
    /// arm is that prose cannot reach one. A pattern that happens to split every marker in today's
    /// two checkpoints is not that promise; checking the output is.
    ControlTokenFromText(u32),
}

impl TokenizerError {
    /// **The reason, with nothing of the input in it** (ADR-0079 SA-7).
    ///
    /// [`Self::Unrepresentable`] carries the piece that had no id, which is a fragment of whatever
    /// text was encoded — for the free-prompt worker that is a stranger's prompt. `Display` is
    /// right for a converter's console and wrong for anything a prompt reaches, so the two forms
    /// are named separately rather than left to each caller's judgement about which one it holds.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Malformed(_) => "the tokenizer file is not the shape this loader reads",
            Self::Unrepresentable(_) => "a piece of the text has no id in this vocabulary",
            Self::NotUtf8 => "the decoded bytes are not UTF-8",
            Self::ControlTokenFromText(_) => "ordinary text encoded to a control token, which no text may produce",
        }
    }
}

impl std::fmt::Display for TokenizerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(what) => write!(f, "tokenizer.json: {what}"),
            Self::Unrepresentable(p) => write!(f, "no token covers the piece {p:?}"),
            Self::NotUtf8 => write!(f, "decoded bytes are not UTF-8"),
            Self::ControlTokenFromText(id) => write!(f, "ordinary text encoded to the added token {id}, which no text may produce"),
        }
    }
}

impl std::error::Error for TokenizerError {}

/// GPT-2's byte↔char table: the 256 bytes mapped to printable code points so that a byte string
/// is a string the BPE can merge over. The holes (control bytes, space, and the two Latin-1 gaps)
/// are filled from U+0100 upward in byte order.
fn byte_to_char_table() -> [char; 256] {
    let mut assigned = [false; 256];
    let mut table = ['\0'; 256];
    for b in (b'!'..=b'~').chain(0xA1..=0xAC).chain(0xAE..=0xFF) {
        table[b as usize] = char::from_u32(b as u32).expect("a byte is a valid code point");
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

fn is_letter(c: char) -> bool {
    c.general_category_group() == GeneralCategoryGroup::Letter
}

fn is_number(c: char) -> bool {
    c.general_category_group() == GeneralCategoryGroup::Number
}

fn is_space(c: char) -> bool {
    c.is_whitespace()
}

/// One added token: an exact string that is matched before any splitting and emitted as its id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddedToken {
    pub id: u32,
    pub content: String,
    pub special: bool,
}

/// The loaded tokenizer.
pub struct QwenTokenizer {
    vocab: HashMap<String, u32>,
    /// id → token string, for decoding.
    tokens: Vec<String>,
    /// `(left, right)` → rank. Lower merges first.
    merges: HashMap<(String, String), u32>,
    added: Vec<AddedToken>,
    byte_to_char: [char; 256],
    char_to_byte: HashMap<char, u8>,
    /// The bytes the file itself was made of — the artifact's tokenizer commitment is over these.
    source_len: usize,
}

impl QwenTokenizer {
    /// Parse a `tokenizer.json`.
    pub fn from_json(bytes: &[u8]) -> Result<Self, TokenizerError> {
        let root: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| TokenizerError::Malformed("not JSON"))?;
        let model = root.get("model").ok_or(TokenizerError::Malformed("no model"))?;
        let vocab_json = model.get("vocab").and_then(|v| v.as_object()).ok_or(TokenizerError::Malformed("no model.vocab"))?;

        let mut vocab = HashMap::with_capacity(vocab_json.len());
        let mut highest = 0u32;
        for (token, id) in vocab_json {
            let id = id.as_u64().ok_or(TokenizerError::Malformed("a vocab id is not a number"))? as u32;
            highest = highest.max(id);
            vocab.insert(token.clone(), id);
        }

        let merges_json = model.get("merges").and_then(|v| v.as_array()).ok_or(TokenizerError::Malformed("no model.merges"))?;
        let mut merges = HashMap::with_capacity(merges_json.len());
        for (rank, entry) in merges_json.iter().enumerate() {
            // Two shapes exist in the wild: `"a b"` and `["a", "b"]`. Both are read, because a
            // loader that handles one and silently mis-parses the other produces a tokenizer that
            // merges nothing and is only noticed as bad model output.
            let pair = match entry {
                serde_json::Value::String(s) => {
                    let mut it = s.splitn(2, ' ');
                    match (it.next(), it.next()) {
                        (Some(a), Some(b)) => (a.to_string(), b.to_string()),
                        _ => return Err(TokenizerError::Malformed("a merge is not two pieces")),
                    }
                }
                serde_json::Value::Array(parts) if parts.len() == 2 => (
                    parts[0].as_str().ok_or(TokenizerError::Malformed("a merge part is not a string"))?.to_string(),
                    parts[1].as_str().ok_or(TokenizerError::Malformed("a merge part is not a string"))?.to_string(),
                ),
                _ => return Err(TokenizerError::Malformed("a merge is neither a string nor a pair")),
            };
            merges.insert(pair, rank as u32);
        }

        let mut added = Vec::new();
        if let Some(list) = root.get("added_tokens").and_then(|v| v.as_array()) {
            for entry in list {
                let id = entry.get("id").and_then(|v| v.as_u64()).ok_or(TokenizerError::Malformed("added token id"))? as u32;
                let content =
                    entry.get("content").and_then(|v| v.as_str()).ok_or(TokenizerError::Malformed("added token content"))?.to_string();
                highest = highest.max(id);
                added.push(AddedToken { id, content, special: entry.get("special").and_then(|v| v.as_bool()).unwrap_or(false) });
            }
        }
        // Longest content first: `<|im_start|>` must not be shadowed by a shorter added token that
        // happens to be its prefix.
        added.sort_by(|a, b| b.content.len().cmp(&a.content.len()).then(a.id.cmp(&b.id)));

        let mut tokens = vec![String::new(); highest as usize + 1];
        for (token, id) in &vocab {
            tokens[*id as usize] = token.clone();
        }
        for a in &added {
            tokens[a.id as usize] = a.content.clone();
        }

        let byte_to_char = byte_to_char_table();
        let char_to_byte = byte_to_char.iter().enumerate().map(|(b, c)| (*c, b as u8)).collect();
        Ok(Self { vocab, tokens, merges, added, byte_to_char, char_to_byte, source_len: bytes.len() })
    }

    /// Build from a GGUF's `tokenizer.ggml.*` metadata.
    ///
    /// This checkpoint's repository ships no `tokenizer.json` — the vocabulary and the merge table
    /// live in the GGUF header, which is where llama.cpp reads them from too. The pieces are the
    /// same ones [`Self::from_json`] takes and the byte-level alphabet is identical; what differs
    /// is only where they were written down.
    ///
    /// `token_type` is GGUF's per-token classification. Types 3 (CONTROL) and 4 (USER_DEFINED) are
    /// the added tokens: matched whole, before any splitting, exactly as `added_tokens` are in the
    /// JSON form. Anything else is an ordinary BPE piece.
    ///
    /// `source_len` is the number of bytes the vocabulary and merges occupy, so a caller that
    /// commits to a tokenizer commits to a length that changes when either does.
    pub fn from_gguf(tokens: &[String], merges: &[String], token_type: &[i64]) -> Result<Self, TokenizerError> {
        if tokens.is_empty() {
            return Err(TokenizerError::Malformed("no tokenizer.ggml.tokens"));
        }
        let mut vocab = HashMap::with_capacity(tokens.len());
        for (id, token) in tokens.iter().enumerate() {
            // First id wins: a duplicate string in the table is the vocabulary's own business and
            // the lower id is the one a greedy encoder should reach for.
            vocab.entry(token.clone()).or_insert(id as u32);
        }
        let mut table = HashMap::with_capacity(merges.len());
        for (rank, entry) in merges.iter().enumerate() {
            let mut it = entry.splitn(2, ' ');
            match (it.next(), it.next()) {
                (Some(a), Some(b)) => table.insert((a.to_string(), b.to_string()), rank as u32),
                _ => return Err(TokenizerError::Malformed("a merge is not two pieces")),
            };
        }
        let mut added: Vec<AddedToken> = token_type
            .iter()
            .enumerate()
            .filter(|(id, kind)| matches!(**kind, 3 | 4) && *id < tokens.len())
            .map(|(id, kind)| AddedToken { id: id as u32, content: tokens[id].clone(), special: *kind == 3 })
            .collect();
        added.sort_by(|a, b| b.content.len().cmp(&a.content.len()).then(a.id.cmp(&b.id)));

        let byte_to_char = byte_to_char_table();
        let char_to_byte = byte_to_char.iter().enumerate().map(|(b, c)| (*c, b as u8)).collect();
        let source_len = tokens.iter().map(|t| t.len()).sum::<usize>() + merges.iter().map(|m| m.len()).sum::<usize>();
        Ok(Self { vocab, tokens: tokens.to_vec(), merges: table, added, byte_to_char, char_to_byte, source_len })
    }

    /// Ids this tokenizer can produce, one past the highest. Not the model's `vocab_size`, which
    /// is padded — a mismatch between the two is normal and is why the engine's logit row is wider
    /// than this.
    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    /// The bytes the loader consumed, for the caller that commits to them.
    pub fn source_len(&self) -> usize {
        self.source_len
    }

    /// An added token's id by exact content — how a caller names `<|im_start|>` without hardcoding
    /// 151644.
    pub fn added_id(&self, content: &str) -> Option<u32> {
        self.added.iter().find(|a| a.content == content).map(|a| a.id)
    }

    /// **Every added token, by name and id** — the table a worker publishes in its manifest
    /// (`PalwFpWorkerManifestV1::special_tokens`) so a gateway builds
    /// `PalwFpPromptSegmentV1::Special` from a NAME it looked up and never from an id it guessed.
    ///
    /// Read-only, and deliberately the whole table rather than a curated subset: which of a
    /// model's control tokens a template needs is the template's business, and a runtime that
    /// filtered the list would decide it silently for every future template.
    pub fn added_tokens(&self) -> &[AddedToken] {
        &self.added
    }

    /// Is this id one of the added tokens? The check behind a `Special` segment: a gateway names a
    /// control token from the manifest's table, so an id that is not in it is not a control token
    /// however plausible it looks, and passing it through would be a prompt nobody wrote.
    pub fn is_added_id(&self, id: u32) -> bool {
        self.added.iter().any(|a| a.id == id)
    }

    /// **Encode text that must never become a control token** (ADR-0077 Decision 6).
    ///
    /// [`Self::encode`] matches added tokens FIRST, on the raw text, which is correct for a
    /// rendered template and is exactly the smuggling path Decision 6 closes: under it a user who
    /// types the twelve literal characters `<|im_start|>` hands the model a control id, ends the
    /// system turn early and speaks in the template's own voice. This entry point does not consult
    /// the added-token table at all — the text becomes ordinary byte-level BPE pieces, whatever it
    /// spells — so the only way a control id reaches the prompt is a
    /// `PalwFpPromptSegmentV1::Special` the GATEWAY emitted.
    ///
    /// [`Self::encode`] is left exactly as it was: the v1 plain-marker template and the replay
    /// paths depend on its behaviour, and a tokenizer with two rules must say which one it is
    /// applying rather than change the one everybody already calls.
    ///
    /// **The result is checked, not assumed.** Skipping the added-token MATCH is not the same as
    /// producing no added-token ID: `encode_ordinary` resolves its merged pieces against the same
    /// vocabulary the added tokens live in, so an added token the pre-tokenizer does not split
    /// would come back out of the ordinary path. Today's two checkpoints spell their markers
    /// `<|…|>`, which the pattern does split — but "the marker happens to contain characters the
    /// current pattern breaks on" is a coincidence, and Decision 6 is a promise. So the promise is
    /// enforced where it can be seen: on the ids this returns.
    pub fn encode_without_specials(&self, text: &str) -> Result<Vec<u32>, TokenizerError> {
        let mut out = Vec::new();
        self.encode_ordinary(text, &mut out)?;
        if let Some(id) = out.iter().find(|id| self.is_added_id(**id)) {
            return Err(TokenizerError::ControlTokenFromText(*id));
        }
        Ok(out)
    }

    /// Encode text to token ids.
    ///
    /// Added tokens are matched first, on the RAW text, before normalization: they are declared
    /// `normalized: false`, and NFC-ing `<|im_start|>` would be a no-op today and a hazard the
    /// moment a template carries a decomposable character next to one.
    pub fn encode(&self, text: &str) -> Result<Vec<u32>, TokenizerError> {
        let mut out = Vec::new();
        let mut rest = text;
        while !rest.is_empty() {
            match self.leftmost_added(rest) {
                Some((at, added)) => {
                    if at > 0 {
                        self.encode_ordinary(&rest[..at], &mut out)?;
                    }
                    out.push(added.id);
                    rest = &rest[at + added.content.len()..];
                }
                None => {
                    self.encode_ordinary(rest, &mut out)?;
                    break;
                }
            }
        }
        Ok(out)
    }

    fn leftmost_added(&self, text: &str) -> Option<(usize, &AddedToken)> {
        let mut best: Option<(usize, &AddedToken)> = None;
        for a in &self.added {
            if let Some(at) = text.find(&a.content)
                && best.map(|(b, _)| at < b).unwrap_or(true)
            {
                best = Some((at, a));
            }
        }
        best
    }

    fn encode_ordinary(&self, text: &str, out: &mut Vec<u32>) -> Result<(), TokenizerError> {
        let normalized: String = text.nfc().collect();
        for piece in pre_tokenize(&normalized) {
            let mapped: String = piece.bytes().map(|b| self.byte_to_char[b as usize]).collect();
            for token in self.bpe(&mapped) {
                let id = *self.vocab.get(&token).ok_or_else(|| TokenizerError::Unrepresentable(token.clone()))?;
                out.push(id);
            }
        }
        Ok(())
    }

    /// Standard BPE: repeatedly merge the adjacent pair with the lowest rank.
    ///
    /// Written over `Vec<String>` rather than over indices into a symbol table because the
    /// merged strings are what the vocabulary is keyed by; the allocation is real and it is not
    /// where this runtime's time goes.
    fn bpe(&self, word: &str) -> Vec<String> {
        let mut parts: Vec<String> = word.chars().map(|c| c.to_string()).collect();
        if parts.len() < 2 {
            return parts;
        }
        loop {
            let mut best: Option<(usize, u32)> = None;
            for i in 0..parts.len() - 1 {
                if let Some(rank) = self.merges.get(&(parts[i].clone(), parts[i + 1].clone()))
                    && best.map(|(_, r)| *rank < r).unwrap_or(true)
                {
                    best = Some((i, *rank));
                }
            }
            let Some((at, _)) = best else { return parts };
            let merged = format!("{}{}", parts[at], parts[at + 1]);
            parts.splice(at..at + 2, [merged]);
            if parts.len() == 1 {
                return parts;
            }
        }
    }

    /// **The bytes of ONE token** — the primitive a streaming detokenizer needs (ADR-0077
    /// Decision 2; the `Token` frame carries "this id's rendering alone").
    ///
    /// `None`, and not an error, for an id this table does not hold. That is a NORMAL id: a class
    /// registers a padded `vocab_size` and the engine's argmax may select anywhere in its logit
    /// row, so a model can legitimately produce an id past the tokenizer's own table. The caller
    /// renders nothing for it and keeps going — the alternative, which this replaces, was a
    /// `decode` failure that threw away a completed inference because one token had no spelling.
    ///
    /// The concatenation over a run's ids is exactly the answer's bytes, which is what lets a
    /// worker's streamed pieces and its final `rendered` field be the same bytes by construction
    /// rather than by two decoders agreeing.
    pub fn token_bytes(&self, id: u32) -> Option<Vec<u8>> {
        let token = self.tokens.get(id as usize)?;
        if self.added.iter().any(|a| a.id == id) {
            return Some(token.as_bytes().to_vec());
        }
        // A token holding a character outside the byte table has no byte spelling; it is skipped
        // rather than substituted, for the same reason `decode` refuses U+FFFD.
        token.chars().map(|c| self.char_to_byte.get(&c).copied()).collect()
    }

    /// Decode ids back to text.
    pub fn decode(&self, ids: &[u32]) -> Result<String, TokenizerError> {
        let mut bytes = Vec::with_capacity(ids.len() * 4);
        for id in ids {
            let token = self.tokens.get(*id as usize).ok_or(TokenizerError::Malformed("id past the vocabulary"))?;
            if self.added.iter().any(|a| a.id == *id) {
                bytes.extend_from_slice(token.as_bytes());
                continue;
            }
            for c in token.chars() {
                bytes.push(*self.char_to_byte.get(&c).ok_or(TokenizerError::Malformed("a token holds a non-byte character"))?);
            }
        }
        String::from_utf8(bytes).map_err(|_| TokenizerError::NotUtf8)
    }

    /// Decode ignoring an incomplete trailing UTF-8 sequence — what a streaming decoder needs,
    /// since a multi-byte character can straddle two tokens.
    ///
    /// **Ignoring is not substituting**, and the difference is the whole point of this function.
    /// `String::from_utf8_lossy` puts `U+FFFD` where the incomplete tail is, and a streaming caller
    /// that emits the new suffix each step therefore SENT that replacement character before the
    /// next token completed the kanji — so `申し訳` reached the user as `申し` + `<?>`, while the
    /// same run's non-streaming answer was correct. It also made the decoded string non-monotonic
    /// (three bytes appear, then vanish), which a caller slicing at a remembered byte offset can
    /// only survive by luck: the next slice can land inside a character and panic.
    ///
    /// So a tail that is merely INCOMPLETE is held back, and only bytes that are genuinely INVALID
    /// become `U+FFFD` — otherwise one corrupt token would stall the stream forever waiting for a
    /// continuation that is never coming.
    pub fn decode_lossy_tail(&self, ids: &[u32]) -> String {
        let mut bytes = Vec::with_capacity(ids.len() * 4);
        for id in ids {
            let Some(token) = self.tokens.get(*id as usize) else { continue };
            if self.added.iter().any(|a| a.id == *id) {
                bytes.extend_from_slice(token.as_bytes());
                continue;
            }
            for c in token.chars() {
                if let Some(b) = self.char_to_byte.get(&c) {
                    bytes.push(*b);
                }
            }
        }
        let mut out = String::with_capacity(bytes.len());
        let mut rest = &bytes[..];
        loop {
            match std::str::from_utf8(rest) {
                Ok(valid) => {
                    out.push_str(valid);
                    break;
                }
                Err(e) => {
                    let (valid, after) = rest.split_at(e.valid_up_to());
                    out.push_str(std::str::from_utf8(valid).expect("valid_up_to bounds a valid prefix"));
                    match e.error_len() {
                        // Real garbage: mark it and move on, or the stream stops here forever.
                        Some(bad) => {
                            out.push(char::REPLACEMENT_CHARACTER);
                            rest = &after[bad..];
                        }
                        // The rest of this character is in the token that has not been decoded yet.
                        None => break,
                    }
                }
            }
        }
        out
    }
}

/// The pre-tokenizer, as an ordered scan. Returns the pieces in order; their concatenation is the
/// input.
pub fn pre_tokenize(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut pieces = Vec::new();
    let mut at = 0usize;
    while at < bytes.len() {
        let end = match_one(text, at);
        // A position that matches nothing cannot happen — the last two alternatives cover every
        // whitespace and the fourth covers every other non-letter, non-digit — but advancing by a
        // character rather than looping forever is the safe reading if a future pattern change
        // makes it possible.
        let end = if end > at { end } else { at + text[at..].chars().next().map(|c| c.len_utf8()).unwrap_or(1) };
        pieces.push(&text[at..end]);
        at = end;
    }
    pieces
}

/// The seven alternatives, in order. Returns the end offset of the first that matches at `at`, or
/// `at` if none does.
fn match_one(text: &str, at: usize) -> usize {
    let rest = &text[at..];
    let chars: Vec<char> = rest.chars().collect();
    let width = |n: usize| -> usize { chars[..n].iter().map(|c| c.len_utf8()).sum::<usize>() };

    // 1. `(?i:'s|'t|'re|'ve|'m|'ll|'d)`
    if chars.first() == Some(&'\'') {
        let lower: String = chars.iter().take(3).collect::<String>().to_lowercase();
        for suffix in ["'re", "'ve", "'ll", "'s", "'t", "'m", "'d"] {
            if lower.starts_with(suffix) {
                return at + width(suffix.chars().count());
            }
        }
    }
    // 2. `[^\r\n\p{L}\p{N}]?\p{L}+`
    {
        let mut i = 0;
        if let Some(c) = chars.first()
            && *c != '\r'
            && *c != '\n'
            && !is_letter(*c)
            && !is_number(*c)
        {
            i = 1;
        }
        let mut j = i;
        while j < chars.len() && is_letter(chars[j]) {
            j += 1;
        }
        if j > i {
            return at + width(j);
        }
    }
    // 3. `\p{N}` — exactly one
    if chars.first().is_some_and(|c| is_number(*c)) {
        return at + width(1);
    }
    // 4. ` ?[^\s\p{L}\p{N}]+[\r\n]*`
    {
        let i = usize::from(chars.first() == Some(&' '));
        let mut j = i;
        while j < chars.len() && !is_space(chars[j]) && !is_letter(chars[j]) && !is_number(chars[j]) {
            j += 1;
        }
        if j > i {
            while j < chars.len() && (chars[j] == '\r' || chars[j] == '\n') {
                j += 1;
            }
            return at + width(j);
        }
    }
    // 5. `\s*[\r\n]+`
    {
        let mut j = 0;
        while j < chars.len() && is_space(chars[j]) {
            j += 1;
        }
        // The `\s*` is greedy and then `[\r\n]+` must match, so back off to the last run of
        // newlines inside what the whitespace consumed.
        let mut k = j;
        while k > 0 && (chars[k - 1] == '\r' || chars[k - 1] == '\n') {
            k -= 1;
        }
        if k < j {
            return at + width(j);
        }
    }
    // 6. `\s+(?!\S)` — a whitespace run that is not followed by a non-space, i.e. one that runs to
    //    the end of the input. The negative lookahead is the reason this file exists.
    {
        let mut j = 0;
        while j < chars.len() && is_space(chars[j]) {
            j += 1;
        }
        if j > 0 && j == chars.len() {
            return at + width(j);
        }
        // `\s+` is greedy but backtracks: the longest prefix whose next character is a space
        // satisfies the lookahead, which is the run minus its last character.
        if j > 1 {
            return at + width(j - 1);
        }
    }
    // 7. `\s+`
    {
        let mut j = 0;
        while j < chars.len() && is_space(chars[j]) {
            j += 1;
        }
        if j > 0 {
            return at + width(j);
        }
    }
    at
}

/// Qwen2.5's chat template, rendered. `<|im_start|>role\ncontent<|im_end|>\n` per turn, then an
/// open assistant turn for the model to complete.
///
/// Reproduced rather than read from `tokenizer_config.json` because that field is a Jinja program
/// and running one to build a prompt is a larger surface than this runtime needs. A model whose
/// template differs needs its own renderer, and that is a per-class fact like every other —
/// [`crate::chat_template`] is where that fact is decided, and
/// [`crate::chat_template::qwen35_chat_prompt`] is the renderer the QWEN36 lane's reasoning model
/// needed. This function stays exactly what it is: Qwen2.5's template, and the dense tier's.
pub fn qwen_chat_prompt(system: Option<&str>, turns: &[(&str, &str)]) -> String {
    let mut out = String::new();
    if let Some(system) = system {
        out.push_str("<|im_start|>system\n");
        out.push_str(system);
        out.push_str("<|im_end|>\n");
    }
    for (role, content) in turns {
        out.push_str("<|im_start|>");
        out.push_str(role);
        out.push('\n');
        out.push_str(content);
        out.push_str("<|im_end|>\n");
    }
    out.push_str("<|im_start|>assistant\n");
    out
}

/// **A byte-level tokenizer over a tiny vocabulary, for the tests that need one.**
///
/// Every one of the 256 byte characters is its own token and there are no merges, so any text is
/// representable and every id below 256 decodes to exactly one byte — which is what lets a test
/// assert about the SHAPE of an encoding (ordinary pieces vs a control id) without shipping a
/// 150,000-entry vocabulary into the test binary. The two control tokens are declared the way a
/// GGUF declares them (`token_type` 3 = CONTROL), so they land in the added-token table and
/// `encode` matches them on raw text exactly as it does for the real Qwen files.
///
/// The table is [`FIXTURE_VOCAB`] entries wide and not the 258 the two control tokens would make
/// it: a fixture ENGINE's logit row is its class's `vocab_size` wide and its argmax can select any
/// id in it, so a tokenizer narrower than the class could not spell its own model's answer. The
/// two filler ids are declared USER_DEFINED, which is what a GGUF calls a token matched whole that
/// is not a control.
///
/// Placed here, immediately before the test module, so the float-free guard's scan of this file
/// still ends where it always did.
#[cfg(test)]
pub(crate) const FIXTURE_VOCAB: u32 = 260;

#[cfg(test)]
pub(crate) fn byte_level_fixture_v1() -> (QwenTokenizer, u32, u32) {
    let table = byte_to_char_table();
    let mut tokens: Vec<String> = table.iter().map(|c| c.to_string()).collect();
    let mut types = vec![1i64; tokens.len()];
    let im_start = tokens.len() as u32;
    tokens.push("<|im_start|>".to_string());
    types.push(3);
    let im_end = tokens.len() as u32;
    tokens.push("<|im_end|>".to_string());
    types.push(3);
    while (tokens.len() as u32) < FIXTURE_VOCAB {
        tokens.push(format!("<|fixture_{}|>", tokens.len()));
        types.push(4);
    }
    let tokenizer = QwenTokenizer::from_gguf(&tokens, &[], &types).expect("the byte-level fixture builds");
    (tokenizer, im_start, im_end)
}

#[cfg(test)]
mod tests {
    /// **A kanji that straddles two tokens must not reach the user as a replacement character.**
    ///
    /// The streaming server decodes the whole run each step and emits the new suffix. When the run
    /// ends mid-character, the tail has to be HELD, not substituted: substituting emits `U+FFFD`
    /// that the next step then un-says, which the client has already been told. Measured against
    /// the live engine on 2026-09-04: `申し訳ありません` arrived as `申し?ありません` over the
    /// stream while the same run's non-streaming answer was correct.
    ///
    /// This test needs no vocabulary: it exercises the byte-to-string half directly, which is
    /// where the decision lives.
    #[test]
    fn an_incomplete_trailing_character_is_held_back_and_real_garbage_is_not() {
        fn decode_tail(bytes: &[u8]) -> String {
            // The same walk `decode_lossy_tail` performs once the ids are bytes.
            let mut out = String::with_capacity(bytes.len());
            let mut rest = bytes;
            loop {
                match std::str::from_utf8(rest) {
                    Ok(valid) => {
                        out.push_str(valid);
                        break;
                    }
                    Err(e) => {
                        let (valid, after) = rest.split_at(e.valid_up_to());
                        out.push_str(std::str::from_utf8(valid).expect("valid prefix"));
                        match e.error_len() {
                            Some(bad) => {
                                out.push(char::REPLACEMENT_CHARACTER);
                                rest = &after[bad..];
                            }
                            None => break,
                        }
                    }
                }
            }
            out
        }

        let whole = "申し訳".as_bytes();
        // Cut inside the last character: two of its three bytes have arrived.
        let cut = &whole[..whole.len() - 1];
        assert_eq!(decode_tail(cut), "申し", "the half-arrived kanji is held, not replaced");
        assert_eq!(decode_tail(whole), "申し訳", "and completes when its last byte arrives");

        // Monotonic, which is what makes emitting `&decoded[shown..]` safe: every step's output is
        // a prefix of the next. The old lossy decode broke this — it grew by three bytes and then
        // shrank again — so a caller slicing at a remembered offset could land mid-character.
        assert!(decode_tail(whole).starts_with(&decode_tail(cut)));

        // A byte that can never begin or continue a character is marked and stepped over, because
        // a stream that waits for its continuation waits forever.
        assert_eq!(decode_tail(&[0xE7, 0x94, 0xB3, 0xFF, 0x41]), "申\u{fffd}A");
    }

    use super::*;

    /// **ADR-0077 Decision 6: user text can never smuggle a control token.**
    ///
    /// The same twelve characters, through the two entry points: `encode` matches the added token
    /// on raw text and emits the control id — correct for a rendered template, and the reason the
    /// segment-wise arm exists — while `encode_without_specials` never consults the table and
    /// emits ordinary byte pieces. If this ever produced the control id, a user who typed
    /// `<|im_start|>` would be speaking in the template's voice.
    #[test]
    fn user_text_spelling_a_control_token_encodes_to_ordinary_pieces() {
        let (tokenizer, im_start, im_end) = byte_level_fixture_v1();
        let literal = "<|im_start|>";

        assert_eq!(tokenizer.encode(literal).expect("the added-token path encodes"), vec![im_start]);

        let ordinary = tokenizer.encode_without_specials(literal).expect("the specials-disabled path encodes");
        assert_eq!(ordinary.len(), literal.len(), "with no merges every byte is its own piece");
        assert!(!ordinary.contains(&im_start), "the specials-disabled path emitted the control id it exists to refuse");
        assert!(!ordinary.contains(&im_end));
        // And it is a real encoding, not an escape: it decodes back to what the user typed.
        assert_eq!(tokenizer.decode(&ordinary).expect("byte pieces decode"), literal);
    }

    /// **Per-token bytes: the concatenation over a run is the answer, and an id with no spelling
    /// is skipped rather than fatal.**
    ///
    /// The property a streamed answer rests on (ADR-0077 Decision 2). The last assertion is the
    /// one that matters: a class's registered `vocab_size` is padded past the tokenizer's table,
    /// the engine's argmax can select there, and `decode` refuses such a run outright — which
    /// would throw away a completed inference over a token that has no spelling.
    #[test]
    fn a_tokens_bytes_concatenate_to_the_answer_and_a_padded_id_renders_nothing() {
        let (tokenizer, im_start, _) = byte_level_fixture_v1();
        let ids = tokenizer.encode("hi <|im_start|>").expect("the fixture encodes");
        let joined: Vec<u8> = ids.iter().filter_map(|id| tokenizer.token_bytes(*id)).flatten().collect();
        assert_eq!(joined, b"hi <|im_start|>".to_vec(), "the pieces are the whole text");
        assert_eq!(tokenizer.token_bytes(im_start), Some(b"<|im_start|>".to_vec()), "an added token spells itself");

        // A multi-byte character straddles tokens, and no single piece is valid UTF-8 on its own.
        let kanji = tokenizer.encode("字").expect("the fixture encodes");
        assert_eq!(kanji.len(), 3, "no merges: one piece per byte");
        assert!(kanji.iter().all(|id| tokenizer.token_bytes(*id).map(|b| b.len()) == Some(1)));
        assert_eq!(kanji.iter().filter_map(|id| tokenizer.token_bytes(*id)).flatten().collect::<Vec<u8>>(), "字".as_bytes());

        // Past the table: `None`, not an error, and `decode` shows why that distinction exists.
        assert_eq!(tokenizer.token_bytes(FIXTURE_VOCAB), None);
        assert!(tokenizer.decode(&[FIXTURE_VOCAB]).is_err(), "the strict decoder refuses a whole run for one such id");
    }

    /// **The no-specials path checks its own output, and here is the vocabulary that needs it.**
    ///
    /// Decision 6 rests on "ordinary encoding cannot produce a control id". Disabling the added-
    /// token MATCH does not establish that: `encode_ordinary` resolves its pieces against the same
    /// vocabulary the added tokens live in, so an added token the pre-tokenizer does not split
    /// comes straight back out. Today's Qwen files spell their markers `<|…|>` and the pattern
    /// splits those — but a GGUF may declare any token `USER_DEFINED`, including a one-character
    /// one, and then the pattern splits nothing. This fixture declares exactly that, and the
    /// promise holds because it is checked rather than inferred.
    #[test]
    fn ordinary_text_that_resolves_to_an_added_token_is_refused() {
        let table = byte_to_char_table();
        let tokens: Vec<String> = table.iter().map(|c| c.to_string()).collect();
        let mut types = vec![1i64; tokens.len()];
        types[b'x' as usize] = 4;
        let tokenizer = QwenTokenizer::from_gguf(&tokens, &[], &types).expect("the fixture builds");
        assert!(tokenizer.is_added_id(b'x' as u32), "the fixture declares an ordinary-looking added token");

        assert!(tokenizer.encode_without_specials("ab").is_ok(), "text that touches none of it still encodes");
        let err = tokenizer.encode_without_specials("axb").expect_err("prose reached an added token and was refused");
        assert!(matches!(err, TokenizerError::ControlTokenFromText(id) if id == b'x' as u32), "{err}");
        // The plain path still emits it: that is its job, and it is the reason the two are named
        // apart rather than one function with a flag nobody reads at the call site.
        assert_eq!(tokenizer.encode("axb").expect("the plain path encodes"), vec![b'a' as u32, b'x' as u32, b'b' as u32]);
    }

    /// ADR-0079 SA-7: an error a prompt can cause must not carry the prompt.
    #[test]
    fn the_error_kind_carries_no_text_of_the_input() {
        // A piece with no id — the only tokenizer error a stranger's text can provoke.
        let (tokenizer, _, _) = byte_level_fixture_v1();
        let err = TokenizerError::Unrepresentable("SECRET".to_string());
        assert!(format!("{}", err).contains("SECRET"), "Display is the converter's form and keeps the piece");
        assert!(!err.kind().contains("SECRET"), "kind() is the form a worker may log: {}", err.kind());
        // And the redacted form still says which of the three things went wrong.
        assert_ne!(TokenizerError::NotUtf8.kind(), err.kind());
        assert_ne!(TokenizerError::Malformed("x").kind(), err.kind());
        let _ = tokenizer;
    }

    /// The added-token table is what a manifest publishes by NAME, so a gateway never guesses an
    /// id. Both control tokens must be in it, with the ids the fixture assigned.
    #[test]
    fn the_added_token_table_names_every_control_token() {
        let (tokenizer, im_start, im_end) = byte_level_fixture_v1();
        let named: Vec<(String, u32)> = tokenizer.added_tokens().iter().map(|a| (a.content.clone(), a.id)).collect();
        assert!(named.contains(&("<|im_start|>".to_string(), im_start)), "{named:?}");
        assert!(named.contains(&("<|im_end|>".to_string(), im_end)), "{named:?}");
        // Only the declared CONTROL and USER_DEFINED tokens are added tokens; the 256 byte pieces
        // are ordinary BPE and must never be matched whole on raw text.
        assert_eq!(named.len(), (FIXTURE_VOCAB - 256) as usize, "{named:?}");
        assert!(named.iter().all(|(name, _)| name.starts_with("<|")), "a byte piece leaked into the added table: {named:?}");
    }

    /// The byte table is GPT-2's, and the property that matters is that it is a bijection: every
    /// byte has a character and no two bytes share one, or decoding is ambiguous.
    #[test]
    fn the_byte_table_is_a_bijection() {
        let table = byte_to_char_table();
        let mut seen = std::collections::HashSet::new();
        for c in table {
            assert!(seen.insert(c), "the byte table repeats {c:?}");
        }
        assert_eq!(seen.len(), 256);
        // The printable ASCII range maps to itself, which is what makes a token like `Hello`
        // readable in the vocabulary file.
        assert_eq!(table[b'H' as usize], 'H');
        // Space is one of the holes, and GPT-2 fills it with U+0120 — the `Ġ` that prefixes every
        // word-initial token in the file.
        assert_eq!(table[b' ' as usize], 'Ġ');
    }

    /// The pre-tokenizer's pieces must concatenate back to the input, for every input. Anything
    /// else means a character was dropped or duplicated on the way to the model.
    #[test]
    fn pre_tokenization_is_a_partition() {
        for text in [
            "Hello, world!",
            "  leading and trailing  ",
            "日本語のテキストです。",
            "mixed 123 numbers and ABC",
            "line\nbreak\r\nand more",
            "it's a contraction, isn't it",
            "\n\n\n",
            "     ",
            "",
            "emoji 🙂 and ﬀ ligature",
            "Ⅷ is a roman numeral",
        ] {
            let pieces = pre_tokenize(text);
            assert_eq!(pieces.concat(), text, "pieces must partition {text:?}, got {pieces:?}");
        }
    }

    /// The specific splits the pattern promises, spelled out. These are what a wrong alternative
    /// order would change.
    #[test]
    fn the_pattern_splits_where_it_says_it_does() {
        assert_eq!(pre_tokenize("Hello world"), vec!["Hello", " world"]);
        // Digits are one piece each — `\p{N}` matches exactly one.
        assert_eq!(pre_tokenize("a123"), vec!["a", "1", "2", "3"]);
        // A contraction is its own piece, case-insensitively.
        assert_eq!(pre_tokenize("it's"), vec!["it", "'s"]);
        assert_eq!(pre_tokenize("IT'S"), vec!["IT", "'S"]);
        // A run of spaces before a word gives the word its leading space and leaves the rest.
        assert_eq!(pre_tokenize("a  b"), vec!["a", " ", " b"]);
        // Trailing whitespace is its own piece (the lookahead branch).
        assert_eq!(pre_tokenize("a  "), vec!["a", "  "]);
        // A roman numeral is Nl: `\p{L}` is false and `\p{N}` is true, so it takes the number
        // branch. `char::is_alphabetic` would have put it in the letter branch.
        assert_eq!(pre_tokenize("Ⅷ"), vec!["Ⅷ"]);
        assert_eq!(pre_tokenize("aⅧ"), vec!["a", "Ⅷ"]);
    }

    /// The chat template is a string contract; a change to it re-tokenizes every prompt.
    #[test]
    fn the_chat_template_renders_qwen_turns() {
        let prompt = qwen_chat_prompt(Some("You are helpful."), &[("user", "Hi")]);
        assert_eq!(prompt, "<|im_start|>system\nYou are helpful.<|im_end|>\n<|im_start|>user\nHi<|im_end|>\n<|im_start|>assistant\n");
        assert_eq!(qwen_chat_prompt(None, &[]), "<|im_start|>assistant\n");
    }
}
