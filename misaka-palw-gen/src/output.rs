//! **RFC-0003 §I.3 — canonical outputs.**
//!
//! The consensus output of a job is the value of the class's output node — a committed integer
//! tensor — serialised in the canonical byte form of its kind. Its digest is part of the claim;
//! everything a user sees beyond those bytes (PNG, WAV, MP4, UTF-8 rendering) is presentation.
//!
//! ```text
//!   canonical bytes B = the node's elements, row-major, each in the kind's element encoding
//!   tile t            = the bytes of lanes [t·tile_len, min((t+1)·tile_len, E))   — the node's step tiles
//!   leaf_t            = H64(key "misaka-palw/output/tile/v1", le32(t) ‖ bytes of tile t)
//!   node              = H64(key "misaka-palw/output/node/v1", left ‖ right)       — an odd node is PROMOTED
//!   output_root       = H64(key "misaka-palw/output/root/v1", borsh(OutputSpecV1) ‖ le32(tile_len) ‖ merkle_root)
//! ```
//!
//! The tree follows the chain's house rule (`palw_step_leg`, `palw_artifact`): leaves bound to their
//! index, keyed interior nodes, and an odd node promoted, never duplicated (duplication lets a tree
//! over `[a, b, c]` and one over `[a, b, c, c]` share a root). `tile_len` is inside the root, so a
//! root names its own tiling. A step tile and the output tile at the same index can therefore be
//! compared with two openings — the `TirOutputDigestMismatch` fault of RFC-0003 §I.3.2.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::rand::keyed64;

pub const OUTPUT_TILE_KEY_V1: &[u8] = b"misaka-palw/output/tile/v1";
pub const OUTPUT_NODE_KEY_V1: &[u8] = b"misaka-palw/output/node/v1";
pub const OUTPUT_ROOT_KEY_V1: &[u8] = b"misaka-palw/output/root/v1";

/// The step tiles' bounds (`PALW_STEP_MIN_TILE_LEN`, `PALW_STEP_MAX_TILE_LEN`): output tiles are the
/// output node's step tiles.
pub const OUTPUT_MIN_TILE_LEN_V1: u32 = 4;
pub const OUTPUT_MAX_TILE_LEN_V1: u32 = 1 << 16;
/// A PALW-TIR tensor holds at most `2^28` elements, and a dimension is at most `2^24`.
pub const OUTPUT_MAX_ELEMENTS_V1: u64 = 1 << 28;
pub const OUTPUT_MAX_DIM_V1: u32 = 1 << 24;

/// The output kinds (RFC-0003 §I.3.3). Tags are frozen; `ImageRgb8 = 1` is the ImageJob's
/// `output` field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum OutputKindV1 {
    /// Generated token ids, `u32` LE. (Text's consensus form is the FP commitment's ids; no text
    /// claim carries an output root.)
    Tokens = 0,
    /// Raw `u8` HWC, RGB: shape `[H, W, 3]`.
    ImageRgb8 = 1,
    /// PCM `i16` LE, interleaved by channel: shape `[frames, channels]`, meta `le32(sample_rate)`.
    PcmI16 = 2,
    /// `i32` LE `[n, d]` in the class's fixed point: meta `[q, normalised]`.
    EmbeddingI32 = 3,
    /// Raw `u8` THWC: shape `[T, H, W, 3]`, meta `le32(fps_num) ‖ le32(fps_den)`.
    VideoRgb8 = 4,
    /// A raw tensor in its own dtype (a latent-only output): meta `[dtype tag, q]`, dtype tags as
    /// PALW-TIR's (`0` i8, `1` i16, `2` i32).
    TensorLe = 5,
}

impl OutputKindV1 {
    pub const ALL: [OutputKindV1; 6] = [
        OutputKindV1::Tokens,
        OutputKindV1::ImageRgb8,
        OutputKindV1::PcmI16,
        OutputKindV1::EmbeddingI32,
        OutputKindV1::VideoRgb8,
        OutputKindV1::TensorLe,
    ];

    pub const fn tag(self) -> u8 {
        self as u8
    }

    pub fn from_tag(tag: u8) -> Option<Self> {
        Self::ALL.iter().copied().find(|k| k.tag() == tag)
    }

