//! **The second compiler** — RFC-0001 §2.5 (`Params::palw_fp_constraint_v2`): a parsed second-subset [`Schema`]
//! ([`crate::schema::parse_v2`]) → the same byte-level automaton consensus-core pins ([`PalwDecodeConstraintV1`]), at header
//! version 2, content-named by [`compiler_id_v2`]. A pure function of the schema, like [`crate::compile`]'s.
//!
//! This file is the first compiler's source with the second subset added, kept apart and not edited into it: a compiler is
//! named by the bytes of its file (`compiler_id_*` hashes its own source), and a constraint compiled by the first one must keep
//! saying so. What is added:
//!
//! * **A discriminated `anyOf`/`oneOf`** ([`Builder::union_dfa`]): the union's frame reads `{`, the discriminator's key
//!   and the discriminator's string value through ONE literal trie, and each value's terminal enters the chosen branch's
//!   object DFA at its "after a member" node — `,` and the next key, or `}` and the branch's required mask (which excludes
//!   the discriminator, already read). The discriminator is therefore the object's FIRST member, and a branch is a closed
//!   object (`additionalProperties: false`), so the union is exact: no object any branch rejects is admitted and none any
//!   branch admits (first member first) is lost. Refused by name when the schema is not in that form
//!   ([`crate::schema::parse_v2`]).
//! * **Integer ranges** (`minimum`/`maximum`/`exclusiveMinimum`/`exclusiveMaximum` on `integer`): enumerated as the
//!   canonical spellings of every integer in the range, up to [`MAX_INTEGER_RANGE_VALUES_V2`]; a wider range, and any range on
//!   `number`, is refused by name (a real interval is not a regular language of canonical spellings, and a range that is
//!   only checked after the fact is not a committed one).
//! * **`$ref`** is inlined by the parser; nothing here reads it.
//!
//! Everything the first compiler admits compiles here to the same automaton but for the header version and compiler id,
//! which `a_first_subset_schema_compiles_to_the_first_compilers_automaton` holds.

use std::collections::BTreeMap;

use kaspa_consensus_core::palw_decode_constraint_v1::{
    PALW_CONSTRAINT_KEY_SLOTS_V1, PALW_CONSTRAINT_MAX_FRAMES_V1, PALW_CONSTRAINT_MAX_NODES_V1, PalwConstraintActionV1,
    PalwConstraintEdgeV1, PalwConstraintFrameV1, PalwConstraintNodeV1, PalwDecodeConstraintV1,
};
use kaspa_consensus_core::palw_fp_constraint_v2::{PALW_DECODE_CONSTRAINT_VERSION_V2, palw_constraint_validate_v2_v1};
use kaspa_hashes::Hash64;
use serde_json::Value;

use crate::canonical::to_rfc8785;
use crate::schema::{AdditionalProperties, JsonType, MAX_INTEGER_RANGE_VALUES_V2, Schema, UnionSpec, validate};

/// The keyed-hash domain the compiler's content name is minted under.
pub const PALW_CONSTRAINT_COMPILER_DOMAIN_V2: &[u8] = b"misaka-palw/constraint-compiler/v2";

/// **The compiler's content name** — a keyed BLAKE2b-512 over this file's own source bytes, the
/// way ADR-0078 Decision 3 names a transformer by its source tree. Every constraint this compiler
/// emits carries it in the header, so two automata compiled from one schema by two compiler
/// versions are two constraints, and a change to any line of this file (a comment included) is a
/// new compiler. Read from the source at build time; nothing at run time can move it.
pub fn compiler_id_v2() -> Hash64 {
    kaspa_hashes::blake2b_512_keyed(PALW_CONSTRAINT_COMPILER_DOMAIN_V2, include_bytes!("compile_v2.rs"))
}

/// **Compile a schema in the subset to its automaton.** `Err` names the keyword or bound that
/// stopped it; nothing is approximated.
pub fn compile_v2(schema: &Schema) -> Result<PalwDecodeConstraintV1, String> {
    let mut builder = Builder::default();
    let root = builder.value_frame(schema)?;
    builder.finish(root)
}

