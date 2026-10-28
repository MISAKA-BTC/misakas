//! Kind `cad` (ADR-0078 Decision 8): the parametric solid DSL — sketch → extrude → revolve →
//! boolean — and the canonical binary STL writer that makes a `.stl` of it. The row's
//! determinism basis is the strictest one in the table:
//!
//! > exact arithmetic; booleans are the hard part and a kernel that cannot make them exact does
//! > not ship the boolean
//!
//! That sentence is a licence and an obligation, and this module answers both in the open. What
//! ships is stated here so a reader never has to infer it:
//!
//! * **`extrude` and `revolve` ship in full**, over any simple sketch polygon, with every
//!   predicate a sign-of-determinant test in `i128` and no tolerance anywhere.
//! * **The boolean ships for one class: axis-aligned boxes.** Under a `union`, `difference` or
//!   `intersection`, every leaf must be a `box`; an `extrude` or a `revolve` under a boolean is
//!   refused BY NAME. The class is not a taste: see "why boxes" below — it is what the output's
//!   own exactness forces.
//! * **STEP is not shipped.** The row names `.stl` and `.step`; a kind may name several
//!   artifacts (Decision 8's last paragraph), and each artifact is its own transformer with its
//!   own `transformer_id`. `cad/step/v1` can be added later without renumbering anything. It is
//!   absent rather than half-present because a STEP file no CAD tool reads is worse than none,
//!   and this tree has no CAD tool to prove one against.
//! * **NURBS surfaces are not covered at v1**, by the row's own words.
//! * **A boolean whose RESULT is not a solid is refused, not written.** Two boxes that meet
//!   along an edge or at a corner have a boundary that is not a manifold there; the mesh checks
//!   below catch it and the kernel says so. "Exact" includes refusing to name a thing that is
//!   not one.
//!
//! ## Why the boolean's class is axis-aligned boxes, and not "convex polyhedra"
//!
//! An exact boolean of two general solids puts new vertices where three of their planes meet.
//! That point is a rational — exactly computable, no argument — but its coordinates are ratios
//! whose denominators are determinants of the three plane normals, and they do not in general
//! land on the model's fixed-point grid, which is the only place the artifact can hold a number
//! exactly ([`crate::fixed`], and see "the writer" below). So an exact kernel would compute a
//! correct vertex and then be unable to WRITE it. Rounding it is an ε in the binding path, and
//! ADR-0026 §3 forbids exactly that: "There is no ε parameter in the binding path." A kernel
//! that rounds its boolean is a kernel that ships a tolerance, so this one does not.
//!
//! Axis-aligned boxes are the class where the problem does not arise: every plane is
//! perpendicular to an axis, so every intersection vertex is a triple of coordinates that were
//! already in the input. The boolean is evaluated on the grid those coordinates induce — sort
//! the distinct planes per axis, and every cell of the resulting lattice is wholly inside or
//! wholly outside every leaf, so a cell's membership is three integer comparisons at its centre
//! (taken in DOUBLED coordinates, so the centre is an integer too and nothing is halved). The
//! result's boundary is the set of lattice faces with an inside cell on one side and an outside
//! cell on the other — every one of them a rectangle on grid coordinates. No predicate in that
//! paragraph is anything but an integer comparison, which is what "exact" has to mean here.
//!
//! Widening the class is a new transformer (`cad/stl/v2`), not an edit: it needs the artifact to
//! be able to hold an off-grid rational, which means a writer whose numbers are rationals, which
//! STL is not.
//!
//! ## Why `revolve` counts its segments and often refuses
//!
//! A revolve samples the circle at `segments` angles. The vertex at angle `2πk/n` is
//! `(r·cos, r·sin, z)`, so an exact vertex needs `cos(2πk/n)` and `sin(2πk/n)` both rational.
//! Niven's theorem says the only rational values of `cos(rπ)` for rational `r` are `0, ±1/2, ±1`;
//! `sin θ = cos(π/2 − θ)` is under the same rule, and `cos² + sin² = 1` then leaves only the
//! pairs `(±1, 0)` and `(0, ±1)`. So the ONLY regular polygon whose vertices are exact on any
//! grid is the quarter turn: `segments = 4`. Every other segment count is refused with
//! [`DeriveError::Inexact`] naming the theorem — not because the number is hard to compute, but
//! because it is irrational and a rounded vertex is a value two honest hosts could round
//! differently.
//!
//! A revolve that only ever had four sides would be a useless one, so the DSL offers the exact
//! way to have more: `directions`, an explicit list of rational unit vectors `[a, b, c]` with
//! `a² + b² = c²` — the Pythagorean parametrisation of the rational points of the unit circle,
//! which are DENSE in it. The transformer checks the triple exactly, checks the list is in
//! strictly counter-clockwise angular order (by half-plane and then by the sign of a cross
//! product — two integer tests), and checks that each vertex `r·a/c` divides exactly, refusing
//! by name when it does not. A twelve-direction ring from the (3,4,5) triple is in the corpus.
//!
//! The sweep closes the ring from the last direction back to the first, so a short list is a
//! COARSE ring and never a partial one: three directions inside one quadrant make a thin
//! three-sided solid, not an open surface. That is the answer the DSL asked for, and whether it
//! is a solid is decided by the mesh checks below and not by the direction count.
//!
//! ## The grammar `cad/v1`
//!
//! ```text
//! { "v": 1,
//!   "frac_bits": 0..=16,                       every coordinate is mantissa / 2^frac_bits
//!   "sketches": { "<name>": [[x, y], ...] },   closed simple polygons, 3..=256 points
//!   "solid": <node> }
//! ```
//!
//! A sketch is a closed polygon given by its points; the closing edge is implicit and must not
//! be repeated. For an `extrude` the pair is read as `(x, y)`; for a `revolve` it is read as
//! `(r, z)` in the half-plane `r ≥ 0`. Winding may be either way: the transformer reads the
//! sign of the shoelace sum and orients the solid outward from it, which is a pure function of
//! the points and not a repair of the bytes.
//!
//! ```text
//! { "op": "box",          "min": [x,y,z], "max": [x,y,z] }
//! { "op": "extrude",      "sketch": "<name>", "z0": i, "z1": i }
//! { "op": "revolve",      "sketch": "<name>", "segments": 3..=64 }
//! { "op": "revolve",      "sketch": "<name>", "directions": [[a,b,c], ...] }
//! { "op": "union",        "a": <node>, "b": <node> }
//! { "op": "difference",   "a": <node>, "b": <node> }
//! { "op": "intersection", "a": <node>, "b": <node> }
//! ```
//!
//! Canonicalization is [`crate::canon_json`]'s (sorted keys, no whitespace, integers only) plus
//! this schema, and it reorders nothing (Decision 2: nothing semantic). Geometry is NOT the
//! grammar's business: whether a polygon is simple, whether a revolve is exact, whether a
//! boolean is in class — those are the transformer's refusals, and they produce no object
//! either way (X4's outcome, reached by the honest route).
//!
//! ## The writer, and the three free fields a determinism leak hides in
//!
//! Binary STL is trivially canonical ONCE three things nobody's format pins are pinned here:
//!
//! * **The 80-byte header is free bytes.** Every exporter that puts a timestamp, a program name
//!   or a build string there emits a different file for the same solid — which is exactly the
//!   non-determinism this ADR exists to refuse. It is pinned to [`STL_HEADER_TEXT`],
//!   zero-padded, forever; it deliberately does not begin with `solid`, because a binary file
//!   whose header starts with that word is misread as ASCII STL by parsers that sniff.
//! * **The facet normal is free.** An outward unit normal needs a square root, which is
//!   irrational for almost every triangle, so writing one would mean rounding one. It is pinned
//!   to three zero values, the format's documented "derive it from the winding" convention, and
//!   the winding is right-handed (counter-clockwise seen from outside). A consumer that wants
//!   the normal computes it from the vertices, exactly, in whatever precision it likes.
//! * **The triangle order is free.** It is pinned twice: each triangle is rotated so its
//!   smallest vertex comes first (which preserves the winding, and so the orientation), and the
//!   list is then sorted. The artifact is therefore a function of the SET of oriented triangles
//!   and not of the order the kernel happened to build them in.
//!
//! The attribute byte count after every facet is pinned to zero (it is the other free field, and
//! the one colour extensions abuse).
//!
//! Coordinates are written with [`crate::fixed::f32_le_exact`], which builds the IEEE-754
//! binary32 bit pattern from the integer mantissa and the binary scale with integer arithmetic
//! only and REFUSES anything the format cannot hold exactly. No floating-point value is ever
//! computed here; one is only spelled.
//!
//! ## What the transformer checks before it writes (Decision 3, X3)
//!
//! Fail closed and by name: the mesh must be a closed, consistently oriented manifold — every
//! directed edge appears exactly once and its reverse exactly once — it must hold no degenerate
//! and no duplicated triangle, and its exact signed volume (the integer sum of `det(v0,v1,v2)`,
//! six times the volume) must be positive. Those four checks are cheap, exact, and they catch
//! the whole class of "the kernel built something that is not a solid" without a tolerance.
//!
//! ## The three bounds, and why each of them can actually be reached (SA-2)
//!
//! ADR-0078's security amendment SA-2 is written for exactly this kind: *"a procedural or scene
//! DSL can encode a mesh that exhausts memory at build time, on the executor and on every
//! consumer who verifies"*. A `cad/v1` answer is six lines of JSON that ask for a hundred
//! megabytes of triangles, so the bounds are declared ([`BOUNDS`]) and answered BEFORE the
//! kernel allocates, from the DSL alone ([`plan`]), and exceeding one is no object.
//!
//! The grammar bounds the TEXT (points a sketch, sketches a model, nodes a tree); the bounds
//! below bound the BUILD, and the two are deliberately different numbers. Each one bites on a
//! different op, so no bound is dead and none hides another:
//!
//! | bound | value | what reaches it first |
//! |---|---|---|
//! | [`MAX_DSL_BYTES`] | 64 KiB | the text: 32 sketches of 256 points is ~180 KiB |
//! | [`MAX_STEPS`] | 4,000,000 | an `extrude`: ear clipping is `n³`, so a sketch above ~158 points |
//! | [`MAX_ARTIFACT_BYTES`] | 1 MiB | a `revolve`: `2·n·m` triangles, so 256 points × 64 directions |
//!
//! The boolean reaches none of them, because [`BOOLEAN_LEAVES_MAX`] is DERIVED from the artifact
//! ceiling rather than declared: it is the largest leaf count whose worst-case lattice still
//! fits. So a boolean the grammar admits is never refused by a bound, and the ceiling's only
//! job is the revolve's.
//!
//! `max_steps` is counted in this kind's own unit — one exact integer predicate — and [`plan`]
//! is a true upper bound on the number the kernel can execute, not an estimate:
//!
//! ```text
//! a sketch of n points, validated:              n²        the non-adjacent edge-pair scan
//!                       triangulated:           n³        ≤ n clips × n candidates × n containments
//! extrude(n):                        n² + n³ + 4n         and exactly 4n − 4 triangles
//! revolve(n, m directions):          n² + 4·n·m           and at most 2·n·m triangles
//! boolean(L boxes, C lattice cells): C·(L + 6)            and at most 12·C triangles
//! ```

use crate::bytes::{put_u16_le, put_u32_le};
use crate::canon_json::{CanonValue, parse_canonical, write_canonical};
use crate::fixed::f32_le_exact;
use crate::{Artifact, DeriveError, Discipline, Grammar, Transformer, TransformerManifest};
use kaspa_consensus_core::palw_derived_v1::kind;
use std::collections::{BTreeMap, BTreeSet};

/// The grammar's name; its id is `H(domain ‖ name)` (`ids::grammar_id_v1`).
pub const GRAMMAR_NAME: &str = "cad/v1";
/// The transformer's name. A second artifact for this kind (`cad/step/v1`) would be a second
/// name and a second id, never an edit of this one.
pub const TRANSFORMER_NAME: &str = "cad/stl/v1";
/// The canonical writer the manifest names: binary STL with the three free fields pinned.
pub const WRITER_NAME: &str = "stl-binary/1.0/zero-normal-rh-winding-sorted-v1";
/// The artifact's media type (IANA `model/stl`) and file extension.
pub const MEDIA_TYPE: &str = "model/stl";
pub const EXTENSION: &str = "stl";

/// The binary scale a model may declare. Coordinates are `mantissa / 2^frac_bits`.
pub const FRAC_BITS_MAX: i128 = 16;
/// The largest coordinate mantissa, in absolute value. Below `2^24`, every input coordinate has
/// at most 24 significant bits and is therefore exactly a binary32 — so a well-formed model
/// never reaches the writer's refusal, and the refusal is still there for what the kernel
/// computes.
pub const COORD_MAX: i128 = (1 << 24) - 1;
/// Points in one sketch.
pub const SKETCH_POINTS_MIN: usize = 3;
pub const SKETCH_POINTS_MAX: usize = 256;
/// Sketches in one model, and the bytes of a sketch's name.
pub const SKETCHES_MAX: usize = 32;
pub const SKETCH_NAME_MAX_BYTES: usize = 64;
/// Nodes in one solid tree, and how deep it may nest.
pub const SOLID_NODES_MAX: usize = 64;
pub const SOLID_DEPTH_MAX: usize = 24;
/// Directions a `revolve` may sample.
pub const REVOLVE_DIRECTIONS_MIN: usize = 3;
pub const REVOLVE_DIRECTIONS_MAX: usize = 64;
/// The segment counts a `revolve` may ask for. Only [`REVOLVE_SEGMENTS_EXACT`] survives the
/// exactness gate; the rest are admitted by the grammar and refused by the kernel, so the
/// refusal can explain itself.
pub const REVOLVE_SEGMENTS_MIN: i128 = REVOLVE_DIRECTIONS_MIN as i128;
pub const REVOLVE_SEGMENTS_MAX: i128 = REVOLVE_DIRECTIONS_MAX as i128;
/// The only regular polygon with exact rational vertices (Niven's theorem; see the module doc).
pub const REVOLVE_SEGMENTS_EXACT: i128 = 4;

