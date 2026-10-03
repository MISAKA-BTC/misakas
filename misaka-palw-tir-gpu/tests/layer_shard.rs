//! **RFC-0006 §9 — a layer shard verifies its cell from committed boundary rows and its own weights.**
//!
//! An off-chain prototype; no consensus code is involved. It does five things:
//!
//! 1. **Produce.** A small PALW-TIR program runs once as a producer would, and every commit point of every
//!    occurrence at every position is committed as a leaf, with `Fixed` state checkpoints every `C`
//!    positions (the step space of RFC-0002 Phase F, §2.5). The leaves become a Merkle tree. The producer
//!    is cross-checked against the reference evaluator's own run (`Interpreter::run`).
//! 2. **Verify a cell.** A seat verifies the cell of occurrences `[a, b)` over positions `[p, q)` holding
//!    ONLY the claim's root and the params of its occurrences. Its params source refuses everything
//!    else, so a read outside the shard would fail the check. It opens every input it uses against the
//!    root:
//!    - the carry-in of its first occurrence at each position;
//!    - its layers' history rows before `p`;
//!    - their `Fixed` checkpoints at `p`.
//!
//!    It then recomputes every commit point of the cell with the reference evaluator's cone evaluation
//!    (`Interpreter::eval_cone` — the court's own function), position by position, and checks each
//!    recomputed leaf's hash against the root through the committed path. No producer value is used as an
//!    output.
//! 3. **Accept the honest trace,** cell by cell, on a four-layer Qwen2-shaped decoder (attention over a
//!    history) and on the golden vector's GDN program (per-layer `Fixed` state, so checkpoints and segment
//!    starts are exercised).
//! 4. **Find a tampered leaf** inside a cell, at that leaf; and show the detection lemma's first
//!    consequence. A CONSISTENT lie — a boundary row, or a state, altered with every later leaf computed
//!    honestly from it — passes the downstream cell and is found by the upstream cell that computes it.
//! 5. **Report** the fraction of the weights and of the committed trace a cell touched.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::PathBuf;

use misaka_palw_tir::program::{StateKind, TirProgramV1};
use misaka_palw_tir::{ConeEnv, Interpreter, MapParams, ParamSource, Prim, Ref, RunState, Tensor, TirError};
use misaka_palw_tir_gpu::fixtures::{Geo, map_params, qwen2_program};
use serde_json::Value;

// ---------------------------------------------------------------- the committed trace

/// Where a leaf sits: a commit point's value at a position (its node slot, spec 04b §3.3), or a
/// `Fixed` state instance's value after a position (a checkpoint).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Coord {
    Commit { pos: u32, slot: u32 },
    Checkpoint { pos: u32, state: u16, layer: Option<u16> },
}

fn hash(parts: &[&[u8]]) -> [u8; 32] {
    let mut st = blake2b_simd::Params::new().hash_length(32).to_state();
    for p in parts {
        st.update(p);
    }
    let mut o = [0u8; 32];
    o.copy_from_slice(st.finalize().as_bytes());
    o
}

fn leaf_hash(c: &Coord, v: &Tensor) -> [u8; 32] {
    let mut b = Vec::new();
    match c {
        Coord::Commit { pos, slot } => {
            b.push(0);
            b.extend(pos.to_le_bytes());
            b.extend(slot.to_le_bytes());
        }
        Coord::Checkpoint { pos, state, layer } => {
            b.push(1);
            b.extend(pos.to_le_bytes());
            b.extend(state.to_le_bytes());
            b.extend(layer.map_or(u32::MAX, u32::from).to_le_bytes());
        }
    }
    b.extend(v.dtype.name().as_bytes());
    for d in &v.shape {
        b.extend((*d as u64).to_le_bytes());
    }
    for x in &v.data {
        b.extend(x.to_le_bytes());
    }
    hash(&[b"rfc6-prototype/leaf", &b])
}

fn node_hash(l: &[u8; 32], r: &[u8; 32]) -> [u8; 32] {
    hash(&[b"rfc6-prototype/node", l, r])
}

