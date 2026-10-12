//! **`TirProgramV2` — program version 2 (RFC-0003 §I.2.3, spec 04b §15).**
//!
//! Version 1 is text-shaped: a step reads `{token, pos}` and ends in a `logits` node. Version 2
//! adds exactly what the generative profiles need, and nothing else:
//!
//! * **input tensors** — `inputs[j]` is read as `Ref::Input(2 + j)`: an `External` tensor (a job
//!   value or an upstream stage's output, bound by the pipeline) with a declared interval, or a
//!   `Random` tensor whose elements are RFC-0003's `R` (the caller supplies them; this crate never
//!   hashes);
//! * **output kinds** — `Logits` (version 1's meaning exactly), `Rows` (the node at every position)
//!   and `Final` (the node at the last position);
//! * **`post` effects** — in a `Rows`/`Final` program, `post` may `StateWrite` a global `Fixed`
//!   state that `pre` does not write (a denoiser's latent update is the step's last act).
//!
//! **The primitive set is unchanged**: a version-2 program declares `PRIM_SET_ID_V1`, and nothing in
//! version 1 moves — a version-1 program decodes, validates and evaluates exactly as before.
//!
//! **How version 2 reuses version 1.** Every version-2 program has a *version-1 view*
//! ([`TirProgramV2::v1_view`]): its inputs become global params appended after the declared ones,
//! its output node is the view's `logits` node, and a `post` `StateWrite` becomes the `Clamp` to
//! the state's range that the `StateWrite` computes, marked committed so it stays a root. Normal
//! form, types, ranges and evaluation run over that view with the unchanged version-1 code; the
//! version-2 layer adds its own rules ([`crate::validate_v2`]) and applies the `post` writes after
//! a step succeeded ([`crate::interp_v2`]).

use borsh::{BorshDeserialize, BorshSerialize};

use crate::error::{TirError, TirErrorKind, TirResult, err};
use crate::prim::Prim;
use crate::program::*;
use crate::types::DType;

/// The only version this module reads.
pub const TIR_PROGRAM_VERSION_V2: u16 = 2;
/// At most this many input tensors (they are `Input(2)` … `Input(17)`).
pub const MAX_INPUTS_V2: usize = 16;
/// The first `Ref::Input` index that names an input tensor (`0` is the token, `1` the position).
pub const FIRST_INPUT_REF_V2: u8 = 2;

/// A random input's value transform (RFC-0003 §I.1.5). Tags: `Uniform 0`, `Normal 1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum RandomDist {
    /// The word itself, an `idx` in `[0, 2^bits − 1]`; `bits` must be the domain's word width.
    Uniform { bits: u8 },
    /// `PALW_GAUSS_Q24_V1[word]`, an `i32` in Q24; the domain's words are 16 bits.
    Normal,
}

/// Where an input tensor's value comes from. Tags: `External 0`, `Random 1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum InputSource {
    /// Bound by the pipeline to a job value or an upstream stage's output; every element lies in
    /// `[lo, hi]` (range analysis reads the interval; a value outside it is refused).
    External { lo: i64, hi: i64 },
    /// RFC-0003's `R` over a registered domain: element `e` at position `p` is
    /// `dist(R(seed, domain, step, position, e))` with `step = p` when `per_step`, else 0.
    Random { domain: u16, dist: RandomDist, per_step: bool },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct InputDecl {
    /// Inputs and params share one name space.
    pub name: String,
    pub dtype: DType,
    pub shape: Vec<u32>,
    pub source: InputSource,
}

/// The step's result. Tags: `Logits 0`, `Rows 1`, `Final 2`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum OutputDecl {
    /// Version 1's `logits` + `logits_scheme_id`: consumed by a decode rule; `post` writes no state.
    Logits { node: u16, scheme_id: [u8; 64] },
    /// The node's value at every position (an encoder's hidden rows).
    Rows { node: u16 },
    /// The node's value at the last position (a latent, an image).
    Final { node: u16 },
}

impl OutputDecl {
    /// The output node, an index into `post`.
    pub const fn node(&self) -> u16 {
        match *self {
            OutputDecl::Logits { node, .. } | OutputDecl::Rows { node } | OutputDecl::Final { node } => node,
        }
    }

    /// `Rows` and `Final` programs run `post` at every position and may write state there.
    pub const fn allows_post_writes(&self) -> bool {
        !matches!(self, OutputDecl::Logits { .. })
    }

