//! **The compiler** — ADR-0096 Decision 7's "the compiled output of `misaka-palw-constraint`'s
//! JSON-Schema subset": a parsed [`Schema`] → the byte-level automaton consensus-core pins
//! ([`PalwDecodeConstraintV1`]), content-named by [`compiler_id_v1`] in its header. A pure
//! function of the schema; the same schema compiles to the same bytes on every host, which is
//! what lets a job's `constraint_id` name it.
//!
//! # What the automaton admits, precisely
//!
//! **RFC 8785's canonical form and no other spelling — except member order and one space.**
//!
//! * **At most one space (0x20) after each `:` and each `,`, and no other whitespace**: not a
//!   second space, not a newline or a tab, not after `{` or `[`, not before `:`, `,`, `}` or `]`,
//!   and never around the root value. The one space is the single-line spelling every model
//!   writes (`{"name": "Tama", "age": 3}`, `json.dumps`'s default separators): with it dead, a
//!   model whose next token is ` "` is forced onto the best token that does NOT start with a
//!   space, and the first measured masked answer (Qwen2.5-1.5B, 2026-09-10) was
//!   `{"name":null,"age":null}` for a cat that has a name. It cannot loop — one space, then a
//!   byte that is not a space — and nothing after the root value is live, so ADR-0096 §10 B7's
//!   finish ("no byte continues") still fires at the root's last byte. The derivation strips it
//!   (RFC 8785 has no insignificant whitespace), exactly as it canonicalizes a number.
//! * Strings hold well-formed UTF-8 (RFC 3629: no overlongs, no surrogates, nothing past
//!   U+10FFFF) and exactly RFC 8785 §3.2.2.2's escapes — `\"`, `\\`, `\b`, `\t`, `\n`, `\f`,
//!   `\r`, and `\u00xx` in LOWERCASE hex for the remaining C0 controls. `\/`, `\u0041`, `\u001F`
//!   and every other escape RFC 8259 permits are dead: they are not the canonical spelling.
//! * Object members in ANY order. Sorted order is what RFC 8785 says and what the `json` kind's
//!   derivation produces (ADR-0096 Decision 9); a model is not asked to sort, because "sorted"
//!   over an open set of keys is not a small automaton and over a declared set would make the
//!   model's natural order dead. Each declared member AT MOST ONCE and every `required` member
//!   present by the closing `}` — the frame's key mask, ADR-0096 Decision 7's "frame vocabulary",
//!   at most sixteen tracked members per object.
//! * `additionalProperties` absent or `true`: an undeclared member is any JSON string key with any
//!   JSON value; a schema: any string key with a value in that schema; `false`: an undeclared
//!   member is dead at its first byte. An undeclared key that happens to be a proper prefix, or an
//!   extension, of a declared one is an undeclared key — the key trie falls through to the generic
//!   string at the first byte that leaves it.
//! * Numbers by RFC 8259's grammar, `-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?`, for
//!   `number`; the integer grammar `-?(0|[1-9][0-9]*)` for `integer`. **Canonicality of a number
//!   is NOT enforced** (the shortest round-trip digit string, no trailing fraction zeros, the
//!   exponent layout are not regular properties); the derivation canonicalizes. **`minimum` and
//!   `maximum` are NOT enforced by the automaton** — they stay post-hoc in [`validate`] — and so
//!   does the integer-ness of a spelling: `1.0` is an integer to JSON Schema and is admitted only
//!   under `number`; under `integer` the automaton admits the integer grammar, so an integer at or
//!   above 10^21 (canonical spelling `1e+21`) is not reachable there.
//! * `enum` and `const`: the EXACT canonical bytes of each listed value that also satisfies the
//!   schema's other keywords (a value that would fail [`validate`] is not a candidate; a schema
//!   left with none is refused), with the one space after a separator admitted as everywhere
//!   else. An object-valued member must therefore be spelled with sorted keys — it is a literal,
//!   not a grammar.
//! * `minLength` / `maxLength` count characters (code points): a raw multi-byte sequence is one, a
//!   `\u00xx` escape is one. `minItems` / `maxItems` count items.
//! * Nesting to the depth consensus-core pins (`PALW_CONSTRAINT_MAX_DEPTH_V1`, sixteen pushed
//!   frames); a schema at [`MAX_SCHEMA_DEPTH`](crate::schema::MAX_SCHEMA_DEPTH) reaches exactly it.
//!
//! # What is refused by name
//!
//! `pattern` — ADR-0096 Part D: "the regex subset's exact bounds … pinned by table when the crate
//! lands"; this compiler version compiles no pattern, and the entrance validates it post-hoc. An
//! object with more than sixteen tracked members (declared plus required). A `required` name that
//! `properties` does not declare while `additionalProperties: false` forbids it. An `enum`/`const`
//! none of whose values satisfies the schema. And an automaton past consensus-core's bounds — the
//! 64 KiB ceiling binds first; a `maxLength` in the thousands is the usual way to reach it.
//!
//! # `compile_json_object_v1`
//!
//! Decision 3's `json_object` mode: the grammar of ANY JSON value — one self-referential frame
//! with generic string keys and no at-most-once tracking (RFC 8259 does not forbid a duplicate
//! member; the derivation reads the last), unbounded strings and arrays, nesting to the depth.

use std::collections::BTreeMap;

use kaspa_consensus_core::palw_decode_constraint_v1::{
    PALW_CONSTRAINT_KEY_SLOTS_V1, PALW_CONSTRAINT_MAX_FRAMES_V1, PALW_CONSTRAINT_MAX_NODES_V1, PALW_DECODE_CONSTRAINT_VERSION_V1,
    PalwConstraintActionV1, PalwConstraintEdgeV1, PalwConstraintFrameV1, PalwConstraintNodeV1, PalwDecodeConstraintV1,
};
use kaspa_hashes::Hash64;
use serde_json::Value;

use crate::canonical::to_rfc8785;
use crate::schema::{AdditionalProperties, JsonType, Schema, validate};

/// The keyed-hash domain the compiler's content name is minted under.
pub const PALW_CONSTRAINT_COMPILER_DOMAIN_V1: &[u8] = b"misaka-palw/constraint-compiler/v1";

/// **The compiler's content name** — a keyed BLAKE2b-512 over this file's own source bytes, the
/// way ADR-0078 Decision 3 names a transformer by its source tree. Every constraint this compiler
/// emits carries it in the header, so two automata compiled from one schema by two compiler
/// versions are two constraints, and a change to any line of this file (a comment included) is a
/// new compiler. Read from the source at build time; nothing at run time can move it.
pub fn compiler_id_v1() -> Hash64 {
    kaspa_hashes::blake2b_512_keyed(PALW_CONSTRAINT_COMPILER_DOMAIN_V1, include_bytes!("compile.rs"))
}

/// **Compile a schema in the subset to its automaton.** `Err` names the keyword or bound that
/// stopped it; nothing is approximated.
pub fn compile_v1(schema: &Schema) -> Result<PalwDecodeConstraintV1, String> {
    let mut builder = Builder::default();
    let root = builder.value_frame(schema)?;
    builder.finish(root)
}

/// The grammar of any JSON value (ADR-0096 Decision 3's `json_object`).
pub fn compile_json_object_v1() -> PalwDecodeConstraintV1 {
    compile_v1(&Schema::default()).expect("the grammar of any JSON value is inside every bound")
}

type Action = PalwConstraintActionV1;

/// A node under construction: a dense byte table, turned into sorted ranges at the end.
struct NodeB {
    accepting: bool,
    table: Vec<Option<Action>>,
}