/// The producer's committed trace: its leaves in enumeration order and their Merkle tree.
struct Trace {
    leaves: Vec<(Coord, Tensor)>,
    index: BTreeMap<Coord, usize>,
    levels: Vec<Vec<[u8; 32]>>,
}

impl Trace {
    fn new(leaves: Vec<(Coord, Tensor)>) -> Self {
        let mut levels = vec![leaves.iter().map(|(c, v)| leaf_hash(c, v)).collect::<Vec<_>>()];
        while levels.last().expect("a level").len() > 1 {
            let prev = levels.last().expect("a level");
            let next = prev.chunks(2).map(|p| node_hash(&p[0], p.get(1).unwrap_or(&p[0]))).collect();
            levels.push(next);
        }
        let index = leaves.iter().enumerate().map(|(i, (c, _))| (*c, i)).collect();
        Trace { leaves, index, levels }
    }
    fn root(&self) -> [u8; 32] {
        self.levels.last().expect("a root")[0]
    }
    /// Leaf `i` with its path: the sibling hashes from the bottom up.
    fn open(&self, i: usize) -> (&(Coord, Tensor), Vec<[u8; 32]>) {
        let mut path = Vec::new();
        let mut k = i;
        for lvl in &self.levels[..self.levels.len() - 1] {
            let sib = if k.is_multiple_of(2) { *lvl.get(k + 1).unwrap_or(&lvl[k]) } else { lvl[k - 1] };
            path.push(sib);
            k /= 2;
        }
        (&self.leaves[i], path)
    }
    /// Committed bytes, as 4-byte lanes (spec 04b §10.1).
    fn bytes(&self) -> usize {
        self.leaves.iter().map(|(_, v)| 4 * v.data.len()).sum()
    }
    /// The same trace with one leaf replaced (a producer that commits a wrong value), re-rooted.
    fn tampered(&self, c: Coord, f: impl Fn(&Tensor) -> Tensor) -> Trace {
        let mut leaves = self.leaves.clone();
        let i = self.index[&c];
        leaves[i].1 = f(&leaves[i].1);
        Trace::new(leaves)
    }
}

fn verify_path(root: [u8; 32], mut i: usize, leaf: [u8; 32], path: &[[u8; 32]]) -> bool {
    let mut acc = leaf;
    for sib in path {
        acc = if i.is_multiple_of(2) { node_hash(&acc, sib) } else { node_hash(sib, &acc) };
        i /= 2;
    }
    acc == root
}

/// What a verifying seat holds of the claim: its ROOT, and a server it reads leaves from. Every input
/// is opened against the root; every output is checked by hashing the seat's own value up the
/// committed path. Every byte is counted.
struct Opened<'t> {
    server: &'t Trace,
    root: [u8; 32],
    read: RefCell<(usize, usize)>,
    checked: RefCell<(usize, usize)>,
}

impl<'t> Opened<'t> {
    fn new(server: &'t Trace) -> Self {
        Opened { server, root: server.root(), read: RefCell::new((0, 0)), checked: RefCell::new((0, 0)) }
    }
    /// A committed input, opened against the root.
    fn input(&self, c: Coord) -> Tensor {
        let i = *self.server.index.get(&c).unwrap_or_else(|| panic!("no committed leaf at {c:?}"));
        let ((cc, v), path) = self.server.open(i);
        assert_eq!(*cc, c);
        assert!(verify_path(self.root, i, leaf_hash(cc, v), &path), "{c:?} does not open under the root");
        let mut r = self.read.borrow_mut();
        r.0 += 1;
        r.1 += 4 * v.data.len();
        v.clone()
    }
    /// Is the seat's own value of leaf `c` the committed one? (Its hash, up the committed path.)
    fn check(&self, c: Coord, mine: &Tensor) -> bool {
        let i = self.server.index[&c];
        let (_, path) = self.server.open(i);
        let mut k = self.checked.borrow_mut();
        k.0 += 1;
        k.1 += 4 * mine.data.len();
        verify_path(self.root, i, leaf_hash(&c, mine), &path)
    }
}

