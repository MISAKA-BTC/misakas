//! **ADR-0096 Decision 7 — a job may carry a decode constraint, and the committed token is the
//! ADMITTED argmax.** The consensus half: the automaton's pinned form, `admit`, the selection
//! rule v3, and the binding the court receives. Nothing here is armed by its existence; the fence
//! is `Params::palw_fp_decode_constraint`, `None` on every preset and refused at assembly until
//! the build carries all three halves (Decision 8).
//!
//! # What a constraint is
//!
//! A **byte-level pushdown automaton with a finite frame vocabulary**, in one canonical borsh
//! form ([`PalwDecodeConstraintV1`]). JSON needs a stack — an object inside an array inside an
//! object — so the automaton is a set of byte DFAs called *frames* plus a bounded stack of frames
//! ([`PALW_CONSTRAINT_MAX_DEPTH_V1`], the depth the schema subset pins). A frame's node has sorted,
//! disjoint byte-range edges, each carrying one of five actions:
//!
//! ```text
//!   Goto(n)                 consume the byte, move to node n of this frame
//!   Push { frame, resume }  do NOT consume: push (this frame, resume, seen), enter `frame` at its
//!                           start with an empty key mask, and read the same byte there
//!   Pop                     do NOT consume: pop, return to the parent at `resume` with its key
//!                           mask, and read the same byte there — on an empty stack, DEAD
//!   Key { index, next }     consume: set key bit `index` of this frame's mask (DEAD if set), goto
//!   Close { required, next } consume: DEAD unless the mask holds every `required` bit, goto
//! ```
//!
//! A byte with no edge is DEAD, and dead is absorbing: `admit` is "feeding the token's bytes from
//! `s` reaches a non-dead state" (Decision 7), so a token that dies on its third byte is not
//! admitted whatever its first two did. `Push` and `Pop` are epsilon moves decided by the next
//! byte, which is what lets a number — the one JSON value with no terminator of its own — end
//! exactly where its parent's `,` or `}` begins: the number frame's complete nodes carry `Pop`
//! edges on the terminators and the parent reads the terminator at `resume`. A chain of pops is
//! bounded by the stack and a chain of pushes by the depth; a malformed automaton that ping-pongs
//! is cut off by [`PALW_CONSTRAINT_MAX_HOPS_V1`] rather than looped on, because a court must
//! terminate on any bytes a challenger hands it.
//!
//! **The key mask is the subset construction made explicit.** "Keys in any order, each at most
//! once, the required ones present" is a DFA over 2^n subsets; spelling the subset as a 16-bit
//! `seen` field of the runtime state — saved on `Push`, restored on `Pop` — is the same automaton
//! without enumerating it, and bounds an object at sixteen tracked members. Everything is still a
//! deterministic function of `(constraint bytes, byte string)`; no host state reaches it.
//!
//! **After a complete root value nothing is admitted.** A `Pop` edge with an empty stack is dead,
//! and a complete value's only edges are pops on its terminators, so once the root `}` has been
//! read every lane of the vocabulary dies. That is exactly how Decision 7's stop fires: "if no
//! lane is admitted, the committed token is the lowest EOG id and the run's stop reason is
//! `EndOfGeneration`". The end-of-generation token is never *admitted* (its bytes are not JSON);
//! it is *committed by rule* when nothing is, and the court tries that rule with the class's own
//! `eog_token_ids` ([`PalwDecodeConstraintCourtV1`]).
//!
//! # What is pinned here and what is not
//!
//! * Pinned: the serialized form and its bounds (version 1; ≤ 65,535 frames; ≤ 65,535 nodes; ≤ 64
//!   KiB; depth 16; 16 key slots), the step function, `admit`, the id construction, the selection
//!   rule v3, and the court's reading of "not admitted".
//! * Not here: the **compiler** (`misaka-palw-constraint`, a leaf crate — consensus-core defines
//!   the form and never depends on what produces it; its `compiler_id` rides in the header as
//!   provenance and is not interpreted by any rule), the class's **token-to-bytes table** (served
//!   material, `tokenizer_commitment`; the court receives it as a lookup from the caller and owns
//!   no tokenizer), and the **fence**.
//!
//! # The two rules this module adds to Decision 7's text
//!
//! 1. **A lane the class table cannot render is never admitted.** `admit(s, bytes(j))` is
//!    undefined when `bytes(j)` is — a padded vocabulary id, a token holding a non-byte
//!    character — and an answer is a byte string, so a token with no bytes cannot be part of one.
//!    An empty rendering (`Some([])`) is admitted trivially, state unchanged; no shipped tokenizer
//!    has one, and a class that did would be admitting a token that makes no progress.
//! 2. **The state at a position is defined only over an admitted prefix.** The court recomputes
//!    `s` by running the automaton over the rendered prefix; a prefix that dies before the
//!    challenged position names an EARLIER fault, and the court says so rather than adjudicating
//!    the later position from a dead state (`InputSetNotCanonical`, "challenge the first position
//!    that is not admitted").

use kaspa_hashes::Hash64;

use crate::palw_decode_select_v2::{decode_lane_beats_v2, decode_lane_key_v2};

/// The keyed-hash domain every constraint id is minted under (ADR-0096 Decision 7). The same
/// bytes `misaka_palw_constraint::PALW_CONSTRAINT_DOMAIN_V1` spells; the two are held equal by a
/// test in that crate (a leaf crate can import this one, never the reverse).
pub const PALW_CONSTRAINT_DOMAIN_V1: &[u8] = b"misaka-palw/constraint/v1";

/// The form version in the header. A future form is a new number and a new rule, never a
/// reinterpretation of these bytes.
pub const PALW_DECODE_CONSTRAINT_VERSION_V1: u16 = 1;

/// The largest constraint, serialized, a job may carry (Decision 7: "bytes are bounded (64 KiB)
/// and refused above it"). The same number as the entrance's `PALW_CONSTRAINT_MAX_BYTES`.
pub const PALW_CONSTRAINT_MAX_BYTES_V1: usize = 64 * 1024;

/// How many frames may be on the stack — the nesting depth a value may reach. The schema subset
/// pins its own depth at 16 (`MAX_SCHEMA_DEPTH`), and a value at that depth has exactly sixteen
/// frames beneath it, so a push at this depth is dead.
pub const PALW_CONSTRAINT_MAX_DEPTH_V1: usize = 16;

/// Frames are named by a `u16`, and one frame is the least an automaton has.
pub const PALW_CONSTRAINT_MAX_FRAMES_V1: usize = 65_535;

/// Nodes are named by a `u16` within their frame, and the whole automaton holds at most this
/// many — the byte ceiling binds first in practice.
pub const PALW_CONSTRAINT_MAX_NODES_V1: usize = 65_535;

/// Key slots of one object frame: the width of the `seen` mask.
pub const PALW_CONSTRAINT_KEY_SLOTS_V1: u8 = 16;

/// The most epsilon moves (`Push`, `Pop`) one byte may trigger before the step is declared dead.
/// A well-formed automaton never reaches it — a pop chain is bounded by the stack and a push
/// chain by the depth — but a court must terminate on any bytes it is handed.
pub const PALW_CONSTRAINT_MAX_HOPS_V1: usize = 4 * PALW_CONSTRAINT_MAX_DEPTH_V1 + 4;

/// **The automaton, in its pinned form.** `to_bytes` is the canonical serialization the id is
/// over; `from_bytes` refuses anything that is not exactly that.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwDecodeConstraintV1 {
    /// [`PALW_DECODE_CONSTRAINT_VERSION_V1`].
    pub version: u16,
    /// The content name of the compiler that produced these bytes (ADR-0078 Decision 3's
    /// discipline: a transformer is named by what it is). Provenance in the preimage of the id;
    /// no rule reads it.
    pub compiler_id: Hash64,
    /// The frame the root value is read in.
    pub start_frame: u16,
    pub frames: Vec<PalwConstraintFrameV1>,
}

/// One byte DFA. `start` is where a `Push` into this frame begins.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwConstraintFrameV1 {
    pub start: u16,
    pub nodes: Vec<PalwConstraintNodeV1>,
}

/// One node. `accepting` is read only when the stack is empty: a complete value inside an
/// unfinished parent is not an accepted answer.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwConstraintNodeV1 {
    pub accepting: bool,
    /// Sorted by `lo`, pairwise disjoint, each `lo <= hi`. A byte in no range is dead.
    pub edges: Vec<PalwConstraintEdgeV1>,
}

/// One byte range and what reading a byte in it does.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwConstraintEdgeV1 {
    pub lo: u8,
    pub hi: u8,
    pub action: PalwConstraintActionV1,
}

/// The five moves, spelled in the module header.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwConstraintActionV1 {
    Goto(u16),
    Push { frame: u16, resume: u16 },
    Pop,
    Key { index: u8, next: u16 },
    Close { required: u16, next: u16 },
}

/// Why constraint bytes were refused. Every arm is a refusal of the BYTES — malformed material,
/// never a verdict about a claim.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwDecodeConstraintError {
    #[error("the constraint is {0} bytes, past the {PALW_CONSTRAINT_MAX_BYTES_V1}-byte ceiling (ADR-0096 Decision 7)")]
    TooLarge(usize),
    #[error("the constraint bytes do not parse as a version-1 automaton: {0}")]
    Malformed(String),
    #[error("the constraint bytes are not the canonical serialization of the automaton they parse to")]
    NotCanonical,
    #[error("the automaton is not well formed: {0}")]
    Invalid(&'static str),
}

