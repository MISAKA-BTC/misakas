//! Kind `image` (ADR-0078 Decision 8, row `image`): a vector/procedural DSL of integer-coordinate
//! shapes, an integer rasterizer, and a canonical PNG writer. Grammar `image/v1`, transformer
//! `image/png/v1`. Everything on the path from DSL bytes to PNG bytes is integer arithmetic
//! (Decision 3, invariant X3): no floating-point type appears in this file, not even in a comment
//! — a test scans the source for the two type names and fails if either is spelled — no clock,
//! no randomness, no I/O, no hash-map iteration. Coverage is a pixel-centre test, exact; there is
//! no anti-aliasing, because a fractional coverage is a number two hosts could round apart.
//!
//! # The DSL — grammar `image/v1`
//!
//! A JSON object with exactly these keys (an unknown key is a grammar refusal, a missing one too):
//!
//! ```text
//! { "v": 1,
//!   "width": 1..=4096, "height": 1..=4096,        width*height <= 4_194_304 (16 MiB of RGBA)
//!   "background": [r, g, b, a],                    each 0..=255; the canvas's INITIAL value
//!   "layers": [ shape, ... ] }                     0..=4096 shapes, drawn in order
//! ```
//!
//! and each shape is one of
//!
//! ```text
//! { "shape": "rect",    "x": i, "y": i, "w": 1..=2^21, "h": 1..=2^21,   "fill": [r,g,b,a] }
//! { "shape": "circle",  "cx": i, "cy": i, "r": 1..=4096,              "fill": [r,g,b,a] }
//! { "shape": "polygon", "points": [[x,y] x 3..=1024],                 "fill": [r,g,b,a] }
//! { "shape": "line",    "x0": i, "y0": i, "x1": i, "y1": i,           "fill": [r,g,b,a] }
//! ```
//!
//! Every coordinate `i`, `x`, `y` lies in `-2^20..=2^20`; a shape may lie partly or wholly off
//! the canvas and is clipped. The canonical form is `canon_json`'s: sorted keys, no whitespace,
//! integers only. The grammar validates the schema before it canonicalizes, so a canonical DSL
//! is also a valid one, and the transformer refuses bytes that are not canonical.
//!
//! # Coverage rules (the pixel-centre test)
//!
//! Pixel `(px, py)` — column `px` in `0..width`, row `py` in `0..height`, row 0 at the top — is
//! sampled at the integer point `(px, py)`; pixel centres sit ON integer coordinates, there is
//! no half-pixel offset anywhere in this kind.
//!
//! * **rect**: covered iff `x <= px < x + w` and `y <= py < y + h`.
//! * **circle**: covered iff `(px - cx)^2 + (py - cy)^2 <= r^2`, in 64-bit integers.
//! * **polygon**: even-odd scanline fill. For row `py`, an edge `(x0,y0)-(x1,y1)` with `y0 != y1`
//!   crosses the row iff `min(y0,y1) <= py < max(y0,y1)` — half-open, so a vertex row is counted
//!   by exactly one of the two edges that meet at the vertex, and a horizontal edge never crosses.
//!   Its crossing column is `x0 + floor((py - y0) * (x1 - x0) / (y1 - y0))`, the exact
//!   intersection rounded toward negative infinity ([`floor_div`]). The row's crossings are sorted
//!   ascending and paired; pixel `px` is covered iff `c[2k] <= px < c[2k+1]` for some pair `k`.
//!   Because the polygon is closed and the rule is half-open, the crossing count is even.
//! * **line**: Bresenham's algorithm from `(x0,y0)` to `(x1,y1)`, one pixel wide, every
//!   pixel visited once. A line is directed: the reverse line may differ by a pixel where the
//!   error term ties, and that is part of the rule, not a defect.
//!
//! Each covered pixel of a shape is composited exactly once per shape.
//!
//! # Compositing (one rounding rule)
//!
//! Source-over in 8-bit integer arithmetic with a single rule, [`mix`]:
//!
//! ```text
//! mix(src, dst, a) = (src*a + dst*(255 - a) + 127) / 255        (integer division)
//! ```
//!
//! Colour channels: `out_c = mix(fill_c, dst_c, fill_a)`. Alpha, likewise: `out_a = mix(255,
//! dst_a, fill_a)`, which is source-over alpha `a + dst_a*(1 - a)` under the same rounding. The
//! colour rule is a plain lerp by the source alpha (not premultiplied): exact over an opaque
//! background, and a fixed, stated rule over a translucent one. `mix(s, d, 255) = s` and
//! `mix(s, d, 0) = d` hold exactly, so opaque and invisible fills need no special case.
//!
//! # The artifact — writer `png/1.2/rgba8-filter0-stored-v1`
//!
//! Signature; `IHDR` (width, height, bit depth 8, colour type 6 = RGBA, compression 0, filter 0,
//! interlace 0); ONE `IDAT` whose payload is `zlib_stored(raw)` — [`crate::zlib::zlib_stored`],
//! the one deflate "level" this crate uses, stored blocks, exactly one possible output — where
//! `raw` is, per scanline, a filter-type byte 0 followed by `width*4` bytes of RGBA; then `IEND`.
//! Each chunk is length (big-endian), type, data, CRC-32 over type+data. The artifact's size is
//! a pure function of the dimensions ([`png_stored_size`]) and is refused above 32 MiB.
//!
//! # Cost
//!
//! Rasterization is `O(clipped area)` per rect and circle, `O(rows * edges + clipped area)` per
//! polygon and `O(max(|dx|, |dy|))` per line, all bounded by the schema: the worst DSL the
//! grammar admits is 4096 lines of 2^21 steps, which is slow and finite, never unbounded.

use crate::bytes::put_u32_be;
use crate::canon_json::{CanonValue, parse_canonical, write_canonical};
use crate::checksum::crc32;
use crate::zlib::zlib_stored;
use crate::{Artifact, DeriveError, Discipline, Grammar, Transformer, TransformerManifest};
use kaspa_consensus_core::palw_derived_v1::kind;
use std::collections::BTreeMap;

/// The grammar's name (Decision 2): `grammar_id = H(domain ‖ name)`.
pub const GRAMMAR_NAME: &str = "image/v1";
/// The transformer's name, the first field of its manifest (Decision 3).
pub const TRANSFORMER_NAME: &str = "image/png/v1";
/// The canonical writer, named in the manifest: PNG 1.2, 8-bit RGBA, filter type 0 on every
/// scanline, one stored-block zlib stream, chunk order IHDR / IDAT / IEND.
pub const WRITER_NAME: &str = "png/1.2/rgba8-filter0-stored-v1";

/// Largest width and height.
pub const MAX_SIDE: u32 = 4096;
/// Largest `width * height`: 4,194,304 pixels, 16 MiB of RGBA.
pub const MAX_PIXELS: u64 = 4_194_304;
/// Most shapes in `layers`.
pub const MAX_LAYERS: usize = 4096;
/// Fewest and most points of a polygon.
pub const MIN_POLYGON_POINTS: usize = 3;
pub const MAX_POLYGON_POINTS: usize = 1024;
/// Every coordinate lies in `-COORD_LIMIT..=COORD_LIMIT`.
pub const COORD_LIMIT: i64 = 1 << 20;
/// A rect's `w` and `h` lie in `1..=SIZE_LIMIT`, so a rect anchored at `-COORD_LIMIT` can still
/// reach `+COORD_LIMIT`.
pub const SIZE_LIMIT: i64 = 1 << 21;
/// A circle's radius lies in `1..=MAX_RADIUS`.
pub const MAX_RADIUS: i64 = 4096;
/// **ADR-0078 SA-2's `max_dsl_bytes`.** The most answer bytes this kind will look at, checked on
/// the byte COUNT before the parser is asked what the bytes spell — a JSON parser is an allocator
/// driven by its input, and a bound applied after parsing is applied after the damage. Exceeding
/// it is "no object" (Decision 2's parse-failure arm, X4), never a repair and never a truncation.
///
/// The number is the retention payload's own cap (`PALW_FP_DSL_V1_MAX_BYTES`): a DSL above it
/// could not be served to a verifier under Decision 6 even if it derived, so deriving from one
/// would be building a derivation nobody could check. This kind's schema admits documents larger
/// than that in its extreme corner (4,096 layers of 1,024-point polygons), and this ceiling is the
/// binding one — it is far above any answer a class at these widths emits, and far below
/// what a parser could be made to allocate.
pub const MAX_DSL_BYTES: u64 = kaspa_consensus_core::palw_derived_v1::PALW_FP_DSL_V1_MAX_BYTES as u64;