// ---------------------------------------------------------------- params a seat holds

/// The params a shard seat holds: exactly the instances its occurrences reference. Anything else is
/// refused (and recorded), so a cell check that needed another shard's weight would fail.
struct ShardParams<'a> {
    all: &'a MapParams,
    allowed: BTreeSet<(u16, Option<u16>)>,
    served: RefCell<BTreeSet<(u16, Option<u16>)>>,
    refused: RefCell<BTreeSet<(u16, Option<u16>)>>,
}

impl ParamSource for ShardParams<'_> {
    fn param(&self, j: u16, layer: Option<u16>) -> Option<Tensor> {
        if self.allowed.contains(&(j, layer)) {
            self.served.borrow_mut().insert((j, layer));
            self.all.tensors.get(&(j, layer)).cloned()
        } else {
            self.refused.borrow_mut().insert((j, layer));
            None
        }
    }
}

fn param_bytes(all: &MapParams, keys: &BTreeSet<(u16, Option<u16>)>) -> usize {
    keys.iter().map(|k| all.tensors[k].data.len() * all.tensors[k].dtype.width()).sum()
}

// ---------------------------------------------------------------- one occurrence at one position

/// The program walked occurrence by occurrence, every commit point by the reference evaluator's cone
/// evaluation — the same function for the producer and for a verifying seat.
struct Walk<'p> {
    interp: Interpreter<'p>,
    p: &'p TirProgramV1,
    occ: Vec<(u8, Option<u16>)>,
    bases: Vec<u32>,
}

#[derive(Default)]
struct OccOut {
    commits: BTreeMap<u16, Tensor>,
    writes: BTreeMap<u16, Tensor>,
    rows: BTreeMap<u16, Tensor>,
    carry_out: BTreeMap<u8, Tensor>,
}

/// A lie: at `(pos, occurrence, node)`, the value is replaced, and everything after it is computed
/// from the replacement (a consistent lie).
type Lie = (u32, usize, u16, fn(&Tensor) -> Tensor);

impl<'p> Walk<'p> {
    fn new(p: &'p TirProgramV1) -> Self {
        Walk { interp: Interpreter::new(p).expect("a valid program"), p, occ: p.occurrences(), bases: p.occurrence_slot_bases() }
    }

    fn block(&self, o: usize) -> &misaka_palw_tir::Block {
        &self.p.blocks[self.occ[o].0 as usize]
    }

    /// The state instance key of state `j` read or written by occurrence `o`.
    fn inst(&self, j: u16, o: usize) -> (u16, Option<u16>) {
        (j, if self.p.states[j as usize].per_layer { self.occ[o].1 } else { None })
    }

    /// The `Fixed` and `Hist` states occurrence `o` touches.
    fn states_of(&self, o: usize) -> (BTreeSet<u16>, BTreeSet<u16>) {
        let (mut fixed, mut hist) = (BTreeSet::new(), BTreeSet::new());
        for n in &self.block(o).nodes {
            for r in &n.inputs {
                if let Ref::State(j) = r {
                    fixed.insert(*j);
                }
            }
            match n.prim {
                Prim::StateWrite { state } => {
                    fixed.insert(state);
                }
                Prim::HistAppend { state } => {
                    hist.insert(state);
                }
                _ => {}
            }
        }
        (fixed, hist)
    }

    /// Every param instance occurrence `o` references.
    fn params_of(&self, o: usize) -> BTreeSet<(u16, Option<u16>)> {
        let mut out = BTreeSet::new();
        for n in &self.block(o).nodes {
            for r in &n.inputs {
                if let Ref::Param(j) = r {
                    out.insert((*j, if self.p.params[*j as usize].per_layer { self.occ[o].1 } else { None }));
                }
            }
        }
        out
    }

