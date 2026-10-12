//! **Deterministic image preprocessing, in the gateway** (RFC-0003 §II.4; delivery task 5): `misaka.palw.image-preprocess.v1`.
//!
//! > Decoding, EXIF orientation, colour management, alpha and **arbitrary-size resampling** are outside consensus: the gateway resizes or
//! > letterboxes to a declared size and says so.
//!
//! A vision-language class declares its image slots (`{h, w, tile_len}`); a job carries exactly one image per slot, AT the slot's size, and
//! the chain commits `input_root` of those pixels. A person's picture is never that size. This module is the fixed integer function from
//! "a decoded RGB picture of any size" to "the picture the class reads", so that:
//!
//! * the same picture and the same declared fit are the same pixels on every machine (no floats, no library, no platform: `u8` pixels,
//!   `i64`/`u64` arithmetic, rounding half-up — golden vectors from an independent Python implementation, `tests/vectors/`);
//! * what was done is a RECORD ([`PreprocessRecordV1`]): the algorithm id, the fit, the source size and digest, where the content sits in
//!   the canonical image, and the digest of the canonical pixels — the digest the receipt binds, so "this picture, resized this way" is
//!   something a user can later prove and an operator cannot quietly change;
//! * the chain still sees only `input_root` (the canonical pixels). That the raw picture became those pixels by THIS algorithm is the gateway's
//!   statement, not a consensus rule — the record makes it checkable by anyone holding the raw picture (RFC-0003 puts it outside consensus,
//!   and this module does not pretend otherwise).
//!
//! # The algorithm
//!
//! * **`stretch`** — bilinear resampling to the slot's size, half-pixel centres, Q16 fixed point:
//!   `s = clamp(((2·o + 1)·src·2^16) div (2·dst) − 2^15, 0, (src − 1)·2^16)`, `i0 = s >> 16`, `f = s & 0xFFFF`, `i1 = min(i0 + 1, src − 1)`, and
//!   `out = (Σ p·(2^16 − fx or fx)·(2^16 − fy or fy) + 2^31) >> 32` over the four neighbours, per channel. Equal sizes are the identity.
//! * **`letterbox`** — scale to fit inside the slot keeping the aspect ratio (`new = round-half-up(src_other · dst_fit / src_fit)`, at least 1),
//!   resampled with the same bilinear, centred with the floor of half the slack, the rest the declared pad colour.
//!
//! Not an area filter: a large downscale aliases. A class that needs better says so in its own preprocessing stage (RFC-0003 §II.4's
//! integer TIR stage); this is the gateway's declared, simple, reproducible default.
#![allow(dead_code)] // called by `vlm`, which the binary does not route to yet

use kaspa_hashes::Hash64;

use crate::tensor::{DecodedImageV1, MAX_IMAGE_BYTES, MAX_IMAGE_SIDE};

pub const ALGORITHM_ID: &str = "misaka.palw.image-preprocess.v1";
const DOMAIN_PIXELS: &[u8] = b"misaka-palw/gateway/image-pixels/v1";
const DOMAIN_RECORD: &[u8] = b"misaka-palw/gateway/image-preprocess-record/v1";
const Q: i64 = 1 << 16;

/// How a picture of another size becomes the slot's size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    /// Resample to the slot's size, changing the aspect ratio if it differs.
    Stretch,
    /// Fit inside the slot keeping the aspect ratio; the rest is `pad`.
    Letterbox { pad: [u8; 3] },
}

impl Fit {
    fn name(self) -> &'static str {
        match self {
            Fit::Stretch => "stretch",
            Fit::Letterbox { .. } => "letterbox",
        }
    }
}

/// Where the (resampled) content sits in the canonical image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    pub y: u32,
    pub x: u32,
    pub h: u32,
    pub w: u32,
}

/// What was done to one picture, checkable by anyone holding the raw one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreprocessRecordV1 {
    pub fit: Fit,
    pub source_h: u32,
    pub source_w: u32,
    /// [`pixels_digest`] of the raw picture (not the picture: it stays the user's).
    pub source_digest: Hash64,
    pub target_h: u32,
    pub target_w: u32,
    pub placed: Placed,
    /// [`pixels_digest`] of the canonical pixels.
    pub output_digest: Hash64,
}

