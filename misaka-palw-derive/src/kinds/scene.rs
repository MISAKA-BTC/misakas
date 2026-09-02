//! Kind `scene` (ADR-0078 Decision 8, row `scene`): the Canonical Scene DSL — objects,
//! fixed-point transforms, materials by name, hierarchy — a fixed-point mesh builder, and a
//! canonical glTF 2.0 binary writer that makes a `.glb` of it. Grammar `scene/v1`, transformer
//! `scene/glb/v1`, writer `gltf-binary/2.0/canonical-v1`, artifact `model/gltf-binary`.
//!
//! The row's determinism basis is "integer geometry; no float in the path", and this module is
//! that sentence taken literally: no `f32` or `f64` value is ever computed here. glTF *stores*
//! IEEE-754 binary32, so the output path needs binary32 bit patterns — [`crate::fixed`] builds
//! them from an integer and a binary scale with integer arithmetic and REFUSES any value
//! binary32 cannot hold exactly ([`DeriveError::Inexact`]). A refusal, never a rounding: a
//! rounded coordinate is a coordinate two honest hosts could round apart, and X3 asks for
//! byte-identical artifacts on two architectures, not for artifacts that are close.
//!
//! The row's "not covered" column reads "texture synthesis; physics", and this module adds two
//! refusals of its own, by name, in the same spirit — see "What is refused by name" below.
//!
//! # The DSL — grammar `scene/v1`
//!
//! JSON. Every object below carries EXACTLY the keys listed: an unknown key and a missing key
//! are each a refusal that names the key, because "the field was absent so I assumed a default"
//! is how two implementations of one grammar drift apart.
//!
//! ```text
//! { "v": 1,
//!   "frac_bits": 0 | 4 | 8 | 12 | 16,     the binary scale of EVERY length in this document
//!   "materials": [ material, ... ],       1..=256, names unique
//!   "nodes":     [ node, ... ] }          a forest; 1..=1024 nodes in all, depth at most 8
//!
//! material = { "name": string (1..=64 bytes, unique),
//!              "base_color": [r, g, b, a],   each 0..=256, over a denominator of 256
//!              "metallic": 0..=256, "roughness": 0..=256,
//!              "double_sided": true | false }
//!
//! node = { "name": string (0..=64 bytes),
//!          "translation": [x, y, z],      fixed point: the value is x / 2^frac_bits
//!          "rotation": [x, y, z, w],      integers with x²+y²+z²+w² == 4 — see "Rotation"
//!          "scale": [sx, sy, sz],         fixed point, each 1..=2^30 (strictly positive)
//!          "shape": null | shape,         null is a pure transform node
//!          "material": null | string,     the name of a material; null exactly when shape is null
//!          "children": [ node, ... ] }    nested, so the hierarchy is a tree by construction
//!
//! shape = { "shape": "box",      "min": [x, y, z], "max": [x, y, z] }      min < max componentwise
//!       | { "shape": "plane",    "min": [x, z],    "max": [x, z] }         at y = 0, facing +Y
//!       | { "shape": "triangle", "a": [x, y, z], "b": [...], "c": [...] }  one face, as wound
//!       | { "shape": "prism",    "base": [[x, z], ...], "y_min": y, "y_max": y }
//! ```
//!
//! Every coordinate lies in `-2^30..=2^30` fixed-point units. A scene declares at least one
//! shape: a glTF with no mesh would need an empty buffer and an absent `meshes` array, which is
//! a second shape of document, and one shape of document is fewer degrees of freedom.
//!
//! Children are NESTED rather than referenced by index. A tree written as an index graph needs
//! a cycle check and a reachability check, and a grammar that can express a cycle is a grammar
//! whose canonical form has to decide what a cycle means. Nesting cannot express one.
//!
//! # Fixed point, and where the discipline bites
//!
//! A length `n` in the DSL means `n / 2^frac_bits`. Vertex positions reach the artifact as
//! binary32, so each one goes through [`crate::fixed::f32_le_exact`], which refuses a value
//! needing more than 24 significant bits or an out-of-range exponent. `frac_bits: 8` and a
//! coordinate of `16_777_217` is a legal DSL and an `Inexact` refusal: 25 significant bits is
//! one more than binary32 holds, so the transformer produces nothing rather than a mesh whose
//! last bit is the writer's opinion.
//!
//! # Materials: the denominator is 256, not 255
//!
//! glTF's `baseColorFactor`, `metallicFactor` and `roughnessFactor` are binary32 in `[0, 1]`.
//! A channel written as `n/255` is NOT a binary32 value for any `n` but 0 and 255 (255 = 3·5·17,
//! so 1/255 is not dyadic), and this kind refuses to round. `n/256` with `n` in `0..=256` is
//! exact for every `n` — at most nine significant bits — so the DSL's channels run `0..=256`
//! and 256 is full scale. The extra value is not an off-by-one; it is the price of an exact
//! denominator.
//!
//! # Rotation: the twelve exact rotations, and why there are no others
//!
//! A node's rotation is a quaternion, and glTF stores its four components as binary32. A
//! quaternion is admissible here only if it is a UNIT quaternion whose components are exactly
//! representable — that is, dyadic: `(x, y, z, w) / 2^k` with `x² + y² + z² + w² = 4^k`.
//!
//! For `k ≥ 2` every solution is even: four odd squares sum to 4 mod 8, and `4^k ≡ 0 mod 8`
//! once `k ≥ 2`, so an all-odd solution is impossible and a mixed one cannot be `≡ 0 mod 4`;
//! dividing by two descends to `4^(k-1)`. The descent bottoms out at `x² + y² + z² + w² = 4`,
//! whose solutions are the permutations of `(±2, 0, 0, 0)` and the sixteen `(±1, ±1, ±1, ±1)`.
//! So **`k = 1` is not a restriction, it is the normal form**: any dyadic unit quaternion is
//! `2^(k-1)` times one of those, and names the same rotation. The DSL therefore spells the
//! quaternion with an implied denominator of 2 and requires `x² + y² + z² + w² == 4` exactly.
//!
//! Those twenty-four quaternions are twelve rotations (`q` and `-q` are one rotation): the
//! identity, the three half-turns about the axes, and the eight third-turns about the body
//! diagonals — the rotation group of the tetrahedron. Their matrices have entries in
//! `{0, ±1}` only, which is why composing a hierarchy of them stays exact forever.
//!
//! A person who wants a 45° turn is asking for `√2/2`, which is not a binary32 value and not a
//! dyadic rational at all. The grammar refuses it by name rather than accepting a rounded
//! quaternion, exactly as it refuses an inexact coordinate.
//!
//! # Normals: the six axes, and why there are no others
//!
//! A glTF `NORMAL` must be unit length, and its components are binary32. By the same descent —
//! `a² + b² + c² = 4^k` forces `a, b, c` all even for every `k ≥ 1`, because a sum of three odd
//! squares is `3 mod 8` and a mixed sum is not `0 mod 4` — the only exact dyadic unit normals
//! are the six axis directions `(±1, 0, 0)`, `(0, ±1, 0)`, `(0, 0, ±1)`.
//!
//! So a mesh emits `NORMAL` **iff every one of its faces is axis-aligned**, decided exactly:
//! the face's integer cross product has exactly one non-zero component. Otherwise the primitive
//! omits `NORMAL` entirely and the glTF specification's own rule applies — "when normals are
//! not specified, client implementations MUST calculate flat normals" — which for a flat face
//! is the same direction this module would have written, computed by the consumer in its own
//! arithmetic instead of being frozen into the artifact by ours. A degenerate face (a zero
//! cross product) is refused: it has no normal in any arithmetic.
//!
//! # The primitives, and what is refused by name
//!
//! `box`, `plane`, `triangle` and `prism` (an integer convex polygon extruded along Y) are
//! every mesh this build makes. Two shapes a person would ask for are refused, and the refusal
//! names the reason, following the ADR's own "not covered" column:
//!
//! * **`sphere`** — a subdivided icosahedron has vertices containing `φ = (1+√5)/2`, and any
//!   subdivision that is then normalized divides by a square root. There is no exact integer
//!   sphere, so this build has none; a person wanting one writes it as a `prism` or as
//!   `triangle`s and owns the approximation in the DSL, where it is visible and hashed.
//! * **`cylinder`** — a regular n-gon needs `cos(2π/n)`, irrational for every `n` but 4. The
//!   exact replacement is `prism` over a polygon the answer lists explicitly: a sixteen-sided
//!   "cylinder" is sixteen integer points, and the rounding is the model's, in the DSL, rather
//!   than the transformer's, hidden in the bytes.
//!
//! The distinction is the whole point of ADR-0078 Decision 3: what the transformer does must be
//! a pure integer function, so every approximation must happen upstream of `dsl_hash`.
//!
//! # Nothing outside the document (X9)
//!
//! X9 asks that extra inputs of a transformation be named by hash INSIDE the DSL, so that
//! `dsl_hash` still fixes the whole derivation. `scene/v1` meets it by having none: there is no
//! texture, no mesh import, no URI, no reference of any kind to bytes this document does not
//! contain, and a material is named by a string that resolves inside the same document. Every
//! object carries exactly the keys listed above and an unknown key is refused, so the admitted
//! key set IS the answer to "what else could this derivation depend on" — and the test
//! `the_grammar_admits_no_reference_to_bytes_outside_the_document` asserts that set rather than
//! describing it, so a later key that does name outside bytes cannot arrive without a hash and
//! a sentence here.
//!
//! # The hierarchy, composed in fixed point
//!
//! The glTF node tree carries each node's local T/R/S, which is how the format represents a
//! hierarchy and what a viewer expects. In addition the transformer COMPOSES the hierarchy
//! itself, exactly, in [`Dyadic`] arithmetic (an integer mantissa over a power of two, with the
//! common factors of two removed after every operation), and requires every mesh's world-space
//! bounding box to stay inside `±2^30` units. The composition does not reach the bytes — the
//! node tree does — but it reaches the verdict: a scene whose composed extent leaves the bound,
//! or whose composed transform needs more precision than exact integer arithmetic can carry at
//! this depth, is refused rather than written. Rounding it would be the float behaviour this
//! ADR exists to refuse.
//!
//! # The artifact — writer `gltf-binary/2.0/canonical-v1`
//!
//! glTF 2.0 admits many byte encodings of one scene, so every degree of freedom is pinned here
//! and nowhere else:
//!
//! * **Node order** — depth-first, pre-order, siblings in the DSL's order. A node's index is
//!   assigned when it is entered, so a subtree occupies a contiguous range.
//! * **Mesh order** — one glTF mesh per shape-carrying node, numbered in that same walk. No
//!   deduplication of identical geometry: dedup is a second rule, and the DSL's repetition is
//!   the author's to make.
//! * **Accessor and bufferView order** — for mesh `i` in mesh order: `POSITION`, then `NORMAL`
//!   if the mesh has one, then the indices. Accessor `k` uses bufferView `k`; the two arrays
//!   are the same length and the same order, which a test checks.
//! * **Material order** — the DSL's order, all of them, used or not.
//! * **Buffer layout** — the bufferViews in accessor order, each starting at the next multiple
//!   of 4, the gap filled with `0x00`. Positions and normals are `VEC3` binary32 (12 bytes,
//!   already a multiple of 4); indices are `UNSIGNED_SHORT`, so an odd triangle count leaves a
//!   two-byte gap, which is the padding rule's only exercise and one the corpus takes.
//! * **Index type** — always `UNSIGNED_SHORT` (5123). A mesh above 65 536 vertices is refused
//!   by name rather than promoted to `UNSIGNED_INT`: one index rule, no second branch, and the
//!   refusal is here so that a future primitive cannot overflow one quietly. The schema's own
//!   bounds keep every mesh this build makes far below it.
//! * **Accessor `min`/`max`** — emitted on EVERY accessor (glTF requires them only on
//!   `POSITION`) and always the exact componentwise extremes of the accessor's own data.
//! * **The JSON** — `canon_json`'s rules: keys sorted by their UTF-8 bytes, no whitespace,
//!   RFC 8785 strings ([`crate::canon_json::write_string`] is the same function the DSL's
//!   canonicalizer uses). `canon_json` REFUSES non-integer numbers, because no DSL in this
//!   crate has one; a glTF document does, so this module adds the one number form it needs and
//!   nothing else: a dyadic rational `m / 2^f` spelled as its exact finite decimal, computed as
//!   `m·5^f / 10^f` in integer arithmetic, with `m` reduced to odd first so the spelling is the
//!   shortest exact one. No exponent form, no trailing zero, one spelling per value.
//! * **`asset.generator`** — the writer's name, a constant. Not a version string, not a date:
//!   a clock in the artifact is a clock in the hash.
//! * **The GLB container** — header `glTF` / version 2 / total length, then the JSON chunk and
//!   then the BIN chunk, in that order, each `chunkLength` including its own padding: the JSON
//!   chunk padded with `0x20` (space) and the BIN chunk with `0x00`, as the specification's
//!   "Structured JSON Content" and "Binary Buffer" sections require.
//!
//! # The declared bounds, and why they are checked before anything is built (SA-2)
//!
//! A DSL is attacker-shaped: it is a model's answer to a stranger's prompt, and a scene DSL is
//! the shape that can encode a mesh that exhausts memory at build time — on the executor AND on
//! every consumer who verifies. ADR-0078's security amendment SA-2 therefore has this kind
//! declare three numbers and enforce them BEFORE the build:
//!
//! | bound | value | unit |
//! |---|---|---|
//! | `max_dsl_bytes` | 256 KiB | bytes of the answer, and of the canonical DSL |
//! | `max_steps` | 65 536 | this kind's unit: **mesh vertices emitted** |
//! | `max_artifact_bytes` | 2 MiB | bytes of the `.glb` |
//!
//! [`BOUNDS`] is the declaration; [`check_bounds`] is the enforcement, and it is the only place
//! the three are read. Exceeding any of them is "no object" — Decision 2's parse-failure arm —
//! so the refusal is a `DeriveError::Grammar` from the grammar and a `DeriveError::Transformer`
//! from the transformer, and in neither case does a derivation exist (X4).
//!
//! *Before* is the whole point, and it is meant literally at each step:
//!
//! * `max_dsl_bytes` is checked on the raw answer bytes **before `parse_canonical`**, so a
//!   hundred-megabyte answer never becomes a parse tree.
//! * `max_steps` and `max_artifact_bytes` are checked on a [`ScenePlan`] — the vertex, index and
//!   mesh counts **counted from the schema alone** by [`shape_cost`], allocating no geometry —
//!   so a DSL naming ten million vertices is refused before the first `Vec` grows.
//! * The artifact bound is checked against [`predicted_artifact_bytes`], a stated UPPER bound on
//!   what the writer would emit. Predicting high is the safe direction: the gate then refuses a
//!   little more than it must, and a test holds it to `prediction ≥ actual` over the corpus and
//!   over every primitive. The exact size is still checked after the write, where it can only
//!   confirm what the plan already knew.
//!
//! The bounds COMPOSE, and the first one to bite wins. That is not a defect but it IS a trap:
//! three bounds are easy to choose so that one of them can never be reached, and a bound no
//! input can reach is a comment. The values above are chosen so that each is reachable, which
//! takes an argument, because bytes and vertices are nearly the same quantity:
//!
//! * A big prism is the cheapest geometry per byte of DSL and per predicted byte — about 29
//!   predicted artifact bytes per vertex — so a prism scene runs out of VERTICES first.
//! * A box is the most expensive: one node, one mesh, three accessors and three bufferViews for
//!   twenty-four vertices, about 121 predicted bytes each — so a scene of many small boxes runs
//!   out of ARTIFACT BYTES first, at roughly a fifth of the vertex budget.
//! * Anything larger than the answer a class can emit runs out of DSL BYTES first, before it is
//!   parsed at all.
//!
//! `max_artifact_bytes` therefore sits between 29 × `max_steps` and what 1024 boxes predict, and
//! the corpus proves the reachability rather than asserting it: `07-`, `08-` and `09-` exhaust
//! one bound each, which is SA-2's "bound-exhausting corpus". The bounds are `const` in this
//! file, and this file is inside the build's source-tree hash (`build.rs`), so moving one moves
//! `transformer_id`: a build with different bounds is a different transformer, which is
//! Decision 3's rule applied to SA-2.
//!
//! # Cost
//!
//! Mesh building is `O(vertices)`, capped by `max_steps` in a scene and by 65 536 in one mesh
//! (the index type); the composition is `O(nodes · depth)` with a fixed 3×3 matrix per node.
//! Every one of those is decided on the plan before any of it runs.

use crate::bytes::{pad_to, put_u32_le};
use crate::canon_json::{CanonValue, parse_canonical, write_canonical, write_string};
use crate::fixed::f32_le_exact;
use crate::{Artifact, DeriveError, Discipline, Grammar, Transformer, TransformerManifest};
use kaspa_consensus_core::palw_derived_v1::kind;
use std::collections::{BTreeMap, BTreeSet};

/// The grammar's name (Decision 2): `grammar_id = H(domain ‖ name)`.
pub const GRAMMAR_NAME: &str = "scene/v1";
/// The transformer's name, the first field of its manifest (Decision 3).
pub const TRANSFORMER_NAME: &str = "scene/glb/v1";
/// The canonical writer the manifest names.
pub const WRITER_NAME: &str = "gltf-binary/2.0/canonical-v1";
/// The artifact's media type and file extension.
pub const MEDIA_TYPE: &str = "model/gltf-binary";
pub const EXTENSION: &str = "glb";

/// The input and output bounds a transformer declares under ADR-0078 SA-2. Held here rather
/// than in [`TransformerManifest`] because the manifest type belongs to the crate root; see
/// this module's "declared bounds" section, and the patch note that moves them into the
/// manifest so that `transformer_id` names them by field instead of by source-tree hash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransformerBounds {
    /// The largest answer, and the largest canonical DSL, this transformer will look at.
    pub max_dsl_bytes: u64,
    /// The largest artifact it will emit.
    pub max_artifact_bytes: u64,
    /// SA-2's `max_steps`, in this kind's own unit ([`STEPS_UNIT`]).
    pub max_steps: u64,
}

/// SA-2's unit for this kind: one step is one mesh vertex emitted. It is the quantity the
/// build's memory is linear in (12 bytes of position, 12 of normal, and the indices that
/// address it), so bounding it bounds the build.
pub const STEPS_UNIT: &str = "mesh vertices";

/// The declared bounds of `scene/glb/v1` (SA-2). Every enforcement reads them from here.
pub const BOUNDS: TransformerBounds = TransformerBounds { max_dsl_bytes: 256 << 10, max_artifact_bytes: 2 << 20, max_steps: 1 << 16 };

/// The declared bounds, for a caller that has the transformer and not this module.
pub fn declared_bounds() -> TransformerBounds {
    BOUNDS
}