impl PalwDecodeConstraintV1 {
    /// Every structural rule the step function relies on, checked before a byte is read. A
    /// validated automaton cannot index out of range, cannot shift a key bit out of the mask, and
    /// serializes under the ceiling.
    pub fn validate(&self) -> Result<(), PalwDecodeConstraintError> {
        let bad = PalwDecodeConstraintError::Invalid;
        if self.version != PALW_DECODE_CONSTRAINT_VERSION_V1 {
            return Err(bad("the header version is not 1"));
        }
        if self.frames.is_empty() {
            return Err(bad("an automaton has at least one frame"));
        }
        if self.frames.len() > PALW_CONSTRAINT_MAX_FRAMES_V1 {
            return Err(bad("more frames than a u16 can name"));
        }
        if self.start_frame as usize >= self.frames.len() {
            return Err(bad("the start frame is past the frame list"));
        }
        let mut total_nodes = 0usize;
        for frame in &self.frames {
            if frame.nodes.is_empty() {
                return Err(bad("a frame has no nodes"));
            }
            if frame.nodes.len() > PALW_CONSTRAINT_MAX_NODES_V1 {
                return Err(bad("a frame has more nodes than a u16 can name"));
            }
            total_nodes += frame.nodes.len();
            if total_nodes > PALW_CONSTRAINT_MAX_NODES_V1 {
                return Err(bad("more nodes than the automaton ceiling"));
            }
            if frame.start as usize >= frame.nodes.len() {
                return Err(bad("a frame's start node is past its node list"));
            }
            for node in &frame.nodes {
                let mut previous_hi: Option<u8> = None;
                for edge in &node.edges {
                    if edge.lo > edge.hi {
                        return Err(bad("an edge's low byte is above its high byte"));
                    }
                    if let Some(hi) = previous_hi
                        && edge.lo <= hi
                    {
                        return Err(bad("a node's edges are not sorted and disjoint"));
                    }
                    previous_hi = Some(edge.hi);
                    match edge.action {
                        PalwConstraintActionV1::Goto(next)
                        | PalwConstraintActionV1::Key { next, .. }
                        | PalwConstraintActionV1::Close { next, .. } => {
                            if next as usize >= frame.nodes.len() {
                                return Err(bad("an edge targets a node past its frame"));
                            }
                        }
                        PalwConstraintActionV1::Push { frame: target, resume } => {
                            if target as usize >= self.frames.len() {
                                return Err(bad("a push names a frame past the frame list"));
                            }
                            if resume as usize >= frame.nodes.len() {
                                return Err(bad("a push's resume node is past its frame"));
                            }
                        }
                        PalwConstraintActionV1::Pop => {}
                    }
                    if let PalwConstraintActionV1::Key { index, .. } = edge.action
                        && index >= PALW_CONSTRAINT_KEY_SLOTS_V1
                    {
                        return Err(bad("a key index is past the sixteen slots of the mask"));
                    }
                }
            }
        }
        let len = self.to_bytes().len();
        if len > PALW_CONSTRAINT_MAX_BYTES_V1 {
            return Err(PalwDecodeConstraintError::TooLarge(len));
        }
        Ok(())
    }

    /// The canonical bytes — what the id is over and what rides the served answer envelope.
    pub fn to_bytes(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("a plain borsh struct serializes")
    }

    /// Parse and validate. Refuses, in this order: bytes past the ceiling (before any parsing, so
    /// an oversized blob costs one comparison), bytes that do not parse (including trailing
    /// bytes), an automaton that fails [`Self::validate`], and bytes that are not the canonical
    /// serialization of what they parsed to — so one automaton has one id.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PalwDecodeConstraintError> {
        if bytes.len() > PALW_CONSTRAINT_MAX_BYTES_V1 {
            return Err(PalwDecodeConstraintError::TooLarge(bytes.len()));
        }
        let parsed: Self = borsh::from_slice(bytes).map_err(|e| PalwDecodeConstraintError::Malformed(e.to_string()))?;
        parsed.validate()?;
        if parsed.to_bytes() != bytes {
            return Err(PalwDecodeConstraintError::NotCanonical);
        }
        Ok(parsed)
    }

    /// [`constraint_id_v1`] over [`Self::to_bytes`].
    pub fn id(&self) -> Hash64 {
        constraint_id_v1(&self.to_bytes())
    }

    /// Every node of every frame.
    pub fn node_count(&self) -> usize {
        self.frames.iter().map(|f| f.nodes.len()).sum()
    }

    fn node(&self, frame: u16, node: u16) -> Option<&PalwConstraintNodeV1> {
        self.frames.get(frame as usize)?.nodes.get(node as usize)
    }
}

/// **`constraint_id = H("misaka-palw/constraint/v1" ‖ len_le64 ‖ bytes)`** (ADR-0096 Decision 7)
/// — `canonical_id`'s construction from `palw_freeprompt_v3` under this domain: a keyed
/// BLAKE2b-512 whose key is the domain, fed the byte string's length as a little-endian `u64` and
/// then the bytes. The length prefix makes it a function of one byte string rather than of a
/// concatenation. Equal, byte for byte, to `misaka_palw_constraint::constraint_id` over the same
/// bytes; that crate's test holds the two spellings together.
pub fn constraint_id_v1(canonical_bytes: &[u8]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_CONSTRAINT_DOMAIN_V1).to_state();
    state.update(&(canonical_bytes.len() as u64).to_le_bytes());
    state.update(canonical_bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **The runtime state: where the automaton is after a byte string.** A pure function of
/// `(constraint, bytes)`, serializable so two hosts can pin a state sequence (invariant 4).
#[derive(Clone, Debug, PartialEq, Eq, Hash, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwConstraintStateV1 {
    pub frame: u16,
    pub node: u16,
    /// This frame's key mask: bit `i` set once `Key { index: i }` has been read.
    pub seen: u16,
    /// The frames beneath this one, innermost last. At most [`PALW_CONSTRAINT_MAX_DEPTH_V1`].
    pub stack: Vec<PalwConstraintStackEntryV1>,
}

/// What a `Push` saves and a `Pop` restores.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwConstraintStackEntryV1 {
    pub frame: u16,
    pub resume: u16,
    pub seen: u16,
}

/// The state before any byte: the start frame at its start node, an empty mask, an empty stack.
pub fn constraint_start_state_v1(c: &PalwDecodeConstraintV1) -> PalwConstraintStateV1 {
    PalwConstraintStateV1 {
        frame: c.start_frame,
        node: c.frames.get(c.start_frame as usize).map(|f| f.start).unwrap_or(0),
        seen: 0,
        stack: Vec::new(),
    }
}

/// The action of `node` on `byte`, by binary search over the sorted disjoint ranges.
fn edge_action(node: &PalwConstraintNodeV1, byte: u8) -> Option<PalwConstraintActionV1> {
    let after = node.edges.partition_point(|e| e.lo <= byte);
    let edge = node.edges.get(after.checked_sub(1)?)?;
    (byte <= edge.hi).then_some(edge.action)
}

/// **One byte.** `true` and `state` advanced, or `false` for dead — in which case `state` is
/// unspecified and the caller discards it (every public entry point below clones first). Total:
/// no byte string makes this loop, whatever the automaton, because epsilon moves are counted.
pub fn constraint_step_v1(c: &PalwDecodeConstraintV1, state: &mut PalwConstraintStateV1, byte: u8) -> bool {
    let mut hops = 0usize;
    loop {
        let Some(node) = c.node(state.frame, state.node) else { return false };
        let Some(action) = edge_action(node, byte) else { return false };
        match action {
            PalwConstraintActionV1::Goto(next) => {
                state.node = next;
                return true;
            }
            PalwConstraintActionV1::Push { frame, resume } => {
                if state.stack.len() >= PALW_CONSTRAINT_MAX_DEPTH_V1 {
                    return false;
                }
                let Some(target) = c.frames.get(frame as usize) else { return false };
                state.stack.push(PalwConstraintStackEntryV1 { frame: state.frame, resume, seen: state.seen });
                state.frame = frame;
                state.node = target.start;
                state.seen = 0;
            }
            PalwConstraintActionV1::Pop => {
                let Some(entry) = state.stack.pop() else { return false };
                state.frame = entry.frame;
                state.node = entry.resume;
                state.seen = entry.seen;
            }
            PalwConstraintActionV1::Key { index, next } => {
                let Some(bit) = 1u16.checked_shl(index as u32) else { return false };
                if state.seen & bit != 0 {
                    return false;
                }
                state.seen |= bit;
                state.node = next;
                return true;
            }
            PalwConstraintActionV1::Close { required, next } => {
                if state.seen & required != required {
                    return false;
                }
                state.node = next;
                return true;
            }
        }
        // Only the epsilon moves reach here; the byte is read again in the new frame.
        hops += 1;
        if hops > PALW_CONSTRAINT_MAX_HOPS_V1 {
            return false;
        }
    }
}

/// **`admit(s, bytes)`** (ADR-0096 Decision 7): feed the token's bytes from `state`; `Some(next)`
/// when every byte lands on a live node, `None` the moment one is dead. This is what the
/// producer's mask asks of every lane and what the court asks of the committed lane and the
/// beating lane; one function so the two cannot drift.
pub fn constraint_admits_v1(
    c: &PalwDecodeConstraintV1,
    state: &PalwConstraintStateV1,
    token_bytes: &[u8],
) -> Option<PalwConstraintStateV1> {
    let mut next = state.clone();
    for &byte in token_bytes {
        if !constraint_step_v1(c, &mut next, byte) {
            return None;
        }
    }
    Some(next)
}

/// `admit` over a lane's rendering as the class table gives it: **an id the table cannot render
/// (`None`) is not admitted** — rule 1 of the module header.
pub fn constraint_admits_lane_v1(
    c: &PalwDecodeConstraintV1,
    state: &PalwConstraintStateV1,
    rendering: Option<&[u8]>,
) -> Option<PalwConstraintStateV1> {
    // **An empty rendering is never admitted** (ADR-0096 §10 B4): the table gives special tokens
    // and unrenderable ids the empty leaf, and a token that advances no byte would otherwise be
    // admitted at EVERY state — an end-of-generation id in the middle of an answer.
    rendering.filter(|bytes| !bytes.is_empty()).and_then(|bytes| constraint_admits_v1(c, state, bytes))
}

/// The state after a rendered prefix — the tokens' byte strings in order, from the start state.
/// `None` when the prefix is not admitted (dead at some byte), which for the court means an
/// earlier position is the one to challenge.
pub fn constraint_state_after_v1<I, B>(c: &PalwDecodeConstraintV1, prefix: I) -> Option<PalwConstraintStateV1>
where
    I: IntoIterator<Item = B>,
    B: AsRef<[u8]>,
{
    let mut state = constraint_start_state_v1(c);
    for token in prefix {
        for &byte in token.as_ref() {
            if !constraint_step_v1(c, &mut state, byte) {
                return None;
            }
        }
    }
    Some(state)
}

/// A complete answer: the stack is empty and the node is accepting.
pub fn constraint_is_accepting_v1(c: &PalwDecodeConstraintV1, state: &PalwConstraintStateV1) -> bool {
    state.stack.is_empty() && c.node(state.frame, state.node).is_some_and(|n| n.accepting)
}

/// **The producer's mask at one position**: `admitted[j]` for every lane of the vocabulary, from
/// the class table. This is §4's cost — one automaton step per byte of every token — measured by
/// `decode_constraint_admit_cost_over_a_qwen_sized_table`.
pub fn constraint_admitted_lanes_v1(
    c: &PalwDecodeConstraintV1,
    state: &PalwConstraintStateV1,
    vocab: u32,
    token_bytes: &dyn Fn(u32) -> Option<Vec<u8>>,
) -> Vec<bool> {
    (0..vocab).map(|lane| constraint_admits_lane_v1(c, state, token_bytes(lane).as_deref()).is_some()).collect()
}

