//! Kind `map` (ADR-0078 Decision 8, row `map`): "tile / graph DSL: cells, edges, integer
//! attributes → integer map compiler → the map file → pure function". It is the least specified
//! row in the table and its "not covered" cell is empty, so this module both implements the row
//! and states its own edges (§ *What this kind does not cover*). Grammar `map/v1`, transformer
//! `map/mmap/v1`, canonical writer `misaka-map/1/canonical-v1`. Everything from DSL bytes to
//! artifact bytes is integer arithmetic (Decision 3, invariant X3): no floating-point type name
//! is spelled anywhere in this file — a test scans the source for both — no clock, no randomness,
//! no I/O, no hash-map iteration.
//!
//! # What a consumer does with the file
//!
//! The artifact is `MMAP` (below), a compact binary map file a game or a tool loads directly:
//!
//! * the **palette** and the **tile layer** are what it draws, one byte per cell, row-major;
//! * the **region layer** answers "can this creature walk from here to there?" in one comparison
//!   — two cells are mutually reachable iff their region ids are equal and non-zero — and the
//!   **region table** gives each region's area, its total traversal cost, its bounding box and a
//!   representative cell, so a spawner picks a room without scanning the grid;
//! * the **distance field** is a flow field: each cell holds the integer cost of the cheapest
//!   walk from the nearest node, so an agent descends the gradient toward the closest point of
//!   interest with no search of its own;
//! * the **node/edge graph** and its **distance matrix** are the route planner: named places,
//!   directed connections with integer weights, and the all-pairs cheapest cost between them.
//!
//! Three of those five sections are *derived*, not transcribed. That is deliberate. ADR-0078
//! Decision 7 names the door to weight for "a transformer whose computation is an integer step
//! space", and a compiler that computes something is a better candidate for that door than one
//! that re-serializes its input. Nothing here weighs anything today (X5).
//!
//! # The DSL — grammar `map/v1`
//!
//! A JSON object with exactly these keys (an unknown key is a grammar refusal, a missing one
//! too):
//!
//! ```text
//! { "v": 1,
//!   "width": 1..=1024, "height": 1..=1024,      width * height <= 262144
//!   "default": 0..=255,                          the tile of every cell no run covers
//!   "palette": [ {"tile": 0..=255, "cost": 0..=8191, "passable": bool} , … ]   1..=256 entries
//!   "runs":    [ [x, y, len, tile] , … ]                                       0..=65536 runs
//!   "nodes":   [ {"id": str, "x": i, "y": i, "attrs": {name: i64}} , … ]       0..=256 nodes
//!   "edges":   [ {"from": str, "to": str, "w": 0..=1000000} , … ]              0..=4096 edges }
//! ```
//!
//! A **run** paints `len` cells of one row: cells `(x, y) … (x + len − 1, y)`, `len >= 1`,
//! `x + len <= width`. The **palette** must name the default tile and every tile a run paints —
//! a tile with no cost and no passability is a tile the compiler cannot walk on, and guessing
//! one would be a silent default (a run's `cost`, and whether it is `passable`, are what the
//! whole compilation is a function of). A **node** stands on a cell, carries up to 16 integer
//! attributes, and must stand on a *passable* cell: a node on a wall has no region, no distance
//! and no route, so every derived section about it would be a sentinel, and this kind refuses
//! rather than emits meaningless values. An **edge** is DIRECTED (`from` → `to`); an undirected
//! connection is two edges, because a canonical orientation for an unordered pair is a rule with
//! no natural answer and a one-way passage is a thing maps actually have.
//!
//! # Order: the canonical question of this kind, and its answer
//!
//! A map is a grid AND a graph, so the same map arrives with its cells and its edges in any
//! order. **The grammar sorts; it never refuses an ordering** — the order of these four lists
//! carries no meaning, and ADR-0078 Decision 2 makes a canonicalizer "a pure function
//! (whitespace, key order, number form, nothing semantic)", so imposing the one spelling of a set
//! is exactly its job. The keys, each stated once and pinned by
//! [`tests::x3_two_orderings_of_one_map_produce_the_same_dsl_and_the_same_artifact_bytes`]:
//!
//! | list | ascending by | why the order can mean nothing |
//! |---|---|---|
//! | `palette` | `tile` | tile ids are unique; a duplicate is refused |
//! | `runs` | `(y, x)` — row-major, the order the grid is written in | runs may not overlap; a later run can therefore never win over an earlier one |
//! | `nodes` | `id`, by UTF-8 bytes | ids are unique; a duplicate is refused |
//! | `edges` | `(from, to)`, by UTF-8 bytes | `(from, to)` is unique; a duplicate is refused, and so is a self-loop |
//!
//! Every one of those "meaning" columns is a refusal this grammar makes, and it makes them so
//! that the sort is safe: **overlapping runs are the case where order WOULD mean something**
//! (last writer wins), so they are refused by name rather than ordered by a rule the DSL's author
//! did not know about. A node's index in the artifact, and therefore the row and column it owns
//! in the distance matrix, is its position in the sorted node list — so the artifact's own
//! ordering is the DSL's canonical ordering, once.
//!
//! What canonical does NOT mean here: the DSL is not a normal form of the *map*. Two different
//! canonical DSLs can compile to the same grid (a run that paints the default tile, or two
//! adjacent runs of one tile that were not merged). That is correct and deliberate: `dsl_hash`
//! names the answer the model wrote, `artifact_hash` names the map that was made of it, and
//! collapsing the first into the second would be the semantic edit Decision 2 forbids.
//!
//! # The compiler — what the integer work is
//!
//! 1. **The grid.** `width * height` bytes of tile id, filled with `default` and then painted by
//!    every run. Runs do not overlap, so the paint order cannot matter.
//! 2. **Regions** — connected components of the passable cells under 4-connectivity, by
//!    union-find (union by size, path halving) over a single row-major pass that unions each
//!    passable cell with its right and its down neighbour. Region ids are then assigned by a
//!    second row-major pass, `1, 2, 3, …` in the order the components' *first* cells appear, so
//!    the numbering is a function of the grid and not of the union order. `0` is reserved for an
//!    impassable cell. Each region's area, total traversal cost, bounding box and first cell go
//!    in the region table.
//! 3. **The distance field** — one multi-source integer Dijkstra over the passable cells with
//!    every node's cell as a source at distance 0, moving 4-connected, *entering* a cell costing
//!    that cell's tile cost. Unreachable and impassable cells hold [`UNREACHABLE`]. Zero-cost
//!    tiles are admitted, which is why this is Dijkstra and not a breadth-first walk. The output
//!    is a function of the grid alone: shortest distances are unique whatever order the frontier
//!    is popped in, and the frontier is ordered by `(distance, cell index)` anyway, so even the
//!    pop sequence reproduces.
//! 4. **The graph distance matrix** — Floyd–Warshall over the nodes in `u64` with saturating
//!    addition, `n <= 256`, written as `u32` with [`UNREACHABLE`] for "no directed walk".
//!
//! Nothing in the compiler reads a clock, draws a random number or iterates a hash map, and every
//! bound above is a named constant, so the largest map the grammar admits is a finite amount of
//! work stated in advance rather than discovered at run time.
//!
//! # The artifact — `MMAP` v1, writer `misaka-map/1/canonical-v1`
//!
//! Little-endian throughout; strings are a `u32` byte length followed by the bytes. **There is no
//! padding and no alignment anywhere in this file** — every field is written at the offset the
//! preceding fields end at. That is the pinned decision, and its reason is that padding is a
//! place two writers can disagree while both "look right": a reader walks this format with
//! explicit offsets computed from the header, never by casting a struct over it.
//!
//! ```text
//! "MMAP"  u16 version=1  u16 flags=0   (flags MUST be 0; a reader refuses what it cannot name)
//! u32 width  u32 height  u32 palette_count  u32 region_count  u32 node_count  u32 edge_count
//! palette   palette_count × ( u8 tile ‖ u8 passable(0|1) ‖ u16 cost )            ascending by tile
//! tiles     width*height × u8 tile id                                            row-major, y*width + x
//! regions   width*height × u32 region id                                         row-major; 0 = impassable
//! table     region_count × ( u32 id ‖ u32 area ‖ u64 cost_sum ‖ u32 first_cell
//!                            ‖ u16 min_x ‖ u16 min_y ‖ u16 max_x ‖ u16 max_y )   ascending by id
//! distance  width*height × u32 cost to the nearest node                          0xFFFFFFFF = unreachable
//! nodes     node_count × ( str id ‖ u32 x ‖ u32 y ‖ u32 region
//!                          ‖ u32 attr_count ‖ ( str name ‖ i64 value )… )        ascending by id; attrs by name
//! edges     edge_count × ( u32 from_index ‖ u32 to_index ‖ u32 weight )          ascending by (from_index, to_index)
//! matrix    node_count² × u32 cheapest directed cost, row-major i*n + j          0xFFFFFFFF = unreachable
//! u32 CRC-32 over every byte before it
//! ```
//!
//! The exact length is [`artifact_len`], a function of the map and its compilation; an upper
//! bound that needs only the DSL is [`artifact_len_bound`], and the ceiling ([`MAX_ARTIFACT_BYTES`])
//! is checked against the bound *before a single cell is labelled*, so an oversized map costs the
//! executor nothing. The grammar's bounds keep the largest admissible map well under the
//! ceiling, and a test says so rather than leaving a reader to multiply it out.
//!
//! # The bounds this kind declares (ADR-0078 SA-2)
//!
//! "A DSL is attacker-shaped — it is the model's answer to a stranger's prompt". Three bounds,
//! declared as constants of this module, each checked BEFORE the work it guards, each refusal
//! "no object" (Decision 2's parse-failure arm, X4):
//!
//! | bound | value | checked | by |
//! |---|---|---|---|
//! | `max_dsl_bytes` | [`MAX_DSL_BYTES`] | on the byte count, before the parser allocates | [`MapGrammar::canonicalize`] and [`MapCompilerTransformer::run`] |
//! | `max_steps` | [`MAX_COMPILE_STEPS`] | on [`compile_steps_bound`], before [`compile`] | [`derive_artifact`] |
//! | `max_artifact_bytes` | [`MAX_ARTIFACT_BYTES`] | on [`artifact_len_bound`], before [`compile`] | [`derive_artifact`] |
//!
//! They are readable in one value, [`BOUNDS`], for a gateway sizing an answer or a consumer
//! sizing a verification. SA-2 puts them in the transformer's manifest; the shipped
//! `TransformerManifest` has no fields for them yet, and until it does they reach
//! `transformer_id` through `source_tree_sha256` — a bound is a constant of this file, and this
//! file is in the build's source-tree hash, so a changed bound is a different transformer.
//!
//! The corpus exhausts them (SA-2: "X3's drill includes a bound-exhausting corpus"):
//! `92-refused-dsl-too-large.json` is one byte over [`MAX_DSL_BYTES`] and is refused without
//! being parsed — the golden pins the byte-count message, which is the proof that the ceiling
//! ran before the parser did — and `91-refused-beyond-the-ceilings.json` asks for a 1024×1024
//! grid: four times the cells the grammar admits, which is more steps than the step ceiling and
//! half again the artifact ceiling, both measured in
//! [`tests::sa2_the_corpus_sample_asks_for_more_than_both_compiler_ceilings`]. That sample is
//! refused by the grammar's own cell bound, which is these two ceilings' first face — no map the
//! grammar admits can reach either of them, and the transformer checks them a second time
//! anyway, because [`derive_artifact`] can be handed a [`Map`] a caller built by hand.
//!
//! # The invariants, where they touch this kind
//!
//! * **X3** — integer arithmetic, byte-identical on two architectures. No float type name is
//!   spelled in this file, no clock is read, nothing is drawn at random and no hash map is
//!   iterated; the tests scan the source for all four, and the drill compares the artifact
//!   hashes this file produces on x86_64 with the ones it produces on aarch64.
//! * **X4** — every refusal above produces no object and changes nothing about the claim.
//! * **X6** — a consumer with the answer bytes and the object recomputes `dsl_hash`,
//!   `artifact_hash` and `artifact_bytes` here, with no executor to trust.
//! * **X8** — this kind is id 5, `kind::MAP`, assigned once. The chain interprets none of it;
//!   what the id MEANS is this manifest, and an object whose kind disagrees with it is a false
//!   object anyone holding this file can demonstrate.
//! * **X9** — a `map/v1` derivation takes no input but the DSL: there is no hash naming bytes
//!   held elsewhere (Decision 10's transformation mode is not this grammar's), so `dsl_hash`
//!   fixes the whole derivation on its own. Everything the compiler measures — a region's area
//!   and cost, a cell's distance, a route's cost — is an integer cost model, never a clock.
//!
//! # What this kind does not cover
//!
//! Decision 8's `map` row leaves its "not covered" column empty. These are the lines this
//! implementation draws, each because the alternative would be a value two honest hosts could
//! disagree on, or another row's work:
//!
//! * **Height, voxels, a third axis.** A `map/v1` map is a plane. A 3D world is a `scene`
//!   (`.glb`), and a heightfield would need a v2 grammar, not a v1 edit (Decision 8's last
//!   paragraph: a new grammar is a new id, never an edited row).
//! * **Geographic coordinates and projections** — the `gis` half of the candidate table's row 5.
//!   Latitudes and projections are not integers on this path, and a fixed-point projection is a
//!   choice of datum and scale that belongs in its own grammar with its own id.
//! * **Procedural generation from a seed.** A map here is *given*; growing one from a seed and a
//!   rule set is kind 27, `procedural`, whose DSL is the seed and the rules.
//! * **Rendering.** Turning the map into pixels is kind `image`; this transformer emits data, and
//!   drawing it is the consumer's.
//! * **Diagonal movement.** Connectivity and distance are 4-connected. A diagonal step's cost is
//!   irrational under a Euclidean metric, so any integer diagonal cost is an arbitrary choice, and
//!   an arbitrary choice belongs in a named grammar rather than a default nobody voted for.
//! * **Non-square grids** (hex, isometric), **layered/multi-floor maps**, and **negative or
//!   fractional edge weights**: each is a different grammar, and a negative weight would make
//!   Floyd–Warshall's saturating arithmetic wrong rather than merely different.