struct FrameB {
    nodes: Vec<NodeB>,
}

#[derive(Default)]
struct Builder {
    frames: Vec<FrameB>,
    /// The one frame for "any JSON value", created once and pushed by every unconstrained slot.
    any_frame: Option<u16>,
}

/// The string phases of RFC 8785's string grammar: inside text, inside an escape, inside a
/// `\u00xx` escape digit by digit, or inside a UTF-8 sequence with `n` continuation bytes to go
/// (`E0`/`Ed`/`F0`/`F4` are the lead bytes whose first continuation range is narrowed).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Ph {
    Text,
    Esc,
    U,
    U0,
    U00,
    U000,
    U001,
    C1,
    C2,
    C3,
    E0,
    Ed,
    F0,
    F4,
}

const PHASES: [Ph; 14] =
    [Ph::Text, Ph::Esc, Ph::U, Ph::U0, Ph::U00, Ph::U000, Ph::U001, Ph::C1, Ph::C2, Ph::C3, Ph::E0, Ph::Ed, Ph::F0, Ph::F4];

/// One byte of a JSON string in RFC 8785's canonical form. The closing `"` is NOT a step (it is
/// the caller's, since what follows it differs by context); every landing on [`Ph::Text`]
/// completes one character, which is what the length bounds count.
fn string_step(phase: Ph, byte: u8) -> Option<Ph> {
    Some(match (phase, byte) {
        (Ph::Text, b'"') => return None,
        (Ph::Text, b'\\') => Ph::Esc,
        (Ph::Text, 0x20..=0x7f) => Ph::Text,
        (Ph::Text, 0xc2..=0xdf) => Ph::C1,
        (Ph::Text, 0xe0) => Ph::E0,
        (Ph::Text, 0xe1..=0xec | 0xee..=0xef) => Ph::C2,
        (Ph::Text, 0xed) => Ph::Ed,
        (Ph::Text, 0xf0) => Ph::F0,
        (Ph::Text, 0xf1..=0xf3) => Ph::C3,
        (Ph::Text, 0xf4) => Ph::F4,
        (Ph::Esc, b'"' | b'\\' | b'b' | b'f' | b'n' | b'r' | b't') => Ph::Text,
        (Ph::Esc, b'u') => Ph::U,
        (Ph::U, b'0') => Ph::U0,
        (Ph::U0, b'0') => Ph::U00,
        (Ph::U00, b'0') => Ph::U000,
        (Ph::U00, b'1') => Ph::U001,
        (Ph::U000, b'0'..=b'7' | b'b' | b'e' | b'f') => Ph::Text,
        (Ph::U001, b'0'..=b'9' | b'a'..=b'f') => Ph::Text,
        (Ph::C1, 0x80..=0xbf) => Ph::Text,
        (Ph::C2, 0x80..=0xbf) => Ph::C1,
        (Ph::C3, 0x80..=0xbf) => Ph::C2,
        (Ph::E0, 0xa0..=0xbf) => Ph::C1,
        (Ph::Ed, 0x80..=0x9f) => Ph::C1,
        (Ph::F0, 0x90..=0xbf) => Ph::C2,
        (Ph::F4, 0x80..=0x8f) => Ph::C2,
        _ => return None,
    })
}

/// A schema with no keyword at all: the grammar of any JSON value.
fn is_any(s: &Schema) -> bool {
    s.types.is_none()
        && s.properties.is_empty()
        && s.required.is_empty()
        && matches!(s.additional_properties, None | Some(AdditionalProperties::Allowed))
        && s.items.is_none()
        && s.min_items.is_none()
        && s.max_items.is_none()
        && s.enumeration.is_none()
        && s.constant.is_none()
        && s.pattern.is_none()
        && s.min_length.is_none()
        && s.max_length.is_none()
        && s.minimum.is_none()
        && s.maximum.is_none()
}

/// `enum`/`const` as literals: the canonical bytes of every listed value the schema's other
/// keywords accept. `None` when the schema has neither keyword.
fn literal_candidates(schema: &Schema) -> Result<Option<Vec<Vec<u8>>>, String> {
    let values: Vec<&Value> = match (&schema.constant, &schema.enumeration) {
        (Some(c), _) => vec![c],
        (None, Some(e)) => e.iter().collect(),
        (None, None) => return Ok(None),
    };
    let mut out = Vec::with_capacity(values.len());
    for value in values {
        if validate(schema, value).is_ok() {
            out.push(to_rfc8785(value)?);
        }
    }
    if out.is_empty() {
        return Err("no `enum`/`const` value satisfies the schema's other keywords — the schema admits nothing".to_string());
    }
    out.sort();
    out.dedup();
    Ok(Some(out))
}

fn count(value: Option<u64>, keyword: &str) -> Result<Option<usize>, String> {
    value.map(|v| usize::try_from(v).map_err(|_| format!("`{keyword}` {v} is past what an automaton can count"))).transpose()
}

impl Builder {
    fn frame(&mut self) -> Result<u16, String> {
        if self.frames.len() >= PALW_CONSTRAINT_MAX_FRAMES_V1 {
            return Err(format!("the schema compiles to more than {PALW_CONSTRAINT_MAX_FRAMES_V1} frames"));
        }
        self.frames.push(FrameB { nodes: Vec::new() });
        Ok((self.frames.len() - 1) as u16)
    }

    fn node(&mut self, f: u16, accepting: bool) -> Result<u16, String> {
        let nodes = &mut self.frames[f as usize].nodes;
        if nodes.len() >= PALW_CONSTRAINT_MAX_NODES_V1 {
            return Err(format!("the schema compiles to more than {PALW_CONSTRAINT_MAX_NODES_V1} nodes in one frame"));
        }
        nodes.push(NodeB { accepting, table: vec![None; 256] });
        Ok((nodes.len() - 1) as u16)
    }

    /// Claim one byte of a node. A byte claimed twice with different actions is a compiler
    /// invariant broken, reported rather than silently resolved.
    fn set(&mut self, f: u16, from: u16, byte: u8, action: Action) -> Result<(), String> {
        let slot = &mut self.frames[f as usize].nodes[from as usize].table[byte as usize];
        match slot {
            Some(existing) if *existing != action => {
                Err(format!("compiler invariant: byte {byte:#04x} of node {from} in frame {f} is claimed twice"))
            }
            _ => {
                *slot = Some(action);
                Ok(())
            }
        }
    }

    fn set_range(&mut self, f: u16, from: u16, lo: u8, hi: u8, action: Action) -> Result<(), String> {
        for byte in lo..=hi {
            self.set(f, from, byte, action)?;
        }
        Ok(())
    }

    /// Claim a byte only if nothing claimed it first — the fallback edges.
    fn fill(&mut self, f: u16, from: u16, byte: u8, action: Action) {
        let slot = &mut self.frames[f as usize].nodes[from as usize].table[byte as usize];
        if slot.is_none() {
            *slot = Some(action);
        }
    }

    /// Every unclaimed byte pushes `frame`: a value's first byte is read by the value's frame,
    /// and a byte no value starts with dies there.
    fn push_all(&mut self, f: u16, from: u16, frame: u16, resume: u16) {
        for byte in 0..=255u8 {
            self.fill(f, from, byte, Action::Push { frame, resume });
        }
    }