/// Is any lane of the vocabulary admitted from `state`? The question the court asks before it
/// reads Decision 7's stop rule; it stops at the first admitted lane.
pub fn constraint_admits_any_lane_v1(
    c: &PalwDecodeConstraintV1,
    state: &PalwConstraintStateV1,
    vocab: u32,
    token_bytes: &dyn Fn(u32) -> Option<Vec<u8>>,
) -> bool {
    (0..vocab).any(|lane| constraint_admits_lane_v1(c, state, token_bytes(lane).as_deref()).is_some())
}

/// **The selection rule v3** (ADR-0096 Decision 7): the argmax of
/// [`decode_lane_key_v2`] over the lanes `j` with `admitted(j)`, ties to the LOWEST index — the
/// v2 rule's own tie — and `None` when no lane is admitted, in which case **the caller commits the
/// lowest EOG id of the class and stops the run with `EndOfGeneration`**; the token is committed
/// by rule, not selected, and the court tries the rule (see
/// `palw_step_refute::check_tiled_decode_token_refutation_v3`).
///
/// Mask, then key: the constraint decides the candidate set and ADR-0082 Decision 11's key orders
/// it. With `admitted` identically true this is [`crate::palw_decode_select_v2::decode_token_select_v2`]
/// on every non-empty row, and at `T_q = 0` therefore
/// [`crate::palw_step_refute::base0_decode_token_select_v1`] — invariant 1, swept by
/// `decode_token_select_v3_is_v2_when_every_lane_is_admitted`. (An empty row has no lane and
/// answers `None`; v2's `0` on an empty row is a convention this rule does not repeat, and a
/// registered vocabulary is never empty.)
pub fn decode_token_select_v3(
    values: &[i32],
    seed: &[u8; 32],
    position: u32,
    temperature_q: u32,
    admitted: impl Fn(usize) -> bool,
) -> Option<usize> {
    let mut best: Option<(usize, i64)> = None;
    for (lane, value) in values.iter().enumerate() {
        if !admitted(lane) {
            continue;
        }
        let key = decode_lane_key_v2(*value, seed, position, lane, temperature_q);
        best = match best {
            Some((best_lane, best_key)) if !decode_lane_beats_v2(key, lane, best_key, best_lane) => Some((best_lane, best_key)),
            _ => Some((lane, key)),
        };
    }
    best.map(|(lane, _)| lane)
}

/// **Does any byte continue from this state?** 256 automaton steps, no table.
///
/// This is B7's finish test (ADR-0096 §10): for a BYTE-COMPLETE vocabulary — every single byte
/// is a token that renders as itself, which a byte-level BPE vocabulary is, and which the pinned
/// token table certifies — "no lane is admitted" is exactly "no byte continues", so the court can
/// decide the stop from the automaton alone rather than from a table no node holds.
pub fn constraint_admits_any_byte_v1(c: &PalwDecodeConstraintV1, state: &PalwConstraintStateV1) -> bool {
    (0u8..=255).any(|byte| {
        let mut next = state.clone();
        constraint_step_v1(c, &mut next, byte)
    })
}

/// **Where a constrained run is: still reading the answer, or finished** (ADR-0096 §10 B7).
#[derive(Clone, Debug, PartialEq, Eq, Hash, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwConstraintCursorV1 {
    Running(PalwConstraintStateV1),
    /// No byte continued at some position; from there every committed token is the lowest EOG id.
    Finished,
}

pub fn constraint_cursor_start_v1(c: &PalwDecodeConstraintV1) -> PalwConstraintCursorV1 {
    PalwConstraintCursorV1::Running(constraint_start_state_v1(c))
}

/// **Advance past one committed token under B7's rule**, or `None` if the rule forbids it.
///
/// * Running, and some byte continues: the token must be admitted — a NON-EMPTY rendering whose
///   every byte steps alive — and the cursor runs on from the state after it.
/// * Running, and no byte continues: the token must be the class's lowest EOG id, and the run is
///   finished.
/// * Finished: the token must be the lowest EOG id again. The run still ends at its declared
///   budget, so the job context is the one built before the run, exactly as today.
///
/// `token_bytes` is the token's rendering as the table gives it (empty for a special or an
/// unrenderable id); `is_lowest_eog` is whether the token is the class's lowest EOG id.
pub fn constraint_cursor_advance_v1(
    c: &PalwDecodeConstraintV1,
    cursor: &PalwConstraintCursorV1,
    token_bytes: &[u8],
    is_lowest_eog: bool,
) -> Option<PalwConstraintCursorV1> {
    match cursor {
        PalwConstraintCursorV1::Finished => is_lowest_eog.then_some(PalwConstraintCursorV1::Finished),
        PalwConstraintCursorV1::Running(state) => {
            if !constraint_admits_any_byte_v1(c, state) {
                return is_lowest_eog.then_some(PalwConstraintCursorV1::Finished);
            }
            constraint_admits_lane_v1(c, state, Some(token_bytes)).map(PalwConstraintCursorV1::Running)
        }
    }
}

/// The domain of [`rendered_segments_hash_v1`].
pub const PALW_RENDERED_SEGMENTS_DOMAIN_V1: &[u8] = b"misaka-palw/rendered-segments/v1";

/// **ADR-0096 §10 B3: a version-6 answer's rendered-output hash, token by token.**
/// `H(domain ‖ count_le32 ‖ (len_le32 ‖ bytes)*)` over each committed token's rendering as the
/// pinned table gives it, in order — so `output_root` (which already binds the ids) binds each
/// position's BYTES too, and a court close can carry the rendering and have it checked against the
/// claim's own root. The same keyed BLAKE2b-512 as [`constraint_id_v1`].
pub fn rendered_segments_hash_v1<S: AsRef<[u8]>>(segments: &[S]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_RENDERED_SEGMENTS_DOMAIN_V1).to_state();
    state.update(&(segments.len() as u32).to_le_bytes());
    for segment in segments {
        let bytes = segment.as_ref();
        state.update(&(bytes.len() as u32).to_le_bytes());
        state.update(bytes);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **What the court receives when a claim carried a constraint** — bound to the claim by the
/// caller, never stated by the challenger (a challenger who could state the constraint could
/// state "none" and convict an honest masked token; one who could state the table could render
/// an honest token into garbage).
///
/// Three things, because Decision 7's rule needs three: the automaton (from the job's
/// `constraint_id`, resolved through the served answer envelope), the class's token-to-bytes
/// table (the artifact's `tokenizer_commitment`, served beside it), and the class's
/// `eog_token_ids` (the worker manifest's) — without the last the court could see that nothing
/// is admitted but not whether the token committed there is the one the rule names.
pub struct PalwDecodeConstraintCourtV1<'a> {
    pub constraint: &'a PalwDecodeConstraintV1,
    /// **The answer's per-token renderings, as the CLAIM committed them** (ADR-0096 §10 B5) — one
    /// per decode position, checked by the caller against the claim's `output_root` through
    /// [`rendered_segments_hash_v1`]. The court never renders a token itself: no node holds a
    /// tokenizer.
    pub segments: &'a [Vec<u8>],
    /// **The beating lane's rendering, proven by the caller against the pinned token table**
    /// (§10 B4); `None` for the one-disclosure arm, which reads no lane but the committed one.
    pub beat_lane_bytes: Option<&'a [u8]>,
    /// The ids at which generation ends for this class; the rule commits the LOWEST of them.
    pub eog_token_ids: &'a [u32],
}