/// The digest of an RGB picture: its size and its bytes.
pub fn pixels_digest(h: u32, w: u32, rgb: &[u8]) -> Hash64 {
    let mut pre = Vec::with_capacity(8 + rgb.len());
    pre.extend_from_slice(&h.to_le_bytes());
    pre.extend_from_slice(&w.to_le_bytes());
    pre.extend_from_slice(rgb);
    kaspa_hashes::blake2b_512_keyed(DOMAIN_PIXELS, &pre)
}

impl PreprocessRecordV1 {
    /// The one digest a receipt binds for this picture.
    pub fn digest(&self) -> Hash64 {
        let mut pre = Vec::new();
        pre.extend_from_slice(&(ALGORITHM_ID.len() as u64).to_le_bytes());
        pre.extend_from_slice(ALGORITHM_ID.as_bytes());
        match self.fit {
            Fit::Stretch => pre.push(0),
            Fit::Letterbox { pad } => {
                pre.push(1);
                pre.extend_from_slice(&pad);
            }
        }
        for n in [self.source_h, self.source_w, self.target_h, self.target_w, self.placed.y, self.placed.x, self.placed.h, self.placed.w] {
            pre.extend_from_slice(&n.to_le_bytes());
        }
        pre.extend_from_slice(self.source_digest.as_byte_slice());
        pre.extend_from_slice(self.output_digest.as_byte_slice());
        kaspa_hashes::blake2b_512_keyed(DOMAIN_RECORD, &pre)
    }

    pub fn to_json(&self) -> serde_json::Value {
        let hex = |h: &Hash64| faster_hex::hex_string(h.as_byte_slice());
        serde_json::json!({
            "algorithm": ALGORITHM_ID,
            "fit": self.fit.name(),
            "pad": match self.fit { Fit::Letterbox { pad } => Some(pad), Fit::Stretch => None },
            "source": { "h": self.source_h, "w": self.source_w, "digest": hex(&self.source_digest) },
            "target": { "h": self.target_h, "w": self.target_w },
            "placed": { "y": self.placed.y, "x": self.placed.x, "h": self.placed.h, "w": self.placed.w },
            "output_digest": hex(&self.output_digest),
            "record_digest": hex(&self.digest()),
            "note": "the chain commits only the canonical pixels' input_root; that the raw picture became them by this algorithm is the gateway's statement (RFC-0003 §II.4), checkable by anyone holding the raw picture",
        })
    }
}

fn axis_table(src: u32, dst: u32) -> Vec<(usize, usize, i64)> {
    (0..dst)
        .map(|o| {
            let s = ((2 * i64::from(o) + 1) * i64::from(src) * Q).div_euclid(2 * i64::from(dst)) - Q / 2;
            let s = s.clamp(0, (i64::from(src) - 1) * Q);
            let i0 = (s >> 16) as usize;
            (i0, (i0 + 1).min(src as usize - 1), s & (Q - 1))
        })
        .collect()
}

/// Bilinear resampling, Q16, half-pixel centres (see the module doc). `src` is `sh·sw·3` bytes.
pub fn resize_bilinear_q16(src: &[u8], sh: u32, sw: u32, dh: u32, dw: u32) -> Vec<u8> {
    let (ys, xs) = (axis_table(sh, dh), axis_table(sw, dw));
    let mut out = vec![0u8; dh as usize * dw as usize * 3];
    for (oy, (y0, y1, fy)) in ys.iter().enumerate() {
        for (ox, (x0, x1, fx)) in xs.iter().enumerate() {
            for c in 0..3 {
                let p = |y: usize, x: usize| u64::from(src[(y * sw as usize + x) * 3 + c]);
                let (fx, fy) = (*fx as u64, *fy as u64);
                let (gx, gy) = (Q as u64 - fx, Q as u64 - fy);
                let total = p(*y0, *x0) * gx * gy + p(*y0, *x1) * fx * gy + p(*y1, *x0) * gx * fy + p(*y1, *x1) * fx * fy;
                out[(oy * dw as usize + ox) * 3 + c] = ((total + (1 << 31)) >> 32) as u8;
            }
        }
    }
    out
}