/// The artifact ceiling: a PNG above this is refused by the transformer.
pub const ARTIFACT_MAX_BYTES: u64 = 32 * 1024 * 1024;

/// The eight-byte PNG signature.
pub const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

/// A colour with alpha, `[r, g, b, a]`, each `0..=255`.
pub type Rgba = [u8; 4];

/// One shape of the `layers` array, validated and in range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shape {
    Rect { x: i64, y: i64, w: i64, h: i64, fill: Rgba },
    Circle { cx: i64, cy: i64, r: i64, fill: Rgba },
    Polygon { points: Vec<(i64, i64)>, fill: Rgba },
    Line { x0: i64, y0: i64, x1: i64, y1: i64, fill: Rgba },
}

/// A validated `image/v1` document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageDsl {
    pub width: u32,
    pub height: u32,
    pub background: Rgba,
    pub layers: Vec<Shape>,
}

/// The grammar `image/v1`: parse, validate the schema, re-emit canonically.
pub struct ImageGrammar;

/// The transformer `image/png/v1`: canonical `image/v1` bytes to a PNG.
pub struct ImagePngTransformer;

/// This kind's grammar and transformer, as the registry sees them.
pub fn register() -> (Vec<Box<dyn Grammar>>, Vec<Box<dyn Transformer>>) {
    (vec![Box::new(ImageGrammar)], vec![Box::new(ImagePngTransformer)])
}

impl Grammar for ImageGrammar {
    fn name(&self) -> &'static str {
        GRAMMAR_NAME
    }

    /// Parse → validate → write. A violation of the schema is a grammar refusal (X4: no object,
    /// the claim untouched), never a repair.
    fn canonicalize(&self, answer: &[u8]) -> Result<Vec<u8>, DeriveError> {
        crate::check_dsl_bytes(MAX_DSL_BYTES, answer)?;
        let tree = parse_canonical(answer)?;
        ImageDsl::from_tree(&tree)?;
        Ok(write_canonical(&tree))
    }
}

impl Transformer for ImagePngTransformer {
    fn manifest(&self) -> TransformerManifest {
        TransformerManifest {
            name: TRANSFORMER_NAME,
            kind: kind::IMAGE,
            grammar: GRAMMAR_NAME,
            discipline: Discipline::Integer,
            writer: WRITER_NAME,
            source_tree_sha256: crate::SOURCE_TREE_SHA256_HEX,
            // ADR-0078 SA-2: the ceilings this kind enforces, each already a constant above.
            max_dsl_bytes: MAX_DSL_BYTES,
            max_artifact_bytes: ARTIFACT_MAX_BYTES,
            max_steps: MAX_PIXELS,
        }
    }

    /// Re-canonicalize and refuse anything that is not already canonical; then rasterize and
    /// write. The refusal is the trait's rule ("refuse, not repair"): a transformer that repaired
    /// its input would let two spellings of one answer name one artifact under two `dsl_hash`es.
    fn run(&self, dsl: &[u8]) -> Result<Artifact, DeriveError> {
        crate::check_dsl_bytes(MAX_DSL_BYTES, dsl)?;
        let tree = parse_canonical(dsl)?;
        let image = ImageDsl::from_tree(&tree)?;
        if write_canonical(&tree) != dsl {
            return Err(DeriveError::Transformer(
                "input is not canonical image/v1 bytes; canonicalize under the grammar first".into(),
            ));
        }
        let size = png_stored_size(image.width, image.height);
        if size > ARTIFACT_MAX_BYTES {
            return Err(DeriveError::Transformer(format!(
                "a {}x{} RGBA PNG is {size} bytes, above the {ARTIFACT_MAX_BYTES}-byte artifact ceiling",
                image.width, image.height
            )));
        }
        let rgba = rasterize(&image);
        let bytes = write_png_rgba8(image.width, image.height, &rgba);
        Ok(Artifact { bytes, media_type: "image/png", extension: "png" })
    }
}

// ---------------------------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------------------------

fn grammar_err(msg: String) -> DeriveError {
    DeriveError::Grammar(msg)
}

type Obj = BTreeMap<String, CanonValue>;

/// Refuse any key outside `allowed`. Keys are visited in `BTreeMap` order, so the key named in
/// the refusal is the same on every host.
fn only_keys(obj: &Obj, allowed: &[&str], ctx: &str) -> Result<(), DeriveError> {
    for key in obj.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(grammar_err(format!("{ctx}unknown key {key:?}")));
        }
    }
    Ok(())
}

/// `obj[key]` as an integer in `lo..=hi`; missing, non-integer, out of `i64`, or out of range
/// are each a refusal naming the key.
fn int_field(obj: &Obj, key: &str, lo: i64, hi: i64, ctx: &str) -> Result<i64, DeriveError> {
    let v = obj.get(key).ok_or_else(|| grammar_err(format!("{ctx}missing key {key:?}")))?;
    match v.as_i64() {
        Some(n) if (lo..=hi).contains(&n) => Ok(n),
        _ => Err(grammar_err(format!("{ctx}{key:?} must be an integer in {lo}..={hi}"))),
    }
}

/// A coordinate field: an integer in `-COORD_LIMIT..=COORD_LIMIT`.
fn coord_field(obj: &Obj, key: &str, ctx: &str) -> Result<i64, DeriveError> {
    int_field(obj, key, -COORD_LIMIT, COORD_LIMIT, ctx)
}

/// `obj[key]` as `[r, g, b, a]`, each `0..=255`.
fn rgba_field(obj: &Obj, key: &str, ctx: &str) -> Result<Rgba, DeriveError> {
    let refuse = || grammar_err(format!("{ctx}{key:?} must be [r,g,b,a] with each channel in 0..=255"));
    let v = obj.get(key).ok_or_else(|| grammar_err(format!("{ctx}missing key {key:?}")))?;
    let arr = v.as_arr().ok_or_else(refuse)?;
    if arr.len() != 4 {
        return Err(refuse());
    }
    let mut out = [0u8; 4];
    for (slot, item) in out.iter_mut().zip(arr) {
        *slot = match item.as_i64() {
            Some(n) if (0..=255).contains(&n) => n as u8,
            _ => return Err(refuse()),
        };
    }
    Ok(out)
}

impl ImageDsl {
    /// Parse and validate `bytes` (canonical or not) under `image/v1`.
    pub fn parse(bytes: &[u8]) -> Result<Self, DeriveError> {
        Self::from_tree(&parse_canonical(bytes)?)
    }

    /// Validate a parsed tree against the schema in the module doc.
    pub fn from_tree(tree: &CanonValue) -> Result<Self, DeriveError> {
        let obj = tree.as_obj().ok_or_else(|| grammar_err("image/v1: the DSL must be a JSON object".into()))?;
        only_keys(obj, &["v", "width", "height", "background", "layers"], "")?;
        match obj.get("v") {
            Some(CanonValue::Int(1)) => {}
            Some(_) => return Err(grammar_err("\"v\" must be 1".into())),
            None => return Err(grammar_err("missing key \"v\"".into())),
        }
        let width = int_field(obj, "width", 1, MAX_SIDE as i64, "")? as u32;
        let height = int_field(obj, "height", 1, MAX_SIDE as i64, "")? as u32;
        if width as u64 * height as u64 > MAX_PIXELS {
            return Err(grammar_err(format!("width*height must not exceed {MAX_PIXELS} pixels ({width}x{height} given)")));
        }
        let background = rgba_field(obj, "background", "")?;
        let layers_v = obj.get("layers").ok_or_else(|| grammar_err("missing key \"layers\"".into()))?;
        let layers_arr = layers_v.as_arr().ok_or_else(|| grammar_err("\"layers\" must be an array".into()))?;
        if layers_arr.len() > MAX_LAYERS {
            return Err(grammar_err(format!("\"layers\" holds at most {MAX_LAYERS} shapes ({} given)", layers_arr.len())));
        }
        let mut layers = Vec::with_capacity(layers_arr.len());
        for (i, layer) in layers_arr.iter().enumerate() {
            layers.push(parse_shape(layer, i)?);
        }
        Ok(ImageDsl { width, height, background, layers })
    }
}