    /// The source of the row `HistAppend` state `j` appends in occurrence `o`: a commit point of `o`,
    /// or a carry-in (the previous occurrence's carry-out).
    fn row_coord(&self, o: usize, j: u16, pos: u32) -> Coord {
        let b = self.block(o);
        let n = b.nodes.iter().find(|n| n.prim == Prim::HistAppend { state: j }).expect("the appender");
        match n.inputs[0] {
            Ref::Node(m) => Coord::Commit { pos, slot: self.bases[o] + m as u32 },
            Ref::CarryIn(k) => Coord::Commit { pos, slot: self.bases[o - 1] + self.block(o - 1).carry_out[k as usize] as u32 },
            ref other => panic!("a history row from {other:?}"),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn run_occurrence(
        &self,
        o: usize,
        pos: u32,
        token: u32,
        params: &dyn ParamSource,
        carry_in: BTreeMap<u8, Tensor>,
        fixed: BTreeMap<u16, Tensor>,
        hist_prior: BTreeMap<u16, Vec<Tensor>>,
        lie: Option<Lie>,
    ) -> Result<OccOut, TirError> {
        let (block, layer) = self.occ[o];
        let b = self.block(o);
        let mut env = ConeEnv { token: Some(token), pos, carry_in: carry_in.clone(), fixed, hist_prior, supplied: BTreeMap::new() };
        let mut out = OccOut::default();
        for (n, node) in b.nodes.iter().enumerate() {
            let write = matches!(node.prim, Prim::StateWrite { .. });
            if !(node.commit || write) {
                continue;
            }
            let mut v = self.interp.eval_cone(block, layer, n as u16, params, &env)?;
            if let Some((lp, lo, ln, f)) = lie
                && (lp, lo, ln) == (pos, o, n as u16)
            {
                v = f(&v);
            }
            if let Prim::StateWrite { state } = node.prim {
                out.writes.insert(state, v.clone());
            }
            if node.commit {
                out.commits.insert(n as u16, v.clone());
            }
            env.supplied.insert(n as u16, v);
        }
        for node in &b.nodes {
            if let Prim::HistAppend { state } = node.prim {
                let row = match node.inputs[0] {
                    Ref::Node(m) => env.supplied[&m].clone(),
                    Ref::CarryIn(k) => carry_in[&k].clone(),
                    ref other => panic!("a history row from {other:?}"),
                };
                out.rows.insert(state, row);
            }
        }
        for (k, c) in b.carry_out.iter().enumerate() {
            out.carry_out.insert(k as u8, env.supplied[c].clone());
        }
        Ok(out)
    }

    fn zeros(&self, j: u16) -> Tensor {
        let s = &self.p.states[j as usize];
        Tensor::zeros(s.dtype, &s.shape.iter().map(|d| *d as usize).collect::<Vec<_>>())
    }

    fn window(&self, j: u16) -> u32 {
        match self.p.states[j as usize].kind {
            StateKind::Hist { window } => window,
            StateKind::Fixed { .. } => unreachable!(),
        }
    }

    // ------------------------------------------------------------ the producer

    /// Run every occurrence at every position and commit the leaves: commit points in slot order, then
    /// every `Fixed` instance after each position `a` with `(a + 1) % c == 0`.
    fn produce(&self, params: &MapParams, tokens: &[u32], c: u32, lie: Option<Lie>) -> Trace {
        let mut leaves = Vec::new();
        let mut fixed: BTreeMap<(u16, Option<u16>), Tensor> = BTreeMap::new();
        let mut hist: BTreeMap<(u16, Option<u16>), Vec<Tensor>> = BTreeMap::new();
        for (pos, tok) in tokens.iter().enumerate() {
            let pos = pos as u32;
            let mut carry = BTreeMap::new();
            let mut writes = Vec::new();
            for o in 0..self.occ.len() {
                let (fx, hs) = self.states_of(o);
                let fenv = fx.iter().map(|j| (*j, fixed.get(&self.inst(*j, o)).cloned().unwrap_or_else(|| self.zeros(*j)))).collect();
                let henv = hs
                    .iter()
                    .map(|j| {
                        let rows = hist.get(&self.inst(*j, o)).cloned().unwrap_or_default();
                        let keep = (pos as usize).min(self.window(*j) as usize - 1);
                        (*j, rows[rows.len() - keep..].to_vec())
                    })
                    .collect();
                let out = self.run_occurrence(o, pos, *tok, params, carry, fenv, henv, lie).expect("the producer's step");
                for (n, v) in &out.commits {
                    leaves.push((Coord::Commit { pos, slot: self.bases[o] + *n as u32 }, v.clone()));
                }
                for (j, v) in out.writes {
                    writes.push((self.inst(j, o), v));
                }
                for (j, row) in out.rows {
                    hist.entry(self.inst(j, o)).or_default().push(row);
                }
                carry = out.carry_out;
            }
            for (k, v) in writes {
                fixed.insert(k, v);
            }
            if (pos + 1).is_multiple_of(c) {
                let mut instances: BTreeSet<(u16, Option<u16>)> = BTreeSet::new();
                for o in 0..self.occ.len() {
                    for j in self.states_of(o).0 {
                        instances.insert(self.inst(j, o));
                    }
                }
                for k in instances {
                    let v = fixed.get(&k).cloned().unwrap_or_else(|| self.zeros(k.0));
                    leaves.push((Coord::Checkpoint { pos, state: k.0, layer: k.1 }, v));
                }
            }
        }
        Trace::new(leaves)
    }

    // ------------------------------------------------------------ a seat checking one cell

    /// Verify the cell of occurrences `occs` over positions `positions` holding only the root and the
    /// cell's params. `Ok` with the cell's footprint, or the first leaf that does not recompute.
    fn verify_cell(
        &self,
        opened: &Opened<'_>,
        params: &ShardParams<'_>,
        occs: Range<usize>,
        positions: Range<u32>,
        tokens: &[u32],
        c: u32,
    ) -> Result<(), Coord> {
        assert!(positions.start.is_multiple_of(c), "a segment starts at a checkpoint");
        let mut my_fixed: BTreeMap<(u16, Option<u16>), Tensor> = BTreeMap::new();
        let mut my_rows: BTreeMap<(u16, Option<u16>), BTreeMap<u32, Tensor>> = BTreeMap::new();
        for pos in positions.clone() {
            // The carry-in of the cell's first occurrence: the previous occurrence's committed carry-out.
            let mut carry: BTreeMap<u8, Tensor> = if occs.start == 0 {
                BTreeMap::new()
            } else {
                let prev = occs.start - 1;
                self.block(prev)
                    .carry_out
                    .iter()
                    .enumerate()
                    .map(|(k, n)| (k as u8, opened.input(Coord::Commit { pos, slot: self.bases[prev] + *n as u32 })))
                    .collect()
            };
            let mut writes = Vec::new();
            for o in occs.clone() {
                let (fx, hs) = self.states_of(o);
                let mut fenv = BTreeMap::new();
                for j in &fx {
                    let k = self.inst(*j, o);
                    assert!(self.p.states[*j as usize].per_layer, "the prototype's cells hold per-layer states only");
                    let v = match my_fixed.get(&k) {
                        Some(v) => v.clone(),
                        None if pos == 0 => self.zeros(*j),
                        None => opened.input(Coord::Checkpoint { pos: pos - 1, state: k.0, layer: k.1 }),
                    };
                    my_fixed.entry(k).or_insert_with(|| v.clone());
                    fenv.insert(*j, v);
                }
                let mut henv = BTreeMap::new();
                for j in &hs {
                    let k = self.inst(*j, o);
                    let keep = (pos as usize).min(self.window(*j) as usize - 1) as u32;
                    let rows = (pos - keep..pos)
                        .map(|r| if r < positions.start { opened.input(self.row_coord(o, *j, r)) } else { my_rows[&k][&r].clone() })
                        .collect();
                    henv.insert(*j, rows);
                }
                let out = self
                    .run_occurrence(o, pos, tokens[pos as usize], params, carry, fenv, henv, None)
                    .unwrap_or_else(|e| panic!("the cell's evaluation failed ({e}); refused params: {:?}", params.refused.borrow()));
                for (n, v) in &out.commits {
                    let coord = Coord::Commit { pos, slot: self.bases[o] + *n as u32 };
                    if !opened.check(coord, v) {
                        return Err(coord);
                    }
                }
                for (j, v) in out.writes {
                    writes.push((self.inst(j, o), v));
                }
                for (j, row) in out.rows {
                    my_rows.entry(self.inst(j, o)).or_default().insert(pos, row);
                }
                carry = out.carry_out;
            }
            for (k, v) in writes {
                my_fixed.insert(k, v);
            }
            if (pos + 1).is_multiple_of(c) {
                for (k, v) in &my_fixed {
                    let coord = Coord::Checkpoint { pos, state: k.0, layer: k.1 };
                    if !opened.check(coord, v) {
                        return Err(coord);
                    }
                }
            }
        }
        Ok(())
    }

    fn shard_params<'a>(&self, all: &'a MapParams, occs: Range<usize>) -> ShardParams<'a> {
        let allowed = occs.flat_map(|o| self.params_of(o)).collect();
        ShardParams { all, allowed, served: RefCell::default(), refused: RefCell::default() }
    }
}

