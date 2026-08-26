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
}

impl std::fmt::Display for TokenizerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(what) => write!(f, "tokenizer.json: {what}"),
            Self::Unrepresentable(p) => write!(f, "no token covers the piece {p:?}"),
            Self::NotUtf8 => write!(f, "decoded bytes are not UTF-8"),
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
        String::from_utf8_lossy(&bytes).into_owned()
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
/// template differs needs its own renderer, and that is a per-class fact like every other.
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

#[cfg(test)]
mod tests {
    use super::*;

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