/// The grammar of any JSON value (ADR-0096 Decision 3's `json_object`).
pub fn compile_json_object_v2() -> PalwDecodeConstraintV1 {
    compile_v2(&Schema::default()).expect("the grammar of any JSON value is inside every bound")
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
        && s.exclusive_minimum.is_none()
        && s.exclusive_maximum.is_none()
        && s.union.is_none()
}

/// `enum`/`const` as literals: the canonical bytes of every listed value the schema's other
/// keywords accept. `None` when the schema has neither keyword.
fn literal_candidates(schema: &Schema) -> Result<Option<Vec<Vec<u8>>>, String> {
    let range: Vec<Value>;
    let values: Vec<&Value> = match (&schema.constant, &schema.enumeration) {
        (Some(c), _) => vec![c],
        (None, Some(e)) => e.iter().collect(),
        (None, None) => match integer_range(schema)? {
            Some(values) => {
                range = values;
                range.iter().collect()
            }
            None => return Ok(None),
        },
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

/// **An integer range, enumerated** (second subset): when the schema is exactly `integer` with at least one numeric bound,
/// every integer the bounds admit, as values (their canonical spellings become the literals). `None` for a schema with no
/// bound or another type. Refused by name: a range on `number` (or a type list mixing `number` in), a bound that is not
/// finite, and a range wider than [`MAX_INTEGER_RANGE_VALUES_V2`].
fn integer_range(schema: &Schema) -> Result<Option<Vec<Value>>, String> {
    let bounded = schema.minimum.is_some()
        || schema.maximum.is_some()
        || schema.exclusive_minimum.is_some()
        || schema.exclusive_maximum.is_some();
    if !bounded {
        return Ok(None);
    }
    let types = schema.types.as_deref().unwrap_or(&[]);
    if types.contains(&JsonType::Number) {
        return Err(
            "a numeric range on `number` is not compiled: a real interval is not a regular language of canonical spellings, and a range \
             that is only checked after the fact is not a committed one — use `integer` with a bounded range, or `enum`"
                .to_string(),
        );
    }
    if types != [JsonType::Integer] {
        return Err("a numeric range compiles only on a schema whose `type` is exactly `integer`".to_string());
    }
    let lo = [schema.minimum.map(f64::ceil), schema.exclusive_minimum.map(|m| m.floor() + 1.0)]
        .into_iter()
        .flatten()
        .fold(None, |acc: Option<f64>, v| Some(acc.map_or(v, |a| a.max(v))));
    let hi = [schema.maximum.map(f64::floor), schema.exclusive_maximum.map(|m| m.ceil() - 1.0)]
        .into_iter()
        .flatten()
        .fold(None, |acc: Option<f64>, v| Some(acc.map_or(v, |a| a.min(v))));
    let (Some(lo), Some(hi)) = (lo, hi) else {
        return Err("an integer range compiles only with BOTH a lower and an upper bound (an unbounded side is not a finite set of literals)".to_string());
    };
    if !lo.is_finite() || !hi.is_finite() || lo.abs() > 9.0e15 || hi.abs() > 9.0e15 {
        return Err("an integer bound is past 9e15, beyond what a double carries exactly".to_string());
    }
    if lo > hi {
        return Err("the integer range admits no integer".to_string());
    }
    let (lo, hi) = (lo as i64, hi as i64);
    let width = (hi - lo) as u64 + 1;
    if width > MAX_INTEGER_RANGE_VALUES_V2 {
        return Err(format!(
            "the integer range admits {width} values, past the {MAX_INTEGER_RANGE_VALUES_V2} the compiler enumerates (each is a literal of the automaton); narrow it or list the values"
        ));
    }
    Ok(Some((lo..=hi).map(|n| Value::from(n)).collect()))
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
            PalwDecodeConstraintV1 { version: PALW_DECODE_CONSTRAINT_VERSION_V2, compiler_id: compiler_id_v2(), start_frame, frames };
        palw_constraint_validate_v2_v1(&constraint).map_err(|e| format!("the compiled automaton is outside the second form's bounds: {e}"))?;
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
        if let Some(union) = &schema.union {
            return self.union_dfa(f, start, done, union);
        }
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
        }
        let mut trie = vec![Tn { children: BTreeMap::new(), terminal: false }];
        for literal in literals {
            if literal.is_empty() {
                return Err("an empty literal admits the empty answer, which is no JSON value".to_string());
            }
            let mut at = 0usize;
            for &byte in literal {
                let fresh = trie.len();
                let child = *trie[at].children.entry(byte).or_insert(fresh);
                if child == fresh {
                    trie.push(Tn { children: BTreeMap::new(), terminal: false });
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
                self.push_all(f, next, items, resume);
            }
        }
        Ok(())
    }

    /// `{` members `}`: a key trie over the tracked members' canonical spellings, each terminal
    /// setting its key bit, the closing `}` checking the required mask; undeclared members fall
    /// through to a generic string key when `additionalProperties` allows them.
    fn object_dfa(&mut self, f: u16, start: u16, done: u16, schema: &Schema) -> Result<(), String> {
        self.object_dfa_entry(f, start, done, schema).map(|_| ())
    }

    /// [`Self::object_dfa`], returning its "after a member's value" node: where a union's branch enters once its
    /// discriminator (the object's first member) has been read.
    fn object_dfa_entry(&mut self, f: u16, start: u16, done: u16, schema: &Schema) -> Result<u16, String> {
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

        // The generic key string, for undeclared members: one node per phase, the closing quote
        // leading to the additional value.
        let mut generic: BTreeMap<Ph, u16> = BTreeMap::new();
        let mut add_colon = None;
        if let Some(frame) = additional_frame {
            let colon = self.node(f, false)?;
            let value = self.node(f, false)?;
            self.set(f, colon, b':', Action::Goto(value))?;
            self.push_all(f, value, frame, after_value);
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
            self.push_all(f, value, *frame, after_value);
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
        Ok(after_value)
    }

    /// **A discriminated union** (see the module note): `{`, the discriminator key, `:`, then the discriminator's string value
    /// through one trie whose terminals enter each branch's object DFA after its first member.
    fn union_dfa(&mut self, f: u16, start: u16, done: u16, union: &UnionSpec) -> Result<(), String> {
        // Each branch's tail: its object without the discriminator (declared and required there, already read here).
        let mut entries: Vec<u16> = Vec::with_capacity(union.branches.len());
        for branch in &union.branches {
            let mut tail = branch.schema.clone();
            tail.properties.retain(|(name, _)| name != &union.discriminator);
            tail.required.retain(|name| name != &union.discriminator);
            let unused_start = self.node(f, false)?;
            entries.push(self.object_dfa_entry(f, unused_start, done, &tail)?);
        }
        // The prefix every branch shares: `{`, the key's canonical spelling (quotes included), `:`.
        let key = to_rfc8785(&Value::String(union.discriminator.clone()))?;
        let mut prefix = vec![b'{'];
        prefix.extend_from_slice(&key);
        prefix.push(b':');
        let mut at = start;
        for &byte in &prefix {
            let next = self.node(f, false)?;
            self.set(f, at, byte, Action::Goto(next))?;
            at = next;
        }
        // The value trie: one path per branch's canonical spelling, its last byte entering the branch.
        struct Tn {
            children: BTreeMap<u8, usize>,
            enter: Option<u16>,
        }
        let mut trie = vec![Tn { children: BTreeMap::new(), enter: None }];
        for (branch, &entry) in union.branches.iter().zip(&entries) {
            let spelled = to_rfc8785(&Value::String(branch.value.clone()))?;
            let mut node = 0usize;
            for &byte in &spelled {
                let fresh = trie.len();
                let child = *trie[node].children.entry(byte).or_insert(fresh);
                if child == fresh {
                    trie.push(Tn { children: BTreeMap::new(), enter: None });
                }
                node = child;
            }
            if trie[node].enter.is_some() {
                return Err(format!("compiler invariant: two branches spell the discriminator value {:?} alike", branch.value));
            }
            trie[node].enter = Some(entry);
        }
        let mut ids = vec![at];
        for _ in trie.iter().skip(1) {
            ids.push(self.node(f, false)?);
        }
        for (tn, &from) in trie.iter().zip(ids.iter()) {
            for (&byte, &child) in &tn.children {
                self.set(f, from, byte, Action::Goto(ids[child]))?;
            }
        }
        // A terminal's node IS the branch's entry (the string's closing quote is the trie's last byte): re-point the edge
        // that reached it. The last byte of a spelled value is the closing quote, so its parent's edge is replaced.
        for (child_index, tn) in trie.iter().enumerate() {
            if let Some(entry) = tn.enter {
                for (parent_index, parent) in trie.iter().enumerate() {
                    for (&byte, &child) in &parent.children {
                        if child == child_index {
                            let from = ids[parent_index];
                            self.frames[f as usize].nodes[from as usize].table[byte as usize] = Some(Action::Goto(entry));
                        }
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
    use crate::schema::{parse, parse_v2, validate};
    use kaspa_consensus_core::palw_decode_constraint_v1::{constraint_is_accepting_v1, constraint_state_after_v1};
    use serde_json::json;

    fn compiled(schema: Value) -> PalwDecodeConstraintV1 {
        let parsed = parse_v2(&schema).unwrap_or_else(|e| panic!("{schema} should parse: {e}"));
        compile_v2(&parsed).unwrap_or_else(|e| panic!("{schema} should compile: {e}"))
    }

    fn refused(schema: Value) -> String {
        match parse_v2(&schema) {
            Err(e) => e,
            Ok(parsed) => compile_v2(&parsed).expect_err(&format!("{schema} must be refused")),
        }
    }

    fn accepts(c: &PalwDecodeConstraintV1, bytes: &[u8]) -> bool {
        constraint_state_after_v1(c, [bytes]).is_some_and(|s| constraint_is_accepting_v1(c, &s))
    }

    fn shapes() -> Value {
        json!({ "oneOf": [
            { "type": "object", "properties": { "kind": { "const": "circle" }, "r": { "type": "integer", "minimum": 1, "maximum": 9 } },
              "required": ["kind", "r"], "additionalProperties": false },
            { "type": "object", "properties": { "kind": { "const": "rect" }, "w": { "type": "integer", "minimum": 0, "maximum": 3 }, "h": { "type": "integer", "minimum": 0, "maximum": 3 } },
              "required": ["kind", "w", "h"], "additionalProperties": false },
            { "type": "object", "properties": { "kind": { "const": "dot" }, "label": { "type": "string", "maxLength": 3 } },
              "required": ["kind"], "additionalProperties": false }
        ] })
    }

    /// **The union is exact where the discriminator is the first member**: every canonical object in a small cross product
    /// of tags and members, spelled discriminator-first, is accepted by the automaton exactly when the second subset's own
    /// validator accepts it; and the spellings the subset does not carry (discriminator not first, a repeated
    /// discriminator, whitespace, an unknown tag, a member of another branch, a missing required one) are dead or incomplete.
    #[test]
    fn a_discriminated_union_is_exactly_its_branches() {
        let schema = shapes();
        let parsed = parse_v2(&schema).unwrap();
        let c = compile_v2(&parsed).unwrap();
        let mut checked = 0;
        for kind in ["circle", "rect", "dot", "tri"] {
            for r in [None, Some(0), Some(3), Some(10)] {
                for w in [None, Some(2), Some(4)] {
                    for h in [None, Some(0)] {
                        for label in [None, Some("ab"), Some("abcd")] {
                            let mut members = vec![format!("\"kind\":\"{kind}\"")];
                            let mut object = serde_json::Map::new();
                            object.insert("kind".into(), json!(kind));
                            if let Some(r) = r {
                                members.push(format!("\"r\":{r}"));
                                object.insert("r".into(), json!(r));
                            }
                            if let Some(w) = w {
                                members.push(format!("\"w\":{w}"));
                                object.insert("w".into(), json!(w));
                            }
                            if let Some(h) = h {
                                members.push(format!("\"h\":{h}"));
                                object.insert("h".into(), json!(h));
                            }
                            if let Some(l) = label {
                                members.push(format!("\"label\":\"{l}\""));
                                object.insert("label".into(), json!(l));
                            }
                            let text = format!("{{{}}}", members.join(","));
                            let valid = validate(&parsed, &Value::Object(object)).is_ok();
                            assert_eq!(accepts(&c, text.as_bytes()), valid, "{text}");
                            checked += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(checked, 4 * 4 * 3 * 2 * 3);
        // Members after the discriminator may come in any order.
        assert!(accepts(&c, br#"{"kind":"rect","h":2,"w":1}"#) && accepts(&c, br#"{"kind":"rect","w":1,"h":2}"#));
        // What the subset does not carry.
        for dead in [
            &br#"{"r":3,"kind":"circle"}"#[..],        // the discriminator is the first member
            br#"{"kind":"circle","kind":"circle","r":3}"#, // a repeated discriminator
            br#"{ "kind":"dot"}"#,                     // whitespace
            br#"{"kind":"tri"}"#,                      // no such branch
            br#"{"kind":"dot","r":3}"#,                // another branch's member
            br#"{"kind":"circle","r":3,"r":4}"#,       // a repeated member
            br#"[{"kind":"dot"}]"#,                    // not an object
        ] {
            assert!(!accepts(&c, dead), "{}", String::from_utf8_lossy(dead));
        }
        assert!(!accepts(&c, br#"{"kind":"circle"}"#), "a missing required member never completes");
        // `oneOf` and `anyOf` are one automaton: the keyword is the author's spelling, the branches are disjoint by tag.
        let any_of = json!({ "anyOf": schema["oneOf"].clone() });
        assert_eq!(compiled(any_of).to_bytes(), c.to_bytes());
    }

    /// A union nests anywhere a value does: in an array's items and in a member, beside other members.
    #[test]
    fn a_union_nests_in_arrays_and_members() {
        let c = compiled(json!({ "type": "object", "properties": {
            "shapes": { "type": "array", "items": shapes(), "maxItems": 2 },
            "n": { "type": "integer", "minimum": 1, "maximum": 2 } },
            "required": ["shapes"], "additionalProperties": false }));
        assert!(accepts(&c, br#"{"shapes":[{"kind":"dot"},{"kind":"circle","r":4}],"n":2}"#));
        assert!(accepts(&c, br#"{"shapes":[]}"#));
        assert!(!accepts(&c, br#"{"shapes":[{"kind":"dot"},{"kind":"dot"},{"kind":"dot"}]}"#));
        assert!(!accepts(&c, br#"{"shapes":[{"kind":"circle","r":4}],"n":3}"#));
    }

    /// `$ref` is inlined: the automaton is the one the inlined schema compiles to, byte for byte.
    #[test]
    fn a_non_recursive_ref_compiles_to_its_inlined_schema() {
        let referenced = json!({
            "$defs": { "point": { "type": "object", "properties": { "x": { "type": "integer", "minimum": 0, "maximum": 5 }, "y": { "type": "integer", "minimum": 0, "maximum": 5 } }, "required": ["x", "y"], "additionalProperties": false } },
            "type": "object", "properties": { "a": { "$ref": "#/$defs/point" }, "b": { "$ref": "#/$defs/point" } }, "required": ["a", "b"], "additionalProperties": false
        });
        let point = referenced["$defs"]["point"].clone();
        let inlined = json!({ "type": "object", "properties": { "a": point.clone(), "b": point }, "required": ["a", "b"], "additionalProperties": false });
        assert_eq!(compiled(referenced).to_bytes(), compiled(inlined).to_bytes());
        // A reference inside a union's branch resolves too.
        let c = compiled(json!({
            "$defs": { "tag": { "const": "p" } },
            "oneOf": [
                { "type": "object", "properties": { "kind": { "$ref": "#/$defs/tag" } }, "required": ["kind"], "additionalProperties": false },
                { "type": "object", "properties": { "kind": { "const": "q" } }, "required": ["kind"], "additionalProperties": false }
            ]
        }));
        assert!(accepts(&c, br#"{"kind":"p"}"#) && accepts(&c, br#"{"kind":"q"}"#) && !accepts(&c, br#"{"kind":"r"}"#));
    }

    #[test]
    fn a_ref_that_is_recursive_missing_or_outside_the_defs_is_refused_by_name() {
        let tree = json!({ "$defs": { "node": { "type": "object", "properties": { "next": { "$ref": "#/$defs/node" } } } }, "$ref": "#/$defs/node" });
        let r = refused(tree);
        assert!(r.contains("is recursive"), "{r}");
        let mutual = json!({ "$defs": { "a": { "$ref": "#/$defs/b" }, "b": { "$ref": "#/$defs/a" } }, "$ref": "#/$defs/a" });
        assert!(refused(mutual).contains("is recursive"));
        assert!(refused(json!({ "$ref": "#/$defs/nope" })).contains("names no entry of `$defs`"));
        assert!(refused(json!({ "$defs": { "a": { "type": "null" } }, "$ref": "https://example.com/schema.json" })).contains("not a `#/$defs/<name>` reference"));
        assert!(refused(json!({ "$defs": { "a": { "type": "null" } }, "$ref": "#/$defs/a", "type": "null" })).contains("beside `$ref`"));
        assert!(refused(json!({ "type": "object", "properties": { "x": { "$defs": {} } } })).contains("definitions live at the schema's root only"));
        // A diamond past the expansion cap.
        let mut defs = serde_json::Map::new();
        defs.insert("l0".into(), json!({ "type": "null" }));
        for level in 1..=7 {
            let below = format!("#/$defs/l{}", level - 1);
            defs.insert(
                format!("l{level}"),
                json!({ "type": "array", "items": { "$ref": below.clone() }, "maxItems": 2, "properties": { "a": { "$ref": below.clone() }, "b": { "$ref": below } } }),
            );
        }
        let why = refused(json!({ "$defs": Value::Object(defs), "$ref": "#/$defs/l7" }));
        assert!(why.contains("expands more than 64 references"), "{why}");
    }

    #[test]
    fn an_integer_range_is_enumerated_exactly_and_everything_else_is_refused() {
        let c = compiled(json!({ "type": "integer", "minimum": -2, "maximum": 3 }));
        for n in -2..=3 {
            assert!(accepts(&c, n.to_string().as_bytes()), "{n}");
        }
        for n in [-3, 4, 10, 100] {
            assert!(!accepts(&c, n.to_string().as_bytes()), "{n}");
        }
        assert!(!accepts(&c, b"01") && !accepts(&c, b"-0") && !accepts(&c, b"1.0") && !accepts(&c, b"+1"), "only canonical spellings");
        let exclusive = compiled(json!({ "type": "integer", "exclusiveMinimum": 0, "exclusiveMaximum": 3 }));
        assert!(accepts(&exclusive, b"1") && accepts(&exclusive, b"2") && !accepts(&exclusive, b"0") && !accepts(&exclusive, b"3"));
        let mixed = compiled(json!({ "type": "integer", "minimum": 0.5, "exclusiveMaximum": 3.5 }));
        assert!(!accepts(&mixed, b"0") && accepts(&mixed, b"1") && accepts(&mixed, b"3") && !accepts(&mixed, b"4"), "fractional bounds round inward");
        // Inside an object, with the terminators the parent reads.
        let object = compiled(json!({ "type": "object", "properties": { "n": { "type": "integer", "minimum": 1, "maximum": 12 } }, "required": ["n"], "additionalProperties": false }));
        assert!(accepts(&object, br#"{"n":12}"#) && accepts(&object, br#"{"n":1}"#) && !accepts(&object, br#"{"n":13}"#) && !accepts(&object, br#"{"n":0}"#));
        for (schema, needle) in [
            (json!({ "type": "number", "minimum": 0, "maximum": 1 }), "a numeric range on `number` is not compiled"),
            (json!({ "type": "integer", "minimum": 0 }), "BOTH a lower and an upper bound"),
            (json!({ "type": "integer", "minimum": 0, "maximum": 5000 }), "past the 1024 the compiler enumerates"),
            (json!({ "type": ["integer", "null"], "minimum": 0, "maximum": 5 }), "exactly `integer`"),
            (json!({ "type": "integer", "minimum": 5, "maximum": 1 }), "admits nothing"),
            (json!({ "type": "integer", "exclusiveMinimum": 3, "exclusiveMaximum": 4 }), "admits no integer"),
        ] {
            assert!(refused(schema.clone()).contains(needle), "{schema}: {}", refused(schema.clone()));
        }
    }

    #[test]
    fn a_union_outside_the_discriminated_form_is_refused_by_name() {
        let branch = |tag: Value, open: bool| {
            let mut b = json!({ "type": "object", "properties": { "kind": tag }, "required": ["kind"] });
            if !open {
                b["additionalProperties"] = json!(false);
            }
            b
        };
        let cases = [
            (json!({ "oneOf": [branch(json!({ "const": "a" }), false)] }), "has 1 branches"),
            (json!({ "oneOf": [branch(json!({ "const": "a" }), true), branch(json!({ "const": "b" }), true)] }), "is not closed"),
            (json!({ "oneOf": [branch(json!({ "const": "a" }), false), branch(json!({ "const": "a" }), false)] }), "share the discriminator value"),
            (json!({ "oneOf": [branch(json!({ "type": "string" }), false), branch(json!({ "type": "string" }), false)] }), "is not discriminated"),
            (json!({ "oneOf": [branch(json!({ "const": "a" }), false), { "type": "string" }] }), "is not `type: \"object\"`"),
            (json!({ "oneOf": [branch(json!({ "const": "a" }), false), branch(json!({ "const": "b" }), false)], "anyOf": [] }), "both at"),
            (json!({ "type": "object", "required": ["z"], "oneOf": [branch(json!({ "const": "a" }), false), branch(json!({ "const": "b" }), false)] }), "carries a sibling keyword"),
            (json!({ "oneOf": 3 }), "where a list of schemas was expected"),
        ];
        for (schema, needle) in cases {
            let why = refused(schema.clone());
            assert!(why.contains(needle), "{schema}: {why}");
        }
        // Seventeen branches.
        let many: Vec<Value> = (0..17).map(|i| branch(json!({ "const": format!("t{i}") }), false)).collect();
        assert!(refused(json!({ "oneOf": many })).contains("has 17 branches"));
        // The first subset still refuses the keywords by name (nothing was loosened).
        let first = parse(&shapes()).unwrap_err();
        assert!(first.contains("`oneOf`") && first.contains("outside the JSON-Schema subset"), "{first}");
        assert!(parse(&json!({ "$ref": "#/$defs/a" })).unwrap_err().contains("`$ref`"));
        assert!(parse(&json!({ "type": "integer", "exclusiveMinimum": 1 })).unwrap_err().contains("`exclusiveMinimum`"));
    }

    /// Everything the first compiler admits compiles here to the same automaton but for its header version and the
    /// compiler that named it; and the second compiler's name is its own.
    #[test]
    fn a_first_subset_schema_compiles_to_the_first_compilers_automaton() {
        for schema in [
            json!({ "type": "object", "properties": { "a": { "type": "string", "maxLength": 4 }, "b": { "type": "array", "items": { "type": "boolean" }, "maxItems": 3 } }, "required": ["a"] }),
            json!({ "type": ["integer", "null"] }),
            json!({ "enum": ["x", "y", 3] }),
            json!({}),
        ] {
            let first = crate::compile::compile_v1(&parse(&schema).unwrap()).unwrap();
            let second = compile_v2(&parse_v2(&schema).unwrap()).unwrap();
            assert_eq!(first.frames, second.frames, "{schema}");
            assert_eq!(first.start_frame, second.start_frame);
            assert_eq!(second.version, 2);
            assert_ne!(first.compiler_id, second.compiler_id, "the compilers are named apart");
            assert_eq!(second.compiler_id, compiler_id_v2());
            assert_ne!(first.id(), second.id());
        }
        assert_ne!(compiler_id_v2(), crate::compile::compiler_id_v1());
        assert_ne!(compiler_id_v2(), Hash64::default());
        let any = compile_json_object_v2();
        assert_eq!(any.compiler_id, compiler_id_v2());
        assert!(accepts(&any, br#"{"a":[1,{"b":null}]}"#));
    }

    /// The compiled automaton is admitted by the chain's second-form rule and refused by the first's.
    #[test]
    fn the_compiled_automaton_is_a_second_form_constraint() {
        use kaspa_consensus_core::palw_fp_constraint_v2::{PalwConstraintFormErrorV1, palw_constraint_admitted_v1};
        let c = compiled(shapes());
        let bytes = c.to_bytes();
        assert_eq!(palw_constraint_admitted_v1(&bytes, true).unwrap(), c);
        assert_eq!(palw_constraint_admitted_v1(&bytes, false), Err(PalwConstraintFormErrorV1::SecondFormNotArmed));
        assert_eq!(c.id(), crate::constraint_id(&bytes));
    }
}