    pub const fn name(&self) -> &'static str {
        match self {
            OutputDecl::Logits { .. } => "Logits",
            OutputDecl::Rows { .. } => "Rows",
            OutputDecl::Final { .. } => "Final",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TirProgramV2 {
    pub version: u16,
    /// `PRIM_SET_ID_V1`: version 2 adds no primitive.
    pub prim_set_id: [u8; 64],
    pub token_bound: u32,
    pub history_bound: u32,
    pub inputs: Vec<InputDecl>,
    pub params: Vec<ParamDecl>,
    pub consts: Vec<ConstDecl>,
    pub states: Vec<StateDecl>,
    pub blocks: Vec<Block>,
    pub schedule: Schedule,
    pub output: OutputDecl,
}

/// Which `step` coordinate a random domain draws at (RFC-0003 §I.1.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RandStepRule {
    Zero,
    PerStep,
    Declared,
}

/// **RFC-0003 §I.1.4's domain table, as normal form needs it**: `(id, word bits, step rule)` for
/// every domain a program input may name. Domain 0 (RFC-0001's text sampler) is not a program input.
/// The keys and the hashing belong to the caller (`misaka-palw-gen`); `tests/program_v2.rs` checks
/// that the two tables agree.
pub const RANDOM_INPUT_DOMAINS_V2: [(u16, u8, RandStepRule); 7] = [
    (1, 16, RandStepRule::Zero),
    (2, 16, RandStepRule::PerStep),
    (3, 16, RandStepRule::Zero),
    (4, 16, RandStepRule::PerStep),
    (5, 16, RandStepRule::Zero),
    (6, 16, RandStepRule::PerStep),
    (7, 32, RandStepRule::Declared),
];

/// The ends of `PALW_GAUSS_Q24_V1` (`misaka-palw-gen`): the interval of a `Normal` input.
pub const GAUSS_Q24_V1_MIN: i64 = -72_560_101;
pub const GAUSS_Q24_V1_MAX: i64 = 72_560_101;

/// The registered rules of a random input domain.
pub fn random_input_domain_v2(domain: u16) -> Option<(u8, RandStepRule)> {
    RANDOM_INPUT_DOMAINS_V2.iter().find(|(id, _, _)| *id == domain).map(|(_, b, r)| (*b, *r))
}

impl InputDecl {
    /// The interval every element of the input lies in: `[lo, hi]` for an external input, the word
    /// range for `Uniform`, the table's range for `Normal`.
    pub fn interval(&self) -> (i128, i128) {
        match self.source {
            InputSource::External { lo, hi } => (lo as i128, hi as i128),
            InputSource::Random { dist: RandomDist::Uniform { bits }, .. } => (0, (1i128 << bits) - 1),
            InputSource::Random { dist: RandomDist::Normal, .. } => (GAUSS_Q24_V1_MIN as i128, GAUSS_Q24_V1_MAX as i128),
        }
    }

    pub fn is_external(&self) -> bool {
        matches!(self.source, InputSource::External { .. })
    }
}

impl TirProgramV2 {
    /// The canonical bytes. Borsh is deterministic, so this is the only encoding of `self`.
    pub fn encode(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("encoding into a Vec cannot fail")
    }

    /// Decode bytes that must be the unique encoding of a version-2 program in normal form: within
    /// [`MAX_PROGRAM_BYTES`], a version-2 prefix, strict Borsh, re-encoding byte-identical, and
    /// [`crate::validate_v2::validate_v2`] passing.
    pub fn decode_canonical(bytes: &[u8]) -> TirResult<Self> {
        if bytes.len() > MAX_PROGRAM_BYTES {
            return err(TirErrorKind::Encoding, format!("{} bytes exceed the {MAX_PROGRAM_BYTES}-byte cap", bytes.len()));
        }
        match program_version(bytes) {
            Some(TIR_PROGRAM_VERSION_V2) => {}
            Some(v) => return err(TirErrorKind::NormalForm, format!("version {v} is not {TIR_PROGRAM_VERSION_V2}")),
            None => return err(TirErrorKind::Encoding, "no version"),
        }
        let program: TirProgramV2 = borsh::from_slice(bytes).map_err(|e| TirError::new(TirErrorKind::Encoding, e.to_string()))?;
        if program.encode() != bytes {
            return err(TirErrorKind::Encoding, "re-encoding differs: not the canonical encoding");
        }
        crate::validate_v2::validate_v2(&program)?;
        Ok(program)
    }