    pub const fn name(self) -> &'static str {
        match self {
            OutputKindV1::Tokens => "Tokens",
            OutputKindV1::ImageRgb8 => "ImageRgb8",
            OutputKindV1::PcmI16 => "PcmI16",
            OutputKindV1::EmbeddingI32 => "EmbeddingI32",
            OutputKindV1::VideoRgb8 => "VideoRgb8",
            OutputKindV1::TensorLe => "TensorLe",
        }
    }
}

/// The output header: kind, shape and kind-specific metadata. Its Borsh bytes are inside the root.
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct OutputSpecV1 {
    pub kind: u8,
    pub shape: Vec<u32>,
    pub meta: Vec<u8>,
}

/// Why an output was refused. Every refusal is a value, never a panic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutputErrorV1 {
    UnknownKind(u8),
    /// The shape does not fit the kind (rank, a fixed extent, a zero or oversized dimension).
    Shape(String),
    /// The metadata does not fit the kind.
    Meta(String),
    /// The number of values is not the shape's element count.
    Count {
        want: u64,
        got: u64,
    },
    /// A value outside the kind's value domain (a pixel above 255, a sample outside i16, …).
    Domain {
        index: u64,
        value: i64,
    },
    /// `tile_len` outside `[4, 2^16]`.
    TileLen(u32),
}

/// What the kind's element encoding is: bytes per element and the value domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputLayoutV1 {
    pub elements: u64,
    pub element_bytes: usize,
    pub lo: i64,
    pub hi: i64,
    /// `true` for `Tokens` (unsigned `u32`); every other kind is two's complement or `u8`.
    pub unsigned: bool,
}

impl OutputSpecV1 {
    pub fn tokens(n: u32) -> Self {
        Self { kind: OutputKindV1::Tokens.tag(), shape: vec![n], meta: vec![] }
    }
    pub fn image_rgb8(h: u32, w: u32) -> Self {
        Self { kind: OutputKindV1::ImageRgb8.tag(), shape: vec![h, w, 3], meta: vec![] }
    }
    pub fn pcm_i16(frames: u32, channels: u32, sample_rate: u32) -> Self {
        Self { kind: OutputKindV1::PcmI16.tag(), shape: vec![frames, channels], meta: sample_rate.to_le_bytes().to_vec() }
    }
    pub fn embedding_i32(n: u32, d: u32, q: u8, normalised: bool) -> Self {
        Self { kind: OutputKindV1::EmbeddingI32.tag(), shape: vec![n, d], meta: vec![q, normalised as u8] }
    }
    pub fn video_rgb8(t: u32, h: u32, w: u32, fps_num: u32, fps_den: u32) -> Self {
        let mut meta = fps_num.to_le_bytes().to_vec();
        meta.extend_from_slice(&fps_den.to_le_bytes());
        Self { kind: OutputKindV1::VideoRgb8.tag(), shape: vec![t, h, w, 3], meta }
    }
    /// `dtype_tag`: PALW-TIR's `0` i8, `1` i16, `2` i32.
    pub fn tensor_le(dtype_tag: u8, shape: Vec<u32>, q: u8) -> Self {
        Self { kind: OutputKindV1::TensorLe.tag(), shape, meta: vec![dtype_tag, q] }
    }

    pub fn encode(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("encoding into a Vec cannot fail")
    }

