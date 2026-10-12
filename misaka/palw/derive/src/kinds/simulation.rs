//! Kind `simulation` (ADR-0078 Decision 8, row 7): "scenario DSL: entities, integer rules, a
//! seed, a step count → integer step simulator → a trace + a summary; determinism basis: an
//! integer state machine — the first candidate for Decision 7". This module is that row: the
//! grammar `simulation/v1`, the transformer `simulation/trace/v1`, and the canonical writer of
//! the `MSIM` artifact.
//!
//! Why the artifact is a hash chain and not only a final state: Decision 7 opens the door to
//! weight for "a transformer whose computation is an integer step space — every operation a
//! catalogued kernel, every intermediate committable". A simulator's intermediates are its
//! states, so every state this transformer passes through is spelled as one byte string and
//! committed in a chain, `h_i = H(h_{i-1} ‖ i ‖ state_i)`. The trace is what a step-space
//! court would check one link at a time; nothing in it weighs anything today (X5).
//!
//! ## The grammar `simulation/v1`
//!
//! A JSON object with exactly these keys (an unknown or a missing key is a grammar refusal):
//!
//! ```text
//! v        1
//! seed     0..=2^64-1        drives the splitmix64 stream (the only randomness there is)
//! steps    1..=100000
//! width    1..=256
//! height   1..=256
//! rules    {"kind":"life","birth":[0..=8 …],"survive":[0..=8 …]}
//!          | {"kind":"agents","bounce":bool,"jitter":0..=16}
//! cells    [[x,y] …]  0..=65536 entries, in range, no duplicates; empty unless life
//! entities [{"id","x","y","vx","vy","attrs"} …]  0..=4096 entries; empty unless agents
//!            id: 1..=64 bytes, unique; x,y in range; vx,vy in -64..=64;
//!            attrs: {name: i64} with at most 16 names of 1..=64 bytes each
//! ```
//!
//! Canonical form: `canon_json`'s (sorted keys, no whitespace, integers only) plus the order
//! of the sets the grammar declares — `birth`/`survive` ascending, `cells` ascending by
//! `[x, y]`, `entities` ascending by `id` — because none of those orders means anything (the
//! transition visits entities by id, never by position in the list), and a canonical form has
//! one spelling per meaning. Nothing else is touched (Decision 2).
//!
//! ## The state machine
//!
//! `life`: a toroidal grid `width × height` of `u8` cells, row-major (`y * width + x`); a
//! step counts the eight neighbours of every cell (wrapping), and a cell is 1 next step if it
//! was 0 and its count is in `birth`, or it was 1 and its count is in `survive` (Conway is
//! B3/S23). State bytes: `width u32 ‖ height u32 ‖ grid`.
//!
//! `agents`: a list of entities in id order. A step visits each entity in that order: with
//! `jitter > 0` it draws two `u64` from the splitmix64 stream and adds
//! `(draw mod (2·jitter+1)) − jitter` to `vx` and then, from the second draw, to `vy`,
//! clamping each to `-64..=64`; then it moves by its velocity on each axis. With `bounce`,
//! a lane of `len` cells has walls at −½ and `len − ½`: the unfolded target `u = pos + v` is
//! reflected across every wall it crosses (position `m` if `m < len` else `2·len − 1 − m`,
//! with `m = u mod 2·len`), and the velocity component is negated once per crossing (an odd
//! number of walls flips it). Without `bounce` positions wrap (`u mod len`). Each wall
//! crossing (bounce) or lane crossing (wrap) is one wall event in the summary. State bytes:
//! `count u32`, then per entity `id (u32 len ‖ bytes) ‖ x ‖ y ‖ vx ‖ vy (i64 le) ‖ attr
//! count u32 ‖ (name ‖ value i64 le)…` in name order.
//!
//! ## The trace
//!
//! `h_i = BLAKE2b-512_keyed("misaka-palw/derive/simulation/step/v1")(h_{i−1} ‖ i (u64 le) ‖
//! state_i bytes)` for `i = 0..=steps`, with `h_{−1}` sixty-four zero bytes, so `h_0` commits
//! the initial state under the same link function as every other step.
//!
//! ## The artifact `MSIM` v1 (little-endian; strings are `u32` length + bytes)
//!
//! ```text
//! "MSIM"  u16 version=1  u8 rules (0 life, 1 agents)  u64 seed  u32 steps  u32 width  u32 height
//! u32 count = steps+1, then count × 64-byte h_i
//! summary: life   → u32 population per step (count values)
//!          agents → u64 Σ|vx|+|vy| per step (count values), then u64 wall events in total
//! final state bytes
//! u32 CRC-32 over everything before it
//! ```
//!
//! The artifact's length is a function of the scenario alone (`artifact_len`), so the 64 MiB
//! ceiling is checked before a single step runs; the grammar's bounds keep the largest run
//! well under it, and the ceiling is stated so a reader need not derive that.
//!
//! ## Discipline (Decision 3, X3)
//!
//! Integer arithmetic on explicit `u64`/`i64`/`u32`/`u8` types, no floating point on any
//! path (a test scans this file for the two float type names), no clock, no OS randomness,
//! no I/O, and no hash-map iteration — every ordered thing is a `Vec` or a `BTreeMap`.

use crate::bytes::{put_i64_le, put_u16_le, put_u32_le, put_u64_le};
use crate::canon_json::{CanonValue, parse_canonical, write_canonical};
use crate::checksum::crc32;
use crate::{Artifact, DeriveError, Discipline, Grammar, Transformer, TransformerManifest};
use kaspa_consensus_core::palw_derived_v1::kind;
use std::collections::BTreeMap;

pub const GRAMMAR_NAME: &str = "simulation/v1";
pub const TRANSFORMER_NAME: &str = "simulation/trace/v1";
pub const WRITER_NAME: &str = "misaka-sim-trace/1/canonical-v1";
/// The key of every link in the trace.
pub const STEP_DOMAIN: &[u8] = b"misaka-palw/derive/simulation/step/v1";
pub const MAGIC: &[u8; 4] = b"MSIM";
pub const ARTIFACT_VERSION: u16 = 1;
pub const RULES_LIFE: u8 = 0;
pub const RULES_AGENTS: u8 = 1;
pub const MAX_STEPS: u32 = 100_000;
pub const MAX_DIM: u32 = 256;
pub const MAX_CELLS: usize = 65_536;
pub const MAX_ENTITIES: usize = 4096;
pub const MAX_ATTRS: usize = 16;
pub const MAX_NAME_BYTES: usize = 64;
pub const MAX_SPEED: i64 = 64;
pub const MAX_JITTER: u8 = 16;
/// **ADR-0078 SA-2's `max_dsl_bytes`.** The most answer bytes this kind will look at, checked on
/// the byte COUNT before the parser is asked what the bytes spell — a JSON parser is an allocator
/// driven by its input, and a bound applied after parsing is applied after the damage. Exceeding
/// it is "no object" (Decision 2's parse-failure arm, X4), never a repair and never a truncation.
///
/// The number is the retention payload's own cap (`PALW_FP_DSL_V1_MAX_BYTES`): a DSL above it
/// could not be served to a verifier under Decision 6 even if it derived, so deriving from one
/// would be building a derivation nobody could check. This kind's schema admits documents larger
/// than that in its extreme corner (4,096 entities of 16 attributes), and this ceiling is the
/// binding one — it is far above any answer a class at these widths emits, and far below
/// what a parser could be made to allocate.
pub const MAX_DSL_BYTES: u64 = kaspa_consensus_core::palw_derived_v1::PALW_FP_DSL_V1_MAX_BYTES as u64;