    /// After a separator (`:` or `,`): the value's frame reads the next byte, or ONE space comes
    /// first and then the frame reads the byte after it. A second space dies in the frame's start
    /// node, since no JSON value starts with one.
    fn one_space_then_push(&mut self, f: u16, from: u16, frame: u16, resume: u16) -> Result<(), String> {
        let spaced = self.node(f, false)?;
        self.set(f, from, b' ', Action::Goto(spaced))?;
        self.push_all(f, from, frame, resume);
        self.push_all(f, spaced, frame, resume);
        Ok(())
    }

    /// A complete value's exits: its parent reads the terminator.
    fn pops(&mut self, f: u16, node: u16) -> Result<(), String> {
        for byte in b",]}" {
            self.set(f, node, *byte, Action::Pop)?;
        }
        Ok(())
    }

    fn finish(self, start_frame: u16) -> Result<PalwDecodeConstraintV1, String> {
        let frames = self
            .frames
            .into_iter()
            .map(|frame| PalwConstraintFrameV1 {
                start: 0,
                nodes: frame
                    .nodes
                    .into_iter()
                    .map(|n| PalwConstraintNodeV1 { accepting: n.accepting, edges: ranges(&n.table) })
                    .collect(),
            })
            .collect();
        let constraint =
            PalwDecodeConstraintV1 { version: PALW_DECODE_CONSTRAINT_VERSION_V1, compiler_id: compiler_id_v1(), start_frame, frames };
        constraint.validate().map_err(|e| format!("the compiled automaton is outside consensus-core's bounds: {e}"))?;
        Ok(constraint)
    }

    /// The frame that reads one value of `schema`. Node 0 is the start, node 1 the shared
    /// `done` (accepting; pops on a terminator).
    fn value_frame(&mut self, schema: &Schema) -> Result<u16, String> {
        if is_any(schema) {
            if let Some(f) = self.any_frame {
                return Ok(f);
            }
            let f = self.frame()?;
            self.any_frame = Some(f);
            self.fill_value(f, schema)?;
            return Ok(f);
        }
        let f = self.frame()?;
        self.fill_value(f, schema)?;
        Ok(f)
    }

    fn fill_value(&mut self, f: u16, schema: &Schema) -> Result<(), String> {
        let start = self.node(f, false)?;
        let done = self.node(f, true)?;
        self.pops(f, done)?;
        if let Some(literals) = literal_candidates(schema)? {
            return self.literal_trie(f, start, &literals);
        }
        if let Some(pattern) = &schema.pattern {
            return Err(format!(
                "`pattern` {:?} is not compiled by this compiler version: ADR-0096 Part D pins the regex subset by table when it lands, \
                 and until then a pattern is validated after the fact and cannot be committed",
                pattern.source
            ));
        }
        let all = [
            JsonType::Object,
            JsonType::Array,
            JsonType::String,
            JsonType::Number,
            JsonType::Integer,
            JsonType::Boolean,
            JsonType::Null,
        ];
        let types: Vec<JsonType> = schema.types.clone().unwrap_or_else(|| all.to_vec());
        let mut literals: Vec<Vec<u8>> = Vec::new();
        for t in &types {
            match t {
                JsonType::Null => literals.push(b"null".to_vec()),
                JsonType::Boolean => {
                    literals.push(b"true".to_vec());
                    literals.push(b"false".to_vec());
                }
                JsonType::String => self.string_dfa(f, start, done, schema)?,
                JsonType::Number => self.number_dfa(f, start, true)?,
                JsonType::Integer => {
                    if !types.contains(&JsonType::Number) {
                        self.number_dfa(f, start, false)?;
                    }
                }
                JsonType::Array => self.array_dfa(f, start, done, schema)?,
                JsonType::Object => self.object_dfa(f, start, done, schema)?,
            }
        }
        if !literals.is_empty() {
            self.literal_trie(f, start, &literals)?;
        }
        Ok(())
    }

    /// A trie of byte strings from `root`; a terminal node is a complete value (accepting, pops).
    fn literal_trie(&mut self, f: u16, root: u16, literals: &[Vec<u8>]) -> Result<(), String> {
        struct Tn {
            children: BTreeMap<u8, usize>,
            terminal: bool,
            /// Reached by a `:` or `,` outside a string: the one space may come next (the module
            /// doc's rule holds inside a literal too, so an object-valued `const` reads the same
            /// single-line spelling as an object schema does).
            after_separator: bool,
        }
        let mut trie = vec![Tn { children: BTreeMap::new(), terminal: false, after_separator: false }];
        for literal in literals {
            if literal.is_empty() {
                return Err("an empty literal admits the empty answer, which is no JSON value".to_string());
            }
            let mut at = 0usize;
            let (mut in_string, mut escaped) = (false, false);
            for &byte in literal {
                let separator = !in_string && (byte == b':' || byte == b',');
                if in_string {
                    if escaped {
                        escaped = false;
                    } else if byte == b'\\' {
                        escaped = true;
                    } else if byte == b'"' {
                        in_string = false;
                    }
                } else if byte == b'"' {
                    in_string = true;
                }
                let fresh = trie.len();
                let child = *trie[at].children.entry(byte).or_insert(fresh);
                if child == fresh {
                    trie.push(Tn { children: BTreeMap::new(), terminal: false, after_separator: separator });
                }
                at = child;
            }
            trie[at].terminal = true;
        }
        let mut ids = vec![root];
        for tn in trie.iter().skip(1) {
            ids.push(self.node(f, tn.terminal)?);
        }
        for (tn, &from) in trie.iter().zip(ids.iter()) {
            for (&byte, &child) in &tn.children {
                self.set(f, from, byte, Action::Goto(ids[child]))?;
            }
            if tn.after_separator {
                // The spaced twin reads exactly what the node reads; a second space is dead
                // because no canonical literal has a space after a separator.
                let spaced = self.node(f, false)?;
                self.set(f, from, b' ', Action::Goto(spaced))?;
                for (&byte, &child) in &tn.children {
                    self.set(f, spaced, byte, Action::Goto(ids[child]))?;
                }
            }
            if tn.terminal {
                self.pops(f, from)?;
            }
        }
        Ok(())
    }

    /// A string with character bounds: one layer of phase nodes per count, a character completing
    /// on every landing on `Text`. With no `maxLength` the layers collapse at `minLength`.
    fn string_dfa(&mut self, f: u16, start: u16, done: u16, schema: &Schema) -> Result<(), String> {
        let min = count(schema.min_length, "minLength")?.unwrap_or(0);
        let max = count(schema.max_length, "maxLength")?;
        let layers = match max {
            Some(m) => m + 1,
            None => min + 1,
        };
        let mut nodes: Vec<BTreeMap<Ph, u16>> = vec![BTreeMap::new(); layers];
        for layer in nodes.iter_mut() {
            layer.insert(Ph::Text, self.node(f, false)?);
        }
        self.set(f, start, b'"', Action::Goto(nodes[0][&Ph::Text]))?;
        let mut work: Vec<(usize, Ph)> = (0..layers).map(|k| (k, Ph::Text)).collect();
        while let Some((k, phase)) = work.pop() {
            let from = nodes[k][&phase];
            for byte in 0..=255u8 {
                if phase == Ph::Text && byte == b'"' {
                    if k >= min {
                        self.set(f, from, byte, Action::Goto(done))?;
                    }
                    continue;
                }
                let Some(next_phase) = string_step(phase, byte) else { continue };
                let next_layer = if next_phase == Ph::Text {
                    match max {
                        Some(m) if k + 1 > m => continue,
                        Some(_) => k + 1,
                        None => (k + 1).min(min),
                    }
                } else {
                    k
                };
                let target = match nodes[next_layer].get(&next_phase) {
                    Some(&n) => n,
                    None => {
                        let n = self.node(f, false)?;
                        nodes[next_layer].insert(next_phase, n);
                        work.push((next_layer, next_phase));
                        n
                    }
                };
                self.set(f, from, byte, Action::Goto(target))?;
            }
        }
        Ok(())
    }