// --- ADR-0078 SA-2: the three bounds this transformer declares, enforced before it runs ------

/// SA-2's `max_dsl_bytes`. The DSL is a stranger's prompt answered by a model, so its size is
/// bounded before a parser sees it — on the executor and on every consumer who verifies.
pub const MAX_DSL_BYTES: usize = 64 << 10;
/// SA-2's `max_artifact_bytes`. A derived artifact is a thing a person keeps and a thing every
/// verifier rebuilds; one mebibyte of binary STL is about 21,000 triangles.
pub const MAX_ARTIFACT_BYTES: usize = 1 << 20;
/// SA-2's `max_steps`, in this kind's own unit: one exact integer predicate — an orientation
/// determinant, a containment test, a rational turn, a lattice-cell membership. The budget is
/// answered from the DSL alone by [`plan`], before the kernel allocates anything.
pub const MAX_STEPS: u64 = 4_000_000;

/// Derived from [`MAX_ARTIFACT_BYTES`], never declared: what the facet size leaves room for.
pub const TRIANGLES_MAX: usize = (MAX_ARTIFACT_BYTES - STL_PREAMBLE_BYTES) / STL_FACET_BYTES;
/// A lattice cell has six faces and a face is two triangles.
pub const CSG_FACES_PER_CELL: usize = 6;
pub const TRIANGLES_PER_FACE: usize = 2;

/// The cells a boolean of `leaves` axis-aligned boxes can induce: each box contributes at most
/// two distinct planes per axis, so at most `2·leaves − 1` cells along each of the three.
pub const fn csg_cells_bound(leaves: usize) -> usize {
    let per_axis = 2 * leaves - 1;
    per_axis * per_axis * per_axis
}

/// Boxes under one boolean tree — derived from [`MAX_ARTIFACT_BYTES`], never declared: the
/// largest leaf count whose WORST-CASE lattice still fits the artifact ceiling. Deriving it
/// rather than picking it is what makes the ceiling a bound on the revolve alone: a boolean the
/// grammar admits can never be refused by the ceiling, so the two bounds do not overlap and
/// neither hides the other.
pub const BOOLEAN_LEAVES_MAX: usize = {
    let mut leaves = 1;
    while csg_cells_bound(leaves + 1) * CSG_FACES_PER_CELL * TRIANGLES_PER_FACE <= TRIANGLES_MAX {
        leaves += 1;
    }
    leaves
};
/// Derived from [`BOOLEAN_LEAVES_MAX`], never declared.
pub const CSG_CELLS_MAX: usize = csg_cells_bound(BOOLEAN_LEAVES_MAX);

/// The bounds a manifest carries under ADR-0078 SA-2. The manifest type in `lib.rs` does not
/// hold these fields yet (`derive-core` owns it); until it does they are declared here, spelled
/// once, and enforced by [`CadStlTransformer::run`] before any of the kernel runs — which is
/// what SA-2 asks for. When the fields land, this is the value that fills them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CadBounds {
    pub max_dsl_bytes: usize,
    pub max_artifact_bytes: usize,
    pub max_steps: u64,
}

/// This transformer's declared bounds (SA-2).
pub const BOUNDS: CadBounds = CadBounds { max_dsl_bytes: MAX_DSL_BYTES, max_artifact_bytes: MAX_ARTIFACT_BYTES, max_steps: MAX_STEPS };

/// The 80 free bytes of a binary STL, pinned. Anything a build or a clock could vary is exactly
/// what this ADR refuses to let into an artifact; it deliberately does not start with `solid`.
pub const STL_HEADER_TEXT: &str = "misaka-palw cad/stl/v1; normals zero; right-hand winding; no timestamp";
/// The other free field: the two bytes after every facet.
pub const STL_ATTRIBUTE_BYTE_COUNT: u16 = 0;
/// A facet on the wire: 12 binary32 values and the attribute count.
pub const STL_FACET_BYTES: usize = 50;
/// The header and the facet count.
pub const STL_PREAMBLE_BYTES: usize = 84;

/// The grammar `cad/v1`.
pub struct CadGrammar;
/// The transformer `cad/stl/v1`: canonical `cad/v1` bytes to a canonical binary STL.
pub struct CadStlTransformer;

/// This kind's grammar and transformer, as the registry sees them.
pub fn register() -> (Vec<Box<dyn Grammar>>, Vec<Box<dyn Transformer>>) {
    (vec![Box::new(CadGrammar)], vec![Box::new(CadStlTransformer)])
}

impl Grammar for CadGrammar {
    fn name(&self) -> &'static str {
        GRAMMAR_NAME
    }

    /// Parse, hold to the schema, re-emit. A refusal here is `DeriveError::Grammar` (X4): the
    /// answer is not `cad/v1`, no object exists, and the claim is untouched. The size bound is
    /// taken BEFORE the parser (SA-2), because a parser is the first thing an oversized answer
    /// would spend memory on.
    fn canonicalize(&self, answer: &[u8]) -> Result<Vec<u8>, DeriveError> {
        if answer.len() > MAX_DSL_BYTES {
            return Err(grammar(format!("the answer is {} bytes; at most {MAX_DSL_BYTES} (ADR-0078 SA-2)", answer.len())));
        }
        let value = parse_canonical(answer)?;
        parse_model(&value)?;
        Ok(write_canonical(&value))
    }
}

impl Transformer for CadStlTransformer {
    fn manifest(&self) -> TransformerManifest {
        TransformerManifest {
            name: TRANSFORMER_NAME,
            kind: kind::CAD,
            grammar: GRAMMAR_NAME,
            // ADR-0078 Decision 3's second discipline, and the only kind in the tree that
            // declares it: the revolve's directions are exact rationals, not integers.
            discipline: Discipline::ExactRational,
            writer: WRITER_NAME,
            source_tree_sha256: crate::SOURCE_TREE_SHA256_HEX,
            // ADR-0078 SA-2, from this module's own declaration and not a copy of it.
            max_dsl_bytes: BOUNDS.max_dsl_bytes as u64,
            max_artifact_bytes: BOUNDS.max_artifact_bytes as u64,
            max_steps: BOUNDS.max_steps,
        }
    }

    /// Every declared bound is answered before the kernel builds anything (SA-2): the DSL's size
    /// before the parser, the step budget and the artifact ceiling from the parsed model alone.
    fn run(&self, dsl: &[u8]) -> Result<Artifact, DeriveError> {
        if dsl.len() > MAX_DSL_BYTES {
            return Err(refuse(format!("the dsl is {} bytes; at most {MAX_DSL_BYTES} (ADR-0078 SA-2)", dsl.len())));
        }
        let model = canonical_model(dsl)?;
        plan(&model)?.check()?;
        let triangles = mesh(&model)?;
        let bytes = write_stl(&triangles, model.frac_bits)?;
        Ok(Artifact { bytes, media_type: MEDIA_TYPE, extension: EXTENSION })
    }
}

/// What the kernel will cost, from the DSL alone — SA-2's gate, answered before anything is
/// built. Both numbers are upper bounds proved by construction (see the module doc's table),
/// never measurements, so the refusal happens before the work and not after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub steps: u64,
    pub triangles: u64,
    pub artifact_bytes: u64,
}

impl Plan {
    /// The two bounds a plan can exceed. Exceeding one is "no object" (SA-2, Decision 2's arm).
    pub fn check(&self) -> Result<(), DeriveError> {
        if self.steps > MAX_STEPS {
            return Err(refuse(format!(
                "the model needs at most {} exact predicates to build; the step budget is {MAX_STEPS} (ADR-0078 SA-2). \
                 A sketch's triangulation costs the cube of its point count, so this is a smaller sketch, not a bigger \
                 budget",
                self.steps
            )));
        }
        if self.artifact_bytes > MAX_ARTIFACT_BYTES as u64 {
            return Err(refuse(format!(
                "the model builds at most {} triangles, which is {} bytes of binary STL; the artifact ceiling is \
                 {MAX_ARTIFACT_BYTES} (ADR-0078 SA-2)",
                self.triangles, self.artifact_bytes
            )));
        }
        Ok(())
    }
}

/// The plan of a model: what its root op can cost. Saturating throughout — a bound that
/// overflowed would be a bound that passed.
pub fn plan(model: &Model) -> Result<Plan, DeriveError> {
    let (steps, triangles) = match &model.solid {
        Node::Extrude { sketch, .. } => {
            let n = sketch_of(model, sketch)?.len() as u64;
            // 2n side triangles and n − 2 in each of the two caps; the grammar guarantees n ≥ 3
            (
                n.saturating_mul(n).saturating_add(n.saturating_pow(3)).saturating_add(n.saturating_mul(4)),
                n.saturating_mul(4).saturating_sub(4),
            )
        }
        Node::Revolve { sketch, segments, directions } => {
            let n = sketch_of(model, sketch)?.len() as u64;
            // a `segments` revolve resolves to REVOLVE_SEGMENTS_EXACT directions or is refused;
            // either way the count the kernel will sweep is known here
            let m = match (segments, directions) {
                (Some(_), None) => REVOLVE_SEGMENTS_EXACT as u64,
                (None, Some(d)) => d.len() as u64,
                _ => return Err(refuse("revolve: exactly one of segments and directions is required".into())),
            };
            (n.saturating_mul(n).saturating_add(4u64.saturating_mul(n).saturating_mul(m)), n.saturating_mul(m).saturating_mul(2))
        }
        node => {
            let mut boxes = Vec::new();
            csg_of(node, &mut boxes)?;
            let cells = csg_lattice_cells(&boxes) as u64;
            let leaves = boxes.len() as u64;
            (
                cells.saturating_mul(leaves.saturating_add(CSG_FACES_PER_CELL as u64)),
                cells.saturating_mul((CSG_FACES_PER_CELL * TRIANGLES_PER_FACE) as u64),
            )
        }
    };
    Ok(Plan {
        steps,
        triangles,
        artifact_bytes: (triangles.saturating_mul(STL_FACET_BYTES as u64)).saturating_add(STL_PREAMBLE_BYTES as u64),
    })
}

// ---------------------------------------------------------------------------------------------
// The model
// ---------------------------------------------------------------------------------------------

/// A point of a sketch: `(x, y)` for an extrude, `(r, z)` for a revolve, both in mantissa units.
pub type P2 = [i64; 2];
/// A vertex, in mantissa units.
pub type P3 = [i64; 3];
/// An oriented triangle; the winding is right-handed seen from outside the solid.
pub type Tri = [P3; 3];

/// One node of the solid tree. `Cuboid` spells the DSL's `box`; the variant is not called `Box`
/// so that the pointer type in the same declaration stays legible.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    Cuboid {
        min: P3,
        max: P3,
    },
    Extrude {
        sketch: String,
        z0: i64,
        z1: i64,
    },
    /// The directions are already resolved from `segments` where the grammar saw one; the
    /// exactness gate is the transformer's, so the resolution is too.
    Revolve {
        sketch: String,
        segments: Option<i64>,
        directions: Option<Vec<[i64; 3]>>,
    },
    Union(Box<Node>, Box<Node>),
    Difference(Box<Node>, Box<Node>),
    Intersection(Box<Node>, Box<Node>),
}

impl Node {
    /// The `op` this node was spelled with — what a refusal names.
    pub fn op_name(&self) -> &'static str {
        match self {
            Node::Cuboid { .. } => "box",
            Node::Extrude { .. } => "extrude",
            Node::Revolve { .. } => "revolve",
            Node::Union(..) => "union",
            Node::Difference(..) => "difference",
            Node::Intersection(..) => "intersection",
        }
    }
}

/// A whole answer, validated against the schema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Model {
    pub frac_bits: u32,
    pub sketches: BTreeMap<String, Vec<P2>>,
    pub solid: Node,
}

fn grammar(msg: String) -> DeriveError {
    DeriveError::Grammar(msg)
}

fn refuse(msg: String) -> DeriveError {
    DeriveError::Transformer(msg)
}

fn object<'a>(v: &'a CanonValue, what: &str) -> Result<&'a BTreeMap<String, CanonValue>, DeriveError> {
    v.as_obj().ok_or_else(|| grammar(format!("{what} is not an object")))
}

/// Exactly `expected` keys: an unknown key and a missing key are each a refusal by name.
fn exact_keys(obj: &BTreeMap<String, CanonValue>, expected: &[&str], what: &str) -> Result<(), DeriveError> {
    for key in obj.keys() {
        if !expected.contains(&key.as_str()) {
            return Err(grammar(format!("{what}: unknown key {key:?}")));
        }
    }
    for key in expected {
        if !obj.contains_key(*key) {
            return Err(grammar(format!("{what}: missing key {key:?}")));
        }
    }
    Ok(())
}

fn integer(v: &CanonValue, what: &str) -> Result<i128, DeriveError> {
    match v {
        CanonValue::Int(i) => Ok(*i),
        _ => Err(grammar(format!("{what} must be an integer"))),
    }
}