use crate::bytes::{put_i64_le, put_u16_le, put_u32_le, put_u64_le};
use crate::canon_json::{CanonValue, parse_canonical, write_canonical};
use crate::checksum::crc32;
use crate::{Artifact, DeriveError, Discipline, Grammar, Transformer, TransformerManifest};
use kaspa_consensus_core::palw_derived_v1::kind;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

/// The grammar's name (Decision 2): `grammar_id = H(domain ‖ name)`.
pub const GRAMMAR_NAME: &str = "map/v1";
/// The transformer's name, the first field of its manifest (Decision 3).
pub const TRANSFORMER_NAME: &str = "map/mmap/v1";
/// The canonical writer named in the manifest.
pub const WRITER_NAME: &str = "misaka-map/1/canonical-v1";
pub const MAGIC: &[u8; 4] = b"MMAP";
pub const ARTIFACT_VERSION: u16 = 1;
/// Reserved, and pinned at zero: a reader that finds another value is reading a file this
/// writer did not write, and should say so rather than guess which sections are present.
pub const ARTIFACT_FLAGS: u16 = 0;
pub const MEDIA_TYPE: &str = "application/vnd.misaka.mmap";
pub const EXTENSION: &str = "mmap";

/// Largest `width` and `height`.
pub const MAX_SIDE: u32 = 1024;
/// Largest `width * height`. 2^18 cells, so the tile layer is at most 256 KiB and the two u32
/// layers 1 MiB each.
pub const MAX_CELLS: u64 = 1 << 18;
/// Largest per-tile movement cost. Chosen so that [`MAX_FINITE_DISTANCE`] stays below
/// [`UNREACHABLE`]; the `const` assertion below is the proof, not this sentence.
pub const MAX_TILE_COST: u32 = (1 << 13) - 1;
/// Most palette entries — one per distinct tile id, and a tile id is a `u8`.
pub const MAX_PALETTE: usize = 256;
pub const MAX_RUNS: usize = 65_536;
pub const MAX_NODES: usize = 256;
pub const MAX_EDGES: usize = 4096;
pub const MAX_EDGE_WEIGHT: i64 = 1_000_000;
pub const MAX_ATTRS: usize = 16;
pub const MAX_NAME_BYTES: usize = 64;
/// "No walk exists", in the distance field and in the distance matrix alike. It is `u32::MAX`
/// and never a real distance, which the two `const` assertions below establish from the bounds
/// rather than assert by hand.
pub const UNREACHABLE: u32 = u32::MAX;
/// The region id of an impassable cell. Regions are numbered from 1.
pub const IMPASSABLE_REGION: u32 = 0;

// ─── the three bounds this transformer declares (ADR-0078 SA-2) ────────────────────────────

/// **`max_dsl_bytes`.** The most answer bytes this kind will look at. A DSL is attacker-shaped —
/// it is the model's answer to a stranger's prompt — and a JSON parser is an allocator driven by
/// its input, so the ceiling is checked on the byte COUNT before the parser is asked what the
/// bytes spell. Exceeding it is "no object" through Decision 2's parse-failure arm (a grammar
/// refusal, X4), never a repair and never a truncation.
///
/// The number is the retention payload's own cap (`PALW_FP_DSL_V1_MAX_BYTES`): a DSL above it
/// could not be served to a verifier under Decision 6 even if it compiled, so deriving from one
/// would be building a derivation nobody could check. It is spelled here rather than imported,
/// because a bound is part of what `transformer_id` names and only THIS crate's source tree
/// reaches that id (Decision 3); the assertion below is what keeps the two numbers in step.
pub const MAX_DSL_BYTES: usize = 4 * 1024 * 1024;
const _: () = assert!(
    MAX_DSL_BYTES <= kaspa_consensus_core::palw_derived_v1::PALW_FP_DSL_V1_MAX_BYTES,
    "a DSL this kind admits must fit the payload that would serve it (ADR-0078 Decision 6)"
);

/// **`max_artifact_bytes`.** The largest map file this transformer hands out — to the person who
/// asked, and to every consumer who re-runs the derivation to check it (Decision 5). Checked
/// against [`artifact_len_bound`], which needs only the DSL, before a single cell is labelled.
pub const MAX_ARTIFACT_BYTES: usize = 16 * 1024 * 1024;

/// **`max_steps`, in this kind's own unit.** SA-2 asks for `max_steps` "or the kind's own unit";
/// a map compiler's unit is the visit: one look at a cell, or at one entry of the route matrix.
/// The whole compilation is bounded by the DSL alone, with no data-dependent loop anywhere:
///
/// ```text
/// grid fill        cells, then Σ run lengths (runs may not overlap, so Σ len <= cells)   2·cells
/// union pass       two unions per cell                                                   2·cells
/// labelling pass   one visit per cell                                                      cells
/// Dijkstra         a cell is relaxed at most once per neighbour (4) and each pop looks
///                  at its four neighbours                                               20·cells
/// Floyd–Warshall   one visit per (k, i, j)                                                   n³
/// ```
///
/// so [`compile_steps_bound`] is `25·cells + n³` and the ceiling is what the grammar's own
/// largest map costs — `MAX_CELLS` cells and `MAX_NODES` nodes. A map the grammar admits can
/// therefore never exceed it, which is the point: the grammar is this ceiling's first face, and
/// the transformer checks it a second time because [`derive_artifact`] can be called with a
/// [`Map`] a caller built by hand and never passed through the grammar at all.
pub const STEPS_PER_CELL: u64 = 25;
pub const MAX_COMPILE_STEPS: u64 = STEPS_PER_CELL * MAX_CELLS + (MAX_NODES as u64).pow(3);

/// The three bounds of SA-2, in one value, so that a gateway sizing an answer or a consumer
/// sizing a verification can read them without reading this file.
///
/// SA-2 puts them in the transformer's manifest. The shipped `TransformerManifest` has no fields
/// for them yet (that type is not this module's to edit); until it does, they reach
/// `transformer_id` the way every other constant here does — through `source_tree_sha256`, which
/// IS a manifest field, so changing a bound changes the id and a derivation made under the old
/// bound stays checkable against the old id forever (Decision 8's last paragraph).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeclaredBounds {
    pub max_dsl_bytes: u64,
    pub max_artifact_bytes: u64,
    /// In the unit [`MAX_COMPILE_STEPS`] documents: cell visits and matrix-entry visits.
    pub max_steps: u64,
}

/// What `map/mmap/v1` declares (SA-2), enforced by [`MapGrammar::canonicalize`],
/// [`MapCompilerTransformer::run`] and [`derive_artifact`] before the work each one guards.
pub const BOUNDS: DeclaredBounds = DeclaredBounds {
    max_dsl_bytes: MAX_DSL_BYTES as u64,
    max_artifact_bytes: MAX_ARTIFACT_BYTES as u64,
    max_steps: MAX_COMPILE_STEPS,
};

/// SA-2's first gate, on both the grammar's and the transformer's entry: refuse on the byte
/// count, before anything parses or allocates.
fn check_dsl_bytes(bytes: &[u8]) -> Result<(), DeriveError> {
    if bytes.len() > MAX_DSL_BYTES {
        return Err(grammar(format!(
            "a DSL of {} bytes exceeds the {MAX_DSL_BYTES}-byte ceiling this transformer declares (ADR-0078 SA-2)",
            bytes.len()
        )));
    }
    Ok(())
}

/// The largest finite grid distance: a cheapest walk visits a cell at most once, and entering
/// each costs at most [`MAX_TILE_COST`].
const MAX_FINITE_DISTANCE: u64 = MAX_CELLS * MAX_TILE_COST as u64;
const _: () = assert!(MAX_FINITE_DISTANCE < UNREACHABLE as u64, "a real distance must never collide with the sentinel");
/// The largest finite graph distance: a cheapest directed walk uses at most `n − 1` edges.
const MAX_FINITE_GRAPH_DISTANCE: u64 = (MAX_NODES as u64 - 1) * MAX_EDGE_WEIGHT as u64;
const _: () = assert!(MAX_FINITE_GRAPH_DISTANCE < UNREACHABLE as u64, "a real route cost must never collide with the sentinel");
/// The artifact spells a tile cost in sixteen bits, so the grammar's ceiling has to fit there.
const _: () = assert!(MAX_TILE_COST <= u16::MAX as u32, "a palette cost must fit the field the writer gives it");

/// This kind's grammar and transformer, as the registry sees them.
pub fn register() -> (Vec<Box<dyn Grammar>>, Vec<Box<dyn Transformer>>) {
    (vec![Box::new(MapGrammar)], vec![Box::new(MapCompilerTransformer)])
}

/// The grammar `map/v1`: parse, validate the schema above, re-emit with every declared list in
/// its canonical order.
pub struct MapGrammar;

impl Grammar for MapGrammar {
    fn name(&self) -> &'static str {
        GRAMMAR_NAME
    }

    fn canonicalize(&self, answer: &[u8]) -> Result<Vec<u8>, DeriveError> {
        check_dsl_bytes(answer)?;
        let map = Map::from_canon(&parse_canonical(answer)?)?;
        Ok(write_canonical(&map.to_canon()))
    }
}

/// The transformer `map/mmap/v1`: canonical map bytes in, an `MMAP` artifact out.
pub struct MapCompilerTransformer;

impl Transformer for MapCompilerTransformer {
    fn manifest(&self) -> TransformerManifest {
        TransformerManifest {
            name: TRANSFORMER_NAME,
            kind: kind::MAP,
            grammar: GRAMMAR_NAME,
            discipline: Discipline::Integer,
            writer: WRITER_NAME,
            source_tree_sha256: crate::SOURCE_TREE_SHA256_HEX,
            // ADR-0078 SA-2, from this module's own declaration and not a copy of it.
            max_dsl_bytes: BOUNDS.max_dsl_bytes,
            max_artifact_bytes: BOUNDS.max_artifact_bytes,
            max_steps: BOUNDS.max_steps,
        }
    }

    /// Re-canonicalizes and refuses anything but canonical bytes — a transformer repairs nothing
    /// (Decision 3) — then compiles and writes. The DSL ceiling is checked here as well as in the
    /// grammar, because a consumer verifying a derivation may hand these bytes straight in
    /// (`verify`), and a transformer that trusts its caller's bounds has none of its own (SA-2).
    fn run(&self, dsl: &[u8]) -> Result<Artifact, DeriveError> {
        check_dsl_bytes(dsl)?;
        let map = Map::from_canon(&parse_canonical(dsl)?)?;
        if write_canonical(&map.to_canon()) != dsl {
            return Err(DeriveError::Transformer("input is not canonical map/v1 bytes".into()));
        }
        Ok(Artifact { bytes: derive_artifact(&map)?, media_type: MEDIA_TYPE, extension: EXTENSION })
    }
}

// ─── the DSL ───────────────────────────────────────────────────────────────────────────────

/// One palette entry: what a tile costs to enter and whether it can be entered at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tile {
    pub tile: u8,
    pub cost: u32,
    pub passable: bool,
}

/// One painted horizontal span: cells `(x, y) … (x + len − 1, y)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileRun {
    pub x: u32,
    pub y: u32,
    pub len: u32,
    pub tile: u8,
}

/// A named place on the grid, with its integer attributes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub id: String,
    pub x: u32,
    pub y: u32,
    pub attrs: BTreeMap<String, i64>,
}

/// A directed connection between two named places, with its integer weight.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub w: u32,
}

/// A validated map. Its canonical bytes are `write_canonical(&map.to_canon())`, and every list
/// below is already in the canonical order the module doc's table names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Map {
    pub width: u32,
    pub height: u32,
    pub default: u8,
    /// Ascending by `tile`, unique, non-empty.
    pub palette: Vec<Tile>,
    /// Ascending by `(y, x)`, non-overlapping.
    pub runs: Vec<TileRun>,
    /// Ascending by `id`, unique.
    pub nodes: Vec<Node>,
    /// Ascending by `(from, to)`, unique, no self-loop, every endpoint a node.
    pub edges: Vec<Edge>,
}