    /// The index `j` of `Ref::Input(j)` for input tensor `k`.
    pub fn input_ref(k: usize) -> Ref {
        Ref::Input(FIRST_INPUT_REF_V2 + k as u8)
    }

    /// The block occurrences of one position in execution order (as version 1).
    pub fn occurrences(&self) -> Vec<(u8, Option<u16>)> {
        let mut out = Vec::with_capacity(self.schedule.layers.len() + 2);
        out.push((self.schedule.pre, None));
        for (l, b) in self.schedule.layers.iter().enumerate() {
            out.push((*b, Some(l as u16)));
        }
        out.push((self.schedule.post, None));
        out
    }

    /// The `post` `StateWrite` nodes of a `Rows`/`Final` program: `(node, state)`.
    pub fn post_writes(&self) -> Vec<(u16, u16)> {
        if !self.output.allows_post_writes() {
            return Vec::new();
        }
        let Some(post) = self.blocks.get(self.schedule.post as usize) else { return Vec::new() };
        post.nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| if let Prim::StateWrite { state } = n.prim { Some((i as u16, state)) } else { None })
            .collect()
    }

    /// **The version-1 view** (spec 04b §15.3): inputs appended to the params as global params (so
    /// `Ref::Input(2 + k)` becomes `Ref::Param(|params| + k)`), the output node as `logits`, and each
    /// `post` `StateWrite` of a `Rows`/`Final` program as a committed `Clamp` to its state's range —
    /// the value the `StateWrite` computes. Built only for programs whose version-2 rules have
    /// passed; version-1 normal form then checks the rest.
    pub fn v1_view(&self) -> TirProgramV1 {
        let first = self.params.len() as u16;
        let mut params = self.params.clone();
        params.extend(self.inputs.iter().map(|i| ParamDecl {
            name: i.name.clone(),
            dtype: i.dtype,
            shape: i.shape.clone(),
            per_layer: false,
        }));
        let post = self.schedule.post as usize;
        let post_writes = self.output.allows_post_writes();
        let blocks = self
            .blocks
            .iter()
            .enumerate()
            .map(|(bi, b)| Block {
                name: b.name.clone(),
                carry_in: b.carry_in.clone(),
                carry_out: b.carry_out.clone(),
                nodes: b
                    .nodes
                    .iter()
                    .map(|n| {
                        let inputs = n
                            .inputs
                            .iter()
                            .map(|r| match *r {
                                Ref::Input(j) if j >= FIRST_INPUT_REF_V2 => Ref::Param(first + (j - FIRST_INPUT_REF_V2) as u16),
                                other => other,
                            })
                            .collect();
                        let fixed_range = |s: u16| match self.states.get(s as usize).map(|d| d.kind) {
                            Some(StateKind::Fixed { lo, hi }) => Some((lo, hi)),
                            _ => None,
                        };
                        match n.prim {
                            Prim::StateWrite { state } if bi == post && post_writes => match fixed_range(state) {
                                Some((lo, hi)) => Node { prim: Prim::Clamp { lo, hi }, inputs, out: n.out.clone(), commit: true },
                                None => Node { prim: n.prim.clone(), inputs, out: n.out.clone(), commit: n.commit },
                            },
                            _ => Node { prim: n.prim.clone(), inputs, out: n.out.clone(), commit: n.commit },
                        }
                    })
                    .collect(),
            })
            .collect();
        let (logits, logits_scheme_id) = match self.output {
            OutputDecl::Logits { node, scheme_id } => (node, scheme_id),
            OutputDecl::Rows { node } | OutputDecl::Final { node } => (node, [0u8; 64]),
        };
        TirProgramV1 {
            version: TIR_PROGRAM_VERSION_V1,
            prim_set_id: self.prim_set_id,
            token_bound: self.token_bound,
            history_bound: self.history_bound,
            params,
            consts: self.consts.clone(),
            states: self.states.clone(),
            blocks,
            schedule: self.schedule.clone(),
            logits,
            logits_scheme_id,
        }
    }