fn parse_shape(v: &CanonValue, index: usize) -> Result<Shape, DeriveError> {
    let obj = v.as_obj().ok_or_else(|| grammar_err(format!("layers[{index}]: a shape must be a JSON object")))?;
    let name = match obj.get("shape") {
        None => return Err(grammar_err(format!("layers[{index}]: missing key \"shape\""))),
        Some(CanonValue::Str(s)) => s.as_str(),
        Some(_) => return Err(grammar_err(format!("layers[{index}]: \"shape\" must be a string"))),
    };
    let ctx = format!("layers[{index}] ({name}): ");
    match name {
        "rect" => {
            only_keys(obj, &["shape", "x", "y", "w", "h", "fill"], &ctx)?;
            Ok(Shape::Rect {
                x: coord_field(obj, "x", &ctx)?,
                y: coord_field(obj, "y", &ctx)?,
                w: int_field(obj, "w", 1, SIZE_LIMIT, &ctx)?,
                h: int_field(obj, "h", 1, SIZE_LIMIT, &ctx)?,
                fill: rgba_field(obj, "fill", &ctx)?,
            })
        }
        "circle" => {
            only_keys(obj, &["shape", "cx", "cy", "r", "fill"], &ctx)?;
            Ok(Shape::Circle {
                cx: coord_field(obj, "cx", &ctx)?,
                cy: coord_field(obj, "cy", &ctx)?,
                r: int_field(obj, "r", 1, MAX_RADIUS, &ctx)?,
                fill: rgba_field(obj, "fill", &ctx)?,
            })
        }
        "polygon" => {
            only_keys(obj, &["shape", "points", "fill"], &ctx)?;
            let pts = obj.get("points").ok_or_else(|| grammar_err(format!("{ctx}missing key \"points\"")))?;
            let arr = pts.as_arr().ok_or_else(|| grammar_err(format!("{ctx}\"points\" must be an array of [x,y]")))?;
            if !(MIN_POLYGON_POINTS..=MAX_POLYGON_POINTS).contains(&arr.len()) {
                return Err(grammar_err(format!(
                    "{ctx}\"points\" holds {MIN_POLYGON_POINTS}..={MAX_POLYGON_POINTS} points ({} given)",
                    arr.len()
                )));
            }
            let mut points = Vec::with_capacity(arr.len());
            for (j, p) in arr.iter().enumerate() {
                let refuse = || {
                    grammar_err(format!("{ctx}\"points\"[{j}] must be [x,y] with each coordinate in -{COORD_LIMIT}..={COORD_LIMIT}"))
                };
                let pair = p.as_arr().ok_or_else(refuse)?;
                if pair.len() != 2 {
                    return Err(refuse());
                }
                let x = pair[0].as_i64().filter(|n| (-COORD_LIMIT..=COORD_LIMIT).contains(n)).ok_or_else(refuse)?;
                let y = pair[1].as_i64().filter(|n| (-COORD_LIMIT..=COORD_LIMIT).contains(n)).ok_or_else(refuse)?;
                points.push((x, y));
            }
            Ok(Shape::Polygon { points, fill: rgba_field(obj, "fill", &ctx)? })
        }
        "line" => {
            only_keys(obj, &["shape", "x0", "y0", "x1", "y1", "fill"], &ctx)?;
            Ok(Shape::Line {
                x0: coord_field(obj, "x0", &ctx)?,
                y0: coord_field(obj, "y0", &ctx)?,
                x1: coord_field(obj, "x1", &ctx)?,
                y1: coord_field(obj, "y1", &ctx)?,
                fill: rgba_field(obj, "fill", &ctx)?,
            })
        }
        other => Err(grammar_err(format!("layers[{index}]: unknown shape {other:?}"))),
    }
}

// ---------------------------------------------------------------------------------------------
// Arithmetic
// ---------------------------------------------------------------------------------------------

/// Floor division: the largest integer `q` with `q * b <= a` (for `b > 0`), i.e. `a / b` rounded
/// toward negative infinity, where Rust's `/` rounds toward zero. `b` must be non-zero.
pub fn floor_div(a: i64, b: i64) -> i64 {
    let q = a / b;
    let r = a % b;
    if r != 0 && ((r < 0) != (b < 0)) { q - 1 } else { q }
}

/// The one compositing rule: `(src*a + dst*(255 - a) + 127) / 255`. Exact at both ends:
/// `mix(s, d, 255) == s`, `mix(s, d, 0) == d`.
pub fn mix(src: u8, dst: u8, a: u8) -> u8 {
    let (s, d, a) = (src as u32, dst as u32, a as u32);
    ((s * a + d * (255 - a) + 127) / 255) as u8
}

// ---------------------------------------------------------------------------------------------
// Rasterizer
// ---------------------------------------------------------------------------------------------

struct Canvas {
    width: i64,
    height: i64,
    /// RGBA, row-major, row 0 first.
    px: Vec<u8>,
}

impl Canvas {
    fn new(width: u32, height: u32, background: Rgba) -> Self {
        let px = background.repeat(width as usize * height as usize);
        Canvas { width: width as i64, height: height as i64, px }
    }