    /// RFC 8259's number grammar, or its integer part alone. Complete nodes are accepting and pop
    /// on a terminator — a number ends where its parent's byte begins.
    fn number_dfa(&mut self, f: u16, start: u16, full: bool) -> Result<(), String> {
        let neg = self.node(f, false)?;
        let zero = self.node(f, true)?;
        let int = self.node(f, true)?;
        self.set(f, start, b'-', Action::Goto(neg))?;
        self.set(f, start, b'0', Action::Goto(zero))?;
        self.set_range(f, start, b'1', b'9', Action::Goto(int))?;
        self.set(f, neg, b'0', Action::Goto(zero))?;
        self.set_range(f, neg, b'1', b'9', Action::Goto(int))?;
        self.set_range(f, int, b'0', b'9', Action::Goto(int))?;
        self.pops(f, zero)?;
        self.pops(f, int)?;
        if !full {
            return Ok(());
        }
        let frac_start = self.node(f, false)?;
        let frac = self.node(f, true)?;
        let exp_start = self.node(f, false)?;
        let exp_sign = self.node(f, false)?;
        let exp = self.node(f, true)?;
        for complete in [zero, int, frac] {
            if complete != frac {
                self.set(f, complete, b'.', Action::Goto(frac_start))?;
            }
            self.set(f, complete, b'e', Action::Goto(exp_start))?;
            self.set(f, complete, b'E', Action::Goto(exp_start))?;
        }
        self.set_range(f, frac_start, b'0', b'9', Action::Goto(frac))?;
        self.set_range(f, frac, b'0', b'9', Action::Goto(frac))?;
        self.pops(f, frac)?;
        self.set(f, exp_start, b'+', Action::Goto(exp_sign))?;
        self.set(f, exp_start, b'-', Action::Goto(exp_sign))?;
        self.set_range(f, exp_start, b'0', b'9', Action::Goto(exp))?;
        self.set_range(f, exp_sign, b'0', b'9', Action::Goto(exp))?;
        self.set_range(f, exp, b'0', b'9', Action::Goto(exp))?;
        self.pops(f, exp)?;
        Ok(())
    }

    /// `[` items `]` with item counting: `after[k]` is "k + 1 items read", `]` is live from
    /// `minItems`, `,` up to `maxItems`; with no `maxItems` the last layer loops.
    fn array_dfa(&mut self, f: u16, start: u16, done: u16, schema: &Schema) -> Result<(), String> {
        let any = Schema::default();
        let items = self.value_frame(schema.items.as_deref().unwrap_or(&any))?;
        let min = count(schema.min_items, "minItems")?.unwrap_or(0);
        let max = count(schema.max_items, "maxItems")?;
        let open = self.node(f, false)?;
        self.set(f, start, b'[', Action::Goto(open))?;
        if min == 0 {
            self.set(f, open, b']', Action::Goto(done))?;
        }
        let cap = max.unwrap_or(min.max(1));
        if cap == 0 {
            return Ok(()); // `maxItems: 0`: the empty array and nothing else
        }
        let mut after = Vec::with_capacity(cap);
        for _ in 0..cap {
            after.push(self.node(f, false)?);
        }
        self.push_all(f, open, items, after[0]);
        for k in 1..=cap {
            let at = after[k - 1];
            if k >= min {
                self.set(f, at, b']', Action::Goto(done))?;
            }
            let more = max.is_none_or(|m| k < m);
            if more {
                let resume = if k < cap { after[k] } else { at };
                let next = self.node(f, false)?;
                self.set(f, at, b',', Action::Goto(next))?;
                self.one_space_then_push(f, next, items, resume)?;
            }
        }
        Ok(())
    }

    /// `{` members `}`: a key trie over the tracked members' canonical spellings, each terminal
    /// setting its key bit, the closing `}` checking the required mask; undeclared members fall
    /// through to a generic string key when `additionalProperties` allows them.
    fn object_dfa(&mut self, f: u16, start: u16, done: u16, schema: &Schema) -> Result<(), String> {
        let any = Schema::default();
        let mut tracked: Vec<(&str, Option<&Schema>)> = schema.properties.iter().map(|(n, s)| (n.as_str(), Some(s))).collect();
        for name in &schema.required {
            if !tracked.iter().any(|(n, _)| *n == name.as_str()) {
                tracked.push((name.as_str(), None));
            }
        }
        if tracked.len() > PALW_CONSTRAINT_KEY_SLOTS_V1 as usize {
            return Err(format!(
                "the object declares {} members (`properties` and `required` names) where the automaton tracks at most {}: ADR-0096 \
                 Decision 7's frame vocabulary is sixteen key slots",
                tracked.len(),
                PALW_CONSTRAINT_KEY_SLOTS_V1
            ));
        }
        let additional: Option<&Schema> = match &schema.additional_properties {
            None | Some(AdditionalProperties::Allowed) => Some(&any),
            Some(AdditionalProperties::Forbidden) => None,
            Some(AdditionalProperties::Schema(s)) => Some(s),
        };
        let mut required_mask = 0u16;
        for (index, (name, _)) in tracked.iter().enumerate() {
            if schema.required.iter().any(|r| r == name) {
                required_mask |= 1 << index;
            }
        }
        let mut key_frames = Vec::with_capacity(tracked.len());
        for (name, sub) in &tracked {
            let sub = match sub {
                Some(s) => *s,
                None => additional.ok_or_else(|| {
                    format!(
                        "`required` names {name:?}, which `properties` does not declare and `additionalProperties: false` forbids — \
                         the object admits nothing"
                    )
                })?,
            };
            key_frames.push(self.value_frame(sub)?);
        }
        let additional_frame = match additional {
            Some(s) => Some(self.value_frame(s)?),
            None => None,
        };

        let open = self.node(f, false)?;
        let after_value = self.node(f, false)?;
        let next_key = self.node(f, false)?;
        let trie_root = self.node(f, false)?;
        self.set(f, start, b'{', Action::Goto(open))?;
        self.set(f, open, b'}', Action::Close { required: required_mask, next: done })?;
        self.set(f, open, b'"', Action::Goto(trie_root))?;
        self.set(f, after_value, b',', Action::Goto(next_key))?;
        self.set(f, after_value, b'}', Action::Close { required: required_mask, next: done })?;
        self.set(f, next_key, b'"', Action::Goto(trie_root))?;
        let next_key_spaced = self.node(f, false)?;
        self.set(f, next_key, b' ', Action::Goto(next_key_spaced))?;
        self.set(f, next_key_spaced, b'"', Action::Goto(trie_root))?;

        // The generic key string, for undeclared members: one node per phase, the closing quote
        // leading to the additional value.
        let mut generic: BTreeMap<Ph, u16> = BTreeMap::new();
        let mut add_colon = None;
        if let Some(frame) = additional_frame {
            let colon = self.node(f, false)?;
            let value = self.node(f, false)?;
            self.set(f, colon, b':', Action::Goto(value))?;
            self.one_space_then_push(f, value, frame, after_value)?;
            add_colon = Some(colon);
            for phase in PHASES {
                generic.insert(phase, self.node(f, false)?);
            }
            for phase in PHASES {
                let from = generic[&phase];
                for byte in 0..=255u8 {
                    if phase == Ph::Text && byte == b'"' {
                        self.set(f, from, byte, Action::Goto(colon))?;
                    } else if let Some(next) = string_step(phase, byte) {
                        self.set(f, from, byte, Action::Goto(generic[&next]))?;
                    }
                }
            }
        }

        // Per tracked member: `:` then the member's value frame.
        let mut colon_of = Vec::with_capacity(tracked.len());
        for frame in &key_frames {
            let colon = self.node(f, false)?;
            let value = self.node(f, false)?;
            self.set(f, colon, b':', Action::Goto(value))?;
            self.one_space_then_push(f, value, *frame, after_value)?;
            colon_of.push(colon);
        }

        // The key trie over canonical spellings (RFC 8785's escapes, without the quotes), each
        // node knowing its string phase so a byte that leaves the trie lands in the right generic
        // phase.
        struct Tn {
            children: BTreeMap<u8, usize>,
            terminal: Option<usize>,
            phase: Ph,
        }
        let mut trie = vec![Tn { children: BTreeMap::new(), terminal: None, phase: Ph::Text }];
        for (index, (name, _)) in tracked.iter().enumerate() {
            let spelled = to_rfc8785(&Value::String((*name).to_string()))?;
            let inner = &spelled[1..spelled.len() - 1];
            let mut at = 0usize;
            let mut phase = Ph::Text;
            for &byte in inner {
                phase = string_step(phase, byte)
                    .ok_or_else(|| format!("compiler invariant: the canonical spelling of member {name:?} is not a JSON string"))?;
                let fresh = trie.len();
                let child = *trie[at].children.entry(byte).or_insert(fresh);
                if child == fresh {
                    trie.push(Tn { children: BTreeMap::new(), terminal: None, phase });
                }
                at = child;
            }
            if trie[at].phase != Ph::Text || trie[at].terminal.is_some() {
                return Err(format!("compiler invariant: member {name:?} does not end in text, or is declared twice"));
            }
            trie[at].terminal = Some(index);
        }
        let mut ids = vec![trie_root];
        for _ in trie.iter().skip(1) {
            ids.push(self.node(f, false)?);
        }
        for (tn, &from) in trie.iter().zip(ids.iter()) {
            for (&byte, &child) in &tn.children {
                self.set(f, from, byte, Action::Goto(ids[child]))?;
            }
            if tn.phase == Ph::Text {
                match tn.terminal {
                    Some(index) => self.set(f, from, b'"', Action::Key { index: index as u8, next: colon_of[index] })?,
                    None => {
                        if let Some(colon) = add_colon {
                            self.set(f, from, b'"', Action::Goto(colon))?;
                        }
                    }
                }
            }
            if !generic.is_empty() {
                for byte in 0..=255u8 {
                    if let Some(next) = string_step(tn.phase, byte) {
                        self.fill(f, from, byte, Action::Goto(generic[&next]));
                    }
                }
            }
        }
        Ok(())
    }
}