/// The binary scales a document may declare. Bounded so that the decimal spelling of a value
/// stays inside `i128` and so that the accumulated scale of a composed hierarchy stays bounded.
pub const FRAC_BITS_ALLOWED: [i128; 5] = [0, 4, 8, 12, 16];
/// Every coordinate and every translation lies in `-COORD_LIMIT..=COORD_LIMIT` fixed-point
/// units. Deliberately larger than `2^24`, so that a legal DSL can still name a coordinate
/// binary32 cannot hold: the `Inexact` refusal is reachable, and the corpus reaches it.
pub const COORD_LIMIT: i64 = 1 << 30;
/// A scale component lies in `1..=SCALE_MAX` fixed-point units — strictly positive. A negative
/// scale inverts the determinant, and glTF derives the front face from the winding after the
/// node transform, so a negative scale silently turns every mesh inside out. Refused instead.
pub const SCALE_MAX: i64 = 1 << 30;
/// Most nodes in a document, over the whole forest.
pub const MAX_NODES: usize = 1024;
/// Deepest nesting of `children`. Bounds the composition's accumulated precision.
pub const MAX_DEPTH: usize = 8;
/// Most materials in a document.
pub const MAX_MATERIALS: usize = 256;
/// Longest node or material name, in bytes.
pub const MAX_NAME_BYTES: usize = 64;
/// A prism's base polygon holds `PRISM_MIN_POINTS..=PRISM_MAX_POINTS` points.
pub const PRISM_MIN_POINTS: usize = 3;
pub const PRISM_MAX_POINTS: usize = 256;
/// Most vertices in one mesh: the index type is `UNSIGNED_SHORT` and this is what it addresses.
pub const MAX_MESH_VERTICES: usize = 65_536;
/// SA-2's `max_dsl_bytes`, spelled once in [`BOUNDS`].
pub const MAX_DSL_BYTES: usize = BOUNDS.max_dsl_bytes as usize;
/// Most vertices in a whole scene — SA-2's `max_steps` in this kind's unit, spelled once in
/// [`BOUNDS`]. The name is kept for the builder's last-line check, which the plan makes
/// unreachable and which stays because an unreachable refusal is cheaper than a wrong mesh.
pub const MAX_TOTAL_VERTICES: usize = BOUNDS.max_steps as usize;
/// SA-2's `max_artifact_bytes`, spelled once in [`BOUNDS`].
pub const ARTIFACT_MAX_BYTES: usize = BOUNDS.max_artifact_bytes as usize;
/// A material channel is `n / CHANNEL_DENOMINATOR` with `n` in `0..=CHANNEL_DENOMINATOR`.
pub const CHANNEL_DENOMINATOR: i64 = 256;

/// `glTF` little-endian — the GLB magic.
pub const GLB_MAGIC: u32 = 0x4654_6C67;
/// The container version this writer emits.
pub const GLB_VERSION: u32 = 2;
/// `JSON` little-endian — the structured-content chunk type.
pub const GLB_CHUNK_JSON: u32 = 0x4E4F_534A;
/// `BIN\0` little-endian — the binary-buffer chunk type.
pub const GLB_CHUNK_BIN: u32 = 0x004E_4942;
/// The JSON chunk is padded with spaces, the BIN chunk with zeroes — the specification's own
/// fill bytes, so that a padded chunk is still valid JSON and still zeroed data.
pub const GLB_JSON_PAD: u8 = 0x20;
pub const GLB_BIN_PAD: u8 = 0x00;

/// glTF component types and the two buffer targets this writer uses.
pub const COMPONENT_FLOAT: u64 = 5126;
pub const COMPONENT_UNSIGNED_SHORT: u64 = 5123;
pub const TARGET_ARRAY_BUFFER: u64 = 34962;
pub const TARGET_ELEMENT_ARRAY_BUFFER: u64 = 34963;
/// `TRIANGLES`. It is glTF's default and this writer states it anyway: a default omitted is a
/// second spelling of the same document.
pub const MODE_TRIANGLES: u64 = 4;

/// The exact norm a `scene/v1` quaternion must have: the components are over a denominator of
/// two, so a unit quaternion is `x² + y² + z² + w² == 4` (see the module doc's descent).
pub const ROTATION_NORM: i64 = 4;

/// The most fractional bits an exactly-composed transform may carry, and the largest mantissa.
/// Both are refusals, not truncations: past them the composition would have to round.
pub const DYADIC_MAX_FRAC_BITS: u32 = 96;
pub const DYADIC_MAX_MAGNITUDE: i128 = 1 << 110;

/// The grammar `scene/v1`.
pub struct SceneGrammar;
/// The transformer `scene/glb/v1`: canonical `scene/v1` bytes to a binary glTF.
pub struct SceneGlbTransformer;

/// This kind's grammar and transformer, as the registry sees them.
pub fn register() -> (Vec<Box<dyn Grammar>>, Vec<Box<dyn Transformer>>) {
    (vec![Box::new(SceneGrammar)], vec![Box::new(SceneGlbTransformer)])
}

impl Grammar for SceneGrammar {
    fn name(&self) -> &'static str {
        GRAMMAR_NAME
    }

    /// Bound, parse, hold to the schema, bound again, re-emit. Any refusal is
    /// `DeriveError::Grammar`, which is X4: no object exists and the claim is untouched.
    ///
    /// The byte bound comes FIRST, on the raw answer, because `parse_canonical` allocates a
    /// tree the size of its input and SA-2's whole point is that the input is a stranger's
    /// (see this module's "declared bounds"). The plan's bounds come before the caller can
    /// hand the canonical bytes to a transformer, so no builder ever sees an unbounded scene.
    fn canonicalize(&self, answer: &[u8]) -> Result<Vec<u8>, DeriveError> {
        check_dsl_bytes(answer.len(), "the answer", grammar_err)?;
        let tree = parse_canonical(answer)?;
        let scene = SceneDsl::from_tree(&tree)?;
        check_bounds(&scene, grammar_err)?;
        let canonical = write_canonical(&tree);
        check_dsl_bytes(canonical.len(), "the canonical DSL", grammar_err)?;
        Ok(canonical)
    }
}

impl Transformer for SceneGlbTransformer {
    fn manifest(&self) -> TransformerManifest {
        TransformerManifest {
            name: TRANSFORMER_NAME,
            kind: kind::SCENE,
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

    /// `run` re-enforces every bound rather than trusting that the grammar did (SA-2: enforced
    /// before it runs, and this is where it runs). The trait's contract is only that the input
    /// is canonical, and `run` is public: a caller who reaches it directly gets the same gate,
    /// as a `DeriveError::Transformer` because it is the transformer refusing.
    fn run(&self, dsl: &[u8]) -> Result<Artifact, DeriveError> {
        check_dsl_bytes(dsl.len(), "the canonical DSL", transformer_err)?;
        let scene = canonical_scene(dsl)?;
        check_bounds(&scene, transformer_err)?;
        let bytes = write_glb(&scene)?;
        Ok(Artifact { bytes, media_type: MEDIA_TYPE, extension: EXTENSION })
    }
}

// ---------------------------------------------------------------------------------------------
// SA-2 — the declared bounds, enforced before the build
// ---------------------------------------------------------------------------------------------

/// What a scene would cost to build, counted from the schema alone: no mesh is assembled, no
/// transform is composed, nothing the size of the output is allocated. SA-2's gate runs on this.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScenePlan {
    /// Nodes in the forest, geometry-bearing or not.
    pub nodes: usize,
    /// Shape-carrying nodes: one glTF mesh each, in the same walk order.
    pub meshes: usize,
    /// SA-2's steps, in this kind's unit: vertices the builder would emit.
    pub vertices: usize,
    /// Indices the builder would emit — three per triangle.
    pub indices: usize,
}

/// The vertices and indices one shape emits, counted without building it.
///
/// This is the builder's own arithmetic, stated once: every face contributes one vertex per
/// corner (faces do not share vertices, so a normal is per-face and flat) and `corners - 2`
/// triangles from its fan. A box is six four-corner faces; a plane is one; a triangle is one of
/// three corners; a prism over `n` points is two `n`-corner caps and `n` four-corner sides.
/// [`the_plan_is_the_builders_own_arithmetic`] holds it to the builder over every primitive.
pub fn shape_cost(shape: &Shape) -> (usize, usize) {
    let fan = |corners: usize| 3 * (corners - 2);
    match shape {
        Shape::Box { .. } => (6 * 4, 6 * fan(4)),
        Shape::Plane { .. } => (4, fan(4)),
        Shape::Triangle { .. } => (3, fan(3)),
        Shape::Prism { base, .. } => {
            let n = base.len();
            (2 * n + 4 * n, 2 * fan(n) + n * fan(4))
        }
    }
}

/// Count a whole document's build, walking the forest the way [`build_scene`] will.
pub fn plan_scene(dsl: &SceneDsl) -> ScenePlan {
    fn walk(nodes: &[Node], plan: &mut ScenePlan) {
        for node in nodes {
            plan.nodes += 1;
            if let Some(shape) = &node.shape {
                let (vertices, indices) = shape_cost(shape);
                plan.meshes += 1;
                // Saturating, because the fields of `SceneDsl` are public and a hand-built one
                // need not have passed the schema's node bound. A saturated count is above the
                // true one, so the gate below still refuses — the safe direction.
                plan.vertices = plan.vertices.saturating_add(vertices);
                plan.indices = plan.indices.saturating_add(indices);
            }
            walk(&node.children, plan);
        }
    }
    let mut plan = ScenePlan::default();
    walk(&dsl.nodes, &mut plan);
    plan
}

/// A per-item byte budget for the glTF JSON, each one an upper bound on what [`gltf_json`]
/// writes for that item. They are generous on purpose — the number they feed is a REFUSAL
/// threshold, and a threshold that predicts high refuses a little more than it must, while one
/// that predicts low lets a scene through to exhaust the memory the bound exists to protect.
/// `prediction_is_an_upper_bound_on_what_the_writer_emits` holds each of them to the writer.
mod json_budget {
    /// `{"asset":{...},"buffers":[{...}],"scene":0,"scenes":[{"nodes":[]}]}` and the array
    /// brackets and commas of the six top-level arrays.
    pub const FIXED: usize = 512;
    /// One accessor: eleven keys of fixed text, plus six numbers of at most 29 bytes each (a
    /// sign, ten integer digits, a point and sixteen fraction digits — `frac_bits` is at most
    /// 16 and a coordinate at most 2^30), plus three indices of at most eight digits.
    pub const ACCESSOR: usize = 128 + 6 * 29 + 3 * 8;
    /// One bufferView: four keys and three numbers of at most ten digits.
    pub const BUFFER_VIEW: usize = 96 + 3 * 10;
    /// One mesh: a single primitive with at most five numeric fields of eight digits.
    pub const MESH: usize = 128 + 5 * 8;
    /// One node, without its children: a name of at most 64 bytes (each of which a control
    /// character would spell as the six bytes of `\u00XX`), four rotation components of at most
    /// four bytes, and six T/S components of at most 29.
    pub const NODE: usize = 128 + 64 * 6 + 4 * 4 + 6 * 29 + 8;
    /// One entry of a node's `children` array. Every node is some node's child at most once, so
    /// the whole document's child arrays cost at most `nodes` of these.
    pub const CHILD_REF: usize = 9;
    /// One material: a name of at most 64 escaped bytes and six channels of at most 12.
    pub const MATERIAL: usize = 160 + 64 * 6 + 6 * 12;
}

/// An upper bound on the artifact a plan would produce, computed before it is produced.
///
/// The binary buffer is exact arithmetic on the plan (12 bytes per position, 12 per normal — a
/// mesh may omit `NORMAL`, and assuming it does not is the safe direction — 2 per index, and at
/// most three padding bytes before each of the three views). The JSON is the per-item budget
/// above. The GLB frame is its 12-byte header and two 8-byte chunk headers, plus at most three
/// bytes of padding on each chunk.
pub fn predicted_artifact_bytes(plan: &ScenePlan, materials: usize) -> u64 {
    let term = |count: usize, each: usize| (count as u64).saturating_mul(each as u64);
    let bin = term(plan.vertices, 24).saturating_add(term(plan.indices, 2)).saturating_add(term(plan.meshes, 9));
    let json = (json_budget::FIXED as u64)
        .saturating_add(term(plan.meshes, 3 * json_budget::ACCESSOR + 3 * json_budget::BUFFER_VIEW + json_budget::MESH))
        .saturating_add(term(plan.nodes, json_budget::NODE + json_budget::CHILD_REF))
        .saturating_add(term(materials, json_budget::MATERIAL));
    // the GLB frame: a 12-byte header, two 8-byte chunk headers and at most three bytes of
    // padding on each chunk
    bin.saturating_add(json).saturating_add(12 + 8 + 8 + 3 + 3)
}

fn transformer_err(msg: String) -> DeriveError {
    DeriveError::Transformer(msg)
}

/// SA-2's `max_dsl_bytes`, on whichever byte string is in hand.
pub fn check_dsl_bytes(len: usize, what: &str, refuse: fn(String) -> DeriveError) -> Result<(), DeriveError> {
    if len as u64 > BOUNDS.max_dsl_bytes {
        return Err(refuse(format!(
            "{what} is {len} bytes, past the declared max_dsl_bytes of {}; a bound exceeded is no object (ADR-0078 SA-2)",
            BOUNDS.max_dsl_bytes
        )));
    }
    Ok(())
}

/// SA-2's `max_steps` and `max_artifact_bytes`, decided on the plan and therefore before the
/// build. Returns the plan so the caller does not count twice.
pub fn check_bounds(dsl: &SceneDsl, refuse: fn(String) -> DeriveError) -> Result<ScenePlan, DeriveError> {
    let plan = plan_scene(dsl);
    if plan.vertices as u64 > BOUNDS.max_steps {
        return Err(refuse(format!(
            "the scene builds {} {STEPS_UNIT}, past the declared max_steps of {}; a bound exceeded is no object \
             (ADR-0078 SA-2), and it is decided before a vertex is allocated",
            plan.vertices, BOUNDS.max_steps
        )));
    }
    let predicted = predicted_artifact_bytes(&plan, dsl.materials.len());
    if predicted > BOUNDS.max_artifact_bytes {
        return Err(refuse(format!(
            "the scene would write at most {predicted} bytes, past the declared max_artifact_bytes of {}; a bound \
             exceeded is no object (ADR-0078 SA-2), and it is decided before a byte is written",
            BOUNDS.max_artifact_bytes
        )));
    }
    Ok(plan)
}

// ---------------------------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------------------------

type Obj = BTreeMap<String, CanonValue>;

fn grammar_err(msg: String) -> DeriveError {
    DeriveError::Grammar(msg)
}

/// Exactly `expected`: an unknown key and a missing key are each a refusal naming the key.
/// Keys are visited in `BTreeMap` order, so the key a refusal names is the same on every host.
fn exact_keys(obj: &Obj, expected: &[&str], ctx: &str) -> Result<(), DeriveError> {
    for key in obj.keys() {
        if !expected.contains(&key.as_str()) {
            return Err(grammar_err(format!("{ctx}unknown key {key:?}")));
        }
    }
    for key in expected {
        if !obj.contains_key(*key) {
            return Err(grammar_err(format!("{ctx}missing key {key:?}")));
        }
    }
    Ok(())
}

fn int_field(obj: &Obj, key: &str, lo: i64, hi: i64, ctx: &str) -> Result<i64, DeriveError> {
    match obj.get(key).and_then(|v| v.as_i64()) {
        Some(n) if (lo..=hi).contains(&n) => Ok(n),
        _ => Err(grammar_err(format!("{ctx}{key:?} must be an integer in {lo}..={hi}"))),
    }
}

fn str_field<'a>(obj: &'a Obj, key: &str, ctx: &str) -> Result<&'a str, DeriveError> {
    let s = obj.get(key).and_then(|v| v.as_str()).ok_or_else(|| grammar_err(format!("{ctx}{key:?} must be a string")))?;
    if s.len() > MAX_NAME_BYTES {
        return Err(grammar_err(format!("{ctx}{key:?} is {} bytes; at most {MAX_NAME_BYTES}", s.len())));
    }
    Ok(s)
}

fn bool_field(obj: &Obj, key: &str, ctx: &str) -> Result<bool, DeriveError> {
    obj.get(key).and_then(|v| v.as_bool()).ok_or_else(|| grammar_err(format!("{ctx}{key:?} must be true or false")))
}

/// A fixed-length array of coordinates, each in `-COORD_LIMIT..=COORD_LIMIT`.
fn coords_field<const N: usize>(obj: &Obj, key: &str, ctx: &str) -> Result<[i64; N], DeriveError> {
    let refuse = || grammar_err(format!("{ctx}{key:?} must be {N} integers in {}..={COORD_LIMIT}", -COORD_LIMIT));
    let arr = obj.get(key).and_then(|v| v.as_arr()).ok_or_else(refuse)?;
    if arr.len() != N {
        return Err(refuse());
    }
    let mut out = [0i64; N];
    for (slot, item) in out.iter_mut().zip(arr) {
        *slot = match item.as_i64() {
            Some(n) if (-COORD_LIMIT..=COORD_LIMIT).contains(&n) => n,
            _ => return Err(refuse()),
        };
    }
    Ok(out)
}

/// One material, as the DSL declares it. Channels are over [`CHANNEL_DENOMINATOR`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Material {
    pub name: String,
    pub base_color: [i64; 4],
    pub metallic: i64,
    pub roughness: i64,
    pub double_sided: bool,
}

/// A primitive, in fixed-point units at the document's `frac_bits`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shape {
    /// An axis-aligned box from `min` to `max`, `min < max` componentwise.
    Box { min: [i64; 3], max: [i64; 3] },
    /// A quad in the plane `y = 0`, facing `+Y`, from `(min.x, min.z)` to `(max.x, max.z)`.
    Plane { min: [i64; 2], max: [i64; 2] },
    /// One triangle, wound as written; its normal is `(b - a) × (c - a)`.
    Triangle { a: [i64; 3], b: [i64; 3], c: [i64; 3] },
    /// A strictly convex polygon in the XZ plane, extruded from `y_min` to `y_max`.
    Prism { base: Vec<[i64; 2]>, y_min: i64, y_max: i64 },
}

/// One node of the DSL's forest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub name: String,
    pub translation: [i64; 3],
    /// The quaternion over a denominator of two; `x² + y² + z² + w² == ROTATION_NORM`.
    pub rotation: [i64; 4],
    pub scale: [i64; 3],
    pub shape: Option<Shape>,
    /// The index into [`SceneDsl::materials`] of the material this node's shape wears.
    pub material: Option<usize>,
    pub children: Vec<Node>,
}

/// A validated `scene/v1` document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneDsl {
    pub frac_bits: u32,
    pub materials: Vec<Material>,
    pub nodes: Vec<Node>,
}

impl SceneDsl {
    /// Parse and validate `bytes` (canonical or not) under `scene/v1`.
    pub fn parse(bytes: &[u8]) -> Result<Self, DeriveError> {
        Self::from_tree(&parse_canonical(bytes)?)
    }