pub const MAX_ARTIFACT_BYTES: usize = 64 * 1024 * 1024;
/// What precedes `h_0`: a link with no predecessor, spelled as sixty-four zero bytes.
pub const GENESIS_PREV: [u8; 64] = [0u8; 64];
pub const MEDIA_TYPE: &str = "application/vnd.misaka.msim";
pub const EXTENSION: &str = "msim";

/// This kind's grammar and transformer, as the registry sees them.
pub fn register() -> (Vec<Box<dyn Grammar>>, Vec<Box<dyn Transformer>>) {
    (vec![Box::new(SimulationGrammar)], vec![Box::new(SimulationTraceTransformer)])
}

/// The grammar `simulation/v1`: parse, validate the schema above, re-emit canonically.
pub struct SimulationGrammar;

impl Grammar for SimulationGrammar {
    fn name(&self) -> &'static str {
        GRAMMAR_NAME
    }

    fn canonicalize(&self, answer: &[u8]) -> Result<Vec<u8>, DeriveError> {
        crate::check_dsl_bytes(MAX_DSL_BYTES, answer)?;
        let scenario = Scenario::from_canon(&parse_canonical(answer)?)?;
        Ok(write_canonical(&scenario.to_canon()))
    }
}

/// The transformer `simulation/trace/v1`: canonical scenario bytes in, an `MSIM` artifact out.
pub struct SimulationTraceTransformer;

impl Transformer for SimulationTraceTransformer {
    fn manifest(&self) -> TransformerManifest {
        TransformerManifest {
            name: TRANSFORMER_NAME,
            kind: kind::SIMULATION,
            grammar: GRAMMAR_NAME,
            discipline: Discipline::Integer,
            writer: WRITER_NAME,
            source_tree_sha256: crate::SOURCE_TREE_SHA256_HEX,
            // ADR-0078 SA-2: the ceilings this kind enforces, each already a constant above.
            max_dsl_bytes: MAX_DSL_BYTES,
            max_artifact_bytes: MAX_ARTIFACT_BYTES as u64,
            max_steps: MAX_STEPS as u64,
        }
    }

    /// Re-canonicalizes and refuses anything but canonical bytes (a transformer repairs
    /// nothing), then simulates and writes.
    fn run(&self, dsl: &[u8]) -> Result<Artifact, DeriveError> {
        crate::check_dsl_bytes(MAX_DSL_BYTES, dsl)?;
        let scenario = Scenario::from_canon(&parse_canonical(dsl)?)?;
        if write_canonical(&scenario.to_canon()) != dsl {
            return Err(DeriveError::Transformer("input is not canonical simulation/v1 bytes".into()));
        }
        Ok(Artifact { bytes: derive_artifact(&scenario)?, media_type: MEDIA_TYPE, extension: EXTENSION })
    }
}

// ─── the scenario ──────────────────────────────────────────────────────────────────────────

/// The integer rules a scenario runs under.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rules {
    /// Neighbour counts, each `0..=8`, ascending, unique.
    Life {
        birth: Vec<u8>,
        survive: Vec<u8>,
    },
    Agents {
        bounce: bool,
        jitter: u8,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entity {
    pub id: String,
    pub x: i64,
    pub y: i64,
    pub vx: i64,
    pub vy: i64,
    pub attrs: BTreeMap<String, i64>,
}

/// A validated scenario. Its canonical bytes are `write_canonical(&scenario.to_canon())`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scenario {
    pub seed: u64,
    pub steps: u32,
    pub width: u32,
    pub height: u32,
    pub rules: Rules,
    /// Ascending by `(x, y)`, unique; empty unless `rules` is life.
    pub cells: Vec<(u32, u32)>,
    /// Ascending by id, unique; empty unless `rules` is agents.
    pub entities: Vec<Entity>,
}

fn grammar(msg: impl Into<String>) -> DeriveError {
    DeriveError::Grammar(msg.into())
}

/// Exactly `keys`, no more and no fewer.
fn expect_keys(obj: &BTreeMap<String, CanonValue>, what: &str, keys: &[&str]) -> Result<(), DeriveError> {
    for k in obj.keys() {
        if !keys.contains(&k.as_str()) {
            return Err(grammar(format!("{what} has unknown key {k:?}")));
        }
    }
    for k in keys {
        if !obj.contains_key(*k) {
            return Err(grammar(format!("{what} is missing key {k:?}")));
        }
    }
    Ok(())
}

fn int_in(v: &CanonValue, what: &str, lo: i64, hi: i64) -> Result<i64, DeriveError> {
    match v.as_i64() {
        Some(i) if (lo..=hi).contains(&i) => Ok(i),
        _ => Err(grammar(format!("{what} must be an integer in {lo}..={hi}"))),
    }
}

impl Scenario {
    /// Validate a parsed tree against the grammar. Every refusal names its reason.
    pub fn from_canon(v: &CanonValue) -> Result<Self, DeriveError> {
        let obj = v.as_obj().ok_or_else(|| grammar("the scenario must be a JSON object"))?;
        expect_keys(obj, "the scenario", &["v", "seed", "steps", "width", "height", "rules", "cells", "entities"])?;
        if obj["v"].as_i64() != Some(1) {
            return Err(grammar("v must be 1"));
        }
        let seed = obj["seed"].as_u64().ok_or_else(|| grammar(format!("seed must be an integer in 0..={}", u64::MAX)))?;
        let steps = int_in(&obj["steps"], "steps", 1, i64::from(MAX_STEPS))? as u32;
        let width = int_in(&obj["width"], "width", 1, i64::from(MAX_DIM))? as u32;
        let height = int_in(&obj["height"], "height", 1, i64::from(MAX_DIM))? as u32;
        let rules = parse_rules(&obj["rules"])?;
        let cells = parse_cells(&obj["cells"], width, height)?;
        let entities = parse_entities(&obj["entities"], width, height)?;
        match rules {
            Rules::Life { .. } if !entities.is_empty() => return Err(grammar("life takes no entities; entities must be empty")),
            Rules::Agents { .. } if !cells.is_empty() => return Err(grammar("agents take no cells; cells must be empty")),
            _ => {}
        }
        Ok(Scenario { seed, steps, width, height, rules, cells, entities })
    }

    /// The canonical tree: exact keys, the declared sets in ascending order.
    pub fn to_canon(&self) -> CanonValue {
        let mut top = BTreeMap::new();
        top.insert("v".to_string(), CanonValue::Int(1));
        top.insert("seed".to_string(), CanonValue::Int(i128::from(self.seed)));
        top.insert("steps".to_string(), CanonValue::Int(i128::from(self.steps)));
        top.insert("width".to_string(), CanonValue::Int(i128::from(self.width)));
        top.insert("height".to_string(), CanonValue::Int(i128::from(self.height)));
        let mut rules = BTreeMap::new();
        match &self.rules {
            Rules::Life { birth, survive } => {
                rules.insert("kind".to_string(), CanonValue::Str("life".to_string()));
                rules.insert("birth".to_string(), rule_set_canon(birth));
                rules.insert("survive".to_string(), rule_set_canon(survive));
            }
            Rules::Agents { bounce, jitter } => {
                rules.insert("kind".to_string(), CanonValue::Str("agents".to_string()));
                rules.insert("bounce".to_string(), CanonValue::Bool(*bounce));
                rules.insert("jitter".to_string(), CanonValue::Int(i128::from(*jitter)));
            }
        }
        top.insert("rules".to_string(), CanonValue::Obj(rules));
        let cells = self
            .cells
            .iter()
            .map(|(x, y)| CanonValue::Arr(vec![CanonValue::Int(i128::from(*x)), CanonValue::Int(i128::from(*y))]))
            .collect();
        top.insert("cells".to_string(), CanonValue::Arr(cells));
        top.insert("entities".to_string(), CanonValue::Arr(self.entities.iter().map(Entity::to_canon).collect()));
        CanonValue::Obj(top)
    }