// ---------------------------------------------------------------- fixtures

struct Fixture {
    name: &'static str,
    program: TirProgramV1,
    params: MapParams,
    tokens: Vec<u32>,
    /// The checkpoint interval `C` (segments start at multiples of it).
    c: u32,
    /// The shards as occurrence ranges (`pre` with the first, `post` with the last).
    shards: Vec<Range<usize>>,
    segments: Vec<Range<u32>>,
}

/// A four-layer decoder in `tir-exec-bench`'s conventions at a tiny width.
fn dense() -> Fixture {
    let g = Geo { layers: 4, d: 64, heads: 4, kv: 2, hd: 16, ff: 128, vocab: 256 };
    let (program, fills) = qwen2_program(&g);
    let params = map_params(&program, &fills, 0x5eed);
    let tokens = (0..12u32).map(|i| (i * 37 + 11) % g.vocab).collect();
    Fixture { name: "dense-4layer", program, params, tokens, c: 4, shards: vec![0..3, 3..6], segments: vec![0..4, 4..8, 8..12] }
}

/// Eight layers at a wider width, cut four ways: the weights a shard holds approach a quarter.
fn dense8() -> Fixture {
    let g = Geo { layers: 8, d: 256, heads: 8, kv: 2, hd: 32, ff: 1024, vocab: 512 };
    let (program, fills) = qwen2_program(&g);
    let params = map_params(&program, &fills, 0x8eed);
    let tokens = (0..8u32).map(|i| (i * 53 + 7) % g.vocab).collect();
    Fixture {
        name: "dense-8layer-4shard",
        program,
        params,
        tokens,
        c: 4,
        shards: vec![0..3, 3..5, 5..7, 7..10],
        segments: vec![0..4, 4..8],
    }
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

/// The golden vector's gated-delta program: two layers with per-layer `Fixed` state.
fn gdn() -> Fixture {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../consensus-vectors/tir-v1/programs/gdn-k2-v4-grouped.json");
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(path).expect("the vector")).expect("json");
    let program = TirProgramV1::decode_canonical(&hex(doc["program_borsh_hex"].as_str().unwrap())).expect("a canonical program");
    let mut params = MapParams::default();
    for e in doc["params"].as_array().unwrap() {
        let j = e["param"].as_u64().unwrap() as u16;
        let layer = e["layer"].as_u64().map(|l| l as u16);
        let d = &program.params[j as usize];
        let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
        params.tensors.insert((j, layer), Tensor::from_le_bytes(d.dtype, &shape, &hex(e["le_hex"].as_str().unwrap())).unwrap());
    }
    let tb = program.token_bound;
    let tokens = (0..8u32).map(|i| (i * 7 + 3) % tb).collect();
    let n_occ = program.occurrences().len();
    let shards = vec![0..2, 2..n_occ];
    Fixture { name: "gdn-k2-v4", program, params, tokens, c: 2, shards, segments: vec![0..2, 2..4, 4..6, 6..8] }
}

