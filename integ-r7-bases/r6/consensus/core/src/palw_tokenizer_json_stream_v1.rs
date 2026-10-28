//! Bounded raw-JSON tokenizer derivation. All parser output is tentative until the EXACT
//! signed-length legacy tokenizer hash completes. Duplicate keys/ids use an auxiliary SMT;
//! it does not replace that raw-file hash or the token table's existing leaf hash.
use crate::Hash64;
use crate::palw_json_key_set_v1::{PALW_JSON_KEY_HASH_DOMAIN_V1, PalwJsonKeyInsertProofV1, PalwJsonKeySetV1, palw_json_key_hash_v1};
use crate::palw_rendering_stream_v1::LeafHashContinuationV1 as HashContinuation;
use crate::palw_tokenizer_rendering_v1::{PALW_TOKENIZER_RAW_DOMAIN_V1, palw_qwen_char_byte_v1};
use borsh::{BorshDeserialize, BorshSerialize};

pub const PALW_TOKENIZER_RENDERING_DIGEST_DOMAIN_V1: &[u8] = b"misaka-palw/tokenizer/rendering/v1";
pub const PALW_TOKENIZER_JSON_PART_BYTES_V1: usize = 65_536;
// serde_json's default remaining_depth=128 decrements BEFORE entering each container.
const MAX_DEPTH: usize = 127;
const ORDINARY_ID_NAMESPACE: u64 = u64::MAX;
const ADDED_ID_NAMESPACE: u64 = u64::MAX - 1;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTokenizerRenderingDigestV1 {
    pub length: u64,
    pub digest: Hash64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwTokenizerJsonVerdictV1 {
    Invalid,
    WorkBoundExceeded,
    Rendering(Option<PalwTokenizerRenderingDigestV1>),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
enum Scope {
    Root,
    Model,
    Vocab,
    Merges,
    MergePair,
    AddedList,
    AddedEntry,
    Ignore,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
enum Field {
    Ignore,
    Model,
    Vocab,
    Merges,
    Added,
    Id,
    Content,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct Frame {
    object: bool,
    scope: Scope,
    namespace: u64,
    // Objects:0 key-or-end,1 key-after-comma,2 colon,3 value,4 comma-or-end.
    // Arrays:0 value-or-end,1 value-after-comma,4 comma-or-end.
    phase: u8,
    field: Field,
    flags: u8,
    items: u64,
    candidate: Option<PalwTokenizerRenderingDigestV1>,
    added_id: Option<u32>,
    added_content: Option<PalwTokenizerRenderingDigestV1>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct StringState {
    key: bool,
    word: Vec<u8>,
    long_word: bool,
    utf8: Vec<u8>,
    //0 ordinary,1 after slash,2 first unicode escape,3 expect low slash,4 expect low u,5 low escape.
    escape: u8,
    hex: u16,
    hex_digits: u8,
    high: u16,
    key_hash: HashContinuation,
    ordinary: HashContinuation,
    literal: HashContinuation,
    ordinary_len: u64,
    literal_len: u64,
    non_byte: bool,
    space: bool,
}
impl StringState {
    fn new(key: bool) -> Self {
        Self {
            key,
            word: Vec::new(),
            long_word: false,
            utf8: Vec::new(),
            escape: 0,
            hex: 0,
            hex_digits: 0,
            high: 0,
            key_hash: HashContinuation::new(PALW_JSON_KEY_HASH_DOMAIN_V1),
            ordinary: HashContinuation::new(PALW_TOKENIZER_RENDERING_DIGEST_DOMAIN_V1),
            literal: HashContinuation::new(PALW_TOKENIZER_RENDERING_DIGEST_DOMAIN_V1),
            ordinary_len: 0,
            literal_len: 0,
            non_byte: false,
            space: false,
        }
    }
    fn character(&mut self, c: char) -> Result<(), ()> {
        let mut buf = [0u8; 4];
        let bytes = c.encode_utf8(&mut buf).as_bytes();
        self.key_hash.update(bytes);
        self.literal.update(bytes);
        self.literal_len = self.literal_len.checked_add(bytes.len() as u64).ok_or(())?;
        if !self.long_word {
            if self.word.len() + bytes.len() <= 16 {
                self.word.extend_from_slice(bytes);
            } else {
                self.long_word = true;
                self.word.clear();
            }
        }
        self.space |= c == ' ';
        if let Some(b) = palw_qwen_char_byte_v1(c) {
            self.ordinary.update(&[b]);
            self.ordinary_len = self.ordinary_len.checked_add(1).ok_or(())?;
        } else {
            self.non_byte = true;
        }
        Ok(())
    }
    // Returns true when the closing quote was consumed. No decoded string is retained.
    fn byte(&mut self, b: u8) -> Result<bool, ()> {
        if self.escape == 2 || self.escape == 5 {
            let nibble = match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                b'A'..=b'F' => b - b'A' + 10,
                _ => return Err(()),
            };
            self.hex = self.hex * 16 + u16::from(nibble);
            self.hex_digits += 1;
            if self.hex_digits == 4 {
                let value = self.hex;
                self.hex = 0;
                self.hex_digits = 0;
                if self.escape == 5 {
                    if !(0xDC00..=0xDFFF).contains(&value) {
                        return Err(());
                    }
                    let c = 0x10000 + ((u32::from(self.high) - 0xD800) << 10) + (u32::from(value) - 0xDC00);
                    self.escape = 0;
                    self.high = 0;
                    self.character(char::from_u32(c).ok_or(())?)?;
                } else if (0xD800..=0xDBFF).contains(&value) {
                    self.high = value;
                    self.escape = 3;
                } else if (0xDC00..=0xDFFF).contains(&value) {
                    return Err(());
                } else {
                    self.escape = 0;
                    self.character(char::from_u32(u32::from(value)).ok_or(())?)?;
                }
            }
            return Ok(false);
        }
        if self.escape == 3 {
            if b != b'\\' {
                return Err(());
            }
            self.escape = 4;
            return Ok(false);
        }
        if self.escape == 4 {
            if b != b'u' {
                return Err(());
            }
            self.escape = 5;
            return Ok(false);
        }
        if self.escape == 1 {
            self.escape = 0;
            let c = match b {
                b'"' => '"',
                b'\\' => '\\',
                b'/' => '/',
                b'b' => '\u{8}',
                b'f' => '\u{C}',
                b'n' => '\n',
                b'r' => '\r',
                b't' => '\t',
                b'u' => {
                    self.escape = 2;
                    return Ok(false);
                }
                _ => return Err(()),
            };
            self.character(c)?;
            return Ok(false);
        }
        if !self.utf8.is_empty() {
            if b & 0xC0 != 0x80 {
                return Err(());
            }
            self.utf8.push(b);
            let expected = match self.utf8[0] {
                0xC2..=0xDF => 2,
                0xE0..=0xEF => 3,
                0xF0..=0xF4 => 4,
                _ => return Err(()),
            };
            if self.utf8.len() == expected {
                let c = std::str::from_utf8(&self.utf8).map_err(|_| ())?.chars().next().ok_or(())?;
                self.utf8.clear();
                self.character(c)?;
            }
            return Ok(false);
        }
        match b {
            b'"' => Ok(true),
            b'\\' => {
                self.escape = 1;
                Ok(false)
            }
            0..=31 => Err(()),
            32..=127 => {
                self.character(char::from(b))?;
                Ok(false)
            }
            0xC2..=0xF4 => {
                self.utf8.push(b);
                Ok(false)
            }
            _ => Err(()),
        }
    }
    fn ordinary_digest(&self) -> Option<PalwTokenizerRenderingDigestV1> {
        (!self.non_byte).then(|| PalwTokenizerRenderingDigestV1 { length: self.ordinary_len, digest: self.ordinary.finish() })
    }
    fn literal_digest(&self) -> PalwTokenizerRenderingDigestV1 {
        PalwTokenizerRenderingDigestV1 { length: self.literal_len, digest: self.literal.finish() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) struct NumberState {
    //0 sign,1 zero,2 integer,3 dot,4 fraction,5 exponent marker,6 exponent sign,7 exponent digits.
    phase: u8,
    positive: bool,
    value: Option<u32>,
    float: bool,
    integer_digits: u64,
    leading_zeros: u64,
    significant: Vec<u8>,
    seen_nonzero: bool,
    exponent: u64,
    exponent_positive: bool,
}
impl NumberState {
    pub(crate) fn new(b: u8) -> Self {
        let mut s = Self {
            phase: if b == b'-' {
                0
            } else if b == b'0' {
                1
            } else {
                2
            },
            positive: b != b'-',
            value: Some(0),
            float: false,
            integer_digits: 0,
            leading_zeros: 0,
            significant: Vec::new(),
            seen_nonzero: false,
            exponent: 0,
            exponent_positive: true,
        };
        if b.is_ascii_digit() {
            s.digit(b, false).expect("the first digit has bounded counters");
        }
        s
    }
    fn digit(&mut self, b: u8, fraction: bool) -> Result<(), ()> {
        if !fraction {
            self.integer_digits = self.integer_digits.checked_add(1).ok_or(())?;
            self.value = self.value.and_then(|n| n.checked_mul(10).and_then(|n| n.checked_add(u32::from(b - b'0'))));
        }
        if !self.seen_nonzero && b == b'0' {
            self.leading_zeros = self.leading_zeros.checked_add(1).ok_or(())?;
        } else {
            self.seen_nonzero = true;
            if self.significant.len() < 309 {
                self.significant.push(b);
            }
        }
        Ok(())
    }
    // false: caller must reprocess the delimiter after completing this number.
    pub(crate) fn byte(&mut self, b: u8) -> Result<bool, ()> {
        match self.phase {
            0 => match b {
                b'0' => {
                    self.phase = 1;
                    self.digit(b, false)?;
                }
                b'1'..=b'9' => {
                    self.phase = 2;
                    self.digit(b, false)?;
                }
                _ => return Err(()),
            },
            1 | 2 => match b {
                b'0'..=b'9' => {
                    if self.phase == 1 {
                        return Err(());
                    }
                    self.digit(b, false)?;
                }
                b'.' => {
                    self.phase = 3;
                    self.float = true;
                }
                b'e' | b'E' => {
                    self.phase = 5;
                    self.float = true;
                }
                _ => return Ok(false),
            },
            3 => {
                if !b.is_ascii_digit() {
                    return Err(());
                }
                self.phase = 4;
                self.digit(b, true)?;
            }
            4 => match b {
                b'0'..=b'9' => self.digit(b, true)?,
                b'e' | b'E' => {
                    self.phase = 5;
                }
                _ => return Ok(false),
            },
            5 => match b {
                b'+' => self.phase = 6,
                b'-' => {
                    self.phase = 6;
                    self.exponent_positive = false;
                }
                b'0'..=b'9' => {
                    self.phase = 7;
                    self.exponent = u64::from(b - b'0');
                }
                _ => return Err(()),
            },
            6 => {
                if !b.is_ascii_digit() {
                    return Err(());
                }
                self.phase = 7;
                self.exponent = u64::from(b - b'0');
            }
            7 => {
                if !b.is_ascii_digit() {
                    return Ok(false);
                }
                self.exponent = self.exponent.saturating_mul(10).saturating_add(u64::from(b - b'0'));
            }
            _ => return Err(()),
        }
        Ok(true)
    }
    pub(crate) fn complete(&self) -> bool {
        matches!(self.phase, 1 | 2 | 4 | 7)
    }
    pub(crate) fn id(&self) -> Option<u32> {
        if self.complete() && self.positive && !self.float { self.value } else { None }
    }
    pub(crate) fn finite(&self) -> bool {
        if !self.complete() {
            return false;
        }
        if !self.seen_nonzero {
            return true;
        }
        // Exact decimal comparison to2^1024-2^970, the first magnitude that rounds to
        // infinity under nearest/even IEEE754. No float arithmetic or library feature enters
        // this decision. Only309 significant digits and bounded counters are retained.
        const OVERFLOW_MIDPOINT:&[u8]=b"179769313486231580793728971405303415079934132710037826936173778980444968292764750946649017977587207096330286416692887910946555547851940402630657488671505820681908902000708383676273854845817711531764475730270069855571366959622842914819860834936475292719074168444365510704342711559699508093042880177904174497792";
        let magnitude = i128::from(self.integer_digits) - i128::from(self.leading_zeros)
            + if self.exponent_positive { i128::from(self.exponent) } else { -i128::from(self.exponent) };
        if magnitude < 309 {
            return true;
        }
        if magnitude > 309 {
            return false;
        }
        for (index, bound) in OVERFLOW_MIDPOINT.iter().enumerate() {
            let digit = self.significant.get(index).copied().unwrap_or(b'0');
            if digit < *bound {
                return true;
            }
            if digit > *bound {
                return false;
            }
        }
        false
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
enum Lex {
    Idle,
    String(StringState),
    Number(NumberState),
    Keyword { kind: u8, index: u8 },
}

/// Host observers receive only tentative bounded digests. Uniqueness/schema and the
/// immutable exact-length flat raw hash must all authenticate at EOF before use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwTokenizerRenderingEventV1 {
    Ordinary { id: u32, rendering: Option<PalwTokenizerRenderingDigestV1> },
    Added { id: u32, rendering: PalwTokenizerRenderingDigestV1 },
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTokenizerJsonStreamV1 {
    context: Hash64,
    tokenizer: Hash64,
    length: u64,
    offset: u64,
    target: u32,
    hash: HashContinuation,
    stack: Vec<Frame>,
    lex: Lex,
    keys: PalwJsonKeySetV1,
    key_events: u64,
    work_overrun: bool,
    pending: Option<(u64, Hash64)>,
    next_object: u64,
    started: bool,
    root_done: bool,
    invalid: bool,
    sealed: bool,
    highest: u32,
    chosen: Option<Option<PalwTokenizerRenderingDigestV1>>,
    added_chosen: Option<PalwTokenizerRenderingDigestV1>,
}
impl PalwTokenizerJsonStreamV1 {
    pub fn new(context: Hash64, tokenizer: Hash64, exact_length: u64, target: u32) -> Self {
        let mut hash = HashContinuation::new(PALW_TOKENIZER_RAW_DOMAIN_V1);
        hash.update(&exact_length.to_le_bytes());
        Self {
            context,
            tokenizer,
            length: exact_length,
            offset: 0,
            target,
            hash,
            stack: Vec::new(),
            lex: Lex::Idle,
            keys: PalwJsonKeySetV1::default(),
            key_events: 0,
            work_overrun: false,
            pending: None,
            next_object: 0,
            started: false,
            root_done: false,
            invalid: false,
            sealed: false,
            highest: 0,
            chosen: None,
            added_chosen: None,
        }
    }
    pub fn offset(&self) -> u64 {
        self.offset
    }
    /// Public progress needs to distinguish raw EOF from its authenticated seal.
    /// This exposes only the phase, never a tentative parser verdict.
    pub fn is_sealed_v1(&self) -> bool {
        self.sealed
    }
    pub fn pending_key(&self) -> Option<(u64, Hash64)> {
        self.pending
    }
    /// Identity already authenticated by native seal/finish; no caller flag.
    pub(crate) fn sealed_artifact_binding_v1(&self)->Result<(Hash64,u64),String>{
        self.check()?;
        if !self.sealed||self.offset!=self.length||self.pending.is_some()||self.hash.finish()!=self.tokenizer{return Err("parser artifact binding before authenticated exact EOF".into());}
        Ok((self.tokenizer,self.length))
    }
    pub fn key_set_root(&self) -> Hash64 {
        self.keys.root()
    }
    pub fn key_events(&self) -> u64 {
        self.key_events
    }
    /// Switch to raw-hash-only exactly at the first parser-derived event over the signed
    /// bound. The outer seed must supply the immutable claim's own bound. An unproved event
    /// is not a duplicate-key claim; its authenticated raw grammar alone refutes the work bound.
    pub fn mark_work_overrun(&mut self, context: Hash64, signed_bound: u64) -> Result<(), String> {
        self.check()?;
        if context != self.context || self.pending.is_none() || self.invalid || self.sealed || self.key_events != signed_bound {
            return Err("work overrun needs the exact next parser event after the signed bound".into());
        }
        self.work_overrun = true;
        self.invalid = true;
        self.pending = None;
        self.stack.clear();
        self.lex = Lex::Idle;
        Ok(())
    }
    pub fn key_insertions(&self) -> u64 {
        self.keys.insertions()
    }
    pub fn check(&self) -> Result<(), String> {
        if self.stack.len() > MAX_DEPTH
            || self.offset > self.length
            || self.next_object >= ADDED_ID_NAMESPACE
            || self.key_events > self.offset
            || self.keys.insertions() > self.key_events
            || (self.work_overrun && (!self.invalid || self.pending.is_some()))
            || self.hash.byte_count() != Some(128 + 8 + u128::from(self.offset))
        {
            return Err("tokenizer JSON continuation has inconsistent bounded counters".into());
        }
        if self.stack.iter().any(|f| !matches!(f.phase, 0..=4) || f.namespace >= ADDED_ID_NAMESPACE) {
            return Err("tokenizer JSON stack is not bounded canonical parser state".into());
        }
        if let Lex::Number(n) = &self.lex
            && n.significant.len() > 309
        {
            return Err("tokenizer number continuation is unbounded".into());
        }
        if let Lex::String(s) = &self.lex
            && (s.word.len() > 16
                || s.utf8.len() > 4
                || s.escape > 5
                || s.hex_digits > 3
                || s.key_hash.byte_count().is_none()
                || s.ordinary.byte_count().is_none()
                || s.literal.byte_count().is_none())
        {
            return Err("tokenizer JSON string continuation exceeds bounded state".into());
        }
        Ok(())
    }
    /// One absence OR membership proof for the exact parser-derived key. A genuine duplicate
    /// marks tentative Invalid and continues hashing; forged proofs never mutate the cursor.
    pub fn prove_key(&mut self, context: Hash64, proof: &PalwJsonKeyInsertProofV1) -> Result<(), String> {
        self.check()?;
        if context != self.context {
            return Err("tokenizer key proof changes context".into());
        }
        let (namespace, key) = self.pending.ok_or("tokenizer JSON does not await a key proof")?;
        let events = self.key_events.checked_add(1).ok_or("the tokenizer event count overflows")?;
        if self.keys.contains(namespace, key, proof)? {
            self.invalid = true;
        } else {
            self.keys.insert(namespace, key, proof)?;
        }
        self.pending = None;
        self.key_events = events;
        Ok(())
    }
    /// Consume at most65KiB, pausing at a parser-derived duplicate-key/id proof. The caller
    /// carries only the unconsumed suffix again, with this exact offset. No skipped/replayed raw
    /// bytes can enter the flat tokenizer hash. All fields are owned by the surrounding court.
    pub fn push(&mut self, context: Hash64, offset: u64, bytes: &[u8]) -> Result<usize, String> {
        self.push_with_observer_v1(context, offset, bytes, |_| {})
    }
    /// Same consensus parser and proof pauses, with transient host notifications. No events
    /// or extra state are serialized. Added-token overrides may precede ordinary vocab.
    pub fn push_with_observer_v1(
        &mut self,
        context: Hash64,
        offset: u64,
        bytes: &[u8],
        mut observer: impl FnMut(PalwTokenizerRenderingEventV1),
    ) -> Result<usize, String> {
        self.check()?;
        if context != self.context
            || offset != self.offset
            || bytes.is_empty()
            || bytes.len() > PALW_TOKENIZER_JSON_PART_BYTES_V1
            || bytes.len() as u64 > self.length - self.offset
            || self.sealed
            || self.pending.is_some()
        {
            return Err("tokenizer JSON part changes context/offset or exceeds the bounded remaining input".into());
        }
        let mut consumed = 0;
        while consumed < bytes.len() {
            if self.invalid {
                consumed = bytes.len();
                break;
            }
            let b = bytes[consumed];
            if self.parse_byte(b, &mut observer).is_err() {
                self.invalid = true;
            }
            consumed += 1;
            if self.pending.is_some() {
                break;
            }
        }
        self.hash.update(&bytes[..consumed]);
        self.offset += consumed as u64;
        self.check()?;
        Ok(consumed)
    }
    /// The qualified document is a root object. Any live lexeme/container at exact EOF
    /// is invalid JSON, so sealing must not demand a new irrelevant uniqueness proof.
    /// The original exact-length flat hash still authenticates before Invalid is usable.
    pub fn seal(&mut self) -> Result<(), String> {
        self.check()?;
        if self.offset != self.length || self.pending.is_some() {
            return Err("tokenizer raw input/key proofs are incomplete".into());
        }
        if self.sealed {
            return Ok(());
        }
        if !self.invalid && (!matches!(self.lex, Lex::Idle) || !self.root_done || !self.stack.is_empty()) {
            self.invalid = true;
        }
        self.sealed = true;
        Ok(())
    }
    pub fn finish(&self) -> Result<PalwTokenizerJsonVerdictV1, String> {
        self.check()?;
        if !self.sealed || self.offset != self.length || self.pending.is_some() {
            return Err("tokenizer JSON is not sealed".into());
        }
        if self.hash.finish() != self.tokenizer {
            return Err("tokenizer bytes do not open the immutable exact-length raw hash".into());
        }
        if self.work_overrun {
            return Ok(PalwTokenizerJsonVerdictV1::WorkBoundExceeded);
        }
        if self.invalid {
            return Ok(PalwTokenizerJsonVerdictV1::Invalid);
        }
        let rendering = if let Some(added) = &self.added_chosen {
            Some(added.clone())
        } else if self.target > self.highest {
            None
        } else {
            self.chosen.clone().unwrap_or_else(|| {
                Some(PalwTokenizerRenderingDigestV1 {
                    length: 0,
                    digest: HashContinuation::new(PALW_TOKENIZER_RENDERING_DIGEST_DOMAIN_V1).finish(),
                })
            })
        };
        Ok(PalwTokenizerJsonVerdictV1::Rendering(rendering))
    }
    fn parse_byte(&mut self, b: u8, observer: &mut dyn FnMut(PalwTokenizerRenderingEventV1)) -> Result<(), ()> {
        let lex = std::mem::replace(&mut self.lex, Lex::Idle);
        match lex {
            Lex::Idle => self.idle(b, observer),
            Lex::String(mut s) => {
                if s.byte(b)? {
                    self.string_end(s)
                } else {
                    self.lex = Lex::String(s);
                    Ok(())
                }
            }
            Lex::Number(mut n) => {
                if n.byte(b)? {
                    self.lex = Lex::Number(n);
                    Ok(())
                } else {
                    self.number_end(n, observer)?;
                    self.idle(b, observer)
                }
            }
            Lex::Keyword { kind, index } => {
                let word = match kind {
                    0 => b"true".as_slice(),
                    1 => b"false".as_slice(),
                    _ => b"null".as_slice(),
                };
                if word.get(index as usize) != Some(&b) {
                    return Err(());
                }
                if index as usize + 1 < word.len() {
                    self.lex = Lex::Keyword { kind, index: index + 1 };
                }
                Ok(())
            }
        }
    }
    fn idle(&mut self, b: u8, observer: &mut dyn FnMut(PalwTokenizerRenderingEventV1)) -> Result<(), ()> {
        if matches!(b, b' ' | b'\n' | b'\r' | b'\t') {
            return Ok(());
        }
        let Some(last) = self.stack.last() else {
            if self.started || self.root_done {
                return Err(());
            }
            self.started = true;
            return self.value(b);
        };
        if last.object {
            match last.phase {
                0 | 1 => {
                    if b == b'"' {
                        self.lex = Lex::String(StringState::new(true));
                        Ok(())
                    } else if b == b'}' && last.phase == 0 {
                        self.close(true, observer)
                    } else {
                        Err(())
                    }
                }
                2 => {
                    if b != b':' {
                        return Err(());
                    }
                    self.stack.last_mut().unwrap().phase = 3;
                    Ok(())
                }
                3 => self.value(b),
                4 => match b {
                    b',' => {
                        self.stack.last_mut().unwrap().phase = 1;
                        Ok(())
                    }
                    b'}' => self.close(true, observer),
                    _ => Err(()),
                },
                _ => Err(()),
            }
        } else {
            match last.phase {
                0 | 1 => {
                    if b == b']' && last.phase == 0 {
                        self.close(false, observer)
                    } else {
                        self.value(b)
                    }
                }
                4 => match b {
                    b',' => {
                        self.stack.last_mut().unwrap().phase = 1;
                        Ok(())
                    }
                    b']' => self.close(false, observer),
                    _ => Err(()),
                },
                _ => Err(()),
            }
        }
    }
    fn value(&mut self, b: u8) -> Result<(), ()> {
        let mut scope = Scope::Ignore;
        if let Some(parent) = self.stack.last_mut() {
            if parent.object {
                scope = match parent.field {
                    Field::Model => {
                        if b != b'{' {
                            return Err(());
                        }
                        parent.flags |= 1;
                        Scope::Model
                    }
                    Field::Vocab => {
                        if b != b'{' {
                            return Err(());
                        }
                        parent.flags |= 1;
                        Scope::Vocab
                    }
                    Field::Merges => {
                        if b != b'[' {
                            return Err(());
                        }
                        parent.flags |= 2;
                        Scope::Merges
                    }
                    Field::Added => {
                        if b != b'[' {
                            return Err(());
                        }
                        Scope::AddedList
                    }
                    Field::Content => {
                        if b != b'"' {
                            return Err(());
                        }
                        Scope::Ignore
                    }
                    Field::Id => {
                        if !matches!(b, b'-' | b'0'..=b'9') {
                            return Err(());
                        }
                        Scope::Ignore
                    }
                    Field::Ignore => {
                        if parent.scope == Scope::Vocab && !matches!(b, b'-' | b'0'..=b'9') {
                            return Err(());
                        }
                        Scope::Ignore
                    }
                };
            } else {
                parent.items = parent.items.checked_add(1).ok_or(())?;
                match parent.scope {
                    Scope::Merges => {
                        if b == b'[' {
                            scope = Scope::MergePair;
                        } else if b != b'"' {
                            return Err(());
                        }
                    }
                    Scope::MergePair => {
                        if b != b'"' || parent.items > 2 {
                            return Err(());
                        }
                    }
                    Scope::AddedList => {
                        if b != b'{' {
                            return Err(());
                        }
                        scope = Scope::AddedEntry
                    }
                    _ => {}
                }
            }
            parent.phase = 4;
        } else {
            if b != b'{' {
                return Err(());
            }
            scope = Scope::Root;
        }
        match b {
            b'{' | b'[' => {
                if self.stack.len() >= MAX_DEPTH {
                    return Err(());
                }
                let namespace = if b == b'{' {
                    let n = self.next_object;
                    self.next_object = self.next_object.checked_add(1).filter(|n| *n < ADDED_ID_NAMESPACE).ok_or(())?;
                    n
                } else {
                    0
                };
                self.stack.push(Frame {
                    object: b == b'{',
                    scope,
                    namespace,
                    phase: 0,
                    field: Field::Ignore,
                    flags: 0,
                    items: 0,
                    candidate: None,
                    added_id: None,
                    added_content: None,
                });
                Ok(())
            }
            b'"' => {
                self.lex = Lex::String(StringState::new(false));
                Ok(())
            }
            b'-' | b'0'..=b'9' => {
                self.lex = Lex::Number(NumberState::new(b));
                Ok(())
            }
            b't' => {
                self.lex = Lex::Keyword { kind: 0, index: 1 };
                Ok(())
            }
            b'f' => {
                self.lex = Lex::Keyword { kind: 1, index: 1 };
                Ok(())
            }
            b'n' => {
                self.lex = Lex::Keyword { kind: 2, index: 1 };
                Ok(())
            }
            _ => Err(()),
        }
    }
    fn close(&mut self, object: bool, observer: &mut dyn FnMut(PalwTokenizerRenderingEventV1)) -> Result<(), ()> {
        let frame = self.stack.pop().ok_or(())?;
        if frame.object != object {
            return Err(());
        }
        match frame.scope {
            Scope::Root if frame.flags & 1 == 0 => return Err(()),
            Scope::Model if frame.flags & 3 != 3 => return Err(()),
            Scope::MergePair if frame.items != 2 => return Err(()),
            Scope::AddedEntry => {
                let id = frame.added_id.ok_or(())?;
                let content = frame.added_content.ok_or(())?;
                observer(PalwTokenizerRenderingEventV1::Added { id, rendering: content.clone() });
                if id == self.target {
                    self.added_chosen = Some(content);
                }
            }
            _ => {}
        }
        if self.stack.is_empty() {
            self.root_done = true;
        }
        Ok(())
    }
    fn string_end(&mut self, s: StringState) -> Result<(), ()> {
        let frame = self.stack.last_mut().ok_or(())?;
        if s.key {
            frame.phase = 2;
            frame.field = match (frame.scope, s.word.as_slice(), s.long_word) {
                (Scope::Root, b"model", false) => Field::Model,
                (Scope::Root, b"added_tokens", false) => Field::Added,
                (Scope::Model, b"vocab", false) => Field::Vocab,
                (Scope::Model, b"merges", false) => Field::Merges,
                (Scope::AddedEntry, b"id", false) => Field::Id,
                (Scope::AddedEntry, b"content", false) => Field::Content,
                _ => Field::Ignore,
            };
            if frame.scope == Scope::Vocab {
                frame.candidate = s.ordinary_digest();
            }
            self.pending = Some((frame.namespace, s.key_hash.finish()));
        } else if frame.scope == Scope::AddedEntry && frame.field == Field::Content {
            frame.added_content = Some(s.literal_digest());
        } else if frame.scope == Scope::Merges && !s.space {
            return Err(());
        }
        Ok(())
    }
    fn number_end(&mut self, n: NumberState, observer: &mut dyn FnMut(PalwTokenizerRenderingEventV1)) -> Result<(), ()> {
        if !n.finite() {
            return Err(());
        }
        let frame = self.stack.last_mut().ok_or(())?;
        if frame.scope == Scope::Vocab {
            let id = n.id().ok_or(())?;
            self.highest = self.highest.max(id);
            observer(PalwTokenizerRenderingEventV1::Ordinary { id, rendering: frame.candidate.clone() });
            if id == self.target {
                self.chosen = Some(frame.candidate.clone());
            }
            self.pending = Some((ORDINARY_ID_NAMESPACE, palw_json_key_hash_v1(&id.to_le_bytes())));
        } else if frame.scope == Scope::AddedEntry && frame.field == Field::Id {
            let id = n.id().ok_or(())?;
            frame.added_id = Some(id);
            self.highest = self.highest.max(id);
            self.pending = Some((ADDED_ID_NAMESPACE, palw_json_key_hash_v1(&id.to_le_bytes())));
        }
        Ok(())
    }
}

/// Every uniqueness-proof event consumes two distinct raw syntax bytes: the
/// quotes of an object key, or the colon and a numeric byte of its ID value.
/// ID namespaces are requested only for numeric object fields. No event can
/// reuse another event's pair. This bounds valid documents and invalid prefixes;
/// one retained slot for tiny/empty invalid operands is conservative.
pub fn palw_tokenizer_key_event_byte_bound_v1(raw_bytes: u64) -> u64 {
    (raw_bytes / 2).max(1)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::palw_tokenizer_rendering_v1::{PalwQwenRenderingMapV1, palw_tokenizer_raw_commitment_v1};
    use std::collections::BTreeMap;
    fn hash(key: &[u8], fields: &[&[u8]]) -> Hash64 {
        let mut h = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
        for f in fields {
            h.update(f);
        }
        Hash64::from_bytes(h.finalize().as_bytes().try_into().unwrap())
    }
    fn node(height: usize, left: Hash64, right: Hash64) -> Hash64 {
        hash(b"misaka-palw/json/object-key-node/v1", &[&(height as u16).to_le_bytes(), left.as_byte_slice(), right.as_byte_slice()])
    }
    fn prefix(path: Hash64, height: usize) -> [u8; 64] {
        let mut bytes: [u8; 64] = *path.as_byte_slice();
        for bit in 512 - height..512 {
            bytes[bit / 8] &= !(1 << (7 - bit % 8));
        }
        bytes
    }
    // Test-only proof book. Production may persist its sparse nodes to disk independently of
    // the court's constant-size set root; no challenger-supplied root enters the parser.
    pub(crate) struct Book {
        defaults: Vec<Hash64>,
        nodes: BTreeMap<(usize, [u8; 64]), Hash64>,
    }
    impl Book {
        pub(crate) fn new() -> Self {
            let mut defaults = vec![hash(b"misaka-palw/json/object-key-empty/v1", &[])];
            for h in 1..=512 {
                let last = defaults[h - 1];
                defaults.push(node(h, last, last));
            }
            Self { defaults, nodes: BTreeMap::new() }
        }
        pub(crate) fn proof(&self, namespace: u64, key_hash: Hash64) -> PalwJsonKeyInsertProofV1 {
            let path = crate::palw_json_key_set_v1::palw_json_key_path_v1(namespace, key_hash);
            let mut siblings = Vec::new();
            for h in 0..512 {
                let mut other = prefix(path, h);
                let bit = 511 - h;
                other[bit / 8] ^= 1 << (7 - bit % 8);
                siblings.push(self.nodes.get(&(h, other)).copied().unwrap_or(self.defaults[h]));
            }
            PalwJsonKeyInsertProofV1 { namespace, key_hash, siblings }
        }
        pub(crate) fn insert(&mut self, namespace: u64, key_hash: Hash64) {
            let path = crate::palw_json_key_set_v1::palw_json_key_path_v1(namespace, key_hash);
            self.nodes.insert((0, prefix(path, 0)), hash(b"misaka-palw/json/object-key-present/v1", &[path.as_byte_slice()]));
            for h in 1..=512 {
                let left = prefix(path, h);
                let mut right = left;
                let bit = 512 - h;
                right[bit / 8] |= 1 << (7 - bit % 8);
                let l = self.nodes.get(&(h - 1, left)).copied().unwrap_or(self.defaults[h - 1]);
                let r = self.nodes.get(&(h - 1, right)).copied().unwrap_or(self.defaults[h - 1]);
                self.nodes.insert((h, left), node(h, l, r));
            }
        }
        fn satisfy(&mut self, s: &mut PalwTokenizerJsonStreamV1) {
            let (namespace, key) = s.pending_key().unwrap();
            let proof = self.proof(namespace, key);
            let before = borsh::to_vec(s).unwrap();
            let mut forged = proof.clone();
            forged.siblings.pop();
            assert!(s.prove_key(s.context, &forged).is_err());
            assert_eq!(borsh::to_vec(s).unwrap(), before);
            s.prove_key(s.context, &proof).unwrap();
            self.insert(namespace, key);
            assert_eq!(s.key_set_root(), self.nodes.get(&(512, [0; 64])).copied().unwrap_or(self.defaults[512]));
        }
    }
    fn derive(raw: &[u8], id: u32, chunk: usize) -> PalwTokenizerJsonVerdictV1 {
        let context = Hash64::from_u64_word(18);
        let mut s = PalwTokenizerJsonStreamV1::new(context, palw_tokenizer_raw_commitment_v1(raw), raw.len() as u64, id);
        let mut book = Book::new();
        while s.offset() < raw.len() as u64 {
            if s.pending_key().is_some() {
                book.satisfy(&mut s);
                continue;
            }
            let offset = s.offset();
            let end = (offset as usize + chunk).min(raw.len());
            let before = borsh::to_vec(&s).unwrap();
            assert!(s.push(context, offset + 1, &raw[offset as usize..end]).is_err());
            assert_eq!(borsh::to_vec(&s).unwrap(), before);
            s.push(context, offset, &raw[offset as usize..end]).unwrap();
            let encoded = borsh::to_vec(&s).unwrap();
            assert!(encoded.len() < 64 * 1024);
            s = borsh::from_slice(&encoded).unwrap();
            assert!(s.finish().is_err());
        }
        loop {
            if s.pending_key().is_some() {
                book.satisfy(&mut s);
                continue;
            }
            match s.seal() {
                Ok(()) => break,
                Err(_) if s.pending_key().is_some() => continue,
                other => panic!("seal {other:?}"),
            }
        }
        s.finish().unwrap()
    }
    #[test]
    fn authenticated_byte_bound_covers_key_events_at_every_parser_pause() {
        let cases: &[&[u8]] = &[
            br#"{"model":{"type":"BPE","vocab":{"":0,"a":1,"b":2},"merges":[]}}"#,
            br#"{"added_tokens":[{"id":0,"content":"x"}],"model":{"type":"BPE","vocab":{"a":0},"merges":[]}}"#,
            br#"{"":{"":{"":0}},"model":{"type":"BPE","vocab":{"a":0},"merges":[]}}"#,
            br#"{"model":{"type":"BPE","vocab":{"a":0,"a":1}}}"#,
            br#"{"model":{"type":"BPE","vocab":{"a":0,"b":"#,
            br#"{"added_tokens":[{"id":0,"content":"x"},{"id":0}],"model":{}}"#,
            br#"{"model":{"type":"BPE","vocab":{"\\":0,"\u0061":1},"merges":[]}}"#,
            b"", b"0", b"{\"\"", b"{\"\":0}",
        ];
        for raw in cases {
            for chunk in [1, 2, 7, 65_536] {
                let context = Hash64::from_u64_word(0xB017);
                let mut stream = PalwTokenizerJsonStreamV1::new(
                    context, palw_tokenizer_raw_commitment_v1(raw), raw.len() as u64, 0,
                );
                let mut book = Book::new();
                loop {
                    if stream.pending_key().is_some() {
                        book.satisfy(&mut stream);
                        assert!(stream.key_events() <= stream.offset() / 2,
                            "distinct consumed syntax pairs: raw={raw:?}, chunk={chunk}");
                        assert!(stream.key_events() <= palw_tokenizer_key_event_byte_bound_v1(raw.len() as u64));
                    } else if stream.offset() < raw.len() as u64 {
                        let at = stream.offset();
                        let end = (at as usize + chunk).min(raw.len());
                        stream.push(context, at, &raw[at as usize..end]).unwrap();
                    } else {
                        stream.seal().unwrap();
                        stream.finish().unwrap();
                        break;
                    }
                }
            }
        }
    }

    fn expected(raw: &[u8], id: u32) -> PalwTokenizerJsonVerdictV1 {
        match PalwQwenRenderingMapV1::parse(raw) {
            Err(_) => PalwTokenizerJsonVerdictV1::Invalid,
            Ok(map) => PalwTokenizerJsonVerdictV1::Rendering(map.rendering(id).map(|bytes| {
                let mut h = HashContinuation::new(PALW_TOKENIZER_RENDERING_DIGEST_DOMAIN_V1);
                h.update(&bytes);
                PalwTokenizerRenderingDigestV1 { length: bytes.len() as u64, digest: h.finish() }
            })),
        }
    }
    #[test]
    fn observer_digest_events_are_tentative_and_do_not_change_serialized_parser_state() {
        let raw = br#"{"added_tokens":[{"id":0,"content":"override"}],"model":{"vocab":{"a":0,"\u6f22":2},"merges":[]}}"#;
        let context = Hash64::from_u64_word(18);
        let mut plain = PalwTokenizerJsonStreamV1::new(context, palw_tokenizer_raw_commitment_v1(raw), raw.len() as u64, 0);
        let mut observed = plain.clone();
        let mut book = Book::new();
        let mut events = vec![];
        while plain.offset() < raw.len() as u64 {
            if let Some((namespace, key)) = plain.pending_key() {
                let proof = book.proof(namespace, key);
                plain.prove_key(context, &proof).unwrap();
                observed.prove_key(context, &proof).unwrap();
                book.insert(namespace, key);
            } else {
                let offset = plain.offset();
                let end = (offset as usize + 7).min(raw.len());
                let n = plain.push(context, offset, &raw[offset as usize..end]).unwrap();
                let observed_n =
                    observed.push_with_observer_v1(context, offset, &raw[offset as usize..end], |e| events.push(e)).unwrap();
                assert_eq!(n, observed_n);
            }
            assert_eq!(borsh::to_vec(&plain).unwrap(), borsh::to_vec(&observed).unwrap());
        }
        plain.seal().unwrap();
        observed.seal().unwrap();
        assert_eq!(plain.finish(), observed.finish());
        assert_eq!(observed.finish().unwrap(), expected(raw, 0));
        assert_eq!(events.len(), 3);
        assert!(matches!(&events[0],PalwTokenizerRenderingEventV1::Added {id:0,rendering} if rendering.length==8));
        assert!(matches!(&events[1],PalwTokenizerRenderingEventV1::Ordinary {id:0,rendering:Some(r)} if r.length==1));
        assert!(matches!(&events[2], PalwTokenizerRenderingEventV1::Ordinary { id: 2, rendering: None }));
    }
    #[test]
    fn exact_eof_inside_vocab_number_seals_invalid_without_an_unreachable_new_proof() {
        let raw = br#"{"model":{"vocab":{"a":0"#;
        let context = Hash64::from_u64_word(18);
        for hash in [palw_tokenizer_raw_commitment_v1(raw), Hash64::from_u64_word(0xbad18)] {
            let mut s = PalwTokenizerJsonStreamV1::new(context, hash, raw.len() as u64, 0);
            let mut book = Book::new();
            while s.offset() < raw.len() as u64 {
                if s.pending_key().is_some() {
                    book.satisfy(&mut s);
                } else {
                    let offset = s.offset();
                    s.push(context, offset, &raw[offset as usize..(offset as usize + 1)]).unwrap();
                }
            }
            assert!(s.pending_key().is_none());
            assert!(!s.is_sealed_v1());
            s.seal().unwrap();
            assert!(s.is_sealed_v1());
            assert!(s.pending_key().is_none());
            if hash == palw_tokenizer_raw_commitment_v1(raw) {
                assert_eq!(s.finish(), Ok(PalwTokenizerJsonVerdictV1::Invalid));
            } else {
                assert!(s.finish().is_err());
            }
        }
    }
    #[test]
    fn required_schema_and_merge_types_match_the_whole_file_rule() {
        let cases: [&[u8]; 10] = [
            br#"{}"#,
            br#"{"model":{}}"#,
            br#"{"model":{"vocab":{}}}"#,
            br#"{"model":{"merges":[]}}"#,
            br#"{"model":{"vocab":{},"merges":["unspaced"]}}"#,
            br#"{"model":{"vocab":{},"merges":[["a"]]}}"#,
            br#"{"model":{"vocab":{},"merges":[["a",1]]}}"#,
            br#"{"model":{"vocab":{},"merges":[["a","b","c"]]}}"#,
            br#"{"model":{"vocab":{},"merges":[]}}"#,
            br#"{"model":{"vocab":{},"merges":["a b",["a","b"]]}}"#,
        ];
        for raw in cases {
            for chunk in [1, 7, 65_536] {
                assert_eq!(derive(raw, 0, chunk), expected(raw, 0), "{}", String::from_utf8_lossy(raw));
            }
        }
    }

    #[test]
    fn exact_json_grammar_and_renderer_agree_across_every_cursor_boundary_and_restart() {
        let samples:[&[u8];9]=[
            br#"{"added_tokens":[{"id":0,"content":"literal"}],"model":{"vocab":{"a":0},"merges":[]}}"#,
            br#"{"model":{"vocab":{"a":0,"b":2},"merges":["a b",["b","a"]]},"added_tokens":[{"content":"\uD83D\uDE00","id":2}],"junk":{"meta":[true,null,-1,1e-900]}}"#,
            br#"{"model":{"merges":[],"vocab":{"\u0100":0,"\u6f22":1}},"added_tokens":[{"id":3,"content":"\u0000\\\""}]}"#,
            br#"{"model":{"vocab":{"a":0,"b":0},"merges":[]}}"#,
            br#"{"model":{"vocab":{"a":0,"a":1},"merges":[]}}"#,
            br#"{"model":{"vocab":{"a":0},"merges":[]},"added_tokens":[{"id":0,"content":"a"},{"id":0,"content":"b"}]}"#,
            br#"{"model":{"vocab":{"a":0},"merges":[]},"junk":1e999}"#,
            br#"{"model":{"vocab":{"a":0},"merges":[]},"junk":"\uD800"}"#,
            br#"{"model":{"vocab":{"a":0},"merges":[]}} trailing"#,
        ];
        for raw in samples {
            for id in 0..5 {
                for chunk in [1, 7, 65_536] {
                    assert_eq!(derive(raw, id, chunk), expected(raw, id), "id{id} chunk{chunk} raw{:?}", raw);
                }
            }
        }
    }
    #[test]
    fn decimal_overflow_boundary_and_integer_ids_are_feature_independent() {
        fn number(raw: &[u8]) -> NumberState {
            let mut n = NumberState::new(raw[0]);
            for &b in &raw[1..] {
                assert!(n.byte(b).unwrap());
            }
            n
        }
        // Computed integer pin: 2^1024 - 2^970. Equality rounds to infinity (even),
        // while one below is finite; this must not depend on serde float_roundtrip.
        let threshold = b"179769313486231580793728971405303415079934132710037826936173778980444968292764750946649017977587207096330286416692887910946555547851940402630657488671505820681908902000708383676273854845817711531764475730270069855571366959622842914819860834936475292719074168444365510704342711559699508093042880177904174497792";
        let mut below = threshold.to_vec();
        *below.last_mut().unwrap() = b'1';
        assert!(number(&below).finite());
        assert!(!number(threshold).finite());
        assert!(number(b"1.7976931348623157e308").finite());
        assert!(!number(b"1.7976931348623159e308").finite());
        assert!(number(b"0e99999999999999999999999999999999").finite());
        assert!(number(b"1e-99999999999999999999999999999999").finite());
        assert_eq!(number(b"4294967295").id(), Some(u32::MAX));
        for raw in [b"4294967296".as_slice(), b"-0", b"1.0", b"1e0"] {
            assert!(number(raw).finite());
            assert_eq!(number(raw).id(), None);
        }
        for value in [below.as_slice(), threshold.as_slice(), b"-0", b"1.0", b"1e0"] {
            let mut raw = br#"{"model":{"vocab":{"a":0},"merges":[]},"meta":"#.to_vec();
            raw.extend_from_slice(value);
            raw.push(b'}');
            assert_eq!(derive(&raw, 0, 7), expected(&raw, 0));
        }
    }
    #[test]
    fn normative_container_depth_and_malformed_delimiters_are_exact() {
        for count in [125, 126, 127] {
            let mut raw = br#"{"model":{"vocab":{"a":0},"merges":[]},"meta":"#.to_vec();
            raw.extend(std::iter::repeat_n(b'[', count));
            raw.extend_from_slice(b"null");
            raw.extend(std::iter::repeat_n(b']', count));
            raw.push(b'}');
            let verdict = derive(&raw, 0, 7);
            assert_eq!(verdict, expected(&raw, 0));
            assert_eq!(matches!(verdict, PalwTokenizerJsonVerdictV1::Invalid), count >= 127);
        }
        for tail in [b"[1,]".as_slice(), b"[,1]", b"[01]", b"[1.]", b"[1e+]", b"{\"x\":true,}", b"[NaN]"] {
            let mut raw = br#"{"model":{"vocab":{"a":0},"merges":[]},"meta":"#.to_vec();
            raw.extend_from_slice(tail);
            raw.push(b'}');
            assert_eq!(derive(&raw, 0, 1), PalwTokenizerJsonVerdictV1::Invalid);
            assert_eq!(expected(&raw, 0), PalwTokenizerJsonVerdictV1::Invalid);
        }
    }
    #[test]
    fn understated_work_bound_only_convicts_after_the_original_exact_raw_hash() {
        let raw = br#"{"model":{"vocab":{"a":0},"merges":[]}}"#;
        let context = Hash64::from_u64_word(18);
        for bound in [0, 1, 3] {
            let mut stream = PalwTokenizerJsonStreamV1::new(context, palw_tokenizer_raw_commitment_v1(raw), raw.len() as u64, 0);
            let mut book = Book::new();
            loop {
                if stream.pending_key().is_some() {
                    if stream.key_events() == bound {
                        break;
                    }
                    book.satisfy(&mut stream);
                } else {
                    let offset = stream.offset() as usize;
                    stream.push(context, offset as u64, &raw[offset..]).unwrap();
                }
            }
            let before = borsh::to_vec(&stream).unwrap();
            assert!(stream.mark_work_overrun(context, bound + 1).is_err());
            assert_eq!(borsh::to_vec(&stream).unwrap(), before);
            stream.mark_work_overrun(context, bound).unwrap();
            assert!(stream.finish().is_err());
            let offset = stream.offset() as usize;
            stream.push(context, offset as u64, &raw[offset..]).unwrap();
            stream.seal().unwrap();
            assert_eq!(stream.finish().unwrap(), PalwTokenizerJsonVerdictV1::WorkBoundExceeded);
            stream.tokenizer = Hash64::from_u64_word(0xBAD);
            assert!(stream.finish().is_err());
        }
    }
    #[test]
    fn unknown_large_strings_keep_the_serialized_parser_bounded_and_false_raw_hash_never_verdicts() {
        let prefix = br#"{"model":{"vocab":{"a":0},"merges":[]},"ignored":""#;
        let suffix = br#""}"#;
        let context = Hash64::from_u64_word(18);
        let block = vec![b'x'; 65_536];
        let count = 16;
        let len = (prefix.len() + block.len() * count + suffix.len()) as u64;
        let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_TOKENIZER_RAW_DOMAIN_V1).to_state();
        h.update(&len.to_le_bytes());
        h.update(prefix);
        for _ in 0..count {
            h.update(&block);
        }
        h.update(suffix);
        let tokenizer = Hash64::from_bytes(h.finalize().as_bytes().try_into().unwrap());
        let mut s = PalwTokenizerJsonStreamV1::new(context, tokenizer, len, 0);
        let mut book = Book::new();
        let mut at = 0;
        while at < prefix.len() {
            if s.pending_key().is_some() {
                book.satisfy(&mut s);
            } else {
                at += s.push(context, s.offset(), &prefix[at..]).unwrap();
            }
        }
        if s.pending_key().is_some() {
            book.satisfy(&mut s);
        }
        let footprint = borsh::to_vec(&s).unwrap().len();
        for _ in 0..count {
            assert_eq!(s.push(context, s.offset(), &block).unwrap(), block.len());
            assert!(borsh::to_vec(&s).unwrap().len() <= footprint + 128);
        }
        s.push(context, s.offset(), suffix).unwrap();
        s.seal().unwrap();
        assert_eq!(s.finish().unwrap(), expected(br#"{"model":{"vocab":{"a":0},"merges":[]}}"#, 0));
        s.tokenizer = Hash64::from_u64_word(0xBAD);
        assert!(s.finish().is_err());
    }
}

// Read-only native allocation inventory; no state/root/codec field is added.
#[path = "palw_native_resource_v1/private/palw_tokenizer_json_stream_v1.rs"]
mod native_resource_inventory_v1;


/// Owner-local parser growth bound, derived from the existing grammar, not a protocol cap.
/// Includes one parser inline value, the bounded Frame stack, and both String Vecs.
pub(crate) fn producer_parser_requests_v1() -> Result<u64, crate::palw_native_resource_v1::NativeCoverageV1> {
    use crate::palw_native_resource_v1::TirNativeRequestsV1;
    let mut q=TirNativeRequestsV1::new();
    q.exact_v1::<PalwTokenizerJsonStreamV1>(1)?;
    q.vec_v1::<Frame>(MAX_DEPTH as u64)?;
    q.vec_v1::<u8>(16)?;
    q.vec_v1::<u8>(4)?;
    Ok(q.bytes_v1())
}