    pub fn rules_tag(&self) -> u8 {
        match self.rules {
            Rules::Life { .. } => RULES_LIFE,
            Rules::Agents { .. } => RULES_AGENTS,
        }
    }
}

fn rule_set_canon(set: &[u8]) -> CanonValue {
    CanonValue::Arr(set.iter().map(|n| CanonValue::Int(i128::from(*n))).collect())
}

fn parse_rules(v: &CanonValue) -> Result<Rules, DeriveError> {
    let obj = v.as_obj().ok_or_else(|| grammar("rules must be an object"))?;
    let kind_name = obj.get("kind").and_then(CanonValue::as_str).unwrap_or("");
    match kind_name {
        "life" => {
            expect_keys(obj, "life rules", &["kind", "birth", "survive"])?;
            Ok(Rules::Life { birth: parse_rule_set(&obj["birth"], "birth")?, survive: parse_rule_set(&obj["survive"], "survive")? })
        }
        "agents" => {
            expect_keys(obj, "agents rules", &["kind", "bounce", "jitter"])?;
            let bounce = obj["bounce"].as_bool().ok_or_else(|| grammar("bounce must be a boolean"))?;
            let jitter = int_in(&obj["jitter"], "jitter", 0, i64::from(MAX_JITTER))? as u8;
            Ok(Rules::Agents { bounce, jitter })
        }
        _ => Err(grammar("rules.kind must be \"life\" or \"agents\"")),
    }
}

fn parse_rule_set(v: &CanonValue, what: &str) -> Result<Vec<u8>, DeriveError> {
    let arr = v.as_arr().ok_or_else(|| grammar(format!("{what} must be an array of neighbour counts")))?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let n = int_in(item, &format!("{what} entry"), 0, 8)? as u8;
        if out.contains(&n) {
            return Err(grammar(format!("{what} lists neighbour count {n} twice")));
        }
        out.push(n);
    }
    out.sort_unstable();
    Ok(out)
}

fn parse_cells(v: &CanonValue, width: u32, height: u32) -> Result<Vec<(u32, u32)>, DeriveError> {
    let arr = v.as_arr().ok_or_else(|| grammar("cells must be an array of [x, y] pairs"))?;
    if arr.len() > MAX_CELLS {
        return Err(grammar(format!("more than {MAX_CELLS} cells")));
    }
    let mut cells = Vec::with_capacity(arr.len());
    for item in arr {
        let pair = item.as_arr().filter(|p| p.len() == 2).ok_or_else(|| grammar("a cell must be an [x, y] pair"))?;
        let x = int_in(&pair[0], "cell x", 0, i64::from(width) - 1)? as u32;
        let y = int_in(&pair[1], "cell y", 0, i64::from(height) - 1)? as u32;
        cells.push((x, y));
    }
    cells.sort_unstable();
    if let Some(w) = cells.windows(2).find(|w| w[0] == w[1]) {
        return Err(grammar(format!("cell [{}, {}] is listed twice", w[0].0, w[0].1)));
    }
    Ok(cells)
}

fn parse_entities(v: &CanonValue, width: u32, height: u32) -> Result<Vec<Entity>, DeriveError> {
    let arr = v.as_arr().ok_or_else(|| grammar("entities must be an array of entity objects"))?;
    if arr.len() > MAX_ENTITIES {
        return Err(grammar(format!("more than {MAX_ENTITIES} entities")));
    }
    let mut entities = Vec::with_capacity(arr.len());
    for item in arr {
        entities.push(Entity::from_canon(item, width, height)?);
    }
    entities.sort_by(|a, b| a.id.cmp(&b.id));
    if let Some(w) = entities.windows(2).find(|w| w[0].id == w[1].id) {
        return Err(grammar(format!("entity id {:?} is used twice", w[0].id)));
    }
    Ok(entities)
}

impl Entity {
    fn from_canon(v: &CanonValue, width: u32, height: u32) -> Result<Self, DeriveError> {
        let obj = v.as_obj().ok_or_else(|| grammar("an entity must be an object"))?;
        expect_keys(obj, "an entity", &["id", "x", "y", "vx", "vy", "attrs"])?;
        let id = obj["id"]
            .as_str()
            .filter(|s| (1..=MAX_NAME_BYTES).contains(&s.len()))
            .ok_or_else(|| grammar(format!("entity id must be a string of 1..={MAX_NAME_BYTES} bytes")))?
            .to_string();
        let x = int_in(&obj["x"], "entity x", 0, i64::from(width) - 1)?;
        let y = int_in(&obj["y"], "entity y", 0, i64::from(height) - 1)?;
        let vx = int_in(&obj["vx"], "entity vx", -MAX_SPEED, MAX_SPEED)?;
        let vy = int_in(&obj["vy"], "entity vy", -MAX_SPEED, MAX_SPEED)?;
        let attrs_obj = obj["attrs"].as_obj().ok_or_else(|| grammar("attrs must be an object of integer attributes"))?;
        if attrs_obj.len() > MAX_ATTRS {
            return Err(grammar(format!("more than {MAX_ATTRS} attrs")));
        }
        let mut attrs = BTreeMap::new();
        for (k, val) in attrs_obj {
            if !(1..=MAX_NAME_BYTES).contains(&k.len()) {
                return Err(grammar(format!("an attr key must be 1..={MAX_NAME_BYTES} bytes")));
            }
            let i = val.as_i64().ok_or_else(|| grammar(format!("attr {k:?} must be an integer in i64 range")))?;
            attrs.insert(k.clone(), i);
        }
        Ok(Entity { id, x, y, vx, vy, attrs })
    }

    fn to_canon(&self) -> CanonValue {
        let mut o = BTreeMap::new();
        o.insert("id".to_string(), CanonValue::Str(self.id.clone()));
        o.insert("x".to_string(), CanonValue::Int(i128::from(self.x)));
        o.insert("y".to_string(), CanonValue::Int(i128::from(self.y)));
        o.insert("vx".to_string(), CanonValue::Int(i128::from(self.vx)));
        o.insert("vy".to_string(), CanonValue::Int(i128::from(self.vy)));
        let attrs = self.attrs.iter().map(|(k, v)| (k.clone(), CanonValue::Int(i128::from(*v)))).collect();
        o.insert("attrs".to_string(), CanonValue::Obj(attrs));
        CanonValue::Obj(o)
    }
}

// ─── the state machine ─────────────────────────────────────────────────────────────────────

/// splitmix64 (Steele, Lea, Flood 2014): the scenario's seed is its state, and every draw
/// is one `u64`. The only randomness in the kind, and it is a function of the DSL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        SplitMix64 { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LifeState {
    pub width: u32,
    pub height: u32,
    /// Bit `n` set ⇔ neighbour count `n` is in the set.
    pub birth: u16,
    pub survive: u16,
    /// Row-major, `y * width + x`, each cell 0 or 1.
    pub grid: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentsState {
    pub width: i64,
    pub height: i64,
    pub bounce: bool,
    pub jitter: u8,
    /// In id order.
    pub agents: Vec<Entity>,
}

/// The committed state: what a step transforms and what every link hashes. The rules ride
/// beside it because they are constant; they are the DSL's, committed by `dsl_hash`, and are
/// not in the state bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Life(LifeState),
    Agents(AgentsState),
}

fn rule_mask(set: &[u8]) -> u16 {
    set.iter().fold(0u16, |m, n| m | (1u16 << n))
}