// ---------------------------------------------------------------- the tests

fn produced(f: &Fixture, walk: &Walk<'_>, lie: Option<Lie>) -> Trace {
    let t = walk.produce(&f.params, &f.tokens, f.c, lie);
    if lie.is_none() {
        // The producer IS the reference evaluator's run, commit point for commit point.
        let run = walk.interp.run(&f.params, &f.tokens).expect("the reference run");
        let mut want = 0usize;
        for step in &run {
            for c in &step.commits {
                let coord = Coord::Commit { pos: step.pos, slot: c.slot };
                assert_eq!(t.leaves[t.index[&coord]].1, c.value, "{}: the producer ≠ the reference at {coord:?}", f.name);
                want += 1;
            }
        }
        let have = t.leaves.iter().filter(|(c, _)| matches!(c, Coord::Commit { .. })).count();
        assert_eq!(have, want, "{}: every commit point of the reference run is a leaf", f.name);
    }
    t
}

fn check_all(f: &Fixture, walk: &Walk<'_>, trace: &Trace) -> BTreeMap<(usize, usize), Result<(), Coord>> {
    let mut out = BTreeMap::new();
    for (si, shard) in f.shards.iter().enumerate() {
        for (pi, seg) in f.segments.iter().enumerate() {
            let opened = Opened::new(trace);
            let params = walk.shard_params(&f.params, shard.clone());
            let r = walk.verify_cell(&opened, &params, shard.clone(), seg.clone(), &f.tokens, f.c);
            assert!(params.refused.borrow().is_empty(), "{}: cell ({si}, {pi}) asked for a param outside its shard", f.name);
            out.insert((si, pi), r);
        }
    }
    out
}