fn integer_in(v: &CanonValue, lo: i128, hi: i128, what: &str) -> Result<i128, DeriveError> {
    let i = integer(v, what)?;
    if (lo..=hi).contains(&i) { Ok(i) } else { Err(grammar(format!("{what} {i} is outside {lo}..={hi}"))) }
}

fn coordinate(v: &CanonValue, what: &str) -> Result<i64, DeriveError> {
    Ok(integer_in(v, -COORD_MAX, COORD_MAX, what)? as i64)
}

/// A fixed-length integer array, e.g. a point or a box corner.
fn integer_array<const N: usize>(v: &CanonValue, what: &str) -> Result<[i64; N], DeriveError> {
    let items = v.as_arr().ok_or_else(|| grammar(format!("{what} must be an array")))?;
    if items.len() != N {
        return Err(grammar(format!("{what} must hold {N} integers, not {}", items.len())));
    }
    let mut out = [0i64; N];
    for (i, item) in items.iter().enumerate() {
        out[i] = coordinate(item, &format!("{what}[{i}]"))?;
    }
    Ok(out)
}

/// Hold a parsed answer to the `cad/v1` schema and lift it into a [`Model`]. Every violation is
/// `DeriveError::Grammar` and names what it saw. Geometry is not checked here (see the module
/// doc): the grammar's job is shape, and the kernel's is whether the shape is a solid.
pub fn parse_model(v: &CanonValue) -> Result<Model, DeriveError> {
    let top = object(v, "top level")?;
    exact_keys(top, &["v", "frac_bits", "sketches", "solid"], "top level")?;
    let version = integer(&top["v"], "v")?;
    if version != 1 {
        return Err(grammar(format!("v must be 1, not {version}")));
    }
    let frac_bits = integer_in(&top["frac_bits"], 0, FRAC_BITS_MAX, "frac_bits")? as u32;

    let sketches_in = object(&top["sketches"], "sketches")?;
    if sketches_in.len() > SKETCHES_MAX {
        return Err(grammar(format!("sketches holds {} sketches; at most {SKETCHES_MAX}", sketches_in.len())));
    }
    let mut sketches: BTreeMap<String, Vec<P2>> = BTreeMap::new();
    for (name, points_v) in sketches_in {
        if name.is_empty() || name.len() > SKETCH_NAME_MAX_BYTES {
            return Err(grammar(format!("sketch name {name:?} is {} bytes; 1..={SKETCH_NAME_MAX_BYTES} are allowed", name.len())));
        }
        let what = format!("sketch {name:?}");
        let points_in = points_v.as_arr().ok_or_else(|| grammar(format!("{what} must be an array of points")))?;
        if !(SKETCH_POINTS_MIN..=SKETCH_POINTS_MAX).contains(&points_in.len()) {
            return Err(grammar(format!(
                "{what} holds {} points; {SKETCH_POINTS_MIN}..={SKETCH_POINTS_MAX} are allowed",
                points_in.len()
            )));
        }
        let mut points = Vec::with_capacity(points_in.len());
        for (i, p) in points_in.iter().enumerate() {
            points.push(integer_array::<2>(p, &format!("{what} point {i}"))?);
        }
        sketches.insert(name.clone(), points);
    }

    let mut nodes = 0usize;
    let solid = parse_node(&top["solid"], "solid", 0, &mut nodes, &sketches)?;
    Ok(Model { frac_bits, sketches, solid })
}

fn parse_node(
    v: &CanonValue,
    what: &str,
    depth: usize,
    nodes: &mut usize,
    sketches: &BTreeMap<String, Vec<P2>>,
) -> Result<Node, DeriveError> {
    if depth > SOLID_DEPTH_MAX {
        return Err(grammar(format!("{what} nests deeper than {SOLID_DEPTH_MAX}")));
    }
    *nodes += 1;
    if *nodes > SOLID_NODES_MAX {
        return Err(grammar(format!("the solid tree holds more than {SOLID_NODES_MAX} nodes")));
    }
    let obj = object(v, what)?;
    let op = obj.get("op").and_then(CanonValue::as_str).ok_or_else(|| grammar(format!("{what}: op must be a string")))?;
    let sketch_named = |key: &str| -> Result<String, DeriveError> {
        let name = obj.get(key).and_then(CanonValue::as_str).ok_or_else(|| grammar(format!("{what}: {key} must be a string")))?;
        if !sketches.contains_key(name) {
            return Err(grammar(format!("{what}: sketch {name:?} is not declared in sketches")));
        }
        Ok(name.to_string())
    };
    match op {
        "box" => {
            exact_keys(obj, &["op", "min", "max"], what)?;
            let min = integer_array::<3>(&obj["min"], &format!("{what} min"))?;
            let max = integer_array::<3>(&obj["max"], &format!("{what} max"))?;
            for (axis, name) in ["x", "y", "z"].iter().enumerate() {
                if min[axis] >= max[axis] {
                    return Err(grammar(format!("{what}: min {name} {} is not below max {name} {}", min[axis], max[axis])));
                }
            }
            Ok(Node::Cuboid { min, max })
        }
        "extrude" => {
            exact_keys(obj, &["op", "sketch", "z0", "z1"], what)?;
            let sketch = sketch_named("sketch")?;
            let z0 = coordinate(&obj["z0"], &format!("{what} z0"))?;
            let z1 = coordinate(&obj["z1"], &format!("{what} z1"))?;
            if z0 >= z1 {
                return Err(grammar(format!("{what}: z0 {z0} is not below z1 {z1}")));
            }
            Ok(Node::Extrude { sketch, z0, z1 })
        }
        "revolve" => {
            let sketch = sketch_named("sketch")?;
            let has_segments = obj.contains_key("segments");
            let has_directions = obj.contains_key("directions");
            match (has_segments, has_directions) {
                (true, true) => Err(grammar(format!("{what}: segments and directions are alternatives; give exactly one"))),
                (false, false) => Err(grammar(format!("{what}: give exactly one of segments and directions"))),
                (true, false) => {
                    exact_keys(obj, &["op", "sketch", "segments"], what)?;
                    let segments =
                        integer_in(&obj["segments"], REVOLVE_SEGMENTS_MIN, REVOLVE_SEGMENTS_MAX, &format!("{what} segments"))?;
                    Ok(Node::Revolve { sketch, segments: Some(segments as i64), directions: None })
                }
                (false, true) => {
                    exact_keys(obj, &["op", "sketch", "directions"], what)?;
                    let items = obj["directions"].as_arr().ok_or_else(|| grammar(format!("{what}: directions must be an array")))?;
                    if !(REVOLVE_DIRECTIONS_MIN..=REVOLVE_DIRECTIONS_MAX).contains(&items.len()) {
                        return Err(grammar(format!(
                            "{what}: directions holds {}; {REVOLVE_DIRECTIONS_MIN}..={REVOLVE_DIRECTIONS_MAX} are allowed",
                            items.len()
                        )));
                    }
                    let mut directions = Vec::with_capacity(items.len());
                    for (i, item) in items.iter().enumerate() {
                        directions.push(integer_array::<3>(item, &format!("{what} direction {i}"))?);
                    }
                    Ok(Node::Revolve { sketch, segments: None, directions: Some(directions) })
                }
            }
        }
        "union" | "difference" | "intersection" => {
            exact_keys(obj, &["op", "a", "b"], what)?;
            let a = Box::new(parse_node(&obj["a"], &format!("{what}.a"), depth + 1, nodes, sketches)?);
            let b = Box::new(parse_node(&obj["b"], &format!("{what}.b"), depth + 1, nodes, sketches)?);
            Ok(match op {
                "union" => Node::Union(a, b),
                "difference" => Node::Difference(a, b),
                _ => Node::Intersection(a, b),
            })
        }
        other => Err(grammar(format!("{what}: unknown op {other:?}"))),
    }
}

/// The model behind canonical `cad/v1` bytes. The transformer repairs nothing: input that is
/// not exactly the grammar's own output — unparseable, off-schema, or merely spelled
/// differently — is refused as `DeriveError::Transformer`.
pub fn canonical_model(dsl: &[u8]) -> Result<Model, DeriveError> {
    let not_canonical = |e: DeriveError| refuse(format!("input is not canonical cad/v1: {e}"));
    let value = parse_canonical(dsl).map_err(not_canonical)?;
    let model = parse_model(&value).map_err(not_canonical)?;
    if write_canonical(&value) != dsl {
        return Err(refuse("input is not canonical cad/v1: the bytes differ from their canonical form".into()));
    }
    Ok(model)
}

// ---------------------------------------------------------------------------------------------
// Exact planar predicates — every one a sign of a determinant, in i128, with no tolerance
// ---------------------------------------------------------------------------------------------

/// Twice the signed area of the triangle `a b c`: positive when `a b c` turns left. The only
/// orientation test in this module, used by the sketch validator, the ear clipper and the
/// winding decision.
pub fn orient2(a: P2, b: P2, c: P2) -> i128 {
    let (abx, aby) = (i128::from(b[0]) - i128::from(a[0]), i128::from(b[1]) - i128::from(a[1]));
    let (acx, acy) = (i128::from(c[0]) - i128::from(a[0]), i128::from(c[1]) - i128::from(a[1]));
    abx * acy - aby * acx
}

/// Twice the signed area of a closed polygon (the shoelace sum). Its sign is the winding.
pub fn signed_area2(points: &[P2]) -> i128 {
    let mut sum = 0i128;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        sum += i128::from(a[0]) * i128::from(b[1]) - i128::from(b[0]) * i128::from(a[1]);
    }
    sum
}

/// Is `p` on the segment `a b`, given that the three are already known to be collinear?
fn on_segment(a: P2, b: P2, p: P2) -> bool {
    p[0] >= a[0].min(b[0]) && p[0] <= a[0].max(b[0]) && p[1] >= a[1].min(b[1]) && p[1] <= a[1].max(b[1])
}

/// Do the closed segments `p1 p2` and `p3 p4` share a point? Four orientation signs and, for
/// the collinear case, four containment tests — no arithmetic beyond `i128`.
pub fn segments_intersect(p1: P2, p2: P2, p3: P2, p4: P2) -> bool {
    let d1 = orient2(p3, p4, p1).signum();
    let d2 = orient2(p3, p4, p2).signum();
    let d3 = orient2(p1, p2, p3).signum();
    let d4 = orient2(p1, p2, p4).signum();
    if d1 * d2 < 0 && d3 * d4 < 0 {
        return true;
    }
    (d1 == 0 && on_segment(p3, p4, p1))
        || (d2 == 0 && on_segment(p3, p4, p2))
        || (d3 == 0 && on_segment(p1, p2, p3))
        || (d4 == 0 && on_segment(p1, p2, p4))
}

/// Is `p` inside the closed triangle `a b c` (which must turn left)? Three sign tests; a point
/// on an edge counts as inside, which makes the ear test conservative rather than optimistic.
fn point_in_triangle(p: P2, a: P2, b: P2, c: P2) -> bool {
    orient2(a, b, p) >= 0 && orient2(b, c, p) >= 0 && orient2(c, a, p) >= 0
}

/// A sketch's points, oriented counter-clockwise, after the three checks that make a point list
/// a polygon: no point repeats, no vertex folds its two edges back on each other, and no two
/// non-adjacent edges meet. Each failure names itself; none of them has a tolerance in it.
pub fn simple_polygon_ccw(name: &str, points: &[P2]) -> Result<Vec<P2>, DeriveError> {
    let n = points.len();
    let mut seen: BTreeSet<P2> = BTreeSet::new();
    for (i, p) in points.iter().enumerate() {
        if !seen.insert(*p) {
            return Err(refuse(format!("sketch {name:?}: point {i} {p:?} repeats an earlier point; a sketch is a simple polygon")));
        }
    }
    // A vertex whose two edges are collinear is a straight-through point (harmless — the ear
    // clipper drops it) unless the path reverses there, which is a spike and not a corner.
    for i in 0..n {
        let (a, b, c) = (points[i], points[(i + 1) % n], points[(i + 2) % n]);
        if orient2(a, b, c) == 0 && turns_back(a, b, c) {
            return Err(refuse(format!("sketch {name:?}: the edges at point {b:?} fold back on themselves")));
        }
    }
    // Two adjacent edges that are not collinear meet only at the vertex they share, so the loop
    // above covers them; every other pair must not meet at all.
    for i in 0..n {
        for j in (i + 2)..n {
            if i == 0 && j == n - 1 {
                continue;
            }
            if segments_intersect(points[i], points[(i + 1) % n], points[j], points[(j + 1) % n]) {
                return Err(refuse(format!("sketch {name:?}: edge {i} and edge {j} cross; a sketch is a simple polygon")));
            }
        }
    }
    let area2 = signed_area2(points);
    if area2 == 0 {
        return Err(refuse(format!("sketch {name:?}: the polygon encloses no area")));
    }
    let mut out = points.to_vec();
    if area2 < 0 {
        out.reverse();
    }
    Ok(out)
}

/// For three collinear points, does the path `a → b → c` reverse direction at `b`? A polygon
/// with such a spike is not simple.
fn turns_back(a: P2, b: P2, c: P2) -> bool {
    let dot = (i128::from(b[0]) - i128::from(a[0])) * (i128::from(c[0]) - i128::from(b[0]))
        + (i128::from(b[1]) - i128::from(a[1])) * (i128::from(c[1]) - i128::from(b[1]));
    dot <= 0
}