    /// Check the header against its kind and return the element encoding.
    pub fn layout(&self) -> Result<OutputLayoutV1, OutputErrorV1> {
        let kind = OutputKindV1::from_tag(self.kind).ok_or(OutputErrorV1::UnknownKind(self.kind))?;
        let shape_err = |m: &str| Err(OutputErrorV1::Shape(format!("{}: {m}", kind.name())));
        let meta_err = |m: &str| Err(OutputErrorV1::Meta(format!("{}: {m}", kind.name())));
        if self.shape.is_empty() || self.shape.len() > 4 {
            return shape_err("rank 1..=4");
        }
        if self.shape.iter().any(|d| *d == 0 || *d > OUTPUT_MAX_DIM_V1) {
            return shape_err("every dimension is in [1, 2^24]");
        }
        let elements = self.shape.iter().map(|d| *d as u64).product::<u64>();
        if elements > OUTPUT_MAX_ELEMENTS_V1 {
            return shape_err("more than 2^28 elements");
        }
        let (rank, last_is_3) = (self.shape.len(), self.shape.last() == Some(&3));
        let (element_bytes, lo, hi, unsigned) = match kind {
            OutputKindV1::Tokens => {
                if rank != 1 {
                    return shape_err("shape [n]");
                }
                if !self.meta.is_empty() {
                    return meta_err("no metadata");
                }
                (4, 0, u32::MAX as i64, true)
            }
            OutputKindV1::ImageRgb8 => {
                if rank != 3 || !last_is_3 {
                    return shape_err("shape [H, W, 3]");
                }
                if !self.meta.is_empty() {
                    return meta_err("no metadata");
                }
                (1, 0, 255, false)
            }
            OutputKindV1::PcmI16 => {
                if rank != 2 {
                    return shape_err("shape [frames, channels]");
                }
                if self.meta.len() != 4 || u32::from_le_bytes([self.meta[0], self.meta[1], self.meta[2], self.meta[3]]) == 0 {
                    return meta_err("meta is le32(sample_rate), sample_rate ≥ 1");
                }
                (2, i16::MIN as i64, i16::MAX as i64, false)
            }
            OutputKindV1::EmbeddingI32 => {
                if rank != 2 {
                    return shape_err("shape [n, d]");
                }
                if self.meta.len() != 2 || self.meta[0] > 31 || self.meta[1] > 1 {
                    return meta_err("meta is [q ≤ 31, normalised ∈ {0, 1}]");
                }
                (4, i32::MIN as i64, i32::MAX as i64, false)
            }
            OutputKindV1::VideoRgb8 => {
                if rank != 4 || !last_is_3 {
                    return shape_err("shape [T, H, W, 3]");
                }
                let num = self.meta.get(0..4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
                let den = self.meta.get(4..8).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
                if self.meta.len() != 8 || num == Some(0) || den == Some(0) {
                    return meta_err("meta is le32(fps_num) ‖ le32(fps_den), both ≥ 1");
                }
                (1, 0, 255, false)
            }
            OutputKindV1::TensorLe => {
                if self.meta.len() != 2 || self.meta[1] > 62 {
                    return meta_err("meta is [dtype tag, q ≤ 62]");
                }
                match self.meta[0] {
                    0 => (1, i8::MIN as i64, i8::MAX as i64, false),
                    1 => (2, i16::MIN as i64, i16::MAX as i64, false),
                    2 => (4, i32::MIN as i64, i32::MAX as i64, false),
                    t => return meta_err(&format!("dtype tag {t} is not i8 (0), i16 (1) or i32 (2)")),
                }
            }
        };
        Ok(OutputLayoutV1 { elements, element_bytes, lo, hi, unsigned })
    }

    /// **The canonical bytes**: every value in the kind's domain, in the kind's element encoding,
    /// row-major.
    pub fn canonical_bytes(&self, values: &[i64]) -> Result<Vec<u8>, OutputErrorV1> {
        let l = self.layout()?;
        if values.len() as u64 != l.elements {
            return Err(OutputErrorV1::Count { want: l.elements, got: values.len() as u64 });
        }
        self.lane_bytes(values)
    }

    /// **The canonical bytes of a run of elements** — a tile's lanes, say — in the kind's element
    /// encoding; a value outside the kind's domain is refused at its index within the run. The whole
    /// output's bytes are this over every element ([`Self::canonical_bytes`]).
    pub fn lane_bytes(&self, values: &[i64]) -> Result<Vec<u8>, OutputErrorV1> {
        let l = self.layout()?;
        let mut out = Vec::with_capacity(values.len() * l.element_bytes);
        for (i, v) in values.iter().enumerate() {
            if *v < l.lo || *v > l.hi {
                return Err(OutputErrorV1::Domain { index: i as u64, value: *v });
            }
            let le = if l.unsigned { (*v as u64).to_le_bytes() } else { (*v).to_le_bytes() };
            out.extend_from_slice(&le[..l.element_bytes]);
        }
        Ok(out)
    }
}

/// `leaf_t = H64(key tile, le32(t) ‖ bytes)`.
pub fn output_tile_leaf_v1(t: u32, tile_bytes: &[u8]) -> [u8; 64] {
    keyed64(OUTPUT_TILE_KEY_V1, &[&t.to_le_bytes(), tile_bytes])
}

fn node(left: &[u8; 64], right: &[u8; 64]) -> [u8; 64] {
    keyed64(OUTPUT_NODE_KEY_V1, &[left, right])
}

/// The Merkle root over ordered leaves; an odd node is promoted. `None` for no leaves.
pub fn output_merkle_root_v1(leaves: &[[u8; 64]]) -> Option<[u8; 64]> {
    if leaves.is_empty() {
        return None;
    }
    let mut level = leaves.to_vec();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut pairs = level.chunks_exact(2);
        for p in &mut pairs {
            next.push(node(&p[0], &p[1]));
        }
        if let [odd] = pairs.remainder() {
            next.push(*odd);
        }
        level = next;
    }
    Some(level[0])
}