#[test]
fn an_honest_trace_verifies_cell_by_cell_from_boundary_rows_and_only_the_cells_own_weights() {
    for f in [dense(), gdn(), dense8()] {
        let walk = Walk::new(&f.program);
        let trace = produced(&f, &walk, None);
        let all_params: BTreeSet<(u16, Option<u16>)> = f.params.tensors.keys().copied().collect();
        let total_params = param_bytes(&f.params, &all_params);
        eprintln!(
            "{}: {} occurrences, {} positions, {} leaves ({} KiB of lanes), params {} KiB, root {}",
            f.name,
            walk.occ.len(),
            f.tokens.len(),
            trace.leaves.len(),
            trace.bytes() / 1024,
            total_params / 1024,
            trace.root().iter().take(6).map(|b| format!("{b:02x}")).collect::<String>()
        );
        for (si, shard) in f.shards.iter().enumerate() {
            for (pi, seg) in f.segments.iter().enumerate() {
                let opened = Opened::new(&trace);
                let params = walk.shard_params(&f.params, shard.clone());
                let r = walk.verify_cell(&opened, &params, shard.clone(), seg.clone(), &f.tokens, f.c);
                assert_eq!(r, Ok(()), "{}: the honest cell ({si}, {pi}) does not verify", f.name);
                assert!(params.refused.borrow().is_empty());
                let served = param_bytes(&f.params, &params.served.borrow());
                let (rl, rb) = *opened.read.borrow();
                let (cl, cb) = *opened.checked.borrow();
                eprintln!(
                    "  cell (occurrences {shard:?}, positions {seg:?}): weights {:5.1} % ({} of {} instances); trace read {:5.2} % ({rl} leaves), checked {:5.1} % ({cl} leaves)",
                    100.0 * served as f64 / total_params as f64,
                    params.served.borrow().len(),
                    all_params.len(),
                    100.0 * rb as f64 / trace.bytes() as f64,
                    100.0 * cb as f64 / trace.bytes() as f64
                );
            }
        }
        // The cells tile the trace: every leaf is checked by exactly one cell.
        let mut covered = 0usize;
        for shard in &f.shards {
            for seg in &f.segments {
                let opened = Opened::new(&trace);
                let params = walk.shard_params(&f.params, shard.clone());
                walk.verify_cell(&opened, &params, shard.clone(), seg.clone(), &f.tokens, f.c).unwrap();
                covered += opened.checked.borrow().0;
            }
        }
        assert_eq!(covered, trace.leaves.len(), "{}: the cells' checks tile the committed leaves", f.name);
    }
}