impl PalwDecodeConstraintCourtV1<'_> {
    /// The id Decision 7's stop rule commits.
    pub fn lowest_eog_id(&self) -> Option<u32> {
        self.eog_token_ids.iter().copied().min()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::palw_decode_select_v2::{
        PALW_DECODE_SEED_GREEDY, PALW_DECODE_TEMPERATURE_GREEDY, PalwDecodeSamplingV2, decode_token_select_v2,
    };
    use crate::palw_step_leg::PalwStepOpeningV1;
    use crate::palw_step_refute::{
        PALW_LOGITS_TILE_LANES, PalwStepRefuteError, PalwTiledDecodePinV1, base0_decode_token_select_v1,
        check_tiled_decode_token_refutation_capped_v2, check_tiled_decode_token_refutation_v3, tiled_logits_row_root_v1,
        tiled_logits_scheme_id_v1, tiled_logits_tile_leaf_v1, tiled_logits_trace_root_v1,
    };
    use PalwConstraintActionV1::{Close, Goto, Key, Pop, Push};

    // ---------------------------------------------------------------------------------------
    // A test-side frame assembler. The compiler lives in the leaf crate; consensus-core's tests
    // build automata by hand so what is pinned here is the FORM and the STEP, not a compiler.
    // ---------------------------------------------------------------------------------------

    struct Fb {
        start: u16,
        nodes: Vec<PalwConstraintNodeV1>,
    }

    impl Fb {
        fn new() -> Self {
            Fb { start: 0, nodes: Vec::new() }
        }
        fn node(&mut self, accepting: bool) -> u16 {
            self.nodes.push(PalwConstraintNodeV1 { accepting, edges: Vec::new() });
            (self.nodes.len() - 1) as u16
        }
        fn range(&mut self, from: u16, lo: u8, hi: u8, action: PalwConstraintActionV1) {
            let edges = &mut self.nodes[from as usize].edges;
            edges.push(PalwConstraintEdgeV1 { lo, hi, action });
            edges.sort_by_key(|e| e.lo);
        }
        fn byte(&mut self, from: u16, b: u8, action: PalwConstraintActionV1) {
            self.range(from, b, b, action);
        }
        fn bytes(&mut self, from: u16, bs: &[u8], action: PalwConstraintActionV1) {
            for &b in bs {
                self.byte(from, b, action);
            }
        }
        fn frame(self) -> PalwConstraintFrameV1 {
            PalwConstraintFrameV1 { start: self.start, nodes: self.nodes }
        }
    }

    fn assemble(frames: Vec<PalwConstraintFrameV1>, start_frame: u16) -> PalwDecodeConstraintV1 {
        let c = PalwDecodeConstraintV1 {
            version: PALW_DECODE_CONSTRAINT_VERSION_V1,
            compiler_id: Hash64::from_u64_word(0x0096_0007),
            start_frame,
            frames,
        };
        c.validate().expect("the test's automata are well formed");
        c
    }

    /// **Nested integer arrays**: `[` … `]` with `,`-separated items that are integers or arrays,
    /// the frame pushing ITSELF (at index `own`) for every item — every action but `Key`/`Close`,
    /// and the depth.
    fn int_arrays_frame(own: u16) -> PalwConstraintFrameV1 {
        let mut f = Fb::new();
        let s = f.node(false);
        let open = f.node(false);
        let int = f.node(true);
        let after = f.node(false);
        let next = f.node(false);
        let done = f.node(true);
        f.byte(s, b'[', Goto(open));
        f.range(s, b'0', b'9', Goto(int));
        f.byte(open, b']', Goto(done));
        f.byte(open, b'[', Push { frame: own, resume: after });
        f.range(open, b'0', b'9', Push { frame: own, resume: after });
        f.range(int, b'0', b'9', Goto(int));
        f.bytes(int, b",]}", Pop);
        f.byte(after, b',', Goto(next));
        f.byte(after, b']', Goto(done));
        f.byte(next, b'[', Push { frame: own, resume: after });
        f.range(next, b'0', b'9', Push { frame: own, resume: after });
        f.bytes(done, b",]}", Pop);
        f.frame()
    }

    fn nested_int_arrays() -> PalwDecodeConstraintV1 {
        assemble(vec![int_arrays_frame(0)], 0)
    }

    /// **An object with keys `a` (required) and `b`**, values from the array grammar above, keys
    /// in any order and each at most once — `Key` and `Close` over the mask.
    fn object_ab() -> PalwDecodeConstraintV1 {
        let values = int_arrays_frame(1);
        let mut f = Fb::new();
        let s = f.node(false);
        let open = f.node(false);
        let k0 = f.node(false);
        let ka = f.node(false);
        let kb = f.node(false);
        let colon = f.node(false);
        let val = f.node(false);
        let after = f.node(false);
        let next_key = f.node(false);
        let done = f.node(true);
        f.byte(s, b'{', Goto(open));
        f.byte(open, b'}', Close { required: 0b01, next: done });
        f.byte(open, b'"', Goto(k0));
        f.byte(k0, b'a', Goto(ka));
        f.byte(k0, b'b', Goto(kb));
        f.byte(ka, b'"', Key { index: 0, next: colon });
        f.byte(kb, b'"', Key { index: 1, next: colon });
        f.byte(colon, b':', Goto(val));
        f.byte(val, b'[', Push { frame: 1, resume: after });
        f.range(val, b'0', b'9', Push { frame: 1, resume: after });
        f.byte(after, b',', Goto(next_key));
        f.byte(after, b'}', Close { required: 0b01, next: done });
        f.byte(next_key, b'"', Goto(k0));
        f.bytes(done, b",}]", Pop);
        assemble(vec![f.frame(), values], 0)
    }

    /// **`[` one digit `]`**, and then nothing — the smallest grammar whose run ENDS, so the
    /// "no lane admitted → lowest EOG" rule is reachable by the court.
    pub(crate) fn bracket_digit() -> PalwDecodeConstraintV1 {
        let mut f = Fb::new();
        let s = f.node(false);
        let open = f.node(false);
        let digit = f.node(false);
        let done = f.node(true);
        f.byte(s, b'[', Goto(open));
        f.range(open, b'0', b'9', Goto(digit));
        f.byte(digit, b']', Goto(done));
        assemble(vec![f.frame()], 0)
    }

    /// **A JSON string with well-formed UTF-8 and RFC 8785's escapes** — the shape of the
    /// producer's heaviest state (most of a byte-level vocabulary is admitted in a string), used
    /// by the cost measurement and by the multi-byte admission checks.
    fn json_string() -> (PalwDecodeConstraintV1, u16) {
        let mut f = Fb::new();
        let s = f.node(false);
        let t = f.node(false);
        let done = f.node(true);
        let esc = f.node(false);
        let (c1, c2, c3) = (f.node(false), f.node(false), f.node(false));
        let (e0, ed, f0, f4) = (f.node(false), f.node(false), f.node(false), f.node(false));
        let (u, u0, u00, u000, u001) = (f.node(false), f.node(false), f.node(false), f.node(false), f.node(false));
        f.byte(s, b'"', Goto(t));
        f.byte(t, b'"', Goto(done));
        f.byte(t, b'\\', Goto(esc));
        f.range(t, 0x20, 0x21, Goto(t));
        f.range(t, 0x23, 0x5b, Goto(t));
        f.range(t, 0x5d, 0x7f, Goto(t));
        f.range(t, 0xc2, 0xdf, Goto(c1));
        f.byte(t, 0xe0, Goto(e0));
        f.range(t, 0xe1, 0xec, Goto(c2));
        f.byte(t, 0xed, Goto(ed));
        f.range(t, 0xee, 0xef, Goto(c2));
        f.byte(t, 0xf0, Goto(f0));
        f.range(t, 0xf1, 0xf3, Goto(c3));
        f.byte(t, 0xf4, Goto(f4));
        f.range(c1, 0x80, 0xbf, Goto(t));
        f.range(c2, 0x80, 0xbf, Goto(c1));
        f.range(c3, 0x80, 0xbf, Goto(c2));
        f.range(e0, 0xa0, 0xbf, Goto(c1));
        f.range(ed, 0x80, 0x9f, Goto(c1));
        f.range(f0, 0x90, 0xbf, Goto(c2));
        f.range(f4, 0x80, 0x8f, Goto(c2));
        f.bytes(esc, b"\"\\bfnrt", Goto(t));
        f.byte(esc, b'u', Goto(u));
        f.byte(u, b'0', Goto(u0));
        f.byte(u0, b'0', Goto(u00));
        f.byte(u00, b'0', Goto(u000));
        f.byte(u00, b'1', Goto(u001));
        f.range(u000, b'0', b'7', Goto(t));
        f.bytes(u000, b"bef", Goto(t));
        f.range(u001, b'0', b'9', Goto(t));
        f.range(u001, b'a', b'f', Goto(t));
        f.bytes(done, b",}]", Pop);
        (assemble(vec![f.frame()], 0), t)
    }

    fn after(c: &PalwDecodeConstraintV1, text: &[u8]) -> Option<PalwConstraintStateV1> {
        constraint_state_after_v1(c, [text])
    }

    fn accepts(c: &PalwDecodeConstraintV1, text: &[u8]) -> bool {
        after(c, text).is_some_and(|s| constraint_is_accepting_v1(c, &s))
    }

    // ---------------------------------------------------------------------------------------
    // The form: bytes, bounds, the id
    // ---------------------------------------------------------------------------------------

    /// The bytes parse back to the same automaton with the same id, and every structural rule
    /// refuses by name — a target past the frame, an unsorted edge, a key past the mask, a
    /// version that is not 1, a trailing byte, and the size ceiling.
    #[test]
    fn decode_constraint_round_trips_and_refuses_what_is_not_well_formed() {
        for c in [nested_int_arrays(), object_ab(), bracket_digit(), json_string().0] {
            let bytes = c.to_bytes();
            let back = PalwDecodeConstraintV1::from_bytes(&bytes).expect("canonical bytes parse");
            assert_eq!(back, c);
            assert_eq!(back.id(), c.id());
            assert_eq!(constraint_id_v1(&bytes), c.id());
            assert!(bytes.len() < 2_048, "these automata are small: {} bytes", bytes.len());
            let mut trailing = bytes.clone();
            trailing.push(0);
            assert!(matches!(PalwDecodeConstraintV1::from_bytes(&trailing), Err(PalwDecodeConstraintError::Malformed(_))));
        }
        let bad = |edit: &dyn Fn(&mut PalwDecodeConstraintV1)| {
            let mut c = object_ab();
            edit(&mut c);
            c.validate().expect_err("must be refused")
        };
        let invalid = |e: PalwDecodeConstraintError| match e {
            PalwDecodeConstraintError::Invalid(why) => why,
            other => panic!("expected Invalid, got {other:?}"),
        };
        assert_eq!(invalid(bad(&|c| c.version = 2)), "the header version is not 1");
        assert_eq!(invalid(bad(&|c| c.start_frame = 7)), "the start frame is past the frame list");
        assert_eq!(invalid(bad(&|c| c.frames.clear())), "an automaton has at least one frame");
        assert_eq!(invalid(bad(&|c| c.frames[0].start = 99)), "a frame's start node is past its node list");
        assert_eq!(invalid(bad(&|c| c.frames[0].nodes.clear())), "a frame has no nodes");
        assert_eq!(invalid(bad(&|c| c.frames[0].nodes[0].edges[0].action = Goto(200))), "an edge targets a node past its frame");
        assert_eq!(
            invalid(bad(&|c| c.frames[0].nodes[6].edges[0].action = Push { frame: 9, resume: 0 })),
            "a push names a frame past the frame list"
        );
        assert_eq!(
            invalid(bad(&|c| c.frames[0].nodes[6].edges[0].action = Push { frame: 1, resume: 90 })),
            "a push's resume node is past its frame"
        );
        assert_eq!(
            invalid(bad(&|c| c.frames[0].nodes[3].edges[0].action = Key { index: 16, next: 0 })),
            "a key index is past the sixteen slots of the mask"
        );
        assert_eq!(invalid(bad(&|c| c.frames[0].nodes[1].edges.swap(0, 1))), "a node's edges are not sorted and disjoint");
        assert_eq!(
            invalid(bad(&|c| c.frames[0].nodes[1].edges.push(PalwConstraintEdgeV1 { lo: b'}', hi: b'}', action: Pop }))),
            "a node's edges are not sorted and disjoint"
        );
        assert_eq!(invalid(bad(&|c| c.frames[0].nodes[1].edges[0].hi = 0)), "an edge's low byte is above its high byte");
        // The ceiling, both ways: a validated automaton above it is refused with its size, and
        // bytes above it are refused before parsing.
        let mut fat = nested_int_arrays();
        let filler = PalwConstraintNodeV1 { accepting: false, edges: vec![PalwConstraintEdgeV1 { lo: 0, hi: 255, action: Pop }] };
        while fat.to_bytes().len() <= PALW_CONSTRAINT_MAX_BYTES_V1 {
            fat.frames[0].nodes.extend(std::iter::repeat_n(filler.clone(), 512));
        }
        let size = fat.to_bytes().len();
        assert_eq!(fat.validate(), Err(PalwDecodeConstraintError::TooLarge(size)));
        assert_eq!(
            PalwDecodeConstraintV1::from_bytes(&vec![0u8; PALW_CONSTRAINT_MAX_BYTES_V1 + 1]),
            Err(PalwDecodeConstraintError::TooLarge(65_537))
        );
        assert_eq!(PALW_CONSTRAINT_MAX_BYTES_V1, 65_536);
        assert_eq!(PALW_CONSTRAINT_MAX_DEPTH_V1, 16);
    }

    /// **The id is `canonical_id`'s construction under the constraint domain**: re-spelled with
    /// the tree's keyed helper, length-prefixed, and moved by one byte of either. The equality
    /// with the leaf crate's `constraint_id` is pinned THERE (it can import this crate).
    #[test]
    fn decode_constraint_id_is_the_keyed_length_prefixed_construction() {
        let bytes = bracket_digit().to_bytes();
        let mut preimage = (bytes.len() as u64).to_le_bytes().to_vec();
        preimage.extend_from_slice(&bytes);
        assert_eq!(constraint_id_v1(&bytes), kaspa_hashes::blake2b_512_keyed(PALW_CONSTRAINT_DOMAIN_V1, &preimage));
        assert_ne!(constraint_id_v1(&bytes), constraint_id_v1(&bytes[..bytes.len() - 1]));
        assert_ne!(constraint_id_v1(&bytes), kaspa_hashes::blake2b_512_keyed(PALW_CONSTRAINT_DOMAIN_V1, &bytes), "the length prefix");
        assert_ne!(constraint_id_v1(&[]), Hash64::default(), "an empty byte string is still an id, never `none`");
        assert_eq!(PALW_CONSTRAINT_DOMAIN_V1, b"misaka-palw/constraint/v1");
        // Two automata that differ only in the compiler id are two constraints: the header is in
        // the preimage.
        let mut other = bracket_digit();
        other.compiler_id = Hash64::from_u64_word(1);
        assert_ne!(other.id(), bracket_digit().id());
    }

    // ---------------------------------------------------------------------------------------
    // The step
    // ---------------------------------------------------------------------------------------

    /// Push, pop and re-dispatch over nested arrays: what the grammar admits is accepted, what it
    /// does not is dead at the first offending byte, a complete root value admits nothing, and
    /// the depth is exactly the pinned one.
    #[test]
    fn decode_constraint_admits_the_grammar_and_dies_outside_it() {
        let c = nested_int_arrays();
        for ok in [&b"[]"[..], b"[1]", b"[12,3]", b"[[1],[2,[3,4]],5]", b"7", b"[[[]]]", b"[1,[],2]"] {
            assert!(accepts(&c, ok), "{:?} is in the grammar", String::from_utf8_lossy(ok));
        }
        // Complete but not accepted: an open value.
        for open in [&b"["[..], b"[1", b"[1,", b"[[1]", b"[1,[2"] {
            let s = after(&c, open).expect("a prefix of the grammar is live");
            assert!(!constraint_is_accepting_v1(&c, &s), "{:?} is not complete", String::from_utf8_lossy(open));
        }
        // Dead: the first byte outside the grammar kills, and dead is absorbing.
        for dead in [&b"[,]"[..], b"[1,]", b"]", b"[1]]", b"[1] ", b" [1]", b"[1]2", b"7,", b"[a]", b"[1 ,2]", b"[1]["] {
            assert!(after(&c, dead).is_none(), "{:?} is outside the grammar", String::from_utf8_lossy(dead));
        }
        // After a complete root value, NOTHING is admitted — every byte, including the ones the
        // frame has Pop edges for (the stack is empty).
        let done = after(&c, b"[1]").unwrap();
        assert!((0..=255u8).all(|b| constraint_admits_v1(&c, &done, &[b]).is_none()));
        // A token is admitted or not as a whole: `1,` from `[` is fine; `1,]` is not.
        let opened = after(&c, b"[").unwrap();
        assert!(constraint_admits_v1(&c, &opened, b"1,").is_some());
        assert!(constraint_admits_v1(&c, &opened, b"1,]").is_none());
        assert!(constraint_admits_v1(&c, &opened, b"").is_some(), "an empty rendering is admitted trivially");
        assert_eq!(constraint_admits_v1(&c, &opened, b"").unwrap(), opened, "and moves nothing");
        // The depth: the root frame plus sixteen pushed frames is the ceiling. Sixteen brackets
        // and a digit reach it (fifteen array pushes, one integer push); seventeen brackets reach
        // it too, and there an item — which is a push — is dead while closing is live.
        let mut sixteen_and_a_digit = vec![b'['; 16];
        sixteen_and_a_digit.push(b'1');
        let at_ceiling = after(&c, &sixteen_and_a_digit).expect("a value at the ceiling is live");
        assert_eq!(at_ceiling.stack.len(), PALW_CONSTRAINT_MAX_DEPTH_V1);
        let seventeen = vec![b'['; 17];
        let deep = after(&c, &seventeen).expect("sixteen pushes are admitted");
        assert_eq!(deep.stack.len(), PALW_CONSTRAINT_MAX_DEPTH_V1);
        assert!(constraint_admits_v1(&c, &deep, b"[").is_none(), "the seventeenth push is dead");
        assert!(constraint_admits_v1(&c, &deep, b"1").is_none(), "an item at the ceiling is a push, and dead");
        assert!(constraint_admits_v1(&c, &deep, b"]").is_some(), "closing needs no push");
        let mut closed = seventeen.clone();
        closed.extend(std::iter::repeat_n(b']', 17));
        assert!(accepts(&c, &closed));
    }

    /// `Key` and `Close` over the mask: any order, each at most once, the required one present,
    /// and the mask is per frame — a nested value neither sees nor disturbs its parent's keys.
    #[test]
    fn decode_constraint_keys_are_any_order_at_most_once_and_required_closes() {
        let c = object_ab();
        for ok in [&br#"{"a":1}"#[..], br#"{"a":1,"b":2}"#, br#"{"b":2,"a":1}"#, br#"{"a":[1,[2,3]],"b":[]}"#] {
            assert!(accepts(&c, ok), "{:?}", String::from_utf8_lossy(ok));
        }
        // Missing the required key: dead at the `}` that would close without it.
        assert!(after(&c, br#"{"b":2"#).is_some());
        assert!(after(&c, br#"{"b":2}"#).is_none());
        assert!(after(&c, b"{}").is_none());
        // Twice: dead at the second key's closing quote.
        assert!(after(&c, br#"{"a":1,"a"#).is_some());
        assert!(after(&c, br#"{"a":1,"a""#).is_none());
        assert!(after(&c, br#"{"b":1,"a":2,"b""#).is_none());
        // An undeclared key is dead at its first byte (this grammar is closed).
        assert!(after(&c, br#"{"c"#).is_none());
        // The mask is saved on push and restored on pop — and the pop is decided by the NEXT
        // byte: after the array's `]` the state is still the array frame's, the object's mask
        // waiting on the stack, until the `,` or `}` that pops it.
        let s = after(&c, br#"{"a":[1,2]"#).unwrap();
        assert_eq!(s.stack.len(), 1, "the array's `]` is read; the pop waits for the next byte");
        assert_eq!(s.stack[0].seen, 0b01, "the object's mask waits on the stack");
        assert_eq!(s.seen, 0, "the array frame has its own, empty, mask");
        let s = after(&c, br#"{"a":[1,2],"#).unwrap();
        assert!(s.stack.is_empty());
        assert_eq!(s.seen, 0b01, "restored by the pop");
        let inner = after(&c, br#"{"a":[1"#).unwrap();
        assert_eq!(inner.seen, 0, "the array frame has its own, empty, mask");
        assert_eq!(inner.stack.len(), 2, "object → array → integer");
        assert_eq!(inner.stack[0].seen, 0b01, "the object's mask waits on the stack");
    }

    /// Multi-byte tokens against the string grammar: a token that straddles a UTF-8 sequence is
    /// admitted, one that breaks it is not, and RFC 8785's escapes are exactly the admitted ones.
    #[test]
    fn decode_constraint_string_frame_reads_utf8_and_the_canonical_escapes() {
        let (c, _) = json_string();
        let canonical = concat!(r#""a\"b\\c\nd\u"#, "001f", r#"\u"#, "000b", "\"");
        for ok in ["\"\"", "\"héllo\"", "\"日本語\"", "\"😂\"", canonical, "\"\u{7f}\u{2028}/\""] {
            assert!(accepts(&c, ok.as_bytes()), "{ok:?}");
        }
        for dead in [
            "\"\u{1}\"",
            r#""\/""#,
            concat!(r#""\u"#, "0041", "\""),
            concat!(r#""\u"#, "0008", "\""),
            concat!(r#""\u"#, "001F", "\""),
            r#""\x""#,
            "\"a\nb\"",
            "\"\u{7f}",
            "\"x\"y",
            "\"x\" ",
            "\"\"\"",
        ] {
            assert!(!accepts(&c, dead.as_bytes()), "{dead:?}");
        }
        // Raw invalid UTF-8: an overlong encoding, a surrogate, a bare continuation, a truncated
        // sequence followed by ASCII.
        for dead in
            [&[b'"', 0xc0, 0x80, b'"'][..], &[b'"', 0xed, 0xa0, 0x80, b'"'], &[b'"', 0x80, b'"'], &[b'"', 0xe3, 0x81, b'a', b'"']]
        {
            assert!(after(&c, dead).is_none(), "{dead:x?}");
        }
        // A token holding half of a character is admitted, and the next token must finish it.
        let open = after(&c, b"\"").unwrap();
        let half = constraint_admits_v1(&c, &open, &[0xe6, 0x97]).expect("two bytes of 日 are live");
        assert!(constraint_admits_v1(&c, &half, &[0xa5]).is_some());
        assert!(constraint_admits_v1(&c, &half, b"a").is_none());
        assert!(constraint_admits_v1(&c, &half, b"\"").is_none());
    }

    /// **Invariant 4**: the state sequence over a prefix is a function of the constraint bytes
    /// and the prefix alone — the same on the automaton, on its parse from bytes on "another
    /// host", and on a second run; and the sequence itself serializes, so two hosts can compare
    /// it rather than trust each other.
    #[test]
    fn decode_constraint_state_sequence_is_pinned_across_serializations() {
        let here = object_ab();
        let there = PalwDecodeConstraintV1::from_bytes(&here.to_bytes()).unwrap();
        let prefix: Vec<&[u8]> = vec![b"{", b"\"a\"", b":", b"[1", b",", b"[2", b"]]", b",\"b\":", b"3", b"}"];
        let sequence = |c: &PalwDecodeConstraintV1| -> Vec<Vec<u8>> {
            let mut s = constraint_start_state_v1(c);
            let mut out = vec![borsh::to_vec(&s).unwrap()];
            for token in &prefix {
                s = constraint_admits_v1(c, &s, token).expect("the prefix is in the grammar");
                out.push(borsh::to_vec(&s).unwrap());
            }
            out
        };
        let a = sequence(&here);
        assert_eq!(a, sequence(&there));
        assert_eq!(a, sequence(&here), "and deterministic on one host");
        assert_eq!(a.len(), prefix.len() + 1);
        assert!(a.windows(2).all(|w| w[0] != w[1]), "every token moves the state");
        let last = constraint_state_after_v1(&here, prefix.iter().copied()).unwrap();
        assert_eq!(borsh::to_vec(&last).unwrap(), *a.last().unwrap());
        assert!(constraint_is_accepting_v1(&here, &last));
        // A dead prefix has no state, from either spelling.
        assert!(constraint_state_after_v1(&here, [&b"{"[..], b"\"c\""]).is_none());
        assert!(constraint_state_after_v1(&there, [&b"{"[..], b"\"c\""]).is_none());
    }

    /// A malformed automaton that pushes and pops on the same byte forever is cut off, not
    /// looped on — the step is total on any bytes a challenger hands the court.
    #[test]
    fn decode_constraint_step_terminates_on_a_ping_pong_automaton() {
        let mut f = Fb::new();
        let s = f.node(false);
        f.byte(s, b'x', Push { frame: 0, resume: s });
        let mut c = assemble(vec![f.frame()], 0);
        // Sixteen pushes then dead — the depth, not a hang.
        assert!(after(&c, b"x").is_none());
        // Now a pop at the start of the pushed frame: push, pop, push, pop, … cut by the hop cap.
        let popper = f_pop_frame();
        c.frames.push(popper);
        c.frames[0].nodes[0].edges[0].action = Push { frame: 1, resume: 0 };
        c.validate().unwrap();
        assert!(after(&c, b"x").is_none());
        assert!(after(&c, b"y").is_none());
    }

    fn f_pop_frame() -> PalwConstraintFrameV1 {
        let mut f = Fb::new();
        let s = f.node(false);
        f.range(s, 0, 255, Pop);
        f.frame()
    }

    // ---------------------------------------------------------------------------------------
    // The selection rule v3
    // ---------------------------------------------------------------------------------------

    /// A cheap deterministic row generator — the same rows on every host.
    fn rows(seed: u64, count: usize, width: usize, spread: i32) -> Vec<Vec<i32>> {
        let mut x = seed | 1;
        let mut next = move || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        (0..count).map(|_| (0..width).map(|_| ((next() % (2 * spread as u64 + 1)) as i64 - spread as i64) as i32).collect()).collect()
    }

    /// **Invariant 1.** With every lane admitted, v3 IS v2 on every row — random rows, all-tie
    /// rows, extreme rows, at every temperature and seed swept — and at `T_q = 0` therefore v1.
    #[test]
    fn decode_token_select_v3_is_v2_when_every_lane_is_admitted() {
        let seeds = [PALW_DECODE_SEED_GREEDY, [7u8; 32], [0xA5u8; 32]];
        let temperatures = [PALW_DECODE_TEMPERATURE_GREEDY, 1, 1 << 20, 1 << 24, 1 << 28, u32::MAX];
        for width in [1usize, 2, 3, 17, 64, 129, 1024] {
            for spread in [1i32, 3, 1_000, i32::MAX / 2] {
                for row in rows(0x0096_0007 ^ width as u64, 16, width, spread) {
                    for seed in &seeds {
                        for position in [0u32, 1, 4_095] {
                            for t in temperatures {
                                let v2 = decode_token_select_v2(&row, seed, position, t);
                                assert_eq!(decode_token_select_v3(&row, seed, position, t, |_| true), Some(v2));
                                if t == PALW_DECODE_TEMPERATURE_GREEDY {
                                    assert_eq!(v2, base0_decode_token_select_v1(&row));
                                }
                            }
                        }
                    }
                }
            }
        }
        for row in [vec![i32::MIN; 5], vec![i32::MAX; 5], vec![0; 5], vec![i32::MIN, i32::MAX, i32::MIN]] {
            assert_eq!(
                decode_token_select_v3(&row, &[3u8; 32], 9, 1 << 26, |_| true),
                Some(decode_token_select_v2(&row, &[3u8; 32], 9, 1 << 26))
            );
            assert_eq!(
                decode_token_select_v3(&row, &PALW_DECODE_SEED_GREEDY, 0, 0, |_| true),
                Some(base0_decode_token_select_v1(&row))
            );
        }
        assert_eq!(decode_token_select_v3(&[], &PALW_DECODE_SEED_GREEDY, 0, 0, |_| true), None, "an empty row has no lane");
    }

    /// Mask, then key: the answer is the keyed argmax OVER THE ADMITTED LANES with ties to the
    /// lowest admitted index; a masked-out lane never wins however large its logit; and no
    /// admitted lane is `None` — the signal to commit the lowest EOG id and stop.
    #[test]
    fn decode_token_select_v3_picks_the_admitted_argmax_and_none_when_nothing_is() {
        let row = vec![5, 100, 100, 7, 100, -3, 99];
        let greedy = |admitted: &dyn Fn(usize) -> bool| decode_token_select_v3(&row, &PALW_DECODE_SEED_GREEDY, 0, 0, admitted);
        assert_eq!(greedy(&|_| true), Some(1));
        assert_eq!(greedy(&|j| j != 1), Some(2), "the tie goes to the lowest ADMITTED index");
        assert_eq!(greedy(&|j| j > 2), Some(4));
        assert_eq!(greedy(&|j| j == 5 || j == 0), Some(0));
        assert_eq!(greedy(&|j| j == 5), Some(5), "the only admitted lane wins with the row's lowest logit");
        assert_eq!(greedy(&|_| false), None);
        // Under a seed and a temperature the same holds over the KEYS: the admitted winner is the
        // admitted lane whose key no other admitted lane beats.
        let sampling = PalwDecodeSamplingV2 { seed: [0x2Bu8; 32], temperature_q: 1 << 28 };
        for row in rows(0xBEEF, 20, 40, 500) {
            let mask: Vec<bool> = (0..40).map(|j| j % 3 != 1).collect();
            let winner = decode_token_select_v3(&row, &sampling.seed, 3, sampling.temperature_q, |j| mask[j]).unwrap();
            assert!(mask[winner]);
            let wk = sampling.lane_key(row[winner], 3, winner);
            for (j, v) in row.iter().enumerate() {
                if mask[j] {
                    assert!(!decode_lane_beats_v2(sampling.lane_key(*v, 3, j), j, wk, winner), "lane {j} beats the winner");
                }
            }
        }
    }

    // ---------------------------------------------------------------------------------------
    // The refutation v3 — the court's two new arms, over the tiled pin
    // ---------------------------------------------------------------------------------------

    const VOCAB: usize = 10_000;
    const EOG_LOW: u32 = 9_998;
    const EOG_HIGH: u32 = 9_999;
    const UNRENDERABLE: u32 = 7_000;
    const BRACKET_SEVEN: u32 = 300;
    const SEVEN_BRACKET: u32 = 301;

    /// A synthetic class table over the fixture's 10,000-lane vocabulary: the 256 single bytes,
    /// two multi-byte tokens, the two end-of-generation tokens, and a padded remainder the table
    /// cannot render.
    fn class_table(id: u32) -> Option<Vec<u8>> {
        match id {
            0..=255 => Some(vec![id as u8]),
            BRACKET_SEVEN => Some(b"[7".to_vec()),
            SEVEN_BRACKET => Some(b"7]".to_vec()),
            // Special tokens render EMPTY in the table (ADR-0096 §10 B4): never answer content.
            EOG_LOW | EOG_HIGH => Some(Vec::new()),
            _ => None,
        }
    }

    const EOG_IDS: [u32; 2] = [EOG_HIGH, EOG_LOW];

    /// A token's rendering as the table gives it: empty for a special or an unrenderable id.
    fn segment(id: u32) -> Vec<u8> {
        class_table(id).unwrap_or_default()
    }

    fn segments(ids: &[u32]) -> Vec<Vec<u8>> {
        ids.iter().map(|id| segment(*id)).collect()
    }

    /// The court as the close arm feeds it: the claim's renderings of the pin's ids, and the
    /// beating lane's rendering (what the table opening would prove).
    fn try_v3(
        binding: &crate::palw_step_leg::PalwStepBindingV2,
        pin: &crate::palw_step_refute::PalwTiledDecodePinV1,
        sampling: PalwDecodeSamplingV2,
        c: &PalwDecodeConstraintV1,
    ) -> Result<crate::palw_step_leg::PalwStepRefutationVerdictV1, crate::palw_step_refute::PalwStepRefuteError> {
        let segs = segments(&pin.generated_token_ids);
        let beat = segment(pin.beat_lane);
        let court =
            PalwDecodeConstraintCourtV1 { constraint: c, segments: &segs, beat_lane_bytes: Some(&beat), eog_token_ids: &EOG_IDS };
        check_tiled_decode_token_refutation_v3(binding, pin, sampling, Some(court), cap())
    }

    /// Rows whose raw argmax is a lane the constraint FORBIDS (`x`, so v2 and v3 disagree at every
    /// position), with a secondary structure among the admitted lanes: `[` above `[7`, the digits
    /// ordered `3 > 8 > 1 > …`, `]` above everything else that is live.
    fn constrained_rows(decode: usize) -> Vec<Vec<i32>> {
        let base = rows(0xC0FFEE, decode, VOCAB, 1_000);
        base.into_iter()
            .map(|mut row| {
                row[b'x' as usize] = 1_000_000;
                row[b'[' as usize] = 500_000;
                row[BRACKET_SEVEN as usize] = 400_000;
                row[b'3' as usize] = 300_000;
                row[b'8' as usize] = 200_000;
                row[b'1' as usize] = 100_000;
                row[b']' as usize] = 50_000;
                row[SEVEN_BRACKET as usize] = 40_000;
                row
            })
            .collect()
    }

    /// The honest producer under §10 B7: at every position the admitted keyed argmax while some
    /// byte continues; from the first position where none does, the lowest EOG id — to the end of
    /// the budget, one id per row.
    fn honest_run(c: &PalwDecodeConstraintV1, rows: &[Vec<i32>], sampling: PalwDecodeSamplingV2) -> Vec<u32> {
        let mut ids = Vec::new();
        let mut cursor = constraint_cursor_start_v1(c);
        for (p, row) in rows.iter().enumerate() {
            let next = match &cursor {
                PalwConstraintCursorV1::Running(state) if constraint_admits_any_byte_v1(c, state) => {
                    let mask = constraint_admitted_lanes_v1(c, state, VOCAB as u32, &class_table);
                    decode_token_select_v3(row, &sampling.seed, p as u32, sampling.temperature_q, |j| mask[j])
                        .expect("a byte-complete table admits a lane wherever a byte continues") as u32
                }
                _ => EOG_LOW,
            };
            cursor =
                constraint_cursor_advance_v1(c, &cursor, &segment(next), next == EOG_LOW).expect("the honest run follows the rule");
            ids.push(next);
        }
        ids
    }

    /// The fixture's binding, re-committed with the given rows and ids under the tiled scheme at
    /// the synthetic vocabulary and decode count.
    fn constrained_binding(rows: &[Vec<i32>], ids: &[u32]) -> crate::palw_step_leg::PalwStepBindingV2 {
        let (mut binding, _m, _r, _) = crate::palw_step_refute::tests::base0_honest_decode_commitment();
        binding.shape_profile.vocab_size = VOCAB as u32;
        binding.shape_profile.logits_scheme_id = tiled_logits_scheme_id_v1();
        binding.job_context.exact_decode_tokens = ids.len() as u32;
        let rows = &rows[..ids.len()];
        binding.full_logits_trace_root = tiled_logits_trace_root_v1(&binding.job_context, rows, ids).expect("a tree");
        crate::palw_step_refute::tests::rebind_committed_root(&mut binding);
        binding
    }

    /// A challenger's two-tile pin for one position — the same assembly the decode close's own
    /// tests use, from the full rows.
    fn tiled_pin(
        ctx: &crate::palw_v2::PalwJobContextV2,
        rows: &[Vec<i32>],
        ids: &[u32],
        position: u32,
        beat_lane: u32,
    ) -> PalwTiledDecodePinV1 {
        let ctx_hash = ctx.context_hash();
        let row = &rows[position as usize];
        let tiles: Vec<Vec<i32>> = row.chunks(PALW_LOGITS_TILE_LANES).map(<[i32]>::to_vec).collect();
        let tile_leaves: Vec<Hash64> =
            tiles.iter().enumerate().map(|(t, lanes)| tiled_logits_tile_leaf_v1(&ctx_hash, position, t as u32, lanes)).collect();
        let path_for = |leaves: &[Hash64], index: usize| -> Vec<Hash64> {
            crate::palw_step_leg::step_merkle_path_v1(leaves, index).expect("the test's trees are inside the leg bounds")
        };
        let row_roots: Vec<Hash64> = rows[..ids.len()]
            .iter()
            .enumerate()
            .map(|(r, lanes)| tiled_logits_row_root_v1(&ctx_hash, r as u32, lanes).expect("the fixture's rows have lanes"))
            .collect();
        let committed = ids[position as usize] as usize;
        let (ct, bt) = (committed / PALW_LOGITS_TILE_LANES, beat_lane as usize / PALW_LOGITS_TILE_LANES);
        PalwTiledDecodePinV1 {
            position,
            generated_token_ids: ids.to_vec(),
            row_root: row_roots[position as usize],
            row_opening: PalwStepOpeningV1 {
                leaf_index: position as u64,
                leaf_hash: row_roots[position as usize],
                siblings: path_for(&row_roots, position as usize),
            },
            committed_tile_lanes: tiles[ct].clone(),
            committed_opening: PalwStepOpeningV1 {
                leaf_index: ct as u64,
                leaf_hash: tile_leaves[ct],
                siblings: path_for(&tile_leaves, ct),
            },
            beat_tile_lanes: tiles[bt].clone(),
            beat_opening: PalwStepOpeningV1 {
                leaf_index: bt as u64,
                leaf_hash: tile_leaves[bt],
                siblings: path_for(&tile_leaves, bt),
            },
            beat_lane,
        }
    }

    /// The one-disclosure form: the ids and the row opening, NO tile — what the third arm needs
    /// and nothing more. A v2 court refuses this pin as malformed (an empty tile is not the
    /// scheme's width), so the wire form is unchanged and the arm is unreachable before the fence.
    fn one_disclosure_pin(
        ctx: &crate::palw_v2::PalwJobContextV2,
        rows: &[Vec<i32>],
        ids: &[u32],
        position: u32,
    ) -> PalwTiledDecodePinV1 {
        let mut pin = tiled_pin(ctx, rows, ids, position, 0);
        let empty = PalwStepOpeningV1 { leaf_index: 0, leaf_hash: Hash64::default(), siblings: Vec::new() };
        pin.committed_tile_lanes.clear();
        pin.beat_tile_lanes.clear();
        pin.committed_opening = empty.clone();
        pin.beat_opening = empty;
        pin.beat_lane = 0;
        pin
    }

    fn cap() -> u64 {
        crate::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES
    }

    fn fault_at(position: u32) -> crate::palw_step_leg::PalwStepFaultV1 {
        crate::palw_step_leg::PalwStepFaultV1::DecodeTokenMismatch { position }
    }

    /// **Without a constraint the v3 arm IS the v2 arm, verdict for verdict** — honest and lying
    /// runs, greedy and seeded, every pin the v2 tests carry, and the one-disclosure shape (which
    /// v2 refuses as malformed, and so must v3 when nothing is bound).
    #[test]
    fn refutation_v3_without_a_constraint_is_the_v2_verdict_byte_for_byte() {
        let rows = constrained_rows(4);
        let greedy = PalwDecodeSamplingV2::GREEDY;
        let hot = PalwDecodeSamplingV2 { seed: [0x2Bu8; 32], temperature_q: 1 << 28 };
        let mut compared = 0usize;
        for sampling in [greedy, hot] {
            let honest: Vec<u32> = rows.iter().enumerate().map(|(p, r)| sampling.select(r, p as u32) as u32).collect();
            let mut lying = honest.clone();
            lying[2] = lying[2].wrapping_add(11) % VOCAB as u32;
            for ids in [&honest, &lying] {
                let binding = constrained_binding(&rows, ids);
                for p in 0..ids.len() as u32 {
                    for beat in [0u32, b'x' as u32, honest[p as usize], VOCAB as u32 - 1, ids[p as usize]] {
                        let pin = tiled_pin(&binding.job_context, &rows, ids, p, beat);
                        let v2 = check_tiled_decode_token_refutation_capped_v2(&binding, &pin, sampling, cap());
                        let v3 = check_tiled_decode_token_refutation_v3(&binding, &pin, sampling, None, cap());
                        assert_eq!(v3, v2, "position {p}, beat {beat}");
                        compared += 1;
                    }
                    let bare = one_disclosure_pin(&binding.job_context, &rows, ids, p);
                    let v2 = check_tiled_decode_token_refutation_capped_v2(&binding, &bare, sampling, cap());
                    assert!(matches!(v2, Err(PalwStepRefuteError::InputSetNotCanonical(_))));
                    assert_eq!(check_tiled_decode_token_refutation_v3(&binding, &bare, sampling, None, cap()), v2);
                }
            }
        }
        assert!(compared >= 80, "{compared} verdicts compared");
        // And a lie is convicted by both, so the equality above is not a run of refusals.
        let ids: Vec<u32> = rows.iter().map(|r| base0_decode_token_select_v1(r) as u32).collect();
        let mut lying = ids.clone();
        lying[1] = 5;
        let binding = constrained_binding(&rows, &lying);
        let pin = tiled_pin(&binding.job_context, &rows, &lying, 1, ids[1]);
        let verdict = check_tiled_decode_token_refutation_v3(&binding, &pin, greedy, None, cap()).expect("convicted");
        assert_eq!(verdict.fault, fault_at(1));
        assert_eq!(Ok(verdict), check_tiled_decode_token_refutation_capped_v2(&binding, &pin, greedy, cap()));
    }

    /// **Invariant 2.** An honest constrained run clears under both arms at every position —
    /// including the stop, where the committed lowest EOG id is not admitted and is not a fault —
    /// and altering one committed id to a token the constraint forbids is convicted by the
    /// one-disclosure arm, tile or no tile.
    #[test]
    fn refutation_v3_clears_an_honest_constrained_run_and_convicts_the_altered_id() {
        let c = bracket_digit();
        let rows = constrained_rows(6);
        for sampling in [PalwDecodeSamplingV2::GREEDY, PalwDecodeSamplingV2 { seed: [0x77u8; 32], temperature_q: 1 << 28 }] {
            let ids = honest_run(&c, &rows, sampling);
            // `[`, a digit, `]`, then the stop: the lowest EOG id from the fourth position to the
            // end of the budget (§10 B7).
            assert_eq!(ids.len(), 6, "{ids:?}");
            assert!(ids[3..].iter().all(|id| *id == EOG_LOW), "{ids:?}");
            assert_eq!(ids[0], b'[' as u32);
            assert!(class_table(ids[1]).unwrap().iter().all(u8::is_ascii_digit));
            assert_eq!(ids[2], b']' as u32);
            assert_eq!(ids[3], EOG_LOW);
            let binding = constrained_binding(&rows, &ids);
            for p in 0..6u32 {
                let bare = one_disclosure_pin(&binding.job_context, &rows, &ids, p);
                assert!(
                    matches!(try_v3(&binding, &bare, sampling, &c), Err(PalwStepRefuteError::NoFaultFound)),
                    "honest position {p} under the one-disclosure arm"
                );
                for beat in [0u32, b'x' as u32, b'[' as u32, b'3' as u32, b']' as u32, BRACKET_SEVEN, EOG_HIGH, VOCAB as u32 - 1] {
                    if beat == ids[p as usize] {
                        continue;
                    }
                    let pin = tiled_pin(&binding.job_context, &rows, &ids, p, beat);
                    assert!(
                        matches!(try_v3(&binding, &pin, sampling, &c), Err(PalwStepRefuteError::NoFaultFound)),
                        "honest position {p} against lane {beat}"
                    );
                }
            }

            // The negative control: one id altered to a token the constraint does not admit at
            // its position — `x` where a digit was due. Convicted from the ids alone.
            let mut altered = ids.clone();
            altered[1] = b'x' as u32;
            let binding = constrained_binding(&rows, &altered);
            let bare = one_disclosure_pin(&binding.job_context, &rows, &altered, 1);
            let verdict = try_v3(&binding, &bare, sampling, &c).expect("convicted");
            assert_eq!(verdict.fault, fault_at(1));
            // With tiles supplied the same arm convicts first — no tile is read for it.
            let pin = tiled_pin(&binding.job_context, &rows, &altered, 1, 0);
            assert_eq!(try_v3(&binding, &pin, sampling, &c), Ok(verdict.clone()));
            // A position AFTER the altered one is not adjudicated from a dead state: the court
            // names the earlier fault instead.
            let later = one_disclosure_pin(&binding.job_context, &rows, &altered, 2);
            assert!(matches!(try_v3(&binding, &later, sampling, &c), Err(PalwStepRefuteError::InputSetNotCanonical(_))));
            // An unrenderable id is not admitted either — and is convicted where it sits.
            let mut padded = ids.clone();
            padded[1] = UNRENDERABLE;
            let binding = constrained_binding(&rows, &padded);
            let bare = one_disclosure_pin(&binding.job_context, &rows, &padded, 1);
            assert_eq!(try_v3(&binding, &bare, sampling, &c).map(|v| v.fault), Ok(fault_at(1)));
            let later = one_disclosure_pin(&binding.job_context, &rows, &padded, 2);
            assert!(matches!(try_v3(&binding, &later, sampling, &c), Err(PalwStepRefuteError::InputSetNotCanonical(_))));
        }
    }

    /// **Invariant 3.** Under a constraint the two-disclosure arm convicts a producer that
    /// committed an admitted lane when an admitted lane with a strictly greater key existed, and
    /// refuses to convict when the beating lane is one the constraint forbids — however large its
    /// logit. The same pin under v2 (no constraint bound) convicts, which is why the constraint
    /// must come from the claim and never from the challenger.
    #[test]
    fn refutation_v3_does_not_convict_on_a_lane_the_constraint_forbids() {
        let c = bracket_digit();
        let rows = constrained_rows(6);
        for sampling in [PalwDecodeSamplingV2::GREEDY, PalwDecodeSamplingV2 { seed: [0x5Au8; 32], temperature_q: 1 << 27 }] {
            let honest = honest_run(&c, &rows, sampling);
            // A producer that committed the second-best ADMITTED digit at position 1.
            let state = after(&c, b"[").unwrap();
            let mask = constraint_admitted_lanes_v1(&c, &state, VOCAB as u32, &class_table);
            let best = honest[1] as usize;
            let second =
                decode_token_select_v3(&rows[1], &sampling.seed, 1, sampling.temperature_q, |j| mask[j] && j != best).unwrap();
            let mut cheated = honest.clone();
            cheated[1] = second as u32;
            let binding = constrained_binding(&rows, &cheated);
            // The one-disclosure arm finds nothing: the token IS admitted.
            let bare = one_disclosure_pin(&binding.job_context, &rows, &cheated, 1);
            assert!(matches!(try_v3(&binding, &bare, sampling, &c), Err(PalwStepRefuteError::NoFaultFound)));
            // The admitted argmax beats it: convicted.
            let pin = tiled_pin(&binding.job_context, &rows, &cheated, 1, best as u32);
            let verdict = try_v3(&binding, &pin, sampling, &c).expect("convicted");
            assert_eq!(verdict.fault, fault_at(1));
            // `x` has the row's greatest key and the constraint forbids it: NOT a fault under the
            // claim's constraint — and a conviction under v2, which knows no constraint.
            let forbidden = tiled_pin(&binding.job_context, &rows, &cheated, 1, b'x' as u32);
            assert!(matches!(try_v3(&binding, &forbidden, sampling, &c), Err(PalwStepRefuteError::NoFaultFound)));
            assert!(check_tiled_decode_token_refutation_capped_v2(&binding, &forbidden, sampling, cap()).is_ok());
            // An unrenderable beating lane, and the EOG token as a beating lane, are forbidden too.
            for beat in [UNRENDERABLE, EOG_LOW, EOG_HIGH, BRACKET_SEVEN] {
                let pin = tiled_pin(&binding.job_context, &rows, &cheated, 1, beat);
                assert!(matches!(try_v3(&binding, &pin, sampling, &c), Err(PalwStepRefuteError::NoFaultFound)));
            }
            // A beating lane that is admitted but does not beat: no fault.
            let weaker = tiled_pin(&binding.job_context, &rows, &honest, 1, b'1' as u32);
            let binding = constrained_binding(&rows, &honest);
            assert!(matches!(try_v3(&binding, &weaker, sampling, &c), Err(PalwStepRefuteError::NoFaultFound)));
        }
    }

    /// **Decision 7's stop rule, tried.** Where no lane is admitted the committed token must be
    /// the LOWEST EOG id: an honest stop clears; a garbage token there is a fault; the higher EOG
    /// id there is a fault; and the arm needs no tile for any of it. Where a lane IS admitted the
    /// EOG id is a fault like any other forbidden token.
    #[test]
    fn refutation_v3_tries_the_lowest_eog_rule_where_nothing_is_admitted() {
        let c = bracket_digit();
        let rows = constrained_rows(6);
        let sampling = PalwDecodeSamplingV2::GREEDY;
        let honest = honest_run(&c, &rows, sampling);
        assert_eq!(honest[3], EOG_LOW);
        let try_last = |ids: &[u32]| {
            let binding = constrained_binding(&rows, ids);
            let bare = one_disclosure_pin(&binding.job_context, &rows, ids, 3);
            try_v3(&binding, &bare, sampling, &c)
        };
        assert!(matches!(try_last(&honest), Err(PalwStepRefuteError::NoFaultFound)));
        let mut garbage = honest.clone();
        garbage[3] = b'A' as u32;
        assert_eq!(try_last(&garbage).map(|v| v.fault), Ok(fault_at(3)));
        let mut wrong_eog = honest.clone();
        wrong_eog[3] = EOG_HIGH;
        assert_eq!(try_last(&wrong_eog).map(|v| v.fault), Ok(fault_at(3)));
        let mut padded = honest.clone();
        padded[3] = UNRENDERABLE;
        assert_eq!(try_last(&padded).map(|v| v.fault), Ok(fault_at(3)));
        // The EOG id where a digit was due: forbidden, convicted.
        let mut early = honest.clone();
        early[1] = EOG_LOW;
        let binding = constrained_binding(&rows, &early);
        let bare = one_disclosure_pin(&binding.job_context, &rows, &early, 1);
        assert_eq!(try_v3(&binding, &bare, sampling, &c).map(|v| v.fault), Ok(fault_at(1)));
        // A multi-byte token: `[7` at position 0 is admitted (and wins when `[` is masked away by
        // a rows edit), after which only `]` is live and the stop fires one position earlier.
        let mut rows2 = rows.clone();
        rows2[0][b'[' as usize] = 0;
        let ids = honest_run(&c, &rows2, sampling);
        assert_eq!(ids, vec![BRACKET_SEVEN, b']' as u32, EOG_LOW, EOG_LOW, EOG_LOW, EOG_LOW]);
        let binding = constrained_binding(&rows2, &ids);
        for p in 0..6u32 {
            let bare = one_disclosure_pin(&binding.job_context, &rows2, &ids, p);
            assert!(matches!(try_v3(&binding, &bare, sampling, &c), Err(PalwStepRefuteError::NoFaultFound)));
        }
        // And a class with no EOG id at all cannot try the stop: malformed binding, no verdict.
        let segs = segments(&ids);
        let no_eog = PalwDecodeConstraintCourtV1 { constraint: &c, segments: &segs, beat_lane_bytes: None, eog_token_ids: &[] };
        let bare = one_disclosure_pin(&binding.job_context, &rows2, &ids, 2);
        assert!(matches!(
            check_tiled_decode_token_refutation_v3(&binding, &bare, sampling, Some(no_eog), cap()),
            Err(PalwStepRefuteError::InputSetNotCanonical(_))
        ));
    }

    /// Bent evidence under a constraint is refused as evidence, never adjudicated: a bent tile, a
    /// row opening that does not walk, an id count that is not the run's, a position past the run.
    #[test]
    fn refutation_v3_refuses_bent_material_before_it_reads_the_constraint() {
        let c = bracket_digit();
        let rows = constrained_rows(6);
        let sampling = PalwDecodeSamplingV2::GREEDY;
        let ids = honest_run(&c, &rows, sampling);
        let binding = constrained_binding(&rows, &ids);
        let refused = |pin: &PalwTiledDecodePinV1| {
            matches!(try_v3(&binding, pin, sampling, &c), Err(PalwStepRefuteError::InputSetNotCanonical(_)))
        };
        let mut bent = tiled_pin(&binding.job_context, &rows, &ids, 1, b'3' as u32);
        bent.beat_tile_lanes[0] = bent.beat_tile_lanes[0].wrapping_add(1);
        assert!(refused(&bent));
        let mut short = one_disclosure_pin(&binding.job_context, &rows, &ids, 1);
        short.generated_token_ids.pop();
        assert!(refused(&short));
        let mut past = one_disclosure_pin(&binding.job_context, &rows, &ids, 1);
        past.position = 9;
        assert!(refused(&past));
        let mut moved = one_disclosure_pin(&binding.job_context, &rows, &ids, 1);
        moved.row_opening.leaf_index = 2;
        assert!(refused(&moved));
        let mut half = tiled_pin(&binding.job_context, &rows, &ids, 1, b'3' as u32);
        half.beat_tile_lanes.clear();
        assert!(refused(&half), "one tile is neither arm's shape");
        // A wrong constraint (another automaton than the claim's) is the CALLER's error to avoid;
        // bound to a run it does not describe, it reads the run as forbidden at position 0.
        let other = object_ab();
        let bare = one_disclosure_pin(&binding.job_context, &rows, &ids, 0);
        assert_eq!(try_v3(&binding, &bare, sampling, &other).map(|v| v.fault), Ok(fault_at(0)));
    }

    // ---------------------------------------------------------------------------------------
    // §4's producer cost
    // ---------------------------------------------------------------------------------------

    /// **ADR-0096 §4, measured**: `admit` over a synthetic 151,936-entry byte table (tokens of
    /// 1–8 bytes, mostly printable) at the string frame's text node — the producer's heaviest
    /// state, where most of the vocabulary is admitted and every byte of every token is read.
    /// Prints the wall time of the bare automaton loop and of the `&dyn Fn` table path the court
    /// uses; it asserts nothing about time (a limit is not a verdict).
    #[test]
    fn decode_constraint_admit_cost_over_a_qwen_sized_table() {
        const VOCAB_QWEN: u32 = 151_936;
        let (c, text) = json_string();
        let state = PalwConstraintStateV1 { frame: 0, node: text, seen: 0, stack: Vec::new() };
        let mut x = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let table: Vec<Vec<u8>> = (0..VOCAB_QWEN)
            .map(|_| {
                let len = 1 + (next() % 8) as usize;
                (0..len)
                    .map(|_| {
                        let r = next();
                        if r % 10 < 9 { 0x20 + (r >> 8) as u8 % 0x5f } else { 0x80 + (r >> 8) as u8 % 0x80 }
                    })
                    .collect()
            })
            .collect();
        let bytes: usize = table.iter().map(Vec::len).sum();
        let started = std::time::Instant::now();
        let admitted = table.iter().filter(|t| constraint_admits_v1(&c, &state, t).is_some()).count();
        let bare = started.elapsed();
        let lookup = |lane: u32| -> Option<Vec<u8>> { table.get(lane as usize).cloned() };
        let started = std::time::Instant::now();
        let mask = constraint_admitted_lanes_v1(&c, &state, VOCAB_QWEN, &lookup);
        let through_table = started.elapsed();
        assert_eq!(mask.iter().filter(|b| **b).count(), admitted);
        assert!(admitted > VOCAB_QWEN as usize / 2, "most of a printable table is admitted in a string: {admitted}");
        println!(
            "ADR-0096 §4 producer cost: admit over {VOCAB_QWEN} lanes ({bytes} bytes, {admitted} admitted) at the string text node: \
             {bare:?} bare automaton loop, {through_table:?} through the `&dyn Fn` table (debug={})",
            cfg!(debug_assertions)
        );
    }
}