fn check_tile_len(tile_len: u32) -> Result<(), OutputErrorV1> {
    if !(OUTPUT_MIN_TILE_LEN_V1..=OUTPUT_MAX_TILE_LEN_V1).contains(&tile_len) {
        return Err(OutputErrorV1::TileLen(tile_len));
    }
    Ok(())
}

/// The canonical bytes cut at the output node's step tiles.
pub fn output_tiles_v1(spec: &OutputSpecV1, values: &[i64], tile_len: u32) -> Result<Vec<Vec<u8>>, OutputErrorV1> {
    check_tile_len(tile_len)?;
    let l = spec.layout()?;
    let bytes = spec.canonical_bytes(values)?;
    Ok(bytes.chunks(tile_len as usize * l.element_bytes).map(|c| c.to_vec()).collect())
}

/// The leaves of the output tree.
pub fn output_leaves_v1(spec: &OutputSpecV1, values: &[i64], tile_len: u32) -> Result<Vec<[u8; 64]>, OutputErrorV1> {
    Ok(output_tiles_v1(spec, values, tile_len)?.iter().enumerate().map(|(t, b)| output_tile_leaf_v1(t as u32, b)).collect())
}

fn root_from_merkle(spec: &OutputSpecV1, tile_len: u32, merkle: &[u8; 64]) -> [u8; 64] {
    keyed64(OUTPUT_ROOT_KEY_V1, &[&spec.encode(), &tile_len.to_le_bytes(), merkle])
}

/// **`output_root`** of a tensor output.
pub fn output_root_v1(spec: &OutputSpecV1, values: &[i64], tile_len: u32) -> Result<[u8; 64], OutputErrorV1> {
    let leaves = output_leaves_v1(spec, values, tile_len)?;
    let merkle = output_merkle_root_v1(&leaves).expect("a shape with dimensions ≥ 1 has at least one tile");
    Ok(root_from_merkle(spec, tile_len, &merkle))
}

/// The number of tiles of an output: `⌈E / tile_len⌉`.
pub fn output_tile_count_v1(spec: &OutputSpecV1, tile_len: u32) -> Result<u64, OutputErrorV1> {
    check_tile_len(tile_len)?;
    Ok(spec.layout()?.elements.div_ceil(tile_len as u64))
}

/// The authentication path of leaf `t`: its siblings, bottom-up. A level where the node is the
/// promoted odd one contributes nothing. `None` when `t` is out of range.
pub fn output_tile_proof_v1(leaves: &[[u8; 64]], t: usize) -> Option<Vec<[u8; 64]>> {
    if t >= leaves.len() {
        return None;
    }
    let mut path = Vec::new();
    let mut level = leaves.to_vec();
    let mut i = t;
    while level.len() > 1 {
        let sibling = i ^ 1;
        if sibling < level.len() {
            path.push(level[sibling]);
        }
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut pairs = level.chunks_exact(2);
        for p in &mut pairs {
            next.push(node(&p[0], &p[1]));
        }
        if let [odd] = pairs.remainder() {
            next.push(*odd);
        }
        level = next;
        i /= 2;
    }
    Some(path)
}