#[test]
fn a_tampered_leaf_inside_a_cell_is_found_by_that_cell_at_that_leaf() {
    let f = dense();
    let walk = Walk::new(&f.program);
    let honest = produced(&f, &walk, None);
    // A commit point of layer 2 (occurrence 3, shard 1) at position 5 (segment 1): its first commit point.
    let block = walk.block(3);
    let n = block.nodes.iter().position(|n| n.commit).expect("a commit point") as u32;
    let coord = Coord::Commit { pos: 5, slot: walk.bases[3] + n };
    let lying = honest.tampered(coord, |v| {
        let mut v = v.clone();
        v.data[0] = if v.data[0] == v.dtype.max_value() { v.data[0] - 1 } else { v.data[0] + 1 };
        v
    });
    let results = check_all(&f, &walk, &lying);
    assert_eq!(results[&(1, 1)], Err(coord), "the cell holding the leaf finds it, at it");
    for (cell, r) in &results {
        if *cell != (1, 1) {
            assert_eq!(*r, Ok(()), "cell {cell:?} neither holds nor reads the tampered leaf");
        }
    }
    eprintln!("tampered {coord:?}: found by cell (shard 1, segment 1) at that leaf; the other five cells verify");
}

fn bump(v: &Tensor) -> Tensor {
    let mut v = v.clone();
    let i = v.data.len() / 2;
    v.data[i] = if v.data[i] >= v.dtype.max_value() - 7 { v.data[i] - 7 } else { v.data[i] + 7 };
    v
}

#[test]
fn a_consistent_lie_at_a_boundary_row_passes_downstream_and_is_found_upstream() {
    let f = dense();
    let walk = Walk::new(&f.program);
    // Layer 1's carry-out at position 6 — shard 0's output, shard 1's input — altered, and every later
    // leaf computed honestly from the altered row.
    let block = walk.block(2);
    let node = block.carry_out[0];
    let lie: Lie = (6, 2, node, bump);
    let lying = produced(&f, &walk, Some(lie));
    let coord = Coord::Commit { pos: 6, slot: walk.bases[2] + node as u32 };
    let results = check_all(&f, &walk, &lying);
    assert_eq!(results[&(0, 1)], Err(coord), "the upstream cell that computes the boundary row finds the lie");
    for (cell, r) in &results {
        if *cell != (0, 1) {
            assert_eq!(*r, Ok(()), "cell {cell:?} sees inputs and outputs that agree with each other");
        }
    }
    eprintln!("consistent lie at {coord:?} (a boundary row): the downstream cell (1, 1) verifies; the upstream cell (0, 1) finds it");
}

#[test]
fn a_consistent_lie_in_a_state_is_found_by_the_segment_that_writes_it_and_not_by_the_next() {
    let f = gdn();
    let walk = Walk::new(&f.program);
    // Layer 1's state write (occurrence 2) at position 3 — the last position of segment 1, so the
    // lie is in the checkpoint segment 2 starts from — altered, and every later leaf computed from it.
    let block = walk.block(2);
    let node = block.nodes.iter().position(|n| matches!(n.prim, Prim::StateWrite { .. })).expect("a state write") as u16;
    let lie: Lie = (3, 2, node, bump);
    let lying = produced(&f, &walk, Some(lie));
    let results = check_all(&f, &walk, &lying);
    let found = results[&(1, 1)];
    assert!(found.is_err(), "the cell that writes the state finds the lie");
    eprintln!("consistent lie in a state at position 3: cell (1, 1) finds it at {:?}", found.unwrap_err());
    assert_eq!(results[&(1, 2)], Ok(()), "the next segment starts from the lying checkpoint and verifies");
    for (cell, r) in &results {
        if cell.0 == 0 {
            assert_eq!(*r, Ok(()), "shard 0 neither holds nor reads layer 1's state");
        }
    }
    // The state is per-layer and committed only every C positions: a lie is found at the first leaf
    // computed from it — a commit point reading the new state, or the checkpoint itself.
    let _ = RunState::default();
}