/// Dense table → sorted disjoint ranges, adjacent bytes with one action merged.
fn ranges(table: &[Option<Action>]) -> Vec<PalwConstraintEdgeV1> {
    let mut out = Vec::new();
    let mut lo = 0usize;
    while lo < table.len() {
        let Some(action) = table[lo] else {
            lo += 1;
            continue;
        };
        let mut hi = lo;
        while hi + 1 < table.len() && table[hi + 1] == Some(action) {
            hi += 1;
        }
        out.push(PalwConstraintEdgeV1 { lo: lo as u8, hi: hi as u8, action });
        lo = hi + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::parse;
    use kaspa_consensus_core::palw_decode_constraint_v1::{
        PALW_CONSTRAINT_MAX_BYTES_V1, PALW_CONSTRAINT_MAX_DEPTH_V1, constraint_admits_v1, constraint_id_v1,
        constraint_is_accepting_v1, constraint_state_after_v1,
    };
    use serde_json::json;

    fn compiled(schema: Value) -> PalwDecodeConstraintV1 {
        let parsed = parse(&schema).unwrap_or_else(|e| panic!("{schema} should parse: {e}"));
        compile_v1(&parsed).unwrap_or_else(|e| panic!("{schema} should compile: {e}"))
    }

    fn accepts(c: &PalwDecodeConstraintV1, bytes: &[u8]) -> bool {
        constraint_state_after_v1(c, [bytes]).is_some_and(|s| constraint_is_accepting_v1(c, &s))
    }

    fn canonical(value: &Value) -> Vec<u8> {
        to_rfc8785(value).unwrap()
    }

    /// The canonical bytes with one space after every `:` and `,` outside strings — the
    /// single-line spelling `json.dumps` writes by default, and the one the compiler admits.
    fn single_line(canonical: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(canonical.len() * 2);
        let (mut in_string, mut escaped) = (false, false);
        for &byte in canonical {
            out.push(byte);
            if in_string {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    in_string = false;
                }
            } else if byte == b'"' {
                in_string = true;
            } else if byte == b':' || byte == b',' {
                out.push(b' ');
            }
        }
        out
    }

    /// The canonical bytes of every value that validates are accepted; the canonical bytes of
    /// every value that does not validate are not (dead, or live but not complete); and a
    /// non-canonical spelling of a valid value — whitespace, an escape RFC 8785 does not use — is
    /// dead. Over a handful of schemas that together cover the subset.
    #[test]
    fn every_canonical_answer_that_validates_is_accepted_and_nothing_else_is() {
        let person = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "minLength": 1, "maxLength": 8 },
                "age": { "type": "integer" },
                "tags": { "type": "array", "items": { "type": "string" }, "maxItems": 2 },
                "kind": { "enum": ["person", "robot", 7] },
                "score": { "type": "number", "minimum": 0 },
                "nick": { "type": ["string", "null"] },
                "ok": { "type": "boolean" },
                "extra": { "type": "object", "additionalProperties": { "type": "integer" } }
            },
            "required": ["name", "age"],
            "additionalProperties": false
        });
        let valid = [
            json!({ "name": "Ada", "age": 36 }),
            json!({ "age": 0, "name": "A" }),
            json!({ "name": "Ada", "age": -1, "tags": [], "kind": "robot", "score": 1.5e3, "nick": null, "ok": true, "extra": {} }),
            json!({ "name": "日本語です", "age": 7, "tags": ["a", "b"], "kind": 7, "score": 0.25, "nick": "x", "ok": false, "extra": { "h": 1, "": 2 } }),
            json!({ "name": "q\"\n\u{1f}", "age": 12345678901u64 }),
        ];
        let invalid = [
            json!({ "name": "Ada" }),
            json!({ "age": 3 }),
            json!({ "name": "", "age": 3 }),
            json!({ "name": "toolongname", "age": 3 }),
            json!({ "name": "Ada", "age": 3.5 }),
            json!({ "name": "Ada", "age": 3, "tags": ["a", "b", "c"] }),
            json!({ "name": "Ada", "age": 3, "tags": [1] }),
            json!({ "name": "Ada", "age": 3, "kind": "alien" }),
            json!({ "name": "Ada", "age": 3, "nick": 1 }),
            json!({ "name": "Ada", "age": 3, "ok": "yes" }),
            json!({ "name": "Ada", "age": 3, "extra": { "h": "no" } }),
            json!({ "name": "Ada", "age": 3, "zzz": 1 }),
            json!(["not", "an", "object"]),
            json!("Ada"),
        ];
        let schemas: Vec<(Value, Vec<Value>, Vec<Value>)> = vec![
            (person, valid.to_vec(), invalid.to_vec()),
            (
                json!({ "type": "array", "items": { "type": "number" }, "minItems": 1, "maxItems": 3 }),
                vec![json!([1]), json!([1, 2.5, -3e-7]), json!([0, 0, 0])],
                vec![json!([]), json!([1, 2, 3, 4]), json!(["1"]), json!(1), json!({})],
            ),
            (
                json!({ "type": "array", "items": { "type": "array", "items": { "type": "integer" }, "minItems": 2 } }),
                vec![json!([]), json!([[1, 2]]), json!([[1, 2, 3], [4, 5]])],
                vec![json!([[1]]), json!([[]]), json!([1, 2]), json!([["a", "b"]])],
            ),
            (
                json!({ "const": { "b": [1, "x"], "a": null } }),
                vec![json!({ "a": null, "b": [1, "x"] })],
                vec![json!({ "a": null, "b": [1, "y"] }), json!({ "a": null }), json!(null)],
            ),
            (
                json!({ "type": "string", "enum": ["a", "ab", "abc", 1] }),
                vec![json!("a"), json!("ab"), json!("abc")],
                vec![json!(1), json!("abcd"), json!("b"), json!("")],
            ),
            (
                json!({ "type": ["integer", "null"], "maximum": 10 }),
                vec![json!(0), json!(-5), json!(10), json!(null)],
                vec![json!(1.5), json!("1"), json!(true)],
            ),
            (
                json!({ "type": "object", "properties": { "n": { "type": "number" } }, "required": ["m"] }),
                vec![json!({ "m": 1 }), json!({ "m": [1, { "x": null }], "n": 2, "z": "any" })],
                vec![json!({ "n": 2 }), json!({}), json!({ "n": "s", "m": 1 })],
            ),
            (
                json!({ "type": "object", "properties": { "ab": { "const": 1 } }, "required": ["ab"], "additionalProperties": { "type": "boolean" } }),
                vec![json!({ "ab": 1 }), json!({ "a": true, "ab": 1, "abc": false, "b": true })],
                vec![json!({ "ab": 2 }), json!({ "a": 1, "ab": 1 }), json!({ "abc": true })],
            ),
        ];
        let mut accepted = 0usize;
        let mut refused = 0usize;
        for (schema, valid, invalid) in schemas {
            let parsed = parse(&schema).unwrap();
            let c = compile_v1(&parsed).unwrap_or_else(|e| panic!("{schema}: {e}"));
            for v in &valid {
                validate(&parsed, v).unwrap_or_else(|e| panic!("{v} should validate against {schema}: {e:?}"));
                assert!(accepts(&c, &canonical(v)), "{schema} must accept {v}");
                accepted += 1;
                // One space after each separator is the single-line spelling, admitted.
                assert!(accepts(&c, &single_line(&canonical(v))), "{schema} must accept the single-line spelling of {v}");
                // Any other whitespace — the pretty spelling's newlines and indentation — is dead.
                let spaced = serde_json::to_vec_pretty(v).unwrap();
                if spaced != canonical(v) {
                    assert!(!accepts(&c, &spaced), "{schema} must not accept the pretty spelling of {v}");
                }
            }
            for v in &invalid {
                assert!(validate(&parsed, v).is_err(), "{v} should NOT validate against {schema}");
                assert!(!accepts(&c, &canonical(v)), "{schema} must not accept {v}");
                refused += 1;
            }
            // Round trip: the bytes parse to the same automaton and the same id.
            let bytes = c.to_bytes();
            let back = PalwDecodeConstraintV1::from_bytes(&bytes).unwrap();
            assert_eq!(back, c);
            assert_eq!(back.id(), c.id());
            assert_eq!(c.compiler_id, compiler_id_v1());
            assert!(bytes.len() <= PALW_CONSTRAINT_MAX_BYTES_V1);
        }
        assert!(accepted >= 20 && refused >= 30, "{accepted} accepted, {refused} refused");
        // `minimum`/`maximum` stay post-hoc: a value only they refuse is ACCEPTED by the automaton
        // and REFUSED by `validate` — the one place the two deliberately disagree.
        let bounded = parse(&json!({ "type": ["integer", "null"], "maximum": 10 })).unwrap();
        let c = compile_v1(&bounded).unwrap();
        for over in [json!(11), json!(12345678901234u64), json!(1e2)] {
            assert!(validate(&bounded, &over).is_err());
            assert!(accepts(&c, &canonical(&over)), "{over} is the automaton's to admit and the validator's to refuse");
        }
    }

    /// Decision 3's `json_object`: any canonical JSON value is accepted, and its single-line
    /// spelling; malformed JSON is not, whitespace other than one space after a separator is
    /// dead, and the depth is consensus-core's.
    #[test]
    fn the_json_object_grammar_admits_any_canonical_json_value() {
        let c = compile_json_object_v1();
        assert_eq!(c.frames.len(), 1, "one self-referential frame");
        for v in [
            json!(null),
            json!(true),
            json!(-0.5e-7),
            json!("s\"\\\n\u{1}é😂"),
            json!([]),
            json!({}),
            json!([1, [2, [3, { "a": [null, false, "x"] }]]]),
            json!({ "": "", "k": { "k": { "k": [] } }, "z": 1e21 }),
            json!({ "dup": 1, "other": 2 }),
        ] {
            assert!(accepts(&c, &canonical(&v)), "{v}");
            assert!(accepts(&c, &single_line(&canonical(&v))), "single-line {v}");
        }
        for spaced in [&b"[1, 2]"[..], b"{\"a\": 1, \"b\": [true, null]}", b"{\"name\": \"Tama\", \"age\": 3}"] {
            assert!(accepts(&c, spaced), "{:?}", String::from_utf8_lossy(spaced));
        }
        // Duplicate keys are not tracked here (RFC 8259 permits them; the derivation reads the
        // last).
        assert!(accepts(&c, br#"{"a":1,"a":2}"#));
        for dead in [
            &b" 1"[..],
            b"1 ",
            b"{ }",
            b"[1,  2]",
            b"[ 1]",
            b"[1 ,2]",
            b"[1 ]",
            b"{ \"a\":1}",
            b"{\"a\":1 }",
            b"{\"a\":  1}",
            b"{\"a\":\n1}",
            b"{\"a\":\t1}",
            b"{\"a\":1,\n\"b\":2}",
            b"{\"a\":1} ",
            b"{\"a\" :1}",
            b"01",
            b"1.",
            b".5",
            b"+1",
            b"[1,]",
            b"{\"a\":1,}",
            b"{a:1}",
            b"'s'",
            b"\"\\/\"",
            b"tru",
            b"nul",
            b"True",
            b"[1]]",
            b"{}{}",
        ] {
            assert!(!accepts(&c, dead), "{:?}", String::from_utf8_lossy(dead));
        }
        // Complete-but-open values are live and not accepted.
        for open in [&b"["[..], b"{\"a\"", b"{\"a\":", b"[1,", b"\"abc", b"-", b"1e", b"1e+"] {
            let s = constraint_state_after_v1(&c, [open]).expect("live");
            assert!(!constraint_is_accepting_v1(&c, &s), "{:?}", String::from_utf8_lossy(open));
        }
        // The depth: sixteen pushed frames. `[` × 17 is the seventeenth bracket at stack sixteen;
        // an eighteenth is dead, closing is live.
        let deep = vec![b'['; 17];
        let s = constraint_state_after_v1(&c, [&deep[..]]).unwrap();
        assert_eq!(s.stack.len(), PALW_CONSTRAINT_MAX_DEPTH_V1);
        assert!(constraint_admits_v1(&c, &s, b"[").is_none());
        assert!(constraint_admits_v1(&c, &s, b"1").is_none(), "an item is a push");
        let mut closed = deep.clone();
        closed.extend(std::iter::repeat_n(b']', 17));
        assert!(accepts(&c, &closed));
        // After a complete root value, nothing is admitted: this is how the run's stop fires.
        let done = constraint_state_after_v1(&c, [&b"{\"a\":[1]}"[..]]).unwrap();
        assert!(constraint_is_accepting_v1(&c, &done));
        assert!((0..=255u8).all(|b| constraint_admits_v1(&c, &done, &[b]).is_none()));
        // The same after the single-line spelling: the one space never reaches past the root.
        let done = constraint_state_after_v1(&c, [&b"{\"a\": [1, 2]}"[..]]).unwrap();
        assert!(constraint_is_accepting_v1(&c, &done));
        assert!((0..=255u8).all(|b| constraint_admits_v1(&c, &done, &[b]).is_none()));
        // A root number is complete AND continuable: digits stay live, so its stop is the budget.
        let number = constraint_state_after_v1(&c, [&b"12"[..]]).unwrap();
        assert!(constraint_is_accepting_v1(&c, &number));
        assert!(constraint_admits_v1(&c, &number, b"3").is_some());
        assert!(constraint_admits_v1(&c, &number, b",").is_none());
    }

    /// Keys in any order, each at most once, the required ones present by `}`; an undeclared key
    /// that extends or truncates a declared spelling is an undeclared key; the key mask is per
    /// frame; a key needing escapes is matched by its canonical spelling.
    #[test]
    fn objects_admit_keys_in_any_order_each_at_most_once() {
        let c = compiled(json!({
            "type": "object",
            "properties": { "a": { "type": "integer" }, "ab": { "type": "integer" }, "q\"\\": { "type": "boolean" }, "日本": { "type": "null" } },
            "required": ["a", "日本"],
            "additionalProperties": false
        }));
        for ok in [r#"{"a":1,"日本":null}"#, r#"{"日本":null,"a":1}"#, r#"{"ab":2,"日本":null,"q\"\\":true,"a":1}"#] {
            assert!(accepts(&c, ok.as_bytes()), "{ok:?}");
        }
        for dead in [
            r#"{"a":1,"a":2,"日本":null}"#,
            r#"{"a":1}"#,
            "{}",
            r#"{"abc":1,"a":1,"日本":null}"#,
            r#"{"":1,"a":1,"日本":null}"#,
            r#"{"日":null,"a":1}"#,
            r#"{"q\"":true,"a":1,"日本":null}"#,
            r#"{"a":1,"日本":null,}"#,
            r#"{"a":1,,"日本":null}"#,
        ] {
            assert!(!accepts(&c, dead.as_bytes()), "{dead:?}");
        }
        // Open objects: an undeclared key takes the additional schema; a declared one keeps its
        // own and is still at most once.
        let open = compiled(
            json!({ "type": "object", "properties": { "a": { "type": "integer" } }, "additionalProperties": { "type": "string" } }),
        );
        assert!(accepts(&open, br#"{"b":"x","a":1,"abc":"y","":"z"}"#));
        assert!(!accepts(&open, br#"{"a":"x"}"#));
        assert!(!accepts(&open, br#"{"b":1}"#));
        assert!(!accepts(&open, br#"{"a":1,"a":1}"#));
        assert!(accepts(&open, br#"{"b":"x","b":"y"}"#), "undeclared members are not tracked");
        // Nested objects each keep their own mask.
        let nested = compiled(json!({
            "type": "object",
            "properties": { "o": { "type": "object", "properties": { "o": { "type": "integer" } }, "required": ["o"], "additionalProperties": false } },
            "required": ["o"],
            "additionalProperties": false
        }));
        assert!(accepts(&nested, br#"{"o":{"o":1}}"#));
        assert!(!accepts(&nested, br#"{"o":{}}"#));
        assert!(!accepts(&nested, br#"{"o":{"o":1},"o":{"o":1}}"#));
    }

    /// Lengths count characters, not bytes: a multi-byte character and a `\u00xx` escape are one
    /// each; the bounds are exact at both ends; and `maxLength: 0` is the empty string only.
    #[test]
    fn string_lengths_count_characters() {
        let c = compiled(json!({ "type": "string", "minLength": 2, "maxLength": 3 }));
        for ok in ["\"ab\"", "\"abc\"", "\"日本\"", "\"日本語\"", "\"😂😂\"", "\"a\\n\"", "\"\\u001f\\u000b\\t\""] {
            assert!(accepts(&c, ok.as_bytes()), "{ok:?}");
        }
        for dead in ["\"\"", "\"a\"", "\"abcd\"", "\"日\"", "\"日本語で\"", "\"\\n\"", "\"aaaa"] {
            assert!(!accepts(&c, dead.as_bytes()), "{dead:?}");
        }
        let empty = compiled(json!({ "type": "string", "maxLength": 0 }));
        assert!(accepts(&empty, b"\"\""));
        assert!(!accepts(&empty, b"\"a\""));
        let at_least = compiled(json!({ "type": "string", "minLength": 3 }));
        assert!(!accepts(&at_least, b"\"ab\""));
        assert!(accepts(&at_least, b"\"abc\""));
        assert!(accepts(&at_least, "\"abcdefghijklmnopqrstuvwxyz日本語\"".as_bytes()));
        // A truncated UTF-8 sequence before the closing quote is dead, at any count.
        assert!(constraint_state_after_v1(&at_least, [&[b'"', b'a', b'b', 0xe6, 0x97, b'"'][..]]).is_none());
    }

    /// `minItems` / `maxItems` at their edges, empty arrays, and the unbounded loop.
    #[test]
    fn arrays_count_items() {
        let c = compiled(json!({ "type": "array", "items": { "type": "boolean" }, "minItems": 2, "maxItems": 3 }));
        assert!(accepts(&c, b"[true,false]"));
        assert!(accepts(&c, b"[true,false,true]"));
        assert!(!accepts(&c, b"[true]"));
        assert!(!accepts(&c, b"[true,false,true,false]"));
        assert!(!accepts(&c, b"[]"));
        assert!(!accepts(&c, b"[true,1]"));
        let none = compiled(json!({ "type": "array", "maxItems": 0 }));
        assert!(accepts(&none, b"[]"));
        assert!(!accepts(&none, b"[1]"));
        let unbounded = compiled(json!({ "type": "array", "items": { "type": "integer" } }));
        assert!(accepts(&unbounded, b"[]"));
        let long = format!("[{}]", (0..200).map(|i| i.to_string()).collect::<Vec<_>>().join(","));
        assert!(accepts(&unbounded, long.as_bytes()));
        assert!(!accepts(&unbounded, b"[1,2,]"));
    }

    /// The number grammars: RFC 8259's for `number`, the integer part for `integer`; both stop
    /// exactly where the parent's terminator begins; `minimum`/`maximum` are not enforced.
    #[test]
    fn numbers_follow_the_json_grammar_and_integers_the_integer_grammar() {
        let number = compiled(json!({ "type": "number", "minimum": 0, "maximum": 1 }));
        for ok in ["0", "-0", "1", "-1", "10", "1.5", "0.001", "1e5", "1E-5", "1.25e+7", "123456789.987654321", "5"] {
            assert!(accepts(&number, ok.as_bytes()), "{ok}");
        }
        for dead in ["01", "1.", ".5", "+1", "1e", "1e+", "--1", "0x1", "1_0", "1 ", "NaN", "Infinity", "-"] {
            assert!(!accepts(&number, dead.as_bytes()), "{dead}");
        }
        let integer = compiled(json!({ "type": "integer" }));
        for ok in ["0", "-7", "123456789012345678901234567890"] {
            assert!(accepts(&integer, ok.as_bytes()), "{ok}");
        }
        for dead in ["1.0", "1e2", "01", "-", "1.5"] {
            assert!(!accepts(&integer, dead.as_bytes()), "{dead}");
        }
        // Inside an array the terminator pops the number and the parent reads it.
        let list = compiled(json!({ "type": "array", "items": { "type": "number" } }));
        assert!(accepts(&list, b"[1,2.5,-3e2]"));
        assert!(!accepts(&list, b"[1,2.]"));
        assert!(!accepts(&list, b"[1,]"));
        // `integer` beside `number` is `number`.
        let both = compiled(json!({ "type": ["integer", "number"] }));
        assert!(accepts(&both, b"1.5"));
    }

    /// `enum` and `const` are literal tries of canonical bytes: a member that fails the schema's
    /// other keywords is not a candidate; an object member must be spelled sorted; prefixes of
    /// one another are handled; a schema left with no candidate is refused.
    #[test]
    fn enum_and_const_are_exact_canonical_bytes() {
        let c = compiled(json!({ "enum": [1, 12, 123, "1", { "b": 1, "a": [true] }, null, 2.50] }));
        for ok in [&b"1"[..], b"12", b"123", b"\"1\"", br#"{"a":[true],"b":1}"#, b"null", b"2.5"] {
            assert!(accepts(&c, ok), "{:?}", String::from_utf8_lossy(ok));
        }
        for dead in [&b"1234"[..], b"2", br#"{"b":1,"a":[true]}"#, b"2.50", b"\"12\"", b"true"] {
            assert!(!accepts(&c, dead), "{:?}", String::from_utf8_lossy(dead));
        }
        // Typed: the string members only.
        let typed = compiled(json!({ "type": "string", "enum": ["x", 1, "yy"] }));
        assert!(accepts(&typed, b"\"x\"") && accepts(&typed, b"\"yy\"") && !accepts(&typed, b"1"));
        // A const beside an enum that does not hold it admits nothing; a typed enum with no
        // member of the type admits nothing.
        for bad in [json!({ "enum": ["a"], "const": "b" }), json!({ "type": "integer", "enum": ["a", 1.5] })] {
            let err = compile_v1(&parse(&bad).unwrap()).unwrap_err();
            assert!(err.contains("admits nothing"), "{err}");
        }
    }

    /// Refusals by name: `pattern`, seventeen members, a required-but-forbidden member, and the
    /// byte ceiling.
    #[test]
    fn the_compiler_refuses_by_name() {
        let pattern = compile_v1(&parse(&json!({ "type": "string", "pattern": "^a" })).unwrap()).unwrap_err();
        assert!(pattern.contains("`pattern`") && pattern.contains("Part D"), "{pattern}");
        let mut props = serde_json::Map::new();
        for i in 0..17 {
            props.insert(format!("k{i}"), json!({ "type": "null" }));
        }
        let wide = compile_v1(&parse(&json!({ "type": "object", "properties": props })).unwrap()).unwrap_err();
        assert!(wide.contains("17 members") && wide.contains("sixteen"), "{wide}");
        let forbidden =
            compile_v1(&parse(&json!({ "type": "object", "required": ["x"], "additionalProperties": false })).unwrap()).unwrap_err();
        assert!(forbidden.contains("`required` names \"x\"") && forbidden.contains("admits nothing"), "{forbidden}");
        // The bounds, in the order they bind: a string of a thousand characters is fourteen phase
        // nodes per character — past the byte ceiling; five thousand is past the node ceiling
        // before the bytes are even counted.
        let large = compile_v1(&parse(&json!({ "type": "string", "maxLength": 1000 })).unwrap()).unwrap_err();
        assert!(large.contains("outside consensus-core's bounds") && large.contains("65536-byte ceiling"), "{large}");
        let huge = compile_v1(&parse(&json!({ "type": "string", "maxLength": 5000 })).unwrap()).unwrap_err();
        assert!(huge.contains("more than 65535 nodes"), "{huge}");
        // And where a bounded string DOES fit: two hundred characters, alone, is inside the
        // ceiling — the practical width of one bounded string field on this compiler.
        let fits = compile_v1(&parse(&json!({ "type": "string", "maxLength": 200 })).unwrap()).unwrap();
        assert!(fits.to_bytes().len() <= PALW_CONSTRAINT_MAX_BYTES_V1);
        // Sixteen members compile, and a required member that `properties` does not declare is
        // tracked through the additional schema when one is allowed.
        props.remove("k16");
        assert!(compile_v1(&parse(&json!({ "type": "object", "properties": props })).unwrap()).is_ok());
        let tracked = compiled(json!({ "type": "object", "required": ["x"], "additionalProperties": { "type": "integer" } }));
        assert!(accepts(&tracked, br#"{"x":1}"#));
        assert!(!accepts(&tracked, br#"{"y":1}"#));
        assert!(!accepts(&tracked, br#"{"x":1,"x":2}"#));
    }

    /// **The two spellings of the id are one function**: consensus-core's `constraint_id_v1` and
    /// this crate's `constraint_id`, over compiled bytes and over arbitrary bytes; the compiler's
    /// content name is stable, non-zero, and in every header; and the depth a schema at
    /// `MAX_SCHEMA_DEPTH` reaches is exactly consensus-core's ceiling.
    #[test]
    fn the_constraint_id_spellings_agree_and_the_compiler_is_content_named() {
        let c = compile_json_object_v1();
        let bytes = c.to_bytes();
        assert_eq!(constraint_id_v1(&bytes), crate::constraint_id(&bytes));
        assert_eq!(c.id(), crate::constraint_id(&bytes));
        for arbitrary in [&b""[..], b"x", &[0u8; 1000], bytes.as_slice()] {
            assert_eq!(constraint_id_v1(arbitrary), crate::constraint_id(arbitrary));
        }
        assert_eq!(compiler_id_v1(), compiler_id_v1());
        assert_ne!(compiler_id_v1(), Hash64::default());
        assert_eq!(c.compiler_id, compiler_id_v1());
        assert_eq!(compiled(json!({ "type": "null" })).compiler_id, compiler_id_v1());
        // The same schema compiles to the same bytes, twice.
        assert_eq!(
            compiled(json!({ "type": "array", "items": { "type": "string" } })).to_bytes(),
            compiled(json!({ "type": "array", "items": { "type": "string" } })).to_bytes()
        );

        fn nested(levels: usize) -> Value {
            let mut v = json!({ "type": "string" });
            for _ in 0..levels {
                v = json!({ "type": "object", "properties": { "n": v }, "required": ["n"], "additionalProperties": false });
            }
            v
        }
        let deep = compiled(nested(crate::schema::MAX_SCHEMA_DEPTH));
        let mut text = String::new();
        for _ in 0..crate::schema::MAX_SCHEMA_DEPTH {
            text.push_str("{\"n\":");
        }
        text.push_str("\"leaf\"");
        for _ in 0..crate::schema::MAX_SCHEMA_DEPTH {
            text.push('}');
        }
        assert!(accepts(&deep, text.as_bytes()), "a schema at the pinned depth is reachable");
        let inner = constraint_state_after_v1(&deep, [&text.as_bytes()[..text.len() - crate::schema::MAX_SCHEMA_DEPTH]]).unwrap();
        assert_eq!(inner.stack.len(), PALW_CONSTRAINT_MAX_DEPTH_V1);
    }
}