/// **Verify one output tile against `output_root`**, as the court's consistency check will: the
/// tile's length is the one the spec and `tile_len` imply, and its path leads to the root.
pub fn verify_output_tile_v1(
    root: &[u8; 64],
    spec: &OutputSpecV1,
    tile_len: u32,
    t: u64,
    tile_bytes: &[u8],
    proof: &[[u8; 64]],
) -> bool {
    let (Ok(l), Ok(n)) = (spec.layout(), output_tile_count_v1(spec, tile_len)) else { return false };
    if t >= n || t > u32::MAX as u64 {
        return false;
    }
    let lanes = if t + 1 == n { l.elements - t * tile_len as u64 } else { tile_len as u64 };
    if tile_bytes.len() as u64 != lanes * l.element_bytes as u64 {
        return false;
    }
    let mut h = output_tile_leaf_v1(t as u32, tile_bytes);
    let (mut i, mut width, mut used) = (t, n, 0usize);
    while width > 1 {
        if i ^ 1 < width {
            let Some(s) = proof.get(used) else { return false };
            used += 1;
            h = if i % 2 == 0 { node(&h, s) } else { node(s, &h) };
        }
        i /= 2;
        width = width.div_ceil(2);
    }
    used == proof.len() && root_from_merkle(spec, tile_len, &h) == *root
}

/// The key of [`output_set_id_v1`].
pub const OUTPUT_SET_ID_KEY_V1: &[u8] = b"misaka-palw/output-set-id/v1";

/// **The output-set descriptor**: every kind (tag, name, element encoding, shape rule, metadata) and
/// the digest's construction (its three keys, the root's preimage), as one ASCII line. The fence
/// `palw_gen_v1` carries its hash, so two builds whose canonical outputs differ in any way have
/// different consensus identities where the fence is armed.
pub fn output_set_descriptor_v1() -> String {
    let kinds = [
        "0:Tokens:u32le:[n]:-",
        "1:ImageRgb8:u8:[H,W,3]:-",
        "2:PcmI16:i16le:[frames,channels]:le32(sample_rate>=1)",
        "3:EmbeddingI32:i32le:[n,d]:[q<=31,normalised<=1]",
        "4:VideoRgb8:u8:[T,H,W,3]:le32(fps_num>=1)le32(fps_den>=1)",
        "5:TensorLe:i8|i16|i32le:rank1-4:[dtype<=2,q<=62]",
    ];
    format!(
        "palw-output/v1/kinds={}/tile_len=[{OUTPUT_MIN_TILE_LEN_V1},{OUTPUT_MAX_TILE_LEN_V1}]/leaf={}(le32(t)|bytes)/node={}(left|right)/odd=promoted/root={}(borsh(spec)|le32(tile_len)|merkle)",
        kinds.join(","),
        String::from_utf8_lossy(OUTPUT_TILE_KEY_V1),
        String::from_utf8_lossy(OUTPUT_NODE_KEY_V1),
        String::from_utf8_lossy(OUTPUT_ROOT_KEY_V1),
    )
}