    /// Validate a parsed tree against the schema in the module doc.
    pub fn from_tree(tree: &CanonValue) -> Result<Self, DeriveError> {
        let obj = tree.as_obj().ok_or_else(|| grammar_err("scene/v1: the DSL must be a JSON object".into()))?;
        exact_keys(obj, &["v", "frac_bits", "materials", "nodes"], "")?;
        match obj.get("v") {
            Some(CanonValue::Int(1)) => {}
            _ => return Err(grammar_err("\"v\" must be 1".into())),
        }
        let frac_bits = match obj.get("frac_bits").and_then(|v| v.as_i64()) {
            Some(n) if FRAC_BITS_ALLOWED.contains(&(n as i128)) => n as u32,
            _ => {
                let list = FRAC_BITS_ALLOWED.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(", ");
                return Err(grammar_err(format!("\"frac_bits\" must be one of {list}")));
            }
        };

        let materials_arr =
            obj.get("materials").and_then(|v| v.as_arr()).ok_or_else(|| grammar_err("\"materials\" must be an array".into()))?;
        if materials_arr.is_empty() || materials_arr.len() > MAX_MATERIALS {
            return Err(grammar_err(format!("\"materials\" holds 1..={MAX_MATERIALS} materials ({} given)", materials_arr.len())));
        }
        let mut materials = Vec::with_capacity(materials_arr.len());
        let mut seen = BTreeSet::new();
        for (i, m) in materials_arr.iter().enumerate() {
            let material = parse_material(m, i)?;
            if !seen.insert(material.name.clone()) {
                return Err(grammar_err(format!("materials[{i}]: the name {:?} is declared twice", material.name)));
            }
            materials.push(material);
        }
        let by_name: BTreeMap<&str, usize> = materials.iter().enumerate().map(|(i, m)| (m.name.as_str(), i)).collect();

        let nodes_arr = obj.get("nodes").and_then(|v| v.as_arr()).ok_or_else(|| grammar_err("\"nodes\" must be an array".into()))?;
        if nodes_arr.is_empty() {
            return Err(grammar_err("\"nodes\" holds at least one node".into()));
        }
        let mut counted = 0usize;
        let mut shapes = 0usize;
        let mut nodes = Vec::with_capacity(nodes_arr.len());
        for (i, n) in nodes_arr.iter().enumerate() {
            nodes.push(parse_node(n, &format!("nodes[{i}]"), 1, &by_name, &mut counted, &mut shapes)?);
        }
        if shapes == 0 {
            return Err(grammar_err("a scene declares at least one shape; a glTF with no mesh is a second shape of document".into()));
        }
        Ok(SceneDsl { frac_bits, materials, nodes })
    }
}

fn parse_material(v: &CanonValue, index: usize) -> Result<Material, DeriveError> {
    let ctx = format!("materials[{index}]: ");
    let obj = v.as_obj().ok_or_else(|| grammar_err(format!("{ctx}a material must be a JSON object")))?;
    exact_keys(obj, &["name", "base_color", "metallic", "roughness", "double_sided"], &ctx)?;
    let name = str_field(obj, "name", &ctx)?;
    if name.is_empty() {
        return Err(grammar_err(format!("{ctx}\"name\" must not be empty: a material is referenced by name")));
    }
    let refuse_colour =
        || grammar_err(format!("{ctx}\"base_color\" must be [r,g,b,a] with each channel in 0..={CHANNEL_DENOMINATOR}"));
    let arr = obj.get("base_color").and_then(|v| v.as_arr()).ok_or_else(refuse_colour)?;
    if arr.len() != 4 {
        return Err(refuse_colour());
    }
    let mut base_color = [0i64; 4];
    for (slot, item) in base_color.iter_mut().zip(arr) {
        *slot = match item.as_i64() {
            Some(n) if (0..=CHANNEL_DENOMINATOR).contains(&n) => n,
            _ => return Err(refuse_colour()),
        };
    }
    let metallic = int_field(obj, "metallic", 0, CHANNEL_DENOMINATOR, &ctx)?;
    let roughness = int_field(obj, "roughness", 0, CHANNEL_DENOMINATOR, &ctx)?;
    let double_sided = bool_field(obj, "double_sided", &ctx)?;
    Ok(Material { name: name.to_string(), base_color, metallic, roughness, double_sided })
}

fn parse_node(
    v: &CanonValue,
    ctx_path: &str,
    depth: usize,
    by_name: &BTreeMap<&str, usize>,
    counted: &mut usize,
    shapes: &mut usize,
) -> Result<Node, DeriveError> {
    if depth > MAX_DEPTH {
        return Err(grammar_err(format!("{ctx_path}: nesting is deeper than {MAX_DEPTH}")));
    }
    *counted += 1;
    if *counted > MAX_NODES {
        return Err(grammar_err(format!("a scene holds at most {MAX_NODES} nodes")));
    }
    let ctx = format!("{ctx_path}: ");
    let obj = v.as_obj().ok_or_else(|| grammar_err(format!("{ctx}a node must be a JSON object")))?;
    exact_keys(obj, &["name", "translation", "rotation", "scale", "shape", "material", "children"], &ctx)?;
    let name = str_field(obj, "name", &ctx)?.to_string();
    let translation = coords_field::<3>(obj, "translation", &ctx)?;
    let rotation = parse_rotation(obj, &ctx)?;
    let refuse_scale = || grammar_err(format!("{ctx}\"scale\" must be three integers in 1..={SCALE_MAX}"));
    let scale_arr = obj.get("scale").and_then(|v| v.as_arr()).ok_or_else(refuse_scale)?;
    if scale_arr.len() != 3 {
        return Err(refuse_scale());
    }
    let mut scale = [0i64; 3];
    for (slot, item) in scale.iter_mut().zip(scale_arr) {
        *slot = match item.as_i64() {
            Some(n) if (1..=SCALE_MAX).contains(&n) => n,
            _ => return Err(refuse_scale()),
        };
    }

    let shape = match obj.get("shape") {
        Some(CanonValue::Null) => None,
        Some(other) => Some(parse_shape(other, &ctx)?),
        None => unreachable!("exact_keys demanded \"shape\""),
    };
    let material = match (obj.get("material"), &shape) {
        (Some(CanonValue::Null), None) => None,
        (Some(CanonValue::Str(want)), Some(_)) => Some(*by_name.get(want.as_str()).ok_or_else(|| {
            let known = by_name.keys().map(|k| format!("{k:?}")).collect::<Vec<_>>().join(", ");
            grammar_err(format!("{ctx}\"material\" names {want:?}, which is not declared; the scene declares {known}"))
        })?),
        (_, Some(_)) => {
            return Err(grammar_err(format!("{ctx}a node with a shape names a material: every mesh wears one")));
        }
        (_, None) => {
            return Err(grammar_err(format!("{ctx}a node without a shape has no material: \"material\" must be null")));
        }
    };
    if shape.is_some() {
        *shapes += 1;
    }

    let children_arr =
        obj.get("children").and_then(|v| v.as_arr()).ok_or_else(|| grammar_err(format!("{ctx}\"children\" must be an array")))?;
    let mut children = Vec::with_capacity(children_arr.len());
    for (i, c) in children_arr.iter().enumerate() {
        children.push(parse_node(c, &format!("{ctx_path}.children[{i}]"), depth + 1, by_name, counted, shapes)?);
    }
    Ok(Node { name, translation, rotation, scale, shape, material, children })
}

/// The quaternion, held to the exact-unit rule of the module doc.
fn parse_rotation(obj: &Obj, ctx: &str) -> Result<[i64; 4], DeriveError> {
    let refuse = || {
        grammar_err(format!(
            "{ctx}\"rotation\" must be four integers [x,y,z,w] with x²+y²+z²+w² == {ROTATION_NORM}; \
             the identity is [0,0,0,2]. Only a dyadic unit quaternion is exactly representable as \
             binary32, and every one of them reduces to this normal form — the twelve rotations of \
             the tetrahedral group. A 45° turn asks for √2/2, which is not one, and is refused \
             rather than rounded"
        ))
    };
    let arr = obj.get("rotation").and_then(|v| v.as_arr()).ok_or_else(refuse)?;
    if arr.len() != 4 {
        return Err(refuse());
    }
    let mut q = [0i64; 4];
    for (slot, item) in q.iter_mut().zip(arr) {
        *slot = match item.as_i64() {
            Some(n) if (-2..=2).contains(&n) => n,
            _ => return Err(refuse()),
        };
    }
    if q.iter().map(|c| c * c).sum::<i64>() != ROTATION_NORM {
        return Err(refuse());
    }
    Ok(q)
}

fn parse_shape(v: &CanonValue, ctx: &str) -> Result<Shape, DeriveError> {
    let obj = v.as_obj().ok_or_else(|| grammar_err(format!("{ctx}\"shape\" must be null or a JSON object")))?;
    let name = match obj.get("shape") {
        Some(CanonValue::Str(s)) => s.as_str(),
        Some(_) => return Err(grammar_err(format!("{ctx}\"shape\".\"shape\" must be a string"))),
        None => return Err(grammar_err(format!("{ctx}\"shape\" is missing key \"shape\""))),
    };
    let ctx = format!("{ctx}shape {name:?}: ");
    match name {
        "box" => {
            exact_keys(obj, &["shape", "min", "max"], &ctx)?;
            let min = coords_field::<3>(obj, "min", &ctx)?;
            let max = coords_field::<3>(obj, "max", &ctx)?;
            for axis in 0..3 {
                if min[axis] >= max[axis] {
                    return Err(grammar_err(format!("{ctx}\"min\" must be below \"max\" on every axis (axis {axis})")));
                }
            }
            Ok(Shape::Box { min, max })
        }
        "plane" => {
            exact_keys(obj, &["shape", "min", "max"], &ctx)?;
            let min = coords_field::<2>(obj, "min", &ctx)?;
            let max = coords_field::<2>(obj, "max", &ctx)?;
            for axis in 0..2 {
                if min[axis] >= max[axis] {
                    return Err(grammar_err(format!("{ctx}\"min\" must be below \"max\" on both axes (axis {axis})")));
                }
            }
            Ok(Shape::Plane { min, max })
        }
        "triangle" => {
            exact_keys(obj, &["shape", "a", "b", "c"], &ctx)?;
            let a = coords_field::<3>(obj, "a", &ctx)?;
            let b = coords_field::<3>(obj, "b", &ctx)?;
            let c = coords_field::<3>(obj, "c", &ctx)?;
            if cross(sub(b, a), sub(c, a)) == [0, 0, 0] {
                return Err(grammar_err(format!("{ctx}the three points are collinear: a degenerate face has no normal")));
            }
            Ok(Shape::Triangle { a, b, c })
        }
        "prism" => {
            exact_keys(obj, &["shape", "base", "y_min", "y_max"], &ctx)?;
            let y_min = int_field(obj, "y_min", -COORD_LIMIT, COORD_LIMIT, &ctx)?;
            let y_max = int_field(obj, "y_max", -COORD_LIMIT, COORD_LIMIT, &ctx)?;
            if y_min >= y_max {
                return Err(grammar_err(format!("{ctx}\"y_min\" must be below \"y_max\"")));
            }
            let base_arr =
                obj.get("base").and_then(|v| v.as_arr()).ok_or_else(|| grammar_err(format!("{ctx}\"base\" must be an array")))?;
            if !(PRISM_MIN_POINTS..=PRISM_MAX_POINTS).contains(&base_arr.len()) {
                return Err(grammar_err(format!(
                    "{ctx}\"base\" holds {PRISM_MIN_POINTS}..={PRISM_MAX_POINTS} points ({} given)",
                    base_arr.len()
                )));
            }
            let mut base = Vec::with_capacity(base_arr.len());
            for (i, p) in base_arr.iter().enumerate() {
                let refuse =
                    || grammar_err(format!("{ctx}\"base\"[{i}] must be [x, z] with each in {}..={COORD_LIMIT}", -COORD_LIMIT));
                let pair = p.as_arr().ok_or_else(refuse)?;
                if pair.len() != 2 {
                    return Err(refuse());
                }
                let mut xz = [0i64; 2];
                for (slot, item) in xz.iter_mut().zip(pair) {
                    *slot = match item.as_i64() {
                        Some(n) if (-COORD_LIMIT..=COORD_LIMIT).contains(&n) => n,
                        _ => return Err(refuse()),
                    };
                }
                base.push(xz);
            }
            check_prism_base(&base, &ctx)?;
            Ok(Shape::Prism { base, y_min, y_max })
        }
        "sphere" => Err(grammar_err(format!(
            "{ctx}not covered. A subdivided icosahedron's vertices contain φ = (1+√5)/2 and any \
             normalization divides by a square root, so there is no exact integer sphere. Write \
             the approximation in the DSL — as \"prism\" or as \"triangle\"s — where it is \
             visible and hashed, instead of asking the transformer to round"
        ))),
        "cylinder" => Err(grammar_err(format!(
            "{ctx}not covered. A regular n-gon needs cos(2π/n), irrational for every n but 4. \
             Use \"prism\" over a base polygon the answer lists explicitly: a sixteen-sided \
             cylinder is sixteen integer points, and the rounding is then the model's, in the \
             DSL, rather than the transformer's, hidden in the bytes"
        ))),
        other => {
            Err(grammar_err(format!("{ctx}unknown shape {other:?}; this build makes \"box\", \"plane\", \"triangle\" and \"prism\"")))
        }
    }
}

/// A prism's base must be a strictly convex polygon wound so that the TOP cap faces `+Y`.
///
/// Both conditions are integer identities, checked exactly. The winding condition is stated as
/// the sign of the doubled signed area in the `(x, z)` plane, `Σ (x_i·z_{i+1} − x_{i+1}·z_i)`,
/// which must be NEGATIVE: with `+Y` up and a right-handed frame, that is the order whose fan
/// triangles have a `+Y` cross product. Strict convexity — every consecutive turn the same
/// non-zero sign — is what lets the caps be a fan from vertex 0 without a general triangulator,
/// and it rules out the collinear vertex, which is a face with no normal.
fn check_prism_base(base: &[[i64; 2]], ctx: &str) -> Result<(), DeriveError> {
    let n = base.len();
    let mut area2: i128 = 0;
    for i in 0..n {
        let p = base[i];
        let q = base[(i + 1) % n];
        area2 += p[0] as i128 * q[1] as i128 - q[0] as i128 * p[1] as i128;
    }
    if area2 >= 0 {
        return Err(grammar_err(format!(
            "{ctx}\"base\" must be wound so that Σ(x_i·z_{{i+1}} − x_{{i+1}}·z_i) is negative — the \
             winding whose top cap faces +Y; it is {area2}. Reverse the point order"
        )));
    }
    for i in 0..n {
        let a = base[i];
        let b = base[(i + 1) % n];
        let c = base[(i + 2) % n];
        let turn = (b[0] as i128 - a[0] as i128) * (c[1] as i128 - b[1] as i128)
            - (b[1] as i128 - a[1] as i128) * (c[0] as i128 - b[0] as i128);
        if turn >= 0 {
            return Err(grammar_err(format!(
                "{ctx}\"base\" must be strictly convex; the turn at point {} is {turn}, which is \
                 flat or reflex. A fan from point 0 only triangulates a convex polygon, and a \
                 collinear vertex is a face with no normal",
                (i + 1) % n
            )));
        }
    }
    Ok(())
}

/// The scene behind canonical `scene/v1` bytes. The transformer repairs nothing: input that is
/// not exactly the grammar's own output — unparseable, off-schema, or merely spelled
/// differently — is refused as `DeriveError::Transformer`.
pub fn canonical_scene(dsl: &[u8]) -> Result<SceneDsl, DeriveError> {
    let not_canonical = |e: DeriveError| DeriveError::Transformer(format!("input is not canonical scene/v1: {e}"));
    let tree = parse_canonical(dsl).map_err(not_canonical)?;
    let scene = SceneDsl::from_tree(&tree).map_err(not_canonical)?;
    if write_canonical(&tree) != dsl {
        return Err(DeriveError::Transformer("input is not canonical scene/v1: the bytes differ from their canonical form".into()));
    }
    Ok(scene)
}

// ---------------------------------------------------------------------------------------------
// Integer vector helpers
// ---------------------------------------------------------------------------------------------

fn sub(a: [i64; 3], b: [i64; 3]) -> [i64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// The cross product in `i128`, because the operands are differences of coordinates bounded by
/// `2^30` and their products need 62 bits before the subtraction.
fn cross(u: [i64; 3], v: [i64; 3]) -> [i128; 3] {
    let (u, v) = ([u[0] as i128, u[1] as i128, u[2] as i128], [v[0] as i128, v[1] as i128, v[2] as i128]);
    [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]
}

/// The exact unit normal of a face whose cross product is `c`, or `None` when there is none in
/// this arithmetic. Exactly one non-zero component means the direction is an axis and the unit
/// normal is `±1` on it; anything else would need a square root (see the module doc's descent:
/// the six axes are the ONLY exact dyadic unit normals), and a mesh containing such a face
/// omits `NORMAL` so that the consumer computes flat normals under the glTF specification's own
/// rule. An all-zero cross is a degenerate face and is refused upstream, never silently kept.
fn axis_normal(c: [i128; 3]) -> Option<[i8; 3]> {
    let nonzero = c.iter().filter(|v| **v != 0).count();
    if nonzero != 1 {
        return None;
    }
    let mut out = [0i8; 3];
    for (slot, value) in out.iter_mut().zip(c) {
        *slot = match value.signum() {
            1 => 1,
            -1 => -1,
            _ => 0,
        };
    }
    Some(out)
}

// ---------------------------------------------------------------------------------------------
// Exact dyadic arithmetic — the composed hierarchy
// ---------------------------------------------------------------------------------------------

/// An exact dyadic rational `m / 2^f`, always normalized so that `m` is odd or zero. Two
/// `Dyadic`s are equal iff they are the same number, which is what makes `PartialEq` meaningful
/// and what keeps `f` from growing without bound as a hierarchy composes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dyadic {
    pub m: i128,
    pub f: u32,
}

fn inexact(msg: String) -> DeriveError {
    DeriveError::Inexact(msg)
}

impl Dyadic {
    /// Normalize on construction: strip the common factors of two. Exact — a shift by a zero bit
    /// loses nothing — and it is what bounds `f`.
    pub fn new(mut m: i128, mut f: u32) -> Dyadic {
        if m == 0 {
            return Dyadic { m: 0, f: 0 };
        }
        while f > 0 && m % 2 == 0 {
            m /= 2;
            f -= 1;
        }
        Dyadic { m, f }
    }

    pub const ZERO: Dyadic = Dyadic { m: 0, f: 0 };
    pub const ONE: Dyadic = Dyadic { m: 1, f: 0 };

    /// The fixed-point value `v / 2^frac_bits`.
    pub fn fixed(v: i64, frac_bits: u32) -> Dyadic {
        Dyadic::new(v as i128, frac_bits)
    }

    fn guard(self, what: &str) -> Result<Dyadic, DeriveError> {
        if self.f > DYADIC_MAX_FRAC_BITS {
            return Err(inexact(format!(
                "the composed transform's {what} needs {} fractional bits; exact composition carries \
                 at most {DYADIC_MAX_FRAC_BITS}, and rounding here is the float behaviour ADR-0078 \
                 Decision 3 refuses",
                self.f
            )));
        }
        if self.m.unsigned_abs() > DYADIC_MAX_MAGNITUDE as u128 {
            return Err(inexact(format!("the composed transform's {what} exceeds the exact-composition magnitude bound")));
        }
        Ok(self)
    }

    pub fn mul(self, other: Dyadic, what: &str) -> Result<Dyadic, DeriveError> {
        let m = self
            .m
            .checked_mul(other.m)
            .ok_or_else(|| inexact(format!("the composed transform's {what} overflowed exact integer composition")))?;
        let f = self.f + other.f;
        Dyadic::new(m, f).guard(what)
    }