impl State {
    pub fn initial(s: &Scenario) -> State {
        match &s.rules {
            Rules::Life { birth, survive } => {
                let w = s.width as usize;
                let mut grid = vec![0u8; w * (s.height as usize)];
                for (x, y) in &s.cells {
                    grid[(*y as usize) * w + (*x as usize)] = 1;
                }
                State::Life(LifeState { width: s.width, height: s.height, birth: rule_mask(birth), survive: rule_mask(survive), grid })
            }
            Rules::Agents { bounce, jitter } => State::Agents(AgentsState {
                width: i64::from(s.width),
                height: i64::from(s.height),
                bounce: *bounce,
                jitter: *jitter,
                agents: s.entities.clone(),
            }),
        }
    }

    /// The integer transition. Returns this step's wall events (always 0 for life).
    pub fn step(&mut self, rng: &mut SplitMix64) -> u64 {
        match self {
            State::Life(l) => {
                step_life(l);
                0
            }
            State::Agents(a) => step_agents(a, rng),
        }
    }

    /// The state's one spelling — the preimage of its link.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        match self {
            State::Life(l) => {
                let mut out = Vec::with_capacity(8 + l.grid.len());
                put_u32_le(&mut out, l.width);
                put_u32_le(&mut out, l.height);
                out.extend_from_slice(&l.grid);
                out
            }
            State::Agents(a) => {
                let mut out = Vec::new();
                put_u32_le(&mut out, a.agents.len() as u32);
                for e in &a.agents {
                    put_str(&mut out, &e.id);
                    put_i64_le(&mut out, e.x);
                    put_i64_le(&mut out, e.y);
                    put_i64_le(&mut out, e.vx);
                    put_i64_le(&mut out, e.vy);
                    put_u32_le(&mut out, e.attrs.len() as u32);
                    for (k, v) in &e.attrs {
                        put_str(&mut out, k);
                        put_i64_le(&mut out, *v);
                    }
                }
                out
            }
        }
    }

    /// The summary's per-step measure: population for life, `Σ |vx| + |vy|` for agents.
    pub fn measure(&self) -> u64 {
        match self {
            State::Life(l) => l.grid.iter().map(|c| u64::from(*c)).sum(),
            State::Agents(a) => a.agents.iter().map(|e| e.vx.unsigned_abs() + e.vy.unsigned_abs()).sum(),
        }
    }
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    put_u32_le(out, s.len() as u32);
    out.extend_from_slice(s.as_bytes());
}

fn step_life(l: &mut LifeState) {
    let w = l.width as usize;
    let h = l.height as usize;
    let g = &l.grid;
    let mut next = vec![0u8; w * h];
    for y in 0..h {
        let up = ((y + h - 1) % h) * w;
        let row = y * w;
        let down = ((y + 1) % h) * w;
        for x in 0..w {
            let left = (x + w - 1) % w;
            let right = (x + 1) % w;
            let n = g[up + left]
                + g[up + x]
                + g[up + right]
                + g[row + left]
                + g[row + right]
                + g[down + left]
                + g[down + x]
                + g[down + right];
            let rule = if g[row + x] != 0 { l.survive } else { l.birth };
            next[row + x] = u8::from(rule & (1u16 << n) != 0);
        }
    }
    l.grid = next;
}

fn step_agents(a: &mut AgentsState, rng: &mut SplitMix64) -> u64 {
    let mut walls = 0u64;
    let j = i64::from(a.jitter);
    for e in a.agents.iter_mut() {
        if j > 0 {
            let span = (2 * j + 1) as u64;
            let dx = (rng.next_u64() % span) as i64 - j;
            let dy = (rng.next_u64() % span) as i64 - j;
            e.vx = (e.vx + dx).clamp(-MAX_SPEED, MAX_SPEED);
            e.vy = (e.vy + dy).clamp(-MAX_SPEED, MAX_SPEED);
        }
        let (x, vx, wx) = advance(e.x, e.vx, a.width, a.bounce);
        let (y, vy, wy) = advance(e.y, e.vy, a.height, a.bounce);
        e.x = x;
        e.vx = vx;
        e.y = y;
        e.vy = vy;
        walls += wx + wy;
    }
    walls
}

/// One axis of one move: from `pos` by `v` in a lane of `len` cells. Returns the new position,
/// the new velocity component and the number of walls (bounce) or lane edges (wrap) crossed.
pub fn advance(pos: i64, v: i64, len: i64, bounce: bool) -> (i64, i64, u64) {
    let u = pos + v;
    let segment = u.div_euclid(len);
    let crossings = segment.unsigned_abs();
    if bounce {
        let m = u.rem_euclid(2 * len);
        let folded = if m < len { m } else { 2 * len - 1 - m };
        let v2 = if segment.rem_euclid(2) == 1 { -v } else { v };
        (folded, v2, crossings)
    } else {
        (u.rem_euclid(len), v, crossings)
    }
}

// ─── the trace and the artifact ────────────────────────────────────────────────────────────

/// One link: `H_key(prev ‖ index ‖ state)`.
pub fn link_hash(prev: &[u8; 64], index: u64, state_bytes: &[u8]) -> [u8; 64] {
    let mut st = blake2b_simd::Params::new().hash_length(64).key(STEP_DOMAIN).to_state();
    st.update(prev);
    st.update(&index.to_le_bytes());
    st.update(state_bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(st.finalize().as_bytes());
    out
}

/// A finished run: the chain, the per-step measures, the wall events and the final state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub trace: Vec<[u8; 64]>,
    pub measures: Vec<u64>,
    pub wall_events: u64,
    pub final_state: State,
}

pub fn simulate(s: &Scenario) -> Run {
    let count = s.steps as usize + 1;
    let mut rng = SplitMix64::new(s.seed);
    let mut state = State::initial(s);
    let mut trace = Vec::with_capacity(count);
    let mut measures = Vec::with_capacity(count);
    let mut wall_events = 0u64;
    let mut prev = GENESIS_PREV;
    for i in 0..=s.steps {
        if i > 0 {
            wall_events += state.step(&mut rng);
        }
        let h = link_hash(&prev, u64::from(i), &state.canonical_bytes());
        trace.push(h);
        measures.push(state.measure());
        prev = h;
    }
    Run { trace, measures, wall_events, final_state: state }
}

const HEADER_LEN: usize = 4 + 2 + 1 + 8 + 4 + 4 + 4;

/// The artifact's exact length, from the scenario alone: the header, the chain, the summary,
/// the state (whose length never changes across steps — ids and attribute names are constant
/// and every number is fixed-width) and the checksum.
pub fn artifact_len(s: &Scenario) -> usize {
    let count = s.steps as usize + 1;
    let summary = match s.rules {
        Rules::Life { .. } => 4 * count,
        Rules::Agents { .. } => 8 * count + 8,
    };
    HEADER_LEN + 4 + 64 * count + summary + State::initial(s).canonical_bytes().len() + 4
}

/// Simulate and write, refusing an artifact above the ceiling before running.
pub fn derive_artifact(s: &Scenario) -> Result<Vec<u8>, DeriveError> {
    let len = artifact_len(s);
    if len > MAX_ARTIFACT_BYTES {
        return Err(DeriveError::Transformer(format!("an artifact of {len} bytes exceeds the {MAX_ARTIFACT_BYTES}-byte ceiling")));
    }
    let run = simulate(s);
    let bytes = write_artifact(s, &run);
    debug_assert_eq!(bytes.len(), len);
    Ok(bytes)
}