/// `output_set_id = BLAKE2b-512(key = "misaka-palw/output-set-id/v1", output_set_descriptor_v1())`.
pub fn output_set_id_v1() -> [u8; 64] {
    keyed64(OUTPUT_SET_ID_KEY_V1, &[output_set_descriptor_v1().as_bytes()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_output_set_descriptor_names_every_kind_and_every_key() {
        let d = output_set_descriptor_v1();
        for k in OutputKindV1::ALL {
            assert!(d.contains(&format!("{}:{}:", k.tag(), k.name())), "{}", k.name());
        }
        for key in [OUTPUT_TILE_KEY_V1, OUTPUT_NODE_KEY_V1, OUTPUT_ROOT_KEY_V1] {
            assert!(d.contains(std::str::from_utf8(key).unwrap()));
        }
        assert!(d.is_ascii());
    }

    #[test]
    fn every_kind_round_trips_its_tag() {
        for k in OutputKindV1::ALL {
            assert_eq!(OutputKindV1::from_tag(k.tag()), Some(k));
        }
        assert_eq!(OutputKindV1::from_tag(6), None);
        assert_eq!(OutputKindV1::ImageRgb8.tag(), 1, "the ImageJob's output field");
    }

    #[test]
    fn an_odd_tile_is_promoted_not_duplicated() {
        let spec = OutputSpecV1::image_rgb8(3, 4); // 36 lanes
        let v: Vec<i64> = (0..36).map(|i| (i * 7 % 256) as i64).collect();
        let leaves = output_leaves_v1(&spec, &v, 12).unwrap();
        assert_eq!(leaves.len(), 3);
        let mut dup = leaves.clone();
        dup.push(leaves[2]);
        assert_ne!(output_merkle_root_v1(&leaves), output_merkle_root_v1(&dup));
    }

    #[test]
    fn every_tile_of_every_tree_shape_verifies_and_a_changed_byte_does_not() {
        for n_px in [1u32, 2, 3, 4, 5, 7, 8, 9, 16, 17, 33] {
            let spec = OutputSpecV1::image_rgb8(1, n_px);
            let v: Vec<i64> = (0..3 * n_px as i64).map(|i| (i * 31 + 5) % 256).collect();
            let tile_len = 4;
            let root = output_root_v1(&spec, &v, tile_len).unwrap();
            let tiles = output_tiles_v1(&spec, &v, tile_len).unwrap();
            let leaves = output_leaves_v1(&spec, &v, tile_len).unwrap();
            for (t, bytes) in tiles.iter().enumerate() {
                let proof = output_tile_proof_v1(&leaves, t).unwrap();
                assert!(verify_output_tile_v1(&root, &spec, tile_len, t as u64, bytes, &proof), "n_px {n_px} tile {t}");
                let mut bad = bytes.clone();
                bad[0] ^= 1;
                assert!(!verify_output_tile_v1(&root, &spec, tile_len, t as u64, &bad, &proof));
                assert!(!verify_output_tile_v1(&root, &spec, 8, t as u64, bytes, &proof), "the root names its tiling");
            }
        }
    }

    #[test]
    fn values_outside_the_domain_are_refused() {
        let img = OutputSpecV1::image_rgb8(1, 1);
        assert_eq!(img.canonical_bytes(&[0, 255, 256]), Err(OutputErrorV1::Domain { index: 2, value: 256 }));
        assert_eq!(img.canonical_bytes(&[0, -1, 0]), Err(OutputErrorV1::Domain { index: 1, value: -1 }));
        assert_eq!(img.canonical_bytes(&[0, 0]), Err(OutputErrorV1::Count { want: 3, got: 2 }));
        let pcm = OutputSpecV1::pcm_i16(2, 1, 48_000);
        assert_eq!(pcm.canonical_bytes(&[-32768, 32767]).unwrap(), vec![0x00, 0x80, 0xff, 0x7f]);
        assert!(pcm.canonical_bytes(&[32768, 0]).is_err());
        let tok = OutputSpecV1::tokens(1);
        assert_eq!(tok.canonical_bytes(&[4_294_967_295]).unwrap(), vec![0xff; 4]);
    }

    #[test]
    fn malformed_headers_are_refused() {
        assert!(matches!(OutputSpecV1 { kind: 9, shape: vec![1], meta: vec![] }.layout(), Err(OutputErrorV1::UnknownKind(9))));
        assert!(matches!(OutputSpecV1 { kind: 1, shape: vec![2, 2, 4], meta: vec![] }.layout(), Err(OutputErrorV1::Shape(_))));
        assert!(matches!(OutputSpecV1 { kind: 1, shape: vec![0, 2, 3], meta: vec![] }.layout(), Err(OutputErrorV1::Shape(_))));
        assert!(matches!(OutputSpecV1::pcm_i16(1, 1, 0).layout(), Err(OutputErrorV1::Meta(_))));
        assert!(matches!(OutputSpecV1::video_rgb8(1, 1, 1, 30, 0).layout(), Err(OutputErrorV1::Meta(_))));
        assert!(matches!(OutputSpecV1::embedding_i32(1, 4, 32, true).layout(), Err(OutputErrorV1::Meta(_))));
        assert!(matches!(OutputSpecV1::tensor_le(3, vec![4], 0).layout(), Err(OutputErrorV1::Meta(_))));
        assert!(matches!(output_root_v1(&OutputSpecV1::image_rgb8(1, 1), &[0, 0, 0], 3), Err(OutputErrorV1::TileLen(3))));
    }
}