/// Ear clipping over exact orientation tests: the triangulation of a counter-clockwise simple
/// polygon, as index triples. Exactly `n − 2` triangles, and EVERY point of the sketch is a
/// vertex of one of them — which is what makes an extrude closed, because its side walls
/// reference every point and the cap has to give each one back.
///
/// So a vertex that lies straight on the line through its two live neighbours is skipped, not
/// removed: clipping it would emit a triangle with no area, and removing it without one would
/// take a point out of the cap that the walls still name — a prism with a slit in it, caught
/// later by [`check_closed_oriented`] and better not built. The polygon's other ears are found
/// instead. If no ear can be cut at all, the input was not the simple polygon it claimed to be,
/// and the refusal says so rather than emitting a wrong mesh.
pub fn triangulate(name: &str, points: &[P2]) -> Result<Vec<[usize; 3]>, DeriveError> {
    let mut live: Vec<usize> = (0..points.len()).collect();
    let mut out = Vec::with_capacity(points.len().saturating_sub(2));
    while live.len() > 3 {
        let m = live.len();
        let mut cut = None;
        for k in 0..m {
            let (i0, i1, i2) = (live[(k + m - 1) % m], live[k], live[(k + 1) % m]);
            // reflex is not an ear, and straight-through is not one either (see above)
            if orient2(points[i0], points[i1], points[i2]) <= 0 {
                continue;
            }
            let clear = live
                .iter()
                .all(|&j| j == i0 || j == i1 || j == i2 || !point_in_triangle(points[j], points[i0], points[i1], points[i2]));
            if clear {
                cut = Some((k, [i0, i1, i2]));
                break;
            }
        }
        match cut {
            Some((k, triangle)) => {
                out.push(triangle);
                live.remove(k);
            }
            None => {
                return Err(refuse(format!(
                    "sketch {name:?}: ear clipping found no ear among {m} remaining points; the polygon is not simple"
                )));
            }
        }
    }
    let (i0, i1, i2) = (live[0], live[1], live[2]);
    if orient2(points[i0], points[i1], points[i2]) <= 0 {
        return Err(refuse(format!("sketch {name:?}: the last three points enclose no area")));
    }
    out.push([i0, i1, i2]);
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// The kernel
// ---------------------------------------------------------------------------------------------

/// The mesh of a model: the oriented triangles, canonical order, checked.
pub fn mesh(model: &Model) -> Result<Vec<Tri>, DeriveError> {
    let raw = match &model.solid {
        Node::Extrude { sketch, z0, z1 } => extrude_mesh(model, sketch, *z0, *z1)?,
        Node::Revolve { sketch, segments, directions } => revolve_mesh(model, sketch, *segments, directions.as_deref())?,
        node => csg_mesh(node)?,
    };
    let triangles = canonical_mesh(raw)?;
    check_closed_oriented(&triangles)?;
    let six_v = six_times_volume(&triangles);
    if six_v <= 0 {
        return Err(refuse(format!(
            "the mesh's exact signed volume is {six_v}/6, which is not positive; the solid is inside out or empty"
        )));
    }
    Ok(triangles)
}

fn sketch_of<'a>(model: &'a Model, name: &str) -> Result<&'a [P2], DeriveError> {
    // the grammar already refused an undeclared name, so this cannot fail on a parsed model;
    // it is still a named refusal rather than an unwrap, because a panic is not a refusal
    model.sketches.get(name).map(Vec::as_slice).ok_or_else(|| refuse(format!("sketch {name:?} is not declared")))
}

/// A prism: the sketch swept from `z0` to `z1`. Side quads wind so their normal points away
/// from the interior, the top cap is the triangulation and the bottom cap is its reverse.
fn extrude_mesh(model: &Model, name: &str, z0: i64, z1: i64) -> Result<Vec<Tri>, DeriveError> {
    let points = simple_polygon_ccw(name, sketch_of(model, name)?)?;
    let n = points.len();
    let mut out = Vec::with_capacity(2 * n + 2 * (n - 2));
    for i in 0..n {
        let a = points[i];
        let b = points[(i + 1) % n];
        push_quad(&mut out, [a[0], a[1], z0], [b[0], b[1], z0], [b[0], b[1], z1], [a[0], a[1], z1]);
    }
    for [i0, i1, i2] in triangulate(name, &points)? {
        let (a, b, c) = (points[i0], points[i1], points[i2]);
        out.push([[a[0], a[1], z1], [b[0], b[1], z1], [c[0], c[1], z1]]);
        out.push([[c[0], c[1], z0], [b[0], b[1], z0], [a[0], a[1], z0]]);
    }
    Ok(out)
}

/// The exact rational unit directions of a `segments`-sided regular ring, or the refusal Niven's
/// theorem forces (see the module doc).
pub fn regular_directions(segments: i64) -> Result<Vec<[i64; 3]>, DeriveError> {
    if i128::from(segments) != REVOLVE_SEGMENTS_EXACT {
        return Err(DeriveError::Inexact(format!(
            "revolve segments {segments}: a regular ring's vertex is (r·cos(2πk/{segments}), r·sin(2πk/{segments}), z), and \
             cos and sin of a rational multiple of π are both rational only at a quarter turn (Niven's theorem), so only \
             segments = {REVOLVE_SEGMENTS_EXACT} has an exact vertex on any fixed-point grid; give `directions` as exact \
             rational unit vectors [a, b, c] with a² + b² = c² for any other ring"
        )));
    }
    Ok(vec![[1, 0, 1], [0, 1, 1], [-1, 0, 1], [0, -1, 1]])
}

fn gcd3(a: i64, b: i64, c: i64) -> i64 {
    fn gcd(mut a: i64, mut b: i64) -> i64 {
        while b != 0 {
            let t = a % b;
            a = b;
            b = t;
        }
        a.abs()
    }
    gcd(gcd(a, b), c)
}

/// Which half of the circle a direction lies in: `0` for `[0, π)`, `1` for `[π, 2π)`. The first
/// half of the two-integer angular comparison, so that no angle is ever computed.
fn half_plane(d: [i64; 3]) -> u8 {
    if d[1] > 0 || (d[1] == 0 && d[0] > 0) { 0 } else { 1 }
}

/// Hold a `directions` list to what makes it a ring: every entry a primitive Pythagorean triple
/// with a positive denominator, and the list in strictly counter-clockwise angular order.
pub fn check_directions(directions: &[[i64; 3]]) -> Result<(), DeriveError> {
    if !(REVOLVE_DIRECTIONS_MIN..=REVOLVE_DIRECTIONS_MAX).contains(&directions.len()) {
        return Err(refuse(format!(
            "revolve directions holds {}; {REVOLVE_DIRECTIONS_MIN}..={REVOLVE_DIRECTIONS_MAX} are allowed",
            directions.len()
        )));
    }
    for (i, d) in directions.iter().enumerate() {
        let (a, b, c) = (d[0], d[1], d[2]);
        if c <= 0 {
            return Err(refuse(format!("revolve direction {i} {d:?}: the denominator c must be positive")));
        }
        if i128::from(a) * i128::from(a) + i128::from(b) * i128::from(b) != i128::from(c) * i128::from(c) {
            return Err(DeriveError::Inexact(format!(
                "revolve direction {i} {d:?}: a² + b² ≠ c², so (a/c, b/c) is not a point of the unit circle and the \
                 direction is not a rotation"
            )));
        }
        if gcd3(a, b, c) != 1 {
            return Err(refuse(format!(
                "revolve direction {i} {d:?}: give the triple in lowest terms, so that two entries are the same direction \
                 exactly when they are the same three integers"
            )));
        }
    }
    for i in 0..directions.len() - 1 {
        let (p, q) = (directions[i], directions[i + 1]);
        let cross = i128::from(p[0]) * i128::from(q[1]) - i128::from(p[1]) * i128::from(q[0]);
        let increasing = match (half_plane(p), half_plane(q)) {
            (0, 1) => true,
            (1, 0) => false,
            _ => cross > 0,
        };
        if !increasing {
            return Err(refuse(format!(
                "revolve direction {} {q:?} does not follow {p:?} counter-clockwise; the ring must be given in strictly \
                 increasing angular order and go round exactly once",
                i + 1
            )));
        }
    }
    Ok(())
}

/// `(r, z)` turned by the rational direction `[a, b, c]`, or the refusal that the product does
/// not divide. This is the one place a coordinate is a rational rather than an integer, and the
/// kernel refuses rather than rounds — ADR-0026's rule, applied to geometry.
fn revolve_vertex(r: i64, z: i64, d: [i64; 3]) -> Result<P3, DeriveError> {
    let (a, b, c) = (i128::from(d[0]), i128::from(d[1]), i128::from(d[2]));
    let r = i128::from(r);
    let mut out = [0i64, 0, z];
    for (slot, (num, axis)) in out.iter_mut().zip([(r * a, 'x'), (r * b, 'y')]) {
        if num % c != 0 {
            return Err(DeriveError::Inexact(format!(
                "revolve: the {axis} of radius {r} turned by {d:?} is {num}/{c}, which is not on the model's fixed-point \
                 grid; choose radii that are multiples of the direction's denominator"
            )));
        }
        let value = num / c;
        if value.abs() > COORD_MAX {
            return Err(DeriveError::Inexact(format!(
                "revolve: the {axis} of radius {r} turned by {d:?} is {value}, past the coordinate ceiling {COORD_MAX}"
            )));
        }
        *slot = value as i64;
    }
    Ok(out)
}

/// The sketch swept around the z axis through the given ring of directions. The sketch's points
/// are read as `(r, z)` in the half-plane `r ≥ 0`; a point on the axis makes one triangle of its
/// quad degenerate, and that triangle is dropped, which is what closes a cone at its tip.
fn revolve_mesh(model: &Model, name: &str, segments: Option<i64>, directions: Option<&[[i64; 3]]>) -> Result<Vec<Tri>, DeriveError> {
    let directions = match (segments, directions) {
        (Some(n), None) => regular_directions(n)?,
        (None, Some(d)) => d.to_vec(),
        // the grammar admits exactly one of the two; a model that reached here with neither or
        // both was not built by the grammar
        _ => return Err(refuse("revolve: exactly one of segments and directions is required".into())),
    };
    check_directions(&directions)?;
    let profile = simple_polygon_ccw(name, sketch_of(model, name)?)?;
    for (i, p) in profile.iter().enumerate() {
        if p[0] < 0 {
            return Err(refuse(format!(
                "sketch {name:?}: point {i} has radius {}, and a revolve's profile lives in the half-plane r ≥ 0",
                p[0]
            )));
        }
    }
    let m = directions.len();
    let mut out = Vec::with_capacity(2 * profile.len() * m);
    for i in 0..profile.len() {
        let p0 = profile[i];
        let p1 = profile[(i + 1) % profile.len()];
        for k in 0..m {
            let (dk, dn) = (directions[k], directions[(k + 1) % m]);
            let a = revolve_vertex(p0[0], p0[1], dk)?;
            let b = revolve_vertex(p0[0], p0[1], dn)?;
            let c = revolve_vertex(p1[0], p1[1], dn)?;
            let d = revolve_vertex(p1[0], p1[1], dk)?;
            if a != b {
                out.push([a, b, c]);
            }
            if c != d {
                out.push([a, c, d]);
            }
        }
    }
    Ok(out)
}

/// A boolean tree whose leaves are PROVED to be boxes. `csg_of` is the only way to build one,
/// so the evaluator below cannot be handed a leaf outside the class: the gate is structural and
/// not a check someone can forget to call.
enum Csg {
    Leaf(usize),
    Union(Box<Csg>, Box<Csg>),
    Intersection(Box<Csg>, Box<Csg>),
    Difference(Box<Csg>, Box<Csg>),
}

/// The class gate of ADR-0078's cad row (see the module doc): a boolean's leaves must be boxes,
/// and anything else is refused by name with the reason it is refused.
fn csg_of(node: &Node, boxes: &mut Vec<(P3, P3)>) -> Result<Csg, DeriveError> {
    match node {
        Node::Cuboid { min, max } => {
            boxes.push((*min, *max));
            if boxes.len() > BOOLEAN_LEAVES_MAX {
                return Err(refuse(format!("a boolean tree may hold at most {BOOLEAN_LEAVES_MAX} boxes")));
            }
            Ok(Csg::Leaf(boxes.len() - 1))
        }
        Node::Union(a, b) => Ok(Csg::Union(Box::new(csg_of(a, boxes)?), Box::new(csg_of(b, boxes)?))),
        Node::Intersection(a, b) => Ok(Csg::Intersection(Box::new(csg_of(a, boxes)?), Box::new(csg_of(b, boxes)?))),
        Node::Difference(a, b) => Ok(Csg::Difference(Box::new(csg_of(a, boxes)?), Box::new(csg_of(b, boxes)?))),
        other => Err(refuse(format!(
            "boolean: every leaf under a union, difference or intersection must be a `box`, and this tree holds a \
             `{}`. A boolean of general solids puts new vertices where three planes meet, whose coordinates are \
             rationals off the model's fixed-point grid; rounding one would be an ε in the binding path, which \
             ADR-0026 refuses. Axis-aligned boxes are the class whose intersection vertices are already grid points, \
             so it is the class this kernel ships the boolean for",
            other.op_name()
        ))),
    }
}