fn grammar(msg: impl Into<String>) -> DeriveError {
    DeriveError::Grammar(msg.into())
}

/// Exactly `keys`, no more and no fewer — an unknown key is as much a refusal as a missing one,
/// because a key this grammar does not read is a meaning the artifact would silently drop.
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

fn name_str(v: &CanonValue, what: &str) -> Result<String, DeriveError> {
    v.as_str()
        .filter(|s| (1..=MAX_NAME_BYTES).contains(&s.len()))
        .map(|s| s.to_string())
        .ok_or_else(|| grammar(format!("{what} must be a string of 1..={MAX_NAME_BYTES} bytes")))
}

impl Map {
    /// Validate a parsed tree against the grammar. Every refusal names the rule that refused
    /// and what it saw; nothing is repaired and nothing is defaulted.
    pub fn from_canon(v: &CanonValue) -> Result<Self, DeriveError> {
        let obj = v.as_obj().ok_or_else(|| grammar("the map must be a JSON object"))?;
        expect_keys(obj, "the map", &["v", "width", "height", "default", "palette", "runs", "nodes", "edges"])?;
        if obj["v"].as_i64() != Some(1) {
            return Err(grammar("v must be 1"));
        }
        let width = int_in(&obj["width"], "width", 1, i64::from(MAX_SIDE))? as u32;
        let height = int_in(&obj["height"], "height", 1, i64::from(MAX_SIDE))? as u32;
        let cells = u64::from(width) * u64::from(height);
        if cells > MAX_CELLS {
            return Err(grammar(format!("{width}x{height} is {cells} cells; the grammar admits {MAX_CELLS}")));
        }
        let default = int_in(&obj["default"], "default", 0, 255)? as u8;
        let palette = parse_palette(&obj["palette"])?;
        let runs = parse_runs(&obj["runs"], width, height)?;
        // The palette is the only source of a tile's cost and passability, so a tile that reaches
        // the grid without one would be a silent default in the middle of the compilation.
        let known = |t: u8| palette.iter().any(|p| p.tile == t);
        if !known(default) {
            return Err(grammar(format!("the default tile {default} has no palette entry")));
        }
        if let Some(r) = runs.iter().find(|r| !known(r.tile)) {
            return Err(grammar(format!("a run paints tile {} at ({}, {}), which has no palette entry", r.tile, r.x, r.y)));
        }
        let nodes = parse_nodes(&obj["nodes"], width, height)?;
        let edges = parse_edges(&obj["edges"], &nodes)?;
        let map = Map { width, height, default, palette, runs, nodes, edges };
        // A node on a wall has no region, no distance and no route: every derived section about
        // it would be a sentinel, so the map is refused instead of compiled into meaningless
        // values (the house rule: fail closed, and by name).
        let grid = map.grid();
        for n in &map.nodes {
            let tile = grid[map.cell_index(n.x, n.y)];
            if !map.tile_passable(tile) {
                return Err(grammar(format!(
                    "node {:?} stands on tile {tile} at ({}, {}), which the palette marks impassable",
                    n.id, n.x, n.y
                )));
            }
        }
        Ok(map)
    }

    /// The canonical tree: exactly the grammar's keys, every declared list in its canonical
    /// order (the module doc's table).
    pub fn to_canon(&self) -> CanonValue {
        let mut top = BTreeMap::new();
        top.insert("v".to_string(), CanonValue::Int(1));
        top.insert("width".to_string(), CanonValue::Int(i128::from(self.width)));
        top.insert("height".to_string(), CanonValue::Int(i128::from(self.height)));
        top.insert("default".to_string(), CanonValue::Int(i128::from(self.default)));
        let palette = self
            .palette
            .iter()
            .map(|p| {
                let mut o = BTreeMap::new();
                o.insert("tile".to_string(), CanonValue::Int(i128::from(p.tile)));
                o.insert("cost".to_string(), CanonValue::Int(i128::from(p.cost)));
                o.insert("passable".to_string(), CanonValue::Bool(p.passable));
                CanonValue::Obj(o)
            })
            .collect();
        top.insert("palette".to_string(), CanonValue::Arr(palette));
        let runs = self
            .runs
            .iter()
            .map(|r| {
                CanonValue::Arr(vec![
                    CanonValue::Int(i128::from(r.x)),
                    CanonValue::Int(i128::from(r.y)),
                    CanonValue::Int(i128::from(r.len)),
                    CanonValue::Int(i128::from(r.tile)),
                ])
            })
            .collect();
        top.insert("runs".to_string(), CanonValue::Arr(runs));
        let nodes = self
            .nodes
            .iter()
            .map(|n| {
                let mut o = BTreeMap::new();
                o.insert("id".to_string(), CanonValue::Str(n.id.clone()));
                o.insert("x".to_string(), CanonValue::Int(i128::from(n.x)));
                o.insert("y".to_string(), CanonValue::Int(i128::from(n.y)));
                let attrs = n.attrs.iter().map(|(k, v)| (k.clone(), CanonValue::Int(i128::from(*v)))).collect();
                o.insert("attrs".to_string(), CanonValue::Obj(attrs));
                CanonValue::Obj(o)
            })
            .collect();
        top.insert("nodes".to_string(), CanonValue::Arr(nodes));
        let edges = self
            .edges
            .iter()
            .map(|e| {
                let mut o = BTreeMap::new();
                o.insert("from".to_string(), CanonValue::Str(e.from.clone()));
                o.insert("to".to_string(), CanonValue::Str(e.to.clone()));
                o.insert("w".to_string(), CanonValue::Int(i128::from(e.w)));
                CanonValue::Obj(o)
            })
            .collect();
        top.insert("edges".to_string(), CanonValue::Arr(edges));
        CanonValue::Obj(top)
    }

    pub fn cells(&self) -> usize {
        self.width as usize * self.height as usize
    }

    pub fn cell_index(&self, x: u32, y: u32) -> usize {
        y as usize * self.width as usize + x as usize
    }

    /// The palette lookups, as two 256-entry integer tables — a tile id is a `u8`, so this is
    /// the whole map from tile to (cost, passability) with no search on the hot path. A tile
    /// with no palette entry cannot reach the grid (`from_canon` refuses it), so the unnamed
    /// slots are never read; they are impassable at cost 0 so that a misuse fails closed.
    pub fn palette_tables(&self) -> ([u32; 256], [bool; 256]) {
        let mut cost = [0u32; 256];
        let mut passable = [false; 256];
        for p in &self.palette {
            cost[p.tile as usize] = p.cost;
            passable[p.tile as usize] = p.passable;
        }
        (cost, passable)
    }

    fn tile_passable(&self, tile: u8) -> bool {
        self.palette.iter().any(|p| p.tile == tile && p.passable)
    }

    /// The tile layer: `default` everywhere, then every run painted. The runs do not overlap
    /// (the grammar refuses overlap), so the order they are painted in cannot change the result.
    pub fn grid(&self) -> Vec<u8> {
        let mut grid = vec![self.default; self.cells()];
        for r in &self.runs {
            let start = self.cell_index(r.x, r.y);
            grid[start..start + r.len as usize].fill(r.tile);
        }
        grid
    }
}

fn parse_palette(v: &CanonValue) -> Result<Vec<Tile>, DeriveError> {
    let arr = v.as_arr().ok_or_else(|| grammar("palette must be an array of tile entries"))?;
    if arr.is_empty() {
        return Err(grammar("palette is empty; a map with no tile entries has no costs and no walls"));
    }
    if arr.len() > MAX_PALETTE {
        return Err(grammar(format!("more than {MAX_PALETTE} palette entries")));
    }
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let obj = item.as_obj().ok_or_else(|| grammar("a palette entry must be an object"))?;
        expect_keys(obj, "a palette entry", &["tile", "cost", "passable"])?;
        let tile = int_in(&obj["tile"], "a palette entry's tile", 0, 255)? as u8;
        let cost = int_in(&obj["cost"], "a palette entry's cost", 0, i64::from(MAX_TILE_COST))? as u32;
        let passable = obj["passable"].as_bool().ok_or_else(|| grammar("a palette entry's passable must be a boolean"))?;
        out.push(Tile { tile, cost, passable });
    }
    out.sort_unstable_by_key(|p| p.tile);
    if let Some(w) = out.windows(2).find(|w| w[0].tile == w[1].tile) {
        return Err(grammar(format!("tile {} has two palette entries", w[0].tile)));
    }
    Ok(out)
}

fn parse_runs(v: &CanonValue, width: u32, height: u32) -> Result<Vec<TileRun>, DeriveError> {
    let arr = v.as_arr().ok_or_else(|| grammar("runs must be an array of [x, y, len, tile] quads"))?;
    if arr.len() > MAX_RUNS {
        return Err(grammar(format!("more than {MAX_RUNS} runs")));
    }
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let q = item.as_arr().filter(|q| q.len() == 4).ok_or_else(|| grammar("a run must be an [x, y, len, tile] quad"))?;
        let x = int_in(&q[0], "a run's x", 0, i64::from(width) - 1)? as u32;
        let y = int_in(&q[1], "a run's y", 0, i64::from(height) - 1)? as u32;
        let len = int_in(&q[2], "a run's len", 1, i64::from(width - x))? as u32;
        let tile = int_in(&q[3], "a run's tile", 0, 255)? as u8;
        out.push(TileRun { x, y, len, tile });
    }
    // Row-major order, and then the one refusal that makes the sort safe: overlapping runs are
    // the case where the DSL's order WOULD carry meaning (last writer wins), so they are refused
    // rather than resolved by a rule the author never chose.
    // The sort key is the whole run, not only `(y, x)`: two runs that share a row and a column
    // overlap by construction and are about to be refused, and a total key makes the refusal a
    // deterministic function of the answer rather than of an unstable sort's tie-breaking.
    out.sort_unstable_by_key(|r| (r.y, r.x, r.len, r.tile));
    for w in out.windows(2) {
        if w[0].y == w[1].y && w[0].x + w[0].len > w[1].x {
            return Err(grammar(format!(
                "the run at ({}, {}) of length {} overlaps the run at ({}, {})",
                w[0].x, w[0].y, w[0].len, w[1].x, w[1].y
            )));
        }
    }
    Ok(out)
}

fn parse_nodes(v: &CanonValue, width: u32, height: u32) -> Result<Vec<Node>, DeriveError> {
    let arr = v.as_arr().ok_or_else(|| grammar("nodes must be an array of node objects"))?;
    if arr.len() > MAX_NODES {
        return Err(grammar(format!("more than {MAX_NODES} nodes")));
    }
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let obj = item.as_obj().ok_or_else(|| grammar("a node must be an object"))?;
        expect_keys(obj, "a node", &["id", "x", "y", "attrs"])?;
        let id = name_str(&obj["id"], "a node's id")?;
        let x = int_in(&obj["x"], "a node's x", 0, i64::from(width) - 1)? as u32;
        let y = int_in(&obj["y"], "a node's y", 0, i64::from(height) - 1)? as u32;
        let attrs_obj = obj["attrs"].as_obj().ok_or_else(|| grammar("a node's attrs must be an object of integer attributes"))?;
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
        out.push(Node { id, x, y, attrs });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    if let Some(w) = out.windows(2).find(|w| w[0].id == w[1].id) {
        return Err(grammar(format!("node id {:?} is used twice", w[0].id)));
    }
    Ok(out)
}

fn parse_edges(v: &CanonValue, nodes: &[Node]) -> Result<Vec<Edge>, DeriveError> {
    let arr = v.as_arr().ok_or_else(|| grammar("edges must be an array of edge objects"))?;
    if arr.len() > MAX_EDGES {
        return Err(grammar(format!("more than {MAX_EDGES} edges")));
    }
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let obj = item.as_obj().ok_or_else(|| grammar("an edge must be an object"))?;
        expect_keys(obj, "an edge", &["from", "to", "w"])?;
        let from = name_str(&obj["from"], "an edge's from")?;
        let to = name_str(&obj["to"], "an edge's to")?;
        let w = int_in(&obj["w"], "an edge's w", 0, MAX_EDGE_WEIGHT)? as u32;
        for end in [&from, &to] {
            if !nodes.iter().any(|n| n.id == *end) {
                return Err(grammar(format!("an edge names {end:?}, which is not a node")));
            }
        }
        // A self-loop is never on a cheapest walk, so it is a field with no effect — and a field
        // with no effect is a second spelling of the same map.
        if from == to {
            return Err(grammar(format!("an edge runs from {from:?} to itself")));
        }
        out.push(Edge { from, to, w });
    }
    out.sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));
    if let Some(w) = out.windows(2).find(|w| w[0].from == w[1].from && w[0].to == w[1].to) {
        return Err(grammar(format!("two edges run from {:?} to {:?}", w[0].from, w[0].to)));
    }
    Ok(out)
}

// ─── the compiler ──────────────────────────────────────────────────────────────────────────

/// A disjoint-set forest over cell indices: union by size, path halving. Which cell ends up the
/// root depends on the union order, and nothing downstream reads the root — [`compile`] numbers
/// the components by the row-major position of their FIRST cell, so the region ids are a
/// function of the grid and of nothing else.
struct DisjointSet {
    parent: Vec<u32>,
    size: Vec<u32>,
}