/// The canonical writer `misaka-sim-trace/1/canonical-v1`.
pub fn write_artifact(s: &Scenario, run: &Run) -> Vec<u8> {
    let mut out = Vec::with_capacity(artifact_len(s));
    out.extend_from_slice(MAGIC);
    put_u16_le(&mut out, ARTIFACT_VERSION);
    out.push(s.rules_tag());
    put_u64_le(&mut out, s.seed);
    put_u32_le(&mut out, s.steps);
    put_u32_le(&mut out, s.width);
    put_u32_le(&mut out, s.height);
    put_u32_le(&mut out, run.trace.len() as u32);
    for h in &run.trace {
        out.extend_from_slice(h);
    }
    match s.rules {
        Rules::Life { .. } => {
            for m in &run.measures {
                put_u32_le(&mut out, *m as u32);
            }
        }
        Rules::Agents { .. } => {
            for m in &run.measures {
                put_u64_le(&mut out, *m);
            }
            put_u64_le(&mut out, run.wall_events);
        }
    }
    out.extend_from_slice(&run.final_state.canonical_bytes());
    let crc = crc32(&out);
    put_u32_le(&mut out, crc);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::derive::{ClaimBinding, derive_with};
    use crate::ids::{artifact_hash_v1, dsl_hash_v1, grammar_id_v1};
    use kaspa_hashes::Hash64;
    use serde_json::{Value, json};

    // ── builders ──

    fn life(width: u32, height: u32, steps: u32, cells: &[(u32, u32)]) -> Value {
        json!({
            "v": 1, "seed": 7, "steps": steps, "width": width, "height": height,
            "rules": {"kind": "life", "birth": [3], "survive": [2, 3]},
            "cells": cells.iter().map(|(x, y)| json!([x, y])).collect::<Vec<_>>(),
            "entities": [],
        })
    }

    fn entity(id: &str, x: i64, y: i64, vx: i64, vy: i64) -> Value {
        json!({"id": id, "x": x, "y": y, "vx": vx, "vy": vy, "attrs": {}})
    }

    fn agents(width: u32, height: u32, steps: u32, seed: u64, bounce: bool, jitter: u8, entities: Vec<Value>) -> Value {
        json!({
            "v": 1, "seed": seed, "steps": steps, "width": width, "height": height,
            "rules": {"kind": "agents", "bounce": bounce, "jitter": jitter},
            "cells": [], "entities": entities,
        })
    }

    fn bytes(v: &Value) -> Vec<u8> {
        serde_json::to_vec(v).unwrap()
    }

    fn canon(v: &Value) -> Vec<u8> {
        SimulationGrammar.canonicalize(&bytes(v)).unwrap()
    }

    fn scenario(v: &Value) -> Scenario {
        Scenario::from_canon(&parse_canonical(&bytes(v)).unwrap()).unwrap()
    }

    fn run(v: &Value) -> Vec<u8> {
        SimulationTraceTransformer.run(&canon(v)).unwrap().bytes
    }

    fn refusal(input: &[u8]) -> String {
        match SimulationGrammar.canonicalize(input) {
            Err(DeriveError::Grammar(m)) => m,
            other => panic!("expected a grammar refusal, got {other:?}"),
        }
    }

    // ── a reader for the artifact, so the tests parse what the writer wrote ──

    struct Reader<'a> {
        b: &'a [u8],
        pos: usize,
    }

    impl<'a> Reader<'a> {
        fn take(&mut self, n: usize) -> &'a [u8] {
            let s = &self.b[self.pos..self.pos + n];
            self.pos += n;
            s
        }
        fn u8(&mut self) -> u8 {
            self.take(1)[0]
        }
        fn u16(&mut self) -> u16 {
            u16::from_le_bytes(self.take(2).try_into().unwrap())
        }
        fn u32(&mut self) -> u32 {
            u32::from_le_bytes(self.take(4).try_into().unwrap())
        }
        fn u64(&mut self) -> u64 {
            u64::from_le_bytes(self.take(8).try_into().unwrap())
        }
        fn hash(&mut self) -> [u8; 64] {
            self.take(64).try_into().unwrap()
        }
    }

    struct Parsed {
        rules: u8,
        seed: u64,
        steps: u32,
        width: u32,
        height: u32,
        trace: Vec<[u8; 64]>,
        population: Vec<u32>,
        motion: Vec<u64>,
        wall_events: u64,
        final_state: Vec<u8>,
    }

    fn parse_artifact(a: &[u8]) -> Parsed {
        let mut r = Reader { b: a, pos: 0 };
        assert_eq!(r.take(4), MAGIC);
        assert_eq!(r.u16(), ARTIFACT_VERSION);
        let rules = r.u8();
        let seed = r.u64();
        let steps = r.u32();
        let width = r.u32();
        let height = r.u32();
        let count = r.u32() as usize;
        assert_eq!(count, steps as usize + 1, "trace count is steps + 1");
        let trace = (0..count).map(|_| r.hash()).collect();
        let (mut population, mut motion, mut wall_events) = (Vec::new(), Vec::new(), 0);
        match rules {
            RULES_LIFE => population = (0..count).map(|_| r.u32()).collect(),
            RULES_AGENTS => {
                motion = (0..count).map(|_| r.u64()).collect();
                wall_events = r.u64();
            }
            other => panic!("unknown rules tag {other}"),
        }
        let final_state = a[r.pos..a.len() - 4].to_vec();
        let crc = u32::from_le_bytes(a[a.len() - 4..].try_into().unwrap());
        assert_eq!(crc, crc32(&a[..a.len() - 4]), "trailing CRC-32");
        Parsed { rules, seed, steps, width, height, trace, population, motion, wall_events, final_state }
    }

    // ── (1) canonicalization ──

    #[test]
    fn canonicalization_is_idempotent_and_sorts_the_declared_sets() {
        let messy = br#" { "entities" : [ ], "cells": [[3,1],[1,1],[2,1]], "width": 5, "height": 3, "steps": 2,
            "rules": {"survive": [3, 2], "kind": "life", "birth": [3]}, "seed": 9, "v": 1 } "#;
        let once = SimulationGrammar.canonicalize(messy).unwrap();
        assert_eq!(
            once,
            br#"{"cells":[[1,1],[2,1],[3,1]],"entities":[],"height":3,"rules":{"birth":[3],"kind":"life","survive":[2,3]},"seed":9,"steps":2,"v":1,"width":5}"#
        );
        let twice = SimulationGrammar.canonicalize(&once).unwrap();
        assert_eq!(once, twice);

        let unordered = agents(8, 8, 1, 0, false, 0, vec![entity("b", 1, 1, 0, 0), entity("a", 2, 2, 0, 0)]);
        let c = canon(&unordered);
        let s = Scenario::from_canon(&parse_canonical(&c).unwrap()).unwrap();
        assert_eq!(s.entities.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(SimulationGrammar.canonicalize(&c).unwrap(), c);
        assert!(c.starts_with(br#"{"cells":[],"entities":[{"attrs":{},"id":"a""#));
    }

    // ── (2) every schema refusal, each with its own fragment ──

    #[test]
    fn every_schema_refusal_is_named_distinctly() {
        let l = || life(8, 8, 4, &[(1, 1)]);
        let a = || agents(8, 8, 4, 1, true, 2, vec![entity("e", 1, 1, 1, 0)]);
        // `set(v, ["a", "0", "b"], x)`: a numeric segment indexes an array, any other an object.
        let set = |mut v: Value, path: &[&str], to: Value| {
            let mut cur = &mut v;
            for p in &path[..path.len() - 1] {
                cur = match p.parse::<usize>() {
                    Ok(i) if cur.is_array() => &mut cur[i],
                    _ => &mut cur[*p],
                };
            }
            let last = path[path.len() - 1];
            match last.parse::<usize>() {
                Ok(i) if cur.is_array() => cur[i] = to,
                _ => cur[last] = to,
            }
            v
        };
        let mut many_cells = life(256, 256, 1, &[]);
        many_cells["cells"] = (0..=MAX_CELLS as u32).map(|i| json!([i % 256, i / 256])).collect::<Vec<_>>().into();
        let mut many_entities = agents(256, 256, 1, 0, false, 0, vec![]);
        many_entities["entities"] = (0..=MAX_ENTITIES).map(|i| entity(&format!("e{i}"), 0, 0, 0, 0)).collect::<Vec<_>>().into();
        let mut many_attrs = a();
        many_attrs["entities"][0]["attrs"] =
            (0..=MAX_ATTRS).map(|i| (format!("k{i}"), json!(i))).collect::<serde_json::Map<_, _>>().into();
        let mut life_with_entity = l();
        life_with_entity["entities"] = json!([entity("e", 0, 0, 0, 0)]);
        let mut agents_with_cell = a();
        agents_with_cell["cells"] = json!([[0, 0]]);
        let mut extra_top = l();
        extra_top["gravity"] = json!(9);
        let mut missing_top = l();
        missing_top.as_object_mut().unwrap().remove("cells");
        let mut extra_life = l();
        extra_life["rules"]["wrap"] = json!(true);
        let mut missing_life = l();
        missing_life["rules"].as_object_mut().unwrap().remove("survive");
        let mut extra_agents = a();
        extra_agents["rules"]["gravity"] = json!(1);
        let mut extra_entity = a();
        extra_entity["entities"][0]["mass"] = json!(1);
        let mut missing_entity = a();
        missing_entity["entities"][0].as_object_mut().unwrap().remove("vy");
        let mut dup_id = a();
        dup_id["entities"] = json!([entity("e", 0, 0, 0, 0), entity("e", 1, 1, 0, 0)]);
        let mut empty_attr_key = a();
        empty_attr_key["entities"][0]["attrs"] = json!({"": 1});
        let mut str_attr = a();
        str_attr["entities"][0]["attrs"] = json!({"hp": "full"});

        let cases: Vec<(&str, Vec<u8>, &str)> = vec![
            ("not an object", b"[1]".to_vec(), "must be a JSON object"),
            ("unknown top key", bytes(&extra_top), "the scenario has unknown key"),
            ("missing top key", bytes(&missing_top), "the scenario is missing key"),
            ("v", bytes(&set(l(), &["v"], json!(2))), "v must be 1"),
            ("seed negative", bytes(&set(l(), &["seed"], json!(-1))), "seed must be an integer in 0..=18446744073709551615"),
            ("steps zero", bytes(&set(l(), &["steps"], json!(0))), "steps must be an integer in 1..=100000"),
            ("steps too many", bytes(&set(l(), &["steps"], json!(100_001))), "steps must be an integer"),
            ("width", bytes(&set(l(), &["width"], json!(257))), "width must be an integer in 1..=256"),
            ("height", bytes(&set(l(), &["height"], json!(0))), "height must be an integer in 1..=256"),
            ("rules not object", bytes(&set(l(), &["rules"], json!(5))), "rules must be an object"),
            ("rules kind", bytes(&set(l(), &["rules", "kind"], json!("fluid"))), "rules.kind must be"),
            ("life extra key", bytes(&extra_life), "life rules has unknown key"),
            ("life missing key", bytes(&missing_life), "life rules is missing key"),
            ("birth not array", bytes(&set(l(), &["rules", "birth"], json!(3))), "birth must be an array"),
            ("birth range", bytes(&set(l(), &["rules", "birth"], json!([9]))), "birth entry must be an integer in 0..=8"),
            ("survive range", bytes(&set(l(), &["rules", "survive"], json!([-1]))), "survive entry must be an integer in 0..=8"),
            ("survive dup", bytes(&set(l(), &["rules", "survive"], json!([2, 2]))), "survive lists neighbour count 2 twice"),
            ("life with entity", bytes(&life_with_entity), "life takes no entities"),
            ("cells not array", bytes(&set(l(), &["cells"], json!({}))), "cells must be an array"),
            ("too many cells", bytes(&many_cells), "more than 65536 cells"),
            ("cell not a pair", bytes(&set(l(), &["cells"], json!([[1]]))), "a cell must be an [x, y] pair"),
            ("cell x", bytes(&set(l(), &["cells"], json!([[8, 0]]))), "cell x must be an integer in 0..=7"),
            ("cell y", bytes(&set(l(), &["cells"], json!([[0, -1]]))), "cell y must be an integer in 0..=7"),
            ("cell dup", bytes(&set(l(), &["cells"], json!([[2, 3], [2, 3]]))), "cell [2, 3] is listed twice"),
            ("agents with cell", bytes(&agents_with_cell), "agents take no cells"),
            ("agents extra key", bytes(&extra_agents), "agents rules has unknown key"),
            ("bounce", bytes(&set(a(), &["rules", "bounce"], json!(1))), "bounce must be a boolean"),
            ("jitter", bytes(&set(a(), &["rules", "jitter"], json!(17))), "jitter must be an integer in 0..=16"),
            ("entities not array", bytes(&set(a(), &["entities"], json!("x"))), "entities must be an array"),
            ("too many entities", bytes(&many_entities), "more than 4096 entities"),
            ("entity not object", bytes(&set(a(), &["entities"], json!([1]))), "an entity must be an object"),
            ("entity extra key", bytes(&extra_entity), "an entity has unknown key"),
            ("entity missing key", bytes(&missing_entity), "an entity is missing key"),
            ("entity id empty", bytes(&set(a(), &["entities", "0", "id"], json!(""))), "entity id must be a string of 1..=64 bytes"),
            ("entity id dup", bytes(&dup_id), "is used twice"),
            ("entity x", bytes(&set(a(), &["entities", "0", "x"], json!(-1))), "entity x must be an integer in 0..=7"),
            ("entity y", bytes(&set(a(), &["entities", "0", "y"], json!(8))), "entity y must be an integer in 0..=7"),
            ("entity vx", bytes(&set(a(), &["entities", "0", "vx"], json!(65))), "entity vx must be an integer in -64..=64"),
            ("entity vy", bytes(&set(a(), &["entities", "0", "vy"], json!(-65))), "entity vy must be an integer in -64..=64"),
            ("attrs not object", bytes(&set(a(), &["entities", "0", "attrs"], json!([]))), "attrs must be an object"),
            ("too many attrs", bytes(&many_attrs), "more than 16 attrs"),
            ("attr key empty", bytes(&empty_attr_key), "an attr key must be 1..=64 bytes"),
            ("attr value string", bytes(&str_attr), "must be an integer in i64 range"),
            ("float anywhere", br#"{"v":1,"seed":1.5}"#.to_vec(), "non-integer number"),
            ("duplicate json key", br#"{"v":1,"v":1}"#.to_vec(), "duplicate key"),
        ];
        let mut fragments = std::collections::BTreeSet::new();
        for (label, input, fragment) in &cases {
            let msg = refusal(input);
            assert!(msg.contains(fragment), "{label}: refusal {msg:?} does not carry {fragment:?}");
            assert!(fragments.insert(*fragment), "{label}: fragment {fragment:?} is not distinct");
        }
        assert_eq!(fragments.len(), cases.len());
        // The two bases every case mutates are themselves accepted, so each refusal is the mutation's.
        assert!(!canon(&l()).is_empty() && !canon(&a()).is_empty(), "the bases themselves are accepted");
    }

    // ── (3) determinism ──

    #[test]
    fn same_dsl_twice_and_whitespace_or_key_order_variants_give_identical_bytes() {
        let v = agents(16, 12, 30, 42, true, 3, vec![entity("p", 3, 4, 2, -1), entity("q", 10, 1, -3, 2)]);
        let first = run(&v);
        let second = run(&v);
        assert_eq!(first, second);
        let reordered = br#"{"entities":[{"vy":2,"vx":-3,"y":1,"x":10,"id":"q","attrs":{}},
                              {"attrs":{},"vy":-1,"vx":2,"y":4,"x":3,"id":"p"}],
            "cells":[],   "rules":{"jitter":3,"bounce":true,"kind":"agents"},
            "height":12,"width":16,"steps":30,"seed":42,"v":1}"#;
        let canonical = SimulationGrammar.canonicalize(reordered).unwrap();
        assert_eq!(canonical, canon(&v));
        assert_eq!(SimulationTraceTransformer.run(&canonical).unwrap().bytes, first);
    }

    #[test]
    fn the_transformer_refuses_non_canonical_input() {
        let v = life(4, 4, 1, &[(1, 1)]);
        let pretty = serde_json::to_vec_pretty(&v).unwrap();
        match SimulationTraceTransformer.run(&pretty) {
            Err(DeriveError::Transformer(m)) => assert!(m.contains("not canonical")),
            other => panic!("{other:?}"),
        }
        assert!(matches!(SimulationTraceTransformer.run(b"{}"), Err(DeriveError::Grammar(_))));
    }

    #[test]
    fn manifest_and_registration() {
        let m = SimulationTraceTransformer.manifest();
        assert_eq!(m.name, "simulation/trace/v1");
        assert_eq!(m.kind, kind::SIMULATION);
        assert_eq!(m.grammar, "simulation/v1");
        assert_eq!(m.discipline, Discipline::Integer);
        assert_eq!(m.writer, "misaka-sim-trace/1/canonical-v1");
        assert_eq!(m.source_tree_sha256, crate::SOURCE_TREE_SHA256_HEX);
        let (g, t) = register();
        assert_eq!(g.len(), 1);
        assert_eq!(t.len(), 1);
        assert_eq!(g[0].name(), SimulationGrammar.name());
        assert_eq!(t[0].manifest(), m);
    }

    // ── (4) life ──

    fn states_of(v: &Value) -> Vec<State> {
        let sc = scenario(v);
        let mut rng = SplitMix64::new(sc.seed);
        let mut st = State::initial(&sc);
        let mut out = vec![st.clone()];
        for _ in 0..sc.steps {
            st.step(&mut rng);
            out.push(st.clone());
        }
        out
    }

    #[test]
    fn a_blinker_oscillates_with_period_two() {
        let v = life(5, 5, 6, &[(1, 2), (2, 2), (3, 2)]);
        let states = states_of(&v);
        assert_ne!(states[1], states[0]);
        assert_eq!(states[2], states[0]);
        assert_eq!(states[3], states[1]);
        let p = parse_artifact(&run(&v));
        assert_eq!(p.population, vec![3; 7]);
        assert_eq!(p.rules, RULES_LIFE);
        if let State::Life(l) = &states[1] {
            let alive: Vec<usize> = l.grid.iter().enumerate().filter(|(_, c)| **c == 1).map(|(i, _)| i).collect();
            assert_eq!(alive, vec![2 + 5, 2 + 10, 2 + 15], "the vertical phase");
        }
    }

    #[test]
    fn a_block_is_still() {
        let v = life(4, 4, 5, &[(1, 1), (2, 1), (1, 2), (2, 2)]);
        let states = states_of(&v);
        assert!(states.iter().all(|s| *s == states[0]));
        let p = parse_artifact(&run(&v));
        assert_eq!(p.population, vec![4; 6]);
        // Equal states, distinct links: the index is in every preimage.
        assert_ne!(p.trace[0], p.trace[1]);
    }

    #[test]
    fn a_glider_on_an_8x8_torus_returns_home_after_32_steps() {
        let v = life(8, 8, 32, &[(1, 0), (2, 1), (0, 2), (1, 2), (2, 2)]);
        let states = states_of(&v);
        assert_eq!(states[32], states[0]);
        assert_ne!(states[4], states[0]);
        for (i, s) in states.iter().enumerate() {
            assert_eq!(s.measure(), 5, "population at step {i}");
        }
        // After four steps the glider has moved by (1, 1).
        if let (State::Life(a), State::Life(b)) = (&states[0], &states[4]) {
            let shifted: Vec<u8> = (0..64).map(|i| a.grid[((i / 8 + 7) % 8) * 8 + (i % 8 + 7) % 8]).collect();
            assert_eq!(b.grid, shifted);
        }
        let p = parse_artifact(&run(&v));
        assert_eq!(p.population, vec![5; 33]);
    }

    // ── (5) agents ──

    #[test]
    fn splitmix64_known_vectors() {
        let mut r = SplitMix64::new(0);
        assert_eq!(r.next_u64(), 0xE220_A839_7B1D_CDAF);
        assert_eq!(r.next_u64(), 0x6E78_9E6A_A1B9_65F4);
        let mut r = SplitMix64::new(1);
        assert_eq!(r.next_u64(), 0x910A_2DEC_8902_5CC1);
    }

    #[test]
    fn a_bouncing_entity_reflects_where_the_hand_computation_says() {
        // Lane of 8 cells (walls at -1/2 and 7 1/2), from 5 by +4: 6 7 | 7 6 → 6, velocity -4.
        // Then 6 by -4 → 2. Then 2 by -4: 1 0 | 0 1 → 1, velocity +4.
        let v = agents(8, 8, 3, 0, true, 0, vec![entity("e", 5, 3, 4, 0)]);
        let states = states_of(&v);
        let at = |i: usize| match &states[i] {
            State::Agents(a) => (a.agents[0].x, a.agents[0].vx, a.agents[0].y, a.agents[0].vy),
            _ => unreachable!(),
        };
        assert_eq!(at(1), (6, -4, 3, 0));
        assert_eq!(at(2), (2, -4, 3, 0));
        assert_eq!(at(3), (1, 4, 3, 0));
        let p = parse_artifact(&run(&v));
        assert_eq!(p.wall_events, 2);
        assert_eq!(p.motion, vec![4; 4]);
        // A hit from the last cell bounces in place; two walls in one move cancel the flip.
        assert_eq!(advance(7, 1, 8, true), (7, -1, 1));
        assert_eq!(advance(0, -1, 8, true), (0, 1, 1));
        assert_eq!(advance(0, 16, 8, true), (0, 16, 2));
        assert_eq!(advance(0, -8, 8, true), (7, 8, 1));
        // A lane of one cell has nowhere to go and never loops.
        assert_eq!(advance(0, 5, 1, true), (0, -5, 5));
    }

    #[test]
    fn a_wrapping_entity_wraps() {
        let v = agents(8, 8, 3, 0, false, 0, vec![entity("e", 5, 7, 4, 1)]);
        let states = states_of(&v);
        let at = |i: usize| match &states[i] {
            State::Agents(a) => (a.agents[0].x, a.agents[0].vx, a.agents[0].y, a.agents[0].vy),
            _ => unreachable!(),
        };
        assert_eq!(at(1), (1, 4, 0, 1));
        assert_eq!(at(2), (5, 4, 1, 1));
        assert_eq!(at(3), (1, 4, 2, 1));
        let p = parse_artifact(&run(&v));
        assert_eq!(p.wall_events, 3, "two x wraps and one y wrap");
        assert_eq!(advance(0, -1, 8, false), (7, -1, 1));
        assert_eq!(advance(3, -20, 8, false), (7, -20, 3));
    }

    #[test]
    fn jitter_is_deterministic_per_seed_and_drawn_in_id_order() {
        let mk = |seed: u64| agents(64, 64, 20, seed, false, 2, vec![entity("b", 8, 8, 0, 0), entity("a", 1, 1, 0, 0)]);
        assert_eq!(run(&mk(5)), run(&mk(5)));
        assert_ne!(run(&mk(5)), run(&mk(6)));
        assert_ne!(parse_artifact(&run(&mk(5))).trace[1], parse_artifact(&run(&mk(6))).trace[1]);
        // The first two draws are "a"'s (id order), the next two "b"'s.
        let mut rng = SplitMix64::new(5);
        let draw = |rng: &mut SplitMix64| (rng.next_u64() % 5) as i64 - 2;
        let (ax, ay, bx, by) = (draw(&mut rng), draw(&mut rng), draw(&mut rng), draw(&mut rng));
        let states = states_of(&mk(5));
        match &states[1] {
            State::Agents(s) => {
                assert_eq!(s.agents[0].id, "a");
                assert_eq!((s.agents[0].vx, s.agents[0].vy), (ax, ay));
                assert_eq!((s.agents[0].x, s.agents[0].y), ((1 + ax).rem_euclid(64), (1 + ay).rem_euclid(64)));
                assert_eq!((s.agents[1].vx, s.agents[1].vy), (bx, by));
            }
            _ => unreachable!(),
        }
        // Velocity never leaves the clamp, however long the run.
        let long = agents(4, 4, 400, 9, true, 16, vec![entity("z", 0, 0, 64, -64)]);
        for s in states_of(&long) {
            if let State::Agents(a) = s {
                assert!(a.agents[0].vx.abs() <= MAX_SPEED && a.agents[0].vy.abs() <= MAX_SPEED);
                assert!((0..4).contains(&a.agents[0].x) && (0..4).contains(&a.agents[0].y));
            }
        }
    }

    // ── (6) structure: the reader recomputes every link ──

    fn check_structure(v: &Value) {
        let sc = scenario(v);
        let art = run(v);
        assert_eq!(art.len(), artifact_len(&sc), "artifact_len predicts the writer");
        let p = parse_artifact(&art);
        assert_eq!(p.rules, sc.rules_tag());
        assert_eq!(p.seed, sc.seed);
        assert_eq!(p.steps, sc.steps);
        assert_eq!(p.width, sc.width);
        assert_eq!(p.height, sc.height);
        let mut rng = SplitMix64::new(sc.seed);
        let mut st = State::initial(&sc);
        let mut prev = GENESIS_PREV;
        let mut walls = 0u64;
        for i in 0..=sc.steps {
            if i > 0 {
                walls += st.step(&mut rng);
            }
            let h = link_hash(&prev, u64::from(i), &st.canonical_bytes());
            assert_eq!(p.trace[i as usize], h, "link {i}");
            match &st {
                State::Life(_) => assert_eq!(u64::from(p.population[i as usize]), st.measure()),
                State::Agents(_) => assert_eq!(p.motion[i as usize], st.measure()),
            }
            prev = h;
        }
        assert_eq!(p.final_state, st.canonical_bytes());
        match st {
            State::Life(_) => {
                assert_eq!(p.population.len(), sc.steps as usize + 1);
                assert!(p.motion.is_empty());
            }
            State::Agents(_) => {
                assert_eq!(p.motion.len(), sc.steps as usize + 1);
                assert_eq!(p.wall_events, walls);
            }
        }
        // A different predecessor or index would not reproduce the link.
        assert_ne!(link_hash(&[1u8; 64], 0, &[]), link_hash(&GENESIS_PREV, 0, &[]));
        assert_ne!(link_hash(&GENESIS_PREV, 1, &[]), link_hash(&GENESIS_PREV, 0, &[]));
    }

    #[test]
    fn the_artifact_parses_and_every_link_recomputes() {
        check_structure(&life(12, 9, 25, &[(1, 0), (2, 1), (0, 2), (1, 2), (2, 2), (8, 8), (9, 8), (8, 7)]));
        let mut e = entity("k", 3, 3, 1, 2);
        e["attrs"] = json!({"hp": 100, "team": -2, "z": 9_223_372_036_854_775_807i64});
        check_structure(&agents(10, 7, 40, 77, true, 1, vec![e, entity("j", 9, 6, -5, 0)]));
        check_structure(&agents(10, 7, 40, 77, false, 4, vec![entity("j", 9, 6, -5, 0)]));
    }

    #[test]
    fn the_ceiling_is_checked_before_a_step_runs() {
        let mut sc = scenario(&life(4, 4, 1, &[]));
        sc.steps = 2_000_000; // beyond the grammar, so only the transformer's own guard sees it
        assert!(artifact_len(&sc) > MAX_ARTIFACT_BYTES);
        match derive_artifact(&sc) {
            Err(DeriveError::Transformer(m)) => assert!(m.contains("ceiling")),
            other => panic!("{other:?}"),
        }
        let biggest = scenario(&life(256, 256, 1, &[]));
        let mut biggest = biggest;
        biggest.steps = MAX_STEPS;
        assert!(artifact_len(&biggest) < MAX_ARTIFACT_BYTES, "the grammar's largest run fits");
    }

    // ── (7) the corpus and its golden ──

    #[test]
    fn corpus_matches_golden() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus").join("simulation");
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".json") && n != "golden.json")
            .collect();
        names.sort();
        assert!(names.len() >= 4, "the corpus holds at least four samples: {names:?}");
        let binding = ClaimBinding {
            network_domain: Hash64::from_bytes([0x01; 64]),
            claim_id: Hash64::from_bytes([0x02; 64]),
            output_root: Hash64::from_bytes([0x03; 64]),
            executor_pubkey: vec![0x11; 2592],
        };
        let mut actual: BTreeMap<String, Value> = BTreeMap::new();
        for name in &names {
            let answer = std::fs::read(dir.join(name)).unwrap();
            let d = derive_with(&SimulationGrammar, &SimulationTraceTransformer, &binding, &answer).unwrap();
            assert_eq!(d.grammar_id, grammar_id_v1(GRAMMAR_NAME));
            assert_eq!(d.dsl_hash, dsl_hash_v1(&d.grammar_id, &d.canonical_dsl));
            assert_eq!(d.artifact_hash, artifact_hash_v1(&d.artifact.bytes));
            assert_eq!(d.kind, kind::SIMULATION);
            assert_eq!(d.object.artifact_bytes, d.artifact.bytes.len() as u64);
            actual.insert(
                name.clone(),
                json!({
                    "dsl_hash": d.dsl_hash.to_string(),
                    "artifact_hash": d.artifact_hash.to_string(),
                    "artifact_bytes": d.artifact.bytes.len(),
                }),
            );
        }
        let golden_path = dir.join("golden.json");
        if std::env::var_os("PALW_DERIVE_WRITE_GOLDEN").is_some() {
            std::fs::write(&golden_path, serde_json::to_string_pretty(&actual).unwrap() + "\n").unwrap();
        }
        let golden: BTreeMap<String, Value> = serde_json::from_slice(&std::fs::read(&golden_path).unwrap()).unwrap();
        assert_eq!(golden, actual, "the corpus golden moved; a new grammar or writer is a new name, not an edit");
    }

    // ── (8) the discipline, scanned ──

    #[test]
    fn no_floating_point_type_names_in_this_file() {
        let src = include_str!("simulation.rs");
        for needle in [String::from("f") + "64", String::from("f") + "32"] {
            assert!(!src.contains(&needle), "{needle} appears in simulation.rs");
        }
    }
}