/// **Preprocess one picture to a slot's size.** Refuses a malformed picture (zero side, wrong byte count, over the entrance's caps) and a
/// slot that is empty or past the caps, by name.
pub fn preprocess(image: &DecodedImageV1, target_h: u32, target_w: u32, fit: Fit) -> Result<(DecodedImageV1, PreprocessRecordV1), String> {
    let (sh, sw) = (image.h, image.w);
    if sh == 0 || sw == 0 || sh > MAX_IMAGE_SIDE || sw > MAX_IMAGE_SIDE {
        return Err(format!("the picture is {sh}x{sw}: each side must be 1..={MAX_IMAGE_SIDE}"));
    }
    if image.rgb.len() != sh as usize * sw as usize * 3 {
        return Err(format!("the picture is {sh}x{sw} and carries {} bytes where {} were expected", image.rgb.len(), sh as usize * sw as usize * 3));
    }
    if target_h == 0 || target_w == 0 || target_h > MAX_IMAGE_SIDE || target_w > MAX_IMAGE_SIDE || target_h as usize * target_w as usize * 3 > MAX_IMAGE_BYTES {
        return Err(format!("the slot is {target_h}x{target_w}, outside what a slot may declare"));
    }
    let (canonical, placed) = match fit {
        Fit::Stretch => (resize_bilinear_q16(&image.rgb, sh, sw, target_h, target_w), Placed { y: 0, x: 0, h: target_h, w: target_w }),
        Fit::Letterbox { pad } => {
            let (sh64, sw64, th64, tw64) = (u64::from(sh), u64::from(sw), u64::from(target_h), u64::from(target_w));
            let (nh, nw) = if sw64 * th64 >= sh64 * tw64 {
                (((sh64 * tw64 * 2 + sw64) / (2 * sw64)).clamp(1, th64), tw64)
            } else {
                (th64, ((sw64 * th64 * 2 + sh64) / (2 * sh64)).clamp(1, tw64))
            };
            let (nh, nw) = (nh as u32, nw as u32);
            let (oy, ox) = ((target_h - nh) / 2, (target_w - nw) / 2);
            let inner = resize_bilinear_q16(&image.rgb, sh, sw, nh, nw);
            let mut out = Vec::with_capacity(target_h as usize * target_w as usize * 3);
            for _ in 0..target_h as usize * target_w as usize {
                out.extend_from_slice(&pad);
            }
            for y in 0..nh as usize {
                let dst = ((oy as usize + y) * target_w as usize + ox as usize) * 3;
                out[dst..dst + nw as usize * 3].copy_from_slice(&inner[y * nw as usize * 3..(y + 1) * nw as usize * 3]);
            }
            (out, Placed { y: oy, x: ox, h: nh, w: nw })
        }
    };
    let record = PreprocessRecordV1 {
        fit,
        source_h: sh,
        source_w: sw,
        source_digest: pixels_digest(sh, sw, &image.rgb),
        target_h,
        target_w,
        placed,
        output_digest: pixels_digest(target_h, target_w, &canonical),
    };
    Ok((DecodedImageV1 { h: target_h, w: target_w, rgb: canonical }, record))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Digest as _;

    fn unhex(s: &str) -> Vec<u8> {
        let mut out = vec![0u8; s.len() / 2];
        faster_hex::hex_decode(s.as_bytes(), &mut out).expect("hex");
        out
    }

    /// **The golden vectors** (`tests/vectors/image-preprocess-v1.json`), produced by an independent Python implementation
    /// (`gen_image_preprocess_vectors.py`): every case's canonical pixels (or, for the large ones, their SHA-256) and placement.
    #[test]
    fn the_golden_vectors_from_the_independent_implementation_hold() {
        let doc: serde_json::Value = serde_json::from_str(include_str!("../tests/vectors/image-preprocess-v1.json")).unwrap();
        assert_eq!(doc["algorithm"], ALGORITHM_ID);
        let cases = doc["cases"].as_array().unwrap();
        assert!(cases.len() >= 10);
        for c in cases {
            let name = c["name"].as_str().unwrap();
            let (sh, sw) = (c["src_h"].as_u64().unwrap() as u32, c["src_w"].as_u64().unwrap() as u32);
            let (dh, dw) = (c["dst_h"].as_u64().unwrap() as u32, c["dst_w"].as_u64().unwrap() as u32);
            let fit = match c["fit"].as_str().unwrap() {
                "stretch" => Fit::Stretch,
                "letterbox" => {
                    let p = c["pad"].as_array().unwrap();
                    Fit::Letterbox { pad: [p[0].as_u64().unwrap() as u8, p[1].as_u64().unwrap() as u8, p[2].as_u64().unwrap() as u8] }
                }
                other => panic!("{other}"),
            };
            let image = DecodedImageV1 { h: sh, w: sw, rgb: unhex(c["src_rgb_hex"].as_str().unwrap()) };
            let (out, record) = preprocess(&image, dh, dw, fit).unwrap_or_else(|e| panic!("{name}: {e}"));
            if let Some(expect) = c["out_rgb_hex"].as_str() {
                assert_eq!(faster_hex::hex_string(&out.rgb), expect, "{name}: the canonical pixels");
            }
            assert_eq!(faster_hex::hex_string(&sha2::Sha256::digest(&out.rgb)), c["out_sha256"].as_str().unwrap(), "{name}: sha256 of the pixels");
            let p = c["placed"].as_array().unwrap();
            let placed = [record.placed.y, record.placed.x, record.placed.h, record.placed.w];
            assert_eq!(placed.map(u64::from), [p[0].as_u64().unwrap(), p[1].as_u64().unwrap(), p[2].as_u64().unwrap(), p[3].as_u64().unwrap()], "{name}: placement");
            assert_eq!((out.h, out.w), (dh, dw));
        }
    }

    /// Three results derived BY HAND, so the vectors are not merely the code agreeing with itself.
    #[test]
    fn three_results_derived_by_hand() {
        let gray = |v: &[u8]| -> Vec<u8> { v.iter().flat_map(|x| [*x, *x, *x]).collect() };
        // [0, 255] widened to four columns, half-pixel centres: sources 0, 0.25, 0.75, 1.0 -> 0, 63.75+.5, 191.25+.5, 255 -> 0, 64, 191, 255.
        let (wide, _) = preprocess(&DecodedImageV1 { h: 1, w: 2, rgb: gray(&[0, 255]) }, 1, 4, Fit::Stretch).unwrap();
        assert_eq!(wide.rgb, gray(&[0, 64, 191, 255]));
        // Four pixels into one: the mean, 25.
        let (one, _) = preprocess(&DecodedImageV1 { h: 2, w: 2, rgb: gray(&[10, 20, 30, 40]) }, 1, 1, Fit::Stretch).unwrap();
        assert_eq!(one.rgb, gray(&[25]));
        // A 4x2 (tall) picture into 4x4: height fills (4), width = round(2*4/4) = 2, centred at column 1: columns 0 and 3 are the pad.
        let tall = DecodedImageV1 { h: 4, w: 2, rgb: (0..24).collect() };
        let (boxed, record) = preprocess(&tall, 4, 4, Fit::Letterbox { pad: [9, 9, 9] }).unwrap();
        assert_eq!((record.placed.y, record.placed.x, record.placed.h, record.placed.w), (0, 1, 4, 2));
        for y in 0..4usize {
            assert_eq!(&boxed.rgb[(y * 4) * 3..(y * 4) * 3 + 3], &[9, 9, 9], "left pad, row {y}");
            assert_eq!(&boxed.rgb[(y * 4 + 3) * 3..(y * 4 + 3) * 3 + 3], &[9, 9, 9], "right pad, row {y}");
            // The inner 4x2 -> 4x2 is the identity: the picture's own pixels.
            assert_eq!(&boxed.rgb[(y * 4 + 1) * 3..(y * 4 + 3) * 3], &tall.rgb[y * 2 * 3..(y + 1) * 2 * 3], "content, row {y}");
        }
    }

    #[test]
    fn the_same_size_is_the_identity_and_the_function_is_pure() {
        let image = DecodedImageV1 { h: 3, w: 2, rgb: (0..18).map(|i| (i * 13) as u8).collect() };
        for fit in [Fit::Stretch, Fit::Letterbox { pad: [1, 2, 3] }] {
            let (out, record) = preprocess(&image, 3, 2, fit).unwrap();
            assert_eq!(out, image, "{fit:?}: a picture already at the slot's size is untouched");
            assert_eq!(record.placed, Placed { y: 0, x: 0, h: 3, w: 2 });
            assert_eq!(record.source_digest, record.output_digest);
            assert_eq!(preprocess(&image, 3, 2, fit).unwrap().1, record, "pure");
        }
        // Output is always within the byte range and of the slot's size, for awkward ratios.
        for (sh, sw, th, tw) in [(1, 1, 4, 4), (7, 3, 2, 5), (1, 100, 3, 3), (100, 1, 3, 3), (64, 64, 1, 1)] {
            let src = DecodedImageV1 { h: sh, w: sw, rgb: (0..sh * sw * 3).map(|i| (i * 7 + 3) as u8).collect() };
            for fit in [Fit::Stretch, Fit::Letterbox { pad: [0, 0, 0] }] {
                let (out, rec) = preprocess(&src, th, tw, fit).unwrap();
                assert_eq!((out.h, out.w, out.rgb.len()), (th, tw, th as usize * tw as usize * 3));
                assert!(rec.placed.h >= 1 && rec.placed.w >= 1 && rec.placed.y + rec.placed.h <= th && rec.placed.x + rec.placed.w <= tw, "{rec:?}");
            }
        }
    }

    /// The record is what the receipt binds: a different picture, fit, pad, slot or algorithm outcome is a different record.
    #[test]
    fn the_record_changes_with_the_picture_the_fit_the_pad_and_the_slot() {
        let image = DecodedImageV1 { h: 5, w: 7, rgb: (0..105).map(|i| (i * 31) as u8).collect() };
        let base = preprocess(&image, 4, 4, Fit::Letterbox { pad: [0, 0, 0] }).unwrap();
        let mut other_pixels = image.clone();
        other_pixels.rgb[0] ^= 1;
        let digests = [
            ("one changed byte of the raw picture", preprocess(&other_pixels, 4, 4, Fit::Letterbox { pad: [0, 0, 0] }).unwrap().1.digest()),
            ("another pad colour", preprocess(&image, 4, 4, Fit::Letterbox { pad: [1, 0, 0] }).unwrap().1.digest()),
            ("stretch instead of letterbox", preprocess(&image, 4, 4, Fit::Stretch).unwrap().1.digest()),
            ("another slot size", preprocess(&image, 4, 5, Fit::Letterbox { pad: [0, 0, 0] }).unwrap().1.digest()),
        ];
        for (what, d) in digests {
            assert_ne!(d, base.1.digest(), "{what}");
        }
        // The canonical pixels differ when the fit does — so the V5 job's input_root (which commits them) differs too.
        assert_ne!(base.0.rgb, preprocess(&image, 4, 4, Fit::Stretch).unwrap().0.rgb);
        let json = base.1.to_json();
        assert_eq!(json["algorithm"], ALGORITHM_ID);
        assert_eq!(json["fit"], "letterbox");
    }

    #[test]
    fn a_malformed_picture_or_slot_is_refused_by_name() {
        let ok = DecodedImageV1 { h: 2, w: 2, rgb: vec![0; 12] };
        assert!(preprocess(&DecodedImageV1 { h: 0, w: 2, rgb: vec![] }, 2, 2, Fit::Stretch).is_err());
        assert!(preprocess(&DecodedImageV1 { h: 2, w: 2, rgb: vec![0; 11] }, 2, 2, Fit::Stretch).unwrap_err().contains("expected"));
        assert!(preprocess(&ok, 0, 2, Fit::Stretch).is_err());
        assert!(preprocess(&ok, 2, MAX_IMAGE_SIDE + 1, Fit::Stretch).is_err());
        assert!(preprocess(&DecodedImageV1 { h: MAX_IMAGE_SIDE + 1, w: 1, rgb: vec![] }, 2, 2, Fit::Stretch).is_err());
    }
}