    /// **Build a version-2 program from a version-1 one** — the construction path for tools and
    /// tests, which write programs with [`crate::builder`]: each param named in `inputs` (by its
    /// index in `v1.params`) becomes an input tensor with that source, every reference to it
    /// becomes the matching `Ref::Input`, and `output` replaces `logits`. The other params keep
    /// their order. The result is not validated here; `decode_canonical` or `validate_v2` does that.
    pub fn from_v1_lifting_params(v1: &TirProgramV1, inputs: &[(u16, InputSource)], output: OutputDecl) -> TirResult<Self> {
        if inputs.len() > MAX_INPUTS_V2 {
            return err(TirErrorKind::NormalForm, format!("at most {MAX_INPUTS_V2} inputs"));
        }
        let mut new_index: Vec<Option<u16>> = Vec::with_capacity(v1.params.len());
        let mut input_of: Vec<Option<usize>> = vec![None; v1.params.len()];
        for (k, (p, _)) in inputs.iter().enumerate() {
            let slot = input_of
                .get_mut(*p as usize)
                .ok_or_else(|| TirError::new(TirErrorKind::NormalForm, format!("no param {p} to lift")))?;
            if slot.replace(k).is_some() {
                return err(TirErrorKind::NormalForm, format!("param {p} lifted twice"));
            }
        }
        let mut params = Vec::new();
        for (j, d) in v1.params.iter().enumerate() {
            if input_of[j].is_some() {
                new_index.push(None);
            } else {
                new_index.push(Some(params.len() as u16));
                params.push(d.clone());
            }
        }
        let inputs_decl = inputs
            .iter()
            .map(|(p, source)| {
                let d = &v1.params[*p as usize];
                InputDecl { name: d.name.clone(), dtype: d.dtype, shape: d.shape.clone(), source: *source }
            })
            .collect();
        let blocks = v1
            .blocks
            .iter()
            .map(|b| Block {
                name: b.name.clone(),
                carry_in: b.carry_in.clone(),
                carry_out: b.carry_out.clone(),
                nodes: b
                    .nodes
                    .iter()
                    .map(|n| Node {
                        prim: n.prim.clone(),
                        out: n.out.clone(),
                        commit: n.commit,
                        inputs: n
                            .inputs
                            .iter()
                            .map(|r| match *r {
                                Ref::Param(j) => match input_of.get(j as usize).copied().flatten() {
                                    Some(k) => TirProgramV2::input_ref(k),
                                    None => Ref::Param(new_index.get(j as usize).copied().flatten().unwrap_or(j)),
                                },
                                other => other,
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect();
        Ok(TirProgramV2 {
            version: TIR_PROGRAM_VERSION_V2,
            prim_set_id: v1.prim_set_id,
            token_bound: v1.token_bound,
            history_bound: v1.history_bound,
            inputs: inputs_decl,
            params,
            consts: v1.consts.clone(),
            states: v1.states.clone(),
            blocks,
            schedule: v1.schedule.clone(),
            output,
        })
    }
}

/// The version a program's bytes declare: its first two bytes, little-endian.
pub fn program_version(bytes: &[u8]) -> Option<u16> {
    Some(u16::from_le_bytes([*bytes.first()?, *bytes.get(1)?]))
}

/// A decoded program of either version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TirProgram {
    V1(TirProgramV1),
    V2(TirProgramV2),
}

impl TirProgram {
    /// Decode by the version prefix: version 1 through [`TirProgramV1::decode_canonical`] exactly as
    /// before, version 2 through [`TirProgramV2::decode_canonical`]; any other version is refused.
    pub fn decode_canonical(bytes: &[u8]) -> TirResult<Self> {
        match program_version(bytes) {
            Some(TIR_PROGRAM_VERSION_V1) => TirProgramV1::decode_canonical(bytes).map(TirProgram::V1),
            Some(TIR_PROGRAM_VERSION_V2) => TirProgramV2::decode_canonical(bytes).map(TirProgram::V2),
            Some(v) if bytes.len() > MAX_PROGRAM_BYTES => {
                err(TirErrorKind::Encoding, format!("{} bytes exceed the {MAX_PROGRAM_BYTES}-byte cap (version {v})", bytes.len()))
            }
            Some(v) => err(TirErrorKind::NormalForm, format!("version {v} is neither 1 nor 2")),
            None => err(TirErrorKind::Encoding, "no version"),
        }
    }
}