    pub fn add(self, other: Dyadic, what: &str) -> Result<Dyadic, DeriveError> {
        let f = self.f.max(other.f);
        let lift = |d: Dyadic| -> Result<i128, DeriveError> {
            let shift = f - d.f;
            if shift >= 127 {
                return Err(inexact(format!("the composed transform's {what} overflowed exact integer composition")));
            }
            d.m.checked_mul(1i128 << shift)
                .ok_or_else(|| inexact(format!("the composed transform's {what} overflowed exact integer composition")))
        };
        let m = lift(self)?
            .checked_add(lift(other)?)
            .ok_or_else(|| inexact(format!("the composed transform's {what} overflowed exact integer composition")))?;
        Dyadic::new(m, f).guard(what)
    }

    /// `|self| <= limit / 2^frac_bits`, decided exactly by cross-multiplication.
    pub fn abs_within(self, limit: i64, frac_bits: u32, what: &str) -> Result<bool, DeriveError> {
        let overflow = || inexact(format!("the composed transform's {what} overflowed the exact extent comparison"));
        let shift = |value: i128, by: u32| -> Result<i128, DeriveError> {
            if by >= 127 {
                return Err(overflow());
            }
            value.checked_mul(1i128 << by).ok_or_else(overflow)
        };
        // |m| / 2^f <= limit / 2^frac_bits  <=>  |m| * 2^frac_bits <= limit * 2^f
        Ok(shift(self.m.unsigned_abs() as i128, frac_bits)? <= shift(limit as i128, self.f)?)
    }
}

/// An affine transform with an exact dyadic linear part and translation. Every transform this
/// module composes is a signed permutation (the rotation) times a positive diagonal (the scale),
/// so the family is closed under composition and the image of an axis-aligned box is the box of
/// the images of its corners — which is why the world extent can be checked on eight points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transform {
    pub linear: [[Dyadic; 3]; 3],
    pub translation: [Dyadic; 3],
}

impl Transform {
    pub fn identity() -> Transform {
        let mut linear = [[Dyadic::ZERO; 3]; 3];
        for (i, row) in linear.iter_mut().enumerate() {
            row[i] = Dyadic::ONE;
        }
        Transform { linear, translation: [Dyadic::ZERO; 3] }
    }

    /// A node's own transform: `T · R · S`, the order glTF states for a node's TRS.
    pub fn from_node(node: &Node, frac_bits: u32) -> Result<Transform, DeriveError> {
        let r = rotation_matrix(node.rotation)?;
        let mut linear = [[Dyadic::ZERO; 3]; 3];
        for (i, row) in linear.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                // R · S: column j of R scaled by the j-th scale component.
                *cell = Dyadic::new(r[i][j] as i128, 0).mul(Dyadic::fixed(node.scale[j], frac_bits), "scale")?;
            }
        }
        let translation = [
            Dyadic::fixed(node.translation[0], frac_bits),
            Dyadic::fixed(node.translation[1], frac_bits),
            Dyadic::fixed(node.translation[2], frac_bits),
        ];
        Ok(Transform { linear, translation })
    }

    /// `self ∘ child`: apply the child first, then this one.
    pub fn compose(&self, child: &Transform) -> Result<Transform, DeriveError> {
        let mut linear = [[Dyadic::ZERO; 3]; 3];
        for (i, row) in linear.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                let mut acc = Dyadic::ZERO;
                for k in 0..3 {
                    acc = acc.add(self.linear[i][k].mul(child.linear[k][j], "linear part")?, "linear part")?;
                }
                *cell = acc;
            }
        }
        let mut translation = [Dyadic::ZERO; 3];
        for (i, slot) in translation.iter_mut().enumerate() {
            let mut acc = self.translation[i];
            for k in 0..3 {
                acc = acc.add(self.linear[i][k].mul(child.translation[k], "translation")?, "translation")?;
            }
            *slot = acc;
        }
        Ok(Transform { linear, translation })
    }

    /// The image of a fixed-point point.
    pub fn apply(&self, point: [i64; 3], frac_bits: u32) -> Result<[Dyadic; 3], DeriveError> {
        let p = [Dyadic::fixed(point[0], frac_bits), Dyadic::fixed(point[1], frac_bits), Dyadic::fixed(point[2], frac_bits)];
        let mut out = [Dyadic::ZERO; 3];
        for (i, slot) in out.iter_mut().enumerate() {
            let mut acc = self.translation[i];
            for (k, component) in p.iter().enumerate() {
                acc = acc.add(self.linear[i][k].mul(*component, "world position")?, "world position")?;
            }
            *slot = acc;
        }
        Ok(out)
    }
}

/// The rotation matrix of a `scene/v1` quaternion `q / 2`.
///
/// With `X = x/2` and the usual quaternion-to-matrix identities, every entry is
/// `(integer) / 2`, and the norm identity `x²+y²+z²+w² = 4` makes each of those integers even —
/// so every entry lands in `{0, ±1}` and the matrix is a signed permutation with determinant
/// `+1`. That is the whole reason a hierarchy of these composes exactly forever. The refusal
/// below should be unreachable given [`parse_rotation`]; it is written as a refusal, by name,
/// rather than an assumption, because an unreachable state that produces a wrong mesh is worse
/// than an unreachable state that produces an error.
pub fn rotation_matrix(q: [i64; 4]) -> Result<[[i8; 3]; 3], DeriveError> {
    let [x, y, z, w] = q;
    let numerators = [
        [2 - y * y - z * z, x * y - z * w, x * z + y * w],
        [x * y + z * w, 2 - x * x - z * z, y * z - x * w],
        [x * z - y * w, y * z + x * w, 2 - x * x - y * y],
    ];
    let mut out = [[0i8; 3]; 3];
    for (row_out, row) in out.iter_mut().zip(numerators) {
        for (cell, n) in row_out.iter_mut().zip(row) {
            if n % 2 != 0 || !(-2..=2).contains(&n) {
                return Err(DeriveError::Transformer(format!(
                    "rotation {q:?} does not give an exact {{0,±1}} matrix (entry numerator {n}); \
                     only the twelve tetrahedral rotations are admissible"
                )));
            }
            *cell = (n / 2) as i8;
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// The fixed-point mesh builder
// ---------------------------------------------------------------------------------------------

/// A built mesh in the document's fixed-point units. `normals` is `Some` exactly when every
/// face of this mesh is axis-aligned (see [`axis_normal`]); otherwise the primitive omits
/// `NORMAL` and the consumer computes flat normals under the glTF rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mesh {
    pub positions: Vec<[i64; 3]>,
    pub normals: Option<Vec<[i8; 3]>>,
    pub indices: Vec<u16>,
}

impl Mesh {
    /// The componentwise extremes of the positions, for the accessor's `min`/`max`.
    pub fn bounds(&self) -> ([i64; 3], [i64; 3]) {
        let mut min = self.positions[0];
        let mut max = self.positions[0];
        for p in &self.positions {
            for axis in 0..3 {
                min[axis] = min[axis].min(p[axis]);
                max[axis] = max[axis].max(p[axis]);
            }
        }
        (min, max)
    }
}

/// One face of a builder's working set: four or three positions in winding order.
struct Face {
    positions: Vec<[i64; 3]>,
}

/// Assemble a mesh from faces: vertices in face order (each face owns its own, so a normal is
/// per-face and flat), a triangle fan inside each face, and a `NORMAL` attribute only when
/// every face's normal is exact.
fn assemble(faces: Vec<Face>) -> Result<Mesh, DeriveError> {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut indices = Vec::new();
    let mut all_axis_aligned = true;
    for face in faces {
        let base = positions.len();
        let c = cross(sub(face.positions[1], face.positions[0]), sub(face.positions[2], face.positions[0]));
        if c == [0, 0, 0] {
            return Err(DeriveError::Transformer("a degenerate face reached the builder: it has no normal in any arithmetic".into()));
        }
        let normal = axis_normal(c);
        if normal.is_none() {
            all_axis_aligned = false;
        }
        let normal = normal.unwrap_or([0, 0, 0]);
        for p in &face.positions {
            positions.push(*p);
            normals.push(normal);
        }
        if positions.len() > MAX_MESH_VERTICES {
            return Err(DeriveError::Transformer(format!(
                "a mesh holds at most {MAX_MESH_VERTICES} vertices: the index type is UNSIGNED_SHORT and \
                 this build never promotes it, so an overflow is a refusal and not a second index rule"
            )));
        }
        for k in 1..face.positions.len() - 1 {
            indices.push(base as u16);
            indices.push((base + k) as u16);
            indices.push((base + k + 1) as u16);
        }
    }
    Ok(Mesh { positions, normals: if all_axis_aligned { Some(normals) } else { None }, indices })
}

/// The six faces of an axis-aligned box, in the pinned order `+X, −X, +Y, −Y, +Z, −Z`, each
/// wound counter-clockwise as seen from outside — which is what makes glTF's default front
/// face the outside one.
fn box_faces(min: [i64; 3], max: [i64; 3]) -> Vec<Face> {
    let ([x0, y0, z0], [x1, y1, z1]) = (min, max);
    let f = |a: [i64; 3], b: [i64; 3], c: [i64; 3], d: [i64; 3]| Face { positions: vec![a, b, c, d] };
    vec![
        f([x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1]),
        f([x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]),
        f([x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0]),
        f([x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]),
        f([x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]),
        f([x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0]),
    ]
}

/// The faces of a prism: the bottom cap in reversed base order (so it faces `−Y`), the top cap
/// in base order (so it faces `+Y`), then one quad per base edge in edge order.
fn prism_faces(base: &[[i64; 2]], y_min: i64, y_max: i64) -> Vec<Face> {
    let n = base.len();
    let mut faces = Vec::with_capacity(2 + n);
    faces.push(Face { positions: base.iter().rev().map(|p| [p[0], y_min, p[1]]).collect() });
    faces.push(Face { positions: base.iter().map(|p| [p[0], y_max, p[1]]).collect() });
    for i in 0..n {
        let p = base[i];
        let q = base[(i + 1) % n];
        faces.push(Face { positions: vec![[p[0], y_min, p[1]], [q[0], y_min, q[1]], [q[0], y_max, q[1]], [p[0], y_max, p[1]]] });
    }
    faces
}

/// Build one shape's mesh.
pub fn build_mesh(shape: &Shape) -> Result<Mesh, DeriveError> {
    let faces = match shape {
        Shape::Box { min, max } => box_faces(*min, *max),
        Shape::Plane { min, max } => {
            let (x0, z0, x1, z1) = (min[0], min[1], max[0], max[1]);
            vec![Face { positions: vec![[x0, 0, z1], [x1, 0, z1], [x1, 0, z0], [x0, 0, z0]] }]
        }
        Shape::Triangle { a, b, c } => vec![Face { positions: vec![*a, *b, *c] }],
        Shape::Prism { base, y_min, y_max } => prism_faces(base, *y_min, *y_max),
    };
    assemble(faces)
}

// ---------------------------------------------------------------------------------------------
// The flattened glTF node tree and its meshes
// ---------------------------------------------------------------------------------------------

/// One glTF node: the DSL's local T/R/S, the mesh it carries, and its children's glTF indices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlatNode {
    pub name: String,
    pub translation: [i64; 3],
    pub rotation: [i64; 4],
    pub scale: [i64; 3],
    pub mesh: Option<usize>,
    pub children: Vec<usize>,
}

/// A mesh with the material index its node named.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuiltMesh {
    pub mesh: Mesh,
    pub material: usize,
}

/// The whole scene, built: the flattened node tree, the meshes, and the roots of the glTF scene.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuiltScene {
    pub frac_bits: u32,
    pub materials: Vec<Material>,
    pub nodes: Vec<FlatNode>,
    pub meshes: Vec<BuiltMesh>,
    pub roots: Vec<usize>,
    /// Each mesh's world-space bounding box, composed exactly (see [`Transform`]). It does not
    /// reach the bytes — the node tree carries the hierarchy — but it reaches the verdict.
    pub world_bounds: Vec<([Dyadic; 3], [Dyadic; 3])>,
}

/// Flatten the DSL's forest depth-first, pre-order, building each shape's mesh and composing
/// each node's world transform exactly as it goes.
pub fn build_scene(dsl: &SceneDsl) -> Result<BuiltScene, DeriveError> {
    let mut built = BuiltScene {
        frac_bits: dsl.frac_bits,
        materials: dsl.materials.clone(),
        nodes: Vec::new(),
        meshes: Vec::new(),
        roots: Vec::new(),
        world_bounds: Vec::new(),
    };
    let mut vertices = 0usize;
    let roots = flatten(&dsl.nodes, &Transform::identity(), dsl.frac_bits, &mut built, &mut vertices)?;
    built.roots = roots;
    Ok(built)
}

fn flatten(
    siblings: &[Node],
    parent: &Transform,
    frac_bits: u32,
    built: &mut BuiltScene,
    vertices: &mut usize,
) -> Result<Vec<usize>, DeriveError> {
    let mut ids = Vec::with_capacity(siblings.len());
    for node in siblings {
        let id = built.nodes.len();
        ids.push(id);
        // The index is claimed before the subtree is walked, which is what makes the numbering
        // pre-order and a subtree's indices contiguous.
        built.nodes.push(FlatNode {
            name: node.name.clone(),
            translation: node.translation,
            rotation: node.rotation,
            scale: node.scale,
            mesh: None,
            children: Vec::new(),
        });
        let world = parent.compose(&Transform::from_node(node, frac_bits)?)?;
        if let Some(shape) = &node.shape {
            let mesh = build_mesh(shape)?;
            *vertices += mesh.positions.len();
            if *vertices > MAX_TOTAL_VERTICES {
                return Err(DeriveError::Transformer(format!("a scene holds at most {MAX_TOTAL_VERTICES} vertices in all")));
            }
            let (min, max) = mesh.bounds();
            built.world_bounds.push(world_box(&world, min, max, frac_bits)?);
            built.meshes.push(BuiltMesh {
                mesh,
                material: node.material.expect("the grammar demands a material on every shape-carrying node"),
            });
            built.nodes[id].mesh = Some(built.meshes.len() - 1);
        }
        let children = flatten(&node.children, &world, frac_bits, built, vertices)?;
        built.nodes[id].children = children;
    }
    Ok(ids)
}

/// The world-space bounding box of a local box, and the extent check that refuses a scene the
/// composition places outside `±COORD_LIMIT` units. Eight corners suffice because every
/// admissible linear part is a signed permutation times a positive diagonal.
fn world_box(world: &Transform, min: [i64; 3], max: [i64; 3], frac_bits: u32) -> Result<([Dyadic; 3], [Dyadic; 3]), DeriveError> {
    let mut lo: Option<[Dyadic; 3]> = None;
    let mut hi: Option<[Dyadic; 3]> = None;
    for corner in 0..8u8 {
        let point = [
            if corner & 1 == 0 { min[0] } else { max[0] },
            if corner & 2 == 0 { min[1] } else { max[1] },
            if corner & 4 == 0 { min[2] } else { max[2] },
        ];
        let w = world.apply(point, frac_bits)?;
        for (axis, value) in w.iter().enumerate() {
            if !value.abs_within(COORD_LIMIT, frac_bits, "world extent")? {
                return Err(inexact(format!(
                    "the composed hierarchy places a vertex at {}/2^{} on axis {axis}, outside the \
                     ±{COORD_LIMIT}-unit world bound; the scene is refused rather than clamped",
                    value.m, value.f
                )));
            }
        }
        lo = Some(match lo {
            None => w,
            Some(l) => [dyadic_min(l[0], w[0]), dyadic_min(l[1], w[1]), dyadic_min(l[2], w[2])],
        });
        hi = Some(match hi {
            None => w,
            Some(h) => [dyadic_max(h[0], w[0]), dyadic_max(h[1], w[1]), dyadic_max(h[2], w[2])],
        });
    }
    Ok((lo.expect("eight corners"), hi.expect("eight corners")))
}

/// Compare two dyadics exactly by cross-multiplying to the common scale. Both mantissas are
/// bounded by [`DYADIC_MAX_MAGNITUDE`] and both scales by [`DYADIC_MAX_FRAC_BITS`], and the
/// values reaching here have already passed [`Dyadic::abs_within`], so the shift below cannot
/// overflow; the saturating form is here so that a future caller cannot make it panic.
fn dyadic_le(a: Dyadic, b: Dyadic) -> bool {
    let f = a.f.max(b.f);
    let lift = |d: Dyadic| d.m.saturating_mul(1i128 << (f - d.f).min(126));
    lift(a) <= lift(b)
}

fn dyadic_min(a: Dyadic, b: Dyadic) -> Dyadic {
    if dyadic_le(a, b) { a } else { b }
}

fn dyadic_max(a: Dyadic, b: Dyadic) -> Dyadic {
    if dyadic_le(a, b) { b } else { a }
}

// ---------------------------------------------------------------------------------------------
// The canonical JSON of the glTF document
// ---------------------------------------------------------------------------------------------

/// One value of the glTF JSON document. `Num { m, f }` is the dyadic rational `m / 2^f`; `f = 0`
/// is a plain integer. This is `canon_json`'s value type plus the one form it refuses — a
/// non-integer number — because `canon_json` speaks for the DSLs of this crate, which have none,
/// and a glTF document has them by construction. The writing rules are `canon_json`'s
/// unchanged: sorted keys, no whitespace, and [`write_string`] for every string.
#[derive(Clone, Debug, PartialEq, Eq)]
enum J {
    Num { m: i64, f: u32 },
    Str(String),
    Bool(bool),
    Arr(Vec<J>),
    Obj(BTreeMap<String, J>),
}