impl DisjointSet {
    fn new(n: usize) -> Self {
        DisjointSet { parent: (0..n as u32).collect(), size: vec![1u32; n] }
    }

    fn find(&mut self, mut x: u32) -> u32 {
        while self.parent[x as usize] != x {
            let grandparent = self.parent[self.parent[x as usize] as usize];
            self.parent[x as usize] = grandparent;
            x = grandparent;
        }
        x
    }

    fn union(&mut self, a: u32, b: u32) {
        let (mut ra, mut rb) = (self.find(a), self.find(b));
        if ra == rb {
            return;
        }
        if self.size[ra as usize] < self.size[rb as usize] {
            std::mem::swap(&mut ra, &mut rb);
        }
        self.parent[rb as usize] = ra;
        self.size[ra as usize] += self.size[rb as usize];
    }
}

/// One connected component of the passable cells. `id` counts from 1; `0` is
/// [`IMPASSABLE_REGION`] and never appears here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    pub id: u32,
    /// How many cells the component holds.
    pub area: u32,
    /// The sum of those cells' tile costs — what it costs to enter every cell of the region once.
    pub cost_sum: u64,
    /// The row-major index of the component's first cell; the id order IS this order.
    pub first_cell: u32,
    pub min_x: u16,
    pub min_y: u16,
    pub max_x: u16,
    pub max_y: u16,
}

/// What the compiler derived from the map: three sections the DSL does not contain, plus the
/// tile layer it does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Compiled {
    /// Tile per cell, row-major.
    pub grid: Vec<u8>,
    /// Region id per cell, row-major; [`IMPASSABLE_REGION`] where the tile cannot be entered.
    pub region: Vec<u32>,
    /// Ascending by `id`.
    pub regions: Vec<Region>,
    /// Cost of the cheapest walk from the nearest node, per cell; [`UNREACHABLE`] where none.
    pub distance: Vec<u32>,
    /// The region each node stands in, in node order. Never [`IMPASSABLE_REGION`] — the grammar
    /// refuses a node on a wall.
    pub node_region: Vec<u32>,
    /// All-pairs cheapest directed cost between nodes, row-major `i * n + j`; [`UNREACHABLE`]
    /// where no directed walk exists. `n * n` entries.
    pub matrix: Vec<u32>,
}

/// The whole integer computation: grid, regions, distance field, route matrix. A pure function
/// of the map (ADR-0078 Decision 3): no clock, no randomness, no I/O, no hash-map iteration.
pub fn compile(map: &Map) -> Compiled {
    let (cost, passable_tile) = map.palette_tables();
    let w = map.width as usize;
    let h = map.height as usize;
    let cells = map.cells();
    let grid = map.grid();

    // (1) components of the passable cells, 4-connected. Unioning each cell with its right and
    // its down neighbour in one row-major pass reaches every 4-adjacency exactly once.
    let mut ds = DisjointSet::new(cells);
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if !passable_tile[grid[i] as usize] {
                continue;
            }
            if x + 1 < w && passable_tile[grid[i + 1] as usize] {
                ds.union(i as u32, (i + 1) as u32);
            }
            if y + 1 < h && passable_tile[grid[i + w] as usize] {
                ds.union(i as u32, (i + w) as u32);
            }
        }
    }

    // (2) number them by the row-major position of each component's first cell, and accumulate
    // the region table in the same pass — so both the ids and the table are a function of the
    // grid alone, never of the order the unions happened in.
    let mut region = vec![IMPASSABLE_REGION; cells];
    let mut by_root: Vec<u32> = vec![IMPASSABLE_REGION; cells];
    let mut regions: Vec<Region> = Vec::new();
    for i in 0..cells {
        if !passable_tile[grid[i] as usize] {
            continue;
        }
        let root = ds.find(i as u32) as usize;
        let (x, y) = ((i % w) as u16, (i / w) as u16);
        if by_root[root] == IMPASSABLE_REGION {
            let id = regions.len() as u32 + 1;
            by_root[root] = id;
            regions.push(Region { id, area: 0, cost_sum: 0, first_cell: i as u32, min_x: x, min_y: y, max_x: x, max_y: y });
        }
        let id = by_root[root];
        region[i] = id;
        let r = &mut regions[id as usize - 1];
        r.area += 1;
        r.cost_sum += u64::from(cost[grid[i] as usize]);
        r.min_x = r.min_x.min(x);
        r.min_y = r.min_y.min(y);
        r.max_x = r.max_x.max(x);
        r.max_y = r.max_y.max(y);
    }

    // (3) the flow field: one multi-source Dijkstra with every node's cell as a source. Entering
    // a cell costs that cell's tile cost; zero-cost tiles are legal, which is why this is
    // Dijkstra and not a breadth-first walk. The frontier is keyed by (distance, cell index), so
    // the pop order is reproducible as well as the distances, which are unique regardless.
    let mut distance = vec![UNREACHABLE; cells];
    let mut frontier: BinaryHeap<Reverse<(u32, u32)>> = BinaryHeap::new();
    for n in &map.nodes {
        let i = map.cell_index(n.x, n.y);
        if distance[i] != 0 {
            distance[i] = 0;
            frontier.push(Reverse((0, i as u32)));
        }
    }
    while let Some(Reverse((d, i))) = frontier.pop() {
        let i = i as usize;
        if d > distance[i] {
            continue; // a stale copy of a cell already settled more cheaply
        }
        let (x, y) = (i % w, i / w);
        let mut neighbours = [usize::MAX; 4];
        neighbours[0] = if y > 0 { i - w } else { usize::MAX };
        neighbours[1] = if x > 0 { i - 1 } else { usize::MAX };
        neighbours[2] = if x + 1 < w { i + 1 } else { usize::MAX };
        neighbours[3] = if y + 1 < h { i + w } else { usize::MAX };
        for j in neighbours {
            if j == usize::MAX || !passable_tile[grid[j] as usize] {
                continue;
            }
            // Widened for the addition alone: MAX_FINITE_DISTANCE is below UNREACHABLE by the
            // const assertion at the top of this file, so the narrowing back to u32 is exact.
            let next = u64::from(d) + u64::from(cost[grid[j] as usize]);
            if next < u64::from(distance[j]) {
                debug_assert!(next <= MAX_FINITE_DISTANCE);
                distance[j] = next as u32;
                frontier.push(Reverse((next as u32, j as u32)));
            }
        }
    }

    let node_region = map.nodes.iter().map(|n| region[map.cell_index(n.x, n.y)]).collect();

    // (4) all-pairs cheapest directed cost over the named places (Floyd–Warshall). Saturating
    // addition keeps u64::MAX absorbing, so "no walk" propagates instead of wrapping.
    let n = map.nodes.len();
    const INF: u64 = u64::MAX;
    let mut d = vec![INF; n * n];
    for i in 0..n {
        d[i * n + i] = 0; // a place is at no cost from itself
    }
    for e in &map.edges {
        let i = node_index(&map.nodes, &e.from);
        let j = node_index(&map.nodes, &e.to);
        d[i * n + j] = u64::from(e.w);
    }
    for k in 0..n {
        for i in 0..n {
            let dik = d[i * n + k];
            if dik == INF {
                continue;
            }
            for j in 0..n {
                let through = dik.saturating_add(d[k * n + j]);
                if through < d[i * n + j] {
                    d[i * n + j] = through;
                }
            }
        }
    }
    let matrix = d
        .into_iter()
        .map(|v| {
            if v == INF {
                UNREACHABLE
            } else {
                debug_assert!(v <= MAX_FINITE_GRAPH_DISTANCE);
                v as u32
            }
        })
        .collect();

    Compiled { grid, region, regions, distance, node_region, matrix }
}

/// A node's index — its position in the canonical (ascending by id) node list, which is the row
/// and column it owns in the distance matrix. The grammar has already checked that every edge
/// endpoint is a node, so this is a lookup and not a search that can fail.
fn node_index(nodes: &[Node], id: &str) -> usize {
    nodes.binary_search_by(|n| n.id.as_str().cmp(id)).expect("the grammar refuses an edge whose endpoint is not a node")
}

// ─── the writer ────────────────────────────────────────────────────────────────────────────

/// `"MMAP"` ‖ version ‖ flags ‖ width ‖ height ‖ palette_count ‖ region_count ‖ node_count ‖
/// edge_count: four magic bytes, two `u16` and SIX `u32` — counted as six rather than added up
/// one at a time, because the version this replaces added five and was four bytes short of what
/// [`write_artifact`] emits (a `debug_assert` in [`derive_artifact`] is what said so).
const HEADER_LEN: usize = 4 + 2 + 2 + 6 * 4;
const PALETTE_ENTRY_LEN: usize = 1 + 1 + 2;
const REGION_ENTRY_LEN: usize = 4 + 4 + 8 + 4 + 2 + 2 + 2 + 2;
const EDGE_ENTRY_LEN: usize = 4 + 4 + 4;
const CRC_LEN: usize = 4;

/// The bytes one node occupies: its id, its position, its region, and its attributes.
fn node_entry_len(n: &Node) -> usize {
    4 + n.id.len() + 4 + 4 + 4 + 4 + n.attrs.keys().map(|k| 4 + k.len() + 8).sum::<usize>()
}

/// The artifact's exact length, from the map and its compilation. Nothing is padded and nothing
/// is aligned, so this is a plain sum of the sections in the order the writer emits them.
pub fn artifact_len(map: &Map, compiled: &Compiled) -> usize {
    let cells = map.cells();
    let n = map.nodes.len();
    HEADER_LEN
        + map.palette.len() * PALETTE_ENTRY_LEN
        + cells
        + 4 * cells
        + compiled.regions.len() * REGION_ENTRY_LEN
        + 4 * cells
        + map.nodes.iter().map(node_entry_len).sum::<usize>()
        + map.edges.len() * EDGE_ENTRY_LEN
        + 4 * n * n
        + CRC_LEN
}

/// An upper bound on [`artifact_len`] that needs only the DSL, so the ceiling can be checked
/// BEFORE a single cell is labelled. The one term the DSL does not fix is the region count, and
/// a grid of `c` cells cannot hold more than `⌈c/2⌉` 4-connected components (a checkerboard is
/// the extreme).
///
/// Saturating, not wrapping: this is a guard, and a guard that overflows into a small number is
/// a guard that opens. A [`Map`] the grammar produced is nowhere near the edge (`MAX_CELLS`
/// cells), but this function's job is to be right about the one a caller built by hand.
pub fn artifact_len_bound(map: &Map) -> usize {
    let cells = map.cells();
    let n = map.nodes.len();
    [
        HEADER_LEN,
        map.palette.len().saturating_mul(PALETTE_ENTRY_LEN),
        cells,
        cells.saturating_mul(4),
        cells.div_ceil(2).saturating_mul(REGION_ENTRY_LEN),
        cells.saturating_mul(4),
        map.nodes.iter().map(node_entry_len).sum::<usize>(),
        map.edges.len().saturating_mul(EDGE_ENTRY_LEN),
        n.saturating_mul(n).saturating_mul(4),
        CRC_LEN,
    ]
    .into_iter()
    .fold(0usize, usize::saturating_add)
}

/// The compiler's work in the unit [`MAX_COMPILE_STEPS`] documents, from the DSL alone — the
/// value SA-2's step ceiling is checked against before [`compile`] runs. Saturating, for the
/// reason [`artifact_len_bound`] gives.
pub fn compile_steps_bound(map: &Map) -> u64 {
    (map.cells() as u64).saturating_mul(STEPS_PER_CELL).saturating_add((map.nodes.len() as u64).saturating_pow(3))
}

/// Compile and write, refusing above either ceiling before the compiler runs (ADR-0078 SA-2:
/// "enforced before it runs"). Both quantities are functions of the DSL alone, so an oversized
/// map costs the executor — and every consumer verifying it — two multiplications and nothing
/// else. The steps are checked first because they bound the time and the bytes the space, and a
/// map that is refused should be refused by the cheaper wall.
pub fn derive_artifact(map: &Map) -> Result<Vec<u8>, DeriveError> {
    let steps = compile_steps_bound(map);
    if steps > MAX_COMPILE_STEPS {
        return Err(DeriveError::Transformer(format!(
            "compiling this map would take up to {steps} steps, above the {MAX_COMPILE_STEPS}-step ceiling"
        )));
    }
    let bound = artifact_len_bound(map);
    if bound > MAX_ARTIFACT_BYTES {
        return Err(DeriveError::Transformer(format!(
            "a map file of up to {bound} bytes exceeds the {MAX_ARTIFACT_BYTES}-byte ceiling"
        )));
    }
    let compiled = compile(map);
    let bytes = write_artifact(map, &compiled);
    debug_assert_eq!(bytes.len(), artifact_len(map, &compiled));
    Ok(bytes)
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    put_u32_le(out, s.len() as u32);
    out.extend_from_slice(s.as_bytes());
}