/// Is the point `centre2` (in doubled coordinates, so a cell centre is an integer and nothing is
/// halved) inside the tree? Three integer comparisons a leaf, and the set operations on top.
fn csg_inside(csg: &Csg, boxes: &[(P3, P3)], centre2: [i128; 3]) -> bool {
    match csg {
        Csg::Leaf(i) => {
            let (min, max) = boxes[*i];
            (0..3).all(|k| 2 * i128::from(min[k]) < centre2[k] && centre2[k] < 2 * i128::from(max[k]))
        }
        Csg::Union(a, b) => csg_inside(a, boxes, centre2) || csg_inside(b, boxes, centre2),
        Csg::Intersection(a, b) => csg_inside(a, boxes, centre2) && csg_inside(b, boxes, centre2),
        Csg::Difference(a, b) => csg_inside(a, boxes, centre2) && !csg_inside(b, boxes, centre2),
    }
}

/// The distinct planes the boxes induce on each axis, sorted. Every vertex of the answer is one
/// of these coordinates, which is the whole reason the class is axis-aligned boxes.
fn csg_lattice_planes(boxes: &[(P3, P3)]) -> [Vec<i64>; 3] {
    let mut planes: [Vec<i64>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for (min, max) in boxes {
        for (axis, plane) in planes.iter_mut().enumerate() {
            plane.push(min[axis]);
            plane.push(max[axis]);
        }
    }
    for axis in planes.iter_mut() {
        axis.sort_unstable();
        axis.dedup();
    }
    planes
}

/// The lattice's cell count — spelled once, so [`plan`]'s bound and [`csg_mesh`]'s loop are the
/// same number and not two that drifted.
fn csg_lattice_cells(boxes: &[(P3, P3)]) -> usize {
    let planes = csg_lattice_planes(boxes);
    (planes[0].len() - 1) * (planes[1].len() - 1) * (planes[2].len() - 1)
}

/// The boundary of a boolean of axis-aligned boxes, exactly (see the module doc). The lattice is
/// derived from the boxes' own planes, so no coordinate in the answer is new.
fn csg_mesh(node: &Node) -> Result<Vec<Tri>, DeriveError> {
    let mut boxes = Vec::new();
    let csg = csg_of(node, &mut boxes)?;
    let planes = csg_lattice_planes(&boxes);
    let counts = [planes[0].len() - 1, planes[1].len() - 1, planes[2].len() - 1];
    let cells = counts[0] * counts[1] * counts[2];
    if cells > CSG_CELLS_MAX {
        return Err(refuse(format!("the boolean's lattice holds {cells} cells; at most {CSG_CELLS_MAX}")));
    }
    let at = |i: usize, j: usize, k: usize| (i * counts[1] + j) * counts[2] + k;
    let mut inside = vec![false; cells];
    for i in 0..counts[0] {
        for j in 0..counts[1] {
            for k in 0..counts[2] {
                let centre2 = [
                    i128::from(planes[0][i]) + i128::from(planes[0][i + 1]),
                    i128::from(planes[1][j]) + i128::from(planes[1][j + 1]),
                    i128::from(planes[2][k]) + i128::from(planes[2][k + 1]),
                ];
                inside[at(i, j, k)] = csg_inside(&csg, &boxes, centre2);
            }
        }
    }
    if !inside.iter().any(|c| *c) {
        return Err(refuse("the boolean's result is empty; there is no solid to write".into()));
    }
    let mut out = Vec::new();
    for i in 0..counts[0] {
        for j in 0..counts[1] {
            for k in 0..counts[2] {
                if !inside[at(i, j, k)] {
                    continue;
                }
                let cell = [i, j, k];
                let lo = [planes[0][i], planes[1][j], planes[2][k]];
                let hi = [planes[0][i + 1], planes[1][j + 1], planes[2][k + 1]];
                for axis in 0..3 {
                    for positive in [false, true] {
                        let neighbour = if positive {
                            if cell[axis] + 1 < counts[axis] { Some(cell[axis] + 1) } else { None }
                        } else if cell[axis] > 0 {
                            Some(cell[axis] - 1)
                        } else {
                            None
                        };
                        let filled = neighbour.is_some_and(|n| {
                            let mut c = cell;
                            c[axis] = n;
                            inside[at(c[0], c[1], c[2])]
                        });
                        if !filled {
                            push_lattice_face(&mut out, axis, positive, lo, hi);
                        }
                    }
                }
            }
        }
    }
    Ok(out)
}

/// One face of a lattice cell, wound so its normal points out of the cell. `u` and `v` are the
/// other two axes in cyclic order, which is what makes the winding uniform across all six.
fn push_lattice_face(out: &mut Vec<Tri>, axis: usize, positive: bool, lo: P3, hi: P3) {
    let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
    let plane = if positive { hi[axis] } else { lo[axis] };
    let corner = |uu: i64, vv: i64| {
        let mut p = [0i64; 3];
        p[axis] = plane;
        p[u] = uu;
        p[v] = vv;
        p
    };
    let (p0, p1, p2, p3) = (corner(lo[u], lo[v]), corner(hi[u], lo[v]), corner(hi[u], hi[v]), corner(lo[u], hi[v]));
    if positive {
        push_quad(out, p0, p1, p2, p3);
    } else {
        push_quad(out, p0, p3, p2, p1);
    }
}

/// A quad as two triangles, split on the `p0 p2` diagonal so the split is a function of the
/// corner order and not of the geometry.
fn push_quad(out: &mut Vec<Tri>, p0: P3, p1: P3, p2: P3, p3: P3) {
    out.push([p0, p1, p2]);
    out.push([p0, p2, p3]);
}

// ---------------------------------------------------------------------------------------------
// The mesh's canonical form and its four checks
// ---------------------------------------------------------------------------------------------

/// The cross product of the triangle's two edges, exactly. Zero exactly when the triangle has
/// no area.
fn triangle_cross(t: &Tri) -> [i128; 3] {
    let e1 = [
        i128::from(t[1][0]) - i128::from(t[0][0]),
        i128::from(t[1][1]) - i128::from(t[0][1]),
        i128::from(t[1][2]) - i128::from(t[0][2]),
    ];
    let e2 = [
        i128::from(t[2][0]) - i128::from(t[0][0]),
        i128::from(t[2][1]) - i128::from(t[0][1]),
        i128::from(t[2][2]) - i128::from(t[0][2]),
    ];
    [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]]
}

/// Rotate a triangle so its smallest vertex comes first. A rotation is not a reflection, so the
/// winding — and with it the orientation — survives.
fn canonical_rotation(t: Tri) -> Tri {
    let start = if t[0] <= t[1] && t[0] <= t[2] {
        0
    } else if t[1] <= t[2] {
        1
    } else {
        2
    };
    [t[start], t[(start + 1) % 3], t[(start + 2) % 3]]
}

/// The mesh in the writer's canonical order, with the degenerate and the duplicated refused. A
/// duplicated facet means the kernel built an internal wall, which is not a solid's boundary.
pub fn canonical_mesh(raw: Vec<Tri>) -> Result<Vec<Tri>, DeriveError> {
    if raw.is_empty() {
        return Err(refuse("the kernel produced no triangles; there is no solid to write".into()));
    }
    if raw.len() > TRIANGLES_MAX {
        return Err(refuse(format!("the mesh holds {} triangles; at most {TRIANGLES_MAX}", raw.len())));
    }
    let mut out = Vec::with_capacity(raw.len());
    for t in raw {
        if triangle_cross(&t) == [0, 0, 0] {
            return Err(refuse(format!("triangle {t:?} has no area")));
        }
        out.push(canonical_rotation(t));
    }
    out.sort_unstable();
    for pair in out.windows(2) {
        if pair[0] == pair[1] {
            return Err(refuse(format!("triangle {:?} appears twice; the mesh holds an internal wall", pair[0])));
        }
    }
    Ok(out)
}

/// Every directed edge exactly once, and its reverse exactly once: the boundary is a closed,
/// consistently oriented manifold. Exact, integer, and it costs one map.
pub fn check_closed_oriented(triangles: &[Tri]) -> Result<(), DeriveError> {
    let mut edges: BTreeMap<(P3, P3), u32> = BTreeMap::new();
    for t in triangles {
        for k in 0..3 {
            *edges.entry((t[k], t[(k + 1) % 3])).or_insert(0) += 1;
        }
    }
    for ((a, b), count) in &edges {
        if *count != 1 {
            return Err(refuse(format!(
                "the directed edge {a:?} → {b:?} appears {count} times; a closed oriented surface uses each exactly once"
            )));
        }
        if edges.get(&(*b, *a)).copied().unwrap_or(0) != 1 {
            return Err(refuse(format!("the edge {a:?} → {b:?} has no matching {b:?} → {a:?}; the surface is not closed")));
        }
    }
    Ok(())
}

/// Six times the mesh's signed volume, exactly: the integer sum of `det(v0, v1, v2)` over the
/// triangles (the divergence theorem on a closed surface). Positive for an outward orientation.
pub fn six_times_volume(triangles: &[Tri]) -> i128 {
    let mut sum = 0i128;
    for t in triangles {
        let (a, b, c) = (t[0], t[1], t[2]);
        let (ax, ay, az) = (i128::from(a[0]), i128::from(a[1]), i128::from(a[2]));
        let (bx, by, bz) = (i128::from(b[0]), i128::from(b[1]), i128::from(b[2]));
        let (cx, cy, cz) = (i128::from(c[0]), i128::from(c[1]), i128::from(c[2]));
        sum += ax * (by * cz - bz * cy) - ay * (bx * cz - bz * cx) + az * (bx * cy - by * cx);
    }
    sum
}

// ---------------------------------------------------------------------------------------------
// The writer
// ---------------------------------------------------------------------------------------------

/// The 80 header bytes: [`STL_HEADER_TEXT`], zero-padded. Not a clock, not a build string, not a
/// program name — the classic determinism leak of every CAD exporter lives in exactly these
/// bytes, and this is where it is closed.
pub fn stl_header() -> [u8; 80] {
    let mut header = [0u8; 80];
    let text = STL_HEADER_TEXT.as_bytes();
    header[..text.len()].copy_from_slice(text);
    header
}