impl J {
    fn int(v: i64) -> J {
        J::Num { m: v, f: 0 }
    }
    fn uint(v: u64) -> J {
        J::Num { m: v as i64, f: 0 }
    }
    fn obj(pairs: Vec<(&str, J)>) -> J {
        J::Obj(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }
    fn str(s: &str) -> J {
        J::Str(s.to_string())
    }
}

/// The exact finite decimal of `m / 2^f`.
///
/// Every dyadic rational has one: `m / 2^f = m·5^f / 10^f`. The mantissa is reduced to odd
/// first, so the spelling has no trailing zero and there is exactly one spelling per value —
/// which is the whole requirement, because two spellings of one scene are two artifacts. No
/// exponent form is ever emitted: `1e3` and `1000` are the same JSON number and this writer
/// must not have a choice. All of it is integer arithmetic; the bound below is a refusal
/// rather than a truncation, for the reason every refusal in this file is.
fn write_number(m: i64, f: u32, out: &mut Vec<u8>) -> Result<(), DeriveError> {
    let d = Dyadic::new(m as i128, f);
    if d.m == 0 {
        out.push(b'0');
        return Ok(());
    }
    if d.f == 0 {
        out.extend_from_slice(d.m.to_string().as_bytes());
        return Ok(());
    }
    if d.m < 0 {
        out.push(b'-');
    }
    let pow5 = 5u128
        .checked_pow(d.f)
        .ok_or_else(|| inexact(format!("{}/2^{} needs more decimal digits than exact integer spelling carries", d.m, d.f)))?;
    let pow10 = 10u128
        .checked_pow(d.f)
        .ok_or_else(|| inexact(format!("{}/2^{} needs more decimal digits than exact integer spelling carries", d.m, d.f)))?;
    let scaled =
        d.m.unsigned_abs()
            .checked_mul(pow5)
            .ok_or_else(|| inexact(format!("{}/2^{} needs more decimal digits than exact integer spelling carries", d.m, d.f)))?;
    out.extend_from_slice((scaled / pow10).to_string().as_bytes());
    out.push(b'.');
    let fraction = (scaled % pow10).to_string();
    for _ in fraction.len()..d.f as usize {
        out.push(b'0');
    }
    out.extend_from_slice(fraction.as_bytes());
    Ok(())
}

fn write_json(v: &J, out: &mut Vec<u8>) -> Result<(), DeriveError> {
    match v {
        J::Num { m, f } => write_number(*m, *f, out)?,
        J::Str(s) => write_string(s, out),
        J::Bool(true) => out.extend_from_slice(b"true"),
        J::Bool(false) => out.extend_from_slice(b"false"),
        J::Arr(items) => {
            out.push(b'[');
            for (n, item) in items.iter().enumerate() {
                if n > 0 {
                    out.push(b',');
                }
                write_json(item, out)?;
            }
            out.push(b']');
        }
        J::Obj(map) => {
            out.push(b'{');
            for (n, (k, value)) in map.iter().enumerate() {
                if n > 0 {
                    out.push(b',');
                }
                write_string(k, out);
                out.push(b':');
                write_json(value, out)?;
            }
            out.push(b'}');
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The canonical glTF / GLB writer
// ---------------------------------------------------------------------------------------------

/// One bufferView, in the pinned order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferView {
    pub offset: usize,
    pub length: usize,
    pub target: u64,
}

/// One accessor; accessor `k` reads bufferView `k`, which a test checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Accessor {
    pub component_type: u64,
    pub count: usize,
    pub ty: &'static str,
    /// `(mantissa, frac_bits)` per component, exactly the data's extremes.
    pub min: Vec<(i64, u32)>,
    pub max: Vec<(i64, u32)>,
}

/// The binary buffer and the accessor / bufferView tables that describe it.
pub struct Binary {
    pub bytes: Vec<u8>,
    pub views: Vec<BufferView>,
    pub accessors: Vec<Accessor>,
    /// For mesh `i`: `(position accessor, normal accessor or none, index accessor)`.
    pub per_mesh: Vec<(usize, Option<usize>, usize)>,
}

/// Lay out the binary buffer: for each mesh in mesh order, `POSITION`, then `NORMAL` when the
/// mesh has one, then the indices; each view aligned to four bytes with `0x00` fill.
pub fn write_binary(scene: &BuiltScene) -> Result<Binary, DeriveError> {
    let mut bytes = Vec::new();
    let mut views = Vec::new();
    let mut accessors = Vec::new();
    let mut per_mesh = Vec::with_capacity(scene.meshes.len());
    let fb = scene.frac_bits;
    for built in &scene.meshes {
        let mesh = &built.mesh;
        let (min, max) = mesh.bounds();

        pad_to(&mut bytes, 4, GLB_BIN_PAD);
        let position_offset = bytes.len();
        for p in &mesh.positions {
            for component in p {
                // The one place the discipline bites: a fixed-point coordinate becomes a
                // binary32 bit pattern built by integer arithmetic, or the whole derivation is
                // refused. A value binary32 cannot hold exactly is never rounded here, because a
                // rounded coordinate is a coordinate two honest hosts could round apart and X3
                // asks for identical bytes, not for close ones.
                bytes.extend_from_slice(&f32_le_exact(*component, fb)?);
            }
        }
        views.push(BufferView { offset: position_offset, length: bytes.len() - position_offset, target: TARGET_ARRAY_BUFFER });
        let position_accessor = accessors.len();
        accessors.push(Accessor {
            component_type: COMPONENT_FLOAT,
            count: mesh.positions.len(),
            ty: "VEC3",
            min: min.iter().map(|v| (*v, fb)).collect(),
            max: max.iter().map(|v| (*v, fb)).collect(),
        });

        let normal_accessor = match &mesh.normals {
            None => None,
            Some(normals) => {
                pad_to(&mut bytes, 4, GLB_BIN_PAD);
                let offset = bytes.len();
                for n in normals {
                    for component in n {
                        bytes.extend_from_slice(&f32_le_exact(*component as i64, 0)?);
                    }
                }
                views.push(BufferView { offset, length: bytes.len() - offset, target: TARGET_ARRAY_BUFFER });
                let (mut lo, mut hi) = ([1i64; 3], [-1i64; 3]);
                for n in normals {
                    for axis in 0..3 {
                        lo[axis] = lo[axis].min(n[axis] as i64);
                        hi[axis] = hi[axis].max(n[axis] as i64);
                    }
                }
                let index = accessors.len();
                accessors.push(Accessor {
                    component_type: COMPONENT_FLOAT,
                    count: normals.len(),
                    ty: "VEC3",
                    min: lo.iter().map(|v| (*v, 0)).collect(),
                    max: hi.iter().map(|v| (*v, 0)).collect(),
                });
                Some(index)
            }
        };

        pad_to(&mut bytes, 4, GLB_BIN_PAD);
        let index_offset = bytes.len();
        for i in &mesh.indices {
            bytes.extend_from_slice(&i.to_le_bytes());
        }
        views.push(BufferView { offset: index_offset, length: bytes.len() - index_offset, target: TARGET_ELEMENT_ARRAY_BUFFER });
        let index_accessor = accessors.len();
        accessors.push(Accessor {
            component_type: COMPONENT_UNSIGNED_SHORT,
            count: mesh.indices.len(),
            ty: "SCALAR",
            min: vec![(*mesh.indices.iter().min().expect("a mesh has faces") as i64, 0)],
            max: vec![(*mesh.indices.iter().max().expect("a mesh has faces") as i64, 0)],
        });
        per_mesh.push((position_accessor, normal_accessor, index_accessor));
    }
    Ok(Binary { bytes, views, accessors, per_mesh })
}

fn accessor_json(a: &Accessor, view: usize) -> J {
    let numbers = |values: &[(i64, u32)]| J::Arr(values.iter().map(|(m, f)| J::Num { m: *m, f: *f }).collect());
    J::obj(vec![
        ("bufferView", J::uint(view as u64)),
        ("byteOffset", J::int(0)),
        ("componentType", J::uint(a.component_type)),
        ("count", J::uint(a.count as u64)),
        ("max", numbers(&a.max)),
        ("min", numbers(&a.min)),
        ("type", J::str(a.ty)),
    ])
}

fn material_json(m: &Material) -> J {
    let channel = |v: i64| J::Num { m: v, f: CHANNEL_DENOMINATOR.trailing_zeros() };
    J::obj(vec![
        ("doubleSided", J::Bool(m.double_sided)),
        ("name", J::str(&m.name)),
        (
            "pbrMetallicRoughness",
            J::obj(vec![
                ("baseColorFactor", J::Arr(m.base_color.iter().map(|c| channel(*c)).collect())),
                ("metallicFactor", channel(m.metallic)),
                ("roughnessFactor", channel(m.roughness)),
            ]),
        ),
    ])
}

/// The glTF JSON document of a built scene: every array in its pinned order.
pub fn gltf_json(scene: &BuiltScene, binary: &Binary) -> Result<Vec<u8>, DeriveError> {
    let fb = scene.frac_bits;
    let accessors = J::Arr(binary.accessors.iter().enumerate().map(|(k, a)| accessor_json(a, k)).collect());
    let buffer_views = J::Arr(
        binary
            .views
            .iter()
            .map(|v| {
                J::obj(vec![
                    ("buffer", J::int(0)),
                    ("byteLength", J::uint(v.length as u64)),
                    ("byteOffset", J::uint(v.offset as u64)),
                    ("target", J::uint(v.target)),
                ])
            })
            .collect(),
    );
    let meshes = J::Arr(
        binary
            .per_mesh
            .iter()
            .zip(&scene.meshes)
            .map(|((position, normal, indices), built)| {
                let mut attributes = vec![("POSITION", J::uint(*position as u64))];
                if let Some(n) = normal {
                    attributes.push(("NORMAL", J::uint(*n as u64)));
                }
                J::obj(vec![(
                    "primitives",
                    J::Arr(vec![J::obj(vec![
                        ("attributes", J::obj(attributes)),
                        ("indices", J::uint(*indices as u64)),
                        ("material", J::uint(built.material as u64)),
                        ("mode", J::uint(MODE_TRIANGLES)),
                    ])]),
                )])
            })
            .collect(),
    );
    let nodes = J::Arr(
        scene
            .nodes
            .iter()
            .map(|n| {
                let mut fields = vec![
                    ("name", J::str(&n.name)),
                    ("rotation", J::Arr(n.rotation.iter().map(|c| J::Num { m: *c, f: 1 }).collect())),
                    ("scale", J::Arr(n.scale.iter().map(|c| J::Num { m: *c, f: fb }).collect())),
                    ("translation", J::Arr(n.translation.iter().map(|c| J::Num { m: *c, f: fb }).collect())),
                ];
                if let Some(mesh) = n.mesh {
                    fields.push(("mesh", J::uint(mesh as u64)));
                }
                // glTF's schema gives `children` a minimum of one item, so a leaf omits it. It
                // is the only conditional field in this writer, and it is the format's rule.
                if !n.children.is_empty() {
                    fields.push(("children", J::Arr(n.children.iter().map(|c| J::uint(*c as u64)).collect())));
                }
                J::obj(fields)
            })
            .collect(),
    );
    let doc = J::obj(vec![
        ("accessors", accessors),
        ("asset", J::obj(vec![("generator", J::str(WRITER_NAME)), ("version", J::str("2.0"))])),
        ("bufferViews", buffer_views),
        ("buffers", J::Arr(vec![J::obj(vec![("byteLength", J::uint(binary.bytes.len() as u64))])])),
        ("materials", J::Arr(scene.materials.iter().map(material_json).collect())),
        ("meshes", meshes),
        ("nodes", nodes),
        ("scene", J::int(0)),
        ("scenes", J::Arr(vec![J::obj(vec![("nodes", J::Arr(scene.roots.iter().map(|r| J::uint(*r as u64)).collect()))])])),
    ]);
    let mut out = Vec::new();
    write_json(&doc, &mut out)?;
    Ok(out)
}

fn chunk(out: &mut Vec<u8>, ty: u32, body: &[u8], pad: u8) {
    let padded = body.len().next_multiple_of(4);
    put_u32_le(out, padded as u32);
    put_u32_le(out, ty);
    out.extend_from_slice(body);
    for _ in body.len()..padded {
        out.push(pad);
    }
}

/// The canonical binary glTF of a built scene: the header, the JSON chunk, the BIN chunk.
pub fn write_glb(dsl: &SceneDsl) -> Result<Vec<u8>, DeriveError> {
    let scene = build_scene(dsl)?;
    let binary = write_binary(&scene)?;
    let json = gltf_json(&scene, &binary)?;
    let mut out = Vec::with_capacity(28 + json.len() + binary.bytes.len());
    put_u32_le(&mut out, GLB_MAGIC);
    put_u32_le(&mut out, GLB_VERSION);
    put_u32_le(&mut out, 0); // the total length, filled in once both chunks are written
    chunk(&mut out, GLB_CHUNK_JSON, &json, GLB_JSON_PAD);
    chunk(&mut out, GLB_CHUNK_BIN, &binary.bytes, GLB_BIN_PAD);
    let total = out.len() as u32;
    out[8..12].copy_from_slice(&total.to_le_bytes());
    check_artifact_size(out.len())?;
    Ok(out)
}

fn check_artifact_size(len: usize) -> Result<(), DeriveError> {
    if len > ARTIFACT_MAX_BYTES {
        return Err(DeriveError::Transformer(format!("artifact is {len} bytes; at most {ARTIFACT_MAX_BYTES}")));
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::derive::{ClaimBinding, Derivation, derive_named, derive_with};
    use crate::ids::{artifact_hash_v1, dsl_hash_v1, grammar_id_v1, transformer_id};
    use crate::{verify, verify_artifact_bytes};
    use kaspa_consensus_core::palw_derived_v1::PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN;
    use kaspa_hashes::Hash64;
    use std::path::PathBuf;

    /// One box under one material — the smallest scene the grammar admits.
    const CUBE: &str = r#"{
      "v": 1, "frac_bits": 8,
      "materials": [ { "name": "steel", "base_color": [200, 200, 210, 256], "metallic": 230,
                       "roughness": 64, "double_sided": false } ],
      "nodes": [ { "name": "cube", "translation": [0, 0, 0], "rotation": [0, 0, 0, 2],
                   "scale": [256, 256, 256], "material": "steel", "children": [],
                   "shape": { "shape": "box", "min": [-256, -256, -256], "max": [256, 256, 256] } } ]
    }"#;

    /// A two-level hierarchy: a scaled, rotated parent carrying a translated child.
    const HIERARCHY: &str = r#"{
      "v": 1, "frac_bits": 8,
      "materials": [ { "name": "a", "base_color": [256, 0, 0, 256], "metallic": 0, "roughness": 256,
                       "double_sided": false },
                     { "name": "b", "base_color": [0, 0, 256, 128], "metallic": 128, "roughness": 128,
                       "double_sided": true } ],
      "nodes": [ { "name": "root", "translation": [512, 0, 0], "rotation": [1, 1, 1, 1],
                   "scale": [512, 512, 512], "material": null, "shape": null, "children": [
          { "name": "child", "translation": [0, 256, 0], "rotation": [0, 0, 0, 2],
            "scale": [128, 128, 128], "material": "a", "children": [],
            "shape": { "shape": "box", "min": [-64, -64, -64], "max": [64, 64, 64] } },
          { "name": "floor", "translation": [0, -256, 0], "rotation": [2, 0, 0, 0],
            "scale": [256, 256, 256], "material": "b", "children": [],
            "shape": { "shape": "plane", "min": [-512, -512], "max": [512, 512] } } ] } ]
    }"#;

    fn binding() -> ClaimBinding {
        ClaimBinding {
            network_domain: Hash64::from_bytes([0x51; 64]),
            claim_id: Hash64::from_bytes([0x52; 64]),
            output_root: Hash64::from_bytes([0x53; 64]),
            executor_pubkey: vec![0x54; PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN],
        }
    }

    fn canonical(answer: &str) -> Vec<u8> {
        SceneGrammar.canonicalize(answer.as_bytes()).unwrap()
    }

    fn glb(answer: &str) -> Vec<u8> {
        SceneGlbTransformer.run(&canonical(answer)).unwrap().bytes
    }

    #[track_caller]
    fn refused(answer: &str, fragment: &str) {
        match SceneGrammar.canonicalize(answer.as_bytes()) {
            Err(DeriveError::Grammar(msg)) => assert!(msg.contains(fragment), "refusal {msg:?} does not mention {fragment:?}"),
            other => panic!("expected a grammar refusal mentioning {fragment:?}, got {other:?}"),
        }
    }

    /// `CUBE` with one substring replaced — how every refusal below is built.
    fn cube_with(from: &str, to: &str) -> String {
        assert!(CUBE.contains(from), "{from:?} is not in the sample");
        CUBE.replacen(from, to, 1)
    }

    // ----- (1) canonicalization and registration --------------------------------------------

    #[test]
    fn canonicalization_sorts_keys_strips_whitespace_and_is_idempotent() {
        let once = canonical(CUBE);
        let twice = SceneGrammar.canonicalize(&once).unwrap();
        assert_eq!(once, twice);
        assert!(!once.contains(&b' '), "canonical bytes carry no whitespace");
        let text = std::str::from_utf8(&once).unwrap();
        assert!(text.starts_with(r#"{"frac_bits":8,"materials":[{"base_color":[200,200,210,256],"double_sided":false"#), "{text}");
        assert!(text.ends_with(r#""v":1}"#), "{text}");
    }

    #[test]
    fn registration_and_manifest_name_this_build() {
        let (grammars, transformers) = register();
        assert_eq!(grammars.len(), 1);
        assert_eq!(transformers.len(), 1);
        assert_eq!(grammars[0].name(), GRAMMAR_NAME);
        let m = transformers[0].manifest();
        assert_eq!(m.name, "scene/glb/v1");
        assert_eq!(m.kind, kind::SCENE);
        assert_eq!(m.kind, 1);
        assert_eq!(m.grammar, "scene/v1");
        assert_eq!(m.discipline, Discipline::Integer);
        assert_eq!(m.writer, "gltf-binary/2.0/canonical-v1");
        assert_eq!(m.source_tree_sha256, crate::SOURCE_TREE_SHA256_HEX);
        assert!(crate::registry::transformer_by_name(TRANSFORMER_NAME).is_some());
        assert!(crate::registry::grammar_by_name(GRAMMAR_NAME).is_some());
        assert!(crate::registry::transformer_by_id(&transformer_id(&m)).is_some());
        let a = SceneGlbTransformer.run(&canonical(CUBE)).unwrap();
        assert_eq!((a.media_type, a.extension), ("model/gltf-binary", "glb"));
    }

    // ----- (2) every schema refusal, by name ------------------------------------------------

    #[test]
    fn refuses_every_schema_violation_with_a_named_reason() {
        refused("[1]", "the DSL must be a JSON object");
        refused("{", "json");
        refused(r#"{"v":1.5}"#, "non-integer");
        refused(&cube_with(r#""v": 1,"#, r#""v": 1, "extra": 1,"#), "unknown key \"extra\"");
        refused(&cube_with(r#""frac_bits": 8,"#, ""), "missing key \"frac_bits\"");
        refused(&cube_with(r#""v": 1,"#, r#""v": 2,"#), "\"v\" must be 1");
        refused(&cube_with(r#""frac_bits": 8,"#, r#""frac_bits": 5,"#), "must be one of 0, 4, 8, 12, 16");
        refused(&cube_with(r#""materials": ["#, r#""materials": 7, "unused": ["#), "unknown key");
        refused(&cube_with(r#""name": "steel","#, r#""name": "steel", "emissive": 1,"#), "unknown key \"emissive\"");
        refused(&cube_with(r#""name": "steel","#, r#""name": "","#), "must not be empty");
        refused(&cube_with(r#""metallic": 230,"#, r#""metallic": 257,"#), "\"metallic\" must be an integer in 0..=256");
        refused(&cube_with(r#""base_color": [200, 200, 210, 256],"#, r#""base_color": [200, 200, 210],"#), "base_color");
        refused(&cube_with(r#""double_sided": false"#, r#""double_sided": 0"#), "must be true or false");
        refused(&cube_with(r#""rotation": [0, 0, 0, 2],"#, r#""rotation": [0, 0, 0, 1],"#), "x²+y²+z²+w² == 4");
        refused(&cube_with(r#""rotation": [0, 0, 0, 2],"#, r#""rotation": [1, 1, 1, 2],"#), "x²+y²+z²+w² == 4");
        refused(&cube_with(r#""scale": [256, 256, 256],"#, r#""scale": [0, 256, 256],"#), "\"scale\" must be three integers in 1..=");
        refused(&cube_with(r#""scale": [256, 256, 256],"#, r#""scale": [-256, 256, 256],"#), "\"scale\" must be three integers");
        refused(&cube_with(r#""translation": [0, 0, 0],"#, r#""translation": [0, 0],"#), "\"translation\" must be 3 integers");
        refused(&cube_with(r#""material": "steel","#, r#""material": "brass","#), "which is not declared");
        refused(&cube_with(r#""material": "steel","#, r#""material": null,"#), "a node with a shape names a material");
        refused(&cube_with(r#""children": [],"#, r#""children": {},"#), "\"children\" must be an array");
        refused(&cube_with(r#""max": [256, 256, 256] } } ]"#, r#""max": [-256, 256, 256] } } ]"#), "must be below \"max\"");
        refused(&cube_with(r#""shape": "box","#, r#""shape": "box", "radius": 2,"#), "unknown key \"radius\"");
        refused(&cube_with(r#""shape": "box","#, r#""shape": "wedge","#), "unknown shape \"wedge\"");
        // an empty scene and a scene with no geometry are each refused
        refused(&cube_with(r#""nodes": [ {"#, r#""nodes": [], "spare": [ {"#), "unknown key");
        let no_shape =
            cube_with(r#""shape": { "shape": "box", "min": [-256, -256, -256], "max": [256, 256, 256] }"#, r#""shape": null"#)
                .replacen(r#""material": "steel","#, r#""material": null,"#, 1);
        refused(&no_shape, "at least one shape");
    }

    #[test]
    fn refuses_a_duplicate_material_name_and_an_over_deep_tree() {
        let two = cube_with(
            r#""materials": [ { "name": "steel", "base_color": [200, 200, 210, 256], "metallic": 230,
                       "roughness": 64, "double_sided": false } ],"#,
            r#""materials": [ { "name": "steel", "base_color": [1,1,1,1], "metallic": 0, "roughness": 0, "double_sided": false },
                              { "name": "steel", "base_color": [2,2,2,2], "metallic": 0, "roughness": 0, "double_sided": false } ],"#,
        );
        refused(&two, "is declared twice");

        // MAX_DEPTH nested nodes are admitted; one more is refused.
        let leaf = r#"{"name":"n","translation":[0,0,0],"rotation":[0,0,0,2],"scale":[1,1,1],"material":"m","children":[],"shape":{"shape":"box","min":[0,0,0],"max":[1,1,1]}}"#;
        let nest = |depth: usize| {
            let mut node = leaf.to_string();
            for _ in 1..depth {
                node = format!(
                    r#"{{"name":"p","translation":[0,0,0],"rotation":[0,0,0,2],"scale":[1,1,1],"material":null,"shape":null,"children":[{node}]}}"#
                );
            }
            format!(
                r#"{{"v":1,"frac_bits":0,"materials":[{{"name":"m","base_color":[1,1,1,1],"metallic":0,"roughness":0,"double_sided":false}}],"nodes":[{node}]}}"#
            )
        };
        assert!(SceneGrammar.canonicalize(nest(MAX_DEPTH).as_bytes()).is_ok());
        refused(&nest(MAX_DEPTH + 1), &format!("nesting is deeper than {MAX_DEPTH}"));
    }

    #[test]
    fn refuses_sphere_and_cylinder_by_name_with_the_reason() {
        refused(&cube_with(r#""shape": "box","#, r#""shape": "sphere","#), "no exact integer sphere");
        refused(&cube_with(r#""shape": "box","#, r#""shape": "cylinder","#), "cos(2π/n)");
    }

    #[test]
    fn refuses_a_prism_base_that_is_wound_or_shaped_wrong() {
        let prism = |base: &str| {
            format!(
                r#"{{"v":1,"frac_bits":0,"materials":[{{"name":"m","base_color":[1,1,1,1],"metallic":0,"roughness":0,"double_sided":false}}],
                    "nodes":[{{"name":"p","translation":[0,0,0],"rotation":[0,0,0,2],"scale":[1,1,1],"material":"m","children":[],
                    "shape":{{"shape":"prism","base":{base},"y_min":0,"y_max":4}}}}]}}"#
            )
        };
        // the square of the module doc: (0,0) (0,1) (1,1) (1,0) has a negative doubled area
        assert!(SceneGrammar.canonicalize(prism("[[0,0],[0,4],[4,4],[4,0]]").as_bytes()).is_ok());
        refused(&prism("[[0,0],[4,0],[4,4],[0,4]]"), "Reverse the point order");
        refused(&prism("[[0,0],[0,4],[0,8],[4,4]]"), "strictly convex");
        refused(&prism("[[0,0],[0,4]]"), "holds 3..=256 points");
    }

    #[test]
    fn the_transformer_refuses_input_that_is_not_canonical() {
        for bad in [CUBE.as_bytes(), b"{", br#"{"v":2}"#, b""] {
            match SceneGlbTransformer.run(bad) {
                Err(DeriveError::Transformer(msg)) => assert!(msg.contains("not canonical scene/v1"), "{msg}"),
                other => panic!("expected a transformer refusal, got {other:?}"),
            }
        }
        let mut padded = canonical(CUBE);
        padded.push(b'\n');
        assert!(matches!(SceneGlbTransformer.run(&padded), Err(DeriveError::Transformer(_))));
        assert!(check_artifact_size(ARTIFACT_MAX_BYTES).is_ok());
        assert!(matches!(check_artifact_size(ARTIFACT_MAX_BYTES + 1), Err(DeriveError::Transformer(_))));
    }

    /// ADR-0078 X4: a parse failure produces no object and no partial artifact.
    #[test]
    fn a_grammar_refusal_yields_no_object_and_no_artifact() {
        let answer = cube_with(r#""frac_bits": 8,"#, r#""frac_bits": 7,"#);
        let out = derive_with(&SceneGrammar, &SceneGlbTransformer, &binding(), answer.as_bytes());
        assert!(matches!(out, Err(DeriveError::Grammar(_))), "{out:?}");
        // and the same through the registry route the gateway uses
        assert!(matches!(derive_named(TRANSFORMER_NAME, &binding(), answer.as_bytes()), Err(DeriveError::Grammar(_))));
    }

    // ----- (3) the exact arithmetic ----------------------------------------------------------

    /// The module doc's claim, enumerated: `x²+y²+z²+w² == 4` has exactly 24 integer solutions,
    /// every one gives a signed-permutation matrix with determinant +1, and they name exactly
    /// twelve distinct rotations — the tetrahedral group.
    #[test]
    fn the_admissible_quaternions_are_the_twelve_tetrahedral_rotations() {
        let mut solutions = Vec::new();
        let mut matrices = BTreeSet::new();
        for x in -2..=2i64 {
            for y in -2..=2i64 {
                for z in -2..=2i64 {
                    for w in -2..=2i64 {
                        if x * x + y * y + z * z + w * w != ROTATION_NORM {
                            continue;
                        }
                        solutions.push([x, y, z, w]);
                        let r = rotation_matrix([x, y, z, w]).unwrap();
                        // a signed permutation: one non-zero of magnitude 1 per row and column
                        for i in 0..3 {
                            assert_eq!(r[i].iter().filter(|v| **v != 0).count(), 1, "row {i} of {r:?}");
                            assert_eq!((0..3).filter(|j| r[*j][i] != 0).count(), 1, "column {i} of {r:?}");
                        }
                        let det = r[0][0] as i32 * (r[1][1] as i32 * r[2][2] as i32 - r[1][2] as i32 * r[2][1] as i32)
                            - r[0][1] as i32 * (r[1][0] as i32 * r[2][2] as i32 - r[1][2] as i32 * r[2][0] as i32)
                            + r[0][2] as i32 * (r[1][0] as i32 * r[2][1] as i32 - r[1][1] as i32 * r[2][0] as i32);
                        assert_eq!(det, 1, "rotation {:?} is not a rotation", [x, y, z, w]);
                        matrices.insert(r);
                    }
                }
            }
        }
        assert_eq!(solutions.len(), 24, "the eight ±2 axis quaternions and the sixteen all-ones");
        assert_eq!(matrices.len(), 12, "q and -q are one rotation: twelve rotations");
        assert_eq!(rotation_matrix([0, 0, 0, 2]).unwrap(), [[1, 0, 0], [0, 1, 0], [0, 0, 1]], "the identity");
        assert_eq!(rotation_matrix([2, 0, 0, 0]).unwrap(), [[1, 0, 0], [0, -1, 0], [0, 0, -1]], "a half turn about X");
        assert_eq!(rotation_matrix([1, 1, 1, 1]).unwrap(), [[0, 0, 1], [1, 0, 0], [0, 1, 0]], "a third turn about (1,1,1)");
    }

    /// The descent in the module doc, checked over the reachable denominators: a dyadic unit
    /// quaternion at `2^k` is always `2^(k-1)` times one at `2^1`, so denominator 2 is the
    /// normal form and not a restriction.
    #[test]
    fn no_larger_dyadic_denominator_admits_a_new_rotation() {
        for k in 2..=4u32 {
            let norm = 1i64 << (2 * k);
            let bound = 1i64 << k;
            let mut found = 0;
            for x in -bound..=bound {
                for y in -bound..=bound {
                    for z in -bound..=bound {
                        for w in -bound..=bound {
                            if x * x + y * y + z * z + w * w != norm {
                                continue;
                            }
                            found += 1;
                            assert!(
                                x % 2 == 0 && y % 2 == 0 && z % 2 == 0 && w % 2 == 0,
                                "({x},{y},{z},{w}) at 4^{k} is not all even: the descent fails"
                            );
                        }
                    }
                }
            }
            assert_eq!(found, 24, "4^{k} has the same 24 solutions, all of them doubled");
        }
    }

    /// The other half of the descent: `a²+b²+c² = 4^k` has only the axis solutions, so the six
    /// axis directions are the only exact dyadic unit normals — which is why a mesh with a
    /// non-axis-aligned face omits `NORMAL` instead of writing a rounded one.
    #[test]
    fn the_only_exact_unit_normals_are_the_six_axes() {
        for k in 1..=5u32 {
            let norm = 1i64 << (2 * k);
            let bound = 1i64 << k;
            let mut solutions = 0;
            for a in -bound..=bound {
                for b in -bound..=bound {
                    for c in -bound..=bound {
                        if a * a + b * b + c * c != norm {
                            continue;
                        }
                        solutions += 1;
                        let nonzero = [a, b, c].iter().filter(|v| **v != 0).count();
                        assert_eq!(nonzero, 1, "({a},{b},{c}) at 4^{k} is a non-axis dyadic unit normal");
                    }
                }
            }
            assert_eq!(solutions, 6, "4^{k}: the six axis directions and nothing else");
        }
        assert_eq!(axis_normal([0, 5, 0]), Some([0, 1, 0]));
        assert_eq!(axis_normal([-3, 0, 0]), Some([-1, 0, 0]));
        assert_eq!(axis_normal([1, 1, 0]), None);
        assert_eq!(axis_normal([0, 0, 0]), None);
    }

    #[test]
    fn dyadic_normalizes_and_composes_exactly() {
        assert_eq!(Dyadic::new(8, 3), Dyadic::ONE);
        assert_eq!(Dyadic::new(0, 9), Dyadic::ZERO);
        assert_eq!(Dyadic::fixed(384, 8), Dyadic { m: 3, f: 1 }); // 384/256 = 1.5
        let half = Dyadic::new(1, 1);
        assert_eq!(half.mul(half, "t").unwrap(), Dyadic { m: 1, f: 2 });
        assert_eq!(half.add(half, "t").unwrap(), Dyadic::ONE);
        assert_eq!(Dyadic::new(-3, 2).add(Dyadic::new(3, 2), "t").unwrap(), Dyadic::ZERO);
        assert!(Dyadic::fixed(COORD_LIMIT, 8).abs_within(COORD_LIMIT, 8, "t").unwrap());
        assert!(!Dyadic::fixed(COORD_LIMIT + 1, 8).abs_within(COORD_LIMIT, 8, "t").unwrap());
        // a composition that runs out of exact room is a refusal, never a rounding
        let odd = Dyadic::new(65_543, 16);
        let mut acc = odd;
        let mut refused = false;
        for _ in 0..12 {
            match acc.mul(odd, "t") {
                Ok(next) => acc = next,
                Err(DeriveError::Inexact(msg)) => {
                    assert!(msg.contains("exact"), "{msg}");
                    refused = true;
                    break;
                }
                other => panic!("{other:?}"),
            }
        }
        assert!(refused, "the exact-composition bound is reachable and refuses by name");
    }

    #[test]
    fn the_hierarchy_composes_in_fixed_point() {
        let dsl = SceneDsl::parse(HIERARCHY.as_bytes()).unwrap();
        let built = build_scene(&dsl).unwrap();
        // root: translate (2,0,0), rotate (1,1,1,1) → the cyclic permutation, scale 2
        let root = Transform::from_node(&dsl.nodes[0], dsl.frac_bits).unwrap();
        assert_eq!(root.translation, [Dyadic::new(2, 0), Dyadic::ZERO, Dyadic::ZERO]);
        // R·S with R = [[0,0,1],[1,0,0],[0,1,0]] and S = 2I
        assert_eq!(root.linear[0], [Dyadic::ZERO, Dyadic::ZERO, Dyadic::new(2, 0)]);
        assert_eq!(root.linear[1], [Dyadic::new(2, 0), Dyadic::ZERO, Dyadic::ZERO]);
        assert_eq!(root.linear[2], [Dyadic::ZERO, Dyadic::new(2, 0), Dyadic::ZERO]);
        // child: translate (0,1,0) under the root. The root's rotation sends +Y to +Z, and the
        // root scales by 2, so the child's origin lands at (2,0,0) + 2·(0,0,1) = (2,0,2):
        let child = root.compose(&Transform::from_node(&dsl.nodes[0].children[0], dsl.frac_bits).unwrap()).unwrap();
        assert_eq!(child.translation, [Dyadic::new(2, 0), Dyadic::ZERO, Dyadic::new(2, 0)]);
        // the composed scale is 2 · 0.5 = 1 on every axis, so the child's ±0.25 box keeps its
        // size and only moves; the rotation sends its x to world y, y to world z, z to world x
        let (lo, hi) = &built.world_bounds[0];
        assert_eq!(*lo, [Dyadic::new(7, 2), Dyadic::new(-1, 2), Dyadic::new(7, 2)]);
        assert_eq!(*hi, [Dyadic::new(9, 2), Dyadic::new(1, 2), Dyadic::new(9, 2)]);
    }

    #[test]
    fn a_scene_the_composition_places_outside_the_world_bound_is_refused() {
        // The world bound is COORD_LIMIT/2^frac_bits = 2^30/2^8 = 2^22 world units. A scale of
        // SCALE_MAX = 2^30 units is 2^22, so a box whose local half-extent is 2 units lands a
        // corner at 2^23 — one doubling outside. The half-extent of 1 sits exactly ON the
        // bound and is admitted, which is asserted below so the boundary is not a guess.
        let scaled = |half: i64| {
            cube_with(r#""scale": [256, 256, 256],"#, &format!(r#""scale": [{SCALE_MAX}, 256, 256],"#)).replacen(
                r#""min": [-256, -256, -256], "max": [256, 256, 256]"#,
                &format!(r#""min": [{}, -256, -256], "max": [{}, 256, 256]"#, -256 * half, 256 * half),
                1,
            )
        };
        let at_the_edge = SceneGrammar.canonicalize(scaled(1).as_bytes()).unwrap();
        assert!(SceneGlbTransformer.run(&at_the_edge).is_ok(), "a corner exactly on the world bound is inside it");
        let canonical = SceneGrammar.canonicalize(scaled(2).as_bytes()).expect("the grammar admits it; the composition does not");
        match SceneGlbTransformer.run(&canonical) {
            Err(DeriveError::Inexact(msg)) => assert!(msg.contains("world bound"), "{msg}"),
            other => panic!("expected a world-bound refusal, got {other:?}"),
        }
    }

    /// The place the discipline bites (ADR-0078 Decision 3): a coordinate binary32 cannot hold
    /// is a legal DSL and an `Inexact` refusal, never a rounded vertex.
    #[test]
    fn a_coordinate_binary32_cannot_hold_is_refused_not_rounded() {
        let inexact = cube_with(r#""max": [256, 256, 256] } } ]"#, r#""max": [16777217, 256, 256] } } ]"#);
        let canonical = SceneGrammar.canonicalize(inexact.as_bytes()).expect("the grammar admits it; the writer does not");
        match SceneGlbTransformer.run(&canonical) {
            Err(DeriveError::Inexact(msg)) => assert!(msg.contains("significant bits"), "{msg}"),
            other => panic!("expected an Inexact refusal, got {other:?}"),
        }
        // and 2^24 itself, one significant bit, is fine
        let exact = cube_with(r#""max": [256, 256, 256] } } ]"#, r#""max": [16777216, 256, 256] } } ]"#);
        assert!(SceneGlbTransformer.run(&SceneGrammar.canonicalize(exact.as_bytes()).unwrap()).is_ok());
    }

    #[test]
    fn the_decimal_spelling_is_exact_and_has_one_form_per_value() {
        let spell = |m: i64, f: u32| {
            let mut out = Vec::new();
            write_number(m, f, &mut out).unwrap();
            String::from_utf8(out).unwrap()
        };
        assert_eq!(spell(0, 8), "0");
        assert_eq!(spell(256, 8), "1");
        assert_eq!(spell(-256, 8), "-1");
        assert_eq!(spell(128, 8), "0.5");
        assert_eq!(spell(1, 8), "0.00390625");
        assert_eq!(spell(-1, 8), "-0.00390625");
        assert_eq!(spell(384, 8), "1.5");
        assert_eq!(spell(1, 16), "0.0000152587890625");
        assert_eq!(spell(7, 0), "7");
        // every spelling parses back to exactly the value it names, and the binary32 of that
        // value is the one the buffer carries — checked without ever computing in a float
        for (m, f) in [(1i64, 8u32), (-3, 4), (255, 8), (65_535, 16), (1 << 23, 0)] {
            let text = spell(m, f);
            let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert!(parsed.is_number(), "{text} is not a JSON number");
            let (int, frac) = text.split_once('.').unwrap_or((text.as_str(), ""));
            let scaled: i128 = format!("{int}{frac}").parse().unwrap();
            let denominator = 10i128.pow(frac.len() as u32);
            assert_eq!(scaled * (1i128 << f), m as i128 * denominator, "{text} is not {m}/2^{f}");
        }
    }

    // ----- (4) a structural walk of the GLB --------------------------------------------------

    /// A structural validator, which is what an in-tree test can honestly claim: there is no
    /// glTF library in this workspace, so "the GLB loads" is checked as "the container, the
    /// chunk framing, the JSON, and every byte range the JSON names are well formed and inside
    /// their parent". It is not a renderer, and it does not claim to be one; what it does claim
    /// it checks exactly, including the padding bytes the writer pins.
    struct Walk {
        json: serde_json::Value,
        bin: Vec<u8>,
    }

    fn walk(bytes: &[u8]) -> Walk {
        let u32_at = |offset: usize| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        assert!(bytes.len() >= 20, "a GLB is at least a header and two chunk headers");
        assert_eq!(u32_at(0), GLB_MAGIC, "magic");
        assert_eq!(u32_at(4), GLB_VERSION, "version");
        assert_eq!(u32_at(8) as usize, bytes.len(), "the header's total length is the file's");
        assert!(bytes.len().is_multiple_of(4), "a GLB's length is a multiple of four");

        let json_len = u32_at(12) as usize;
        assert_eq!(u32_at(16), GLB_CHUNK_JSON, "the first chunk is JSON");
        assert!(json_len.is_multiple_of(4), "the JSON chunk's length includes its padding");
        let json_bytes = &bytes[20..20 + json_len];
        let trailing = json_bytes.iter().rev().take_while(|b| **b == GLB_JSON_PAD).count();
        assert!(trailing < 4, "the JSON chunk is padded with at most three spaces");
        assert_eq!(json_bytes[json_len - trailing - 1], b'}', "the JSON ends before its padding");
        let json: serde_json::Value = serde_json::from_slice(&json_bytes[..json_len - trailing]).expect("the JSON chunk parses");

        let bin_start = 20 + json_len;
        let bin_len = u32_at(bin_start) as usize;
        assert_eq!(u32_at(bin_start + 4), GLB_CHUNK_BIN, "the second chunk is BIN");
        assert!(bin_len.is_multiple_of(4), "the BIN chunk's length includes its padding");
        assert_eq!(bin_start + 8 + bin_len, bytes.len(), "the two chunks are the whole file");
        let bin = bytes[bin_start + 8..bin_start + 8 + bin_len].to_vec();

        // every bufferView inside the buffer, every accessor inside its bufferView, and every
        // padding byte between views is the writer's fill byte
        let buffer_length = json["buffers"][0]["byteLength"].as_u64().unwrap() as usize;
        assert!(buffer_length <= bin.len() && bin.len() - buffer_length < 4, "the BIN chunk is the buffer plus its padding");
        for pad in &bin[buffer_length..] {
            assert_eq!(*pad, GLB_BIN_PAD, "the BIN chunk is zero-padded");
        }
        let views = json["bufferViews"].as_array().unwrap();
        let accessors = json["accessors"].as_array().unwrap();
        assert_eq!(views.len(), accessors.len(), "accessor k reads bufferView k");
        let mut covered = vec![false; buffer_length];
        for (k, view) in views.iter().enumerate() {
            let offset = view["byteOffset"].as_u64().unwrap() as usize;
            let length = view["byteLength"].as_u64().unwrap() as usize;
            assert_eq!(view["buffer"].as_u64().unwrap(), 0);
            assert!(offset.is_multiple_of(4), "bufferView {k} starts at a multiple of four");
            assert!(offset + length <= buffer_length, "bufferView {k} runs past the buffer");
            for slot in &mut covered[offset..offset + length] {
                assert!(!*slot, "bufferView {k} overlaps another");
                *slot = true;
            }
            let a = &accessors[k];
            assert_eq!(a["bufferView"].as_u64().unwrap() as usize, k, "accessor {k} reads its own view");
            assert_eq!(a["byteOffset"].as_u64().unwrap(), 0);
            let component = match a["componentType"].as_u64().unwrap() {
                5123 => 2,
                5126 => 4,
                other => panic!("componentType {other} is not one this writer emits"),
            };
            let components = match a["type"].as_str().unwrap() {
                "SCALAR" => 1,
                "VEC3" => 3,
                other => panic!("type {other} is not one this writer emits"),
            };
            let count = a["count"].as_u64().unwrap() as usize;
            assert_eq!(count * components * component, length, "accessor {k} does not fill its view");
            assert_eq!(a["min"].as_array().unwrap().len(), components, "accessor {k} min");
            assert_eq!(a["max"].as_array().unwrap().len(), components, "accessor {k} max");
        }
        for (offset, taken) in covered.iter().enumerate() {
            if !taken {
                assert_eq!(bin[offset], GLB_BIN_PAD, "the alignment gap at {offset} is not the writer's fill byte");
            }
        }
        // every index is inside its primitive's vertex count, and every node index resolves
        for mesh in json["meshes"].as_array().unwrap() {
            for primitive in mesh["primitives"].as_array().unwrap() {
                assert_eq!(primitive["mode"].as_u64().unwrap(), MODE_TRIANGLES);
                let position = primitive["attributes"]["POSITION"].as_u64().unwrap() as usize;
                let vertices = accessors[position]["count"].as_u64().unwrap();
                let indices = primitive["indices"].as_u64().unwrap() as usize;
                assert_eq!(accessors[indices]["count"].as_u64().unwrap() % 3, 0, "indices are whole triangles");
                assert!(accessors[indices]["max"][0].as_u64().unwrap() < vertices, "an index runs past the vertices");
                if let Some(normal) = primitive["attributes"].get("NORMAL") {
                    assert_eq!(accessors[normal.as_u64().unwrap() as usize]["count"].as_u64().unwrap(), vertices);
                }
                assert!((primitive["material"].as_u64().unwrap() as usize) < json["materials"].as_array().unwrap().len());
            }
        }
        let node_count = json["nodes"].as_array().unwrap().len();
        let mut seen_as_child = vec![false; node_count];
        for node in json["nodes"].as_array().unwrap() {
            for child in node["children"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
                let c = child.as_u64().unwrap() as usize;
                assert!(c < node_count);
                assert!(!seen_as_child[c], "node {c} has two parents");
                seen_as_child[c] = true;
            }
        }
        for root in json["scenes"][0]["nodes"].as_array().unwrap() {
            assert!(!seen_as_child[root.as_u64().unwrap() as usize], "a root is also a child");
        }
        assert_eq!(json["scene"].as_u64().unwrap(), 0);
        assert_eq!(json["asset"]["version"].as_str().unwrap(), "2.0");
        assert_eq!(json["asset"]["generator"].as_str().unwrap(), WRITER_NAME);
        Walk { json, bin }
    }

    /// Read a `VEC3` binary32 accessor back as the integer mantissas that produced it, without
    /// computing in a float: each 4-byte group is compared against `f32_le_exact` of a candidate.
    fn read_vec3(w: &Walk, accessor: usize, frac_bits: u32, search: &dyn Fn(usize, usize) -> i64) -> Vec<[i64; 3]> {
        let a = &w.json["accessors"][accessor];
        let view = &w.json["bufferViews"][a["bufferView"].as_u64().unwrap() as usize];
        let offset = view["byteOffset"].as_u64().unwrap() as usize;
        let count = a["count"].as_u64().unwrap() as usize;
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let mut point = [0i64; 3];
            for (axis, slot) in point.iter_mut().enumerate() {
                let at = offset + i * 12 + axis * 4;
                let expected = search(i, axis);
                assert_eq!(&w.bin[at..at + 4], &f32_le_exact(expected, frac_bits).unwrap(), "vertex {i} axis {axis}");
                *slot = expected;
            }
            out.push(point);
        }
        out
    }

    #[test]
    fn the_glb_is_structurally_well_formed_for_every_primitive() {
        for answer in [CUBE, HIERARCHY, &prism_answer(8), &triangle_answer()] {
            let w = walk(&glb(answer));
            assert!(!w.json["meshes"].as_array().unwrap().is_empty());
        }
    }

    /// A prism of `n` points around a square-ish ring — an exact integer polygon, wound so the
    /// top cap faces +Y.
    fn prism_answer(n: usize) -> String {
        // a convex polygon on an integer "circle": the points of a regular 4k-gon are not
        // integers, so the corpus uses an explicitly listed convex integer polygon instead —
        // which is precisely the substitution the module doc names for `cylinder`.
        let ring: Vec<[i64; 2]> = match n {
            4 => vec![[0, 0], [0, 8], [8, 8], [8, 0]],
            8 => vec![[0, 3], [0, 5], [3, 8], [5, 8], [8, 5], [8, 3], [5, 0], [3, 0]],
            _ => panic!("the test builds 4- and 8-gons"),
        };
        let base = ring.iter().map(|p| format!("[{},{}]", p[0], p[1])).collect::<Vec<_>>().join(",");
        format!(
            r#"{{"v":1,"frac_bits":4,"materials":[{{"name":"m","base_color":[128,128,128,256],"metallic":0,"roughness":200,"double_sided":false}}],
                "nodes":[{{"name":"tower","translation":[0,0,0],"rotation":[0,0,0,2],"scale":[16,16,16],"material":"m","children":[],
                "shape":{{"shape":"prism","base":[{base}],"y_min":0,"y_max":32}}}}]}}"#
        )
    }

    fn triangle_answer() -> String {
        r#"{"v":1,"frac_bits":8,"materials":[{"name":"m","base_color":[256,256,0,256],"metallic":0,"roughness":128,"double_sided":true}],
            "nodes":[{"name":"tri","translation":[0,0,0],"rotation":[0,0,0,2],"scale":[256,256,256],"material":"m","children":[],
            "shape":{"shape":"triangle","a":[0,0,0],"b":[256,0,0],"c":[0,256,256]}}]}"#
            .to_string()
    }

    #[test]
    fn a_box_winds_counter_clockwise_and_its_normals_point_outward() {
        let dsl = SceneDsl::parse(CUBE.as_bytes()).unwrap();
        let mesh = build_mesh(dsl.nodes[0].shape.as_ref().unwrap()).unwrap();
        assert_eq!(mesh.positions.len(), 24, "six faces of four vertices");
        assert_eq!(mesh.indices.len(), 36, "twelve triangles");
        let normals = mesh.normals.as_ref().expect("a box is axis-aligned, so it carries normals");
        // every triangle's own cross product agrees with the normal the writer emitted, and the
        // face's normal points away from the box's centre — which is what "outward" means here
        for triangle in mesh.indices.chunks(3) {
            let (a, b, c) =
                (mesh.positions[triangle[0] as usize], mesh.positions[triangle[1] as usize], mesh.positions[triangle[2] as usize]);
            let n = axis_normal(cross(sub(b, a), sub(c, a))).expect("a box face is axis-aligned");
            assert_eq!(n, normals[triangle[0] as usize], "the emitted normal is the winding's");
            // the centre of the cube is the origin: the face's own coordinate on the normal's
            // axis has the normal's sign, so the winding faces out
            let axis = n.iter().position(|v| *v != 0).unwrap();
            assert_eq!(a[axis].signum() as i8, n[axis], "face {a:?} winds inward");
        }
        assert_eq!(
            normals.chunks(4).map(|f| f[0]).collect::<Vec<_>>(),
            vec![[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]],
            "the pinned face order +X, -X, +Y, -Y, +Z, -Z"
        );
    }

    #[test]
    fn a_prism_caps_face_up_and_down_and_its_sides_face_out() {
        let dsl = SceneDsl::parse(prism_answer(4).as_bytes()).unwrap();
        let mesh = build_mesh(dsl.nodes[0].shape.as_ref().unwrap()).unwrap();
        assert_eq!(mesh.positions.len(), 4 + 4 + 4 * 4, "two caps and four side quads");
        let normals = mesh.normals.as_ref().expect("a rectilinear prism is axis-aligned");
        assert_eq!(normals[0], [0, -1, 0], "the bottom cap faces -Y");
        assert_eq!(normals[4], [0, 1, 0], "the top cap faces +Y");
        // the square runs 0..8 on both axes, so each side's normal points away from (4, 4)
        for quad in 0..4 {
            let base = 8 + quad * 4;
            let n = normals[base];
            let p = mesh.positions[base];
            let axis = n.iter().position(|v| *v != 0).unwrap();
            let centre = 4i64;
            assert_eq!((p[if axis == 0 { 0 } else { 2 }] - centre).signum() as i8, n[axis], "side {quad} faces inward");
        }
        // an eight-gon still has axis-aligned caps but diagonal sides, so it omits NORMAL
        let eight = SceneDsl::parse(prism_answer(8).as_bytes()).unwrap();
        let mesh8 = build_mesh(eight.nodes[0].shape.as_ref().unwrap()).unwrap();
        assert!(mesh8.normals.is_none(), "a diagonal face has no exact unit normal, so the mesh omits NORMAL");
        let w = walk(&glb(&prism_answer(8)));
        assert!(w.json["meshes"][0]["primitives"][0]["attributes"].get("NORMAL").is_none());
        assert_eq!(w.json["accessors"].as_array().unwrap().len(), 2, "POSITION and the indices, no NORMAL");
    }

    #[test]
    fn an_odd_triangle_count_leaves_the_pinned_alignment_gap() {
        // a triangle is one face: three indices, six bytes, so the next view needs two bytes of
        // padding — the only exercise the alignment rule gets, and the corpus takes it too
        let two_triangles = r#"{"v":1,"frac_bits":8,"materials":[{"name":"m","base_color":[256,0,0,256],"metallic":0,"roughness":0,"double_sided":false}],
                "nodes":[{"name":"a","translation":[0,0,0],"rotation":[0,0,0,2],"scale":[256,256,256],"material":"m","children":[],
                           "shape":{"shape":"triangle","a":[0,0,0],"b":[256,0,0],"c":[0,0,256]}},
                         {"name":"b","translation":[0,0,0],"rotation":[0,0,0,2],"scale":[256,256,256],"material":"m","children":[],
                           "shape":{"shape":"triangle","a":[0,0,0],"b":[256,0,0],"c":[0,256,256]}}]}"#;
        let bytes = glb(two_triangles);
        let w = walk(&bytes);
        let views = w.json["bufferViews"].as_array().unwrap();
        let first_indices = views.iter().find(|v| v["byteLength"].as_u64() == Some(6)).expect("a three-index view of six bytes");
        let end = first_indices["byteOffset"].as_u64().unwrap() as usize + 6;
        assert!(!end.is_multiple_of(4), "the sample was meant to leave an unaligned end");
        assert_eq!(&w.bin[end..end + 2], &[GLB_BIN_PAD, GLB_BIN_PAD], "the gap is the pinned fill byte");
        // the first triangle lies in the XZ plane, so it keeps its normal; the second is
        // diagonal, so its mesh omits NORMAL — two meshes, two different accessor shapes
        assert!(w.json["meshes"][0]["primitives"][0]["attributes"].get("NORMAL").is_some());
        assert!(w.json["meshes"][1]["primitives"][0]["attributes"].get("NORMAL").is_none());
    }

    #[test]
    fn the_positions_in_the_buffer_are_the_fixed_point_vertices() {
        let dsl = SceneDsl::parse(CUBE.as_bytes()).unwrap();
        let mesh = build_mesh(dsl.nodes[0].shape.as_ref().unwrap()).unwrap();
        let w = walk(&glb(CUBE));
        let read = read_vec3(&w, 0, dsl.frac_bits, &|i, axis| mesh.positions[i][axis]);
        assert_eq!(read, mesh.positions, "the buffer carries exactly the builder's integers");
        let (min, max) = mesh.bounds();
        let spell = |m: i64| {
            let mut out = Vec::new();
            write_number(m, dsl.frac_bits, &mut out).unwrap();
            String::from_utf8(out).unwrap()
        };
        for axis in 0..3 {
            assert_eq!(w.json["accessors"][0]["min"][axis].to_string(), spell(min[axis]));
            assert_eq!(w.json["accessors"][0]["max"][axis].to_string(), spell(max[axis]));
        }
    }

    #[test]
    fn the_node_tree_is_depth_first_pre_order_with_the_dsl_s_transforms() {
        let w = walk(&glb(HIERARCHY));
        let nodes = w.json["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0]["name"].as_str().unwrap(), "root");
        assert_eq!(nodes[1]["name"].as_str().unwrap(), "child");
        assert_eq!(nodes[2]["name"].as_str().unwrap(), "floor");
        assert_eq!(nodes[0]["children"], serde_json::json!([1, 2]));
        assert!(nodes[0].get("mesh").is_none(), "a pure transform node carries no mesh");
        assert_eq!(nodes[1]["mesh"].as_u64().unwrap(), 0);
        assert_eq!(nodes[2]["mesh"].as_u64().unwrap(), 1);
        assert_eq!(w.json["scenes"][0]["nodes"], serde_json::json!([0]));
        // the TRS is the DSL's, spelled exactly: rotation over two, the rest over 2^frac_bits
        assert_eq!(nodes[0]["rotation"].to_string(), "[0.5,0.5,0.5,0.5]");
        assert_eq!(nodes[0]["translation"].to_string(), "[2.0,0,0]".replace(".0", ""));
        assert_eq!(nodes[0]["scale"].to_string(), "[2,2,2]");
        assert_eq!(nodes[1]["translation"].to_string(), "[0,1,0]");
        assert_eq!(nodes[1]["scale"].to_string(), "[0.5,0.5,0.5]");
        assert_eq!(nodes[2]["rotation"].to_string(), "[1,0,0,0]");
        // the materials are the DSL's, in the DSL's order, over a denominator of 256
        let materials = w.json["materials"].as_array().unwrap();
        assert_eq!(materials[0]["name"].as_str().unwrap(), "a");
        assert_eq!(materials[0]["pbrMetallicRoughness"]["baseColorFactor"].to_string(), "[1,0,0,1]");
        assert!(!materials[0]["doubleSided"].as_bool().unwrap());
        assert_eq!(materials[1]["pbrMetallicRoughness"]["baseColorFactor"].to_string(), "[0,0,1,0.5]");
        assert_eq!(materials[1]["pbrMetallicRoughness"]["metallicFactor"].to_string(), "0.5");
        assert!(materials[1]["doubleSided"].as_bool().unwrap());
    }

    // ----- (5) determinism (X3, pinned here; the second architecture is the drill's) ---------

    #[test]
    fn the_same_dsl_twice_is_the_same_bytes_and_spelling_does_not_reach_them() {
        for answer in [CUBE, HIERARCHY, &prism_answer(8)] {
            assert_eq!(glb(answer), glb(answer));
        }
        let reordered = r#"{"nodes":[{"shape":{"max":[256,256,256],"min":[-256,-256,-256],"shape":"box"},"children":[],
            "material":"steel","scale":[256,256,256],"rotation":[0,0,0,2],"translation":[0,0,0],"name":"cube"}],
            "materials":[{"roughness":64,"metallic":230,"double_sided":false,"base_color":[200,200,210,256],"name":"steel"}],
            "frac_bits":8,"v":1}"#;
        assert_eq!(canonical(reordered), canonical(CUBE), "key order and whitespace do not survive canonicalization");
        assert_eq!(glb(reordered), glb(CUBE));
    }

    /// ADR-0078 X9: extra inputs of a transformation are named by hash inside the DSL, so that
    /// `dsl_hash` fixes the whole derivation. `scene/v1` satisfies it by having no extra inputs
    /// at all, and this asserts that rather than saying it: the admitted key set is spelled out,
    /// every corpus sample uses only those keys, and a key that could name bytes outside the
    /// document is refused wherever it is inserted.
    #[test]
    fn the_grammar_admits_no_reference_to_bytes_outside_the_document() {
        const ADMITTED: &[&str] = &[
            // the document
            "v",
            "frac_bits",
            "materials",
            "nodes", //
            // a material
            "name",
            "base_color",
            "metallic",
            "roughness",
            "double_sided", //
            // a node
            "translation",
            "rotation",
            "scale",
            "shape",
            "material",
            "children", //
            // a shape
            "min",
            "max",
            "a",
            "b",
            "c",
            "base",
            "y_min",
            "y_max",
        ];
        fn keys(v: &CanonValue, seen: &mut BTreeSet<String>) {
            match v {
                CanonValue::Obj(o) => {
                    for (k, value) in o {
                        seen.insert(k.clone());
                        keys(value, seen);
                    }
                }
                CanonValue::Arr(items) => items.iter().for_each(|i| keys(i, seen)),
                _ => {}
            }
        }
        let mut used = BTreeSet::new();
        for (name, answer) in corpus_files() {
            if let Ok(tree) = parse_canonical(&answer) {
                let mut here = BTreeSet::new();
                keys(&tree, &mut here);
                for k in &here {
                    assert!(ADMITTED.contains(&k.as_str()), "{name} uses key {k:?}, which is not in the admitted set");
                }
                used.extend(here);
            }
        }
        assert!(used.len() >= 20, "the corpus exercises {} of the {} admitted keys", used.len(), ADMITTED.len());
        // and a key that could name bytes outside the document is refused at every level
        for (site, replacement) in [
            (r#""v": 1,"#, r#""v": 1, "uri": "https://example.invalid/city.bin","#),
            (r#""name": "steel","#, r#""name": "steel", "texture": "aabbcc","#),
            (r#""translation": [0, 0, 0],"#, r#""translation": [0, 0, 0], "mesh_uri": "x","#),
            (r#""shape": "box","#, r#""shape": "box", "image": "aabbcc","#),
        ] {
            refused(&cube_with(site, replacement), "unknown key");
        }
    }

    // ----- (5b) SA-2: the declared bounds, enforced before the build -------------------------

    /// A strictly convex integer polygon of `4m` points, wound so that the top cap faces `+Y`
    /// (the grammar's own winding rule, which is asserted by using it).
    ///
    /// The construction is the classical one: `4m` edge vectors whose directions turn strictly
    /// clockwise once around, summing to zero by antipodal symmetry. `(k, m+1-k)` for `k` in
    /// `1..=m` sweeps the first quadrant with strictly decreasing slope, `(m+1-k, -k)` the
    /// fourth, and the other two are their negatives. Distinct slopes give strict convexity;
    /// the zero sum closes the polygon; the partial sums are the vertices, all integers.
    fn convex_ring(m: i64) -> Vec<[i64; 2]> {
        let mut edges = Vec::new();
        for k in 1..=m {
            edges.push([k, m + 1 - k]);
        }
        for k in 1..=m {
            edges.push([m + 1 - k, -k]);
        }
        for k in 1..=m {
            edges.push([-k, -(m + 1 - k)]);
        }
        for k in 1..=m {
            edges.push([-(m + 1 - k), k]);
        }
        assert_eq!(edges.iter().map(|e| e[0]).sum::<i64>(), 0, "the ring closes on x");
        assert_eq!(edges.iter().map(|e| e[1]).sum::<i64>(), 0, "the ring closes on z");
        let mut points = Vec::with_capacity(edges.len());
        let mut at = [0i64, 0];
        for e in &edges {
            points.push(at);
            at = [at[0] + e[0], at[1] + e[1]];
        }
        points
    }

    fn ring_json(points: &[[i64; 2]]) -> String {
        points.iter().map(|p| format!("[{},{}]", p[0], p[1])).collect::<Vec<_>>().join(",")
    }

    /// A document of `count` identical shape nodes under one material — the shape that makes a
    /// bound reachable, written the way a corpus file is written.
    fn repeated_shape_scene(count: usize, shape: &str) -> String {
        let node = format!(
            r#"{{"children":[],"material":"m","name":"","rotation":[0,0,0,2],"scale":[1,1,1],"shape":{shape},"translation":[0,0,0]}}"#
        );
        let nodes = vec![node; count].join(",");
        format!(
            r#"{{"frac_bits":0,"materials":[{{"base_color":[256,256,256,256],"double_sided":false,"metallic":0,"name":"m","roughness":128}}],"nodes":[{nodes}],"v":1}}"#
        )
    }

    /// The `08-` corpus shape: a 256-point prism, the densest geometry per byte of DSL.
    fn dense_prism_scene(count: usize) -> String {
        let ring = ring_json(&convex_ring(64));
        repeated_shape_scene(count, &format!(r#"{{"base":[{ring}],"shape":"prism","y_max":1,"y_min":0}}"#))
    }

    /// The `09-` corpus shape: a unit box, the most expensive geometry per predicted byte.
    fn many_boxes_scene(count: usize) -> String {
        repeated_shape_scene(count, r#"{"max":[1,1,1],"min":[0,0,0],"shape":"box"}"#)
    }

    #[test]
    fn the_declared_bounds_are_the_ones_enforced() {
        assert_eq!(declared_bounds(), BOUNDS);
        assert_eq!(BOUNDS.max_dsl_bytes, 262_144);
        assert_eq!(BOUNDS.max_steps, 65_536);
        assert_eq!(BOUNDS.max_artifact_bytes, 2_097_152);
        assert_eq!(STEPS_UNIT, "mesh vertices");
        // one spelling: the constants the code reads are the declaration, not a copy of it
        assert_eq!(MAX_DSL_BYTES as u64, BOUNDS.max_dsl_bytes);
        assert_eq!(MAX_TOTAL_VERTICES as u64, BOUNDS.max_steps);
        assert_eq!(ARTIFACT_MAX_BYTES as u64, BOUNDS.max_artifact_bytes);
        // each is reachable: the module doc's density argument, as arithmetic
        let prism_at_the_budget =
            predicted_artifact_bytes(&ScenePlan { nodes: 42, meshes: 42, vertices: 42 * 1536, indices: 42 * 3060 }, 1);
        assert!(prism_at_the_budget < BOUNDS.max_artifact_bytes, "a full prism budget must fit, so max_steps can bite");
        let every_box = predicted_artifact_bytes(
            &ScenePlan { nodes: MAX_NODES, meshes: MAX_NODES, vertices: MAX_NODES * 24, indices: MAX_NODES * 36 },
            1,
        );
        assert!(every_box > BOUNDS.max_artifact_bytes, "a full node budget of boxes must not fit, so max_artifact_bytes can bite");
        assert!((MAX_NODES * 24) as u64 <= BOUNDS.max_steps, "and it must not be max_steps that stops it");
    }

    /// The plan is the builder's arithmetic, not a second opinion about it: for every primitive
    /// the schema admits, `shape_cost` is exactly what `build_mesh` emits.
    #[test]
    fn the_plan_is_the_builders_own_arithmetic() {
        let shapes = [
            Shape::Box { min: [0, 0, 0], max: [1, 1, 1] },
            Shape::Plane { min: [0, 0], max: [1, 1] },
            Shape::Triangle { a: [0, 0, 0], b: [1, 0, 0], c: [0, 0, 1] },
            Shape::Prism { base: vec![[0, 0], [0, 4], [4, 4], [4, 0]], y_min: 0, y_max: 1 },
            Shape::Prism { base: convex_ring(2), y_min: 0, y_max: 1 },
            Shape::Prism { base: convex_ring(64), y_min: 0, y_max: 1 },
        ];
        for shape in &shapes {
            let mesh = build_mesh(shape).unwrap();
            assert_eq!(shape_cost(shape), (mesh.positions.len(), mesh.indices.len()), "{shape:?}");
        }
        // and over a whole document, the plan is the built scene
        for answer in [CUBE, HIERARCHY, &prism_answer(8), &triangle_answer()] {
            let dsl = SceneDsl::parse(answer.as_bytes()).unwrap();
            let plan = plan_scene(&dsl);
            let built = build_scene(&dsl).unwrap();
            assert_eq!(plan.nodes, built.nodes.len());
            assert_eq!(plan.meshes, built.meshes.len());
            assert_eq!(plan.vertices, built.meshes.iter().map(|m| m.mesh.positions.len()).sum::<usize>());
            assert_eq!(plan.indices, built.meshes.iter().map(|m| m.mesh.indices.len()).sum::<usize>());
        }
    }

    /// The prediction that gates the artifact bound is an UPPER bound on what the writer emits.
    /// Predicting high refuses a little more than it must; predicting low would let a scene
    /// through to exhaust the memory the bound exists to protect, so only this direction is
    /// safe and only this direction is asserted.
    #[test]
    fn prediction_is_an_upper_bound_on_what_the_writer_emits() {
        let long_name = "x".repeat(MAX_NAME_BYTES);
        let named = CUBE.replacen(r#""name": "cube""#, &format!(r#""name": "{long_name}""#), 1);
        for answer in [
            CUBE,
            HIERARCHY,
            &prism_answer(4),
            &prism_answer(8),
            &triangle_answer(),
            &named,
            &many_boxes_scene(64),
            &dense_prism_scene(4),
        ] {
            let dsl = SceneDsl::parse(answer.as_bytes()).unwrap();
            let predicted = predicted_artifact_bytes(&plan_scene(&dsl), dsl.materials.len());
            let actual = glb(answer).len() as u64;
            assert!(predicted >= actual, "predicted {predicted} < actual {actual} for a {} node scene", dsl.nodes.len());
        }
    }

    /// SA-2, the sentence that matters: enforced BEFORE it runs. Each bound is exceeded on its
    /// own, by both routes (the grammar's and the transformer's), and the refusal names it.
    #[test]
    fn each_declared_bound_is_enforced_before_the_build() {
        // max_dsl_bytes, on the raw answer. The sample is not even JSON: if the byte bound ran
        // after the parse, the refusal would name the parser instead, so this also pins the
        // ORDER — which is the whole of SA-2's "before it runs".
        let huge = "x".repeat(MAX_DSL_BYTES + 1);
        match SceneGrammar.canonicalize(huge.as_bytes()) {
            Err(DeriveError::Grammar(msg)) => {
                assert!(msg.contains("max_dsl_bytes"), "{msg}");
                assert!(!msg.contains("json"), "the parser ran before the byte bound: {msg}");
            }
            other => panic!("expected a max_dsl_bytes refusal, got {other:?}"),
        }
        assert!(matches!(SceneGlbTransformer.run(huge.as_bytes()), Err(DeriveError::Transformer(m)) if m.contains("max_dsl_bytes")));
        assert!(SceneGrammar.canonicalize(&vec![b'x'; MAX_DSL_BYTES]).is_err(), "under the bound it is refused as JSON, not as bytes");

        // max_steps: 43 prisms of 256 points are 66 048 vertices, past 65 536, and the DSL that
        // says so is well under the byte bound.
        let over_steps = dense_prism_scene(43);
        assert!(over_steps.len() <= MAX_DSL_BYTES, "the sample must exceed max_steps and nothing else");
        let plan = plan_scene(&SceneDsl::parse(over_steps.as_bytes()).unwrap());
        assert!(plan.vertices as u64 > BOUNDS.max_steps);
        assert!(
            predicted_artifact_bytes(&plan, 1) <= BOUNDS.max_artifact_bytes,
            "the sample must be stopped by steps ALONE — a full prism budget fitting inside the artifact bound is \
             exactly what makes max_steps reachable (see the module doc's density argument)"
        );
        refused(&over_steps, "max_steps");
        assert!(SceneGrammar.canonicalize(dense_prism_scene(42).as_bytes()).is_ok(), "42 prisms are inside every bound");

        // max_artifact_bytes: boxes are the expensive shape, and 760 of them predict past 2 MiB
        // while using a quarter of the vertex budget.
        let over_artifact = many_boxes_scene(760);
        assert!(over_artifact.len() <= MAX_DSL_BYTES);
        let plan = plan_scene(&SceneDsl::parse(over_artifact.as_bytes()).unwrap());
        assert!(plan.vertices as u64 <= BOUNDS.max_steps, "this sample must be stopped by bytes, not by steps");
        refused(&over_artifact, "max_artifact_bytes");

        // and the node count, which is the schema's own bound and reachable inside the others
        let over_nodes = many_boxes_scene(MAX_NODES + 1);
        assert!(over_nodes.len() <= MAX_DSL_BYTES, "the node bound must be reachable inside the byte bound");
        refused(&over_nodes, &format!("at most {MAX_NODES} nodes"));

        // nothing was built in any of those: the derivation has no object either way (X4)
        for answer in [huge.as_str(), over_steps.as_str(), over_artifact.as_str(), over_nodes.as_str()] {
            assert!(derive_with(&SceneGrammar, &SceneGlbTransformer, &binding(), answer.as_bytes()).is_err());
            assert!(derive_named(TRANSFORMER_NAME, &binding(), answer.as_bytes()).is_err());
        }
    }

    // ----- (6) the corpus and its golden ------------------------------------------------------

    fn corpus_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("corpus").join("scene")
    }

    fn corpus_files() -> Vec<(String, Vec<u8>)> {
        let mut files: Vec<(String, Vec<u8>)> = std::fs::read_dir(corpus_dir())
            .expect("corpus/scene exists")
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
            .filter(|p| p.file_name().and_then(|n| n.to_str()) != Some("golden.json"))
            .map(|p| (p.file_name().unwrap().to_str().unwrap().to_string(), std::fs::read(&p).unwrap()))
            .collect();
        files.sort();
        assert!(files.len() >= 9, "the corpus holds {} samples; at least nine are expected", files.len());
        files
    }

    /// One corpus sample's pinned outcome: either the two hashes and the size, or the exact
    /// refusal. A refusal is pinned by its whole message on purpose — X3 asks for the same
    /// artifact on two architectures, and a refusal is an outcome like any other, so two hosts
    /// that refuse a bound-exhausting DSL differently have diverged just as surely.
    fn golden_entry(d: &Derivation) -> serde_json::Value {
        serde_json::json!({
            "dsl_hash": d.dsl_hash.to_string(),
            "artifact_hash": d.artifact_hash.to_string(),
            "artifact_bytes": d.artifact.bytes.len(),
        })
    }

    /// ADR-0078 X3 (determinism, pinned here), X4 and SA-2 (a bound exceeded is no object) and
    /// X6 (the consumer recomputes both hashes from the answer bytes and the object alone).
    #[test]
    fn corpus_matches_golden_and_verifies_through_the_registry() {
        let golden: serde_json::Value =
            serde_json::from_slice(&std::fs::read(corpus_dir().join("golden.json")).expect("golden.json")).unwrap();
        let golden = golden.as_object().expect("golden.json is an object");
        let grammar_id = grammar_id_v1(GRAMMAR_NAME);
        let (mut derived, mut refusals) = (0usize, Vec::new());
        for (name, answer) in corpus_files() {
            let g = golden.get(&name).unwrap_or_else(|| panic!("{name} has no golden entry; pin it"));
            if let Some(expected) = g.get("refused") {
                // X4 and SA-2: no object, no artifact, and the same refusal every time.
                let err = match derive_with(&SceneGrammar, &SceneGlbTransformer, &binding(), &answer) {
                    Err(e) => e,
                    Ok(d) => panic!("{name} must be refused, but derived {} bytes", d.artifact.bytes.len()),
                };
                assert_eq!(err.to_string(), expected.as_str().unwrap(), "{name}: the refusal moved");
                // and through the registry route the gateway uses, identically
                let named = derive_named(TRANSFORMER_NAME, &binding(), &answer).unwrap_err();
                assert_eq!(named.to_string(), err.to_string(), "{name}: the two routes refuse differently");
                refusals.push(err.to_string());
                continue;
            }
            let d = derive_with(&SceneGrammar, &SceneGlbTransformer, &binding(), &answer).unwrap_or_else(|e| panic!("{name}: {e}"));
            derived += 1;
            assert_eq!(d.kind, kind::SCENE);
            assert_eq!(d.grammar_id, grammar_id);
            assert_eq!(d.dsl_hash, dsl_hash_v1(&grammar_id, &d.canonical_dsl));
            assert_eq!(d.artifact_hash, artifact_hash_v1(&d.artifact.bytes));
            assert_eq!(d.artifact.media_type, MEDIA_TYPE);
            assert_eq!(d.artifact.extension, EXTENSION);
            assert_eq!(d.object.artifact_bytes as usize, d.artifact.bytes.len());
            assert_eq!(g, &golden_entry(&d), "{name}: the pinned derivation moved");
            // X6: the consumer's path — from the answer and the object alone, through the registry
            let v = verify(&d.object, &answer).unwrap();
            assert!(v.all_match(), "{name}: {v:?}");
            assert_eq!(v.recomputed_dsl_hash, d.dsl_hash);
            assert_eq!(v.recomputed_artifact_hash, d.artifact_hash);
            assert!(verify_artifact_bytes(&d.object, &d.artifact.bytes), "{name}");
            // the registry route names the same object, and a second run is the same bytes
            assert_eq!(derive_named(TRANSFORMER_NAME, &binding(), &answer).unwrap().object, d.object);
            assert_eq!(SceneGlbTransformer.run(&d.canonical_dsl).unwrap().bytes, d.artifact.bytes);
            assert_eq!(SceneGrammar.canonicalize(&d.canonical_dsl).unwrap(), d.canonical_dsl);
            // the prediction the bound gate used was an upper bound on what was written
            let plan = plan_scene(&SceneDsl::parse(&answer).unwrap());
            let predicted = predicted_artifact_bytes(&plan, SceneDsl::parse(&answer).unwrap().materials.len());
            assert!(predicted >= d.artifact.bytes.len() as u64, "{name}: predicted {predicted} < actual");
            // and the bytes are a structurally well-formed GLB
            walk(&d.artifact.bytes);
        }
        assert_eq!(golden.len(), derived + refusals.len(), "no stale golden entries");
        assert!(derived >= 5, "the corpus derives {derived} artifacts; at least five are expected");
        // SA-2's bound-exhausting corpus: each declared bound is reached by a sample, and the
        // discipline's own refusal too. A bound no corpus file reaches is a comment.
        for needle in ["max_dsl_bytes", "max_steps", "max_artifact_bytes", "binary32"] {
            assert!(refusals.iter().any(|r| r.contains(needle)), "no corpus sample exhausts {needle}");
        }
    }

    /// Re-pin: `cargo test -p misaka-palw-derive print_scene_golden -- --ignored --nocapture`.
    /// Ignored because pinning is a decision, not a test. With `PALW_DERIVE_SCENE_DUMP_DIR` set,
    /// the `.glb` files are written there too, to be opened in a viewer.
    #[test]
    #[ignore]
    fn print_scene_golden() {
        let dump = std::env::var_os("PALW_DERIVE_SCENE_DUMP_DIR").map(PathBuf::from);
        let mut out = serde_json::Map::new();
        for (name, answer) in corpus_files() {
            match derive_with(&SceneGrammar, &SceneGlbTransformer, &binding(), &answer) {
                Ok(d) => {
                    if let Some(dir) = &dump {
                        std::fs::write(dir.join(format!("{name}.glb")), &d.artifact.bytes).unwrap();
                    }
                    out.insert(name, golden_entry(&d));
                }
                Err(e) => {
                    out.insert(name, serde_json::json!({ "refused": e.to_string() }));
                }
            }
        }
        println!("{}", serde_json::to_string_pretty(&serde_json::Value::Object(out)).unwrap());
    }

    // ----- (7) the discipline, scanned --------------------------------------------------------

    /// ADR-0078 Decision 3 declares no floating-point arithmetic on any path that reaches the
    /// output. glTF nonetheless STORES binary32, so this file names `crate::fixed`'s two
    /// converters — and nothing else.
    ///
    /// The scan reads the CODE: comments are cut first, because the doc above names the two
    /// types in order to say it refuses them, and a gate that cannot tell a prohibition from a
    /// use would force the file to stop explaining itself. What remains is stripped of the two
    /// named converters and must spell neither type. `f32_le_exact` builds a bit pattern with
    /// integer arithmetic; no value of either type is created or computed with here.
    #[test]
    fn no_floating_point_type_is_spelled_in_the_code_of_this_file() {
        let code = include_str!("scene.rs")
            .lines()
            .map(|line| match line.find("//") {
                Some(at) => &line[..at],
                None => line,
            })
            .collect::<Vec<_>>()
            .join("\n")
            .replace("f32_le_exact", "")
            .replace("f32_bits_exact", "");
        for width in ["32", "64"] {
            let token = format!("f{width}");
            assert!(!code.contains(&token), "the code of scene.rs spells {token} outside the two named exact converters");
        }
        assert!(!code.contains(&format!("Hash{}", "Map")), "an unordered map is a source of divergence");
    }
}