    /// Composite `fill` onto pixel `(x, y)`; off-canvas is a no-op.
    fn blend(&mut self, x: i64, y: i64, fill: Rgba) {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return;
        }
        let i = ((y * self.width + x) as usize) * 4;
        let a = fill[3];
        let p = &mut self.px[i..i + 4];
        p[0] = mix(fill[0], p[0], a);
        p[1] = mix(fill[1], p[1], a);
        p[2] = mix(fill[2], p[2], a);
        p[3] = mix(255, p[3], a);
    }

    fn fill_rect(&mut self, x: i64, y: i64, w: i64, h: i64, fill: Rgba) {
        let (x_lo, x_hi) = (x.max(0), (x + w).min(self.width));
        let (y_lo, y_hi) = (y.max(0), (y + h).min(self.height));
        for py in y_lo..y_hi {
            for px in x_lo..x_hi {
                self.blend(px, py, fill);
            }
        }
    }

    fn fill_circle(&mut self, cx: i64, cy: i64, r: i64, fill: Rgba) {
        let rr = r * r;
        let (x_lo, x_hi) = ((cx - r).max(0), (cx + r).min(self.width - 1));
        let (y_lo, y_hi) = ((cy - r).max(0), (cy + r).min(self.height - 1));
        for py in y_lo..=y_hi {
            let dy = py - cy;
            for px in x_lo..=x_hi {
                let dx = px - cx;
                if dx * dx + dy * dy <= rr {
                    self.blend(px, py, fill);
                }
            }
        }
    }

    /// Even-odd scanline fill; the rule is stated in the module doc.
    fn fill_polygon(&mut self, points: &[(i64, i64)], fill: Rgba) {
        let y_min = points.iter().map(|p| p.1).min().unwrap_or(0);
        let y_max = points.iter().map(|p| p.1).max().unwrap_or(0);
        let (row_lo, row_hi) = (y_min.max(0), y_max.min(self.height));
        let mut crossings: Vec<i64> = Vec::with_capacity(points.len());
        for py in row_lo..row_hi {
            crossings.clear();
            for (&(x0, y0), &(x1, y1)) in points.iter().zip(points.iter().cycle().skip(1)) {
                if y0 != y1 && y0.min(y1) <= py && py < y0.max(y1) {
                    crossings.push(x0 + floor_div((py - y0) * (x1 - x0), y1 - y0));
                }
            }
            crossings.sort_unstable();
            for pair in crossings.as_chunks::<2>().0 {
                let (x_lo, x_hi) = (pair[0].max(0), pair[1].min(self.width));
                for px in x_lo..x_hi {
                    self.blend(px, py, fill);
                }
            }
        }
    }

    /// Bresenham, all octants, one pixel wide, each pixel once.
    fn draw_line(&mut self, x0: i64, y0: i64, x1: i64, y1: i64, fill: Rgba) {
        // A line whose bounding box misses the canvas plots nothing; skip the walk.
        if x0.max(x1) < 0 || y0.max(y1) < 0 || x0.min(x1) >= self.width || y0.min(y1) >= self.height {
            return;
        }
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let (mut x, mut y) = (x0, y0);
        let mut err = dx + dy;
        loop {
            self.blend(x, y, fill);
            if x == x1 && y == y1 {
                return;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }
}

/// Rasterize a validated document to RGBA8, row-major, `width*height*4` bytes.
pub fn rasterize(image: &ImageDsl) -> Vec<u8> {
    let mut canvas = Canvas::new(image.width, image.height, image.background);
    for shape in &image.layers {
        match shape {
            Shape::Rect { x, y, w, h, fill } => canvas.fill_rect(*x, *y, *w, *h, *fill),
            Shape::Circle { cx, cy, r, fill } => canvas.fill_circle(*cx, *cy, *r, *fill),
            Shape::Polygon { points, fill } => canvas.fill_polygon(points, *fill),
            Shape::Line { x0, y0, x1, y1, fill } => canvas.draw_line(*x0, *y0, *x1, *y1, *fill),
        }
    }
    canvas.px
}

// ---------------------------------------------------------------------------------------------
// PNG writer
// ---------------------------------------------------------------------------------------------

/// One chunk: length (BE), type, data, CRC-32 over type+data.
fn put_chunk(out: &mut Vec<u8>, ty: &[u8; 4], data: &[u8]) {
    put_u32_be(out, data.len() as u32);
    let start = out.len();
    out.extend_from_slice(ty);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    put_u32_be(out, crc);
}

/// The canonical PNG of `rgba` (`width*height*4` bytes, row-major): IHDR, one IDAT of stored
/// zlib blocks over filter-0 scanlines, IEND.
pub fn write_png_rgba8(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let row = width as usize * 4;
    debug_assert_eq!(rgba.len(), row * height as usize);
    let mut raw = Vec::with_capacity(height as usize * (row + 1));
    for line in rgba.chunks_exact(row) {
        raw.push(0);
        raw.extend_from_slice(line);
    }
    let mut ihdr = Vec::with_capacity(13);
    put_u32_be(&mut ihdr, width);
    put_u32_be(&mut ihdr, height);
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut out = Vec::with_capacity(png_stored_size(width, height) as usize);
    out.extend_from_slice(&PNG_SIGNATURE);
    put_chunk(&mut out, b"IHDR", &ihdr);
    put_chunk(&mut out, b"IDAT", &zlib_stored(&raw));
    put_chunk(&mut out, b"IEND", &[]);
    out
}

/// The exact byte size of [`write_png_rgba8`]'s output for these dimensions: signature 8, IHDR
/// chunk 25, IDAT chunk 12 + zlib (2 header + raw + 5 per stored block + 4 adler), IEND chunk 12.
pub fn png_stored_size(width: u32, height: u32) -> u64 {
    let raw = height as u64 * (1 + 4 * width as u64);
    let blocks = raw.div_ceil(65_535).max(1);
    let zlib = 2 + raw + 5 * blocks + 4;
    8 + 25 + (12 + zlib) + 12
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::derive::{ClaimBinding, derive_with, verify};
    use crate::ids::{artifact_hash_v1, dsl_hash_v1, grammar_id_v1};
    use kaspa_hashes::Hash64;
    use std::path::PathBuf;

    // ----- helpers -------------------------------------------------------------------------

    fn base(layers: &str) -> String {
        format!(r#"{{"v":1,"width":4,"height":4,"background":[0,0,255,255],"layers":[{layers}]}}"#)
    }

    fn canon(input: &str) -> Result<Vec<u8>, DeriveError> {
        ImageGrammar.canonicalize(input.as_bytes())
    }

    fn png_of(input: &str) -> Vec<u8> {
        let dsl = canon(input).expect("valid DSL");
        ImagePngTransformer.run(&dsl).expect("transformer").bytes
    }

    /// `canonicalize` refuses `input` with a grammar error whose text contains `fragment`.
    fn refuses(input: &str, fragment: &str) {
        match canon(input) {
            Err(DeriveError::Grammar(msg)) => assert!(msg.contains(fragment), "expected {fragment:?} in {msg:?}"),
            other => panic!("expected a grammar refusal containing {fragment:?}, got {other:?}"),
        }
    }

    /// A stored-block zlib inflater: enough to read what `zlib_stored` writes, and to check
    /// the adler.
    fn inflate_stored(z: &[u8]) -> Vec<u8> {
        assert_eq!(&z[..2], &[0x78, 0x01], "zlib header");
        let mut i = 2;
        let mut out = Vec::new();
        loop {
            let hdr = z[i];
            i += 1;
            assert_eq!(hdr & 0x06, 0, "BTYPE must be 00 (stored)");
            let len = u16::from_le_bytes([z[i], z[i + 1]]);
            let nlen = u16::from_le_bytes([z[i + 2], z[i + 3]]);
            assert_eq!(!len, nlen, "NLEN is the complement of LEN");
            i += 4;
            out.extend_from_slice(&z[i..i + len as usize]);
            i += len as usize;
            if hdr & 1 == 1 {
                break;
            }
        }
        let adler = u32::from_be_bytes([z[i], z[i + 1], z[i + 2], z[i + 3]]);
        assert_eq!(adler, crate::checksum::adler32(&out), "adler32");
        assert_eq!(i + 4, z.len(), "nothing after the adler");
        out
    }

    struct Chunk {
        ty: [u8; 4],
        data: Vec<u8>,
    }

    /// Split a PNG into chunks, checking the signature and every CRC.
    fn parse_png(png: &[u8]) -> Vec<Chunk> {
        assert_eq!(&png[..8], &PNG_SIGNATURE, "signature");
        let mut i = 8;
        let mut chunks = Vec::new();
        while i < png.len() {
            let len = u32::from_be_bytes([png[i], png[i + 1], png[i + 2], png[i + 3]]) as usize;
            let ty: [u8; 4] = png[i + 4..i + 8].try_into().unwrap();
            let data = png[i + 8..i + 8 + len].to_vec();
            let crc = u32::from_be_bytes(png[i + 8 + len..i + 12 + len].try_into().unwrap());
            assert_eq!(crc, crc32(&png[i + 4..i + 8 + len]), "chunk {} crc", String::from_utf8_lossy(&ty));
            chunks.push(Chunk { ty, data });
            i += 12 + len;
        }
        assert_eq!(i, png.len());
        chunks
    }

    /// Structural checks (signature, IHDR, chunk order, CRCs, the zlib stream, filter bytes) and
    /// the decoded pixels: `(width, height, rgba)`.
    fn decode(png: &[u8]) -> (u32, u32, Vec<u8>) {
        let chunks = parse_png(png);
        assert_eq!(chunks.len(), 3, "IHDR, one IDAT, IEND");
        assert_eq!(&chunks[0].ty, b"IHDR");
        assert_eq!(&chunks[1].ty, b"IDAT");
        assert_eq!(&chunks[2].ty, b"IEND");
        assert!(chunks[2].data.is_empty());
        let ihdr = &chunks[0].data;
        assert_eq!(ihdr.len(), 13);
        let width = u32::from_be_bytes(ihdr[0..4].try_into().unwrap());
        let height = u32::from_be_bytes(ihdr[4..8].try_into().unwrap());
        assert_eq!(&ihdr[8..13], &[8, 6, 0, 0, 0], "bit depth 8, RGBA, compression 0, filter 0, interlace 0");
        let raw = inflate_stored(&chunks[1].data);
        let stride = 1 + 4 * width as usize;
        assert_eq!(raw.len(), stride * height as usize);
        let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
        for line in raw.chunks_exact(stride) {
            assert_eq!(line[0], 0, "filter type 0 on every scanline");
            rgba.extend_from_slice(&line[1..]);
        }
        assert_eq!(png.len() as u64, png_stored_size(width, height));
        (width, height, rgba)
    }

    fn px(img: &(u32, u32, Vec<u8>), x: u32, y: u32) -> Rgba {
        let i = ((y * img.0 + x) * 4) as usize;
        img.2[i..i + 4].try_into().unwrap()
    }

    /// Every pixel of `img` whose value is not `background`.
    fn painted(img: &(u32, u32, Vec<u8>), background: Rgba) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        for y in 0..img.1 {
            for x in 0..img.0 {
                if px(img, x, y) != background {
                    out.push((x, y));
                }
            }
        }
        out
    }

    const BLUE: Rgba = [0, 0, 255, 255];
    const RED: Rgba = [255, 0, 0, 255];

    // ----- (1) canonicalization ------------------------------------------------------------

    #[test]
    fn canonicalization_sorts_keys_strips_whitespace_and_is_idempotent() {
        let messy = r#" {
            "layers" : [ { "fill": [1, 2, 3, 4], "shape": "rect", "y": 1, "x": 0, "h": 2, "w": 3 } ],
            "v": 1, "width" : 4, "background": [0, 0, 255, 255], "height": 4 } "#;
        let once = canon(messy).unwrap();
        assert_eq!(
            once,
            br#"{"background":[0,0,255,255],"height":4,"layers":[{"fill":[1,2,3,4],"h":2,"shape":"rect","w":3,"x":0,"y":1}],"v":1,"width":4}"#
        );
        let twice = canon(std::str::from_utf8(&once).unwrap()).unwrap();
        assert_eq!(once, twice);
    }

    // ----- (2) every schema refusal ---------------------------------------------------------

    #[test]
    fn refuses_every_schema_violation_with_a_named_reason() {
        // not JSON / not UTF-8 / duplicate keys / floats come from canon_json, restated here so
        // this kind's callers can rely on them.
        refuses("{", "json:");
        assert!(matches!(ImageGrammar.canonicalize(&[0xFF, 0xFE]), Err(DeriveError::Grammar(m)) if m.contains("not UTF-8")));
        refuses(r#"{"v":1,"v":1}"#, "duplicate key");
        refuses(r#"{"v":1,"width":4.0,"height":4,"background":[0,0,0,255],"layers":[]}"#, "non-integer number");
        // the document
        refuses("[]", "must be a JSON object");
        refuses(r#"{"v":1,"width":4,"height":4,"background":[0,0,0,255],"layers":[],"extra":1}"#, "unknown key \"extra\"");
        refuses(r#"{"v":1,"height":4,"background":[0,0,0,255],"layers":[]}"#, "missing key \"width\"");
        refuses(r#"{"width":4,"height":4,"background":[0,0,0,255],"layers":[]}"#, "missing key \"v\"");
        refuses(r#"{"v":2,"width":4,"height":4,"background":[0,0,0,255],"layers":[]}"#, "\"v\" must be 1");
        refuses(r#"{"v":"1","width":4,"height":4,"background":[0,0,0,255],"layers":[]}"#, "\"v\" must be 1");
        refuses(r#"{"v":1,"width":0,"height":4,"background":[0,0,0,255],"layers":[]}"#, "\"width\" must be an integer in 1..=4096");
        refuses(r#"{"v":1,"width":4097,"height":4,"background":[0,0,0,255],"layers":[]}"#, "\"width\" must be an integer in 1..=4096");
        refuses(r#"{"v":1,"width":4,"height":-1,"background":[0,0,0,255],"layers":[]}"#, "\"height\" must be an integer in 1..=4096");
        refuses(r#"{"v":1,"width":4096,"height":4096,"background":[0,0,0,255],"layers":[]}"#, "must not exceed 4194304 pixels");
        refuses(r#"{"v":1,"width":4,"height":4,"background":[0,0,0],"layers":[]}"#, "\"background\" must be [r,g,b,a]");
        refuses(r#"{"v":1,"width":4,"height":4,"background":[0,0,0,256],"layers":[]}"#, "\"background\" must be [r,g,b,a]");
        refuses(r#"{"v":1,"width":4,"height":4,"background":"blue","layers":[]}"#, "\"background\" must be [r,g,b,a]");
        refuses(r#"{"v":1,"width":4,"height":4,"background":[0,0,0,255]}"#, "missing key \"layers\"");
        refuses(r#"{"v":1,"width":4,"height":4,"background":[0,0,0,255],"layers":{}}"#, "\"layers\" must be an array");
        let too_many = std::iter::repeat_n(r#"{"shape":"rect","x":0,"y":0,"w":1,"h":1,"fill":[0,0,0,255]}"#, MAX_LAYERS + 1)
            .collect::<Vec<_>>()
            .join(",");
        refuses(&base(&too_many), "\"layers\" holds at most 4096 shapes");
        // shapes
        refuses(&base("1"), "layers[0]: a shape must be a JSON object");
        refuses(&base("{}"), "layers[0]: missing key \"shape\"");
        refuses(&base(r#"{"shape":5}"#), "layers[0]: \"shape\" must be a string");
        refuses(&base(r#"{"shape":"blob"}"#), "layers[0]: unknown shape \"blob\"");
        refuses(&base(r#"{"shape":"rect","x":0,"y":0,"w":1,"h":1,"fill":[0,0,0,255],"z":1}"#), "layers[0] (rect): unknown key \"z\"");
        refuses(&base(r#"{"shape":"rect","x":0,"y":0,"h":1,"fill":[0,0,0,255]}"#), "layers[0] (rect): missing key \"w\"");
        refuses(&base(r#"{"shape":"rect","x":0,"y":0,"w":0,"h":1,"fill":[0,0,0,255]}"#), "\"w\" must be an integer in 1..=2097152");
        refuses(
            &base(r#"{"shape":"rect","x":0,"y":0,"w":1,"h":2097153,"fill":[0,0,0,255]}"#),
            "\"h\" must be an integer in 1..=2097152",
        );
        refuses(
            &base(r#"{"shape":"rect","x":1048577,"y":0,"w":1,"h":1,"fill":[0,0,0,255]}"#),
            "\"x\" must be an integer in -1048576..=1048576",
        );
        refuses(
            &base(r#"{"shape":"rect","x":0,"y":-1048577,"w":1,"h":1,"fill":[0,0,0,255]}"#),
            "\"y\" must be an integer in -1048576..=1048576",
        );
        refuses(&base(r#"{"shape":"rect","x":0,"y":0,"w":1,"h":1,"fill":[0,0,0]}"#), "(rect): \"fill\" must be [r,g,b,a]");
        refuses(&base(r#"{"shape":"rect","x":0,"y":0,"w":1,"h":1,"fill":[0,0,0,-1]}"#), "(rect): \"fill\" must be [r,g,b,a]");
        refuses(&base(r#"{"shape":"rect","x":0,"y":0,"w":1,"h":1}"#), "(rect): missing key \"fill\"");
        refuses(&base(r#"{"shape":"circle","cx":0,"cy":0,"r":0,"fill":[0,0,0,255]}"#), "\"r\" must be an integer in 1..=4096");
        refuses(&base(r#"{"shape":"circle","cx":0,"cy":0,"r":4097,"fill":[0,0,0,255]}"#), "\"r\" must be an integer in 1..=4096");
        refuses(&base(r#"{"shape":"circle","cx":0,"cy":0,"r":1,"fill":[0,0,0,255],"radius":1}"#), "(circle): unknown key \"radius\"");
        refuses(&base(r#"{"shape":"circle","cy":0,"r":1,"fill":[0,0,0,255]}"#), "(circle): missing key \"cx\"");
        refuses(&base(r#"{"shape":"polygon","fill":[0,0,0,255]}"#), "(polygon): missing key \"points\"");
        refuses(&base(r#"{"shape":"polygon","points":7,"fill":[0,0,0,255]}"#), "\"points\" must be an array of [x,y]");
        refuses(&base(r#"{"shape":"polygon","points":[[0,0],[1,1]],"fill":[0,0,0,255]}"#), "\"points\" holds 3..=1024 points");
        let many = std::iter::repeat_n("[0,0]", MAX_POLYGON_POINTS + 1).collect::<Vec<_>>().join(",");
        refuses(&base(&format!(r#"{{"shape":"polygon","points":[{many}],"fill":[0,0,0,255]}}"#)), "\"points\" holds 3..=1024 points");
        refuses(&base(r#"{"shape":"polygon","points":[[0,0],[1,1],[2]],"fill":[0,0,0,255]}"#), "\"points\"[2] must be [x,y]");
        refuses(&base(r#"{"shape":"polygon","points":[[0,0],[1,1,1],[2,2]],"fill":[0,0,0,255]}"#), "\"points\"[1] must be [x,y]");
        refuses(&base(r#"{"shape":"polygon","points":[[0,0],[1,"1"],[2,2]],"fill":[0,0,0,255]}"#), "\"points\"[1] must be [x,y]");
        refuses(&base(r#"{"shape":"polygon","points":[[0,0],[1,1],[2,1048577]],"fill":[0,0,0,255]}"#), "\"points\"[2] must be [x,y]");
        refuses(
            &base(r#"{"shape":"polygon","points":[[0,0],[1,1],[2,2]],"fill":[0,0,0,255],"closed":true}"#),
            "(polygon): unknown key \"closed\"",
        );
        refuses(&base(r#"{"shape":"line","x0":0,"y0":0,"x1":1,"fill":[0,0,0,255]}"#), "(line): missing key \"y1\"");
        refuses(
            &base(r#"{"shape":"line","x0":0,"y0":0,"x1":1,"y1":1,"fill":[0,0,0,255],"width":2}"#),
            "(line): unknown key \"width\"",
        );
        refuses(&base(r#"{"shape":"line","x0":0,"y0":0,"x1":-1048577,"y1":1,"fill":[0,0,0,255]}"#), "\"x1\" must be an integer in");
        // the row names the layer index
        refuses(
            &base(r#"{"shape":"rect","x":0,"y":0,"w":1,"h":1,"fill":[0,0,0,255]},{"shape":"circle"}"#),
            "layers[1] (circle): missing key",
        );
    }

    #[test]
    fn transformer_refuses_non_canonical_and_invalid_input() {
        let canonical = canon(&base("")).unwrap();
        assert!(ImagePngTransformer.run(&canonical).is_ok());
        let spaced = format!("{} ", std::str::from_utf8(&canonical).unwrap());
        match ImagePngTransformer.run(spaced.as_bytes()) {
            Err(DeriveError::Transformer(m)) => assert!(m.contains("not canonical"), "{m}"),
            other => panic!("{other:?}"),
        }
        let reordered = r#"{"v":1,"background":[0,0,255,255],"height":4,"layers":[],"width":4}"#;
        assert!(matches!(ImagePngTransformer.run(reordered.as_bytes()), Err(DeriveError::Transformer(_))));
        assert!(matches!(ImagePngTransformer.run(b"{\"v\":2}"), Err(DeriveError::Grammar(_))));
        assert!(matches!(ImagePngTransformer.run(b"nope"), Err(DeriveError::Grammar(_))));
    }

    // ----- (3) determinism -------------------------------------------------------------------

    #[test]
    fn same_dsl_twice_and_every_spelling_give_identical_bytes() {
        let a = r#"{"v":1,"width":8,"height":6,"background":[10,20,30,255],"layers":[{"shape":"circle","cx":3,"cy":3,"r":2,"fill":[200,100,0,128]},{"shape":"line","x0":0,"y0":5,"x1":7,"y1":0,"fill":[0,0,0,255]}]}"#;
        let b = r#" { "layers": [ {"r": 2, "cy": 3, "cx": 3, "fill": [200, 100, 0, 128], "shape": "circle"},
                      {"fill": [0,0,0,255], "y1": 0, "x1": 7, "y0": 5, "x0": 0, "shape": "line"} ],
                    "background": [10, 20, 30, 255], "height": 6, "width": 8, "v": 1 } "#;
        let ca = canon(a).unwrap();
        let cb = canon(b).unwrap();
        assert_eq!(ca, cb, "whitespace and key order do not reach the canonical bytes");
        let p1 = png_of(a);
        let p2 = png_of(a);
        let p3 = png_of(b);
        assert_eq!(p1, p2);
        assert_eq!(p1, p3);
        assert_eq!(p1.len() as u64, png_stored_size(8, 6));
    }

    // ----- (4) structural PNG --------------------------------------------------------------

    #[test]
    fn png_structure_ihdr_crcs_zlib_and_filter_bytes() {
        let png = png_of(r#"{"v":1,"width":5,"height":3,"background":[1,2,3,4],"layers":[]}"#);
        let (w, h, rgba) = decode(&png);
        assert_eq!((w, h), (5, 3));
        assert_eq!(rgba.len(), 5 * 3 * 4);
        assert!(rgba.as_chunks::<4>().0.iter().all(|p| *p == [1, 2, 3, 4]));
        // the exact bytes of the head: signature, IHDR length, type, fields
        assert_eq!(&png[..8], &PNG_SIGNATURE);
        assert_eq!(&png[8..12], &[0, 0, 0, 13]);
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(&png[16..29], &[0, 0, 0, 5, 0, 0, 0, 3, 8, 6, 0, 0, 0]);
        // IDAT payload is exactly zlib_stored(raw)
        let stride = 1 + 5 * 4;
        let mut row = vec![0u8];
        row.extend_from_slice(&[1, 2, 3, 4].repeat(5));
        let raw = row.repeat(3);
        assert_eq!(raw.len(), stride * 3);
        let chunks = parse_png(&png);
        assert_eq!(chunks[1].data, zlib_stored(&raw));
        // the tail: IEND with its fixed CRC
        assert_eq!(&png[png.len() - 12..], &[0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82]);
    }

    #[test]
    fn idat_splits_into_several_stored_blocks_above_65535_raw_bytes() {
        // 256x80 RGBA: raw = 80 * (1 + 1024) = 82,000 bytes > 65,535 → two stored blocks
        let png = png_of(r#"{"v":1,"width":256,"height":80,"background":[9,9,9,9],"layers":[]}"#);
        let (w, h, rgba) = decode(&png);
        assert_eq!((w, h), (256, 80));
        assert!(rgba.as_chunks::<4>().0.iter().all(|p| *p == [9, 9, 9, 9]));
        assert_eq!(png_stored_size(256, 80), 8 + 25 + 12 + (2 + 82_000 + 5 * 2 + 4) + 12);
    }

    #[test]
    fn artifact_ceiling_is_a_function_of_the_dimensions() {
        assert!(png_stored_size(2048, 2048) <= ARTIFACT_MAX_BYTES, "the largest admissible canvas fits");
        assert!(png_stored_size(4096, 4096) > ARTIFACT_MAX_BYTES, "a canvas the grammar already refuses would not");
        assert_eq!(png_stored_size(1, 1), 8 + 25 + 12 + (2 + 5 + 5 + 4) + 12);
    }

    // ----- (5) pixels ------------------------------------------------------------------------

    #[test]
    fn mix_is_exact_at_both_ends_and_matches_the_hand_computation() {
        for s in 0..=255u8 {
            for d in 0..=255u8 {
                assert_eq!(mix(s, d, 255), s);
                assert_eq!(mix(s, d, 0), d);
            }
        }
        // [200,100,0,128] over opaque blue [0,0,255,255], by hand:
        //   r: (200*128 + 0*127 + 127)/255 = 25727/255 = 100 r227
        //   g: (100*128 + 0*127 + 127)/255 = 12927/255 = 50  r177
        //   b: (0*128 + 255*127 + 127)/255 = 32512/255 = 127 r127
        //   a: (255*128 + 255*127 + 127)/255 = 65152/255 = 255 r127
        assert_eq!(mix(200, 0, 128), 100);
        assert_eq!(mix(100, 0, 128), 50);
        assert_eq!(mix(0, 255, 128), 127);
        assert_eq!(mix(255, 255, 128), 255);
        // half-transparent white over fully transparent black: colour 128, alpha 128
        assert_eq!(mix(255, 0, 128), 128);
    }

    #[test]
    fn floor_div_rounds_toward_negative_infinity() {
        assert_eq!(floor_div(7, 2), 3);
        assert_eq!(floor_div(-7, 2), -4);
        assert_eq!(floor_div(7, -2), -4);
        assert_eq!(floor_div(-7, -2), 3);
        assert_eq!(floor_div(6, 3), 2);
        assert_eq!(floor_div(-6, 3), -2);
        assert_eq!(floor_div(6, -3), -2);
        assert_eq!(floor_div(-6, -3), 2);
        assert_eq!(floor_div(0, -5), 0);
        assert_eq!(floor_div(-1, 3), -1);
        assert_eq!(floor_div(1, -3), -1);
        assert_eq!(floor_div(-3, 2), -2);
        assert_eq!(floor_div(i64::MIN + 1, 2), -(1i64 << 62));
    }

    #[test]
    fn four_by_four_rect_and_circle_with_a_hand_composited_overlap() {
        // background opaque blue; rect (0,0) 2x2 opaque red; circle r=1 at (1,1) half-transparent
        // orange [200,100,0,128]. The circle covers (1,1),(0,1),(2,1),(1,0),(1,2).
        let png = png_of(&base(
            r#"{"shape":"rect","x":0,"y":0,"w":2,"h":2,"fill":[255,0,0,255]},{"shape":"circle","cx":1,"cy":1,"r":1,"fill":[200,100,0,128]}"#,
        ));
        let img = decode(&png);
        // orange over red:  r (200*128+255*127+127)/255 = 58112/255 = 227; g 50; b 0; a 255
        let over_red: Rgba = [227, 50, 0, 255];
        // orange over blue: r 100; g 50; b 127; a 255 (the hand computation in `mix`'s test)
        let over_blue: Rgba = [100, 50, 127, 255];
        let expected: [[Rgba; 4]; 4] = [
            [RED, over_red, BLUE, BLUE],
            [over_red, over_red, over_blue, BLUE],
            [BLUE, over_blue, BLUE, BLUE],
            [BLUE, BLUE, BLUE, BLUE],
        ];
        for (y, row) in expected.iter().enumerate() {
            for (x, want) in row.iter().enumerate() {
                assert_eq!(px(&img, x as u32, y as u32), *want, "pixel ({x},{y})");
            }
        }
    }

    #[test]
    fn translucent_fill_over_a_transparent_background_keeps_the_alpha_rule() {
        let png = png_of(
            r#"{"v":1,"width":2,"height":1,"background":[0,0,0,0],"layers":[{"shape":"rect","x":0,"y":0,"w":1,"h":1,"fill":[255,255,255,128]}]}"#,
        );
        let img = decode(&png);
        assert_eq!(px(&img, 0, 0), [128, 128, 128, 128]);
        assert_eq!(px(&img, 1, 0), [0, 0, 0, 0]);
    }

    #[test]
    fn triangle_uses_the_half_open_scanline_rule_with_floor_division() {
        // (0,0),(3,0),(0,2): row 0 crossings [0,3) → 0,1,2; row 1: edge (3,0)-(0,2) crosses at
        // 3 + floor(1*(-3)/2) = 3 + (-2) = 1 (truncation would give 2) → [0,1) → pixel 0 only.
        let png = png_of(&base(r#"{"shape":"polygon","points":[[0,0],[3,0],[0,2]],"fill":[255,0,0,255]}"#));
        let img = decode(&png);
        assert_eq!(painted(&img, BLUE), vec![(0, 0), (1, 0), (2, 0), (0, 1)]);
        // the same triangle listed clockwise gives the same pixels
        let png = png_of(&base(r#"{"shape":"polygon","points":[[0,2],[3,0],[0,0]],"fill":[255,0,0,255]}"#));
        assert_eq!(painted(&decode(&png), BLUE), vec![(0, 0), (1, 0), (2, 0), (0, 1)]);
        // right triangle (0,0),(3,0),(0,3): rows 0..3 with spans [0,3),[0,2),[0,1)
        let png = png_of(&base(r#"{"shape":"polygon","points":[[0,0],[3,0],[0,3]],"fill":[255,0,0,255]}"#));
        assert_eq!(painted(&decode(&png), BLUE), vec![(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (0, 2)]);
    }

    #[test]
    fn diamond_and_off_canvas_polygons() {
        // diamond (2,0),(4,2),(2,4),(0,2) on 5x5: the apex row's span is empty (half-open),
        // rows 1..=3 hold [1,3), [0,4), [1,3).
        let dsl = r#"{"v":1,"width":5,"height":5,"background":[0,0,255,255],"layers":[{"shape":"polygon","points":[[2,0],[4,2],[2,4],[0,2]],"fill":[255,0,0,255]}]}"#;
        let img = decode(&png_of(dsl));
        assert_eq!(painted(&img, BLUE), vec![(1, 1), (2, 1), (0, 2), (1, 2), (2, 2), (3, 2), (1, 3), (2, 3)]);
        // a square straddling the origin is clipped to the canvas
        let png = png_of(&base(r#"{"shape":"polygon","points":[[-2,-2],[2,-2],[2,2],[-2,2]],"fill":[255,0,0,255]}"#));
        assert_eq!(painted(&decode(&png), BLUE), vec![(0, 0), (1, 0), (0, 1), (1, 1)]);
        // a polygon entirely off the canvas paints nothing; so does a degenerate (collinear) one
        let png = png_of(&base(r#"{"shape":"polygon","points":[[10,10],[12,10],[11,12]],"fill":[255,0,0,255]}"#));
        assert!(painted(&decode(&png), BLUE).is_empty());
        let png = png_of(&base(r#"{"shape":"polygon","points":[[0,1],[3,1],[1,1]],"fill":[255,0,0,255]}"#));
        assert!(painted(&decode(&png), BLUE).is_empty());
        // a self-intersecting bow-tie under even-odd: (0,0),(3,3),(3,0),(0,3). Every edge
        // spans rows 0..3; per row the four crossings are (0,0)-(3,3) → py, (3,3)-(3,0) → 3,
        // (3,0)-(0,3) → 3 - py, (0,3)-(0,0) → 0:
        //   row 0: [0,0,3,3] → spans [0,0) and [3,3): nothing
        //   row 1: [0,1,2,3] → [0,1) and [2,3): pixels 0 and 2
        //   row 2: [0,1,2,3] → pixels 0 and 2
        let png = png_of(&base(r#"{"shape":"polygon","points":[[0,0],[3,3],[3,0],[0,3]],"fill":[255,0,0,255]}"#));
        assert_eq!(painted(&decode(&png), BLUE), vec![(0, 1), (2, 1), (0, 2), (2, 2)]);
    }

    #[test]
    fn lines_are_bresenham_one_pixel_wide() {
        let diagonal = png_of(&base(r#"{"shape":"line","x0":0,"y0":0,"x1":3,"y1":3,"fill":[255,0,0,255]}"#));
        assert_eq!(painted(&decode(&diagonal), BLUE), vec![(0, 0), (1, 1), (2, 2), (3, 3)]);
        let anti = png_of(&base(r#"{"shape":"line","x0":3,"y0":0,"x1":0,"y1":3,"fill":[255,0,0,255]}"#));
        assert_eq!(painted(&decode(&anti), BLUE), vec![(3, 0), (2, 1), (1, 2), (0, 3)]);
        let shallow = png_of(&base(r#"{"shape":"line","x0":0,"y0":0,"x1":3,"y1":1,"fill":[255,0,0,255]}"#));
        assert_eq!(painted(&decode(&shallow), BLUE), vec![(0, 0), (1, 0), (2, 1), (3, 1)]);
        let steep = png_of(&base(r#"{"shape":"line","x0":0,"y0":0,"x1":1,"y1":3,"fill":[255,0,0,255]}"#));
        assert_eq!(painted(&decode(&steep), BLUE), vec![(0, 0), (0, 1), (1, 2), (1, 3)]);
        let point = png_of(&base(r#"{"shape":"line","x0":2,"y0":2,"x1":2,"y1":2,"fill":[255,0,0,255]}"#));
        assert_eq!(painted(&decode(&point), BLUE), vec![(2, 2)]);
        // a horizontal line that starts and ends off the canvas paints the whole row once
        let across = png_of(&base(r#"{"shape":"line","x0":-9,"y0":1,"x1":9,"y1":1,"fill":[255,255,255,128]}"#));
        let img = decode(&across);
        assert_eq!(painted(&img, BLUE), vec![(0, 1), (1, 1), (2, 1), (3, 1)]);
        assert_eq!(px(&img, 0, 1), [128, 128, mix(255, 255, 128), 255]);
        // one that misses the canvas entirely
        let miss = png_of(&base(r#"{"shape":"line","x0":-1048576,"y0":-1048576,"x1":-1,"y1":1048576,"fill":[255,0,0,255]}"#));
        assert!(painted(&decode(&miss), BLUE).is_empty());
    }

    #[test]
    fn circles_and_rects_clip_and_composite_once_per_pixel() {
        // a circle centred off-canvas, and a rect from the far negative corner
        let png = png_of(&base(
            r#"{"shape":"circle","cx":-1,"cy":-1,"r":2,"fill":[255,0,0,255]},{"shape":"rect","x":-1048576,"y":2,"w":2097152,"h":1,"fill":[255,255,255,128]}"#,
        ));
        let img = decode(&png);
        // (px+1)^2 + (py+1)^2 <= 4 → (0,0) [2], (1,0) [5 > 4: no], (0,1) [5: no] → only (0,0)
        assert_eq!(px(&img, 0, 0), RED);
        assert_eq!(px(&img, 1, 0), BLUE);
        assert_eq!(px(&img, 0, 1), BLUE);
        // row 2 was blended exactly once: mix(255, 0, 128) = 128, mix(255, 255, 128) = 255
        for x in 0..4 {
            assert_eq!(px(&img, x, 2), [128, 128, 255, 255]);
        }
        assert_eq!(px(&img, 0, 3), BLUE);
        // a large circle covers the whole canvas
        let png = png_of(&base(r#"{"shape":"circle","cx":1,"cy":1,"r":4096,"fill":[255,0,0,255]}"#));
        assert_eq!(painted(&decode(&png), BLUE).len(), 16);
        // an empty layer list is the background alone
        let png = png_of(&base(""));
        assert!(painted(&decode(&png), BLUE).is_empty());
    }

    // ----- (6) the corpus and its golden -----------------------------------------------------

    fn corpus_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("corpus").join("image")
    }

    fn corpus_files() -> Vec<(String, Vec<u8>)> {
        let mut files: Vec<(String, Vec<u8>)> = std::fs::read_dir(corpus_dir())
            .expect("corpus/image exists")
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
            .filter(|p| p.file_name().and_then(|n| n.to_str()) != Some("golden.json"))
            .map(|p| (p.file_name().unwrap().to_str().unwrap().to_string(), std::fs::read(&p).unwrap()))
            .collect();
        files.sort();
        assert!(files.len() >= 4, "at least four corpus samples");
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

    #[test]
    fn corpus_matches_golden_and_verifies_through_the_registry() {
        let golden: serde_json::Value =
            serde_json::from_slice(&std::fs::read(corpus_dir().join("golden.json")).expect("golden.json")).unwrap();
        let grammar_id = grammar_id_v1(GRAMMAR_NAME);
        for (name, answer) in corpus_files() {
            let d = derive_with(&ImageGrammar, &ImagePngTransformer, &binding(), &answer).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(d.kind, kind::IMAGE);
            assert_eq!(d.grammar_id, grammar_id);
            assert_eq!(d.dsl_hash, dsl_hash_v1(&grammar_id, &d.canonical_dsl));
            assert_eq!(d.artifact_hash, artifact_hash_v1(&d.artifact.bytes));
            assert_eq!(d.artifact.media_type, "image/png");
            assert_eq!(d.artifact.extension, "png");
            let (w, h, _) = decode(&d.artifact.bytes);
            assert_eq!(d.artifact.bytes.len() as u64, png_stored_size(w, h));
            let g = golden.get(&name).unwrap_or_else(|| panic!("{name} has no golden entry"));
            assert_eq!(g["dsl_hash"].as_str().unwrap(), d.dsl_hash.to_string(), "{name}: dsl_hash");
            assert_eq!(g["artifact_hash"].as_str().unwrap(), d.artifact_hash.to_string(), "{name}: artifact_hash");
            assert_eq!(g["artifact_bytes"].as_u64().unwrap(), d.artifact.bytes.len() as u64, "{name}: artifact_bytes");
            // X6: the consumer's path, through the registry, from the answer and the object alone
            let v = verify(&d.object, &answer).unwrap();
            assert!(v.all_match(), "{name}: {v:?}");
            // and the canonical bytes are a fixed point of the grammar
            assert_eq!(ImageGrammar.canonicalize(&d.canonical_dsl).unwrap(), d.canonical_dsl);
        }
        assert_eq!(golden.as_object().unwrap().len(), corpus_files().len(), "no stale golden entries");
    }

    /// Regenerate the golden: `cargo test -p misaka-palw-derive print_image_golden -- --ignored
    /// --nocapture`, then paste. Ignored because pinning is a decision, not a test. With
    /// `PALW_DERIVE_IMAGE_DUMP_DIR` set, the artifacts are also written there, to be looked at.
    #[test]
    #[ignore]
    fn print_image_golden() {
        let dump_dir = std::env::var_os("PALW_DERIVE_IMAGE_DUMP_DIR").map(PathBuf::from);
        let mut out = serde_json::Map::new();
        for (name, answer) in corpus_files() {
            let d = derive_with(&ImageGrammar, &ImagePngTransformer, &binding(), &answer).unwrap();
            if let Some(dir) = &dump_dir {
                std::fs::write(dir.join(format!("{name}.png")), &d.artifact.bytes).unwrap();
            }
            out.insert(
                name,
                serde_json::json!({
                    "dsl_hash": d.dsl_hash.to_string(),
                    "artifact_hash": d.artifact_hash.to_string(),
                    "artifact_bytes": d.artifact.bytes.len(),
                }),
            );
        }
        println!("{}", serde_json::to_string_pretty(&serde_json::Value::Object(out)).unwrap());
    }

    // ----- (7) the discipline, scanned -------------------------------------------------------

    #[test]
    fn no_floating_point_type_is_spelled_in_this_file() {
        let src = include_str!("image.rs");
        assert!(!src.contains(concat!("f", "64")));
        assert!(!src.contains(concat!("f", "32")));
        assert!(!src.contains(concat!("Hash", "Map")));
    }

    #[test]
    fn manifest_names_this_build_and_the_registry_finds_it() {
        let m = ImagePngTransformer.manifest();
        assert_eq!(m.name, "image/png/v1");
        assert_eq!(m.kind, kind::IMAGE);
        assert_eq!(m.grammar, "image/v1");
        assert_eq!(m.discipline, Discipline::Integer);
        assert_eq!(m.writer, "png/1.2/rgba8-filter0-stored-v1");
        assert_eq!(m.source_tree_sha256, crate::SOURCE_TREE_SHA256_HEX);
        assert!(crate::registry::transformer_by_name("image/png/v1").is_some());
        assert!(crate::registry::grammar_by_name("image/v1").is_some());
    }
}