/// The canonical binary STL of a canonical mesh.
pub fn write_stl(triangles: &[Tri], frac_bits: u32) -> Result<Vec<u8>, DeriveError> {
    if triangles.len() > TRIANGLES_MAX {
        return Err(refuse(format!("the mesh holds {} triangles; at most {TRIANGLES_MAX}", triangles.len())));
    }
    let mut out = Vec::with_capacity(STL_PREAMBLE_BYTES + triangles.len() * STL_FACET_BYTES);
    out.extend_from_slice(&stl_header());
    put_u32_le(&mut out, triangles.len() as u32);
    for t in triangles {
        // the facet normal: three zero values, the format's "derive it from the winding"
        // convention, because an exact unit normal needs a square root and this kernel does not
        // round (see the module doc)
        out.extend_from_slice(&[0u8; 12]);
        for vertex in t {
            for coordinate in vertex {
                out.extend_from_slice(&f32_le_exact(*coordinate, frac_bits)?);
            }
        }
        put_u16_le(&mut out, STL_ATTRIBUTE_BYTE_COUNT);
    }
    // [`plan`] already refused anything that could reach here; this is the writer's own
    // statement of the same bound, so the ceiling holds even if the writer is called directly.
    if out.len() > MAX_ARTIFACT_BYTES {
        return Err(refuse(format!("artifact is {} bytes; at most {MAX_ARTIFACT_BYTES} (ADR-0078 SA-2)", out.len())));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::derive::{ClaimBinding, derive_named, derive_with};
    use crate::ids::{artifact_hash_v1, dsl_hash_v1, grammar_id_v1, transformer_id};
    use kaspa_consensus_core::palw_derived_v1::PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN;
    use kaspa_hashes::Hash64;
    use std::path::{Path, PathBuf};

    /// An L bracket: one reflex vertex, which is the cheapest thing an ear clipper can get wrong.
    const BRACKET: &str = r#"{
        "v": 1, "frac_bits": 0,
        "sketches": { "bracket": [[0,0],[4,0],[4,2],[2,2],[2,4],[0,4]] },
        "solid": { "op": "extrude", "sketch": "bracket", "z0": 0, "z1": 3 }
    }"#;

    /// A box with a blind square hole — a `difference` whose result is a solid with a floor.
    const NOTCHED: &str = r#"{
        "v": 1, "frac_bits": 0, "sketches": {},
        "solid": { "op": "difference",
                   "a": { "op": "box", "min": [0,0,0], "max": [8,8,8] },
                   "b": { "op": "box", "min": [2,2,4], "max": [6,6,12] } }
    }"#;

    /// The only exact regular ring there is.
    const QUARTERS: &str = r#"{
        "v": 1, "frac_bits": 0,
        "sketches": { "profile": [[2,0],[6,0],[6,4],[2,4]] },
        "solid": { "op": "revolve", "sketch": "profile", "segments": 4 }
    }"#;

    /// The twelve rational directions the (3,4,5) triple and the axes give, counter-clockwise.
    const TWELVE: &str = "[[1,0,1],[4,3,5],[3,4,5],[0,1,1],[-3,4,5],[-4,3,5],[-1,0,1],[-4,-3,5],[-3,-4,5],[0,-1,1],[3,-4,5],[4,-3,5]]";

    fn washer() -> String {
        format!(
            r#"{{"v":1,"frac_bits":0,"sketches":{{"washer":[[5,0],[10,0],[10,8],[5,8]]}},
                 "solid":{{"op":"revolve","sketch":"washer","directions":{TWELVE}}}}}"#
        )
    }

    fn binding() -> ClaimBinding {
        ClaimBinding {
            network_domain: Hash64::from_bytes([1u8; 64]),
            claim_id: Hash64::from_bytes([2u8; 64]),
            output_root: Hash64::from_bytes([3u8; 64]),
            executor_pubkey: vec![7u8; PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN],
        }
    }

    fn canonical(answer: &str) -> Vec<u8> {
        CadGrammar.canonicalize(answer.as_bytes()).unwrap_or_else(|e| panic!("{answer}\n refused: {e}"))
    }

    fn artifact(answer: &str) -> Vec<u8> {
        CadStlTransformer.run(&canonical(answer)).unwrap_or_else(|e| panic!("{answer}\n refused: {e}")).bytes
    }

    fn model(answer: &str) -> Model {
        canonical_model(&canonical(answer)).unwrap()
    }

    fn triangles(answer: &str) -> Vec<Tri> {
        mesh(&model(answer)).unwrap_or_else(|e| panic!("{answer}\n refused: {e}"))
    }

    /// `BRACKET` with one substring replaced — how every refusal below is built.
    fn bracket_with(from: &str, to: &str) -> String {
        assert!(BRACKET.contains(from), "{from:?} is not in the sample");
        BRACKET.replacen(from, to, 1)
    }

    #[track_caller]
    fn refused(answer: &str, fragment: &str) {
        match CadGrammar.canonicalize(answer.as_bytes()) {
            Err(DeriveError::Grammar(msg)) => assert!(msg.contains(fragment), "refusal {msg:?} does not mention {fragment:?}"),
            other => panic!("expected a grammar refusal mentioning {fragment:?}, got {other:?}"),
        }
    }

    /// The grammar admits it; the kernel does not. The two halves of "no object" (X4).
    #[track_caller]
    fn kernel_refuses(answer: &str, fragment: &str) {
        match CadStlTransformer.run(&canonical(answer)) {
            Err(DeriveError::Transformer(msg)) => {
                assert!(msg.contains(fragment), "refusal {msg:?} does not mention {fragment:?}")
            }
            other => panic!("expected a transformer refusal mentioning {fragment:?}, got {other:?}"),
        }
    }

    /// The kernel refuses because a value is not exactly representable — never rounds it.
    #[track_caller]
    fn inexact(answer: &str, fragment: &str) {
        match CadStlTransformer.run(&canonical(answer)) {
            Err(DeriveError::Inexact(msg)) => assert!(msg.contains(fragment), "refusal {msg:?} does not mention {fragment:?}"),
            other => panic!("expected an inexactness refusal mentioning {fragment:?}, got {other:?}"),
        }
    }

    // ---- (1) the grammar's canonical form ---------------------------------------------------

    #[test]
    fn canonical_form_sorts_keys_strips_whitespace_and_is_idempotent() {
        let once = canonical(BRACKET);
        let expected = br#"{"frac_bits":0,"sketches":{"bracket":[[0,0],[4,0],[4,2],[2,2],[2,4],[0,4]]},"solid":{"op":"extrude","sketch":"bracket","z0":0,"z1":3},"v":1}"#;
        assert_eq!(std::str::from_utf8(&once).unwrap(), std::str::from_utf8(expected).unwrap());
        assert_eq!(CadGrammar.canonicalize(&once).unwrap(), once);
    }

    #[test]
    fn registration_and_manifest() {
        let (grammars, transformers) = register();
        assert_eq!(grammars.len(), 1);
        assert_eq!(transformers.len(), 1);
        assert_eq!(grammars[0].name(), GRAMMAR_NAME);
        let m = transformers[0].manifest();
        assert_eq!(m.name, "cad/stl/v1");
        assert_eq!(m.kind, kind::CAD);
        assert_eq!(m.kind, 3);
        assert_eq!(m.grammar, "cad/v1");
        // ADR-0078 Decision 3's second discipline, and the only kind in the tree that declares it
        assert_eq!(m.discipline, Discipline::ExactRational);
        assert_eq!(m.writer, "stl-binary/1.0/zero-normal-rh-winding-sorted-v1");
        assert_eq!(m.source_tree_sha256, crate::SOURCE_TREE_SHA256_HEX);
        assert!(crate::registry::transformer_by_name(TRANSFORMER_NAME).is_some());
        assert!(crate::registry::grammar_by_name(GRAMMAR_NAME).is_some());
        assert!(crate::registry::transformer_by_id(&transformer_id(&m)).is_some());
        let a = CadStlTransformer.run(&canonical(BRACKET)).unwrap();
        assert_eq!((a.media_type, a.extension), ("model/stl", "stl"));
    }

    // ---- (2) every schema refusal ------------------------------------------------------------

    #[test]
    fn refuses_what_is_not_the_schema() {
        refused("[1]", "top level is not an object");
        refused("{", "json");
        refused(r#"{"v":1.5}"#, "non-integer");
        refused(&bracket_with(r#""v": 1,"#, r#""v": 1, "units": "mm","#), "unknown key \"units\"");
        refused(&bracket_with(r#""frac_bits": 0,"#, ""), "missing key \"frac_bits\"");
        refused(&bracket_with(r#""v": 1,"#, r#""v": 2,"#), "v must be 1, not 2");
        refused(&bracket_with(r#""v": 1,"#, r#""v": "1","#), "v must be an integer");
        refused(&bracket_with(r#""frac_bits": 0,"#, r#""frac_bits": 17,"#), "frac_bits 17 is outside 0..=16");
        refused(&bracket_with(r#""frac_bits": 0,"#, r#""frac_bits": -1,"#), "frac_bits -1 is outside");
        refused(
            r#"{"v":1,"frac_bits":0,"sketches":[],"solid":{"op":"box","min":[0,0,0],"max":[1,1,1]}}"#,
            "sketches is not an object",
        );
        refused(r#"{"v":1,"frac_bits":0,"sketches":{},"solid":4}"#, "solid is not an object");
        refused(&bracket_with(r#""bracket": [[0,0]"#, r#""": [[0,0]"#), "is 0 bytes");
        refused(&bracket_with(r#""bracket":"#, &format!(r#""{}":"#, "x".repeat(65))), "is 65 bytes");
        refused(&bracket_with(r#"[[0,0],[4,0],[4,2],[2,2],[2,4],[0,4]]"#, r#"[[0,0],[4,0]]"#), "holds 2 points; 3..=256");
        refused(&bracket_with(r#"[[0,0],[4,0],[4,2],[2,2],[2,4],[0,4]]"#, "7"), "must be an array of points");
        refused(&bracket_with("[0,0]", "[0,0,0]"), "must hold 2 integers, not 3");
        refused(&bracket_with("[0,0]", r#"["a",0]"#), "must be an integer");
        refused(&bracket_with("[0,0]", "[16777216,0]"), "is outside -16777215..=16777215");
        refused(&bracket_with(r#""op": "extrude""#, r#""op": "loft""#), "unknown op \"loft\"");
        refused(&bracket_with(r#""op": "extrude""#, r#""op": 4"#), "op must be a string");
        refused(&bracket_with(r#""sketch": "bracket""#, r#""sketch": "flange""#), "sketch \"flange\" is not declared");
        refused(&bracket_with(r#""z0": 0, "z1": 3"#, r#""z0": 3, "z1": 3"#), "z0 3 is not below z1 3");
        refused(&bracket_with(r#""z0": 0, "z1": 3"#, r#""z1": 3"#), "missing key \"z0\"");
        // boxes
        let boxed = |min: &str, max: &str| {
            format!(r#"{{"v":1,"frac_bits":0,"sketches":{{}},"solid":{{"op":"box","min":{min},"max":{max}}}}}"#)
        };
        refused(&boxed("[0,0,0]", "[0,1,1]"), "min x 0 is not below max x 0");
        refused(&boxed("[0,0,0]", "[1,0,1]"), "min y 0 is not below max y 0");
        refused(&boxed("[0,0,0]", "[1,1,0]"), "min z 0 is not below max z 0");
        refused(&boxed("[0,0]", "[1,1,1]"), "must hold 3 integers, not 2");
        assert!(CadGrammar.canonicalize(boxed("[0,0,0]", "[1,1,1]").as_bytes()).is_ok());
        // revolves
        let rev = |body: &str| {
            format!(
                r#"{{"v":1,"frac_bits":0,"sketches":{{"p":[[2,0],[6,0],[6,4],[2,4]]}},"solid":{{"op":"revolve","sketch":"p"{body}}}}}"#
            )
        };
        refused(&rev(""), "give exactly one of segments and directions");
        refused(&rev(r#","segments":4,"directions":[[1,0,1],[0,1,1],[-1,0,1]]"#), "segments and directions are alternatives");
        refused(&rev(r#","segments":2"#), "segments 2 is outside 3..=64");
        refused(&rev(r#","segments":65"#), "segments 65 is outside 3..=64");
        refused(&rev(r#","directions":{"a":1}"#), "directions must be an array");
        refused(&rev(r#","directions":[[1,0,1],[0,1,1]]"#), "directions holds 2; 3..=64");
        refused(&rev(r#","directions":[[1,0],[0,1,1],[-1,0,1]]"#), "must hold 3 integers, not 2");
    }

    #[test]
    fn refuses_more_sketches_and_deeper_trees_than_the_schema_allows() {
        let sketch = r#"[[0,0],[1,0],[0,1]]"#;
        let sketches = |n: usize| {
            let body: Vec<String> = (0..n).map(|i| format!(r#""s{i}":{sketch}"#)).collect();
            format!(
                r#"{{"v":1,"frac_bits":0,"sketches":{{{}}},"solid":{{"op":"extrude","sketch":"s0","z0":0,"z1":1}}}}"#,
                body.join(",")
            )
        };
        assert!(CadGrammar.canonicalize(sketches(SKETCHES_MAX).as_bytes()).is_ok());
        refused(&sketches(SKETCHES_MAX + 1), &format!("holds {} sketches; at most {SKETCHES_MAX}", SKETCHES_MAX + 1));

        // a right-leaning chain of unions, one box a level
        let nest = |depth: usize| {
            let leaf = |i: usize| format!(r#"{{"op":"box","min":[{i},0,0],"max":[{},1,1]}}"#, i + 1);
            let mut node = leaf(0);
            for i in 1..=depth {
                node = format!(r#"{{"op":"union","a":{},"b":{}}}"#, leaf(i), node);
            }
            format!(r#"{{"v":1,"frac_bits":0,"sketches":{{}},"solid":{node}}}"#)
        };
        assert!(CadGrammar.canonicalize(nest(SOLID_DEPTH_MAX).as_bytes()).is_ok());
        refused(&nest(SOLID_DEPTH_MAX + 1), &format!("nests deeper than {SOLID_DEPTH_MAX}"));
    }

    // ---- (3) the bounds ADR-0078 SA-2 asks for ----------------------------------------------

    #[test]
    fn the_bounds_are_declared_once_and_the_derived_ones_follow_from_them() {
        assert_eq!(BOUNDS, CadBounds { max_dsl_bytes: 65_536, max_artifact_bytes: 1_048_576, max_steps: 4_000_000 });
        // the triangle ceiling is the artifact ceiling, in facets
        assert_eq!(TRIANGLES_MAX, (MAX_ARTIFACT_BYTES - STL_PREAMBLE_BYTES) / STL_FACET_BYTES);
        assert_eq!(STL_PREAMBLE_BYTES + TRIANGLES_MAX * STL_FACET_BYTES, 1_048_534);
        // and the boolean's leaf count is the largest whose WORST lattice still fits it, so a
        // boolean the grammar admits is never refused by the ceiling
        assert!(csg_cells_bound(BOOLEAN_LEAVES_MAX) * CSG_FACES_PER_CELL * TRIANGLES_PER_FACE <= TRIANGLES_MAX);
        assert!(csg_cells_bound(BOOLEAN_LEAVES_MAX + 1) * CSG_FACES_PER_CELL * TRIANGLES_PER_FACE > TRIANGLES_MAX);
        assert_eq!(BOOLEAN_LEAVES_MAX, 6);
        assert_eq!(CSG_CELLS_MAX, 11 * 11 * 11);
    }

    #[test]
    fn the_dsl_bound_is_taken_before_the_parser() {
        // bytes that are not JSON at all: the refusal must still be the size one, because the
        // size is checked before anything reads them (SA-2)
        let huge = vec![b'{'; MAX_DSL_BYTES + 1];
        match CadGrammar.canonicalize(&huge) {
            Err(DeriveError::Grammar(msg)) => {
                assert!(msg.contains(&format!("the answer is {} bytes", MAX_DSL_BYTES + 1)), "{msg}");
                assert!(msg.contains("SA-2"), "{msg}");
            }
            other => panic!("expected the size refusal, got {other:?}"),
        }
        match CadStlTransformer.run(&huge) {
            Err(DeriveError::Transformer(msg)) => assert!(msg.contains("at most 65536"), "{msg}"),
            other => panic!("expected the size refusal, got {other:?}"),
        }
        // one byte under is a parse refusal, which proves the check above was the size and not
        // the parser
        let nearly = vec![b'{'; MAX_DSL_BYTES];
        match CadGrammar.canonicalize(&nearly) {
            Err(DeriveError::Grammar(msg)) => assert!(msg.contains("json"), "{msg}"),
            other => panic!("expected a parse refusal, got {other:?}"),
        }
    }

    #[test]
    fn the_step_and_artifact_bounds_are_answered_from_the_dsl_before_the_kernel_runs() {
        // an extrude whose triangulation is the cube of its point count
        let comb = |n: usize| {
            let pts: Vec<String> = (0..n - 2).map(|x| format!("[{x},0]")).collect();
            format!(
                r#"{{"v":1,"frac_bits":0,"sketches":{{"c":[{},[{},50],[0,50]]}},"solid":{{"op":"extrude","sketch":"c","z0":0,"z1":4}}}}"#,
                pts.join(","),
                n - 3
            )
        };
        let steps_of = |n: usize| plan(&model(&comb(n))).unwrap().steps;
        assert_eq!(steps_of(10), 100 + 1000 + 40);
        assert!(steps_of(158) <= MAX_STEPS, "158 points is inside the budget: {}", steps_of(158));
        assert!(steps_of(159) > MAX_STEPS, "159 points is outside it: {}", steps_of(159));
        assert!(CadStlTransformer.run(&canonical(&comb(158))).is_ok());
        kernel_refuses(&comb(159), "the step budget is 4000000");
        kernel_refuses(&comb(159), "ADR-0078 SA-2");

        // a revolve is cheap in predicates and expensive in triangles: the other bound catches it
        let profile: Vec<String> = (0..SKETCH_POINTS_MAX - 2).map(|z| format!("[{},{z}]", 5 + z % 3)).collect();
        let big_revolve = format!(
            r#"{{"v":1,"frac_bits":0,"sketches":{{"t":[{},[40,254],[40,0]]}},"solid":{{"op":"revolve","sketch":"t","directions":{}}}}}"#,
            profile.join(","),
            format_args!(
                "[{}]",
                (0..REVOLVE_DIRECTIONS_MAX).map(|k| format!("[{},0,1]", 1 + k as i64 % 2)).collect::<Vec<_>>().join(",")
            )
        );
        let p = plan(&model(&big_revolve)).unwrap();
        assert_eq!(p.triangles, 2 * 256 * 64);
        assert!(p.steps <= MAX_STEPS, "the step budget is not what refuses this one: {}", p.steps);
        assert!(p.artifact_bytes > MAX_ARTIFACT_BYTES as u64);
        kernel_refuses(&big_revolve, "the artifact ceiling is 1048576");
    }

    #[test]
    fn the_plan_is_an_upper_bound_on_what_the_kernel_then_does() {
        for answer in [
            BRACKET,
            NOTCHED,
            QUARTERS,
            &washer(),
            r#"{"v":1,"frac_bits":0,"sketches":{},"solid":{"op":"box","min":[0,0,0],"max":[3,3,3]}}"#,
        ] {
            let m = model(answer);
            let p = plan(&m).unwrap();
            let built = mesh(&m).unwrap();
            assert!(
                built.len() as u64 <= p.triangles,
                "the plan promised {} triangles and the kernel built {}",
                p.triangles,
                built.len()
            );
            let bytes = write_stl(&built, m.frac_bits).unwrap();
            assert!(bytes.len() as u64 <= p.artifact_bytes);
            assert_eq!(bytes.len(), STL_PREAMBLE_BYTES + built.len() * STL_FACET_BYTES);
        }
        // an extrude's triangle count is exact, not merely bounded
        assert_eq!(plan(&model(BRACKET)).unwrap().triangles, 4 * 6 - 4);
        assert_eq!(triangles(BRACKET).len(), 4 * 6 - 4);
    }

    // ---- (4) the kernel: extrude ------------------------------------------------------------

    /// A prism's exact volume is the polygon's exact area times its height, and
    /// `six_times_volume` is six of them — an integer identity with no tolerance in it.
    #[track_caller]
    fn assert_prism_volume(answer: &str, area2: i128, height: i128) {
        let t = triangles(answer);
        check_closed_oriented(&t).unwrap();
        assert_eq!(six_times_volume(&t), 3 * area2 * height);
    }

    #[test]
    fn an_extrude_is_a_closed_solid_whose_exact_volume_is_the_area_times_the_height() {
        assert_eq!(signed_area2(&[[0, 0], [4, 0], [4, 2], [2, 2], [2, 4], [0, 4]]), 24);
        assert_prism_volume(BRACKET, 24, 3);
        // the same outline wound the other way is the same solid: the kernel reads the winding
        let clockwise = bracket_with("[[0,0],[4,0],[4,2],[2,2],[2,4],[0,4]]", "[[0,4],[2,4],[2,2],[4,2],[4,0],[0,0]]");
        assert_eq!(triangles(&clockwise), triangles(BRACKET));
    }

    #[test]
    fn a_point_that_does_not_turn_stays_in_the_cap_and_the_prism_stays_closed() {
        // the middle point of the bottom edge is straight through: clipping it would emit a
        // triangle with no area and dropping it would leave the side wall's vertex out of the
        // cap, which is a prism with a slit in it
        let plate = r#"{"v":1,"frac_bits":4,
            "sketches":{"plate":[[0,0],[16,0],[32,0],[32,32],[16,32],[0,32]]},
            "solid":{"op":"extrude","sketch":"plate","z0":0,"z1":16}}"#;
        assert_prism_volume(plate, 2048, 16);
        let t = triangles(plate);
        assert_eq!(t.len(), 4 * 6 - 4);
        // every point of the sketch is a vertex of the mesh, at both heights
        let vertices: BTreeSet<P3> = t.iter().flatten().copied().collect();
        for p in [[0, 0], [16, 0], [32, 0], [32, 32], [16, 32], [0, 32]] {
            for z in [0, 16] {
                assert!(vertices.contains(&[p[0], p[1], z]), "the cap lost {p:?} at z={z}");
            }
        }
        assert_eq!(triangulate("plate", &[[0, 0], [16, 0], [32, 0], [32, 32], [16, 32], [0, 32]]).unwrap().len(), 4);
    }

    #[test]
    fn a_sketch_that_is_not_a_simple_polygon_is_refused_by_name() {
        kernel_refuses(&bracket_with("[2,2],[2,4]", "[2,2],[0,0]"), "repeats an earlier point");
        kernel_refuses(&bracket_with("[[0,0],[4,0],[4,2],[2,2],[2,4],[0,4]]", "[[0,0],[4,0],[2,0],[4,4]]"), "fold back");
        // a bow tie: two edges that are not adjacent cross
        kernel_refuses(&bracket_with("[[0,0],[4,0],[4,2],[2,2],[2,4],[0,4]]", "[[0,0],[4,4],[4,0],[0,4]]"), "cross");
        kernel_refuses(&bracket_with("[[0,0],[4,0],[4,2],[2,2],[2,4],[0,4]]", "[[0,0],[1,0],[2,0]]"), "fold back");
    }

    // ---- (4b) the kernel: revolve, and the theorem that bounds it ---------------------------

    #[test]
    fn only_the_quarter_turn_is_an_exact_regular_ring() {
        assert_eq!(regular_directions(4).unwrap(), vec![[1, 0, 1], [0, 1, 1], [-1, 0, 1], [0, -1, 1]]);
        for n in [3i64, 5, 6, 8, 12, 64] {
            match regular_directions(n) {
                Err(DeriveError::Inexact(msg)) => assert!(msg.contains("Niven"), "{msg}"),
                other => panic!("segments {n} must be refused by name, got {other:?}"),
            }
        }
        inexact(&QUARTERS.replace(r#""segments": 4"#, r#""segments": 6"#), "Niven");
        // and the ring it does ship is a solid: the prism over the annulus between two diamonds
        // of circumradius 6 and 2, whose areas are 2r², so 72 − 8 = 64 at a height of 4
        let t = triangles(QUARTERS);
        check_closed_oriented(&t).unwrap();
        assert_eq!(six_times_volume(&t), 6 * 64 * 4);
        assert_eq!(t.len(), 2 * 4 * 4);
    }

    #[test]
    fn a_directions_ring_must_be_exact_lowest_terms_and_counter_clockwise() {
        assert!(check_directions(&[[1, 0, 1], [0, 1, 1], [-1, 0, 1], [0, -1, 1]]).is_ok());
        let ring = |d: &str| washer().replace(TWELVE, d);
        inexact(&ring("[[3,4,6],[0,1,1],[-1,0,1]]"), "a² + b² ≠ c²");
        kernel_refuses(&ring("[[6,8,10],[0,1,1],[-1,0,1]]"), "lowest terms");
        kernel_refuses(&ring("[[1,0,-1],[0,1,1],[-1,0,1]]"), "denominator c must be positive");
        kernel_refuses(&ring("[[0,1,1],[1,0,1],[-1,0,1]]"), "counter-clockwise");
        kernel_refuses(&ring("[[1,0,1],[0,1,1],[0,1,1]]"), "counter-clockwise");
    }

    #[test]
    fn a_vertex_that_is_not_on_the_grid_is_refused_rather_than_rounded() {
        // the (3,4,5) directions need radii that are multiples of five; three is not one
        let off_grid = washer().replace("[[5,0],[10,0],[10,8],[5,8]]", "[[3,0],[10,0],[10,8],[3,8]]");
        inexact(&off_grid, "not on the model's fixed-point grid");
        assert!(matches!(revolve_vertex(3, 0, [3, 4, 5]), Err(DeriveError::Inexact(_))));
        assert_eq!(revolve_vertex(5, 7, [3, 4, 5]).unwrap(), [3, 4, 7]);
        assert_eq!(revolve_vertex(10, -1, [-4, 3, 5]).unwrap(), [-8, 6, -1]);
    }

    #[test]
    fn the_twelve_direction_washer_is_the_exact_prism_over_its_annulus() {
        let t = triangles(&washer());
        check_closed_oriented(&t).unwrap();
        assert_eq!(t.len(), 2 * 4 * 12);
        // a 12-gon on these directions has area 74r²/25 (the sum of the twelve exact crosses is
        // 148/25), so the annulus between r = 10 and r = 5 is 296 − 74 = 222, at a height of 8
        assert_eq!(six_times_volume(&t), 6 * 222 * 8);
    }

    #[test]
    fn a_profile_off_the_half_plane_is_refused() {
        kernel_refuses(&QUARTERS.replace("[[2,0],[6,0],[6,4],[2,4]]", "[[-2,0],[6,0],[6,4],[-2,4]]"), "half-plane r ≥ 0");
    }

    // ---- (4c) the kernel: the boolean, which is exact or absent -----------------------------

    #[test]
    fn the_boolean_ships_for_boxes_and_refuses_everything_else_by_name() {
        let with_extrude = r#"{"v":1,"frac_bits":0,
            "sketches":{"s":[[0,0],[4,0],[4,4],[0,4]]},
            "solid":{"op":"union","a":{"op":"box","min":[0,0,0],"max":[2,2,2]},
                                 "b":{"op":"extrude","sketch":"s","z0":0,"z1":1}}}"#;
        kernel_refuses(with_extrude, "must be a `box`, and this tree holds a `extrude`");
        kernel_refuses(with_extrude, "ADR-0026 refuses");
        let with_revolve = r#"{"v":1,"frac_bits":0,
            "sketches":{"s":[[2,0],[6,0],[6,4],[2,4]]},
            "solid":{"op":"difference","a":{"op":"box","min":[0,0,0],"max":[9,9,9]},
                                       "b":{"op":"revolve","sketch":"s","segments":4}}}"#;
        kernel_refuses(with_revolve, "holds a `revolve`");
    }

    #[test]
    fn the_boolean_is_exact_on_the_lattice_the_boxes_own_planes_induce() {
        // 8³ minus the part of [2,6]×[2,6]×[4,12] that is inside it: 512 − 64
        let t = triangles(NOTCHED);
        check_closed_oriented(&t).unwrap();
        assert_eq!(six_times_volume(&t), 6 * (512 - 64));
        // every vertex is a coordinate that was already in the answer — the point of the class
        let planes: BTreeSet<i64> = [0, 2, 4, 6, 8].into_iter().collect();
        for v in t.iter().flatten() {
            for c in v {
                assert!(planes.contains(c), "the boolean invented the coordinate {c}");
            }
        }
        // union of an intersection and a box: |[3,6]³| + |[5,10]³| − |[5,6]³| = 27 + 125 − 1
        let nested = r#"{"v":1,"frac_bits":0,"sketches":{},
            "solid":{"op":"union",
                     "a":{"op":"intersection","a":{"op":"box","min":[0,0,0],"max":[6,6,6]},
                                              "b":{"op":"box","min":[3,3,3],"max":[9,9,9]}},
                     "b":{"op":"box","min":[5,5,5],"max":[10,10,10]}}}"#;
        let t = triangles(nested);
        check_closed_oriented(&t).unwrap();
        assert_eq!(six_times_volume(&t), 6 * (27 + 125 - 1));
        // a bare box is the same machinery with one leaf
        let cube = r#"{"v":1,"frac_bits":2,"sketches":{},"solid":{"op":"box","min":[-6,-6,-6],"max":[6,6,6]}}"#;
        let t = triangles(cube);
        assert_eq!(t.len(), 12);
        assert_eq!(six_times_volume(&t), 6 * 12 * 12 * 12);
    }

    #[test]
    fn a_boolean_whose_result_is_not_a_solid_is_refused_and_not_written() {
        // two cubes that meet along one edge: the surface is not a manifold there, and the
        // kernel says so rather than emitting a mesh no consumer could read
        let edge_touch = r#"{"v":1,"frac_bits":0,"sketches":{},
            "solid":{"op":"union","a":{"op":"box","min":[0,0,0],"max":[1,1,1]},
                                  "b":{"op":"box","min":[1,1,0],"max":[2,2,1]}}}"#;
        kernel_refuses(edge_touch, "appears 2 times");
        let disjoint = r#"{"v":1,"frac_bits":0,"sketches":{},
            "solid":{"op":"intersection","a":{"op":"box","min":[0,0,0],"max":[1,1,1]},
                                         "b":{"op":"box","min":[5,5,5],"max":[6,6,6]}}}"#;
        kernel_refuses(disjoint, "the boolean's result is empty");
        // the whole of a difference removed
        let erased = r#"{"v":1,"frac_bits":0,"sketches":{},
            "solid":{"op":"difference","a":{"op":"box","min":[0,0,0],"max":[1,1,1]},
                                       "b":{"op":"box","min":[-1,-1,-1],"max":[2,2,2]}}}"#;
        kernel_refuses(erased, "the boolean's result is empty");
    }

    #[test]
    fn a_boolean_holds_at_most_the_leaves_the_artifact_ceiling_allows() {
        let boxes = |n: usize| {
            let leaf = |i: usize| format!(r#"{{"op":"box","min":[{},0,0],"max":[{},1,1]}}"#, 2 * i, 2 * i + 1);
            let mut node = leaf(0);
            for i in 1..n {
                node = format!(r#"{{"op":"union","a":{node},"b":{}}}"#, leaf(i));
            }
            format!(r#"{{"v":1,"frac_bits":0,"sketches":{{}},"solid":{node}}}"#)
        };
        assert!(CadStlTransformer.run(&canonical(&boxes(BOOLEAN_LEAVES_MAX))).is_ok());
        kernel_refuses(&boxes(BOOLEAN_LEAVES_MAX + 1), &format!("at most {BOOLEAN_LEAVES_MAX} boxes"));
    }

    // ---- (5) the writer's three free fields, pinned -----------------------------------------

    /// The exact integer mantissa a binary32 word holds, in units of `2^-frac_bits`. Integer
    /// arithmetic, so the test that checks the writer is under the writer's own discipline.
    fn decode_exact(bits: u32, frac_bits: u32) -> i64 {
        if bits == 0 {
            return 0;
        }
        let biased = ((bits >> 23) & 0xFF) as i32;
        assert!((1..=254).contains(&biased), "the artifact holds a subnormal, an infinity or a NaN");
        let mantissa = u64::from((1u32 << 23) | (bits & 0x7F_FFFF));
        let shift = biased - 127 - 23 + frac_bits as i32;
        let magnitude: i128 = if shift >= 0 {
            i128::from(mantissa) << shift
        } else {
            let drop = (-shift) as u32;
            assert!(drop < 64 && mantissa.trailing_zeros() >= drop, "the artifact holds a value off the grid");
            i128::from(mantissa >> drop)
        };
        (if bits >> 31 == 1 { -magnitude } else { magnitude }) as i64
    }

    /// The facets of a binary STL, decoded back to the exact integer vertices that made them.
    fn walk(bytes: &[u8], frac_bits: u32) -> Vec<Tri> {
        assert!(bytes.len() >= STL_PREAMBLE_BYTES);
        assert_eq!(&bytes[..80], &stl_header()[..], "the header is not the pinned one");
        assert!(!bytes.starts_with(b"solid"), "a binary STL whose header starts with `solid` is sniffed as ASCII");
        let count = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
        assert_eq!(bytes.len(), STL_PREAMBLE_BYTES + count * STL_FACET_BYTES, "the facet count does not match the length");
        let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        let mut out = Vec::with_capacity(count);
        for f in 0..count {
            let base = STL_PREAMBLE_BYTES + f * STL_FACET_BYTES;
            assert_eq!(&bytes[base..base + 12], &[0u8; 12], "facet {f} carries a normal; the writer pins it to zero");
            let mut tri = [[0i64; 3]; 3];
            for (v, vertex) in tri.iter_mut().enumerate() {
                for (c, coordinate) in vertex.iter_mut().enumerate() {
                    *coordinate = decode_exact(word(base + 12 + v * 12 + c * 4), frac_bits);
                }
            }
            assert_eq!(
                u16::from_le_bytes(bytes[base + 48..base + 50].try_into().unwrap()),
                STL_ATTRIBUTE_BYTE_COUNT,
                "facet {f} carries an attribute count"
            );
            out.push(tri);
        }
        out
    }

    #[test]
    fn the_bytes_are_the_mesh_the_kernel_built_and_the_three_free_fields_are_pinned() {
        for answer in [BRACKET, NOTCHED, QUARTERS, &washer()] {
            let m = model(answer);
            let built = mesh(&m).unwrap();
            let decoded = walk(&artifact(answer), m.frac_bits);
            assert_eq!(decoded, built, "the artifact does not decode back to the mesh");
        }
        let header = stl_header();
        assert_eq!(&header[..STL_HEADER_TEXT.len()], STL_HEADER_TEXT.as_bytes());
        assert!(header[STL_HEADER_TEXT.len()..].iter().all(|b| *b == 0), "the 80 free bytes are not zero-padded");
        assert!(!STL_HEADER_TEXT.contains("solid"));
    }

    #[test]
    fn the_triangle_order_is_a_function_of_the_set_and_not_of_the_build() {
        let built = triangles(BRACKET);
        // the same triangles, each rotated (which keeps the winding) and the list reversed
        let mut shuffled: Vec<Tri> = built.iter().map(|t| [t[1], t[2], t[0]]).collect();
        shuffled.reverse();
        assert_ne!(shuffled, built);
        assert_eq!(canonical_mesh(shuffled).unwrap(), built);
        // a canonical mesh is already canonical
        assert_eq!(canonical_mesh(built.clone()).unwrap(), built);
        // and the canonical rotation is a rotation, never a reflection
        for t in &built {
            let r = canonical_rotation(*t);
            assert!(r == *t || r == [t[1], t[2], t[0]] || r == [t[2], t[0], t[1]]);
            assert_eq!(triangle_cross(&r), triangle_cross(t));
        }
    }

    #[test]
    fn the_mesh_checks_refuse_what_is_not_a_solid() {
        assert!(matches!(canonical_mesh(vec![]), Err(DeriveError::Transformer(_))));
        let flat: Tri = [[0, 0, 0], [1, 0, 0], [2, 0, 0]];
        assert!(matches!(canonical_mesh(vec![flat]), Err(DeriveError::Transformer(_))));
        let t: Tri = [[0, 0, 0], [1, 0, 0], [0, 1, 0]];
        match canonical_mesh(vec![t, t]) {
            Err(DeriveError::Transformer(msg)) => assert!(msg.contains("appears twice"), "{msg}"),
            other => panic!("a duplicated facet must be refused, got {other:?}"),
        }
        // an inside-out solid: every triangle of a good mesh, reversed
        let inside_out: Vec<Tri> = triangles(BRACKET).into_iter().map(|t| [t[0], t[2], t[1]]).collect();
        let inside_out = canonical_mesh(inside_out).unwrap();
        check_closed_oriented(&inside_out).unwrap();
        assert_eq!(six_times_volume(&inside_out), -six_times_volume(&triangles(BRACKET)));
        // an open surface: one facet short
        let mut open = triangles(BRACKET);
        open.pop();
        assert!(check_closed_oriented(&open).is_err());
    }

    // ---- (6) determinism, and the transformer's canonical-input rule ------------------------

    #[test]
    fn the_same_dsl_twice_is_the_same_bytes() {
        for answer in [BRACKET, NOTCHED, QUARTERS, &washer()] {
            assert_eq!(artifact(answer), artifact(answer));
        }
    }

    #[test]
    fn key_order_and_whitespace_change_nothing() {
        let reordered = r#"{"solid":{"z1":3,"z0":0,"sketch":"bracket","op":"extrude"},
                            "sketches":{"bracket":[[0,0],[4,0],[4,2],[2,2],[2,4],[0,4]]},
                            "frac_bits":0,"v":1}"#;
        assert_eq!(canonical(reordered), canonical(BRACKET));
        assert_eq!(artifact(reordered), artifact(BRACKET));
    }

    #[test]
    fn the_transformer_refuses_input_that_is_not_canonical() {
        for bad in [BRACKET.as_bytes(), b"{", br#"{"v":2}"#, b""] {
            match CadStlTransformer.run(bad) {
                Err(DeriveError::Transformer(msg)) => assert!(msg.contains("not canonical cad/v1"), "{msg}"),
                other => panic!("expected a transformer refusal, got {other:?}"),
            }
        }
        let mut padded = canonical(BRACKET);
        padded.push(b'\n');
        assert!(matches!(CadStlTransformer.run(&padded), Err(DeriveError::Transformer(_))));
    }

    // ---- (7) the fixture corpus, and X3's two-architecture instrument ------------------------

    fn corpus_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus").join("cad")
    }

    fn corpus() -> BTreeMap<String, Vec<u8>> {
        let mut files = BTreeMap::new();
        for entry in std::fs::read_dir(corpus_dir()).expect("corpus/cad exists") {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_str().unwrap().to_string();
            if name.ends_with(".json") && name != "golden.json" {
                files.insert(name, std::fs::read(&path).unwrap());
            }
        }
        assert!(files.len() >= 8, "the corpus holds {} samples; at least eight are expected", files.len());
        files
    }

    #[test]
    fn corpus_derives_to_the_golden_values() {
        let golden: serde_json::Value = serde_json::from_slice(&std::fs::read(corpus_dir().join("golden.json")).unwrap()).unwrap();
        let golden = golden.as_object().unwrap();
        let files = corpus();
        let grammar_id = grammar_id_v1(GRAMMAR_NAME);
        let mut refusals = 0;
        for (name, answer) in &files {
            let g = golden.get(name).unwrap_or_else(|| panic!("{name} has no entry in golden.json; pin it"));
            // SA-2's bound-exhausting half of the corpus: the entry pins the refusal, because a
            // refusal that differed between two hosts would be as bad as a differing artifact
            if let Some(expected) = g.get("refused").and_then(|r| r.as_str()) {
                refusals += 1;
                match derive_with(&CadGrammar, &CadStlTransformer, &binding(), answer) {
                    Err(e) => assert_eq!(e.to_string(), expected, "{name}"),
                    Ok(_) => panic!("{name} is meant to exceed a declared bound and it derived"),
                }
                continue;
            }
            let d = derive_with(&CadGrammar, &CadStlTransformer, &binding(), answer).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(g["dsl_hash"].as_str().unwrap(), d.dsl_hash.to_string(), "{name} dsl_hash");
            assert_eq!(g["artifact_hash"].as_str().unwrap(), d.artifact_hash.to_string(), "{name} artifact_hash");
            assert_eq!(g["artifact_bytes"].as_u64().unwrap(), d.object.artifact_bytes, "{name} artifact_bytes");
            assert_eq!(d.object.artifact_bytes as usize, d.artifact.bytes.len());
            // the ids recomputed the way a consumer does (Decision 5, X6)
            assert_eq!(dsl_hash_v1(&grammar_id, &d.canonical_dsl), d.dsl_hash);
            assert_eq!(artifact_hash_v1(&d.artifact.bytes), d.artifact_hash);
            assert_eq!(d.grammar_id, grammar_id);
            assert_eq!(d.kind, kind::CAD);
            let named = derive_named(TRANSFORMER_NAME, &binding(), answer).unwrap();
            assert_eq!(named.object, d.object);
            assert!(crate::verify(&d.object, answer).unwrap().all_match(), "{name}");
            assert!(crate::verify_artifact_bytes(&d.object, &d.artifact.bytes));
            assert_eq!(CadStlTransformer.run(&d.canonical_dsl).unwrap().bytes, d.artifact.bytes);
            // the bytes walk back to a closed, outward-oriented solid
            let m = canonical_model(&d.canonical_dsl).unwrap();
            let decoded = walk(&d.artifact.bytes, m.frac_bits);
            check_closed_oriented(&decoded).unwrap();
            assert!(six_times_volume(&decoded) > 0);
        }
        assert_eq!(refusals, 3, "SA-2 asks the drill's corpus to exhaust every declared bound; there are three");
        for name in golden.keys() {
            assert!(files.contains_key(name), "golden.json names {name}, which is not in the corpus");
        }
    }

    /// Re-pin: `cargo test -p misaka-palw-derive cad::tests::print_golden -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn print_golden() {
        let mut out = serde_json::Map::new();
        for (name, answer) in &corpus() {
            let mut entry = serde_json::Map::new();
            match derive_with(&CadGrammar, &CadStlTransformer, &binding(), answer) {
                Ok(d) => {
                    entry.insert("dsl_hash".into(), d.dsl_hash.to_string().into());
                    entry.insert("artifact_hash".into(), d.artifact_hash.to_string().into());
                    entry.insert("artifact_bytes".into(), d.object.artifact_bytes.into());
                }
                Err(e) => {
                    entry.insert("refused".into(), e.to_string().into());
                }
            }
            out.insert(name.clone(), entry.into());
        }
        println!("{}", serde_json::to_string_pretty(&serde_json::Value::Object(out)).unwrap());
    }

    // ---- (8) the discipline, scanned ---------------------------------------------------------

    #[test]
    fn the_only_float_this_file_names_is_the_exact_writer_it_calls() {
        let src = include_str!("cad.rs");
        assert!(!src.contains(concat!("f", "64")), "cad.rs names the wider float type");
        assert!(!src.contains(concat!("Hash", "Map")), "cad.rs uses an unordered map");
        // Every mention of the narrow one is the name of `crate::fixed`'s exact bit-pattern
        // builder, which computes in integers and REFUSES what binary32 cannot hold. There is
        // no float type, no float literal and no float operation on the path to the output.
        let exact = concat!("f", "32_le_exact");
        for (at, _) in src.match_indices(concat!("f", "32")) {
            assert!(src[at..].starts_with(exact), "cad.rs names the narrow float type at {at}: {:?}", &src[at..at + 20]);
        }
        assert!(src.contains(exact), "the scan is a statement about a real call, not an absence");
    }
}