/// The canonical writer `misaka-map/1/canonical-v1` — the layout in the module doc, in that
/// order, little-endian, with no padding anywhere.
pub fn write_artifact(map: &Map, compiled: &Compiled) -> Vec<u8> {
    let mut out = Vec::with_capacity(artifact_len(map, compiled));
    out.extend_from_slice(MAGIC);
    put_u16_le(&mut out, ARTIFACT_VERSION);
    put_u16_le(&mut out, ARTIFACT_FLAGS);
    put_u32_le(&mut out, map.width);
    put_u32_le(&mut out, map.height);
    put_u32_le(&mut out, map.palette.len() as u32);
    put_u32_le(&mut out, compiled.regions.len() as u32);
    put_u32_le(&mut out, map.nodes.len() as u32);
    put_u32_le(&mut out, map.edges.len() as u32);
    for p in &map.palette {
        out.push(p.tile);
        out.push(u8::from(p.passable));
        put_u16_le(&mut out, p.cost as u16);
    }
    out.extend_from_slice(&compiled.grid);
    for r in &compiled.region {
        put_u32_le(&mut out, *r);
    }
    for r in &compiled.regions {
        put_u32_le(&mut out, r.id);
        put_u32_le(&mut out, r.area);
        put_u64_le(&mut out, r.cost_sum);
        put_u32_le(&mut out, r.first_cell);
        put_u16_le(&mut out, r.min_x);
        put_u16_le(&mut out, r.min_y);
        put_u16_le(&mut out, r.max_x);
        put_u16_le(&mut out, r.max_y);
    }
    for d in &compiled.distance {
        put_u32_le(&mut out, *d);
    }
    for (n, node_region) in map.nodes.iter().zip(&compiled.node_region) {
        put_str(&mut out, &n.id);
        put_u32_le(&mut out, n.x);
        put_u32_le(&mut out, n.y);
        put_u32_le(&mut out, *node_region);
        put_u32_le(&mut out, n.attrs.len() as u32);
        for (k, v) in &n.attrs {
            put_str(&mut out, k);
            put_i64_le(&mut out, *v);
        }
    }
    for e in &map.edges {
        put_u32_le(&mut out, node_index(&map.nodes, &e.from) as u32);
        put_u32_le(&mut out, node_index(&map.nodes, &e.to) as u32);
        put_u32_le(&mut out, e.w);
    }
    for m in &compiled.matrix {
        put_u32_le(&mut out, *m);
    }
    let crc = crc32(&out);
    put_u32_le(&mut out, crc);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::derive::{ClaimBinding, derive_named, derive_with, verify};
    use crate::ids::{artifact_hash_v1, dsl_hash_v1, grammar_id_v1};
    use kaspa_hashes::Hash64;
    use serde_json::{Value, json};
    use std::path::PathBuf;

    // ── builders ──────────────────────────────────────────────────────────────────────────

    /// The map every refusal case mutates: two regions' worth of grid, three tiles (one of them
    /// a wall), two named places and one directed edge.
    fn base() -> Value {
        json!({
            "v": 1, "width": 5, "height": 4, "default": 0,
            "palette": [
                {"tile": 0, "cost": 1, "passable": true},
                {"tile": 1, "cost": 0, "passable": false},
                {"tile": 2, "cost": 3, "passable": true}
            ],
            "runs": [[1, 1, 3, 1], [0, 3, 5, 2]],
            "nodes": [
                {"id": "start", "x": 0, "y": 0, "attrs": {"hp": 10}},
                {"id": "end", "x": 4, "y": 2, "attrs": {}}
            ],
            "edges": [{"from": "start", "to": "end", "w": 7}]
        })
    }

    fn bytes(v: &Value) -> Vec<u8> {
        serde_json::to_vec(v).unwrap()
    }

    fn canon(v: &Value) -> Vec<u8> {
        MapGrammar.canonicalize(&bytes(v)).unwrap()
    }

    fn map_of(v: &Value) -> Map {
        Map::from_canon(&parse_canonical(&canon(v)).unwrap()).unwrap()
    }

    fn artifact(v: &Value) -> Vec<u8> {
        MapCompilerTransformer.run(&canon(v)).unwrap().bytes
    }

    fn refusal(input: &[u8]) -> String {
        match MapGrammar.canonicalize(input) {
            Err(DeriveError::Grammar(m)) => m,
            other => panic!("expected a grammar refusal, got {other:?}"),
        }
    }

    /// `set(v, ["nodes", "0", "x"], json!(9))`: a numeric segment indexes an array, any other
    /// segment an object.
    fn set(mut v: Value, path: &[&str], to: Value) -> Value {
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
    }

    fn drop_key(mut v: Value, path: &[&str], key: &str) -> Value {
        let mut cur = &mut v;
        for p in path {
            cur = match p.parse::<usize>() {
                Ok(i) if cur.is_array() => &mut cur[i],
                _ => &mut cur[*p],
            };
        }
        cur.as_object_mut().unwrap().remove(key);
        v
    }

    // ── a reader, so the tests parse what the writer wrote ─────────────────────────────────

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
        fn i64(&mut self) -> i64 {
            i64::from_le_bytes(self.take(8).try_into().unwrap())
        }
        fn string(&mut self) -> String {
            let n = self.u32() as usize;
            String::from_utf8(self.take(n).to_vec()).unwrap()
        }
    }

    struct ParsedNode {
        id: String,
        x: u32,
        y: u32,
        region: u32,
        attrs: BTreeMap<String, i64>,
    }

    struct Parsed {
        width: u32,
        height: u32,
        palette: Vec<Tile>,
        grid: Vec<u8>,
        region: Vec<u32>,
        regions: Vec<Region>,
        distance: Vec<u32>,
        nodes: Vec<ParsedNode>,
        edges: Vec<(u32, u32, u32)>,
        matrix: Vec<u32>,
    }

    fn parse_artifact(a: &[u8]) -> Parsed {
        let mut r = Reader { b: a, pos: 0 };
        assert_eq!(r.take(4), MAGIC);
        assert_eq!(r.u16(), ARTIFACT_VERSION);
        assert_eq!(r.u16(), ARTIFACT_FLAGS);
        let width = r.u32();
        let height = r.u32();
        let palette_count = r.u32() as usize;
        let region_count = r.u32() as usize;
        let node_count = r.u32() as usize;
        let edge_count = r.u32() as usize;
        let cells = width as usize * height as usize;
        let palette = (0..palette_count)
            .map(|_| {
                let tile = r.u8();
                let passable = r.u8();
                assert!(passable <= 1, "passable is a 0/1 byte");
                Tile { tile, passable: passable == 1, cost: u32::from(r.u16()) }
            })
            .collect();
        let grid = r.take(cells).to_vec();
        let region = (0..cells).map(|_| r.u32()).collect();
        let regions = (0..region_count)
            .map(|_| Region {
                id: r.u32(),
                area: r.u32(),
                cost_sum: r.u64(),
                first_cell: r.u32(),
                min_x: r.u16(),
                min_y: r.u16(),
                max_x: r.u16(),
                max_y: r.u16(),
            })
            .collect();
        let distance = (0..cells).map(|_| r.u32()).collect();
        let nodes = (0..node_count)
            .map(|_| {
                let id = r.string();
                let (x, y, region, attr_count) = (r.u32(), r.u32(), r.u32(), r.u32() as usize);
                let attrs = (0..attr_count).map(|_| (r.string(), r.i64())).collect();
                ParsedNode { id, x, y, region, attrs }
            })
            .collect();
        let edges = (0..edge_count).map(|_| (r.u32(), r.u32(), r.u32())).collect();
        let matrix = (0..node_count * node_count).map(|_| r.u32()).collect();
        assert_eq!(r.pos, a.len() - CRC_LEN, "the reader consumed every section and nothing else");
        let crc = u32::from_le_bytes(a[a.len() - CRC_LEN..].try_into().unwrap());
        assert_eq!(crc, crc32(&a[..a.len() - CRC_LEN]), "trailing CRC-32");
        Parsed { width, height, palette, grid, region, regions, distance, nodes, edges, matrix }
    }

    // ── (1) canonicalization and the ORDER decision ────────────────────────────────────────

    #[test]
    fn canonicalization_sorts_every_declared_list_and_is_idempotent() {
        let once = canon(&base());
        assert_eq!(
            once,
            br#"{"default":0,"edges":[{"from":"start","to":"end","w":7}],"height":4,"nodes":[{"attrs":{},"id":"end","x":4,"y":2},{"attrs":{"hp":10},"id":"start","x":0,"y":0}],"palette":[{"cost":1,"passable":true,"tile":0},{"cost":0,"passable":false,"tile":1},{"cost":3,"passable":true,"tile":2}],"runs":[[1,1,3,1],[0,3,5,2]],"v":1,"width":5}"#
        );
        assert_eq!(MapGrammar.canonicalize(&once).unwrap(), once, "canonical bytes are a fixed point");
        let m = map_of(&base());
        assert_eq!(m.nodes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["end", "start"], "nodes ascend by id");
        assert_eq!(m.palette.iter().map(|p| p.tile).collect::<Vec<_>>(), [0, 1, 2], "the palette ascends by tile");
        assert_eq!(m.runs.iter().map(|r| (r.y, r.x)).collect::<Vec<_>>(), [(1, 1), (3, 0)], "runs ascend by (y, x)");
    }

    /// **The canonical-order pin.** Two spellings of ONE map — every list in a different order,
    /// every object's keys in a different order, whitespace everywhere — canonicalize to the same
    /// bytes and therefore compile to the same artifact, byte for byte. This is the answer the
    /// module doc's table states: the grammar SORTS, and the four refusals that make sorting safe
    /// (overlapping runs, duplicate tiles, duplicate ids, duplicate edges) are what keep an order
    /// from ever carrying meaning. (ADR-0078 X3: one DSL, one artifact — here, one MAP, one
    /// artifact, whatever order it arrived in.)
    #[test]
    fn x3_two_orderings_of_one_map_produce_the_same_dsl_and_the_same_artifact_bytes() {
        let forward = br#"{"v":1,"width":6,"height":4,"default":0,
            "palette":[{"tile":0,"cost":1,"passable":true},{"tile":1,"cost":0,"passable":false},{"tile":2,"cost":4,"passable":true}],
            "runs":[[0,0,2,2],[2,1,2,1],[4,3,2,2]],
            "nodes":[{"id":"a","x":0,"y":0,"attrs":{"kind":1,"tier":2}},{"id":"b","x":5,"y":0,"attrs":{}},{"id":"c","x":1,"y":3,"attrs":{"tier":9}}],
            "edges":[{"from":"a","to":"b","w":3},{"from":"b","to":"c","w":4},{"from":"a","to":"c","w":11}]}"#;
        let reversed = br#"  {  "edges" : [ {"w":11,"to":"c","from":"a"} , {"w":4,"to":"c","from":"b"} , {"w":3,"to":"b","from":"a"} ] ,
            "nodes" : [ {"attrs":{"tier":9},"y":3,"x":1,"id":"c"} , {"y":0,"x":0,"attrs":{"tier":2,"kind":1},"id":"a"} , {"id":"b","x":5,"y":0,"attrs":{}} ] ,
            "runs" : [ [4,3,2,2] , [2,1,2,1] , [0,0,2,2] ] ,
            "palette" : [ {"passable":true,"cost":4,"tile":2} , {"tile":0,"passable":true,"cost":1} , {"cost":0,"tile":1,"passable":false} ] ,
            "default" : 0 , "height" : 4 , "width" : 6 , "v" : 1 }  "#;
        let a = MapGrammar.canonicalize(forward).unwrap();
        let b = MapGrammar.canonicalize(reversed).unwrap();
        assert_eq!(a, b, "two orderings of one map must canonicalize to one byte string");
        let grammar_id = grammar_id_v1(GRAMMAR_NAME);
        assert_eq!(dsl_hash_v1(&grammar_id, &a), dsl_hash_v1(&grammar_id, &b));
        let art_a = MapCompilerTransformer.run(&a).unwrap().bytes;
        let art_b = MapCompilerTransformer.run(&b).unwrap().bytes;
        assert_eq!(art_a, art_b, "one map, one artifact");
        assert_eq!(artifact_hash_v1(&art_a), artifact_hash_v1(&art_b));
        // And the node order the artifact uses is the DSL's canonical one, so the matrix rows
        // are addressed by ascending id however the answer listed them.
        let p = parse_artifact(&art_a);
        assert_eq!(p.nodes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
        assert_eq!(p.edges, vec![(0, 1, 3), (0, 2, 11), (1, 2, 4)], "edges ascend by (from, to) index");
    }

    /// Deriving the same answer twice is the same artifact, and so is deriving it through the
    /// registry by name — the other half of "byte-identical" that a two-architecture drill
    /// completes (X3).
    #[test]
    fn x3_the_derivation_is_reproducible_and_declares_integer_arithmetic() {
        let first = artifact(&base());
        assert_eq!(artifact(&base()), first);
        let binding = binding();
        let a = derive_named(TRANSFORMER_NAME, &binding, &bytes(&base())).unwrap();
        let b = derive_named(TRANSFORMER_NAME, &binding, &canon(&base())).unwrap();
        assert_eq!(a.artifact.bytes, first);
        assert_eq!(b.artifact.bytes, first);
        assert_eq!(a.artifact_hash, b.artifact_hash);
        assert_eq!(MapCompilerTransformer.manifest().discipline, Discipline::Integer);
    }

    // ── (2) every schema refusal, each named distinctly (X4) ───────────────────────────────

    #[test]
    fn x4_every_schema_refusal_is_named_distinctly() {
        let b = base;
        let mut many_palette = b();
        many_palette["palette"] =
            (0..=MAX_PALETTE).map(|i| json!({"tile": i % 256, "cost": 1, "passable": true})).collect::<Vec<_>>().into();
        let mut many_runs = b();
        many_runs["runs"] = (0..=MAX_RUNS).map(|_| json!([0, 0, 1, 0])).collect::<Vec<_>>().into();
        let mut many_nodes = b();
        many_nodes["nodes"] =
            (0..=MAX_NODES).map(|i| json!({"id": format!("n{i}"), "x": 0, "y": 0, "attrs": {}})).collect::<Vec<_>>().into();
        let mut many_edges = b();
        many_edges["edges"] = (0..=MAX_EDGES).map(|_| json!({"from": "start", "to": "end", "w": 1})).collect::<Vec<_>>().into();
        let mut many_attrs = b();
        many_attrs["nodes"][0]["attrs"] =
            (0..=MAX_ATTRS).map(|i| (format!("k{i}"), json!(i))).collect::<serde_json::Map<_, _>>().into();
        let mut two_edges = b();
        two_edges["edges"] = json!([{"from": "start", "to": "end", "w": 7}, {"from": "start", "to": "end", "w": 2}]);

        let cases: Vec<(&str, Vec<u8>, &str)> = vec![
            ("not an object", b"[1]".to_vec(), "the map must be a JSON object"),
            ("unknown top key", bytes(&set(b(), &["biome"], json!("ice"))), "the map has unknown key"),
            ("missing top key", bytes(&drop_key(b(), &[], "runs")), "the map is missing key"),
            ("v", bytes(&set(b(), &["v"], json!(2))), "v must be 1"),
            ("width", bytes(&set(b(), &["width"], json!(0))), "width must be an integer in 1..=1024"),
            ("height", bytes(&set(b(), &["height"], json!(1025))), "height must be an integer in 1..=1024"),
            (
                "too many cells",
                bytes(&set(set(b(), &["width"], json!(1024)), &["height"], json!(1024))),
                "cells; the grammar admits 262144",
            ),
            ("default range", bytes(&set(b(), &["default"], json!(256))), "default must be an integer in 0..=255"),
            ("palette not array", bytes(&set(b(), &["palette"], json!({}))), "palette must be an array"),
            ("palette empty", bytes(&set(b(), &["palette"], json!([]))), "palette is empty"),
            ("too many palette entries", bytes(&many_palette), "more than 256 palette entries"),
            ("palette entry not object", bytes(&set(b(), &["palette", "0"], json!(3))), "a palette entry must be an object"),
            (
                "palette entry unknown key",
                bytes(&set(b(), &["palette", "0", "slippery"], json!(true))),
                "a palette entry has unknown key",
            ),
            ("palette entry missing key", bytes(&drop_key(b(), &["palette", "0"], "cost")), "a palette entry is missing key"),
            (
                "palette tile",
                bytes(&set(b(), &["palette", "0", "tile"], json!(-1))),
                "a palette entry's tile must be an integer in 0..=255",
            ),
            (
                "palette cost",
                bytes(&set(b(), &["palette", "0", "cost"], json!(8192))),
                "a palette entry's cost must be an integer in 0..=8191",
            ),
            (
                "passable not bool",
                bytes(&set(b(), &["palette", "0", "passable"], json!(1))),
                "a palette entry's passable must be a boolean",
            ),
            ("duplicate tile", bytes(&set(b(), &["palette", "1", "tile"], json!(0))), "tile 0 has two palette entries"),
            ("default not in palette", bytes(&set(b(), &["default"], json!(9))), "the default tile 9 has no palette entry"),
            ("run tile not in palette", bytes(&set(b(), &["runs", "0"], json!([0, 0, 1, 7]))), "a run paints tile 7 at (0, 0)"),
            ("runs not array", bytes(&set(b(), &["runs"], json!("none"))), "runs must be an array"),
            ("too many runs", bytes(&many_runs), "more than 65536 runs"),
            ("run not a quad", bytes(&set(b(), &["runs", "0"], json!([0, 0, 1]))), "a run must be an [x, y, len, tile] quad"),
            ("run x", bytes(&set(b(), &["runs", "0"], json!([5, 0, 1, 0]))), "a run's x must be an integer in 0..=4"),
            ("run y", bytes(&set(b(), &["runs", "0"], json!([0, 4, 1, 0]))), "a run's y must be an integer in 0..=3"),
            ("run len", bytes(&set(b(), &["runs", "0"], json!([1, 1, 9, 1]))), "a run's len must be an integer in 1..=4"),
            ("run tile", bytes(&set(b(), &["runs", "0"], json!([0, 0, 1, 300]))), "a run's tile must be an integer in 0..=255"),
            ("overlapping runs", bytes(&set(b(), &["runs", "0"], json!([0, 3, 2, 1]))), "overlaps the run at"),
            ("nodes not array", bytes(&set(b(), &["nodes"], json!(7))), "nodes must be an array"),
            ("too many nodes", bytes(&many_nodes), "more than 256 nodes"),
            ("node not object", bytes(&set(b(), &["nodes", "0"], json!("start"))), "a node must be an object"),
            ("node unknown key", bytes(&set(b(), &["nodes", "0", "level"], json!(3))), "a node has unknown key"),
            ("node missing key", bytes(&drop_key(b(), &["nodes", "0"], "y")), "a node is missing key"),
            ("node id", bytes(&set(b(), &["nodes", "0", "id"], json!(""))), "a node's id must be a string of 1..=64 bytes"),
            ("node x", bytes(&set(b(), &["nodes", "0", "x"], json!(5))), "a node's x must be an integer in 0..=4"),
            ("node y", bytes(&set(b(), &["nodes", "0", "y"], json!(-1))), "a node's y must be an integer in 0..=3"),
            ("duplicate node id", bytes(&set(b(), &["nodes", "1", "id"], json!("start"))), "node id \"start\" is used twice"),
            ("attrs not object", bytes(&set(b(), &["nodes", "0", "attrs"], json!([]))), "a node's attrs must be an object"),
            ("too many attrs", bytes(&many_attrs), "more than 16 attrs"),
            ("attr key empty", bytes(&set(b(), &["nodes", "0", "attrs"], json!({"": 1}))), "an attr key must be 1..=64 bytes"),
            (
                "attr value string",
                bytes(&set(b(), &["nodes", "0", "attrs"], json!({"hp": "full"}))),
                "must be an integer in i64 range",
            ),
            (
                "node on a wall",
                bytes(&set(set(b(), &["nodes", "0", "x"], json!(1)), &["nodes", "0", "y"], json!(1))),
                "which the palette marks impassable",
            ),
            ("edges not array", bytes(&set(b(), &["edges"], json!({}))), "edges must be an array"),
            ("too many edges", bytes(&many_edges), "more than 4096 edges"),
            ("edge not object", bytes(&set(b(), &["edges", "0"], json!([1, 2]))), "an edge must be an object"),
            ("edge unknown key", bytes(&set(b(), &["edges", "0", "toll"], json!(1))), "an edge has unknown key"),
            ("edge missing key", bytes(&drop_key(b(), &["edges", "0"], "w")), "an edge is missing key"),
            ("edge from", bytes(&set(b(), &["edges", "0", "from"], json!(4))), "an edge's from must be a string of 1..=64 bytes"),
            ("edge names a stranger", bytes(&set(b(), &["edges", "0", "to"], json!("nowhere"))), "which is not a node"),
            ("edge weight", bytes(&set(b(), &["edges", "0", "w"], json!(1_000_001))), "an edge's w must be an integer in 0..=1000000"),
            ("self loop", bytes(&set(b(), &["edges", "0", "to"], json!("start"))), "to itself"),
            ("duplicate edge", bytes(&two_edges), "two edges run from"),
            ("float anywhere", br#"{"v":1,"width":1.5}"#.to_vec(), "non-integer number"),
            ("duplicate json key", br#"{"v":1,"v":1}"#.to_vec(), "duplicate key"),
        ];
        let mut fragments = std::collections::BTreeSet::new();
        for (label, input, fragment) in &cases {
            let msg = refusal(input);
            assert!(msg.contains(fragment), "{label}: refusal {msg:?} does not carry {fragment:?}");
            assert!(fragments.insert(*fragment), "{label}: fragment {fragment:?} is not distinct");
        }
        assert_eq!(fragments.len(), cases.len());
        assert!(!canon(&base()).is_empty(), "the base every case mutates is itself accepted");
    }

    /// X4 in the words the invariant uses: a parse failure produces NO object. `derive_named`
    /// returns the grammar's refusal and nothing else — no `Derivation`, so no `dsl_hash`, no
    /// `artifact_hash` and nothing for a caller to submit.
    #[test]
    fn x4_a_grammar_refusal_produces_no_object() {
        let broken = bytes(&set(base(), &["runs", "0"], json!([0, 3, 2, 1]))); // overlapping runs
        match derive_named(TRANSFORMER_NAME, &binding(), &broken) {
            Err(DeriveError::Grammar(m)) => assert!(m.contains("overlaps the run at"), "{m}"),
            other => panic!("expected a grammar refusal with no object, got {other:?}"),
        }
        // The same bytes handed straight to the transformer refuse in the same place, so there is
        // no path around the grammar into a compilation.
        assert!(matches!(MapCompilerTransformer.run(&broken), Err(DeriveError::Grammar(_))));
    }

    #[test]
    fn the_transformer_refuses_non_canonical_input() {
        let pretty = serde_json::to_vec_pretty(&base()).unwrap();
        match MapCompilerTransformer.run(&pretty) {
            Err(DeriveError::Transformer(m)) => assert!(m.contains("not canonical")),
            other => panic!("{other:?}"),
        }
        // Canonical JSON that is not canonical map/v1 — the lists are unsorted — is refused too.
        let unsorted = br#"{"default":0,"edges":[],"height":1,"nodes":[],"palette":[{"cost":1,"passable":true,"tile":1},{"cost":1,"passable":true,"tile":0}],"runs":[],"v":1,"width":1}"#;
        match MapCompilerTransformer.run(unsorted) {
            Err(DeriveError::Transformer(m)) => assert!(m.contains("not canonical")),
            other => panic!("{other:?}"),
        }
        assert!(matches!(MapCompilerTransformer.run(b"{}"), Err(DeriveError::Grammar(_))));
    }

    // ── (3) the compiler, against hand computations ────────────────────────────────────────

    /// A wall down the middle of a 5x3 grid is two regions, numbered by their first cell.
    #[test]
    fn region_labelling_matches_the_hand_computation() {
        let v = json!({
            "v": 1, "width": 5, "height": 3, "default": 0,
            "palette": [{"tile": 0, "cost": 1, "passable": true}, {"tile": 1, "cost": 0, "passable": false}],
            "runs": [[2, 0, 1, 1], [2, 1, 1, 1], [2, 2, 1, 1]],
            "nodes": [], "edges": []
        });
        let m = map_of(&v);
        let c = compile(&m);
        assert_eq!(c.region, vec![1, 1, 0, 2, 2, 1, 1, 0, 2, 2, 1, 1, 0, 2, 2]);
        assert_eq!(c.regions.len(), 2);
        assert_eq!(c.regions[0], Region { id: 1, area: 6, cost_sum: 6, first_cell: 0, min_x: 0, min_y: 0, max_x: 1, max_y: 2 });
        assert_eq!(c.regions[1], Region { id: 2, area: 6, cost_sum: 6, first_cell: 3, min_x: 3, min_y: 0, max_x: 4, max_y: 2 });
        // Region 0 is the wall, and it is never in the table.
        assert!(c.regions.iter().all(|r| r.id != IMPASSABLE_REGION));
        assert_eq!(c.region.iter().filter(|r| **r == IMPASSABLE_REGION).count(), 3);
        // Diagonal neighbours are NOT connected (the "not covered" line about 4-connectivity):
        // a checkerboard of two tiles is one region per passable cell.
        let checker = json!({
            "v": 1, "width": 3, "height": 3, "default": 1,
            "palette": [{"tile": 0, "cost": 1, "passable": true}, {"tile": 1, "cost": 0, "passable": false}],
            "runs": [[0, 0, 1, 0], [2, 0, 1, 0], [1, 1, 1, 0], [0, 2, 1, 0], [2, 2, 1, 0]],
            "nodes": [], "edges": []
        });
        let cc = compile(&map_of(&checker));
        assert_eq!(cc.regions.len(), 5, "five passable cells, no two of them 4-adjacent");
        assert_eq!(cc.region, vec![1, 0, 2, 0, 3, 0, 4, 0, 5]);
        assert!(cc.regions.len() <= 9usize.div_ceil(2), "the bound artifact_len_bound assumes: at most ⌈cells/2⌉ regions");
    }

    /// The flow field is a cheapest-cost field, not a hop count: the detour with four cheap
    /// steps beats the one expensive step, which is what makes this Dijkstra and not a
    /// breadth-first walk.
    #[test]
    fn the_distance_field_is_a_cheapest_cost_field_not_a_hop_count() {
        let v = json!({
            "v": 1, "width": 3, "height": 2, "default": 0,
            "palette": [{"tile": 0, "cost": 1, "passable": true}, {"tile": 5, "cost": 10, "passable": true}],
            "runs": [[1, 0, 1, 5]],
            "nodes": [{"id": "a", "x": 0, "y": 0, "attrs": {}}], "edges": []
        });
        let c = compile(&map_of(&v));
        // row 0: a(0) toll(10) far ; row 1: 1 2 3 — so (2,0) is 4 the long way, not 11.
        assert_eq!(c.distance, vec![0, 10, 4, 1, 2, 3]);
        // A cell no node can reach keeps the sentinel, and so does every wall.
        let walled = json!({
            "v": 1, "width": 4, "height": 1, "default": 0,
            "palette": [{"tile": 0, "cost": 1, "passable": true}, {"tile": 1, "cost": 0, "passable": false}],
            "runs": [[2, 0, 1, 1]],
            "nodes": [{"id": "a", "x": 0, "y": 0, "attrs": {}}], "edges": []
        });
        let w = compile(&map_of(&walled));
        assert_eq!(w.distance, vec![0, 1, UNREACHABLE, UNREACHABLE]);
        // With no nodes at all, nothing is reachable — and that is a value, not a failure.
        let none = json!({
            "v": 1, "width": 2, "height": 1, "default": 0,
            "palette": [{"tile": 0, "cost": 1, "passable": true}], "runs": [], "nodes": [], "edges": []
        });
        assert_eq!(compile(&map_of(&none)).distance, vec![UNREACHABLE, UNREACHABLE]);
        // Two nodes: every cell takes the nearer of them.
        let two = json!({
            "v": 1, "width": 5, "height": 1, "default": 0,
            "palette": [{"tile": 0, "cost": 1, "passable": true}], "runs": [],
            "nodes": [{"id": "a", "x": 0, "y": 0, "attrs": {}}, {"id": "b", "x": 4, "y": 0, "attrs": {}}], "edges": []
        });
        assert_eq!(compile(&map_of(&two)).distance, vec![0, 1, 2, 1, 0]);
    }

    /// The route matrix is directed, transitive and honest about what has no route.
    #[test]
    fn the_graph_distance_matrix_matches_the_hand_computation() {
        let v = json!({
            "v": 1, "width": 4, "height": 1, "default": 0,
            "palette": [{"tile": 0, "cost": 1, "passable": true}],
            "runs": [],
            "nodes": [
                {"id": "a", "x": 0, "y": 0, "attrs": {}}, {"id": "b", "x": 1, "y": 0, "attrs": {}},
                {"id": "c", "x": 2, "y": 0, "attrs": {}}, {"id": "d", "x": 3, "y": 0, "attrs": {}}
            ],
            "edges": [
                {"from": "a", "to": "b", "w": 5}, {"from": "b", "to": "c", "w": 2},
                {"from": "a", "to": "c", "w": 100}, {"from": "c", "to": "a", "w": 1}
            ]
        });
        let c = compile(&map_of(&v));
        let u = UNREACHABLE;
        #[rustfmt::skip]
        let expected = vec![
            0, 5, 7, u, // a: a→c is 7 through b, never the 100 the DSL also offers
            3, 0, 2, u, // b: b→a is 2 + 1
            1, 6, 0, u, // c
            u, u, u, 0, // d has no edges at all
        ];
        assert_eq!(c.matrix, expected);
        // Zero-weight edges are legal and do not confuse the sentinel.
        let zero = json!({
            "v": 1, "width": 2, "height": 1, "default": 0,
            "palette": [{"tile": 0, "cost": 1, "passable": true}], "runs": [],
            "nodes": [{"id": "a", "x": 0, "y": 0, "attrs": {}}, {"id": "b", "x": 1, "y": 0, "attrs": {}}],
            "edges": [{"from": "a", "to": "b", "w": 0}]
        });
        assert_eq!(compile(&map_of(&zero)).matrix, vec![0, 0, UNREACHABLE, 0]);
        // No nodes: an empty matrix, not a one-entry one.
        let none = json!({
            "v": 1, "width": 1, "height": 1, "default": 0,
            "palette": [{"tile": 0, "cost": 1, "passable": true}], "runs": [], "nodes": [], "edges": []
        });
        assert!(compile(&map_of(&none)).matrix.is_empty());
    }

    // ── (4) the artifact ───────────────────────────────────────────────────────────────────

    fn check_structure(v: &Value) {
        let m = map_of(v);
        let c = compile(&m);
        let a = artifact(v);
        assert_eq!(a.len(), artifact_len(&m, &c), "artifact_len predicts the writer exactly");
        assert!(a.len() <= artifact_len_bound(&m), "the DSL-only bound is an upper bound");
        let p = parse_artifact(&a);
        assert_eq!((p.width, p.height), (m.width, m.height));
        assert_eq!(p.palette, m.palette);
        assert_eq!(p.grid, c.grid);
        assert_eq!(p.region, c.region);
        assert_eq!(p.regions, c.regions);
        assert_eq!(p.distance, c.distance);
        assert_eq!(p.matrix, c.matrix);
        assert_eq!(p.nodes.len(), m.nodes.len());
        for (i, (pn, n)) in p.nodes.iter().zip(&m.nodes).enumerate() {
            assert_eq!((pn.id.as_str(), pn.x, pn.y), (n.id.as_str(), n.x, n.y));
            assert_eq!(pn.attrs, n.attrs);
            assert_eq!(pn.region, c.node_region[i]);
            assert_ne!(pn.region, IMPASSABLE_REGION, "the grammar refuses a node on a wall");
            assert_eq!(c.distance[m.cell_index(n.x, n.y)], 0, "a node's own cell is at distance 0");
        }
        assert_eq!(p.edges.len(), m.edges.len());
        for ((from, to, w), e) in p.edges.iter().zip(&m.edges) {
            assert_eq!(p.nodes[*from as usize].id, e.from);
            assert_eq!(p.nodes[*to as usize].id, e.to);
            assert_eq!(*w, e.w);
            // An edge's own weight is an upper bound on the route it names.
            assert!(c.matrix[*from as usize * m.nodes.len() + *to as usize] <= *w);
        }
        // Two cells are mutually reachable iff they share a non-zero region — the property the
        // region layer exists to answer.
        for (i, r) in c.region.iter().enumerate() {
            let reachable = c.distance[i] != UNREACHABLE;
            let has_node_in_region = m.nodes.iter().any(|n| c.region[m.cell_index(n.x, n.y)] == *r);
            assert_eq!(reachable, *r != IMPASSABLE_REGION && has_node_in_region, "cell {i}");
        }
    }

    #[test]
    fn the_artifact_parses_and_every_section_recomputes() {
        check_structure(&base());
        check_structure(&json!({
            "v": 1, "width": 9, "height": 7, "default": 0,
            "palette": [
                {"tile": 0, "cost": 1, "passable": true}, {"tile": 1, "cost": 0, "passable": false},
                {"tile": 2, "cost": 5, "passable": true}, {"tile": 3, "cost": 8191, "passable": true}
            ],
            "runs": [[0, 3, 9, 1], [4, 0, 1, 1], [4, 6, 1, 3], [2, 5, 4, 2]],
            "nodes": [
                {"id": "north", "x": 0, "y": 0, "attrs": {"tier": -3, "big": i64::MAX}},
                {"id": "south", "x": 8, "y": 6, "attrs": {}}
            ],
            "edges": [{"from": "north", "to": "south", "w": 1000000}, {"from": "south", "to": "north", "w": 0}]
        }));
        // A map with no graph at all still writes every section, just empty ones.
        check_structure(&json!({
            "v": 1, "width": 3, "height": 3, "default": 0,
            "palette": [{"tile": 0, "cost": 2, "passable": true}], "runs": [], "nodes": [], "edges": []
        }));
    }

    // ── (4b) the three declared bounds (ADR-0078 SA-2) ─────────────────────────────────────

    /// `max_dsl_bytes`, on the byte count, BEFORE the parser: the boundary itself is admitted and
    /// one byte more is refused — and the refusal names the byte count rather than anything the
    /// bytes spell, which is what proves the ceiling ran first (a parser reaching this input
    /// would have said "trailing characters", not "4194305 bytes").
    #[test]
    fn sa2_the_dsl_ceiling_refuses_on_the_byte_count_before_the_parser_runs() {
        // A valid map, padded with whitespace to exactly the ceiling: still one map, still
        // derives, and its artifact is the unpadded map's artifact.
        let mut at_the_line = bytes(&base());
        at_the_line.resize(MAX_DSL_BYTES, b' ');
        assert_eq!(at_the_line.len(), MAX_DSL_BYTES);
        assert_eq!(MapGrammar.canonicalize(&at_the_line).unwrap(), canon(&base()));
        assert_eq!(
            derive_named(TRANSFORMER_NAME, &binding(), &at_the_line).unwrap().artifact.bytes,
            artifact(&base()),
            "padding is whitespace; whitespace is what a canonicalizer removes"
        );

        let mut over = at_the_line.clone();
        over.push(b' ');
        for (what, msg) in [
            ("the grammar", refusal(&over)),
            ("the transformer", {
                match MapCompilerTransformer.run(&over) {
                    Err(DeriveError::Grammar(m)) => m,
                    other => panic!("expected the transformer to enforce its own bound, got {other:?}"),
                }
            }),
        ] {
            assert!(msg.contains(&format!("a DSL of {} bytes", over.len())), "{what}: {msg}");
            assert!(msg.contains(&MAX_DSL_BYTES.to_string()), "{what}: {msg}");
        }
        // And no object comes of it (X4), by the path a gateway actually takes.
        assert!(matches!(derive_named(TRANSFORMER_NAME, &binding(), &over), Err(DeriveError::Grammar(_))));
        // Bytes that are not even UTF-8 are refused on the count too, not on their contents:
        // the order of the two checks is the whole point of the bound.
        let mut garbage = vec![0xFFu8; MAX_DSL_BYTES + 1];
        garbage[0] = b'{';
        assert!(refusal(&garbage).contains("exceeds the"));
    }

    /// `max_steps` and `max_artifact_bytes`, each checked before a cell is labelled, and each
    /// tripped on its own by a map that is under the other's ceiling — so neither guard is dead
    /// code hiding behind the other. Both maps are built by hand: no map the grammar admits can
    /// reach either ceiling, which the next test measures.
    #[test]
    fn sa2_both_compiler_ceilings_are_checked_before_a_cell_is_labelled() {
        // 4096×4096 = 16.7M cells: over the step ceiling first, and the steps are checked first
        // because they are the cheaper wall.
        let mut vast = map_of(&base());
        vast.width = 4096;
        vast.height = 4096;
        assert!(compile_steps_bound(&vast) > MAX_COMPILE_STEPS);
        match derive_artifact(&vast) {
            Err(DeriveError::Transformer(m)) => assert!(m.contains("step ceiling"), "{m}"),
            other => panic!("{other:?}"),
        }
        // 1000×800 = 800,000 cells: under the step ceiling, over the artifact ceiling — the
        // second guard, reached only because the first one let it through.
        let mut wide = map_of(&base());
        wide.width = 1000;
        wide.height = 800;
        assert!(compile_steps_bound(&wide) <= MAX_COMPILE_STEPS, "this case must get past the step guard");
        assert!(artifact_len_bound(&wide) > MAX_ARTIFACT_BYTES);
        match derive_artifact(&wide) {
            Err(DeriveError::Transformer(m)) => assert!(m.contains("byte ceiling"), "{m}"),
            other => panic!("{other:?}"),
        }
        // A guard that overflows into a small number is a guard that opens (the saturating
        // arithmetic): the largest map a `u32` width and height can spell is still refused.
        let mut absurd = map_of(&base());
        absurd.width = u32::MAX;
        absurd.height = u32::MAX;
        assert_eq!(compile_steps_bound(&absurd), u64::MAX);
        assert_eq!(artifact_len_bound(&absurd), usize::MAX);
        assert!(matches!(derive_artifact(&absurd), Err(DeriveError::Transformer(_))));
    }

    /// The largest map the GRAMMAR admits, built at every bound at once, and measured against all
    /// three declared ceilings. This is why the two compiler ceilings are unreachable from any
    /// answer: the grammar's own bounds are their first face. The step ceiling is not merely
    /// above this map — it IS this map, which is how it was derived.
    #[test]
    fn sa2_the_largest_map_the_grammar_admits_is_under_every_declared_bound() {
        let widest = "w".repeat(MAX_NAME_BYTES);
        let palette: Vec<Tile> = (0..MAX_PALETTE).map(|t| Tile { tile: t as u8, cost: MAX_TILE_COST, passable: true }).collect();
        // 1024 × 256 = MAX_CELLS, with the widest coordinates a run can carry.
        let (width, height) = (MAX_SIDE, MAX_CELLS as u32 / MAX_SIDE);
        let runs: Vec<TileRun> =
            (0..MAX_RUNS as u32).map(|i| TileRun { x: i % width, y: i / width, len: 1, tile: (i % 256) as u8 }).collect();
        let nodes: Vec<Node> = (0..MAX_NODES)
            // The digits lead so that the ids are distinct AND ascending in the order they are
            // built, which is the canonical order the grammar demands.
            .map(|i| Node {
                id: format!("{i:04}{widest}")[..MAX_NAME_BYTES].to_string(),
                x: width - 1,
                y: height - 1,
                attrs: (0..MAX_ATTRS)
                    .map(|a| (format!("{a:04}{widest}")[..MAX_NAME_BYTES].to_string(), i64::MIN + a as i64))
                    .collect(),
            })
            .collect();
        let mut edges = Vec::new();
        'outer: for from in &nodes {
            for to in &nodes {
                if from.id != to.id {
                    edges.push(Edge { from: from.id.clone(), to: to.id.clone(), w: MAX_EDGE_WEIGHT as u32 });
                    if edges.len() == MAX_EDGES {
                        break 'outer;
                    }
                }
            }
        }
        let biggest = Map { width, height, default: 0, palette, runs, nodes, edges };
        assert_eq!(biggest.cells() as u64, MAX_CELLS);

        // (a) it is a map this grammar really admits — canonical bytes in, the same map out.
        let dsl = write_canonical(&biggest.to_canon());
        // `cargo test -- --nocapture sa2_the_largest` prints the three measurements, so a reader
        // re-deriving the ceilings need not multiply the bounds out by hand.
        println!(
            "the grammar's largest map: {} DSL bytes (ceiling {MAX_DSL_BYTES}), {} steps (ceiling {MAX_COMPILE_STEPS}), \
             {} artifact bytes (ceiling {MAX_ARTIFACT_BYTES})",
            dsl.len(),
            compile_steps_bound(&biggest),
            artifact_len_bound(&biggest)
        );
        assert_eq!(Map::from_canon(&parse_canonical(&dsl).unwrap()).unwrap(), biggest, "the worst case is a real map");
        // (b) max_dsl_bytes holds it, with room to spare.
        assert!(dsl.len() < MAX_DSL_BYTES, "the grammar's largest DSL is {} bytes", dsl.len());
        assert!(MapGrammar.canonicalize(&dsl).is_ok());
        // (c) max_steps is exactly this map's cost — the ceiling was derived from the grammar,
        // not chosen — so nothing the grammar admits exceeds it.
        assert_eq!(compile_steps_bound(&biggest), MAX_COMPILE_STEPS);
        // (d) max_artifact_bytes holds it too, at the pessimistic (checkerboard) region count.
        assert!(
            artifact_len_bound(&biggest) < MAX_ARTIFACT_BYTES,
            "the grammar's largest map is {} bytes",
            artifact_len_bound(&biggest)
        );
        // And the bounds a caller reads are the constants these assertions used.
        assert_eq!(
            BOUNDS,
            DeclaredBounds {
                max_dsl_bytes: MAX_DSL_BYTES as u64,
                max_artifact_bytes: MAX_ARTIFACT_BYTES as u64,
                max_steps: MAX_COMPILE_STEPS
            }
        );
    }

    // ── (5) the corpus and its golden (X6) ─────────────────────────────────────────────────

    fn corpus_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("corpus").join("map")
    }

    /// Every `corpus/map/*.json` sample, by file name, `golden.json` excluded. A name carrying
    /// `-refused-` is a sample this kind must REFUSE; the golden pins the message, because WHICH
    /// wall a sample hits is the thing under test — a bound-exhausting sample that was refused
    /// for some other reason would prove nothing about the bound (ADR-0078 SA-2).
    fn corpus_files() -> Vec<(String, Vec<u8>)> {
        let mut files: Vec<(String, Vec<u8>)> = std::fs::read_dir(corpus_dir())
            .expect("corpus/map exists")
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
            .filter(|p| p.file_name().and_then(|n| n.to_str()) != Some("golden.json"))
            .map(|p| (p.file_name().unwrap().to_str().unwrap().to_string(), std::fs::read(&p).unwrap()))
            .collect();
        files.sort();
        assert!(files.len() >= 5, "the corpus holds at least five samples: {files:?}");
        files
    }

    fn binding() -> ClaimBinding {
        ClaimBinding {
            network_domain: Hash64::from_bytes([0x11; 64]),
            claim_id: Hash64::from_bytes([0x22; 64]),
            output_root: Hash64::from_bytes([0x33; 64]),
            executor_pubkey: vec![0x44; 2592],
        }
    }

    /// What the golden holds for one sample: the derivation's three pinned values, or the
    /// grammar's refusal.
    fn corpus_entry(answer: &[u8], name: &str) -> Value {
        match derive_with(&MapGrammar, &MapCompilerTransformer, &binding(), answer) {
            Ok(d) => {
                assert_eq!(d.kind, kind::MAP);
                assert_eq!(d.grammar_id, grammar_id_v1(GRAMMAR_NAME));
                assert_eq!(d.dsl_hash, dsl_hash_v1(&d.grammar_id, &d.canonical_dsl));
                assert_eq!(d.artifact_hash, artifact_hash_v1(&d.artifact.bytes));
                assert_eq!(d.artifact.media_type, MEDIA_TYPE);
                assert_eq!(d.artifact.extension, EXTENSION);
                assert_eq!(d.object.artifact_bytes, d.artifact.bytes.len() as u64);
                // X6: the consumer's own path, from the answer and the object alone.
                let v = verify(&d.object, answer).unwrap();
                assert!(v.all_match(), "{name}: {v:?}");
                // and the canonical bytes are a fixed point of the grammar
                assert_eq!(MapGrammar.canonicalize(&d.canonical_dsl).unwrap(), d.canonical_dsl);
                json!({
                    "dsl_hash": d.dsl_hash.to_string(),
                    "artifact_hash": d.artifact_hash.to_string(),
                    "artifact_bytes": d.artifact.bytes.len(),
                })
            }
            // Any refusal, in the words the caller would see: `DeriveError`'s own Display names
            // the arm (`grammar:` / `transformer:`), so the golden pins which wall was hit.
            Err(e) => json!({ "refused": e.to_string() }),
        }
    }

    #[test]
    fn x6_corpus_matches_golden_and_verifies_through_the_registry() {
        let golden: BTreeMap<String, Value> =
            serde_json::from_slice(&std::fs::read(corpus_dir().join("golden.json")).expect("golden.json")).unwrap();
        let mut actual: BTreeMap<String, Value> = BTreeMap::new();
        for (name, answer) in corpus_files() {
            let entry = corpus_entry(&answer, &name);
            if name.contains("-refused-") {
                assert!(entry.get("refused").is_some(), "{name} is named as a refusal but derived");
            } else {
                assert!(entry.get("artifact_hash").is_some(), "{name} did not derive: {entry}");
            }
            actual.insert(name, entry);
        }
        assert_eq!(golden, actual, "corpus/map/golden.json moved; a new grammar or writer is a new name, not an edit");
        // The two orderings of one map are two files with one pair of hashes — the ordering
        // decision, pinned in the corpus and not only in a unit test.
        assert_eq!(actual["03-order-forward.json"], actual["04-order-shuffled.json"]);
    }

    /// Regenerate the golden: `MISAKA_PALW_POW_FIXTURE=1 cargo test -p misaka-palw-derive
    /// print_map_golden -- --ignored --nocapture`, then paste. Ignored because pinning is a
    /// decision, not a test. With `PALW_DERIVE_MAP_DUMP_DIR` set, the artifacts are written
    /// there too, to be looked at.
    #[test]
    #[ignore]
    fn print_map_golden() {
        let dump = std::env::var_os("PALW_DERIVE_MAP_DUMP_DIR").map(PathBuf::from);
        let mut out: BTreeMap<String, Value> = BTreeMap::new();
        for (name, answer) in corpus_files() {
            if let (Some(dir), Ok(d)) = (&dump, derive_with(&MapGrammar, &MapCompilerTransformer, &binding(), &answer)) {
                std::fs::write(dir.join(format!("{name}.{EXTENSION}")), &d.artifact.bytes).unwrap();
            }
            out.insert(name.clone(), corpus_entry(&answer, &name));
        }
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
    }

    // ── (6) the manifest and the discipline, scanned (X3) ──────────────────────────────────

    #[test]
    fn manifest_names_this_build_and_the_registry_finds_it() {
        let m = MapCompilerTransformer.manifest();
        assert_eq!(m.name, TRANSFORMER_NAME);
        assert_eq!(m.kind, kind::MAP);
        assert_eq!(m.grammar, GRAMMAR_NAME);
        assert_eq!(m.discipline, Discipline::Integer);
        assert_eq!(m.writer, WRITER_NAME);
        assert_eq!(m.source_tree_sha256, crate::SOURCE_TREE_SHA256_HEX);
        assert!(crate::registry::transformer_by_name(TRANSFORMER_NAME).is_some());
        assert!(crate::registry::grammar_by_name(GRAMMAR_NAME).is_some());
        let (g, t) = register();
        assert_eq!(g.len(), 1);
        assert_eq!(t.len(), 1);
        assert_eq!(g[0].name(), GRAMMAR_NAME);
        assert_eq!(t[0].manifest(), m);
    }

    #[test]
    fn x3_no_floating_point_type_is_spelled_in_this_file() {
        let src = include_str!("map.rs");
        assert!(!src.contains(concat!("f", "64")));
        assert!(!src.contains(concat!("f", "32")));
        assert!(!src.contains(concat!("Hash", "Map")));
    }

    /// X3's other half and X9's second sentence: the transformer reads no clock, draws nothing at
    /// random and touches no file, socket or environment variable — every metric it writes is an
    /// integer cost model of the map. Scanned over the code, not the tests: the tests read the
    /// corpus from disk, which is why the scan stops where `#[cfg(test)]` begins.
    #[test]
    fn x3_x9_the_transformer_reads_no_clock_no_randomness_and_no_world() {
        let src = include_str!("map.rs");
        let code = src.split(concat!("#[cfg(", "test)]")).next().expect("this file has a code half");
        assert!(code.len() > 1000, "the split found no code half");
        for forbidden in [
            concat!("std::", "time"),
            concat!("Inst", "ant"),
            concat!("Sys", "temTime"),
            concat!("rand", "::"),
            concat!("thread_", "rng"),
            concat!("std::", "fs"),
            concat!("std::", "net"),
            concat!("std::", "env"),
            concat!("std::", "process"),
        ] {
            assert!(!code.contains(forbidden), "the compiler must not reach {forbidden}");
        }
    }

    /// X8: the id is the kind table's row, assigned once. This file is the manifest that says
    /// what it means — the chain checks `kind != 0` and interprets nothing else, and an object
    /// whose kind disagreed with this manifest would be a false object anyone holding this file
    /// could demonstrate (Decision 5), never a second meaning.
    #[test]
    fn x8_the_kind_is_the_table_s_row_five_and_the_chain_interprets_none_of_it() {
        assert_eq!(kind::MAP, 5);
        assert_ne!(kind::MAP, 0);
        assert_eq!(kind::name(kind::MAP), Some("map"));
        assert_eq!(kind::id("map"), Some(kind::MAP));
        let d = derive_named(TRANSFORMER_NAME, &binding(), &bytes(&base())).unwrap();
        assert_eq!(d.object.kind, kind::MAP);
        // The object this kind builds passes the chain's stateless shape check (X2's half that
        // needs no state): a non-zero kind, an ML-DSA-87 key length, a non-empty artifact.
        kaspa_consensus_core::palw_derived_v1::check_derived_shape_v1(&d.object).expect("the shape the transition applies");
    }

    /// The corpus's bound-exhausting sample, measured rather than described (SA-2): the grid it
    /// asks for is over both compiler ceilings, and the grammar's cell bound is what refuses it
    /// first — which is exactly why no answer can reach [`derive_artifact`]'s two guards.
    #[test]
    fn sa2_the_corpus_sample_asks_for_more_than_both_compiler_ceilings() {
        let answer = std::fs::read(corpus_dir().join("91-refused-beyond-the-ceilings.json")).expect("the sample");
        let asked: Value = serde_json::from_slice(&answer).unwrap();
        let mut asks = map_of(&base());
        asks.width = asked["width"].as_u64().unwrap() as u32;
        asks.height = asked["height"].as_u64().unwrap() as u32;
        asks.nodes.clear();
        assert!(compile_steps_bound(&asks) > MAX_COMPILE_STEPS, "the sample must ask for more steps than the ceiling");
        assert!(artifact_len_bound(&asks) > MAX_ARTIFACT_BYTES, "the sample must ask for a file over the ceiling");
        // And the wall it actually hits is the grammar's, one bound earlier.
        let msg = refusal(&answer);
        assert!(msg.contains("cells; the grammar admits"), "{msg}");
        assert!(asks.cells() as u64 > MAX_CELLS);
    }
}
